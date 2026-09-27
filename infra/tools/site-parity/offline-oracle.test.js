"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { OfflineOracle, HEADLESS_PROFILE } = require("./offline-oracle");
const { cacheRoot, verify, loadMain, loadWorker } = require("./frozen-site");
const { orderedSelections } = require("./client-enumeration");
const contract = require("../../../bridge/tools/runtime-contract");
const { validate, resolveRef } = require("../../../bridge/tools/validate");
const oracle = new OfflineOracle(); // Deliberately fail when the adopted external baseline is unavailable.
const schema = resolveRef("runtime-v1.schema.json#", "runtime-v1.schema.json");
function assertSchema(value) { assert.deepEqual(validate(schema.schema, value, "$", schema.docName), []); }

test("client-only cache validates executable dependencies and cannot load a worker", () => {
  const original = cacheRoot(), root = path.join(original, "client-loader-check");
  const baseline = JSON.parse(fs.readFileSync(path.join(original, "baseline.json"), "utf8"));
  const dependencies = baseline.files.filter(file => /^main-/.test(file.name) || file.name === "acorn-8.15.0.js");
  if (!dependencies.some(file => file.name === "acorn-8.15.0.js")) dependencies.push({
    name: "acorn-8.15.0.js", sha256: "fdb08546776ec6228b03e8d02b40d4ab3255bae5f401adba7ff5dad927ac5c9c", bytes: 241575,
  });
  const manifest = { ...baseline, executionScope: "frozen-client", files: dependencies };
  const owned = [...dependencies.map(file => file.name), "baseline.json"];
  if (fs.existsSync(root)) {
    assert.equal(fs.lstatSync(root).isSymbolicLink(), false, "owned loader slot cannot be a link");
    assert.deepEqual(fs.readdirSync(root), [], "owned loader slot must be empty");
  } else fs.mkdirSync(root);
  try {
    for (const file of dependencies) fs.copyFileSync(path.join(original, file.name), path.join(root, file.name));
    fs.writeFileSync(path.join(root, "baseline.json"), JSON.stringify(manifest));
    const client = loadMain(root);
    assert.equal(client.evaluate("typeof createInitialBoard"), "function");
    assert.equal(client.manifest.executionScope, "frozen-client");
    assert.throws(() => verify(root), /requires the original worker/);
    assert.throws(() => loadWorker(root), /requires the original worker/);
    manifest.files = dependencies.filter(file => file.name !== "acorn-8.15.0.js");
    fs.writeFileSync(path.join(root, "baseline.json"), JSON.stringify(manifest));
    assert.throws(() => loadMain(root), /main asset and the pinned parser/);
    manifest.files = [...dependencies, dependencies[0]];
    fs.writeFileSync(path.join(root, "baseline.json"), JSON.stringify(manifest));
    assert.throws(() => loadMain(root), /Duplicate baseline/);
  } finally {
    for (const name of owned) if (fs.existsSync(path.join(root, name))) fs.unlinkSync(path.join(root, name));
    fs.rmdirSync(root);
  }
});

