"""Additive, versioned semantic projections over verified public ObservationIR.

The typed-input-v1 encoder and deployed model families remain unchanged. This
module defines the next model input contract without interpreting private game
state or changing the public wire format.
"""

from __future__ import annotations

from dataclasses import dataclass
from hashlib import sha256
from typing import Any, Mapping, Sequence

import numpy as np

from .encoding import canonical_json
from .ir import ObservationIR, TypedEncoder, TypedPosition


ENTITY_KINDS = ("global", "piece", "card", "rule", "effect", "terrain", "portal", "cell", "history")
SPATIAL_VERSION = "fixed8-spatial-v1"
ENTITY_VERSION = "entity-token-v1"
GLOBAL_VERSION = "global-context-v1"
ACTION_VERSION = "typed-candidate-tree-v1"
OUTPUT_VERSION = "candidate-policy-value-v1"


@dataclass(frozen=True)
class SpatialFeatureSpec:
    version: str = SPATIAL_VERSION
    height: int = 8
    width: int = 8
    aggregation: str = "mean"

    def __post_init__(self) -> None:
        if (self.version, self.height, self.width, self.aggregation) != (SPATIAL_VERSION, 8, 8, "mean"):
            raise ValueError("fixed8 spatial contract requires 8x8 mean aggregation")


@dataclass(frozen=True)
class EntityFeatureSpec:
    version: str = ENTITY_VERSION
    category_buckets: int = 4096
    numeric_width: int = 8
    kinds: tuple[str, ...] = ENTITY_KINDS

    def __post_init__(self) -> None:
        if (self.version, self.category_buckets, self.numeric_width, self.kinds) != (ENTITY_VERSION, 4096, 8, ENTITY_KINDS):
            raise ValueError("unsupported entity token contract")


@dataclass(frozen=True)
class GlobalContextSpec:
    version: str = GLOBAL_VERSION
    legacy_condition_version: str = "typed-input-v1"
    legacy_condition_width: int = 8


@dataclass(frozen=True)
class CandidateActionSpec:
    version: str = ACTION_VERSION
    source: str = "typed-input-v1 candidate tree"


@dataclass(frozen=True)
class ModelOutputSpec:
    version: str = OUTPUT_VERSION
    value_perspective: str = "observation.viewer"
    padded_logit: float = -1.0e9


@dataclass(frozen=True)
class ArchitectureSpec:
    spatial: SpatialFeatureSpec = SpatialFeatureSpec()
    entity: EntityFeatureSpec = EntityFeatureSpec()
    global_context: GlobalContextSpec = GlobalContextSpec()
    action: CandidateActionSpec = CandidateActionSpec()
    output: ModelOutputSpec = ModelOutputSpec()

    def metadata(self, typed_encoder: TypedEncoder, family: str) -> dict[str, Any]:
        if family not in ("fixed8-resnet", "entity-token-transformer"):
            raise ValueError("unknown architecture family")
        return {"model_family": family, "architecture_version": self.spatial.version if family == "fixed8-resnet" else self.entity.version,
                "encoder_version": typed_encoder.spec.encoder_version, "encoder_hash": typed_encoder.spec.digest,
                "condition_version": self.global_context.version, "legacy_condition_version": self.global_context.legacy_condition_version,
                "action_encoding_version": self.action.version, "output_version": self.output.version,
                "rules_version": typed_encoder.spec.rules_version, "catalog_version": typed_encoder.spec.catalog_version,
                "catalog_hash": typed_encoder.spec.catalog_hash, "value_perspective": self.output.value_perspective,
                "input_names": ("geometry", "entity_category", "entity_numeric", "entity_coord", "entity_mask",
                                "relation_index", "relation_kind", "relation_mask", "occupancy",
                                "spatial_state", "condition", "candidate_category", "candidate_numeric",
                                "candidate_coord", "candidate_coord_valid", "candidate_parent",
                                "candidate_order", "candidate_target_index", "candidate_node_mask", "candidate_mask"),
                "shape_semantics": {"geometry": "[B,4]", "entity_category": "[B,E,6]", "entity_numeric": "[B,E,8]",
                                    "entity_coord": "[B,E,2]", "occupancy": "[B,E,H,W]",
                                    "spatial_state": "[B,4,H,W]", "condition": "[B,8]",
                                    "candidate_mask": "[B,A]", "policy_logits": "[B,A]", "value": "[B,1]"},
                "dtypes": {"categories": "int64", "numbers": "float32", "masks": "bool"},
                "mask_semantics": "true means real; padded policy logit is -1e9",
                "film": "global context and unchanged eight-scalar condition; gamma,beta inside graph",
                "lora": "static adapter only; legacy adapter artifacts are not interchangeable"}

    def digest(self, typed_encoder: TypedEncoder, family: str) -> str:
        return sha256(canonical_json(self.metadata(typed_encoder, family)).encode("utf-8")).hexdigest()


