//! The source renderer's public piece values and badges. Nested rule state,
//! private identities and deadlines are represented by displayed counters.
use crate::*;
use serde_json::{Value, json};

// The policy deliberately uses a small JSON Schema subset. Keep this boundary
// equivalent to the JS/Python validators: numeric integers include 2.0, and
// const/enum compare canonical JSON rather than serde's numeric representation.
fn canonical_equal(left: &Value, right: &Value) -> Result<bool> {
    Ok(serde_jcs::to_vec(left).map_err(EngineError::serialization)?
        == serde_jcs::to_vec(right).map_err(EngineError::serialization)?)
}
fn validate_surface(schema: &Value, value: &Value, path: &str, depth: usize) -> Result<()> {
    const KEYWORDS: &[&str] = &[
        "type",
        "const",
        "enum",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "minItems",
        "maxItems",
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
        "anyOf",
    ];
    let schema = schema.as_object().ok_or_else(|| {
        EngineError::InvalidState("source surface schema must be an object".into())
    })?;
    if depth > 64 || schema.keys().any(|key| !KEYWORDS.contains(&key.as_str())) {
        return Err(EngineError::InvalidState(
            "unsupported source surface schema".into(),
        ));
    }
    let invalid = || EngineError::InvalidState(format!("invalid public surface at {path}"));
    if let Some(branches) = schema.get("anyOf") {
        let branches = branches
            .as_array()
            .filter(|branches| !branches.is_empty())
            .ok_or_else(invalid)?;
        if !branches
            .iter()
            .any(|branch| validate_surface(branch, value, path, depth + 1).is_ok())
        {
            return Err(invalid());
        }
    }
    let actual = match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n)
            if n.as_f64()
                .is_some_and(|n| n.is_finite() && n.fract() == 0.0) =>
        {
            "integer"
        }
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    if let Some(expected) = schema.get("type") {
        let matches = |kind: &Value| {
            kind.as_str()
                .is_some_and(|kind| kind == actual || kind == "number" && actual == "integer")
        };
        if !(matches(expected)
            || expected
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(matches)))
        {
            return Err(invalid());
        }
    }
    if let Some(constant) = schema.get("const")
        && !canonical_equal(value, constant)?
    {
        return Err(invalid());
    }
    if let Some(values) = schema.get("enum") {
        let values = values.as_array().ok_or_else(invalid)?;
        let mut found = false;
        for candidate in values {
            if canonical_equal(value, candidate)? {
                found = true;
                break;
            }
        }
        if !found {
            return Err(invalid());
        }
    }
    if let Some(number) = value.as_f64()
        && (!number.is_finite()
            || schema
                .get("minimum")
                .and_then(Value::as_f64)
                .is_some_and(|min| number < min)
            || schema
                .get("maximum")
                .and_then(Value::as_f64)
                .is_some_and(|max| number > max))
    {
        return Err(invalid());
    }
    if let Some(string) = value.as_str() {
        let length = string.chars().count() as u64;
        if schema
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|min| length < min)
            || schema
                .get("maxLength")
                .and_then(Value::as_u64)
                .is_some_and(|max| length > max)
        {
            return Err(invalid());
        }
    }
    if let Some(array) = value.as_array() {
        let length = array.len() as u64;
        if schema
            .get("minItems")
            .and_then(Value::as_u64)
            .is_some_and(|min| length < min)
            || schema
                .get("maxItems")
                .and_then(Value::as_u64)
                .is_some_and(|max| length > max)
        {
            return Err(invalid());
        }
        if let Some(items) = schema.get("items") {
            for (index, value) in array.iter().enumerate() {
                validate_surface(items, value, &format!("{path}[{index}]"), depth + 1)?;
            }
        }
    }
    if let Some(object) = value.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);
        let required_missing = schema
            .get("required")
            .and_then(Value::as_array)
            .is_some_and(|required| {
                required
                    .iter()
                    .any(|key| key.as_str().is_none_or(|key| !object.contains_key(key)))
            });
        let unknown = schema.get("additionalProperties") == Some(&json!(false))
            && object
                .keys()
                .any(|key| properties.is_none_or(|properties| !properties.contains_key(key)));
        if required_missing || unknown {
            return Err(invalid());
        }
        if let Some(properties) = properties {
            for (key, value) in object {
                if let Some(property) = properties.get(key) {
                    validate_surface(property, value, &format!("{path}.{key}"), depth + 1)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_public_piece_with_policy(
    piece: &Value,
    path: &str,
    policy: &crate::state::ObservationPolicy,
) -> Result<()> {
    if piece.as_object().is_none_or(|fields| {
        fields
            .keys()
            .any(|key| !policy.piece_public_fields.contains(key))
    }) {
        return Err(EngineError::InvalidState(format!(
            "unknown public piece field at {path}"
        )));
    }
    validate_surface(&policy.public_piece_schema, piece, path, 0)
}
pub(crate) fn validate_public_piece(piece: &Value, path: &str) -> Result<()> {
    validate_public_piece_with_policy(piece, path, crate::state::observation_policy())
}
fn validate_public_cards_with_policy(
    cards: &[Value],
    path: &str,
    policy: &crate::state::ObservationPolicy,
) -> Result<()> {
    for card in cards {
        let fields = card
            .as_object()
            .ok_or_else(|| EngineError::InvalidState(format!("invalid public card at {path}")))?;
        if fields
            .keys()
            .any(|key| !policy.card_public_fields.contains(key))
        {
            return Err(EngineError::InvalidState(format!(
                "unknown public card field at {path}"
            )));
        }
        if let Some(revealed) = fields.get("revealed") {
            validate_surface(
                &policy.card_revelation_schema,
                revealed,
                &format!("{path}.revealed"),
                0,
            )?;
        }
    }
    Ok(())
}
pub(crate) fn validate_public_cards(cards: &[Value], path: &str) -> Result<()> {
    validate_public_cards_with_policy(cards, path, crate::state::observation_policy())
}
pub(crate) fn validate_projection(observation: &Observation) -> Result<()> {
    validate_projection_for_ruleset(observation, RULES_VERSION_V6)
}

/// The frozen v7 client emits no selection hints outside the acting side's
/// ordinary play window. Active hints call `getLegalMoves` and
/// `getVisibleCardTargetSquares`; until both v7 kernels have source parity,
/// a partial move/card list would be a false public contract.
pub(crate) fn public_hints_v7(state: &GameState, viewer: Color) -> Result<Value> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 public hints for rules version {}",
            state.ruleset_id
        )));
    }
    if state.mode != "play"
        || state.turn != viewer
        || truth(state.extra.get("pendingPromotion"))
        || truth(state.extra.get("activeTrolley"))
    {
        return Ok(json!({"moves":[],"cardTargets":[]}));
    }
    Err(EngineError::UnsupportedFeature(
        "v7 active public move and card target hints".into(),
    ))
}

