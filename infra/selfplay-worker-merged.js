// Node worker_threads version of a self-play game generator: plays one full
// game start-to-finish using the (fast, simplified) engine, and reports the
// move-by-move (board, evalScore, outcome) tuples for later NNUE training.
// This is deliberately NOT the review-quality engine.optimized.js -- self-play
// data generation wants speed over per-move perfection, so it uses a much
// smaller search depth/time budget per move.
const { parentPort, workerData } = require("worker_threads");
// Uses the REAL site aiWorker.js logic directly (site-oracle/real-ai-engine.js,
// a patched-to-export copy fetched live from https://augmentchess.org/assets/aiWorker.js)
// instead of our own reimplemented engine.optimized.js -- this makes self-play data
// generation immune to any card/piece porting bugs the audit is still finding,
// since the rules themselves are now the real site logic, not a reimplementation.
globalThis.self = globalThis;
globalThis.addEventListener = () => {};
const engine = require("./engine-merged.js");

// NNUE-in-the-loop self-play (2026-09-12 experiment, opt-in via
// SELFPLAY_NNUE_EVAL=1): normal self-play always searches with the hand-
// coded evaluateState(), which means training data quality is capped by
// that heuristic's own move-choice ceiling no matter how good the NNUE
// trained on it gets. This plugs the currently-deployed NNUE in as the
// search's leaf evaluator instead, mirroring extension/nnue.js's own
// evaluateForSearch adapter exactly (same encode -> forward -> *SCORE_SCALE,
// same evaluateState() fallback on terminal positions) so self-play-side
// behavior matches what the live extension would actually do. Root safety
// filters (rootCandidateHardSafetyIssue/rootCandidateSoftSafetyIssue) don't
// read context.evalFn at all -- confirmed by reading engine.optimized.js --
// so they stay active unchanged regardless of which evaluator this plugs in.
// Kept default-off: this is a higher-risk data source (see the 2026-09-12
// discussion on feedback-loop/self-reinforcement risk) meant to be run in
// small, separately-evaluated batches, never silently mixed into the main
// self-play pipeline.
let nnueEvalFn = null;
if (process.env.SELFPLAY_NNUE_EVAL === "1") {
  const path = require("path");
  const { encodeBoard } = require("./nnue/encode.js");
  const { loadWeights, forward } = require("./nnue/forward.js");
  const weightsPath = process.env.SELFPLAY_NNUE_WEIGHTS || path.join(__dirname, "extension", "model", "nnue-squall.json");
  const nnueWeights = loadWeights(weightsPath);
  const SCORE_SCALE = 100; // matches extension/nnue.js's evaluateForSearch
  nnueEvalFn = function (boardState, aiColor) {
    const input = encodeBoard(boardState.board, aiColor, boardState.deckSlots);
    if (input === null) return engine.evaluateState(boardState, aiColor); // terminal position
    return forward(nnueWeights, input) * SCORE_SCALE;
  };
  console.log("[worker] SELFPLAY_NNUE_EVAL active, weights:", weightsPath);
}

