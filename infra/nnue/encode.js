// Feature encoding: one plane per (piece type x color), 64 squares each,
// from the mover's own perspective (their pieces always occupy the first
// half of the planes) so the net doesn't have to separately learn "white to
// move" vs "black to move" versions of the same idea.
//
// Piece list must match selfplay-worker.js's SELFPLAY_SPECIAL_TYPES exactly
// -- self-play only ever places these special types (plus the 6 standard
// ones), so anything else here would just be dead, always-zero input planes.
// If that list changes, update this one too.
//
// Updated 2026-09-06: selfplay-worker.js's list grew from 13 to 16 more
// types; added all of them here EXCEPT coffin/babyBear (owner asked to leave
// those two out for now and fold them in later -- they have no real
// movement of their own, only a self-play-only "hop to a random adjacent
// square" house rule, so how to represent them is still an open question).
// Boards containing coffin/babyBear still train fine meanwhile -- an
// unlisted type's square just stays all-zero, same as any other unknown type.
const STANDARD_TYPES = ["pawn", "knight", "bishop", "rook", "queen", "king"];
const SPECIAL_TYPES = [
  "amazon", "cardinal", "pegasus", "assassin", "dragon",
  "cannon", "grasshopper", "hook", "herald", "camel", "alfil", "ferz", "eagle",
  "berserker", "magicGirl", "windmill", "trickster", "merchant",
  "knightmaster", "standardBearer", "idol", "siren", "reaper", "recruiter",
  "guard", "colossus", "bigRook",
  // Added 2026-09-14 (engine-merged.js graft session): hedgehog/princess/
  // campfire joined SELFPLAY_SPECIAL_TYPES. coffin/babyBear are still
  // deliberately excluded (see the 2026-09-06 note above -- no real
  // movement of their own, only a self-play house rule).
  "hedgehog", "princess", "campfire",
  // Added 2026-09-15 (16-new-card patch graft): paladin/octopus/clockwork/
  // parrot joined SELFPLAY_SPECIAL_TYPES as the 4 PIECE-phase cards from
  // that batch (all single-square, all self-contained -- see
  // selfplay-worker-merged.js's SELFPLAY_SPECIAL_TYPES comment for why).
  "paladin", "octopus", "clockwork", "parrot"
];
const ALL_TYPES = [...STANDARD_TYPES, ...SPECIAL_TYPES];

const PIECE_INDEX = {};
ALL_TYPES.forEach((type, i) => { PIECE_INDEX[type] = i; });

const PLANE_COUNT = ALL_TYPES.length; // 33

// Replaced 2026-09-08: previously 3 hand-rolled scalar hints (material,
// king safety, special-piece-count), computed straight from the board with
// no engine dependency. Superseded by feeding the network the SAME ~21
// named sub-scores evaluateState() itself uses (via engine.js's
// evaluateStateComponents), not just a plain output-blend of the two models
// (that ensemble approach was tried and rejected -- see nnue/tune-eval.js
// and project memory, it was strictly worse at every blend weight because
// two models trained on the same noisy data make correlated errors).
// This is a different mechanism: the raw sub-scores go in as EXTRA INPUT
// features alongside the one-hot board planes, so the small 16/16-unit
// network doesn't have to spend its limited capacity re-deriving "count up
// material", "who's ahead on king safety", etc. from scratch before it can
// even start on subtler board patterns -- while still being trained
// end-to-end, so it can learn nonlinear interactions the hand-coded
// weighted sum (evaluateState) can't. tune-eval.js already confirmed on
// this exact data that a plain linear regression over just these 21
// features alone reaches 69.1% (vs the raw-board NNUE's 55.8%), which is
// the direct evidence this feature set carries a lot more signal than the
// tiny network was managing to pull out of the one-hot board on its own.
// Switched from engine.optimized.js 2026-09-14: that file is our hand-ported
// reimplementation and was found (via a card-by-card audit against the real
// site) to have real behavioral bugs in exactly the kind of card/piece logic
// evaluateStateComponents below reads (cardsSelf/cardsEnemy/specialSelf
// etc.). engine-merged.js is the authentic-site-rules + our-own-enhancements
// engine that replaced it for self-play data generation -- using it here too
// keeps encoding consistent with how the training data was actually
// generated. See audit-data/graft-progress.md for the full story.
const engine = require("../engine-merged.js");

