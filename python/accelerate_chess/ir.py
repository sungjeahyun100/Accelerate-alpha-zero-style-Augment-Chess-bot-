"""Versioned, typed features derived only from a viewer's public projection.

This module is intentionally separate from the fixed 8x8 public-utf8-v2
encoder.  The source observation is validated before a semantic tree is built;
arbitrary identifiers and information-state hashes are binding metadata, not
model features.  Numeric and category tensors never contain serialized JSON.
"""

from __future__ import annotations

from dataclasses import dataclass, field, replace
from hashlib import sha256
import json
import math
import re
from typing import Any, Mapping, Sequence

import numpy as np

from .encoding import (
    EncoderSpec,
    PublicEncoder,
    PublicObservation,
    SOURCE_PROJECTIONS,
    canonical_json,
)


IR_VERSION = "semantic-ir-v1"
ENCODER_VERSION = "typed-input-v1"
DESCRIPTOR_VERSION = "move-program-v1"
HISTORY_VERSION = "public-history-summary-v2"
BELIEF_VERSION = "public-particle-summary-v3"
BELIEF_PROPOSAL_PROFILES = (
    "source-prior-v1", "source-weighted-conditional-step-v1",
    "source-weighted-offer-proposal-v1",
)
SYNTHETIC_OBSERVATION_VERSION = "synthetic-geometry-v1"
PUBLIC_OBSERVATION_VERSION = "accelerate-observation-v2"
V7_RULES_VERSION = "augment-site-20260928-e5ed84fcf8e72a24"
SPATIAL_CHANNELS = ("empty", "unknown", "hole", "occupied", "own", "opponent")
RECORD_CATEGORY_SLOTS = ("kind", "field", "symbol", "owner")
RELATION_CATEGORY_SLOTS = ("kind", "field")
CANDIDATE_CATEGORY_SLOTS = RECORD_CATEGORY_SLOTS
RECORD_NUMERIC_SLOTS = ("scalar", "has_scalar", "span_length", "ordinal", "row_start", "col_start", "row_end", "col_end")
RELATION_NUMERIC_SLOTS = ("ordinal", "has_ordinal", "delta_row", "delta_col")
CANDIDATE_NUMERIC_SLOTS = ("scalar", "has_scalar", "child_count", "depth", "row_start", "col_start", "row_end", "col_end")
CONDITION_SLOTS = ("viewer_white", "turn_is_viewer", "actions_remaining_16", "move_count_512", "full_move_256", "opponent_hand_count_16", "own_card_count_32", "history_event_count_512")
INPUT_ORDER_B = (
    "record_category", "record_numeric", "record_coord", "record_spatial_valid", "record_mask",
    "relation_index", "relation_category", "relation_numeric", "relation_mask",
    "candidate_category", "candidate_numeric", "candidate_coord", "candidate_coord_valid",
    "candidate_parent", "candidate_order", "candidate_target_index", "candidate_node_mask",
    "candidate_mask", "condition",
)
INPUT_ORDER_A = ("spatial", "layout_mask", *INPUT_ORDER_B)
_INPUTS = {
    "spatial": ("float32", ["batch", len(SPATIAL_CHANNELS), "height", "width"]),
    "layout_mask": ("bool", ["batch", 1, "height", "width"]),
    "record_category": ("int64", ["batch", "records", 4]),
    "record_numeric": ("float32", ["batch", "records", 8]),
    "record_coord": ("float32", ["batch", "records", 2]),
    "record_spatial_valid": ("bool", ["batch", "records"]),
    "record_mask": ("bool", ["batch", "records"]),
    "relation_index": ("int64", ["batch", "relations", 2]),
    "relation_category": ("int64", ["batch", "relations", 2]),
    "relation_numeric": ("float32", ["batch", "relations", 4]),
    "relation_mask": ("bool", ["batch", "relations"]),
    "candidate_category": ("int64", ["batch", "actions", "nodes", 4]),
    "candidate_numeric": ("float32", ["batch", "actions", "nodes", 8]),
    "candidate_coord": ("float32", ["batch", "actions", "nodes", 2]),
    "candidate_coord_valid": ("bool", ["batch", "actions", "nodes"]),
    "candidate_parent": ("int64", ["batch", "actions", "nodes"]),
    "candidate_order": ("int64", ["batch", "actions", "nodes"]),
    "candidate_target_index": ("int64", ["batch", "actions", "nodes"]),
    "candidate_node_mask": ("bool", ["batch", "actions", "nodes"]),
    "candidate_mask": ("bool", ["batch", "actions"]),
    "condition": ("float32", ["batch", len(CONDITION_SLOTS)]),
}

_SYMBOLS = {
    "", "global", "cell-run", "piece", "object", "array", "scalar", "coordinate", "intent",
    "contains", "array-element", "same-identity", "semantic-link", "occupies", "history", "descriptor",
    "observation", "cell", "publicState", "ownCards", "revealedOpponentCards", "belief",
    "empty", "unknown", "hole", "occupied", "white", "black", "neutral", "true", "false", "null",
    "instance-reference", "value", "root", "from", "to", "move", "destination", "target",
    "selection", "selections", "orderedTargets", "cardInstanceId", "pieceId", "instanceId",
    "cardId", "cardInstanceIds", "bundleIndex", "doomedIndex", "jumpCapture", "capture", "captured", "promotionType", "pieceType",
    "choice", "choices", "cardSlot", "targetType", "targetColor", "destinationType",
    "directionChoice", "spell", "reload", "bundle", "index", "slot", "count",
    "row", "col", "type", "color", "kind", "square", "boardChanges", "before", "after",
    "actor", "nextActor", "phase", "result", "outcome", "captures", "count", "geometry",
    "originRow", "originCol", "height", "width", "valid", "visible", "collapsed",
    "event_count", "actor_counts", "decision_actor_changes", "board_change_count",
    "recent_events", "board_change_squares", "own_card_count", "opponent_card_count",
    "public-history-summary-v2", "public-particle-summary-v3", "uniform-public-intents",
    "source-importance-filter-v2", "independent-source-draws", "source-prior-v1",
    "source-weighted-conditional-step-v1", "source-weighted-offer-proposal-v1",
    "version", "particle_count", "distinct_particle_instances", "trace_steps",
    "opponent_action_prior", "filter_version", "chance_prior", "conditional_steps",
    "proposal_profiles", "effective_sample_size", "transition", "ongoing", "terminal", "draw",
    "normal", "chaos", "grand", "opening", "middle", "end", "play", "draft",
    "OPENING", "MIDDLE", "END", "GRAND", "gameover", "premove", "moves", "destinations",
    "cardTargets", "targets", "triggerColor", "triggerTurn", "enabled", "initialMs",
    "incrementMs", "whiteMs", "blackMs", "runningColor", "timeoutWinner", "timeoutLoser",
    "primitive", "direction", "maxDistance", "activationCondition",
    "activateAtParentDistance", "children", "modifierId", "sourceId", "source",
    "expiration", "roots", "base", "modifiers", "program", "dr", "dc", "remaining",
    "Any", "NoCapture", "MustCapture", "permanent", "actions", "ownerTurns", "activations",
    "MOVE", "TAKE", "TAKEMOVE", "BOTHTAKEMOVE", "CATCH", "JUMP", "SHIFT",
}
_IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z0-9_.:-]{0,63}\Z")
_PRIVATE_NAMES = {
    "rng", "rngstate", "randomstate", "seed", "randomtape", "opponentcards",
    "positionkey", "positionid", "actionid", "windowid", "hiddenstate", "privatecards",
    "actualposition", "fullposition", "privateposition", "informationstatekey",
}
_INSTANCE_FIELDS = {"instanceId", "cardInstanceId", "pieceId", "sourcePieceId", "targetPieceId",
                    "sourceId", "modifierId"}
