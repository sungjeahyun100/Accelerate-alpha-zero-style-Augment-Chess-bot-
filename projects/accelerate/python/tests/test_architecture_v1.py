"""Public semantic projection and common candidate output contract."""

from dataclasses import replace

import pytest

torch = pytest.importorskip("torch")

from accelerate_chess.architecture import ArchitectureSpec, EntityTokenEncoder, Fixed8x8SpatialEncoder
from accelerate_chess.ir import BoardGeometry, ObservationIR, SYNTHETIC_OBSERVATION_VERSION, TypedEncoder
from accelerate_chess.network.architecture_v1 import (
    EntityTokenTransformer, Fixed8x8ResNet, batch_projected_positions,
)
from accelerate_chess.network.typed_context import TypedContextConfig
from test_ir import frame, intents, spec


def _context():
    size = len(spec().category_vocabulary)
    return TypedContextConfig((size,) * 4, (size,) * 2, (size,) * 4, hidden_dim=16)


def _synthetic(width=8, height=8):
    contract = replace(spec(), observation_version=SYNTHETIC_OBSERVATION_VERSION)
    board = [[None] * width for _ in range(height)]
    piece = {"type": "pawn", "color": "white", "anchorRow": 2, "anchorCol": 2,
             "status": {"witchTrial": True}}
    board[2][2] = piece
    board[2][3] = piece
    kinds = [["empty"] * width for _ in range(height)]
    kinds[2][2] = kinds[2][3] = "piece"
    return ObservationIR.from_components(spec=contract, geometry=BoardGeometry(0, 0, height, width),
                                         viewer="white", turn="white", board=board, cell_kinds=kinds,
                                         own_cards=[{"id": "slime", "instanceId": "public-card", "used": False}],
                                         public_state={"actionsRemaining": 1, "boardMarks": [
                                             {"kind": "fogHidden", "square": {"row": 1, "col": 1}}]})


def test_fixed_spatial_and_entity_projections_preserve_one_piece_and_public_state():
    ir = _synthetic()
    encoder = Fixed8x8SpatialEncoder(TypedEncoder(replace(spec(), observation_version=SYNTHETIC_OBSERVATION_VERSION)))
    position = encoder.encode(ir, [])
    kinds = position.entity_category[:, 0]
    assert (kinds == 1).sum() == 1
    piece = int((kinds == 1).nonzero()[0][0])
    assert position.occupancy[piece, 2, 2] == position.occupancy[piece, 2, 3] == 1
    assert position.entity_numeric[piece, 5] == 2 / 64
    assert (kinds == 2).sum() == 1
    assert (kinds == 5).sum() == 1
    assert position.spatial_state[3, 1, 1] == 1
    assert position.condition.shape == (8,)
    assert encoder.spec.metadata(encoder.typed_encoder, "fixed8-resnet")["value_perspective"] == "observation.viewer"


def test_large_piece_entity_uses_canonical_anchor_independent_of_first_footprint_cell():
    contract = replace(spec(), observation_version=SYNTHETIC_OBSERVATION_VERSION)
    encoder = EntityTokenEncoder(TypedEncoder(contract))
    anchor = (3, 3)

    def project(cells, *, with_anchor=True):
        board = [[None] * 8 for _ in range(8)]
        kinds = [["empty"] * 8 for _ in range(8)]
        piece = {"type": "pawn", "color": "white"}
        if with_anchor:
            piece.update(anchorRow=anchor[0], anchorCol=anchor[1])
        for row, col in cells:
            board[row][col] = piece
            kinds[row][col] = "piece"
        ir = ObservationIR.from_components(spec=contract, geometry=BoardGeometry(0, 0, 8, 8),
                                           viewer="white", turn="white", board=board, cell_kinds=kinds)
        return encoder.encode(ir, [])

    footprints = (((2, 2), (2, 3), (3, 2), (3, 3)),
                  ((2, 3), (2, 4), (3, 3), (3, 4)))
    for cells in footprints:
        projected = project(cells)
        pieces = (projected.entity_category[:, 0] == 1).nonzero()[0]
        assert len(pieces) == 1
        index = int(pieces[0])
        assert projected.entity_coord[index].tolist() == pytest.approx([3 / 7, 3 / 7])
        assert projected.entity_numeric[index, 3:5].tolist() == pytest.approx([3 / 7, 3 / 7])
        assert projected.occupancy[index].sum() == 4
        assert all(projected.occupancy[index, row, col] == 1 for row, col in cells)
    single = project(((4, 5),), with_anchor=False)
    single_index = int((single.entity_category[:, 0] == 1).nonzero()[0][0])
    assert single.entity_coord[single_index].tolist() == pytest.approx([4 / 7, 5 / 7])


