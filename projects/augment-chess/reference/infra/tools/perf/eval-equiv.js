// Usage: node tools/perf/eval-equiv.js [N=3200] [--bench]
// Compares engine-orig.js (git HEAD copy) vs engine-merged.js on sampled positions.
const fs = require("fs"), path = require("path");
const root = path.join(__dirname, "..", "..");
const N = +process.argv[2] || 3200;
const orig = require("./engine-orig.js");
const cur = require(path.join(root, "engine-merged.js"));
const lines = fs.readFileSync(path.join(root, "data","experiments","selfplay-data.merged-engine-16cards-local-2026-09-15.jsonl"), "utf8").split("\n").filter(Boolean);
const step = Math.max(1, Math.floor(lines.length / N));
const recs = [];
for (let i = 0; i < lines.length && recs.length < N; i += step) { try { recs.push(JSON.parse(lines[i])); } catch (e) {} }
function makeState(engine, board, deckSlots) {
  const state = engine.cloneState({});
  state.board = board.map((row) => row.map((p) => (p ? { type: p.t, color: p.c, moved: true } : null)));
  state.mode = "play";
  state.deckSlots = deckSlots ? { white: deckSlots.white || [], black: deckSlots.black || [] } : { white: [], black: [] };
  state.captures = { white: [], black: [] };
  state.aiSearchNoCards = true;
  engine.setWorkerBoardDimensions(state);
  return state;
}
const same = (a, b) => a === b || (a !== a && b !== b);
let diffs = 0, checked = 0, terminals = 0;
const tOrig = [], tCur = [];
let totO = 0, totC = 0;
const cachePath = path.join(__dirname, "orig-results.json");
const saved = (process.env.NOSAVE || fs.existsSync(cachePath)) ? null : [];
for (let r = 0; r < 1; r++) { // rep 0 = correctness, rep 1 = timing (fresh states)
  for (let i = 0; i < recs.length; i++) {
    const rec = recs[i]; if (i % 500 === 0) console.error("pos", i);
    for (const color of ["white", "black"]) {
      const so = makeState(orig, rec.board, rec.deckSlots), sc = makeState(cur, rec.board, rec.deckSlots);
      let t = process.hrtime.bigint();
      const a = orig.evaluateStateComponents(so, color);
      const t1 = process.hrtime.bigint();
      const b = cur.evaluateStateComponents(sc, color);
      const t2 = process.hrtime.bigint();
      totO += Number(t1 - t); totC += Number(t2 - t1);
      {
        checked++; if (a.terminal !== null) terminals++;
        if (saved) saved.push(a);
        const ka = Object.keys(a), kb = Object.keys(b);
        let bad = ka.length !== kb.length;
        for (const k of ka) if (!same(a[k], b[k])) bad = true;
        if (bad) { diffs++; if (diffs < 6) console.log("DIFF pos", i, color, JSON.stringify(a), JSON.stringify(b)); }
      }
    }
  }
  
}
if (saved) fs.writeFileSync(cachePath, JSON.stringify(saved));
console.log(`checked ${checked} (pos x color), terminal ${terminals}, diffs ${diffs}`);
console.log(`orig ${(totO/1e6/(recs.length*2)).toFixed(3)} ms/call, new ${(totC/1e6/(recs.length*2)).toFixed(3)} ms/call, speedup ${(totO/totC).toFixed(2)}x`);
process.exit(diffs ? 1 : 0);
