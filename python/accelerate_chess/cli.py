"""Finite developer commands; export/evaluation never activate a model.

The activation reference records its validation backend as provenance. Each
execution command selects its backend explicitly (ort by default); the stored
reference never silently switches that selection to tract.
"""
from __future__ import annotations

import argparse
from dataclasses import asdict
import json
import os
from pathlib import Path
import signal
import sys
import time

import numpy as np
import torch

from .encoding import EncoderSpec, PublicEncoder, canonical_json
from .inference import ProductionEvaluator
from .network.artifacts import export_onnx, file_sha256, load_adapter, load_base, load_manifest, save_adapter, save_base
from .network.model import ModelConfig, PolicyValueNetwork
from .replay import EpisodeRecorder, ReplayEpisode, artifact_root, atomic_json, read_json, slot
from .search import BeliefLimits, InformationSetSearch, NativeSourceFactory, ParticleBelief, PublicTracker, SearchLimits
from .training import DatasetCursor, ReplayDataset, TrainingLimits, create_optimizer, load_training_checkpoint, optimize, save_training_checkpoint


def default_spec(catalog_path: str | None = None, *, observation_policy: dict | None = None):
    if observation_policy is None:
        from ._native import site_observation_policy
        observation_policy = site_observation_policy()
    if catalog_path:
        catalog = read_json(catalog_path)
    else:
        from ._native import site_catalog
        catalog = site_catalog()
    return EncoderSpec.from_catalog(catalog, observation_policy=observation_policy, history_encoding="public-history-summary-v1", action_encoding="public-decision-intent-v1")


def _configuration(args):
    config = read_json(args.config) if args.config else {"gameStyle": "normal"}
    if not isinstance(config, dict):
        raise ValueError("configuration must be a public GameConfig object")
    return config


def _manifest(args, root, spec):
    if args.manifest:
        return Path(args.manifest).resolve()
    activation = read_json(slot(root, "models", "active") / "activation.json")
    if set(activation) != {"version", "manifest", "manifest_sha256", "model_sha256", "encoder_hash", "backend"} or activation["version"] != "accelerate-activation-v1" or activation["encoder_hash"] != spec.digest or activation["backend"] not in ("ort", "tract"):
        raise ValueError("active model reference is incompatible")
    path = Path(activation["manifest"])
    if file_sha256(path) != activation["manifest_sha256"] or load_manifest(path, spec)["model_sha256"] != activation["model_sha256"]:
        raise ValueError("activated artifact changed since explicit activation")
    return path


def _search(args, spec, manifest, backend):
    limits = SearchLimits(iterations=args.iterations, max_depth=args.depth, elapsed_ms=args.search_ms,
                          max_nodes=args.nodes, max_edges=args.edges, max_candidates=args.candidates,
                          leaf_batch_size=args.leaf_batch_size)
    return InformationSetSearch(PublicEncoder(spec), ProductionEvaluator(manifest, spec, backend, threads=args.threads), limits=limits)


def initialize(args, root, spec):
    directory = slot(root, "models", args.slot)
    base = directory / "base.pt"
    if base.exists() and not args.overwrite:
        raise FileExistsError("model slot already contains a base; explicit --overwrite is required")
    torch.manual_seed(args.seed)
    model = PolicyValueNetwork(ModelConfig(spec.board_channels, spec.condition_dim, spec.action_dim,
                    channels=args.channels, residual_blocks=args.blocks, lora_rank=args.rank, lora_alpha=float(args.rank))).eval()
    fingerprint = save_base(model, spec, base)
    manifest = export_onnx(model, spec, directory / "deployment")
    return {"base": str(base), "base_hash": fingerprint, "manifest": str(manifest), "encoder_hash": spec.digest,
            "architecture": {"channels": args.channels, "blocks": args.blocks, "rank": args.rank}, "activated": False}


def choose(args, root, spec, cancelled):
    tracker = PublicTracker.from_snapshot(read_json(args.trace))
    factory = NativeSourceFactory(_configuration(args))
    belief = ParticleBelief(tracker, factory, seed=args.belief_seed,
              limits=BeliefLimits(particles=args.particles, proposals=args.proposals, elapsed_ms=args.belief_ms), cancelled=cancelled)
    search = _search(args, spec, _manifest(args, root, spec), args.backend)
    return asdict(search.run(belief, cancelled=cancelled))


