"""Replay verdict contracts. Written for user execution; not run here."""

import unittest
from copy import deepcopy

from semantic_differential import _replay_sequence, replay_diagnostics


def position(n):
    return {"state": {"turn": "white", "board": [[n]], "moveReplay": {"white": None}}}


def step(kind, n=1, actions=None):
    actions = actions if actions is not None else [{"type": "move", "color": "white"}]
    return {"kind": kind, "beforeActions": actions, "position": position(n),
            "actions": actions, "observations": {"white": {}, "black": {}},
            "replayFrame": {"delta": [{"row": 1, "col": 1}]},
            "result": {"status": "ongoing", "winner": None, "outcome": None}}


class ReplaySequenceTests(unittest.TestCase):
    def test_missing_and_incomplete_are_never_pass(self):
        for evidence in (None, {"status": "unsupported", "reason": "no bridge"},
                         {"status": "complete", "sourceSteps": [], "rustSteps": []}):
            self.assertEqual(_replay_sequence(evidence, True)[0].status, "UNSUPPORTED")

    def test_complete_execution_remains_inconclusive(self):
        steps = [step("replay"), step("follow")]
        verdict, details = _replay_sequence({"status": "complete", "sourceSteps": steps,
                                             "rustSteps": deepcopy(steps)}, True)
        self.assertEqual(verdict.status, "INCONCLUSIVE")
        self.assertEqual(len(details), 2)

    def test_equal_state_legal_difference_is_mismatch(self):
        source = [step("replay"), step("follow")]
        rust = deepcopy(source)
        rust[1]["beforeActions"] = [{"type": "card", "cardId": "other"}]
        verdict, details = _replay_sequence({"status": "complete", "sourceSteps": source,
                                             "rustSteps": rust}, True)
        self.assertEqual(verdict.status, "MISMATCH")
        self.assertEqual(details[1]["checks"]["beforeLegalActions"]["status"], "MISMATCH")

    def test_independent_outcomes_do_not_make_false_mismatch(self):
        source = [step("replay"), step("follow")]
        rust = deepcopy(source)
        rust[0]["position"] = position(2)
        rust[0]["actions"] = [{"type": "card", "cardId": "other"}]
        verdict, details = _replay_sequence({"status": "complete", "sourceSteps": source,
                                             "rustSteps": rust}, True)
        self.assertEqual(verdict.status, "INCONCLUSIVE")
        self.assertEqual(details[0]["checks"]["nextRuleState"]["status"], "INCONCLUSIVE")

    def test_equal_state_followup_observation_difference_is_mismatch(self):
        source = [step("replay"), step("follow")]
        rust = deepcopy(source)
        rust[1]["observations"]["white"] = {"changed": True}
        self.assertEqual(_replay_sequence({"status": "complete", "sourceSteps": source,
                                           "rustSteps": rust}, True)[0].status, "MISMATCH")

    def test_unavailable_bridge_requires_complete_evidence(self):
        self.assertEqual(_replay_sequence({"status": "availability", "sourceSteps": [step("bridge")],
                                           "rustSteps": []}, True)[0].status, "UNSUPPORTED")

    def test_step_mismatch_reaches_summary(self):
        source = [step("replay"), step("follow")]
        rust = deepcopy(source)
        rust[1]["actions"] = [{"type": "card", "cardId": "other"}]
        verdict, steps = _replay_sequence({"status": "complete", "sourceSteps": source,
                                           "rustSteps": rust}, True)
        case = {"checks": {"replaySequence": verdict.as_dict()}, "replaySteps": steps,
                "replaySequence": {"status": "complete", "sourceStatus": "complete",
                                   "sourceSteps": source}}
        summary = replay_diagnostics([case], [{"attempts": 3, "acquired": {"color": "white"}}])
        self.assertEqual(summary["Replay Mismatch"], 1)
        self.assertEqual(summary["Replay Step Mismatches"], 1)
        self.assertEqual(summary["Replay Search Attempts"], 3)


if __name__ == "__main__":
    unittest.main()
