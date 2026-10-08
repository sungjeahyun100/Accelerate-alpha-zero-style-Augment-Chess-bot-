#!/usr/bin/env node
"use strict";

const assert = require("node:assert/strict");
const vm = require("node:vm");
const { loadSource } = require("./october-source-probe");

const FIXTURE_IDS = Object.freeze(["metal", "qxe1", "taunt"]);
const gcd = (a, b) => b === 0n ? a : gcd(b, a % b);
function fraction(numerator, denominator) {
  const divisor = gcd(numerator, denominator);
  return { numerator: numerator / divisor, denominator: denominator / divisor };
}
function multiply(left, numerator, denominator) {
  return fraction(left.numerator * BigInt(numerator), left.denominator * BigInt(denominator));
}
function add(left, right) {
  return fraction(left.numerator * right.denominator + right.numerator * left.denominator,
    left.denominator * right.denominator);
}

function exactDistribution(sourcePath, parserPath) {
  const context = loadSource(sourcePath, parserPath);
  context.__fixedRandom = () => 0.5;
  vm.runInContext("Math.random=__fixedRandom;selectedGameStyle='normal';localPlayMode='local';playMode='local';resetGame(false,[]);beginInitialGameFlow();state.completeRandom=true;", context, { timeout: 15000 });
  assert.equal(vm.runInContext("usesOctober7Balance(state)", context), true, "Fixture did not enter October balance.");
  context.__fixtureIds = FIXTURE_IDS;
  const cards = JSON.parse(vm.runInContext("JSON.stringify(__fixtureIds.map(id=>{const card=CARD_DEFS.find(item=>item.id===id);if(!card)throw new Error('Missing source card '+id);return {id,stars:card.stars,weightUnits:draftCardWeight(card)*8};}))", context));
  const units = new Map(cards.map(card => {
    assert.ok(Number.isSafeInteger(card.weightUnits) && card.weightUnits > 0,
      `Source weight is not an exact eighth for ${card.id}.`);
    return [card.id, card.weightUnits];
  }));
  const conflict = (id, selected) => {
    context.__candidateId = id;
    context.__selectedIds = selected;
    return vm.runInContext("hasLatestMutuallyExclusiveDraftCard(__candidateId,new Set(__selectedIds))", context);
  };
  assert.equal(conflict("metal", ["qxe1"]), true);
  assert.equal(conflict("qxe1", ["metal"]), true);
  assert.equal(conflict("taunt", ["metal"]), false);
  const outcomes = new Map();
  let verifiedPaths = 0;
  function visit(remaining, selected, tape, probability) {
    if (selected.length === 2 || remaining.length === 0) {
      context.__tape = tape.slice();
      vm.runInContext("Math.random=()=>{if(!__tape.length)throw new Error('Source consumed unexpected RNG');return __tape.shift();}", context);
      const actual = JSON.parse(vm.runInContext("JSON.stringify(drawCompatibleDraftCards(__fixtureIds.map(id=>CARD_DEFS.find(card=>card.id===id)),2).map(card=>card.id))", context, { timeout: 15000 }));
      assert.deepEqual(actual, selected, `Source branch disagreed with exact enumeration: ${tape.join(',')}`);
      assert.equal(context.__tape.length, 0, "Source did not consume the expected weighted and identity draws.");
      const key = selected.join(",");
      outcomes.set(key, add(outcomes.get(key) || fraction(0n, 1n), probability));
      verifiedPaths++;
      return;
    }
    const total = remaining.reduce((sum, id) => sum + units.get(id), 0);
    let preceding = 0;
    for (const id of remaining) {
      const weight = units.get(id);
      // A midpoint strictly inside this card's interval avoids floating-point
      // boundary ties while replaying the original weightedChoice implementation.
      const roll = (preceding + weight / 2) / total;
      const accepted = !conflict(id, selected);
      visit(remaining.filter(candidate => candidate !== id), accepted ? [...selected, id] : selected,
        [...tape, roll, ...(accepted ? [0.5] : [])], multiply(probability, weight, total));
      preceding += weight;
    }
  }
  visit([...FIXTURE_IDS], [], [], fraction(1n, 1n));
  const total = [...outcomes.values()].reduce(add, fraction(0n, 1n));
  assert.deepEqual(total, fraction(1n, 1n), "Exact outcome mass is not one.");
  const conditionalOnFirstAccepted = {};
  for (const first of FIXTURE_IDS) {
    const matching = [...outcomes].filter(([key]) => key.startsWith(first + ","));
    const mass = matching.reduce((sum, [, value]) => add(sum, value), fraction(0n, 1n));
    if (mass.numerator === 0n) continue;
    conditionalOnFirstAccepted[first] = Object.fromEntries(matching.map(([key, value]) => {
      const conditional = fraction(value.numerator * mass.denominator, value.denominator * mass.numerator);
      return [key.split(",")[1], `${conditional.numerator}/${conditional.denominator}`];
    }));
  }
  return { fixtureCardIds: FIXTURE_IDS, drawCount: 2, cards, verifiedPaths,
    outcomes: Object.fromEntries([...outcomes].sort(([a], [b]) => a.localeCompare(b)).map(([key, value]) =>
      [key, `${value.numerator}/${value.denominator}`])), conditionalOnFirstAccepted };
}

if (require.main === module) {
  try {
    const [sourcePath, parserPath] = process.argv.slice(2);
    if (!sourcePath || !parserPath) throw new Error("Usage: october-draft-distribution.js ABSOLUTE_MAIN_PATH ABSOLUTE_ACORN_PATH");
    console.log(JSON.stringify(exactDistribution(sourcePath, parserPath), null, 2));
  } catch (error) { console.error(error.stack || error); process.exitCode = 1; }
}
module.exports = { exactDistribution };
