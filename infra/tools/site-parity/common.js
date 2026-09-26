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
// 2026-09-24: was a hardcoded constant per script (777/12345/4242) -- with
// a 240-card pool but only ~30 sampled cards per run, a fixed seed means
// the SAME cards get tested forever no matter how many days site-watch.yml
// runs. Rotates by UTC date instead (still reproducible within a day for
// debugging a specific failure, `salt` keeps the 3 test scripts on
// different streams from each other on the same day).
function defaultSeed(salt) {
  const day = new Date().toISOString().slice(0, 10).replace(/-/g, "");
  let h = Number(day) + salt * 2654435761;
  return h >>> 0;
}
// 2026-09-24: expanded from a hand-picked 30-card subset to the site's full
// card catalog (240, extracted from PRE_SEPTEMBER18_SNAPSHOT in
// site-oracle/site-bundle-20260919.js's `effect` fields) -- the 30-card
// pool meant ~194 of the site's cards were NEVER exercised by any parity
// test, no matter how many samples/seeds ran. Found via a scarecrow bug
// that only got caught because it happened to be in the old 30. A few
// entries here are special-mode rule cards (chess960, diagonalChess, etc.)
// our engine may not implement -- harmless, generateActions/applyAction
// just produces nothing for an effect neither side recognizes.
const POOL = ["acceleration","alekhineMachineGun","amazon","apprenticeKnights","armistice","assassin","babyBear","backwardKnight","barricade","basicTraining","bigRook","binaMate","blackBox","blackHole","blueJeans","breakthroughOrder","callingCard","camouflageRule","canceling","freeCastling","chain","chameleonMutation","charge","checker","chess344200","chess960","chimera","cleanupSacrifice","clonePassive","collapse","conscription","constitutionalMonarchy","conversion","conveyorRule","coolGuy","cornerKick","coronation","crownRule","deathSquad","democracy","desperado","diagonalChess","dice","disarm","dragon","dutch","eagle","earlyPromotion","easternPolicy","elephantEscape","emergencyEvacuation","enPassantBang","encouragement","evasion","exhaustion","exile","fanaticalRitual","fastGrowth","feudalContract","fianchetto","fileSurge","finalWeapon","football","freeze","frenzy","gale","ghost","grasshopper","guard","hallucination","herald","highGround","highway","holdout","homecoming","hook","horde","horseRiding","icbm","iceSheet","idol","imperialStudies","inertia","initiative","injury","insight","ironMonarch","jester","joker","judgment","knightmaster","knightmate","lastResistance","lastStand","pawnLeap","lobster","localConscription","log","londonSystem","machoChess","madHorse","martyrdom","merchantGuild","missionary","mistake","mistakeCard","mongolianGambit","monochromeChess","monsterRule","moving","necromancy","ordination","othello","otherworld","overwhelm","palace","panic","pawnConversion","pawnStorm","pegasus","periodicCollapse","poisonedPawn","portal","portalGun","freeMove","promotionRush","prophecy","quantumMechanics","queenAfterimage","queenCavalry","queensGambit","qxe1","racingKing","radicalCharge","randomRoulette","reaper","recycling","reformation","religiousVictory","reposition","retreat","revelation","reversePawns","rookLift","royalCommand","royalShield","ruleBombs","ruleTicket","sacrifice","saturation","scarecrow","schrodingerPawns","severance","bishopSnipe","socialism","pawnSprint","spy","stake","standardBearer","stealth","submerge","substitution","suicideBomber","summonColossus","switcheroo","taunt","traitor","transcendence","trojanHorse","trolley","twins","ultimatum","undergroundBunker","underpromotion","vanish","vip","vortex","whiteBox","windmill","winterKingdom","witchTrial","wizard","zugzwang","kingOfTheHill","loyalist","blackMagic","parry","fleetingDream","emptyLunchbox","genevaConvention","platformRule","cleanupPieces","relay","fieldPromotion","frontlineResponse","hypocrisy","siegeRam","suspiciousPotion","berserker","magicGirl","replayMove","slime","siren","trickster","gomoku","undead","majesty","infiltration","killerKing","campfire","outpost","hedgehog","nullification","recurrence","princess","resolve","vanguard","reversal","miracle","bigBishop","overtake","captureTheFlag","highlander","thief","disassembly","falseStart","proficiency","locustSwarm","longEnPassant","extinction","symmetry","brutus","clockwork","mutation","parrot","paladin","octopus","metal","chessNPow30"];
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
module.exports = { real, parseArgs, rngMaker, defaultSeed, POOL, makeState, strip, norm, makeDecks };
