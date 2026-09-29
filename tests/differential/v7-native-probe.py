"""Compare a pinned JS v7 probe batch with an installed PyO3 engine.

The Node coordinator owns source generation. This worker never reconstructs
oracle state or treats a missing/unsupported native engine as a skipped test.
"""

from __future__ import annotations

import json
import math
import sys
from collections import Counter
from typing import Any


PAGE_SIZE = 64
MAX_PAGES = 128
MAX_ACTIONS = 4096
MAX_EXAMINED = 65536


def difference(expected: Any, actual: Any, path: str = "$", depth: int = 0) -> str | None:
    if depth > 64:
        return f"{path}: comparison depth exceeded"
    if isinstance(expected, bool) or isinstance(actual, bool):
        return None if type(expected) is type(actual) and expected == actual else path
    if isinstance(expected, (int, float)) and isinstance(actual, (int, float)):
        return None if math.isfinite(expected) and math.isfinite(actual) and expected == actual else path
    if type(expected) is not type(actual):
        return path
    if isinstance(expected, dict):
        if expected.keys() != actual.keys():
            missing = sorted(expected.keys() - actual.keys())
            extra = sorted(actual.keys() - expected.keys())
            return f"{path}: keys missing={missing[:3]} extra={extra[:3]}"
        for key in sorted(expected):
            found = difference(expected[key], actual[key], f"{path}.{key}", depth + 1)
            if found:
                return found
        return None
    if isinstance(expected, list):
        if len(expected) != len(actual):
            return f"{path}: length {len(expected)} != {len(actual)}"
        for index, (left, right) in enumerate(zip(expected, actual)):
            found = difference(left, right, f"{path}[{index}]", depth + 1)
            if found:
                return found
        return None
    return None if expected == actual else path


def failure(status: str, reason: str, **details: Any) -> dict[str, Any]:
    return {"status": status, "reason": reason, **details}


def collect_actions(position: Any) -> tuple[list[dict[str, Any]], int]:
    stream = position.action_stream()
    actions: list[dict[str, Any]] = []
    examined = 0
    for _ in range(MAX_PAGES):
        page = stream.next_page(PAGE_SIZE)
        if not isinstance(page["examined"], int) or page["examined"] < 0:
            raise RuntimeError("native action stream returned invalid examined count")
        examined += page["examined"]
        actions.extend(action.snapshot() for action in page["actions"])
        if len(actions) > MAX_ACTIONS or examined > MAX_EXAMINED:
            raise RuntimeError("native legal action stream exceeded probe budget")
        if page["exhausted"]:
            return actions, examined
        if not page["actions"] and page["examined"] == 0:
            raise RuntimeError("native legal action stream made no progress")
    raise RuntimeError("native legal action stream did not exhaust within page budget")


def check_rejection(position: Any, payload: dict[str, Any], native: Any) -> dict[str, Any] | None:
    before = position.snapshot()
    try:
        action = position.bind_action(payload)
        position.apply(action)
    except native.UnsupportedFeatureError as exc:
        return failure("unsupported", f"native rejection path unsupported: {exc}")
    except (native.NativeError, ValueError) as exc:
        changed = difference(before, position.snapshot())
        return failure("mismatch", "rejected action mutated native position", path=changed) if changed else None
    return failure("mismatch", "native accepted source-rejected wrong-actor action")


