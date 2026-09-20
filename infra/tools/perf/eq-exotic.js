globalThis.self = globalThis; globalThis.addEventListener = () => {};
const oldE = require("D:/증강체스엔진/tools/perf/engine-orig.js");
const newE = require("D:/증강체스엔진/engine-merged.js");
let x = 31337 >>> 0; const rng = () => ((x = (x * 1664525 + 1013904223) >>> 0) / 4294967296);
const TYPES = ["pawn","knight","bishop","rook","queen","amazon","cardinal","grasshopper","hook","camel","alfil","berserker","thief","paladin","octopus","clockwork","brutus","checker","campfire","princess","hedgehog","undead","siren","trickster","slime","scarecrow","merchant","cannon","eagle","magicGirl","colossus","bigRook","wall"];
const POOL = ["campfire","reversal","scarecrow","othello","gale","freeze","witchTrial","locustSwarm","thief","parrot","paladin","metal","brutus","clockwork","octopus","pawnStorm","platformRule","desperado","royalShield","charge","disarm","socialism","cleanupPieces","substitution","promotionRush","vip","ultimatum","zugzwang"];
function mk(engine, board, decks, extra) { const st = engine.cloneState({}); st.board = JSON.parse(JSON.stringify(board)); st.mode = "play"; st.turn = "white"; st.actionsRemaining = 1; st.deckSlots = JSON.parse(JSON.stringify(decks)); st.captures = { white: [], black: [] }; st.aiSearchNoCards = false; Object.assign(st, JSON.parse(JSON.stringify(extra))); if (st.board.some((r) => r.some((p) => p?.type === "campfire"))) st.hasCampfire = true; engine.setWorkerBoardDimensions(st); return st; }
let n = 0, diffs = 0; const samples = [];
for (let i = 0; i < 2000; i++) {
  const b = Array.from({ length: 8 }, () => Array(8).fill(null));
  const put = (t, c) => { for (let k = 0; k < 40; k++) { const r = Math.floor(rng()*8), col = Math.floor(rng()*8); if (!b[r][col] && !(t === "pawn" && (r === 0 || r === 7))) { b[r][col] = { type: t, color: t === "wall" ? null : c, moved: rng() < 0.5 }; return; } } };
  put("king", "white"); put("king", "black");
  const cnt = 3 + Math.floor(rng() * 14); for (let k = 0; k < cnt; k++) put(TYPES[Math.floor(rng()*TYPES.length)], rng() < 0.5 ? "white" : "black");
  const decks = { white: [], black: [] };
  for (const col of ["white","black"]) for (let k = 0; k < 4; k++) { const e = POOL[Math.floor(rng()*POOL.length)]; decks[col].push({ id: e, instanceId: col+e+k, effect: e, stars: 2, used: rng() < 0.2, recovering: false }); }
  const extra = {}; if (rng() < 0.2) extra.socialism = { white: 1, black: 0 }; if (rng() < 0.2) extra.platformRule = { enabled: true, countUnit: "ply", nextAt: 10 }; if (rng() < 0.2) extra.zugzwang = { white: true, black: false };
  for (const color of ["white", "black"]) {
    let A, B;
    try { A = oldE.evaluateStateComponents(mk(oldE, b, decks, extra), color); } catch (e) { A = "ERR:" + e.message; }
    try { B = newE.evaluateStateComponents(mk(newE, b, decks, extra), color); } catch (e) { B = "ERR:" + e.message; }
    n++;
    const sa = JSON.stringify(A), sb = JSON.stringify(B);
    if (sa !== sb) { diffs++; if (samples.length < 4) samples.push(sa.slice(0, 200) + "\n   vs " + sb.slice(0, 200)); }
  }
}
console.log("compared", n, "diffs", diffs); samples.forEach((s) => console.log(" ", s));
