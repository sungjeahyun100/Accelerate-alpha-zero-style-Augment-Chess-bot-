// Mixes several self-play datasets into one training file plus one shared
// validation file, sampling WHOLE GAMES so a game never straddles train/val.
//
//   node nnue/mix-datasets.js --out mix.jsonl --val-out val.jsonl [--seed 1] \
//        r3=/path/round3.jsonl:1.0:0.10  r2=/path/round2.jsonl:0.3:0.10  r1=/path/round1.jsonl:0.15:0
//
// Each source is  name=file:trainFraction:valTailFraction
//   valTailFraction  the LAST fraction of that source's games goes to the
//                    validation file (never to training) -- the same "last 10% of
//                    games" definition train.js uses, so a source's val part is
//                    stable when only its earlier games change
//   trainFraction    each remaining game is kept with this probability (1 = all)
//                    -- the way to down-weight an older/noisier round
// Games are found the way train.js does it (a new game starts when the piece
// count returns to 32 after a capture). Lines are copied unchanged.
const fs = require("fs");

const args = process.argv.slice(2);
let out = null, valOut = null, seed = 1;
const sources = [];
for (let i = 0; i < args.length; i += 1) {
  if (args[i] === "--out") out = args[++i];
  else if (args[i] === "--val-out") valOut = args[++i];
  else if (args[i] === "--seed") seed = Number(args[++i]) || 1;
  else sources.push(args[i]);
}
if (!out || !sources.length) {
  console.error("Usage: node mix-datasets.js --out mix.jsonl [--val-out val.jsonl] [--seed N] name=file:trainFrac:valTailFrac ...");
  process.exit(1);
}

function countPieces(board) {
  let n = 0;
  for (const row of board) for (const p of row) if (p) n += 1;
  return n;
}
function splitGames(lines) {
  const games = [];
  let cur = [];
  let sawCapture = false;
  lines.forEach((line, i) => {
    const count = countPieces(JSON.parse(line).board);
    if (i > 0 && count === 32 && sawCapture) {
      games.push(cur);
      cur = [];
      sawCapture = false;
    }
    cur.push(line);
    if (count < 32) sawCapture = true;
  });
  if (cur.length) games.push(cur);
  return games;
}
function rngFactory(s) {
  let x = s >>> 0;
  return () => ((x = (x * 1664525 + 1013904223) >>> 0) / 4294967296);
}

const rng = rngFactory(seed);
const trainLines = [];
const valLines = [];
const summary = [];
for (const spec of sources) {
  const eq = spec.indexOf("=");
  const name = spec.slice(0, eq);
  // split from the right: the file path itself may contain ":" (Windows drive letters)
  const parts = spec.slice(eq + 1).split(":");
  const valFrac = parts.length >= 3 ? Number(parts.pop()) : 0;
  const trainFrac = parts.length >= 2 ? Number(parts.pop()) : 1;
  const file = parts.join(":");
  const lines = fs.readFileSync(file, "utf8").split("\n").filter(Boolean);
  const games = splitGames(lines);
  const valCount = Math.floor(games.length * valFrac);
  const trainGames = games.slice(0, games.length - valCount);
  const valGames = games.slice(games.length - valCount);
  let kept = 0, keptPositions = 0;
  for (const g of trainGames) {
    if (rng() < trainFrac) {
      kept += 1;
      keptPositions += g.length;
      for (const l of g) trainLines.push(l);
    }
  }
  let valPositions = 0;
  for (const g of valGames) {
    valPositions += g.length;
    for (const l of g) valLines.push(l);
  }
  summary.push(`${name}: ${lines.length} positions / ${games.length} games -> train ${keptPositions} (${kept} games), val ${valPositions} (${valGames.length} games)`);
}
fs.writeFileSync(out, trainLines.join("\n") + "\n");
if (valOut) fs.writeFileSync(valOut, valLines.join("\n") + "\n");
console.log(summary.join("\n"));
console.log(`total: train ${trainLines.length} positions -> ${out}` + (valOut ? `, val ${valLines.length} positions -> ${valOut}` : ""));
