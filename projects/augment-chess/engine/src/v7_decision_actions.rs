//! Public non-card decisions from the pinned v7 client.
//!
//! Movement candidates, promotion and Trolley activation have separate rule
//! owners. This module executes Wizard spells, Shotgun reloads, optional-move
//! skips, and responses to an already active Trolley window. The transition
//! host owns source admission, event history, and the final atomic commit.
//! Source `main-OahWs0tU.js` SHA-256:
//! `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`.

use crate::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn owns(kind: ActionKind) -> bool {
    matches!(
        kind,
        ActionKind::WizardSpell
            | ActionKind::ShotgunReload
            | ActionKind::FileSurgeSkip
            | ActionKind::TrolleyChoice
    )
}

/// The private working copy is necessary even for a spell which retains its
/// turn: visibility, a replay callback, or the ensuing endMove can fail after
/// mana, board or RNG changes have already occurred.
pub(crate) fn apply(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    if state.ruleset_id != RULES_VERSION_V7 || !owns(action.kind) {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 special decision execution {:?} for {}",
            action.kind, state.ruleset_id
        )));
    }
    if state.mode == "gameover" {
        return Err(EngineError::Terminal);
    }
    if state.mode != "play" {
        return Err(EngineError::IllegalAction);
    }
    if crate::observation::truth(state.extra.get("pendingPromotion")) {
        return Err(EngineError::IllegalAction);
    }
    if action.color != state.decision_actor() {
        return Err(EngineError::WrongActor);
    }
    if action.card_id.is_some() || action.card_instance_id.is_some() || action.destination.is_some()
    {
        return Err(EngineError::IllegalAction);
    }
    let active_trolley = crate::observation::truth(state.extra.get("activeTrolley"));
    if active_trolley != (action.kind == ActionKind::TrolleyChoice) {
        return Err(EngineError::IllegalAction);
    }
    let mut working = state.clone();
    let captures = if action.kind == ActionKind::TrolleyChoice {
        apply_trolley_choice(&mut working, action)?
    } else {
        // This is applyAiAction's opening boundary. Trolley responses bypass
        // that function and therefore retain their existing transient fields.
        working.turn = action.color;
        clear_selection(&mut working);
        match action.kind {
            ActionKind::WizardSpell => apply_wizard_spell(&mut working, action)?,
            ActionKind::ShotgunReload => apply_shotgun_reload(&mut working, action)?,
            ActionKind::FileSurgeSkip => apply_optional_move_skip(&mut working, action)?,
            _ => unreachable!("the special owner was checked above"),
        }
    };
    *state = working;
    Ok(captures)
}

fn clear_selection(state: &mut GameState) {
    state.extra.insert("selected".into(), Value::Null);
    state.extra.insert("legalMoves".into(), json!([]));
    state.extra.insert("targeting".into(), Value::Null);
}

fn square_name(square: Square) -> String {
    format!("{}{}", char::from(b'a' + square.col), 8 - square.row)
}

fn source_square(value: &Value) -> Result<Square> {
    let square: Square =
        serde_json::from_value(value.clone()).map_err(EngineError::serialization)?;
    if square.row >= 8 || square.col >= 8 {
        return Err(EngineError::IllegalAction);
    }
    Ok(square)
}

fn origin(action: &Action) -> Result<Square> {
    action
        .from
        .filter(|from| from.row < 8 && from.col < 8)
        .ok_or(EngineError::IllegalAction)
}

fn exact_fields(action: &Action, fields: &[&str], allow_target: bool) -> Result<()> {
    if action.extra.len() != fields.len()
        || fields
            .iter()
            .any(|field| !action.extra.contains_key(*field))
        || (!allow_target && crate::card_effects::has_card_selection(action))
    {
        return Err(EngineError::IllegalAction);
    }
    Ok(())
}

fn nullable_number(piece: &Piece, field: &str, fallback: f64) -> Result<f64> {
    match piece.extra.get(field) {
        None | Some(Value::Null) => Ok(fallback),
        Some(value) => crate::card_effects::js_number(Some(value), 0)
            .filter(|number| number.is_finite())
            .ok_or_else(|| {
                EngineError::InvalidState(format!("v7 {} {field} must be finite", piece.kind))
            }),
    }
}

