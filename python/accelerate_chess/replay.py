"""Public replay, terminal-only targets and fixed external artifact storage."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import tempfile
from typing import Any, Mapping

from .encoding import EncoderSpec, PublicEncoder, canonical_json
from .search import PublicTracker, SEARCH_VERSION, SearchResult

REPLAY_VERSION = "accelerate-replay-v2"
MAX_REPLAY_BYTES = 16 * 1024 * 1024


def artifact_root(explicit: str | Path | None = None) -> Path:
    if explicit is not None:
        root = Path(explicit).expanduser().resolve()
    elif os.environ.get("RUNNER_TEMP"):
        root = Path(os.environ["RUNNER_TEMP"]) / "Accelerate"
    elif os.name == "nt" and os.environ.get("APPDATA"):
        root = Path(os.environ["APPDATA"]) / "Accelerate"
    elif os.name != "nt":
        # WSL persistent outputs use the host root. No guessed home fallback.
        try:
            host = subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", "$env:APPDATA"], capture_output=True, text=True, timeout=15, check=True).stdout.strip()
            if not host:
                raise RuntimeError("host APPDATA is empty")
            translated = subprocess.run(["wslpath", "-u", host], capture_output=True, text=True, timeout=15, check=True).stdout.strip()
            if not translated:
                raise RuntimeError("host APPDATA translation is empty")
            root = Path(translated) / "Accelerate"
        except (OSError, subprocess.SubprocessError) as error:
            raise RuntimeError("host APPDATA could not be resolved; provide an explicit external artifact root") from error
    else:
        raise RuntimeError("APPDATA is unavailable; provide an explicit external artifact root")
    root = root.resolve()
    repository = Path(__file__).resolve().parents[2]
    if root == repository or repository in root.parents or any((ancestor / ".git").exists() for ancestor in (root, *root.parents)):
        raise ValueError("generated artifacts must be outside the source checkout")
    root.mkdir(parents=True, exist_ok=True)
    return root


def slot(root: Path, category: str, name: str) -> Path:
    if category not in {"models", "datasets", "runs", "reports", "tmp", "build", "cache"} or not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}", name):
        raise ValueError("artifact slots need an allowed category and stable safe name")
    target = (root / category / name).resolve()
    if root.resolve() not in target.parents:
        raise ValueError("artifact slot escapes its owned external root")
    target.mkdir(parents=True, exist_ok=True)
    return target


def atomic_json(path: str | Path, payload: Any) -> None:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    text = canonical_json(payload) + "\n"
    if len(text.encode()) > MAX_REPLAY_BYTES:
        raise ValueError("public JSON artifact exceeds its 16 MiB storage boundary")
    temporary = None
    try:
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", newline="\n", dir=path.parent,
                                        prefix=f".{path.name}.", suffix=".tmp", delete=False) as output:
            temporary = Path(output.name)
            output.write(text)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def read_json(path: str | Path) -> Any:
    path = Path(path)
    if path.stat().st_size > MAX_REPLAY_BYTES:
        raise ValueError("public JSON input exceeds the storage boundary")
    value = json.loads(path.read_text(encoding="utf-8"))
    canonical_json(value)
    return value


def _digest(value) -> str:
    return hashlib.sha256(canonical_json(value).encode()).hexdigest()


@dataclass(frozen=True)
class TrainingExample:
    observation: dict[str, Any]
    intents: tuple[dict[str, Any], ...]
    policy: tuple[float, ...]
    value: float
    actor: str
    belief_summary: dict[str, Any] | None = None


class EpisodeRecorder:
    """Store each projected event once; decisions reference public trace frames."""
    def __init__(self, initial_observations: Mapping[str, Mapping[str, Any]], spec: EncoderSpec, *, environment_seed: int, belief_seed: int, evidence_kind: str = "bounded-verification", model_sha256: str | None = None):
        if set(initial_observations) != {"white", "black"}:
            raise ValueError("replay needs both viewers' initial public frames")
        if type(environment_seed) is not int or not 0 <= environment_seed < 2**32 or type(belief_seed) is not int or not 0 <= belief_seed < 2**64:
            raise ValueError("replay seeds must be bounded integers")
        if evidence_kind not in ("bounded-verification", "selfplay", "synthetic"):
            raise ValueError("unknown replay evidence kind")
        if (model_sha256 is None and evidence_kind != "synthetic") or (model_sha256 is not None and (not isinstance(model_sha256, str) or len(model_sha256) != 64 or any(digit not in "0123456789abcdef" for digit in model_sha256))):
            raise ValueError("native replay needs the verified teacher model SHA-256")
        self._encoder = PublicEncoder(spec)
        self.trackers = {viewer: PublicTracker(self._encoder.validate_observation(frame).to_native()) for viewer, frame in initial_observations.items()}
        if any(tracker.viewer != viewer for viewer, tracker in self.trackers.items()):
            raise ValueError("replay viewer identity mismatch")
        self.spec = spec
        self.metadata = {"environment_seed": environment_seed, "belief_seed": belief_seed, "evidence_kind": evidence_kind,
                         "rules_version": spec.rules_version, "catalog_hash": spec.catalog_hash,
                         "encoder_hash": spec.digest, "search_version": SEARCH_VERSION, "model_sha256": model_sha256}
        if belief_seed > 2**53 - 1:
            self.metadata["belief_seed"] = str(belief_seed)
        self.decisions: list[dict[str, Any]] = []
        self.outcome = {"status": "unfinished", "winner": None, "reason": "not-finished"}

    def record_decision(self, actor: str, result: SearchResult) -> None:
        if actor not in self.trackers or result.information_state_key != self.trackers[actor].latest["informationStateKey"] or result.encoder_hash != self.spec.digest:
            raise ValueError("decision actor/public frame/model contract mismatch")
        if result.model_sha256 != self.metadata["model_sha256"]:
            raise ValueError("decision teacher model differs from the episode provenance")
        candidates = [{"intent": item["intent"], "visits": item["visits"], "probability": item["probability"], "action_key": item["action_key"]} for item in result.policy]
        record = {"actor": actor, "trace_step": self.trackers[actor].steps, "information_state_key": result.information_state_key,
                  "chosen_intent": result.intent, "candidates": candidates, "transition_completed": False,
                  "belief_summary": result.belief_summary,
                  "search": {"iterations": result.iterations, "stop_reason": result.stop_reason, "partial_coverage": result.partial_coverage,
                             "legal_actions_exhausted": result.legal_actions_exhausted, "version": result.version,
                             "inference_batches": result.inference_batches, "max_inference_batch": result.max_inference_batch}}
        _validate_decision(record, self.trackers, self.spec)
        self.decisions.append(json.loads(canonical_json(record)))

    def advance(self, observations: Mapping[str, Mapping[str, Any]], *, actor: str, intent: Mapping[str, Any]):
        if set(observations) != {"white", "black"} or actor not in self.trackers:
            raise ValueError("advance requires both public projections and the actual decision actor")
        observations = {viewer: self._encoder.validate_observation(frame).to_native() for viewer, frame in observations.items()}
        # Validate both projections without partially advancing one tracker.
        for viewer, tracker in self.trackers.items():
            tracker.validate_append(observations[viewer], own_intent=intent if viewer == actor else None)
        for viewer, tracker in self.trackers.items():
            tracker.append(observations[viewer], own_intent=intent if viewer == actor else None)
        if self.decisions and self.decisions[-1]["actor"] == actor and self.decisions[-1]["trace_step"] == self.trackers[actor].steps - 1 and self.decisions[-1]["chosen_intent"] == intent:
            self.decisions[-1]["transition_completed"] = True

    def finish(self, outcome: str | None, reason: str):
        if outcome not in (None, "white", "black", "draw") or not isinstance(reason, str) or not reason:
            raise ValueError("invalid terminal/unfinished replay outcome")
        self.outcome = {"status": "unfinished" if outcome is None else "terminal", "winner": outcome,
                        "reason": reason}

    def snapshot(self):
        content = {"version": REPLAY_VERSION, "metadata": self.metadata, "encoder": self.spec.to_dict(), "observation_policy": self.spec.observation_policy,
                   "traces": {viewer: tracker.snapshot() for viewer, tracker in self.trackers.items()},
                   "decisions": self.decisions, "outcome": self.outcome}
        return {**content, "replay_hash": _digest(content)}

    def save(self, path):
        payload = self.snapshot()
        ReplayEpisode(payload, self.spec)
        atomic_json(path, payload)


def _validate_decision(record, trackers, spec):
    if not isinstance(record, dict) or set(record) != {"actor", "trace_step", "information_state_key", "chosen_intent", "candidates", "search", "transition_completed", "belief_summary"} or record["actor"] not in trackers or type(record["transition_completed"]) is not bool:
        raise ValueError("invalid replay decision contract")
    observation = trackers[record["actor"]].frame_at(record["trace_step"])
    if observation["informationStateKey"] != record["information_state_key"]:
        raise ValueError("replay decision information identity mismatch")
    if record["transition_completed"]:
        actor_trace = trackers[record["actor"]]
        if record["trace_step"] >= actor_trace.steps:
            raise ValueError("completed replay decision lacks its public transition")
        selected = actor_trace._steps[record["trace_step"]].own_intent
        if selected is None or canonical_json(selected) != canonical_json(record["chosen_intent"]):
            raise ValueError("completed replay decision selected intent differs from the actor trace")
    candidates = record["candidates"]
    if not isinstance(candidates, list) or not candidates or len(candidates) > 4096:
        raise ValueError("replay candidates must be a nonempty finite list")
    intents = []
    visits = []
    for candidate in candidates:
        if set(candidate) != {"intent", "visits", "probability", "action_key"} or type(candidate["visits"]) is not int or candidate["visits"] < 0 or type(candidate["probability"]) not in (int, float) or not math.isfinite(candidate["probability"]) or not 0 <= candidate["probability"] <= 1:
            raise ValueError("invalid replay policy candidate")
        if canonical_json(candidate["intent"]) != candidate["action_key"]:
            raise ValueError("replay public intent identity mismatch")
        intents.append(candidate["intent"])
        visits.append(candidate["visits"])
    total = sum(visits)
    if not total or any(abs(candidate["probability"] - count / total) > 1e-6 for candidate, count in zip(candidates, visits)) or canonical_json(record["chosen_intent"]) not in {candidate["action_key"] for candidate in candidates}:
        raise ValueError("replay visits and policy target do not agree")
    if record["belief_summary"] is not None and not isinstance(record["belief_summary"], dict):
        raise ValueError("replay belief summary must contain public JSON data")
    PublicEncoder(spec).encode(observation, intents, belief_summary=record["belief_summary"])
    search = record["search"]
    if not isinstance(search, dict) or set(search) != {"iterations", "stop_reason", "partial_coverage", "legal_actions_exhausted", "version", "inference_batches", "max_inference_batch"} or search["version"] != SEARCH_VERSION or type(search["iterations"]) is not int or search["iterations"] < 1 or type(search["partial_coverage"]) is not bool or type(search["legal_actions_exhausted"]) is not bool or not isinstance(search["stop_reason"], str) or type(search["inference_batches"]) is not int or search["inference_batches"] < 0 or type(search["max_inference_batch"]) is not int or not 0 <= search["max_inference_batch"] <= 64:
        raise ValueError("invalid replay search provenance")


class ReplayEpisode:
    def __init__(self, payload, expected_spec: EncoderSpec | None = None):
        fields = {"version", "metadata", "encoder", "observation_policy", "traces", "decisions", "outcome", "replay_hash"}
        if not isinstance(payload, dict):
            raise ValueError("replay contract must be a JSON object")
        payload = json.loads(canonical_json(payload))
        if not isinstance(payload, dict) or set(payload) != fields or payload["version"] != REPLAY_VERSION or payload["replay_hash"] != _digest({key: value for key, value in payload.items() if key != "replay_hash"}):
            raise ValueError("replay contract or content hash mismatch")
        self.spec = EncoderSpec.from_dict(payload["encoder"], observation_policy=payload["observation_policy"])
        if expected_spec is not None and expected_spec.digest != self.spec.digest:
            raise ValueError("replay encoder compatibility mismatch")
        metadata = payload["metadata"]
        if not isinstance(metadata, dict) or set(metadata) != {"environment_seed", "belief_seed", "evidence_kind", "rules_version", "catalog_hash", "encoder_hash", "search_version", "model_sha256"} or (metadata["rules_version"], metadata["catalog_hash"], metadata["encoder_hash"], metadata["search_version"]) != (self.spec.rules_version, self.spec.catalog_hash, self.spec.digest, SEARCH_VERSION):
            raise ValueError("replay provenance differs from the model contracts")
        model_hash = metadata["model_sha256"]
        if (model_hash is None and metadata["evidence_kind"] != "synthetic") or (model_hash is not None and (not isinstance(model_hash, str) or len(model_hash) != 64 or any(digit not in "0123456789abcdef" for digit in model_hash))):
            raise ValueError("replay teacher model SHA-256 is invalid")
        belief_seed = metadata["belief_seed"]
        if isinstance(belief_seed, str) and belief_seed.isdigit() and len(belief_seed) <= 20:
            belief_seed = int(belief_seed)
        if type(metadata["environment_seed"]) is not int or not 0 <= metadata["environment_seed"] < 2**32 or type(belief_seed) is not int or not 0 <= belief_seed < 2**64 or metadata["evidence_kind"] not in ("bounded-verification", "selfplay", "synthetic"):
            raise ValueError("invalid replay seed/evidence provenance")
        if not isinstance(payload["traces"], dict) or set(payload["traces"]) != {"white", "black"}:
            raise ValueError("replay public traces are missing")
        self.trackers = {viewer: PublicTracker.from_snapshot(trace) for viewer, trace in payload["traces"].items()}
        encoder = PublicEncoder(self.spec)
        for tracker in self.trackers.values():
            encoder.validate_observation(tracker.initial)
            for _, frame in tracker.frames():
                encoder.validate_observation(frame)
        if any(tracker.viewer != viewer for viewer, tracker in self.trackers.items()) or len({tracker.steps for tracker in self.trackers.values()}) != 1:
            raise ValueError("replay public projection steps do not agree")
        self.decisions = payload["decisions"]
        if not isinstance(self.decisions, list) or len(self.decisions) > self.trackers["white"].steps + 1:
            raise ValueError("replay decision count exceeds completed source transitions")
        previous = -1
        for record in self.decisions:
            _validate_decision(record, self.trackers, self.spec)
            if record["trace_step"] <= previous:
                raise ValueError("replay decisions must increase monotonically")
            previous = record["trace_step"]
            if not record["transition_completed"] and (record is not self.decisions[-1] or record["trace_step"] != self.trackers["white"].steps):
                raise ValueError("only the last attempted decision can lack a completed transition")
        self.outcome = payload["outcome"]
        if not isinstance(self.outcome, dict) or set(self.outcome) != {"status", "winner", "reason"} or self.outcome["status"] not in ("terminal", "unfinished") or not isinstance(self.outcome["reason"], str) or not self.outcome["reason"]:
            raise ValueError("invalid replay outcome")
        if (self.outcome["status"] == "unfinished" and self.outcome["winner"] is not None) or (self.outcome["status"] == "terminal" and self.outcome["winner"] not in ("white", "black", "draw")):
            raise ValueError("unfinished episodes cannot carry a defeat/draw training label")
        if self.outcome["status"] == "terminal":
            if any(not record["transition_completed"] for record in self.decisions):
                raise ValueError("terminal replay cannot contain an unexecuted policy decision")
            for tracker in self.trackers.values():
                history = tracker.latest["history"]
                if not history or history[-1].get("result", {}).get("status") != "terminal" or history[-1]["result"].get("outcome") != self.outcome["winner"]:
                    raise ValueError("terminal labels require matching public native result records")
        self.replay_hash = payload["replay_hash"]

    @classmethod
    def load(cls, path, expected_spec=None):
        return cls(read_json(path), expected_spec)

    def examples(self):
        if self.outcome["status"] != "terminal":
            return
        winner = self.outcome["winner"]
        for record in self.decisions:
            yield TrainingExample(self.trackers[record["actor"]].frame_at(record["trace_step"]),
                     tuple(candidate["intent"] for candidate in record["candidates"]),
                     tuple(candidate["probability"] for candidate in record["candidates"]),
                     0. if winner == "draw" else (1. if winner == record["actor"] else -1.), record["actor"], record["belief_summary"])
