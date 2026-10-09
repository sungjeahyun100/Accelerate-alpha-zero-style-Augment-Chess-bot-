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
// A complete case owns one finite native call. Grouping unrelated cases made
// the 60-second deadline cumulative and hid which complete proof timed out.
// Keep every case and its complete legal stream; do not extend the deadline.
const MAX_BATCH_CASES = 1;
const MAX_PLAYOUT_DECISIONS = 128;
const STYLES = ["normal", "chaos", "grand"];
const NO_CASE_FAILURE_STATUSES = new Set([
  "native-timeout", "native-unavailable", "native-unsupported", "probe-error", "version-mismatch",
]);
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

// Evaluated in the pinned client's VM. This projects already verified source
// descriptors through its UI helpers; it does not generate moves or execute
// replacement rules. Mode names and the canonical large anchor are wire choices.
function sourcePublicPayloads(payloads) {
  const square = cell => {
    if (!cell || !Number.isInteger(cell.row) || !Number.isInteger(cell.col) || !inBounds(cell.row, cell.col))
      throw new TypeError("Source public selection has an invalid board coordinate");
    return { row: cell.row, col: cell.col };
  };
  return payloads.map(payload => {
    if (payload.type === "trolleyChoice")
      return [{ type: payload.type, color: payload.color, doomedIndex: payload.doomedIndex }];
    // Cards, drafts and explicit decisions already use their source public
    // payload. In particular ordered target arrays must not be sorted/deduped.
    if (payload.type !== "move") return [payload];
    const move = payload.move;
    const selectionMode = move.shotgunBlast ? "shotgun" : move.shotgunSnipe ? "snipe" :
      move.setLogDirection ? "log-direction" : null;
    let destinations;
    if (move.bigRookMove || move.colossusMove || move.colossusBody) destinations = [square(move)];
    else {
      let keys = moveHighlightKeys(move);
      // Highlights can omit clickable body/blast/excluded cells. The source
      // click handler uses moveContainsSquare instead. Its descriptor order
      // gives the aliases; presentation exclusions never reorder those aliases.
      const surface = move.bodyCells || move.displayCells || move.highlightCells || move.sectorCells;
      if (surface) keys = surface.filter(cell => moveContainsSquare(move, cell.row, cell.col)).map(squareKey);
      else if (!keys.length && moveContainsSquare(move, move.row, move.col)) keys = [squareKey(move)];
      destinations = keys.map(key => {
        if (typeof key !== "string" || !/^\d-\d$/.test(key)) throw new TypeError("Source public click key is malformed");
        const [row, col] = key.split("-").map(Number);
        return square({ row, col });
      });
    }
    let origins = [square(payload.from)];
    if (move.quantumFrom) {
      const physical = square(payload.from);
      const ghost = square(move.quantumFrom);
      const piece = state.board[physical.row]?.[physical.col];
      if (!piece) throw new Error("Source quantum public origin lost its physical piece");
      origins = quantumCellsForItemAt(piece, ghost.row, ghost.col).filter(cell => {
        const selection = normalizePieceSquare(cell.row, cell.col);
        return sameSquare(selection, physical) && sameSquare(selection.quantumFrom, ghost);
      }).map(square);
      if (isLargePiece(piece)) origins = origins.slice(0, 1);
    }
    return origins.flatMap(from => destinations.map(destination => {
      const intent = { type: "move", color: payload.color, from, destination };
      if (selectionMode !== null) intent.selectionMode = selectionMode;
      return intent;
    }));
  });
}