_CARD_REF_FIELDS = {"instanceId", "cardInstanceId"}
_CELL_KINDS = frozenset({"empty", "unknown", "hole", "piece"})


def _hash(value: Any) -> str:
    return sha256(canonical_json(value).encode("utf-8")).hexdigest()


def _reject_private(value: Any, *, allow_identity: bool = False, depth: int = 0) -> None:
    if depth > 64:
        raise ValueError("public semantic input exceeds nesting limit")
    if isinstance(value, Mapping):
        for key, item in value.items():
            if not isinstance(key, str):
                raise ValueError("public semantic object keys must be strings")
            normalized = key.lower().replace("_", "")
            if normalized in _PRIVATE_NAMES and not (allow_identity and normalized == "informationstatekey"):
                raise ValueError(f"private field {key!r} cannot enter semantic features")
            _reject_private(item, depth=depth + 1)
    elif isinstance(value, (list, tuple)):
        for item in value:
            _reject_private(item, depth=depth + 1)


def _validate_belief_summary(summary: Mapping[str, Any]) -> None:
    expected = {"version", "particle_count", "distinct_particle_instances", "trace_steps",
                "opponent_action_prior", "filter_version", "chance_prior", "conditional_steps",
                "proposal_profiles", "effective_sample_size"}
    if not isinstance(summary, Mapping) or set(summary) != expected:
        raise ValueError("typed public belief summary has an unknown shape")
    fixed = {"version": BELIEF_VERSION, "opponent_action_prior": "uniform-public-intents",
             "filter_version": "source-importance-filter-v2",
             "chance_prior": "independent-source-draws",
             "conditional_steps": "source-weighted-conditional-step-v1"}
    if any(summary[key] != value for key, value in fixed.items()):
        raise ValueError("typed public belief summary version or prior differs from the contract")
    for key in ("particle_count", "distinct_particle_instances", "trace_steps"):
        if type(summary[key]) is not int or summary[key] < 0:
            raise ValueError(f"typed public belief {key} must be a nonnegative integer")
    if summary["distinct_particle_instances"] > summary["particle_count"]:
        raise ValueError("typed public belief distinct count exceeds its particle count")
    effective = summary["effective_sample_size"]
    if (type(effective) not in (int, float) or not math.isfinite(effective)
            or not 0 <= effective <= summary["particle_count"] + 1e-6):
        raise ValueError("typed public belief effective sample size is invalid")
    profiles = summary["proposal_profiles"]
    if (not isinstance(profiles, list) or any(not isinstance(item, str) for item in profiles)
            or profiles != sorted(set(profiles))
            or any(item not in BELIEF_PROPOSAL_PROFILES for item in profiles)):
        raise ValueError("unknown typed public belief proposal profile")


def _schema_symbols(schema: Any, into: set[str]) -> None:
    if isinstance(schema, Mapping):
        properties = schema.get("properties")
        if isinstance(properties, Mapping):
            into.update(properties)
        for key in ("enum", "const"):
            values = schema.get(key)
            values = values if isinstance(values, list) else [values]
            for value in values:
                if isinstance(value, str) and _IDENTIFIER.fullmatch(value):
                    into.add(value)
        for key, child in schema.items():
            if key not in ("enum", "const"):
                _schema_symbols(child, into)
    elif isinstance(schema, (list, tuple)):
        for item in schema:
            _schema_symbols(item, into)


def _catalog_symbols(value: Any, into: set[str]) -> None:
    if isinstance(value, Mapping):
        for key, child in value.items():
            if _IDENTIFIER.fullmatch(key):
                into.add(key)
            _catalog_symbols(child, into)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _catalog_symbols(child, into)
    elif isinstance(value, str) and _IDENTIFIER.fullmatch(value):
        into.add(value)