def selfplay(args, root, spec, cancelled):
    from ._native import Position
    if not 1 <= args.games <= 64 or not 1 <= args.max_plies <= 4096 or not 1 <= args.elapsed_ms <= 86_400_000:
        raise ValueError("selfplay needs finite games, plies and elapsed time limits")
    config = _configuration(args)
    output = slot(root, "datasets", args.run_id)
    search = _search(args, spec, _manifest(args, root, spec), args.backend)
    started = time.monotonic()
    episodes = []
    for game in range(args.games):
        if cancelled() or (time.monotonic() - started) * 1000 >= args.elapsed_ms:
            break
        recorder = None
        path = output / f"episode-{game:04d}.json"
        try:
            position = Position.new_game(config, (args.seed + game) % 2**32)
            recorder = EpisodeRecorder({viewer: position.observe(viewer) for viewer in ("white", "black")}, spec,
                        environment_seed=(args.seed + game) % 2**32, belief_seed=args.belief_seed,
                        evidence_kind="bounded-verification" if args.verification else "selfplay",
                        model_sha256=search.evaluator.session.model_sha256)
            limits = BeliefLimits(particles=args.particles, proposals=args.proposals, elapsed_ms=args.belief_ms)
            beliefs = {viewer: ParticleBelief(tracker, NativeSourceFactory(config), seed=args.belief_seed + index,
                        limits=limits, cancelled=cancelled) for index, (viewer, tracker) in enumerate(recorder.trackers.items())}
            reason = "ply-limit"
            for _ in range(args.max_plies):
                if position.result is not None:
                    reason = "source-terminal"
                    break
                if cancelled() or (time.monotonic() - started) * 1000 >= args.elapsed_ms:
                    reason = "cancelled" if cancelled() else "elapsed"
                    break
                actor = position.decision_actor
                if recorder.trackers[actor].latest != position.observe(actor):
                    raise ValueError("environment public projection diverged from the complete replay")
                result = search.run(beliefs[actor], cancelled=cancelled)
                recorder.record_decision(actor, result)
                if cancelled() or result.stop_reason == "cancelled" or (time.monotonic() - started) * 1000 >= args.elapsed_ms:
                    reason = "cancelled" if cancelled() or result.stop_reason == "cancelled" else "elapsed"
                    break
                action = position.bind_public_intent(result.intent)
                if cancelled() or (time.monotonic() - started) * 1000 >= args.elapsed_ms:
                    reason = "cancelled" if cancelled() else "elapsed"
                    break
                child = position.apply(action).position
                recorder.advance({viewer: child.observe(viewer) for viewer in ("white", "black")}, actor=actor, intent=result.intent)
                position = child
            recorder.finish(position.result, "source-terminal" if position.result is not None else reason)
            recorder.save(path)
            episodes.append({"path": str(path), **recorder.outcome})
            if reason in ("cancelled", "elapsed"):
                break
        except (Exception, KeyboardInterrupt) as error:
            if recorder is not None:
                recorder.finish(None, f"{type(error).__name__}: {error}")
                recorder.save(path)
            atomic_json(slot(root, "reports", args.run_id) / "failure.json",
                {"status": "unfinished" if recorder is not None else "initialization-failed", "episode": str(path) if recorder is not None else None,
                 "error": type(error).__name__, "reason": str(error)})
            raise
    report = {"episodes": episodes, "stop_reason": "cancelled" if cancelled() else ("elapsed" if (time.monotonic() - started) * 1000 >= args.elapsed_ms else "games"),
              "games_requested": args.games, "evidence_kind": "bounded-verification" if args.verification else "selfplay"}
    atomic_json(slot(root, "reports", args.run_id) / "selfplay.json", report)
    return report


def train(args, root, spec, cancelled):
    model, _ = load_base(args.base, spec)
    if args.adapter:
        if args.mode != "adapter":
            raise ValueError("adapter checkpoint cannot be used in base training mode")
        load_adapter(model, spec, args.adapter)
    if args.device == "cuda" and not torch.cuda.is_available():
        raise ValueError("requested CUDA device is unavailable")
    model.to(args.device)
    optimizer = create_optimizer(model, mode=args.mode, learning_rate=args.learning_rate)
    dataset = ReplayDataset(args.replay, spec)
    cursor = DatasetCursor(dataset, args.seed)
    previous = load_training_checkpoint(model, optimizer, spec, cursor, args.resume) if args.resume else 0
    directory = slot(root, "runs", args.run_id)
    checkpoint = directory / "training.pt"
    limits = TrainingLimits(steps=args.steps, batch_size=args.batch_size, elapsed_ms=args.elapsed_ms,
                  max_parameter_state_bytes=args.memory_mib * 1024 * 1024)
    if not 1 <= args.checkpoint_every <= 1_000_000:
        raise ValueError("checkpoint interval must be finite and positive")
    last_saved = previous
    def checkpoint_progress(completed):
        nonlocal last_saved
        if completed % args.checkpoint_every == 0:
            save_training_checkpoint(model, optimizer, spec, cursor, checkpoint, completed_steps=previous + completed)
            last_saved = previous + completed
    try:
        report = optimize(model, optimizer, PublicEncoder(spec), cursor, limits=limits, cancelled=cancelled, on_step=checkpoint_progress)
        save_training_checkpoint(model, optimizer, spec, cursor, checkpoint, completed_steps=previous + report["steps"])
    except (Exception, KeyboardInterrupt) as error:
        # Do not retry, lower resources or relabel data. A prior valid checkpoint
        # remains intact; a non-finite failed state cannot overwrite it.
        atomic_json(slot(root, "reports", args.run_id) / "training-failure.json",
                    {"status": "failed", "error": type(error).__name__, "reason": str(error), "last_checkpointed_step": last_saved})
        raise
    if args.mode == "base":
        save_base(model, spec, directory / "base.pt")
    else:
        save_adapter(model, spec, directory / "adapter.pt")
    report.update({"checkpoint": str(checkpoint), "completed_steps": previous + report["steps"], "mode": args.mode})
    atomic_json(slot(root, "reports", args.run_id) / "training.json", report)
    return report


