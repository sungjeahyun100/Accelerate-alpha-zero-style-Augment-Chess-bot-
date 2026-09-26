#!/usr/bin/env node
// DRAFT (bridge-draft-0). One-off helper that produced bridge/examples/*.json from the
// site-reference-v1 fixtures (PR #19, tests/differential/fixtures/site-reference-v1/).
// Not needed to run validation; the generated examples are committed. Dependency-free.
//
//   node bridge/tools/make-examples-from-fixtures.js <fixtures-dir> [--out=bridge/examples]
//
// It only reads the fixture data (JSONL). No site code is involved or copied.
"use strict";
const fs = require("fs");
const path = require("path");

const dir = process.argv[2];
const outArg = process.argv.find((a) => a.startsWith("--out="));
const OUT = outArg ? outArg.slice(6) : path.join(__dirname, "..", "examples");
if (!dir) { console.error("usage: make-examples-from-fixtures.js <fixtures-dir> [--out=DIR]"); process.exit(1); }

const PROTOCOL = "bridge-draft-0";
const rd = (f) => fs.readFileSync(path.join(dir, f), "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l));
const fx = { pieces: rd("pieces.jsonl"), cards: rd("cards.jsonl"), playouts: rd("playouts.jsonl"), gameover: rd("gameover.jsonl") };
const byId = new Map([].concat(fx.pieces, fx.cards, fx.playouts, fx.gameover).map((f) => [f.id, f]));

// ---- site state <-> bridge GameState (draft) ----
// Core keys move to the top level; every other site key is kept verbatim under `extra`.
const CORE = ["turn", "mode", "winner", "actionsRemaining", "moveCount", "fullMove", "turnsTaken",
  "cardsUsedThisTurn", "board", "deckSlots", "captures", "enPassant"];
function toGameState(site) {
  const s = {};
  for (const k of CORE) s[k] = site[k];
  s.winner = site.winner === "" ? null : site.winner; // site uses "" for "no winner yet"
  s.extra = {};
  for (const k of Object.keys(site)) if (!CORE.includes(k)) s.extra[k] = site[k];
  return s;
}

// ---- fixture stateDelta applier (format documented in the fixture README) ----
function applyDelta(base, node) {
  if ("=" in node) return node["="];
  if (node.o || node.d) {
    const out = Array.isArray(base) ? base.slice() : Object.assign({}, base);
    for (const k of Object.keys(node.o || {})) out[k] = applyDelta(base[k], node.o[k]);
    for (const k of node.d || []) delete out[k];
    return out;
  }
  if (node.a) {
    const out = base.slice();
    for (const i of Object.keys(node.a)) out[i] = applyDelta(base[i], node.a[i]);
    return out;
  }
  throw new Error("bad delta node");
}

// ---- compact-but-readable JSON writer ----
function fmt(v, ind) {
  const c = JSON.stringify(v);
  if (c === undefined) return "null";
  if (c.length <= 110 || typeof v !== "object" || v === null) return c;
  const pad = "  ".repeat(ind + 1), end = "  ".repeat(ind);
  if (Array.isArray(v)) return "[\n" + v.map((x) => pad + fmt(x, ind + 1)).join(",\n") + "\n" + end + "]";
  return "{\n" + Object.keys(v).map((k) => pad + JSON.stringify(k) + ": " + fmt(v[k], ind + 1)).join(",\n") + "\n" + end + "}";
}
const manifest = [];
function put(name, schemaRef, obj, note) {
  fs.mkdirSync(OUT, { recursive: true });
  fs.writeFileSync(path.join(OUT, name), fmt(obj, 0) + "\n");
  manifest.push({ file: name, schema: schemaRef, valid: true, note });
}

const findApplied = (f, pred) => f.expected.applied.find((a) => a.ok && pred(a.action));

// 1. new_game (card definitions once) -- ids from the 239 cards seen in the fixtures
const ids = new Set();
for (const f of [].concat(fx.pieces, fx.cards, fx.playouts, fx.gameover))
  for (const c of ["white", "black"]) for (const s of f.state.deckSlots[c]) ids.add(s.id);
const cards = [...ids].sort().map((id) => ({ id, effect: id }));
const p1 = byId.get("playout-00001");
const s1 = toGameState(p1.state);
put("new_game.request.json", "request.schema.json", {
  protocol: PROTOCOL, type: "new_game", id: 1, cards, initialState: s1, seed: 20260926,
}, "cards: id/effect only (239 ids seen in the fixtures); initialState: playout-00001");
put("new_game.response.json", "response.schema.json", {
  protocol: PROTOCOL, type: "new_game_response", id: 1, ok: true, gameId: "g-1", cardCount: cards.length, state: s1,
}, "example response");

// 2. get_legal_actions
const legal = p1.expected.legalActions.map((k) => JSON.parse(k));
put("get_legal_actions.request.json", "request.schema.json", { protocol: PROTOCOL, type: "get_legal_actions", id: 2, gameId: "g-1", state: s1 }, "playout-00001");
put("get_legal_actions.response.json", "response.schema.json", { protocol: PROTOCOL, type: "get_legal_actions_response", id: 2, ok: true, actions: legal }, "all " + legal.length + " legal actions of playout-00001");

// 3. apply_action: a plain move, a card without target, a card with {row,col} target
function applyExample(n, f, a, label) {
  const after = toGameState(applyDelta(f.state, a.stateDelta));
  put("apply_action." + label + ".request.json", "request.schema.json", { protocol: PROTOCOL, type: "apply_action", id: n, gameId: "g-1", state: toGameState(f.state), action: a.action }, f.id);
  put("apply_action." + label + ".response.json", "response.schema.json", { protocol: PROTOCOL, type: "apply_action_response", id: n, ok: true, state: after }, "state = fixture state + stateDelta (" + f.id + ")");
}
applyExample(3, p1, findApplied(p1, (a) => a.type === "move"), "move");
applyExample(4, p1, findApplied(p1, (a) => a.type === "card" && !("target" in a)), "card-no-target");
let cf = null, ca = null;
for (const f of fx.cards) { const a = findApplied(f, (x) => x.type === "card" && x.target && x.target.row !== undefined && !x.target.knight); if (a) { cf = f; ca = a; break; } }
applyExample(5, cf, ca, "card-cell-target");

// 4. game end: an action that ends the game, then get_result on the terminal position
const g1 = byId.get("gameover-00003");
const ga = g1.expected.applied.find((a) => a.ok && a.action.type === "move" && a.stateDelta && applyDelta(g1.state, a.stateDelta).mode === "gameover");
applyExample(6, g1, ga, "ends-game");
const t1 = byId.get("gameover-00004");
put("get_result.request.json", "request.schema.json", { protocol: PROTOCOL, type: "get_result", id: 7, gameId: "g-1", state: toGameState(t1.state) }, "terminal position " + t1.id);
put("get_result.response.json", "response.schema.json", { protocol: PROTOCOL, type: "get_result_response", id: 7, ok: true, terminal: true, mode: t1.state.mode, winner: t1.state.winner === "" ? null : t1.state.winner }, "");

// 5. Action samples with different target shapes (from legalActions of card fixtures)
const shapes = {};
for (const f of fx.cards) for (const k of f.expected.legalActions) {
  const a = JSON.parse(k); if (a.type !== "card") continue;
  const t = a.target; const s = !("target" in a) ? "none" : t === null ? "null" : t.selections ? "selections" : t.ruleId ? "ruleId" : t.knight ? "cell+knight" : "cell";
  if (!shapes[s]) shapes[s] = a;
}
put("action.card-targets.json", "action.schema.json#/$defs/ActionList", Object.values(shapes).concat([legal.find((a) => a.type === "move")]), "one card action per observed target shape + a move");

// 6. GameState alone
put("game-state.start.json", "game-state.schema.json", s1, "playout-00001 converted");

console.log("wrote", manifest.length, "examples to", OUT, "(bridge/examples/manifest.json is maintained by hand)");
