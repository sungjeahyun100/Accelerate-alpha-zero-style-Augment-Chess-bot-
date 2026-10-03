"""Finite policy/value optimization and validated deterministic resume state.

These entrypoints implement training; ordinary verification runs use only tiny
synthetic optimizer steps. Replay files stream through one bounded episode
cache. Unfinished episodes never enter the supervised target index.
"""
from __future__ import annotations

from copy import deepcopy
from dataclasses import asdict, dataclass
import hashlib
import math
import os
from pathlib import Path
import random
import tempfile
from threading import Lock
import time
from typing import Any, Callable, Sequence
from weakref import WeakValueDictionary

import numpy as np
import torch

from .encoding import EncoderSpec, PublicEncoder, batch_positions, canonical_json
from .network.artifacts import MAX_ARTIFACT_BYTES, _validate_state
from .network.model import PolicyValueNetwork, tensor_state_hash
from .replay import MAX_REPLAY_BYTES, ReplayEpisode, TrainingExample

TRAINING_VERSION = "accelerate-training-checkpoint-v1"
TYPED_TRAINING_VERSION = "accelerate-training-checkpoint-v2"

_checkpoint_commit_locks_guard = Lock()
_checkpoint_commit_locks = WeakValueDictionary()


def _checkpoint_commit_lock(path: Path):
    # Keep the registry short-lived while callers retain their lock through commit.
    destination = os.path.normcase(os.fspath(path.parent.resolve() / path.name))
    with _checkpoint_commit_locks_guard:
        lock = _checkpoint_commit_locks.get(destination)
        if lock is None:
            lock = Lock()
            _checkpoint_commit_locks[destination] = lock
        return lock


def _sha(path):
    digest = hashlib.sha256()
    total = 0
    with Path(path).open("rb") as file:
        while chunk := file.read(min(1 << 20, MAX_REPLAY_BYTES - total + 1)):
            total += len(chunk)
            if total > MAX_REPLAY_BYTES:
                raise ValueError("replay input exceeds the 16 MiB storage boundary")
            digest.update(chunk)
    return digest.hexdigest()


class ReplayDataset:
    """Index references, retaining at most one decoded episode at a time."""
    def __init__(self, paths: Sequence[str | Path], spec: EncoderSpec, *, architecture_family: str | None = None,
                 max_files: int = 4096, max_bytes: int = 1_073_741_824, max_examples: int = 1_000_000):
        from .ir import TypedEncoderSpec

        if not paths or len(paths) > max_files or not 1 <= max_files <= 4096 or not 1 <= max_examples <= 1_000_000 or not 1 <= max_bytes <= 1_073_741_824:
            raise ValueError("dataset needs explicit finite replay inputs and limits")
        typed = isinstance(spec, TypedEncoderSpec)
        if (typed and architecture_family not in (None, "mask-resnet", "entity-transformer")
                or not typed and architecture_family is not None):
            raise ValueError("dataset model family differs from the encoder contract")
        self.spec = spec
        self.architecture_family = architecture_family
        self.paths = tuple(Path(path).resolve() for path in paths)
        if len(set(self.paths)) != len(self.paths) or sum(path.stat().st_size for path in self.paths) > max_bytes:
            raise ValueError("duplicate replay inputs or dataset byte budget exceeded")
        self._hashes = tuple(_sha(path) for path in self.paths)
        self.index: list[tuple[int, int]] = []
        for file_index, path in enumerate(self.paths):
            episode = ReplayEpisode.load(path, spec)
            if typed:
                if self.architecture_family is None:
                    self.architecture_family = episode.architecture_family
                elif episode.architecture_family != self.architecture_family:
                    raise ValueError("typed replay architecture differs from the dataset family")
            if episode.outcome["status"] != "terminal":
                continue
            for decision in range(len(episode.decisions)):
                if len(self.index) >= max_examples:
                    raise ValueError("dataset target count exceeds its finite limit")
                self.index.append((file_index, decision))
        if not self.index:
            raise ValueError("dataset contains no terminal policy/value targets; unfinished episodes were excluded")
        identity = {"files": self._hashes, "index": self.index, "encoder": spec.digest}
        if typed:
            identity["architecture_family"] = self.architecture_family
        self.digest = hashlib.sha256(canonical_json(identity).encode()).hexdigest()
        self._cached_file = -1
        self._cached_episode = None

    def __len__(self):
        return len(self.index)

    def __getitem__(self, index: int) -> TrainingExample:
        file_index, decision = self.index[index]
        if file_index != self._cached_file:
            if _sha(self.paths[file_index]) != self._hashes[file_index]:
                raise ValueError("replay input changed after the dataset cursor was created")
            self._cached_episode = ReplayEpisode.load(self.paths[file_index], self.spec)
            self._cached_file = file_index
        episode = self._cached_episode
        record = episode.decisions[decision]
        winner = episode.outcome["winner"]
        return TrainingExample(episode.trackers[record["actor"]].frame_at(record["trace_step"]),
            tuple(candidate["intent"] for candidate in record["candidates"]),
            tuple(candidate["probability"] for candidate in record["candidates"]),
            0. if winner == "draw" else (1. if winner == record["actor"] else -1.), record["actor"], record["belief_summary"])


