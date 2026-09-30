"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const { FrozenClientSource, GameAdapter } = require("../../oracle/game-adapter/src");
const { OracleRuntime } = require("../../oracle/game-adapter/src/game-adapter");
const { createRuntimeContract } = require("../../contracts/tools/runtime-contract");

const CLIENT_SHA256 = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
const parent = process.env.RUNNER_TEMP || process.env.APPDATA;
const root = process.env.ACCELERATE_SITE_BASELINE_LATEST || process.env.ACCELERATE_SITE_BASELINE ||
  (parent && path.join(parent, "Accelerate", "cache", "site-baseline-20260928-e5ed84fc"));
if (!root || !path.isAbsolute(root)) throw new Error("A pinned absolute v7 client root is required.");
const contract = createRuntimeContract({ baseline: "site-20260928" });
const source = new FrozenClientSource(root, { expectedClientSha256: CLIENT_SHA256 });

function playPosition() {
  const adapter = new GameAdapter({ source, contract });
  let position = adapter.newGame({ gameStyle: "normal" }, 12345);
  for (let count = 0; position.state.mode === "draft" && count < 32; count++) {
    const offered = adapter.actions(position);
    const choice = offered[position.state.draft.color === "white" ? 1 : 0];
    assert.ok(choice, "source draft offer");
    const step = adapter.apply(position, choice, { recordHistory: false });
    assert.equal(step.ok, true);
    position = step.position;
  }
  assert.equal(position.state.mode, "play");
  adapter.dispose();
  return position;
}

const base = playPosition();
function cardPosition(cardId, bounded = true) {
  const direct = new OracleRuntime({ source, contract });
  direct.restore(base);
  direct.main.context.__cardId = cardId;
  // Source-created draft/board, with only the target surface narrowed. Every
  // candidate can be exhausted without a stored fixture or prefix probe.
  direct.evaluate(`
    state.deckSlots.white=[cloneCard(CARD_DEFS.find(card=>card.id===__cardId)),null,null];
    state.deckSlots.black=[null,null,null];
    state.deck.white=state.deckSlots.white.filter(Boolean);state.deck.black=[];
    state.playerCards=state.deckSlots;
    if(${bounded}) {
      if(__cardId==='panic') {
        for(let row=0;row<8;row++)for(let col=0;col<8;col++)
          if(state.board[row][col]?.color==='black'&&
             !(row===0&&(col===0||col===1||col===4)))state.board[row][col]=null;
      } else {
        const openColumns=__cardId==='hypocrisy'?4:2;
        for(let row=0;row<8;row++)for(let col=0;col<8;col++)
          if(state.board[row][col]===null&&!(row===2&&col<openColumns))
            state.board[row][col]=piece('neutral','wall');
      }
    }
  `);
  return direct.snapshot();
}

function permutations(squares) {
  const result = [];
  function visit(selected, remaining) {
    if (!remaining.length) { result.push(selected); return; }
    for (let index = 0; index < remaining.length; index++)
      visit([...selected, remaining[index]], remaining.filter((_, other) => other !== index));
  }
  visit([], squares);
  return result;
}

for (const { cardId, squares } of [
  { cardId: "portal-gun", squares: [{ row: 2, col: 0 }, { row: 2, col: 1 }] },
  { cardId: "hypocrisy", squares: [0, 1, 2, 3].map(col => ({ row: 2, col })) },
  { cardId: "panic", squares: [{ row: 0, col: 0 }, { row: 0, col: 1 }] },
]) {
  test(`v7 ${cardId} stream exhausts each source-accepted ordered choice once`, () => {
    const position = cardPosition(cardId);
    const expected = permutations(squares);
    const adapter = new GameAdapter({ source, contract });
    const eager = adapter.actions(position).filter(candidate => candidate.payload.cardId === cardId);
    assert.deepEqual(eager.map(candidate => candidate.payload.target.selections), expected);
    const cursor = adapter.actionStream(position, { cardId });
    const actions = [];
    let exhausted = false;
    for (let pages = 0; pages < 12 && !exhausted; pages++) {
      const page = cursor.nextPage(3, { maxExamined: 3 });
      assert.ok(page.examined <= 3);
      actions.push(...page.actions);
      exhausted = page.exhausted;
    }
    cursor.dispose();
    assert.equal(exhausted, true, "the entire small target surface must be examined");
    assert.deepEqual(actions.map(candidate => candidate.payload.target.selections), expected);
    assert.equal(new Set(actions.map(candidate => candidate.actionId)).size, expected.length);

    const outcomes = new Map();
    for (const candidate of actions) {
      const direct = new OracleRuntime({ source, contract });
      direct.restore(position);
      direct.main.context.__action = contract.jsonCopy(candidate.payload);
      assert.equal(direct.evaluate("applyAiAction(__action)").ok, true);
      const sourceResult = direct.snapshot();
      const step = adapter.apply(position, candidate, { recordHistory: false });
      assert.equal(step.ok, true);
      assert.deepEqual(step.position.state, sourceResult.state);
      assert.deepEqual(step.position.rng, sourceResult.rng);
      outcomes.set(contract.canonical(candidate.payload.target.selections), sourceResult.positionId);
    }
    assert.notEqual(outcomes.get(contract.canonical(expected[0])),
      outcomes.get(contract.canonical([squares[1], squares[0], ...squares.slice(2)])),
      "the source distinguishes reversed clicks in full Position");
    adapter.dispose();
  });
}

test("large hypocrisy family stays bounded in actions and pageable in actionStream", () => {
  const position = cardPosition("hypocrisy", false);
  const adapter = new GameAdapter({ source, contract });
  assert.throws(() => adapter.actions(position), /candidate budget; use actionStream/);
  const cursor = adapter.actionStream(position, { cardId: "hypocrisy", legal: false });
  const first = cursor.nextPage(2, { maxExamined: 2 });
  const second = cursor.nextPage(2, { maxExamined: 2 });
  assert.equal(first.examined, 2);
  assert.equal(second.examined, 2);
  assert.equal(first.stopReason, "page-limit");
  assert.equal(second.stopReason, "page-limit");
  assert.equal(new Set([...first.actions, ...second.actions].map(candidate => candidate.actionId)).size, 4);
  cursor.dispose();
  assert.throws(() => cursor.nextPage(), /disposed/);
  adapter.dispose();
});
