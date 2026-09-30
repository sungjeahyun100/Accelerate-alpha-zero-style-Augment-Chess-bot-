//! Frozen v7 piece and status lifetime callbacks at completed-turn boundaries.
//!
//! Source: `main-OahWs0tU.js` SHA-256
//! `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`.
//! The callbacks are deliberately split at the source's intervening rule
//! callbacks. The transition owner must call each function at its named stage.
//! Removal, promotion, and campaign reactions delegate to their source-owned
//! callbacks. Errors propagate before the enclosing cloned phase commits.

use crate::v7_turn_flow::V7FlowControl;
use crate::{Color, EngineError, GameState, Piece, PieceColor, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// main93646–93713의 outgoing 정산은 명시적 movingColor를 기준으로 한다.
/// Spy 승급이 기물 색을 바꾸어도 state.turn은 switch 단계까지 그대로이므로
/// 여기서 turn==actor를 요구하지 않는다. source는 gameover만 중단한다.
/// 공개 행동과 실제 incoming callback의 mode/actor 게이트는 각 소유자가 검사한다.
fn outgoing_boundary(state: &GameState) -> Result<Option<V7FlowControl>> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 piece lifecycle on rules version {}",
            state.ruleset_id
        )));
    }
    if state.mode == "gameover" {
        return Ok(Some(V7FlowControl::Terminal));
    }
    Ok(None)
}

fn truth(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|number| number != 0.0),
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Array(_) | Value::Object(_) | Value::Bool(true)) => true,
    }
}

/// Source 1229-1242, `septemberQueueRecurrence`. Environmental removals call
/// this directly: they do not schedule an undead resurrection. Pawn capture
/// credit belongs to the caller's preceding source callback.
pub(crate) fn queue_recurrence(
    state: &mut GameState,
    captured: &Piece,
    capturer: Color,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 septemberQueueRecurrence on rules version {}",
            state.ruleset_id
        )));
    }
    if captured.color.owner().is_none() || !truth(captured.extra.get("recurrence")) {
        return Ok(false);
    }
    let mut next = state.clone();
    let queue = next
        .extra
        .entry("pendingRecurrences")
        .or_insert_with(|| json!([]));
    if !queue.is_array() {
        *queue = json!([]);
    }
    let entries = queue
        .as_array_mut()
        .ok_or_else(|| EngineError::InvalidState("v7 pendingRecurrences must be array".into()))?;
    if !entries.iter().any(|entry| {
        entry
            .get("piece")
            .and_then(|piece| piece.get("id"))
            .and_then(Value::as_str)
            == Some(captured.id.as_str())
    }) {
        let mut saved = captured.clone();
        saved.extra.shift_remove("recurrence");
        saved.extra.shift_remove("outpostProtected");
        saved
            .source_order
            .retain(|key| key != "recurrence" && key != "outpostProtected");
        saved.moved = true;
        if !saved.source_order.iter().any(|key| key == "moved") {
            saved.source_order.push("moved".into());
        }
        if crate::card_effects::js_number(saved.extra.get("maxHp"), 0).is_some_and(|hp| hp > 0.0) {
            saved
                .extra
                .insert("hp".into(), saved.extra["maxHp"].clone());
        }
        entries.push(json!({"piece":saved,"capturedBy":capturer}));
    }
    if let Some(resurrections) = next
        .extra
        .get_mut("undeadResurrections")
        .and_then(Value::as_array_mut)
    {
        resurrections.retain(|entry| {
            entry
                .get("piece")
                .and_then(|piece| piece.get("id"))
                .and_then(Value::as_str)
                != Some(captured.id.as_str())
        });
    }
    *state = next;
    Ok(true)
}

#[derive(Clone, Debug)]
pub(crate) struct RecurrenceRevival {
    pub(crate) piece: Piece,
    pub(crate) origin: Square,
    pub(crate) cells: Vec<Square>,
}

fn recurrence_black_hole_cell(state: &GameState, at: Square) -> bool {
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

fn recurrence_candidates(state: &GameState, piece: &Piece) -> Result<Vec<Vec<Square>>> {
    let Some(owner) = piece.color.owner() else {
        return Ok(Vec::new());
    };
    // The frozen port admits the source's 8x8 profile. A large piece uses one
    // 2x2 anchor candidate and shares its identity across all four cells.
    if state.board.len() != 8 || state.board.iter().any(|line| line.len() != 8) {
        return Err(EngineError::UnsupportedFeature(
            "v7 recurrence requires frozen 8x8 board profile".into(),
        ));
    }
    let size = if piece.is_large() { 2_u8 } else { 1_u8 };
    let mut rows = (0_u8..=8 - size).collect::<Vec<_>>();
    if owner == Color::White {
        rows.reverse();
    }
    for row in rows {
        let mut candidates = Vec::new();
        for col in 0_u8..=8 - size {
            let mut cells = Vec::with_capacity(usize::from(size * size));
            let mut open = true;
            for dr in 0..size {
                for dc in 0..size {
                    let at = Square {
                        row: row + dr,
                        col: col + dc,
                    };
                    cells.push(at);
                    // Source supplies d4, but deliberately omits the separate
                    // synchronization option to expansionDestinationAllowed.
                    if !crate::movement::open_placement(state, at, None)?
                        || crate::movement::collapsed(state, at)
                        || recurrence_black_hole_cell(state, at)
                        || !crate::movement::d4_destination_allowed(state, piece.color, &[at])
                    {
                        open = false;
                        break;
                    }
                }
                if !open {
                    break;
                }
            }
            if open {
                candidates.push(cells);
            }
        }
        if !candidates.is_empty() {
            return Ok(candidates);
        }
    }
    Ok(Vec::new())
}

/// Source 1243-1277 and 98827-98837. Each queued identity revives on the first
/// available home-side rank. A blocked entry stays queued and consumes no
/// random draw; an identity already present is discarded without a draw.
/// The returned identities let environmental defeat callers omit revived
/// victims before adjudicating royal loss, democracy, and Reaper souls.
pub(crate) fn resolve_recurrences(state: &mut GameState) -> Result<Vec<RecurrenceRevival>> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 resolveRecurrences on rules version {}",
            state.ruleset_id
        )));
    }
    let Some(queue) = state
        .extra
        .get("pendingRecurrences")
        .filter(|value| truth(Some(value)))
    else {
        return Ok(Vec::new());
    };
    let entries = queue
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("v7 pendingRecurrences must be array".into()))?;
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let mut next = state.clone();
    let mut remaining = Vec::new();
    let mut revived = Vec::new();
    for entry in entries {
        let mut piece: Piece =
            serde_json::from_value(entry.get("piece").cloned().ok_or_else(|| {
                EngineError::InvalidState("v7 pendingRecurrences.piece missing".into())
            })?)
            .map_err(EngineError::serialization)?;
        if next
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|existing| existing.id == piece.id)
        {
            continue;
        }
        let candidates = recurrence_candidates(&next, &piece)?;
        if candidates.is_empty() {
            remaining.push(entry.clone());
            continue;
        }
        let index = (next.rng.sample()? * candidates.len() as f64).floor() as usize;
        next.rng.record_last_probability(
            1.0 / candidates.len() as f64,
            "source Recurrence placement",
        )?;
        let cells = candidates[index].clone();
        let origin = cells[0];
        if piece.is_large() {
            piece.extra.insert("anchorRow".into(), json!(origin.row));
            piece.extra.insert("anchorCol".into(), json!(origin.col));
        }
        for cell in &cells {
            next.board[cell.row as usize][cell.col as usize] = Some(piece.clone());
        }
        let captured_by: Color =
            serde_json::from_value(entry.get("capturedBy").cloned().ok_or_else(|| {
                EngineError::InvalidState("v7 pendingRecurrences.capturedBy missing".into())
            })?)
            .map_err(EngineError::serialization)?;
        next.captures
            .get_mut(captured_by)
            .retain(|captured| captured.id != piece.id);
        crate::card_effects::mark_animation(&mut next, &piece)?;
        crate::replay::add_piece_action_log(
            &mut next,
            &piece,
            Some(origin),
            None,
            format!("회귀: {}에서 다시 소환되었습니다.", square_name(origin)),
        )?;
        revived.push(RecurrenceRevival {
            piece,
            origin,
            cells,
        });
    }
    next.extra
        .insert("pendingRecurrences".into(), Value::Array(remaining));
    *state = next;
    Ok(revived)
}

