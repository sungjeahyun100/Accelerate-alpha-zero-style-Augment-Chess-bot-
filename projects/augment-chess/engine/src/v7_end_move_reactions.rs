//! Source-ordered v7 `endMove` settlement before `completeTurnAfterMove`.
//!
//! The frozen client calls automatic reactions before its completed-turn
//! callback. A pending reaction must not disappear just because the ordinary
//! board move succeeded. `preflight_end_move` is read-only and is intended to
//! run on the transaction-owned position before any end-move mutation.

use crate::v7_turn_flow::V7FlowControl;
use crate::{Color, EngineError, GameState, Piece, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Source endMove keeps these private values across its counted callbacks and
/// restores them in finally. The full transition caller owns that finalization;
/// they must never become fields in a public Position envelope.
#[derive(Default)]
pub(crate) struct V7EndMoveContext {
    pub replay_capture: Option<crate::replay::MoveReplayCapture>,
    pub previous_history_number: Option<Value>,
    pub entered_history_scope: bool,
}

/// Check source-reachable reactions before the first v7 end-move mutation.
/// Each unsupported error names the exact source callback and active field.
pub(crate) fn preflight_end_move(state: &GameState, actor: Color) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 endMove on rules version {}",
            state.ruleset_id
        )));
    }
    if state.mode == "gameover" {
        return Ok(());
    }
    // The internal source callback settles movingColor, which may differ
    // from state.turn after Spy promotion. Public admission owns actor checks.
    if state.mode != "play" {
        return Err(EngineError::WrongActor);
    }
    if state
        .extra
        .get("freeMoveResolution")
        .is_some_and(has_entries)
    {
        return Err(unsupported(
            "freeMoveResolution",
            "source execution context must not enter the position DTO",
        ));
    }
    // The source's execution-only freeMoveResolution global returns before
    // counted queues and one-turn status ticks; those fields cannot block it.
    if state.free_move_resolution == Some(actor) {
        return Ok(());
    }

    // These legacy queue names do not belong to the frozen v7 source. Reject
    // an active foreign DTO explicitly rather than silently discarding it.
    for (field, callback) in [
        ("pendingUndead", "resolveUndeadResurrectionsAfterMove"),
        ("pendingRuleMonsters", "moveRuleMonsters"),
    ] {
        if state.extra.get(field).is_some_and(has_entries) {
            return Err(unsupported(field, callback));
        }
    }
    for field in ["temporaryQueens", "necromancy"] {
        let Some(value) = state.extra.get(field) else {
            continue;
        };
        // Source tickNecromancy normalizes a non-array queue to an empty
        // array; temporaryQueens is an array invariant of the host state.
        if field == "necromancy" && !value.is_array() {
            continue;
        }
        let entries = value
            .as_array()
            .ok_or_else(|| EngineError::InvalidState(format!("v7 {field} must be an array")))?;
        if entries.len() > 256 {
            return Err(EngineError::InvalidState(format!(
                "v7 {field} exceeds 256 entries"
            )));
        }
        for entry in entries {
            if entry.get("color").and_then(Value::as_str) != Some(actor.as_str()) {
                continue;
            }
            if field == "temporaryQueens"
                && crate::observation::number(entry.get("remaining")).is_none()
            {
                return Err(EngineError::InvalidState(
                    "v7 temporaryQueens.remaining must be a finite number".into(),
                ));
            }
        }
    }
    if state.extra.get("activeTrolley").is_some_and(has_entries) {
        return Err(unsupported(
            "activeTrolley",
            "resolveTrolleyBundle must finish the active decision before endMove",
        ));
    }

    let mut seen = BTreeSet::new();
    for piece in state.board.iter().flatten().flatten() {
        if !seen.insert(piece.id.clone()) {
            continue;
        }
        if piece.color != actor {
            continue;
        }
        if piece.extra.get("witchTrial").is_some_and(js_truthy) {
            witch_trial_counts_on(piece, actor, state)?;
            remaining(piece, "witchTrial")?;
        }
        for field in ["disarmed", "staked", "severed", "iceSheet"] {
            if !piece.extra.get(field).is_some_and(js_truthy) {
                continue;
            }
            if field == "severed"
                && piece
                    .extra
                    .get(field)
                    .and_then(|value| crate::observation::number(value.get("remaining")))
                    .is_none()
            {
                continue;
            }
            remaining(piece, field)?;
        }
    }
    // Witch trials can count on the opposite color's turn. Check them in a
    // separate pass rather than assuming their subject owns the counter.
    for piece in state.board.iter().flatten().flatten() {
        if piece.color != actor && piece.extra.get("witchTrial").is_some_and(js_truthy) {
            witch_trial_counts_on(piece, actor, state)?;
            remaining(piece, "witchTrial")?;
        }
    }
    Ok(())
}

/// Settle the source's `endMove` callbacks before `moveCount++`. The caller
/// must not run the legacy v6 `tick_piece_turn_effects` on the same v7 turn.
/// A `Continue` result must be followed by the move-counted callbacks and
/// then `settle_end_move_after_count`, before first-move cards are forced.
#[cfg(test)]
pub(crate) fn settle_end_move_before_count(
    state: &mut GameState,
    actor: Color,
) -> Result<V7FlowControl> {
    let mut context = V7EndMoveContext::default();
    let result = settle_end_move_before_count_with_context(state, actor, &mut context);
    // This compatibility API runs only the pre-count callback group. Its
    // caller cannot finish the source's entire try/finally, so keep its prior
    // history-key lifetime; the real endMove caller retains the context instead.
    if context.entered_history_scope {
        match context.previous_history_number {
            Some(value) => {
                state.extra.insert("activeHistoryMoveNumber".into(), value);
            }
            None => {
                state.extra.shift_remove("activeHistoryMoveNumber");
            }
        }
    }
    result
}

/// The full endMove caller must finalize a context that entered the history
/// scope on success, same-turn return, terminal return, and error alike.
pub(crate) fn settle_end_move_before_count_with_context(
    state: &mut GameState,
    actor: Color,
    context: &mut V7EndMoveContext,
) -> Result<V7FlowControl> {
    crate::legal_profile::measure("settle_end_move_before_count", || {
        settle_end_move_before_count_with_context_profiled(state, actor, context)
    })
}

pub(crate) fn settle_end_move_before_count_with_context_profiled(
    state: &mut GameState,
    actor: Color,
    context: &mut V7EndMoveContext,
) -> Result<V7FlowControl> {
    // Source main99894 keeps actor and snapshot in one private capture. Copy
    // that pair together; snapshot.turn must never be used to infer its actor.
    context.replay_capture = crate::replay::active_move_capture(state)?;
    context.previous_history_number = None;
    context.entered_history_scope = false;
    preflight_end_move(state, actor)?;

    // Frozen `endMove` resolves recurrence before rule bombs and democracy.
    // The same helper is also called by source refreshSubmergedPieces; it
    // leaves blocked revivals queued and consumes no RNG when none fit.
    crate::v7_piece_lifecycle::resolve_recurrences(state)?;
    crate::v7_threat::break_initiative_by_check_v7(state, actor)?;
    crate::replay::commit_active_move(state, actor)?;
    if state.mode == "gameover" {
        return Ok(V7FlowControl::Terminal);
    }

    // Frozen `endMove` resolves rule bombs after recurrence/replay admission
    // and before democracy, retaliation, submerged refresh, or flag checks.
    // The bomb helper owns its source normalization, capture ledger, replay,
    // and precise Unsupported boundary for active environmental callbacks.
    crate::v7_rule_bombs::resolve_under_pieces(state, actor, false)?;
    for color in [Color::White, Color::Black] {
        if crate::flow::check_democracy_defeat(
            state,
            color,
            color.opponent(),
            "모든 폰이 잡혔습니다.",
        )? {
            return Ok(V7FlowControl::Terminal);
        }
    }
    // main93301 explicitly records a bomb terminal before entering the later
    // history scope. The full caller must not add another pre-scope record.
    if state.mode == "gameover" {
        crate::replay::record(state, "gameover")?;
        return Ok(V7FlowControl::Terminal);
    }
    crate::v7_capture_reactions::resolve_pending_bear_retaliations(state, Some(actor), None)?;
    crate::v7_capture_reactions::resolve_pending_trojan_horse_retaliations(
        state,
        Some(actor),
        None,
    )?;
    if state.mode == "gameover" {
        return Ok(V7FlowControl::Terminal);
    }
    crate::v7_turn_entry::resolve_twin_swaps(state)?;
    if crate::v7_threat::check_racing_kings_v7(state)? {
        crate::replay::record(state, "gameover")?;
        return Ok(V7FlowControl::Terminal);
    }

    // The source next refreshes submerged identities and checks royal
    // herald/racing outcomes before considering a same-turn continuation.
    crate::transition::refresh_submerged(state)?;
    if crate::v7_capture_objectives::after_end_move_reactions(state, actor)?
        != V7FlowControl::Continue
    {
        return Ok(V7FlowControl::Terminal);
    }
    if crate::transition::resolve_herald_threats(state, actor)? {
        return Ok(V7FlowControl::Terminal);
    }
    if state.free_move_resolution == Some(actor) {
        clear_selection(state);
        crate::v7_board_hazards::black_hole_deaths(state, actor)?;
        return Ok(
            if crate::transition::resolve_herald_threats(state, actor)? {
                V7FlowControl::Terminal
            } else {
                V7FlowControl::RetainTurn
            },
        );
    }

    // endMove clears repeat blocks before same-turn credits. The frozen
    // source's early return leaves all completed-turn counters untouched.
    for_each_unique_piece(state, |piece, _| {
        if piece.color == actor {
            piece.extra.shift_remove("idolEncoreRestTurn");
        }
        Ok(())
    })?;
    // main93332 enters this scope only after the FreeMove early return and
    // Idol repeat-block cleanup. Later notation uses the current full move;
    // the full caller restores the previous key in its source finally block.
    context.previous_history_number = state.extra.get("activeHistoryMoveNumber").cloned();
    context.entered_history_scope = true;
    state.extra.insert(
        "activeHistoryMoveNumber".into(),
        json!(state.full_move.max(1)),
    );
    clear_selection(state);
    crate::v7_board_hazards::black_hole_deaths(state, actor)?;
    if state.mode == "gameover" || crate::transition::resolve_herald_threats(state, actor)? {
        return Ok(V7FlowControl::Terminal);
    }
    auto_advance_logs(state, actor)?;
    crate::v7_board_hazards::update_palaces(state)?;
    if crate::transition::resolve_herald_threats(state, actor)? {
        return Ok(V7FlowControl::Terminal);
    }
    if consume_idol_encore(state, actor)? {
        return Ok(V7FlowControl::RetainTurn);
    }
    if consume_resolve_credit(state, actor)? {
        crate::replay::record(state, "resolve")?;
        return Ok(V7FlowControl::RetainTurn);
    }
    if let Some(extra_move) = state
        .extra
        .get_mut("effects")
        .and_then(Value::as_object_mut)
        .and_then(|effects| effects.get_mut("extraMove"))
    {
        let count = extra_move.as_i64().ok_or_else(|| {
            EngineError::InvalidState("v7 effects.extraMove must be an integer".into())
        })?;
        if count < 0 {
            return Err(EngineError::InvalidState(
                "v7 effects.extraMove cannot be negative".into(),
            ));
        }
        if count > 0 {
            *extra_move = json!(count - 1);
            return Ok(V7FlowControl::RetainTurn);
        }
    }
    if state.actions_remaining > 1 {
        state.actions_remaining -= 1;
        if crate::v7_turn_flow::check_no_action_loss_for_color_v7(state, actor)? {
            return Ok(V7FlowControl::Terminal);
        }
        return Ok(V7FlowControl::RetainTurn);
    }
    if retain_time_stop(state, actor)? {
        return Ok(V7FlowControl::RetainTurn);
    }

    // main93370 restores the active metal mover before completeThreeTurn
    // decreases its cooldown. Same-turn returns and FreeMove bypass this hook.
    crate::v7_move_transition::sync_active_metal(state)?;

    // `completeThreeTurn`, the second submerged refresh, and the thief path
    // reset precede the temporary-queen and necromancy queues in the source.
    let snapshot = state.clone();
    for_each_unique_piece(state, |piece, square| {
        if piece.color == actor {
            if piece.extra.get("metalized").is_some_and(js_truthy) {
                let cooldown =
                    crate::observation::number(piece.extra.get("metalCooldown")).unwrap_or(0.0);
                if cooldown > 0.0 {
                    piece
                        .extra
                        .insert("metalCooldown".into(), js_number(cooldown - 1.0));
                }
            }
            if piece.ability_kind() == "octopus" && !has_adjacent_enemy(&snapshot, square, actor) {
                piece.extra.insert("submerged".into(), json!(true));
            }
        }
        Ok(())
    })?;
    crate::transition::refresh_submerged(state)?;
    let arrested_thieves = tick_thief_arrests(state, actor)?;
    resolve_wanted_arrests(state, actor)?;
    if !arrested_thieves.is_empty() {
        crate::replay::queue_visual(
            state,
            json!({
                "type":"board-change","effect":"thief-arrest","color":actor,
                "removals":arrested_thieves.iter().map(|(square,piece)| json!({
                    "square":square,"color":piece.color,"pieceType":piece.kind
                })).collect::<Vec<_>>(),
                "relocations":[],"transformations":[],"spawns":[]
            }),
        )?;
    }
    if state.extra.contains_key("disassembly") {
        state.set_flag("disassembly", actor, false);
    }
    tick_temporary_queens(state, actor)?;
    tick_necromancy(state, actor)?;
    apply_delayed_hazards(state, actor)?;
    clear_time_traveler_turn_effects(state, actor)?;
    if state.mode == "gameover" || crate::transition::resolve_herald_threats(state, actor)? {
        return Ok(V7FlowControl::Terminal);
    }
    Ok(V7FlowControl::Continue)
}

