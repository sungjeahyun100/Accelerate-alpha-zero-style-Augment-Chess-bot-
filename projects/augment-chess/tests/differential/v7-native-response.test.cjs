"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { createRuntimeContract } = require("../../contracts/tools/runtime-contract");
const { SOURCE_SHA256, PROFILE, inspectNativeComparison } = require("./v7-native-differential.cjs");

const expected = [{ name: "opening" }, { name: "after-move" }];
const passed = expected.map(item => ({ ...item, status: "pass" }));

test("전체 ordered 결과만 성공으로 인수한다", () => {
  assert.deepEqual(inspectNativeComparison({ status: "pass", cases: passed }, expected),
    { cases: passed, failure: null });
  for (const cases of [passed.slice(0, 1), [...passed, passed[0]], [...passed].reverse()]) {
    assert.equal(inspectNativeComparison({ status: "pass", cases }, expected).failure.status, "probe-error");
  }
});

test("실행 전 오류의 정확한 종류와 원래 진단을 보존한다", () => {
  for (const status of ["native-timeout", "native-unavailable", "native-unsupported", "probe-error", "version-mismatch"]) {
    const reason = `${status}: worker stage failed`;
    const result = inspectNativeComparison({ status, reason }, expected);
    assert.equal(result.failure.status, status);
    assert.equal(result.failure.reason, reason);
    assert.equal(result.failure.observedCases, 0);
  }
  assert.equal(inspectNativeComparison({ status: "native-timeout", reason: " " }, expected).failure.status,
    "probe-error");
});

test("집계와 사례의 모순·잘못된 상태를 성공이나 정상 실패로 위장하지 않는다", () => {
  for (const response of [
    { status: "fail", cases: passed },
    { status: "pass", cases: [passed[0], { ...passed[1], status: "mismatch", reason: "state differs" }] },
    { status: "unknown", cases: passed },
    { status: "fail", cases: [passed[0], { name: "after-move" }] },
    null,
  ]) assert.equal(inspectNativeComparison(response, expected).failure.status, "probe-error");
});

test("실제 사례 실패와 원문 메시지는 전체 실패로 전파한다", () => {
  const result = inspectNativeComparison({ status: "fail", cases: [passed[0],
    { name: "after-move", status: "mismatch", reason: "state.board[5][0] differs" }] }, expected);
  assert.equal(result.failure.status, "fail");
  assert.equal(result.failure.reason, "state.board[5][0] differs");
  assert.equal(result.failure.expectedCases, 2);
  assert.equal(result.failure.observedCases, 2);
});

test("Python preflight는 이전 profile과 빠진·변조된 execution manifest를 거부한다", () => {
  const contract = createRuntimeContract({ baseline: "site-20260928" });
  const request = {
    phase: "preflight", sourceSha256: SOURCE_SHA256, profile: PROFILE,
    rulesVersion: contract.catalog.rulesVersion, catalogVersion: contract.catalog.catalogVersion,
    executionProfile: contract.catalog.executionProfile, observationPolicy: contract.observationPolicy,
  };
  const nativeCatalog = {
    rulesVersion: request.rulesVersion, catalogVersion: request.catalogVersion,
    source: { files: [{ name: "main-pinned.js", sha256: SOURCE_SHA256 }] },
    executionProfile: request.executionProfile,
  };
  // Exercise the actual ingress without installing a wheel or writing a
  // shadow package. This fake supplies metadata only; no rules are simulated.
  const fakeNativePreflight = `
import io, json, runpy, sys, types
fixture = json.load(sys.stdin)
package = types.ModuleType("accelerate_chess")
package.__path__ = []
native = types.ModuleType("accelerate_chess._native")
native.site_catalog = lambda _rules: fixture["catalog"]
native.site_observation_policy = lambda _rules: fixture["policy"]
package._native = native
sys.modules["accelerate_chess"] = package
sys.modules["accelerate_chess._native"] = native
sys.stdin = io.StringIO(json.dumps(fixture["request"]))
runpy.run_path(sys.argv[1], run_name="__main__")
`;
  const ingress = (value, catalog = nativeCatalog) => {
    const child = spawnSync(process.env.PYTHON || "python", ["-I", "-c", fakeNativePreflight,
      path.join(__dirname, "v7-native-probe.py")], {
      input: JSON.stringify({ request: value, catalog, policy: request.observationPolicy }),
      encoding: "utf8", timeout: 10000, maxBuffer: 1024 * 1024, windowsHide: true,
    });
    assert.ifError(child.error);
    assert.equal(child.status, 0, child.stderr);
    return JSON.parse(child.stdout);
  };
  assert.deepEqual(ingress(request), { status: "ready" });
  for (const mutate of [
    value => { value.profile = "accelerate-headless-semantic-v7"; },
    value => { delete value.executionProfile; },
    value => { delete value.executionProfile.sha256; },
    value => { value.executionProfile.sha256 = "0".repeat(64); },
    value => { value.executionProfile.version = "accelerate-headless-semantic-v7"; },
    value => { value.executionProfile.manifest = "unreviewed-profile.json"; },
  ]) {
    const altered = contract.jsonCopy(request);
    mutate(altered);
    const failure = ingress(altered);
    assert.equal(failure.status, "version-mismatch");
    assert.match(failure.reason, /source\/profile identity differs|execution profile identity differs/);
  }
  const missingNativeProfile = contract.jsonCopy(nativeCatalog);
  delete missingNativeProfile.executionProfile;
  assert.equal(ingress(request, missingNativeProfile).status, "version-mismatch");
  const staleNativeProfile = contract.jsonCopy(nativeCatalog);
  staleNativeProfile.executionProfile.version = "accelerate-headless-semantic-v7";
  assert.equal(ingress(request, staleNativeProfile).status, "version-mismatch");
});
