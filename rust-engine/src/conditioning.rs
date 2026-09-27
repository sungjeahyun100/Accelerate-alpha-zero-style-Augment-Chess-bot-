//! Conditional initial chance and public identity binding. This module accepts
//! public frames only; particle filtering, resampling and tree search are callers'
//! responsibilities. No actual private position, seed or future RNG is an input.
use crate::*;
use serde_json::Value;
use std::collections::BTreeMap;

fn same_content<T: serde::Serialize>(left: &T, right: &T) -> Result<bool> {
    let canonical = |value| {
        serde_jcs::to_vec(value).map_err(|error| {
            EngineError::Serialization(format!("public canonicalization: {error}"))
        })
    };
    Ok(canonical(left)? == canonical(right)?)
}

fn checked_observation(value: Value) -> Result<Observation> {
    crate::state::validate_json_value(&value, 0)?;
    let object = value
        .as_object()
        .ok_or_else(|| EngineError::InvalidState("public observation must be an object".into()))?;
    let fields = [
        "protocolVersion",
        "viewer",
        "turn",
        "board",
        "ownCards",
        "opponentHandCount",
        "publicState",
        "history",
        "informationStateKey",
    ];
    if object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field)) {
        return Err(EngineError::InvalidState(
            "public observation has unknown or missing fields".into(),
        ));
    }
    let mut observation: Observation =
        serde_json::from_value(value).map_err(EngineError::serialization)?;
    if observation.protocol_version != crate::state::observation_protocol()
        || observation
            .public_state
            .get("projectionVersion")
            .and_then(Value::as_str)
            != Some(crate::state::observation_projection())
        || observation
            .public_state
            .get("observationPolicyHash")
            .and_then(Value::as_str)
            != Some(crate::state::observation_policy_hash())
        || observation.board.len() != 8
        || observation.board.iter().any(|row| row.len() != 8)
    {
        return Err(EngineError::InvalidState(
            "invalid public observation protocol or board".into(),
        ));
    }
    let supplied = observation.information_state_key.clone();
    observation.refresh_key();
    if observation.information_state_key != supplied {
        return Err(EngineError::InvalidState(
            "public observation informationStateKey mismatch".into(),
        ));
    }
    Ok(observation)
}

pub(crate) fn sample_initial(config: GameConfig, expected: Value, seed: u32) -> Result<Position> {
    let expected = checked_observation(expected)?;
    if !expected.history.is_empty()
        || !expected.own_cards.is_empty()
        || expected.opponent_hand_count != 0
        || expected
            .public_state
            .get("revealedOpponentCards")
            .and_then(Value::as_array)
            .is_none_or(|cards| !cards.is_empty())
    {
        return Err(EngineError::InvalidState(
            "conditioning requires the initial public frame, before any card acquisition".into(),
        ));
    }
    let sampled = Position::new_game(config.clone(), u64::from(seed))?;
    let mut state = sampled.state().clone();
    if !config.draft_delete {
        if config.game_style == "grand" {
            let draft = expected.public_state.get("draft").ok_or_else(|| {
                EngineError::InvalidState("initial grand pool must be public".into())
            })?;
            let choices = draft
                .get("choices")
                .and_then(Value::as_array)
                .ok_or_else(|| EngineError::InvalidState("public grand pool is missing".into()))?;
            crate::draft::condition_grand_initial_choices(&mut state, choices)?;
        } else {
            if let Some(draft) = expected.public_state.get("draft") {
                if draft.get("color").and_then(Value::as_str) != Some(expected.viewer.as_str()) {
                    return Err(EngineError::InvalidState(
                        "initial private opposing draft may not be supplied as public".into(),
                    ));
                }
                let choices = draft
                    .get("choices")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        EngineError::InvalidState("own initial draft choices missing".into())
                    })?;
                crate::draft::condition_initial_offer(&mut state, choices)?;
            } else if expected.viewer == state.decision_actor() {
                return Err(EngineError::InvalidState(
                    "own initial draft phase must be public".into(),
                ));
            }
        }
    }
    let conditioned = sampled.with_state(state)?;
    let actual = conditioned.try_observe(expected.viewer)?;
    if !same_content(&actual, &expected)? {
        return Err(EngineError::ConditioningMismatch(
            "initial public frame is impossible under the supplied source configuration".into(),
        ));
    }
    Ok(conditioned)
}

