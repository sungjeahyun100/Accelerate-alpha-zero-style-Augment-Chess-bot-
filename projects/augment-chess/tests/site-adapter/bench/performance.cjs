#!/usr/bin/env node
"use strict";

// Finite, source-pinned measurements. This script is an experiment, not a CI
// correctness gate. The raw report belongs outside the repository.
const fs = require("node:fs");
const crypto = require("node:crypto");
const os = require("node:os");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const { performance } = require("node:perf_hooks");
const { FrozenClientSource, GameAdapter } = require("../../../oracle/game-adapter/src");
const { OracleRuntime } = require("../../../oracle/game-adapter/src/game-adapter");
const contract = require("../../../contracts/tools/runtime-contract").createRuntimeContract({ baseline: "site-20260928" });

const EXPECTED_CLIENT_SHA256 = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
const STYLES = ["normal", "chaos", "grand"];
const NUMBER_OPTIONS = new Set(["samples", "cold-samples"]);
const STRING_OPTIONS = new Set(["source-root", "output", "label", "style", "compare", "stage"]);

function usage() {
  return "Usage: node tests/site-adapter/bench/performance.cjs [--source-root ABSOLUTE_PATH] [--stage source|all] [--style normal|chaos|grand|all] [--samples 20] [--cold-samples 5] [--label before|after] [--output ABSOLUTE_PATH] [--compare ABSOLUTE_PATH]";
}

function options(argv) {
  const parsed = { stage: "all", style: "all", samples: 20, "cold-samples": 5, label: "baseline" };
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    if (!flag?.startsWith("--") || index + 1 >= argv.length) throw new TypeError(usage());
    const key = flag.slice(2);
    if (!NUMBER_OPTIONS.has(key) && !STRING_OPTIONS.has(key)) throw new TypeError(`Unknown option ${flag}. ${usage()}`);
    if (Object.hasOwn(parsed, key) && !["samples", "cold-samples", "label", "style", "stage"].includes(key)) throw new TypeError(`Duplicate option ${flag}.`);
    parsed[key] = NUMBER_OPTIONS.has(key) ? Number(argv[index + 1]) : argv[index + 1];
  }
  if (!Number.isSafeInteger(parsed.samples) || parsed.samples < 1 || parsed.samples > 1000) throw new TypeError("samples must be 1..1000.");
  if (!Number.isSafeInteger(parsed["cold-samples"]) || parsed["cold-samples"] < 1 || parsed["cold-samples"] > 100) throw new TypeError("cold-samples must be 1..100.");
  if (parsed.style !== "all" && !STYLES.includes(parsed.style)) throw new TypeError("Unknown game style.");
  if (!["source", "all"].includes(parsed.stage)) throw new TypeError("stage must be source or all.");
  if (!/^[a-z0-9][a-z0-9_-]{0,31}$/i.test(parsed.label)) throw new TypeError("label must be a short filename-safe token.");
  for (const key of ["source-root", "output", "compare"]) {
    if (parsed[key] && !path.isAbsolute(parsed[key])) throw new TypeError(`${key} must be an absolute path.`);
  }
  return parsed;
}

function externalRoot(kind) {
  const parent = process.env.RUNNER_TEMP || process.env.APPDATA;
  if (!parent) throw new Error("RUNNER_TEMP or APPDATA is required for site adapter measurements.");
  return path.join(parent, "Accelerate", kind, "site-adapter");
}

function percentile(values, fraction) {
  if (!values.length) throw new TypeError("Cannot calculate an empty percentile.");
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil(fraction * sorted.length) - 1];
}

function memory() {
  const current = process.memoryUsage();
  return { rssBytes: current.rss, heapUsedBytes: current.heapUsed, externalBytes: current.external,
    processMaxRssKiB: process.resourceUsage().maxRSS };
}

let resultSink = 0;
function consume(value) {
  if (value === undefined || value === null) throw new Error("The measured operation returned no value.");
  resultSink += typeof value === "object" ? Object.keys(value).length : 1;
  return value;
}

function measure(name, samples, operation, { warmups = 1 } = {}) {
  for (let index = 0; index < warmups; index++) consume(operation());
  if (global.gc) global.gc();
  const before = memory();
  const durationsMs = [];
  let observedRssBytes = before.rssBytes;
  for (let index = 0; index < samples; index++) {
    const start = performance.now();
    consume(operation());
    durationsMs.push(performance.now() - start);
    observedRssBytes = Math.max(observedRssBytes, process.memoryUsage().rss);
  }
  const after = memory();
  return { name, samples, p50Ms: percentile(durationsMs, 0.5), p95Ms: percentile(durationsMs, 0.95),
    minMs: Math.min(...durationsMs), maxMs: Math.max(...durationsMs),
    meanMs: durationsMs.reduce((total, value) => total + value, 0) / samples,
    memory: { before, after, observedRssBytes }, durationsMs };
}

