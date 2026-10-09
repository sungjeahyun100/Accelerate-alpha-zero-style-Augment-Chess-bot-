pub(crate) use crate::v7_card_context::{begin_v7_card, finish_v7_card};
pub(crate) use crate::v7_rule_bombs::cancel_prophecies as cancel_prophecies_by_capture;
use crate::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
#[path = "tests/v7_terminal_royal_capture.rs"]
mod v7_terminal_royal_capture;

pub(crate) fn card_actions(state: &GameState, card: &CardSlot) -> Result<Vec<Action>> {
    if state.ruleset_id == RULES_VERSION_V7 {
        match crate::card_registry::action_policy(state, card) {
            Ok(_) => {}
            Err(EngineError::IllegalAction) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        }
    }
    let candidates = card_ui_actions(state, card)?;
    let mut accepted = Vec::new();
    for action in candidates {
        if validate_card_action(state, card, &action)? {
            accepted.push(action);
        }
    }
    Ok(accepted)
}
pub(crate) fn validate_card_action(
    state: &GameState,
    card: &CardSlot,
    action: &Action,
) -> Result<bool> {
    if state.ruleset_id == RULES_VERSION_V7 {
        match crate::card_registry::action_policy(state, card) {
            Ok(_) => {}
            Err(EngineError::IllegalAction) => return Ok(false),
            Err(error) => return Err(error),
        }
    }
    if let Some(accepted) = crate::card_effects::validate(state, card, action)? {
        return Ok(accepted);
    }
    Ok(card_ui_actions(state, card)?.contains(action))
}
pub(crate) fn card_ui_actions(state: &GameState, card: &CardSlot) -> Result<Vec<Action>> {
    if let Some(actions) = crate::card_effects::actions(state, card)? {
        return Ok(actions);
    }
    let color = state.turn;
    match card.effect.as_str() {
        "genevaConvention" | "cornerKick" | "retreat" | "otherworld" | "enPassantBang" => {
            if state.flag(&card.effect, color) {
                return Ok(Vec::new());
            }
            if card.effect == "cornerKick"
                && !state
                    .board
                    .iter()
                    .flatten()
                    .flatten()
                    .any(|p| p.color == color && p.kind == "knight")
            {
                return Ok(Vec::new());
            }
            if matches!(card.effect.as_str(), "otherworld" | "enPassantBang")
                && !state
                    .board
                    .iter()
                    .flatten()
                    .flatten()
                    .any(|piece| piece.color == color && piece.kind == "pawn")
            {
                return Ok(Vec::new());
            }
            Ok(vec![Action::card(color, card, None)])
        }
        "conversion" => {
            if state
                .board
                .iter()
                .flatten()
                .flatten()
                .any(|p| p.color == color && p.kind == "knight")
            {
                Ok(vec![Action::card(color, card, None)])
            } else {
                Ok(Vec::new())
            }
        }
        "reversal" => {
            if state.flag("reversal", color) {
                return Ok(Vec::new());
            }
            let mut actions = Vec::new();
            for row in 0..8 {
                for col in 0..8 {
                    if state.at(Square { row, col }).is_some_and(|p| {
                        p.color == color
                            && matches!(
                                p.kind.as_str(),
                                "knight"
                                    | "bishop"
                                    | "camel"
                                    | "alfil"
                                    | "ferz"
                                    | "man"
                                    | "guard"
                                    | "knightmaster"
                            )
                    }) {
                        actions.push(Action::card(
                            color,
                            card,
                            Some(json!({"row":row,"col":col})),
                        ));
                    }
                }
            }
            Ok(actions)
        }
        _ => Err(EngineError::UnsupportedFeature(format!(
            "card {}",
            card.effect
        ))),
    }
}

/// Draft acquisition uses the same effect kernel without treating automatic
/// passives as a player action. Returning false is the client's rejected effect
/// (for example no remaining pawn), rather than a successful placeholder.
pub(crate) fn apply_draft_passive(
    state: &mut GameState,
    color: Color,
    slot: usize,
) -> Result<bool> {
    let card = state
        .deck_slots
        .get(color)
        .get(slot)
        .ok_or(EngineError::IllegalAction)?
        .clone();
    if card.vacant
        || card.used
        || card
            .extra
            .get("nextTurnPending")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Ok(false);
    }
    if [Color::White, Color::Black]
        .into_iter()
        .any(|side| state.flag("clonePassive", side))
    {
        return Err(EngineError::UnsupportedFeature(
            "sharing acquired passive with clone recipients".into(),
        ));
    }
    let has = |kind: &str| {
        state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.color == color && piece.kind == kind)
    };
    let effect = card.effect.as_str();
    let needed = match effect {
        "retreat" | "pawnSprint" | "pawnLeap" => Some("pawn"),
        "cornerKick" | "radicalCharge" => Some("knight"),
        "fianchetto" => Some("bishop"),
        _ => None,
    };
    if needed.is_some_and(|kind| !has(kind)) || effect == "cornerKick" && state.flag(effect, color)
    {
        return Ok(false);
    }
    match effect {
        "democracy" => {
            if !apply_democracy(state, color)? {
                return Ok(false);
            }
        }
        "genevaConvention" | "retreat" | "cornerKick" | "pawnSprint" | "pawnLeap"
        | "fianchetto" | "rookLift" | "backwardKnight" | "fileSurge" | "earlyPromotion"
        | "fastGrowth" | "underpromotion" | "finalWeapon" | "religiousVictory" | "binaMate"
        | "overwhelm" | "radicalCharge" | "madHorse" | "coronation" | "majesty"
        | "infiltration" | "killerKing" | "resolve" | "vanguard" => {
            state.set_flag(effect, color, true)
        }
        "kingOfTheHill" => state.set_flag("hillKing", color, true),
        "queenAfterimage" => state.set_flag("afterimageQueen", color, true),
        "injury" => state.set_flag("knightInjury", color.opponent(), true),
        "d4" | "e4" | "solidarity" | "bishopInfiltration" | "synchronization" | "assembly"
        | "vigilance" | "roller" => {
            if state.flag(effect, color) {
                return Ok(false);
            }
            let entry = state
                .extra
                .entry(effect.to_owned())
                .or_insert_with(|| json!({}));
            let sides = entry.as_object_mut().ok_or_else(|| {
                EngineError::InvalidState(format!("{effect} must be a player map"))
            })?;
            sides.insert(
                color.as_str().to_owned(),
                if effect == "bishopInfiltration" {
                    json!(3)
                } else {
                    json!(true)
                },
            );
        }
        "conversion" => {
            if !has("knight") {
                return Ok(false);
            }
            let lock = state
                .turns_taken
                .get(color)
                .checked_add(1)
                .ok_or_else(|| EngineError::InvalidState("capture lock overflow".into()))?;
            for (row, cells) in state.board.iter_mut().enumerate() {
                for (col, piece) in cells.iter_mut().enumerate() {
                    let Some(piece) = piece.as_mut() else {
                        continue;
                    };
                    if piece.color == color && piece.kind == "knight" {
                        piece.kind = "bishop".into();
                        piece.moved = true;
                        piece.extra.insert(
                            "origin".into(),
                            json!(format!("{}{}", char::from(b'a' + col as u8), 8 - row)),
                        );
                        piece
                            .extra
                            .insert("freshNoCaptureUntil".into(), json!(lock));
                        piece.extra.shift_remove("vipInvitation");
                        piece.extra.shift_remove("holdoutPromotion");
                    }
                }
            }
        }
        _ => match crate::opening::apply(state, color, effect)? {
            Some(true) => {}
            Some(false) => return Ok(false),
            None => {
                return Err(EngineError::UnsupportedFeature(format!(
                    "acquired passive {effect}"
                )));
            }
        },
    }
    for side in [Color::White, Color::Black] {
        if state.flag("religiousVictory", side) {
            let own = state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|p| p.color == side && p.kind == "bishop")
                .count();
            let other = state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|p| p.color == side.opponent() && p.kind == "bishop")
                .count();
            if own >= other + 3 {
                state.mode = "gameover".into();
                state.winner = Some(side.as_str().into());
                state.extra.insert(
                    "replayEndReason".into(),
                    json!("종교 승리: 비숍이 상대방보다 3개 더 많습니다."),
                );
                break;
            }
        }
    }
    let used_at = crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?;
    let stored = &mut state.deck_slots.get_mut(color)[slot];
    stored.used = true;
    stored.recovering = false;
    stored.source_order.retain(|name| name != "recovering");
    stored.extra.insert("passiveApplied".into(), json!(true));
    stored.extra.insert("usedAt".into(), json!(used_at));
    crate::replay::add_log(
        state,
        format!(
            "{} 패시브: {}",
            crate::replay::label(color),
            card.extra
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("undefined")
        ),
    )?;
    Ok(true)
}

/// The source normalizes both player flags and requires a physical pawn;
/// pending recurrence is counted for survival, but not for initial activation.
fn apply_democracy(state: &mut GameState, color: Color) -> Result<bool> {
    if state.flag("democracy", color)
        || !state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.color == color && piece.kind == "pawn")
    {
        return Ok(false);
    }
    state.extra.insert("democracy".into(), json!({"white":state.flag("democracy",Color::White),"black":state.flag("democracy",Color::Black)}));
    state.set_flag("democracy", color, true);
    Ok(true)
}

/// 원문의 `apply(..., {recordHistory:false})`처럼 공개 event 없이 같은 규칙을 실행한다.
/// 호출자는 사본을 소유해야 한다. 원문 board replay와 RNG는 정상 실행과 동일하게 정산한다.
pub(crate) fn apply_without_public_event(
    state: &mut GameState,
    action: &Action,
) -> Result<Vec<Piece>> {
    let captures = match action.kind {
        ActionKind::Move => apply_move(state, action, false)?,
        ActionKind::Card => {
            crate::legal_profile::measure("card_effect_apply", || apply_card(state, action))?
        }
        ActionKind::Promotion if state.ruleset_id == RULES_VERSION_V7 => {
            crate::v7_promotion::start_deferred_promotion_v7(state, action)?;
            Vec::new()
        }
        ActionKind::PromotionChoice => apply_promotion(state, action)?,
        kind if state.ruleset_id == RULES_VERSION_V7 && crate::v7_decision_actions::owns(kind) => {
            crate::v7_decision_actions::apply(state, action)?
        }
        ActionKind::DraftPick | ActionKind::DraftBundlePick => {
            crate::draft::apply_pick(state, action)?
        }
        other => return Err(EngineError::UnsupportedFeature(format!("action {other:?}"))),
    };
    prune_board_potion_effects(state)?;
    crate::replay::settle(state)?;
    Ok(captures)
}

pub(crate) fn apply(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    let actor = action.color;
    let before_v7 = (state.ruleset_id == RULES_VERSION_V7).then(|| state.clone());
    let before = if before_v7.is_some() {
        None
    } else {
        Some(Sides {
            white: observe_for_public_event(state, Color::White)?,
            black: observe_for_public_event(state, Color::Black)?,
            white_first: true,
        })
    };
    let captures = apply_without_public_event(state, action)?;
    if let Some(before) = before_v7 {
        let mut source_action = action.clone();
        source_action.position_key = None;
        crate::v7_replay::append_event(&before, state, &source_action)?;
        return Ok(captures);
    }
    let before = before.expect("v6 transition has a prior public observation");
    let transition = |viewer| -> Result<PublicTransition> {
        let after = observe_for_public_event(state, viewer)?;
        let before = before.get(viewer);
        let mut board_changes = Vec::new();
        if before.board.len() != after.board.len()
            || before
                .board
                .iter()
                .zip(&after.board)
                .any(|(old, new)| old.len() != new.len())
        {
            return Err(EngineError::UnsupportedFeature(
                "public transition across a changed board extent".into(),
            ));
        }
        for row in 0..before.board.len() {
            for col in 0..before.board[row].len() {
                if before.board[row][col] != after.board[row][col] {
                    board_changes.push(BoardChange {
                        square: Square {
                            row: u8::try_from(row).map_err(|_| {
                                EngineError::UnsupportedFeature(
                                    "public transition row exceeds wire coordinate".into(),
                                )
                            })?,
                            col: u8::try_from(col).map_err(|_| {
                                EngineError::UnsupportedFeature(
                                    "public transition column exceeds wire coordinate".into(),
                                )
                            })?,
                        },
                        before: before.board[row][col].clone(),
                        after: after.board[row][col].clone(),
                    });
                }
            }
        }
        let winner = state
            .result()
            .as_ref()
            .map(|result| serde_json::to_value(result).expect("result serializes"));
        let result = json!({"protocolVersion":"accelerate-result-v1","status":if winner.is_some(){"terminal"}else{"ongoing"},"winner":if matches!(state.result(),Some(GameResult::White|GameResult::Black)){json!(state.winner)}else{Value::Null},"outcome":winner,"reason":if state.result().is_some(){state.extra.get("replayEndReason").cloned().unwrap_or(json!(""))}else{json!("")}});
        Ok(PublicTransition {
            kind: "transition".into(),
            actor,
            next_actor: state.decision_actor(),
            phase: state.mode.clone(),
            board_changes,
            own_cards: after.own_cards,
            revealed_opponent_cards: after
                .public_state
                .get("revealedOpponentCards")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            captures: state.public_captures(),
            result,
        })
    };
    let mut exact = action.clone();
    exact.position_key = None;
    let event = PublicEvent {
        protocol_version: "accelerate-game-event-v1".into(),
        actor,
        action: exact,
        turn_changed: state.turn != before.white.turn,
        public: Sides {
            white: transition(Color::White)?,
            black: transition(Color::Black)?,
            white_first: true,
        },
    };
    state
        .history
        .push(serde_json::to_value(event).expect("game event serializes"));
    Ok(captures)
}

/// PublicEvent records board/card/result changes. The source's active-play
/// highlight calculation is a separate viewer API and is not an input to this
/// transition. In v7, keep that still-unported hint surface fail-closed while
/// allowing validated internal projections to record draft-to-play events.
fn observe_for_public_event(state: &GameState, viewer: Color) -> Result<Observation> {
    if state.ruleset_id == RULES_VERSION_V7 {
        let observation = state.observe_checked(viewer)?;
        crate::observation::validate_projection_for_ruleset(&observation, &state.ruleset_id)?;
        Ok(observation)
    } else {
        state.try_observe(viewer)
    }
}

pub(crate) fn execute_threat_move(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    let previous = state.threat_probe_depth;
    state.threat_probe_depth = previous
        .checked_add(1)
        .ok_or_else(|| EngineError::InvalidState("v7 threat probe depth overflow".into()))?;
    let result =
        crate::v7_card_context::with_ai_simulation(state, |state| apply_move(state, action, true));
    state.threat_probe_depth = previous;
    result
}

/// 예약 Free Move는 endMove의 반응을 실행하지만 보통 턴의 count/credit를 소비하지 않는다.
pub(crate) fn apply_v7_free_move(
    state: &mut GameState,
    actor: Color,
    from: Square,
    target: MoveTarget,
) -> Result<bool> {
    apply_v7_free_move_with_context(state, actor, from, target, false)
}

/// 원문 Don Quixote 자동 이동의 transient marker와 기보를 보존한다.
pub(crate) fn apply_v7_don_quixote_free_move(
    state: &mut GameState,
    actor: Color,
    from: Square,
    target: MoveTarget,
) -> Result<bool> {
    apply_v7_free_move_with_context(state, actor, from, target, true)
}

fn apply_v7_free_move_with_context(
    state: &mut GameState,
    actor: Color,
    from: Square,
    target: MoveTarget,
    don_quixote: bool,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 || state.turn != actor {
        return Err(EngineError::WrongActor);
    }
    let mut working = state.clone();
    let previous = working.free_move_resolution;
    let previous_don_quixote = working.free_move_don_quixote;
    working.free_move_resolution = Some(actor);
    working.free_move_don_quixote = don_quixote;
    let action = Action::movement(actor, from, target);
    apply_move(&mut working, &action, false)?;
    working.free_move_resolution = previous;
    working.free_move_don_quixote = previous_don_quixote;
    *state = working;
    Ok(true)
}

