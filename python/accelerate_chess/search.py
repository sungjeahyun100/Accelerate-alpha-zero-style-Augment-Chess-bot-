"""Bounded information-set PUCT over independently reconstructed particles.

Only public observations enter this module's environment boundary. Native
positions owned here are created by a public-conditioned source factory; the
actual environment is never supplied. Python owns tracking/filtering/search,
and all legal actions, UI intent resolution and game transitions stay native.
"""
from __future__ import annotations

from dataclasses import dataclass, field
import hashlib
import json
import math
import time
from typing import Any, Callable, Mapping, Protocol, Sequence

import numpy as np

from .encoding import HISTORY_SUMMARY_VERSION, PublicEncoder, PublicObservation, batch_positions, canonical_json
from .inference import ProductionEvaluator

TRACE_VERSION = "accelerate-public-trace-v1"
SEARCH_VERSION = "availability-puct-v1"
MAX_PUBLIC_BYTES = 8 * 1024 * 1024
MAX_INFERENCE_ELEMENTS = 16_777_216


class SearchError(RuntimeError):
    """A public reconstruction/search boundary failed explicitly."""


class MissingHistoryError(SearchError):
    pass


class InformationMismatchError(SearchError):
    pass


class ParticleExhaustedError(SearchError):
    pass


class SourceCapabilityError(SearchError):
    pass


class SearchBudgetError(SearchError):
    pass


def _copy(value: Any) -> Any:
    return json.loads(canonical_json(value))


def _public(observation: Mapping[str, Any]) -> dict[str, Any]:
    # Identity is recomputed before any field can become a tracker/NN feature.
    PublicObservation.from_native(observation)
    PublicEncoder._reject_private(observation)
    encoded = canonical_json(observation)
    if len(encoded.encode()) > MAX_PUBLIC_BYTES:
        raise SearchBudgetError("public observation exceeds the 8 MiB trace boundary")
    result = json.loads(encoded)
    if len(result["board"]) != 8 or any(len(row) != 8 for row in result["board"]):
        raise InformationMismatchError("public board must be 8 by 8")
    return result


def _patch(before: Any, after: Any, path: tuple[str, ...] = ()) -> list[dict[str, Any]]:
    """Persistent trace deltas; arrays replace once instead of prefix copies."""
    if before == after:
        return []
    if isinstance(before, dict) and isinstance(after, dict):
        changes = [{"path": list(path + (key,)), "remove": True} for key in sorted(before.keys() - after.keys())]
        for key in sorted(after):
            if key not in before:
                changes.append({"path": list(path + (key,)), "value": after[key]})
            else:
                changes.extend(_patch(before[key], after[key], path + (key,)))
        return changes
    return [{"path": list(path), "value": after}]


def _apply_patch(body: dict[str, Any], patch: Sequence[Mapping[str, Any]]) -> None:
    for change in patch:
        path = change.get("path")
        if not isinstance(path, list) or not path or any(not isinstance(part, str) for part in path) or path[0] in ("history", "viewer", "protocolVersion"):
            raise InformationMismatchError("invalid public trace patch path")
        target = body
        for part in path[:-1]:
            if not isinstance(target.get(part), dict):
                raise InformationMismatchError("public trace patch parent is absent")
            target = target[part]
        if set(change) == {"path", "remove"} and change["remove"] is True and path[-1] in target:
            del target[path[-1]]
        elif set(change) == {"path", "value"}:
            target[path[-1]] = _copy(change["value"])
        else:
            raise InformationMismatchError("invalid public trace patch operation")


@dataclass(frozen=True)
class TraceStep:
    patch: tuple[dict[str, Any], ...]
    events: tuple[dict[str, Any], ...]
    own_intent: dict[str, Any] | None


