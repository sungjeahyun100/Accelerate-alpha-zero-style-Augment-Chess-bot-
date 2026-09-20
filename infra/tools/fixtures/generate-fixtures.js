#!/usr/bin/env node
// Oracle fixtures ("answer sheets") for differential tests of a port (e.g. the Rust engine).
//
// Every fixture = one game position (the full state as JSON) + what the ORACLE says about it:
//   expected.legalActions  all legal actions (normalised: random ids removed, sorted, de-duplicated)
//   expected.applied[]     for a sample of those actions: the raw action, whether applying it
//                          succeeded and a canonical signature of the resulting state
// A port passes when, for every fixture, it produces the same legal action set and the same
// signature after applying the same action.
//
// Coverage is balanced on purpose: every piece type and every card id gets the same number of
// dedicated fixtures (--per-piece / --per-card), on top of random mid-game positions taken from
// random playouts. A coverage report says how often each piece/card really showed up and which
// cards never produced a card action (opening-only or passive cards).
//
//   node tools/fixtures/generate-fixtures.js [--out=fixtures.jsonl] [--seed=1] [--per-piece=10]
//        [--per-card=6] [--playouts=100] [--plies=30] [--max-applied=6] [--oracle=engine|site]
//        [--full-state] [--coverage=coverage.json]
//   node tools/fixtures/generate-fixtures.js --serve
//        oracle server: reads one JSON request per line on stdin, answers one JSON line on stdout.
//        request  {"id","color","state","actions":[raw actions to apply]}
//        response {"id","legalActions":[normalised keys, sorted],"applied":[{"ok","signature"}...]}
//        A port implements the SAME protocol; tests/differential/run-differential.js talks to both.
//   node tools/fixtures/generate-fixtures.js --verify=fixtures.jsonl
//        re-runs the oracle from each fixture's JSON state and checks it reproduces the fixture
//        (proves the JSON state is complete enough to be handed to another implementation)
//
// --oracle=engine (default) uses engine-merged.js; --oracle=site uses the real site worker
// (needs `node tools/site-parity/fetch-real-worker.js` once; network).
const fs = require("fs");
const path = require("path");
const zlib = require("zlib");

const arg = (name, def) => {
  const a = process.argv.slice(2).find((x) => x === "--" + name || x.startsWith("--" + name + "="));
  if (!a) return def;
  return a.includes("=") ? a.slice(a.indexOf("=") + 1) : true;
};
const ROOT = path.resolve(__dirname, "..", "..");
const oracleName = arg("oracle", "engine");
let oracle;
if (oracleName === "site") oracle = require(path.join(ROOT, "tools", "site-parity", "common.js")).real;
else oracle = require(path.join(ROOT, "engine-merged.js"));
const { ALL_TYPES, CARD_POOL_TYPES } = require(path.join(ROOT, "nnue", "encode.js"));

function rngMaker(seed) { let x = seed >>> 0 || 1; return () => ((x = (x * 1664525 + 1013904223) >>> 0) / 4294967296); }
const strip = (o) => { if (o && typeof o === "object") { delete o.id; delete o.instanceId; delete o.pieceId; for (const k in o) strip(o[k]); } return o; };
const normAction = (a) => JSON.stringify(strip(JSON.parse(JSON.stringify(a))));
const clone = (o) => JSON.parse(JSON.stringify(o));