fn apply_move(state: &mut GameState, action: &Action, threat_probe: bool) -> Result<Vec<Piece>> {
    if state.ruleset_id == RULES_VERSION_V7 {
        return crate::v7_move_transition::execute(state, action, threat_probe);
    }
    let from = action.from.ok_or(EngineError::IllegalAction)?;
    let target = action
        .destination
        .as_ref()
        .ok_or(EngineError::IllegalAction)?;
    let mut to = target.square();
    if from.row >= 8 || from.col >= 8 || to.row >= 8 || to.col >= 8 {
        return Err(EngineError::IllegalAction);
    }
    let mut piece = state.at(from).cloned().ok_or(EngineError::IllegalAction)?;
    let original_piece = piece.clone();
    if state.ruleset_id == RULES_VERSION_V7 {
        if let Some(outcome) = crate::v7_move_execution::execute_log_direction(state, from, target)?
        {
            return finish_stationary_move(state, outcome);
        }
        if let Some(outcome) =
            crate::v7_move_execution::prepare_shotgun_selection(state, from, target)?
        {
            return finish_stationary_move(state, outcome);
        }
    }
    // movePiece stores this immediately before changing the first board move.
    // The automatic OPENING card may restore only these fields if that move
    // made its effect impossible. It deliberately leaves clock, RNG and move
    // bookkeeping at their post-move values.
    if !threat_probe && should_store_first_move_undo(state, action.color)? {
        let undo = capture_first_move_undo(state, action.color);
        state.extra.insert("firstMoveUndo".into(), undo);
    }
    if state.ruleset_id == RULES_VERSION_V7
        && let Some(outcome) =
            crate::v7_move_execution::execute_shotgun(state, from, target, threat_probe)?
    {
        return finish_stationary_move(state, outcome);
    }
    let replay_before = crate::replay::begin_move(state, action.color)?;
    let actor = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let original_type = piece.kind.clone();
    let mut captures = Vec::new();
    let capture_options = crate::v7_capture_reactions::CaptureOptions {
        attacker_landing: Some(to),
        defer_notation: true,
        threat_probe,
        allow_jester: piece.kind != "jester"
            && (piece.flag("crownBearer") || state.royal_identity(&piece)),
        ..Default::default()
    };
    if state.ruleset_id == RULES_VERSION_V7 && !piece.is_large() && !target.flag("castle") {
        let mut defense_squares = Vec::new();
        if let (Some(row), Some(col)) = (
            target.flags.get("capturedRow").and_then(Value::as_u64),
            target.flags.get("capturedCol").and_then(Value::as_u64),
        ) && target.flag("enPassant")
            && row < 8
            && col < 8
        {
            defense_squares.push(Square {
                row: row as u8,
                col: col as u8,
            });
        }
        defense_squares.push(to);
        for at in defense_squares {
            let Some(before_target) = state.at(at).cloned() else {
                continue;
            };
            if before_target.color == piece.color {
                continue;
            }
            let privacy = move_privacy_v7(state, &piece, from)?;
            let outcome = crate::v7_capture_reactions::attack_defended_piece(
                state,
                &mut piece,
                at,
                &format!("{}의 공격", crate::replay::piece_label(&original_type)),
                &capture_options,
            )?;
            if outcome == crate::v7_capture_reactions::DefendedAttack::NotDefended {
                continue;
            }
            let health = match outcome {
                crate::v7_capture_reactions::DefendedAttack::Health { ref removed } => {
                    if let Some(removed) = removed {
                        captures.push(removed.clone());
                    }
                    let victim = state
                        .at(at)
                        .cloned()
                        .or_else(|| removed.clone())
                        .unwrap_or(before_target.clone());
                    crate::replay::queue_hp_attack_notation(
                        state, &piece, from, &victim, at, &privacy,
                    )?;
                    if removed.is_some() {
                        finalize_immediate_reaper_execution(
                            state,
                            &mut piece,
                            from,
                            &original_type,
                        )?;
                    }
                    true
                }
                _ => false,
            };
            if !health {
                crate::replay::add_log(
                    state,
                    format!(
                        "{}{}의 방어막이 {}을 막았습니다.",
                        char::from(b'a' + at.col),
                        8 - at.row,
                        if target.flag("enPassant") {
                            "앙파상"
                        } else {
                            "공격"
                        }
                    ),
                )?;
            }
            clear_moving_extra_flags(&mut piece);
            piece
                .extra
                .insert("coolGuyCapturedLast".into(), json!(!captures.is_empty()));
            update_piece(state, &piece);
            state.en_passant = None;
            if state.mode != "gameover" {
                end_move_for_decision(state, actor, true, Some("move"))?;
            }
            return Ok(captures);
        }
    }
    // The v7 source marks an ordinary mover after its capture, replay sound,
    // and terminal-return boundary. A captured royal can end the game before
    // that stage, leaving the landed mover's original moved flag intact.
    let defer_regular_moved = state.ruleset_id == RULES_VERSION_V7
        && !target.flag("castle")
        && !target.flag("missionaryConvert")
        && !piece.is_large();
    if target.flag("castle") {
        let rook_from: Square = serde_json::from_value(
            target
                .flags
                .get("rookFrom")
                .cloned()
                .ok_or(EngineError::IllegalAction)?,
        )
        .map_err(EngineError::serialization)?;
        let rook_to: Square = serde_json::from_value(
            target
                .flags
                .get("rookTo")
                .cloned()
                .ok_or(EngineError::IllegalAction)?,
        )
        .map_err(EngineError::serialization)?;
        let mut rook = state
            .at(rook_from)
            .cloned()
            .ok_or(EngineError::IllegalAction)?;
        rook.moved = true;
        clear_piece(state, &rook.id);
        state.board[rook_to.row as usize][rook_to.col as usize] = Some(rook);
        state.set_flag("castled", actor, true);
    }
    if target.flag("enPassant") {
        let square = Square::new(
            target
                .flags
                .get("capturedRow")
                .and_then(Value::as_u64)
                .ok_or(EngineError::IllegalAction)? as u8,
            target
                .flags
                .get("capturedCol")
                .and_then(Value::as_u64)
                .ok_or(EngineError::IllegalAction)? as u8,
        )?;
        if let Some(victim) = state.at(square).cloned() {
            if state.ruleset_id == RULES_VERSION_V7 {
                if let Some(victim) = crate::v7_capture_reactions::capture_at(
                    state,
                    &mut piece,
                    square,
                    &capture_options,
                )? {
                    captures.push(victim);
                }
            } else {
                capture(state, &piece, victim, &mut captures)?;
            }
        }
    }
    if target.flag("checkerCapture") {
        let square: Square = serde_json::from_value(
            target
                .flags
                .get("jumpCapture")
                .cloned()
                .ok_or(EngineError::IllegalAction)?,
        )
        .map_err(EngineError::serialization)?;
        if let Some(victim) = state.at(square).cloned() {
            if state.ruleset_id == RULES_VERSION_V7 {
                if let Some(victim) = crate::v7_capture_reactions::capture_at(
                    state,
                    &mut piece,
                    square,
                    &capture_options,
                )? {
                    captures.push(victim);
                }
            } else {
                capture(state, &piece, victim, &mut captures)?;
            }
        }
    }
    if target.flag("missionaryConvert") {
        let mut converted = state.at(to).cloned().ok_or(EngineError::IllegalAction)?;
        converted.color = actor.into();
        converted.moved = true;
        state.board[to.row as usize][to.col as usize] = Some(converted);
        piece.moved = true;
        update_piece(state, &piece);
    } else if piece.is_large() {
        let cells = [
            to,
            Square {
                row: to.row + 1,
                col: to.col,
            },
            Square {
                row: to.row,
                col: to.col + 1,
            },
            Square {
                row: to.row + 1,
                col: to.col + 1,
            },
        ];
        let mut victims = BTreeSet::new();
        for cell in cells {
            if let Some(victim) = state.at(cell).cloned()
                && victim.id != piece.id
                && victims.insert(victim.id.clone())
            {
                if state.ruleset_id == RULES_VERSION_V7 {
                    let mut options = capture_options.clone();
                    options.attacker_landing_cells = Some(cells.to_vec());
                    if let Some(victim) =
                        crate::v7_capture_reactions::capture_at(state, &mut piece, cell, &options)?
                    {
                        captures.push(victim);
                    }
                } else {
                    capture(state, &piece, victim, &mut captures)?;
                }
            }
        }
        clear_piece(state, &piece.id);
        piece.moved = true;
        piece.extra.insert("anchorRow".into(), json!(to.row));
        piece.extra.insert("anchorCol".into(), json!(to.col));
        for cell in cells {
            state.board[cell.row as usize][cell.col as usize] = Some(piece.clone());
        }
    } else {
        if let Some(victim) = state.at(to).cloned() {
            if state.ruleset_id == RULES_VERSION_V7 {
                if let Some(victim) = crate::v7_capture_reactions::capture_at(
                    state,
                    &mut piece,
                    to,
                    &capture_options,
                )? {
                    captures.push(victim);
                }
            } else {
                capture(state, &piece, victim, &mut captures)?;
            }
        }
        if state.ruleset_id == RULES_VERSION_V7
            && crate::observation::truth(piece.extra.get("pendingReaperDefeat"))
        {
            if !threat_probe {
                crate::replay::queue_move(
                    state,
                    &replay_before,
                    &original_piece,
                    from,
                    to,
                    target,
                    !captures.is_empty(),
                )?;
            }
            finalize_immediate_reaper_execution(state, &mut piece, from, &original_type)?;
            return Ok(captures);
        }
        if state.ruleset_id == RULES_VERSION_V7
            && piece.kind == "reaper"
            && let Some(value) = piece.extra.shift_remove("reaperExecutionTarget")
        {
            let destination: Square =
                serde_json::from_value(value).map_err(EngineError::serialization)?;
            if destination.row >= 8 || destination.col >= 8 {
                return Err(EngineError::InvalidState(
                    "reaper execution landing outside 8x8 board".into(),
                ));
            }
            to = destination;
        }
        if state.at(to).is_some() {
            if state.ruleset_id == RULES_VERSION_V7 {
                state.extra.insert("selected".into(), Value::Null);
                state.extra.insert("legalMoves".into(), json!([]));
                return Ok(captures);
            }
            return Err(EngineError::UnsupportedFeature(
                "capture leaves occupied landing".into(),
            ));
        }
        clear_piece(state, &piece.id);
        if !defer_regular_moved {
            piece.moved = true;
        }
        if state.ruleset_id != RULES_VERSION_V7 {
            piece
                .extra
                .insert("coolGuyCapturedLast".into(), json!(!captures.is_empty()));
        }
        let mut visited = piece
            .extra
            .get("thiefVisited")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for square in [from, to] {
            let value = json!(format!("{},{}", square.row, square.col));
            if !visited.contains(&value) {
                visited.push(value);
            }
        }
        piece.extra.insert("thiefVisited".into(), json!(visited));
        piece.extra.insert(
            "thiefLastDirection".into(),
            json!(format!(
                "{},{}",
                (i16::from(to.row) - i16::from(from.row)).signum(),
                (i16::from(to.col) - i16::from(from.col)).signum()
            )),
        );
        if piece.kind == "checker" && to.row == actor.promotion_row() {
            piece.kind = "checkerKing".into();
        }
        if !captures.is_empty() {
            piece.extra.insert(
                "capturesMade".into(),
                json!(piece.number("capturesMade") + captures.len() as i64),
            );
            piece.extra.insert(
                "totalCaptures".into(),
                json!(piece.number("totalCaptures") + captures.len() as i64),
            );
        }
        state.board[to.row as usize][to.col as usize] = Some(piece.clone());
    }
    state.en_passant = None;
    if target.flag("standardPawnDoubleStep") {
        state.en_passant = Some(EnPassant {
            row: ((u16::from(from.row) + u16::from(to.row)) / 2) as u8,
            col: to.col,
            captured_row: to.row,
            captured_col: to.col,
            color: actor,
            extra: Fields::new(),
        });
    }
    state.extra.insert("parrotMovement".into(), {
        let mut memory = state
            .extra
            .get("parrotMovement")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        memory.insert(actor.as_str().into(), json!({"type":original_type}));
        Value::Object(memory)
    });
    crate::card_effects::mark_animation(state, &piece)?;
    state.extra.insert(
        "lastMove".into(),
        json!({"from":from,"to":to,"pieceId":piece.id,"pieceType":original_type,
            "soundName":if target.flag("castle"){"castle"}else if captures.is_empty(){if actor==Color::White {"moveSelf"} else {"moveOpponent"}}else{"capture"},
            "soundColor":actor,"hiddenFrom":piece.extra.get("hiddenFrom").and_then(Value::as_str).unwrap_or(""),
            "idolEncoreEligible":false,"idolEncoreId":"","idolEncorePieceId":"","idolEncoreConsumed":false}),
    );
    // queueMoveHistoryNotation creates its identifier before promotion and turn
    // settlement, sharing the source random stream with later rule draws.
    if !threat_probe {
        crate::replay::queue_move(
            state,
            &replay_before,
            &original_piece,
            from,
            to,
            target,
            !captures.is_empty(),
        )?;
    }
    if state.ruleset_id == RULES_VERSION_V7 {
        crate::v7_board_hazards::flush_reaper_execution_notations(state, &mut piece)?;
    }
    let mut replay_interrupted = false;
    if !threat_probe {
        let sound = if target.flag("castle") {
            "castle"
        } else if captures.is_empty() {
            if actor == Color::White {
                "moveSelf"
            } else {
                "moveOpponent"
            }
        } else {
            "capture"
        };
        replay_interrupted = if state.ruleset_id == RULES_VERSION_V7 {
            // The v7 threat probe simulates against a private clone. Source
            // commitMoveReplayCapture still runs after that sound callback.
            crate::v7_threat::play_move_sound_v7(state, sound, actor)?;
            false
        } else {
            crate::threat::play_move_sound(state, sound, actor)?
        };
    }
    if state.ruleset_id == RULES_VERSION_V7 {
        // Both ordinary moves and the source's child-state threat simulation
        // settle bombs after the sound boundary, before terminal early return.
        crate::v7_rule_bombs::resolve_under_pieces(state, actor, false)?;
    }
    // Source movePiece returns immediately after the move sound/under-piece
    // rule-bomb stage when that stage has already ended the game. In that
    // branch endMove never commits activeMoveReplayCapture.
    let terminal_after_move_sound =
        state.ruleset_id == RULES_VERSION_V7 && state.mode == "gameover";
    if state.ruleset_id == RULES_VERSION_V7 {
        if state.mode == "gameover" {
            return Ok(captures);
        }
        let Some(landed) = state.at(to).filter(|landed| landed.id == piece.id).cloned() else {
            end_move_for_decision(state, actor, true, Some("move"))?;
            return Ok(captures);
        };
        piece = landed;
        if !target.flag("missionaryConvert") {
            crate::v7_capture_reactions::resolve_pending_feudal_strike(state, &mut piece, to)?;
            crate::v7_capture_reactions::resolve_pending_trojan_horse_retaliations(
                state,
                Some(actor),
                Some(&piece.id),
            )?;
            if state.mode == "gameover" {
                return Ok(captures);
            }
            let Some(landed) = state.at(to).filter(|landed| landed.id == piece.id).cloned() else {
                end_move_for_decision(state, actor, true, Some("move"))?;
                return Ok(captures);
            };
            piece = landed;
        }
    }
    // main92263–92307: the threat-sound probe observes bindings before the
    // moving piece releases its allies through trackMovingProgress.
    if defer_regular_moved && state.mode != "gameover" {
        piece.moved = true;
        update_piece(state, &piece);
    }
    if state.ruleset_id == RULES_VERSION_V7
        && state.mode != "gameover"
        && !piece.is_large()
        && !target.flag("missionaryConvert")
    {
        // The source returns from a royal-capture gameover after landing and
        // replay sound, before assigning this transient capture marker.
        piece
            .extra
            .insert("coolGuyCapturedLast".into(), json!(!captures.is_empty()));
        update_piece(state, &piece);
    }
    if state.ruleset_id != RULES_VERSION_V7 || state.mode != "gameover" {
        crate::replay::track_moving(state, &piece)?;
    }
    if state.ruleset_id != RULES_VERSION_V7 || state.mode != "gameover" {
        // A captured royal ends the frozen v7 move after its terminal sound.
        // The source has already logged the winner and returns before the
        // ordinary piece-move log; replay finalization still follows below.
        crate::replay::add_log(
            state,
            format!(
                "{} {}: {}{} -> {}{}",
                crate::replay::label(actor),
                crate::replay::piece_label(&original_type),
                char::from(b'a' + from.col),
                8 - from.row,
                char::from(b'a' + to.col),
                8 - to.row
            ),
        )?;
    }
    if original_type == "pawn" || !captures.is_empty() {
        crate::flow::mark_progress(state);
    }
    if state.result().is_none()
        && matches!(piece.kind.as_str(), "pawn" | "squire" | "standardBearer")
        && to.row == actor.promotion_row()
        && !piece.flag("noPromotion")
    {
        let black_hidden = piece.extra.get("hiddenFrom").and_then(Value::as_str) == Some("black");
        let white_hidden = piece.extra.get("hiddenFrom").and_then(Value::as_str) == Some("white");
        crate::flow::pause_clock(state)?;
        state.extra.insert("pendingPromotion".into(),json!({"row":to.row,"col":to.col,"color":actor,"choices":["queen","rook","bishop","knight"],"privacy":{"white":{"originVisible":!white_hidden,"typeKnown":!white_hidden},"black":{"originVisible":!black_hidden,"typeKnown":!black_hidden}}}));
        return Ok(captures);
    }
    if state.result().is_none() {
        transform_chimera_after_move(state, &mut piece, to)?;
        finish_move(state, actor)?;
    }
    if !replay_interrupted && !terminal_after_move_sound {
        crate::replay::commit_move(state, &replay_before, actor)?;
    }
    if !threat_probe {
        crate::replay::record(
            state,
            if state.mode == "gameover" {
                "gameover"
            } else {
                "move"
            },
        )?;
    }
    Ok(captures)
}

