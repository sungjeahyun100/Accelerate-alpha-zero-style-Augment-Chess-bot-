"use strict";

const assert = require("node:assert/strict");
const { test } = require("node:test");
const vm = require("node:vm");
const { loadSource, probe } = require("./october-source-probe");

const source = process.env.OCTOBER_SOURCE_MAIN;
const parser = process.env.OCTOBER_ACORN_PARSER;
const run = (context, code) => vm.runInContext(code, context, { timeout: 15000 });
const read = (context, code) => JSON.parse(run(context, `JSON.stringify(${code})`));

function game() {
  assert.ok(source && parser, "Set OCTOBER_SOURCE_MAIN and OCTOBER_ACORN_PARSER to absolute paths.");
  const context = loadSource(source, parser);
  context.__fixedRandom = () => 0.5;
  run(context, "Math.random=__fixedRandom;selectedGameStyle='normal';localPlayMode='local';playMode='local';resetGame(false,[]);beginInitialGameFlow();");
  for (let step = 0; step < 16 && run(context, "state.mode==='draft'"); step++) {
    assert.equal(run(context, "(()=>{const color=state.draft.color,phase=state.draft.phase,card=state.draft.choices[0];if(!card)return false;const ok=finishDraft(card,null,{auto:true});if(ok)completeDraftStep(color,phase);return ok;})()"), true);
  }
  assert.equal(run(context, "state.mode"), "play");
  assert.equal(run(context, "usesOctober7Balance(state)"), true);
  return context;
}

test("source digest rejects an altered bundle", () => {
  assert.throws(() => loadSource(parser, parser), /October source SHA-256 mismatch/);
});

for (const style of ["normal", "chaos", "grand"]) {
  test(`source initializes and accepts a draft choice in ${style}`, () => {
    const result = probe(source, parser, style);
    assert.equal(result.applied.ok, true);
    assert.ok(result.before.choices.length > 0);
  });
}

test("normal source move advances the turn; invalid source action is rejected", () => {
  const context = game();
  assert.deepEqual(read(context, "({chimera:CARD_BY_ID.get('chimera')?.target,chimeraStars:CARD_BY_ID.get('chimera')?.stars,switcherooStars:CARD_BY_ID.get('switcheroo')?.stars,holdoutText:CARD_BY_ID.get('holdout')?.text.includes('28')})"),
    { chimera: "own-queen", chimeraStars: 4, switcherooStars: 2, holdoutText: true });
  assert.equal(read(context, "applyAiAction({type:'move',color:'white',from:{row:7,col:4},move:{row:0,col:0}})").ok, false);
  const action = read(context, "collectValidAiActions('white',{includeCards:false,exhaustiveCards:false}).find(a=>a.type==='move')");
  context.__action = action;
  assert.equal(read(context, "applyAiAction(__action)").ok, true);
  assert.equal(run(context, "state.turn"), "black");
  assert.equal(run(context, "state.moveCount"), 1);
});

// The following fixtures invoke original source rule functions on a normally
// initialized board. Card ownership is bypassed, so these are synthetic card
// effect fixtures and do not prove deck binding or legal card action admission.
test("switcheroo source effect enables a legal king-pawn swap with both identities retained", () => {
  const context = game();
  const original = read(context, "({king:state.board[7][4].id,pawn:state.board[6][0].id})");
  assert.equal(read(context, "switcheroo()").ok, true);
  const action = read(context, "collectValidAiActions('white',{includeCards:false,exhaustiveCards:false}).find(a=>a.move?.switcherooMove&&a.move.row===6&&a.move.col===0)");
  assert.ok(action);
  context.__action = action;
  assert.equal(read(context, "applyAiAction(__action)").ok, true);
  assert.deepEqual(read(context, "({king:state.board[6][0].id,pawn:state.board[7][4].id,moveCount:state.moveCount,turn:state.turn})"),
    { ...original, moveCount: 1, turn: "black" });
});

test("holdout source effect promotes at the 28 shared-turn boundary", () => {
  const context = game();
  assert.equal(read(context, "holdout({row:6,col:0})").ok, true);
  assert.equal(run(context, "state.board[6][0].holdoutPromotion.readyTurn"), 28);
  run(context, "state.turnsTaken={white:27,black:27}");
  assert.equal(run(context, "resolveHoldoutPromotions('white')"), 0);
  assert.equal(run(context, "state.board[6][0].type"), "pawn");
  run(context, "state.turnsTaken={white:28,black:28}");
  assert.equal(run(context, "resolveHoldoutPromotions('white')"), 1);
  assert.equal(run(context, "state.board[6][0].type"), "queen");
});

