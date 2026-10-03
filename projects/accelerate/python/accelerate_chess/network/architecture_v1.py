"""Fixed 8x8 ResNet and semantic entity Transformer with one output contract.

These are additive model families. Existing typed and legacy artifacts retain
their own versions and cannot be loaded as weights for these architectures.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Sequence

import numpy as np
import torch
from torch import Tensor, nn

from ..architecture import ProjectedPosition
from ..ir import batch_typed_positions
from .entity_transformer import EntityTransformerBlock, EntityTransformerConfig, LoRALinear
from .model import LoRAConv2d, ModelConfig, ResidualFiLMBlock, is_adapter_parameter
from .typed_context import CandidateScorer, TypedContextConfig, masked_mean


@dataclass(frozen=True)
class ModelBatch:
    geometry: Tensor  # [B,4]: origin row/col and unpadded height/width
    entity_category: Tensor
    entity_numeric: Tensor
    entity_coord: Tensor
    entity_mask: Tensor
    relation_index: Tensor
    relation_kind: Tensor
    relation_mask: Tensor
    occupancy: Tensor
    spatial_state: Tensor
    condition: Tensor
    candidates: tuple[Tensor, ...]

    def to(self, device: torch.device | str) -> ModelBatch:
        return ModelBatch(*(value.to(device) if isinstance(value, Tensor) else tuple(t.to(device) for t in value)
                            for value in vars(self).values()))


def batch_projected_positions(positions: Sequence[ProjectedPosition]) -> ModelBatch:
    if not positions:
        raise ValueError("projected batch cannot be empty")
    typed = batch_typed_positions([position.typed for position in positions])
    height = max(position.occupancy.shape[1] for position in positions)
    width = max(position.occupancy.shape[2] for position in positions)
    count = max(position.entity_mask.size for position in positions)
    edges = max(position.relation_mask.size for position in positions)
    if count > 2048 or edges > 8192:
        raise ValueError("projected batch exceeds entity or relation limits")

    def padded(name: str, shape: tuple[int, ...], dtype: np.dtype) -> np.ndarray:
        array = np.zeros((len(positions), *shape), dtype=dtype)
        for index, position in enumerate(positions):
            source = getattr(position, name)
            array[(index, *(slice(0, size) for size in source.shape))] = source
        return array

    tensor = lambda array: torch.from_numpy(array.copy())
    names = ("candidate_category", "candidate_numeric", "candidate_coord", "candidate_coord_valid",
             "candidate_parent", "candidate_order", "candidate_target_index", "candidate_node_mask", "candidate_mask")
    candidates = []
    for name in names:
        source = typed.inputs[name].copy()
        if name == "candidate_target_index":
            source.fill(-1)
            for index, position in enumerate(positions):
                target = position.candidate_target_index
                source[index, :target.shape[0], :target.shape[1]] = target
        candidates.append(tensor(source))
    geometry = np.asarray([(p.typed.geometry.origin_row, p.typed.geometry.origin_col,
                            p.typed.geometry.height, p.typed.geometry.width) for p in positions], np.int64)
    return ModelBatch(tensor(geometry), tensor(padded("entity_category", (count, 6), np.int64)),
                      tensor(padded("entity_numeric", (count, 8), np.float32)),
                      tensor(padded("entity_coord", (count, 2), np.float32)),
                      tensor(padded("entity_mask", (count,), np.bool_)),
                      tensor(padded("relation_index", (edges, 2), np.int64)),
                      tensor(padded("relation_kind", (edges,), np.int64)),
                      tensor(padded("relation_mask", (edges,), np.bool_)),
                      tensor(padded("occupancy", (count, height, width), np.float32)),
                      tensor(padded("spatial_state", (4, height, width), np.float32)),
                      tensor(padded("condition", (8,), np.float32)), tuple(candidates))


class SemanticEntityEmbedding(nn.Module):
    def __init__(self, width: int):
        super().__init__()
        self.kind = nn.Embedding(9, width)
        self.subtype = nn.Embedding(4096, width)
        self.owner = nn.Embedding(4096, width)
        self.phase = nn.Embedding(4096, width)
        self.state = nn.Embedding(4096, width)
        self.descriptor = nn.Embedding(4096, width)
        self.numeric = nn.Linear(8, width)
        self.coordinate = nn.Linear(2, width, bias=False)
        self.relation = nn.Embedding(3, width)
        self.output = nn.LayerNorm(width)

    def forward(self, batch: ModelBatch) -> Tensor:
        category = batch.entity_category
        mask = batch.entity_mask.unsqueeze(-1)
        tokens = (self.kind(category[..., 0]) + self.subtype(category[..., 1])
                  + self.owner(category[..., 2]) + self.phase(category[..., 3])
                  + self.state(category[..., 4]) + self.descriptor(category[..., 5])
                  + self.numeric(batch.entity_numeric) + self.coordinate(batch.entity_coord))
        source = batch.relation_index[..., 0]
        target = batch.relation_index[..., 1]
        edge = self.relation(batch.relation_kind) * batch.relation_mask.unsqueeze(-1)
        messages = torch.zeros_like(tokens).scatter_add(1, target.unsqueeze(-1).expand_as(edge), edge)
        messages = messages.scatter_add(1, source.unsqueeze(-1).expand_as(edge), edge)
        count = torch.zeros_like(tokens[..., :1]).scatter_add(1, target.unsqueeze(-1), batch.relation_mask.unsqueeze(-1).to(tokens.dtype))
        count = count.scatter_add(1, source.unsqueeze(-1), batch.relation_mask.unsqueeze(-1).to(tokens.dtype))
        return self.output(tokens + messages / count.clamp_min(1)) * mask


class _SharedModel(nn.Module):
    def __init__(self, typed: TypedContextConfig):
        super().__init__()
        self.typed = typed
        self.entities = SemanticEntityEmbedding(typed.hidden_dim)
        self.scorer = CandidateScorer(typed)
        self.global_encoder = nn.Sequential(nn.Linear(typed.hidden_dim + 8, typed.hidden_dim), nn.ReLU())

    def configure_training(self, mode: str):
        if mode not in ("base", "adapter"):
            raise ValueError("training mode must be base or adapter")
        self.training_mode = mode
        for name, parameter in self.named_parameters():
            parameter.grad = None
            parameter.requires_grad_(is_adapter_parameter(name) == (mode == "adapter"))
        for module in self.modules():
            if isinstance(module, (LoRAConv2d, LoRALinear)):
                module.enabled = mode == "adapter"
        self.train(True)
        return (parameter for parameter in self.parameters() if parameter.requires_grad)

    def base_state(self) -> dict[str, Tensor]:
        return {name: value for name, value in self.state_dict().items() if not is_adapter_parameter(name)}

    def adapter_state(self) -> dict[str, Tensor]:
        return {name: value for name, value in self.state_dict().items() if is_adapter_parameter(name)}

    def _context(self, batch: ModelBatch, tokens: Tensor) -> Tensor:
        global_mask = batch.entity_mask & ((batch.entity_category[..., 0] == 0)
                                          | (batch.entity_category[..., 0] == 2)
                                          | (batch.entity_category[..., 0] == 3)
                                          | (batch.entity_category[..., 0] == 4)
                                          | (batch.entity_category[..., 0] == 8))
        return self.global_encoder(torch.cat((masked_mean(tokens, global_mask), batch.condition), dim=-1))

    def _score(self, batch: ModelBatch, state: Tensor, tokens: Tensor) -> tuple[Tensor, Tensor]:
        return self.scorer(state, tokens, *batch.candidates)

    def evaluate(self, batch: ModelBatch) -> tuple[Tensor, Tensor]:
        self._validate(batch)
        if self.training:
            raise ValueError("evaluate requires model.eval()")
        with torch.no_grad():
            result = self(batch)
        if not all(bool(torch.isfinite(value).all()) for value in result):
            raise ValueError("model produced nonfinite output")
        return result

    def _validate(self, batch: ModelBatch) -> None:
        if not isinstance(batch, ModelBatch) or batch.entity_category.ndim != 3 or batch.entity_category.shape[-1] != 6:
            raise ValueError("invalid semantic model batch")
        b, e = batch.entity_mask.shape
        if (not 1 <= b <= 64 or not 1 <= e <= 2048 or batch.geometry.shape != (b, 4)
                or batch.geometry.dtype != torch.int64 or batch.entity_category.shape[:2] != (b, e)
                or batch.entity_numeric.shape != (b, e, 8) or batch.entity_coord.shape != (b, e, 2)
                or batch.occupancy.shape[:2] != (b, e) or batch.spatial_state.shape != (b, 4, *batch.occupancy.shape[2:])
                or batch.condition.shape != (b, 8) or not bool(batch.entity_mask[:, 0].all())):
            raise ValueError("semantic model input shapes or global token are invalid")
        if (batch.relation_index.shape[:2] != batch.relation_mask.shape
                or batch.relation_index.shape[-1] != 2 or batch.relation_kind.shape != batch.relation_mask.shape
                or batch.relation_index.dtype != torch.int64 or batch.relation_kind.dtype != torch.int64
                or batch.relation_mask.dtype != torch.bool
                or bool(((batch.relation_index < 0) | (batch.relation_index >= e)).any())
                or bool(((batch.relation_kind < 0) | (batch.relation_kind >= 3)).any())):
            raise ValueError("semantic relation index is invalid")
        if bool((batch.geometry[:, 2:] < 1).any()) or bool((batch.geometry[:, 2:] > 32).any()):
            raise ValueError("semantic geometry is outside supported bounds")
        if (batch.entity_category.dtype != torch.int64 or batch.entity_numeric.dtype != torch.float32
                or batch.entity_coord.dtype != torch.float32 or batch.entity_mask.dtype != torch.bool
                or batch.occupancy.dtype != torch.float32 or batch.spatial_state.dtype != torch.float32
                or batch.condition.dtype != torch.float32):
            raise ValueError("semantic model dtype mismatch")
        if (bool((batch.entity_category[..., 0] < 0).any()) or bool((batch.entity_category[..., 0] >= 9).any())
                or bool((batch.entity_category[..., 1:] < 0).any()) or bool((batch.entity_category[..., 1:] >= 4096).any())):
            raise ValueError("semantic category is outside its vocabulary")
        if not all(bool(torch.isfinite(item).all()) for item in (batch.entity_numeric, batch.entity_coord, batch.occupancy,
                                                                  batch.spatial_state, batch.condition)):
            raise ValueError("semantic model input has nonfinite values")
        if bool((batch.occupancy < 0).any()):
            raise ValueError("entity occupancy cannot be negative")
        if (len(batch.candidates) != 9 or batch.candidates[-1].shape[0] != b
                or batch.candidates[-1].dtype != torch.bool):
            raise ValueError("candidate contract is invalid")
        (cc, cn, xy, xy_valid, parent, order, target, node_mask, candidate_mask) = batch.candidates
        if (cc.ndim != 4 or cc.shape[0] != b or cc.shape[-1] != 4 or cc.shape[1] < 1 or cc.shape[2] < 1
                or cc.shape[1] > 4096 or cc.shape[2] > 256):
            raise ValueError("candidate category shape is invalid")
        node_shape = cc.shape[:3]
        if (cn.shape != (*node_shape, self.typed.candidate_numeric_dim) or xy.shape != (*node_shape, 2)
                or any(item.shape != node_shape for item in (xy_valid, parent, order, target, node_mask))
                or candidate_mask.shape != node_shape[:2]):
            raise ValueError("candidate tensor shapes are inconsistent")
        if (cc.dtype != torch.int64 or cn.dtype != torch.float32 or xy.dtype != torch.float32
                or xy_valid.dtype != torch.bool or parent.dtype != torch.int64 or order.dtype != torch.int64
                or target.dtype != torch.int64 or node_mask.dtype != torch.bool):
            raise ValueError("candidate tensor dtype mismatch")
        if not bool(torch.isfinite(cn).all()) or not bool(torch.isfinite(xy).all()):
            raise ValueError("candidate tensor contains nonfinite values")
        if bool((parent >= node_shape[2]).any()) or bool((parent < -1).any()):
            raise ValueError("candidate parent is outside the candidate tree")
        for slot, size in enumerate(self.typed.candidate_category_sizes):
            if bool((cc[..., slot] < 0).any()) or bool((cc[..., slot] >= size).any()):
                raise ValueError("candidate category is outside the versioned vocabulary")
        target = batch.candidates[6]
        if bool((target >= e).any()) or bool((target < -1).any()):
            raise ValueError("candidate target is outside entity batch")
        tensors = [value for value in vars(batch).values() if isinstance(value, Tensor)] + list(batch.candidates)
        if sum(t.numel() * t.element_size() for t in tensors) > 64 * 1024 * 1024:
            raise ValueError("semantic model input exceeds 64 MiB")
        if any(t.device != batch.entity_category.device for t in tensors) or next(self.parameters()).device != batch.entity_category.device:
            raise ValueError("model and semantic batch devices differ")


class Fixed8x8ResNet(_SharedModel):
    def __init__(self, typed: TypedContextConfig, *, channels: int = 128, residual_blocks: int = 8,
                 lora_rank: int = 8, lora_alpha: float = 8.):
        super().__init__(typed)
        self.config = ModelConfig(typed.hidden_dim + 8, typed.hidden_dim, 1, channels,
                                  residual_blocks, lora_rank, lora_alpha)
        self.stem = nn.Conv2d(typed.hidden_dim + 8, channels, 3, padding=1)
        self.blocks = nn.ModuleList(ResidualFiLMBlock(self.config) for _ in range(residual_blocks))
        self.film_context = nn.Linear(typed.hidden_dim, channels)
        self.state = nn.Linear(channels + typed.hidden_dim, typed.hidden_dim)
        self.configure_training("base")

    def train(self, mode: bool = True) -> Fixed8x8ResNet:
        super().train(mode)
        if mode and self.training_mode == "adapter":
            for module in self.modules():
                if isinstance(module, nn.BatchNorm2d):
                    module.eval()
        return self

    def forward(self, batch: ModelBatch) -> tuple[Tensor, Tensor]:
        tokens = self.entities(batch)
        context = self._context(batch, tokens)
        kind = batch.entity_category[..., 0]
        piece = (kind == 1) & batch.entity_mask
        spatial_entity = ((kind == 1) | (kind == 4) | (kind == 5) | (kind == 6)) & batch.entity_mask
        occupied = batch.occupancy * spatial_entity.unsqueeze(-1).unsqueeze(-1)
        count = occupied.sum(dim=1, keepdim=True)
        spatial_embeddings = torch.einsum("behw,bed->bdhw", occupied, tokens) / count.clamp_min(1)
        piece_occupancy = occupied * piece.unsqueeze(-1).unsqueeze(-1)
        anchor = torch.einsum("behw,be->bhw", piece_occupancy, batch.entity_numeric[..., 3]).unsqueeze(1)
        footprint = torch.einsum("behw,be->bhw", piece_occupancy, batch.entity_numeric[..., 5]).unsqueeze(1)
        anchor_row = (batch.entity_numeric[..., 3] * 7).round().long().clamp(0, 7)
        anchor_col = (batch.entity_numeric[..., 4] * 7).round().long().clamp(0, 7)
        anchor_map = torch.zeros(batch.entity_mask.shape[0], 64, dtype=tokens.dtype, device=tokens.device)
        anchor_map = anchor_map.scatter_add(1, anchor_row * 8 + anchor_col, piece.to(tokens.dtype))
        spatial = torch.cat((spatial_embeddings, batch.spatial_state, count, anchor, footprint,
                             anchor_map.reshape(-1, 1, 8, 8)), dim=1)
        features = torch.relu(self.stem(spatial))
        film = self.film_context(context)
        for block in self.blocks:
            features = block(features, film)
        pooled = features.mean(dim=(2, 3))
        state = torch.relu(self.state(torch.cat((pooled, context + masked_mean(tokens, batch.entity_mask)), dim=-1)))
        return self._score(batch, state, tokens)

    def _validate(self, batch: ModelBatch) -> None:
        super()._validate(batch)
        if (batch.occupancy.shape[2:] != (8, 8)
                or bool((batch.geometry != torch.tensor([0, 0, 8, 8], device=batch.geometry.device)).any())):
            raise ValueError("fixed8-spatial-v1 requires exactly 8x8 input")


class EntityTokenTransformer(_SharedModel):
    def __init__(self, typed: TypedContextConfig, *, blocks: int = 4, heads: int = 4,
                 ffn_dim: int = 512, lora_rank: int = 8, lora_alpha: float = 8.):
        super().__init__(typed)
        config = EntityTransformerConfig(typed, blocks, heads, ffn_dim, lora_rank, lora_alpha)
        self.config = config
        self.blocks = nn.ModuleList(EntityTransformerBlock(config) for _ in range(blocks))
        self.condition_projection = nn.Linear(typed.hidden_dim, 8)
        self.relation_bias = nn.Embedding(3, heads)
        self.spatial_bias = nn.Parameter(torch.zeros(heads, 2))
        self.final_norm = nn.LayerNorm(typed.hidden_dim)
        self.state = nn.Sequential(nn.Linear(2 * typed.hidden_dim, typed.hidden_dim), nn.ReLU())
        self.configure_training("base")

    def forward(self, batch: ModelBatch) -> tuple[Tensor, Tensor]:
        tokens = self.entities(batch)
        context = self._context(batch, tokens)
        condition = self.condition_projection(context) + batch.condition
        b, e = batch.entity_mask.shape
        delta = batch.entity_coord[:, :, None, :] - batch.entity_coord[:, None, :, :]
        bias = torch.einsum("bijd,hd->bhij", delta, self.spatial_bias)
        edge = self.relation_bias(batch.relation_kind) * batch.relation_mask.unsqueeze(-1)
        flat = torch.zeros(b, self.config.heads, e * e, device=tokens.device, dtype=tokens.dtype)
        index = batch.relation_index[..., 0] * e + batch.relation_index[..., 1]
        flat = flat.scatter_add(2, index.unsqueeze(1).expand(-1, self.config.heads, -1), edge.transpose(1, 2))
        bias = bias + flat.reshape(b, self.config.heads, e, e)
        for block in self.blocks:
            tokens = block(tokens, condition, batch.entity_mask, bias)
        tokens = self.final_norm(tokens) * batch.entity_mask.unsqueeze(-1)
        state = self.state(torch.cat((tokens[:, 0], masked_mean(tokens, batch.entity_mask)), dim=-1))
        return self._score(batch, state, tokens)

    def _validate(self, batch: ModelBatch) -> None:
        super()._validate(batch)
        b, e = batch.entity_mask.shape
        if b * self.config.heads * e * e * 12 > 256 * 1024 * 1024:
            raise ValueError("entity attention exceeds 256 MiB working estimate")
