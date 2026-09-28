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

    for initializer in graph.graph.initializer:
        validate_tensor(initializer)
    producers = {}
    for node in graph.graph.node:
        if node.domain not in ("", "ai.onnx") or not node.op_type:
            raise ValueError("unsupported ONNX node domain")
        for output in node.output:
            if output in producers:
                raise ValueError("duplicate ONNX producer")
            producers[output] = node
        for attribute in node.attribute:
            if attribute.HasField("g") or attribute.graphs or not math.isfinite(attribute.f) or any(not math.isfinite(value) for value in attribute.floats):
                raise ValueError("nested graph or non-finite ONNX attribute unsupported")
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
