//! Frozen v7 callbacks owned by the incoming player after a completed turn.
//!
//! The phase functions are deliberately separate. The client interleaves
//! portal, collapse, siren, herald, Brutus and Don callbacks between these
//! phases (main-OahWs0tU.js:93726-93775). The transition owner calls each
//! phase at that source boundary inside one host transaction. An active
//! callback whose full state/replay/RNG behavior is not yet ported errors
//! explicitly; it must never be treated as a successful no-op.

use crate::v7_turn_flow::V7FlowControl;
use crate::{
    CardSlot, Color, EngineError, GameState, MoveTarget, Piece, RULES_VERSION_V7, Result, RngState,
    Square,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

fn unsupported(callback: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!("v7 incoming turn requires unported {callback}"))
}

fn check_entry(state: &GameState, incoming: Color) -> Result<V7FlowControl> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 incoming turn on rules version {}",
            state.ruleset_id
        )));
    }
    if state.mode == "gameover" {
        return Ok(V7FlowControl::Terminal);
    }
    if state.mode != "play" || state.turn != incoming {
        return Err(EngineError::WrongActor);
    }
    Ok(V7FlowControl::Continue)
}

fn square_name(square: Square) -> String {
    format!("{}{}", char::from(b'a' + square.col), 8 - square.row)
}

fn square(row: usize, col: usize) -> Result<Square> {
    let row = u8::try_from(row)
        .map_err(|_| EngineError::InvalidState("v7 board row outside wire square".into()))?;
    let col = u8::try_from(col)
        .map_err(|_| EngineError::InvalidState("v7 board column outside wire square".into()))?;
    Square::new(row, col)
}

fn js_truth(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|number| number != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Bool(true) | Value::Array(_) | Value::Object(_)) => true,
    }
}

fn board_entries(state: &GameState) -> Result<Vec<(Square, Piece)>> {
    let mut entries = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            if let Some(piece) = piece {
                entries.push((square(row, col)?, piece.clone()));
            }
        }
    }
    Ok(entries)
}

fn has_siren_ability(piece: &Piece) -> bool {
    piece.ability_kind() == "siren"
        || piece.kind == "trickster"
            && piece.extra.get("tricksterPreviousAbilityForTurn") == Some(&json!("siren"))
}

fn siren_convertible_target(piece: &Piece) -> bool {
    piece.color.owner().is_some()
        && !piece.is_large()
        && !matches!(
            piece.kind.as_str(),
            "wall" | "football" | "blackHole" | "monster" | "coffin" | "crown"
        )
}

fn first_piece_by_id(state: &GameState, id: &str) -> Result<Option<(Square, Piece)>> {
    for (row, cells) in state.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            if let Some(piece) = piece
                && piece.id == id
            {
                return Ok(Some((square(row, col)?, piece.clone())));
            }
        }
    }
    Ok(None)
}

/// Source 93737/93163: after incoming collapse and before herald adjudication.
/// This same callback runs when initial draft completes. Exposure is keyed by
/// the incoming player's completed turn count, so a repeated projection is a
/// no-op. Conversion keeps the former piece for royal-loss reactions after
/// the live board has already acquired the siren owner's identity.
/// startClockForTurn also calls it with an explicit color. Source mode/color
/// mismatches are no-ops, unlike the actor admission of other entry callbacks.
pub(crate) fn tick_siren_exposure_for_turn_start(
    state: &mut GameState,
    incoming: Color,
) -> Result<V7FlowControl> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 incoming turn on rules version {}",
            state.ruleset_id
        )));
    }
    if state.mode == "gameover" {
        return Ok(V7FlowControl::Terminal);
    }
    if state.mode != "play" || state.turn != incoming {
        return Ok(V7FlowControl::Continue);
    }
    let mut next = state.clone();
    let flow = tick_siren_exposure(&mut next, incoming)?;
    *state = next;
    Ok(flow)
}

fn tick_siren_exposure(state: &mut GameState, incoming: Color) -> Result<V7FlowControl> {
    let key = format!("{}:{}", incoming.as_str(), state.turns_taken.get(incoming));
    let mut exposure = state
        .extra
        .get("sirenExposure")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if exposure.get("__turnStartKey").and_then(Value::as_str) == Some(key.as_str()) {
        return Ok(V7FlowControl::Continue);
    }
    exposure.insert("__turnStartKey".into(), json!(key));
    let mut sirens = Vec::new();
    let mut seen = BTreeSet::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            if let Some(piece) = piece
                && has_siren_ability(piece)
                && !piece.id.is_empty()
                && seen.insert(piece.id.clone())
            {
                sirens.push((square(row, col)?, piece.clone()));
            }
        }
    }
    exposure.retain(|id, _| id == "__turnStartKey" || seen.contains(id));
    state
        .extra
        .insert("sirenExposure".into(), Value::Object(exposure));
    for (siren_at, saved_siren) in sirens {
        if state.mode == "gameover" {
            return Ok(V7FlowControl::Terminal);
        }
        // The source list stores live references. An earlier Siren can
        // defect this later Siren, whose new owner must be read here.
        let siren = state
            .at(siren_at)
            .filter(|piece| piece.id == saved_siren.id)
            .cloned()
            .unwrap_or(saved_siren);
        if siren.color != incoming {
            continue;
        }
        if state.threat_probe_depth > 0 && siren.flag("kingThreatSuppressed") {
            continue;
        }
        let mut nearby = Vec::new();
        let mut near_seen = BTreeSet::new();
        for (row, cells) in state.board.iter().enumerate() {
            for (col, target) in cells.iter().enumerate() {
                let Some(target) = target else { continue };
                let target_at = square(row, col)?;
                if target.id.is_empty()
                    || target.color == siren.color
                    || !siren_convertible_target(target)
                    || siren_at.row.abs_diff(target_at.row) > 1
                    || siren_at.col.abs_diff(target_at.col) > 1
                {
                    continue;
                }
                if near_seen.insert(target.id.clone()) {
                    nearby.push(target.id.clone());
                }
            }
        }
        let prior = state.extra["sirenExposure"]
            .get(&siren.id)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut counts = Map::new();
        let mut converted_ids = Vec::new();
        for id in nearby {
            let previous = crate::card_effects::js_number(prior.get(&id), 0)
                .filter(|value| value.is_finite())
                .unwrap_or(0.0)
                .floor()
                .max(0.0);
            let next = previous + 1.0;
            counts.insert(
                id.clone(),
                if next <= 9_007_199_254_740_991.0 {
                    json!(next as u64)
                } else {
                    json!(next)
                },
            );
            if next >= 2.0 {
                converted_ids.push(id);
            }
        }
        state.extra["sirenExposure"]
            .as_object_mut()
            .expect("normalized exposure")
            .insert(siren.id.clone(), Value::Object(counts));
        for id in converted_ids {
            if state.mode == "gameover" {
                return Ok(V7FlowControl::Terminal);
            }
            let Some((target_at, original)) = first_piece_by_id(state, &id)? else {
                continue;
            };
            if original.color == siren.color
                || !siren_convertible_target(&original)
                || siren_at.row.abs_diff(target_at.row) > 1
                || siren_at.col.abs_diff(target_at.col) > 1
            {
                continue;
            }
            let former = original.color.owner().ok_or(EngineError::WrongActor)?;
            let mut converted = original.clone();
            converted.color = incoming.into();
            converted.moved = true;
            crate::card_effects::mark_transformed_origin_with_options(
                state,
                &mut converted,
                target_at,
                true,
            )?;
            converted.extra.shift_remove("freshNoCaptureUntil");
            converted
                .extra
                .insert("coolGuyCapturedLast".into(), json!(false));
            state.board[usize::from(target_at.row)][usize::from(target_at.col)] =
                Some(converted.clone());
            crate::card_effects::mark_animation(state, &converted)?;
            state.extra["sirenExposure"][&siren.id]
                .as_object_mut()
                .expect("normalized siren count")
                .shift_remove(&id);
            if has_siren_ability(&converted) {
                state.extra["sirenExposure"]
                    .as_object_mut()
                    .expect("normalized exposure")
                    .shift_remove(&converted.id);
            }
            crate::replay::add_log(
                state,
                format!(
                    "세이렌: {}의 {} {}이 전향했습니다.",
                    square_name(target_at),
                    crate::replay::label(former),
                    crate::replay::source_piece_label(&original.kind).unwrap_or(&original.kind)
                ),
            )?;
            let threat_probe = state.threat_probe_depth > 0;
            crate::v7_threat::mark_king_threat_removal_cause(
                state,
                &original,
                target_at,
                &json!({"attacker":siren,"origin":siren_at,"label":"세이렌 전향"}),
                threat_probe,
            )?;
            crate::transition::resolve_royal_capture(state, &original, incoming)?;
        }
    }
    Ok(if state.mode == "gameover" {
        V7FlowControl::Terminal
    } else {
        V7FlowControl::Continue
    })
}

fn clear_queens_gambit_after_transformation(piece: &mut Piece) -> Result<()> {
    if !piece.flag("queensGambitProtection") {
        return Ok(());
    }
    let previous_protected = piece.flag("queensGambitPreviousProtected");
    for field in [
        "lastResistance",
        "sacrificeProtection",
        "coronationProtection",
    ] {
        if let Some(value) = piece.extra.get_mut(field)
            && js_truth(Some(value))
            && !previous_protected
        {
            let timed = value.as_object_mut().ok_or_else(|| {
                EngineError::InvalidState(format!("{field} protection must be an object"))
            })?;
            timed.insert("previousProtected".into(), json!(false));
        }
    }
    piece.extra.shift_remove("queensGambitProtection");
    piece.extra.shift_remove("queensGambitPreviousProtected");
    if !previous_protected
        && [
            "lastResistance",
            "sacrificeProtection",
            "coronationProtection",
        ]
        .into_iter()
        .all(|field| !js_truth(piece.extra.get(field)))
    {
        piece.extra.shift_remove("protected");
    }
    Ok(())
}

/// Source 93727/99527, immediately before Portal Gun reservations.
pub(crate) fn resolve_vip_invitations(
    state: &mut GameState,
    incoming: Color,
) -> Result<V7FlowControl> {
    if check_entry(state, incoming)? == V7FlowControl::Terminal {
        return Ok(V7FlowControl::Terminal);
    }
    if !state.board.iter().flatten().flatten().any(|piece| {
        piece.color == incoming
            && piece.kind == "pawn"
            && js_truth(piece.extra.get("vipInvitation"))
    }) {
        return Ok(V7FlowControl::Continue);
    }
    let candidates = board_entries(state)?;
    for (at, mut piece) in candidates {
        if piece.color != incoming || piece.kind != "pawn" {
            continue;
        }
        let Some(invitation) = piece.extra.get("vipInvitation") else {
            continue;
        };
        if !js_truth(Some(invitation)) {
            continue;
        }
        let trigger = invitation
            .get("triggerTurn")
            .and_then(Value::as_u64)
            .filter(|turn| *turn > 0)
            .ok_or_else(|| {
                EngineError::InvalidState("vipInvitation.triggerTurn must be positive".into())
            })?;
        if u64::from(*state.turns_taken.get(incoming)) < trigger {
            continue;
        }
        piece.extra.shift_remove("vipInvitation");
        piece.extra.shift_remove("holdoutPromotion");
        piece.kind = "vip".into();
        clear_queens_gambit_after_transformation(&mut piece)?;
        piece.moved = true;
        crate::card_effects::mark_transformed_origin_with_options(state, &mut piece, at, true)?;
        state.board[usize::from(at.row)][usize::from(at.col)] = Some(piece.clone());
        crate::card_effects::mark_animation(state, &piece)?;
        crate::replay::add_piece_action_log(
            state,
            &piece,
            Some(at),
            None,
            format!("귀빈: {}의 폰이 귀빈으로 변했습니다.", square_name(at)),
        )?;
    }
    Ok(V7FlowControl::Continue)
}

