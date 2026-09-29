"""Variable-geometry ResNet over the shared typed public observation contract.

The existing 8x8 network and its checkpoint format live in ``model.py``. This
family consumes spatial features and the same typed records, relations, and
candidate descriptions as the entity model.
"""

from __future__ import annotations

from copy import deepcopy
from dataclasses import asdict, dataclass
import hashlib
import math
from typing import Iterable

import torch
from torch import Tensor, nn

from ..encoding import canonical_json
from .model import AdapterDescriptor, LoRAConv2d, PolicyValueNetwork, is_adapter_parameter, tensor_state_hash
from .typed_context import CandidateScorer, TypedContextConfig, TypedContextEncoder, masked_mean, validate_typed_inputs


MAX_MODEL_PARAMETERS = 64_000_000
MAX_INPUT_BYTES = 64 * 1024 * 1024
MAX_INTERMEDIATE_BYTES = 256 * 1024 * 1024


@dataclass(frozen=True)
class MaskResNetConfig:
    board_channels: int
    typed_context: TypedContextConfig
    condition_dim: int = 8
    channels: int = 128
    residual_blocks: int = 8
    lora_rank: int = 8
    lora_alpha: float = 8.
    lora_dropout: float = 0.
    max_batch: int = 64
    max_board_axis: int = 32
    max_candidates: int = 4096
    architecture_version: str = "mask-resnet-v2"

    def __post_init__(self) -> None:
        for name in ("board_channels", "condition_dim", "channels", "residual_blocks", "lora_rank",
                     "max_batch", "max_board_axis", "max_candidates"):
            value = getattr(self, name)
            if type(value) is not int or not 1 <= value <= 1_048_576:
                raise ValueError(f"{name} must be a positive bounded integer")
        if not isinstance(self.typed_context, TypedContextConfig):
            raise ValueError("typed_context must be a TypedContextConfig")
        if self.board_channels != 6:
            raise ValueError("typed-input-v1 spatial input has six channels")
        if self.condition_dim != 8:
            raise ValueError("typed public FiLM condition has eight fields")
        if self.channels > 4096 or self.residual_blocks > 64 or self.lora_rank > 4096:
            raise ValueError("ResNet channels, block count, or LoRA rank exceed architecture limits")
        if self.max_batch > 64 or self.max_board_axis > 32 or self.max_candidates > 4096:
            raise ValueError("model runtime limits exceed the supported deployment profile")
        if (type(self.lora_alpha) not in (int, float) or type(self.lora_dropout) not in (int, float)
                or not math.isfinite(self.lora_alpha) or self.lora_alpha <= 0
                or not math.isfinite(self.lora_dropout) or self.lora_dropout != 0):
            raise ValueError("static convolution LoRA requires positive alpha and dropout 0")
        if self.architecture_version != "mask-resnet-v2":
            raise ValueError("unsupported mask ResNet architecture version")
        category_parameters = self.typed_context.hidden_dim * (
            sum(self.typed_context.record_category_sizes)
            + sum(self.typed_context.relation_category_sizes)
            + sum(self.typed_context.candidate_category_sizes)
        )
        if self.estimated_spatial_parameters + category_parameters > MAX_MODEL_PARAMETERS:
            raise ValueError("model parameter lower bound exceeds 64 million")

    @property
    def estimated_spatial_parameters(self) -> int:
        c, r = self.channels, self.lora_rank
        return (9 * self.board_channels * c + self.condition_dim * c +
                self.residual_blocks * (20 * c * c + 20 * r * c + 8 * c) +
                c * self.typed_context.hidden_dim)

    @property
    def digest(self) -> str:
        return hashlib.sha256(canonical_json(asdict(self)).encode()).hexdigest()

    def validate_geometry(self, batch: int, height: int, width: int, candidates: int,
                          candidate_nodes: int, records: int, relations: int) -> None:
        if not 1 <= batch <= self.max_batch or not 1 <= height <= self.max_board_axis or not 1 <= width <= self.max_board_axis:
            raise ValueError(f"model geometry {(batch, height, width)} exceeds limits "
                             f"{(self.max_batch, self.max_board_axis, self.max_board_axis)}")
        if not 1 <= candidates <= self.max_candidates:
            raise ValueError(f"candidate count {candidates} exceeds limit {self.max_candidates}")
        if not 1 <= candidate_nodes <= 64 or not 1 <= records <= 2048 or not 0 <= relations <= 8192:
            raise ValueError("typed record, relation, or candidate node count exceeds model limits")
        # Conservative inference estimate for the spatial stack and the shared
        # typed record/relation/candidate computations, before allocating output.
        spatial_bytes = batch * self.channels * height * width * 4 * 6
        d = self.typed_context.hidden_dim
        typed_bytes = batch * d * 4 * (records * 6 + relations * 6 + candidates * candidate_nodes * 4)
        if spatial_bytes + typed_bytes > MAX_INTERMEDIATE_BYTES:
            raise ValueError("model intermediate tensor estimate exceeds 256 MiB")


