"""Small search contracts, followed by real bounded native integration.

The synthetic source is an information-set/chance test double, not a second
chess rules implementation. Native integration exercises the actual rules.
"""
from copy import deepcopy
from dataclasses import replace
import hashlib
import json
from pathlib import Path

import numpy as np
import pytest

from accelerate_chess.encoding import EncoderSpec, PublicEncoder, canonical_json
from accelerate_chess.inference import ProductionEvaluator
from accelerate_chess.search import (BeliefLimits, InformationMismatchError, InformationSetSearch,
    MissingHistoryError, NativeSourceFactory, ParticleBelief, ParticleExhaustedError,
    PublicTracker, SearchBudgetError, SearchLimits)


def sign(frame):
    frame["informationStateKey"] = hashlib.sha256(canonical_json({key: value for key, value in frame.items() if key != "informationStateKey"}).encode()).hexdigest()
    return frame


class TestAction:
    __test__ = False
    def __init__(self, stage, latent):
        self.stage, self.latent = stage, latent

    def public_intent(self):
        return {"type": "move", "color": "white", "from": {"row": 6 - self.stage, "col": 1}, "move": {"row": 5 - self.stage, "col": 1}}

    def as_payload(self):
        # Native-only capture/piece ID differs despite one public UI choice.
        return {**self.public_intent(), "move": {"row": 5 - self.stage, "col": 1, "capture": bool(self.latent), "pieceId": f"private-{self.latent}"}}


class TestStream:
    __test__ = False
    def __init__(self, actions):
        self.actions, self.offset = actions, 0

    def next_page(self, limit):
        page = self.actions[self.offset:self.offset + limit]
        self.offset += len(page)
        return {"actions": page, "exhausted": self.offset == len(self.actions)}


class TestPosition:
    __test__ = False
    def __init__(self, latent=0, stage=0, *, chance=False, reaction=False, history=()):
        self.latent, self.stage, self.chance, self.reaction = latent, stage, chance, reaction
        self.history = list(history)
        self.decision_actor = "white" if stage == 0 or reaction else "black"
        self.result = ("white" if latent else "black") if chance and stage else None

    def observe(self, viewer):
        board = [[None] * 8 for _ in range(8)]
        board[6 - self.stage][1] = {"type": "pawn", "color": "white"}
        board[2][0] = {"type": "wall", "color": "neutral", "id": "wall-2-0"}
        move = TestAction(self.stage, self.latent).public_intent()
        return sign({"protocolVersion": "accelerate-observation-v1", "viewer": viewer,
            "board": board, "turn": "white" if self.stage == 0 else "black",
            "ownCards": [], "opponentHandCount": 1, "history": deepcopy(self.history),
            "publicState": {"gameStyle": "normal", "phase": "play", "revealedOpponentCards": [{"id": "slime", "instanceId": "revealed", "used": False}],
                "legalHints": {"moves": [{"from": move["from"], "destinations": [move["move"]]}], "cardTargets": []}}, "informationStateKey": ""})

    def action_stream(self):
        return TestStream([TestAction(self.stage, self.latent)])

    def bind_public_intent(self, intent):
        action = TestAction(self.stage, self.latent)
        if intent != action.public_intent():
            raise ValueError("impossible public choice")
        return action

    def apply(self, action):
        assert action.as_payload()["move"]["capture"] == bool(self.latent)
        child = TestPosition(self.latent, self.stage + 1, chance=self.chance, reaction=self.reaction, history=self.history)
        event = {"kind": "transition", "actor": self.decision_actor, "nextActor": child.decision_actor,
            "phase": "play", "boardChanges": [{"square": {"row": 6 - self.stage, "col": 1}, "before": {"type": "pawn", "color": "white"}, "after": None}],
            "ownCards": [], "revealedOpponentCards": [{"id": "slime", "instanceId": "revealed", "used": False}], "captures": {"white": [], "black": []},
            "result": {"protocolVersion": "accelerate-result-v1", "status": "terminal" if child.result else "ongoing", "winner": child.result, "outcome": child.result, "reason": ""}}
        child.history.append(event)
        return type("Step", (), {"position": child, "turn_changed": True})()


