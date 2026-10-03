"""Shared typed public context and candidate heads for both model families.

The encoder consumes only the public IR tensors. Record order is deliberately
absent from the features: relationship endpoints are gathered by index, and
messages are reduced back to the referenced records.
"""

from __future__ import annotations

from dataclasses import dataclass

import torch
from torch import Tensor, nn


@dataclass(frozen=True)
class TypedContextConfig:
    record_category_sizes: tuple[int, int, int, int]
    relation_category_sizes: tuple[int, int]
    candidate_category_sizes: tuple[int, int, int, int]
    hidden_dim: int = 128
    record_numeric_dim: int = 8
    relation_numeric_dim: int = 4
    candidate_numeric_dim: int = 8

    def __post_init__(self) -> None:
        for name, count in (("record_category_sizes", 4), ("relation_category_sizes", 2),
                            ("candidate_category_sizes", 4)):
            sizes = getattr(self, name)
            if not isinstance(sizes, tuple) or len(sizes) != count or any(type(size) is not int or not 1 <= size <= 65_536 for size in sizes):
                raise ValueError(f"{name} needs {count} bounded vocabulary sizes")
        for name in ("hidden_dim", "record_numeric_dim", "relation_numeric_dim", "candidate_numeric_dim"):
            value = getattr(self, name)
            if type(value) is not int or not 1 <= value <= 4096:
                raise ValueError(f"{name} must be a positive bounded integer")
        if self.hidden_dim * (sum(self.record_category_sizes) + sum(self.relation_category_sizes)
                              + sum(self.candidate_category_sizes)) > 64_000_000:
            raise ValueError("typed category embeddings exceed the model parameter budget")


def masked_mean(tokens: Tensor, mask: Tensor) -> Tensor:
    """Pool real records only; a caller validates the required global record."""
    weights = mask.unsqueeze(-1).to(tokens.dtype)
    return (tokens * weights).sum(dim=1) / weights.sum(dim=1).clamp_min(1)


class TypedContextEncoder(nn.Module):
    """Encode typed records and relation messages without positional ordinals."""

    def __init__(self, config: TypedContextConfig):
        super().__init__()
        self.config = config
        d = config.hidden_dim
        self.record_embeddings = nn.ModuleList(nn.Embedding(size, d) for size in config.record_category_sizes)
        self.record_numeric = nn.Linear(config.record_numeric_dim, d)
        self.record_spatial = nn.Linear(2, d, bias=False)
        self.relation_embeddings = nn.ModuleList(nn.Embedding(size, d) for size in config.relation_category_sizes)
        self.relation_numeric = nn.Linear(config.relation_numeric_dim, d)
        self.message_to_source = nn.Sequential(nn.Linear(3 * d, d), nn.ReLU(), nn.Linear(d, d))
        self.message_to_target = nn.Sequential(nn.Linear(3 * d, d), nn.ReLU(), nn.Linear(d, d))
        self.output_norm = nn.LayerNorm(d)

    def relation_tokens(self, relation_category: Tensor, relation_numeric: Tensor, relation_mask: Tensor) -> Tensor:
        category = torch.where(relation_mask.unsqueeze(-1), relation_category, torch.zeros_like(relation_category))
        result = self.relation_numeric(relation_numeric * relation_mask.unsqueeze(-1).to(relation_numeric.dtype))
        for field, embedding in enumerate(self.relation_embeddings):
            result = result + embedding(category[..., field])
        return result * relation_mask.unsqueeze(-1).to(result.dtype)

    def forward(
        self, record_category: Tensor, record_numeric: Tensor, record_coord: Tensor,
        record_spatial_valid: Tensor, record_mask: Tensor, relation_index: Tensor,
        relation_category: Tensor, relation_numeric: Tensor, relation_mask: Tensor,
    ) -> Tensor:
        category = torch.where(record_mask.unsqueeze(-1), record_category, torch.zeros_like(record_category))
        records = self.record_numeric(record_numeric * record_mask.unsqueeze(-1).to(record_numeric.dtype))
        for field, embedding in enumerate(self.record_embeddings):
            records = records + embedding(category[..., field])
        spatial = record_coord * record_spatial_valid.unsqueeze(-1).to(record_coord.dtype)
        records = records + self.record_spatial(spatial)
        records = records * record_mask.unsqueeze(-1).to(records.dtype)

        relation = self.relation_tokens(relation_category, relation_numeric, relation_mask)
        safe_index = torch.where(relation_mask.unsqueeze(-1), relation_index, torch.zeros_like(relation_index))
        width = records.shape[-1]
        source = torch.gather(records, 1, safe_index[..., 0:1].expand(-1, -1, width))
        target = torch.gather(records, 1, safe_index[..., 1:2].expand(-1, -1, width))
        edge = torch.cat((source, target, relation), dim=-1)
        edge_mask = relation_mask.unsqueeze(-1).to(records.dtype)
        to_source = self.message_to_source(edge) * edge_mask
        to_target = self.message_to_target(edge) * edge_mask
        source_index = safe_index[..., 0:1].expand_as(to_source)
        target_index = safe_index[..., 1:2].expand_as(to_target)
        messages = torch.zeros_like(records)
        messages = messages.scatter_add(1, source_index, to_source)
        messages = messages.scatter_add(1, target_index, to_target)
        counts = torch.zeros_like(records[..., :1])
        counts = counts.scatter_add(1, safe_index[..., 0:1], edge_mask)
        counts = counts.scatter_add(1, safe_index[..., 1:2], edge_mask)
        return self.output_norm(records + messages / counts.clamp_min(1)) * record_mask.unsqueeze(-1).to(records.dtype)


