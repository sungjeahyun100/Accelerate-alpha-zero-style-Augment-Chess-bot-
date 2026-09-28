"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const { FrozenClientSource, GameAdapter } = require("../../../packages/game-adapter/src");
const { OracleRuntime } = require("../../../packages/game-adapter/src/game-adapter");
const { createRuntimeContract } = require("../../../bridge/tools/runtime-contract");

const LATEST_SHA = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
const root = process.env.ACCELERATE_SITE_BASELINE_LATEST || process.env.ACCELERATE_SITE_BASELINE ||
  path.join(process.env.RUNNER_TEMP || process.env.APPDATA || "", "Accelerate", "cache", "site-baseline-20260928-e5ed84fc");
if (!path.isAbsolute(root)) throw new Error("Set ACCELERATE_SITE_BASELINE_LATEST to the pinned external client directory.");
const contract = createRuntimeContract({ baseline: "site-20260928" });
const source = new FrozenClientSource(root, { expectedClientSha256: LATEST_SHA });
const makeAdapter = options => new GameAdapter({ source, contract, ...options });
const makeDirect = options => new OracleRuntime({ source, contract, ...options });
const copy = value => JSON.parse(JSON.stringify(value));

test("latest source identity is immutable after verification", () => {
  const main = source.manifest.files.find(file => file.name.startsWith("main-"));
  assert.equal(main.sha256, LATEST_SHA);
  assert.equal(contract.ORACLE_PROFILE_VERSION, "accelerate-headless-semantic-v7");
  assert.throws(() => { source.root = path.dirname(root); }, TypeError);
  assert.throws(() => { source.manifest = { ...source.manifest, files: [] }; }, TypeError);
  assert.throws(() => { main.sha256 = "0".repeat(64); }, TypeError);
  assert.equal(source.createRuntime().evaluate("typeof createInitialBoard"), "function");
});

for (const style of ["normal", "chaos", "grand"]) {
  test(`${style}: source initialization, draft, observation and 8x8 transition`, () => {
    const adapter = makeAdapter();
    let position = adapter.newGame({ gameStyle: style }, 12345);
    const direct = makeDirect();
    direct.random = contract.rng(12345);
    direct.main.context.__style = style;
    direct.evaluate("selectedGameStyle=__style;localPlayMode='local';playMode='local';draftDeleteEnabled=false;ruleOpeningEnabled=true;ruleSelectionEnabled=false;selectedRuleCardIds=[];deathmatchEnabled=true;resetGame(false,[]);beginInitialGameFlow();");
    assert.deepEqual(position.state, direct.snapshot().state, "source initial state");
    assert.deepEqual(position.rng, direct.random, "source RNG cursor and tape");
    let selections = 0;
    while (position.state.mode === "draft" && selections < 32) {
      const choices = adapter.actions(position);
      assert.ok(choices.length > 0, "draft has a source selection");
      const step = adapter.apply(position, choices[0], { recordHistory: false });
      assert.equal(step.ok, true, step.error?.message);
      position = step.position;
      selections++;
    }
    assert.equal(position.state.mode, "play", "draft must finish within the explicit bound");
    assert.equal(position.state.board.length, 8);
    assert.ok(position.state.board.every(row => row.length === 8));
    for (const viewer of ["white", "black"]) {
      const observation = adapter.observe(position, viewer);
      contract.validateObservation(observation);
      assert.equal(observation.publicState.rulesVersion, position.rulesVersion);
      assert.equal(observation.publicState.catalogVersion, position.catalogVersion);
      assert.ok(adapter.publicHints(position, viewer));
    }
    const legal = adapter.actions(position).find(action => action.payload.type === "move");
    assert.ok(legal, "source provides a legal board transition");
    direct.restore(position);
    direct.main.context.__action = copy(legal.payload);
    const raw = direct.evaluate("applyAiAction(__action)");
    assert.equal(raw.ok, true, raw.message);
    const expected = direct.snapshot();
    const actual = adapter.apply(position, legal, { recordHistory: false });
    assert.equal(actual.ok, true, actual.error?.message);
    assert.deepEqual(actual.position.state, expected.state, "full post-transition source state");
    assert.deepEqual(actual.position.rng, expected.rng, "source RNG after transition");
    assert.equal(actual.result.status, adapter.result(actual.position).status);
    adapter.dispose();
  });
}

