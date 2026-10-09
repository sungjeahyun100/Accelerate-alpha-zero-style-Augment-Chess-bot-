"""Rule transition parity contracts. Written for user execution; not run here."""
from __future__ import annotations

import json
import unittest
from copy import deepcopy

from rule_projection import compare_exact_distribution, compare_positions, compare_sample_distribution, rule_projection
from semantic_differential import _outcome_key, compare_case


class ProjectionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.position = {
            "rulesVersion": "v7", "positionId": "source-id", "rng": {"state": 3},
            "state": {"board": [[{"id": "p", "type": "pawn"}]], "turn": "white",
                      "lastMove": {"pieceId": "p", "soundName": "capture", "soundColor": "white"},
                      "pendingReplayVisuals": [], "pendingNotation": None, "pendingNotations": [],
                      "boardHistory": [{"move": "p"}]},
        }

    def test_ui_fields_and_rng_do_not_enter_projection(self) -> None:
        other = deepcopy(self.position)
        other["positionId"] = "rust-id"
        other["rng"] = {"state": 900}
        other["state"]["lastMove"]["soundName"] = "checkDanger"
        other["state"]["forceAnimatedPieceIds"] = {"__simType": "Set", "values": ["p"]}
        self.assertEqual(compare_positions(self.position, other).status, "PASS")

    def test_rule_history_owner_and_unknown_fields_remain(self) -> None:
        for key, value in (("boardHistory", [{"move": "other"}]),
                           ("hiddenHand", ["private-card"]),
                           ("lastMove", {"pieceId": "p", "soundColor": "black"})):
            other = deepcopy(self.position)
            other["state"][key] = value
            self.assertEqual(compare_positions(self.position, other).status, "MISMATCH")

    def test_unsettled_replay_is_unsupported(self) -> None:
        other = deepcopy(self.position)
        other["state"]["pendingReplayVisuals"] = [{"type": "vanish"}]
        self.assertEqual(compare_positions(self.position, other).status, "UNSUPPORTED")

    def test_safe_number_spelling(self) -> None:
        source, rust = deepcopy(self.position), deepcopy(self.position)
        source["state"]["moveCount"] = 1
        rust["state"]["moveCount"] = 1.0
        self.assertEqual(compare_positions(source, rust).status, "PASS")

    def test_joint_distribution_requires_normalized_complete_weights(self) -> None:
        source = {"a+b": "1/2", "a+c": "1/2"}
        self.assertEqual(compare_exact_distribution(source, source).status, "PASS")
        self.assertEqual(compare_exact_distribution(source, {"a+b": "1/4", "a+c": "3/4"}).status, "MISMATCH")
        self.assertEqual(compare_exact_distribution(source, {"a+b": "1/2"}).status, "UNSUPPORTED")
        self.assertEqual(compare_sample_distribution({"a": 1}, {"a": 1}, tolerance=0.01).status, "INCONCLUSIVE")


class TransitionTests(unittest.TestCase):
    def setUp(self) -> None:
        state = {"board": [[{"id": "p", "type": "pawn"}]], "turn": "white"}
        self.position = {"state": state, "rng": {"state": 1}, "positionId": "source"}
        self.action = {"type": "move", "color": "white"}
        self.case = {
            "name": "move", "generationStatus": "complete", "transitionKind": "deterministic",
            "source": self.position, "rust": deepcopy(self.position),
            "sourceActions": [self.action], "rustActions": [self.action],
            "sourceAction": self.action, "rustAction": self.action,
            "sourceObservations": {"white": {}, "black": {}},
            "rustObservations": {"white": {}, "black": {}},
            "sourceRejection": {"rejected": True, "unchanged": True},
            "rustRejection": {"rejected": True, "unchanged": True},
            "sourceNext": deepcopy(self.position), "rustNext": deepcopy(self.position),
            "sourceNextObservations": {"white": {}, "black": {}},
            "rustNextObservations": {"white": {}, "black": {}},
            "sourceResult": {"status": "ongoing", "winner": None, "outcome": None, "reason": "text"},
            "rustResult": {"status": "ongoing", "winner": None, "outcome": None},
        }

    def test_deterministic_next_state_is_checked(self) -> None:
        self.assertEqual(compare_case(self.case)["status"], "PASS")
        self.case["rustNext"]["state"]["turn"] = "black"
        self.assertEqual(compare_case(self.case)["checks"]["nextRuleState"]["status"], "MISMATCH")

    def test_stochastic_single_draw_difference_is_not_mismatch(self) -> None:
        self.case["transitionKind"] = "stochastic"
        self.case["rustNext"]["state"]["turn"] = "black"
        result = compare_case(self.case)
        self.assertEqual(result["status"], "INCONCLUSIVE")
        conditioned_on = json.dumps([rule_projection(self.position),
                                     json.dumps(self.action, sort_keys=True, separators=(",", ":"), ensure_ascii=False)],
                                    sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        source_outcome = _outcome_key(self.case["sourceNext"], self.case["sourceNextObservations"], self.case["sourceResult"])
        rust_outcome = _outcome_key(self.case["rustNext"], self.case["rustNextObservations"], self.case["rustResult"])
        self.case["distribution"] = {"kind": "exact", "complete": True,
                                      "conditionedOn": conditioned_on,
                                      "source": {source_outcome: "1/2", rust_outcome: "1/2"},
                                      "rust": {source_outcome: "1/2", rust_outcome: "1/2"}}
        self.assertEqual(compare_case(self.case)["status"], "PASS")
        self.case["distribution"]["rust"] = {source_outcome: "1/4", rust_outcome: "3/4"}
        self.assertEqual(compare_case(self.case)["status"], "MISMATCH")

    def test_unclassified_transition_and_incomplete_generation_do_not_pass(self) -> None:
        self.case["transitionKind"] = "unknown"
        self.assertEqual(compare_case(self.case)["status"], "INCONCLUSIVE")
        self.case["generationStatus"] = "unsupported"
        self.assertEqual(compare_case(self.case)["status"], "UNSUPPORTED")


if __name__ == "__main__":
    unittest.main()
