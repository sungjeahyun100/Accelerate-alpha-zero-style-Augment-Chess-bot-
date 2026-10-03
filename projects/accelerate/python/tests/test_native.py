"""Permanent FFI contracts: ownership, versioned transport and explicit errors."""
from concurrent.futures import ThreadPoolExecutor
from copy import deepcopy
import hashlib
import json

import jcs
import numpy as np
import pytest

from accelerate_chess import (GameAdapterClient, GameAdapterSession, Position, NativeError,
                              StaleActionError, UnsupportedFeatureError,
                              ConditioningMismatchError, RULES_VERSION_V7, site_catalog,
                              site_observation_policy)
from accelerate_chess.ir import TypedEncoderSpec


def _digest(value):
    return hashlib.sha256(jcs.canonicalize(value)).hexdigest()


def test_frozen_catalog_and_observation_policy_are_versioned_owned_copies():
    v6 = site_catalog()
    v7_version = "augment-site-20260928-e5ed84fcf8e72a24"
    v7 = site_catalog(v7_version)
    policy = site_observation_policy(v7_version)
    assert v6["rulesVersion"] != v7_version
    assert v7["rulesVersion"] == policy["rulesVersion"] == v7_version
    assert next(file for file in v7["source"]["files"]
                if file["name"] == "main-OahWs0tU.js")["sha256"] == (
        "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
    )
    v7["cards"].clear()
    policy["rulesVersion"] = "changed"
    assert len(site_catalog(v7_version)["cards"]) == 256
    assert site_observation_policy(v7_version)["rulesVersion"] == v7_version
    with pytest.raises(ValueError, match="unknown rules version"):
        site_catalog("unknown")


def _state():
    # A small explicit v7 host state tests field presence and game behavior.
    # Frozen v7 endMove always updates the source-owned palaces array, even
    # when no palace effect is active; absence is not an executable default.
    board = [[None for _ in range(8)] for _ in range(8)]
    for row, col, kind, color, identity in (
        (7, 7, "king", "white", "white-king"),
        (0, 7, "king", "black", "black-king"),
        (6, 0, "rook", "white", "white-rook"),
        (1, 0, "rook", "black", "black-rook"),
    ):
        board[row][col] = {"type": kind, "color": color, "id": identity}
    return {"board": board, "turn": "white", "mode": "play",
            "rulesetId": RULES_VERSION_V7, "actionsRemaining": 1,
            "middleDraftDone": True, "endDraftDone": False,
            "turnsTaken": {"white": 0, "black": 0}, "moveCount": 0,
            "castlingCanceled": {"white": False, "black": False},
            "palaces": [],
            "deckSlots": {"white": [None], "black": []},
            "marker": {"nested": [1, True, None]}}


def _legacy_state():
    state = _state()
    del state["rulesetId"]
    return state


def _envelope(state):
    content = {"protocolVersion": "accelerate-position-v1",
               "rulesVersion": RULES_VERSION_V7,
               "catalogVersion": site_catalog(RULES_VERSION_V7)["catalogVersion"],
               "state": deepcopy(state),
               "rng": {"algorithm": "lcg32-v1", "state": 0, "cursor": 0, "tape": []},
               "history": []}
    return {**content, "positionId": _digest(content)}


def _client(envelope):
    spec = TypedEncoderSpec.from_catalog(site_catalog(RULES_VERSION_V7),
                observation_policy=site_observation_policy(RULES_VERSION_V7))
    return GameAdapterClient(GameAdapterSession.from_envelope(envelope), spec)


def _request(session, adapter_id, capability_id, payload):
    descriptor = next(item for item in session.descriptors()
                      if item["adapterId"] == adapter_id)
    capability = next(item for item in descriptor["capabilities"]
                      if item["id"] == capability_id)
    return {"requestId": "ffi-contract", "projectId": descriptor["projectId"],
            "adapterId": adapter_id, "contractVersion": descriptor["contractVersion"],
            "implementationVersion": descriptor["implementationVersion"],
            "capabilityId": capability_id,
            "requestSchema": capability["requestSchema"],
            "responseSchema": capability["responseSchema"],
            "snapshotRevision": session.snapshot_revision,
            "limits": descriptor["callLimits"], "payload": payload}