// main95132. Each impact observes quantum occupancy before reading the live
// target; meteor's repeated cells damage an HP identity repeatedly, while
// lightning and ordinary pieces deduplicate by source identity.
fn apply_delayed_hazards(state: &mut GameState, actor: Color) -> Result<()> {
    let Some(entries) = state
        .extra
        .get("delayedHazards")
        .and_then(Value::as_array)
        .cloned()
    else {
        return Ok(());
    };
    if entries.len() > 256 {
        return Err(EngineError::InvalidState(
            "v7 delayedHazards exceeds 256 entries".into(),
        ));
    }
    let (due, future): (Vec<_>, Vec<_>) = entries.into_iter().partition(|entry| {
        entry.get("triggerAfter").and_then(Value::as_str) == Some(actor.as_str())
    });
    if due.is_empty() {
        return Ok(());
    }
    let mut next = state.clone();
    next.extra.insert("delayedHazards".into(), json!(future));
    for hazard in due {
        let owner = match hazard.get("owner").and_then(Value::as_str) {
            Some("white") => Color::White,
            Some("black") => Color::Black,
            _ => {
                return Err(EngineError::InvalidState(
                    "v7 delayedHazards.owner must name a player".into(),
                ));
            }
        };
        let spell = hazard
            .get("type")
            .and_then(Value::as_str)
            .filter(|spell| matches!(*spell, "meteor" | "lightning"))
            .ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 delayedHazards.type must be meteor or lightning".into(),
                )
            })?;
        let cells = hazard
            .get("cells")
            .and_then(Value::as_array)
            .filter(|cells| cells.len() <= 256)
            .ok_or_else(|| {
                EngineError::InvalidState("v7 delayedHazards.cells must be a bounded array".into())
            })?;
        let squares = cells
            .iter()
            .map(|cell| {
                serde_json::from_value::<Square>(cell.clone())
                    .map_err(EngineError::serialization)
                    .and_then(|square| {
                        if square.row < 8 && square.col < 8 {
                            Ok(square)
                        } else {
                            Err(EngineError::InvalidState(
                                "v7 delayedHazards cell outside board".into(),
                            ))
                        }
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        crate::replay::queue_visual(
            &mut next,
            json!({"type":"magic","phase":"impact","spell":spell,"color":owner,"cells":cells}),
        )?;
        next.extra.insert(
            "wizardImpact".into(),
            json!(
                cells
                    .iter()
                    .map(|cell| {
                        let mut cell = cell.clone();
                        cell["type"] = json!(spell);
                        cell
                    })
                    .collect::<Vec<_>>()
            ),
        );
        let mut caster = hazard
            .get("casterId")
            .and_then(Value::as_str)
            .map(|id| queued_piece_by_id(&next, id))
            .transpose()?
            .flatten()
            .map(|(_, piece)| piece);
        let mut seen = BTreeSet::new();
        let label = if spell == "meteor" {
            "메테오"
        } else {
            "번개"
        };
        for square in &squares {
            crate::v7_quantum_state::observe_quantum_at(&mut next, *square, owner)?;
            let Some(mut target) = next.at(*square).cloned() else {
                continue;
            };
            let hp = crate::v7_capture_reactions::is_hp_piece(&next, &target);
            let repeats = spell == "meteor" && hp;
            if target.kind == "wall"
                || !repeats && seen.contains(&target.id)
                || crate::movement::frozen(&target)
                || crate::v7_capture_reactions::nullification_blocks_optional(
                    &target,
                    caster.as_ref(),
                )
            {
                continue;
            }
            if !repeats {
                seen.insert(target.id.clone());
            }
            crate::v7_capture_reactions::break_initiative_by_attack(&mut next, &target, owner)?;
            if source_protected(&target) {
                if crate::observation::truth(target.extra.get("shielded")) {
                    crate::v7_capture_reactions::break_shield(&mut next, &mut target, owner)?;
                    replace_queued_piece(&mut next, &target);
                }
                crate::replay::add_log(
                    &mut next,
                    format!("{}의 보호가 {}를 막았습니다.", square_name(*square), label),
                )?;
                continue;
            }
            if crate::movement::v7_encouraged_at(&next, &target, *square) {
                continue;
            }
            let threat_probe = next.threat_probe_depth > 0;
            let options = crate::v7_capture_reactions::CaptureOptions {
                allow_jester: true,
                threat_probe,
                threat_source: Some(json!({"attacker":caster,"label":label})),
                ..Default::default()
            };
            let captured = if hp {
                crate::v7_capture_reactions::damage_health_piece_with_optional_attacker(
                    &mut next,
                    *square,
                    owner,
                    caster.as_mut(),
                    label,
                    &options,
                )?
            } else {
                crate::v7_capture_reactions::capture_at_with_optional_attacker(
                    &mut next,
                    *square,
                    owner,
                    caster.as_mut(),
                    &options,
                )?
            };
            if let Some(caster) = &caster {
                replace_queued_piece(&mut next, caster);
            }
            if !hp
                && captured
                    .as_ref()
                    .is_some_and(|piece| crate::observation::truth(piece.extra.get("explosive")))
            {
                crate::v7_board_hazards::explode_at(&mut next, *square, "자폭병")?;
            }
        }
        if next.extra.get("crownRule").is_some_and(js_truthy) {
            crate::v7_board_automata::reconcile_crown_rule(&mut next, true)?;
        }
        let cells_text = squares
            .iter()
            .map(|square| square_name(*square))
            .collect::<Vec<_>>()
            .join(", ");
        if spell != "lightning" {
            let notation_cells = cells_text.chars().take(12).collect::<String>();
            let notation_name = if notation_cells.is_empty() {
                label.to_owned()
            } else {
                format!("{label} {notation_cells}")
            };
            crate::replay::queue_special_effect_notation(
                &mut next,
                owner,
                &notation_name,
                &format!(
                    "{} {}가 {}에 적중",
                    crate::replay::label(owner),
                    label,
                    cells_text
                ),
            )?;
        }
        crate::replay::add_log(&mut next, format!("{label}가 {cells_text}에 떨어졌습니다."))?;
    }
    *state = next;
    Ok(())
}

fn source_protected(piece: &Piece) -> bool {
    piece.kind != "scarecrow"
        && (crate::observation::truth(piece.extra.get("shielded"))
            || crate::observation::truth(piece.extra.get("protected"))
            || crate::observation::number(
                piece
                    .extra
                    .get("vigilanceProtection")
                    .and_then(|status| status.get("remaining")),
            )
            .is_some_and(|remaining| remaining > 0.0)
            || matches!(piece.kind.as_str(), "football" | "monster"))
}

fn square_name(square: Square) -> String {
    format!("{}{}", char::from(b'a' + square.col), 8 - square.row)
}

// main102778/main102808. This accessor normalizes legacy campaign data even
// when this color has no active attack credit; only past/future are phases.
fn clear_time_traveler_turn_effects(state: &mut GameState, actor: Color) -> Result<()> {
    let Some(campaign) = state.extra.get_mut("campaign") else {
        return Ok(());
    };
    if campaign.get("setup").and_then(Value::as_str) != Some("timeTraveler") {
        return Ok(());
    }
    let campaign = campaign
        .as_object_mut()
        .ok_or_else(|| EngineError::InvalidState("v7 campaign must be an object".into()))?;
    if !crate::observation::truth(campaign.get("timeTraveler")) {
        campaign.insert(
            "timeTraveler".into(),
            json!({"visited":[],"phase":"future","attackEnabledFor":null}),
        );
    }
    let data = campaign
        .get_mut("timeTraveler")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            EngineError::InvalidState("v7 campaign.timeTraveler must be an object".into())
        })?;
    data.shift_remove("afterimageArmed");
    if !matches!(
        data.get("phase").and_then(Value::as_str),
        Some("past" | "future")
    ) {
        data.insert("phase".into(), json!("future"));
    }
    if data.get("attackEnabledFor").and_then(Value::as_str) == Some(actor.as_str()) {
        data.insert("attackEnabledFor".into(), Value::Null);
    }
    Ok(())
}

// main15857. Legacy countdown and the pre-September26 remake remove cells
// directly; they do not append a capture or invoke ordinary capture effects.
fn tick_thief_arrests(state: &mut GameState, actor: Color) -> Result<Vec<(Square, Piece)>> {
    let profile = match state
        .extra
        .get("cardState")
        .filter(|value| js_truthy(value))
    {
        Some(context) => context.get("profile"),
        None => state.extra.get("profile"),
    };
    let profile_hash = profile
        .and_then(|profile| profile.get("catalogHash"))
        .and_then(Value::as_str);
    let remake = if profile_hash.is_some() {
        crate::v7_queued_effects::uses_september18_balance(state)?;
        true
    } else {
        state.extra.get("thiefRemake") != Some(&Value::Bool(false))
    };
    let september26 = crate::v7_queued_effects::uses_september26_rebalance(state)?;
    let mut removed = Vec::new();
    for_each_unique_piece(state, |piece, square| {
        if remake {
            piece.extra.shift_remove("thiefVisited");
            piece.extra.shift_remove("thiefLastDirection");
            if piece.color == actor
                && !september26
                && piece.ability_kind() == "thief"
                && !crate::observation::truth(piece.extra.get("submerged"))
            {
                removed.push((square, piece.clone()));
            }
        } else if piece.color == actor
            && let Some(remaining) = piece
                .extra
                .get("thiefTurnsLeft")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && value.fract() == 0.0)
        {
            piece
                .extra
                .insert("thiefTurnsLeft".into(), js_number(remaining - 1.0));
            if remaining <= 1.0 {
                removed.push((square, piece.clone()));
            }
        }
        Ok(())
    })?;
    for (square, piece) in &removed {
        crate::transition::remove_piece_from_board_cells(state, piece, *square)?;
    }
    for (_, piece) in &removed {
        crate::transition::grant_vigilance_protection(state, piece)?;
    }
    Ok(removed)
}

/// Source main72670. All colors lose the previous ability on every completed
/// endMove, including the `countMove: false` branch.
pub(crate) fn clear_previous_trickster_abilities(state: &mut GameState) {
    for piece in state.board.iter_mut().flatten().flatten() {
        if piece.kind == "trickster" {
            piece.extra.shift_remove("tricksterPreviousAbilityForTurn");
            piece
                .source_order
                .retain(|field| field != "tricksterPreviousAbilityForTurn");
        }
    }
}

/// Source main93249, after undead resurrection and before Otherworld. A
/// simultaneous five-column completion draws only in September22 profiles.
pub(crate) fn check_gomoku_victory(state: &mut GameState) -> Result<bool> {
    crate::legal_profile::measure("game_over_gomoku_check", || {
        check_gomoku_victory_profiled(state)
    })
}

