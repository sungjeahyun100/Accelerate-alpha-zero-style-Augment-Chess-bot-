"""Regression checks for the CI success-cache trust boundary."""

from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).with_name("native-ci-reuse.py")
WORKFLOW = SOURCE.parents[1] / "workflows" / "native-bot.yml"
spec = importlib.util.spec_from_file_location("native_ci_reuse", SOURCE)
assert spec and spec.loader
reuse = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reuse)
NODE_VERSION_FUNCTION = reuse.node_version
PROFILE_INPUTS = (".github/scripts/native_validation_profile.py", *reuse.EXECUTION_SOURCE_PATHS)
CHECK_SOURCE = SOURCE.with_name("native-bot-check.py")
check_spec = importlib.util.spec_from_file_location("native_bot_check", CHECK_SOURCE)
assert check_spec and check_spec.loader
check = importlib.util.module_from_spec(check_spec)
check_spec.loader.exec_module(check)


def records(paths: dict[str, str]) -> bytes:
    return b"".join(f"100644 {oid} 0\t{path}\0".encode()
                    for path, oid in paths.items())


def rust_receipt(scope: str, **overrides) -> dict:
    return {
        "crate": "augment-chess-engine", "listed": True, "executed": True, "exitCode": 0,
        "required_tests": list(reuse.RUST_REQUIRED_TESTS),
        "required_engine_tests": list(reuse.RUST_REQUIRED_TESTS[:5]), "validationScope": scope,
        "packages": list(reuse.RUST_CORE_PACKAGES) if scope == "core" else ["--workspace"],
        "command": reuse.rust_test_command(scope), **overrides,
    }


def write_rust_receipt(directory: Path, scope: str, **overrides) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{scope}-scope.json").write_text(json.dumps(rust_receipt(scope, **overrides)), encoding="utf-8")
    (directory / f"{scope}-test-list.txt").write_text(
        "".join(f"tests::{name}: test\n" for name in reuse.RUST_REQUIRED_TESTS[:5]), encoding="utf-8")
    (directory / f"{scope}-test-output.txt").write_text(
        "".join(f"test tests::{name} ... ok\n" for name in reuse.RUST_REQUIRED_TESTS), encoding="utf-8")


