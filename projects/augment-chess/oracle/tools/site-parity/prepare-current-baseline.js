#!/usr/bin/env node
"use strict";

// Prepare reviewed, source-derived metadata for the pinned current client.
// The raw site bundle and parser remain in the external baseline cache.
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { loadMain, verify } = require("./frozen-site");
const { executionProfileForSha, metadataDigest } = require("../../game-adapter/src/reviewed-initializers");
const { rng, nextRandom } = require("../../../contracts/tools/runtime-contract");

const LEGACY = "20260927";
const CURRENT = "20260928";
const EXPECTED_MAIN_SHA256 = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
const RULES_VERSION = `augment-site-${CURRENT}-${EXPECTED_MAIN_SHA256.slice(0, 16)}`;
const PROJECTION_VERSION = "source-visible-20260928-v2";
const REVERSAL_PUBLIC_SCHEMA = { type: "object",
  properties: { white: { type: "boolean" }, black: { type: "boolean" } },
  required: ["white", "black"], additionalProperties: false };
const REVERSAL_FIELD_EVIDENCE = {
  representation: "공개 반전 카드의 진영별 활성 여부만 보존한다. 공개 getLegalMoves에서 룩과 비숍의 방향을 바꾸며 해당 진영 턴이 끝나면 해제된다. 기물 식별자·좌표·미래 난수는 포함하지 않는다.",
  sourceAnchors: [82751, 93661, 95730, 95739, 100187, 100194, 100195, 110952],
};
const EXECUTION_PROFILE = executionProfileForSha(EXPECTED_MAIN_SHA256);
const PROFILE_VERSION = EXECUTION_PROFILE.profileVersion;
const EXECUTION_PROFILE_SHA256 = metadataDigest(EXECUTION_PROFILE);
const SOURCE_PUBLIC_CATALOG_HASH = EXECUTION_PROFILE.sourcePublicCatalogHash;
const CATALOG_VERSION = metadataDigest({ contractVersion: "augment-v7-execution-catalog-v1",
  sourcePublicCatalogHash: SOURCE_PUBLIC_CATALOG_HASH, executionProfileSha256: EXECUTION_PROFILE_SHA256 });
const EXECUTION_PROFILE_REFERENCE = Object.freeze({ version: PROFILE_VERSION,
  sha256: EXECUTION_PROFILE_SHA256, manifest: `execution-profile-${CURRENT}.json` });
const catalogPath = name => path.resolve(__dirname, `../../../contracts/catalog/${name}-${LEGACY}.json`);
const newPath = name => path.resolve(__dirname, `../../../contracts/catalog/${name}-${CURRENT}.json`);
const load = name => JSON.parse(fs.readFileSync(catalogPath(name), "utf8"));
const sourceValue = (main, name) => JSON.parse(main.evaluate(`JSON.stringify(${name} instanceof Set?[...${name}]:${name})`));
const digest = value => crypto.createHash("sha256").update(JSON.stringify(value)).digest("hex");
function materialize(target, value, write) {
  const content = JSON.stringify(value, null, 2) + "\n";
  if (write) fs.writeFileSync(target, content);
  else assert.equal(fs.readFileSync(target, "utf8").replace(/\r\n/g, "\n"), content,
    `Frozen baseline metadata differs: ${path.basename(target)}`);
}

function withReviewedReversalField(fields, value) {
  const entries = Object.entries(fields);
  const index = entries.findIndex(([name]) => name === "revolvingDoorGuard");
  assert.ok(index >= 0 && !Object.hasOwn(fields, "reversal"),
    "Expected the legacy projection before the reviewed reversal field.");
  entries.splice(index, 0, ["reversal", value]);
  return Object.fromEntries(entries);
}

