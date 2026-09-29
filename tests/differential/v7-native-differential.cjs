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
const NO_CASE_FAILURE_STATUSES = new Set([
  "native-timeout", "native-unavailable", "native-unsupported", "probe-error", "version-mismatch",
]);
// Three small assertions extracted from the external seed19-active-only source
// manifest. The complete source positions stay outside Git and are regenerated.
const ACTIVE_SEED19 = Object.freeze({
  normal: { positionDigest: "08ecf6a04a6ddaa277e646ce24e863d0aecd2f10badf3f6c379b715421f59453", legalCount: 21 },
  chaos: { positionDigest: "5c98166c0007febab1e00965bc1241bb4530088aa1e34367bf727703c6079108", legalCount: 22 },
  grand: { positionDigest: "a7bfd8e2bae424bf59f742813312bf72effc7634c294f6874e8787891f379645", legalCount: 36 },
});

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
  const observations = Object.fromEntries(["white", "black"].map(viewer => {
    const observation = adapter.observe(position, viewer);
    contract.validateObservation(observation);
    return [viewer, observation];
  }));
  const first = actions[0];
  const rejectPayload = contract.jsonCopy({ ...first.payload, color: first.payload.color === "white" ? "black" : "white" });
  const rejected = adapter.apply(position, contract.action(position, rejectPayload));
  if (rejected.ok || contract.canonical(rejected.position) !== contract.canonical(position) ||
      contract.canonical(rejected.result) !== contract.canonical(adapter.result(position)))
    throw new Error(`${name}: source wrong-actor rejection changed the full position or result`);
  const samples = [];
  const firstCard = actions.find(action => action.payload.type === "card");
  for (const action of [actions[0], actions[actions.length - 1], firstCard].filter(Boolean)) {
    if (samples.some(sample => sample.action.actionId === action.actionId)) continue;
    const step = adapter.apply(position, action, { recordHistory: true });
    if (!step.ok || step.position.history.length !== position.history.length + 1)
      throw new Error(`${name}: source sample failed full-history apply`);
    contract.validatePosition(step.position);
    contract.validateResult(step.result);
    if (step.position.positionId === action.positionId)
      throw new Error(`${name}: source sample retained the old Position identity`);
    let staleRejected = false;
    try { adapter.apply(step.position, action); }
    catch (error) {
      if (!(error instanceof TypeError) || !/Stale or incompatible action/.test(error.message)) throw error;
      staleRejected = true;
    }
    if (!staleRejected) throw new Error(`${name}: source accepted a stale action`);
    const nextObservations = Object.fromEntries(["white", "black"].map(viewer => {
      const observation = adapter.observe(step.position, viewer);
      contract.validateObservation(observation);
      return [viewer, observation];
    }));
    samples.push({ action, position: step.position, result: step.result, observations: nextObservations });
  }
  const result = adapter.result(position);
  contract.validateResult(result);
  const actionTypes = [...new Set(actions.map(action => action.payload.type))].sort();
  return {
    input: { name, position, result, observations, actions, rejectPayload, samples },
    summary: { name, mode: position.state.mode, positionDigest: contract.digest(position), legalCount: actions.length,
      sourceExamined: examined, actionTypes, sampleTypes: samples.map(sample => sample.action.payload.type),
      sampleCount: samples.length, rejectCount: 1, staleRejectCount: samples.length,
      observationViewers: Object.keys(observations) },
  };
}

function firstActiveDraftAction(adapter, contract, position) {
  const activation = new Map(contract.catalog.cards.map(card => [card.id, card.activation]));
  const offered = new Map(position.state.draft.choices.map(card => [card.instanceId, card.id]));
  return adapter.actions(position).find(action => {
    const payload = action.payload;
    const ids = payload.type === "draftPick" ? [payload.cardInstanceId] :
      payload.type === "draftBundlePick" ? payload.cardInstanceIds : [];
    return ids.length > 0 && ids.every(instanceId => activation.get(offered.get(instanceId)) === "ACTIVE");
  });
}

