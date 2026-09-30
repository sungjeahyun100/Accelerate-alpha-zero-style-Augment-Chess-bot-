"use strict";

// Deliberately an explicit runner, not a node --test file: a missing native
// wheel or a partial v7 engine must fail its own gate instead of being skipped.
const fs = require("node:fs");
const path = require("node:path");
const { createHash } = require("node:crypto");
const { spawnSync } = require("node:child_process");
const { FrozenClientSource, GameAdapter } = require("../../oracle/game-adapter/src");
const { OracleRuntime } = require("../../oracle/game-adapter/src/game-adapter");
const { createRuntimeContract } = require("../../contracts/tools/runtime-contract");

const SOURCE_SHA256 = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
const PROFILE = "accelerate-headless-semantic-v7-faithful-init-v1";
const PAGE_SIZE = 64;
const MAX_PAGES = 128;
const MAX_ACTIONS = 4096;
const MAX_EXAMINED = 65536;
const MAX_BATCH_BYTES = 16 * 1024 * 1024;
const MAX_BATCH_CASES = 8;
const MAX_PLAYOUT_DECISIONS = 128;
const STYLES = ["normal", "chaos", "grand"];
// Three small assertions regenerated from the fully initialized pinned client.
// The complete source positions stay outside Git and are regenerated.
const ACTIVE_SEED19 = Object.freeze({
  normal: { positionDigest: "611b6e82856b84768ef2de8d88ecf9e7f9e6b14226688de37dfd350590a1538c", legalCount: 21 },
  chaos: { positionDigest: "f1064080e694b4e13e679666c7cfc2b618e80668d69f793b3bb3696eff5e1623", legalCount: 22 },
  grand: { positionDigest: "c2ff9d98b14ea137138898e382fd151449e1c1b81b122e28435f867580f00bdf", legalCount: 36 },
});

function options(argv) {
  const selected = { python: process.env.PYTHON || "python", source: null, oracleOnly: false, exportCases: false, playouts: [] };
  for (let index = 0; index < argv.length; index++) {
    if (argv[index] === "--python" && argv[index + 1]) selected.python = argv[++index];
    else if (argv[index] === "--source" && argv[index + 1]) selected.source = argv[++index];
    else if (argv[index] === "--oracle-only") selected.oracleOnly = true;
    else if (argv[index] === "--export-cases") selected.exportCases = true;
    else if (argv[index].startsWith("--playout=")) {
      const match = /^--playout=(normal|chaos|grand):(\d+):(\d+)$/.exec(argv[index]);
      if (!match || !Number.isSafeInteger(Number(match[2])) || Number(match[3]) > MAX_PLAYOUT_DECISIONS || Number(match[3]) < 1)
        throw new TypeError("Use --playout=STYLE:SEED:DECISIONS with 1..128 decisions.");
      selected.playouts.push({ style: match[1], seed: Number(match[2]), decisions: Number(match[3]) });
      if (selected.playouts.length > 18) throw new TypeError("At most 18 playouts per invocation; shard additional runs.");
    } else throw new TypeError("Usage: node projects/augment-chess/tests/differential/v7-native-differential.cjs [--python PYTHON] [--source ABSOLUTE_PINNED_ROOT] [--oracle-only] [--export-cases] [--playout=STYLE:SEED:DECISIONS]...");
  }
  if (new Set(selected.playouts.map(item => `${item.style}:${item.seed}`)).size !== selected.playouts.length)
    throw new TypeError("Duplicate style/seed playout.");
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

function buildCase(adapter, contract, name, position, requiredSampleId = null) {
  contract.validatePosition(position);
  const { actions, examined } = sourceActions(adapter, position);
  if (!actions.length && position.state.mode !== "gameover") throw new Error(`${name}: nonterminal source has no action for reject/apply probes`);
  const observations = Object.fromEntries(["white", "black"].map(viewer => {
    const observation = adapter.observe(position, viewer);
    contract.validateObservation(observation);
    return [viewer, observation];
  }));
  const first = actions[0];
  const rejectPayload = first ? contract.jsonCopy({ ...first.payload, color: first.payload.color === "white" ? "black" : "white" }) : null;
  if (rejectPayload) {
    const rejected = adapter.apply(position, contract.action(position, rejectPayload));
    if (rejected.ok || contract.canonical(rejected.position) !== contract.canonical(position) ||
        contract.canonical(rejected.result) !== contract.canonical(adapter.result(position)))
      throw new Error(`${name}: source wrong-actor rejection changed the full position or result`);
  }
  const samples = [];
  const firstCard = actions.find(action => action.payload.type === "card");
  for (const action of [first, actions[actions.length - 1], firstCard,
    actions.find(action => action.actionId === requiredSampleId)].filter(Boolean)) {
    if (samples.some(sample => sample.action.actionId === action.actionId)) continue;
    const step = adapter.apply(position, action, { recordHistory: true });
    if (!step.ok || step.position.history.length !== position.history.length + 1)
      throw new Error(`${name}: source sample failed full-history apply`);
    contract.validatePosition(step.position);
    contract.validateResult(step.result);
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
      sampleCount: samples.length, rejectCount: rejectPayload ? 1 : 0, observationViewers: Object.keys(observations),
      resultStatus: result.status },
  };
}