function sourcePublicIntents(runtime, contract, position, actions) {
  contract.validatePosition(position);
  for (const action of actions) contract.validateAction(position, action);
  runtime.restore(position);
  let projections;
  try {
    runtime.main.context.__nativeProjectionPayloads = actions.map(action => action.payload);
    projections = JSON.parse(runtime.evaluate(`JSON.stringify((${sourcePublicPayloads.toString()})(__nativeProjectionPayloads))`));
  } finally {
    delete runtime.main.context.__nativeProjectionPayloads;
  }
  if (!Array.isArray(projections) || projections.length !== actions.length)
    throw new Error("Source public projection changed the full action count");
  const publicIntents = [], byActionId = new Map(), seen = new Set();
  for (let index = 0; index < actions.length; index++) {
    const aliases = [], aliasKeys = new Set();
    for (const value of projections[index]) {
      const key = contract.canonical(value);
      if (aliasKeys.has(key)) continue;
      aliasKeys.add(key);
      const intent = contract.deepFreeze(contract.jsonCopy(value));
      aliases.push(intent);
      if (!seen.has(key)) {
        if (publicIntents.length >= MAX_ACTIONS)
          throw new Error("Source complete public intent stream exceeded probe budget; no prefix is accepted");
        seen.add(key);
        publicIntents.push(intent);
      }
    }
    if (!aliases.length) throw new Error(`Source action ${actions[index].actionId} has no public UI intent`);
    byActionId.set(actions[index].actionId, aliases);
  }
  return { publicIntents, byActionId };
}

// Continue a committed move through the actual frozen action stream. A Replay
// sequence is evidence only when the card is admitted by that stream.
function sourceReplaySequence(adapter, contract, runtime, start, actor) {
  let position = start;
  const steps = [];
  const take = (kind, action, available) => {
    const { publicIntents, byActionId } = sourcePublicIntents(runtime, contract, position, available);
    const intent = byActionId.get(action.actionId)?.[0];
    if (!intent) throw new Error("Replay sequence action has no public intent");
    const applied = adapter.apply(position, action, { recordHistory: true });
    if (!applied.ok) throw new Error("Frozen Replay sequence action was rejected");
    position = applied.position;
    const { actions: nextActions } = sourceActions(adapter, position);
    const nextIntents = sourcePublicIntents(runtime, contract, position, nextActions).publicIntents;
    const observations = Object.fromEntries(["white", "black"].map(viewer => [viewer, adapter.observe(position, viewer)]));
    steps.push({ kind, publicIntent: intent, beforeActions: publicIntents,
      position, actions: nextIntents, observations, result: adapter.result(position) });
  };
  try {
    for (let bridge = 0; position.state.turn !== actor && bridge < 2; bridge++) {
      const { actions } = sourceActions(adapter, position);
      const move = actions.find(action => action.payload.type === "move");
      if (!move) return { status: "unsupported", reason: "no natural bridge move returns the Replay owner to turn", steps: [] };
      take("bridge", move, actions);
    }
    if (position.state.turn !== actor)
      return { status: "unsupported", reason: "Replay owner did not regain the turn", steps: [] };
    const { actions } = sourceActions(adapter, position);
    const replay = actions.find(action => action.payload.type === "card" && action.payload.cardId === "replay");
    if (!replay) return { status: "unavailable",
      reason: "Replay card is absent from the full frozen legal stream", steps,
      beforeActions: sourcePublicIntents(runtime, contract, position, actions).publicIntents };
    take("replay", replay, actions);
    const { actions: after } = sourceActions(adapter, position);
    const follow = after.find(action => action.payload.type === "move") || after[0];
    if (!follow) return { status: "unsupported", reason: "no legal follow-up action after Replay", steps: [] };
    take("follow", follow, after);
    return { status: "complete", steps };
  } catch (error) {
    return { status: "unsupported", reason: `frozen Replay continuation: ${error.message}`, steps: [] };
  }
}

