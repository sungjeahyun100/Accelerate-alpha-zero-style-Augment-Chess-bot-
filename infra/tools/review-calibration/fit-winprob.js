// Fits how a search score maps to the real chance of winning, from self-play
// data (positions carry the mover's searchScore and the game's final outcome).
// Groundwork for classifying review moves by WIN-PROBABILITY LOSS instead of
// fixed score gaps: a 100-point drop matters little when +1500 ahead and a lot
// when equal.
//
//   node tools/review-calibration/fit-winprob.js <data.jsonl> [more.jsonl ...]
//
// Model: expectedScore(s) = 1 / (1 + exp(-s / k))  (win = 1, draw = 0.5, loss = 0).
// k is fitted by grid search on squared error, overall and per completedDepth.
// Sentinel scores (forced/opening-book INF fractions) are clipped.
const fs = require("fs");
const files = process.argv.slice(2);
if (!files.length) { console.error("Usage: node fit-winprob.js <data.jsonl> [...]"); process.exit(1); }

const rows = [];
for (const f of files) {
  for (const line of fs.readFileSync(f, "utf8").split("\n")) {
    if (!line) continue;
    let r; try { r = JSON.parse(line); } catch (e) { continue; }
    if (typeof r.searchScore !== "number" || r.outcome === undefined || r.outcome === null) continue;
    if (r.unfinished) continue;
    rows.push({ s: Math.max(-3000, Math.min(3000, r.searchScore)), y: r.outcome === 1 ? 1 : r.outcome === -1 ? 0 : 0.5, d: Number(r.completedDepth) || 0 });
  }
}
console.log(`positions with a search score and a finished game: ${rows.length}`);

function sse(list, k) { let t = 0; for (const r of list) { const p = 1 / (1 + Math.exp(-r.s / k)); t += (p - r.y) ** 2; } return t / list.length; }
function fit(list) {
  let best = { k: null, err: Infinity };
  for (let k = 100; k <= 4000; k += 50) { const e = sse(list, k); if (e < best.err) best = { k, err: e }; }
  return best;
}
const base = list => { let m = 0; for (const r of list) m += r.y; m /= list.length; let v = 0; for (const r of list) v += (r.y - m) ** 2; return v / list.length; };

const all = fit(rows);
console.log(`\noverall: k = ${all.k}  (mean squared error ${all.err.toFixed(4)} vs ${base(rows).toFixed(4)} for always predicting the average)`);

console.log("\nper completed depth:");
for (const d of [...new Set(rows.map(r => r.d))].sort((a, b) => a - b)) {
  const list = rows.filter(r => r.d === d);
  if (list.length < 300) continue;
  const f = fit(list);
  console.log(`  depth ${d}: n=${String(list.length).padStart(6)}  k=${String(f.k).padStart(4)}  mse=${f.err.toFixed(4)} (baseline ${base(list).toFixed(4)})`);
}

console.log("\nobserved vs fitted (all depths), bins of score:");
const edges = [-3000, -1500, -700, -350, -150, -50, 50, 150, 350, 700, 1500, 3001];
for (let i = 0; i < edges.length - 1; i++) {
  const list = rows.filter(r => r.s >= edges[i] && r.s < edges[i + 1]);
  if (!list.length) continue;
  const obs = list.reduce((a, r) => a + r.y, 0) / list.length;
  const mid = list.reduce((a, r) => a + r.s, 0) / list.length;
  const fitted = 1 / (1 + Math.exp(-mid / all.k));
  console.log(`  [${String(edges[i]).padStart(5)}, ${String(edges[i + 1]).padStart(5)})  n=${String(list.length).padStart(6)}  observed ${obs.toFixed(3)}  fitted ${fitted.toFixed(3)}`);
}

// What a fixed win-probability loss means in score points at different situations.
console.log("\nscore drop equal to a given win-probability loss (k = " + all.k + "):");
for (const start of [0, 300, 800, 1500]) {
  const p0 = 1 / (1 + Math.exp(-start / all.k));
  const cells = [0.02, 0.05, 0.10, 0.20].map(dp => {
    const p1 = Math.max(0.001, p0 - dp);
    const s1 = -all.k * Math.log(1 / p1 - 1);
    return `${(dp * 100).toFixed(0)}%: ${(start - s1).toFixed(0)}`;
  });
  console.log(`  starting at score ${String(start).padStart(4)} (win ${(p0 * 100).toFixed(0)}%):  ${cells.join("   ")}`);
}
