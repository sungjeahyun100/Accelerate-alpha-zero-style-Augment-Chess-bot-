import test from 'node:test';
import assert from 'node:assert/strict';
import definitions from '../../../augment-chess/contracts/catalog/card-definitions-20260928.json' with {type: 'json'};
import {buildCardIntent, buildMoveIntent, getCardSelectionForm} from '../src/card-selection.ts';
import {cardView} from '../src/presentation.ts';
const card = (id: string) => cardView({id, instanceId: `public-${id}`});
test('frozen card forms cover definitions; compound fields and ordered choices preserve public wire semantics', () => {
  for (const definition of definitions.definitions) assert.doesNotThrow(() => getCardSelectionForm(definition.id), definition.id);
  const first = {row: 1, col: 2}, second = {row: 4, col: 5};
  assert.deepEqual(buildCardIntent(card('amazon'), 'white', {target: first, knight: second}).target, {...first, knight: second});
  assert.deepEqual(buildCardIntent(card('premove'), 'white', {selections: [first, second]}).target, {selections: [{from: first, to: second}]});
  assert.deepEqual(buildCardIntent(card('pawn-storm'), 'white', {selections: [second, first]}).target, {selections: [second, first]});
  assert.throws(() => buildCardIntent(card('premove'), 'white', {selections: [first, second, first]}), /한 쌍/);
  assert.deepEqual(buildMoveIntent('white', first, second, 'snipe'), {type: 'move', color: 'white', from: first, destination: second, selectionMode: 'snipe'});
});