fn move_privacy_v7(state: &GameState, piece: &Piece, from: Square) -> Result<Value> {
    let mut privacy = serde_json::Map::new();
    for viewer in [Color::White, Color::Black] {
        let visible = crate::observation::piece_visible_to_color_at_v7(state, piece, from, viewer)?;
        privacy.insert(viewer.as_str().into(), json!({"originVisible":visible,"typeKnown":visible || piece.color==viewer || piece.extra.get("hiddenFrom").and_then(Value::as_str)==Some(viewer.as_str())}));
    }
    Ok(Value::Object(privacy))
}

fn clear_moving_extra_flags(piece: &mut Piece) {
    for flag in [
        "frenzyExtraMove",
        "thiefSecondMove",
        "fileSurgeSecondMove",
        "rookLiftSecondMove",
        "ironMonarchExtraMove",
        "underpromotionSecondMove",
        "checkerChainCapture",
        "madHorseSecondMove",
        "platformExtraMove",
        "desperado",
    ] {
        piece.extra.shift_remove(flag);
    }
}

pub(crate) fn finish_stationary_move(
    state: &mut GameState,
    outcome: crate::v7_move_execution::StationaryMoveOutcome,
) -> Result<Vec<Piece>> {
    if let Some(before) = outcome.replay_before
        && let Some(mut capture) = crate::replay::active_move_capture(state)?
    {
        // A source-cancelled capture (including FreeMove) stays cancelled.
        // Preserve the beginMove actor; the outcome cannot invent a new one.
        capture.before = before;
        crate::replay::replace_active_move_capture(state, Some(capture))?;
    }
    match outcome.completion {
        crate::v7_move_execution::StationaryCompletion::EndMove { actor, .. } => {
            end_move_for_decision(state, actor, true, Some("move"))?;
        }
        crate::v7_move_execution::StationaryCompletion::EndGrapplerPull {
            actor,
            visual_target,
        } => {
            end_move_for_decision(state, actor, true, Some("move"))?;
            crate::v7_move_execution::finish_grappler_visual(state, &visual_target)?;
        }
        crate::v7_move_execution::StationaryCompletion::RecordTerminal => {
            crate::replay::record(state, "gameover")?;
        }
        crate::v7_move_execution::StationaryCompletion::Return => {}
    }
    Ok(outcome.captures)
}

fn capture(
    state: &mut GameState,
    attacker: &Piece,
    mut victim: Piece,
    captures: &mut Vec<Piece>,
) -> Result<()> {
    if crate::observation::truth(victim.extra.get("feudalContractId"))
        || victim.ability_kind() == "undead"
        || victim.ability_kind() == "reaper"
    {
        return Err(EngineError::UnsupportedFeature(
            "feudal/undead/reaper capture callback".into(),
        ));
    }
    for field in [
        "evasion",
        "parry",
        "explosive",
        "poisonedPawn",
        "recurrence",
        "trojanHorse",
    ] {
        if crate::observation::truth(victim.extra.get(field)) {
            return Err(EngineError::UnsupportedFeature(format!(
                "capture reaction {field}"
            )));
        }
    }
    if victim.flag("shielded") || victim.flag("protected") {
        return Err(EngineError::UnsupportedFeature(
            "shield/protection capture reaction".into(),
        ));
    }
    if victim
        .extra
        .get("hp")
        .and_then(Value::as_i64)
        .is_some_and(|hp| hp > 1)
    {
        return Err(EngineError::UnsupportedFeature(
            "HP capture reaction".into(),
        ));
    }
    clear_piece(state, &victim.id);
    victim.extra.shift_remove("checkerChainCapture");
    let actor = attacker.color.owner().ok_or(EngineError::WrongActor)?;
    add_capture_type(state, "capturedTypes", actor, &victim.kind)?;
    add_capture_type(state, "turnCaptures", actor, &victim.kind)?;
    if state.extra.contains_key("mediumMovement")
        && !matches!(
            victim.kind.as_str(),
            "wall" | "football" | "blackHole" | "black-hole"
        )
    {
        let memory = if victim.kind == "medium" {
            state.extra.get("mediumMovement").cloned()
        } else if victim.kind == "parrot" {
            state
                .extra
                .get("parrotMovement")
                .and_then(|value| value.get(victim.color.as_str()))
                .cloned()
        } else {
            crate::card_effects::current_base_movement(state, &victim)
        };
        state
            .extra
            .insert("mediumMovement".into(), memory.unwrap_or(Value::Null));
    }
    grant_vigilance_protection(state, &victim)?;
    if let Some(owner) = victim.color.owner() {
        crate::replay::normalize_color_booleans(state, "magicGirlSurge");
        state.set_flag("magicGirlSurge", owner, true);
        crate::replay::normalize_color_booleans(state, "magicGirlSurgeRefreshPending");
        if owner == state.turn
            && state.board.iter().flatten().flatten().any(|piece| {
                piece.color == owner
                    && matches!(piece.ability_kind(), "magicGirl" | "parrot" | "medium")
            })
        {
            state.set_flag("magicGirlSurgeRefreshPending", owner, true);
        }
    }
    state.captures.get_mut(actor).push(victim.clone());
    captures.push(victim.clone());
    resolve_royal_capture(state, &victim, actor)?;
    if let Some(owner) = victim.color.owner() {
        crate::flow::check_democracy_defeat(
            state,
            owner,
            if owner == actor {
                actor.opponent()
            } else {
                actor
            },
            "모든 폰이 잡혔습니다.",
        )?;
    }
    if state.ruleset_id == RULES_VERSION_V7 {
        // Frozen capturePieceAt settles campaign objectives after royal and
        // democracy defeat. The callback is inert once either has ended the
        // game; active campaign states may conclude on this capture.
        crate::v7_capture_objectives::check_campaign_objectives(state)?;
    }
    Ok(())
}

pub(crate) fn add_capture_type(
    state: &mut GameState,
    field: &str,
    owner: Color,
    kind: &str,
) -> Result<()> {
    let Some(value) = state
        .extra
        .get_mut(field)
        .and_then(|value| value.get_mut(owner.as_str()))
    else {
        return Ok(());
    };
    let values = value
        .get_mut("values")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            EngineError::InvalidState(format!("{field} player entry is not a source Set"))
        })?;
    let kind = json!(kind);
    if !values.contains(&kind) {
        values.push(kind);
    }
    Ok(())
}

/// Royal loss is a separately ordered source callback. Installation cards
/// invoke it after placing their replacement, whereas ordinary captures call
/// it immediately after recording the victim.
pub(crate) fn resolve_royal_capture(
    state: &mut GameState,
    victim: &Piece,
    actor: Color,
) -> Result<()> {
    if victim.flag("recurrence")
        && state
            .extra
            .get("pendingRecurrences")
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry.get("piece").and_then(|piece| piece.get("id")) == Some(&json!(victim.id))
                })
            })
    {
        return Ok(());
    }
    if state.ruleset_id == RULES_VERSION_V7 {
        crate::v7_promotion::resolve_recycling_holdout_after_queen_loss_v7(state, victim)?;
        if state.democracy_protects_royal(victim) {
            let owner = victim.color.owner().ok_or(EngineError::WrongActor)?;
            state.set_flag("kingDead", owner, true);
            if state.flag("zugzwang", owner)
                && !state.board.iter().flatten().flatten().any(|piece| {
                    piece.color == owner && crate::v7_threat::is_royal_identity_v7(state, piece)
                })
            {
                state.set_flag("zugzwang", owner, false);
            }
            return Ok(());
        }
        let Some(owner) = victim.color.owner() else {
            return Ok(());
        };
        if victim.kind == "vip" || victim.kind == "merchant" {
            let reason = if victim.kind == "vip" {
                format!("{} 귀빈이 잡혔습니다.", crate::replay::label(owner))
            } else {
                format!("{} 상인이 쓰러졌습니다.", crate::replay::label(owner))
            };
            return crate::flow::end_game(state, Some(actor), &reason);
        }
        if victim.flag("regencyHeir")
            && state.flag("kingDead", owner)
            && state.flag("regency", owner)
        {
            crate::flow::end_game(state, Some(actor), "왕위를 찬탈한 기물이 잡혔습니다.")?;
            return Ok(());
        }
        if crate::v7_board_hazards::source_royal_king(state, victim)? {
            state.set_flag("kingDead", owner, true);
            let heir = if state.flag("regency", owner) {
                crate::v7_board_hazards::ensure_regency_heir(state, owner)?
            } else {
                None
            };
            if heir.is_none() {
                crate::flow::end_game(
                    state,
                    Some(actor),
                    &format!("{} 킹이 잡혔습니다.", crate::replay::label(owner)),
                )?;
            }
        }
        if victim.kind == "queen"
            && state.flag("kingDead", owner)
            && state.flag("regency", owner)
            && (crate::v7_board_hazards::ensure_regency_heir(state, owner)?.is_none()
                || victim.flag("regencyHeir"))
        {
            crate::flow::end_game(state, Some(actor), "왕위를 계승할 퀸이 잡혔습니다.")?;
        }
        return Ok(());
    } else if state.flag("recycling", victim.color) && victim.kind == "queen" {
        return Err(EngineError::UnsupportedFeature(
            "recycling royal-loss promotions".into(),
        ));
    }
    if state.ruleset_id != RULES_VERSION_V7
        && state.flag("regency", victim.color)
        && (victim.is_royal() || victim.kind == "queen" || victim.flag("regencyHeir"))
    {
        return Err(EngineError::UnsupportedFeature(
            "regency royal-loss succession".into(),
        ));
    }
    if state.democracy_protects_royal(victim) {
        state.set_flag(
            "kingDead",
            victim.color.owner().ok_or(EngineError::WrongActor)?,
            true,
        );
        if !state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.color == victim.color && state.royal_identity(piece))
        {
            state.set_flag(
                "zugzwang",
                victim.color.owner().ok_or(EngineError::WrongActor)?,
                false,
            );
        }
    } else if victim.is_defeat_royal() {
        let label = if victim.color == Color::White {
            "백"
        } else {
            "흑"
        };
        let reason = if victim.kind == "vip" {
            format!("{label} 귀빈이 잡혔습니다.")
        } else if victim.kind == "merchant" {
            format!("{label} 상인이 쓰러졌습니다.")
        } else {
            state.set_flag(
                "kingDead",
                victim.color.owner().ok_or(EngineError::WrongActor)?,
                true,
            );
            format!("{label} 킹이 잡혔습니다.")
        };
        crate::flow::end_game(state, Some(actor), &reason)?;
    }
    Ok(())
}

pub(crate) fn grant_vigilance_protection(state: &mut GameState, victim: &Piece) -> Result<()> {
    let Some(owner) = victim.color.owner() else {
        return Ok(());
    };
    if !state.flag("vigilance", owner) {
        return Ok(());
    }
    let enemy = owner.opponent();
    let remaining = if state.turn == enemy { 2.0 } else { 1.0 };
    let mut royals = Vec::new();
    let mut seen = BTreeSet::new();
    for royal in state.board.iter().flatten().flatten() {
        if royal.id != victim.id
            && royal.color == owner
            && state.royal_identity(royal)
            && seen.insert(royal.id.clone())
        {
            let mut royal = royal.clone();
            let old = crate::observation::number(
                royal
                    .extra
                    .get("vigilanceProtection")
                    .and_then(|value| value.get("remaining")),
            )
            .unwrap_or(0.0);
            royal.extra.insert(
                "vigilanceProtection".into(),
                json!({"countBy":enemy,"remaining":f64::max(remaining,old)}),
            );
            royals.push(royal);
        }
    }
    for royal in royals {
        update_piece(state, &royal);
    }
    Ok(())
}

/// Scarecrow performs direct removal, not sacrifice: it does not cancel
/// prophecies or trigger ordinary capture/reaper reactions at this boundary.
pub(crate) fn scarecrow_remove(
    state: &mut GameState,
    square: Square,
    capture_owner: Color,
) -> Result<Option<Piece>> {
    let Some(victim) = state.at(square).cloned() else {
        return Ok(None);
    };
    clear_piece(state, &victim.id);
    grant_vigilance_protection(state, &victim)?;
    state.captures.get_mut(capture_owner).push(victim.clone());
    Ok(Some(victim))
}

pub(crate) fn clear_piece(state: &mut GameState, id: &str) {
    for cell in state.board.iter_mut().flatten() {
        if cell.as_ref().is_some_and(|p| p.id == id) {
            *cell = None;
        }
    }
}

/// 원문 `normalizePieceSquare`: 유효한 큰 기물 anchor와 양자 별칭을 찾는다.
pub(crate) fn normalize_piece_square(state: &GameState, square: Square) -> Result<Square> {
    if square.row >= 8 || square.col >= 8 {
        return Err(EngineError::IllegalAction);
    }
    if let Some(piece) = state.at(square) {
        if piece.is_large() {
            let coordinate = |field| {
                crate::observation::number(piece.extra.get(field))
                    .filter(|number| number.fract() == 0.0 && (0.0..8.0).contains(number))
                    .map(|number| number as u8)
            };
            if let (Some(row), Some(col)) = (coordinate("anchorRow"), coordinate("anchorCol")) {
                let anchor = Square { row, col };
                if state.at(anchor).is_some_and(|item| {
                    item.is_large()
                        && ((!piece.id.is_empty() && item.id == piece.id) || item == piece)
                }) {
                    return Ok(anchor);
                }
            }
        }
        return Ok(square);
    }
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let origin = Square { row, col };
            let Some(piece) = state.at(origin) else {
                continue;
            };
            if !seen.insert(piece.id.clone()) {
                continue;
            }
            let Some(quantum) = piece.extra.get("quantum").filter(|value| value.is_object()) else {
                continue;
            };
            let Some(qrow) = crate::observation::number(quantum.get("row")) else {
                continue;
            };
            let Some(qcol) = crate::observation::number(quantum.get("col")) else {
                continue;
            };
            let extent = if piece.is_large() { 2.0 } else { 1.0 };
            if f64::from(square.row) >= qrow
                && f64::from(square.row) < qrow + extent
                && f64::from(square.col) >= qcol
                && f64::from(square.col) < qcol + extent
            {
                // origin is physical; recursion cannot enter the empty-square branch.
                return normalize_piece_square(state, origin);
            }
        }
    }
    Ok(square)
}