/// Validate projected field shapes against the selected frozen policy. This
/// does not establish dynamic source parity or admit a v7 executable Position.
pub(crate) fn validate_projection_for_ruleset(
    observation: &Observation,
    ruleset_id: &str,
) -> Result<()> {
    let policy = crate::state::observation_policy_for_ruleset(ruleset_id)?;
    if observation.protocol_version != crate::state::observation_protocol_for_ruleset(ruleset_id)?
        || observation
            .public_state
            .get("projectionVersion")
            .and_then(Value::as_str)
            != Some(crate::state::observation_projection_for_ruleset(
                ruleset_id,
            )?)
        || observation
            .public_state
            .get("rulesVersion")
            .and_then(Value::as_str)
            != Some(ruleset_id)
        || observation
            .public_state
            .get("observationPolicyHash")
            .and_then(Value::as_str)
            != Some(crate::state::observation_policy_hash_for_ruleset(
                ruleset_id,
            )?)
    {
        return Err(EngineError::InvalidState(
            "public observation policy identity mismatch".into(),
        ));
    }
    for (key, value) in &observation.public_state {
        if policy.state_public_fields.contains(key) {
            let schema = policy.state_value_schemas.get(key).ok_or_else(|| {
                EngineError::InvalidState(format!("missing source state surface schema for {key}"))
            })?;
            validate_surface(schema, value, &format!("publicState.{key}"), 0)?;
        } else if !policy.derived_public_fields.contains(key) {
            return Err(EngineError::InvalidState(format!(
                "unknown public state field {key}"
            )));
        }
    }
    for key in ["boardMarks", "relationships", "overlays"] {
        let schema = policy.surface_schemas.get(key).ok_or_else(|| {
            EngineError::InvalidState(format!("missing source surface schema {key}"))
        })?;
        let value = observation
            .public_state
            .get(key)
            .ok_or_else(|| EngineError::InvalidState(format!("missing source surface {key}")))?;
        validate_surface(schema, value, key, 0)?;
    }
    validate_surface(
        &policy.selection_schema,
        observation
            .public_state
            .get("selectionPhase")
            .unwrap_or(&Value::Null),
        "selectionPhase",
        0,
    )?;
    validate_surface(
        &policy.deathmatch_schema,
        observation
            .public_state
            .get("deathmatchStatus")
            .ok_or_else(|| EngineError::InvalidState("missing deathmatch status surface".into()))?,
        "deathmatchStatus",
        0,
    )?;
    for row in &observation.board {
        for piece in row.iter().flatten() {
            validate_public_piece_with_policy(piece, "board.piece", policy)?;
        }
    }
    validate_public_cards_with_policy(&observation.own_cards, "ownCards", policy)?;
    for key in ["revealedOpponentCards", "draft"] {
        let cards = if key == "draft" {
            observation
                .public_state
                .get(key)
                .and_then(|v| v.get("choices"))
        } else {
            observation.public_state.get(key)
        };
        if let Some(cards) = cards.and_then(Value::as_array) {
            validate_public_cards_with_policy(cards, key, policy)?;
        }
    }
    for event in &observation.history {
        for change in event
            .get("boardChanges")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for key in ["before", "after"] {
                if let Some(piece) = change.get(key).filter(|v| !v.is_null()) {
                    validate_public_piece_with_policy(piece, "history.piece", policy)?;
                }
            }
        }
        for key in ["ownCards", "revealedOpponentCards"] {
            if let Some(cards) = event.get(key).and_then(Value::as_array) {
                validate_public_cards_with_policy(cards, "history.cards", policy)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn truth(value: Option<&Value>) -> bool {
    value.is_some_and(|v| match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    })
}
pub(crate) fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(|v| match v {
            Value::Null => Some(0.0),
            Value::Bool(v) => Some(f64::from(u8::from(*v))),
            Value::Number(v) => v.as_f64(),
            Value::String(v) if v.trim().is_empty() => Some(0.0),
            Value::String(v) => v.trim().parse().ok(),
            _ => None,
        })
        .filter(|v| v.is_finite())
}

/// Local warning eligibility from main94747/94868/94923. This is the source
/// predicate, not a prediction of the next winner or DOM-toast lifetime.
pub(crate) fn deathmatch_status(state: &GameState) -> Value {
    let deathmatch = state.extra.get("deathmatch");
    let active = truth(deathmatch.and_then(|value| value.get("active")));
    let limit = number(state.extra.get("deathmatchLimitTurns"))
        .filter(|value| *value > 0.0)
        .map(|value| (value + 0.5).floor().max(1.0))
        .unwrap_or(10.0);
    let interval = number(deathmatch.and_then(|value| value.get("intervalHalfTurns")))
        .filter(|value| *value != 0.0)
        .unwrap_or(limit * 2.0)
        .max(1.0);
    let count = number(deathmatch.and_then(|value| value.get("halfTurnsSinceProgress")))
        .unwrap_or(0.0)
        .clamp(0.0, interval);
    let remaining = (interval - count).max(0.0);
    let warning = deathmatch.and_then(|value| value.get("active")) == Some(&Value::Bool(true))
        && state.mode == "play"
        && !truth(deathmatch.and_then(|value| value.get("progressThisTurn")))
        && remaining > 0.0
        && remaining <= 2.0;
    json!({"active":active,"warning":warning})
}
fn nested<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    value.and_then(|v| v.get(key))
}
fn count(status: &mut Fields, name: &str, value: f64) {
    status.insert(name.into(), json!(value));
}
fn flag(status: &mut Fields, name: &str, active: bool) {
    if active {
        status.insert(name.into(), json!(true));
    }
}
fn ability(piece: &Piece) -> &str {
    if piece.kind == "trickster" && piece.ability_kind() == "trickster" {
        "queen"
    } else {
        piece.ability_kind()
    }
}
fn cooling(state: &GameState, piece: &Piece, turns: f64) -> bool {
    let p = &piece.extra;
    if truth(p.get("repositionSecondMove")) {
        return true;
    }
    let fresh = number(p.get("freshNoCaptureUntil")).is_some_and(|v| v != 0.0 && turns < v);
    let card = number(p.get("cardNoCaptureUntil")).is_some_and(|v| v != 0.0 && turns < v)
        || number(p.get("promotionRushUntil")).is_some_and(|end| end != 0.0 && turns < end);
    let free = truth(nested(
        state.extra.get("freeMoveCaptureLock"),
        piece.color.as_str(),
    ));
    let quantum = truth(nested(
        state.extra.get("quantumPending"),
        piece.color.as_str(),
    )) || number(p.get("quantumNoCaptureUntil"))
        .is_some_and(|v| v != 0.0 && turns < v);
    let manner = (truth(state.extra.get("coolGuy")) || truth(p.get("potionManner")))
        && truth(p.get("coolGuyCapturedLast"));
    let locked = fresh
        || piece.kind != "monster"
            && (free
                || card
                || manner
                || !matches!(piece.kind.as_str(), "checker" | "checkerKing") && quantum);
    let chaos = number(state.extra.get("chaosNoCaptureUntilHalfTurn")).is_some_and(|v| {
        v > f64::from(state.turns_taken.white) + f64::from(state.turns_taken.black)
    });
    locked && (!(fresh && chaos) || free || card || quantum || manner)
}