fn card_identity_map(
    current: &[Value],
    expected: &[Value],
    map: &mut BTreeMap<String, String>,
) -> Result<()> {
    if current.len() != expected.len() {
        return Err(EngineError::ConditioningMismatch(
            "public card counts changed during identity binding".into(),
        ));
    }
    for (current, expected) in current.iter().zip(expected) {
        let old = current
            .get("instanceId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                EngineError::InvalidState("sampled public card identity missing".into())
            })?;
        let new = expected
            .get("instanceId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or_else(|| {
                EngineError::InvalidState(
                    "conditioned public card identity missing or oversized".into(),
                )
            })?;
        let mut left = current.clone();
        let mut right = expected.clone();
        left.as_object_mut()
            .ok_or(EngineError::IllegalAction)?
            .remove("instanceId");
        right
            .as_object_mut()
            .ok_or(EngineError::IllegalAction)?
            .remove("instanceId");
        if !same_content(&left, &right)? {
            return Err(EngineError::ConditioningMismatch(
                "identity binding cannot alter card type, phase, status, slot or other semantics"
                    .into(),
            ));
        }
        if map.get(old).is_some_and(|prior| prior != new)
            || map.iter().any(|(key, value)| key != old && value == new)
        {
            return Err(EngineError::InvalidState(
                "public identity binding must be one-to-one".into(),
            ));
        }
        map.insert(old.into(), new.into());
    }
    Ok(())
}
fn remap(value: &mut Value, map: &BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            if let Some(replacement) = map.get(text) {
                *text = replacement.clone();
            }
        }
        Value::Array(items) => {
            for item in items {
                remap(item, map);
            }
        }
        Value::Object(items) => {
            for item in items.values_mut() {
                remap(item, map);
            }
        }
        _ => {}
    }
}
pub(crate) fn condition_identities(position: &Position, expected: Value) -> Result<Position> {
    let expected = checked_observation(expected)?;
    let current = position.try_observe(expected.viewer)?;
    let mut map = BTreeMap::new();
    card_identity_map(&current.own_cards, &expected.own_cards, &mut map)?;
    let array = |state: &Fields, field: &str| -> Result<Vec<Value>> {
        state
            .get(field)
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| EngineError::InvalidState(format!("public {field} missing")))
    };
    card_identity_map(
        &array(&current.public_state, "revealedOpponentCards")?,
        &array(&expected.public_state, "revealedOpponentCards")?,
        &mut map,
    )?;
    match (
        current.public_state.get("draft"),
        expected.public_state.get("draft"),
    ) {
        (Some(left), Some(right)) => {
            let choices = |draft: &Value| -> Result<Vec<Value>> {
                draft
                    .get("choices")
                    .and_then(Value::as_array)
                    .cloned()
                    .ok_or_else(|| EngineError::InvalidState("public draft choices missing".into()))
            };
            card_identity_map(&choices(left)?, &choices(right)?, &mut map)?;
        }
        (None, None) => {}
        _ => {
            return Err(EngineError::ConditioningMismatch(
                "identity binding cannot change the public draft phase".into(),
            ));
        }
    }
    let mut raw = serde_json::to_value(position.state()).map_err(EngineError::serialization)?;
    // The metadata RNG is never relabeled or sampled here. Only existing opaque
    // identity strings and references to them are changed together.
    let rng = raw
        .as_object_mut()
        .expect("state object")
        .remove("rng")
        .expect("RNG metadata");
    remap(&mut raw, &map);
    raw["rng"] = rng;
    let state: GameState = serde_json::from_value(raw).map_err(EngineError::serialization)?;
    let conditioned = position.with_state(state)?;
    if !same_content(&conditioned.try_observe(expected.viewer)?, &expected)? {
        return Err(EngineError::ConditioningMismatch(
            "public frame differs beyond opaque identities".into(),
        ));
    }
    Ok(conditioned)
}

/// A suffix must be a possible Number.toString(36).slice(2) output. This check
/// never recovers or installs the actual generator state behind a public ID.
pub(crate) fn validate_initial_identity(id: &str, identity: &str) -> Result<()> {
    let prefix = format!("{id}-");
    let suffix = identity.strip_prefix(&prefix).ok_or_else(|| {
        EngineError::InvalidState("initial card identity has a different definition prefix".into())
    })?;
    if suffix.len() > 2200
        || !suffix
            .bytes()
            .all(|digit| digit.is_ascii_digit() || digit.is_ascii_lowercase())
    {
        return Err(EngineError::InvalidState(
            "invalid initial random identity suffix".into(),
        ));
    }
    let mut value = 0.0_f64;
    for byte in suffix.bytes().rev() {
        let digit = if byte.is_ascii_digit() {
            byte - b'0'
        } else {
            byte - b'a' + 10
        };
        value = (value + f64::from(digit)) / 36.0;
    }
    let approximate = (value * 4_294_967_296.0).round() / 4_294_967_296.0;
    let mut candidates = vec![value, approximate];
    for offset in 1..=4 {
        if value.to_bits() >= offset {
            candidates.push(f64::from_bits(value.to_bits() - offset));
        }
        candidates.push(f64::from_bits(value.to_bits() + offset));
    }
    if candidates.into_iter().any(|candidate| {
        (0.0..1.0).contains(&candidate)
            && crate::draft::random_suffix(candidate).is_ok_and(|encoded| encoded == suffix)
    }) {
        Ok(())
    } else {
        Err(EngineError::InvalidState(
            "initial opaque identity is not a source random suffix".into(),
        ))
    }
}
