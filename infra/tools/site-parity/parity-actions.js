// generateActions parity on random sparse boards. Usage: node parity-actions.js [engine|-] [N=400] [seed=12345]
const C = require("./common"); const { real } = C; const args = C.parseArgs(); const eng = require(args.engine);
const N = args.N || 400, rng = C.rngMaker(args.seed || C.defaultSeed(1));
const TYPES = ["pawn","knight","bishop","rook","queen","amazon","cardinal","pegasus","assassin","dragon","cannon","grasshopper","hook","herald","camel","alfil","ferz","eagle","berserker","magicGirl","thief","paladin","octopus","clockwork","brutus","checker","campfire","princess","hedgehog","undead","siren","trickster","slime","scarecrow","merchant"];
function makeBoard() {
  const b = Array.from({ length: 8 }, () => Array(8).fill(null));
  const place = (type, color) => { for (let t = 0; t < 30; t++) { const r = Math.floor(rng()*8), c = Math.floor(rng()*8); if (!b[r][c] && !(type==="pawn" && (r===0||r===7))) { b[r][c] = { type, color, moved: rng()<0.5 }; return; } } };
  place("king","white"); place("king","black");
  const n = 3 + Math.floor(rng()*7);
  for (let i = 0; i < n; i++) place(TYPES[Math.floor(rng()*TYPES.length)], rng()<0.5 ? "white" : "black");
  return b;
}
let states = 0, diffStates = 0, errs = 0; const byCause = {}, byCard = {}; const samples = [];
for (let i = 0; i < N; i++) {
  const board = makeBoard(), decks = C.makeDecks(rng, 3), color = rng()<0.5 ? "white" : "black";
  let A, B;
  try { A = real.generateActions(C.makeState(real, board, decks, color), color).map(C.norm).sort(); B = eng.generateActions(C.makeState(eng, board, decks, color), color).map(C.norm).sort(); }
  catch (e) { errs++; if (samples.length < 3) samples.push("ERR " + e.message); continue; }
  states++;
  if (A.length !== B.length || A.some((x, j) => x !== B[j])) {
    diffStates++; const sa = new Set(A), sb = new Set(B);
    const onlyReal = A.filter((x) => !sb.has(x)), onlyEng = B.filter((x) => !sa.has(x));
    const key = (onlyReal.length ? "R" : "") + (onlyEng.length ? "E" : ""); byCause[key] = (byCause[key]||0)+1;
    for (const x of [...onlyReal, ...onlyEng]) { const o = JSON.parse(x); const k = o.type === "card" ? "card:" + o.cardId : "move"; byCard[k] = (byCard[k]||0)+1; }
    if (samples.length < 8) samples.push(`i=${i} ${color} real=${A.length} eng=${B.length} onlyReal=${onlyReal.length} onlyEng=${onlyEng.length}\n   real-only: ${(onlyReal[0]||"").slice(0,160)}\n   eng-only : ${(onlyEng[0]||"").slice(0,160)}`);
  }
}
console.log(`compared=${states} errors=${errs} differing=${diffStates} (${(100*diffStates/Math.max(1,states)).toFixed(1)}%) sides=${JSON.stringify(byCause)} byAction=${JSON.stringify(byCard)}`);
samples.forEach((s) => console.log(s));
