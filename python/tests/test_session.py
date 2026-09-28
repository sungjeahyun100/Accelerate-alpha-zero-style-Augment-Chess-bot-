"""Public replay and one synthetic optimizer step with deterministic resume."""
from copy import deepcopy
from dataclasses import replace
import hashlib
import json
import os
from pathlib import Path
import random
import signal

import numpy as np
import pytest
import torch

from accelerate_chess import cli
from accelerate_chess.encoding import PublicEncoder, batch_positions, canonical_json
from accelerate_chess.network.artifacts import export_onnx, load_manifest, save_base
from accelerate_chess.network.model import ModelConfig, PolicyValueNetwork, tensor_state_hash
from accelerate_chess.replay import EpisodeRecorder, ReplayEpisode, atomic_json, read_json
from accelerate_chess.search import PublicTracker, SearchResult
from accelerate_chess.training import (DatasetCursor, ReplayDataset, TrainingLimits, _rng_snapshot,
    _tree_hash, create_optimizer, load_training_checkpoint, optimize, save_training_checkpoint)
from test_search import TestAction, TestPosition, spec
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
    return directory


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


def test_public_replay_terminal_labels_and_streamed_dataset(session_directory, monkeypatch):
    completed = synthetic_episode()
    completed.save(session_directory / "episode.json")
    unfinished = synthetic_episode(terminal=False)
    unfinished.save(session_directory / "unfinished.json")
    episode = ReplayEpisode.load(session_directory / "episode.json", spec())
    examples = list(episode.examples())
    assert len(examples) == 2 and all(example.value == 1. for example in examples)
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


def test_cli_defaults_are_explicit_intent_summary_and_full_resnet():
    arguments = cli.parser().parse_args(["init"])
    assert (arguments.channels, arguments.blocks, arguments.rank) == (128, 8, 8)
    catalog_path = Path(__file__).parents[2] / "bridge/catalog/site-20260927.json"
    contract = cli.default_spec(str(catalog_path), observation_policy=observation_policy())
    assert contract.history_encoding == "public-history-summary-v1" and contract.action_encoding == "public-decision-intent-v1"
    assert cli.parser().parse_args(["evaluate", "--replay", "episode.json"]).backend == "ort"
    assert cli.parser().parse_args(["selfplay"]).max_plies == 2


def test_actual_native_cli_choose_bounded_episode_evaluate_and_explicit_activation(session_directory, capsys, monkeypatch):
    """Requires actual installed Rust rules/ort/tract; no production fallback."""
    from accelerate_chess import Position
    contract = cli.default_spec()
    model = PolicyValueNetwork(ModelConfig(contract.board_channels, contract.condition_dim, contract.action_dim, channels=4, residual_blocks=1, lora_rank=2, lora_alpha=2.)).eval()
    base = session_directory / "base.pt"
    save_base(model, contract, base)
    manifest = export_onnx(model, contract, session_directory / "deployment")
    config = {"gameStyle": "normal", "draftDelete": True}
    config_path = session_directory / "public-config.json"
    atomic_json(config_path, config)
    initial = Position.new_game(config, 37)
    trace_path = session_directory / "public-trace.json"
    atomic_json(trace_path, PublicTracker(initial.observe(initial.decision_actor)).snapshot())
    prefix = ["--artifact-root", str(session_directory), "--threads", "1"]
    activation = session_directory / "models" / "active" / "activation.json"
    previous_activation = activation.read_bytes() if activation.exists() else None
    arguments = ["--manifest", str(manifest), "--config", str(config_path), "--iterations", "4", "--depth", "1", "--particles", "2", "--proposals", "4"]
    assert cli.main(prefix + ["choose", "--trace", str(trace_path)] + arguments) == 0
    chosen = json.loads(capsys.readouterr().out.strip().splitlines()[-1])
    assert chosen["iterations"] == 4 and chosen["max_inference_batch"] == 4 and chosen["inference_batches"] == 2
    assert initial.bind_public_intent(chosen["intent"])
    assert cli.main(prefix + ["selfplay", "--verification", "--run-id", "verification", "--max-plies", "1"] + arguments) == 0
    capsys.readouterr()
    episode_path = session_directory / "datasets" / "verification" / "episode-0000.json"
    episode = ReplayEpisode.load(episode_path, contract)
    assert episode.outcome["status"] == "unfinished" and len(episode.decisions) == 1 and list(episode.examples()) == []
    assert episode.decisions[0]["search"]["max_inference_batch"] == 4
    assert cli.main(prefix + ["evaluate", "--manifest", str(manifest), "--backend", "tract", "--replay", str(episode_path), "--max-samples", "1"]) == 0
    capsys.readouterr()
    assert (activation.read_bytes() if activation.exists() else None) == previous_activation
    assert cli.main(["--artifact-root", str(session_directory), "--threads", "2", "evaluate", "--manifest", str(manifest), "--backend", "tract", "--replay", str(episode_path)]) == 2
    assert "requires threads=1" in capsys.readouterr().err
    assert (activation.read_bytes() if activation.exists() else None) == previous_activation
    assert cli.main(prefix + ["activate", "--manifest", str(manifest), "--expected-sha256", "0" * 64]) == 2
    assert (activation.read_bytes() if activation.exists() else None) == previous_activation
    capsys.readouterr()
    sha = load_manifest(manifest, contract)["model_sha256"]
    assert cli.main(prefix + ["activate", "--manifest", str(manifest), "--expected-sha256", sha]) == 0
    capsys.readouterr()
    assert read_json(activation)["model_sha256"] == sha
    original_run = cli.InformationSetSearch.run
    real_clock = cli.time.monotonic
    deadline_expired = False
    def deadline_after_search(*args, **kwargs):
        nonlocal deadline_expired
        result = original_run(*args, **kwargs)
        deadline_expired = True
        return result
    monkeypatch.setattr(cli.time, "monotonic", lambda: real_clock() + (11. if deadline_expired else 0.))
    monkeypatch.setattr(cli.InformationSetSearch, "run", deadline_after_search)
    assert cli.main(prefix + ["selfplay", "--verification", "--run-id", "elapsed", "--max-plies", "1"] + arguments) == 2
    capsys.readouterr()
    elapsed = ReplayEpisode.load(session_directory / "datasets" / "elapsed" / "episode-0000.json", contract)
    assert elapsed.outcome == {"status": "unfinished", "winner": None, "reason": "elapsed"}
    assert len(elapsed.decisions) == 1 and not elapsed.decisions[0]["transition_completed"]
    assert all(tracker.steps == 0 for tracker in elapsed.trackers.values())
    monkeypatch.setattr(cli.time, "monotonic", real_clock)
    # Cancel after actual Rust inference completes, before the selected choice
    # can bind/apply in the actual environment. Preserve the pending decision.
    def cancel_after_search(*args, **kwargs):
        result = original_run(*args, **kwargs)
        signal.raise_signal(signal.SIGINT)
        return result
    monkeypatch.setattr(cli.InformationSetSearch, "run", cancel_after_search)
    assert cli.main(prefix + ["selfplay", "--verification", "--run-id", "cancelled", "--max-plies", "1"] + arguments) == 130
    capsys.readouterr()
    cancelled = ReplayEpisode.load(session_directory / "datasets" / "cancelled" / "episode-0000.json", contract)
    assert cancelled.outcome == {"status": "unfinished", "winner": None, "reason": "cancelled"}
    assert len(cancelled.decisions) == 1 and not cancelled.decisions[0]["transition_completed"]
    assert all(tracker.steps == 0 for tracker in cancelled.trackers.values())
    assert list(cancelled.examples()) == []
