//! Turn completion and the site's long-game adjudication. All counts belong to
//! the immutable rule state; UI timers and replay rendering are not executed.
use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) fn end_game(state: &mut GameState, winner: Option<Color>, reason: &str) -> Result<()> {
    pause_clock(state)?;
    state.mode = "gameover".into();
    state.winner = winner.map(|color| color.as_str().into());
    if state
        .extra
        .get("replayEndedAt")
        .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
    {
        let catalog: Value =
            serde_json::from_str(include_str!("../../bridge/catalog/site-20260927.json"))
                .expect("adopted catalog");
        state.extra.insert(
            "replayEndedAt".into(),
            catalog["source"]["frozenAt"].clone(),
        );
    }
    state.extra.insert("replayEndReason".into(), json!(reason));
    for field in [
        "selected",
        "targeting",
        "ruleTicketChoice",
        "jokerChoice",
        "barricadeDirectionChoice",
        "barricadePreview",
        "drawOffer",
    ] {
        state.extra.insert(field.into(), Value::Null);
    }
    state.extra.insert("legalMoves".into(), json!([]));
    crate::replay::add_log(
        state,
        format!(
            "{}: {reason}",
            winner
                .map(|color| format!("{} 승리", crate::replay::label(color)))
                .unwrap_or_else(|| "무승부".into())
        ),
    )?;
    state.gameover_replay_pending = true;
    Ok(())
}

pub(crate) fn check_democracy_defeat(
    state: &mut GameState,
    color: Color,
    winner: Color,
    cause: &str,
) -> Result<bool> {
    if state.flag("democracy", color)
        && state.flag("zugzwang", color)
        && !state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.color == color && state.royal_identity(piece))
    {
        state.set_flag("zugzwang", color, false);
    }
    if state.mode == "gameover" || !state.flag("democracy", color) {
        return Ok(false);
    }
    let recurring = state
        .extra
        .get("pendingRecurrences")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|entry| entry["piece"]["color"] == color.as_str() && entry["piece"]["type"] == "pawn");
    if recurring
        || state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.color == color && piece.kind == "pawn")
    {
        return Ok(false);
    }
    end_game(
        state,
        Some(winner),
        &format!("{}의 {cause}", crate::replay::label(color)),
    )?;
    Ok(true)
}

/// The adopted local oracle profile advances rules at the catalog's frozen
/// logical time. Clock state remains part of the rules snapshot; a wall-clock
/// scheduler must supply elapsed time separately rather than enter this kernel.
fn clock_remaining(state: &GameState, color: Color) -> Result<Option<f64>> {
    let Some(clock) = state.extra.get("clock") else {
        return Ok(None);
    };
    if clock["enabled"] != true {
        return Ok(None);
    }
    let key = format!("{}Ms", color.as_str());
    let stored = clock[&key]
        .as_f64()
        .ok_or_else(|| EngineError::InvalidState("enabled clock has no stored time".into()))?;
    let runnable = state.mode == "play"
        && state.turn == color
        && state.extra.get("activeTrolley").is_none_or(|v| v.is_null())
        && state
            .extra
            .get("pendingPromotion")
            .is_none_or(|v| v.is_null())
        && state.extra.get("turnResolving") != Some(&json!(true));
    let remaining = if runnable
        && clock["runningColor"] == color.as_str()
        && clock["lastStartedAt"].as_f64().is_some_and(|v| v != 0.0)
    {
        stored
            - (crate::draft::frozen_timestamp()? as f64 - clock["lastStartedAt"].as_f64().unwrap())
    } else {
        stored
    };
    Ok(Some(remaining.max(0.0)))
}

pub(crate) fn pause_clock(state: &mut GameState) -> Result<()> {
    let Some(clock) = state.extra.get("clock") else {
        return Ok(());
    };
    if clock["enabled"] != true {
        return Ok(());
    }
    let running = clock["runningColor"]
        .as_str()
        .and_then(|value| match value {
            "white" => Some(Color::White),
            "black" => Some(Color::Black),
            _ => None,
        });
    let remaining = running
        .map(|color| clock_remaining(state, color))
        .transpose()?
        .flatten();
    let terminal = state.mode == "gameover";
    let clock = state.extra.get_mut("clock").expect("existing clock");
    if let (Some(color), Some(remaining)) = (running, remaining) {
        clock[format!("{}Ms", color.as_str())] = json!(remaining);
    }
    if !terminal {
        clock["runningColor"] = Value::Null;
        clock["lastStartedAt"] = Value::Null;
    }
    Ok(())
}

pub(crate) fn start_clock(state: &mut GameState) -> Result<()> {
    tick_siren_turn_start(state)?;
    if state.mode != "play"
        || state
            .extra
            .get("activeTrolley")
            .is_some_and(|v| !v.is_null())
        || state
            .extra
            .get("pendingPromotion")
            .is_some_and(|v| !v.is_null())
        || state.extra.get("turnResolving") == Some(&json!(true))
    {
        return Ok(());
    }
    let Some(clock) = state.extra.get_mut("clock") else {
        return Ok(());
    };
    if clock["enabled"] != true
        || (clock["runningColor"] == state.turn.as_str()
            && clock["lastStartedAt"]
                .as_f64()
                .is_some_and(|value| value != 0.0))
    {
        return Ok(());
    }
    clock["runningColor"] = json!(state.turn);
    clock["lastStartedAt"] = json!(crate::draft::frozen_timestamp()?);
    Ok(())
}

