"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { options, replayAbsence, replayDraftChoice, validateReplayWitness,
  caseBudgetContext, caseBudgetError, jsonBudgetFailure, sourceReplaySequence } = require("./v7-native-differential.cjs");

test("Replay search has finite seed and decision bounds", () => {
  assert.deepEqual(options(["--replay-search=normal:10:2:64"]).replaySearches,
    [{ style: "normal", startSeed: 10, seedCount: 2, decisions: 64 }]);
  for (const value of ["normal:0:33:1", "normal:0:1:129", "normal:0:0:1", "normal:bad:1:2"])
    assert.throws(() => options([`--replay-search=${value}`]), TypeError);
});

test("only an offered and legal Replay draft choice is selected", () => {
  const position = { state: { draft: { choices: [
    { id: "other", instanceId: "a" }, { id: "replay", instanceId: "r" },
  ] } } };
  const actions = [{ payload: { type: "draftPick", cardInstanceId: "a" } },
    { payload: { type: "draftPick", cardInstanceId: "r" } }];
  assert.equal(replayDraftChoice(position, actions), actions[1]);
  assert.equal(replayDraftChoice(position, actions.slice(0, 1)), undefined);
});

test("Replay absence distinguishes ownership, frame and turn", () => {
  const position = { state: { turn: "white", deckSlots: { white: [] }, moveReplay: { white: null } } };
  assert.match(replayAbsence(position, "black"), /turn/);
  assert.match(replayAbsence(position, "white"), /deck slots/);
  position.state.deckSlots.white.push({ id: "replay" });
  assert.match(replayAbsence(position, "white"), /frame/);
  position.state.deckSlots.white[0].used = true;
  assert.match(replayAbsence(position, "white"), /used/);
  position.state.deckSlots.white[0].used = false;
  position.state.moveReplay.white = { delta: [{ row: 1, col: 1 }] };
  assert.match(replayAbsence(position, "white"), /conditions/);
});

test("a natural witness requires draft ownership, legal Replay and follow-up", () => {
  const search = { acquired: { color: "white" }, draft: [{ selectedCardIds: ["replay"] }],
    preReplayActions: [] };
  const sample = { replaySequence: { status: "complete", steps: [
    { kind: "replay", beforeActions: [{ type: "card", cardId: "replay" }] },
    { kind: "follow" },
  ] } };
  assert.equal(validateReplayWitness(search, sample), true);
  assert.throws(() => validateReplayWitness({ ...search, acquired: null }, sample));
  assert.throws(() => validateReplayWitness(search, { replaySequence: {
    ...sample.replaySequence, steps: sample.replaySequence.steps.slice(0, 1),
  } }));
});

test("JSON budget failure identifies the bounded search position without state contents", () => {
  const item = { summary: { name: "normal-seed7-replay-52",
    replaySearch: { style: "normal", seed: 7, decision: 52, phase: "END" } },
  input: { position: { state: { mode: "play", board: "private" }, history: [] } } };
  assert.deepEqual(caseBudgetContext(item), {
    style: "normal", seed: 7, decision: 52, phase: "END", "position.history.length": 0,
  });
  const error = caseBudgetError(item);
  for (const marker of ["normal", "7", "52", "END", "position.history.length"])
    assert.ok(error.includes(marker));
  assert.ok(!error.includes("private"));
});
test("JSON budget failure remains a failure across Replay continuation and reports only diagnostics", () => {
  const budgetError = new TypeError("JSON exceeds depth64/nodes100000/bytes8MiB limits.");
  budgetError.code = "JSON_BUDGET_EXCEEDED";
  budgetError.budget = { exceeded: "bytes", path: "$/boardHistory/0", topLevelFieldPath: "$/boardHistory" };
  budgetError.stateFields = { boardHistory: { nodes: 42, bytes: 1234, depth: 7 } };
  const adapter = { actionStream: () => ({ nextPage: () => { throw budgetError; }, dispose: () => {} }) };
  const position = { state: { mode: "play", turn: "white", board: "private" }, history: [] };
  assert.throws(() => sourceReplaySequence(adapter, null, null, position, "white"),
    error => error === budgetError);
  budgetError.searchContext = { style: "normal", seed: 7, decision: 52,
    stage: "legal enumeration", actionCategory: "source candidate enumeration",
    stateMode: "play", turn: "white", positionHistoryLength: 0 };
  const report = jsonBudgetFailure(budgetError);
  assert.equal(report.code, "JSON_BUDGET_EXCEEDED");
  assert.equal(report.exceededJsonPath, "$/boardHistory/0");
  assert.equal(report.positionHistoryLength, 0);
  assert.ok(!JSON.stringify(report).includes("private"));
});
