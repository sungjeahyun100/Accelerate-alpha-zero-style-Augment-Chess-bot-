"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { inspectNativeComparison } = require("./v7-native-differential.cjs");

const expected = [{ name: "opening" }, { name: "after-move" }];
const passed = expected.map(item => ({ ...item, status: "pass" }));

test("전체 ordered 결과만 성공으로 인수한다", () => {
  assert.deepEqual(inspectNativeComparison({ status: "pass", cases: passed }, expected),
    { cases: passed, failure: null });
  for (const cases of [passed.slice(0, 1), [...passed, passed[0]], [...passed].reverse()]) {
    assert.equal(inspectNativeComparison({ status: "pass", cases }, expected).failure.status, "probe-error");
  }
});

test("실행 전 오류의 정확한 종류와 원래 진단을 보존한다", () => {
  for (const status of ["native-timeout", "native-unavailable", "native-unsupported", "probe-error", "version-mismatch"]) {
    const reason = `${status}: worker stage failed`;
    const result = inspectNativeComparison({ status, reason }, expected);
    assert.equal(result.failure.status, status);
    assert.equal(result.failure.reason, reason);
    assert.equal(result.failure.observedCases, 0);
  }
  assert.equal(inspectNativeComparison({ status: "native-timeout", reason: " " }, expected).failure.status,
    "probe-error");
});

test("집계와 사례의 모순·잘못된 상태를 성공이나 정상 실패로 위장하지 않는다", () => {
  for (const response of [
    { status: "fail", cases: passed },
    { status: "pass", cases: [passed[0], { ...passed[1], status: "mismatch", reason: "state differs" }] },
    { status: "unknown", cases: passed },
    { status: "fail", cases: [passed[0], { name: "after-move" }] },
    null,
  ]) assert.equal(inspectNativeComparison(response, expected).failure.status, "probe-error");
});

test("실제 사례 실패와 원문 메시지는 전체 실패로 전파한다", () => {
  const result = inspectNativeComparison({ status: "fail", cases: [passed[0],
    { name: "after-move", status: "mismatch", reason: "state.board[5][0] differs" }] }, expected);
  assert.equal(result.failure.status, "fail");
  assert.equal(result.failure.reason, "state.board[5][0] differs");
  assert.equal(result.failure.expectedCases, 2);
  assert.equal(result.failure.observedCases, 2);
});