const FEATURE_NAMES = [
  "material", "exchange",
  "positionSelf", "positionEnemy",
  "kingSafetySelf", "kingSafetyEnemy",
  "pressureSelf", "pressureEnemy",
  "specialSelf", "specialEnemy",
  "bloodMoonSelf", "bloodMoonEnemy",
  "cardsSelf", "cardsEnemy",
  "campaignSelf", "campaignEnemy",
  "ultimatum",
  "persistentObjectivesSelf", "persistentObjectivesEnemy",
  "bonus", "tacticalSafety"
];
const EXTRA_FEATURE_COUNT = FEATURE_NAMES.length; // 21

// Card pool one-hot (added 2026-09-12): cardsSelf/cardsEnemy above collapse
// an entire hand into one aggregate threat number -- the network has no way
// to tell "royalShield in hand" from "freeze in hand", only the combined
// level. One-hot presence (own hand / enemy hand) of each self-play
// card-pool effect gives it that per-card signal to actually learn from.
// Scoped to SELFPLAY_CARD_POOL, not the full 224-card catalog: self-play
// (the only data source right now) never draws outside this pool, so a slot
// for any other card would just always be zero and never get trained --
// expand this list only after SELFPLAY_CARD_POOL itself grows. MUST match
// selfplay-worker-merged.js's SELFPLAY_CARD_POOL exactly.
// Expanded 2026-09-14 from 18 -> 173 cards (the pool grew across several
// sessions of card-audit work; this encoding had fallen behind and was
// silently blind to every card added since the 2026-09-12 18-card list --
// INPUT_SIZE changes here, so any existing trained weights file's shape no
// longer matches and must be retrained from scratch, not warm-loaded as-is).
const CARD_POOL_TYPES = [
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
  "qxe1", "nullification", "recurrence", "outpost", "killerKing", "majesty",
  "overtake", "leap", "vanguard", "infiltration", "reversal", "lastStand",
  "fastGrowth", "earlyPromotion", "bribe", "conscription", "barricade",
  "collapse",
  // 2026-09-15 patch batch: all 16 new cards, matching
  // selfplay-worker-merged.js's SELFPLAY_CARD_POOL addition exactly.
  "highlander", "thief", "disassembly", "falseStart", "proficiency",
  "locustSwarm", "longEnPassant", "extinction", "symmetry", "brutus",
  "clockwork", "mutation", "parrot", "paladin", "octopus", "metal"
];
const CARD_POOL_INDEX = {};
CARD_POOL_TYPES.forEach((effect, i) => { CARD_POOL_INDEX[effect] = i; });
const CARD_ONEHOT_COUNT = CARD_POOL_TYPES.length * 2; // own + enemy

// Layout: [board planes][card one-hot][ply feature, opt-in][the 21 named
// features]. The named features stay LAST (not just appended after the
// board planes like before) specifically so train.js's `featureBase =
// INPUT_SIZE - ORIGINAL_EVAL_WEIGHTS.length` (used to warm-start the wide
// layer at evaluateState()'s own coefficients) keeps working unmodified --
// it assumes those 21 are the final 21 dimensions of the input, and this
// ordering keeps that true regardless of what gets inserted in the middle
// (new dims must go BEFORE this block, never after).
//
// PLY_FEATURE (2026-09-12 experiment, ABLATE_PLY_FEATURE=1 opt-in): none of
// the 21 named features expose game phase directly -- only indirectly via
// piece count. Recorded self-play entries don't carry an actual ply/move
// number field (selfplay-worker.js's `record.push` never wrote one, and
// retrofitting it would only cover NEW data, not the existing 113k-position
// set), so this uses normalized piece count (already available on every
// entry, old and new alike) as a game-phase proxy instead of a real ply
// count. Opt-in via env var so INPUT_SIZE -- and therefore every existing
// weights file's shape -- is unaffected unless explicitly requested.
const PLY_FEATURE_ENABLED = process.env.ABLATE_PLY_FEATURE === "1";
const PLY_FEATURE_COUNT = PLY_FEATURE_ENABLED ? 1 : 0;
const BOARD_SIZE = PLANE_COUNT * 2 * 64;
const INPUT_SIZE = BOARD_SIZE + CARD_ONEHOT_COUNT + PLY_FEATURE_COUNT + EXTRA_FEATURE_COUNT;

