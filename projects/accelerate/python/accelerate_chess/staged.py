"""Source-backed, bounded stage experiments; no GameState mutation."""
from __future__ import annotations

from dataclasses import dataclass
from hashlib import sha256
import math
from pathlib import Path
import random
import time
from typing import Any, Mapping

import numpy as np

from .encoding import canonical_json
from .ir import ObservationIR, TypedEncoder, batch_typed_positions
from .replay import TrainingExample, read_json
from .search import BeliefLimits, NativeSourceFactory, ParticleBelief, PublicTracker


def _sha256_text(value):
    return isinstance(value, str) and len(value) == 64 and all(
        digit in "0123456789abcdef" for digit in value)


@dataclass(frozen=True)
class StageDefinition:
    name: str
    entry_draft: str
    next_draft: str | None
    max_actions: int = 128

    def __post_init__(self):
        if (not self.name or not self.name.replace("_", "").isalnum()
                or self.entry_draft not in ("MIDDLE", "END")
                or self.next_draft not in (None, "MIDDLE", "END")
                or self.next_draft == self.entry_draft
                or not 1 <= self.max_actions <= 4096):
            raise ValueError("unsupported or unbounded source stage")


MIDDLE = StageDefinition("middle", "MIDDLE", "END")
END = StageDefinition("end", "END", None)


@dataclass(frozen=True)
class SyntheticConfig:
    stage: StageDefinition
    seed: int
    max_actions: int = 256
    capture_bias: float = 2.0
    assumed_ply_center: int = 45
    assumed_ply_jitter: int = 5

    def __post_init__(self):
        if (not 0 <= self.seed < 2**32 or not 1 <= self.max_actions <= 4096
                or not math.isfinite(self.capture_bias) or not 1 <= self.capture_bias <= 100
                or self.assumed_ply_jitter < 0
                or self.stage.entry_draft == "END" and self.assumed_ply_center - self.assumed_ply_jitter < 31):
            raise ValueError("invalid synthetic generation limits")


def draft_phase(position) -> str | None:
    public = position.observe(position.decision_actor)["publicState"]
    draft = public.get("draft")
    if public.get("mode") != "draft" or not isinstance(draft, dict):
        return None
    phase = draft.get("phase")
    if phase not in ("OPENING", "MIDDLE", "END"):
        raise ValueError("unsupported source draft phase")
    return phase


def pending_card_resolution(position) -> bool:
    """Source decision windows that can remain after draft mode closes."""
    public = position.observe(position.decision_actor)["publicState"]
    return any(bool(public.get(name))
               for name in ("pendingPromotion", "activeTrolley", "ruleTicketChoice",
                            "jokerChoice", "barricadeDirectionChoice"))


def _apply(position, intent):
    return position.apply(position.bind_public_intent(intent)).position


def _counts(observation):
    counts = {"white": 0, "black": 0}
    for row in observation["board"]:
        for piece in row:
            if piece is not None and piece.get("color") in counts:
                counts[piece["color"]] += 1
    return counts


def statistics(position):
    observation = position.observe(position.decision_actor)
    histogram = {}
    kings = {"white": [], "black": []}
    for row_index, row in enumerate(observation["board"]):
        for col, piece in enumerate(row):
            if piece is None:
                continue
            color, kind = piece.get("color"), piece.get("type")
            key = f"{color}:{kind}"
            histogram[key] = histogram.get(key, 0) + 1
            if kind == "king" and color in kings:
                kings[color].append([row_index, col])
    public = observation["publicState"]
    return {"source": "synthetic", "piece_count": _counts(observation), "piece_types": histogram,
            "king_locations": kings, "move_count": public.get("moveCount"),
            "turns_taken": public.get("turnsTaken"), "draft_phase": draft_phase(position),
            "legal_action_count": len(position.legal_intents()) if position.result is None else 0,
            "terminal": position.result is not None}


