"""CPU-only fixtures for the derived local performance report."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
from types import ModuleType
import unittest
from unittest.mock import patch

from accelerate_chess.bench import report


BASE = {"benchmark_version": "local-performance-v1", "git_sha": "a" * 40,
        "timestamp": "2026-10-04T00:00:00+00:00", "cpu": "Test CPU", "logical_cpu_count": 8,
        "gpu": "Test GPU", "gpu_vram_bytes": 8 * 2**30, "python_version": "3.12",
        "rust_version": "rustc test", "torch_version": "2.14", "cuda_version": "13.0",
        "hostname": "private-host"}


def payload(kind, config, results, **changes):
    return {**BASE, "kind": kind, "config": config, "results": results, **changes}


def engine(workers=1):
    return payload("engine", {"workers": workers, "artifact_root": "/private/source"},
                   {"fork": {"status": "ok", "ops_per_second": 100 * workers},
                    "chance_transition": {"status": "unsupported", "reason": "no API"}})


def inference(dtype="fp32", status="ok"):
    row = {"model_name": "resnet-s", "dtype": dtype, "batch_size": 16,
           "candidate_count": 32, "candidate_nodes": 3, "synthetic": True, "status": status}
    if status == "ok":
        row.update(positions_per_second=120, latency_mean_ms=10, latency_p50_ms=9,
                   latency_p95_ms=12, peak_vram_bytes=2**30)
    else:
        row["reason"] = "ModelBatch limit" if status == "unsupported" else "GPU OOM"
    return payload("inference", {"model": "resnet-s", "dtype": dtype}, {"measurements": [row]})


def typed(family="mask-resnet", dtype="fp32", batch=16, hidden=128, rate=100, power="performance"):
    params = 4_196_226 if family == "mask-resnet" else (4_159_570 if hidden == 208 else 2_232_850)
    config = {"model_family": family, "profile": "normal", "dtype": dtype, "device": "cuda",
              "batch_size": batch, "transformer_hidden_dim": hidden, "steps": 100, "warmup": 3,
              "torch_threads": 1, "torch_interop_threads": 1}
    result = {"status": "ok", "synthetic": True, "parameter_count": params,
              "trainable_parameter_count": params - 1000, "parameter_bytes": params * 4,
              "workload_shape": {"record_count": 16, "candidate_count": 80, "batch_size": batch},
              "architecture_config": {"typed": {"hidden_dim": hidden}, "blocks": 4},
              "samples_per_second": rate, "steps_per_second": rate / batch,
              "mean_step_ms": batch * 1000 / rate, "forward_ms": 10, "backward_ms": 20,
              "optimizer_ms": 5, "peak_vram_bytes": 400 * 2**20}
    return payload("typed-training", config, result, power_profile=power)


class ReportTests(unittest.TestCase):
    def setUp(self):
        from tempfile import TemporaryDirectory
        self.temp = TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "external"
        self.root.mkdir()

    def put(self, run, data):
        folder = self.root / "reports" / run
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / f"{data['kind']}.json"
        path.write_text(json.dumps(data), encoding="utf-8")
        return path

    def test_engine_parse_and_worker_aggregation(self):
        self.put("w1", engine(1))
        self.put("w2", engine(2))
        summary, sources = report.collect(self.root)
        rows = summary["measurements"]["engine"]
        self.assertEqual([(r["workers"], r["ops_per_second"]) for r in rows], [(1, 100), (2, 200)])
        self.assertEqual(len(sources), 2)
        self.assertEqual(len(summary["incomplete"]), 2)
        self.assertNotIn("artifact_root", sources[0]["config"])
        self.assertEqual(sources[0]["source_json_path"], "reports/w1/engine.json")

    def test_inference_dtype_separation_and_status(self):
        self.put("fp32", inference())
        self.put("bf16", inference("bf16"))
        self.put("oom", inference("bf16", "oom"))
        self.put("unsupported", inference("fp32", "unsupported"))
        summary, _ = report.collect(self.root)
        self.assertEqual({r["dtype"] for r in summary["measurements"]["inference"]}, {"fp32", "bf16"})
        self.assertEqual(len(summary["measurements"]["inference"]), 2)
        self.assertEqual({r["status"] for r in summary["incomplete"]}, {"oom", "unsupported"})
        self.assertTrue(all("positions_per_second" not in r for r in summary["incomplete"]))

    def test_failure_reason_redacts_host_and_absolute_path(self):
        failed = inference("fp32", "oom")
        failed["results"]["measurements"][0]["reason"] = "private-host wrote /home/user/secret/file"
        self.put("failed", failed)
        summary, sources = report.collect(self.root)
        rendered = report.markdown(summary, sources, [])
        self.assertNotIn("private-host", rendered)
        self.assertNotIn("/home/user", rendered)
        self.assertIn("[path]", rendered)

    def test_pipeline_unsupported_and_training(self):
        self.put("pipeline", payload("pipeline", {}, {"games_per_hour": {"status": "unsupported", "reason": "no queue"}}))
        self.put("training", payload("training", {"model": "resnet-s", "profile": "small", "dtype": "fp32", "device": "cpu", "batch_size": 2},
                                     {"status": "ok", "synthetic": True, "samples_per_second": 10, "steps_per_second": 5,
                                      "mean_step_ms": 200, "forward_ms": 50, "backward_ms": 80, "optimizer_ms": 30,
                                      "peak_vram_bytes": 0, "replay_loading": "unsupported: no replay"}))
        summary, sources = report.collect(self.root)
        self.assertEqual(summary["measurements"]["training"][0]["peak_vram_bytes"], 0)
        self.assertIn("Synthetic optimizer", report.markdown(summary, sources, []))
        self.assertIn("no queue", report.markdown(summary, sources, []))
        self.assertNotIn("private-host", report.markdown(summary, sources, []))

    def test_git_sha_warning_and_hardware_rejection(self):
        self.put("first", engine())
        self.put("second", engine() | {"git_sha": "b" * 40})
        summary, _ = report.collect(self.root)
        self.assertEqual(len(summary["warnings"]), 1)
        self.put("third", engine() | {"cpu": "Different CPU"})
        with self.assertRaisesRegex(ValueError, "different CPU/GPU"):
            report.collect(self.root)
        selected, _ = report.collect(self.root, ["first", "second"])
        self.assertEqual(len(selected["source_runs"]), 2)

    def test_malformed_and_unsupported_version(self):
        bad = self.root / "reports" / "bad"
        bad.mkdir(parents=True)
        (bad / "engine.json").write_text("{broken", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "malformed"):
            report.collect(self.root, ["bad"])
        (bad / "engine.json").write_text(json.dumps(engine() | {"benchmark_version": "future"}), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "unsupported benchmark version"):
            report.collect(self.root, ["bad"])
        malformed = engine()
        malformed["results"]["fork"]["ops_per_second"] = float("nan")
        (bad / "engine.json").write_text(json.dumps(malformed), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "nonfinite JSON constant"):
            report.collect(self.root, ["bad"])

    def test_report_id_collision_preserves_existing_output(self):
        self.put("source", engine())
        output = Path(self.temp.name) / "results"
        output.mkdir()
        existing = output / "taken"
        existing.mkdir()
        marker = existing / "summary.md"
        marker.write_text("original", encoding="utf-8")
        fake = ModuleType("accelerate_chess.replay")
        fake.artifact_root = lambda explicit: self.root
        with patch.dict(sys.modules, {"accelerate_chess.replay": fake}), patch.object(report, "REPORTS", output):
            with self.assertRaises(FileExistsError):
                report.generate(report_id="taken", artifact=str(self.root))
        self.assertEqual(marker.read_text(encoding="utf-8"), "original")

    def test_missing_matplotlib_does_not_reserve_report_id(self):
        self.put("source", engine())
        output = Path(self.temp.name) / "results"
        fake = ModuleType("accelerate_chess.replay")
        fake.artifact_root = lambda explicit: self.root
        with patch.dict(sys.modules, {"accelerate_chess.replay": fake}), patch.object(report, "REPORTS", output), \
             patch.object(report.importlib.util, "find_spec", return_value=None):
            with self.assertRaisesRegex(RuntimeError, "matplotlib is required"):
                report.generate(report_id="new", artifact=str(self.root))
        self.assertFalse((output / "new").exists())

    def test_typed_aggregate_comparison_and_precision(self):
        for index, rate in enumerate((90, 100, 110), 1):
            self.put(f"mask-fp32-{index}", typed(rate=rate))
        self.put("transformer-fp32", typed("entity-transformer", hidden=208, rate=150))
        self.put("mask-bf16", typed(dtype="bf16", rate=120))
        self.put("transformer-h128", typed("entity-transformer", hidden=128, rate=130))
        self.put("mask-b32", typed(batch=32, rate=200))
        summary, sources = report.collect(self.root)
        aggregates = summary["aggregates"]["typed-training"]
        self.assertEqual(len(aggregates), 5)
        mask = next(a for a in aggregates if a["model"] == "mask-resnet" and a["dtype"] == "fp32" and a["batch_size"] == 16)
        self.assertEqual(mask["source_run_ids"], [f"mask-fp32-{i}" for i in (1, 2, 3)])
        self.assertEqual(mask["count"], 3)
        self.assertEqual(mask["metrics"]["samples_per_second"],
                         {"mean": 100, "median": 100, "std": 10, "cv_percent": 10,
                          "min": 90, "max": 110})
        pair = summary["comparisons"]["typed-training"]
        self.assertEqual(len(pair), 1)
        self.assertAlmostEqual(pair[0]["delta_percent"]["samples_per_second"], 50)
        self.assertAlmostEqual(pair[0]["parameter_difference_percent"], -0.87354, places=3)
        scaling = summary["precision_scaling"]
        self.assertEqual(len(scaling), 1)
        self.assertAlmostEqual(scaling[0]["delta_percent"], 20)
        self.assertIsNone(next(a for a in aggregates if a["model"] == "entity-transformer")["metrics"]["samples_per_second"]["std"])
        rendered = report.markdown(summary, sources, [])
        for section in ("Typed architecture aggregate", "Matched-size comparison", "Stability", "Precision scaling"):
            self.assertIn(section, rendered)

    def test_typed_config_and_power_separation(self):
        self.put("base", typed())
        changed = typed()
        changed["results"]["architecture_config"]["blocks"] = 8
        self.put("blocks", changed)
        self.put("balanced", typed(power="balanced"))
        self.put("legacy", typed(power=None))
        summary, _ = report.collect(self.root)
        self.assertEqual(len(summary["aggregates"]["typed-training"]), 4)
        self.assertTrue(any("power profiles" in warning for warning in summary["warnings"]))
        malformed = typed(power="performance /private/path")
        self.put("malformed", malformed)
        with self.assertRaisesRegex(ValueError, "power_profile"):
            report.collect(self.root)

    def test_safe_prefix_selection(self):
        self.put("matched-a", typed())
        self.put("other", typed())
        summary, _ = report.collect(self.root, run_prefix="matched-")
        self.assertEqual(len(summary["source_runs"]), 1)
        with self.assertRaisesRegex(ValueError, "invalid run prefix"):
            report.collect(self.root, run_prefix="../")
        with self.assertRaisesRegex(ValueError, "cannot be combined"):
            report.collect(self.root, ["other"], run_prefix="matched-")

    @unittest.skipUnless(importlib.util.find_spec("matplotlib"), "matplotlib unavailable")
    def test_typed_aggregate_charts(self):
        self.put("mask", typed(batch=64))
        self.put("transformer", typed("entity-transformer", batch=64, hidden=208))
        self.put("mask-bf16", typed(dtype="bf16", batch=64))
        self.put("transformer-bf16", typed("entity-transformer", dtype="bf16", batch=64, hidden=208))
        summary, _ = report.collect(self.root)
        directory = Path(self.temp.name) / "charts"
        directory.mkdir()
        charts = report.charts_for(summary, directory)
        for name in ("typed-throughput-aggregate.png", "typed-step-time-aggregate.png",
                     "typed-vram-aggregate.png", "typed-breakdown-b64.png", "typed-bf16-scaling.png"):
            self.assertIn(name, charts)
            self.assertEqual((directory / name).read_bytes()[:8], b"\x89PNG\r\n\x1a\n")

    @unittest.skipUnless(importlib.util.find_spec("matplotlib"), "matplotlib unavailable in this environment")
    def test_png_and_complete_report(self):
        self.put("w1", engine(1))
        self.put("w2", engine(2))
        self.put("infer", inference())
        output = Path(self.temp.name) / "results"
        fake = ModuleType("accelerate_chess.replay")
        fake.artifact_root = lambda explicit: self.root
        with patch.dict(sys.modules, {"accelerate_chess.replay": fake}), patch.object(report, "REPORTS", output):
            target = report.generate(report_id="fresh", artifact=str(self.root))
        for name in ("engine-ops.png", "engine-worker-scaling.png", "inference-throughput.png", "inference-latency.png", "inference-vram.png"):
            self.assertEqual((target / name).read_bytes()[:8], b"\x89PNG\r\n\x1a\n")
        self.assertTrue((target / "summary.md").exists())
        self.assertEqual(len(json.loads((target / "metadata.json").read_text())["source_runs"]), 3)
        self.assertFalse((target / "training-throughput.png").exists())


if __name__ == "__main__":
    unittest.main()
