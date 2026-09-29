"""Meaningful invariants of the typed public entity model."""

from __future__ import annotations

import pytest

torch = pytest.importorskip("torch")

from accelerate_chess.network.entity_transformer import EntityTransformer, EntityTransformerConfig, LoRALinear
from accelerate_chess.network.typed_context import CandidateScorer, TypedContextConfig


def _config() -> EntityTransformerConfig:
    return EntityTransformerConfig(TypedContextConfig((16, 16, 16, 16), (16, 16), (16, 16, 16, 16)))


def _inputs() -> tuple[torch.Tensor, ...]:
    i64 = torch.int64
    f32 = torch.float32
    rc = torch.tensor([[[1, 0, 0, 0], [2, 3, 1, 0], [4, 2, 0, 0], [5, 0, 1, 0]]], dtype=i64)
    rn = torch.zeros((1, 4, 8), dtype=f32)
    rn[0, 1, 0] = 2
    xy = torch.tensor([[[0, 0], [1, 2], [2, 3], [0, 0]]], dtype=f32)
    spatial = torch.tensor([[False, True, True, False]])
    records = torch.ones((1, 4), dtype=torch.bool)
    ri = torch.tensor([[[1, 2], [3, 1]]], dtype=i64)
    relc = torch.tensor([[[1, 2], [3, 0]]], dtype=i64)
    reln = torch.zeros((1, 2, 4), dtype=f32)
    reln[0, 0, 0] = 1
    relations = torch.ones((1, 2), dtype=torch.bool)
    cc = torch.tensor([[[[1, 0, 0, 0], [2, 3, 0, 0], [3, 4, 0, 0]],
                        [[1, 0, 0, 0], [4, 2, 0, 0], [5, 3, 0, 0]]]], dtype=i64)
    cn = torch.zeros((1, 2, 3, 8), dtype=f32)
    cxy = torch.tensor([[[[0, 0], [1, 2], [2, 3]], [[0, 0], [0, 0], [1, 2]]]], dtype=f32)
    cxy_valid = torch.tensor([[[False, True, True], [False, False, True]]])
    parent = torch.tensor([[[-1, 0, 0], [-1, 0, 1]]], dtype=i64)
    order = torch.tensor([[[0, 0, 1], [0, 0, 0]]], dtype=i64)
    target = torch.tensor([[[-1, 1, 2], [-1, -1, 1]]], dtype=i64)
    nodes = torch.ones((1, 2, 3), dtype=torch.bool)
    candidates = torch.ones((1, 2), dtype=torch.bool)
    condition = torch.zeros((1, 8), dtype=f32)
    return (rc, rn, xy, spatial, records, ri, relc, reln, relations,
            cc, cn, cxy, cxy_valid, parent, order, target, nodes, candidates, condition)


def _model() -> EntityTransformer:
    torch.manual_seed(724)
    return EntityTransformer(_config()).eval()


def test_shared_policy_split_preserves_the_original_affine_and_bias():
    config = _config().typed
    scorer = CandidateScorer(config).eval()
    inputs = _inputs()
    state = torch.randn((1, config.hidden_dim))
    records = torch.randn((1, 4, config.hidden_dim))
    fused_nodes = []
    hook = scorer.node_fusion.register_forward_hook(lambda _module, _args, output: fused_nodes.append(output.detach()))
    try:
        logits, _ = scorer(state, records, *inputs[9:18])
    finally:
        hook.remove()
    candidates = fused_nodes[0].mean(dim=2)
    expanded_state = state.unsqueeze(1).expand(-1, candidates.shape[1], -1)
    reference = scorer.policy(torch.cat((expanded_state, candidates), dim=-1)).squeeze(-1)
    torch.testing.assert_close(logits, reference, atol=1e-6, rtol=1e-5)


