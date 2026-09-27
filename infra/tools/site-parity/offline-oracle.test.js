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
  oracle.restore(p);
  oracle.evaluate("queueMicrotask(()=>{state.restoreCallbackProbe=true;});scheduledGameOverReplayState={pending:'prior-position'};activePieceAnimationUntil.set('prior-animation',Date.now()+240);");
  const beforeState = oracle.state(), beforeRandom = contract.jsonCopy(oracle.random);
  for (const [field, malformed] of [["values", { __simType: "Set", values: 12 }], ["entries", { __simType: "Map", entries: 12 }]]) {
    const state = contract.jsonCopy(p.state);
    state.restoreMalformedCollection = malformed;
    const input = contract.position(state, { ...p.rng, cursor: p.rng.cursor + 1 });
    assert.throws(() => oracle.restore(input), /iterable/, field);
    assert.deepEqual(oracle.state(), beforeState, "failed decoding preserves the live state");
    assert.deepEqual(oracle.random, beforeRandom, "failed decoding preserves the live RNG");
    assert.equal(oracle.evaluate("__microtasks.length"), 1);
    assert.equal(oracle.evaluate("scheduledGameOverReplayState.pending"), "prior-position");
    assert.equal(oracle.evaluate("activePieceAnimationUntil.has('prior-animation')"), true, "failed decoding preserves the live renderer cache");
    assert.equal(Object.hasOwn(oracle.main.context, "__restoredState"), false);
  }
  assert.equal(oracle.snapshot().state.restoreCallbackProbe, true, "the original callback still executes");
  assert.equal(oracle.evaluate("activePieceAnimationUntil.has('prior-animation')"), true, "snapshot settlement retains this invocation's renderer cache");
  oracle.restore(p);
  assert.equal(oracle.evaluate("activePieceAnimationUntil.size"), 0, "successful restoration starts a cold renderer invocation");
});

