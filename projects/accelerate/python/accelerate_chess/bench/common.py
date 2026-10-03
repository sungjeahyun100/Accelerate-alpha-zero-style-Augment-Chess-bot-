"""Shared result envelope and finite benchmark arguments."""
from __future__ import annotations

from datetime import datetime, timezone
import argparse
import json
import os
import platform
import subprocess
import sys

from accelerate_chess.replay import artifact_root, reserve_slot, atomic_json
from . import VERSION



def positive(value: str) -> int:
    number = int(value)
    if not 1 <= number <= 1_000_000:
        raise argparse.ArgumentTypeError("benchmark count must be within 1..1000000")
    return number


def workers(value: str) -> int:
    number = positive(value)
    if number > (os.cpu_count() or 1):
        raise argparse.ArgumentTypeError("workers exceed logical CPU count")
    return number


def report(kind: str, config: dict, results: dict, *, output_root=None, run_id=None) -> dict:
    def version(command):
        try:
            return subprocess.run(command, check=True, capture_output=True, text=True, timeout=5).stdout.strip()
        except (OSError, subprocess.SubprocessError):
            return None

    cpu = platform.processor()
    if not cpu and sys.platform.startswith("linux"):
        try:
            with open("/proc/cpuinfo", encoding="utf-8") as source:
                cpu = next(line.split(":", 1)[1].strip() for line in source
                           if line.startswith("model name"))
        except (OSError, StopIteration):
            cpu = None
    gpu = gpu_vram_bytes = None
    try:
        import torch
        torch_version, cuda_version = torch.__version__, torch.version.cuda
        if torch.cuda.is_available():
            gpu = torch.cuda.get_device_name()
            gpu_vram_bytes = torch.cuda.get_device_properties(0).total_memory
    except ImportError:
        torch_version = cuda_version = None
    payload = {"benchmark_version": VERSION, "kind": kind,
               "git_sha": version(["git", "rev-parse", "HEAD"]),
               "timestamp": datetime.now(timezone.utc).isoformat(),
               "hostname": platform.node(), "os": platform.platform(),
               "cpu": cpu, "logical_cpu_count": os.cpu_count(),
               "gpu": gpu, "gpu_vram_bytes": gpu_vram_bytes,
               "python_version": sys.version.split()[0],
               "rust_version": version(["rustc", "--version"]),
               "torch_version": torch_version, "cuda_version": cuda_version,
               "config": config, "results": results}
    if run_id is not None:
        path = reserve_slot(artifact_root(output_root), "reports", run_id) / f"{kind}.json"
        atomic_json(path, payload)
        print(f"report: {path}")
    print(json.dumps(payload, indent=2, default=str))
    return payload
