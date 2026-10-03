"""Permanent FFI contracts: ownership, versioned transport and explicit errors."""
from concurrent.futures import ThreadPoolExecutor
from copy import deepcopy
import hashlib
import json

import jcs
import numpy as np
import pytest

from accelerate_chess import (Position, NativeError, StaleActionError, UnsupportedFeatureError,
                              GameAdapterClient, GameAdapterSession, site_catalog,
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
    # A small source-shaped state tests field presence as well as game behavior.
    board = [[None for _ in range(8)] for _ in range(8)]
    for row, col, kind, color, identity in (
        (7, 7, "king", "white", "white-king"),
        (0, 7, "king", "black", "black-king"),
        (6, 0, "rook", "white", "white-rook"),
        (1, 0, "rook", "black", "black-rook"),
    ):
        board[row][col] = {"type": kind, "color": color, "id": identity}
    return {"board": board, "turn": "white", "actionsRemaining": 1,
            "deckSlots": {"white": [None], "black": []},
            "marker": {"nested": [1, True, None]}}


def test_v6_public_execution_is_closed_with_explicit_version_error():
    legacy_catalog = site_catalog()
    snapshot = {"protocolVersion": "accelerate-position-v1",
                "rulesVersion": legacy_catalog["rulesVersion"],
                "catalogVersion": legacy_catalog["catalogVersion"],
                "state": _state(),
                "rng": {"algorithm": "lcg32-v1", "state": 0, "cursor": 0, "tape": []},
                "history": []}
    snapshot["positionId"] = _digest(snapshot)
    for import_legacy in (lambda: Position.new_game(rules_version=legacy_catalog["rulesVersion"]),
                          lambda: Position.from_state(_state()),
                          lambda: Position.from_snapshot(snapshot)):
        with pytest.raises(UnsupportedFeatureError, match="v6 rule execution is retired"):
            import_legacy()


def _v7_client():
    version = "augment-site-20260928-e5ed84fcf8e72a24"
    spec = TypedEncoderSpec.from_catalog(
        site_catalog(version), observation_policy=site_observation_policy(version))
    return GameAdapterClient.new_game({"gameStyle": "normal"}, 37, spec=spec)


def test_v7_public_calls_preserve_intents_and_paged_order():
    client = _v7_client()
    initial = client.observe("white")
    revision = client.snapshot_revision
    intents = client.legal_intents()
    assert intents and all("positionId" not in intent and "actionId" not in intent
                           for intent in intents)
    stream = client.action_stream()
    paged = []
    while True:
        page = stream.next_page(1)
        assert set(page) == {"actions", "exhausted", "examined"}
        assert len(page["actions"]) <= page["examined"] <= 1
        paged.extend(action.public_intent() for action in page["actions"])
        if page["exhausted"]:
            break
    assert tuple(paged) == intents
    assert stream.next_page(1) == {"actions": (), "exhausted": True, "examined": 0}
    with pytest.raises(ValueError, match="page limit"):
        stream.next_page(4097)
    action = client.bind_public_intent(intents[0])
    assert action.public_intent() == intents[0]
    step = client.apply(action)
    assert client.snapshot_revision == revision and client.observe("white") == initial
    assert step.position.snapshot_revision != revision
    assert step.actor == intents[0]["color"]
    assert step.position.observe("white")["history"][-1]["actor"] == step.actor


def test_v7_lifetime_branching_and_stale_actions_are_immutable():
    client = _v7_client()
    original_revision = client.snapshot_revision
    original_public = client.observe("white")
    action = client.bind_public_intent(client.legal_intents()[0])
    branch = client.apply(action).position
    assert client.snapshot_revision == original_revision
    assert client.observe("white") == original_public
    with pytest.raises(ValueError, match="another game adapter revision"):
        branch.apply(action)
    with pytest.raises(StaleActionError):
        branch._session.fork(snapshot_revision=original_revision)
    observed = branch.observe("white")
    unchanged = deepcopy(observed)
    observed["board"][6][0] = None
    assert branch.observe("white") == unchanged
    assert client.apply(action).position.observe("white") == unchanged
    del client
    assert action.public_intent()["color"] == "white"
    assert branch.legal_intents()


def test_v7_transport_rejects_corruption_and_private_metadata():
    client = _v7_client()
    revision = client.snapshot_revision
    intent = client.legal_intents()[0]
    with pytest.raises(ValueError, match="private field"):
        client.bind_public_intent({**intent, "positionId": "private"})
    with pytest.raises(ValueError):
        client.bind_public_intent({**intent, "clientNote": "unverified"})
    with pytest.raises(StaleActionError):
        client._session.fork(snapshot_revision=f"{revision}-expired")
    with pytest.raises((NativeError, ValueError)):
        GameAdapterSession.from_envelope({"protocolVersion": "corrupt"})
    with pytest.raises(ValueError, match="uint32"):
        GameAdapterClient.new_game({"gameStyle": "normal"}, 2**32, spec=client.spec)
    assert client.snapshot_revision == revision
    assert client.bind_public_intent(intent).public_intent() == intent


def test_v7_public_observation_excludes_private_state_and_returns_owned_copies():
    client = _v7_client()
    observation = client.observe("white")
    assert set(observation) == {"protocolVersion", "viewer", "board", "turn", "ownCards",
                                "opponentHandCount", "publicState", "history", "informationStateKey"}
    serialized = json.dumps(observation)
    assert client.snapshot_revision not in serialized
    assert "lcg32-v1" not in serialized and "futureDraftOffers" not in serialized
    original = deepcopy(observation)
    observation["board"][6][0] = {"type": "queen", "color": "white"}
    observation["history"].append({"actor": "forged"})
    assert client.observe("white") == original
    with pytest.raises(ValueError, match="viewer"):
        client.observe("spectator")


def test_v7_native_numpy_conversion_is_finite_and_bounded():
    version = "augment-site-20260928-e5ed84fcf8e72a24"
    # The empty strided vector is a valid source RULE pool. It crosses the
    # native conversion boundary before the host owns the configuration.
    pool = np.arange(0, dtype=np.int64)[::2]
    session = GameAdapterSession.new_game({"gameStyle": "normal", "ruleCardIds": pool},
                                          37, rules_version=version)
    assert session.decision_actor == "white"
    for invalid in (np.array([np.nan]), np.array([np.inf]),
                    np.array([2**53], dtype=np.uint64),
                    np.array([1e21], dtype=np.float64),
                    np.array([object()], dtype=object)):
        with pytest.raises((TypeError, ValueError)):
            GameAdapterSession.new_game({"starWinLimit": invalid}, 37,
                                        rules_version=version)
    with pytest.raises(ValueError, match="limit"):
        GameAdapterSession.new_game({"starWinLimit": np.empty((100_001, 0))}, 37,
                                    rules_version=version)
    with pytest.raises(ValueError, match="exact"):
        GameAdapterSession.new_game({"starWinLimit": 2**53}, 37,
                                    rules_version=version)


def test_v7_native_threads_share_no_mutable_branch_state():
    client = _v7_client()
    before = client.observe("white")
    revision = client.snapshot_revision
    action = client.bind_public_intent(client.legal_intents()[0])
    def branch(_):
        child = client.apply(action).position
        return child.snapshot_revision, child.observe("white")
    with ThreadPoolExecutor(max_workers=4) as pool:
        branches = list(pool.map(branch, range(16)))
    assert all(branch == branches[0] for branch in branches)
    assert client.snapshot_revision == revision and client.observe("white") == before
    branches[0][1]["board"][6][0] = None
    assert branches[1][1]["board"][6][0] == before["board"][6][0]
