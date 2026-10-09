"""Compare paired frozen-JS and Rust rule transitions without running either engine.

The producer records one realized transition per side. A single stochastic
realization is evidence of execution, never evidence that two laws disagree.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

from rule_projection import (
    Verdict, compare_exact_distribution, compare_positions,
    compare_sample_distribution, rule_projection,
)

MAX_CASES = 10_000
MAX_LINE_BYTES = 16 * 1024 * 1024
REQUIRED = (
    "name", "source", "rust", "sourceActions", "rustActions", "sourceAction", "rustAction",
    "sourceObservations", "rustObservations", "sourceRejection", "rustRejection",
    "sourceNext", "rustNext", "sourceNextObservations", "rustNextObservations",
    "sourceResult", "rustResult", "transitionKind",
)


def _action_meaning(action: Any) -> str:
    if not isinstance(action, dict):
        raise ValueError("action must be an object")
    payload = action.get("payload", action)
    if not isinstance(payload, dict):
        raise ValueError("action payload must be an object")
    return json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def _legal_meanings(actions: Any) -> list[str]:
    if not isinstance(actions, list):
        raise ValueError("legal actions must be a complete array")
    return sorted(_action_meaning(action) for action in actions)


def _equal(source: Any, rust: Any, description: str) -> Verdict:
    if source == rust:
        return Verdict("PASS", f"{description} agree", "paired-artifact-v2")
    return Verdict("MISMATCH", f"{description} differ", "paired-artifact-v2")


def _viewers(value: Any) -> bool:
    return isinstance(value, dict) and set(value) == {"white", "black"}


def _result(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError("transition result must be an object")
    required = ("status", "winner", "outcome")
    if any(key not in value for key in required):
        raise ValueError("transition result is missing status, winner, or outcome")
    # The Korean replay reason is presentation text. Unknown result fields are
    # intentionally retained, except the declared display-only reason.
    return {key: item for key, item in value.items() if key != "reason"}


def _outcome_key(position: Any, observations: Any, result: Any) -> str:
    return json.dumps(
        [rule_projection(position), observations, _result(result)],
        sort_keys=True, separators=(",", ":"), ensure_ascii=False,
    )


def _distribution(value: Any, conditioned_on: str, realized: tuple[str, str]) -> Verdict:
    if not isinstance(value, dict):
        return Verdict("INCONCLUSIVE", "joint transition distribution was not supplied", "paired-artifact-v2")
    if value.get("conditionedOn") != conditioned_on:
        return Verdict("UNSUPPORTED", "distribution conditioning key differs from the selected state and action", "paired-artifact-v2")
    if any(not isinstance(value.get(side), dict) or outcome not in value[side]
           for side, outcome in zip(("source", "rust"), realized)):
        return Verdict("UNSUPPORTED", "distribution omits a realized joint rule outcome", "paired-artifact-v2")
    if value.get("kind") == "exact" and value.get("complete") is True:
        return compare_exact_distribution(value.get("source"), value.get("rust"))
    if value.get("kind") == "sample":
        return compare_sample_distribution(
            value.get("source"), value.get("rust"),
            tolerance=value.get("tolerance", 0.0), alpha=value.get("alpha", 0.01),
        )
    return Verdict("UNSUPPORTED", "distribution needs complete exact support or bounded samples", "paired-artifact-v2")


def compare_case(case: dict[str, Any]) -> dict[str, Any]:
    name = case.get("name")
    if case.get("generationStatus") not in (None, "complete"):
        return {"name": name, "status": "UNSUPPORTED", "reason": case.get("generationReason", "paired generation incomplete"), "checks": {}}
    missing = [key for key in REQUIRED if key not in case]
    if missing:
        return {"name": name, "status": "UNSUPPORTED", "reason": f"missing required evidence: {missing}", "checks": {}}
    checks: dict[str, dict[str, str]] = {}
    checks["initialRuleState"] = compare_positions(case["source"], case["rust"]).as_dict()
    for key in ("sourceObservations", "rustObservations", "sourceNextObservations", "rustNextObservations"):
        if not _viewers(case[key]):
            return {"name": name, "status": "UNSUPPORTED", "reason": f"{key} must contain both viewers", "checks": checks}
    checks["initialPublicObservations"] = _equal(
        case["sourceObservations"], case["rustObservations"], "initial public observations").as_dict()
    for key in ("sourceRejection", "rustRejection"):
        result = case[key]
        if not isinstance(result, dict) or result.get("rejected") is not True or result.get("unchanged") is not True:
            checks["wrongActorRejection"] = Verdict("MISMATCH", f"{key} did not reject without mutation", "paired-artifact-v2").as_dict()
            break
    else:
        checks["wrongActorRejection"] = Verdict("PASS", "both reject without changing state", "paired-artifact-v2").as_dict()
    try:
        source_actions = _legal_meanings(case["sourceActions"])
        rust_actions = _legal_meanings(case["rustActions"])
        source_action = _action_meaning(case["sourceAction"])
        rust_action = _action_meaning(case["rustAction"])
    except ValueError as error:
        return {"name": name, "status": "UNSUPPORTED", "reason": str(error), "checks": checks}
    checks["legalActions"] = _equal(source_actions, rust_actions, "complete legal action meanings").as_dict()
    checks["selectedAction"] = _equal(source_action, rust_action, "selected action meanings").as_dict()
    if source_action not in source_actions or rust_action not in rust_actions:
        checks["selectedAction"] = Verdict("UNSUPPORTED", "selected action absent from a legal action list", "paired-artifact-v2").as_dict()

    kind = case["transitionKind"]
    if kind == "deterministic":
        checks["nextRuleState"] = compare_positions(case["sourceNext"], case["rustNext"]).as_dict()
        checks["nextPublicObservations"] = _equal(
            case["sourceNextObservations"], case["rustNextObservations"], "next public observations").as_dict()
        try:
            checks["result"] = _equal(_result(case["sourceResult"]), _result(case["rustResult"]), "rule result").as_dict()
        except ValueError as error:
            checks["result"] = Verdict("UNSUPPORTED", str(error), "paired-artifact-v2").as_dict()
    elif kind == "stochastic":
        # Validate both actual outputs, but do not compare two independent draws.
        try:
            realized = (
                _outcome_key(case["sourceNext"], case["sourceNextObservations"], case["sourceResult"]),
                _outcome_key(case["rustNext"], case["rustNextObservations"], case["rustResult"]),
            )
        except (ValueError, NotImplementedError) as error:
            checks["nextRuleState"] = Verdict("UNSUPPORTED", str(error), "paired-artifact-v2").as_dict()
            realized = None
        conditioned_on = json.dumps(
            [rule_projection(case["source"]), source_action], sort_keys=True,
            separators=(",", ":"), ensure_ascii=False,
        ) if checks["initialRuleState"]["status"] == "PASS" else ""
        checks["transitionDistribution"] = (
            _distribution(case.get("distribution"), conditioned_on, realized)
            if realized is not None else Verdict("UNSUPPORTED", "actual joint outcomes cannot be projected", "paired-artifact-v2")
        ).as_dict()
    elif kind == "unknown":
        checks["transitionDistribution"] = Verdict(
            "INCONCLUSIVE", "transition was not classified; individual outcomes are not a distribution proof",
            "paired-artifact-v2").as_dict()
    else:
        checks["transitionDistribution"] = Verdict("UNSUPPORTED", "unknown transition kind", "paired-artifact-v2").as_dict()
    statuses = {check["status"] for check in checks.values()}
    status = next((item for item in ("MISMATCH", "UNSUPPORTED", "INCONCLUSIVE") if item in statuses), "PASS")
    return {"name": name, "status": status, "checks": checks}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pairs", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    cases = []
    with args.pairs.open("rb") as source:
        for number, raw in enumerate(source, 1):
            if number > MAX_CASES or len(raw) > MAX_LINE_BYTES:
                raise ValueError(f"case budget exceeded at line {number}")
            item = json.loads(raw)
            if not isinstance(item, dict):
                raise ValueError(f"line {number} must be an object")
            cases.append(compare_case(item))
    if not cases:
        raise ValueError("paired input is empty")
    statuses = {case["status"] for case in cases}
    overall = next((item for item in ("MISMATCH", "UNSUPPORTED", "INCONCLUSIVE") if item in statuses), "PASS")
    report = {"contract": "augment-rule-semantic-v2", "status": overall, "cases": cases,
              "note": "stochastic single draws are never compared; sampled PASS is statistical only"}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": overall, "caseCount": len(cases), "report": str(args.report)}, ensure_ascii=False))
    return 0 if overall == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