class DatasetCursor:
    def __init__(self, dataset, seed: int):
        if not len(dataset) or type(seed) is not int or not 0 <= seed < 2**64:
            raise ValueError("shuffle cursor requires a nonempty dataset and bounded independent seed")
        self.dataset = dataset
        self.rng = np.random.default_rng(seed)
        self.order = self.rng.permutation(len(dataset)).tolist()
        self.offset = self.epoch = 0

    def next_batch(self, count: int):
        if type(count) is not int or not 1 <= count <= 64:
            raise ValueError("training batches must contain 1..64 examples")
        # A replay may become unreadable after the index was built. Do not
        # consume a shuffle position until the whole requested batch is read.
        before = (self.order, self.offset, self.epoch, deepcopy(self.rng.bit_generator.state))
        try:
            indices = []
            for _ in range(count):
                if self.offset == len(self.order):
                    self.order = self.rng.permutation(len(self.dataset)).tolist()
                    self.offset = 0
                    self.epoch += 1
                indices.append(self.order[self.offset])
                self.offset += 1
            return [self.dataset[index] for index in indices]
        except Exception:
            self.order, self.offset, self.epoch = before[:3]
            self.rng.bit_generator.state = before[3]
            raise

    def snapshot(self):
        state = deepcopy(self.rng.bit_generator.state)
        # PCG64 integers exceed JSON's exact double range. Decimal strings are
        # explicit state serialization, not rounded JCS numeric values.
        for key in ("state", "inc"):
            state["state"][key] = str(state["state"][key])
        return {"dataset_hash": self.dataset.digest, "order": self.order.copy(), "offset": self.offset,
                "epoch": self.epoch, "shuffle_state": state}

    def validate(self, snapshot):
        if not isinstance(snapshot, dict) or set(snapshot) != {"dataset_hash", "order", "offset", "epoch", "shuffle_state"} or snapshot["dataset_hash"] != self.dataset.digest:
            raise ValueError("checkpoint dataset identity mismatch")
        order = snapshot["order"]
        if not isinstance(order, list) or len(order) != len(self.dataset) or any(type(item) is not int for item in order) or sorted(order) != list(range(len(self.dataset))) or type(snapshot["offset"]) is not int or not 0 <= snapshot["offset"] <= len(order) or type(snapshot["epoch"]) is not int or snapshot["epoch"] < 0:
            raise ValueError("invalid checkpoint dataset cursor/order")
        state = deepcopy(snapshot["shuffle_state"])
        if not isinstance(state, dict) or state.get("bit_generator") != "PCG64":
            raise ValueError("checkpoint shuffle generator differs")
        try:
            for key in ("state", "inc"):
                if not isinstance(state["state"][key], str) or not state["state"][key].isdigit():
                    raise ValueError("invalid decimal shuffle state")
                state["state"][key] = int(state["state"][key])
            generator = np.random.PCG64()
            generator.state = state
        except (KeyError, TypeError, ValueError, OverflowError) as error:
            raise ValueError("invalid checkpoint shuffle state") from error
        return state

    def restore(self, snapshot):
        state = self.validate(snapshot)
        self.order, self.offset, self.epoch = snapshot["order"].copy(), snapshot["offset"], snapshot["epoch"]
        self.rng.bit_generator.state = state