/// 빈 id는 하나의 셀을 가리킨다. 서로 다른 무명 기물을 함께 지우지 않는다.
pub(crate) fn remove_piece_from_board_cells(
    state: &mut GameState,
    piece: &Piece,
    square: Square,
) -> Result<()> {
    if square.row >= 8 || square.col >= 8 {
        return Err(EngineError::IllegalAction);
    }
    if !piece.id.is_empty() {
        clear_piece(state, &piece.id);
    } else if piece.is_large() {
        for cell in state.board.iter_mut().flatten() {
            if cell.as_ref().is_some_and(|item| item == piece) {
                *cell = None;
            }
        }
    } else {
        state.board[usize::from(square.row)][usize::from(square.col)] = None;
    }
    Ok(())
}

/// 원문의 사신 처형 후 이동 표시·기보·종료 frame 정산 순서.
pub(crate) fn finalize_immediate_reaper_execution(
    state: &mut GameState,
    active: &mut Piece,
    from: Square,
    _moved_as_type: &str,
) -> Result<bool> {
    let execution = crate::v7_board_hazards::consume_active_royal_reaper_execution(state, active)?;
    let reached = if let Some(execution) = &execution {
        Some(execution.to)
    } else {
        crate::v7_board_hazards::consume_immediate_reaper_target(state, active)?
    };
    if let Some(to) = reached {
        let (piece, origin) = execution
            .as_ref()
            .map(|entry| (&entry.reaper, entry.from))
            .unwrap_or((active, from));
        let actor = piece.color.owner().ok_or(EngineError::WrongActor)?;
        let medium = state
            .active_v7_move_context
            .as_ref()
            .and_then(crate::v7_move_execution::medium_move_snapshot)
            .map(|(id, memory)| (id.to_owned(), memory.clone()));
        crate::card_effects::set_last_move_with_medium_memory(
            state,
            crate::card_effects::LastMoveContext {
                from: origin,
                to,
                sound_name: "capture",
                sound_color: actor,
                hidden_from: "",
                moved_override: None,
                original_medium: medium.as_ref().map(|(id, memory)| (id.as_str(), memory)),
            },
        )?;
        let message = if crate::replay::fog_log_redaction_active_v7(state) {
            format!("{} 기물이 이동했습니다.", crate::replay::label(actor))
        } else {
            format!(
                "{} 사신: {}{} -> {}{}",
                crate::replay::label(actor),
                char::from(b'a' + origin.col),
                8 - origin.row,
                char::from(b'a' + to.col),
                8 - to.row
            )
        };
        crate::replay::add_log(state, message)?;
    }
    let notations = crate::v7_board_hazards::flush_reaper_execution_notations(state, active)?;
    if (reached.is_some() || notations > 0)
        && state.mode == "gameover"
        && state.threat_probe_depth == 0
    {
        crate::replay::record(state, "gameover")?;
    }
    Ok(reached.is_some())
}

/// Permanent Judgment uses direct environmental removal rather than an
/// ordinary attack or sacrifice: shields/HP do not block it, and vigilance
/// belongs to the removed piece's side. Recurrence and royal-system callbacks
/// retain explicit support guards until their shared kernels are ported.
pub(crate) fn judgment_remove(
    state: &mut GameState,
    square: Square,
    actor: Color,
) -> Result<Option<Piece>> {
    let Some(piece) = state.at(square).cloned() else {
        return Ok(None);
    };
    let owner = piece.color.owner().ok_or(EngineError::IllegalAction)?;
    if state.royal_identity(&piece)
        || piece.is_large()
        || matches!(piece.kind.as_str(), "wall" | "football" | "blackHole")
    {
        return Err(EngineError::IllegalAction);
    }
    if state.ruleset_id == RULES_VERSION_V7 {
        let mut working = state.clone();
        let capture_owner = if owner == actor {
            actor.opponent()
        } else {
            actor
        };
        remove_piece_from_board_cells(&mut working, &piece, square)?;
        grant_vigilance_protection(&mut working, &piece)?;
        working.captures.get_mut(capture_owner).push(piece.clone());
        crate::card_effects::mark_vanish_animation(&mut working, &piece, square)?;
        crate::flow::mark_progress(&mut working);
        crate::replay::add_log(
            &mut working,
            format!(
                "레드카드: {}{}의 {}이 마지막 드래프트 이후 영구적으로 제거되었습니다.",
                char::from(b'a' + square.col),
                8 - square.row,
                crate::replay::source_piece_label(&piece.kind)
                    .filter(|name| !name.is_empty())
                    .unwrap_or(&piece.kind)
            ),
        )?;
        crate::v7_board_hazards::resolve_environmental_defeats(
            &mut working,
            &[crate::v7_board_hazards::EnvironmentalRemoval {
                piece: piece.clone(),
                square,
                capture_owner,
            }],
            "레드카드",
            false,
        )?;
        crate::v7_capture_objectives::check_campaign_objectives(&mut working)?;
        *state = working;
        return Ok(Some(piece));
    }
    for key in ["crownRule", "campaignScenarioId"] {
        if crate::observation::truth(state.extra.get(key)) {
            return Err(EngineError::UnsupportedFeature(format!(
                "Judgment environmental {key} reconciliation"
            )));
        }
    }
    if crate::observation::truth(piece.extra.get("recurrence"))
        || state
            .extra
            .get("pendingRecurrences")
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty())
    {
        return Err(EngineError::UnsupportedFeature(
            "Judgment recurrence revival".into(),
        ));
    }
    if [Color::White, Color::Black]
        .into_iter()
        .any(|color| state.flag("democracy", color) || state.flag("regency", color))
    {
        return Err(EngineError::UnsupportedFeature(
            "Judgment democracy/regency environmental defeat".into(),
        ));
    }
    if state.board.iter().enumerate().any(|(row, cells)| {
        cells.iter().enumerate().any(|(col, cell)| {
            cell.as_ref().is_some_and(|candidate| {
                candidate.kind == "reaper"
                    && candidate.id != piece.id
                    && row.abs_diff(usize::from(square.row)) <= 1
                    && col.abs_diff(usize::from(square.col)) <= 1
            })
        })
    }) {
        return Err(EngineError::UnsupportedFeature(
            "Judgment adjacent reaper soul/execution".into(),
        ));
    }
    let capture_owner = if owner == actor {
        actor.opponent()
    } else {
        actor
    };
    clear_piece(state, &piece.id);
    grant_vigilance_protection(state, &piece)?;
    state.captures.get_mut(capture_owner).push(piece.clone());
    crate::card_effects::mark_vanish_animation(state, &piece, square)?;
    crate::flow::mark_progress(state);
    crate::replay::add_log(
        state,
        format!(
            "레드카드: {}{}의 {}이 마지막 드래프트 이후 영구적으로 제거되었습니다.",
            char::from(b'a' + square.col),
            8 - square.row,
            crate::replay::piece_label(&piece.kind)
        ),
    )?;
    if piece.kind == "pawn"
        && state.flag("resolve", owner)
        && crate::observation::number(
            state
                .extra
                .get("resolveSpentTurn")
                .and_then(|v| v.get(owner.as_str())),
        ) != Some(f64::from(*state.turns_taken.get(owner)))
    {
        if !crate::observation::truth(state.extra.get("resolveReady")) {
            state
                .extra
                .insert("resolveReady".into(), json!({"white":false,"black":false}));
        }
        let ready = state
            .extra
            .get_mut("resolveReady")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| EngineError::InvalidState("resolveReady must be a color map".into()))?;
        ready.insert(owner.as_str().into(), json!(true));
    }
    Ok(Some(piece))
}
/// Sacrifice bypasses ordinary shield and HP capture protection. The source
/// stores it as a capture without granting wizard mana or capturedTypes.
pub(crate) fn record_new_card_capture_reactions(
    state: &mut GameState,
    captured: &Piece,
    capturer: Color,
    non_capture: bool,
) -> Result<()> {
    crate::v7_capture_reactions::record_new_card_capture_reactions_with_options(
        state,
        captured,
        capturer,
        non_capture,
        false,
    )
}

pub(crate) fn sacrifice(
    state: &mut GameState,
    square: Square,
    capture_color: Color,
) -> Result<Option<Piece>> {
    if state.ruleset_id == RULES_VERSION_V7 {
        let mut working = state.clone();
        let square = normalize_piece_square(&working, square)?;
        let Some(piece) = working.at(square).cloned() else {
            return Ok(None);
        };
        remove_piece_from_board_cells(&mut working, &piece, square)?;
        grant_vigilance_protection(&mut working, &piece)?;
        crate::v7_board_hazards::resolve_reaper_nearby_deaths(
            &mut working,
            &[crate::v7_board_hazards::EnvironmentalRemoval {
                piece: piece.clone(),
                square,
                capture_owner: capture_color,
            }],
        )?;
        cancel_prophecies_by_capture(&mut working)?;
        working.captures.get_mut(capture_color).push(piece.clone());
        crate::v7_promotion::resolve_recycling_holdout_after_queen_loss_v7(&mut working, &piece)?;
        *state = working;
        return Ok(Some(piece));
    }
    let Some(piece) = state.at(square).cloned() else {
        return Ok(None);
    };
    for color in [Color::White, Color::Black] {
        if state.flag("vigilance", color) || state.flag("recycling", color) {
            return Err(EngineError::UnsupportedFeature(
                "sacrifice vigilance/recycling reaction".into(),
            ));
        }
    }
    if state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|candidate| candidate.ability_kind() == "reaper")
    {
        return Err(EngineError::UnsupportedFeature(
            "sacrifice reaper nearby deaths".into(),
        ));
    }
    clear_piece(state, &piece.id);
    if let Some(prophecy) = state
        .extra
        .get_mut("prophecy")
        .and_then(Value::as_object_mut)
    {
        for color in [Color::White, Color::Black] {
            if prophecy
                .get(color.as_str())
                .is_some_and(|entry| !entry.is_null())
            {
                prophecy.insert(color.as_str().into(), Value::Null);
            }
        }
    }
    state.captures.get_mut(capture_color).push(piece.clone());
    Ok(Some(piece))
}
/// Expansion removal adds a replay visual even when the DOM animation returns
/// early. Ordinary sacrifices deliberately do not queue this visual.
pub(crate) fn expansion_sacrifice(
    state: &mut GameState,
    square: Square,
    capture_color: Color,
) -> Result<Option<Piece>> {
    let removed = sacrifice(state, square, capture_color)?;
    if let Some(piece) = &removed {
        crate::card_effects::mark_vanish_animation(state, piece, square)?;
        crate::replay::queue_visual(
            state,
            json!({"type":"board-change","effect":"cleanup-sacrifice","color":capture_color,"removals":[{"square":square,"color":piece.color,"pieceType":piece.kind}],"relocations":[],"transformations":[],"spawns":[]}),
        )?;
    }
    Ok(removed)
}

/// Source `forceRemovePieceAt` for effect-driven removal. Unlike a sacrifice,
/// it records capture types and resolves the same royal, democracy, and
/// campaign callbacks as a forced capture. The transaction keeps an
/// unsupported reaction from publishing a half-removed piece.
pub(crate) fn force_remove_piece_at(
    state: &mut GameState,
    square: Square,
    capturer: Color,
) -> Result<Option<Piece>> {
    force_remove_piece_at_with_options(state, square, capturer, &ForceRemovalOptions::default())
}

#[derive(Default)]
pub(crate) struct ForceRemovalOptions<'a> {
    pub(crate) suppress_reaper_progress: bool,
    pub(crate) count_as_capture: bool,
    pub(crate) suppress_calling_card: bool,
    pub(crate) attacker: Option<&'a Piece>,
    pub(crate) threat_source: Option<&'a Value>,
}

pub(crate) fn force_remove_piece_at_with_options(
    state: &mut GameState,
    square: Square,
    capturer: Color,
    options: &ForceRemovalOptions<'_>,
) -> Result<Option<Piece>> {
    let mut working = state.clone();
    let removed = force_remove_piece_at_raw(&mut working, square, capturer, options)?;
    *state = working;
    Ok(removed)
}

fn force_remove_piece_at_raw(
    state: &mut GameState,
    square: Square,
    capturer: Color,
    options: &ForceRemovalOptions<'_>,
) -> Result<Option<Piece>> {
    let Some(captured) = state.at(square).cloned() else {
        return Ok(None);
    };
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "forceRemovePieceAt requires v7 source semantics".into(),
        ));
    }
    // Source remembers the victim's base movement before protection or board
    // mutation; neutral terrain does not update the Medium memory.
    if state.extra.contains_key("mediumMovement")
        && !matches!(
            captured.kind.as_str(),
            "wall" | "football" | "blackHole" | "black-hole"
        )
    {
        let memory = if captured.kind == "medium" {
            state.extra.get("mediumMovement").cloned()
        } else if captured.kind == "parrot" {
            state
                .extra
                .get("parrotMovement")
                .and_then(|value| value.get(captured.color.as_str()))
                .cloned()
        } else {
            crate::card_effects::current_base_movement(state, &captured)
        };
        state
            .extra
            .insert("mediumMovement".into(), memory.unwrap_or(Value::Null));
    }
    grant_vigilance_protection(state, &captured)?;
    if let Some(initiative) = state
        .extra
        .get_mut("initiative")
        .and_then(Value::as_object_mut)
        && initiative
            .get(captured.color.as_str())
            .is_some_and(|entry| entry["by"] == json!(capturer))
    {
        initiative.insert(captured.color.as_str().into(), Value::Null);
        crate::replay::add_log(state, "선공권 제한이 해제되었습니다.".into())?;
    }
    add_capture_type(state, "capturedTypes", capturer, &captured.kind)?;
    add_capture_type(state, "turnCaptures", capturer, &captured.kind)?;
    if captured.is_large() {
        remove_piece_from_board_cells(state, &captured, square)?;
    } else {
        state.board[usize::from(square.row)][usize::from(square.col)] = None;
    }
    crate::v7_threat::mark_king_threat_removal_cause(
        state,
        &captured,
        square,
        options.threat_source.unwrap_or(&Value::Null),
        state.threat_probe_depth > 0,
    )?;
    if let Some(prophecy) = state.extra.get_mut("prophecy") {
        let entries = prophecy
            .as_object_mut()
            .ok_or_else(|| EngineError::InvalidState("prophecy must be a player map".into()))?;
        for color in [Color::White, Color::Black] {
            if entries
                .get(color.as_str())
                .is_some_and(|entry| !entry.is_null())
            {
                entries.insert(color.as_str().into(), Value::Null);
            }
        }
    }
    state.captures.get_mut(capturer).push(captured.clone());
    let defeat_winner = if captured.color == capturer {
        capturer.opponent()
    } else {
        capturer
    };
    resolve_royal_capture(state, &captured, defeat_winner)?;
    if let Some(owner) = captured.color.owner() {
        crate::flow::check_democracy_defeat(state, owner, defeat_winner, "모든 폰이 잡혔습니다.")?;
    }
    crate::flow::mark_progress(state);
    if options.count_as_capture && !options.suppress_reaper_progress {
        let mut active = options.attacker.cloned();
        crate::v7_board_hazards::resolve_reaper_nearby_deaths_with_context(
            state,
            &[crate::v7_board_hazards::EnvironmentalRemoval {
                piece: captured.clone(),
                square,
                capture_owner: capturer,
            }],
            active.as_mut(),
            None,
            false,
        )?;
    }
    if !options.suppress_calling_card {
        crate::v7_capture_reactions::resolve_calling_card_capture(state, &captured, None, square)?;
    }
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(Some(captured))
}

