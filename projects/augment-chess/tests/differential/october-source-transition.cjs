#!/usr/bin/env node
"use strict";

// Source-side differential receipt. The original October bundle and parser
// remain external; this program records source results without inventing a
// Rust expectation or claiming a complete player observation.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { loadSource } = require("../../oracle/tools/site-parity/october-source-probe");
const profile = require("../../contracts/catalog/execution-profile-20261007-probe.json");

const run = (context, code) => vm.runInContext(code, context, { timeout: 15000 });
const read = (context, code) => JSON.parse(run(context, `JSON.stringify(${code})`));

function seededContext(sourcePath, parserPath, seed) {
  const context = loadSource(sourcePath, parserPath);
  context.__seed = seed;
  context.__random = () => {
    let value = context.__seed;
    value ^= value << 13;
    value ^= value >>> 17;
    value ^= value << 5;
    context.__seed = value;
    return (value >>> 0) / 0x100000000;
  };
  return context;
}

function start(sourcePath, parserPath, seed, { deathmatch = true } = {}) {
  const context = seededContext(sourcePath, parserPath, seed);
  context.__deathmatch = deathmatch;
  run(context, "Math.random=__random;selectedGameStyle='normal';localPlayMode='local';playMode='local';ruleSelectionEnabled=false;selectedRuleCardIds=[];deathmatchEnabled=__deathmatch;resetGame(false,[]);beginInitialGameFlow()");
  assert.equal(run(context, "state.mode"), "draft");
  context.__initialSourceState = read(context, "state");
  context.__initialRngState = context.__seed;
  return context;
}

function finishOpening(context, preferred) {
  for (let pick = 0; pick < 2; pick++) {
    context.__preferred = pick === 0 ? preferred : null;
    assert.equal(run(context, "(()=>{const color=state.draft.color,phase=state.draft.phase,card=state.draft.choices.find(card=>card.id===__preferred)||state.draft.choices[0];const ok=finishDraft(card,null,{auto:true});if(ok)completeDraftStep(color,phase);return ok})()"), true);
  }
  assert.equal(run(context, "state.mode"), "play");
}

function apply(context, action) {
  assert.ok(action, "Original source did not offer the requested legal action.");
  context.__action = action;
  assert.equal(read(context, "applyAiAction(__action)").ok, true);
}

function cardAction(context, id, select = actions => actions[0]) {
  context.__cardId = id;
  const actions = read(context, "collectValidAiActions('white',{includeCards:true,exhaustiveCards:true}).filter(action=>action.type==='card'&&action.cardId===__cardId)");
  assert.ok(actions.length, `Original source did not offer ${id} card action.`);
  const action = select(actions);
  apply(context, action);
  return action;
}

function firstMove(context) {
  return read(context, "collectValidAiActions(state.turn,{includeCards:false,exhaustiveCards:false}).find(action=>action.type==='move')");
}

function moveIntent(action) {
  return { type: "move", color: action.color, from: action.from,
    destination: { row: action.move.row, col: action.move.col } };
}

function cardIntent(action) {
  return { type: "card", color: action.color, cardId: action.cardId,
    target: action.target ?? null };
}

function receipt(name, context, seed, before, after, replay, coverage, chance = null, ruleIds = []) {
  const { sourceState: beforeState, ...beforeProjection } = before;
  const { sourceState: afterState, ...afterProjection } = after;
  return {
    schemaVersion: 1, caseName: name, coverage,
    sourceMainSha256: profile.sourceMainSha256,
    sourcePublicCatalogHash: profile.sourcePublicCatalogHash,
    rulesVersion: profile.rulesVersion,
    executionProfileVersion: profile.profileVersion,
    gameStyle: "normal", seed, ruleIds,
    replay,
    sourceExpected: { before: beforeProjection, after: afterProjection },
    sourceEvidence: { initialState: context.__initialSourceState ?? null,
      initialRngState: context.__initialRngState ?? null, beforeState, afterState,
      serialization: "JSON.stringify(state); non-JSON internals are not included" },
    chance: { observedRngState: context.__seed ?? null, outcomeDistribution: chance },
    observationScope: "pieceVisibleToColorAt board projection for both viewers; full observation unverified",
  };
}