@dataclass(frozen=True)
class TrainingLimits:
    steps: int = 1
    batch_size: int = 2
    elapsed_ms: int = 60_000
    max_parameter_state_bytes: int = 1_073_741_824
    gradient_norm: float = 5.

    def __post_init__(self):
        for name, upper in (("steps", 1_000_000), ("batch_size", 64), ("elapsed_ms", 86_400_000), ("max_parameter_state_bytes", 8_589_934_592)):
            value = getattr(self, name)
            if type(value) is not int or not 1 <= value <= upper:
                raise ValueError(f"training {name} must have a finite positive limit")
        if not math.isfinite(self.gradient_norm) or not 0 < self.gradient_norm <= 1000:
            raise ValueError("gradient clipping norm must be finite and positive")


def create_optimizer(model: PolicyValueNetwork, *, mode: str, learning_rate: float = 3e-4, weight_decay: float = .01):
    if not math.isfinite(learning_rate) or not 0 < learning_rate <= 1 or not math.isfinite(weight_decay) or not 0 <= weight_decay <= 1:
        raise ValueError("AdamW learning rate/weight decay is invalid")
    model.configure_training(mode)
    return torch.optim.AdamW([parameter for parameter in model.parameters() if parameter.requires_grad], lr=learning_rate, weight_decay=weight_decay)


