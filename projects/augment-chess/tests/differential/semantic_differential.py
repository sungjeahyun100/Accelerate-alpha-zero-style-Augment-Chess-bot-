"""Compare paired frozen-JS and Rust rule transitions without running either engine.

The producer records one realized transition per side. A single stochastic
realization is evidence of execution, never evidence that two laws disagree.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any

from rule_projection import (
    Verdict, _first_difference, compare_exact_distribution, compare_positions,
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
    return Verdict("MISMATCH", f"{description} differ at {_first_difference(source, rust)}", "paired-artifact-v3")


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


def _conditional_equal(source: Any, rust: Any, description: str, same_state: bool) -> Verdict:
    if source == rust:
        return Verdict("PASS", f"{description} agree", "replay-sequence-v2")
    if same_state:
        return _equal(source, rust, description)
    return Verdict("INCONCLUSIVE", f"{description} differ after unclassified prior transitions",
                   "replay-sequence-v2")


def _replay_outcome(details: list[dict[str, Any]], complete: bool) -> Verdict:
    statuses = {check["status"] for step in details for check in step["checks"].values()}
    if "MISMATCH" in statuses:
        return Verdict("MISMATCH", "Replay step has a mismatch under an equal rule state",
                       "replay-sequence-v2")
    return Verdict("INCONCLUSIVE" if complete else "UNSUPPORTED",
                   "Replay executions are single realizations with unclassified transition laws" if complete
                   else "Replay execution or follow-up is incomplete", "replay-sequence-v2")


def _replay_sequence(value: Any, initial_same_state: bool = False) -> tuple[Verdict, list[dict[str, Any]]]:
    if isinstance(value, dict) and value.get("status") == "availability":
        source, rust = value.get("sourceSteps"), value.get("rustSteps")
        if not isinstance(source, list) or not isinstance(rust, list) or len(source) != len(rust):
            return Verdict("UNSUPPORTED", "Replay availability bridge has inconsistent steps", "replay-sequence-v1"), []
        try:
            details = []
            same = initial_same_state
            for index, (left, right) in enumerate(zip(source, rust)):
                state = compare_positions(left["position"], right["position"])
                if state.status == "UNSUPPORTED":
                    return state, details
                next_same = state.status == "PASS"
                details.append({"index": index, "kind": "bridge", "checks": {
                    "nextRuleState": (state if next_same else Verdict("INCONCLUSIVE", state.reason,
                        "replay-sequence-v2")).as_dict(),
                    "nextLegalActions": _conditional_equal(_legal_meanings(left["actions"]),
                        _legal_meanings(right["actions"]), "bridge next legal actions", next_same).as_dict(),
                    "replayFrame": _conditional_equal(left["replayFrame"], right["replayFrame"],
                        "bridge Replay frame", next_same).as_dict(),
                }})
                same = next_same
            before = _conditional_equal(_legal_meanings(value["sourceBeforeActions"]),
                                        _legal_meanings(value["rustBeforeActions"]),
                                        "Replay unavailable legal streams", same)
        except (KeyError, ValueError) as error:
            return Verdict("UNSUPPORTED", f"Replay availability evidence malformed: {error}", "replay-sequence-v1"), []
        details.append({"index": len(source), "kind": "availability",
                        "checks": {"legalActions": before.as_dict()}})
        return _replay_outcome(details, True), details
    if isinstance(value, dict) and value.get("status") == "unavailable":
        steps, index = value.get("sourceSteps"), value.get("failedStep")
        if isinstance(steps, list) and type(index) is int and 0 <= index < len(steps):
            try:
                prior = value.get("rustSteps", [])
                same = initial_same_state if index == 0 else (
                    isinstance(prior, list) and len(prior) >= index and
                    compare_positions(steps[index - 1]["position"], prior[index - 1]["position"]).status == "PASS")
                before = _conditional_equal(_legal_meanings(steps[index]["beforeActions"]),
                                            _legal_meanings(value["rustBeforeActions"]),
                                            "Replay availability in the full legal streams", same)
                detail = [{"index": index, "kind": steps[index].get("kind"),
                           "checks": {"beforeLegalActions": before.as_dict()}}]
                return (_replay_outcome(detail, False) if before.status == "MISMATCH" else
                        Verdict("UNSUPPORTED", f"Rust Replay continuation unavailable at step {index}: {value.get('reason')}",
                                "replay-sequence-v2")), detail
            except (KeyError, ValueError):
                pass
        return Verdict("UNSUPPORTED", str(value.get("reason", "Replay continuation unavailable")), "replay-sequence-v1"), []
    if not isinstance(value, dict) or value.get("status") != "complete":
        reason = value.get("reason", "Replay sequence was not supplied") if isinstance(value, dict) else "Replay sequence was not supplied"
        return Verdict("UNSUPPORTED", str(reason), "replay-sequence-v1"), []
    source, rust = value.get("sourceSteps"), value.get("rustSteps")
    if not isinstance(source, list) or not isinstance(rust, list) or len(source) != len(rust) or not 2 <= len(source) <= 4:
        return Verdict("UNSUPPORTED", "Replay sequence step count or shape differs", "replay-sequence-v1"), []
    details = []
    same = initial_same_state
    for index, (left, right) in enumerate(zip(source, rust)):
        if not isinstance(left, dict) or not isinstance(right, dict) or left.get("kind") != right.get("kind"):
            return Verdict("UNSUPPORTED", f"Replay step {index} kind missing or different", "replay-sequence-v1"), details
        try:
            before = _conditional_equal(_legal_meanings(left["beforeActions"]),
                                        _legal_meanings(right["beforeActions"]), "pre-step legal actions", same)
            state = compare_positions(left["position"], right["position"])
            if state.status == "UNSUPPORTED":
                return state, details
            next_same = state.status == "PASS"
            state = state if next_same else Verdict("INCONCLUSIVE", state.reason, "replay-sequence-v2")
            after = _conditional_equal(_legal_meanings(left["actions"]),
                                       _legal_meanings(right["actions"]), "post-step legal actions", next_same)
            observations = _conditional_equal(left["observations"], right["observations"],
                                              "post-step public observations", next_same)
            result = _conditional_equal(_result(left["result"]), _result(right["result"]),
                                        "post-step result", next_same)
            frame = _conditional_equal(left["replayFrame"], right["replayFrame"],
                                       "post-step Replay frame", next_same)
        except (KeyError, ValueError) as error:
            return Verdict("UNSUPPORTED", f"Replay step {index}: {error}", "replay-sequence-v1"), details
        details.append({"index": index, "kind": left["kind"], "checks": {
            "beforeLegalActions": before.as_dict(), "nextRuleState": state.as_dict(),
            "nextPublicObservations": observations.as_dict(), "nextLegalActions": after.as_dict(),
            "result": result.as_dict(),
            "replayFrame": frame.as_dict(),
        }})
        same = next_same
    # A chained single realization is not a distribution proof. Retain exact
    # comparison details for diagnosis, but require transition classifications
    # before using any result as a parity PASS or a stochastic mismatch.
    return _replay_outcome(details, True), details


def compare_case(case: dict[str, Any]) -> dict[str, Any]:
    name = case.get("name")
    if case.get("_provenanceVerified") is False:
        return {"name": name, "status": "UNSUPPORTED",
                "reason": "paired row provenance is missing or differs from the supplied source export",
                "checks": {"provenance": Verdict("UNSUPPORTED", "source identity or case digest differs",
                                                  "paired-provenance-v1").as_dict()}}
    if case.get("generationStatus") not in (None, "complete"):
        return {"name": name, "status": "UNSUPPORTED", "reason": case.get("generationReason", "paired generation incomplete"), "checks": {}}
    missing = [key for key in REQUIRED if key not in case]
    if missing:
        return {"name": name, "status": "UNSUPPORTED", "reason": f"missing required evidence: {missing}", "checks": {}}
    checks: dict[str, dict[str, str]] = {}
    checks["provenance"] = Verdict(
        "PASS" if case.get("_provenanceVerified") is True else "UNSUPPORTED",
        "source export and case digest verified" if case.get("_provenanceVerified") is True else
        "source report/cases were not verified against this paired row",
        "paired-provenance-v1",
    ).as_dict()
    checks["initialRuleState"] = compare_positions(case["source"], case["rust"]).as_dict()
    for key in ("sourceObservations", "rustObservations", "sourceNextObservations", "rustNextObservations"):
        if not _viewers(case[key]):
            return {"name": name, "status": "UNSUPPORTED", "reason": f"{key} must contain both viewers", "checks": checks}
    checks["initialPublicObservations"] = _equal(
        case["sourceObservations"], case["rustObservations"], "initial public observations").as_dict()
    for key in ("sourceRejection", "rustRejection"):
        result = case[key]
        if not isinstance(result, dict) or (key == "sourceRejection" and result.get("method") != "frozen-adapter-apply"):
            checks["wrongActorRejection"] = Verdict("UNSUPPORTED", f"{key} has no independent execution evidence", "paired-artifact-v3").as_dict()
            break
        if result.get("rejected") is not True or result.get("unchanged") is not True:
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
        evidence = case.get("classificationEvidence")
        if not isinstance(evidence, dict) or evidence.get("method") != "verified-rule-effect-analysis" or not evidence.get("rulePaths"):
            checks["classification"] = Verdict(
                "UNSUPPORTED", "deterministic classification lacks audited rule-effect paths",
                "classification-v1").as_dict()
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
    replay_start = compare_positions(case["sourceNext"], case["rustNext"])
    replay_verdict, replay_steps = (
        (replay_start, []) if replay_start.status == "UNSUPPORTED" else
        _replay_sequence(case.get("replaySequence"), replay_start.status == "PASS"))
    checks["replaySequence"] = replay_verdict.as_dict()
    statuses = {check["status"] for check in checks.values()}
    status = next((item for item in ("MISMATCH", "UNSUPPORTED", "INCONCLUSIVE") if item in statuses), "PASS")
    return {"name": name, "status": status, "transitionKind": kind,
            "checks": checks, "replaySteps": replay_steps}


def replay_diagnostics(cases: list[dict[str, Any]], searches: list[dict[str, Any]]) -> dict[str, int]:
    replay_values = [item.get("replaySequence", {}) for item in cases if isinstance(item.get("replaySequence"), dict)]
    replay_steps = [step for case in cases for step in case.get("replaySteps", [])]
    replay_checks = [check["status"] for step in replay_steps for check in step.get("checks", {}).values()]
    return {
        "Replay Search Attempts": sum(item.get("attempts", 0) for item in searches),
        "Replay Card Acquired": sum(bool(item.get("acquired")) for item in searches),
        "Replay Available": sum(any(step.get("kind") == "replay" for step in value.get("sourceSteps", []))
                                for value in replay_values),
        "Replay Executed": sum(any(step.get("kind") == "replay" for step in value.get("sourceSteps", []))
                               for value in replay_values),
        "Replay Follow-up Executed": sum(any(step.get("kind") == "follow" for step in value.get("sourceSteps", []))
                                         for value in replay_values),
        "Replay Source Complete": sum(value.get("sourceStatus") == "complete" for value in replay_values),
        "Replay Rust Complete": sum(value.get("status") == "complete" for value in replay_values),
        "Replay State Matches": sum(step.get("checks", {}).get("nextRuleState", {}).get("status") == "PASS"
                                     for step in replay_steps),
        "Replay Frame Matches": sum(step.get("checks", {}).get("replayFrame", {}).get("status") == "PASS"
                                     for step in replay_steps),
        "Replay Legal Action Matches": sum(check["status"] == "PASS" for step in replay_steps
                                           for key, check in step.get("checks", {}).items()
                                           if key in ("beforeLegalActions", "nextLegalActions", "legalActions")),
        "Replay Observation Matches": sum(step.get("checks", {}).get("nextPublicObservations", {}).get("status") == "PASS"
                                           for step in replay_steps),
        "Replay Mismatch": sum(case.get("checks", {}).get("replaySequence", {}).get("status") == "MISMATCH"
                               for case in cases),
        "Replay Step Mismatches": replay_checks.count("MISMATCH"),
        "Replay Unsupported": sum(case.get("checks", {}).get("replaySequence", {}).get("status") == "UNSUPPORTED"
                                  for case in cases),
        "Replay Inconclusive": sum(case.get("checks", {}).get("replaySequence", {}).get("status") == "INCONCLUSIVE"
                                   for case in cases),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pairs", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--source-report", type=Path)
    parser.add_argument("--source-cases", type=Path)
    args = parser.parse_args()
    if bool(args.source_report) != bool(args.source_cases):
        raise ValueError("--source-report and --source-cases must be supplied together")
    source_lines = None
    export_sha = None
    if args.source_report:
        source_report = json.loads(args.source_report.read_text(encoding="utf-8"))
        raw_source = args.source_cases.read_bytes()
        export_sha = hashlib.sha256(raw_source).hexdigest()
        source_lines = raw_source.splitlines()
        if source_report.get("status") != "oracle-only" or source_report.get("sourceExport", {}).get("sha256") != export_sha or source_report.get("sourceExport", {}).get("cases") != len(source_lines):
            raise ValueError("source report, case count, or export digest differs")
    cases = []
    with args.pairs.open("rb") as source:
        for number, raw in enumerate(source, 1):
            if number > MAX_CASES or len(raw) > MAX_LINE_BYTES:
                raise ValueError(f"case budget exceeded at line {number}")
            item = json.loads(raw)
            if not isinstance(item, dict):
                raise ValueError(f"line {number} must be an object")
            item["_provenanceVerified"] = False
            provenance = item.get("provenance")
            if source_lines is not None and isinstance(provenance, dict):
                line_number = provenance.get("sourceCaseLine")
                sample_index = provenance.get("sampleIndex")
                identity_ok = (
                    type(line_number) is int and 1 <= line_number <= len(source_lines)
                    and type(sample_index) is int and sample_index >= 0
                    and provenance.get("sourceExportSha256") == export_sha
                    and provenance.get("sourceCaseSha256") == hashlib.sha256(source_lines[line_number - 1]).hexdigest()
                    and provenance.get("sourceSha256") == source_report.get("source", {}).get("sha256")
                    and provenance.get("profile") == source_report.get("source", {}).get("profile")
                )
                if identity_ok:
                    original = json.loads(source_lines[line_number - 1])
                    samples = original.get("samples", [])
                    item["_provenanceVerified"] = (
                        sample_index < len(samples)
                        and item.get("name") == f"{original.get('name')}/sample[{sample_index}]"
                        and item.get("source") == original.get("position")
                        and item.get("sourceActions") == original.get("publicIntents")
                        and item.get("sourceAction") == samples[sample_index].get("publicIntent")
                        and item.get("sourceNext") == samples[sample_index].get("position")
                        and item.get("sourceRejection") == original.get("sourceRejection")
                        and (samples[sample_index].get("replaySequence", {}).get("status") not in ("complete", "unavailable")
                             or item.get("replaySequence", {}).get("sourceSteps") ==
                             samples[sample_index]["replaySequence"].get("steps"))
                    )
            cases.append(compare_case(item))
    if not cases:
        raise ValueError("paired input is empty")
    statuses = {case["status"] for case in cases}
    overall = next((item for item in ("MISMATCH", "UNSUPPORTED", "INCONCLUSIVE") if item in statuses), "PASS")
    categories = {
        "Initial State Parity": ("initialRuleState", "initialPublicObservations"),
        "Legal Action Parity": ("legalActions", "selectedAction"),
        "Deterministic Transition Parity": ("nextRuleState", "nextPublicObservations", "result"),
        "Stochastic Distribution Evidence": ("transitionDistribution",),
        "Replay Sequence Parity": ("replaySequence",),
    }
    summary = {}
    for label, keys in categories.items():
        relevant = [case["checks"][key]["status"] for case in cases for key in keys
                    if key in case["checks"] and
                    (label != "Deterministic Transition Parity" or case.get("transitionKind") == "deterministic") and
                    (label != "Stochastic Distribution Evidence" or case.get("transitionKind") == "stochastic")]
        summary[label] = {"compared": len(relevant), "passed": relevant.count("PASS"),
                          "mismatched": relevant.count("MISMATCH"),
                          "unsupported": relevant.count("UNSUPPORTED"),
                          "inconclusive": relevant.count("INCONCLUSIVE"),
                          "notApplicable": len(cases) * len(keys) - len(relevant)}
    summary["Unsupported Cases"] = sum(case["status"] == "UNSUPPORTED" for case in cases)
    summary["Inconclusive Cases"] = sum(case["status"] == "INCONCLUSIVE" for case in cases)
    searches = source_report.get("replaySearch", []) if args.source_report else []
    if not isinstance(searches, list):
        raise ValueError("source Replay search summary is malformed")
    summary["Replay Diagnostics"] = replay_diagnostics(cases, searches)
    report = {"contract": "augment-rule-semantic-v3", "status": overall, "summary": summary, "cases": cases,
              "note": "stochastic single draws are never compared; sampled PASS is statistical only"}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": overall, "caseCount": len(cases), "report": str(args.report)}, ensure_ascii=False))
    return 0 if overall == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
