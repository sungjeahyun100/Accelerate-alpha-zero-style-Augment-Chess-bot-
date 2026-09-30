"""Small contract scenarios; no trained weights or large fixtures in Git."""

from dataclasses import replace
from concurrent.futures import ThreadPoolExecutor
from copy import deepcopy
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
from accelerate_chess.ir import TypedEncoderSpec
from accelerate_chess.network.artifacts import (
    MAX_MANIFEST_BYTES, OnnxEvaluator, _typed_frozen_source, _validate_graph, export_onnx,
    export_typed_onnx, file_sha256, load_adapter, load_base, load_manifest, save_adapter, save_base,
)
from accelerate_chess.network.entity_transformer import EntityTransformerConfig
from accelerate_chess.network.mask_resnet import MaskResNetConfig, MaskResNetPolicyValueNetwork
from accelerate_chess.network.model import AdapterDescriptor, ModelConfig, PolicyValueNetwork, is_adapter_parameter, masked_policy, tensor_state_hash
from accelerate_chess.network.typed_context import TypedContextConfig
import accelerate_chess.training as training_module
from accelerate_chess.training import DatasetCursor, create_optimizer, load_training_checkpoint, save_training_checkpoint

torch.set_num_threads(1)


def _mask_resnet_case():
    """Small typed public state with a linked record and ordered target child."""
    torch.manual_seed(20260929)
    context = TypedContextConfig((4, 4, 4, 4), (4, 4), (4, 4, 4, 4), hidden_dim=8)
    config = MaskResNetConfig(board_channels=6, typed_context=context, channels=8,
                              residual_blocks=2, lora_rank=2, lora_alpha=2.)
    model = MaskResNetPolicyValueNetwork(config).eval()
    spatial = torch.zeros(1, 6, 5, 7)
    spatial[:, 0] = 1.
    spatial[0, 0, 2, 3] = 0.
    spatial[0, 3, 2, 3] = 1.
    spatial[0, 4, 2, 3] = 1.
    layout_mask = torch.ones(1, 1, 5, 7, dtype=torch.bool)
    record_category = torch.zeros(1, 3, 4, dtype=torch.int64)
    record_category[0, 1, 0] = 1
    record_category[0, 2, 0] = 2
    record_numeric = torch.zeros(1, 3, 8)
    record_numeric[0, 1, 0] = .5
    record_coord = torch.tensor([[[0., 0.], [2., 3.], [4., 6.]]])
    record_spatial_valid = torch.tensor([[False, True, True]])
    record_mask = torch.ones(1, 3, dtype=torch.bool)
    relation_index = torch.tensor([[[1, 2]]], dtype=torch.int64)
    relation_category = torch.zeros(1, 1, 2, dtype=torch.int64)
    relation_numeric = torch.zeros(1, 1, 4)
    relation_mask = torch.ones(1, 1, dtype=torch.bool)
    candidate_category = torch.zeros(1, 3, 2, 4, dtype=torch.int64)
    candidate_category[0, :, 0, 0] = torch.tensor([0, 1, 2])
    candidate_numeric = torch.zeros(1, 3, 2, 8)
    candidate_numeric[0, :, 0, 0] = torch.tensor([.25, .5, .75])
    candidate_coord = torch.zeros(1, 3, 2, 2)
    candidate_coord[0, :, 1, :] = torch.tensor([2., 3.])
    candidate_coord_valid = torch.tensor([[[False, True]] * 3])
    candidate_parent = torch.tensor([[[-1, 0]] * 3], dtype=torch.int64)
    candidate_order = torch.tensor([[[0, 0]] * 3], dtype=torch.int64)
    candidate_target_index = torch.tensor([[[-1, 1]] * 3], dtype=torch.int64)
    candidate_node_mask = torch.ones(1, 3, 2, dtype=torch.bool)
    candidate_mask = torch.ones(1, 3, dtype=torch.bool)
    condition = torch.zeros(1, 8)
    inputs = (spatial, layout_mask, record_category, record_numeric, record_coord,
              record_spatial_valid, record_mask, relation_index, relation_category,
              relation_numeric, relation_mask, candidate_category, candidate_numeric,
              candidate_coord, candidate_coord_valid, candidate_parent, candidate_order,
              candidate_target_index, candidate_node_mask, candidate_mask, condition)
    return model, inputs