def optimize(model, optimizer, encoder: PublicEncoder, cursor: DatasetCursor, *, limits: TrainingLimits = TrainingLimits(), cancelled: Callable[[], bool] | None = None, on_step: Callable[[int], None] | None = None):
    from .ir import TypedEncoder
    from .network.mask_resnet import MaskResNetPolicyValueNetwork
    from .network.entity_transformer import EntityTransformer

    if not isinstance(optimizer, torch.optim.AdamW) or encoder.spec.digest != cursor.dataset.spec.digest:
        raise ValueError("optimization requires matching model/dataset contracts and AdamW")
    typed = isinstance(encoder, TypedEncoder)
    if typed:
        if isinstance(model, MaskResNetPolicyValueNetwork):
            family, context = "mask-resnet", model.config.typed_context
            if model.config.board_channels != len(encoder.spec.feature_schema["spatial_channels"]):
                raise ValueError("typed ResNet spatial channels differ from the encoder")
        elif isinstance(model, EntityTransformer):
            family, context = "entity-transformer", model.config.typed
        else:
            raise ValueError("typed optimization needs a typed A or B model")
        if cursor.dataset.architecture_family != family:
            raise ValueError("typed dataset teacher family differs from the trained model")
        vocabulary = len(encoder.spec.category_vocabulary)
        if (context.record_category_sizes, context.relation_category_sizes, context.candidate_category_sizes) != ((vocabulary,) * 4, (vocabulary,) * 2, (vocabulary,) * 4):
            raise ValueError("typed model category vocabulary differs from the encoder")
    elif not isinstance(encoder, PublicEncoder) or not isinstance(model, PolicyValueNetwork) or (model.config.board_channels, model.config.condition_dim, model.config.action_dim) != (encoder.spec.board_channels, encoder.spec.condition_dim, encoder.spec.action_dim):
        raise ValueError("optimizer model feature dimensions differ")
    if {id(parameter) for group in optimizer.param_groups for parameter in group["params"]} != {id(parameter) for parameter in model.parameters() if parameter.requires_grad}:
        raise ValueError("optimizer parameters differ from the base/adapter training mode")
    model_bytes = sum(tensor.numel() * tensor.element_size() for tensor in model.state_dict().values())
    trainable = sum(parameter.numel() * parameter.element_size() for parameter in model.parameters() if parameter.requires_grad)
    if model_bytes + 3 * trainable > limits.max_parameter_state_bytes:
        raise ValueError("training parameter/gradient/AdamW state budget exceeded before optimization")
    cancelled = cancelled or (lambda: False)
    start = time.monotonic()
    model.train()
    completed = 0
    metrics = []
    reason = "steps"
    for _ in range(limits.steps):
        was_cancelled = cancelled()
        if was_cancelled or (time.monotonic() - start) * 1000 >= limits.elapsed_ms:
            reason = "cancelled" if was_cancelled else "elapsed"
            break
        examples = cursor.next_batch(limits.batch_size)
        if typed:
            from .ir import ObservationIR, batch_typed_positions

            encoded = batch_typed_positions([encoder.encode(ObservationIR.from_public(example.observation,
                encoder.spec, belief_summary=example.belief_summary), example.intents) for example in examples])
            arrays = encoded.as_family_inputs(family)
            action_mask = encoded.candidate_mask
        else:
            model.config.validate_working_set(len(examples), max(1, max(len(example.intents) for example in examples)))
            encoded = batch_positions([encoder.encode(example.observation, example.intents,
                belief_summary=example.belief_summary) for example in examples])
            arrays = (encoded.board, encoded.condition, encoded.action_features)
            action_mask = encoded.action_mask
        tensors = tuple(torch.from_numpy(array).to(next(model.parameters()).device) for array in arrays)
        model.validate_inputs(*tensors)
        policy = torch.zeros(action_mask.shape, device=tensors[0].device)
        for row, example in enumerate(examples):
            if len(example.policy) != len(example.intents) or not np.isfinite(example.policy).all() or any(probability < 0 for probability in example.policy) or abs(sum(example.policy) - 1) > 1e-6 or example.value not in (-1., 0., 1.) or example.actor != example.observation["viewer"]:
                raise ValueError("training policy/value target is invalid")
            policy[row, :len(example.policy)] = torch.tensor(example.policy, device=policy.device)
        targets = torch.tensor([[example.value] for example in examples], device=policy.device)
        optimizer.zero_grad(set_to_none=True)
        logits, value = model(*tensors)
        mask = torch.from_numpy(action_mask).to(policy.device)
        log_policy = torch.log_softmax(logits.masked_fill(~mask, -torch.inf), dim=1)
        policy_loss = -(policy * log_policy.masked_fill(~mask, 0)).sum(dim=1).mean()
        value_loss = torch.nn.functional.mse_loss(value, targets)
        loss = policy_loss + value_loss
        if not bool(torch.isfinite(loss)):
            raise ValueError("non-finite policy/value loss")
        loss.backward()
        torch.nn.utils.clip_grad_norm_([parameter for parameter in model.parameters() if parameter.requires_grad], limits.gradient_norm, error_if_nonfinite=True)
        optimizer.step()
        if any(not bool(torch.isfinite(parameter).all()) for parameter in model.parameters()):
            raise ValueError("optimizer produced non-finite model parameters")
        completed += 1
        metrics.append({"policy_ce": float(policy_loss.detach()), "value_mse": float(value_loss.detach())})
        if len(metrics) > 128:
            metrics.pop(0)
        if on_step is not None:
            on_step(completed)
    return {"steps": completed, "stop_reason": reason, "metrics": metrics, "metrics_scope": "last-128-steps"}


def _tree_hash(value) -> str:
    digest = hashlib.sha256()
    tensor_bytes = nodes = 0
    def visit(item, depth=0):
        nonlocal tensor_bytes, nodes
        nodes += 1
        if depth > 64 or nodes > 1_000_000:
            raise ValueError("checkpoint metadata exceeds its traversal budget")
        if isinstance(item, torch.Tensor):
            tensor_bytes += item.numel() * item.element_size()
            if tensor_bytes > MAX_ARTIFACT_BYTES:
                raise ValueError("checkpoint tensor metadata exceeds its aggregate allocation budget")
            tensor = item.detach().cpu().contiguous()
            if tensor.is_floating_point() and not bool(torch.isfinite(tensor).all()):
                raise ValueError("checkpoint tensors must be finite")
            header = canonical_json([str(tensor.dtype), list(tensor.shape)]).encode()
            digest.update(b"tensor" + len(header).to_bytes(8, "big") + header)
            digest.update(tensor.reshape(-1).view(torch.uint8).numpy().tobytes())
        elif isinstance(item, dict):
            digest.update(b"dict" + len(item).to_bytes(8, "big"))
            for key in sorted(item, key=lambda value: (type(value).__name__, str(value))):
                visit(key, depth + 1)
                visit(item[key], depth + 1)
        elif isinstance(item, (tuple, list)):
            digest.update(b"sequence" + len(item).to_bytes(8, "big"))
            for child in item:
                visit(child, depth + 1)
        else:
            text = canonical_json(item).encode()
            digest.update(b"json" + len(text).to_bytes(8, "big") + text)
    visit(value)
    return digest.hexdigest()


