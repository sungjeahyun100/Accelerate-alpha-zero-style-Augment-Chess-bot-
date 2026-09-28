"""Permanent FFI contracts: ownership, versioned transport and explicit errors."""
from concurrent.futures import ThreadPoolExecutor
from copy import deepcopy
import hashlib
import json

import jcs
import numpy as np
import pytest

from accelerate_chess import (Position, NativeError, StaleActionError,
                              ConditioningMismatchError, site_catalog)


def _digest(value):
    return hashlib.sha256(jcs.canonicalize(value)).hexdigest()


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


def _move(position, from_square, to_square):
    return next(a for a in position.legal_actions()
                if a.as_payload().get("from") == {"row": from_square[0], "col": from_square[1]}
                and a.as_payload().get("move", {}).get("row") == to_square[0]
                and a.as_payload().get("move", {}).get("col") == to_square[1])


def test_direct_calls_match_versioned_json_and_preserve_source_presence():
    raw = _state()
    position = Position.from_state(raw)
    assert position.state() == raw
    snapshot = position.snapshot()
    assert snapshot["state"] == raw
    content = {k: v for k, v in snapshot.items() if k != "positionId"}
    assert position.position_id == _digest(content)
    restored = Position.from_json(position.to_json())
    assert restored.snapshot() == snapshot == Position.from_snapshot(snapshot).snapshot()
    numeric = deepcopy(snapshot)
    numeric["rng"]["tape"] = [0.0, 0.5]
    numeric["positionId"] = _digest({k: v for k, v in numeric.items() if k != "positionId"})
    canonical_snapshot = json.loads(jcs.canonicalize(numeric))
    canonical_restored = Position.from_snapshot(canonical_snapshot)
    assert jcs.canonicalize(canonical_restored.snapshot()) == jcs.canonicalize(numeric)
    action = _move(position, (6, 0), (5, 0))
    assert action.action_id == _digest(action.as_payload())
    assert "positionKey" not in action.as_payload()
    assert action.snapshot()["positionId"] == position.position_id
    assert position.bind_snapshot(action.snapshot()).as_payload() == action.as_payload()
    intent = action.public_intent()
    assert position.bind_public_intent(intent).as_payload() == action.as_payload()
    assert position.bind_public_intent(intent).public_intent() == intent
    stream = position.action_stream()
    paged = []
    while True:
        page = stream.next_page(3)
        assert set(page) == {"actions", "exhausted"} and len(page["actions"]) <= 3
        paged.extend(action.snapshot() for action in page["actions"])
        if page["exhausted"]:
            break
    assert paged == [a.snapshot() for a in position.legal_actions()]
    assert stream.next_page(3) == {"actions": (), "exhausted": True}
    with pytest.raises(ValueError, match="page size"):
        stream.next_page(4097)
    del stream
    step = position.apply(action)
    other = restored.apply(restored.bind_action(action.as_payload()))
    assert step.position.snapshot() == other.position.snapshot()
    assert step.actor == "white" and step.turn_changed
    assert step.position.actor == step.position.decision_actor == "black"
    assert step.captures == [] and step.result is None
    assert step.position.state()["deckSlots"]["white"] == [None]
    assert step.position.state()["marker"] == raw["marker"]
    history = step.position.snapshot()["history"]
    assert len(history) == 1 and history[0]["protocolVersion"] == "accelerate-game-event-v1"
    assert step.position.observe("white")["history"] == [history[0]["public"]["white"]]
    assert "action" not in step.position.observe("white")["history"][0]


