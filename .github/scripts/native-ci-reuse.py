"""동결 소스·실행 프로필에 연결된 native CI 성공 근거의 키와 유효성을 검사한다.

깨끗한 checkout의 stage 0 Git blob을 사용한다. 관련 없는 파일만 변경한 커밋은
근거를 재사용할 수 있으며 선택한 소스·검사·fixture·lock·profile·CI 명령이 바뀌면
키가 바뀐다. 운영 runtime contract가 profile과 composite catalog를 인증한다.
관련 workflow 검사를 모두 통과한 뒤에만 성공 marker를 쓰고 GitHub cache action은
해당 job이 성공한 경우에만 그 marker를 게시한다.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys

from native_validation_profile import (
    EXECUTION_SOURCE_PATHS, RUST_CORE_PACKAGES, RUST_REQUIRED_TESTS,
    execution_identity, require_execution_identity, rust_test_command,
)


REPOSITORY = Path(__file__).resolve().parents[2]
SHARED_PATHS = (
    "Cargo.toml", "Cargo.lock", ".github/repository-policy.json",
    ".github/scripts/", ".github/workflows/native-bot.yml",
    "packages/adapter-contract/", "packages/adapter-runtime/", "projects/accelerate/native/",
    "projects/accelerate/runtime/", "projects/augment-chess/contracts/",
    "projects/augment-chess/engine/", "projects/augment-chess/tools/v6-migration/",
)
CORE_PATHS = tuple(path for path in SHARED_PATHS
                   if path not in {"projects/accelerate/native/", "projects/accelerate/runtime/"})
NATIVE_EXTRA_PATHS = (
    ".gitignore", "package.json", "pyproject.toml", "uv.lock",
    "projects/accelerate/pyproject.toml",
    "projects/accelerate/NOTICE.md", "projects/accelerate/python/",
    "projects/augment-chess/oracle/",
    "projects/augment-chess/tests/differential/",
    "projects/augment-chess/tests/site-adapter/",
)
# Frozen JS parity has no Rust engine or wheel dependency; unrelated engine
# commits must not invalidate an already successful source-pinned audit.
ADAPTER_PATHS = (
    ".github/scripts/native-bot-check.py", ".github/scripts/native-ci-reuse.py",
    ".github/scripts/native_validation_profile.py",
    ".github/workflows/native-bot.yml", "package.json",
    "projects/augment-chess/contracts/",
    "projects/augment-chess/oracle/", "projects/augment-chess/tests/site-adapter/",
    "projects/augment-chess/tests/differential/v7-native-differential.cjs",
    "projects/augment-chess/tests/differential/v7-native-response.test.cjs",
)
SCOPES = {"rust": SHARED_PATHS, "core": CORE_PATHS, "native": SHARED_PATHS + NATIVE_EXTRA_PATHS,
          "adapter": ADAPTER_PATHS}


def node_version() -> str:
    result = subprocess.run(["node", "--version"], cwd=REPOSITORY,
                            check=True, capture_output=True, text=True)
    warning = getattr(result, "stderr", "")
    if warning:
        print(warning, end="" if warning.endswith("\n") else "\n", file=sys.stderr)
    version = result.stdout.strip()
    if not re.fullmatch(r"v22\.\d+\.\d+", version):
        raise RuntimeError(f"CI success reuse requires the selected Node 22 runtime, got {version!r}")
    return version


def identity(scope: str) -> dict[str, object]:
    result = subprocess.run(
        ["git", "ls-files", "--stage", "-z", "--", *SCOPES[scope]],
        cwd=REPOSITORY, check=True, capture_output=True,
    )
    if getattr(result, "stderr", b""):
        warning = result.stderr.decode("utf-8", errors="replace")
        print(warning, end="" if warning.endswith("\n") else "\n", file=sys.stderr)
    records = sorted(record for record in result.stdout.split(b"\0") if record)
    names = set()
    for record in records:
        header, separator, name = record.partition(b"\t")
        if not separator or len(header.split()) != 3 or header.split()[2] != b"0":
            raise RuntimeError("CI cache identity requires a resolved stage-zero Git index")
        names.add(name.decode("utf-8"))
    required = {".github/workflows/native-bot.yml", ".github/scripts/native-ci-reuse.py",
                ".github/scripts/native_validation_profile.py", *EXECUTION_SOURCE_PATHS}
    if scope in {"rust", "core", "native"}:
        required.update({"Cargo.toml", "Cargo.lock"})
    if scope == "native":
        required.update({".gitignore", "package.json", "uv.lock",
                         "projects/accelerate/pyproject.toml",
                         "projects/accelerate/python/accelerate_chess/__init__.py"})
    if scope == "adapter":
        required.update({"package.json", ".github/scripts/native-bot-check.py",
                         "projects/augment-chess/contracts/catalog/site-20260928.json",
                         "projects/augment-chess/oracle/game-adapter/src/index.js",
                         "projects/augment-chess/tests/site-adapter/parity/latest-client.test.cjs",
                         "projects/augment-chess/tests/differential/v7-native-differential.cjs",
                         "projects/augment-chess/tests/differential/v7-native-response.test.cjs"})
    missing = required - names
    if missing:
        raise RuntimeError(f"CI cache identity omitted required tracked inputs: {sorted(missing)}")
    selected_node = node_version()
    profile_identity = execution_identity(REPOSITORY)
    digest = hashlib.sha256()
    for value in ("native-ci-reuse-v3", scope, platform.system(), platform.machine(),
                  platform.python_version(), os.environ.get("ImageVersion", ""),
                  "rust-1.96.0", selected_node, "uv-0.12.19",
                  json.dumps(profile_identity, sort_keys=True, separators=(",", ":"))):
        digest.update(value.encode("utf-8") + b"\0")
    for record in records:
        digest.update(record + b"\0")
    fingerprint = digest.hexdigest()
    return {"schemaVersion": 2, "scope": scope, "inputFingerprint": fingerprint,
            "executionIdentity": profile_identity,
            "key": f"accelerate-{scope}-verified-v3-{platform.system()}-{fingerprint}"}


def marker_path(scope: str) -> Path:
    runner_temp = os.environ.get("RUNNER_TEMP")
    if not runner_temp:
        raise RuntimeError("RUNNER_TEMP is required for CI validation markers")
    root = Path(runner_temp).resolve() / "Accelerate"
    if root.is_relative_to(REPOSITORY):
        raise RuntimeError("CI validation markers must stay outside the checkout")
    return root / "reports" / "native-bot" / platform.system() / f"{scope}-success.json"


def report_json(path: Path, label: str) -> dict:
    if path.is_symlink() or not path.is_file():
        raise RuntimeError(f"{label} is missing or not a regular file")
    try:
        content = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise RuntimeError(f"{label} cannot be read as JSON: {error}") from error
    if not isinstance(content, dict):
        raise RuntimeError(f"{label} must be a JSON object")
    return content


def validate_evidence(scope: str, marker: Path) -> None:
    reports = marker.parents[2]
    job_reports = marker.parent
    if scope in {"rust", "core"}:
        listed = report_json(job_reports / f"{scope}-scope.json", "Rust test scope report")
        test_list = job_reports / f"{scope}-test-list.txt"
        if test_list.is_symlink() or not test_list.is_file():
            raise RuntimeError("Rust test list is missing or not a regular file")
        expected_packages = list(RUST_CORE_PACKAGES) if scope == "core" else ["--workspace"]
        if (listed.get("validationScope") != scope or listed.get("command") != rust_test_command(scope)
                or listed.get("packages") != expected_packages):
            raise RuntimeError(f"Rust {scope} test scope report differs from the requested validation scope or command")
        names = listed.get("required_tests")
        if (listed.get("listed") is not True or listed.get("crate") != "augment-chess-engine"
                or listed.get("executed") is not True or type(listed.get("exitCode")) is not int
                or listed["exitCode"] != 0 or names != list(RUST_REQUIRED_TESTS)
                or listed.get("required_engine_tests") != list(RUST_REQUIRED_TESTS[:5])):
            raise RuntimeError("Rust test scope report lacks executed required tests")
        listing = test_list.read_text(encoding="utf-8")
        if any(f"{name}: test" not in listing for name in RUST_REQUIRED_TESTS[:5]):
            raise RuntimeError("Rust test list does not contain every required test")
        output_path = job_reports / f"{scope}-test-output.txt"
        if output_path.is_symlink() or not output_path.is_file():
            raise RuntimeError("Rust executed test output is missing or not a regular file")
        output = output_path.read_text(encoding="utf-8")
        passed = {match.group(1).rsplit("::", 1)[-1] for match in
                  re.finditer(r"^test (\S+) \.\.\. ok$", output, re.MULTILINE)}
        missing = sorted(set(RUST_REQUIRED_TESTS) - passed)
        if missing:
            raise RuntimeError(f"Rust {scope} executed output lacks passed required tests: {missing}")
    elif scope == "native":
        packaging = report_json(job_reports / "packaging.json", "installed wheel report")
        tests = report_json(job_reports / "test-scope.json", "installed test scope report")
        differential = report_json(reports / "v7-native-differential" / "report.json",
                                   "source-pinned native differential report")
        installed_policy = packaging.get("observation_policy")
        conditioned_modes = tests.get("default_weighted_conditioning_modes")
        if (packaging.get("installed") is not True
                or not isinstance(installed_policy, dict)
                or installed_policy.get("source_distribution_match") is not True
                or tests.get("skips") != 0
                or not isinstance(conditioned_modes, list)
                or sorted(conditioned_modes) != ["chaos", "grand", "normal"]
                or differential.get("status") != "pass"
                or differential.get("decision") != "bounded-P8-probe-pass-only"):
            raise RuntimeError("installed wheel, test, or source differential success evidence is incomplete")
        expected_profile = execution_identity(REPOSITORY)
        require_execution_identity(packaging.get("execution_identity"), expected_profile, "installed wheel")
        require_execution_identity(installed_policy.get("execution_identity"), expected_profile, "installed native catalog")
        binary_hashes = {"sdist_sha256": packaging.get("sdist_sha256"),
                         "wheel_sha256": packaging.get("wheel_sha256"),
                         "native_module_sha256": installed_policy.get("native_module_sha256")}
        if (installed_policy.get("installed_wheel_match") is not True
                or any(not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value)
                       for value in binary_hashes.values())):
            raise RuntimeError(f"installed native source/wheel/binary SHA evidence is incomplete: {binary_hashes}")
        source_files = packaging.get("required_source_sha256")
        missing_source = [relative for relative in EXECUTION_SOURCE_PATHS
                          if not isinstance(source_files, dict) or relative not in source_files]
        if (not isinstance(source_files, dict)
                or missing_source
                or source_files[EXECUTION_SOURCE_PATHS[0]] != expected_profile["executionManifestFileSha256"]):
            raise RuntimeError(f"installed wheel source evidence omits or changes an execution manifest/catalog dependency: "
                               f"missing={missing_source}, expected_manifest_sha256={expected_profile['executionManifestFileSha256']}, "
                               f"observed_manifest_sha256={source_files.get(EXECUTION_SOURCE_PATHS[0]) if isinstance(source_files, dict) else None}")
        abi = installed_policy.get("native_abi")
        tags = abi.get("wheel_tags") if isinstance(abi, dict) else None
        if (not isinstance(abi, dict) or abi.get("source_manifest_verified") is not True
                or abi.get("installed_binary_match") is not True
                or abi.get("abi") != "abi3" or abi.get("python_minimum") != "3.12"
                or not isinstance(tags, list) or not tags
                or any(not isinstance(tag, str) or not re.fullmatch(r"cp312-abi3-[A-Za-z0-9_.]+", tag)
                       or tag.endswith("-any") for tag in tags)):
            raise RuntimeError(f"installed wheel source ABI/platform evidence is incomplete: expected abi3-py312, observed={abi!r}")
        source = differential.get("source")
        expected_source = {"sha256": expected_profile["sourceMainSha256"],
                           "profile": expected_profile["profileVersion"],
                           "rulesVersion": expected_profile["rulesVersion"],
                           "catalogVersion": expected_profile["catalogVersion"]}
        observed_source = ({name: source.get(name) for name in expected_source}
                           if isinstance(source, dict) else source)
        require_execution_identity(observed_source, expected_source, "native differential")
        draft_boundary = installed_policy.get("game_adapter_draft_boundary")
        required_boundaries = (
            "installed_exports_match", "source_schema_match", "ordered_public_candidates",
            "unknown_field_rejected", "parent_revision_preserved", "parent_public_observation_preserved",
            "branch_revision_changed", "public_observation_valid", "private_transition_event_omitted",
        )
        missing = [name for name in required_boundaries
                   if not isinstance(draft_boundary, dict) or draft_boundary.get(name) is not True]
        if missing:
            raise RuntimeError(f"installed game adapter draft success evidence is incomplete: {missing}")
        adapter_modes = tests.get("game_adapter_draft_modes")
        if not isinstance(adapter_modes, list) or sorted(adapter_modes) != ["chaos", "grand", "normal"]:
            raise RuntimeError("installed game adapter test evidence omits a required draft mode")
    elif scope == "adapter":
        frozen = report_json(job_reports / "frozen-source.json", "v6 frozen source report")
        current = report_json(job_reports / "current-client-source.json", "v7 frozen source report")
        files = current.get("files")
        expected_profile = execution_identity(REPOSITORY)
        require_execution_identity(current.get("executionIdentity"), expected_profile, "frozen source report")
        main = [item for item in files if isinstance(item, dict)
                and isinstance(item.get("name"), str) and item["name"].startswith("main-")] if isinstance(files, list) else []
        if (frozen.get("schemaVersion") != 1 or not isinstance(frozen.get("files"), list) or not frozen["files"]
                or current.get("schemaVersion") != 1 or len(main) != 1
                or main[0].get("sha256") != expected_profile["sourceMainSha256"]
                or not any(isinstance(item, dict) and item.get("name") == "acorn-8.15.0.js"
                           and item.get("sha256") == expected_profile["parserSha256"] for item in files)):
            raise RuntimeError("frozen source reports do not identify both reviewed clients")
        shard_text = os.environ.get("ACCELERATE_ADAPTER_SHARDS", "")
        expected = {"Linux": "0 2 4 6", "Windows": "1 3 5 7"}.get(platform.system())
        if shard_text != expected:
            raise RuntimeError("adapter audit shard assignment differs from the reviewed CI matrix")
        for shard in shard_text.split():
            audit = report_json(reports / "site-adapter"
                                / f"card-surface-site-20260928-shard-{shard}-of-8.json",
                                f"v7 adapter audit shard {shard}")
            expected_audit = {"rulesVersion": expected_profile["rulesVersion"],
                              "catalogVersion": expected_profile["catalogVersion"],
                              "profileVersion": expected_profile["profileVersion"]}
            require_execution_identity({name: audit.get(name) for name in expected_audit},
                                       expected_audit, f"v7 adapter audit shard {shard}")
            if (audit.get("baseline") != "site-20260928"
                    or audit.get("sourceSha256") != main[0]["sha256"]
                    or audit.get("shardIndex") != int(shard) or audit.get("shardCount") != 8
                    or audit.get("styles") != ["normal", "chaos", "grand"]
                    or audit.get("status") != "bounded-probe-complete-with-reachability-gaps"
                    or audit.get("errorCells") != 0 or audit.get("unprobedCells") != 0
                    or not isinstance(audit.get("selectedCards"), int) or audit["selectedCards"] <= 0
                    or audit.get("probedCells") != audit["selectedCards"] * 3):
                raise RuntimeError(f"v7 adapter audit shard {shard} lacks complete successful evidence")
    else:
        raise RuntimeError(f"unknown CI validation scope: {scope}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=("key", "record", "verify"))
    parser.add_argument("scope", choices=tuple(SCOPES))
    args = parser.parse_args()
    expected = identity(args.scope)
    if args.phase == "key":
        output = os.environ.get("GITHUB_OUTPUT")
        if not output:
            raise RuntimeError("GITHUB_OUTPUT is required for CI cache key export")
        with Path(output).open("a", encoding="utf-8") as stream:
            stream.write(f"key={expected['key']}\n")
            stream.write(f"scope={args.scope}\n")
        print(f"{args.scope} validation identity: {expected['inputFingerprint']}")
        return
    marker = marker_path(args.scope)
    if args.phase == "record":
        validate_evidence(args.scope, marker)
        marker.parent.mkdir(parents=True, exist_ok=True)
        marker.write_text(json.dumps(expected, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        print(f"Recorded successful {args.scope} validation for {expected['inputFingerprint']}")
        return
    if not marker.is_file() or marker.is_symlink():
        raise RuntimeError(f"cached {args.scope} validation marker is missing or not a regular file")
    observed = json.loads(marker.read_text(encoding="utf-8"))
    if observed != expected:
        raise RuntimeError(f"cached {args.scope} validation identity differs from this checkout")
    validate_evidence(args.scope, marker)
    print(f"Reused successful {args.scope} validation for {expected['inputFingerprint']}")
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with Path(summary).open("a", encoding="utf-8") as stream:
            stream.write(f"Reused successful {args.scope} checks for the same tracked inputs and runner image.\n")


if __name__ == "__main__":
    try:
        main()
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        for stream in (error.stdout, error.stderr):
            if stream:
                diagnostic = stream.decode("utf-8", errors="replace") if isinstance(stream, bytes) else stream
                print(diagnostic, end="" if diagnostic.endswith("\n") else "\n", file=sys.stderr)
        raise
