const fs = require("fs");
const path = require("path");
const os = require("os");
const crypto = require("crypto");
const { Worker } = require("worker_threads");
const tf = require("@tensorflow/tfjs");
require("@tensorflow/tfjs-backend-wasm");
const { encodeBoard, INPUT_SIZE, FEATURE_NAMES } = require("./encode.js");

const DATA_FILE = path.join(__dirname, "..", "selfplay-data.jsonl");
const MODEL_DIR = "file://" + path.join(__dirname, "model");

// evaluateState()'s own hand-picked coefficients (engine.js), in the exact
// same order as encode.js's FEATURE_NAMES -- copied from tune-eval.js's
// ORIGINAL_WEIGHTS, which measured these at 69.1% sign-agreement accuracy
// on this project's validation split (see buildModel()'s wide-layer warm
// start below for why this lives here). Keep this in sync with
// tune-eval.js and engine.js's evaluateState() by hand if either changes --
// there's no shared source between them.
const ORIGINAL_EVAL_WEIGHTS = [
  1.35, 1,
  1, -0.88,
  1, -0.72,
  1, -0.65,
  1, -0.82,
  1, -0.82,
  1, -0.92,
  1, -0.9,
  1,
  1, -1,
  1, 1
];

function countPieces(board) {
  let count = 0;
  for (const row of board) for (const p of row) if (p) count += 1;
  return count;
}

// Blend the outcome label with the recorded search score (2026-09-07,
// tried after plain-outcome labels kept training a model that couldn't
// beat evaluateState()): a whole game's outcome is a noisy signal for any
// ONE position in it, especially from self-play this shallow
// (completedDepth averaging under 1) -- a mediocre move in a game that
// still gets won looks identical to a great move under pure outcome
// labeling. searchScore is a per-move reading, so mixing in more of it
// gives the label more position-specific signal instead of relying mostly
// on the game-wide result. This project deliberately avoided training on
// search score at all before, over concern about permanently baking in
// this (weak) engine's own biases; that concern applies much more to an
// iterative self-play loop (train -> generate -> train again) than to the
// one-shot training done here, so a heavier blend for variance reduction
// is a smaller risk than it would be in that setting.
// Tried raising 0.15 -> 0.4 (2026-09-09), reasoning that a whole-game
// outcome can be flipped by one unrelated blunder many moves after the
// position being labeled, so leaning more on the per-position searchScore
// should reduce label noise. Tested in isolation (ABLATE_BLEND=0.4,
// blunder-weighting off) on the scaled-warm-start setup: 64.6% vs 65.2%
// for 0.15 -- WORSE, not better, so reverted to 0.15 as the default rather
// than keep an untested-as-actually-helping change. Left as a real
// hypothesis for someone to revisit with a cleaner test (this run also had
// the scaled-warm-start wide path in it, so 0.4's effect wasn't tested
// against the plain baseline), not a dead end.
// searchScore is null on exploration plies (no search ran) -- outcome only
// for those. Otherwise squashed the same way evaluateState() was squashed
// for the hand-coded comparison (tanh(score / 400)), which also gracefully
// handles the engine's large sentinel scores (opening-book/forced-move
// INF fractions, reaper-execution ±1e5) by saturating them to ~±1, same as
// a real decisive outcome would.
// Overridable via env for controlled A/B/ablation runs (2026-09-09) --
// ABLATE_BLEND/ABLATE_BLUNDER let a driver script isolate each change's
// individual effect without hand-editing this file between runs.
const SEARCH_SCORE_BLEND_WEIGHT = process.env.ABLATE_BLEND !== undefined ? Number(process.env.ABLATE_BLEND) : 0.15;
const SEARCH_SCORE_SCALE = 400;
// BLEND_BY_DEPTH=1 (2026-09-19): trust the search score more where the search
// actually went deeper. Rounds 1-2 were ~90% depth-1 (the "search score" there is
// barely more than the hand-coded evaluation), round 3 reaches depth 2-6, so a
// single global weight either wastes the deep scores or over-trusts the shallow
// ones. BLEND_DEPTH_MAP = weights for completedDepth 1,2,3,>=4 (default
// 0.25,0.45,0.65,0.8); ABLATE_BLEND is ignored while this is on.
const BLEND_BY_DEPTH = process.env.BLEND_BY_DEPTH === "1";
const BLEND_DEPTH_MAP = (process.env.BLEND_DEPTH_MAP || "0.25,0.45,0.65,0.8").split(",").map(Number);
function searchBlendWeight(entry) {
  if (!BLEND_BY_DEPTH) return SEARCH_SCORE_BLEND_WEIGHT;
  const d = Math.max(1, Math.floor(Number(entry.completedDepth) || 1));
  return BLEND_DEPTH_MAP[Math.min(BLEND_DEPTH_MAP.length - 1, d - 1)];
}
// RESIDUAL=1 (2026-09-19): train the net on what the hand-coded evaluator gets
// WRONG -- target tanh((searchScore - evalBefore) / RESID_SCALE), evalBefore being
// the hand-coded static score logged at self-play time. The engine then plays
// with handcoded + K * output (match spec @hybrid<K>, K ~ RESID_SCALE), so
// material/tactics stay with the hand-coded part. Positions without a search
// score (exploration plies) get target 0 ("no correction known").
const RESIDUAL = process.env.RESIDUAL === "1";
const RESID_SCALE = Number(process.env.RESID_SCALE || 300);
function blendedLabel(entry) {
  if (RESIDUAL) {
    if (entry.searchScore == null || entry.evalBefore == null) return 0;
    return Math.tanh((entry.searchScore - entry.evalBefore) / RESID_SCALE);
  }
  if (entry.searchScore == null) return entry.outcome;
  const searchSignal = Math.tanh(entry.searchScore / SEARCH_SCORE_SCALE);
  const w = searchBlendWeight(entry);
  return (1 - w) * entry.outcome + w * searchSignal;
}

