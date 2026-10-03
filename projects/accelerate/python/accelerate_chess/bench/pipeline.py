"""Capability report for cross-game leaf batching, without invented throughput."""
from __future__ import annotations

import argparse
import os

from .common import positive, report, workers


def run(args):
    if args.workers > (os.cpu_count() or 1):
        raise ValueError("workers exceed logical CPU count")
    if not 1 <= args.max_inference_batch <= 64 or not 0 <= args.batch_wait_us <= 1_000_000:
        raise ValueError("batch and wait must be within the current bounded search contract")
    if not 0 <= args.seed < 2**32:
        raise ValueError("seed must be uint32")
    reason = ("InformationSetSearch currently batches leaves within one decision. "
              "It has no shared cross-game inference request queue, dispatch accounting, "
              "or complete source-backed game throughput contract.")
    values = {name: {"status": "unsupported", "reason": reason} for name in (
        "simulations_per_second", "decisions_per_second", "nn_requests_per_second",
        "average_inference_batch", "p50_inference_batch", "p95_inference_batch",
        "queue_wait_ms", "cpu_elapsed", "wall_elapsed", "games_per_hour", "positions_per_hour")}
    return report("pipeline", vars(args), values, output_root=args.artifact_root, run_id=args.run_id)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workers", type=workers, default=1)
    parser.add_argument("--concurrent-games", type=positive, default=2)
    parser.add_argument("--mcts-simulations", type=positive, default=32)
    parser.add_argument("--max-inference-batch", type=positive, default=4)
    parser.add_argument("--batch-wait-us", type=int, default=500)
    parser.add_argument("--device", choices=["cpu", "cuda"], default="cpu")
    parser.add_argument("--dtype", choices=["fp32", "bf16", "fp16"], default="fp32")
    parser.add_argument("--seed", type=int, default=37)
    parser.add_argument("--artifact-root")
    parser.add_argument("--run-id")
    run(parser.parse_args())


if __name__ == "__main__":
    main()