// Movement-only special pieces confirmed (by reading engine.js's actual move
// functions this session, not guessed) to work standalone without any
// card/deck/rule state -- no ability actions, no ammo/chain/platform/crown
// mechanics. Standard pieces already get full coverage for free via the real
// Stockfish hybrid in the review feature; self-play data only needs to teach
// a future NNUE about positions where a special piece like these is actually
// on the board, since that's the blind spot Stockfish can't help with.
const SELFPLAY_SPECIAL_TYPES = [
  "amazon", "cardinal", "pegasus", "assassin", "dragon",
  "cannon", "grasshopper", "hook", "herald", "camel", "alfil", "ferz", "eagle",
  // Added 2026-09-05 after reading generateMovesForPiece for the rest of the
  // encyclopedia. berserker's tier is computed live from the board (no card
  // state needed) so it's exactly as safe as the pieces above. magicGirl's
  // awakening (magicGirlSurge) fires from recordWorkerCapturedPieces, the
  // engine's general capture-recording path used everywhere a piece is
  // captured (not just the bear-retaliation call site) -- so it awakens
  // normally whenever that color loses a piece, same as real play, no
  // special-casing needed here. It's a one-turn-only buff (reset when that
  // color's turn ends), so it flickers on right after a capture and off again.
  "berserker", "magicGirl",
  // windmill/trickster/merchant need a small amount of self-play-only state
  // management (see advanceSelfPlaySpecialState below) since their real
  // rules depend on card/turn state this sandbox doesn't otherwise have.
  // These update rules were specified directly by the project owner for
  // self-play purposes -- they are not a guess at the real game's rules.
  "windmill", "trickster", "merchant",
  // Added after finding these are "auras"/passive rules baked directly into
  // core engine functions (pawnMoves, canWorkerCaptureTarget,
  // resolveWorkerReaperNearbyDeaths, the siren exposure tick) rather than
  // card-gated -- engine.generateActions/applyAction already reproduce them
  // exactly with zero extra plumbing here, same as magicGirl's surge above.
  // recruiter/guard can never actually capture without a card-only
  // "basicTraining"/royal-command state this sandbox doesn't have, so they'll
  // just wander harmlessly -- a real (if passive) subset of their rules, not
  // a guess.
  "knightmaster", "standardBearer", "idol", "siren", "reaper", "recruiter", "guard",
  // coffin/babyBear have NO real movement at all (generateMovesForPiece
  // returns [] unconditionally) -- the project owner specified a self-play
  // house rule for them instead: each of their own turns, hop to one of the
  // 8 adjacent squares if any is empty. This is handled by hand in
  // advanceSelfPlaySpecialState below since the real engine will never
  // generate a move for these types.
  "coffin", "babyBear",
  // colossus/bigRook are 2x2 "large pieces" -- the same piece object is
  // placed in all 4 of colossusCells(anchorRow, anchorCol) and tagged with
  // anchorRow/anchorCol, which is exactly how the real engine represents
  // them (confirmed via forEachPiece's anchor-cell skip and
  // clearPieceCells' whole-board object-identity scan, both in engine.js).
  // makeStartingBoard below has dedicated placement logic for these two
  // instead of the normal single-square draw. scarecrow (no real movement
  // rule of its own -- purely a conveyor-belt card delivery, not a
  // placeable piece) and shotgunKing (ammo-gated, excluded per owner) are
  // deliberately NOT included.
  "colossus", "bigRook",
  // 2026-09-13 batch: hedgehog/princess/campfire are single-square PIECE
  // types (unlike bigBishop, deliberately NOT added here -- see the
  // session report for why: it reuses bigRook's move-application plumbing
  // but several secondary target-exclusion lists elsewhere in
  // engine.optimized.js only know about "bigRook"/"colossus" by name, a
  // residual gap this session's time budget didn't cover closing).
  // princess/campfire need no extra init (their behavior -- queen-count
  // check, adjacency aura -- is computed live from the board every time).
  // hedgehog needs its counter-attack budget seeded (see makeSpecialPiece).
  "hedgehog", "princess", "campfire",
  // 2026-09-15 patch batch (16 new cards): paladin/octopus/clockwork/parrot
  // are the 4 PIECE-phase cards, all single-square, all confirmed
  // self-contained by reading generateMovesForPiece/threeMoveAllowed in the
  // freshly re-fetched aiWorker.js (site-oracle/aiWorker-fresh-20260915.js):
  // - paladin: plain leapMoves(knight deltas); its no-capture rule lives in
  //   canWorkerCaptureTarget (attackerType-based) and threeMoveAllowed, both
  //   read live off the piece/board, no card/deck state.
  // - octopus: plain leapMoves(queenDirections); its submerge-when-no-
  //   adjacent-enemy tick (completeThreeTurn) runs unconditionally every
  //   turn inside finishWorkerMove, not gated behind ever having drawn the
  //   "octopus"/"metal" cards.
  // - clockwork: rayMoves(queenDirections) gated by a live 8-neighbor
  //   allied-piece adjacency check in generateMovesForPiece -- no state.
  // - parrot: mimics boardState.parrotMovement[color], which every move
  //   (any piece, any color) now lazily seeds via applyMoveAction's wrapper
  //   (see engine-merged.js); a parrot placed before any move has happened
  //   for its color simply has no moves yet (matches real: `if
  //   (!memory?.type) return [];`), not a crash.
  "paladin", "octopus", "clockwork", "parrot"
];

// Pool trickster draws its per-turn movement type from. Deliberately NOT the
// full TRICKSTER_MOVEMENT_TYPES pool from engine.js -- types that depend on
// external resource/structure state (merchant's gold, shotgunKing's ammo,
// colossus/bigRook's 2x2 footprint) would just be permanently-inert or
// malformed without that state, which teaches the network nothing. Restrict
// to the movement-only-safe pieces confirmed above.
const TRICKSTER_SELFPLAY_POOL = [
  "amazon", "cardinal", "pegasus", "assassin", "dragon",
  "cannon", "grasshopper", "hook", "herald", "camel", "alfil", "ferz", "eagle",
  "berserker", "magicGirl", "queen", "rook", "bishop", "knight"
];

// Fraction of eligible (non-king, non-pawn) squares per side that get
// replaced by a random special piece, per game. 0 is a valid roll -- most
// games should still be "mostly standard, maybe one or two specials" rather
// than every game being special-piece-saturated, since the real games this
// is meant to generalize to are like that too.
const SPECIAL_PIECE_CHANCE = 0.2;

function makeSpecialPiece(type, color, rng) {
  const piece = { type, color, moved: false };
  if (type === "windmill") piece.windmillMode = "bishop"; // owner's spec: starts as bishop
  if (type === "trickster") piece.tricksterMoveType = TRICKSTER_SELFPLAY_POOL[Math.floor(rng() * TRICKSTER_SELFPLAY_POOL.length)];
  if (type === "merchant") piece.gold = 0;
  // hedgehog (2026-09-13 batch): king-step movement is self-contained, and
  // its "역습" counter-attack is resolved generically by
  // resolveWorkerBearRetaliation on every capture (same unconditional
  // capture-recording path "bear" itself would use) -- it just needs its
  // counter budget seeded, mirroring randomRouletteInitialPieceState's
  // { bearRetaliationsRemaining: 3 } for this type in engine.optimized.js.
  if (type === "hedgehog") piece.bearRetaliationsRemaining = 3;
  return piece;
}