@dataclass(frozen=True)
class ProjectedPosition:
    typed: TypedPosition
    entity_category: np.ndarray  # [E,6]: kind, type, owner, phase, state, descriptor
    entity_numeric: np.ndarray  # [E,8]: state/counter/visibility/anchor/footprint
    entity_coord: np.ndarray  # [E,2], normalized in geometry
    entity_mask: np.ndarray  # [E]
    relation_index: np.ndarray  # [R,2]
    relation_kind: np.ndarray  # [R]
    relation_mask: np.ndarray  # [R]
    occupancy: np.ndarray  # [E,H,W], one logical piece may occupy many cells
    spatial_state: np.ndarray  # [4,H,W]: empty, unknown, hole, terrain/effect
    condition: np.ndarray  # unchanged typed-input-v1 eight scalars
    candidate_target_index: np.ndarray  # [A,N], remapped to entity indexes


def _bucket(value: Any) -> int:
    if value is None or value == "":
        return 0
    return 1 + int.from_bytes(sha256(str(value).encode("utf-8")).digest()[:4], "big") % 4095


def _semantic_value(value: Any) -> Any:
    """Remove public reference identity while retaining descriptor meaning."""
    identity = {"instanceId", "cardInstanceId", "pieceId", "sourceId", "modifierId",
                "sourcePieceId", "targetPieceId", "informationStateKey", "history_hash"}
    if isinstance(value, Mapping):
        return {key: _semantic_value(item) for key, item in value.items() if key not in identity}
    if isinstance(value, (list, tuple)):
        return [_semantic_value(item) for item in value]
    return value


def _finite_number(value: Any, scale: float = 1.) -> float:
    if type(value) not in (int, float) or not np.isfinite(value):
        return 0.
    return float(np.clip(value / scale, -1_000., 1_000.))


