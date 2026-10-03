"""Compare a pinned JS v7 probe batch with an installed PyO3 engine.

The Node coordinator owns source generation. This worker never reconstructs
oracle state or treats a missing/unsupported native engine as a skipped test.
"""

from __future__ import annotations

import json
import math
import sys
from typing import Any


PAGE_SIZE = 64
MAX_PAGES = 128
MAX_ACTIONS = 4096
MAX_EXAMINED = 65536
SOURCE_SHA256 = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
PROFILE = "accelerate-headless-semantic-v7-faithful-init-v1"


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
    intents: list[dict[str, Any]] = []
    examined = 0
    for _ in range(MAX_PAGES):
        page = stream.next_page(PAGE_SIZE)
        examined += page["examined"]
        intents.extend(action.public_intent() for action in page["actions"])
        if len(intents) > MAX_ACTIONS or examined > MAX_EXAMINED:
            raise RuntimeError("native public action stream exceeded probe budget")
        if page["exhausted"]:
            return intents, examined
        if not page["actions"] and page["examined"] == 0:
            raise RuntimeError("native public action stream made no progress")
    raise RuntimeError("native public action stream did not exhaust within page budget")


def public_intent(source_action: dict[str, Any]) -> dict[str, Any]:
    """The source UI choice for the frozen corpus's ordinary moves and offers."""
    payload = source_action["payload"]
    if payload["type"] == "move":
        target = payload["move"]
        intent = {"type": "move", "color": payload["color"], "from": payload["from"],
                  "destination": {"row": target["row"], "col": target["col"]}}
        for flag, mode in (("shotgunBlast", "shotgun"), ("shotgunSnipe", "snipe"),
                           ("setLogDirection", "log-direction")):
            if target.get(flag):
                intent["selectionMode"] = mode
        return intent
    if payload["type"] == "trolleyChoice":
        return {key: payload[key] for key in ("type", "color", "doomedIndex")}
    return payload


def compare_observations(position: Any, expected: dict[str, Any], stage: str) -> dict[str, Any] | None:
    for viewer in ("white", "black"):
        try:
            actual = position.observe(viewer)
        except Exception as exc:
            return failure("observation-error", f"native {stage} public observation: {type(exc).__name__}: {exc}", viewer=viewer)
        changed = difference(expected[viewer], actual)
        if changed:
            return failure("mismatch", f"{stage} public observation differs", viewer=viewer, path=changed)
    return None


def compare_case(case: dict[str, Any], native: Any, spec: Any) -> dict[str, Any]:
    from accelerate_chess import GameAdapterClient

    result: dict[str, Any] = {"name": case["name"], "mode": case["position"]["state"]["mode"]}
    try:
        position = GameAdapterClient(native.GameAdapterSession.from_envelope(case["position"]), spec)
    except Exception as exc:
        return {**result, **failure("import-error", f"{type(exc).__name__}: {exc}")}
    if position.snapshot_revision != case["position"]["positionId"]:
        return {**result, **failure("mismatch", "imported source revision differs")}
    if position.result != case["result"]["outcome"]:
        return {**result, **failure("mismatch", "baseline result differs")}
    observed = compare_observations(position, case["observations"], "baseline")
    if observed:
        return {**result, **observed}

    try:
        actual, examined = collect_actions(position)
        eager = position.legal_intents()
    except Exception as exc:
        return {**result, **failure("legal-error", f"{type(exc).__name__}: {exc}")}
    expected = case["actions"]
    if len(actual) != len(expected) or tuple(actual) != eager:
        return {**result, **failure("mismatch", "ordered public action stream differs from source count or eager native list",
                                    sourceCount=len(expected), nativeCount=len(actual), nativeExamined=examined)}
    for index, (source_action, native_intent) in enumerate(zip(expected, actual)):
        changed = difference(public_intent(source_action), native_intent)
        if changed:
            return {**result, **failure("mismatch", "ordered source public intent differs", index=index, path=changed)}
        try:
            bound = position.bind_public_intent(native_intent)
        except Exception as exc:
            return {**result, **failure("bind-error", f"{type(exc).__name__}: {exc}", index=index)}
        if bound.public_intent() != native_intent:
            return {**result, **failure("mismatch", "native binding changed public intent", index=index)}

    if case["rejectPayload"] is not None:
        before = position.snapshot_revision
        rejected_intent = public_intent({"payload": case["rejectPayload"]})
        try:
            position.apply_public_intent(rejected_intent)
        except native.NativeError as exc:
            if "wrong_game_actor" not in str(exc):
                return {**result, **failure("rejection-error", f"unexpected native rejection: {exc}")}
        except Exception as exc:
            return {**result, **failure("rejection-error", f"{type(exc).__name__}: {exc}")}
        else:
            return {**result, **failure("mismatch", "native accepted source-rejected wrong-actor intent")}
        if position.snapshot_revision != before or position.result != case["result"]["outcome"]:
            return {**result, **failure("mismatch", "rejected intent mutated native position")}
        rejected = compare_observations(position, case["observations"], "rejected")
        if rejected:
            return {**result, **rejected}

    by_id = {source_action["actionId"]: index for index, source_action in enumerate(expected)}
    for index, sample in enumerate(case["samples"]):
        action_index = by_id.get(sample["action"]["actionId"])
        if action_index is None:
            return {**result, **failure("probe-error", "source sample not in complete legal stream", sample=index)}
        try:
            action = position.bind_public_intent(actual[action_index])
            step = position.apply(action)
        except Exception as exc:
            return {**result, **failure("apply-error", f"{type(exc).__name__}: {exc}", sample=index)}
        # Position IDs cover the entire state, RNG and public history. Equal
        # revisions therefore verify private transition parity without exporting it.
        if step.position.snapshot_revision != sample["position"]["positionId"]:
            return {**result, **failure("mismatch", "applied full state/history/RNG revision differs", sample=index)}
        expected_event = sample["position"]["history"][-1]
        if step.actor != expected_event["actor"] or step.turn_changed != expected_event["turnChanged"]:
            return {**result, **failure("mismatch", "applied actor/turn change differs", sample=index)}
        if step.result != sample["result"]["outcome"] or step.position.result != sample["result"]["outcome"]:
            return {**result, **failure("mismatch", "applied result differs", sample=index)}
        observed = compare_observations(step.position, sample["observations"], f"sample {index}")
        if observed:
            return {**result, **observed, "sample": index}
        try:
            step.position.apply(action)
        except ValueError:
            pass
        else:
            return {**result, **failure("mismatch", "native accepted stale public intent", sample=index)}
        if step.position.snapshot_revision != sample["position"]["positionId"]:
            return {**result, **failure("mismatch", "stale action mutated native position", sample=index)}
        if position.snapshot_revision != case["position"]["positionId"]:
            return {**result, **failure("mismatch", "apply mutated source native position", sample=index)}
    return {**result, "status": "pass", "legalCount": len(actual), "nativeExamined": examined,
            "bound": len(expected), "applied": len(case["samples"]),
            "rejected": 1 if case["rejectPayload"] is not None else 0,
            "staleApplyRejected": len(case["samples"]), "publicViewers": 2}


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
    from accelerate_chess.ir import TypedEncoderSpec

    spec = TypedEncoderSpec.from_catalog(catalog, observation_policy=native_policy)
    cases = [compare_case(case, native, spec) for case in source_cases]
    status = "pass" if all(case["status"] == "pass" for case in cases) else "fail"
    print(json.dumps({"status": status, "cases": cases}, allow_nan=False))


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(json.dumps(failure("probe-error", f"{type(exc).__name__}: {exc}")))