// Geometric decay applied to a position's sample weight based on how many
// plies happened after it before the game actually ended (pliesFromEnd).
// Surviving the tainted/unfinished filters above is not all-or-nothing
// trustworthiness -- a position right before the result is far more tightly
// linked to it than one 40 plies earlier, where ordinary (non-exploration)
// but still search-quality-limited play could easily have changed things.
// Older data predates `pliesFromEnd` (undefined -> weight 1, i.e. untouched).
const PLIES_FROM_END_DECAY = 0.98;
function sampleWeightFor(pliesFromEnd) {
  return pliesFromEnd == null ? 1 : Math.pow(PLIES_FROM_END_DECAY, pliesFromEnd);
}

// Positions aren't tagged with a game id, so reconstruct game boundaries
// from the board sequence itself: a game's first position always has all 32
// pieces (special-piece substitution swaps types, never counts), and within
// one game the count only ever goes DOWN (captures) until a new game resets
// it back to 32. Plain "count === 32" can't be used as the boundary signal
// by itself -- most games open with several capture-free plies, so many
// CONSECUTIVE positions within a single game legitimately show 32 -- so
// only count a return to 32 as a new game once the count has actually
// dropped below 32 at some point since the last boundary.
function assignGameIds(pieceCounts) {
  const gameIds = new Array(pieceCounts.length);
  let gameId = 0;
  let sawCaptureThisGame = false;
  pieceCounts.forEach((count, i) => {
    if (i > 0 && count === 32 && sawCaptureThisGame) {
      gameId += 1;
      sawCaptureThisGame = false;
    }
    gameIds[i] = gameId;
    if (count < 32) sawCaptureThisGame = true;
  });
  return gameIds;
}

// encodeBoard() now calls evaluateStateComponents() per position (added
// for the 21-feature injection) -- real per-position work that, run
// serially on the main thread for tens of thousands of positions, became
// the dominant cost of a retrain (confirmed 2026-09-09: a run sat with no
// "positions loaded" log for minutes). Two independent speedups, since this
// project reruns training on the SAME data file repeatedly while iterating
// on hyperparameters/architecture:
//  1. Split the encoding work across worker_threads (encode-worker.js),
//     same idea as selfplay-worker.js's pool.
//  2. Cache the encoded result to disk keyed on the data file's own
//     CONTENT (a hash, not its path) plus encode.js's own mtime (so an
//     architecture/feature change invalidates stale caches instead of
//     silently loading mismatched data). A second run over the same
//     content -- including two DIFFERENT runs, since snapshotDataFile()
//     below always copies the live selfplay-data.jsonl into a new
//     uniquely-timestamped file, so the path is never the same twice even
//     when the content is -- then skips straight to training. (First
//     version of this keyed on the snapshot's own filename instead of its
//     content, which defeated the whole point: every run got its own
//     never-reused snapshot name, so it was cache-missing every single
//     time -- caught 2026-09-09 when a second run visibly re-encoded from
//     scratch instead of reusing the first run's cache.)
const ENCODE_CACHE_DIR = path.join(__dirname, "cache");

function encodeCacheKey(dataFile) {
  const encodeMtime = fs.statSync(path.join(__dirname, "encode.js")).mtimeMs;
  const contentHash = crypto.createHash("sha1").update(fs.readFileSync(dataFile)).digest("hex").slice(0, 16);
  // ENCODE_CACHE_SALT (2026-09-19): the encoded features come from
  // engine-merged.js's evaluateStateComponents, but this key only covered the
  // data file + encode.js mtime -- so after an engine rules/balance change an
  // old cache would silently be reused with stale features. The cloud train
  // workflow sets the salt to a hash of engine-merged.js + encode.js; unset
  // (default) keeps the legacy key so existing caches and local runs behave
  // exactly as before.
  const salt = process.env.ENCODE_CACHE_SALT ? "-" + process.env.ENCODE_CACHE_SALT : "";
  return contentHash + "." + Math.round(encodeMtime) + salt;
}

async function encodeInParallel(entries) {
  // Reserving one core (os.cpus().length - 1) makes sense on a local dev
  // machine so it stays responsive for the person using it -- but on a
  // headless CI runner there's no one to keep responsive, so that reserved
  // core is pure waste. Confirmed live 2026-09-17: GitHub's standard
  // 2-core runner computed workerCount=1, meaning the cloud encode job (the
  // whole reason it moved off the owner's machine) ran with ZERO actual
  // parallelism. ENCODE_WORKER_COUNT lets the cloud workflow request the
  // full core count explicitly without changing local behavior.
  const workerCount = process.env.ENCODE_WORKER_COUNT
    ? Math.max(1, Number(process.env.ENCODE_WORKER_COUNT))
    : Math.max(1, os.cpus().length - 1);
  const chunkSize = Math.ceil(entries.length / workerCount) || 1;
  const chunks = [];
  for (let i = 0; i < entries.length; i += chunkSize) chunks.push(entries.slice(i, i + chunkSize));
  console.log("encoding", entries.length, "positions across", chunks.length, "worker(s)...");

  function runChunk(chunk) {
    return new Promise((resolve, reject) => {
      const worker = new Worker(path.join(__dirname, "encode-worker.js"), {
        workerData: { boards: chunk.map((e) => ({ board: e.board, turn: e.turn, deckSlots: e.deckSlots })) }
      });
      worker.on("message", (msg) => { worker.terminate(); resolve(msg); });
      worker.on("error", (err) => { worker.terminate(); reject(err); });
    });
  }

  const chunkResults = await Promise.all(chunks.map(runChunk));
  let survivorCount = 0;
  for (const { nullFlags } of chunkResults) {
    for (const flag of nullFlags) if (!flag) survivorCount += 1;
  }
  const flat = new Float32Array(survivorCount * INPUT_SIZE);
  const survivorMask = new Uint8Array(entries.length);
  let writeIdx = 0;
  let entryIdx = 0;
  for (const { out, nullFlags } of chunkResults) {
    for (let i = 0; i < nullFlags.length; i++, entryIdx++) {
      if (nullFlags[i]) continue;
      survivorMask[entryIdx] = 1;
      flat.set(out.subarray(i * INPUT_SIZE, (i + 1) * INPUT_SIZE), writeIdx * INPUT_SIZE);
      writeIdx += 1;
    }
  }
  const skippedTerminal = entries.length - survivorCount;
  return { flat, survivorMask, skippedTerminal };
}

