"""Stage target tests with a fake source adapter; source legality stays native."""
from types import SimpleNamespace
from unittest.mock import patch

import pytest

from accelerate_chess import staged


class Position:
    def __init__(self, step=0, terminal=None):
        self.step = step
        self.result = terminal
        self.snapshot_revision = str(step)
        self.decision_actor = ("white", "black", "white", "black")[step]

    def observe(self, viewer):
        phase = "END" if self.step == 1 else None
        return {"viewer": viewer, "publicState": {"mode": "draft" if phase else "play",
                "draft": {"phase": phase} if phase else None}}

    def bind_public_intent(self, intent):
        return intent

    def apply(self, intent):
        return SimpleNamespace(position=Position(self.step + 1))


class Tracker:
    def __init__(self, frame):
        self.latest = frame

    def append(self, frame, **_):
        self.latest = frame


class Search:
    def __init__(self):
        self.encoder = SimpleNamespace(spec=SimpleNamespace(digest="spec"))
        self.evaluator = SimpleNamespace(architecture_family="mask-resnet")

    def run(self, *_args, **_kwargs):
        return SimpleNamespace(intent={"type": "move"}, policy=({"intent": {"type": "move"},
            "visits": 2, "probability": 1.0},), belief_summary=None, nodes=3,
            stop_reason="iterations")


def test_terminal_before_boundary_never_calls_teacher():
    position = Position(terminal="white")
    trackers = {viewer: Tracker(position.observe(viewer)) for viewer in ("white", "black")}
    teacher = SimpleNamespace(spec=SimpleNamespace(digest="spec"),
                              architecture_family="mask-resnet")
    with patch.object(staged, "NativeSourceFactory"), patch.object(staged, "ParticleBelief"), \
         patch.object(staged, "teacher_value") as value, patch.object(staged, "statistics", return_value={}):
        result = staged.rollout_stage(position, trackers, staged.MIDDLE, Search(), teacher,
                                      {"gameStyle": "normal"})
    assert result["outcome"] == "terminal"
    value.assert_not_called()


def test_bootstrap_waits_for_complete_draft_and_uses_viewer_sign():
    position = Position()
    trackers = {viewer: Tracker(position.observe(viewer)) for viewer in ("white", "black")}
    teacher = SimpleNamespace(spec=SimpleNamespace(digest="spec"),
                              architecture_family="mask-resnet",
                              session=SimpleNamespace(model_sha256="a" * 64))
    with patch.object(staged, "NativeSourceFactory"), patch.object(staged, "ParticleBelief"), \
         patch.object(staged, "teacher_value", return_value=("white", .4)) as value, \
         patch.object(staged, "statistics", return_value={}):
        result = staged.rollout_stage(position, trackers, staged.MIDDLE, Search(), teacher,
                                      {"gameStyle": "normal"})
    assert result["outcome"] == "bootstrap:end"
    assert result["samples"][0]["value"] == pytest.approx(.4)
    assert result["samples"][0]["teacher_checkpoint_sha256"] == "a" * 64
    assert len(result["boundary_actions"]) == 1
    assert value.call_args.args[0].step == 2


def test_turn_free_action_does_not_determine_value_sign_by_action_count():
    position = Position()
    position.decision_actor = "black"
    trackers = {viewer: Tracker(position.observe(viewer)) for viewer in ("white", "black")}
    teacher = SimpleNamespace(spec=SimpleNamespace(digest="spec"),
                              architecture_family="mask-resnet",
                              session=SimpleNamespace(model_sha256="b" * 64))
    with patch.object(staged, "NativeSourceFactory"), patch.object(staged, "ParticleBelief"), \
         patch.object(staged, "teacher_value", return_value=("white", .4)), \
         patch.object(staged, "statistics", return_value={}):
        result = staged.rollout_stage(position, trackers, staged.MIDDLE, Search(), teacher,
                                      {"gameStyle": "normal"})
    assert result["samples"][0]["value"] == pytest.approx(-.4)


def test_teacher_waits_for_post_draft_card_choice():
    class PendingPosition(Position):
        def observe(self, viewer):
            frame = super().observe(viewer)
            if self.step == 2:
                frame["publicState"]["ruleTicketChoice"] = {"color": "white"}
            return frame

        def apply(self, intent):
            return SimpleNamespace(position=PendingPosition(self.step + 1))

    position = PendingPosition()
    trackers = {viewer: Tracker(position.observe(viewer)) for viewer in ("white", "black")}
    teacher = SimpleNamespace(spec=SimpleNamespace(digest="spec"),
                              architecture_family="mask-resnet",
                              session=SimpleNamespace(model_sha256="a" * 64))
    with patch.object(staged, "NativeSourceFactory"), patch.object(staged, "ParticleBelief"), \
         patch.object(staged, "teacher_value", return_value=("black", .4)) as value, \
         patch.object(staged, "statistics", return_value={}):
        result = staged.rollout_stage(position, trackers, staged.MIDDLE, Search(), teacher,
                                      {"gameStyle": "normal"})
    assert len(result["boundary_actions"]) == 2
    assert value.call_args.args[0].step == 3


def test_synthetic_end_assumption_cannot_precede_31():
    with pytest.raises(ValueError, match="limits"):
        staged.SyntheticConfig(staged.END, 1, assumed_ply_center=30, assumed_ply_jitter=0)


def test_staged_dataset_validates_public_policy_and_frozen_teacher(tmp_path):
    from accelerate_chess.adapter_client import GameAdapterClient
    from accelerate_chess.cli import default_spec
    from accelerate_chess.replay import atomic_json

    spec = default_spec(model_family="mask-resnet")
    position = GameAdapterClient.new_game({"gameStyle": "normal"}, 37, spec=spec)
    actor = position.decision_actor
    sample = {"actor": actor, "observation": position.observe(actor),
              "candidates": [{"intent": position.legal_intents()[0], "visits": 2,
                              "probability": 1.0}], "belief_summary": None, "value": .25,
              "value_target_source": "bootstrap:end",
              "teacher_checkpoint_sha256": "a" * 64}
    payload = {"version": "accelerate-staged-v1", "encoder_hash": spec.digest,
               "architecture_family": "mask-resnet", "provenance": {"source": "synthetic"},
               "teacher_checkpoint_sha256": "a" * 64,
               "teacher_deployment": {"model_sha256": "a" * 64,
                                      "base_hash": "b" * 64, "manifest_sha256": "c" * 64},
               "samples": [sample]}
    path = tmp_path / "staged.json"
    atomic_json(path, payload)
    assert staged.StagedDataset([path], spec, "mask-resnet")[0].value == .25
    payload["samples"][0]["teacher_checkpoint_sha256"] = None
    atomic_json(path, payload)
    with pytest.raises(ValueError, match="teacher"):
        staged.StagedDataset([path], spec, "mask-resnet")
