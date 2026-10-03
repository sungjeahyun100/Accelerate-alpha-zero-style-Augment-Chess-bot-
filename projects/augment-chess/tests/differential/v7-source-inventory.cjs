#!/usr/bin/env node
"use strict";

// Evidence levels are deliberately separate. A catalog definition, a source
// draft pool candidate, and an observed source transition are different claims.
const fs = require("node:fs");
const path = require("node:path");
const { FrozenClientSource, GameAdapter } = require("../../oracle/game-adapter/src");
const { createRuntimeContract } = require("../../contracts/tools/runtime-contract");

const SOURCE_SHA256 = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
const PROFILE = "accelerate-headless-semantic-v7-faithful-init-v1";
const STYLES = ["normal", "chaos", "grand"];
const MAX_DRAFT_PICKS = 64;
const copy = value => JSON.parse(JSON.stringify(value));

function baselineRoot() {
  const configured = process.env.ACCELERATE_SITE_BASELINE_LATEST || process.env.ACCELERATE_SITE_BASELINE;
  if (configured) {
    if (!path.isAbsolute(configured)) throw new TypeError("Pinned source root must be absolute.");
    return configured;
  }
  const parent = process.env.RUNNER_TEMP || process.env.APPDATA;
  if (!parent || !path.isAbsolute(parent)) throw new Error("Set an absolute RUNNER_TEMP, APPDATA, or ACCELERATE_SITE_BASELINE_LATEST.");
  return path.join(parent, "Accelerate", "cache", "site-baseline-20260928-e5ed84fc");
}

function sourceFacts(source) {
  const runtime = source.createRuntime();
  return copy(runtime.evaluate(`({
    definitions: CARD_DEFS.map(card => ({
      id: card.id, effect: card.effect,
      draftCategory: CARD_CATEGORY_BY_ID[card.id] || card.phase,
      passive: PASSIVE_CARD_IDS.has(card.id)
    })),
    rulePool: ruleCardPool().map(card => card.id),
    rulePoolWithoutDeathmatch: (() => {
      const previous = deathmatchEnabled;
      deathmatchEnabled = false;
      try { return ruleCardPool().map(card => card.id); }
      finally { deathmatchEnabled = previous; }
    })(),
    deletedIds: [...DELETED_CARD_IDS]
  })`));
}

function boardPieceTypes(position) {
  return [...new Set(position.state.board.flat().filter(Boolean).map(piece => piece.type))].sort();
}

function openingWitnesses(source, contract, seeds) {
  const witnesses = [];
  const errors = [];
  for (const style of STYLES) for (const seed of seeds) {
    const adapter = new GameAdapter({ source, contract });
    let stage = "newGame";
    try {
      let position = adapter.newGame({ gameStyle: style }, seed);
      const initialPieces = boardPieceTypes(position);
      const offeredCards = new Set();
      const selectedCards = new Set();
      const seenActionTypes = new Set();
      let draftPicks = 0;
      while (position.state.mode === "draft" && draftPicks < MAX_DRAFT_PICKS) {
        const choices = position.state.draft?.choices;
        if (!Array.isArray(choices) || !choices.length) throw new Error(`${style} seed ${seed}: draft has no source offer`);
        const offered = new Map(choices.map(card => [card.instanceId, card.id]));
        for (const id of offered.values()) offeredCards.add(id);
        stage = `draft action ${draftPicks}`;
        const action = adapter.actions(position)[0];
        if (!action) throw new Error(`${style} seed ${seed}: source draft has no executable action`);
        seenActionTypes.add(action.payload.type);
        const ids = action.payload.type === "draftBundlePick" ? action.payload.cardInstanceIds :
          action.payload.type === "draftPick" ? [action.payload.cardInstanceId] : [];
        for (const instanceId of ids) {
          const id = offered.get(instanceId);
          if (!id) throw new Error(`${style} seed ${seed}: selected draft card is absent from the offer`);
          selectedCards.add(id);
        }
        stage = `draft apply ${draftPicks} ${ids.map(instanceId => offered.get(instanceId)).join(",")}`;
        const step = adapter.apply(position, action, { recordHistory: false });
        if (!step.ok) throw new Error(`${style} seed ${seed}: source rejected its own draft action: ${step.error?.message}`);
        position = step.position;
        draftPicks++;
      }
      if (position.state.mode !== "play") throw new Error(`${style} seed ${seed}: draft did not reach play in ${MAX_DRAFT_PICKS} picks`);
      stage = "first-play actionStream";
      const cursor = adapter.actionStream(position);
      let firstPlay;
      try { stage = "first-play page"; firstPlay = cursor.nextPage(16, { maxExamined: 256 }); }
      finally { cursor.dispose(); }
      for (const action of firstPlay.actions) seenActionTypes.add(action.payload.type);
      for (const viewer of ["white", "black"]) {
        stage = `first-play observation ${viewer}`;
        contract.validateObservation(adapter.observe(position, viewer));
      }
      witnesses.push({ style, seed, draftPicks, initialPieces,
        offeredCards: [...offeredCards].sort(), selectedCards: [...selectedCards].sort(),
        firstPlayActionTypes: [...seenActionTypes].sort(), firstPlaySampledActionCount: firstPlay.actions.length,
        firstPlayExamined: firstPlay.examined, firstPlayExhausted: firstPlay.exhausted,
        firstPlayStopReason: firstPlay.stopReason,
        firstPlayPositionDigest: contract.digest(position) });
    } catch (error) {
      errors.push({ style, seed, stage, name: error.name, message: error.message });
    } finally { adapter.dispose(); }
  }
  return { witnesses, errors };
}