def generate_position(config: SyntheticConfig, spec, *, game_style="normal", search=None,
                      prefix_provenance=None):
    """Reach a real completed draft via admitted actions and record every step."""
    from .adapter_client import GameAdapterClient

    if game_style not in ("normal", "chaos"):
        raise ValueError("only Basic and Chaos source games are supported")
    if prefix_provenance is None:
        rng = random.Random(config.seed)
        position = GameAdapterClient.new_game({"gameStyle": game_style}, config.seed, spec=spec)
        trackers = {viewer: PublicTracker(position.observe(viewer), typed_spec=spec)
                    for viewer in ("white", "black")}
        history = []
    else:
        if (prefix_provenance.get("stage") != "middle"
                or config.stage.entry_draft != "END"
                or prefix_provenance.get("seed") != config.seed
                or prefix_provenance.get("format") != game_style):
            raise ValueError("end generation requires a matching completed middle predecessor")
        position, trackers = replay_position(prefix_provenance, spec)
        rng = random.Random(config.seed ^ 0xE0D0)
        history = list(prefix_provenance["actions"])
    if search is None or search.encoder.spec.digest != spec.digest:
        raise ValueError("synthetic draft decisions require a compatible public search policy")
    candidate_probes = 0
    entered = False
    for _ in range(config.max_actions):
        if position.result is not None:
            raise ValueError("source game terminated before the requested stage")
        phase = draft_phase(position)
        entered |= phase == config.stage.entry_draft
        if entered and phase is None and not pending_card_resolution(position):
            break
        intents = position.legal_intents()
        if not intents:
            raise ValueError("nonterminal source state has no admitted actions")
        pending = pending_card_resolution(position)
        if phase is None and not pending and config.capture_bias > 1:
            # Favor captures only after verifying candidate source transitions.
            candidates = rng.sample(intents, min(12, len(intents)))
            before = _counts(position.observe(position.decision_actor))
            weights = []
            for intent in candidates:
                child = _apply(position, intent)
                candidate_probes += 1
                after = _counts(child.observe(child.decision_actor))
                captured = sum(max(0, before[color] - after[color]) for color in before)
                weights.append(config.capture_bias if captured else 1.0)
            selected = rng.choices(candidates, weights=weights, k=1)[0]
        elif phase is not None or pending:
            # Card choice is a model decision over the source action space.
            # Source chance effects are settled by the native apply transaction.
            selected = choose_public_decision(position, intents, search.evaluator, spec)
        else:
            selected = rng.choice(intents)
        source_id = position.snapshot_revision
        actor = position.decision_actor
        position = _apply(position, selected)
        for viewer, tracker in trackers.items():
            tracker.append(position.observe(viewer), own_intent=selected if viewer == actor else None)
        history.append({"source_position_id": source_id, "intent": selected,
                        "generated_position_id": position.snapshot_revision, "draft_phase": phase})
    else:
        raise ValueError("source stage was not reached within the generation budget")
    if not entered or draft_phase(position) is not None or pending_card_resolution(position):
        raise ValueError("requested source draft is incomplete")
    public = position.observe(position.decision_actor)["publicState"]
    assumed_ply = config.assumed_ply_center + rng.randint(-config.assumed_ply_jitter,
                                                           config.assumed_ply_jitter)
    return position, trackers, {"source": "synthetic", "seed": config.seed, "format": game_style,
                      "encoder_hash": spec.digest,
                      "policy_model_sha256": search.evaluator.session.model_sha256,
                      "generation_config": {"max_actions": config.max_actions,
                                            "capture_bias": config.capture_bias,
                                            "assumed_ply_center": config.assumed_ply_center,
                                            "assumed_ply_jitter": config.assumed_ply_jitter,
                                            "prefix_stage": None if prefix_provenance is None else "middle"},
                      "stage": config.stage.name, "assumed_ply": assumed_ply,
                      "actual_move_count": public.get("moveCount"),
                      "source_position_id": history[0]["source_position_id"],
                      "generated_position_id": position.snapshot_revision,
                      "candidate_probe_transitions": candidate_probes,
                      "selected_draft_intents": [step["intent"] for step in history
                                                 if step["draft_phase"] is not None],
                      "card_action_intents": [step["intent"] for step in history
                                              if step["draft_phase"] is None
                                              and step["intent"].get("type") == "card"],
                      "actions": history, "statistics": statistics(position)}