@dataclass(frozen=True)
class TypedEncoderSpec:
    rules_version: str
    catalog_version: str
    catalog_hash: str
    observation_policy_hash: str
    observation_version: str
    category_vocabulary: tuple[str, ...]
    max_board_axis: int = 32
    max_records: int = 2048
    max_relations: int = 8192
    max_candidates: int = 4096
    max_candidate_nodes: int = 64
    max_batch: int = 64
    max_input_bytes: int = 64 * 1024 * 1024
    ir_version: str = IR_VERSION
    descriptor_version: str = DESCRIPTOR_VERSION
    encoder_version: str = ENCODER_VERSION
    history_version: str = HISTORY_VERSION
    _catalog: Mapping[str, Any] | None = field(default=None, repr=False, compare=False)
    _policy: Mapping[str, Any] | None = field(default=None, repr=False, compare=False)
    _vocabulary_ids: Mapping[str, int] = field(default_factory=dict, repr=False, compare=False)
    _legacy_validator: PublicEncoder | None = field(default=None, repr=False, compare=False)

    def __post_init__(self) -> None:
        if (self.ir_version, self.descriptor_version, self.encoder_version, self.history_version) != (IR_VERSION, DESCRIPTOR_VERSION, ENCODER_VERSION, HISTORY_VERSION):
            raise ValueError("unsupported typed feature contract version")
        if self.observation_version not in (PUBLIC_OBSERVATION_VERSION, SYNTHETIC_OBSERVATION_VERSION):
            raise ValueError("unsupported typed observation provenance")
        for name in ("catalog_hash", "observation_policy_hash"):
            digest = getattr(self, name)
            if not isinstance(digest, str) or len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
                raise ValueError(f"{name} needs lowercase SHA-256")
        if not self.rules_version or not self.catalog_version or not self.category_vocabulary or self.category_vocabulary[0] != "":
            raise ValueError("typed spec needs rules/catalog provenance and padding category")
        if len(self.category_vocabulary) > 65_536 or len(set(self.category_vocabulary)) != len(self.category_vocabulary):
            raise ValueError("category vocabulary is duplicated or too large")
        for name, upper in (("max_board_axis", 32), ("max_records", 2048), ("max_relations", 8192),
                            ("max_candidates", 4096), ("max_candidate_nodes", 64), ("max_batch", 64),
                            ("max_input_bytes", 64 * 1024 * 1024)):
            value = getattr(self, name)
            if type(value) is not int or not 1 <= value <= upper:
                raise ValueError(f"{name} exceeds the typed deployment profile")
        object.__setattr__(self, "_vocabulary_ids", {value: index for index, value in enumerate(self.category_vocabulary)})

    @classmethod
    def from_catalog(cls, catalog: Mapping[str, Any], *, observation_policy: Mapping[str, Any],
                     observation_version: str = PUBLIC_OBSERVATION_VERSION,
                     **limits: int) -> TypedEncoderSpec:
        if not isinstance(catalog, Mapping) or catalog.get("schemaVersion") != 1 or not isinstance(observation_policy, Mapping):
            raise ValueError("typed spec needs a source catalog and observation policy")
        rules = catalog.get("rulesVersion")
        if (rules not in SOURCE_PROJECTIONS or observation_policy.get("rulesVersion") != rules
                or observation_policy.get("projectionVersion") != SOURCE_PROJECTIONS[rules]
                or observation_policy.get("protocolVersion") != PUBLIC_OBSERVATION_VERSION):
            raise ValueError("typed spec source projection provenance mismatch")
        symbols = set(_SYMBOLS)
        for key in ("stateValueSchemas", "surfaceSchemas", "publicPieceSchema", "cardRevelationSchema", "selectionSchema", "deathmatchSchema"):
            _schema_symbols(observation_policy.get(key), symbols)
        for key in ("statePublicFields", "derivedPublicFields", "piecePublicFields", "cardPublicFields"):
            symbols.update(observation_policy.get(key, ()))
        _catalog_symbols(catalog, symbols)
        vocabulary = ("", *sorted(symbols - {""}))
        valid_limits = {"max_board_axis", "max_records", "max_relations", "max_candidates",
                        "max_candidate_nodes", "max_batch", "max_input_bytes"}
        if set(limits) - valid_limits:
            raise ValueError("unknown typed encoder limit")
        return cls(rules, str(catalog["catalogVersion"]), _hash(catalog), _hash(observation_policy),
                   observation_version, vocabulary, **limits, _catalog=json.loads(canonical_json(catalog)),
                   _policy=json.loads(canonical_json(observation_policy)))

    def category_id(self, value: str) -> int:
        try:
            return self._vocabulary_ids[value]
        except KeyError as error:
            raise ValueError(f"symbol {value!r} is absent from the versioned public vocabulary") from error

    @property
    def feature_schema(self) -> dict[str, Any]:
        return {
            "inputs": {name: {"dtype": dtype, "shape": shape} for name, (dtype, shape) in _INPUTS.items()},
            "input_order": {"mask-resnet": list(INPUT_ORDER_A), "entity-transformer": list(INPUT_ORDER_B)},
            "category_vocabulary": list(self.category_vocabulary),
            "category_slots": {"record": list(RECORD_CATEGORY_SLOTS), "relation": list(RELATION_CATEGORY_SLOTS), "candidate": list(CANDIDATE_CATEGORY_SLOTS)},
            "numeric_slots": {"record": list(RECORD_NUMERIC_SLOTS), "relation": list(RELATION_NUMERIC_SLOTS), "candidate": list(CANDIDATE_NUMERIC_SLOTS), "condition": list(CONDITION_SLOTS)},
            "spatial_channels": list(SPATIAL_CHANNELS),
            "coordinate_frame": "absolute public Coord mapped to local row/column in current geometry; no viewer rotation",
            "history_version": self.history_version,
            "history_coordinates": "ordered absolute [row,col] pairs; Observation v2 does not supply past event geometry",
            "descriptor_identity": "sourceId, modifierId and modifier source are reference metadata, not category content",
            "card_aliases": "same public instance may appear once per own, revealed-opponent, or draft-choice surface; compatible views link by same-identity",
            "belief_summary": {"version": BELIEF_VERSION, "proposal_profiles": list(BELIEF_PROPOSAL_PROFILES),
                               "opponent_action_prior": "uniform-public-intents",
                               "filter_version": "source-importance-filter-v2",
                               "chance_prior": "independent-source-draws",
                               "conditional_steps": "source-weighted-conditional-step-v1"},
            "limits": {key: getattr(self, key) for key in ("max_board_axis", "max_records", "max_relations", "max_candidates", "max_candidate_nodes", "max_batch", "max_input_bytes")},
            "candidate_tree": "root index zero; child parent index, array order, public record target index or -1",
            "padding": "layout_mask false only for batch padding; hole remains within layout",
        }

    @property
    def feature_schema_hash(self) -> str:
        return _hash(self.feature_schema)

    def to_dict(self) -> dict[str, Any]:
        return {name: list(value) if name == "category_vocabulary" else value
                for name, value in vars(self).items() if not name.startswith("_")}

    @classmethod
    def from_dict(cls, data: Mapping[str, Any], *, catalog: Mapping[str, Any], observation_policy: Mapping[str, Any]) -> TypedEncoderSpec:
        if not isinstance(data, Mapping):
            raise ValueError("typed encoder spec must be an object")
        limit_names = ("max_board_axis", "max_records", "max_relations", "max_candidates",
                       "max_candidate_nodes", "max_batch", "max_input_bytes")
        expected = cls.from_catalog(catalog, observation_policy=observation_policy,
                                    observation_version=data.get("observation_version", ""),
                                    **{name: data[name] for name in limit_names if name in data})
        if dict(data) != expected.to_dict():
            raise ValueError("typed encoder spec differs from source-bound catalog and policy")
        return expected

    @property
    def encoder_metadata(self) -> dict[str, Any]:
        return {"rules_version": self.rules_version, "catalog_version": self.catalog_version,
                "catalog_hash": self.catalog_hash, "observation_policy_hash": self.observation_policy_hash,
                "observation_version": self.observation_version, "ir_version": self.ir_version,
                "descriptor_version": self.descriptor_version, "encoder_version": self.encoder_version,
                "feature_schema": self.feature_schema, "feature_schema_hash": self.feature_schema_hash,
                "value_perspective": "observation.viewer"}

    def contract(self) -> dict[str, Any]:
        return self.encoder_metadata

    @property
    def digest(self) -> str:
        return _hash(self.encoder_metadata)

    def legacy_validator(self) -> PublicEncoder:
        if self._catalog is None or self._policy is None:
            raise ValueError("typed spec needs its source-bound catalog and policy")
        if self._legacy_validator is not None:
            cached = self._legacy_validator.spec
            if (cached.rules_version != self.rules_version
                    or cached.catalog_version != self.catalog_version
                    or cached.catalog_hash != self.catalog_hash
                    or cached.observation_policy_hash != self.observation_policy_hash):
                raise ValueError("cached public validator differs from the typed source contract")
            return self._legacy_validator
        legacy = EncoderSpec.from_catalog(self._catalog, observation_policy=self._policy)
        if legacy.catalog_hash != self.catalog_hash or legacy.observation_policy_hash != self.observation_policy_hash:
            raise ValueError("typed spec source metadata changed")
        validator = PublicEncoder(legacy)
        object.__setattr__(self, "_legacy_validator", validator)
        return validator


@dataclass(frozen=True)
class BoardGeometry:
    origin_row: int
    origin_col: int
    height: int
    width: int

    def __post_init__(self) -> None:
        if any(type(value) is not int for value in (self.origin_row, self.origin_col, self.height, self.width)):
            raise ValueError("geometry coordinates and extents must be integers")
        if not 1 <= self.height <= 32 or not 1 <= self.width <= 32:
            raise ValueError("geometry extents must lie within the typed deployment profile")
        if any(abs(value) > 1_000_000 for value in (self.origin_row, self.origin_col)):
            raise ValueError("geometry origin exceeds the supported coordinate range")

    def local(self, row: int, col: int) -> tuple[int, int]:
        y, x = row - self.origin_row, col - self.origin_col
        if not 0 <= y < self.height or not 0 <= x < self.width:
            raise ValueError(f"coordinate {(row, col)} is outside geometry {self}")
        return y, x

    def normalized(self, row: int, col: int) -> tuple[float, float]:
        y, x = self.local(row, col)
        return (y / max(1, self.height - 1), x / max(1, self.width - 1))