fn reloaded_ammo(shotgun: &Piece, ammo: f64, maximum: f64) -> Result<f64> {
    // Source `(ammo ?? 0) + 1` concatenates a primitive string before
    // Math.min performs Number conversion. Converting before addition would
    // silently turn the source's ammo="1", maxAmmo=3 result 3 into 2.
    let incremented = match shotgun.extra.get("ammo") {
        Some(Value::String(text)) => {
            crate::card_effects::js_number(Some(&Value::String(format!("{text}1"))), 0)
                .filter(|number| number.is_finite())
                .ok_or_else(|| {
                    EngineError::InvalidState(
                        "v7 Shotgun reload string addition produced a non-finite ammo value".into(),
                    )
                })?
        }
        Some(Value::Array(_) | Value::Object(_)) => {
            return Err(EngineError::UnsupportedFeature(
                "v7 Shotgun reload requires object/array ammo ToPrimitive addition".into(),
            ));
        }
        _ => ammo + 1.0,
    };
    let next = maximum.min(incremented);
    if !next.is_finite() {
        return Err(EngineError::InvalidState(
            "v7 Shotgun reload ammo result must be finite".into(),
        ));
    }
    Ok(next)
}

fn ammo_is_full(shotgun: &Piece, ammo: f64, maximum: f64) -> Result<bool> {
    for field in ["ammo", "maxAmmo"] {
        if matches!(
            shotgun.extra.get(field),
            Some(Value::Array(_) | Value::Object(_))
        ) {
            return Err(EngineError::UnsupportedFeature(format!(
                "v7 Shotgun reload requires object/array {field} ToPrimitive comparison",
            )));
        }
    }
    if let (Some(Value::String(ammo)), Some(Value::String(maximum))) =
        (shotgun.extra.get("ammo"), shotgun.extra.get("maxAmmo"))
    {
        // JavaScript relational comparison preserves two primitive strings,
        // including its UTF-16 lexical order; other primitive pairs are numeric.
        return Ok(ammo.encode_utf16().cmp(maximum.encode_utf16()).is_ge());
    }
    Ok(ammo >= maximum)
}

fn notation_move_number(state: &GameState) -> Result<u64> {
    let number = crate::observation::number(state.extra.get("activeHistoryMoveNumber"))
        .unwrap_or_else(|| f64::from(state.full_move).max(1.0))
        .max(0.0)
        .floor();
    if number > u64::MAX as f64 {
        return Err(EngineError::InvalidState(
            "v7 special notation move number overflow".into(),
        ));
    }
    Ok(number as u64)
}

fn spell_info(id: &str) -> Result<(&'static str, f64)> {
    match id {
        "lightning" => Ok(("번개", 1.0)),
        "shield" => Ok(("보호", 2.0)),
        "meteor" => Ok(("메테오", 3.0)),
        "timeStop" => Ok(("시간 정지", 5.0)),
        _ => Err(EngineError::IllegalAction),
    }
}

fn source_privacy(state: &GameState, piece: &Piece, at: Square) -> Result<Value> {
    let mut privacy = serde_json::Map::new();
    for viewer in [Color::White, Color::Black] {
        let visible = crate::observation::piece_visible_to_color_at_v7(state, piece, at, viewer)?;
        let type_known = visible
            || piece.color == viewer
            || piece.extra.get("hiddenFrom").and_then(Value::as_str) == Some(viewer.as_str());
        privacy.insert(
            viewer.as_str().into(),
            json!({"originVisible":visible,"typeKnown":type_known}),
        );
    }
    Ok(Value::Object(privacy))
}

fn add_private_piece_log(
    state: &mut GameState,
    piece: &Piece,
    at: Square,
    privacy: Option<&Value>,
    message: String,
) -> Result<()> {
    let owner = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let viewer = owner.opponent();
    let hidden_origin = privacy
        .and_then(|value| value.get(viewer.as_str()))
        .and_then(|value| value.get("originVisible"))
        == Some(&Value::Bool(false));
    let visible = crate::observation::piece_visible_to_color_at_v7(state, piece, at, viewer)?;
    crate::replay::add_log(
        state,
        if hidden_origin || !visible {
            "기물이 행동했습니다.".into()
        } else {
            message
        },
    )
}