def replay_position(provenance, spec):
    """Recreate a generated state by verifying every recorded source transition."""
    from .adapter_client import GameAdapterClient

    if (not isinstance(provenance, dict) or provenance.get("source") != "synthetic"
            or provenance.get("format") not in ("normal", "chaos")
            or type(provenance.get("seed")) is not int
            or not 0 <= provenance["seed"] < 2**32
            or provenance.get("encoder_hash") not in (None, spec.digest)
            or not isinstance(provenance.get("actions"), list)
            or not 1 <= len(provenance["actions"]) <= 4096):
        raise ValueError("invalid synthetic source provenance")
    position = GameAdapterClient.new_game({"gameStyle": provenance["format"]},
                                          provenance["seed"], spec=spec)
    trackers = {viewer: PublicTracker(position.observe(viewer), typed_spec=spec)
                for viewer in ("white", "black")}
    for step in provenance["actions"]:
        if (position.snapshot_revision != step["source_position_id"]
                or draft_phase(position) != step["draft_phase"]):
            raise ValueError("synthetic replay source identity or phase changed")
        actor = position.decision_actor
        position = _apply(position, step["intent"])
        if position.snapshot_revision != step["generated_position_id"]:
            raise ValueError("synthetic replay transition differs from recorded source")
        for viewer, tracker in trackers.items():
            tracker.append(position.observe(viewer),
                           own_intent=step["intent"] if viewer == actor else None)
    if (position.snapshot_revision != provenance["generated_position_id"]
            or statistics(position) != provenance["statistics"]):
        raise ValueError("synthetic replay final state differs from recorded source")
    return position, trackers


def teacher_value(position, teacher, spec):
    """Evaluate the completed next-stage public view, never its private state."""
    viewer = position.decision_actor
    intents = position.legal_intents()
    if not intents:
        raise ValueError("nonterminal teacher state has no admitted actions")
    encoded = TypedEncoder(spec).encode(ObservationIR.from_public(position.observe(viewer), spec), intents)
    batch = batch_typed_positions([encoded])
    order = spec.feature_schema["input_order"][teacher.architecture_family]
    inputs = dict(zip(order, batch.as_family_inputs(teacher.architecture_family), strict=True))
    _, values = teacher.evaluate_typed(inputs)
    if values.shape != (1, 1) or not np.isfinite(values).all() or abs(float(values[0, 0])) > 1.00001:
        raise ValueError("teacher returned an invalid value")
    return viewer, float(values[0, 0])


def choose_public_decision(position, intents, evaluator, spec):
    """Deterministic policy choice at a real draft decision node."""
    viewer = position.decision_actor
    encoded = TypedEncoder(spec).encode(ObservationIR.from_public(position.observe(viewer), spec), intents)
    batch = batch_typed_positions([encoded])
    order = spec.feature_schema["input_order"][evaluator.architecture_family]
    inputs = dict(zip(order, batch.as_family_inputs(evaluator.architecture_family), strict=True))
    logits, values = evaluator.evaluate_typed(inputs)
    if logits.shape != (1, len(intents)) or values.shape != (1, 1) or not np.isfinite(logits).all():
        raise ValueError("draft policy evaluator returned invalid scores")
    return intents[int(np.argmax(logits[0]))]


