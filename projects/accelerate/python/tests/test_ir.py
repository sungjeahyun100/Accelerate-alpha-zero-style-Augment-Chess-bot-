"""Focused public typed IR checks without generated fixtures or trained models."""

from __future__ import annotations

from copy import deepcopy
from dataclasses import replace
from functools import lru_cache
import hashlib
import json
from pathlib import Path

import numpy as np
import pytest

from accelerate_chess.encoding import PUBLIC_MOVE_SELECTION_MODES, PublicEncoder, canonical_json
from accelerate_chess.ir import (
    BoardGeometry, HISTORY_VERSION, MAX_CANDIDATE_NODES, ObservationIR, TypedEncoder,
    TypedEncoderSpec, batch_typed_positions, validate_typed_public_observation,
)


@lru_cache(maxsize=1)
def source():
    root = Path(__file__).resolve().parents[3] / "augment-chess" / "contracts" / "catalog"
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


def test_public_move_selection_is_distinct_typed_content_and_preserves_ordinary_intent():
    contract = spec()
    public = frame()
    ordinary = intents()[0]
    candidates = [ordinary, *[{**ordinary, "selectionMode": mode}
                              for mode in PUBLIC_MOVE_SELECTION_MODES]]
    encoded = TypedEncoder(contract).encode(ObservationIR.from_public(public, contract), candidates)
    assert encoded.actions == tuple(candidates)
    assert encoded.action_keys == tuple(canonical_json(intent) for intent in candidates)
    assert len(set(encoded.action_keys)) == 4
    categories = encoded.inputs["candidate_category"]
    selector_nodes = categories[:, :, 1] == contract.category_id("selectionMode")
    assert not selector_nodes[0].any()
    for index, mode in enumerate(PUBLIC_MOVE_SELECTION_MODES, start=1):
        assert selector_nodes[index].sum() == 1
        assert categories[index, selector_nodes[index], 2].tolist() == [contract.category_id(mode)]
    selector_contract = contract.feature_schema["public_move_selection"]
    assert selector_contract["modes"] == list(PUBLIC_MOVE_SELECTION_MODES)
    assert selector_contract["ordinary"] == "field omitted"

    # The compatibility feature path preserves the same public intent identity;
    # it must not manufacture private source flags for a selected click.
    utf8 = PublicEncoder(replace(contract.legacy_validator().spec,
                                 action_encoding="public-decision-intent-v1"))
    assert utf8.encode(public, candidates).action_keys == encoded.action_keys


@pytest.mark.parametrize("mode", ["move", "reload", "shotgunBlast", "unrecognized", None, True, []])
def test_public_move_selection_rejects_noncanonical_values_before_encoding(mode):
    contract = spec()
    public = frame()
    candidate = {**intents()[0], "selectionMode": mode}
    with pytest.raises(ValueError, match="selectionMode"):
        TypedEncoder(contract).encode(ObservationIR.from_public(public, contract), [candidate])
    utf8 = PublicEncoder(replace(contract.legacy_validator().spec,
                                 action_encoding="public-decision-intent-v1"))
    with pytest.raises(ValueError, match="selectionMode"):
        utf8.encode(public, [candidate])


def test_public_move_selection_cannot_enter_other_action_types_or_private_flags():
    contract = spec()
    public_ir = ObservationIR.from_public(frame(), contract)
    candidates = [{**intents()[1], "selectionMode": "shotgun"},
                  {**intents()[0], "selectionMode": "shotgun", "flags": {"shotgunBlast": True}},
                  {**intents()[0], "selectionMode": "log-direction",
                   "destination": {"row": 5, "col": 1, "setLogDirection": {"dr": -1, "dc": 0}}}]
    for candidate in candidates:
        with pytest.raises(ValueError, match="selectionMode|v7 public move intent"):
            TypedEncoder(contract).encode(public_ir, [candidate])