fn queue_spell_notation(
    state: &mut GameState,
    wizard: &Piece,
    from: Square,
    spell_id: &str,
    spell_name: &str,
    cells: &[Square],
    privacy: &Value,
) -> Result<()> {
    let actor = wizard.color.owner().ok_or(EngineError::WrongActor)?;
    crate::replay::queue_visual(
        state,
        json!({
            "type":"magic","phase":"cast","spell":spell_id,"color":actor,"cells":cells
        }),
    )?;
    let text = if cells.len() == 1 {
        format!("!{spell_name} {}", square_name(cells[0]))
    } else {
        format!("!{spell_name}")
    };
    let move_number = notation_move_number(state)?;
    let mut notation = crate::replay::queue_notation(
        state,
        "special",
        actor,
        text.clone(),
        format!("{} 마법사가 {spell_name} 사용", crate::replay::label(actor)),
        move_number,
    )?;
    let viewer = actor.opponent();
    let visible = crate::observation::piece_visible_to_color_at_v7(state, wizard, from, viewer)?;
    if !visible {
        let known = privacy[viewer.as_str()]["typeKnown"] == true;
        let code = crate::replay::piece_code(&state.ruleset_id, &wizard.kind);
        let masked = if known {
            format!("{}??", if code.is_empty() { "P" } else { &code })
        } else {
            "???".into()
        };
        if masked != text {
            notation["redactions"] = json!({viewer.as_str():{
                "text":masked,"description":format!("상대의 숨겨진 이동 ({masked})")
            }});
            if let Some(entries) = state
                .extra
                .get_mut("pendingNotations")
                .and_then(Value::as_array_mut)
                && let Some(entry) = entries
                    .iter_mut()
                    .find(|entry| entry["id"] == notation["id"])
            {
                *entry = notation.clone();
            }
            state.extra.insert("pendingNotation".into(), notation);
        }
    }
    Ok(())
}

fn apply_wizard_spell(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    exact_fields(action, &["spellId"], true)?;
    let from = origin(action)?;
    let spell_id = action
        .extra
        .get("spellId")
        .and_then(Value::as_str)
        .ok_or(EngineError::IllegalAction)?;
    let (spell_name, cost) = spell_info(spell_id)?;
    let mut wizard = state
        .at(from)
        .filter(|piece| piece.color == action.color && piece.ability_kind() == "wizard")
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    // isWizardSpellBlockedByZugzwang clears only this stale Democracy lock.
    if state.flag("democracy", action.color)
        && state.flag("zugzwang", action.color)
        && !state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.color == action.color && state.royal_identity(piece))
    {
        state.set_flag("zugzwang", action.color, false);
    }
    if state.flag("zugzwang", action.color) {
        return Err(EngineError::IllegalAction);
    }
    let mana = nullable_number(&wizard, "mana", 0.0)?;
    if mana < cost {
        return Err(EngineError::IllegalAction);
    }
    let target = source_square(action.target.as_ref().ok_or(EngineError::IllegalAction)?)?;
    let privacy = source_privacy(state, &wizard, from)?;
    let (cells, message) = match spell_id {
        "timeStop" => {
            if target != from {
                return Err(EngineError::IllegalAction);
            }
            state.set_flag("skipTurn", action.color.opponent(), true);
            (
                vec![from],
                format!(
                    "{} 마법사가 시간 정지를 사용했습니다.",
                    crate::replay::label(action.color)
                ),
            )
        }
        "shield" => {
            let mut protected = state
                .at(target)
                .filter(|piece| {
                    piece.color == action.color
                        && !["wall", "scarecrow"].contains(&piece.kind.as_str())
                })
                .cloned()
                .ok_or(EngineError::IllegalAction)?;
            protected.extra.insert("shielded".into(), json!(true));
            crate::transition::update_piece(state, &protected);
            if protected.id == wizard.id {
                wizard = protected;
            }
            (
                vec![target],
                format!("{}에 보호 마법을 사용했습니다.", square_name(target)),
            )
        }
        "lightning" | "meteor" => {
            let cells = if spell_id == "meteor" {
                let row = target.row.min(6);
                let col = target.col.min(6);
                vec![
                    Square { row, col },
                    Square { row, col: col + 1 },
                    Square { row: row + 1, col },
                    Square {
                        row: row + 1,
                        col: col + 1,
                    },
                ]
            } else {
                vec![target]
            };
            state
                .extra
                .get_mut("delayedHazards")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| {
                    EngineError::InvalidState("v7 delayedHazards must be an array".into())
                })?
                .push(
                    json!({"type":spell_id,"cells":cells,"triggerAfter":action.color.opponent(),
                    "owner":action.color,"casterId":wizard.id}),
                );
            let message = if spell_id == "meteor" {
                format!(
                    "{} 주변 2x2에 메테오를 지정했습니다.",
                    square_name(cells[0])
                )
            } else {
                format!("{}에 번개 마법을 지정했습니다.", square_name(target))
            };
            (cells, message)
        }
        _ => unreachable!("spell_info rejected unknown IDs"),
    };
    wizard.extra.insert("mana".into(), json!(mana - cost));
    crate::transition::update_piece(state, &wizard);
    queue_spell_notation(state, &wizard, from, spell_id, spell_name, &cells, &privacy)?;
    add_private_piece_log(state, &wizard, from, Some(&privacy), message)?;
    if spell_id != "timeStop" {
        crate::transition::end_move_for_decision(state, action.color, true, None)?;
    }
    Ok(Vec::new())
}

