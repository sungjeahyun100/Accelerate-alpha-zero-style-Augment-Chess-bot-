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
from accelerate_chess.ir import ObservationIR, TypedEncoder, TypedEncoderSpec, batch_typed_positions
from accelerate_chess.search import (BeliefLimits, InformationMismatchError, InformationSetSearch,
    MissingHistoryError, NativeSourceFactory, ParticleBelief, ParticleExhaustedError,
    PublicTracker, SearchBudgetError, SearchLimits, SourceCapabilityError, TransitionProposal,
    TypedInformationSetSearch, _SearchState, _allowed, _bind_source_public_intent, _stream)
from test_ir import frame as v7_public_frame, source as v7_source
from test_model_stack import observation_policy


def test_native_source_factory_rejects_retired_v6_execution():
    with pytest.raises(SourceCapabilityError, match="v6 game execution is retired"):
        NativeSourceFactory({"gameStyle": "normal"})


def test_host_public_intent_roundtrip_rejects_extra_fields_and_reordered_targets():
    source = {"type": "card", "cardId": "slime",
              "orderedTargets": [{"row": 3, "col": 2}, {"row": 4, "col": 2}]}

    class Bound:
        def public_intent(self):
            return deepcopy(source)

    class Host:
        def bind_public_intent(self, _intent):
            return Bound()

    assert isinstance(_bind_source_public_intent(Host(), source), Bound)
    with pytest.raises(InformationMismatchError, match="public intent fields"):
        _bind_source_public_intent(Host(), {**source, "unverified": True})
    with pytest.raises(InformationMismatchError, match="ordered selections"):
        _bind_source_public_intent(Host(), {**source, "orderedTargets": list(reversed(source["orderedTargets"]))})
    with pytest.raises(SourceCapabilityError, match="cannot validate"):
        _bind_source_public_intent(object(), source)


def sign(frame):
    frame["informationStateKey"] = hashlib.sha256(canonical_json({key: value for key, value in frame.items() if key != "informationStateKey"}).encode()).hexdigest()
    return frame


class TestAction:
    __test__ = False
    def __init__(self, stage, latent):
        self.stage, self.latent = stage, latent

    def public_intent(self):
        return {"type": "move", "color": "white", "from": {"row": 6 - self.stage, "col": 1},
                "destination": {"row": 5 - self.stage, "col": 1}}

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
        observation = v7_public_frame()
        board = [[None] * 8 for _ in range(8)]
        board[6 - self.stage][1] = {"type": "pawn", "color": "white", "status": {}}
        board[2][0] = {"type": "wall", "color": "neutral", "status": {}}
        move = TestAction(self.stage, self.latent).public_intent()
        observation.update({"viewer": viewer, "board": board,
                            "turn": "white" if self.stage == 0 else "black",
                            "ownCards": [], "opponentHandCount": 1,
                            "history": deepcopy(self.history)})
        observation["publicState"].update({
            "gameStyle": "normal", "mode": "play", "phase": "OPENING",
            "revealedOpponentCards": [{"id": "slime", "instanceId": "revealed", "used": False}],
            "legalHints": {"moves": [{"from": move["from"],
                                       "destinations": [move["destination"]]}],
                           "cardTargets": []},
        })
        return sign(observation)

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
    catalog, policy = v7_source()
    return EncoderSpec.from_catalog(catalog, observation_policy=policy,
        piece_payload_bytes=128, public_payload_bytes=4096, action_payload_bytes=256,
        history_encoding="public-history-summary-v1", action_encoding="public-decision-intent-v1")


def belief(*, chance=False, reaction=False, particles=16, typed_spec=None):
    tracker = PublicTracker(TestPosition(chance=chance, reaction=reaction).observe("white"),
                            typed_spec=typed_spec)
    return ParticleBelief(tracker, TestFactory(chance=chance, reaction=reaction), seed=19,
        limits=BeliefLimits(particles=particles, proposals=particles * 2, elapsed_ms=1000))


@lru_cache(maxsize=1)
def typed_spec():
    catalog, policy = v7_source()
    return TypedEncoderSpec.from_catalog(catalog, observation_policy=policy)