class PublicTracker:
    """One initial public frame and append-only public deltas/events.

    Creating a tracker halfway through a game without the initial frame is an
    error. append() records each environment action, even if its public effect
    is empty. The caller may attach only a choice made by this viewer.
    """
    def __init__(self, initial: Mapping[str, Any], *, max_steps: int = 100_000, max_bytes: int = MAX_PUBLIC_BYTES):
        if type(max_steps) is not int or not 1 <= max_steps <= 100_000 or type(max_bytes) is not int or not 1024 <= max_bytes <= MAX_PUBLIC_BYTES:
            raise ValueError("trace limits are out of range")
        self._initial = _public(initial)
        if self._initial["history"]:
            raise MissingHistoryError("reconstruction requires the initial public frame and every subsequent step")
        self.viewer = self._initial["viewer"]
        self._body = {key: value for key, value in self._initial.items() if key != "history"}
        self._events: list[dict[str, Any]] = []
        self._steps: list[TraceStep] = []
        self._bytes = len(canonical_json(self._initial).encode())
        self.max_steps, self.max_bytes = max_steps, max_bytes
        if self._bytes > max_bytes:
            raise SearchBudgetError("initial public frame exceeds the trace byte budget")

    @property
    def initial(self) -> dict[str, Any]:
        return _copy(self._initial)

    @property
    def latest(self) -> dict[str, Any]:
        return _copy({**self._body, "history": self._events})

    @property
    def steps(self) -> int:
        return len(self._steps)

    def append(self, observation: Mapping[str, Any], *, own_intent: Mapping[str, Any] | None = None) -> None:
        self._append(observation, own_intent=own_intent, commit=True)

    def validate_append(self, observation: Mapping[str, Any], *, own_intent: Mapping[str, Any] | None = None) -> None:
        self._append(observation, own_intent=own_intent, commit=False)

    def _append(self, observation, *, own_intent, commit):
        after = _public(observation)
        if after["viewer"] != self.viewer:
            raise InformationMismatchError("a tracker cannot change its viewer")
        history = after["history"]
        if len(history) != len(self._events) + 1 or history[:-1] != self._events:
            raise MissingHistoryError("append requires exactly one unaltered public transition at a time")
        event = history[-1]
        if event.get("kind") != "transition" or event.get("actor") not in ("white", "black"):
            raise InformationMismatchError("trace requires native public Transition v1 events")
        if own_intent is not None and event["actor"] != self.viewer:
            raise InformationMismatchError("only the viewer's own selected intent may enter the trace")
        intent = _copy(own_intent) if own_intent is not None else None
        if intent is not None:
            if not isinstance(intent, dict):
                raise InformationMismatchError("selected public intent must be an object")
            PublicEncoder._reject_private(intent)
        body = {key: value for key, value in after.items() if key != "history"}
        patch = _patch(self._body, body)
        size = len(canonical_json({"patch": patch, "events": [event], "own_intent": intent}).encode())
        if len(self._steps) >= self.max_steps or self._bytes + size > self.max_bytes:
            raise SearchBudgetError("full public trace storage budget exhausted; no events were truncated")
        if not commit:
            return
        self._steps.append(TraceStep(tuple(patch), (event,), intent))
        self._events.append(event)
        self._body = body
        self._bytes += size

    def frames(self):
        body = _copy({key: value for key, value in self._initial.items() if key != "history"})
        events: list[dict[str, Any]] = []
        for step in self._steps:
            _apply_patch(body, step.patch)
            events.extend(_copy(step.events))
            frame = {**body, "history": events}
            _public(frame)
            yield step, frame

    def snapshot(self) -> dict[str, Any]:
        return _copy({"protocolVersion": TRACE_VERSION, "viewer": self.viewer, "initial": self._initial,
                      "steps": [{"patch": step.patch, "events": step.events, "ownIntent": step.own_intent} for step in self._steps]})

    @classmethod
    def from_snapshot(cls, trace: Mapping[str, Any]):
        if not isinstance(trace, Mapping) or set(trace) != {"protocolVersion", "viewer", "initial", "steps"} or trace["protocolVersion"] != TRACE_VERSION or not isinstance(trace["steps"], list):
            raise InformationMismatchError("invalid public trace envelope")
        tracker = cls(trace["initial"])
        if trace["viewer"] != tracker.viewer:
            raise InformationMismatchError("public trace viewer mismatch")
        frame = tracker.latest
        for step in trace["steps"]:
            if not isinstance(step, Mapping) or set(step) != {"patch", "events", "ownIntent"} or not isinstance(step["events"], list) or len(step["events"]) != 1:
                raise MissingHistoryError("public trace requires one transition per step")
            _apply_patch(frame, step["patch"])
            frame["history"].extend(_copy(step["events"]))
            tracker.append(frame, own_intent=step["ownIntent"])
        return tracker

    def frame_at(self, step_index: int) -> dict[str, Any]:
        if type(step_index) is not int or not 0 <= step_index <= self.steps:
            raise MissingHistoryError("public frame index is outside the complete trace")
        if not step_index:
            return self.initial
        for index, (_, frame) in enumerate(self.frames(), 1):
            if index == step_index:
                return _copy(frame)
        raise MissingHistoryError("public frame is absent")


