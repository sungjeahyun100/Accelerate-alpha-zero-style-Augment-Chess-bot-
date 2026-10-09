//! Turn completion and the site's long-game adjudication. All counts belong to
//! the immutable rule state; UI timers and replay rendering are not executed.
use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) fn end_game(state: &mut GameState, winner: Option<Color>, reason: &str) -> Result<()> {
    crate::legal_profile::measure("game_over_apply", || {
        end_game_profiled(state, winner, reason)
    })
}

pub(crate) fn end_game_profiled(
    state: &mut GameState,
    winner: Option<Color>,
    reason: &str,
) -> Result<()> {
    end_game_with_source_winner(state, winner.map(Color::as_str), reason)
}

/// Extinction 원문은 null 대신 문자열 `draw`를 전달한다. 저장 상태와
/// 원문의 `undefined 승리` 로그까지 보존하되 임의 winner 문자열은 받지 않는다.
pub(crate) fn end_game_with_draw_literal(state: &mut GameState, reason: &str) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "literal draw winner requires v7".into(),
        ));
    }
    end_game_with_source_winner(state, Some("draw"), reason)
}

fn end_game_with_source_winner(
    state: &mut GameState,
    winner: Option<&str>,
    reason: &str,
) -> Result<()> {
    let needs_end_timestamp = state
        .extra
        .get("replayEndedAt")
        .is_none_or(|v| v.is_null() || v.as_str() == Some(""));
    let ended_at = if needs_end_timestamp {
        let catalog_source = match state.ruleset_id.as_str() {
            RULES_VERSION_V6 => include_str!("../../contracts/catalog/site-20260927.json"),
            RULES_VERSION_V7 => include_str!("../../contracts/catalog/site-20260928.json"),
            other => {
                return Err(EngineError::UnsupportedFeature(format!(
                    "end-game source for rules version {other}"
                )));
            }
        };
        let catalog: Value = serde_json::from_str(catalog_source).expect("adopted catalog");
        Some(catalog["source"]["frozenAt"].clone())
    } else {
        None
    };
    let pause_live_clock = if state.ruleset_id == RULES_VERSION_V7 {
        // main109754: AI simulation과 threat probe에서는 clock을 멈추지 않는다.
        !state.is_ai_simulation()
    } else {
        state.threat_probe_depth == 0
    };
    if pause_live_clock {
        pause_clock(state)?;
    }
    state.mode = "gameover".into();
    state.winner = winner.map(str::to_owned);
    if let Some(ended_at) = ended_at {
        state.extra.insert("replayEndedAt".into(), ended_at);
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
                .map(|color| format!(
                    "{} 승리",
                    match color {
                        "white" => "백",
                        "black" => "흑",
                        _ => "undefined",
                    }
                ))
                .unwrap_or_else(|| "무승부".into())
        ),
    )?;
    // main109739의 microtask 예약은 AI만 막는다. Threat probe의 snapshot은
    // recordBoardHistory가 소유하는 compact simulation event를 정산한다.
    state.gameover_replay_pending = if state.ruleset_id == RULES_VERSION_V7 {
        state.ai_simulation_depth == 0
    } else {
        state.threat_probe_depth == 0
    };
    Ok(())
}

pub(crate) fn check_democracy_defeat(
    state: &mut GameState,
    color: Color,
    winner: Color,
    cause: &str,
) -> Result<bool> {
    crate::legal_profile::measure("game_over_democracy_check", || {
        check_democracy_defeat_profiled(state, color, winner, cause)
    })
}

pub(crate) fn check_democracy_defeat_profiled(
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
    if !crate::observation::truth(clock.get("enabled")) {
        return Ok(None);
    }
    let key = format!("{}Ms", color.as_str());
    if state.ruleset_id == RULES_VERSION_V7 {
        // Each frozen local restore starts a cold performance.now display
        // anchor. The old Date.now epoch identifies it; it is not elapsed time.
        return Ok(Some(
            crate::observation::number(clock.get(&key))
                .filter(|value| value.is_finite())
                .unwrap_or(0.0)
                .max(0.0),
        ));
    }
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
            - (crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)? as f64
                - clock["lastStartedAt"].as_f64().unwrap())
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
        .filter(|_| crate::observation::truth(clock.get("lastStartedAt")))
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
    start_clock_for(state, state.turn)
}