test("RULE setup and invalid actions preserve input and recover the adapter VM", () => {
  const adapter = makeAdapter();
  const position = adapter.newGame({ gameStyle: "normal", draftDelete: true, ruleCardIds: ["acceleration"] }, 37);
  assert.equal(position.state.mode, "play");
  assert.ok([position.state.appliedRuleCard?.id, ...(position.state.additionalRuleCards || []).map(card => card.id)].includes("acceleration"));
  const unchanged = JSON.stringify(position);
  const legal = adapter.actions(position).find(action => action.payload.type === "move");
  assert.ok(legal);
  const rejected = adapter.apply(position, contract.action(position, { ...legal.payload, color: "black" }));
  assert.equal(rejected.ok, false);
  assert.equal(rejected.position.positionId, position.positionId);
  assert.match(rejected.error.message, /Wrong acting player/);
  const actual = adapter.apply(position, legal, { recordHistory: false });
  const fresh = makeAdapter().apply(position, legal, { recordHistory: false });
  assert.equal(actual.ok, true);
  assert.deepEqual(actual.position, fresh.position, "rejected apply cannot contaminate later transitions");
  assert.equal(JSON.stringify(position), unchanged, "caller position is immutable");
  assert.throws(() => adapter.apply(actual.position, legal), /Stale or incompatible action/i, "stale action must fail closed");
  assert.deepEqual(adapter.observe(position, "white"), makeAdapter().observe(position, "white"));
  adapter.dispose();
});

test("unknown source state is named in the visibility failure and later calls recover", () => {
  const adapter = makeAdapter();
  const position = adapter.newGame({ draftDelete: true }, 7);
  const state = contract.jsonCopy(position.state);
  state.unreviewedPublicLeakProbe = { secret: "test-only" };
  const unknown = contract.position(state, position.rng);
  assert.throws(() => adapter.observe(unknown, "white"), /Unclassified site state fields.*unreviewedPublicLeakProbe/);
  contract.validateObservation(adapter.observe(position, "white"));
  adapter.dispose();
});

test("independent cursors retain page order across observation and rejected apply", () => {
  const adapter = makeAdapter();
  const position = adapter.newGame({ draftDelete: true }, 22);
  const expected = adapter.actions(position).map(action => action.actionId);
  const first = adapter.actionStream(position);
  const second = adapter.actionStream(position);
  const firstPage = first.nextPage(3);
  adapter.observe(position, "black");
  const wrong = contract.action(position, { ...adapter.actions(position)[0].payload, color: "black" });
  assert.equal(adapter.apply(position, wrong).ok, false);
  const secondPage = second.nextPage(5);
  const nextPage = first.nextPage(2);
  assert.deepEqual([...firstPage.actions, ...nextPage.actions].map(action => action.actionId), expected.slice(0, 5));
  assert.deepEqual(secondPage.actions.map(action => action.actionId), expected.slice(0, 5));
  assert.ok([firstPage, secondPage, nextPage].every(page => page.examined <= 4096));
  first.dispose(); second.dispose();
  assert.throws(() => first.nextPage(), /disposed/);
  adapter.dispose();
});

test("custom microtask limit remains effective after staged newGame", () => {
  const direct = makeDirect({ maxMicrotasks: 1 });
  direct.newGame({ draftDelete: true }, 19);
  assert.equal(direct.main.context.__maxMicrotasks, 1);
  assert.throws(() => direct.evaluate("queueMicrotask(()=>{});queueMicrotask(()=>{})"), /microtask budget exceeded/);
});



