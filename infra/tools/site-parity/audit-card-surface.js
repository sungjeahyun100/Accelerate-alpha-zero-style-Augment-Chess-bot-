#!/usr/bin/env node
"use strict";
const fs = require("node:fs");
const path = require("node:path");
const { OfflineOracle } = require("./offline-oracle");
const contract = require("../../../bridge/tools/runtime-contract");

// A bounded API-surface probe, not a proof that every card rule is correct.
function audit({ actionSamples = 2, timeoutMs = 120000 } = {}) {
  const oracle = new OfflineOracle(undefined, { maxCandidates: 100000 });
  const deadline = Date.now() + timeoutMs;
  const report = { schemaVersion: 1, rulesVersion: contract.catalog.rulesVersion, catalogVersion: contract.catalog.catalogVersion, scope: "initial-standard-board-only", completeRuleCoverage: false, actionSamplesPerCard: actionSamples, cards: [], errors: [] };
  for (const definition of contract.catalog.cards) {
    if (Date.now() >= deadline) { report.errors.push({ kind: "time-budget", remainingCards: contract.catalog.cards.length - report.cards.length }); break; }
    try {
      const initial = oracle.newGame({}, 12345);
      oracle.restore(initial);
      oracle.main.context.__effect = definition.effect;
      oracle.evaluate("state.mode='play';state.turn='white';state.draftLocked=false;state.deckSlots.white=[cloneCard(CARD_DEFS.find(card=>card.effect===__effect)),null,null];state.deckSlots.black=[null,null,null];state.deck.white=state.deckSlots.white.filter(Boolean);state.deck.black=[];state.playerCards=state.deckSlots;");
      const p = oracle.snapshot(), raw = oracle.candidates(p).filter(payload => payload.type === "card" && payload.cardId === definition.id);
      const sampled = raw.slice(0, actionSamples).map(payload => {
        const step = oracle.apply(p, contract.action(p, payload));
        if (step.ok) { oracle.observe(step.position, "white"); oracle.observe(step.position, "black"); }
        return { accepted: step.ok, result: step.result.status };
      });
      report.cards.push({ id: definition.id, effect: definition.effect, candidates: raw.length, sampled });
    } catch (error) { report.cards.push({ id: definition.id, effect: definition.effect, error: error.message }); report.errors.push({ id: definition.id, message: error.message }); }
  }
  report.cardsProbed = report.cards.length;
  report.cardsWithCandidate = report.cards.filter(card => card.candidates > 0).length;
  report.status = report.errors.length ? "surface-gaps" : "bounded-surface-probe-passed";
  return report;
}
module.exports = { audit };
if (require.main === module) {
  const report = audit();
  const parent = process.env.RUNNER_TEMP || process.env.APPDATA;
  if (!parent) throw new Error("Report root requires RUNNER_TEMP or APPDATA.");
  const root = path.join(parent, "Accelerate", "reports", "full-stack-site");
  fs.mkdirSync(root, { recursive: true });
  const target = path.join(root, "card-surface-audit.json");
  fs.writeFileSync(target, JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify({ status: report.status, cardsProbed: report.cardsProbed, cardsWithCandidate: report.cardsWithCandidate, errors: report.errors, report: target }, null, 2));
  if (report.errors.length) process.exitCode = 1;
}