fn apply_shotgun_reload(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    exact_fields(action, &[], false)?;
    let from = origin(action)?;
    let mut shotgun = state
        .at(from)
        .filter(|piece| piece.color == action.color && piece.kind == "shotgunKing")
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    let max_ammo = nullable_number(&shotgun, "maxAmmo", 3.0)?;
    let ammo = nullable_number(&shotgun, "ammo", 0.0)?;
    // The raw source callback succeeds even if reloadShotgunKing displays
    // its already-full notice. Candidate admission excludes that action.
    if ammo_is_full(&shotgun, ammo, max_ammo)? {
        return Ok(Vec::new());
    }
    let reloaded = reloaded_ammo(&shotgun, ammo, max_ammo)?;
    shotgun.extra.insert("ammo".into(), json!(reloaded));
    shotgun.moved = true;
    crate::transition::update_piece(state, &shotgun);
    state.en_passant = None;
    state.extra.insert("shotgunAction".into(), json!("move"));
    state.extra.insert("shotgunPreview".into(), json!([]));
    add_private_piece_log(
        state,
        &shotgun,
        from,
        None,
        format!(
            "{} 샷건 킹이 장전했습니다. ({reloaded}/{max_ammo})",
            crate::replay::label(action.color)
        ),
    )?;
    crate::transition::resolve_herald_threats(state, action.color)?;
    if state.mode == "gameover" {
        crate::replay::record(state, "gameover")?;
    } else {
        crate::transition::end_move_for_decision(state, action.color, true, None)?;
    }
    Ok(Vec::new())
}

fn forced_extra_origin(state: &GameState, actor: Color) -> Option<Square> {
    const FLAGS: &[&str] = &[
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
    ];
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .find(|&at| {
            state.at(at).is_some_and(|piece| {
                piece.color == actor
                    && FLAGS
                        .iter()
                        .any(|field| crate::observation::truth(piece.extra.get(*field)))
            })
        })
}

fn optional_presentation(piece: &Piece) -> Option<(&'static str, String, &'static str)> {
    let has = |field| crate::observation::truth(piece.extra.get(field));
    // The v7 source always enables usesThiefRemake; Thief continuation is
    // required and can therefore prevent every optional skip presentation.
    if has("thiefSecondMove") {
        return None;
    }
    let (title, name, reason) = if has("ironMonarchExtraMove") {
        ("친정", "킹", "iron monarch skip")
    } else if has("rookLiftSecondMove") {
        ("룩 리프트", "룩", "rook lift skip")
    } else if has("fileSurgeSecondMove") {
        ("급수", "나이트", "file surge skip")
    } else if has("madHorseSecondMove") {
        ("광마", "나이트", "mad horse skip")
    } else if has("platformExtraMove") {
        // main:95251 uses the complete v7 labels and falls back to "기물".
        let name = crate::replay::source_piece_label(&piece.kind)
            .filter(|label| !label.is_empty())
            .unwrap_or("기물");
        return Some(("발판", name.into(), "platform skip"));
    } else {
        return None;
    };
    Some((title, name.into(), reason))
}

