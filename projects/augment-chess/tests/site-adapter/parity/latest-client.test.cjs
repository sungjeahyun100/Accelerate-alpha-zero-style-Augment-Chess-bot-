"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const fs = require("node:fs");
const { FrozenClientSource, GameAdapter } = require("../../../oracle/game-adapter/src");
const { OracleRuntime } = require("../../../oracle/game-adapter/src/game-adapter");
const { retainedNodes, executionProfileForSha, metadataDigest } = require("../../../oracle/game-adapter/src/reviewed-initializers");
const { createRuntimeContract } = require("../../../contracts/tools/runtime-contract");

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
  assert.equal(source.executionProfile.profileVersion, "accelerate-headless-semantic-v7-faithful-init-v1");
  assert.equal(contract.ORACLE_PROFILE_VERSION, source.executionProfile.profileVersion);
  assert.equal(source.executionProfileSha256, metadataDigest(source.executionProfile));
  assert.throws(() => { source.root = path.dirname(root); }, TypeError);
  assert.throws(() => { source.manifest = { ...source.manifest, files: [] }; }, TypeError);
  assert.throws(() => { main.sha256 = "0".repeat(64); }, TypeError);
  assert.throws(() => { source.executionProfile.replayMetadata.labels.king = "changed"; }, TypeError);
  assert.equal(source.createRuntime().evaluate("typeof createInitialBoard"), "function");
});

test("faithful initialization preserves source labels and campaign metadata before explicit newGame", () => {
  const runtime = source.createRuntime();
  const actual = JSON.parse(runtime.evaluate("JSON.stringify({frameKeys:REPLAY_FRAME_KEYS,codes:PIECE_NOTATION_CODES,labels:TYPE_LABELS})"));
  assert.deepEqual(actual, source.executionProfile.replayMetadata);
  assert.equal(actual.frameKeys.length, 222);
  assert.equal(Object.keys(actual.labels).length, 79);
  assert.equal(actual.codes.medium, "GR");
  assert.equal(runtime.evaluate("typeof state"), "undefined", "browser resetGame startup ran before deterministic host setup");
  const campaigns = JSON.parse(runtime.evaluate("JSON.stringify(CAMPAIGN_DEFS.map(({id,name})=>({id,name})))"));
  assert.ok(campaigns.some(campaign => campaign.id === "janggi"));
  assert.ok(campaigns.some(campaign => campaign.id === "knight-game"));
  assert.equal(campaigns.find(campaign => campaign.id === "knight-journey").name, "미친 기사의 여행");
  assert.equal(runtime.evaluate("Object.isFrozen(CARDS$1)"), true);
  assert.equal(runtime.executionProfileSha256, source.executionProfileSha256);
});

test("reviewed initializer partition rejects missing dependencies and changed AST boundaries", () => {
  const raw = fs.readFileSync(path.join(root, "main-OahWs0tU.js"), "utf8");
  const acorn = require(path.join(root, "acorn-8.15.0.js"));
  const ast = acorn.parse(raw, { ecmaVersion: "latest", sourceType: "module" });
  const selected = retainedNodes(ast, LATEST_SHA, raw);
  assert.equal(selected.length, 7260 + 175);
  assert.ok(selected.every((node, index) => index === 0 || node.start > selected[index - 1].start));
  const excluded = new Set(source.executionProfile.excludedInitializers.map(node => node.start));
  assert.ok(selected.every(node => !excluded.has(node.start)));
  const helper = source.executionProfile.pureHelperDeclarations.find(node => node.name === "internalBalanceCard");
  const missing = { ...ast, body: ast.body.filter(node => node.start !== helper.start) };
  assert.throws(() => retainedNodes(missing, LATEST_SHA, raw), /declaration integrity changed.*dependencies/);
  const initializer = source.executionProfile.initializers.find(node => node.category === "piece-label-initialization");
  const changed = { ...ast, body: ast.body.map(node => node.start === initializer.start ? { ...node, end: node.end - 1 } : node) };
  assert.throws(() => retainedNodes(changed, LATEST_SHA, raw), /Reviewed frozen client initializer changed/);
  assert.throws(() => retainedNodes(ast, LATEST_SHA, raw + "\n"), /source SHA-256 mismatch/);
  assert.throws(() => executionProfileForSha("0".repeat(64)), /No reviewed headless initialization profile/);

  const legacySha = source.executionProfile.legacyCompatibility.sourceV6Sha256;
  assert.equal(executionProfileForSha(legacySha).profileVersion, "accelerate-headless-semantic-v6");
  assert.equal(executionProfileForSha(legacySha).initializers.length, 0);
  assert.ok(retainedNodes(ast, legacySha).every(node => ["FunctionDeclaration", "VariableDeclaration", "ClassDeclaration"].includes(node.type)));
});