def test_source_pinned_grappler_primary_hint_allows_compound_public_choices():
    catalog = json.loads((Path(__file__).resolve().parents[3] / "augment-chess/contracts/catalog/site-20260928.json").read_text(encoding="utf-8"))
    assert next(file for file in catalog["source"]["files"] if file["name"] == "main-OahWs0tU.js")["sha256"] == (
        "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
    )
    # Frozen seed-19 grand first play: the visible Grappler hint is the queen
    # at d1, while the source exposes four public minor choices at b1/c1/f1/g1.
    instance = "grappler-xvn13x9he"
    observation = {"publicState": {"rulesVersion": catalog["rulesVersion"],
        "projectionVersion": "source-visible-20260928-v2", "legalHints": {"moves": [], "cardTargets": [
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
    assert consume([{"actions": (action,), "exhausted": True, "examined": 0}])[0] == [((action,), True)]

    # The second page drains an already examined candidate's public alias.
    pages, requests = consume([
        {"actions": (action,), "exhausted": False, "examined": 1},
        {"actions": (action,), "exhausted": False, "examined": 0},
        {"actions": (), "exhausted": True, "examined": 1},
    ])
    assert pages == [((action,), False), ((action,), False), ((), True)]
    assert requests == [2, 2, 1]

    # An unchecked alias producer cannot bypass the finite result budget.
    pages, requests = consume([
        {"actions": (action,), "exhausted": False, "examined": 0}
        for _ in range(3)
    ])
    assert len(pages) == len(requests) == 3
    assert requests == [2, 2, 1]
    assert not pages[-1][1]

    for invalid in (
        {"actions": (), "exhausted": False, "examined": 3},
        {"actions": (), "exhausted": False, "examined": 0},
        {"actions": (), "exhausted": False},
        {"actions": (action,), "exhausted": True},
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
                candidate.public_intent = lambda index=index: {"type": "move", "color": "white", "from": {"row": 6, "col": 1}, "destination": {"row": 5, "col": index + 1}}
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
    with pytest.raises(SourceCapabilityError, match="transition compatibility"):
        factory.transition_compatible(MissingCondition(), object(), expected)

    monkeypatch.setitem(sys.modules, "accelerate_chess._native",
                        SimpleNamespace(ConditioningMismatchError=type("ConditioningMismatchError", (Exception,), {})))

    class MissingDensity(MissingCondition):
        def condition_hidden_opening_draft(self, expected, seed):
            return {"position": self, "importance_weight": 1.,
                    "source_probability": None, "proposal_probability": None}

        def apply_weighted_conditioned_public(self, action, expected, seed):
            return {"position": self, "importance_weight": 1.,
                    "source_probability": None, "proposal_probability": None}

    with pytest.raises(InformationMismatchError, match="source/proposal chance density"):
        factory.prepare_transition(MissingDensity(), expected, 7)
    with pytest.raises(InformationMismatchError, match="source/proposal chance density"):
        factory.apply_conditioned(MissingDensity(), object(), expected, 7)

    class InvalidSourcePrior(MissingCondition):
        def __init__(self, weight, source, proposed):
            self.weight, self.source, self.proposed = weight, source, proposed

        def observe(self, viewer):
            before = deepcopy(initial)
            before["publicState"]["mode"] = "play"
            sign(before)
            return before

        def apply_weighted_conditioned_public(self, action, expected, seed):
            return {"position": self, "importance_weight": self.weight,
                    "source_probability": self.source, "proposal_probability": self.proposed}

    # These doubles exercise only rejection of impossible metadata. Successful
    # source-prior reconstruction is exercised by the real native mode flows.
    factory._position_type = InvalidSourcePrior
    for metadata in ((2., .5, .25), (2., .5, .5)):
        with pytest.raises(InformationMismatchError, match="source-prior play transition"):
            factory.apply_conditioned(InvalidSourcePrior(*metadata), object(), expected, 7)


def test_native_streamed_public_actions_reuse_source_admission_across_families():
    factory = object.__new__(NativeSourceFactory)

    class StreamPosition:
        snapshot_revision = "source-revision"

        def bind_public_intent(self, _intent):
            raise AssertionError("a streamed candidate must not re-enumerate source actions")

    class StreamAction:
        revision = "source-revision"

        def __init__(self, intent):
            self.intent = intent

        def public_intent(self):
            return deepcopy(self.intent)

    factory._position_type = StreamPosition
    factory._action_type = StreamAction
    position = StreamPosition()
    for intent in ({"type": "move", "color": "white", "from": {"row": 6, "col": 4},
                    "destination": {"row": 5, "col": 4}},
                   {"type": "card", "color": "white", "cardId": "slime",
                    "cardInstanceId": "public-1", "target": {"row": 3, "col": 2}},
                   {"type": "trolleyChoice", "choice": "left"}):
        action = StreamAction(intent)
        assert factory.bind_streamed_public_intent(position, action, intent) is action
        with pytest.raises(InformationMismatchError, match="changed its public intent"):
            factory.bind_streamed_public_intent(position, action, {**intent, "extra": True})
        action.revision = "stale-revision"
        with pytest.raises(InformationMismatchError, match="another Position revision"):
            factory.bind_streamed_public_intent(position, action, intent)


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
                action.public_intent = lambda col=col: {"type": "move", "color": "white", "from": {"row": 6, "col": 1}, "destination": {"row": 5, "col": col}}
                choices.append(action)
            return TestStream(choices)

        def observe(self, viewer):
            observation = super().observe(viewer)
            observation["publicState"]["legalHints"]["moves"][0]["destinations"] = [{"row": 5, "col": col} for col in range(8)]
            return sign(observation)

        def bind_public_intent(self, intent):
            return next(action for action in self.action_stream().actions
                        if action.public_intent() == intent)

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


@pytest.mark.parametrize("family", ["mask-resnet", "entity-transformer"])
def test_typed_leaf_budget_is_checked_before_padded_batch_allocation(family, monkeypatch):
    from test_ir import frame as v7_frame, intents as v7_intents, spec as v7_spec

    contract = v7_spec()
    request = (v7_frame(), v7_intents(), None)
    encoded = TypedEncoder(contract).encode(ObservationIR.from_public(request[0], contract), request[1])
    padded = batch_typed_positions([encoded])
    order = contract.feature_schema["input_order"][family]
    exact_elements = sum(padded.inputs[name].size for name in order)
    assert exact_elements > 1

    class TypedEvaluator(ProductionEvaluator):
        def __init__(self):
            self.spec = contract
            self.calls = 0

        @property
        def architecture_family(self):
            return family

        def evaluate_typed(self, inputs):
            self.calls += 1
            return (np.zeros(inputs["candidate_mask"].shape, np.float32),
                    np.zeros((len(inputs["candidate_mask"]), 1), np.float32))

    evaluator = TypedEvaluator()
    exact = TypedInformationSetSearch(TypedEncoder(contract), evaluator,
        limits=SearchLimits(iterations=1, max_depth=1, elapsed_ms=None,
                            max_inference_elements=exact_elements))
    assert len(exact._evaluate_many([request], _SearchState())) == 1
    assert evaluator.calls == 1

    capped = TypedInformationSetSearch(TypedEncoder(contract), evaluator,
        limits=SearchLimits(iterations=1, max_depth=1, elapsed_ms=None,
                            max_inference_elements=exact_elements - 1))

    def padding_must_not_run(_):
        raise AssertionError("oversized typed batch must fail before padding")

    monkeypatch.setattr("accelerate_chess.ir.batch_typed_positions", padding_must_not_run)
    with pytest.raises(SearchBudgetError, match="inference element budget"):
        capped._evaluate_many([request], _SearchState())
    assert evaluator.calls == 1


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
    from accelerate_chess import GameAdapterClient
    _native_mode_flow(mode, draft_delete, TypedEncoder(typed_spec()), GameAdapterClient)


@pytest.mark.parametrize("mode", ["normal", "chaos"])
def test_native_default_weighted_conditioning_completion_gate(mode, record_property):
    """No skip: public fixture choices complete draft and both actors' play.

    Completion uses reviewed manual cards; source-prior stochastic rejection
    is a separate contract, not a promise that every trace fits 16 proposals.
    Both draft actors, both play actors, complete public projection/history,
    reconstruction, and the original finite work limits remain required.
    """
    from accelerate_chess import GameAdapterClient
    encoder = TypedEncoder(typed_spec())
    started = time.monotonic()
    try:
        _native_mode_flow(mode, False, encoder, GameAdapterClient)
    finally:
        # The CI subprocess has a finite wall watchdog. This property records
        # duration without turning machine load into a rule-correctness gate.
        record_property("wall_seconds", round(time.monotonic() - started, 3))


def _native_mode_flow(mode, draft_delete, encoder, GameAdapterClient):
    config = {"gameStyle": mode, "draftDelete": draft_delete}
    contract = encoder.spec
    actual = GameAdapterClient.new_game(config, 37, spec=contract)
    trackers = {viewer: PublicTracker(actual.observe(viewer), typed_spec=contract)
                for viewer in ("white", "black")}
    recorded = {viewer: [tracker.initial] for viewer, tracker in trackers.items()}
    limits = BeliefLimits(particles=2, proposals=16, actions_per_transition=4096, elapsed_ms=None)
    beliefs = {viewer: ParticleBelief(tracker, NativeSourceFactory(config, typed_spec=contract), seed=71 + index,
        limits=limits)
        for index, (viewer, tracker) in enumerate(trackers.items())}
    # At most twelve grand picks and two actual play actions. Candidates come
    # only from a sampled source world; the actual environment binds afterward.
    play_actors = []
    for _ in range(14):
        actor = actual.decision_actor
        posterior = beliefs[actor]
        posterior.synchronize()
        observation = trackers[actor].latest
        mode_before = observation["publicState"]["mode"]
        sampled = posterior.draw()
        requested_bundle = None
        if mode == "chaos" and mode_before == "draft" and actor == "white":
            # This is a public test input policy, not a game-rule predicate.
            # These source-pinned MIDDLE cards require manual activation and
            # preserve the completion flow without an automatic Otherworld
            # pawn draw on the first move. Real stochastic p/q and rejection
            # remain covered by native source-prior tests.
            choices = observation["publicState"]["draft"]["choices"]
            requested_cards = {"switcheroo", "royal-command"}
            candidates = [index // 2 for index in range(0, len(choices), 2)
                          if {card["id"] for card in choices[index:index + 2]} == requested_cards]
            assert len(candidates) == 1, (
                "chaos completion fixture requires one public switcheroo/royal-command bundle; "
                f"got {[card['id'] for card in choices]}"
            )
            requested_bundle = candidates[0]
            assert all(card.get("phase") == "MIDDLE" for card in choices[
                requested_bundle * 2:requested_bundle * 2 + 2
            ]), "reviewed completion cards must retain their source-pinned MIDDLE metadata"
        streamed = None
        for actions, _ in _stream(sampled, 1, 4096):
            if not actions:
                continue
            candidate = actions[0]
            public_intent = candidate.public_intent()
            if (requested_bundle is None or public_intent.get("type") == "draftBundlePick"
                    and public_intent.get("bundleIndex") == requested_bundle):
                streamed = candidate
                break
        assert streamed is not None, "source stream did not admit the requested public fixture intent"
        intent = streamed.public_intent()
        if mode_before == "draft" and trackers[actor].steps == 0:
            assert posterior.factory.bind_streamed_public_intent(sampled, streamed, intent) is streamed
            assert canonical_json(streamed.public_intent()) == canonical_json(
                sampled.bind_public_intent(intent).public_intent())
        feature = encoder.encode(ObservationIR.from_public(observation, contract,
                                 belief_summary=posterior.summary), [intent])
        assert feature.action_keys == (canonical_json(intent),)
        before_particles = [(particle, particle.snapshot_revision, particle.observe(viewer))
                            for viewer, belief in beliefs.items() for particle in belief._particles]
        stepped = actual.apply(actual.bind_public_intent(intent)).position
        for viewer, tracker in trackers.items():
            after = stepped.observe(viewer)
            assert after["history"][:-1] == recorded[viewer][-1]["history"]
            assert len(after["history"]) == tracker.steps + 1
            tracker.append(after, own_intent=intent if viewer == actor else None)
            recorded[viewer].append(after)
        actual = stepped
        for viewer, belief in beliefs.items():
            belief.synchronize()
            assert all(particle.observe(viewer) == trackers[viewer].latest for particle in belief._particles)
            assert belief.summary["trace_steps"] == trackers[viewer].steps
        for particle, revision, before in before_particles:
            assert particle.snapshot_revision == revision
            assert particle.observe(before["viewer"]) == before
        if mode_before == "play":
            play_actors.append(actor)
            if len(play_actors) == 2:
                assert play_actors == ["white", "black"]
                assert trackers[actor].steps == (2 if draft_delete else 14 if mode == "grand" else 4)
                for index, (viewer, tracker) in enumerate(trackers.items()):
                    restored = PublicTracker.from_snapshot(tracker.snapshot(), typed_spec=contract)
                    assert restored.initial == recorded[viewer][0]
                    assert restored.latest == recorded[viewer][-1]
                    for (_, frame), source_frame in zip(restored.frames(), recorded[viewer][1:], strict=True):
                        assert frame == source_frame
                    reconstructed = ParticleBelief(restored, NativeSourceFactory(config, typed_spec=contract),
                                                   seed=191 + index, limits=limits)
                    assert all(particle.observe(viewer) == recorded[viewer][-1]
                               for particle in reconstructed._particles)
                    assert reconstructed.summary["trace_steps"] == len(recorded[viewer]) - 1
                    assert "source-prior-v1" in reconstructed.summary["proposal_profiles"]
                    assert "source-prior-v1" in beliefs[viewer].summary["proposal_profiles"]
                return
    pytest.fail("finite native flow did not reconstruct draft and both actors' play transitions")
