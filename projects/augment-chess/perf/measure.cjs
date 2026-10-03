#!/usr/bin/env node
"use strict";

// Source-pinned, bounded investigation. Raw fixture positions stay in memory;
// the report goes outside Git. A ratio requires full measured-case parity.
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { execFileSync, spawnSync } = require("node:child_process");
const { performance } = require("node:perf_hooks");
const { getHeapStatistics } = require("node:v8");
const { FrozenClientSource, GameAdapter } = require("../oracle/game-adapter/src");
const { createRuntimeContract } = require("../contracts/tools/runtime-contract");

const ROOT = path.resolve(__dirname, "../../..");
const CLIENT_SHA256 = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
const STYLES = ["normal", "chaos", "grand"];
const MAX_TOTAL_OPERATIONS = 50_000;
const MAX_NATIVE_RESPONSE_BYTES = 8 * 1024 * 1024;
const SOURCE_FINGERPRINT_KEYS = ["engineSource", "adapterRuntimeSource", "gameContractSource", "perfNativeSource",
  "oracleAdapterSource", "oracleAdapterManifestSha256", "sharedSchemaSource", "harnessSource", "workspaceManifestSha256"];
const SOURCE_DIRECTORIES_TO_SKIP = new Set([
  "node_modules", "dist", "dist-server", "build", "coverage", "models", "checkpoints", ".cache", "target",
]);
let resultSink;
let measurementDeadline = Infinity;

function checkDeadline() {
  if (performance.now() > measurementDeadline) throw new Error("Measurement elapsed-time budget exceeded between synchronous operations.");
}

function describeError(error, depth = 0) {
  if (!error || typeof error !== "object") return String(error);
  const header = `${error.name || "Error"}${error.code ? ` [${error.code}]` : ""}: ${error.message || "missing error message"}`;
  if (depth >= 3) return header;
  const details = [];
  if (error.cause !== undefined) details.push(`cause: ${describeError(error.cause, depth + 1)}`);
  if (Array.isArray(error.errors)) {
    for (const [index, nested] of error.errors.slice(0, 16).entries()) details.push(`error ${index + 1}: ${describeError(nested, depth + 1)}`);
    if (error.errors.length > 16) details.push(`${error.errors.length - 16} additional errors omitted from bounded diagnostic`);
  }
  return details.length ? `${header}; ${details.join("; ")}` : header;
}

function usage() {
  return "Usage: node projects/augment-chess/perf/measure.cjs [--source-root ABSOLUTE_PATH] [--native-bin ABSOLUTE_PATH] [--style normal|chaos|grand|all] [--samples 7] [--warmups 2] [--iterations 1] [--timeout-ms 120000] [--output ABSOLUTE_PATH]";
}

function parseOptions(argv) {
  const parsed = { style: "all", samples: 7, warmups: 2, iterations: 1, "timeout-ms": 120000 };
  const names = new Set(["source-root", "native-bin", "style", "samples", "warmups", "iterations", "timeout-ms", "output"]);
  const seen = new Set();
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (!flag?.startsWith("--") || !names.has(flag.slice(2)) || value === undefined) throw new TypeError(usage());
    const key = flag.slice(2);
    if (seen.has(key)) throw new TypeError(`Duplicate ${flag}.`);
    seen.add(key);
    parsed[key] = ["samples", "warmups", "iterations", "timeout-ms"].includes(key) ? Number(value) : value;
  }
  const externalCache = process.env.RUNNER_TEMP || process.env.APPDATA;
  parsed["source-root"] ||= process.env.ACCELERATE_SITE_BASELINE_LATEST ||
    process.env.ACCELERATE_SITE_BASELINE ||
    (externalCache && path.join(externalCache, "Accelerate", "cache", "site-baseline-20260928-e5ed84fc"));
  if (!parsed["source-root"] || !path.isAbsolute(parsed["source-root"]))
    throw new TypeError("A pinned absolute --source-root or ACCELERATE_SITE_BASELINE is required.");
  for (const key of ["native-bin", "output"]) {
    if (parsed[key] && !path.isAbsolute(parsed[key])) throw new TypeError(`${key} must be absolute.`);
  }
  if (parsed.style !== "all" && !STYLES.includes(parsed.style)) throw new TypeError("Unknown game style.");
  for (const [key, minimum, maximum] of [["samples", 1, 100], ["warmups", 0, 20], ["iterations", 1, 100], ["timeout-ms", 1000, 600000]]) {
    if (!Number.isSafeInteger(parsed[key]) || parsed[key] < minimum || parsed[key] > maximum)
      throw new TypeError(`${key} must be ${minimum}..${maximum}.`);
  }
  const styleCount = parsed.style === "all" ? STYLES.length : 1;
  // Reserve both cold calls, main stages, every boundary diagnostic,
  // preflight and opt-in allocator samples. Inner engine/VM work is separate.
  const perCase = 2 * parsed.samples + 14 * (parsed.samples + parsed.warmups) * parsed.iterations + 40 + 11 * parsed.samples;
  const nativeOperations = parsed["native-bin"] ?
    styleCount * perCase + 200 * (parsed.samples + parsed.warmups) : 0;
  if (styleCount * perCase + nativeOperations > MAX_TOTAL_OPERATIONS)
    throw new TypeError(`Measurement exceeds ${MAX_TOTAL_OPERATIONS} bounded operations.`);
  return parsed;
}