/// Source 65806-65827. Called after a captured piece has left the board, also
/// for effect removals such as Exile. Recurrence takes precedence over undead;
/// neither branch invents a resurrection action on the current turn.
pub(crate) fn schedule_undead_resurrection(
    state: &mut GameState,
    captured: &Piece,
    capturer: Color,
    after_turn_boundary: bool,
) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 scheduleUndeadResurrection on rules version {}",
            state.ruleset_id
        )));
    }
    let Some(owner) = captured.color.owner() else {
        return Ok(());
    };
    let mut next = state.clone();
    if captured.kind == "pawn" {
        crate::v7_queued_effects::note_resolve_pawn_capture(&mut next, captured)?;
    }
    if queue_recurrence(&mut next, captured, capturer)? {
        *state = next;
        return Ok(());
    }
    if captured.ability_kind() != "undead" {
        *state = next;
        return Ok(());
    }
    let existing = next
        .extra
        .entry("undeadResurrections")
        .or_insert_with(|| json!([]));
    if !existing.is_array() {
        *existing = json!([]);
    }
    if !captured.id.is_empty()
        && existing.as_array().is_some_and(|entries| {
            entries.iter().any(|entry| {
                entry
                    .get("piece")
                    .and_then(|piece| piece.get("id"))
                    .and_then(Value::as_str)
                    == Some(captured.id.as_str())
            })
        })
    {
        *state = next;
        return Ok(());
    }
    let delay = if crate::v7_queued_effects::uses_september26_rebalance(&next)? {
        4_u64
    } else {
        6_u64
    };
    let timestamp = crate::draft::frozen_timestamp_for_ruleset(&next.ruleset_id)?;
    let id = if captured.id.is_empty() {
        timestamp.to_string()
    } else {
        captured.id.clone()
    };
    let suffix: String =
        crate::draft::random_suffix(next.rng.sample_opaque("source pending undead identity")?)?
            .chars()
            .take(6)
            .collect();
    let due_move_count = u64::from(next.move_count)
        .checked_add(delay + u64::from(!after_turn_boundary))
        .ok_or_else(|| EngineError::InvalidState("v7 undead dueMoveCount overflow".into()))?;
    let remaining_half_turns = delay + u64::from(!after_turn_boundary);
    let queue = next
        .extra
        .get_mut("undeadResurrections")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState("v7 undeadResurrections must be array".into()))?;
    queue.push(json!({
        "id":format!("undead-{id}-{suffix}"),
        "color":owner,
        "capturedBy":capturer,
        "piece":captured,
        "dueMoveCount":due_move_count,
        "remainingHalfTurns":remaining_half_turns,
    }));
    *state = next;
    Ok(())
}

const TRICKSTER_UNDEAD_STATUS_FIELDS: &[&str] = &[
    "poisonedPawn",
    "poisonStunTurns",
    "poisonStunColor",
    "evasion",
    "frozen",
    "frozenByCard",
    "iceSheet",
    "crownBearer",
    "callingCard",
    "loyalist",
    "parry",
    "emptyLunchbox",
    "shielded",
    "protected",
    "queensGambitProtection",
    "queensGambitPreviousProtected",
    "basicTraining",
    "potionBasicTraining",
    "coronationProtection",
    "ghost",
    "chameleon",
    "undergroundBunker",
    "witchTrial",
    "disarmed",
    "lastResistance",
    "staked",
    "severed",
    "inertia",
    "promotionRushUntil",
    "sacrificeProtection",
    "frenzy",
    "cardNoCaptureUntil",
    "spyOwner",
    "hiddenFrom",
    "submerged",
    "trojanHorse",
    "quantum",
    "quantumFirstObservationFails",
];

/// Source 93212-93250 and advanceScheduledHalfTurn(3660). The endMove owner
/// calls this after moveCount++, pending lobsters, and previous Trickster
/// ability cleanup. A failed home-rank placement still consumes one draw and
/// requeues the advanced entry for the next completed half-turn.
pub(crate) fn resolve_undead_resurrections_after_move(state: &mut GameState) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 resolveUndeadResurrectionsAfterMove on rules version {}",
            state.ruleset_id,
        )));
    }
    let Some(entries) = state
        .extra
        .get("undeadResurrections")
        .and_then(Value::as_array)
    else {
        return Ok(());
    };
    if entries.is_empty() {
        return Ok(());
    }
    let mut next = state.clone();
    let mut remaining = Vec::new();
    for entry in entries {
        let mut advanced = entry.clone();
        let boundary_remaining = crate::card_effects::js_number(entry.get("remainingHalfTurns"), 0)
            .filter(|value| {
                value.fract() == 0.0 && (0.0..=9_007_199_254_740_991.0).contains(value)
            });
        if let Some(counter) = boundary_remaining {
            let counter = (counter - 1.0).max(0.0);
            let fields = advanced.as_object_mut().ok_or_else(|| {
                EngineError::InvalidState("v7 undeadResurrections entries must be objects".into())
            })?;
            fields.insert("remainingHalfTurns".into(), json!(counter as u64));
            if counter > 0.0 {
                remaining.push(advanced);
                continue;
            }
        } else {
            let due = crate::card_effects::js_number(entry.get("dueMoveCount"), 0)
                .filter(|value| *value != 0.0)
                .unwrap_or(f64::INFINITY);
            if f64::from(next.move_count) < due {
                remaining.push(advanced);
                continue;
            }
        }
        let owner: Color =
            serde_json::from_value(entry.get("color").cloned().ok_or_else(|| {
                EngineError::InvalidState("v7 undeadResurrections.color missing".into())
            })?)
            .map_err(EngineError::serialization)?;
        let row = if owner == Color::White { 7 } else { 0 };
        let mut candidates = Vec::new();
        for col in 0..8 {
            let at = Square { row, col };
            if crate::movement::open_placement(&next, at, Some(owner))?
                && !crate::movement::collapsed(&next, at)
                && !recurrence_black_hole_cell(&next, at)
            {
                candidates.push(at);
            }
        }
        let draw = if candidates.is_empty() {
            next.rng
                .sample_invariant("source empty Undead Resurrection placement")?
        } else {
            let draw = next.rng.sample()?;
            next.rng.record_last_probability(
                1.0 / candidates.len() as f64,
                "source Undead Resurrection placement",
            )?;
            draw
        };
        if candidates.is_empty() {
            let fields = advanced.as_object_mut().ok_or_else(|| {
                EngineError::InvalidState("v7 undeadResurrections entries must be objects".into())
            })?;
            fields.insert("dueMoveCount".into(), json!(u64::from(next.move_count) + 1));
            fields.insert("remainingHalfTurns".into(), json!(1));
            remaining.push(advanced);
            continue;
        }
        let destination = candidates[(draw * candidates.len() as f64).floor() as usize];
        let mut revived: Piece =
            serde_json::from_value(entry.get("piece").cloned().ok_or_else(|| {
                EngineError::InvalidState("v7 undeadResurrections.piece missing".into())
            })?)
            .map_err(EngineError::serialization)?;
        if revived.kind == "trickster" {
            for field in TRICKSTER_UNDEAD_STATUS_FIELDS {
                revived.extra.shift_remove(*field);
            }
            revived
                .source_order
                .retain(|field| !TRICKSTER_UNDEAD_STATUS_FIELDS.contains(&field.as_str()));
            revived
                .extra
                .insert("tricksterMoveType".into(), json!("undead"));
        } else {
            revived.kind = "undead".into();
        }
        revived.color = owner.into();
        revived.moved = true;
        next.board[destination.row as usize][destination.col as usize] = Some(revived.clone());
        crate::card_effects::mark_animation(&mut next, &revived)?;
        let captured_by: Color =
            serde_json::from_value(entry.get("capturedBy").cloned().ok_or_else(|| {
                EngineError::InvalidState("v7 undeadResurrections.capturedBy missing".into())
            })?)
            .map_err(EngineError::serialization)?;
        next.captures
            .get_mut(captured_by)
            .retain(|captured| captured.id != revived.id);
        crate::replay::add_log(
            &mut next,
            format!("언데드: {}에서 부활했습니다.", square_name(destination)),
        )?;
    }
    next.extra
        .insert("undeadResurrections".into(), Value::Array(remaining));
    *state = next;
    Ok(())
}

