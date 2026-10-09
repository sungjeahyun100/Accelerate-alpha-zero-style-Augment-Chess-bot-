//! 동결 v7 이동의 공통 실행 순서와 거래 경계.
//!
//! 후보 기하, 포획 반응, 특수 이동과 개별 기물 효과는 각 객체가 소유한다.
//! 이 모듈은 같은 mover identity와 이동 전 지역 변수를 유지하며 원문의
//! 관측 → 방어 → 포획 → 착지 → 기보 → 반응 → 승급 → 추가 이동을 연결한다.

use crate::observation::truth;
use crate::v7_capture_reactions::{CaptureOptions, DefendedAttack};
use crate::v7_move_continuations::{
    V7ContinuationControl, V7MoveContinuationInput, V7MoveContinuationSnapshot,
};
use crate::v7_move_execution::{V7MovePrelude, V7MoveSelection};
use crate::v7_move_piece_effects::{V7MovePieceEffectsContext, V7PlacementPhase};
use crate::{
    Action, Color, EnPassant, EngineError, Fields, GameState, MoveTarget, Piece, Result, Square,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn square_name(at: Square) -> String {
    format!("{}{}", char::from(b'a' + at.col), 8 - at.row)
}

fn descriptor_square(target: &MoveTarget, key: &str) -> Result<Option<Square>> {
    let Some(value) = target.flags.get(key).filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let at: Square = serde_json::from_value(value.clone()).map_err(|error| {
        EngineError::InvalidState(format!("v7 movement {key} is not a square: {error}"))
    })?;
    if at.row >= 8 || at.col >= 8 {
        return Err(EngineError::InvalidState(format!(
            "v7 movement {key} is outside the frozen board"
        )));
    }
    Ok(Some(at))
}

fn en_passant_victim(target: &MoveTarget) -> Result<Option<Square>> {
    if !target.flag("enPassant") {
        return Ok(None);
    }
    let coordinate = |name| {
        target
            .flags
            .get(name)
            .and_then(Value::as_u64)
            .filter(|number| *number < 8)
            .map(|number| number as u8)
            .ok_or_else(|| {
                EngineError::InvalidState(format!(
                    "v7 en passant {name} is not an in-bounds integer"
                ))
            })
    };
    Ok(Some(Square {
        row: coordinate("capturedRow")?,
        col: coordinate("capturedCol")?,
    }))
}

fn publish(state: &mut GameState, moving: &Piece) {
    crate::v7_board_hazards::replace_object_aliases(state, moving);
}

fn live(state: &GameState, id: &str, at: Square) -> Option<Piece> {
    state.at(at).filter(|piece| piece.id == id).cloned()
}

fn delete_property(piece: &mut Piece, key: &str) {
    piece.extra.shift_remove(key);
    piece.source_order.retain(|name| name != key);
}

fn cancel_selection(state: &mut GameState) {
    state.extra.insert("selected".into(), Value::Null);
    state.extra.insert("legalMoves".into(), json!([]));
}

#[derive(Default)]
struct CaptureLedger {
    captures: Vec<Piece>,
    direct: Option<Piece>,
    jump: Option<Piece>,
    explosions: Vec<Square>,
    notation_cells: [Vec<Square>; 2],
    captured_something: bool,
    captured_outside_jump: bool,
    captured_by_moving_piece: bool,
    captured_mad_horse_friendly: bool,
    checker_jumped_attack: bool,
}

impl CaptureLedger {
    fn remember(&mut self, captured: Piece, at: Square, jumping: bool) {
        self.captured_something = true;
        self.captured_by_moving_piece = true;
        if !jumping {
            self.captured_outside_jump = true;
            if let Some(color) = captured.color.owner() {
                let cells = &mut self.notation_cells[usize::from(color == Color::Black)];
                if !cells.contains(&at) {
                    cells.push(at);
                }
            }
            self.direct = Some(captured.clone());
        } else {
            self.jump = Some(captured.clone());
        }
        if truth(captured.extra.get("explosive"))
            && !truth(captured.extra.get("kingThreatSuppressed"))
        {
            self.explosions.push(at);
        }
        self.captures.push(captured);
    }

    fn known_squares(&self) -> Value {
        let mut known = Fields::new();
        for color in [Color::White, Color::Black] {
            let cells = &self.notation_cells[usize::from(color == Color::Black)];
            if cells.len() == 1 {
                known.insert(color.as_str().into(), json!(cells[0]));
            }
        }
        Value::Object(known)
    }
}

fn finish_control(
    state: &mut GameState,
    actor: Color,
    control: V7ContinuationControl,
) -> Result<()> {
    crate::legal_profile::measure("move_finish_control", || {
        finish_control_profiled(state, actor, control)
    })
}

fn finish_control_profiled(
    state: &mut GameState,
    actor: Color,
    control: V7ContinuationControl,
) -> Result<()> {
    if let V7ContinuationControl::FinishMove(color) = control {
        crate::transition::end_move_for_decision(state, color, true, Some("move"))?;
    } else if control == V7ContinuationControl::Continue {
        crate::transition::end_move_for_decision(state, actor, true, Some("move"))?;
    }
    Ok(())
}

fn finish_removed_mover(
    state: &mut GameState,
    moving: &mut Piece,
    start: &V7MoveContinuationSnapshot,
    from: Square,
    to: Square,
    captured: bool,
) -> Result<()> {
    crate::legal_profile::measure("move_finish_removed_mover", || {
        finish_removed_mover_profiled(state, moving, start, from, to, captured)
    })
}

fn finish_removed_mover_profiled(
    state: &mut GameState,
    moving: &mut Piece,
    start: &V7MoveContinuationSnapshot,
    from: Square,
    to: Square,
    captured: bool,
) -> Result<()> {
    let actor = moving.color.owner().ok_or(EngineError::WrongActor)?;
    if !crate::v7_move_continuations::retain_backward_knight_turn_v7(
        state,
        moving,
        crate::v7_move_continuations::V7BackwardKnightMove {
            color: actor,
            moved_as_type: &start.moved_as_type,
            from,
            to,
            captured,
            queued: start.had_queued_backward_knight_turn,
        },
    )? {
        crate::transition::end_move_for_decision(state, actor, true, Some("move"))?;
    }
    Ok(())
}

/// main92022-92123. 제자리 방어·HP 공격에는 beginMoveReplayCapture가 없다.
fn stationary_defense(
    state: &mut GameState,
    moving: &mut Piece,
    from: Square,
    prelude: &V7MovePrelude,
    target: &MoveTarget,
    start: &V7MoveContinuationSnapshot,
    options: &CaptureOptions,
) -> Result<Option<Vec<Piece>>> {
    let actor = moving.color.owner().ok_or(EngineError::WrongActor)?;
    let mut candidates = Vec::new();
    if let Some((at, victim)) = prelude
        .portal_entry
        .zip(prelude.portal_entry_target.as_ref())
    {
        candidates.push((at, victim.clone(), "포탈 진입", false, true));
    }
    if let Some(victim) = &prelude.landing_target {
        candidates.push((prelude.destination, victim.clone(), "공격", true, true));
    }
    if let Some(at) = en_passant_victim(target)?
        && let Some(victim) = state.at(at).cloned()
    {
        candidates.push((at, victim, "앙파상", false, false));
    }
    for (at, original_victim, name, direct_hp, immediate_reaper) in candidates {
        // Forced Mistake skips the direct defense branches, but the source's
        // en-passant callback has its own unconditional defense handling.
        let mut local_options = options.clone();
        if name == "앙파상" {
            local_options.force_capture = false;
        }
        let source_label = if name == "앙파상" {
            name.to_owned()
        } else {
            format!(
                "{}의 {}",
                crate::replay::source_piece_label(&moving.kind).unwrap_or("undefined"),
                if name == "포탈 진입" {
                    "포탈 진입 공격"
                } else {
                    "공격"
                }
            )
        };
        let outcome = crate::v7_capture_reactions::attack_defended_piece(
            state,
            moving,
            at,
            &source_label,
            &local_options,
        )?;
        if outcome == DefendedAttack::NotDefended {
            continue;
        }
        let mut captures = Vec::new();
        if let DefendedAttack::Health { removed } = outcome {
            let captured = removed.is_some();
            let victim = state
                .at(at)
                .filter(|piece| piece.id == original_victim.id)
                .cloned()
                .or_else(|| removed.clone())
                .unwrap_or(original_victim);
            crate::replay::queue_hp_attack_notation(
                state,
                moving,
                from,
                &victim,
                at,
                &prelude.privacy,
            )?;
            if let Some(removed) = removed {
                captures.push(removed);
            }
            if captured && immediate_reaper {
                crate::transition::finalize_immediate_reaper_execution(
                    state,
                    moving,
                    from,
                    &start.moved_as_type,
                )?;
            }
            if captured && state.mode != "gameover" {
                crate::card_effects::apply_transcendence_capture_upgrade_v7(
                    state,
                    moving,
                    from,
                    &start.moved_as_type,
                )?;
            }
            let control = crate::v7_move_continuations::after_stationary_hp_v7(
                state, moving, from, start, captured, direct_hp,
            )?;
            finish_control(state, actor, control)?;
        } else {
            crate::v7_move_continuations::clear_consumed_extra_move_flags(moving, start);
            moving
                .extra
                .insert("coolGuyCapturedLast".into(), json!(false));
            publish(state, moving);
            crate::replay::add_log(
                state,
                format!("{}의 방어막이 {}을 막았습니다.", square_name(at), name),
            )?;
            state.en_passant = None;
            crate::transition::end_move_for_decision(state, actor, true, Some("move"))?;
        }
        return Ok(Some(captures));
    }
    Ok(None)
}

fn next_en_passant(
    piece: &Piece,
    from: Square,
    to: Square,
    target: &MoveTarget,
    moved_as_type: &str,
) -> Result<Option<EnPassant>> {
    if target.flag("mistakeReverse")
        || piece.kind != "pawn"
        || moved_as_type != "pawn"
        || from.col != to.col
    {
        return Ok(None);
    }
    let distance = from.row.abs_diff(to.row);
    if !(distance == 2 && target.flags.get("standardPawnDoubleStep") == Some(&json!(true))
        || distance == 3 && target.flags.get("pawnSprintTripleStep") == Some(&json!(true)))
    {
        return Ok(None);
    }
    let actor = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let direction = (i16::from(to.row) - i16::from(from.row)).signum();
    let mut extra = Fields::new();
    if distance == 3 {
        extra.insert(
            "additional".into(),
            json!([{"row":i16::from(to.row)-direction*2,
            "col":to.col,"capturedRow":to.row,"capturedCol":to.col,"color":actor}]),
        );
    }
    Ok(Some(EnPassant {
        row: (i16::from(to.row) - direction) as u8,
        col: to.col,
        captured_row: to.row,
        captured_col: to.col,
        color: actor,
        extra,
    }))
}

fn piece_move_log(
    state: &GameState,
    moving: &Piece,
    from: Square,
    to: Square,
    privacy: &Value,
) -> Result<String> {
    let actor = moving.color.owner().ok_or(EngineError::WrongActor)?;
    if crate::replay::fog_log_redaction_active_v7(state) {
        return Ok(format!(
            "{} 기물이 이동했습니다.",
            crate::replay::label(actor)
        ));
    }
    let origin_hidden = privacy
        .get(actor.opponent().as_str())
        .and_then(|entry| entry.get("originVisible"))
        == Some(&json!(false));
    if origin_hidden || crate::observation::piece_hidden_from_v7(state, moving, to).is_some() {
        return Ok("기물이 움직였습니다.".into());
    }
    Ok(format!(
        "{} {}: {} -> {}",
        crate::replay::label(actor),
        crate::replay::source_piece_label(&moving.kind).unwrap_or("undefined"),
        square_name(from),
        square_name(to)
    ))
}

pub(crate) fn sync_active_metal(state: &mut GameState) -> Result<()> {
    if let Some(context) = state.active_v7_move_context.clone() {
        crate::v7_move_execution::sync_move_execution_context(state, &context)?;
    }
    Ok(())
}

pub(crate) fn finish_active_roller(state: &mut GameState) -> Result<()> {
    if let Some(mut context) = state.active_v7_move_context.take() {
        let result = crate::v7_move_execution::finish_roller_context(state, &mut context);
        state.active_v7_move_context = Some(context);
        result?;
    }
    Ok(())
}

fn defeat_royal(state: &GameState, piece: Option<&Piece>) -> Result<bool> {
    let Some(piece) = piece else {
        return Ok(false);
    };
    Ok(crate::v7_board_hazards::source_royal_identity(state, piece)? || piece.kind == "vip")
}

/// main91454: wrapper-local metal/roller/medium과 saturation 문맥은
/// 실패·취소·중첩 호출에서도 원래 값으로 복원한다.
pub(crate) fn execute(
    state: &mut GameState,
    action: &Action,
    threat_probe: bool,
) -> Result<Vec<Piece>> {
    crate::legal_profile::measure("move_execute", || {
        execute_profiled(state, action, threat_probe)
    })
}

pub(crate) fn execute_profiled(
    state: &mut GameState,
    action: &Action,
    threat_probe: bool,
) -> Result<Vec<Piece>> {
    let mut effective_action = action.clone();
    let mut from = action.from.ok_or(EngineError::IllegalAction)?;
    let target = action
        .destination
        .as_ref()
        .ok_or(EngineError::IllegalAction)?;
    if let Some(ghost) = descriptor_square(target, "quantumFrom")? {
        let original = state.at(from).cloned().ok_or(EngineError::IllegalAction)?;
        from = crate::movement::v7_normalize_origin(&original, from)?;
        crate::v7_quantum_state::prepare_quantum_move(state, from, ghost, &original.id)?;
        from = ghost;
        effective_action.from = Some(ghost);
    }
    let target = effective_action
        .destination
        .as_ref()
        .ok_or(EngineError::IllegalAction)?;
    let context = crate::v7_move_execution::capture_move_execution_context(state, from, target)?;
    let saturation = state.at(from).map(|piece| {
        (
            piece.id.clone(),
            crate::v7_capture_reactions::saturation_locked(state, piece),
        )
    });
    let previous = state.active_v7_move_context.replace(context);
    let previous_saturation = std::mem::replace(&mut state.active_v7_saturation_attack, saturation);
    let mut result = execute_core(state, &effective_action, threat_probe);
    state.active_v7_saturation_attack = previous_saturation;
    if result.is_ok()
        && let Err(error) = sync_active_metal(state)
    {
        result = Err(error);
    }
    let cleanup = finish_active_roller(state);
    state.active_v7_move_context = previous;
    match (result, cleanup) {
        (Ok(captures), Ok(())) => Ok(captures),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(EngineError::InvalidState(format!(
            "v7 move failed: {error}; Roller finalization also failed: {cleanup}"
        ))),
    }
}

fn execute_core(state: &mut GameState, action: &Action, threat_probe: bool) -> Result<Vec<Piece>> {
    crate::legal_profile::measure("move_core", || {
        execute_core_profiled(state, action, threat_probe)
    })
}

fn execute_core_profiled(
    state: &mut GameState,
    action: &Action,
    threat_probe: bool,
) -> Result<Vec<Piece>> {
    crate::v7_card_context::begin_board_action(state);
    let mut from = action.from.ok_or(EngineError::IllegalAction)?;
    let target = action
        .destination
        .as_ref()
        .ok_or(EngineError::IllegalAction)?;
    if from.row >= 8 || from.col >= 8 || target.row >= 8 || target.col >= 8 {
        return Err(EngineError::IllegalAction);
    }
    let original = state.at(from).cloned().ok_or(EngineError::IllegalAction)?;
    from = crate::movement::v7_normalize_origin(&original, from)?;
    match crate::movement::v7_move_attempt_origin_verdict(state, &original, from, target)? {
        crate::movement::V7MoveOriginVerdict::Allowed => {}
        crate::movement::V7MoveOriginVerdict::Cancel => return Ok(Vec::new()),
        crate::movement::V7MoveOriginVerdict::ClearSelection => {
            cancel_selection(state);
            return Ok(Vec::new());
        }
    }
    if let Some(outcome) =
        crate::v7_move_execution::execute_football_move(state, from, target, threat_probe)?
    {
        return crate::transition::finish_stationary_move(state, outcome);
    }
    if let Some(outcome) = crate::v7_move_execution::execute_log_direction(state, from, target)? {
        return crate::transition::finish_stationary_move(state, outcome);
    }
    let mut prelude =
        match crate::v7_move_execution::prepare_v7_move_selection(state, from, target)? {
            V7MoveSelection::Ready(prelude) => *prelude,
            V7MoveSelection::Return(outcome) => {
                return crate::transition::finish_stationary_move(state, outcome);
            }
        };
    let mut moving = prelude.moving.clone();
    let mut actor = moving.color.owner().ok_or(EngineError::WrongActor)?;
    let start = crate::v7_move_continuations::capture_move_start(&mut moving);
    publish(state, &moving);
    let thief_end = if crate::v7_move_execution::uses_thief_remake_v7(state) {
        prelude.portal_entry.unwrap_or(prelude.destination)
    } else {
        prelude.destination
    };
    let internal_thief_jump = start.moved_ability_type == "thief"
        && crate::movement::v7_thief_jumped_piece(state, from, thief_end)?;
    if crate::transition::should_store_first_move_undo(state, actor)? {
        let undo = crate::transition::capture_first_move_undo(state, actor);
        state.extra.insert("firstMoveUndo".into(), undo);
    }
    if let Some(outcome) = crate::v7_move_execution::execute_castle(
        state,
        from,
        target,
        &prelude.privacy,
        threat_probe,
    )? {
        return crate::transition::finish_stationary_move(state, outcome);
    }
    if let Some(outcome) = crate::v7_move_execution::execute_large_piece_move(
        state,
        from,
        target,
        &prelude.privacy,
        threat_probe,
    )? {
        return crate::transition::finish_stationary_move(state, outcome);
    }
    if let Some(outcome) =
        crate::v7_move_execution::execute_colossus_sector(state, from, target, threat_probe)?
    {
        return crate::transition::finish_stationary_move(state, outcome);
    }
    if target.flag("dragonSwap")
        && let Some(outcome) = crate::v7_move_execution::execute_position_swap_with_privacy(
            state,
            from,
            target,
            threat_probe,
            Some(&prelude.privacy),
        )?
    {
        return crate::transition::finish_stationary_move(state, outcome);
    }
    if let Some(outcome) =
        crate::v7_move_execution::execute_shotgun(state, from, target, threat_probe)?
    {
        return crate::transition::finish_stationary_move(state, outcome);
    }
    if let Some(outcome) = crate::v7_move_execution::execute_grappler_pull(state, from, target)? {
        return crate::transition::finish_stationary_move(state, outcome);
    }
    if let Some(outcome) =
        crate::v7_move_execution::execute_merchant_purchase(state, from, target, threat_probe)?
    {
        return crate::transition::finish_stationary_move(state, outcome);
    }
    if !target.flag("dragonSwap")
        && let Some(outcome) = crate::v7_move_execution::execute_position_swap_with_privacy(
            state,
            from,
            target,
            threat_probe,
            Some(&prelude.privacy),
        )?
    {
        return crate::transition::finish_stationary_move(state, outcome);
    }
    let mut siege_replay = None;
    let mut siege_captures = Vec::new();
    let mut siege_captured_something = false;
    let mut siege_chameleon_victim = None;
    if let Some(outcome) = crate::v7_move_execution::execute_siege_ram_pre_capture(
        state,
        &mut prelude,
        target,
        threat_probe,
    )? {
        match outcome {
            crate::v7_move_execution::SiegeRamPreCapture::Return(outcome) => {
                return crate::transition::finish_stationary_move(state, outcome);
            }
            crate::v7_move_execution::SiegeRamPreCapture::Continue {
                captured_something,
                chameleon_victim,
                captures,
                replay_before,
            } => {
                siege_replay = Some(*replay_before);
                siege_captures = captures;
                siege_captured_something = captured_something;
                siege_chameleon_victim = chameleon_victim;
                moving = prelude.moving.clone();
            }
        }
    }
    if let Some(outcome) = crate::v7_move_execution::execute_missionary_conversion(
        state,
        from,
        target,
        &prelude.privacy,
        prelude.quantum_observation.captured_illusion(),
        threat_probe,
    )? {
        return crate::transition::finish_stationary_move(state, outcome);
    }
    if !crate::movement::v7_move_attempt_capture_allowed(
        state,
        &moving,
        from,
        target,
        crate::movement::V7CaptureLanding {
            actual_destination: prelude.destination,
            landing_target: prelude.landing_target.as_ref(),
            portal_entry_target: prelude.portal_entry_target.as_ref(),
            mad_horse_entry: prelude.mad_horse_entry_capture,
            mad_horse_exit: prelude.mad_horse_exit_capture,
        },
    )? {
        return Ok(Vec::new());
    }
    let disambiguation = if target.flag("mistakeReverse") {
        String::new()
    } else {
        crate::replay::move_notation_disambiguation_v7(state, &moving, from, prelude.destination)?
    };
    if let Some(plan) = crate::v7_move_execution::roll_mistake_reversal(state, &prelude, target)? {
        crate::replay::queue_mistake_attempt_notation_v7(
            state,
            &moving,
            from,
            plan.attempted_destination,
            target,
            &crate::replay::V7MoveNotationOptions {
                piece_type: &start.moved_as_type,
                disambiguation: &disambiguation,
                promotion: target.flags.get("promotion").and_then(Value::as_str),
                privacy: &prelude.privacy,
                capture: true,
                capture_known_squares: &json!({plan.counter.color.as_str():plan.counter_from}),
                game_end: plan.attempted_game_end,
            },
        )?;
        let context = crate::v7_move_execution::begin_mistake_reversal(state, &plan)?;
        let mut reverse = MoveTarget::at(plan.destination);
        reverse.flags.insert("mistakeReverse".into(), json!(true));
        let result = execute(
            state,
            &Action::movement(
                plan.counter.color.owner().ok_or(EngineError::WrongActor)?,
                plan.counter_from,
                reverse,
            ),
            threat_probe,
        );
        if let Err(error) = result {
            crate::v7_move_execution::restore_mistake_context(state, &context);
            return Err(error);
        }
        let captures = result?;
        let outcome = crate::v7_move_execution::complete_mistake_reversal(state, context)?;
        crate::transition::finish_stationary_move(state, outcome)?;
        return Ok(captures);
    }
    let options = CaptureOptions {
        force_capture: target.flag("mistakeReverse"),
        allow_jester: target.flag("mistakeReverse")
            || moving.kind != "jester"
                && (truth(moving.extra.get("crownBearer"))
                    || crate::v7_board_hazards::source_royal_identity(state, &moving)?),
        attacker_landing: Some(prelude.destination),
        defer_notation: true,
        threat_probe,
        saturation_locked: Some(crate::v7_capture_reactions::saturation_locked(
            state, &moving,
        )),
        ..Default::default()
    };
    if let Some(captures) =
        stationary_defense(state, &mut moving, from, &prelude, target, &start, &options)?
    {
        return Ok(captures);
    }
    if siege_replay.is_none() {
        crate::replay::begin_move(state, actor)?;
    }
    let legal = if state.flag("quantumPending", actor) {
        crate::movement::v7_legal_move_targets(
            state,
            &moving,
            from,
            crate::movement::V7MoveOptions::default(),
        )?
    } else {
        Vec::new()
    };
    let quantum_candidates = crate::v7_quantum_state::candidate_moves_for_move(
        state,
        &moving,
        prelude.destination,
        &legal,
    )?;
    crate::v7_move_execution::push_mad_knight_undo_before_move(state, &moving)?;
    let mut ledger = CaptureLedger {
        captured_something: prelude.quantum_observation.captured_illusion()
            || siege_captured_something,
        captured_outside_jump: prelude.quantum_observation.captured_illusion(),
        captures: siege_captures,
        ..Default::default()
    };
    let mut capture_targets = Vec::new();
    if let Some((at, victim)) = prelude
        .portal_entry
        .zip(prelude.portal_entry_target.as_ref())
    {
        capture_targets.push((at, victim.clone(), prelude.mad_horse_entry_capture, true));
    }
    if let Some(at) = en_passant_victim(target)?
        && let Some(victim) = state.at(at).cloned()
    {
        capture_targets.push((at, victim, false, false));
    }
    if let Some(victim) = &prelude.landing_target {
        capture_targets.push((
            prelude.destination,
            victim.clone(),
            prelude.mad_horse_exit_capture,
            true,
        ));
    }
    for (at, previous, mad_horse, cancel_on_failure) in capture_targets {
        let captured = crate::v7_capture_reactions::capture_at(state, &mut moving, at, &options)?;
        if let Some(captured) = captured {
            if mad_horse && captured.color == moving.color {
                ledger.captured_mad_horse_friendly = true;
            }
            ledger.remember(captured, at, false);
        } else if cancel_on_failure && state.at(at).is_some_and(|victim| victim.id == previous.id) {
            cancel_selection(state);
            return Ok(ledger.captures);
        }
    }
    if let Some(at) = descriptor_square(target, "jumpCapture")?
        && let Some(mut jumped) = state.at(at).cloned()
    {
        let allowed = if target.flag("checkerCapture") {
            crate::movement::v7_checker_capture_target_allowed(state, &moving, &jumped, at)?
        } else {
            crate::movement::v7_radical_charge_capture_target_allowed(state, &moving, &jumped, at)?
        };
        if allowed && truth(jumped.extra.get("shielded")) {
            crate::v7_capture_reactions::break_initiative_by_attack(state, &jumped, actor)?;
            crate::v7_capture_reactions::break_shield(state, &mut jumped, actor)?;
            ledger.checker_jumped_attack = target.flag("checkerCapture");
            crate::replay::add_piece_action_log(
                state,
                &moving,
                Some(prelude.destination),
                Some(&prelude.privacy),
                format!(
                    "{} {}의 가호를 벗겼습니다.",
                    if target.flag("checkerCapture") {
                        "체커가"
                    } else {
                        "난폭한 돌진이"
                    },
                    square_name(at)
                ),
            )?;
        } else if allowed && crate::v7_capture_reactions::is_hp_piece(state, &jumped) {
            crate::v7_capture_reactions::break_initiative_by_attack(state, &jumped, actor)?;
            let hits = if target.flag("checkerCapture") {
                1
            } else {
                crate::observation::number(target.flags.get("jumpCaptureHits"))
                    .filter(|number| *number != 0.0)
                    .unwrap_or(1.0)
                    .clamp(1.0, 2.0)
                    .ceil() as usize
            };
            for _ in 0..hits {
                if let Some(captured) =
                    crate::v7_capture_reactions::damage_health_piece_with_optional_attacker(
                        state,
                        at,
                        actor,
                        Some(&mut moving),
                        if target.flag("checkerCapture") {
                            "체커"
                        } else {
                            "난폭한 돌진"
                        },
                        &options,
                    )?
                {
                    // Source HP jump damage does not add this victim to the
                    // ordinary explosive-square queue.
                    ledger.remember(captured, at, true);
                    ledger.explosions.retain(|cell| *cell != at);
                    break;
                }
            }
            ledger.checker_jumped_attack = target.flag("checkerCapture");
        } else if allowed
            && let Some(captured) =
                crate::v7_capture_reactions::capture_at(state, &mut moving, at, &options)?
        {
            ledger.remember(captured, at, true);
            ledger.checker_jumped_attack = target.flag("checkerCapture");
        }
    }
    let notation_to = prelude.destination;
    let known = ledger.known_squares();
    let atomic = crate::v7_promotion::atomic_base_promotion_type_v7(target);
    if truth(moving.extra.get("pendingReaperDefeat")) {
        if !target.flag("mistakeReverse") {
            crate::replay::queue_v7_move_notation_with_options(
                state,
                &moving,
                from,
                notation_to,
                target,
                &crate::replay::V7MoveNotationOptions {
                    piece_type: &start.moved_as_type,
                    disambiguation: &disambiguation,
                    promotion: None,
                    privacy: &prelude.privacy,
                    capture: ledger.captured_outside_jump,
                    capture_known_squares: &known,
                    game_end: false,
                },
            )?;
        }
        crate::replay::queue_jump_capture_notation_v7(
            state,
            &moving,
            target,
            ledger.jump.as_ref(),
            &prelude.privacy,
        )?;
        crate::transition::finalize_immediate_reaper_execution(
            state,
            &mut moving,
            from,
            &start.moved_as_type,
        )?;
        return Ok(ledger.captures);
    }
    let reaper_to = if moving.kind == "reaper" {
        descriptor_square(
            &MoveTarget {
                row: target.row,
                col: target.col,
                flags: moving.extra.clone(),
            },
            "reaperExecutionTarget",
        )?
    } else {
        None
    };
    let to = reaper_to.unwrap_or(prelude.destination);
    delete_property(&mut moving, "reaperExecutionTarget");
    state.board[to.row as usize][to.col as usize] = Some(moving.clone());
    state.board[from.row as usize][from.col as usize] = None;
    if let Some(context) = state.active_v7_move_context.as_mut() {
        crate::v7_move_execution::note_roller_arrival(context, from, to);
    }
    let effects = V7MovePieceEffectsContext {
        from,
        landing: to,
        start: &start,
        direct_captured_piece: ledger.direct.as_ref(),
        jump_captured_piece: ledger.jump.as_ref(),
        siege_ram_chameleon_victim: siege_chameleon_victim.as_ref(),
        captured_something: ledger.captured_something,
        slime_move: target.flag("slimeMove"),
        switcheroo_move: prelude.switcheroo,
        portal_entry: prelude.portal_entry,
        portal_exit: prelude.portal_exit,
        privacy: Some(&prelude.privacy),
    };
    crate::v7_move_piece_effects::after_placement_before_last_move(
        state,
        &mut moving,
        &effects,
        V7PlacementPhase::BeforeLandingPromotion,
    )?;
    let mut promoted = atomic.is_some();
    if let Some(kind) = atomic {
        crate::v7_promotion::apply_atomic_landing_promotion_v7(
            state,
            to,
            kind,
            Some(&prelude.privacy),
        )?;
        moving = live(state, &moving.id, to).ok_or_else(|| {
            EngineError::InvalidState("atomic promotion lost mover identity".into())
        })?;
    }
    let placement = crate::v7_move_piece_effects::after_placement_before_last_move(
        state,
        &mut moving,
        &effects,
        V7PlacementPhase::AfterLandingPromotion,
    )?;
    // Spy promotion changes this object's color without changing state.turn.
    // Later source callbacks read moving.color rather than the initial actor.
    actor = moving.color.owner().ok_or(EngineError::WrongActor)?;
    let black_hole_death_move =
        !ledger.captured_something && crate::movement::v7_black_hole_cells(state)?.contains(&to);
    let sound = if ledger.captured_something || black_hole_death_move {
        "capture"
    } else if actor == Color::White {
        "moveSelf"
    } else {
        "moveOpponent"
    };
    let medium = state
        .active_v7_move_context
        .as_ref()
        .and_then(crate::v7_move_execution::medium_move_snapshot)
        .map(|(id, memory)| (id.to_owned(), memory.clone()));
    let highlight_hidden =
        crate::v7_move_execution::hidden_from_for_move(state, &moving, to, &prelude.privacy)?;
    crate::card_effects::set_last_move_with_medium_memory(
        state,
        crate::card_effects::LastMoveContext {
            from,
            to,
            sound_name: sound,
            sound_color: actor,
            hidden_from: highlight_hidden,
            moved_override: Some(&moving),
            original_medium: medium.as_ref().map(|(id, memory)| (id.as_str(), memory)),
        },
    )?;
    if !target.flag("mistakeReverse") {
        let game_end = state.mode == "gameover"
            && (defeat_royal(state, ledger.direct.as_ref())?
                || defeat_royal(state, ledger.jump.as_ref())?
                || reaper_to.is_some());
        crate::replay::queue_v7_move_notation_with_options(
            state,
            &moving,
            from,
            notation_to,
            target,
            &crate::replay::V7MoveNotationOptions {
                piece_type: &start.moved_as_type,
                disambiguation: &disambiguation,
                promotion: atomic.or(if placement.transformed_into_crown {
                    Some("crown")
                } else {
                    None
                }),
                privacy: &prelude.privacy,
                capture: ledger.captured_outside_jump || black_hole_death_move,
                capture_known_squares: &known,
                game_end,
            },
        )?;
    }
    crate::replay::queue_jump_capture_notation_v7(
        state,
        &moving,
        target,
        ledger.jump.as_ref(),
        &prelude.privacy,
    )?;
    crate::v7_board_hazards::flush_reaper_execution_notations(state, &mut moving)?;
    crate::v7_quantum_state::clear_quantum_for_piece_in_place(state, &mut moving)?;
    let quantum_to = crate::v7_quantum_state::apply_after_move_in_place(
        state,
        &mut moving,
        to,
        if reaper_to.is_some() {
            &[]
        } else {
            &quantum_candidates
        },
    )?;
    if let Some(ghost) = quantum_to
        && let Some(last) = state.extra.get_mut("lastMove")
    {
        last["quantumTo"] = json!(ghost);
    }
    let hidden = state
        .extra
        .get("lastMove")
        .and_then(|value| value.get("hiddenFrom"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut endpoints = vec![from, to];
    if let Some(ghost) = quantum_to {
        endpoints.push(ghost);
    }
    crate::card_effects::track_acceleration_trail(
        state,
        actor,
        &endpoints,
        truth(moving.extra.get("repositionSecondMove")),
        &hidden,
    )?;
    if truth(moving.extra.get("repositionSecondMove"))
        && let Some(trail) = state
            .extra
            .get_mut("accelerationTrail")
            .filter(|trail| trail.get("color") == Some(&json!(actor)))
    {
        trail["clearOnTurnStart"] = json!(actor);
    }
    crate::card_effects::mark_animation(state, &moving)?;
    if moving.kind == "crown" && ledger.captured_something {
        crate::v7_threat::play_crown_capture_sound_v7(state, actor)?;
    } else {
        crate::v7_threat::play_move_sound_v7(state, sound, actor)?;
    }
    crate::v7_rule_bombs::resolve_under_pieces(state, actor, false)?;
    if state.mode == "gameover" {
        crate::replay::record(state, "gameover")?;
        return Ok(ledger.captures);
    }
    if let Some(current) = live(state, &moving.id, to) {
        moving = current;
    } else {
        finish_removed_mover(
            state,
            &mut moving,
            &start,
            from,
            to,
            ledger.captured_something,
        )?;
        return Ok(ledger.captures);
    }
    crate::v7_capture_reactions::resolve_pending_feudal_strike(state, &mut moving, to)?;
    crate::v7_capture_reactions::resolve_pending_trojan_horse_retaliations(
        state,
        Some(actor),
        Some(&moving.id),
    )?;
    if let Some(current) = live(state, &moving.id, to) {
        moving = current;
    } else {
        finish_removed_mover(
            state,
            &mut moving,
            &start,
            from,
            to,
            ledger.captured_something,
        )?;
        return Ok(ledger.captures);
    }
    let surviving =
        crate::v7_move_piece_effects::after_surviving_move(state, &mut moving, &effects)?;
    if !target.flag("mistakeReverse") {
        let message = piece_move_log(state, &moving, from, to, &prelude.privacy)?;
        crate::replay::add_log(state, message)?;
    }
    if let Some((entry, exit)) = prelude.portal_entry.zip(prelude.portal_exit)
        && hidden.is_empty()
    {
        crate::replay::add_log(
            state,
            format!(
                "포탈: {}에서 {}로 {}했습니다.",
                square_name(entry),
                square_name(exit),
                if target.flag("portalThrough") {
                    "관통"
                } else {
                    "이동"
                }
            ),
        )?;
    }
    if prelude.switcheroo {
        crate::replay::add_piece_action_log(
            state,
            &moving,
            Some(to),
            Some(&prelude.privacy),
            format!(
                "바꿔치기: {}의 폰을 제거하고 킹이 이동했습니다.",
                square_name(to)
            ),
        )?;
    }
    let en_passant = next_en_passant(&moving, from, to, target, &start.moved_as_type)?;
    state.en_passant = en_passant.or_else(|| {
        state
            .en_passant
            .as_ref()
            .filter(|right| right.color == actor)
            .cloned()
    });
    crate::transition::refresh_submerged_with_options(state, true)?;
    let mut exploded = BTreeSet::new();
    for at in &ledger.explosions {
        if !exploded.insert(*at) {
            continue;
        }
        crate::v7_board_hazards::explode_at(state, *at, "자폭병")?;
        if state.mode == "gameover" {
            crate::replay::record(state, "gameover")?;
            return Ok(ledger.captures);
        }
        if let Some(current) = live(state, &moving.id, to) {
            moving = current;
        } else {
            finish_removed_mover(
                state,
                &mut moving,
                &start,
                from,
                to,
                ledger.captured_something,
            )?;
            return Ok(ledger.captures);
        }
    }
    match crate::v7_promotion::resolve_landing_promotion_v7(
        state,
        to,
        &start.moved_as_type,
        Some(&prelude.privacy),
    )? {
        crate::v7_promotion::V7LandingPromotionControl::Window => return Ok(ledger.captures),
        crate::v7_promotion::V7LandingPromotionControl::Continued { promoted: late } => {
            promoted |= late
        }
    }
    moving = live(state, &moving.id, to)
        .ok_or_else(|| EngineError::InvalidState("landing promotion lost mover identity".into()))?;
    actor = moving.color.owner().ok_or(EngineError::WrongActor)?;
    if surviving.was_charge_rush_pawn_move
        && moving.kind == "pawn"
        && crate::v7_piece_lifecycle::resolve_last_sprint_failure(state, &moving, to)?
    {
        crate::transition::end_move_for_decision(state, actor, true, Some("move"))?;
        return Ok(ledger.captures);
    }
    let transcendence = if ledger.captured_by_moving_piece && !promoted {
        crate::card_effects::apply_transcendence_capture_upgrade_v7(
            state,
            &mut moving,
            to,
            &start.moved_as_type,
        )?
    } else {
        None
    };
    if transcendence.is_none() {
        crate::v7_move_piece_effects::transform_chimera_after_move_v7(
            state,
            &mut moving,
            to,
            Some(&prelude.privacy),
        )?;
    }
    crate::v7_move_piece_effects::reconcile_after_type_change(
        state,
        &mut moving,
        &start.moved_as_type,
        promoted,
    )?;
    crate::v7_threat::resolve_herald_threats_v7(state, actor)?;
    crate::v7_threat::check_racing_kings_v7(state)?;
    crate::v7_turn_entry::check_conscription(state, Color::White)?;
    crate::v7_turn_entry::check_conscription(state, Color::Black)?;
    if state.mode == "gameover" {
        crate::replay::record(state, "gameover")?;
        return Ok(ledger.captures);
    }
    let continuation = V7MoveContinuationInput {
        piece_id: moving.id.clone(),
        from,
        landing: to,
        start,
        captured_something: ledger.captured_something,
        direct_captured_piece: ledger.direct.clone(),
        internal_thief_jump,
        mad_horse_friendly_capture: prelude.mad_horse_entry_capture
            || prelude.mad_horse_exit_capture,
        captured_mad_horse_friendly: ledger.captured_mad_horse_friendly,
        checker_jumped_attack: ledger.checker_jumped_attack,
        checker_capture: target.flag("checkerCapture"),
        chameleon_transformed: surviving.chameleon_transformed,
    };
    let control = crate::v7_move_continuations::before_middle_callbacks_v7(state, &continuation)?;
    if control != V7ContinuationControl::Continue {
        finish_control(state, actor, control)?;
        return Ok(ledger.captures);
    }
    crate::v7_threat::break_initiative_by_check_v7(state, actor)?;
    if state.free_move_resolution != Some(actor)
        && crate::v7_campaign::handle_knight_journey_moved(state, &moving, from, to)?
    {
        return Ok(ledger.captures);
    }
    let control = crate::v7_move_continuations::after_middle_callbacks_v7(state, &continuation)?;
    finish_control(state, actor, control)?;
    Ok(ledger.captures)
}
