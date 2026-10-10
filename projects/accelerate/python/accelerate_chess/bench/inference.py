"""CUDA throughput of the production fixed8 model on declared synthetic shapes."""
from __future__ import annotations

import argparse
from contextlib import nullcontext
import statistics
import time

from .common import positive, report

PROFILES = {"small": (32, 3), "normal": (80, 8), "monster": (160, 160)}
MODELS = {"resnet-s": (128, 8), "resnet-m": (192, 12)}


def synthetic_batch(batch_size: int, profile: str, device="cpu"):
    """Construct the exact ModelBatch tensor contract; values are synthetic."""
    import torch
    from accelerate_chess.network.architecture_v1 import ModelBatch

    if profile not in PROFILES or not 1 <= batch_size <= 64:
        raise ValueError("profile is unknown or model batch exceeds its 1..64 contract")
    b, (a, n), e = batch_size, PROFILES[profile], 2
    zeros = torch.zeros
    category = zeros((b, e, 6), dtype=torch.long)
    mask = zeros((b, e), dtype=torch.bool)
    mask[:, 0] = True
    candidate_mask = torch.ones((b, a), dtype=torch.bool)
    nodes = torch.ones((b, a, n), dtype=torch.bool)
    candidates = (zeros((b, a, n, 4), dtype=torch.long), zeros((b, a, n, 8)),
                  zeros((b, a, n, 2)), zeros((b, a, n), dtype=torch.bool),
                  torch.full((b, a, n), -1, dtype=torch.long), zeros((b, a, n), dtype=torch.long),
                  torch.full((b, a, n), -1, dtype=torch.long), nodes, candidate_mask)
    geometry = torch.tensor([[0, 0, 8, 8]] * b, dtype=torch.long)
    result = ModelBatch(geometry, category, zeros((b, e, 8)), zeros((b, e, 2)), mask,
                        zeros((b, 1, 2), dtype=torch.long), zeros((b, 1), dtype=torch.long),
                        zeros((b, 1), dtype=torch.bool), zeros((b, e, 8, 8)),
                        zeros((b, 4, 8, 8)), zeros((b, 8)), candidates)
    return result.to(device)


def model_config(name: str):
    from accelerate_chess.network.typed_context import TypedContextConfig
    from accelerate_chess.network.architecture_v1 import Fixed8x8ResNet

    channels, blocks = MODELS[name]
    context = TypedContextConfig((4096,) * 4, (4096,) * 2, (4096,) * 4, hidden_dim=64)
    return Fixed8x8ResNet(context, channels=channels, residual_blocks=blocks).eval()


def run(args):
    import torch

    if args.device != "cuda" or not torch.cuda.is_available():
        raise RuntimeError("CUDA unavailable; this benchmark requires an actual CUDA device")
    if args.dtype == "bf16" and not torch.cuda.is_bf16_supported():
        raise RuntimeError("CUDA BF16 unsupported on this device")
    if any(b < 1 or b > 256 for b in args.batch_sizes):
        raise ValueError("batch sizes must be within 1..256")
    torch.manual_seed(args.seed)
    torch.cuda.manual_seed_all(args.seed)
    model = model_config(args.model).to("cuda")
    parameter_count = sum(p.numel() for p in model.parameters())
    result = []
    for profile in args.profiles:
        actions, nodes = PROFILES[profile]
        for b in args.batch_sizes:
            base = {"model_name": args.model, "parameter_count": parameter_count,
                    "dtype": args.dtype, "batch_size": b, "candidate_count": actions,
                    "candidate_nodes": nodes, "synthetic": True,
                    "warmup_iterations": args.warmup, "iterations": args.iterations,
                    "device_name": torch.cuda.get_device_name(), "torch_version": torch.__version__,
                    "cuda_version": torch.version.cuda}
            if b > 64:
                result.append({**base, "status": "unsupported", "reason": "ModelBatch contract limits B to 64"})
                continue
            try:
                batch = synthetic_batch(b, profile, "cuda")
                model._validate(batch)
                context = nullcontext() if args.dtype == "fp32" else torch.autocast("cuda", dtype={"bf16": torch.bfloat16, "fp16": torch.float16}[args.dtype])
                torch.cuda.reset_peak_memory_stats()
                times = []
                with torch.inference_mode(), context:
                    for _ in range(args.warmup):
                        model(batch)
                    torch.cuda.synchronize()
                    for _ in range(args.iterations):
                        start = time.perf_counter()
                        logits, value = model(batch)
                        torch.cuda.synchronize()
                        times.append((time.perf_counter() - start) * 1000)
                if logits.shape != (b, actions) or value.shape != (b, 1) or not torch.isfinite(logits).all() or not torch.isfinite(value).all():
                    raise ValueError("production model output contract failed")
                result.append({**base, "status": "ok", "latency_mean_ms": statistics.mean(times),
                               "latency_p50_ms": statistics.median(times),
                               "latency_p95_ms": sorted(times)[min(len(times)-1, int(.95 * len(times)))],
                               "positions_per_second": b * len(times) / (sum(times) / 1000),
                               "peak_vram_bytes": torch.cuda.max_memory_allocated()})
            except torch.cuda.OutOfMemoryError as error:
                result.append({**base, "status": "oom", "reason": str(error)})
                torch.cuda.empty_cache()
            except ValueError as error:
                if "exceeds 64 MiB" not in str(error):
                    raise
                result.append({**base, "status": "unsupported", "reason": str(error)})
    return report("inference", vars(args), {"measurements": result}, output_root=args.artifact_root, run_id=args.run_id)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", choices=MODELS, default="resnet-s")
    parser.add_argument("--profiles", nargs="+", choices=PROFILES, default=["small", "normal", "monster"])
    parser.add_argument("--batch-sizes", type=positive, nargs="+", default=[1, 2, 4, 8, 16, 32, 64, 128, 256])
    parser.add_argument("--warmup", type=positive, default=3)
    parser.add_argument("--iterations", type=positive, default=10)
    parser.add_argument("--dtype", choices=["fp32", "bf16", "fp16"], default="fp32")
    parser.add_argument("--device", choices=["cuda"], default="cuda")
    parser.add_argument("--seed", type=int, default=37)
    parser.add_argument("--artifact-root")
    parser.add_argument("--run-id")
    run(parser.parse_args())


if __name__ == "__main__":
    main()