function deriveInitialTemplate(root, main, previous) {
  // 초기 template는 idle reset의 최초 replay 기록을 마친 뒤 재사용하는 기본값이다.
  // 실제 source 후속 문장을 실행하고, 생성된 기록을 확인한 뒤 template 투영을 한다.
  const raw = fs.readFileSync(path.join(root, "main-OahWs0tU.js"), "utf8");
  const hash = text => crypto.createHash("sha256").update(text).digest("hex");
  assert.equal(hash(raw), EXPECTED_MAIN_SHA256, "Initial template source SHA differs from the reviewed pin");
  const start = 10261490, end = 10271297;
  const expression = raw.slice(start, end);
  assert.equal(hash(expression), "c6e4eb6e74770c59062970ce46b8ca75f8a0624b982a91d286f845b88d44e05c",
    "Reviewed resetGame state constructor changed");
  const acorn = require(path.join(root, "acorn-8.15.0.js"));
  const ast = acorn.parseExpressionAt(raw, start, { ecmaVersion: "latest" });
  assert.ok(ast.type === "ObjectExpression" && ast.end === end, "Reset template AST boundary changed");
  const replayStart = 10271298, replayEnd = 10271456;
  const replayInitialization = raw.slice(replayStart, replayEnd);
  assert.equal(hash(replayInitialization), "1951de559e53a1d2b3777bab22a9e75a01a77101992e52ae610ee8a2c0202d68",
    "Reviewed resetGame post-constructor replay sequence changed");
  const replayAst = acorn.parse(replayInitialization, { ecmaVersion: "latest" });
  assert.deepEqual(replayAst.body.map(node => node.type), ["ExpressionStatement", "IfStatement", "IfStatement", "ExpressionStatement"],
    "Reset post-constructor replay statement boundaries changed");
  const finalReplayCall = replayAst.body.at(-1).expression;
  assert.ok(finalReplayCall.type === "CallExpression" && finalReplayCall.callee.name === "recordBoardHistory" &&
    finalReplayCall.arguments.length === 1 && finalReplayCall.arguments[0].value === "initial",
  "Reset template boundary is not the source initial history commit");
  const replayCommitOffset = replayStart + replayAst.body.at(-1).start;
  const replayPreparation = raw.slice(replayStart, replayCommitOffset);
  const replayCommit = raw.slice(replayCommitOffset, replayEnd);
  let random = rng(0);
  main.context.__initialTemplateRandom = () => {
    const sample = nextRandom(random); random = sample.rng; return sample.value;
  };
  main.context.__initialTemplateTime = Date.parse(main.manifest.frozenAt);
  assert.ok(Number.isFinite(main.context.__initialTemplateTime), "Initial template frozen date is invalid");
  main.evaluate(`
    const __InitialTemplateNativeDate=Date;
    Date=class extends __InitialTemplateNativeDate {
      constructor(...args){super(...(args.length?args:[__initialTemplateTime]));}
      static now(){return __initialTemplateTime;}
    };
    Math.random=__initialTemplateRandom;
  `);
  const generated = JSON.parse(main.evaluate(`(()=>{
    const start=false, banSnapshot=null, previousDevMode=false, initialGameStyle=gameStyleForNewGame();
    const serialize=value=>JSON.stringify(value,(_key,current)=>
      current instanceof Set?{__simType:'Set',values:[...current]}:
      current instanceof Map?{__simType:'Map',entries:[...current]}:current);
    state=${expression};
    ${replayPreparation}
    const historyCapture=JSON.parse(serialize({board:state.board,lastMove:state.lastMove,
      blackHole:state.blackHole||[],winterKingdom:state.winterKingdom||null,
      camouflageRule:Boolean(state.camouflageRule),turn:state.turn,
      moveCount:state.moveCount,fullMove:state.fullMove}));
    ${replayCommit}
    return serialize({state,historyCapture,initialFrame:captureReplayFrame(state)});
  })()`));
  const { state, historyCapture, initialFrame } = generated;
  assert.equal(state.mode, "idle", "Reset constructor is not an idle template");
  assert.equal(state.replayTimelineReady, true, "Source initial history commit did not initialize its replay timeline");
  assert.deepEqual(state.replayEvents, [], "Idle reset unexpectedly committed replay events");
  assert.equal(state.replayEventNonce, 0, "Idle reset unexpectedly consumed replay event identities");
  assert.deepEqual(state.notationTimeline, [], "Idle reset unexpectedly committed notation entries");
  assert.deepEqual(state.replayBaseFrame, initialFrame, "Source idle reset base frame differs from its committed initial position");
  assert.deepEqual(state.replayTailFrame, initialFrame, "Source idle reset tail frame differs from its committed initial position");
  assert.deepEqual(state.boardHistory, [{ ...historyCapture, cardAnimation: null, effects: [], notation: null,
    label: "initial", replayIndex: 0 }], "Source idle reset history differs from its pre-replay capture boundary");
  // template는 새 구성·seed로 초기 기록을 다시 만들므로 이미 materialize한 기록만 비운다.
  // replayTimelineReady는 원문 ensureReplayTimeline 결과를 그대로 유지한다.
  state.boardHistory = [];
  state.replayBaseFrame = null;
  state.replayTailFrame = null;
  assert.ok(Array.isArray(state.board) && state.board.length === 8 && state.board.every(row => Array.isArray(row) && row.length === 8),
    "Idle template board is not 8x8");
  const ids = new Set();
  for (const row of state.board) for (const piece of row) {
    if (piece === null) continue;
    assert.ok(typeof piece.id === "string" && typeof piece.origin === "string", "Initial piece lacks source identity/origin");
    const id = `${piece.color}-${piece.type}-${piece.origin}`;
    assert.ok(!ids.has(id), `Duplicate canonical initial piece ID ${id}`);
    ids.add(id); piece.id = id;
  }
  assert.equal(ids.size, 32, "Reset constructor changed the ordinary initial piece count");
  assert.equal(state.replayStartedAt, new Date(main.context.__initialTemplateTime).toISOString(), "Initial template date was not deterministic");
  state.replayStartedAt = "";
  assert.deepEqual(state, previous.state, "Frozen idle reset template differs from the source initial replay commit; review the exact fields before changing defaults");
  return { ...previous, rulesVersion: RULES_VERSION, catalogVersion: CATALOG_VERSION, state,
    sourcePublicCatalogHash: SOURCE_PUBLIC_CATALOG_HASH, executionProfile: EXECUTION_PROFILE_REFERENCE,
    templateDerivation: {
      sourceExecutionBoundary: "resetGame(false,null):after-recordBoardHistory(initial)",
      historyCaptureBoundary: "before recordBoardHistory(initial); replay frames validated after source ensureReplayTimeline",
      sourceConstructor: { start, end, sha256: hash(expression) },
      sourcePostConstructor: { start: replayStart, end: replayEnd, sha256: hash(replayInitialization) },
      normalization: {
        initialPieceIds: "color-type-origin; regenerated from each new-game seed",
        replayStartedAt: "empty; restored from the frozen source date at newGame",
        committedInitialReplay: "validated boardHistory/replayBaseFrame/replayTailFrame omitted from reusable defaults; source replayTimelineReady retained",
      },
    } };
}

