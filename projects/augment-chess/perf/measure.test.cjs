"use strict";

const assert = require("node:assert/strict");
const path = require("node:path");
const test = require("node:test");
const { compare, parseOptions, reportPath, validateBuildProvenance, reusableInputFingerprint } = require("./measure.cjs");

const phases = ["newGameFreshSession", "observeWhiteFreshSession", "observeWhiteWarmSession",
  "legalActions", "applyWithHistory"];
const timing = value => ({ samples: 2, iterationsPerSample: 1,
  p50Ns: value, p95Ns: value, durationsNs: [value, value] });

function fixture(transitionStatus = "supported") {
  const expected = {
    newGamePosition: { state: { turn: "white" } },
    observationWhite: { viewer: "white" },
    observationBlack: { viewer: "black" },
    legalPayloads: [{ type: "move", from: [6, 4], to: [4, 4] }],
    nextState: { turn: "black" }, nextRng: { state: 7 }, nextHistory: [{ kind: "move" }],
    nextPosition: { state: { turn: "black" }, rng: { state: 7 }, history: [{ kind: "move" }], positionId: "source-next-id" },
  };
  const reference = [{ expected, report: { style: "normal",
    timings: Object.fromEntries(phases.map(name => [name, timing(200)])) } }];
  const native = { cases: [{ style: "normal",
  newGame: { status: "supported", preflight: expected.newGamePosition,
    timings: { newGameFreshSession: timing(100) } },
  observation: { status: "supported", preflight: {
    observationWhite: expected.observationWhite,
    observationBlack: expected.observationBlack,
  }, timings: Object.fromEntries(phases.slice(1, 3).map(name => [name, timing(100)])) },
  transition: transitionStatus === "supported" ? { status: "supported", preflight: {
    legalPayloads: expected.legalPayloads, nextState: expected.nextState,
    nextRng: expected.nextRng, nextHistory: expected.nextHistory, nextPosition: expected.nextPosition,
  }, timings: Object.fromEntries(phases.slice(3).map(name => [name, timing(100)])) } :
    { status: "unsupported", reason: "v7 transition is closed" } }] };
  return { contract: { canonical: JSON.stringify }, reference, native };
}

test("ratios require complete ordered legal payload and next state, RNG, history parity", () => {
  const { contract, reference, native } = fixture();
  const matched = compare(contract, reference, native)[0];
  assert.equal(matched.parity, "matched");
  assert.equal(matched.transition.parity, "matched");
  assert.equal(matched.ratios.observeWhiteWarmSession.observedP50Ratio, 2);
  assert.equal(matched.transition.ratios.applyWithHistory.observedP95Ratio, 2);

  native.cases[0].transition.preflight.nextHistory = [];
  const mismatch = compare(contract, reference, native)[0];
  assert.equal(mismatch.transition.parity, "mismatch");
  assert.match(mismatch.transition.mismatchedFields[0], /^nextHistory: differs/);
  assert.equal(mismatch.transition.ratios, null);
});

test("unsupported transition never produces a speed ratio", () => {
  const { contract, reference, native } = fixture("unsupported");
  const result = compare(contract, reference, native)[0];
  assert.equal(result.parity, "unsupported");
  assert.equal(result.transition.parity, "unsupported");
  assert.equal(result.transition.ratios, null);
  assert.equal(result.ratios.newGameFreshSession.observedP50Ratio, 2);
});

test("unsupported observation still retains verified new-game stage and no observation ratio", () => {
  const { contract, reference, native } = fixture("unsupported");
  native.cases[0].observation = { status: "unsupported", reason: "UI hints not ported" };
  const result = compare(contract, reference, native)[0];
  assert.equal(result.parity, "unsupported");
  assert.equal(result.newGame.parity, "matched");
  assert.equal(result.observation.parity, "unsupported");
  assert.equal(result.observation.ratios, null);
  assert.equal(result.ratios.newGameFreshSession.observedP50Ratio, 2);
  assert.equal(Object.hasOwn(result.ratios, "observeWhiteWarmSession"), false);
});

test("zero or malformed native timing cannot yield an infinite or fabricated ratio", () => {
  const { contract, reference, native } = fixture();
  native.cases[0].observation.timings.observeWhiteWarmSession.p50Ns = 0;
  assert.throws(() => compare(contract, reference, native), /observeWhiteWarmSession p50Ns must be finite and positive/);
  native.cases[0].observation.timings.observeWhiteWarmSession = timing(100);
  native.cases[0].transition.timings.legalActions.samples = 1;
  assert.throws(() => compare(contract, reference, native), /legalActions timing sample shape differs/);
});