// fs.writeFileSync hands the whole buffer to a single write() syscall, which
// has a hard length cap of 2^31-1 bytes (RangeError ERR_OUT_OF_RANGE) --
// confirmed live 2026-09-17 in the cloud encode job on the round-1 dataset
// (192,984 positions, INPUT_SIZE 5509 -> a 4.2GB flat buffer, comfortably
// past the 2.1GB cap even though the 7GB runner had plenty of RAM for the
// buffer itself). Chunking the write in <2GB pieces sidesteps the syscall
// limit regardless of how large the encoded feature set grows.
function writeFileChunked(filePath, buffer, chunkSize = 1 << 30) {
  const fd = fs.openSync(filePath, "w");
  try {
    let offset = 0;
    while (offset < buffer.length) {
      const end = Math.min(offset + chunkSize, buffer.length);
      fs.writeSync(fd, buffer, offset, end - offset);
      offset = end;
    }
  } finally {
    fs.closeSync(fd);
  }
}

// Mirror of writeFileChunked: fs.readFileSync/readSync also cap out around
// 2GiB (ERR_FS_FILE_TOO_LARGE), hit live 2026-09-17 loading back the round-1
// dataset's 4.2GB cache file. Pre-allocate the full buffer (its size is
// known up front, unlike a streaming append) and fill it via repeated
// readSync calls, each under the 2GB-per-call cap.
function readFileChunked(filePath, chunkSize = 1 << 30) {
  const size = fs.statSync(filePath).size;
  const buffer = Buffer.allocUnsafe(size);
  const fd = fs.openSync(filePath, "r");
  try {
    let offset = 0;
    while (offset < size) {
      const end = Math.min(offset + chunkSize, size);
      fs.readSync(fd, buffer, offset, end - offset, offset);
      offset = end;
    }
  } finally {
    fs.closeSync(fd);
  }
  return buffer;
}

async function getEncodedInputs(dataFile, kept) {
  fs.mkdirSync(ENCODE_CACHE_DIR, { recursive: true });
  const key = encodeCacheKey(dataFile);
  const binPath = path.join(ENCODE_CACHE_DIR, key + ".bin");
  const metaPath = path.join(ENCODE_CACHE_DIR, key + ".meta.json");
  if (fs.existsSync(binPath) && fs.existsSync(metaPath)) {
    const meta = JSON.parse(fs.readFileSync(metaPath, "utf8"));
    if (meta.keptCount === kept.length) {
      const buf = readFileChunked(binPath);
      const flat = new Float32Array(buf.buffer, buf.byteOffset, buf.byteLength / 4);
      console.log("loaded cached encoded features (" + (flat.length / INPUT_SIZE) + " rows) -- skipping re-encode");
      return { flat, survivorMask: Uint8Array.from(meta.survivorMask), skippedTerminal: meta.skippedTerminal };
    }
    console.log("encode cache found but kept-count mismatch (" + meta.keptCount + " vs " + kept.length + ") -- re-encoding");
  }
  const result = await encodeInParallel(kept);
  writeFileChunked(binPath, Buffer.from(result.flat.buffer, result.flat.byteOffset, result.flat.byteLength));
  fs.writeFileSync(metaPath, JSON.stringify({
    keptCount: kept.length,
    survivorMask: Array.from(result.survivorMask),
    skippedTerminal: result.skippedTerminal
  }));
  return result;
}

