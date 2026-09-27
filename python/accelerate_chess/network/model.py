"""Residual policy/value model with explicit FiLM and separate static LoRA."""

from __future__ import annotations

from copy import deepcopy
from dataclasses import asdict, dataclass
import hashlib
import math
from typing import Any, Iterable

import torch
from torch import Tensor, nn

from ..encoding import canonical_json

MAX_MODEL_PARAMETERS = 64_000_000
MAX_WORKING_ELEMENTS = 256 * 1024 * 1024 // 4


@dataclass(frozen=True)
class ModelConfig:
    board_channels: int
    condition_dim: int
    action_dim: int
    channels: int = 128
    residual_blocks: int = 8
    lora_rank: int = 8
    lora_alpha: float = 8.
    lora_dropout: float = 0.
    architecture_version: str = "resnet-film-action-v1"

    def __post_init__(self) -> None:
        for name in ("board_channels", "condition_dim", "action_dim", "channels", "residual_blocks", "lora_rank"):
            value = getattr(self, name)
            if type(value) is not int or not 1 <= value <= 1_048_576:
                raise ValueError(f"{name} must be a positive bounded integer")
        if self.channels > 4096 or self.residual_blocks > 64 or self.lora_rank > 4096:
            raise ValueError("model channels/rank/block count exceed runtime architecture limits")
        if not math.isfinite(self.lora_alpha) or self.lora_alpha <= 0 or self.lora_dropout != 0:
            raise ValueError("static convolution LoRA requires positive alpha and dropout 0")
        if self.architecture_version != "resnet-film-action-v1":
            raise ValueError("unsupported model architecture version")
        if self.estimated_parameters > MAX_MODEL_PARAMETERS:
            raise ValueError("model aggregate parameter budget exceeds 64 million")
        self.validate_working_set(1, 1)

    @property
    def estimated_parameters(self) -> int:
        """Conservative allocation budget identical to the Rust manifest gate."""
        c, r = self.channels, self.lora_rank
        return c * self.board_channels * 9 + c * self.condition_dim + c * self.action_dim + self.residual_blocks * (20 * c * c + 20 * r * c + 8 * c) + 4 * c * c + 8 * c

    def validate_working_set(self, batch: int, candidates: int) -> None:
        inputs = batch * (64 * self.board_channels + self.condition_dim + candidates * self.action_dim)
        activations = batch * self.channels * (256 + 2 * candidates)
        if inputs + activations > MAX_WORKING_ELEMENTS:
            raise ValueError("model input and activation working set exceeds 256 MiB")

    @property
    def digest(self) -> str:
        return hashlib.sha256(canonical_json(asdict(self)).encode()).hexdigest()


@dataclass(frozen=True)
class AdapterDescriptor:
    base_hash: str
    config_hash: str
    encoder_hash: str
    generator: str = "static-lora"
    condition_lifetime: str = "global"
    mergeable: bool = True
    version: str = "lora-convolution-v1"

    def validate_merge(self) -> None:
        if not self.mergeable or self.generator != "static-lora" or self.condition_lifetime != "global":
            raise ValueError("only globally fixed static adapters can be merged")
        if self.version != "lora-convolution-v1":
            raise ValueError("unsupported adapter version")


class LoRAConv2d(nn.Module):
    def __init__(self, channels: int, rank: int, alpha: float):
        super().__init__()
        self.base = nn.Conv2d(channels, channels, 3, padding=1, bias=False)
        self.lora_a = nn.Parameter(torch.empty(rank, channels, 3, 3))
        self.lora_b = nn.Parameter(torch.zeros(channels, rank, 1, 1))
        nn.init.kaiming_uniform_(self.lora_a, a=math.sqrt(5))
        self.scale = alpha / rank
        self.enabled = False

    def forward(self, value: Tensor) -> Tensor:
        result = self.base(value)
        if self.enabled:
            low = nn.functional.conv2d(value, self.lora_a, padding=1)
            result = result + nn.functional.conv2d(low, self.lora_b) * self.scale
        return result

    def delta(self) -> Tensor:
        return torch.einsum("or,rijk->oijk", self.lora_b[:, :, 0, 0], self.lora_a) * self.scale


class ResidualFiLMBlock(nn.Module):
    def __init__(self, config: ModelConfig):
        super().__init__()
        self.conv1 = LoRAConv2d(config.channels, config.lora_rank, config.lora_alpha)
        self.bn1 = nn.BatchNorm2d(config.channels)
        self.conv2 = LoRAConv2d(config.channels, config.lora_rank, config.lora_alpha)
        self.bn2 = nn.BatchNorm2d(config.channels)
        self.film = nn.Linear(config.channels, 2 * config.channels)

    def forward(self, value: Tensor, condition: Tensor) -> Tensor:
        residual = self.conv2(torch.relu(self.bn1(self.conv1(value))))
        residual = self.bn2(residual)
        gamma_delta, beta = self.film(condition).chunk(2, dim=1)
        residual = residual * (1 + gamma_delta[:, :, None, None]) + beta[:, :, None, None]
        return torch.relu(value + residual)


def is_adapter_parameter(name: str) -> bool:
    return name.endswith(".lora_a") or name.endswith(".lora_b")


def tensor_state_hash(state: dict[str, Tensor]) -> str:
    digest = hashlib.sha256()
    for name, value in sorted(state.items()):
        array = value.detach().cpu().contiguous()
        digest.update(name.encode())
        digest.update(str(array.dtype).encode())
        digest.update(str(tuple(array.shape)).encode())
        digest.update(array.numpy().tobytes())
    return digest.hexdigest()