function projection(context) {
  return read(context, `({
    sourceState: state,
    mode: state.mode, turn: state.turn, moveCount: state.moveCount,
    turnsTaken: state.turnsTaken, cardsUsedThisTurn: state.cardsUsedThisTurn,
    actionsRemaining: state.actionsRemaining, fullMove: state.fullMove,
    switcheroo: state.switcheroo, winner: state.winner,
    board: state.board.flatMap((line,row)=>line.flatMap((piece,col)=>piece?[{
      row,col,id:piece.id,type:piece.type,color:piece.color,moved:Boolean(piece.moved),
      holdoutPromotion:piece.holdoutPromotion||null,chimera:Boolean(piece.chimera),
      reaperCaptures:piece.reaperCaptures??null
    }]:[])),
    deck: Object.fromEntries(['white','black'].map(color=>[color,playerDeck(color).filter(Boolean).map(card=>({
      id:card.id,instanceId:card.instanceId,used:Boolean(card.used)
    }))])),
    visibleBoard: Object.fromEntries(['white','black'].map(viewer=>[viewer,state.board.flatMap((line,row)=>
      line.flatMap((piece,col)=>piece&&pieceVisibleToColorAt(piece,row,col,viewer,state.board)?
        [{row,col,type:piece.type,color:piece.color}]:[]))]))
  })`);
}

function switcherooRustInput(sourceState) {
  // Reviewed, bounded import shape; sourceEvidence is diagnostic material only.
  const fields = ["board", "deckSlots", "captures", "turn", "mode", "actionsRemaining",
    "switcheroo", "moveCount", "turnsTaken", "cardsUsedThisTurn", "winner", "fullMove"];
  return {
    ...Object.fromEntries(fields.map(key => [key, sourceState[key]])),
    octoberCatalogHash: profile.sourcePublicCatalogHash,
    octoberExecutionProfile: profile.profileVersion,
    octoberSourceMainSha256: profile.sourceMainSha256,
  };
}

function switcheroo(sourcePath, parserPath) {
  const seed = 169;
  const context = start(sourcePath, parserPath, seed);
  finishOpening(context, "switcheroo");
  assert.equal(read(context, "playerDeck('white').some(card=>card?.id==='switcheroo')"), true);
  const card = read(context, "collectValidAiActions('white',{includeCards:true,exhaustiveCards:true}).find(action=>action.type==='card'&&action.cardId==='switcheroo')");
  const cardBefore = projection(context);
  apply(context, card);
  const actions = read(context, "collectValidAiActions('white',{includeCards:false,exhaustiveCards:true})");
  const move = actions.find(action => action.type === "move" &&
    action.move?.switcherooMove && action.move.row === 6 && action.move.col === 0);
  assert.ok(move, "Original source did not offer king/pawn switcheroo.");
  const before = projection(context);
  const king = before.board.find(piece => piece.row === 7 && piece.col === 4);
  const pawn = before.board.find(piece => piece.row === 6 && piece.col === 0);
  assert.equal(king.type, "king");
  assert.equal(pawn.type, "pawn");
  apply(context, move);
  const after = projection(context);
  assert.equal(after.board.find(piece => piece.id === king.id)?.row, 6);
  assert.equal(after.board.find(piece => piece.id === king.id)?.col, 0);
  assert.equal(after.board.find(piece => piece.id === pawn.id)?.row, 7);
  assert.equal(after.board.find(piece => piece.id === pawn.id)?.col, 4);
  assert.equal(after.moveCount, before.moveCount + 1);
  const result = receipt("switcheroo", context, seed, before, after,
    { openingChoice: "switcheroo", otherOpeningChoice: "first offered", cardAction: card,
      publicIntent: moveIntent(move), sourceAction: move },
    "source switcheroo transition and visible board only");
  const {sourceState: cardBeforeState, ...cardBeforeProjection} = cardBefore;
  result.sourceExpected.cardBefore = cardBeforeProjection;
  result.sourceExpected.cardAfter = result.sourceExpected.before;
  result.sourceEvidence.cardBeforeState = cardBeforeState;
  result.rustInput = {
    cardBefore: switcherooRustInput(cardBeforeState),
    before: switcherooRustInput(before.sourceState),
  };
  return result;
}