def test_lifetime_branching_and_stale_actions_are_immutable():
    position = Position.from_state(_state())
    before = position.snapshot()
    action = _move(position, (6, 0), (5, 0))
    branch = position.apply(action).position
    assert position.snapshot() == before
    with pytest.raises(StaleActionError):
        branch.apply(action)
    expected_public = branch.observe("white")
    with pytest.raises(StaleActionError):
        branch.apply_conditioned_public(action, expected_public, 71)
    with pytest.raises(StaleActionError):
        branch.public_transition_compatible(action, expected_public)
    with pytest.raises(StaleActionError):
        branch.apply_weighted_conditioned_public(action, expected_public, 71)
    assert position.public_transition_compatible(action, expected_public)
    proposal = position.apply_weighted_conditioned_public(action, expected_public, 71)
    assert set(proposal) == {"step", "importance_weight", "source_probability", "proposal_probability"}
    assert proposal["importance_weight"] == proposal["source_probability"] / proposal["proposal_probability"] == 1.
    proposed = proposal["step"].position
    assert proposed.observe("white") == expected_public
    # Returned control mappings and caller-owned observations cannot mutate
    # either immutable native branch, even when the Python owners are released.
    proposed_before = proposed.snapshot()
    proposal["source_probability"] = 0.
    expected_public["board"][5][0]["type"] = "queen"
    assert proposed.snapshot() == proposed_before and branch.observe("white")["board"][5][0]["type"] == "rook"
    del proposal
    assert proposed.snapshot() == proposed_before
    expected_public = branch.observe("white")
    with pytest.raises(OverflowError):
        position.apply_conditioned_public(action, expected_public, 2**32)
    with pytest.raises(OverflowError):
        position.condition_hidden_opening_draft(expected_public, 2**32)
    with pytest.raises(OverflowError):
        position.apply_weighted_conditioned_public(action, expected_public, 2**32)
    # Public-frame conversion must fail before detached native work starts.
    with pytest.raises(TypeError):
        position.public_transition_compatible(action, {object(): 1})
    with pytest.raises(TypeError):
        position.condition_hidden_opening_draft({"board": object()}, 71)
    assert position.snapshot() == before
    payload = action.as_payload()
    payload["move"]["magicCapture"] = True
    with pytest.raises(NativeError, match="legal"):
        position.bind_action(payload)
    assert action.as_payload() != payload and position.snapshot() == before
    assert position.apply(action).position.snapshot() == branch.snapshot()
    stream = position.action_stream()
    expected_ids = sorted(a.action_id for a in position.legal_actions())
    del position
    # Action owns its payload and derived branches own their snapshot, with no
    # Python parent reference or mutable alias needed to keep either alive.
    assert branch.legal_actions() and action.as_payload()["from"]["row"] == 6
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
    assert sorted(a.action_id for page in pages for a in page) == expected_ids
    assert all(a.as_payload()["color"] == "white" for page in pages for a in page)
    assert action.public_intent()["from"] == {"row": 6, "col": 0}
    with pytest.raises(AttributeError):
        action.position_id = "rewritten"


def test_transport_rejects_corruption_versions_and_duplicate_private_metadata():
    position = Position.from_state(_state())
    catalog = site_catalog()
    assert catalog["rulesVersion"] == position.snapshot()["rulesVersion"]
    catalog["rulesVersion"] = "caller mutation"
    assert site_catalog()["rulesVersion"] != catalog["rulesVersion"]
    observation = position.observe("white")
    assert position.condition_public_identities(observation).observe("white") == observation
    observation["board"][6][0]["type"] = "bishop"
    observation["informationStateKey"] = _digest(
        {k: v for k, v in observation.items() if k != "informationStateKey"})
    with pytest.raises(ConditioningMismatchError):
        position.condition_public_identities(observation)
    observation["informationStateKey"] = "invalid"
    with pytest.raises(NativeError) as malformed:
        position.condition_public_identities(observation)
    assert not isinstance(malformed.value, ConditioningMismatchError)
    snapshot = position.snapshot()
    for field, value in (("positionId", "bad"), ("protocolVersion", "old"),
                         ("rulesVersion", "old"), ("catalogVersion", "old")):
        bad = deepcopy(snapshot)
        bad[field] = value
        with pytest.raises(ValueError):
            Position.from_snapshot(bad)
    bad = deepcopy(snapshot)
    bad["state"]["rng"] = bad["rng"]
    bad["positionId"] = _digest({k: v for k, v in bad.items() if k != "positionId"})
    with pytest.raises(ValueError, match="duplicate"):
        Position.from_snapshot(bad)
    with pytest.raises(ValueError):
        Position.from_json("{bad json")
    with pytest.raises(ValueError):
        Position.new_game(seed=2**32)
    bad = deepcopy(snapshot)
    bad["history"] = [{"actor": "white", "unexpectedPrivate": "invalid"}]
    bad["positionId"] = _digest({k: v for k, v in bad.items() if k != "positionId"})
    with pytest.raises(ValueError, match="history"):
        Position.from_snapshot(bad)
    action = position.legal_actions()[0]
    bad_action = action.snapshot()
    bad_action["actionId"] = "bad"
    with pytest.raises(ValueError, match="identity"):
        position.bind_snapshot(bad_action)


