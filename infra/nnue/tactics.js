// Tactics test set: does a short search with evaluator X find the move a much
// deeper search prefers? Win/loss matches are mostly draws, so this gives a
// much less noisy strength signal per CPU-hour.
//
//   node nnue/tactics.js build <data.jsonl> <out.json> [count=60] [refMs=6000] [shortMs=200] [seed=1]
//       samples mid-game positions, asks the reference search (handcoded, deep, long)
//       for its best move + score, and keeps only positions where a quick search
//       (shortMs, depth 2) picks something else -- the "hard" ones.
//   node nnue/tactics.js eval <tactics.json> <spec> [ms=300] [depth=3]
//       spec = handcoded | <weights.json>[@lin100|atanh400|hybrid300]  (same as match.yml)
//       prints the share of positions where the evaluator's search picks the reference move
//       (plus the mean reference-score loss when it can be measured cheaply).
const fs = require("fs");
const path = require("path");
const engine = require(path.join(__dirname, "..", "engine-merged.js"));
const { loadWeights, forward } = require("./forward.js");
const { encodeBoard } = require("./encode.js");

const SCORE_SCALE = 100;
function outputMap(name) {
  if (!name) return (o) => o * SCORE_SCALE;
  let m = /^lin([0-9]+)$/.exec(name);
  if (m) { const k = Number(m[1]); return (o) => o * k; }
  m = /^atanh([0-9]+)$/.exec(name);
  if (m) { const k = Number(m[1]); return (o) => k * Math.atanh(Math.max(-0.995, Math.min(0.995, o))); }
  m = /^hybrid([0-9]+)$/.exec(name);
  if (m) { const k = Number(m[1]); return (o, s, c) => engine.evaluateState(s, c) + k * o; }
  throw new Error("unknown output map: " + name);
}
function makeEvalFn(spec) {
  const at = spec.lastIndexOf("@");
  const base = at > 0 ? spec.slice(0, at) : spec;
  const map = outputMap(at > 0 ? spec.slice(at + 1) : "");
  if (base === "handcoded") return undefined;
  const weights = loadWeights(base);
  return (s, c) => {
    const input = encodeBoard(s.board, c, s.deckSlots);
    if (input === null) return engine.evaluateState(s, c);
    return map(forward(weights, input), s, c);
  };
}
function stateFromRecord(rec) {
  const s = engine.cloneState({});
  s.board = rec.board.map((r) => r.map((p) => (p ? { type: p.t, color: p.c, moved: true } : null)));
  s.mode = "play";
  s.turn = rec.turn;
  s.deckSlots = { white: rec.deckSlots?.white || [], black: rec.deckSlots?.black || [] };
  s.captures = { white: [], black: [] };
  s.aiSearchNoCards = true;
  // a hand-built state must carry these too, otherwise root safety rejects every candidate (score 250000000, 0 nodes)
  s.turnsTaken = { white: 10, black: 10 };
  s.actionsRemaining = 1;
  s.moveCount = 20;
  s.castlingCanceled = { white: true, black: true };
  engine.setWorkerBoardDimensions(s);
  return s;
}
function search(state, color, opts) {
  const actions = engine.generateActions(state, color);
  if (!actions.length) return null;
  return engine.searchBestAction(state, actions, color, opts.depth, opts.ms, { flexibleBudget: true, evalFn: opts.evalFn, limits: opts.limits });
}
const key = (a) => JSON.stringify(a);
function rng(seed) { let x = seed >>> 0 || 1; return () => ((x = (x * 1664525 + 1013904223) >>> 0) / 4294967296); }

const [mode, ...args] = process.argv.slice(2);
if (mode === "build") {
  const [data, out, count = "60", refMs = "6000", shortMs = "200", seed = "1"] = args;
  const lines = fs.readFileSync(data, "utf8").split("\n").filter(Boolean);
  const rand = rng(Number(seed));
  const picked = [];
  const kept = [];
  let tried = 0;
  while (kept.length < Number(count) && tried < Number(count) * 12) {
    tried++;
    const rec = JSON.parse(lines[Math.floor(rand() * lines.length)]);
    const pieces = rec.board.flat().filter(Boolean).length;
    if (pieces < 8 || pieces > 30) continue;
    const s = stateFromRecord(rec);
    const quick = search(s, rec.turn, { depth: 2, ms: Number(shortMs) });
    if (!quick || !quick.action) continue;
    const ref = search(stateFromRecord(rec), rec.turn, { depth: 4, ms: Number(refMs), limits: { movetimeMs: Number(refMs), minDepth: 3, extend: true } });
    if (!ref || !ref.action || (ref.completedDepth || 0) < 3) continue;
    if (key(ref.action) === key(quick.action)) continue;
    kept.push({ board: rec.board, deckSlots: rec.deckSlots, turn: rec.turn, refAction: ref.action, refScore: ref.score, refDepth: ref.completedDepth });
    console.log(`kept ${kept.length}/${count} (tried ${tried}) ref depth ${ref.completedDepth} score ${Math.round(ref.score)}`);
  }
  fs.writeFileSync(out, JSON.stringify(kept));
  console.log("wrote", out, kept.length);
} else if (mode === "eval") {
  const [file, spec, ms = "300", depth = "3"] = args;
  const set = JSON.parse(fs.readFileSync(file, "utf8"));
  const evalFn = makeEvalFn(spec);
  let hit = 0;
  for (const p of set) {
    const r = search(stateFromRecord(p), p.turn, { depth: Number(depth), ms: Number(ms), evalFn });
    if (r && r.action && key(r.action) === key(p.refAction)) hit++;
  }
  console.log(`${spec}: found the reference move in ${hit}/${set.length} (${((100 * hit) / set.length).toFixed(1)}%) at depth ${depth}, ${ms} ms`);
} else {
  console.log("usage: node nnue/tactics.js build|eval ...");
  process.exit(1);
}