fn start_next_queued_knight_move(
    state: &mut GameState,
    piece: &mut Piece,
    at: Square,
) -> Result<bool> {
    let mut seen = BTreeSet::new();
    let mut reasons = piece
        .extra
        .get("queuedKnightExtraMoveReasons")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|reason| ["mad-horse", "file-surge"].contains(reason))
        .filter(|reason| seen.insert((*reason).to_owned()))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    piece.extra.shift_remove("queuedKnightExtraMoveReasons");
    while !reasons.is_empty() {
        let reason = reasons.remove(0);
        if !reasons.is_empty() {
            piece
                .extra
                .insert("queuedKnightExtraMoveReasons".into(), json!(reasons));
        }
        piece.extra.insert(
            if reason == "mad-horse" {
                "madHorseSecondMove"
            } else {
                "fileSurgeSecondMove"
            }
            .into(),
            json!(true),
        );
        crate::transition::update_piece(state, piece);
        let moves = if optional_presentation(piece).is_some() {
            crate::movement::v7_legal_move_targets(
                state,
                piece,
                at,
                crate::movement::V7MoveOptions::default(),
            )?
        } else {
            Vec::new()
        };
        if !moves.is_empty() {
            state.move_count = state.move_count.checked_add(1).ok_or_else(|| {
                EngineError::InvalidState("v7 queued extra move count overflow".into())
            })?;
            crate::replay::record(state, &reason)?;
            // startForcedExtraMove repeats getLegalMoves after the replay
            // callback rather than retaining the earlier pre-record list.
            let moves = crate::movement::v7_legal_move_targets(
                state,
                piece,
                at,
                crate::movement::V7MoveOptions::default(),
            )?;
            if !moves.is_empty() {
                state.extra.insert("selected".into(), json!(at));
                state.extra.insert("legalMoves".into(), json!(moves));
                state.extra.insert("targeting".into(), Value::Null);
                return Ok(true);
            }
            // Source immediately returns startForcedExtraMove's result here,
            // including false; it does not consume another queued reason or
            // clear the continuation flags after a post-record empty list.
            return Ok(false);
        }
        for field in [
            "madHorseSecondMove",
            "thiefSecondMove",
            "fileSurgeSecondMove",
            "queuedKnightExtraMoveReasons",
        ] {
            piece.extra.shift_remove(field);
        }
        crate::transition::update_piece(state, piece);
    }
    crate::transition::update_piece(state, piece);
    Ok(false)
}

fn apply_optional_move_skip(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    exact_fields(action, &[], false)?;
    let from = origin(action)?;
    if forced_extra_origin(state, action.color) != Some(from) {
        return Err(EngineError::IllegalAction);
    }
    let mut piece = state.at(from).cloned().ok_or(EngineError::IllegalAction)?;
    let (title, piece_name, history_reason) =
        optional_presentation(&piece).ok_or(EngineError::IllegalAction)?;
    for field in [
        "thiefSecondMove",
        "fileSurgeSecondMove",
        "rookLiftSecondMove",
        "ironMonarchExtraMove",
        "madHorseSecondMove",
    ] {
        piece.extra.shift_remove(field);
    }
    crate::transition::update_piece(state, &piece);
    add_private_piece_log(
        state,
        &piece,
        from,
        None,
        format!(
            "{title}: {}의 {piece_name}이 추가 이동을 포기했습니다.",
            square_name(from)
        ),
    )?;
    if start_next_queued_knight_move(state, &mut piece, from)? {
        return Ok(Vec::new());
    }
    // Source's non-queued predicate receives empty type/no capture here;
    // only a previously queued backward capture can retain this turn.
    if piece.extra.get("queuedBackwardKnightTurn") == Some(&Value::Bool(true)) {
        piece.extra.shift_remove("queuedBackwardKnightTurn");
        crate::transition::update_piece(state, &piece);
        clear_selection(state);
        state.move_count = state
            .move_count
            .checked_add(1)
            .ok_or_else(|| EngineError::InvalidState("v7 backward knight count overflow".into()))?;
        crate::replay::record(state, "backward knight")?;
        return Ok(Vec::new());
    }
    crate::transition::end_move_for_decision(state, action.color, false, Some(history_reason))?;
    Ok(Vec::new())
}

