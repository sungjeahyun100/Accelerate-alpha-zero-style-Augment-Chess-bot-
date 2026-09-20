// Quick smoke test for engine-merged.js, following the loading convention
// documented in site-oracle/README.md and used by selfplay-worker-real.js.
globalThis.self = globalThis;
globalThis.addEventListener = () => {};
const engine = require("./engine-merged.js");

function makeStartingBoard() {
  const back = ["rook", "knight", "bishop", "queen", "king", "bishop", "knight", "rook"];
  const board = Array.from({ length: 8 }, () => Array(8).fill(null));
  for (const color of ["black", "white"]) {
    const backRow = color === "black" ? 0 : 7;
    const pawnRow = color === "black" ? 1 : 6;
    for (let c = 0; c < 8; c++) board[pawnRow][c] = { type: "pawn", color, moved: false };
    for (let c = 0; c < 8; c++) board[backRow][c] = { type: back[c], color, moved: false };
  }
  return board;
}

function makeInitialState(board) {
  const state = engine.cloneState({});
  state.board = board;
  state.mode = "play";
  state.turn = "white";
  state.actionsRemaining = 1;
  state.deckSlots = { white: [], black: [] };
  state.captures = { white: [], black: [] };
  engine.setWorkerBoardDimensions(state);
  return state;
}

let failures = 0;
function check(name, cond) {
  if (cond) {
    console.log(`PASS: ${name}`);
  } else {
    console.log(`FAIL: ${name}`);
    failures += 1;
  }
}

// 1) Standard starting position -> exactly 20 actions.
const startState = makeInitialState(makeStartingBoard());
const startActions = engine.generateActions(startState, "white");
check(`generateActions on start position returns 20 (got ${startActions.length})`, startActions.length === 20);

// 2) searchBestAction completes without crashing on a plain position.
try {
  const result = engine.searchBestAction(startState, startActions, "white", 3, 500);
  check("searchBestAction completes on start position", Boolean(result && result.action));
} catch (e) {
  console.log("FAIL: searchBestAction threw on start position:", e.stack);
  failures += 1;
}

// 3) Boards with special pieces / cards from this session's additions.
function scenario(name, setup) {
  try {
    const board = Array.from({ length: 8 }, () => Array(8).fill(null));
    board[7][4] = { type: "king", color: "white", moved: false };
    board[0][4] = { type: "king", color: "black", moved: false };
    setup(board);
    const state = makeInitialState(board);
    const actions = engine.generateActions(state, "white");
    const result = engine.searchBestAction(state, actions, "white", 2, 300);
    check(`${name}: generateActions+searchBestAction survive`, true);
  } catch (e) {
    console.log(`FAIL: ${name} threw:`, e.stack);
    failures += 1;
  }
}

scenario("bigBishop", (board) => {
  board[4][4] = { type: "bigBishop", color: "white", moved: false, anchorRow: 4, anchorCol: 4, hp: 2, maxHp: 2 };
  board[4][5] = board[4][4];
  board[5][4] = board[4][4];
  board[5][5] = board[4][4];
});

scenario("campfire", (board) => {
  board[3][3] = { type: "campfire", color: "white", moved: false };
});

scenario("princess", (board) => {
  board[3][3] = { type: "princess", color: "white", moved: false };
});

scenario("bribe (knight present, card played)", (board) => {
  board[3][3] = { type: "knight", color: "white", moved: false };
});

scenario("outpost (piece present)", (board) => {
  board[3][3] = { type: "rook", color: "white", moved: false };
});

scenario("witchTrial (piece marked)", (board) => {
  board[3][3] = { type: "rook", color: "black", moved: false, witchTrial: { by: "white", remaining: 1 } };
});

scenario("infiltration (submerged flag set)", (board) => {
  board[3][3] = { type: "rook", color: "white", moved: false, submerged: true };
});

scenario("recurrence (pendingRecurrences present)", (board) => {
  board[3][3] = { type: "rook", color: "white", moved: false };
});

console.log(failures === 0 ? "\nALL SMOKE CHECKS PASSED" : `\n${failures} SMOKE CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
