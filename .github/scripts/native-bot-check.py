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
import re
import subprocess
import sys
import tarfile
import urllib.request
import xml.etree.ElementTree as ET

from native_validation_profile import (
    EXECUTION_SOURCE_PATHS, RUST_CORE_PACKAGES, RUST_REQUIRED_TESTS,
    execution_identity, require_execution_identity, rust_test_command,
)

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


def validation_scope():
    """두 수동 선택이 겹치거나 잘못된 boolean이면 실행 전에 정확히 실패한다."""
    selections = {}
    for name in ("ACCELERATE_CI_CORE_ONLY", "ACCELERATE_CI_ADAPTER_ONLY"):
        value = os.environ.get(name, "false")
        if value not in {"true", "false"}:
            raise RuntimeError(f"{name} requires true or false, observed {value!r}")
        selections[name] = value == "true"
    if all(selections.values()):
        raise RuntimeError("core_only and adapter_only cannot both be true; select exactly one scope or full validation")
    scope = ("core" if selections["ACCELERATE_CI_CORE_ONLY"] else
             "adapter" if selections["ACCELERATE_CI_ADAPTER_ONLY"] else "rust")
    print(f"Requested validation scope: {scope}", flush=True)
    return scope


def rust_scope(scope="rust"):
    """선택한 실제 cargo 검사를 실행하고 필수 검사가 ok였는지 함께 보존한다."""
    _, _, _, reports, _ = locations()
    reports.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment.pop("PYTHONPATH", None)
    command = rust_test_command(scope)
    print("+", " ".join(command), flush=True)
    executed = subprocess.run(command, cwd=REPOSITORY, env=environment,
                              capture_output=True, text=True, timeout=1200)
    # 종료 코드가 실패여도 원래 stdout/stderr를 먼저 남긴다. ignored는 ok로 세지 않는다.
    output = executed.stdout + executed.stderr
    (reports / f"{scope}-test-output.txt").write_text(output, encoding="utf-8")
    if executed.stdout:
        print(executed.stdout, end="" if executed.stdout.endswith("\n") else "\n", flush=True)
    if executed.stderr:
        print(executed.stderr, end="" if executed.stderr.endswith("\n") else "\n", file=sys.stderr, flush=True)
    executed.check_returncode()
    passed = {match.group(1).rsplit("::", 1)[-1] for match in
              re.finditer(r"^test (\S+) \.\.\. ok$", executed.stdout, re.MULTILINE)}
    not_passed = sorted(set(RUST_REQUIRED_TESTS) - passed)
    if not_passed:
        raise RuntimeError(f"Rust {scope} validation did not execute required tests successfully: {not_passed}")
    result = subprocess.run(
        ["cargo", "test", "-p", "augment-chess-engine", "--locked", "--", "--list"],
        cwd=REPOSITORY, env=environment, capture_output=True, text=True,
        check=True, timeout=180,
    )
    if result.stderr:
        print(result.stderr, end="" if result.stderr.endswith("\n") else "\n", file=sys.stderr)
    required_engine_tests = RUST_REQUIRED_TESTS[:5]
    listed = {line.removesuffix(": test").rsplit("::", 1)[-1]
              for line in result.stdout.splitlines() if line.endswith(": test")}
    missing = [name for name in required_engine_tests if name not in listed]
    if missing:
        raise RuntimeError(f"Rust workspace omitted required geometry or MoveProgram tests: {missing}")
    (reports / f"{scope}-test-list.txt").write_text(result.stdout, encoding="utf-8")
    (reports / f"{scope}-scope.json").write_text(json.dumps({
        "crate": "augment-chess-engine", "required_tests": list(RUST_REQUIRED_TESTS),
        "required_engine_tests": list(required_engine_tests), "validationScope": scope,
        "packages": list(RUST_CORE_PACKAGES) if scope == "core" else ["--workspace"],
        "listed": True, "executed": True, "exitCode": executed.returncode, "command": command,
        "execution": " ".join(command),
        "scope": "core contract tests; external source receipts and full v7 rules parity are separate evidence",
    }, indent=2) + "\n", encoding="utf-8")