function sourceFile(manifest) {
  const entries = manifest?.files?.filter(file => /^main-[A-Za-z0-9_-]+\.js$/.test(file.name)) || [];
  if (entries.length !== 1) throw new Error("Exactly one frozen main client is required.");
  return entries[0];
}

function adapter(source) {
  return new GameAdapter({ source, contract, maxCandidates: 100000, maxMicrotasks: 256 });
}

function measureStyle(source, style, samples) {
  const oracle = adapter(source);
  const runtime = new OracleRuntime({ source, contract, maxCandidates: 100000, maxMicrotasks: 256 });
  const config = { gameStyle: style, draftDelete: true };
  const position = oracle.newGame(config, 37);
  if (position.state.mode !== "play") throw new Error(`${style}: the fixture did not reach play.`);
  const rawCandidates = runtime.candidates(position);
  if (!rawCandidates.length) throw new Error(`${style}: no initial source candidate was available.`);
  const available = oracle.actions(position);
  if (!available.length) throw new Error(`${style}: no initial legal action was available.`);
  const chosen = available[0];
  const applied = oracle.apply(position, chosen);
  if (!applied.ok || applied.position.history.length !== 1) throw new Error(`${style}: history fixture failed.`);
  const draftPosition = oracle.newGame({ gameStyle: style }, 37);
  if (draftPosition.state.mode !== "draft") throw new Error(`${style}: the fixture did not reach draft.`);
  const draftCandidateCount = oracle.actions(draftPosition).length;

  const scenarios = [
    ["newGame", () => oracle.newGame(config, 37)],
    ["rawCandidates", () => runtime.candidates(position)],
    ["candidatePage20", () => oracle.actionStream(position, { legal: false }).nextPage(20, { maxExamined: 256 })],
    ["draftCandidatePage20", () => oracle.actionStream(draftPosition).nextPage(20, { maxExamined: 256 })],
    ["legalActions", () => oracle.actions(position)],
    ["applyNoHistory", () => {
      const step = oracle.apply(position, chosen, { recordHistory: false });
      if (!step.ok) throw new Error("Expected accepted action without history.");
      return step.position;
    }],
    ["applyWithHistory", () => {
      const step = oracle.apply(position, chosen);
      if (!step.ok || step.position.history.length !== 1) throw new Error("Expected accepted action with history.");
      return step.position;
    }],
    ["observeWhite", () => oracle.observe(position, "white")],
    ["observeBlack", () => oracle.observe(position, "black")],
    ["publicHints", () => oracle.publicHints(position, "white")],
    ["observeWithHistory", () => oracle.observe(applied.position, "white")],
    ["restore", () => { runtime.restore(position); return 1; }],
    ["snapshot", () => runtime.snapshot()],
  ];
  return {
    style, fixture: { initialPositionId: position.positionId, firstActionId: chosen.actionId,
      afterPositionId: applied.position.positionId, initialMode: position.state.mode,
      rawCandidateCount: rawCandidates.length, legalActionCount: available.length,
      draftPositionId: draftPosition.positionId, draftCandidateCount },
    phases: scenarios.map(([name, operation]) => {
      if (name === "snapshot") runtime.restore(position);
      return measure(name, samples, operation);
    }),
  };
}

function currentRevision() {
  try {
    return execFileSync("git", ["rev-parse", "HEAD"], { cwd: path.resolve(__dirname, "../../../../.."), encoding: "utf8" }).trim();
  } catch {
    return null;
  }
}

function implementationHashes() {
  const projectRoot = path.resolve(__dirname, "../../../../..");
  const files = [
    "projects/augment-chess/oracle/game-adapter/src/index.js",
    "projects/augment-chess/oracle/game-adapter/src/game-adapter.js",
    "projects/augment-chess/oracle/game-adapter/src/frozen-client-source.js",
    "projects/augment-chess/oracle/game-adapter/src/client-enumeration.js",
    "projects/augment-chess/contracts/tools/runtime-contract.js",
  ];
  return Object.fromEntries(files.map(file => [file,
    crypto.createHash("sha256").update(fs.readFileSync(path.join(projectRoot, file))).digest("hex")]));
}

function comparable(before, after) {
  const keys = ["stage", "clientSha256", "parserSha256", "rulesVersion", "catalogVersion", "profileVersion",
    "node", "platform", "arch", "cpuModel", "cpus", "gcExposed", "samples", "coldSamples", "styles"];
  for (const key of keys) {
    if (JSON.stringify(before[key]) !== JSON.stringify(after[key])) throw new Error(`Cannot compare reports with different ${key}.`);
  }
  for (let index = 0; index < after.scenarios.length; index++) {
    if (JSON.stringify(before.scenarios[index]?.fixture) !== JSON.stringify(after.scenarios[index]?.fixture)) {
      throw new Error(`Cannot compare reports with different ${after.scenarios[index].style} fixtures.`);
    }
  }
}

