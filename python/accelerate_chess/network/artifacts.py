"""Checked base/adapter checkpoints and dynamic ONNX deployment bundles.

Callers choose an artifact directory outside the source checkout according to
the project's artifact policy. Loaders validate metadata and tensors before
mutating a live model; adapter generation/application/merge are separate APIs.
"""

from __future__ import annotations

from dataclasses import asdict
import hashlib
import json
import math
import os
from pathlib import Path
import tempfile
from typing import Any, Mapping

import torch
import numpy as np
from torch import Tensor

from ..encoding import EncoderSpec, canonical_json
from .model import AdapterDescriptor, ModelConfig, PolicyValueNetwork, tensor_state_hash


CHECKPOINT_VERSION = "model-checkpoint-v2"
BUNDLE_VERSION = "onnx-policy-value-v2"
TYPED_BUNDLE_VERSION = "onnx-policy-value-v3"
MAX_ARTIFACT_BYTES = 512 * 1024 * 1024
MAX_MANIFEST_BYTES = 2 * 1024 * 1024


def _onnx_contract() -> dict[str, Any]:
    return {"opset": 18, "dtype": "float32", "inputs": ["board", "condition", "action_features"], "outputs": ["policy_logits", "value"], "dynamic_axes": {"batch": ["board:0", "condition:0", "action_features:0", "policy_logits:0", "value:0"], "actions": ["action_features:1", "policy_logits:1"]}}


def _valid_hash(value: Any) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(character in "0123456789abcdef" for character in value)


def _atomic_torch_save(value: Any, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, suffix=".tmp", delete=False) as file:
        temporary = Path(file.name)
    try:
        torch.save(value, temporary)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def _copy_state(state: Mapping[str, Tensor]) -> dict[str, Tensor]:
    return {name: tensor.detach().cpu().clone() for name, tensor in state.items()}


def _validate_state(state: Any, template: Mapping[str, Tensor]) -> dict[str, Tensor]:
    if not isinstance(state, dict) or set(state) != set(template):
        raise ValueError("checkpoint tensor names differ from the model contract")
    for name, tensor in state.items():
        expected = template[name]
        if not isinstance(tensor, Tensor) or tensor.shape != expected.shape or tensor.dtype != expected.dtype:
            raise ValueError(f"invalid checkpoint tensor {name}")
        if tensor.is_floating_point() and not bool(torch.isfinite(tensor).all()):
            raise ValueError(f"nonfinite checkpoint tensor {name}")
    return state


def _load(path: str | Path) -> dict[str, Any]:
    if Path(path).stat().st_size > MAX_ARTIFACT_BYTES:
        raise ValueError("checkpoint exceeds the 512 MiB artifact size limit")
    payload = torch.load(path, map_location="cpu", weights_only=True, mmap=True)
    if not isinstance(payload, dict) or payload.get("version") != CHECKPOINT_VERSION:
        raise ValueError("unsupported checkpoint format")
    return payload


def _check_dimensions(model: PolicyValueNetwork, spec: EncoderSpec) -> None:
    if (model.config.board_channels, model.config.condition_dim, model.config.action_dim) != (spec.board_channels, spec.condition_dim, spec.action_dim):
        raise ValueError("encoder and model feature dimensions differ")
    if any(value.is_floating_point() and (value.dtype != torch.float32 or not bool(torch.isfinite(value).all())) for value in model.state_dict().values()):
        raise ValueError("model artifacts require finite float32 weights and buffers")


def save_base(model: PolicyValueNetwork, spec: EncoderSpec, path: str | Path) -> str:
    _check_dimensions(model, spec)
    if model.merged:
        raise ValueError("a merged deployment copy cannot overwrite a base checkpoint")
    state = _copy_state(model.base_state())
    fingerprint = tensor_state_hash(state)
    _atomic_torch_save({"version": CHECKPOINT_VERSION, "kind": "base", "config": asdict(model.config), "encoder": spec.to_dict(), "observation_policy": spec.observation_policy, "encoder_hash": spec.digest, "base_hash": fingerprint, "state": state}, Path(path))
    return fingerprint


def load_base(path: str | Path, expected_spec: EncoderSpec | None = None) -> tuple[PolicyValueNetwork, EncoderSpec]:
    payload = _load(path)
    if set(payload) != {"version", "kind", "config", "encoder", "observation_policy", "encoder_hash", "base_hash", "state"} or payload["kind"] != "base":
        raise ValueError("checkpoint is not a base model")
    spec = EncoderSpec.from_dict(payload["encoder"], observation_policy=payload["observation_policy"])
    if spec.digest != payload["encoder_hash"] or expected_spec is not None and expected_spec.digest != spec.digest:
        raise ValueError("base encoder compatibility mismatch")
    model = PolicyValueNetwork(ModelConfig(**payload["config"]))
    _check_dimensions(model, spec)
    state = _validate_state(payload["state"], model.base_state())
    if tensor_state_hash(state) != payload["base_hash"]:
        raise ValueError("base model hash mismatch")
    model.load_state_dict({**model.state_dict(), **state}, strict=True)
    model.eval()
    return model, spec