def test_entity_permutation_and_relation_endpoint_remapping():
    model = _model()
    inputs = list(_inputs())
    original = model.evaluate(*inputs)
    permutation = torch.tensor([0, 2, 1, 3])
    reverse = torch.argsort(permutation)
    for index in (0, 1, 2, 3, 4):
        inputs[index] = inputs[index][:, permutation]
    inputs[5] = reverse[inputs[5]]
    inputs[15] = torch.where(inputs[15] >= 0, reverse[inputs[15].clamp_min(0)], inputs[15])
    moved = model.evaluate(*inputs)
    torch.testing.assert_close(moved[0], original[0], atol=1e-5, rtol=1e-4)
    torch.testing.assert_close(moved[1], original[1], atol=1e-5, rtol=1e-4)


def test_candidate_permutation_split_and_padding_preserve_state_value():
    model = _model()
    inputs = list(_inputs())
    original = model.evaluate(*inputs)
    swapped = list(inputs)
    for index in range(9, 18):
        swapped[index] = swapped[index][:, [1, 0]]
    swapped_result = model.evaluate(*swapped)
    torch.testing.assert_close(swapped_result[0], original[0][:, [1, 0]], atol=1e-5, rtol=1e-4)
    torch.testing.assert_close(swapped_result[1], original[1], atol=1e-5, rtol=1e-4)

    for action in (0, 1):
        split = list(inputs)
        for index in range(9, 18):
            split[index] = split[index][:, action:action + 1]
        logits, value = model.evaluate(*split)
        torch.testing.assert_close(logits[:, 0], original[0][:, action], atol=1e-5, rtol=1e-4)
        torch.testing.assert_close(value, original[1], atol=1e-5, rtol=1e-4)

    padded = list(inputs)
    padded[0] = torch.cat((padded[0], torch.full((1, 1, 4), 999, dtype=torch.int64)), dim=1)
    padded[1] = torch.cat((padded[1], torch.full((1, 1, 8), 7.0)), dim=1)
    padded[2] = torch.cat((padded[2], torch.full((1, 1, 2), 100.0)), dim=1)
    padded[3] = torch.cat((padded[3], torch.zeros((1, 1), dtype=torch.bool)), dim=1)
    padded[4] = torch.cat((padded[4], torch.zeros((1, 1), dtype=torch.bool)), dim=1)
    padded[5] = torch.cat((padded[5], torch.full((1, 1, 2), 999, dtype=torch.int64)), dim=1)
    padded[6] = torch.cat((padded[6], torch.full((1, 1, 2), 999, dtype=torch.int64)), dim=1)
    padded[7] = torch.cat((padded[7], torch.full((1, 1, 4), 7.0)), dim=1)
    padded[8] = torch.cat((padded[8], torch.zeros((1, 1), dtype=torch.bool)), dim=1)
    tail = ((999, torch.int64), (7.0, torch.float32), (100.0, torch.float32),
            (False, torch.bool), (999, torch.int64), (999, torch.int64),
            (999, torch.int64), (False, torch.bool))
    for index, (fill, dtype) in enumerate(tail, start=9):
        shape = list(padded[index].shape)
        shape[2] = 1
        padded[index] = torch.cat((padded[index], torch.full(shape, fill, dtype=dtype)), dim=2)
    padded_result = model.evaluate(*padded)
    torch.testing.assert_close(padded_result[0], original[0], atol=1e-5, rtol=1e-4)
    torch.testing.assert_close(padded_result[1], original[1], atol=1e-5, rtol=1e-4)


