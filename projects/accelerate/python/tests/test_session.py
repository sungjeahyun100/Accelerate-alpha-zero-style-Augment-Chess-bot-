"""Public replay and one synthetic optimizer step with deterministic resume."""
from copy import deepcopy
from contextlib import nullcontext
from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace
import hashlib
from io import BytesIO
import json
import os
from pathlib import Path
import random
import signal
import sys
from tempfile import TemporaryDirectory
from threading import Barrier, Lock
from types import ModuleType, SimpleNamespace

import numpy as np
import pytest
import torch

from accelerate_chess import cli
from accelerate_chess import training as training_module
from accelerate_chess.encoding import PublicEncoder, batch_positions, canonical_json
from accelerate_chess.ir import ObservationIR, TypedEncoder, TypedEncoderSpec, batch_typed_positions
from accelerate_chess.network.entity_transformer import EntityTransformer, EntityTransformerConfig
from accelerate_chess.network.mask_resnet import MaskResNetConfig, MaskResNetPolicyValueNetwork
from accelerate_chess.network.typed_context import TypedContextConfig
from accelerate_chess.network.artifacts import export_onnx, export_typed_onnx, load_manifest, load_typed_base, save_base
from accelerate_chess.network.model import ModelConfig, PolicyValueNetwork, tensor_state_hash
from accelerate_chess.replay import MAX_REPLAY_BYTES, EpisodeRecorder, ReplayEpisode, atomic_json, read_json, reserve_slot, writer_claim
from accelerate_chess.search import PublicTracker, SearchBudgetError, SearchResult
from accelerate_chess.training import (DatasetCursor, ReplayDataset, TrainingLimits, _rng_snapshot,
    _tree_hash, create_optimizer, load_training_checkpoint, optimize, save_training_checkpoint)
from test_search import TestAction, TestPosition, spec
from test_ir import source as v7_source
from test_model_stack import observation_policy, resign

torch.set_num_threads(1)