def _mask_resnet_select_candidates(inputs, selection):
    selected = list(inputs)
    for index in range(11, 20):
        selected[index] = inputs[index][:, selection]
    return tuple(selected)


@pytest.mark.parametrize("family", ("mask-resnet", "entity-transformer"))
@pytest.mark.parametrize("field,value", (("lora_alpha", True), ("lora_dropout", False)))
def test_typed_model_configs_reject_boolean_lora_numbers(family, field, value):
    context = TypedContextConfig((4,) * 4, (4,) * 2, (4,) * 4, hidden_dim=8)
    if family == "mask-resnet":
        config_type, arguments = MaskResNetConfig, {"board_channels": 6, "typed_context": context}
    else:
        config_type, arguments = EntityTransformerConfig, {"typed": context}
    with pytest.raises(ValueError, match="LoRA"):
        config_type(**arguments, **{field: value})


def test_mask_resnet_padding_and_candidate_partition_invariance():
    model, inputs = _mask_resnet_case()
    logits, value = model.evaluate(*inputs)
    assert logits.shape == (1, 3) and value.shape == (1, 1)
    assert torch.isfinite(logits).all() and torch.isfinite(value).all()

    padded = list(inputs)
    padded[0] = torch.randn(1, 6, 10, 12)
    padded[0][:, :, :5, :7] = inputs[0]
    padded[1] = torch.zeros(1, 1, 10, 12, dtype=torch.bool)
    padded[1][:, :, :5, :7] = True
    padded_logits, padded_value = model.evaluate(*padded)
    torch.testing.assert_close(padded_logits, logits, atol=1e-5, rtol=1e-4)
    torch.testing.assert_close(padded_value, value, atol=1e-5, rtol=1e-4)

    permuted = _mask_resnet_select_candidates(inputs, [2, 0, 1])
    permutation_logits, permutation_value = model.evaluate(*permuted)
    torch.testing.assert_close(permutation_logits, logits[:, [2, 0, 1]], atol=1e-6, rtol=1e-5)
    torch.testing.assert_close(permutation_value, value, atol=0, rtol=0)
    first_logits, first_value = model.evaluate(*_mask_resnet_select_candidates(inputs, slice(0, 1)))
    rest_logits, rest_value = model.evaluate(*_mask_resnet_select_candidates(inputs, slice(1, 3)))
    torch.testing.assert_close(torch.cat((first_logits, rest_logits), dim=1), logits, atol=1e-6, rtol=1e-5)
    torch.testing.assert_close(first_value, value, atol=0, rtol=0)
    torch.testing.assert_close(rest_value, value, atol=0, rtol=0)

    changed = list(inputs)
    changed[12] = inputs[12] + .7
    changed_logits, changed_value = model.evaluate(*changed)
    assert not torch.allclose(changed_logits, logits)
    torch.testing.assert_close(changed_value, value, atol=0, rtol=0)


