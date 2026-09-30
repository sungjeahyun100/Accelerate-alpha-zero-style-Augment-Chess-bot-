"""CI 재활용과 sdist 검증이 함께 사용하는 동결 실행 프로필 식별자.

JCS와 composite catalog 계산은 운영 runtime contract에 위임한다. 원문
catalogHash와 실행 프로필을 포함한 catalogVersion을 서로 대체하지 않는다.
이 조회는 게임·oracle을 실행하거나 네트워크에 접근하지 않는다.
"""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys


CATALOG_ROOT = "projects/augment-chess/contracts"
EXECUTION_SOURCE_PATHS = (
    f"{CATALOG_ROOT}/catalog/execution-profile-20260928.json",
    f"{CATALOG_ROOT}/catalog/site-20260928.json",
    f"{CATALOG_ROOT}/catalog/card-definitions-20260928.json",
    f"{CATALOG_ROOT}/catalog/card-presentation-20260928.json",
    f"{CATALOG_ROOT}/catalog/observation-20260928.json",
    f"{CATALOG_ROOT}/tools/runtime-contract.js",
)
PROFILE_VERSION = "accelerate-headless-semantic-v7-faithful-init-v1"


def require_execution_identity(observed, expected: dict[str, str], label: str) -> None:
    if not isinstance(observed, dict):
        raise RuntimeError(f"{label} execution profile/catalog identity must be an object, got {type(observed).__name__}")
    differences = {key: {"expected": expected.get(key), "observed": observed.get(key)}
                   for key in sorted(set(expected) | set(observed))
                   if key not in expected or key not in observed or observed[key] != expected[key]}
    if differences:
        raise RuntimeError(f"{label} execution profile/catalog identity differs from this checkout: "
                           + json.dumps(differences, sort_keys=True))


def execution_identity(repository: Path) -> dict[str, str]:
    repository = repository.resolve()
    for relative in EXECUTION_SOURCE_PATHS:
        source = repository / relative
        if source.is_symlink() or not source.is_file() or not source.resolve().is_relative_to(repository):
            raise RuntimeError(f"CI execution identity requires an ordinary source file: {relative}")
    script = """
const fs=require('node:fs'),{createHash}=require('node:crypto');
if(process.versions.node.split('.')[0]!=='22')
  throw new Error('CI execution identity requires Node 22, observed '+process.version);
const {createRuntimeContract}=require(process.argv[1]);
const contract=createRuntimeContract({baseline:'site-20260928'});
const profile=contract.executionProfile;
if(!profile||contract.ORACLE_PROFILE_VERSION!==process.argv[3])
  throw new Error('CI execution profile differs: expected '+process.argv[3]+', observed '+contract.ORACLE_PROFILE_VERSION);
const bytes=fs.readFileSync(process.argv[2]);
process.stdout.write(JSON.stringify({
  rulesVersion:contract.catalog.rulesVersion,
  catalogVersion:contract.catalog.catalogVersion,
  catalogSha256:contract.digest(contract.catalog),
  sourcePublicCatalogHash:contract.catalog.sourcePublicCatalogHash,
  profileVersion:contract.ORACLE_PROFILE_VERSION,
  executionProfileSha256:contract.executionProfileSha256,
  executionManifestFileSha256:createHash('sha256').update(bytes).digest('hex'),
  sourceMainSha256:profile.sourceMainSha256,
  parserSha256:profile.parserSha256
}));
"""
    result = subprocess.run(
        ["node", "-e", script, str(repository / f"{CATALOG_ROOT}/tools/runtime-contract.js"),
         str(repository / EXECUTION_SOURCE_PATHS[0]), PROFILE_VERSION],
        cwd=repository, check=True, capture_output=True, text=True,
        encoding="utf-8", timeout=30,
    )
    if result.stderr:
        print(result.stderr, end="" if result.stderr.endswith("\n") else "\n", file=sys.stderr)
    try:
        identity = json.loads(result.stdout)
    except ValueError as error:
        raise RuntimeError(f"CI execution identity is not valid JSON: {error}") from error
    required = {"rulesVersion", "catalogVersion", "catalogSha256", "sourcePublicCatalogHash",
                "profileVersion", "executionProfileSha256", "executionManifestFileSha256",
                "sourceMainSha256", "parserSha256"}
    if (not isinstance(identity, dict) or set(identity) != required
            or any(not isinstance(value, str) or not value for value in identity.values())):
        raise RuntimeError("CI execution identity omits required source, catalog, parser, or profile fields")
    return identity
