#!/usr/bin/env node
"use strict";

// Prepare reviewed, source-derived metadata for the pinned current client.
// The raw site bundle and parser remain in the external baseline cache.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { loadMain, verify } = require("./frozen-site");

const LEGACY = "20260927";
const CURRENT = "20260928";
const EXPECTED_MAIN_SHA256 = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
const RULES_VERSION = `augment-site-${CURRENT}-${EXPECTED_MAIN_SHA256.slice(0, 16)}`;
const PROJECTION_VERSION = "source-visible-20260928-v1";
const PROFILE_VERSION = "accelerate-headless-semantic-v7";
const catalogPath = name => path.resolve(__dirname, `../../../bridge/catalog/${name}-${LEGACY}.json`);
const newPath = name => path.resolve(__dirname, `../../../bridge/catalog/${name}-${CURRENT}.json`);
const load = name => JSON.parse(fs.readFileSync(catalogPath(name), "utf8"));
const sourceValue = (main, name) => JSON.parse(main.evaluate(`JSON.stringify(${name} instanceof Set?[...${name}]:${name})`));
function materialize(target, value, write) {
  const content = JSON.stringify(value, null, 2) + "\n";
  if (write) fs.writeFileSync(target, content, { flag: "wx" });
  else assert.equal(fs.readFileSync(target, "utf8").replace(/\r\n/g, "\n"), content,
    `Frozen baseline metadata differs: ${path.basename(target)}`);
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

  const oldSite = load("site");
  const sourceCatalogHash = main.evaluate("SEPTEMBER26_CATALOG_HASH");
  assert.equal(sourceCatalogHash, oldSite.catalogVersion, "Public catalog changed; review before deriving a new baseline");
  const cardFields = Object.keys(oldSite.cards[0]);
  const projectedCards = JSON.parse(main.evaluate(`JSON.stringify(CARDS$1.map(card=>({${cardFields.map(key => `${JSON.stringify(key)}:card[${JSON.stringify(key)}]`).join(",")}})))`));
  assert.deepEqual(projectedCards, oldSite.cards, "Public card metadata changed; review before deriving a new baseline");
  const currentSite = { ...oldSite, rulesVersion: RULES_VERSION, source: sourceMetadata };

  const oldDefinitions = load("card-definitions");
  const definitions = JSON.parse(main.evaluate("JSON.stringify(CARD_DEFS.map(({name,text,art,...rules})=>rules))"));
  assert.deepEqual(definitions, oldDefinitions.definitions, "Semantic card definitions changed; review before deriving a new baseline");
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
  const currentDefinitions = { ...oldDefinitions, rulesVersion: RULES_VERSION,
    sourceMainSha256: EXPECTED_MAIN_SHA256, constants };

  const oldDraft = load("draft");
  const draftConstants = { ...oldDraft.constants,
    LATEST_MUTUALLY_EXCLUSIVE_DRAFT_CARD_GROUPS: constants.LATEST_MUTUALLY_EXCLUSIVE_DRAFT_CARD_GROUPS,
    COLOSSUS_FALSE_START_GROUP: constants.COLOSSUS_FALSE_START_GROUP,
    COLOSSUS_EXCLUSIVE_GROUPS: constants.COLOSSUS_EXCLUSIVE_GROUPS };
  for (const [name, oldValue] of Object.entries(oldDraft.constants)) {
    if (["CARD_CATEGORY_GROUPS", "LATEST_MUTUALLY_EXCLUSIVE_DRAFT_CARD_GROUPS"].includes(name)) continue;
    assert.deepEqual(draftConstants[name], oldValue, `Unexpected changed draft constant ${name}`);
  }
  const currentDraft = { ...oldDraft, rulesVersion: RULES_VERSION, constants: draftConstants };

  // The old and current source produce byte-equivalent initial raw state
  // under identical deterministic clock/RNG and the same headless reset setup.
  // That comparison is separate from dynamic transition coverage.
  const currentInitial = { ...load("initial-state"), rulesVersion: RULES_VERSION };

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
  const currentObservation = { ...oldObservation, rulesVersion: RULES_VERSION,
    projectionVersion: PROJECTION_VERSION,
    stateFieldClassification: { ...oldObservation.stateFieldClassification, internalBookkeeping },
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

  const schemaPath = path.resolve(__dirname, "../../../bridge/schemas/runtime-v1.schema.json");
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
  currentSchema.$id = `runtime-site-${CURRENT}.schema.json`;

  const currentSchemaPath = path.resolve(__dirname, `../../../bridge/schemas/runtime-site-${CURRENT}.schema.json`);
  for (const [name, value] of Object.entries({ site: currentSite, "card-definitions": currentDefinitions,
    draft: currentDraft, "initial-state": currentInitial, observation: currentObservation })) {
    materialize(newPath(name), value, write);
  }
  materialize(currentSchemaPath, currentSchema, write);
  return { rulesVersion: RULES_VERSION, catalogVersion: oldSite.catalogVersion,
    projectionVersion: PROJECTION_VERSION, profileVersion: PROFILE_VERSION,
    mainSha256: EXPECTED_MAIN_SHA256, cards: projectedCards.length, definitions: definitions.length,
    colossusExclusiveGroups: constants.COLOSSUS_EXCLUSIVE_GROUPS.length };
}

module.exports = { prepare, RULES_VERSION, PROJECTION_VERSION, PROFILE_VERSION, EXPECTED_MAIN_SHA256 };
if (require.main === module) {
  const [mode, root] = process.argv.slice(2);
  try {
    if (!["--verify", "--write"].includes(mode) || !root || !path.isAbsolute(root)) throw new Error("Usage: prepare-current-baseline.js [--verify|--write] ABSOLUTE_BASELINE_DIRECTORY");
    console.log(JSON.stringify(prepare(root, { write: mode === "--write" }), null, 2));
  }
  catch (error) { console.error(error.stack); process.exitCode = 1; }
}