def test_mask_resnet_holes_and_padding_excluded_from_normalization():
    model, inputs = _mask_resnet_case()
    with_hole = list(inputs)
    with_hole[0] = inputs[0].clone()
    with_hole[0][0, 0, 1, 1] = 0.
    with_hole[0][0, 2, 1, 1] = 1.
    baseline = model.evaluate(*with_hole)

    noisy_hole = list(with_hole)
    noisy_hole[0] = with_hole[0].clone()
    noisy_hole[0][0, [0, 1, 3, 4, 5], 1, 1] = 900.
    observed = model.evaluate(*noisy_hole)
    for expected, actual in zip(baseline, observed, strict=True):
        torch.testing.assert_close(actual, expected, atol=0, rtol=0)

    padded = list(with_hole)
    padded[0] = torch.randn(1, 6, 9, 11)
    padded[0][:, :, :5, :7] = with_hole[0]
    padded[1] = torch.zeros(1, 1, 9, 11, dtype=torch.bool)
    padded[1][:, :, :5, :7] = True
    reference = deepcopy(model).train()
    enlarged = deepcopy(model).train()
    reference_output = reference(*with_hole)
    enlarged_output = enlarged(*padded)
    for expected, actual in zip(reference_output, enlarged_output, strict=True):
        torch.testing.assert_close(actual, expected, atol=1e-5, rtol=1e-4)
    for expected, actual in zip(reference.modules(), enlarged.modules(), strict=True):
        if isinstance(expected, torch.nn.BatchNorm2d):
            torch.testing.assert_close(actual.running_mean, expected.running_mean, atol=1e-6, rtol=1e-5)
            torch.testing.assert_close(actual.running_var, expected.running_var, atol=1e-6, rtol=1e-5)

    all_holes = list(inputs)
    all_holes[0] = torch.zeros_like(inputs[0])
    all_holes[0][:, 2] = 1.
    all_hole_logits, all_hole_value = model.evaluate(*all_holes)
    assert torch.isfinite(all_hole_logits).all() and torch.isfinite(all_hole_value).all()


def test_mask_resnet_film_adapter_merge_and_input_limits():
    model, inputs = _mask_resnet_case()
    with torch.no_grad():
        model.spatial.condition_projection[0].weight.zero_()
        model.spatial.condition_projection[0].bias.zero_()
        model.spatial.condition_projection[0].weight[0, 0] = 1.
        model.spatial.blocks[0].film.weight[model.config.channels, 0] = 2.
        model.spatial.blocks[0].conv2.lora_b.fill_(.03)
    base_hash = model.base_hash
    adapter_parameters = tuple(model.configure_training("adapter"))
    assert adapter_parameters and all(parameter.requires_grad for parameter in adapter_parameters)
    assert all(not parameter.requires_grad for name, parameter in model.named_parameters()
               if not is_adapter_parameter(name))
    model.eval()
    conditioned = list(inputs)
    conditioned[-1] = torch.ones_like(inputs[-1])
    neutral_logits, neutral_value = model.evaluate(*inputs)
    active_logits, active_value = model.evaluate(*conditioned)
    assert not torch.allclose(active_logits, neutral_logits)
    assert not torch.allclose(active_value, neutral_value)

    adapter_hash = tensor_state_hash(model.adapter_state())
    optimizer = torch.optim.SGD(adapter_parameters, lr=.01)
    training_logits, training_value = model(*conditioned)
    (training_logits.mean() + training_value.mean()).backward()
    optimizer.step()
    optimizer.zero_grad(set_to_none=True)
    assert tensor_state_hash(model.adapter_state()) != adapter_hash
    assert model.base_hash == base_hash
    active_logits, active_value = model.evaluate(*conditioned)

    original_hash = tensor_state_hash(model.state_dict())
    merged = model.merged_copy(model.adapter_descriptor("a" * 64), "a" * 64)
    merged_logits, merged_value = merged.evaluate(*conditioned)
    torch.testing.assert_close(merged_logits, active_logits, atol=1e-5, rtol=1e-4)
    torch.testing.assert_close(merged_value, active_value, atol=1e-5, rtol=1e-4)
    assert tensor_state_hash(model.state_dict()) == original_hash and model.base_hash == base_hash
    with pytest.raises(ValueError, match="compatibility"):
        model.merged_copy(model.adapter_descriptor("a" * 64), "b" * 64)

    empty_layout = list(inputs)
    empty_layout[1] = torch.zeros_like(inputs[1])
    with pytest.raises(ValueError, match="layout cell"):
        model.evaluate(*empty_layout)
    too_wide = list(inputs)
    too_wide[0] = torch.zeros(1, 6, 5, 33)
    too_wide[1] = torch.ones(1, 1, 5, 33, dtype=torch.bool)
    with pytest.raises(ValueError, match="geometry"):
        model.evaluate(*too_wide)