@pytest.mark.parametrize("anchor", ({"anchorRow": 3}, {"anchorRow": 99, "anchorCol": 3}))
def test_malformed_or_out_of_bounds_piece_anchor_is_rejected(anchor):
    contract = replace(spec(), observation_version=SYNTHETIC_OBSERVATION_VERSION)
    board = [[None] * 8 for _ in range(8)]
    kinds = [["empty"] * 8 for _ in range(8)]
    board[2][2] = {"type": "pawn", "color": "white", **anchor}
    kinds[2][2] = "piece"
    ir = ObservationIR.from_components(spec=contract, geometry=BoardGeometry(0, 0, 8, 8),
                                       viewer="white", turn="white", board=board, cell_kinds=kinds)
    with pytest.raises(ValueError, match="anchor|outside geometry"):
        EntityTokenEncoder(TypedEncoder(contract)).encode(ir, [])


def test_fixed8_rejects_other_geometry_while_entity_projection_accepts_it():
    ir = _synthetic(7, 6)
    encoder = TypedEncoder(replace(spec(), observation_version=SYNTHETIC_OBSERVATION_VERSION))
    with pytest.raises(ValueError, match="8x8"):
        Fixed8x8SpatialEncoder(encoder).encode(ir, [])
    assert EntityTokenEncoder(encoder).encode(ir, []).occupancy.shape[-2:] == (6, 7)
    mixed = batch_projected_positions([EntityTokenEncoder(encoder).encode(_synthetic(), []),
                                       EntityTokenEncoder(encoder).encode(ir, [])])
    assert mixed.occupancy.shape[-2:] == (8, 8)
    model = Fixed8x8ResNet(_context(), channels=16, residual_blocks=1, lora_rank=2, lora_alpha=2.).eval()
    with pytest.raises(ValueError, match="8x8"):
        model.evaluate(mixed)


def test_both_models_score_identical_public_candidates_and_batch_padding():
    torch.set_num_threads(1)
    encoder = Fixed8x8SpatialEncoder(TypedEncoder(spec()))
    public = ObservationIR.from_public(frame(), spec())
    one = encoder.encode(public, intents())
    two = encoder.encode(public, intents()[:1])
    batch = batch_projected_positions([one, two])
    assert batch.candidates[-1].tolist() == [[True, True], [True, False]]
    assert batch.entity_mask.shape[0] == 2
    resnet = Fixed8x8ResNet(_context(), channels=16, residual_blocks=1, lora_rank=2, lora_alpha=2.).eval()
    transformer = EntityTokenTransformer(_context(), blocks=1, heads=2, ffn_dim=32,
                                         lora_rank=2, lora_alpha=2.).eval()
    for model in (resnet, transformer):
        logits, value = model.evaluate(batch)
        assert logits.shape == (2, 2) and value.shape == (2, 1)
        assert torch.isfinite(logits).all() and torch.isfinite(value).all()
        assert logits[1, 1] == -1.0e9
        assert (value.abs() <= 1).all()
    synthetic_encoder = TypedEncoder(replace(spec(), observation_version=SYNTHETIC_OBSERVATION_VERSION))
    with pytest.raises(ValueError, match="8x8"):
        resnet.evaluate(batch_projected_positions([EntityTokenEncoder(synthetic_encoder).encode(
            _synthetic(7, 6), [])]))