pub(crate) fn check_gomoku_victory_profiled(state: &mut GameState) -> Result<bool> {
    if state.mode == "gameover" {
        return Ok(false);
    }
    let winning = |color: Color| -> Option<Vec<Square>> {
        if !state.flag("gomoku", color) {
            return None;
        }
        for col in 0..8u8 {
            let mut run = Vec::<(Square, String)>::new();
            for row in 0..8u8 {
                let square = Square { row, col };
                let key = state
                    .at(square)
                    .filter(|piece| piece.color == color)
                    .map(|piece| {
                        if piece.id.is_empty() {
                            format!("{row}:{col}")
                        } else {
                            piece.id.clone()
                        }
                    });
                if key
                    .as_ref()
                    .is_none_or(|key| run.iter().any(|(_, seen)| seen == key))
                {
                    run.clear();
                } else if let Some(key) = key {
                    run.push((square, key));
                    if run.len() >= 5 {
                        return Some(
                            run.iter()
                                .rev()
                                .take(5)
                                .rev()
                                .map(|(square, _)| *square)
                                .collect(),
                        );
                    }
                }
            }
        }
        None
    };
    let white = winning(Color::White);
    let black = winning(Color::Black);
    if crate::v7_threat::uses_september22_rules(state)
        && let (Some(white), Some(black)) = (&white, &black)
    {
        state.extra.insert(
            "gomokuVictoryCells".into(),
            json!(white.iter().chain(black).collect::<Vec<_>>()),
        );
        crate::flow::end_game(state, None, "양측이 동시에 오목을 완성하여 무승부입니다.")?;
        return Ok(true);
    }
    for (color, cells) in [(Color::White, white), (Color::Black, black)] {
        if let Some(cells) = cells {
            state
                .extra
                .insert("gomokuVictoryCells".into(), json!(cells));
            crate::flow::end_game(
                state,
                Some(color),
                &format!(
                    "{}이 세로로 기물 5개를 완성했습니다.",
                    crate::replay::label(color)
                ),
            )?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// main93777. Scheduled Pawn Storm consumes every due queue entry first,
/// orders the referenced identities stably by row, and rechecks each pawn
/// against the board produced by the preceding advance.
pub(crate) fn apply_pending_pawn_storm_for_turn(
    state: &mut GameState,
    actor: Color,
) -> Result<usize> {
    let Some(entries) = state
        .extra
        .get("pendingPawnStorm")
        .and_then(Value::as_array)
        .cloned()
    else {
        return Ok(0);
    };
    if entries.is_empty() {
        return Ok(0);
    }
    if entries.len() > 256 {
        return Err(EngineError::InvalidState(
            "v7 pendingPawnStorm exceeds 256 entries".into(),
        ));
    }
    let (due, future): (Vec<_>, Vec<_>) = entries
        .into_iter()
        .partition(|entry| entry.get("color").and_then(Value::as_str) == Some(actor.as_str()));
    if due.is_empty() {
        return Ok(0);
    }
    let mut next = state.clone();
    next.extra.insert("pendingPawnStorm".into(), json!(future));
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    for entry in due {
        let refs = entry.get("pieces").and_then(Value::as_array);
        for reference in refs.into_iter().flatten() {
            let Some(id) = reference
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
            else {
                continue;
            };
            if !seen.insert(id.to_owned()) {
                continue;
            }
            if let Some((square, piece)) = queued_piece_by_id(&next, id)?
                && can_pending_pawn_storm_advance(&next, &piece, square, actor)?
            {
                ordered.push((square, id.to_owned()));
            }
        }
    }
    let dir = pawn_direction(&next, actor);
    ordered.sort_by_key(|(square, _)| if dir < 0 { square.row } else { 7 - square.row });
    let mut moves = Vec::new();
    for (from, id) in ordered {
        let Some(mut piece) = next.at(from).cloned().filter(|piece| piece.id == id) else {
            continue;
        };
        if !can_pending_pawn_storm_advance(&next, &piece, from, actor)? {
            continue;
        }
        let to = from
            .offset(pawn_direction(&next, actor), 0)
            .ok_or_else(|| {
                EngineError::InvalidState("v7 pawn storm destination outside board".into())
            })?;
        let privacy = movement_privacy(&next, &piece, from)?;
        next.board[usize::from(from.row)][usize::from(from.col)] = None;
        piece.moved = true;
        crate::transition::mark_card_no_capture(&next, &mut piece)?;
        crate::card_effects::note_ultimatum_movement(&mut next, &mut piece)?;
        next.board[usize::from(to.row)][usize::from(to.col)] = Some(piece.clone());
        crate::card_effects::mark_animation(&mut next, &piece)?;
        if crate::v7_promotion::should_promote_v7(&next, &piece, to)? {
            crate::v7_promotion::auto_promote_forced_pawn_v7(&mut next, to)?;
            piece = next.at(to).cloned().ok_or_else(|| {
                EngineError::InvalidState("v7 promoted pawn storm subject disappeared".into())
            })?;
        }
        moves.push((from, to, movement_hidden_from(&next, &piece, to, &privacy)?));
    }
    if !moves.is_empty() {
        if next.extra.get("crownRule").is_some_and(js_truthy) {
            crate::v7_board_automata::reconcile_crown_rule(&mut next, true)?;
        }
        crate::flow::mark_progress(&mut next);
        let hidden = moves
            .iter()
            .find(|entry| !entry.2.is_empty())
            .map_or("", |entry| entry.2.as_str());
        let sound = if actor == Color::White {
            "moveSelf"
        } else {
            "moveOpponent"
        };
        crate::card_effects::set_last_move(
            &mut next, moves[0].0, moves[0].1, sound, actor, hidden, None,
        )?;
        next.extra
            .get_mut("lastMove")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                EngineError::InvalidState("v7 pawn storm lastMove must be an object".into())
            })?
            .insert(
                "pawnStormMoves".into(),
                json!(
                    moves
                        .iter()
                        .map(|(from, to, _)| json!({"from":from,"to":to}))
                        .collect::<Vec<_>>()
                ),
            );
        crate::v7_threat::play_move_sound_v7(&mut next, sound, actor)?;
        crate::replay::add_log(
            &mut next,
            format!(
                "폰 스톰: {} 폰 {}개가 한 칸씩 전진했습니다.",
                crate::replay::label(actor),
                moves.len()
            ),
        )?;
        crate::replay::queue_special_effect_notation(
            &mut next,
            actor,
            &format!("폰스톰×{}", moves.len()),
            &format!(
                "{} 폰 스톰으로 폰 {}개 이동",
                crate::replay::label(actor),
                moves.len()
            ),
        )?;
    }
    let moved = moves.len();
    *state = next;
    Ok(moved)
}

fn pawn_direction(state: &GameState, actor: Color) -> i8 {
    let reversed = crate::observation::number(
        state
            .extra
            .get("effects")
            .and_then(|effects| effects.get("pawnReverse"))
            .and_then(|map| map.get(actor.as_str())),
    )
    .is_some_and(|value| value > 0.0);
    actor.pawn_dir() * if reversed { -1 } else { 1 }
}

fn can_pending_pawn_storm_advance(
    state: &GameState,
    piece: &Piece,
    from: Square,
    actor: Color,
) -> Result<bool> {
    if piece.color != actor
        || piece.kind != "pawn"
        || crate::movement::frozen(piece)
        || crate::observation::number(
            piece
                .extra
                .get("staked")
                .and_then(|status| status.get("remaining")),
        )
        .is_some_and(|value| value > 0.0)
    {
        return Ok(false);
    }
    let direction = if crate::v7_queued_effects::uses_september18_balance(state)? {
        pawn_direction(state, actor)
    } else {
        actor.pawn_dir()
    };
    let Some(to) = from.offset(direction, 0) else {
        return Ok(false);
    };
    crate::movement::open_relocation(state, to)
}

fn movement_privacy(state: &GameState, piece: &Piece, from: Square) -> Result<Value> {
    let mut privacy = serde_json::Map::new();
    for viewer in [Color::White, Color::Black] {
        let visible = crate::observation::piece_visible_to_color_at_v7(state, piece, from, viewer)?;
        privacy.insert(viewer.as_str().into(), json!({"originVisible":visible,
            "typeKnown":visible || piece.color == viewer || piece.extra.get("hiddenFrom").and_then(Value::as_str) == Some(viewer.as_str())}));
    }
    Ok(Value::Object(privacy))
}

fn movement_hidden_from(
    state: &GameState,
    piece: &Piece,
    destination: Square,
    privacy: &Value,
) -> Result<String> {
    let Some(owner) = piece.color.owner() else {
        return Ok(String::new());
    };
    let viewer = owner.opponent();
    Ok(
        if privacy[viewer.as_str()]["originVisible"] == json!(false)
            || !crate::observation::piece_visible_to_color_at_v7(state, piece, destination, viewer)?
        {
            viewer.as_str().into()
        } else {
            String::new()
        },
    )
}

/// Source main101374. Every physical monster acts once in row-major snapshot
/// order. Empty candidate sets still consume the source randomChoice draw.
pub(crate) fn move_rule_monsters(state: &mut GameState, moving_color: Color) -> Result<usize> {
    // main101375 only suppresses king-threat probes. Ordinary AI simulations
    // still move monsters; replay helpers separately suppress their notation
    // and visuals through is_ai_simulation(), without consuming event-id RNG.
    if state.threat_probe_depth > 0 {
        return Ok(0);
    }
    let mut seen = BTreeSet::new();
    let mut monsters = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if let Some(piece) = state.at(square).filter(|piece| piece.kind == "monster")
                && seen.insert(piece.id.clone())
            {
                monsters.push((square, piece.clone()));
            }
        }
    }
    if monsters.is_empty() {
        return Ok(0);
    }
    let mut next = state.clone();
    let mut transitions = Vec::new();
    let mut removed = Vec::new();
    for (from, mut monster) in monsters {
        if next.at(from).is_none_or(|piece| piece.id != monster.id)
            || crate::observation::number(monster.extra.get("poisonStunTurns"))
                .is_some_and(|turns| turns > 0.0)
        {
            continue;
        }
        let mut candidates = Vec::new();
        for dr in -1..=1 {
            for dc in -1..=1 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let Some(to) = from.offset(dr, dc) else {
                    continue;
                };
                if crate::movement::collapsed(&next, to)
                    || crate::v7_queued_effects::crown_ground_at(&next, to)
                    || movement_reserved(&next, to)
                {
                    continue;
                }
                if let Some(target) = next.at(to) {
                    if high_ground(&next, to) && !high_ground(&next, from)
                        || crate::v7_threat::uses_september22_rules(&next)
                            && crate::observation::truth(target.extra.get("outpostProtected"))
                        || crate::movement::v7_uses_revolving_door_guard(&next)
                            && target.ability_kind() == "revolvingDoor"
                        || crate::observation::truth(target.extra.get("protected"))
                        || crate::observation::number(
                            target
                                .extra
                                .get("vigilanceProtection")
                                .and_then(|status| status.get("remaining")),
                        )
                        .is_some_and(|remaining| remaining > 0.0)
                        || crate::movement::v7_encouraged_at(&next, target, to)
                    {
                        continue;
                    }
                    let allowed = if let Some(owner) = monster.color.owner() {
                        target.color.owner().is_some()
                            && target.color != owner
                            && !crate::observation::truth(target.extra.get("submerged"))
                            && (!crate::v7_threat::uses_september22_rules(&next)
                                || !crate::observation::truth(target.extra.get("outpostProtected")))
                            && !matches!(
                                target.kind.as_str(),
                                "wall"
                                    | "football"
                                    | "monster"
                                    | "blackHole"
                                    | "black-hole"
                                    | "darkWizard"
                                    | "dark-wizard"
                                    | "scarecrow"
                            )
                    } else {
                        crate::movement::v7_can_capture_target_automatic(
                            &next, &monster, target, true,
                        )?
                    };
                    if !allowed {
                        continue;
                    }
                }
                candidates.push(to);
            }
        }
        if candidates.is_empty() {
            next.rng
                .sample_invariant("source empty Monster destination")?;
            continue;
        }
        let choice = crate::transition::sample_choice(&mut next, candidates.len())?;
        let to = candidates[choice];
        let privacy = movement_privacy(&next, &monster, from)?;
        next.board[usize::from(from.row)][usize::from(from.col)] = None;
        let captured = next.at(to).cloned();
        let captured_privacy = captured
            .as_ref()
            .map(|piece| movement_privacy(&next, piece, to))
            .transpose()?;
        if let Some(piece) = &captured {
            crate::transition::remove_piece_from_board_cells(&mut next, piece, to)?;
            crate::transition::grant_vigilance_protection(&mut next, piece)?;
            crate::transition::cancel_prophecies_by_capture(&mut next)?;
            let owner = piece
                .color
                .owner()
                .ok_or_else(|| {
                    EngineError::InvalidState("v7 monster captured a non-player piece".into())
                })?
                .opponent();
            next.captures.get_mut(owner).push(piece.clone());
            crate::v7_piece_lifecycle::schedule_undead_resurrection(&mut next, piece, owner, true)?;
            crate::v7_capture_reactions::grant_wizard_mana(&mut next, piece.color, 1)?;
            removed.push(crate::v7_board_hazards::EnvironmentalRemoval {
                piece: piece.clone(),
                square: to,
                capture_owner: owner,
            });
            if crate::observation::truth(piece.extra.get("poisonedPawn")) {
                let turns = crate::observation::number(monster.extra.get("poisonStunTurns"))
                    .unwrap_or(0.0)
                    .max(3.0);
                monster
                    .extra
                    .insert("poisonStunTurns".into(), js_number(turns));
                monster
                    .extra
                    .insert("poisonStunColor".into(), json!(moving_color));
            }
        }
        monster.moved = true;
        crate::card_effects::note_ultimatum_movement(&mut next, &mut monster)?;
        next.board[usize::from(to.row)][usize::from(to.col)] = Some(monster.clone());
        crate::card_effects::mark_animation(&mut next, &monster)?;
        add_auto_move_highlight(&mut next, from, to, "", None, "")?;
        let concealed = if let Some(piece) = &captured {
            !movement_hidden_from(
                &next,
                piece,
                to,
                captured_privacy.as_ref().unwrap_or(&Value::Null),
            )?
            .is_empty()
        } else {
            false
        };
        let game_end = captured
            .as_ref()
            .map(|piece| {
                crate::v7_board_hazards::source_royal_identity(&next, piece)
                    .map(|royal| royal || piece.kind == "vip")
            })
            .transpose()?
            .unwrap_or(false);
        let description = format!(
            "괴물 {}에서 {}{}",
            square_name(from),
            square_name(to),
            if captured.is_some() {
                " 포획"
            } else {
                " 자동 이동"
            }
        );
        crate::replay::queue_automatic_move_notation_with_options(
            &mut next,
            &monster,
            from,
            to,
            crate::replay::AutomaticMoveNotationOptions {
                privacy: Some(&privacy),
                notation_color: moving_color,
                piece_type: "monster",
                capture: captured.is_some(),
                game_end,
                description: &description,
            },
        )?;
        transitions.push(json!({"from":from,"to":to,
            "item":{"id":monster.id,"color":monster.color,"type":"monster"},
            "captured":if concealed {Value::Null} else { captured.as_ref().map_or(Value::Null,|piece|json!({"id":piece.id,"color":piece.color,"type":piece.kind})) }
        }));
        let message = if fog_log_redaction(&next) || concealed {
            "괴물이 이동했습니다.".into()
        } else {
            format!(
                "괴물: {} -> {}{}",
                square_name(from),
                square_name(to),
                captured.as_ref().map_or(String::new(), |piece| format!(
                    ", {} 포획",
                    crate::replay::source_piece_label(&piece.kind).unwrap_or(&piece.kind)
                ))
            )
        };
        crate::replay::add_log(&mut next, message)?;
    }
    if !transitions.is_empty() {
        crate::replay::queue_visual(
            &mut next,
            json!({"type":"monster-move","color":"neutral","transitions":transitions}),
        )?;
    }
    if !removed.is_empty() {
        crate::flow::mark_progress(&mut next);
        for entry in &removed {
            crate::v7_board_hazards::resolve_reaper_nearby_deaths(
                &mut next,
                std::slice::from_ref(entry),
            )?;
        }
        crate::v7_board_hazards::resolve_environmental_defeats(&mut next, &removed, "괴물", true)?;
        crate::v7_capture_objectives::check_campaign_objectives(&mut next)?;
    }
    let moved = transitions.len();
    *state = next;
    Ok(moved)
}

fn movement_reserved(state: &GameState, square: Square) -> bool {
    let at = |entry: &Value| {
        entry.get("row").and_then(Value::as_u64) == Some(square.row.into())
            && entry.get("col").and_then(Value::as_u64) == Some(square.col.into())
    };
    ["pendingScarecrows", "pendingLobsters"]
        .into_iter()
        .any(|field| {
            state
                .extra
                .get(field)
                .and_then(Value::as_array)
                .is_some_and(|entries| {
                    entries.iter().any(|entry| {
                        (field != "pendingScarecrows"
                            || !crate::observation::truth(entry.get("pieceId")))
                            && at(entry)
                    })
                })
        })
        || state
            .extra
            .get("pendingPortals")
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry.get("blocksMovement") == Some(&json!(true))
                        && entry
                            .get("cells")
                            .and_then(Value::as_array)
                            .is_some_and(|cells| cells.iter().any(at))
                })
            })
}