def validate_typed_public_observation(observation: Mapping[str, Any], spec: TypedEncoderSpec,
                                      *, belief_summary: Mapping[str, Any] | None = None) -> PublicObservation:
    """Validate a source-bound v2 frame under the frozen site's 8x8 wire contract.

    Variable geometry belongs to the explicit synthetic component profile;
    accepting it with a source rules/policy identity would claim site parity
    that neither frozen client nor the native Position currently provides.
    """
    if spec.observation_version != PUBLIC_OBSERVATION_VERSION:
        raise ValueError("native v2 observation needs a v2 typed encoder spec")
    owned = json.loads(canonical_json(observation))
    summary = json.loads(canonical_json(belief_summary)) if belief_summary is not None else None
    verified = PublicObservation.from_native(owned, belief_summary=summary)
    public_state = verified.public["publicState"]
    if (public_state.get("projectionVersion") != SOURCE_PROJECTIONS[spec.rules_version]
            or public_state.get("observationPolicyHash") != spec.observation_policy_hash
            or public_state.get("rulesVersion", spec.rules_version) != spec.rules_version
            or "catalogVersion" in public_state
            and public_state["catalogVersion"] != spec.catalog_version):
        raise ValueError("typed public observation policy or catalog provenance mismatch")
    board = verified.board
    if (not isinstance(board, list) or not board or len(board) > spec.max_board_axis
            or not isinstance(board[0], list) or not board[0]
            or len(board[0]) > spec.max_board_axis
            or any(not isinstance(row, list) or len(row) != len(board[0]) for row in board)):
        raise ValueError("typed public board needs bounded rectangular geometry")
    geometry = (len(board), len(board[0]))
    if geometry != (8, 8):
        raise ValueError("source-bound public observation must use the site's 8x8 board")
    return spec.legacy_validator().validate_observation(owned, belief_summary=summary)


@dataclass(frozen=True)
class ObservationIR:
    """Owned public semantic source. Arbitrary IDs are kept only for linking."""

    geometry: BoardGeometry
    viewer: str
    turn: str
    board: tuple[tuple[Mapping[str, Any] | None, ...], ...]
    cell_kinds: tuple[tuple[str, ...], ...]
    own_cards: tuple[Mapping[str, Any], ...]
    opponent_hand_count: int
    public_state: Mapping[str, Any]
    history_summary: Mapping[str, Any]
    belief_summary: Mapping[str, Any] | None
    descriptors: tuple[Mapping[str, Any], ...]
    observation_version: str
    information_state_key: str
    rules_version: str
    catalog_hash: str
    observation_policy_hash: str

    def __post_init__(self) -> None:
        g = self.geometry
        if self.viewer not in ("white", "black") or self.turn not in ("white", "black"):
            raise ValueError("IR viewer and turn must be colors")
        if len(self.board) != g.height or len(self.cell_kinds) != g.height:
            raise ValueError("IR board height differs from geometry")
        for pieces, kinds in zip(self.board, self.cell_kinds):
            if len(pieces) != g.width or len(kinds) != g.width:
                raise ValueError("IR board width differs from geometry")
            for piece, kind in zip(pieces, kinds):
                if kind not in _CELL_KINDS or (kind == "piece") != (piece is not None):
                    raise ValueError("IR piece and cell kind disagree")
        if type(self.opponent_hand_count) is not int or self.opponent_hand_count < 0:
            raise ValueError("IR opponent hand count is invalid")
        if self.observation_version not in (PUBLIC_OBSERVATION_VERSION, SYNTHETIC_OBSERVATION_VERSION):
            raise ValueError("unsupported IR observation version")
        for value in (self.board, self.own_cards, self.public_state, self.history_summary, self.belief_summary, self.descriptors):
            _reject_private(value)
            canonical_json(value)
        if self.belief_summary is not None:
            _validate_belief_summary(self.belief_summary)
        if not isinstance(self.information_state_key, str) or len(self.information_state_key) != 64 or any(c not in "0123456789abcdef" for c in self.information_state_key):
            raise ValueError("IR information state binding must be SHA-256")

    @classmethod
    def from_public(cls, observation: Mapping[str, Any], spec: TypedEncoderSpec,
                    *, belief_summary: Mapping[str, Any] | None = None) -> ObservationIR:
        """Validate a native v2 viewer projection before materializing IR."""
        if spec.observation_version != PUBLIC_OBSERVATION_VERSION:
            raise ValueError("native v2 observation needs a v2 typed encoder spec")
        verified = validate_typed_public_observation(observation, spec, belief_summary=belief_summary)
        native = json.loads(canonical_json(verified.to_native()))
        g = BoardGeometry(0, 0, len(native["board"]), len(native["board"][0]))
        public = native["publicState"]
        holes = {_coord(square, g) for square in public.get("collapsedCells", [])}
        fog = {_coord(mark["square"], g) for mark in public.get("boardMarks", []) if mark.get("kind") == "fogHidden"}
        kinds = []
        for y, row in enumerate(native["board"]):
            kinds.append(tuple("hole" if (y, x) in holes else "piece" if piece is not None else "unknown" if (y, x) in fog else "empty"
                               for x, piece in enumerate(row)))
        if any(native["board"][y][x] is not None for y, x in holes):
            raise ValueError("collapsed cells cannot contain a public piece")
        summary = summarize_history_v2(native["history"])
        return cls(g, native["viewer"], native["turn"], tuple(tuple(row) for row in native["board"]),
                   tuple(kinds), tuple(native["ownCards"]), native["opponentHandCount"], public,
                   summary, json.loads(canonical_json(belief_summary)) if belief_summary is not None else None,
                   (), PUBLIC_OBSERVATION_VERSION, native["informationStateKey"], spec.rules_version,
                   spec.catalog_hash, spec.observation_policy_hash)

    @classmethod
    def from_components(cls, *, spec: TypedEncoderSpec, geometry: BoardGeometry, viewer: str, turn: str,
                        board: Sequence[Sequence[Mapping[str, Any] | None]],
                        cell_kinds: Sequence[Sequence[str]], own_cards: Sequence[Mapping[str, Any]] = (),
                        opponent_hand_count: int = 0, public_state: Mapping[str, Any] | None = None,
                        history_summary: Mapping[str, Any] | None = None,
                        belief_summary: Mapping[str, Any] | None = None,
                        descriptors: Sequence[Mapping[str, Any]] = ()) -> ObservationIR:
        """Explicit synthetic geometry boundary; never claims site-v7 parity."""
        if spec.observation_version != SYNTHETIC_OBSERVATION_VERSION:
            raise ValueError("component construction needs a synthetic geometry spec")
        state = json.loads(canonical_json(public_state or {}))
        board_copy = json.loads(canonical_json(board))
        cards_copy = json.loads(canonical_json(own_cards))
        history_copy = json.loads(canonical_json(history_summary or _empty_history_summary()))
        belief_copy = json.loads(canonical_json(belief_summary)) if belief_summary is not None else None
        descriptors_copy = json.loads(canonical_json(descriptors))
        _validate_descriptors(descriptors_copy)
        binding = _hash({"geometry": vars(geometry), "viewer": viewer, "turn": turn, "board": board_copy,
                         "cellKinds": cell_kinds, "cards": cards_copy, "opponentHandCount": opponent_hand_count,
                         "publicState": state, "history": history_copy, "belief": belief_copy,
                         "descriptors": descriptors_copy})
        return cls(geometry, viewer, turn, tuple(tuple(row) for row in board_copy),
                   tuple(tuple(row) for row in cell_kinds), tuple(cards_copy), opponent_hand_count,
                   state, history_copy, belief_copy, tuple(descriptors_copy), SYNTHETIC_OBSERVATION_VERSION,
                   binding, spec.rules_version, spec.catalog_hash, spec.observation_policy_hash)


def _coord(value: Any, geometry: BoardGeometry) -> tuple[int, int]:
    if (not isinstance(value, Mapping) or type(value.get("row")) is not int
            or type(value.get("col")) is not int):
        raise ValueError("public coordinate needs integer row and col")
    return geometry.local(value["row"], value["col"])


def _empty_history_summary() -> dict[str, Any]:
    return {"version": HISTORY_VERSION, "event_count": 0, "actor_counts": {"white": 0, "black": 0},
            "decision_actor_changes": 0, "board_change_count": 0, "recent_events": []}