function makeStartingBoard(rng) {
  const back = ["rook", "knight", "bishop", "queen", "king", "bishop", "knight", "rook"];
  const board = Array.from({ length: 8 }, () => Array(8).fill(null));
  for (const color of ["black", "white"]) {
    const backRow = color === "black" ? 0 : 7;
    const pawnRow = color === "black" ? 1 : 6;
    // Default-fill this side's pawn rank first; a 2x2 large-piece placement
    // below may overwrite two of these cells before we get to them.
    for (let c = 0; c < 8; c++) board[pawnRow][c] = { type: "pawn", color, moved: false };
    const consumedCols = new Set();
    for (let c = 0; c < 8; c++) {
      if (consumedCols.has(c)) continue;
      const backType = back[c];
      if (backType === "king" || !(rng() < SPECIAL_PIECE_CHANCE)) {
        board[backRow][c] = { type: backType, color, moved: false };
        continue;
      }
      const type = SELFPLAY_SPECIAL_TYPES[Math.floor(rng() * SELFPLAY_SPECIAL_TYPES.length)];
      if (type === "colossus" || type === "bigRook") {
        // Large pieces need a free 2-wide, 2-tall footprint: this column and
        // the next, spanning the back rank and the pawn rank right behind
        // it. If the next column doesn't exist, is the king's column, or was
        // already claimed by a neighboring large piece, just fall back to
        // this square's plain back-rank piece instead of forcing it.
        const nextCol = c + 1;
        if (nextCol < 8 && back[nextCol] !== "king" && !consumedCols.has(nextCol)) {
          const anchorRow = Math.min(backRow, pawnRow);
          const large = { type, color, moved: false, anchorRow, anchorCol: c };
          [{ row: backRow, col: c }, { row: backRow, col: nextCol }, { row: pawnRow, col: c }, { row: pawnRow, col: nextCol }]
            .forEach(({ row, col }) => { board[row][col] = large; });
          consumedCols.add(c);
          consumedCols.add(nextCol);
          continue;
        }
        board[backRow][c] = { type: backType, color, moved: false };
        continue;
      }
      board[backRow][c] = makeSpecialPiece(type, color, rng);
    }
  }
  return board;
}