test("old initialization profile selectors and changed bootstrap scripts fail explicitly", () => {
  assert.throws(() => new FrozenClientSource(root, {
    expectedClientSha256: LATEST_SHA, expectedExecutionProfileVersion: "accelerate-headless-semantic-v7",
  }), /execution profile mismatch.*accelerate-headless-semantic-v7-faithful-init-v1/);
  assert.throws(() => source.assertBootstrapScript("aiSimulationDepth = 0;"), /headless bootstrap mismatch.*expected.*received/);
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

test("reversal projects source activation, movement and expiry for both viewers in every style", () => {
  for (const style of ["normal", "chaos", "grand"]) {
    const direct = makeDirect();
    direct.newGame({ gameStyle: style, draftDelete: true }, 19);
    direct.evaluate(`
      state.mode='play';state.turn='white';state.board=Array.from({length:8},()=>Array(8).fill(null));
      state.board[7][7]=piece('white','king');state.board[0][7]=piece('black','king');
      state.board[4][3]=piece('white','rook');state.board[5][3]=piece('white','bishop');
      state.board[6][1]=piece('white','knight');state.positionCounts=new Map();
      state.deckSlots.white=[cloneCard(CARD_DEFS.find(card=>card.id==='reversal')),null,null];
      state.deckSlots.black=[null,null,null];state.deck.white=state.deckSlots.white.filter(Boolean);
      state.deck.black=[];state.playerCards=state.deckSlots;
    `);
    const before = direct.snapshot();
    const saved = JSON.stringify(before);
    const adapter = makeAdapter();
    try {
      const action = contract.action(before, { type: "card", color: "white", cardId: "reversal",
        cardInstanceId: before.state.deckSlots.white[0].instanceId, target: { row: 6, col: 1 } });
      const step = adapter.apply(before, action, { recordHistory: false });
      assert.equal(step.ok, true, `${style}: ${step.error?.message}`);
      assert.deepEqual(step.position.state.reversal, { white: true, black: false });
      assert.equal(step.position.state.board[6][1], null, "the source sacrifices the selected minor");
      assert.equal(JSON.stringify(before), saved, "activation preserves the caller's snapshot");
      for (const viewer of ["white", "black"]) {
        const observation = adapter.observe(step.position, viewer);
        contract.validateObservation(observation);
        assert.deepEqual(observation.publicState.reversal, { white: true, black: false });
        assert.notStrictEqual(observation.publicState.reversal, step.position.state.reversal);
        assert.ok(Object.isFrozen(observation.publicState.reversal));
      }
      const hints = adapter.publicHints(step.position, "white");
      const destinations = (row, col) => hints.moves.find(move => move.from.row === row && move.from.col === col)?.destinations || [];
      assert.ok(destinations(4, 3).some(square => square.row === 3 && square.col === 2), "active rook uses source bishop directions");
      assert.ok(destinations(5, 3).some(square => square.row === 5 && square.col === 2), "active bishop uses source rook directions");
      const unexpected = copy(step.position.state);
      unexpected.reversal.futureChoice = "unreviewed";
      assert.throws(() => adapter.observe(contract.position(unexpected, step.position.rng), "white"),
        /Invalid source public value.*reversal.*futureChoice/);
      const invalidFlag = copy(step.position.state);
      invalidFlag.reversal.white = "true";
      assert.throws(() => adapter.observe(contract.position(invalidFlag, step.position.rng), "white"),
        /Invalid source public value.*reversal\.white/);
      direct.restore(step.position);
      direct.evaluate("completeTurnAfterMove('white')");
      const expired = direct.snapshot();
      assert.deepEqual(expired.state.reversal, { white: false, black: false });
      for (const viewer of ["white", "black"]) {
        assert.deepEqual(adapter.observe(expired, viewer).publicState.reversal, { white: false, black: false });
      }
    } finally {
      adapter.dispose();
    }
  }
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

function emptyPortalPolicyFixture(options = {}) {
  const direct = makeDirect(options);
  direct.newGame({ gameStyle: "normal" }, 19);
  direct.evaluate(`
    state.mode='play';state.turn='white';state.board=Array.from({length:8},()=>Array(8).fill(null));
    state.deckSlots.white=[{...CARD_DEFS.find(card=>card.id==='portal-gun'),instanceId:'policy-portal'},null,null];
    state.deckSlots.black=[null,null,null];state.deck.white=state.deckSlots.white.filter(Boolean);state.deck.black=[];
    state.playerCards=state.deckSlots;
  `);
  return { direct, position: direct.snapshot() };
}

const ORIGINAL_AI_IDENTITIES = "collectAiCardTargets===__sourceCollectAiCardTargets&&collectAiWizardActions===__sourceCollectAiWizardActions&&isAiUsefulSpecialMove===__sourceIsAiUsefulSpecialMove";

test("public enumeration preserves the original private AI policy between cursor pages", () => {
  const { direct, position } = emptyPortalPolicyFixture();
  assert.equal(direct.evaluate(ORIGINAL_AI_IDENTITIES), true);
  assert.equal(direct.evaluate("collectAiCardTargets(findDeckCard('policy-portal'),'white',{exhaustive:true}).length"), 16);
  assert.equal(direct.candidates(position).filter(action => action.cardId === "portal-gun").length, 4032);
  assert.equal(direct.evaluate(ORIGINAL_AI_IDENTITIES), true);

  const stream = direct.actionStream(position, { cardId: "portal-gun", legal: false });
  const first = stream.nextPage(1, { maxExamined: 1 });
  assert.deepEqual(first.actions[0].payload.target.selections, [{ row: 0, col: 0 }, { row: 0, col: 1 }]);
  assert.equal(direct.evaluate(ORIGINAL_AI_IDENTITIES), true, "a paused generator retained public collector overrides");
  assert.equal(direct.evaluate("collectAiCardTargets(findDeckCard('policy-portal'),'white',{exhaustive:true}).length"), 16);
  const second = stream.nextPage(1, { maxExamined: 1 });
  assert.deepEqual(second.actions[0].payload.target.selections, [{ row: 0, col: 0 }, { row: 0, col: 2 }]);
  assert.equal(direct.evaluate(ORIGINAL_AI_IDENTITIES), true);
});

test("public collector scopes restore nested exceptions and use original simulation predicates", () => {
  const { direct } = emptyPortalPolicyFixture();
  const result = JSON.parse(direct.evaluate(`JSON.stringify(withPublicEnumerationPolicy(()=>{
    const outer=[collectAiCardTargets,collectAiWizardActions,isAiUsefulSpecialMove];
    let nestedError=false;
    try { withPublicEnumerationPolicy(()=>{throw new Error('nested-public-policy-probe');}); }
    catch(error) { nestedError=error.message==='nested-public-policy-probe'; }
    const nestedRestored=outer.every((entry,index)=>entry===[collectAiCardTargets,collectAiWizardActions,isAiUsefulSpecialMove][index]);
    const wizard={type:'wizard',color:'white',mana:1};
    const body={colossusBody:true,row:3,col:4};
    const publicResult={targets:collectAiCardTargets(findDeckCard('policy-portal'),'white',{exhaustive:true}).length,
      wizard:collectAiWizardActions(wizard,3,3,'white').length,useful:isAiUsefulSpecialMove(wizard,3,3,body,'white'),
      defaultTargets:collectAiCardTargets(findDeckCard('policy-portal'),'white').length};
    const simulate=(field)=>{
      const previous=field==='ai'?aiSimulationDepth:kingThreatProbeDepth;
      try {
        if(field==='ai')aiSimulationDepth=previous+1;else kingThreatProbeDepth=previous+1;
        return {targets:collectAiCardTargets(findDeckCard('policy-portal'),'white',{exhaustive:true}).length,
          wizard:collectAiWizardActions(wizard,3,3,'white').length,useful:isAiUsefulSpecialMove(wizard,3,3,body,'white')};
      } finally {if(field==='ai')aiSimulationDepth=previous;else kingThreatProbeDepth=previous;}
    };
    return {nestedError,nestedRestored,publicResult,availability:simulate('ai'),danger:simulate('threat')};
  }))`));
  assert.deepEqual(result, {
    nestedError: true, nestedRestored: true,
    publicResult: { targets: 4032, wizard: 64, useful: true, defaultTargets: 16 },
    availability: { targets: 16, wizard: 0, useful: false },
    danger: { targets: 16, wizard: 0, useful: false },
  });
  assert.equal(direct.evaluate(ORIGINAL_AI_IDENTITIES), true);
  assert.throws(() => direct.evaluate("withPublicEnumerationPolicy(()=>{throw new Error('outer-public-policy-probe');})"), /outer-public-policy-probe/);
  assert.equal(direct.evaluate(ORIGINAL_AI_IDENTITIES), true, "throwing public collection retained overrides");
});

test("candidate budget refusal restores source collectors before a lazy cursor resumes", () => {
  const { direct, position } = emptyPortalPolicyFixture({ maxCandidates: 1 });
  assert.throws(() => direct.candidates(position), /candidate budget; use actionStream/);
  assert.equal(direct.evaluate(ORIGINAL_AI_IDENTITIES), true);
  const stream = direct.actionStream(position, { cardId: "portal-gun", legal: false });
  const page = stream.nextPage(1, { maxExamined: 1 });
  assert.equal(page.actions.length, 1);
  assert.equal(page.examined, 1);
  assert.equal(page.stopReason, "page-limit");
  assert.equal(direct.evaluate(ORIGINAL_AI_IDENTITIES), true);
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

test("restored source RULE pool accepts capture-the-flag and leaves newGame reusable", () => {
  const adapter = makeAdapter();
  const before = adapter.newGame({ draftDelete: true }, 29);
  const capture = adapter.newGame({ draftDelete: true, ruleCardIds: ["capture-the-flag"] }, 29);
  assert.equal(capture.state.mode, "play");
  assert.equal(capture.state.appliedRuleCard?.id, "capture-the-flag");
  for (const viewer of ["white", "black"]) contract.validateObservation(adapter.observe(capture, viewer));
  assert.throws(() => adapter.newGame({ draftDelete: true, ruleCardIds: ["missing-rule"] }, 29), /Unknown RULE card/);
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
