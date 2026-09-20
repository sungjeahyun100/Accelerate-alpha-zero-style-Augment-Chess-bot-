// Applies the SAME random action in site worker and engine; compares resulting state.
// The action is taken from the real list and re-found in the engine list by normalized content.
// Usage: node parity-apply.js [engine|-] [N=300] [seed=777]
const C = require("./common"); const { real } = C; const args = C.parseArgs(); const eng = require(args.engine);
const N = args.N || 300, rng = C.rngMaker(args.seed || 777);
const TYPES = ["pawn","knight","bishop","rook","queen","amazon","cardinal","grasshopper","hook","camel","alfil","berserker","thief","paladin","octopus","clockwork","brutus","checker","campfire","princess","hedgehog","undead","siren","trickster","slime","scarecrow","merchant"];
function makeBoard() {
  const b = Array.from({ length: 8 }, () => Array(8).fill(null));
  const place = (type, color) => { for (let t = 0; t < 30; t++) { const r = Math.floor(rng()*8), c = Math.floor(rng()*8); if (!b[r][c] && !(type==="pawn" && (r===0||r===7))) { b[r][c] = { type, color, moved: rng()<0.5 }; return; } } };
  place("king","white"); place("king","black");
  const n = 3 + Math.floor(rng()*7);
  for (let i = 0; i < n; i++) place(TYPES[Math.floor(rng()*TYPES.length)], rng()<0.5 ? "white" : "black");
  return b;
}
function sig(st) {
  const bd = st.board.map((row) => row.map((p) => p ? [p.type,p.color,!!p.moved,!!p.shielded,!!p.frozen,!!p.witchTrial,p.witchTrial?.countBy||"",p.frozenByCard?.countBy||"",!!p.recurrence,!!p.defected,p.freshNoCaptureUntil??""].join(":") : "-").join(",")).join("|");
  return [st.mode, st.winner||"", bd,
    JSON.stringify((st.pendingScarecrows||[]).map((e) => [e.row,e.col,!!e.reserved,e.remainingOwnTurns??"",e.by||""])),
    JSON.stringify((st.pendingGales||[]).map((e) => [e.color,e.remainingOwnTurns??"",e.triggerTurn??""])),
    JSON.stringify(st.othelloPending||null), JSON.stringify(st.reversal||null),
    JSON.stringify(st.platformRule ? [st.platformRule.enabled, st.platformRule.cadence||"", st.platformRule.nextAt, st.platformRule.countUnit] : null), st.turn].join("\n");
}
let cmp = 0, diff = 0, errs = 0, missing = 0; const byCard = {}; const samples = [];
for (let i = 0; i < N; i++) {
  const board = makeBoard(), decks = C.makeDecks(rng, 3), color = rng()<0.5 ? "white" : "black";
  const actions = real.generateActions(C.makeState(real, board, decks, color), color);
  const cards = actions.filter((a) => a.type === "card");
  const pool = cards.length && rng() < 0.85 ? cards : actions;
  if (!pool.length) continue;
  const act = pool[Math.floor(rng()*pool.length)];
  try {
    const A = C.makeState(real, board, decks, color), B = C.makeState(eng, board, decks, color);
    const lb = eng.generateActions(B, color); const key = C.norm(act); const ea = lb.find((z) => C.norm(z) === key);
    if (!ea) { missing++; continue; } // generation parity is parity-actions.js's job
    const ra = real.applyAction(A, JSON.parse(JSON.stringify(act)), color), rb = eng.applyAction(B, JSON.parse(JSON.stringify(ea)), color);
    cmp++;
    if (!(ra.ok === rb.ok && (!ra.ok || sig(A) === sig(B)))) { diff++; const k = act.type === "card" ? act.cardId : "move"; byCard[k] = (byCard[k]||0)+1;
      if (samples.length < 6) { const la = sig(A).split("\n"), lb2 = sig(B).split("\n"); const k2 = la.findIndex((x, j) => x !== lb2[j]);
        samples.push(`i=${i} ${act.type}:${act.cardId||""} realOk=${ra.ok} engOk=${rb.ok} line=${k2}\n   real: ${(la[k2]||"").slice(0,200)}\n   eng : ${(lb2[k2]||"").slice(0,200)}`); } }
  } catch (e) { errs++; if (samples.length < 6) samples.push("ERR " + e.message.slice(0,150)); }
}
console.log(`applied=${cmp} notInEngineList=${missing} errors=${errs} differing=${diff} byAction=${JSON.stringify(byCard)}`); samples.forEach((s) => console.log(s));