/// Source 93729/104200. Remove all due queue entries before sacrificing each
/// source queen, applying the target-centered explosion, and recording the
/// special notation. Later entries observe earlier explosions in source order.
pub(crate) fn resolve_pending_icbm(
    state: &mut GameState,
    incoming: Color,
) -> Result<V7FlowControl> {
    if check_entry(state, incoming)? == V7FlowControl::Terminal {
        return Ok(V7FlowControl::Terminal);
    }
    let Some(value) = state.extra.get("pendingIcbm") else {
        return Ok(V7FlowControl::Continue);
    };
    let entries = value
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("pendingIcbm must be an array".into()))?;
    let mut retained = Vec::with_capacity(entries.len());
    let mut due = Vec::new();
    for entry in entries {
        let owner = entry
            .get("color")
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::InvalidState("pendingIcbm.color missing".into()))?;
        if !matches!(owner, "white" | "black") {
            return Err(EngineError::InvalidState(
                "pendingIcbm.color invalid".into(),
            ));
        }
        let trigger = entry
            .get("triggerTurn")
            .and_then(Value::as_u64)
            .filter(|turn| *turn > 0)
            .ok_or_else(|| {
                EngineError::InvalidState("pendingIcbm.triggerTurn must be positive".into())
            })?;
        if owner != incoming.as_str() || u64::from(*state.turns_taken.get(incoming)) < trigger {
            retained.push(entry.clone());
        } else {
            due.push(entry.clone());
        }
    }
    if due.is_empty() {
        return Ok(V7FlowControl::Continue);
    }
    let mut next = state.clone();
    next.extra
        .insert("pendingIcbm".into(), Value::Array(retained));
    for entry in due {
        let source_id = entry
            .get("sourceQueenId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| EngineError::InvalidState("pendingIcbm.sourceQueenId missing".into()))?;
        let target_id = entry
            .get("targetQueenId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| EngineError::InvalidState("pendingIcbm.targetQueenId missing".into()))?;
        let source = first_piece_by_id(&next, source_id)?.filter(|(_, piece)| {
            piece.color == incoming && piece.kind == "queen" && !piece.flag("regencyHeir")
        });
        let Some((source_at, _)) = source else {
            crate::replay::add_log(
                &mut next,
                format!(
                    "ICBM 취소: {} 퀸이 사라졌거나 다른 기물로 변경되었습니다.",
                    crate::replay::label(incoming),
                ),
            )?;
            continue;
        };
        let target = first_piece_by_id(&next, target_id)?.filter(|(_, piece)| {
            piece.color == incoming.opponent()
                && piece.kind == "queen"
                && !piece.flag("regencyHeir")
        });
        let Some((blast_at, _)) = target else {
            crate::replay::add_log(
                &mut next,
                "ICBM 취소: 조준했던 상대 퀸이 사라졌거나 다른 기물로 변경되었습니다.".into(),
            )?;
            continue;
        };
        crate::transition::force_remove_piece_at_with_options(
            &mut next,
            source_at,
            incoming.opponent(),
            &crate::transition::ForceRemovalOptions {
                threat_source: Some(&json!({"label":"ICBM"})),
                ..Default::default()
            },
        )?;
        crate::v7_board_hazards::explode_at(&mut next, blast_at, "ICBM")?;
        crate::replay::queue_special_effect_notation(
            &mut next,
            incoming,
            "ICBM",
            &format!(
                "{} ICBM이 {}에서 폭발",
                crate::replay::label(incoming),
                square_name(blast_at)
            ),
        )?;
    }
    let terminal = next.mode == "gameover";
    *state = next;
    Ok(if terminal {
        V7FlowControl::Terminal
    } else {
        V7FlowControl::Continue
    })
}
fn resolve_baby_bear_growth(state: &mut GameState, incoming: Color) -> Result<()> {
    if !state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|piece| piece.color == incoming && piece.ability_kind() == "babyBear")
    {
        return Ok(());
    }
    let mut seen = BTreeSet::new();
    let shared_turns = state.turns_taken.white.min(state.turns_taken.black);
    let entries = board_entries(state)?;
    let mut grown = Vec::new();
    for (at, mut piece) in entries {
        if piece.color != incoming
            || piece.ability_kind() != "babyBear"
            || !seen.insert(piece.id.clone())
        {
            continue;
        }
        let ready = crate::card_effects::js_number(piece.extra.get("babyBearGrowAtTurn"), 0)
            .map(|ready| f64::from(shared_turns) >= ready)
            .or_else(|| {
                crate::card_effects::js_number(piece.extra.get("babyBearGrowAtMove"), 0)
                    .map(|ready| f64::from(state.move_count) >= ready)
            });
        if ready != Some(true) {
            continue;
        }
        piece.kind = "bear".into();
        for field in [
            "tricksterMoveType",
            "tricksterPreviousAbilityForTurn",
            "babyBearGrowAtTurn",
            "babyBearGrowAtMove",
            "babyBearMoveAfterTurn",
        ] {
            piece.extra.shift_remove(field);
        }
        piece
            .extra
            .insert("bearRetaliationsRemaining".into(), json!(2));
        piece.moved = true;
        if piece.id.is_empty() {
            state.board[usize::from(at.row)][usize::from(at.col)] = Some(piece.clone());
        } else {
            crate::transition::update_piece(state, &piece);
        }
        crate::card_effects::mark_animation(state, &piece)?;
        grown.push((at, piece));
    }
    for (at, piece) in &grown {
        crate::replay::add_piece_action_log(
            state,
            piece,
            Some(*at),
            None,
            format!("아기곰: {}에서 곰으로 성장했습니다.", square_name(*at)),
        )?;
    }
    if !grown.is_empty() {
        crate::threat::play_move_sound(state, "promote", incoming)?;
    }
    Ok(())
}

pub(crate) fn resolve_twin_swaps(state: &mut GameState) -> Result<()> {
    if !state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|piece| js_truth(piece.extra.get("twinBondId")))
    {
        return Ok(());
    }
    let mut seen = BTreeSet::new();
    // Source Map iteration is insertion ordered: several swaps consume one
    // notation RNG draw each, so lexicographically sorting the bond IDs would
    // silently change the following position.
    let mut bonds: Vec<(String, Vec<(Square, Piece)>)> = Vec::new();
    for (at, piece) in board_entries(state)? {
        let Some(bond) = piece
            .extra
            .get("twinBondId")
            .filter(|value| js_truth(Some(value)))
        else {
            continue;
        };
        let bond = bond
            .as_str()
            .ok_or_else(|| EngineError::InvalidState("twinBondId must be a string".into()))?;
        if seen.insert(piece.id.clone()) {
            if let Some((_, entries)) = bonds.iter_mut().find(|(id, _)| id == bond) {
                entries.push((at, piece));
            } else {
                bonds.push((bond.into(), vec![(at, piece)]));
            }
        }
    }
    let mut swapped = 0;
    for (_, entries) in bonds {
        if entries.len() == 2
            && entries
                .iter()
                .all(|(_, piece)| piece.ability_kind() != "slime")
        {
            let pending: f64 = entries
                .iter()
                .map(|(_, piece)| {
                    crate::card_effects::js_number(piece.extra.get("twinSwapPending"), 0)
                        .unwrap_or(0.0)
                })
                .sum();
            let [(first_at, mut first), (second_at, mut second)] =
                <[(Square, Piece); 2]>::try_from(entries).map_err(|_| {
                    EngineError::InvalidState("v7 twin pair must contain two pieces".into())
                })?;
            first.extra.shift_remove("twinSwapPending");
            second.extra.shift_remove("twinSwapPending");
            if pending % 2.0 != 0.0 {
                if first.is_large() || second.is_large() {
                    return Err(unsupported("resolveTwinSwaps large piece aliases"));
                }
                let first_color = first
                    .color
                    .owner()
                    .ok_or_else(|| unsupported("resolveTwinSwaps neutral piece notation"))?;
                first.moved = true;
                second.moved = true;
                state.board[usize::from(first_at.row)][usize::from(first_at.col)] =
                    Some(second.clone());
                state.board[usize::from(second_at.row)][usize::from(second_at.col)] =
                    Some(first.clone());
                crate::card_effects::mark_animation(state, &first)?;
                crate::card_effects::mark_animation(state, &second)?;
                crate::replay::add_log(
                    state,
                    format!(
                        "환상의 콤비: {}와 {}의 기물이 위치를 바꿨습니다.",
                        square_name(first_at),
                        square_name(second_at)
                    ),
                )?;
                crate::replay::queue_special_effect_notation(
                    state,
                    first_color,
                    "환상의 콤비",
                    &format!(
                        "{} 환상의 콤비 위치 교환",
                        crate::replay::label(first_color)
                    ),
                )?;
                swapped += 1;
            } else {
                state.board[usize::from(first_at.row)][usize::from(first_at.col)] = Some(first);
                state.board[usize::from(second_at.row)][usize::from(second_at.col)] = Some(second);
            }
        } else {
            for (at, _) in entries {
                let piece = state.at_mut(at).ok_or_else(|| {
                    EngineError::InvalidState("twin vanished during turn entry".into())
                })?;
                for field in ["twinBondId", "twinPartnerId", "twinSwapPending"] {
                    piece.extra.shift_remove(field);
                }
            }
        }
    }
    if swapped > 0 {
        let bonds = crate::card_effects::normalize_chain_bonds(state.extra.get("chainBonds"))?;
        let mut active = Vec::new();
        let mut broken = 0;
        for bond in bonds {
            let first = bond["aId"]
                .as_str()
                .and_then(|id| piece_square_by_id(state, id));
            let second = bond["bId"]
                .as_str()
                .and_then(|id| piece_square_by_id(state, id));
            if first.zip(second).is_some_and(|(first, second)| {
                first
                    .row
                    .abs_diff(second.row)
                    .max(first.col.abs_diff(second.col))
                    <= 2
            }) {
                active.push(bond);
            } else {
                broken += 1;
            }
        }
        state.extra.insert("chainBonds".into(), json!(active));
        if broken > 0 {
            crate::replay::add_log(
                state,
                format!(
                    "사슬: 연결된 기물이 사라지거나 사이가 3칸 이상 벌어져 {broken}개의 사슬이 끊어졌습니다."
                ),
            )?;
        }
    }
    Ok(())
}

fn piece_square_by_id(state: &GameState, id: &str) -> Option<Square> {
    for (row, line) in state.board.iter().enumerate() {
        for (col, piece) in line.iter().enumerate() {
            if piece.as_ref().is_some_and(|piece| piece.id == id) {
                return Some(Square {
                    row: row as u8,
                    col: col as u8,
                });
            }
        }
    }
    None
}

fn activate_pending_draft_cards(state: &mut GameState, incoming: Color) -> Result<()> {
    // Source 69183 skips only aiSimulationDepth, then reads playerDeck even
    // when no pending card exists. A royal-threat probe alone still pads it.
    if state.ai_simulation_depth > 0 {
        return Ok(());
    }
    let current_turn = u64::from(*state.turns_taken.get(incoming));
    let grand = state.extra.get("gameStyle").and_then(Value::as_str) == Some("grand")
        && !js_truth(state.extra.get("campaign"));
    let slots = crate::draft::v7_player_deck(state, incoming)?.len();
    let mut changed = false;
    for slot in 0..slots {
        let card = &state.deck_slots.get(incoming)[slot];
        if card.vacant
            || !js_truth(card.extra.get("nextTurnPending"))
            || js_truth(card.extra.get("devCard"))
        {
            continue;
        }
        let definition = crate::card_registry::definition_for(&state.ruleset_id, &card.id)?;
        let phase = definition.source_definition["phase"]
            .as_str()
            .unwrap_or("END");
        if !matches!(phase, "MIDDLE" | "END") {
            continue;
        }
        let since = card.extra.get("nextTurnPendingSinceTurn");
        if since.is_none_or(Value::is_null) {
            state.deck_slots.get_mut(incoming)[slot]
                .extra
                .insert("nextTurnPendingSinceTurn".into(), json!(current_turn));
            changed = true;
            continue;
        }
        let since = since.and_then(Value::as_u64).ok_or_else(|| {
            EngineError::InvalidState("nextTurnPendingSinceTurn must be integer".into())
        })?;
        let delay = if grand && phase == "END" { 3 } else { 1 };
        if current_turn < since.saturating_add(delay) {
            continue;
        }
        let passive = definition.activation == Some(crate::card_registry::CardActType::Passive);
        let card = &mut state.deck_slots.get_mut(incoming)[slot];
        card.extra.shift_remove("nextTurnPending");
        card.extra.shift_remove("nextTurnPendingSinceTurn");
        if passive {
            crate::transition::apply_draft_passive(state, incoming, slot)?;
        }
        changed = true;
    }
    if changed {
        crate::flow::note_card_event(state)?;
    }
    Ok(())
}