fn find_piece(state: &GameState, id: &str) -> Result<Option<(Square, Piece)>> {
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            let Some(piece) = state.at(at).filter(|piece| piece.id == id) else {
                continue;
            };
            let square = if piece.is_large() {
                let coordinate = |field: &str, fallback: u8| -> Result<u8> {
                    let value = piece
                        .extra
                        .get(field)
                        .and_then(Value::as_u64)
                        .unwrap_or(u64::from(fallback));
                    u8::try_from(value)
                        .ok()
                        .filter(|value| *value < 8)
                        .ok_or_else(|| {
                            EngineError::InvalidState(format!(
                                "v7 Trolley large-piece {field} is outside the 8x8 board"
                            ))
                        })
                };
                Square {
                    row: coordinate("anchorRow", row)?,
                    col: coordinate("anchorCol", col)?,
                }
            } else {
                at
            };
            return Ok(Some((square, piece.clone())));
        }
    }
    Ok(None)
}

fn trolley_bundle_label(choice: &Value) -> Result<String> {
    let refs = choice
        .get("pieces")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineError::InvalidState("v7 Trolley choice pieces must be an array".into())
        })?;
    let mut labels = Vec::<(String, usize)>::new();
    for reference in refs {
        let kind = reference.get("type").and_then(Value::as_str).unwrap_or("");
        // main:93949: TYPE_LABELS[type] || label || type || "기물".
        let name = crate::replay::source_piece_label(kind)
            .filter(|label| !label.is_empty())
            .or_else(|| {
                reference
                    .get("label")
                    .and_then(Value::as_str)
                    .filter(|label| !label.is_empty())
            })
            .unwrap_or(if kind.is_empty() { "기물" } else { kind });
        if let Some((_, count)) = labels.iter_mut().find(|(label, _)| label == name) {
            *count += 1;
        } else {
            labels.push((name.into(), 1));
        }
    }
    Ok(labels
        .into_iter()
        .map(|(label, count)| {
            if count > 1 {
                format!("{label} {count}개")
            } else {
                label
            }
        })
        .collect::<Vec<_>>()
        .join(", "))
}

fn remove_trolley_piece(
    state: &mut GameState,
    reference: &Value,
    by: Color,
) -> Result<Option<crate::v7_board_hazards::EnvironmentalRemoval>> {
    let Some(id) = reference.get("id").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some((at, piece)) = find_piece(state, id)? else {
        return Ok(None);
    };
    if serde_json::to_value(piece.color).map_err(EngineError::serialization)? != reference["color"]
        || state.royal_identity(&piece)
    {
        return Ok(None);
    }
    if piece.is_large() {
        crate::transition::clear_piece(state, &piece.id);
    } else {
        state.board[at.row as usize][at.col as usize] = None;
    }
    crate::transition::cancel_prophecies_by_capture(state)?;
    state.captures.get_mut(by).push(piece.clone());
    crate::transition::add_capture_type(state, "capturedTypes", by, &piece.kind)?;
    crate::v7_capture_reactions::grant_wizard_mana(state, piece.color, 1)?;
    // Source markKingThreatRemovalCause is inert here: Trolley refuses every
    // royal identity before removal, and normal decisions are not probes.
    crate::transition::resolve_royal_capture(
        state,
        &piece,
        if piece.color == by { by.opponent() } else { by },
    )?;
    crate::v7_rule_bombs::mark_deathmatch_progress(state)?;
    crate::card_effects::mark_animation(state, &piece)?;
    Ok(Some(crate::v7_board_hazards::EnvironmentalRemoval {
        piece,
        square: at,
        capture_owner: by,
    }))
}