test("operation budget covers reference, native, and synthetic dispatch calls", () => {
  const source = path.resolve(__dirname);
  const native = path.resolve(__dirname, "native", "unused-binary");
  assert.throws(() => parseOptions(["--source-root", source, "--native-bin", native,
    "--samples", "100", "--warmups", "20", "--iterations", "100"]), /bounded operations/);
  assert.equal(parseOptions(["--source-root", source, "--samples", "1",
    "--warmups", "0", "--iterations", "1"]).samples, 1);
});

test("a repository folder beginning with two dots is still an internal report path", () => {
  const repo = path.resolve(__dirname, "../../..");
  assert.throws(() => reportPath({ output: path.join(repo, "..not-external", "report.json") }), /outside the repository/);
  assert.throws(() => reportPath({ output: path.join(repo, ".env.json") }), /non-secret JSON file/);
});

test("next Position identity mismatch rejects apply ratio even when state, RNG and history match", () => {
  const { contract, reference, native } = fixture();
  native.cases[0].transition.preflight.nextPosition = {
    ...native.cases[0].transition.preflight.nextPosition, positionId: "other-position-id",
  };
  const result = compare(contract, reference, native)[0];
  assert.equal(result.parity, "mismatch");
  assert.equal(result.transition.ratios, null);
  assert.match(result.mismatchedFields[0], /^nextPosition: differs/);
});

test("unsupported stages preserve a specific reason rather than a blank success", () => {
  const { contract, reference, native } = fixture("unsupported");
  native.cases[0].transition.reason = "";
  assert.throws(() => compare(contract, reference, native), /unsupported transition must preserve its exact error reason/);
});

test("reported native quantiles must be derived from the actual samples", () => {
  const { contract, reference, native } = fixture();
  native.cases[0].transition.timings.applyWithHistory.p50Ns = 1;
  assert.throws(() => compare(contract, reference, native), /reported quantiles differ/);
});

test("stale native code cannot be attributed to the current engine source", () => {
  const names = ["engineSource", "adapterRuntimeSource", "gameContractSource", "perfNativeSource"];
  const sources = Object.fromEntries(names.map((name, index) => [name, { sha256: String(index).repeat(64), files: index + 1 }]));
  const build = { sources: structuredClone(sources), compiler: "rustc fixture", target: "fixture-target",
    profile: "release", allocationProbe: false };
  assert.equal(validateBuildProvenance(build, sources), build);
  build.sources.engineSource.sha256 = "f".repeat(64);
  assert.throws(() => validateBuildProvenance(build, sources), /Native executable engineSource fingerprint differs/);
  assert.throws(() => validateBuildProvenance({}, sources), /lacks compiler\/target\/profile\/feature provenance/);
});

test("input fingerprint permits another Git SHA but rejects changed engine, fixture and resource inputs", () => {
  const contract = { canonical: JSON.stringify };
  const report = { schemaVersion: 2, sourceClientSha256: "frozen-source", parserSha256: "frozen-parser",
    rulesVersion: "v7", catalogVersion: "catalog", profileVersion: "headless-profile",
    samples: 7, warmups: 2, iterations: 1,
    resources: { workerCount: 1, elapsedBudgetMs: 120000 },
    environment: { gitRevision: "old-git-sha", platform: "linux", architecture: "x64", node: "v22",
      cpuModel: "fixture", logicalCpuCount: 4, memoryBytes: 8e9,
      engineSource: { sha256: "engine" }, adapterRuntimeSource: { sha256: "runtime" },
      gameContractSource: { sha256: "contract" }, perfNativeSource: { sha256: "perf" },
      oracleAdapterSource: { sha256: "oracle" }, oracleAdapterManifestSha256: "manifest",
      sharedSchemaSource: { sha256: "schema" }, harnessSource: { sha256: "harness" }, workspaceManifestSha256: "workspace" },
    reference: [{ style: "normal", positionDigest: "start", actionDigest: "action", nextPositionDigest: "next",
      legalPayloadsDigest: "ordered-legal" }], nativeBinarySha256: "binary",
    nativeBuildProvenance: { compiler: "rustc fixture", profile: "release", allocationProbe: false } };
  const original = reusableInputFingerprint(contract, report);
  report.environment.gitRevision = "new-git-sha";
  assert.equal(reusableInputFingerprint(contract, report), original);
  for (const change of [
    copy => { copy.environment.engineSource.sha256 = "changed-engine"; },
    copy => { copy.reference[0].nextPositionDigest = "changed-next"; },
    copy => { copy.resources.workerCount = 2; },
    copy => { copy.nativeBuildProvenance.profile = "debug"; },
  ]) {
    const changed = structuredClone(report);
    change(changed);
    assert.notEqual(reusableInputFingerprint(contract, changed), original);
  }
});