fn array(value: Option<&Value>) -> impl Iterator<Item = &Value> {
    value.and_then(Value::as_array).into_iter().flatten()
}
fn square(value: &Value) -> Option<Square> {
    let row = number(value.get("row"))?;
    let col = number(value.get("col"))?;
    (row.fract() == 0.0
        && col.fract() == 0.0
        && (0.0..8.0).contains(&row)
        && (0.0..8.0).contains(&col))
    .then_some(Square {
        row: row as u8,
        col: col as u8,
    })
}
fn first_piece<'a>(state: &'a GameState, id: &str) -> Option<(Square, &'a Piece)> {
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .find_map(|square| state.at(square).filter(|p| p.id == id).map(|p| (square, p)))
}
fn remaining_half(entry: &Value, move_count: u32) -> f64 {
    if let Some(remaining) = number(entry.get("remainingHalfTurns"))
        .filter(|n| n.fract() == 0.0 && *n >= 0.0 && *n <= 9007199254740991.0)
    {
        return remaining;
    }
    let due = number(entry.get("dueMoveCount"))
        .unwrap_or(0.0)
        .floor()
        .max(0.0);
    if due != 0.0 {
        (due - f64::from(move_count)).max(0.0)
    } else {
        0.0
    }
}
fn royal_like(state: &GameState, piece: &Piece) -> bool {
    state.royal_identity(piece)
        || matches!(
            piece.kind.as_str(),
            "merchant" | "timeTraveler" | "vampireLord"
        )
}
fn crown_entries(value: Option<&Value>) -> Vec<&Value> {
    value
        .filter(|v| truth(Some(v)))
        .map(|v| {
            v.get("crowns")
                .and_then(Value::as_array)
                .filter(|a| !a.is_empty())
                .map(|a| a.iter().collect())
                .unwrap_or_else(|| vec![v])
        })
        .unwrap_or_default()
}
fn royal_badges(state: &GameState, piece: &Piece, at: Square, status: &mut Fields) {
    let royal = state.royal_identity(piece);
    let turns = piece
        .color
        .owner()
        .map(|c| f64::from(*state.turns_taken.get(c)))
        .unwrap_or(0.0);
    if royal {
        let pawn = number(nested(
            nested(state.extra.get("effects"), "pawnReverse"),
            piece.color.as_str(),
        ))
        .unwrap_or(0.0)
        .max(0.0);
        if pawn > 0.0 {
            count(status, "pawnReverseRemaining", pawn.min(9.0));
        }
        if let Some(window) = nested(state.extra.get("royalCommand"), piece.color.as_str())
            && let Some(start) =
                number(window.get("activeTurn")).filter(|n| n.fract() == 0.0 && *n >= 0.0)
        {
            let end = number(window.get("expiresTurn"))
                .filter(|n| n.fract() == 0.0 && *n > start)
                .unwrap_or(start + 1.0);
            let remaining = (end - turns.max(start)).max(0.0);
            if remaining > 0.0 {
                count(status, "royalCommandRemaining", remaining.min(9.0));
            }
        }
        let prophecy = number(nested(
            nested(state.extra.get("prophecy"), piece.color.as_str()),
            "remainingHalfTurns",
        ))
        .unwrap_or(0.0)
        .clamp(0.0, 6.0);
        if prophecy > 0.0 {
            count(
                status,
                "prophecyRemaining",
                (prophecy / 2.0).ceil().clamp(1.0, 3.0),
            );
        }
        let royals = state
            .board
            .iter()
            .enumerate()
            .flat_map(|(row, cells)| {
                cells.iter().enumerate().filter_map(move |(col, p)| {
                    p.as_ref()
                        .filter(|p| p.color == piece.color && state.royal_identity(p))
                        .map(|p| {
                            (
                                Square {
                                    row: row as u8,
                                    col: col as u8,
                                },
                                p,
                            )
                        })
                })
            })
            .collect::<Vec<_>>();
        let anchor = royals
            .iter()
            .find(|(_, p)| p.kind == "king")
            .or_else(|| royals.first());
        if anchor.is_some_and(|(s, p)| *s == at && p.id == piece.id) {
            let remaining = array(state.extra.get("undeadResurrections"))
                .filter(|entry| {
                    entry.get("color").and_then(Value::as_str) == Some(piece.color.as_str())
                })
                .map(|e| remaining_half(e, state.move_count))
                .filter(|n| *n > 0.0)
                .reduce(f64::min);
            if let Some(remaining) = remaining {
                count(
                    status,
                    "undeadResurrectionRemaining",
                    (remaining / 2.0).ceil(),
                );
            }
        }
    }
    if (royal || piece.kind == "merchant")
        && let Some(value) = state.extra.get("ultimatum").filter(|v| truth(Some(v)))
    {
        let remaining = if let Some(half) = number(value.get("remainingHalfTurns"))
            .filter(|n| n.fract() == 0.0 && n.abs() <= 9007199254740991.0)
        {
            (half / 2.0).ceil()
        } else if let Some(end) = number(value.get("expiresFullMove")) {
            end - f64::from(state.full_move.max(1))
        } else {
            number(value.get("remaining")).unwrap_or(0.0)
        };
        if remaining > 0.0 {
            count(status, "ultimatumRemaining", remaining.clamp(1.0, 4.0));
        }
    }
    if royal_like(state, piece) {
        let armistice = state.extra.get("armistice");
        let remaining = number(nested(armistice, "remaining").or(armistice))
            .unwrap_or(0.0)
            .floor()
            .max(0.0);
        if remaining > 0.0 {
            count(status, "armisticeRemaining", remaining.clamp(1.0, 9.0));
        }
        let gale = array(state.extra.get("pendingGales"))
            .map(|entry| {
                let remaining = entry
                    .get("remainingOwnTurns")
                    .filter(|v| !v.is_null())
                    .or_else(|| entry.get("remaining"));
                number(remaining)
                    .map(|n| n.floor().max(0.0))
                    .unwrap_or_else(|| {
                        (number(entry.get("triggerTurn")).unwrap_or(0.0)
                            - number(nested(
                                state.extra.get("turnsTaken"),
                                entry.get("color").and_then(Value::as_str).unwrap_or(""),
                            ))
                            .unwrap_or_else(|| {
                                match entry.get("color").and_then(Value::as_str) {
                                    Some("white") => f64::from(state.turns_taken.white),
                                    Some("black") => f64::from(state.turns_taken.black),
                                    _ => 0.0,
                                }
                            }))
                        .max(0.0)
                    })
            })
            .filter(|n| *n > 0.0)
            .reduce(f64::min);
        if let Some(gale) = gale {
            count(status, "galeRemaining", gale.clamp(1.0, 9.0));
        }
        let ice = state
            .board
            .iter()
            .flatten()
            .flatten()
            .filter(|p| {
                let ranged =
                    matches!(
                        p.kind.as_str(),
                        "magicGirl"
                            | "berserker"
                            | "trickster"
                            | "brutus"
                            | "bigBishop"
                            | "rook"
                            | "bishop"
                            | "queen"
                            | "bear"
                            | "amazon"
                            | "cardinal"
                            | "cannon"
                            | "herald"
                            | "hook"
                            | "protestant"
                            | "windmill"
                            | "windmillBishop"
                            | "windmillRook"
                            | "bigRook"
                            | "jester"
                            | "idol"
                    ) || p.kind == "princess"
                        && !state.board.iter().flatten().flatten().any(|q| {
                            q.kind == "queen" && q.color == p.color && !q.flag("regencyHeir")
                        });
                p.color == piece.color && truth(p.extra.get("iceSheet")) && ranged
            })
            .map(|p| {
                number(nested(p.extra.get("iceSheet"), "remaining"))
                    .unwrap_or(0.0)
                    .max(0.0)
            })
            .fold(0.0, f64::max);
        if ice > 0.0 {
            count(status, "iceSheetKingRemaining", ice.clamp(1.0, 9.0));
        }
    }
    if truth(piece.extra.get("crownBearer")) {
        let tokens = piece.extra.get("crownTokenIds").and_then(Value::as_array);
        let held = crown_entries(state.extra.get("crownRule"))
            .into_iter()
            .enumerate()
            .filter(|(index, entry)| {
                entry
                    .get("holderId")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty() && s == piece.id)
                    || tokens.is_some_and(|ids| {
                        ids.contains(
                            &entry
                                .get("id")
                                .filter(|v| truth(Some(v)))
                                .cloned()
                                .unwrap_or_else(|| json!(format!("crown-{}", index + 1))),
                        )
                    })
            })
            .map(|(_, entry)| {
                let divide = if entry.get("countUnit").and_then(Value::as_str) == Some("full-turn")
                {
                    1.0
                } else {
                    2.0
                };
                (number(nested(entry.get("heldMoves"), piece.color.as_str())).unwrap_or(0.0)
                    / divide)
                    .floor()
                    .max(0.0)
            })
            .fold(0.0, f64::max);
        count(status, "crownHeldMoves", held);
    }
    if let Some(id) = piece
        .extra
        .get("feudalContractId")
        .filter(|v| truth(Some(v)))
        && let Some(contract) = array(state.extra.get("feudalContracts")).find(|entry| {
            entry.get("id") == Some(id)
                && entry.get("pawnId").and_then(Value::as_str) == Some(&piece.id)
        })
        && let Some((_, guardian)) = contract
            .get("guardianId")
            .and_then(Value::as_str)
            .and_then(|id| first_piece(state, id))
    {
        status.insert("feudalGuardianType".into(), json!(guardian.kind));
    }
}
fn potion_active(piece: &Piece, id: &str) -> bool {
    let p = &piece.extra;
    match id {
        "mannerNoCapture" => truth(p.get("potionManner")) && truth(p.get("coolGuyCapturedLast")),
        "saturationNoCapture" => {
            truth(p.get("potionSaturation")) && number(p.get("capturesMade")).unwrap_or(0.0) >= 3.0
        }
        "poisonStun" => number(p.get("poisonStunTurns")).unwrap_or(0.0) > 0.0,
        "shield" => truth(p.get("shielded")),
        "stealth" => truth(p.get("hiddenFrom")),
        "submerge" => truth(p.get("submerged")),
        "freeze" => truth(p.get("frozen")),
        "stake" => truth(p.get("staked")),
        "disarm" => truth(p.get("disarmed")),
        "severance" => truth(p.get("severed")),
        "outpostProtection" => truth(p.get("outpostProtected")),
        "queensGambitProtection" => truth(p.get("protected")),
        "sacrificeProtection"
        | "lastResistance"
        | "coronationProtection"
        | "evasion"
        | "parry"
        | "basicTraining"
        | "loyalist"
        | "ghost"
        | "chameleon"
        | "chimera"
        | "witchTrial"
        | "callingCard"
        | "emptyLunchbox"
        | "explosive"
        | "poisonedPawn"
        | "trojanHorse"
        | "inertia"
        | "nullification"
        | "recurrence"
        | "grapplerBound" => truth(p.get(id)),
        _ => false,
    }
}