test("chimera source rejects a knight, accepts a queen, and enumerates source RNG outcomes", () => {
  const context = game();
  assert.equal(read(context, "chimera({row:7,col:1})").ok, false);
  assert.equal(read(context, "chimera({row:7,col:3})").ok, true);
  assert.equal(run(context, "state.board[7][3].chimera"), true);
  const types = read(context, "chimeraMajorTypes(state)");
  assert.ok(types.length > 1);
  assert.equal(new Set(types).size, types.length, "Each source major type must occupy one uniform RNG interval.");
  const observed = [];
  for (let index = 0; index < types.length; index++) {
    context.__roll = (index + 0.5) / types.length;
    observed.push(run(context, "(()=>{Math.random=()=>__roll;return chooseChimeraNextType('queen')})()"));
  }
  assert.deepEqual(observed, types);
});

test("monster source timing fires only after positive multiples of three half-moves", () => {
  const context = game();
  assert.deepEqual(read(context, "[0,1,2,3,4,5,6].map(n=>monsterMoveDue(state,n,'white'))"),
    [false, false, false, true, false, false, true]);
  assert.equal(read(context, "monsterRule()").ok, true);
  assert.equal(read(context, "state.board.flat().filter(p=>p?.type==='monster').length"), 1);
});

test("spawned monster moves automatically after the third applied half-move", () => {
  const context = game();
  for (let index = 0; index < 2; index++) {
    context.__action = read(context, "collectValidAiActions(state.turn,{includeCards:false,exhaustiveCards:false}).find(a=>a.type==='move')");
    assert.equal(read(context, "applyAiAction(__action)").ok, true);
  }
  assert.equal(run(context, "state.moveCount"), 2);
  assert.equal(read(context, "monsterRule()").ok, true);
  const before = read(context, "state.board.flatMap((row,r)=>row.map((p,c)=>p?.type==='monster'?{row:r,col:c,id:p.id}:null).filter(Boolean))");
  context.__action = read(context, "collectValidAiActions(state.turn,{includeCards:false,exhaustiveCards:false}).find(a=>a.type==='move')");
  assert.equal(read(context, "applyAiAction(__action)").ok, true);
  const after = read(context, "state.board.flatMap((row,r)=>row.map((p,c)=>p?.type==='monster'?{row:r,col:c,id:p.id}:null).filter(Boolean))");
  assert.equal(run(context, "state.moveCount"), 3);
  assert.equal(after.length, 1);
  assert.equal(after[0].id, before[0].id);
  assert.notDeepEqual(after[0], before[0]);
});

test("reaper source effect and allied-capture filter", () => {
  const context = game();
  assert.equal(read(context, "reaper({row:7,col:1})").ok, false);
  assert.equal(read(context, "reaper({row:7,col:3})").ok, true);
  assert.equal(run(context, "state.board[7][3].type"), "reaper");
  assert.equal(run(context, "reaperCaptureTarget(state)"), 2);
  assert.deepEqual(read(context, "[['white','white','black'],['white','black','white'],['white','white','white']].map(([a,b,c])=>reaperSoulCountsCapture(a,b,c,state))"),
    [true, false, false]);
});

test("source visibility gives each viewer a distinct board without leaking hidden piece fields", () => {
  const context = game();
  const whiteMove = read(context, "collectValidAiActions('white',{includeCards:false,exhaustiveCards:false}).find(a=>a.type==='move')");
  context.__action = whiteMove;
  assert.equal(read(context, "applyAiAction(__action)").ok, true);
  assert.equal(run(context, "state.turn"), "black");
  assert.equal(read(context, "stealth({row:0,col:2})").ok, true);
  const white = read(context, "__publicPieceView(state.board[0][2],0,2,'white')");
  const black = read(context, "__publicPieceView(state.board[0][2],0,2,'black')");
  assert.equal(white, null);
  assert.equal(black.type, "bishop");
  assert.equal(Object.hasOwn(black, "hiddenFrom"), false);
  assert.equal(read(context, "__publicPieceView(state.board[0][3],0,3,'white')").type, "queen");
});
