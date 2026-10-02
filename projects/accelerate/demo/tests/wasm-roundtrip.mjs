import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {demoPaths} from '../scripts/paths.mjs';
const paths = demoPaths();
const fixtures = JSON.parse(await readFile(join(paths.reports, 'native-fixtures.json'), 'utf8'));
assert.equal(fixtures.schemaVersion, 1);
const binding = await import(pathToFileURL(join(paths.wasm, 'augment_chess_browser.js')).href);
await binding.default({module_or_path: await readFile(join(paths.wasm, 'augment_chess_browser_bg.wasm'))});
assert.equal('browser_test_case' in binding, false, 'Production WASM must omit test state constructors');
const testBinding = await import(pathToFileURL(join(paths.wasmTest, 'augment_chess_browser.js')).href);
await testBinding.default({module_or_path: await readFile(join(paths.wasmTest, 'augment_chess_browser_bg.wasm'))});
let steps = 0;
for (const reference of fixtures.cases) {
  const game = reference.fixtureId ? testBinding.browser_test_case(reference.fixtureId) : binding.BrowserGameSession.new_game(JSON.stringify(reference.config), reference.seed);
  if (reference.fixtureId) assert.equal(reference.semanticWitness, true, reference.fixtureId);
  try {
    for (const step of reference.steps) {
      const actual = JSON.parse(game.invoke_json(JSON.stringify(step.request), 30000));
      assert.deepEqual(actual, step.outcome, `${reference.fixtureId ?? reference.config.gameStyle}: ${step.request.capabilityId}`); steps++;
    }
    assert.equal(game.revision(), reference.finalRevision);
    assert.equal(game.decision_actor(), reference.finalDecisionActor);
    assert.equal(game.result() ?? null, reference.finalResult);
  } finally { game.free(); }
}
console.log(JSON.stringify({stage: 'native-wasm-parity', cases: fixtures.cases.length, steps, productionFixtureExport: false}));
