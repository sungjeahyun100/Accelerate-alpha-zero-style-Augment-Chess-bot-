"""Source-bound Python client for the Augment Chess adapter object.

The native session owns private state and transactions. This client sends the
exact installed descriptor selection on every call; only source-valid public
observations and intents cross into Python model/search input.
"""

from __future__ import annotations

from dataclasses import dataclass
from copy import deepcopy
from functools import lru_cache
from hashlib import sha256
from threading import RLock
from typing import Any, Mapping
import json
import math
import warnings

from .encoding import canonical_json, validate_public_move_intent
from .ir import (ObservationIR, PUBLIC_OBSERVATION_VERSION, TypedEncoderSpec,
                 V7_RULES_VERSION, _reject_private as _reject_private_semantics)


_CAPABILITIES = {
    "public-observation": {"observe": "read_only"},
    "public-actions": {"legal-actions": "read_only",
                       "legal-actions-page": "read_only",
                       "bind-public-intent": "read_only",
                       "apply-public-intent": "transactional"},
}
_PAYLOAD_KIND = {
    "observe": "observation",
    "legal-actions": "legal_actions",
    "legal-actions-page": "legal_actions_page",
    "bind-public-intent": "bound_public_intent",
    "apply-public-intent": "applied_public_intent",
}


def _owned(value: Any) -> Any:
    return json.loads(canonical_json(value))


@lru_cache(maxsize=1)
def _installed_provenance() -> tuple[str, str, str]:
    from ._native import RULES_VERSION_V7, site_catalog, site_observation_policy

    if RULES_VERSION_V7 != V7_RULES_VERSION:
        raise ValueError("installed native rules version differs from the pinned v7 public contract")
    catalog = site_catalog(V7_RULES_VERSION)
    policy = site_observation_policy(V7_RULES_VERSION)
    if catalog.get("rulesVersion") != V7_RULES_VERSION or policy.get("rulesVersion") != V7_RULES_VERSION:
        raise ValueError("installed native catalog or observation policy has another rules provenance")
    return (catalog["catalogVersion"], sha256(canonical_json(catalog).encode()).hexdigest(),
            sha256(canonical_json(policy).encode()).hexdigest())


def _descriptors(session: Any) -> dict[str, tuple[dict[str, Any], dict[str, dict[str, Any]]]]:
    descriptors = session.descriptors()
    if not isinstance(descriptors, list) or len(descriptors) != len(_CAPABILITIES):
        raise ValueError("native game adapter descriptor set differs from the pinned v7 client")
    selected = {}
    for descriptor in descriptors:
        if (not isinstance(descriptor, dict)
                or set(descriptor) != {"projectId", "adapterId", "contractVersion", "implementationVersion",
                                       "capabilities", "deterministic", "callLimits"}
                or descriptor.get("projectId") != "augment-chess"
                or descriptor.get("contractVersion") != {"major": 1, "minor": 0}
                or descriptor.get("deterministic") is not True
                or not isinstance(descriptor.get("implementationVersion"), str)
                or not descriptor["implementationVersion"].startswith("v7-e5ed84fcf8e72a24-")):
            raise ValueError("native game adapter identity or contract version mismatch")
        adapter_id = descriptor.get("adapterId")
        if adapter_id not in _CAPABILITIES or adapter_id in selected:
            raise ValueError("native game adapter identity is duplicated or unknown")
        limits = descriptor.get("callLimits")
        if (not isinstance(limits, dict) or set(limits) != {"maxWork", "maxResults"}
                or any(type(limits[name]) is not int or not 1 <= limits[name] <= 1_000_000
                       for name in limits)):
            raise ValueError("native game adapter call limits are invalid")
        capabilities = descriptor.get("capabilities")
        if not isinstance(capabilities, list) or len(capabilities) != len(_CAPABILITIES[adapter_id]):
            raise ValueError("native game adapter capabilities differ from the pinned v7 client")
        by_id = {}
        for capability in capabilities:
            if not isinstance(capability, dict) or set(capability) != {"id", "access", "requestSchema", "responseSchema"}:
                raise ValueError("native game adapter capability descriptor is invalid")
            cap_id = capability.get("id")
            if (cap_id not in _CAPABILITIES[adapter_id] or cap_id in by_id
                    or capability.get("access") != _CAPABILITIES[adapter_id][cap_id]):
                raise ValueError("native game adapter capability or access is unknown")
            for schema_name in ("requestSchema", "responseSchema"):
                schema = capability.get(schema_name)
                if (not isinstance(schema, dict) or set(schema) != {"id", "sha256"}
                        or not isinstance(schema["id"], str) or not schema["id"].startswith("urn:augment-chess:adapter:")
                        or not isinstance(schema["sha256"], str) or len(schema["sha256"]) != 64
                        or any(character not in "0123456789abcdef" for character in schema["sha256"])):
                    raise ValueError("native game adapter schema reference is invalid")
            by_id[cap_id] = capability
        selected[adapter_id] = (descriptor, by_id)
    return selected


