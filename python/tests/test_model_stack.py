"""Small contract scenarios; no trained weights or large fixtures in Git."""

from dataclasses import replace
from concurrent.futures import ThreadPoolExecutor
from io import BytesIO
import json
import hashlib
import os
from pathlib import Path
from functools import lru_cache
from types import SimpleNamespace
from threading import Barrier

import numpy as np
import pytest
import torch

from accelerate_chess.encoding import EncoderSpec, PublicEncoder, PublicObservation, batch_positions, canonical_json, decode_json_tail
from accelerate_chess.network.artifacts import MAX_MANIFEST_BYTES, OnnxEvaluator, _validate_graph, export_onnx, load_adapter, load_base, load_manifest, save_adapter, save_base
from accelerate_chess.network.model import AdapterDescriptor, ModelConfig, PolicyValueNetwork, is_adapter_parameter, masked_policy, tensor_state_hash
import accelerate_chess.training as training_module
from accelerate_chess.training import DatasetCursor, create_optimizer, load_training_checkpoint, save_training_checkpoint

torch.set_num_threads(1)


@lru_cache(maxsize=1)
def observation_policy():
    return json.loads((Path(__file__).resolve().parents[2] / "bridge/catalog/observation-20260927.json").read_text(encoding="utf-8"))


def spec() -> EncoderSpec:
    policy = observation_policy()
    return EncoderSpec(policy["rulesVersion"], "a" * 64, ("king", "pawn", "wall"), ("slime", "rule-ticket"), ("crown",), hashlib.sha256(canonical_json(policy).encode()).hexdigest(), piece_payload_bytes=256, public_payload_bytes=2304, action_payload_bytes=512).with_observation_policy(policy)


def resign(observation):
    observation["informationStateKey"] = hashlib.sha256(canonical_json({key: value for key, value in observation.items() if key != "informationStateKey"}).encode()).hexdigest()
    return observation


def observation(player="white"):
    board = [[None for _ in range(8)] for _ in range(8)]
    board[6][1] = {"type": "pawn", "color": "white", "moved": False, "shielded": True, "status": {"witchTrial": True, "witchTrialRemaining": 2}}
    board[2][0] = {"type": "wall", "color": "neutral", "status": {}}
    return resign({"protocolVersion": "accelerate-observation-v2", "viewer": player, "board": board, "turn": player,
            "ownCards": [{"id": "slime", "instanceId": "s1", "used": False}], "opponentHandCount": 3,
            "publicState": {"projectionVersion": observation_policy()["projectionVersion"], "observationPolicyHash": spec().observation_policy_hash, "deathmatchStatus": {"active": False, "warning": False}, "actionsRemaining": 1, "moveCount": 2, "fullMove": 1, "ruleCardIds": ["crown"], "revealedOpponentCards": [{"id": "rule-ticket", "instanceId": "public-opponent-1", "used": True}], "boardMarks": [{"kind": "meteor", "square": {"row": 3, "col": 2}}], "relationships": [], "overlays": []},
            "history": [{"type": "move", "color": "black", "from": {"row": 1, "col": 2}}], "informationStateKey": ""})


def actions():
    return [{"type": "move", "color": "white", "from": {"row": 6, "col": 1}, "move": {"row": 5, "col": 1, "jumpCapture": False}, "positionKey": "state-1"},
            {"type": "card", "color": "white", "cardId": "slime", "cardInstanceId": "s1", "target": {"row": 3, "col": 2, "selections": [{"row": 2, "col": 2}]}, "positionKey": "state-1"}]


def tiny_model(contract: EncoderSpec | None = None):
    contract = contract or spec()
    torch.manual_seed(12)
    return PolicyValueNetwork(ModelConfig(contract.board_channels, contract.condition_dim, contract.action_dim, channels=4, residual_blocks=1, lora_rank=2, lora_alpha=2.))