// Self-play card pool (expanded 2026-09-13 per explicit owner instruction
// "자가대국에 모든 카드를 풀에 추가해" -- add ALL cards to the self-play
// pool). This supersedes the previous 18-card "conservative verified-safe
// subset", which had been narrowed down unilaterally in an earlier session
// without telling the owner; the owner was unhappy about that silent
// narrowing and wants the full hand-draftable catalog actually included this
// time, not re-narrowed again out of caution.
//
// This pool is every card `effect` id engine.optimized.js has REAL apply
// logic for (found by grepping every `effect === "..."` / `card.effect ===
// "..."` dispatch branch across its two big effect blocks -- the eligibility/
// canUseCard-style chain and the apply/resolve chain -- plus the targeting
// chain), MINUS:
//   - the ~20 RULE-category cards (monochromeChess, blackHole, football,
//     platformRule, etc. -- see WORKER_RULE_TICKET_CANDIDATES in
//     engine.optimized.js). Those are a structurally separate mechanism:
//     board-wide rules that apply automatically at game start / via RULE-mode
//     selection, and per the site's own rules page are NEVER drafted into a
//     player's hand in the real game (hand drafts only ever offer OPENING/
//     MIDDLE/END category "패시브"/"액티브" cards). Putting a RULE effect in
//     this hand pool would teach self-play from card appearances that can
//     never actually happen in a real drafted hand, so they stay out on
//     structural grounds, not caution.
//   - "alehkineMachineGun", a legacy misspelled alias for "alekhineMachineGun"
//     (both exist in one eligibility check for backward compatibility, but
//     only the correctly-spelled id is ever actually assigned to a real
//     card) -- kept "alekhineMachineGun" only, to avoid a duplicate-effect
//     hand slot that's really the same card twice.
//   - "bloodCard": its full targeting requires hasVampireLord(boardState,
//     color) (engine.optimized.js's generateCardTargets), i.e. an actual
//     vampireLord piece already on the board. Self-play's board/piece setup
//     (SELFPLAY_SPECIAL_TYPES above) never places a vampireLord, so this card
//     would silently generate zero targets and no-op every single game. That
//     needs a whole extra special-piece type added to the self-play piece
//     roster to fix properly, which is out of scope for a card-pool change --
//     excluded for now, named here explicitly rather than silently.
// "timeSaveLoad" and "knightJourneyHint" also appear in engine.optimized.js
// (only in its card star-cost table) but have NO apply/eligibility/targeting
// logic anywhere in the file -- they're referenced by name with no real
// implementation, i.e. a dead end, not an oversight to fix here.
//
// Everything else below was checked for the same "does it read board-wide
// state this sandbox never initializes" risk the old comment worried about
// (kingOfTheHill/democracy/socialism/ruleTicket/genevaConvention/etc. were
// specifically named as concerns): engine.optimized.js's cloneState (which
// makeInitialState below calls via `engine.cloneState({})`) already
// normalizes essentially every one of these board-wide fields (hillKing,
// democracy, socialism, exhaustion, imperialStudies, fianchetto,
// pawnConversion, cornerKick, underpromotion, fileSurge, rookLift,
// afterimageQueen, royalCommand, coronation, ...) to safe empty/false
// defaults, and ruleTicket's own pendingRuleTickets mechanism is fully
// self-contained (pushed in its apply code, consumed by a per-turn tick
// elsewhere in engine.optimized.js) -- so none of those need any extra
// self-play-side init after all. This was verified empirically too: see the
// smoke-test batch run after this change (noted in the project HANDOFF/
// report), which played a batch of games with this full pool and fixed the
// couple of real crashes it turned up rather than quietly dropping those
// cards.
const SELFPLAY_CARD_POOL = [
  "alekhineMachineGun", "amazon", "apprenticeKnights", "armistice", "babyBear",
  "basicTraining", "bigRook", "binaMate", "bishopSnipe", "blackBox",
  "blackMagic", "blueJeans", "breakthroughOrder", "callingCard", "canceling",
  "chain", "chameleonMutation", "charge", "checker", "chimera",
  "cleanupPieces", "cleanupSacrifice", "clonePassive", "conversion",
  "cornerKick", "coronation", "deathSquad", "democracy", "desperado", "dice",
  "disarm", "dutch", "eagle", "emergencyEvacuation", "emptyLunchbox",
  "enPassantBang", "encouragement", "evasion", "exhaustion", "exile",
  "fanaticalRitual", "feudalContract", "fianchetto", "fieldPromotion",
  "fileSurge", "finalWeapon", "fleetingDream", "freeCastling", "freeMove",
  "freeze", "frenzy", "frontlineResponse", "gale", "genevaConvention",
  "ghost", "gomoku", "guard", "hallucination", "holdout", "homecoming",
  "hook", "horde", "horseRiding", "hypocrisy", "icbm", "iceSheet", "idol",
  "imperialStudies", "inertia", "injury", "insight", "ironMonarch", "joker",
  "judgment", "kingOfTheHill", "knightmate", "lastResistance", "lobster",
  "localConscription", "loyalist", "madHorse", "martyrdom", "merchantGuild",
  "missionary", "mistakeCard", "mongolianGambit", "moving", "ordination",
  "othello", "otherworld", "overwhelm", "palace", "panic", "parry",
  "pawnConversion", "pawnStorm", "poisonedPawn", "portalGun", "promotionRush",
  "prophecy", "quantumMechanics", "queenAfterimage", "queenCavalry",
  "queensGambit", "racingKing", "randomRoulette", "reaper", "reformation",
  "relay", "religiousVictory", "replayMove", "reposition", "retreat",
  "reversePawns", "rookLift", "royalCommand", "royalShield", "ruleTicket",
  "sacrifice", "severance", "shotgunKing", "socialism", "spy", "stake",
  "submerge", "substitution", "suicideBomber", "summonColossus",
  "suspiciousPotion", "switcheroo", "taunt", "timeClumsyAttack", "timeIsMine",
  "timePhaseShift", "traitor", "trickster", "trojanHorse", "trolley",
  "twins", "ultimatum", "undergroundBunker", "underpromotion", "vanish",
  "vip", "vortex", "whiteBox", "windmill", "witchTrial", "wizard", "zugzwang",
  // qxe1 (2026-09-13, found missing via a full effect-id audit against
  // engine.optimized.js): king/queen "usurpation" swap card
  // (workerQxe1UsurpationPlan). Not a RULE card, needs no extra self-play
  // state -- only requires this color to have a king and a queen on the
  // board, true in every game.
  "qxe1",
  // 2026-09-13: the 25-card "missing mechanics" batch (added to
  // engine.optimized.js this session, ported from the site bundle where
  // real source was found -- see the session report for per-card
  // citations). All 17 non-PIECE cards below were checked the same way as
  // the rest of this pool: their apply branches only touch per-piece flags
  // (nullification/recurrence/outpost/bribe) or lazily-initialized
  // boardState.<effect> passive flags read everywhere via optional
  // chaining (majesty/killerKing/overtake/pawnLeap/vanguard/infiltration/
  // reversal), so cloneState({}) not pre-seeding those fields is safe.
  // "miracle" and "schrodingerPawns" (bishop-range mass conversion and the
  // dual-location "quantum" pawn state) were NOT implemented this session
  // (flagged explicitly in the report) and are intentionally absent here.
  // "resolve" was implemented as a flag only -- the actual "doesn't
  // consume the turn" mechanic was not wired into the core move committer
  // (too risky to touch ~15 call sites under this session's time budget) --
  // so it is also intentionally excluded from self-play for now (it would
  // be a safe no-op-ish flag, not a crash risk, but also not a real effect
  // yet, so including it would just teach the network a card that does
  // nothing).
  "nullification", "recurrence", "outpost", "killerKing", "majesty",
  "overtake", "leap", "vanguard", "infiltration", "reversal", "lastStand",
  "fastGrowth", "earlyPromotion", "bribe", "conscription", "barricade",
  "collapse",
  // 2026-09-15 patch batch: all 16 new cards, ported from the fresh
  // aiWorker.js this session (see engine-merged.js's INTERNAL_THREE/FIVE/
  // EIGHT dispatch and the graft-progress notes). All checked the same way
  // as the rest of this pool -- their apply/tick logic only touches
  // lazily-initialized per-color boardState.<effect> flags or per-piece
  // fields (metalized/thiefVisited/locustOrigin/parrotMovement/etc.), none
  // of which makeInitialState needs to pre-seed (all read via optional
  // chaining with safe falsy defaults). "highlander" is an instant win
  // condition wired into resolveWorkerReligiousVictory/
  // updateWorkerCaptureFlags (both already called every turn inside
  // finishWorkerMove) -- needs no extra state at all. "paladin"/"octopus"/
  // "clockwork"/"parrot" (the 4 PIECE-phase cards) are listed here too so
  // self-play can also draw them as CARDS (transforming an existing piece
  // finish game start), on top of being directly placeable via
  // SELFPLAY_SPECIAL_TYPES above.
  "highlander", "thief", "disassembly", "falseStart", "proficiency",
  "locustSwarm", "longEnPassant", "extinction", "symmetry", "brutus",
  "clockwork", "mutation", "parrot", "paladin", "octopus", "metal"
];
// Real games apparently deal a fixed hand size that depends on game mode
// (owner: "반은 3장 받고 반은 6개 받는 게임" -- basic mode deals 3, grand
// mode deals 6, roughly a coin flip which one a given game is), not always
// the same number -- so self-play picks one hand size per GAME (both sides
// get the same size, matching a single game being one mode or the other),
// not per side.
const SELFPLAY_HAND_SIZES = [3, 6];

