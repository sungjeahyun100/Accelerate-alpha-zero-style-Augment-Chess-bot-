"""Structural contracts for the production typed synthetic optimizer probe."""
from argparse import Namespace

import pytest

torch = pytest.importorskip("torch")

from accelerate_chess.bench import typed_training as bench
from accelerate_chess.cli import default_spec


def args(tmp_path, family="entity-transformer", **changes):
    values = dict(model_family=family, profile="small", steps=1, batch_size=1,
                  warmup=1, device="cpu", dtype="fp32", seed=37,
                  torch_threads=1, torch_interop_threads=1,
                  artifact_root=str(tmp_path), run_id=None, transformer_hidden_dim=128)
    values.update(changes)
    return Namespace(**values)


@pytest.mark.parametrize("family", ["mask-resnet", "entity-transformer"])
def test_typed_cpu_smoke_and_counts(family, tmp_path, monkeypatch, capsys):
    monkeypatch.setattr(bench, "artifact_root", lambda path: tmp_path)
    calls = []
    original = bench.validate_fp32_training_state

    def checked(model, optimizer):
        calls.append(max((int(state["step"]) for state in optimizer.state.values()), default=0))
        original(model, optimizer)

    monkeypatch.setattr(bench, "validate_fp32_training_state", checked)
    payload = bench.run(args(tmp_path, family, steps=2))
    result = payload["results"]
    capsys.readouterr()
    model = bench.model_for(family, len(default_spec(model_family=family).category_vocabulary))
    assert result["parameter_count"] == sum(p.numel() for p in model.parameters())
    assert result["trainable_parameter_count"] == sum(p.numel() for p in model.parameters() if p.requires_grad)
    assert result["parameter_bytes"] == sum(p.numel() * p.element_size() for p in model.parameters())
    assert result["warmup"] == 1 and result["timed_samples"] == 2
    assert calls == [0, 3]
    assert result["torch_threads"] == torch.get_num_threads() == 1
    assert result["torch_interop_threads"] == torch.get_num_interop_threads() == 1
    assert result["status"] == "ok" and result["peak_vram_bytes"] == 0
    assert payload["config"]["transformer_hidden_dim"] == 128
    context_key = "typed" if family == "entity-transformer" else "typed_context"
    assert result["architecture_config"][context_key]["hidden_dim"] == 128


def test_shared_typed_state_projects_to_both_families():
    from accelerate_chess.ir import INPUT_ORDER_A, INPUT_ORDER_B

    vocabulary = len(default_spec(model_family="mask-resnet").category_vocabulary)
    state = bench.synthetic_batch(1, "small", 37, vocabulary)
    assert all(left is right for left, right in zip(
        state.as_family_inputs("mask-resnet")[2:], state.as_family_inputs("entity-transformer")))
    for family, order in (("mask-resnet", INPUT_ORDER_A), ("entity-transformer", INPUT_ORDER_B)):
        model = bench.model_for(family, vocabulary)
        inputs = tuple(torch.from_numpy(state.inputs[name]) for name in order)
        model.validate_inputs(*inputs)
        logits, value = model(*inputs)
        assert logits.shape == (1, 32) and value.shape == (1, 1)


@pytest.mark.parametrize("hidden_dim", [128, 208])
def test_transformer_hidden_dim_model_and_report(hidden_dim, tmp_path, monkeypatch, capsys):
    monkeypatch.setattr(bench, "artifact_root", lambda path: tmp_path)
    vocabulary = len(default_spec(model_family="entity-transformer").category_vocabulary)
    model = bench.model_for("entity-transformer", vocabulary, hidden_dim)
    assert model.config.typed.hidden_dim == hidden_dim
    payload = bench.run(args(tmp_path, transformer_hidden_dim=hidden_dim, warmup=0))
    capsys.readouterr()
    assert payload["config"]["transformer_hidden_dim"] == hidden_dim
    result = payload["results"]
    assert result["architecture_config"]["typed"]["hidden_dim"] == hidden_dim
    assert result["parameter_count"] == sum(p.numel() for p in model.parameters())
    assert result["trainable_parameter_count"] == sum(
        p.numel() for p in model.parameters() if p.requires_grad)
    assert result["parameter_bytes"] == sum(
        p.numel() * p.element_size() for p in model.parameters())