function sourceCases(source, contract) {
  const cases = [];
  for (const style of STYLES) {
    const adapter = new GameAdapter({ source, contract });
    try {
      for (const { seed, policy } of [{ seed: 37, policy: "first" }, { seed: 19, policy: "first-active" }]) {
        let position = adapter.newGame({ gameStyle: style }, seed);
        if (seed === 37) cases.push(buildCase(adapter, contract, `${style}-seed37-draft`, position));
        let picks = 0;
        while (position.state.mode === "draft" && picks < 64) {
          const choice = policy === "first-active" ? firstActiveDraftAction(adapter, contract, position) : adapter.actions(position)[0];
          if (!choice) throw new Error(`${style} seed ${seed}: draft has no ${policy} legal choice`);
          const step = adapter.apply(position, choice, { recordHistory: false });
          if (!step.ok) throw new Error(`${style} seed ${seed}: source rejected its own draft choice`);
          position = step.position;
          picks++;
        }
        if (position.state.mode !== "play")
          throw new Error(`${style} seed ${seed}: source draft did not reach play within 64 choices`);
        const name = `${style}-seed${seed}-${policy}-play`;
        const item = buildCase(adapter, contract, name, position);
        item.summary.draftChoicesToPlay = picks;
        item.summary.seed = seed;
        item.summary.draftPolicy = policy;
        if (seed === 19) {
          const expected = ACTIVE_SEED19[style];
          if (item.summary.positionDigest !== expected.positionDigest || item.summary.legalCount !== expected.legalCount)
            throw new Error(`${name}: regenerated source differs from pinned external active-only manifest`);
        }
        cases.push(item);
      }
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
    scope: "normal/chaos/grand seed37 draft/first play and seed19 first-ACTIVE play; full legal stream, both public viewers, sampled draft/move/card applies; not complete rule or terminal coverage",
    completeRuleCoverage: false,
    projectGo: false,
    bounds: { pageSize: PAGE_SIZE, maxPages: MAX_PAGES, maxActions: MAX_ACTIONS, maxExamined: MAX_EXAMINED, maxDraftChoices: 64, maxSamplesPerPosition: 3 },
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
    const identity = { rulesVersion: contract.catalog.rulesVersion, catalogVersion: contract.catalog.catalogVersion,
      sourceSha256: SOURCE_SHA256, profile: PROFILE, observationPolicy: contract.observationPolicy };
    const preflight = args.oracleOnly ? { status: "oracle-only", reason: "native comparison explicitly omitted" } :
      nativeProbe(args.python, { phase: "preflight", ...identity });
    report.nativePreflight = preflight;
    if (args.oracleOnly || preflight.status !== "ready") report.status = preflight.status;
    else {
      const comparison = nativeProbe(args.python, { phase: "compare", ...identity, cases: cases.map(item => item.input) });
      report.native = comparison;
      if (!Array.isArray(comparison?.cases) && NO_CASE_FAILURE_STATUSES.has(comparison?.status) &&
          typeof comparison.reason === "string" && comparison.reason.trim()) {
        report.status = comparison.status;
        report.reason = comparison.reason;
      } else if (!comparison || !Array.isArray(comparison.cases) || comparison.cases.length !== cases.length ||
          comparison.cases.some((item, index) => item?.name !== cases[index].input.name)) {
        report.status = "probe-error";
        report.reason = "native comparison returned missing, extra, or reordered case results";
      } else if (comparison.status === "pass" && comparison.cases.some(item => item.status !== "pass")) {
        report.status = "probe-error";
        report.reason = "native comparison reported pass with a nonpassing case";
      } else if (!["pass", "fail"].includes(comparison.status)) {
        report.status = "probe-error";
        report.reason = "native comparison returned an invalid aggregate status";
      } else report.status = comparison.status;
    }
  } catch (error) {
    report.status = "setup-error";
    report.reason = `${error.name}: ${error.message}`;
  }
  report.decision = report.status === "pass" ? "bounded-P8-probe-pass-only" : "NO-GO";
  fs.mkdirSync(path.dirname(reportPath), { recursive: true });
  fs.writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ status: report.status, decision: report.decision, report: reportPath,
    sourceCases: report.sourceCases?.length || 0, nativeCases: report.native?.cases?.length || 0,
    reason: report.reason || report.nativePreflight?.reason || report.native?.cases?.find(item => item.status !== "pass")?.reason || undefined }));
  if (report.status !== "pass") process.exitCode = 1;
}

main();
