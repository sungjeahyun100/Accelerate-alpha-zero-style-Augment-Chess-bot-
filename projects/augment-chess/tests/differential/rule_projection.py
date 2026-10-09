"""Conservative rule-state projection for paired JS/Rust v7 snapshots.

This diagnostic does not claim a complete headless state model. Unknown fields
stay in the projection, so they cannot silently become ignored differences.
"""

from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
from fractions import Fraction
from math import log, sqrt
from typing import Any, Mapping


PRESENTATION_FIELDS = frozenset({
    "animatedPieceIds", "forceAnimatedPieceIds", "pendingReplayVisuals",
    "pendingNotation", "pendingNotations",
})


@dataclass(frozen=True)
class Verdict:
    status: str
    reason: str
    evidence: str

    def as_dict(self) -> dict[str, str]:
        return {"status": self.status, "reason": self.reason, "evidence": self.evidence}


def rule_projection(position: Mapping[str, Any]) -> dict[str, Any]:
    """Keep every unclassified field; strip only proven presentation data.

    This accepts a source-shaped Position envelope or an equivalent exported
    Rust envelope. Replay frames and public history remain until their rule
    dependencies have been separately modeled. A nonempty pending replay queue
    is rejected because recording it can change later replay indexes.
    """
    if not isinstance(position, Mapping) or not isinstance(position.get("state"), Mapping):
        raise ValueError("position.state must be an object")
    result = deepcopy(dict(position))
    state = result["state"]
    if state.get("pendingReplayVisuals") not in (None, []):
        raise NotImplementedError("nonempty pendingReplayVisuals can change replay history")
    if state.get("pendingNotation") is not None or state.get("pendingNotations") not in (None, []):
        raise NotImplementedError("pending notation can change replay history")
    for key in PRESENTATION_FIELDS:
        state.pop(key, None)
    last_move = state.get("lastMove")
    if isinstance(last_move, dict):
        # soundColor is retained: Idol Encore reads it as the moving owner.
        last_move.pop("soundName", None)
    result.pop("rng", None)
    result.pop("positionId", None)
    return result


def _first_difference(left: Any, right: Any, path: str = "$") -> str | None:
    if type(left) is not bool and type(right) is not bool and type(left) in (int, float) and type(right) in (int, float):
        return None if left == right else path
    if type(left) is not type(right):
        return path
    if isinstance(left, dict):
        if left.keys() != right.keys():
            return f"{path}: missing={sorted(left.keys() - right.keys())} extra={sorted(right.keys() - left.keys())}"
        for key in sorted(left):
            found = _first_difference(left[key], right[key], f"{path}.{key}")
            if found:
                return found
        return None
    if isinstance(left, list):
        if len(left) != len(right):
            return f"{path}: length {len(left)} != {len(right)}"
        for index, (a, b) in enumerate(zip(left, right)):
            found = _first_difference(a, b, f"{path}[{index}]")
            if found:
                return found
        return None
    return None if left == right else path


def compare_positions(source: Mapping[str, Any], rust: Mapping[str, Any]) -> Verdict:
    try:
        left, right = rule_projection(source), rule_projection(rust)
    except (ValueError, NotImplementedError) as error:
        return Verdict("UNSUPPORTED", str(error), "rule-projection-v1")
    path = _first_difference(left, right)
    if path:
        return Verdict("MISMATCH", path, "rule-projection-v1")
    return Verdict("PASS", "projected snapshots agree", "rule-projection-v1; not a transition-distribution proof")


def compare_exact_distribution(
    source: Mapping[str, str], rust: Mapping[str, str]
) -> Verdict:
    """Compare complete finite *joint* outcome maps with rational weights."""
    if not isinstance(source, Mapping) or not isinstance(rust, Mapping):
        return Verdict("UNSUPPORTED", "joint outcome map must be an object", "exact-joint-v1")
    if any(type(value) is not str for value in (*source.values(), *rust.values())):
        return Verdict("UNSUPPORTED", "exact probabilities must be rational strings", "exact-joint-v1")
    try:
        left = {key: Fraction(value) for key, value in source.items()}
        right = {key: Fraction(value) for key, value in rust.items()}
    except (TypeError, ValueError, ZeroDivisionError) as error:
        return Verdict("UNSUPPORTED", f"invalid exact weight: {error}", "exact-joint-v1")
    if not left or not right or any(weight < 0 for weight in (*left.values(), *right.values())):
        return Verdict("UNSUPPORTED", "empty distribution or negative weight", "exact-joint-v1")
    if sum(left.values()) != 1 or sum(right.values()) != 1:
        return Verdict("UNSUPPORTED", "incomplete or unnormalised outcome map", "exact-joint-v1")
    if {key: value for key, value in left.items() if value} != {key: value for key, value in right.items() if value}:
        return Verdict("MISMATCH", "joint outcome probabilities differ", "exact-joint-v1")
    return Verdict("PASS", "complete joint outcome maps agree", "exact-joint-v1")


def compare_sample_distribution(
    source: Mapping[str, int], rust: Mapping[str, int], *, tolerance: float,
    alpha: float = 0.01,
) -> Verdict:
    """Bound each joint-outcome frequency difference with a union bound.

    PASS is a finite-sample tolerance decision, never mathematical equality.
    Samples must already be conditioned on the same projected state and action.
    """
    if not isinstance(source, Mapping) or not isinstance(rust, Mapping):
        return Verdict("UNSUPPORTED", "sample counts must be objects", "sample-joint-v1")
    keys = set(source) | set(rust)
    if (not keys or type(alpha) not in (int, float) or type(tolerance) not in (int, float)
            or not (0 < alpha < 1) or not (0 < tolerance < 1)):
        return Verdict("UNSUPPORTED", "invalid statistical design", "sample-joint-v1")
    if any(type(value) is not int or value < 0 for value in (*source.values(), *rust.values())):
        return Verdict("UNSUPPORTED", "counts must be nonnegative integers", "sample-joint-v1")
    n_source, n_rust = sum(source.values()), sum(rust.values())
    if n_source == 0 or n_rust == 0:
        return Verdict("INCONCLUSIVE", "both samples need observations", "sample-joint-v1")
    radius = sqrt(log(4 * len(keys) / alpha) / (2 * n_source)) + sqrt(
        log(4 * len(keys) / alpha) / (2 * n_rust)
    )
    observed = max(abs(source.get(key, 0) / n_source - rust.get(key, 0) / n_rust) for key in keys)
    if observed - radius > tolerance:
        return Verdict("MISMATCH", "joint outcome frequency gap exceeds tolerance with confidence bound", "sample-joint-v1")
    if observed + radius <= tolerance:
        return Verdict("PASS", "joint outcome frequency gap is within predeclared tolerance with confidence bound; statistical only", "sample-joint-v1")
    return Verdict("INCONCLUSIVE", "sample size does not resolve the tolerance decision", "sample-joint-v1")