// ---------------------------------------------------------------- state construction
const STANDARD = ["pawn", "knight", "bishop", "rook", "queen"];
function baseState(board, decks, color, rng) {
  const st = oracle.cloneState({});
  st.board = clone(board); st.mode = "play"; st.turn = color; st.actionsRemaining = 1;
  st.deckSlots = clone(decks); st.captures = { white: [], black: [] }; st.aiSearchNoCards = false;
  // fields a hand-built state must carry (otherwise the oracle's root safety misbehaves)
  st.turnsTaken = { white: Math.floor(rng() * 20), black: Math.floor(rng() * 20) };
  st.moveCount = st.turnsTaken.white + st.turnsTaken.black;
  st.castlingCanceled = { white: rng() < 0.5, black: rng() < 0.5 };
  st.parrotMovement = { white: null, black: null };
  if (st.board.some((row) => row.some((p) => p?.type === "campfire"))) st.hasCampfire = true;
  oracle.setWorkerBoardDimensions(st);
  return st;
}
function emptyBoard() { return Array.from({ length: 8 }, () => Array(8).fill(null)); }
function placeRandom(b, type, color, rng) {
  for (let t = 0; t < 60; t++) {
    const r = Math.floor(rng() * 8), c = Math.floor(rng() * 8);
    if (b[r][c]) continue;
    if (type === "pawn" && (r === 0 || r === 7)) continue;
    b[r][c] = { type, color, moved: rng() < 0.5 };
    return [r, c];
  }
  return null;
}
function randomOtherType(rng) { return rng() < 0.3 ? STANDARD[Math.floor(rng() * STANDARD.length)] : ALL_TYPES[Math.floor(rng() * ALL_TYPES.length)]; }
function makeBoard(rng, mustType) {
  const b = emptyBoard();
  placeRandom(b, "king", "white", rng); placeRandom(b, "king", "black", rng);
  let pos = null, mustColor = null;
  if (mustType && mustType !== "king") { mustColor = rng() < 0.5 ? "white" : "black"; pos = placeRandom(b, mustType, mustColor, rng); }
  const n = 4 + Math.floor(rng() * 9);
  for (let i = 0; i < n; i++) placeRandom(b, randomOtherType(rng), rng() < 0.5 ? "white" : "black", rng);
  return { board: b, mustPos: pos, mustColor };
}
function cardObj(color, effect, k, rng) { return { id: effect, instanceId: `${color}-${effect}-${k}`, effect, stars: 1 + Math.floor(rng() * 5), used: false, recovering: false }; }
function makeDecks(rng, mustCard, mustColor) {
  const decks = { white: [], black: [] };
  for (const col of ["white", "black"]) {
    const used = new Set(); const want = 3 + Math.floor(rng() * 3);
    if (mustCard && col === mustColor) { used.add(mustCard); decks[col].push(cardObj(col, mustCard, 0, rng)); }
    for (let k = 1; decks[col].length < want && k < 40; k++) {
      const e = CARD_POOL_TYPES[Math.floor(rng() * CARD_POOL_TYPES.length)];
      if (used.has(e)) continue; used.add(e); decks[col].push(cardObj(col, e, k, rng));
    }
  }
  return decks;
}