test("public call sequence does not contaminate a later grand card transition", () => {
  const producer = makeAdapter();
  let position = producer.newGame({ gameStyle: "grand" }, 12345);
  for (let choices = 0; position.state.mode === "draft" && choices < 32; choices++) {
    const step = producer.apply(position, producer.actions(position)[0], { recordHistory: false });
    assert.equal(step.ok, true);
    position = step.position;
  }
  assert.equal(position.state.mode, "play");
  const fixture = makeDirect();
  fixture.restore(position);
  fixture.evaluate("{const card=CARD_DEFS.find(item=>item.id==='summon-colossus');state.deckSlots.white=[cloneCard(card),null,null];state.deckSlots.black=[null,null,null];state.deck.white=state.deckSlots.white.filter(Boolean);state.deck.black=[];state.playerCards=state.deckSlots;}");
  const synthetic = fixture.snapshot();
  const fresh = makeAdapter();
  const cardAction = fresh.actions(synthetic).find(action => action.payload.cardId === "summon-colossus");
  assert.ok(cardAction, "source offers the card on the constructed grand board");
  const expected = fresh.apply(synthetic, cardAction, { recordHistory: false });
  assert.equal(expected.ok, true);
  const reused = makeAdapter();
  reused.actions(synthetic);
  reused.observe(synthetic, "white");
  reused.publicHints(synthetic, "black");
  reused.result(synthetic);
  const wrong = contract.action(synthetic, { ...cardAction.payload, color: "black" });
  assert.equal(reused.apply(synthetic, wrong).ok, false);
  assert.deepEqual(reused.apply(synthetic, cardAction, { recordHistory: false }).position, expected.position);
  reused.observe(expected.position, "black");
  reused.actions(expected.position);
  assert.deepEqual(reused.apply(synthetic, cardAction, { recordHistory: false }).position, expected.position);
  producer.dispose(); fresh.dispose(); reused.dispose();
});

for (const scenario of [
  {
    name: "royal capture terminates the game",
    setup: "state.board=Array.from({length:8},()=>Array(8).fill(null));state.board[7][4]=piece('white','king');state.board[0][4]=piece('black','king');state.board[1][4]=piece('white','rook');",
    from: { row: 1, col: 4 }, to: { row: 0, col: 4 },
    check(step) {
      assert.equal(step.result.status, "terminal");
      assert.equal(step.result.winner, "white");
      assert.match(step.result.reason, /킹/);
    },
  },
  {
    name: "opponent immobility terminates after a real turn transition",
    setup: "state.board=Array.from({length:8},()=>Array(8).fill(null));state.board[7][7]=piece('white','king');state.board[6][7]=piece('white','rook');state.board[0][0]=piece('black','king');for(const [r,c] of [[0,1],[1,0],[1,1]])state.board[r][c]=piece('neutral','wall');",
    from: { row: 6, col: 7 }, to: { row: 5, col: 7 },
    check(step) {
      assert.equal(step.position.state.turn, "black");
      assert.equal(step.result.status, "terminal");
      assert.equal(step.result.winner, "white");
      assert.match(step.result.reason, /움직일 수 있는 기물/);
    },
  },
  {
    name: "colossus HP attack damages without moving the attacker",
    setup: "state.board=Array.from({length:8},()=>Array(8).fill(null));state.board[7][4]=piece('white','king');state.board[0][4]=piece('black','king');placeColossus(piece('black','colossus'),3,3);state.board[3][0]=piece('white','rook');",
    from: { row: 3, col: 0 }, to: { row: 3, col: 3 },
    check(step) {
      assert.equal(step.result.status, "ongoing");
      assert.equal(step.position.state.board[3][3].hp, 2);
      assert.equal(step.position.state.board[3][0].type, "rook");
    },
  },
]) {
  test(scenario.name + " matches a direct client transition", () => {
    const fixture = makeDirect();
    fixture.newGame({ draftDelete: true }, 7);
    fixture.evaluate(scenario.setup);
    const position = fixture.snapshot();
    const adapter = makeAdapter();
    const action = adapter.actions(position).find(candidate => {
      const payload = candidate.payload;
      return payload.type === "move" && payload.from.row === scenario.from.row &&
        payload.from.col === scenario.from.col && payload.move.row === scenario.to.row &&
        payload.move.col === scenario.to.col;
    });
    assert.ok(action, "the source exposes the transition as a legal action");
    const direct = makeDirect();
    direct.restore(position);
    direct.main.context.__action = copy(action.payload);
    const raw = direct.evaluate("applyAiAction(__action)");
    assert.equal(raw.ok, true, raw.message);
    const expected = direct.snapshot();
    const step = adapter.apply(position, action, { recordHistory: false });
    assert.equal(step.ok, true, step.error?.message);
    assert.deepEqual(step.position.state, expected.state);
    assert.deepEqual(step.position.rng, expected.rng);
    scenario.check(step);
    adapter.dispose();
  });
}

