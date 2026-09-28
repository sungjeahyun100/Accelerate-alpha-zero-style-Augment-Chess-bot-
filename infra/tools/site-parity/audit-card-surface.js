#!/usr/bin/env node
"use strict";
const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const { FrozenClientSource, cacheRoot } = require("../../../packages/game-adapter/src");
const { OfflineOracle } = require("./offline-oracle");
const { createRuntimeContract } = require("../../../bridge/tools/runtime-contract");

const STYLES = ["normal", "chaos", "grand"];
const errorInfo = error => ({ name: error.name, message: error.message });
function sourceRoot(baseline, root) {
  if (root) {
    if (!path.isAbsolute(root)) throw new TypeError("The site baseline directory must be absolute.");
    return root;
  }
  if (baseline === "site-20260927") return cacheRoot();
  if (baseline !== "site-20260928") throw new TypeError("Unknown site baseline.");
  const configured = process.env.ACCELERATE_SITE_BASELINE_LATEST || process.env.ACCELERATE_SITE_BASELINE;
  if (configured) {
    if (!path.isAbsolute(configured)) throw new TypeError("The site baseline directory must be absolute.");
    return configured;
  }
  const parent = process.env.RUNNER_TEMP || process.env.APPDATA;
  if (!parent) throw new Error("Set ACCELERATE_SITE_BASELINE_LATEST or RUNNER_TEMP/APPDATA.");
  return path.join(parent, "Accelerate", "cache", "site-baseline-20260928-e5ed84fc");
}
function playBase(oracle, style, seed) {
  let position = oracle.newGame({ gameStyle: style }, seed);
  let count = 0;
  while (position.state.mode === "draft" && count < 32) {
    const offered = oracle.actions(position);
    // The first white offer at this seed is last-stand and turns all pawns
    // into fanatics. Keep a genuine source-selected pawn-preserving board.
    const preferredIndex = position.state.draft.color === "white" ? 1 : 0;
    const choice = offered[preferredIndex];
    if (!choice) throw new Error("Preferred source draft offer is unavailable.");
    const step = oracle.apply(position, choice, { recordHistory: false });
    if (!step.ok) throw new Error("Source rejected its draft choice: " + step.error?.message);
    position = step.position;
    count++;
  }
  if (position.state.mode !== "play") throw new Error("Source draft did not finish within 32 choices.");
  return position;
}
function syntheticHand(oracle, position, cardId) {
  oracle.restore(position);
  oracle.main.context.__auditCardId = cardId;
  try {
    oracle.evaluate("{const d=CARD_DEFS.find(card=>card.id===__auditCardId);if(!d)throw new Error('Card definition absent from source.');state.deckSlots.white=[cloneCard(d),null,null];state.deckSlots.black=[null,null,null];state.deck.white=state.deckSlots.white.filter(Boolean);state.deck.black=[];state.playerCards=state.deckSlots;}");
    return oracle.snapshot();
  } finally { delete oracle.main.context.__auditCardId; }
}
function cardProbe(oracle, contract, base, card, actionSamples) {
  const position = syntheticHand(oracle, base, card.id);
  const cursor = oracle.actionStream(position, { cardId: card.id, legal: false });
  let page;
  try { page = cursor.nextPage(actionSamples, { maxExamined: 4096 }); }
  finally { cursor.dispose(); }
  const result = {
    kind: "synthetic-hand-on-source-play-state",
    sampledCandidateCount: page.actions.length, candidatesExhausted: page.exhausted,
    examined: page.examined, stopReason: page.stopReason, sourceComparison: [],
  };
  for (const action of page.actions) {
    // Raw source execution can mutate non-Position globals. Each side starts
    // in a fresh VM to avoid contaminating the comparison itself.
    const direct = new OfflineOracle(oracle.root, { source: oracle.source, contract });
    const wrapped = new OfflineOracle(oracle.root, { source: oracle.source, contract });
    direct.restore(position);
    direct.main.context.__auditAction = contract.jsonCopy(action.payload);
    let raw;
    try { raw = direct.evaluate("applyAiAction(__auditAction)"); }
    finally { delete direct.main.context.__auditAction; }
    const expected = raw?.ok ? direct.snapshot() : null;
    const step = wrapped.apply(position, action, { recordHistory: false });
    assert.equal(step.ok, Boolean(raw?.ok));
    if (raw?.ok) {
      assert.deepEqual(step.position.state, expected.state);
      assert.deepEqual(step.position.rng, expected.rng);
      for (const viewer of ["white", "black"]) contract.validateObservation(wrapped.observe(step.position, viewer));
    } else assert.equal(step.position.positionId, position.positionId);
    result.sourceComparison.push({ actionType: action.payload.type, sourceAccepted: Boolean(raw?.ok), stateAndRngEqual: Boolean(raw?.ok) });
  }
  if (!page.actions.length) result.gap = "No action on this one synthetic position; reachability on other boards remains unproved.";
  else if (!page.exhausted) result.gap = "Only a bounded prefix of card targets was checked.";
  return result;
}
function ruleProbe(oracle, contract, style, cardId, seed) {
  const available = oracle.evaluate("ruleCardPool().map(card=>card.id)");
  if (!available.includes(cardId)) return {
    kind: "source-rule-unavailable", installed: false, sourcePool: false,
    gap: "Catalog RULE ID is absent from the current client's selectable ruleCardPool.",
  };
  const position = oracle.newGame({ gameStyle: style, draftDelete: true, ruleCardIds: [cardId] }, seed);
  const active = [position.state.appliedRuleCard?.id, ...(position.state.additionalRuleCards || []).map(card => card.id)];
  if (!active.includes(cardId)) throw new Error("Source did not install requested RULE card.");
  for (const viewer of ["white", "black"]) contract.validateObservation(oracle.observe(position, viewer));
  return { kind: "source-rule-setup", installed: true, mode: position.state.mode,
    gap: "RULE interactions and later turns were not exhaustively checked." };
}
function audit({ baseline = "site-20260927", root, modes = STYLES, actionSamples = 2,
  timeoutMs = 120000, shardIndex = 0, shardCount = 1, seed = 12345 } = {}) {
  if (!Array.isArray(modes) || !modes.length || modes.some(mode => !STYLES.includes(mode))) throw new TypeError("Unknown style.");
  if (!Number.isSafeInteger(actionSamples) || actionSamples < 1 || actionSamples > 16) throw new TypeError("actionSamples must be 1..16.");
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1) throw new TypeError("timeoutMs must be positive.");
  if (!Number.isSafeInteger(shardIndex) || !Number.isSafeInteger(shardCount) || shardCount < 1 || shardIndex < 0 || shardIndex >= shardCount) throw new TypeError("Invalid shard.");
  const contract = createRuntimeContract({ baseline });
  const source = new FrozenClientSource(sourceRoot(baseline, root), {
    expectedClientSha256: contract.catalog.source.files.find(file => /^main-/.test(file.name)).sha256,
  });
  const oracle = new OfflineOracle(source.root, { source, contract, maxCandidates: 100000 });
  const deadline = Date.now() + timeoutMs;
  const cards = contract.catalog.cards.map((card, index) => ({
    id: card.id, category: card.draftCategory, effect: card.effect,
    selected: index % shardCount === shardIndex, probes: {},
  }));
  const report = {
    schemaVersion: 2, baseline, sourceSha256: source.manifest.files.find(file => /^main-/.test(file.name)).sha256,
    rulesVersion: contract.catalog.rulesVersion, catalogVersion: contract.catalog.catalogVersion,
    profileVersion: contract.ORACLE_PROFILE_VERSION, styles: [...modes],
    scope: "bounded source execution on one seeded 8x8 play position per style; white selects the second source draft offer, black the first; injected cards are synthetic",
    completeRuleCoverage: false, actionSamplesPerCard: actionSamples,
    shardIndex, shardCount, cards, setupErrors: [], status: "running",
  };
  for (const style of modes) {
    let base;
    try { base = playBase(oracle, style, seed); }
    catch (error) { report.setupErrors.push({ style, ...errorInfo(error) }); continue; }
    for (const card of cards) {
      if (!card.selected) continue;
      if (Date.now() >= deadline) { card.probes[style] = { skipped: "time-budget" }; continue; }
      try {
        card.probes[style] = card.category === "RULE"
          ? ruleProbe(oracle, contract, style, card.id, seed)
          : cardProbe(oracle, contract, base, card, actionSamples);
      } catch (error) { card.probes[style] = { error: errorInfo(error) }; }
    }
  }
  report.selectedCards = cards.filter(card => card.selected).length;
  report.probedCells = cards.reduce((sum, card) => sum + Object.values(card.probes).filter(probe => !probe.skipped).length, 0);
  report.errorCells = cards.reduce((sum, card) => sum + Object.values(card.probes).filter(probe => probe.error).length, 0);
  report.noCandidateCells = cards.reduce((sum, card) => sum + Object.values(card.probes).filter(probe => probe.sampledCandidateCount === 0).length, 0);
  report.unprobedCells = report.selectedCards * modes.length - report.probedCells;
  report.status = report.setupErrors.length || report.errorCells ? "source-surface-errors"
    : report.unprobedCells ? "bounded-probe-incomplete" : "bounded-probe-complete-with-reachability-gaps";
  return report;
}
function parseArgs(argv) {
  const options = {};
  for (const arg of argv) {
    const [name, value] = arg.split("=", 2);
    if (name === "--baseline") options.baseline = value;
    else if (name === "--root") options.root = value;
    else if (name === "--modes") options.modes = value.split(",");
    else if (name === "--samples") options.actionSamples = Number(value);
    else if (name === "--timeout-ms") options.timeoutMs = Number(value);
    else if (name === "--shard") {
      const match = /^(\d+)\/(\d+)$/.exec(value || "");
      if (!match) throw new TypeError("Use --shard=index/count.");
      options.shardIndex = Number(match[1]); options.shardCount = Number(match[2]);
    } else throw new TypeError("Unknown audit option: " + name);
  }
  return options;
}
module.exports = { audit };
if (require.main === module) {
  try {
    const report = audit(parseArgs(process.argv.slice(2)));
    const parent = process.env.RUNNER_TEMP || process.env.APPDATA;
    if (!parent) throw new Error("Report root requires RUNNER_TEMP or APPDATA.");
    const output = path.join(parent, "Accelerate", "reports", "site-adapter");
    fs.mkdirSync(output, { recursive: true });
    const file = path.join(output, "card-surface-" + report.baseline + "-shard-" + report.shardIndex + "-of-" + report.shardCount + ".json");
    fs.writeFileSync(file, JSON.stringify(report, null, 2) + "\n");
    console.log(JSON.stringify({ status: report.status, selectedCards: report.selectedCards,
      probedCells: report.probedCells, errorCells: report.errorCells,
      noCandidateCells: report.noCandidateCells, unprobedCells: report.unprobedCells, report: file }));
    const failures = [
      ...report.setupErrors.map(error => ({ kind: "setup", style: error.style, name: error.name, message: error.message })),
      ...report.cards.flatMap(card => Object.entries(card.probes).filter(([, probe]) => probe.error)
        .map(([style, probe]) => ({ kind: "card", cardId: card.id, style,
          name: probe.error.name, message: probe.error.message }))),
    ];
    for (const failure of failures.slice(0, 20)) console.error(JSON.stringify(failure));
    if (failures.length > 20) console.error(JSON.stringify({
      kind: "remaining-errors", count: failures.length - 20, report: file,
    }));
    if (report.unprobedCells) console.error(JSON.stringify({
      kind: "unprobed-cells", count: report.unprobedCells, report: file,
    }));
    if (failures.length || report.unprobedCells) process.exitCode = 1;
  } catch (error) { console.error(error.stack); process.exitCode = 1; }
}
