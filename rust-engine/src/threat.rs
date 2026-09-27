//! Source royal-capture probes used by playMoveSound. These simulations own
//! their boards/RNG and never replace an unported reaction with geometric
//! check detection. The supported direct-capture path runs the actual move
//! kernel; automatic hazards and forced continuations retain explicit errors.
use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn royal(state: &GameState, piece: &Piece) -> bool {
    state.royal_identity(piece)
        || matches!(piece.kind.as_str(), "vip" | "timeTraveler" | "vampireLord")
}

fn tick_protection(state: &mut GameState, color: Color, field: &str) {
    let mut seen = BTreeSet::new();
    let pieces = state
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|piece| {
            piece.color == color
                && piece
                    .extra
                    .get(field)
                    .is_some_and(|value| crate::observation::truth(Some(value)))
        })
        .filter(|piece| seen.insert(piece.id.clone()))
        .cloned()
        .collect::<Vec<_>>();
    for mut piece in pieces {
        let remaining = crate::observation::number(
            piece
                .extra
                .get(field)
                .and_then(|value| value.get("remaining")),
        )
        .unwrap_or(0.0);
        let remaining = if field == "sacrificeProtection" {
            remaining.max(0.0)
        } else {
            remaining
        } - 1.0;
        if remaining > 0.0 {
            if let Some(value) = piece.extra.get_mut(field).and_then(Value::as_object_mut) {
                value.insert("remaining".into(), json!(remaining));
            }
        } else {
            let other = if field == "lastResistance" {
                "sacrificeProtection"
            } else {
                "lastResistance"
            };
            let keep = crate::observation::truth(
                piece
                    .extra
                    .get(field)
                    .and_then(|value| value.get("previousProtected")),
            ) || ["coronationProtection", other, "queensGambitProtection"]
                .into_iter()
                .any(|name| crate::observation::truth(piece.extra.get(name)));
            piece.extra.shift_remove(field);
            if !keep {
                piece.extra.shift_remove("protected");
            }
        }
        for cell in state
            .board
            .iter_mut()
            .flatten()
            .flatten()
            .filter(|cell| cell.id == piece.id)
        {
            *cell = piece.clone();
        }
    }
}

fn clone_window(state: &GameState, defender: Color) -> GameState {
    let mut state = state.clone();
    for name in [
        "selected",
        "dragging",
        "targeting",
        "replayBaseFrame",
        "replayTailFrame",
        "pendingNotation",
        "collapsePending",
        "periodicCollapse",
    ] {
        state.extra.insert(name.into(), Value::Null);
    }
    for name in [
        "legalMoves",
        "boardHistory",
        "replayEvents",
        "notationTimeline",
        "notationEvents",
        "pendingNotations",
        "pendingReplayVisuals",
        "onlineEvents",
        "kingThreatEffectCauses",
        "kingThreatCaptureCauses",
        "logs",
    ] {
        state.extra.insert(name.into(), json!([]));
    }
    if state.turn == defender {
        tick_protection(&mut state, defender, "lastResistance");
        tick_protection(&mut state, defender, "sacrificeProtection");
    }
    state.mode = "play".into();
    state.turn = defender.opponent();
    state.actions_remaining = 1;
    if let Some(effects) = state
        .extra
        .get_mut("effects")
        .and_then(Value::as_object_mut)
    {
        effects.insert("extraMove".into(), json!(0));
    }
    state.history.clear();
    state.gameover_replay_pending = false;
    state
}

