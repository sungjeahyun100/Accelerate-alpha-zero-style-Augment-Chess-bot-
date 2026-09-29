"""Focused public typed IR checks without generated fixtures or trained models."""

from __future__ import annotations

from copy import deepcopy
from functools import lru_cache
import hashlib
import json
from pathlib import Path

import numpy as np
import pytest

from accelerate_chess.encoding import canonical_json
from accelerate_chess.ir import (
    BoardGeometry, HISTORY_VERSION, ObservationIR, TypedEncoder,
    TypedEncoderSpec, batch_typed_positions, validate_typed_public_observation,
)


@lru_cache(maxsize=1)
def source():
    root = Path(__file__).resolve().parents[2] / "bridge" / "catalog"
    catalog = json.loads((root / "site-20260928.json").read_text(encoding="utf-8"))
    policy = json.loads((root / "observation-20260928.json").read_text(encoding="utf-8"))
    return catalog, policy


@lru_cache(maxsize=1)
def spec():
    catalog, policy = source()
    return TypedEncoderSpec.from_catalog(catalog, observation_policy=policy)


def signed(observation):
    observation["informationStateKey"] = hashlib.sha256(canonical_json({
        key: value for key, value in observation.items() if key != "informationStateKey"
    }).encode()).hexdigest()
    return observation


def frame():
    _, policy = source()
    board = [[None for _ in range(8)] for _ in range(8)]
    board[6][1] = {"type": "pawn", "color": "white", "status": {"witchTrial": True},
                   "anchorRow": 6, "anchorCol": 1}
    return signed({"protocolVersion": "accelerate-observation-v2", "viewer": "white",
                   "turn": "white", "board": board,
                   "ownCards": [{"id": "slime", "instanceId": "arbitrary-public-slot-1", "used": False}],
                   "opponentHandCount": 2,
                   "publicState": {"rulesVersion": policy["rulesVersion"],
                                   "projectionVersion": policy["projectionVersion"],
                                   "observationPolicyHash": spec().observation_policy_hash,
                                   "deathmatchStatus": {"active": False, "warning": False},
                                   "actionsRemaining": 1, "moveCount": 1, "fullMove": 1,
                                   "collapsedCells": [{"row": 7, "col": 7}],
                                   "boardMarks": [{"kind": "fogHidden", "square": {"row": 2, "col": 2}}],
                                   "relationships": [], "overlays": [],
                                   "legalHints": {"moves": [], "cardTargets": [{
                                       "cardInstanceId": "arbitrary-public-slot-1",
                                       "targets": [{"row": 3, "col": 2}]}]}},
                   "history": [], "informationStateKey": ""})


def intents():
    return [
        {"type": "move", "color": "white", "from": {"row": 6, "col": 1},
         "destination": {"row": 5, "col": 1}},
        {"type": "card", "color": "white", "cardId": "slime",
         "cardInstanceId": "arbitrary-public-slot-1",
         "target": {"row": 3, "col": 2,
                    "selections": [{"row": 3, "col": 3}, {"row": 4, "col": 3}]}}
    ]


def test_source_bound_spec_and_public_v2_geometry_cells():
    contract = spec()
    assert contract.digest == hashlib.sha256(canonical_json(contract.contract()).encode()).hexdigest()
    assert contract.feature_schema_hash == hashlib.sha256(canonical_json(contract.feature_schema).encode()).hexdigest()
    assert contract.feature_schema["input_order"]["mask-resnet"][:2] == ["spatial", "layout_mask"]
    assert len(contract.category_vocabulary) < 65_536
    catalog, policy = source()
    assert TypedEncoderSpec.from_dict(contract.to_dict(), catalog=catalog, observation_policy=policy).digest == contract.digest
    ir = ObservationIR.from_public(frame(), contract)
    assert ir.cell_kinds[0][0] == "empty"
    assert ir.cell_kinds[2][2] == "unknown"
    assert ir.cell_kinds[6][1] == "piece"
    assert ir.cell_kinds[7][7] == "hole"
    encoded = TypedEncoder(contract).encode(ir, intents(), examined=5, exhaustive=False)
    spatial, layout = encoded.inputs["spatial"], encoded.inputs["layout_mask"]
    assert spatial.shape == (6, 8, 8) and layout.shape == (1, 8, 8)
    assert spatial[0, 0, 0] == spatial[1, 2, 2] == spatial[2, 7, 7] == 1
    # An unmarked public null stays empty even if private state could contain
    # a hiddenFrom/camouflage occupant at that square. Only visible fog marks
    # authorize an unknown-cell feature.
    assert spatial[0, 2, 4] == 1 and spatial[1, 2, 4] == 0
    assert spatial[1].sum() == 1
    assert spatial[3, 6, 1] == spatial[4, 6, 1] == 1
    assert layout[0, 7, 7]
    assert encoded.examined == 5 and not encoded.exhaustive
    assert encoded.inputs["record_mask"][0]
    assert encoded.inputs["candidate_node_mask"][:, 0].all()
    assert np.any(encoded.inputs["candidate_target_index"][1] >= 0)
    assert np.any(encoded.inputs["candidate_order"][1] == 1)
    relation_kinds = encoded.inputs["relation_category"][:, 0]
    assert np.any(relation_kinds == contract.category_id("semantic-link"))