pub(crate) fn piece_view(
    state: &GameState,
    piece: &Piece,
    square: Square,
    viewer: Color,
    visual_type: &str,
) -> Value {
    let p = &piece.extra;
    let mut status = Fields::new();
    let turns = piece
        .color
        .owner()
        .map(|color| f64::from(*state.turns_taken.get(color)))
        .unwrap_or(0.0);
    let shared = f64::from(state.turns_taken.white.min(state.turns_taken.black));
    let full = state.mode == "gameover";
    let own = piece.color == viewer;
    let trickster_visible = full || own && state.mode == "play";
    for name in [
        "chameleon",
        "chimera",
        "staked",
        "explosive",
        "brutalKnight",
        "bribed",
        "witchTrial",
        "disarmed",
        "severed",
        "inertia",
        "frenzy",
        "crownBearer",
        "iceSheet",
        "lastResistance",
        "sacrificeProtection",
        "necromancy",
        "regencyHeir",
        "bloodCurse",
        "poisonedPawn",
        "evasion",
        "ghost",
        "metalized",
        "loyalist",
        "parry",
        "emptyLunchbox",
        "nullification",
        "recurrence",
        "wanted",
        "grapplerBound",
        "callingCard",
        "basicTraining",
    ] {
        flag(&mut status, name, truth(p.get(name)));
    }
    flag(
        &mut status,
        "protected",
        truth(p.get("protected"))
            || number(nested(p.get("vigilanceProtection"), "remaining")).is_some_and(|v| v > 0.0),
    );
    flag(
        &mut status,
        "holdout",
        piece.kind == "pawn" && truth(p.get("holdoutPromotion")),
    );
    flag(&mut status, "cooling", cooling(state, piece, turns));
    let exhaustion = nested(state.extra.get("exhaustion"), piece.color.as_str());
    flag(
        &mut status,
        "exhaustionLocked",
        piece.color.owner().is_some()
            && !piece.flag("regencyHeir")
            && !piece.flag("crownRoyal")
            && !matches!(
                piece.kind.as_str(),
                "king" | "royalKnight" | "shotgunKing" | "darkWizard"
            )
            && truth(nested(exhaustion, "enabled"))
            && nested(exhaustion, "pieceId").and_then(Value::as_str) == Some(&piece.id)
            && number(nested(exhaustion, "count")).unwrap_or(0.0).floor() >= 3.0,
    );
    let dice = nested(state.extra.get("diceLocks"), piece.color.as_str());
    flag(
        &mut status,
        "diceLocked",
        number(nested(dice, "remaining")).unwrap_or(0.0) > 0.0
            && nested(dice, "type")
                .and_then(Value::as_str)
                .is_some_and(|kind| {
                    if kind == "king" {
                        state.royal_identity(piece)
                    } else {
                        kind == piece.kind
                    }
                }),
    );
    flag(&mut status, "quantum", truth(p.get("quantum")));
    flag(
        &mut status,
        "quantumShadow",
        truth(p.get("isQuantumShadow")),
    );
    flag(&mut status, "twinsLinked", truth(p.get("twinBondId")));
    flag(
        &mut status,
        "regencyRoyal",
        piece.flag("regencyHeir") && state.royal_identity(piece),
    );
    flag(
        &mut status,
        "stealthed",
        (full || own)
            && (truth(p.get("hiddenFrom"))
                || piece
                    .color
                    .owner()
                    .is_some_and(|owner| state.flag("camouflageRule", owner.opponent()))
                    && !state.royal_identity(piece)
                    && ((number(p.get("anchorRow")).unwrap_or(f64::from(square.row))
                        + number(p.get("anchorCol")).unwrap_or(f64::from(square.col)))
                        % 2.0
                        == 0.0)
                        == (piece.color == Color::White)),
    );
    flag(
        &mut status,
        "magicGirlAwakened",
        piece.kind == "magicGirl"
            && p.get("simpleEditorMagicGirlAwakened")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| state.flag("magicGirlSurge", piece.color)),
    );
    let saturation = if truth(state.extra.get("saturationRule")) || truth(p.get("potionSaturation"))
    {
        number(p.get("capturesMade"))
            .unwrap_or(0.0)
            .floor()
            .clamp(0.0, 3.0)
    } else {
        0.0
    };
    flag(&mut status, "saturationLocked", saturation >= 3.0);
    if saturation > 0.0 {
        count(&mut status, "saturationCaptures", saturation);
    }
    let restriction = p
        .get("captureRestriction")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            if piece.kind == "trickster"
                || state.flag("hallucination", piece.color) && visual_type == "queen"
            {
                match ability(piece) {
                    "guard" | "revolvingDoor" => Some("immune"),
                    "jester" => Some("royal-only"),
                    _ => None,
                }
            } else {
                None
            }
        });
    if let Some(restriction) = restriction {
        status.insert("captureRestriction".into(), json!(restriction));
    }
    flag(
        &mut status,
        "sirenWarning",
        state
            .extra
            .get("sirenExposure")
            .and_then(Value::as_object)
            .is_some_and(|groups| {
                groups.values().any(|group| {
                    number(group.get(&piece.id)).is_some_and(|n| (1.0..2.0).contains(&n.floor()))
                })
            }),
    );
    let captures = number(p.get("totalCaptures")).unwrap_or(0.0).max(0.0);
    if piece.kind == "pawn"
        && state.flag("fieldPromotion", piece.color)
        && !truth(p.get("specialPromotionUsed"))
        && (captures == 1.0 || truth(state.extra.get("recycling")) && captures >= 2.0)
    {
        flag(&mut status, "fieldPromotionReady", captures >= 2.0);
        count(
            &mut status,
            "fieldPromotionCapturesRemaining",
            if captures >= 2.0 { 0.0 } else { 1.0 },
        );
    }
    if piece.kind == "bishop"
        && let Some(remaining) = nested(state.extra.get("bishopInfiltration"), piece.color.as_str())
            .and_then(Value::as_f64)
            .filter(|n| *n > 0.0)
    {
        count(&mut status, "ghillieRemaining", remaining);
    }
    royal_badges(state, piece, square, &mut status);
    if ability(piece) == "reaper" {
        count(
            &mut status,
            "reaperCaptures",
            number(p.get("reaperCaptures"))
                .unwrap_or(0.0)
                .floor()
                .clamp(0.0, 4.0),
        );
    }
    if piece.kind == "pawn" && truth(p.get("vipInvitation")) {
        count(
            &mut status,
            "vipRemaining",
            (number(nested(p.get("vipInvitation"), "triggerTurn")).unwrap_or(0.0) - turns)
                .clamp(1.0, 3.0),
        );
    }
    if piece.kind == "babyBear"
        && (truth(p.get("babyBearGrowAtTurn")) || truth(p.get("babyBearGrowAtMove")))
    {
        let growth = number(p.get("babyBearGrowAtTurn"))
            .map(|n| n - shared)
            .or_else(|| {
                number(p.get("babyBearGrowAtMove"))
                    .map(|n| ((n - f64::from(state.move_count)) / 2.0).ceil())
            })
            .unwrap_or(9.0);
        count(
            &mut status,
            "babyBearGrowthRemaining",
            growth.clamp(1.0, 9.0),
        );
    } else if piece.kind == "bear"
        && number(p.get("bearRetaliationsRemaining")).is_some_and(|n| n > 0.0)
    {
        count(
            &mut status,
            "retaliationsRemaining",
            number(p.get("bearRetaliationsRemaining"))
                .unwrap()
                .clamp(1.0, 2.0),
        );
    }
    if truth(p.get("metalized")) && number(p.get("metalCooldown")).is_some_and(|n| n > 0.0) {
        count(
            &mut status,
            "metalCooldown",
            number(p.get("metalCooldown")).unwrap(),
        );
    }
    if truth(p.get("emptyLunchbox")) {
        count(
            &mut status,
            "emptyLunchboxRemaining",
            (number(nested(p.get("emptyLunchbox"), "deadlineTurn")).unwrap_or(0.0) - turns)
                .clamp(0.0, 9.0),
        );
    }
    let poison = number(p.get("poisonStunTurns"))
        .unwrap_or(0.0)
        .floor()
        .max(0.0);
    flag(&mut status, "poisonStunned", poison > 0.0);
    if poison > 0.0 {
        count(
            &mut status,
            "poisonStunRemaining",
            number(p.get("poisonStunTurns")).unwrap().clamp(1.0, 9.0),
        );
    }
    let frozen = number(nested(p.get("frozenByCard"), "remaining"))
        .unwrap_or(0.0)
        .clamp(0.0, 9.0);
    if frozen > 0.0 {
        count(&mut status, "frozenRemaining", frozen);
    }
    let holdout = if piece.kind == "pawn" && truth(p.get("holdoutPromotion")) {
        number(nested(p.get("holdoutPromotion"), "readyTurn"))
            .or_else(|| number(nested(p.get("holdoutPromotion"), "readyMove")))
            .unwrap_or(0.0)
            - shared
    } else {
        0.0
    };
    let severance = number(nested(p.get("severed"), "remaining"))
        .or_else(|| {
            number(nested(p.get("severed"), "expiresFullMove"))
                .map(|v| v - f64::from(state.full_move.max(1)))
        })
        .unwrap_or(0.0);
    for (name, n, max) in [
        ("holdoutRemaining", holdout, 9007199254740991.0),
        (
            "stakedRemaining",
            number(nested(p.get("staked"), "remaining")).unwrap_or(0.0),
            9007199254740991.0,
        ),
        ("severanceRemaining", severance, 9.0),
        (
            "lastResistanceRemaining",
            number(nested(p.get("lastResistance"), "remaining")).unwrap_or(0.0),
            9.0,
        ),
        (
            "sacrificeProtectionRemaining",
            number(nested(p.get("sacrificeProtection"), "remaining")).unwrap_or(0.0),
            9.0,
        ),
    ] {
        if n > 0.0 {
            count(&mut status, name, n.clamp(1.0, max));
        }
    }
    if truth(p.get("witchTrial")) {
        count(
            &mut status,
            "witchTrialRemaining",
            number(nested(p.get("witchTrial"), "remaining"))
                .filter(|v| *v != 0.0)
                .unwrap_or(1.0)
                .clamp(1.0, 9.0),
        );
    }
    if piece.kind == "trickster" && trickster_visible {
        status.insert("tricksterMovement".into(), json!(ability(piece)));
    }
    if state.royal_identity(piece) && state.flag("kingKnight", piece.color) {
        status.insert("horseRiding".into(), json!("knight"));
    }
    if let Some(types) = p.get("imperialMoves").and_then(Value::as_array) {
        let excluded = [
            "bigBishop",
            "",
            "king",
            "royalKnight",
            "shotgunKing",
            "merchant",
            "recruiter",
            "wall",
            "scarecrow",
            "football",
            "blackHole",
            "colossus",
            "bigRook",
            "coffin",
            "log",
            "timeTraveler",
            "wizard",
        ];
        let mut unique = Vec::new();
        for kind in types
            .iter()
            .filter_map(Value::as_str)
            .filter(|s| !excluded.contains(s))
        {
            if !unique.contains(&kind) {
                unique.push(kind);
            }
        }
        if !unique.is_empty() {
            status.insert("imperialStudyTypes".into(), json!(unique));
        }
    }
    if piece.kind == "merchant" {
        count(&mut status, "gold", number(p.get("gold")).unwrap_or(0.0));
    }
    if truth(p.get("bribedRemaining")) {
        count(
            &mut status,
            "bribedRemaining",
            number(p.get("bribedRemaining")).unwrap_or(0.0),
        );
    }
    let necro = number(p.get("necromancyRemaining"))
        .filter(|v| *v != 0.0)
        .or_else(|| number(nested(p.get("necromancy"), "remaining")))
        .unwrap_or(0.0)
        .clamp(0.0, 9.0);
    if necro > 0.0 {
        count(&mut status, "necromancyRemaining", necro);
    }
    for kind in ["medium", "parrot"] {
        if ability(piece) == kind && (piece.kind != "trickster" || trickster_visible) {
            let memory = if kind == "medium" {
                state.extra.get("mediumMovement")
            } else {
                nested(state.extra.get("parrotMovement"), piece.color.as_str())
            };
            if let Some(typ) = nested(memory, "type")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                let mut output = String::new();
                let mut upper = false;
                for ch in typ.chars() {
                    if ch == '-' {
                        upper = true;
                    } else if upper {
                        output.extend(ch.to_uppercase());
                        upper = false;
                    } else {
                        output.push(ch);
                    }
                }
                if output == "windmill" {
                    output =
                        if nested(memory, "windmillMode").and_then(Value::as_str) == Some("rook") {
                            "windmillRook"
                        } else {
                            "windmillBishop"
                        }
                        .into();
                }
                status.insert(format!("{kind}Movement"), json!(output));
            }
        }
    }
    flag(
        &mut status,
        "locustReady",
        state.flag("locustSwarm", piece.color)
            && matches!(piece.kind.as_str(), "knight" | "bishop" | "camel" | "rook")
            && !piece.moved
            && !truth(p.get("locustUsed"))
            && p.get("locustOrigin")
                .is_some_and(|v| v["row"] == json!(square.row) && v["col"] == json!(square.col)),
    );
    if full {
        flag(
            &mut status,
            "spy",
            p.get("spyOwner")
                .and_then(Value::as_str)
                .is_some_and(|s| ["white", "black"].contains(&s)),
        );
        flag(&mut status, "trojanHorse", truth(p.get("trojanHorse")));
    }
    if number(nested(
        nested(state.extra.get("hallucination"), viewer.as_str()),
        "remaining",
    ))
    .unwrap_or(0.0)
        <= 0.0
    {
        let mut active = Vec::new();
        for id in p
            .get("potionEffects")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if potion_active(piece, id) && !active.contains(&id) {
                active.push(id);
            }
        }
        if !active.is_empty() {
            status.insert("potionEffects".into(), json!(active));
        }
    }
    let mut view = json!({"type":visual_type,"color":piece.color,"status":status});
    if own || full {
        view["moved"] = json!(piece.moved);
    }
    for name in ["shielded", "frozen", "submerged"] {
        if p.contains_key(name) {
            view[name] = json!(truth(p.get(name)));
        }
    }
    if piece.is_large()
        || piece.kind == "shotgunKing"
        || state.royal_identity(piece)
            && truth(p.get("undergroundBunker"))
            && number(p.get("hp")).is_some()
    {
        let max = number(p.get("maxHp"))
            .filter(|v| *v != 0.0)
            .or_else(|| number(p.get("hp")).filter(|v| *v != 0.0))
            .unwrap_or(1.0)
            .max(1.0);
        view["maxHp"] = json!(max);
        view["hp"] = json!(number(p.get("hp")).unwrap_or(max).clamp(0.0, max));
    }
    if ability(piece) == "wizard"
        && (piece.kind != "trickster" || trickster_visible)
        && let Some(mana) = number(p.get("mana"))
    {
        view["mana"] = json!(mana);
        if own {
            view["maxMana"] = json!(number(p.get("maxMana")).unwrap_or(5.0));
        }
    }
    if piece.kind == "shotgunKing" {
        view["facing"] = p
            .get("facing")
            .filter(|v| truth(Some(v)))
            .cloned()
            .unwrap_or_else(|| {
                json!(if piece.color == Color::White {
                    "up"
                } else {
                    "down"
                })
            });
        if own {
            view["ammo"] = json!(number(p.get("ammo")).unwrap_or(0.0));
            view["maxAmmo"] = json!(number(p.get("maxAmmo")).unwrap_or(3.0));
        }
    }
    if piece.is_large() {
        for name in ["anchorRow", "anchorCol"] {
            if let Some(value) = p.get(name).filter(|v| v.as_i64().is_some()) {
                view[name] = value.clone();
            }
        }
    }
    if piece.kind == "log"
        && let Some(dir) = p.get("logDir").filter(|v| truth(Some(v)))
    {
        view["logDir"] = json!({"dr":dir["dr"],"dc":dir["dc"]});
    }
    if piece.kind == "windmill" {
        view["windmillMode"] = json!(if p.get("windmillMode").and_then(Value::as_str)
            == Some("rook")
        {
            "rook"
        } else {
            "bishop"
        });
    }
    view
}