@dataclass(frozen=True)
class _PublicIntentAction:
    """A host-checked public choice bound to one source Position revision."""

    intent_json: str
    revision: str

    def public_intent(self) -> dict[str, Any]:
        return json.loads(self.intent_json)


def _action(intent: Mapping[str, Any], revision: str) -> _PublicIntentAction:
    return _PublicIntentAction(canonical_json(intent), revision)


def _independent_seed(seed: int) -> int:
    if type(seed) is not int or not 0 <= seed < 2**32:
        raise ValueError("independent v7 particle seed must be uint32")
    return seed


@dataclass(frozen=True)
class _AdapterStep:
    position: GameAdapterClient
    actor: str
    turn_changed: bool
    result: str | None


class _ActionStream:
    def __init__(self, client: GameAdapterClient):
        self._client = client
        self._revision = client.snapshot_revision
        self._cursor: str | None = None
        self._exhausted = False
        self._lock = RLock()

    def next_page(self, limit: int) -> dict[str, Any]:
        if type(limit) is not int or not 1 <= limit <= 4096:
            raise ValueError("game adapter action page limit must be within 1..=4096")
        with self._lock:
            if self._exhausted:
                return {"actions": (), "examined": 0, "exhausted": True}
            page = self._client.legal_intents_page(limit=limit, max_examined=limit,
                                                   cursor=self._cursor,
                                                   _expected_revision=self._revision)
            actions = tuple(_action(intent, self._revision) for intent in page["intents"])
            self._cursor, self._exhausted = page["cursor"], page["exhausted"]
            return {"actions": actions, "examined": page["examined"],
                    "exhausted": self._exhausted}