def summarize_history_v2(history: Sequence[Mapping[str, Any]]) -> dict[str, Any]:
    """Bounded public history features; the full trace remains with replay.

    Observation v2 does not carry each event's board geometry. Historical
    coordinates therefore remain ordered absolute pairs, even when outside
    the current board. The digest is verification metadata, never a node.
    """
    required = {"kind", "actor", "nextActor", "phase", "boardChanges", "ownCards",
                "revealedOpponentCards", "captures", "result"}
    counts = {"white": 0, "black": 0}
    actor_changes = board_changes = 0
    recent: list[dict[str, Any]] = []
    for event in history:
        if not isinstance(event, Mapping) or set(event) != required or event["kind"] != "transition":
            raise ValueError("typed history needs public Transition v1 events")
        if event["actor"] not in counts or event["nextActor"] not in counts or not isinstance(event["phase"], str):
            raise ValueError("invalid public transition actor or phase")
        for key in ("boardChanges", "ownCards", "revealedOpponentCards"):
            if not isinstance(event[key], (list, tuple)):
                raise ValueError(f"public transition {key} must be an array")
        if not isinstance(event["result"], Mapping):
            raise ValueError("public transition result must be an object")
        squares = []
        for change in event["boardChanges"]:
            if not isinstance(change, Mapping) or set(change) != {"square", "before", "after"}:
                raise ValueError("public board change needs square, before and after")
            square = change["square"]
            if (not isinstance(square, Mapping) or set(square) != {"row", "col"}
                    or any(type(square[key]) is not int or not 0 <= square[key] < 32
                           for key in ("row", "col"))):
                raise ValueError("public history square needs bounded absolute coordinates")
            squares.append([square["row"], square["col"]])
        counts[event["actor"]] += 1
        actor_changes += event["actor"] != event["nextActor"]
        board_changes += len(squares)
        recent.append({"actor": event["actor"], "nextActor": event["nextActor"],
                       "phase": event["phase"], "board_change_count": len(squares),
                       "board_change_squares": squares, "own_card_count": len(event["ownCards"]),
                       "opponent_card_count": len(event["revealedOpponentCards"]),
                       "outcome": event["result"].get("outcome")})
        if len(recent) > 8:
            recent.pop(0)
    return {"version": HISTORY_VERSION, "event_count": len(history), "history_hash": _hash(history),
            "actor_counts": counts, "decision_actor_changes": actor_changes,
            "board_change_count": board_changes, "recent_events": recent}


def _validate_descriptors(descriptors: Sequence[Mapping[str, Any]]) -> None:
    node_fields = {"primitive", "direction", "maxDistance", "activationCondition",
                   "activateAtParentDistance", "children"}
    primitives = {"MOVE", "TAKE", "TAKEMOVE", "BOTHTAKEMOVE", "CATCH", "JUMP", "SHIFT"}

    def check(node: Any, depth: int, *, root: bool = False) -> None:
        if depth > 64 or not isinstance(node, Mapping):
            raise ValueError("unsupported public move descriptor")
        if "primitive" in node:
            if set(node) - node_fields or node["primitive"] not in primitives:
                raise ValueError("descriptor node needs a known primitive and typed fields")
            direction = node.get("direction")
            if (not isinstance(direction, Mapping) or set(direction) != {"dr", "dc"}
                    or any(type(direction[key]) is not int or abs(direction[key]) > 32 for key in ("dr", "dc"))):
                raise ValueError("invalid public move direction")
            if direction["dr"] == direction["dc"] == 0:
                raise ValueError("public move direction cannot be zero")
            if root and node.get("activateAtParentDistance") is not None:
                raise ValueError("public move root cannot have a parent distance")
            for key, lower in (("maxDistance", 1), ("activateAtParentDistance", 1)):
                value = node.get(key)
                if value is not None and (type(value) is not int or not lower <= value <= 4096):
                    raise ValueError(f"invalid public move {key}")
            if node.get("activationCondition", "Any") not in {"Any", "NoCapture", "MustCapture"}:
                raise ValueError("invalid public move activation condition")
            children = node.get("children", ())
            if not isinstance(children, (list, tuple)):
                raise ValueError("descriptor children must be an array")
            for child in children:
                check(child, depth + 1)
        elif "roots" in node:
            if (set(node) != {"sourceId", "roots"} or not isinstance(node["sourceId"], str)
                    or not node["sourceId"] or not isinstance(node["roots"], (list, tuple))):
                raise ValueError("invalid public move program")
            for root in node["roots"]:
                check(root, depth + 1, root=True)
        elif "program" in node:
            if (set(node) != {"modifierId", "source", "program", "expiration"}
                    or not isinstance(node["modifierId"], str) or not node["modifierId"]
                    or not isinstance(node["source"], str) or not node["source"]
                    or not isinstance(node["expiration"], Mapping)
                    or set(node["expiration"]) - {"kind", "remaining", "owner"}
                    or node["expiration"].get("kind") not in {"permanent", "actions", "ownerTurns", "activations"}):
                raise ValueError("invalid public move modifier")
            expiration = node["expiration"]
            kind = expiration["kind"]
            required = {"kind"} if kind == "permanent" else {"kind", "owner", "remaining"} if kind == "ownerTurns" else {"kind", "remaining"}
            if (set(expiration) != required
                    or "remaining" in expiration and (type(expiration["remaining"]) is not int
                                                      or not 0 <= expiration["remaining"] <= 4096)
                    or "owner" in expiration and expiration["owner"] not in ("white", "black")):
                raise ValueError("invalid public move modifier expiration")
            check(node["program"], depth + 1)
        elif "base" in node:
            if set(node) != {"base", "modifiers"} or not isinstance(node["modifiers"], (list, tuple)):
                raise ValueError("invalid public move program set")
            check(node["base"], depth + 1)
            for modifier in node["modifiers"]:
                check(modifier, depth + 1)
        else:
            raise ValueError("unsupported public move descriptor shape")

    if not isinstance(descriptors, (list, tuple)):
        raise ValueError("public descriptors must be an array")
    for descriptor in descriptors:
        check(descriptor, 0)


def _float(value: Any, field: str) -> float:
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ValueError(f"{field} must be a finite number")
    converted = float(np.float32(value))
    if not math.isfinite(converted):
        raise ValueError(f"{field} exceeds float32 range")
    return converted


@dataclass(frozen=True)
class _Node:
    category: tuple[int, int, int, int]
    numeric: tuple[float, ...]
    coord: tuple[float, float]
    coord_valid: bool
    parent: int = -1
    order: int = -1
    target_index: int = -1


@dataclass(frozen=True)
class _Edge:
    source: int
    target: int
    category: tuple[int, int]
    numeric: tuple[float, float, float, float]


def _card_surface(path: tuple[str, ...]) -> str | None:
    return {("ownCards",): "ownCards",
            ("publicState", "revealedOpponentCards"): "revealedOpponentCards",
            ("publicState", "draft", "choices"): "choices"}.get(path)