def export(args, root, spec):
    model, _ = load_base(args.base, spec)
    descriptor = load_adapter(model, spec, args.adapter) if args.adapter else None
    manifest = export_onnx(model, spec, slot(root, "models", args.slot), descriptor=descriptor)
    return {"manifest": str(manifest), "model_sha256": load_manifest(manifest, spec)["model_sha256"], "activated": False}


def evaluate(args, root, spec, cancelled):
    if not 1 <= args.max_samples <= 4096:
        raise ValueError("evaluation needs a finite positive sample limit")
    manifest = _manifest(args, root, spec)
    evaluator = ProductionEvaluator(manifest, spec, args.backend, threads=args.threads)
    episode = ReplayEpisode.load(args.replay, spec)
    encoder = PublicEncoder(spec)
    metrics = []
    for record in episode.decisions[:args.max_samples]:
        if cancelled():
            break
        observation = episode.trackers[record["actor"]].frame_at(record["trace_step"])
        candidates = record["candidates"]
        encoded = encoder.encode(observation, [item["intent"] for item in candidates], belief_summary=record["belief_summary"])
        logits, values = evaluator.evaluate(encoded.board[None], encoded.condition[None], encoded.action_features[None])
        logits = logits[0].astype(np.float64)
        log_policy = logits - (logits.max() + np.log(np.exp(logits - logits.max()).sum()))
        policy = np.array([item["probability"] for item in candidates])
        metric = {"policy_ce": float(-(policy * log_policy).sum()), "value_prediction": float(values[0, 0])}
        if episode.outcome["status"] == "terminal":
            winner = episode.outcome["winner"]
            target = 0. if winner == "draw" else (1. if winner == record["actor"] else -1.)
            metric["value_mse"] = (metric["value_prediction"] - target)**2
        metrics.append(metric)
    report = {"samples": len(metrics), "metrics": metrics, "backend": args.backend, "episode_status": episode.outcome["status"], "activated": False}
    atomic_json(slot(root, "reports", args.run_id) / "evaluation.json", report)
    return report


def activate(args, root, spec):
    manifest_path = Path(args.manifest).resolve()
    manifest = load_manifest(manifest_path, spec)
    if len(args.expected_sha256) != 64 or manifest["model_sha256"] != args.expected_sha256:
        raise ValueError("explicit activation SHA-256 does not match the verified artifact")
    evaluator = ProductionEvaluator(manifest_path, spec, args.backend, threads=args.threads)
    logits, value = evaluator.evaluate(np.zeros((1, spec.board_channels, 8, 8), np.float32),
        np.zeros((1, spec.condition_dim), np.float32), np.zeros((1, 1, spec.action_dim), np.float32))
    if not np.isfinite(logits).all() or not np.isfinite(value).all():
        raise ValueError("activation native inference smoke returned non-finite outputs")
    reference = {"version": "accelerate-activation-v1", "manifest": str(manifest_path),
                 "manifest_sha256": file_sha256(manifest_path), "model_sha256": manifest["model_sha256"],
                 "encoder_hash": spec.digest, "backend": args.backend}
    path = slot(root, "models", "active") / "activation.json"
    atomic_json(path, reference)
    return {"activation": str(path), **reference}


