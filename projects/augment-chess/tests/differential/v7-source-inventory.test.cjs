"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { collectInventory } = require("./v7-source-inventory.cjs");

test("pinned v7 definitions, effective draft categories, activation and RULE pool agree with catalog", () => {
  const report = collectInventory();
  assert.equal(report.sourceSha256, "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c");
  assert.equal(report.summary.publicCards, 256);
  assert.equal(report.summary.sourceDefinitions, 257);
  assert.deepEqual(report.summary.extraSourceDefinitions, ["shotgun-king"]);
  assert.deepEqual(report.auxiliaryDefinitions.map(card => card.id), ["shotgun-king"]);
  assert.equal(report.auxiliaryDefinitions[0].publicCatalogEntry, false);
  assert.equal(report.summary.sourceRulePool, 27);
  assert.equal(report.summary.sourceRulePoolWithoutDeathmatch, 26);
  assert.deepEqual(report.summary.deathmatchConditionalRuleIds, ["revelation"]);
  assert.deepEqual(report.summary.catalogRulesAbsentFromSourcePool, []);
  assert.deepEqual(report.identityGaps, [], report.identityGaps.join("\n"));
});

test("inventory never promotes definitions or synthetic surface checks to verified reachability", () => {
  const report = collectInventory();
  assert.equal(report.completeRuleCoverage, false);
  assert.equal(report.witnesses.length, 0);
  assert.deepEqual(report.witnessErrors, []);
  assert.ok(report.cards.every(card => card.effectParity === "unverified-by-this-inventory"));
  assert.ok(report.cards.every(card => card.openingOfferWitness === false));
  assert.ok(report.cards.every(card => card.sourceTransitionEvidence === "unproven"));
  assert.equal(report.capabilityEvidence.aiNoCardsThreat, "not-verified-by-this-inventory");
  assert.equal(report.capabilityEvidence.publicIntent, "not-verified-by-this-inventory");
  const scope = report.acceptanceObligations;
  assert.deepEqual(scope.styles.map(item => item.style), ["normal", "chaos", "grand"]);
  assert.equal(scope.board.rows, 8);
  assert.equal(scope.board.columns, 8);
  assert.deepEqual(scope.publicCards.map(card => card.id), report.cards.map(card => card.id));
  assert.deepEqual(scope.pieceTypes.map(piece => piece.id), report.pieces.map(piece => piece.id));
  assert.equal(scope.rules.length, 27);
  assert.deepEqual(scope.rules.filter(rule => !rule.sourcePoolMemberWithoutDeathmatch).map(rule => rule.id), ["revelation"]);
  assert.equal(scope.auxiliaryDefinitions[0].effectAndSettlementParity, "unverified-by-this-inventory");
  assert.ok(scope.publicCards.every(card => card.interactionBranchParity === "unverified-by-this-inventory"));
  assert.equal(scope.actionTypes.length, report.summary.actionTypesDeclared);
  assert.ok(scope.actionTypes.every(item => item.completeOrderedLegalParity === "unverified-by-this-inventory"));
  assert.deepEqual(scope.uiChoices.map(item => item.source), report.summary.sourceUiChoicesDeclared);
  assert.equal(scope.phasesAndTerminalBranches, "not-enumerated-or-proven-by-this-inventory");
});

test("source-driven seed 20260928 draft settlement preserves bounded target semantics", () => {
  const report = collectInventory({ witnessSeeds: [20260928] });
  assert.deepEqual(report.witnessErrors, [], JSON.stringify(report.witnessErrors));
  assert.equal(report.witnesses.length, 3);
  assert.ok(report.witnesses.every(witness => witness.firstPlayExamined <= 256));
});