class _SemanticTree:
    def __init__(self, spec: TypedEncoderSpec, geometry: BoardGeometry, *, candidate: bool = False,
                 target_cells: Mapping[tuple[int, int], int] | None = None,
                 card_targets: Mapping[str, int] | None = None):
        self.spec = spec
        self.geometry = geometry
        self.candidate = candidate
        self.nodes: list[_Node] = []
        self.edges: list[_Edge] = []
        self.target_cells = target_cells or {}
        self.card_targets = card_targets or {}
        self.card_instances: dict[str, int] = {}
        self._card_views: dict[str, dict[str, Mapping[str, Any]]] = {}
        self._card_alias_records: set[int] = set()

    def _categories(self, kind: str, field: str, symbol: str = "", owner: str = "") -> tuple[int, int, int, int]:
        return tuple(self.spec.category_id(value) for value in (kind, field, symbol, owner))  # type: ignore[return-value]

    def add(self, kind: str, field: str, *, symbol: str = "", owner: str = "",
            number: float | None = None, coordinate: tuple[int, int] | None = None,
            span_length: int = 0, parent: int = -1, order: int = -1,
            target_index: int = -1, depth: int = 0) -> int:
        limit = self.spec.max_candidate_nodes if self.candidate else self.spec.max_records
        if len(self.nodes) >= limit:
            raise ValueError(f"semantic {'candidate node' if self.candidate else 'record'} count exceeds {limit}")
        xy = self.geometry.normalized(*coordinate) if coordinate is not None else (0., 0.)
        if self.candidate:
            row, col = coordinate if coordinate is not None else (0, 0)
            numeric = (_float(number, field) if number is not None else 0., float(number is not None),
                       0., float(depth), float(row), float(col), float(span_length), 0.)
        else:
            row, col = coordinate if coordinate is not None else (0, 0)
            numeric = (_float(number, field) if number is not None else 0., float(number is not None),
                       float(span_length), float(max(0, order)), float(row), float(col),
                       float(row), float(col + max(0, span_length - 1)))
        if len(numeric) != 8:
            raise AssertionError("typed numeric slot definition must remain eight-wide")
        index = len(self.nodes)
        self.nodes.append(_Node(self._categories(kind, field, symbol, owner), numeric, xy,
                                coordinate is not None, parent, order, target_index))
        if not self.candidate and parent >= 0:
            self._link(parent, index, "array-element" if order >= 0 else "contains", field, order)
        return index

    def _link(self, source: int, target: int, kind: str, field: str, order: int = -1) -> None:
        if len(self.edges) >= self.spec.max_relations:
            raise ValueError(f"semantic relation count exceeds {self.spec.max_relations}")
        a, b = self.nodes[source], self.nodes[target]
        dr = b.coord[0] - a.coord[0] if a.coord_valid and b.coord_valid else 0.
        dc = b.coord[1] - a.coord[1] if a.coord_valid and b.coord_valid else 0.
        self.edges.append(_Edge(source, target,
                                (self.spec.category_id(kind), self.spec.category_id(field)),
                                (float(max(0, order)), float(order >= 0), dr, dc)))

    def emit(self, value: Any, field: str, parent: int, *, order: int = -1,
             depth: int = 0, path: tuple[str, ...] = ()) -> int:
        if depth > 64:
            raise ValueError("semantic tree exceeds depth limit")
        if field == "cardInstanceIds":
            if not isinstance(value, (list, tuple)) or any(not isinstance(item, str) for item in value):
                raise ValueError("public card instance bundle must be an array of references")
            index = self.add("array", field, parent=parent, order=order, depth=depth)
            for ordinal, identifier in enumerate(value):
                self.emit(identifier, "cardInstanceId", index, order=ordinal, depth=depth + 1,
                          path=(*path, field))
            self._set_child_count(index, len(value))
            return index
        if field in _INSTANCE_FIELDS or (field == "source" and "descriptor" in path):
            if not isinstance(value, str):
                raise ValueError("public instance reference must be a string")
            target = self.card_targets.get(value, -1) if self.candidate and field in _CARD_REF_FIELDS else -1
            if self.candidate and field == "cardInstanceId" and target < 0:
                raise ValueError("candidate card instance is not in the viewer's public cards")
            index = self.add("scalar", field, symbol="instance-reference", parent=parent,
                             order=order, target_index=target, depth=depth)
            if not self.candidate and field in _CARD_REF_FIELDS:
                linked = self.card_instances.get(value)
                if linked is not None and linked != parent and parent not in self._card_alias_records:
                    self._link(parent, linked, "semantic-link", field)
            return index
        if isinstance(value, Mapping):
            coordinate: tuple[int, int] | None = None
            if type(value.get("row")) is int and type(value.get("col")) is int:
                _coord(value, self.geometry)
                coordinate = (value["row"], value["col"])
            target = self.target_cells.get(coordinate, -1) if self.candidate and coordinate is not None else -1
            index = self.add("coordinate" if coordinate is not None else "object", field,
                             parent=parent, order=order, coordinate=coordinate,
                             target_index=target, depth=depth)
            surface = _card_surface(path) if not self.candidate else None
            if surface is not None and "instanceId" in value and "id" in value:
                identifier = value["instanceId"]
                if not isinstance(identifier, str) or not identifier:
                    raise ValueError("invalid public card instance identifier")
                views = self._card_views.setdefault(identifier, {})
                if surface in views:
                    raise ValueError("duplicate public card instance in one surface")
                if (surface == "ownCards" and "revealedOpponentCards" in views
                        or surface == "revealedOpponentCards" and "ownCards" in views):
                    raise ValueError("one public card instance cannot have opposite owners")
                for prior in views.values():
                    if any(canonical_json(value[key]) != canonical_json(prior[key])
                           for key in value.keys() & prior.keys()):
                        raise ValueError("conflicting public card alias fields")
                if views:
                    self._link(index, self.card_instances[identifier], "same-identity", "instanceId")
                    self._card_alias_records.add(index)
                else:
                    self.card_instances[identifier] = index
                views[surface] = value
            for key, child in sorted(value.items()):
                self.emit(child, key, index, depth=depth + 1, path=(*path, field))
            self._set_child_count(index, len(value))
            return index
        if isinstance(value, (list, tuple)):
            index = self.add("array", field, parent=parent, order=order, depth=depth)
            for ordinal, child in enumerate(value):
                self.emit(child, "value", index, order=ordinal, depth=depth + 1, path=(*path, field))
            self._set_child_count(index, len(value))
            return index
        if value is None:
            return self.add("scalar", field, symbol="null", parent=parent, order=order, depth=depth)
        if type(value) is bool:
            return self.add("scalar", field, symbol="true" if value else "false",
                            parent=parent, order=order, depth=depth)
        if type(value) in (int, float):
            return self.add("scalar", field, number=value, parent=parent, order=order, depth=depth)
        if isinstance(value, str):
            return self.add("scalar", field, symbol=value, parent=parent, order=order, depth=depth)
        raise ValueError(f"unsupported semantic value at {field}")

    def _set_child_count(self, index: int, count: int) -> None:
        if not self.candidate:
            return
        node = self.nodes[index]
        numeric = list(node.numeric)
        numeric[2] = float(count)
        self.nodes[index] = replace(node, numeric=tuple(numeric))


@dataclass(frozen=True)
class TypedPosition:
    inputs: Mapping[str, np.ndarray]
    actions: tuple[dict[str, Any], ...]
    action_keys: tuple[str, ...]
    spec_digest: str
    information_state_key: str
    geometry: BoardGeometry
    examined: int
    exhaustive: bool
    max_batch: int
    max_input_bytes: int

    @property
    def candidate_mask(self) -> np.ndarray:
        return self.inputs["candidate_mask"]


@dataclass(frozen=True)
class TypedBatch:
    inputs: Mapping[str, np.ndarray]
    positions: tuple[TypedPosition, ...]
    spec_digest: str

    @property
    def candidate_mask(self) -> np.ndarray:
        return self.inputs["candidate_mask"]

    @property
    def spatial(self) -> np.ndarray:
        return self.inputs["spatial"]

    @property
    def layout_mask(self) -> np.ndarray:
        return self.inputs["layout_mask"]

    def as_family_inputs(self, family: str) -> tuple[np.ndarray, ...]:
        if family == "mask-resnet":
            order = INPUT_ORDER_A
        elif family == "entity-transformer":
            order = INPUT_ORDER_B
        else:
            raise ValueError(f"unknown typed architecture family: {family!r}")
        return tuple(self.inputs[name] for name in order)