@pytest.fixture
def session_directory():
    if os.environ.get("ACCELERATE_TEST_ARTIFACTS"):
        directory = Path(os.environ["ACCELERATE_TEST_ARTIFACTS"]) / "session"
    elif os.environ.get("RUNNER_TEMP"):
        directory = Path(os.environ["RUNNER_TEMP"]) / "Accelerate" / "tmp" / "session"
    elif os.environ.get("APPDATA"):
        directory = Path(os.environ["APPDATA"]) / "Accelerate" / "tmp" / "full-stack-implementation" / "session"
    else:
        directory = Path(os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache"))) / "accelerate" / "test" / "session"
    directory.mkdir(parents=True, exist_ok=True)
    # A fresh owned case directory makes immutable run IDs safe across repeated test runs.
    with TemporaryDirectory(prefix="case-", dir=directory) as temporary:
        yield Path(temporary)


def synthetic_episode(*, terminal=True):
    contract = spec()
    position = TestPosition(1, reaction=True)
    recorder = EpisodeRecorder({viewer: position.observe(viewer) for viewer in ("white", "black")}, contract,
                    environment_seed=37, belief_seed=71, evidence_kind="synthetic")
    for step in range(2 if terminal else 1):
        frame = position.observe("white")
        intent = TestAction(step, 1).public_intent()
        key = canonical_json(intent)
        decision = SearchResult(intent, key, ({"action_key": key, "intent": intent, "visits": 3, "availability": 3,
                   "probability": 1., "value": .2},), frame["informationStateKey"], 3, "iterations", True, False, 1, 1, contract.digest)
        recorder.record_decision("white", decision)
        if terminal and step == 1:
            position.chance = True
        child = position.apply(position.bind_public_intent(intent)).position
        recorder.advance({viewer: child.observe(viewer) for viewer in ("white", "black")}, actor="white", intent=intent)
        position = child
    recorder.finish("white" if terminal else None, "synthetic-source-terminal" if terminal else "verification-limit")
    return recorder


def synthetic_typed_contract():
    _, policy = v7_source()
    catalog = {"schemaVersion": 1, "rulesVersion": policy["rulesVersion"],
               "catalogVersion": "synthetic-typed-v1", "pieceTypes": ["pawn", "wall"],
               "cards": [{"id": "slime", "draftCategory": "MIDDLE"}], "actionTypes": ["move"]}
    return TypedEncoderSpec.from_catalog(catalog, observation_policy=policy), catalog, policy


def test_typed_replay_requires_source_bound_ir_and_model_io_contract(session_directory, monkeypatch):
    contract, catalog, policy = synthetic_typed_contract()
    position = TestPosition()
    recorder = EpisodeRecorder({viewer: position.observe(viewer) for viewer in ("white", "black")},
        contract, environment_seed=37, belief_seed=71, evidence_kind="synthetic",
        architecture_family="mask-resnet")
    intent = TestAction(0, 0).public_intent()
    key = canonical_json(intent)
    decision = SearchResult(intent, key, ({"action_key": key, "intent": intent, "visits": 2,
        "availability": 2, "probability": 1., "value": .25},), position.observe("white")["informationStateKey"],
        2, "iterations", True, False, 1, 1, contract.digest)
    recorder.record_decision("white", decision)
    child = position.apply(position.bind_public_intent(intent)).position
    recorder.advance({viewer: child.observe(viewer) for viewer in ("white", "black")},
                     actor="white", intent=intent)
    recorder.finish(None, "finite-verification-limit")
    path = session_directory / "typed-episode.json"
    recorder.save(path)
    episode = ReplayEpisode.load(path, contract)
    assert episode.architecture_family == "mask-resnet"
    assert episode.outcome == {"status": "unfinished", "winner": None, "reason": "finite-verification-limit"}
    assert len(episode.decisions) == 1 and episode.decisions[0]["transition_completed"]
    assert list(episode.examples()) == []
    with pytest.raises(ValueError, match="no terminal policy/value targets"):
        ReplayDataset([path], contract)
    assert (episode.spec.ir_version, episode.spec.descriptor_version, episode.spec.encoder_version) == (
        "semantic-ir-v1", "move-program-v1", "typed-input-v1")
    assert episode.spec.digest == recorder.snapshot()["metadata"]["encoder_hash"]
    assert recorder.snapshot()["metadata"]["adapter_hash"] is None
    with pytest.raises(ValueError, match="exact source-bound encoder spec"):
        ReplayEpisode.load(path)
    changed_catalog = {**catalog, "catalogVersion": "synthetic-typed-v2"}
    different = TypedEncoderSpec.from_catalog(changed_catalog, observation_policy=policy)
    with pytest.raises(ValueError, match="exact source-bound encoder spec"):
        ReplayEpisode.load(path, different)
    altered = deepcopy(recorder.snapshot())
    altered["metadata"]["model_io_version"] = "legacy-float32-v2"
    altered["replay_hash"] = hashlib.sha256(canonical_json({key: value for key, value in altered.items()
                                                               if key != "replay_hash"}).encode()).hexdigest()
    with pytest.raises(ValueError, match="model/IR provenance"):
        ReplayEpisode(altered, contract)
    altered = deepcopy(recorder.snapshot())
    altered["metadata"]["adapter_hash"] = "b" * 64
    altered["replay_hash"] = hashlib.sha256(canonical_json({key: value for key, value in altered.items()
                                                               if key != "replay_hash"}).encode()).hexdigest()
    with pytest.raises(ValueError, match="adapter hash and descriptor"):
        ReplayEpisode(altered, contract)
    mixed_family = deepcopy(recorder.snapshot())
    mixed_family["metadata"]["architecture_family"] = "entity-transformer"
    mixed_family["replay_hash"] = hashlib.sha256(canonical_json({key: value for key, value in mixed_family.items()
                                                                    if key != "replay_hash"}).encode()).hexdigest()
    mixed_path = session_directory / "other-family-episode.json"
    atomic_json(mixed_path, mixed_family)
    with pytest.raises(ValueError, match="architecture differs"):
        ReplayDataset([path, mixed_path], contract)

    class TypedEvaluator:
        def __init__(self, manifest, expected_spec, backend, *, threads):
            assert expected_spec.digest == contract.digest
            self.architecture_family = "mask-resnet"
            self.session = self
            self.model_sha256 = "a" * 64

        def evaluate_typed(self, inputs):
            assert tuple(inputs) == tuple(contract.feature_schema["input_order"]["mask-resnet"])
            return (np.zeros(inputs["candidate_mask"].shape, np.float32),
                    np.zeros((len(inputs["candidate_mask"]), 1), np.float32))

    monkeypatch.setattr(cli, "ProductionEvaluator", TypedEvaluator)
    args = cli.parser().parse_args(["--model-family", "mask-resnet", "evaluate",
        "--manifest", str(session_directory / "unused-v3-manifest.json"), "--replay", str(path),
        "--run-id", "typed-evaluation"])
    report = cli.evaluate(args, session_directory, contract, lambda: False)
    assert report["version"] == "accelerate-evaluation-v2"
    assert report["architecture_family"] == "mask-resnet"
    assert report["samples"] == 1 and "value_mse" not in report["metrics"][0]
    sample = cli._typed_sample_inputs(path, contract, "mask-resnet")
    assert tuple(sample) == tuple(contract.feature_schema["input_order"]["mask-resnet"])
    assert sample["candidate_mask"].shape == (1, 1) and bool(sample["candidate_mask"][0, 0])
    public_sample = session_directory / "typed-public-sample.json"
    atomic_json(public_sample, {"version": "typed-inference-sample-v1",
        "architecture_family": "mask-resnet", "encoder_hash": contract.digest,
        "observation": position.observe("white"), "intents": [intent], "belief_summary": None})
    bootstrapped = cli._typed_sample_inputs(None, contract, "mask-resnet", sample_public=public_sample)
    assert all(np.array_equal(bootstrapped[name], sample[name]) for name in sample)
    mismatched_sample = read_json(public_sample)
    mismatched_sample["encoder_hash"] = "b" * 64
    atomic_json(public_sample, mismatched_sample)
    with pytest.raises(ValueError, match="source/model contract"):
        cli._typed_sample_inputs(None, contract, "mask-resnet", sample_public=public_sample)
    with pytest.raises(ValueError, match="family"):
        cli._typed_sample_inputs(path, contract, "entity-transformer")
    manifest_path = session_directory / "source-bound-v3.json"
    manifest_path.write_text("{}\n", encoding="utf-8")
    monkeypatch.setattr(cli, "load_manifest", lambda manifest, expected: {
        "version": "onnx-policy-value-v3", "model_sha256": "a" * 64,
        "architecture_family": "mask-resnet", "model_io_version": "typed-policy-value-v1"})
    activate_args = cli.parser().parse_args(["--model-family", "mask-resnet", "activate",
        "--manifest", str(manifest_path), "--sample-replay", str(path), "--expected-sha256", "a" * 64])
    activation = cli.activate(activate_args, session_directory, contract)
    assert activation["version"] == "accelerate-activation-v2"
    assert read_json(activation["activation"])["architecture_family"] == "mask-resnet"
    active_args = cli.parser().parse_args(["--model-family", "mask-resnet", "choose", "--trace", "unused"])
    assert cli._manifest(active_args, session_directory, contract) == manifest_path.resolve()


@pytest.mark.parametrize("family", ["mask-resnet", "entity-transformer"])
def test_typed_cli_initializes_source_bound_base_without_deploying(session_directory, family):
    source = Path(__file__).resolve().parents[3] / "augment-chess" / "contracts" / "catalog"
    catalog = read_json(source / "site-20260928.json")
    policy = read_json(source / "observation-20260928.json")
    contract = TypedEncoderSpec.from_catalog(catalog, observation_policy=policy)
    args = cli.parser().parse_args(["--model-family", family, "init", "--slot", family,
        "--channels", "16", "--blocks", "1", "--rank", "2"])
    result = cli.initialize(args, session_directory, contract)
    model, loaded_spec = load_typed_base(result["base"], contract)
    assert loaded_spec.digest == contract.digest and result["base_hash"] == model.base_hash
    assert result["architecture_family"] == family and result["manifest"] is None
    assert not (session_directory / "models" / family / "deployment").exists()
    with pytest.raises(FileExistsError, match="explicit --overwrite"):
        cli.initialize(args, session_directory, contract)


def test_mask_resnet_cli_warm_start_copies_only_legacy_residual_weights(session_directory):
    source = Path(__file__).resolve().parents[3] / "augment-chess" / "contracts" / "catalog"
    contract = TypedEncoderSpec.from_catalog(read_json(source / "site-20260928.json"),
        observation_policy=read_json(source / "observation-20260928.json"))
    legacy_contract = spec()
    legacy = PolicyValueNetwork(ModelConfig(legacy_contract.board_channels,
        legacy_contract.condition_dim, legacy_contract.action_dim, channels=8,
        residual_blocks=1, lora_rank=2, lora_alpha=2.))
    legacy_path = session_directory / "legacy-base.pt"
    save_base(legacy, legacy_contract, legacy_path)
    args = cli.parser().parse_args(["--model-family", "mask-resnet", "init", "--slot", "warm",
        "--channels", "8", "--blocks", "1", "--rank", "2", "--warm-start-legacy-base", str(legacy_path)])
    result = cli.initialize(args, session_directory, contract)
    initialized, _ = load_typed_base(result["base"], contract)
    assert len(result["warm_start"]["transferred_tensors"]) == 2
    torch.testing.assert_close(initialized.spatial.blocks[0].conv1.base.weight,
                               legacy.blocks[0].conv1.base.weight, rtol=0, atol=0)
    torch.testing.assert_close(initialized.spatial.blocks[0].conv2.base.weight,
                               legacy.blocks[0].conv2.base.weight, rtol=0, atol=0)


@pytest.mark.parametrize("family,mode", [("mask-resnet", "base"), ("entity-transformer", "adapter")])
def test_typed_synthetic_optimizer_checkpoint_resume(session_directory, family, mode):
    contract, _, _ = synthetic_typed_contract()
    position = TestPosition(1, reaction=True)
    recorder = EpisodeRecorder({viewer: position.observe(viewer) for viewer in ("white", "black")},
        contract, environment_seed=37, belief_seed=71, evidence_kind="synthetic",
        architecture_family=family)
    for step in range(2):
        frame = position.observe("white")
        intent = TestAction(step, 1).public_intent()
        key = canonical_json(intent)
        recorder.record_decision("white", SearchResult(intent, key, ({"action_key": key, "intent": intent,
            "visits": 2, "availability": 2, "probability": 1., "value": .25},),
            frame["informationStateKey"], 2, "iterations", True, False, 1, 1, contract.digest))
        if step == 1:
            position.chance = True
        child = position.apply(position.bind_public_intent(intent)).position
        recorder.advance({viewer: child.observe(viewer) for viewer in ("white", "black")},
                         actor="white", intent=intent)
        position = child
    recorder.finish("white", "synthetic-source-terminal")
    path = session_directory / f"{family}-episode.json"
    recorder.save(path)
    dataset = ReplayDataset([path], contract)
    assert len(dataset) == 2 and dataset[0].value == 1.
    context = TypedContextConfig((len(contract.category_vocabulary),) * 4,
                                 (len(contract.category_vocabulary),) * 2,
                                 (len(contract.category_vocabulary),) * 4, hidden_dim=16)
    if family == "mask-resnet":
        model = MaskResNetPolicyValueNetwork(MaskResNetConfig(6, context, channels=8,
            residual_blocks=1, lora_rank=2, lora_alpha=2.))
    else:
        model = EntityTransformer(EntityTransformerConfig(context, blocks=1, heads=2,
            ffn_dim=32, lora_rank=2, lora_alpha=2.))
    prior = deepcopy(model)
    optimizer = create_optimizer(model, mode=mode)
    cursor = DatasetCursor(dataset, seed=19)
    report = optimize(model, optimizer, TypedEncoder(contract), cursor,
                      limits=TrainingLimits(steps=1, batch_size=1, elapsed_ms=30_000))
    assert report["steps"] == 1 and report["stop_reason"] == "steps"
    checkpoint = session_directory / f"{family}-resume.pt"
    saved_hash = save_training_checkpoint(model, optimizer, contract, cursor, checkpoint,
                                          completed_steps=1)
    resumed = prior
    resumed_optimizer = create_optimizer(resumed, mode=mode)
    resumed_cursor = DatasetCursor(dataset, seed=19)
    assert load_training_checkpoint(resumed, resumed_optimizer, contract, resumed_cursor,
                                    checkpoint) == 1
    saved = torch.load(checkpoint, weights_only=True)
    assert saved["dtype"] == "fp32"
    assert saved["version"] == training_module.TYPED_DTYPE_TRAINING_VERSION
    saved.pop("dtype")
    saved["version"] = training_module.TYPED_TRAINING_VERSION
    saved["checkpoint_hash"] = _tree_hash({key: value for key, value in saved.items() if key != "checkpoint_hash"})
    old_checkpoint = session_directory / f"{family}-old-resume.pt"
    torch.save(saved, old_checkpoint)
    assert load_training_checkpoint(resumed, resumed_optimizer, contract, resumed_cursor,
                                    old_checkpoint) == 1
    assert saved_hash and resumed_cursor.snapshot() == cursor.snapshot()
    for name, value in model.state_dict().items():
        torch.testing.assert_close(resumed.state_dict()[name], value, rtol=0, atol=0)


def test_atomic_public_json_saves_do_not_share_a_temporary_file(session_directory, monkeypatch):
    path = session_directory / "concurrent-public.json"
    barrier = Barrier(2)
    replace = os.replace
    replace_lock = Lock()
    temporary_sources = []

    def simultaneous_replace(source, target):
        with replace_lock:
            temporary_sources.append(Path(source))
        barrier.wait(timeout=10)
        with replace_lock:
            replace(source, target)

    monkeypatch.setattr("accelerate_chess.replay.os.replace", simultaneous_replace)
    with ThreadPoolExecutor(max_workers=2) as workers:
        futures = [workers.submit(atomic_json, path, {"writer": writer}) for writer in (0, 1)]
        for future in futures:
            future.result(timeout=15)

    assert len(temporary_sources) == 2 and temporary_sources[0] != temporary_sources[1]
    assert read_json(path) in ({"writer": 0}, {"writer": 1})
    assert not list(session_directory.glob(".concurrent-public.json.*.tmp"))

    # The on-disk size is small, but the opened input can grow after stat.
    original_open = Path.open
    oversized = b"{}" + b" " * (MAX_REPLAY_BYTES - 1)
    with monkeypatch.context() as grown:
        grown.setattr(Path, "open", lambda candidate, *args, **kwargs:
                      BytesIO(oversized) if candidate == path else original_open(candidate, *args, **kwargs))
        with pytest.raises(ValueError, match="storage boundary"):
            read_json(path)


def test_public_replay_terminal_labels_and_streamed_dataset(session_directory, monkeypatch):
    completed = synthetic_episode()
    completed.save(session_directory / "episode.json")
    unfinished = synthetic_episode(terminal=False)
    unfinished.save(session_directory / "unfinished.json")
    episode = ReplayEpisode.load(session_directory / "episode.json", spec())
    examples = list(episode.examples())
    assert len(examples) == 2 and all(example.value == 1. for example in examples)
    terminal_outcome = deepcopy(completed.outcome)
    with pytest.raises(ValueError, match="unfinished replay cannot hide a terminal public result"):
        completed.finish(None, "cancelled-after-terminal")
    assert completed.outcome == terminal_outcome
    relabelled = deepcopy(completed.snapshot())
    relabelled["outcome"] = {"status": "unfinished", "winner": None, "reason": "elapsed"}
    relabelled["replay_hash"] = hashlib.sha256(canonical_json({key: value for key, value in relabelled.items()
                                                               if key != "replay_hash"}).encode()).hexdigest()
    relabelled_path = session_directory / "terminal-disguised-as-unfinished.json"
    atomic_json(relabelled_path, relabelled)
    with pytest.raises(ValueError, match="unfinished replay cannot hide a terminal public result"):
        ReplayDataset([relabelled_path], spec())
    assert examples[1].observation["turn"] == "black" and examples[1].actor == "white"
    assert completed.snapshot()["traces"]["black"]["steps"][0]["ownIntent"] is None
    for missing_or_wrong in (None, TestAction(1, 1).public_intent()):
        altered = deepcopy(completed.snapshot())
        altered["traces"]["white"]["steps"][0]["ownIntent"] = missing_or_wrong
        altered["replay_hash"] = hashlib.sha256(canonical_json({key: value for key, value in altered.items() if key != "replay_hash"}).encode()).hexdigest()
        with pytest.raises(ValueError, match="selected intent"):
            ReplayEpisode(altered, spec())
    assert list(ReplayEpisode.load(session_directory / "unfinished.json", spec()).examples()) == []
    dataset = ReplayDataset([session_directory / "episode.json", session_directory / "unfinished.json"], spec())
    assert len(dataset) == 2 and dataset[1].value == 1.
    assert dataset._cached_episode is not None and dataset._cached_file == 0
    content = completed.snapshot()
    content["outcome"]["winner"] = "draw"
    with pytest.raises(ValueError, match="hash"):
        ReplayEpisode(content, spec())
    incompatible_policy = completed.snapshot()
    incompatible_policy["observation_policy"]["projectionVersion"] = "other-projection"
    incompatible_policy["replay_hash"] = hashlib.sha256(canonical_json({key: value for key, value in incompatible_policy.items() if key != "replay_hash"}).encode()).hexdigest()
    with pytest.raises(ValueError, match="policy"):
        ReplayEpisode(incompatible_policy, spec())
    with pytest.raises(ValueError, match="no terminal"):
        ReplayDataset([session_directory / "unfinished.json"], spec())
    pending = synthetic_episode(terminal=False)
    intent = TestAction(1, 1).public_intent()
    frame = pending.trackers["white"].latest
    decision = SearchResult(intent, canonical_json(intent), ({"action_key": canonical_json(intent), "intent": intent,
        "visits": 1, "availability": 1, "probability": 1., "value": .2},), frame["informationStateKey"], 1, "iterations", True, False, 1, 1, spec().digest)
    pending.record_decision("white", decision)
    pending.finish(None, "cancelled-before-apply")
    ReplayEpisode(pending.snapshot(), spec())
    assert pending.snapshot()["decisions"][-1]["transition_completed"] is False
    initial = {viewer: TestPosition(1).observe(viewer) for viewer in ("white", "black")}
    # A decision-free, unfinished replay still has a complete policy boundary.
    # Public validation retains owned JSON and allocates no neural features.
    with monkeypatch.context() as public_only:
        public_only.setattr("accelerate_chess.encoding.np.zeros", lambda *args, **kwargs: (_ for _ in ()).throw(AssertionError("public validation allocated features")))
        empty = EpisodeRecorder(initial, spec(), environment_seed=37, belief_seed=71, evidence_kind="synthetic")
        empty.finish(None, "no-decisions")
        assert list(ReplayEpisode(empty.snapshot(), spec()).examples()) == []
        owned = PublicEncoder(spec()).validate_observation(initial["white"])
    initial["white"]["publicState"]["observationPolicyHash"] = "0" * 64
    assert owned.public["publicState"]["observationPolicyHash"] == spec().observation_policy_hash
    assert empty.trackers["white"].initial["publicState"]["observationPolicyHash"] == spec().observation_policy_hash
    for modify in (
        lambda frame: frame["publicState"].update(projectionVersion="source-visible-20260927-v2"),
        lambda frame: frame["publicState"].update(observationPolicyHash="0" * 64),
        lambda frame: frame["publicState"].pop("deathmatchStatus"),
    ):
        malformed = deepcopy(empty.snapshot())
        frame = malformed["traces"]["white"]["initial"]
        modify(frame); resign(frame)
        malformed["replay_hash"] = hashlib.sha256(canonical_json({key: value for key, value in malformed.items() if key != "replay_hash"}).encode()).hexdigest()
        with pytest.raises(ValueError, match="policy|projection|deathmatch"):
            EpisodeRecorder({viewer: trace["initial"] for viewer, trace in malformed["traces"].items()}, spec(), environment_seed=37, belief_seed=71, evidence_kind="synthetic")
        with pytest.raises(ValueError, match="policy|projection|deathmatch"):
            ReplayEpisode(malformed, spec())
    malformed = deepcopy(completed.snapshot())
    malformed["decisions"] = []
    malformed["outcome"] = {"status": "unfinished", "winner": None, "reason": "invalid-intermediate-frame"}
    frame = completed.trackers["white"].frame_at(1)
    frame["publicState"]["observationPolicyHash"] = "0" * 64
    resign(frame)
    malformed["traces"]["white"]["steps"][0]["patch"].extend([
        {"path": ["publicState", "observationPolicyHash"], "value": "0" * 64},
        {"path": ["informationStateKey"], "value": frame["informationStateKey"]},
    ])
    malformed["traces"]["white"]["steps"][1]["patch"].extend([
        {"path": ["publicState", "observationPolicyHash"], "value": spec().observation_policy_hash},
        {"path": ["informationStateKey"], "value": completed.trackers["white"].latest["informationStateKey"]},
    ])
    malformed["replay_hash"] = hashlib.sha256(canonical_json({key: value for key, value in malformed.items() if key != "replay_hash"}).encode()).hexdigest()
    with pytest.raises(ValueError, match="policy"):
        ReplayEpisode(malformed, spec())
    external = deepcopy(empty.snapshot())
    loaded = ReplayEpisode(external, spec())
    external["outcome"]["winner"] = "black"
    assert loaded.outcome["winner"] is None and list(loaded.examples()) == []
    oversized = session_directory / "oversized-replay.json"
    with oversized.open("wb") as output:
        output.truncate(MAX_REPLAY_BYTES + 1)
    with pytest.raises(ValueError, match="16 MiB storage boundary"):
        ReplayDataset([oversized], spec())


def test_pending_public_decision_rejects_conflicting_transition_without_mutation():
    contract = spec()
    position = TestPosition(1, reaction=True)
    recorder = EpisodeRecorder({viewer: position.observe(viewer) for viewer in ("white", "black")},
        contract, environment_seed=37, belief_seed=71, evidence_kind="synthetic")
    intent = TestAction(0, 1).public_intent()
    key = canonical_json(intent)
    result = SearchResult(intent, key, ({"action_key": key, "intent": intent, "visits": 1,
        "availability": 1, "probability": 1., "value": .2},),
        position.observe("white")["informationStateKey"], 1, "iterations", True, False,
        1, 1, contract.digest)
    recorder.record_decision("white", result)
    before = recorder.snapshot()
    with pytest.raises(ValueError, match="already pending"):
        recorder.record_decision("white", result)
    child = position.apply(position.bind_public_intent(intent)).position
    observations = {viewer: child.observe(viewer) for viewer in ("white", "black")}
    with pytest.raises(ValueError, match="pending actor or selected intent"):
        recorder.advance(observations, actor="black", intent=intent)
    with pytest.raises(ValueError, match="pending actor or selected intent"):
        recorder.advance(observations, actor="white", intent=TestAction(1, 1).public_intent())
    assert recorder.snapshot() == before
    recorder.advance(observations, actor="white", intent=intent)
    assert recorder.decisions[0]["transition_completed"]
    ReplayEpisode(recorder.snapshot(), contract)


def test_synthetic_optimizer_and_rng_cursor_resume_preserve_failure_state(session_directory):
    synthetic_episode().save(session_directory / "episode.json")
    dataset = ReplayDataset([session_directory / "episode.json"], spec())
    torch.manual_seed(12)
    model = PolicyValueNetwork(ModelConfig(spec().board_channels, spec().condition_dim, spec().action_dim, channels=4, residual_blocks=1, lora_rank=2, lora_alpha=2.))
    optimizer = create_optimizer(model, mode="base")
    cursor = DatasetCursor(dataset, 19)
    report = optimize(model, optimizer, PublicEncoder(spec()), cursor, limits=TrainingLimits(steps=1, batch_size=1))
    assert report["steps"] == 1 and np.isfinite(report["metrics"][0]["value_mse"])
    checkpoint = session_directory / "training.pt"
    save_training_checkpoint(model, optimizer, spec(), cursor, checkpoint, completed_steps=1)
    saved = torch.load(checkpoint, weights_only=True)
    assert (saved["version"], saved["dtype"]) == (training_module.DTYPE_TRAINING_VERSION, "fp32")
    expected_rng = (random.random(), np.random.random(), torch.rand(3))
    expected_examples = cursor.next_batch(3)
    expected_indices = [example.observation["informationStateKey"] for example in expected_examples]
    reference_state = _tree_hash(optimizer.state_dict())
    random.seed(100)
    np.random.seed(100)
    torch.manual_seed(100)
    restored = PolicyValueNetwork(model.config)
    restored_optimizer = create_optimizer(restored, mode="base")
    restored_cursor = DatasetCursor(dataset, 99)
    assert load_training_checkpoint(restored, restored_optimizer, spec(), restored_cursor, checkpoint) == 1
    assert tensor_state_hash(restored.state_dict()) == tensor_state_hash(model.state_dict())
    assert _tree_hash(restored_optimizer.state_dict()) == reference_state
    assert (random.random(), np.random.random()) == expected_rng[:2]
    torch.testing.assert_close(torch.rand(3), expected_rng[2], rtol=0, atol=0)
    restored_examples = restored_cursor.next_batch(3)
    assert [example.observation["informationStateKey"] for example in restored_examples] == expected_indices
    # A next-step forward/gradient comparison uses no additional optimizer step.
    encoder = PublicEncoder(spec())
    encoded = batch_positions([encoder.encode(example.observation, example.intents) for example in expected_examples])
    tensors = tuple(torch.from_numpy(array) for array in (encoded.board, encoded.condition, encoded.action_features))
    for candidate in (model, restored):
        candidate.zero_grad(set_to_none=True)
        logits, values = candidate(*tensors)
        values.square().mean().backward()
    for (name, parameter), (_, other) in zip(model.named_parameters(), restored.named_parameters()):
        if parameter.grad is not None:
            torch.testing.assert_close(parameter.grad, other.grad, rtol=0, atol=0)
    before = (tensor_state_hash(restored.state_dict()), _tree_hash(restored_optimizer.state_dict()), restored_cursor.snapshot(), _tree_hash(_rng_snapshot()))
    invalid = torch.load(checkpoint, weights_only=True)
    first = next(iter(invalid["optimizer"]["state"].values()))
    first["exp_avg"] = first["exp_avg"].reshape(-1)[:1]
    invalid["checkpoint_hash"] = _tree_hash({key: value for key, value in invalid.items() if key != "checkpoint_hash"})
    bad_path = session_directory / "invalid-training.pt"
    torch.save(invalid, bad_path)
    with pytest.raises(ValueError, match="optimizer tensor shape"):
        load_training_checkpoint(restored, restored_optimizer, spec(), restored_cursor, bad_path)
    assert before == (tensor_state_hash(restored.state_dict()), _tree_hash(restored_optimizer.state_dict()), restored_cursor.snapshot(), _tree_hash(_rng_snapshot()))
    huge_view = torch.as_strided(torch.zeros(1), (512 * 1024 * 1024 // 4 + 1,), (0,))
    with pytest.raises(ValueError, match="aggregate allocation budget"):
        _tree_hash(huge_view)
    # Its storage is one float; this remains a small metadata rejection input.
    first["exp_avg"] = huge_view
    torch.save(invalid, bad_path)
    with pytest.raises(ValueError, match="optimizer tensor shape"):
        load_training_checkpoint(restored, restored_optimizer, spec(), restored_cursor, bad_path)
    assert before == (tensor_state_hash(restored.state_dict()), _tree_hash(restored_optimizer.state_dict()), restored_cursor.snapshot(), _tree_hash(_rng_snapshot()))
    with pytest.raises(ValueError, match="compatibility"):
        wrong_spec = replace(spec(), rules_version="wrong-site")
        load_training_checkpoint(restored, restored_optimizer, wrong_spec, restored_cursor, checkpoint)


def test_training_dtype_validation_and_fp32_forward(monkeypatch, tmp_path):
    command = ["--model-family", "legacy-resnet", "train", "--base", "base.pt", "--replay", "episode.json"]
    assert cli.parser().parse_args(command).dtype == "fp32"
    assert cli.parser().parse_args(command + ["--dtype", "fp32"]).dtype == "fp32"
    assert cli.parser().parse_args(command + ["--dtype", "bf16"]).dtype == "bf16"
    with pytest.raises(ValueError, match="requires CUDA"):
        cli.train(cli.parser().parse_args(command + ["--dtype", "bf16"]), tmp_path, spec(), lambda: False)
    training_module.validate_training_dtype("fp32", "cpu")
    with pytest.raises(ValueError, match="requires CUDA"):
        training_module.validate_training_dtype("bf16", "cpu")
    with pytest.raises(ValueError, match="unsupported training dtype"):
        training_module.validate_training_dtype("fp16", "cuda")
    monkeypatch.setattr(torch.cuda, "is_available", lambda: False)
    with pytest.raises(RuntimeError, match="unavailable"):
        training_module.validate_training_dtype("bf16", "cuda")
    monkeypatch.setattr(torch.cuda, "is_available", lambda: True)
    monkeypatch.setattr(torch.cuda, "is_bf16_supported", lambda: False)
    with pytest.raises(RuntimeError, match="unsupported"):
        training_module.validate_training_dtype("bf16", "cuda")
    monkeypatch.setattr(torch.cuda, "is_bf16_supported", lambda: True)
    training_module.validate_training_dtype("bf16", "cuda")

    calls = []
    def autocast(*, device_type, dtype):
        calls.append((device_type, dtype))
        return nullcontext()
    monkeypatch.setattr(torch, "autocast", autocast)
    class Forward:
        def __call__(self, tensor):
            return tensor.to(torch.bfloat16), tensor.to(torch.bfloat16)
    source = torch.tensor([[1., 2.]], requires_grad=True)
    logits, value = training_module.training_forward(Forward(), (source,), dtype="bf16")
    assert calls == [("cuda", torch.bfloat16)]
    assert logits.dtype == value.dtype == torch.float32
    loss = -torch.log_softmax(logits, 1).mean() + torch.nn.functional.mse_loss(value, torch.zeros_like(value))
    assert loss.dtype == torch.float32
    loss.backward()
    assert source.grad is not None
    calls.clear()
    training_module.training_forward(Forward(), (source,), dtype="fp32")
    assert calls == []


def test_training_checkpoint_dtype_schema_and_resume_mismatch(session_directory, monkeypatch):
    synthetic_episode().save(session_directory / "episode.json")
    dataset = ReplayDataset([session_directory / "episode.json"], spec())
    model = PolicyValueNetwork(ModelConfig(spec().board_channels, spec().condition_dim,
        spec().action_dim, channels=4, residual_blocks=1, lora_rank=2, lora_alpha=2.))
    optimizer = create_optimizer(model, mode="base")
    cursor = DatasetCursor(dataset, 19)
    fp32 = session_directory / "fp32.pt"
    save_training_checkpoint(model, optimizer, spec(), cursor, fp32, completed_steps=0)
    saved = torch.load(fp32, weights_only=True)
    assert saved["dtype"] == "fp32"
    prior = (tensor_state_hash(model.state_dict()), cursor.snapshot())
    monkeypatch.setattr(training_module, "validate_training_dtype", lambda dtype, device: None)
    with pytest.raises(ValueError, match="dtype mismatch"):
        load_training_checkpoint(model, optimizer, spec(), cursor, fp32, dtype="bf16")
    bf16 = session_directory / "bf16.pt"
    save_training_checkpoint(model, optimizer, spec(), cursor, bf16, completed_steps=0, dtype="bf16")
    assert torch.load(bf16, weights_only=True)["dtype"] == "bf16"
    with pytest.raises(ValueError, match="dtype mismatch"):
        load_training_checkpoint(model, optimizer, spec(), cursor, bf16, dtype="fp32")
    assert prior == (tensor_state_hash(model.state_dict()), cursor.snapshot())
    old = torch.load(fp32, weights_only=True)
    old.pop("dtype")
    old["version"] = training_module.TRAINING_VERSION
    old["checkpoint_hash"] = _tree_hash({key: value for key, value in old.items() if key != "checkpoint_hash"})
    old_path = session_directory / "old.pt"
    torch.save(old, old_path)
    assert load_training_checkpoint(model, optimizer, spec(), cursor, old_path, dtype="fp32") == 0
    with pytest.raises(ValueError, match="dtype mismatch"):
        load_training_checkpoint(model, optimizer, spec(), cursor, old_path, dtype="bf16")
    malformed = torch.load(fp32, weights_only=True)
    malformed.pop("dtype")
    torch.save(malformed, session_directory / "malformed.pt")
    with pytest.raises(ValueError, match="format mismatch"):
        load_training_checkpoint(model, optimizer, spec(), cursor, session_directory / "malformed.pt")


@pytest.mark.skipif(not torch.cuda.is_available() or not torch.cuda.is_bf16_supported(),
                    reason="CUDA BF16 hardware is optional")
def test_cuda_bf16_training_keeps_fp32_master_and_adamw_state(session_directory):
    synthetic_episode().save(session_directory / "episode.json")
    dataset = ReplayDataset([session_directory / "episode.json"], spec())
    model = PolicyValueNetwork(ModelConfig(spec().board_channels, spec().condition_dim,
        spec().action_dim, channels=4, residual_blocks=1, lora_rank=2, lora_alpha=2.)).to("cuda")
    optimizer = create_optimizer(model, mode="base")
    cursor = DatasetCursor(dataset, 19)
    result = optimize(model, optimizer, PublicEncoder(spec()), cursor,
                      limits=TrainingLimits(steps=1, batch_size=1), dtype="bf16")
    assert result["steps"] == 1 and result["dtype"] == "bf16"
    training_module.validate_fp32_training_state(model, optimizer)
    checkpoint = session_directory / "bf16-cuda.pt"
    save_training_checkpoint(model, optimizer, spec(), cursor, checkpoint, completed_steps=1, dtype="bf16")
    assert torch.load(checkpoint, weights_only=True)["dtype"] == "bf16"


def test_optimizer_cancellation_is_classified_from_one_signal_read(session_directory):
    replay = session_directory / "episode.json"
    synthetic_episode().save(replay)
    dataset = ReplayDataset([replay], spec())
    model = PolicyValueNetwork(ModelConfig(spec().board_channels, spec().condition_dim,
        spec().action_dim, channels=4, residual_blocks=1, lora_rank=2, lora_alpha=2.))
    optimizer = create_optimizer(model, mode="base")
    cursor = DatasetCursor(dataset, 19)
    signals = iter((True, False))
    report = optimize(model, optimizer, PublicEncoder(spec()), cursor,
        limits=TrainingLimits(steps=1, batch_size=1), cancelled=lambda: next(signals))
    assert report["steps"] == 0 and report["stop_reason"] == "cancelled"
    assert cursor.offset == 0 and next(signals) is False


def test_legacy_game_commands_fail_before_artifact_or_native_setup(capsys):
    for command in (["choose", "--trace", "unused.json"], ["selfplay"]):
        assert cli.main(command) == 2
        assert "v6 game execution is retired" in capsys.readouterr().err


def test_cli_defaults_are_explicit_intent_summary_and_full_resnet():
    arguments = cli.parser().parse_args(["init"])
    assert (arguments.channels, arguments.blocks, arguments.rank) == (128, 8, 8)
    catalog_path = Path(__file__).resolve().parents[3] / "augment-chess/contracts/catalog/site-20260927.json"
    contract = cli.default_spec(str(catalog_path), observation_policy=observation_policy())
    assert contract.history_encoding == "public-history-summary-v1" and contract.action_encoding == "public-decision-intent-v1"
    assert cli.parser().parse_args(["evaluate", "--replay", "episode.json"]).backend == "ort"
    assert cli.parser().parse_args(["selfplay"]).max_plies == 2


def test_cli_typed_default_requests_pinned_v7_native_metadata(monkeypatch):
    from accelerate_chess.ir import V7_RULES_VERSION

    source = Path(__file__).resolve().parents[3] / "augment-chess" / "contracts" / "catalog"
    catalog = read_json(source / "site-20260928.json")
    policy = read_json(source / "observation-20260928.json")
    requests = []
    native = ModuleType("accelerate_chess._native")
    def catalog_for(version=None):
        requests.append(("catalog", version))
        return catalog
    def policy_for(version=None):
        requests.append(("policy", version))
        return policy
    native.site_catalog = catalog_for
    native.site_observation_policy = policy_for
    monkeypatch.setitem(sys.modules, "accelerate_chess._native", native)
    for family in ("mask-resnet", "entity-transformer"):
        contract = cli.default_spec(model_family=family)
        assert contract.rules_version == V7_RULES_VERSION
    assert requests == [("catalog", V7_RULES_VERSION), ("policy", V7_RULES_VERSION)] * 2
    with pytest.raises(ValueError, match="pinned v7"):
        cli.default_spec(str(source / "site-20260927.json"), observation_policy=policy,
                         model_family="mask-resnet")


def test_train_existing_run_slot_requires_matching_explicit_resume(session_directory, monkeypatch):
    root = session_directory / "train-slot-guard"
    checkpoint = root / "runs" / "training" / "training.pt"
    checkpoint.parent.mkdir(parents=True, exist_ok=True)
    checkpoint.write_bytes(b"prior checkpoint must survive")
    loaded = []

    def load_base_after_slot_check(*args):
        loaded.append(True)
        raise RuntimeError("model load reached")

    monkeypatch.setattr(cli, "load_base", load_base_after_slot_check)
    command = ["train", "--base", "base.pt", "--replay", "episode.json"]
    run = lambda options: cli.train(cli.parser().parse_args(command + options), root, spec(), lambda: False)

    with pytest.raises(FileExistsError, match="pass --resume"):
        run([])  # The default run ID must not silently replace prior training.
    with pytest.raises(FileExistsError, match="different checkpoint"):
        run(["--resume", str(root / "other-training.pt")])
    assert loaded == [] and checkpoint.read_bytes() == b"prior checkpoint must survive"

    with pytest.raises(RuntimeError, match="model load reached"):
        run(["--resume", str(checkpoint)])
    with pytest.raises(RuntimeError, match="model load reached"):
        run(["--resume", str(checkpoint), "--run-id", "continued"])
    assert loaded == [True, True] and checkpoint.read_bytes() == b"prior checkpoint must survive"
    assert not (checkpoint.parent / ".writer-claim").exists()


def test_selfplay_reserves_a_fresh_run_before_search_and_keeps_prior_outputs(session_directory, monkeypatch):
    with TemporaryDirectory(prefix="selfplay-slot-", dir=session_directory) as temporary:
        root = Path(temporary)
        episode = root / "datasets" / "verification" / "episode-0000.json"
        report = root / "reports" / "verification" / "selfplay.json"
        episode.parent.mkdir(parents=True)
        report.parent.mkdir(parents=True)
        episode.write_bytes(b"prior replay")
        report.write_bytes(b"prior report")

        def fail_search_setup(*args):
            raise RuntimeError("search setup failed")

        monkeypatch.setattr(cli, "_search", fail_search_setup)
        args = cli.parser().parse_args(["selfplay", "--run-id", "verification"])
        with pytest.raises(FileExistsError, match="new --run-id"):
            cli.selfplay(args, root, spec(), lambda: False)
        assert episode.read_bytes() == b"prior replay" and report.read_bytes() == b"prior report"

        monkeypatch.setattr(cli, "_manifest", lambda *args: root / "unused-manifest.json")
        setup = cli.parser().parse_args(["selfplay", "--run-id", "setup-failure"])
        with pytest.raises(RuntimeError, match="search setup failed"):
            cli.selfplay(setup, root, spec(), lambda: False)
        failure = read_json(root / "reports" / "setup-failure" / "failure.json")
        assert failure["status"] == "initialization-failed" and failure["episode"] is None
        assert (root / "datasets" / "setup-failure").is_dir()

        barrier = Barrier(2)
        def reserve():
            barrier.wait(timeout=10)
            try:
                reserve_slot(root, "datasets", "new-run")
                return True
            except FileExistsError:
                return False

        with ThreadPoolExecutor(max_workers=2) as workers:
            results = [future.result(timeout=15) for future in (workers.submit(reserve), workers.submit(reserve))]
        assert sorted(results) == [False, True]


def test_train_writer_claim_blocks_resume_and_stale_claim_requires_manual_recovery(session_directory, monkeypatch):
    with TemporaryDirectory(prefix="training-claim-", dir=session_directory) as temporary:
        root = Path(temporary)
        checkpoint = root / "runs" / "training" / "training.pt"
        checkpoint.parent.mkdir(parents=True)
        checkpoint.write_bytes(b"prior checkpoint")
        model_loaded = []
        monkeypatch.setattr(cli, "load_base", lambda *args: model_loaded.append(True))
        args = cli.parser().parse_args(["train", "--base", "base.pt", "--replay", "episode.json",
                                        "--resume", str(checkpoint)])
        with writer_claim(checkpoint.parent):
            owner = read_json(checkpoint.parent / ".writer-claim" / "owner.json")
            assert owner["pid"] == os.getpid()
            with pytest.raises(FileExistsError, match="verify its process has stopped"):
                cli.train(args, root, spec(), lambda: False)
        assert not (checkpoint.parent / ".writer-claim").exists()
        def fail_owner_metadata(*args):
            raise OSError("owner metadata write failed")
        with monkeypatch.context() as failing_metadata:
            failing_metadata.setattr("accelerate_chess.replay.atomic_json", fail_owner_metadata)
            with pytest.raises(OSError, match="owner metadata write failed"):
                with writer_claim(checkpoint.parent):
                    pytest.fail("writer should not enter without owner metadata")
        assert not (checkpoint.parent / ".writer-claim").exists()
        (checkpoint.parent / ".writer-claim").mkdir()  # Simulate an interrupted writer.
        with pytest.raises(FileExistsError, match="remove the stale claim manually"):
            cli.train(args, root, spec(), lambda: False)
        assert model_loaded == [] and checkpoint.read_bytes() == b"prior checkpoint"
        alias = root / "runs" / "aliased"
        try:
            alias.symlink_to(checkpoint.parent, target_is_directory=True)
        except (OSError, NotImplementedError):
            pass  # The stale-claim checks above still run on hosts without symlink support.
        else:
            aliased = cli.parser().parse_args(["train", "--base", "base.pt", "--replay", "episode.json",
                                                "--resume", str(checkpoint), "--run-id", "aliased"])
            with pytest.raises(ValueError, match="slot is a symlink"):
                cli.train(aliased, root, spec(), lambda: False)
            assert checkpoint.read_bytes() == b"prior checkpoint"


def test_export_refuses_existing_bundle_before_loading_and_blocks_concurrent_writer(session_directory, monkeypatch):
    with TemporaryDirectory(prefix="export-slot-", dir=session_directory) as temporary:
        root = Path(temporary)
        directory = root / "models" / "deployment"
        directory.mkdir(parents=True)
        model = directory / "model.onnx"
        manifest = directory / "manifest.json"
        model.write_bytes(b"prior onnx")
        manifest.write_bytes(b"prior manifest")
        loaded = []
        monkeypatch.setattr(cli, "load_base", lambda *args: loaded.append(True))
        args = cli.parser().parse_args(["export", "--base", "base.pt"])
        with pytest.raises(FileExistsError, match="new --slot"):
            cli.export(args, root, spec())
        with writer_claim(directory):
            with pytest.raises(FileExistsError, match="writer claim exists"):
                cli.export(args, root, spec())
        assert loaded == [] and model.read_bytes() == b"prior onnx" and manifest.read_bytes() == b"prior manifest"


def test_export_refuses_existing_file_or_link_and_aliased_slot(session_directory, monkeypatch):
    with TemporaryDirectory(prefix="export-link-", dir=session_directory) as temporary:
        root = Path(temporary)
        directory = root / "models" / "deployment"
        directory.mkdir(parents=True)
        target = root / "other-model.onnx"
        target.write_bytes(b"outside target")
        link = directory / "model.onnx"
        linked = True
        try:
            link.symlink_to(target)
        except (OSError, NotImplementedError):
            linked = False
            if link.is_symlink():
                link.unlink()
            link.write_bytes(b"prior onnx")
        def unexpected_load(*args):
            raise AssertionError("occupied or aliased slots must be rejected before model load")
        monkeypatch.setattr(cli, "load_base", unexpected_load)
        args = cli.parser().parse_args(["export", "--base", "base.pt"])
        with pytest.raises(FileExistsError, match="new --slot"):
            cli.export(args, root, spec())
        assert link.is_symlink() == linked and target.read_bytes() == b"outside target"
        if linked:
            # Packaged Windows Python can create a symlink whose APPDATA target
            # is redirected by the host and cannot be dereferenced. The raw
            # link target and the untouched target file still prove rejection.
            assert os.path.normcase(str(link.readlink()).removeprefix("\\\\?\\")) == os.path.normcase(str(target))
            if link.exists():
                assert link.read_bytes() == b"outside target"
        else:
            assert link.read_bytes() == b"prior onnx"

        alias = root / "models" / "aliased"
        alias_args = cli.parser().parse_args(["export", "--base", "base.pt", "--slot", "aliased"])
        try:
            alias.symlink_to(directory, target_is_directory=True)
        except (OSError, NotImplementedError):
            original_is_symlink = Path.is_symlink
            with monkeypatch.context() as aliased_path:
                aliased_path.setattr(Path, "is_symlink", lambda candidate: candidate == alias or original_is_symlink(candidate))
                with pytest.raises(ValueError, match="slot is a symlink"):
                    cli.export(alias_args, root, spec())
        else:
            with pytest.raises(ValueError, match="slot is a symlink"):
                cli.export(alias_args, root, spec())
        assert not (directory / ".writer-claim").exists()
        assert target.read_bytes() == b"outside target" and link.is_symlink() == linked


def test_selfplay_failed_replay_save_is_not_retried_and_writes_failure_report(session_directory):
    with TemporaryDirectory(prefix="selfplay-failure-", dir=session_directory) as temporary:
        root = Path(temporary)
        path = root / "datasets" / "oversize" / "episode-0000.json"
        class Recorder:
            saves = 0
            def save(self, path):
                self.saves += 1
                raise ValueError("public JSON artifact exceeds its 16 MiB storage boundary")

        recorder = Recorder()
        original = ValueError("public JSON artifact exceeds its 16 MiB storage boundary")
        cli._record_selfplay_failure(root, "oversize", path, recorder, original,
                                     save_attempted=True, replay_saved=False)
        report = read_json(root / "reports" / "oversize" / "failure.json")
        assert recorder.saves == 0 and report["status"] == "execution-failed"
        assert report["error"] == "ValueError"
        assert report["replay_saved"] is False and report["episode"] == str(path)


def test_selfplay_deadline_includes_setup_and_errors_remain_failures(session_directory, monkeypatch):
    root = session_directory / "bounded-selfplay"
    native = ModuleType("accelerate_chess._native")

    class NeverStartedPosition:
        @staticmethod
        def new_game(*args):
            raise AssertionError("an expired run must not start a native game")

    native.Position = NeverStartedPosition
    monkeypatch.setitem(sys.modules, "accelerate_chess._native", native)
    monkeypatch.setattr(cli, "_manifest", lambda *args: root / "unused-manifest.json")
    monkeypatch.setattr(cli, "_search", lambda *args: object())
    ticks = 0

    def clock():
        nonlocal ticks
        ticks += 1
        return 0. if ticks == 1 else .02

    monkeypatch.setattr(cli.time, "monotonic", clock)
    args = cli.parser().parse_args(["selfplay", "--run-id", "setup-overrun", "--elapsed-ms", "10"])
    report = cli.selfplay(args, root, spec(), lambda: False)
    assert report["episodes"] == [] and report["stop_reason"] == "elapsed"
    assert read_json(root / "reports" / "setup-overrun" / "selfplay.json") == report

    failed = synthetic_episode(terminal=False)
    path = root / "datasets" / "execution-error" / "episode-0000.json"
    cli._record_selfplay_failure(root, "execution-error", path, failed,
        RuntimeError("native transition failed"), save_attempted=False, replay_saved=False)
    failure = read_json(root / "reports" / "execution-error" / "failure.json")
    assert failure["status"] == "execution-failed" and failure["replay_saved"] is True
    assert ReplayEpisode.load(path, spec()).outcome == {
        "status": "unfinished", "winner": None, "reason": "RuntimeError: native transition failed"}


def test_selfplay_search_cancellation_keeps_unfinished_replay_without_failure_report(session_directory, monkeypatch):
    from accelerate_chess.adapter_client import GameAdapterClient

    root = session_directory / "cancelled-selfplay"
    monkeypatch.setattr(GameAdapterClient, "new_game",
                        staticmethod(lambda *args, **kwargs: TestPosition(1, reaction=True)))
    monkeypatch.setattr(cli, "_manifest", lambda *args: root / "unused-manifest.json")
    monkeypatch.setattr(cli, "NativeSourceFactory", lambda *args, **kwargs: object())
    for stage, message in (("belief", "belief reconstruction cancelled"),
                           ("search", "no public decision was evaluated before cancelled"),
                           ("real-error", "belief reconstruction time budget exhausted")):
        signal = {"set": False}

        def belief(*args, **kwargs):
            if stage != "search":
                signal["set"] = True
                raise SearchBudgetError(message)
            return object()

        def run(*args, **kwargs):
            signal["set"] = True
            raise SearchBudgetError(message)

        monkeypatch.setattr(cli, "ParticleBelief", belief)
        monkeypatch.setattr(cli, "_search", lambda *args: SimpleNamespace(
            evaluator=SimpleNamespace(session=SimpleNamespace(model_sha256="a" * 64)), run=run))
        args = cli.parser().parse_args(["selfplay", "--run-id", stage, "--max-plies", "1"])
        episode_path = root / "datasets" / stage / "episode-0000.json"
        if stage == "real-error":
            with pytest.raises(SearchBudgetError, match="time budget exhausted"):
                cli.selfplay(args, root, spec(), lambda: signal["set"])
            failure = read_json(root / "reports" / stage / "failure.json")
            assert failure["status"] == "execution-failed"
            assert ReplayEpisode.load(episode_path, spec()).outcome["status"] == "unfinished"
        else:
            report = cli.selfplay(args, root, spec(), lambda: signal["set"])
            assert report["stop_reason"] == "cancelled" and len(report["episodes"]) == 1
            episode = ReplayEpisode.load(episode_path, spec())
            assert episode.outcome == {"status": "unfinished", "winner": None, "reason": "cancelled"}
            assert episode.decisions == [] and list(episode.examples()) == []
            assert not (root / "reports" / stage / "failure.json").exists()


def test_cli_signal_does_not_hide_an_unrelated_native_failure(session_directory, monkeypatch, capsys):
    contract, _, _ = synthetic_typed_contract()
    monkeypatch.setattr(cli, "default_spec", lambda *args, **kwargs: contract)

    def failed_choose(*args):
        signal.raise_signal(signal.SIGINT)
        raise RuntimeError("native adapter failed")

    monkeypatch.setattr(cli, "choose", failed_choose)
    status = cli.main(["--artifact-root", str(session_directory), "--model-family", "mask-resnet",
                       "choose", "--trace", "unused-public-trace.json"])
    assert status == 2
    assert "RuntimeError: native adapter failed" in capsys.readouterr().err


def test_evaluation_report_identifies_complete_limited_and_cancelled_samples(session_directory, monkeypatch):
    replay_path = session_directory / "evaluation-episode.json"
    synthetic_episode().save(replay_path)
    episode = ReplayEpisode.load(replay_path, spec())

    class Evaluator:
        def __init__(self, manifest, contract, backend, *, threads):
            self.session = self
            self.model_sha256 = "a" * 64

        def evaluate(self, board, condition, action_features):
            return np.zeros((1, action_features.shape[1]), np.float32), np.zeros((1, 1), np.float32)

    monkeypatch.setattr(cli, "ProductionEvaluator", Evaluator)

    def run(name, limit, cancelled):
        args = cli.parser().parse_args(["evaluate", "--manifest", str(session_directory / "model.json"),
            "--replay", str(replay_path), "--max-samples", str(limit), "--run-id", name])
        report = cli.evaluate(args, session_directory, spec(), cancelled)
        assert read_json(session_directory / "reports" / name / "evaluation.json") == report
        assert report["version"] == "accelerate-evaluation-v1"
        assert report["replay_hash"] == episode.replay_hash
        assert report["encoder_hash"] == spec().digest
        assert report["model_sha256"] == "a" * 64
        assert report["decisions_available"] == 2 and report["sample_limit"] == limit
        return report

    complete = run("evaluation-complete", 2, lambda: False)
    assert complete["stop_reason"] == "complete" and complete["samples"] == 2
    assert all("value_mse" in metric for metric in complete["metrics"])
    limited = run("evaluation-limited", 1, lambda: False)
    assert limited["stop_reason"] == "sample-limit" and limited["samples"] == 1

    def signal_after_processed_samples(count):
        checks = 0

        def cancelled():
            nonlocal checks
            checks += 1
            return checks > count

        return cancelled

    late_complete_signal = signal_after_processed_samples(2)
    assert run("evaluation-late-complete", 2, late_complete_signal)["stop_reason"] == "complete"
    assert late_complete_signal()
    late_limited_signal = signal_after_processed_samples(1)
    assert run("evaluation-late-limited", 1, late_limited_signal)["stop_reason"] == "sample-limit"
    assert late_limited_signal()
    checks = 0

    def cancelled_after_first():
        nonlocal checks
        checks += 1
        return checks > 1

    interrupted = run("evaluation-cancelled", 2, cancelled_after_first)
    assert interrupted["stop_reason"] == "cancelled" and interrupted["samples"] == 1


def test_actual_native_cli_choose_bounded_episode_evaluate_and_explicit_activation(session_directory, capsys, monkeypatch):
    """Requires actual installed Rust rules/ort/tract; no production fallback."""
    from accelerate_chess import GameAdapterClient
    contract = cli.default_spec(model_family="mask-resnet")
    config = {"gameStyle": "normal", "draftDelete": True}
    config_path = session_directory / "public-config.json"
    atomic_json(config_path, config)
    initial = GameAdapterClient.new_game(config, 37, spec=contract)
    initial_observation = initial.observe(initial.decision_actor)
    first_intents = initial.legal_intents()[:1]
    assert first_intents, "the v7 public play surface must admit a first action"
    sample = batch_typed_positions([TypedEncoder(contract).encode(
        ObservationIR.from_public(initial_observation, contract), first_intents)])
    order = contract.feature_schema["input_order"]["mask-resnet"]
    inputs = dict(zip(order, sample.as_family_inputs("mask-resnet"), strict=True))
    model_args = cli.parser().parse_args(["--model-family", "mask-resnet", "init", "--channels", "8",
                                          "--blocks", "1", "--rank", "2"])
    model, _ = cli._typed_model(model_args, contract)
    manifest = export_typed_onnx(model.eval(), contract, session_directory / "deployment", inputs,
                                 architecture_family="mask-resnet")
    trace_path = session_directory / "public-trace.json"
    atomic_json(trace_path, PublicTracker(initial.observe(initial.decision_actor)).snapshot())
    model_prefix = ["--artifact-root", str(session_directory), "--model-family", "mask-resnet"]
    prefix = model_prefix + ["--threads", "1"]
    activation = session_directory / "models" / "active" / "activation.json"
    previous_activation = activation.read_bytes() if activation.exists() else None
    # This is a finite-work correctness check, not a one-second performance
    # gate. Keep real Rust/ORT/tract calls and control only the Python clock;
    # the elapsed branch below advances it after actual search has completed.
    logical_now = 0.
    monkeypatch.setattr(cli.time, "monotonic", lambda: logical_now)
    arguments = ["--manifest", str(manifest), "--config", str(config_path), "--iterations", "4", "--depth", "1",
                 "--particles", "2", "--proposals", "4", "--search-ms", "1000", "--belief-ms", "5000"]
    episode_budget_ms = 10000
    episode_limits = ["--max-plies", "1", "--elapsed-ms", str(episode_budget_ms)]
    assert cli.main(prefix + ["choose", "--trace", str(trace_path)] + arguments) == 0
    chosen = json.loads(capsys.readouterr().out.strip().splitlines()[-1])
    assert chosen["iterations"] == 4 and chosen["max_inference_batch"] == 4 and chosen["inference_batches"] == 2
    assert initial.bind_public_intent(chosen["intent"])
    assert cli.main(prefix + ["selfplay", "--verification", "--run-id", "verification"] + episode_limits + arguments) == 0
    capsys.readouterr()
    episode_path = session_directory / "datasets" / "verification" / "episode-0000.json"
    episode = ReplayEpisode.load(episode_path, contract)
    assert episode.outcome["status"] == "unfinished" and len(episode.decisions) == 1 and list(episode.examples()) == []
    assert episode.decisions[0]["search"]["max_inference_batch"] == 4
    assert cli.main(prefix + ["evaluate", "--manifest", str(manifest), "--backend", "tract", "--replay", str(episode_path), "--max-samples", "1"]) == 0
    capsys.readouterr()
    assert (activation.read_bytes() if activation.exists() else None) == previous_activation
    assert cli.main(model_prefix + ["--threads", "2", "evaluate", "--manifest", str(manifest), "--backend", "tract", "--replay", str(episode_path)]) == 2
    assert "requires threads=1" in capsys.readouterr().err
    assert (activation.read_bytes() if activation.exists() else None) == previous_activation
    assert cli.main(prefix + ["activate", "--manifest", str(manifest), "--sample-replay", str(episode_path),
                              "--expected-sha256", "0" * 64]) == 2
    assert (activation.read_bytes() if activation.exists() else None) == previous_activation
    capsys.readouterr()
    sha = load_manifest(manifest, contract)["model_sha256"]
    assert cli.main(prefix + ["activate", "--manifest", str(manifest), "--sample-replay", str(episode_path),
                              "--expected-sha256", sha]) == 0
    capsys.readouterr()
    assert read_json(activation)["model_sha256"] == sha
    original_run = cli.InformationSetSearch.run
    def deadline_after_search(*args, **kwargs):
        nonlocal logical_now
        result = original_run(*args, **kwargs)
        # Expire the explicit episode deadline by one millisecond only after
        # real inference/search, before binding/applying the selected intent.
        logical_now += (episode_budget_ms + 1) / 1000
        return result
    monkeypatch.setattr(cli.InformationSetSearch, "run", deadline_after_search)
    assert cli.main(prefix + ["selfplay", "--verification", "--run-id", "elapsed"] + episode_limits + arguments) == 2
    capsys.readouterr()
    elapsed = ReplayEpisode.load(session_directory / "datasets" / "elapsed" / "episode-0000.json", contract)
    assert elapsed.outcome == {"status": "unfinished", "winner": None, "reason": "elapsed"}
    assert len(elapsed.decisions) == 1 and not elapsed.decisions[0]["transition_completed"]
    assert elapsed.decisions[0]["search"]["iterations"] == 4
    assert all(tracker.steps == 0 for tracker in elapsed.trackers.values())
    logical_now = 0.
    # Cancel after actual Rust inference completes, before the selected choice
    # can bind/apply in the actual environment. Preserve the pending decision.
    def cancel_after_search(*args, **kwargs):
        result = original_run(*args, **kwargs)
        signal.raise_signal(signal.SIGINT)
        return result
    monkeypatch.setattr(cli.InformationSetSearch, "run", cancel_after_search)
    assert cli.main(prefix + ["selfplay", "--verification", "--run-id", "cancelled"] + episode_limits + arguments) == 130
    capsys.readouterr()
    cancelled = ReplayEpisode.load(session_directory / "datasets" / "cancelled" / "episode-0000.json", contract)
    assert cancelled.outcome == {"status": "unfinished", "winner": None, "reason": "cancelled"}
    assert len(cancelled.decisions) == 1 and not cancelled.decisions[0]["transition_completed"]
    assert all(tracker.steps == 0 for tracker in cancelled.trackers.values())
    assert list(cancelled.examples()) == []