/// 원문 startClockForTurn의 명시 actor 경계. 승급 창도 Siren tick을 먼저
/// 실행하며, freeMove의 임시 actor가 state.turn과 달라도 그 actor를 사용한다.
pub(crate) fn start_clock_for(state: &mut GameState, color: Color) -> Result<()> {
    if state.ruleset_id == RULES_VERSION_V7 {
        if state.mode == "play" {
            crate::v7_turn_entry::tick_siren_exposure_for_turn_start(state, color)?;
        }
    } else {
        tick_siren_turn_start(state)?;
    }
    if state.mode != "play"
        || state.turn != color
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
        || (clock["runningColor"] == color.as_str()
            && crate::observation::truth(clock.get("lastStartedAt")))
    {
        return Ok(());
    }
    clock["runningColor"] = json!(color);
    clock["lastStartedAt"] = json!(crate::draft::frozen_timestamp_for_ruleset(
        &state.ruleset_id
    )?);
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
    if state.ruleset_id == RULES_VERSION_V7 {
        return crate::v7_turn_flow::check_no_action_loss_v7(state);
    }
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
    // The client excludes cards while an extra move is forced. Its untargeted
    // card probe may consume the real RNG, so the order matters even when a
    // legal board move is eventually found.
    let forced_extra_move = has_active_forced_extra_move(state);
    if !forced_extra_move && crate::transition::available_card_action(state, state.turn)? {
        return Ok(false);
    }
    // The source card probe above is allowed to advance the real RNG. Only
    // after it returns false may this bounded first-play witness short-circuit
    // move enumeration. Relay adds 128 extra move actions, so this helper
    // proves existence of one orthodox pawn move, not the public move list.
    if state.ruleset_id == RULES_VERSION_V7
        && crate::movement::v7_first_play_has_legal_move_witness(state, state.turn)? == Some(true)
    {
        return Ok(false);
    }
    if !crate::movement::legal_move_actions(state)?.is_empty() {
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
    // The source's hasAnyLegalMove probes football regardless of its neutral
    // color. The current Rust move enumerator covers only actor-colored
    // pieces; a football can therefore defeat the apparent empty-action
    // conclusion. Until source getLegalMoves is fully connected for this
    // piece, do not declare a v7 loss from that incomplete enumeration.
    if state.ruleset_id == RULES_VERSION_V7
        && state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.kind == "football")
    {
        return Err(EngineError::UnsupportedFeature(
            "v7 no-action football legality".into(),
        ));
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

fn has_active_forced_extra_move(state: &GameState) -> bool {
    state.board.iter().flatten().flatten().any(|piece| {
        piece.color == state.turn
            && [
                "thiefSecondMove",
                "frenzyExtraMove",
                "fileSurgeSecondMove",
                "rookLiftSecondMove",
                "ironMonarchExtraMove",
                "underpromotionSecondMove",
                "checkerChainCapture",
                "madHorseSecondMove",
                "platformExtraMove",
                "desperado",
            ]
            .into_iter()
            .any(|key| crate::observation::truth(piece.extra.get(key)))
    })
}

/// Frozen v7 completeTurnAfterMove calls this after repetition/star settlement
/// and before checkNoActionLoss. The four source-probed seed-37 normal/chaos
/// MIDDLE/END entry states agree on the complete state and RNG after the draw.
/// A wider long-game transition still needs predecessor effect coverage.
pub(crate) fn maybe_start_milestone_draft(state: &mut GameState) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 || state.mode == "draft" || state.mode == "gameover" {
        return Ok(false);
    }
    if state.mode != "play" {
        return Err(EngineError::UnsupportedFeature(
            "v7 milestone draft from non-play state".into(),
        ));
    }
    if state.extra.get("gameStyle").and_then(Value::as_str) == Some("grand")
        && !crate::observation::truth(state.extra.get("campaign"))
    {
        return Ok(false);
    }
    if has_active_forced_extra_move(state)
        || state.extra.get("localMode").and_then(Value::as_str) == Some("tutorial")
        || crate::observation::truth(state.extra.get("draftDelete"))
    {
        return Ok(false);
    }
    let shared_turns = state.turns_taken.white.min(state.turns_taken.black);
    let middle_done = crate::observation::truth(state.extra.get("middleDraftDone"));
    let (phase, milestone) = if !crate::observation::truth(state.extra.get("endDraftDone"))
        && middle_done
        && shared_turns >= 20
    {
        ("END", 20)
    } else if !middle_done && shared_turns >= 10 {
        ("MIDDLE", 10)
    } else {
        return Ok(false);
    };
    let style = state.extra.get("gameStyle").and_then(Value::as_str);
    if !matches!(style, Some("normal" | "chaos"))
        || crate::observation::truth(state.extra.get("campaign"))
        || crate::observation::truth(state.extra.get("shotgunDlc"))
        || state
            .extra
            .get("shotgunOpeningColor")
            .is_some_and(|v| !v.is_null())
        || state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| crate::observation::truth(piece.extra.get("repositionSecondMove")))
    {
        return Err(EngineError::UnsupportedFeature(
            "v7 milestone draft predecessor or first-pick variant".into(),
        ));
    }
    let resume_turn = state.turn;
    crate::replay::add_log(
        state,
        format!("양쪽 모두 {milestone}턴을 마쳐 {phase} 카드 선택으로 넘어갑니다."),
    )?;
    state
        .extra
        .insert("draftResumeTurn".into(), json!(resume_turn));
    crate::draft::start_draft(state, Color::White, phase)?;
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
    clock["lastStartedAt"] = json!(crate::draft::frozen_timestamp_for_ruleset(
        &state.ruleset_id
    )?);
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
        .fold(0.0, |sum, card| sum + card.star_value())
}