def test_identity_fields_do_not_become_entity_features():
    public = frame()
    first = Fixed8x8SpatialEncoder(TypedEncoder(spec())).encode(ObservationIR.from_public(public, spec()), intents())
    public["ownCards"][0]["instanceId"] = "another-public-reference"
    public["publicState"]["legalHints"]["cardTargets"][0]["cardInstanceId"] = "another-public-reference"
    public["informationStateKey"] = ""
    from test_ir import signed
    signed(public)
    actions = [intents()[0], {**intents()[1], "cardInstanceId": "another-public-reference"}]
    second = Fixed8x8SpatialEncoder(TypedEncoder(spec())).encode(ObservationIR.from_public(public, spec()), actions)
    assert (first.entity_category == second.entity_category).all()
    assert (first.entity_numeric == second.entity_numeric).all()
    assert (first.occupancy == second.occupancy).all()


def test_private_state_is_rejected_and_architecture_hash_is_distinct():
    contract = replace(spec(), observation_version=SYNTHETIC_OBSERVATION_VERSION)
    with pytest.raises(ValueError, match="private field"):
        ObservationIR.from_components(spec=contract, geometry=BoardGeometry(0, 0, 8, 8),
                                      viewer="white", turn="white", board=[[None] * 8 for _ in range(8)],
                                      cell_kinds=[["empty"] * 8 for _ in range(8)],
                                      public_state={"rng": 42})
    encoder = TypedEncoder(spec())
    architecture = ArchitectureSpec()
    assert architecture.digest(encoder, "fixed8-resnet") != architecture.digest(encoder, "entity-token-transformer")
    assert architecture.digest(encoder, "fixed8-resnet") != encoder.spec.digest


def test_portal_entity_and_relation_indexes_are_bounded():
    public = frame()
    public["publicState"]["boardMarks"].append({"kind": "portal", "square": {"row": 6, "col": 1}})
    from test_ir import signed
    signed(public)
    projected = Fixed8x8SpatialEncoder(TypedEncoder(spec())).encode(
        ObservationIR.from_public(public, spec()), intents()[:1])
    kinds = projected.entity_category[:, 0]
    assert (kinds == 6).sum() == 1
    assert projected.relation_mask.any()
    assert projected.relation_index[projected.relation_mask].max() < len(kinds)
    portal = int((kinds == 6).nonzero()[0][0])
    assert ((projected.relation_index == portal).any(axis=1) & projected.relation_mask).any()
    assert projected.occupancy[:, 6, 1].sum() == 2  # piece and portal share a square


def test_public_history_square_is_an_entity_relation():
    contract = replace(spec(), observation_version=SYNTHETIC_OBSERVATION_VERSION)
    history = {"version": "public-history-summary-v2", "event_count": 1,
               "actor_counts": {"white": 1, "black": 0}, "decision_actor_changes": 1,
               "board_change_count": 1, "recent_events": [{"actor": "white", "nextActor": "black",
               "phase": "play", "board_change_count": 1, "board_change_squares": [[3, 4]],
               "own_card_count": 0, "opponent_card_count": 0, "outcome": None}]}
    ir = ObservationIR.from_components(spec=contract, geometry=BoardGeometry(0, 0, 8, 8),
                                       viewer="white", turn="white", board=[[None] * 8 for _ in range(8)],
                                       cell_kinds=[["empty"] * 8 for _ in range(8)], history_summary=history)
    projected = EntityTokenEncoder(TypedEncoder(contract)).encode(ir, [])
    history_indexes = (projected.entity_category[:, 0] == 8).nonzero()[0]
    assert len(history_indexes) == 3  # summary, event, changed square
    square_index = int(history_indexes[-1])
    assert projected.entity_coord[square_index].tolist() == pytest.approx([3 / 7, 4 / 7])
    assert ((projected.relation_index == square_index).any(axis=1) & projected.relation_mask).any()