def configure():
    scope = validation_scope()
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
        "ACCELERATE_RUST_SCOPE": "core" if scope == "core" else "rust",
    }
    with Path(os.environ["GITHUB_ENV"]).open("a", encoding="utf-8") as file:
        for key, value in values.items():
            if "\n" in value or "\r" in value:
                raise RuntimeError("invalid environment path")
            file.write(f"{key}={value}\n")
    (reports / "environment.json").write_text(json.dumps({
        "python": sys.version, "platform": platform.platform(),
        "artifact_root": str(root), "rust_minimum": "1.96.0",
        "scope": ("Rust core contracts; installed wheel, bot integration and measurements excluded" if scope == "core"
                  else "code and bounded synthetic optimizer/export/inference checks; no learning campaign"),
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
    # Maturin relocates the mixed project's python-source to the sdist root,
    # while workspace crates retain their repository-relative paths.
    required = ["Cargo.toml", "Cargo.lock", "pyproject.toml", "NOTICE.md",
                "packages/adapter-runtime/Cargo.toml",
                "projects/augment-chess/contracts/Cargo.toml", "projects/augment-chess/contracts/src/lib.rs",
                "projects/augment-chess/engine/Cargo.toml",
                "projects/augment-chess/engine/src/lib.rs",
                "projects/accelerate/native/Cargo.toml",
                "projects/accelerate/native/src/lib.rs", "projects/accelerate/native/src/game_adapter.rs",
                "projects/accelerate/runtime/Cargo.toml",
                "projects/accelerate/runtime/src/lib.rs",
                "python/accelerate_chess/adapter_client.py",
                "python/accelerate_chess/ir.py",
                "python/accelerate_chess/network/typed_context.py",
                "python/accelerate_chess/network/mask_resnet.py",
                "python/accelerate_chess/network/entity_transformer.py"]
    required += [f"projects/augment-chess/contracts/catalog/{name}-{date}.json" for date in ("20260927", "20260928")
                 for name in ("site", "observation", "initial-state", "draft", "card-definitions")]
    required += [f"projects/augment-chess/contracts/schemas/adapter-{name}-v1.schema.json"
                 for name in ("actions-request", "actions-response", "observe-request", "observe-response")]
    required = sorted(set(required) | set(EXECUTION_SOURCE_PATHS))
    for relative in required:
        if not (project / relative).is_file():
            raise RuntimeError(f"source distribution missing {relative}")
    return project, required


def build():
    _, _, directory, reports, python = locations()
    run("uv", "lock", "--check")
    run("uv", "sync", "--locked", "--extra", "validation", "--no-install-project",
        cwd=REPOSITORY / "projects/accelerate")
    distribution = directory / "sdist"
    wheels = directory / "wheels" / "sdist-roundtrip"
    run(python, "-m", "maturin", "sdist", "--manifest-path", "native/Cargo.toml",
        "--out", distribution, cwd=REPOSITORY / "projects/accelerate")
    archives = list(distribution.glob("accelerate_chess-*.tar.gz"))
    if len(archives) != 1:
        raise RuntimeError("expected exactly one source distribution")
    project, required = unpack(archives[0], distribution / "extracted")
    source_identity = execution_identity(project)
    require_execution_identity(source_identity, execution_identity(REPOSITORY), "source distribution")
    # Source archives use reproducible mtimes. A reused target can otherwise
    # accept stale local-crate fingerprints after re-extraction; force those
    # owned release crates to rebuild, retaining external dependency artifacts.
    run("cargo", "clean", "--release", "--manifest-path", project / "Cargo.toml",
        "-p", "adapter-runtime", "-p", "augment-chess-contracts", "-p", "augment-chess-engine",
        "-p", "accelerate-native", "-p", "accelerate-runtime",
        cwd=project)
    run(python, "-m", "maturin", "build", "--release", "--locked", "--interpreter", python,
        "--manifest-path", project / "projects/accelerate/native/Cargo.toml", "--out", wheels,
        cwd=project)
    built = list(wheels.glob("accelerate_chess-*.whl"))
    if len(built) != 1:
        raise RuntimeError("expected exactly one platform wheel")
    run("uv", "pip", "install", "--python", python, "--no-deps", "--reinstall", built[0])
    reports.mkdir(parents=True, exist_ok=True)
    run(python, "-c", """
import hashlib, json, re, sys, tomllib, zipfile
from pathlib import Path
import jcs
from accelerate_chess import (Position, ActionStream, InferenceSession, site_observation_policy,
                              GameAdapterSession, GameAdapterClient, NativeError)
from accelerate_chess.encoding import EncoderSpec
from accelerate_chess.ir import TypedEncoderSpec
import accelerate_chess as package
import accelerate_chess.encoding as encoding
import accelerate_chess.ir as typed_ir
import accelerate_chess.adapter_client as adapter_client
import accelerate_chess.network.entity_transformer as entity_transformer
import accelerate_chess.network.mask_resnet as mask_resnet
import accelerate_chess.network.typed_context as typed_context
import accelerate_chess._native as native
checkout, policy_source, wheel, report = map(Path, sys.argv[1:5])
expected_execution_identity = json.loads(sys.argv[5])
native_path = Path(native.__file__).resolve()
module_paths = {'native_module': native_path,
    'python_package': Path(package.__file__).resolve(),
    'encoder_module': Path(encoding.__file__).resolve(),
    'typed_ir_module': Path(typed_ir.__file__).resolve(),
    'adapter_client_module': Path(adapter_client.__file__).resolve(),
    'entity_model_module': Path(entity_transformer.__file__).resolve(),
    'resnet_model_module': Path(mask_resnet.__file__).resolve(),
    'typed_context_module': Path(typed_context.__file__).resolve()}
for path in module_paths.values():
    if path.is_relative_to(checkout.resolve()) or not path.is_relative_to(Path(sys.prefix).resolve()):
        raise RuntimeError('packaging smoke must import native and Python code from the installed wheel')
source_python = policy_source.parents[4] / 'python/accelerate_chess'
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
    wheel_metadata = [name for name in names if name.endswith('.dist-info/WHEEL')]
    if len(wheel_metadata) != 1:
        raise RuntimeError('wheel must contain exactly one ABI/platform metadata record')
    wheel_tags = [line.partition(':')[2].strip()
                  for line in archive.read(wheel_metadata[0]).decode('utf-8').splitlines()
                  if line.startswith('Tag:')]
    if (not wheel_tags or any(not re.fullmatch(r'cp312-abi3-[A-Za-z0-9_.]+', tag)
                              or tag.endswith('-any') for tag in wheel_tags)):
        raise RuntimeError('native wheel ABI/platform tags differ from abi3-py312: '+repr(wheel_tags))
native_manifest = tomllib.loads((policy_source.parents[4] / 'projects/accelerate/native/Cargo.toml').read_text(encoding='utf-8'))
pyo3 = native_manifest.get('dependencies',{}).get('pyo3',{})
abi_features = [feature for feature in pyo3.get('features',[]) if feature.startswith('abi3')]
if abi_features != ['abi3-py312'] or sys.version_info[:2] != (3,12):
    raise RuntimeError('installed native ABI smoke requires source abi3-py312 and Python 3.12: '+repr(abi_features))
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
execution_manifest_path = policy_source.parent / 'execution-profile-20260928.json'
execution_manifest = json.loads(execution_manifest_path.read_text(encoding='utf-8'))
installed_execution_identity = {
    'rulesVersion': latest_catalog['rulesVersion'],
    'catalogVersion': latest_catalog['catalogVersion'],
    'catalogSha256': hashlib.sha256(jcs.canonicalize(latest_catalog)).hexdigest(),
    'sourcePublicCatalogHash': latest_catalog['sourcePublicCatalogHash'],
    'profileVersion': latest_catalog['executionProfile']['version'],
    'executionProfileSha256': hashlib.sha256(jcs.canonicalize(execution_manifest)).hexdigest(),
    'executionManifestFileSha256': hashlib.sha256(execution_manifest_path.read_bytes()).hexdigest(),
    'sourceMainSha256': execution_manifest['sourceMainSha256'],
    'parserSha256': execution_manifest['parserSha256'],
}
identity_differences = {
    key: {'expected': expected_execution_identity.get(key), 'observed': installed_execution_identity.get(key)}
    for key in sorted(set(expected_execution_identity) | set(installed_execution_identity))
    if key not in expected_execution_identity or key not in installed_execution_identity
    or expected_execution_identity[key] != installed_execution_identity[key]
}
profile_digest = installed_execution_identity['executionProfileSha256']
if latest_catalog['executionProfile']['sha256'] != profile_digest:
    identity_differences['compiledExecutionProfileSha256'] = {
        'expected': profile_digest, 'observed': latest_catalog['executionProfile']['sha256'],
    }
if execution_manifest['profileVersion'] != installed_execution_identity['profileVersion']:
    identity_differences['manifestProfileVersion'] = {
        'expected': installed_execution_identity['profileVersion'], 'observed': execution_manifest['profileVersion'],
    }
if identity_differences:
    raise RuntimeError('installed native execution identity differs from the source distribution: '
                       + json.dumps(identity_differences, sort_keys=True))
typed_spec = TypedEncoderSpec.from_catalog(latest_catalog, observation_policy=latest_policy)
if typed_spec.catalog_hash != hashlib.sha256(jcs.canonicalize(latest_catalog)).hexdigest():
    raise RuntimeError('installed typed encoder catalog hash differs from v7 catalog')
if native.GameAdapterSession is not GameAdapterSession or adapter_client.GameAdapterClient is not GameAdapterClient:
    raise RuntimeError('installed public game adapter classes differ from their implementation exports')
adapter_config = {'gameStyle': 'normal', 'draftDelete': False}
adapter_session = GameAdapterSession.new_game(adapter_config, 37, rules_version=latest_version)
descriptors = adapter_session.descriptors()
if ({descriptor['adapterId'] for descriptor in descriptors} != {'public-observation', 'public-actions'}
        or len(descriptors) != 2):
    raise RuntimeError('installed game adapter descriptor set differs from the v7 public contract')
schema_directory = policy_source.parent.parent / 'schemas'
adapter_schemas = {}
for name in ('actions-request', 'actions-response', 'observe-request', 'observe-response'):
    schema = json.loads((schema_directory / f'adapter-{name}-v1.schema.json').read_text(encoding='utf-8'))
    adapter_schemas[schema['$id']] = hashlib.sha256(jcs.canonicalize(schema)).hexdigest()
referenced_schemas = set()
for descriptor in descriptors:
    if descriptor['projectId'] != 'augment-chess' or descriptor['contractVersion'] != {'major': 1, 'minor': 0}:
        raise RuntimeError('installed game adapter descriptor identity or contract version differs')
    for capability in descriptor['capabilities']:
        for field in ('requestSchema', 'responseSchema'):
            reference = capability[field]
            if adapter_schemas.get(reference['id']) != reference['sha256']:
                raise RuntimeError(f'installed game adapter {field} differs from its source distribution schema')
            referenced_schemas.add(reference['id'])
if referenced_schemas != adapter_schemas.keys():
    raise RuntimeError('installed game adapter omitted a required public schema')
adapter = GameAdapterClient(adapter_session, typed_spec)
adapter_before = adapter.observe('white')
adapter_revision = adapter.snapshot_revision
adapter_intents = adapter.legal_intents()
if not adapter_intents or any(intent['type'] not in ('draftPick', 'draftBundlePick') for intent in adapter_intents):
    raise RuntimeError('installed draft game adapter did not return the complete public draft candidate set')
adapter_page = adapter.action_stream().next_page(1)
if (adapter_page['examined'] != 1 or len(adapter_page['actions']) != 1
        or adapter_page['actions'][0].public_intent() != adapter_intents[0]):
    raise RuntimeError('installed draft game adapter stream changed the ordered public candidate')
adapter_action = adapter.bind_public_intent(adapter_intents[0])
if adapter_action.public_intent() != adapter_intents[0] or adapter_action.revision != adapter_revision:
    raise RuntimeError('installed draft game adapter binding changed the public intent or revision')
try:
    adapter.bind_public_intent({**adapter_intents[0], 'clientNote': 'unverified'})
except NativeError:
    pass
else:
    raise RuntimeError('installed draft game adapter admitted an unknown public intent field')
if adapter.snapshot_revision != adapter_revision:
    raise RuntimeError('installed draft game adapter rejection changed its parent revision')
adapter_step = adapter.apply(adapter_action)
adapter_after = adapter_step.position.observe('white')
if (adapter.snapshot_revision != adapter_revision or adapter.observe('white') != adapter_before
        or adapter_step.position.snapshot_revision == adapter_revision or adapter_after == adapter_before
        or hasattr(adapter_step, 'event')):
    raise RuntimeError('installed draft game adapter branch failed public isolation or parent preservation')
if adapter_after['history'][-1]['actor'] != 'white':
    raise RuntimeError('installed draft game adapter omitted its source public history actor')
typed_ir.ObservationIR.from_public(adapter_after, typed_spec)
adapter_smoke = {
    'scope': 'public v7 draft observation, legal intent binding, rejection and branch application',
    'installed_exports_match': True, 'source_schema_match': True,
    'ordered_public_candidates': True, 'unknown_field_rejected': True,
    'parent_revision_preserved': True, 'parent_public_observation_preserved': True,
    'branch_revision_changed': True, 'public_observation_valid': True,
    'private_transition_event_omitted': True,
    'schemas': adapter_schemas,
}
policy.clear()
if jcs.canonicalize(site_observation_policy()) != policy_bytes:
    raise RuntimeError('observation policy must return an independently owned value')
report.write_text(json.dumps({
    **{key: str(path) for key, path in module_paths.items()},
    'python_source_sha256': source_hashes,
    'native_module_sha256': native_hash,
    'native_abi': {'abi':'abi3','python_minimum':'3.12','wheel_tags':wheel_tags,
                   'source_manifest_verified':True,'installed_binary_match':True},
    'execution_identity': installed_execution_identity,
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
    'game_adapter_draft_boundary': adapter_smoke,
}, indent=2) + '\\n', encoding='utf-8')
print('installed native:', native_path)
""", REPOSITORY, project / "projects/augment-chess/contracts/catalog/observation-20260927.json",
        built[0], reports / "installed-policy.json", json.dumps(source_identity, sort_keys=True))
    (reports / "packaging.json").write_text(json.dumps({
        "sdist": archives[0].name, "sdist_sha256": sha(archives[0]),
        "wheel": built[0].name, "wheel_sha256": sha(built[0]),
        "required_source_files": required, "installed": True,
        "execution_identity": source_identity,
        "required_source_sha256": {relative: sha(project / relative) for relative in required},
        "observation_policy": json.loads((reports / "installed-policy.json").read_text(encoding="utf-8")),
    }, indent=2) + "\n", encoding="utf-8")


def tests():
    _, _, _, reports, python = locations()
    reports.mkdir(parents=True, exist_ok=True)
    report = reports / "pytest.xml"
    bf16_probe = subprocess.run(
        [str(python), "-c", "import torch; print(int(torch.cuda.is_available() and torch.cuda.is_bf16_supported()))"],
        cwd=REPOSITORY, capture_output=True, text=True, check=True, timeout=30,
    )
    if bf16_probe.stdout.strip() not in {"0", "1"}:
        raise RuntimeError(f"unexpected CUDA BF16 capability result: {bf16_probe.stdout!r}")
    cuda_bf16_available = bf16_probe.stdout.strip() == "1"
    optional_bf16_case = "projects/accelerate/python/tests/test_session.py::test_cuda_bf16_training_keeps_fp32_master_and_adamw_state"
    optional_selection = [] if cuda_bf16_available else [f"--deselect={optional_bf16_case}"]
    if not cuda_bf16_available:
        print(f"optional CUDA BF16 hardware unsupported; deselecting {optional_bf16_case}", flush=True)
    run(python, "-m", "pytest", "projects/accelerate/python/tests/test_native.py", "projects/accelerate/python/tests/test_model_stack.py",
        "projects/accelerate/python/tests/test_ir.py", "projects/accelerate/python/tests/test_entity_transformer.py",
        "projects/accelerate/python/tests/test_inference_runtime.py", "projects/accelerate/python/tests/test_search.py", "projects/accelerate/python/tests/test_session.py",
        "projects/accelerate/python/tests/test_adapter_client.py", "projects/accelerate/python/tests/test_architecture_v1.py",
        "-p", "no:cacheprovider",
        "--junitxml", report, "-ra", *optional_selection, timeout=1800)
    suites = ET.parse(report).getroot().findall("testsuite")
    if not suites or any(int(suite.get("skipped", "0")) for suite in suites):
        raise RuntimeError("native bot CI requires real tests with no skips")
    cases = [case for suite in suites for case in suite.findall("testcase")]
    if cuda_bf16_available and not any(case.get("name") == optional_bf16_case.rsplit("::", 1)[1] for case in cases):
        raise RuntimeError("CUDA BF16 hardware is available but its optional training check was not executed")
    for module, minimum in (("test_native", 6), ("test_model_stack", 7), ("test_ir", 7),
                            ("test_entity_transformer", 7), ("test_inference_runtime", 6),
                            ("test_search", 11), ("test_session", 4),
                            ("test_adapter_client", 5), ("test_architecture_v1", 1)):
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
    adapter_modes = ("normal", "chaos", "grand")
    for mode in adapter_modes:
        required = f"test_v7_draft_public_intents_are_exact_and_branches_are_isolated[{mode}]"
        if not any(case.get("name") == required for case in cases):
            raise RuntimeError(f"missing installed game adapter {mode} public draft branch check")
    (reports / "test-scope.json").write_text(json.dumps({
        "implementation": "installed wheel; native/typed IR/two model families/ort/tract/search/replay/CLI",
        "skips": 0, "default_weighted_conditioning_modes": list(default_checks),
        "optional_cuda_bf16": "executed" if cuda_bf16_available else "unsupported",
        "deselected_optional_tests": [] if cuda_bf16_available else [optional_bf16_case],
        "typed_contract_checks": list(typed_checks),
        "game_adapter_draft_modes": list(adapter_modes),
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
    catalog = json.loads((REPOSITORY / "projects/augment-chess/contracts/catalog/site-20260927.json").read_text(encoding="utf-8"))
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
    run("node", "--test", "projects/augment-chess/contracts/tools/runtime-contract.test.js",
        "projects/augment-chess/oracle/tools/site-parity/offline-oracle.test.js", timeout=180)


def current_client():
    """Verify the reviewed v7 client in an isolated slot; reuse the v6 parser."""
    root, _, _, reports, _ = locations()
    reports.mkdir(parents=True, exist_ok=True)
    catalog = json.loads((REPOSITORY / "projects/augment-chess/contracts/catalog/site-20260928.json").read_text(encoding="utf-8"))
    profile_identity = execution_identity(REPOSITORY)
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
    os.environ["ACCELERATE_SITE_BASELINE_LATEST"] = str(destination)
    run("node", "projects/augment-chess/oracle/tools/site-parity/prepare-current-baseline.js", "--verify", destination, timeout=180)
    run("node", "--test", "projects/augment-chess/contracts/tools/runtime-contract.test.js",
        "projects/augment-chess/tests/site-adapter/parity/latest-client.test.cjs",
        "projects/augment-chess/tests/differential/v7-native-response.test.cjs", timeout=180)
    # Cache에 보존할 검증 보고서와 원문 loader의 baseline manifest를 구별한다.
    # 성공한 current-client 단계만 실제 composite 실행 식별자를 보고서에 남긴다.
    (reports / "current-client-source.json").write_text(json.dumps({
        **baseline, "executionIdentity": profile_identity,
    }, indent=2) + "\n", encoding="utf-8")


def v7_differential():
    """Compare the installed wheel with the same pinned client on this OS."""
    root, _, _, _, python = locations()
    try:
        current_client()
    except Exception as error:
        # Keep an explicit NO-GO report even when source preparation stops
        # before the differential runner can create its own detailed report.
        destination = root / "reports" / "v7-native-differential"
        destination.mkdir(parents=True, exist_ok=True)
        (destination / "report.json").write_text(json.dumps({
            "gate": "source-pinned-v7-native-differential-probe",
            "status": "setup-error", "decision": "NO-GO",
            "phase": "pinned-source-preparation",
            "errorType": type(error).__name__,
            "reason": str(error),
        }, indent=2) + "\n", encoding="utf-8")
        raise
    run("node", "projects/augment-chess/tests/differential/v7-native-differential.cjs", "--python", python)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=("configure", "validate-scope", "rust-scope", "build", "tests", "frozen", "current-client",
                                          "v7-differential"))
    parser.add_argument("--scope", choices=("rust", "core"), default="rust")
    arguments = parser.parse_args()
    phase = arguments.phase
    try:
        {"configure": configure, "validate-scope": validation_scope,
         "rust-scope": lambda: rust_scope(arguments.scope), "build": build, "tests": tests, "frozen": frozen,
         "current-client": current_client, "v7-differential": v7_differential}[phase]()
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        for stream in (error.stdout, error.stderr):
            if stream:
                diagnostic = stream.decode("utf-8", errors="replace") if isinstance(stream, bytes) else stream
                print(diagnostic, end="" if diagnostic.endswith("\n") else "\n", file=sys.stderr)
        raise
