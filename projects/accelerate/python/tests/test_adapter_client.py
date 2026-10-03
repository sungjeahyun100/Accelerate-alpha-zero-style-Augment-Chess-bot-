"""Installed v7 host boundary against the frozen public catalog and policy."""

import math
from copy import deepcopy
from threading import RLock
from types import SimpleNamespace

import pytest

from accelerate_chess.encoding import PUBLIC_MOVE_SELECTION_MODES, canonical_json
from accelerate_chess.ir import ObservationIR, TypedEncoderSpec
from test_ir import signed, source


def pinned_spec():
    catalog, policy = source()
    return TypedEncoderSpec.from_catalog(catalog, observation_policy=policy)


@pytest.mark.parametrize("private_name", ["actionId", "fullPosition", "informationStateKey", "windowId"])
def test_public_action_boundary_rejects_nested_execution_and_identity_fields(private_name):
    from accelerate_chess import GameAdapterClient

    client = object.__new__(GameAdapterClient)
    leaked = {"type": "card", "color": "white", "choice": {private_name: "private"}}
    client._call = lambda *args: {"kind": "legal_actions", "intents": [leaked]}
    with pytest.raises(ValueError, match="private field"):
        client.legal_intents()
    with pytest.raises(ValueError, match="private field"):
        client.bind_public_intent(leaked)


@pytest.mark.parametrize("mode", ["move", "reload", "shotgunBlast", None, True, []])
def test_invalid_public_move_selection_fails_before_transport_and_in_native_results(mode):
    from accelerate_chess import GameAdapterClient

    client = object.__new__(GameAdapterClient)
    intent = {"type": "move", "color": "white", "from": {"row": 6, "col": 1},
              "destination": {"row": 5, "col": 1}, "selectionMode": mode}
    # No session/lock exists: rejection must precede transport initialization.
    with pytest.raises(ValueError, match="selectionMode"):
        client.bind_public_intent(intent)
    with pytest.raises(ValueError, match="selectionMode"):
        client._apply_exact_intent(intent)
    client._call = lambda *args: {"kind": "legal_actions", "intents": [intent]}
    with pytest.raises(ValueError, match="selectionMode"):
        client.legal_intents()


def test_public_move_selectors_are_preserved_exactly_across_facade_binding():
    from accelerate_chess import GameAdapterClient

    client = object.__new__(GameAdapterClient)
    client._session = SimpleNamespace(snapshot_revision="source-revision")
    client._lock = RLock()
    ordinary = {"type": "move", "color": "white", "from": {"row": 6, "col": 1},
                "destination": {"row": 5, "col": 1}}
    candidates = [ordinary, *[{**ordinary, "selectionMode": mode}
                              for mode in PUBLIC_MOVE_SELECTION_MODES]]
    sent = []

    def bound_call(adapter_id, capability_id, payload, *, expected_revision):
        assert adapter_id == "public-actions" and capability_id == "bind-public-intent"
        assert expected_revision == "source-revision"
        sent.append(payload["intent"])
        return {"kind": "bound_public_intent", "intent": payload["intent"]}

    client._call = bound_call
    for intent in candidates:
        assert client.bind_public_intent(intent).public_intent() == intent
    assert sent == candidates
    assert len({canonical_json(intent) for intent in sent}) == 4
    assert "selectionMode" not in sent[0]
    assert GameAdapterClient._validated_intents(candidates) == tuple(candidates)


def test_v7_game_seed_rejects_non_uint32_before_native_setup():
    from accelerate_chess import GameAdapterClient

    for seed in (True, -1, 2**32, "19"):
        with pytest.raises(ValueError, match="uint32"):
            GameAdapterClient.new_game({"gameStyle": "normal"}, seed, spec=pinned_spec())


def test_native_revision_change_cannot_relabel_a_bound_public_intent():
    from accelerate_chess import GameAdapterClient

    client = object.__new__(GameAdapterClient)
    client._session = SimpleNamespace(snapshot_revision="source-revision-before")
    client._lock = RLock()
    intent = {"type": "draftPick", "color": "white", "cardInstanceId": "public-offer-1"}

    def raced_call(adapter_id, capability_id, payload, *, expected_revision):
        assert expected_revision == "source-revision-before"
        client._session.snapshot_revision = "source-revision-after"
        return {"kind": "bound_public_intent", "intent": payload["intent"]}

    client._call = raced_call
    with pytest.raises(ValueError, match="revision changed while binding"):
        client.bind_public_intent(intent)


def test_forked_revision_mismatch_cannot_apply_an_old_public_action():
    from accelerate_chess import GameAdapterClient
    from accelerate_chess.adapter_client import _action

    client = object.__new__(GameAdapterClient)
    client._session = SimpleNamespace(snapshot_revision="source-revision-before")
    client._lock = RLock()
    client.fork = lambda: SimpleNamespace(snapshot_revision="source-revision-after")
    action = _action({"type": "draftPick", "color": "white", "cardInstanceId": "public-offer-1"},
                     "source-revision-before")
    with pytest.raises(ValueError, match="revision changed while forking"):
        client.apply(action)


