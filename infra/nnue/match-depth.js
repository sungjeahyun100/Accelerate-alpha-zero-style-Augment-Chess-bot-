// Handcoded evaluator at two DIFFERENT search depths, head to head, using the
// real self-play game loop. Answers "does more depth actually help?" (TODO:
// decides whether evaluateStateComponents speed work is worth it).
// Games run in color-swapped pairs sharing a seed (same as match-two-models.js).
//
// Usage: node match-depth.js <depthA> <depthB> [pairCount] [pairStart]
// Env: MATCH_SEARCH_MS (base time per move, default 1500 -- must be large
// enough for the deeper side to actually reach its depth, otherwise the
// comparison silently degrades to "same depth, different label"), MATCH_MAX_PLIES.
const path = require("path");
const [, , dA, dB, pairCountArg, pairStartArg] = process.argv;
if (!dA || !dB) { console.error("Usage: node match-depth.js <depthA> <depthB> [pairCount] [pairStart]"); process.exit(1); }
const DEPTH_A = Number(dA), DEPTH_B = Number(dB);
const PAIR_COUNT = Number(pairCountArg) || 10, PAIR_START = Number(pairStartArg) || 0;
const TIME_MS = Number(process.env.MATCH_SEARCH_MS) || 1500;
const MAX_PLIES = Number(process.env.MATCH_MAX_PLIES) || 200;
const { playOneGame } = require(path.join(__dirname, "..", "selfplay-worker-merged.js"));
let a = 0, b = 0, d = 0, u = 0;
function play(seed, aWhite, label) {
  const t0 = Date.now();
  const r = playOneGame({ searchDepth: DEPTH_A, searchTimeMs: TIME_MS, maxPlies: MAX_PLIES, seed, flexibleBudget: true,
    searchDepthByColor: aWhite ? { white: DEPTH_A, black: DEPTH_B } : { white: DEPTH_B, black: DEPTH_A } });
  const aColor = aWhite ? "white" : "black";
  const res = r.outcome === "draw" ? "draw" : r.outcome === "unfinished" ? "unfinished" : r.outcome === aColor ? "A" : "B";
  if (res === "A") a++; else if (res === "B") b++; else if (res === "draw") d++; else u++;
  console.log(`RESULT ${label} seed=${seed} A(depth ${DEPTH_A})=${aColor} plies=${r.plies} outcome=${r.outcome} -> ${res} (${((Date.now() - t0) / 1000).toFixed(0)}s)`);
}
console.log(`depth ${DEPTH_A} (A) vs depth ${DEPTH_B} (B), pairs ${PAIR_START}..${PAIR_START + PAIR_COUNT - 1}, time=${TIME_MS}ms`);
for (let i = PAIR_START; i < PAIR_START + PAIR_COUNT; i++) {
  const seed = 2000000 + i * 7919;
  play(seed, true, `pair ${i} g1`); play(seed, false, `pair ${i} g2`);
}
console.log(`SUMMARY A=${a} B=${b} draws=${d} unfinished=${u}`);