function buildCase(adapter, contract, name, position, requiredSampleId = null, publicRuntime = null) {
  contract.validatePosition(position);
  const { actions, examined } = sourceActions(adapter, position);
  if (!actions.length && position.state.mode !== "gameover") throw new Error(`${name}: nonterminal source has no action for reject/apply probes`);
  if (!publicRuntime) throw new TypeError(`${name}: pinned public projection runtime is required`);
  const { publicIntents, byActionId } = sourcePublicIntents(publicRuntime, contract, position, actions);
  const observations = Object.fromEntries(["white", "black"].map(viewer => {
    const observation = adapter.observe(position, viewer);
    contract.validateObservation(observation);
    return [viewer, observation];
  }));
  const first = actions[0];
  const rejectPayload = first ? contract.jsonCopy({ ...first.payload, color: first.payload.color === "white" ? "black" : "white" }) : null;
  const rejectPublicIntent = first ? contract.jsonCopy({ ...byActionId.get(first.actionId)[0], color: first.payload.color === "white" ? "black" : "white" }) : null;
  let sourceRejection = null;
  if (rejectPayload) {
    const rejected = adapter.apply(position, contract.action(position, rejectPayload));
    sourceRejection = {
      rejected: rejected.ok === false,
      unchanged: contract.canonical(rejected.position) === contract.canonical(position) &&
        contract.canonical(rejected.result) === contract.canonical(adapter.result(position)),
      method: "frozen-adapter-apply",
    };
    if (!sourceRejection.rejected || !sourceRejection.unchanged)
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
    const publicIntentAliases = byActionId.get(action.actionId);
    samples.push({ action, publicIntent: publicIntentAliases[0], publicIntentAliases,
      position: step.position, result: step.result, observations: nextObservations,
      replaySequence: action.payload.type === "move"
        ? sourceReplaySequence(adapter, contract, publicRuntime, step.position, action.payload.color)
        : { status: "unsupported", reason: "initial action is not a committed move", steps: [] } });
  }
  const result = adapter.result(position);
  contract.validateResult(result);
  const actionTypes = [...new Set(actions.map(action => action.payload.type))].sort();
  return {
    input: { name, position, result, observations, actions, publicIntents, rejectPayload, rejectPublicIntent, sourceRejection, samples },
    summary: { name, mode: position.state.mode, positionDigest: contract.digest(position), legalCount: actions.length,
      sourceExamined: examined, publicIntentCount: publicIntents.length, actionTypes, sampleTypes: samples.map(sample => sample.action.payload.type),
      sampleCount: samples.length, rejectCount: rejectPayload ? 1 : 0, staleRejectCount: samples.length,
      observationViewers: Object.keys(observations),
      resultStatus: result.status },
  };
}

