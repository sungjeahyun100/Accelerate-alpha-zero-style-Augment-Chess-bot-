//! Turn completion and the site's long-game adjudication. All counts belong to
//! the immutable rule state; UI timers and replay rendering are not executed.
use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) fn star_total(state: &GameState, color: Color) -> f64 {
    state
        .deck_slots
        .get(color)
        .iter()
        .filter(|card| !card.vacant)
        .map(CardSlot::star_value)
        .sum()
}

pub(crate) fn record_position(state: &mut GameState) -> Result<u64> {
    let mut seen = BTreeSet::new();
    let mut pieces = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let Some(piece) = state.at(Square { row, col }) else {
                continue;
            };
            if piece.kind == "wall" || !seen.insert(piece.id.clone()) {
                continue;
            }
            let attribute = |name: &str| {
                piece
                    .extra
                    .get(name)
                    .filter(|v| !v.is_null())
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string())
                    })
                    .unwrap_or_default()
            };
            pieces.push(format!(
                "{row},{col}:{}:{}:{}:{}:{}",
                piece.color.as_str(),
                piece.kind,
                attribute("hp"),
                attribute("ammo"),
                u8::from(piece.flag("frozen") || piece.number("frozen") > 0)
            ));
        }
    }
    pieces.sort();
    let salt = state
        .extra
        .get("repetitionSalt")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let key = format!("{}|{salt}|{}", state.turn.as_str(), pieces.join(";"));
    let table = state
        .extra
        .entry("positionCounts")
        .or_insert_with(|| json!({"__simType":"Map","entries":[]}));
    let entries = table
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            EngineError::InvalidState("positionCounts must use the Map snapshot encoding".into())
        })?;
    if let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.get(0).and_then(Value::as_str) == Some(&key))
    {
        let count = entry
            .get(1)
            .and_then(Value::as_u64)
            .ok_or_else(|| EngineError::InvalidState("invalid repetition count".into()))?
            .checked_add(1)
            .ok_or_else(|| EngineError::InvalidState("repetition count overflow".into()))?;
        entry[1] = json!(count);
        Ok(count)
    } else {
        entries.push(json!([key, 1]));
        Ok(1)
    }
}

fn shotgun_color(state: &GameState) -> Option<Color> {
    [Color::White, Color::Black].into_iter().find(|&color| {
        state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|p| p.color == color && p.kind == "shotgunKing")
            || state
                .deck_slots
                .get(color)
                .iter()
                .any(|c| !c.vacant && c.id == "shotgun-king")
    })
}
pub(crate) fn resolve_stars(state: &mut GameState) {
    let winner = shotgun_color(state).map(Color::opponent).or_else(|| {
        let white = star_total(state, Color::White);
        let black = star_total(state, Color::Black);
        if white < black {
            Some(Color::White)
        } else if black < white {
            Some(Color::Black)
        } else {
            None
        }
    });
    state.mode = "gameover".into();
    state.winner = winner.map(|color| color.as_str().into());
}

pub(crate) fn mark_progress(state: &mut GameState) {
    if state.result().is_some() {
        return;
    }
    if let Some(dm) = state
        .extra
        .get_mut("deathmatch")
        .and_then(Value::as_object_mut)
        && dm.get("active").and_then(Value::as_bool) == Some(true)
    {
        dm.insert("halfTurnsSinceProgress".into(), json!(0));
        dm.insert("progressThisTurn".into(), json!(true));
        dm.insert("warningKey".into(), json!(""));
    }
}
pub(crate) fn tick_deathmatch(state: &mut GameState, moving_color: Color) -> Result<bool> {
    if moving_color != Color::Black {
        return Ok(false);
    }
    let Some(dm) = state
        .extra
        .get_mut("deathmatch")
        .and_then(Value::as_object_mut)
    else {
        return Ok(false);
    };
    if dm.get("active").and_then(Value::as_bool) != Some(true) {
        return Ok(false);
    }
    let interval = dm
        .get("intervalHalfTurns")
        .and_then(Value::as_u64)
        .ok_or_else(|| EngineError::InvalidState("deathmatch interval missing".into()))?;
    if interval == 0 {
        return Err(EngineError::InvalidState(
            "deathmatch interval must be positive".into(),
        ));
    }
    dm.insert("warningKey".into(), json!(""));
    if dm.get("progressThisTurn").and_then(Value::as_bool) == Some(true) {
        dm.insert("halfTurnsSinceProgress".into(), json!(0));
        dm.insert("progressThisTurn".into(), json!(false));
        return Ok(false);
    }
    let count = dm
        .get("halfTurnsSinceProgress")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_add(2)
        .min(interval);
    dm.insert("halfTurnsSinceProgress".into(), json!(count));
    if count >= interval {
        resolve_stars(state);
        Ok(true)
    } else {
        Ok(false)
    }
}

pub(crate) fn check_termination(state: &mut GameState) -> Result<bool> {
    if state.result().is_some() {
        return Ok(true);
    }
    if shotgun_color(state).is_none() && record_position(state)? >= 3 {
        resolve_stars(state);
        return Ok(true);
    }
    let active = state
        .extra
        .get("deathmatch")
        .and_then(|dm| dm.get("active"))
        .and_then(Value::as_bool)
        == Some(true);
    let shared = state.turns_taken.white.min(state.turns_taken.black);
    let limit = state
        .extra
        .get("starWinLimit")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .unwrap_or(45);
    if !active && u64::from(shared) >= limit {
        if state
            .extra
            .get("deathmatchEnabled")
            .and_then(Value::as_bool)
            == Some(true)
        {
            let turns = state
                .extra
                .get("deathmatchLimitTurns")
                .and_then(Value::as_u64)
                .filter(|value| *value > 0)
                .unwrap_or(10);
            let interval = turns
                .checked_mul(2)
                .ok_or_else(|| EngineError::InvalidState("deathmatch interval overflow".into()))?;
            state.extra.insert("deathmatch".into(),json!({"active":true,"startedAtTurn":shared,"halfTurnsSinceProgress":0,"intervalHalfTurns":interval,"progressThisTurn":false,"warningKey":""}));
            state
                .extra
                .insert("endPhaseStartMove".into(), json!(shared));
        } else {
            resolve_stars(state);
            return Ok(true);
        }
    }
    Ok(false)
}
pub(crate) fn note_card_event(state: &mut GameState) -> Result<()> {
    let salt = state
        .extra
        .get("repetitionSalt")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| EngineError::InvalidState("repetition salt overflow".into()))?;
    state.extra.insert("repetitionSalt".into(), json!(salt));
    state.extra.insert(
        "positionCounts".into(),
        json!({"__simType":"Map","entries":[]}),
    );
    Ok(())
}
