"""Render validated local-performance-v1 artifacts into repository-local reports."""
from __future__ import annotations

import argparse
from collections import defaultdict
from datetime import datetime
import importlib.util
import json
import math
from pathlib import Path
import re

from . import VERSION

KINDS = {"engine", "inference", "training", "pipeline"}
OPERATIONS = ("fork", "legal", "bind", "apply", "observe", "encode", "transition")
REPORTS = Path(__file__).resolve().parents[3] / "bench" / "results"
ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9_-]{0,63}\Z")
INFERENCE_METRICS = ("positions_per_second", "latency_mean_ms", "latency_p50_ms", "latency_p95_ms", "peak_vram_bytes")
TRAINING_METRICS = ("samples_per_second", "steps_per_second", "mean_step_ms", "forward_ms", "backward_ms", "optimizer_ms", "peak_vram_bytes")


def _reject_nonfinite(value):
    raise ValueError(f"nonfinite JSON constant {value}")


def _number(value, where, *, allow_zero=False):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0 or (not allow_zero and value == 0):
        raise ValueError(f"{where}: expected a finite {'nonnegative' if allow_zero else 'positive'} number")
    return value


def _field(obj, name, kind, where, *, optional=False):
    value = obj.get(name)
    if value is None and optional:
        return None
    if kind is int:
        if type(value) is not int or value < 1:
            raise ValueError(f"{where}.{name}: expected a positive integer")
    elif not isinstance(value, kind) or (kind is str and not value):
        raise ValueError(f"{where}.{name}: expected {kind.__name__}")
    return value


def _status(obj, where):
    status = _field(obj, "status", str, where)
    if status not in {"ok", "unsupported", "oom", "failed", "error"}:
        raise ValueError(f"{where}.status: unknown status {status!r}")
    if status != "ok":
        _field(obj, "reason", str, where)
    return status


def _validate(payload, run_id, relative):
    where = str(relative)
    if not isinstance(payload, dict):
        raise ValueError(f"{where}: expected JSON object")
    if payload.get("benchmark_version") != VERSION:
        raise ValueError(f"{where}: unsupported benchmark version {payload.get('benchmark_version')!r}")
    kind = _field(payload, "kind", str, where)
    if kind not in KINDS or relative.name != f"{kind}.json":
        raise ValueError(f"{where}: unknown or mismatched benchmark kind {kind!r}")
    for key in ("git_sha", "timestamp"):
        _field(payload, key, str, where, optional=key == "git_sha")
    try:
        datetime.fromisoformat(payload["timestamp"])
    except (ValueError, TypeError) as error:
        raise ValueError(f"{where}.timestamp: invalid ISO timestamp") from error
    config = _field(payload, "config", dict, where)
    results = _field(payload, "results", dict, where)
    rows, incomplete = [], []

    def missing(label, item):
        incomplete.append({"run_id": run_id, "kind": kind, "measurement": label,
                           "status": item["status"], "reason": item["reason"]})

    if kind == "engine":
        workers = _field(config, "workers", int, where + ".config")
        for operation, item in results.items():
            location = f"{where}.results.{operation}"
            if not isinstance(item, dict):
                raise ValueError(f"{location}: expected JSON object")
            if _status(item, location) != "ok":
                missing(operation, item)
                continue
            if operation not in OPERATIONS:
                raise ValueError(f"{location}: unknown operation")
            rows.append({"run_id": run_id, "operation": operation, "workers": workers,
                         "ops_per_second": _number(item.get("ops_per_second"), location + ".ops_per_second")})
    elif kind == "inference":
        measurements = _field(results, "measurements", list, where + ".results")
        for index, item in enumerate(measurements):
            location = f"{where}.results.measurements[{index}]"
            if not isinstance(item, dict):
                raise ValueError(f"{location}: expected JSON object")
            model = _field(item, "model_name", str, location)
            dtype = _field(item, "dtype", str, location)
            batch = _field(item, "batch_size", int, location)
            candidates = _field(item, "candidate_count", int, location)
            nodes = _field(item, "candidate_nodes", int, location)
            profile = next((name for name, shape in {"small": (32, 3), "normal": (80, 8), "monster": (160, 160)}.items() if shape == (candidates, nodes)), None)
            if profile is None:
                raise ValueError(f"{location}: unknown candidate profile")
            label = f"{model} / {dtype} / {profile} / batch {batch}"
            if _status(item, location) != "ok":
                missing(label, item)
                continue
            row = {"run_id": run_id, "model": model, "dtype": dtype, "profile": profile,
                   "batch_size": batch, "synthetic": item.get("synthetic") is True}
            for metric in INFERENCE_METRICS:
                row[metric] = _number(item.get(metric), location + "." + metric, allow_zero=metric == "peak_vram_bytes")
            rows.append(row)
    elif kind == "training":
        if _status(results, where + ".results") != "ok":
            missing("training", results)
        else:
            row = {"run_id": run_id, "model": _field(config, "model", str, where + ".config"),
                   "profile": _field(config, "profile", str, where + ".config"),
                   "dtype": _field(config, "dtype", str, where + ".config"),
                   "device": _field(config, "device", str, where + ".config"),
                   "batch_size": _field(config, "batch_size", int, where + ".config"),
                   "synthetic": results.get("synthetic") is True}
            if not row["synthetic"]:
                raise ValueError(f"{where}.results.synthetic: expected true")
            for metric in TRAINING_METRICS:
                row[metric] = _number(results.get(metric), where + ".results." + metric, allow_zero=metric == "peak_vram_bytes")
            rows.append(row)
        if isinstance(results.get("replay_loading"), str) and results["replay_loading"].startswith("unsupported"):
            incomplete.append({"run_id": run_id, "kind": kind, "measurement": "replay_loading", "status": "unsupported", "reason": results["replay_loading"]})
    else:
        for measurement, item in results.items():
            location = f"{where}.results.{measurement}"
            if not isinstance(item, dict):
                raise ValueError(f"{location}: expected JSON object")
            if _status(item, location) == "ok":
                raise ValueError(f"{location}: pipeline numeric schema is not supported by {VERSION} reporter")
            missing(measurement, item)
    return kind, rows, incomplete