/// The client calls this before checking whether its clock is enabled. Empty
/// aura boards still update the boundary key and remove stale exposure entries.
/// Active conversion needs the same defection/capture callback kernel as moves.
fn tick_siren_turn_start(state: &mut GameState) -> Result<()> {
    if state.mode != "play" {
        return Ok(());
    }
    let key = format!(
        "{}:{}",
        state.turn.as_str(),
        state.turns_taken.get(state.turn)
    );
    if state
        .extra
        .get("sirenExposure")
        .and_then(|v| v.get("__turnStartKey"))
        == Some(&json!(key))
    {
        return Ok(());
    }
    if state.board.iter().flatten().flatten().any(|piece| {
        piece.ability_kind() == "siren"
            || piece.kind == "trickster"
                && piece
                    .extra
                    .get("tricksterPreviousAbilityForTurn")
                    .and_then(Value::as_str)
                    == Some("siren")
    }) {
        return Err(EngineError::UnsupportedFeature(
            "Siren turn-start conversion".into(),
        ));
    }
    state
        .extra
        .insert("sirenExposure".into(), json!({"__turnStartKey":key}));
    Ok(())
}

/// Source checkNoActionLoss probes cards before moves, even on a mobile board.
/// Its untargeted trial may consume global RNG; skipping directly to a legal
/// move would change future draws while leaving today's board identical.
pub(crate) fn check_no_action_loss(state: &mut GameState) -> Result<bool> {
    if state.mode != "play"
        || crate::observation::truth(state.extra.get("pendingPromotion"))
        || crate::observation::truth(state.extra.get("targeting"))
    {
        return Ok(false);
    }
    if state
        .extra
        .get("chainBonds")
        .and_then(Value::as_array)
        .is_some_and(|bonds| !bonds.is_empty())
    {
        return Err(EngineError::UnsupportedFeature(
            "no-action chain normalization".into(),
        ));
    }
    if crate::transition::available_card_action(state, state.turn)?
        || !crate::movement::legal_move_actions(state)?.is_empty()
    {
        return Ok(false);
    }
    if state.board.iter().flatten().flatten().any(|piece| {
        piece.color == state.turn
            && (piece.kind == "shotgunKing"
                && crate::observation::number(piece.extra.get("ammo")).unwrap_or(0.0)
                    < crate::observation::number(piece.extra.get("maxAmmo")).unwrap_or(3.0)
                || piece.ability_kind() == "wizard"
                    && crate::observation::number(piece.extra.get("mana")).unwrap_or(0.0) >= 1.0)
    }) {
        return Ok(false);
    }
    end_game(
        state,
        Some(state.turn.opponent()),
        &format!(
            "{}은 사용할 카드와 움직일 수 있는 기물이 없습니다.",
            crate::replay::label(state.turn)
        ),
    )?;
    Ok(true)
}

pub(crate) fn commit_turn_clock(state: &mut GameState, color: Color) -> Result<bool> {
    let Some(clock) = state.extra.get("clock") else {
        return Ok(true);
    };
    if clock["enabled"] != true
        || clock["runningColor"] != color.as_str()
        || !clock["lastStartedAt"]
            .as_f64()
            .is_some_and(|value| value != 0.0)
    {
        return Ok(true);
    }
    let remaining = clock_remaining(state, color)?.expect("enabled clock");
    let increment = clock["incrementMs"]
        .as_f64()
        .filter(|value| [0.0, 3000.0, 5000.0, 7000.0, 10000.0, 15000.0].contains(value))
        .unwrap_or(10000.0);
    let clock = state.extra.get_mut("clock").expect("existing clock");
    clock["lastStartedAt"] = json!(crate::draft::frozen_timestamp()?);
    clock[format!("{}Ms", color.as_str())] = json!(if remaining > 0.0 {
        remaining + increment
    } else {
        0.0
    });
    if remaining > 0.0 {
        return Ok(true);
    }
    clock["runningColor"] = Value::Null;
    clock["lastStartedAt"] = Value::Null;
    clock["timeoutLoser"] = json!(color);
    end_game(
        state,
        Some(color.opponent()),
        if color == Color::White {
            "백 시간패"
        } else {
            "흑 시간패"
        },
    )?;
    Ok(false)
}

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
pub(crate) fn resolve_stars(state: &mut GameState) -> Result<()> {
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
    pause_clock(state)?;
    state.mode = "gameover".into();
    state.winner = winner.map(|color| color.as_str().into());
    Ok(())
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
        resolve_stars(state)?;
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
        resolve_stars(state)?;
        return Ok(true);
    }
    check_star_limit(state)
}

/// Cards check the turn limit without adding a board repetition occurrence.
pub(crate) fn check_star_limit(state: &mut GameState) -> Result<bool> {
    if state.result().is_some() {
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
            crate::replay::add_log(
                state,
                format!(
                    "연장전 시작: {limit}수 이후 {turns}수 동안 폰 이동, 포획, 액티브 카드 사용이 없으면 별이 더 적은 쪽이 승리합니다."
                ),
            )?;
        } else {
            resolve_stars(state)?;
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
