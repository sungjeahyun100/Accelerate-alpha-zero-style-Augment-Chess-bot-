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
SEARCH_VERSION = "availability-puct-v3"
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

    def _frames_since(self, revision: int):
        if type(revision) is not int or not 0 <= revision <= self.steps:
            raise InformationMismatchError("public tracker revision is invalid")
        if revision + 1 == self.steps:
            # append already validated this full frame and its history. Check
            # the owned current frame once, without replaying every old patch.
            yield self._steps[-1], _public({**self._body, "history": self._events})
            return
        for index, frame in enumerate(self.frames()):
            if index >= revision:
                yield frame

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

    def apply_conditioned(self, position: Any, action: Any, expected: Mapping[str, Any], independent_seed: int) -> TransitionProposal | None:
        """Return a source-conditioned child and its validated chance correction."""


@dataclass(frozen=True)
class TransitionProposal:
    """Source-owned latent proposal and its prior/proposal density correction.

    The unchanged proposal has weight one. Conditional proposals carry the
    actual source chance probability and proposal probability. These describe
    independently sampled source draws, never a posterior over actual RNG
    seeds. Python applies the correction and the opponent-action likelihood.
    """
    position: Any
    importance_weight: float = 1.
    source_probability: float | None = None
    proposal_probability: float | None = None
    profile: str = "source-prior-v1"

    def __post_init__(self):
        if type(self.importance_weight) not in (int, float) or not math.isfinite(self.importance_weight) or self.importance_weight <= 0:
            raise InformationMismatchError("source proposal importance must be finite and positive")
        if (self.source_probability is None) != (self.proposal_probability is None):
            raise InformationMismatchError("source proposal probability metadata is incomplete")
        if self.source_probability is not None:
            for probability in (self.source_probability, self.proposal_probability):
                if type(probability) not in (int, float) or not math.isfinite(probability) or not 0 < probability <= 1:
                    raise InformationMismatchError("source proposal probabilities must be finite within (0,1]")
            if not math.isclose(self.importance_weight, self.source_probability / self.proposal_probability, rel_tol=1e-10, abs_tol=0.):
                raise InformationMismatchError("source proposal importance does not equal source/proposal probability")
        if not isinstance(self.profile, str) or not self.profile or len(self.profile) > 128:
            raise InformationMismatchError("source proposal profile is invalid")


