#!/usr/bin/env node
"use strict";
const fs = require("node:fs");
const path = require("node:path");
const { loadWorker, cacheRoot } = require("./frozen-site");
const { canonical } = require("../../../bridge/tools/runtime-contract");

// Legacy fixture comparison strips incidental IDs only; runtime execution never does.
function normalized(value) {
  if (Array.isArray(value)) return value.map(normalized);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).filter(([key]) => !["id", "instanceId", "pieceId"].includes(key)).map(([key, child]) => [key, normalized(child)]));
  return value;
}
function patchState(before, delta) {
  if (Object.hasOwn(delta, "=")) return structuredClone(delta["="]);
  if (delta.a) {
    const after = structuredClone(before);
    for (const [index, change] of Object.entries(delta.a)) after[Number(index)] = patchState(after[Number(index)], change);
    return after;
  }
  if (delta.o || delta.d) {
    const after = structuredClone(before || {});
    for (const key of delta.d || []) delete after[key];
    for (const [key, change] of Object.entries(delta.o || {})) after[key] = patchState(after[key], change);
    return after;
  }
  throw new TypeError("Invalid fixture state delta.");
}
function compare(root = cacheRoot(), fixtureRoot = path.resolve(__dirname, "../../../tests/differential/fixtures/site-reference-v1")) {
  const loaded = loadWorker(root);
  const metadata = JSON.parse(fs.readFileSync(path.join(fixtureRoot, "meta.json"), "utf8"));
  const latestWorker = loaded.manifest.files.find(file => file.name === "aiWorker.raw.js");
  const report = { schemaVersion: 1, fixtureVersion: metadata.format, baselineWorkerSha256: latestWorker.sha256, fixtureWorkerSha256: metadata.site.aiWorkerSha256, sameWorker: latestWorker.sha256 === metadata.site.aiWorkerSha256, fixtures: 0, actionsCompared: 0, actionListMatches: 0, appliedFullStateMatches: 0, appliedResultMatches: 0, actionAcceptanceMatches: 0, failures: [], gaps: ["Legacy fixtures contain worker AI-filtered candidates, not full client legal actions.", "Legacy fixtures do not cover real game initialization/drafting or public observation/history.", "Legacy fixture RNG tapes are absent, so stochastic transition parity cannot be asserted."] };
  const fixtureCardEffects = new Set(), fixturePieces = new Set();
  for (const name of ["pieces.jsonl", "cards.jsonl", "playouts.jsonl", "gameover.jsonl"]) {
    const text = fs.readFileSync(path.join(fixtureRoot, name), "utf8");
    for (const line of text.split(/\r?\n/).filter(Boolean)) {
      const fixture = JSON.parse(line);
      report.fixtures++;
      for (const row of fixture.state.board) for (const cell of row) if (cell) fixturePieces.add(cell.type);
      for (const cards of Object.values(fixture.state.deckSlots || {})) for (const card of cards) if (card) fixtureCardEffects.add(card.effect);
      try {
        loaded.api.setWorkerBoardDimensions(8, 8);
        const rawActions = loaded.api.generateActions(loaded.api.cloneState(fixture.state), fixture.color);
        const actual = [...new Set(rawActions.map(action => JSON.stringify(normalized(action))))].sort();
        const equalActions = canonical(actual) === canonical(fixture.expected.legalActions);
        if (equalActions) report.actionListMatches++;
        else report.failures.push({ id: fixture.id, source: fixture.source, kind: "action-list", expected: fixture.expected.legalActions.length, actual: actual.length });
        for (const expected of fixture.expected.applied) {
          report.actionsCompared++;
          // The original generator applies actions to worker cloneState, whose
          // defaults and shared large-piece identity are part of the reference.
          const state = loaded.api.cloneState(fixture.state);
          const result = loaded.api.applyAction(state, structuredClone(expected.action), fixture.color);
          if (!!result.ok === expected.ok) report.actionAcceptanceMatches++;
          else report.failures.push({ id: fixture.id, source: fixture.source, kind: "acceptance", action: expected.key });
          if (expected.result && canonical(normalized(JSON.parse(JSON.stringify(result)))) === canonical(normalized(expected.result))) report.appliedResultMatches++;
          else report.failures.push({ id: fixture.id, source: fixture.source, kind: "full-result", action: expected.key });
          if (expected.stateDelta) {
            const reference = patchState(fixture.state, expected.stateDelta);
            if (canonical(normalized(state)) === canonical(normalized(reference))) report.appliedFullStateMatches++;
            else report.failures.push({ id: fixture.id, source: fixture.source, kind: "full-state", action: expected.key, keys: [...new Set([...Object.keys(reference), ...Object.keys(state)])].filter(key => canonical(normalized(reference[key] ?? null)) !== canonical(normalized(state[key] ?? null))) });
          } else if (!expected.ok && canonical(normalized(state)) !== canonical(normalized(fixture.state))) {
            report.failures.push({ id: fixture.id, source: fixture.source, kind: "rejected-state-mutated", action: expected.key });
          }
        }
      } catch (error) { report.failures.push({ id: fixture.id, source: fixture.source, kind: "execution-error", message: error.message }); }
    }
  }
  const catalog = require("../../../bridge/catalog/site-20260927.json");
  report.catalog = { latestCards: catalog.cards.length, observedLegacyEffects: fixtureCardEffects.size, unobservedEffects: catalog.cards.filter(card => !fixtureCardEffects.has(card.effect)).map(card => card.effect), observedLegacyPieces: fixturePieces.size, unobservedPieceTypes: catalog.pieceTypes.filter(type => !fixturePieces.has(type)) };
  report.status = report.sameWorker && !report.failures.length ? "compatible-worker-reference" : "historical-reference-only";
  return report;
}
module.exports = { normalized, patchState, compare };
if (require.main === module) {
  try {
    const report = compare(process.argv[2] || cacheRoot());
    const parent = process.env.RUNNER_TEMP || process.env.APPDATA;
    if (!parent) throw new Error("Report root requires RUNNER_TEMP or APPDATA.");
    const root = path.join(parent, "Accelerate", "reports", "full-stack-site");
    fs.mkdirSync(root, { recursive: true });
    const target = path.join(root, "legacy-fixture-comparison.json");
    fs.writeFileSync(target, JSON.stringify(report, null, 2) + "\n");
    console.log(JSON.stringify({ status: report.status, fixtures: report.fixtures, actionsCompared: report.actionsCompared, actionListMatches: report.actionListMatches, appliedFullStateMatches: report.appliedFullStateMatches, appliedResultMatches: report.appliedResultMatches, actionAcceptanceMatches: report.actionAcceptanceMatches, catalog: report.catalog, report: target }, null, 2));
  } catch (error) { console.error(error.stack); process.exitCode = 1; }
}