pub(crate) fn visible_type(state: &GameState, piece: &Piece, viewer: Color) -> String {
    let hallucinated =
        nested(state.extra.get("hallucination"), viewer.as_str()).is_some_and(|entry| {
            entry.get("color").and_then(Value::as_str) == Some(piece.color.as_str())
                && number(entry.get("remaining")).unwrap_or(0.0) > 0.0
        });
    let visual = if hallucinated
        && !matches!(
            piece.kind.as_str(),
            "wall" | "football" | "blackHole" | "monster"
        ) {
        "queen"
    } else if piece.kind == "windmill" {
        if piece.extra.get("windmillMode").and_then(Value::as_str) == Some("rook") {
            "windmillRook"
        } else {
            "windmillBishop"
        }
    } else if piece.kind == "log" && truth(piece.extra.get("logDir")) {
        "logRolling"
    } else {
        &piece.kind
    };
    let mut normalized = String::new();
    let mut dash = false;
    for ch in visual.chars() {
        if dash && ch.is_ascii_lowercase() {
            normalized.push(ch.to_ascii_uppercase());
            dash = false;
        } else {
            if dash {
                normalized.push('-');
                dash = false;
            }
            if ch == '-' {
                dash = true;
            } else {
                normalized.push(ch);
            }
        }
    }
    if dash {
        normalized.push('-');
    }
    normalized
}