def _masked(value: Tensor, layout_mask: Tensor) -> Tensor:
    return value * layout_mask.to(dtype=value.dtype)


class MaskedBatchNorm2d(nn.BatchNorm2d):
    """Use only playable cells for training statistics and spatial output.

    The batch axis still contributes to the statistics, as in BatchNorm2d.
    Keeping its affine and running-stat buffers also permits the adapter path
    to freeze normalization while a base model remains in training mode.
    """

    def forward(self, value: Tensor, valid_mask: Tensor) -> Tensor:
        if not self.training:
            return _masked(super().forward(value), valid_mask)

        weights = valid_mask.to(dtype=value.dtype)
        count = weights.sum()
        mean = (value * weights).sum(dim=(0, 2, 3)) / count.clamp_min(1)
        centered = (value - mean[None, :, None, None]) * weights
        variance = centered.square().sum(dim=(0, 2, 3)) / count.clamp_min(1)
        if self.track_running_stats and bool(count > 0):
            with torch.no_grad():
                self.num_batches_tracked.add_(1)
                momentum = self.momentum if self.momentum is not None else 1.0 / self.num_batches_tracked.item()
                self.running_mean.lerp_(mean.detach(), momentum)
                unbiased = variance.detach() * count / (count - 1).clamp_min(1)
                self.running_var.lerp_(unbiased, momentum)
        normalized = (value - mean[None, :, None, None]) * torch.rsqrt(variance[None, :, None, None] + self.eps)
        if self.affine:
            normalized = normalized * self.weight[None, :, None, None] + self.bias[None, :, None, None]
        return _masked(normalized, valid_mask)


class MaskedResidualFiLMBlock(nn.Module):
    def __init__(self, config: MaskResNetConfig):
        super().__init__()
        self.conv1 = LoRAConv2d(config.channels, config.lora_rank, config.lora_alpha)
        self.bn1 = MaskedBatchNorm2d(config.channels)
        self.conv2 = LoRAConv2d(config.channels, config.lora_rank, config.lora_alpha)
        self.bn2 = MaskedBatchNorm2d(config.channels)
        self.film = nn.Linear(config.channels, 2 * config.channels)
        nn.init.zeros_(self.film.weight)
        nn.init.zeros_(self.film.bias)

    def forward(self, value: Tensor, layout_mask: Tensor, condition: Tensor) -> Tensor:
        residual = _masked(self.conv1(value), layout_mask)
        residual = self.bn1(residual, layout_mask)
        residual = _masked(torch.relu(residual), layout_mask)
        residual = _masked(self.conv2(residual), layout_mask)
        residual = self.bn2(residual, layout_mask)
        gamma, beta = self.film(condition).chunk(2, dim=1)
        residual = _masked(residual * (1 + gamma[:, :, None, None]) + beta[:, :, None, None], layout_mask)
        return _masked(torch.relu(value + residual), layout_mask)


class MaskedResNetBackbone(nn.Module):
    def __init__(self, config: MaskResNetConfig):
        super().__init__()
        self.stem_conv = nn.Conv2d(config.board_channels, config.channels, 3, padding=1, bias=False)
        self.stem_norm = MaskedBatchNorm2d(config.channels)
        self.condition_projection = nn.Sequential(nn.Linear(config.condition_dim, config.channels), nn.LeakyReLU(0.01))
        self.blocks = nn.ModuleList(MaskedResidualFiLMBlock(config) for _ in range(config.residual_blocks))

    def forward(self, spatial: Tensor, layout_mask: Tensor, condition: Tensor) -> Tensor:
        # The layout includes collapsed cells; their typed hole marker is
        # visible to adjacent convolutions, but their other fields and their
        # own activations cannot enter normalization or pooling. Padding has
        # neither an activation nor a hole marker.
        hole = layout_mask & (spatial[:, 2:3] > 0.5)
        playable = layout_mask & ~hole
        features = torch.cat((_masked(spatial[:, :2], playable), hole.to(spatial.dtype),
                              _masked(spatial[:, 3:], playable)), dim=1)
        features = _masked(self.stem_conv(features), playable)
        features = self.stem_norm(features, playable)
        features = _masked(torch.relu(features), playable)
        projected_condition = self.condition_projection(condition)
        for block in self.blocks:
            features = block(features, playable, projected_condition)
        weights = playable.to(dtype=features.dtype)
        return features.sum(dim=(2, 3)) / weights.sum(dim=(2, 3)).clamp_min(1)