def test_v7_public_v2_rectangular_board_has_a_typed_only_validation_boundary():
    public = frame()
    public["board"] = [row[:7] for row in public["board"][:5]]
    public["publicState"]["collapsedCells"] = []
    public["history"] = [{"kind": "transition", "actor": "white", "nextActor": "black",
                          "phase": "play", "boardChanges": [{"square": {"row": 6, "col": 7},
                                                               "before": None, "after": None}],
                          "ownCards": [], "revealedOpponentCards": [], "captures": [],
                          "result": {"outcome": "ongoing"}}]
    signed(public)
    with pytest.raises(ValueError, match="8x8"):
        spec().legacy_validator().validate_observation(public)
    verified = validate_typed_public_observation(public, spec())
    assert len(verified.board) == 5 and len(verified.board[0]) == 7
    ir = ObservationIR.from_public(public, spec())
    assert ir.geometry == BoardGeometry(0, 0, 5, 7)
    assert ir.history_summary["recent_events"][0]["board_change_squares"] == [[6, 7]]
    encoded = TypedEncoder(spec()).encode(ir, [])
    assert encoded.inputs["layout_mask"].shape == (1, 5, 7)
    assert encoded.inputs["layout_mask"].all()
    assert encoded.inputs["spatial"][1, 2, 2] == 1
    ragged = deepcopy(public)
    ragged["board"][0].pop()
    signed(ragged)
    with pytest.raises(ValueError, match="rectangular geometry"):
        ObservationIR.from_public(ragged, spec())
    source_root = Path(__file__).resolve().parents[2] / "bridge" / "catalog"
    old_catalog = json.loads((source_root / "site-20260927.json").read_text(encoding="utf-8"))
    old_policy = json.loads((source_root / "observation-20260927.json").read_text(encoding="utf-8"))
    old_spec = TypedEncoderSpec.from_catalog(old_catalog, observation_policy=old_policy)
    old_public = deepcopy(public)
    old_public["publicState"].update({"rulesVersion": old_spec.rules_version,
                                      "projectionVersion": old_policy["projectionVersion"],
                                      "observationPolicyHash": old_spec.observation_policy_hash})
    signed(old_public)
    with pytest.raises(ValueError, match="v7 typed profile"):
        validate_typed_public_observation(old_public, old_spec)


def test_private_metadata_and_arbitrary_id_renaming_do_not_enter_features():
    original = frame()
    action = intents()
    encoder = TypedEncoder(spec())
    before = encoder.encode(ObservationIR.from_public(original, spec()), action)
    renamed = deepcopy(original)
    renamed["ownCards"][0]["instanceId"] = "another-public-slot"
    renamed["publicState"]["legalHints"]["cardTargets"][0]["cardInstanceId"] = "another-public-slot"
    signed(renamed)
    action[1]["cardInstanceId"] = "another-public-slot"
    after = encoder.encode(ObservationIR.from_public(renamed, spec()), action)
    for name in before.inputs:
        np.testing.assert_array_equal(before.inputs[name], after.inputs[name])
    assert before.action_keys != after.action_keys
    tampered = frame()
    tampered["positionId"] = "hidden-position"
    with pytest.raises(ValueError):
        ObservationIR.from_public(tampered, spec())
    with pytest.raises(ValueError, match="private field"):
        encoder.encode(ObservationIR.from_public(frame(), spec()), [{"type": "move", "rng": 1}])


