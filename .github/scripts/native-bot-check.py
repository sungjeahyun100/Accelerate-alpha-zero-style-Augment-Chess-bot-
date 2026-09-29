"""Cross-platform CI packaging and frozen-source checks; outputs stay external.

Run each phase from the repository root. No command starts a training or game
campaign. The installed wheel, rather than an editable source package, supplies
the native and Python implementation used by pytest.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import subprocess
import sys
import tarfile
import urllib.request
import xml.etree.ElementTree as ET

REPOSITORY = Path(__file__).resolve().parents[2]


def locations():
    if not os.environ.get("RUNNER_TEMP"):
        raise RuntimeError("RUNNER_TEMP is required for these CI-only fixed slots")
    root = Path(os.environ["RUNNER_TEMP"]).resolve() / "Accelerate"
    if root.is_relative_to(REPOSITORY):
        raise RuntimeError("CI outputs must be outside the checkout")
    system = platform.system()
    build = root / "build" / system / "native-bot"
    reports = root / "reports" / "native-bot" / system
    environment = Path(os.environ.get("UV_PROJECT_ENVIRONMENT", str(build / "venv"))).resolve()
    if environment.is_relative_to(REPOSITORY):
        raise RuntimeError("the validation environment must be outside the checkout")
    python = environment / ("Scripts/python.exe" if system == "Windows" else "bin/python")
    return root, system, build, reports, python


def run(*arguments, cwd=REPOSITORY, timeout=1200):
    display = ("<inline program>" if "\n" in str(argument) else str(argument)
               for argument in arguments)
    print("+", " ".join(display), flush=True)
    environment = os.environ.copy()
    environment.pop("PYTHONPATH", None)
    subprocess.run(list(map(str, arguments)), cwd=cwd, env=environment,
                   check=True, timeout=timeout)


def sha(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def rust_scope():
    """Confirm the workspace run still contains the new core contract tests."""
    _, _, _, reports, _ = locations()
    environment = os.environ.copy()
    environment.pop("PYTHONPATH", None)
    result = subprocess.run(
        ["cargo", "test", "-p", "accelerate-engine", "--locked", "--", "--list"],
        cwd=REPOSITORY, env=environment, capture_output=True, text=True,
        check=True, timeout=180,
    )
    required = (
        "geometry_maps_rectangles_and_signed_extents_without_relabeling_coordinates",
        "synthetic_resize_requires_new_cells_and_an_explicit_clipping_policy",
        "child_search_uses_parent_square_but_preserves_original_origin_and_board",
        "shift_checks_third_party_collision_mutual_overlap_and_stops_on_first_piece",
        "explicit_stack_matches_independent_recursive_oracle_for_singletons",
    )
    listed = {line.removesuffix(": test").rsplit("::", 1)[-1]
              for line in result.stdout.splitlines() if line.endswith(": test")}
    missing = [name for name in required if name not in listed]
    if missing:
        raise RuntimeError(f"Rust workspace omitted required geometry or MoveProgram tests: {missing}")
    reports.mkdir(parents=True, exist_ok=True)
    (reports / "rust-test-list.txt").write_text(result.stdout, encoding="utf-8")
    (reports / "rust-scope.json").write_text(json.dumps({
        "crate": "accelerate-engine", "required_tests": list(required),
        "listed": True, "execution": "cargo test --workspace --locked in the preceding CI step",
        "scope": "geometry and MoveProgram code contracts; full v7 rules parity remains pending",
    }, indent=2) + "\n", encoding="utf-8")


def configure():
    root, system, build, reports, _ = locations()
    reports.mkdir(parents=True, exist_ok=True)
    values = {
        "CARGO_TARGET_DIR": str(build / "cargo"),
        "CARGO_HOME": str(root / "cache" / system / "cargo"),
        "CARGO_BUILD_JOBS": "4",
        "UV_CACHE_DIR": str(root / "cache" / system / "uv"),
        "UV_PROJECT_ENVIRONMENT": str(build / "venv"),
        "UV_PYTHON_DOWNLOADS": "never",
        "UV_LINK_MODE": "copy",
        "PYTHONDONTWRITEBYTECODE": "1",
        "PYTHONUTF8": "1",
        "ACCELERATE_TEST_ARTIFACTS": str(root / "models" / system / "native-bot-validation"),
        "ACCELERATE_SITE_BASELINE": str(root / "cache" / "site-baseline-client"),
    }
    with Path(os.environ["GITHUB_ENV"]).open("a", encoding="utf-8") as file:
        for key, value in values.items():
            if "\n" in value or "\r" in value:
                raise RuntimeError("invalid environment path")
            file.write(f"{key}={value}\n")
    (reports / "environment.json").write_text(json.dumps({
        "python": sys.version, "platform": platform.platform(),
        "artifact_root": str(root), "rust_minimum": "1.96.0",
        "scope": "code and bounded synthetic optimizer/export/inference checks; no learning campaign",
    }, indent=2) + "\n", encoding="utf-8")


def unpack(archive, destination):
    """Validate the actual source distribution before bounded data extraction."""
    destination = destination.resolve()
    if destination.exists() and any(destination.iterdir()):
        raise RuntimeError("source extraction requires an empty owned fixed slot; stale files may not join a new sdist")
    destination.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive, "r:gz") as source:
        members = source.getmembers()
        if len(members) > 20_000 or sum(item.size for item in members) > 100 * 1024 * 1024:
            raise RuntimeError("source distribution exceeds the source-size budget")
        roots = set()
        for item in members:
            name = PurePosixPath(item.name)
            if name.is_absolute() or ".." in name.parts or "\\" in item.name:
                raise RuntimeError("unsafe source archive path")
            if not (item.isfile() or item.isdir()) or not name.parts:
                raise RuntimeError("source archive must contain ordinary files and directories")
            if any(part in {"target", "__pycache__", ".venv", "node_modules", "models", "checkpoints"}
                   for part in name.parts) or name.suffix in {".onnx", ".pt", ".safetensors", ".so", ".pyd", ".pyc", ".whl"}:
                raise RuntimeError("generated artifact found in source distribution")
            if not (destination / item.name).resolve().is_relative_to(destination):
                raise RuntimeError("source archive escapes its extraction slot")
            roots.add(name.parts[0])
        if len(roots) != 1:
            raise RuntimeError("source distribution must have one project root")
        source.extractall(destination, members=members, filter="data")
    project = destination / roots.pop()
    required = ["Cargo.toml", "Cargo.lock", "pyproject.toml", "NOTICE.md", "rust-engine/src/lib.rs",
                "bridge/native/src/lib.rs", "bridge/runtime/src/lib.rs",
                "python/accelerate_chess/ir.py",
                "python/accelerate_chess/network/typed_context.py",
                "python/accelerate_chess/network/mask_resnet.py",
                "python/accelerate_chess/network/entity_transformer.py"]
    required += [f"bridge/catalog/{name}-{date}.json" for date in ("20260927", "20260928")
                 for name in ("site", "observation", "initial-state", "draft", "card-definitions")]
    for relative in required:
        if not (project / relative).is_file():
            raise RuntimeError(f"source distribution missing {relative}")
    return project, required


def build():
    _, _, directory, reports, python = locations()
    run("uv", "lock", "--check")
    run("uv", "sync", "--locked", "--extra", "validation", "--no-install-project")
    distribution = directory / "sdist"
    wheels = directory / "wheels" / "sdist-roundtrip"
    run(python, "-m", "maturin", "sdist", "--manifest-path", "bridge/native/Cargo.toml",
        "--out", distribution)
    archives = list(distribution.glob("accelerate_chess-*.tar.gz"))
    if len(archives) != 1:
        raise RuntimeError("expected exactly one source distribution")
    project, required = unpack(archives[0], distribution / "extracted")
    # Source archives use reproducible mtimes. A reused target can otherwise
    # accept stale local-crate fingerprints after re-extraction; force those
    # three owned release crates to rebuild, retaining dependency artifacts.
    run("cargo", "clean", "--release", "--manifest-path", project / "Cargo.toml",
        "-p", "accelerate-engine", "-p", "accelerate-native", "-p", "accelerate-runtime",
        cwd=project)
    run(python, "-m", "maturin", "build", "--release", "--locked", "--interpreter", python,
        "--manifest-path", project / "bridge/native/Cargo.toml", "--out", wheels,
        cwd=project)
    built = list(wheels.glob("accelerate_chess-*.whl"))
    if len(built) != 1:
        raise RuntimeError("expected exactly one platform wheel")
    run("uv", "pip", "install", "--python", python, "--no-deps", "--reinstall", built[0])
    reports.mkdir(parents=True, exist_ok=True)
    run(python, "-c", """