class SourceParticleFactory(Protocol):
    def sample_initial(self, public_initial: Mapping[str, Any], independent_seed: int) -> Any:
        """Create a source-valid conditional particle, never import actual state."""

    def apply_conditioned(self, position: Any, action: Any, expected: Mapping[str, Any], independent_seed: int) -> Any:
        """Condition an observed past draw/opaque ID using native source helpers."""


class NativeSourceFactory:
    """Narrow source-rule constructor/conditioning calls; no Python rules."""
    def __init__(self, public_config: Mapping[str, Any]):
        if not isinstance(public_config, Mapping):
            raise TypeError("source factory accepts only public game configuration")
        PublicEncoder._reject_private(public_config)
        if set(public_config) - {"gameStyle", "draftDelete", "ruleCardIds", "starWinLimit", "deathmatchEnabled", "deathmatchLimitTurns"}:
            raise ValueError("source factory configuration contains fields outside the public GameConfig contract")
        self._config = _copy(public_config)
        from ._native import Position
        self._position_type = Position

    def sample_initial(self, public_initial: Mapping[str, Any], independent_seed: int):
        constructor = getattr(self._position_type, "sample_initial_public", None)
        if constructor is None:
            raise SourceCapabilityError("native source-conditioned initial factory is unavailable")
        from ._native import ConditioningMismatchError
        try:
            return constructor(self._config, _public(public_initial), independent_seed)
        except ConditioningMismatchError:
            return None

    def apply_conditioned(self, position, action, expected, independent_seed):
        apply = getattr(position, "apply_conditioned_public", None)
        if apply is not None:
            step = apply(action, expected, independent_seed)
            return None if step is None else step.position
        child = position.apply(action).position
        if _public(child.observe(expected["viewer"])) == expected:
            return child
        condition = getattr(child, "condition_public_identities", None)
        if condition is None:
            raise SourceCapabilityError("native past transition identity conditioning is unavailable")
        # Native checks card definitions/status/source feasibility and updates
        # every internal reference together; Python never patches rules state.
        from ._native import ConditioningMismatchError
        try:
            return condition(expected)
        except ConditioningMismatchError:
            return None


def _actor(position: Any) -> str:
    actor = position.decision_actor
    if callable(actor):
        actor = actor()
    if actor not in ("white", "black"):
        raise InformationMismatchError("sampled position has no valid decision actor")
    return actor


def _intent(action: Any) -> dict[str, Any]:
    project = getattr(action, "public_intent", None)
    if project is None:
        raise SourceCapabilityError("native action requires source-defined public_intent; hidden execution flags cannot be guessed away")
    intent = _copy(project())
    if not isinstance(intent, dict) or not isinstance(intent.get("type"), str):
        raise InformationMismatchError("native public decision intent is invalid")
    PublicEncoder._reject_private(intent)
    return intent


