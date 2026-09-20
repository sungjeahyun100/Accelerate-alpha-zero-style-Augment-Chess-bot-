// Regression guard for evaluateStateComponents. The NNUE feature values come
// from this function, so an unintended change silently invalidates every
// trained model. Deterministic random boards (special pieces, card decks,
// socialism/platform/zugzwang state) are evaluated for both colors and the
// per-position hash is compared against golden-eval.json.
//   node tools/ci/golden-eval.js          -> check (exit 1 on any difference)
//   node tools/ci/golden-eval.js --update -> rewrite golden-eval.json
// An INTENTIONAL eval change (new rules, balance patch) must run --update and
// commit the new golden file together with a note in the commit message.
globalThis.self = globalThis; globalThis.addEventListener = () => {};
const fs = require("fs"), path = require("path"), crypto = require("crypto");
const engine = require(path.join(__dirname, "..", "..", "engine-merged.js"));
const GOLDEN = path.join(__dirname, "golden-eval.json");
const N = 400;
let x = 31337 >>> 0; const rng = () => ((x = (x * 1664525 + 1013904223) >>> 0) / 4294967296);
const TYPES = ["pawn","knight","bishop","rook","queen","amazon","cardinal","grasshopper","hook","camel","alfil","berserker","thief","paladin","octopus","clockwork","brutus","checker","campfire","princess","hedgehog","undead","siren","trickster","slime","scarecrow","merchant","cannon","eagle","magicGirl","colossus","bigRook","wall"];
const POOL = ["campfire","reversal","scarecrow","othello","gale","freeze","witchTrial","locustSwarm","thief","parrot","paladin","metal","brutus","clockwork","octopus","pawnStorm","platformRule","desperado","royalShield","charge","disarm","socialism","cleanupPieces","substitution","promotionRush","vip","ultimatum","zugzwang"];
function mk(board, decks, extra, color) {
  const st = engine.cloneState({}); st.board = JSON.parse(JSON.stringify(board)); st.mode = "play"; st.turn = color; st.actionsRemaining = 1;
  st.deckSlots = JSON.parse(JSON.stringify(decks)); st.captures = { white: [], black: [] }; st.aiSearchNoCards = false; Object.assign(st, JSON.parse(JSON.stringify(extra)));
  if (st.board.some((r) => r.some((p) => p?.type === "campfire"))) st.hasCampfire = true;
  engine.setWorkerBoardDimensions(st); return st;
}
const hashes = [];
for (let i = 0; i < N; i++) {
  const b = Array.from({ length: 8 }, () => Array(8).fill(null));
  const put = (t, c) => { for (let k = 0; k < 40; k++) { const r = Math.floor(rng()*8), col = Math.floor(rng()*8); if (!b[r][col] && !(t === "pawn" && (r === 0 || r === 7))) { b[r][col] = { type: t, color: t === "wall" ? null : c, moved: rng() < 0.5 }; return; } } };
  put("king", "white"); put("king", "black");
  const cnt = 3 + Math.floor(rng() * 14); for (let k = 0; k < cnt; k++) put(TYPES[Math.floor(rng()*TYPES.length)], rng() < 0.5 ? "white" : "black");
  const decks = { white: [], black: [] };
  for (const col of ["white","black"]) for (let k = 0; k < 4; k++) { const e = POOL[Math.floor(rng()*POOL.length)]; decks[col].push({ id: e, instanceId: col+e+k, effect: e, stars: 2, used: rng() < 0.2, recovering: false }); }
  const extra = {}; if (rng() < 0.2) extra.socialism = { white: 1, black: 0 }; if (rng() < 0.2) extra.platformRule = { enabled: true, countUnit: "ply", nextAt: 10 }; if (rng() < 0.2) extra.zugzwang = { white: true, black: false };
  for (const color of ["white", "black"]) {
    let r; try { r = JSON.stringify(engine.evaluateStateComponents(mk(b, decks, extra, color), color)); } catch (e) { r = "ERR:" + e.message; }
    hashes.push(crypto.createHash("sha1").update(r).digest("hex").slice(0, 12));
  }
}
if (process.argv.includes("--update")) { fs.writeFileSync(GOLDEN, JSON.stringify({ n: hashes.length, hashes }) + "\n"); console.log("golden updated:", hashes.length); process.exit(0); }
const g = JSON.parse(fs.readFileSync(GOLDEN, "utf8"));
let diffs = 0; const firstBad = [];
hashes.forEach((h, i) => { if (h !== g.hashes[i]) { diffs++; if (firstBad.length < 5) firstBad.push(i); } });
console.log(`golden-eval: ${hashes.length} evaluations, ${diffs} differ`, diffs ? "first at " + firstBad.join(",") : "");
if (diffs || hashes.length !== g.n) { console.error("FAIL: evaluateStateComponents output changed. If intentional, run: node tools/ci/golden-eval.js --update"); process.exit(1); }