test("large-piece snapshot restoration retains source aliases within independent board frames", () => {
  for (const kind of ["colossus", "bigRook", "bigBishop"]) {
    oracle.newGame({ draftDelete: true }, 29, [0.1, 0.2]);
    oracle.main.context.__largeKind = kind;
    oracle.evaluate("state.board=Array.from({length:8},()=>Array(8).fill(null));placeColossus(piece('white',__largeKind),3,3);state.boardHistory=[{board:cloneBoardPreservePieces()}];state.replayBaseFrame=captureReplayFrame();state.replayTailFrame=captureReplayFrame();");
    const input = oracle.snapshot(), unchanged = JSON.stringify(input);
    oracle.evaluate("nullification({row:4,col:4})");
    const direct = oracle.snapshot();
    assert.equal(direct.state.board.flat().filter(piece => piece?.nullification).length, 4);
    oracle.restore(JSON.parse(unchanged));
    assert.equal(oracle.evaluate("[state.board,...state.boardHistory.map(frame=>frame.board),state.replayBaseFrame.board,state.replayTailFrame.board].every(board=>board[3][3]===board[3][4]&&board[3][3]===board[4][3]&&board[3][3]===board[4][4])"), true);
    assert.equal(oracle.evaluate("new Set([state.board[3][3],state.boardHistory[0].board[3][3],state.replayBaseFrame.board[3][3],state.replayTailFrame.board[3][3]]).size"), 4, "equal IDs in different frames remain separate objects");
    oracle.evaluate("nullification({row:4,col:4})");
    assert.deepEqual(oracle.snapshot(), direct, "actual source transition, full state and RNG match after restoration");
    assert.equal(oracle.state().boardHistory[0].board[3][3].nullification, undefined);
    assert.equal(JSON.stringify(input), unchanged, "restoration never mutates caller snapshot data");
    const edits = [
      state => { state.board[4][4].color = "black"; },
      state => { state.board[4][4].nullification = true; },
      state => { for (const piece of state.board.flat().filter(Boolean)) delete piece.id; },
      state => { state.board[0][0] = { ...state.board[3][3], type: "rook" }; },
      state => { state.boardHistory[0].board[4][4].type = "rook"; },
    ];
    for (const edit of edits) {
      const state = contract.jsonCopy(input.state); edit(state);
      const invalid = contract.position(state, { ...input.rng, cursor: input.rng.cursor + 1 });
      const current = oracle.snapshot();
      assert.throws(() => oracle.restore(invalid), /snapshot piece|snapshot footprint/);
      assert.deepEqual(oracle.snapshot(), current, "invalid alias groups preserve the live position and RNG");
    }
    const ordinary = contract.jsonCopy(input.state);
    ordinary.board = Array.from({ length: 8 }, () => Array(8).fill(null));
    ordinary.board[1][1] = ordinary.board[1][2] = { id: "duplicate-rook", type: "rook", color: "white" };
    assert.throws(() => oracle.restore(contract.position(ordinary, input.rng)), /Duplicate non-large/);
  }
  oracle.newGame({ draftDelete: true }, 29);
  oracle.evaluate("state.board=Array.from({length:8},()=>Array(8).fill(null));placeColossus(piece('white','bigRook'),7,6);");
  const clipped = oracle.snapshot();
  assert.equal(clipped.state.board.flat().filter(Boolean).length, 2, "the actual source helper clips its footprint at the edge");
  oracle.restore(clipped);
  assert.equal(oracle.evaluate("state.board[7][6]===state.board[7][7]"), true);
  assert.deepEqual(oracle.snapshot(), clipped, "source-created partial footprints roundtrip without silent repair");
  oracle.evaluate("for(const piece of state.board.flat().filter(Boolean)){delete piece.anchorRow;delete piece.anchorCol;}");
  const legacy = oracle.snapshot(); oracle.restore(legacy);
  assert.equal(oracle.evaluate("state.board[7][6]===state.board[7][7]"), true);
  assert.deepEqual(oracle.snapshot(), legacy, "source relinking does not require or invent missing anchor metadata");
  for (const kind of ["bigRook", "bigBishop"]) {
    oracle.newGame({ draftDelete: true }, 31); oracle.main.context.__kind = kind;
    oracle.evaluate("(()=>{state.board=Array.from({length:8},()=>Array(8).fill(null));const large=piece('black',__kind);placeColossus(large,3,3);large.origin='a8';})()");
    assert.equal(oracle.evaluate("exile({row:3,col:3}).ok"), true);
    oracle.evaluate("state.turn='black'");
    const disconnected = oracle.snapshot();
    oracle.main.context.__candidate = disconnected.state;
    assert.equal(oracle.evaluate("(()=>{const candidate=__decode(__candidate);normalizeDeserializedState(candidate);return candidate.board[0][0]===candidate.board[3][4]&&candidate.board[3][4]===candidate.board[4][3]&&candidate.board[4][3]===candidate.board[4][4];})()"), true);
    const directResult = JSON.parse(oracle.evaluate("__encode(nullification({row:4,col:4}))"));
    assert.equal(directResult.ok, true);
    const direct = oracle.snapshot(); oracle.restore(disconnected);
    assert.equal(oracle.evaluate("state.board[0][0]===state.board[4][4]"), true);
    assert.deepEqual(oracle.snapshot(), disconnected, "source-created disconnected aliases preserve the old metadata");
    assert.deepEqual(JSON.parse(oracle.evaluate("__encode(nullification({row:4,col:4}))")), directResult);
    assert.deepEqual(oracle.snapshot(), direct, "source effect/state/RNG match after disconnected restoration");
  }
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
  assert.equal(HEADLESS_PROFILE.version, contract.ORACLE_PROFILE_VERSION);
  assert.equal(HEADLESS_PROFILE.rendererContext, "cold-activePieceAnimationUntil-at-admission");
  // Source94100's vanish ghost calls createPieceElement75203, whose external
  // deadline Map75801 otherwise changes the serialized animation Set75820
  // when the same immutable snapshot is admitted a second time.
  const ghost = "playPieceVanishLocalEffect([{item:clonePlain(state.board[6][0]),row:6,col:0}])";
  const fresh = new OfflineOracle(); fresh.restore(initial); fresh.evaluate(ghost);
  const expected = fresh.snapshot();
  assert.equal(expected.state.animatedPieceIds.values.includes(initial.state.board[6][0].id), true);
  assert.deepEqual(expected.rng, initial.rng, "the actual source ghost performs no RNG draw");
  for (let repeat = 0; repeat < 2; repeat++) {
    oracle.restore(initial); oracle.evaluate(ghost);
    assert.deepEqual(oracle.snapshot(), expected, "fresh and repeated admitted source queries preserve the complete state and RNG");
    assert.equal(oracle.evaluate("activePieceAnimationUntil.size"), 1);
    oracle.evaluate(ghost);
    assert.equal(oracle.evaluate("activePieceAnimationUntil.size"), 1, "cache activity survives within one invocation");
    assert.deepEqual(oracle.snapshot(), expected);
  }
  oracle.evaluate("activePieceAnimationUntil.set('prior-game',Date.now()+240)");
  oracle.newGame({ draftDelete: true }, 41);
  assert.equal(oracle.evaluate("activePieceAnimationUntil.size"), 0, "a valid new game starts with a cold renderer cache");
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
  oracle.restore(p);
  oracle.evaluate("state.board=Array.from({length:8},()=>Array(8).fill(null));state.turnsTaken={white:8,black:5};state.board[3][3]={...piece('black','trickster'),tricksterMoveType:'wizard',mana:4,maxMana:5};state.board[4][2]={...piece('black','pawn'),vipInvitation:{by:'white',triggerTurn:8},holdoutPromotion:{by:'white',readyTurn:19},witchTrial:{by:'white',remaining:2,countBy:'white'}};state.board[2][2]={...piece('black','siren'),hiddenFrom:'white'};state.winterKingdom={enabled:true,previewIds:[state.board[2][2].id,state.board[4][2].id],frozenIds:[],lastCycle:1};state.deckSlots.black=[{...CARD_BY_ID.get('black-box'),instanceId:'box',used:false,boxRevealedCardId:'parry'},{...CARD_BY_ID.get('random-roulette'),instanceId:'roulette',used:true,randomRouletteResultType:'reaper'}];");
  const surface = oracle.snapshot(), white = oracle.observe(surface, "white"), black = oracle.observe(surface, "black");
  assertSchema(white); assertSchema(black);
  assert.equal(white.board[3][3].mana, undefined); assert.equal(white.board[3][3].status.tricksterMovement, undefined);
  assert.equal(black.board[3][3].mana, 4); assert.equal(black.board[3][3].status.tricksterMovement, "wizard");
  assert.deepEqual([white.board[4][2].status.vipRemaining,white.board[4][2].status.holdoutRemaining,white.board[4][2].status.witchTrialRemaining], [3,14,2]);
  assert.equal(white.board[2][2], null);
  assert.ok(white.publicState.boardMarks.some(mark=>mark.kind==="sirenAura"&&mark.square.row===2&&mark.square.col===2));
  assert.equal(white.publicState.boardMarks.filter(mark=>mark.kind==="winterForecast").length, 1);
  assert.deepEqual(white.publicState.winterKingdom, {enabled:true});
  assert.equal(white.publicState.revealedOpponentCards[0].revealed, undefined);
  assert.deepEqual(white.publicState.revealedOpponentCards[1].revealed, {rouletteType:"reaper"});
  assert.ok(!JSON.stringify(white).includes("triggerTurn")&&!JSON.stringify(white).includes("previewIds"));
  oracle.newGame({draftDelete:true},43);
  oracle.evaluate("state.activeTrolley=buildTrolleyDilemmaForColor('white','black')");
  const trolley=oracle.snapshot(), changed=contract.jsonCopy(trolley.state);
  assert.ok(trolley.state.activeTrolley, "the actual source produces a visible two-bundle dilemma");
  changed.activeTrolley.id="private-other-window";
  const originalView=oracle.observe(trolley,"white"), changedView=oracle.observe(contract.position(changed,contract.rng(123)),"white");
  assert.equal(originalView.publicState.selectionPhase.kind,"trolley");
  assert.equal(originalView.publicState.selectionPhase.windowId,undefined);
  assert.equal(originalView.informationStateKey,changedView.informationStateKey);
  assert.ok(!JSON.stringify(originalView).includes(trolley.state.activeTrolley.id));
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
