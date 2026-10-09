"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { options, replayAbsence, replayDraftChoice, validateReplayWitness } = require("./v7-native-differential.cjs");

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