fn apply_additional_rule_card(state: &mut GameState, entry: &Value) -> Result<()> {
    let Some(id) = entry.get("ruleId").and_then(Value::as_str) else {
        return Ok(());
    };
    let definition = crate::card_registry::registry_for(RULES_VERSION_V7)?
        .cards
        .get(id);
    let Some(definition) = definition else {
        return Ok(());
    };
    let name = definition
        .source_definition
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(id);
    let selected = state
        .deck_slots
        .white
        .iter()
        .chain(&state.deck_slots.black)
        .filter(|card| !card.vacant)
        .map(|card| card.id.as_str())
        .collect::<BTreeSet<_>>();
    let conflict = crate::card_effects::september18(state)
        && (id == "reverse-pawns"
            && (selected.contains("rule-ticket") || selected.contains("macho-chess"))
            || matches!(id, "rule-ticket" | "macho-chess") && selected.contains("reverse-pawns"));
    if conflict {
        return crate::replay::add_log(state, format!("규칙 티켓 실패: {name}"));
    }
    let color = match entry.get("color").and_then(Value::as_str) {
        Some("black") => Color::Black,
        Some("white") | None => Color::White,
        _ => {
            return Err(EngineError::InvalidState(
                "pendingRuleTickets.color must be a player color".into(),
            ));
        }
    };
    let card: CardSlot = serde_json::from_value(definition.source_definition.clone())
        .map_err(EngineError::serialization)?;
    let previous_turn = state.turn;
    state.turn = color;
    let application = crate::transition::begin_v7_card(state)?;
    let applied = crate::card_effects::apply_v7_additional_rule_effect(state, &card)
        .and_then(|()| crate::transition::finish_v7_card(state, application));
    state.turn = previous_turn;
    if let Err(error) = applied {
        return if error == EngineError::IllegalAction {
            crate::replay::add_log(state, format!("규칙 티켓 실패: {name}"))
        } else {
            Err(error)
        };
    }
    let mut additional = state
        .extra
        .get("additionalRuleCards")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !additional
        .iter()
        .any(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
    {
        // cloneCard consumes a draw even though the stable ticket ID replaces
        // its random instance ID immediately afterwards in the source.
        let mut clone = crate::draft::clone_card(state, &definition.source_definition)?;
        clone["instanceId"] = json!(format!("rule-ticket-applied-{id}"));
        additional.push(clone);
    }
    state
        .extra
        .insert("additionalRuleCards".into(), Value::Array(additional));
    crate::v7_board_hazards::post_card(state, color)?;
    crate::replay::queue_notation(
        state,
        "card",
        color,
        format!("@{name}"),
        format!("{} 추가 RULE {name} 적용", crate::replay::label(color)),
        u64::from(state.full_move),
    )?;
    crate::replay::add_log(state, format!("규칙 티켓 발동: {name}"))
}

fn tick_rule_tickets(state: &mut GameState, incoming: Color) -> Result<()> {
    let Some(value) = state.extra.get("pendingRuleTickets") else {
        return Ok(());
    };
    let entries = value
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("pendingRuleTickets must be array".into()))?
        .clone();
    let mut next = Vec::with_capacity(entries.len());
    let mut due = Vec::new();
    for mut entry in entries {
        let owner = if entry.get("color").and_then(Value::as_str) == Some("black") {
            Color::Black
        } else {
            Color::White
        };
        if let Some(start) = crate::card_effects::js_number(entry.get("startTurnCount"), 0) {
            if owner == incoming
                && f64::from(*state.turns_taken.get(owner)) > start.floor().max(0.0)
            {
                due.push(entry);
                continue;
            }
        } else {
            let remaining = crate::card_effects::js_number(entry.get("remainingHalfTurns"), 0)
                .unwrap_or(0.0)
                .max(0.0)
                - 1.0;
            if owner == incoming && remaining <= 0.0 {
                due.push(entry);
                continue;
            }
            let map = entry.as_object_mut().ok_or_else(|| {
                EngineError::InvalidState("pendingRuleTickets entry must be object".into())
            })?;
            map.insert(
                "remainingHalfTurns".into(),
                if remaining.fract() == 0.0
                    && remaining >= i64::MIN as f64
                    && remaining < i64::MAX as f64
                {
                    json!(remaining as i64)
                } else {
                    json!(remaining)
                },
            );
        }
        next.push(entry);
    }
    state
        .extra
        .insert("pendingRuleTickets".into(), Value::Array(next));
    for entry in due {
        apply_additional_rule_card(state, &entry)?;
    }
    Ok(())
}

// main69207: raw aiSimulationDepth만 no-op이며 playerDeck 읽기가 padding을 소유한다.
// toast는 ephemeral host UI다. 원문은 truthy notice를 전달하고 값을 삭제한다.
fn apply_white_box_notices(state: &mut GameState, incoming: Color) -> Result<usize> {
    if state.ai_simulation_depth > 0 {
        return Ok(0);
    }
    let mut shown = 0;
    for card in crate::draft::v7_player_deck(state, incoming)? {
        if card.vacant || js_truth(card.extra.get("devCard")) {
            continue;
        }
        if js_truth(card.extra.get("whiteBoxPendingNotice")) {
            card.extra.shift_remove("whiteBoxPendingNotice");
            shown += 1;
        }
    }
    if shown > 0 {
        crate::flow::note_card_event(state)?;
    }
    Ok(shown)
}

/// main69220: 첫 OPENING 시작은 White → Black 순서로 양측 알림을 처리한다.
/// 그 외 phase/수 번호는 지정 색만 처리하며 호출자가 mode·clock 순서를 소유한다.
pub(crate) fn apply_pending_white_boxes_for_play_start(
    state: &mut GameState,
    incoming: Color,
    phase: &str,
) -> Result<usize> {
    if state.ruleset_id == crate::RULES_VERSION_V6 {
        return Ok(0);
    }
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "WhiteBox play-start requires frozen v7 rules".into(),
        ));
    }
    let mut next = state.clone();
    let shown = if phase == "OPENING" && next.move_count == 0 {
        let white = apply_white_box_notices(&mut next, Color::White)?;
        white + apply_white_box_notices(&mut next, Color::Black)?
    } else {
        apply_white_box_notices(&mut next, incoming)?
    };
    *state = next;
    Ok(shown)
}

fn panic_relocation_moves(state: &GameState, piece: &Piece, at: Square) -> Result<Vec<Square>> {
    let mut projection = state.clone();
    projection.turn = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let candidates = crate::movement::v7_legal_move_targets(
        &projection,
        piece,
        at,
        crate::movement::V7MoveOptions::default(),
    )?;
    let mut result = Vec::new();
    for candidate in candidates {
        if [
            "castle",
            "enPassant",
            "jumpCapture",
            "colossusAttack",
            "colossusMove",
            "colossusBody",
            "shotgunBlast",
            "shotgunSnipe",
            "merchantBuy",
            "dragonSwap",
            "bigRookMove",
            "setLogDirection",
        ]
        .into_iter()
        .any(|field| candidate.flag(field))
        {
            continue;
        }
        let destination = candidate.square();
        if black_hole_cell(state, destination)
            || crate::movement::collapsed(state, destination)
            || state.at(destination).is_some()
        {
            continue;
        }
        if matches!(piece.kind.as_str(), "pawn" | "squire" | "standardBearer")
            && crate::v7_promotion::should_promote_v7(state, piece, destination)?
        {
            continue;
        }
        result.push(destination);
    }
    Ok(result)
}

fn black_hole_cell(state: &GameState, at: Square) -> bool {
    state
        .extra
        .get("blackHole")
        .and_then(Value::as_array)
        .is_some_and(|cells| {
            cells.iter().any(|cell| {
                crate::card_effects::js_number(cell.get("row"), 0) == Some(f64::from(at.row))
                    && crate::card_effects::js_number(cell.get("col"), 0) == Some(f64::from(at.col))
            })
        })
}

fn fog_log_redaction_active(state: &GameState) -> bool {
    state
        .extra
        .get("campaign")
        .and_then(|campaign| campaign.get("setup"))
        .and_then(Value::as_str)
        .is_some_and(|setup| matches!(setup, "fogWar" | "fog"))
        || js_truth(state.extra.get("fogWar"))
        || js_truth(state.extra.get("fogOfWar"))
        || js_truth(state.extra.get("fog").and_then(|fog| fog.get("enabled")))
}

fn move_privacy_snapshot(state: &GameState, piece: &Piece, at: Square) -> Result<Value> {
    let mut privacy = Map::new();
    for viewer in [Color::White, Color::Black] {
        let visible = crate::observation::piece_visible_to_color_at_v7(state, piece, at, viewer)?;
        privacy.insert(viewer.as_str().into(), json!({
            "originVisible":visible,
            "typeKnown":visible || piece.color == viewer || piece.extra.get("hiddenFrom") == Some(&json!(viewer)),
        }));
    }
    Ok(Value::Object(privacy))
}

fn baby_bear_destination(target: &MoveTarget) -> Result<Square> {
    let descriptor = if target.flag("portalLanding") {
        target.flags.get("portalExit")
    } else {
        None
    };
    if let Some((row, col)) = descriptor
        .and_then(|cell| {
            cell.get("row")
                .and_then(Value::as_u64)
                .zip(cell.get("col").and_then(Value::as_u64))
        })
        .filter(|&(row, col)| row < 8 && col < 8)
    {
        return Square::new(row as u8, col as u8);
    }
    if let Some((row, col)) = target
        .flags
        .get("anchorRow")
        .and_then(Value::as_u64)
        .zip(target.flags.get("anchorCol").and_then(Value::as_u64))
        .filter(|&(row, col)| row < 8 && col < 8)
    {
        return Square::new(row as u8, col as u8);
    }
    Square::new(target.row, target.col)
}

fn add_automatic_log_highlight(
    state: &mut GameState,
    from: Square,
    to: Square,
    owner: Color,
    hidden_from: &str,
) -> Result<()> {
    if let Some(last) = state
        .extra
        .get_mut("lastMove")
        .filter(|last| js_truth(Some(last)))
    {
        let fields = last.as_object_mut().ok_or_else(|| {
            EngineError::InvalidState("automatic log movement lastMove must be an object".into())
        })?;
        let mut traces = fields
            .get("logMoves")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        traces.push(json!({"from":from,"to":to,"hiddenFrom":hidden_from}));
        fields.insert("logMoves".into(), Value::Array(traces));
        return Ok(());
    }
    let sound = if owner == Color::White {
        "moveSelf"
    } else {
        "moveOpponent"
    };
    crate::card_effects::set_last_move(state, from, to, sound, owner, hidden_from, None)
}