class CandidateScorer(nn.Module):
    """Score complete public-intent trees, with a state-only value head."""

    def __init__(self, config: TypedContextConfig):
        super().__init__()
        self.config = config
        d = config.hidden_dim
        self.candidate_embeddings = nn.ModuleList(nn.Embedding(size, d) for size in config.candidate_category_sizes)
        self.candidate_numeric = nn.Linear(config.candidate_numeric_dim, d)
        self.candidate_spatial = nn.Linear(2, d, bias=False)
        self.candidate_order = nn.Linear(1, d, bias=False)
        self.target_projection = nn.Linear(d, d, bias=False)
        self.node_fusion = nn.Sequential(nn.Linear(2 * d, d), nn.ReLU())
        self.policy = nn.Sequential(nn.Linear(2 * d, d), nn.ReLU(), nn.Linear(d, 1))
        self.value = nn.Sequential(nn.Linear(d, d), nn.ReLU(), nn.Linear(d, 1), nn.Tanh())

    def forward(
        self, state_context: Tensor, record_tokens: Tensor,
        candidate_category: Tensor, candidate_numeric: Tensor, candidate_coord: Tensor,
        candidate_coord_valid: Tensor, candidate_parent: Tensor, candidate_order: Tensor,
        candidate_target_index: Tensor, candidate_node_mask: Tensor, candidate_mask: Tensor,
    ) -> tuple[Tensor, Tensor]:
        node_mask = candidate_node_mask & candidate_mask.unsqueeze(-1)
        category = torch.where(node_mask.unsqueeze(-1), candidate_category, torch.zeros_like(candidate_category))
        nodes = self.candidate_numeric(candidate_numeric * node_mask.unsqueeze(-1).to(candidate_numeric.dtype))
        for field, embedding in enumerate(self.candidate_embeddings):
            nodes = nodes + embedding(category[..., field])
        spatial = candidate_coord * (candidate_coord_valid & node_mask).unsqueeze(-1).to(candidate_coord.dtype)
        nodes = nodes + self.candidate_spatial(spatial)
        order = candidate_order.clamp_min(0).to(nodes.dtype).unsqueeze(-1) / 32.0
        nodes = nodes + self.candidate_order(order * node_mask.unsqueeze(-1).to(nodes.dtype))

        batch, actions, width = candidate_target_index.shape
        target_valid = (candidate_target_index >= 0) & node_mask
        safe_target = torch.where(target_valid, candidate_target_index, torch.zeros_like(candidate_target_index))
        targets = torch.gather(record_tokens, 1, safe_target.reshape(batch, actions * width, 1).expand(-1, -1, nodes.shape[-1]))
        targets = targets.reshape(batch, actions, width, nodes.shape[-1])
        nodes = nodes + self.target_projection(targets) * target_valid.unsqueeze(-1).to(nodes.dtype)
        nodes = nodes * node_mask.unsqueeze(-1).to(nodes.dtype)

        safe_parent = torch.where(node_mask & (candidate_parent >= 0), candidate_parent, torch.zeros_like(candidate_parent))
        parent = torch.gather(nodes, 2, safe_parent.unsqueeze(-1).expand_as(nodes))
        parent = parent * ((candidate_parent >= 0) & node_mask).unsqueeze(-1).to(nodes.dtype)
        nodes = self.node_fusion(torch.cat((nodes, parent), dim=-1)) * node_mask.unsqueeze(-1).to(nodes.dtype)
        weights = node_mask.unsqueeze(-1).to(nodes.dtype)
        candidates = nodes.sum(dim=2) / weights.sum(dim=2).clamp_min(1)
        # Keep the original first Linear's parameters and exact affine map,
        # while applying its state half before the dynamic candidate axis.
        # This avoids an expanded state/candidate Concat in the ONNX graph.
        first = self.policy[0]
        state_width = state_context.shape[-1]
        state_projection = nn.functional.linear(state_context, first.weight[:, :state_width], first.bias)
        candidate_projection = nn.functional.linear(candidates, first.weight[:, state_width:], None)
        hidden = self.policy[1](state_projection.unsqueeze(1) + candidate_projection)
        logits = self.policy[2](hidden).squeeze(-1)
        return logits.masked_fill(~candidate_mask, -1.0e9), self.value(state_context)


