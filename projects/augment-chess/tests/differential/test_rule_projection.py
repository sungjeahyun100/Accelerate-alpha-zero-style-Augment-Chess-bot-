"""Unit contracts for conservative semantic projection; intentionally not run here."""

from __future__ import annotations

import unittest

from rule_projection import (
    compare_exact_distribution,
    compare_positions,
    compare_sample_distribution,
)


class RuleProjectionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.position = {
            "rulesVersion": "v7", "positionId": "source-id", "rng": {"state": 3},
            "state": {
                "board": [[{"id": "p", "type": "pawn"}]], "turn": "white",
                "lastMove": {"pieceId": "p", "soundName": "capture", "soundColor": "white"},
                "pendingReplayVisuals": [], "pendingNotation": None, "pendingNotations": [],
                "boardHistory": [{"move": "p"}],
            },
        }

    def test_rng_and_sound_name_are_excluded(self) -> None:
        other = {**self.position, "positionId": "rust-id", "rng": {"state": 900}}
        other["state"] = {**self.position["state"], "lastMove": {
            **self.position["state"]["lastMove"], "soundName": "checkDanger"}}
        self.assertEqual(compare_positions(self.position, other).status, "PASS")

    def test_rule_fields_and_unclassified_fields_remain(self) -> None:
        other = {**self.position, "state": {**self.position["state"]}}
        other["state"]["lastMove"] = {**other["state"]["lastMove"], "soundColor": "black"}
        self.assertEqual(compare_positions(self.position, other).status, "MISMATCH")
        other["state"]["lastMove"] = self.position["state"]["lastMove"]
        other["state"]["newUnclassifiedField"] = 1
        self.assertEqual(compare_positions(self.position, other).status, "MISMATCH")

    def test_json_safe_number_spelling_does_not_change_rules(self) -> None:
        other = {**self.position, "state": {**self.position["state"], "moveCount": 1.0}}
        source = {**self.position, "state": {**self.position["state"], "moveCount": 1}}
        self.assertEqual(compare_positions(source, other).status, "PASS")

    def test_unsettled_presentation_replay_is_not_a_pass(self) -> None:
        other = {**self.position, "state": {**self.position["state"],
                                           "pendingReplayVisuals": [{"type": "vanish"}]}}
        self.assertEqual(compare_positions(self.position, other).status, "UNSUPPORTED")

    def test_rule_history_and_hidden_information_remain_compared(self) -> None:
        other = {**self.position, "state": {**self.position["state"]}}
        other["state"]["boardHistory"] = [{"move": "another"}]
        self.assertEqual(compare_positions(self.position, other).status, "MISMATCH")
        other["state"]["boardHistory"] = self.position["state"]["boardHistory"]
        other["state"]["hiddenHand"] = ["private-card"]
        self.assertEqual(compare_positions(self.position, other).status, "MISMATCH")

    def test_joint_probabilities_must_match(self) -> None:
        source = {"a+b": "1/2", "a+c": "1/2"}
        self.assertEqual(compare_exact_distribution(source, dict(source)).status, "PASS")
        self.assertEqual(compare_exact_distribution(source, {"a+b": "1/4", "a+c": "3/4"}).status,
                         "MISMATCH")
        self.assertEqual(compare_exact_distribution(source, {"a+b": "1/2"}).status, "UNSUPPORTED")

    def test_statistical_budget_is_inconclusive_when_small(self) -> None:
        result = compare_sample_distribution({"a": 1}, {"a": 1}, tolerance=0.01)
        self.assertEqual(result.status, "INCONCLUSIVE")
        self.assertEqual(compare_sample_distribution({}, {}, tolerance=0.01).status, "UNSUPPORTED")


class PairedReportTests(unittest.TestCase):
    def test_missing_probability_evidence_cannot_pass(self) -> None:
        from semantic_differential import compare_case
        position = {"state": {"board": [], "turn": "white"}}
        case = {
            "name": "minimal", "source": position, "rust": position,
            "sourceActions": [{"type": "move"}], "rustActions": [{"type": "move"}],
            "sourceAction": {"type": "move"}, "rustAction": {"type": "move"},
            "sourceObservations": {"white": {}, "black": {}},
            "rustObservations": {"white": {}, "black": {}},
            "sourceRejection": {"rejected": True, "unchanged": True},
            "rustRejection": {"rejected": True, "unchanged": True},
        }
        self.assertEqual(compare_case(case)["status"], "UNSUPPORTED")
        case["distribution"] = {"kind": "exact", "complete": True,
                                "source": {"next": "1"}, "rust": {"next": "1"}}
        self.assertEqual(compare_case(case)["status"], "PASS")
        case["sourceActions"] = [{"actionId": "source", "positionId": "old", "payload": {"type": "move"}}]
        case["rustActions"] = [{"actionId": "rust", "positionId": "new", "payload": {"type": "move"}}]
        self.assertEqual(compare_case(case)["status"], "PASS")
        case["rustActions"] = [{"type": "card"}]
        case["rustAction"] = {"type": "card"}
        self.assertEqual(compare_case(case)["status"], "MISMATCH")

if __name__ == "__main__":
    unittest.main()