@pytest.mark.parametrize(("selection_count", "expected_nodes"), [(20, 67), (64, 199)])
def test_complete_public_selection_preserves_all_ordered_coordinates(selection_count, expected_nodes):
    contract = spec()
    public = frame()
    public["ownCards"] = [{"id": "pawn-storm", "instanceId": "public-pawn-storm", "used": False}]
    public["publicState"]["legalHints"]["cardTargets"] = []
    signed(public)
    # This checks the representation boundary. Source admission separately
    # checks whether each selected square contains an eligible pawn.
    selected = [{"row": index // 8, "col": index % 8}
                for index in range(selection_count - 1, -1, -1)]
    candidate = {"type": "card", "color": "white", "cardId": "pawn-storm",
                 "cardInstanceId": "public-pawn-storm", "target": {"selections": selected}}
    public_ir = ObservationIR.from_public(public, contract)
    encoded = TypedEncoder(contract).encode(public_ir, [candidate])
    assert contract.max_candidate_nodes == MAX_CANDIDATE_NODES == 256
    assert encoded.inputs["candidate_node_mask"][0].sum() == expected_nodes
    assert encoded.actions == (candidate,)
    assert encoded.action_keys == (canonical_json(candidate),)
    categories = encoded.inputs["candidate_category"][0]
    selection_array = np.flatnonzero(categories[:, 1] == contract.category_id("selections"))
    assert selection_array.size == 1
    coordinates = np.flatnonzero(encoded.inputs["candidate_coord_valid"][0])
    assert coordinates.size == selection_count
    np.testing.assert_array_equal(encoded.inputs["candidate_order"][0, coordinates],
                                  np.arange(selection_count))
    np.testing.assert_array_equal(encoded.inputs["candidate_parent"][0, coordinates],
                                  np.full(selection_count, selection_array[0]))
    # Numeric row/col slots are absolute; normalized coordinates use the same
    # caller-provided order and the independently known 8x8 geometry.
    expected_coordinates = np.array([[item["row"], item["col"]] for item in selected])
    np.testing.assert_array_equal(encoded.inputs["candidate_numeric"][0, coordinates, 4:6],
                                  expected_coordinates)
    np.testing.assert_allclose(encoded.inputs["candidate_coord"][0, coordinates],
                               expected_coordinates / 7, atol=1e-7, rtol=0)

    legacy = replace(contract, max_candidate_nodes=64)
    with pytest.raises(ValueError, match="candidate node count exceeds 64"):
        TypedEncoder(legacy).encode(public_ir, [candidate])
    limited = replace(contract, max_input_bytes=1)
    with pytest.raises(ValueError, match="typed input uses more than 1 bytes"):
        TypedEncoder(limited).encode(public_ir, [candidate])


def test_typed_candidate_deployment_node_cap_is_finite():
    with pytest.raises(ValueError, match="max_candidate_nodes exceeds"):
        replace(spec(), max_candidate_nodes=257)


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


def test_source_bound_v7_observation_rejects_synthetic_rectangular_geometry():
    public = frame()
    public["board"] = [row[:7] for row in public["board"][:5]]
    public["publicState"]["collapsedCells"] = []
    signed(public)
    with pytest.raises(ValueError, match="8x8"):
        spec().legacy_validator().validate_observation(public)
    with pytest.raises(ValueError, match="source-bound.*8x8"):
        validate_typed_public_observation(public, spec())
    with pytest.raises(ValueError, match="source-bound.*8x8"):
        ObservationIR.from_public(public, spec())
    ragged = deepcopy(public)
    ragged["board"][0].pop()
    signed(ragged)
    with pytest.raises(ValueError, match="rectangular geometry"):
        ObservationIR.from_public(ragged, spec())
    source_root = Path(__file__).resolve().parents[3] / "augment-chess" / "contracts" / "catalog"
    old_catalog = json.loads((source_root / "site-20260927.json").read_text(encoding="utf-8"))
    old_policy = json.loads((source_root / "observation-20260927.json").read_text(encoding="utf-8"))
    old_spec = TypedEncoderSpec.from_catalog(old_catalog, observation_policy=old_policy)
    old_public = deepcopy(public)
    old_public["publicState"].update({"rulesVersion": old_spec.rules_version,
                                      "projectionVersion": old_policy["projectionVersion"],
                                      "observationPolicyHash": old_spec.observation_policy_hash})
    signed(old_public)
    with pytest.raises(ValueError, match="source-bound.*8x8"):
        validate_typed_public_observation(old_public, old_spec)
    public_ir = ObservationIR.from_public(frame(), spec())
    with pytest.raises(ValueError, match="source-bound.*8x8"):
        replace(public_ir, geometry=BoardGeometry(0, 0, 1, 1),
                board=((None,),), cell_kinds=(("empty",),))
    with pytest.raises(ValueError, match="source-bound.*8x8"):
        replace(public_ir, geometry=BoardGeometry(1, 0, 8, 8))


def test_source_ir_must_remain_the_verified_from_public_projection():
    encoder = TypedEncoder(spec())
    verified = ObservationIR.from_public(frame(), spec())
    encoder.encode(verified, intents())

    # A caller can copy the projected values, but that copy has not passed the
    # raw observation's signature and visibility-policy boundary.
    manually_built = ObservationIR(**{
        name: value for name, value in vars(verified).items() if name != "_public_seal"
    })
    with pytest.raises(ValueError, match="unchanged from_public"):
        encoder.encode(manually_built, intents())

    with pytest.raises(ValueError, match="unchanged from_public"):
        encoder.encode(replace(verified, viewer="black"), intents())

    # Frozen dataclass fields can still contain mutable JSON objects.
    verified.public_state["actionsRemaining"] = 2
    with pytest.raises(ValueError, match="unchanged from_public"):
        encoder.encode(verified, intents())


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


def test_synthetic_public_history_summary_is_typed_and_keeps_past_absolute_squares():
    catalog, policy = source()
    synthetic = TypedEncoderSpec.from_catalog(catalog, observation_policy=policy,
                                              observation_version="synthetic-geometry-v1")
    summary = {"version": HISTORY_VERSION, "event_count": 1,
               "actor_counts": {"white": 1, "black": 0},
               "decision_actor_changes": 1, "board_change_count": 1,
               "recent_events": [{"actor": "white", "nextActor": "black", "phase": "OPENING",
                                  "board_change_count": 1, "board_change_squares": [[-2, 4]],
                                  "own_card_count": 0, "opponent_card_count": 0,
                                  "outcome": None}], "history_hash": "a" * 64}
    base = dict(spec=synthetic, geometry=BoardGeometry(0, 0, 1, 1), viewer="white",
                turn="black", board=[[None]], cell_kinds=[["empty"]])
    synthetic_ir = ObservationIR.from_components(**base, history_summary=summary)
    first = TypedEncoder(synthetic).encode(synthetic_ir, [])
    with pytest.raises(ValueError, match="synthetic IR information state binding is stale"):
        replace(synthetic_ir, viewer="black")
    # The old event had no geometry. Its public coordinates remain absolute,
    # even though neither is on the current 1x1 board.
    numeric_values = first.inputs["record_numeric"][:, 0]
    assert -2 in numeric_values and 4 in numeric_values
    changed_digest = deepcopy(summary)
    changed_digest["history_hash"] = "b" * 64
    second = TypedEncoder(synthetic).encode(ObservationIR.from_components(
        **base, history_summary=changed_digest), [])
    for name in first.inputs:
        np.testing.assert_array_equal(first.inputs[name], second.inputs[name])

    malformed = [
        {**summary, "hiddenCardCount": 2},
        {**summary, "event_count": 2},
        {**summary, "board_change_count": 0},
        {**summary, "board_change_count": 2},
        {**summary, "recent_events": []},
        {**summary, "recent_events": [{**summary["recent_events"][0],
                                       "nextActor": "white"}]},
        {**summary, "recent_events": [{**summary["recent_events"][0],
                                       "board_change_squares": [[-2, 4], [3, 5]]}]},
        {**summary, "recent_events": [{**summary["recent_events"][0],
                                       "board_change_squares": [[1_000_001, 4]]}]},
        {**summary, "recent_events": [{**summary["recent_events"][0],
                                       "actor": ["white"]}]},
        {},
    ]
    for invalid in malformed:
        with pytest.raises(ValueError, match="history"):
            ObservationIR.from_components(**base, history_summary=invalid)

    longer = {**summary, "event_count": 9, "actor_counts": {"white": 9, "black": 0},
              "decision_actor_changes": 9, "board_change_count": 9,
              "recent_events": summary["recent_events"] * 8}
    ObservationIR.from_components(**base, history_summary=longer)
    with pytest.raises(ValueError, match="recent event window"):
        ObservationIR.from_components(**base, history_summary={
            **longer, "recent_events": longer["recent_events"][:-1]})


def test_candidate_actor_must_match_the_value_perspective_in_a_draft():
    ir = ObservationIR.from_public(frame(), spec())
    with pytest.raises(ValueError, match="candidate decision actor"):
        TypedEncoder(spec()).encode(ir, [{"type": "draftPick", "color": "black",
                                          "cardInstanceId": "visible-draft-choice"}])
