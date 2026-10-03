"""Finite synthetic optimizer throughput for the production fixed8 model."""
from __future__ import annotations

import argparse
import statistics
import time

from .common import positive, report
from .inference import MODELS, PROFILES, model_config, synthetic_batch
from accelerate_chess.training import (TRAINING_DTYPES, training_forward,
    validate_fp32_training_state, validate_training_dtype)


def run(args):
    import torch
    from accelerate_chess.replay import artifact_root, reserve_slot

    validate_training_dtype(args.dtype, args.device)
    if args.device == "cuda" and not torch.cuda.is_available():
        raise RuntimeError("CUDA unavailable; choose --device cpu or install the opt-in CUDA environment")
    if not 1 <= args.batch_size <= 64 or not 1 <= args.steps <= 100:
        raise ValueError("training smoke is bounded to batch 1..64 and steps 1..100")
    torch.manual_seed(args.seed)
    if args.device == "cuda":
        torch.cuda.manual_seed_all(args.seed)
        torch.cuda.reset_peak_memory_stats()
    model = model_config(args.model).to(args.device).train()
    optimizer = torch.optim.AdamW(model.parameters(), lr=3e-4)
    validate_fp32_training_state(model, optimizer)
    timings = {key: [] for key in ("data_load_ms", "forward_ms", "backward_ms", "optimizer_ms", "step_ms")}

    def sync():
        if args.device == "cuda":
            torch.cuda.synchronize()

    started = time.perf_counter()
    for _ in range(args.steps):
        step_start = time.perf_counter()
        load_start = step_start
        batch = synthetic_batch(args.batch_size, args.profile, args.device)
        model._validate(batch)
        sync()
        timings["data_load_ms"].append((time.perf_counter() - load_start) * 1000)
        optimizer.zero_grad(set_to_none=True)
        forward_start = time.perf_counter()
        logits, value = training_forward(model, (batch,), dtype=args.dtype)
        loss = -torch.log_softmax(logits, 1).mean() + torch.nn.functional.mse_loss(value, torch.zeros_like(value))
        sync()
        timings["forward_ms"].append((time.perf_counter() - forward_start) * 1000)
        if not bool(torch.isfinite(loss)):
            raise ValueError("nonfinite synthetic loss")
        backward_start = time.perf_counter()
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 5., error_if_nonfinite=True)
        sync()
        timings["backward_ms"].append((time.perf_counter() - backward_start) * 1000)
        optimizer_start = time.perf_counter()
        optimizer.step()
        sync()
        timings["optimizer_ms"].append((time.perf_counter() - optimizer_start) * 1000)
        timings["step_ms"].append((time.perf_counter() - step_start) * 1000)
    elapsed = time.perf_counter() - started
    validate_fp32_training_state(model, optimizer)
    checkpoint = None
    if args.run_id:
        checkpoint = reserve_slot(artifact_root(args.artifact_root), "models", args.run_id) / "smoke.pt"
        torch.save({"model": model.state_dict(), "optimizer": optimizer.state_dict(),
                    "steps": args.steps, "synthetic": True, "dtype": args.dtype}, checkpoint)
    results = {key: statistics.mean(value) for key, value in timings.items() if key != "step_ms"}
    results.update({"status": "ok", "synthetic": True, "steps": args.steps, "dtype": args.dtype,
                    "samples_per_second": args.steps * args.batch_size / elapsed,
                    "steps_per_second": args.steps / elapsed,
                    "mean_step_ms": statistics.mean(timings["step_ms"]),
                    "peak_vram_bytes": torch.cuda.max_memory_allocated() if args.device == "cuda" else 0,
                    "checkpoint": str(checkpoint) if checkpoint else None,
                    "replay_loading": "unsupported: use accelerate_chess.cli train with terminal replay"})
    return report("training", vars(args), results, output_root=args.artifact_root, run_id=args.run_id)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", choices=MODELS, default="resnet-s")
    parser.add_argument("--profile", choices=PROFILES, default="small")
    parser.add_argument("--steps", type=positive, default=10)
    parser.add_argument("--batch-size", type=positive, default=2)
    parser.add_argument("--device", choices=["cpu", "cuda"], default="cpu")
    parser.add_argument("--dtype", choices=TRAINING_DTYPES, default="fp32")
    parser.add_argument("--seed", type=int, default=37)
    parser.add_argument("--artifact-root")
    parser.add_argument("--run-id")
    run(parser.parse_args())


if __name__ == "__main__":
    main()
