"""E2E orchestration boundaries; native source behavior is tested elsewhere."""
from types import SimpleNamespace
import json

import pytest
import torch

from accelerate_chess.bench import e2e
from accelerate_chess.bench.e2e import _arena_assignment, _terminal_paths


def test_arena_color_pair_and_terminal_gate():
    assert _arena_assignment(0) == {"white": "trained", "black": "baseline"}
    assert _arena_assignment(1) == {"white": "baseline", "black": "trained"}
    assert _arena_assignment(2) == _arena_assignment(0)
    with pytest.raises(ValueError):
        _arena_assignment(-1)
    items = [{"path": "unfinished.json"}, {"path": "terminal.json"}]
    episodes = [SimpleNamespace(outcome={"status": "unfinished"}),
                SimpleNamespace(outcome={"status": "terminal"})]
    assert _terminal_paths(items, episodes) == ["terminal.json"]
    assert _terminal_paths(items[:1], episodes[:1]) == []
    with pytest.raises(ValueError):
        _terminal_paths(items, episodes[:1])


def test_nonterminal_receipt_blocks_training(tmp_path, monkeypatch, capsys):
    spec = SimpleNamespace(rules_version="source-v7", catalog_hash="a" * 64, digest="b" * 64)
    monkeypatch.setattr(e2e, "artifact_root", lambda path: tmp_path)
    monkeypatch.setattr(e2e.cli, "default_spec", lambda **kwargs: spec)
    monkeypatch.setattr(torch.cuda, "is_available", lambda: False)
    def bootstrap(args, expected_spec, directory):
        path = directory / "sample.json"
        path.write_text("{}", encoding="utf-8")
        return path
    def initialize(args, root, expected_spec):
        path = root / "base.pt"
        path.write_bytes(b"base")
        return {"base": str(path)}
    def export(args, root, expected_spec):
        path = root / "manifest.json"
        path.write_text("{}", encoding="utf-8")
        return {"manifest": str(path), "model_sha256": "c" * 64}
    def selfplay(args, root, expected_spec, cancelled):
        path = root / "episode.json"
        path.write_text("{}", encoding="utf-8")
        return {"episodes": [{"path": str(path)}], "workload": {"search_elapsed_seconds": 0., "candidates": 0}}
    monkeypatch.setattr(e2e, "_bootstrap", bootstrap)
    monkeypatch.setattr(e2e.cli, "initialize", initialize)
    monkeypatch.setattr(e2e, "load_typed_base", lambda *args: (torch.nn.Linear(1, 1), None))
    monkeypatch.setattr(e2e.cli, "export", export)
    monkeypatch.setattr(e2e.cli, "selfplay", selfplay)
    monkeypatch.setattr(e2e.ReplayEpisode, "load", lambda *args: SimpleNamespace(
        outcome={"status": "unfinished"}, decisions=[], trackers={"white": SimpleNamespace(steps=0)}))
    monkeypatch.setattr(e2e.cli, "train", lambda *args, **kwargs: pytest.fail("unfinished replay reached train"))
    assert e2e.main(["--artifact-root", str(tmp_path), "--run-id", "terminal-gate"]) == 2
    capsys.readouterr()
    receipt = json.loads((tmp_path / "reports" / "terminal-gate" / "e2e.json").read_text())
    assert receipt["blocker"] == "no-terminal-replay"
    assert receipt["stages"]["replay-load"]["status"] == "ok"
    assert "training" not in receipt["stages"]