pub(crate) fn record_position(state: &mut GameState) -> Result<u64> {
    let mut seen = BTreeSet::new();
    let mut pieces = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, cell) in cells.iter().enumerate() {
            let Some(piece) = cell.as_ref() else {
                continue;
            };
            if piece.kind == "wall" || !seen.insert(piece.id.clone()) {
                continue;
            }
            let attribute = |name: &str| -> Result<String> {
                piece
                    .extra
                    .get(name)
                    .filter(|v| !v.is_null())
                    .map(|v| {
                        if state.ruleset_id == RULES_VERSION_V7 {
                            return source_interpolated_text(v, 0);
                        }
                        Ok(v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string()))
                    })
                    .unwrap_or_else(|| Ok(String::new()))
            };
            pieces.push(format!(
                "{row},{col}:{}:{}:{}:{}:{}",
                piece.color.as_str(),
                piece.kind,
                attribute("hp")?,
                attribute("ammo")?,
                u8::from(crate::observation::truth(piece.extra.get("frozen")))
            ));
        }
    }
    pieces.sort();
    let salt = if state.ruleset_id == RULES_VERSION_V7 {
        state
            .extra
            .get("repetitionSalt")
            .filter(|value| crate::observation::truth(Some(value)))
            .map(|value| source_interpolated_text(value, 0))
            .transpose()?
            .unwrap_or_else(|| "0".into())
    } else {
        state
            .extra
            .get("repetitionSalt")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .to_string()
    };
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
pub(crate) fn resolve_stars(state: &mut GameState, reason: &str) -> Result<()> {
    let prefix = if reason.is_empty() {
        String::new()
    } else {
        format!("{reason}: ")
    };
    let penalty = if state.ruleset_id == RULES_VERSION_V7 {
        crate::card_registry::v7_shotgun_penalty_color(state)?
    } else {
        shotgun_color(state)
    };
    if let Some(color) = penalty {
        return end_game(
            state,
            Some(color.opponent()),
            &format!("{prefix}샷건 킹은 장기전 판정에서 패배합니다."),
        );
    }
    let white = star_total(state, Color::White);
    let black = star_total(state, Color::Black);
    let scores = format!("({white} : {black})");
    if white == black {
        end_game(
            state,
            None,
            &format!("{prefix}별 합계가 같아 무승부입니다. {scores}"),
        )
    } else {
        let winner = if white < black {
            Color::White
        } else {
            Color::Black
        };
        end_game(
            state,
            Some(winner),
            &format!("{prefix}덱의 별이 더 적습니다. {scores}"),
        )
    }
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
        resolve_stars(state, "")?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub(crate) fn check_termination(state: &mut GameState) -> Result<bool> {
    if state.result().is_some() {
        return Ok(true);
    }
    let penalty = if state.ruleset_id == RULES_VERSION_V7 {
        crate::card_registry::v7_shotgun_penalty_color(state)?
    } else {
        shotgun_color(state)
    };
    if penalty.is_none() && record_position(state)? >= 3 {
        resolve_stars(state, "3회 동형반복")?;
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
            // The v7 client stores the normalized setting before deriving
            // its interval. The v6 branch keeps its original integer rule.
            let turns = if state.ruleset_id == RULES_VERSION_V7 {
                let rounded = crate::observation::number(state.extra.get("deathmatchLimitTurns"))
                    .filter(|value| *value > 0.0)
                    .map(|value| value.round().max(1.0))
                    .unwrap_or(10.0);
                // The doubled interval must be exact in the client's Number
                // counter and in the serialized source snapshot.
                if rounded >= (1_u64 << 52) as f64 {
                    return Err(EngineError::InvalidState(
                        "deathmatch interval exceeds exact integer bound".into(),
                    ));
                }
                let turns = rounded as u64;
                state
                    .extra
                    .insert("deathmatchLimitTurns".into(), json!(turns));
                turns
            } else {
                state
                    .extra
                    .get("deathmatchLimitTurns")
                    .and_then(Value::as_u64)
                    .filter(|value| *value > 0)
                    .unwrap_or(10)
            };
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
            resolve_stars(state, &format!("{limit}수"))?;
            return Ok(true);
        }
    }
    Ok(false)
}
pub(crate) fn note_card_event(state: &mut GameState) -> Result<()> {
    if state.ruleset_id == RULES_VERSION_V7 {
        // main94804: JavaScript Number의 정수/실수 표현을 구분하지 않는다.
        let previous = state
            .extra
            .get("repetitionSalt")
            .filter(|value| crate::observation::truth(Some(value)));
        let salt = match previous {
            None => json!(1),
            Some(Value::Number(number)) => {
                let next = number
                    .as_f64()
                    .filter(|number| number.is_finite())
                    .ok_or_else(|| {
                        EngineError::InvalidState(
                            "v7 repetitionSalt requires a finite Number".into(),
                        )
                    })?
                    + 1.0;
                if !next.is_finite() {
                    return Err(EngineError::InvalidState(
                        "v7 repetitionSalt increment is nonfinite".into(),
                    ));
                }
                serde_json::from_str(
                    &serde_jcs::to_string(&json!(next)).map_err(EngineError::serialization)?,
                )
                .map_err(EngineError::serialization)?
            }
            Some(Value::Bool(true)) => json!(2),
            Some(value) => Value::String(format!("{}1", source_interpolated_text(value, 0)?)),
        };
        // 원문의 truthy Map만 clear한다. 누락/null Map을 생성하는 시점은 recordPosition이다.
        if let Some(table) = state
            .extra
            .get_mut("positionCounts")
            .filter(|value| crate::observation::truth(Some(value)))
        {
            if table.get("__simType").and_then(Value::as_str) != Some("Map") {
                return Err(EngineError::InvalidState(
                    "v7 positionCounts.clear requires the Map snapshot encoding".into(),
                ));
            }
            table
                .get_mut("entries")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| {
                    EngineError::InvalidState("v7 positionCounts Map requires entries".into())
                })?
                .clear();
        }
        state.extra.insert("repetitionSalt".into(), salt);
        return Ok(());
    }
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