def _stream(position: Any, limit: int, maximum: int):
    stream = position.action_stream()
    examined = 0
    while examined < maximum:
        page = stream.next_page(min(limit, maximum - examined))
        actions, exhausted = page["actions"], page["exhausted"]
        if not isinstance(actions, (tuple, list)) or type(exhausted) is not bool or len(actions) > min(limit, maximum - examined) or (not actions and not exhausted):
            raise InformationMismatchError("native action stream returned an invalid page")
        examined += len(actions)
        yield actions, exhausted
        if exhausted:
            return


@dataclass(frozen=True)
class BeliefLimits:
    particles: int = 32
    proposals: int = 512
    page_size: int = 64
    actions_per_transition: int = 4096
    elapsed_ms: int = 5000

    def __post_init__(self):
        for name, maximum in (("particles", 1024), ("proposals", 100_000), ("page_size", 4096), ("actions_per_transition", 65_536), ("elapsed_ms", 3_600_000)):
            value = getattr(self, name)
            if type(value) is not int or not 1 <= value <= maximum:
                raise ValueError(f"belief {name} is outside its finite limit")
        if self.proposals < self.particles:
            raise ValueError("belief proposal budget must cover its requested particles")


class ParticleBelief:
    """Finite bootstrap filter of independent conditional source particles.

    Conditioning on public transitions does not select a favorable chance
    outcome. Matching hidden actions are reservoir-sampled uniformly, then
    particles are uniformly resampled. This is an explicit uniform legal
    opponent-action prior, not a learned opponent-policy posterior.
    """
    def __init__(self, tracker: PublicTracker, factory: SourceParticleFactory, *, seed: int, limits: BeliefLimits = BeliefLimits(), cancelled: Callable[[], bool] | None = None):
        if not isinstance(tracker, PublicTracker) or type(seed) is not int or not 0 <= seed < 2**64:
            raise TypeError("belief requires a public tracker and independent uint64 belief seed")
        self.tracker, self.factory, self.limits = tracker, factory, limits
        self.seed = seed
        self._rng = np.random.default_rng(seed)
        self._cancelled = cancelled or (lambda: False)
        self._particles: list[Any] = []
        self._revision = -1
        self.proposals_used = 0
        self.rebuild()

    def _check(self, started: float):
        if self._cancelled():
            raise SearchBudgetError("belief reconstruction cancelled")
        if (time.monotonic() - started) * 1000 >= self.limits.elapsed_ms:
            raise SearchBudgetError("belief reconstruction time budget exhausted")

    def _seed(self) -> int:
        return int(self._rng.integers(0, 2**32, dtype=np.uint64))

    def _advance(self, position: Any, step: TraceStep, expected: Mapping[str, Any], started: float):
        known = canonical_json(step.own_intent) if step.own_intent is not None else None
        if known is not None:
            if _actor(position) != self.tracker.viewer:
                return None
            action = position.bind_public_intent(step.own_intent)
            child = self.factory.apply_conditioned(position, action, expected, self._seed())
            if child is None:
                return None
            observed = _public(child.observe(self.tracker.viewer))
            return child if observed == expected else None
        selected, matches, exhausted = None, 0, False
        seen: set[str] = set()
        for actions, exhausted in _stream(position, self.limits.page_size, self.limits.actions_per_transition):
            for action in actions:
                self._check(started)
                intent = _intent(action)
                key = canonical_json(intent)
                if key in seen:
                    continue
                seen.add(key)
                bound = position.bind_public_intent(intent)
                child = self.factory.apply_conditioned(position, bound, expected, self._seed())
                if child is not None and _public(child.observe(self.tracker.viewer)) == expected:
                    matches += 1
                    if int(self._rng.integers(matches)) == 0:
                        selected = child
        if not exhausted:
            raise SearchBudgetError("belief action enumeration is incomplete; a partial posterior is not accepted")
        # Hidden worlds with many compatible observations have higher
        # likelihood under the declared uniform-intent opponent prior.
        return selected if matches and self._rng.random() < matches / len(seen) else None

    def rebuild(self) -> None:
        started = time.monotonic()
        initial = self.tracker.initial
        particles: list[Any] = []
        proposals = 0
        while len(particles) < self.limits.particles and proposals < self.limits.proposals:
            self._check(started)
            proposals += 1
            position = self.factory.sample_initial(initial, self._seed())
            if position is None:
                continue
            if _public(position.observe(self.tracker.viewer)) != initial:
                raise InformationMismatchError("source-conditioned initial particle does not match the full public frame")
            for step, frame in self.tracker.frames():
                position = self._advance(position, step, frame, started)
                if position is None:
                    break
            if position is not None:
                particles.append(position)
        if not particles:
            raise ParticleExhaustedError("no source-valid particles reproduce the complete public trace within the finite proposal budget")
        # Incomplete target count is explicit; usable posterior samples can be
        # resampled with replacement without pretending independence increased.
        self._particles, self._revision = particles, self.tracker.steps
        self.proposals_used = proposals

    def synchronize(self) -> None:
        if self._revision == self.tracker.steps:
            return
        if self._revision > self.tracker.steps:
            raise InformationMismatchError("public tracker revision went backwards")
        started = time.monotonic()
        surviving = self._particles
        for index, (step, frame) in enumerate(self.tracker.frames()):
            if index < self._revision:
                continue
            surviving = [child for position in surviving if (child := self._advance(position, step, frame, started)) is not None]
            if not surviving:
                self.rebuild()
                return
            # Standard bootstrap resampling; duplicate particles remain marked
            # in the summary rather than reported as distinct determinizations.
            surviving = [surviving[int(self._rng.integers(len(surviving)))] for _ in range(self.limits.particles)]
        self._particles, self._revision = surviving, self.tracker.steps

    def draw(self):
        if not self._particles:
            raise ParticleExhaustedError("the public belief contains no particles")
        if self._revision != self.tracker.steps:
            raise InformationMismatchError("synchronize public belief before search")
        return self._particles[int(self._rng.integers(len(self._particles)))]

    @property
    def summary(self) -> dict[str, Any]:
        return {"version": "public-particle-summary-v1", "particle_count": len(self._particles),
                "distinct_particle_instances": len({id(position) for position in self._particles}),
                "trace_steps": self._revision, "opponent_action_prior": "uniform-public-intents"}