// Sample without replacement (Fisher-Yates partial shuffle) so a single
// hand never holds the same card effect twice -- real drafts don't offer
// duplicates either. Pool (18) comfortably covers the largest hand (6).
function drawSelfPlayCards(handSize, rng) {
  const pool = SELFPLAY_CARD_POOL.slice();
  const drawn = [];
  for (let i = 0; i < handSize && pool.length; i++) {
    const idx = Math.floor(rng() * pool.length);
    drawn.push(pool[idx]);
    pool.splice(idx, 1);
  }
  return drawn;
}

function makeSelfPlayDeck(color, rng, handSize) {
  return drawSelfPlayCards(handSize, rng).map((effect, i) => ({
    id: effect,
    instanceId: `${color}-${effect}-${i}`,
    effect,
    stars: 1 + Math.floor(rng() * 5),
    used: false,
    recovering: false
  }));
}

function makeInitialState(rng) {
  const state = engine.cloneState({});
  state.board = makeStartingBoard(rng);
  // Perf: canWorkerCaptureTarget's campfire-aura check early-exits on this
  // flag (see the comment at isWorkerCampfireProtected in
  // engine.optimized.js), so it needs setting here too since campfire can
  // be placed directly by the special-piece roster, not only via the card.
  if (state.board.some((row) => row.some((piece) => piece?.type === "campfire"))) state.hasCampfire = true;
  state.mode = "play";
  state.turn = "white";
  state.actionsRemaining = 1;
  const handSize = SELFPLAY_HAND_SIZES[Math.floor(rng() * SELFPLAY_HAND_SIZES.length)];
  state.deckSlots = { white: makeSelfPlayDeck("white", rng, handSize), black: makeSelfPlayDeck("black", rng, handSize) };
  state.captures = { white: [], black: [] };
  state.aiSearchNoCards = false; // cards enabled in self-play as of 2026-09-11 (see SELFPLAY_CARD_POOL above)
  state.aiFastEval = true; // throughput over per-move quality for data generation
  engine.setWorkerBoardDimensions(state);
  return state;
}

// Self-play-only state updates for the three special types whose real rules
// depend on card/turn machinery this sandbox doesn't have. Rules here were
// specified directly by the project owner (windmill starts bishop and
// alternates every ply, trickster re-rolls its borrowed movement type every
// ply, merchant gains 1 gold on each of its own side's turns) -- these are
// deliberate self-play house rules, not a guess at the real game's mechanics.
const EIGHT_DIRECTIONS = [
  [-1, -1], [-1, 0], [-1, 1], [0, -1], [0, 1], [1, -1], [1, 0], [1, 1]
];

