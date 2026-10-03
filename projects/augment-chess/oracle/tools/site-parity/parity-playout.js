// Multi-ply playouts: same random action (matched by normalized content) in both; stop at first divergence and classify.
// Usage: node parity-playout.js [engine|-] [GAMES=30] [seed=4242] [PLIES=40]
const C = require("./common"); const { real } = C; const args = C.parseArgs(); const eng = require(args.engine);
const GAMES = args.N || 30, PLIES = args.plies, rng = C.rngMaker(args.seed || C.defaultSeed(3));
const SPECIAL = ["amazon","cardinal","grasshopper","hook","camel","berserker","thief","paladin","octopus","clockwork","brutus","checker","campfire","princess","scarecrow","slime","trickster"];
function startBoard() {
  const back = ["rook","knight","bishop","queen","king","bishop","knight","rook"];
  const b = Array.from({ length: 8 }, () => Array(8).fill(null));
  for (const [col, br, pr] of [["black",0,1],["white",7,6]]) for (let c=0;c<8;c++){ b[pr][c]={type:"pawn",color:col,moved:false}; b[br][c]={type:back[c],color:col,moved:false}; }
  for (let k = 0; k < 3; k++) { const t = SPECIAL[Math.floor(rng()*SPECIAL.length)], c = Math.floor(rng()*8); const col = rng()<0.5?"white":"black"; const r = col==="white"?5:2; if (!b[r][c]) b[r][c]={type:t,color:col,moved:false}; }
  return b;
}
function sig(st) {
  const bd = st.board.map((row) => row.map((p) => p ? [p.type,p.color,!!p.moved,!!p.shielded,!!p.frozen,!!p.witchTrial,!!p.recurrence,!!p.defected].join(":") : "-").join(",")).join("|");
  return [st.mode, st.winner||"", st.turn, bd, JSON.stringify((st.pendingScarecrows||[]).map((e)=>[e.row,e.col,!!e.reserved,e.remainingOwnTurns??""])), JSON.stringify((st.pendingGales||[]).map((e)=>[e.color,e.remainingOwnTurns??"",e.triggerTurn??""]))].join("\n");
}
const desc = (a) => a.type + ":" + (a.cardId || "") + " " + JSON.stringify(a.from || "") + JSON.stringify(a.move || a.target || "");
let games = 0, plies = 0, cardPlies = 0, diverged = 0; const byCause = {}; const samples = [];
for (let g = 0; g < GAMES; g++) {
  const board = startBoard(), decks = C.makeDecks(rng, 4);
  const A = C.makeState(real, board, decks, "white"), B = C.makeState(eng, board, decks, "white");
  games++; const hist = [];
  const report = (cause, msg) => { diverged++; byCause[cause] = (byCause[cause]||0)+1; if (samples.length < 80) samples.push(`[${cause}] game ${g} ply ${hist.length} hist=[${hist.slice(-3).join(" ; ")}]\n   ${msg}`); };
  for (let p = 0; p < PLIES; p++) {
    if (A.mode === "gameover") break;
    const col = A.turn, la = real.generateActions(A, col), lb = eng.generateActions(B, col);
    const na = la.map(C.norm), nb = lb.map(C.norm), sa = new Set(na), sb = new Set(nb);
    const last = hist.length ? hist[hist.length-1].split(" ")[1] : "start";
    if (na.length !== nb.length || na.some((s) => !sb.has(s)) || nb.some((s) => !sa.has(s))) {
      const oR = na.filter((s) => !sb.has(s)), oE = nb.filter((s) => !sa.has(s));
      report("actions-differ after " + last, `real=${na.length} eng=${nb.length} onlyReal=${(oR[0]||"").slice(0,150)} onlyEng=${(oE[0]||"").slice(0,150)}`); break;
    }
    if (!la.length) break;
    const cards = la.filter((a) => a.type === "card");
    const act = (cards.length && rng() < 0.3) ? cards[Math.floor(rng()*cards.length)] : la[Math.floor(rng()*la.length)];
    const ea = lb[nb.indexOf(C.norm(act))]; // match by normalized content, not list index
    if (act.type === "card") cardPlies++;
    const ra = real.applyAction(A, JSON.parse(JSON.stringify(act)), col), rb = eng.applyAction(B, JSON.parse(JSON.stringify(ea)), col);
    plies++; hist.push(col + " " + desc(act));
    if (ra.ok !== rb.ok || (ra.ok && sig(A) !== sig(B))) {
      const x = sig(A).split("\n"), y = sig(B).split("\n"); const k = x.findIndex((s, j) => s !== y[j]);
      report("state-differs after " + (act.type === "card" ? act.cardId : "move"), `okReal=${ra.ok} okEng=${rb.ok} line ${k}\n   real: ${(x[k]||"").slice(0,160)}\n   eng : ${(y[k]||"").slice(0,160)}`); break;
    }
    if (!ra.ok) break;
  }
}
console.log(`games=${games} plies=${plies} (card plies=${cardPlies}) diverged=${diverged} byCause=${JSON.stringify(byCause)}`); samples.forEach((s) => console.log(s));