class GameAdapterClient:
    """A branchable host client; revision is transport control, never a feature."""

    def __init__(self, session: Any, spec: TypedEncoderSpec):
        from ._native import GameAdapterSession

        if type(session) is not GameAdapterSession:
            raise TypeError("game adapter client requires an installed native GameAdapterSession")
        if (not isinstance(spec, TypedEncoderSpec) or spec.rules_version != V7_RULES_VERSION
                or spec.observation_version != PUBLIC_OBSERVATION_VERSION):
            raise ValueError("game adapter client needs a source-bound v7 public typed spec")
        spec.legacy_validator()
        if (spec.catalog_version, spec.catalog_hash, spec.observation_policy_hash) != _installed_provenance():
            raise ValueError("game adapter typed spec catalog or observation policy differs from the installed native source contract")
        self._session = session
        self.spec = spec
        self._selected = _descriptors(session)
        self._lock = RLock()
        self._sequence = 0
        self._observed_revision: str | None = None
        self._observations: dict[str, dict[str, Any]] = {}

    @classmethod
    def new_game(cls, config: Mapping[str, Any], seed: int, *, spec: TypedEncoderSpec | None = None) -> GameAdapterClient:
        if not isinstance(config, Mapping):
            raise TypeError("game adapter configuration must be a public mapping")
        if type(seed) is not int or not 0 <= seed < 2**32:
            raise ValueError("v7 game seed must be uint32")
        _reject_private_semantics(config)
        from ._native import GameAdapterSession, RULES_VERSION_V7, site_catalog, site_observation_policy

        if RULES_VERSION_V7 != V7_RULES_VERSION:
            raise ValueError("installed native rules version differs from the pinned v7 public contract")
        if spec is None:
            spec = TypedEncoderSpec.from_catalog(site_catalog(RULES_VERSION_V7),
                                                  observation_policy=site_observation_policy(RULES_VERSION_V7))
        return cls(GameAdapterSession.new_game(_owned(config), seed, rules_version=RULES_VERSION_V7), spec)

    @classmethod
    def sample_initial_public(cls, config: Mapping[str, Any], public_observation: Mapping[str, Any],
                              independent_seed: int, *, spec: TypedEncoderSpec) -> GameAdapterClient:
        from ._native import GameAdapterSession

        _independent_seed(independent_seed)
        if not isinstance(config, Mapping):
            raise TypeError("game adapter configuration must be a public mapping")
        _reject_private_semantics(config)
        ObservationIR.from_public(public_observation, spec)
        return cls(GameAdapterSession.sample_initial_public(_owned(config), _owned(public_observation),
                                                             independent_seed), spec)

    @property
    def snapshot_revision(self) -> str:
        return self._session.snapshot_revision

    @property
    def decision_actor(self) -> str:
        actor = self._session.decision_actor
        if actor not in ("white", "black"):
            raise ValueError("native game adapter returned an invalid decision actor")
        return actor

    @property
    def result(self) -> str | None:
        outcome = self._session.result
        if outcome not in (None, "white", "black", "draw"):
            raise ValueError("native game adapter returned an invalid game result")
        return outcome

    def fork(self) -> GameAdapterClient:
        with self._lock:
            revision = self.snapshot_revision
            return type(self)(self._session.fork(snapshot_revision=revision), self.spec)

    def _call(self, adapter_id: str, capability_id: str, payload: Mapping[str, Any],
              *, expected_revision: str | None = None) -> dict[str, Any]:
        with self._lock:
            descriptor, capabilities = self._selected[adapter_id]
            capability = capabilities[capability_id]
            self._sequence += 1
            if self._sequence > 1_000_000_000:
                raise ValueError("game adapter request ID budget exhausted")
            request_id = f"python-{self._sequence}"
            request = {"requestId": request_id, "projectId": descriptor["projectId"],
                       "adapterId": descriptor["adapterId"],
                       "contractVersion": descriptor["contractVersion"],
                       "implementationVersion": descriptor["implementationVersion"],
                       "capabilityId": capability_id,
                       "requestSchema": capability["requestSchema"],
                       "responseSchema": capability["responseSchema"],
                       "snapshotRevision": (self._session.snapshot_revision
                                            if expected_revision is None else expected_revision),
                       "limits": descriptor["callLimits"], "payload": _owned(payload)}
            response = self._session.invoke(request)
            if (not isinstance(response, dict)
                    or set(response) != {"requestId", "responseSchema", "snapshotRevision",
                                         "result", "page", "diagnostics"}
                    or response["requestId"] != request_id
                    or response["responseSchema"] != capability["responseSchema"]
                    or response["snapshotRevision"] != self._session.snapshot_revision):
                raise ValueError("native game adapter response identity, revision or page mismatch")
            diagnostics = response["diagnostics"]
            if not isinstance(diagnostics, list):
                raise ValueError("native game adapter diagnostics are invalid")
            for diagnostic in diagnostics:
                if (not isinstance(diagnostic, dict) or set(diagnostic) != {"severity", "code", "message"}
                        or diagnostic["severity"] not in ("info", "warning")
                        or not isinstance(diagnostic["code"], str)
                        or not isinstance(diagnostic["message"], str)):
                    raise ValueError("native game adapter diagnostic shape is invalid")
                warnings.warn(f"{diagnostic['severity']} [{diagnostic['code']}]: {diagnostic['message']}",
                              stacklevel=2)
            result = response["result"]
            if not isinstance(result, dict) or result.get("kind") != _PAYLOAD_KIND[capability_id]:
                raise ValueError("native game adapter returned a result for another capability")
            page = response["page"]
            if capability_id == "legal-actions-page":
                if (not isinstance(page, dict) or set(page) != {"examined", "cursor", "exhausted"}
                        or type(page["examined"]) is not int or page["examined"] < 0
                        or type(page["exhausted"]) is not bool
                        or page != {"examined": result.get("examined"), "cursor": result.get("cursor"),
                                    "exhausted": result.get("exhausted")}):
                    raise ValueError("native game adapter page accounting differs from its public action page")
            elif page is not None:
                raise ValueError("native game adapter returned page metadata for a nonpaged capability")
            return _owned(result)

    def observe(self, viewer: str) -> dict[str, Any]:
        if viewer not in ("white", "black"):
            raise ValueError("game adapter viewer must be white or black")
        with self._lock:
            revision = self.snapshot_revision
            if self._observed_revision != revision:
                self._observed_revision = revision
                self._observations.clear()
            if viewer not in self._observations:
                result = self._call("public-observation", "observe", {"kind": "observe", "viewer": viewer})
                if set(result) != {"kind", "observation"}:
                    raise ValueError("native game adapter observation result has unknown fields")
                observation = result["observation"]
                ObservationIR.from_public(observation, self.spec)
                if self.snapshot_revision != revision:
                    raise ValueError("game adapter revision changed while observing")
                self._observations[viewer] = observation
            return deepcopy(self._observations[viewer])

    def bind_public_intent(self, intent: Mapping[str, Any]) -> _PublicIntentAction:
        if not isinstance(intent, Mapping):
            raise TypeError("game adapter public intent must be a mapping")
        _reject_private_semantics(intent)
        owned = _owned(intent)
        _reject_private_semantics(owned)
        validate_public_move_intent(owned, V7_RULES_VERSION)
        with self._lock:
            revision = self.snapshot_revision
            result = self._call("public-actions", "bind-public-intent",
                                {"kind": "bind_public_intent", "intent": owned},
                                expected_revision=revision)
            if set(result) != {"kind", "intent"} or result["intent"] != owned:
                raise ValueError("native host changed the public intent fields or ordered selections")
            if revision != self.snapshot_revision:
                raise ValueError("game adapter revision changed while binding a public intent")
            return _action(owned, revision)

    def legal_intents(self) -> tuple[dict[str, Any], ...]:
        result = self._call("public-actions", "legal-actions", {"kind": "legal_actions"})
        if set(result) != {"kind", "intents"} or not isinstance(result["intents"], list):
            raise ValueError("native game adapter legal action result is invalid")
        return self._validated_intents(result["intents"])

    @staticmethod
    def _validated_intents(intents: list[Any]) -> tuple[dict[str, Any], ...]:
        for intent in intents:
            if not isinstance(intent, dict) or not isinstance(intent.get("type"), str):
                raise ValueError("native game adapter returned an invalid public intent")
            _reject_private_semantics(intent)
            validate_public_move_intent(intent, V7_RULES_VERSION)
        keys = [canonical_json(intent) for intent in intents]
        if len(keys) != len(set(keys)):
            raise ValueError("native game adapter emitted duplicate public intents")
        # The host has already admitted the complete ordered set. Bind one
        # selected intent again only when it is used against a Position; that
        # preserves stale-revision checks without N extra full-set traversals.
        return tuple(intents)

    def legal_intents_page(self, *, limit: int = 64, max_examined: int = 65_536,
                           cursor: str | None = None,
                           _expected_revision: str | None = None) -> dict[str, Any]:
        if type(limit) is not int or not 1 <= limit <= 4096:
            raise ValueError("game adapter action page limit must be within 1..=4096")
        if type(max_examined) is not int or not 1 <= max_examined <= 65_536:
            raise ValueError("game adapter action examination budget must be within 1..=65536")
        if cursor is not None and (not isinstance(cursor, str) or not cursor):
            raise ValueError("game adapter cursor must be a nonempty opaque string or None")
        result = self._call("public-actions", "legal-actions-page",
                            {"kind": "legal_actions_page", "limit": limit,
                             "max_examined": max_examined, "cursor": cursor},
                            expected_revision=_expected_revision)
        if (set(result) != {"kind", "intents", "examined", "exhausted", "stop_reason", "cursor"}
                or not isinstance(result["intents"], list)
                or len(result["intents"]) > limit
                or type(result["examined"]) is not int or not 0 <= result["examined"] <= max_examined
                or type(result["exhausted"]) is not bool
                or result["stop_reason"] not in ("exhausted", "page-limit", "examined-budget")):
            raise ValueError("native game adapter public action page has invalid fields or exceeds its requested limits")
        exhausted, next_cursor = result["exhausted"], result["cursor"]
        if (exhausted and next_cursor is not None
                or not exhausted and (not isinstance(next_cursor, str) or not next_cursor)
                or exhausted != (result["stop_reason"] == "exhausted")
                or result["stop_reason"] == "page-limit" and len(result["intents"]) != limit
                or result["stop_reason"] == "examined-budget" and result["examined"] != max_examined):
            raise ValueError("native game adapter public action cursor or stop reason is inconsistent")
        if not exhausted and result["examined"] == 0 and not result["intents"]:
            raise ValueError("native game adapter public action page made no progress")
        result["intents"] = list(self._validated_intents(result["intents"]))
        return result

    def action_stream(self) -> _ActionStream:
        with self._lock:
            # The host cursor owns one immutable source snapshot. Advancing the
            # environment later cannot change this stream or its issued actions.
            return _ActionStream(self.fork())

    def apply_public_intent(self, intent: Mapping[str, Any]) -> dict[str, Any]:
        with self._lock:
            action = self.bind_public_intent(intent)
            return self._apply_exact_intent(action.public_intent(), expected_revision=action.revision)

    def _apply_exact_intent(self, owned: Mapping[str, Any],
                            *, expected_revision: str | None = None) -> dict[str, Any]:
        _reject_private_semantics(owned)
        validate_public_move_intent(owned, V7_RULES_VERSION)
        actor = self.decision_actor
        previous_revision = self.snapshot_revision if expected_revision is None else expected_revision
        result = self._call("public-actions", "apply-public-intent",
                            {"kind": "apply_public_intent", "intent": owned},
                            expected_revision=previous_revision)
        if set(result) != {"kind", "actor", "turn_changed", "result"}:
            raise ValueError("native game adapter apply result has unknown fields")
        if (result["actor"] != actor or type(result["turn_changed"]) is not bool
                or result["result"] not in (None, "white", "black", "draw")
                or self.snapshot_revision == previous_revision):
            raise ValueError("native game adapter apply result or committed revision is invalid")
        self._observations.clear()
        self._observed_revision = None
        return result

    def apply(self, action: _PublicIntentAction) -> _AdapterStep:
        with self._lock:
            if type(action) is not _PublicIntentAction or action.revision != self.snapshot_revision:
                raise ValueError("public intent action belongs to another game adapter revision")
            branch = self.fork()
            if branch.snapshot_revision != action.revision:
                raise ValueError("game adapter revision changed while forking a public intent action")
        # The immutable action was issued at this revision; the native write
        # transaction still rebinds it against its working Position.
        result = branch._apply_exact_intent(action.public_intent(), expected_revision=action.revision)
        return _AdapterStep(branch, result["actor"], result["turn_changed"], result["result"])

    def condition_hidden_opening_draft(self, expected_next_public: Mapping[str, Any],
                                       independent_seed: int) -> dict[str, Any]:
        _independent_seed(independent_seed)
        ObservationIR.from_public(expected_next_public, self.spec)
        with self._lock:
            revision = self.snapshot_revision
            proposal = self._session.condition_hidden_opening_draft(_owned(expected_next_public),
                                                                     independent_seed,
                                                                     snapshot_revision=revision)
        return self._proposal(proposal)

    def condition_hidden_stage_draft(self, expected_next_public: Mapping[str, Any],
                                     independent_seed: int) -> dict[str, Any]:
        _independent_seed(independent_seed)
        ObservationIR.from_public(expected_next_public, self.spec)
        with self._lock:
            revision = self.snapshot_revision
            proposal = self._session.condition_hidden_stage_draft(_owned(expected_next_public),
                                                                   independent_seed,
                                                                   snapshot_revision=revision)
        return self._proposal(proposal)

    def public_transition_compatible(self, action: _PublicIntentAction,
                                     expected_next_public: Mapping[str, Any]) -> bool:
        ObservationIR.from_public(expected_next_public, self.spec)
        with self._lock:
            intent = self._checked_action(action)
            result = self._session.public_transition_compatible(intent, _owned(expected_next_public),
                                                                  snapshot_revision=action.revision)
        if type(result) is not bool:
            raise ValueError("native game adapter compatibility response is not boolean")
        return result

    def public_delta_candidate_intents(self, origin: Mapping[str, Any],
                                       destination: Mapping[str, Any]) -> dict[str, Any]:
        for square in (origin, destination):
            if (not isinstance(square, Mapping) or set(square) != {"row", "col"}
                    or any(type(square[key]) is not int or not 0 <= square[key] < 8
                           for key in ("row", "col"))):
                raise ValueError("public delta square must be an 8x8 coordinate")
        with self._lock:
            revision = self.snapshot_revision
            result = self._session.public_delta_candidate_intents(
                _owned(origin), _owned(destination), snapshot_revision=revision)
        if (not isinstance(result, dict) or set(result) != {"intents", "legal_count", "examined"}
                or not isinstance(result["intents"], list)
                or type(result["legal_count"]) is not int or result["legal_count"] < 1
                or type(result["examined"]) is not int or result["examined"] < 0
                or len(result["intents"]) > result["legal_count"]):
            raise ValueError("native public delta candidates have invalid accounting")
        result["intents"] = list(self._validated_intents(result["intents"]))
        # These intents came from the source-admitted cursor for this exact
        # revision. Compatibility and apply rebind and revalidate each chosen
        # action, as with action_stream(), without a separate Python bind call.
        result["actions"] = tuple(_action(intent, revision) for intent in result["intents"])
        return result

    def apply_weighted_conditioned_public(self, action: _PublicIntentAction,
                                          expected_next_public: Mapping[str, Any],
                                          independent_seed: int) -> dict[str, Any]:
        _independent_seed(independent_seed)
        ObservationIR.from_public(expected_next_public, self.spec)
        with self._lock:
            intent = self._checked_action(action)
            proposal = self._session.apply_weighted_conditioned_public(intent,
                           _owned(expected_next_public), independent_seed,
                           snapshot_revision=action.revision)
        return self._proposal(proposal)

    def _checked_action(self, action: _PublicIntentAction) -> dict[str, Any]:
        if type(action) is not _PublicIntentAction or action.revision != self.snapshot_revision:
            raise ValueError("public intent action belongs to another game adapter revision")
        intent = action.public_intent()
        _reject_private_semantics(intent)
        validate_public_move_intent(intent, V7_RULES_VERSION)
        return intent

    def _proposal(self, proposal: Mapping[str, Any]) -> dict[str, Any]:
        from ._native import GameAdapterSession

        if not isinstance(proposal, Mapping) or set(proposal) != {
                "position", "importance_weight", "source_probability", "proposal_probability"}:
            raise ValueError("native v7 source proposal metadata has an invalid shape")
        if type(proposal["position"]) is not GameAdapterSession:
            raise ValueError("native v7 source proposal has no game adapter position")
        weight, source, proposed = (proposal[name] for name in (
            "importance_weight", "source_probability", "proposal_probability"))
        if (any(type(value) not in (int, float) or not math.isfinite(value) or value <= 0
                for value in (weight, source, proposed))
                or source > 1 or proposed > 1
                or not math.isclose(weight, source / proposed, rel_tol=1e-10, abs_tol=0.)):
            raise ValueError("native v7 source proposal has invalid p/q density metadata")
        return {"position": type(self)(proposal["position"], self.spec),
                "importance_weight": weight, "source_probability": source,
                "proposal_probability": proposed}