function reportPath(selected) {
  const base = process.env.RUNNER_TEMP || process.env.APPDATA;
  if (!selected.output && (!base || !path.isAbsolute(base)))
    throw new TypeError("RUNNER_TEMP or APPDATA must provide an absolute external report root.");
  const output = path.resolve(selected.output || path.join(base, "Accelerate", "reports", "adapter-transition-perf", "report.json"));
  if (path.extname(output).toLowerCase() !== ".json" || /^\.env|credential|service[-_]account|api[-_]?key|ssh[-_]?key/i.test(path.basename(output)))
    throw new TypeError("Performance report output must be a non-secret JSON file.");
  const relative = path.relative(ROOT, output);
  if (relative !== ".." && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative))
    throw new TypeError("Raw performance reports must be written outside the repository.");
  if (fs.existsSync(output) && fs.lstatSync(output).isSymbolicLink())
    throw new TypeError("Performance report output must not be a symlink.");
  let ancestor = path.dirname(output);
  while (!fs.existsSync(ancestor)) ancestor = path.dirname(ancestor);
  const resolved = path.resolve(fs.realpathSync(ancestor), path.relative(ancestor, output));
  const realRelative = path.relative(fs.realpathSync(ROOT), resolved);
  if (realRelative !== ".." && !realRelative.startsWith(`..${path.sep}`) && !path.isAbsolute(realRelative))
    throw new TypeError("Performance report parent resolves inside the repository through a link or junction.");
  return output;
}

function gitRevision() {
  try { return execFileSync("git", ["rev-parse", "HEAD"], { cwd: ROOT, encoding: "utf8" }).trim(); }
  catch (error) { throw new Error(`Cannot determine Git revision: ${error.message}`); }
}

function workingTreeDirty() {
  try {
    return execFileSync("git", ["status", "--porcelain", "--",
      "projects/augment-chess/engine", "packages/adapter-runtime",
      "projects/augment-chess/oracle/game-adapter", "projects/augment-chess/contracts",
      "projects/augment-chess/perf", "packages/adapter-contract", "Cargo.toml", "Cargo.lock"],
    { cwd: ROOT, encoding: "utf8" }).trim().length > 0;
  } catch (error) { throw new Error(`Cannot determine working-tree state: ${error.message}`); }
}

function sourceDigest(directory, extensions) {
  const digest = crypto.createHash("sha256");
  const files = [];
  function collect(current, relative) {
    for (const entry of fs.readdirSync(current, { withFileTypes: true }).sort((a, b) =>
      a.name < b.name ? -1 : a.name > b.name ? 1 : 0)) {
      const next = path.join(current, entry.name);
      const name = path.join(relative, entry.name).replaceAll("\\", "/");
      if (entry.name.startsWith(".env") || /credential|service[-_]account|api[-_]?key|ssh[-_]?key/i.test(entry.name)) continue;
      if (entry.isDirectory() && !SOURCE_DIRECTORIES_TO_SKIP.has(entry.name)) collect(next, name);
      else if (entry.isFile() && extensions.some(extension => entry.name.endsWith(extension))) files.push([next, name]);
      else if (entry.isSymbolicLink()) throw new Error(`Source fingerprint refuses symlink ${name}.`);
    }
  }
  collect(directory, "");
  for (const [file, name] of files) {
    digest.update(name).update("\0").update(fs.readFileSync(file)).update("\0");
  }
  return { sha256: digest.digest("hex"), files: files.length };
}

