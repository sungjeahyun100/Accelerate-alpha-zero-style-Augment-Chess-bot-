// Shared harness for the parity tests.
globalThis.self = globalThis; globalThis.addEventListener = () => {};
const fs = require("fs"), path = require("path");
const REAL_PATH = path.join(__dirname, ".cache", "real-worker.js");
if (!fs.existsSync(REAL_PATH)) { console.error("Missing " + REAL_PATH + " -- run: node fetch-real-worker.js"); process.exit(1); }
const real = require(REAL_PATH);
const ENGINE_DEFAULT = path.resolve(__dirname, "..", "..", "engine-merged.js");
function parseArgs() { // [engine|-] [N] [seed] [plies]
  const a = process.argv.slice(2);
  return { engine: a[0] && a[0] !== "-" ? path.resolve(a[0]) : ENGINE_DEFAULT, N: Number(a[1] || 0), seed: Number(a[2] || 0), plies: Number(a[3] || 40) };
}
const rngMaker = (seed) => { let x = seed >>> 0; return () => ((x = (x * 1664525 + 1013904223) >>> 0) / 4294967296); };
const POOL = ["campfire","reversal","scarecrow","othello","gale","freeze","witchTrial","locustSwarm","grasshopper","thief","parrot","paladin","metal","brutus","clockwork","octopus","pawnStorm","platformRule","desperado","royalShield","charge","disarm","socialism","cleanupPieces","substitution","promotionRush","poisonStun","portalGun","severance","inertia"];
function makeState(engine, board, decks, color) {
  const st = engine.cloneState({});
  st.board = JSON.parse(JSON.stringify(board)); st.mode = "play"; st.turn = color; st.actionsRemaining = 1;
  st.deckSlots = JSON.parse(JSON.stringify(decks)); st.captures = { white: [], black: [] }; st.aiSearchNoCards = false;
  // The site's real game state always has this field from the first move (main bundle:
  // `parrotMovement: { white: null, black: null }` in the initial state). Without it the
  // site's rememberLocalMovement never records, which real games never see.
  st.parrotMovement = { white: null, black: null };
  if (st.board.some((row) => row.some((p) => p?.type === "campfire"))) st.hasCampfire = true;
  engine.setWorkerBoardDimensions(st); return st;
}
function strip(o) { if (o && typeof o === "object") { delete o.id; delete o.instanceId; delete o.pieceId; for (const k in o) strip(o[k]); } return o; }
const norm = (a) => JSON.stringify(strip(JSON.parse(JSON.stringify(a)))); // action normalised: random ids removed
function makeDecks(rng, per) {
  const decks = { white: [], black: [] };
  for (const col of ["white", "black"]) { const used = new Set(); for (let k = 0; k < per; k++) { const e = POOL[Math.floor(rng() * POOL.length)]; if (used.has(e)) continue; used.add(e); decks[col].push({ id: e, instanceId: `${col}-${e}-${k}`, effect: e, stars: 2, used: false, recovering: false }); } }
  return decks;
}
module.exports = { real, parseArgs, rngMaker, POOL, makeState, strip, norm, makeDecks };