function advanceSelfPlaySpecialState(state, rng, moverColor) {
  const board = state.board;
  const hops = [];
  for (let r = 0; r < board.length; r++) {
    for (let c = 0; c < board[r].length; c++) {
      const piece = board[r][c];
      if (!piece) continue;
      if (piece.type === "windmill") {
        piece.windmillMode = piece.windmillMode === "rook" ? "bishop" : "rook";
      } else if (piece.type === "trickster") {
        piece.tricksterMoveType = TRICKSTER_SELFPLAY_POOL[Math.floor(rng() * TRICKSTER_SELFPLAY_POOL.length)];
      } else if (piece.type === "merchant" && piece.color === moverColor) {
        piece.gold = (piece.gold || 0) + 1;
      } else if ((piece.type === "coffin" || piece.type === "babyBear") && piece.color === moverColor) {
        hops.push({ row: r, col: c });
      }
    }
  }
  // coffin/babyBear have no real moves at all -- self-play house rule: hop to
  // a random empty adjacent square on their own side's turn, if one exists.
  hops.forEach(({ row, col }) => {
    const piece = board[row]?.[col];
    if (!piece) return;
    const empty = EIGHT_DIRECTIONS.map(([dr, dc]) => ({ row: row + dr, col: col + dc })).filter(
      ({ row: r, col: c }) => r >= 0 && r < board.length && c >= 0 && c < board[r].length && !board[r][c]
    );
    if (!empty.length) return;
    const dest = empty[Math.floor(rng() * empty.length)];
    board[row][col] = null;
    board[dest.row][dest.col] = piece;
  });
}

function compactBoard(board) {
  return board.map((row) => row.map((p) => (p ? { t: p.type, c: p.color } : null)));
}

// Only the fields nnue/encode.js's card-threat scoring actually needs
// (cardThreatScore/singleCardThreatValue read effect/stars/used/recovering,
// isWorkerTurnExclusiveCardBlocked reads effect-or-id) -- not the full card
// object shape, same spirit as compactBoard above.
function compactDeck(deckSlots) {
  const compact = (arr) => (arr || []).map((c) => ({
    id: c.id,
    effect: c.effect,
    stars: c.stars,
    used: Boolean(c.used),
    recovering: Boolean(c.recovering)
  }));
  return { white: compact(deckSlots?.white), black: compact(deckSlots?.black) };
}