class TypedEncoder:
    """Convert one validated public IR to both model families' shared tensors."""

    def __init__(self, spec: TypedEncoderSpec):
        if not isinstance(spec, TypedEncoderSpec):
            raise TypeError("typed encoder needs a TypedEncoderSpec")
        if spec._catalog is None or spec._policy is None:
            raise ValueError("typed encoder spec must be bound to source catalog and policy")
        self.spec = spec
        self._spec_digest = spec.digest
        self._card_ids = frozenset(card["id"] for card in spec._catalog["cards"])

    def encode(self, ir: ObservationIR, actions: Sequence[Mapping[str, Any]], *,
               examined: int | None = None, exhaustive: bool = True) -> TypedPosition:
        if not isinstance(ir, ObservationIR):
            raise TypeError("typed encoding requires an ObservationIR, never a Position")
        if (ir.observation_version != self.spec.observation_version or ir.rules_version != self.spec.rules_version
                or ir.catalog_hash != self.spec.catalog_hash
                or ir.observation_policy_hash != self.spec.observation_policy_hash):
            raise ValueError("IR provenance differs from the typed encoder contract")
        if len(actions) > self.spec.max_candidates:
            raise ValueError(f"candidate count {len(actions)} exceeds {self.spec.max_candidates}")
        if examined is None:
            examined = len(actions)
        if type(examined) is not int or examined < len(actions) or type(exhaustive) is not bool:
            raise ValueError("candidate work accounting is invalid")
        if ir.geometry.height > self.spec.max_board_axis or ir.geometry.width > self.spec.max_board_axis:
            raise ValueError("IR geometry exceeds this typed encoder's board axis")

        tree = _SemanticTree(self.spec, ir.geometry)
        tree.add("global", "observation", symbol=ir.viewer, owner=ir.viewer,
                 number=ir.opponent_hand_count)
        spatial = np.zeros((len(SPATIAL_CHANNELS), ir.geometry.height, ir.geometry.width), np.float32)
        layout_mask = np.ones((1, ir.geometry.height, ir.geometry.width), np.bool_)
        target_cells: dict[tuple[int, int], int] = {}
        anchor_records: dict[tuple[int, int, str, str], tuple[int, str]] = {}
        for y, row in enumerate(ir.cell_kinds):
            x = 0
            while x < ir.geometry.width:
                kind = row[x]
                row_abs = ir.geometry.origin_row + y
                col_abs = ir.geometry.origin_col + x
                if kind == "piece":
                    piece = ir.board[y][x]
                    assert piece is not None
                    color = piece.get("color")
                    piece_type = piece.get("type")
                    if color not in ("white", "black", "neutral") or not isinstance(piece_type, str):
                        raise ValueError("IR public piece needs color and type")
                    anchor_row, anchor_col = piece.get("anchorRow"), piece.get("anchorCol")
                    anchor = (anchor_row, anchor_col, piece_type, color) if type(anchor_row) is int and type(anchor_col) is int else None
                    if anchor is not None and anchor in anchor_records:
                        index, expected_piece = anchor_records[anchor]
                        if canonical_json(piece) != expected_piece:
                            raise ValueError("one public footprint identity has inconsistent piece attributes")
                    else:
                        index = tree.add("piece", "cell", symbol=piece_type, owner=color,
                                         coordinate=(row_abs, col_abs), parent=0, span_length=1)
                        for field_name, value in sorted(piece.items()):
                            tree.emit(value, field_name, index, path=("piece",))
                        if anchor is not None:
                            anchor_records[anchor] = (index, canonical_json(piece))
                    if anchor is not None:
                        footprint_cell = tree.add("cell-run", "cell", symbol="occupied",
                                                  coordinate=(row_abs, col_abs), parent=0,
                                                  span_length=1)
                        tree._link(index, footprint_cell, "occupies", "cell")
                    target_cells[(row_abs, col_abs)] = index
                    spatial[SPATIAL_CHANNELS.index("occupied"), y, x] = 1.
                    if color == ir.viewer:
                        spatial[SPATIAL_CHANNELS.index("own"), y, x] = 1.
                    elif color != "neutral":
                        spatial[SPATIAL_CHANNELS.index("opponent"), y, x] = 1.
                    x += 1
                    continue
                end = x + 1
                while end < ir.geometry.width and row[end] == kind:
                    end += 1
                index = tree.add("cell-run", "cell", symbol=kind,
                                 coordinate=(row_abs, col_abs), span_length=end - x, parent=0)
                spatial[SPATIAL_CHANNELS.index(kind), y, x:end] = 1.
                for col in range(x, end):
                    target_cells[(row_abs, ir.geometry.origin_col + col)] = index
                x = end

        # Geometry is explicit even when all cells happen to be empty.  The
        # absolute origin is a public property of the synthetic profile.
        tree.emit({"originRow": ir.geometry.origin_row, "originCol": ir.geometry.origin_col,
                   "height": ir.geometry.height, "width": ir.geometry.width}, "geometry", 0)
        tree.emit(ir.own_cards, "ownCards", 0)
        state = {key: value for key, value in ir.public_state.items()
                 if key not in {"rulesVersion", "catalogVersion", "projectionVersion", "observationPolicyHash"}}
        if state:
            tree.emit(state, "publicState", 0)
        if ir.belief_summary is not None:
            tree.emit(ir.belief_summary, "belief", 0)
        history = {key: value for key, value in ir.history_summary.items() if key != "history_hash"}
        if history.get("version") != HISTORY_VERSION:
            raise ValueError("IR history summary version differs from typed contract")
        tree.emit(history, "history", 0)
        if ir.descriptors:
            _validate_descriptors(ir.descriptors)
            tree.emit(ir.descriptors, "descriptor", 0)

        candidates: list[_SemanticTree] = []
        copied: list[dict[str, Any]] = []
        keys: list[str] = []
        seen_keys: set[str] = set()
        for action in actions:
            if not isinstance(action, Mapping):
                raise ValueError("candidate must be a public intent object")
            _reject_private(action)
            owned = json.loads(canonical_json(action))
            if not isinstance(owned.get("type"), str) or owned["type"] not in self.spec._catalog.get("actionTypes", ()):
                raise ValueError("candidate needs a catalog action type")
            if owned.get("color") != ir.viewer:
                raise ValueError("candidate decision actor differs from the observation viewer")
            if self.spec.rules_version == V7_RULES_VERSION and owned["type"] == "move":
                if set(owned) != {"type", "color", "from", "destination"}:
                    raise ValueError("v7 public move intent needs only actor, from and destination")
                for name in ("from", "destination"):
                    if not isinstance(owned[name], Mapping) or set(owned[name]) != {"row", "col"}:
                        raise ValueError("v7 public move intent needs exact public coordinates")
                    _coord(owned[name], ir.geometry)
            if "pieceId" in owned:
                raise ValueError("candidate cannot carry an execution piece identifier")
            if "cardId" in owned and owned["cardId"] not in self._card_ids:
                raise ValueError("candidate card ID is outside the source catalog")
            key = canonical_json(owned)
            if key in seen_keys:
                raise ValueError("duplicate public intent is one policy choice")
            seen_keys.add(key)
            candidate = _SemanticTree(self.spec, ir.geometry, candidate=True,
                                      target_cells=target_cells, card_targets=tree.card_instances)
            candidate.add("intent", "root", symbol=owned["type"], owner=owned.get("color", ""))
            for field_name, value in sorted(owned.items()):
                candidate.emit(value, field_name, 0, path=("intent",))
            candidate._set_child_count(0, len(owned))
            candidates.append(candidate)
            copied.append(owned)
            keys.append(key)

        inputs = self._arrays(tree, candidates, spatial, layout_mask, ir)
        if sum(array.nbytes for array in inputs.values()) > self.spec.max_input_bytes:
            raise ValueError(f"typed input uses more than {self.spec.max_input_bytes} bytes")
        return TypedPosition(inputs, tuple(copied), tuple(keys), self._spec_digest,
                             ir.information_state_key, ir.geometry, examined, exhaustive,
                             self.spec.max_batch, self.spec.max_input_bytes)

    def _arrays(self, tree: _SemanticTree, candidates: Sequence[_SemanticTree],
                spatial: np.ndarray, layout_mask: np.ndarray,
                ir: ObservationIR) -> dict[str, np.ndarray]:
        n = len(tree.nodes)
        r = max(1, len(tree.edges))
        a = max(1, len(candidates))
        t = max((len(candidate.nodes) for candidate in candidates), default=1)
        arrays: dict[str, np.ndarray] = {
            "spatial": spatial, "layout_mask": layout_mask,
            "record_category": np.zeros((n, 4), np.int64),
            "record_numeric": np.zeros((n, 8), np.float32),
            "record_coord": np.zeros((n, 2), np.float32),
            "record_spatial_valid": np.zeros(n, np.bool_),
            "record_mask": np.ones(n, np.bool_),
            "relation_index": np.zeros((r, 2), np.int64),
            "relation_category": np.zeros((r, 2), np.int64),
            "relation_numeric": np.zeros((r, 4), np.float32),
            "relation_mask": np.zeros(r, np.bool_),
            "candidate_category": np.zeros((a, t, 4), np.int64),
            "candidate_numeric": np.zeros((a, t, 8), np.float32),
            "candidate_coord": np.zeros((a, t, 2), np.float32),
            "candidate_coord_valid": np.zeros((a, t), np.bool_),
            "candidate_parent": np.full((a, t), -1, np.int64),
            "candidate_order": np.full((a, t), -1, np.int64),
            "candidate_target_index": np.full((a, t), -1, np.int64),
            "candidate_node_mask": np.zeros((a, t), np.bool_),
            "candidate_mask": np.zeros(a, np.bool_),
            "condition": self._condition(ir),
        }
        for index, node in enumerate(tree.nodes):
            arrays["record_category"][index] = node.category
            arrays["record_numeric"][index] = node.numeric
            arrays["record_coord"][index] = node.coord
            arrays["record_spatial_valid"][index] = node.coord_valid
        for index, edge in enumerate(tree.edges):
            arrays["relation_index"][index] = (edge.source, edge.target)
            arrays["relation_category"][index] = edge.category
            arrays["relation_numeric"][index] = edge.numeric
            arrays["relation_mask"][index] = True
        for action_index, candidate in enumerate(candidates):
            arrays["candidate_mask"][action_index] = True
            for node_index, node in enumerate(candidate.nodes):
                arrays["candidate_category"][action_index, node_index] = node.category
                arrays["candidate_numeric"][action_index, node_index] = node.numeric
                arrays["candidate_coord"][action_index, node_index] = node.coord
                arrays["candidate_coord_valid"][action_index, node_index] = node.coord_valid
                arrays["candidate_parent"][action_index, node_index] = node.parent
                arrays["candidate_order"][action_index, node_index] = node.order
                arrays["candidate_target_index"][action_index, node_index] = node.target_index
                arrays["candidate_node_mask"][action_index, node_index] = True
        return arrays

    @staticmethod
    def _condition(ir: ObservationIR) -> np.ndarray:
        state = ir.public_state
        counts = (state.get("actionsRemaining", 0), state.get("moveCount", 0),
                  state.get("fullMove", 0))
        values = (float(ir.viewer == "white"), float(ir.turn == ir.viewer),
                  _float(counts[0], "actionsRemaining") / 16,
                  _float(counts[1], "moveCount") / 512,
                  _float(counts[2], "fullMove") / 256,
                  _float(ir.opponent_hand_count, "opponentHandCount") / 16,
                  _float(len(ir.own_cards), "ownCardCount") / 32,
                  _float(ir.history_summary.get("event_count", 0), "eventCount") / 512)
        condition = np.asarray(values, np.float32)
        if not np.isfinite(condition).all():
            raise ValueError("typed FiLM condition must be finite float32")
        return condition