async function loadData(dataFile = DATA_FILE) {
  const lines = fs.readFileSync(dataFile, "utf8").split("\n").filter(Boolean);
  // Captured here instead of re-reading the file later (see the sanity
  // check below) -- a concurrent self-play run (or, once, an accidental
  // manual delete of the snapshot) can make that file gone by the time
  // training finishes, crashing the run after the expensive part is
  // already done and the weights are already saved. This has no such
  // problem since it's just a reference into what's already in memory.
  const firstBoard = lines.length ? JSON.parse(lines[0]).board : null;
  // Game boundaries MUST be computed over every ply in original order,
  // tainted/unfinished included -- assignGameIds detects a new game by
  // piece count resetting to 32, which only works walking an unbroken
  // sequence. Computing it AFTER dropping tainted/unfinished plies (an
  // earlier version of this did, for the blunder-down-weighting below)
  // breaks that detection: with chunks of plies missing, unrelated games
  // can get spliced into one, and once that happens a game with 80+ real
  // plies in it will contain SOME blunder-sized swing pretty much always --
  // confirmed live 2026-09-10, 1191 of 1254 "games" got flagged, which is
  // the earlier per-game-percentile analysis's ~10% expectation blown way
  // past. allEntries/allPieceCounts/allGameIds below stay aligned with
  // `lines` one-to-one; `kept` carries each surviving entry's REAL gameId
  // alongside it instead of ever recomputing boundaries on a filtered
  // subsequence.
  const allEntries = lines.map((line) => JSON.parse(line));
  const allPieceCounts = allEntries.map((entry) => countPieces(entry.board));
  const allGameIds = assignGameIds(allPieceCounts);
  const kept = [];
  let skippedTainted = 0;
  let skippedUnfinished = 0;
  allEntries.forEach((entry, i) => {
    // Skip positions whose outcome label is unreliable because a random
    // exploration move happened somewhere between this position and the
    // game's end (see selfplay-worker.js's explorationTainted comment).
    // Older data files predate this field and won't have it -- treated as
    // untainted rather than silently dropped, so this stays a no-op on them.
    if (entry.explorationTainted) {
      skippedTainted += 1;
      return;
    }
    // Skip positions from games that hit the ply cap without a real
    // conclusion. Those were being labeled 0 the same as an actual draw,
    // but "ran out of plies" tells you nothing about who was winning --
    // unlike a real draw (repetition/50-move), which IS a meaningful,
    // trustworthy outcome. Older data predates the `unfinished` field and
    // won't have it -- treated as finished (no-op) on those.
    if (entry.unfinished) {
      skippedUnfinished += 1;
      return;
    }
    kept.push({ entry, sourceGameId: allGameIds[i], pieceCount: allPieceCounts[i] });
  });
  if (skippedTainted) console.log("skipped exploration-tainted positions:", skippedTainted);
  if (skippedUnfinished) console.log("skipped unfinished-game positions:", skippedUnfinished);

  // Blunder-game down-weighting -- DISABLED BY DEFAULT (2026-09-10) after
  // three failed calibration attempts in one session, each catching the
  // last one's mistake and introducing a new one:
  //   1. Threshold 5000 came from the PER-MOVE delta distribution's p95,
  //      but a game has dozens of moves, so "some move exceeds the
  //      per-move p95" is ~1-0.95^(moves/game), a >90% per-GAME event, not
  //      5% -- 1216/1254 games got flagged.
  //   2. Re-sampled the PER-GAME MAXIMUM instead (p90 9101) and raised the
  //      threshold to 10000. Still 1191/1254 flagged, because the gameId
  //      grouping for the down-weighting was (wrongly) recomputed on the
  //      tainted/unfinished-FILTERED sequence, which splices unrelated
  //      games together (fixed as its own bug, see kept/sourceGameId
  //      above) -- but the threshold itself was never actually validated
  //      end to end before this was found.
  //   3. With the gameId bug fixed, still 1899/1906 flagged. Root cause:
  //      evalDelta on a game's OWN FINAL move (checkmate/king-capture) is
  //      a mate-scale sentinel (confirmed live: values ~5e8), which
  //      trivially clears any tactics-scale threshold -- that's not a
  //      blunder, it's every decisive game ending normally. Plus this
  //      engine's big pieces (amazon/merchant/hook, values 13-20 in
  //      encode.js's PIECE_VALUE) apparently make even mid-game
  //      routine captures swing evaluateState()'s raw units into the
  //      same ballpark as whatever "blunder-sized" was supposed to mean,
  //      so 10000 wasn't a real tactics/blunder boundary in this engine's
  //      scale to begin with.
  // Leaving this OFF (weight 1 = no-op) rather than attempting a 4th
  // threshold guess -- would need excluding each game's own final ply
  // and a properly-derived (not guessed) mid-game threshold before this
  // is worth trying again. ABLATE_BLUNDER env var still works for anyone
  // deliberately re-testing it.
  const BLUNDER_EVAL_DELTA_THRESHOLD = 10000;
  const BLUNDER_GAME_WEIGHT = process.env.ABLATE_BLUNDER !== undefined ? Number(process.env.ABLATE_BLUNDER) : 1;
  const totalSourceGames = allGameIds.length ? allGameIds[allGameIds.length - 1] + 1 : 0;
  const gameHasBlunder = new Array(totalSourceGames).fill(false);
  allEntries.forEach((entry, i) => {
    if (entry.evalDelta != null && Math.abs(entry.evalDelta) >= BLUNDER_EVAL_DELTA_THRESHOLD) {
      gameHasBlunder[allGameIds[i]] = true;
    }
  });
  let blunderGames = 0;
  for (const has of gameHasBlunder) if (has) blunderGames += 1;
  if (blunderGames) console.log("down-weighting", blunderGames, "of", totalSourceGames, "games containing a big eval swing (>=", BLUNDER_EVAL_DELTA_THRESHOLD, ")");

  const { flat, survivorMask, skippedTerminal } = await getEncodedInputs(dataFile, kept.map((k) => k.entry));
  if (skippedTerminal) console.log("skipped terminal positions:", skippedTerminal);

  const inputs = [];
  const labels = [];
  const trainingLabels = [];
  const pieceCounts = [];
  const sampleWeights = [];
  let row = 0;
  kept.forEach((k, i) => {
    if (!survivorMask[i]) return;
    inputs.push(flat.subarray(row * INPUT_SIZE, (row + 1) * INPUT_SIZE));
    row += 1;
    const entry = k.entry;
    // `labels` stays the pure, clean outcome (-1/0/1) -- used for
    // validation/accuracy measurement, which needs to compare against the
    // real ground truth, not a blended training target. The blended
    // version used for actual training lives in `trainingLabels` instead.
    labels.push(entry.outcome);
    trainingLabels.push(blendedLabel(entry));
    pieceCounts.push(k.pieceCount);
    const blunderMultiplier = gameHasBlunder[k.sourceGameId] ? BLUNDER_GAME_WEIGHT : 1;
    sampleWeights.push(sampleWeightFor(entry.pliesFromEnd) * blunderMultiplier);
  });
  const gameIds = assignGameIds(pieceCounts);
  console.log("reconstructed games:", gameIds.length ? gameIds[gameIds.length - 1] + 1 : 0);
  return { inputs, labels, trainingLabels, pieceCounts, sampleWeights, firstBoard, gameIds };
}