def test_condition_relation_and_order_reach_outputs():
    model = _model()
    inputs = list(_inputs())
    with torch.no_grad():
        model.blocks[0].film.weight[:, 0] = 0.01
    baseline = model.evaluate(*inputs)
    conditioned = list(inputs)
    conditioned[18] = torch.ones_like(conditioned[18])
    changed = model.evaluate(*conditioned)
    assert not torch.allclose(baseline[0], changed[0])
    assert not torch.allclose(baseline[1], changed[1])

    related = list(inputs)
    related[6] = related[6].clone()
    related[6][0, 0, 0] = 6
    assert not torch.allclose(baseline[1], model.evaluate(*related)[1])
    reordered = list(inputs)
    reordered[14] = reordered[14].clone()
    reordered[14][0, 0, 2] = 3
    assert not torch.allclose(baseline[0], model.evaluate(*reordered)[0])
    redirected = list(inputs)
    redirected[15] = redirected[15].clone()
    redirected[15][0, 0, 2] = 1
    redirected_logits, redirected_value = model.evaluate(*redirected)
    assert not torch.allclose(baseline[0], redirected_logits)
    torch.testing.assert_close(baseline[1], redirected_value)


def test_nonzero_qv_adapter_merges_without_mutating_source():
    model = _model()
    model.configure_training("adapter")
    assert all(parameter.requires_grad == name.endswith((".lora_a", ".lora_b"))
               for name, parameter in model.named_parameters())
    with torch.no_grad():
        for module in model.modules():
            if isinstance(module, LoRALinear):
                module.lora_b.fill_(0.003)
    model.eval()
    base_hash = model.base_hash
    adapter_before = {name: value.detach().clone() for name, value in model.adapter_state().items()}
    unmerged = model.evaluate(*_inputs())
    merged = model.merged_copy(model.adapter_descriptor("e" * 64), "e" * 64)
    merged_output = merged.evaluate(*_inputs())
    torch.testing.assert_close(unmerged[0], merged_output[0], atol=1e-5, rtol=1e-4)
    torch.testing.assert_close(unmerged[1], merged_output[1], atol=1e-5, rtol=1e-4)
    assert model.base_hash == base_hash
    assert all(torch.equal(value, adapter_before[name]) for name, value in model.adapter_state().items())
    with pytest.raises(ValueError, match="compatibility"):
        model.merged_copy(model.adapter_descriptor("e" * 64), "f" * 64)


def test_adapter_optimizer_updates_only_qv_lora():
    model = _model()
    parameters = list(model.configure_training("adapter"))
    before_base = model.base_hash
    before_adapter = {name: value.detach().clone() for name, value in model.adapter_state().items()}
    optimizer = torch.optim.SGD(parameters, lr=0.1)
    logits, value = model(*_inputs())
    (logits.square().mean() + value.square().mean()).backward()
    optimizer.step()
    assert model.base_hash == before_base
    assert any(not torch.equal(value, before_adapter[name]) for name, value in model.adapter_state().items())


def test_typed_input_boundary_rejects_invalid_relation_and_parent():
    model = _model()
    inputs = list(_inputs())
    inputs[5] = inputs[5].clone()
    inputs[5][0, 0, 1] = 9
    with pytest.raises(ValueError, match="relation endpoint"):
        model.validate_inputs(*inputs)
    inputs = list(_inputs())
    inputs[13] = inputs[13].clone()
    inputs[13][0, 0, 2] = 2
    with pytest.raises(ValueError, match="candidate parent"):
        model.validate_inputs(*inputs)


def test_all_candidates_masked_and_only_global_record_are_finite():
    model = _model()
    inputs = list(_inputs())
    inputs[3][:, 1:] = False
    inputs[4][:, 1:] = False
    inputs[8][:] = False
    inputs[12][:] = False
    inputs[15][:] = -1
    inputs[16][:] = False
    inputs[17][:] = False
    logits, value = model.evaluate(*inputs)
    assert logits.shape == (1, 2) and value.shape == (1, 1)
    assert torch.isfinite(logits).all() and torch.isfinite(value).all()
    assert torch.equal(logits, torch.full_like(logits, -1.0e9))


def test_transformer_rejects_excessive_parameter_config_before_allocation():
    context = TypedContextConfig((1, 1, 1, 1), (1, 1), (1, 1, 1, 1), hidden_dim=4096)
    with pytest.raises(ValueError, match="parameter lower bound"):
        EntityTransformerConfig(context, blocks=64, heads=4, ffn_dim=4096)