class MaskResNetPolicyValueNetwork(nn.Module):
    """Spatial ResNet plus the shared record context and candidate scorer."""

    def __init__(self, config: MaskResNetConfig):
        super().__init__()
        self.config = config
        self.training_mode = "base"
        self.merged = False
        self.spatial = MaskedResNetBackbone(config)
        self.typed_context = TypedContextEncoder(config.typed_context)
        self.spatial_projection = nn.Linear(config.channels, config.typed_context.hidden_dim)
        self.candidate_scorer = CandidateScorer(config.typed_context)
        if sum(parameter.numel() for parameter in self.parameters()) > MAX_MODEL_PARAMETERS:
            raise ValueError("model aggregate parameter budget exceeds 64 million")
        self.configure_training("base")

    def forward(
        self,
        spatial: Tensor,
        layout_mask: Tensor,
        record_category: Tensor,
        record_numeric: Tensor,
        record_coord: Tensor,
        record_spatial_valid: Tensor,
        record_mask: Tensor,
        relation_index: Tensor,
        relation_category: Tensor,
        relation_numeric: Tensor,
        relation_mask: Tensor,
        candidate_category: Tensor,
        candidate_numeric: Tensor,
        candidate_coord: Tensor,
        candidate_coord_valid: Tensor,
        candidate_parent: Tensor,
        candidate_order: Tensor,
        candidate_target_index: Tensor,
        candidate_node_mask: Tensor,
        candidate_mask: Tensor,
        condition: Tensor,
    ) -> tuple[Tensor, Tensor]:
        spatial_context = self.spatial(spatial, layout_mask, condition)
        record_tokens = self.typed_context(record_category, record_numeric, record_coord,
                                           record_spatial_valid, record_mask, relation_index,
                                           relation_category, relation_numeric, relation_mask)
        record_context = masked_mean(record_tokens, record_mask)
        state_context = record_context + self.spatial_projection(spatial_context)
        return self.candidate_scorer(state_context, record_tokens, candidate_category, candidate_numeric,
                                     candidate_coord, candidate_coord_valid, candidate_parent,
                                     candidate_order, candidate_target_index, candidate_node_mask,
                                     candidate_mask)

    def validate_inputs(self, *inputs: Tensor) -> None:
        if len(inputs) != 21 or any(not isinstance(item, Tensor) for item in inputs):
            raise ValueError("mask ResNet requires 21 typed tensor inputs")
        (spatial, layout_mask, record_category, record_numeric, record_coord,
         record_spatial_valid, record_mask, relation_index, relation_category,
         relation_numeric, relation_mask, candidate_category, candidate_numeric,
         candidate_coord, candidate_coord_valid, candidate_parent, candidate_order,
         candidate_target_index, candidate_node_mask, candidate_mask, condition) = inputs
        if spatial.ndim != 4 or spatial.shape[1] != self.config.board_channels:
            raise ValueError("invalid spatial feature shape")
        batch, _, height, width = spatial.shape
        if layout_mask.shape != (batch, 1, height, width) or layout_mask.dtype != torch.bool:
            raise ValueError("invalid layout mask")
        if condition.shape != (batch, self.config.condition_dim):
            raise ValueError("invalid FiLM condition shape")
        if candidate_category.ndim != 4 or candidate_category.shape[0] != batch:
            raise ValueError("invalid candidate category shape")
        candidates = candidate_category.shape[1]
        candidate_nodes = candidate_category.shape[2]
        if record_category.ndim != 3 or relation_index.ndim != 3:
            raise ValueError("invalid record or relation rank")
        self.config.validate_geometry(batch, height, width, candidates, candidate_nodes,
                                      record_category.shape[1], relation_index.shape[1])
        if not bool(layout_mask.any(dim=(1, 2, 3)).all()):
            raise ValueError("each spatial observation needs a layout cell")
        if sum(value.numel() * value.element_size() for value in inputs) > MAX_INPUT_BYTES:
            raise ValueError("model inputs exceed 64 MiB")
        if len({value.device for value in inputs}) != 1 or spatial.device != next(self.parameters()).device:
            raise ValueError("model and input devices differ")
        for value in (spatial, condition):
            if value.dtype != torch.float32 or not bool(torch.isfinite(value).all()):
                raise ValueError("spatial and FiLM inputs must be finite float32")
        validate_typed_inputs(self.config.typed_context, inputs[2:])

    def evaluate(self, *inputs: Tensor) -> tuple[Tensor, Tensor]:
        self.validate_inputs(*inputs)
        if self.training:
            raise ValueError("evaluate requires model.eval() so normalization statistics remain fixed")
        with torch.no_grad():
            outputs = self(*inputs)
        if any(not bool(torch.isfinite(value).all()) for value in outputs):
            raise ValueError("model produced non-finite output")
        return outputs

    def configure_training(self, mode: str) -> Iterable[nn.Parameter]:
        if mode not in ("base", "adapter") or self.merged:
            raise ValueError("choose base or adapter training on an unmerged model")
        self.training_mode = mode
        for name, parameter in self.named_parameters():
            parameter.grad = None
            parameter.requires_grad_(is_adapter_parameter(name) == (mode == "adapter"))
        for module in self.modules():
            if isinstance(module, LoRAConv2d):
                module.enabled = mode == "adapter"
        self.train(True)
        return (parameter for parameter in self.parameters() if parameter.requires_grad)

    def train(self, mode: bool = True) -> MaskResNetPolicyValueNetwork:
        super().train(mode)
        if mode and self.training_mode == "adapter":
            for module in self.modules():
                if isinstance(module, nn.BatchNorm2d):
                    module.eval()
        return self

    def base_state(self) -> dict[str, Tensor]:
        return {name: value for name, value in self.state_dict().items() if not is_adapter_parameter(name)}

    def adapter_state(self) -> dict[str, Tensor]:
        return {name: value for name, value in self.state_dict().items() if is_adapter_parameter(name)}

    @property
    def base_hash(self) -> str:
        return tensor_state_hash(self.base_state())

    def adapter_descriptor(self, encoder_hash: str) -> AdapterDescriptor:
        return AdapterDescriptor(self.base_hash, self.config.digest, encoder_hash)

    def warm_start_residual_convolutions(self, legacy: PolicyValueNetwork) -> tuple[str, ...]:
        """Initialize matching residual base convolutions from the 8x8 family.

        This is a weight-only warm start. The new spatial input, FiLM, typed
        context, candidate scorer, normalization state, optimizer, RNG, and
        data cursor retain their own initialization; it is not resume.
        """
        if not isinstance(legacy, PolicyValueNetwork) or legacy.merged:
            raise ValueError("warm start requires an unmerged legacy ResNet")
        if self.merged or self.training_mode != "base":
            raise ValueError("warm start requires an unmerged new base model")
        if len(legacy.blocks) != len(self.spatial.blocks):
            raise ValueError("warm start residual block count mismatch")
        transfers: list[tuple[str, Tensor, Tensor]] = []
        for index, (source_block, target_block) in enumerate(zip(legacy.blocks, self.spatial.blocks, strict=True)):
            for conv_name in ("conv1", "conv2"):
                name = f"spatial.blocks.{index}.{conv_name}.base.weight"
                try:
                    source = getattr(source_block, conv_name).base.weight
                    target = getattr(target_block, conv_name).base.weight
                except AttributeError as error:
                    raise ValueError(f"warm start missing residual weight: {name}") from error
                if source.shape != target.shape or source.dtype != target.dtype or not bool(torch.isfinite(source).all()):
                    raise ValueError(f"warm start incompatible residual weight: {name}")
                transfers.append((name, source.detach(), target))
        # Prepare all tensors before changing the destination. A failed shape,
        # dtype, device transfer, or allocation leaves the model untouched.
        prepared = [(name, source.to(device=target.device).clone(), target)
                    for name, source, target in transfers]
        with torch.no_grad():
            for _, source, target in prepared:
                target.copy_(source)
        return tuple(name for name, _, _ in prepared)

    def merged_copy(self, descriptor: AdapterDescriptor, encoder_hash: str) -> MaskResNetPolicyValueNetwork:
        descriptor.validate_merge()
        if self.merged or descriptor.base_hash != self.base_hash or descriptor.config_hash != self.config.digest or descriptor.encoder_hash != encoder_hash:
            raise ValueError("adapter and base/encoder compatibility mismatch")
        merged = deepcopy(self)
        with torch.no_grad():
            for module in merged.modules():
                if isinstance(module, LoRAConv2d):
                    module.base.weight.add_(module.delta())
                    module.enabled = False
                    module.lora_a.zero_()
                    module.lora_b.zero_()
        merged.merged = True
        merged.eval()
        for parameter in merged.parameters():
            parameter.requires_grad_(False)
        return merged