// Shrunk from 32/32 and given L2 weight decay -- the previous run (51k
// positions, 2432-dim input, no regularization) still overfit badly (val_loss
// climbed monotonically from epoch 0), which pointed at model capacity/lack
// of regularization rather than data volume. A smaller, penalized network is
// the fix to try first before throwing more data at it.
// Learning rate lowered 0.001->0.0002 and dropout(0.2) added after each
// hidden layer (2026-09-07): best epoch has been ~0 in every run so far --
// with so few gradient steps landing before it starts memorizing, a big
// step size means most of the "damage" happens within the very first
// epoch. A smaller step size spreads the same amount of learning over more
// epochs, and dropout directly fights fast memorization by preventing the
// network from leaning on any single neuron/feature too early. Goal here
// is specifically to see best-epoch move past 0-1, not (necessarily) a
// better final accuracy by itself.
// NNUE_VARIANT=old reproduces the exact pre-2026-09-07 settings (no
// dropout, lr 0.001, epochs 15, patience 6) so it can be run back-to-back
// with the new settings for a same-day A/B. NNUE_VARIANT=linear replaces
// the whole 2-hidden-layer network with a single linear layer straight from
// input to output (2026-09-07) -- a baseline sanity check: if a plain
// linear model scores about the same as the fancier network, that's a
// strong signal the extra hidden layers aren't earning their keep at this
// data size, and effort should go toward data/labels rather than
// architecture. See main()'s use of EPOCHS_CAP/PATIENCE for the other half
// of the old/new toggle (linear reuses the "old" schedule).
const VARIANT = process.env.NNUE_VARIANT === "old" ? "old" : process.env.NNUE_VARIANT === "linear" ? "linear" : "new";
const L2 = 0.001;
const DROPOUT_RATE = VARIANT === "new" ? 0.2 : 0;
const LEARNING_RATE = VARIANT === "new" ? 0.0002 : 0.001;
const EPOCHS_CAP = VARIANT === "new" ? 40 : 15;
const PATIENCE = VARIANT === "new" ? 12 : 6;
console.log("NNUE_VARIANT:", VARIANT, `(lr=${LEARNING_RATE}, dropout=${DROPOUT_RATE}, epochs=${EPOCHS_CAP}, patience=${PATIENCE})`);
function buildModel() {
  if (VARIANT === "linear") {
    const linearModel = tf.sequential();
    linearModel.add(tf.layers.dense({
      units: 1,
      activation: "tanh",
      inputShape: [INPUT_SIZE],
      kernelRegularizer: tf.regularizers.l2({ l2: L2 })
    }));
    linearModel.compile({ optimizer: tf.train.adam(LEARNING_RATE), loss: "meanSquaredError" });
    return linearModel;
  }
  // "Wide & deep" (2026-09-08): switched from a plain Sequential stack to
  // the Functional API to add a direct linear path from the raw input
  // straight to the output, running alongside the existing 2-hidden-layer
  // path, summed before the final tanh. Motivation: nnue/tune-eval.js
  // measured evaluateState()'s own ORIGINAL hand-picked weights (not a
  // fitted regression -- a data-fitted linear regression over the same 21
  // features actually did WORSE, 61.5%, than the untouched hand-picked
  // ones, 69.1%, on this exact data/split) at 69.1%, while this deep-only
  // network was stuck at 55.8-64.6% -- meaning the network wasn't even
  // matching what the existing hand-picked linear combination already gets
  // for free. The wide path gives it that same shape (a direct linear map
  // from input to output, same "big sparse linear layer + one-hot input"
  // idea as the first layer in real NNUE designs), so the deep path only
  // has to learn a *correction* on top instead of having to rediscover a
  // working linear baseline from scratch through two ReLU layers.
  //
  // Wide&deep alone only closed part of the gap (65.6% -- see HANDOFF/
  // memory), still well under 69.1%, with best-epoch stuck at ~1: not
  // enough gradient steps land on the wide path's random initialization
  // before early stopping kicks in for it to rediscover a 21-number linear
  // fit on its own. Warm-starting it (2026-09-09) skips that rediscovery
  // entirely -- the wide kernel's 21 feature-input rows start AT
  // evaluateState()'s own coefficients (already proven to reach 69.1% by
  // itself) and the one-hot-board rows start at 0, so training begins from
  // "at least as good as the hand-coded eval" instead of from scratch, and
  // only has to find genuine improvements on top rather than also having
  // to re-derive the baseline within a handful of epochs.
  // ABLATE_DEEP_UNITS (added 2026-09-12): the deep path's hidden width has
  // been 16 since before the card one-hot features existed -- worth
  // re-testing with more capacity now that INPUT_SIZE grew (4245->4281) and
  // there's a real per-card signal for it to actually use.
  const DEEP_UNITS = process.env.ABLATE_DEEP_UNITS !== undefined ? Number(process.env.ABLATE_DEEP_UNITS) : 16;
  const input = tf.input({ shape: [INPUT_SIZE] });
  let deep = tf.layers.dense({
    units: DEEP_UNITS,
    activation: "relu",
    kernelRegularizer: tf.regularizers.l2({ l2: L2 })
  }).apply(input);
  if (DROPOUT_RATE > 0) deep = tf.layers.dropout({ rate: DROPOUT_RATE }).apply(deep);
  deep = tf.layers.dense({
    units: DEEP_UNITS,
    activation: "relu",
    kernelRegularizer: tf.regularizers.l2({ l2: L2 })
  }).apply(deep);
  if (DROPOUT_RATE > 0) deep = tf.layers.dropout({ rate: DROPOUT_RATE }).apply(deep);
  const deepOut = tf.layers.dense({ units: 1, useBias: false }).apply(deep);
  const wideLayer = tf.layers.dense({
    units: 1,
    useBias: false,
    kernelRegularizer: tf.regularizers.l2({ l2: L2 })
  });
  const wideOut = wideLayer.apply(input);
  const merged = tf.layers.add().apply([deepOut, wideOut]);
  const output = tf.layers.activation({ activation: "tanh" }).apply(merged);
  const model = tf.model({ inputs: input, outputs: output });
  model.compile({ optimizer: tf.train.adam(LEARNING_RATE), loss: "meanSquaredError" });

  // Warm start: overwrite the wide layer's freshly (randomly) initialized
  // kernel so its 21 feature-input rows start at evaluateState()'s own
  // coefficients instead of noise -- see the big comment above for why.
  // The one-hot board-plane rows (everything before those 21) start at 0,
  // same as a fresh random init would roughly average to, so this only
  // changes the part of the kernel that has a known-good answer to start
  // from.
  //
  // WIDE_WARM_START_SCALE (2026-09-09): the first warm-started run scored
  // WORSE than no warm start at all (63.6% vs 65.6%) despite training
  // finally running past epoch 1 (best epoch 9) -- traced to
  // evaluateStateComponents()'s raw sub-scores being nowhere near
  // normalized (sampled ~250 real positions: mean |raw weighted sum| 1759,
  // max 8719), so feeding ORIGINAL_EVAL_WEIGHTS in unscaled saturated the
  // final tanh almost everywhere from epoch 0 (the game-1 starting
  // position's own prediction came out as an exact -1). A saturated tanh
  // has ~zero gradient, so this wasn't "starting from a good answer and
  // refining it" -- it was starting pinned against the wall the whole
  // network then had to claw its way back from. This scale brings the
  // typical pre-tanh magnitude down to roughly O(1-3) (order-of-magnitude
  // choice from that same sample, not precisely tuned) -- small enough to
  // leave real gradient signal, while keeping the coefficients'
  // *proportions* to each other (and hence the ranking/sign behavior that
  // made them 69.1%-good in the first place) exactly intact.
  // ABLATE_WARM_START=0 (added 2026-09-12): with card self-play data now
  // available, worth re-testing whether the network can beat the hand-coded
  // eval on its own instead of converging back near it (see McNemar
  // comparison this session -- new/old both statistically tied with
  // evaluateState()'s own 21-weight formula, consistent with the wide path
  // starting AT that formula and the deep path only ever learning a small
  // correction on top). 0 = pure random init, same as before warm-start was
  // introduced.
  const WIDE_WARM_START_SCALE = process.env.ABLATE_WARM_START !== undefined ? Number(process.env.ABLATE_WARM_START) : 0.001;
  const wideKernel = new Float32Array(INPUT_SIZE);
  const featureBase = INPUT_SIZE - ORIGINAL_EVAL_WEIGHTS.length;
  ORIGINAL_EVAL_WEIGHTS.forEach((w, i) => { wideKernel[featureBase + i] = w * WIDE_WARM_START_SCALE; });
  wideLayer.setWeights([tf.tensor(wideKernel, [INPUT_SIZE, 1])]);

  return model;
}