/// Source 108562-108610. Candidate generation is distinct from manual moves:
/// even a frozen Baby Bear can move through this automatic raw callback.
/// Empty pools consume randomChoice's draw. Existing lastMove receives a
/// logMoves trace; no move sound, manual capture, or turn count is introduced.
fn auto_move_baby_bears(state: &mut GameState, incoming: Color) -> Result<()> {
    let mut seen = BTreeSet::new();
    let babies = board_entries(state)?
        .into_iter()
        .filter(|(_, piece)| {
            piece.color == incoming
                && piece.ability_kind() == "babyBear"
                && seen.insert(piece.id.clone())
        })
        .collect::<Vec<_>>();
    for (from, saved) in babies {
        if state.mode == "gameover" {
            continue;
        }
        let Some(mut piece) = state.at(from).filter(|piece| piece.id == saved.id).cloned() else {
            continue;
        };
        let moves = crate::movement::v7_baby_bear_automatic_moves(state, &piece, from)?;
        let draw = if moves.is_empty() {
            state
                .rng
                .sample_invariant("source empty automatic Baby Bear moves")?
        } else {
            let draw = state.rng.sample()?;
            state.rng.record_last_probability(
                1.0 / moves.len() as f64,
                "source automatic Baby Bear destination",
            )?;
            draw
        };
        if moves.is_empty() {
            continue;
        }
        let selected = &moves[(draw * moves.len() as f64).floor() as usize];
        let to = baby_bear_destination(selected)?;
        let privacy = move_privacy_snapshot(state, &piece, from)?;
        state.board[from.row as usize][from.col as usize] = None;
        piece.moved = true;
        state.board[to.row as usize][to.col as usize] = Some(piece.clone());
        if piece.kind == "trickster" {
            crate::card_effects::reroll_trickster_ability(state, &mut piece)?;
        }
        crate::card_effects::note_ultimatum_movement(state, &mut piece)?;
        crate::card_effects::mark_animation(state, &piece)?;
        state.board[to.row as usize][to.col as usize] = Some(piece.clone());
        let viewer = incoming.opponent();
        let destination_visible =
            crate::observation::piece_visible_to_color_at_v7(state, &piece, to, viewer)?;
        let origin_visible = privacy[viewer.as_str()]["originVisible"] == json!(true);
        let hidden_from = if origin_visible && destination_visible {
            ""
        } else {
            viewer.as_str()
        };
        add_automatic_log_highlight(state, from, to, incoming, hidden_from)?;
        let trail_hidden = state
            .extra
            .get("lastMove")
            .and_then(|last| last.get("hiddenFrom"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        crate::card_effects::track_acceleration_trail(
            state,
            incoming,
            &[from, to],
            false,
            &trail_hidden,
        )?;
        crate::replay::queue_automatic_move_notation(
            state,
            &piece,
            from,
            to,
            Some(&privacy),
            false,
        )?;
        let message = if fog_log_redaction_active(state) {
            format!("{} 기물이 이동했습니다.", crate::replay::label(incoming))
        } else if !hidden_from.is_empty() {
            if !origin_visible
                || crate::observation::piece_hidden_from_v7(state, &piece, to).is_some()
            {
                "기물이 움직였습니다.".into()
            } else {
                format!(
                    "{} {}: {} -> {}",
                    crate::replay::label(incoming),
                    crate::replay::source_piece_label(&piece.kind).unwrap_or("undefined"),
                    square_name(from),
                    square_name(to)
                )
            }
        } else {
            format!(
                "{}의 아기곰이 {}로 움직였습니다.",
                square_name(from),
                square_name(to)
            )
        };
        crate::replay::add_log(state, message)?;
    }
    Ok(())
}

fn relocate_panicked_piece(
    state: &mut GameState,
    mut piece: Piece,
    from: Square,
    to: Square,
) -> Result<(Value, String)> {
    let owner = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let viewer = owner.opponent();
    let origin_visible =
        crate::observation::piece_visible_to_color_at_v7(state, &piece, from, viewer)?;
    state.board[to.row as usize][to.col as usize] = Some(piece.clone());
    state.board[from.row as usize][from.col as usize] = None;
    piece.moved = true;
    piece.extra.shift_remove("quantum");
    piece.source_order.retain(|field| field != "quantum");
    state.board[to.row as usize][to.col as usize] = Some(piece.clone());
    crate::card_effects::mark_animation(state, &piece)?;
    crate::card_effects::note_ultimatum_movement(state, &mut piece)?;
    state.board[to.row as usize][to.col as usize] = Some(piece.clone());
    let destination_visible =
        crate::observation::piece_visible_to_color_at_v7(state, &piece, to, viewer)?;
    let hidden_from = if origin_visible && destination_visible {
        ""
    } else {
        viewer.as_str()
    };
    let sound = if owner == Color::White {
        "moveSelf"
    } else {
        "moveOpponent"
    };
    crate::card_effects::set_last_move(state, from, to, sound, owner, hidden_from, None)?;
    crate::threat::play_move_sound(state, sound, owner)?;
    crate::replay::add_log(
        state,
        if fog_log_redaction_active(state) {
            format!("{} 기물이 이동했습니다.", crate::replay::label(owner))
        } else if !origin_visible
            || crate::observation::piece_hidden_from_v7(state, &piece, to).is_some()
        {
            "기물이 움직였습니다.".into()
        } else {
            format!(
                "{} {}: {} -> {}",
                crate::replay::label(owner),
                crate::replay::source_piece_label(&piece.kind).unwrap_or("undefined"),
                square_name(from),
                square_name(to)
            )
        },
    )?;
    let effective_hidden = state
        .extra
        .get("lastMove")
        .and_then(|last| last.get("hiddenFrom"))
        .and_then(Value::as_str)
        .unwrap_or(hidden_from)
        .to_owned();
    Ok((
        json!({"from":from,"to":to,"pieceId":piece.id,"pieceType":piece.kind}),
        effective_hidden,
    ))
}

fn settle_pending_panic(state: &mut GameState, incoming: Color) -> Result<()> {
    let Some(value) = state.extra.get("pendingPanic") else {
        return Ok(());
    };
    let entries = value
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("pendingPanic must be array".into()))?;
    let mut next = Vec::new();
    let mut due = Vec::new();
    for entry in entries {
        if entry.get("color").and_then(Value::as_str) != Some(incoming.as_str()) {
            next.push(entry.clone());
            continue;
        }
        due.push(entry.clone());
        let pieces = entry.get("pieces");
        if pieces.is_some_and(|value| value.as_array().is_none()) {
            return Err(EngineError::InvalidState(
                "pendingPanic.pieces must be array".into(),
            ));
        }
    }
    if due.is_empty() {
        return Ok(());
    }
    // Delete every due entry before resolving refs. A stale/ineligible ref
    // consumes no draw; a live ref calls randomChoice even for an empty pool.
    let mut settled = state.clone();
    settled
        .extra
        .insert("pendingPanic".into(), Value::Array(next.clone()));
    let mut panic_moves = Vec::new();
    let mut panic_hidden_from = String::new();
    for entry in &due {
        let by = entry.get("by").and_then(Value::as_str);
        let target_color = if by == Some("white") {
            Color::Black
        } else {
            Color::White
        };
        let Some(refs) = entry.get("pieces").and_then(Value::as_array) else {
            continue;
        };
        for piece_ref in refs {
            let Some(id) = piece_ref.get("id").and_then(Value::as_str) else {
                continue;
            };
            let Some((at, piece)) = first_piece_by_id(&settled, id)? else {
                continue;
            };
            if piece.color != incoming
                || piece.color != target_color
                || settled.royal_identity(&piece)
                || matches!(
                    piece.kind.as_str(),
                    "merchant" | "wall" | "football" | "colossus" | "bigRook" | "bigBishop"
                )
                || next.iter().any(|pending| {
                    pending
                        .get("pieces")
                        .and_then(Value::as_array)
                        .is_some_and(|refs| {
                            refs.iter()
                                .any(|other| other.get("id").and_then(Value::as_str) == Some(id))
                        })
                })
            {
                continue;
            }
            let candidates = panic_relocation_moves(&settled, &piece, at)?;
            let draw = if candidates.is_empty() {
                settled
                    .rng
                    .sample_invariant("source empty Panic relocation")?
            } else {
                let draw = settled.rng.sample()?;
                settled.rng.record_last_probability(
                    1.0 / candidates.len() as f64,
                    "source Panic relocation destination",
                )?;
                draw
            };
            if candidates.is_empty() {
                continue;
            }
            let destination = candidates[(draw * candidates.len() as f64).floor() as usize];
            let (trace, hidden_from) =
                relocate_panicked_piece(&mut settled, piece, at, destination)?;
            if panic_hidden_from.is_empty() && matches!(hidden_from.as_str(), "white" | "black") {
                panic_hidden_from = hidden_from;
            }
            panic_moves.push(trace);
        }
    }
    if !panic_moves.is_empty() {
        let count = panic_moves.len();
        if let Some(last_move) = settled
            .extra
            .get_mut("lastMove")
            .filter(|last| js_truth(Some(last)))
        {
            let last_move = last_move.as_object_mut().ok_or_else(|| {
                EngineError::InvalidState("panic lastMove must be an object".into())
            })?;
            last_move.insert("panicMoves".into(), Value::Array(panic_moves));
            if !panic_hidden_from.is_empty() {
                last_move.insert("hiddenFrom".into(), json!(panic_hidden_from));
                settled
                    .extra
                    .insert("accelerationTrail".into(), Value::Null);
            }
        }
        crate::replay::add_log(
            &mut settled,
            format!(
                "패닉: {} 기물 {count}개가 무작위 칸으로 도망쳤습니다.",
                crate::replay::label(incoming),
            ),
        )?;
    }
    *state = settled;
    Ok(())
}

/// Source 94460-94545. Vanishing is an environmental removal: it records the
/// victim in the opponent's captures, but does not grant ordinary capture
/// types, mana, or undead scheduling. Recurrence resolves before royal loss.
fn resolve_vanishing_for_turn_start(state: &mut GameState, incoming: Color) -> Result<()> {
    // Source aiSimulationDepth is host execution context, never a board field.
    if state.is_ai_simulation() {
        return Ok(());
    }
    if ![Color::White, Color::Black]
        .into_iter()
        .any(|color| state.flag("vanishing", color))
    {
        return Ok(());
    }
    let exclude_kings = state.turns_taken.white.min(state.turns_taken.black) == 0;
    let mut seen = BTreeSet::new();
    let mut eligible = Vec::new();
    for (at, piece) in board_entries(state)? {
        if piece.color != incoming
            || matches!(
                piece.kind.as_str(),
                "wall" | "football" | "blackHole" | "coffin"
            )
        {
            continue;
        }
        let key = if piece.id.is_empty() {
            format!("{},{}", at.row, at.col)
        } else {
            piece.id.clone()
        };
        if !seen.insert(key) {
            continue;
        }
        let royal = crate::v7_piece_lifecycle::king_augment_recipient(state, &piece)?;
        if exclude_kings && royal {
            continue;
        }
        let origin = if piece.is_large() {
            let row = piece
                .extra
                .get("anchorRow")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    EngineError::InvalidState("vanishing large piece anchorRow missing".into())
                })?;
            let col = piece
                .extra
                .get("anchorCol")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    EngineError::InvalidState("vanishing large piece anchorCol missing".into())
                })?;
            square(row as usize, col as usize)?
        } else {
            at
        };
        eligible.push((origin, piece, if royal { 0.65 } else { 1.0 }));
    }
    if eligible.is_empty() {
        return Ok(());
    }
    let total_weight = eligible.iter().map(|(_, _, weight)| *weight).sum::<f64>();
    let mut next = state.clone();
    let mut roll = next.rng.sample()? * total_weight;
    let mut selected = eligible.last().cloned().ok_or_else(|| {
        EngineError::InvalidState("vanishing candidate list unexpectedly empty".into())
    })?;
    for entry in eligible {
        roll -= entry.2;
        if roll < 0.0 {
            selected = entry;
            break;
        }
    }
    let (at, victim, selected_weight) = selected;
    next.rng.record_last_probability(
        selected_weight / total_weight,
        "source Vanishing victim weight",
    )?;
    let mut privacy = Map::new();
    for viewer in [Color::White, Color::Black] {
        let visible = crate::observation::piece_visible_to_color_at_v7(&next, &victim, at, viewer)?;
        privacy.insert(viewer.as_str().into(), json!({
            "originVisible":visible,
            "typeKnown":visible || victim.color == viewer || victim.extra.get("hiddenFrom") == Some(&json!(viewer)),
        }));
    }
    crate::card_effects::mark_vanish_animation(&mut next, &victim, at)?;
    if victim.id.is_empty() {
        if victim.is_large() {
            return Err(unsupported(
                "removeVanishingPiece aliased large piece without identity",
            ));
        }
        next.board[at.row as usize][at.col as usize] = None;
    } else {
        crate::transition::clear_piece(&mut next, &victim.id);
    }
    crate::transition::grant_vigilance_protection(&mut next, &victim)?;
    if let Some(prophecy) = next
        .extra
        .get_mut("prophecy")
        .filter(|value| js_truth(Some(value)))
    {
        let entries = prophecy.as_object_mut().ok_or_else(|| {
            EngineError::InvalidState("vanishing prophecy must be a color map".into())
        })?;
        for color in [Color::White, Color::Black] {
            if js_truth(entries.get(color.as_str())) {
                entries.insert(color.as_str().into(), Value::Null);
            }
        }
    }
    let capture_owner = incoming.opponent();
    next.captures.get_mut(capture_owner).push(victim.clone());
    crate::v7_rule_bombs::mark_deathmatch_progress(&mut next)?;
    crate::threat::play_move_sound(&mut next, "capture", incoming)?;
    crate::replay::add_piece_action_log(
        &mut next,
        &victim,
        Some(at),
        Some(&Value::Object(privacy)),
        format!(
            "소멸: {}의 {} {}이 사라졌습니다.",
            square_name(at),
            crate::replay::label(incoming),
            crate::replay::source_piece_label(&victim.kind).unwrap_or(&victim.kind),
        ),
    )?;
    crate::v7_board_hazards::resolve_environmental_defeats(
        &mut next,
        &[crate::v7_board_hazards::EnvironmentalRemoval {
            piece: victim,
            square: at,
            capture_owner,
        }],
        "소멸",
        false,
    )?;
    if next.mode != "gameover" {
        crate::v7_capture_objectives::check_campaign_objectives(&mut next)?;
    }
    *state = next;
    Ok(())
}

#[derive(Clone)]
struct TrolleyPieceRef {
    id: String,
    kind: String,
    at: Square,
    value: u32,
}

impl TrolleyPieceRef {
    fn source_value(&self, color: Color) -> Value {
        json!({
            "id": self.id,
            "type": self.kind,
            "color": color,
            "row": self.at.row,
            "col": self.at.col,
            "value": self.value,
            "label": crate::replay::source_piece_label(&self.kind)
                .unwrap_or(if self.kind.is_empty() { "기물" } else { &self.kind }),
        })
    }
}

#[derive(Clone)]
struct TrolleyBundle {
    pieces: Vec<TrolleyPieceRef>,
    value: u32,
}

impl TrolleyBundle {
    fn source_value(&self, color: Color, id: &str) -> Value {
        json!({
            "id": id,
            "pieces": self.pieces.iter().map(|piece| piece.source_value(color)).collect::<Vec<_>>(),
            "value": self.value,
        })
    }
}

fn source_shuffle<T>(items: &mut [T], rng: &mut RngState) -> Result<()> {
    for index in (1..items.len()).rev() {
        let other = (rng.sample()? * (index + 1) as f64).floor() as usize;
        rng.record_last_probability(
            1.0 / (index + 1) as f64,
            "source Trolley activation shuffle",
        )?;
        items.swap(index, other);
    }
    Ok(())
}