/// Source numbers are JavaScript Numbers, while admitted snapshots retain
/// only safe integral lifecycle counters. Refuse malformed/fractional counters
/// rather than treating an unknown countdown as zero.
fn counter(value: Option<&Value>, field: &str) -> Result<i64> {
    let Some(value) = value else { return Ok(0) };
    let number = match value {
        Value::Null => return Ok(0),
        Value::Bool(value) => return Ok(i64::from(*value)),
        Value::Number(number) => number.as_f64(),
        Value::String(value) => {
            if value.trim().is_empty() {
                return Ok(0);
            }
            value.trim().parse::<f64>().ok()
        }
        Value::Array(_) | Value::Object(_) => None,
    }
    .ok_or_else(|| EngineError::InvalidState(format!("v7 {field} must be numeric")))?;
    if !number.is_finite() || number.fract() != 0.0 || number.abs() > 9_007_199_254_740_991.0 {
        return Err(EngineError::InvalidState(format!(
            "v7 {field} must be a JavaScript-safe integer"
        )));
    }
    Ok(number as i64)
}

fn color_slot_mut<'a>(
    state: &'a mut GameState,
    key: &str,
    color: Color,
) -> Result<Option<&'a mut Value>> {
    let Some(value) = state.extra.get_mut(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let sides = value
        .as_object_mut()
        .ok_or_else(|| EngineError::InvalidState(format!("v7 {key} must be a color map")))?;
    Ok(sides.get_mut(color.as_str()))
}

fn square_name(square: Square) -> String {
    format!("{}{}", char::from(b'a' + square.col), 8 - square.row)
}

fn is_king_augment_recipient(
    state: &GameState,
    piece: &Piece,
    actor: Color,
    september18: bool,
) -> bool {
    if piece.color != actor {
        return false;
    }
    piece.flag("crownRoyal")
        || piece.flag("editorRoyal")
        || matches!(
            piece.kind.as_str(),
            "king" | "royalKnight" | "shotgunKing" | "darkWizard"
        )
        || (piece.kind == "merchant" && september18)
        || (piece.flag("regencyHeir")
            && state.flag("kingDead", actor)
            && state.flag("regency", actor))
}

pub(crate) fn king_augment_recipient(state: &GameState, piece: &Piece) -> Result<bool> {
    let Some(owner) = piece.color.owner() else {
        return Ok(false);
    };
    Ok(is_king_augment_recipient(
        state,
        piece,
        owner,
        uses_september18_balance(state)?,
    ))
}

fn first_royal(state: &GameState, actor: Color) -> Result<Option<Square>> {
    let september18 = uses_september18_balance(state)?;
    Ok(state.board.iter().enumerate().find_map(|(row, line)| {
        line.iter().enumerate().find_map(|(col, occupant)| {
            occupant
                .as_ref()
                .filter(|piece| is_king_augment_recipient(state, piece, actor, september18))
                .map(|_| Square {
                    row: row as u8,
                    col: col as u8,
                })
        })
    }))
}

fn adjacent(first: Square, second: Square) -> bool {
    let dr = first.row.abs_diff(second.row);
    let dc = first.col.abs_diff(second.col);
    dr.max(dc) == 1
}

fn has_democracy_pawn(state: &GameState, color: Color) -> bool {
    state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|piece| piece.color == color && piece.kind == "pawn")
        || state
            .extra
            .get("pendingRecurrences")
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry["piece"]["color"] == color.as_str() && entry["piece"]["type"] == "pawn"
                })
            })
}

fn settle_democracy_after_lunchboxes(state: &mut GameState) -> Result<V7FlowControl> {
    let defeated: Vec<_> = [Color::White, Color::Black]
        .into_iter()
        .filter(|&color| state.flag("democracy", color) && !has_democracy_pawn(state, color))
        .collect();
    const CAUSE: &str = "마지막 폰이 빈 찬합으로 사라졌습니다.";
    if defeated.len() > 1 {
        crate::flow::end_game(state, None, &format!("양쪽의 {CAUSE}"))?;
        return Ok(V7FlowControl::Terminal);
    }
    if let Some(&loser) = defeated.first()
        && crate::flow::check_democracy_defeat(state, loser, loser.opponent(), CAUSE)?
    {
        return Ok(V7FlowControl::Terminal);
    }
    Ok(V7FlowControl::Continue)
}

/// Immediately after `turnsTaken[actor]++` and before the platform/crown
/// callbacks (source 93579-93609, 93664-93668). This includes the democracy
/// check even when no lunchbox exists. The royal position is recomputed for
/// each queued victim, because an earlier removal can change succession.
pub(crate) fn after_count(state: &mut GameState, actor: Color) -> Result<V7FlowControl> {
    if let Some(flow) = outgoing_boundary(state)? {
        return Ok(flow);
    }
    let mut seen = BTreeSet::new();
    let mut due = Vec::new();
    for (row, line) in state.board.iter().enumerate() {
        for (col, occupant) in line.iter().enumerate() {
            let Some(piece) = occupant else { continue };
            if piece.color != actor || !truth(piece.extra.get("emptyLunchbox")) {
                continue;
            }
            let square = Square {
                row: row as u8,
                col: col as u8,
            };
            let identity = if piece.id.is_empty() {
                format!("{row}:{col}")
            } else {
                piece.id.clone()
            };
            if !seen.insert(identity) {
                continue;
            }
            due.push((piece.clone(), square));
        }
    }
    if due.is_empty()
        && ![Color::White, Color::Black]
            .into_iter()
            .any(|color| state.flag("democracy", color))
    {
        return Ok(V7FlowControl::Continue);
    }
    let mut next = state.clone();
    for (mut piece, square) in due {
        let royal = first_royal(&next, actor)?;
        if royal.is_some_and(|royal| adjacent(square, royal)) {
            piece.extra.shift_remove("emptyLunchbox");
            for occupant in next.board.iter_mut().flatten().flatten() {
                if !piece.id.is_empty() && occupant.id == piece.id {
                    occupant.extra.shift_remove("emptyLunchbox");
                }
            }
            if piece.id.is_empty()
                && let Some(occupant) = next.at_mut(square)
            {
                occupant.extra.shift_remove("emptyLunchbox");
            }
            crate::replay::add_piece_action_log(
                &mut next,
                &piece,
                Some(square),
                None,
                format!(
                    "빈 찬합: {}의 기물이 킹 곁에 도착해 살아남았습니다.",
                    square_name(square)
                ),
            )?;
            continue;
        }
        let lunchbox = piece
            .extra
            .get("emptyLunchbox")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                EngineError::InvalidState("v7 emptyLunchbox must be an object".into())
            })?;
        let deadline = counter(lunchbox.get("deadlineTurn"), "emptyLunchbox.deadlineTurn")?;
        if i64::from(*next.turns_taken.get(actor)) < deadline {
            continue;
        }
        let by = match lunchbox.get("by").and_then(Value::as_str) {
            Some("white") => Color::White,
            Some("black") => Color::Black,
            _ => actor.opponent(),
        };
        piece.extra.shift_remove("emptyLunchbox");
        crate::card_effects::mark_vanish_animation(&mut next, &piece, square)?;
        for occupant in next.board.iter_mut().flatten().flatten() {
            if !piece.id.is_empty() && occupant.id == piece.id {
                occupant.extra.shift_remove("emptyLunchbox");
            }
        }
        if piece.id.is_empty()
            && let Some(occupant) = next.at_mut(square)
        {
            occupant.extra.shift_remove("emptyLunchbox");
        }
        crate::transition::force_remove_piece_at_with_options(
            &mut next,
            square,
            by,
            &crate::transition::ForceRemovalOptions {
                suppress_calling_card: true,
                threat_source: Some(&json!({"label":"빈 찬합"})),
                ..Default::default()
            },
        )?;
        crate::replay::add_log(
            &mut next,
            format!(
                "빈 찬합: {}의 기물이 킹 곁에 가지 못해 자결했습니다.",
                square_name(square)
            ),
        )?;
    }
    let flow = if next.mode == "gameover" {
        V7FlowControl::Terminal
    } else {
        settle_democracy_after_lunchboxes(&mut next)?
    };
    *state = next;
    Ok(flow)
}