def test_v6_public_execution_is_closed_with_explicit_version_error():
    legacy_catalog = site_catalog()
    snapshot = {"protocolVersion": "accelerate-position-v1",
                "rulesVersion": legacy_catalog["rulesVersion"],
                "catalogVersion": legacy_catalog["catalogVersion"],
                "state": _legacy_state(),
                "rng": {"algorithm": "lcg32-v1", "state": 0, "cursor": 0, "tape": []},
                "history": []}
    snapshot["positionId"] = _digest(snapshot)
    for import_legacy in (lambda: Position.new_game(rules_version=legacy_catalog["rulesVersion"]),
                          lambda: Position.from_state(_legacy_state()),
                          lambda: Position.from_snapshot(snapshot)):
        with pytest.raises(UnsupportedFeatureError, match="v6 rule execution is retired"):
            import_legacy()
    # Position remains a retired transport surface; only the explicit v7 host
    # may execute. A v7 rules label must not revive the legacy implementation.
    with pytest.raises(UnsupportedFeatureError, match="explicit v7 rules version"):
        Position.new_game()
    with pytest.raises(UnsupportedFeatureError, match="requires the Augment Chess adapter host"):
        Position.new_game(rules_version=RULES_VERSION_V7)
    with pytest.raises(UnsupportedFeatureError, match="v7 rules profile is not executable"):
        Position.from_state(_state())
    with pytest.raises(UnsupportedFeatureError, match="v7 rules profile is not executable"):
        Position.from_snapshot(_envelope(_state()))


def _move(position, from_square, to_square):
    intent = next(a for a in position.legal_intents()
                  if a.get("from") == {"row": from_square[0], "col": from_square[1]}
                  and a.get("destination", {}).get("row") == to_square[0]
                  and a.get("destination", {}).get("col") == to_square[1])
    return position.bind_public_intent(intent)


def test_direct_calls_match_versioned_json_and_preserve_source_presence():
    raw = _state()
    snapshot = _envelope(raw)
    position = _client(snapshot)
    content = {k: v for k, v in snapshot.items() if k != "positionId"}
    # Host admission verifies the digest of the complete source shape, including
    # absent fields and vacant slots, without exporting its private envelope.
    assert position.snapshot_revision == _digest(content)
    restored = _client(json.loads(json.dumps(snapshot)))
    assert restored.snapshot_revision == position.snapshot_revision
    assert restored.observe("white") == position.observe("white")
    explicit_default = deepcopy(snapshot)
    explicit_default["state"]["fullMove"] = 1
    explicit_default["positionId"] = _digest({k: v for k, v in explicit_default.items()
                                             if k != "positionId"})
    assert _client(explicit_default).snapshot_revision != position.snapshot_revision
    numeric = deepcopy(snapshot)
    numeric["rng"]["tape"] = [0.0, 0.5]
    numeric["positionId"] = _digest({k: v for k, v in numeric.items() if k != "positionId"})
    canonical_snapshot = json.loads(jcs.canonicalize(numeric))
    canonical_restored = _client(canonical_snapshot)
    assert canonical_restored.snapshot_revision == numeric["positionId"]
    assert canonical_restored.observe("white") == position.observe("white")
    request = _request(position._session, "public-observation", "observe",
                       {"kind": "observe", "viewer": "white"})
    direct = position._session.invoke(request)
    assert direct["requestId"] == request["requestId"]
    assert direct["snapshotRevision"] == position.snapshot_revision
    assert direct["result"]["observation"] == position.observe("white")
    # Returned mappings and the caller's source state cannot rewrite the host.
    direct["result"]["observation"]["board"][6][0]["type"] = "queen"
    snapshot["state"]["board"][6][0]["type"] = "bishop"
    assert position.observe("white")["board"][6][0]["type"] == "rook"
    action = _move(position, (6, 0), (5, 0))
    intent = action.public_intent()
    assert not {"positionKey", "positionId", "actionId"} & intent.keys()
    assert action.revision == position.snapshot_revision
    assert position.bind_public_intent(intent).public_intent() == intent
    stream = position.action_stream()
    paged = []
    while True:
        page = stream.next_page(3)
        assert set(page) == {"actions", "exhausted", "examined"}
        assert type(page["examined"]) is int
        assert len(page["actions"]) <= page["examined"] <= 3
        assert page["examined"] > 0 or page["exhausted"]
        paged.extend(action.public_intent() for action in page["actions"])
        if page["exhausted"]:
            break
    assert paged == list(position.legal_intents())
    assert stream.next_page(3) == {"actions": (), "exhausted": True, "examined": 0}
    with pytest.raises(ValueError, match="page limit"):
        stream.next_page(4097)
    del stream
    step = position.apply(action)
    other = restored.apply(restored.bind_public_intent(intent))
    assert step.position.snapshot_revision == other.position.snapshot_revision
    assert step.position.observe("white") == other.position.observe("white")
    assert step.actor == "white" and step.turn_changed
    assert step.position.decision_actor == "black" and step.result is None
    observed = step.position.observe("white")
    assert observed["ownCards"] == []
    assert observed["publicState"]["captures"] == {"white": [], "black": []}
    assert "marker" not in observed["publicState"]
    assert len(observed["history"]) == 1 and observed["history"][0]["actor"] == "white"
    assert "action" not in observed["history"][0]
    # Private marker/slot presence remains part of the committed identity even
    # though the two resulting public views cannot distinguish it.
    absent = deepcopy(raw)
    del absent["marker"]
    absent["deckSlots"]["white"] = []
    alternate = _client(_envelope(absent))
    alternate_step = alternate.apply(alternate.bind_public_intent(intent))
    assert alternate_step.position.observe("white") == observed
    assert alternate_step.position.snapshot_revision != step.position.snapshot_revision