pub(crate) fn has_royal_capture(state: &GameState, defender: Color) -> Result<bool> {
    let mut seen = BTreeSet::new();
    let royals = state
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|piece| piece.color == defender && royal(state, piece))
        .filter(|piece| seen.insert(piece.id.clone()))
        .map(|piece| piece.id.clone())
        .collect::<BTreeSet<_>>();
    if royals.is_empty() {
        return Ok(false);
    }
    for field in [
        "conveyorRule",
        "ultimatum",
        "pendingGales",
        "delayedHazards",
        "pendingOtherworld",
    ] {
        if state.extra.get(field).is_some_and(|value| match value {
            Value::Array(values) => !values.is_empty(),
            Value::Object(values) => values
                .iter()
                .any(|(_, value)| crate::observation::truth(Some(value))),
            _ => crate::observation::truth(Some(value)),
        }) {
            return Err(EngineError::UnsupportedFeature(format!(
                "royal threat automatic reaction {field}"
            )));
        }
    }
    if state.board.iter().flatten().flatten().any(|piece| {
        piece.ability_kind() == "log" && crate::observation::truth(piece.extra.get("logDir"))
            || piece.ability_kind() == "siren"
            || piece.color == defender
                && (piece.ability_kind() == "brutus"
                    || piece
                        .extra
                        .get("witchTrial")
                        .and_then(|value| crate::observation::number(value.get("remaining")))
                        .is_some_and(|remaining| remaining <= 1.0))
    }) {
        return Err(EngineError::UnsupportedFeature(
            "royal threat automatic piece reaction".into(),
        ));
    }
    let window = clone_window(state, defender);
    for action in crate::movement::legal_move_actions(&window)? {
        let attacker = window
            .at(action.from.ok_or(EngineError::IllegalAction)?)
            .ok_or(EngineError::IllegalAction)?;
        if attacker.kind == "trickster"
            || attacker.extra.get("hiddenFrom").and_then(Value::as_str) == Some(defender.as_str())
        {
            continue;
        }
        let target = action
            .destination
            .as_ref()
            .ok_or(EngineError::IllegalAction)?;
        let mut cells = vec![target.square()];
        for name in ["jumpCapture"] {
            if let Some(value) = target.flags.get(name) {
                cells.push(
                    serde_json::from_value(value.clone()).map_err(EngineError::serialization)?,
                );
            }
        }
        for name in [
            "bigRookLandingCaptures",
            "colossusLandingCaptures",
            "sectorCells",
        ] {
            if let Some(value) = target.flags.get(name) {
                cells.extend(
                    serde_json::from_value::<Vec<Square>>(value.clone())
                        .map_err(EngineError::serialization)?,
                );
            }
        }
        if !cells.iter().any(|square| {
            window
                .at(*square)
                .is_some_and(|piece| royals.contains(&piece.id))
        }) {
            if cells.iter().any(|square| {
                window
                    .at(*square)
                    .is_some_and(|piece| crate::observation::truth(piece.extra.get("explosive")))
                    && (0..8).any(|row| {
                        (0..8).any(|col| {
                            square.row.abs_diff(row) <= 1
                                && square.col.abs_diff(col) <= 1
                                && window
                                    .at(Square { row, col })
                                    .is_some_and(|piece| royals.contains(&piece.id))
                        })
                    })
            }) {
                return Err(EngineError::UnsupportedFeature(
                    "royal threat explosive capture reaction".into(),
                ));
            }
            continue;
        }
        let mut child = window.clone();
        match crate::transition::execute_threat_move(&mut child, &action) {
            Ok(captures) => {
                if captures.iter().any(|piece| royals.contains(&piece.id))
                    || royals.iter().any(|id| {
                        !child
                            .board
                            .iter()
                            .flatten()
                            .flatten()
                            .any(|piece| &piece.id == id && piece.color == defender)
                    })
                {
                    return Ok(true);
                }
            }
            Err(EngineError::IllegalAction | EngineError::WrongActor | EngineError::Terminal) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

/// Headless local profile uses loadCheckAlertEnabled's absent-storage default
/// true. The source callback mutates lastMove metadata; audio output is absent.
pub(crate) fn play_move_sound(state: &mut GameState, default: &str, color: Color) -> Result<()> {
    if state.mode != "play" {
        return Ok(());
    }
    let mut check = false;
    for defender in [Color::White, Color::Black] {
        if has_royal_capture(state, defender)? {
            check = true;
            break;
        }
    }
    if check
        && let Some(last) = state
            .extra
            .get_mut("lastMove")
            .and_then(Value::as_object_mut)
        && last.get("soundName").and_then(Value::as_str) == Some(default)
        && (!crate::observation::truth(last.get("soundColor"))
            || last.get("soundColor").and_then(Value::as_str) == Some(color.as_str()))
    {
        last.insert("soundName".into(), json!("checkDanger"));
    }
    Ok(())
}