class PolicyValueNetwork(nn.Module):
    def __init__(self, config: ModelConfig):
        super().__init__()
        self.config = config
        self.training_mode = "base"
        self.merged = False
        self.stem = nn.Sequential(nn.Conv2d(config.board_channels, config.channels, 3, padding=1, bias=False), nn.BatchNorm2d(config.channels), nn.ReLU())
        self.blocks = nn.ModuleList(ResidualFiLMBlock(config) for _ in range(config.residual_blocks))
        # A shared learned projection keeps a large lossless public condition
        # from duplicating a large matrix in every residual block.
        self.condition_projection = nn.Sequential(nn.Linear(config.condition_dim, config.channels), nn.LeakyReLU(0.01))
        self.context = nn.Linear(config.channels, config.channels)
        self.action_projection = nn.Linear(config.action_dim, config.channels)
        self.policy_head = nn.Linear(config.channels, 1)
        self.value_head = nn.Sequential(nn.Linear(config.channels, config.channels), nn.ReLU(), nn.Linear(config.channels, 1), nn.Tanh())
        self.configure_training("base")

    def forward(self, board: Tensor, condition: Tensor, action_features: Tensor) -> tuple[Tensor, Tensor]:
        features = self.stem(board)
        condition = self.condition_projection(condition)
        for block in self.blocks:
            features = block(features, condition)
        pooled = features.mean(dim=(2, 3))
        candidates = torch.relu(self.action_projection(action_features) + self.context(pooled)[:, None, :])
        return self.policy_head(candidates).squeeze(-1), self.value_head(pooled)

    def validate_inputs(self, board: Tensor, condition: Tensor, action_features: Tensor) -> None:
        if board.ndim != 4 or board.shape[1:] != (self.config.board_channels, 8, 8) or board.shape[0] < 1:
            raise ValueError("invalid board shape")
        if condition.shape != (board.shape[0], self.config.condition_dim):
            raise ValueError("invalid FiLM condition shape")
        if action_features.ndim != 3 or action_features.shape[0] != board.shape[0] or action_features.shape[1] < 1 or action_features.shape[2] != self.config.action_dim:
            raise ValueError("invalid action features shape")
        self.config.validate_working_set(board.shape[0], action_features.shape[1])
        if any(value.dtype != torch.float32 or not bool(torch.isfinite(value).all()) for value in (board, condition, action_features)):
            raise ValueError("model inputs must be finite float32 tensors")
        if len({value.device for value in (board, condition, action_features, next(self.parameters()))}) != 1:
            raise ValueError("model and input devices differ")

    def evaluate(self, board: Tensor, condition: Tensor, action_features: Tensor) -> tuple[Tensor, Tensor]:
        self.validate_inputs(board, condition, action_features)
        if self.training:
            raise ValueError("evaluate requires model.eval() so normalization statistics remain fixed")
        with torch.no_grad():
            outputs = self(board, condition, action_features)
        if any(not bool(torch.isfinite(value).all()) for value in outputs):
            raise ValueError("model produced nonfinite outputs")
        return outputs

    def configure_training(self, mode: str) -> Iterable[nn.Parameter]:
        if mode not in ("base", "adapter") or self.merged:
            raise ValueError("choose base or adapter training on an unmerged model")
        self.training_mode = mode
        for name, parameter in self.named_parameters():
            # Optimizers consult .grad, not requires_grad, when stepping. Clear
            # stale gradients from the previous mode before freezing weights.
            parameter.grad = None
            parameter.requires_grad_(is_adapter_parameter(name) == (mode == "adapter"))
        for module in self.modules():
            if isinstance(module, LoRAConv2d):
                module.enabled = mode == "adapter"
        self.train(True)
        return (parameter for parameter in self.parameters() if parameter.requires_grad)

    def train(self, mode: bool = True) -> PolicyValueNetwork:
        super().train(mode)
        if mode and self.training_mode == "adapter":
            for module in self.modules():
                if isinstance(module, nn.BatchNorm2d):
                    module.eval()
        return self

    def base_state(self) -> dict[str, Tensor]:
        return {key: value for key, value in self.state_dict().items() if not is_adapter_parameter(key)}

    def adapter_state(self) -> dict[str, Tensor]:
        return {key: value for key, value in self.state_dict().items() if is_adapter_parameter(key)}

    @property
    def base_hash(self) -> str:
        return tensor_state_hash(self.base_state())

    def adapter_descriptor(self, encoder_hash: str) -> AdapterDescriptor:
        return AdapterDescriptor(self.base_hash, self.config.digest, encoder_hash)

    def merged_copy(self, descriptor: AdapterDescriptor, encoder_hash: str) -> PolicyValueNetwork:
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


def masked_policy(logits: Tensor, mask: Tensor) -> Tensor:
    """Terminal rows return zeros; padding can never receive probability mass."""
    if logits.ndim != 2 or mask.shape != logits.shape or mask.dtype != torch.bool or not bool(torch.isfinite(logits).all()):
        raise ValueError("invalid logits or action mask")
    masked = logits.masked_fill(~mask, -torch.inf)
    terminal = ~mask.any(dim=1, keepdim=True)
    safe = torch.where(terminal, torch.zeros_like(masked), masked)
    return torch.softmax(safe, dim=1).masked_fill(~mask, 0.)