// ---------------------------------------------------------------- oracle answers
function signature(st) {
  // Canonical, language-neutral comparison object (see docs/PORTING-GUIDE.md for the exact cell format).
  const cell = (p) => p ? [p.type, p.color, !!p.moved, !!p.shielded, !!p.frozen, !!p.witchTrial, p.witchTrial?.countBy || "", p.frozenByCard?.countBy || "", !!p.recurrence, !!p.defected, p.hp ?? "", p.promotionRushUntil ?? ""].join(":") : "-";
  const bd = st.board.map((row) => row.map(cell));
  const decks = { white: [], black: [] };
  for (const c of ["white", "black"]) decks[c] = (st.deckSlots?.[c] || []).map((k) => `${k.effect}:${k.used ? 1 : 0}:${k.recovering ? 1 : 0}`);
  return {
    mode: st.mode, winner: st.winner || "", turn: st.turn, actionsRemaining: st.actionsRemaining, board: bd, decks,
    turnsTaken: st.turnsTaken || null, moveCount: st.moveCount ?? null,
    pendingScarecrows: (st.pendingScarecrows || []).map((e) => [e.row, e.col, !!e.reserved, e.remainingOwnTurns ?? "", e.by || ""]),
    pendingGales: (st.pendingGales || []).map((e) => [e.color, e.remainingOwnTurns ?? "", e.triggerTurn ?? ""]),
    othello: st.othelloPending || null, reversal: st.reversal || null,
    platform: st.platformRule ? [st.platformRule.enabled, st.platformRule.cadence || "", st.platformRule.nextAt, st.platformRule.countUnit] : null
  };
}
function legalList(st, color) {
  const raw = oracle.generateActions(st, color);
  const seen = new Set(), keys = [];
  raw.forEach((a) => { const k = normAction(a); if (!seen.has(k)) { seen.add(k); keys.push(k); } });
  keys.sort();
  return { raw, keys };
}
function applyOn(st, action, color) {
  const copy = oracle.cloneState(st);
  const res = oracle.applyAction(copy, clone(action), color);
  return { ok: !!res.ok, signature: res.ok ? signature(copy) : null, state: copy };
}
// Expected answers are computed from the REHYDRATED state (the state exactly as a port receives it as
// JSON), never from the live in-memory state. An action whose result differs between two identical runs
// (random effects such as brutus) is left out and counted in expected.skippedNondeterministic (8 identical runs required).
function answer(stLive, color, picks, maxApplied, fullState) {
  const json = clone(stLive);
  const st = rehydrate(json);
  const { raw, keys } = legalList(st, color);
  const applied = [];
  let skipped = 0;
  for (const a of picks(raw)) {
    if (applied.length >= maxApplied) break;
    const r = applyOn(rehydrate(json), a, color);
    // 8 repeats: a random effect can repeat its result by chance (e.g. one of two random targets)
    let stable = true;
    for (let k = 0; k < 7 && stable; k++) { const r2 = applyOn(rehydrate(json), a, color); if (r.ok !== r2.ok || JSON.stringify(r.signature) !== JSON.stringify(r2.signature)) stable = false; }
    if (!stable) { skipped++; continue; }
    const item = { action: clone(a), key: normAction(a), ok: r.ok, signature: r.signature };
    if (fullState && r.ok) item.state = clone(r.state);
    applied.push(item);
  }
  return { legalActions: keys, applied, skippedNondeterministic: skipped, terminal: { mode: st.mode, winner: st.winner || "" } };
}
function sampleActions(raw, rng, preferred, maxApplied) {
  const pref = raw.filter(preferred);
  const rest = raw.filter((a) => !preferred(a));
  const shuffle = (arr) => { for (let i = arr.length - 1; i > 0; i--) { const j = Math.floor(rng() * (i + 1)); [arr[i], arr[j]] = [arr[j], arr[i]]; } return arr; };
  const out = shuffle(pref.slice()).slice(0, Math.ceil(maxApplied / 2)).concat(shuffle(rest.slice()).slice(0, maxApplied));
  return out.slice(0, maxApplied);
}