function* sourcePlayoutCases(source, contract, { style, seed, decisions }) {
  const adapter = new GameAdapter({ source, contract });
  try {
    let position = adapter.newGame({ gameStyle: style }, seed);
    for (let pick = 0; position.state.mode === "draft" && pick < 64; pick++) {
      const choice = adapter.actions(position)[0];
      if (!choice) throw new Error(`${style} seed ${seed}: source draft has no action`);
      const step = adapter.apply(position, choice, { recordHistory: false });
      if (!step.ok) throw new Error(`${style} seed ${seed}: source rejected own draft choice`);
      position = step.position;
    }
    if (position.state.mode !== "play") throw new Error(`${style} seed ${seed}: source draft did not reach play in 64 picks`);
    for (let decision = 0; decision <= decisions; decision++) {
      const item = buildCase(adapter, contract, `${style}-seed${seed}-playout-${decision}`, position);
      item.summary.playoutDecision = decision;
      item.summary.seed = seed;
      item.summary.style = style;
      yield item;
      if (position.state.mode === "gameover" || decision === decisions) break;
      const actions = item.input.actions;
      // A fixed policy exercises cards when available, while otherwise moving
      // through actual source legal actions. It never invents or patches state.
      const preferredType = decision % 4 === 0 ? "card" : "move";
      const preferred = actions.filter(action => action.payload.type === preferredType);
      const pool = preferred.length ? preferred : actions;
      const choice = pool[Math.floor(decision / 4) % pool.length];
      if (!choice) throw new Error(`${style} seed ${seed} decision ${decision}: no source action to advance`);
      const step = adapter.apply(position, choice, { recordHistory: false });
      if (!step.ok) throw new Error(`${style} seed ${seed} decision ${decision}: source rejected its own legal action`);
      position = step.position;
    }
  } finally { adapter.dispose(); }
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

// corpus와 manifest 생성기가 동일한 source draft 정책·경계를 사용한다.
function advanceInitialDraft(adapter, contract, initial, { style, seed, policy, onPick = null }) {
  if (!["first", "first-active"].includes(policy) || (onPick !== null && typeof onPick !== "function"))
    throw new TypeError("Expected a known initial draft policy and optional trace callback");
  let position = initial, picks = 0;
  while (position.state.mode === "draft" && picks < 64) {
    const choice = policy === "first-active" ? firstActiveDraftAction(adapter, contract, position) : adapter.actions(position)[0];
    if (!choice) throw new Error(`${style} seed ${seed}: draft has no ${policy} legal choice`);
    if (onPick) onPick(position, choice, picks);
    const step = adapter.apply(position, choice, { recordHistory: false });
    if (!step.ok) throw new Error(`${style} seed ${seed}: source rejected its own draft choice`);
    position = step.position;
    picks++;
  }
  if (position.state.mode !== "play")
    throw new Error(`${style} seed ${seed}: source draft did not reach play within 64 choices`);
  return { position, picks };
}

function sourceCases(source, contract) {
  const cases = [];
  for (const style of STYLES) {
    const adapter = new GameAdapter({ source, contract });
    try {
      for (const { seed, policy } of [{ seed: 37, policy: "first" }, { seed: 19, policy: "first-active" }]) {
        const initial = adapter.newGame({ gameStyle: style }, seed);
        if (seed === 37) cases.push(buildCase(adapter, contract, `${style}-seed37-draft`, initial));
        const { position, picks } = advanceInitialDraft(adapter, contract, initial, { style, seed, policy });
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
  cases.push(...syntheticTerminalCases(source, contract));
  cases.push(...syntheticTimedStatusCases(source, contract));
  return cases;
}

function syntheticTerminalCases(source, contract) {
  const cases = [];
  for (const scenario of [
    {
      name: "royal-capture",
      setup: "state.board=Array.from({length:8},()=>Array(8).fill(null));state.board[7][4]=piece('white','king');state.board[0][4]=piece('black','king');state.board[1][4]=piece('white','rook');",
      from: { row: 1, col: 4 }, to: { row: 0, col: 4 },
    },
    {
      name: "opponent-immobility",
      setup: "state.board=Array.from({length:8},()=>Array(8).fill(null));state.board[7][7]=piece('white','king');state.board[6][7]=piece('white','rook');state.board[0][0]=piece('black','king');for(const [r,c] of [[0,1],[1,0],[1,1]])state.board[r][c]=piece('neutral','wall');",
      from: { row: 6, col: 7 }, to: { row: 5, col: 7 },
    },
  ]) {
    const fixture = new OracleRuntime({ source, contract });
    const adapter = new GameAdapter({ source, contract });
    try {
      fixture.newGame({ draftDelete: true }, 7);
      fixture.evaluate(scenario.setup);
      const position = fixture.snapshot();
      const action = adapter.actions(position).find(candidate => {
        const payload = candidate.payload;
        return payload.type === "move" && payload.from.row === scenario.from.row &&
          payload.from.col === scenario.from.col && payload.move.row === scenario.to.row &&
          payload.move.col === scenario.to.col;
      });
      if (!action) throw new Error(`${scenario.name}: source terminal move is absent from legal actions`);
      const before = buildCase(adapter, contract, `${scenario.name}-before`, position, action.actionId);
      before.summary.syntheticSetup = true;
      before.summary.style = "normal";
      cases.push(before);
      const step = adapter.apply(position, action, { recordHistory: true });
      if (!step.ok || step.result.status !== "terminal" || step.result.winner !== "white")
        throw new Error(`${scenario.name}: source move did not produce the expected white terminal result`);
      const after = buildCase(adapter, contract, `${scenario.name}-after`, step.position);
      after.summary.syntheticSetup = true;
      after.summary.style = "normal";
      cases.push(after);
    } finally { adapter.dispose(); }
  }
  return cases;
}

function syntheticTimedStatusCases(source, contract) {
  const cases = [];
  for (const scenario of [
    { name: "disarmed", square: [6, 0], remaining: 2, afterSquare: [5, 0], afterRemaining: 1 },
    { name: "severed", square: [6, 0], remaining: 2, afterSquare: [5, 0], afterRemaining: 1 },
    { name: "iceSheet", square: [6, 1], remaining: 1, afterSquare: [6, 1], afterRemaining: null },
    { name: "staked", square: [6, 1], remaining: 1, afterSquare: [6, 1], afterRemaining: null, afterShielded: true },
  ]) {
    const fixture = new OracleRuntime({ source, contract });
    const adapter = new GameAdapter({ source, contract });
    try {
      fixture.newGame({ draftDelete: true }, 22);
      fixture.evaluate(`state.board[${scenario.square[0]}][${scenario.square[1]}].${scenario.name}={remaining:${scenario.remaining}};`);
      // snapshot() rebinds the mutated source state to a fresh Position ID.
      const position = fixture.snapshot();
      const action = adapter.actions(position).find(candidate => {
        const payload = candidate.payload;
        return payload.type === "move" && payload.from.row === 6 && payload.from.col === 0 &&
          payload.move.row === 5 && payload.move.col === 0;
      });
      if (!action) throw new Error(`${scenario.name}: source a2-a3 trigger is absent from legal actions`);
      const before = buildCase(adapter, contract, `${scenario.name}-tick-before`, position, action.actionId);
      before.summary.syntheticSetup = true;
      before.summary.style = "normal";
      before.summary.sourceStatus = scenario.name;
      cases.push(before);
      const step = adapter.apply(position, action, { recordHistory: true });
      if (!step.ok) throw new Error(`${scenario.name}: source rejected its own status tick trigger`);
      const piece = step.position.state.board[scenario.afterSquare[0]][scenario.afterSquare[1]];
      if (!piece || (piece[scenario.name]?.remaining ?? null) !== scenario.afterRemaining ||
          (scenario.afterShielded === true && piece.shielded !== true))
        throw new Error(`${scenario.name}: source status did not tick as expected`);
      const after = buildCase(adapter, contract, `${scenario.name}-tick-after`, step.position);
      after.summary.syntheticSetup = true;
      after.summary.style = "normal";
      after.summary.sourceStatus = scenario.name;
      cases.push(after);
    } finally { adapter.dispose(); }
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

function compareCases(python, identity, items, report, oracleOnly, exportCase = null) {
  const nativeCases = [];
  let batch = [];
  let bytes = Buffer.byteLength(JSON.stringify({ phase: "compare", ...identity, cases: [] }));
  let status = oracleOnly ? "oracle-only" : "pass";
  const flush = () => {
    if (!batch.length || oracleOnly) { batch = []; return; }
    const response = nativeProbe(python, { phase: "compare", ...identity, cases: batch });
    const comparison = response && typeof response === "object" && !Array.isArray(response) ? response :
      { status: "probe-error", reason: "native worker returned a non-object response" };
    const observedCases = Array.isArray(comparison.cases) ? comparison.cases : [];
    nativeCases.push(...observedCases);
    const incomplete = observedCases.length !== batch.length;
    const outOfOrder = !incomplete && observedCases.some((item, index) => item?.name !== batch[index].name);
    const perCaseFailed = observedCases.some(item => item?.status !== "pass");
    const inconsistentStatus = !incomplete && !outOfOrder &&
      ((comparison.status === "pass" && perCaseFailed) || (comparison.status === "fail" && !perCaseFailed));
    if (comparison.status !== "pass" || incomplete || outOfOrder || inconsistentStatus) {
      const failureStatus = incomplete || outOfOrder || inconsistentStatus ? "probe-error" : comparison.status || "probe-error";
      const shapeReason = incomplete ? `native worker returned ${observedCases.length}/${batch.length} cases` :
        outOfOrder ? "native worker changed case names or order" :
          inconsistentStatus ? "native worker aggregate status contradicts case statuses" : null;
      const failure = { status: failureStatus, firstCase: batch[0].name, expectedCases: batch.length,
        observedCases: observedCases.length,
        reason: [shapeReason, comparison.reason || observedCases.find(item => item?.status !== "pass")?.reason]
          .filter(Boolean).join("; ") || "native batch failed" };
      if (status === "pass") { status = failureStatus; report.nativeFailure = failure; }
      (report.nativeFailures ??= []).push(failure);
    }
    batch = [];
    bytes = Buffer.byteLength(JSON.stringify({ phase: "compare", ...identity, cases: [] }));
  };
  for (const item of items) {
    const itemBytes = Buffer.byteLength(JSON.stringify(item.input)) + 1;
    if (itemBytes + bytes > MAX_BATCH_BYTES) flush();
    if (itemBytes + bytes > MAX_BATCH_BYTES) throw new Error(`${item.summary.name}: one source case exceeds the 16 MiB native batch budget`);
    if (exportCase) exportCase(item.input);
    report.sourceCases.push(item.summary);
    if (!oracleOnly) {
      batch.push(item.input);
      bytes += itemBytes;
      if (batch.length >= MAX_BATCH_CASES) flush();
    }
  }
  flush();
  return { status, cases: nativeCases };
}

function main() {
  const parent = process.env.RUNNER_TEMP || process.env.APPDATA;
  if (!parent || !path.isAbsolute(parent)) throw new Error("APPDATA or RUNNER_TEMP must be an absolute report root");
  const reportPath = path.join(parent, "Accelerate", "reports", "v7-native-differential", "report.json");
  fs.mkdirSync(path.dirname(reportPath), { recursive: true });
  const report = {
    gate: "source-pinned-v7-native-differential-probe",
    status: "setup-error",
    generatedAt: new Date().toISOString(),
    scope: "normal/chaos/grand seed37 draft/first play and seed19 first-ACTIVE play; synthetic royal-capture and immobility terminal states plus disarmed/severed/iceSheet/staked ticks; optional bounded source-driven multi-turn playouts; full legal stream, both public viewers, sampled applies; not complete rule coverage",
    completeRuleCoverage: false,
    projectGo: false,
    bounds: { pageSize: PAGE_SIZE, maxPages: MAX_PAGES, maxActions: MAX_ACTIONS, maxExamined: MAX_EXAMINED, maxDraftChoices: 64, maxSamplesPerPosition: 4,
      maxPlayoutDecisions: MAX_PLAYOUT_DECISIONS, maxBatchCases: MAX_BATCH_CASES, maxBatchBytes: MAX_BATCH_BYTES },
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
    report.source = { sha256: SOURCE_SHA256, profile: PROFILE, rulesVersion: contract.catalog.rulesVersion,
      catalogVersion: contract.catalog.catalogVersion, sourcePublicCatalogHash: contract.catalog.sourcePublicCatalogHash,
      executionProfile: { version: source.executionProfile.profileVersion, sha256: source.executionProfileSha256,
        manifest: "execution-profile-20260928.json" },
      observationPolicyHash: contract.digest(contract.observationPolicy) };
    report.sourceCases = [];
    report.playouts = args.playouts;
    const exportPath = path.join(path.dirname(reportPath), "source-cases.jsonl");
    if (args.exportCases) fs.writeFileSync(exportPath, "");
    const exportCase = args.exportCases ? input => fs.appendFileSync(exportPath, `${JSON.stringify(input)}\n`) : null;
    const identity = { rulesVersion: contract.catalog.rulesVersion, catalogVersion: contract.catalog.catalogVersion,
      sourceSha256: SOURCE_SHA256, profile: PROFILE, observationPolicy: contract.observationPolicy };
    const preflight = args.oracleOnly ? { status: "oracle-only", reason: "native comparison explicitly omitted" } :
      nativeProbe(args.python, { phase: "preflight", ...identity });
    report.nativePreflight = preflight;
    if (!args.oracleOnly && preflight.status !== "ready") report.status = preflight.status;
    else {
      const items = (function* () {
        yield* sourceCases(source, contract);
        for (const playout of args.playouts) yield* sourcePlayoutCases(source, contract, playout);
      })();
      report.native = compareCases(args.python, identity, items, report, args.oracleOnly, exportCase);
      report.status = report.native.status;
      if (args.exportCases) report.sourceExport = {
        file: "source-cases.jsonl", cases: report.sourceCases.length,
        sha256: createHash("sha256").update(fs.readFileSync(exportPath)).digest("hex"),
      };
    }
    report.coverage = {
      styles: [...new Set(report.sourceCases.map(item => item.style || item.name.split("-")[0]))].sort(),
      modes: [...new Set(report.sourceCases.map(item => item.mode))].sort(),
      actionTypes: [...new Set(report.sourceCases.flatMap(item => item.actionTypes))].sort(),
      resultStatuses: [...new Set(report.sourceCases.map(item => item.resultStatus))].sort(),
      playoutPositions: report.sourceCases.filter(item => Number.isInteger(item.playoutDecision)).length,
      syntheticPositions: report.sourceCases.filter(item => item.syntheticSetup === true).length,
      syntheticStatusTypes: [...new Set(report.sourceCases.map(item => item.sourceStatus).filter(Boolean))].sort(),
    };
  } catch (error) {
    report.status = "setup-error";
    report.reason = `${error.name}: ${error.message}`;
  }
  report.decision = report.status === "pass" ? "bounded-P8-probe-pass-only" : "NO-GO";
  fs.mkdirSync(path.dirname(reportPath), { recursive: true });
  fs.writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ status: report.status, decision: report.decision,
    report: "Accelerate/reports/v7-native-differential/report.json",
    sourceCases: report.sourceCases?.length || 0, nativeCases: report.native?.cases?.length || 0,
    reason: report.reason || report.nativePreflight?.reason || report.nativeFailure?.reason || report.native?.cases?.find(item => item.status !== "pass")?.reason || undefined }));
  if (report.status !== "pass") process.exitCode = 1;
}

module.exports = Object.freeze({ SOURCE_SHA256, PROFILE, ACTIVE_SEED19, STYLES,
  sourceActions, buildCase, firstActiveDraftAction, advanceInitialDraft, sourceCases,
  syntheticTerminalCases, syntheticTimedStatusCases });
if (require.main === module) main();