function prepare(root, { write = false } = {}) {
  const manifest = verify(root, { scope: "client" });
  const mainFile = manifest.files.find(file => /^main-/.test(file.name));
  assert.equal(mainFile.sha256, EXPECTED_MAIN_SHA256, "Current site main SHA differs from reviewed pin");
  assert.equal(mainFile.name, "main-OahWs0tU.js", "Current site main asset differs from reviewed pin");
  // CI intentionally downloads only the executed main and parser. Retain the
  // reviewed index/worker provenance without fetching mutable URLs or
  // claiming that those two files were executed by the headless adapter.
  let sourceMetadata = manifest;
  if (manifest.executionScope === "frozen-client") {
    if (write) throw new Error("Writing full source provenance requires the original full baseline manifest.");
    const committed = JSON.parse(fs.readFileSync(newPath("site"), "utf8")).source;
    assert.equal(manifest.schemaVersion, committed.schemaVersion, "Client-only source schema differs from reviewed source.");
    assert.equal(manifest.site, committed.site, "Client-only source site differs from reviewed source.");
    assert.equal(manifest.frozenAt, committed.frozenAt, "Client-only source time differs from reviewed source.");
    assert.deepEqual(manifest.files, committed.files.filter(file => /^main-/.test(file.name) || file.name === "acorn-8.15.0.js"),
      "Executed main/parser differ from the reviewed full provenance.");
    sourceMetadata = committed;
  }
  const main = loadMain(root);
  assert.equal(main.executionProfile.profileVersion, PROFILE_VERSION, "Source initialization profile differs from the reviewed metadata");
  assert.equal(main.executionProfileSha256, EXECUTION_PROFILE_SHA256, "Source execution profile digest differs from the reviewed metadata");
  assert.deepEqual(sourceValue(main, "({frameKeys:REPLAY_FRAME_KEYS,codes:PIECE_NOTATION_CODES,labels:TYPE_LABELS})"),
    EXECUTION_PROFILE.replayMetadata, "Faithful source replay metadata differs from the profile manifest");

  const oldSite = load("site");
  const sourceCatalogHash = main.evaluate("SEPTEMBER26_CATALOG_HASH");
  assert.equal(sourceCatalogHash, SOURCE_PUBLIC_CATALOG_HASH, "Official source balance hash differs from the execution profile");
  assert.equal(sourceCatalogHash, oldSite.catalogVersion, "Public catalog changed; review before deriving a new baseline");
  const cardFields = Object.keys(oldSite.cards[0]);
  const projectedCards = JSON.parse(main.evaluate(`JSON.stringify(CARDS$1.map(card=>({${cardFields.map(key => `${JSON.stringify(key)}:card[${JSON.stringify(key)}]`).join(",")}})))`));
  assert.deepEqual(projectedCards, oldSite.cards, "Public card metadata changed; review before deriving a new baseline");
  const currentSite = { ...oldSite, catalogVersion: CATALOG_VERSION, rulesVersion: RULES_VERSION, source: sourceMetadata,
    sourcePublicCatalogHash: SOURCE_PUBLIC_CATALOG_HASH, executionProfile: EXECUTION_PROFILE_REFERENCE };

  const oldDefinitions = load("card-definitions");
  const definitions = JSON.parse(main.evaluate("JSON.stringify(CARD_DEFS.map(({name,text,art,help,helpItems,helpIcons,...rules})=>rules))"));
  assert.equal(definitions.length, oldDefinitions.definitions.length, "Pinned source definition count changed");
  const changedFields = { phase: 0, stars: 0, target: 0 };
  for (let index = 0; index < definitions.length; index++) {
    const current = definitions[index], previous = oldDefinitions.definitions[index];
    assert.equal(current.id, previous.id, `Source definition order changed at ${index}`);
    assert.equal(current.effect, previous.effect, `Source effect changed for ${current.id}`);
    for (const field of new Set([...Object.keys(current), ...Object.keys(previous)])) {
      if (JSON.stringify(current[field]) === JSON.stringify(previous[field])) continue;
      if (!Object.hasOwn(changedFields, field)) throw new Error(`Unexpected semantic source change: ${current.id}.${field}`);
      changedFields[field]++;
    }
  }
  assert.deepEqual(changedFields, { phase: 26, stars: 44, target: 4 }, "Reviewed v7 initializer semantic change count differs");
  assert.equal(digest(definitions), "369bceccd593ea56faa4adbd5f5d8157c4c6b0e9badb0a230669a05633a479bb",
    "Reviewed v7 semantic definitions digest differs");
  const constants = {};
  for (const name of Object.keys(oldDefinitions.constants)) constants[name] = sourceValue(main, name);
  constants.COLOSSUS_FALSE_START_GROUP = sourceValue(main, "COLOSSUS_FALSE_START_GROUP");
  constants.COLOSSUS_EXCLUSIVE_GROUPS = sourceValue(main, "COLOSSUS_EXCLUSIVE_GROUPS");
  for (const [name, oldValue] of Object.entries(oldDefinitions.constants)) {
    if (name !== "LATEST_MUTUALLY_EXCLUSIVE_DRAFT_CARD_GROUPS") assert.deepEqual(constants[name], oldValue, `Unexpected changed card constant ${name}`);
  }
  assert.deepEqual(constants.LATEST_MUTUALLY_EXCLUSIVE_DRAFT_CARD_GROUPS,
    [...constants.COLOSSUS_EXCLUSIVE_GROUPS, ...oldDefinitions.constants.LATEST_MUTUALLY_EXCLUSIVE_DRAFT_CARD_GROUPS],
    "Draft exclusivity changed outside the reviewed colossus groups");
  const currentDefinitions = { ...oldDefinitions, rulesVersion: RULES_VERSION, catalogVersion: CATALOG_VERSION,
    sourceMainSha256: EXPECTED_MAIN_SHA256,
    presentationFieldsExcluded: ["name", "text", "art", "help", "helpItems", "helpIcons"],
    definitions, constants, sourcePublicCatalogHash: SOURCE_PUBLIC_CATALOG_HASH, executionProfile: EXECUTION_PROFILE_REFERENCE };

  const oldDraft = load("draft");
  const sourceGroups = sourceValue(main, "CARD_CATEGORY_GROUPS");
  assert.equal(digest(sourceGroups), "c4470b030fe1ad2fd2f61a85406afc7c086e2a7aeeb6cdae9e9b65f5fdc5038d",
    "Reviewed v7 draft group digest differs");
  assert.equal(sourceGroups.RULE.length, 27, "The pinned source RULE pool changed");
  const draftConstants = { ...oldDraft.constants, CARD_CATEGORY_GROUPS: sourceGroups,
    LATEST_MUTUALLY_EXCLUSIVE_DRAFT_CARD_GROUPS: constants.LATEST_MUTUALLY_EXCLUSIVE_DRAFT_CARD_GROUPS,
    COLOSSUS_FALSE_START_GROUP: constants.COLOSSUS_FALSE_START_GROUP,
    COLOSSUS_EXCLUSIVE_GROUPS: constants.COLOSSUS_EXCLUSIVE_GROUPS };
  for (const [name, oldValue] of Object.entries(oldDraft.constants)) {
    if (["CARD_CATEGORY_GROUPS", "LATEST_MUTUALLY_EXCLUSIVE_DRAFT_CARD_GROUPS"].includes(name)) continue;
    assert.deepEqual(draftConstants[name], oldValue, `Unexpected changed draft constant ${name}`);
  }
  const weights = JSON.parse(main.evaluate("JSON.stringify(CARD_DEFS.map(card=>({id:card.id,effect:card.effect,phase:CARD_CATEGORY_BY_ID[card.id]||card.phase,stars:card.stars,weight:draftCardWeight(card),openingWeight:draftCardWeight(card,{openingBoost:true})})))"));
  assert.equal(weights.length, oldDraft.weights.length, "Pinned source draft weight count changed");
  const weightChanges = { phase: 0, stars: 0, weight: 0, openingWeight: 0 };
  for (let index = 0; index < weights.length; index++) {
    const current = weights[index], previous = oldDraft.weights[index];
    assert.equal(current.id, previous.id, `Source draft weight order changed at ${index}`);
    assert.equal(current.effect, previous.effect, `Source draft effect changed for ${current.id}`);
    for (const field of new Set([...Object.keys(current), ...Object.keys(previous)])) {
      if (JSON.stringify(current[field]) === JSON.stringify(previous[field])) continue;
      if (!Object.hasOwn(weightChanges, field)) throw new Error(`Unexpected source draft weight change: ${current.id}.${field}`);
      weightChanges[field]++;
    }
  }
  assert.deepEqual(weightChanges, { phase: 4, stars: 44, weight: 30, openingWeight: 30 },
    "Reviewed v7 draft weight changes differ");
  assert.equal(digest(weights), "482dd7916e10917524eba4bbdb72ca46d59662cd43c6bc40d1ef69d0f772b0bc",
    "Reviewed v7 draft weights digest differs");
  const currentDraft = { ...oldDraft, rulesVersion: RULES_VERSION, constants: draftConstants, weights };

  const currentInitial = deriveInitialTemplate(root, main, load("initial-state"));

  const oldObservation = load("observation");
  const oldBookkeeping = oldObservation.stateFieldClassification.internalBookkeeping;
  const bookkeepingInsert = oldBookkeeping.indexOf("pendingBearRetaliations");
  assert.ok(bookkeepingInsert > 0 && !oldBookkeeping.includes("othelloPending"),
    "Expected the reviewed pre-Othello bookkeeping classification.");
  const internalBookkeeping = [...oldBookkeeping.slice(0, bookkeepingInsert), "othelloPending",
    ...oldBookkeeping.slice(bookkeepingInsert)];
  const oldReview = Object.entries(oldObservation.remainingStateReview);
  const reviewInsert = oldReview.findIndex(([name]) => name === "overwhelm");
  assert.ok(reviewInsert > 0 && !Object.hasOwn(oldObservation.remainingStateReview, "othelloPending"),
    "Expected the reviewed pre-Othello source-field audit.");
  oldReview.splice(reviewInsert, 0, ["othelloPending", {
    classification: "internalBookkeeping",
    anchors: [43195, 50083, 82837, 93671, 93672, 105009, 105010, 110920],
    rawDisposition: "Per-color end-of-turn Othello recheck latch; source renderer does not read the raw boolean map, so it is not directly projected to either viewer.",
    publicMeaning: "The played card and resulting board conversions are visible through existing public card, board and event projections; no separate latch badge is rendered.",
  }]);
  const visibilityReview = oldObservation.visibilityReview.replace("remaining 52 raw state fields", "remaining 53 raw state fields");
  assert.notEqual(visibilityReview, oldObservation.visibilityReview,
    "Expected the pre-Othello visibility review count.");
  assert.ok(!oldObservation.statePublicFields.includes("reversal") &&
    oldObservation.dynamicPublicFieldEvidence.fields.includes("roller"),
  "Expected the inherited public state before the reviewed reversal field.");
  const currentObservation = { ...oldObservation, rulesVersion: RULES_VERSION,
    projectionVersion: PROJECTION_VERSION,
    statePublicFields: [...oldObservation.statePublicFields, "reversal"].sort(),
    stateFieldClassification: { ...oldObservation.stateFieldClassification, internalBookkeeping },
    dynamicPublicFieldEvidence: { ...oldObservation.dynamicPublicFieldEvidence,
      fields: oldObservation.dynamicPublicFieldEvidence.fields.flatMap(name => name === "roller" ? ["reversal", name] : [name]) },
    reviewedFieldEvidence: withReviewedReversalField(oldObservation.reviewedFieldEvidence, REVERSAL_FIELD_EVIDENCE),
    stateValueSchemas: withReviewedReversalField(oldObservation.stateValueSchemas, REVERSAL_PUBLIC_SCHEMA),
    visibilityReview,
    remainingStateReview: Object.fromEntries(oldReview),
    sourceAudit: {
      currentMainSha256: EXPECTED_MAIN_SHA256,
      inheritedProjectionBasisSha256: oldSite.source.files.find(file => /^main-/.test(file.name)).sha256,
      status: "field-shapes-retained; dynamic-current-source-observation-parity-pending",
      changedSourceFamilies: ["chameleon-eye-presentation", "don-quixote-route-presentation", "piece-notation"]
    },
    coverage: { ...oldObservation.coverage,
      status: "partial-source-projection",
      currentSourceDisposition: "Existing public field schemas are retained provisionally. The current client's changed presentation and transition paths require execution-level review before full coverage is claimed." } };

  const schemaPath = path.resolve(__dirname, "../../../contracts/schemas/runtime-v1.schema.json");
  const schema = JSON.parse(fs.readFileSync(schemaPath, "utf8"));
  const oldRules = oldSite.rulesVersion;
  const oldProjection = oldObservation.projectionVersion;
  const replace = { [oldRules]: RULES_VERSION, [oldProjection]: PROJECTION_VERSION };
  const counts = { [oldRules]: 0, [oldProjection]: 0 };
  function visit(value) {
    if (typeof value === "string" && Object.hasOwn(replace, value)) { counts[value]++; return replace[value]; }
    if (Array.isArray(value)) return value.map(visit);
    if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([key, entry]) => [key, visit(entry)]));
    return value;
  }
  const currentSchema = visit(schema);
  assert.equal(counts[oldRules], 1, "Expected exactly one Position rulesVersion schema constant");
  assert.equal(counts[oldProjection], 1, "Expected exactly one Observation projectionVersion schema constant");
  assert.equal(currentSchema.$defs.Position.properties.catalogVersion.const, oldSite.catalogVersion,
    "Expected the legacy official catalog constant before applying the execution profile identity");
  currentSchema.$defs.Position.properties.catalogVersion.const = CATALOG_VERSION;
  currentSchema.$defs.Observation.properties.publicState.properties = withReviewedReversalField(
    currentSchema.$defs.Observation.properties.publicState.properties, REVERSAL_PUBLIC_SCHEMA);
  currentSchema.$id = `runtime-site-${CURRENT}.schema.json`;

  const currentSchemaPath = path.resolve(__dirname, `../../../contracts/schemas/runtime-site-${CURRENT}.schema.json`);
  for (const [name, value] of Object.entries({ site: currentSite, "card-definitions": currentDefinitions,
    draft: currentDraft, "initial-state": currentInitial, observation: currentObservation })) {
    materialize(newPath(name), value, write);
  }
  materialize(currentSchemaPath, currentSchema, write);
  return { rulesVersion: RULES_VERSION, catalogVersion: CATALOG_VERSION,
    sourcePublicCatalogHash: SOURCE_PUBLIC_CATALOG_HASH, executionProfileSha256: EXECUTION_PROFILE_SHA256,
    projectionVersion: PROJECTION_VERSION, profileVersion: PROFILE_VERSION,
    mainSha256: EXPECTED_MAIN_SHA256, cards: projectedCards.length, definitions: definitions.length,
    colossusExclusiveGroups: constants.COLOSSUS_EXCLUSIVE_GROUPS.length };
}

module.exports = { prepare, RULES_VERSION, PROJECTION_VERSION, PROFILE_VERSION, EXPECTED_MAIN_SHA256,
  CATALOG_VERSION, SOURCE_PUBLIC_CATALOG_HASH, EXECUTION_PROFILE_SHA256 };
if (require.main === module) {
  const [mode, root] = process.argv.slice(2);
  try {
    if (!["--verify", "--write"].includes(mode) || !root || !path.isAbsolute(root)) throw new Error("Usage: prepare-current-baseline.js [--verify|--write] ABSOLUTE_BASELINE_DIRECTORY");
    console.log(JSON.stringify(prepare(root, { write: mode === "--write" }), null, 2));
  }
  catch (error) { console.error(error.stack); process.exitCode = 1; }
}