def save_adapter(model: PolicyValueNetwork, spec: EncoderSpec, path: str | Path, descriptor: AdapterDescriptor | None = None) -> AdapterDescriptor:
    _check_dimensions(model, spec)
    if model.merged:
        raise ValueError("merged models no longer contain separate adapters")
    descriptor = descriptor or model.adapter_descriptor(spec.digest)
    if (descriptor.base_hash, descriptor.config_hash, descriptor.encoder_hash) != (model.base_hash, model.config.digest, spec.digest):
        raise ValueError("adapter compatibility mismatch")
    # The descriptor exposes the Hypernetwork extension boundary. This file
    # format stores static parameters and therefore refuses dynamic generators.
    descriptor.validate_merge()
    state = _copy_state(model.adapter_state())
    _atomic_torch_save({"version": CHECKPOINT_VERSION, "kind": "adapter", "descriptor": asdict(descriptor), "adapter_hash": tensor_state_hash(state), "state": state}, Path(path))
    return descriptor


def load_adapter(model: PolicyValueNetwork, spec: EncoderSpec, path: str | Path) -> AdapterDescriptor:
    _check_dimensions(model, spec)
    if model.merged:
        raise ValueError("cannot apply an adapter to a merged copy")
    payload = _load(path)
    if set(payload) != {"version", "kind", "descriptor", "adapter_hash", "state"} or payload["kind"] != "adapter":
        raise ValueError("checkpoint is not a static adapter")
    descriptor = AdapterDescriptor(**payload["descriptor"])
    descriptor.validate_merge()
    if (descriptor.base_hash, descriptor.config_hash, descriptor.encoder_hash) != (model.base_hash, model.config.digest, spec.digest):
        raise ValueError("adapter does not match this base, architecture, and encoder")
    state = _validate_state(payload["state"], model.adapter_state())
    if tensor_state_hash(state) != payload["adapter_hash"]:
        raise ValueError("adapter tensor hash mismatch")
    model.load_state_dict({**model.state_dict(), **state}, strict=True)
    model.configure_training("adapter")
    model.eval()
    return descriptor