/// Mutate the first row-major instance of each source piece identity, then
/// synchronize any Rust board aliases for large pieces. The source's aliases
/// are references to one object and must not tick a countdown twice.
fn each_piece_once(
    state: &mut GameState,
    mut apply: impl FnMut(&mut Piece) -> Result<()>,
) -> Result<()> {
    let mut updated = BTreeMap::new();
    for piece in state.board.iter_mut().flatten().flatten() {
        if updated.contains_key(&piece.id) {
            continue;
        }
        apply(piece)?;
        updated.insert(piece.id.clone(), piece.clone());
    }
    for piece in state.board.iter_mut().flatten().flatten() {
        if let Some(canonical) = updated.get(&piece.id) {
            *piece = canonical.clone();
        }
    }
    Ok(())
}

fn winter_frozen_ids(state: &GameState) -> Result<BTreeSet<String>> {
    let Some(value) = state
        .extra
        .get("winterKingdom")
        .and_then(|winter| winter.get("frozenIds"))
    else {
        return Ok(BTreeSet::new());
    };
    let values = value.as_array().ok_or_else(|| {
        EngineError::InvalidState("v7 winterKingdom.frozenIds must be an array".into())
    })?;
    values
        .iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 winterKingdom.frozenIds entries must be strings".into(),
                )
            })
        })
        .collect()
}

fn uses_september18_balance(state: &GameState) -> Result<bool> {
    let source = state
        .extra
        .get("cardState")
        .filter(|value| truth(Some(value)));
    let profile = source
        .and_then(|value| value.get("profile"))
        .or_else(|| state.extra.get("profile"));
    let hash = profile
        .and_then(|value| value.get("catalogHash"))
        .and_then(Value::as_str)
        .filter(|hash| !hash.is_empty());
    let Some(hash) = hash else {
        return Ok(state.extra.get("september18Balance") != Some(&Value::Bool(false)));
    };
    if [
        "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
        "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
        "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs",
        "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc",
        "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g",
        "HVzv4vxzNKDRi-ylshTw9HnLmIlyiQhMScQ4caG_aSI",
    ]
    .contains(&hash)
    {
        Ok(true)
    } else {
        Err(EngineError::UnsupportedFeature(format!(
            "v7 frozenByCard unknown catalogHash {hash}"
        )))
    }
}

fn tick_piece_protections(state: &mut GameState, actor: Color) -> Result<(usize, usize)> {
    let winter_ids = winter_frozen_ids(state)?;
    let september18 = uses_september18_balance(state)?;
    let mut thawed = 0;
    let mut recovered = 0;
    each_piece_once(state, |piece| {
        if piece.color == actor && truth(piece.extra.get("sacrificeProtection")) {
            let protection = piece
                .extra
                .get("sacrificeProtection")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    EngineError::InvalidState("v7 sacrificeProtection must be an object".into())
                })?;
            let remaining =
                counter(protection.get("remaining"), "sacrificeProtection.remaining")?.max(0) - 1;
            let keep = truth(protection.get("previousProtected"))
                || [
                    "coronationProtection",
                    "lastResistance",
                    "queensGambitProtection",
                ]
                .into_iter()
                .any(|key| truth(piece.extra.get(key)));
            if remaining > 0 {
                piece.extra.get_mut("sacrificeProtection").unwrap()["remaining"] = json!(remaining);
            } else {
                piece.extra.shift_remove("sacrificeProtection");
                if !keep {
                    piece.extra.shift_remove("protected");
                }
            }
        }
        if truth(piece.extra.get("frozenByCard")) {
            let frozen = piece
                .extra
                .get("frozenByCard")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    EngineError::InvalidState("v7 frozenByCard must be an object".into())
                })?;
            let count_by =
                frozen
                    .get("countBy")
                    .and_then(Value::as_str)
                    .and_then(|name| match name {
                        "white" => Some(PieceColor::White),
                        "black" => Some(PieceColor::Black),
                        _ => None,
                    });
            let tick_owner = count_by
                .map(|owner| {
                    if september18 {
                        owner.owner().unwrap().opponent().into()
                    } else {
                        owner
                    }
                })
                .unwrap_or(piece.color);
            if tick_owner == actor {
                let remaining =
                    (counter(frozen.get("remaining"), "frozenByCard.remaining")? - 1).max(0);
                if remaining > 0 {
                    piece.extra.get_mut("frozenByCard").unwrap()["remaining"] = json!(remaining);
                } else {
                    piece.extra.shift_remove("frozenByCard");
                    if !winter_ids.contains(&piece.id) {
                        piece.extra.shift_remove("frozen");
                    }
                    thawed += 1;
                }
            }
        }
        let stun = counter(piece.extra.get("poisonStunTurns"), "poisonStunTurns")?.max(0);
        if stun > 0 {
            let count_by = piece
                .extra
                .get("poisonStunColor")
                .and_then(Value::as_str)
                .and_then(|name| match name {
                    "white" => Some(PieceColor::White),
                    "black" => Some(PieceColor::Black),
                    _ => None,
                });
            if count_by == Some(actor.into())
                || (count_by.is_none()
                    && (piece.color == actor || piece.color == PieceColor::Neutral))
            {
                if stun > 1 {
                    piece
                        .extra
                        .insert("poisonStunTurns".into(), json!(stun - 1));
                } else {
                    piece.extra.shift_remove("poisonStunTurns");
                    piece.extra.shift_remove("poisonStunColor");
                    recovered += 1;
                }
            }
        }
        Ok(())
    })?;
    Ok((thawed, recovered))
}