fn trolley_pieces(state: &GameState, incoming: Color) -> Result<Vec<TrolleyPieceRef>> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for (at, piece) in board_entries(state)? {
        if piece.color != incoming
            || state.royal_identity(&piece)
            || matches!(piece.kind.as_str(), "wall" | "football" | "blackHole")
        {
            continue;
        }
        // Number.isFinite(null) is false: unknown piece values are excluded
        // before the first shuffle, while values above 10 leave after it.
        let Some(value) = crate::eligibility::v7_piece_combat_value(state, &piece.kind)? else {
            continue;
        };
        let id = if piece.id.is_empty() {
            format!("{}:{}", at.row, at.col)
        } else {
            piece.id
        };
        if seen.insert(id.clone()) {
            result.push(TrolleyPieceRef {
                id,
                kind: piece.kind,
                at,
                value,
            });
        }
    }
    Ok(result)
}

fn trolley_bundles(pieces: &[TrolleyPieceRef]) -> [Vec<TrolleyBundle>; 11] {
    fn walk(
        pieces: &[TrolleyPieceRef],
        start: usize,
        picked: &mut Vec<TrolleyPieceRef>,
        score: u32,
        buckets: &mut [Vec<TrolleyBundle>; 11],
    ) {
        if score > 10 {
            return;
        }
        if score >= 2 && buckets[score as usize].len() < 240 {
            buckets[score as usize].push(TrolleyBundle {
                pieces: picked.clone(),
                value: score,
            });
        }
        if picked.len() >= 4 {
            return;
        }
        for index in start..pieces.len() {
            let value = pieces[index].value;
            if score + value > 10 {
                continue;
            }
            picked.push(pieces[index].clone());
            walk(pieces, index + 1, picked, score + value, buckets);
            picked.pop();
        }
    }
    let mut buckets: [Vec<TrolleyBundle>; 11] = std::array::from_fn(|_| Vec::new());
    walk(pieces, 0, &mut Vec::new(), 0, &mut buckets);
    buckets
}

fn trolley_pairs(
    buckets: &[Vec<TrolleyBundle>; 11],
    low_score: usize,
    high_score: usize,
) -> Vec<(TrolleyBundle, TrolleyBundle)> {
    let mut pairs = Vec::new();
    for (first_index, left) in buckets[low_score].iter().enumerate() {
        for (second_index, right) in buckets[high_score].iter().enumerate() {
            if low_score == high_score && second_index <= first_index {
                continue;
            }
            let left_ids = left
                .pieces
                .iter()
                .map(|piece| &piece.id)
                .collect::<BTreeSet<_>>();
            if right
                .pieces
                .iter()
                .all(|piece| !left_ids.contains(&piece.id))
            {
                pairs.push((left.clone(), right.clone()));
            }
        }
    }
    pairs
}

fn trolley_dilemma(state: &mut GameState, incoming: Color, by: Color) -> Result<Option<Value>> {
    let mut pieces = trolley_pieces(state, incoming)?;
    source_shuffle(&mut pieces, &mut state.rng)?;
    pieces.retain(|piece| piece.value > 0 && piece.value <= 10);
    pieces.truncate(32);
    let buckets = trolley_bundles(&pieces);
    let mut lows = (2_usize..=8).collect::<Vec<_>>();
    source_shuffle(&mut lows, &mut state.rng)?;
    for low in lows {
        let mut high_candidates = Vec::new();
        for high in low..=usize::min(10, low + 2) {
            let pairs = trolley_pairs(&buckets, low, high);
            if !pairs.is_empty() {
                high_candidates.push(pairs);
            }
        }
        if high_candidates.is_empty() {
            continue;
        }
        let high_index = (state.rng.sample()? * high_candidates.len() as f64).floor() as usize;
        state.rng.record_last_probability(
            1.0 / high_candidates.len() as f64,
            "source Trolley high bucket",
        )?;
        let pairs = &high_candidates[high_index];
        let pair_index = (state.rng.sample()? * pairs.len() as f64).floor() as usize;
        state
            .rng
            .record_last_probability(1.0 / pairs.len() as f64, "source Trolley pair")?;
        let (mut left, mut right) = pairs[pair_index].clone();
        let direction = state.rng.sample()?;
        state
            .rng
            .record_last_probability(0.5, "source Trolley orientation")?;
        if direction >= 0.5 {
            std::mem::swap(&mut left, &mut right);
        }
        let timestamp = crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?;
        let suffix = crate::draft::random_suffix(
            state.rng.sample_opaque("source active Trolley identity")?,
        )?;
        return Ok(Some(json!({
            "id": format!("trolley-{timestamp}-{suffix}"),
            "color": incoming,
            "by": by,
            "choices": [left.source_value(incoming, "left"), right.source_value(incoming, "right")],
            "createdMove": state.move_count,
            "choiceStartedAt": timestamp,
        })));
    }
    Ok(None)
}

fn settle_pending_trolley(state: &mut GameState, incoming: Color) -> Result<()> {
    if js_truth(state.extra.get("activeTrolley")) {
        return Ok(());
    }
    let Some(value) = state.extra.get("pendingTrolley") else {
        return Ok(());
    };
    let mut entries = value
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("pendingTrolley must be array".into()))?
        .clone();
    let Some(index) = entries
        .iter()
        .position(|entry| entry.get("color").and_then(Value::as_str) == Some(incoming.as_str()))
    else {
        return Ok(());
    };
    let mut next = state.clone();
    let pending = entries.remove(index);
    next.extra
        .insert("pendingTrolley".into(), Value::Array(entries));
    let by = match pending.get("by").and_then(Value::as_str) {
        Some("white") => Color::White,
        Some("black") => Color::Black,
        None => incoming.opponent(),
        Some(_) => {
            return Err(EngineError::InvalidState(
                "pendingTrolley.by must be color".into(),
            ));
        }
    };
    if let Some(mut active) = trolley_dilemma(&mut next, incoming, by)? {
        if let Some(id) = pending.get("id").filter(|value| js_truth(Some(value))) {
            active["id"] = id.clone();
        }
        // The shared local oracle clock uses the verified frozen timestamp
        // and cold display anchor, and reports malformed balances exactly.
        crate::flow::pause_clock(&mut next)?;
        next.extra.insert("activeTrolley".into(), active);
        next.extra.insert("selected".into(), Value::Null);
        next.extra.insert("legalMoves".into(), json!([]));
        next.extra.insert("targeting".into(), Value::Null);
        next.extra.insert("barricadePreview".into(), Value::Null);
        next.extra
            .insert("barricadeDirectionChoice".into(), Value::Null);
        crate::replay::add_log(
            &mut next,
            format!(
                "트롤리: {}이 딜레마에 빠졌습니다.",
                crate::replay::label(incoming)
            ),
        )?;
    } else {
        crate::replay::add_log(
            &mut next,
            format!(
                "트롤리: {}에게 보낼 수 있는 딜레마가 없어 트롤리가 지나갔습니다.",
                crate::replay::label(incoming)
            ),
        )?;
    }
    *state = next;
    Ok(())
}

/// Source 93743-93754. Incoming automatic callbacks preserve their ordered
/// state mutations, random draws, and interruptions. Shared callbacks report
/// unsupported active branches without committing the cloned turn state.
pub(crate) fn resolve_incoming_automatic_effects(
    state: &mut GameState,
    incoming: Color,
) -> Result<V7FlowControl> {
    if check_entry(state, incoming)? == V7FlowControl::Terminal {
        return Ok(V7FlowControl::Terminal);
    }
    let mut next = state.clone();
    resolve_vanishing_for_turn_start(&mut next, incoming)?;
    if next.mode == "gameover" {
        *state = next;
        return Ok(V7FlowControl::Terminal);
    }
    resolve_baby_bear_growth(&mut next, incoming)?;
    auto_move_baby_bears(&mut next, incoming)?;
    resolve_twin_swaps(&mut next)?;
    activate_pending_draft_cards(&mut next, incoming)?;
    if next.mode == "gameover" {
        *state = next;
        return Ok(V7FlowControl::Terminal);
    }
    tick_rule_tickets(&mut next, incoming)?;
    if next.mode == "gameover" {
        *state = next;
        return Ok(V7FlowControl::Terminal);
    }
    apply_white_box_notices(&mut next, incoming)?;
    settle_pending_panic(&mut next, incoming)?;
    settle_pending_trolley(&mut next, incoming)?;
    let terminal = next.mode == "gameover";
    *state = next;
    Ok(if terminal {
        V7FlowControl::Terminal
    } else {
        V7FlowControl::Continue
    })
}

/// Source 93757; call before coronation and before the local Don callback.
pub(crate) fn clear_turn_start_acceleration_trail(
    state: &mut GameState,
    incoming: Color,
) -> Result<V7FlowControl> {
    if check_entry(state, incoming)? == V7FlowControl::Terminal {
        return Ok(V7FlowControl::Terminal);
    }
    if state
        .extra
        .get("accelerationTrail")
        .and_then(|trail| trail.get("clearOnTurnStart"))
        .and_then(Value::as_str)
        == Some(incoming.as_str())
    {
        state.extra.insert("accelerationTrail".into(), Value::Null);
    }
    Ok(V7FlowControl::Continue)
}

/// Source 93763/95098, after Don and before action-limit calculation.
pub(crate) fn apply_pending_acceleration(
    state: &mut GameState,
    incoming: Color,
) -> Result<V7FlowControl> {
    if check_entry(state, incoming)? == V7FlowControl::Terminal {
        return Ok(V7FlowControl::Terminal);
    }
    let Some(pending) = state.extra.get("accelerationPendingFor") else {
        return Ok(V7FlowControl::Continue);
    };
    if pending.is_null() || pending == &json!(false) || pending == &json!("") {
        return Ok(V7FlowControl::Continue);
    }
    let pending = pending.as_str().ok_or_else(|| {
        EngineError::InvalidState("accelerationPendingFor must be color or null".into())
    })?;
    if !matches!(pending, "white" | "black") {
        return Err(EngineError::InvalidState(
            "accelerationPendingFor must be a player color".into(),
        ));
    }
    if pending != incoming.as_str() {
        return Ok(V7FlowControl::Continue);
    }
    let completed = state.turns_taken.black;
    let starts_after = state
        .extra
        .get("accelerationStartsAfterBlackTurns")
        .filter(|value| !value.is_null())
        .map(|value| {
            value.as_u64().ok_or_else(|| {
                EngineError::InvalidState(
                    "accelerationStartsAfterBlackTurns must be integer".into(),
                )
            })
        })
        .transpose()?
        .unwrap_or(2);
    if u64::from(completed) < starts_after {
        state.extra.insert(
            "accelerationPendingTurns".into(),
            json!(starts_after - u64::from(completed)),
        );
    } else {
        state.extra.insert("acceleration".into(), json!(true));
        state
            .extra
            .insert("accelerationPendingFor".into(), Value::Null);
        state
            .extra
            .insert("accelerationPendingTurns".into(), json!(0));
        crate::replay::add_log(
            state,
            "가속이 적용되어 모든 플레이어가 한 턴에 2번 행동합니다.".into(),
        )?;
    }
    Ok(V7FlowControl::Continue)
}

fn grant_merchant_gold(state: &mut GameState, incoming: Color) -> Result<()> {
    if !state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|piece| piece.color == incoming && piece.kind == "merchant")
    {
        return Ok(());
    }
    let mut seen = BTreeSet::new();
    for (at, mut piece) in board_entries(state)? {
        if piece.color != incoming || piece.kind != "merchant" || !seen.insert(piece.id.clone()) {
            continue;
        }
        let gold = piece.extra.get("gold").map_or(Ok(0), |value| {
            if value.is_null() {
                Ok(0)
            } else {
                value.as_i64().ok_or_else(|| {
                    EngineError::InvalidState("merchant.gold must be integer".into())
                })
            }
        })?;
        let gold = gold
            .checked_add(1)
            .ok_or_else(|| EngineError::InvalidState("merchant.gold overflow".into()))?;
        piece.extra.insert("gold".into(), json!(gold));
        state.board[usize::from(at.row)][usize::from(at.col)] = Some(piece.clone());
        crate::replay::add_piece_action_log(
            state,
            &piece,
            Some(at),
            None,
            format!(
                "{} 상인이 1골드를 얻었습니다. ({gold})",
                crate::replay::label(incoming)
            ),
        )?;
    }
    Ok(())
}

