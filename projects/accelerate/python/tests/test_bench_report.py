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
