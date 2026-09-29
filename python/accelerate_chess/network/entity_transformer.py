"""Typed public-entity policy/value Transformer with static Q/V LoRA.

The IR supplies one valid global record at index zero. Other records have no
learned ordinal position: pairwise public coordinates and typed relations
provide structure, while padding is masked after every residual operation.
"""

from __future__ import annotations

from copy import deepcopy
from dataclasses import asdict, dataclass
import hashlib
import math

import torch
from torch import Tensor, nn

from ..encoding import canonical_json
from .typed_context import CandidateScorer, TypedContextConfig, TypedContextEncoder, masked_mean, validate_typed_inputs


@dataclass(frozen=True)
class EntityTransformerConfig:
    typed: TypedContextConfig
    blocks: int = 4
    heads: int = 4
    ffn_dim: int = 512
    lora_rank: int = 8
    lora_alpha: float = 8.0
    lora_dropout: float = 0.0
    architecture_version: str = "entity-transformer-film-lora-v1"

    def __post_init__(self) -> None:
        if not isinstance(self.typed, TypedContextConfig):
            raise ValueError("typed context config is required")
        for name in ("blocks", "heads", "ffn_dim", "lora_rank"):
            value = getattr(self, name)
            if type(value) is not int or not 1 <= value <= 4096:
                raise ValueError(f"{name} must be a positive bounded integer")
        if self.blocks > 64 or self.typed.hidden_dim % self.heads or self.lora_rank > self.typed.hidden_dim:
            raise ValueError("invalid Transformer depth, head count, or LoRA rank")
        if not math.isfinite(self.lora_alpha) or self.lora_alpha <= 0 or self.lora_dropout != 0:
            raise ValueError("static Transformer LoRA requires positive alpha and dropout zero")
        if self.architecture_version != "entity-transformer-film-lora-v1":
            raise ValueError("unsupported Transformer architecture version")
        width = self.typed.hidden_dim
        embeddings = width * (sum(self.typed.record_category_sizes)
                              + sum(self.typed.relation_category_sizes)
                              + sum(self.typed.candidate_category_sizes))
        block_weights = self.blocks * (4 * width * width + 2 * width * self.ffn_dim
                                       + 4 * width * self.lora_rank)
        if embeddings + block_weights > 64_000_000:
            raise ValueError("Transformer parameter lower bound exceeds 64 million")

    @property
    def digest(self) -> str:
        return hashlib.sha256(canonical_json(asdict(self)).encode()).hexdigest()


@dataclass(frozen=True)
class TransformerAdapterDescriptor:
    base_hash: str
    config_hash: str
    encoder_hash: str
    generator: str = "static-lora"
    condition_lifetime: str = "global"
    mergeable: bool = True
    version: str = "lora-transformer-qv-v1"

    def validate_merge(self) -> None:
        if (not self.mergeable or self.generator != "static-lora" or self.condition_lifetime != "global"
                or self.version != "lora-transformer-qv-v1"):
            raise ValueError("only a fixed compatible Transformer adapter can be merged")


class LoRALinear(nn.Module):
    def __init__(self, width: int, rank: int, alpha: float):
        super().__init__()
        self.base = nn.Linear(width, width)
        self.lora_a = nn.Parameter(torch.empty(rank, width))
        self.lora_b = nn.Parameter(torch.zeros(width, rank))
        nn.init.kaiming_uniform_(self.lora_a, a=math.sqrt(5))
        self.scale = alpha / rank
        self.enabled = False

    def forward(self, value: Tensor) -> Tensor:
        result = self.base(value)
        if self.enabled:
            result = result + nn.functional.linear(nn.functional.linear(value, self.lora_a), self.lora_b) * self.scale
        return result

    def delta(self) -> Tensor:
        return (self.lora_b @ self.lora_a) * self.scale


class EntitySelfAttention(nn.Module):
    def __init__(self, config: EntityTransformerConfig):
        super().__init__()
        width = config.typed.hidden_dim
        self.heads = config.heads
        self.head_width = width // config.heads
        self.q = LoRALinear(width, config.lora_rank, config.lora_alpha)
        self.k = nn.Linear(width, width)
        self.v = LoRALinear(width, config.lora_rank, config.lora_alpha)
        self.output = nn.Linear(width, width)

    def forward(self, tokens: Tensor, record_mask: Tensor, attention_bias: Tensor) -> Tensor:
        batch, count, width = tokens.shape
        q = self.q(tokens).reshape(batch, count, self.heads, self.head_width).transpose(1, 2)
        k = self.k(tokens).reshape(batch, count, self.heads, self.head_width).transpose(1, 2)
        v = self.v(tokens).reshape(batch, count, self.heads, self.head_width).transpose(1, 2)
        scores = torch.matmul(q, k.transpose(-2, -1)) * (self.head_width ** -0.5)
        scores = scores + attention_bias
        scores = scores.masked_fill(~record_mask[:, None, None, :], -1.0e4)
        weights = torch.softmax(scores, dim=-1)
        attended = torch.matmul(weights, v).transpose(1, 2).reshape(batch, count, width)
        return self.output(attended) * record_mask.unsqueeze(-1).to(tokens.dtype)