def test_synthetic_geometry_padding_and_candidate_split():
    catalog, policy = source()
    synthetic = TypedEncoderSpec.from_catalog(catalog, observation_policy=policy,
                                              observation_version="synthetic-geometry-v1")
    first = ObservationIR.from_components(spec=synthetic, geometry=BoardGeometry(-2, 4, 5, 7),
                                          viewer="black", turn="black",
                                          board=[[None] * 7 for _ in range(5)],
                                          cell_kinds=[["empty"] * 7 for _ in range(5)])
    second = ObservationIR.from_components(spec=synthetic, geometry=BoardGeometry(0, 0, 1, 1),
                                           viewer="white", turn="white", board=[[None]],
                                           cell_kinds=[["hole"]])
    encoder = TypedEncoder(synthetic)
    all_actions = [{"type": "move", "from": {"row": -2, "col": 4},
                    "color": "black", "destination": {"row": -1, "col": 4}},
                   {"type": "move", "from": {"row": -2, "col": 4},
                    "color": "black", "destination": {"row": 0, "col": 4}}]
    whole = encoder.encode(first, all_actions)
    split = encoder.encode(first, all_actions[1:])
    batched = batch_typed_positions([whole, encoder.encode(second, [])])
    assert batched.inputs["spatial"].shape == (2, 6, 5, 7)
    assert batched.layout_mask[0].all()
    assert batched.layout_mask[1, 0, 0, 0]
    assert not batched.layout_mask[1, 0, 1:, :].any()
    assert batched.spatial[1, 2, 0, 0] == 1
    assert batched.candidate_mask.tolist() == [[True, True], [False, False]]
    for name in ("candidate_category", "candidate_numeric", "candidate_coord",
                 "candidate_coord_valid", "candidate_parent", "candidate_order",
                 "candidate_target_index", "candidate_node_mask"):
        np.testing.assert_array_equal(whole.inputs[name][1, :split.inputs[name].shape[1]],
                                      split.inputs[name][0])
    assert batched.as_family_inputs("entity-transformer")[0].shape[0] == 2


def test_public_footprint_has_one_piece_entity_and_ordered_cell_links():
    catalog, policy = source()
    synthetic = TypedEncoderSpec.from_catalog(catalog, observation_policy=policy,
                                              observation_version="synthetic-geometry-v1")
    piece = {"type": "bigRook", "color": "white", "status": {},
             "anchorRow": -1, "anchorCol": 2}
    board = [[None for _ in range(3)] for _ in range(2)]
    board[0][0] = deepcopy(piece)
    board[0][1] = deepcopy(piece)
    kinds = [["piece", "piece", "empty"], ["empty", "empty", "empty"]]
    ir = ObservationIR.from_components(spec=synthetic, geometry=BoardGeometry(-1, 2, 2, 3),
                                       viewer="white", turn="white", board=board, cell_kinds=kinds)
    encoded = TypedEncoder(synthetic).encode(ir, [{"type": "move", "color": "white",
                                                   "from": {"row": -1, "col": 2},
                                                   "destination": {"row": 0, "col": 2}}])
    kind_slot = encoded.inputs["record_category"][:, 0]
    assert np.count_nonzero(kind_slot == synthetic.category_id("piece")) == 1
    relation_kinds = encoded.inputs["relation_category"][:, 0]
    assert np.count_nonzero(relation_kinds == synthetic.category_id("occupies")) == 2
    from_nodes = encoded.inputs["candidate_category"][0, :, 1] == synthetic.category_id("from")
    assert encoded.inputs["candidate_target_index"][0, from_nodes].max() >= 0