class NativeSourceFactory:
    """Narrow source-rule constructor/conditioning calls; no Python rules."""
    def __init__(self, public_config: Mapping[str, Any]):
        if not isinstance(public_config, Mapping):
            raise TypeError("source factory accepts only public game configuration")
        PublicEncoder._reject_private(public_config)
        if set(public_config) - {"gameStyle", "draftDelete", "ruleCardIds", "starWinLimit", "deathmatchEnabled", "deathmatchLimitTurns"}:
            raise ValueError("source factory configuration contains fields outside the public GameConfig contract")
        self._config = _copy(public_config)
        from ._native import Action, Position
        self._action_type = Action
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
        from ._native import ConditioningMismatchError
        apply = getattr(position, "apply_weighted_conditioned_public", None)
        if apply is None:
            raise SourceCapabilityError("native weighted public transition conditioning is unavailable")
        try:
            proposal = apply(action, expected, independent_seed)
        except ConditioningMismatchError:
            return None
        if not isinstance(proposal, Mapping) or set(proposal) != {"step", "importance_weight", "source_probability", "proposal_probability"}:
            raise InformationMismatchError("native conditioned step metadata has an invalid shape")
        step = proposal["step"]
        if step is None or not hasattr(step, "position"):
            raise InformationMismatchError("native conditioned step has no child position")
        # The former StepResult-only helper proves compatibility, but carries
        # no observed-chance likelihood. It is never a posterior fallback.
        return TransitionProposal(step.position, proposal["importance_weight"],
            proposal["source_probability"], proposal["proposal_probability"],
            "source-weighted-conditional-step-v1")

    def prepare_transition(self, position, expected, independent_seed):
        before = _public(position.observe(expected["viewer"]))
        public = before["publicState"]
        if (public.get("mode") != "draft" or public.get("phase") != "OPENING"
                or _actor(position) == expected["viewer"] or "draft" in public
                or len(expected["publicState"].get("revealedOpponentCards", ())) <= len(public.get("revealedOpponentCards", ()))):
            return TransitionProposal(position)
        condition = getattr(position, "condition_hidden_opening_draft", None)
        if condition is None:
            return TransitionProposal(position)
        from ._native import ConditioningMismatchError
        try:
            proposal = condition(expected, independent_seed)
        except ConditioningMismatchError:
            return None
        if not isinstance(proposal, Mapping) or set(proposal) != {"position", "importance_weight", "source_probability", "proposal_probability"}:
            raise InformationMismatchError("native source proposal metadata has an invalid shape")
        result = TransitionProposal(**proposal, profile="source-weighted-offer-proposal-v1")
        if _public(result.position.observe(expected["viewer"])) != before:
            raise InformationMismatchError("latent conditioning changed the prior public observation")
        return result

    def transition_compatible(self, position, action, expected):
        compatible = getattr(position, "public_transition_compatible", None)
        if compatible is None:
            return True
        result = compatible(action, expected)
        if type(result) is not bool:
            raise InformationMismatchError("native public transition compatibility must be boolean")
        return result

    def bind_streamed_public_intent(self, position, action, intent):
        # A native draft stream already returns the exact current-position
        # Action. Compatibility and apply retain the native ownership check.
        # Other families still need source UI intent resolution (move flags,
        # targets and ordered choices must not be guessed in Python).
        if (type(position) is self._position_type and type(action) is self._action_type
                and intent.get("type") in ("draftPick", "draftBundlePick")):
            return action
        return position.bind_public_intent(intent)


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
        requested = min(limit, maximum - examined)
        page = stream.next_page(requested)
        if not isinstance(page, Mapping) or set(page) != {"actions", "exhausted", "examined"}:
            raise InformationMismatchError("native action stream returned an invalid page")
        actions, exhausted = page["actions"], page["exhausted"]
        if not isinstance(actions, (tuple, list)) or type(exhausted) is not bool or len(actions) > requested:
            raise InformationMismatchError("native action stream returned an invalid page")
        page_examined = page["examined"]
        if (type(page_examined) is not int or not len(actions) <= page_examined <= requested
                or (page_examined == 0 and not exhausted)):
            raise InformationMismatchError("native action stream returned an invalid page")
        examined += page_examined
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
    """Finite importance filter of independent conditional source particles.

    Matching hidden actions are reservoir-sampled uniformly. A source-owned
    conditional proposal contributes its prior/proposal correction; the public
    action likelihood is matching intents divided by all unique legal intents.
    Log weights are normalized before bootstrap resampling. The opponent prior
    is explicit and uniform; the chance prior uses independent source draws.
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
        self._proposal_profiles: set[str] = set()
        self._effective_sample_size = 0.
        self.rebuild()

    def _check(self, started: float):
        if self._cancelled():
            raise SearchBudgetError("belief reconstruction cancelled")
        if (time.monotonic() - started) * 1000 >= self.limits.elapsed_ms:
            raise SearchBudgetError("belief reconstruction time budget exhausted")

    def _seed(self) -> int:
        return int(self._rng.integers(0, 2**32, dtype=np.uint64))

    def _advance(self, position: Any, step: TraceStep, expected: Mapping[str, Any], started: float):
        if _actor(position) != step.events[0]["actor"]:
            return None
        prepare = getattr(self.factory, "prepare_transition", None)
        proposal = prepare(position, expected, self._seed()) if prepare is not None else TransitionProposal(position)
        if proposal is None:
            return None
        if not isinstance(proposal, TransitionProposal):
            raise InformationMismatchError("source transition proposal must carry validated density metadata")
        if proposal.source_probability is not None and _public(proposal.position.observe(self.tracker.viewer)) != _public(position.observe(self.tracker.viewer)):
            raise InformationMismatchError("latent proposal changed the prior public observation")
        self._proposal_profiles.add(proposal.profile)
        position = proposal.position
        log_weight = math.log(proposal.importance_weight)
        compatibility = getattr(self.factory, "transition_compatible", None)

        def compatible(action):
            result = compatibility(position, action, expected) if compatibility is not None else True
            if type(result) is not bool:
                raise InformationMismatchError("source transition compatibility must be boolean")
            return result

        def advance(action):
            child = self.factory.apply_conditioned(position, action, expected, self._seed())
            if child is None:
                return None
            if not isinstance(child, TransitionProposal):
                raise InformationMismatchError("conditioned source transition must carry validated density metadata")
            self._proposal_profiles.add(child.profile)
            if _public(child.position.observe(self.tracker.viewer)) != expected:
                return None
            return child.position, math.log(child.importance_weight)

        known = canonical_json(step.own_intent) if step.own_intent is not None else None
        if known is not None:
            if _actor(position) != self.tracker.viewer:
                return None
            action = position.bind_public_intent(step.own_intent)
            if not compatible(action):
                return None
            child = advance(action)
            if child is None:
                return None
            return child[0], log_weight + child[1]
        selected, log_mass, exhausted = None, -math.inf, False
        seen: set[str] = set()
        bind_streamed = getattr(self.factory, "bind_streamed_public_intent", None)
        for actions, exhausted in _stream(position, self.limits.page_size, self.limits.actions_per_transition):
            if not actions:
                self._check(started)
            for action in actions:
                self._check(started)
                intent = _intent(action)
                key = canonical_json(intent)
                if key in seen:
                    continue
                seen.add(key)
                bound = (bind_streamed(position, action, intent) if bind_streamed is not None
                         else position.bind_public_intent(intent))
                # Provably incompatible actions retain their prior mass in the
                # denominator. Source rules, rather than Python heuristics,
                # decide whether their effects need to be evaluated.
                if not compatible(bound):
                    continue
                child = advance(bound)
                if child is not None:
                    # Source-conditioned chance proposals can have different
                    # corrections for different intents. Keep the uniform
                    # intent prior, but select a child by its corrected mass.
                    log_mass = float(np.logaddexp(log_mass, child[1]))
                    if self._rng.random() < math.exp(child[1] - log_mass):
                        selected = child[0]
        if not exhausted:
            raise SearchBudgetError("belief action enumeration is incomplete; a partial posterior is not accepted")
        return (selected, log_weight + log_mass - math.log(len(seen))) if selected is not None else None

    def _resample(self, weighted):
        if not weighted:
            raise ParticleExhaustedError("no source-valid weighted particles reproduce the public trace")
        logs = np.asarray([weight for _, weight in weighted], dtype=np.float64)
        if not np.isfinite(logs).all():
            raise InformationMismatchError("source posterior log weights must be finite")
        weights = np.exp(logs - logs.max())
        weights /= weights.sum()
        # Stratified bootstrap resampling keeps equal-weight populations
        # diverse without the ordering correlation of one shared offset.
        thresholds = (self._rng.random(self.limits.particles) + np.arange(self.limits.particles)) / self.limits.particles
        indices = np.searchsorted(np.cumsum(weights), thresholds, side="right")
        particles = [weighted[int(index)][0] for index in indices]
        return particles, float(1. / np.square(weights).sum())

    def rebuild(self) -> None:
        started = time.monotonic()
        initial = self.tracker.initial
        weighted: list[tuple[Any, float]] = []
        proposals = 0
        while len(weighted) < self.limits.particles and proposals < self.limits.proposals:
            self._check(started)
            proposals += 1
            position = self.factory.sample_initial(initial, self._seed())
            if position is None:
                continue
            if _public(position.observe(self.tracker.viewer)) != initial:
                raise InformationMismatchError("source-conditioned initial particle does not match the full public frame")
            log_weight = 0.
            for step, frame in self.tracker.frames():
                advanced = self._advance(position, step, frame, started)
                if advanced is None:
                    position = None
                    break
                position, transition_weight = advanced
                log_weight += transition_weight
            if position is not None:
                weighted.append((position, log_weight))
        if not weighted:
            raise ParticleExhaustedError("no source-valid particles reproduce the complete public trace within the finite proposal budget")
        particles, effective_size = self._resample(weighted)
        self._particles, self._effective_sample_size = particles, effective_size
        self._revision = self.tracker.steps
        self.proposals_used = proposals

    def synchronize(self) -> None:
        if self._revision == self.tracker.steps:
            return
        if self._revision > self.tracker.steps:
            raise InformationMismatchError("public tracker revision went backwards")
        started = time.monotonic()
        surviving = self._particles
        for step, frame in self.tracker._frames_since(self._revision):
            weighted = [child for position in surviving if (child := self._advance(position, step, frame, started)) is not None]
            if not weighted:
                self.rebuild()
                return
            surviving, effective_size = self._resample(weighted)
        self._effective_sample_size = effective_size
        self._particles, self._revision = surviving, self.tracker.steps

    def draw(self):
        if not self._particles:
            raise ParticleExhaustedError("the public belief contains no particles")
        if self._revision != self.tracker.steps:
            raise InformationMismatchError("synchronize public belief before search")
        return self._particles[int(self._rng.integers(len(self._particles)))]

    @property
    def summary(self) -> dict[str, Any]:
        return {"version": "public-particle-summary-v3", "particle_count": len(self._particles),
                "distinct_particle_instances": len({id(position) for position in self._particles}),
                "trace_steps": self._revision, "opponent_action_prior": "uniform-public-intents",
                "filter_version": "source-importance-filter-v2", "chance_prior": "independent-source-draws",
                "conditional_steps": "source-weighted-conditional-step-v1",
                "proposal_profiles": sorted(self._proposal_profiles), "effective_sample_size": self._effective_sample_size}


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