def test_typed_export_and_manifest_bind_model_config_to_encoder_and_onnx(artifact_directory):
    catalog, policy = _typed_frozen_source()
    spec = TypedEncoderSpec.from_catalog(catalog, observation_policy=policy)
    small_model, inputs = _mask_resnet_case()
    bundle_path = artifact_directory / "typed-feature-dimensions"
    samples = dict(zip(spec.feature_schema["input_order"]["mask-resnet"], inputs, strict=True))
    with pytest.raises(ValueError, match="category embeddings"):
        export_typed_onnx(small_model, spec, bundle_path, samples, architecture_family="mask-resnet")

    vocabulary = len(spec.category_vocabulary)
    context = replace(small_model.config.typed_context,
                      record_category_sizes=(vocabulary,) * 4,
                      relation_category_sizes=(vocabulary,) * 2,
                      candidate_category_sizes=(vocabulary,) * 4)
    model = MaskResNetPolicyValueNetwork(replace(small_model.config, typed_context=context)).eval()
    manifest_path = export_typed_onnx(model, spec, bundle_path, samples, architecture_family="mask-resnet")
    payload = load_manifest(manifest_path, spec)
    for field, wrong in (("record_numeric_dim", 9), ("record_category_sizes", [vocabulary - 1] * 4)):
        changed = deepcopy(payload)
        changed["model_config"]["typed_context"][field] = wrong
        changed["model_config_hash"] = hashlib.sha256(canonical_json(changed["model_config"]).encode()).hexdigest()
        manifest_path.write_text(canonical_json(changed) + "\n", encoding="utf-8")
        with pytest.raises(ValueError, match="typed model .* differ"):
            load_manifest(manifest_path, spec, inspect_graph=False)

    import onnx

    model_path = manifest_path.with_name("model.onnx")
    original_bytes = model_path.read_bytes()
    original_graph = onnx.load_model_from_string(original_bytes)
    matches = [index for index, item in enumerate(original_graph.metadata_props)
               if item.key == "accelerate.model_config_sha256"]
    assert len(matches) == 1
    assert original_graph.metadata_props[matches[0]].value == payload["model_config_hash"]
    try:
        for defect in ("missing", "duplicate", "malformed", "different"):
            graph = deepcopy(original_graph)
            if defect == "missing":
                del graph.metadata_props[matches[0]]
            elif defect == "duplicate":
                duplicate = graph.metadata_props.add()
                duplicate.key = "accelerate.model_config_sha256"
                duplicate.value = payload["model_config_hash"]
            else:
                graph.metadata_props[matches[0]].value = "G" * 64 if defect == "malformed" else "f" * 64
            onnx.save_model(graph, model_path, save_as_external_data=False)
            changed = deepcopy(payload)
            changed["model_sha256"] = file_sha256(model_path)
            manifest_path.write_text(canonical_json(changed) + "\n", encoding="utf-8")
            with pytest.raises(ValueError, match="typed ONNX model configuration metadata mismatch"):
                load_manifest(manifest_path, spec)
    finally:
        model_path.write_bytes(original_bytes)
        manifest_path.write_text(canonical_json(payload) + "\n", encoding="utf-8")