function collectInventory({ sourceRoot = baselineRoot(), witnessSeeds = [] } = {}) {
  if (!path.isAbsolute(sourceRoot)) throw new TypeError("Pinned source root must be absolute.");
  if (!Array.isArray(witnessSeeds) || witnessSeeds.some(seed => !Number.isSafeInteger(seed) || seed < 0))
    throw new TypeError("Witness seeds must be non-negative safe integers.");
  const contract = createRuntimeContract({ baseline: "site-20260928" });
  if (contract.ORACLE_PROFILE_VERSION !== PROFILE ||
      contract.catalog.source.files.find(file => /^main-/.test(file.name))?.sha256 !== SOURCE_SHA256)
    throw new Error("v7 catalog source/profile identity mismatch");
  const source = new FrozenClientSource(sourceRoot, { expectedClientSha256: SOURCE_SHA256 });
  const facts = sourceFacts(source);
  const definitions = new Map(facts.definitions.map(card => [card.id, card]));
  const published = new Set(contract.catalog.cards.map(card => card.id));
  const rulePool = new Set(facts.rulePool);
  const rulePoolWithoutDeathmatch = new Set(facts.rulePoolWithoutDeathmatch);
  const { witnesses, errors: witnessErrors } = openingWitnesses(source, contract, witnessSeeds);
  const offered = new Set(witnesses.flatMap(witness => witness.offeredCards));
  const selected = new Set(witnesses.flatMap(witness => witness.selectedCards));
  const initialPieces = new Set(witnesses.flatMap(witness => witness.initialPieces));
  const identityGaps = [];
  if (definitions.size !== facts.definitions.length) identityGaps.push("duplicate source card definition IDs");
  if (published.size !== contract.catalog.cards.length) identityGaps.push("duplicate public catalog card IDs");
  for (const card of contract.catalog.cards) {
    const definition = definitions.get(card.id);
    if (!definition) identityGaps.push(`${card.id}: missing pinned source definition`);
    else {
      if (definition.effect !== card.effect) identityGaps.push(`${card.id}: source effect ${definition.effect} != catalog ${card.effect}`);
      if (definition.draftCategory !== card.draftCategory)
        identityGaps.push(`${card.id}: source draft category ${definition.draftCategory} != catalog ${card.draftCategory}`);
      if ((definition.passive ? "PASSIVE" : "ACTIVE") !== card.activation)
        identityGaps.push(`${card.id}: source activation differs from catalog ${card.activation}`);
    }
  }
  for (const id of rulePool) {
    if (!published.has(id)) identityGaps.push(`${id}: source RULE pool ID missing from public catalog`);
    else if (contract.catalog.cards.find(card => card.id === id)?.draftCategory !== "RULE")
      identityGaps.push(`${id}: source RULE pool ID has non-RULE catalog category`);
  }
  const cards = contract.catalog.cards.map(card => ({
    id: card.id, category: card.draftCategory, effect: card.effect,
    sourceDefined: definitions.has(card.id),
    sourceRuleSelectable: card.draftCategory === "RULE" ? rulePool.has(card.id) : null,
    sourceRuleSelectableWithoutDeathmatch: card.draftCategory === "RULE" ? rulePoolWithoutDeathmatch.has(card.id) : null,
    sourceDraftCategory: definitions.get(card.id)?.draftCategory || null,
    openingOfferWitness: offered.has(card.id), openingSelectionWitness: selected.has(card.id),
    effectParity: "unverified-by-this-inventory",
    sourceSelectionEvidence: card.draftCategory === "RULE" ?
      rulePool.has(card.id) ? "source-rule-pool-default-deathmatch" : "not-in-current-source-rule-pool" :
      offered.has(card.id) ? "witnessed-opening-offer" : "unproven",
    sourceTransitionEvidence: selected.has(card.id) ? "witnessed-opening-acquisition" : "unproven",
  }));
  const pieces = contract.catalog.pieceTypes.map(id => ({ id,
    catalogRole: contract.catalog.pieceCatalogRole[id] || "unclassified",
    initialBoardWitness: initialPieces.has(id), laterSourceReachability: "unproven" }));
  const auxiliaryDefinitions = facts.definitions.filter(card => !published.has(card.id)).map(card => ({
    ...card, publicCatalogEntry: false,
    sourceReachability: "unproven-by-this-inventory",
    effectParity: "unverified-by-this-inventory",
  }));
  // This is a scope ledger, not a parity receipt. Preserve every declared
  // branch even when an opening witness never encounters it. Source reachability
  // and native parity need independent evidence from the differential runners.
  const acceptanceObligations = {
    board: { rows: 8, columns: 8, rectangularSyntheticProfiles: "separate-from-frozen-site-parity" },
    styles: STYLES.map(style => ({ style, ruleParity: "unverified-by-this-inventory" })),
    publicCards: cards.map(card => ({ id: card.id, activation: definitions.get(card.id)?.passive ? "PASSIVE" : "ACTIVE",
      sourcePreconditionsAndReachability: selected.has(card.id) ? "bounded-opening-acquisition-only" : "unproven-by-this-inventory",
      acquisitionParity: "unverified-by-this-inventory",
      effectAndSettlementParity: "unverified-by-this-inventory",
      interactionBranchParity: "unverified-by-this-inventory" })),
    auxiliaryDefinitions: auxiliaryDefinitions.map(card => ({ id: card.id,
      sourcePreconditionsAndReachability: card.sourceReachability, effectAndSettlementParity: card.effectParity })),
    rules: [...rulePool].map(id => ({ id,
      sourcePoolMembership: "verified-source-pool-only",
      sourcePoolMemberWithoutDeathmatch: rulePoolWithoutDeathmatch.has(id),
      runtimeAndInteractionParity: "unverified-by-this-inventory" })),
    pieceTypes: pieces.map(piece => ({ id: piece.id,
      sourcePreconditionsAndReachability: initialPieces.has(piece.id) ? "initial-board-witness-only" : "unproven-by-this-inventory",
      completeMovementAndRestrictionParity: "unverified-by-this-inventory" })),
    actionTypes: contract.catalog.actionTypes.map(type => ({ type,
      openingActionTypeWitness: witnesses.some(witness => witness.firstPlayActionTypes.includes(type)),
      completeOrderedLegalParity: "unverified-by-this-inventory",
      exactBindingAndRejectionParity: "unverified-by-this-inventory",
      fullPositionRngHistoryResultParity: "unverified-by-this-inventory" })),
    uiChoices: contract.catalog.sourceUiChoices.map(choice => ({ ...copy(choice),
      atomicTargetAndSettlementParity: "unverified-by-this-inventory" })),
    phasesAndTerminalBranches: "not-enumerated-or-proven-by-this-inventory",
    publicObservationAndHiddenStateBoundary: "unverified-by-this-inventory",
    sourceReachableInteractionCoverage: "unproven-by-this-inventory",
  };
  return {
    schemaVersion: 2, sourceSha256: SOURCE_SHA256, profile: PROFILE,
    rulesVersion: contract.catalog.rulesVersion, catalogVersion: contract.catalog.catalogVersion,
    scope: "Definitions and current source RULE pool, plus bounded natural opening witnesses; no full rule, card interaction, late draft, or terminal proof.",
    completeRuleCoverage: false,
    summary: {
      publicCards: cards.length, sourceDefinitions: facts.definitions.length,
      extraSourceDefinitions: facts.definitions.filter(card => !published.has(card.id)).map(card => card.id).sort(),
      sourceDeletedIds: facts.deletedIds.sort(),
      sourceRulePool: rulePool.size,
      sourceRulePoolWithoutDeathmatch: rulePoolWithoutDeathmatch.size,
      deathmatchConditionalRuleIds: [...rulePool].filter(id => !rulePoolWithoutDeathmatch.has(id)).sort(),
      catalogRulesAbsentFromSourcePool: cards.filter(card => card.category === "RULE" && !card.sourceRuleSelectable).map(card => card.id),
      openingOfferWitnesses: cards.filter(card => card.openingOfferWitness).length,
      openingSelectionWitnesses: cards.filter(card => card.openingSelectionWitness).length,
      initialPieceWitnesses: pieces.filter(piece => piece.initialBoardWitness).length,
      actionTypesDeclared: contract.catalog.actionTypes.length,
      actionTypesWitnessed: [...new Set(witnesses.flatMap(witness => witness.firstPlayActionTypes))].sort(),
      sourceUiChoicesDeclared: contract.catalog.sourceUiChoices.map(choice => choice.source),
      witnessFailures: witnessErrors.length,
    },
    identityGaps, cards, auxiliaryDefinitions, pieces, witnesses, witnessErrors, acceptanceObligations,
    capabilityEvidence: {
      rawLegal: witnesses.length ? "bounded-first-play-prefix-witness" : "not-run",
      uiHints: "not-verified-by-this-inventory",
      publicIntent: "not-verified-by-this-inventory",
      aiNoCardsThreat: "not-verified-by-this-inventory",
      terminal: "not-verified-by-this-inventory",
    },
  };
}