import hashlib, json, sys, zipfile
from pathlib import Path
import jcs
from accelerate_chess import Position, ActionStream, InferenceSession, site_observation_policy
from accelerate_chess.encoding import EncoderSpec
from accelerate_chess.ir import TypedEncoderSpec
import accelerate_chess as package
import accelerate_chess.encoding as encoding
import accelerate_chess.ir as typed_ir
import accelerate_chess.network.entity_transformer as entity_transformer
import accelerate_chess.network.mask_resnet as mask_resnet
import accelerate_chess.network.typed_context as typed_context
import accelerate_chess._native as native
checkout, policy_source, wheel, report = map(Path, sys.argv[1:])
native_path = Path(native.__file__).resolve()
module_paths = {'native_module': native_path,
    'python_package': Path(package.__file__).resolve(),
    'encoder_module': Path(encoding.__file__).resolve(),
    'typed_ir_module': Path(typed_ir.__file__).resolve(),
    'entity_model_module': Path(entity_transformer.__file__).resolve(),
    'resnet_model_module': Path(mask_resnet.__file__).resolve(),
    'typed_context_module': Path(typed_context.__file__).resolve()}
for path in module_paths.values():
    if path.is_relative_to(checkout.resolve()) or not path.is_relative_to(Path(sys.prefix).resolve()):
        raise RuntimeError('packaging smoke must import native and Python code from the installed wheel')
