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


test("selected opening RULE monster fires before draft and moves after three source actions", () => {
  assert.ok(source && parser);
  const context = loadSource(source, parser);
  context.__fixedRandom = () => 0.1;
  run(context, "Math.random=__fixedRandom;selectedGameStyle='normal';localPlayMode='local';playMode='local';ruleOpeningEnabled=true;ruleSelectionEnabled=true;selectedRuleCardIds=['monster'];resetGame(false,[]);maybeApplyOpeningRuleEvent();beginInitialGameFlow()");
  assert.deepEqual(read(context, "({enabled:state.ruleSelectionEnabled,selected:state.selectedRuleCardIds,event:state.ruleOpeningEvent.status,applied:state.appliedRuleCard.id})"),
    { enabled: true, selected: ["monster"], event: "hit", applied: "monster" });
  assert.equal(read(context, "state.board.flat().filter(piece=>piece?.type==='monster').length"), 1);
  for (let step = 0; step < 16 && run(context, "state.mode==='draft'"); step++) {
    assert.equal(run(context, "(()=>{const color=state.draft.color,phase=state.draft.phase,card=state.draft.choices[0];const ok=finishDraft(card,null,{auto:true});if(ok)completeDraftStep(color,phase);return ok})()"), true);
  }
  assert.equal(run(context, "state.mode"), "play");
  const monster = () => read(context, "state.board.flatMap((row,r)=>row.map((piece,c)=>piece?.type==='monster'?{id:piece.id,row:r,col:c}:null).filter(Boolean))");
  const start = monster();
  assert.equal(start.length, 1);
  for (let step = 1; step <= 3; step++) {
    const action = read(context, "collectValidAiActions(state.turn,{includeCards:false,exhaustiveCards:false}).find(action=>action.type==='move')");
    play(context, action);
    assert.equal(run(context, "state.moveCount"), step);
    if (step < 3) assert.deepEqual(monster(), start);
  }
  const end = monster();
  assert.equal(end.length, 1);
  assert.equal(end[0].id, start[0].id);
  assert.notDeepEqual(end, start);
});

test("draft-acquired chimera transforms through an actual queen move", () => {
  const context = acquire("chimera");
  cardAction(context, "chimera");
  const queenId = run(context, "state.board[7][3].id");
  const move = (fromRow, fromCol, toRow, toCol) => {
    const action = read(context, `collectValidAiActions(state.turn,{includeCards:false,exhaustiveCards:false}).find(action=>action.type==='move'&&action.from.row===${fromRow}&&action.from.col===${fromCol}&&action.move.row===${toRow}&&action.move.col===${toCol})`);
    play(context, action);
  };
  move(6, 3, 5, 3);
  const black = read(context, "collectValidAiActions('black',{includeCards:false,exhaustiveCards:false}).find(action=>action.type==='move')");
  play(context, black);
  const possible = read(context, "chimeraMajorTypes(state)");
  move(7, 3, 6, 3);
  const transformed = read(context, "({id:state.board[6][3].id,type:state.board[6][3].type,chimera:state.board[6][3].chimera,moveCount:state.moveCount,turn:state.turn})");
  assert.equal(transformed.id, queenId);
  assert.equal(transformed.chimera, true);
  assert.ok(possible.includes(transformed.type));
  assert.equal(transformed.moveCount, 3);
  assert.equal(transformed.turn, "black");
});


test("draft-acquired holdout promotes after 28 completed shared turns of source legal play", () => {
  assert.ok(source && parser);
  const context = loadSource(source, parser);
  context.__seed = seeds.holdout;
  context.__random = () => {
    let value = context.__seed;
    value ^= value << 13;
    value ^= value >>> 17;
    value ^= value << 5;
    context.__seed = value;
    return (value >>> 0) / 0x100000000;
  };
  run(context, "Math.random=__random;selectedGameStyle='normal';localPlayMode='local';playMode='local';ruleSelectionEnabled=false;selectedRuleCardIds=[];deathmatchEnabled=false;resetGame(false,[]);beginInitialGameFlow()");
  for (let pick = 0; pick < 2; pick++) {
    assert.equal(run(context, "(()=>{const color=state.draft.color,phase=state.draft.phase,card=state.draft.choices.find(card=>card.id==='holdout')||state.draft.choices[0];const ok=finishDraft(card,null,{auto:true});if(ok)completeDraftStep(color,phase);return ok})()"), true);
  }
  assert.equal(run(context, "state.mode"), "play");
  cardAction(context, "holdout", actions => actions.find(action => action.target?.row === 6 && action.target?.col === 0));
  const pawnId = run(context, "state.board[6][0].id");
  assert.equal(run(context, "state.board[6][0].holdoutPromotion.readyTurn"), 28);
  let observed27 = false;
  for (let step = 0; step < 140; step++) {
    if (run(context, "state.mode") === "draft") {
      assert.equal(run(context, "(()=>{const color=state.draft.color,phase=state.draft.phase,card=state.draft.choices[0];if(!card)return false;const ok=finishDraft(card,null,{auto:true});if(ok)completeDraftStep(color,phase);return ok})()"), true);
      continue;
    }
    assert.equal(run(context, "state.mode"), "play", "Source game ended before holdout promotion.");
    const actions = read(context, "collectValidAiActions(state.turn,{includeCards:false,exhaustiveCards:false}).filter(action=>action.type==='move'&&!(action.from.row===6&&action.from.col===0)&&!state.board[action.move.row][action.move.col]&&!action.move.enPassant&&state.board[action.from.row][action.from.col]?.type!=='king')");
    assert.ok(actions.length > 0, "Source has no quiet legal move.");
    play(context, actions[(step * 17 + 3) % actions.length]);
    const shared = run(context, "sharedTurnCount()");
    if (shared === 27) {
      observed27 = true;
      assert.equal(run(context, "state.board[6][0].type"), "pawn");
    }
    if (shared === 28) {
      assert.equal(observed27, true);
      assert.deepEqual(read(context, "({id:state.board[6][0].id,type:state.board[6][0].type,promoted:state.board[6][0].promotedFromPawn,turnsTaken:state.turnsTaken,winner:state.winner})"),
        { id: pawnId, type: "queen", promoted: true, turnsTaken: { white: 28, black: 28 }, winner: null });
      return;
    }
  }
  assert.fail("Source did not reach 28 completed shared turns within the bounded playout.");
});