def test_ordered_draft_bundle_references_public_choices():
    public = frame()
    public["ownCards"] = []
    public["publicState"]["draft"] = {"kind": "chaos", "phase": "OPENING", "color": "white",
                                      "choices": [{"id": "slime", "instanceId": "slot-a"},
                                                  {"id": "slime", "instanceId": "slot-b"}]}
    signed(public)
    ir = ObservationIR.from_public(public, spec())
    action = {"type": "draftBundlePick", "color": "white", "bundleIndex": 0,
              "cardInstanceIds": ["slot-a", "slot-b"]}
    encoded = TypedEncoder(spec()).encode(ir, [action])
    field_ids = encoded.inputs["candidate_category"][0, :, 1]
    selected = field_ids == spec().category_id("cardInstanceId")
    assert selected.sum() == 2
    assert encoded.inputs["candidate_order"][0, selected].tolist() == [0, 1]
    targets = encoded.inputs["candidate_target_index"][0, selected]
    assert np.all(targets >= 0) and targets[0] != targets[1]


def test_public_card_aliases_share_identity_but_reject_conflicting_entities():
    public = frame()
    choice = {"id": "king-of-the-hill", "instanceId": "shared-public-card",
              "effect": "kingOfTheHill", "phase": "OPENING", "stars": 2.5}
    public["publicState"]["draft"] = {"kind": "grand", "phase": "OPENING",
                                      "color": "white", "choices": [choice]}
    public["publicState"]["revealedOpponentCards"] = [{**choice, "slot": 0,
                                                        "firstTurnCard": True}]
    signed(public)
    action = {"type": "draftPick", "color": "white", "cardInstanceId": "shared-public-card"}
    encoded = TypedEncoder(spec()).encode(ObservationIR.from_public(public, spec()), [action])
    relation_kinds = encoded.inputs["relation_category"][:, 0]
    assert np.count_nonzero(relation_kinds == spec().category_id("same-identity")) == 1
    target = encoded.inputs["candidate_target_index"][0]
    assert np.count_nonzero(target >= 0) == 1
    assert encoded.inputs["record_category"][target[target >= 0][0], 0] == spec().category_id("object")
    conflicting = deepcopy(public)
    conflicting["publicState"]["revealedOpponentCards"][0]["id"] = "slime"
    signed(conflicting)
    with pytest.raises(ValueError, match="conflicting public card alias"):
        TypedEncoder(spec()).encode(ObservationIR.from_public(conflicting, spec()), [action])
    repeated = deepcopy(public)
    repeated["publicState"]["draft"]["choices"].append(deepcopy(choice))
    signed(repeated)
    with pytest.raises(ValueError, match="duplicate public card instance in one surface"):
        TypedEncoder(spec()).encode(ObservationIR.from_public(repeated, spec()), [action])
    opposite_owners = deepcopy(public)
    opposite_owners["ownCards"] = [deepcopy(choice)]
    signed(opposite_owners)
    with pytest.raises(ValueError, match="opposite owners"):
        TypedEncoder(spec()).encode(ObservationIR.from_public(opposite_owners, spec()), [action])


def test_public_belief_proposal_profiles_have_a_fixed_typed_domain():
    summary = {"version": "public-particle-summary-v3", "particle_count": 4,
               "distinct_particle_instances": 3, "trace_steps": 1,
               "opponent_action_prior": "uniform-public-intents",
               "filter_version": "source-importance-filter-v2",
               "chance_prior": "independent-source-draws",
               "conditional_steps": "source-weighted-conditional-step-v1",
               "proposal_profiles": ["source-prior-v1"],
               "effective_sample_size": 3.5}
    encoded = TypedEncoder(spec()).encode(ObservationIR.from_public(
        frame(), spec(), belief_summary=summary), [])
    assert np.any(encoded.inputs["record_category"][:, 2] == spec().category_id("source-prior-v1"))
    all_profiles = deepcopy(summary)
    all_profiles["proposal_profiles"] = sorted(("source-prior-v1",
                                                "source-weighted-conditional-step-v1",
                                                "source-weighted-offer-proposal-v1"))
    TypedEncoder(spec()).encode(ObservationIR.from_public(
        frame(), spec(), belief_summary=all_profiles), [])
    unknown = deepcopy(summary)
    unknown["proposal_profiles"] = ["unreviewed-proposal-v1"]
    with pytest.raises(ValueError, match="unknown typed public belief proposal profile"):
        ObservationIR.from_public(frame(), spec(), belief_summary=unknown)