source_python = policy_source.parents[2] / 'python/accelerate_chess'
source_modules = {path.relative_to(source_python).as_posix(): path
                  for path in source_python.rglob('*.py')}
source_hashes = {}
installed_package = module_paths['python_package'].parent
with zipfile.ZipFile(wheel) as archive:
    names = archive.namelist()
    if len(names) != len(set(names)):
        raise RuntimeError('wheel contains duplicate paths')
    packaged_modules = {name.removeprefix('accelerate_chess/'): name for name in names
                        if name.startswith('accelerate_chess/') and name.endswith('.py')}
    if source_modules.keys() != packaged_modules.keys():
        raise RuntimeError('wheel Python modules differ from the source distribution')
    for name, source in source_modules.items():
        installed = (installed_package / name).resolve()
        if not installed.is_relative_to(installed_package):
            raise RuntimeError('installed Python module escapes the package')
        source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        if (source_hash != hashlib.sha256(archive.read(packaged_modules[name])).hexdigest()
                or source_hash != hashlib.sha256(installed.read_bytes()).hexdigest()):
            raise RuntimeError('wheel or installed Python code differs from the source distribution')
        source_hashes[name] = source_hash
    native_members = [name for name in names if name.startswith('accelerate_chess/_native.')
                      and name.endswith(('.so', '.pyd'))]
    if len(native_members) != 1 or Path(native_members[0]).name != native_path.name:
        raise RuntimeError('wheel must supply exactly the imported native extension')
    native_hash = hashlib.sha256(native_path.read_bytes()).hexdigest()
    if native_hash != hashlib.sha256(archive.read(native_members[0])).hexdigest():
        raise RuntimeError('installed native extension differs from the wheel')
catalog = native.site_catalog()
source_catalog = json.loads((policy_source.parent / 'site-20260927.json').read_text(encoding='utf-8'))
catalog_bytes = jcs.canonicalize(catalog)
if catalog_bytes != jcs.canonicalize(source_catalog):
    raise RuntimeError('installed site catalog differs from the source distribution')
policy = site_observation_policy()
source_policy = json.loads(policy_source.read_text(encoding='utf-8'))
policy_bytes = jcs.canonicalize(policy)
if policy_bytes != jcs.canonicalize(source_policy):
    raise RuntimeError('installed observation policy differs from source distribution')
