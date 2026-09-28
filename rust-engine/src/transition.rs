use crate::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn card_actions(state: &GameState, card: &CardSlot) -> Result<Vec<Action>> {
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
    let stored = &mut state.deck_slots.get_mut(color)[slot];
    stored.used = true;
    stored.recovering = false;
    stored.source_order.retain(|name| name != "recovering");
    stored.extra.insert("passiveApplied".into(), json!(true));
    stored
        .extra
        .insert("usedAt".into(), json!(crate::draft::frozen_timestamp()?));
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

pub(crate) fn apply(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    let actor = action.color;
    let before = Sides {
        white: state.observe(Color::White),
        black: state.observe(Color::Black),
        white_first: true,
    };
    let captures = match action.kind {
        ActionKind::Move => apply_move(state, action, false)?,
        ActionKind::Card => apply_card(state, action)?,
        ActionKind::PromotionChoice => apply_promotion(state, action)?,
        ActionKind::DraftPick | ActionKind::DraftBundlePick => {
            crate::draft::apply_pick(state, action)?
        }
        other => return Err(EngineError::UnsupportedFeature(format!("action {other:?}"))),
    };
    prune_board_potion_effects(state)?;
    crate::replay::settle(state)?;
    let transition = |viewer| {
        let after = state.observe(viewer);
        let before = before.get(viewer);
        let mut board_changes = Vec::new();
        for row in 0..8 {
            for col in 0..8 {
                if before.board[row][col] != after.board[row][col] {
                    board_changes.push(BoardChange {
                        square: Square {
                            row: row as u8,
                            col: col as u8,
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
        PublicTransition {
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
        }
    };
    let mut exact = action.clone();
    exact.position_key = None;
    let event = PublicEvent {
        protocol_version: "accelerate-game-event-v1".into(),
        actor,
        action: exact,
        turn_changed: state.turn != before.white.turn,
        public: Sides {
            white: transition(Color::White),
            black: transition(Color::Black),
            white_first: true,
        },
    };
    state
        .history
        .push(serde_json::to_value(event).expect("game event serializes"));
    Ok(captures)
}

pub(crate) fn execute_threat_move(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    apply_move(state, action, true)
}

fn apply_move(state: &mut GameState, action: &Action, threat_probe: bool) -> Result<Vec<Piece>> {
    let from = action.from.ok_or(EngineError::IllegalAction)?;
    let target = action
        .destination
        .as_ref()
        .ok_or(EngineError::IllegalAction)?;
    let to = target.square();
    let mut piece = state.at(from).cloned().ok_or(EngineError::IllegalAction)?;
    let original_piece = piece.clone();
    // movePiece stores this immediately before changing the first board move.
    // The automatic OPENING card may restore only these fields if that move
    // made its effect impossible. It deliberately leaves clock, RNG and move
    // bookkeeping at their post-move values.
    if !threat_probe && should_store_first_move_undo(state, action.color) {
        let undo = capture_first_move_undo(state, action.color);
        state.extra.insert("firstMoveUndo".into(), undo);
    }
    let replay_before = crate::replay::begin_move(state, action.color)?;
    let actor = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let original_type = piece.kind.clone();
    let mut captures = Vec::new();
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
            capture(state, &piece, victim, &mut captures)?;
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
            capture(state, &piece, victim, &mut captures)?;
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
                capture(state, &piece, victim, &mut captures)?;
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
            capture(state, &piece, victim, &mut captures)?;
        }
        if state.at(to).is_some() {
            return Err(EngineError::UnsupportedFeature(
                "capture leaves occupied landing".into(),
            ));
        }
        clear_piece(state, &piece.id);
        piece.moved = true;
        piece
            .extra
            .insert("coolGuyCapturedLast".into(), json!(!captures.is_empty()));
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
        replay_interrupted = crate::threat::play_move_sound(state, sound, actor)?;
    }
    // main92263–92307: the threat-sound probe observes bindings before the
    // moving piece releases its allies through trackMovingProgress.
    crate::replay::track_moving(state, &piece)?;
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
    if !replay_interrupted {
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
    Ok(())
}

fn add_capture_type(state: &mut GameState, field: &str, owner: Color, kind: &str) -> Result<()> {
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
    if state.flag("recycling", victim.color) && victim.kind == "queen" {
        return Err(EngineError::UnsupportedFeature(
            "recycling royal-loss promotions".into(),
        ));
    }
    if state.flag("regency", victim.color)
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
pub(crate) fn sacrifice(
    state: &mut GameState,
    square: Square,
    capture_color: Color,
) -> Result<Option<Piece>> {
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
fn update_piece(state: &mut GameState, piece: &Piece) {
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
    prune_board_potion_effects(state)?;
    crate::replay::normalize_color_booleans(state, "skipTurn");
    if state.actions_remaining > 1 {
        state.actions_remaining -= 1;
        return Ok(());
    }
    if !crate::flow::commit_turn_clock(state, actor)? {
        return Ok(());
    }
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
    state.move_count = state
        .move_count
        .checked_add(1)
        .ok_or_else(|| EngineError::InvalidState("move count overflow".into()))?;
    advance_otherworld(state)?;
    tick_piece_turn_effects(state, actor)?;
    if state.extra.contains_key("enPassantFrenzy") {
        state.set_flag("enPassantFrenzy", actor, false);
    }
    resolve_first_move_cards(state, actor)?;
    let turns = state.turns_taken.get_mut(actor);
    *turns = turns
        .checked_add(1)
        .ok_or_else(|| EngineError::InvalidState("turn count overflow".into()))?;
    crate::threat::tick_protection(state, actor, "sacrificeProtection");
    tick_card_frozen_and_poison(state, actor)?;
    if crate::flow::tick_deathmatch(state, actor)? {
        return Ok(());
    }
    if actor == Color::Black {
        state.full_move = state
            .full_move
            .checked_add(1)
            .ok_or_else(|| EngineError::InvalidState("full move count overflow".into()))?;
    }
    crate::replay::normalize_winter_after_turn(state)?;
    state.cards_used_this_turn = Sides::new(
        state.cards_used_this_turn.white,
        state.cards_used_this_turn.black,
    );
    *state.cards_used_this_turn.get_mut(actor) = 0;
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
    let refresh = state.extra["magicGirlSurgeRefreshPending"][actor.as_str()].clone();
    state
        .extra
        .get_mut("magicGirlSurge")
        .expect("normalized colors")[actor.as_str()] = refresh;
    state
        .extra
        .get_mut("magicGirlSurgeRefreshPending")
        .expect("normalized colors")[actor.as_str()] = json!(false);
    if state.extra.contains_key("reversal") {
        state.set_flag("reversal", actor, false);
    }
    state.turn = actor.opponent();
    let incoming = state.turn;
    // main93664-93667 resolves incoming-turn reservations after the actor
    // switch. VIP and ICBM pending contexts remain explicit unsupported
    // movement states; ordinary Portal Gun reservations settle here.
    crate::card_effects::resolve_pending_portals_for_turn(state, incoming)?;
    clear_coronation_protection(state, state.turn);
    state.actions_remaining = if state.flag("acceleration", state.turn) {
        2
    } else {
        1
    };
    crate::flow::start_clock(state)?;
    if crate::flow::check_termination(state)? {
        crate::flow::pause_clock(state)?;
    }
    crate::flow::check_no_action_loss(state)?;
    Ok(())
}

fn first_move_auto_card(card: &CardSlot) -> bool {
    if card.vacant || card.used || !crate::observation::truth(card.extra.get("firstTurnCard")) {
        return false;
    }
    // libraryCardPhase prefers the frozen definition over the stored card's
    // mutable phase. The two exceptional IDs are also accepted by the source.
    let phase = crate::draft::definitions()
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

fn should_store_first_move_undo(state: &GameState, actor: Color) -> bool {
    !state.flag("firstMoveCardsForced", actor)
        && *state.turns_taken.get(actor) == 0
        && state.deck_slots.get(actor).iter().any(first_move_auto_card)
}

// main88121 captures exactly the fields restored by main90846. The remaining
// state (including the clock, RNG, moveCount, logs and replay capture) stays
// advanced even when the first board move is canceled.
fn capture_first_move_undo(state: &GameState, actor: Color) -> Value {
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
    if state.flag("firstMoveCardsForced", actor) || *state.turns_taken.get(actor) != 0 {
        return Ok(());
    }
    let cards = state
        .deck_slots
        .get(actor)
        .iter()
        .filter(|card| first_move_auto_card(card))
        .cloned()
        .collect::<Vec<_>>();
    if cards.is_empty() {
        state.set_flag("firstMoveCardsForced", actor, true);
        return Ok(());
    }
    let mut resolved_card_count = 0usize;
    for card in cards {
        // The targeted cards, shotgun opening and failed-effect move rollback
        // have distinct source branches. Do not count them as successful uses.
        if !matches!(card.effect.as_str(), "otherworld" | "guard")
            || crate::observation::truth(card.extra.get("target"))
        {
            return Err(EngineError::UnsupportedFeature(format!(
                "first-move automatic card {}",
                card.id
            )));
        }
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
        let mut effect = apply_card_raw(state, &card, &Action::card(actor, &card, None));
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
                effect = apply_card_raw(state, &card, &Action::card(actor, &card, None));
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
                            automatic_card_failure_message(&card.effect)?
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
                        automatic_card_failure_message(&card.effect)?
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
        let live_card = state
            .deck_slots
            .get_mut(actor)
            .iter_mut()
            .find(|candidate| candidate.instance_id == card.instance_id)
            .ok_or_else(|| EngineError::InvalidState("automatic card instance was lost".into()))?;
        live_card.used = true;
        live_card
            .extra
            .insert("usedAt".into(), json!(crate::draft::frozen_timestamp()?));
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
    let captures = apply_card_raw(state, &card, action)?;
    // Potion and black-box effects can reveal metadata on this exact instance.
    // Source finishCard receives that updated object, including replay/log data.
    let updated_card = state.deck_slots.get(color)[slot].clone();
    finish_card(state, &updated_card, slot, captures)
}

fn apply_card_raw(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
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
    if card.extra.get("devCard") != Some(&json!(true)) {
        state.deck_slots.get_mut(color)[slot].used = true;
        state.deck_slots.get_mut(color)[slot]
            .extra
            .insert("usedAt".into(), json!(crate::draft::frozen_timestamp()?));
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
    if !crate::draft::is_passive_definition(
        &serde_json::to_value(card).map_err(EngineError::serialization)?,
    ) && card.extra.get("phase").and_then(Value::as_str) != Some("RULE")
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
    refresh_submerged(state)?;
    if state
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
    crate::flow::check_star_limit(state)?;
    if card.id == "brainwash" && state.mode == "play" {
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
    if state.flag("draftDelete", color) {
        return Ok(false);
    }
    let cards = state.deck_slots.get(color).clone();
    for card in cards {
        if card.vacant
            || card.used
            || card.recovering
            || crate::observation::truth(card.extra.get("devCard"))
            || crate::observation::truth(card.extra.get("nextTurnPending"))
        {
            continue;
        }
        if crate::observation::truth(card.extra.get("target")) {
            let mut probe = state.clone();
            probe.turn = color;
            if !card_ui_actions(&probe, &card)?.is_empty() {
                return Ok(true);
            }
            continue;
        }
        let mut probe = state.clone();
        probe.turn = color;
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

fn apply_otherworld(state: &mut GameState) -> Result<()> {
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
    let suffix = crate::draft::random_suffix(state.rng.sample()?)?
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
    let entry = json!({"id":format!("otherworld-{}-{suffix}",crate::draft::frozen_timestamp()?),
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
    Ok(selected)
}

// main99042 advances the explicit half-turn scheduler on every completed
// board move. Due returns need their spawn/crush/collapse/notation callbacks;
// they are explicit pending work rather than silently discarded plans.
fn advance_otherworld(state: &mut GameState) -> Result<()> {
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
    if state
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
            let Some(piece) = state.at(from).filter(|p| p.flag("submerged")) else {
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
        update_piece(state, &piece);
        crate::card_effects::mark_animation(state, &piece)?;
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