test("threefold repetition reaches the source's star-equality draw through real moves", () => {
  const fixture = makeDirect();
  fixture.newGame({ draftDelete: true }, 7);
  fixture.evaluate("state.board=Array.from({length:8},()=>Array(8).fill(null));state.board[7][7]=piece('white','king');state.board[0][0]=piece('black','king');state.positionCounts=new Map();");
  let position = fixture.snapshot();
  const adapter = makeAdapter();
  const path = [
    [{ row: 7, col: 7 }, { row: 7, col: 6 }],
    [{ row: 0, col: 0 }, { row: 0, col: 1 }],
    [{ row: 7, col: 6 }, { row: 7, col: 7 }],
    [{ row: 0, col: 1 }, { row: 0, col: 0 }],
  ];
  for (let index = 0; index < 9; index++) {
    const [from, to] = path[index % path.length];
    const action = adapter.actions(position).find(candidate => {
      const payload = candidate.payload;
      return payload.type === "move" && payload.from.row === from.row &&
        payload.from.col === from.col && payload.move.row === to.row &&
        payload.move.col === to.col;
    });
    assert.ok(action, "source must offer move " + index);
    const direct = makeDirect();
    direct.restore(position);
    direct.main.context.__action = copy(action.payload);
    const raw = direct.evaluate("applyAiAction(__action)");
    assert.equal(raw.ok, true);
    const expected = direct.snapshot();
    const step = adapter.apply(position, action, { recordHistory: false });
    assert.equal(step.ok, true);
    assert.deepEqual(step.position.state, expected.state);
    assert.deepEqual(step.position.rng, expected.rng);
    if (index < 8) assert.equal(step.result.status, "ongoing");
    position = step.position;
  }
  const result = adapter.result(position);
  assert.equal(result.status, "terminal");
  assert.equal(result.outcome, "draw");
  assert.equal(result.winner, null);
  assert.match(result.reason, /3회 동형반복/);
  adapter.dispose();
});

test("shortened overtime limits exercise the source's no-progress star result", () => {
  const adapter = makeAdapter();
  const defaults = adapter.newGame({ draftDelete: true }, 7);
  assert.equal(defaults.state.starWinLimit, 45);
  assert.equal(defaults.state.deathmatchLimitTurns, 10);
  const fixture = makeDirect();
  fixture.newGame({ draftDelete: true, starWinLimit: 1, deathmatchLimitTurns: 1 }, 7);
  fixture.evaluate("state.board=Array.from({length:8},()=>Array(8).fill(null));state.board[7][7]=piece('white','king');state.board[0][0]=piece('black','king');state.positionCounts=new Map();");
  let position = fixture.snapshot();
  const path = [
    [{ row: 7, col: 7 }, { row: 7, col: 6 }],
    [{ row: 0, col: 0 }, { row: 0, col: 1 }],
    [{ row: 7, col: 6 }, { row: 6, col: 6 }],
    [{ row: 0, col: 1 }, { row: 1, col: 1 }],
  ];
  for (let index = 0; index < path.length; index++) {
    const [from, to] = path[index];
    const action = adapter.actions(position).find(candidate => {
      const payload = candidate.payload;
      return payload.type === "move" && payload.from.row === from.row &&
        payload.from.col === from.col && payload.move.row === to.row &&
        payload.move.col === to.col;
    });
    assert.ok(action);
    const direct = makeDirect();
    direct.restore(position);
    direct.main.context.__action = copy(action.payload);
    const raw = direct.evaluate("applyAiAction(__action)");
    assert.equal(raw.ok, true);
    const expected = direct.snapshot();
    const step = adapter.apply(position, action, { recordHistory: false });
    assert.equal(step.ok, true);
    assert.deepEqual(step.position.state, expected.state);
    assert.deepEqual(step.position.rng, expected.rng);
    if (index === 1) assert.equal(step.position.state.deathmatch.active, true);
    if (index < 3) assert.equal(step.result.status, "ongoing");
    position = step.position;
  }
  const result = adapter.result(position);
  assert.equal(result.status, "terminal");
  assert.equal(result.outcome, "draw");
  assert.match(result.reason, /별 합계가 같아 무승부/);
  adapter.dispose();
});

test("explicit unavailable RULE fails with its ID and leaves newGame reusable", () => {
  const adapter = makeAdapter();
  const before = adapter.newGame({ draftDelete: true }, 29);
  assert.throws(
    () => adapter.newGame({ draftDelete: true, ruleCardIds: ["capture-the-flag"] }, 29),
    /RULE cards unavailable in pinned source pool: capture-the-flag/
  );
  const after = adapter.newGame({ draftDelete: true }, 29);
  assert.deepEqual(after, before);
  const legal = adapter.actions(after)[0];
  assert.ok(legal);
  assert.equal(adapter.apply(after, legal, { recordHistory: false }).ok, true);
  adapter.dispose();
});