// ---------------------------------------------------------------- generation
function generate() {
  const seed = Number(arg("seed", 1));
  const perPiece = Number(arg("per-piece", 10)), perCard = Number(arg("per-card", 6));
  const playouts = Number(arg("playouts", 100)), plies = Number(arg("plies", 30));
  const maxApplied = Number(arg("max-applied", 6)), fullState = !!arg("full-state", false);
  const out = arg("out", "oracle-fixtures.jsonl");
  const rng = rngMaker(seed);
  const cov = { pieces: {}, cards: {} };
  ALL_TYPES.forEach((t) => (cov.pieces[t] = { fixtures: 0 }));
  CARD_POOL_TYPES.forEach((c) => (cov.cards[c] = { inDeck: 0, actionGenerated: 0, applied: 0 }));
  const fixtures = [];
  let n = 0;
  const push = (source, st, color, ans) => {
    fixtures.push({ id: "f" + String(++n).padStart(6, "0"), source, seed, oracle: oracleName, color, state: clone(st), expected: ans });
    const seenTypes = new Set(); st.board.forEach((row) => row.forEach((p) => { if (p) seenTypes.add(p.type); })); seenTypes.forEach((ty) => { if (cov.pieces[ty]) cov.pieces[ty].fixtures++; });
  };

  // 1) one block per piece type, balanced
  for (const type of ALL_TYPES) {
    for (let k = 0; k < perPiece; k++) {
      const { board, mustPos, mustColor } = makeBoard(rng, type);
      const color = mustColor && rng() < 0.7 ? mustColor : (rng() < 0.5 ? "white" : "black");
      const st = baseState(board, makeDecks(rng, null, null), color, rng);
      const isTarget = (a) => mustPos && a.from && a.from.row === mustPos[0] && a.from.col === mustPos[1];
      push("piece:" + type, st, color, answer(st, color, (raw) => sampleActions(raw, rng, isTarget, maxApplied), maxApplied, fullState));
    }
  }
  // 2) one block per card id, balanced; retry a few boards until the card yields an action
  for (const card of CARD_POOL_TYPES) {
    for (let k = 0; k < perCard; k++) {
      let st, color, ans;
      for (let attempt = 0; attempt < 12; attempt++) {
        color = rng() < 0.5 ? "white" : "black";
        const { board } = makeBoard(rng, null);
        st = baseState(board, makeDecks(rng, card, color), color, rng);
        const legal = oracle.generateActions(st, color);
        if (legal.some((a) => a.type === "card" && a.cardId === card)) break;
      }
      const isCard = (a) => a.type === "card" && a.cardId === card;
      ans = answer(st, color, (raw) => sampleActions(raw, rng, isCard, maxApplied), maxApplied, fullState);
      const c = cov.cards[card];
      c.inDeck++;
      if (ans.applied.some((x) => x.key.includes('"cardId":"' + card + '"'))) c.applied++;
      if (legalList(st, color).raw.some(isCard)) c.actionGenerated++;
      push("card:" + card, st, color, ans);
    }
  }
  // 3) mid-game positions from random playouts (states reachable in real play)
  for (let g = 0; g < playouts; g++) {
    const b = emptyBoard();
    const back = ["rook", "knight", "bishop", "queen", "king", "bishop", "knight", "rook"];
    for (const [col, br, pr] of [["black", 0, 1], ["white", 7, 6]]) for (let c = 0; c < 8; c++) { b[pr][c] = { type: "pawn", color: col, moved: false }; b[br][c] = { type: back[c], color: col, moved: false }; }
    for (let s = 0; s < 3; s++) { const t = ALL_TYPES[Math.floor(rng() * ALL_TYPES.length)]; const c = Math.floor(rng() * 8); const col = rng() < 0.5 ? "white" : "black"; const r = col === "white" ? 5 : 2; if (!b[r][c] && t !== "king") b[r][c] = { type: t, color: col, moved: false }; }
    let color = "white";
    const st = baseState(b, makeDecks(rng, null, null), color, rng);
    st.turnsTaken = { white: 0, black: 0 }; st.moveCount = 0;
    for (let p = 0; p < plies && st.mode !== "gameover"; p++) {
      color = st.turn;
      const { raw } = legalList(st, color);
      if (!raw.length) break;
      if (p % 3 === 2) push(`playout:${g}:${p}`, st, color, answer(st, color, (r2) => sampleActions(r2, rng, (a) => a.type === "card", maxApplied), maxApplied, fullState));
      const cards = raw.filter((a) => a.type === "card");
      const act = cards.length && rng() < 0.3 ? cards[Math.floor(rng() * cards.length)] : raw[Math.floor(rng() * raw.length)];
      if (!oracle.applyAction(st, clone(act), color).ok) break;
    }
  }
  fs.mkdirSync(path.dirname(path.resolve(out)), { recursive: true });
  const text = fixtures.map((f) => JSON.stringify(f)).join("\n") + "\n";
  fs.writeFileSync(out, out.endsWith(".gz") ? zlib.gzipSync(text) : text);
  const covFile = arg("coverage", null);
  const zeroPieces = Object.entries(cov.pieces).filter(([, v]) => !v.fixtures).map(([k]) => k);
  const noAction = Object.entries(cov.cards).filter(([, v]) => v.inDeck && !v.actionGenerated).map(([k]) => k);
  const report = { seed, oracle: oracleName, fixtures: fixtures.length, perPiece, perCard, playouts, coverage: cov, pieceTypesNeverOnBoard: zeroPieces, cardsThatNeverProducedACardAction: noAction };
  if (covFile) fs.writeFileSync(covFile, JSON.stringify(report, null, 2));
  const pf = Object.values(cov.pieces).map((v) => v.fixtures).sort((a, b) => a - b);
  console.log(`fixtures: ${fixtures.length}  -> ${out}`);
  console.log(`piece coverage (fixtures containing the piece): min ${pf[0]} median ${pf[pf.length >> 1]} max ${pf[pf.length - 1]}; never seen: ${zeroPieces.length ? zeroPieces.join(",") : "none"}`);
  console.log(`cards: ${CARD_POOL_TYPES.length} ids, ${CARD_POOL_TYPES.length - noAction.length} produced a card action at least once, ${noAction.length} never (opening-only/passive/needs setup)`);
}