@dataclass(frozen=True)
class SearchLimits:
    iterations: int = 128
    max_depth: int = 32
    max_nodes: int = 4096
    max_edges: int = 65_536
    elapsed_ms: int = 1000
    page_size: int = 64
    max_candidates: int = 256
    max_examined_actions: int = 4096
    leaf_batch_size: int = 4
    max_inference_elements: int = MAX_INFERENCE_ELEMENTS
    widening_constant: float = 4.
    widening_exponent: float = .5
    cpuct: float = 1.5

    def __post_init__(self):
        for name, maximum in (("iterations", 1_000_000), ("max_depth", 256), ("max_nodes", 1_000_000), ("max_edges", 1_000_000), ("elapsed_ms", 3_600_000), ("page_size", 4096), ("max_candidates", 4096), ("max_examined_actions", 65_536), ("leaf_batch_size", 64), ("max_inference_elements", MAX_INFERENCE_ELEMENTS)):
            value = getattr(self, name)
            if type(value) is not int or not 1 <= value <= maximum:
                raise ValueError(f"search {name} is outside its finite limit")
        if self.max_candidates > self.max_examined_actions or not math.isfinite(self.cpuct) or not 0 < self.cpuct <= 100 or not math.isfinite(self.widening_constant) or not 0 < self.widening_constant <= 4096 or not 0 < self.widening_exponent <= 1:
            raise ValueError("invalid progressive widening/PUCT contract")


@dataclass
class _Edge:
    intent: dict[str, Any]
    visits: int = 0
    availability: int = 0
    value_sum: float = 0.
    prior_sum: float = 0.
    in_flight: int = 0

    @property
    def q(self):
        return self.value_sum / self.visits if self.visits else 0.