if policy != native.site_observation_policy():
    raise RuntimeError('public package and native observation policy differ')
policy_hash = hashlib.sha256(policy_bytes).hexdigest()
spec = EncoderSpec.from_catalog(catalog, observation_policy=policy)
if spec.observation_policy_hash != policy_hash or spec.contract()['observation_policy'] != policy:
    raise RuntimeError('installed encoder does not retain the compiled observation policy')
restored = EncoderSpec.from_dict(spec.to_dict(), observation_policy=source_policy)
if restored.contract() != spec.contract():
    raise RuntimeError('installed encoder policy contract does not roundtrip')
latest_catalog_source = json.loads((policy_source.parent / 'site-20260928.json').read_text(encoding='utf-8'))
latest_policy_source = json.loads((policy_source.parent / 'observation-20260928.json').read_text(encoding='utf-8'))
latest_version = latest_catalog_source['rulesVersion']
latest_catalog = native.site_catalog(latest_version)
latest_policy = native.site_observation_policy(latest_version)
if (jcs.canonicalize(latest_catalog) != jcs.canonicalize(latest_catalog_source)
        or jcs.canonicalize(latest_policy) != jcs.canonicalize(latest_policy_source)):
    raise RuntimeError('installed v7 catalog or observation policy differs from the source distribution')
typed_spec = TypedEncoderSpec.from_catalog(latest_catalog, observation_policy=latest_policy)
if typed_spec.catalog_hash != hashlib.sha256(jcs.canonicalize(latest_catalog)).hexdigest():
    raise RuntimeError('installed typed encoder catalog hash differs from v7 catalog')
policy.clear()
if jcs.canonicalize(site_observation_policy()) != policy_bytes:
    raise RuntimeError('observation policy must return an independently owned value')