def _source_files(root, runs):
    reports = root / "reports"
    if runs:
        if len(set(runs)) != len(runs):
            raise ValueError("duplicate run ID in --runs")
        for run_id in runs:
            if not ID.fullmatch(run_id):
                raise ValueError(f"invalid run ID: {run_id!r}")
            directory = reports / run_id
            if not directory.is_dir() or directory.is_symlink():
                raise FileNotFoundError(f"benchmark run not found: reports/{run_id}")
            files = sorted(directory.glob("*.json"))
            if not files:
                raise FileNotFoundError(f"benchmark JSON not found: reports/{run_id}")
            yield from files
    else:
        if not reports.is_dir():
            raise FileNotFoundError("benchmark reports directory not found")
        for directory in sorted(reports.iterdir()):
            if directory.is_dir() and not directory.is_symlink() and ID.fullmatch(directory.name):
                yield from sorted(directory.glob("*.json"))


def collect(root, runs=None):
    sources, data, incomplete = [], defaultdict(list), []
    for path in _source_files(root, runs):
        relative = path.relative_to(root)
        if path.is_symlink() or not path.is_file() or root.resolve() not in path.resolve().parents:
            raise ValueError(f"unsafe benchmark JSON path: {relative}")
        try:
            payload = json.loads(path.read_text(encoding="utf-8"), parse_constant=_reject_nonfinite)
        except (OSError, UnicodeError, ValueError) as error:
            raise ValueError(f"{relative}: malformed or unreadable JSON: {error}") from error
        kind, rows, absent = _validate(payload, path.parent.name, relative)
        # Failure text can contain a host name or a local filesystem path.
        hostname = payload.get("hostname")
        for item in absent:
            reason = item["reason"]
            if isinstance(hostname, str) and hostname:
                reason = reason.replace(hostname, "[host]")
            reason = re.sub(r"(?<![\w])(?:[A-Za-z]:[\\/]|/)[^\s,;:]+", "[path]", reason)
            item["reason"] = reason
        # Raw configs can contain absolute artifact paths. Keep only portable fields.
        config = {key: value for key, value in payload["config"].items() if key != "artifact_root"}
        sources.append({"run_id": path.parent.name, "kind": kind, "benchmark_version": VERSION,
                        "git_sha": payload["git_sha"], "timestamp": payload["timestamp"],
                        "config": config, "source_json_path": relative.as_posix(),
                        "environment": {key: payload.get(key) for key in ("cpu", "logical_cpu_count", "gpu", "gpu_vram_bytes", "python_version", "rust_version", "torch_version", "cuda_version")}})
        data[kind].extend(rows)
        incomplete.extend(absent)
    if not sources:
        raise ValueError("no benchmark JSON selected")
    hardware = {(s["environment"]["cpu"], s["environment"]["logical_cpu_count"], s["environment"]["gpu"], s["environment"]["gpu_vram_bytes"]) for s in sources}
    if len(hardware) > 1:
        raise ValueError("selected runs have different CPU/GPU hardware; create separate reports with --runs")
    shas = {s["git_sha"] for s in sources}
    warnings = ["Selected runs use different Git SHAs; measurements are not one revision baseline."] if len(shas) > 1 else []
    return {"report_version": 1, "benchmark_version": VERSION, "environment": sources[0]["environment"],
            "warnings": warnings, "measurements": {kind: data[kind] for kind in sorted(KINDS) if data[kind]},
            "incomplete": incomplete, "source_runs": [{"run_id": s["run_id"], "kind": s["kind"], "source_json_path": s["source_json_path"]} for s in sources]}, sources