function holdout(sourcePath, parserPath) {
  const seed = 74;
  const context = start(sourcePath, parserPath, seed, { deathmatch: false });
  finishOpening(context, "holdout");
  const card = cardAction(context, "holdout",
    actions => actions.find(action => action.target?.row === 6 && action.target?.col === 0));
  const pawnId = run(context, "state.board[6][0].id");
  assert.equal(run(context, "state.board[6][0].holdoutPromotion.readyTurn"), 28);
  const actions = [];
  let before;
  for (let step = 0; step < 140; step++) {
    if (run(context, "state.mode") === "draft") {
      const choice = read(context, "state.draft.choices[0]");
      const accepted = run(context, "(()=>{const color=state.draft.color,phase=state.draft.phase,card=state.draft.choices[0];const ok=finishDraft(card,null,{auto:true});if(ok)completeDraftStep(color,phase);return ok})()");
      assert.equal(accepted, true);
      actions.push({ type: "draft", choice });
      continue;
    }
    assert.equal(run(context, "state.mode"), "play", "Source ended before holdout promotion.");
    const choices = read(context, "collectValidAiActions(state.turn,{includeCards:false,exhaustiveCards:false}).filter(action=>action.type==='move'&&!(action.from.row===6&&action.from.col===0)&&!state.board[action.move.row][action.move.col]&&!action.move.enPassant&&state.board[action.from.row][action.from.col]?.type!=='king')");
    assert.ok(choices.length, "Source offered no quiet move before holdout promotion.");
    const action = choices[(step * 17 + 3) % choices.length];
    apply(context, action);
    actions.push(action);
    const shared = run(context, "sharedTurnCount()");
    if (shared === 27) {
      before = projection(context);
      assert.equal(before.board.find(piece => piece.id === pawnId)?.type, "pawn");
    }
    if (shared === 28) {
      const after = projection(context);
      assert.ok(before, "Missing shared turn 27 boundary.");
      assert.equal(after.board.find(piece => piece.id === pawnId)?.type, "queen");
      assert.equal(after.moveCount, 56);
      return receipt("holdout", context, seed, before, after,
        { openingChoice: "holdout", cardAction: card, actions,
          publicIntent: moveIntent(action) },
        "source 27/28 shared-turn promotion boundary and visible board only");
    }
  }
  throw new Error("Source did not reach shared turn 28 within 140 decisions.");
}

function monster(sourcePath, parserPath) {
  const context = loadSource(sourcePath, parserPath);
  context.__fixedRandom = () => 0.1;
  run(context, "Math.random=__fixedRandom;selectedGameStyle='normal';localPlayMode='local';playMode='local';ruleOpeningEnabled=true;ruleSelectionEnabled=true;selectedRuleCardIds=['monster'];resetGame(false,[]);maybeApplyOpeningRuleEvent();beginInitialGameFlow()");
  context.__initialSourceState = read(context, "state");
  assert.equal(run(context, "state.ruleOpeningEvent.status"), "hit");
  assert.equal(run(context, "state.appliedRuleCard.id"), "monster");
  const draftActions = [];
  for (let step = 0; step < 16 && run(context, "state.mode==='draft'"); step++) {
    draftActions.push(read(context, "state.draft.choices[0]"));
    assert.equal(run(context, "(()=>{const color=state.draft.color,phase=state.draft.phase,card=state.draft.choices[0];const ok=finishDraft(card,null,{auto:true});if(ok)completeDraftStep(color,phase);return ok})()"), true);
  }
  assert.equal(run(context, "state.mode"), "play");
  const actions = [];
  for (let step = 0; step < 2; step++) {
    const action = firstMove(context);
    apply(context, action);
    actions.push(action);
  }
  const before = projection(context);
  const prior = before.board.find(piece => piece.type === "monster");
  assert.ok(prior, "RULE monster was not spawned.");
  const action = firstMove(context);
  apply(context, action);
  actions.push(action);
  const after = projection(context);
  const moved = after.board.find(piece => piece.id === prior.id);
  assert.ok(moved && (moved.row !== prior.row || moved.col !== prior.col));
  assert.equal(after.moveCount, 3);
  return receipt("monster", context, null, before, after,
    { sourceRandom: "fixed 0.1 per draw", ruleSelection: ["monster"], draftActions, actions,
      publicIntent: moveIntent(action) },
    "selected opening RULE and third-half-move transition; one deterministic RNG path and visible board only",
    null, ["monster"]);
}