def batch_typed_positions(positions: Sequence[TypedPosition]) -> TypedBatch:
    """Pad independent public feature tensors without altering real cells."""
    if not positions or any(not isinstance(position, TypedPosition) for position in positions):
        raise ValueError("typed batch needs at least one TypedPosition")
    if (len(positions) > min(position.max_batch for position in positions)
            or len({position.spec_digest for position in positions}) != 1):
        raise ValueError("typed batch size or encoder contracts are incompatible")
    expected = set(INPUT_ORDER_A)
    for position in positions:
        if set(position.inputs) != expected:
            raise ValueError("typed position has missing or unknown tensor inputs")
        mask = position.inputs["candidate_mask"]
        if (mask.ndim != 1 or len(position.actions) != len(position.action_keys)
                or int(mask.sum()) != len(position.actions)
                or not mask[:len(position.actions)].all() or mask[len(position.actions):].any()):
            raise ValueError("typed candidate mask differs from public intents")
        if (position.inputs["layout_mask"].shape != (1, position.geometry.height, position.geometry.width)
                or not position.inputs["record_mask"][0]):
            raise ValueError("typed geometry or global record is invalid")
        for name, array in position.inputs.items():
            dtype = {"float32": np.float32, "int64": np.int64, "bool": np.bool_}[_INPUTS[name][0]]
            if not isinstance(array, np.ndarray) or array.dtype != dtype:
                raise ValueError(f"typed input {name} has wrong dtype")
            if array.dtype == np.float32 and not np.isfinite(array).all():
                raise ValueError(f"typed input {name} has non-finite values")
    maxima = {"height": max(p.geometry.height for p in positions),
              "width": max(p.geometry.width for p in positions),
              "records": max(p.inputs["record_mask"].shape[0] for p in positions),
              "relations": max(p.inputs["relation_mask"].shape[0] for p in positions),
              "actions": max(p.inputs["candidate_mask"].shape[0] for p in positions),
              "nodes": max(p.inputs["candidate_node_mask"].shape[1] for p in positions)}
    arrays: dict[str, np.ndarray] = {}
    batch = len(positions)
    estimated = 0
    for name in INPUT_ORDER_A:
        dtype_name, dimensions = _INPUTS[name]
        dtype = {"float32": np.float32, "int64": np.int64, "bool": np.bool_}[dtype_name]
        shape = (batch, *(maxima[dim] if isinstance(dim, str) else dim for dim in dimensions[1:]))
        estimated += int(np.prod(shape, dtype=np.int64)) * np.dtype(dtype).itemsize
        if estimated > min(position.max_input_bytes for position in positions):
            raise ValueError("typed batch exceeds its versioned input byte budget")
        arrays[name] = np.full(shape, -1 if name in ("candidate_parent", "candidate_order", "candidate_target_index") else 0, dtype)
    for batch_index, position in enumerate(positions):
        for name, source in position.inputs.items():
            target = arrays[name]
            slices = (batch_index, *(slice(0, length) for length in source.shape))
            target[slices] = source
    return TypedBatch(arrays, tuple(positions), positions[0].spec_digest)
