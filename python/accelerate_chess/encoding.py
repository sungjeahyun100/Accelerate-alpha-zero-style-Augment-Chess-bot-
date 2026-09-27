"""Versioned, lossless features of *public* observations and candidate actions.

The structured features expose useful chess semantics; canonical UTF-8 tails
preserve every remaining public attribute, including nested flags. Capacities
are part of the model contract: overflow is an error, never truncation. The
encoder has no API accepting a full Position or its hidden RNG state.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
import hashlib
import json
import math
from typing import Any, Mapping, Sequence

import numpy as np
import jcs


ENCODER_VERSION = "public-utf8-v1"
ACTION_VERSION = "candidate-payload-v1"
CONDITION_VERSION = "public-film-v1"
OBSERVATION_VERSION = "accelerate-observation-v1"
HISTORY_SUMMARY_VERSION = "public-history-summary-v1"
ACTION_TYPES = ("move", "card", "promotion", "promotionChoice", "shotgunReload", "wizardSpell", "fileSurgeSkip", "draftPick", "draftBundlePick", "trolleyChoice")


def canonical_json(value: Any) -> str:
    """RFC 8785/JCS identity shared with JS/Rust, with safe numeric bounds."""
    def validate(item: Any, depth: int = 0) -> None:
        if depth > 64:
            raise ValueError("canonical JSON nesting exceeds the supported boundary")
        if item is None or isinstance(item, (str, bool)):
            return
        if type(item) is int:
            if abs(item) > 2**53 - 1:
                raise ValueError("canonical JSON integers must fit the exact IEEE-754 range")
        elif type(item) is float:
            if not math.isfinite(item):
                raise ValueError("canonical JSON numbers must be finite")
            if item.is_integer() and abs(item) > 2**53 - 1:
                raise ValueError("canonical JSON integral floats must fit the exact IEEE-754 range")
        elif isinstance(item, Mapping):
            for key, child in item.items():
                if not isinstance(key, str):
                    raise ValueError("canonical JSON object keys must be strings")
                validate(child, depth + 1)
        elif isinstance(item, (tuple, list)):
            for child in item:
                validate(child, depth + 1)
        else:
            raise ValueError("canonical JSON accepts only JSON values")
    try:
        validate(value)
        return jcs.canonicalize(value).decode("utf-8")
    except (TypeError, ValueError, UnicodeError) as error:
        raise ValueError("features must be finite JSON data") from error


@dataclass(frozen=True)
class EncoderSpec:
    rules_version: str
    catalog_hash: str
    piece_ids: tuple[str, ...]
    card_ids: tuple[str, ...]
    rule_ids: tuple[str, ...]
    piece_payload_bytes: int = 2048
    public_payload_bytes: int = 32768
    action_payload_bytes: int = 4096
    encoder_version: str = ENCODER_VERSION
    action_version: str = ACTION_VERSION
    condition_version: str = CONDITION_VERSION
    action_types: tuple[str, ...] = ACTION_TYPES
    catalog_version: str = ""
    history_encoding: str = "full"
    action_encoding: str = "exact-payload"

    def __post_init__(self) -> None:
        if not self.rules_version or len(self.catalog_hash) != 64:
            raise ValueError("rules version and SHA-256 catalog hash are required")
        try:
            bytes.fromhex(self.catalog_hash)
        except ValueError as error:
            raise ValueError("catalog hash must be hexadecimal") from error
        for field in ("piece_ids", "card_ids", "rule_ids", "action_types"):
            values = getattr(self, field)
            if not isinstance(values, tuple) or len(set(values)) != len(values) or any(not isinstance(x, str) or not x for x in values):
                raise ValueError(f"{field} must contain unique nonempty IDs in a fixed order")
        if not self.piece_ids or not self.action_types:
            raise ValueError("piece and action catalogs cannot be empty")
        for capacity in (self.piece_payload_bytes, self.public_payload_bytes, self.action_payload_bytes):
            if type(capacity) is not int or not 1 <= capacity <= 1_048_576:
                raise ValueError("payload capacities must be positive bounded integers")
        if (self.encoder_version, self.action_version, self.condition_version) != (ENCODER_VERSION, ACTION_VERSION, CONDITION_VERSION):
            raise ValueError("unsupported feature contract version")
        if self.history_encoding not in ("full", HISTORY_SUMMARY_VERSION):
            raise ValueError("unsupported explicit public history encoding")
        if self.action_encoding not in ("exact-payload", "public-decision-intent-v1"):
            raise ValueError("unsupported explicit action encoding")

    @property
    def board_channels(self) -> int:
        return len(self.piece_ids) + 5 + self.piece_payload_bytes

    @property
    def condition_dim(self) -> int:
        return 2 * len(self.card_ids) + len(self.rule_ids) + 5 + self.public_payload_bytes

    @property
    def action_dim(self) -> int:
        return len(self.action_types) + len(self.card_ids) + 7 + self.action_payload_bytes

    def to_dict(self) -> dict[str, Any]:
        result = asdict(self)
        for key in ("piece_ids", "card_ids", "rule_ids", "action_types"):
            result[key] = list(result[key])
        return result

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> EncoderSpec:
        values = dict(data)
        for key in ("piece_ids", "card_ids", "rule_ids", "action_types"):
            if key in values:
                values[key] = tuple(values[key])
        return cls(**values)

    @classmethod
    def from_catalog(cls, catalog: Mapping[str, Any], **capacities: int) -> EncoderSpec:
        """Catalog IDs are serialized in stable lexical order, never inferred."""
        if catalog.get("schemaVersion") != 1:
            raise ValueError("unsupported catalog schema")
        cards = catalog["cards"]
        return cls(catalog["rulesVersion"], hashlib.sha256(canonical_json(catalog).encode("utf-8")).hexdigest(),
                   tuple(sorted(catalog["pieceTypes"])), tuple(sorted(card["id"] for card in cards)),
                   tuple(sorted(card["id"] for card in cards if card["draftCategory"] == "RULE")),
                   action_types=tuple(catalog["actionTypes"]), catalog_version=catalog["catalogVersion"], **capacities)

    def contract(self) -> dict[str, Any]:
        return {
            "spec": self.to_dict(),
            "observation_version": OBSERVATION_VERSION,
            "dtype": "float32", "board_layout": "NCHW", "value_perspective": "observation.viewer",
            "board_fields": ["piece-id-onehot", "own", "opponent", "moved", "occupied", "canonical-json-byte-length/capacity", "canonical-json-utf8-bytes/255"],
            "condition_fields": ["own-card-id-counts", "revealed-opponent-card-id-counts", "rule-id-presence", "viewer-is-white", "actionsRemaining/16", "moveCount/512", "fullMove/256", "canonical-json-byte-length/capacity", "canonical-json-utf8-bytes/255"],
            "action_fields": ["action-type-onehot", "card-id-onehot", "from-row/7", "from-col/7", "to-row/7", "to-col/7", "target-row/7", "target-col/7", "canonical-json-byte-length/capacity", "canonical-json-utf8-bytes/255"],
            "board_channels": self.board_channels, "condition_dim": self.condition_dim, "action_dim": self.action_dim,
            "coordinate_orientation": "viewer-black rotates both axes; JSON tails retain absolute site coordinates",
            "history_policy": {
                "mode": self.history_encoding,
                "version": "public-history-full-v1" if self.history_encoding == "full" else HISTORY_SUMMARY_VERSION,
                "recent_events": 0 if self.history_encoding == "full" else 8,
                "summary_fields": [] if self.history_encoding == "full" else ["event_count", "history_hash", "actor_counts", "decision_actor_changes", "board_change_count", "recent_events"],
                "full_history_owner": "tracker-and-replay",
            },
            "action_policy": {"mode": self.action_encoding,
                              "selection_identity": "canonical-semantic-payload" if self.action_encoding == "exact-payload" else "source-ui-choice",
                              "execution_payload": "lossless" if self.action_encoding == "exact-payload" else "native-only"},
        }

    @property
    def digest(self) -> str:
        return hashlib.sha256(canonical_json(self.contract()).encode("utf-8")).hexdigest()


@dataclass(frozen=True)
class PublicObservation:
    """Only the native observe() result may supply these public fields.

    History and belief_summary are supplied by the information-set tracker,
    never by inspecting an actual hidden game state.
    """
    player: str
    board: Sequence[Sequence[Mapping[str, Any] | None]]
    public: Mapping[str, Any]
    history: Sequence[Mapping[str, Any]] = ()
    belief_summary: Mapping[str, Any] | None = None

    @classmethod
    def from_native(cls, observation: Mapping[str, Any], *, belief_summary: Mapping[str, Any] | None = None) -> PublicObservation:
        expected = {"protocolVersion", "viewer", "board", "turn", "ownCards", "opponentHandCount", "publicState", "history", "informationStateKey"}
        if not isinstance(observation, Mapping) or set(observation) != expected or observation["protocolVersion"] != OBSERVATION_VERSION:
            raise ValueError("expected the exact public Observation v1 contract, not a full game snapshot")
        if observation["viewer"] not in ("white", "black") or observation["turn"] not in ("white", "black"):
            raise ValueError("observation viewer/turn is invalid")
        if type(observation["opponentHandCount"]) is not int or observation["opponentHandCount"] < 0:
            raise ValueError("opponent hand count must be nonnegative")
        key = observation["informationStateKey"]
        if not isinstance(key, str) or len(key) != 64 or any(character not in "0123456789abcdef" for character in key):
            raise ValueError("observation needs a SHA-256 public information state key")
        content = {name: value for name, value in observation.items() if name != "informationStateKey"}
        if hashlib.sha256(canonical_json(content).encode("utf-8")).hexdigest() != key:
            raise ValueError("public observation information state identity mismatch")
        if not isinstance(observation["publicState"], Mapping) or not isinstance(observation["ownCards"], (tuple, list)) or not isinstance(observation["history"], (tuple, list)):
            raise ValueError("observation public state, cards and history are invalid")
        if any(not isinstance(card, Mapping) for card in observation["ownCards"]) or any(not isinstance(event, Mapping) for event in observation["history"]):
            raise ValueError("public cards and history must contain JSON objects")
        public = {key: observation[key] for key in ("turn", "ownCards", "opponentHandCount", "publicState", "informationStateKey")}
        return cls(observation["viewer"], observation["board"], public, observation["history"], belief_summary)

    def to_native(self) -> dict[str, Any]:
        """Rebuild the public envelope so typed callers cannot bypass identity."""
        return {"protocolVersion": OBSERVATION_VERSION, "viewer": self.player, "board": self.board,
                "history": self.history, **self.public}


@dataclass(frozen=True)
class EncodedPosition:
    board: np.ndarray
    condition: np.ndarray
    action_features: np.ndarray
    actions: tuple[dict[str, Any], ...]
    action_keys: tuple[str, ...]
    spec_digest: str


@dataclass(frozen=True)
class EncodedBatch:
    board: np.ndarray
    condition: np.ndarray
    action_features: np.ndarray
    action_mask: np.ndarray
    positions: tuple[EncodedPosition, ...]


def _bytes(value: Any, capacity: int) -> np.ndarray:
    payload = canonical_json(value).encode("utf-8")
    if len(payload) > capacity:
        raise ValueError(f"public payload uses {len(payload)} bytes; contract permits {capacity}")
    result = np.zeros(capacity + 1, dtype=np.float32)
    result[0] = len(payload) / capacity
    result[1:len(payload) + 1] = np.frombuffer(payload, dtype=np.uint8) / np.float32(255)
    return result


def decode_json_tail(values: np.ndarray) -> Any:
    """Recover the canonical payload, useful when auditing an encoder contract."""
    if values.ndim != 1 or not np.isfinite(values).all():
        raise ValueError("JSON tail must be a finite vector")
    length = round(float(values[0]) * (len(values) - 1))
    if not 0 <= values[0] <= 1 or not 0 <= length <= len(values) - 1:
        raise ValueError("invalid JSON tail byte length")
    byte_values = np.rint(values[1:length + 1] * 255)
    if np.any(byte_values < 0) or np.any(byte_values > 255):
        raise ValueError("invalid UTF-8 byte features")
    return json.loads(byte_values.astype(np.uint8).tobytes().decode("utf-8"))


def _onehot(ids: tuple[str, ...], value: str, name: str) -> np.ndarray:
    if value not in ids:
        raise ValueError(f"unknown {name}: {value!r}")
    result = np.zeros(len(ids), np.float32)
    result[ids.index(value)] = 1
    return result


def _number(value: Any, name: str) -> float:
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ValueError(f"{name} must be finite numeric data")
    converted = np.float32(value)
    if not np.isfinite(converted):
        raise ValueError(f"{name} exceeds float32 range")
    return float(converted)


def summarize_public_history(history: Sequence[Mapping[str, Any]]) -> dict[str, Any]:
    """Explicit lossy neural features of verified public transitions.

    Original events remain in PublicTracker/replay. No history is removed from
    observation hashing, belief filtering, or the full encoder mode. Event
    fields match the native PublicTransition/observation v1 projection.
    """
    fields = {"kind", "actor", "nextActor", "phase", "boardChanges", "ownCards", "revealedOpponentCards", "captures", "result"}
    actors = {"white": 0, "black": 0}
    changes = actor_changes = 0
    recent: list[dict[str, Any]] = []
    for event in history:
        if set(event) != fields or event["kind"] != "transition" or event["actor"] not in actors or event["nextActor"] not in actors:
            raise ValueError("summary history requires native public Transition v1 events")
        if not isinstance(event["phase"], str) or not isinstance(event["boardChanges"], (list, tuple)) or not isinstance(event["ownCards"], (list, tuple)) or not isinstance(event["revealedOpponentCards"], (list, tuple)) or not isinstance(event["result"], Mapping):
            raise ValueError("invalid public transition summary input")
        squares = []
        for change in event["boardChanges"]:
            if not isinstance(change, Mapping) or set(change) != {"square", "before", "after"}:
                raise ValueError("invalid public board change")
            square = change["square"]
            if not isinstance(square, Mapping) or set(square) != {"row", "col"} or any(type(square[name]) is not int or not 0 <= square[name] < 8 for name in ("row", "col")):
                raise ValueError("public board change coordinates must lie on the board")
            squares.append([square["row"], square["col"]])
        actors[event["actor"]] += 1
        actor_changes += event["actor"] != event["nextActor"]
        changes += len(squares)
        recent.append({"actor": event["actor"], "nextActor": event["nextActor"], "phase": event["phase"],
                       "board_change_count": len(squares), "board_change_squares": squares,
                       "own_card_count": len(event["ownCards"]), "opponent_card_count": len(event["revealedOpponentCards"]),
                       "outcome": event["result"].get("outcome")})
        if len(recent) > 8:
            recent.pop(0)
    return {"version": HISTORY_SUMMARY_VERSION, "event_count": len(history),
            "history_hash": hashlib.sha256(canonical_json(history).encode()).hexdigest(),
            "actor_counts": actors, "decision_actor_changes": actor_changes,
            "board_change_count": changes, "recent_events": recent}


def _coordinates(value: Any, player: str) -> list[float]:
    if not isinstance(value, Mapping) or "row" not in value or "col" not in value:
        return [-1., -1.]
    row, col = value["row"], value["col"]
    if type(row) is not int or type(col) is not int or not 0 <= row < 8 or not 0 <= col < 8:
        raise ValueError("action coordinates must lie on the 8x8 board")
    if player == "black":
        row, col = 7 - row, 7 - col
    return [row / 7, col / 7]


class PublicEncoder:
    def __init__(self, spec: EncoderSpec):
        self.spec = spec

    def encode(self, observation: PublicObservation | Mapping[str, Any], actions: Sequence[Mapping[str, Any]], *, belief_summary: Mapping[str, Any] | None = None) -> EncodedPosition:
        if isinstance(observation, Mapping):
            observation = PublicObservation.from_native(observation, belief_summary=belief_summary)
        elif belief_summary is not None:
            raise ValueError("belief summary must be attached to the typed public observation or supplied with a native observation")
        if not isinstance(observation, PublicObservation):
            raise TypeError("encoding requires a PublicObservation, never a Position")
        observation = PublicObservation.from_native(observation.to_native(), belief_summary=observation.belief_summary)
        if observation.player not in ("white", "black"):
            raise ValueError("unknown observation player")
        if len(observation.board) != 8 or any(len(row) != 8 for row in observation.board):
            raise ValueError("only an 8x8 public board is supported")
        # These keys cannot be publicly supplied even through auxiliary history.
        if set(observation.public) != {"turn", "ownCards", "opponentHandCount", "publicState", "informationStateKey"}:
            raise ValueError("typed public observation fields differ from the native v1 contract")
        # The public information key is an opaque lookup key, not a feature.
        # It is checked before features are materialized.
        public_data = {"public": {key: value for key, value in observation.public.items() if key != "informationStateKey"}, "history": observation.history, "belief": observation.belief_summary}
        self._reject_private(public_data)
        self._reject_private(observation.board)
        if self.spec.history_encoding == HISTORY_SUMMARY_VERSION:
            public_data["history"] = summarize_public_history(observation.history)
        board = np.zeros((self.spec.board_channels, 8, 8), np.float32)
        for row_index, row in enumerate(observation.board):
            for col_index, piece in enumerate(row):
                if piece is None:
                    continue
                if not isinstance(piece, Mapping):
                    raise ValueError("public board cells must be JSON pieces or null")
                color = piece.get("color")
                if color not in ("white", "black", "neutral") or type(piece.get("moved", False)) is not bool:
                    raise ValueError("piece color and moved flag are invalid")
                prefix = np.concatenate((_onehot(self.spec.piece_ids, piece.get("type"), "piece"), np.array([color == observation.player, color in ("white", "black") and color != observation.player, piece.get("moved", False), 1.], np.float32)))
                encoded = np.concatenate((prefix, _bytes(piece, self.spec.piece_payload_bytes)))
                target_row, target_col = (7-row_index, 7-col_index) if observation.player == "black" else (row_index, col_index)
                board[:, target_row, target_col] = encoded
        condition = self._condition(observation, public_data)
        copied = tuple(json.loads(canonical_json(action)) for action in actions)
        if self.spec.action_encoding == "public-decision-intent-v1":
            for action in copied:
                if any(key in action for key in ("protocolVersion", "positionKey", "positionId", "actionId")):
                    raise ValueError("public decision intent encoding cannot accept execution envelopes or private position metadata")
                self._reject_private(action)
            payloads = copied
        else:
            payloads = tuple(self._payload(action) for action in copied)
        keys = tuple(canonical_json(action) for action in payloads)
        if len(set(keys)) != len(keys):
            raise ValueError("duplicate execution actions are not a policy choice")
        vectors = [self._action(action, observation.player) for action in payloads]
        features = np.stack(vectors) if vectors else np.empty((0, self.spec.action_dim), np.float32)
        return EncodedPosition(board, condition, features, copied, keys, self.spec.digest)

    def _condition(self, observation: PublicObservation, payload: Mapping[str, Any]) -> np.ndarray:
        cards = np.zeros(len(self.spec.card_ids), np.float32)
        public = observation.public
        for slot in public.get("ownCards", ()):
            cards += _onehot(self.spec.card_ids, slot.get("id"), "card")
        game = public.get("publicState", {})
        opponent_cards = np.zeros(len(self.spec.card_ids), np.float32)
        for slot in game.get("revealedOpponentCards", ()):
            if not isinstance(slot, Mapping):
                raise ValueError("revealed opponent cards must be public JSON objects")
            opponent_cards += _onehot(self.spec.card_ids, slot.get("id"), "card")
        rules = np.zeros(len(self.spec.rule_ids), np.float32)
        for rule_id in game.get("rules", ()):
            rules = np.maximum(rules, _onehot(self.spec.rule_ids, rule_id, "rule"))
        counters = np.array([observation.player == "white", *[_number(game.get(key, 0), key) / scale for key, scale in (("actionsRemaining", 16), ("moveCount", 512), ("fullMove", 256))]], np.float32)
        return np.concatenate((cards, opponent_cards, rules, counters, _bytes(payload, self.spec.public_payload_bytes)))

    def _action(self, action: Mapping[str, Any], player: str) -> np.ndarray:
        self._reject_private(action)
        action_type = action.get("type")
        prefix = _onehot(self.spec.action_types, action_type, "action type")
        card = np.zeros(len(self.spec.card_ids), np.float32)
        if "cardId" in action:
            card = _onehot(self.spec.card_ids, action["cardId"], "card")
        coordinates = np.array([*_coordinates(action.get("from"), player), *_coordinates(action.get("move", action.get("destination")), player), *_coordinates(action.get("target"), player)], np.float32)
        return np.concatenate((prefix, card, coordinates, _bytes(action, self.spec.action_payload_bytes)))

    @staticmethod
    def _payload(action: Mapping[str, Any]) -> dict[str, Any]:
        if not isinstance(action, Mapping):
            raise ValueError("a candidate action must be a semantic payload or v1 execution envelope")
        if "protocolVersion" in action:
            if set(action) != {"protocolVersion", "positionId", "actionId", "payload"} or action["protocolVersion"] != "accelerate-action-v1" or not isinstance(action["payload"], Mapping):
                raise ValueError("invalid action execution envelope")
            action = action["payload"]
        # Stale-position checks belong to the execution envelope. Private state
        # identifiers must not alter features for observationally equal games.
        return {key: value for key, value in action.items() if key not in {"positionKey", "positionId", "actionId"}}

    @staticmethod
    def _reject_private(value: Any, depth: int = 0) -> None:
        if depth > 64:
            raise ValueError("public JSON nesting exceeds the supported boundary")
        if isinstance(value, Mapping):
            for key, item in value.items():
                if not isinstance(key, str):
                    raise ValueError("public JSON object keys must be strings")
                if key.lower().replace("_", "") in {"rng", "rngstate", "randomstate", "seed", "randomtape", "opponentcards", "positionkey", "positionid", "hiddenstate", "privatecards", "actualposition"}:
                    raise ValueError(f"private field {key!r} cannot enter public features")
                PublicEncoder._reject_private(item, depth + 1)
        elif isinstance(value, (list, tuple)):
            for item in value:
                PublicEncoder._reject_private(item, depth + 1)


def batch_positions(positions: Sequence[EncodedPosition]) -> EncodedBatch:
    if not positions:
        raise ValueError("an inference batch must contain positions")
    if len({position.spec_digest for position in positions}) != 1:
        raise ValueError("cannot combine incompatible feature contracts")
    largest = max(len(position.actions) for position in positions)
    # Terminal positions have no candidates; one masked placeholder keeps tensor
    # dimensions usable while value remains meaningful.
    largest = max(1, largest)
    features = np.zeros((len(positions), largest, positions[0].action_features.shape[1]), np.float32)
    mask = np.zeros((len(positions), largest), np.bool_)
    for index, position in enumerate(positions):
        count = len(position.actions)
        features[index, :count] = position.action_features
        mask[index, :count] = True
    return EncodedBatch(np.stack([p.board for p in positions]), np.stack([p.condition for p in positions]), features, mask, tuple(positions))
