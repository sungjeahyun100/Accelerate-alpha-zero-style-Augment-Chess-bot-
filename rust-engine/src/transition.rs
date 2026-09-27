use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

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
        "genevaConvention" | "cornerKick" | "retreat" => {
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

pub(crate) fn apply(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    let actor = action.color;
    let before = Sides {
        white: state.observe(Color::White),
        black: state.observe(Color::Black),
        white_first: true,
    };
    let captures = match action.kind {
        ActionKind::Move => apply_move(state, action)?,
        ActionKind::Card => apply_card(state, action)?,
        ActionKind::PromotionChoice => apply_promotion(state, action)?,
        ActionKind::DraftPick | ActionKind::DraftBundlePick => {
            crate::draft::apply_pick(state, action)?
        }
        other => return Err(EngineError::UnsupportedFeature(format!("action {other:?}"))),
    };
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

fn apply_move(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    let from = action.from.ok_or(EngineError::IllegalAction)?;
    let target = action
        .destination
        .as_ref()
        .ok_or(EngineError::IllegalAction)?;
    let to = target.square();
    let mut piece = state.at(from).cloned().ok_or(EngineError::IllegalAction)?;
    let original_piece = piece.clone();
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
    crate::replay::track_moving(state, &piece)?;
    state.extra.insert(
        "lastMove".into(),
        json!({"from":from,"to":to,"pieceId":piece.id,"pieceType":original_type,
            "soundName":if target.flag("castle"){"castle"}else if captures.is_empty(){if actor==Color::White {"moveSelf"} else {"moveOpponent"}}else{"capture"},
            "soundColor":actor,"hiddenFrom":piece.extra.get("hiddenFrom").and_then(Value::as_str).unwrap_or(""),
            "idolEncoreEligible":false,"idolEncoreId":"","idolEncorePieceId":"","idolEncoreConsumed":false}),
    );
    // queueMoveHistoryNotation creates its identifier before promotion and turn
    // settlement, sharing the source random stream with later rule draws.
    crate::replay::queue_move(
        state,
        &replay_before,
        &original_piece,
        from,
        to,
        target,
        !captures.is_empty(),
    )?;
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
        finish_move(state, actor)?;
    }
    crate::replay::commit_move(state, &replay_before, actor)?;
    crate::replay::record(
        state,
        if state.mode == "gameover" {
            "gameover"
        } else {
            "move"
        },
    )?;
    Ok(captures)
}

fn capture(
    state: &mut GameState,
    attacker: &Piece,
    mut victim: Piece,
    captures: &mut Vec<Piece>,
) -> Result<()> {
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
    state.captures.get_mut(actor).push(victim.clone());
    captures.push(victim.clone());
    if victim.is_defeat_royal() {
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

pub(crate) fn clear_piece(state: &mut GameState, id: &str) {
    for cell in state.board.iter_mut().flatten() {
        if cell.as_ref().is_some_and(|p| p.id == id) {
            *cell = None;
        }
    }
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

fn finish_move(state: &mut GameState, actor: Color) -> Result<()> {
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
    let turns = state.turns_taken.get_mut(actor);
    *turns = turns
        .checked_add(1)
        .ok_or_else(|| EngineError::InvalidState("turn count overflow".into()))?;
    if crate::flow::tick_deathmatch(state, actor)? {
        return Ok(());
    }
    if actor == Color::Black {
        state.full_move = state
            .full_move
            .checked_add(1)
            .ok_or_else(|| EngineError::InvalidState("full move count overflow".into()))?;
    }
    state.cards_used_this_turn = Sides::new(
        state.cards_used_this_turn.white,
        state.cards_used_this_turn.black,
    );
    *state.cards_used_this_turn.get_mut(actor) = 0;
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
    state.actions_remaining = if state.flag("acceleration", state.turn) {
        2
    } else {
        1
    };
    if state.extra.contains_key("firstMoveCardsForced") {
        state.set_flag("firstMoveCardsForced", actor, true);
    }
    if let Some(exposure) = state
        .extra
        .get_mut("sirenExposure")
        .and_then(Value::as_object_mut)
    {
        exposure.insert(
            "__turnStartKey".into(),
            json!(format!(
                "{}:{}",
                state.turn.as_str(),
                state.turns_taken.get(state.turn)
            )),
        );
    }
    if let Some(winter) = state
        .extra
        .get_mut("winterKingdom")
        .and_then(Value::as_object_mut)
    {
        winter.entry("disabledByLastWarmth").or_insert(json!(false));
    }
    crate::flow::start_clock(state)?;
    if crate::flow::check_termination(state)? {
        crate::flow::pause_clock(state)?;
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
    let effect = state.deck_slots.get(color)[slot].effect.clone();
    let card = state.deck_slots.get(color)[slot].clone();
    let mut captures = Vec::new();
    match effect.as_str() {
        "genevaConvention" | "cornerKick" | "retreat" => state.set_flag(&effect, color, true),
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
        _ => {
            let card = state.deck_slots.get(color)[slot].clone();
            captures = crate::card_effects::apply(state, &card, action)?
                .ok_or_else(|| EngineError::UnsupportedFeature(format!("card {effect}")))?;
        }
    }
    refresh_submerged(state)?;
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
        &serde_json::to_value(&card).map_err(EngineError::serialization)?,
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
    crate::replay::queue_card(state, color, &card)?;
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
    crate::replay::record(state, "card")?;
    Ok(captures)
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