// A card counts as "present" if it's a live, usable instance (not already
// used/recovering) of a pool effect -- matches how the engine's own
// isWorkerTurnExclusiveCardBlocked-adjacent checks treat a card as "in hand"
// elsewhere. `color` is whichever side's hand to write (mover or opponent);
// caller picks the write offset so mover's own hand and the enemy's land in
// separate blocks.
function writeCardOneHot(input, offset, deckSlots, color) {
  const deck = deckSlots?.[color] || [];
  deck.forEach((card) => {
    if (!card || card.used || card.recovering) return;
    const idx = CARD_POOL_INDEX[card.effect];
    if (idx === undefined) return;
    input[offset + idx] = 1;
  });
}

// deckSlots (added 2026-09-11): pass-through of a real self-play deck when
// the entry has one, so cardsSelf/cardsEnemy (FEATURE_NAMES below) stop
// being permanently-zero dead inputs -- evaluateStateComponents' card
// scoring reads boardState.deckSlots directly, no aiSearchNoCards gating
// involved (that flag only affects move generation, not static eval), so
// nothing else here needs to change. Older callers/cached data with no
// deckSlots field fall back to the previous empty-deck behavior unchanged.
function makeState(board, deckSlots) {
  const state = engine.cloneState({});
  state.board = board.map((row) => row.map((p) => (p ? { type: p.t, color: p.c, moved: true } : null)));
  state.mode = "play";
  state.deckSlots = deckSlots ? { white: deckSlots.white || [], black: deckSlots.black || [] } : { white: [], black: [] };
  state.captures = { white: [], black: [] };
  state.aiSearchNoCards = true;
  engine.setWorkerBoardDimensions(state);
  return state;
}

// Returns null for terminal positions (checkmate/no-survival-piece etc.) --
// self-play only ever records positions mid-game, so this should be rare
// (~0.2% in practice, per tune-eval.js's count), but callers must skip
// nulls rather than feed a partial/garbage row into the model.
function encodeBoard(board, mover, deckSlots) {
  const c = engine.evaluateStateComponents(makeState(board, deckSlots), mover);
  if (c.terminal !== null) return null;

  const input = new Float32Array(INPUT_SIZE);
  for (let r = 0; r < 8; r++) {
    for (let col = 0; col < 8; col++) {
      const p = board[r][col];
      if (!p) continue;
      const typeIdx = PIECE_INDEX[p.t];
      if (typeIdx === undefined) continue; // unsupported piece type -> skip (still 0 there)
      const colorOffset = p.c === mover ? 0 : PLANE_COUNT;
      const square = r * 8 + col;
      input[(typeIdx + colorOffset) * 64 + square] = 1;
    }
  }
  const enemy = mover === "white" ? "black" : "white";
  writeCardOneHot(input, BOARD_SIZE, deckSlots, mover);
  writeCardOneHot(input, BOARD_SIZE + CARD_POOL_TYPES.length, deckSlots, enemy);

  if (PLY_FEATURE_ENABLED) {
    let pieceCount = 0;
    for (const row of board) for (const p of row) if (p) pieceCount += 1;
    // Normalized to [0,1], 32 pieces (game start) -> 1.0, fewer -> lower.
    input[BOARD_SIZE + CARD_ONEHOT_COUNT] = pieceCount / 32;
  }

  const base = INPUT_SIZE - EXTRA_FEATURE_COUNT;
  FEATURE_NAMES.forEach((name, i) => { input[base + i] = c[name]; });
  return input;
}

module.exports = { encodeBoard, INPUT_SIZE, PIECE_INDEX, ALL_TYPES, FEATURE_NAMES, CARD_POOL_TYPES };
