// Checks the Stockfish-style search limits (options.limits) of engine-merged.js
// on real positions:  node tools/perf/limits-test.js [N=30]
//   A  soft time only (extend off)                -> baseline depth/time
//   B  soft time + progress-based extension       -> never shallower than A, time <= hard cap
//   C  minDepth 2 with a tiny soft time           -> depth 2 reached
//   D  infinite time + depth 2                    -> exactly depth 2
//   E  infinite time + node limit                 -> stops near the node limit
// Exit code 1 if an invariant is violated.
const fs = require("fs"), path = require("path");
const root = path.join(__dirname, "..", "..");
globalThis.self = globalThis; globalThis.addEventListener = () => {};
const engine = require(path.join(root, "engine-merged.js"));
const N = +process.argv[2] || 30;
const lines = fs.readFileSync(path.join(root, "data", "experiments", "selfplay-data.merged-engine-16cards-local-2026-09-15.jsonl"), "utf8").split("\n").filter(Boolean);
const step = Math.floor(lines.length / N);
function mk(rec) {
  const s = engine.cloneState({});
  s.board = rec.board.map((r) => r.map((p) => (p ? { type: p.t, color: p.c, moved: true } : null)));
  s.mode = "play"; s.turn = rec.turn;
  s.deckSlots = { white: rec.deckSlots?.white || [], black: rec.deckSlots?.black || [] };
  s.captures = { white: [], black: [] }; s.aiSearchNoCards = true;
  engine.setWorkerBoardDimensions(s); return s;
}
const SOFT = +process.env.SOFT || 3000, HARD = +process.env.HARD || 6000, SLACK = 800; // slack: the engine only checks the clock between nodes
let n = 0, fails = 0, extendedCount = 0, sumA = 0, sumB = 0, deeper = 0, maxOver = 0, cOk = 0, cN = 0, dOk = 0, dN = 0, eOk = 0, eN = 0;
const fail = (msg) => { fails++; if (fails <= 8) console.log("FAIL", msg); };
for (let i = 0; i < lines.length && n < N; i += step) {
  const rec = JSON.parse(lines[i]); n++;
  const run = (limits) => { const s = mk(rec); const a = engine.generateActions(s, rec.turn); if (a.length < 2) return null; return engine.searchBestAction(s, a, rec.turn, 12, 5000, { limits }); };
  process.stderr.write("pos " + i + "\n");
  const A = run({ movetimeMs: SOFT, extend: false, predictiveStop: false });
  if (!A) continue;
  const B = run({ movetimeMs: SOFT, hardTimeMs: HARD });
  if (A.opening || B.opening || A.forced) { n--; continue; }
  sumA += A.completedDepth; sumB += B.completedDepth;
  if (B.extended) extendedCount++;
  if (B.completedDepth > A.completedDepth) deeper++;
  if (B.timeMs > HARD + SLACK) fail(`pos ${i}: B took ${B.timeMs.toFixed(0)}ms > hard ${HARD}+${SLACK}`);
  maxOver = Math.max(maxOver, B.timeMs - HARD);
  if (B.completedDepth + 1 < A.completedDepth) fail(`pos ${i}: B depth ${B.completedDepth} << A depth ${A.completedDepth}`);
  const C = run({ movetimeMs: 100, minDepth: 2, hardTimeMs: 15000 }); cN++;
  if (C.completedDepth >= 2) cOk++; else if (C.timeMs < 15000 - 400) fail(`pos ${i}: C stopped at depth ${C.completedDepth} after ${C.timeMs.toFixed(0)}ms (< hard)`);
  const D = run({ infinite: true, depth: 2 }); dN++;
  if (D.completedDepth === 2) dOk++; else fail(`pos ${i}: D completedDepth ${D.completedDepth} != 2`);
  const E = run({ infinite: true, nodes: 15000, depth: 10 }); eN++;
  if (E.nodes <= 15000 * 1.5 + 2000) eOk++; else fail(`pos ${i}: E used ${E.nodes} nodes for a 15000 limit`);
}
console.log(`positions ${n}: soft ${SOFT}ms / hard ${HARD}ms`);
console.log(`  mean completed depth  A(no extend) ${(sumA / n).toFixed(2)}  ->  B(extend) ${(sumB / n).toFixed(2)};  B deeper in ${deeper}/${n}, extension used in ${extendedCount}/${n}, max overshoot past hard ${maxOver.toFixed(0)}ms`);
console.log(`  C minDepth 2 reached: ${cOk}/${cN}   D exact depth: ${dOk}/${dN}   E node limit respected: ${eOk}/${eN}`);
if (fails) { console.log(`${fails} invariant violation(s)`); process.exit(1); }
console.log("limits-test: OK");