/// `removeExpansionEffectPiece` adds its separate cleanup visual only after
/// the source force-removal callbacks have succeeded.
pub(crate) fn expansion_effect_remove(
    state: &mut GameState,
    square: Square,
    capturer: Color,
    _label: &str,
) -> Result<Option<Piece>> {
    let mut working = state.clone();
    let removed = force_remove_piece_at_raw(
        &mut working,
        square,
        capturer,
        &ForceRemovalOptions::default(),
    )?;
    if let Some(piece) = &removed {
        crate::card_effects::mark_vanish_animation(&mut working, piece, square)?;
        crate::replay::queue_visual(
            &mut working,
            json!({"type":"board-change","effect":"cleanup-sacrifice","color":capturer,"removals":[{"square":square,"color":piece.color,"pieceType":piece.kind}],"relocations":[],"transformations":[],"spawns":[]}),
        )?;
    }
    *state = working;
    Ok(removed)
}
pub(crate) fn update_piece(state: &mut GameState, piece: &Piece) {
    for cell in state.board.iter_mut().flatten() {
        if cell.as_ref().is_some_and(|p| p.id == piece.id) {
            *cell = Some(piece.clone());
        }
    }
}

fn transform_chimera_after_move(
    state: &mut GameState,
    piece: &mut Piece,
    square: Square,
) -> Result<()> {
    if !crate::observation::truth(piece.extra.get("chimera"))
        || state.at(square).is_none_or(|at| at.id != piece.id)
    {
        return Ok(());
    }
    let monochrome = crate::observation::truth(state.extra.get("monochromeChess"));
    let normalize = |kind| {
        if monochrome && kind == "knight" {
            "camel"
        } else {
            kind
        }
    };
    let kinds = if piece.kind == "queen" {
        &["pawn", "knight", "bishop", "rook"][..]
    } else {
        &["pawn", "knight", "bishop", "rook", "queen"][..]
    };
    let options = kinds
        .iter()
        .copied()
        .map(normalize)
        .filter(|kind| *kind != normalize(piece.kind.as_str()))
        .map(|kind| {
            (
                kind,
                if piece.kind == "queen" {
                    25.0
                } else if kind == "queen" {
                    10.0
                } else {
                    30.0
                },
            )
        })
        .collect::<Vec<_>>();
    let weights = options
        .iter()
        .map(|(_, weight)| *weight)
        .collect::<Vec<_>>();
    piece.kind = options[sample_weighted(state, &weights)?].0.into();
    for field in [
        "windmillMode",
        "logDir",
        "logRollAfterTurn",
        "mana",
        "maxMana",
        "ammo",
        "maxAmmo",
        "facing",
    ] {
        piece.extra.shift_remove(field);
    }
    piece.moved = true;
    piece.extra.insert("shielded".into(), json!(false));
    if monochrome {
        piece
            .extra
            .insert("monoShade".into(), json!((square.row + square.col) % 2));
    }
    update_piece(state, piece);
    crate::card_effects::mark_animation(state, piece)?;
    crate::replay::add_piece_action_log(
        state,
        piece,
        Some(square),
        None,
        format!(
            "키메라: {}{}의 기물이 {}으로 변신했습니다.",
            char::from(b'a' + square.col),
            8 - square.row,
            crate::replay::piece_label(&piece.kind)
        ),
    )?;
    Ok(())
}

fn finish_move(state: &mut GameState, actor: Color) -> Result<()> {
    finish_move_with_count(state, actor, true)
}

pub(crate) fn end_move_for_decision(
    state: &mut GameState,
    actor: Color,
    count_move: bool,
    history_reason: Option<&str>,
) -> Result<()> {
    crate::legal_profile::measure("end_move_for_decision", || {
        end_move_for_decision_profiled(state, actor, count_move, history_reason)
    })
}

pub(crate) fn end_move_for_decision_profiled(
    state: &mut GameState,
    actor: Color,
    count_move: bool,
    history_reason: Option<&str>,
) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "decision endMove requires v7 source semantics".into(),
        ));
    }
    finish_move_with_history(state, actor, count_move, history_reason)
}

fn finish_move_with_count(state: &mut GameState, actor: Color, count_move: bool) -> Result<()> {
    finish_move_with_history(state, actor, count_move, None)
}

fn finish_move_with_history(
    state: &mut GameState,
    actor: Color,
    count_move: bool,
    history_reason: Option<&str>,
) -> Result<()> {
    crate::legal_profile::measure("finish_move_with_history", || {
        finish_move_with_history_profiled(state, actor, count_move, history_reason)
    })
}

fn finish_move_with_history_profiled(
    state: &mut GameState,
    actor: Color,
    count_move: bool,
    history_reason: Option<&str>,
) -> Result<()> {
    let mut context = crate::v7_end_move_reactions::V7EndMoveContext::default();
    let result = finish_move_with_count_inner(state, actor, count_move, &mut context);
    if context.entered_history_scope {
        // Source endMove's finally block runs for retained turns and errors.
        // Commit again from the original capture after complete-turn callbacks.
        let finalization = (|| {
            if let Some(before) = context.replay_capture {
                crate::replay::replace_active_move_capture(state, Some(before))?;
                crate::replay::commit_active_move(state, actor)?;
            }
            crate::replay::record(
                state,
                if state.mode == "gameover" {
                    "gameover"
                } else {
                    history_reason.unwrap_or("move")
                },
            )
        })();
        if let Some(previous) = context.previous_history_number {
            state
                .extra
                .insert("activeHistoryMoveNumber".into(), previous);
        } else {
            state.extra.shift_remove("activeHistoryMoveNumber");
        }
        finalization?;
    }
    result
}

fn finish_move_with_count_inner(
    state: &mut GameState,
    actor: Color,
    count_move: bool,
    context: &mut crate::v7_end_move_reactions::V7EndMoveContext,
) -> Result<()> {
    crate::legal_profile::measure("finish_move_with_count_inner", || {
        finish_move_with_count_inner_profiled(state, actor, count_move, context)
    })
}

fn finish_move_with_count_inner_profiled(
    state: &mut GameState,
    actor: Color,
    count_move: bool,
    context: &mut crate::v7_end_move_reactions::V7EndMoveContext,
) -> Result<()> {
    prune_board_potion_effects(state)?;
    let v7 = state.ruleset_id == RULES_VERSION_V7;
    if v7 {
        crate::v7_move_transition::finish_active_roller(state)?;
        state.active_v7_saturation_attack = None;
        match crate::v7_end_move_reactions::settle_end_move_before_count_with_context(
            state, actor, context,
        )? {
            crate::v7_turn_flow::V7FlowControl::Continue => {}
            crate::v7_turn_flow::V7FlowControl::RetainTurn
            | crate::v7_turn_flow::V7FlowControl::Terminal => return Ok(()),
        }
    } else {
        crate::replay::normalize_color_booleans(state, "skipTurn");
        if state.actions_remaining > 1 {
            state.actions_remaining -= 1;
            // The source probes for an action immediately after consuming one of
            // the current player's extra moves. This can settle an immobile
            // position and can advance the source RNG through a card probe.
            crate::flow::check_no_action_loss(state)?;
            return Ok(());
        }
    }
    if !v7 && !crate::flow::commit_turn_clock(state, actor)? {
        return Ok(());
    }
    // With thiefRemake enabled, v7 tickThiefArrests clears every piece's
    // path memory before actor-specific arrests. v6 clears only the actor's.
    if !v7 {
        for piece in state
            .board
            .iter_mut()
            .flatten()
            .flatten()
            .filter(|piece| piece.color == actor)
        {
            piece.extra.shift_remove("thiefVisited");
            piece.extra.shift_remove("thiefLastDirection");
        }
    }
    if count_move {
        state.move_count = state
            .move_count
            .checked_add(1)
            .ok_or_else(|| EngineError::InvalidState("move count overflow".into()))?;
        if v7 {
            crate::v7_queued_effects::resolve_pending_lobsters_after_move(state)?;
            crate::v7_end_move_reactions::clear_previous_trickster_abilities(state);
            crate::v7_piece_lifecycle::resolve_undead_resurrections_after_move(state)?;
            if crate::v7_end_move_reactions::check_gomoku_victory(state)? {
                return Ok(());
            }
        }
        advance_otherworld(state)?;
        if v7 && actor == Color::Black {
            crate::v7_end_move_reactions::move_rule_monsters(state, actor)?;
        }
    } else if v7 {
        crate::v7_end_move_reactions::clear_previous_trickster_abilities(state);
    }
    if v7 {
        if state.mode == "gameover" {
            return Ok(());
        }
        // finalizeMove checks both conscription sides after its counted
        // otherworld callback, then settles temporary status before forcing
        // opening cards. completeTurnAfterMove checks conscription again.
        for color in [Color::White, Color::Black] {
            crate::v7_turn_entry::check_conscription(state, color)?;
            if state.mode == "gameover" {
                return Ok(());
            }
        }
        if v7_stage_stops(crate::v7_end_move_reactions::settle_end_move_after_count(
            state, actor,
        )?) {
            return Ok(());
        }
    }
    if !v7 {
        tick_piece_turn_effects(state, actor)?;
    }
    if !v7 && state.extra.contains_key("enPassantFrenzy") {
        state.set_flag("enPassantFrenzy", actor, false);
    }
    resolve_first_move_cards(state, actor)?;
    if v7 {
        crate::v7_end_move_reactions::apply_pending_pawn_storm_for_turn(state, actor)?;
        crate::v7_threat::resolve_herald_threats_v7(state, actor)?;
        if state.mode == "gameover" {
            return Ok(());
        }
        if count_move {
            crate::v7_end_move_reactions::tick_armistice_after_action(state, actor)?;
        }
        // main93647: completed-turn clock/flags follow counted rules and
        // first-move/PawnStorm/Herald, before the Black conveyor.
        if !crate::flow::commit_turn_clock(state, actor)? {
            return Ok(());
        }
        let rule = crate::v7_turn_flow::V7TurnFlowRule;
        rule.preflight(state, actor)?;
        rule.clear_actor_pre_count_flags(state, actor)?;
    }
    if v7 && v7_stage_stops(crate::v7_board_automata::V7BoardAutomata.before_count(state, actor)?) {
        return Ok(());
    }
    if v7 {
        crate::v7_turn_flow::V7TurnFlowRule.after_board_before_count(state, actor)?;
    }
    let turns = state.turns_taken.get_mut(actor);
    *turns = turns
        .checked_add(1)
        .ok_or_else(|| EngineError::InvalidState("turn count overflow".into()))?;
    if state.ruleset_id == RULES_VERSION_V7 {
        if v7_stage_stops(crate::v7_piece_lifecycle::after_count(state, actor)?) {
            return Ok(());
        }
        if v7_stage_stops(
            crate::v7_board_automata::V7BoardAutomata.after_empty_lunchboxes(state, actor)?,
        ) {
            return Ok(());
        }
        // The frozen client's completed-turn Othello scan follows the actor's
        // mistake-card reset and precedes pending gales and later turn ticks.
        crate::replay::normalize_color_booleans(state, "mistakeCard");
        state
            .extra
            .get_mut("mistakeCard")
            .expect("normalized colors")[actor.as_str()] = json!(false);
        crate::turn_effects_v7::settle_after_completed_turn(state, actor)?;
        // main93675 settles pending gales even when Othello ended the game.
        if v7_stage_stops(crate::v7_queued_effects::after_count(state, actor)?) {
            return Ok(());
        }
        if v7_stage_stops(crate::v7_board_automata::V7BoardAutomata.after_taboo(state, actor)?) {
            return Ok(());
        }
        if v7_stage_stops(crate::v7_piece_lifecycle::after_revolving(state, actor)?) {
            return Ok(());
        }
    }
    if !v7 {
        crate::threat::tick_protection(state, actor, "sacrificeProtection");
        tick_card_frozen_and_poison(state, actor)?;
    }
    if crate::flow::tick_deathmatch(state, actor)? {
        return Ok(());
    }
    if v7 {
        if v7_stage_stops(crate::v7_piece_lifecycle::after_deathmatch(state, actor)?) {
            return Ok(());
        }
        if v7_stage_stops(crate::v7_queued_effects::after_prophecy(state, actor)?) {
            return Ok(());
        }
        if v7_stage_stops(crate::v7_piece_lifecycle::after_scarecrows(state, actor)?) {
            return Ok(());
        }
    }
    if actor == Color::Black {
        state.full_move = state
            .full_move
            .checked_add(1)
            .ok_or_else(|| EngineError::InvalidState("full move count overflow".into()))?;
    }
    if v7
        && v7_stage_stops(crate::v7_piece_lifecycle::after_full_move_cleanup(
            state, actor,
        )?)
    {
        return Ok(());
    }
    crate::replay::normalize_winter_after_turn(state)?;
    if v7
        && v7_stage_stops(crate::v7_board_automata::V7BoardAutomata.after_full_move(state, actor)?)
    {
        return Ok(());
    }
    if v7 && v7_stage_stops(crate::v7_queued_effects::after_winter(state, actor)?) {
        return Ok(());
    }
    if !v7 {
        state.cards_used_this_turn = Sides::new(
            state.cards_used_this_turn.white,
            state.cards_used_this_turn.black,
        );
        *state.cards_used_this_turn.get_mut(actor) = 0;
    }
    if let Some(current) = state
        .extra
        .get("turnCaptures")
        .and_then(|value| value.get(actor.as_str()))
        .cloned()
    {
        if let Some(last) = state
            .extra
            .get_mut("lastTurnCaptures")
            .and_then(Value::as_object_mut)
        {
            last.insert(actor.as_str().into(), current);
        }
        state
            .extra
            .get_mut("turnCaptures")
            .expect("turn captures present")[actor.as_str()] =
            json!({"__simType":"Set","values":[]});
    }
    for key in [
        "zugzwang",
        "mistakeCard",
        "freeMoveCaptureLock",
        "magicGirlSurge",
        "magicGirlSurgeRefreshPending",
    ] {
        crate::replay::normalize_color_booleans(state, key);
    }
    for key in ["zugzwang", "mistakeCard", "freeMoveCaptureLock"] {
        state.extra.get_mut(key).expect("normalized colors")[actor.as_str()] = json!(false);
    }
    if v7 && v7_stage_stops(crate::v7_queued_effects::after_capture_reset(state, actor)?) {
        return Ok(());
    }
    let refresh = state.extra["magicGirlSurgeRefreshPending"][actor.as_str()].clone();
    state
        .extra
        .get_mut("magicGirlSurge")
        .expect("normalized colors")[actor.as_str()] = refresh;
    state
        .extra
        .get_mut("magicGirlSurgeRefreshPending")
        .expect("normalized colors")[actor.as_str()] = json!(false);
    if !v7 && state.extra.contains_key("reversal") {
        state.set_flag("reversal", actor, false);
    }
    if v7 {
        state.cards_used_this_turn = Sides::new(
            state.cards_used_this_turn.white,
            state.cards_used_this_turn.black,
        );
        *state.cards_used_this_turn.get_mut(actor) = 0;
        if v7_stage_stops(crate::v7_capture_objectives::after_completed_turn_count(
            state, actor,
        )?) {
            return Ok(());
        }
    }
    state.turn = actor.opponent();
    let incoming = state.turn;
    if v7
        && v7_stage_stops(crate::v7_turn_entry::resolve_vip_invitations(
            state, incoming,
        )?)
    {
        return Ok(());
    }
    // main93664-93667 resolves incoming-turn reservations after the actor
    // switch. VIP and ICBM pending contexts remain explicit unsupported
    // movement states; ordinary Portal Gun reservations settle here.
    crate::card_effects::resolve_pending_portals_for_turn(state, incoming)?;
    if v7 {
        if v7_stage_stops(crate::v7_turn_entry::resolve_pending_icbm(state, incoming)?) {
            return Ok(());
        }
        if v7_stage_stops(
            crate::v7_board_automata::V7BoardAutomata.incoming_after_switch(state, incoming)?,
        ) {
            return Ok(());
        }
        if v7_stage_stops(crate::v7_turn_entry::tick_siren_exposure_for_turn_start(
            state, incoming,
        )?) {
            return Ok(());
        }
        if v7_stage_stops(crate::v7_incoming_reactions::herald_after_siren(
            state, actor,
        )?) {
            return Ok(());
        }
        if v7_stage_stops(crate::v7_turn_entry::resolve_incoming_automatic_effects(
            state, incoming,
        )?) {
            return Ok(());
        }
        if v7_stage_stops(
            crate::v7_incoming_reactions::maybe_grant_night_blood_for_turn(state, incoming)?,
        ) {
            return Ok(());
        }
        if v7_stage_stops(
            crate::v7_incoming_reactions::reset_time_traveler_cards_for_turn(state, incoming)?,
        ) {
            return Ok(());
        }
        if v7_stage_stops(crate::v7_turn_entry::clear_turn_start_acceleration_trail(
            state, incoming,
        )?) {
            return Ok(());
        }
    }
    clear_coronation_protection(state, state.turn);
    if state.ruleset_id == RULES_VERSION_V7 {
        if v7_stage_stops(crate::v7_incoming_reactions::resolve_holdout_promotions(
            state, incoming,
        )?) {
            return Ok(());
        }
        if v7_stage_stops(crate::v7_incoming_reactions::resolve_local_brutus(
            state, incoming,
        )?) {
            return Ok(());
        }
        // Source completeTurnAfterMove resolves local Don Quixote after the
        // incoming side's coronation/Brutus window and before its action limit.
        // The helper admits only a source-verified inert predecessor profile.
        crate::turn_effects_v7::resolve_don_quixote_turn_entry(state, incoming)?;
        if state.mode == "gameover" {
            return Ok(());
        }
        if v7_stage_stops(crate::v7_turn_entry::apply_pending_acceleration(
            state, incoming,
        )?) {
            return Ok(());
        }
    }
    state.actions_remaining = if state.flag("acceleration", state.turn) {
        2
    } else {
        1
    };
    crate::flow::start_clock(state)?;
    if v7
        && v7_stage_stops(crate::v7_turn_entry::settle_incoming_economy(
            state, incoming,
        )?)
    {
        return Ok(());
    }
    if v7
        && v7_stage_stops(crate::v7_incoming_reactions::herald_after_economy(
            state, actor,
        )?)
    {
        return Ok(());
    }
    if crate::flow::check_termination(state)? {
        crate::flow::pause_clock(state)?;
    }
    if state.ruleset_id == RULES_VERSION_V7 {
        // Frozen completeTurnAfterMove checks repetition/star limits, then
        // enters a MIDDLE/END draft before probing next-turn no-action loss.
        crate::flow::maybe_start_milestone_draft(state)?;
    }
    crate::flow::check_no_action_loss(state)?;
    Ok(())
}