def compare_case(case: dict[str, Any], native: Any) -> dict[str, Any]:
    result: dict[str, Any] = {"name": case["name"], "mode": case["position"]["state"]["mode"]}
    try:
        position = native.Position.from_snapshot(case["position"])
    except native.UnsupportedFeatureError as exc:
        return {**result, **failure("unsupported", f"native v7 import unsupported: {exc}")}
    except Exception as exc:  # Preserve the actual import gate; never call it a skip.
        return {**result, **failure("import-error", f"{type(exc).__name__}: {exc}")}

    changed = difference(case["position"], position.snapshot())
    if changed:
        return {**result, **failure("mismatch", "imported full position differs", path=changed)}
    if position.result != case["result"]["outcome"]:
        return {**result, **failure("mismatch", "baseline result differs", expected=case["result"]["outcome"], actual=position.result)}

    try:
        actual, examined = collect_actions(position)
    except native.UnsupportedFeatureError as exc:
        return {**result, **failure("unsupported", f"native v7 legal stream unsupported: {exc}")}
    except Exception as exc:
        return {**result, **failure("legal-error", f"{type(exc).__name__}: {exc}")}
    expected = case["actions"]
    expected_ids = Counter(action["actionId"] for action in expected)
    actual_ids = Counter(action["actionId"] for action in actual)
    if expected_ids != actual_ids:
        return {**result, **failure("mismatch", "full legal action multiset differs", sourceCount=len(expected), nativeCount=len(actual), nativeExamined=examined, missingIds=list((expected_ids - actual_ids).elements())[:3], extraIds=list((actual_ids - expected_ids).elements())[:3])}
    expected_by_id = {action["actionId"]: action for action in expected}
    for action in actual:
        changed = difference(expected_by_id[action["actionId"]], action)
        if changed:
            return {**result, **failure("mismatch", "legal action envelope differs", actionId=action["actionId"], path=changed)}

    rejected = check_rejection(position, case["rejectPayload"], native)
    if rejected:
        return {**result, **rejected}

    for index, sample in enumerate(case["samples"]):
        try:
            action = position.bind_snapshot(sample["action"])
            step = position.apply(action)
        except native.UnsupportedFeatureError as exc:
            return {**result, **failure("unsupported", f"native v7 apply unsupported: {exc}", sample=index)}
        except Exception as exc:
            return {**result, **failure("apply-error", f"{type(exc).__name__}: {exc}", sample=index)}
        changed = difference(sample["position"], step.position.snapshot())
        if changed:
            return {**result, **failure("mismatch", "applied full position/history/RNG differs", sample=index, path=changed)}
        expected_event = sample["position"]["history"][-1]
        if step.actor != expected_event["actor"] or step.turn_changed != expected_event["turnChanged"]:
            return {**result, **failure("mismatch", "applied actor/turn change differs", sample=index)}
        if step.result != sample["result"]["outcome"] or step.position.result != sample["result"]["outcome"]:
            return {**result, **failure("mismatch", "applied result differs", sample=index)}
        if difference(case["position"], position.snapshot()):
            return {**result, **failure("mismatch", "apply mutated source native position", sample=index)}
    return {**result, "status": "pass", "legalCount": len(actual), "nativeExamined": examined, "applied": len(case["samples"]), "rejected": 1}


def main() -> None:
    request = json.load(sys.stdin)
    try:
        import accelerate_chess._native as native
    except (ImportError, OSError) as exc:
        print(json.dumps(failure("native-unavailable", f"{type(exc).__name__}: {exc}")))
        return
    try:
        catalog = native.site_catalog(request["rulesVersion"])
    except Exception as exc:
        print(json.dumps(failure("native-unsupported", f"v7 catalog unavailable: {type(exc).__name__}: {exc}")))
        return
    if catalog.get("rulesVersion") != request["rulesVersion"] or catalog.get("catalogVersion") != request["catalogVersion"]:
        print(json.dumps(failure("version-mismatch", "native v7 catalog identity differs", nativeRulesVersion=catalog.get("rulesVersion"), nativeCatalogVersion=catalog.get("catalogVersion"))))
        return
    if request["phase"] == "preflight":
        print(json.dumps({"status": "ready", "nativeModule": native.__file__}))
        return
    cases = [compare_case(case, native) for case in request["cases"]]
    status = "pass" if all(case["status"] == "pass" for case in cases) else "fail"
    print(json.dumps({"status": status, "nativeModule": native.__file__, "cases": cases}, allow_nan=False))


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(json.dumps(failure("probe-error", f"{type(exc).__name__}: {exc}")))