class SuccessCacheTests(unittest.TestCase):
    def setUp(self):
        runtime = patch.object(reuse, "node_version", return_value="v22.22.0")
        runtime.start()
        self.addCleanup(runtime.stop)
        self.profile_identity = {
            "rulesVersion": "augment-site-20260928-e5ed84fcf8e72a24",
            "catalogVersion": "c" * 64, "catalogSha256": "d" * 64,
            "sourcePublicCatalogHash": "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
            "profileVersion": "accelerate-headless-semantic-v7-faithful-init-v1",
            "executionProfileSha256": "e" * 64, "executionManifestFileSha256": "f" * 64,
            "sourceMainSha256": "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c",
            "parserSha256": "fdb08546776ec6228b03e8d02b40d4ab3255bae5f401adba7ff5dad927ac5c9c",
        }
        profile = patch.object(reuse, "execution_identity", return_value=self.profile_identity)
        profile.start()
        self.addCleanup(profile.stop)

    def test_same_blobs_reuse_across_commits_but_engine_change_invalidates(self):
        paths = {path: "a" * 40 for path in (
            "Cargo.toml", "Cargo.lock", ".github/workflows/native-bot.yml",
            ".github/scripts/native-ci-reuse.py", "projects/augment-chess/engine/src/lib.rs", *PROFILE_INPUTS)}
        with (patch.object(reuse.subprocess, "run", return_value=SimpleNamespace(stdout=records(paths))),
              patch.object(reuse.platform, "system", return_value="TestOS"),
              patch.object(reuse.platform, "machine", return_value="x86_64"),
              patch.object(reuse.platform, "python_version", return_value="3.12.0")):
            first = reuse.identity("rust")
            self.assertEqual(first, reuse.identity("rust"))
        paths["projects/augment-chess/engine/src/lib.rs"] = "b" * 40
        with (patch.object(reuse.subprocess, "run", return_value=SimpleNamespace(stdout=records(paths))),
              patch.object(reuse.platform, "system", return_value="TestOS"),
              patch.object(reuse.platform, "machine", return_value="x86_64"),
              patch.object(reuse.platform, "python_version", return_value="3.12.0")):
            self.assertNotEqual(first["key"], reuse.identity("rust")["key"])

    def test_native_identity_requires_moved_python_package_and_lock(self):
        paths = {path: "a" * 40 for path in (
            "Cargo.toml", "Cargo.lock", ".github/workflows/native-bot.yml",
            ".github/scripts/native-ci-reuse.py", "uv.lock",
            "projects/accelerate/pyproject.toml", *PROFILE_INPUTS)}
        with patch.object(reuse.subprocess, "run", return_value=SimpleNamespace(stdout=records(paths))):
            with self.assertRaisesRegex(RuntimeError, "accelerate_chess/__init__"):
                reuse.identity("native")

    def test_adapter_success_tracks_oracle_without_engine_dependency(self):
        self.assertNotIn("projects/augment-chess/engine/", reuse.SCOPES["adapter"])
        paths = {path: "a" * 40 for path in (
            ".github/workflows/native-bot.yml", ".github/scripts/native-ci-reuse.py",
            ".github/scripts/native-bot-check.py", "package.json",
            "projects/augment-chess/contracts/catalog/site-20260928.json",
            "projects/augment-chess/oracle/game-adapter/src/index.js",
            "projects/augment-chess/tests/site-adapter/parity/latest-client.test.cjs",
            "projects/augment-chess/tests/differential/v7-native-differential.cjs",
            "projects/augment-chess/tests/differential/v7-native-response.test.cjs", *PROFILE_INPUTS)}
        with (patch.object(reuse.subprocess, "run", return_value=SimpleNamespace(stdout=records(paths))),
              patch.object(reuse.platform, "system", return_value="TestOS"),
              patch.object(reuse.platform, "machine", return_value="x86_64"),
              patch.object(reuse.platform, "python_version", return_value="3.12.0")):
            first = reuse.identity("adapter")
            self.assertEqual(first, reuse.identity("adapter"))
        paths["projects/augment-chess/oracle/game-adapter/src/index.js"] = "b" * 40
        with (patch.object(reuse.subprocess, "run", return_value=SimpleNamespace(stdout=records(paths))),
              patch.object(reuse.platform, "system", return_value="TestOS"),
              patch.object(reuse.platform, "machine", return_value="x86_64"),
              patch.object(reuse.platform, "python_version", return_value="3.12.0")):
            self.assertNotEqual(first["key"], reuse.identity("adapter")["key"])

    def test_unmerged_index_and_changed_marker_are_rejected(self):
        paths = {path: "a" * 40 for path in (
            "Cargo.toml", "Cargo.lock", ".github/workflows/native-bot.yml",
            ".github/scripts/native-ci-reuse.py", *PROFILE_INPUTS)}
        unresolved = records(paths).replace(b" 0\tCargo.toml", b" 2\tCargo.toml")
        with patch.object(reuse.subprocess, "run", return_value=SimpleNamespace(stdout=unresolved)):
            with self.assertRaisesRegex(RuntimeError, "stage-zero"):
                reuse.identity("rust")
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "rust-success.json"
            marker.write_text(json.dumps({"key": "stale"}), encoding="utf-8")
            with (patch.object(reuse, "identity", return_value={"key": "current", "inputFingerprint": "current"}),
                  patch.object(reuse, "marker_path", return_value=marker),
                  patch.object(sys, "argv", [str(SOURCE), "verify", "rust"])):
                with self.assertRaisesRegex(RuntimeError, "identity differs"):
                    reuse.main()

    def test_real_git_index_changes_for_workflow_script_package_and_node(self):
        paths = (
            ".github/workflows/native-bot.yml", ".github/scripts/native-ci-reuse.py",
            ".github/scripts/native-bot-check.py", "Cargo.toml", "Cargo.lock",
            ".gitignore", "package.json", "uv.lock",
            "projects/accelerate/pyproject.toml",
            "projects/accelerate/native/Cargo.toml",
            "projects/accelerate/python/accelerate_chess/__init__.py",
            "projects/augment-chess/contracts/catalog/site-20260928.json",
            "projects/augment-chess/oracle/game-adapter/src/index.js",
            "projects/augment-chess/tests/site-adapter/parity/latest-client.test.cjs",
            "projects/augment-chess/tests/differential/v7-native-differential.cjs",
            "projects/augment-chess/tests/differential/v7-native-response.test.cjs",
            *PROFILE_INPUTS,
        )
        with tempfile.TemporaryDirectory() as directory:
            repository = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=repository, check=True, capture_output=True)
            for name in paths:
                target = repository / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text("original\n", encoding="utf-8")
            subprocess.run(["git", "add", "-A"], cwd=repository, check=True, capture_output=True)
            with patch.object(reuse, "REPOSITORY", repository):
                first = {scope: reuse.identity(scope)["key"] for scope in reuse.SCOPES}
                for name in (".github/workflows/native-bot.yml",
                             ".github/scripts/native-bot-check.py", "package.json",
                             "projects/accelerate/native/Cargo.toml",
                             "projects/augment-chess/contracts/catalog/execution-profile-20260928.json",
                             "projects/augment-chess/contracts/catalog/site-20260928.json",
                             "projects/augment-chess/tests/differential/v7-native-response.test.cjs"):
                    target = repository / name
                    target.write_text("changed\n", encoding="utf-8")
                    subprocess.run(["git", "add", "--", name], cwd=repository,
                                   check=True, capture_output=True)
                    changed = {scope: reuse.identity(scope)["key"] for scope in reuse.SCOPES}
                    affected = ({"native", "adapter"} if name == "package.json" else
                                {"rust", "native"} if name == "projects/accelerate/native/Cargo.toml" else
                                {"native", "adapter"} if name == "projects/augment-chess/tests/differential/v7-native-response.test.cjs" else
                                {"rust", "core", "native", "adapter"})
                    for scope in reuse.SCOPES:
                        self.assertEqual(first[scope] != changed[scope], scope in affected,
                                         f"{scope} omitted an input change from {name}")
                    target.write_text("original\n", encoding="utf-8")
                    subprocess.run(["git", "add", "--", name], cwd=repository,
                                   check=True, capture_output=True)
                with patch.object(reuse, "node_version", return_value="v22.23.0"):
                    for scope in reuse.SCOPES:
                        self.assertNotEqual(first[scope], reuse.identity(scope)["key"])
                profile_path = repository / reuse.EXECUTION_SOURCE_PATHS[0]
                subprocess.run(["git", "rm", "--cached", "--", reuse.EXECUTION_SOURCE_PATHS[0]], cwd=repository,
                               check=True, capture_output=True)
                self.assertTrue(profile_path.is_file())
                for scope in reuse.SCOPES:
                    with self.assertRaisesRegex(RuntimeError, "execution-profile-20260928.json"):
                        reuse.identity(scope)

    def test_node_runtime_must_be_selected_v22_and_exact_patch_is_recorded(self):
        with patch.object(reuse.subprocess, "run", return_value=SimpleNamespace(stdout="v22.22.1\n")):
            self.assertEqual(NODE_VERSION_FUNCTION(), "v22.22.1")
        with patch.object(reuse.subprocess, "run", return_value=SimpleNamespace(stdout="v20.20.2\n")):
            with self.assertRaisesRegex(RuntimeError, "Node 22"):
                NODE_VERSION_FUNCTION()

    def test_workflow_records_only_after_successful_final_validation(self):
        source = WORKFLOW.read_text(encoding="utf-8")
        lines = source.splitlines()

        def step(name):
            start = next(index for index, line in enumerate(lines)
                         if line.strip() == f"- name: {name}")
            end = next((index for index in range(start + 1, len(lines))
                        if lines[index].startswith("      - ")), len(lines))
            return "\n".join(lines[start:end])

        for scope, name, final_step in (
            ("rust", "Record successful Rust validation", "rust_tests"),
            ("native", "Record successful installed validation", "native_differential"),
            ("adapter", "Record successful frozen adapter validation", "adapter_audit"),
        ):
            expected = ("if: ${{ success() && steps.verified.outputs.cache-hit != 'true' "
                        f"&& steps.{final_step}.outcome == 'success' }}}}")
            self.assertIn(expected, step(name))
            command = 'record "$ACCELERATE_RUST_SCOPE"' if scope == "rust" else f"record {scope}"
            self.assertIn(command, step(name))
            self.assertIn(f"id: {final_step}", source)
        self.assertNotIn("continue-on-error: true", source)
        native_job = source.split("  native-bot:\n", 1)[1].split("  game-adapter:\n", 1)[0]
        self.assertLess(native_job.index("actions/setup-node@"), native_job.index("key native"))

    def test_record_rejects_missing_or_failed_receipts_before_writing_marker(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "reports" / "native-bot" / "TestOS" / "rust-success.json"
            with (patch.object(reuse, "identity", return_value={"key": "current", "inputFingerprint": "current"}),
                  patch.object(reuse, "marker_path", return_value=marker),
                  patch.object(sys, "argv", [str(SOURCE), "record", "rust"])):
                with self.assertRaisesRegex(RuntimeError, "Rust test scope report"):
                    reuse.main()
                self.assertFalse(marker.exists())
                write_rust_receipt(marker.parent, "rust", listed=False)
                with self.assertRaisesRegex(RuntimeError, "lacks executed required tests"):
                    reuse.main()
                self.assertFalse(marker.exists())
                write_rust_receipt(marker.parent, "rust")
                reuse.main()
                self.assertTrue(marker.is_file())
                with patch.object(sys, "argv", [str(SOURCE), "verify", "rust"]):
                    reuse.main()
                    write_rust_receipt(marker.parent, "rust", listed=False)
                    with self.assertRaisesRegex(RuntimeError, "lacks executed required tests"):
                        reuse.main()

    def test_core_and_workspace_have_distinct_keys_markers_and_source_closures(self):
        paths = {path: "a" * 40 for path in (
            "Cargo.toml", "Cargo.lock", ".github/workflows/native-bot.yml",
            ".github/scripts/native-ci-reuse.py", *PROFILE_INPUTS)}
        # Windows platform.system()도 subprocess를 쓰므로 Git index mock과 분리한다.
        with (patch.object(reuse.subprocess, "run", return_value=SimpleNamespace(stdout=records(paths))),
              patch.object(reuse.platform, "system", return_value="TestOS"),
              patch.object(reuse.platform, "machine", return_value="x86_64"),
              patch.object(reuse.platform, "python_version", return_value="3.12.0")):
            self.assertNotEqual(reuse.identity("core")["key"], reuse.identity("rust")["key"])
        for path in ("projects/accelerate/native/", "projects/accelerate/runtime/"):
            self.assertNotIn(path, reuse.SCOPES["core"])
            self.assertIn(path, reuse.SCOPES["rust"])
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {"RUNNER_TEMP": directory}):
            self.assertNotEqual(reuse.marker_path("core"), reuse.marker_path("rust"))

    def test_core_success_cannot_be_verified_as_workspace_success(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "rust-success.json"
            marker.write_text(json.dumps({"scope": "core", "key": "core-key"}), encoding="utf-8")
            with (patch.object(reuse, "identity", return_value={"scope": "rust", "key": "rust-key"}),
                  patch.object(reuse, "marker_path", return_value=marker),
                  patch.object(sys, "argv", [str(SOURCE), "verify", "rust"])):
                with self.assertRaisesRegex(RuntimeError, "identity differs"):
                    reuse.main()

    def test_rust_receipts_bind_scope_package_command_and_executed_tests(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "reports" / "native-bot" / "TestOS"
            marker = root / "core-success.json"
            write_rust_receipt(root, "core")
            reuse.validate_evidence("core", marker)
            for field, changed in (("validationScope", "rust"), ("packages", ["--workspace"]),
                                   ("command", reuse.rust_test_command("rust"))):
                write_rust_receipt(root, "core", **{field: changed})
                with self.assertRaisesRegex(RuntimeError, "requested validation scope or command"):
                    reuse.validate_evidence("core", marker)
            for field, changed in (("executed", False), ("exitCode", 1), ("exitCode", False),
                                   ("required_tests", list(reuse.RUST_REQUIRED_TESTS[:-1]))):
                write_rust_receipt(root, "core", **{field: changed})
                with self.assertRaisesRegex(RuntimeError, "lacks executed required tests"):
                    reuse.validate_evidence("core", marker)
            write_rust_receipt(root, "core")
            output = root / "core-test-output.txt"
            # 목록 존재·exit 0만으로 필수 검사를 실행한 것으로 인정하지 않는다.
            output.write_text("test tests::one_unrelated_test ... ok\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "executed output lacks passed required tests"):
                reuse.validate_evidence("core", marker)
            write_rust_receipt(root, "core")
            output.write_text(output.read_text(encoding="utf-8").replace(" ... ok", " ... ignored", 1), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "executed output lacks passed required tests"):
                reuse.validate_evidence("core", marker)

    def test_core_and_workspace_reports_are_not_interchangeable(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "reports" / "native-bot" / "TestOS"
            write_rust_receipt(root, "rust")
            with self.assertRaisesRegex(RuntimeError, "Rust test scope report"):
                reuse.validate_evidence("core", root / "core-success.json")

    def test_requested_scope_rejects_conflicting_and_invalid_booleans(self):
        for core_only, adapter_only, expected in (("false", "false", "rust"), ("true", "false", "core"),
                                                  ("false", "true", "adapter")):
            with patch.dict(os.environ, {"ACCELERATE_CI_CORE_ONLY": core_only,
                                         "ACCELERATE_CI_ADAPTER_ONLY": adapter_only}):
                self.assertEqual(check.validation_scope(), expected)
        with patch.dict(os.environ, {"ACCELERATE_CI_CORE_ONLY": "true", "ACCELERATE_CI_ADAPTER_ONLY": "true"}):
            with self.assertRaisesRegex(RuntimeError, "cannot both be true"):
                check.validation_scope()
        with patch.dict(os.environ, {"ACCELERATE_CI_CORE_ONLY": "1", "ACCELERATE_CI_ADAPTER_ONLY": "false"}):
            with self.assertRaisesRegex(RuntimeError, "requires true or false"):
                check.validation_scope()

    def test_rust_runner_preserves_failure_and_rejects_list_only_success(self):
        with tempfile.TemporaryDirectory() as directory:
            reports = Path(directory) / "reports"
            locations = (Path(directory), "TestOS", Path(directory), reports, Path(directory) / "python")
            failed = subprocess.CompletedProcess(reuse.rust_test_command("core"), 101,
                                                 stdout="test failures\n", stderr="exact compiler error\n")
            with (patch.object(check, "locations", return_value=locations),
                  patch.object(check.subprocess, "run", return_value=failed)):
                with self.assertRaises(subprocess.CalledProcessError) as error:
                    check.rust_scope("core")
                self.assertEqual(error.exception.returncode, 101)
                self.assertIn("exact compiler error", (reports / "core-test-output.txt").read_text(encoding="utf-8"))
                self.assertFalse((reports / "core-scope.json").exists())
            list_only = subprocess.CompletedProcess(reuse.rust_test_command("core"), 0,
                                                    stdout="a_test: test\n", stderr="")
            with (patch.object(check, "locations", return_value=locations),
                  patch.object(check.subprocess, "run", return_value=list_only)):
                with self.assertRaisesRegex(RuntimeError, "did not execute required tests successfully"):
                    check.rust_scope("core")
                self.assertFalse((reports / "core-scope.json").exists())

    def test_rust_runner_executes_exact_core_command_and_reports_observed_success(self):
        with tempfile.TemporaryDirectory() as directory:
            reports = Path(directory) / "reports"
            locations = (Path(directory), "TestOS", Path(directory), reports, Path(directory) / "python")
            executed = subprocess.CompletedProcess(reuse.rust_test_command("core"), 0,
                stdout="".join(f"test tests::{name} ... ok\n" for name in reuse.RUST_REQUIRED_TESTS),
                stderr="one precise development warning\n")
            listed = subprocess.CompletedProcess(["cargo", "test", "--list"], 0,
                stdout="".join(f"tests::{name}: test\n" for name in reuse.RUST_REQUIRED_TESTS[:5]), stderr="")
            with (patch.object(check, "locations", return_value=locations),
                  patch.object(check.subprocess, "run", side_effect=[executed, listed]) as command):
                check.rust_scope("core")
                self.assertEqual(command.call_args_list[0].args[0], reuse.rust_test_command("core"))
                self.assertNotIn("--workspace", command.call_args_list[0].args[0])
            report = json.loads((reports / "core-scope.json").read_text(encoding="utf-8"))
            self.assertEqual(report["validationScope"], "core")
            self.assertEqual(report["exitCode"], 0)
            self.assertEqual(report["required_tests"], list(reuse.RUST_REQUIRED_TESTS))
            self.assertIn("one precise development warning", (reports / "core-test-output.txt").read_text(encoding="utf-8"))

    def test_native_receipt_requires_passed_installed_differential(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "reports" / "native-bot" / "TestOS" / "native-success.json"
            marker.parent.mkdir(parents=True)
            differential = marker.parents[2] / "v7-native-differential" / "report.json"
            differential.parent.mkdir(parents=True)
            (marker.parent / "packaging.json").write_text(json.dumps({
                "installed": True, "execution_identity": self.profile_identity,
                "sdist_sha256":"a" * 64,"wheel_sha256":"b" * 64,
                "required_source_sha256": {path:self.profile_identity["executionManifestFileSha256"]
                                           for path in reuse.EXECUTION_SOURCE_PATHS},
                "observation_policy": {
                    "source_distribution_match": True,
                    "installed_wheel_match":True,"native_module_sha256":"a" * 64,
                    "execution_identity": self.profile_identity,
                    "native_abi": {"abi":"abi3","python_minimum":"3.12",
                                   "wheel_tags":["cp312-abi3-testos_x86_64"],
                                   "source_manifest_verified":True,"installed_binary_match":True},
                    "game_adapter_draft_boundary": {
                        name: True for name in (
                            "installed_exports_match", "source_schema_match", "ordered_public_candidates",
                            "unknown_field_rejected", "parent_revision_preserved", "parent_public_observation_preserved",
                            "branch_revision_changed", "public_observation_valid", "private_transition_event_omitted",
                        )
                    },
                },
            }), encoding="utf-8")
            (marker.parent / "test-scope.json").write_text(json.dumps({
                "skips": 0, "default_weighted_conditioning_modes": ["normal", "chaos", "grand"],
                "game_adapter_draft_modes": ["normal", "chaos", "grand"],
            }), encoding="utf-8")
            differential.write_text(json.dumps({
                "status": "probe-error", "decision": "NO-GO",
            }), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "differential success evidence"):
                reuse.validate_evidence("native", marker)
            differential.write_text(json.dumps({
                "status": "pass", "decision": "bounded-P8-probe-pass-only",
                "source": {"sha256": self.profile_identity["sourceMainSha256"],
                           "profile": self.profile_identity["profileVersion"],
                           "rulesVersion": self.profile_identity["rulesVersion"],
                           "catalogVersion": self.profile_identity["catalogVersion"]},
            }), encoding="utf-8")
            reuse.validate_evidence("native", marker)
            packaging_path = marker.parent / "packaging.json"
            packaging = json.loads(packaging_path.read_text(encoding="utf-8"))
            packaging["observation_policy"]["game_adapter_draft_boundary"]["parent_revision_preserved"] = False
            packaging_path.write_text(json.dumps(packaging), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "parent_revision_preserved"):
                reuse.validate_evidence("native", marker)
            packaging["observation_policy"]["game_adapter_draft_boundary"]["parent_revision_preserved"] = True
            packaging_path.write_text(json.dumps(packaging), encoding="utf-8")
            scope_path = marker.parent / "test-scope.json"
            tests = json.loads(scope_path.read_text(encoding="utf-8"))
            tests["game_adapter_draft_modes"].remove("grand")
            scope_path.write_text(json.dumps(tests), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "required draft mode"):
                reuse.validate_evidence("native", marker)
            tests["game_adapter_draft_modes"].append("grand")
            scope_path.write_text(json.dumps(tests), encoding="utf-8")
            for field in ("profileVersion", "catalogVersion", "executionManifestFileSha256"):
                packaging["execution_identity"][field] = "stale"
                packaging_path.write_text(json.dumps(packaging), encoding="utf-8")
                with self.assertRaisesRegex(RuntimeError, field):
                    reuse.validate_evidence("native", marker)
                packaging["execution_identity"][field] = self.profile_identity[field]
            packaging["observation_policy"]["native_abi"]["abi"] = "cp312"
            packaging_path.write_text(json.dumps(packaging), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "ABI/platform"):
                reuse.validate_evidence("native", marker)
            packaging["observation_policy"]["native_abi"]["abi"] = "abi3"
            packaging_path.write_text(json.dumps(packaging), encoding="utf-8")
            source_report = json.loads(differential.read_text(encoding="utf-8"))
            source_report["source"]["profile"] = "accelerate-headless-semantic-v7"
            differential.write_text(json.dumps(source_report), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "native differential.*profile"):
                reuse.validate_evidence("native", marker)

    def test_adapter_receipt_requires_all_successful_platform_shards(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "reports" / "native-bot" / "Linux" / "adapter-success.json"
            marker.parent.mkdir(parents=True)
            (marker.parent / "frozen-source.json").write_text(json.dumps({
                "schemaVersion": 1, "files": [{"name": "legacy-main.js"}],
            }), encoding="utf-8")
            source_sha = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            (marker.parent / "current-client-source.json").write_text(json.dumps({
                "schemaVersion": 1, "executionIdentity": self.profile_identity,
                "files": [{"name": "main-OahWs0tU.js", "sha256": source_sha},
                          {"name":"acorn-8.15.0.js","sha256":self.profile_identity["parserSha256"]}],
            }), encoding="utf-8")
            audit_dir = marker.parents[2] / "site-adapter"
            audit_dir.mkdir()
            with (patch.object(reuse.platform, "system", return_value="Linux"),
                  patch.dict(os.environ, {"ACCELERATE_ADAPTER_SHARDS": "0 2 4 6"})):
                with self.assertRaisesRegex(RuntimeError, "audit shard 0"):
                    reuse.validate_evidence("adapter", marker)
                for shard in (0, 2, 4, 6):
                    (audit_dir / f"card-surface-site-20260928-shard-{shard}-of-8.json").write_text(
                        json.dumps({"baseline": "site-20260928", "sourceSha256": source_sha,
                                    "rulesVersion":self.profile_identity["rulesVersion"],
                                    "catalogVersion":self.profile_identity["catalogVersion"],
                                    "profileVersion":self.profile_identity["profileVersion"],
                                    "shardIndex": shard, "shardCount": 8,
                                    "styles": ["normal", "chaos", "grand"],
                                    "status": "bounded-probe-complete-with-reachability-gaps",
                                    "errorCells": 0, "unprobedCells": 0,
                                    "selectedCards": 32, "probedCells": 96}), encoding="utf-8")
                reuse.validate_evidence("adapter", marker)
                incomplete = audit_dir / "card-surface-site-20260928-shard-6-of-8.json"
                report = json.loads(incomplete.read_text(encoding="utf-8"))
                report["unprobedCells"] = 1
                incomplete.write_text(json.dumps(report), encoding="utf-8")
                with self.assertRaisesRegex(RuntimeError, "shard 6 lacks complete"):
                    reuse.validate_evidence("adapter", marker)
                report["unprobedCells"] = 0
                for field in ("profileVersion", "catalogVersion"):
                    report[field] = "legacy"
                    incomplete.write_text(json.dumps(report), encoding="utf-8")
                    with self.assertRaisesRegex(RuntimeError, field):
                        reuse.validate_evidence("adapter", marker)
                    report[field] = self.profile_identity[field]


if __name__ == "__main__":
    unittest.main()