def _cell(value):
    if value is None:
        return "—"
    if isinstance(value, float):
        return f"{value:,.3f}"
    return str(value).replace("|", "\\|").replace("\n", " ").replace("\r", " ")


def _table(headers, rows):
    return "| " + " | ".join(headers) + " |\n| " + " | ".join("---" for _ in headers) + " |\n" + "".join("| " + " | ".join(_cell(item) for item in row) + " |\n" for row in rows)


def markdown(summary, sources, charts):
    env = summary["environment"]
    lines = ["# Local Performance Benchmark Report", "", "## Environment", ""]
    fields = (("Git SHA", ", ".join(sorted({s["git_sha"] or "unknown" for s in sources}))),
              ("CPU", env["cpu"]), ("logical CPUs", env["logical_cpu_count"]),
              ("GPU", env["gpu"]), ("VRAM", f"{env['gpu_vram_bytes'] / 2**30:.2f} GiB" if env["gpu_vram_bytes"] is not None else None),
              ("Python", env["python_version"]), ("Rust", env["rust_version"]),
              ("PyTorch", env["torch_version"]), ("CUDA", env["cuda_version"]),
              ("benchmark version", VERSION))
    lines += [f"- {name}: {_cell(value)}" for name, value in fields]
    if summary["warnings"]:
        lines += ["", "## Warnings", ""] + [f"- {_cell(w)}" for w in summary["warnings"]]
    measurements = summary["measurements"]
    lines += ["", "## Engine", ""]
    engine = measurements.get("engine", [])
    lines += [_table(["Run", "Operation", "Workers", "ops/s"], [[r["run_id"], r["operation"], r["workers"], r["ops_per_second"]] for r in engine]) if engine else "No successful engine measurements."]
    lines += ["", "## Neural-network inference", ""]
    inference = measurements.get("inference", [])
    lines += ["Synthetic model inputs; timings exclude encoding and host-to-device transfer."]
    lines += [_table(["Run", "Model", "Dtype", "Profile", "Batch", "positions/s", "Mean ms", "p50 ms", "p95 ms", "Peak VRAM MiB"],
                     [[r["run_id"], r["model"], r["dtype"], r["profile"], r["batch_size"], r["positions_per_second"], r["latency_mean_ms"], r["latency_p50_ms"], r["latency_p95_ms"], r["peak_vram_bytes"] / 2**20] for r in inference]) if inference else "No successful inference measurements."]
    lines += ["", "## Training", "", "Synthetic optimizer steps; replay loading is excluded."]
    training = measurements.get("training", [])
    lines += [_table(["Run", "Model", "Profile", "Dtype", "Device", "Batch", "samples/s", "steps/s", "Mean step ms", "Forward ms", "Backward ms", "Optimizer ms", "Peak VRAM MiB"],
                     [[r["run_id"], r["model"], r["profile"], r["dtype"], r["device"], r["batch_size"], r["samples_per_second"], r["steps_per_second"], r["mean_step_ms"], r["forward_ms"], r["backward_ms"], r["optimizer_ms"], r["peak_vram_bytes"] / 2**20] for r in training]) if training else "No successful training measurements."]
    lines += ["", "## Unsupported / incomplete measurements", ""]
    absent = summary["incomplete"]
    lines += [_table(["Run", "Kind", "Measurement", "Status", "Reason"], [[r[k] for k in ("run_id", "kind", "measurement", "status", "reason")] for r in absent]) if absent else "None."]
    lines += ["", "## Source runs", "", _table(["Run", "Kind", "Source JSON"], [[s["run_id"], s["kind"], s["source_json_path"]] for s in sources])]
    if charts:
        lines += ["", "## Charts", ""] + [f"![{name}]({name})" for name in charts]
    return "\n".join(lines) + "\n"