/// Source 103492-103508. A failed sprint is a direct removal with Reaper
/// reactions, prophecy cancellation, and campaign checks. It deliberately
/// omits the ordinary capture and environmental royal-loss callbacks.
pub(crate) fn resolve_last_sprint_failure(
    state: &mut GameState,
    piece: &Piece,
    at: Square,
) -> Result<bool> {
    if piece.color.owner().is_none()
        || !state.at(at).is_some_and(|current| {
            current.id == piece.id && current.color == piece.color && current.kind == piece.kind
        })
    {
        return Ok(false);
    }
    let owner = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let mut next = state.clone();
    if piece.id.is_empty() {
        next.board[at.row as usize][at.col as usize] = None;
    } else {
        crate::transition::clear_piece(&mut next, &piece.id);
    }
    crate::transition::grant_vigilance_protection(&mut next, piece)?;
    let capture_owner = owner.opponent();
    crate::v7_board_hazards::resolve_reaper_nearby_deaths(
        &mut next,
        &[crate::v7_board_hazards::EnvironmentalRemoval {
            piece: piece.clone(),
            square: at,
            capture_owner,
        }],
    )?;
    if let Some(prophecy) = next
        .extra
        .get_mut("prophecy")
        .filter(|value| truth(Some(value)))
    {
        let entries = prophecy.as_object_mut().ok_or_else(|| {
            EngineError::InvalidState("v7 sprint prophecy must be a color map".into())
        })?;
        for color in [Color::White, Color::Black] {
            if truth(entries.get(color.as_str())) {
                entries.insert(color.as_str().into(), Value::Null);
            }
        }
    }
    next.captures.get_mut(capture_owner).push(piece.clone());
    crate::v7_rule_bombs::mark_deathmatch_progress(&mut next)?;
    crate::replay::add_piece_action_log(
        &mut next,
        piece,
        Some(at),
        None,
        format!(
            "마지막 질주: {}의 폰이 프로모션하지 못해 사망했습니다.",
            square_name(at)
        ),
    )?;
    crate::v7_capture_objectives::check_campaign_objectives(&mut next)?;
    *state = next;
    Ok(true)
}

fn resolve_charge_rush_failures(state: &mut GameState, actor: Color) -> Result<()> {
    let mut doomed = Vec::new();
    for (row, line) in state.board.iter().enumerate() {
        for (col, piece) in line.iter().enumerate() {
            let Some(piece) = piece else { continue };
            if piece.color == actor && piece.extra.get("chargeRush") == Some(&json!(true)) {
                doomed.push((
                    piece.clone(),
                    Square {
                        row: row as u8,
                        col: col as u8,
                    },
                ));
            }
        }
    }
    for (mut piece, at) in doomed {
        piece.extra.shift_remove("chargeRush");
        if piece.id.is_empty() {
            if let Some(current) = state.at_mut(at) {
                current.extra.shift_remove("chargeRush");
            }
        } else {
            for current in state.board.iter_mut().flatten().flatten() {
                if current.id == piece.id {
                    current.extra.shift_remove("chargeRush");
                }
            }
        }
        if piece.kind == "pawn" {
            resolve_last_sprint_failure(state, &piece, at)?;
        }
    }
    Ok(())
}

/// After revolving-door cleanup, before deathmatch (source 93692-93698).
/// Source protection counters tick before charge-rush failures, then
/// hallucination ticks only if the failure reactions have not ended the game.
pub(crate) fn after_revolving(state: &mut GameState, actor: Color) -> Result<V7FlowControl> {
    if let Some(flow) = outgoing_boundary(state)? {
        return Ok(flow);
    }
    let has_pieces = state.board.iter().flatten().flatten().any(|piece| {
        truth(piece.extra.get("sacrificeProtection"))
            || truth(piece.extra.get("frozenByCard"))
            || truth(piece.extra.get("poisonStunTurns"))
            || piece.extra.get("chargeRush") == Some(&Value::Bool(true))
    });
    let has_hallucination = state
        .extra
        .get("hallucination")
        .and_then(|value| value.get(actor.as_str()))
        .is_some_and(|value| truth(Some(value)));
    if !has_pieces && !has_hallucination {
        return Ok(V7FlowControl::Continue);
    }
    let mut next = state.clone();
    let (thawed, recovered) = tick_piece_protections(&mut next, actor)?;
    if thawed > 0 {
        crate::replay::add_log(
            &mut next,
            format!("빙결: 기물 {thawed}개의 얼음이 녹았습니다."),
        )?;
    }
    if recovered > 0 {
        crate::replay::add_log(
            &mut next,
            format!(
                "독이 든 폰: {} 기물 {recovered}개가 다시 움직일 수 있습니다.",
                crate::replay::label(actor)
            ),
        )?;
    }
    resolve_charge_rush_failures(&mut next, actor)?;
    if next.mode == "gameover" {
        *state = next;
        return Ok(V7FlowControl::Terminal);
    }
    if let Some(entry) = color_slot_mut(&mut next, "hallucination", actor)?
        && truth(Some(entry))
    {
        let fields = entry.as_object().ok_or_else(|| {
            EngineError::InvalidState("v7 hallucination entry must be an object".into())
        })?;
        if !fields.contains_key("remaining") {
            return Err(EngineError::InvalidState(
                "v7 hallucination.remaining is required".into(),
            ));
        }
        let remaining = fields.get("remaining");
        let remaining = (counter(remaining, "hallucination.remaining")? - 1).max(0);
        if remaining > 0 {
            entry["remaining"] = json!(remaining);
        } else {
            *entry = Value::Null;
            crate::replay::add_log(
                &mut next,
                format!("{}의 환각이 풀렸습니다.", crate::replay::label(actor)),
            )?;
        }
    }
    *state = next;
    Ok(V7FlowControl::Continue)
}

/// After deathmatch adjudication, before pending scarecrows (93698-93700).
pub(crate) fn after_deathmatch(state: &mut GameState, _actor: Color) -> Result<V7FlowControl> {
    if let Some(flow) = outgoing_boundary(state)? {
        return Ok(flow);
    }
    if ![Color::White, Color::Black].into_iter().any(|color| {
        state
            .extra
            .get("prophecy")
            .and_then(|value| value.get(color.as_str()))
            .is_some_and(|entry| truth(Some(entry)))
    }) {
        return Ok(V7FlowControl::Continue);
    }
    let mut next = state.clone();
    let current_move_count = i64::from(next.move_count);
    for color in [Color::White, Color::Black] {
        let Some(entry) = color_slot_mut(&mut next, "prophecy", color)? else {
            continue;
        };
        if !truth(Some(entry)) {
            continue;
        }
        let fields = entry.as_object_mut().ok_or_else(|| {
            EngineError::InvalidState("v7 prophecy entry must be an object".into())
        })?;
        if let Some(skip) = fields.get("skipMoveCount")
            && counter(Some(skip), "prophecy.skipMoveCount")? == current_move_count
        {
            fields.shift_remove("skipMoveCount");
            continue;
        }
        let remaining = (counter(
            fields.get("remainingHalfTurns"),
            "prophecy.remainingHalfTurns",
        )? - 1)
            .max(0);
        if remaining > 0 {
            fields.insert("remainingHalfTurns".into(), json!(remaining));
        } else {
            *entry = Value::Null;
            crate::flow::end_game(
                &mut next,
                Some(color),
                &format!("{} 종전이 실현되었습니다.", crate::replay::label(color)),
            )?;
            *state = next;
            return Ok(V7FlowControl::Terminal);
        }
    }
    *state = next;
    Ok(V7FlowControl::Continue)
}