def rollout_stage(position, trackers, stage, search, teacher, config: Mapping[str, Any], *,
                  belief_seed=71, belief_ms=30000, particles=1, proposals=4,
                  cancelled=lambda: False):
    """Record root visits, settle the real next draft, then freeze one target."""
    if (stage.next_draft is None) != (teacher is None):
        raise ValueError("middle needs a teacher; end must use a real terminal")
    spec = search.encoder.spec
    if teacher is not None and (teacher.spec.digest != spec.digest
                                or teacher.architecture_family != search.evaluator.architecture_family):
        raise ValueError("student and teacher model input contracts differ")
    if set(trackers) != {"white", "black"} or any(
        trackers[viewer].latest != position.observe(viewer) for viewer in trackers
    ):
        raise ValueError("stage rollout requires a complete source public history")
    belief_started = time.monotonic()
    beliefs = {viewer: ParticleBelief(trackers[viewer], NativeSourceFactory(config, typed_spec=spec),
               seed=belief_seed + index, limits=BeliefLimits(particles=particles,
                   proposals=proposals, elapsed_ms=belief_ms),
               cancelled=cancelled) for index, viewer in enumerate(trackers)}
    belief_initialization_seconds = time.monotonic() - belief_started
    samples, boundary_actions = [], []
    root_belief_seconds = []
    boundary_seen = False
    teacher_seconds = 0.0
    started = time.monotonic()
    for _ in range(stage.max_actions):
        if cancelled():
            raise InterruptedError("stage rollout cancelled")
        if position.result is not None:
            source, winner, teacher_viewer, value = "terminal", position.result, None, None
            break
        phase = draft_phase(position)
        boundary_seen |= phase == stage.next_draft and phase is not None
        if boundary_seen and phase is None and not pending_card_resolution(position):
            teacher_started = time.monotonic()
            teacher_viewer, value = teacher_value(position, teacher, spec)
            teacher_seconds = time.monotonic() - teacher_started
            source, winner = f"bootstrap:{stage.next_draft.lower()}", None
            break
        actor = position.decision_actor
        observation = position.observe(actor)
        if trackers[actor].latest != observation:
            raise ValueError("source public state diverged from rollout tracker")
        root_started = time.monotonic()
        try:
            beliefs[actor].synchronize()
        except Exception as error:
            error.belief_diagnostics = {viewer: belief.diagnostics for viewer, belief in beliefs.items()}
            raise
        root_belief_seconds.append(time.monotonic() - root_started)
        try:
            result = search.run(beliefs[actor], cancelled=cancelled)
        except Exception as error:
            error.belief_diagnostics = {viewer: belief.diagnostics for viewer, belief in beliefs.items()}
            error.belief_initialization_seconds = belief_initialization_seconds
            error.root_belief_seconds = root_belief_seconds
            error.stage_samples_completed = len(samples)
            raise
        if result.stop_reason != "iterations":
            error = ValueError("stage search did not complete its fixed iteration budget")
            error.belief_diagnostics = {viewer: belief.diagnostics for viewer, belief in beliefs.items()}
            error.belief_initialization_seconds = belief_initialization_seconds
            error.root_belief_seconds = root_belief_seconds
            error.stage_samples_completed = len(samples)
            raise error
        if boundary_seen:
            boundary_actions.append(result.intent)
        else:
            samples.append({"actor": actor, "observation": observation,
                            "candidates": [{"intent": item["intent"], "visits": item["visits"],
                                            "probability": item["probability"]} for item in result.policy],
                            "belief_summary": result.belief_summary, "mcts_nodes": result.nodes})
        child = _apply(position, result.intent)
        for viewer, tracker in trackers.items():
            tracker.append(child.observe(viewer), own_intent=result.intent if viewer == actor else None)
        position = child
    else:
        raise ValueError("source terminal or card boundary exceeded stage action budget")
    if source.startswith("bootstrap") and not boundary_actions:
        raise ValueError("source card transition was not settled before bootstrap")
    teacher_hash = teacher.session.model_sha256 if source.startswith("bootstrap") else None
    for sample in samples:
        actor = sample["actor"]
        sample["value"] = (0.0 if winner == "draw" else 1.0 if winner == actor else -1.0) if source == "terminal" else value * (1.0 if actor == teacher_viewer else -1.0)
        sample["value_target_source"] = source
        sample["teacher_checkpoint_sha256"] = teacher_hash
    return {"samples": samples, "outcome": source, "winner": winner,
            "belief_initialization_seconds": belief_initialization_seconds,
            "root_belief_seconds": root_belief_seconds,
            "belief_diagnostics": {viewer: belief.diagnostics for viewer, belief in beliefs.items()},
            "teacher_checkpoint_sha256": teacher_hash, "boundary_actions": boundary_actions,
            "teacher_inference_seconds": teacher_seconds,
            "final_position_id": position.snapshot_revision, "elapsed_seconds": time.monotonic() - started,
            "statistics": statistics(position)}