fn v7_stage_stops(control: crate::v7_turn_flow::V7FlowControl) -> bool {
    control != crate::v7_turn_flow::V7FlowControl::Continue
}

fn first_move_auto_card(card: &CardSlot, definitions: &crate::draft::Definitions) -> bool {
    if card.vacant || card.used || !crate::observation::truth(card.extra.get("firstTurnCard")) {
        return false;
    }
    // libraryCardPhase prefers the frozen definition over the stored card's
    // mutable phase. The two exceptional IDs are also accepted by the source.
    let phase = definitions
        .definitions
        .iter()
        .find(|definition| definition["id"] == card.id)
        .and_then(|definition| definition["phase"].as_str())
        .or_else(|| card.extra.get("phase").and_then(Value::as_str))
        .unwrap_or("END");
    phase == "OPENING"
        || matches!(
            card.id.as_str(),
            "shotgun-king" | "black-tower-legacy-magic"
        )
}

pub(crate) fn should_store_first_move_undo(state: &GameState, actor: Color) -> Result<bool> {
    let definitions = crate::draft::definitions_for_ruleset(&state.ruleset_id)?;
    Ok(!state.flag("firstMoveCardsForced", actor)
        && *state.turns_taken.get(actor) == 0
        && state
            .deck_slots
            .get(actor)
            .iter()
            .any(|card| first_move_auto_card(card, definitions)))
}

// main88121 captures exactly the fields restored by main90846. The remaining
// state (including the clock, RNG, moveCount, logs and replay capture) stays
// advanced even when the first board move is canceled.
pub(crate) fn capture_first_move_undo(state: &GameState, actor: Color) -> Value {
    let field = |name: &str, fallback: Value| state.extra.get(name).cloned().unwrap_or(fallback);
    let player_map = |name: &str| {
        let source = state.extra.get(name).unwrap_or(&Value::Null);
        let mut rebuilt = serde_json::Map::new();
        for color in [Color::White, Color::Black] {
            rebuilt.insert(color.as_str().into(), source[color.as_str()].clone());
        }
        Value::Object(rebuilt)
    };
    let mut captures = serde_json::Map::new();
    captures.insert("white".into(), json!(state.captures.white));
    captures.insert("black".into(), json!(state.captures.black));
    json!({
        "color": actor,
        "board": state.board,
        "captures": captures,
        "enPassant": state.en_passant,
        "selected": field("selected", Value::Null),
        "legalMoves": field("legalMoves", json!([])),
        "kingDead": field("kingDead", json!({})),
        "castled": field("castled", json!({"white":false,"black":false})),
        "capturedTypes": player_map("capturedTypes"),
        "turnCaptures": player_map("turnCaptures"),
    })
}

fn restore_first_move_undo(state: &mut GameState, undo: &Value) -> Result<()> {
    let field = |name: &str| {
        undo.get(name)
            .cloned()
            .ok_or_else(|| EngineError::InvalidState(format!("firstMoveUndo.{name} missing")))
    };
    let board: Vec<Vec<Option<Piece>>> =
        serde_json::from_value(field("board")?).map_err(EngineError::serialization)?;
    if board.len() != 8 || board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::InvalidState(
            "firstMoveUndo.board must be 8x8".into(),
        ));
    }
    let mut captures: Sides<Vec<Piece>> =
        serde_json::from_value(field("captures")?).map_err(EngineError::serialization)?;
    // restoreFirstMoveUndo reconstructs these maps white-then-black even when
    // the admitted position was JCS-sorted black-then-white. Replay compares
    // JSON.stringify, so this order change is an observable delta.
    captures.white_first = true;
    let en_passant =
        serde_json::from_value(field("enPassant")?).map_err(EngineError::serialization)?;
    let selected = field("selected")?;
    let legal_moves = field("legalMoves")?;
    if !legal_moves.is_array() {
        return Err(EngineError::InvalidState(
            "firstMoveUndo.legalMoves must be an array".into(),
        ));
    }
    let king_dead = field("kingDead")?;
    let castled = field("castled")?;
    let captured_types = field("capturedTypes")?;
    let turn_captures = field("turnCaptures")?;
    for (name, value) in [
        ("kingDead", &king_dead),
        ("castled", &castled),
        ("capturedTypes", &captured_types),
        ("turnCaptures", &turn_captures),
    ] {
        if !value.is_object() {
            return Err(EngineError::InvalidState(format!(
                "firstMoveUndo.{name} must be an object"
            )));
        }
    }
    state.board = board;
    state.captures = captures;
    state.en_passant = en_passant;
    for (name, value) in [
        ("selected", selected),
        ("legalMoves", legal_moves),
        ("kingDead", king_dead),
        ("castled", castled),
        ("capturedTypes", captured_types),
        ("turnCaptures", turn_captures),
    ] {
        state.extra.insert(name.into(), value);
    }
    Ok(())
}

fn automatic_card_failure_message(effect: &str) -> Result<&'static str> {
    match effect {
        "guard" => Ok("킹 바로 앞에 근위병으로 바꿀 아군 폰이 없습니다."),
        "otherworld" => Ok("이세계로 보낼 아군 폰이 없습니다."),
        _ => Err(EngineError::UnsupportedFeature(format!(
            "first-move automatic failure message for {effect}"
        ))),
    }
}

// main87905: this happens after a successful first board move and before the
// owner-turn counter advances. It is separate from finishCard: an automatic
// card is marked used, but does not consume a card action or add progress.
fn resolve_first_move_cards(state: &mut GameState, actor: Color) -> Result<()> {
    crate::legal_profile::measure("end_move_auto_effects", || {
        resolve_first_move_cards_profiled(state, actor)
    })
}