fn high_ground(state: &GameState, square: Square) -> bool {
    state
        .extra
        .get("highGround")
        .and_then(Value::as_array)
        .is_some_and(|cells| {
            cells.iter().any(|cell| {
                cell.get("row").and_then(Value::as_u64) == Some(square.row.into())
                    && cell.get("col").and_then(Value::as_u64) == Some(square.col.into())
            })
        })
}

fn fog_log_redaction(state: &GameState) -> bool {
    matches!(
        state
            .extra
            .get("campaign")
            .and_then(|campaign| campaign.get("setup"))
            .and_then(Value::as_str),
        Some("fogWar" | "fog")
    ) || ["fogWar", "fogOfWar"]
        .into_iter()
        .any(|field| state.extra.get(field).is_some_and(js_truthy))
        || crate::observation::truth(state.extra.get("fog").and_then(|fog| fog.get("enabled")))
}

fn add_auto_move_highlight(
    state: &mut GameState,
    from: Square,
    to: Square,
    sound: &str,
    color: Option<Color>,
    hidden: &str,
) -> Result<()> {
    if let Some(last) = state
        .extra
        .get_mut("lastMove")
        .filter(|value| js_truthy(value))
    {
        let last = last.as_object_mut().ok_or_else(|| {
            EngineError::InvalidState("v7 automatic lastMove must be an object".into())
        })?;
        if !last.get("logMoves").is_some_and(Value::is_array) {
            last.insert("logMoves".into(), json!([]));
        }
        last.get_mut("logMoves").and_then(Value::as_array_mut).expect("normalized log traces")
            .push(json!({"from":from,"to":to,"hiddenFrom":if matches!(hidden,"white"|"black") {hidden} else {""}}));
        return Ok(());
    }
    let bookkeeping_color = color.unwrap_or(state.turn);
    crate::card_effects::set_last_move(state, from, to, sound, bookkeeping_color, hidden, None)?;
    if color.is_none() {
        state
            .extra
            .get_mut("lastMove")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| EngineError::InvalidState("v7 automatic lastMove missing".into()))?
            .insert("soundColor".into(), json!(""));
    }
    Ok(())
}

// Source main108481. These are automatic relocations, not ordinary actions:
// no move counter, completed turn, movement RNG or promotion is introduced.
fn auto_advance_logs(state: &mut GameState, actor: Color) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut logs = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let from = Square { row, col };
            if let Some(piece) = state.at(from).filter(|piece| {
                piece.color == actor
                    && piece.ability_kind() == "log"
                    && crate::observation::truth(piece.extra.get("logDir"))
            }) && seen.insert(piece.id.clone())
            {
                logs.push((from, piece.id.clone()));
            }
        }
    }
    if logs.is_empty() {
        return Ok(());
    }
    let mut next = state.clone();
    for (from, id) in logs {
        if next.mode == "gameover" {
            break;
        }
        let Some(mut log) = next.at(from).cloned().filter(|piece| piece.id == id) else {
            continue;
        };
        let turns = f64::from(*next.turns_taken.get(actor));
        if crate::observation::number(log.extra.get("logRollAfterTurn"))
            .filter(|value| {
                value.is_finite() && value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_991.0
            })
            .is_some_and(|deadline| turns < deadline)
        {
            continue;
        }
        log.extra
            .insert("logRollAfterTurn".into(), js_number(turns + 1.0));
        replace_queued_piece(&mut next, &log);
        let direction = log
            .extra
            .get("logDir")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                EngineError::InvalidState("v7 automatic logDir must be an object".into())
            })?;
        let coordinate = |field: &str| {
            direction
                .get(field)
                .and_then(Value::as_i64)
                .filter(|value| (-1..=1).contains(value))
                .map(|value| value as i8)
        };
        let (Some(dr), Some(dc)) = (coordinate("dr"), coordinate("dc")) else {
            return Err(EngineError::InvalidState(
                "v7 automatic logDir must contain adjacent integral dr/dc".into(),
            ));
        };
        if dr == 0 && dc == 0 {
            return Err(EngineError::InvalidState(
                "v7 automatic logDir cannot remain on its square".into(),
            ));
        }
        let privacy = movement_privacy(&next, &log, from)?;
        let Some(to) = from
            .offset(dr, dc)
            .filter(|to| !crate::movement::collapsed(&next, *to))
        else {
            stop_automatic_log(&mut next, &mut log)?;
            replace_queued_piece(&mut next, &log);
            continue;
        };
        if !crate::v7_rule_geometry::chain_destination_allowed(&next, &log, to)? {
            continue;
        }
        let target = next.at(to).cloned();
        if let Some(target) = &target {
            if target.color == log.color
                || crate::movement::v7_initiative_capture_locked(&next, log.color)
                || !crate::movement::v7_can_capture_target_automatic(&next, &log, target, true)?
                || source_protected(target)
                || crate::movement::v7_encouraged_at(&next, target, to)
            {
                stop_automatic_log(&mut next, &mut log)?;
                replace_queued_piece(&mut next, &log);
                continue;
            }
            let manner = crate::movement::v7_manner_capture_locked(&next, &log);
            if manner || crate::v7_capture_reactions::saturation_locked(&next, &log) {
                stop_automatic_log(&mut next, &mut log)?;
                replace_queued_piece(&mut next, &log);
                crate::replay::add_piece_action_log(
                    &mut next,
                    &log,
                    Some(from),
                    Some(&privacy),
                    format!(
                        "{}의 통나무가 {}로 멈췄습니다.",
                        square_name(from),
                        if manner { "매너 효과" } else { "포화" }
                    ),
                )?;
                continue;
            }
            if crate::v7_capture_reactions::is_hp_piece(&next, target) {
                let old_move = next.extra.get("lastMove").cloned().unwrap_or(Value::Null);
                let old_trail = next
                    .extra
                    .get("accelerationTrail")
                    .cloned()
                    .unwrap_or(Value::Null);
                let options = crate::v7_capture_reactions::CaptureOptions {
                    threat_probe: next.threat_probe_depth > 0,
                    ..Default::default()
                };
                let removed =
                    crate::v7_capture_reactions::damage_health_piece_with_optional_attacker(
                        &mut next,
                        to,
                        actor,
                        Some(&mut log),
                        "통나무 충돌",
                        &options,
                    )?
                    .is_some();
                log.extra
                    .insert("coolGuyCapturedLast".into(), json!(removed));
                next.extra.insert("lastMove".into(), old_move);
                next.extra.insert("accelerationTrail".into(), old_trail);
                stop_automatic_log(&mut next, &mut log)?;
                replace_queued_piece(&mut next, &log);
                continue;
            }
        }
        let captured = if target.is_some() {
            let options = crate::v7_capture_reactions::CaptureOptions {
                attacker_landing: Some(to),
                threat_probe: next.threat_probe_depth > 0,
                ..Default::default()
            };
            crate::v7_capture_reactions::capture_at_with_optional_attacker(
                &mut next,
                to,
                actor,
                Some(&mut log),
                &options,
            )?
        } else {
            None
        };
        next.board[usize::from(from.row)][usize::from(from.col)] = None;
        log.moved = true;
        if target.is_none() {
            crate::card_effects::note_ultimatum_movement(&mut next, &mut log)?;
        }
        next.board[usize::from(to.row)][usize::from(to.col)] = Some(log.clone());
        let hidden = movement_hidden_from(&next, &log, to, &privacy)?;
        let sound = if captured.is_some() {
            "capture"
        } else if actor == Color::White {
            "moveSelf"
        } else {
            "moveOpponent"
        };
        add_auto_move_highlight(&mut next, from, to, sound, Some(actor), &hidden)?;
        log.extra
            .insert("coolGuyCapturedLast".into(), json!(captured.is_some()));
        replace_queued_piece(&mut next, &log);
        let message = if fog_log_redaction(&next) {
            format!("{} 기물이 이동했습니다.", crate::replay::label(actor))
        } else if !hidden.is_empty() {
            "기물이 움직였습니다.".into()
        } else if let Some(captured) = &captured {
            format!(
                "{}의 통나무가 {}을 밀어냈습니다.",
                square_name(from),
                crate::replay::source_piece_label(&captured.kind).unwrap_or("undefined")
            )
        } else {
            format!(
                "{}의 통나무가 {}로 굴러갔습니다.",
                square_name(from),
                square_name(to)
            )
        };
        crate::replay::add_log(&mut next, message)?;
        if target.is_some() {
            crate::card_effects::note_ultimatum_movement(&mut next, &mut log)?;
            replace_queued_piece(&mut next, &log);
        }
        if captured
            .as_ref()
            .is_some_and(|piece| crate::observation::truth(piece.extra.get("explosive")))
        {
            crate::v7_board_hazards::explode_at(&mut next, to, "자폭병")?;
        }
    }
    *state = next;
    Ok(())
}

fn stop_automatic_log(state: &mut GameState, log: &mut Piece) -> Result<()> {
    log.extra.insert("logDir".into(), Value::Null);
    log.extra.shift_remove("logRollAfterTurn");
    if log.kind == "trickster" && log.ability_kind() == "log" {
        crate::card_effects::reroll_trickster_ability(state, log)?;
    }
    Ok(())
}

/// Settle the source's callbacks after `moveCount++`, pending otherworld,
/// and both conscription checks, but before `forceFirstMoveCardsAfterMove`.
/// The host calls this only after a `Continue` pre-count result in the same
/// transaction. Any terminal result must stop the remaining turn pipeline.
pub(crate) fn settle_end_move_after_count(
    state: &mut GameState,
    actor: Color,
) -> Result<V7FlowControl> {
    crate::legal_profile::measure("settle_end_move_after_count", || {
        settle_end_move_after_count_profiled(state, actor)
    })
}

pub(crate) fn settle_end_move_after_count_profiled(
    state: &mut GameState,
    actor: Color,
) -> Result<V7FlowControl> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 counted endMove on rules version {}",
            state.ruleset_id
        )));
    }
    if state.mode == "gameover" {
        return Ok(V7FlowControl::Terminal);
    }
    // This outgoing group still uses the original movingColor even when an
    // internal promotion has changed the live turn color.
    if state.mode != "play" {
        return Err(EngineError::WrongActor);
    }

    if let Some(effects) = state
        .extra
        .get_mut("effects")
        .and_then(Value::as_object_mut)
    {
        if effects.get("pawnQueen").and_then(Value::as_str) == Some(actor.as_str()) {
            effects.insert("pawnQueen".into(), Value::Null);
        }
        if let Some(reverse) = effects.get_mut("pawnReverse") {
            let map = reverse.as_object_mut().ok_or_else(|| {
                EngineError::InvalidState("v7 effects.pawnReverse must be a color map".into())
            })?;
            if let Some(number) = map.get(actor.as_str()).and_then(Value::as_f64)
                && number > 0.0
            {
                map.insert(actor.as_str().into(), js_number(number - 1.0));
            }
        }
    }
    for field in ["taunt", "socialism"] {
        let Some(value) = state.extra.get_mut(field) else {
            continue;
        };
        let map = value
            .as_object_mut()
            .ok_or_else(|| EngineError::InvalidState(format!("v7 {field} must be a color map")))?;
        if let Some(number) = map.get(actor.as_str()).and_then(Value::as_f64)
            && number > 0.0
        {
            map.insert(actor.as_str().into(), js_number(number - 1.0));
        }
    }

    // The source applies each status class to all piece identities before
    // starting the next class; in particular, a staked expiry is logged before
    // the last-resistance protection tick.
    resolve_witch_trials(state, actor)?;
    tick_actor_status(state, actor, "disarmed")?;
    tick_actor_status(state, actor, "staked")?;
    crate::threat::tick_protection(state, actor, "lastResistance");
    tick_severed_pieces(state, actor)?;
    tick_actor_status(state, actor, "iceSheet")?;

    // Source `checkReligiousVictory` follows every status tick and runs
    // before one-turn cleanup or first-move automatic cards.
    if crate::v7_passive_terminal::after_end_move_status(state)? {
        return Ok(V7FlowControl::Terminal);
    }
    for_each_unique_piece(state, |piece, _| {
        if piece.color == actor {
            for field in [
                "repositionSecondMove",
                "frenzy",
                "frenzyExtraMove",
                "rookLiftChain",
            ] {
                piece.extra.shift_remove(field);
            }
        }
        Ok(())
    })?;
    crate::replay::normalize_color_booleans(state, "zugzwang");
    state
        .extra
        .get_mut("zugzwang")
        .expect("normalized zugzwang colors")[actor.as_str()] = json!(false);
    for field in [
        "freeCastling",
        "switcheroo",
        "relay",
        "bishopSnipe",
        "breakthroughPawns",
        "enPassantFrenzy",
    ] {
        let Some(value) = state.extra.get(field) else {
            continue;
        };
        if matches!(field, "switcheroo" | "relay" | "enPassantFrenzy") && !js_truthy(value) {
            continue;
        }
        state.set_flag(field, actor, false);
    }
    Ok(V7FlowControl::Continue)
}

fn resolve_wanted_arrests(state: &mut GameState, actor: Color) -> Result<()> {
    let mut due = Vec::new();
    for_each_unique_piece(state, |piece, square| {
        if piece.color == actor
            && piece.extra.get("wanted").is_some_and(js_truthy)
            && !piece.extra.get("submerged").is_some_and(js_truthy)
        {
            due.push((square, piece.clone()));
        }
        Ok(())
    })?;
    let mut removals = Vec::new();
    for (square, piece) in due {
        if state.at(square).is_none_or(|item| item.id != piece.id) {
            continue;
        }
        let threat_source = json!({"label":"수배"});
        let options = crate::transition::ForceRemovalOptions {
            threat_source: Some(&threat_source),
            ..Default::default()
        };
        if crate::transition::force_remove_piece_at_with_options(
            state,
            square,
            actor.opponent(),
            &options,
        )?
        .is_some()
        {
            removals.push(json!({"square":square,"color":piece.color,"pieceType":piece.kind}));
        }
    }
    if !removals.is_empty() {
        crate::replay::queue_visual(
            state,
            json!({"type":"board-change","effect":"thief-arrest","color":actor,
                "removals":removals,"relocations":[],"transformations":[],"spawns":[]}),
        )?;
    }
    Ok(())
}

