"use strict";

// Deliberately an explicit runner, not a node --test file: a missing native
// wheel or a partial v7 engine must fail its own gate instead of being skipped.
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { FrozenClientSource, GameAdapter } = require("../../packages/game-adapter/src");
const { createRuntimeContract } = require("../../bridge/tools/runtime-contract");

const SOURCE_SHA256 = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
const PROFILE = "accelerate-headless-semantic-v7";
const PAGE_SIZE = 64;
const MAX_PAGES = 128;
const MAX_ACTIONS = 4096;
const MAX_EXAMINED = 65536;
const MAX_BATCH_BYTES = 16 * 1024 * 1024;
const STYLES = ["normal", "chaos", "grand"];

function options(argv) {
  const selected = { python: process.env.PYTHON || "python", source: null, oracleOnly: false };
  for (let index = 0; index < argv.length; index++) {
    if (argv[index] === "--python" && argv[index + 1]) selected.python = argv[++index];
    else if (argv[index] === "--source" && argv[index + 1]) selected.source = argv[++index];
    else if (argv[index] === "--oracle-only") selected.oracleOnly = true;
    else throw new TypeError("Usage: node tests/differential/v7-native-differential.cjs [--python PYTHON] [--source ABSOLUTE_PINNED_ROOT] [--oracle-only]");
  }
  return selected;
}

function sourceActions(adapter, position) {
  const cursor = adapter.actionStream(position);
  const actions = [];
  let examined = 0;
  try {
    for (let pageIndex = 0; pageIndex < MAX_PAGES; pageIndex++) {
      const page = cursor.nextPage(PAGE_SIZE, { maxExamined: 4096 });
      actions.push(...page.actions);
      examined += page.examined;
      if (actions.length > MAX_ACTIONS || examined > MAX_EXAMINED)
        throw new Error("source full legal stream exceeded probe budget; no prefix is accepted");
      if (page.exhausted) {
        if (new Set(actions.map(action => action.actionId)).size !== actions.length)
          throw new Error("source full legal stream emitted duplicate action identities");
        return { actions, examined };
      }
      if (!page.actions.length && page.examined === 0)
        throw new Error("source full legal stream made no progress");
    }
    throw new Error("source full legal stream did not exhaust within page budget");
  } finally {
    cursor.dispose();
  }
}

function buildCase(adapter, contract, name, position) {
  contract.validatePosition(position);
  const { actions, examined } = sourceActions(adapter, position);
  if (!actions.length) throw new Error(`${name}: source has no action for reject/apply probes`);
  const first = actions[0];
  const rejectPayload = contract.jsonCopy({ ...first.payload, color: first.payload.color === "white" ? "black" : "white" });
  const rejected = adapter.apply(position, contract.action(position, rejectPayload));
  if (rejected.ok || contract.canonical(rejected.position) !== contract.canonical(position) ||
      contract.canonical(rejected.result) !== contract.canonical(adapter.result(position)))
    throw new Error(`${name}: source wrong-actor rejection changed the full position or result`);
  const samples = [];
  for (const action of [actions[0], actions[actions.length - 1]]) {
    if (samples.some(sample => sample.action.actionId === action.actionId)) continue;
    const step = adapter.apply(position, action, { recordHistory: true });
    if (!step.ok || step.position.history.length !== position.history.length + 1)
      throw new Error(`${name}: source sample failed full-history apply`);
    contract.validatePosition(step.position);
    contract.validateResult(step.result);
    samples.push({ action, position: step.position, result: step.result });
  }
  const result = adapter.result(position);
  contract.validateResult(result);
  const actionTypes = [...new Set(actions.map(action => action.payload.type))].sort();
  return {
    input: { name, position, result, actions, rejectPayload, samples },
    summary: { name, mode: position.state.mode, legalCount: actions.length, sourceExamined: examined, actionTypes, sampleCount: samples.length, rejectCount: 1 },
  };
}

function sourceCases(source, contract) {
  const cases = [];
  for (const style of STYLES) {
    const adapter = new GameAdapter({ source, contract });
    try {
      let position = adapter.newGame({ gameStyle: style }, 37);
      cases.push(buildCase(adapter, contract, `${style}-draft`, position));
      let picks = 0;
      while (position.state.mode === "draft" && picks < 64) {
        const first = adapter.actions(position)[0];
        if (!first) throw new Error(`${style}: draft has no legal choice`);
        const step = adapter.apply(position, first, { recordHistory: false });
        if (!step.ok) throw new Error(`${style}: source rejected its own draft choice`);
        position = step.position;
        picks++;
      }
      if (position.state.mode !== "play")
        throw new Error(`${style}: source draft did not reach play within 64 choices`);
      cases.push(buildCase(adapter, contract, `${style}-play`, position));
      cases[cases.length - 1].summary.draftChoicesToPlay = picks;
    } finally {
      adapter.dispose();
    }
  }
  return cases;
}