// Simple mulberry32 PRNG seeded per-game so each worker/game gets an
// independent, reproducible-if-needed random stream instead of everyone
// sharing Math.random()'s global state across concurrent worker threads.
function makeRng(seed) {
  let a = seed >>> 0;
  return function () {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// Exploration: instead of always playing the search's own top choice, pick a
// random legal action some of the time so self-play games actually visit a
// wider variety of opening/early-middlegame positions than always-greedy
// play would (which collapses onto the same few lines).
//
// Changed 2026-09-05 to be opening-only (was 15%/6%/2% across the whole
// game): explorationTainted (see playOneGame) discards every position from
// the start of the game up through the LAST exploration ply, since a random
// move anywhere later can flip the final outcome independent of position
// quality. With exploration spread through the whole game, most games'
// last exploration ply landed late, so most of each game's positions were
// getting discarded -- diversity was being generated but never actually
// reaching training. Concentrating exploration in the opening keeps that
// diversity while leaving nearly the whole rest of each game clean and
// usable (the same tradeoff AlphaZero-style systems make by only injecting
// root/opening noise rather than randomizing throughout).
const EXPLORATION_OPENING_PLIES = 10;
function explorationChance(plyIndex) {
  return plyIndex < EXPLORATION_OPENING_PLIES ? 0.15 : 0;
}

function playOneGame({ searchDepth, searchTimeMs, maxPlies, seed, flexibleBudget = true, evalFnByColor = null, searchDepthByColor = null, limitsByColor = null, paramsByColor = null, handicap = 0 }) {
  const rng = makeRng(seed);
  const state = makeInitialState(rng);
  // handicap (matches only): remove N minor/major pieces (not queen/king) from a
  // seed-chosen side, using a SEPARATE rng so handicap 0 leaves the game stream
  // untouched. Colour-swapped pairs share the seed, so the same side is short in
  // both games of a pair and the comparison stays fair -- it just makes far fewer draws.
  if (handicap > 0) {
    const hrng = makeRng(seed + 99991);
    const shortColor = hrng() < 0.5 ? "white" : "black";
    for (let k = 0; k < handicap; k += 1) {
      const cells = [];
      state.board.forEach((row, r) => row.forEach((p, c) => { if (p && p.color === shortColor && ["knight", "bishop", "rook"].includes(p.type)) cells.push([r, c]); }));
      if (!cells.length) break;
      const [r, c] = cells[Math.floor(hrng() * cells.length)];
      state.board[r][c] = null;
    }
  }
  const record = [];
  let plies = 0;

  // The extracted engine.js has NO stalemate/threefold-repetition/50-move
  // draw logic at all (confirmed by grepping the whole file -- the only
  // "draw" outcomes in there are specific CARD effects, which self-play has
  // disabled). Left alone, self-play games essentially never end in a draw
  // and instead run out the clock at maxPlies as "unfinished", which was the
  // real cause of the earlier skewed win/loss-heavy outcome distribution.
  // Both real-chess draw rules below are implemented here, self-play-only,
  // without touching engine.js's actual rules.
  const positionCounts = new Map();
  let pliesSinceProgress = 0;
  let drawReason = null;

  function positionKey(board, turn) {
    let key = turn;
    // barricade ("바리케이드", 2026-09-13 batch) places colorless `wall`
    // pieces (piece.color is null/undefined, same as any other neutral
    // board object) -- guard against that instead of assuming every piece
    // has a two-letter color.
    for (const row of board) for (const piece of row) key += piece ? (piece.color ? piece.color[0] : "n") + piece.type : ".";
    return key;
  }

  while (state.mode === "play" && plies < maxPlies) {
    const color = state.turn;
    advanceSelfPlaySpecialState(state, rng, color);
    const actions = engine.generateActions(state, color);
    if (!actions.length) break;

    // Cheap "how good is the position before this move" reading (no
    // search), so we can record how much the move that actually gets played
    // shifted the evaluation -- see the `evalDelta` field below.
    const evalBefore = engine.evaluateState(state, color);

    let chosenAction;
    let searchScore;
    let completedDepth = null;
    if (rng() < explorationChance(plies)) {
      chosenAction = actions[Math.floor(rng() * actions.length)];
      searchScore = null; // no search was run for this ply, nothing meaningful to log here
    } else {
      // flexibleBudget: true ("우리 AI") -- with the original behavior, a
      // depth that didn't finish within searchTimeMs got discarded entirely
      // and fell back to a shallow safety-only heuristic pick (see
      // engine.js's searchBestAction comment). That was found to be almost
      // always what happened at self-play's tight time budget, so self-play
      // games were far shallower than intended. This lets a depth actually
      // finish (larger effective deadline) and keeps partial progress
      // instead of discarding it.
      // Scale the BASE time budget with root candidate count too (added
      // 2026-09-11), not just the after-the-fact retry below. A full 6-card
      // hand can noticeably more than double root candidate count (~20 ->
      // ~40 confirmed in an isolated benchmark this session), so a
      // card-heavy position is predictably more likely to need extra time,
      // not just occasionally unlucky -- better to give it up front than
      // rely on catching the failure after the fact every time. Modest and
      // capped so this only nudges card-heavy positions rather than
      // slowing every move down.
      const CANDIDATE_BASELINE = 20;
      const EXTRA_MS_PER_CANDIDATE = 1.5;
      const MAX_BASE_SEARCH_MS = searchTimeMs * 2.5;
      const adaptiveSearchTimeMs = Math.min(
        MAX_BASE_SEARCH_MS,
        searchTimeMs + Math.max(0, actions.length - CANDIDATE_BASELINE) * EXTRA_MS_PER_CANDIDATE
      );
      let result = engine.searchBestAction(state, actions, color, (searchDepthByColor && searchDepthByColor[color]) || searchDepth, adaptiveSearchTimeMs, { flexibleBudget, evalFn: (evalFnByColor && evalFnByColor[color]) || nnueEvalFn || void 0, limits: (limitsByColor && limitsByColor[color]) || void 0, params: (paramsByColor && paramsByColor[color]) || void 0 });
      // Adaptive retry (added 2026-09-11): completedDepth 0 means
      // searchAtDepth never finished even once within the budget, so
      // searchBestAction's returned action is really just
      // pickRootHardSafetyFallback's safe pick -- fine to play, but its
      // `score` is still the function's initial -INF sentinel (see
      // engine.js:2470), not a real evaluation. That's what was showing up
      // as evalDelta ~= -1e9 in the recorded data. More root candidates
      // (cards, plus this session's added per-card-candidate safety/combo
      // checks) make this timeout more likely than before. Rather than
      // giving every single move a bigger budget (throughput hit for the
      // ~85%+ of plies that already finish fine), only retry the ones that
      // actually failed, with more room -- keeps self-play fast on average
      // while still getting a real score for the plies that need it. One
      // retry only, no unbounded loop.
      if (!result.completedDepth) {
        result = engine.searchBestAction(state, actions, color, (searchDepthByColor && searchDepthByColor[color]) || searchDepth, adaptiveSearchTimeMs * 4, { flexibleBudget, evalFn: (evalFnByColor && evalFnByColor[color]) || nnueEvalFn || void 0, limits: (limitsByColor && limitsByColor[color]) || void 0, params: (paramsByColor && paramsByColor[color]) || void 0 });
      }
      if (!result.action) break;
      chosenAction = result.action;
      searchScore = result.score;
      completedDepth = result.completedDepth ?? null;
    }

    record.push({
      board: compactBoard(state.board),
      deckSlots: compactDeck(state.deckSlots),
      turn: color,
      searchScore,
      completedDepth,
      evalBefore: Math.round(evalBefore),
      // How much this specific move swung the position's evaluation, from
      // the mover's own perspective -- a cheap per-move "how impactful was
      // this" signal to sit alongside the outcome label, not a replacement
      // for it (search score is still not used as the training label, see
      // the `outcome` field below).
      evalDelta: searchScore === null ? null : Math.round(searchScore - evalBefore)
    });

    // "Progress" = a capture or a pawn move, same definition the 50-move
    // rule uses in real chess. Checked against the board BEFORE applying,
    // since that's when we can still see what was on the destination square.
    const moverPiece = chosenAction.type === "move" ? state.board[chosenAction.from.row]?.[chosenAction.from.col] : null;
    const destOccupied = chosenAction.type === "move" && Boolean(state.board[chosenAction.move.row]?.[chosenAction.move.col]);
    const isProgress = destOccupied || moverPiece?.type === "pawn";

    const applied = engine.applyAction(state, chosenAction, color);
    if (!applied.ok) break;
    plies += 1;

    // Decoupled from maxPlies (2026-09-07): this used to scale WITH
    // maxPlies (maxPlies * 0.6), so raising maxPlies to let genuinely still-
    // fighting games run longer also loosened this stagnation check by the
    // same amount -- defeating the point. A truly stalled game (no capture/
    // pawn move for a while) should get cut short at a fixed threshold
    // regardless of how generous the overall ply budget is; maxPlies itself
    // is now just a much higher outer safety net for games that are
    // actually still making progress (see its own comment where it's set).
    const STALL_PLIES_THRESHOLD = 30;
    pliesSinceProgress = isProgress ? 0 : pliesSinceProgress + 1;
    if (pliesSinceProgress >= STALL_PLIES_THRESHOLD) {
      drawReason = "50-move";
      break;
    }
    // Only check repetition when the turn actually passed (added
    // 2026-09-11): cards never end the turn (see childDepthAfterAction in
    // engine.js), so a card-only ply leaves board+turn completely
    // unchanged. positionKey hashes board+turn with no notion of card/deck
    // state, so 3 non-board-moving cards played back-to-back by the same
    // side used to hash to the identical key 3 times and get this falsely
    // declared a "repetition" draw after just a few plies -- nothing on
    // the board had actually repeated, the mover just used some cards.
    // Real position repetition is only meaningful at the point control
    // passes back and forth anyway, so gating on that is also the more
    // correct definition, not just a card-specific patch.
    if (state.turn !== color) {
      const key = positionKey(state.board, state.turn);
      const count = (positionCounts.get(key) || 0) + 1;
      positionCounts.set(key, count);
      if (count >= 3) {
        drawReason = "repetition";
        break;
      }
    }
  }
  const outcome = drawReason ? "draw" : state.mode === "gameover" ? state.winner || "draw" : "unfinished";
  // Label each position by the actual game outcome from its mover's
  // perspective (1 = that side went on to win, -1 = lost, 0 = draw/unfinished)
  // -- this is what a value net should learn from, rather than our own
  // (weak) search score, which would just teach it to imitate our biases.
  //
  // unfinished !== draw: a real draw (repetition/50-move) is a known,
  // meaningful outcome -- "unfinished" just means the ply cap was hit before
  // anything was decided, which tells you nothing about who was actually
  // better (the game could easily have been heading toward a win for either
  // side). Collapsing both to the same label=0 as if "unfinished" meant
  // "balanced" was quietly injecting label noise -- flag it separately so
  // training can drop these instead (see nnue/train.js).
  record.forEach((entry, i) => {
    entry.outcome = outcome === "draw" || outcome === "unfinished" ? 0 : outcome === entry.turn ? 1 : -1;
    entry.unfinished = outcome === "unfinished";
    // How many plies happened AFTER this position before the game actually
    // ended (0 = this was the last recorded position). Not every surviving
    // (untainted, finished) position is equally trustworthy -- one right
    // before checkmate is much more tightly linked to the outcome than one
    // from 40 plies earlier, where plenty of real (non-exploration) but
    // still search-quality-limited play could have changed the trajectory.
    // Recorded so training can weight positions by proximity to the result
    // instead of the current all-or-nothing include/exclude filtering.
    entry.pliesFromEnd = record.length - 1 - i;
  });
  // Mark positions whose outcome label is untrustworthy: if ANY move from
  // this position onward (including the move made right here) was a random
  // exploration pick rather than the search's own choice, the causal link
  // between "was this position good" and "did this side go on to win" is
  // broken -- a lucky/unlucky random move later in the game can flip the
  // result independent of position quality. Trained on unfiltered, this
  // shows up as the value net immediately overfitting to outcome noise
  // (val_loss got worse from epoch 0 even after shrinking the network and
  // adding L2 -- see the retraining discussion this was decided from).
  // Backward pass so the taint propagates from any exploration ply to every
  // earlier position in the same game.
  let tainted = false;
  for (let i = record.length - 1; i >= 0; i -= 1) {
    tainted = tainted || record[i].searchScore === null;
    record[i].explorationTainted = tainted;
  }
  return {
    plies,
    outcome,
    record,
    positions: record.length
  };
}

// parentPort is only non-null when actually running inside a real
// worker_threads Worker (normal self-play path, unchanged). When this file
// is `require()`'d directly instead (2026-09-18, for a head-to-head match
// script comparing two NNUE weight files), parentPort is null and this
// exports playOneGame/makeRng for reuse instead of executing as a worker --
// reuses the REAL game-setup/turn-loop logic (deck draft, special pieces,
// draw rules) rather than a second hand-written copy that could drift from it.
if (parentPort) {
  const startedAt = Date.now();
  const result = playOneGame(workerData);
  parentPort.postMessage({ ...result, ms: Date.now() - startedAt });
} else {
  module.exports = { playOneGame, makeRng };
}