function compare(before, after) {
  comparable(before, after);
  const phases = [["cold", before.cold, after.cold], ["runtime", before.runtime, after.runtime]];
  if (before.construction && after.construction) phases.push(["construction", before.construction, after.construction]);
  const global = phases.map(([scope, base, phase]) => {
    if (base.name !== phase.name) throw new Error(`Global phase ${scope} differs between reports.`);
    return { style: null, phase: phase.name, beforeP50Ms: base.p50Ms, afterP50Ms: phase.p50Ms,
      p50Ratio: base.p50Ms / phase.p50Ms, beforeP95Ms: base.p95Ms, afterP95Ms: phase.p95Ms,
      p95Ratio: base.p95Ms / phase.p95Ms };
  });
  const styles = after.scenarios.flatMap((scenario, scenarioIndex) => scenario.phases.map((phase, phaseIndex) => {
    const base = before.scenarios[scenarioIndex].phases[phaseIndex];
    if (base.name !== phase.name) throw new Error("Phase order differs between reports.");
    return { style: scenario.style, phase: phase.name, beforeP50Ms: base.p50Ms, afterP50Ms: phase.p50Ms,
      p50Ratio: base.p50Ms / phase.p50Ms, beforeP95Ms: base.p95Ms, afterP95Ms: phase.p95Ms,
      p95Ratio: base.p95Ms / phase.p95Ms };
  }));
  return [...global, ...styles];
}

function run(argv = process.argv.slice(2)) {
  const selected = options(argv);
  const root = selected["source-root"] || process.env.ACCELERATE_SITE_BASELINE;
  if (!root || !path.isAbsolute(root)) throw new Error("Pass an absolute --source-root or ACCELERATE_SITE_BASELINE.");
  const output = selected.output || path.join(externalRoot("reports"), `performance-${selected.label}.json`);
  const start = new Date().toISOString();
  const source = new FrozenClientSource(root, { expectedClientSha256: EXPECTED_CLIENT_SHA256 });
  const main = sourceFile(source.manifest);
  if (main.sha256 !== EXPECTED_CLIENT_SHA256) throw new Error(`Expected the adopted client ${EXPECTED_CLIENT_SHA256}; got ${main.sha256}.`);
  const parser = source.manifest.files.find(file => /^acorn-/.test(file.name));
  const cold = measure("sourceVerifyParseCompile", selected["cold-samples"], () => new FrozenClientSource(root, { expectedClientSha256: EXPECTED_CLIENT_SHA256 }), { warmups: 0 });
  const runtime = measure("createRuntime", selected.samples, () => source.createRuntime());
  const construction = selected.stage === "all" ? measure("adapterConstruction", selected.samples, () => adapter(source)) : null;
  const styles = selected.stage === "all" ? selected.style === "all" ? STYLES : [selected.style] : [];
  const cpu = os.cpus();
  const report = {
    schemaVersion: 1, stage: selected.stage, label: selected.label, startedAt: start, finishedAt: new Date().toISOString(),
    node: process.version, platform: process.platform, arch: process.arch, cpuModel: cpu[0]?.model || null,
    cpus: cpu.length, gcExposed: Boolean(global.gc),
    gitRevision: currentRevision(), implementationHashes: implementationHashes(),
    clientName: main.name, clientSha256: main.sha256,
    parserSha256: parser?.sha256 || null,
    // A source-only run does not instantiate the game runtime. Do not attach
    // whichever contract happens to be in this checkout to that measurement.
    rulesVersion: selected.stage === "all" ? contract.catalog.rulesVersion : null,
    catalogVersion: selected.stage === "all" ? contract.catalog.catalogVersion : null,
    profileVersion: selected.stage === "all" ? contract.ORACLE_PROFILE_VERSION : null,
    samples: selected.samples, coldSamples: selected["cold-samples"], styles,
    cold, runtime, construction, scenarios: styles.map(style => measureStyle(source, style, selected.samples)),
    processMemory: memory(), sink: resultSink,
  };
  const comparison = selected.compare ? compare(JSON.parse(fs.readFileSync(selected.compare, "utf8")), report) : null;
  const summary = {
    clientSha256: report.clientSha256, rulesVersion: report.rulesVersion, samples: report.samples,
    coldSamples: report.coldSamples, styles: report.styles,
    coldP50Ms: cold.p50Ms, runtimeP50Ms: runtime.p50Ms, constructionP50Ms: construction?.p50Ms || null,
    phases: report.scenarios.map(scenario => ({ style: scenario.style,
      p50Ms: Object.fromEntries(scenario.phases.map(phase => [phase.name, phase.p50Ms])),
      p95Ms: Object.fromEntries(scenario.phases.map(phase => [phase.name, phase.p95Ms])) })),
    processMaxRssKiB: report.processMemory.processMaxRssKiB,
  };
  if (comparison) summary.comparison = comparison;
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, JSON.stringify(report, null, 2) + "\n");
  process.stdout.write(JSON.stringify(summary, null, 2) + "\n");
  return report;
}

if (require.main === module) {
  try { run(); }
  catch (error) { console.error(`${error.name}: ${error.message}`); process.exitCode = 1; }
}

module.exports = { run, options, percentile, comparable, compare };