def parser():
    main = argparse.ArgumentParser(prog="accelerate-chess")
    main.add_argument("--artifact-root", help="external fixed artifact root; defaults to host APPDATA/Accelerate")
    main.add_argument("--catalog", help="explicit frozen catalog JSON; installed native catalog is the default")
    main.add_argument("--threads", type=int, default=1, help="CPU threads for Torch and Rust ort (1..64); tract requires 1")
    commands = main.add_subparsers(dest="command", required=True)
    init = commands.add_parser("init")
    init.add_argument("--slot", default="default")
    init.add_argument("--seed", type=int, default=42)
    init.add_argument("--channels", type=int, default=128)
    init.add_argument("--blocks", type=int, default=8)
    init.add_argument("--rank", type=int, default=8)
    init.add_argument("--overwrite", action="store_true")
    for name in ("choose", "selfplay"):
        command = commands.add_parser(name)
        command.add_argument("--config", help="public GameConfig JSON; defaults to source normal mode")
        command.add_argument("--manifest")
        command.add_argument("--backend", choices=("ort", "tract"), default="ort", help="explicit execution backend; activation provenance does not change this selection")
        command.add_argument("--belief-seed", type=int, default=71)
        command.add_argument("--particles", type=int, default=8)
        command.add_argument("--proposals", type=int, default=64)
        command.add_argument("--belief-ms", type=int, default=5000)
        command.add_argument("--iterations", type=int, default=32)
        command.add_argument("--leaf-batch-size", type=int, default=4)
        command.add_argument("--depth", type=int, default=8)
        command.add_argument("--search-ms", type=int, default=1000)
        command.add_argument("--nodes", type=int, default=4096)
        command.add_argument("--edges", type=int, default=65536)
        command.add_argument("--candidates", type=int, default=256)
        if name == "choose":
            command.add_argument("--trace", required=True)
        else:
            command.add_argument("--games", type=int, default=1)
            command.add_argument("--max-plies", type=int, default=2)
            command.add_argument("--elapsed-ms", type=int, default=10000)
            command.add_argument("--seed", type=int, default=37)
            command.add_argument("--run-id", default="verification")
            command.add_argument("--verification", action="store_true")
    command = commands.add_parser("train")
    command.add_argument("--base", required=True)
    command.add_argument("--adapter")
    command.add_argument("--mode", choices=("base", "adapter"), default="base")
    command.add_argument("--replay", nargs="+", required=True)
    command.add_argument("--resume")
    command.add_argument("--checkpoint-every", type=int, default=100)
    command.add_argument("--run-id", default="training")
    command.add_argument("--seed", type=int, default=19)
    command.add_argument("--steps", type=int, default=1)
    command.add_argument("--batch-size", type=int, default=2)
    command.add_argument("--elapsed-ms", type=int, default=60000)
    command.add_argument("--memory-mib", type=int, default=1024)
    command.add_argument("--learning-rate", type=float, default=3e-4)
    command.add_argument("--device", choices=("cpu", "cuda"), default="cpu")
    command = commands.add_parser("export")
    command.add_argument("--base", required=True)
    command.add_argument("--adapter")
    command.add_argument("--slot", default="deployment")
    command = commands.add_parser("evaluate")
    command.add_argument("--manifest")
    command.add_argument("--backend", choices=("ort", "tract"), default="ort", help="explicit execution backend; tract requires --threads 1")
    command.add_argument("--replay", required=True)
    command.add_argument("--max-samples", type=int, default=8)
    command.add_argument("--run-id", default="evaluation")
    command = commands.add_parser("activate")
    command.add_argument("--manifest", required=True)
    command.add_argument("--expected-sha256", required=True)
    command.add_argument("--backend", choices=("ort", "tract"), default="ort", help="backend used to verify activation; recorded as provenance, with no automatic runtime selection")
    return main


def main(arguments=None):
    args = parser().parse_args(arguments)
    stopped = False
    def cancel(signum, frame):
        nonlocal stopped
        stopped = True
    previous = signal.signal(signal.SIGINT, cancel)
    try:
        if not 1 <= args.threads <= 64:
            raise ValueError("CPU threads must be bounded to 1..64")
        if hasattr(args, "seed") and not 0 <= args.seed < 2**32:
            raise ValueError("command seed must fit uint32 without silent normalization")
        if hasattr(args, "belief_seed") and not 0 <= args.belief_seed < 2**32 - 1:
            raise ValueError("independent belief seed must fit uint32 including its second viewer")
        torch.set_num_threads(args.threads)
        root, spec = artifact_root(args.artifact_root), default_spec(args.catalog)
        cancelled = lambda: stopped
        if args.command == "init":
            result = initialize(args, root, spec)
        elif args.command == "choose":
            result = choose(args, root, spec, cancelled)
        elif args.command == "selfplay":
            result = selfplay(args, root, spec, cancelled)
        elif args.command == "train":
            result = train(args, root, spec, cancelled)
        elif args.command == "export":
            result = export(args, root, spec)
        elif args.command == "evaluate":
            result = evaluate(args, root, spec, cancelled)
        else:
            result = activate(args, root, spec)
        print(canonical_json(result))
        if stopped:
            return 130
        return 2 if args.command in ("selfplay", "train") and result.get("stop_reason") == "elapsed" else 0
    except (Exception, KeyboardInterrupt) as error:
        print(f"{type(error).__name__}: {error}", file=sys.stderr)
        return 130 if stopped else 2
    finally:
        signal.signal(signal.SIGINT, previous)


if __name__ == "__main__":
    raise SystemExit(main())