class TestFactory:
    __test__ = False
    def __init__(self, *, chance=False, reaction=False):
        self.chance, self.reaction = chance, reaction
        self.seeds = []

    def sample_initial(self, initial, seed):
        self.seeds.append(seed)
        return TestPosition(seed % 2, chance=self.chance, reaction=self.reaction)

    def apply_conditioned(self, position, action, expected, seed):
        return position.apply(action).position


class TestEvaluator(ProductionEvaluator):
    __test__ = False
    def __init__(self, spec, value=.6):
        self.spec, self.value = spec, value
        self.calls = []

    def evaluate(self, board, condition, action_features):
        self.calls.append((board.copy(), condition.copy(), action_features.copy()))
        return np.zeros(action_features.shape[:2], np.float32), np.full((len(board), 1), self.value, np.float32)


def spec():
    return EncoderSpec("site-small", "a" * 64, ("pawn", "wall"), ("slime",), (),
        piece_payload_bytes=128, public_payload_bytes=4096, action_payload_bytes=256,
        history_encoding="public-history-summary-v1", action_encoding="public-decision-intent-v1")


def belief(*, chance=False, reaction=False, particles=16):
    tracker = PublicTracker(TestPosition(chance=chance, reaction=reaction).observe("white"))
    return ParticleBelief(tracker, TestFactory(chance=chance, reaction=reaction), seed=19,
        limits=BeliefLimits(particles=particles, proposals=particles * 2, elapsed_ms=1000))


def test_public_trace_identity_complete_history_and_belief_filter():
    initial = TestPosition().observe("white")
    tracker = PublicTracker(initial)
    hidden_actual = TestPosition(1)
    action = hidden_actual.bind_public_intent(TestAction(0, 0).public_intent())
    child = hidden_actual.apply(action).position
    tracker.append(child.observe("white"), own_intent=action.public_intent())
    step, frame = next(tracker.frames())
    assert frame == tracker.latest and step.own_intent == action.public_intent()
    snapshot = tracker.snapshot()
    assert "history" not in {change["path"][0] for change in snapshot["steps"][0]["patch"]}
    reconstructed = ParticleBelief(tracker, TestFactory(), seed=9, limits=BeliefLimits(particles=4, proposals=8))
    assert reconstructed.draw().observe("white") == tracker.latest
    assert reconstructed.summary["opponent_action_prior"] == "uniform-public-intents"
    with pytest.raises(MissingHistoryError):
        PublicTracker(child.observe("white"))
    with pytest.raises(MissingHistoryError):
        tracker.append(child.observe("white"))
    tampered = initial.copy()
    tampered["informationStateKey"] = "a" * 64
    with pytest.raises(ValueError, match="identity mismatch"):
        PublicTracker(tampered)
    leaked = deepcopy(initial)
    leaked["publicState"]["rngState"] = {"seed": 17}
    sign(leaked)
    with pytest.raises(ValueError, match="private field"):
        PublicTracker(leaked)
    wrong_viewer = child.observe("black")
    with pytest.raises(InformationMismatchError, match="viewer"):
        tracker.append(wrong_viewer)


def test_unmatched_public_trace_and_empty_belief_fail_explicitly():
    posterior = belief(particles=4)
    child = TestPosition().apply(TestAction(0, 0)).position.observe("white")
    child["board"][0][0] = {"type": "wall", "color": "neutral"}
    sign(child)
    posterior.tracker.append(child)
    with pytest.raises(ParticleExhaustedError):
        posterior.synchronize()
    valid = belief(particles=4)
    valid._particles.clear()  # Force the owned filter's exhausted boundary.
    with pytest.raises(ParticleExhaustedError):
        valid.draw()
    with pytest.raises(TypeError, match="public tracker"):
        ParticleBelief(TestPosition(), TestFactory(), seed=1)
    class RejectedFactory(TestFactory):
        def sample_initial(self, initial, seed):
            return None
    with pytest.raises(ParticleExhaustedError):
        ParticleBelief(PublicTracker(TestPosition().observe("white")), RejectedFactory(), seed=1,
                      limits=BeliefLimits(particles=1, proposals=2))