class EntityTokenEncoder:
    """Semantic entity projection; a logical piece occupies one token."""

    def __init__(self, typed_encoder: TypedEncoder, spec: ArchitectureSpec = ArchitectureSpec()):
        self.typed_encoder = typed_encoder
        self.spec = spec

    def encode(self, ir: ObservationIR, actions: Sequence[Mapping[str, Any]]) -> ProjectedPosition:
        typed = self.typed_encoder.encode(ir, actions)
        geometry = ir.geometry
        entities: list[tuple[list[int], list[float], tuple[float, float], list[tuple[int, int]]]] = []
        relations: list[tuple[int, int, int]] = []
        cell_owner: dict[tuple[int, int], int] = {}
        card_owner: dict[str, int] = {}
        piece_identity: dict[tuple[Any, ...], int] = {}

        def add(kind: str, subtype: Any = "", owner: Any = "", *, row: int | None = None,
                col: int | None = None, data: Mapping[str, Any] | None = None,
                cells: Sequence[tuple[int, int]] = ()) -> int:
            data = data or {}
            xy = geometry.normalized(row, col) if row is not None and col is not None else (0., 0.)
            anchor_row = data.get("anchorRow", row)
            anchor_col = data.get("anchorCol", col)
            anchor_xy = geometry.normalized(anchor_row, anchor_col) if type(anchor_row) is int and type(anchor_col) is int else xy
            numeric = [_finite_number(data.get("counter", data.get("charges")), 32), _finite_number(data.get("remaining"), 32),
                       float(data.get("visible", True) is True), anchor_xy[0], anchor_xy[1],
                       _finite_number(len(cells), 64), _finite_number(data.get("state")) ,
                       _finite_number(data.get("duration"), 64)]
            descriptor = data.get("descriptor", data.get("moveProgram"))
            if isinstance(descriptor, Mapping):
                descriptor = canonical_json(_semantic_value(descriptor))
            state = data.get("state", data.get("status", data.get("used", data.get("enabled"))))
            if isinstance(state, Mapping):
                state = canonical_json(_semantic_value(state))
            categories = [ENTITY_KINDS.index(kind), _bucket(subtype), _bucket(owner),
                          _bucket(ir.public_state.get("phase")), _bucket(state), _bucket(descriptor)]
            index = len(entities)
            entities.append((categories, numeric, xy, list(cells)))
            return index

        add("global", "observation", ir.viewer, data={"counter": ir.opponent_hand_count})
        for y, row in enumerate(ir.board):
            for x, piece in enumerate(row):
                if piece is None:
                    continue
                absolute = (geometry.origin_row + y, geometry.origin_col + x)
                anchor = (piece.get("anchorRow"), piece.get("anchorCol"))
                identity = (anchor, piece.get("type"), piece.get("color")) if all(type(v) is int for v in anchor) else (absolute,)
                if identity in piece_identity:
                    index = piece_identity[identity]
                    # The underlying typed encoder already rejects inconsistent aliases.
                    entities[index][3].append((y, x))
                else:
                    index = add("piece", piece.get("type"), piece.get("color"), row=absolute[0], col=absolute[1],
                                data=piece, cells=[(y, x)])
                    piece_identity[identity] = index
                cell_owner[absolute] = index
        for card in ir.own_cards:
            index = add("card", card.get("id", card.get("cardId")), ir.viewer, data=card)
            if isinstance(card.get("instanceId"), str):
                card_owner[card["instanceId"]] = index
        # Public semantic collections become entities. Their fields are folded
        # into attributes; no JSON field or BPE token is emitted.
        surfaces = (("revealedOpponentCards", "card"), ("activeRules", "rule"),
                    ("rules", "rule"), ("effects", "effect"), ("activeEffects", "effect"),
                    ("boardMarks", "terrain"), ("terrain", "terrain"), ("portals", "portal"),
                    ("relationships", "effect"), ("overlays", "effect"), ("deck", "card"))
        for field, kind in surfaces:
            collection = ir.public_state.get(field, ())
            if isinstance(collection, Mapping):
                collection = (collection,)
            if not isinstance(collection, (list, tuple)):
                continue
            for item in collection:
                if not isinstance(item, Mapping):
                    continue
                square = item.get("square", item)
                row = square.get("row") if isinstance(square, Mapping) else None
                col = square.get("col") if isinstance(square, Mapping) else None
                if type(row) is not int or type(col) is not int:
                    row = col = None
                subtype = item.get("type", item.get("kind", item.get("id", field)))
                owner = item.get("owner", item.get("color", ""))
                actual_kind = "portal" if field == "boardMarks" and subtype == "portal" else kind
                spatial_cells = [geometry.local(row, col)] if row is not None and actual_kind in ("terrain", "effect", "portal") else ()
                index = add(actual_kind, subtype, owner, row=row, col=col, data=item, cells=spatial_cells)
                if row is not None and (row, col) in cell_owner:
                    relations.append((cell_owner[(row, col)], index, 1))
                if field == "relationships":
                    start, end = item.get("from"), item.get("to")
                    if isinstance(start, Mapping) and isinstance(end, Mapping):
                        a = cell_owner.get((start.get("row"), start.get("col")))
                        b = cell_owner.get((end.get("row"), end.get("col")))
                        if a is not None:
                            relations.append((a, index, 1))
                        if b is not None:
                            relations.append((index, b, 1))
                if kind == "card" and isinstance(item.get("instanceId"), str):
                    card_owner[item["instanceId"]] = index
        for descriptor in ir.descriptors:
            add("rule", descriptor.get("primitive", "move-program"), descriptor.get("owner", ""),
                data={"descriptor": descriptor})
        summary = ir.history_summary
        add("history", "summary", data={"counter": summary["event_count"],
                                        "remaining": summary["decision_actor_changes"],
                                        "duration": summary["board_change_count"]})
        for ordinal, event in enumerate(summary["recent_events"]):
            event_index = add("history", event["phase"], event["actor"],
                              data={"counter": event["board_change_count"], "remaining": event["own_card_count"],
                                    "duration": event["opponent_card_count"], "state": ordinal,
                                    "descriptor": {"outcome": event["outcome"], "nextActor": event["nextActor"]}})
            for order, square in enumerate(event["board_change_squares"]):
                row, col = square
                inside = (geometry.origin_row <= row < geometry.origin_row + geometry.height
                          and geometry.origin_col <= col < geometry.origin_col + geometry.width)
                square_index = add("history", "changed-square", event["actor"],
                                   row=row if inside else None, col=col if inside else None,
                                   data={"counter": row, "remaining": col, "state": order})
                relations.append((event_index, square_index, 1))
        # Explicit unavailable cells preserve unknown versus collapsed terrain.
        for y, row in enumerate(ir.cell_kinds):
            for x, kind in enumerate(row):
                if kind in ("unknown", "hole"):
                    add("cell", kind, row=geometry.origin_row + y, col=geometry.origin_col + x)
        for index in range(1, len(entities)):
            relations.append((0, index, 0))
        for piece_index, (_, _, _, cells) in enumerate(entities):
            if cells:
                for y, x in cells:
                    for other_index, (_, _, xy, _) in enumerate(entities):
                        if other_index != piece_index and xy == geometry.normalized(geometry.origin_row + y, geometry.origin_col + x):
                            relations.append((piece_index, other_index, 2))
        if len(entities) > self.typed_encoder.spec.max_records or len(relations) > self.typed_encoder.spec.max_relations:
            raise ValueError("entity projection exceeds versioned record/relation limits")
        occupancy = np.zeros((len(entities), geometry.height, geometry.width), np.float32)
        for index, (_, numeric, _, cells) in enumerate(entities):
            for y, x in cells:
                occupancy[index, y, x] = 1.
            if cells:
                numeric[5] = len(cells) / 64.
        state = np.zeros((4, geometry.height, geometry.width), np.float32)
        for y, row in enumerate(ir.cell_kinds):
            for x, kind in enumerate(row):
                if kind in ("empty", "unknown", "hole"):
                    state[("empty", "unknown", "hole").index(kind), y, x] = 1.
        for field in ("boardMarks", "terrain"):
            for item in ir.public_state.get(field, ()):
                if isinstance(item, Mapping):
                    square = item.get("square", item)
                    if isinstance(square, Mapping) and type(square.get("row")) is int and type(square.get("col")) is int:
                        y, x = geometry.local(square["row"], square["col"])
                        state[3, y, x] += 1.
        target = typed.inputs["candidate_target_index"].copy()
        target.fill(-1)
        coords = typed.inputs["candidate_coord"]
        valid = typed.inputs["candidate_coord_valid"]
        for a, action in enumerate(typed.actions):
            for n in range(target.shape[1]):
                if valid[a, n]:
                    for cell, index in cell_owner.items():
                        if np.allclose(coords[a, n], geometry.normalized(*cell)):
                            target[a, n] = index
                            break
                elif action.get("cardInstanceId") in card_owner and n > 0:
                    field_id = typed.inputs["candidate_category"][a, n, 1]
                    if field_id == self.typed_encoder.spec.category_id("cardInstanceId"):
                        target[a, n] = card_owner[action["cardInstanceId"]]
        edge_index = np.asarray([(a, b) for a, b, _ in relations] or [(0, 0)], np.int64)
        edge_kind = np.asarray([kind for _, _, kind in relations] or [0], np.int64)
        edge_mask = np.asarray([True] * len(relations) or [False], np.bool_)
        return ProjectedPosition(typed, np.asarray([e[0] for e in entities], np.int64),
                                 np.asarray([e[1] for e in entities], np.float32),
                                 np.asarray([e[2] for e in entities], np.float32),
                                 np.ones(len(entities), np.bool_), edge_index, edge_kind, edge_mask,
                                 occupancy, state, typed.inputs["condition"], target)


class Fixed8x8SpatialEncoder(EntityTokenEncoder):
    def encode(self, ir: ObservationIR, actions: Sequence[Mapping[str, Any]]) -> ProjectedPosition:
        if (ir.geometry.origin_row, ir.geometry.origin_col, ir.geometry.height, ir.geometry.width) != (0, 0, 8, 8):
            raise ValueError("fixed8-spatial-v1 requires the 8x8 board at origin (0,0)")
        return super().encode(ir, actions)