def test_mask_resnet_warm_start_copies_only_compatible_residual_base_weights():
    target, _ = _mask_resnet_case()
    source = PolicyValueNetwork(ModelConfig(board_channels=3, condition_dim=4, action_dim=5,
                                            channels=8, residual_blocks=2, lora_rank=2,
                                            lora_alpha=2.)).eval()
    with torch.no_grad():
        for index, block in enumerate(source.blocks):
            block.conv1.base.weight.fill_(index + .1)
            block.conv2.base.weight.fill_(index + .2)
            block.bn1.running_mean.fill_(index + .3)
            block.film.weight.fill_(index + .4)
    source_hash = tensor_state_hash(source.state_dict())
    before = {name: value.detach().clone() for name, value in target.state_dict().items()}
    copied = target.warm_start_residual_convolutions(source)
    assert copied == tuple(f"spatial.blocks.{block}.{conv}.base.weight"
                           for block in range(2) for conv in ("conv1", "conv2"))
    for name, value in target.state_dict().items():
        if name in copied:
            parts = name.split(".")
            expected = getattr(source.blocks[int(parts[2])], parts[3]).base.weight
            torch.testing.assert_close(value, expected, atol=0, rtol=0)
        else:
            torch.testing.assert_close(value, before[name], atol=0, rtol=0)
    assert tensor_state_hash(source.state_dict()) == source_hash

    target_hash = tensor_state_hash(target.state_dict())
    incompatible = PolicyValueNetwork(ModelConfig(board_channels=3, condition_dim=4, action_dim=5,
                                                  channels=4, residual_blocks=2, lora_rank=2,
                                                  lora_alpha=2.)).eval()
    with pytest.raises(ValueError, match="incompatible residual weight"):
        target.warm_start_residual_convolutions(incompatible)
    missing = PolicyValueNetwork(ModelConfig(board_channels=3, condition_dim=4, action_dim=5,
                                             channels=8, residual_blocks=1, lora_rank=2,
                                             lora_alpha=2.)).eval()
    with pytest.raises(ValueError, match="block count"):
        target.warm_start_residual_convolutions(missing)
    source.blocks[1].conv2 = torch.nn.Identity()
    with pytest.raises(ValueError, match="missing residual weight"):
        target.warm_start_residual_convolutions(source)
    assert tensor_state_hash(target.state_dict()) == target_hash


@lru_cache(maxsize=1)
def observation_policy():
    return json.loads((Path(__file__).resolve().parents[3] / "augment-chess/contracts/catalog/observation-20260927.json").read_text(encoding="utf-8"))


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


def test_frozen_v7_public_projection_is_explicit_and_incompatible_with_v6():
    catalog_root = Path(__file__).resolve().parents[3] / "augment-chess" / "contracts" / "catalog"
    catalog = json.loads((catalog_root / "site-20260928.json").read_text(encoding="utf-8"))
    policy = json.loads((catalog_root / "observation-20260928.json").read_text(encoding="utf-8"))
    latest = EncoderSpec.from_catalog(catalog, observation_policy=policy,
                                      action_encoding="public-decision-intent-v1")
    assert latest.rules_version == "augment-site-20260928-e5ed84fcf8e72a24"
    assert policy["projectionVersion"] == "source-visible-20260928-v1"
    with pytest.raises(ValueError, match="policy version or rules provenance"):
        EncoderSpec.from_catalog(catalog, observation_policy=observation_policy())

    public = observation()
    public["publicState"].update(projectionVersion=policy["projectionVersion"],
                                  rulesVersion=catalog["rulesVersion"],
                                  observationPolicyHash=latest.observation_policy_hash)
    resign(public)
    intent = {"type": "move", "color": "white", "from": {"row": 6, "col": 1},
              "destination": {"row": 5, "col": 1}}
    assert PublicEncoder(latest).encode(public, [intent]).action_features.shape == (1, latest.action_dim)
    with pytest.raises(ValueError, match="policy compatibility"):
        PublicEncoder(spec()).encode(public, actions())
    wrong_rules = json.loads(canonical_json(public))
    wrong_rules["publicState"]["rulesVersion"] = spec().rules_version
    resign(wrong_rules)
    with pytest.raises(ValueError, match="projection provenance"):
        PublicEncoder(latest).encode(wrong_rules, [intent])
    missing_rules = json.loads(canonical_json(public))
    del missing_rules["publicState"]["rulesVersion"]
    resign(missing_rules)
    with pytest.raises(ValueError, match="projection provenance"):
        PublicEncoder(latest).encode(missing_rules, [intent])
    wrong_policy = json.loads(canonical_json(public))
    wrong_policy["publicState"]["observationPolicyHash"] = spec().observation_policy_hash
    resign(wrong_policy)
    with pytest.raises(ValueError, match="policy compatibility"):
        PublicEncoder(latest).encode(wrong_policy, [intent])


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
        with pytest.raises(ValueError, match="non-finite.*attribute"):
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
