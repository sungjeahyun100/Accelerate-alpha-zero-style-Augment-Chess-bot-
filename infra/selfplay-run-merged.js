const { Worker } = require("worker_threads");
const os = require("os");
const path = require("path");
const fs = require("fs");

const WORKER_COUNT = Math.max(1, os.cpus().length - 1);
const RUN_MS = Number(process.argv[2] || 600000); // default 10 minutes
// SELFPLAY_SEARCH_DEPTH env override (2026-09-13): for the NNUE-bootstrap
// deep-search quality test -- deeper search than the usual bulk-throughput
// depth=3 gives NNUE-guided self-play a real chance to find genuinely better
// lines than the hand-coded eval would, at the cost of far fewer games/hour.
const SEARCH_DEPTH = Number(process.env.SELFPLAY_SEARCH_DEPTH) || 3;
// (f) from the 2026-09-06 throughput/quality comparison: flexibleBudget
// ("우리 AI") with an 80ms raw budget (effective ~640ms after the 8x
// multiplier in engine.optimized.js's searchBestAction) reliably reaches
// completedDepth ~1 while keeping ~38% of the original heuristic-only
// throughput -- chosen over the faster-but-shallower options after the
// forEachPiece/workerUniqueAlliedPieceCount perf fixes made this depth
// newly affordable.
// SELFPLAY_SEARCH_MS env override (2026-09-07): for a deliberately small,
// deep-search batch (quality over quantity -- see the 2026-09-07 "beat
// evaluateState()" investigation) instead of the usual bulk-throughput
// run. Piece variety (SELFPLAY_SPECIAL_TYPES) is untouched either way --
// this only trades off how MANY positions come out per hour, not which
// piece types can appear in them.
const SEARCH_TIME_MS = Number(process.env.SELFPLAY_SEARCH_MS) || 80;
const FLEXIBLE_BUDGET = true;
// Raised from 60 (2026-09-06): 42% of games were hitting the ply cap before
// anything was decided ("unfinished"), which nnue/train.js now excludes
// entirely from training (see its `unfinished` skip) since an unfinished
// game's outcome is unknown, not "balanced" -- so those games were pure
// wasted self-play compute. Giving games more room to actually conclude
// should cut that waste and also finally produce some endgame-ish (few
// pieces left) positions, which the last validation set had zero of.
// Raised again 90->150 (2026-09-06): checked what "unfinished" games
// actually looked like at the 90-ply cutoff -- they average FEWER pieces
// remaining (22.6) than decisive games do at their end (26.6), meaning they
// were genuinely still trading material in an active fight, not stuck/
// stalled. Per-move search cost doesn't depend on maxPlies, so total
// positions/hour should stay roughly flat (same CPU-seconds -> same number
// of moves computed) -- this mainly trades "fewer completed games per hour"
// for "more of those games actually reaching a decisive result" rather than
// being pure throughput loss.
// Raised again 150->300 (2026-09-07), now that selfplay-worker.js's
// stagnation check (50-move-rule-equivalent) no longer scales with this
// value -- previously, raising maxPlies also loosened the stall-detection
// threshold by the same factor, so a "genuinely stuck" game and a
// "genuinely still fighting" game got harder to tell apart the higher this
// went. Now that they're decoupled, this is purely an outer safety net for
// still-progressing games -- a stalled game still gets cut at a fixed 30
// plies without progress, regardless of this number.
// NOTE: this only takes effect on a fresh `node selfplay-run-merged.js`
// invocation -- an already-running process has this constant baked in from
// startup and won't pick up the change.
const MAX_PLIES = 300;
// This is the merged-engine (engine-merged.js via selfplay-worker-merged.js)
// data-generation run -- output goes to its own clearly-labeled file so it
// never collides with or overwrites the old engine.optimized.js-based
// selfplay-data*.jsonl production data (those are gitignored and untouched
// by this run).
const OUT_FILE = process.env.SELFPLAY_OUT_FILE
  ? path.join(__dirname, process.env.SELFPLAY_OUT_FILE)
  : path.join(__dirname, `selfplay-data.merged-engine-${new Date().toISOString().slice(0, 10)}.jsonl`);

// Every game needs its own random seed -- without this, selfplay-worker.js's
// seeded PRNG (used for special-piece placement and exploration moves)
// defaults to the same fixed seed every time, so every "different" game was
// actually replaying the same deterministic setup over and over (caught this
// by noticing a 174:12 outcome skew that got MORE extreme with more games,
// not less -- real per-game variance should average out, a repeated single
// game doesn't).
let nextSeed = 1;
function runGame() {
  const seed = (Date.now() % 1e9) * 1000 + (nextSeed++ % 1000);
  return new Promise((resolve) => {
    const worker = new Worker(path.join(__dirname, "selfplay-worker-merged.js"), {
      workerData: { searchDepth: SEARCH_DEPTH, searchTimeMs: SEARCH_TIME_MS, maxPlies: MAX_PLIES, seed, flexibleBudget: FLEXIBLE_BUDGET }
    });
    worker.on("message", (msg) => {
      worker.terminate();
      resolve(msg);
    });
    worker.on("error", (err) => {
      worker.terminate();
      resolve({ error: String(err) });
    });
  });
}

async function main() {
  const deadline = Date.now() + RUN_MS;
  const outStream = fs.createWriteStream(OUT_FILE, { flags: "w" });
  let gamesDone = 0;
  let positionsWritten = 0;
  let errors = 0;
  const outcomeCounts = { white: 0, black: 0, draw: 0, unfinished: 0 };

  console.log("workers:", WORKER_COUNT, "run budget (ms):", RUN_MS, "-> until", new Date(deadline).toISOString());

  const t0 = Date.now();
  // Progress heartbeat -- for a 1-hour run, checking in at 15 and 45 minutes
  // is more useful than the original 30-minute cadence (used for longer
  // multi-hour production runs), so it fires every 15 minutes here instead.
  const progressTimer = setInterval(() => {
    const elapsedMin = ((Date.now() - t0) / 60000).toFixed(1);
    console.log(
      `[progress @ ${elapsedMin}min] games: ${gamesDone} errors: ${errors} positions: ${positionsWritten}`,
      "outcomes:", JSON.stringify(outcomeCounts)
    );
  }, 15 * 60 * 1000);

  async function workerLoop() {
    while (Date.now() < deadline) {
      const result = await runGame();
      if (result.error) {
        errors += 1;
        continue;
      }
      gamesDone += 1;
      outcomeCounts[result.outcome] = (outcomeCounts[result.outcome] || 0) + 1;
      for (const entry of result.record) {
        outStream.write(JSON.stringify(entry) + "\n");
        positionsWritten += 1;
      }
    }
  }

  await Promise.all(Array.from({ length: WORKER_COUNT }, () => workerLoop()));
  clearInterval(progressTimer);
  outStream.end();
  const elapsed = Date.now() - t0;

  console.log("elapsed ms:", elapsed);
  console.log("games:", gamesDone, "errors:", errors);
  console.log("games/sec:", (gamesDone / (elapsed / 1000)).toFixed(3));
  console.log("positions written:", positionsWritten, "-> ", OUT_FILE);
  console.log("outcome distribution:", outcomeCounts);
}

main();