def _rng_snapshot():
    legacy = np.random.get_state()
    return {"python": random.getstate(), "numpy": (legacy[0], legacy[1].tolist(), legacy[2], legacy[3], legacy[4]),
            "torch_cpu": torch.get_rng_state(), "torch_cuda": torch.cuda.get_rng_state_all() if torch.cuda.is_available() else []}


def _rng_validate(snapshot):
    if not isinstance(snapshot, dict) or set(snapshot) != {"python", "numpy", "torch_cpu", "torch_cuda"}:
        raise ValueError("checkpoint RNG state fields mismatch")
    random.Random().setstate(snapshot["python"])
    numpy_state = snapshot["numpy"]
    if not isinstance(numpy_state, tuple) or len(numpy_state) != 5:
        raise ValueError("invalid NumPy RNG checkpoint")
    np.random.RandomState().set_state((numpy_state[0], np.asarray(numpy_state[1], dtype=np.uint32), *numpy_state[2:]))
    cpu = snapshot["torch_cpu"]
    if not isinstance(cpu, torch.Tensor) or cpu.dtype != torch.uint8 or cpu.shape != torch.get_rng_state().shape:
        raise ValueError("invalid Torch CPU RNG checkpoint")
    torch.Generator().set_state(cpu)
    cuda = snapshot["torch_cuda"]
    if not isinstance(cuda, list) or (cuda and (not torch.cuda.is_available() or len(cuda) != torch.cuda.device_count())) or any(not isinstance(state, torch.Tensor) or state.dtype != torch.uint8 or state.ndim != 1 for state in cuda):
        raise ValueError("checkpoint CUDA RNG topology differs")
    return (numpy_state[0], np.asarray(numpy_state[1], dtype=np.uint32), *numpy_state[2:])


def _restore_rng(snapshot):
    numpy_state = _rng_validate(snapshot)
    random.setstate(snapshot["python"])
    np.random.set_state(numpy_state)
    torch.set_rng_state(snapshot["torch_cpu"])
    if snapshot["torch_cuda"]:
        torch.cuda.set_rng_state_all(snapshot["torch_cuda"])


def _optimizer_names(model, optimizer):
    names = {id(parameter): name for name, parameter in model.named_parameters()}
    return [[names[id(parameter)] for parameter in group["params"]] for group in optimizer.param_groups]


