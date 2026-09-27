"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const { OfflineOracle } = require("./offline-oracle");
const contract = require("../../../bridge/tools/runtime-contract");
const { validate, resolveRef } = require("../../../bridge/tools/validate");
const oracle = new OfflineOracle(); // Deliberately fail when the adopted external baseline is unavailable.
const schema = resolveRef("runtime-v1.schema.json#", "runtime-v1.schema.json");
function assertSchema(value) { assert.deepEqual(validate(schema.schema, value, "$", schema.docName), []); }

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
