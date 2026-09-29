"""Small search contracts, followed by real bounded native integration.

The synthetic source is an information-set/chance test double, not a second
chess rules implementation. Native integration exercises the actual rules.
"""
from copy import deepcopy
from functools import lru_cache
from dataclasses import replace
import hashlib
import json
import math
from pathlib import Path
import time

import numpy as np
import pytest

from accelerate_chess.encoding import EncoderSpec, PublicEncoder, canonical_json
from accelerate_chess.inference import ProductionEvaluator
from accelerate_chess.ir import TypedEncoder, TypedEncoderSpec
from accelerate_chess.search import (BeliefLimits, InformationMismatchError, InformationSetSearch,
    MissingHistoryError, NativeSourceFactory, ParticleBelief, ParticleExhaustedError,
    PublicTracker, SearchBudgetError, SearchLimits, SourceCapabilityError, TransitionProposal,
    TypedInformationSetSearch, _allowed, _stream)
from test_model_stack import observation_policy


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
        return {"actions": page, "examined": len(page), "exhausted": self.offset == len(self.actions)}


class TestPosition:
    __test__ = False
    def __init__(self, latent=0, stage=0, *, chance=False, reaction=False, history=()):
        self.latent, self.stage, self.chance, self.reaction = latent, stage, chance, reaction
        self.history = list(history)
        self.decision_actor = "white" if stage == 0 or reaction else "black"
        self.result = ("white" if latent else "black") if chance and stage else None

    def observe(self, viewer):
        board = [[None] * 8 for _ in range(8)]
        board[6 - self.stage][1] = {"type": "pawn", "color": "white", "status": {}}
        board[2][0] = {"type": "wall", "color": "neutral", "status": {}}
        move = TestAction(self.stage, self.latent).public_intent()
        return sign({"protocolVersion": "accelerate-observation-v2", "viewer": viewer,
            "board": board, "turn": "white" if self.stage == 0 else "black",
            "ownCards": [], "opponentHandCount": 1, "history": deepcopy(self.history),
            "publicState": {"projectionVersion": observation_policy()["projectionVersion"], "observationPolicyHash": spec().observation_policy_hash, "deathmatchStatus": {"active": False, "warning": False}, "boardMarks": [], "relationships": [], "overlays": [], "gameStyle": "normal", "phase": "play", "revealedOpponentCards": [{"id": "slime", "instanceId": "revealed", "used": False}],
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
            "phase": "play", "boardChanges": [{"square": {"row": 6 - self.stage, "col": 1}, "before": {"type": "pawn", "color": "white", "status": {}}, "after": None}],
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
        return TransitionProposal(position.apply(action).position, 1., 1., 1., "synthetic-deterministic-step-v1")


class TestEvaluator(ProductionEvaluator):
    __test__ = False
    def __init__(self, spec, value=.6):
        self.spec, self.value = spec, value
        self.calls = []

    def evaluate(self, board, condition, action_features):
        self.calls.append((board.copy(), condition.copy(), action_features.copy()))
        return np.zeros(action_features.shape[:2], np.float32), np.full((len(board), 1), self.value, np.float32)


@lru_cache(maxsize=1)
def spec():
    policy = observation_policy()
    return EncoderSpec(policy["rulesVersion"], "a" * 64, ("pawn", "wall"), ("slime",), (), hashlib.sha256(canonical_json(policy).encode()).hexdigest(),
        piece_payload_bytes=128, public_payload_bytes=4096, action_payload_bytes=256,
        history_encoding="public-history-summary-v1", action_encoding="public-decision-intent-v1").with_observation_policy(policy)


def belief(*, chance=False, reaction=False, particles=16, typed_spec=None):
    tracker = PublicTracker(TestPosition(chance=chance, reaction=reaction).observe("white"),
                            typed_spec=typed_spec)
    return ParticleBelief(tracker, TestFactory(chance=chance, reaction=reaction), seed=19,
        limits=BeliefLimits(particles=particles, proposals=particles * 2, elapsed_ms=1000))


@lru_cache(maxsize=1)
def typed_spec():
    policy = observation_policy()
    catalog = {"schemaVersion": 1, "rulesVersion": policy["rulesVersion"],
               "catalogVersion": "synthetic-typed-v1", "pieceTypes": ["pawn", "wall"],
               "cards": [{"id": "slime", "draftCategory": "MIDDLE"}], "actionTypes": ["move"]}
    return TypedEncoderSpec.from_catalog(catalog, observation_policy=policy)


def test_source_pinned_grappler_primary_hint_allows_compound_public_choices():
    catalog = json.loads((Path(__file__).parents[2] / "bridge/catalog/site-20260928.json").read_text(encoding="utf-8"))
    assert next(file for file in catalog["source"]["files"] if file["name"] == "main-OahWs0tU.js")["sha256"] == (
        "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
    )
    # Frozen seed-19 grand first play: the visible Grappler hint is the queen
    # at d1, while the source exposes four public minor choices at b1/c1/f1/g1.
    instance = "grappler-xvn13x9he"
    observation = {"publicState": {"rulesVersion": catalog["rulesVersion"],
        "projectionVersion": "source-visible-20260928-v1", "legalHints": {"moves": [], "cardTargets": [
        {"cardInstanceId": instance, "targets": [{"row": 7, "col": 3}]}
    ]}}}
    intent = {"type": "card", "color": "white", "cardId": "grappler", "cardInstanceId": instance}
    for col in (1, 2, 5, 6):
        assert _allowed({**intent, "target": {"row": 7, "col": 3, "minor": {"row": 7, "col": col}}}, observation)
    assert not _allowed({**intent, "target": {"row": 7, "col": 2, "minor": {"row": 7, "col": 1}}}, observation)
    assert not _allowed({**intent, "cardInstanceId": "different", "target": {"row": 7, "col": 3}}, observation)
    for target in ({"selections": [{"row": 7, "col": 3}]}, {"row": 7}, [], {"row": True, "col": 3}):
        with pytest.raises(InformationMismatchError, match="primary click coordinates"):
            _allowed({**intent, "target": target}, observation)
    with pytest.raises(SourceCapabilityError, match="source rules version"):
        _allowed({**intent, "target": {"row": 7, "col": 3}}, {"publicState": {
            **observation["publicState"], "rulesVersion": "unverified"}})


def test_legacy_portal_gun_nested_selection_hints_remain_supported():
    policy = observation_policy()
    assert policy["rulesVersion"] == "augment-site-20260927-abfe01a035813875"
    squares = [{"row": 1, "col": 1}, {"row": 2, "col": 2}]
    observation = {"publicState": {"rulesVersion": policy["rulesVersion"],
        "projectionVersion": policy["projectionVersion"], "legalHints": {"moves": [], "cardTargets": [
            {"cardInstanceId": "portal-stream", "targets": squares}
        ]}}}
    intent = {"type": "card", "color": "white", "cardId": "portal-gun", "cardInstanceId": "portal-stream",
        "target": {"selections": squares}}
    assert _allowed(intent, observation)
    legacy = deepcopy(observation)
    del legacy["publicState"]["rulesVersion"]
    assert _allowed(intent, legacy)
    assert not _allowed(intent, {"publicState": {**observation["publicState"], "legalHints": {
        "moves": [], "cardTargets": [{"cardInstanceId": "portal-stream", "targets": squares[:1]}]}}})


def test_action_stream_counts_examined_candidates_and_rejects_zero_progress():
    class ScriptedStream:
        def __init__(self, pages):
            self.pages = iter(pages)
            self.requests = []

        def next_page(self, limit):
            self.requests.append(limit)
            return next(self.pages)

    def consume(pages):
        stream = ScriptedStream(pages)
        position = TestPosition()
        position.action_stream = lambda: stream
        return list(_stream(position, 2, 3)), stream.requests

    action = TestAction(0, 0)
    pages, requests = consume([
        {"actions": (), "exhausted": False, "examined": 2},
        {"actions": (action,), "exhausted": True, "examined": 1},
    ])
    assert pages == [((), False), ((action,), True)]
    assert requests == [2, 1]
    assert consume([{"actions": (), "exhausted": True, "examined": 0}])[0] == [((), True)]

    for invalid in (
        {"actions": (), "exhausted": False, "examined": 3},
        {"actions": (), "exhausted": False, "examined": 0},
        {"actions": (), "exhausted": False},
        {"actions": (action,), "exhausted": True},
        {"actions": (action,), "exhausted": True, "examined": 0},
    ):
        with pytest.raises(InformationMismatchError, match="invalid page"):
            consume([invalid])


def test_public_trace_identity_complete_history_and_belief_filter():
    initial = TestPosition().observe("white")
    tracker = PublicTracker(initial)
    hidden_actual = TestPosition(1)
    action = hidden_actual.bind_public_intent(TestAction(0, 0).public_intent())
    child = hidden_actual.apply(action).position
    tracker.append(child.observe("white"), own_intent=action.public_intent())
    step, frame = next(tracker.frames())
    assert frame == tracker.latest and step.own_intent == action.public_intent()
    assert list(tracker._frames_since(0)) == list(tracker.frames())
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
    # A known two-world proposal over-samples the less likely source world.
    # The density correction must affect filtering, rather than turn into
    # unweighted duplicate particles. No chess rules are implemented here.
    class WeightedFactory(TestFactory):
        def __init__(self, mode):
            super().__init__()
            self.generated, self.mode = 0, mode

        def sample_initial(self, initial, seed):
            self.generated += 1
            return TestPosition(self.generated % 2)

        def prepare_transition(self, position, expected, seed):
            if self.mode == "step":
                return TransitionProposal(position)
            source_probability = .75 if position.latent else .25
            return TransitionProposal(position, source_probability / .5,
                source_probability, .5, "synthetic-known-density-v1")

        def apply_conditioned(self, position, action, expected, seed):
            step = super().apply_conditioned(position, action, expected, seed)
            if self.mode == "proposal":
                return step
            # A forced observed event has proposal probability one, while its
            # source probability depends on the hidden world. This likelihood
            # must multiply the latent proposal correction, not replace it.
            probability = .75 if position.latent else .25
            return TransitionProposal(step.position, probability, probability, 1., "synthetic-observed-chance-v1")

    for mode, frequency in [("proposal", .75), ("step", .75), ("both", .9)]:
        observed = PublicTracker(TestPosition().observe("black"))
        weighted = ParticleBelief(observed, WeightedFactory(mode), seed=17,
            limits=BeliefLimits(particles=128, proposals=128, elapsed_ms=3000))
        observed.append(child.observe("black"))
        weighted.synchronize()
        assert sum(position.latent for position in weighted._particles) / 128 == pytest.approx(frequency, abs=.05)
        assert weighted.summary["effective_sample_size"] < 128
    assert weighted.summary["version"] == "public-particle-summary-v3"
    assert weighted.summary["chance_prior"] == "independent-source-draws"
    assert weighted.summary["conditional_steps"] == "source-weighted-conditional-step-v1"
    # Different hidden opponent intents can carry different observed-event
    # likelihoods. The child reservoir must use those masses, while the prior
    # denominator still counts both choices (mass (.25+.75)/2).
    class BranchFactory(TestFactory):
        def sample_initial(self, initial, seed):
            position = TestPosition()
            actions = [TestAction(0, index) for index in (0, 1)]
            for index, candidate in enumerate(actions):
                candidate.public_intent = lambda index=index: {"type": "move", "color": "white", "from": {"row": 6, "col": 1}, "move": {"row": 5, "col": index + 1}}
            position.action_stream = lambda: TestStream(actions)
            position.bind_public_intent = lambda intent: next(action for action in actions if action.public_intent() == intent)
            return position

        def apply_conditioned(self, position, action, expected, seed):
            child = position.apply(TestAction(0, position.latent)).position
            child.latent = action.latent
            probability = .75 if action.latent else .25
            return TransitionProposal(child, probability, probability, 1., "synthetic-observed-intent-chance-v1")

    branch_tracker = PublicTracker(TestPosition().observe("black"))
    branches = ParticleBelief(branch_tracker, BranchFactory(), seed=23,
        limits=BeliefLimits(particles=1, proposals=1, elapsed_ms=3000))
    branch_position = branches.draw()
    branch_tracker.append(child.observe("black"))
    transition, expected = next(branch_tracker.frames())
    samples = [branches._advance(branch_position, transition, expected, time.monotonic()) for _ in range(256)]
    assert all(log_weight == pytest.approx(math.log(.5)) for _, log_weight in samples)
    assert sum(position.latent for position, _ in samples) / 256 == pytest.approx(.75, abs=.07)
    with pytest.raises(InformationMismatchError, match="does not equal"):
        TransitionProposal(TestPosition(), 1., .75, .5)
    with pytest.raises(InformationMismatchError, match="finite and positive"):
        TransitionProposal(TestPosition(), float("nan"))


def test_native_source_chance_requires_conditioning_and_explicit_densities(monkeypatch):
    import sys
    from types import SimpleNamespace

    initial = TestPosition().observe("white")
    initial["publicState"].update(mode="draft", phase="OPENING", revealedOpponentCards=[])
    sign(initial)
    expected = deepcopy(initial)
    expected["publicState"]["revealedOpponentCards"] = [{"id": "slime", "instanceId": "revealed", "used": False}]
    sign(expected)
    factory = object.__new__(NativeSourceFactory)
    factory.typed_spec = None

    class MissingCondition:
        decision_actor = "black"

        def observe(self, viewer):
            return initial

    with pytest.raises(SourceCapabilityError, match="hidden opening offer conditioning"):
        factory.prepare_transition(MissingCondition(), expected, 7)

    monkeypatch.setitem(sys.modules, "accelerate_chess._native",
                        SimpleNamespace(ConditioningMismatchError=type("ConditioningMismatchError", (Exception,), {})))

    class MissingDensity(MissingCondition):
        def condition_hidden_opening_draft(self, expected, seed):
            return {"position": self, "importance_weight": 1.,
                    "source_probability": None, "proposal_probability": None}

        def apply_weighted_conditioned_public(self, action, expected, seed):
            return {"step": SimpleNamespace(position=self), "importance_weight": 1.,
                    "source_probability": None, "proposal_probability": None}

    with pytest.raises(InformationMismatchError, match="source/proposal chance density"):
        factory.prepare_transition(MissingDensity(), expected, 7)
    with pytest.raises(InformationMismatchError, match="source/proposal chance density"):
        factory.apply_conditioned(MissingDensity(), object(), expected, 7)


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
    class UnweightedFactory(TestFactory):
        def apply_conditioned(self, position, action, expected, seed):
            return position.apply(action).position
    unweighted = ParticleBelief(PublicTracker(TestPosition().observe("white")), UnweightedFactory(), seed=1,
        limits=BeliefLimits(particles=1, proposals=1))
    unweighted.tracker.append(TestPosition().apply(TestAction(0, 0)).position.observe("white"),
        own_intent=TestAction(0, 0).public_intent())
    with pytest.raises(InformationMismatchError, match="carry validated density"):
        unweighted.synchronize()


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


@pytest.mark.parametrize("depth,nodes", [(1, 2), (2, 1), (2, 2)])
def test_search_does_not_label_unconditioned_children_with_root_posterior(depth, nodes):
    requests = []

    class SummarySearch(InformationSetSearch):
        def _evaluate_many(self, batch, state):
            requests.extend((len(observation["history"]), summary)
                            for observation, _, summary in batch)
            return super()._evaluate_many(batch, state)

    posterior = belief(particles=2)
    result = SummarySearch(PublicEncoder(spec()), TestEvaluator(spec()),
                           limits=SearchLimits(iterations=1, max_depth=depth,
                                               max_nodes=nodes, leaf_batch_size=1,
                                               elapsed_ms=None)).run(posterior)
    assert requests[0] == (0, posterior.summary)
    assert requests[1] == (1, None)
    assert result.belief_summary == posterior.summary
    assert result.policy[0]["visits"] == 1


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


def test_work_limits_and_injected_clocks_have_separate_boundaries():
    initial = TestPosition().observe("white")

    def unexpected_clock():
        raise AssertionError("a finite-work completion run must not read wall time")

    posterior = ParticleBelief(PublicTracker(initial), TestFactory(), seed=19,
        limits=BeliefLimits(particles=2, proposals=2, elapsed_ms=None), clock=unexpected_clock)
    assert posterior.proposals_used == 2 and len(posterior._particles) == 2
    search = InformationSetSearch(PublicEncoder(spec()), TestEvaluator(spec()),
        limits=SearchLimits(iterations=1, max_depth=1, elapsed_ms=None), clock=unexpected_clock)
    assert search.run(posterior).iterations == 1

    belief_times = iter((0., .006))
    with pytest.raises(SearchBudgetError, match="belief reconstruction time budget exhausted"):
        ParticleBelief(PublicTracker(initial), TestFactory(), seed=19,
            limits=BeliefLimits(particles=1, proposals=1, elapsed_ms=5), clock=lambda: next(belief_times))
    with pytest.raises(SearchBudgetError, match="belief reconstruction cancelled"):
        ParticleBelief(PublicTracker(initial), TestFactory(), seed=19,
            limits=BeliefLimits(particles=1, proposals=1, elapsed_ms=None), cancelled=lambda: True)

    search_times = iter((0., .006))
    timed = InformationSetSearch(PublicEncoder(spec()), TestEvaluator(spec()),
        limits=SearchLimits(iterations=1, max_depth=1, elapsed_ms=5), clock=lambda: next(search_times))
    with pytest.raises(SearchBudgetError, match="no public decision was evaluated before elapsed"):
        timed.run(posterior)
    with pytest.raises(ValueError, match="elapsed_ms"):
        BeliefLimits(elapsed_ms=0)
    with pytest.raises(ValueError, match="elapsed_ms"):
        SearchLimits(elapsed_ms=0)


@pytest.mark.parametrize("family", ["mask-resnet", "entity-transformer"])
def test_typed_search_keeps_public_intent_and_family_tensor_contract(family, monkeypatch):
    contract = typed_spec()

    class TypedEvaluator(ProductionEvaluator):
        def __init__(self):
            self.spec = contract
            self.inputs = []

        @property
        def architecture_family(self):
            return family

        def evaluate_typed(self, inputs):
            self.inputs.append(inputs)
            return (np.zeros(inputs["candidate_mask"].shape, np.float32),
                    np.full((len(inputs["candidate_mask"]), 1), .25, np.float32))

    evaluator = TypedEvaluator()
    posterior = belief(particles=2, typed_spec=contract)
    search = TypedInformationSetSearch(TypedEncoder(contract), evaluator,
        limits=SearchLimits(iterations=1, max_depth=1, elapsed_ms=None))
    result = search.run(posterior)
    assert result.intent == TestAction(0, 0).public_intent()
    assert result.encoder_hash == contract.digest and result.iterations == 1
    assert evaluator.inputs
    assert all(tuple(inputs) == tuple(contract.feature_schema["input_order"][family]) for inputs in evaluator.inputs)
    assert all(inputs["candidate_mask"].dtype == np.bool_ for inputs in evaluator.inputs)

    exact_limit = max(sum(array.size for array in inputs.values()) for inputs in evaluator.inputs)
    exact = TypedInformationSetSearch(TypedEncoder(contract), evaluator,
        limits=SearchLimits(iterations=1, max_depth=1, elapsed_ms=None,
                            max_inference_elements=exact_limit))
    assert exact.run(belief(particles=2, typed_spec=contract)).iterations == 1

    elements = min(sum(array.size for array in inputs.values()) for inputs in evaluator.inputs)
    assert elements > 1
    capped = TypedInformationSetSearch(TypedEncoder(contract), evaluator,
        limits=SearchLimits(iterations=1, max_depth=1, elapsed_ms=None,
                            max_inference_elements=elements - 1))

    def padding_must_not_run(_):
        raise AssertionError("oversized typed batch must fail before padding")

    monkeypatch.setattr("accelerate_chess.ir.batch_typed_positions", padding_must_not_run)
    with pytest.raises(SearchBudgetError, match="inference element budget"):
        capped.run(belief(particles=2, typed_spec=contract))


def test_typed_tracker_binds_v7_public_projection_and_replays_exact_frames():
    from test_ir import frame as v7_public_frame, signed, spec as v7_typed_spec

    contract = v7_typed_spec()
    observation = v7_public_frame()
    tracker = PublicTracker(observation, typed_spec=contract)
    assert tracker.latest == observation and tracker.typed_spec.digest == contract.digest
    assert PublicTracker.from_snapshot(tracker.snapshot(), typed_spec=contract).latest == observation
    changed = replace(contract, observation_policy_hash="b" * 64)
    with pytest.raises(ValueError, match="policy|contract|provenance"):
        PublicTracker(observation, typed_spec=changed)
    with pytest.raises(ValueError, match="public|snapshot|contract"):
        PublicTracker({**observation, "positionId": "hidden-private-id"}, typed_spec=contract)
    rectangle = deepcopy(observation)
    rectangle["board"] = [row[:7] for row in rectangle["board"][:5]]
    rectangle["publicState"]["collapsedCells"] = []
    signed(rectangle)
    with pytest.raises(ValueError, match="source-bound.*8x8"):
        PublicTracker(rectangle, typed_spec=contract)
    with pytest.raises(InformationMismatchError, match="8 by 8"):
        PublicTracker(rectangle)


@pytest.mark.parametrize("mode,draft_delete", [("normal", True), ("chaos", True), ("grand", True), ("grand", False)])
def test_native_supported_conditioned_modes_and_public_intent_integration(mode, draft_delete):
    """No skip: production source factory/intent are required for completion."""
    from accelerate_chess import Position, site_observation_policy
    catalog = json.loads((Path(__file__).parents[2] / "bridge/catalog/site-20260927.json").read_text(encoding="utf-8"))
    encoder = PublicEncoder(EncoderSpec.from_catalog(catalog, observation_policy=site_observation_policy(), history_encoding="public-history-summary-v1", action_encoding="public-decision-intent-v1"))
    _native_mode_flow(mode, draft_delete, encoder, Position)


@pytest.mark.parametrize("mode", ["normal", "chaos"])
def test_native_default_weighted_conditioning_completion_gate(mode, record_property):
    """No skip: default draft-to-play requires both public posteriors."""
    from accelerate_chess import Position, site_observation_policy
    catalog = json.loads((Path(__file__).parents[2] / "bridge/catalog/site-20260927.json").read_text(encoding="utf-8"))
    encoder = PublicEncoder(EncoderSpec.from_catalog(catalog, observation_policy=site_observation_policy(), history_encoding="public-history-summary-v1", action_encoding="public-decision-intent-v1"))
    started = time.monotonic()
    try:
        _native_mode_flow(mode, False, encoder, Position)
    finally:
        # The CI subprocess has a finite wall watchdog. This property records
        # duration without turning machine load into a rule-correctness gate.
        record_property("wall_seconds", round(time.monotonic() - started, 3))


def _native_mode_flow(mode, draft_delete, encoder, Position):
    config = {"gameStyle": mode, "draftDelete": draft_delete}
    actual = Position.new_game(config, 37)
    trackers = {viewer: PublicTracker(actual.observe(viewer)) for viewer in ("white", "black")}
    beliefs = {viewer: ParticleBelief(tracker, NativeSourceFactory(config), seed=71 + index,
        limits=BeliefLimits(particles=2, proposals=16, actions_per_transition=4096, elapsed_ms=None))
        for index, (viewer, tracker) in enumerate(trackers.items())}
    # At most twelve grand picks and one actual play action. Candidates come
    # only from a sampled source world; the actual environment binds afterward.
    for _ in range(13):
        actor = actual.decision_actor
        posterior = beliefs[actor]
        posterior.synchronize()
        observation = trackers[actor].latest
        mode_before = observation["publicState"]["mode"]
        sampled = posterior.draw()
        streamed = None
        for actions, _ in _stream(sampled, 1, 4096):
            if actions:
                streamed = actions[0]
                break
        assert streamed is not None
        intent = streamed.public_intent()
        if mode_before == "draft" and trackers[actor].steps == 0:
            assert posterior.factory.bind_streamed_public_intent(sampled, streamed, intent) is streamed
            assert canonical_json(streamed.as_payload()) == canonical_json(
                sampled.bind_public_intent(intent).as_payload())
        feature = encoder.encode(observation, [intent], belief_summary=posterior.summary)
        assert feature.action_keys == (canonical_json(intent),)
        stepped = actual.apply(actual.bind_public_intent(intent)).position
        for viewer, tracker in trackers.items():
            tracker.append(stepped.observe(viewer), own_intent=intent if viewer == actor else None)
        actual = stepped
        for viewer, belief in beliefs.items():
            belief.synchronize()
            assert canonical_json(belief.draw().observe(viewer)) == canonical_json(trackers[viewer].latest)
        if mode_before == "play":
            assert trackers[actor].steps == (1 if draft_delete else 13 if mode == "grand" else 3)
            return
    pytest.fail("finite native default flow did not complete draft and one play transition")