fn cells(value: Option<&Value>) -> Vec<Square> {
    array(value).filter_map(square).collect()
}
fn contains(value: Option<&Value>, at: Square) -> bool {
    array(value).any(|v| square(v) == Some(at))
}
fn strict_square(value: &Value) -> Option<Square> {
    value
        .get("row")
        .and_then(Value::as_f64)
        .filter(|r| r.fract() == 0.0 && (0.0..8.0).contains(r))
        .zip(
            value
                .get("col")
                .and_then(Value::as_f64)
                .filter(|c| c.fract() == 0.0 && (0.0..8.0).contains(c)),
        )
        .map(|(r, c)| Square {
            row: r as u8,
            col: c as u8,
        })
}
fn crown_ground(entry: &Value) -> Option<Square> {
    if truth(entry.get("removed")) {
        None
    } else if entry == &json!(true) {
        Some(Square { row: 3, col: 3 })
    } else {
        entry.get("ground").and_then(strict_square)
    }
}
fn collapse_warning(state: &GameState, at: Square) -> bool {
    let periodic = state.extra.get("periodicCollapse");
    let shared = f64::from(state.turns_taken.white.min(state.turns_taken.black));
    let next = number(nested(periodic, "nextAt"))
        .filter(|n| *n != 0.0)
        .unwrap_or((shared / 20.0).floor() * 20.0 + 20.0)
        .floor()
        .max(1.0);
    if !(truth(state.extra.get("collapsePending"))
        || truth(nested(periodic, "enabled")) && shared == next - 1.0)
    {
        return false;
    }
    let depth = number(state.extra.get("collapseDepth"))
        .filter(|n| *n != 0.0)
        .unwrap_or(if truth(state.extra.get("collapsed")) {
            1.0
        } else {
            0.0
        })
        .floor()
        .clamp(0.0, 4.0) as u8;
    depth < 4
        && at.row >= depth
        && at.row < 8 - depth
        && at.col >= depth
        && at.col < 8 - depth
        && (at.row == depth || at.row == 7 - depth || at.col == depth || at.col == 7 - depth)
}
fn revolving_rotations(state: &GameState, at: Square) -> Vec<u16> {
    let ring = [
        (-1, -1),
        (-1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
        (1, 0),
        (1, -1),
        (0, -1),
    ];
    let mut rotations = Vec::new();
    for row in at.row.saturating_sub(1)..=at.row.saturating_add(1).min(7) {
        for col in at.col.saturating_sub(1)..=at.col.saturating_add(1).min(7) {
            let center = Square { row, col };
            if row == 0
                || row == 7
                || col == 0
                || col == 7
                || !state
                    .at(center)
                    .is_some_and(|p| matches!(p.kind.as_str(), "revolvingDoor" | "revolving-door"))
            {
                continue;
            }
            let index = ring
                .iter()
                .position(|&(dr, dc)| center.offset(dr, dc) == Some(at));
            if let Some(index) = index {
                let to = center
                    .offset(ring[(index + 1) % 8].0, ring[(index + 1) % 8].1)
                    .expect("interior ring");
                let rotation = ((f64::from(to.row) - f64::from(at.row))
                    .atan2(f64::from(to.col) - f64::from(at.col))
                    .to_degrees()
                    + 360.0)
                    % 360.0;
                let rotation = rotation.round() as u16;
                if !rotations.contains(&rotation) {
                    rotations.push(rotation);
                }
            }
        }
    }
    rotations
}

/// Source renderBoard, aura appenders, quantum glyphs and chain overlays.
/// Each marker keeps its own visibility gate; a hidden occupant does not hide
/// public terrain or the source's deliberately unfiltered Siren aura.
pub(crate) fn board_surface(state: &GameState, viewer: Color) -> Value {
    let mut marks = Vec::new();
    let mut relationships = Vec::new();
    let mut overlays = Vec::new();
    let extra = &state.extra;
    let black = cells(extra.get("blackHole"));
    let bombs = cells(extra.get("ruleBombs"));
    let portal = extra.get("portalRule");
    let portals = if portal == Some(&json!(true)) || truth(nested(portal, "enabled")) {
        let custom = cells(nested(portal, "cells"));
        if custom.len() == 2 && custom[0] != custom[1] {
            custom
        } else {
            vec![Square { row: 5, col: 2 }, Square { row: 2, col: 5 }]
        }
    } else {
        Vec::new()
    };
    let platform = extra.get("platformRule");
    let platforms = if truth(nested(platform, "enabled")) {
        let list = cells(nested(platform, "cells"));
        if list.is_empty() {
            nested(platform, "cell")
                .and_then(square)
                .into_iter()
                .collect()
        } else {
            list
        }
    } else {
        Vec::new()
    };
    let trail = extra.get("accelerationTrail");
    let trails = if nested(trail, "hiddenFrom").and_then(Value::as_str) != Some(viewer.as_str()) {
        cells(nested(trail, "cells"))
    } else {
        Vec::new()
    };
    let mut scarecrows = Vec::new();
    for entry in array(extra.get("pendingScarecrows")) {
        let at = if truth(entry.get("pieceId")) {
            entry
                .get("pieceId")
                .and_then(Value::as_str)
                .and_then(|id| first_piece(state, id))
                .map(|(s, _)| s)
        } else {
            square(entry)
        };
        if let Some(at) = at {
            scarecrows.push((at, entry));
        }
    }
    let crowns = crown_entries(extra.get("crownRule"));
    let mut add = |kind: &str, at: Square, details: Value| {
        let mut mark = json!({"kind":kind,"square":at});
        if let Some(details) = details.as_object() {
            mark.as_object_mut()
                .expect("mark object")
                .extend(details.clone());
        }
        marks.push(mark);
    };
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            let occupant = state.at(at);
            // Fog requires campaign/Mad-AI UI configuration, outside the frozen
            // normal/chaos/grand GameConfig. Those configurations are rejected at
            // the supported rule boundary; ordinary frozen games have no fog set.
            let hidden = occupant.is_some_and(|p| !state.piece_visible(p, at, viewer));
            for (kind, list) in [
                ("blackHole", &black),
                ("ruleBomb", &bombs),
                ("portal", &portals),
                ("platform", &platforms),
                ("accelerationTrail", &trails),
            ] {
                if list.contains(&at) {
                    add(kind, at, json!({}));
                }
            }
            if array(extra.get("palaces")).any(|palace| contains(palace.get("cells"), at)) {
                add("palace", at, json!({}));
            }
            if crowns.iter().any(|entry| crown_ground(entry) == Some(at)) {
                add("crownGround", at, json!({}));
            }
            let hazard =
                array(extra.get("delayedHazards")).find(|entry| contains(entry.get("cells"), at));
            let impact =
                array(extra.get("wizardImpact")).find(|entry| strict_square(entry) == Some(at));
            if let Some(hazard) = impact.or(hazard) {
                add(
                    if hazard.get("type").and_then(Value::as_str) == Some("lightning") {
                        "lightning"
                    } else {
                        "meteor"
                    },
                    at,
                    json!({}),
                );
            } else if collapse_warning(state, at) {
                add("meteor", at, json!({}));
            }
            if truth(extra.get("conveyorRule")) {
                let direction = if row == 0 && col < 7 {
                    Some("right")
                } else if col == 7 && row < 7 {
                    Some("down")
                } else if row == 7 && col > 0 {
                    Some("left")
                } else if col == 0 && row > 0 {
                    Some("up")
                } else {
                    None
                };
                if let Some(direction) = direction {
                    add("conveyor", at, json!({"direction":direction}));
                }
            }
            if occupant.is_some_and(|p| crate::movement::encouraged(state, p)) {
                add("encouraged", at, json!({}));
            }
            if occupant.is_some_and(|p| p.kind == "pawn" && state.flag("resolveReady", p.color)) {
                add("resolveReady", at, json!({}));
            }
            if occupant.is_some_and(|p| {
                !hidden
                    && array(nested(extra.get("winterKingdom"), "previewIds"))
                        .any(|id| id.as_str() == Some(&p.id))
            }) {
                add("winterForecast", at, json!({}));
            }
            if nested(platform, "previewCell").and_then(strict_square) == Some(at) {
                add("platformForecast", at, json!({}));
            }
            if let Some((index, _)) = array(extra.get("gomokuVictoryCells"))
                .enumerate()
                .filter(|(_, cell)| strict_square(cell) == Some(at))
                .last()
            {
                add("gomokuVictory", at, json!({"index":index}));
            }
            for owner in [Color::White, Color::Black] {
                if nested(nested(extra.get("captureTheFlag"), "flags"), owner.as_str())
                    .and_then(strict_square)
                    == Some(at)
                {
                    add("captureFlag", at, json!({"owner":owner}));
                }
            }
            if let Some((_, entry)) = scarecrows.iter().rev().find(|(s, _)| *s == at) {
                let bound = truth(entry.get("pieceId"));
                if !bound || occupant.is_some_and(|p| p.flag("scarecrowReserved")) {
                    add("scarecrowReserved", at, json!({}));
                }
                if if bound {
                    occupant.is_some() && !hidden
                } else {
                    occupant.is_none() || hidden
                } {
                    let remaining = if entry.get("remainingOwnTurns").is_some() {
                        number(entry.get("remainingOwnTurns"))
                            .filter(|n| *n != 0.0)
                            .unwrap_or(1.0)
                            .max(1.0)
                    } else {
                        (number(entry.get("remainingHalfTurns"))
                            .filter(|n| *n != 0.0)
                            .unwrap_or(1.0)
                            .max(1.0)
                            / 2.0)
                            .ceil()
                            .clamp(1.0, 9.0)
                    };
                    add(
                        "scarecrowPreview",
                        at,
                        json!({"owner":if entry.get("color").and_then(Value::as_str)==Some("black"){Color::Black}else{Color::White},"remaining":remaining}),
                    );
                }
            }
            if let Some(entry) = array(extra.get("pendingLobsters"))
                .filter(|entry| square(entry) == Some(at))
                .last()
            {
                add("lobsterReserved", at, json!({}));
                if occupant.is_none() {
                    add(
                        "lobsterPreview",
                        at,
                        json!({"owner":if entry.get("color").and_then(Value::as_str)==Some("black"){Color::Black}else{Color::White},"remaining":(remaining_half(entry,state.move_count).max(1.0)/2.0).ceil().clamp(1.0,9.0)}),
                    );
                }
            }
            if array(extra.get("pendingPortals")).any(|entry| contains(entry.get("cells"), at)) {
                add("portalReserved", at, json!({}));
                if occupant.is_none() {
                    add("portalPreview", at, json!({}));
                }
            }
            if let Some(entry) = array(extra.get("pendingOtherworld"))
                .filter(|entry| square(entry) == Some(at))
                .last()
            {
                add(
                    "otherworldOrigin",
                    at,
                    json!({"remaining":(remaining_half(entry,state.move_count)/2.0).ceil().max(0.0)}),
                );
            }
            for owner in [Color::White, Color::Black] {
                let target_row = if owner == Color::White { 4 } else { 3 };
                if at
                    == (Square {
                        row: target_row,
                        col: 3,
                    })
                    && state.flag("d4", owner)
                {
                    add("d4Forbidden", at, json!({"owner":owner}));
                }
                if at
                    == (Square {
                        row: target_row,
                        col: 4,
                    })
                    && state.flag("e4", owner)
                {
                    add("e4Destination", at, json!({"owner":owner}));
                }
            }
            if array(extra.get("tabooPending"))
                .any(|entry| entry.get("square").and_then(strict_square) == Some(at))
            {
                add("taboo", at, json!({}));
            }
            for rotation in revolving_rotations(state, at) {
                add("revolvingDoor", at, json!({"rotation":rotation}));
            }
        }
    }
    let mut quantum_seen = std::collections::BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            let Some(piece) = state.at(at) else {
                continue;
            };
            let visible = state.piece_visible(piece, at, viewer);
            for kind in [
                "knightmaster",
                "clockwork",
                "paladin",
                "idol",
                "siren",
                "reaper",
            ] {
                if ability(piece) != kind || kind != "siren" && !visible {
                    continue;
                }
                add(&format!("{kind}Aura"), at, json!({}));
                for (dr, dc) in crate::movement::KING {
                    if let Some(cell) = at.offset(*dr, *dc) {
                        add(&format!("{kind}Aura"), cell, json!({}));
                    }
                }
            }
            if piece.kind == "darkWizard" && truth(piece.extra.get("darkMagicCircle")) && visible {
                let center = piece.extra.get("darkMagicCircle");
                let row = number(nested(center, "centerRow"))
                    .filter(|n| n.fract() == 0.0)
                    .unwrap_or(f64::from(row)) as i16;
                let col = number(nested(center, "centerCol"))
                    .filter(|n| n.fract() == 0.0)
                    .unwrap_or(f64::from(col)) as i16;
                for r in row - 1..=row + 1 {
                    for c in col - 1..=col + 1 {
                        if (0..8).contains(&r) && (0..8).contains(&c) {
                            add(
                                "darkMagicDomain",
                                Square {
                                    row: r as u8,
                                    col: c as u8,
                                },
                                json!({}),
                            );
                        }
                    }
                }
            }
            if !truth(piece.extra.get("quantum"))
                || piece.id.is_empty()
                || !visible
                || !quantum_seen.insert(piece.id.clone())
            {
                continue;
            }
            if let Some(anchor) = piece.extra.get("quantum").and_then(square) {
                let mut cells = vec![anchor];
                if piece.is_large() {
                    cells = [
                        Some(anchor),
                        anchor.offset(0, 1),
                        anchor.offset(1, 0),
                        anchor.offset(1, 1),
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    if cells.len() != 4 {
                        continue;
                    }
                }
                let mut content = piece_view(
                    state,
                    piece,
                    at,
                    viewer,
                    &visible_type(state, piece, viewer),
                );
                // The ghost renderer calls createPieceElement without a physical
                // square, so origin-dependent badges do not appear on the ghost.
                content["status"]
                    .as_object_mut()
                    .expect("piece status")
                    .shift_remove("undeadResurrectionRemaining");
                content["status"]
                    .as_object_mut()
                    .expect("piece status")
                    .shift_remove("locustReady");
                content["status"]
                    .as_object_mut()
                    .expect("piece status")
                    .shift_remove("sirenWarning");
                overlays.push(json!({"kind":"quantum","cells":cells,"piece":content}));
            }
        }
    }
    let mut pairs = std::collections::BTreeSet::new();
    for bond in array(extra.get("chainBonds")).take(64) {
        let Some(a) = bond
            .get("aId")
            .and_then(Value::as_str)
            .map(|s| s.chars().take(160).collect::<String>())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(b) = bond
            .get("bId")
            .and_then(Value::as_str)
            .map(|s| s.chars().take(160).collect::<String>())
            .filter(|s| !s.is_empty() && s != &a)
        else {
            continue;
        };
        let pair = if a < b {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };
        if !pairs.insert(pair) {
            continue;
        }
        let Some((from, first)) = first_piece(state, &a) else {
            continue;
        };
        let Some((to, second)) = first_piece(state, &b) else {
            continue;
        };
        if from == to
            || !state.piece_visible(first, from, viewer)
            || !state.piece_visible(second, to, viewer)
        {
            continue;
        }
        let r = from.row.abs_diff(to.row);
        let c = from.col.abs_diff(to.col);
        let major = r.max(c);
        let minor = r.min(c);
        let length = match (major, minor) {
            (1, 0) => "one",
            (1, 1) => "sqrt2",
            (2, 0) => "two",
            (2, 1) => "sqrt5",
            (2, 2) => "two-sqrt2",
            _ => "offset",
        };
        relationships.push(json!({"kind":"chain","owner":if bond.get("by").and_then(Value::as_str)==Some("black"){Color::Black}else{Color::White},"from":from,"to":to,"length":length}));
    }
    json!({"boardMarks":marks,"relationships":relationships,"overlays":overlays})
}