/// After scarecrow callback, before `fullMove++` (93700-93706).
pub(crate) fn after_scarecrows(state: &mut GameState, actor: Color) -> Result<V7FlowControl> {
    if let Some(flow) = outgoing_boundary(state)? {
        return Ok(flow);
    }
    let has_lock = ["initiative", "diceLocks"].into_iter().any(|key| {
        state
            .extra
            .get(key)
            .and_then(|value| value.get(actor.as_str()))
            .is_some_and(|value| truth(Some(value)))
    });
    let has_cooldown = state.board.iter().flatten().flatten().any(|piece| {
        piece.color == actor
            && piece.kind == "shotgunKing"
            && truth(piece.extra.get("snipeCooldown"))
    });
    let blood_moon = state
        .extra
        .get("campaign")
        .and_then(|value| value.get("setup"))
        .and_then(Value::as_str)
        == Some("bloodMoon");
    if !has_lock && !has_cooldown && !blood_moon {
        return Ok(V7FlowControl::Continue);
    }
    let mut next = state.clone();
    let turns = i64::from(*next.turns_taken.get(actor));
    if let Some(entry) = color_slot_mut(&mut next, "initiative", actor)?
        && truth(Some(entry))
    {
        let fields = entry.as_object().ok_or_else(|| {
            EngineError::InvalidState("v7 initiative entry must be an object".into())
        })?;
        let start = counter(fields.get("startTurn"), "initiative.startTurn")?;
        let limit = counter(fields.get("limit"), "initiative.limit")?;
        if turns - start >= if limit == 0 { 7 } else { limit } {
            *entry = Value::Null;
            crate::replay::add_log(
                &mut next,
                format!(
                    "{}의 선공권 제한이 끝났습니다.",
                    crate::replay::label(actor)
                ),
            )?;
        }
    }
    if let Some(entry) = color_slot_mut(&mut next, "diceLocks", actor)?
        && truth(Some(entry))
    {
        let fields = entry.as_object_mut().ok_or_else(|| {
            EngineError::InvalidState("v7 diceLocks entry must be an object".into())
        })?;
        if !fields.contains_key("remaining") {
            return Err(EngineError::InvalidState(
                "v7 diceLocks.remaining is required".into(),
            ));
        }
        let remaining = counter(fields.get("remaining"), "diceLocks.remaining")? - 1;
        if remaining > 0 {
            fields.insert("remaining".into(), json!(remaining));
        } else {
            *entry = Value::Null;
        }
    }
    each_piece_once(&mut next, |piece| {
        if piece.color == actor && piece.kind == "shotgunKing" {
            let cooldown = counter(piece.extra.get("snipeCooldown"), "snipeCooldown")?;
            if cooldown > 0 {
                piece
                    .extra
                    .insert("snipeCooldown".into(), json!(cooldown - 1));
            }
        }
        Ok(())
    })?;
    crate::v7_campaign::clear_blood_moon_turn_effects(&mut next, actor)?;
    *state = next;
    Ok(V7FlowControl::Continue)
}