class EntityTransformerBlock(nn.Module):
    def __init__(self, config: EntityTransformerConfig):
        super().__init__()
        width = config.typed.hidden_dim
        self.attention_norm = nn.LayerNorm(width)
        self.attention = EntitySelfAttention(config)
        self.ffn_norm = nn.LayerNorm(width)
        self.ffn = nn.Sequential(nn.Linear(width, config.ffn_dim), nn.ReLU(), nn.Linear(config.ffn_dim, width))
        self.film = nn.Linear(8, 2 * width)
        nn.init.zeros_(self.film.weight)
        nn.init.zeros_(self.film.bias)

    def forward(self, tokens: Tensor, condition: Tensor, record_mask: Tensor, attention_bias: Tensor) -> Tensor:
        valid = record_mask.unsqueeze(-1).to(tokens.dtype)
        tokens = (tokens + self.attention(self.attention_norm(tokens), record_mask, attention_bias)) * valid
        residual = self.ffn(self.ffn_norm(tokens))
        gamma, beta = self.film(condition).chunk(2, dim=-1)
        residual = residual * (1 + gamma.unsqueeze(1)) + beta.unsqueeze(1)
        return (tokens + residual) * valid


def _tensor_hash(state: dict[str, Tensor]) -> str:
    digest = hashlib.sha256()
    for name, value in sorted(state.items()):
        array = value.detach().cpu().contiguous()
        digest.update(name.encode())
        digest.update(str(array.dtype).encode())
        digest.update(str(tuple(array.shape)).encode())
        digest.update(array.numpy().tobytes())
    return digest.hexdigest()


