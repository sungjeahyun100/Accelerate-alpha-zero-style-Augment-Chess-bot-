"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const { FrozenClientSource, GameAdapter } = require("../../../packages/game-adapter/src");
const contract = require("../../../bridge/tools/runtime-contract").createRuntimeContract({ baseline: "site-20260928" });

const EXPECTED_CLIENT_SHA256 = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
const parent = process.env.RUNNER_TEMP || process.env.APPDATA;
const root = process.env.ACCELERATE_SITE_BASELINE || (parent && path.join(parent, "Accelerate", "cache", "site-baseline-20260928-e5ed84fc"));

test("draft cursor pages preserve all three modes' candidates and source transition", () => {
  if (!root || !path.isAbsolute(root)) throw new Error("An absolute frozen client cache is required.");
  const source = new FrozenClientSource(root, { expectedClientSha256: EXPECTED_CLIENT_SHA256 });
  const adapter = new GameAdapter({ source, contract });
  for (const [style, expectedCount] of [["normal", 3], ["chaos", 3], ["grand", 28]]) {
    const position = adapter.newGame({ gameStyle: style }, 37);
    assert.equal(position.state.mode, "draft");
    const expected = adapter.actions(position);
    assert.equal(expected.length, expectedCount);
    const before = adapter.apply(position, expected[0]);
    assert.equal(before.ok, true);
    assert.deepEqual(before.result, adapter.result(before.position));
    for (const viewer of ["white", "black"]) {
      assert.deepEqual(before.position.history.at(-1).public[viewer].result, before.result);
    }

    const left = adapter.actionStream(position);
    const right = adapter.actionStream(position);
    const leftActions = [], rightActions = [];
    let leftDone = false, rightDone = false;
    for (let pageCount = 0; pageCount < 40 && (!leftDone || !rightDone); pageCount++) {
      if (!leftDone) {
        const page = left.nextPage(2, { maxExamined: 2 });
        assert.ok(page.examined <= 2);
        leftActions.push(...page.actions);
        leftDone = page.exhausted;
      }
      if (!rightDone) {
        const page = right.nextPage(3, { maxExamined: 3 });
        assert.ok(page.examined <= 3);
        rightActions.push(...page.actions);
        rightDone = page.exhausted;
      }
    }
    assert.equal(leftDone, true, `${style}: left cursor did not exhaust`);
    assert.equal(rightDone, true, `${style}: right cursor did not exhaust`);
    assert.deepEqual(leftActions, expected, `${style}: left cursor changed order or content`);
    assert.deepEqual(rightActions, expected, `${style}: right cursor changed order or content`);
    assert.equal(position.positionId, leftActions[0].positionId);
    assert.deepEqual(adapter.apply(position, expected[0]), before,
      `${style}: paging changed the source transition or RNG`);
    left.dispose(); right.dispose();
  }
  adapter.dispose();
});