// ---------------------------------------------------------------- verification (round trip)
// JSON has no shared references: a 2x2 piece (colossus, bigRook, bigBishop) is ONE object referenced from
// four cells in memory but arrives as four equal copies with the same id. cloneState() re-joins them by id,
// so the board is rebuilt through it; every other field is copied as-is.
function rehydrate(json) {
  const st = oracle.cloneState({});
  Object.assign(st, clone(json));
  st.board = oracle.cloneState(clone(json)).board;
  // pieces without an id (e.g. a bigRook) are tied together by their anchor: same type, colour and
  // (anchorRow, anchorCol) = one piece = one shared object
  const byAnchor = new Map();
  for (const row of st.board) for (let c = 0; c < row.length; c++) {
    const p = row[c];
    if (!p || p.id || !Number.isInteger(p.anchorRow) || !Number.isInteger(p.anchorCol)) continue;
    const key = p.type + "|" + p.color + "|" + p.anchorRow + "|" + p.anchorCol;
    if (byAnchor.has(key)) row[c] = byAnchor.get(key); else byAnchor.set(key, p);
  }
  oracle.setWorkerBoardDimensions(st);
  return st;
}
function verify(file) {
  const raw = fs.readFileSync(file);
  const text = (file.endsWith(".gz") ? zlib.gunzipSync(raw) : raw).toString("utf8");
  let n = 0, bad = 0, appliedChecked = 0;
  for (const line of text.split("\n").filter(Boolean)) {
    const f = JSON.parse(line); n++;
    const st = rehydrate(f.state);
    const { keys } = legalList(st, f.color);
    let ok = keys.length === f.expected.legalActions.length && keys.every((k, i) => k === f.expected.legalActions[i]);
    for (const item of f.expected.applied) {
      const r = applyOn(rehydrate(f.state), item.action, f.color);
      appliedChecked++;
      if (r.ok !== item.ok || JSON.stringify(r.signature) !== JSON.stringify(item.signature)) ok = false;
    }
    if (!ok) { bad++; if (bad <= 5) console.log("MISMATCH", f.id, f.source); }
  }
  console.log(`verified ${n} fixtures (${appliedChecked} applied actions) against the ${oracleName} oracle: ${bad} mismatches`);
  process.exit(bad ? 1 : 0);
}

function serve() {
  const rl = require("readline").createInterface({ input: process.stdin });
  rl.on("line", (line) => {
    if (!line.trim()) return;
    let req;
    try { req = JSON.parse(line); } catch (e) { process.stdout.write(JSON.stringify({ error: "bad json" }) + "\n"); return; }
    try {
      const st = rehydrate(req.state);
      const { keys } = legalList(st, req.color);
      const applied = (req.actions || []).map((a) => { const r = applyOn(rehydrate(req.state), a, req.color); return { ok: r.ok, signature: r.signature }; });
      process.stdout.write(JSON.stringify({ id: req.id, legalActions: keys, applied }) + "\n");
    } catch (e) { process.stdout.write(JSON.stringify({ id: req.id, error: String(e && e.message || e) }) + "\n"); }
  });
}

const v = arg("verify", null);
if (arg("serve", false)) serve(); else if (v) verify(v); else generate();