class EntityTransformer(nn.Module):
    """Pre-LN Transformer over public records with common typed scorer."""

    def __init__(self, config: EntityTransformerConfig):
        super().__init__()
        self.config = config
        self.training_mode = "base"
        self.merged = False
        self.typed_context = TypedContextEncoder(config.typed)
        self.blocks = nn.ModuleList(EntityTransformerBlock(config) for _ in range(config.blocks))
        self.final_norm = nn.LayerNorm(config.typed.hidden_dim)
        self.relation_bias_forward = nn.Linear(config.typed.hidden_dim, config.heads, bias=False)
        self.relation_bias_reverse = nn.Linear(config.typed.hidden_dim, config.heads, bias=False)
        self.signed_coord_bias = nn.Parameter(torch.zeros(config.heads, 2))
        self.absolute_coord_bias = nn.Parameter(torch.zeros(config.heads, 2))
        self.state_context = nn.Sequential(nn.Linear(2 * config.typed.hidden_dim, config.typed.hidden_dim), nn.ReLU())
        self.scorer = CandidateScorer(config.typed)
        if sum(parameter.numel() for parameter in self.parameters()) > 64_000_000:
            raise ValueError("Transformer exceeds the 64 million parameter budget")
        self.configure_training("base")

    def _attention_bias(
        self, record_coord: Tensor, record_spatial_valid: Tensor, record_mask: Tensor,
        relation_index: Tensor, relation_category: Tensor, relation_numeric: Tensor,
        relation_mask: Tensor,
    ) -> Tensor:
        batch, count = record_mask.shape
        spatial = record_spatial_valid & record_mask
        coordinates = torch.where(spatial.unsqueeze(-1), record_coord, torch.zeros_like(record_coord))
        row = coordinates[..., 0]
        col = coordinates[..., 1]
        dr = row.unsqueeze(2) - row.unsqueeze(1)
        dc = col.unsqueeze(2) - col.unsqueeze(1)
        signed = self.signed_coord_bias
        absolute = self.absolute_coord_bias
        bias = (dr.unsqueeze(1) * signed[None, :, 0, None, None]
                + dc.unsqueeze(1) * signed[None, :, 1, None, None]
                + dr.abs().unsqueeze(1) * absolute[None, :, 0, None, None]
                + dc.abs().unsqueeze(1) * absolute[None, :, 1, None, None])
        bias = bias * (spatial[:, None, :, None] & spatial[:, None, None, :]).to(bias.dtype)

        edges = self.typed_context.relation_tokens(relation_category, relation_numeric, relation_mask)
        forward = self.relation_bias_forward(edges).transpose(1, 2)
        reverse = self.relation_bias_reverse(edges).transpose(1, 2)
        safe_index = torch.where(relation_mask.unsqueeze(-1), relation_index, torch.zeros_like(relation_index))
        source, target = safe_index[..., 0], safe_index[..., 1]
        edge_valid = relation_mask.unsqueeze(1).to(bias.dtype)
        flat = torch.zeros(batch, self.config.heads, count * count, dtype=bias.dtype, device=bias.device)
        flat = flat.scatter_add(2, (source * count + target).unsqueeze(1).expand(-1, self.config.heads, -1), forward * edge_valid)
        flat = flat.scatter_add(2, (target * count + source).unsqueeze(1).expand(-1, self.config.heads, -1), reverse * edge_valid)
        return bias + flat.reshape(batch, self.config.heads, count, count)

    def forward(
        self, record_category: Tensor, record_numeric: Tensor, record_coord: Tensor,
        record_spatial_valid: Tensor, record_mask: Tensor, relation_index: Tensor,
        relation_category: Tensor, relation_numeric: Tensor, relation_mask: Tensor,
        candidate_category: Tensor, candidate_numeric: Tensor, candidate_coord: Tensor,
        candidate_coord_valid: Tensor, candidate_parent: Tensor, candidate_order: Tensor,
        candidate_target_index: Tensor, candidate_node_mask: Tensor, candidate_mask: Tensor,
        condition: Tensor,
    ) -> tuple[Tensor, Tensor]:
        records = self.typed_context(record_category, record_numeric, record_coord, record_spatial_valid,
                                     record_mask, relation_index, relation_category, relation_numeric, relation_mask)
        bias = self._attention_bias(record_coord, record_spatial_valid, record_mask, relation_index,
                                    relation_category, relation_numeric, relation_mask)
        for block in self.blocks:
            records = block(records, condition, record_mask, bias)
        records = self.final_norm(records) * record_mask.unsqueeze(-1).to(records.dtype)
        context = self.state_context(torch.cat((records[:, 0], masked_mean(records, record_mask)), dim=-1))
        return self.scorer(context, records, candidate_category, candidate_numeric, candidate_coord,
                           candidate_coord_valid, candidate_parent, candidate_order, candidate_target_index,
                           candidate_node_mask, candidate_mask)

    def validate_inputs(self, *inputs: Tensor) -> None:
        validate_typed_inputs(self.config.typed, inputs)
        (record_category, _, _, _, _, relation_index, _, _, _, candidate_category, _, _, _,
         _, _, _, _, _, _) = inputs
        batch, records = record_category.shape[:2]
        relations = relation_index.shape[1]
        actions, nodes = candidate_category.shape[1:3]
        if batch > 64 or records > 2048 or relations > 8192 or actions > 4096 or nodes > 64:
            raise ValueError("typed Transformer input exceeds a declared axis limit")
        input_bytes = sum(tensor.numel() * tensor.element_size() for tensor in inputs)
        if input_bytes > 64 * 1024 * 1024:
            raise ValueError("typed Transformer input exceeds 64 MiB")
        width, heads = self.config.typed.hidden_dim, self.config.heads
        estimated_working_bytes = (3 * batch * heads * records * records + 8 * batch * records * width
                                   + 6 * batch * actions * nodes * width) * 4
        if estimated_working_bytes > 256 * 1024 * 1024:
            raise ValueError("typed Transformer attention and candidate buffers exceed 256 MiB")
        if next(self.parameters()).device != inputs[0].device:
            raise ValueError("model and typed inputs must share one device")

    def evaluate(self, *inputs: Tensor) -> tuple[Tensor, Tensor]:
        self.validate_inputs(*inputs)
        if self.training:
            raise ValueError("evaluate requires model.eval()")
        with torch.no_grad():
            outputs = self(*inputs)
        if any(not bool(torch.isfinite(value).all()) for value in outputs):
            raise ValueError("Transformer produced nonfinite outputs")
        return outputs

    def configure_training(self, mode: str):
        if mode not in ("base", "adapter") or self.merged:
            raise ValueError("choose base or adapter training on an unmerged Transformer")
        self.training_mode = mode
        for name, parameter in self.named_parameters():
            parameter.grad = None
            parameter.requires_grad_(name.endswith((".lora_a", ".lora_b")) == (mode == "adapter"))
        for module in self.modules():
            if isinstance(module, LoRALinear):
                module.enabled = mode == "adapter"
        self.train(True)
        return (parameter for parameter in self.parameters() if parameter.requires_grad)

    def base_state(self) -> dict[str, Tensor]:
        return {name: value for name, value in self.state_dict().items() if not name.endswith((".lora_a", ".lora_b"))}

    def adapter_state(self) -> dict[str, Tensor]:
        return {name: value for name, value in self.state_dict().items() if name.endswith((".lora_a", ".lora_b"))}

    @property
    def base_hash(self) -> str:
        return _tensor_hash(self.base_state())

    def adapter_descriptor(self, encoder_hash: str) -> TransformerAdapterDescriptor:
        return TransformerAdapterDescriptor(self.base_hash, self.config.digest, encoder_hash)

    def merged_copy(self, descriptor: TransformerAdapterDescriptor, encoder_hash: str) -> EntityTransformer:
        descriptor.validate_merge()
        if (self.merged or descriptor.base_hash != self.base_hash or descriptor.config_hash != self.config.digest
                or descriptor.encoder_hash != encoder_hash):
            raise ValueError("Transformer adapter and base/encoder compatibility mismatch")
        merged = deepcopy(self)
        with torch.no_grad():
            for module in merged.modules():
                if isinstance(module, LoRALinear):
                    module.base.weight.add_(module.delta())
                    module.enabled = False
                    module.lora_a.zero_()
                    module.lora_b.zero_()
        merged.merged = True
        merged.eval()
        for parameter in merged.parameters():
            parameter.requires_grad_(False)
        return merged