def test_matching_rules_name_does_not_allow_a_different_native_policy():
    from accelerate_chess import GameAdapterClient

    catalog, source_policy = source()
    policy = deepcopy(source_policy)
    policy["unverifiedSourceExtension"] = True
    wrong_spec = TypedEncoderSpec.from_catalog(catalog, observation_policy=policy)
    with pytest.raises(ValueError, match="installed native source contract"):
        GameAdapterClient.new_game({"gameStyle": "normal"}, 37, spec=wrong_spec)


def test_native_snapshot_calls_keep_the_callers_expected_revision():
    from accelerate_chess import GameAdapterClient, StaleActionError

    client = GameAdapterClient.new_game({"gameStyle": "normal"}, 37, spec=pinned_spec())
    public = client.observe("white")
    intent = client.legal_intents()[0]
    revision = client.snapshot_revision
    stale = f"{revision}-expired"
    session = client._session
    calls = (
        lambda: session.fork(snapshot_revision=stale),
        lambda: session.condition_hidden_opening_draft(public, 71, snapshot_revision=stale),
        lambda: session.public_transition_compatible(intent, public, snapshot_revision=stale),
        lambda: session.apply_weighted_conditioned_public(intent, public, 71, snapshot_revision=stale),
    )
    for call in calls:
        with pytest.raises(StaleActionError) as rejected:
            call()
        assert stale in str(rejected.value)
        assert revision in str(rejected.value)
        assert client.snapshot_revision == revision


@pytest.mark.parametrize("style", ["normal", "chaos", "grand"])
def test_v7_draft_public_intents_are_exact_and_branches_are_isolated(style):
    from accelerate_chess import GameAdapterClient

    spec = pinned_spec()
    environment = GameAdapterClient.new_game(
        {"gameStyle": style, "draftDelete": False}, 37, spec=spec)
    initial = environment.observe("white")
    ObservationIR.from_public(initial, spec)
    # Frozen grand draft starts with Black while the board turn is White.
    # Public events must identify the source decision actor, not the viewer.
    actor = "black" if style == "grand" else "white"
    assert environment.decision_actor == actor
    revision = environment.snapshot_revision
    eager = environment.legal_intents()
    assert eager and all(intent["color"] == actor for intent in eager)
    cursor, paged = None, []
    for _ in range(len(eager) + 2):
        page = environment.legal_intents_page(limit=1, max_examined=1, cursor=cursor)
        paged.extend(page["intents"])
        cursor = page["cursor"]
        if page["exhausted"]:
            break
    assert page["exhausted"]
    assert tuple(paged) == eager
    assert environment.snapshot_revision == revision
    stream = environment.action_stream()
    first = stream.next_page(1)
    assert first["examined"] == 1 and len(first["actions"]) == 1
    intent = first["actions"][0].public_intent()
    assert "positionId" not in intent and "actionId" not in intent
    assert environment.bind_public_intent(intent).public_intent() == intent

    with pytest.raises(Exception):
        environment.bind_public_intent({**intent, "clientNote": "unverified"})
    assert environment.snapshot_revision == revision

    step = environment.apply(first["actions"][0])
    assert environment.snapshot_revision == revision
    assert step.position.snapshot_revision != revision
    assert step.actor == actor
    assert not hasattr(step, "event")
    assert step.position.observe("white") != initial
    assert step.position.observe("white")["history"][-1]["actor"] == actor
    ObservationIR.from_public(step.position.observe("white"), spec)
    with pytest.raises(ValueError, match="another game adapter revision"):
        step.position.apply(first["actions"][0])


def test_v7_public_initial_particle_and_weighted_step_keep_density_explicit():
    from accelerate_chess import GameAdapterClient

    spec = pinned_spec()
    config = {"gameStyle": "normal", "draftDelete": False}
    environment = GameAdapterClient.new_game(config, 37, spec=spec)
    public = environment.observe("white")
    particle = GameAdapterClient.sample_initial_public(config, public, 71, spec=spec)
    assert particle.observe("white") == public
    assert particle.snapshot_revision != environment.snapshot_revision

    intent = environment.legal_intents()[0]
    action = particle.bind_public_intent(intent)
    actual = environment.apply(environment.bind_public_intent(intent)).position
    expected = actual.observe("white")
    assert particle.public_transition_compatible(action, expected)
    proposal = particle.apply_weighted_conditioned_public(action, expected, 93)
    assert proposal["position"].observe("white") == expected
    assert proposal["position"].snapshot_revision != particle.snapshot_revision
    assert math.isclose(proposal["importance_weight"],
                        proposal["source_probability"] / proposal["proposal_probability"],
                        rel_tol=1e-10, abs_tol=0)
    assert particle.snapshot_revision == action.revision


