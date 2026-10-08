#!/usr/bin/env node
"use strict";

// A bounded, source-driven comparison. The native request is built from an
// explicit reviewed state contract; sourceExpected is only the comparison.
const assert = require("node:assert/strict");
const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { switcheroo, compare } = require("./october-source-transition.cjs");

function nativeState(receipt, phase) {
  const state = receipt.rustInput[phase];
  const expected = receipt.sourceExpected[phase];
  assert.equal(state.mode, expected.mode);
  assert.equal(state.turn, expected.turn);
  assert.equal(state.moveCount, expected.moveCount);
  assert.deepEqual(state.turnsTaken, expected.turnsTaken);
  return state;
}

function project(state, visibleBoard) {
  const board = state.board.flatMap((line, row) => line.flatMap((piece, col) => piece ? [{
    row, col, id: piece.id, type: piece.type, color: piece.color,
    moved: Boolean(piece.moved), holdoutPromotion: piece.holdoutPromotion || null,
    chimera: Boolean(piece.chimera), reaperCaptures: piece.reaperCaptures ?? null,
  }] : []));
  assert.deepEqual(Object.keys(visibleBoard).sort(), ["black", "white"]);
  const deck = Object.fromEntries(["white", "black"].map(color => [color,
    state.deckSlots[color].filter(Boolean).map(card => ({
      id: card.id, instanceId: card.instanceId, used: Boolean(card.used),
    }))]));
  return {
    mode: state.mode, turn: state.turn, moveCount: state.moveCount,
    turnsTaken: state.turnsTaken, cardsUsedThisTurn: state.cardsUsedThisTurn,
    actionsRemaining: state.actionsRemaining, fullMove: state.fullMove,
    switcheroo: state.switcheroo, winner: state.winner ?? null, board, deck, visibleBoard,
  };
}

function native(binary, request) {
  const tempRoot = process.platform === "win32" && process.env.APPDATA
    ? path.join(process.env.APPDATA, "Accelerate", "tmp")
    : path.join(os.tmpdir(), "Accelerate", "tmp");
  fs.mkdirSync(tempRoot, {recursive: true});
  const inputPath = path.join(tempRoot, "october-native-switcheroo-request.jsonl");
  fs.writeFileSync(inputPath, JSON.stringify(request) + "\n");
  const input = fs.openSync(inputPath, "r");
  let child;
  try {
    child = spawnSync(binary, {
      stdio: [input, "pipe", "pipe"], encoding: "utf8", timeout: 15000,
      maxBuffer: 16 * 1024 * 1024,
    });
  } finally {
    fs.closeSync(input);
    fs.unlinkSync(inputPath);
  }
  if (child.error) throw child.error;
  assert.equal(child.status, 0, child.stderr);
  const lines = child.stdout.trim().split("\n");
  assert.equal(lines.length, 1, "native transport must return exactly one response");
  return JSON.parse(lines[0]);
}

function run(source, parser, binary) {
  const receipt = switcheroo(source, parser);
  const cardInput = nativeState(receipt, "cardBefore");
  const cardRequest = {
    method: "apply_public_intent", state: cardInput, rulesVersion: receipt.rulesVersion,
    intent: {type: "card", color: receipt.replay.cardAction.color,
      cardId: receipt.replay.cardAction.cardId, target: null},
  };
  const cardResult = native(binary, cardRequest);
  assert.equal(cardResult.error, undefined, cardResult.error);
  assert.equal(cardResult.action.cardInstanceId, receipt.replay.cardAction.cardInstanceId);
  assert.deepEqual(project(cardInput, cardResult.visibleBefore), receipt.sourceExpected.cardBefore);
  assert.deepEqual(project(cardResult.state, cardResult.visibleAfter), receipt.sourceExpected.cardAfter);
  const state = cardResult.state;
  const request = {
    method: "apply_public_intent", state, rulesVersion: receipt.rulesVersion,
    intent: receipt.replay.publicIntent,
  };
  const result = native(binary, request);
  assert.equal(result.error, undefined, result.error);
  assert.deepEqual(result.visibleBefore, cardResult.visibleAfter);
  assert.equal(result.action.type, "move");
  assert.equal(result.action.move.switcherooMove, true);
  assert.deepEqual({row: result.action.move.row, col: result.action.move.col},
    receipt.replay.publicIntent.destination);
  const actual = {
    schemaVersion: receipt.schemaVersion, caseName: receipt.caseName,
    rulesVersion: receipt.rulesVersion, sourceMainSha256: receipt.sourceMainSha256,
    sourcePublicCatalogHash: receipt.sourcePublicCatalogHash,
    executionProfileVersion: receipt.executionProfileVersion,
    publicIntent: receipt.replay.publicIntent,
    before: project(state, result.visibleBefore), after: project(result.state, result.visibleAfter),
  };
  compare(receipt, actual);
  for (const [name, altered] of [
    ["wrong rulesVersion", {...request, rulesVersion: "augment-site-20260928-e5ed84fcf8e72a24"}],
    ["wrong catalog", {...request, state: {...state, octoberCatalogHash: "wrong"}}],
    ["wrong profile", {...request, state: {...state, octoberExecutionProfile: "wrong"}}],
    ["wrong source digest", {...request, state: {...state, octoberSourceMainSha256: "wrong"}}],
    ["conflicting state version", {...request, state: {...state, rulesetId: "augment-site-20260928-e5ed84fcf8e72a24"}}],
    ["unused card", {...request, state: {...state, deckSlots: {
      ...state.deckSlots, white: state.deckSlots.white.map(card => ({...card, used: false})),
    }}}],
    ["opponent pawn", {...request, intent: {...request.intent, destination: {row: 1, col: 0}}}],
    ["other piece", {...request, intent: {...request.intent, destination: {row: 7, col: 0}}}],
    ["invalid intent", {...request, intent: {...request.intent, unexpected: true}}],
    ["invalid bound action", {...request, method: "apply", action: {
      ...result.action, move: {...result.action.move, switcherooMove: false},
    }}],
    ["stale action", {...request, method: "apply", state: result.state, action: result.action}],
    ["reused card", {...cardRequest, state, intent: cardRequest.intent}],
    ["missing card effect state", {...cardRequest, state: {...cardInput, switcheroo: undefined}}],
    ["hidden viewer state", {...request, state: {...state, board: state.board.map((line, row) =>
      row === 6 ? line.map((piece, col) =>
        col === 1 ? {...piece, hiddenFrom: ["white"]} : piece) : line)}}],
    ["full action enumeration", {...request, method: "legal_actions"}],
  ]) {
    assert.ok(native(binary, altered).error, name);
  }
  return {status: "pass", caseName: receipt.caseName, checks: 17,
    scope: "switcheroo transition and all-visible board projection for both viewers"};
}

if (require.main === module) {
  try {
    assert.equal(process.argv.length, 5,
      "Usage: october-native-switcheroo.cjs ABSOLUTE_SOURCE ABSOLUTE_ACORN ABSOLUTE_RUST_BINARY");
    console.log(JSON.stringify(run(...process.argv.slice(2))));
  } catch (error) {
    console.error(error.stack || error);
    process.exitCode = 1;
  }
}
module.exports = { run };