def test_lifetime_branching_and_stale_actions_are_immutable():
    position = _client(_envelope(_state()))
    before = position.snapshot_revision, position.observe("white")
    action = _move(position, (6, 0), (5, 0))
    branch = position.apply(action).position
    assert (position.snapshot_revision, position.observe("white")) == before
    with pytest.raises(ValueError, match="another game adapter revision"):
        branch.apply(action)
    expected_public = branch.observe("white")
    with pytest.raises(ValueError, match="another game adapter revision"):
        branch.public_transition_compatible(action, expected_public)
    with pytest.raises(ValueError, match="another game adapter revision"):
        branch.apply_weighted_conditioned_public(action, expected_public, 71)
    with pytest.raises(StaleActionError):
        branch._session.public_transition_compatible(action.public_intent(), expected_public,
                                                      snapshot_revision=action.revision)
    assert position.public_transition_compatible(action, expected_public)
    proposal = position.apply_weighted_conditioned_public(action, expected_public, 71)
    assert set(proposal) == {"position", "importance_weight", "source_probability", "proposal_probability"}
    assert proposal["importance_weight"] == proposal["source_probability"] / proposal["proposal_probability"] == 1.
    proposed = proposal["position"]
    assert proposed.observe("white") == expected_public
    # Returned control mappings and caller-owned observations cannot mutate
    # either immutable native branch, even when the Python owners are released.
    proposed_before = proposed.snapshot_revision, proposed.observe("white")
    proposal["source_probability"] = 0.
    expected_public["board"][5][0]["type"] = "queen"
    assert (proposed.snapshot_revision, proposed.observe("white")) == proposed_before
    assert branch.observe("white")["board"][5][0]["type"] == "rook"
    del proposal
    assert (proposed.snapshot_revision, proposed.observe("white")) == proposed_before
    expected_public = branch.observe("white")
    with pytest.raises(OverflowError):
        position._session.apply_weighted_conditioned_public(action.public_intent(), expected_public, 2**32)
    with pytest.raises(OverflowError):
        position._session.condition_hidden_opening_draft(expected_public, 2**32)
    with pytest.raises(OverflowError):
        position._session.apply_weighted_conditioned_public(action.public_intent(), expected_public, 2**32,
                                                             snapshot_revision=action.revision)
    # Public-frame conversion must fail before detached native work starts.
    with pytest.raises(TypeError):
        position._session.public_transition_compatible(action.public_intent(), {object(): 1})
    with pytest.raises(TypeError):
        position._session.condition_hidden_opening_draft({"board": object()}, 71)
    assert (position.snapshot_revision, position.observe("white")) == before
    payload = action.public_intent()
    payload["destination"]["magicCapture"] = True
    with pytest.raises(ValueError, match="destination needs exact integer row and col coordinates"):
        position.bind_public_intent(payload)
    assert (position.snapshot_revision, position.observe("white")) == before
    # Bypassing the Python facade must still reject private execution flags at
    # the native host, preserving its precise error and the committed state.
    request = _request(position._session, "public-actions", "bind-public-intent",
                       {"kind": "bind_public_intent", "intent": payload})
    with pytest.raises(NativeError, match="illegal_game_action"):
        position._session.invoke(request)
    assert action.public_intent() != payload
    assert (position.snapshot_revision, position.observe("white")) == before
    assert position.apply(action).position.snapshot_revision == branch.snapshot_revision
    stream = position.action_stream()
    expected_ids = sorted(_digest(intent) for intent in position.legal_intents())
    del position
    # Action owns its payload and derived branches own their snapshot, with no
    # Python parent reference or mutable alias needed to keep either alive.
    assert branch.legal_intents() and action.public_intent()["from"]["row"] == 6
    def consume(_):
        actions = []
        while True:
            page = stream.next_page(2)
            actions.extend(page["actions"])
            if page["exhausted"]:
                return actions
    with ThreadPoolExecutor(max_workers=4) as pool:
        pages = list(pool.map(consume, range(4)))
    del stream
    assert sorted(_digest(a.public_intent()) for page in pages for a in page) == expected_ids
    assert all(a.public_intent()["color"] == "white" for page in pages for a in page)
    assert action.public_intent()["from"] == {"row": 6, "col": 0}
    with pytest.raises(AttributeError):
        action.revision = "rewritten"