def _validate_optimizer(state, optimizer, model, names):
    if not isinstance(optimizer, torch.optim.AdamW) or names != _optimizer_names(model, optimizer) or not isinstance(state, dict) or set(state) != {"state", "param_groups"}:
        raise ValueError("checkpoint AdamW parameter identity differs")
    groups = state["param_groups"]
    if not isinstance(groups, list) or len(groups) != len(optimizer.param_groups):
        raise ValueError("checkpoint optimizer groups mismatch")
    shapes = {}
    for saved, current in zip(groups, optimizer.param_groups):
        if set(saved) != set(optimizer.state_dict()["param_groups"][0]) or len(saved["params"]) != len(current["params"]):
            raise ValueError("checkpoint optimizer group fields/size mismatch")
        if not 0 < saved["lr"] <= 1 or not math.isfinite(saved["lr"]) or not 0 <= saved["weight_decay"] <= 1 or not math.isfinite(saved["weight_decay"]) or saved["eps"] <= 0 or not math.isfinite(saved["eps"]) or not isinstance(saved["betas"], (tuple, list)) or len(saved["betas"]) != 2 or any(not math.isfinite(beta) or not 0 <= beta < 1 for beta in saved["betas"]):
            raise ValueError("invalid checkpoint AdamW hyperparameters")
        for flag in ("amsgrad", "maximize", "foreach", "capturable", "differentiable", "fused", "decoupled_weight_decay"):
            if flag in current and saved[flag] != current[flag]:
                raise ValueError("checkpoint AdamW execution settings differ")
        for number, parameter in zip(saved["params"], current["params"]):
            if type(number) is not int or number in shapes:
                raise ValueError("duplicate/invalid checkpoint optimizer parameter")
            shapes[number] = parameter.shape
    for number, values in state["state"].items():
        if number not in shapes or not isinstance(values, dict) or set(values) != {"step", "exp_avg", "exp_avg_sq"}:
            raise ValueError("invalid checkpoint optimizer state")
        for key, tensor in values.items():
            if not isinstance(tensor, torch.Tensor) or tensor.dtype != torch.float32 or tensor.shape != (torch.Size([]) if key == "step" else shapes[number]) or not bool(torch.isfinite(tensor).all()) or (key == "step" and (tensor.item() < 0 or not tensor.item().is_integer())):
                raise ValueError("checkpoint optimizer tensor shape/value mismatch")


def _typed_model_family(model):
    from .network.mask_resnet import MaskResNetPolicyValueNetwork
    from .network.entity_transformer import EntityTransformer

    if isinstance(model, MaskResNetPolicyValueNetwork):
        return "mask-resnet"
    if isinstance(model, EntityTransformer):
        return "entity-transformer"
    raise ValueError("typed checkpoint needs a typed A or B model")


def save_training_checkpoint(model, optimizer, spec: EncoderSpec, cursor: DatasetCursor, path, *, completed_steps: int):
    from .ir import TypedEncoderSpec

    if type(completed_steps) is not int or completed_steps < 0 or cursor.dataset.spec.digest != spec.digest or model.merged:
        raise ValueError("invalid training checkpoint progress/contract")
    typed = isinstance(spec, TypedEncoderSpec)
    if not typed and not isinstance(spec, EncoderSpec):
        raise TypeError("training checkpoint needs a supported encoder spec")
    if not typed and not isinstance(model, PolicyValueNetwork):
        raise TypeError("legacy training checkpoint needs the legacy ResNet model")
    base, adapter = model.base_state(), model.adapter_state()
    state = {"version": TYPED_TRAINING_VERSION if typed else TRAINING_VERSION,
             "mode": model.training_mode, "training": model.training,
             "config": asdict(model.config), "encoder": spec.to_dict(), "encoder_hash": spec.digest,
             "base": {name: tensor.detach().cpu().clone() for name, tensor in base.items()},
             "adapter": {name: tensor.detach().cpu().clone() for name, tensor in adapter.items()},
             "base_hash": tensor_state_hash(base), "adapter_hash": tensor_state_hash(adapter),
             "optimizer": deepcopy(optimizer.state_dict()), "optimizer_names": _optimizer_names(model, optimizer),
             "rng": _rng_snapshot(), "cursor": cursor.snapshot(), "completed_steps": completed_steps}
    if typed:
        state.update({"architecture_family": _typed_model_family(model),
                      "feature_schema_hash": spec.feature_schema_hash,
                      "model_io_version": "typed-policy-value-v1"})
    _validate_optimizer(state["optimizer"], optimizer, model, state["optimizer_names"])
    state["checkpoint_hash"] = _tree_hash(state)
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=f".{path.name}.", suffix=".tmp", delete=False) as output:
            temporary = Path(output.name)
        torch.save(state, temporary)
        if temporary.stat().st_size > MAX_ARTIFACT_BYTES:
            raise ValueError("training checkpoint exceeds the 512 MiB artifact budget")
        with temporary.open("rb+") as saved:
            os.fsync(saved.fileno())
        with _checkpoint_commit_lock(path):
            os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    return state["checkpoint_hash"]