function sourceFingerprints() {
  return {
    engineSource: sourceDigest(path.join(ROOT, "projects/augment-chess/engine"), [".rs", ".toml"]),
    adapterRuntimeSource: sourceDigest(path.join(ROOT, "packages/adapter-runtime"), [".rs", ".toml"]),
    gameContractSource: sourceDigest(path.join(ROOT, "projects/augment-chess/contracts"), [".rs", ".toml", ".json", ".js", ".cjs"]),
    perfNativeSource: sourceDigest(path.join(__dirname, "native"), [".rs", ".toml", ".lock"]),
    oracleAdapterSource: sourceDigest(path.join(ROOT, "projects/augment-chess/oracle/game-adapter/src"), [".js", ".cjs"]),
    oracleAdapterManifestSha256: crypto.createHash("sha256").update(fs.readFileSync(path.join(ROOT, "projects/augment-chess/oracle/game-adapter/package.json"))).digest("hex"),
    sharedSchemaSource: sourceDigest(path.join(ROOT, "packages/adapter-contract"), [".json"]),
    harnessSource: sourceDigest(__dirname, [".cjs", ".rs", ".toml", ".lock"]),
    workspaceManifestSha256: crypto.createHash("sha256").update(fs.readFileSync(path.join(ROOT, "Cargo.toml"))).digest("hex"),
  };
}

function validateBuildProvenance(provenance, currentSources) {
  if (!provenance || typeof provenance.compiler !== "string" || typeof provenance.target !== "string" ||
      !["release", "debug"].includes(provenance.profile) || typeof provenance.allocationProbe !== "boolean")
    throw new Error("Native executable lacks compiler/target/profile/feature provenance; rebuild the harness.");
  for (const key of ["engineSource", "adapterRuntimeSource", "gameContractSource", "perfNativeSource"]) {
    const compiled = provenance.sources?.[key];
    const current = currentSources[key];
    if (!compiled || !current || compiled.sha256 !== current.sha256 || compiled.files !== current.files)
      throw new Error(`Native executable ${key} fingerprint differs from current source (compiled ${compiled?.sha256 || "missing"}, current ${current?.sha256 || "missing"}).`);
  }
  return provenance;
}

function percentile(values, percent) {
  const sorted = [...values].sort((left, right) => left - right);
  if (!sorted.length) throw new TypeError("No timing samples.");
  return sorted[Math.ceil(percent * sorted.length / 100) - 1];
}

function measure(samples, warmups, iterations, operation) {
  for (let count = 0; count < warmups * iterations; count++) { checkDeadline(); resultSink = operation(); }
  const durationsNs = [];
  const beforeMemory = process.memoryUsage();
  for (let sample = 0; sample < samples; sample++) {
    checkDeadline();
    const started = performance.now();
    for (let count = 0; count < iterations; count++) resultSink = operation();
    durationsNs.push((performance.now() - started) * 1e6 / iterations);
  }
  return {
    samples, iterationsPerSample: iterations,
    p50Ns: percentile(durationsNs, 50), p95Ns: percentile(durationsNs, 95),
    minNs: Math.min(...durationsNs), maxNs: Math.max(...durationsNs),
    meanNs: durationsNs.reduce((sum, value) => sum + value, 0) / durationsNs.length,
    durationsNs,
    processMemory: { before: beforeMemory, after: process.memoryUsage(),
      scope: "Node process only; snapshots outside timing; does not observe synchronous or native-child peaks" },
  };
}

function oracleBoundaryDiagnostics(contract, position, selected) {
  const canonical = contract.canonical(position);
  const encoded = JSON.stringify(position);
  const canonicalBytes = Buffer.from(canonical);
  const timings = Object.fromEntries([
    ["sourceEnvelopeClone", () => structuredClone(position)],
    ["canonicalEnvelope", () => contract.canonical(position)],
    ["sha256PreparedCanonicalBytes", () => crypto.createHash("sha256").update(canonicalBytes).digest()],
    ["jsonEncodeEnvelope", () => JSON.stringify(position)],
    ["jsonDecodeEnvelope", () => JSON.parse(encoded)],
  ].map(([name, operation]) => [name, measure(selected.samples, selected.warmups, selected.iterations, operation)]));
  return { scope: "prepared source envelope; JCS/digest and JSON conversion; no wire transport",
    preflight: { canonicalEnvelopeSha256: crypto.createHash("sha256").update(canonicalBytes).digest("hex"),
      canonicalBytes: canonicalBytes.length, jsonBytes: Buffer.byteLength(encoded) }, timings };
}