@dataclass
class _Node:
    actor: str
    visits: int = 0
    edges: dict[str, _Edge] = field(default_factory=dict)


@dataclass
class _SearchState:
    nodes: dict[str, _Node] = field(default_factory=dict)
    edges: int = 0
    completed: int = 0
    stop: str = "iterations"
    root_exhausted: bool = True
    partial: bool = False
    inference_batches: int = 0
    max_inference_batch: int = 0


class _SimulationStopped(Exception):
    """Internal cancellation boundary; an unfinished path gets no value label."""


@dataclass(frozen=True)
class SearchResult:
    intent: dict[str, Any]
    action_key: str
    policy: tuple[dict[str, Any], ...]
    information_state_key: str
    iterations: int
    stop_reason: str
    legal_actions_exhausted: bool
    partial_coverage: bool
    nodes: int
    edges: int
    encoder_hash: str
    version: str = SEARCH_VERSION
    belief_summary: dict[str, Any] | None = None
    model_sha256: str | None = None
    inference_batches: int = 0
    max_inference_batch: int = 0

    def bind(self, environment):
        """The caller's execution boundary; no actual state enters search.

        Only resolve the previously selected public UI choice. Its native
        hidden flags are never returned to the tracker or neural features.
        """
        return environment.bind_public_intent(self.intent)


def _allowed(intent: Mapping[str, Any], observation: Mapping[str, Any]) -> bool:
    """Use projected click hints; never actual-environment legality masks."""
    hints = observation["publicState"].get("legalHints")
    if hints is None:
        raise SourceCapabilityError("source public observation needs legal click hints")
    if intent["type"] == "move":
        origin = intent.get("from")
        destination = intent.get("destination", intent.get("move"))
        if not isinstance(origin, Mapping) or not isinstance(destination, Mapping):
            raise InformationMismatchError("native move intent has no public click coordinates")
        coordinate = {name: destination[name] for name in ("row", "col")}
        return any(item.get("from") == origin and coordinate in item.get("destinations", ()) for item in hints.get("moves", ()))
    if intent["type"] == "card" and "target" in intent:
        targets = next((item["targets"] for item in hints.get("cardTargets", ()) if item.get("cardInstanceId") == intent.get("cardInstanceId")), ())
        def coordinates(value):
            if isinstance(value, Mapping):
                if "row" in value and "col" in value:
                    yield {"row": value["row"], "col": value["col"]}
                for child in value.values():
                    yield from coordinates(child)
            elif isinstance(value, list):
                for child in value:
                    yield from coordinates(child)
        return all(square in targets for square in coordinates(intent["target"]))
    return True