/// The client invokes this after `finalizeMove` and again in incoming-turn
/// economy. The used flag makes the second visit inert after a successful
/// first visit; both source boundaries must remain observable.
pub(crate) fn check_conscription(state: &mut GameState, color: Color) -> Result<()> {
    if !state.flag("conscription", color)
        || state.flag("conscriptionUsed", color)
        || state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.color == color && piece.kind == "pawn")
    {
        return Ok(());
    }
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(unsupported("checkConscription non-8x8 geometry"));
    }
    state.set_flag("conscriptionUsed", color, true);
    let row = if color == Color::White { 6 } else { 1 };
    let mut created = 0;
    for col in 2..=5 {
        if state.board[row][col].is_some() {
            continue;
        }
        let at = square(row, col)?;
        let mut piece = crate::opening::spawn(state, color, "pawn")?;
        piece.extra.insert(
            "freshNoCaptureUntil".into(),
            json!(state.turns_taken.get(color).checked_add(1).ok_or_else(|| {
                EngineError::InvalidState("conscription fresh capture deadline overflow".into())
            })?),
        );
        piece.extra.insert("origin".into(), json!(square_name(at)));
        if js_truth(state.extra.get("monochromeChess")) {
            piece.extra.insert(
                "monoShade".into(),
                json!(if (row + col).is_multiple_of(2) {
                    "light"
                } else {
                    "dark"
                }),
            );
        }
        state.board[row][col] = Some(piece);
        created += 1;
    }
    crate::replay::add_log(
        state,
        format!(
            "{} 징집: {created}개의 폰이 소환되었습니다.",
            crate::replay::label(color)
        ),
    )?;
    Ok(())
}