function oracleCase(source, contract, style, selected) {
  const adapter = new GameAdapter({ source, contract, maxCandidates: 100000, maxMicrotasks: 256 });
  try {
    const config = { gameStyle: style, draftDelete: true };
    const position = adapter.newGame(config, 37);
    contract.validatePosition(position);
    const observationWhite = adapter.observe(position, "white");
    const observationBlack = adapter.observe(position, "black");
    if (position.state.mode !== "play") throw new Error(`${style}: fixture did not reach play.`);
    const actions = adapter.actions(position);
    if (!actions.length) throw new Error(`${style}: source has no first-play legal action.`);
    const action = actions[0];
    const positionDigest = contract.digest(position);
    const actionDigest = contract.digest(action.payload);
    const step = adapter.apply(position, action, { recordHistory: true });
    if (!step.ok || step.position.history.length !== position.history.length + 1)
      throw new Error(`${style}: source first action was not accepted with history: ${step.error ? describeError(step.error) : "public history did not grow by one"}.`);
    contract.validatePosition(step.position);
    const expected = {
      newGamePosition: position,
      observationWhite,
      observationBlack,
      legalPayloads: actions.map(item => item.payload),
      nextState: step.position.state,
      nextRng: step.position.rng,
      nextHistory: step.position.history,
      nextPosition: step.position,
    };
    const timings = {
      newGameFreshSession: measure(selected.samples, 0, 1, () => {
        const fresh = new GameAdapter({ source, contract });
        try { return fresh.newGame(config, 37); }
        finally { fresh.dispose(); }
      }),
      observeWhiteFreshSession: measure(selected.samples, 0, 1, () => {
        const fresh = new GameAdapter({ source, contract });
        try { return fresh.observe(position, "white"); }
        finally { fresh.dispose(); }
      }),
      observeWhiteWarmSession: measure(selected.samples, selected.warmups, selected.iterations,
        () => adapter.observe(position, "white")),
      legalActions: measure(selected.samples, selected.warmups, selected.iterations, () => adapter.actions(position)),
      applyWithHistory: measure(selected.samples, selected.warmups, selected.iterations, () => {
        const result = adapter.apply(position, action, { recordHistory: true });
        if (!result.ok) throw new Error(`${style}: repeated source action rejected: ${describeError(result.error)}`);
        return result;
      }),
    };
    if (contract.digest(position) !== positionDigest || contract.digest(action.payload) !== actionDigest)
      throw new Error(`${style}: benchmark calls mutated their source position or selected action.`);
    return {
      input: { style, config, seed: 37, position, actionPayload: action.payload },
      expected,
      report: {
        style,
        positionDigest,
        actionDigest,
        nextPositionDigest: contract.digest(step.position),
        legalPayloadsDigest: contract.digest(actions.map(item => item.payload)),
        legalCount: actions.length,
        actionType: action.payload.type,
        timings,
        boundaryDiagnostics: oracleBoundaryDiagnostics(contract, position, selected),
      },
    };
  } finally { adapter.dispose(); }
}

function readNativeJson(binary, argv, input = "", timeoutMs = 120000) {
  checkDeadline();
  const remainingMs = Math.max(1, Math.min(timeoutMs, Math.ceil(measurementDeadline - performance.now())));
  const child = spawnSync(binary, argv, { input, encoding: "utf8", timeout: remainingMs,
    maxBuffer: MAX_NATIVE_RESPONSE_BYTES, windowsHide: true });
  if (child.error) throw new Error(`Native benchmark process: ${child.error.name}: ${child.error.message}; signal ${child.signal || "none"}; stderr ${(child.stderr || "").trim()}`);
  if (child.status !== 0) throw new Error(`Native benchmark exited ${child.status}, signal ${child.signal || "none"}: ${(child.stderr || "").trim()}`);
  if (child.stderr?.trim()) process.stderr.write(`Native diagnostic (${argv[0] || "measurement"}): ${child.stderr.trim()}\n`);
  let result;
  try { result = JSON.parse(child.stdout); }
  catch (error) { throw new Error(`Native benchmark returned invalid JSON: ${error.message}; stderr ${(child.stderr || "").trim()}`); }
  result.processDiagnostics = { stderr: child.stderr || null, signal: child.signal, exitCode: child.status };
  return result;
}