class StagedDataset:
    """Validate staged root visits and values for the existing optimizer."""
    def __init__(self, paths, spec, architecture_family):
        if not paths or len(paths) > 4096:
            raise ValueError("staged dataset needs 1..4096 files")
        paths = tuple(Path(path).resolve() for path in paths)
        if len(set(paths)) != len(paths):
            raise ValueError("staged dataset has duplicate input paths")
        if sum(path.stat().st_size for path in paths) > 1_073_741_824:
            raise ValueError("staged dataset exceeds its input byte budget")
        self.examples, identities = [], []
        encoder = TypedEncoder(spec)
        for path in paths:
            payload = read_json(path)
            if (payload.get("version") != "accelerate-staged-v1"
                    or payload.get("encoder_hash") != spec.digest
                    or payload.get("architecture_family") != architecture_family):
                raise ValueError("staged dataset/model contract mismatch")
            if payload.get("provenance", {}).get("source") not in ("synthetic", "selfplay"):
                raise ValueError("staged dataset source provenance is missing")
            identities.append(sha256(canonical_json(payload).encode()).hexdigest())
            for sample in payload["samples"]:
                source, teacher_hash = sample["value_target_source"], sample["teacher_checkpoint_sha256"]
                if source not in ("terminal", "bootstrap:end", "bootstrap:middle"):
                    raise ValueError("unknown staged value target source")
                if source != "terminal" and not _sha256_text(teacher_hash):
                    raise ValueError("bootstrap sample lacks frozen teacher identity")
                if teacher_hash != payload.get("teacher_checkpoint_sha256"):
                    raise ValueError("sample and dataset teacher identities differ")
                deployment = payload.get("teacher_deployment")
                if source != "terminal" and (not isinstance(deployment, dict)
                        or deployment.get("model_sha256") != teacher_hash
                        or not _sha256_text(deployment.get("base_hash"))
                        or not _sha256_text(deployment.get("manifest_sha256"))):
                    raise ValueError("bootstrap sample lacks deployment/checkpoint provenance")
                actor, observation = sample["actor"], sample["observation"]
                value = sample["value"]
                if (actor not in ("white", "black") or observation["viewer"] != actor
                        or not math.isfinite(value) or abs(value) > 1):
                    raise ValueError("staged sample viewer/value mismatch")
                candidates = sample["candidates"]
                if not candidates or any(item["visits"] < 0 for item in candidates):
                    raise ValueError("staged sample has no valid root visits")
                total = sum(item["visits"] for item in candidates)
                if total <= 0 or any(abs(item["probability"] - item["visits"] / total) > 1e-6 for item in candidates):
                    raise ValueError("staged probabilities differ from root visits")
                intents = tuple(item["intent"] for item in candidates)
                summary = sample["belief_summary"]
                encoder.encode(ObservationIR.from_public(observation, spec, belief_summary=summary), intents)
                self.examples.append(TrainingExample(observation, intents,
                    tuple(item["probability"] for item in candidates), value, actor, summary))
        if not self.examples or len(self.examples) > 100_000:
            raise ValueError("staged dataset has no bounded training examples")
        self.digest = sha256(canonical_json({"files": identities, "encoder": spec.digest}).encode()).hexdigest()

    def __len__(self):
        return len(self.examples)

    def __getitem__(self, index):
        return self.examples[index]