def charts_for(summary, directory):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    made = []
    data = summary["measurements"]

    def save(name, title, xlabel, ylabel):
        plt.title(title)
        plt.xlabel(xlabel)
        plt.ylabel(ylabel)
        plt.grid(alpha=.25)
        plt.legend(fontsize="small")
        plt.tight_layout()
        plt.savefig(directory / name, dpi=150)
        plt.close()
        made.append(name)

    engine = data.get("engine", [])
    if engine:
        plt.figure(figsize=(10, 5))
        for run_id in sorted({r["run_id"] for r in engine}):
            rows = {r["operation"]: r for r in engine if r["run_id"] == run_id}
            plt.plot([op for op in OPERATIONS if op in rows], [rows[op]["ops_per_second"] for op in OPERATIONS if op in rows], marker="o", label=run_id)
        save("engine-ops.png", "Engine operations", "operation", "ops/s")
        if len({r["workers"] for r in engine}) > 1:
            plt.figure(figsize=(9, 5))
            for op in OPERATIONS:
                rows = sorted((r for r in engine if r["operation"] == op), key=lambda r: r["workers"])
                if rows:
                    plt.plot([r["workers"] for r in rows], [r["ops_per_second"] for r in rows], marker="o", label=op)
            save("engine-worker-scaling.png", "Engine worker scaling", "workers", "ops/s")
    inference = data.get("inference", [])
    if inference:
        grouped = defaultdict(list)
        for row in inference:
            grouped[(row["model"], row["dtype"], row["profile"], row["run_id"])].append(row)
        profiles = sorted({group[2] for group in grouped})
        subsets = [("", grouped)] if len(grouped) <= 8 else [
            (f"-{profile}", {group: rows for group, rows in grouped.items() if group[2] == profile})
            for profile in profiles]
        for suffix_name, subset in subsets:
            for stem, title, metrics, ylabel, divisor in (
                ("inference-throughput", "Inference throughput", ("positions_per_second",), "positions/s", 1),
                ("inference-latency", "Inference latency", ("latency_mean_ms", "latency_p50_ms", "latency_p95_ms"), "ms", 1),
                ("inference-vram", "Inference peak VRAM", ("peak_vram_bytes",), "MiB", 2**20)):
                plt.figure(figsize=(10, 6))
                for group, rows in sorted(subset.items()):
                    rows.sort(key=lambda r: r["batch_size"])
                    for metric in metrics:
                        suffix = " / " + metric.removeprefix("latency_") if len(metrics) > 1 else ""
                        plt.plot([r["batch_size"] for r in rows], [r[metric] / divisor for r in rows], marker="o", label=" / ".join(group) + suffix)
                save(f"{stem}{suffix_name}.png", title, "batch size", ylabel)
    training = data.get("training", [])
    if training:
        for name, metrics, ylabel, divisor in (
            ("training-throughput.png", ("samples_per_second", "steps_per_second"), "samples/s or steps/s", 1),
            ("training-latency.png", ("mean_step_ms", "forward_ms", "backward_ms", "optimizer_ms"), "ms", 1),
            ("training-vram.png", ("peak_vram_bytes",), "MiB", 2**20)):
            plt.figure(figsize=(max(8, len(training) * .8), 5))
            labels = [r["run_id"] for r in training]
            x = list(range(len(training)))
            width = .8 / len(metrics)
            for index, metric in enumerate(metrics):
                plt.bar([p + index * width for p in x], [r[metric] / divisor for r in training], width, label=metric)
            plt.xticks([p + .4 for p in x], labels, rotation=30, ha="right")
            save(name, "Training (synthetic)", "run", ylabel)
    return made


def generate(*, report_id, artifact=None, runs=None):
    from accelerate_chess.replay import artifact_root
    if not ID.fullmatch(report_id):
        raise ValueError(f"invalid report ID: {report_id!r}")
    root = artifact_root(artifact)
    summary, sources = collect(root, runs)
    if REPORTS.is_symlink():
        raise ValueError("repository report directory must not be a symlink")
    target = REPORTS / report_id
    if target.exists() or target.is_symlink():
        raise FileExistsError(f"report already exists: {target}")
    if importlib.util.find_spec("matplotlib") is None:
        raise RuntimeError("matplotlib is required to render benchmark PNG reports")
    REPORTS.mkdir(parents=True, exist_ok=True)
    target.mkdir()  # Atomic reservation; existing reports are never overwritten.
    try:
        figures = charts_for(summary, target)
        (target / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        (target / "metadata.json").write_text(json.dumps({"report_id": report_id, "source_runs": sources, "warnings": summary["warnings"]}, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        (target / "summary.md").write_text(markdown(summary, sources, figures), encoding="utf-8")
    except Exception:
        # Keep the reserved directory visible; a partial render must not be mistaken for success.
        raise
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report-id", required=True)
    parser.add_argument("--artifact-root")
    parser.add_argument("--runs", nargs="+", metavar="RUN_ID")
    args = parser.parse_args()
    print(generate(report_id=args.report_id, artifact=args.artifact_root, runs=args.runs))


if __name__ == "__main__":
    main()