def test_move_program_descriptor_is_typed_and_source_id_is_not_a_feature():
    catalog, policy = source()
    synthetic = TypedEncoderSpec.from_catalog(catalog, observation_policy=policy,
                                              observation_version="synthetic-geometry-v1")
    program = {"sourceId": "arbitrary-program-identity", "roots": [{
        "primitive": "MOVE", "direction": {"dr": 1, "dc": 0}, "maxDistance": 1,
        "activationCondition": "Any", "activateAtParentDistance": None,
        "children": [{"primitive": "SHIFT", "direction": {"dr": 0, "dc": 1},
                      "maxDistance": 1, "activationCondition": "NoCapture", "children": []}]}]}
    descriptor = {"base": program, "modifiers": [{"modifierId": "arbitrary-modifier-identity",
                                                  "source": "arbitrary-modifier-source",
                                                  "program": deepcopy(program),
                                                  "expiration": {"kind": "ownerTurns",
                                                                 "owner": "white", "remaining": 3}}]}
    base = dict(spec=synthetic, geometry=BoardGeometry(0, 0, 2, 2), viewer="white",
                turn="white", board=[[None] * 2 for _ in range(2)],
                cell_kinds=[["empty"] * 2 for _ in range(2)])
    first = TypedEncoder(synthetic).encode(ObservationIR.from_components(**base, descriptors=[descriptor]), [])
    descriptor["base"]["sourceId"] = "renamed-program-identity"
    descriptor["modifiers"][0]["modifierId"] = "renamed-modifier-identity"
    descriptor["modifiers"][0]["source"] = "renamed-modifier-source"
    descriptor["modifiers"][0]["program"]["sourceId"] = "renamed-modifier-program-identity"
    second = TypedEncoder(synthetic).encode(ObservationIR.from_components(**base, descriptors=[descriptor]), [])
    for name in first.inputs:
        np.testing.assert_array_equal(first.inputs[name], second.inputs[name])
    assert np.any(first.inputs["record_category"][:, 2] == synthetic.category_id("SHIFT"))
    assert np.any(first.inputs["record_category"][:, 2] == synthetic.category_id("ownerTurns"))
    invalid = deepcopy(descriptor)
    invalid["base"]["roots"][0]["direction"] = {"dr": 0, "dc": 0}
    with pytest.raises(ValueError, match="direction cannot be zero"):
        ObservationIR.from_components(**base, descriptors=[invalid])
    invalid = deepcopy(descriptor)
    invalid["base"]["roots"][0]["activateAtParentDistance"] = 1
    with pytest.raises(ValueError, match="root cannot have a parent distance"):
        ObservationIR.from_components(**base, descriptors=[invalid])


def test_fail_closed_limits_and_history_version():
    base = frame()
    base["publicState"]["unexpectedInternalField"] = 1
    signed(base)
    with pytest.raises(ValueError, match="unknown public state fields"):
        ObservationIR.from_public(base, spec())
    with pytest.raises(ValueError, match="candidate work accounting"):
        TypedEncoder(spec()).encode(ObservationIR.from_public(frame(), spec()), intents(), examined=1)
    with pytest.raises(ValueError, match="v7 public move intent"):
        TypedEncoder(spec()).encode(ObservationIR.from_public(frame(), spec()), [
            {"type": "move", "color": "white", "from": {"row": 6, "col": 1},
             "move": {"row": 5, "col": 1, "pieceId": "hidden"}}])
    catalog, policy = source()
    synthetic = TypedEncoderSpec.from_catalog(catalog, observation_policy=policy,
                                              observation_version="synthetic-geometry-v1")
    with pytest.raises(ValueError, match="history summary version"):
        TypedEncoder(synthetic).encode(ObservationIR.from_components(
            spec=synthetic, geometry=BoardGeometry(0, 0, 1, 1), viewer="white", turn="white",
            board=[[None]], cell_kinds=[["empty"]], history_summary={"version": "old"}), [])
    assert HISTORY_VERSION == "public-history-summary-v2"


def test_candidate_actor_must_match_the_value_perspective_in_a_draft():
    ir = ObservationIR.from_public(frame(), spec())
    with pytest.raises(ValueError, match="candidate decision actor"):
        TypedEncoder(spec()).encode(ir, [{"type": "draftPick", "color": "black",
                                          "cardInstanceId": "visible-draft-choice"}])