fn apply_trolley_choice(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    exact_fields(action, &["windowId", "doomedIndex"], false)?;
    if action.from.is_some() {
        return Err(EngineError::IllegalAction);
    }
    let active = state
        .extra
        .get("activeTrolley")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    if active["color"] != json!(action.color) || action.extra.get("windowId") != active.get("id") {
        return Err(EngineError::StaleAction);
    }
    let index = action
        .extra
        .get("doomedIndex")
        .and_then(Value::as_f64)
        .filter(|index| *index == 0.0 || *index == 1.0)
        .ok_or(EngineError::IllegalAction)? as usize;
    let choices = active
        .get("choices")
        .and_then(Value::as_array)
        .filter(|choices| choices.len() >= 2)
        .ok_or_else(|| {
            EngineError::InvalidState("v7 active Trolley requires two choices".into())
        })?;
    let doomed = &choices[index];
    let refs = doomed
        .get("pieces")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineError::InvalidState("v7 Trolley doomed pieces must be an array".into())
        })?;
    let by: Color =
        serde_json::from_value(active["by"].clone()).map_err(EngineError::serialization)?;
    let mut target_snapshots = BTreeMap::new();
    for reference in refs {
        if let Some(id) = reference.get("id").and_then(Value::as_str)
            && let Some((at, piece)) = find_piece(state, id)?
        {
            target_snapshots.insert(id.to_owned(), (at, piece));
        }
    }
    let mut removed = Vec::new();
    let mut targets = Vec::new();
    let mut rendered_targets = Vec::new();
    for reference in refs {
        if let Some(removal) = remove_trolley_piece(state, reference, by)? {
            if let Some((at, piece)) = target_snapshots.get(&removal.piece.id) {
                targets.push(json!({"id":piece.id,"item":piece,"row":at.row,"col":at.col}));
                rendered_targets.push(piece.clone());
            }
            removed.push(removal);
        }
    }
    crate::v7_board_hazards::resolve_reaper_nearby_deaths(state, &removed)?;
    // main:94279 calls playTrolleyEraseLocalEffect before democracy defeat.
    // Its createPieceElement consumes each snapshot's forced animation and
    // adds the ID to animatedPieceIds. Unlike ordinary vanish effects, the
    // Trolley renderer does not apply a viewer-visibility filter.
    if !state.is_ai_simulation() {
        for piece in &rendered_targets {
            crate::card_effects::mark_rendered_piece_animation(state, piece)?;
        }
    }
    if state.mode != "gameover" {
        for color in [Color::White, Color::Black] {
            if crate::flow::check_democracy_defeat(
                state,
                color,
                color.opponent(),
                "마지막 폰이 트롤리에 치였습니다.",
            )? {
                break;
            }
        }
    }
    let ended_by_trolley = state.mode == "gameover";
    state.extra.insert("activeTrolley".into(), Value::Null);
    let bundle = trolley_bundle_label(doomed)?;
    crate::replay::add_log(
        state,
        if removed.is_empty() {
            format!(
                "트롤리: {}이 {bundle} 쪽으로 트롤리를 보냈지만 해당 선로는 이미 비어 있었습니다.",
                crate::replay::label(action.color)
            )
        } else {
            format!(
                "트롤리: {}이 {bundle} 쪽으로 트롤리를 보내 {}을 잃었습니다.",
                crate::replay::label(action.color),
                removed
                    .iter()
                    .map(|entry| crate::replay::source_piece_label(&entry.piece.kind)
                        .filter(|label| !label.is_empty())
                        .unwrap_or(&entry.piece.kind))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        },
    )?;
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    crate::flow::note_card_event(state)?;
    let game_end = state.mode == "gameover"
        && removed
            .iter()
            .any(|entry| state.royal_identity(&entry.piece));
    let move_number = notation_move_number(state)?;
    crate::replay::queue_notation(
        state,
        "effect",
        action.color,
        format!(
            "!트롤리×{}{}",
            removed.len(),
            if game_end { "#" } else { "" }
        ),
        format!(
            "{} 트롤리로 기물 {}개 제거",
            crate::replay::label(action.color),
            removed.len()
        ),
        move_number,
    )?;
    crate::replay::record_with_trolley_effects(state, &targets, by)?;
    if !ended_by_trolley && state.mode != "gameover" {
        crate::flow::start_clock(state)?;
    }
    Ok(removed.into_iter().map(|entry| entry.piece).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn difference(left: &Value, right: &Value, path: &str) -> Option<String> {
        if left == right {
            return None;
        }
        match (left, right) {
            (Value::Number(left), Value::Number(right)) if left.as_f64() == right.as_f64() => None,
            (Value::Object(left), Value::Object(right)) => {
                let keys = left
                    .keys()
                    .chain(right.keys())
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>();
                for key in keys {
                    let path = format!("{path}/{key}");
                    match (left.get(key), right.get(key)) {
                        (Some(left), Some(right)) => {
                            if let Some(path) = difference(left, right, &path) {
                                return Some(path);
                            }
                        }
                        _ => {
                            return Some(format!(
                                "{path} (native field present={}, source field present={})",
                                left.contains_key(key),
                                right.contains_key(key)
                            ));
                        }
                    }
                }
                None
            }
            (Value::Array(left), Value::Array(right)) => {
                if left.len() != right.len() {
                    return Some(format!(
                        "{path}/length (native={}, source={})",
                        left.len(),
                        right.len()
                    ));
                }
                left.iter()
                    .zip(right)
                    .enumerate()
                    .find_map(|(index, (left, right))| {
                        difference(left, right, &format!("{path}/{index}"))
                    })
            }
            _ => Some(format!("{path} (native={left}, source={right})")),
        }
    }

    /// The external receipt is generated by the source's actual apply switch,
    /// before host public-event recording. Engine dispatch, source replay,
    /// RNG and failed-action rollback are checked at that same boundary.
    #[test]
    fn frozen_special_decisions_when_receipts_are_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_DECISION_ACTION_CASES") else {
            return;
        };
        let lines = std::fs::read_to_string(path).unwrap();
        let mut families = BTreeSet::new();
        let mut checked = 0;
        let mut failures = Vec::new();
        for line in lines.lines() {
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            let action: Action =
                serde_json::from_value(receipt["sourceAction"]["payload"].clone()).unwrap();
            assert!(owns(action.kind), "unexpected special receipt action");
            families.insert(
                receipt["sourceAction"]["payload"]["type"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            );
            let mut state: GameState =
                serde_json::from_value(receipt["sourcePosition"]["state"].clone()).unwrap();
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(receipt["sourcePosition"]["rng"].clone()).unwrap();
            let before = state.clone();
            let result = crate::transition::apply_without_public_event(&mut state, &action);
            let expected_ok = receipt["sourceDirectResult"]["ok"] == true;
            if result.is_ok() != expected_ok {
                failures.push(format!(
                    "{} source ok={expected_ok}, native={result:?}",
                    receipt["id"]
                ));
                continue;
            }
            if result.is_err() && state != before {
                failures.push(format!(
                    "{} rejected decision changed state or RNG",
                    receipt["id"]
                ));
            }
            let mut actual = serde_json::to_value(&state).unwrap();
            for envelope in ["rulesetId", "rng", "history"] {
                actual.as_object_mut().unwrap().remove(envelope);
            }
            let expected = &receipt["sourceDirectResult"]["position"];
            if let Some(path) = difference(&actual, &expected["state"], "state") {
                failures.push(format!(
                    "{} direct special state diverged at {path}",
                    receipt["id"]
                ));
            }
            if serde_json::to_value(&state.rng).unwrap() != expected["rng"] {
                failures.push(format!("{} direct special RNG diverged", receipt["id"]));
            }
            if state.history != before.history
                || expected["history"] != receipt["sourcePosition"]["history"]
            {
                failures.push(format!(
                    "{} no-event special boundary changed history",
                    receipt["id"]
                ));
            }
            checked += 1;
        }
        assert!(checked > 0, "special decision source receipt is empty");
        assert_eq!(
            families,
            BTreeSet::from(
                [
                    "wizardSpell",
                    "shotgunReload",
                    "fileSurgeSkip",
                    "trolleyChoice"
                ]
                .map(str::to_owned)
            )
        );
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