def test_private_state_is_absent_from_public_observation_and_numpy_copy():
    raw = _state()
    raw["board"][1][0]["hiddenFrom"] = "white"
    raw["futureDraftOffers"] = ["private-offer"]
    position = Position.from_state(raw)
    observation = position.observe("white")
    assert set(observation) == {"protocolVersion", "viewer", "board", "turn", "ownCards",
                                "opponentHandCount", "publicState", "history", "informationStateKey"}
    assert observation["board"][1][0] is None
    assert observation["informationStateKey"] == _digest({k: v for k, v in observation.items() if k != "informationStateKey"})
    serialized = json.dumps(observation)
    assert position.position_id not in serialized
    assert "lcg32-v1" not in serialized and "private-offer" not in serialized
    public_board = position.board("white")
    assert public_board.shape == (8, 8) and public_board.dtype == object
    assert public_board.tolist() == observation["board"]
    public_board[6, 0]["type"] = "queen"
    assert position.observe("white")["board"][6][0]["type"] == "rook"
    with pytest.raises(ValueError):
        position.observe("spectator")


def test_numpy_input_is_owned_strided_finite_and_bounded():
    raw = _state()
    values = np.arange(12, dtype=np.float64).reshape(3, 4)[:, ::2]
    raw["arrayMarker"] = values
    position = Position.from_state(raw)
    assert position.state()["arrayMarker"] == values.tolist()
    values.fill(-1)
    assert position.state()["arrayMarker"] == [[0., 2.], [4., 6.], [8., 10.]]
    for invalid in (np.array([np.nan]), np.array([np.inf]), np.array([2**53], dtype=np.uint64),
                    np.array([1e21], dtype=np.float64), np.array([-1e21], dtype=np.float32),
                    np.array([object()], dtype=object)):
        raw["arrayMarker"] = invalid
        with pytest.raises((TypeError, ValueError)):
            Position.from_state(raw)
    raw["arrayMarker"] = np.empty((100_001, 0), dtype=np.float64)
    with pytest.raises(ValueError, match="limits"):
        Position.from_state(raw)
    raw["arrayMarker"] = 2**53
    with pytest.raises(ValueError, match="exact"):
        Position.from_state(raw)
    raw["arrayMarker"] = 1e21
    with pytest.raises(ValueError, match="exact"):
        Position.from_state(raw)
    raw["arrayMarker"] = [float("nan")]
    with pytest.raises(ValueError, match="finite"):
        Position.from_state(raw)
    raw["arrayMarker"] = [[]]
    nested = raw["arrayMarker"]
    for _ in range(70):
        nested.append([])
        nested = nested[-1]
    with pytest.raises(ValueError, match="nesting"):
        Position.from_state(raw)


def test_multicell_entity_identity_and_native_threads_share_no_mutable_state():
    raw = _state()
    raw["board"][6][0] = None
    piece = {"type": "bigRook", "color": "white", "id": "large",
             "anchorRow": 4, "anchorCol": 3}
    for row in (4, 5):
        for col in (3, 4):
            raw["board"][row][col] = deepcopy(piece)
    position = Position.from_state(raw)
    before = position.to_json()
    action = _move(position, (4, 3), (3, 3))
    def branch(_):
        step = position.apply(action)
        assert sum(cell is not None and cell.get("id") == "large"
                   for row in step.position.state()["board"] for cell in row) == 4
        return step.position.to_json(), step.position.observe("white")
    with ThreadPoolExecutor(max_workers=4) as pool:
        branches = list(pool.map(branch, range(32)))
    assert all(branch == branches[0] for branch in branches)
    assert position.to_json() == before
    del action, position
    assert json.loads(branches[0][0])["state"]["board"][3][3]["id"] == "large"