// Snapshot the exact data file this run trains on before anything else can
// touch it. Found the hard way (2026-09-06): selfplay-run.js truncates
// selfplay-data.jsonl (flags: "w") at the start of every self-play run, so
// starting a new self-play run after training silently destroys the exact
// dataset a trained model's accuracy numbers were measured against --
// making any later comparison (McNemar test, re-checking accuracy, etc.)
// impossible to redo correctly. Snapshots go in nnue/snapshots/, named by
// timestamp, and are never deleted automatically -- clean them up by hand
// once they're no longer needed for comparison.
function snapshotDataFile() {
  const dir = path.join(__dirname, "snapshots");
  fs.mkdirSync(dir, { recursive: true });
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const dest = path.join(dir, `selfplay-data.${stamp}.jsonl`);
  fs.copyFileSync(DATA_FILE, dest);
  console.log("snapshotted training data to:", dest);
  return dest;
}

async function main() {
  // WASM backend (2026-09-09): benchmarked ~37x faster than the default
  // pure-JS "cpu" backend on this model's exact forward-pass shape (169.6ms
  // vs 4.6ms per pass) -- no native compilation needed (unlike tfjs-node,
  // abandoned earlier for requiring Visual Studio Build Tools), just a
  // WASM binary that ships in the npm package. This is almost certainly
  // why training got so slow once data grew to 87k positions and wide&deep
  // added a second matmul path: the pure-JS backend was always this slow
  // per-op, it just hadn't been pushed hard enough before to notice.
  await tf.setBackend("wasm");
  await tf.ready();
  console.log("tf backend:", tf.getBackend());

  const snapshotPath = snapshotDataFile();
  const { inputs, labels, trainingLabels, pieceCounts, sampleWeights, firstBoard, gameIds } = await loadData(snapshotPath);
  console.log("positions loaded:", inputs.length);
  if (inputs.length < 50) {
    console.log("too little data for a meaningful sanity run, but proceeding anyway to verify the pipeline.");
  }

  // VAL_DATA_FILE (2026-09-19): validate on an external file instead of the last
  // 10% of games of the training data. Lets several data mixes (different rounds,
  // sampling fractions) be compared on exactly the same held-out positions. The
  // file's games must not be in the training data (nnue/mix-datasets.js makes
  // sure of that). Training then uses every position of the main file.
  let externalValStart = null;
  if (process.env.VAL_DATA_FILE) {
    const val = await loadData(process.env.VAL_DATA_FILE);
    externalValStart = inputs.length;
    for (let i = 0; i < val.inputs.length; i += 1) {
      inputs.push(val.inputs[i]);
      labels.push(val.labels[i]);
      trainingLabels.push(val.trainingLabels[i]);
      pieceCounts.push(val.pieceCounts[i]);
      sampleWeights.push(val.sampleWeights[i]);
    }
    console.log("external validation file:", process.env.VAL_DATA_FILE, "->", val.inputs.length, "positions (train", externalValStart + ")");
  }

  // Split by GAME, not by raw position index (2026-09-07): a game's ~20-90
  // plies are all highly correlated (same evolving board), and get written
  // to the file back-to-back. A plain "first 90% of lines" cut can leave
  // one game straddling the boundary -- worse, comparing two training runs
  // whose total position counts differ (e.g. after merging in more
  // self-play) silently changes WHICH games land in validation each time,
  // making accuracy numbers across runs not actually comparable to each
  // other despite looking like the same kind of measurement. Cutting at a
  // game boundary instead means validation is always "the last ~10% of
  // games", a stable, leak-free, comparable definition run to run.
  const totalGames = gameIds.length ? gameIds[gameIds.length - 1] + 1 : 0;
  const valGameStart = Math.floor(totalGames * 0.9);
  const gameBoundaryIndex = gameIds.findIndex((id) => id >= valGameStart);
  const splitAt = externalValStart !== null ? externalValStart : (gameBoundaryIndex === -1 ? gameIds.length : gameBoundaryIndex);
  console.log("train/val split: game", valGameStart, "of", totalGames, "-> position index", splitAt, "of", inputs.length, externalValStart !== null ? "(external validation file)" : "");

  // tfjs.js's LayersModel.fit doesn't support the `sampleWeight` option yet
  // (throws "sample weight is not supported yet" -- confirmed 2026-09-06,
  // @tensorflow/tfjs 4.22). Approximate weighting by oversampling instead:
  // repeat each TRAINING example round(weight * OVERSAMPLE_SCALE) times
  // (minimum 1, so low-weight examples are thinned but not dropped
  // entirely). Validation stays exactly as-is, unduplicated, so accuracy/
  // loss measurements on it remain a clean, unbiased sample.
  // Kept at 1.5 (2026-09-06): a run combining this with batchSize=256
  // tanked accuracy (53.3% vs the earlier 67.9%), but a follow-up small-
  // sample test varying batchSize alone (32/64/128/256) showed no clear
  // batchSize effect -- so batchSize is the better-supported suspect (see
  // its own comment below), not this. Leaving this at 1.5 rather than
  // reverting everything that changed at once; only reverting what's
  // actually implicated.
  const OVERSAMPLE_SCALE = process.env.ABLATE_OVERSAMPLE !== undefined ? Number(process.env.ABLATE_OVERSAMPLE) : 1.5;
  const trainIndices = [];
  for (let i = 0; i < splitAt; i += 1) {
    const copies = Math.max(1, Math.round(sampleWeights[i] * OVERSAMPLE_SCALE));
    for (let c = 0; c < copies; c += 1) trainIndices.push(i);
  }
  console.log("training examples after pliesFromEnd-weighted oversampling:", trainIndices.length, "(from", splitAt, "unique)");

  // tf.tensor2d's own array-of-arrays flatten() chokes (RangeError: Invalid
  // array length) on tens of thousands of Float32Arrays -- build one flat
  // buffer by hand instead and hand tensor2d the shape directly.
  const trainFlat = new Float32Array(trainIndices.length * INPUT_SIZE);
  const trainLabelsFlat = new Float32Array(trainIndices.length);
  trainIndices.forEach((idx, row) => {
    trainFlat.set(inputs[idx], row * INPUT_SIZE);
    trainLabelsFlat[row] = trainingLabels[idx];
  });

  const xVal = tf.tensor2d(inputs.slice(splitAt));
  const yVal = tf.tensor2d(labels.slice(splitAt), [inputs.length - splitAt, 1]);

  const model = buildModel();

  // Manual early stopping with best-weights tracking: tfjs.js's built-in
  // earlyStopping callback can stop training but (unlike Keras'
  // restoreBestWeights) doesn't hand back the best epoch's weights, and the
  // previous run showed the best val_loss is often epoch 0 -- exporting
  // "whatever epoch training happened to stop on" would still ship an
  // overfit model. Snapshot weights as plain arrays (cheap at this model
  // size) whenever val_loss improves, and stop early if it hasn't improved
  // in PATIENCE epochs.
  let bestValLoss = Infinity;
  let bestWeights = null;
  let bestEpoch = -1;
  let epochsSinceBest = 0;
  let stoppedEarly = false;

  // Reverted batchSize 256->64 (2026-09-06) -- best-epoch has been
  // consistently ~0-1 across every run, so a bigger batchSize means far
  // fewer gradient steps land before that first epoch finishes: it was
  // training less, not just faster. EPOCHS_CAP/PATIENCE come from the
  // NNUE_VARIANT toggle above, raised for the "new" variant alongside the
  // lower learning rate/dropout to give slower convergence room to actually
  // show whether best-epoch moves past 0-1.
  //
  // Chunked model.fit() instead of one all-at-once fit(xTrain, yTrain)
  // (2026-09-17): fit() needs its input tensor up front, and on the wasm
  // backend that means copying the WHOLE training set into wasm linear
  // memory as a single allocation before training even starts -- confirmed
  // live on the round-1 dataset (173k oversampled rows, ~3.8GB) as a
  // "memory access out of bounds" wasm RuntimeError, past whatever this
  // build's wasm heap can grow to.
  //
  // A first fix (hand-rolled per-BATCH_SIZE=64-row loop calling
  // trainOnBatch directly) avoided the OOM but was confirmed live to be
  // dramatically slower than fit() itself -- 4+ minutes without finishing
  // even epoch 0, independent of CPU power-saving mode (same slowness with
  // it off). ~2700 individual JS/async round-trips per epoch (tensor
  // create -> await trainOnBatch -> dispose, once per 64-row batch) is
  // apparently a lot more per-call overhead than fit()'s own internal batch
  // loop pays. Splitting into much bigger CHUNK_SIZE tensors and calling
  // fit() ONCE per chunk (still with batchSize:64 internally, so the actual
  // gradient-step granularity/training dynamics are unchanged from before)
  // cuts that down to ~9 JS-level calls per epoch instead of ~2700, while
  // each chunk tensor (20000 rows -> ~440MB) stays comfortably under
  // whatever ceiling broke on the full 3.8GB one.
  const CHUNK_SIZE = 20000;
  const numExamples = trainIndices.length;
  const chunkStarts = [];
  for (let s = 0; s < numExamples; s += CHUNK_SIZE) chunkStarts.push(s);
  const order = new Int32Array(numExamples);
  for (let i = 0; i < numExamples; i += 1) order[i] = i;
  function shuffleOrder() {
    for (let i = order.length - 1; i > 0; i -= 1) {
      const j = Math.floor(Math.random() * (i + 1));
      const tmp = order[i]; order[i] = order[j]; order[j] = tmp;
    }
  }

  for (let epoch = 0; epoch < EPOCHS_CAP; epoch += 1) {
    shuffleOrder();
    let trainLossSum = 0;
    for (const start of chunkStarts) {
      const rows = Math.min(CHUNK_SIZE, numExamples - start);
      const chunkInput = new Float32Array(rows * INPUT_SIZE);
      const chunkLabel = new Float32Array(rows);
      for (let r = 0; r < rows; r += 1) {
        const srcRow = order[start + r];
        chunkInput.set(trainFlat.subarray(srcRow * INPUT_SIZE, (srcRow + 1) * INPUT_SIZE), r * INPUT_SIZE);
        chunkLabel[r] = trainLabelsFlat[srcRow];
      }
      const xChunk = tf.tensor2d(chunkInput, [rows, INPUT_SIZE]);
      const yChunk = tf.tensor2d(chunkLabel, [rows, 1]);
      const history = await model.fit(xChunk, yChunk, { epochs: 1, batchSize: 64, shuffle: true, verbose: 0 });
      trainLossSum += history.history.loss[0] * rows;
      xChunk.dispose();
      yChunk.dispose();
    }
    const trainLoss = trainLossSum / numExamples;
    const valLossTensor = model.evaluate(xVal, yVal, { batchSize: 256 });
    const valLossScalar = Array.isArray(valLossTensor) ? valLossTensor[0] : valLossTensor;
    const valLoss = (await valLossScalar.data())[0];
    (Array.isArray(valLossTensor) ? valLossTensor : [valLossTensor]).forEach((t) => t.dispose());

    // Log every epoch, not just every 5th (2026-09-17) -- diagnosing the
    // round-1 retrain regression needed the real epoch-by-epoch trajectory
    // and GitHub Actions job logs aren't readable via the API this project
    // has access to, so sparse logging meant flying blind on any future
    // cloud run too.
    console.log("epoch", epoch, "loss", trainLoss.toFixed(4), "val_loss", valLoss.toFixed(4));
    if (valLoss < bestValLoss) {
      bestValLoss = valLoss;
      bestEpoch = epoch;
      epochsSinceBest = 0;
      bestWeights = model.getWeights().map((w) => ({ shape: w.shape, data: Array.from(w.dataSync()) }));
    } else {
      epochsSinceBest += 1;
      if (epochsSinceBest >= PATIENCE) {
        stoppedEarly = true;
        break;
      }
    }
  }
  console.log(
    stoppedEarly ? `stopped early at best epoch ${bestEpoch} (val_loss ${bestValLoss.toFixed(4)})` : `finished all epochs, best was epoch ${bestEpoch} (val_loss ${bestValLoss.toFixed(4)})`
  );

  // Plain tfjs (no native/node backend) has no file:// save handler, and we
  // want plain-JSON weights anyway -- the runtime forward pass in the
  // extension will be a small hand-written function, not a bundled tfjs
  // runtime, so export exactly the numbers it needs. Export the BEST epoch's
  // weights, not whatever the model ended up holding after the last epoch.
  const weights = bestWeights || model.getWeights().map((w) => ({ shape: w.shape, data: Array.from(w.dataSync()) }));
  fs.mkdirSync(path.join(__dirname, "model"), { recursive: true });
  // Variant-specific filename so an "old"-variant A/B run doesn't clobber
  // the "new" variant's weights (or vice versa) -- both stick around for
  // comparison instead of only ever having the most recent run's result.
  const weightsFileName = VARIANT === "new" ? "weights.json" : `weights.variant-${VARIANT}.json`;
  const weightsPath = path.join(__dirname, "model", weightsFileName);
  fs.writeFileSync(weightsPath, JSON.stringify(weights));
  console.log("weights saved to nnue/model/" + weightsFileName);

  // Restore the exported (best-epoch) weights onto the model before the
  // sanity check -- otherwise this would predict with the LAST epoch's
  // weights, which is exactly the overfit state we just chose not to export.
  model.setWeights(weights.map((w) => tf.tensor(w.data, w.shape)));

  // Sanity check: starting position should be close to 0 (roughly balanced).
  // Uses firstBoard captured in loadData() rather than re-reading the file
  // here -- learned this the hard way (2026-09-07): a concurrent self-play
  // run, or a manual delete of the snapshot, can make the file gone by now,
  // crashing the run after training/saving already finished.
  const pred = model.predict(tf.tensor2d([encodeBoard(firstBoard, "white")]));
  console.log("prediction for the game-1 starting position (white to move):", (await pred.data())[0]);

  // "Accuracy" isn't really the right frame for a regression model (that's
  // what val_loss/MSE already measures), but the owner asked for a concrete
  // win/loss-direction number, not just a loss value. Sign-agreement rate:
  // for validation positions with a DECISIVE outcome (label != 0 -- draws
  // and unfinished games have no "direction" to agree with), what fraction
  // of the time did the predicted sign (who's favored) match who actually
  // went on to win. Reported overall and split into early/late game (by
  // ply-ish proxy: pieces remaining) since decisive endgame-like positions
  // should be much easier to call correctly than balanced opening ones.
  const valLabels = labels.slice(splitAt);
  const valPreds = await model.predict(xVal).data();
  let decisive = 0, correct = 0;
  let fewPieces = 0, fewCorrect = 0, manyPieces = 0, manyCorrect = 0;
  const valPieceCounts = pieceCounts.slice(splitAt);
  valLabels.forEach((label, i) => {
    if (label === 0) return;
    decisive += 1;
    const agree = Math.sign(valPreds[i]) === Math.sign(label);
    if (agree) correct += 1;
    const pieceCount = valPieceCounts[i];
    if (pieceCount <= 12) {
      fewPieces += 1;
      if (agree) fewCorrect += 1;
    } else {
      manyPieces += 1;
      if (agree) manyCorrect += 1;
    }
  });
  console.log(
    "sign-agreement accuracy on decisive validation positions:",
    decisive ? `${((correct / decisive) * 100).toFixed(1)}% (${correct}/${decisive})` : "n/a (no decisive validation positions)"
  );
  console.log(
    "  fewer pieces on board (<=12, endgame-ish):",
    fewPieces ? `${((fewCorrect / fewPieces) * 100).toFixed(1)}% (${fewCorrect}/${fewPieces})` : "n/a"
  );
  console.log(
    "  more pieces on board (>12, opening/midgame-ish):",
    manyPieces ? `${((manyCorrect / manyPieces) * 100).toFixed(1)}% (${manyCorrect}/${manyPieces})` : "n/a"
  );
}

if (require.main === module) {
  main();
} else {
  // Exposed for one-off analysis scripts (e.g. McNemar comparison between
  // two saved weight files on the same validation split) that need the
  // exact same filtering/game-boundary logic main() uses, without
  // duplicating it and risking a subtly different split.
  module.exports = { loadData, DATA_FILE, INPUT_SIZE, FEATURE_NAMES, snapshotDataFile, ORIGINAL_EVAL_WEIGHTS, buildModel };
}