def test_transport_rejects_corruption_versions_and_duplicate_private_metadata():
    snapshot = _envelope(_state())
    position = _client(snapshot)
    catalog = site_catalog(RULES_VERSION_V7)
    assert catalog["rulesVersion"] == snapshot["rulesVersion"]
    catalog["rulesVersion"] = "caller mutation"
    assert site_catalog(RULES_VERSION_V7)["rulesVersion"] != catalog["rulesVersion"]
    action = _move(position, (6, 0), (5, 0))
    branch = position.apply(action).position
    observation = branch.observe("white")
    assert position.apply_weighted_conditioned_public(action, observation, 71)["position"].observe("white") == observation
    observation["board"][5][0]["type"] = "bishop"
    observation["informationStateKey"] = _digest(
        {k: v for k, v in observation.items() if k != "informationStateKey"})
    with pytest.raises(ConditioningMismatchError):
        position._session.apply_weighted_conditioned_public(action.public_intent(), observation, 71)
    observation["informationStateKey"] = "invalid"
    with pytest.raises(NativeError) as malformed:
        position._session.apply_weighted_conditioned_public(action.public_intent(), observation, 71)
    assert not isinstance(malformed.value, ConditioningMismatchError)
    for field, value in (("positionId", "bad"), ("protocolVersion", "old"),
                         ("rulesVersion", "old"), ("catalogVersion", "old")):
        bad = deepcopy(snapshot)
        bad[field] = value
        with pytest.raises(ValueError):
            GameAdapterSession.from_envelope(bad)
    bad = deepcopy(snapshot)
    bad["state"]["rng"] = bad["rng"]
    bad["positionId"] = _digest({k: v for k, v in bad.items() if k != "positionId"})
    with pytest.raises(ValueError, match="duplicate"):
        GameAdapterSession.from_envelope(bad)
    with pytest.raises(ValueError):
        Position.from_json("{bad json")
    with pytest.raises(ValueError):
        GameAdapterSession.new_game(seed=2**32, rules_version=RULES_VERSION_V7)
    bad = deepcopy(snapshot)
    bad["history"] = [{"actor": "white", "unexpectedPrivate": "invalid"}]
    bad["positionId"] = _digest({k: v for k, v in bad.items() if k != "positionId"})
    with pytest.raises(ValueError, match="history"):
        GameAdapterSession.from_envelope(bad)
    request = _request(position._session, "public-actions", "bind-public-intent",
                       {"kind": "bind_public_intent", "intent": action.public_intent()})
    request["requestSchema"]["sha256"] = "0" * 64
    with pytest.raises(NativeError, match="schema"):
        position._session.invoke(request)
    assert position.snapshot_revision == snapshot["positionId"]