/// After Black's `fullMove++` and source `cleanupFullMoveEffects`, before
/// winter and periodic-collapse callbacks (93706-93710).
pub(crate) fn after_full_move_cleanup(
    state: &mut GameState,
    _actor: Color,
) -> Result<V7FlowControl> {
    if let Some(flow) = outgoing_boundary(state)? {
        return Ok(flow);
    }
    let has_expiring = state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|piece| truth(piece.extra.get("severed")) || truth(piece.extra.get("iceSheet")));
    let blood_moon = state
        .extra
        .get("campaign")
        .and_then(|value| value.get("setup"))
        .and_then(Value::as_str)
        == Some("bloodMoon");
    if !has_expiring && !blood_moon {
        return Ok(V7FlowControl::Continue);
    }
    let mut next = state.clone();
    let full_move = i64::from(next.full_move);
    each_piece_once(&mut next, |piece| {
        if let Some(severed) = piece.extra.get("severed").and_then(Value::as_object)
            && severed.get("expiresFullMove").is_some()
        {
            let expires = counter(severed.get("expiresFullMove"), "severed.expiresFullMove")?;
            let remaining = if severed.get("remaining").is_some() {
                counter(severed.get("remaining"), "severed.remaining")?.max(0)
            } else {
                (expires - full_move).max(0)
            };
            if remaining <= 0 {
                piece.extra.shift_remove("severed");
            }
        }
        if truth(piece.extra.get("iceSheet")) {
            let remaining = match piece.extra.get("iceSheet").and_then(Value::as_object) {
                Some(ice) => counter(ice.get("remaining"), "iceSheet.remaining")?.max(0),
                None => 0,
            };
            if remaining <= 0 {
                piece.extra.shift_remove("iceSheet");
            }
        }
        Ok(())
    })?;
    crate::v7_campaign::update_blood_moon_cycle_log(&mut next)?;
    *state = next;
    Ok(V7FlowControl::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameConfig, Piece};

    fn state() -> GameState {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.mode = "play".into();
        state.turn = Color::White;
        state
    }

    #[test]
    fn undead_capture_schedules_once_with_source_half_turn_offset() {
        let mut before_boundary = state();
        before_boundary.move_count = 11;
        let captured = Piece::new("undead", Color::Black, "undead-black");
        let mut expected_rng = before_boundary.rng.clone();
        expected_rng.sample().unwrap();
        schedule_undead_resurrection(&mut before_boundary, &captured, Color::White, false).unwrap();
        assert_eq!(before_boundary.rng, expected_rng);
        assert_eq!(
            before_boundary.extra["undeadResurrections"][0]["dueMoveCount"],
            json!(16)
        );
        assert_eq!(
            before_boundary.extra["undeadResurrections"][0]["remainingHalfTurns"],
            json!(5)
        );
        let once = before_boundary.clone();
        schedule_undead_resurrection(&mut before_boundary, &captured, Color::White, false).unwrap();
        assert_eq!(before_boundary, once);

        let mut after_boundary = state();
        after_boundary.move_count = 11;
        schedule_undead_resurrection(&mut after_boundary, &captured, Color::White, true).unwrap();
        assert_eq!(
            after_boundary.extra["undeadResurrections"][0]["dueMoveCount"],
            json!(15)
        );
        assert_eq!(
            after_boundary.extra["undeadResurrections"][0]["remainingHalfTurns"],
            json!(4)
        );
    }

    #[test]
    fn undead_countdown_and_blocked_home_rank_retry_preserve_source_rng() {
        let mut state = state();
        let captured = Piece::new("undead", Color::White, "revive-white");
        state.move_count = 5;
        state.extra.insert(
            "undeadResurrections".into(),
            json!([
                {"color":"white","capturedBy":"black","piece":captured,
                 "remainingHalfTurns":2,"dueMoveCount":0},
            ]),
        );
        state.captures.black.push(captured);
        let rng = state.rng.clone();
        resolve_undead_resurrections_after_move(&mut state).unwrap();
        assert_eq!(
            state.extra["undeadResurrections"][0]["remainingHalfTurns"],
            json!(1)
        );
        assert_eq!(state.rng, rng);
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        resolve_undead_resurrections_after_move(&mut state).unwrap();
        assert_eq!(
            state.extra["undeadResurrections"][0]["remainingHalfTurns"],
            json!(1)
        );
        assert_eq!(
            state.extra["undeadResurrections"][0]["dueMoveCount"],
            json!(6)
        );
        assert_eq!(state.rng, expected_rng);
        assert_eq!(state.captures.black.len(), 1);
        state.board[7][4] = None;
        expected_rng.sample().unwrap();
        resolve_undead_resurrections_after_move(&mut state).unwrap();
        assert_eq!(state.board[7][4].as_ref().unwrap().id, "revive-white");
        assert_eq!(state.extra["undeadResurrections"], json!([]));
        assert!(state.captures.black.is_empty());
        assert_eq!(state.rng, expected_rng);
    }

    #[test]
    fn undead_trickster_revives_with_clean_statuses_but_retained_identity_traits() {
        let mut state = state();
        state.board = vec![vec![None; 8]; 8];
        for col in 1..8 {
            state.board[0][col] = Some(Piece::new(
                "wall",
                PieceColor::Neutral,
                format!("block-{col}"),
            ));
        }
        let mut captured = Piece::new("trickster", Color::Black, "undead-trickster");
        for field in TRICKSTER_UNDEAD_STATUS_FIELDS {
            captured.extra.insert((*field).into(), json!(true));
        }
        captured
            .extra
            .insert("tricksterMoveType".into(), json!("undead"));
        captured
            .extra
            .insert("wanted".into(), json!({"by":"white"}));
        state.captures.white.push(captured.clone());
        state.extra.insert(
            "undeadResurrections".into(),
            json!([
                {"color":"black","capturedBy":"white","piece":captured,
                 "remainingHalfTurns":1,"dueMoveCount":100},
            ]),
        );
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        resolve_undead_resurrections_after_move(&mut state).unwrap();
        let revived = state.board[0][0].as_ref().unwrap();
        assert_eq!(revived.id, "undead-trickster");
        assert_eq!(revived.kind, "trickster");
        assert!(revived.moved);
        assert_eq!(revived.extra["tricksterMoveType"], json!("undead"));
        assert_eq!(revived.extra["wanted"], json!({"by":"white"}));
        assert!(
            TRICKSTER_UNDEAD_STATUS_FIELDS
                .iter()
                .all(|field| !revived.extra.contains_key(*field))
        );
        assert_eq!(state.extra["undeadResurrections"], json!([]));
        assert!(state.captures.white.is_empty());
        assert_eq!(state.rng, expected_rng);
    }

    #[test]
    fn recurrence_precedes_undead_and_pawn_capture_sets_resolve_ready() {
        let mut state = state();
        state.set_flag("resolve", Color::Black, true);
        let mut captured = Piece::new("pawn", Color::Black, "recurring-pawn");
        captured.extra.insert("recurrence".into(), json!(true));
        captured.extra.insert("maxHp".into(), json!(3));
        captured.extra.insert("hp".into(), json!(1));
        let rng = state.rng.clone();
        schedule_undead_resurrection(&mut state, &captured, Color::White, false).unwrap();
        assert_eq!(state.rng, rng);
        assert_eq!(state.extra["resolveReady"]["black"], json!(true));
        assert_eq!(
            state.extra["pendingRecurrences"][0]["capturedBy"],
            json!("white")
        );
        assert_eq!(
            state.extra["pendingRecurrences"][0]["piece"]["hp"],
            json!(3)
        );
        assert_eq!(
            state.extra["pendingRecurrences"][0]["piece"]["moved"],
            json!(true)
        );
        assert!(
            state.extra["pendingRecurrences"][0]["piece"]
                .get("recurrence")
                .is_none()
        );
        assert_eq!(state.extra["undeadResurrections"], json!([]));
    }

    #[test]
    fn recurrence_uses_first_home_rank_and_skips_hazards_and_reservations() {
        let mut state = state();
        state.board = vec![vec![None; 8]; 8];
        for col in 0..8 {
            if ![2, 4, 5].contains(&col) {
                state.board[7][col] = Some(Piece::new(
                    "wall",
                    PieceColor::Neutral,
                    format!("wall-{col}"),
                ));
            }
        }
        state
            .extra
            .insert("blackHole".into(), json!([{"row":7,"col":2}]));
        state.extra.insert(
            "pendingScarecrows".into(),
            json!([{"row":7,"col":5,"color":"black"}]),
        );
        let mut captured = Piece::new("queen", Color::White, "recurring-queen");
        captured.extra.insert("recurrence".into(), json!(true));
        captured
            .extra
            .insert("outpostProtected".into(), json!(true));
        state.captures.black.push(captured.clone());
        queue_recurrence(&mut state, &captured, Color::Black).unwrap();
        let history = state.history.clone();
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        let revived = resolve_recurrences(&mut state).unwrap();
        assert_eq!(revived.len(), 1);
        assert_eq!(revived[0].origin, Square { row: 7, col: 4 });
        assert_eq!(revived[0].cells, vec![Square { row: 7, col: 4 }]);
        assert_eq!(revived[0].piece.id, "recurring-queen");
        assert!(revived[0].piece.moved);
        assert!(!revived[0].piece.extra.contains_key("recurrence"));
        assert!(!revived[0].piece.extra.contains_key("outpostProtected"));
        assert!(state.captures.black.is_empty());
        assert_eq!(state.extra["pendingRecurrences"], json!([]));
        assert_eq!(state.extra["logs"][0], "회귀: e1에서 다시 소환되었습니다.");
        assert!(
            state.extra["forceAnimatedPieceIds"]["values"]
                .as_array()
                .unwrap()
                .contains(&json!("recurring-queen"))
        );
        assert_eq!(state.rng, expected_rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn large_recurrence_restores_hp_and_one_shared_anchor() {
        let mut state = state();
        state.board = vec![vec![None; 8]; 8];
        for row in [6, 7] {
            for col in 0..8 {
                if ![4, 5].contains(&col) {
                    state.board[row][col] = Some(Piece::new(
                        "wall",
                        PieceColor::Neutral,
                        format!("wall-{row}-{col}"),
                    ));
                }
            }
        }
        let mut captured = Piece::new("bigRook", Color::White, "recurring-large");
        captured.extra.insert("recurrence".into(), json!(true));
        captured.extra.insert("maxHp".into(), json!(2));
        captured.extra.insert("hp".into(), json!(1));
        captured.extra.insert("anchorRow".into(), json!(3));
        captured.extra.insert("anchorCol".into(), json!(1));
        state.captures.black.push(captured.clone());
        queue_recurrence(&mut state, &captured, Color::Black).unwrap();
        let revived = resolve_recurrences(&mut state).unwrap();
        assert_eq!(revived[0].origin, Square { row: 6, col: 4 });
        assert_eq!(revived[0].cells.len(), 4);
        assert_eq!(revived[0].piece.extra["hp"], 2);
        assert_eq!(revived[0].piece.extra["anchorRow"], 6);
        assert_eq!(revived[0].piece.extra["anchorCol"], 4);
        for at in &revived[0].cells {
            assert_eq!(state.at(*at), Some(&revived[0].piece));
        }
        assert!(state.captures.black.is_empty());
    }

    #[test]
    fn recurrence_full_board_retains_queue_and_live_identity_drops_without_rng() {
        let mut state = state();
        let mut captured = Piece::new("rook", Color::Black, "recurring-rook");
        captured.extra.insert("recurrence".into(), json!(true));
        for row in 0..8 {
            for col in 0..8 {
                state.board[row][col] = Some(Piece::new(
                    "wall",
                    PieceColor::Neutral,
                    format!("wall-{row}-{col}"),
                ));
            }
        }
        queue_recurrence(&mut state, &captured, Color::White).unwrap();
        let before = state.clone();
        assert!(resolve_recurrences(&mut state).unwrap().is_empty());
        assert_eq!(state, before);
        state.board[3][3] = Some(captured);
        let rng = state.rng.clone();
        assert!(resolve_recurrences(&mut state).unwrap().is_empty());
        assert_eq!(state.extra["pendingRecurrences"], json!([]));
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn invalid_recurrence_capture_owner_preserves_queue_board_and_rng() {
        let mut state = state();
        state.board = vec![vec![None; 8]; 8];
        state.extra.insert(
            "pendingRecurrences".into(),
            json!([{
                "piece": Piece::new("queen", Color::White, "q"), "capturedBy":"neutral"
            }]),
        );
        let before = state.clone();
        assert!(resolve_recurrences(&mut state).is_err());
        assert_eq!(state, before);
    }

    #[test]
    fn lunchbox_due_records_clean_victim_and_removes_without_rng() {
        let mut state = state();
        state.turn = Color::Black;
        state.board = vec![vec![None; 8]; 8];
        let mut pawn = Piece::new("pawn", Color::White, "lunchbox");
        pawn.extra
            .insert("emptyLunchbox".into(), json!({"deadlineTurn":1}));
        state.board[4][4] = Some(pawn);
        state.turns_taken.white = 1;
        let rng = state.rng.clone();
        assert_eq!(
            after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert!(state.board[4][4].is_none());
        assert_eq!(state.captures.black[0].id, "lunchbox");
        assert!(!state.captures.black[0].extra.contains_key("emptyLunchbox"));
        assert_eq!(
            state.extra["logs"][0],
            "빈 찬합: e4의 기물이 킹 곁에 가지 못해 자결했습니다."
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.turn, Color::Black);
    }

    #[test]
    fn adjacent_lunchbox_survives_at_first_royal() {
        let mut state = state();
        state.board = vec![vec![None; 8]; 8];
        state.board[4][4] = Some(Piece::new("king", Color::White, "royal"));
        let mut pawn = Piece::new("pawn", Color::White, "lunchbox");
        pawn.extra
            .insert("emptyLunchbox".into(), json!({"deadlineTurn":1}));
        state.board[4][5] = Some(pawn);
        state.turns_taken.white = 1;
        assert_eq!(
            after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert!(
            !state.board[4][5]
                .as_ref()
                .unwrap()
                .extra
                .contains_key("emptyLunchbox")
        );
    }

    #[test]
    fn merchant_is_royal_only_with_september18_balance() {
        let mut state = state();
        state.board = vec![vec![None; 8]; 8];
        state.board[4][4] = Some(Piece::new("merchant", Color::White, "merchant"));
        let mut pawn = Piece::new("pawn", Color::White, "lunchbox");
        pawn.extra
            .insert("emptyLunchbox".into(), json!({"deadlineTurn":1}));
        state.board[4][5] = Some(pawn);
        state.turns_taken.white = 1;
        state.extra.insert("september18Balance".into(), json!(true));
        assert_eq!(
            after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert!(
            !state.board[4][5]
                .as_ref()
                .unwrap()
                .extra
                .contains_key("emptyLunchbox")
        );

        state.board[4][5]
            .as_mut()
            .unwrap()
            .extra
            .insert("emptyLunchbox".into(), json!({"deadlineTurn":1}));
        state
            .extra
            .insert("september18Balance".into(), json!(false));
        assert_eq!(
            after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert!(state.board[4][5].is_none());
        assert_eq!(state.captures.black[0].id, "lunchbox");
    }

    #[test]
    fn prophecy_skip_then_white_win_precedes_black() {
        let mut state = state();
        state.turn = Color::Black;
        state.move_count = 7;
        state.extra.insert(
            "prophecy".into(),
            json!({
                "white":{"skipMoveCount":7,"remainingHalfTurns":1},
                "black":{"remainingHalfTurns":1}
            }),
        );
        assert_eq!(
            after_deathmatch(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal
        );
        assert_eq!(state.winner.as_deref(), Some("black"));
        assert_eq!(state.extra["prophecy"]["white"]["remainingHalfTurns"], 1);
        assert!(
            state.extra["prophecy"]["white"]
                .get("skipMoveCount")
                .is_none()
        );
        assert_eq!(state.turn, Color::Black);
    }

    #[test]
    fn independent_statuses_expire_once_per_piece_identity() {
        let mut state = state();
        state.turn = Color::Black;
        state.board = vec![vec![None; 8]; 8];
        let mut large = Piece::new("colossus", Color::White, "large");
        large
            .extra
            .insert("frozenByCard".into(), json!({"remaining":2}));
        large.extra.insert("frozen".into(), json!(true));
        state.board[4][4] = Some(large.clone());
        state.board[4][5] = Some(large);
        let mut rook = Piece::new("rook", Color::White, "rook");
        rook.extra
            .insert("sacrificeProtection".into(), json!({"remaining":1}));
        rook.extra.insert("protected".into(), json!(true));
        rook.extra.insert("poisonStunTurns".into(), json!(1));
        rook.extra.insert("poisonStunColor".into(), json!("white"));
        rook.extra.insert("chargeRush".into(), json!(true));
        state.board[5][3] = Some(rook);
        state.extra.insert(
            "hallucination".into(),
            json!({"white":{"remaining":1},"black":null}),
        );
        assert_eq!(
            after_revolving(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        for col in [4, 5] {
            assert_eq!(
                state.board[4][col].as_ref().unwrap().extra["frozenByCard"]["remaining"],
                1
            );
        }
        let rook = state.board[5][3].as_ref().unwrap();
        for field in [
            "sacrificeProtection",
            "protected",
            "poisonStunTurns",
            "poisonStunColor",
            "chargeRush",
        ] {
            assert!(
                !rook.extra.contains_key(field),
                "unexpected remaining {field}"
            );
        }
        assert!(state.extra["hallucination"]["white"].is_null());
        assert_eq!(state.turn, Color::Black);
    }

    #[test]
    fn pawn_charge_rush_failure_cancels_prophecy_before_hallucination_tick() {
        let mut state = state();
        state.board = vec![vec![None; 8]; 8];
        let mut pawn = Piece::new("pawn", Color::White, "sprinter");
        pawn.extra.insert("chargeRush".into(), json!(true));
        state.board[4][4] = Some(pawn);
        state.extra.insert(
            "hallucination".into(),
            json!({"white":{"remaining":1},"black":null}),
        );
        state.extra.insert(
            "prophecy".into(),
            json!({"white":{"remainingHalfTurns":3},"black":null}),
        );
        let rng = state.rng.clone();
        assert_eq!(
            after_revolving(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert!(state.board[4][4].is_none());
        assert_eq!(state.captures.black[0].id, "sprinter");
        assert!(!state.captures.black[0].extra.contains_key("chargeRush"));
        assert!(state.extra["prophecy"]["white"].is_null());
        assert!(state.extra["hallucination"]["white"].is_null());
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn lock_and_cooldown_expire_without_ticking_other_side() {
        let mut state = state();
        state.turn = Color::Black;
        state.board = vec![vec![None; 8]; 8];
        let mut white = Piece::new("shotgunKing", Color::White, "shotgun-white");
        white.extra.insert("snipeCooldown".into(), json!(2));
        state.board[3][3] = Some(white);
        let mut black = Piece::new("shotgunKing", Color::Black, "shotgun-black");
        black.extra.insert("snipeCooldown".into(), json!(2));
        state.board[2][2] = Some(black);
        state.turns_taken.white = 2;
        state.extra.insert(
            "initiative".into(),
            json!({"white":{"startTurn":0,"limit":2},"black":null}),
        );
        state.extra.insert(
            "diceLocks".into(),
            json!({"white":{"type":"king","remaining":1},"black":null}),
        );
        assert_eq!(
            after_scarecrows(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert!(state.extra["initiative"]["white"].is_null());
        assert!(state.extra["diceLocks"]["white"].is_null());
        assert_eq!(
            state.board[3][3].as_ref().unwrap().number("snipeCooldown"),
            1
        );
        assert_eq!(
            state.board[2][2].as_ref().unwrap().number("snipeCooldown"),
            2
        );
        assert_eq!(state.turn, Color::Black);
    }

    #[test]
    fn blood_moon_changes_at_shared_turn_four() {
        let mut state = state();
        state.turn = Color::Black;
        state.turns_taken.white = 4;
        state.turns_taken.black = 4;
        state.extra.insert(
            "campaign".into(),
            json!({"setup":"bloodMoon","bloodMoon":{"lastPeriod":0,"sunlightOverride":"white"}}),
        );
        after_scarecrows(&mut state, Color::White).unwrap();
        assert!(state.extra["campaign"]["bloodMoon"]["sunlightOverride"].is_null());
        after_full_move_cleanup(&mut state, Color::White).unwrap();
        assert_eq!(state.extra["campaign"]["bloodMoon"]["lastPeriod"], 1);
        assert_eq!(state.turn, Color::Black);
    }

    #[test]
    fn terminal_outgoing_stages_leave_piece_statuses_rng_and_history_unchanged() {
        type Stage = fn(&mut GameState, Color) -> Result<V7FlowControl>;
        let stages: [(&str, Stage); 5] = [
            ("after_count", after_count),
            ("after_revolving", after_revolving),
            ("after_deathmatch", after_deathmatch),
            ("after_scarecrows", after_scarecrows),
            ("after_full_move_cleanup", after_full_move_cleanup),
        ];
        for (name, stage) in stages {
            let mut terminal = state();
            terminal.mode = "gameover".into();
            terminal.turn = Color::Black;
            let before = terminal.clone();
            assert_eq!(
                stage(&mut terminal, Color::White).unwrap(),
                V7FlowControl::Terminal,
                "{name}"
            );
            assert_eq!(terminal, before, "{name}");
        }
    }
}