report.write_text(json.dumps({
    **{key: str(path) for key, path in module_paths.items()},
    'python_source_sha256': source_hashes,
    'native_module_sha256': native_hash,
    'installed_wheel_match': True,
    'site_catalog_sha256': hashlib.sha256(catalog_bytes).hexdigest(),
    'observation_policy_sha256': policy_hash,
    'observation_protocol': source_policy['protocolVersion'],
    'projection_version': source_policy['projectionVersion'],
    'schema_version': source_policy['schemaVersion'],
    'source_distribution_match': True,
    'owned_copy': True,
    'encoder_spec_hash': spec.digest,
    'encoder_contract_roundtrip': True,
    'v7_catalog_sha256': hashlib.sha256(jcs.canonicalize(latest_catalog)).hexdigest(),
    'v7_observation_policy_sha256': hashlib.sha256(jcs.canonicalize(latest_policy)).hexdigest(),
    'typed_encoder_hash': typed_spec.digest,
}, indent=2) + '\\n', encoding='utf-8')
print('installed native:', native_path)
""", REPOSITORY, project / "bridge/catalog/observation-20260927.json",
        built[0], reports / "installed-policy.json")
    (reports / "packaging.json").write_text(json.dumps({
        "sdist": archives[0].name, "sdist_sha256": sha(archives[0]),
        "wheel": built[0].name, "wheel_sha256": sha(built[0]),
        "required_source_files": required, "installed": True,
        "required_source_sha256": {relative: sha(project / relative) for relative in required},
        "observation_policy": json.loads((reports / "installed-policy.json").read_text(encoding="utf-8")),
    }, indent=2) + "\n", encoding="utf-8")


def tests():
    _, _, _, reports, python = locations()
    reports.mkdir(parents=True, exist_ok=True)
    report = reports / "pytest.xml"
    run(python, "-m", "pytest", "python/tests/test_native.py", "python/tests/test_model_stack.py",
        "python/tests/test_ir.py", "python/tests/test_entity_transformer.py",
        "python/tests/test_inference_runtime.py", "python/tests/test_search.py", "python/tests/test_session.py",
        "-p", "no:cacheprovider",
        "--junitxml", report, "-ra")
    suites = ET.parse(report).getroot().findall("testsuite")
    if not suites or any(int(suite.get("skipped", "0")) for suite in suites):
        raise RuntimeError("native bot CI requires real tests with no skips")
    cases = [case for suite in suites for case in suite.findall("testcase")]
    for module, minimum in (("test_native", 6), ("test_model_stack", 7), ("test_ir", 7),
                            ("test_entity_transformer", 7), ("test_inference_runtime", 6),
                            ("test_search", 11), ("test_session", 4)):
        if sum(case.get("classname", "").endswith(module) for case in cases) < minimum:
            raise RuntimeError(f"missing required checks for {module}")
    default_checks = {
        "normal": "test_native_default_weighted_conditioning_completion_gate[normal]",
        "chaos": "test_native_default_weighted_conditioning_completion_gate[chaos]",
        "grand": "test_native_supported_conditioned_modes_and_public_intent_integration[grand-False]",
    }
    for mode, required in default_checks.items():
        if not any(case.get("name") == required for case in cases):
            raise RuntimeError(f"missing actual default {mode} conditioning completion check")
    typed_checks = {
        "source_bound_ir": "test_source_bound_spec_and_public_v2_geometry_cells",
        "variable_geometry": "test_synthetic_geometry_padding_and_candidate_split",
        "candidate_independence": "test_candidate_permutation_split_and_padding_preserve_state_value",
        "mask_resnet": "test_mask_resnet_padding_and_candidate_partition_invariance",
        "typed_manifest": "test_typed_v3_transformer_export_and_manifest_contract",
        "entity_ort_tract": "test_typed_v3_native_backend_parity_and_input_boundary[entity-transformer]",
        "resnet_ort_tract": "test_typed_v3_native_backend_parity_and_input_boundary[mask-resnet]",
    }
    for boundary, required in typed_checks.items():
        if not any(case.get("name") == required for case in cases):
            raise RuntimeError(f"missing installed typed {boundary} check")
    (reports / "test-scope.json").write_text(json.dumps({
        "implementation": "installed wheel; native/typed IR/two model families/ort/tract/search/replay/CLI",
        "skips": 0, "default_weighted_conditioning_modes": list(default_checks),
        "typed_contract_checks": list(typed_checks),
        "scope": "code and bounded synthetic checks; full rules/catalog coverage is a separate pending gate",
        "actual_learning_campaign": False,
    }, indent=2) + "\n", encoding="utf-8")
    artifact = Path(os.environ["ACCELERATE_TEST_ARTIFACTS"]) / "native-runtime"
    for parity in artifact.glob("*/parity.json"):
        data = json.loads(parity.read_text(encoding="utf-8"))
        (reports / f"parity-{parity.parent.name}.json").write_text(
            json.dumps(data, indent=2) + "\n", encoding="utf-8")
        cases = data["cases"]
        print(f"parity report: {parity.parent.name}", json.dumps({
            "encoder_hash": data["encoder_hash"], "architecture": data["architecture"],
            "cases": len(cases), "max_abs_error": max(max(case["max_abs_error"]) for case in cases),
            "atol": data["atol"], "rtol": data["rtol"],
        }), flush=True)


def frozen():
    root, _, _, reports, _ = locations()
    reports.mkdir(parents=True, exist_ok=True)
    catalog = json.loads((REPOSITORY / "bridge/catalog/site-20260927.json").read_text(encoding="utf-8"))
    metadata = catalog["source"]
    # OfflineOracle executes loadMain only. Index and worker keep their original
    # provenance; their mutable URLs cannot select a new client or worker.
    for name, label in (("index.html", "index"), ("aiWorker.raw.js", "worker")):
        adopted = next(file for file in metadata["files"] if file["name"] == name)
        provenance = {"adopted": adopted, "executed": False,
                      "reason": "not a dependency of the frozen client oracle"}
        try:
            request = urllib.request.Request(adopted["url"], headers={"User-Agent": "Accelerate-frozen-source-CI"})
            with urllib.request.urlopen(request, timeout=30) as response:
                current = response.read(16 * 1024 * 1024 + 1)
            if len(current) > 16 * 1024 * 1024:
                raise RuntimeError("current provenance source exceeds the download budget")
            observed = hashlib.sha256(current).hexdigest()
            provenance.update({"current_sha256": observed, "current_bytes": len(current),
                               "drift": observed != adopted["sha256"]})
        except (OSError, RuntimeError) as error:
            provenance["current_fetch_error"] = str(error)
        (reports / f"{label}-provenance.json").write_text(
            json.dumps(provenance, indent=2) + "\n", encoding="utf-8")
    files = [file for file in metadata["files"] if file["name"].startswith("main-")]
    if len(files) != 1:
        raise RuntimeError("the adopted catalog must identify exactly one frozen client")
    files.append({"name": "acorn-8.15.0.js", "url": "https://unpkg.com/acorn@8.15.0/dist/acorn.js",
                  "sha256": "fdb08546776ec6228b03e8d02b40d4ab3255bae5f401adba7ff5dad927ac5c9c",
                  "bytes": 241575})
    # A separate fixed slot preserves existing full baseline/worker snapshots.
    destination = root / "cache" / "site-baseline-client"
    destination.mkdir(parents=True, exist_ok=True)
    verified = []
    # Download only adopted URLs. Never discover or freeze today's mutable main.
    for file in files:
        request = urllib.request.Request(file["url"], headers={"User-Agent": "Accelerate-frozen-source-CI"})
        with urllib.request.urlopen(request, timeout=30) as response:
            data = response.read(16 * 1024 * 1024 + 1)
        if len(data) > 16 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != file["sha256"]:
            raise RuntimeError(f"adopted frozen source changed or unavailable: {file['name']}; "
                               "update the reviewed rules contract before changing the baseline")
        if "bytes" in file and len(data) != file["bytes"]:
            raise RuntimeError(f"frozen source byte count differs: {file['name']}")
        path = destination / file["name"]
        if path.exists() and path.read_bytes() != data:
            raise RuntimeError(f"refusing to replace existing frozen source: {file['name']}")
        path.write_bytes(data)
        verified.append({**file, "bytes": len(data)})
    baseline = {"schemaVersion": 1, "frozenAt": metadata["frozenAt"],
                "site": "https://augmentchess.org", "executionScope": "frozen-client", "files": verified}
    serialized = json.dumps(baseline, indent=2) + "\n"
    manifest = destination / "baseline.json"
    if manifest.exists() and manifest.read_text(encoding="utf-8") != serialized:
        raise RuntimeError("refusing to replace an existing adopted client manifest")
    manifest.write_text(serialized, encoding="utf-8")
    (reports / "frozen-source.json").write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8")
    os.environ["ACCELERATE_SITE_BASELINE"] = str(destination)
    run("node", "--test", "bridge/tools/runtime-contract.test.js",
        "infra/tools/site-parity/offline-oracle.test.js", timeout=180)


def current_client():
    """Verify the reviewed v7 client in an isolated slot; reuse the v6 parser."""
    root, _, _, reports, _ = locations()
    reports.mkdir(parents=True, exist_ok=True)
    catalog = json.loads((REPOSITORY / "bridge/catalog/site-20260928.json").read_text(encoding="utf-8"))
    metadata = catalog["source"]
    files = metadata["files"]
    mains = [file for file in files if file["name"].startswith("main-")]
    if len(mains) != 1 or mains[0] != {
        "name": "main-OahWs0tU.js",
        "url": "https://augmentchess.org/assets/main-OahWs0tU.js",
        "sha256": "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c",
        "bytes": 12892256,
    }:
        raise RuntimeError("reviewed v7 client identity changed; review the rules baseline")
    parsers = [file for file in files if file["name"] == "acorn-8.15.0.js"]
    if len(parsers) != 1 or parsers[0] != {
        "name": "acorn-8.15.0.js",
        "url": "https://unpkg.com/acorn@8.15.0/dist/acorn.js",
        "sha256": "fdb08546776ec6228b03e8d02b40d4ab3255bae5f401adba7ff5dad927ac5c9c",
        "bytes": 241575,
    }:
        raise RuntimeError("reviewed v7 parser identity changed")
    if metadata["schemaVersion"] != 1 or metadata["site"] != "https://augmentchess.org":
        raise RuntimeError("reviewed v7 source manifest is invalid")

    destination = root / "cache" / "site-baseline-20260928-e5ed84fc"
    destination.mkdir(parents=True, exist_ok=True)
    if not destination.resolve().is_relative_to(root.resolve()) or destination.resolve().is_relative_to(REPOSITORY):
        raise RuntimeError("v7 client cache slot escapes the CI artifact root")
    # The old frozen phase runs first in CI. Reuse only its verified parser
    # bytes; a standalone invocation can fetch the same pinned parser.
    old_parser = root / "cache" / "site-baseline-client" / "acorn-8.15.0.js"
    if old_parser.is_file() and not old_parser.is_symlink():
        if not old_parser.resolve().is_relative_to(root.resolve()):
            raise RuntimeError("reused parser escapes the CI artifact root")
        parser_data = old_parser.read_bytes()
    else:
        parser_data = None

    def download_exact(file):
        request = urllib.request.Request(file["url"], headers={"User-Agent": "Accelerate-frozen-source-CI"})
        with urllib.request.urlopen(request, timeout=30) as response:
            data = response.read(16 * 1024 * 1024 + 1)
        if len(data) != file["bytes"] or hashlib.sha256(data).hexdigest() != file["sha256"]:
            raise RuntimeError(f"adopted v7 source changed or unavailable: {file['name']}")
        return data

    if parser_data is None:
        parser_data = download_exact(parsers[0])
    elif len(parser_data) != parsers[0]["bytes"] or hashlib.sha256(parser_data).hexdigest() != parsers[0]["sha256"]:
        raise RuntimeError("reused parser does not match the reviewed pin")
    main_data = download_exact(mains[0])
    for file, data in ((mains[0], main_data), (parsers[0], parser_data)):
        target = destination / file["name"]
        if target.is_symlink() or (target.exists() and (not target.is_file() or target.read_bytes() != data)):
            raise RuntimeError(f"refusing to replace existing v7 frozen source: {file['name']}")
        if not target.exists():
            target.write_bytes(data)
    baseline = {
        "schemaVersion": 1, "frozenAt": metadata["frozenAt"],
        "site": metadata["site"], "executionScope": "frozen-client",
        "files": [mains[0], parsers[0]],
    }
    serialized = json.dumps(baseline, indent=2) + "\n"
    manifest = destination / "baseline.json"
    if manifest.is_symlink() or (manifest.exists() and manifest.read_text(encoding="utf-8") != serialized):
        raise RuntimeError("refusing to replace an existing v7 client manifest")
    if not manifest.exists():
        manifest.write_text(serialized, encoding="utf-8")
    (reports / "current-client-source.json").write_text(serialized, encoding="utf-8")
    os.environ["ACCELERATE_SITE_BASELINE_LATEST"] = str(destination)
    run("node", "infra/tools/site-parity/prepare-current-baseline.js", "--verify", destination, timeout=180)
    run("node", "--test", "bridge/tools/runtime-contract.test.js",
        "tests/site-adapter/parity/latest-client.test.cjs", timeout=180)


def v7_differential():
    """Compare the installed wheel with the same pinned client on this OS."""
    root, _, _, _, python = locations()
    try:
        current_client()
    except Exception:
        # Keep an explicit NO-GO report even when source preparation stops
        # before the differential runner can create its own detailed report.
        destination = root / "reports" / "v7-native-differential"
        destination.mkdir(parents=True, exist_ok=True)
        (destination / "report.json").write_text(json.dumps({
            "gate": "source-pinned-v7-native-differential-probe",
            "status": "setup-error", "decision": "NO-GO",
            "reason": "pinned v7 source preparation failed; inspect the CI step log",
        }, indent=2) + "\n", encoding="utf-8")
        raise
    run("node", "tests/differential/v7-native-differential.cjs", "--python", python)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=("configure", "rust-scope", "build", "tests", "frozen", "current-client",
                                          "v7-differential"))
    phase = parser.parse_args().phase
    {"configure": configure, "rust-scope": rust_scope, "build": build, "tests": tests, "frozen": frozen,
     "current-client": current_client, "v7-differential": v7_differential}[phase]()