def validate_typed_inputs(config: TypedContextConfig, tensors: tuple[Tensor, ...]) -> None:
    """Check the Python inference boundary before ONNX or native execution."""
    if len(tensors) != 19 or not all(isinstance(item, Tensor) for item in tensors):
        raise ValueError("typed model requires exactly nineteen tensors")
    (rc, rn, xy, spatial, records, ri, relc, reln, relations,
     cc, cn, cxy, cxy_valid, parent, order, target, nodes, candidates, condition) = tensors
    if rc.ndim != 3 or rc.shape[2] != 4 or rc.shape[0] < 1 or rc.shape[1] < 1:
        raise ValueError("invalid record category shape")
    batch, count = rc.shape[:2]
    if rn.shape != (batch, count, config.record_numeric_dim) or xy.shape != (batch, count, 2):
        raise ValueError("invalid record numeric or coordinate shape")
    if spatial.shape != records.shape or records.shape != (batch, count):
        raise ValueError("invalid record masks")
    if ri.ndim != 3 or ri.shape[0] != batch or ri.shape[2] != 2:
        raise ValueError("invalid relation index shape")
    relation_count = ri.shape[1]
    if relc.shape != (batch, relation_count, 2) or reln.shape != (batch, relation_count, config.relation_numeric_dim) or relations.shape != (batch, relation_count):
        raise ValueError("invalid relation feature shapes")
    if cc.ndim != 4 or cc.shape[0] != batch or cc.shape[3] != 4 or cc.shape[1] < 1 or cc.shape[2] < 1:
        raise ValueError("invalid candidate category shape")
    action_count, node_count = cc.shape[1:3]
    node_shape = (batch, action_count, node_count)
    if (cn.shape != (*node_shape, config.candidate_numeric_dim) or cxy.shape != (*node_shape, 2)
            or any(value.shape != node_shape for value in (cxy_valid, parent, order, target, nodes))
            or candidates.shape != (batch, action_count) or condition.shape != (batch, 8)):
        raise ValueError("invalid candidate or condition shapes")
    for name, value in (("record_category", rc), ("relation_index", ri), ("relation_category", relc),
                        ("candidate_category", cc), ("candidate_parent", parent), ("candidate_order", order),
                        ("candidate_target_index", target)):
        if value.dtype != torch.int64:
            raise ValueError(f"{name} must be int64")
    for name, value in (("record_numeric", rn), ("record_coord", xy), ("relation_numeric", reln),
                        ("candidate_numeric", cn), ("candidate_coord", cxy), ("condition", condition)):
        if value.dtype != torch.float32 or not bool(torch.isfinite(value).all()):
            raise ValueError(f"{name} must be finite float32")
    for name, value in (("record_spatial_valid", spatial), ("record_mask", records), ("relation_mask", relations),
                        ("candidate_coord_valid", cxy_valid), ("candidate_node_mask", nodes), ("candidate_mask", candidates)):
        if value.dtype != torch.bool:
            raise ValueError(f"{name} must be bool")
    if not bool(records[:, 0].all()) or bool((spatial & ~records).any()):
        raise ValueError("the global record must be valid and padded records cannot be spatial")
    if bool((nodes & ~candidates.unsqueeze(-1)).any()) or bool((cxy_valid & ~nodes).any()) or bool((candidates & ~nodes[..., 0]).any()):
        raise ValueError("invalid candidate node or coordinate mask")
    if bool(((ri < 0) | (ri >= count))[relations].any()):
        raise ValueError("relation endpoint is out of range")
    if bool((~torch.gather(records, 1, ri.clamp(0, count - 1)[..., 0]) & relations).any()) or bool((~torch.gather(records, 1, ri.clamp(0, count - 1)[..., 1]) & relations).any()):
        raise ValueError("relation endpoint refers to a padded record")
    for values, mask, sizes, name in ((rc, records, config.record_category_sizes, "record"),
                                      (relc, relations, config.relation_category_sizes, "relation"),
                                      (cc, nodes, config.candidate_category_sizes, "candidate")):
        for field, size in enumerate(sizes):
            selected = values[..., field][mask]
            if bool(((selected < 0) | (selected >= size)).any()):
                raise ValueError(f"{name} category {field} is outside the versioned vocabulary")
    if bool(((target < -1) | (target >= count))[nodes].any()):
        raise ValueError("candidate target index is out of range")
    safe_target = target.clamp(0, count - 1)
    selected_target = torch.gather(records, 1, safe_target.reshape(batch, -1)).reshape(node_shape)
    if bool((nodes & (target >= 0) & ~selected_target).any()):
        raise ValueError("candidate target refers to a padded record")
    positions = torch.arange(node_count, device=parent.device).view(1, 1, node_count)
    if bool((nodes[..., 0] & (parent[..., 0] != -1)).any()) or bool((nodes & (positions > 0) & ((parent < 0) | (parent >= positions))).any()):
        raise ValueError("candidate parent must reference an earlier node")
    safe_parent = parent.clamp(0, node_count - 1)
    parent_valid = torch.gather(nodes, 2, safe_parent)
    if bool((nodes & (positions > 0) & ~parent_valid).any()) or bool(((order < -1) | (order > 4096))[nodes].any()):
        raise ValueError("candidate parent or order is invalid")
    if len({value.device for value in tensors}) != 1:
        raise ValueError("typed model inputs must share one device")