def file_sha256(path: str | Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as file:
        for block in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _validate_graph(path: Path, config: ModelConfig) -> None:
    import onnx

    if path.stat().st_size > MAX_ARTIFACT_BYTES:
        raise ValueError("ONNX exceeds the 512 MiB artifact size limit")
    graph = onnx.load(path, load_external_data=False)
    if len(graph.opset_import) != 1 or graph.opset_import[0].domain not in ("", "ai.onnx") or graph.opset_import[0].version != 18 or graph.functions:
        raise ValueError("deployment contract requires ONNX opset 18")
    if graph.graph.sparse_initializer or len(graph.graph.node) > 100_000:
        raise ValueError("unsupported sparse graph or ONNX node budget")
    if [entry.name for entry in graph.graph.input] != ["board", "condition", "action_features"] or [entry.name for entry in graph.graph.output] != ["policy_logits", "value"]:
        raise ValueError("ONNX input/output names differ from the deployment contract")
    expected = {
        "board": [None, config.board_channels, 8, 8], "condition": [None, config.condition_dim],
        "action_features": [None, None, config.action_dim], "policy_logits": [None, None], "value": [None, 1],
    }
    symbols = {}
    for entry in (*graph.graph.input, *graph.graph.output):
        tensor = entry.type.tensor_type
        if tensor.elem_type != onnx.TensorProto.FLOAT:
            raise ValueError("ONNX deployment tensors must be float32")
        dims = tensor.shape.dim
        if len(dims) != len(expected[entry.name]):
            raise ValueError("ONNX rank mismatch")
        for axis, (dim, required) in enumerate(zip(dims, expected[entry.name], strict=True)):
            if required is None and not dim.dim_param or required is not None and dim.dim_value != required:
                raise ValueError("ONNX dimensions must preserve dynamic batch/actions and static feature sizes")
            if required is None:
                role = "batch" if axis == 0 else "actions"
                if role in symbols and symbols[role] != dim.dim_param:
                    raise ValueError("ONNX shared dynamic symbols mismatch")
                symbols[role] = dim.dim_param
    if symbols["batch"] == symbols["actions"]:
        raise ValueError("ONNX batch and action symbols must be independent")

    def validate_tensor(tensor):
        if tensor.data_location == onnx.TensorProto.EXTERNAL or tensor.external_data:
            raise ValueError("deployment bundle must not rely on unrecorded external tensor files")
        if tensor.data_type not in (onnx.TensorProto.FLOAT, onnx.TensorProto.UINT8, onnx.TensorProto.INT32, onnx.TensorProto.INT64, onnx.TensorProto.BOOL):
            raise ValueError("unsupported ONNX initializer dtype; floating weights must be float32")
        if tensor.data_type == onnx.TensorProto.FLOAT and not np.isfinite(onnx.numpy_helper.to_array(tensor)).all():
            raise ValueError("non-finite ONNX weights")

    input_names = {entry.name for entry in graph.graph.input}
    initializer_names: set[str] = set()
    for initializer in graph.graph.initializer:
        if not initializer.name or initializer.name in input_names or initializer.name in initializer_names:
            raise ValueError("ONNX initializer name is empty, duplicate, or shadows an input")
        initializer_names.add(initializer.name)
        validate_tensor(initializer)
    producers = {}
    for node in graph.graph.node:
        if node.domain not in ("", "ai.onnx") or not node.op_type:
            raise ValueError("unsupported ONNX node domain")
        for output in node.output:
            if not output:
                # ONNX permits an omitted optional output with an empty name.
                continue
            if output in input_names or output in initializer_names:
                raise ValueError("ONNX node output shadows an input or initializer")
            if output in producers:
                raise ValueError("duplicate ONNX producer")
            producers[output] = node
        for attribute in node.attribute:
            if attribute.HasField("sparse_tensor") or attribute.sparse_tensors:
                raise ValueError("unsupported sparse graph attribute")
            if attribute.HasField("g") or attribute.graphs or not math.isfinite(attribute.f) or any(not math.isfinite(value) for value in attribute.floats):
                raise ValueError("nested graph or non-finite attribute unsupported")
            if attribute.HasField("t"):
                validate_tensor(attribute.t)
            for tensor in attribute.tensors:
                validate_tensor(tensor)
    for output in ("policy_logits", "value"):
        pending = [output]
        reachable: set[str] = set()
        while pending:
            value = pending.pop()
            if value in reachable:
                continue
            reachable.add(value)
            if value in producers and producers[value].op_type not in ("Shape", "Size"):
                pending.extend(producers[value].input)
        if "condition" not in reachable:
            raise ValueError("FiLM condition input was disconnected from a deployment output")
    onnx.checker.check_model(graph, full_check=True)


def export_onnx(model: PolicyValueNetwork, spec: EncoderSpec, directory: str | Path, *, descriptor: AdapterDescriptor | None = None) -> Path:
    """Export an isolated eval copy; live training mode and tensors are preserved."""
    from copy import deepcopy

    _check_dimensions(model, spec)
    base_hash = model.base_hash
    if descriptor is not None:
        exported = model.merged_copy(descriptor, spec.digest)
        adapter_hash = tensor_state_hash(model.adapter_state())
    else:
        if model.training_mode == "adapter" or model.merged:
            raise ValueError("adapter export requires its compatibility descriptor")
        exported = deepcopy(model).cpu().eval()
        adapter_hash = None
    exported.cpu().eval()
    output_directory = Path(directory)
    output_directory.mkdir(parents=True, exist_ok=True)
    output_path = output_directory / "model.onnx"
    config = model.config
    inputs = (torch.zeros(2, config.board_channels, 8, 8), torch.zeros(2, config.condition_dim), torch.zeros(2, 3, config.action_dim))
    batch = torch.export.Dim("batch", min=1)
    actions = torch.export.Dim("actions", min=1)
    # No custom translation, backend-specific graph, or constant FiLM condition.
    torch.onnx.export(exported, inputs, output_path, dynamo=True, opset_version=18,
                      input_names=["board", "condition", "action_features"], output_names=["policy_logits", "value"],
                      dynamic_shapes=({0: batch}, {0: batch}, {0: batch, 1: actions}), external_data=False)
    _validate_graph(output_path, config)
    manifest = {
        "version": BUNDLE_VERSION, "model_file": "model.onnx", "model_sha256": file_sha256(output_path),
        "base_hash": base_hash, "adapter_hash": adapter_hash,
        "adapter": asdict(descriptor) if descriptor else None,
        "model_config": asdict(config), "model_config_hash": config.digest,
        "encoder": spec.contract(), "encoder_hash": spec.digest,
        "onnx": _onnx_contract(),
        "numerical_tolerance": {"atol": 1e-5, "rtol": 1e-4},
    }
    manifest_path = output_directory / "manifest.json"
    manifest_path.write_text(canonical_json(manifest) + "\n", encoding="utf-8")
    return manifest_path


def load_manifest(path: str | Path, expected_spec: EncoderSpec | None = None, *, inspect_graph: bool = True) -> dict[str, Any]:
    path = Path(path)
    if path.stat().st_size > MAX_MANIFEST_BYTES:
        raise ValueError("artifact exceeds byte limit")
    # The file may grow or be replaced after stat. Keep the actual read bounded
    # to the Rust loader's 2 MiB contract, with one excess byte for rejection.
    with path.open("rb") as source:
        encoded = source.read(MAX_MANIFEST_BYTES + 1)
    if len(encoded) > MAX_MANIFEST_BYTES:
        raise ValueError("artifact exceeds byte limit")
    manifest = json.loads(encoded.decode("utf-8"))
    if isinstance(manifest, dict) and manifest.get("version") == TYPED_BUNDLE_VERSION:
        return _validate_typed_manifest(manifest, path, expected_spec, inspect_graph=inspect_graph)
    required = {"version", "model_file", "model_sha256", "base_hash", "adapter_hash", "adapter", "model_config", "model_config_hash", "encoder", "encoder_hash", "onnx", "numerical_tolerance"}
    if not isinstance(manifest, dict) or set(manifest) != required or manifest["version"] != BUNDLE_VERSION or manifest["model_file"] != "model.onnx":
        raise ValueError("unsupported deployment manifest")
    spec = EncoderSpec.from_dict(manifest["encoder"]["spec"], observation_policy=manifest["encoder"]["observation_policy"])
    config = ModelConfig(**manifest["model_config"])
    if manifest["encoder"] != spec.contract() or manifest["encoder_hash"] != spec.digest or expected_spec is not None and expected_spec.digest != spec.digest:
        raise ValueError("deployment encoder contract mismatch")
    if manifest["model_config_hash"] != config.digest or (config.board_channels, config.condition_dim, config.action_dim) != (spec.board_channels, spec.condition_dim, spec.action_dim):
        raise ValueError("deployment model architecture mismatch")
    if manifest["onnx"] != _onnx_contract() or manifest["numerical_tolerance"] != {"atol": 1e-5, "rtol": 1e-4}:
        raise ValueError("unsupported ONNX deployment contract")
    if not _valid_hash(manifest["base_hash"]) or not _valid_hash(manifest["model_sha256"]) or manifest["adapter_hash"] is not None and not _valid_hash(manifest["adapter_hash"]):
        raise ValueError("invalid deployment model hashes")
    if manifest["adapter"] is not None:
        adapter = AdapterDescriptor(**manifest["adapter"])
        adapter.validate_merge()
        if (adapter.base_hash, adapter.config_hash, adapter.encoder_hash) != (manifest["base_hash"], config.digest, spec.digest) or not manifest["adapter_hash"]:
            raise ValueError("deployment adapter compatibility mismatch")
    elif manifest["adapter_hash"] is not None:
        raise ValueError("unexpected adapter hash")
    model_path = path.parent / manifest["model_file"]
    if model_path.stat().st_size > MAX_ARTIFACT_BYTES:
        raise ValueError("ONNX exceeds the 512 MiB artifact size limit")
    if file_sha256(model_path) != manifest["model_sha256"]:
        raise ValueError("ONNX model file hash mismatch")
    if inspect_graph:
        _validate_graph(model_path, config)
    return manifest


class OnnxEvaluator:
    """CPU reference backend used to validate export and run Python callers."""
    def __init__(self, manifest_path: str | Path, expected_spec: EncoderSpec | None = None):
        import onnxruntime as ort

        self.manifest = load_manifest(manifest_path, expected_spec)
        self.config = ModelConfig(**self.manifest["model_config"])
        self.session = ort.InferenceSession(str(Path(manifest_path).parent / "model.onnx"), providers=["CPUExecutionProvider"])

    def evaluate(self, board: Any, condition: Any, action_features: Any) -> tuple[Any, Any]:
        import numpy as np

        arrays = (board, condition, action_features)
        if any(not isinstance(value, np.ndarray) or value.dtype != np.float32 or not np.isfinite(value).all() for value in arrays):
            raise ValueError("inference expects finite float32 NumPy inputs")
        batch = board.shape[0] if board.ndim == 4 else 0
        if batch < 1 or board.shape[1:] != (self.config.board_channels, 8, 8) or condition.shape != (batch, self.config.condition_dim):
            raise ValueError("invalid board or condition shape")
        if action_features.ndim != 3 or action_features.shape[0] != batch or action_features.shape[1] < 1 or action_features.shape[2] != self.config.action_dim:
            raise ValueError("invalid action features shape")
        self.config.validate_working_set(batch, action_features.shape[1])
        result = self.session.run(["policy_logits", "value"], dict(zip(("board", "condition", "action_features"), arrays, strict=True)))
        if result[0].shape != action_features.shape[:2] or result[1].shape != (batch, 1) or any(not np.isfinite(value).all() for value in result):
            raise ValueError("invalid ONNX output")
        return result[0], result[1]


def _typed_hash(value: Any) -> str:
    return hashlib.sha256(canonical_json(value).encode("utf-8")).hexdigest()


def _typed_io_contract(spec: Any, family: str) -> dict[str, Any]:
    schema = spec.contract()["feature_schema"]
    if family not in ("mask-resnet", "entity-transformer"):
        raise ValueError("unsupported typed architecture family")
    ordered = schema["input_order"][family]
    if len(ordered) != len(set(ordered)) or set(ordered) != set(schema["inputs"] if family == "mask-resnet" else
                                                     (name for name in schema["inputs"] if name not in ("spatial", "layout_mask"))):
        raise ValueError("typed feature schema input order mismatch")
    inputs = []
    for name in ordered:
        item = schema["inputs"][name]
        dtype, shape = item["dtype"], item["shape"]
        if dtype not in ("float32", "int64", "bool") or not isinstance(shape, list) or not shape:
            raise ValueError(f"invalid typed feature schema for {name}")
        inputs.append({"name": name, "dtype": dtype, "shape": shape})
    return {"opset": 18, "inputs": inputs,
            "outputs": [{"name": "policy_logits", "dtype": "float32", "shape": ["batch", "actions"]},
                        {"name": "value", "dtype": "float32", "shape": ["batch", 1]}]}


def _typed_resource_limits() -> dict[str, Any]:
    return {"axis_maxima": {"batch": 64, "actions": 4096, "records": 2048, "relations": 8192,
                            "nodes": 64, "height": 32, "width": 32},
            "max_input_bytes": 64 * 1024 * 1024,
            "max_intermediate_bytes": 256 * 1024 * 1024}


def _validate_typed_adapter_metadata(family: str, adapter: Any, base_hash: str,
                                     config_hash: str, encoder_hash: str) -> None:
    version = {"mask-resnet": "lora-convolution-v1",
               "entity-transformer": "lora-transformer-qv-v1"}[family]
    if (not isinstance(adapter, dict)
            or set(adapter) != {"base_hash", "config_hash", "encoder_hash", "generator",
                                "condition_lifetime", "mergeable", "version"}
            or (adapter["base_hash"], adapter["config_hash"], adapter["encoder_hash"])
            != (base_hash, config_hash, encoder_hash)
            or adapter["generator"] != "static-lora" or adapter["condition_lifetime"] != "global"
            or adapter["mergeable"] is not True or adapter["version"] != version):
        raise ValueError("typed adapter compatibility mismatch")


def _validate_typed_graph(path: Path, contract: Mapping[str, Any]) -> None:
    import onnx

    if path.stat().st_size > MAX_ARTIFACT_BYTES:
        raise ValueError("ONNX exceeds the 512 MiB artifact size limit")
    graph = onnx.load(path, load_external_data=False)
    if len(graph.opset_import) != 1 or graph.opset_import[0].domain not in ("", "ai.onnx") or graph.opset_import[0].version != 18 or graph.functions:
        raise ValueError("typed ONNX requires standard opset 18")
    if graph.graph.sparse_initializer or len(graph.graph.node) > 100_000:
        raise ValueError("unsupported sparse graph or ONNX node budget")
    actual = [*graph.graph.input, *graph.graph.output]
    expected = [*contract["inputs"], *contract["outputs"]]
    if len(actual) != len(expected):
        raise ValueError("typed ONNX input/output count mismatch")
    dtype_ids = {"float32": onnx.TensorProto.FLOAT, "int64": onnx.TensorProto.INT64, "bool": onnx.TensorProto.BOOL}
    for entry, item in zip(actual, expected, strict=True):
        if entry.name != item["name"] or entry.type.tensor_type.elem_type != dtype_ids[item["dtype"]]:
            raise ValueError("typed ONNX tensor name/dtype mismatch")
        dims = entry.type.tensor_type.shape.dim
        if len(dims) != len(item["shape"]):
            raise ValueError("typed ONNX tensor rank mismatch")
        for dim, required in zip(dims, item["shape"], strict=True):
            if isinstance(required, str) and dim.dim_param != required or type(required) is int and dim.dim_value != required:
                raise ValueError("typed ONNX shape/dynamic symbol mismatch")
    graph_inputs = {entry.name for entry in graph.graph.input}
    initializers = set()
    parameter_elements = 0
    for tensor in graph.graph.initializer:
        if not tensor.name or tensor.name in graph_inputs or tensor.name in initializers:
            raise ValueError("typed ONNX initializer shadows an input or is duplicated")
        initializers.add(tensor.name)
        if tensor.data_location == onnx.TensorProto.EXTERNAL or tensor.external_data:
            raise ValueError("external ONNX tensor data is forbidden")
        if tensor.data_type not in (onnx.TensorProto.FLOAT, onnx.TensorProto.UINT8, onnx.TensorProto.INT32, onnx.TensorProto.INT64, onnx.TensorProto.BOOL):
            raise ValueError("unsupported ONNX initializer dtype")
        if tensor.data_type == onnx.TensorProto.FLOAT and not np.isfinite(onnx.numpy_helper.to_array(tensor)).all():
            raise ValueError("non-finite ONNX weights")
        parameter_elements += math.prod(tensor.dims)
    if parameter_elements > 64_000_000:
        raise ValueError("model parameter budget exceeds 64 million")
    producers = {}
    for node in graph.graph.node:
        if node.domain not in ("", "ai.onnx") or not node.op_type:
            raise ValueError("unsupported ONNX node domain")
        for output in node.output:
            if output:
                if output in graph_inputs or output in initializers or output in producers:
                    raise ValueError("typed ONNX node output shadows another value")
                producers[output] = node
        for attribute in node.attribute:
            if attribute.HasField("g") or attribute.graphs or attribute.HasField("sparse_tensor") or attribute.sparse_tensors:
                raise ValueError("nested/sparse ONNX graph unsupported")
            if not math.isfinite(attribute.f) or any(not math.isfinite(value) for value in attribute.floats):
                raise ValueError("non-finite ONNX attribute")
    for output in ("policy_logits", "value"):
        pending, visited = [output], set()
        while pending:
            value = pending.pop()
            if value in visited:
                continue
            visited.add(value)
            if value in producers and producers[value].op_type not in ("Shape", "Size"):
                pending.extend(producers[value].input)
        if "condition" not in visited:
            raise ValueError("FiLM condition is disconnected from an output")
    onnx.checker.check_model(graph, full_check=True)


def export_typed_onnx(model: Any, spec: Any, directory: str | Path, sample_inputs: Mapping[str, Any], *,
                      architecture_family: str, descriptor: Any | None = None) -> Path:
    """Export a typed A/B model with its complete public input and graph contract."""
    from copy import deepcopy

    encoder = spec.contract()
    if spec.digest != _typed_hash(encoder):
        raise ValueError("typed encoder digest mismatch")
    if _typed_family(model) != architecture_family:
        raise ValueError("typed architecture family does not match the model")
    onnx_contract = _typed_io_contract(spec, architecture_family)
    names = [item["name"] for item in onnx_contract["inputs"]]
    if set(sample_inputs) != set(names):
        raise ValueError("sample inputs differ from typed feature schema")
    samples = []
    axes = {}
    symbols = {}
    for item in onnx_contract["inputs"]:
        name = item["name"]
        value = sample_inputs[name]
        if isinstance(value, np.ndarray):
            value = torch.from_numpy(value)
        if not isinstance(value, Tensor):
            raise TypeError(f"sample {name} must be a tensor")
        required_dtype = {"float32": torch.float32, "int64": torch.int64, "bool": torch.bool}[item["dtype"]]
        if value.dtype != required_dtype or value.ndim != len(item["shape"]):
            raise ValueError(f"sample {name} dtype/rank mismatch")
        samples.append(value.detach().cpu().contiguous())
        axes[name] = {}
        for axis, (actual, required) in enumerate(zip(value.shape, item["shape"], strict=True)):
            if isinstance(required, str):
                if required in symbols and symbols[required] != actual:
                    raise ValueError(f"sample shared axis {required} mismatch")
                symbols[required] = actual
                axes[name][axis] = required
            elif actual != required:
                raise ValueError(f"sample {name} fixed dimension mismatch")
    axes.update({"policy_logits": {0: "batch", 1: "actions"}, "value": {0: "batch"}})
    if any(value < 1 or value > _typed_resource_limits()["axis_maxima"][name] for name, value in symbols.items()):
        raise ValueError("sample dynamic shape exceeds resource limits")
    if descriptor is not None:
        _validate_typed_adapter_metadata(architecture_family, asdict(descriptor), model.base_hash,
                                         model.config.digest, spec.digest)
        exported = model.merged_copy(descriptor, spec.digest)
        adapter_hash = tensor_state_hash(model.adapter_state())
    else:
        if getattr(model, "training_mode", None) == "adapter" or getattr(model, "merged", False):
            raise ValueError("adapter export requires its compatibility descriptor")
        exported = deepcopy(model).cpu().eval()
        adapter_hash = None
    exported.cpu().eval()
    if hasattr(exported, "validate_inputs"):
        exported.validate_inputs(*samples)
    output_directory = Path(directory)
    output_directory.mkdir(parents=True, exist_ok=True)
    output_path = output_directory / "model.onnx"
    torch.onnx.export(exported, tuple(samples), output_path, dynamo=False, opset_version=18,
                      input_names=names, output_names=["policy_logits", "value"],
                      dynamic_axes=axes, external_data=False)
    _validate_typed_graph(output_path, onnx_contract)
    config = asdict(model.config)
    config_hash = _typed_hash(config)
    if descriptor is not None and (descriptor.base_hash, descriptor.config_hash, descriptor.encoder_hash) != (model.base_hash, config_hash, spec.digest):
        raise ValueError("typed adapter descriptor compatibility mismatch")
    manifest = {
        "version": TYPED_BUNDLE_VERSION, "model_io_version": "typed-policy-value-v1",
        "architecture_family": architecture_family, "model_file": "model.onnx",
        "model_sha256": file_sha256(output_path), "base_hash": model.base_hash,
        "adapter_hash": adapter_hash, "adapter": asdict(descriptor) if descriptor else None,
        "model_config": config, "model_config_hash": config_hash,
        "encoder": encoder, "encoder_hash": spec.digest,
        "onnx": onnx_contract, "resource_limits": _typed_resource_limits(),
        "numerical_tolerance": {"atol": 1e-5, "rtol": 1e-4},
    }
    manifest_path = output_directory / "manifest.json"
    manifest_path.write_text(canonical_json(manifest) + "\n", encoding="utf-8")
    return manifest_path


def _validate_typed_manifest(manifest: dict[str, Any], path: Path, expected_spec: Any | None,
                             *, inspect_graph: bool) -> dict[str, Any]:
    required = {"version", "model_io_version", "architecture_family", "model_file", "model_sha256",
                "base_hash", "adapter_hash", "adapter", "model_config", "model_config_hash", "encoder",
                "encoder_hash", "onnx", "resource_limits", "numerical_tolerance"}
    if set(manifest) != required or manifest["model_io_version"] != "typed-policy-value-v1" or manifest["model_file"] != "model.onnx":
        raise ValueError("unsupported typed deployment manifest")
    family = manifest["architecture_family"]
    if family not in ("mask-resnet", "entity-transformer"):
        raise ValueError("unsupported typed architecture family")
    encoder, config = manifest["encoder"], manifest["model_config"]
    if (not isinstance(encoder, dict) or set(encoder) != {"rules_version", "catalog_version", "catalog_hash", "observation_policy_hash",
            "observation_version", "ir_version", "descriptor_version", "encoder_version", "feature_schema", "feature_schema_hash", "value_perspective"}
            or not isinstance(config, dict)):
        raise ValueError("invalid typed encoder/model metadata")
    if (_typed_hash(encoder) != manifest["encoder_hash"] or _typed_hash(config) != manifest["model_config_hash"]
            or _typed_hash(encoder["feature_schema"]) != encoder["feature_schema_hash"]
            or expected_spec is not None and expected_spec.digest != manifest["encoder_hash"]):
        raise ValueError("typed encoder/model contract hash mismatch")
    if (encoder["rules_version"] != "augment-site-20260928-e5ed84fcf8e72a24"
            or encoder["ir_version"] != "semantic-ir-v1" or encoder["descriptor_version"] != "move-program-v1"
            or encoder["encoder_version"] != "typed-input-v1" or encoder["value_perspective"] != "observation.viewer"):
        raise ValueError("unsupported typed encoder semantics")
    catalog, policy = _typed_frozen_source()
    if (encoder["catalog_version"] != catalog["catalogVersion"] or encoder["catalog_hash"] != _typed_hash(catalog)
            or encoder["observation_policy_hash"] != _typed_hash(policy)):
        raise ValueError("frozen v7 rules/catalog compatibility mismatch")
    from ..ir import TypedEncoderSpec

    schema = encoder["feature_schema"]
    if not isinstance(schema, dict) or not isinstance(schema.get("limits"), dict):
        raise ValueError("typed feature schema limits missing")
    source_spec = TypedEncoderSpec.from_catalog(
        catalog, observation_policy=policy, observation_version=encoder["observation_version"],
        **schema["limits"])
    if encoder != source_spec.contract():
        raise ValueError("typed feature schema differs from frozen source and versioned encoder")
    if config.get("architecture_version") != {"mask-resnet": "mask-resnet-v2",
                                               "entity-transformer": "entity-transformer-film-lora-v1"}[family]:
        raise ValueError("model architecture version/family mismatch")
    if manifest["onnx"] != _typed_io_contract(expected_spec if expected_spec is not None else _ManifestTypedSpec(encoder), family):
        raise ValueError("typed ONNX IO contract mismatch")
    if manifest["resource_limits"] != _typed_resource_limits() or manifest["numerical_tolerance"] != {"atol": 1e-5, "rtol": 1e-4}:
        raise ValueError("unsupported typed resource/numerical contract")
    if not all(_valid_hash(manifest[key]) for key in ("model_sha256", "base_hash", "model_config_hash", "encoder_hash")):
        raise ValueError("invalid typed deployment hash")
    adapter, adapter_hash = manifest["adapter"], manifest["adapter_hash"]
    if adapter is None:
        if adapter_hash is not None:
            raise ValueError("typed adapter metadata/hash mismatch")
    else:
        if not _valid_hash(adapter_hash):
            raise ValueError("typed adapter metadata/hash mismatch")
        _validate_typed_adapter_metadata(family, adapter, manifest["base_hash"],
                                         manifest["model_config_hash"], manifest["encoder_hash"])
    model_path = path.parent / "model.onnx"
    if model_path.stat().st_size > MAX_ARTIFACT_BYTES or file_sha256(model_path) != manifest["model_sha256"]:
        raise ValueError("typed ONNX model size/hash mismatch")
    if inspect_graph:
        _validate_typed_graph(model_path, manifest["onnx"])
    return manifest


class _ManifestTypedSpec:
    def __init__(self, metadata: dict[str, Any]):
        self.metadata = metadata

    def contract(self) -> dict[str, Any]:
        return self.metadata


TYPED_CHECKPOINT_VERSION = "typed-model-checkpoint-v1"


def _typed_family(model: Any) -> str:
    from .mask_resnet import MaskResNetPolicyValueNetwork
    from .entity_transformer import EntityTransformer

    if isinstance(model, MaskResNetPolicyValueNetwork):
        return "mask-resnet"
    if isinstance(model, EntityTransformer):
        return "entity-transformer"
    raise TypeError("typed checkpoint requires the mask ResNet or entity Transformer")


def _typed_frozen_source() -> tuple[dict[str, Any], dict[str, Any]]:
    """Read source checkout metadata or the same frozen copies in an installed wheel."""
    root = Path(__file__).resolve().parents[3] / "bridge" / "catalog"
    catalog_path = root / "site-20260928.json"
    policy_path = root / "observation-20260928.json"
    if catalog_path.is_file() and policy_path.is_file():
        return (json.loads(catalog_path.read_text(encoding="utf-8")),
                json.loads(policy_path.read_text(encoding="utf-8")))
    from accelerate_chess import site_catalog, site_observation_policy

    rules_version = "augment-site-20260928-e5ed84fcf8e72a24"
    return site_catalog(rules_version), site_observation_policy(rules_version)


def _typed_source_spec(data: Mapping[str, Any]) -> Any:
    from ..ir import TypedEncoderSpec

    catalog, policy = _typed_frozen_source()
    return TypedEncoderSpec.from_dict(data, catalog=catalog, observation_policy=policy)


def _typed_model_from_config(family: str, config: Mapping[str, Any]) -> Any:
    from .typed_context import TypedContextConfig
    from .mask_resnet import MaskResNetConfig, MaskResNetPolicyValueNetwork
    from .entity_transformer import EntityTransformer, EntityTransformerConfig

    data = dict(config)
    if family == "mask-resnet":
        field = "typed_context"
        config_class, model_class = MaskResNetConfig, MaskResNetPolicyValueNetwork
    elif family == "entity-transformer":
        field = "typed"
        config_class, model_class = EntityTransformerConfig, EntityTransformer
    else:
        raise ValueError("unsupported typed checkpoint architecture family")
    context = dict(data[field])
    for name in ("record_category_sizes", "relation_category_sizes", "candidate_category_sizes"):
        context[name] = tuple(context[name])
    data[field] = TypedContextConfig(**context)
    return model_class(config_class(**data))


def _typed_check_model(model: Any, spec: Any) -> tuple[str, dict[str, Any], str]:
    family = _typed_family(model)
    metadata = spec.contract()
    if spec.digest != _typed_hash(metadata) or metadata["rules_version"] != "augment-site-20260928-e5ed84fcf8e72a24":
        raise ValueError("typed checkpoint encoder compatibility mismatch")
    vocabulary_size = len(metadata["feature_schema"]["category_vocabulary"])
    context = model.config.typed_context if family == "mask-resnet" else model.config.typed
    if any(size != vocabulary_size for size in (*context.record_category_sizes,
                                                   *context.relation_category_sizes,
                                                   *context.candidate_category_sizes)):
        raise ValueError("typed model category embeddings differ from encoder vocabulary")
    state = model.state_dict()
    if any(tensor.is_floating_point() and (tensor.dtype != torch.float32 or not bool(torch.isfinite(tensor).all()))
           for tensor in state.values()):
        raise ValueError("typed model requires finite float32 weights and buffers")
    config = asdict(model.config)
    return family, config, _typed_hash(config)


def _load_typed_checkpoint(path: str | Path, kind: str) -> dict[str, Any]:
    path = Path(path)
    if path.stat().st_size > MAX_ARTIFACT_BYTES:
        raise ValueError("typed checkpoint exceeds the 512 MiB artifact limit")
    payload = torch.load(path, map_location="cpu", weights_only=True, mmap=True)
    if not isinstance(payload, dict) or payload.get("version") != TYPED_CHECKPOINT_VERSION or payload.get("kind") != kind:
        raise ValueError("unsupported typed checkpoint format")
    return payload


def save_typed_base(model: Any, spec: Any, path: str | Path) -> str:
    family, config, config_hash = _typed_check_model(model, spec)
    if model.merged:
        raise ValueError("merged deployment copy cannot overwrite a typed base checkpoint")
    state = _copy_state(model.base_state())
    base_hash = tensor_state_hash(state)
    if base_hash != model.base_hash:
        raise ValueError("typed base hash differs from model state")
    _atomic_torch_save({"version": TYPED_CHECKPOINT_VERSION, "kind": "base",
                        "architecture_family": family, "config": config, "model_config_hash": config_hash,
                        "encoder_spec": spec.to_dict(), "encoder_hash": spec.digest,
                        "base_hash": base_hash, "state": state}, Path(path))
    return base_hash


def load_typed_base(path: str | Path, expected_spec: Any | None = None) -> tuple[Any, Any]:
    payload = _load_typed_checkpoint(path, "base")
    if set(payload) != {"version", "kind", "architecture_family", "config", "model_config_hash",
                        "encoder_spec", "encoder_hash", "base_hash", "state"}:
        raise ValueError("invalid typed base checkpoint fields")
    spec = _typed_source_spec(payload["encoder_spec"])
    if spec.digest != payload["encoder_hash"] or expected_spec is not None and expected_spec.digest != spec.digest:
        raise ValueError("typed base encoder compatibility mismatch")
    model = _typed_model_from_config(payload["architecture_family"], payload["config"])
    _, _, config_hash = _typed_check_model(model, spec)
    if config_hash != payload["model_config_hash"] or not _valid_hash(payload["base_hash"]):
        raise ValueError("typed base architecture/hash mismatch")
    state = _validate_state(payload["state"], model.base_state())
    if tensor_state_hash(state) != payload["base_hash"]:
        raise ValueError("typed base state hash mismatch")
    model.load_state_dict({**model.state_dict(), **state}, strict=True)
    model.eval()
    return model, spec


def save_typed_adapter(model: Any, spec: Any, path: str | Path, descriptor: Any | None = None) -> Any:
    family, _, config_hash = _typed_check_model(model, spec)
    if model.merged:
        raise ValueError("merged model cannot save a separate typed adapter")
    descriptor = descriptor or model.adapter_descriptor(spec.digest)
    descriptor.validate_merge()
    if (descriptor.base_hash, descriptor.config_hash, descriptor.encoder_hash) != (model.base_hash, config_hash, spec.digest):
        raise ValueError("typed adapter compatibility mismatch")
    state = _copy_state(model.adapter_state())
    _atomic_torch_save({"version": TYPED_CHECKPOINT_VERSION, "kind": "adapter",
                        "architecture_family": family, "descriptor": asdict(descriptor),
                        "adapter_hash": tensor_state_hash(state), "state": state}, Path(path))
    return descriptor


def load_typed_adapter(model: Any, spec: Any, path: str | Path) -> Any:
    from .model import AdapterDescriptor
    from .entity_transformer import TransformerAdapterDescriptor

    family, _, config_hash = _typed_check_model(model, spec)
    if model.merged:
        raise ValueError("cannot load a typed adapter into a merged model")
    payload = _load_typed_checkpoint(path, "adapter")
    if set(payload) != {"version", "kind", "architecture_family", "descriptor", "adapter_hash", "state"} or payload["architecture_family"] != family:
        raise ValueError("typed adapter checkpoint family/fields mismatch")
    descriptor_class = AdapterDescriptor if family == "mask-resnet" else TransformerAdapterDescriptor
    descriptor = descriptor_class(**payload["descriptor"])
    descriptor.validate_merge()
    if (descriptor.base_hash, descriptor.config_hash, descriptor.encoder_hash) != (model.base_hash, config_hash, spec.digest):
        raise ValueError("typed adapter does not match base, architecture, and encoder")
    state = _validate_state(payload["state"], model.adapter_state())
    if tensor_state_hash(state) != payload["adapter_hash"]:
        raise ValueError("typed adapter state hash mismatch")
    model.load_state_dict({**model.state_dict(), **state}, strict=True)
    model.configure_training("adapter")
    model.eval()
    return descriptor