/// The three source revelation helpers and their roulette/potion ID sets are
/// identical in the pinned v6 and v7 clients. Box-card membership still uses
/// the selected ruleset catalog so a future catalog change cannot leak an ID.
pub(crate) fn card_revelation_for_ruleset(
    card: &CardSlot,
    ruleset_id: &str,
) -> Result<Option<Value>> {
    let definitions = crate::draft::definitions_for_ruleset(ruleset_id)?;
    Ok(card_revelation_with_definitions(card, definitions))
}

fn card_revelation_with_definitions(
    card: &CardSlot,
    definitions: &crate::draft::Definitions,
) -> Option<Value> {
    let mut revealed = Fields::new();
    let box_id = card
        .extra
        .get("boxRevealedCardId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let roulette =
        card.effect == "randomRoulette" || card.effect == "blackBox" && box_id == "random-roulette";
    if roulette
        && let Some(kind) = card
            .extra
            .get("randomRouletteResultType")
            .and_then(Value::as_str)
        && [
            "pawn",
            "knight",
            "bishop",
            "rook",
            "queen",
            "colossus",
            "bigRook",
            "protestant",
            "herald",
            "cannon",
            "fanatic",
            "primeMinister",
            "eagle",
            "amazon",
            "cardinal",
            "pegasus",
            "jester",
            "camel",
            "log",
            "hook",
            "grasshopper",
            "dragon",
            "man",
            "assassin",
            "knightmaster",
            "standardBearer",
            "recruiter",
            "squire",
            "checker",
            "checkerKing",
            "wizard",
            "alfil",
            "bat",
            "guard",
            "reaper",
            "idol",
            "babyBear",
            "lobster",
            "missionary",
            "bear",
            "windmill",
            "siegeRam",
            "magicGirl",
            "berserker",
            "slime",
            "siren",
            "trickster",
            "undead",
            "campfire",
            "hedgehog",
            "princess",
            "thief",
            "brutus",
            "clockwork",
            "parrot",
            "paladin",
            "octopus",
            "grappler",
            "revolvingDoor",
            "donQuixote",
            "medium",
        ]
        .contains(&kind)
    {
        revealed.insert("rouletteType".into(), json!(kind));
    }
    let potion = card.effect == "suspiciousPotion"
        || card.effect == "blackBox" && box_id == "suspicious-potion";
    if potion
        && let Some(id) = card
            .extra
            .get("suspiciousPotionResultId")
            .and_then(Value::as_str)
        && [
            "sacrificeProtection",
            "lastResistance",
            "coronationProtection",
            "shield",
            "evasion",
            "parry",
            "basicTraining",
            "stealth",
            "loyalist",
            "ghost",
            "chameleon",
            "submerge",
            "freeze",
            "chimera",
            "witchTrial",
            "stake",
            "callingCard",
            "emptyLunchbox",
            "poisonStun",
            "disarm",
            "mannerNoCapture",
            "saturationNoCapture",
            "explosive",
            "poisonedPawn",
            "queensGambitProtection",
            "trojanHorse",
            "severance",
            "inertia",
            "outpostProtection",
            "nullification",
            "recurrence",
            "grapplerBound",
        ]
        .contains(&id)
    {
        revealed.insert("potionEffect".into(), json!(id));
    }
    if !box_id.is_empty()
        && (card.effect != "blackBox" || card.used)
        && definitions
            .definitions
            .iter()
            .any(|c| c.get("id").and_then(Value::as_str) == Some(box_id))
    {
        revealed.insert("boxCardId".into(), json!(box_id));
    }
    (!revealed.is_empty()).then_some(Value::Object(revealed))
}

#[cfg(test)]
#[path = "observation_version_tests.rs"]
mod observation_version_tests;