@pytest.fixture
def artifact_directory():
    explicit = os.environ.get("ACCELERATE_TEST_ARTIFACTS")
    if explicit:
        directory = Path(explicit) / "model-stack"
    elif os.environ.get("RUNNER_TEMP"):
        directory = Path(os.environ["RUNNER_TEMP"]) / "Accelerate" / "tmp" / "model-stack"
    elif os.environ.get("APPDATA"):
        directory = Path(os.environ["APPDATA"]) / "Accelerate" / "tmp" / "full-stack-implementation" / "model-stack"
    else:
        directory = Path(os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache"))) / "accelerate" / "model-stack"
    directory.mkdir(parents=True, exist_ok=True)
    return directory


def tensors(batch):
    return tuple(torch.from_numpy(value) for value in (batch.board, batch.condition, batch.action_features))


def test_dataset_cursor_preserves_shuffle_state_when_batch_read_fails():
    class FlakyDataset:
        digest = "synthetic-dataset"
        fail_once = True

        def __len__(self):
            return 2

        def __getitem__(self, index):
            if index == 0 and self.fail_once:
                self.fail_once = False
                raise OSError("replay input changed during batch read")
            return index

    dataset = FlakyDataset()
    cursor = DatasetCursor(dataset, 19)
    before = cursor.snapshot()
    with pytest.raises(OSError, match="replay input changed"):
        cursor.next_batch(3)
    assert cursor.snapshot() == before
    assert cursor.next_batch(3) == DatasetCursor(dataset, 19).next_batch(3)


def test_public_encoder_preserves_attributes_action_identity_and_orientation():
    assert canonical_json({"value": 1.0, "zero": -0., "tiny": 1e-7}) == '{"tiny":1e-7,"value":1,"zero":0}'
    assert canonical_json({"\uffff": 2, "\U0001f600": 1}) == '{"\U0001f600":1,"\uffff":2}'
    contract = spec()
    encoder = PublicEncoder(contract)
    public = observation()
    original = actions()
    encoded = encoder.encode(public, original, belief_summary={"opponent-card-probabilities": {"slime": .5}})
    assert encoded.board.dtype == encoded.condition.dtype == encoded.action_features.dtype == np.float32
    assert encoded.condition[contract.card_ids.index("slime")] == 1
    assert encoded.condition[len(contract.card_ids) + contract.card_ids.index("rule-ticket")] == 1
    assert decode_json_tail(encoded.board[len(contract.piece_ids) + 4:, 6, 1]) == public["board"][6][1]
    assert decode_json_tail(encoded.action_features[1, len(contract.action_types) + len(contract.card_ids) + 6:]) == {key: value for key, value in original[1].items() if key != "positionKey"}
    original[1]["cardInstanceId"] = "mutation"
    assert encoded.actions[1]["cardInstanceId"] == "s1"
    changed = actions()
    changed[0]["move"]["jumpCapture"] = True
    different = encoder.encode(public, changed)
    assert different.action_keys[0] != encoded.action_keys[0]
    assert not np.array_equal(different.action_features[0], encoded.action_features[0])
    black = encoder.encode(observation("black"), actions())
    assert black.board[len(contract.piece_ids)+1, 1, 6] == 1
    assert black.board[len(contract.piece_ids)+1, 6, 1] == 0
    assert not encoded.board[len(contract.piece_ids):len(contract.piece_ids)+2, 2, 0].any()
    assert encoded.board[len(contract.piece_ids)+3, 2, 0] == 1
    assert decode_json_tail(black.board[len(contract.piece_ids)+4:, 5, 7]) == public["board"][2][0]
    # Opaque position IDs can depend on hidden state; only semantics are input.
    wrappers = [{"protocolVersion": "accelerate-action-v1", "positionId": "private-one", "actionId": "action-1", "payload": original[0]}]
    first = encoder.encode(public, wrappers)
    wrappers[0]["positionId"] = "private-two"
    wrappers[0]["payload"]["positionKey"] = "different-private-state"
    second = encoder.encode(public, wrappers)
    np.testing.assert_array_equal(first.action_features, second.action_features)
    assert first.action_keys == second.action_keys and first.actions != second.actions
    pending = observation()
    pending["turn"] = "black"  # Reaction/pending-choice actor may differ from board turn.
    resign(pending)
    assert encoder.encode(pending, actions()).condition[2 * len(contract.card_ids) + len(contract.rule_ids)] == 1
    assert contract.contract()["value_perspective"] == "observation.viewer"
    assert contract.contract()["board_fields"][3] == "own-known-moved"
    moved = observation("black")
    moved["board"][6][1]["moved"] = True  # Full-record display can expose this flag.
    resign(moved)
    assert encoder.encode(moved, actions()).board[len(contract.piece_ids) + 2, 1, 6] == 0


def test_public_boundary_capacity_and_catalog_fail_closed():
    encoder = PublicEncoder(spec())
    altered_policy = PublicEncoder(spec())
    altered_policy.policy["derivedPublicFields"].append("unreviewedHand")
    unreviewed = observation()
    unreviewed["publicState"]["unreviewedHand"] = ["hidden-card"]
    resign(unreviewed)
    with pytest.raises(ValueError, match="unknown public state"):
        altered_policy.encode(unreviewed, actions())
    leaked = observation()
    leaked["rngState"] = 17
    with pytest.raises(ValueError, match="exact public"):
        encoder.encode(leaked, actions())
    invalid_key = observation()
    invalid_key["informationStateKey"] = "old-16-digit-id"
    with pytest.raises(ValueError, match="SHA-256"):
        encoder.encode(invalid_key, actions())
    for tamper in (lambda data: data["history"].append({"type": "tamper"}), lambda data: data["board"][6][1].update({"hp": 3}), lambda data: data.update({"informationStateKey": "b" * 64})):
        tampered = observation()
        tamper(tampered)
        with pytest.raises(ValueError, match="identity mismatch"):
            encoder.encode(tampered, actions())
    with pytest.raises(ValueError, match="finite JSON"):
        canonical_json({"tooLarge": 2**53 + 1})
    with pytest.raises(ValueError, match="finite JSON"):
        canonical_json({"tooLargeFloat": float(2**53)})
    leaked = observation()
    leaked["publicState"]["private_cards"] = ["slime"]
    resign(leaked)
    with pytest.raises(ValueError, match="private field"):
        encoder.encode(leaked, actions())
    with pytest.raises(TypeError, match="PublicObservation"):
        encoder.encode(object(), actions())
    old = observation(); old["protocolVersion"] = "accelerate-observation-v1"; resign(old)
    with pytest.raises(ValueError, match="Observation v2"):
        encoder.encode(old, actions())
    wrong_policy = observation(); wrong_policy["publicState"]["observationPolicyHash"] = "b" * 64; resign(wrong_policy)
    with pytest.raises(ValueError, match="policy"):
        encoder.encode(wrong_policy, actions())
    previous_projection = observation(); previous_projection["publicState"]["projectionVersion"] = "source-visible-20260927-v2"; resign(previous_projection)
    with pytest.raises(ValueError, match="projection"):
        encoder.encode(previous_projection, actions())
    for status in (None, {"active": True}, {"active": True, "warning": 1}, {"active": True, "warning": False, "halfTurnsSinceProgress": 4}):
        invalid = observation()
        if status is None:
            del invalid["publicState"]["deathmatchStatus"]
        else:
            invalid["publicState"]["deathmatchStatus"] = status
        resign(invalid)
        with pytest.raises(ValueError, match="deathmatch"):
            encoder.encode(invalid, actions())
    warned = observation(); warned["publicState"]["deathmatchStatus"] = {"active": True, "warning": True}; resign(warned)
    assert not np.array_equal(encoder.encode(warned, actions()).condition, encoder.encode(observation(), actions()).condition)
    malformed_status = observation(); malformed_status["board"][6][1]["status"]["witchTrialRemaining"] = "two"; resign(malformed_status)
    with pytest.raises(ValueError, match="status"):
        encoder.encode(malformed_status, actions())
    wrong_metadata = json.loads(canonical_json(observation_policy())); wrong_metadata["projectionVersion"] = "other"
    with pytest.raises(ValueError, match="policy"):
        EncoderSpec.from_dict(spec().to_dict(), observation_policy=wrong_metadata)
    unknown = observation()
    unknown["board"][6][1]["type"] = "unversioned-piece"
    resign(unknown)
    with pytest.raises(ValueError, match="piece"):
        encoder.encode(unknown, actions())
    nested = observation(); nested["publicState"]["winterKingdom"] = {"enabled": True, "previewIds": ["secret-piece"]}; resign(nested)
    with pytest.raises(ValueError, match="publicState.winterKingdom"):
        encoder.encode(nested, actions())
    with pytest.raises(ValueError, match="permits"):
        PublicEncoder(replace(spec(), action_payload_bytes=8)).encode(observation(), actions())
    with pytest.raises(ValueError, match="duplicate"):
        encoder.encode(observation(), [actions()[0], actions()[0]])
    # Explicit NN history summary is bounded while full trace/hash stays exact.
    long_history = observation()
    event = {"kind": "transition", "actor": "white", "nextActor": "black", "phase": "play", "boardChanges": [], "ownCards": [], "revealedOpponentCards": [], "captures": {"white": [], "black": []}, "result": {"outcome": None}}
    long_history["history"] = [event.copy() for _ in range(100)]
    resign(long_history)
    with pytest.raises(ValueError, match="permits"):
        encoder.encode(long_history, actions())
    summarized_spec = replace(spec(), history_encoding="public-history-summary-v1")
    summarized = PublicEncoder(summarized_spec).encode(long_history, actions())
    tail = decode_json_tail(summarized.condition[2 * len(spec().card_ids) + len(spec().rule_ids) + 4:])
    assert tail["history"]["event_count"] == 100 and len(tail["history"]["recent_events"]) == 8
    assert tail["history"]["history_hash"] == hashlib.sha256(canonical_json(long_history["history"]).encode()).hexdigest()
    assert len(long_history["history"]) == 100 and summarized_spec.digest != spec().digest
    intent_spec = replace(summarized_spec, action_encoding="public-decision-intent-v1")
    with pytest.raises(ValueError, match="private position metadata"):
        PublicEncoder(intent_spec).encode(long_history, actions())
    intent_encoder=PublicEncoder(intent_spec)
    first=intent_encoder.encode(long_history,[{"type":"trolleyChoice","color":"white","doomedIndex":0}])
    second=intent_encoder.encode(long_history,[{"type":"trolleyChoice","color":"white","doomedIndex":1}])
    assert first.action_keys!=second.action_keys
    with pytest.raises(ValueError, match="private field"):
        intent_encoder.encode(long_history,[{"type":"trolleyChoice","color":"white","doomedIndex":0,"windowId":"random-private-window"}])
    assert intent_spec.digest != summarized_spec.digest


def test_padding_terminal_rows_and_model_input_checks():
    with pytest.raises(ValueError, match="aggregate parameter budget"):
        ModelConfig(1000000, 1000000, 1000000, channels=4096, residual_blocks=64)
    with pytest.raises(ValueError, match="working set"):
        ModelConfig(1000000, 1000000, 1000000, channels=1, residual_blocks=1).validate_working_set(2, 1)
    encoder = PublicEncoder(spec())
    encoded = batch_positions([encoder.encode(observation(), actions()), encoder.encode(observation(), [])])
    model = tiny_model().eval()
    logits, value = model.evaluate(*tensors(encoded))
    assert logits.shape == (2, 2) and value.shape == (2, 1)
    probabilities = masked_policy(logits, torch.from_numpy(encoded.action_mask))
    torch.testing.assert_close(probabilities[0].sum(), torch.tensor(1.))
    assert not probabilities[1].any()
    first = encoder.encode(observation(), actions())
    with pytest.raises(ValueError, match="action feature count"):
        batch_positions([replace(first, action_features=first.action_features[:1])])
    with pytest.raises(ValueError, match="finite float32"):
        batch_positions([replace(first, condition=np.full_like(first.condition, np.nan))])
    invalid = list(tensors(encoded))
    invalid[1] = invalid[1].clone()
    invalid[1][0, 0] = float("nan")
    with pytest.raises(ValueError, match="finite"):
        model.evaluate(*invalid)
    model.train()
    with pytest.raises(ValueError, match="eval"):
        model.evaluate(*tensors(encoded))


def test_synthetic_base_and_adapter_single_steps_preserve_freeze_contract():
    # Exactly one base and one adapter optimizer step: code validation only.
    torch.set_num_threads(1)
    encoder = PublicEncoder(spec())
    batch = batch_positions([encoder.encode(observation(), actions()), encoder.encode(observation("black"), actions())])
    model = tiny_model()
    old_adapter = tensor_state_hash(model.adapter_state())
    optimizer = torch.optim.AdamW(model.configure_training("base"), lr=1e-3)
    optimizer.zero_grad()
    logits, value = model(*tensors(batch))
    loss = -torch.log_softmax(logits, dim=1)[:, 0].mean() + (value - .5).square().mean()
    loss.backward()
    assert model.blocks[0].film.weight.grad is not None and model.blocks[0].film.weight.grad.abs().sum() > 0
    optimizer.step()
    assert tensor_state_hash(model.adapter_state()) == old_adapter
    frozen_hash = model.base_hash
    descriptor = model.adapter_descriptor(spec().digest)
    optimizer = torch.optim.AdamW(model.configure_training("adapter"), lr=1e-3)
    model.train()
    assert all(not layer.training for layer in model.modules() if isinstance(layer, torch.nn.BatchNorm2d))
    optimizer.zero_grad()
    logits, value = model(*tensors(batch))
    loss = -torch.log_softmax(logits, dim=1)[:, 1].mean() + (value + .5).square().mean()
    loss.backward()
    assert all(parameter.grad is None for name, parameter in model.named_parameters() if not is_adapter_parameter(name))
    optimizer.step()
    assert model.base_hash == frozen_hash
    assert tensor_state_hash(model.adapter_state()) != old_adapter
    model.eval()
    before = model.evaluate(*tensors(batch))
    original_hash = tensor_state_hash(model.state_dict())
    merged = model.merged_copy(descriptor, spec().digest)
    for actual, expected in zip(merged.evaluate(*tensors(batch)), before, strict=True):
        torch.testing.assert_close(actual, expected, atol=1e-5, rtol=1e-4)
    assert tensor_state_hash(model.state_dict()) == original_hash
    assert not merged.training and all(not parameter.requires_grad for parameter in merged.parameters())


def test_static_merge_rejects_dynamic_or_incompatible_adapters():
    model = tiny_model()
    descriptor = model.adapter_descriptor(spec().digest)
    with pytest.raises(ValueError, match="globally fixed"):
        model.merged_copy(replace(descriptor, generator="hypernetwork", condition_lifetime="position", mergeable=False), spec().digest)
    with pytest.raises(ValueError, match="compatibility"):
        model.merged_copy(replace(descriptor, base_hash="b" * 64), spec().digest)


def test_base_adapter_checkpoint_roundtrip_and_failed_load_preserves_model(artifact_directory):
    contract = spec()
    model = tiny_model()
    base_path, adapter_path = artifact_directory / "base.pt", artifact_directory / "adapter.pt"
    save_base(model, contract, base_path)
    with torch.no_grad():
        model.blocks[0].conv1.lora_b.fill_(.005)
    model.configure_training("adapter")
    save_adapter(model, contract, adapter_path)
    restored, actual_spec = load_base(base_path, contract)
    load_adapter(restored, actual_spec, adapter_path)
    assert restored.base_hash == model.base_hash
    assert tensor_state_hash(restored.adapter_state()) == tensor_state_hash(model.adapter_state())
    with torch.no_grad():
        restored.stem[0].weight[0, 0, 0, 0] += .01
    original = tensor_state_hash(restored.state_dict())
    with pytest.raises(ValueError, match="does not match"):
        load_adapter(restored, contract, adapter_path)
    assert original == tensor_state_hash(restored.state_dict())
    # Tiny malicious metadata must fail before allocating the described model.
    oversized = torch.load(base_path, weights_only=True)
    oversized["config"].update(board_channels=1000000, condition_dim=1000000, action_dim=1000000, channels=4096, residual_blocks=64)
    oversized_path = artifact_directory / "oversized-metadata.pt"
    torch.save(oversized, oversized_path)
    with pytest.raises(ValueError, match="aggregate parameter budget"):
        load_base(oversized_path)
    incompatible_policy = torch.load(base_path, weights_only=True)
    incompatible_policy["observation_policy"]["projectionVersion"] = "other-projection"
    policy_path = artifact_directory / "incompatible-policy.pt"
    torch.save(incompatible_policy, policy_path)
    with pytest.raises(ValueError, match="policy"):
        load_base(policy_path, contract)


def test_parallel_training_checkpoint_saves_use_distinct_temp_files(artifact_directory, monkeypatch):
    contract = spec()

    class SingleExampleDataset:
        spec = contract
        digest = "synthetic-checkpoint-dataset"

        def __len__(self):
            return 1

    dataset = SingleExampleDataset()
    model = tiny_model(contract)
    optimizer = create_optimizer(model, mode="base")
    cursor = DatasetCursor(dataset, 7)
    checkpoint = artifact_directory / "training-concurrent.pt"
    original_save = torch.save
    barrier = Barrier(2)
    temp_paths = []

    def concurrent_save(state, path):
        temp_paths.append(Path(path))
        barrier.wait(timeout=10)
        original_save(state, path)

    with monkeypatch.context() as patch:
        patch.setattr(training_module.torch, "save", concurrent_save)
        with ThreadPoolExecutor(max_workers=2) as pool:
            results = [pool.submit(save_training_checkpoint, model, optimizer, contract, cursor,
                                   checkpoint, completed_steps=step) for step in (1, 2)]
            for result in results:
                result.result(timeout=15)
    assert len(temp_paths) == 2 and temp_paths[0] != temp_paths[1]
    restored = tiny_model(contract)
    restored_optimizer = create_optimizer(restored, mode="base")
    assert load_training_checkpoint(restored, restored_optimizer, contract, DatasetCursor(dataset, 8), checkpoint) in (1, 2)


def test_onnx_dynamic_batch_actions_film_merge_and_manifest(artifact_directory, monkeypatch):
    torch.set_num_threads(1)
    contract = spec()
    model = tiny_model()
    with torch.no_grad():
        model.blocks[0].conv2.lora_b.fill_(.01)
    model.configure_training("adapter")
    model.eval()
    descriptor = model.adapter_descriptor(contract.digest)
    original_hash = tensor_state_hash(model.state_dict())
    manifest = export_onnx(model, contract, artifact_directory / "deployment", descriptor=descriptor)
    original_stat, original_open = Path.stat, Path.open
    apparent_size = MAX_MANIFEST_BYTES + 1
    allow_open = False
    def manifest_stat(path, *args, **kwargs):
        return SimpleNamespace(st_size=apparent_size) if path == manifest else original_stat(path, *args, **kwargs)
    class GrowingReader(BytesIO):
        def read(self, size=-1):
            assert size == MAX_MANIFEST_BYTES + 1
            return super().read(size)
    def manifest_open(path, *args, **kwargs):
        if path == manifest:
            assert allow_open
            return GrowingReader(b"{}" + b" " * (MAX_MANIFEST_BYTES - 1))
        return original_open(path, *args, **kwargs)
    with monkeypatch.context() as limit:
        limit.setattr(Path, "stat", manifest_stat)
        limit.setattr(Path, "open", manifest_open)
        with pytest.raises(ValueError, match="artifact exceeds byte limit"):
            load_manifest(manifest)
        apparent_size, allow_open = 2, True
        with pytest.raises(ValueError, match="artifact exceeds byte limit"):
            load_manifest(manifest)
    evaluator = OnnxEvaluator(manifest, contract)
    generator = np.random.default_rng(2)
    for batch_size, candidate_count in ((1, 1), (2, 5), (3, 2)):
        arrays = (generator.normal(size=(batch_size, contract.board_channels, 8, 8)).astype(np.float32),
                  generator.normal(size=(batch_size, contract.condition_dim)).astype(np.float32),
                  generator.normal(size=(batch_size, candidate_count, contract.action_dim)).astype(np.float32))
        reference = model.evaluate(*(torch.from_numpy(array) for array in arrays))
        actual = evaluator.evaluate(*arrays)
        for expected, observed in zip(reference, actual, strict=True):
            np.testing.assert_allclose(observed, expected.numpy(), atol=1e-5, rtol=1e-4)
        changed = evaluator.evaluate(arrays[0], arrays[1] + .7, arrays[2])
        assert not np.allclose(changed[1], actual[1])
    assert tensor_state_hash(model.state_dict()) == original_hash
    wrong = replace(contract, rules_version="different-rules")
    with pytest.raises(ValueError, match="encoder contract"):
        load_manifest(manifest, wrong)
    payload = json.loads(manifest.read_text(encoding="utf-8"))
    incompatible_policy = json.loads(canonical_json(payload))
    incompatible_policy["encoder"]["observation_policy"]["projectionVersion"] = "other-projection"
    manifest.write_text(json.dumps(incompatible_policy), encoding="utf-8")
    with pytest.raises(ValueError, match="policy"):
        load_manifest(manifest)
    payload["encoder_hash"] = "b" * 64
    manifest.write_text(json.dumps(payload), encoding="utf-8")
    import onnx
    from copy import deepcopy
    model_path = manifest.parent / "model.onnx"
    original_graph = onnx.load(model_path)
    try:
        wrong_symbols = deepcopy(original_graph)
        wrong_symbols.graph.input[1].type.tensor_type.shape.dim[0].dim_param = "unrelated-batch"
        onnx.save(wrong_symbols, model_path)
        with pytest.raises(ValueError, match="shared dynamic symbols"):
            _validate_graph(model_path, model.config)
        nonfinite = deepcopy(original_graph)
        weight = next(tensor for tensor in nonfinite.graph.initializer if tensor.data_type == onnx.TensorProto.FLOAT)
        array = onnx.numpy_helper.to_array(weight).copy()
        array.flat[0] = np.nan
        weight.CopyFrom(onnx.numpy_helper.from_array(array, weight.name))
        onnx.save(nonfinite, model_path)
        with pytest.raises(ValueError, match="non-finite ONNX weights"):
            _validate_graph(model_path, model.config)
        bad_attribute = deepcopy(original_graph)
        bad_attribute.graph.node[0].attribute.append(onnx.helper.make_attribute("bad-float", float("inf")))
        onnx.save(bad_attribute, model_path)
        with pytest.raises(ValueError, match="non-finite ONNX attribute"):
            _validate_graph(model_path, model.config)
        shape_only = deepcopy(original_graph)
        for node in shape_only.graph.node:
            for index, name in enumerate(node.input):
                if name == "condition":
                    node.input[index] = "constant-condition"
            for index, name in enumerate(node.output):
                if name in ("policy_logits", "value"):
                    node.output[index] = name + "-before-shape"
        shape_only.graph.initializer.append(onnx.numpy_helper.from_array(np.zeros((1, model.config.condition_dim), np.float32), "constant-condition"))
        shape_only.graph.initializer.append(onnx.numpy_helper.from_array(np.array(0., np.float32), "shape-zero"))
        shape_only.graph.node.extend([
            onnx.helper.make_node("Shape", ["condition"], ["condition-shape"]),
            onnx.helper.make_node("Cast", ["condition-shape"], ["condition-shape-float"], to=onnx.TensorProto.FLOAT),
            onnx.helper.make_node("ReduceSum", ["condition-shape-float"], ["condition-shape-sum"], keepdims=0),
            onnx.helper.make_node("Mul", ["condition-shape-sum", "shape-zero"], ["shape-only-zero"]),
            onnx.helper.make_node("Add", ["policy_logits-before-shape", "shape-only-zero"], ["policy_logits"]),
            onnx.helper.make_node("Add", ["value-before-shape", "shape-only-zero"], ["value"]),
        ])
        onnx.save(shape_only, model_path)
        with pytest.raises(ValueError, match="FiLM condition"):
            _validate_graph(model_path, model.config)
    finally:
        onnx.save(original_graph, model_path)
    with pytest.raises(ValueError, match="encoder contract"):
        load_manifest(manifest)
    # Restore the stable artifact slot for native backend parity consumers.
    payload["encoder_hash"] = contract.digest
    manifest.write_text(json.dumps(payload), encoding="utf-8")
    for key, changed in (("onnx", {**payload["onnx"], "inputs": ["board"]}), ("numerical_tolerance", {"atol": 1., "rtol": 1.})):
        invalid = {**payload, key: changed}
        manifest.write_text(json.dumps(invalid), encoding="utf-8")
        with pytest.raises(ValueError, match="ONNX deployment contract"):
            load_manifest(manifest)
    manifest.write_text(json.dumps(payload), encoding="utf-8")