fn resolve_first_move_cards_profiled(state: &mut GameState, actor: Color) -> Result<()> {
    if state.flag("firstMoveCardsForced", actor) || *state.turns_taken.get(actor) != 0 {
        return Ok(());
    }
    let cards = if state.ruleset_id == crate::RULES_VERSION_V7 {
        crate::v7_card_passive::first_move_preflight(state, actor)?
            .into_iter()
            .map(|slot| state.deck_slots.get(actor)[slot].clone())
            .collect::<Vec<_>>()
    } else {
        let definitions = crate::draft::definitions_for_ruleset(&state.ruleset_id)?;
        state
            .deck_slots
            .get(actor)
            .iter()
            .filter(|card| first_move_auto_card(card, definitions))
            .cloned()
            .collect::<Vec<_>>()
    };
    if cards.is_empty() {
        state.set_flag("firstMoveCardsForced", actor, true);
        return Ok(());
    }
    let mut resolved_card_count = 0usize;
    for card in cards {
        let v7 = state.ruleset_id == crate::RULES_VERSION_V7;
        // The v7 preflight owns unsupported special and compound targets.
        // The legacy automatic path remains limited to the two proven effects.
        if !v7
            && (!matches!(card.effect.as_str(), "otherworld" | "guard")
                || crate::observation::truth(card.extra.get("target")))
        {
            return Err(EngineError::UnsupportedFeature(format!(
                "first-move automatic card {}",
                card.id
            )));
        }
        let apply_once = |state: &mut GameState| -> Result<Vec<Piece>> {
            if v7 && crate::v7_card_passive::owns(&card.id) {
                return if crate::v7_card_passive::apply_virtual_effect(state, actor, &card)? {
                    Ok(Vec::new())
                } else {
                    Err(EngineError::IllegalAction)
                };
            }
            // Source forceFirstMoveCard selects the target while temporarily
            // assigning the opening card's owner as the active side. A retry
            // after first-move rollback draws a new target from the new board.
            let target = if v7 {
                crate::v7_card_passive::first_move_target(state, &card)?
            } else {
                None
            };
            let action = Action::card(actor, &card, target);
            apply_card_raw(state, &card, &action)
        };
        let previous_capture_locks = state
            .board
            .iter()
            .flatten()
            .flatten()
            .map(|piece| {
                (
                    piece.id.clone(),
                    piece.extra.get("freshNoCaptureUntil").cloned(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let previous_turn = state.turn;
        state.turn = actor;
        let mut effect = apply_once(state);
        state.turn = previous_turn;
        if matches!(effect, Err(EngineError::IllegalAction)) {
            let undo = (resolved_card_count == 0)
                .then(|| state.extra.get("firstMoveUndo"))
                .flatten()
                .filter(|undo| undo["color"] == json!(actor))
                .cloned();
            let name = card
                .extra
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("undefined");
            if let Some(undo) = undo {
                let completed_first_move = capture_first_move_undo(state, actor);
                restore_first_move_undo(state, &undo)?;
                state.turn = actor;
                effect = apply_once(state);
                state.turn = previous_turn;
                if effect.is_ok() {
                    crate::replay::add_log(
                        state,
                        format!(
                            "{} {name} 카드가 첫 이동에 막혀, 첫 이동을 취소하고 강제로 발동되었습니다.",
                            crate::replay::label(actor)
                        ),
                    )?;
                } else if matches!(effect, Err(EngineError::IllegalAction)) {
                    restore_first_move_undo(state, &completed_first_move)?;
                    crate::replay::add_log(
                        state,
                        format!(
                            "{} {name} 자동 발동 실패: {}",
                            crate::replay::label(actor),
                            if state.ruleset_id == RULES_VERSION_V7 {
                                crate::v7_card_passive::first_move_failure_message(
                                    state, actor, &card,
                                )?
                            } else {
                                automatic_card_failure_message(&card.effect)?.into()
                            }
                        ),
                    )?;
                    continue;
                }
            } else {
                crate::replay::add_log(
                    state,
                    format!(
                        "{} {name} 자동 발동 실패: {}",
                        crate::replay::label(actor),
                        if state.ruleset_id == RULES_VERSION_V7 {
                            crate::v7_card_passive::first_move_failure_message(state, actor, &card)?
                        } else {
                            automatic_card_failure_message(&card.effect)?.into()
                        }
                    ),
                )?;
                continue;
            }
        }
        effect?;
        // main5244/87943 restores the pre-card opening capture lock of every
        // surviving piece. A transformed guard keeps its pawn's former lock.
        for piece in state.board.iter_mut().flatten().flatten() {
            let current = piece.extra.get("freshNoCaptureUntil");
            if !current.and_then(Value::as_f64).is_some_and(f64::is_finite) {
                continue;
            }
            let previous = previous_capture_locks
                .get(&piece.id)
                .and_then(Option::as_ref);
            if current == previous {
                continue;
            }
            if let Some(previous) = previous {
                piece
                    .extra
                    .insert("freshNoCaptureUntil".into(), previous.clone());
            } else {
                piece.extra.shift_remove("freshNoCaptureUntil");
            }
        }
        crate::replay::queue_forced_opening_card(state, actor, &card)?;
        let used_at = crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?;
        let live_card = state
            .deck_slots
            .get_mut(actor)
            .iter_mut()
            .find(|candidate| candidate.instance_id == card.instance_id)
            .ok_or_else(|| EngineError::InvalidState("automatic card instance was lost".into()))?;
        live_card.used = true;
        live_card.extra.insert("usedAt".into(), json!(used_at));
        crate::flow::note_card_event(state)?;
        crate::replay::add_log(
            state,
            format!(
                "{} {} 카드가 첫 이동 후 강제로 발동되었습니다.",
                crate::replay::label(actor),
                card.extra
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("undefined")
            ),
        )?;
        resolved_card_count += 1;
    }
    state.extra.insert("firstMoveUndo".into(), Value::Null);
    state.set_flag("firstMoveCardsForced", actor, true);
    state.extra.insert("selected".into(), Value::Null);
    state.extra.insert("legalMoves".into(), json!([]));
    state.extra.insert("shotgunAction".into(), json!("move"));
    state.extra.insert("shotgunPreview".into(), json!([]));
    Ok(())
}

fn unique_board_pieces(state: &GameState) -> Vec<(Square, Piece)> {
    let mut seen = BTreeSet::new();
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter_map(|square| {
            state
                .at(square)
                .filter(|piece| seen.insert(piece.id.clone()))
                .cloned()
                .map(|piece| (square, piece))
        })
        .collect()
}

// The source decrements these owner-turn counters before turnsTaken advances.
// Expiring witch-trial removal uses the shared environmental capture family;
// that due callback is explicit until the complete reaction kernel is ported.
fn tick_piece_turn_effects(state: &mut GameState, actor: Color) -> Result<()> {
    for (square, mut piece) in unique_board_pieces(state) {
        if let Some(trial) = piece
            .extra
            .get("witchTrial")
            .filter(|v| crate::observation::truth(Some(v)))
        {
            let count_by =
                trial
                    .get("countBy")
                    .and_then(Value::as_str)
                    .and_then(|value| match value {
                        "white" => Some(Color::White),
                        "black" => Some(Color::Black),
                        _ => None,
                    });
            let count_color = if let Some(color) = count_by {
                if state.extra.get("september18Balance") == Some(&json!(false)) {
                    color
                } else {
                    color.opponent()
                }
            } else {
                piece.color.owner().unwrap_or(actor.opponent())
            };
            if count_color == actor {
                let remaining = decrement_remaining(&mut piece, "witchTrial", false)?;
                if remaining <= 0.0 {
                    return Err(EngineError::UnsupportedFeature(
                        "due witch-trial environmental capture".into(),
                    ));
                }
            }
        }
        if piece.color == actor {
            for field in ["disarmed", "staked", "severed", "iceSheet"] {
                if !crate::observation::truth(piece.extra.get(field)) {
                    continue;
                }
                if field == "severed"
                    && crate::observation::number(
                        piece.extra.get(field).and_then(|v| v.get("remaining")),
                    )
                    .is_none()
                {
                    return Err(EngineError::UnsupportedFeature(
                        "legacy full-move severance expiry".into(),
                    ));
                }
                if decrement_remaining(&mut piece, field, false)? <= 0.0 {
                    piece.extra.shift_remove(field);
                    if field == "staked" {
                        piece.extra.insert("shielded".into(), json!(true));
                        if piece
                            .extra
                            .get("potionEffects")
                            .and_then(Value::as_array)
                            .is_some_and(|effects| effects.contains(&json!("stake")))
                        {
                            crate::card_effects::note_potion_effect(&mut piece, "shield")?;
                        }
                        update_piece(state, &piece);
                        crate::card_effects::mark_animation(state, &piece)?;
                        crate::replay::add_piece_action_log(
                            state,
                            &piece,
                            Some(square),
                            None,
                            format!(
                                "말뚝: {}{}의 {}로 가호를 얻었습니다.",
                                char::from(b'a' + square.col),
                                8 - square.row,
                                crate::replay::piece_label(&piece.kind)
                            ),
                        )?;
                    }
                }
            }
        }
        update_piece(state, &piece);
    }
    crate::threat::tick_protection(state, actor, "lastResistance");
    Ok(())
}

fn decrement_remaining(piece: &mut Piece, field: &str, clamp: bool) -> Result<f64> {
    let entry = piece
        .extra
        .get_mut(field)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            EngineError::UnsupportedFeature(format!("non-object {field} turn counter"))
        })?;
    let previous = crate::observation::number(entry.get("remaining"))
        .ok_or_else(|| EngineError::UnsupportedFeature(format!("non-finite {field} remaining")))?;
    let remaining = if clamp {
        (previous - 1.0).max(0.0)
    } else {
        previous - 1.0
    };
    entry.insert("remaining".into(), json!(remaining));
    Ok(remaining)
}

fn tick_card_frozen_and_poison(state: &mut GameState, actor: Color) -> Result<()> {
    let mut thawed = 0;
    let mut recovered = 0;
    for (_, mut piece) in unique_board_pieces(state) {
        if let Some(frozen) = piece
            .extra
            .get("frozenByCard")
            .filter(|v| crate::observation::truth(Some(v)))
        {
            let explicit = frozen
                .get("countBy")
                .and_then(Value::as_str)
                .and_then(|value| match value {
                    "white" => Some(Color::White),
                    "black" => Some(Color::Black),
                    _ => None,
                });
            let color = explicit.map(Color::opponent).or(piece.color.owner());
            if color == Some(actor) && decrement_remaining(&mut piece, "frozenByCard", true)? <= 0.0
            {
                piece.extra.shift_remove("frozenByCard");
                let winter = state
                    .extra
                    .get("winterKingdom")
                    .and_then(|v| v.get("frozenIds"))
                    .and_then(Value::as_array)
                    .is_some_and(|ids| ids.contains(&json!(piece.id)));
                if !winter {
                    piece.extra.shift_remove("frozen");
                }
                thawed += 1;
            }
        }
        let poison = crate::observation::number(piece.extra.get("poisonStunTurns"))
            .unwrap_or(0.0)
            .floor()
            .max(0.0);
        let count_by = piece
            .extra
            .get("poisonStunColor")
            .and_then(Value::as_str)
            .and_then(|value| match value {
                "white" => Some(Color::White),
                "black" => Some(Color::Black),
                _ => None,
            });
        if poison > 0.0
            && count_by.map_or(
                piece.color == actor || piece.color == PieceColor::Neutral,
                |color| color == actor,
            )
        {
            let remaining = (crate::observation::number(piece.extra.get("poisonStunTurns"))
                .unwrap_or(0.0)
                - 1.0)
                .max(0.0);
            if remaining > 0.0 {
                piece
                    .extra
                    .insert("poisonStunTurns".into(), json!(remaining));
            } else {
                piece.extra.shift_remove("poisonStunTurns");
                piece.extra.shift_remove("poisonStunColor");
                recovered += 1;
            }
        }
        update_piece(state, &piece);
    }
    if thawed > 0 {
        crate::replay::add_log(state, format!("빙결: 기물 {thawed}개의 얼음이 녹았습니다."))?;
    }
    if recovered > 0 {
        crate::replay::add_log(
            state,
            format!(
                "독이 든 폰: {} 기물 {recovered}개가 다시 움직일 수 있습니다.",
                crate::replay::label(actor)
            ),
        )?;
    }
    Ok(())
}

fn clear_coronation_protection(state: &mut GameState, color: Color) {
    for (_, mut piece) in unique_board_pieces(state) {
        if piece.color != color
            || !crate::observation::truth(piece.extra.get("coronationProtection"))
        {
            continue;
        }
        let keep = crate::observation::truth(
            piece
                .extra
                .get("coronationProtection")
                .and_then(|v| v.get("previousProtected")),
        ) || [
            "lastResistance",
            "sacrificeProtection",
            "queensGambitProtection",
        ]
        .iter()
        .any(|field| crate::observation::truth(piece.extra.get(*field)));
        piece.extra.shift_remove("coronationProtection");
        if !keep {
            piece.extra.shift_remove("protected");
        }
        update_piece(state, &piece);
    }
}

/// Source renderAll/endMove prune one shared object per board identity.
/// Provenance is presentation data with a rule-state mutation boundary, while
/// the active trait itself is handled by its movement/capture/lifecycle kernel.
fn prune_board_potion_effects(state: &mut GameState) -> Result<()> {
    let mut seen = BTreeSet::new();
    let pieces = state
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|piece| seen.insert(piece.id.clone()))
        .cloned()
        .collect::<Vec<_>>();
    for mut piece in pieces {
        crate::card_effects::prune_potion_effects(&mut piece)?;
        update_piece(state, &piece);
    }
    Ok(())
}

fn apply_promotion(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    if state.ruleset_id == RULES_VERSION_V7 {
        // choosePromotion resumes the move's existing capture; it does not
        // begin a second capture at the promotion window.
        match crate::v7_promotion::apply_pending_promotion_choice_v7(state, action)? {
            crate::v7_promotion::V7PromotionContinuation::AtomicMove { action, .. } => {
                return apply_move(state, &action, false);
            }
            crate::v7_promotion::V7PromotionContinuation::Normal {
                actor,
                square,
                piece_id,
                promoted_type,
                field_promotion,
                terminal,
            } => {
                if !terminal
                    && !crate::v7_move_continuations::after_promotion_choice_v7(
                        state,
                        actor,
                        square,
                        &piece_id,
                        &promoted_type,
                        field_promotion,
                    )?
                {
                    end_move_for_decision(state, actor, true, Some("move"))?;
                }
                return Ok(Vec::new());
            }
        }
    }
    let pending = state
        .extra
        .get("pendingPromotion")
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    if pending.get("atomicMove").is_some() {
        return Err(EngineError::UnsupportedFeature(
            "UI atomic deferred promotion snapshot".into(),
        ));
    }
    let square: Square =
        serde_json::from_value(json!({"row":pending.get("row"),"col":pending.get("col")}))
            .map_err(EngineError::serialization)?;
    let kind = action
        .extra
        .get("promotionType")
        .and_then(Value::as_str)
        .ok_or(EngineError::IllegalAction)?;
    let piece = state.at_mut(square).ok_or(EngineError::IllegalAction)?;
    if piece.color != action.color {
        return Err(EngineError::WrongActor);
    }
    piece.kind = kind.into();
    piece.moved = true;
    piece
        .extra
        .insert("promotedFromPawn".into(), json!(kind != "pawn"));
    piece.extra.shift_remove("noPromotion");
    piece.extra.shift_remove("holdoutPromotion");
    piece.extra.shift_remove("vipInvitation");
    piece.extra.insert(
        "origin".into(),
        json!(format!(
            "{}{}",
            char::from(b'a' + square.col),
            8 - square.row
        )),
    );
    let lock = state.turns_taken.get(action.color).saturating_add(1);
    state
        .at_mut(square)
        .expect("promotion piece")
        .extra
        .insert("freshNoCaptureUntil".into(), json!(lock));
    state.extra.insert("pendingPromotion".into(), Value::Null);
    finish_move(state, action.color)?;
    Ok(Vec::new())
}

fn apply_card(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    let color = state.turn;
    let slot = state
        .deck_slots
        .get(color)
        .iter()
        .position(|card| {
            Some(&card.id) == action.card_id.as_ref()
                && Some(&card.instance_id) == action.card_instance_id.as_ref()
        })
        .ok_or(EngineError::IllegalAction)?;
    let card = state.deck_slots.get(color)[slot].clone();
    if state.ruleset_id == RULES_VERSION_V7 {
        crate::card_registry::action_policy(state, &card)?;
    }
    let captures = apply_card_raw(state, &card, action)?;
    // Potion and black-box effects can reveal metadata on this exact instance.
    // Source finishCard receives that updated object, including replay/log data.
    let updated_card = state.deck_slots.get(color)[slot].clone();
    finish_card(state, &updated_card, slot, captures)
}

pub(crate) fn apply_card_raw(
    state: &mut GameState,
    card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    if state.ruleset_id == RULES_VERSION_V7 {
        let context = begin_v7_card(state)?;
        let captures = if state.is_ai_simulation() {
            crate::card_effects::apply_simulation_effect(state, card, action)?
        } else {
            crate::card_effects::apply(state, card, action)?.ok_or_else(|| {
                EngineError::UnsupportedFeature(format!(
                    "v7 card effect {} has no source-pinned execution",
                    card.id
                ))
            })?
        };
        finish_v7_card(state, context)?;
        return Ok(captures);
    }
    let color = state.turn;
    let effect = &card.effect;
    let mut captures = Vec::new();
    match effect.as_str() {
        "genevaConvention" | "cornerKick" | "retreat" => state.set_flag(effect, color, true),
        "enPassantBang" => {
            if !state
                .board
                .iter()
                .flatten()
                .flatten()
                .any(|piece| piece.color == color && piece.kind == "pawn")
            {
                return Err(EngineError::IllegalAction);
            }
            if !crate::observation::truth(state.extra.get("enPassantFrenzy")) {
                state.extra.insert(
                    "enPassantFrenzy".into(),
                    json!({"white":false,"black":false}),
                );
            }
            state.set_flag("enPassantFrenzy", color, true);
        }
        "conversion" => {
            for cell in state.board.iter_mut().flatten().flatten() {
                if cell.color == color && cell.kind == "knight" {
                    cell.kind = "bishop".into();
                    cell.moved = true;
                }
            }
        }
        "reversal" => {
            let square: Square =
                serde_json::from_value(action.target.clone().ok_or(EngineError::IllegalAction)?)
                    .map_err(EngineError::serialization)?;
            let victim = state
                .at(square)
                .cloned()
                .ok_or(EngineError::IllegalAction)?;
            clear_piece(state, &victim.id);
            state
                .captures
                .get_mut(color.opponent())
                .push(victim.clone());
            captures.push(victim);
            state.set_flag("reversal", color, true);
        }
        "otherworld" => apply_otherworld(state)?,
        _ => {
            captures = crate::card_effects::apply(state, card, action)?
                .ok_or_else(|| EngineError::UnsupportedFeature(format!("card {effect}")))?;
        }
    }
    refresh_submerged(state)?;
    Ok(captures)
}

fn finish_card(
    state: &mut GameState,
    card: &CardSlot,
    slot: usize,
    captures: Vec<Piece>,
) -> Result<Vec<Piece>> {
    let color = state.turn;
    let v7_settle = if state.ruleset_id == RULES_VERSION_V7 {
        Some(crate::card_registry::settle_policy(state, card)?)
    } else {
        None
    };
    // The source copies a roulette result into an online effect event, then
    // removes only that transient from the live card before marking it used
    // and recording the card replay snapshot. Online transport is outside
    // GameState, so retain the copied value only for validation here.
    let (card, online_effect_payload) =
        crate::card_registry::consume_finish_card_transient(state, card, slot)?;
    let card = &card;
    if state.ruleset_id == RULES_VERSION_V7
        && online_effect_payload.is_some()
        && state.ai_simulation_depth == 0
    {
        // main106809: the headless presentation draws two display types,
        // pauses its running main clock, then draws the initial cursor.
        // Its timer is a profile noop; the paused clock is part of Position.
        for _ in 0..2 {
            state
                .rng
                .sample_invariant("roulette presentation display")?;
        }
        if state.extra.get("clock").is_some_and(|clock| {
            clock["enabled"] == true
                && matches!(clock["runningColor"].as_str(), Some("white" | "black"))
        }) {
            crate::flow::pause_clock(state)?;
        }
        state.rng.sample_invariant("roulette presentation cursor")?;
    }
    if card.extra.get("devCard") != Some(&json!(true)) {
        let used_at = crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?;
        if state.ruleset_id == RULES_VERSION_V7 && card.id == "blood" {
            crate::v7_campaign::consume_blood_card(state, card)?;
        } else {
            state.deck_slots.get_mut(color)[slot].used = true;
            state.deck_slots.get_mut(color)[slot]
                .extra
                .insert("usedAt".into(), json!(used_at));
        }
        state.cards_used_this_turn = Sides::new(
            state.cards_used_this_turn.white,
            state.cards_used_this_turn.black,
        );
        let used = state.cards_used_this_turn.get_mut(color);
        *used = used
            .checked_add(1)
            .ok_or_else(|| EngineError::InvalidState("card count overflow".into()))?;
        crate::flow::note_card_event(state)?;
    }
    if !crate::draft::is_passive_definition_for_ruleset(
        &state.ruleset_id,
        &serde_json::to_value(card).map_err(EngineError::serialization)?,
    )? && card.extra.get("phase").and_then(Value::as_str) != Some("RULE")
    {
        crate::flow::mark_progress(state);
    }
    for field in [
        "selected",
        "targeting",
        "barricadePreview",
        "barricadeDirectionChoice",
    ] {
        state.extra.insert(field.into(), Value::Null);
    }
    state.extra.insert("legalMoves".into(), json!([]));
    crate::replay::add_log(
        state,
        format!(
            "카드: {}",
            card.extra
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("undefined")
        ),
    )?;
    crate::replay::queue_card(state, color, card)?;
    state.extra.insert("ruleTicketChoice".into(), Value::Null);
    state.extra.insert("jokerChoice".into(), Value::Null);
    if state.ruleset_id == RULES_VERSION_V7 {
        crate::v7_board_hazards::post_card(state, color)?;
    } else {
        refresh_submerged(state)?;
    }
    if state.ruleset_id != RULES_VERSION_V7
        && state
            .extra
            .get("blackHole")
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty())
    {
        return Err(EngineError::UnsupportedFeature(
            "post-card black-hole deaths".into(),
        ));
    }
    update_palaces(state)?;
    resolve_herald_threats(state, color)?;
    if state.ruleset_id == RULES_VERSION_V7
        && state.mode != "gameover"
        && crate::v7_passive_terminal::after_active_card_settle(state)?
    {
        // The frozen active-card wrapper records a terminal card frame here,
        // before star-limit and any turn-ending callback.
        crate::replay::record(state, "card")?;
        return Ok(captures);
    }
    crate::flow::check_star_limit(state)?;
    if let Some(settle) = v7_settle {
        if settle.end_move {
            if settle.clear_extra_actions {
                state.actions_remaining = 1;
                if let Some(effects) = state
                    .extra
                    .get_mut("effects")
                    .and_then(Value::as_object_mut)
                {
                    effects.insert("extraMove".into(), json!(0));
                }
            }
            finish_move(state, color)?;
        } else {
            crate::flow::check_no_action_loss(state)?;
        }
    } else if card.id == "brainwash" && state.mode == "play" {
        // main86914/86999: brainwash consumes the rest of its owner's turn,
        // including extra actions, before recording the card replay frame.
        state.actions_remaining = 1;
        if let Some(effects) = state
            .extra
            .get_mut("effects")
            .and_then(Value::as_object_mut)
        {
            effects.insert("extraMove".into(), json!(0));
        }
        finish_move(state, color)?;
    } else {
        crate::flow::check_no_action_loss(state)?;
    }
    crate::replay::record(state, "card")?;
    Ok(captures)
}