function invokeNative(binary, selected, cases) {
  const input = JSON.stringify({ samples: selected.samples, warmups: selected.warmups,
    iterations: selected.iterations, cases: cases.map(item => item.input) });
  if (Buffer.byteLength(input) > 4 * 1024 * 1024) throw new Error("Native benchmark request exceeds 4 MiB.");
  const result = readNativeJson(binary, [], input, selected["timeout-ms"]);
  if (result.schemaVersion !== 2 || !Array.isArray(result.cases) || result.cases.length !== cases.length)
    throw new Error("Native benchmark response shape or case count differs.");
  return result;
}

function ratioForPhase(style, name, reference, candidate) {
  if (!reference || !candidate || candidate.samples !== reference.samples ||
      candidate.iterationsPerSample !== reference.iterationsPerSample ||
      !Array.isArray(candidate.durationsNs) || candidate.durationsNs.length !== candidate.samples ||
      !Array.isArray(reference.durationsNs) || reference.durationsNs.length !== reference.samples) {
    throw new Error(`${style}: ${name} timing sample shape differs.`);
  }
  for (const key of ["p50Ns", "p95Ns"]) {
    if (!Number.isFinite(reference[key]) || reference[key] <= 0 ||
        !Number.isFinite(candidate[key]) || candidate[key] <= 0)
      throw new Error(`${style}: ${name} ${key} must be finite and positive.`);
  }
  if ([...reference.durationsNs, ...candidate.durationsNs].some(duration => !Number.isFinite(duration) || duration < 0))
    throw new Error(`${style}: ${name} has an invalid timing sample.`);
  for (const value of [reference, candidate]) {
    if (value.p50Ns !== percentile(value.durationsNs, 50) || value.p95Ns !== percentile(value.durationsNs, 95))
      throw new Error(`${style}: ${name} reported quantiles differ from its timing samples.`);
  }
  return {
    observedP50Ratio: reference.p50Ns / candidate.p50Ns,
    observedP95Ratio: reference.p95Ns / candidate.p95Ns,
  };
}

