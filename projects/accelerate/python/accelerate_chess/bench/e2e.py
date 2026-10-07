"""Finite source-backed training loop and local resource receipt.

The CLI owns model, replay, optimizer and source contracts. This module only
connects them and records stage boundaries. A missing terminal is a blocker.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time

import torch

from accelerate_chess import cli
from accelerate_chess.adapter_client import GameAdapterClient
from accelerate_chess.network.artifacts import file_sha256, load_typed_base
from accelerate_chess.replay import ReplayEpisode, artifact_root, atomic_json, reserve_slot
from accelerate_chess.search import BeliefLimits, NativeSourceFactory, ParticleBelief, PublicTracker
from accelerate_chess.training import ReplayDataset


def _command(args, command, *items):
    return cli.parser().parse_args(["--artifact-root", str(args.artifact_root),
        "--model-family", args.model_family, "--threads", str(args.threads), command, *list(map(str, items))])


def _rss():
    try:
        with open("/proc/self/status", encoding="utf-8") as source:
            fields = dict(line.split(":", 1) for line in source if ":" in line)
        return int(fields["VmHWM"].split()[0]) * 1024
    except (OSError, KeyError, ValueError):
        return None


def _current_rss():
    try:
        with open("/proc/self/status", encoding="utf-8") as source:
            fields = dict(line.split(":", 1) for line in source if ":" in line)
        return int(fields["VmRSS"].split()[0]) * 1024
    except (OSError, KeyError, ValueError):
        return None


def _meminfo():
    try:
        with open("/proc/meminfo", encoding="utf-8") as source:
            fields = {key: int(value.split()[0]) * 1024 for key, value in
                      (line.split(":", 1) for line in source if ":" in line)}
        return {"total_bytes": fields["MemTotal"], "available_bytes": fields["MemAvailable"],
                "swap_total_bytes": fields["SwapTotal"],
                "swap_used_bytes": fields["SwapTotal"] - fields["SwapFree"]}
    except (OSError, KeyError, ValueError):
        return None


def _probe(command):
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=3)
        return result.stdout.strip() if result.returncode == 0 else None
    except (OSError, subprocess.TimeoutExpired):
        return None


class Observer:
    def __init__(self, path, interval):
        self.path, self.interval = path, interval
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self._run, daemon=True)
        self.samples = 0
        self.cost = 0.
        self.gpu_status = "unsupported"

    def _run(self):
        with self.path.open("w", encoding="utf-8") as output:
            while not self.stop.is_set():
                start = time.monotonic()
                row = {"timestamp": datetime.now(timezone.utc).isoformat(),
                       "process_rss_bytes": _current_rss(), "rss_hwm_bytes": _rss(), "memory": _meminfo()}
                try:
                    query = subprocess.run(["nvidia-smi", "--query-gpu=utilization.gpu,memory.used,power.draw,temperature.gpu,clocks.sm",
                        "--format=csv,noheader,nounits"], capture_output=True, text=True, timeout=2)
                    if query.returncode == 0 and query.stdout.strip():
                        row["gpu"] = query.stdout.strip().splitlines()[0].split(", ")
                        self.gpu_status = "measured"
                except (OSError, subprocess.TimeoutExpired):
                    pass
                output.write(json.dumps(row, separators=(",", ":")) + "\n")
                output.flush()
                self.samples += 1
                self.cost += time.monotonic() - start
                self.stop.wait(self.interval)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.stop.set()
        self.thread.join(timeout=3)


def _bootstrap(args, spec, report_dir):
    position = GameAdapterClient.new_game({"gameStyle": "normal"}, args.seed, spec=spec)
    actor = position.decision_actor
    intents = position.legal_intents_page(limit=1)["intents"]
    if not intents:
        raise ValueError("native initial position has no public legal intent")
    path = report_dir / "bootstrap-public.json"
    atomic_json(path, {"version": "typed-inference-sample-v1", "architecture_family": args.model_family,
        "encoder_hash": spec.digest, "observation": position.observe(actor),
        "intents": intents, "belief_summary": None})
    cli._typed_sample_inputs(None, spec, args.model_family, sample_public=path)
    return path


def _arena_assignment(number):
    if type(number) is not int or number < 0:
        raise ValueError("arena game number must be nonnegative")
    return {"white": "trained" if number % 2 == 0 else "baseline",
            "black": "baseline" if number % 2 == 0 else "trained"}


def _terminal_paths(items, episodes):
    if len(items) != len(episodes):
        raise ValueError("selfplay episode and replay validation counts differ")
    return [item["path"] for item, episode in zip(items, episodes)
            if episode.outcome["status"] == "terminal"]


def _arena(args, spec, baseline, trained):
    config = {"gameStyle": "normal"}
    searches = {name: cli._search(_command(args, "selfplay", "--iterations", args.iterations,
        "--leaf-batch-size", args.leaf_batch_size, "--search-ms", args.search_ms), spec, manifest, args.backend)
        for name, manifest in (("baseline", baseline), ("trained", trained))}
    games = []
    for number in range(args.arena_games):
        assignment = _arena_assignment(number)
        seed = (args.seed + number // 2) % 2**32
        position = GameAdapterClient.new_game(config, seed, spec=spec)
        trackers = {color: PublicTracker(position.observe(color), typed_spec=spec) for color in ("white", "black")}
        beliefs = {color: ParticleBelief(trackers[color], NativeSourceFactory(config, typed_spec=spec),
            seed=args.belief_seed + index, limits=BeliefLimits(particles=args.particles,
            proposals=args.proposals, elapsed_ms=args.belief_ms)) for index, color in enumerate(("white", "black"))}
        start = time.monotonic()
        stats = {"decisions": 0, "simulations": 0, "inference_batches": 0, "max_inference_batch": 0}
        reason = "ply-limit"
        for ply in range(args.max_plies):
            if position.result is not None:
                reason = "source-terminal"
                break
            if (time.monotonic() - start) * 1000 >= args.elapsed_ms:
                reason = "elapsed"
                break
            actor = position.decision_actor
            if trackers[actor].latest != position.observe(actor):
                raise ValueError("arena public projection diverged from native source")
            result = searches[assignment[actor]].run(beliefs[actor])
            stats["decisions"] += 1
            stats["simulations"] += result.iterations
            stats["inference_batches"] += result.inference_batches
            stats["max_inference_batch"] = max(stats["max_inference_batch"], result.max_inference_batch)
            child = position.apply(position.bind_public_intent(result.intent)).position
            for color in ("white", "black"):
                trackers[color].append(child.observe(color), own_intent=result.intent if color == actor else None)
            position = child
        if position.result is not None:
            reason = "source-terminal"
        games.append({"assignment": assignment, "environment_seed": seed,
            "belief_seeds": [args.belief_seed, args.belief_seed + 1], "winner": position.result,
            "terminal_reason": reason, "plies": trackers["white"].steps,
            "wall_seconds": time.monotonic() - start, **stats})
    outcomes = {"trained_wins": 0, "baseline_wins": 0, "draws": 0}
    for game in games:
        if game["winner"] == "draw":
            outcomes["draws"] += 1
        elif game["winner"] in ("white", "black"):
            outcomes[game["assignment"][game["winner"]] + "_wins"] += 1
    return {"games_requested": args.arena_games, "games_completed": sum(g["winner"] is not None for g in games),
            "outcomes": outcomes, "games": games, "backend": args.backend, "provider": "unknown"}


def run(args):
    root = artifact_root(args.artifact_root)
    args.artifact_root = root
    report_dir = reserve_slot(root, "reports", args.run_id)
    receipt_path = report_dir / "e2e.json"
    spec = cli.default_spec(model_family=args.model_family)
    git_sha = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    dirty = bool(subprocess.run(["git", "status", "--porcelain"], capture_output=True, text=True).stdout)
    receipt = {"version": "local-e2e-v1", "run_id": args.run_id, "timestamp": datetime.now(timezone.utc).isoformat(),
        "git_sha": git_sha, "git_dirty": dirty, "config": {key: value for key, value in vars(args).items() if key != "artifact_root"},
        "model_family": args.model_family, "rules_version": spec.rules_version,
        "catalog_hash": spec.catalog_hash, "encoder_hash": spec.digest,
        "torch_version": torch.__version__, "cuda_version": torch.version.cuda,
        "cuda_available": torch.cuda.is_available(), "memory_start": _meminfo(),
        "python_version": sys.version.split()[0], "rust_version": _probe(["rustc", "--version"]),
        "cpu_model": _probe(["sh", "-c", "sed -n 's/^model name[[:space:]]*:[[:space:]]*//p' /proc/cpuinfo | head -1"]),
        "logical_cpu_count": os.cpu_count(), "gpu_model": torch.cuda.get_device_name(0) if torch.cuda.is_available() else None,
        "gpu_vram_bytes": torch.cuda.get_device_properties(0).total_memory if torch.cuda.is_available() else None,
        "nvidia_driver": _probe(["nvidia-smi", "--query-gpu=driver_version", "--format=csv,noheader"]),
        "power_profile": _probe(["powerprofilesctl", "get"]),
        "inference_backend": args.backend, "inference_provider": "unknown",
        "stages": {}, "status": "running"}
    def save():
        atomic_json(receipt_path, receipt)
    def stage(name, function):
        start, cpu, rss = time.monotonic(), time.process_time(), _rss()
        status, error, failure, value = "ok", None, None, None
        try:
            if torch.cuda.is_available():
                torch.cuda.synchronize()
                torch.cuda.reset_peak_memory_stats()
            value = function()
        except (Exception, KeyboardInterrupt) as exc:
            failure = exc
        try:
            if torch.cuda.is_available():
                torch.cuda.synchronize()
        except Exception as exc:
            if failure is None:
                failure = exc
            else:
                error = {"cuda_synchronization_error": {"type": type(exc).__name__, "message": str(exc)}}
        if failure is not None:
            status = "failed"
            error = {**(error or {}), "type": type(failure).__name__, "message": str(failure)}
            receipt["status"], receipt["blocker"] = "blocked", name
        receipt["stages"][name] = {"status": status, "wall_seconds": time.monotonic() - start,
            "cpu_seconds": time.process_time() - cpu, "rss_hwm_bytes": _rss(),
            "rss_hwm_before_bytes": rss, "error": error,
            "cuda_peak_allocated_bytes": torch.cuda.max_memory_allocated() if torch.cuda.is_available() else None,
            "cuda_peak_reserved_bytes": torch.cuda.max_memory_reserved() if torch.cuda.is_available() else None}
        save()
        if failure is not None:
            raise failure
        return value
    save()
    with Observer(report_dir / "observer.jsonl", args.observer_interval) as observer:
        try:
            sample = stage("bootstrap", lambda: _bootstrap(args, spec, report_dir))
            init = stage("init", lambda: cli.initialize(_command(args, "init", "--slot", args.run_id + "-initial",
                "--seed", args.seed, "--channels", args.channels, "--blocks", args.blocks, "--rank", args.rank), root, spec))
            receipt["parameter_count"] = sum(p.numel() for p in load_typed_base(init["base"], spec)[0].parameters())
            baseline = stage("export-baseline", lambda: cli.export(_command(args, "export", "--base", init["base"],
                "--sample-public", sample, "--slot", args.run_id + "-baseline"), root, spec))
            receipt["baseline_model_sha256"] = baseline["model_sha256"]
            receipt["artifacts"] = {"bootstrap_sample_sha256": file_sha256(sample),
                "initial_base_sha256": file_sha256(init["base"]),
                "baseline_manifest_sha256": file_sha256(baseline["manifest"])}
            selfplay = stage("selfplay", lambda: cli.selfplay(_command(args, "selfplay", "--manifest", baseline["manifest"],
                "--backend", args.backend, "--run-id", args.run_id, "--games", args.games,
                "--max-plies", args.max_plies, "--elapsed-ms", args.elapsed_ms,
                "--iterations", args.iterations, "--leaf-batch-size", args.leaf_batch_size,
                "--search-ms", args.search_ms, "--seed", args.seed, "--belief-seed", args.belief_seed,
                "--particles", args.particles, "--proposals", args.proposals, "--belief-ms", args.belief_ms), root, spec, lambda: False))
            episodes = stage("replay-load", lambda: [ReplayEpisode.load(item["path"], spec) for item in selfplay["episodes"]])
            terminal = _terminal_paths(selfplay["episodes"], episodes)
            receipt["selfplay"] = {"games": len(episodes), "terminal_episodes": len(terminal),
                "decisions": sum(len(e.decisions) for e in episodes),
                "plies": sum(e.trackers["white"].steps for e in episodes),
                "simulations": sum(d["search"]["iterations"] for e in episodes for d in e.decisions),
                "inference_batches": sum(d["search"]["inference_batches"] for e in episodes for d in e.decisions),
                "max_inference_batch": max((d["search"]["max_inference_batch"] for e in episodes for d in e.decisions), default=0),
                "episode_sha256": {Path(item["path"]).name: file_sha256(item["path"]) for item in selfplay["episodes"]}}
            receipt["selfplay"].update({"search_elapsed_seconds": selfplay["workload"]["search_elapsed_seconds"],
                "candidates": selfplay["workload"]["candidates"]})
            selfplay_seconds = receipt["stages"]["selfplay"]["wall_seconds"]
            receipt["selfplay"]["simulations_per_second"] = receipt["selfplay"]["simulations"] / selfplay_seconds
            receipt["selfplay"]["decisions_per_second"] = receipt["selfplay"]["decisions"] / selfplay_seconds
            receipt["selfplay"]["completed_games_per_hour"] = len(terminal) * 3600 / selfplay_seconds
            receipt["selfplay"]["nn_positions_per_second"] = None
            receipt["selfplay"]["nn_positions_reason"] = "SearchResult records batches, not exact evaluated position count"
            if not terminal:
                receipt["status"], receipt["blocker"] = "blocked", "no-terminal-replay"
                raise RuntimeError("no-terminal-replay: source-backed games did not reach a terminal result within the finite budget")
            dataset = stage("dataset-index", lambda: ReplayDataset(terminal, spec, architecture_family=args.model_family))
            receipt["training_examples"] = len(dataset)
            trained = stage("training", lambda: cli.train(_command(args, "train", "--base", init["base"],
                "--replay", *terminal, "--run-id", args.run_id, "--steps", args.steps,
                "--batch-size", args.batch_size, "--elapsed-ms", args.elapsed_ms,
                "--memory-mib", args.memory_mib, "--device", args.device, "--dtype", args.dtype,
                "--seed", args.seed), root, spec, lambda: False, timings={}))
            receipt["training"] = trained
            if trained["steps"]:
                receipt["training"]["samples_per_second_stage"] = (
                    trained["steps"] * args.batch_size / receipt["stages"]["training"]["wall_seconds"])
            trained_export = stage("export-trained", lambda: cli.export(_command(args, "export",
                "--base", str(root / "runs" / args.run_id / "base.pt"), "--sample-replay", terminal[0],
                "--slot", args.run_id + "-trained"), root, spec))
            receipt["trained_model_sha256"] = trained_export["model_sha256"]
            receipt["artifacts"]["trained_manifest_sha256"] = file_sha256(trained_export["manifest"])
            receipt["artifacts"]["trained_base_sha256"] = file_sha256(root / "runs" / args.run_id / "base.pt")
            receipt["offline_evaluation"] = stage("offline-evaluation", lambda: {
                name: cli.evaluate(_command(args, "evaluate", "--manifest", manifest,
                    "--replay", terminal[0], "--run-id", args.run_id + "-" + name), root, spec, lambda: False)
                for name, manifest in (("baseline", baseline["manifest"]), ("trained", trained_export["manifest"]))})
            receipt["offline_metric_means"] = {name: {key: sum(row[key] for row in value["metrics"]) / len(value["metrics"])
                for key in ("policy_ce", "value_mse") if value["metrics"] and key in value["metrics"][0]}
                for name, value in receipt["offline_evaluation"].items()}
            receipt["arena"] = stage("arena", lambda: _arena(args, spec, baseline["manifest"], trained_export["manifest"]))
            receipt["status"] = "complete"
        except (Exception, KeyboardInterrupt) as exc:
            receipt["failure"] = {"type": type(exc).__name__, "message": str(exc)}
            receipt["memory_at_failure"] = _meminfo()
    receipt["observer"] = {"path": str(report_dir / "observer.jsonl"), "interval_seconds": args.observer_interval,
        "samples": observer.samples, "cost_seconds": observer.cost, "gpu_status": observer.gpu_status}
    save()
    return receipt


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-root", required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--model-family", choices=("mask-resnet", "entity-transformer"), default="mask-resnet")
    parser.add_argument("--backend", choices=("ort", "tract"), default="ort")
    parser.add_argument("--device", choices=("cpu", "cuda"), default="cpu")
    parser.add_argument("--dtype", choices=("fp32", "bf16"), default="fp32")
    for name, default in (("seed", 37), ("belief-seed", 71), ("threads", 1), ("channels", 32),
        ("blocks", 1), ("rank", 4), ("games", 1), ("max-plies", 8), ("elapsed-ms", 120000),
        ("iterations", 2), ("leaf-batch-size", 1), ("search-ms", 5000), ("particles", 2),
        ("proposals", 4), ("belief-ms", 5000), ("steps", 2), ("batch-size", 4),
        ("memory-mib", 1024), ("arena-games", 2)):
        parser.add_argument("--" + name, type=int, default=default)
    parser.add_argument("--observer-interval", type=float, default=1.)
    args = parser.parse_args(argv)
    if not (0 <= args.seed < 2**32 and 0 <= args.belief_seed < 2**32 - 1
            and all(1 <= getattr(args, name) <= limit for name, limit in (
                ("games", 64), ("max_plies", 4096), ("elapsed_ms", 86_400_000),
                ("iterations", 1_000_000), ("leaf_batch_size", 64), ("steps", 1_000_000),
                ("batch_size", 64), ("arena_games", 64), ("threads", 64),
                ("particles", 1024), ("proposals", 65536), ("search_ms", 86_400_000),
                ("belief_ms", 86_400_000)))
            and 0.1 <= args.observer_interval <= 60):
        parser.error("seeds and finite positive budgets are required")
    result = run(args)
    print(json.dumps(result, indent=2, default=str))
    return 0 if result["status"] == "complete" else 2


if __name__ == "__main__":
    raise SystemExit(main())