/// The simulated source card availability shares global randomness with the
/// real position, although its mutated board is discarded. This is different
/// from the public immutable rule-query API, whose probe RNG stays isolated.
pub(crate) fn available_card_action(state: &mut GameState, color: Color) -> Result<bool> {
    // draftDelete is a game-wide source setting, not a per-side status flag.
    if crate::observation::truth(state.extra.get("draftDelete")) {
        return Ok(false);
    }
    // main:94720의 조기 반환 이후 playerDeck(color)를 실제로 읽는 위치다.
    let cards = if state.ruleset_id == RULES_VERSION_V7 {
        crate::draft::v7_player_deck(state, color)?.clone()
    } else {
        state.deck_slots.get(color).clone()
    };
    if state.ruleset_id == RULES_VERSION_V7 {
        for card in &cards {
            if crate::v7_ai_card_candidates::is_v7_ai_card_playable(state, card, color)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    for card in cards {
        if card.vacant
            || card.used
            || card.recovering
            || crate::observation::truth(card.extra.get("devCard"))
            || crate::observation::truth(card.extra.get("nextTurnPending"))
        {
            continue;
        }
        let mut probe = state.clone();
        probe.turn = color;
        if state.ruleset_id == RULES_VERSION_V7 {
            let definition = crate::card_registry::validate_instance(&probe, &card)?;
            if definition.activation != Some(crate::card_registry::CardActType::Active)
                || definition.card_type == Some(crate::card_registry::CardType::Rule)
            {
                continue;
            }
            // The source probes cards before moves. An unowned v7 effect must
            // stop that predicate before its legacy handler can claim an
            // available action or consume the position's RNG.
            match crate::card_registry::action_policy(&probe, &card) {
                Ok(_) => {}
                Err(EngineError::IllegalAction) => continue,
                Err(error) => return Err(error),
            }
        }
        if crate::observation::truth(card.extra.get("target")) {
            if !card_ui_actions(&probe, &card)?.is_empty() {
                return Ok(true);
            }
            continue;
        }
        for field in ["selected", "targeting"] {
            probe.extra.insert(field.into(), Value::Null);
        }
        probe.extra.insert("legalMoves".into(), json!([]));
        let result = apply_card_raw(&mut probe, &card, &Action::card(color, &card, None));
        state.rng = probe.rng;
        match result {
            Ok(_) => return Ok(true),
            Err(EngineError::IllegalAction) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

pub(crate) fn apply_otherworld(state: &mut GameState) -> Result<()> {
    let color = state.turn;
    let mut seen = BTreeSet::new();
    let pawns = state
        .board
        .iter()
        .enumerate()
        .flat_map(|(row, cells)| {
            cells.iter().enumerate().filter_map(move |(col, piece)| {
                piece
                    .as_ref()
                    .filter(|piece| piece.color == color && piece.kind == "pawn")
                    .map(|piece| {
                        (
                            Square {
                                row: row as u8,
                                col: col as u8,
                            },
                            piece.clone(),
                        )
                    })
            })
        })
        .filter(|(_, piece)| seen.insert(piece.id.clone()))
        .collect::<Vec<_>>();
    if pawns.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let index = sample_choice(state, pawns.len())?;
    let (square, piece) = &pawns[index];
    let suffix =
        crate::draft::random_suffix(state.rng.sample_opaque("otherworld return identity")?)?
            .chars()
            .take(6)
            .collect::<String>();
    let due = state
        .move_count
        .checked_add(28)
        .ok_or_else(|| EngineError::InvalidState("otherworld return counter overflow".into()))?;
    let origin = piece
        .extra
        .get("origin")
        .filter(|value| crate::observation::truth(Some(value)))
        .cloned()
        .unwrap_or_else(|| {
            json!(format!(
                "{}{}",
                char::from(b'a' + square.col),
                8 - square.row
            ))
        });
    let entry = json!({"id":format!("otherworld-{}-{suffix}",crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?),
        "color":color,"pieceId":piece.id,"row":square.row,"col":square.col,
        "origin":origin,"dueMoveCount":due,"remainingHalfTurns":28});
    clear_piece(state, &piece.id);
    if !state
        .extra
        .get("pendingOtherworld")
        .is_some_and(Value::is_array)
    {
        state.extra.insert("pendingOtherworld".into(), json!([]));
    }
    state
        .extra
        .get_mut("pendingOtherworld")
        .expect("normalized otherworld")
        .as_array_mut()
        .expect("array")
        .push(entry);
    crate::card_effects::mark_vanish_animation(state, piece, *square)?;
    Ok(())
}

/// Source uniform semantic choice, with the density of the realized outcome.
/// Opaque identity draws use RNG directly. A clone-only availability probe
/// discards this execution trace even though its source RNG cursor is retained.
pub(crate) fn sample_choice(state: &mut GameState, count: usize) -> Result<usize> {
    if !(1..=4096).contains(&count) {
        return Err(EngineError::InvalidState(
            "semantic chance pool must contain 1..=4096 outcomes".into(),
        ));
    }
    let index = (state.rng.sample()? * count as f64).floor() as usize;
    state
        .rng
        .record_last_probability(1.0 / count as f64, "uniform source choice")?;
    if let Some(probability) = &mut state.semantic_chance_probability {
        *probability /= count as f64;
        if !probability.is_finite() || *probability <= 0.0 {
            return Err(EngineError::InvalidState(
                "semantic chance trace density is not finite and positive".into(),
            ));
        }
    }
    Ok(index)
}

pub(crate) fn sample_weighted(state: &mut GameState, weights: &[f64]) -> Result<usize> {
    if weights.is_empty()
        || weights.len() > 4096
        || weights
            .iter()
            .any(|weight| !weight.is_finite() || *weight < 0.0)
    {
        return Err(EngineError::InvalidState(
            "invalid weighted chance pool".into(),
        ));
    }
    let total = weights.iter().sum::<f64>();
    if total <= 0.0 {
        return sample_choice(state, weights.len());
    }
    if !total.is_finite() {
        return Err(EngineError::InvalidState(
            "weighted chance total overflow".into(),
        ));
    }
    let mut roll = state.rng.sample()? * total;
    let mut selected = weights.len() - 1;
    for (index, weight) in weights.iter().enumerate() {
        roll -= weight;
        if roll <= 0.0 {
            selected = index;
            break;
        }
    }
    if let Some(probability) = &mut state.semantic_chance_probability {
        *probability *= weights[selected] / total;
        if !probability.is_finite() || *probability <= 0.0 {
            return Err(EngineError::InvalidState(
                "invalid weighted semantic trace density".into(),
            ));
        }
    }
    state
        .rng
        .record_last_probability(weights[selected] / total, "weighted source choice")?;
    Ok(selected)
}

// main99042 advances the explicit half-turn scheduler on every completed
// board move. Due returns need their spawn/crush/collapse/notation callbacks;
// they are explicit pending work rather than silently discarded plans.
fn advance_otherworld(state: &mut GameState) -> Result<()> {
    if state.ruleset_id == RULES_VERSION_V7 {
        crate::v7_otherworld_returns::resolve_after_move(state)?;
        return Ok(());
    }
    let Some(entries) = state
        .extra
        .get("pendingOtherworld")
        .and_then(Value::as_array)
    else {
        return Ok(());
    };
    if entries.is_empty() {
        return Ok(());
    }
    let mut advanced = entries.clone();
    for entry in &mut advanced {
        let remaining = crate::observation::number(entry.get("remainingHalfTurns"));
        let due = if let Some(remaining) = remaining.filter(|number| {
            number.fract() == 0.0 && *number >= 0.0 && *number <= 9_007_199_254_740_991.0
        }) {
            let next = (remaining - 1.0).max(0.0);
            entry
                .as_object_mut()
                .ok_or_else(|| {
                    EngineError::InvalidState("otherworld scheduler entry must be an object".into())
                })?
                .insert("remainingHalfTurns".into(), json!(next as u64));
            next == 0.0
        } else {
            f64::from(state.move_count)
                >= crate::observation::number(entry.get("dueMoveCount")).unwrap_or(0.0)
        };
        if due {
            return Err(EngineError::UnsupportedFeature(
                "scheduled otherworld return".into(),
            ));
        }
    }
    state
        .extra
        .insert("pendingOtherworld".into(), Value::Array(advanced));
    Ok(())
}

pub(crate) fn refresh_submerged(state: &mut GameState) -> Result<()> {
    refresh_submerged_with_options(state, false)
}

pub(crate) fn refresh_submerged_with_options(
    state: &mut GameState,
    reveal_only: bool,
) -> Result<()> {
    if state.ruleset_id == RULES_VERSION_V7 && !reveal_only {
        crate::v7_piece_lifecycle::resolve_recurrences(state)?;
        crate::v7_card_context::resolve_board_infiltration(state)?;
    }
    if state.ruleset_id != RULES_VERSION_V7
        && state
            .extra
            .get("pendingRecurrences")
            .and_then(Value::as_array)
            .is_some_and(|entries| !entries.is_empty())
    {
        return Err(EngineError::UnsupportedFeature(
            "board recurrence settlement".into(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut revealed = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let from = Square { row, col };
            let Some(piece) = state
                .at(from)
                .filter(|p| crate::observation::truth(p.extra.get("submerged")))
            else {
                continue;
            };
            if !seen.insert(piece.id.clone()) {
                continue;
            }
            let Some(owner) = piece.color.owner() else {
                continue;
            };
            let origin = if piece.is_large() {
                Square {
                    row: piece
                        .extra
                        .get("anchorRow")
                        .and_then(Value::as_u64)
                        .unwrap_or(u64::from(row)) as u8,
                    col: piece
                        .extra
                        .get("anchorCol")
                        .and_then(Value::as_u64)
                        .unwrap_or(u64::from(col)) as u8,
                }
            } else {
                from
            };
            if crate::movement::KING
                .iter()
                .filter_map(|&(dr, dc)| origin.offset(dr, dc))
                .any(|square| {
                    state.at(square).is_some_and(|neighbor| {
                        neighbor.id != piece.id && neighbor.color == owner.opponent()
                    })
                })
            {
                revealed.push((origin, piece.clone()));
            }
        }
    }
    for (origin, mut piece) in revealed {
        piece.extra.shift_remove("submerged");
        piece.source_order.retain(|key| key != "submerged");
        update_piece(state, &piece);
        crate::card_effects::mark_animation(state, &piece)?;
        if state.ruleset_id == RULES_VERSION_V7 {
            crate::replay::add_piece_action_log(
                state,
                &piece,
                Some(origin),
                None,
                format!(
                    "잠복 해제: {}{}의 {}이 발각되었습니다.",
                    char::from(b'a' + origin.col),
                    8 - origin.row,
                    crate::replay::source_piece_label(&piece.kind).unwrap_or(&piece.kind)
                ),
            )?;
            continue;
        }
        let concealed = [Color::White, Color::Black]
            .into_iter()
            .any(|viewer| !state.piece_visible(&piece, origin, viewer));
        crate::replay::add_log(
            state,
            if concealed {
                "기물이 행동했습니다.".into()
            } else {
                format!(
                    "잠복 해제: {}{}의 {}이 발각되었습니다.",
                    char::from(b'a' + origin.col),
                    8 - origin.row,
                    crate::replay::piece_label(&piece.kind)
                )
            },
        )?;
    }
    Ok(())
}

fn herald_victory(state: &GameState, color: Color) -> bool {
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .any(|from| {
            let Some(piece) = state.at(from).filter(|p| p.color == color) else {
                return false;
            };
            if piece.ability_kind() != "herald"
                && !(piece.kind == "trickster"
                    && piece
                        .extra
                        .get("tricksterPreviousAbilityForTurn")
                        .and_then(Value::as_str)
                        == Some("herald"))
            {
                return false;
            }
            crate::movement::KING
                .iter()
                .filter_map(|&(dr, dc)| from.offset(dr, dc))
                .any(|to| {
                    state.at(to).is_some_and(|target| {
                        target.color == color.opponent()
                            && (state.royal_identity(target) || target.kind == "merchant")
                    })
                })
        })
}
pub(crate) fn resolve_herald_for_color(state: &mut GameState, color: Color) -> Result<bool> {
    if state.ruleset_id == RULES_VERSION_V7 {
        return crate::v7_threat::resolve_herald_for_color_v7(state, color);
    }
    if herald_victory(state, color) {
        crate::flow::end_game(
            state,
            Some(color),
            "전령이 상대 킹과 협정을 이끌어냈습니다.",
        )?;
        return Ok(true);
    }
    Ok(false)
}
pub(crate) fn resolve_herald_threats(state: &mut GameState, actor: Color) -> Result<bool> {
    if state.ruleset_id == RULES_VERSION_V7 {
        return crate::v7_threat::resolve_herald_threats_v7_with_probe(
            state,
            actor,
            state.threat_probe_depth > 0,
        );
    }
    if state.result().is_some() || resolve_herald_for_color(state, actor)? {
        return Ok(true);
    }
    if herald_victory(state, actor.opponent()) {
        crate::flow::end_game(
            state,
            Some(actor.opponent()),
            "상대 킹이 전령의 협정권에 들어왔습니다.",
        )?;
        return Ok(true);
    }
    if [Color::White, Color::Black]
        .into_iter()
        .any(|color| state.flag("racingKing", color) || state.flag("binaMate", color))
    {
        return Err(EngineError::UnsupportedFeature(
            "racing-king/double-check threat resolution".into(),
        ));
    }
    Ok(false)
}
pub(crate) fn mark_card_no_capture(state: &GameState, piece: &mut Piece) -> Result<()> {
    let turns = piece
        .color
        .owner()
        .map(|color| *state.turns_taken.get(color))
        .unwrap_or(0);
    let deadline = turns
        .checked_add(1)
        .ok_or_else(|| EngineError::InvalidState("card capture lock overflow".into()))?;
    piece
        .extra
        .insert("cardNoCaptureUntil".into(), json!(deadline));
    Ok(())
}

fn update_palaces(state: &mut GameState) -> Result<()> {
    if state.ruleset_id == RULES_VERSION_V7 {
        return crate::v7_board_hazards::update_palaces(state);
    }
    let Some(palaces) = state.extra.get("palaces") else {
        return Ok(());
    };
    let palaces = palaces
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("palaces must be an array".into()))?
        .clone();
    let mut retained = Vec::new();
    for palace in palaces {
        let color: Color = serde_json::from_value(
            palace
                .get("color")
                .cloned()
                .ok_or_else(|| EngineError::InvalidState("palace color missing".into()))?,
        )
        .map_err(EngineError::serialization)?;
        let king = (0..8)
            .flat_map(|row| (0..8).map(move |col| Square { row, col }))
            .find(|&square| {
                state
                    .at(square)
                    .is_some_and(|p| p.color == color && state.royal_identity(p))
            });
        let Some(king) = king else {
            continue;
        };
        let cells: Vec<Square> = serde_json::from_value(
            palace
                .get("cells")
                .cloned()
                .ok_or_else(|| EngineError::InvalidState("palace cells missing".into()))?,
        )
        .map_err(EngineError::serialization)?;
        if cells.contains(&king) {
            retained.push(palace);
        } else {
            crate::replay::add_log(
                state,
                format!(
                    "{} 킹이 궁성 밖으로 탈출해 궁성이 무너졌습니다.",
                    crate::replay::label(color)
                ),
            )?;
        }
    }
    state.extra.insert("palaces".into(), Value::Array(retained));
    Ok(())
}