def test_puct_value_sign_uses_decision_actor_and_chance_is_sampled():
    contract = spec()
    evaluator = TestEvaluator(contract)
    search = InformationSetSearch(PublicEncoder(contract), evaluator,
        limits=SearchLimits(iterations=1, max_depth=1, elapsed_ms=1000))
    same_actor = search.run(belief(reaction=True))
    changed_actor = search.run(belief(reaction=False))
    assert same_actor.policy[0]["value"] == pytest.approx(.6)
    assert changed_actor.policy[0]["value"] == pytest.approx(-.6)
    # Physical turn changes in both cases; it never controls the value sign.
    chance_search = InformationSetSearch(PublicEncoder(contract), TestEvaluator(contract),
        limits=SearchLimits(iterations=80, max_depth=2, elapsed_ms=2000))
    stochastic = chance_search.run(belief(chance=True, particles=32))
    assert -.5 < stochastic.policy[0]["value"] < .5
    assert stochastic.policy[0]["visits"] == stochastic.policy[0]["availability"] == 80
    assert stochastic.legal_actions_exhausted
    assert stochastic.max_inference_batch == 4 and stochastic.inference_batches == 20
    assert all(len(board) == 4 for board, _, _ in chance_search.evaluator.calls)


def test_hidden_execution_flags_do_not_change_public_features_or_choice():
    assert TestAction(0, 0).as_payload() != TestAction(0, 1).as_payload()
    contract = spec()
    first_eval, second_eval = TestEvaluator(contract), TestEvaluator(contract)
    limits = SearchLimits(iterations=5, max_depth=1, elapsed_ms=1000)
    first = InformationSetSearch(PublicEncoder(contract), first_eval, limits=limits).run(belief())
    second = InformationSetSearch(PublicEncoder(contract), second_eval, limits=limits).run(belief())
    assert first == second
    for a, b in zip(first_eval.calls, second_eval.calls):
        for x, y in zip(a, b):
            np.testing.assert_array_equal(x, y)
    assert "capture" not in canonical_json(first.intent) and "private-" not in first.action_key
    actual_one, actual_two = TestPosition(0), TestPosition(1)
    assert first.bind(actual_one).as_payload() != first.bind(actual_two).as_payload()
    with pytest.raises(TypeError, match="native Rust"):
        InformationSetSearch(PublicEncoder(contract), object())
    with pytest.raises(ValueError, match="public-decision-intent"):
        old = replace(contract, action_encoding="exact-payload")
        InformationSetSearch(PublicEncoder(old), TestEvaluator(old))