function compare(contract, oracle, native) {
  const cases = [];
  for (let index = 0; index < oracle.length; index++) {
    const expected = oracle[index];
    const actual = native.cases[index];
    if (actual.style !== expected.report.style) throw new Error(`Native case order differs at index ${index}.`);
    const compareField = (key, candidate, mismatches) => {
      if (candidate === undefined) { mismatches.push(`${key}: missing`); return; }
      try {
        const expectedText = contract.canonical(expected.expected[key]);
        const actualText = contract.canonical(candidate);
        if (expectedText !== actualText) {
          const digest = value => crypto.createHash("sha256").update(value).digest("hex");
          mismatches.push(`${key}: differs (expected ${digest(expectedText)}, actual ${digest(actualText)})`);
        }
      } catch (error) { mismatches.push(`${key}: invalid (${error.name}: ${error.message})`); }
    };
    const newGame = actual.newGame;
    if (newGame?.status !== "supported" || !newGame.timings?.newGameFreshSession)
      throw new Error(`${actual.style}: native new-game preflight or timing is missing.`);
    const newGameMismatches = [];
    compareField("newGamePosition", newGame.preflight, newGameMismatches);
    const newGameRatios = newGameMismatches.length ? null : {
      newGameFreshSession: ratioForPhase(actual.style, "newGameFreshSession",
        expected.report.timings.newGameFreshSession, newGame.timings.newGameFreshSession),
    };
    const observation = actual.observation;
    if (!observation || !["unsupported", "supported"].includes(observation.status))
      throw new Error(`${actual.style}: native observation status is missing.`);
    if (observation.status === "unsupported" && (typeof observation.reason !== "string" || !observation.reason.trim()))
      throw new Error(`${actual.style}: unsupported observation must preserve its exact error reason.`);
    const observationMismatches = [];
    if (observation.status === "supported") {
      for (const key of ["observationWhite", "observationBlack"])
        compareField(key, observation.preflight?.[key], observationMismatches);
      if (!observation.timings?.observeWhiteFreshSession || !observation.timings?.observeWhiteWarmSession)
        throw new Error(`${actual.style}: supported native observation timings are missing.`);
    }
    const observationRatios = observation.status !== "supported" || observationMismatches.length ? null :
      Object.fromEntries(["observeWhiteFreshSession", "observeWhiteWarmSession"].map(name =>
        [name, ratioForPhase(actual.style, name, expected.report.timings[name], observation.timings[name])]));
    const transition = actual.transition;
    if (!transition || !["unsupported", "supported"].includes(transition.status))
      throw new Error(`${actual.style}: native transition status is missing.`);
    if (transition.status === "unsupported" && (typeof transition.reason !== "string" || !transition.reason.trim()))
      throw new Error(`${actual.style}: unsupported transition must preserve its exact error reason.`);
    const transitionMismatches = [];
    if (transition.status === "supported") {
      for (const key of ["legalPayloads", "nextState", "nextRng", "nextHistory", "nextPosition"]) {
        compareField(key, transition.preflight?.[key], transitionMismatches);
      }
      if (!transition.timings?.legalActions || !transition.timings?.applyWithHistory)
        throw new Error(`${actual.style}: supported native transition timing phases are missing.`);
    }
    const transitionRatios = transition.status !== "supported" || transitionMismatches.length ? null :
      Object.fromEntries(["legalActions", "applyWithHistory"].map(name =>
        [name, ratioForPhase(actual.style, name, expected.report.timings[name], transition.timings[name])]));
    const boundaryMismatches = [];
    const expectedBoundary = expected.report.boundaryDiagnostics?.preflight;
    const actualBoundary = actual.boundaryDiagnostics?.preflight;
    if (expectedBoundary && (!actualBoundary ||
        actualBoundary.canonicalEnvelopeSha256 !== expectedBoundary.canonicalEnvelopeSha256 ||
        actualBoundary.canonicalBytes !== expectedBoundary.canonicalBytes))
      boundaryMismatches.push("boundaryDiagnostics: canonical source envelope bytes/digest differ or are missing");
    const mismatchedFields = [...newGameMismatches, ...observationMismatches, ...transitionMismatches, ...boundaryMismatches];
    cases.push({ style: actual.style,
      parity: mismatchedFields.length ? "mismatch" :
        observation.status === "unsupported" || transition.status === "unsupported" ? "unsupported" : "matched",
      mismatchedFields,
      ratios: { ...newGameRatios, ...observationRatios },
      newGame: { parity: newGameMismatches.length ? "mismatch" : "matched",
        mismatchedFields: newGameMismatches, timings: newGame.timings, ratios: newGameRatios },
      observation: { parity: observation.status === "unsupported" ? "unsupported" :
        observationMismatches.length ? "mismatch" : "matched", reason: observation.reason || null,
        mismatchedFields: observationMismatches, timings: observation.timings || null,
        errorKind: observation.errorKind || null, errorCode: observation.errorCode || null,
        ratios: observationRatios },
      transition: { parity: transition.status === "unsupported" ? "unsupported" :
        transitionMismatches.length ? "mismatch" : "matched", reason: transition.reason || null,
        mismatchedFields: transitionMismatches, timings: transition.timings || null, ratios: transitionRatios,
        errorKind: transition.errorKind || null, errorCode: transition.errorCode || null },
      boundaryDiagnostics: actual.boundaryDiagnostics ? { ...actual.boundaryDiagnostics,
        parity: boundaryMismatches.length ? "mismatch" : "matched", mismatchedFields: boundaryMismatches } : null });
  }
  return cases;
}

