"use strict";

const assert = require("node:assert/strict");
const { test } = require("node:test");
const vm = require("node:vm");
const { loadSource } = require("./october-source-probe");

const source = process.env.OCTOBER_SOURCE_MAIN;
const parser = process.env.OCTOBER_ACORN_PARSER;
const run = (context, code) => vm.runInContext(code, context, { timeout: 15000 });
const read = (context, code) => JSON.parse(run(context, `JSON.stringify(${code})`));
const seeds = Object.freeze({ holdout: 74, switcheroo: 169, chimera: 17, reaper: 66 });

function start(seed) {
  assert.ok(source && parser, "Set OCTOBER_SOURCE_MAIN and OCTOBER_ACORN_PARSER.");
  const context = loadSource(source, parser);
  context.__seed = seed;
  context.__random = () => {
    let value = context.__seed;
    value ^= value << 13;
    value ^= value >>> 17;
    value ^= value << 5;
    context.__seed = value;
    return (value >>> 0) / 0x100000000;
  };
  run(context, "Math.random=__random;selectedGameStyle='normal';localPlayMode='local';playMode='local';ruleSelectionEnabled=false;selectedRuleCardIds=[];resetGame(false,[]);beginInitialGameFlow();");
  assert.equal(run(context, "state.mode"), "draft");
  return context;
}

function acquire(id) {
  const context = start(seeds[id]);
  context.__cardId = id;
  assert.equal(read(context, "state.draft.choices.some(card=>card.id===__cardId)"), true,
    `${id} must be offered by the source's unmodified weighted draft`);
  assert.equal(read(context, "draftPoolForCategories(['OPENING','MIDDLE','PIECE'],'white').some(card=>card.id===__cardId)"), true);
  assert.equal(run(context, "(()=>{const color=state.draft.color,phase=state.draft.phase,card=state.draft.choices.find(c=>c.id===__cardId);const ok=finishDraft(card,null,{auto:true});if(ok)completeDraftStep(color,phase);return ok})()"), true);
  assert.equal(read(context, "playerDeck('white').some(card=>card?.id===__cardId)"), true);
  assert.equal(run(context, "state.draft.color"), "black");
  assert.equal(run(context, "(()=>{const color=state.draft.color,phase=state.draft.phase,card=state.draft.choices[0];const ok=finishDraft(card,null,{auto:true});if(ok)completeDraftStep(color,phase);return ok})()"), true);
  assert.equal(run(context, "state.mode"), "play");
  assert.equal(run(context, "state.turn"), "white");
  return context;
}

function play(context, action) {
  assert.ok(action, "The source must generate the requested action.");
  context.__action = action;
  assert.equal(read(context, "applyAiAction(__action)").ok, true);
}

function cardAction(context, id, select = actions => actions[0]) {
  context.__cardId = id;
  const actions = read(context, "collectValidAiActions('white',{includeCards:true,exhaustiveCards:true}).filter(action=>action.type==='card'&&action.cardId===__cardId)");
  assert.ok(actions.length > 0, `${id} is owned but has no source legal card action`);
  play(context, select(actions));
  assert.equal(read(context, "playerDeck('white').find(card=>card?.id===__cardId)?.used"), true);
}

test("four P0 cards occur in actual weighted opening offers and can be selected", () => {
  for (const id of Object.keys(seeds)) {
    const context = acquire(id);
    assert.equal(read(context, "playerDeck('white').filter(Boolean).length"), 1);
    assert.equal(read(context, "state.winner"), null);
  }
});

test("draft-acquired holdout and switcheroo apply through source actions", () => {
  const holdout = acquire("holdout");
  cardAction(holdout, "holdout", actions => actions.find(action => action.target?.row === 6 && action.target?.col === 0));
  assert.equal(run(holdout, "state.board[6][0].holdoutPromotion.readyTurn"), 28);
  play(holdout, read(holdout, "collectValidAiActions('white',{includeCards:false,exhaustiveCards:false}).find(action=>action.type==='move')"));
  assert.equal(run(holdout, "state.moveCount"), 1);
  assert.equal(run(holdout, "state.turn"), "black");

  const switcheroo = acquire("switcheroo");
  const before = read(switcheroo, "({king:state.board[7][4].id,pawn:state.board[6][0].id})");
  cardAction(switcheroo, "switcheroo");
  const swap = read(switcheroo, "collectValidAiActions('white',{includeCards:false,exhaustiveCards:false}).find(action=>action.move?.switcherooMove&&action.move.row===6&&action.move.col===0)");
  play(switcheroo, swap);
  assert.deepEqual(read(switcheroo, "({king:state.board[6][0].id,pawn:state.board[7][4].id,turn:state.turn,moveCount:state.moveCount})"),
    { ...before, turn: "black", moveCount: 1 });
});

test("draft-acquired chimera and reaper transform the source queen", () => {
  for (const id of ["chimera", "reaper"]) {
    const context = acquire(id);
    cardAction(context, id);
    assert.equal(run(context, id === "chimera" ? "state.board[7][3].chimera" : "state.board[7][3].type==='reaper'"), true);
    play(context, read(context, "collectValidAiActions('white',{includeCards:false,exhaustiveCards:false}).find(action=>action.type==='move')"));
    assert.equal(run(context, "state.turn"), "black");
    assert.equal(run(context, "state.moveCount"), 1);
  }
});

test("monster is a RULE event, outside the ordinary draft pool", () => {
  const context = start(1);
  assert.equal(run(context, "CARD_CATEGORY_BY_ID.monster"), "RULE");
  assert.equal(read(context, "draftPoolForCategories(['OPENING','MIDDLE','PIECE'],'white').some(card=>card.id==='monster')"), false);
  assert.equal(read(context, "ruleCardPool().some(card=>card.id==='monster')"), true);
  assert.equal(read(context, "state.draft.choices.some(card=>card.id==='monster')"), false);
});

test("both viewers receive distinct source board projections after stealth", () => {
  const context = start(1);
  for (let step = 0; step < 2; step++) {
    const color = run(context, "state.draft.color");
    assert.equal(run(context, "(()=>{const color=state.draft.color,phase=state.draft.phase,card=state.draft.choices[0];const ok=finishDraft(card,null,{auto:true});if(ok)completeDraftStep(color,phase);return ok})()"), true, color);
  }
  play(context, read(context, "collectValidAiActions('white',{includeCards:false,exhaustiveCards:false}).find(action=>action.type==='move')"));
  assert.equal(run(context, "state.turn"), "black");
  assert.equal(read(context, "stealth({row:0,col:2})").ok, true);
  const views = read(context, "Object.fromEntries(['white','black'].map(viewer=>[viewer,state.board.map((row,r)=>row.map((piece,c)=>piece?__publicPieceView(piece,r,c,viewer):null))]))");
  assert.equal(views.white[0][2], null);
  assert.equal(views.black[0][2].type, "bishop");
  assert.equal(Object.hasOwn(views.black[0][2], "hiddenFrom"), false);
  assert.equal(views.white[0][3].type, "queen");
  assert.equal(views.black[0][3].type, "queen");
  const whiteVisible = views.white.flat().filter(Boolean).length;
  const blackVisible = views.black.flat().filter(Boolean).length;
  assert.equal(blackVisible - whiteVisible, 1);
});