@pytest.mark.parametrize("style", ["normal", "chaos", "grand"])
def test_v7_play_public_particles_use_one_source_attempt_and_preserve_independent_branches(style):
    from accelerate_chess import ConditioningMismatchError, GameAdapterClient, StaleActionError
    from accelerate_chess.search import NativeSourceFactory

    spec = pinned_spec()
    config = {"gameStyle": style, "draftDelete": True}
    environment = GameAdapterClient.new_game(config, 37, spec=spec)
    intent = environment.legal_intents()[0]
    actual_first = environment.apply(environment.bind_public_intent(intent)).position
    second_intent = actual_first.legal_intents()[0]
    actual_second = actual_first.apply(actual_first.bind_public_intent(second_intent)).position
    factory = NativeSourceFactory(config, typed_spec=spec)
    for viewer in ("white", "black"):
        initial = environment.observe(viewer)
        expected = actual_first.observe(viewer)
        expected_second = actual_second.observe(viewer)
        # Only public frames/configuration cross into the source factory. The
        # environment's Position and seed/RNG never become proposal inputs.
        particle = factory.sample_initial(initial, 71)
        sibling = factory.sample_initial(initial, 72)
        assert particle is not None and sibling is not None
        assert particle.observe(viewer) == sibling.observe(viewer) == initial
        assert len({environment.snapshot_revision, particle.snapshot_revision, sibling.snapshot_revision}) == 3
        original_revision, sibling_revision = particle.snapshot_revision, sibling.snapshot_revision
        action = particle.bind_public_intent(intent)
        assert factory.transition_compatible(particle, action, expected)
        proposal = factory.apply_conditioned(particle, action, expected, 93)
        repeated = factory.apply_conditioned(particle, action, expected, 93)
        independent = factory.apply_conditioned(particle, action, expected, 94)
        assert proposal is not None and repeated is not None and independent is not None
        for step in (proposal, repeated, independent):
            assert step.profile == "source-prior-v1"
            assert step.source_probability == step.proposal_probability
            assert step.importance_weight == 1.
            assert step.position.observe(viewer) == expected
            assert step.position.observe(viewer)["history"][:-1] == initial["history"]
        assert proposal.position.snapshot_revision == repeated.position.snapshot_revision
        assert proposal.position.snapshot_revision != independent.position.snapshot_revision

        malformed = {**expected, "informationStateKey": "0" * 64}
        with pytest.raises(ValueError, match="identity"):
            factory.apply_conditioned(particle, action, malformed, 93)

        # A valid signature cannot authorize a different public result or the
        # rewriting of an earlier event in the complete observed history.
        mismatch = deepcopy(expected)
        mismatch["history"][-1]["actor"] = "black"
        signed(mismatch)
        ObservationIR.from_public(mismatch, spec)
        with pytest.raises(ConditioningMismatchError):
            particle.apply_weighted_conditioned_public(action, mismatch, 93)
        assert factory.apply_conditioned(particle, action, mismatch, 93) is None

        next_action = proposal.position.bind_public_intent(second_intent)
        rewritten = deepcopy(expected_second)
        rewritten["history"][0]["actor"] = "black"
        signed(rewritten)
        ObservationIR.from_public(rewritten, spec)
        with pytest.raises(ConditioningMismatchError):
            proposal.position.apply_weighted_conditioned_public(next_action, rewritten, 95)
        assert factory.apply_conditioned(proposal.position, next_action, rewritten, 95) is None

        stale = f"{original_revision}-expired"
        with pytest.raises(StaleActionError) as rejected:
            particle._session.apply_weighted_conditioned_public(
                intent, expected, 93, snapshot_revision=stale)
        assert stale in str(rejected.value) and original_revision in str(rejected.value)
        with pytest.raises(ValueError, match="another game adapter revision"):
            proposal.position.apply_weighted_conditioned_public(action, expected, 93)
        assert particle.snapshot_revision == original_revision
        assert sibling.snapshot_revision == sibling_revision
        assert particle.observe(viewer) == sibling.observe(viewer) == initial
        assert proposal.position.observe(viewer) == independent.position.observe(viewer) == expected


def test_v7_particle_seed_and_public_frame_validation_precede_native_sampling():
    from accelerate_chess import GameAdapterClient

    spec = pinned_spec()
    config = {"gameStyle": "normal", "draftDelete": False}
    environment = GameAdapterClient.new_game(config, 37, spec=spec)
    public = environment.observe("white")
    with pytest.raises(ValueError, match="uint32"):
        GameAdapterClient.sample_initial_public(config, public, 2**32, spec=spec)
    with pytest.raises(ValueError, match="private|unknown|public"):
        GameAdapterClient.sample_initial_public(config, {**public, "positionId": "private"}, 71,
                                                spec=spec)
