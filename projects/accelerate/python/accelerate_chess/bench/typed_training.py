"""Finite synthetic optimizer benchmark for production typed architectures."""
from __future__ import annotations

import argparse
from dataclasses import asdict
import os
import statistics
import time

import numpy as np

from accelerate_chess.ir import INPUT_ORDER_A, TypedBatch
from accelerate_chess.network import (EntityTransformer, EntityTransformerConfig,
    MaskResNetConfig, MaskResNetPolicyValueNetwork, TypedContextConfig)
from accelerate_chess.replay import artifact_root, reserve_slot
from accelerate_chess.training import (TRAINING_DTYPES, training_forward,
    validate_fp32_training_state, validate_training_dtype)
from .common import report, workers


# Synthetic load shapes; these are not estimates of game position distributions.
PROFILES = {
    "small": (8, 8, 32, 3, 8, 8),
    "normal": (16, 24, 80, 8, 8, 8),
    "monster": (32, 64, 160, 160, 8, 8),
}


def synthetic_batch(batch_size: int, profile: str, seed: int, vocabulary: int) -> TypedBatch:
    """Create one typed state which can be projected into either family order."""
    if profile not in PROFILES or not 1 <= batch_size <= 64:
        raise ValueError("typed profile or batch size is outside the benchmark bounds")
    records, relations, actions, nodes, height, width = PROFILES[profile]
    rng = np.random.default_rng(seed)
    f = lambda *shape: rng.random(shape, dtype=np.float32)
    z = lambda *shape: np.zeros(shape, dtype=np.int64)
    valid = lambda *shape: np.ones(shape, dtype=np.bool_)
    cells = rng.integers(0, height * width, (batch_size, records), dtype=np.int64)
    coordinates = np.stack((cells // width, cells % width), axis=-1).astype(np.float32)
    coordinates[..., 0] /= max(height - 1, 1)
    coordinates[..., 1] /= max(width - 1, 1)
    spatial = np.zeros((batch_size, 6, height, width), dtype=np.float32)
    spatial[:, 0] = 1.
    for row in range(batch_size):
        occupied = cells[row, 1:]
        spatial[row, 0, occupied // width, occupied % width] = 0.
        spatial[row, 3, occupied // width, occupied % width] = 1.
    targets = rng.integers(0, records, (batch_size, actions, nodes), dtype=np.int64)
    candidate_coordinates = np.take_along_axis(
        coordinates[:, None, :, :], targets[..., None], axis=2)
    candidate_mask = valid(batch_size, actions)
    candidate_mask[:, -1] = False
    node_mask = np.broadcast_to(candidate_mask[..., None], (batch_size, actions, nodes)).copy()
    arrays = {
        "spatial": spatial,
        "layout_mask": valid(batch_size, 1, height, width),
        "record_category": rng.integers(0, vocabulary, (batch_size, records, 4), dtype=np.int64),
        "record_numeric": f(batch_size, records, 8),
        "record_coord": coordinates,
        "record_spatial_valid": valid(batch_size, records),
        "record_mask": valid(batch_size, records),
        "relation_index": rng.integers(0, records, (batch_size, relations, 2), dtype=np.int64),
        "relation_category": rng.integers(0, vocabulary, (batch_size, relations, 2), dtype=np.int64),
        "relation_numeric": f(batch_size, relations, 4),
        "relation_mask": valid(batch_size, relations),
        "candidate_category": rng.integers(0, vocabulary, (batch_size, actions, nodes, 4), dtype=np.int64),
        "candidate_numeric": f(batch_size, actions, nodes, 8),
        "candidate_coord": candidate_coordinates,
        "candidate_coord_valid": node_mask.copy(),
        "candidate_parent": np.broadcast_to(np.arange(nodes, dtype=np.int64) - 1,
            (batch_size, actions, nodes)).copy(),
        "candidate_order": np.broadcast_to(np.arange(nodes, dtype=np.int64),
            (batch_size, actions, nodes)).copy(),
        "candidate_target_index": targets,
        "candidate_node_mask": node_mask,
        "candidate_mask": candidate_mask,
        "condition": f(batch_size, 8),
    }
    return TypedBatch(arrays, (), "synthetic-typed-training-v1")


def model_for(family: str, vocabulary: int):
    context = TypedContextConfig((vocabulary,) * 4, (vocabulary,) * 2,
                                 (vocabulary,) * 4, hidden_dim=128)
    if family == "mask-resnet":
        config = MaskResNetConfig(6, context, channels=128, residual_blocks=8,
                                  lora_rank=8, lora_alpha=8.)
        return MaskResNetPolicyValueNetwork(config)
    if family == "entity-transformer":
        config = EntityTransformerConfig(context, blocks=4, heads=4, ffn_dim=512,
                                         lora_rank=8, lora_alpha=8.)
        return EntityTransformer(config)
    raise ValueError(f"unknown typed architecture family: {family!r}")


def run(args):
    import torch
    from accelerate_chess.cli import default_spec

    if args.model_family not in ("mask-resnet", "entity-transformer") or args.profile not in PROFILES:
        raise ValueError("unknown typed model family or profile")
    if not 1 <= args.batch_size <= 64 or not 1 <= args.steps <= 100 or not 0 <= args.warmup <= 20:
        raise ValueError("typed training is bounded to batch 1..64, steps 1..100, warmup 0..20")
    cpu_count = os.cpu_count() or 1
    if not 1 <= args.torch_threads <= cpu_count or not 1 <= args.torch_interop_threads <= cpu_count:
        raise ValueError("PyTorch thread counts must be within 1..logical CPU count")
    if args.device not in ("cpu", "cuda"):
        raise ValueError("unknown device")
    validate_training_dtype(args.dtype, args.device)
    if args.device == "cuda" and not torch.cuda.is_available():
        raise RuntimeError("CUDA unavailable")
    # Check the external artifact boundary even for stdout-only runs.
    root = artifact_root(args.artifact_root)
    torch.set_num_threads(args.torch_threads)
    if torch.get_num_interop_threads() != args.torch_interop_threads:
        torch.set_num_interop_threads(args.torch_interop_threads)
    torch.manual_seed(args.seed)
    if args.device == "cuda":
        torch.cuda.manual_seed_all(args.seed)
    spec = default_spec(model_family=args.model_family)
    vocabulary = len(spec.category_vocabulary)
    model = model_for(args.model_family, vocabulary).to(args.device).train()
    optimizer = torch.optim.AdamW((p for p in model.parameters() if p.requires_grad), lr=3e-4)
    validate_fp32_training_state(model, optimizer)
    state = synthetic_batch(args.batch_size, args.profile, args.seed, vocabulary)
    arrays = state.as_family_inputs(args.model_family)
    inputs = tuple(torch.from_numpy(array).to(args.device) for array in arrays)
    model.validate_inputs(*inputs)
    mask = torch.from_numpy(state.candidate_mask).to(args.device)
    policy = mask.float() / mask.sum(dim=1, keepdim=True)
    target_value = torch.zeros((args.batch_size, 1), device=args.device)
    trainable = [p for p in model.parameters() if p.requires_grad]

    def sync():
        if args.device == "cuda":
            torch.cuda.synchronize()

    def step(measure: bool):
        timings = {}
        optimizer.zero_grad(set_to_none=True)
        start = time.perf_counter()
        logits, value = training_forward(model, inputs, dtype=args.dtype)
        log_policy = torch.log_softmax(logits.masked_fill(~mask, -torch.inf), dim=1)
        loss = -(policy * log_policy.masked_fill(~mask, 0)).sum(dim=1).mean()
        loss = loss + torch.nn.functional.mse_loss(value, target_value)
        if not bool(torch.isfinite(loss)):
            raise ValueError("nonfinite synthetic policy/value loss")
        sync()
        timings["forward_ms"] = (time.perf_counter() - start) * 1000
        start = time.perf_counter()
        loss.backward()
        torch.nn.utils.clip_grad_norm_(trainable, 5., error_if_nonfinite=True)
        sync()
        timings["backward_ms"] = (time.perf_counter() - start) * 1000
        start = time.perf_counter()
        optimizer.step()
        sync()
        timings["optimizer_ms"] = (time.perf_counter() - start) * 1000
        return timings if measure else None

    for _ in range(args.warmup):
        step(False)
    sync()
    if args.device == "cuda":
        torch.cuda.reset_peak_memory_stats()
    samples = {key: [] for key in ("data_load_ms", "forward_ms", "backward_ms", "optimizer_ms", "step_ms")}
    started = time.perf_counter()
    for _ in range(args.steps):
        step_start = time.perf_counter()
        # The prepared synthetic batch is reused; transfer/encoding is outside throughput.
        samples["data_load_ms"].append(0.)
        for key, value in step(True).items():
            samples[key].append(value)
        samples["step_ms"].append((time.perf_counter() - step_start) * 1000)
    sync()
    elapsed = time.perf_counter() - started
    peak_vram = torch.cuda.max_memory_allocated() if args.device == "cuda" else 0
    validate_fp32_training_state(model, optimizer)
    checkpoint = None
    if args.run_id:
        validate_fp32_training_state(model, optimizer)
        checkpoint = reserve_slot(root, "models", args.run_id) / "smoke.pt"
        torch.save({"format": "typed-training-benchmark-smoke-v1", "model": model.state_dict(),
                    "optimizer": optimizer.state_dict(), "architecture_family": args.model_family,
                    "architecture_config": asdict(model.config), "dtype": args.dtype,
                    "synthetic": True, "steps": args.steps, "warmup": args.warmup}, checkpoint)
    shape = dict(zip(("record_count", "relation_count", "candidate_count", "candidate_node_count",
                      "board_height", "board_width"), PROFILES[args.profile]))
    results = {key: statistics.mean(values) for key, values in samples.items() if key != "step_ms"}
    results.update({"status": "ok", "synthetic": True, "steps": args.steps,
                    "warmup": args.warmup, "timed_samples": args.steps * args.batch_size,
                    "elapsed": elapsed, "mean_step_ms": statistics.mean(samples["step_ms"]),
                    "samples_per_second": args.steps * args.batch_size / elapsed,
                    "steps_per_second": args.steps / elapsed, "peak_vram_bytes": peak_vram,
                    "parameter_count": sum(p.numel() for p in model.parameters()),
                    "trainable_parameter_count": sum(p.numel() for p in trainable),
                    "parameter_bytes": sum(p.numel() * p.element_size() for p in model.parameters()),
                    "architecture_config": asdict(model.config), "workload_shape": {"batch_size": args.batch_size, **shape},
                    "torch_threads": torch.get_num_threads(),
                    "torch_interop_threads": torch.get_num_interop_threads(),
                    "checkpoint": str(checkpoint) if checkpoint else None,
                    "replay_loading": "unsupported: synthetic prepared typed batch"})
    config = {**vars(args), "torch_threads": torch.get_num_threads(),
              "torch_interop_threads": torch.get_num_interop_threads()}
    return report("typed-training", config, results, output_root=root, run_id=args.run_id)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model-family", choices=("mask-resnet", "entity-transformer"), required=True)
    parser.add_argument("--profile", choices=PROFILES, default="small")
    parser.add_argument("--steps", type=int, default=10)
    parser.add_argument("--batch-size", type=int, default=2)
    parser.add_argument("--warmup", type=int, default=3)
    parser.add_argument("--device", choices=("cpu", "cuda"), default="cpu")
    parser.add_argument("--dtype", choices=TRAINING_DTYPES, default="fp32")
    parser.add_argument("--seed", type=int, default=37)
    parser.add_argument("--torch-threads", type=workers, default=1)
    parser.add_argument("--torch-interop-threads", type=workers, default=1)
    parser.add_argument("--artifact-root")
    parser.add_argument("--run-id")
    run(parser.parse_args())


if __name__ == "__main__":
    main()
