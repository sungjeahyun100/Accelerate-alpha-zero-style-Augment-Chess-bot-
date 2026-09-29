"""Real native CPU backends; generated ONNX bundles remain outside Git."""
from concurrent.futures import ThreadPoolExecutor
from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path

import jcs
import numpy as np
import pytest
import torch

from accelerate_chess import InferenceSession, ProductionEvaluator
from accelerate_chess.encoding import EncoderSpec
from accelerate_chess.network.artifacts import export_onnx, export_typed_onnx, load_manifest
from accelerate_chess.network.model import ModelConfig, PolicyValueNetwork, tensor_state_hash
from accelerate_chess.ir import TypedEncoderSpec
from accelerate_chess.network.typed_context import TypedContextConfig
from accelerate_chess.network.entity_transformer import EntityTransformer, EntityTransformerConfig
from accelerate_chess.network.mask_resnet import MaskResNetConfig, MaskResNetPolicyValueNetwork

torch.set_num_threads(1)


def _root():
    if os.environ.get("ACCELERATE_TEST_ARTIFACTS"):
        return Path(os.environ["ACCELERATE_TEST_ARTIFACTS"]) / "native-runtime"
    if os.environ.get("RUNNER_TEMP"):
        return Path(os.environ["RUNNER_TEMP"]) / "Accelerate" / "tmp" / "native-runtime"
    if os.environ.get("APPDATA"):
        return Path(os.environ["APPDATA"]) / "Accelerate" / "tmp" / "full-stack-implementation" / "native-runtime"
    return Path(os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache"))) / "accelerate" / "native-runtime"


def _spec(full=False, history_encoding="full", action_encoding="exact-payload", baseline="20260927"):
    catalog_root = Path(__file__).resolve().parents[2] / "bridge/catalog"
    catalog = json.loads((catalog_root / f"site-{baseline}.json").read_text(encoding="utf-8"))
    policy = json.loads((catalog_root / f"observation-{baseline}.json").read_text(encoding="utf-8"))
    return EncoderSpec.from_catalog(catalog, observation_policy=policy, history_encoding=history_encoding, action_encoding=action_encoding, **({} if full else dict(piece_payload_bytes=32, public_payload_bytes=64, action_payload_bytes=48)))


def _model(spec, full=False):
    torch.manual_seed(71)
    extra = {} if full else dict(channels=4, residual_blocks=1, lora_rank=2, lora_alpha=2.)
    return PolicyValueNetwork(ModelConfig(spec.board_channels, spec.condition_dim, spec.action_dim, **extra)).eval()


def _arrays(spec, batch, actions):
    generator = np.random.default_rng(batch * 19 + actions)
    return (generator.normal(size=(batch, spec.board_channels, 8, 8)).astype(np.float32),
            generator.normal(size=(batch, spec.condition_dim)).astype(np.float32),
            generator.normal(size=(batch, actions, spec.action_dim)).astype(np.float32))


def _typed_spec():
    root = Path(__file__).resolve().parents[2] / "bridge" / "catalog"
    catalog = json.loads((root / "site-20260928.json").read_text(encoding="utf-8"))
    policy = json.loads((root / "observation-20260928.json").read_text(encoding="utf-8"))
    return TypedEncoderSpec.from_catalog(catalog, observation_policy=policy)


def _typed_arrays(spec, family, *, batch=2, records=3, relations=2, actions=2, nodes=3, height=5, width=7):
    extents = {"batch": batch, "records": records, "relations": relations,
               "actions": actions, "nodes": nodes, "height": height, "width": width}
    order = spec.feature_schema["input_order"][family]
    result = {}
    for name in order:
        metadata = spec.feature_schema["inputs"][name]
        shape = tuple(extents.get(axis, axis) for axis in metadata["shape"])
        dtype = {"float32": np.float32, "int64": np.int64, "bool": np.bool_}[metadata["dtype"]]
        result[name] = np.zeros(shape, dtype=dtype)
    result["record_mask"][:] = True
    result["candidate_mask"][:] = True
    result["candidate_node_mask"][:, :, 0] = True
    result["candidate_parent"][:] = -1
    result["candidate_target_index"][:] = -1
    if family == "mask-resnet":
        result["layout_mask"][:] = True
    return result


def test_typed_v3_transformer_export_and_manifest_contract():
    import onnxruntime

    spec = _typed_spec()
    vocabulary = len(spec.category_vocabulary)
    context = TypedContextConfig((vocabulary,) * 4, (vocabulary,) * 2, (vocabulary,) * 4,
                                 hidden_dim=16)
    model = EntityTransformer(EntityTransformerConfig(context, blocks=1, heads=4, ffn_dim=32,
                                                       lora_rank=2, lora_alpha=2.)).eval()
    arrays = _typed_arrays(spec, "entity-transformer")
    reference = model.evaluate(*(torch.from_numpy(value) for value in arrays.values()))
    manifest_path = export_typed_onnx(model, spec, _root() / "typed-transformer", arrays,
                                      architecture_family="entity-transformer")
    manifest = load_manifest(manifest_path, spec)
    assert manifest["version"] == "onnx-policy-value-v3"
    assert manifest["model_io_version"] == "typed-policy-value-v1"
    assert {item["dtype"] for item in manifest["onnx"]["inputs"]} == {"float32", "int64", "bool"}
    session = onnxruntime.InferenceSession(str(manifest_path.with_name("model.onnx")),
                                           providers=["CPUExecutionProvider"])
    result = session.run(["policy_logits", "value"], arrays)
    for actual, expected in zip(result, reference, strict=True):
        np.testing.assert_allclose(actual, expected.detach().numpy(), atol=1e-5, rtol=1e-4)
    changed = deepcopy(manifest)
    changed["onnx"]["inputs"][0]["dtype"] = "float32"
    invalid = _root() / "typed-transformer-invalid"
    invalid.mkdir(parents=True, exist_ok=True)
    (invalid / "model.onnx").write_bytes(manifest_path.with_name("model.onnx").read_bytes())
    (invalid / "manifest.json").write_text(json.dumps(changed), encoding="utf-8")
    with pytest.raises(ValueError, match="contract"):
        load_manifest(invalid / "manifest.json", spec)


@pytest.mark.parametrize("family", ("entity-transformer", "mask-resnet"))
def test_typed_v3_native_backend_parity_and_input_boundary(family):
    spec = _typed_spec()
    vocabulary = len(spec.category_vocabulary)
    context = TypedContextConfig((vocabulary,) * 4, (vocabulary,) * 2, (vocabulary,) * 4,
                                 hidden_dim=16)
    if family == "entity-transformer":
        model = EntityTransformer(EntityTransformerConfig(context, blocks=1, heads=4, ffn_dim=32,
                                                           lora_rank=2, lora_alpha=2.)).eval()
    else:
        model = MaskResNetPolicyValueNetwork(MaskResNetConfig(6, context, channels=8,
                                                               residual_blocks=1, lora_rank=2,
                                                               lora_alpha=2.)).eval()
    arrays = _typed_arrays(spec, family)
    with torch.no_grad():
        reference = model.evaluate(*(torch.from_numpy(value) for value in arrays.values()))
    manifest_path = export_typed_onnx(model, spec, _root() / f"typed-native-{family}", arrays,
                                      architecture_family=family)
    for backend in ("ort", "tract"):
        evaluator = ProductionEvaluator(manifest_path, spec, backend)
        assert evaluator.architecture_family == family
        assert evaluator.spec.digest == spec.digest
        observed = evaluator.evaluate_typed(arrays)
        assert tuple(value.shape for value in observed) == ((2, 2), (2, 1))
        for actual, expected in zip(observed, reference, strict=True):
            np.testing.assert_allclose(actual, expected.detach().numpy(), atol=1e-5, rtol=1e-4)
        strided = {name: np.asfortranarray(value) for name, value in arrays.items()}
        for actual, expected in zip(evaluator.evaluate_typed(strided), observed, strict=True):
            np.testing.assert_array_equal(actual, expected)
        invalid = {name: value.copy() for name, value in arrays.items()}
        invalid["record_category"][0, 0, 0] = vocabulary
        with pytest.raises(ValueError, match="vocabulary"):
            evaluator.evaluate_typed(invalid)
        invalid = {name: value.copy() for name, value in arrays.items()}
        invalid["candidate_node_mask"][0, 0, 1] = True
        invalid["candidate_parent"][0, 0, 1] = 2
        with pytest.raises(ValueError, match="parent"):
            evaluator.evaluate_typed(invalid)
        invalid = dict(arrays)
        invalid["record_category"] = invalid["record_category"].astype(np.float32)
        with pytest.raises((TypeError, ValueError)):
            evaluator.evaluate_typed(invalid)
        invalid = dict(arrays)
        invalid["candidate_numeric"] = invalid["candidate_numeric"][:, :, :1, :]
        with pytest.raises(ValueError, match="axis"):
            evaluator.evaluate_typed(invalid)


def test_typed_v3_rejects_self_consistent_wrong_feature_schema():
    spec = _typed_spec()
    vocabulary = len(spec.category_vocabulary)
    context = TypedContextConfig((vocabulary,) * 4, (vocabulary,) * 2, (vocabulary,) * 4,
                                 hidden_dim=16)
    model = EntityTransformer(EntityTransformerConfig(context, blocks=1, heads=4, ffn_dim=32,
                                                       lora_rank=2, lora_alpha=2.)).eval()
    arrays = _typed_arrays(spec, "entity-transformer")
    manifest_path = export_typed_onnx(model, spec, _root() / "typed-schema-source", arrays,
                                      architecture_family="entity-transformer")
    original = json.loads(manifest_path.read_text(encoding="utf-8"))
    invalid = _root() / "typed-schema-invalid"
    invalid.mkdir(parents=True, exist_ok=True)
    (invalid / "model.onnx").write_bytes(manifest_path.with_name("model.onnx").read_bytes())
    for semantic in ("numeric_slots", "descriptor_identity", "card_aliases", "belief_summary"):
        altered = deepcopy(original)
        schema = altered["encoder"]["feature_schema"]
        if semantic == "numeric_slots":
            schema["numeric_slots"]["record"][0] = "private_scalar"
        elif semantic == "descriptor_identity":
            schema["descriptor_identity"] = "unversioned private identity"
        elif semantic == "card_aliases":
            schema["card_aliases"] = "duplicate each public card instance without identity links"
        else:
            schema["belief_summary"]["chance_prior"] = "private-outcome-prior"
        altered["encoder"]["feature_schema_hash"] = hashlib.sha256(jcs.canonicalize(schema)).hexdigest()
        altered["encoder_hash"] = hashlib.sha256(jcs.canonicalize(altered["encoder"])).hexdigest()
        (invalid / "manifest.json").write_text(json.dumps(altered), encoding="utf-8")
        with pytest.raises(ValueError, match="feature schema"):
            load_manifest(invalid / "manifest.json")
        with pytest.raises(ValueError, match="feature semantics"):
            InferenceSession(invalid / "manifest.json")


def test_v7_explicit_bundle_runs_both_backends_and_rejects_v6_spec():
    latest = _spec(history_encoding="public-history-summary-v1",
                   action_encoding="public-decision-intent-v1", baseline="20260928")
    previous = _spec(history_encoding="public-history-summary-v1",
                     action_encoding="public-decision-intent-v1")
    assert latest.digest != previous.digest
    model = _model(latest)
    manifest = export_onnx(model, latest, _root() / "v7-contract")
    assert load_manifest(manifest, latest)["encoder_hash"] == latest.digest
    with pytest.raises(ValueError, match="encoder contract mismatch"):
        load_manifest(manifest, previous)
    arrays = _arrays(latest, 1, 2)
    expected = model.evaluate(*(torch.from_numpy(value) for value in arrays))
    for backend in ("ort", "tract"):
        session = InferenceSession(manifest, backend, latest.digest)
        for actual, reference in zip(session.evaluate(*arrays), expected, strict=True):
            np.testing.assert_allclose(actual, reference.detach().numpy(), atol=1e-5, rtol=1e-4)
    with pytest.raises(ValueError, match="compatibility"):
        InferenceSession(manifest, expected_encoder_hash=previous.digest)


@pytest.mark.parametrize("full", [False, True], ids=["small", "resnet8x128"])
@pytest.mark.parametrize("history_encoding", ["full", "public-history-summary-v1"])
def test_real_ort_tract_base_adapter_merge_dynamic_shapes_and_film(full, history_encoding):
    spec = _spec(full, history_encoding, action_encoding="public-decision-intent-v1")
    model = _model(spec, full)
    if full:
        assert (model.config.residual_blocks, model.config.channels, model.config.lora_rank,
                model.config.lora_alpha, model.config.lora_dropout) == (8, 128, 8, 8., 0.)
    directory = _root() / f"{'full' if full else 'small'}-{history_encoding}"
    evidence = []
    before = tensor_state_hash(model.state_dict())
    base_path = export_onnx(model, spec, directory / "base")
    assert tensor_state_hash(model.state_dict()) == before
    # A deterministic nonzero adapter exercises merge math without running any
    # learning. Base/adapter optimizer-step smoke belongs to the model tests.
    with torch.no_grad():
        for block in model.blocks:
            block.conv1.lora_b.fill_(.001)
            block.conv2.lora_b.fill_(.0015)
    model.configure_training("adapter")
    model.eval()
    descriptor = model.adapter_descriptor(spec.digest)
    adapter_before = tensor_state_hash(model.state_dict())
    adapter_path = export_onnx(model, spec, directory / "adapter", descriptor=descriptor)
    assert tensor_state_hash(model.state_dict()) == adapter_before
    merged = model.merged_copy(descriptor, spec.digest)
    for kind, manifest, reference in (("base", base_path, None), ("adapter", adapter_path, model)):
        base_reference = _model(spec, full) if reference is None else reference
        sessions = [InferenceSession(manifest, backend, spec.digest) for backend in ("ort", "tract")]
        assert [session.backend for session in sessions] == ["ort", "tract"]
        for batch, actions in ((1, 1), (2, 5), (3, 2)):
            arrays = _arrays(spec, batch, actions)
            expected = base_reference.evaluate(*(torch.from_numpy(v) for v in arrays))
            if kind == "adapter":
                for left, right in zip(expected, merged.evaluate(*(torch.from_numpy(v) for v in arrays)), strict=True):
                    torch.testing.assert_close(left, right, atol=1e-5, rtol=1e-4)
            results = [session.evaluate(*arrays) for session in sessions]
            for session, observed in zip(sessions, results, strict=True):
                for actual, value in zip(observed, expected, strict=True):
                    np.testing.assert_allclose(actual, value.numpy(), atol=1e-5, rtol=1e-4)
                evidence.append({"kind": kind, "backend": session.backend, "batch": batch, "actions": actions,
                                 "condition_changed": False, "model_sha256": session.model_sha256,
                                 "max_abs_error": [float(np.max(np.abs(actual - value.numpy()))) for actual, value in zip(observed, expected, strict=True)]})
            changed_arrays = (arrays[0], arrays[1] + .7, arrays[2])
            changed_expected = base_reference.evaluate(*(torch.from_numpy(v) for v in changed_arrays))
            for session, previous in zip(sessions, results, strict=True):
                changed = session.evaluate(*changed_arrays)
                for actual, value in zip(changed, changed_expected, strict=True):
                    np.testing.assert_allclose(actual, value.numpy(), atol=1e-5, rtol=1e-4)
                assert not np.allclose(previous[1], changed[1])
                evidence.append({"kind": kind, "backend": session.backend, "batch": batch, "actions": actions,
                                 "condition_changed": True, "model_sha256": session.model_sha256,
                                 "max_abs_error": [float(np.max(np.abs(actual - value.numpy()))) for actual, value in zip(changed, changed_expected, strict=True)]})
        del sessions, base_reference
    assert tensor_state_hash(model.state_dict()) == adapter_before
    (directory / "parity.json").write_text(json.dumps({"encoder_hash": spec.digest, "catalog_hash": spec.catalog_hash,
        "history_encoding": spec.history_encoding, "action_encoding": spec.action_encoding,
        "architecture": {"blocks": model.config.residual_blocks, "channels": model.config.channels,
                         "lora_rank": model.config.lora_rank, "lora_alpha": model.config.lora_alpha},
        "atol": 1e-5, "rtol": 1e-4, "cases": evidence}, indent=2) + "\n", encoding="utf-8")


@pytest.fixture(scope="module")
def small_bundle():
    spec = _spec()
    manifest = export_onnx(_model(spec), spec, _root() / "boundary")
    return spec, manifest


def test_backend_selection_owned_arrays_strides_limits_and_concurrency(small_bundle):
    spec, manifest = small_bundle
    session = InferenceSession(manifest, expected_encoder_hash=spec.digest)
    assert session.backend == "ort"
    assert ProductionEvaluator(manifest, spec, "tract").backend == "tract"
    with pytest.raises(ValueError, match="unsupported backend"):
        InferenceSession(manifest, "automatic")
    with pytest.raises(ValueError, match="compatibility"):
        InferenceSession(manifest, expected_encoder_hash="b" * 64)
    with pytest.raises(ValueError, match="threads=1"):
        InferenceSession(manifest, "tract", threads=2)
    arrays = _arrays(spec, 2, 3)
    reference = session.evaluate(*arrays)
    strided = tuple(np.asfortranarray(value) for value in arrays)
    for left, right in zip(reference, session.evaluate(*strided), strict=True):
        np.testing.assert_array_equal(left, right)
    with ThreadPoolExecutor(max_workers=4) as pool:
        outputs = list(pool.map(lambda _: session.evaluate(*arrays), range(8)))
    for result in outputs:
        for actual, expected in zip(result, reference, strict=True):
            np.testing.assert_array_equal(actual, expected)
    outputs[0][0].fill(1000)
    np.testing.assert_array_equal(session.evaluate(*arrays)[0], reference[0])
    for invalid in ((arrays[0].astype(np.float64), arrays[1], arrays[2]),
                    (arrays[0][:, :, :7, :], arrays[1], arrays[2]),
                    (arrays[0], arrays[1], arrays[2][:, :0, :])):
        with pytest.raises((TypeError, ValueError)):
            session.evaluate(*invalid)
    invalid = tuple(value.copy() for value in arrays)
    invalid[1][0, 0] = np.nan
    with pytest.raises(ValueError, match="finite"):
        session.evaluate(*invalid)
    with pytest.raises(ValueError, match="limits"):
        InferenceSession(manifest, max_batch=1).evaluate(*arrays)
    with pytest.raises(ValueError, match="element"):
        InferenceSession(manifest, max_input_elements=1).evaluate(*arrays)


def test_runtime_rejects_manifest_semantics_hashes_and_graph_corruption(small_bundle):
    import onnx
    spec, manifest = small_bundle
    original = json.loads(manifest.read_text(encoding="utf-8"))
    model = manifest.with_name("model.onnx").read_bytes()
    directory = _root() / "invalid"
    directory.mkdir(parents=True, exist_ok=True)
    invalid_path = directory / "manifest.json"
    model_path = directory / "model.onnx"
    model_path.write_bytes(model)
    def write(value):
        invalid_path.write_text(json.dumps(value), encoding="utf-8")
    for field, value in (("model_sha256", "a" * 64), ("version", "old"),
                         ("model_file", "../model.onnx"), ("unexpected", 1)):
        bad = deepcopy(original)
        bad[field] = value
        write(bad)
        with pytest.raises(ValueError):
            InferenceSession(invalid_path)
    bad = deepcopy(original)
    bad["encoder"]["value_perspective"] = "actual-hidden-actor"
    bad["encoder_hash"] = hashlib.sha256(jcs.canonicalize(bad["encoder"])).hexdigest()
    write(bad)
    with pytest.raises(ValueError, match="semantics"):
        InferenceSession(invalid_path)
    graph = onnx.load_model_from_string(model)
    graph.graph.input[0].type.tensor_type.elem_type = onnx.TensorProto.DOUBLE
    corrupted = graph.SerializeToString()
    model_path.write_bytes(corrupted)
    bad = deepcopy(original)
    bad["model_sha256"] = hashlib.sha256(corrupted).hexdigest()
    write(bad)
    with pytest.raises(ValueError, match="float32"):
        InferenceSession(invalid_path, "tract")

    def reject_graph(change, message):
        graph = onnx.load_model_from_string(model)
        change(graph.graph)
        corrupted = graph.SerializeToString()
        model_path.write_bytes(corrupted)
        bad = deepcopy(original)
        bad["model_sha256"] = hashlib.sha256(corrupted).hexdigest()
        write(bad)
        for backend in ("ort", "tract"):
            with pytest.raises(ValueError, match=message):
                InferenceSession(invalid_path, backend)
        with pytest.raises(ValueError, match=message):
            load_manifest(invalid_path, spec)

    def nonfinite_repeated_attribute(graph):
        attribute = graph.node[0].attribute.add()
        attribute.name = "invalid_repeated_float"
        attribute.type = onnx.AttributeProto.FLOATS
        attribute.floats.append(float("nan"))

    def sparse_attribute(graph):
        attribute = graph.node[0].attribute.add()
        attribute.name = "unsupported_sparse"
        attribute.type = onnx.AttributeProto.SPARSE_TENSOR
        attribute.sparse_tensor.values.data_type = onnx.TensorProto.FLOAT

    reject_graph(nonfinite_repeated_attribute, "non-finite attribute")
    reject_graph(sparse_attribute, "sparse graph")
    reject_graph(lambda graph: setattr(graph.initializer[0], "name", "condition"), "shadows an input")
    reject_graph(lambda graph: graph.node[0].output.__setitem__(0, "condition"), "shadows an input")