function* sourcePlayoutCases(source, contract, { style, seed, decisions }) {
  const adapter = new GameAdapter({ source, contract });
  const publicRuntime = new OracleRuntime({ source, contract });
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
      const item = buildCase(adapter, contract, `${style}-seed${seed}-playout-${decision}`, position, null, publicRuntime);
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
    const publicRuntime = new OracleRuntime({ source, contract });
    try {
      for (const { seed, policy } of [{ seed: 37, policy: "first" }, { seed: 19, policy: "first-active" }]) {
        const initial = adapter.newGame({ gameStyle: style }, seed);
        if (seed === 37) cases.push(buildCase(adapter, contract, `${style}-seed37-draft`, initial, null, publicRuntime));
        const { position, picks } = advanceInitialDraft(adapter, contract, initial, { style, seed, policy });
        const name = `${style}-seed${seed}-${policy}-play`;
        const item = buildCase(adapter, contract, name, position, null, publicRuntime);
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
      const before = buildCase(adapter, contract, `${scenario.name}-before`, position, action.actionId, fixture);
      before.summary.syntheticSetup = true;
      before.summary.style = "normal";
      cases.push(before);
      const step = adapter.apply(position, action, { recordHistory: true });
      if (!step.ok || step.result.status !== "terminal" || step.result.winner !== "white")
        throw new Error(`${scenario.name}: source move did not produce the expected white terminal result`);
      const after = buildCase(adapter, contract, `${scenario.name}-after`, step.position, null, fixture);
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
      const before = buildCase(adapter, contract, `${scenario.name}-tick-before`, position, action.actionId, fixture);
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
      const after = buildCase(adapter, contract, `${scenario.name}-tick-after`, step.position, null, fixture);
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

// 개수·순서·집계 결과를 검사하면서 실행 전 오류의 원래 종류와 메시지를 보존한다.
function inspectNativeComparison(response, expectedCases) {
  const comparison = response && typeof response === "object" && !Array.isArray(response) ? response :
    { status: "probe-error", reason: "native worker returned a non-object response" };
  const cases = Array.isArray(comparison.cases) ? comparison.cases : [];
  const reportedNoCases = !Array.isArray(comparison.cases) && NO_CASE_FAILURE_STATUSES.has(comparison.status) &&
    typeof comparison.reason === "string" && comparison.reason.trim().length > 0;
  const incomplete = !reportedNoCases && cases.length !== expectedCases.length;
  const outOfOrder = !incomplete && cases.some((item, index) => item?.name !== expectedCases[index].name);
  const invalidCase = cases.some(item => typeof item?.status !== "string" || !item.status.trim());
  const perCaseFailed = cases.some(item => item?.status !== "pass");
  const invalidStatus = !reportedNoCases && !["pass", "fail"].includes(comparison.status);
  const inconsistentStatus = !reportedNoCases && !incomplete && !outOfOrder &&
    ((comparison.status === "pass" && perCaseFailed) || (comparison.status === "fail" && !perCaseFailed));
  if (!reportedNoCases && !incomplete && !outOfOrder && !invalidCase && !invalidStatus && !inconsistentStatus &&
      comparison.status === "pass") return { cases, failure: null };
  const malformed = incomplete || outOfOrder || invalidCase || invalidStatus || inconsistentStatus;
  const shapeReason = incomplete ? `native worker returned ${cases.length}/${expectedCases.length} cases` :
    outOfOrder ? "native worker changed case names or order" :
      invalidCase ? "native worker returned a missing or invalid case status" :
        invalidStatus ? "native worker returned an invalid aggregate status" :
          inconsistentStatus ? "native worker aggregate status contradicts case statuses" : null;
  return { cases, failure: {
    status: malformed ? "probe-error" : comparison.status,
    firstCase: expectedCases[0]?.name,
    expectedCases: expectedCases.length,
    observedCases: cases.length,
    reason: [shapeReason, comparison.reason || cases.find(item => item?.status !== "pass")?.reason]
      .filter(Boolean).join("; ") || "native batch failed",
  } };
}

function compareCases(python, identity, items, report, oracleOnly, exportCase = null) {
  const nativeCases = [];
  let batch = [];
  let bytes = Buffer.byteLength(JSON.stringify({ phase: "compare", ...identity, cases: [] }));
  let status = oracleOnly ? "oracle-only" : "pass";
  const flush = () => {
    if (!batch.length || oracleOnly) { batch = []; return; }
    const response = nativeProbe(python, { phase: "compare", ...identity, cases: batch });
    const { cases: observedCases, failure } = inspectNativeComparison(response, batch);
    nativeCases.push(...observedCases);
    if (failure) {
      if (status === "pass") { status = failure.status; report.nativeFailure = failure; }
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
    if (source.executionProfile.profileVersion !== contract.catalog.executionProfile.version ||
        source.executionProfileSha256 !== contract.catalog.executionProfile.sha256)
      throw new Error("v7 source execution manifest identity mismatch");
    report.source = { sha256: SOURCE_SHA256, profile: PROFILE, rulesVersion: contract.catalog.rulesVersion,
      catalogVersion: contract.catalog.catalogVersion, sourcePublicCatalogHash: contract.catalog.sourcePublicCatalogHash,
      executionProfile: contract.jsonCopy(contract.catalog.executionProfile),
      observationPolicyHash: contract.digest(contract.observationPolicy) };
    report.sourceCases = [];
    report.playouts = args.playouts;
    const exportPath = path.join(path.dirname(reportPath), "source-cases.jsonl");
    if (args.exportCases) fs.writeFileSync(exportPath, "");
    const exportCase = args.exportCases ? input => fs.appendFileSync(exportPath, `${JSON.stringify(input)}\n`) : null;
    const identity = { rulesVersion: contract.catalog.rulesVersion, catalogVersion: contract.catalog.catalogVersion,
      sourceSha256: SOURCE_SHA256, profile: PROFILE, executionProfile: contract.jsonCopy(contract.catalog.executionProfile),
      observationPolicy: contract.observationPolicy };
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
  sourceActions, sourcePublicPayloads, sourcePublicIntents, buildCase, firstActiveDraftAction, advanceInitialDraft, sourceCases,
  syntheticTerminalCases, syntheticTimedStatusCases, inspectNativeComparison });
if (require.main === module) main();