def load_training_checkpoint(model, optimizer, spec: EncoderSpec, cursor: DatasetCursor, path) -> int:
    from .ir import TypedEncoderSpec

    path = Path(path)
    if path.stat().st_size > MAX_ARTIFACT_BYTES:
        raise ValueError("training checkpoint exceeds the artifact budget")
    payload = torch.load(path, map_location="cpu", weights_only=True, mmap=True)
    fields = {"version", "mode", "training", "config", "encoder", "encoder_hash", "base", "adapter", "base_hash", "adapter_hash", "optimizer", "optimizer_names", "rng", "cursor", "completed_steps", "checkpoint_hash"}
    typed = isinstance(spec, TypedEncoderSpec)
    if not typed and not isinstance(spec, EncoderSpec):
        raise TypeError("training checkpoint needs a supported encoder spec")
    if not typed and not isinstance(model, PolicyValueNetwork):
        raise TypeError("legacy training checkpoint needs the legacy ResNet model")
    if typed:
        fields.update({"architecture_family", "feature_schema_hash", "model_io_version"})
    if not isinstance(payload, dict) or set(payload) != fields or payload["version"] != (TYPED_TRAINING_VERSION if typed else TRAINING_VERSION):
        raise ValueError("training checkpoint format mismatch")
    if typed:
        if (payload["encoder"] != spec.to_dict() or payload["architecture_family"] != _typed_model_family(model)
                or payload["feature_schema_hash"] != spec.feature_schema_hash
                or payload["model_io_version"] != "typed-policy-value-v1"):
            raise ValueError("typed training checkpoint model/encoder provenance mismatch")
    else:
        saved_spec = EncoderSpec.from_dict(payload["encoder"], observation_policy=spec.observation_policy)
        if saved_spec.digest != spec.digest:
            raise ValueError("training checkpoint encoder compatibility mismatch")
    if payload["encoder_hash"] != spec.digest or payload["config"] != asdict(model.config) or payload["mode"] != model.training_mode or type(payload["training"]) is not bool or type(payload["completed_steps"]) is not int or payload["completed_steps"] < 0:
        raise ValueError("training checkpoint mode/model/encoder compatibility mismatch")
    if payload["mode"] == "adapter" and payload["base_hash"] != model.base_hash:
        raise ValueError("adapter training checkpoint cannot replace a different frozen base")
    base = _validate_state(payload["base"], model.base_state())
    adapter = _validate_state(payload["adapter"], model.adapter_state())
    if tensor_state_hash(base) != payload["base_hash"] or tensor_state_hash(adapter) != payload["adapter_hash"]:
        raise ValueError("training checkpoint base/adapter hash mismatch")
    _validate_optimizer(payload["optimizer"], optimizer, model, payload["optimizer_names"])
    _rng_validate(payload["rng"])
    cursor.validate(payload["cursor"])
    # Validate expected tensor shapes before hashing: torch serialization can
    # describe an enormous zero-stride view backed by a tiny file. Hashing such
    # an unchecked view first would allocate it through contiguous().
    if payload["checkpoint_hash"] != _tree_hash({key: value for key, value in payload.items() if key != "checkpoint_hash"}):
        raise ValueError("training checkpoint content hash mismatch")
    # All contracts validate before the first mutation. Roll back if a library
    # operation fails while committing the otherwise validated resume state.
    before = ({name: tensor.detach().clone() for name, tensor in model.state_dict().items()}, deepcopy(optimizer.state_dict()), cursor.snapshot(), _rng_snapshot(), model.training)
    try:
        model.load_state_dict({**base, **adapter}, strict=True)
        optimizer.load_state_dict(payload["optimizer"])
        cursor.restore(payload["cursor"])
        _restore_rng(payload["rng"])
        model.train(payload["training"])
    except Exception:
        model.load_state_dict(before[0], strict=True)
        optimizer.load_state_dict(before[1])
        cursor.restore(before[2])
        _restore_rng(before[3])
        model.train(before[4])
        raise
    return payload["completed_steps"]