function nativeProbe(python, request) {
  const input = JSON.stringify(request);
  if (Buffer.byteLength(input) > MAX_BATCH_BYTES)
    throw new Error("v7 native probe batch exceeds 16 MiB; narrow the generated scenarios");
  const child = spawnSync(python, [path.join(__dirname, "v7-native-probe.py")], {
    input, encoding: "utf8", timeout: 60000, maxBuffer: 4 * 1024 * 1024,
    windowsHide: true,
  });
  if (child.error) return { status: child.error.code === "ETIMEDOUT" ? "native-timeout" : "native-unavailable",
    reason: `${child.error.name}: ${child.error.message}` };
  if (child.status !== 0) return { status: "probe-error", reason: `native worker exit ${child.status}: ${(child.stderr || "").trim().slice(0, 500)}` };
  try { return JSON.parse(child.stdout); }
  catch { return { status: "probe-error", reason: "native worker did not return one JSON response" }; }
}

function main() {
  const parent = process.env.RUNNER_TEMP || process.env.APPDATA;
  if (!parent || !path.isAbsolute(parent)) throw new Error("APPDATA or RUNNER_TEMP must be an absolute report root");
  const reportPath = path.join(parent, "Accelerate", "reports", "v7-native-differential", "report.json");
  const report = {
    gate: "source-pinned-v7-native-differential-probe",
    status: "setup-error",
    generatedAt: new Date().toISOString(),
    scope: "normal/chaos/grand initial draft and first play after deterministic source draft; sampled first/last applies; not complete rule coverage or project GO",
    completeRuleCoverage: false,
    bounds: { pageSize: PAGE_SIZE, maxPages: MAX_PAGES, maxActions: MAX_ACTIONS, maxExamined: MAX_EXAMINED, maxDraftChoices: 64, maxSamplesPerPosition: 2 },
  };
  try {
    const args = options(process.argv.slice(2));
    const sourceRoot = args.source || process.env.ACCELERATE_SITE_BASELINE_LATEST || process.env.ACCELERATE_SITE_BASELINE ||
      path.join(parent, "Accelerate", "cache", "site-baseline-20260928-e5ed84fc");
    if (!path.isAbsolute(sourceRoot)) throw new Error("pinned source root must be absolute");
    const contract = createRuntimeContract({ baseline: "site-20260928" });
    if (contract.ORACLE_PROFILE_VERSION !== PROFILE || contract.catalog.source.files.find(file => /^main-/.test(file.name))?.sha256 !== SOURCE_SHA256)
      throw new Error("v7 contract source/profile identity mismatch");
    const source = new FrozenClientSource(sourceRoot, { expectedClientSha256: SOURCE_SHA256 });
    report.source = { root: sourceRoot, sha256: SOURCE_SHA256, profile: PROFILE, rulesVersion: contract.catalog.rulesVersion,
      catalogVersion: contract.catalog.catalogVersion, observationPolicyHash: contract.digest(contract.observationPolicy) };
    const cases = sourceCases(source, contract);
    report.sourceCases = cases.map(item => item.summary);
    report.coverage = {
      styles: STYLES,
      modes: [...new Set(cases.map(item => item.summary.mode))],
      actionTypes: [...new Set(cases.flatMap(item => item.summary.actionTypes))].sort(),
      resultStatuses: [...new Set(cases.map(item => item.input.result.status))].sort(),
    };
    const preflight = args.oracleOnly ? { status: "oracle-only", reason: "native comparison explicitly omitted" } :
      nativeProbe(args.python, { phase: "preflight", rulesVersion: contract.catalog.rulesVersion, catalogVersion: contract.catalog.catalogVersion });
    report.nativePreflight = preflight;
    if (args.oracleOnly || preflight.status !== "ready") report.status = preflight.status;
    else {
      const comparison = nativeProbe(args.python, { phase: "compare", rulesVersion: contract.catalog.rulesVersion,
        catalogVersion: contract.catalog.catalogVersion, cases: cases.map(item => item.input) });
      report.native = comparison;
      report.status = comparison.status === "pass" ? "pass" : comparison.status;
    }
  } catch (error) {
    report.status = "setup-error";
    report.reason = `${error.name}: ${error.message}`;
  }
  fs.mkdirSync(path.dirname(reportPath), { recursive: true });
  fs.writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ status: report.status, report: reportPath, sourceCases: report.sourceCases?.length || 0,
    nativeCases: report.native?.cases?.length || 0, reason: report.reason || report.nativePreflight?.reason || undefined }));
  if (report.status !== "pass") process.exitCode = 1;
}

main();