for (const scenario of [
  { id: "frenzy", setup: "" },
  { id: "log", setup: "" },
  { id: "holdout", setup: "" },
  { id: "exile", setup: "state.board[1][4]=null;state.board[2][4]=piece('black','pawn');state.board[2][4].origin='e7';state.board[2][4].moved=true;" },
  { id: "othello", setup: "state.board[4][2]=piece('white','rook');state.board[4][3]=piece('black','pawn');state.board[4][4]=piece('white','rook');" },
  { id: "judgment", setup: "state.board[7][0].totalCaptures=3;" },
  { id: "emergency-evacuation", setup: "state.board[5][0]=piece('white','rook');state.board[6][0]=null;" },
  { id: "homecoming", setup: "const item=state.board[7][0];state.board[7][0]=null;state.board[5][0]=item;item.moved=true;" },
  { id: "outpost", setup: "const item=state.board[7][1];state.board[7][1]=null;state.board[2][1]=item;item.moved=true;" },
  { id: "miracle", setup: "const item=state.board[7][2];state.board[7][2]=null;state.board[4][2]=item;item.moved=true;state.board[3][3]=piece('black','pawn');" },
  { id: "necromancy", setup: "state.captures.black=[piece('white','rook')];" },
  { id: "joker", setup: "const used=cloneCard(CARD_DEFS.find(x=>x.id==='freeze'));used.used=true;state.deckSlots.white[1]=used;state.deck.white=state.deckSlots.white.filter(Boolean);" },
]) {
test("card " + scenario.id + " has a source-enumerated target under its synthetic precondition", () => {
    const producer = makeAdapter();
    let base = producer.newGame({ gameStyle: "normal" }, 12345);
    for (let choices = 0; base.state.mode === "draft" && choices < 32; choices++) {
      const offered = producer.actions(base);
      const preferred = base.state.draft.color === "white" ? 1 : 0;
      assert.ok(offered[preferred], "preferred source draft choice must exist");
      const step = producer.apply(base, offered[preferred], { recordHistory: false });
      assert.equal(step.ok, true);
      base = step.position;
    }
    assert.equal(base.state.mode, "play");
    assert.ok(base.state.deckSlots.white.some(card => card?.id === "suicide-bomber"),
      "second white offer preserves pawns instead of selecting last-stand");
    const fixture = makeDirect();
    fixture.restore(base);
    fixture.main.context.__cardId = scenario.id;
    fixture.evaluate("{const card=CARD_DEFS.find(item=>item.id===__cardId);state.deckSlots.white=[cloneCard(card),null,null];state.deckSlots.black=[null,null,null];state.deck.white=state.deckSlots.white.filter(Boolean);state.deck.black=[];state.playerCards=state.deckSlots;" + scenario.setup + "}");
    const position = fixture.snapshot();
    const adapter = makeAdapter();
    const cursor = adapter.actionStream(position, { cardId: scenario.id, legal: false });
    const page = cursor.nextPage(1, { maxExamined: 4096 });
    cursor.dispose();
    assert.ok(page.actions.length, "source offers a target under the stated precondition");
    const action = page.actions[0];
    const direct = makeDirect();
    direct.restore(position);
    direct.main.context.__action = copy(action.payload);
    const raw = direct.evaluate("applyAiAction(__action)");
    assert.equal(raw.ok, true, raw.message);
    const expected = direct.snapshot();
    const step = adapter.apply(position, action, { recordHistory: false });
    assert.equal(step.ok, true, step.error?.message);
    assert.deepEqual(step.position.state, expected.state);
    assert.deepEqual(step.position.rng, expected.rng);
    for (const viewer of ["white", "black"]) {
      const observation = adapter.observe(step.position, viewer);
      contract.validateObservation(observation);
      if (scenario.id === "othello") {
        assert.equal(step.position.state.othelloPending.white, true);
        assert.equal(Object.hasOwn(observation.publicState, "othelloPending"), false);
        assert.equal(observation.board[4][3].color, "white");
        const shownCards = viewer === "white"
          ? observation.ownCards : observation.publicState.revealedOpponentCards;
        assert.equal(shownCards.find(card => card.id === "othello")?.used, true);
      }
    }
    producer.dispose(); adapter.dispose();
  });
}