def test_global_condition_and_terrain_share_the_semantic_model_input():
    torch.set_num_threads(1)
    encoder = Fixed8x8SpatialEncoder(TypedEncoder(spec()))
    position = encoder.encode(ObservationIR.from_public(frame(), spec()), intents()[:1])
    batch = batch_projected_positions([position])
    assert batch.spatial_state[0, 3, 2, 2] == 1  # public fog mark
    assert bool(batch.occupancy.any())
    altered = replace(batch, condition=batch.condition + 1)
    resnet = Fixed8x8ResNet(_context(), channels=16, residual_blocks=1, lora_rank=2, lora_alpha=2.).eval()
    transformer = EntityTokenTransformer(_context(), blocks=1, heads=2, ffn_dim=32,
                                         lora_rank=2, lora_alpha=2.).eval()
    with torch.no_grad():
        transformer.blocks[0].film.weight[:, 0] = .01
    for model in (resnet, transformer):
        before = model.evaluate(batch)
        after = model.evaluate(altered)
        assert not torch.allclose(before[1], after[1])


def test_new_models_keep_static_lora_separate_from_film():
    torch.set_num_threads(1)
    encoder = Fixed8x8SpatialEncoder(TypedEncoder(spec()))
    batch = batch_projected_positions([encoder.encode(ObservationIR.from_public(frame(), spec()), intents()[:1])])
    models = (Fixed8x8ResNet(_context(), channels=16, residual_blocks=1, lora_rank=2, lora_alpha=2.),
              EntityTokenTransformer(_context(), blocks=1, heads=2, ffn_dim=32, lora_rank=2, lora_alpha=2.))
    for model in models:
        model.eval().evaluate(batch)
        assert model.adapter_state() and model.base_state()
        with torch.no_grad():
            if isinstance(model, Fixed8x8ResNet):
                module = model.blocks[0].conv1
                probe = torch.randn(1, 16, 8, 8)
            else:
                module = model.blocks[0].attention.q
                probe = torch.randn(1, 2, 16)
            baseline = module(probe)
            module.lora_b.fill_(.1)
        model.configure_training("adapter")
        assert all("lora_" in name for name, p in model.named_parameters() if p.requires_grad)
        assert any("film" in name for name, p in model.named_parameters() if not p.requires_grad)
        model.eval().evaluate(batch)
        with torch.no_grad():
            assert not torch.allclose(baseline, module(probe))


def test_resnet_adapter_step_preserves_all_base_weights_and_batchnorm_buffers():
    torch.set_num_threads(1)
    torch.manual_seed(317)
    encoder = Fixed8x8SpatialEncoder(TypedEncoder(spec()))
    batch = batch_projected_positions([encoder.encode(ObservationIR.from_public(frame(), spec()), intents()[:1])])
    model = Fixed8x8ResNet(_context(), channels=16, residual_blocks=1, lora_rank=2, lora_alpha=2.)
    bn = [module for module in model.modules() if isinstance(module, torch.nn.BatchNorm2d)]
    assert bn and all(module.training for module in bn)
    model.train()
    model(batch)
    assert all(int(module.num_batches_tracked) == 1 for module in bn)

    base_before = {name: tensor.detach().clone() for name, tensor in model.base_state().items()}
    adapter_before = {name: tensor.detach().clone() for name, tensor in model.adapter_state().items()}
    bn_before = {name: tensor.detach().clone() for name, tensor in model.named_buffers()
                 if name.endswith(("running_mean", "running_var", "num_batches_tracked"))}

    optimizer = torch.optim.SGD(model.configure_training("adapter"), lr=0.1)
    model.train()  # A later caller must not re-enable BatchNorm updates.
    assert all(not module.training for module in bn)
    optimizer.zero_grad()
    logits, value = model(batch)
    (logits.sum() + value.sum()).backward()
    optimizer.step()

    assert any(not torch.equal(adapter_before[name], tensor) for name, tensor in model.adapter_state().items())
    assert all(torch.equal(base_before[name], tensor) for name, tensor in model.base_state().items())
    assert set(bn_before) == {name for name, tensor in model.named_buffers()
                              if name.endswith(("running_mean", "running_var", "num_batches_tracked"))}
    assert all(torch.equal(bn_before[name], tensor) for name, tensor in model.named_buffers() if name in bn_before)

    model.configure_training("base")
    model.train()
    assert all(module.training for module in bn)