@pytest.mark.parametrize("hidden_dim", [0, -4, 127, 4097])
def test_invalid_transformer_hidden_dim_rejected(hidden_dim):
    vocabulary = len(default_spec(model_family="entity-transformer").category_vocabulary)
    with pytest.raises(ValueError):
        bench.model_for("entity-transformer", vocabulary, hidden_dim)


def test_transformer_hidden_dim_obeys_parameter_budget():
    vocabulary = len(default_spec(model_family="entity-transformer").category_vocabulary)
    with pytest.raises(ValueError, match="64 million"):
        bench.model_for("entity-transformer", vocabulary, 4096)


def test_mask_resnet_rejects_nondefault_transformer_hidden_dim(tmp_path, monkeypatch):
    monkeypatch.setattr(bench, "artifact_root", lambda path: tmp_path)
    with pytest.raises(ValueError, match="applies only to entity-transformer"):
        bench.run(args(tmp_path, family="mask-resnet", transformer_hidden_dim=208))


@pytest.mark.parametrize("extra_args, expected", [([], 128), (["--transformer-hidden-dim", "208"], 208)])
def test_transformer_hidden_dim_cli(monkeypatch, extra_args, expected):
    captured = []
    monkeypatch.setattr(bench, "run", lambda parsed: captured.append(parsed.transformer_hidden_dim))
    monkeypatch.setattr("sys.argv", ["typed_training", "--model-family", "entity-transformer", *extra_args])
    bench.main()
    assert captured == [expected]


def test_typed_report_kind_is_separate(tmp_path, monkeypatch, capsys):
    from pathlib import Path
    from accelerate_chess.bench.report import _validate

    monkeypatch.setattr(bench, "artifact_root", lambda path: tmp_path)
    payload = bench.run(args(tmp_path, warmup=0))
    capsys.readouterr()
    kind, rows, _ = _validate(payload, "typed-check", Path("reports/typed-check/typed-training.json"))
    assert kind == "typed-training" and rows[0]["parameter_count"] > 0
    with pytest.raises(ValueError, match="mismatched benchmark kind"):
        _validate(payload, "typed-check", Path("reports/typed-check/training.json"))


def test_checkpoint_state_checked_before_save(tmp_path, monkeypatch, capsys):
    monkeypatch.setattr(bench, "artifact_root", lambda path: tmp_path)
    calls = []
    original = bench.validate_fp32_training_state

    def checked(model, optimizer):
        calls.append("validate")
        original(model, optimizer)

    def saved(payload, path):
        calls.append("save")
        assert payload["format"] == "typed-training-benchmark-smoke-v1"
        assert payload["synthetic"] is True

    monkeypatch.setattr(bench, "validate_fp32_training_state", checked)
    monkeypatch.setattr(torch, "save", saved)
    monkeypatch.setattr(bench, "report", lambda *a, **kw: None)
    bench.run(args(tmp_path, run_id="typed-check"))
    capsys.readouterr()
    assert calls == ["validate", "validate", "validate", "save"]


@pytest.mark.parametrize("change", [
    {"model_family": "bad"}, {"profile": "bad"}, {"steps": 0},
    {"batch_size": 65}, {"warmup": 21}, {"torch_threads": 0},
    {"torch_interop_threads": 10000}, {"dtype": "fp16"},
])
def test_invalid_config_rejected(tmp_path, change):
    with pytest.raises(ValueError):
        bench.run(args(tmp_path, **change))


def test_source_artifact_root_rejected():
    from pathlib import Path
    source = Path(__file__).resolve().parents[3]
    with pytest.raises(ValueError, match="outside the source checkout"):
        bench.run(args(source, artifact_root=str(source)))


@pytest.mark.skipif(not torch.cuda.is_available() or not torch.cuda.is_bf16_supported(),
                    reason="CUDA BF16 hardware is optional")
def test_cuda_bf16_smoke(tmp_path, monkeypatch, capsys):
    monkeypatch.setattr(bench, "artifact_root", lambda path: tmp_path)
    result = bench.run(args(tmp_path, device="cuda", dtype="bf16"))["results"]
    capsys.readouterr()
    assert result["status"] == "ok" and result["peak_vram_bytes"] > 0