fn resolve_witch_trials(state: &mut GameState, actor: Color) -> Result<()> {
    let snapshot = state.clone();
    let mut removals = Vec::new();
    for_each_unique_piece(state, |piece, square| {
        if !piece.extra.get("witchTrial").is_some_and(js_truthy)
            || !witch_trial_counts_on(piece, actor, &snapshot)?
        {
            return Ok(());
        }
        let next = remaining(piece, "witchTrial")? - 1.0;
        let trial = piece
            .extra
            .get_mut("witchTrial")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| EngineError::InvalidState("v7 witchTrial must be an object".into()))?;
        trial.insert("remaining".into(), js_number(next));
        if next <= 0.0 {
            let capturer = trial.get("by").filter(|value| js_truthy(value)).map_or(
                Ok(actor.opponent()),
                |value| match value.as_str() {
                    Some("white") => Ok(Color::White),
                    Some("black") => Ok(Color::Black),
                    _ => Err(EngineError::InvalidState(
                        "v7 witchTrial.by must name a player".into(),
                    )),
                },
            )?;
            removals.push((square, capturer, piece.kind.clone()));
        }
        Ok(())
    })?;
    for (square, capturer, kind) in removals {
        let threat_source = json!({"label":"마녀재판"});
        let options = crate::transition::ForceRemovalOptions {
            count_as_capture: true,
            threat_source: Some(&threat_source),
            ..Default::default()
        };
        if let Some(removed) = crate::transition::force_remove_piece_at_with_options(
            state, square, capturer, &options,
        )? {
            crate::v7_capture_reactions::record_new_card_capture_reactions(
                state, &removed, capturer, false,
            )?;
            crate::replay::add_piece_action_log(
                state,
                &removed,
                Some(square),
                None,
                format!(
                    "마녀재판: {}이 제거되었습니다.",
                    crate::replay::source_piece_label(&kind).unwrap_or("기물")
                ),
            )?;
        }
    }
    Ok(())
}

fn tick_severed_pieces(state: &mut GameState, actor: Color) -> Result<()> {
    let full_move = f64::from(state.full_move);
    for_each_unique_piece(state, |piece, _| {
        if piece.color != actor || !piece.extra.get("severed").is_some_and(js_truthy) {
            return Ok(());
        }
        let severed = piece.extra.get("severed");
        if crate::observation::number(severed.and_then(|value| value.get("remaining"))).is_some() {
            tick_piece_counter(piece, "severed")?;
        } else {
            // A legacy full-move expiry is checked here, without decrementing
            // it; cleanupFullMoveEffects checks it again after Black advances.
            let expires =
                crate::observation::number(severed.and_then(|value| value.get("expiresFullMove")));
            if expires.is_none_or(|expires| (expires - full_move).max(0.0) <= 0.0) {
                piece.extra.shift_remove("severed");
            }
        }
        Ok(())
    })
}

fn tick_actor_status(state: &mut GameState, actor: Color, field: &str) -> Result<()> {
    let mut staked_promotions = Vec::new();
    for_each_unique_piece(state, |piece, square| {
        if piece.color == actor && piece.extra.get(field).is_some_and(js_truthy) {
            let expired = tick_piece_counter(piece, field)?;
            if field == "staked" && expired {
                staked_promotions.push((piece.clone(), square));
            }
        }
        Ok(())
    })?;
    for (piece, square) in staked_promotions {
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
                crate::replay::source_piece_label(&piece.kind).unwrap_or("기물")
            ),
        )?;
    }
    Ok(())
}

// Source 99026-99065: the encore is one use per actor turn. The moved piece
// temporarily rests before hasAnyLegalMove is evaluated; an unproved legal
// move verdict stays an explicit error instead of granting an extra turn.
fn consume_idol_encore(state: &mut GameState, actor: Color) -> Result<bool> {
    let Some(last_move) = state.extra.get("lastMove").cloned() else {
        return Ok(false);
    };
    if !last_move.get("idolEncoreEligible").is_some_and(js_truthy)
        || last_move.get("idolEncoreConsumed").is_some_and(js_truthy)
        || last_move.get("soundColor").and_then(Value::as_str) != Some(actor.as_str())
    {
        return Ok(false);
    }
    let idol_id = last_move
        .get("idolEncoreId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let turn_number = f64::from(*state.turns_taken.get(actor));
    let used_key = format!("turn:{}", actor.as_str());
    let already_used = crate::observation::number(
        state
            .extra
            .get("idolEncoreUsedByPiece")
            .and_then(|used| used.get(&used_key)),
    ) == Some(turn_number);
    let cancel = |state: &mut GameState| -> Result<bool> {
        state
            .extra
            .get_mut("lastMove")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| EngineError::InvalidState("v7 lastMove must be an object".into()))?
            .insert("idolEncoreEligible".into(), json!(false));
        Ok(false)
    };
    if idol_id.is_empty() || already_used {
        return cancel(state);
    }
    let idol_piece = queued_piece_by_id(state, idol_id)?.map(|(_, piece)| piece);
    let moved_id = last_move
        .get("idolEncorePieceId")
        .filter(|value| js_truthy(value))
        .or_else(|| last_move.get("pieceId"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut moved_piece = queued_piece_by_id(state, moved_id)?.map(|(_, piece)| piece);
    if !idol_piece
        .as_ref()
        .is_some_and(|piece| piece.ability_kind() == "idol" && piece.color == actor)
        || moved_piece.is_none() && !crate::v7_queued_effects::uses_september18_balance(state)?
        || moved_piece
            .as_ref()
            .is_some_and(|piece| piece.color != actor || piece.ability_kind() == "idol")
    {
        return cancel(state);
    }
    if let Some(piece) = moved_piece.as_mut() {
        piece.extra.insert(
            "idolEncoreRestTurn".into(),
            json!(*state.turns_taken.get(actor)),
        );
        replace_queued_piece(state, piece);
    }
    if !crate::movement::v7_has_any_legal_move(state, actor)? {
        if let Some(piece) = moved_piece.as_mut() {
            piece.extra.shift_remove("idolEncoreRestTurn");
            replace_queued_piece(state, piece);
        }
        return cancel(state);
    }
    let used = state
        .extra
        .entry("idolEncoreUsedByPiece")
        .or_insert_with(|| json!({}));
    if !js_truthy(used) {
        *used = json!({});
    }
    used.as_object_mut()
        .ok_or_else(|| {
            EngineError::InvalidState("v7 idolEncoreUsedByPiece must be an object".into())
        })?
        .insert(used_key, json!(*state.turns_taken.get(actor)));
    state
        .extra
        .get_mut("lastMove")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState("v7 lastMove must be an object".into()))?
        .insert("idolEncoreConsumed".into(), json!(true));
    state.extra.insert("selected".into(), Value::Null);
    state.extra.insert("legalMoves".into(), json!([]));
    Ok(true)
}

// Source 1302-1315 consumes Resolve before time-distortion/acceleration and
// marks only the credited pawn as resting in the rebalance profile.
fn consume_resolve_credit(state: &mut GameState, actor: Color) -> Result<bool> {
    if !state
        .extra
        .get("resolveMoveCredit")
        .and_then(|value| value.get(actor.as_str()))
        .is_some_and(js_truthy)
    {
        return Ok(false);
    }
    let rebalance = crate::v7_queued_effects::uses_september26_rebalance(state)?;
    let id = state
        .extra
        .get("resolveCreditPieceId")
        .and_then(|value| value.get(actor.as_str()))
        .and_then(Value::as_str)
        .map(str::to_owned);
    state
        .extra
        .get_mut("resolveMoveCredit")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            EngineError::InvalidState("v7 resolveMoveCredit must be a color map".into())
        })?
        .insert(actor.as_str().into(), json!(false));
    if rebalance {
        if let Some(id) = id
            && let Some((_, mut piece)) = queued_piece_by_id(state, &id)?
        {
            piece.extra.insert(
                "idolEncoreRestTurn".into(),
                json!(*state.turns_taken.get(actor)),
            );
            replace_queued_piece(state, &piece);
        }
        if let Some(credits) = state
            .extra
            .get_mut("resolveCreditPieceId")
            .and_then(Value::as_object_mut)
        {
            credits.shift_remove(actor.as_str());
        }
    }
    state.en_passant = None;
    Ok(true)
}

fn retain_time_stop(state: &mut GameState, actor: Color) -> Result<bool> {
    crate::replay::normalize_color_booleans(state, "skipTurn");
    let stopped = actor.opponent();
    if !state.extra["skipTurn"][stopped.as_str()]
        .as_bool()
        .unwrap_or(false)
    {
        return Ok(false);
    }
    state.extra["skipTurn"][stopped.as_str()] = json!(false);
    state.turn = actor;
    state.en_passant = None;
    state.actions_remaining = if state.extra.get("acceleration").is_some_and(js_truthy) {
        2
    } else {
        1
    };
    crate::replay::add_log(
        state,
        format!(
            "{}의 시간 정지 이동은 같은 턴에 이어집니다.",
            crate::replay::label(actor),
        ),
    )?;
    Ok(true)
}

/// Source 74183: run after first-move cards and before completeTurnAfterMove,
/// only for a counted action. Both colors must have acted to consume a round.
pub(crate) fn tick_armistice_after_action(state: &mut GameState, actor: Color) -> Result<bool> {
    let original = state.extra.get("armistice");
    let value = original
        .and_then(|value| value.get("remaining"))
        .filter(|value| !value.is_null())
        .or(original);
    let mut remaining = crate::observation::number(value)
        .unwrap_or(0.0)
        .floor()
        .max(0.0);
    if remaining == 0.0 {
        state.extra.insert("armistice".into(), Value::Null);
        return Ok(false);
    }
    let by = original
        .and_then(|value| value.get("by"))
        .and_then(Value::as_str)
        .filter(|color| matches!(*color, "white" | "black"))
        .map(str::to_owned);
    let mut acted = Vec::<String>::new();
    for color in original
        .and_then(|value| value.get("actedColors"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|color| matches!(*color, "white" | "black"))
    {
        if !acted.iter().any(|existing| existing == color) {
            acted.push(color.into());
        }
    }
    if !acted.iter().any(|color| color == actor.as_str()) {
        acted.push(actor.as_str().into());
    }
    if acted.len() == 2 {
        remaining -= 1.0;
        acted.clear();
    }
    if remaining > 0.0 {
        state.extra.insert(
            "armistice".into(),
            json!({"remaining":js_number(remaining),"by":by,"actedColors":acted}),
        );
    } else {
        state.extra.insert("armistice".into(), Value::Null);
        crate::replay::add_log(state, "휴전이 끝났습니다.".into())?;
    }
    Ok(true)
}

// Source 109121-109135: the queue counts down even if its former subject has
// disappeared. Expiry transforms only an amazon which still has `bribed`.
fn tick_temporary_queens(state: &mut GameState, actor: Color) -> Result<()> {
    let Some(entries) = state
        .extra
        .get("temporaryQueens")
        .and_then(Value::as_array)
        .cloned()
    else {
        return Ok(());
    };
    let mut next = Vec::with_capacity(entries.len());
    for mut entry in entries {
        if entry.get("color").and_then(Value::as_str) != Some(actor.as_str()) {
            next.push(entry);
            continue;
        }
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let remaining = crate::observation::number(entry.get("remaining")).ok_or_else(|| {
            EngineError::InvalidState("v7 temporaryQueens.remaining must be a finite number".into())
        })? - 1.0;
        entry
            .as_object_mut()
            .ok_or_else(|| {
                EngineError::InvalidState("v7 temporaryQueens entry must be an object".into())
            })?
            .insert("remaining".into(), js_number(remaining));
        if let Some((square, mut piece)) = queued_piece_by_id(state, &id)? {
            piece
                .extra
                .insert("bribedRemaining".into(), js_number(remaining.max(0.0)));
            let expired = remaining <= 0.0
                && piece.kind == "amazon"
                && piece.extra.get("bribed").is_some_and(js_truthy);
            if expired {
                piece.kind = if state.extra.get("monochromeChess").is_some_and(js_truthy) {
                    "camel"
                } else {
                    "knight"
                }
                .into();
                piece.extra.insert("bribed".into(), json!(false));
                piece.extra.insert("bribedRemaining".into(), Value::Null);
            }
            replace_queued_piece(state, &piece);
            if expired {
                crate::replay::add_piece_action_log(
                    state,
                    &piece,
                    Some(square),
                    None,
                    format!(
                        "{}{}의 삼일천하가 끝나 {}로 돌아왔습니다.",
                        char::from(b'a' + square.col),
                        8 - square.row,
                        crate::replay::source_piece_label(&piece.kind).unwrap_or("undefined"),
                    ),
                )?;
            }
        }
        if remaining > 0.0 {
            next.push(entry);
        }
    }
    state
        .extra
        .insert("temporaryQueens".into(), Value::Array(next));
    Ok(())
}

// Source 109138-109163: a changed or missing subject invalidates the queue;
// only a still-revived subject has its lifetime counted and becomes a pawn.
fn tick_necromancy(state: &mut GameState, actor: Color) -> Result<()> {
    let entries = state
        .extra
        .get("necromancy")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut next = Vec::with_capacity(entries.len());
    for mut entry in entries {
        if entry.get("color").and_then(Value::as_str) != Some(actor.as_str()) {
            next.push(entry);
            continue;
        }
        let id = entry.get("id").and_then(Value::as_str).unwrap_or_default();
        let Some((square, mut piece)) = queued_piece_by_id(state, id)? else {
            continue;
        };
        let revived_type = entry
            .get("revivedType")
            .filter(|value| js_truthy(value))
            .or_else(|| {
                piece
                    .extra
                    .get("necromancy")
                    .and_then(|value| value.get("revivedType"))
            })
            .and_then(Value::as_str);
        let still_revived = piece.color == actor
            && piece.extra.get("necromancy").is_some_and(js_truthy)
            && revived_type == Some(piece.kind.as_str());
        if !still_revived {
            piece.extra.shift_remove("necromancy");
            piece.extra.shift_remove("necromancyRemaining");
            replace_queued_piece(state, &piece);
            continue;
        }
        let remaining = crate::observation::number(entry.get("remaining"))
            .unwrap_or(0.0)
            .max(0.0)
            - 1.0;
        entry
            .as_object_mut()
            .ok_or_else(|| {
                EngineError::InvalidState("v7 necromancy entry must be an object".into())
            })?
            .insert("remaining".into(), js_number(remaining));
        let displayed = js_number(remaining.max(0.0));
        piece
            .extra
            .insert("necromancyRemaining".into(), displayed.clone());
        piece
            .extra
            .get_mut("necromancy")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                EngineError::InvalidState("v7 active piece.necromancy must be an object".into())
            })?
            .insert("remaining".into(), displayed);
        if remaining > 0.0 {
            replace_queued_piece(state, &piece);
            next.push(entry);
            continue;
        }
        piece.kind = "pawn".into();
        piece.extra.shift_remove("necromancy");
        piece.extra.shift_remove("necromancyRemaining");
        replace_queued_piece(state, &piece);
        crate::card_effects::mark_animation(state, &piece)?;
        crate::replay::add_piece_action_log(
            state,
            &piece,
            Some(square),
            None,
            format!(
                "빙의: {}{}의 기물이 다시 폰으로 돌아왔습니다.",
                char::from(b'a' + square.col),
                8 - square.row,
            ),
        )?;
    }
    state.extra.insert("necromancy".into(), Value::Array(next));
    Ok(())
}