for (const [style, choices, picks, cards] of [["normal", 3, 2, 1], ["chaos", 3, 2, 2], ["grand", 28, 12, 6]]) test(`actual ${style} initialization and draft reaches play`, () => {
  let p = oracle.newGame({ gameStyle: style }, 12345);
  assert.equal(oracle.actions(p).length, choices);
  for (let step = 0; step < picks; step++) {
    const result = oracle.apply(p, oracle.actions(p)[0]);
    assert.equal(result.ok, true); assertSchema(result); p = result.position;
  }
  assert.equal(p.state.mode, "play");
  assert.equal(p.state.deckSlots.white.filter(Boolean).length, cards);
  assert.equal(p.state.deckSlots.black.filter(Boolean).length, cards);
  assert.equal(p.history.length, picks);
  const observation = oracle.observe(p, "white");
  assert.equal(observation.publicState.revealedOpponentCards.length, cards); assertSchema(observation);
});
test("snapshot restore preserves exact RNG, public hints and 20 initial moves", () => {
  const p = oracle.newGame({ draftDelete: true }, 22, [0.1, 0.2, 0.3]);
  assert.equal(oracle.actions(p).length, 20);
  assert.equal(oracle.publicHints(p, "white").moves.flatMap(piece => piece.destinations).length, 20);
  const a = oracle.actions(p)[0], left = oracle.apply(p, a), right = oracle.apply(JSON.parse(JSON.stringify(p)), JSON.parse(JSON.stringify(a)));
  assert.equal(left.position.positionId, right.position.positionId); assertSchema(left);
  const wrong = contract.action(p, { ...a.payload, color: "black" });
  const rejected = oracle.apply(p, wrong);
  assert.equal(rejected.ok, false); assert.equal(rejected.position.positionId, p.positionId);
});
test("actual draft acquisition applies passive effects outside AI simulation", () => {
  let p = oracle.newGame({ gameStyle: "grand" }, 12345);
  const choice = p.state.draft.choices.find(card => card.id === "d4");
  assert.ok(choice, "deterministic fixture-free initial pool contains d4");
  const a = oracle.actions(p).find(action => action.payload.cardInstanceId === choice.instanceId);
  const owner = a.payload.color, step = oracle.apply(p, a);
  assert.equal(step.ok, true); p = step.position;
  while (p.state.mode === "draft") p = oracle.apply(p, oracle.actions(p)[0]).position;
  assert.equal(p.state.d4[owner], true);
  const acquired = p.state.deckSlots[owner].find(card => card?.id === "d4");
  assert.equal(acquired.passiveApplied, true); assert.equal(acquired.used, true);
  assert.equal(oracle.observe(p, "black").publicState.d4[owner], true);
  assert.equal(oracle.evaluate("aiSimulationDepth"), 0);
});
test("headless render retains source potion cleanup without inventing DOM RNG", () => {
  const initial = oracle.newGame({ draftDelete: true }, 41), state = contract.jsonCopy(initial.state);
  const active = state.board[6][0], expired = state.board[0][0];
  active.potionEffects = ["poisonStun", "poisonStun", "basicTraining", "unknown-effect"];
  active.poisonStunTurns = 1; active.basicTraining = false;
  expired.potionEffects = ["poisonStun"]; expired.poisonStunTurns = 0;
  const input = contract.position(state, initial.rng);
  oracle.restore(input);
  oracle.evaluate("renderAll()");
  const cleaned = oracle.snapshot();
  assert.deepEqual(cleaned.state.board[6][0].potionEffects, ["poisonStun"]);
  assert.equal(Object.hasOwn(cleaned.state.board[0][0], "potionEffects"), false);
  assert.deepEqual(cleaned.rng, input.rng);
  assert.equal(oracle.evaluate("aiSimulationDepth"), 0);
  assert.equal(input.state.board[6][0].potionEffects.length, 4);
  assert.equal(HEADLESS_PROFILE.browserFutureRngEquality, false);
  oracle.restore(input);
  oracle.evaluate("pruneBoardPotionEffects(state.board)");
  assert.deepEqual(oracle.snapshot().state, cleaned.state);
});
test("terminal microtasks retain source replay, chain cleanup and conditional notation RNG", () => {
  const initial = oracle.newGame({ draftDelete: true }, 45), state = contract.jsonCopy(initial.state);
  state.mode = "gameover"; state.winner = "white";
  state.chainBonds = [{ id: "distant-bond", aId: state.board[7][4].id, bId: state.board[0][4].id, by: "white" }];
  state.ruleTicketChoice = { color: "white" };
  state.pendingNotation = { id: "terminal-known", kind: "special", color: "white", text: "terminal", description: "terminal" };
  const input = contract.position(state, initial.rng);
  oracle.restore(input);
  oracle.evaluate("renderAll();scheduleGameOverReplayRecord();scheduleGameOverReplayRecord()");
  assert.equal(oracle.evaluate("__microtasks.length"), 1);
  assert.equal(oracle.state().chainBonds.length, 1, "settlement is queued, not synchronous");
  const settled = oracle.snapshot();
  assert.deepEqual(settled.state.chainBonds, []);
  assert.equal(settled.state.ruleTicketChoice, null);
  assert.equal(settled.state.pendingNotation, null);
  assert.equal(settled.state.boardHistory.at(-1).label, "gameover");
  assert.deepEqual(settled.rng, input.rng, "this terminal record has no conditional card notation draw");
  assert.deepEqual(oracle.snapshot(), settled, "a second snapshot does not run settlement twice");
  // The actual recorder conditionally creates card IDs, consuming source RNG
  // even when an existing pending notation is ultimately selected instead.
  oracle.restore(input);
  oracle.evaluate("queueMicrotask(()=>recordBoardHistory('gameover',{cardAnimation:{name:'known-card',color:'white'}}))");
  const withNotationDraw = oracle.snapshot();
  assert.deepEqual(withNotationDraw.rng, contract.nextRandom(input.rng).rng);
  assert.equal(withNotationDraw.state.notationEvent.id, "terminal-known");
  oracle.restore(input);
  oracle.evaluate("scheduleGameOverReplayRecord()");
  oracle.restore(initial);
  assert.deepEqual(oracle.snapshot(), initial, "restoration discards callbacks owned by the prior candidate");
  oracle.evaluate("queueMicrotask(function repeat(){queueMicrotask(repeat)})");
  assert.throws(() => oracle.snapshot(), /microtask budget exceeded/);
  oracle.restore(initial);
});
test("ordered premove pages preserve 1..3 plans and resume without materializing the surface", () => {
  const groups = ["a", "b", "c"].map(piece => [0, 1].map(choice => ({ piece, choice })));
  const plans = [...orderedSelections(groups)];
  assert.deepEqual([1, 2, 3].map(length => plans.filter(plan => plan.length === length).length), [6, 24, 48]);
  assert.equal(new Set(plans.map(plan => JSON.stringify(plan))).size, 78);
  assert.ok(plans.some(plan => plan.map(move => move.piece).join("") === "abc"));
  assert.ok(plans.some(plan => plan.map(move => move.piece).join("") === "cba"));
  assert.ok(plans.every(plan => new Set(plan.map(move => move.piece)).size === plan.length));
  assert.throws(() => orderedSelections(groups, 4).next(), /depth/);
  const initial = oracle.newGame({ draftDelete: true }, 83);
  oracle.restore(initial);
  oracle.evaluate("state.draftDelete=false;state.deckSlots.white=[cloneCard(CARD_DEFS.find(card=>card.id==='premove'))];state.deck.white=state.deckSlots.white;state.playerCards=state.deckSlots;");
  const position = oracle.snapshot();
  const bounded = new OfflineOracle(undefined, { maxCandidates: 128 });
  assert.throws(() => bounded.candidates(position), /explicit candidate budget/);
  const first = bounded.actionStream(position, { cardId: "premove", legal: false });
  const second = bounded.actionStream(position, { cardId: "premove", legal: false });
  const a = first.nextPage(5, { maxExamined: 2 });
  assert.equal(a.actions.length, 2); assert.equal(a.exhausted, false);
  assert.equal(a.stopReason, "examined-budget");
  const b = second.nextPage(5), c = first.nextPage(3);
  assert.deepEqual([...a.actions, ...c.actions], b.actions);
  assert.deepEqual(b.actions.map(action => action.payload.target.selections.length), [1, 2, 3, 3, 3]);
  assert.ok(b.actions.every(action => action.positionId === position.positionId));
  assert.ok(b.actions.every(action => bounded.apply(position, action, { recordHistory: false }).ok));
  const legal = bounded.actionStream(position, { cardId: "premove" }).nextPage(3);
  assert.equal(legal.actions.length, 3); assert.equal(legal.stopReason, "page-limit");
  const ordinary = bounded.actionStream(initial), paged = [];
  let page;
  do { page = ordinary.nextPage(7); paged.push(...page.actions); } while (!page.exhausted);
  assert.deepEqual(paged, bounded.actions(initial));
  assert.throws(() => first.nextPage(0), /Page size/);
});
test("hidden piece and private RNG changes do not change viewer projection", () => {
  const p = oracle.newGame({ draftDelete: true }, 42), a = contract.jsonCopy(p.state), b = contract.jsonCopy(p.state);
  a.board[0][0].hiddenFrom = "white"; b.board[0][0].hiddenFrom = "white"; b.board[0][0].type = "bishop";
  const first = contract.position(a, contract.rng(1)), second = contract.position(b, contract.rng(2));
  const one = oracle.observe(first, "white"), two = oracle.observe(second, "white");
  assert.equal(one.board[0][0], null); assert.equal(one.informationStateKey, two.informationStateKey);
  assert.ok(!JSON.stringify(one).includes(first.positionId)); assert.ok(!JSON.stringify(one).includes("hiddenFrom"));
  const step = oracle.apply(first, oracle.actions(first)[0]);
  assert.ok(!JSON.stringify(oracle.observe(step.position, "white").history).includes("actionId"));
  assert.throws(() => oracle.observe(contract.position({ ...a, unknownPublicField: 1 }, contract.rng(1)), "white"), /Unclassified/);
});
test("actual king capture reaches terminal result", () => {
  const initial = oracle.newGame({ draftDelete: true }, 9), state = contract.jsonCopy(initial.state);
  state.board = Array.from({ length: 8 }, () => Array(8).fill(null));
  state.board[7][4] = { color: "white", type: "king", id: "wk", moved: true, shielded: false };
  state.board[1][4] = { color: "white", type: "rook", id: "wr", moved: true, shielded: false };
  state.board[0][4] = { color: "black", type: "king", id: "bk", moved: true, shielded: false };
  const p = contract.position(state, initial.rng), a = oracle.actions(p).find(a => a.payload.type === "move" && a.payload.from.row === 1 && a.payload.move.row === 0 && a.payload.move.col === 4);
  assert.ok(a); const result = oracle.apply(p, a);
  assert.equal(result.ok, true); assert.equal(result.result.status, "terminal"); assert.equal(result.result.winner, "white");
  assert.deepEqual(oracle.actions(result.position), []); assertSchema(result);
});