def test_progressive_widening_cancellation_and_finite_budgets():
    class WidePosition(TestPosition):
        def action_stream(self):
            choices = []
            for col in range(8):
                action = TestAction(0, self.latent)
                action.public_intent = lambda col=col: {"type": "move", "color": "white", "from": {"row": 6, "col": 1}, "move": {"row": 5, "col": col}}
                choices.append(action)
            return TestStream(choices)

        def observe(self, viewer):
            observation = super().observe(viewer)
            observation["publicState"]["legalHints"]["moves"][0]["destinations"] = [{"row": 5, "col": col} for col in range(8)]
            return sign(observation)

        def bind_public_intent(self, intent):
            return TestAction(0, self.latent)

    class WideFactory(TestFactory):
        def sample_initial(self, initial, seed):
            return WidePosition(seed % 2)

    tracker = PublicTracker(WidePosition().observe("white"))
    posterior = ParticleBelief(tracker, WideFactory(), seed=19, limits=BeliefLimits(particles=2, proposals=2))
    limits = SearchLimits(iterations=1, max_depth=1, max_candidates=1, page_size=1, max_examined_actions=8)
    search = InformationSetSearch(PublicEncoder(spec()), TestEvaluator(spec()), limits=limits)
    result = search.run(posterior)
    assert result.partial_coverage and not result.legal_actions_exhausted and result.edges == 1
    with pytest.raises(SearchBudgetError, match="cancelled"):
        search.run(posterior, cancelled=lambda: True)
    with pytest.raises(ValueError, match="finite limit"):
        SearchLimits(iterations=0)
    # A completed group survives cancellation during the next native batch;
    # incomplete paths get no fabricated visits/value or dangling reservations.
    evaluator = TestEvaluator(spec())
    original_evaluate = evaluator.evaluate
    stopped = False
    def cancel_in_batch(*args):
        nonlocal stopped
        result = original_evaluate(*args)
        if len(evaluator.calls) == 3:
            stopped = True
        return result
    evaluator.evaluate = cancel_in_batch
    cancelled_search = InformationSetSearch(PublicEncoder(spec()), evaluator,
                          limits=SearchLimits(iterations=12, max_depth=1, elapsed_ms=1000))
    result = cancelled_search.run(belief(), cancelled=lambda: stopped)
    assert result.stop_reason == "cancelled" and result.iterations == 4
    assert sum(item["visits"] for item in result.policy) == 4
    budget_search = InformationSetSearch(PublicEncoder(spec()), TestEvaluator(spec()),
                        limits=SearchLimits(iterations=4, max_inference_elements=1))
    with pytest.raises(SearchBudgetError, match="inference input budget"):
        budget_search.run(belief())


@pytest.mark.parametrize("mode,draft_delete", [("normal", True), ("chaos", True), ("grand", True), ("grand", False)])
def test_native_supported_conditioned_modes_and_public_intent_integration(mode, draft_delete):
    """No skip: production source factory/intent are required for completion."""
    from accelerate_chess import Position
    catalog = json.loads((Path(__file__).parents[2] / "bridge/catalog/site-20260927.json").read_text(encoding="utf-8"))
    encoder = PublicEncoder(EncoderSpec.from_catalog(catalog, history_encoding="public-history-summary-v1", action_encoding="public-decision-intent-v1"))
    _native_mode_flow(mode, draft_delete, encoder, Position)


@pytest.mark.parametrize("mode", ["normal", "chaos"])
def test_native_default_weighted_conditioning_completion_gate(mode):
    """No skip/xfail: this gate stays red until real inverse source sampling exists."""
    from accelerate_chess import Position
    catalog = json.loads((Path(__file__).parents[2] / "bridge/catalog/site-20260927.json").read_text(encoding="utf-8"))
    encoder = PublicEncoder(EncoderSpec.from_catalog(catalog, history_encoding="public-history-summary-v1", action_encoding="public-decision-intent-v1"))
    _native_mode_flow(mode, False, encoder, Position)


def _native_mode_flow(mode, draft_delete, encoder, Position):
    config = {"gameStyle": mode, "draftDelete": draft_delete}
    actual = Position.new_game(config, 37)
    observation = actual.observe(actual.decision_actor)
    tracker = PublicTracker(observation)
    posterior = ParticleBelief(tracker, NativeSourceFactory(config), seed=71,
        limits=BeliefLimits(particles=2, proposals=4, actions_per_transition=4096, elapsed_ms=3000))
    sampled = posterior.draw()
    page = sampled.action_stream().next_page(1)
    assert page["actions"]
    intent = page["actions"][0].public_intent()
    feature = encoder.encode(observation, [intent], belief_summary=posterior.summary)
    assert feature.action_keys == (canonical_json(intent),)
    selected = actual.bind_public_intent(intent)
    stepped = actual.apply(selected).position
    tracker.append(stepped.observe(tracker.viewer), own_intent=intent)
    posterior.synchronize()
    assert posterior.draw().observe(tracker.viewer) == tracker.latest
