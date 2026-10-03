"""Contract checks for opt-in probes; no CUDA device required."""
from argparse import ArgumentTypeError, Namespace
import json

import pytest

torch = pytest.importorskip("torch")

from accelerate_chess.bench.common import positive, report, workers
from accelerate_chess.bench.inference import PROFILES, model_config, synthetic_batch
from accelerate_chess.bench.pipeline import run as pipeline_run


def test_finite_args_and_worker_limit(monkeypatch):
    monkeypatch.setattr("os.cpu_count", lambda: 4)
    with pytest.raises(ArgumentTypeError):
        positive("0")
    with pytest.raises(ArgumentTypeError):
        workers("6")
    assert workers("4") == 4


@pytest.mark.parametrize("profile", PROFILES)
def test_synthetic_candidate_padding_uses_model_contract(profile):
    batch = synthetic_batch(2, profile)
    actions, nodes = PROFILES[profile]
    assert batch.candidates[0].shape == (2, actions, nodes, 4)
    assert batch.candidates[7].sum() == 2 * actions * nodes
    model = model_config("resnet-s")
    model._validate(batch)
    assert model(batch)[0].shape == (2, actions)


def test_cpu_import_and_cuda_error(monkeypatch):
    from accelerate_chess.bench.inference import run
    monkeypatch.setattr(torch.cuda, "is_available", lambda: False)
    with pytest.raises(RuntimeError, match="CUDA unavailable"):
        run(Namespace(device="cuda", dtype="fp32", model="resnet-s", seed=1,
                      profiles=["small"], batch_sizes=[1], warmup=1, iterations=1,
                      artifact_root=None, run_id=None))


def test_json_envelope_and_pipeline_unsupported(capsys):
    args = Namespace(workers=1, concurrent_games=2, mcts_simulations=2,
                     max_inference_batch=2, batch_wait_us=100, device="cpu",
                     dtype="fp32", seed=1, artifact_root=None, run_id=None)
    payload = pipeline_run(args)
    printed = json.loads(capsys.readouterr().out)
    assert printed["benchmark_version"] == payload["benchmark_version"]
    assert {"config", "results", "git_sha", "timestamp"} <= printed.keys()
    assert printed["results"]["games_per_hour"]["status"] == "unsupported"
    with pytest.raises(ValueError, match="workers"):
        pipeline_run(Namespace(**{**vars(args), "workers": 1_000_000}))


def test_training_smoke_finishes_one_cpu_step(capsys):
    from accelerate_chess.bench.training import run

    args = Namespace(model="resnet-s", profile="small", steps=1, batch_size=1,
                     device="cpu", dtype="fp32", seed=37, artifact_root=None, run_id=None)
    payload = run(args)
    assert payload["results"]["status"] == "ok"
    assert payload["results"]["steps"] == 1
    assert payload["results"]["peak_vram_bytes"] == 0
    assert json.loads(capsys.readouterr().out)["kind"] == "training"