function reusableInputFingerprint(contract, report) {
  const sources = Object.fromEntries(SOURCE_FINGERPRINT_KEYS.map(key => [key, report.environment[key]]));
  const inputs = { schemaVersion: report.schemaVersion, sources,
    sourceClientSha256: report.sourceClientSha256, parserSha256: report.parserSha256,
    rulesVersion: report.rulesVersion, catalogVersion: report.catalogVersion, profileVersion: report.profileVersion,
    environment: Object.fromEntries(["platform", "architecture", "node", "cpuModel", "logicalCpuCount", "memoryBytes"]
      .map(key => [key, report.environment[key]])),
    resources: report.resources,
    measurement: { samples: report.samples, warmups: report.warmups, iterations: report.iterations },
    fixtures: report.reference.map(item => ({ style: item.style, positionDigest: item.positionDigest,
      actionDigest: item.actionDigest, nextPositionDigest: item.nextPositionDigest, legalPayloadsDigest: item.legalPayloadsDigest,
      seed: 37, draftDelete: true })),
    nativeBinarySha256: report.nativeBinarySha256 || null, nativeBuildProvenance: report.nativeBuildProvenance || null };
  return crypto.createHash("sha256").update(contract.canonical(inputs)).digest("hex");
}

function run(argv = process.argv.slice(2)) {
  if (argv.length === 1 && argv[0] === "--help") { process.stdout.write(`${usage()}\n`); return; }
  const selected = parseOptions(argv);
  const output = reportPath(selected);
  measurementDeadline = performance.now() + selected["timeout-ms"];
  const sources = sourceFingerprints();
  const report = {
    schemaVersion: 2, status: "setup-error", startedAt: new Date().toISOString(),
    scope: "frozen-v7 seed 37 draftDelete true; selected normal/chaos/grand styles; fresh new game, fresh/warm white observation, first legal action per selected style",
    sourceClientSha256: CLIENT_SHA256,
    samples: selected.samples, warmups: selected.warmups, iterations: selected.iterations,
    resources: { workerCount: 1, gpu: "none", maxOuterOperations: MAX_TOTAL_OPERATIONS,
      elapsedBudgetMs: selected["timeout-ms"], maxNativeInputBytes: 4 * 1024 * 1024,
      maxNativeResponseBytes: MAX_NATIVE_RESPONSE_BYTES,
      nodeHeapLimitBytes: getHeapStatistics().heap_size_limit,
      nativeMemoryBoundary: "external process limit; this harness does not silently lower work after OOM",
      timeBoundary: "checks between Node synchronous samples and native child timeout; external process boundary required to interrupt a single stuck Node call" },
    styles: selected.style === "all" ? STYLES : [selected.style],
    environment: { platform: process.platform, architecture: process.arch, node: process.version,
      cpuModel: os.cpus()[0]?.model || null, logicalCpuCount: os.cpus().length,
      memoryBytes: os.totalmem(), gitRevision: gitRevision(), workingTreeDirty: workingTreeDirty(),
      ...sources },
    interpretation: "JS oracle VM adapter in process versus Rust public game adapter session. Source loading, process startup, stdin/stdout transport and Python/PyO3 are excluded. Fresh-session phases include construction around a prepared position; warm observation reuses a session. Mutable Rust apply uses a fresh session prepared outside each timer. Ratios require full-position or both public-observation parity; legal/apply require complete ordered payloads and full next Position/state/RNG/history. Boundary costs overlap real calls and are diagnostic; synthetic registry probe is separate. Concurrent work can distort timings; no fixed speed gate or whole-game claim.",
  };
  let failure;
  try {
    const contract = createRuntimeContract({ baseline: "site-20260928" });
    if (contract.catalog.source.files.find(item => /^main-/.test(item.name))?.sha256 !== CLIENT_SHA256)
      throw new Error("Runtime contract does not match the pinned v7 client SHA-256.");
    const source = new FrozenClientSource(selected["source-root"], { expectedClientSha256: CLIENT_SHA256 });
    report.rulesVersion = contract.catalog.rulesVersion;
    report.catalogVersion = contract.catalog.catalogVersion;
    report.profileVersion = contract.ORACLE_PROFILE_VERSION;
    report.parserSha256 = source.manifest.files.find(item => /^acorn-/.test(item.name))?.sha256 || null;
    if (!/^[a-f0-9]{64}$/.test(report.parserSha256 || ""))
      throw new Error("Frozen source manifest does not provide an exact parser SHA-256.");
    if (selected["native-bin"]) {
      report.nativeBinarySha256 = crypto.createHash("sha256").update(fs.readFileSync(selected["native-bin"])).digest("hex");
      const info = readNativeJson(selected["native-bin"], ["--build-info"], "", 10000);
      if (info.schemaVersion !== 2) throw new Error("Native executable uses an older measurement contract; rebuild it.");
      report.nativeBuildProvenance = validateBuildProvenance(info.buildProvenance, sources);
      report.nativeProcessDiagnostics = [info.processDiagnostics];
    }
    const oracle = report.styles.map(style => oracleCase(source, contract, style, selected));
    report.reference = oracle.map(item => item.report);
    if (selected["native-bin"]) {
      const native = invokeNative(selected["native-bin"], selected, oracle);
      report.nativeProcessDiagnostics.push(native.processDiagnostics);
      validateBuildProvenance(native.buildProvenance, sources);
      report.nativeBinaryVersion = native.binaryVersion;
      report.syntheticAdapterDispatch = native.syntheticAdapterDispatch;
      report.native = compare(contract, oracle, native);
      const mismatch = report.native.some(item => item.parity === "mismatch" || item.transition.parity === "mismatch");
      const unsupported = report.native.some(item => item.parity === "unsupported" || item.transition.parity === "unsupported");
      report.status = mismatch ? "parity-mismatch" : unsupported ? "partial-parity-no-go" : "bounded-parity-and-timings";
      report.decision = report.status === "bounded-parity-and-timings" ? "bounded-case-pass-only" : "NO-GO";
      if (mismatch) failure = new Error(`Native measured-case parity differs: ${report.native.filter(item => item.parity === "mismatch").map(item => `${item.style}: ${item.mismatchedFields.join(", ")}`).join("; ")}`);
      else if (unsupported) failure = new Error(`Native measured-case support incomplete: ${report.native.flatMap(item =>
        ["observation", "transition"].filter(stage => item[stage].parity === "unsupported")
          .map(stage => `${item.style}/${stage}: ${item[stage].reason}`)).join("; ")}`);
      if (report.nativeBuildProvenance.allocationProbe) {
        report.verificationStatus = report.status;
        if (!mismatch && !unsupported) report.status = "allocation-diagnostic-only";
        report.decision = "NO-GO";
        for (const item of report.native) {
          item.ratios = null;
          for (const stage of ["newGame", "observation", "transition"]) item[stage].ratios = null;
        }
      }
    } else { report.status = "reference-only"; report.decision = "NO-GO"; }
    if (JSON.stringify(sourceFingerprints()) !== JSON.stringify(sources))
      throw new Error("Relevant source inputs changed during measurement; all timings are unverified for the current revision.");
    report.reuseEvidence = { inputFingerprintSha256: reusableInputFingerprint(contract, report),
      originGitRevision: report.environment.gitRevision,
      eligibleForBoundedResultReuse: report.status === "bounded-parity-and-timings",
      reused: false,
      rule: "Git SHA is provenance, not the input key. Reuse only a successful same-scope receipt when source/schema/catalog/loader/parser/compiler/target/profile/binary/fixtures/environment/resources fingerprint is unchanged. Partial, failed or instrumented timings are diagnostic and cannot satisfy an unverified gate." };
  } catch (error) {
    failure = error;
    report.status = "measurement-error";
    report.decision = "NO-GO";
    report.error = describeError(error);
    if (report.native) for (const item of report.native) {
      item.ratios = null;
      for (const stage of ["newGame", "observation", "transition"]) item[stage].ratios = null;
    }
  }
  report.finishedAt = new Date().toISOString();
  if (failure) report.failure = describeError(failure);
  report.nodeProcessMaxRssKiB = process.resourceUsage().maxRSS;
  report.memoryInterpretation = "Node process-wide maximum; excludes native child and cannot be attributed to an individual phase.";
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify({ status: report.status, decision: report.decision, report: output,
    styles: report.styles, parity: report.native?.map(item => ({ style: item.style,
      newGame: item.newGame.parity, observation: item.observation.parity,
      transition: item.transition.parity,
      observationReason: item.observation.reason, transitionReason: item.transition.reason,
      mismatchedFields: item.mismatchedFields })),
    error: report.error || report.failure })}\n`);
  measurementDeadline = Infinity;
  if (failure) process.exitCode = 1;
  return report;
}

if (require.main === module) {
  try { run(); }
  catch (error) { console.error(describeError(error)); process.exitCode = 1; }
}

module.exports = { run, parseOptions, reportPath, percentile, measure, compare, validateBuildProvenance, reusableInputFingerprint };