fn queued_piece_by_id(state: &GameState, id: &str) -> Result<Option<(Square, Piece)>> {
    for (row, cells) in state.board.iter().enumerate() {
        for (col, cell) in cells.iter().enumerate() {
            let Some(piece) = cell.as_ref().filter(|piece| piece.id == id) else {
                continue;
            };
            let mut square = Square {
                row: row as u8,
                col: col as u8,
            };
            if piece.is_large() {
                let anchor = |field: &str, fallback: u8| -> Result<u8> {
                    piece.extra.get(field).map_or(Ok(fallback), |value| {
                        value
                            .as_u64()
                            .and_then(|value| u8::try_from(value).ok())
                            .filter(|value| *value < 8)
                            .ok_or_else(|| {
                                EngineError::InvalidState(format!(
                                    "v7 queued subject.{field} must be a board index"
                                ))
                            })
                    })
                };
                square.row = anchor("anchorRow", square.row)?;
                square.col = anchor("anchorCol", square.col)?;
            }
            return Ok(Some((square, piece.clone())));
        }
    }
    Ok(None)
}

fn replace_queued_piece(state: &mut GameState, piece: &Piece) {
    for cell in state.board.iter_mut().flatten() {
        if cell
            .as_ref()
            .is_some_and(|existing| existing.id == piece.id)
        {
            *cell = Some(piece.clone());
        }
    }
}

fn has_adjacent_enemy(state: &GameState, square: Square, actor: Color) -> bool {
    crate::movement::KING
        .iter()
        .filter_map(|&(dr, dc)| square.offset(dr, dc))
        .any(|cell| {
            state
                .at(cell)
                .is_some_and(|piece| piece.color == actor.opponent())
        })
}

fn tick_piece_counter(piece: &mut Piece, field: &str) -> Result<bool> {
    let previous = remaining(piece, field)?;
    let next = previous - 1.0;
    if next <= 0.0 {
        piece.extra.shift_remove(field);
        if field == "staked" {
            piece.extra.insert("shielded".into(), json!(true));
            if piece
                .extra
                .get("potionEffects")
                .and_then(Value::as_array)
                .is_some_and(|effects| effects.contains(&json!("stake")))
            {
                crate::card_effects::note_potion_effect(piece, "shield")?;
            }
        }
    } else {
        piece
            .extra
            .get_mut(field)
            .and_then(Value::as_object_mut)
            .expect("preflight verified object")
            .insert("remaining".into(), js_number(next));
    }
    Ok(next <= 0.0)
}

fn remaining(piece: &Piece, field: &str) -> Result<f64> {
    let value = piece
        .extra
        .get(field)
        .and_then(Value::as_object)
        .and_then(|counter| crate::observation::number(counter.get("remaining")))
        .ok_or_else(|| EngineError::InvalidState(format!("v7 {field}.remaining must be finite")))?;
    if !value.is_finite() {
        return Err(EngineError::InvalidState(format!(
            "v7 {field}.remaining must be finite"
        )));
    }
    Ok(value)
}

fn js_number(value: f64) -> Value {
    if value.fract() == 0.0 && value >= i64::MIN as f64 && value < i64::MAX as f64 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

fn witch_trial_counts_on(piece: &Piece, actor: Color, state: &GameState) -> Result<bool> {
    let trial = piece
        .extra
        .get("witchTrial")
        .and_then(Value::as_object)
        .ok_or_else(|| EngineError::InvalidState("v7 witchTrial must be an object".into()))?;
    let count_by = trial.get("countBy").and_then(Value::as_str);
    let color = match count_by {
        Some("white") => Color::White,
        Some("black") => Color::Black,
        Some(other) => {
            return Err(EngineError::InvalidState(format!(
                "v7 witchTrial.countBy has unknown color {other}"
            )));
        }
        None => piece.color.owner().ok_or_else(|| {
            EngineError::InvalidState("v7 neutral witchTrial has no countBy".into())
        })?,
    };
    let count_on =
        if count_by.is_some() && crate::v7_queued_effects::uses_september18_balance(state)? {
            color.opponent()
        } else {
            color
        };
    Ok(count_on == actor)
}

fn for_each_unique_piece(
    state: &mut GameState,
    mut callback: impl FnMut(&mut Piece, Square) -> Result<()>,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut pieces = Vec::new();
    for (row, line) in state.board.iter().enumerate() {
        for (col, cell) in line.iter().enumerate() {
            if let Some(piece) = cell
                && seen.insert(piece.id.clone())
            {
                pieces.push((
                    Square {
                        row: row as u8,
                        col: col as u8,
                    },
                    piece.clone(),
                ));
            }
        }
    }
    for (origin, mut piece) in pieces {
        callback(&mut piece, origin)?;
        for cell in state.board.iter_mut().flatten() {
            if cell.as_ref().is_some_and(|current| current.id == piece.id) {
                *cell = Some(piece.clone());
            }
        }
    }
    Ok(())
}

fn clear_selection(state: &mut GameState) {
    for field in ["selected", "targeting", "wizardSpell"] {
        state.extra.insert(field.into(), Value::Null);
    }
    for field in ["legalMoves", "wizardPreview", "shotgunPreview"] {
        state.extra.insert(field.into(), json!([]));
    }
    state.extra.insert("shotgunAction".into(), json!("move"));
}

fn has_entries(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        other => js_truthy(other),
    }
}

fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn unsupported(field: &str, callback: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!(
        "v7 endMove requires unported {callback} for active {field}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;

    fn play_state() -> GameState {
        play_state_with_seed(19)
    }

    fn play_state_with_seed(seed: u64) -> GameState {
        let config = GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        };
        crate::v7_new_game::new_game(config, seed).unwrap()
    }

    #[test]
    fn malformed_recurrence_is_rejected_before_end_move_mutation() {
        let mut state = play_state();
        state
            .extra
            .insert("pendingRecurrences".into(), json!([{"id":"p"}]));
        let before = state.clone();
        let error = settle_end_move_before_count(&mut state, Color::White).unwrap_err();
        assert!(matches!(error, EngineError::InvalidState(_)));
        assert_eq!(state, before);
    }

    #[test]
    fn opposite_actor_bear_retaliation_remains_queued_without_rng() {
        let mut state = play_state();
        state.extra.insert(
            "pendingBearRetaliations".into(),
            json!([{"attackerId":"p","attackerColor":"black"}]),
        );
        let before_queue = state.extra["pendingBearRetaliations"].clone();
        let before_rng = state.rng.clone();
        assert_eq!(
            settle_end_move_before_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.extra["pendingBearRetaliations"], before_queue);
        assert_eq!(state.rng, before_rng);
    }

    #[test]
    fn nontriggering_rule_bomb_uses_the_source_normalizer() {
        let mut state = play_state();
        state.extra.insert(
            "ruleBombs".into(),
            json!([{"id":"probe","row":4,"col":0,"unrecognized":true}]),
        );
        assert_eq!(
            settle_end_move_before_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(
            state.extra["ruleBombs"],
            json!([{"id":"probe","row":4,"col":0}])
        );
    }

    #[test]
    fn democracy_defeat_is_checked_after_rule_bombs_before_the_move_count() {
        let mut state = play_state();
        state
            .extra
            .insert("democracy".into(), json!({"white":true,"black":false}));
        for cell in state.board.iter_mut().flatten() {
            if cell
                .as_ref()
                .is_some_and(|piece| piece.color == Color::White && piece.kind == "pawn")
            {
                *cell = None;
            }
        }
        let before_count = state.move_count;
        assert_eq!(
            settle_end_move_before_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal
        );
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner.as_deref(), Some("black"));
        assert_eq!(state.move_count, before_count);
    }

    #[test]
    fn same_turn_extra_move_consumes_only_its_credit() {
        let mut state = play_state();
        state.extra.insert("effects".into(), json!({"extraMove":2}));
        state
            .extra
            .insert("activeHistoryMoveNumber".into(), json!(7));
        let piece = state
            .board
            .iter_mut()
            .flatten()
            .flatten()
            .find(|piece| piece.color == Color::White)
            .unwrap();
        piece
            .extra
            .insert("disarmed".into(), json!({"remaining":2}));
        let id = piece.id.clone();
        let before_rng = state.rng.clone();
        let before_turns = state.turns_taken.clone();
        let before_moves = state.move_count;
        assert_eq!(
            settle_end_move_before_count(&mut state, Color::White).unwrap(),
            V7FlowControl::RetainTurn
        );
        assert_eq!(state.extra["effects"]["extraMove"], json!(1));
        assert_eq!(state.turns_taken, before_turns);
        assert_eq!(state.move_count, before_moves);
        let piece = state
            .board
            .iter()
            .flatten()
            .flatten()
            .find(|piece| piece.id == id)
            .unwrap();
        assert_eq!(piece.extra["disarmed"]["remaining"], json!(2));
        assert_eq!(state.rng, before_rng);
        assert_eq!(state.extra["activeHistoryMoveNumber"], json!(7));
    }

    #[test]
    fn retained_turn_context_exposes_capture_and_previous_history_key() {
        for previous in [None, Some(Value::Null), Some(json!(7))] {
            for capture_actor in [None, Some(Color::White), Some(Color::Black)] {
                let mut state = play_state();
                state.full_move = 3;
                state.extra.insert("effects".into(), json!({"extraMove":1}));
                match &previous {
                    Some(value) => {
                        state
                            .extra
                            .insert("activeHistoryMoveNumber".into(), value.clone());
                    }
                    None => {
                        state.extra.shift_remove("activeHistoryMoveNumber");
                    }
                }
                let capture = state.clone();
                state.active_move_replay_before =
                    capture_actor.map(|actor| crate::replay::MoveReplayCapture {
                        actor,
                        before: Box::new(capture.clone()),
                    });
                let mut context = V7EndMoveContext::default();
                assert_eq!(
                    settle_end_move_before_count_with_context(
                        &mut state,
                        Color::White,
                        &mut context,
                    )
                    .unwrap(),
                    V7FlowControl::RetainTurn
                );
                assert!(context.entered_history_scope);
                assert_eq!(context.previous_history_number, previous);
                assert_eq!(
                    context
                        .replay_capture
                        .as_ref()
                        .map(|capture| capture.before.as_ref()),
                    capture_actor.map(|_| &capture)
                );
                assert_eq!(
                    context.replay_capture.as_ref().map(|capture| capture.actor),
                    capture_actor
                );
                assert!(state.active_move_replay_before.is_none());
                assert_eq!(state.extra["activeHistoryMoveNumber"], json!(3));
                assert_eq!(state.move_count, capture.move_count);
                assert_eq!(state.turns_taken, capture.turns_taken);
            }
        }
    }

    #[test]
    fn disarmed_and_ice_sheet_tick_once_per_identity() {
        let mut state = play_state();
        state.turn = Color::Black;
        let piece = state
            .board
            .iter_mut()
            .flatten()
            .flatten()
            .find(|piece| piece.color == Color::White)
            .unwrap();
        piece
            .extra
            .insert("disarmed".into(), json!({"remaining":2}));
        piece
            .extra
            .insert("iceSheet".into(), json!({"remaining":1}));
        let id = piece.id.clone();
        let before_rng = state.rng.clone();
        assert_eq!(
            settle_end_move_before_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        let before_count = state
            .board
            .iter()
            .flatten()
            .flatten()
            .find(|piece| piece.id == id)
            .unwrap();
        assert_eq!(before_count.extra["disarmed"]["remaining"], json!(2));
        assert_eq!(before_count.extra["iceSheet"]["remaining"], json!(1));
        state.move_count += 1;
        assert_eq!(
            settle_end_move_after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        let after = state
            .board
            .iter()
            .flatten()
            .flatten()
            .find(|piece| piece.id == id)
            .unwrap();
        assert_eq!(after.extra["disarmed"]["remaining"], json!(1));
        assert!(after.extra.get("iceSheet").is_none());
        assert_eq!(state.rng, before_rng);
    }

    #[test]
    fn frozen_source_count_boundary_runs_metal_before_status_and_effect_expiry() {
        // Frozen source SHA e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c.
        // A direct headless `endMove("white")` receipt (seed 22, normal,
        // draftDelete) records the metalCooldown setter at moveCount 0 and
        // disarmed.remaining at moveCount 1. It ends with each value at 1,
        // pawnQueen null, pawnReverse/taunt/socialism at 1, and mode play.
        let mut state = play_state_with_seed(22);
        let metal = state.board[6][0].as_mut().unwrap();
        metal.extra.insert("metalized".into(), json!(true));
        metal.extra.insert("metalCooldown".into(), json!(2));
        let disarmed = state.board[6][1].as_mut().unwrap();
        disarmed
            .extra
            .insert("disarmed".into(), json!({"remaining":2}));
        let effects = state
            .extra
            .get_mut("effects")
            .unwrap()
            .as_object_mut()
            .unwrap();
        effects.insert("pawnQueen".into(), json!("white"));
        effects.insert("pawnReverse".into(), json!({"white":2,"black":0}));
        state
            .extra
            .insert("taunt".into(), json!({"white":2,"black":0}));
        state
            .extra
            .insert("socialism".into(), json!({"white":2,"black":0}));
        let initial_count = state.move_count;

        assert_eq!(
            settle_end_move_before_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.move_count, initial_count);
        assert_eq!(
            state.board[6][0].as_ref().unwrap().extra["metalCooldown"],
            json!(1)
        );
        assert_eq!(
            state.board[6][1].as_ref().unwrap().extra["disarmed"]["remaining"],
            json!(2)
        );
        assert_eq!(state.extra["effects"]["pawnQueen"], json!("white"));
        state.move_count += 1;
        assert_eq!(
            settle_end_move_after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(
            state.board[6][1].as_ref().unwrap().extra["disarmed"]["remaining"],
            json!(1)
        );
        assert_eq!(state.extra["effects"]["pawnQueen"], Value::Null);
        assert_eq!(state.extra["effects"]["pawnReverse"]["white"], json!(1));
        assert_eq!(state.extra["taunt"]["white"], json!(1));
        assert_eq!(state.extra["socialism"]["white"], json!(1));
        assert_eq!(state.mode, "play");
    }

    #[test]
    fn religious_victory_stops_before_completed_turn_cleanup() {
        let mut state = play_state();
        state.extra.insert(
            "religiousVictory".into(),
            json!({"white":true,"black":false}),
        );
        state
            .extra
            .insert("zugzwang".into(), json!({"white":true,"black":true}));
        let mut converted = 0;
        for piece in state.board.iter_mut().flatten().flatten() {
            if piece.color == Color::White && matches!(piece.kind.as_str(), "knight" | "rook") {
                piece.kind = "bishop".into();
                converted += 1;
                if converted == 3 {
                    break;
                }
            }
        }
        assert_eq!(converted, 3);
        assert_eq!(
            settle_end_move_before_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        state.move_count += 1;
        assert_eq!(
            settle_end_move_after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal
        );
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner.as_deref(), Some("white"));
        assert_eq!(state.extra["zugzwang"]["white"], json!(true));
    }

    #[test]
    fn due_conscription_reaches_the_counted_source_callback() {
        let mut state = play_state();
        state
            .extra
            .insert("conscription".into(), json!({"white":true,"black":false}));
        state.extra.insert(
            "conscriptionUsed".into(),
            json!({"white":false,"black":false}),
        );
        for cell in state.board.iter_mut().flatten() {
            if cell
                .as_ref()
                .is_some_and(|piece| piece.color == Color::White && piece.kind == "pawn")
            {
                *cell = None;
            }
        }
        assert_eq!(
            settle_end_move_before_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.extra["conscriptionUsed"]["white"], json!(false));
        state.move_count += 1;
        crate::v7_turn_entry::check_conscription(&mut state, Color::White).unwrap();
        assert_eq!(state.extra["conscriptionUsed"]["white"], json!(true));
        assert_eq!(
            state.board[6][2..=5]
                .iter()
                .filter(|cell| cell.as_ref().is_some_and(|piece| piece.kind == "pawn"))
                .count(),
            4
        );
        assert_eq!(
            settle_end_move_after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
    }

    #[test]
    fn nonexpiring_queen_and_necromancy_counters_preserve_rng() {
        let mut state = play_state();
        let white = state
            .board
            .iter_mut()
            .flatten()
            .flatten()
            .find(|piece| piece.color == Color::White)
            .unwrap();
        let id = white.id.clone();
        white.kind = "amazon".into();
        white.extra.insert("bribed".into(), json!(true));
        state.extra.insert(
            "temporaryQueens".into(),
            json!([{"color":"white","id":id,"remaining":3}]),
        );
        let black = state
            .board
            .iter_mut()
            .flatten()
            .flatten()
            .find(|piece| piece.color == Color::Black)
            .unwrap();
        let necro_id = black.id.clone();
        black.kind = "rook".into();
        black.color = Color::White.into();
        black.extra.insert(
            "necromancy".into(),
            json!({"revivedType":"rook","remaining":3}),
        );
        state.extra.insert(
            "necromancy".into(),
            json!([{"color":"white","id":necro_id,"revivedType":"rook","remaining":3}]),
        );
        let before_rng = state.rng.clone();
        assert_eq!(
            settle_end_move_before_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.extra["temporaryQueens"][0]["remaining"], json!(2));
        assert_eq!(state.extra["necromancy"][0]["remaining"], json!(2));
        let queen = state
            .board
            .iter()
            .flatten()
            .flatten()
            .find(|piece| piece.id == id)
            .unwrap();
        assert_eq!(queen.extra["bribedRemaining"], json!(2));
        let revived = state
            .board
            .iter()
            .flatten()
            .flatten()
            .find(|piece| piece.id == necro_id)
            .unwrap();
        assert_eq!(revived.extra["necromancyRemaining"], json!(2));
        assert_eq!(revived.extra["necromancy"]["remaining"], json!(2));
        assert_eq!(state.rng, before_rng);
    }

    #[test]
    fn missing_necromancy_subject_drops_only_its_queue_record() {
        let mut state = play_state();
        state.extra.insert(
            "necromancy".into(),
            json!([{"color":"white","id":"revived","remaining":1}]),
        );
        let mut expected = state.clone();
        expected.extra["necromancy"] = json!([]);
        tick_necromancy(&mut state, Color::White).unwrap();
        assert_eq!(state, expected);
    }

    #[test]
    fn temporary_queen_expiry_uses_monochrome_knight_and_preserves_rng() {
        // main109121: expiry changes an active bribed amazon to the
        // monochrome knight type; it does not animate or capture that piece.
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.extra.insert("monochromeChess".into(), json!(true));
        state.extra.insert("logs".into(), json!([]));
        let mut piece = Piece::new("amazon", Color::White, "temporary");
        piece.extra.insert("bribed".into(), json!(true));
        piece.extra.insert("bribedRemaining".into(), json!(1));
        state.board[4][4] = Some(piece.clone());
        state.extra.insert(
            "temporaryQueens".into(),
            json!([{"color":"white","id":"temporary","remaining":1}]),
        );
        let rng = state.rng.clone();
        let animations = state.extra.get("forceAnimatedPieceIds").cloned();
        let history = state.history.clone();
        tick_temporary_queens(&mut state, Color::White).unwrap();
        let reverted = state.board[4][4].as_ref().unwrap();
        assert_eq!(reverted.kind, "camel");
        assert_eq!(reverted.extra["bribed"], false);
        assert!(reverted.extra["bribedRemaining"].is_null());
        assert_eq!(state.extra["temporaryQueens"], json!([]));
        assert_eq!(
            state.extra["logs"],
            json!(["e4의 삼일천하가 끝나 낙타로 돌아왔습니다."])
        );
        assert_eq!(state.rng, rng);
        assert_eq!(
            state.extra.get("forceAnimatedPieceIds"),
            animations.as_ref()
        );
        assert_eq!(state.history, history);
    }

    #[test]
    fn necromancy_expiry_reverts_to_pawn_and_animates_without_rng() {
        // main109138: expiry clears both possession fields, preserves the
        // identity and move flag, and emits the source piece action log.
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.extra.insert("logs".into(), json!([]));
        state.extra.shift_remove("forceAnimatedPieceIds");
        let mut piece = Piece::new("rook", Color::White, "revived");
        piece.moved = true;
        piece.extra.insert(
            "necromancy".into(),
            json!({"revivedType":"rook","remaining":1}),
        );
        piece.extra.insert("necromancyRemaining".into(), json!(1));
        state.board[4][4] = Some(piece);
        state.extra.insert(
            "necromancy".into(),
            json!([{"color":"white","id":"revived","remaining":1,"revivedType":"rook"}]),
        );
        let rng = state.rng.clone();
        tick_necromancy(&mut state, Color::White).unwrap();
        let reverted = state.board[4][4].as_ref().unwrap();
        assert_eq!(reverted.kind, "pawn");
        assert!(reverted.moved);
        assert!(!reverted.extra.contains_key("necromancy"));
        assert!(!reverted.extra.contains_key("necromancyRemaining"));
        assert_eq!(state.extra["necromancy"], json!([]));
        assert_eq!(
            state.extra["logs"],
            json!(["빙의: e4의 기물이 다시 폰으로 돌아왔습니다."])
        );
        assert_eq!(
            state.extra["forceAnimatedPieceIds"]["values"],
            json!(["revived"])
        );
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn necromancy_changed_subject_discards_status_and_lifetime() {
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        let mut piece = Piece::new("bishop", Color::White, "changed");
        piece.extra.insert(
            "necromancy".into(),
            json!({"revivedType":"rook","remaining":4}),
        );
        piece.extra.insert("necromancyRemaining".into(), json!(4));
        state.board[4][4] = Some(piece);
        state.extra.insert(
            "necromancy".into(),
            json!([{"color":"white","id":"changed","remaining":4,"revivedType":"rook"},
                {"color":"black","id":"future","remaining":4},null]),
        );
        let mut expected = state.clone();
        let changed = expected.board[4][4].as_mut().unwrap();
        changed.extra.shift_remove("necromancy");
        changed.extra.shift_remove("necromancyRemaining");
        expected.extra["necromancy"] = json!([{"color":"black","id":"future","remaining":4},null]);
        tick_necromancy(&mut state, Color::White).unwrap();
        assert_eq!(state, expected);
    }

    #[test]
    fn time_stop_keeps_completed_turn_counters_and_restores_action_limit() {
        let mut state = play_state();
        state
            .extra
            .insert("skipTurn".into(), json!({"white":false,"black":true}));
        state.extra.insert("acceleration".into(), json!(true));
        state.actions_remaining = 1;
        state.extra.insert("logs".into(), json!([]));
        let turns = state.turns_taken.clone();
        let move_count = state.move_count;
        let full_move = state.full_move;
        let rng = state.rng.clone();
        assert_eq!(
            settle_end_move_before_count(&mut state, Color::White).unwrap(),
            V7FlowControl::RetainTurn,
        );
        assert_eq!(state.turn, Color::White);
        assert_eq!(state.actions_remaining, 2);
        assert_eq!(
            state.extra["skipTurn"],
            json!({"white":false,"black":false})
        );
        assert_eq!(
            state.extra["logs"],
            json!(["백의 시간 정지 이동은 같은 턴에 이어집니다."])
        );
        assert_eq!(state.turns_taken, turns);
        assert_eq!(state.move_count, move_count);
        assert_eq!(state.full_move, full_move);
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn resolve_credit_marks_only_its_credited_identity_without_counting_turns() {
        let mut state = play_state();
        let id = state.board[6][0].as_ref().unwrap().id.clone();
        state.extra.insert(
            "resolveMoveCredit".into(),
            json!({"white":true,"black":false}),
        );
        state.extra.insert(
            "resolveCreditPieceId".into(),
            json!({"white":id,"black":"unrelated"}),
        );
        let turns = state.turns_taken.clone();
        let rng = state.rng.clone();
        assert!(consume_resolve_credit(&mut state, Color::White).unwrap());
        assert_eq!(
            state.extra["resolveMoveCredit"],
            json!({"white":false,"black":false})
        );
        assert_eq!(
            state.extra["resolveCreditPieceId"],
            json!({"black":"unrelated"})
        );
        assert_eq!(
            state.board[6][0].as_ref().unwrap().extra["idolEncoreRestTurn"],
            json!(0)
        );
        assert!(
            !state.board[6][1]
                .as_ref()
                .unwrap()
                .extra
                .contains_key("idolEncoreRestTurn")
        );
        assert_eq!(state.turns_taken, turns);
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn armistice_normalizes_and_counts_one_round_after_both_colors_act() {
        let mut state = play_state();
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert(
            "armistice".into(),
            json!({"remaining":2.9,"by":"white","actedColors":["black","invalid","black"],"discard":true}),
        );
        let rng = state.rng.clone();
        assert!(tick_armistice_after_action(&mut state, Color::White).unwrap());
        assert_eq!(
            state.extra["armistice"],
            json!({"remaining":1,"by":"white","actedColors":[]})
        );
        tick_armistice_after_action(&mut state, Color::White).unwrap();
        tick_armistice_after_action(&mut state, Color::White).unwrap();
        assert_eq!(
            state.extra["armistice"],
            json!({"remaining":1,"by":"white","actedColors":["white"]})
        );
        tick_armistice_after_action(&mut state, Color::Black).unwrap();
        assert!(state.extra["armistice"].is_null());
        assert_eq!(state.extra["logs"], json!(["휴전이 끝났습니다."]));
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn idol_encore_used_on_this_turn_clears_eligibility_without_movement_probe() {
        let mut state = play_state();
        state.extra.insert(
            "lastMove".into(),
            json!({"idolEncoreEligible":true,"idolEncoreConsumed":false,
                "idolEncoreId":"idol","idolEncorePieceId":"moved","soundColor":"white"}),
        );
        state
            .extra
            .insert("idolEncoreUsedByPiece".into(), json!({"turn:white":0}));
        let mut expected = state.clone();
        expected.extra["lastMove"]["idolEncoreEligible"] = json!(false);
        assert!(!consume_idol_encore(&mut state, Color::White).unwrap());
        assert_eq!(state, expected);
    }

    #[test]
    fn free_move_context_returns_before_credits_and_status_lifetimes() {
        let mut state = play_state();
        state.turn = Color::Black;
        state.free_move_resolution = Some(Color::White);
        state
            .extra
            .insert("activeHistoryMoveNumber".into(), json!(7));
        let pawn = state.board[6][0].as_mut().unwrap();
        pawn.extra.insert("idolEncoreRestTurn".into(), json!(0));
        pawn.extra.insert("disarmed".into(), json!({"remaining":1}));
        state.extra.insert("effects".into(), json!({"extraMove":2}));
        state.extra.insert(
            "temporaryQueens".into(),
            json!([{"color":"white","id":"future","remaining":1}]),
        );
        state
            .extra
            .insert("pendingPawnStorm".into(), json!([{"color":"white"}]));
        state.board[3][3] = Some(Piece::new("pawn", Color::Black, "flag-invader"));
        state.extra.insert(
            "captureTheFlag".into(),
            json!({
                "flags":{"white":{"row":3,"col":3},"black":{"row":4,"col":4}},
                "occupations":{"white":null,"black":null}
            }),
        );
        let mut expected = state.clone();
        // The source updates occupations before its FreeMove return, using
        // movingColor White rather than the live Black turn.
        expected.extra["captureTheFlag"]["occupations"]["white"] =
            json!({"pieceId":"flag-invader","afterOwnerTurn":1});
        clear_selection(&mut expected);
        let mut context = V7EndMoveContext::default();
        assert_eq!(
            settle_end_move_before_count_with_context(&mut state, Color::White, &mut context)
                .unwrap(),
            V7FlowControl::RetainTurn,
        );
        assert!(!context.entered_history_scope);
        assert!(context.previous_history_number.is_none());
        assert!(context.replay_capture.is_none());
        assert_eq!(state, expected);
    }

    #[test]
    fn legacy_severance_expiry_waits_for_full_move_without_decrementing_it() {
        let mut state = play_state();
        let pawn = state.board[6][0].as_mut().unwrap();
        pawn.extra
            .insert("severed".into(), json!({"expiresFullMove":3}));
        let before = state.clone();
        tick_severed_pieces(&mut state, Color::White).unwrap();
        assert_eq!(state, before);
        state.full_move = 3;
        tick_severed_pieces(&mut state, Color::White).unwrap();
        assert!(
            !state.board[6][0]
                .as_ref()
                .unwrap()
                .extra
                .contains_key("severed")
        );
    }

    #[test]
    fn time_traveler_cleanup_normalizes_phase_and_clears_only_actor_credit() {
        let mut state = play_state();
        state.extra.insert(
            "campaign".into(),
            json!({"setup":"timeTraveler","timeTraveler":{
                "phase":"present","visited":["a1"],"afterimageArmed":true,"attackEnabledFor":"black"
            }}),
        );
        let rng = state.rng.clone();
        clear_time_traveler_turn_effects(&mut state, Color::White).unwrap();
        assert_eq!(
            state.extra["campaign"]["timeTraveler"],
            json!({
                "phase":"future","visited":["a1"],"attackEnabledFor":"black"
            })
        );
        clear_time_traveler_turn_effects(&mut state, Color::Black).unwrap();
        assert_eq!(
            state.extra["campaign"]["timeTraveler"]["attackEnabledFor"],
            Value::Null
        );
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn gomoku_alias_identity_cannot_complete_a_vertical_five() {
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.set_flag("gomoku", Color::White, true);
        for row in 0..5 {
            state.board[row][0] = Some(Piece::new("pawn", Color::White, "same"));
        }
        let before = state.clone();
        assert!(!check_gomoku_victory(&mut state).unwrap());
        assert_eq!(state, before);
    }

    #[test]
    fn simultaneous_gomoku_completions_draw_with_white_then_black_cells() {
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        for color in [Color::White, Color::Black] {
            state.set_flag("gomoku", color, true);
            let col = if color == Color::White { 0 } else { 1 };
            for row in 0..5 {
                state.board[row][col] = Some(Piece::new(
                    "pawn",
                    color,
                    format!("{}-{row}", color.as_str()),
                ));
            }
        }
        assert!(check_gomoku_victory(&mut state).unwrap());
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner, None);
        assert_eq!(
            state.extra["gomokuVictoryCells"],
            json!([
                {"row":0,"col":0},{"row":1,"col":0},{"row":2,"col":0},{"row":3,"col":0},{"row":4,"col":0},
                {"row":0,"col":1},{"row":1,"col":1},{"row":2,"col":1},{"row":3,"col":1},{"row":4,"col":1}
            ])
        );
    }

    #[test]
    fn pending_pawn_storm_keeps_reference_order_on_equal_rows_and_deduplicates() {
        let mut state = play_state();
        let first = state.board[6][1].as_ref().unwrap().id.clone();
        let second = state.board[6][0].as_ref().unwrap().id.clone();
        state.extra.insert(
            "pendingPawnStorm".into(),
            json!([
                {"color":"white","pieces":[{"id":first},{"id":second},{"id":first}]},
                {"color":"black","pieces":[{"id":"later"}]}
            ]),
        );
        assert_eq!(
            apply_pending_pawn_storm_for_turn(&mut state, Color::White).unwrap(),
            2
        );
        assert_eq!(
            state.extra["pendingPawnStorm"],
            json!([{"color":"black","pieces":[{"id":"later"}]}])
        );
        assert_eq!(
            state.extra["lastMove"]["pawnStormMoves"],
            json!([
                {"from":{"row":6,"col":1},"to":{"row":5,"col":1}},
                {"from":{"row":6,"col":0},"to":{"row":5,"col":0}}
            ])
        );
        assert_eq!(
            state.board[5][1].as_ref().unwrap().extra["cardNoCaptureUntil"],
            json!(1)
        );
        assert!(state.board[6][0].is_none() && state.board[6][1].is_none());
    }

    #[test]
    fn automatic_log_waits_for_its_roll_boundary_without_changing_the_position() {
        let mut state = play_state();
        let mut log = Piece::new("log", Color::White, "waiting-log");
        log.extra.insert("logDir".into(), json!({"dr":-1,"dc":0}));
        log.extra.insert("logRollAfterTurn".into(), json!(1));
        state.board[5][0] = Some(log);
        let before = state.clone();
        auto_advance_logs(&mut state, Color::White).unwrap();
        assert_eq!(state, before);
    }

    #[test]
    fn off_board_automatic_log_stops_and_clears_its_roll_boundary() {
        let mut state = play_state();
        let mut log = Piece::new("log", Color::White, "edge-log");
        log.extra.insert("logDir".into(), json!({"dr":-1,"dc":0}));
        state.board[0][0] = Some(log);
        let rng = state.rng.clone();
        auto_advance_logs(&mut state, Color::White).unwrap();
        let log = state.board[0][0].as_ref().unwrap();
        assert_eq!(log.extra["logDir"], Value::Null);
        assert!(!log.extra.contains_key("logRollAfterTurn"));
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn monster_probe_suppression_preserves_the_entire_position() {
        let mut state = play_state();
        state.board[3][3] = Some(Piece::new(
            "monster",
            crate::PieceColor::Neutral,
            "probe-monster",
        ));
        state.threat_probe_depth = 1;
        let before = state.clone();
        assert_eq!(move_rule_monsters(&mut state, Color::Black).unwrap(), 0);
        assert_eq!(state, before);
    }

    #[test]
    fn monster_ai_simulation_moves_without_notation_rng_or_replay_visuals() {
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.board[3][3] = Some(Piece::new(
            "monster",
            crate::PieceColor::Neutral,
            "ai-monster",
        ));
        state.ai_simulation_depth = 1;
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert("pendingNotations".into(), json!([]));
        state.extra.insert("pendingNotation".into(), Value::Null);
        state.extra.insert("pendingReplayVisuals".into(), json!([]));
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        let history = state.history.clone();
        let replay = state.extra.get("replayEvents").cloned();
        assert!(state.is_ai_simulation());
        assert_eq!(state.threat_probe_depth, 0);
        assert_eq!(move_rule_monsters(&mut state, Color::Black).unwrap(), 1);
        assert!(state.board[3][3].is_none());
        assert_eq!(
            state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|piece| piece.id == "ai-monster")
                .count(),
            1
        );
        assert_eq!(state.rng, expected_rng);
        assert_eq!(state.extra["pendingNotations"], json!([]));
        assert_eq!(state.extra["pendingNotation"], Value::Null);
        assert_eq!(state.extra["pendingReplayVisuals"], json!([]));
        assert_eq!(state.extra["logs"].as_array().unwrap().len(), 1);
        assert_eq!(state.history, history);
        assert_eq!(state.extra.get("replayEvents"), replay.as_ref());
    }

    #[test]
    fn monster_with_no_destinations_consumes_exactly_one_choice_draw() {
        let mut state = play_state();
        state.board[3][3] = Some(Piece::new(
            "monster",
            crate::PieceColor::Neutral,
            "boxed-monster",
        ));
        for dr in -1..=1 {
            for dc in -1..=1 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let square = (Square { row: 3, col: 3 }).offset(dr, dc).unwrap();
                state.board[square.row as usize][square.col as usize] = Some(Piece::new(
                    "wall",
                    crate::PieceColor::Neutral,
                    format!("wall-{dr}-{dc}"),
                ));
            }
        }
        let mut expected = state.clone();
        expected.rng.sample().unwrap();
        assert_eq!(move_rule_monsters(&mut state, Color::Black).unwrap(), 0);
        assert_eq!(state, expected);
    }

    #[test]
    fn meteor_repeated_cells_damage_hp_twice_and_keep_opposite_hazards() {
        let mut state = play_state();
        let mut target = Piece::new("shotgunKing", Color::White, "meteor-target");
        target.extra.insert("hp".into(), json!(3));
        target.extra.insert("maxHp".into(), json!(3));
        state.board[3][3] = Some(target);
        state.extra.insert("delayedHazards".into(),json!([
            {"triggerAfter":"white","owner":"black","type":"meteor","cells":[{"row":3,"col":3},{"row":3,"col":3}]},
            {"triggerAfter":"black","owner":"white","type":"lightning","cells":[{"row":4,"col":4}]}
        ]));
        apply_delayed_hazards(&mut state, Color::White).unwrap();
        assert_eq!(state.board[3][3].as_ref().unwrap().extra["hp"], json!(1.0));
        assert_eq!(
            state.extra["delayedHazards"],
            json!([
                {"triggerAfter":"black","owner":"white","type":"lightning","cells":[{"row":4,"col":4}]}
            ])
        );
        assert_eq!(
            state.extra["wizardImpact"],
            json!([
                {"row":3,"col":3,"type":"meteor"},{"row":3,"col":3,"type":"meteor"}
            ])
        );
        assert!(state.captures.black.is_empty());
    }

    #[test]
    #[ignore = "requires a pinned source receipt outside Git"]
    fn external_end_move_callback_source_receipt_matches_full_position() {
        let path = std::env::var("ACCELERATE_V7_END_MOVE_FIXTURE")
            .expect("set ACCELERATE_V7_END_MOVE_FIXTURE to the pinned callback receipt");
        let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let actor = match fixture.get("actor").and_then(Value::as_str) {
            Some("black") => Color::Black,
            Some("white") | None => Color::White,
            Some(other) => panic!("unrecognized receipt actor {other}"),
        };
        let before =
            crate::v7_host::V7HostPosition::from_envelope(fixture["before"].clone()).unwrap();
        let (after, _) = before
            .transact(before.position_id(), |state| {
                match fixture["callback"].as_str() {
                    Some("tickTemporaryQueens") => tick_temporary_queens(state, actor),
                    Some("tickNecromancy") => tick_necromancy(state, actor),
                    Some("resolveWitchTrials") => resolve_witch_trials(state, actor),
                    Some("resolveWantedArrests") => resolve_wanted_arrests(state, actor),
                    Some("tickSeveredPieces") => tick_severed_pieces(state, actor),
                    Some("tickArmisticeAfterAction") => {
                        tick_armistice_after_action(state, actor).map(|_| ())
                    }
                    Some("consumeIdolEncore") => consume_idol_encore(state, actor).map(|_| ()),
                    Some("septemberUseResolveCredit") => {
                        consume_resolve_credit(state, actor).map(|_| ())
                    }
                    Some("retainTimeStopAsSameTurn") => retain_time_stop(state, actor).map(|_| ()),
                    Some("clearTimeTravelerTurnEffects") => {
                        clear_time_traveler_turn_effects(state, actor)
                    }
                    Some("checkGomokuVictory") => check_gomoku_victory(state).map(|_| ()),
                    Some("applyPendingPawnStormForTurn") => {
                        apply_pending_pawn_storm_for_turn(state, actor).map(|_| ())
                    }
                    Some("autoAdvanceLogs") => auto_advance_logs(state, actor),
                    Some("applyDelayedHazards") => apply_delayed_hazards(state, actor),
                    Some("moveRuleMonsters") => move_rule_monsters(state, actor).map(|_| ()),
                    Some("clearPreviousTricksterAbilities") => {
                        clear_previous_trickster_abilities(state);
                        Ok(())
                    }
                    other => Err(EngineError::InvalidState(format!(
                        "unrecognized endMove receipt callback {other:?}"
                    ))),
                }?;
                // Source OracleRuntime.snapshot drains __settleMicrotasks after
                // this one callback, including the queued terminal replay record.
                crate::replay::settle(state)
            })
            .unwrap_or_else(|error| panic!("endMove callback {}: {error}", fixture["callback"]));
        let actual = after.export_envelope().unwrap();
        let mut mismatches = Vec::new();
        crate::tests::source_callback_fixture::compare_value(
            &fixture["after"],
            &actual,
            "endMove.after",
            &mut mismatches,
        )
        .unwrap();
        assert!(
            mismatches.is_empty(),
            "endMove callback {} differs:\n{}",
            fixture["callback"],
            mismatches.join("\n")
        );
    }
}