/// JSON로 전달된 원문 값의 template literal 문자열화. 호출 경계의 depth 64를 유지한다.
fn source_interpolated_text(value: &Value, depth: usize) -> Result<String> {
    if depth > 64 {
        return Err(EngineError::InvalidState(
            "v7 repetition key coercion exceeds depth 64".into(),
        ));
    }
    Ok(match value {
        Value::Null => "null".into(),
        Value::Bool(value) => value.to_string(),
        Value::Number(_) => serde_jcs::to_string(value).map_err(EngineError::serialization)?,
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .map(|value| {
                if value.is_null() {
                    Ok(String::new())
                } else {
                    source_interpolated_text(value, depth + 1)
                }
            })
            .collect::<Result<Vec<_>>>()?
            .join(","),
        Value::Object(object) => {
            if object.contains_key("toString") {
                return Err(EngineError::InvalidState(
                    "v7 repetition key object has a non-callable primitive conversion property"
                        .into(),
                ));
            }
            match object.get("__simType").and_then(Value::as_str) {
                Some("Map") => "[object Map]".into(),
                Some("Set") => "[object Set]".into(),
                _ => "[object Object]".into(),
            }
        }
    })
}

#[cfg(test)]
#[path = "flow_v7_tests.rs"]
mod flow_v7_tests;

#[cfg(test)]
#[path = "flow_upstream_tests.rs"]
mod flow_upstream_tests;