module.exports = { SOURCE_SHA256, STYLES, collectInventory };
if (require.main === module) {
  try {
    const args = process.argv.slice(2);
    if (args.some(arg => !/^--seeds=(?:\d+)(?:,\d+)*$/.test(arg)) || args.length > 1)
      throw new TypeError("Usage: node projects/augment-chess/tests/differential/v7-source-inventory.cjs [--seeds=0,1,7,19,42,20260928]");
    const seeds = args.length ? args[0].slice("--seeds=".length).split(",").map(Number) : [0, 1, 7, 19, 42, 20260928];
    const report = collectInventory({ witnessSeeds: seeds });
    const parent = process.env.RUNNER_TEMP || process.env.APPDATA;
    if (!parent || !path.isAbsolute(parent)) throw new Error("Absolute RUNNER_TEMP or APPDATA report root required.");
    const reportPath = path.join(parent, "Accelerate", "reports", "v7-source-inventory", "report.json");
    fs.mkdirSync(path.dirname(reportPath), { recursive: true });
    fs.writeFileSync(reportPath, JSON.stringify(report, null, 2) + "\n");
    console.log(JSON.stringify({ identityGaps: report.identityGaps.length, witnesses: report.witnesses.length,
      summary: report.summary, report: "Accelerate/reports/v7-source-inventory/report.json" }));
    for (const gap of report.identityGaps) console.error(gap);
    for (const error of report.witnessErrors) console.error(JSON.stringify(error));
    if (report.identityGaps.length || report.witnessErrors.length) process.exitCode = 1;
  } catch (error) { console.error(`${error.name}: ${error.message}`); process.exitCode = 1; }
}