def test_private_state_is_absent_from_public_observation_and_numpy_copy():
    raw = _state()
    raw["board"][1][0]["hiddenFrom"] = "white"
    raw["futureDraftOffers"] = ["private-offer"]
    position = _client(_envelope(raw))
    observation = position.observe("white")
    assert set(observation) == {"protocolVersion", "viewer", "board", "turn", "ownCards",
                                "opponentHandCount", "publicState", "history", "informationStateKey"}
    assert observation["board"][1][0] is None
    assert observation["informationStateKey"] == _digest({k: v for k, v in observation.items() if k != "informationStateKey"})
    serialized = json.dumps(observation)
    assert position.snapshot_revision not in serialized
    assert "lcg32-v1" not in serialized and "private-offer" not in serialized
    # The current host exposes only public frames. Consumers may create owned
    # NumPy copies; the legacy Position.board API is no longer executable.
    public_board = np.array(observation["board"], dtype=object)
    assert public_board.shape == (8, 8) and public_board.dtype == object
    assert public_board.tolist() == observation["board"]
    public_board[6, 0]["type"] = "queen"
    assert position.observe("white")["board"][6][0]["type"] == "rook"
    with pytest.raises(ValueError):
        position.observe("spectator")


def test_numpy_input_is_owned_strided_finite_and_bounded():
    raw = _state()
    values = np.arange(12, dtype=np.float64).reshape(3, 4)[:, ::2]
    raw["arrayMarker"] = values.tolist()
    expected = _envelope(raw)
    transported = deepcopy(expected)
    transported["state"]["arrayMarker"] = values
    position = _client(transported)
    assert position.snapshot_revision == expected["positionId"]
    before = position.observe("white")
    values.fill(-1)
    assert position.snapshot_revision == expected["positionId"]
    assert position.observe("white") == before
    changed = deepcopy(expected)
    changed["state"]["arrayMarker"] = values.tolist()
    changed["positionId"] = _digest({k: v for k, v in changed.items() if k != "positionId"})
    assert _client(changed).snapshot_revision != position.snapshot_revision
    for invalid in (np.array([np.nan]), np.array([np.inf]), np.array([2**53], dtype=np.uint64),
                    np.array([1e21], dtype=np.float64), np.array([-1e21], dtype=np.float32),
                    np.array([object()], dtype=object)):
        transported["state"]["arrayMarker"] = invalid
        with pytest.raises((TypeError, ValueError)):
            GameAdapterSession.from_envelope(transported)
    transported["state"]["arrayMarker"] = np.empty((100_001, 0), dtype=np.float64)
    with pytest.raises(ValueError, match="limits"):
        GameAdapterSession.from_envelope(transported)
    transported["state"]["arrayMarker"] = 2**53
    with pytest.raises(ValueError, match="exact"):
        GameAdapterSession.from_envelope(transported)
    transported["state"]["arrayMarker"] = 1e21
    with pytest.raises(ValueError, match="exact"):
        GameAdapterSession.from_envelope(transported)
    transported["state"]["arrayMarker"] = [float("nan")]
    with pytest.raises(ValueError, match="finite"):
        GameAdapterSession.from_envelope(transported)
    transported["state"]["arrayMarker"] = [[]]
    nested = transported["state"]["arrayMarker"]
    for _ in range(70):
        nested.append([])
        nested = nested[-1]
    with pytest.raises(ValueError, match="nesting"):
        GameAdapterSession.from_envelope(transported)


def test_multicell_entity_identity_and_native_threads_share_no_mutable_state():
    raw = _state()
    raw["board"][6][0] = None
    piece = {"type": "bigRook", "color": "white", "id": "large",
             "anchorRow": 4, "anchorCol": 3}
    for row in (4, 5):
        for col in (3, 4):
            raw["board"][row][col] = deepcopy(piece)
    snapshot = _envelope(raw)
    position = _client(snapshot)
    before = position.snapshot_revision, position.observe("white")
    action = _move(position, (4, 3), (3, 3))
    def branch(_):
        step = position.apply(action)
        public = step.position.observe("white")
        footprint = [(row, col) for row, cells in enumerate(public["board"])
                     for col, cell in enumerate(cells)
                     if cell is not None and cell["type"] == "bigRook"]
        assert footprint == [(3, 3), (3, 4), (4, 3), (4, 4)]
        return step.position.snapshot_revision, public
    with ThreadPoolExecutor(max_workers=4) as pool:
        branches = list(pool.map(branch, range(32)))
    assert all(branch == branches[0] for branch in branches)
    assert (position.snapshot_revision, position.observe("white")) == before
    del action, position
    assert branches[0][1]["board"][3][3]["type"] == "bigRook"
    assert branches[0][1]["board"][3][3]["anchorRow"] == 3
    assert branches[0][1]["board"][3][3]["anchorCol"] == 3
