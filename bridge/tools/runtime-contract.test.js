"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const contract = require("./runtime-contract");
const { validate, resolveRef } = require("./validate");
const state = () => ({ board: Array.from({ length: 8 }, () => Array(8).fill(null)), turn: "white", mode: "play" });
const move = () => ({ type: "move", color: "white", from: { row: 6, col: 0 }, move: { row: 5, col: 0, capture: false } });

test("RFC 8785 canonical number/string ordering rejects non-JSON input", () => {
  assert.equal(contract.canonical({ z: -0, a: [1.0, 1e-7, "한글😀"] }), '{"a":[1,1e-7,"한글😀"],"z":0}');
  for (const value of [NaN, Infinity, undefined, [,], "\ud800", { "\udc00": 1 }, Number.MAX_SAFE_INTEGER + 1]) assert.throws(() => contract.canonical(value));
  assert.throws(() => contract.canonical(Array(100001).fill(null)), /limits/);
  let deep = null; for (let index = 0; index < 65; index++) deep = [deep];
  assert.throws(() => contract.canonical(deep), /limits/);
});
test("position is immutable and RNG tape roundtrips without lossy identity", () => {
  const original = state(), p = contract.position(original, contract.rng(17, [0.125, 0.75]));
  original.turn = "black";
  assert.equal(p.state.turn, "white"); assert.ok(Object.isFrozen(p.state.board[0]));
  assert.equal(contract.validatePosition(JSON.parse(JSON.stringify(p))).positionId, p.positionId);
  assert.throws(() => contract.validatePosition({ ...p, rng: { ...p.rng, state: 18 } }), /identity/);
  assert.throws(() => contract.validatePosition({ ...p, extra: true }), /fields/);
  const first = contract.nextRandom(p.rng), second = contract.nextRandom(first.rng);
  assert.equal(first.value, 0.125); assert.equal(second.value, 0.75); assert.equal(second.rng.cursor, 2);
  assert.throws(() => contract.nextRandom({ ...p.rng, cursor: Number.MAX_SAFE_INTEGER }), /overflow/);
});
test("action identity depends on exact payload and cannot execute against another snapshot", () => {
  const first = contract.position(state(), contract.rng(1)), second = contract.position(state(), contract.rng(2));
  const a = contract.action(first, move()), b = contract.action(second, move());
  assert.equal(a.actionId, b.actionId); assert.notEqual(a.positionId, b.positionId);
  assert.throws(() => contract.validateAction(second, a), /Stale/);
  assert.notEqual(a.actionId, contract.action(first, { ...move(), move: { row: 5, col: 0, capture: true } }).actionId);
  assert.throws(() => contract.validateAction(first, { ...a, extra: true }), /fields/);
});
test("typed history rejects arbitrary data and unknown event fields", () => {
  assert.throws(() => contract.position(state(), contract.rng(1), [{ actor: "white", secret: true }]), /game event/);
  assert.throws(() => contract.position({ ...state(), board: [] }, contract.rng(1)), /8x8/);
  const projected = { kind: "transition", actor: "white", nextActor: "black", phase: "play", boardChanges: [], ownCards: [], revealedOpponentCards: [], captures: { white: [], black: [] }, result: { protocolVersion: contract.VERSIONS.result, status: "ongoing", winner: null, outcome: null, reason: "" } };
  const event = { protocolVersion: "accelerate-game-event-v1", actor: "white", action: move(), turnChanged: true, public: { white: projected, black: projected } };
  contract.validateGameEvent(event);
  const badPiece = contract.jsonCopy(event);
  badPiece.public.white.boardChanges = [{ square: { row: 0, col: 0 }, before: null, after: { type: "rook", color: "black", hiddenFrom: "white" } }];
  assert.throws(() => contract.validateGameEvent(badPiece), /public piece fields/);
  const badResult = contract.jsonCopy(event); badResult.public.white.result.winner = "white";
  assert.throws(() => contract.validateGameEvent(badResult), /Inconsistent/);
  const rawAction = contract.jsonCopy(event); rawAction.public.white.action = move();
  assert.throws(() => contract.validateGameEvent(rawAction), /public transition/);
});
test("runtime schema enforces envelopes, v2 projection provenance and public surfaces", () => {
  const schema = resolveRef("runtime-v1.schema.json#", "runtime-v1.schema.json");
  const p = contract.position(state(), contract.rng(1)), a = contract.action(p, move());
  assert.deepEqual(validate(schema.schema, p, "$", schema.docName), []);
  assert.deepEqual(validate(schema.schema, a, "$", schema.docName), []);
  for (const invalid of [{ ...a, actionId: "x" }, { ...a, payload: { ...a.payload, from: { row: 8, col: 0 } } }, { ...a, payload: { type: "card", color: "white" } }, { ...p, rng: { ...p.rng, tape: [1] } }]) assert.ok(validate(schema.schema, invalid, "$", schema.docName).length > 0);
  const observation = { protocolVersion: contract.VERSIONS.observation, viewer: "white", board: state().board, turn: "white", ownCards: [], opponentHandCount: 0, history: [], publicState: { projectionVersion: contract.observationPolicy.projectionVersion, observationPolicyHash: contract.digest(contract.observationPolicy), deathmatchStatus: { active: false, warning: false }, boardMarks: [], relationships: [], overlays: [] } };
  observation.board[2][0] = { type: "wall", color: "neutral", status: {} };
  observation.informationStateKey = contract.digest(observation);
  contract.validateObservation(observation);
  assert.deepEqual(validate(schema.schema, observation, "$", schema.docName), []);
  const resign = frame => { delete frame.informationStateKey; frame.informationStateKey = contract.digest(frame); return frame; };
  for (const modify of [frame => { frame.protocolVersion = "accelerate-observation-v1"; }, frame => { frame.publicState.observationPolicyHash = "a".repeat(64); }, frame => { frame.board[2][0].status.privateId = "hidden"; }, frame => { frame.publicState.boardMarks.push({ kind: "platformForecast", square: { row: 8, col: 0 } }); }, frame => { delete frame.publicState.relationships; }, frame => { frame.publicState.winterKingdom={enabled:true,previewIds:["secret-piece"]}; }, frame=>{frame.publicState.selectionPhase={kind:"trolley",color:"white",choices:[[],[]],windowId:"private-window"};}]) {
    assert.throws(() => contract.validateObservation(resign(modifyCopy(observation, modify))));
  }
});
test("pinned site baselines bind source, projection and schema without cross-version admission", () => {
  const latest = contract.createRuntimeContract({ baseline: "site-20260928" });
  assert.equal(contract.ORACLE_PROFILE_VERSION, "accelerate-headless-semantic-v6");
  assert.equal(latest.ORACLE_PROFILE_VERSION, "accelerate-headless-semantic-v7");
  assert.equal(latest.catalog.rulesVersion, "augment-site-20260928-e5ed84fcf8e72a24");
  assert.equal(latest.catalog.source.files.find(file => /^main-/.test(file.name)).sha256,
    "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c");
  assert.ok(Object.isFrozen(latest.catalog.source.files[1]));
  assert.ok(Object.isFrozen(latest.catalog.cards[0]));
  assert.ok(Object.isFrozen(latest.observationPolicy.statePublicFields));
  assert.throws(() => { latest.catalog.source.files[1].sha256 = "0".repeat(64); }, TypeError);
  assert.throws(() => { latest.observationPolicy.statePublicFields.push("privateField"); }, TypeError);
  assert.equal(latest.catalog.source.files[1].sha256,
    "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c");
  assert.throws(() => contract.createRuntimeContract({ baseline: "unknown" }), /Unknown runtime baseline/);
  assert.throws(() => contract.createRuntimeContract({ baseline: "site-20260928", catalog: contract.catalog }), /known runtime baseline selector/);
  const oldPosition = contract.position(state(), contract.rng(7));
  const newPosition = latest.position(state(), latest.rng(7));
  assert.notEqual(oldPosition.positionId, newPosition.positionId);
  assert.throws(() => contract.validatePosition(newPosition), /version mismatch/);
  assert.throws(() => latest.validatePosition(oldPosition), /version mismatch/);
  const oldSchema = resolveRef(contract.RUNTIME_SCHEMA_NAME + "#", contract.RUNTIME_SCHEMA_NAME);
  const newSchema = resolveRef(latest.RUNTIME_SCHEMA_NAME + "#", latest.RUNTIME_SCHEMA_NAME);
  assert.deepEqual(validate(newSchema.schema, newPosition, "$", newSchema.docName), []);
  assert.ok(validate(newSchema.schema, oldPosition, "$", newSchema.docName).length);
  assert.ok(validate(oldSchema.schema, newPosition, "$", oldSchema.docName).length);
  const newObservation = { protocolVersion: latest.VERSIONS.observation, viewer: "white", board: state().board,
    turn: "white", ownCards: [], opponentHandCount: 0, history: [], publicState: {
      projectionVersion: latest.observationPolicy.projectionVersion,
      observationPolicyHash: latest.digest(latest.observationPolicy),
      deathmatchStatus: { active: false, warning: false }, boardMarks: [], relationships: [], overlays: [] } };
  newObservation.informationStateKey = latest.digest(newObservation);
  latest.validateObservation(newObservation);
  assert.throws(() => contract.validateObservation(newObservation), /projection policy mismatch/);
  assert.deepEqual(validate(newSchema.schema, newObservation, "$", newSchema.docName), []);
  assert.ok(validate(oldSchema.schema, newObservation, "$", oldSchema.docName).length);
});
function modifyCopy(value, modify) { const copy = contract.jsonCopy(value); modify(copy); return copy; }