fn update_palaces(state: &mut GameState) -> Result<()> {
    let Some(value) = state.extra.get("palaces") else {
        return Ok(());
    };
    let palaces = value
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("palaces must be array".into()))?
        .clone();
    let mut kept = Vec::new();
    for palace in palaces {
        let color = palace
            .get("color")
            .and_then(Value::as_str)
            .and_then(|color| match color {
                "white" => Some(Color::White),
                "black" => Some(Color::Black),
                _ => None,
            })
            .ok_or_else(|| EngineError::InvalidState("palace.color invalid".into()))?;
        let cells = palace
            .get("cells")
            .and_then(Value::as_array)
            .ok_or_else(|| EngineError::InvalidState("palace.cells must be array".into()))?;
        let royal = board_entries(state)?
            .into_iter()
            .find(|(_, piece)| piece.color == color && state.royal_identity(piece));
        let Some((at, _)) = royal else {
            continue;
        };
        let inside = cells.iter().any(|cell| {
            cell.get("row").and_then(Value::as_u64) == Some(u64::from(at.row))
                && cell.get("col").and_then(Value::as_u64) == Some(u64::from(at.col))
        });
        if inside {
            kept.push(palace);
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
    state.extra.insert("palaces".into(), Value::Array(kept));
    Ok(())
}

/// Source 93767-93770, after the incoming clock starts and before the final
/// herald/termination checks. The caller owns the action limit and clock.
pub(crate) fn settle_incoming_economy(
    state: &mut GameState,
    incoming: Color,
) -> Result<V7FlowControl> {
    if check_entry(state, incoming)? == V7FlowControl::Terminal {
        return Ok(V7FlowControl::Terminal);
    }
    grant_merchant_gold(state, incoming)?;
    check_conscription(state, Color::White)?;
    check_conscription(state, Color::Black)?;
    update_palaces(state)?;
    Ok(V7FlowControl::Continue)
}

/// Atomic convenience for an isolated incoming-turn callback profile. The
/// source-order phase functions above are the integration API when other
/// callbacks are present between them.
#[allow(
    dead_code,
    reason = "isolated turn-entry contract and phase-level tests"
)]
pub(crate) fn after_switch(state: &mut GameState, incoming: Color) -> Result<V7FlowControl> {
    if check_entry(state, incoming)? == V7FlowControl::Terminal {
        return Ok(V7FlowControl::Terminal);
    }
    let mut next = state.clone();
    for phase in [
        resolve_vip_invitations,
        resolve_pending_icbm,
        tick_siren_exposure_for_turn_start,
        resolve_incoming_automatic_effects,
        clear_turn_start_acceleration_trail,
        apply_pending_acceleration,
        settle_incoming_economy,
    ] {
        if phase(&mut next, incoming)? == V7FlowControl::Terminal {
            *state = next;
            return Ok(V7FlowControl::Terminal);
        }
    }
    *state = next;
    Ok(V7FlowControl::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;

    fn empty_play() -> GameState {
        let mut state = GameState::new(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.mode = "play".into();
        state.board = vec![vec![None; 8]; 8];
        state.extra.insert("logs".into(), json!([]));
        state.extra.shift_remove("sirenExposure");
        state
    }

    #[test]
    fn vip_due_only_on_own_incoming_turn_and_keeps_rng() {
        let mut state = empty_play();
        state.turn = Color::White;
        state.turns_taken.white = 2;
        let mut pawn = Piece::new("pawn", Color::White, "white-pawn-vip");
        pawn.extra.insert(
            "vipInvitation".into(),
            json!({"by":"black","triggerTurn":2}),
        );
        state.board[5][2] = Some(pawn);
        let rng = state.rng.clone();
        resolve_vip_invitations(&mut state, Color::White).unwrap();
        let vip = state.at(Square { row: 5, col: 2 }).unwrap();
        assert_eq!(vip.kind, "vip");
        assert_eq!(vip.extra["origin"], json!("c3"));
        assert!(!vip.extra.contains_key("vipInvitation"));
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn siren_turn_start_key_is_idempotent_and_exposure_resets_on_departure() {
        let mut state = empty_play();
        state.board[4][4] = Some(Piece::new("siren", Color::White, "siren-a"));
        state.board[4][5] = Some(Piece::new("pawn", Color::Black, "pawn-b"));
        assert!(has_siren_ability(state.board[4][4].as_ref().unwrap()));
        assert!(siren_convertible_target(
            state.board[4][5].as_ref().unwrap()
        ));
        let rng = state.rng.clone();
        tick_siren_exposure_for_turn_start(&mut state, Color::White).unwrap();
        assert_eq!(
            state.extra["sirenExposure"]["__turnStartKey"],
            json!("white:0")
        );
        assert_eq!(state.extra["sirenExposure"]["siren-a"]["pawn-b"], json!(1));
        let first = state.clone();
        tick_siren_exposure_for_turn_start(&mut state, Color::White).unwrap();
        assert_eq!(state, first);
        state.board[4][5] = None;
        state.turns_taken.white = 1;
        tick_siren_exposure_for_turn_start(&mut state, Color::White).unwrap();
        assert_eq!(state.extra["sirenExposure"]["siren-a"], json!({}));
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn siren_explicit_other_turn_color_is_inert_without_changing_actor() {
        let mut state = empty_play();
        state.turn = Color::Black;
        state.turns_taken.white = 1;
        state.board[4][4] = Some(Piece::new("siren", Color::White, "siren-a"));
        state.board[4][5] = Some(Piece::new("pawn", Color::Black, "pawn-b"));
        state.extra.insert(
            "sirenExposure".into(),
            json!({
                "__turnStartKey":"white:0", "siren-a":{"pawn-b":1},
            }),
        );
        let before = state.clone();
        assert_eq!(
            tick_siren_exposure_for_turn_start(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue,
        );
        assert_eq!(state, before);
    }

    #[test]
    fn siren_nonplay_clock_calls_are_inert() {
        let state = empty_play();
        for (mode, expected) in [
            ("draft", V7FlowControl::Continue),
            ("promotion", V7FlowControl::Continue),
            ("gameover", V7FlowControl::Terminal),
        ] {
            let mut inactive = state.clone();
            inactive.mode = mode.into();
            let before = inactive.clone();
            assert_eq!(
                tick_siren_exposure_for_turn_start(&mut inactive, Color::White).unwrap(),
                expected,
            );
            assert_eq!(inactive, before);
        }
    }

    #[test]
    fn siren_converts_adjacent_nonroyal_on_second_exposure_without_rng() {
        let mut state = empty_play();
        state.board[4][4] = Some(Piece::new("siren", Color::White, "siren-a"));
        state.board[4][5] = Some(Piece::new("pawn", Color::Black, "pawn-b"));
        let rng = state.rng.clone();
        tick_siren_exposure_for_turn_start(&mut state, Color::White).unwrap();
        state.turns_taken.white = 1;
        tick_siren_exposure_for_turn_start(&mut state, Color::White).unwrap();
        let pawn = state.at(Square { row: 4, col: 5 }).unwrap();
        assert_eq!(pawn.color, Color::White);
        assert!(pawn.moved);
        assert_eq!(pawn.extra["origin"], json!("f4"));
        assert_eq!(pawn.extra["coolGuyCapturedLast"], json!(false));
        assert_eq!(state.extra["sirenExposure"]["siren-a"], json!({}));
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn siren_royal_defection_changes_board_before_former_side_defeat() {
        let mut state = empty_play();
        state.board[4][4] = Some(Piece::new("siren", Color::White, "siren-a"));
        state.board[4][5] = Some(Piece::new("king", Color::Black, "king-b"));
        let rng = state.rng.clone();
        tick_siren_exposure_for_turn_start(&mut state, Color::White).unwrap();
        state.turns_taken.white = 1;
        assert_eq!(
            tick_siren_exposure_for_turn_start(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal,
        );
        let king = state.at(Square { row: 4, col: 5 }).unwrap();
        assert_eq!(king.id, "king-b");
        assert_eq!(king.color, Color::White);
        assert!(king.moved);
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner.as_deref(), Some("white"));
        assert!(state.flag("kingDead", Color::Black));
        assert!(state.captures.white.is_empty() && state.captures.black.is_empty());
        assert!(!state.extra.contains_key("kingThreatCaptureCauses"));
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn converted_siren_participates_later_in_the_same_incoming_turn() {
        let mut state = empty_play();
        state.board[4][4] = Some(Piece::new("siren", Color::White, "siren-a"));
        state.board[4][5] = Some(Piece::new("siren", Color::Black, "siren-b"));
        state.board[4][6] = Some(Piece::new("pawn", Color::Black, "pawn-b"));
        state.extra.insert(
            "sirenExposure".into(),
            json!({
                "siren-a":{"siren-b":"1.9"},"siren-b":{"pawn-b":1},
            }),
        );
        let rng = state.rng.clone();
        tick_siren_exposure_for_turn_start(&mut state, Color::White).unwrap();
        assert_eq!(state.board[4][5].as_ref().unwrap().color, Color::White);
        assert_eq!(state.board[4][6].as_ref().unwrap().color, Color::Black);
        assert_eq!(state.extra["sirenExposure"]["siren-b"]["pawn-b"], json!(1));
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn missing_icbm_queen_cancels_and_live_launch_removes_source_then_explodes() {
        let mut state = empty_play();
        state.turns_taken.white = 1;
        state.extra.insert("pendingIcbm".into(), json!([{"color":"white","triggerTurn":1,"sourceQueenId":"gone","targetQueenId":"target"}]));
        resolve_pending_icbm(&mut state, Color::White).unwrap();
        assert_eq!(state.extra["pendingIcbm"], json!([]));
        assert_eq!(
            state.extra["logs"][0],
            json!("ICBM 취소: 백 퀸이 사라졌거나 다른 기물로 변경되었습니다.")
        );
        state.board[5][0] = Some(Piece::new("queen", Color::White, "source"));
        state.board[2][0] = Some(Piece::new("queen", Color::Black, "target"));
        state.extra.insert("pendingIcbm".into(), json!([{"color":"white","triggerTurn":1,"sourceQueenId":"source","targetQueenId":"target"}]));
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        resolve_pending_icbm(&mut state, Color::White).unwrap();
        assert_eq!(state.extra["pendingIcbm"], json!([]));
        assert!(state.board[5][0].is_none());
        assert!(state.board[2][0].is_none());
        assert!(
            state
                .captures
                .black
                .iter()
                .any(|piece| piece.id == "source")
        );
        assert!(
            state
                .captures
                .white
                .iter()
                .any(|piece| piece.id == "target")
        );
        assert_eq!(state.rng, expected_rng);
    }

    #[test]
    fn growth_occurs_before_automatic_bear_move_and_is_deterministic() {
        let mut state = empty_play();
        state.turns_taken.white = 3;
        state.turns_taken.black = 3;
        let mut cub = Piece::new("babyBear", Color::White, "cub");
        cub.extra.insert("babyBearGrowAtTurn".into(), json!(3));
        state.board[4][4] = Some(cub);
        resolve_incoming_automatic_effects(&mut state, Color::White).unwrap();
        let bear = state.at(Square { row: 4, col: 4 }).unwrap();
        assert_eq!(bear.kind, "bear");
        assert_eq!(bear.extra["bearRetaliationsRemaining"], json!(2));
        assert!(!bear.extra.contains_key("babyBearGrowAtTurn"));
    }

    #[test]
    fn frozen_baby_bear_automatic_move_preserves_prior_highlight_and_appends_trace() {
        let mut state = empty_play();
        let mut cub = Piece::new("babyBear", Color::White, "auto-cub");
        cub.extra.insert("frozen".into(), json!(true));
        state.board[4][4] = Some(cub);
        let previous = json!({
            "from":{"row":6,"col":0},"to":{"row":5,"col":0},
            "soundName":"moveSelf","soundColor":"white","hiddenFrom":"",
        });
        state.extra.insert("lastMove".into(), previous.clone());
        state.extra.insert("acceleration".into(), json!(true));
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        expected_rng.sample().unwrap();
        resolve_incoming_automatic_effects(&mut state, Color::White).unwrap();
        assert!(state.board[4][4].is_none());
        let (at, moved) = first_piece_by_id(&state, "auto-cub").unwrap().unwrap();
        assert_eq!(moved.kind, "babyBear");
        assert!(moved.moved && moved.flag("frozen"));
        assert_eq!(state.extra["lastMove"]["from"], previous["from"]);
        assert_eq!(state.extra["lastMove"]["to"], previous["to"]);
        assert_eq!(
            state.extra["lastMove"]["logMoves"][0]["from"],
            json!({"row":4,"col":4})
        );
        assert_eq!(state.extra["lastMove"]["logMoves"][0]["to"], json!(at));
        assert_eq!(
            state.extra["accelerationTrail"]["cells"],
            json!([{"row":4,"col":4},at])
        );
        assert_eq!(state.rng, expected_rng);
    }

    #[test]
    fn blocked_baby_bear_automatic_move_consumes_empty_choice_draw() {
        let mut state = empty_play();
        for row in 0..8 {
            for col in 0..8 {
                state.board[row][col] = Some(Piece::new(
                    "wall",
                    crate::PieceColor::Neutral,
                    format!("wall-{row}-{col}"),
                ));
            }
        }
        state.board[4][4] = Some(Piece::new("babyBear", Color::White, "blocked-cub"));
        let before = state.clone();
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        resolve_incoming_automatic_effects(&mut state, Color::White).unwrap();
        assert_eq!(state.board, before.board);
        assert_eq!(state.rng, expected_rng);
        assert_eq!(state.extra.get("lastMove"), before.extra.get("lastMove"));
        assert_eq!(
            state.extra.get("pendingNotations"),
            before.extra.get("pendingNotations")
        );
    }

    #[test]
    fn first_turn_vanishing_with_only_kings_has_no_draw_or_removal() {
        let mut state = empty_play();
        state.board[7][4] = Some(Piece::new("king", Color::White, "white-king"));
        state
            .extra
            .insert("vanishing".into(), json!({"white":true,"black":false}));
        let before = state.clone();
        assert_eq!(
            resolve_incoming_automatic_effects(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.rng, before.rng);
        assert!(state.board[7][4].is_some());
        state.turns_taken.white = 1;
        state.turns_taken.black = 1;
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        assert_eq!(
            resolve_incoming_automatic_effects(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal,
        );
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner.as_deref(), Some("black"));
        assert!(state.board[7][4].is_none());
        assert_eq!(state.captures.black[0].id, "white-king");
        assert_eq!(state.rng, expected_rng);
    }

    #[test]
    fn vanishing_threat_simulation_does_not_remove_or_consume_rng() {
        for (ai_depth, threat_depth) in [(1, 0), (0, 1)] {
            let mut state = empty_play();
            state.board[4][4] = Some(Piece::new("rook", Color::White, "probe-rook"));
            state
                .extra
                .insert("vanishing".into(), json!({"white":true,"black":false}));
            state.ai_simulation_depth = ai_depth;
            state.threat_probe_depth = threat_depth;
            let before = state.clone();
            resolve_vanishing_for_turn_start(&mut state, Color::White).unwrap();
            assert_eq!(state, before);
        }
    }

    #[test]
    fn stale_pending_panic_refs_clear_without_a_random_draw() {
        let mut state = empty_play();
        state.extra.insert(
            "pendingPanic".into(),
            json!([
                {"color":"white","by":"black","pieces":[{"id":"gone"}]},
                {"color":"black","by":"white","pieces":[{"id":"later"}]}
            ]),
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        settle_pending_panic(&mut state, Color::White).unwrap();
        assert_eq!(
            state.extra["pendingPanic"],
            json!([
                {"color":"black","by":"white","pieces":[{"id":"later"}]}
            ])
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
        let mut frozen = Piece::new("pawn", Color::White, "pawn-live");
        frozen.extra.insert("frozen".into(), json!(true));
        state.board[4][4] = Some(frozen);
        state.extra.insert(
            "pendingPanic".into(),
            json!([{"color":"white","by":"black","pieces":[{"id":"pawn-live"}]}]),
        );
        let board = state.board.clone();
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        settle_pending_panic(&mut state, Color::White).unwrap();
        assert_eq!(state.board, board);
        assert_eq!(state.extra["pendingPanic"], json!([]));
        assert_eq!(state.rng, expected_rng);
    }

    #[test]
    fn panic_relocates_once_and_records_movement_without_capture() {
        let mut state = empty_play();
        let mut knight = Piece::new("knight", Color::White, "panicked-knight");
        knight.extra.insert("quantum".into(), json!(false));
        state.board[5][2] = Some(knight);
        state.extra.insert(
            "pendingPanic".into(),
            json!([{"color":"white","by":"black","pieces":[{"id":"panicked-knight"}]}]),
        );
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        settle_pending_panic(&mut state, Color::White).unwrap();
        assert!(state.board[5][2].is_none());
        let (destination, moved) = first_piece_by_id(&state, "panicked-knight")
            .unwrap()
            .unwrap();
        assert_ne!(destination, Square { row: 5, col: 2 });
        assert!(moved.moved);
        assert!(!moved.extra.contains_key("quantum"));
        assert_eq!(
            state.extra["lastMove"]["panicMoves"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            state.extra["lastMove"]["panicMoves"][0]["pieceId"],
            json!("panicked-knight")
        );
        assert_eq!(state.rng, expected_rng);
        assert!(state.captures.white.is_empty() && state.captures.black.is_empty());
        assert_eq!(
            state.extra["logs"][0],
            json!("패닉: 백 기물 1개가 무작위 칸으로 도망쳤습니다.")
        );
    }

    #[test]
    fn trolley_activation_consumes_source_shuffle_and_clears_one_queue_entry() {
        let mut state = empty_play();
        state.extra.insert(
            "pendingTrolley".into(),
            json!([
                {"id":"first","color":"white","by":"black"},
                {"id":"second","color":"white","by":"black"}
            ]),
        );
        let mut expected_rng = state.rng.clone();
        for _ in 0..6 {
            expected_rng.sample().unwrap();
        }
        settle_pending_trolley(&mut state, Color::White).unwrap();
        assert_eq!(state.rng, expected_rng);
        assert_eq!(
            state.extra["pendingTrolley"],
            json!([
                {"id":"second","color":"white","by":"black"}
            ])
        );
        assert_eq!(
            state.extra["logs"][0],
            json!("트롤리: 백에게 보낼 수 있는 딜레마가 없어 트롤리가 지나갔습니다.")
        );

        state.board[4][3] = Some(Piece::new("rook", Color::White, "rook-a"));
        state.board[4][4] = Some(Piece::new("rook", Color::White, "rook-b"));
        let before = state.clone();
        settle_pending_trolley(&mut state, Color::White).unwrap();
        assert_ne!(state, before);
        assert_eq!(state.extra["activeTrolley"]["color"], "white");
        assert_eq!(state.extra["activeTrolley"]["by"], "black");
        assert_eq!(state.extra["activeTrolley"]["choices"][0]["id"], "left");
        assert_eq!(state.extra["activeTrolley"]["choices"][1]["id"], "right");
    }

    #[test]
    fn trolley_pair_matches_frozen_client_full_state_and_rng() {
        // Direct activatePendingTrolleyForTurn('white') on the frozen source
        // e5ed84fc...302c45c, normal draftDelete seed 19. The source's
        // Date.now is pinned to baseline.frozenAt by the headless profile.
        // Full source Positions preserve the original state/RNG/history;
        // their IDs include the adopted faithful execution catalog identity.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        assert_eq!(state.mode, "play");
        state.board = vec![vec![None; 8]; 8];
        state.turn = Color::White;
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert(
            "pendingTrolley".into(),
            json!([{"id":"first","color":"white","by":"black"}]),
        );
        state.board[4][3] = Some(Piece::new("rook", Color::White, "rook-a"));
        state.board[4][4] = Some(Piece::new("rook", Color::White, "rook-b"));
        assert_eq!(
            crate::v7_host::V7HostPosition::from_state(state.clone())
                .unwrap()
                .position_id(),
            "95edd90bb8780d2ee9425a9080e8775aa2ea2793cd8c50b563837d218d3ceb3f"
        );
        settle_pending_trolley(&mut state, Color::White).unwrap();
        assert_eq!(state.rng.cursor, 43);
        assert_eq!(
            crate::v7_host::V7HostPosition::from_state(state)
                .unwrap()
                .position_id(),
            "374919a83dcc5346c04a451f720eb1f905eea919edc88d3d0c2aeb8c917ab95d"
        );
    }

    #[test]
    fn trolley_with_cold_running_clock_preserves_balances_and_clears_anchor() {
        let mut state = empty_play();
        state.board[4][3] = Some(Piece::new("rook", Color::White, "rook-a"));
        state.board[4][4] = Some(Piece::new("rook", Color::White, "rook-b"));
        state.extra.insert(
            "pendingTrolley".into(),
            json!([{"id":"cold","color":"white","by":"black"}]),
        );
        state.extra.insert(
            "clock".into(),
            json!({
                "enabled":true,"runningColor":"white","lastStartedAt":null,
                "whiteMs":123456,"blackMs":654321,
            }),
        );
        settle_pending_trolley(&mut state, Color::White).unwrap();
        assert_eq!(state.extra["activeTrolley"]["id"], "cold");
        assert_eq!(state.extra["clock"]["whiteMs"], 123456);
        assert_eq!(state.extra["clock"]["blackMs"], 654321);
        assert!(state.extra["clock"]["runningColor"].is_null());
        assert!(state.extra["clock"]["lastStartedAt"].is_null());
    }

    #[test]
    fn acceleration_and_merchant_gold_wait_for_their_source_stages() {
        let mut state = empty_play();
        state.turn = Color::Black;
        state.turns_taken.black = 2;
        state
            .extra
            .insert("accelerationPendingFor".into(), json!("black"));
        state
            .extra
            .insert("accelerationStartsAfterBlackTurns".into(), json!(2));
        state.board[3][2] = Some(Piece::new("merchant", Color::Black, "merchant"));
        apply_pending_acceleration(&mut state, Color::Black).unwrap();
        assert_eq!(state.extra["acceleration"], json!(true));
        settle_incoming_economy(&mut state, Color::Black).unwrap();
        assert_eq!(
            state.at(Square { row: 3, col: 2 }).unwrap().extra["gold"],
            json!(1)
        );
    }

    #[test]
    fn white_box_play_start_opening_handles_both_sides_and_other_phase_only_actor() {
        let definition =
            crate::card_registry::definition_for(RULES_VERSION_V7, "acceleration").unwrap();
        let card: crate::CardSlot =
            serde_json::from_value(definition.source_definition.clone()).unwrap();
        let mut baseline = empty_play();
        crate::draft::v7_player_deck(&mut baseline, Color::White).unwrap()[0] = card.clone();
        crate::draft::v7_player_deck(&mut baseline, Color::Black).unwrap()[0] = card.clone();
        baseline.deck_slots.white[0]
            .extra
            .insert("whiteBoxPendingNotice".into(), json!("white notice"));
        baseline.deck_slots.black[0]
            .extra
            .insert("whiteBoxPendingNotice".into(), json!({"notice":"black"}));
        let mut developer = card;
        developer.extra.insert("devCard".into(), json!(true));
        developer
            .extra
            .insert("whiteBoxPendingNotice".into(), json!("developer notice"));
        baseline.deck_slots.white[1] = developer;
        for (phase, shown, consumed_white) in [("OPENING", 2, true), ("MIDDLE", 1, false)] {
            let mut state = baseline.clone();
            let rng = state.rng.clone();
            let salt = state
                .extra
                .get("repetitionSalt")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            assert_eq!(
                apply_pending_white_boxes_for_play_start(&mut state, Color::Black, phase).unwrap(),
                shown
            );
            assert_eq!(
                state.deck_slots.white[0]
                    .extra
                    .contains_key("whiteBoxPendingNotice"),
                !consumed_white
            );
            assert!(
                !state.deck_slots.black[0]
                    .extra
                    .contains_key("whiteBoxPendingNotice")
            );
            assert_eq!(
                state.deck_slots.white[1].extra["whiteBoxPendingNotice"],
                json!("developer notice")
            );
            assert_eq!(state.extra["repetitionSalt"], json!(salt + shown as u64));
            assert_eq!(state.rng, rng);
        }
    }

    #[test]
    fn white_box_ai_guard_precedes_deck_padding_but_threat_probe_does_not_skip() {
        let mut state = empty_play();
        state.deck_slots.white.clear();
        state.deck_slots.black.clear();
        state.ai_simulation_depth = 1;
        let before = state.clone();
        assert_eq!(
            apply_pending_white_boxes_for_play_start(&mut state, Color::White, "OPENING").unwrap(),
            0
        );
        assert_eq!(state, before);
        state.ai_simulation_depth = 0;
        state.threat_probe_depth = 1;
        let rng = state.rng.clone();
        assert_eq!(
            apply_pending_white_boxes_for_play_start(&mut state, Color::White, "OPENING").unwrap(),
            0
        );
        assert_eq!(state.deck_slots.white.len(), 3);
        assert_eq!(state.deck_slots.black.len(), 3);
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn draft_activation_pads_requested_deck_after_ai_guard_only() {
        let mut state = empty_play();
        state.deck_slots.white.clear();
        state.ai_simulation_depth = 1;
        let before = state.clone();
        activate_pending_draft_cards(&mut state, Color::White).unwrap();
        assert_eq!(state, before);

        state.ai_simulation_depth = 0;
        state.threat_probe_depth = 1;
        let other_deck = state.deck_slots.black.clone();
        let rng = state.rng.clone();
        activate_pending_draft_cards(&mut state, Color::White).unwrap();
        assert_eq!(state.deck_slots.white.len(), 3);
        assert!(state.deck_slots.white.iter().all(|card| card.vacant));
        assert_eq!(state.deck_slots.black, other_deck);
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn pending_rule_ticket_counts_every_half_turn_without_early_activation() {
        let mut state = empty_play();
        state.extra.insert(
            "pendingRuleTickets".into(),
            json!([{"color":"black","remainingHalfTurns":2,"ruleId":"acceleration"}]),
        );
        resolve_incoming_automatic_effects(&mut state, Color::White).unwrap();
        assert_eq!(
            state.extra["pendingRuleTickets"][0]["remainingHalfTurns"],
            json!(1)
        );
        state.turn = Color::Black;
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        expected_rng.sample().unwrap();
        resolve_incoming_automatic_effects(&mut state, Color::Black).unwrap();
        assert_eq!(state.extra["pendingRuleTickets"], json!([]));
        assert_eq!(
            state.extra["additionalRuleCards"][0]["id"],
            json!("acceleration")
        );
        assert_eq!(
            state.extra["additionalRuleCards"][0]["instanceId"],
            json!("rule-ticket-applied-acceleration")
        );
        assert_eq!(state.extra["accelerationPendingFor"], json!("black"));
        assert_eq!(state.extra["accelerationStartsAfterBlackTurns"], json!(3));
        assert_eq!(state.rng, expected_rng);
    }

    #[test]
    fn rule_ticket_conflict_and_declined_effect_consume_due_without_draws() {
        for (rule, conflict) in [("macho-chess", true), ("revelation", false)] {
            let mut state = empty_play();
            state.turns_taken.white = 1;
            if conflict {
                let definition =
                    crate::card_registry::definition_for(RULES_VERSION_V7, "reverse-pawns")
                        .unwrap();
                state.deck_slots.white[0] =
                    serde_json::from_value(definition.source_definition.clone()).unwrap();
            } else {
                state.extra.insert("deathmatchEnabled".into(), json!(false));
            }
            state.extra.insert(
                "pendingRuleTickets".into(),
                json!([
                    {"color":"white","startTurnCount":0,"ruleId":rule},
                ]),
            );
            let board = state.board.clone();
            let rng = state.rng.clone();
            tick_rule_tickets(&mut state, Color::White).unwrap();
            assert_eq!(state.extra["pendingRuleTickets"], json!([]));
            assert_eq!(state.board, board);
            assert_eq!(state.rng, rng);
            assert!(
                state
                    .extra
                    .get("additionalRuleCards")
                    .is_none_or(|value| value.as_array().is_some_and(Vec::is_empty))
            );
            assert!(
                state.extra["logs"][0]
                    .as_str()
                    .unwrap()
                    .starts_with("규칙 티켓 실패: ")
            );
        }
    }

    #[test]
    fn conscription_spawns_only_open_center_cells_and_palace_collapses() {
        let mut state = empty_play();
        state.set_flag("conscription", Color::White, true);
        state.board[6][3] = Some(Piece::new("rook", Color::White, "blocker"));
        state.board[7][4] = Some(Piece::new("king", Color::White, "royal"));
        state.extra.insert(
            "palaces".into(),
            json!([{"color":"white","cells":[{"row":7,"col":3}]}]),
        );
        settle_incoming_economy(&mut state, Color::White).unwrap();
        assert_eq!(state.extra["conscriptionUsed"]["white"], json!(true));
        assert_eq!(state.board[6][2].as_ref().unwrap().kind, "pawn");
        assert_eq!(state.board[6][3].as_ref().unwrap().kind, "rook");
        assert_eq!(
            state.board[6][5].as_ref().unwrap().extra["origin"],
            json!("f2")
        );
        assert_eq!(state.extra["palaces"], json!([]));
        assert_eq!(
            state.extra["logs"][0],
            json!("백 킹이 궁성 밖으로 탈출해 궁성이 무너졌습니다.")
        );
        assert_eq!(
            state.extra["logs"][1],
            json!("백 징집: 3개의 폰이 소환되었습니다.")
        );
    }

    #[test]
    fn twin_swap_matches_frozen_client_full_state_and_rng() {
        // Direct resolveTwinSwaps probe against main-OahWs0tU.js SHA-256
        // e5ed84fc...302c45c; normal, draftDelete, seed 19. The source
        // snapshot is kept outside Git under Accelerate/reports.
        // The retained source before/after Positions are rebased only to
        // the adopted faithful execution catalog; state/RNG/history agree.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        assert_eq!(state.mode, "play");
        state.board = vec![vec![None; 8]; 8];
        state.turn = Color::White;
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert("chainBonds".into(), json!([]));
        let mut first = Piece::new("pawn", Color::White, "twin-a");
        first.extra.insert("twinBondId".into(), json!("bond-1"));
        first.extra.insert("twinPartnerId".into(), json!("twin-b"));
        first.extra.insert("twinSwapPending".into(), json!(1));
        let mut second = Piece::new("rook", Color::White, "twin-b");
        second.extra.insert("twinBondId".into(), json!("bond-1"));
        second.extra.insert("twinPartnerId".into(), json!("twin-a"));
        state.board[4][4] = Some(first);
        state.board[3][3] = Some(second);
        assert_eq!(
            crate::v7_host::V7HostPosition::from_state(state.clone())
                .unwrap()
                .position_id(),
            "efcb945c77ab1fbf665b2540be28af099f40c2a87e817604464f1a3ae300bfd5"
        );
        resolve_twin_swaps(&mut state).unwrap();
        assert_eq!(state.rng.cursor, 33);
        assert_eq!(
            crate::v7_host::V7HostPosition::from_state(state)
                .unwrap()
                .position_id(),
            "5dcb376886bd90f7fc5375e00893e1262681e16897a54cd92cdd3662c611089e"
        );
    }

    fn receipt_differences(actual: &Value, expected: &Value, path: &str, output: &mut Vec<String>) {
        if output.len() >= 12
            || serde_jcs::to_vec(actual).unwrap() == serde_jcs::to_vec(expected).unwrap()
        {
            return;
        }
        match (actual, expected) {
            (Value::Object(actual), Value::Object(expected)) => {
                let keys = actual
                    .keys()
                    .chain(expected.keys())
                    .collect::<BTreeSet<_>>();
                for key in keys {
                    let next = format!("{path}.{key}");
                    match (actual.get(key), expected.get(key)) {
                        (Some(actual), Some(expected)) => {
                            receipt_differences(actual, expected, &next, output)
                        }
                        (Some(_), None) => output.push(format!("{next}: unexpected field")),
                        (None, Some(_)) => output.push(format!("{next}: missing field")),
                        _ => {}
                    }
                    if output.len() >= 12 {
                        break;
                    }
                }
            }
            (Value::Array(actual), Value::Array(expected)) => {
                if actual.len() != expected.len() {
                    output.push(format!(
                        "{path}.length: {} != {}",
                        actual.len(),
                        expected.len()
                    ));
                }
                for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                    receipt_differences(actual, expected, &format!("{path}[{index}]"), output);
                    if output.len() >= 12 {
                        break;
                    }
                }
            }
            _ => output.push(format!("{path}: {actual} != {expected}")),
        }
    }

    /// Receipts are generated outside Git by the main integration agent from
    /// the pinned source. This checks complete raw callback Positions,
    /// including RNG and history, before gameover microtask settlement.
    #[test]
    #[ignore = "requires root-generated ACCELERATE_V7_INCOMING_LIFECYCLE_CASES receipt"]
    fn frozen_incoming_lifecycle_callbacks_match_full_positions() {
        let path = std::env::var_os("ACCELERATE_V7_INCOMING_LIFECYCLE_CASES")
            .expect("main agent must provide the source-pinned incoming lifecycle receipt");
        let source =
            std::fs::read_to_string(path).expect("incoming lifecycle receipt must be readable");
        let lines = source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>();
        assert_eq!(
            lines.len(),
            20,
            "all 20 frozen incoming lifecycle cases are required"
        );
        let mut failures = Vec::new();
        for line in lines {
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(receipt["schemaVersion"], json!(1));
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            assert_eq!(receipt["rulesVersion"], RULES_VERSION_V7);
            let name = receipt["name"]
                .as_str()
                .expect("receipt must identify the case");
            let host =
                match crate::v7_host::V7HostPosition::from_envelope(receipt["before"].clone()) {
                    Ok(host) => host,
                    Err(error) => {
                        failures.push(format!("{name} admission: {error}"));
                        continue;
                    }
                };
            let mut working = host.state().clone();
            working.threat_probe_depth = receipt["hostContext"]["threatProbeDepth"]
                .as_u64()
                .unwrap_or(0) as u32;
            let applied = match name {
                "panic-live-knight" | "panic-frozen-empty" => {
                    settle_pending_panic(&mut working, Color::White)
                }
                "icbm-live-launch" => resolve_pending_icbm(&mut working, Color::White).map(|_| ()),
                "siren-royal-conversion"
                | "siren-live-reference-chain"
                | "siren-explicit-other-turn" => {
                    tick_siren_exposure_for_turn_start(&mut working, Color::White).map(|_| ())
                }
                "vanishing-live-royal" | "vanishing-recurrence" | "vanishing-threat-simulation" => {
                    resolve_vanishing_for_turn_start(&mut working, Color::White)
                }
                "baby-frozen-preserved-highlight"
                | "baby-blocked-empty-pool"
                | "baby-trickster-reroll" => auto_move_baby_bears(&mut working, Color::White),
                "brutus-friendly-royal" => {
                    crate::v7_incoming_reactions::resolve_local_brutus(&mut working, Color::White)
                        .map(|_| ())
                }
                "undead-home-rank-blocked" | "undead-trickster-revival" => {
                    crate::v7_piece_lifecycle::resolve_undead_resurrections_after_move(&mut working)
                }
                "trolley-cold-display-old-epoch" => {
                    settle_pending_trolley(&mut working, Color::White)
                }
                "rule-ticket-acceleration-black" => tick_rule_tickets(&mut working, Color::Black),
                "rule-ticket-duplicate-ledger"
                | "rule-ticket-conflict"
                | "rule-ticket-declined-revelation" => {
                    tick_rule_tickets(&mut working, Color::White)
                }
                other => panic!("unexpected incoming lifecycle receipt case {other}"),
            };
            if let Err(error) = applied {
                failures.push(format!("{name} callback: {error}"));
                continue;
            }
            // A terminal callback may queue source gameover microtasks. The
            // receipt intentionally measures before those settle; exporting
            // its raw source checkpoint must not pretend it is a host commit.
            let settled = match crate::v7_host::V7HostPosition::from_state(working) {
                Ok(settled) => settled,
                Err(error) => {
                    failures.push(format!("{name} checkpoint export: {error}"));
                    continue;
                }
            };
            let actual = settled.export_envelope().unwrap();
            let expected = &receipt["after"];
            let mut differences = Vec::new();
            receipt_differences(&actual, expected, "position", &mut differences);
            if !differences.is_empty() {
                failures.push(format!("{name}: {differences:?}"));
            }
        }
        assert!(
            failures.is_empty(),
            "source incoming lifecycle differences:\n{}",
            failures.join("\n")
        );
    }
}
