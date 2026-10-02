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
SOURCE_SHA256 = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
PROFILE = "accelerate-headless-semantic-v7"


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
            if len({action["actionId"] for action in actions}) != len(actions):
                raise RuntimeError("native legal action stream emitted duplicate action identities")
            return actions, examined
        if not page["actions"] and page["examined"] == 0:
            raise RuntimeError("native legal action stream made no progress")
    raise RuntimeError("native legal action stream did not exhaust within page budget")


def full_result(position: Any) -> dict[str, Any]:
    state = position.snapshot()["state"]
    terminal = state["mode"] == "gameover"
    winner = state.get("winner") if terminal and state.get("winner") in ("white", "black") else None
    return {
        "protocolVersion": "accelerate-result-v1",
        "status": "terminal" if terminal else "ongoing",
        "winner": winner,
        "outcome": (state.get("winner") or "draw") if terminal else None,
        "reason": (state.get("replayEndReason") or "") if terminal else "",
    }


def compare_observations(position: Any, expected: dict[str, Any], native: Any, stage: str) -> dict[str, Any] | None:
    for viewer in ("white", "black"):
        try:
            actual = position.observe(viewer)
        except native.UnsupportedFeatureError as exc:
            return failure("unsupported", f"native {stage} public observation unsupported: {exc}", viewer=viewer)
        except Exception as exc:
            return failure("observation-error", f"native {stage} public observation: {type(exc).__name__}: {exc}", viewer=viewer)
        changed = difference(expected[viewer], actual)
        if changed:
            return failure("mismatch", f"{stage} public observation differs", viewer=viewer, path=changed)
    return None


def check_rejection(
    position: Any, payload: dict[str, Any], expected_observations: dict[str, Any], native: Any
) -> dict[str, Any] | None:
    before = position.snapshot()
    before_result = position.result
    try:
        action = position.bind_action(payload)
        position.apply(action)
    except native.UnsupportedFeatureError as exc:
        return failure("unsupported", f"native rejection path unsupported: {exc}")
    except (native.NativeError, ValueError) as exc:
        changed = difference(before, position.snapshot())
        if changed:
            return failure("mismatch", "rejected action mutated native position", path=changed)
        if position.result != before_result:
            return failure("mismatch", "rejected action changed native result")
        return compare_observations(position, expected_observations, native, "rejected")
    return failure("mismatch", "native accepted source-rejected wrong-actor action")


def check_stale(
    applied_position: Any, old_action: Any, source_snapshot: dict[str, Any], native: Any
) -> dict[str, Any] | None:
    before = applied_position.snapshot()
    before_result = applied_position.result
    for route in ("snapshot", "apply"):
        try:
            if route == "snapshot":
                applied_position.bind_snapshot(source_snapshot)
            else:
                applied_position.apply(old_action)
        except native.StaleActionError:
            pass
        except native.UnsupportedFeatureError as exc:
            return failure("unsupported", f"native stale {route} path unsupported: {exc}")
        except Exception as exc:
            return failure("stale-error", f"native stale {route} path: {type(exc).__name__}: {exc}")
        else:
            return failure("mismatch", f"native accepted stale action through {route}")
        changed = difference(before, applied_position.snapshot())
        if changed:
            return failure("mismatch", f"stale {route} mutated applied position", path=changed)
        if applied_position.result != before_result:
            return failure("mismatch", f"stale {route} changed applied result")
    return None


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
    changed = difference(case["result"], full_result(position))
    if changed:
        return {**result, **failure("mismatch", "baseline result envelope differs", path=changed)}
    observed = compare_observations(position, case["observations"], native, "baseline")
    if observed:
        return {**result, **observed}

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
    for index, (source_action, native_action) in enumerate(zip(expected, actual)):
        changed = difference(source_action, native_action)
        if changed:
            return {**result, **failure("mismatch", "ordered legal action envelope differs", index=index, actionId=source_action["actionId"], path=changed)}

    for index, source_action in enumerate(expected):
        try:
            by_payload = position.bind_action(source_action["payload"]).snapshot()
            by_snapshot = position.bind_snapshot(source_action).snapshot()
        except native.UnsupportedFeatureError as exc:
            return {**result, **failure("unsupported", f"native v7 bind unsupported: {exc}", index=index)}
        except Exception as exc:
            return {**result, **failure("bind-error", f"{type(exc).__name__}: {exc}", index=index)}
        for route, bound in (("payload", by_payload), ("snapshot", by_snapshot)):
            changed = difference(source_action, bound)
            if changed:
                return {**result, **failure("mismatch", "legal action bind differs", index=index, route=route, path=changed)}

    if case["rejectPayload"] is not None:
        rejected = check_rejection(position, case["rejectPayload"], case["observations"], native)
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
        changed = difference(sample["result"], full_result(step.position))
        if changed:
            return {**result, **failure("mismatch", "applied result envelope differs", sample=index, path=changed)}
        observed = compare_observations(step.position, sample["observations"], native, f"sample {index}")
        if observed:
            return {**result, **observed, "sample": index}
        stale = check_stale(step.position, action, sample["action"], native)
        if stale:
            return {**result, **stale, "sample": index}
        if difference(case["position"], position.snapshot()):
            return {**result, **failure("mismatch", "apply mutated source native position", sample=index)}
    return {**result, "status": "pass", "legalCount": len(actual), "nativeExamined": examined,
            "bound": len(expected), "applied": len(case["samples"]),
            "rejected": 1 if case["rejectPayload"] is not None else 0,
            "staleBindRejected": len(case["samples"]), "staleApplyRejected": len(case["samples"]),
            "publicViewers": 2}


def main() -> None:
    request = json.load(sys.stdin)
    if request.get("profile") != PROFILE or request.get("sourceSha256") != SOURCE_SHA256:
        print(json.dumps(failure("version-mismatch", "native probe request source/profile identity differs")))
        return
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
    main_files = [item for item in catalog.get("source", {}).get("files", []) if item.get("name", "").startswith("main-")]
    if len(main_files) != 1 or main_files[0].get("sha256") != SOURCE_SHA256:
        print(json.dumps(failure("version-mismatch", "native v7 source SHA differs")))
        return
    try:
        native_policy = native.site_observation_policy(request["rulesVersion"])
    except Exception as exc:
        print(json.dumps(failure("native-unsupported", f"v7 observation policy unavailable: {type(exc).__name__}: {exc}")))
        return
    changed = difference(request["observationPolicy"], native_policy)
    if changed:
        print(json.dumps(failure("version-mismatch", "native v7 observation policy differs", path=changed)))
        return
    if request["phase"] == "preflight":
        print(json.dumps({"status": "ready"}))
        return
    source_cases = request.get("cases")
    if not isinstance(source_cases, list) or not source_cases:
        print(json.dumps(failure("probe-error", "native comparison requires at least one source case")))
        return
    cases = [compare_case(case, native) for case in source_cases]
    status = "pass" if all(case["status"] == "pass" for case in cases) else "fail"
    print(json.dumps({"status": status, "cases": cases}, allow_nan=False))


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(json.dumps(failure("probe-error", f"{type(exc).__name__}: {exc}")))