class InformationSetSearch:
    """Availability-count PUCT, sampled chance, and finite progressive widening."""
    def __init__(self, encoder: PublicEncoder, evaluator: ProductionEvaluator, *, limits: SearchLimits = SearchLimits()):
        if not isinstance(evaluator, ProductionEvaluator):
            raise TypeError("production search requires the explicit native Rust ProductionEvaluator")
        if encoder.spec.digest != evaluator.spec.digest:
            raise ValueError("search encoder and native evaluator contracts differ")
        if encoder.spec.action_encoding != "public-decision-intent-v1":
            raise ValueError("production search requires an explicit public-decision-intent-v1 model contract")
        self.encoder, self.evaluator, self.limits = encoder, evaluator, limits

    def _evaluate_many(self, requests, state):
        # Check before encoding/stacking. Padding is explicit and rows retain
        # their own candidate mask; resource failures never shrink the batch.
        count = len(requests)
        actions = max(1, max(len(intents) for _, intents, _ in requests))
        spec = self.encoder.spec
        elements = count * (64 * spec.board_channels + spec.condition_dim + actions * spec.action_dim)
        if elements > self.limits.max_inference_elements:
            raise SearchBudgetError("requested leaf batch exceeds the declared inference input budget")
        batch = batch_positions([self.encoder.encode(observation, intents, belief_summary=summary)
                                 for observation, intents, summary in requests])
        logits, values = self.evaluator.evaluate(batch.board, batch.condition, batch.action_features)
        if logits.shape != (count, actions) or values.shape != (count, 1) or not np.isfinite(logits).all() or not np.isfinite(values).all() or np.abs(values).max() > 1.00001:
            raise InformationMismatchError("native evaluator returned invalid policy/value")
        state.inference_batches += 1
        state.max_inference_batch = max(state.max_inference_batch, count)
        results = []
        for row, (_, intents, _) in enumerate(requests):
            if not intents:
                results.append((np.empty(0), float(values[row, 0])))
                continue
            scores = logits[row, batch.action_mask[row]].astype(np.float64)
            probabilities = np.exp(scores - scores.max())
            probabilities /= probabilities.sum()
            results.append((probabilities, float(values[row, 0])))
        return results

    @staticmethod
    def _outcome(position, actor):
        outcome = position.result
        if callable(outcome):
            outcome = outcome()
        if outcome is None:
            return None
        if outcome not in ("white", "black", "draw"):
            raise InformationMismatchError("unsupported native terminal result")
        return 0. if outcome == "draw" else (1. if outcome == actor else -1.)

    def _simulation(self, belief, root, state, check):
        """Cooperative leaf requests share one availability tree.

        A path reserves virtual visits while waiting for the next native batch.
        Finally releases them even when cancelled; only completed simulations
        backpropagate values. No worker thread or extra rules implementation is
        involved in preparing a batch of independent particle simulations.
        """
        viewer, root_key = root["viewer"], root["informationStateKey"]
        position = belief.draw()
        if _actor(position) != viewer or _public(position.observe(viewer)) != root:
            raise InformationMismatchError("root belief viewer must equal the actual decision actor and match the public frame")
        path: list[tuple[_Node, _Edge]] = []
        try:
            for _ in range(self.limits.max_depth):
                check()
                actor = _actor(position)
                value, value_actor = self._outcome(position, actor), actor
                if value is not None:
                    break
                observation = _public(position.observe(actor))
                if observation["viewer"] != actor:
                    raise InformationMismatchError("neural observation viewer must be the decision actor")
                key = observation["informationStateKey"]
                if key not in state.nodes:
                    if len(state.nodes) >= self.limits.max_nodes:
                        state.stop, state.partial = "nodes", True
                        _, value = yield observation, [], belief.summary
                        break
                    state.nodes[key] = _Node(actor)
                node = state.nodes[key]
                new = node.visits == 0
                if node.actor != actor:
                    raise InformationMismatchError("information node actor identity changed")
                width = min(self.limits.max_candidates, max(1, math.ceil(self.limits.widening_constant * (node.visits + 1)**self.limits.widening_exponent)))
                available: dict[str, Any] = {}
                intents: dict[str, dict[str, Any]] = {}
                exhausted, capped = False, False
                for actions, page_exhausted in _stream(position, self.limits.page_size, self.limits.max_examined_actions):
                    check()
                    for action in actions:
                        intent = _intent(action)
                        intent_key = canonical_json(intent)
                        if not _allowed(intent, observation) or intent_key in available:
                            continue
                        if intent_key not in node.edges:
                            if len(node.edges) >= width:
                                capped = True
                                continue
                            if state.edges >= self.limits.max_edges:
                                state.stop, state.partial, capped = "edges", True, True
                                continue
                            node.edges[intent_key] = _Edge(intent)
                            state.edges += 1
                        available[intent_key] = position.bind_public_intent(intent)
                        intents[intent_key] = intent
                    exhausted = page_exhausted
                    if capped or len(available) >= width:
                        break
                covered = exhausted and not capped
                if key == root_key:
                    state.root_exhausted &= covered
                state.partial |= not covered
                ordered = sorted(available)
                probabilities, value = yield observation, [intents[item] for item in ordered], belief.summary
                check()
                node.visits += 1
                for item, probability in zip(ordered, probabilities):
                    edge = node.edges[item]
                    edge.availability += 1
                    edge.prior_sum += float(probability)
                if not ordered:
                    if covered:
                        raise SourceCapabilityError("nonterminal native position has no publicly selectable legal intent")
                    state.stop, state.partial = "candidate-budget", True
                    break
                if new and path:
                    break
                selected = max(ordered, key=lambda item: (
                    node.edges[item].q + self.limits.cpuct * (node.edges[item].prior_sum / node.edges[item].availability)
                    * math.sqrt(node.edges[item].availability) / (1 + node.edges[item].visits + node.edges[item].in_flight), item))
                edge = node.edges[selected]
                edge.in_flight += 1
                path.append((node, edge))
                position = position.apply(available[selected]).position
                # Independent native particle RNG samples future chance;
                # alternative random outcomes are never maximized.
            else:
                check()
                state.partial = True
                value_actor = _actor(position)
                value = self._outcome(position, value_actor)
                if value is None:
                    _, value = yield _public(position.observe(value_actor)), [], belief.summary
            check()
            for node, edge in reversed(path):
                edge.visits += 1
                edge.value_sum += value if node.actor == value_actor else -value
            state.completed += 1
        finally:
            for _, edge in path:
                edge.in_flight -= 1

    def run(self, belief: ParticleBelief, *, cancelled: Callable[[], bool] | None = None) -> SearchResult:
        if not isinstance(belief, ParticleBelief):
            raise TypeError("search accepts a public ParticleBelief, never an actual environment Position")
        belief.synchronize()
        root = belief.tracker.latest
        root_key = root["informationStateKey"]
        state = _SearchState()
        started = time.monotonic()
        cancelled = cancelled or (lambda: False)

        def check():
            if cancelled():
                state.stop, state.partial = "cancelled", True
                raise _SimulationStopped()
            if (time.monotonic() - started) * 1000 >= self.limits.elapsed_ms:
                state.stop, state.partial = "elapsed", True
                raise _SimulationStopped()

        issued = 0
        while issued < self.limits.iterations:
            active = []
            try:
                check()
                count = min(self.limits.leaf_batch_size, self.limits.iterations - issued)
                issued += count
                for _ in range(count):
                    simulation = self._simulation(belief, root, state, check)
                    active.append((simulation, next(simulation)))
                while active:
                    check()
                    outputs = self._evaluate_many([request for _, request in active], state)
                    next_active = []
                    for (simulation, _), output in zip(active, outputs):
                        try:
                            next_active.append((simulation, simulation.send(output)))
                        except StopIteration:
                            pass
                    active = next_active
            except _SimulationStopped:
                break
            finally:
                for simulation, _ in active:
                    simulation.close()
            if state.stop != "iterations":
                break
        root_node = state.nodes.get(root_key)
        if root_node is None or not root_node.edges or not any(edge.visits for edge in root_node.edges.values()):
            raise SearchBudgetError(f"no public decision was evaluated before {state.stop}")
        selected = max(root_node.edges, key=lambda item: (root_node.edges[item].visits, root_node.edges[item].q, item))
        total = sum(edge.visits for edge in root_node.edges.values())
        policy = tuple({"action_key": key, "intent": _copy(edge.intent), "visits": edge.visits,
                        "availability": edge.availability, "probability": edge.visits / total if total else 0.,
                        "value": edge.q} for key, edge in sorted(root_node.edges.items()))
        return SearchResult(_copy(root_node.edges[selected].intent), selected, policy, root_key, state.completed, state.stop,
                            state.root_exhausted, state.partial, len(state.nodes), state.edges, self.encoder.spec.digest,
                            belief_summary=_copy(belief.summary),
                            model_sha256=getattr(getattr(self.evaluator, "session", None), "model_sha256", None),
                            inference_batches=state.inference_batches, max_inference_batch=state.max_inference_batch)