function chimera(sourcePath, parserPath) {
  const seed = 17;
  const context = start(sourcePath, parserPath, seed);
  finishOpening(context, "chimera");
  const card = cardAction(context, "chimera");
  const first = read(context, "collectValidAiActions('white',{includeCards:false,exhaustiveCards:false}).find(action=>action.type==='move'&&action.from.row===6&&action.from.col===3&&action.move.row===5&&action.move.col===3)");
  apply(context, first);
  const second = firstMove(context);
  apply(context, second);
  const before = projection(context);
  const queen = before.board.find(piece => piece.row === 7 && piece.col === 3);
  assert.equal(queen?.type, "queen");
  assert.equal(queen.chimera, true);
  const types = read(context, "chimeraMajorTypes(state)");
  const action = read(context, "collectValidAiActions('white',{includeCards:false,exhaustiveCards:false}).find(action=>action.type==='move'&&action.from.row===7&&action.from.col===3&&action.move.row===6&&action.move.col===3)");
  apply(context, action);
  const after = projection(context);
  const transformed = after.board.find(piece => piece.id === queen.id);
  assert.ok(types.includes(transformed?.type));
  return receipt("chimera", context, seed, before, after,
    { openingChoice: "chimera", cardAction: card, actions: [first, second, action], sourceTypes: types,
      publicIntent: moveIntent(action) },
    "source queen move and sampled transformation; full transition distribution unverified");
}

function reaper(sourcePath, parserPath) {
  const seed = 66;
  const context = start(sourcePath, parserPath, seed);
  finishOpening(context, "reaper");
  const before = projection(context);
  const queen = before.board.find(piece => piece.row === 7 && piece.col === 3);
  assert.equal(queen?.type, "queen");
  const card = cardAction(context, "reaper");
  const after = projection(context);
  assert.equal(after.board.find(piece => piece.id === queen.id)?.type, "reaper");
  return receipt("reaper", context, seed, before, after,
    { openingChoice: "reaper", cardAction: card, publicIntent: cardIntent(card) },
    "source card acquisition and queen transformation; allied captures and win unverified");
}

function compare(sourceReceipt, actual) {
  assert.equal(actual.schemaVersion, sourceReceipt.schemaVersion);
  assert.equal(actual.caseName, sourceReceipt.caseName);
  assert.equal(actual.rulesVersion, sourceReceipt.rulesVersion);
  assert.equal(actual.sourceMainSha256, sourceReceipt.sourceMainSha256);
  assert.equal(actual.sourcePublicCatalogHash, sourceReceipt.sourcePublicCatalogHash);
  assert.equal(actual.executionProfileVersion, sourceReceipt.executionProfileVersion);
  assert.deepEqual(actual.publicIntent, sourceReceipt.replay.publicIntent);
  for (const key of ["before", "after"]) {
    assert.deepEqual(actual[key], sourceReceipt.sourceExpected[key], `${key} differs from original October source`);
  }
}

function main(argv) {
  const [sourcePath, parserPath, caseName, ...rest] = argv;
  if (!sourcePath || !parserPath || !path.isAbsolute(sourcePath) || !path.isAbsolute(parserPath))
    throw new TypeError("Usage: october-source-transition.cjs ABSOLUTE_MAIN ABSOLUTE_ACORN CASE [--actual ABSOLUTE_JSON]");
  const cases = { switcheroo, holdout, monster, chimera, reaper };
  if (!Object.hasOwn(cases, caseName)) throw new TypeError("CASE must be switcheroo, holdout, monster, chimera, or reaper.");
  if (rest.length && !(rest.length === 2 && rest[0] === "--actual" && path.isAbsolute(rest[1])))
    throw new TypeError("Expected only --actual ABSOLUTE_JSON.");
  const receipt = cases[caseName](sourcePath, parserPath);
  if (rest.length) {
    const actual = JSON.parse(fs.readFileSync(rest[1], "utf8"));
    compare(receipt, actual);
    process.stdout.write(JSON.stringify({ status: "pass", case: caseName, sourceMainSha256: receipt.sourceMainSha256 }) + "\n");
  } else {
    process.stdout.write(JSON.stringify(receipt) + "\n");
  }
}

if (require.main === module) {
  try { main(process.argv.slice(2)); }
  catch (error) { console.error(error.stack || error); process.exitCode = 1; }
}
module.exports = { switcheroo, holdout, monster, chimera, reaper, compare };
