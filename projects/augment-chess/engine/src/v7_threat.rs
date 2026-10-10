//! Frozen v7 royal-threat probe for the headless local profile.
//!
//! The source's `evaluateKingThreatReport` uses an owned, temporary board and
//! `collectValidAiActions(includeCards:false)`. This is not the public legal
//! surface or the UI highlight surface. Unsupported source branches are
//! errors here: a failed probe must never be reported as "no threat".

use crate::{Action, Color, EngineError, GameState, Piece, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const THREAT_CANDIDATE_BUDGET: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RoyalThreatProbe {
    pub check: bool,
    pub danger: bool,
    /// A source-equivalent move simulation was attempted. This is diagnostic
    /// only and cannot identify whether begin/commit/cancel actually ran.
    pub simulated: bool,
    pub examined: usize,
}

/// Preserve only the rule-visible move Replay capture commands produced by a
/// private royal-threat simulation. Source audio used to call this probe and
/// copy its RNG/sound cue back to the live state. Those presentation effects
/// are deliberately absent; the pending undo capture remains until its future
/// Replay-card dependency can be verified independently.
pub(crate) fn reconcile_move_replay_capture_v7(state: &mut GameState) -> Result<()> {
    if state.mode != "play" || state.is_ai_simulation() || state.threat_probe_depth > 0 {
        return Ok(());
    }
    let replay_scope = crate::replay::ReplayCaptureScope::for_probe(state);
    let mut working = state.clone();
    replay_scope.attach(&mut working);
    let result = (|| {
        for defender in [Color::White, Color::Black] {
            if probe_royal_capture(&mut working, defender, false)?.check {
                break;
            }
        }
        Ok(())
    })();
    let control = crate::replay::active_move_capture(&working)
        .and_then(|capture| crate::replay::replace_active_move_capture(state, capture));
    finish_replay_capture_scope(result, control)
}

/// Source finally restores state/depth without restoring the module-global
/// replay capture. Preserve actual control effects on every exit, including
/// a rejected nested attempt, while retaining both errors if finalization fails.
fn finish_replay_capture_scope<T>(result: Result<T>, control: Result<()>) -> Result<T> {
    match (result, control) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(control)) => Err(EngineError::InvalidState(format!(
            "v7 royal threat failed: {error}; replay capture finalization also failed: {control}",
        ))),
    }
}

/// Called on a transaction-owned state. Successful simulations may advance
/// its RNG while the simulated board stays private. The source-global pending
/// replay capture follows actual begin/commit/cancel commands across clones.
/// The host discards this entire state when a later callback fails.
pub(crate) fn probe_royal_capture(
    state: &mut GameState,
    defender: Color,
    include_danger: bool,
) -> Result<RoyalThreatProbe> {
    crate::legal_profile::measure("threat_probe_royal_capture", || {
        probe_royal_capture_profiled(state, defender, include_danger)
    })
}

pub(crate) fn probe_royal_capture_profiled(
    state: &mut GameState,
    defender: Color,
    include_danger: bool,
) -> Result<RoyalThreatProbe> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 royal threat on rules version {}",
            state.ruleset_id
        )));
    }
    let replay_scope = crate::replay::ReplayCaptureScope::for_probe(state);
    let result = (|| {
        if state.mode != "play" {
            return Ok(RoyalThreatProbe {
                check: false,
                danger: false,
                simulated: false,
                examined: 0,
            });
        }
        let mut window = source_window(state, defender)?;
        replay_scope.attach(&mut window);
        let royals = royal_ids(&window, defender)?;
        if royals.is_empty() {
            return Ok(RoyalThreatProbe {
                check: false,
                danger: false,
                simulated: false,
                examined: 0,
            });
        }
        // The frozen `collectValidAiActions` returns an empty stream while a
        // trolley choice is pending, for both its move and cards-only variants.
        // Even automatic threat candidates cannot be simulated in that window.
        if crate::observation::truth(window.extra.get("activeTrolley")) {
            return Ok(RoyalThreatProbe {
                check: false,
                danger: false,
                simulated: false,
                examined: 0,
            });
        }
        validate_automatic_reaction_schema(&window)?;

        // The complete ordered AiNoCards surface is movement-owned. A failed
        // live kernel reports its concrete error rather than a partial prefix.
        let actions =
            crate::movement::v7_ai_no_cards_move_candidates(&window, defender.opponent())?;
        probe_ordered_candidates(state, &window, defender, include_danger, &royals, &actions)
    })();
    finish_replay_capture_scope(result, replay_scope.carry_into(state))
}

/// The source candidate stream is supplied by movement, but the isolated
/// simulation can be verified independently against a source-reported attack.
/// This boundary also makes the candidate order explicit: every relevant
/// attempt receives the prior attempt's RNG even when its board is discarded.
fn probe_ordered_candidates(
    state: &mut GameState,
    window: &GameState,
    defender: Color,
    include_danger: bool,
    royals: &BTreeSet<String>,
    actions: &[Action],
) -> Result<RoyalThreatProbe> {
    // Direct source-candidate checks in the unit boundary also own a scope;
    // a production window reuses its existing journal rather than resetting it.
    let replay_scope = crate::replay::ReplayCaptureScope::for_probe(window);
    let mut window = window.clone();
    replay_scope.attach(&mut window);
    let result = (|| {
        let mut source_rng = state.rng.clone();
        let refs = source_royal_refs(&window, defender, royals)?;
        let result = collect_capture_entry_keys(
            &window,
            defender,
            &refs,
            actions,
            CaptureEntryContext {
                kind: "check",
                card_id: "",
                depth: 0,
            },
            &mut source_rng,
        )?;
        let check = !result.entries.is_empty();
        let mut danger_result = CaptureEntryKeys::default();
        if !check && include_danger {
            let mut prepared = window.clone();
            prepared.rng = source_rng.clone();
            prepared.semantic_chance_probability = None;
            let card_actions = crate::v7_ai_card_candidates::collect_v7_ai_card_actions(
                &mut prepared,
                defender.opponent(),
                true,
            )?;
            source_rng = prepared.rng.clone();
            danger_result = collect_danger_entry_keys(
                &prepared,
                defender,
                &refs,
                &card_actions,
                &mut source_rng,
            )?;
        }
        state.rng = source_rng;
        Ok(RoyalThreatProbe {
            check,
            danger: !danger_result.entries.is_empty(),
            simulated: result.simulated || danger_result.simulated,
            examined: result.examined + danger_result.examined,
        })
    })();
    finish_replay_capture_scope(result, replay_scope.carry_into(state))
}

#[derive(Clone)]
struct SourceRoyalRef {
    id: String,
    color: Color,
    at: Square,
}

#[derive(Default)]
struct CaptureEntryKeys {
    entries: Vec<String>,
    simulated: bool,
    examined: usize,
}

fn source_royal_refs(
    state: &GameState,
    defender: Color,
    royals: &BTreeSet<String>,
) -> Result<Vec<SourceRoyalRef>> {
    let mut refs = Vec::new();
    let mut seen = BTreeSet::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, cell) in cells.iter().enumerate() {
            let Some(piece) = cell
                .as_ref()
                .filter(|piece| piece.color == defender && royals.contains(&piece.id))
            else {
                continue;
            };
            if seen.insert(piece.id.clone()) {
                refs.push(SourceRoyalRef {
                    id: piece.id.clone(),
                    color: defender,
                    at: Square {
                        row: row as u8,
                        col: col as u8,
                    },
                });
            }
        }
    }
    if seen != *royals {
        return Err(EngineError::InvalidState(
            "v7 royal threat reference has no source board identity".into(),
        ));
    }
    Ok(refs)
}

fn royal_ref_alive(state: &GameState, royal: &SourceRoyalRef) -> bool {
    state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|piece| piece.id == royal.id && piece.color == royal.color)
}

#[derive(Clone, Copy)]
struct CaptureEntryContext<'a> {
    kind: &'a str,
    card_id: &'a str,
    depth: u8,
}

fn collect_capture_entry_keys(
    window: &GameState,
    defender: Color,
    royals: &[SourceRoyalRef],
    actions: &[Action],
    context: CaptureEntryContext<'_>,
    source_rng: &mut crate::RngState,
) -> Result<CaptureEntryKeys> {
    let CaptureEntryContext {
        kind,
        card_id,
        depth,
    } = context;
    if window.mode != "play" || depth > 12 {
        return Ok(CaptureEntryKeys::default());
    }
    if actions.len() > THREAT_CANDIDATE_BUDGET {
        return Err(EngineError::UnsupportedFeature(
            "v7 royal threat candidate limit exceeded".into(),
        ));
    }
    let mut result = CaptureEntryKeys::default();
    let royal_ids: BTreeSet<_> = royals.iter().map(|royal| royal.id.clone()).collect();
    let automatic_threat = has_automatic_threat_candidate(window, royals)?;
    let hidden_attacker_ids = crate::observation::v7_hidden_opponent_piece_ids(window, defender)?;
    for action in actions {
        // Both source relevance alternatives require action.type === move.
        // The full AiNoCards stream also contains reload, promotion, wizard
        // and optional-skip actions, which cannot enter this simulation loop.
        if action.kind != crate::ActionKind::Move {
            continue;
        }
        let from = action.from.ok_or(EngineError::IllegalAction)?;
        let moving = window.at(from).ok_or(EngineError::IllegalAction)?;
        let (attacker, origin) = threat_action_attacker(window, action, moving, from)?;
        if attacker.kind == "trickster" || hidden_attacker_ids.contains(&attacker.id) {
            continue;
        }
        if !automatic_threat && !relevant_move(window, action, moving, &royal_ids)? {
            continue;
        }
        result.examined += 1;
        let mut child = clone_for_threat_simulation(window);
        child.rng = source_rng.clone();
        child.semantic_chance_probability = None;
        suppress_hidden_threat_effects(&mut child, &hidden_attacker_ids);
        let portal_refs = direct_portal_royal_ids(&child, action, royals)?;
        let execution = crate::transition::execute_threat_move(&mut child, action);
        *source_rng = child.rng.clone();
        result.simulated = true;
        match execution {
            Ok(_) => {}
            // Source applyAiAction returns ok:false for a rejected attempt.
            // Its Math.random cursor is outside the discarded state clone.
            Err(EngineError::IllegalAction) => continue,
            Err(error) => return Err(error),
        }
        let removed: BTreeSet<_> = royals
            .iter()
            .filter(|royal| !royal_ref_alive(&child, royal))
            .map(|royal| royal.id.as_str())
            .collect();
        let mut captured = false;
        for royal in royals {
            let causes = recorded_cause_entries(&child, royal)?;
            if !removed.contains(royal.id.as_str())
                && !portal_refs.contains(&royal.id)
                && causes.is_empty()
            {
                continue;
            }
            captured = true;
            let attacker_id = if attacker.id.is_empty() {
                format!("{}:{}", origin.row, origin.col)
            } else {
                attacker.id.clone()
            };
            let direct_key = if kind == "danger" {
                format!(
                    "danger:{}:{card_id}:{attacker_id}:{}:{}",
                    royal.id, origin.row, origin.col
                )
            } else {
                format!(
                    "check:{}:{attacker_id}:{}:{}",
                    royal.id, origin.row, origin.col
                )
            };
            let keys = if causes.is_empty() {
                if removed.is_empty() {
                    vec![direct_key]
                } else {
                    vec![format!(
                        "{kind}:{}:effect:{}",
                        royal.id,
                        child
                            .extra
                            .get("replayEndReason")
                            .and_then(Value::as_str)
                            .filter(|reason| !reason.is_empty())
                            .unwrap_or("removed")
                    )]
                }
            } else {
                causes
                    .into_iter()
                    .map(|entry| {
                        if crate::observation::truth(entry.get("direct"))
                            && entry.get("attackerPieceId").and_then(Value::as_str)
                                == Some(attacker.id.as_str())
                        {
                            Ok(direct_key.clone())
                        } else {
                            entry
                                .get("key")
                                .and_then(Value::as_str)
                                .map(|key| format!("{kind}:{card_id}:{key}"))
                                .ok_or_else(|| {
                                    EngineError::InvalidState(
                                        "v7 recorded threat entry is missing its source key".into(),
                                    )
                                })
                        }
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            for key in keys {
                if !result.entries.contains(&key) {
                    result.entries.push(key);
                }
            }
        }
        if !captured
            && result.entries.len() < 32
            && depth < 12
            && child.mode == "play"
            && child.turn == defender.opponent()
            && has_threat_forced_continuation(&child, defender.opponent())
        {
            let next =
                crate::movement::v7_ai_no_cards_move_candidates(&child, defender.opponent())?;
            let continued = collect_capture_entry_keys(
                &child,
                defender,
                royals,
                &next,
                CaptureEntryContext {
                    kind,
                    card_id,
                    depth: depth + 1,
                },
                source_rng,
            )?;
            // Source appends the recursive list directly; local duplicate
            // suppression applies only when adding the next direct cause.
            result.entries.extend(continued.entries);
            result.simulated |= continued.simulated;
            result.examined += continued.examined;
        }
        if result.entries.len() >= 32 {
            break;
        }
    }
    Ok(result)
}

/// main85601: a danger card is applied raw, without finishCard. A card that
/// removes an original royal itself is excluded; only the following move
/// capture stream establishes danger. Every discarded attempt shares RNG.
fn collect_danger_entry_keys(
    window: &GameState,
    defender: Color,
    royals: &[SourceRoyalRef],
    actions: &[Action],
    source_rng: &mut crate::RngState,
) -> Result<CaptureEntryKeys> {
    if actions.len() > THREAT_CANDIDATE_BUDGET {
        return Err(EngineError::UnsupportedFeature(
            "v7 royal danger card candidate limit exceeded".into(),
        ));
    }
    let attacker = defender.opponent();
    let mut result = CaptureEntryKeys::default();
    for action in actions {
        if action.kind != crate::ActionKind::Card {
            continue;
        }
        let mut child = clone_for_threat_simulation(window);
        child.rng = source_rng.clone();
        child.semantic_chance_probability = None;
        child.turn = attacker;
        let cards = child.deck_slots.get(attacker);
        // Generated candidates retain their source instance identity. The
        // id fallback follows findAiActionCard for restored source actions.
        let card = cards
            .iter()
            .find(|card| {
                !card.vacant
                    && action.card_instance_id.as_deref() == Some(card.instance_id.as_str())
            })
            .or_else(|| {
                cards.iter().find(|card| {
                    if card.vacant
                        || card.used
                        || card.recovering
                        || action.card_id.as_deref() != Some(card.id.as_str())
                    {
                        return false;
                    }
                    let pending_phase = matches!(
                        card.extra.get("phase").and_then(Value::as_str),
                        Some("MIDDLE" | "END")
                    ) && crate::observation::truth(
                        card.extra.get("nextTurnPending"),
                    ) && !crate::observation::truth(card.extra.get("devCard"));
                    !pending_phase
                })
            })
            .cloned();
        let Some(card) = card else {
            continue;
        };
        let definition = crate::card_registry::validate_instance(&child, &card)?;
        if card.extra.get("phase").and_then(Value::as_str) == Some("RULE")
            || crate::draft::is_passive_definition_for_ruleset(
                RULES_VERSION_V7,
                &definition.source_definition,
            )?
            || matches!(card.id.as_str(), "shotgun-king" | "summon-colossus")
        {
            continue;
        }
        let execution = crate::transition::apply_card_raw(&mut child, &card, action);
        *source_rng = child.rng.clone();
        match execution {
            Ok(_) => {}
            Err(EngineError::IllegalAction) => continue,
            Err(error) => return Err(error),
        }
        if child.mode != "play" || royals.iter().any(|royal| !royal_ref_alive(&child, royal)) {
            continue;
        }
        // applyCard may replace the original deck. Source marks the original
        // object used only if it is still attached; no replacement is made.
        if let Some(attached) = child
            .deck_slots
            .get_mut(attacker)
            .iter_mut()
            .find(|attached| attached.instance_id == card.instance_id)
        {
            attached.used = true;
        }
        child.extra.insert("selected".into(), Value::Null);
        child.extra.insert("legalMoves".into(), json!([]));
        child.extra.insert("targeting".into(), Value::Null);
        child.turn = attacker;
        child.actions_remaining = 1;
        if let Some(effects) = child
            .extra
            .get_mut("effects")
            .and_then(Value::as_object_mut)
        {
            effects.insert("extraMove".into(), json!(0));
        }
        validate_automatic_reaction_schema(&child)?;
        let moves = crate::movement::v7_ai_no_cards_move_candidates(&child, attacker)?;
        let captured = collect_capture_entry_keys(
            &child,
            defender,
            royals,
            &moves,
            CaptureEntryContext {
                kind: "danger",
                card_id: &card.id,
                depth: 0,
            },
            source_rng,
        )?;
        result.entries.extend(captured.entries);
        result.simulated |= captured.simulated;
        result.examined += captured.examined;
        if result.entries.len() >= 32 {
            break;
        }
    }
    Ok(result)
}

fn threat_action_attacker<'a>(
    state: &'a GameState,
    action: &Action,
    moving: &'a Piece,
    from: Square,
) -> Result<(&'a Piece, Square)> {
    let Some(move_) = action
        .destination
        .as_ref()
        .filter(|move_| move_.flag("footballKick"))
    else {
        return Ok((moving, from));
    };
    let Some(kicker) = move_.flags.get("kicker") else {
        return Ok((moving, from));
    };
    let (row, col) = source_cell(kicker)?;
    if row >= 8 || col >= 8 {
        return Err(EngineError::InvalidState(
            "v7 football threat kicker outside board".into(),
        ));
    }
    let origin = Square {
        row: row as u8,
        col: col as u8,
    };
    Ok((state.at(origin).unwrap_or(moving), origin))
}

fn recorded_cause_entries<'a>(
    state: &'a GameState,
    royal: &SourceRoyalRef,
) -> Result<Vec<&'a Value>> {
    let mut entries = Vec::new();
    for cause in state
        .extra
        .get("kingThreatCaptureCauses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if cause
            .get("royalRef")
            .and_then(|value| value.get("color"))
            .and_then(Value::as_str)
            != Some(royal.color.as_str())
        {
            continue;
        }
        let entry = cause.get("entry").ok_or_else(|| {
            EngineError::InvalidState("v7 recorded royal cause has no entry".into())
        })?;
        if entry.get("royalId").and_then(Value::as_str) != Some(royal.id.as_str()) {
            continue;
        }
        if !royal_ref_alive(state, royal)
            || crate::observation::truth(entry.get("terminal"))
                && state.mode == "gameover"
                && state.winner.as_deref() != Some(royal.color.as_str())
        {
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn direct_portal_royal_ids(
    state: &GameState,
    action: &Action,
    royals: &[SourceRoyalRef],
) -> Result<BTreeSet<String>> {
    let mut ids = BTreeSet::new();
    let Some(move_) = action
        .destination
        .as_ref()
        .filter(|move_| move_.flag("portalLanding") || move_.flag("portalThrough"))
    else {
        return Ok(ids);
    };
    let Some(attacker) = action.from.and_then(|from| state.at(from)) else {
        return Ok(ids);
    };
    let landing = source_move_destination(move_)?;
    for at in crate::movement::v7_move_capture_target_cells(move_)? {
        let Some(victim) = state.at(at) else {
            continue;
        };
        if victim.color == attacker.color
            || !royals.iter().any(|royal| royal.id == victim.id)
            || victim.kind != "scarecrow"
                && (crate::observation::truth(victim.extra.get("shielded"))
                    || crate::observation::truth(victim.extra.get("protected"))
                    || source_counter(
                        victim
                            .extra
                            .get("vigilanceProtection")
                            .and_then(|value| value.get("remaining")),
                    ) > 0.0
                    || matches!(victim.kind.as_str(), "football" | "monster"))
        {
            continue;
        }
        if crate::observation::truth(victim.extra.get("evasion"))
            && !crate::v7_capture_reactions::evasion_destination_candidates(
                state,
                victim,
                at,
                &crate::v7_capture_reactions::CaptureOptions {
                    attacker_landing: Some(landing),
                    ..Default::default()
                },
            )?
            .is_empty()
        {
            continue;
        }
        ids.insert(victim.id.clone());
    }
    Ok(ids)
}

fn has_threat_forced_continuation(state: &GameState, actor: Color) -> bool {
    if state.board.iter().flatten().flatten().any(|piece| {
        piece.color == actor
            && piece
                .extra
                .get("repositionSecondMove")
                .is_some_and(|move_| {
                    crate::observation::truth(move_.get("used"))
                        || crate::observation::truth(move_.get("forced"))
                })
    }) {
        return true;
    }
    let active = state.board.iter().flatten().flatten().find(|piece| {
        piece.color == actor
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
            .iter()
            .any(|field| crate::observation::truth(piece.extra.get(*field)))
    });
    active.is_some_and(|piece| {
        [
            "thiefSecondMove",
            "fileSurgeSecondMove",
            "rookLiftSecondMove",
            "ironMonarchExtraMove",
            "madHorseSecondMove",
            "underpromotionSecondMove",
        ]
        .iter()
        .any(|field| crate::observation::truth(piece.extra.get(*field)))
    })
}

fn has_automatic_threat_candidate(state: &GameState, royals: &[SourceRoyalRef]) -> Result<bool> {
    if state.board.iter().flatten().flatten().any(|piece| {
        crate::observation::truth(piece.extra.get("logDir"))
            || piece.ability_kind() == "siren"
            || royals.iter().any(|royal| royal.color == piece.color)
                && (piece.ability_kind() == "brutus"
                    || crate::observation::number(
                        piece
                            .extra
                            .get("witchTrial")
                            .and_then(|entry| entry.get("remaining")),
                    )
                    .is_some_and(|remaining| remaining <= 1.0))
    }) || crate::observation::truth(state.extra.get("conveyorRule"))
        || crate::observation::truth(state.extra.get("ultimatum"))
        || state
            .extra
            .get("pendingGales")
            .and_then(Value::as_array)
            .is_some_and(|queue| !queue.is_empty())
    {
        return Ok(true);
    }
    for hazard in state
        .extra
        .get("delayedHazards")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if hazard
            .get("cells")
            .and_then(Value::as_array)
            .is_some_and(|cells| {
                cells
                    .iter()
                    .any(|cell| royals.iter().any(|royal| matches_square(cell, royal.at)))
            })
        {
            return Ok(true);
        }
    }
    for entry in state
        .extra
        .get("pendingOtherworld")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if !royals.iter().any(|royal| {
            crate::card_effects::js_number(entry.get("row"), 0) == Some(f64::from(royal.at.row))
                && crate::card_effects::js_number(entry.get("col"), 0)
                    == Some(f64::from(royal.at.col))
        }) {
            continue;
        }
        let remaining = crate::observation::number(entry.get("remainingHalfTurns"))
            .filter(|value| {
                *value >= 0.0 && *value <= 9_007_199_254_740_991.0 && value.fract() == 0.0
            })
            .unwrap_or_else(|| {
                (source_counter(entry.get("dueMoveCount")).floor().max(0.0)
                    - f64::from(state.move_count))
                .max(0.0)
            });
        if remaining <= 1.0 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `suppressHiddenKingThreatEffects` decorates only the private probe board.
/// The source global probe depth controls consumers of this marker; a public
/// board must never inherit the temporary suppression or changed log direction.
fn suppress_hidden_threat_effects(state: &mut GameState, hidden_ids: &BTreeSet<String>) {
    for piece in state.board.iter_mut().flatten().flatten() {
        if !hidden_ids.contains(&piece.id) {
            continue;
        }
        piece
            .extra
            .insert("kingThreatSuppressed".into(), json!(true));
        if piece.kind == "log" {
            piece.extra.insert("logDir".into(), Value::Null);
        }
    }
}

fn source_window(state: &GameState, defender: Color) -> Result<GameState> {
    let mut window = clone_for_threat_simulation(state);
    window.ai_simulation_depth = state
        .ai_simulation_depth
        .checked_add(1)
        .ok_or_else(|| EngineError::InvalidState("v7 AI simulation depth overflow".into()))?;
    window.threat_probe_depth = state
        .threat_probe_depth
        .checked_add(1)
        .ok_or_else(|| EngineError::InvalidState("v7 threat probe depth overflow".into()))?;
    window.extra.insert("collapsePending".into(), Value::Null);
    window.extra.insert("periodicCollapse".into(), Value::Null);
    if window.turn == defender {
        crate::threat::tick_protection(&mut window, defender, "lastResistance");
        crate::threat::tick_protection(&mut window, defender, "sacrificeProtection");
    }
    window.turn = defender.opponent();
    window.actions_remaining = 1;
    if let Some(effects) = window
        .extra
        .get_mut("effects")
        .and_then(Value::as_object_mut)
    {
        effects.insert("extraMove".into(), json!(0));
    }
    Ok(window)
}

fn clone_for_threat_simulation(state: &GameState) -> GameState {
    let mut window = state.clone();
    for field in [
        "selected",
        "dragging",
        "targeting",
        "replayBaseFrame",
        "replayTailFrame",
        "pendingNotation",
    ] {
        window.extra.insert(field.into(), Value::Null);
    }
    for field in [
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
        window.extra.insert(field.into(), json!([]));
    }
    window.history.clear();
    window.gameover_replay_pending = false;
    window
}

fn royal_ids(state: &GameState, defender: Color) -> Result<BTreeSet<String>> {
    let mut ids = BTreeSet::new();
    for piece in state.board.iter().flatten().flatten() {
        if piece.color != defender || !is_v7_threat_royal(state, piece) {
            continue;
        }
        if piece.id.is_empty() {
            return Err(EngineError::InvalidState(
                "v7 royal threat requires a royal piece identity".into(),
            ));
        }
        ids.insert(piece.id.clone());
    }
    Ok(ids)
}

/// Frozen `isKingThreatRoyalPiece`: the king-threat query has its own royal
/// predicate. Under Regency a living queen suppresses a native royal; once
/// kingDead is set, a marked heir becomes the royal instead.
pub(crate) fn is_v7_threat_royal(state: &GameState, piece: &Piece) -> bool {
    let color = piece.color;
    if matches!(
        piece.kind.as_str(),
        "vip" | "merchant" | "timeTraveler" | "vampireLord"
    ) {
        return true;
    }
    if source_color_truth(state, "democracy", color)
        && (crate::observation::truth(piece.extra.get("regencyHeir"))
            || crate::observation::truth(piece.extra.get("crownRoyal"))
            || matches!(piece.kind.as_str(), "king" | "royalKnight" | "shotgunKing"))
    {
        return false;
    }
    let native = crate::observation::truth(piece.extra.get("crownRoyal"))
        || crate::observation::truth(piece.extra.get("editorRoyal"))
        || matches!(
            piece.kind.as_str(),
            "king" | "royalKnight" | "shotgunKing" | "darkWizard"
        );
    if !source_color_truth(state, "regency", color) {
        return native;
    }
    if crate::observation::truth(piece.extra.get("regencyHeir")) {
        return source_color_truth(state, "kingDead", color);
    }
    let queen_alive = state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|other| other.color == color && other.kind == "queen");
    native && !queen_alive
}

/// Frozen `isRoyalIdentityPiece`, which differs from the defeat/probe royal
/// predicates: democracy and a living Regency queen do not suppress this
/// physical identity. Herald, Racing King and Double Check use this predicate.
pub(crate) fn is_royal_identity_v7(state: &GameState, piece: &Piece) -> bool {
    crate::observation::truth(piece.extra.get("crownRoyal"))
        || crate::observation::truth(piece.extra.get("editorRoyal"))
        || matches!(
            piece.kind.as_str(),
            "king" | "royalKnight" | "shotgunKing" | "darkWizard"
        )
        || piece.kind == "merchant" && uses_september18_balance(state)
        || crate::observation::truth(piece.extra.get("regencyHeir"))
            && source_color_truth(state, "kingDead", piece.color)
            && source_color_truth(state, "regency", piece.color)
}

fn source_catalog_hash(state: &GameState) -> Option<&str> {
    let profile = if let Some(canonical) = state
        .extra
        .get("cardState")
        .filter(|value| crate::observation::truth(Some(value)))
    {
        canonical.get("profile")
    } else {
        state.extra.get("profile")
    };
    profile?
        .get("catalogHash")?
        .as_str()
        .filter(|hash| !hash.is_empty())
}

fn uses_september18_balance(state: &GameState) -> bool {
    source_catalog_hash(state).map_or_else(
        || state.extra.get("september18Balance") != Some(&Value::Bool(false)),
        |hash| {
            [
                "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
                "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
                "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs",
                "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc",
                "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g",
                "HVzv4vxzNKDRi-ylshTw9HnLmIlyiQhMScQ4caG_aSI",
            ]
            .contains(&hash)
        },
    )
}

fn herald_victory_v7(state: &GameState, actor: Color, threat_probe: bool) -> bool {
    state.board.iter().enumerate().any(|(row, cells)| {
        cells.iter().enumerate().any(|(col, cell)| {
            let Some(piece) = cell.as_ref().filter(|piece| piece.color == actor) else {
                return false;
            };
            if piece.ability_kind() != "herald"
                && !(piece.kind == "trickster"
                    && piece
                        .extra
                        .get("tricksterPreviousAbilityForTurn")
                        .and_then(Value::as_str)
                        == Some("herald"))
                || threat_probe
                    && crate::observation::truth(piece.extra.get("kingThreatSuppressed"))
            {
                return false;
            }
            let from = Square {
                row: row as u8,
                col: col as u8,
            };
            crate::movement::KING
                .iter()
                .filter_map(|&(dr, dc)| from.offset(dr, dc))
                .any(|target| {
                    state.at(target).is_some_and(|piece| {
                        piece.color == actor.opponent()
                            && (is_royal_identity_v7(state, piece) || piece.kind == "merchant")
                    })
                })
        })
    })
}

pub(crate) fn resolve_herald_for_color_v7(state: &mut GameState, actor: Color) -> Result<bool> {
    if !herald_victory_v7(state, actor, state.threat_probe_depth > 0) {
        return Ok(false);
    }
    crate::flow::end_game(
        state,
        Some(actor),
        "전령이 상대 킹과 협정을 이끌어냈습니다.",
    )?;
    Ok(true)
}

/// Source `checkRacingKings` checks both colors in their fixed order, rather
/// than choosing the moving side. On a collapsed board its goal moves inward
/// using the normalized current depth, and the first physical royal in board
/// order is the Racing King even if the side has several royal identities.
pub(crate) fn check_racing_kings_v7(state: &mut GameState) -> Result<bool> {
    crate::legal_profile::measure("game_over_racing_kings_check", || {
        check_racing_kings_v7_profiled(state)
    })
}

pub(crate) fn check_racing_kings_v7_profiled(state: &mut GameState) -> Result<bool> {
    if state.mode == "gameover" {
        return Ok(false);
    }
    let mut winners = Vec::new();
    for actor in [Color::White, Color::Black] {
        if !source_color_truth(state, "racingKing", actor)
            && !crate::observation::truth(state.extra.get("machoChess"))
        {
            continue;
        }
        let goal = if crate::observation::truth(state.extra.get("collapsed")) {
            let raw = crate::observation::number(state.extra.get("collapseDepth")).unwrap_or(0.0);
            let depth = (if raw == 0.0 { 1.0 } else { raw }).floor().clamp(0.0, 4.0) as usize;
            if actor == Color::White {
                depth
            } else {
                7usize.saturating_sub(depth)
            }
        } else if actor == Color::White {
            0
        } else {
            7
        };
        let royal_row = state.board.iter().enumerate().find_map(|(row, cells)| {
            cells
                .iter()
                .flatten()
                .any(|piece| piece.color == actor && is_royal_identity_v7(state, piece))
                .then_some(row)
        });
        if royal_row == Some(goal) {
            winners.push(actor);
        }
    }
    if winners.is_empty() {
        return Ok(false);
    }
    let draw = winners.len() == 2;
    crate::flow::end_game(
        state,
        if draw { None } else { winners.first().copied() },
        if draw {
            "양쪽 킹이 동시에 목표 랭크에 도달하여 무승부입니다."
        } else {
            "레이싱 킹이 목표 랭크에 도달했습니다."
        },
    )?;
    Ok(true)
}

pub(crate) fn resolve_herald_threats_v7(state: &mut GameState, actor: Color) -> Result<bool> {
    resolve_herald_threats_v7_with_probe(state, actor, state.threat_probe_depth > 0)
}

/// The source suppression flag belongs to its probe context, not to the wire
/// state. Pass the context explicitly when simulating a threat move.
pub(crate) fn resolve_herald_threats_v7_with_probe(
    state: &mut GameState,
    actor: Color,
    threat_probe: bool,
) -> Result<bool> {
    crate::legal_profile::measure("threat_herald", || {
        resolve_herald_threats_v7_with_probe_profiled(state, actor, threat_probe)
    })
}

pub(crate) fn resolve_herald_threats_v7_with_probe_profiled(
    state: &mut GameState,
    actor: Color,
    threat_probe: bool,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 Herald resolution requires the frozen v7 ruleset".into(),
        ));
    }
    if state.mode == "gameover" {
        return Ok(false);
    }
    for color in [actor, actor.opponent()] {
        if herald_victory_v7(state, color, threat_probe) {
            crate::flow::end_game(
                state,
                Some(color),
                if color == actor {
                    "전령이 상대 킹과 협정을 이끌어냈습니다."
                } else {
                    "상대 킹이 전령의 협정권에 들어왔습니다."
                },
            )?;
            return Ok(true);
        }
    }
    if check_racing_kings_v7(state)? {
        return Ok(true);
    }
    resolve_double_check_threats_v7(state, actor, threat_probe)
}

fn resolve_double_check_threats_v7(
    state: &mut GameState,
    actor: Color,
    threat_probe: bool,
) -> Result<bool> {
    for color in [actor, actor.opponent()] {
        if !source_color_truth(state, "binaMate", color) {
            continue;
        }
        if check_double_check_victory_v7(state, color, threat_probe)? {
            crate::flow::end_game(
                state,
                Some(color),
                "더블 체크: 서로 다른 아군 기물 2개가 동시에 상대 킹을 공격했습니다.",
            )?;
            return Ok(true);
        }
    }
    Ok(false)
}

fn check_double_check_victory_v7(
    state: &GameState,
    actor: Color,
    threat_probe: bool,
) -> Result<bool> {
    if !source_color_truth(state, "binaMate", actor) || state.mode == "gameover" {
        return Ok(false);
    }
    let hidden_attacker_ids =
        crate::observation::v7_hidden_opponent_piece_ids(state, actor.opponent())?;
    for (row, cells) in state.board.iter().enumerate() {
        for (col, cell) in cells.iter().enumerate() {
            let Some(royal) = cell.as_ref().filter(|piece| {
                piece.color == actor.opponent() && is_royal_identity_v7(state, piece)
            }) else {
                continue;
            };
            let target = Square {
                row: row as u8,
                col: col as u8,
            };
            let mut visited = BTreeSet::new();
            let mut checked = BTreeSet::new();
            for (attacker_row, cells) in state.board.iter().enumerate() {
                for (attacker_col, cell) in cells.iter().enumerate() {
                    let Some(piece) = cell.as_ref().filter(|piece| piece.color == actor) else {
                        continue;
                    };
                    if hidden_attacker_ids.contains(&piece.id)
                        || threat_probe
                            && crate::observation::truth(piece.extra.get("kingThreatSuppressed"))
                        || !visited.insert(piece.id.clone())
                    {
                        continue;
                    }
                    let from = normalize_piece_square_v7(
                        state,
                        piece,
                        Square {
                            row: attacker_row as u8,
                            col: attacker_col as u8,
                        },
                    );
                    if piece_can_attack_royal_for_double_check_v7(
                        state, piece, from, royal, target,
                    )? {
                        checked.insert(piece.id.clone());
                    }
                }
            }
            if checked.len() >= 2 {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn piece_can_attack_royal_for_double_check_v7(
    state: &GameState,
    piece: &Piece,
    from: Square,
    royal: &Piece,
    target: Square,
) -> Result<bool> {
    // In the September 22 rules this pure geometry return precedes every
    // frozen/disarmed/capture-policy gate. It is deliberately not derived
    // from the ordinary attack predicate or the UI legal highlight surface.
    if piece.color != royal.color
        && uses_september22_rules(state)
        && crate::movement::v7_double_check_geometry(state, piece, from, target)?
    {
        return Ok(true);
    }
    if piece.color == royal.color || !is_royal_identity_v7(state, royal) {
        return Ok(false);
    }
    let dice_lock = piece
        .color
        .owner()
        .and_then(|actor| state.extra.get("diceLocks")?.get(actor.as_str()));
    let dice_locked = dice_lock.is_some_and(|lock| {
        source_counter(lock.get("remaining")) > 0.0
            && if lock.get("type").and_then(Value::as_str) == Some("king") {
                is_royal_identity_v7(state, piece)
            } else {
                lock.get("type").and_then(Value::as_str) == Some(piece.kind.as_str())
            }
    });
    if crate::movement::frozen(piece)
        || source_counter(
            piece
                .extra
                .get("staked")
                .and_then(|entry| entry.get("remaining")),
        ) > 0.0
        || dice_locked
        || is_royal_identity_v7(state, piece)
            && crate::observation::truth(piece.extra.get("undergroundBunker"))
            && crate::observation::number(piece.extra.get("hp")).is_some()
        || source_counter(
            piece
                .extra
                .get("disarmed")
                .and_then(|entry| entry.get("remaining")),
        ) > 0.0
        || crate::observation::truth(piece.extra.get("repositionSecondMove"))
        || manner_capture_locked(state, piece)
        || initiative_capture_locked(state, piece)
    {
        return Ok(false);
    }
    if !crate::movement::v7_can_capture_target(
        state,
        piece,
        royal,
        false,
        basic_training_capture(state, piece, from, target),
    )? {
        return Ok(false);
    }
    if high_ground_capture_blocked(state, piece, from, target)? {
        return Ok(false);
    }
    let options = crate::movement::V7MoveOptions {
        ignore_forced_turn_move: true,
        ignore_global_capture_force: true,
        ignore_scarecrow_force: true,
        ..crate::movement::V7MoveOptions::default()
    };
    if piece.kind != "shotgunKing" {
        return crate::movement::v7_legal_move_targets(state, piece, from, options)?
            .iter()
            .try_fold(false, |found, move_| {
                Ok(found || move_threatens_royal_for_double_check(move_, from, target)?)
            });
    }
    let mut choices = vec![Value::Null];
    let ammo = source_counter(piece.extra.get("ammo"));
    if ammo >= 2.0 {
        choices.push(json!("shotgun"));
    }
    if ammo >= 3.0 {
        choices.push(json!("snipe"));
    }
    let mut probe = state.clone();
    for choice in choices {
        probe.extra.insert("shotgunAction".into(), choice);
        for move_ in crate::movement::v7_legal_move_targets(&probe, piece, from, options)? {
            if move_threatens_royal_for_double_check(&move_, from, target)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(crate) fn uses_september22_rules(state: &GameState) -> bool {
    source_catalog_hash(state).map_or_else(
        || state.extra.get("scarecrowPieceReservation") != Some(&Value::Bool(false)),
        |hash| {
            [
                "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
                "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
                "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs",
                "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc",
                "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g",
            ]
            .contains(&hash)
        },
    )
}

fn move_threatens_royal_for_double_check(
    move_: &crate::MoveTarget,
    from: Square,
    target: Square,
) -> Result<bool> {
    if move_.flag("shotgunBlast") {
        let direction = move_.flags.get("shotgunDirection").ok_or_else(|| {
            EngineError::InvalidState("v7 shotgun threat missing direction".into())
        })?;
        return shotgun_direction_attacks_square(from, target, direction);
    }
    Ok(source_move_threatens_square(move_, target))
}

/// main2402: attack highlights include portal transit and capture metadata,
/// while stationary non-capture actions do not attack their clicked square.
fn source_move_threatens_square(move_: &crate::MoveTarget, target: Square) -> bool {
    let flag = |field: &str| crate::observation::truth(move_.flags.get(field));
    let direct = ![
        "shotgunBlast",
        "setLogDirection",
        "substitutionSwap",
        "dragonSwap",
        "switcherooMove",
        "merchantBuy",
    ]
    .iter()
    .any(|field| flag(field));
    if direct && move_.square() == target {
        return true;
    }
    if ["portalEntry", "portalExit", "jumpCapture"]
        .iter()
        .any(|field| {
            move_
                .flags
                .get(*field)
                .is_some_and(|cell| matches_square(cell, target))
        })
        || move_.flags.get("capturedRow").and_then(Value::as_i64) == Some(i64::from(target.row))
            && move_.flags.get("capturedCol").and_then(Value::as_i64) == Some(i64::from(target.col))
    {
        return true;
    }
    for field in [
        "sectorCells",
        "colossusLandingCaptures",
        "bigRookLandingCaptures",
    ] {
        if move_
            .flags
            .get(field)
            .and_then(Value::as_array)
            .is_some_and(|cells| cells.iter().any(|cell| matches_square(cell, target)))
        {
            return true;
        }
    }
    (flag("shotgunBlast") || flag("colossusAttack"))
        && move_
            .flags
            .get("highlightCells")
            .and_then(Value::as_array)
            .is_some_and(|cells| cells.iter().any(|cell| matches_square(cell, target)))
}

fn validate_automatic_reaction_schema(state: &GameState) -> Result<()> {
    // These location-bound callbacks now run in the source-ordered common
    // transition and record royal removal causes in its private probe scope.
    // Keep malformed queue diagnostics, rather than blocking a royal cell.
    if let Some(hazards) = state
        .extra
        .get("delayedHazards")
        .filter(|value| !value.is_null())
    {
        let hazards = hazards.as_array().ok_or_else(|| {
            EngineError::InvalidState("v7 delayedHazards must be an array".into())
        })?;
        for hazard in hazards {
            let Some(cells) = hazard.get("cells") else {
                continue;
            };
            let cells = cells.as_array().ok_or_else(|| {
                EngineError::InvalidState("v7 delayedHazards cells must be an array".into())
            })?;
            for cell in cells {
                source_cell(cell)?;
            }
        }
    }
    if let Some(pending) = state
        .extra
        .get("pendingOtherworld")
        .filter(|value| !value.is_null())
    {
        let pending = pending.as_array().ok_or_else(|| {
            EngineError::InvalidState("v7 pendingOtherworld must be an array".into())
        })?;
        for entry in pending {
            source_cell(entry)?;
        }
    }
    // Log collision, incoming Siren/Brutus, and Witch Trial now use shared
    // capture/removal causes in their exact transition stages. An incomplete
    // active move or capture policy is reported by that specific owner.
    // Regency changes which existing identity is royal; `royal_ids` already
    // applies that predicate and succession runs at royal capture.
    // Quiet File Surge/Rook Lift moves enter relevant_move below, and the
    // common transition starts their source continuation after its middle
    // callbacks. collect_capture_entry_keys then searches the retained turn.
    Ok(())
}

fn source_cell(value: &Value) -> Result<(u64, u64)> {
    let row = value.get("row").and_then(Value::as_u64).ok_or_else(|| {
        EngineError::InvalidState(
            "v7 royal threat reaction row must be a nonnegative integer".into(),
        )
    })?;
    let col = value.get("col").and_then(Value::as_u64).ok_or_else(|| {
        EngineError::InvalidState(
            "v7 royal threat reaction col must be a nonnegative integer".into(),
        )
    })?;
    Ok((row, col))
}

fn relevant_move(
    state: &GameState,
    action: &Action,
    attacker: &Piece,
    royals: &BTreeSet<String>,
) -> Result<bool> {
    let target = action
        .destination
        .as_ref()
        .ok_or(EngineError::IllegalAction)?;
    for square in crate::movement::v7_move_capture_target_cells(target)? {
        let Some(victim) = state.at(square) else {
            continue;
        };
        if royals.contains(&victim.id) {
            return Ok(true);
        }
    }
    // Source `kingThreatActionIsRelevant` applies `isCaptureMove` after the
    // royal-ID cell check. That classifier intentionally ignores capture
    // locks used later for legality; a restricted capture can still be a
    // relevant simulation candidate. Reuse the movement owner's exact v7
    // classifier rather than the v6 `can_capture` shortcut.
    if crate::movement::v7_is_capture_move(state, attacker, target)? {
        return Ok(true);
    }
    let destination = source_move_destination(target)?;
    if attacker.kind == "knight" {
        return Ok(target.flag("madHorseCapture")
            || source_color_truth(state, "fileSurge", attacker.color)
                && !crate::observation::truth(attacker.extra.get("thiefSecondMove"))
                && !crate::observation::truth(attacker.extra.get("fileSurgeSecondMove"))
                && [0, 7].contains(&destination.col));
    }
    Ok(attacker.kind == "rook"
        && source_color_truth(state, "rookLift", attacker.color)
        && !crate::observation::truth(attacker.extra.get("rookLiftSecondMove"))
        && [0, 7].contains(&destination.row)
        && [0, 7].contains(&destination.col))
}

fn source_move_destination(target: &crate::MoveTarget) -> Result<Square> {
    let descriptor = if target.flag("portalLanding") {
        target.flags.get("portalExit")
    } else {
        None
    };
    let pair = descriptor
        .map(|value| {
            value
                .get("row")
                .and_then(Value::as_i64)
                .zip(value.get("col").and_then(Value::as_i64))
        })
        .unwrap_or_else(|| {
            target
                .flags
                .get("anchorRow")
                .and_then(Value::as_i64)
                .zip(target.flags.get("anchorCol").and_then(Value::as_i64))
        });
    if let Some((row, col)) = pair {
        if !(0..8).contains(&row) || !(0..8).contains(&col) {
            return Err(EngineError::InvalidState(
                "v7 threat move destination is outside the source board".into(),
            ));
        }
        return Ok(Square {
            row: row as u8,
            col: col as u8,
        });
    }
    Ok(target.square())
}

/// Source `isSquareAttacked`, independently of `getLegalMoves`. The source
/// uses the actual board target unless a caller supplies a replacement piece.
/// Errors stay visible to placement, castling and fog callers: an unsupported
/// attack branch must never be interpreted as a safe or invisible square.
pub(crate) fn is_square_attacked_v7(
    state: &GameState,
    target: Square,
    by: Color,
    target_override: Option<&Piece>,
    ignored_attacker_ids: Option<&BTreeSet<String>>,
) -> Result<bool> {
    crate::legal_profile::measure("threat_square_attacked", || {
        is_square_attacked_v7_profiled(state, target, by, target_override, ignored_attacker_ids)
    })
}

pub(crate) fn is_square_attacked_v7_profiled(
    state: &GameState,
    target: Square,
    by: Color,
    target_override: Option<&Piece>,
    ignored_attacker_ids: Option<&BTreeSet<String>>,
) -> Result<bool> {
    require_attack_geometry(state, target)?;
    for (row, cells) in state.board.iter().enumerate() {
        for (col, candidate) in cells.iter().enumerate() {
            let Some(piece) = candidate
                .as_ref()
                .filter(|piece| piece.color == by && piece.kind != "wall")
            else {
                continue;
            };
            if ignored_attacker_ids.is_some_and(|ids| ids.contains(&piece.id)) {
                continue;
            }
            let from = Square {
                row: row as u8,
                col: col as u8,
            };
            if piece_attacks_square_inner(state, piece, from, target, target_override, false)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// The single-piece predicate is also the source's fog-of-war attack surface.
/// It is pure and does not recursively query visibility or royal danger.
pub(crate) fn piece_attacks_square_v7(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
) -> Result<bool> {
    require_attack_geometry(state, from)?;
    require_attack_geometry(state, target)?;
    piece_attacks_square_inner(state, piece, from, target, None, false)
}

fn require_attack_geometry(state: &GameState, square: Square) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 attack predicate requires the frozen v7 ruleset".into(),
        ));
    }
    if state.board.len() != 8
        || state.board.iter().any(|row| row.len() != 8)
        || square.row >= 8
        || square.col >= 8
    {
        return Err(EngineError::InvalidState(
            "v7 attack predicate requires an 8x8 board and in-bounds square".into(),
        ));
    }
    Ok(())
}

fn source_side_truth(state: &GameState, field: &str, piece: &Piece) -> bool {
    source_color_truth(state, field, piece.color)
}

fn source_color_truth(state: &GameState, field: &str, color: impl Into<crate::PieceColor>) -> bool {
    let color = color.into();
    crate::observation::truth(
        state
            .extra
            .get(field)
            .and_then(|value| value.get(color.as_str())),
    )
}

fn source_side_number(state: &GameState, field: &str, piece: &Piece) -> f64 {
    piece
        .color
        .owner()
        .and_then(|actor| {
            crate::observation::number(
                state
                    .extra
                    .get(field)
                    .and_then(|value| value.get(actor.as_str())),
            )
        })
        .unwrap_or(0.0)
}

fn source_owner_turns(state: &GameState, piece: &Piece) -> f64 {
    piece
        .color
        .owner()
        .map_or(0.0, |actor| f64::from(*state.turns_taken.get(actor)))
}

fn source_counter(value: Option<&Value>) -> f64 {
    crate::observation::number(value).unwrap_or(0.0)
}

fn fresh_capture_locked(state: &GameState, piece: &Piece) -> bool {
    source_owner_turns(state, piece) < source_counter(piece.extra.get("freshNoCaptureUntil"))
}

/// Source `isMannerCaptureLocked` in the headless game-owned profile. The
/// browser's Mad-AI single-player toggle is not a game Position authority.
pub(crate) fn manner_capture_locked(state: &GameState, piece: &Piece) -> bool {
    if crate::observation::truth(piece.extra.get("repositionSecondMove"))
        || fresh_capture_locked(state, piece)
    {
        return true;
    }
    if piece.kind == "monster" {
        return false;
    }
    if source_side_truth(state, "freeMoveCaptureLock", piece)
        || source_owner_turns(state, piece) < source_counter(piece.extra.get("cardNoCaptureUntil"))
        || source_owner_turns(state, piece) < source_counter(piece.extra.get("promotionRushUntil"))
        || (crate::observation::truth(state.extra.get("coolGuy"))
            || crate::observation::truth(piece.extra.get("potionManner")))
            && crate::observation::truth(piece.extra.get("coolGuyCapturedLast"))
    {
        return true;
    }
    if matches!(piece.kind.as_str(), "checker" | "checkerKing") {
        return false;
    }
    source_side_truth(state, "quantumPending", piece)
        || source_owner_turns(state, piece)
            < source_counter(piece.extra.get("quantumNoCaptureUntil"))
}

fn initiative_capture_locked(state: &GameState, piece: &Piece) -> bool {
    let Some(entry) = piece
        .color
        .owner()
        .and_then(|actor| state.extra.get("initiative")?.get(actor.as_str()))
        .filter(|entry| crate::observation::truth(Some(entry)))
    else {
        return false;
    };
    let start = source_counter(entry.get("startTurn"));
    let limit = source_counter(entry.get("limit"));
    source_owner_turns(state, piece) - start < if limit == 0.0 { 7.0 } else { limit }
}

fn uses_revolving_door_guard(state: &GameState) -> bool {
    source_catalog_hash(state).map_or_else(
        || {
            if crate::observation::truth(state.extra.get("campaign")) {
                state.extra.get("revolvingDoorGuard") == Some(&Value::Bool(true))
            } else {
                state.extra.get("revolvingDoorGuard") != Some(&Value::Bool(false))
            }
        },
        |hash| {
            [
                "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
                "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
            ]
            .contains(&hash)
        },
    )
}

fn guard_like(state: &GameState, piece: &Piece) -> bool {
    piece.ability_kind() == "guard"
        || piece.ability_kind() == "revolvingDoor" && uses_revolving_door_guard(state)
}

fn royal_command_capture_access(state: &GameState, piece: &Piece) -> bool {
    if crate::observation::truth(piece.extra.get("crownBearer")) {
        return true;
    }
    if source_side_number(state, "socialism", piece) > 0.0
        && piece.kind != "crown"
        && (piece.kind == "merchant" && !uses_september18_balance(state)
            || !is_royal_identity_v7(state, piece))
    {
        return false;
    }
    if crate::observation::truth(piece.extra.get("royalCommand")) {
        return true;
    }
    let Some(window) = piece
        .color
        .owner()
        .and_then(|actor| state.extra.get("royalCommand")?.get(actor.as_str()))
    else {
        return false;
    };
    let Some(start) = crate::observation::number(window.get("activeTurn"))
        .filter(|value| *value >= 0.0 && value.fract() == 0.0)
    else {
        return false;
    };
    let end = crate::observation::number(window.get("expiresTurn"))
        .filter(|value| *value > start && value.fract() == 0.0)
        .unwrap_or(start + 1.0);
    let turn = source_owner_turns(state, piece);
    start <= turn && turn < end
}

fn basic_training_capture(state: &GameState, piece: &Piece, from: Square, target: Square) -> bool {
    let Some(actor) = piece.color.owner() else {
        return false;
    };
    if !crate::observation::truth(piece.extra.get("basicTraining"))
        || piece.is_large()
        || matches!(piece.kind.as_str(), "wall" | "football" | "blackHole")
        || piece.ability_kind() == "missionary"
        || !crate::observation::truth(piece.extra.get("potionBasicTraining"))
            && [
                "pawn",
                "king",
                "queen",
                "primeMinister",
                "jester",
                "guard",
                "amazon",
                "man",
                "idol",
                "babyBear",
                "bear",
            ]
            .contains(&piece.kind.as_str())
    {
        return false;
    }
    let dr = i16::from(target.row) - i16::from(from.row);
    let dc = i16::from(target.col) - i16::from(from.col);
    (dr == i16::from(actor.pawn_dir())
        || source_color_truth(state, "retreat", actor) && dr == -i16::from(actor.pawn_dir()))
        && dc.abs() == 1
}

fn nullification_blocks(state: &GameState, attacker: &Piece, target: &Piece) -> bool {
    fn identity(piece: &Piece) -> &str {
        if ["regencyHeir", "crownRoyal", "editorRoyal"]
            .iter()
            .any(|field| crate::observation::truth(piece.extra.get(*field)))
            || [
                "king",
                "royalKnight",
                "shotgunKing",
                "darkWizard",
                "merchant",
            ]
            .contains(&piece.kind.as_str())
        {
            "king"
        } else {
            piece.kind.as_str()
        }
    }
    let _ = state;
    target.kind != "scarecrow"
        && crate::observation::truth(target.extra.get("nullification"))
        && attacker.color != target.color
        && identity(attacker) == identity(target)
}

fn piece_attacks_square_inner(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
    override_piece: Option<&Piece>,
    skip_imperial: bool,
) -> Result<bool> {
    if piece.ability_kind() == "campfire" {
        return Ok(false);
    }
    let victim = override_piece.or_else(|| state.at(target));
    let basic = basic_training_capture(state, piece, from, target);
    let royal_command = royal_command_capture_access(state, piece);
    if piece.kind == "darkWizard" && crate::observation::truth(piece.extra.get("darkMagicCircle")) {
        let circle = &piece.extra["darkMagicCircle"];
        let row = circle
            .get("centerRow")
            .and_then(Value::as_i64)
            .unwrap_or(i64::from(from.row));
        let col = circle
            .get("centerCol")
            .and_then(Value::as_i64)
            .unwrap_or(i64::from(from.col));
        if (i64::from(target.row) - row).abs() > 1 || (i64::from(target.col) - col).abs() > 1 {
            return Ok(false);
        }
    }
    if victim.is_some_and(|victim| nullification_blocks(state, piece, victim))
        || victim.is_some_and(|victim| crate::observation::truth(victim.extra.get("submerged")))
        || (piece.ability_kind() == "recruiter" || guard_like(state, piece))
            && !basic
            && !royal_command
        || crate::movement::frozen(piece)
        || source_counter(piece.extra.get("poisonStunTurns"))
            .floor()
            .max(0.0)
            > 0.0
        || source_counter(
            piece
                .extra
                .get("staked")
                .and_then(|entry| entry.get("remaining")),
        ) > 0.0
        || piece.ability_kind() != "missionary"
            && victim.is_some_and(|victim| {
                piece.color.owner().is_some()
                    && victim.color.owner().is_some()
                    && victim.color != piece.color
                    && source_counter(
                        state
                            .extra
                            .get("armistice")
                            .map(|entry| entry.get("remaining").unwrap_or(entry)),
                    )
                    .floor()
                    .max(0.0)
                        > 0.0
            })
        || manner_capture_locked(state, piece)
        || (crate::observation::truth(state.extra.get("saturationRule"))
            || crate::observation::truth(piece.extra.get("potionSaturation")))
            && source_counter(piece.extra.get("capturesMade")) >= 3.0
        || initiative_capture_locked(state, piece)
        || victim.is_some_and(|victim| {
            piece.color.owner().is_some_and(|actor| {
                state
                    .extra
                    .get("freeMoveCaptureLock")
                    .and_then(|sides| sides.get(actor.as_str()))
                    == Some(&Value::Bool(true))
                    && victim.color == actor.opponent()
            })
        })
        || source_counter(state.extra.get("chaosNoCaptureUntilHalfTurn"))
            > f64::from(state.turns_taken.white) + f64::from(state.turns_taken.black)
        || victim.is_some_and(|victim| {
            guard_like(state, victim)
                || victim
                    .extra
                    .get("captureRestriction")
                    .and_then(Value::as_str)
                    == Some("immune")
                || crate::movement::frozen(victim)
                || matches!(piece.kind.as_str(), "colossus" | "shotgunKing")
                    && victim.ability_kind() == "jester"
        })
    {
        return Ok(false);
    }
    // The source applies Basic Training's full capture policy only on its
    // extra pawn capture, rather than on every ordinary geometric attack.
    if basic
        && let Some(victim) = victim
        && !crate::movement::v7_can_capture_target(state, piece, victim, false, true)?
    {
        return Ok(false);
    }
    if state
        .extra
        .get("campaign")
        .and_then(|campaign| campaign.get("setup"))
        .and_then(Value::as_str)
        == Some("timeTraveler")
        && let Some(victim) =
            victim.filter(|victim| !matches!(victim.kind.as_str(), "wall" | "football"))
    {
        let phase = |piece: &Piece| match piece.extra.get("timePhase").and_then(Value::as_str) {
            Some("past") => "past",
            _ => "future",
        };
        if phase(piece) != phase(victim) {
            return Ok(false);
        }
    }
    if high_ground_capture_blocked(state, piece, from, target)? {
        return Ok(false);
    }
    let dr = i16::from(target.row) - i16::from(from.row);
    let dc = i16::from(target.col) - i16::from(from.col);
    let ad_r = dr.abs();
    let ad_c = dc.abs();
    if matches!(piece.kind.as_str(), "colossus" | "bigRook" | "bigBishop") {
        return variant_attack_contains(state, piece, from, target);
    }
    if is_royal_identity_v7(state, piece)
        && crate::observation::truth(piece.extra.get("undergroundBunker"))
        && crate::observation::number(piece.extra.get("hp")).is_some()
    {
        return Ok(false);
    }
    if attack_highway_reaches(state, piece, from, target, victim)? {
        return Ok(true);
    }
    if !skip_imperial
        && is_royal_identity_v7(state, piece)
        && source_side_truth(state, "imperialStudies", piece)
        && piece
            .extra
            .get("imperialMoves")
            .and_then(Value::as_array)
            .is_some_and(|moves| !moves.is_empty())
        && imperial_study_attacks_square(state, piece, from, target, override_piece)?
    {
        return Ok(true);
    }
    if is_royal_identity_v7(state, piece)
        && source_side_truth(state, "killerKing", piece)
        && killer_king_attacks_square(state, piece, from, target)?
    {
        return Ok(true);
    }
    let knight = || {
        crate::variant_movement::knight_deltas_for_move(state, piece, from).map(|deltas| {
            deltas
                .into_iter()
                .any(|(r, c)| dr == i16::from(r) && dc == i16::from(c))
        })
    };
    let king = ad_r.max(ad_c) == 1;
    let queen_ray = || attack_rays_reach(state, piece, from, target, crate::movement::KING);
    let rook_ray = || attack_rays_reach(state, piece, from, target, crate::movement::ORTHO);
    let bishop_ray = || attack_rays_reach(state, piece, from, target, crate::movement::DIAG);
    let orthogonal = dr == 0 && dc != 0 || dc == 0 && dr != 0;
    let diagonal = ad_r == ad_c && ad_r > 0;
    let center = (3..=4).contains(&from.row) && (3..=4).contains(&from.col);
    if is_royal_identity_v7(state, piece)
        && source_side_truth(state, "hillKing", piece)
        && center
        && knight()?
    {
        return Ok(true);
    }
    if crate::observation::truth(piece.extra.get("regencyHeir"))
        && source_color_truth(state, "kingDead", piece.color)
        && source_color_truth(state, "regency", piece.color)
        && source_side_truth(state, "kingKnight", piece)
    {
        return Ok(king
            || knight()?
            || source_side_truth(state, "hillKing", piece) && center && queen_ray()?);
    }
    if !uses_vanguard_diagonal_step(state)
        && is_vanguard_pawn(state, piece, from)
        && piece
            .color
            .owner()
            .is_some_and(|actor| dr == i16::from(actor.pawn_dir()))
        && ad_c <= 1
    {
        return Ok(true);
    }
    let attacked = match piece.kind.as_str() {
        "pawn" | "squire" | "standardBearer" => {
            let actor = piece.color.owner().ok_or(EngineError::IllegalAction)?;
            let direction = i16::from(actor.pawn_dir());
            let adjacent_knightmasters: Vec<_> = crate::movement::KING
                .iter()
                .filter_map(|&(r, c)| from.offset(r, c))
                .filter_map(|at| state.at(at))
                .filter(|ally| ally.color == actor && ally.ability_kind() == "knightmaster")
                .collect();
            if piece.kind == "pawn" && !adjacent_knightmasters.is_empty() {
                return Ok(adjacent_knightmasters
                    .into_iter()
                    .any(|ally| !fresh_capture_locked(state, ally))
                    && knight()?);
            }
            let forward =
                dr == direction || source_side_truth(state, "retreat", piece) && dr == -direction;
            let ordinary = forward
                && if piece.kind == "pawn" && source_side_truth(state, "pawnConversion", piece) {
                    dc == 0
                } else {
                    ad_c == 1
                };
            let same_rank_standard = state.board[usize::from(from.row)]
                .iter()
                .flatten()
                .any(|ally| ally.color == actor && ally.ability_kind() == "standardBearer");
            let capture_ready_standard =
                state.board[usize::from(from.row)]
                    .iter()
                    .flatten()
                    .any(|ally| {
                        ally.color == actor
                            && ally.ability_kind() == "standardBearer"
                            && !fresh_capture_locked(state, ally)
                    });
            ordinary
                || (piece.kind == "standardBearer" || piece.kind == "pawn" && same_rank_standard)
                    && capture_ready_standard
                    && dr == 0
                    && ad_c == 1
        }
        _ if basic => true,
        "knight" => {
            knight()?
                || source_side_truth(state, "cornerKick", piece)
                    && crate::movement::v7_is_board_corner(from)
                    && diagonal
                    && bishop_ray()?
        }
        "royalKnight" => {
            knight()?
                || source_side_truth(state, "royalKnightKing", piece) && king
                || source_side_truth(state, "hillKing", piece) && center && queen_ray()?
        }
        "king" => {
            king || source_side_truth(state, "kingKnight", piece) && knight()?
                || source_side_truth(state, "hillKing", piece) && center && queen_ray()?
        }
        "man" | "guard" | "reaper" | "siren" | "undead" | "vip" | "crown" => king,
        "recruiter" => royal_command && king,
        "knightmaster" | "ferz" => ad_r == 1 && ad_c == 1,
        "camel" => ad_r == 3 && ad_c == 1 || ad_r == 1 && ad_c == 3,
        "alfil" => ad_r == 2 && ad_c == 2,
        "eagle" => (ad_r == 0 || ad_r == 2) && (ad_c == 0 || ad_c == 2) && (ad_r != 0 || ad_c != 0),
        "dragon" | "pegasus" => knight()?,
        "assassin" => {
            knight()?
                || state
                    .at(target)
                    .is_some_and(|victim| is_royal_identity_v7(state, victim))
                    && queen_ray()?
        }
        "darkWizard" => {
            if crate::observation::truth(piece.extra.get("darkMagicCircle")) {
                ad_r + ad_c == 1
            } else {
                king
            }
        }
        "missionary" => {
            ad_r == 1
                && ad_c == 1
                && victim.is_some_and(|victim| {
                    victim.color.owner().is_some() && victim.color != piece.color
                })
        }
        "timeTraveler" => {
            state
                .extra
                .get("campaign")
                .filter(|campaign| {
                    campaign.get("setup").and_then(Value::as_str) == Some("timeTraveler")
                })
                .and_then(|campaign| campaign.get("timeTraveler"))
                .and_then(|traveler| traveler.get("attackEnabledFor"))
                .and_then(Value::as_str)
                == Some(piece.color.as_str())
                && king
        }
        "lobster" => {
            dr == i16::from(
                piece
                    .color
                    .owner()
                    .ok_or(EngineError::IllegalAction)?
                    .pawn_dir(),
            ) && ad_c <= 1
        }
        "slime" => orthogonal && ad_r + ad_c == 3,
        "hedgehog" => {
            source_owner_turns(state, piece)
                >= source_counter(piece.extra.get("bearMoveLockedUntilTurn"))
                && king
        }
        "magicGirl" => {
            if source_side_truth(state, "magicGirlSurge", piece) {
                knight()? || queen_ray()?
            } else {
                king
            }
        }
        "berserker" => {
            let allies = state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|ally| ally.color == piece.color)
                .map(|ally| ally.id.as_str())
                .collect::<BTreeSet<_>>()
                .len();
            if allies <= 5 {
                knight()? || queen_ray()?
            } else {
                king || allies <= 9 && rook_ray()?
            }
        }
        "princess" => {
            if state.board.iter().flatten().flatten().any(|ally| {
                ally.color == piece.color
                    && ally.kind == "queen"
                    && ally.extra.get("regencyHeir") != Some(&Value::Bool(true))
            }) {
                ad_r == 1 && ad_c == 1
            } else {
                queen_ray()?
            }
        }
        "vampireLord" => {
            if blood_moon_night_movement(state, piece) {
                knight()? || queen_ray()?
            } else {
                king
            }
        }
        "bat" => {
            if blood_moon_night_movement(state, piece) {
                crate::v7_special_piece_moves::raw_prime_minister_moves(state, piece, from)?
                    .iter()
                    .any(|move_| move_.square() == target)
            } else {
                orthogonal && ad_r.max(ad_c) <= 2 && rook_ray()?
            }
        }
        "bishop" => {
            let ray_shape = if source_side_truth(state, "reversal", piece) {
                orthogonal
            } else {
                diagonal
            };
            (if source_side_truth(state, "reversal", piece) {
                rook_ray()?
            } else {
                bishop_ray()?
            }) || ray_shape
                && source_side_truth(state, "bishopSnipe", piece)
                && attack_ray_reaches(state, piece, from, target, true)?
        }
        "rook" => {
            if source_side_truth(state, "reversal", piece) {
                bishop_ray()?
            } else {
                rook_ray()?
            }
        }
        "queen" | "bear" => queen_ray()?,
        "amazon" => knight()? || queen_ray()?,
        "windmill" => {
            if piece.extra.get("windmillMode").and_then(Value::as_str) == Some("rook") {
                rook_ray()?
            } else {
                bishop_ray()?
            }
        }
        "jester" => {
            state.at(target).is_some_and(|victim| {
                victim.kind != "jester"
                    && (is_royal_identity_v7(state, victim) || victim.kind == "merchant")
            }) && queen_ray()?
        }
        "shotgunKing" => shotgun_attacks_square(state, piece, from, target)?,
        "checker" | "checkerKing" => checker_attacks_square(state, piece, from, target, victim)?,
        "grasshopper" | "fanatic" | "protestant" | "primeMinister" | "cardinal" | "cannon"
        | "hook" | "trickster" | "siegeRam" | "colossus" | "bigRook" | "bigBishop" => {
            variant_attack_contains(state, piece, from, target)?
        }
        "wall" | "football" | "blackHole" | "coffin" | "campfire" | "wizard" | "idol"
        | "herald" | "merchant" | "log" | "babyBear" | "scarecrow" | "timeAfterimage"
        | "parrot" | "medium" | "paladin" | "unicorn" | "octopus" | "donQuixote"
        | "revolvingDoor" | "clockwork" | "brutus" | "grappler" | "thief" | "alibaba"
        | "monster" => false,
        other => {
            return Err(EngineError::UnsupportedFeature(format!(
                "v7 pieceAttacksSquare piece kind {other}"
            )));
        }
    };
    Ok(attacked)
}

fn matches_square(value: &Value, square: Square) -> bool {
    // Source coordinate metadata is compared with ===. Numeric strings and
    // booleans cannot silently become capture cells or terrain coordinates.
    value.get("row").and_then(Value::as_f64) == Some(f64::from(square.row))
        && value.get("col").and_then(Value::as_f64) == Some(f64::from(square.col))
}

fn high_ground_capture_blocked(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
) -> Result<bool> {
    if !state
        .extra
        .get("highGround")
        .and_then(Value::as_array)
        .is_some_and(|cells| {
            !cells.iter().any(|cell| matches_square(cell, from))
                && cells.iter().any(|cell| matches_square(cell, target))
        })
    {
        return Ok(false);
    }
    Ok(state
        .at(target)
        .is_some_and(|victim| victim.color != piece.color)
        && crate::movement::v7_is_capture_move(state, piece, &crate::MoveTarget::at(target))?)
}

fn uses_vanguard_diagonal_step(state: &GameState) -> bool {
    let Some(hash) = source_catalog_hash(state) else {
        return state.extra.get("vanguardDiagonalOnly") != Some(&Value::Bool(false));
    };
    uses_september18_balance(state)
        || [
            "jkECTP8OBtMeXmZF7wmz1dmgw0mgTFn8jdD-d9LZhxU",
            "s0_j9SmUy1tSUcko_X3I32akB93Iz2bvV1Cn7Nw4Uoo",
            "_Hw8otJVztzWQyIN69bg5khIqcJv-au6UwAcOQhtKcM",
            "Vj80kM6RlfZvbMo9kevioRks2VbcDGk8bASi5uz0yNA",
            "MM-bvPG6PYiUQ0-UrCM3GmEbFfBMTyPxjKk0vFXbXj0",
            "abhSgcd4RVrr2b-bzPtaJo6gbqfUrYKk4IqsrW7oCO0",
            "jvr27l0Kk-YFld45zoyTV-MMOIAXuIXtubma6eKOPL4",
            "uIkywAndqkm8YawR1KMkyw_GROmXd4h1wpCXwH-0oYM",
            "RzvDge9_4q7il_D_pgjgO-vg2cu7Njx-VGcV_0ZeRLA",
            "Rq32ku1EGlC0GbWIZx5RTtxP43JwU2dz1dIrPWElXRM",
            "DxKZRNW24FynvEKBMzBDT7B1AH1e0BZFU-Ixpm1LtoM",
        ]
        .contains(&hash)
}

fn is_vanguard_pawn(state: &GameState, piece: &Piece, from: Square) -> bool {
    if piece.kind != "pawn"
        || piece.color.owner().is_none()
        || !source_side_truth(state, "vanguard", piece)
    {
        return false;
    }
    let mut same_rank = 0;
    for (row, cells) in state.board.iter().enumerate() {
        for _ in cells
            .iter()
            .flatten()
            .filter(|pawn| pawn.kind == "pawn" && pawn.color == piece.color)
        {
            if row == usize::from(from.row) {
                same_rank += 1;
            }
            if piece.color == Color::White && row < usize::from(from.row)
                || piece.color == Color::Black && row > usize::from(from.row)
            {
                return false;
            }
        }
    }
    same_rank == 1
}

fn imperial_study_attacks_square(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
    override_piece: Option<&Piece>,
) -> Result<bool> {
    let excluded = [
        "bigBishop",
        "",
        "king",
        "royalKnight",
        "shotgunKing",
        "merchant",
        "recruiter",
        "wall",
        "scarecrow",
        "football",
        "blackHole",
        "colossus",
        "bigRook",
        "coffin",
        "log",
        "timeTraveler",
        "wizard",
    ];
    let mut seen = BTreeSet::new();
    for value in piece
        .extra
        .get("imperialMoves")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let kind = value.as_str().ok_or_else(|| {
            EngineError::InvalidState(
                "v7 imperialMoves requires canonical source piece type strings".into(),
            )
        })?;
        if excluded.contains(&kind) || !seen.insert(kind) {
            continue;
        }
        // Source temporarily changes the same object, so board-dependent raw
        // kernels must observe the learned kind as well as the direct argument.
        let mut probe = state.clone();
        let mut learned = piece.clone();
        learned.kind = kind.into();
        if kind == "windmill" && !crate::observation::truth(learned.extra.get("windmillMode")) {
            learned.extra.insert("windmillMode".into(), json!("bishop"));
        }
        for cell in probe.board.iter_mut().flatten().flatten() {
            if !piece.id.is_empty() && cell.id == piece.id {
                *cell = learned.clone();
            }
        }
        if piece.id.is_empty() {
            probe.board[from.row as usize][from.col as usize] = Some(learned.clone());
        }
        if piece_attacks_square_inner(&probe, &learned, from, target, override_piece, true)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn killer_king_attacks_square(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
) -> Result<bool> {
    if piece.ability_kind() == "slime" {
        return Ok(false);
    }
    for &(dr, dc) in crate::movement::ORTHO {
        let mut cell = from;
        for _ in 0..8 {
            let Some(next) = cell.offset(dr, dc) else {
                break;
            };
            cell = next;
            let Some(victim) = state.at(cell) else {
                continue;
            };
            if cell == target
                && victim.color != piece.color
                && is_royal_identity_v7(state, victim)
                && crate::movement::v7_can_capture_target(state, piece, victim, false, false)?
            {
                return Ok(true);
            }
            break;
        }
    }
    Ok(false)
}

fn checker_attacks_square(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
    victim: Option<&Piece>,
) -> Result<bool> {
    let dr = i16::from(target.row) - i16::from(from.row);
    let dc = i16::from(target.col) - i16::from(from.col);
    if dc.abs() != 1
        || if piece.kind == "checkerKing" {
            dr.abs() != 1
        } else {
            dr != if piece.color == Color::White { -1 } else { 1 }
        }
    {
        return Ok(false);
    }
    let Some(landing) = target.offset(dr as i8, dc as i8) else {
        return Ok(false);
    };
    if state.at(landing).is_some() {
        return Ok(false);
    }
    match victim {
        None => Ok(true),
        Some(victim) => Ok(crate::movement::v7_can_capture_target(
            state, piece, victim, false, false,
        )? && !crate::movement::v7_encouraged_at(state, victim, target)),
    }
}

fn portal_pair(state: &GameState) -> Result<Option<[Square; 2]>> {
    let Some(value) = state.extra.get("portalRule") else {
        return Ok(None);
    };
    if value != &Value::Bool(true) && !crate::observation::truth(value.get("enabled")) {
        return Ok(None);
    }
    let cells: Vec<_> = value
        .get("cells")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|cell| {
            let row = crate::observation::number(cell.get("row"))?;
            let col = crate::observation::number(cell.get("col"))?;
            (row.fract() == 0.0
                && col.fract() == 0.0
                && (0.0..8.0).contains(&row)
                && (0.0..8.0).contains(&col))
            .then_some(Square {
                row: row as u8,
                col: col as u8,
            })
        })
        .collect();
    Ok(Some(if cells.len() == 2 && cells[0] != cells[1] {
        [cells[0], cells[1]]
    } else {
        [Square { row: 5, col: 2 }, Square { row: 2, col: 5 }]
    }))
}

fn source_ranged_piece(state: &GameState, piece: &Piece) -> bool {
    matches!(
        piece.kind.as_str(),
        "brutus"
            | "bigBishop"
            | "rook"
            | "bishop"
            | "queen"
            | "bear"
            | "amazon"
            | "cardinal"
            | "cannon"
            | "herald"
            | "hook"
            | "protestant"
            | "windmill"
            | "windmillBishop"
            | "windmillRook"
            | "bigRook"
            | "jester"
            | "idol"
    ) || piece.kind == "princess"
        && !state.board.iter().flatten().flatten().any(|ally| {
            ally.color == piece.color
                && ally.kind == "queen"
                && ally.extra.get("regencyHeir") != Some(&Value::Bool(true))
        })
        || piece.kind == "magicGirl" && source_side_truth(state, "magicGirlSurge", piece)
        || piece.kind == "berserker"
            && state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|ally| ally.color == piece.color)
                .map(|ally| &ally.id)
                .collect::<BTreeSet<_>>()
                .len()
                <= 9
}

fn ray_transparent(state: &GameState, attacker: &Piece, target: &Piece, at: Square) -> bool {
    source_side_truth(state, "overtake", attacker)
        && attacker.kind == "rook"
        && target.color == attacker.color
        && !matches!(
            target.kind.as_str(),
            "wall" | "football" | "monster" | "blackHole" | "coffin"
        )
        || ghost_transparent(
            state,
            attacker,
            target,
            at,
            source_ranged_piece(state, attacker),
        )
        || time_phase_transparent(state, attacker, target)
}

fn ghost_transparent(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
    at: Square,
    ranged: bool,
) -> bool {
    crate::movement::v7_stealth_transparent(state, attacker, target, at)
        || target.color == attacker.color
            && crate::observation::truth(target.extra.get("ghost"))
            && ranged
}

fn time_phase_transparent(state: &GameState, attacker: &Piece, target: &Piece) -> bool {
    state
        .extra
        .get("campaign")
        .and_then(|campaign| campaign.get("setup"))
        .and_then(Value::as_str)
        == Some("timeTraveler")
        && !matches!(target.kind.as_str(), "wall" | "football")
        && !(attacker.color == Color::Black && target.color == Color::Black)
        && (attacker.extra.get("timePhase").and_then(Value::as_str) == Some("past"))
            != (target.extra.get("timePhase").and_then(Value::as_str) == Some("past"))
}

fn attack_ray_reaches(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
    snipe: bool,
) -> Result<bool> {
    if from == target {
        return Ok(false);
    }
    let step_r = (i16::from(target.row) - i16::from(from.row)).signum();
    let step_c = (i16::from(target.col) - i16::from(from.col)).signum();
    attack_direction_reaches(state, piece, from, target, step_r, step_c, snipe)
}

fn attack_rays_reach(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
    directions: &[(i8, i8)],
) -> Result<bool> {
    for &(step_r, step_c) in directions {
        if attack_direction_reaches(
            state,
            piece,
            from,
            target,
            i16::from(step_r),
            i16::from(step_c),
            false,
        )? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn attack_direction_reaches(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
    step_r: i16,
    step_c: i16,
    snipe: bool,
) -> Result<bool> {
    let mut row = i16::from(from.row) + step_r;
    let mut col = i16::from(from.col) + step_c;
    let pair = if snipe { None } else { portal_pair(state)? };
    let mut portal_transit = false;
    let mut screens = 0;
    for _ in 0..64 {
        if !(0..8).contains(&row) || !(0..8).contains(&col) {
            return Ok(false);
        }
        let at = Square {
            row: row as u8,
            col: col as u8,
        };
        let exit = pair.and_then(|pair| {
            if pair[0] == at {
                Some(pair[1])
            } else if pair[1] == at {
                Some(pair[0])
            } else {
                None
            }
        });
        if exit.is_some_and(|exit| {
            portal_transit
                || crate::movement::collapsed(state, at)
                || crate::movement::collapsed(state, exit)
        }) {
            return Ok(false);
        }
        if at == target {
            return Ok(!snipe || screens == 1);
        }
        if let Some(blocker) = state.at(at).filter(|blocker| {
            !if snipe {
                ghost_transparent(state, piece, blocker, at, true)
            } else {
                ray_transparent(state, piece, blocker, at)
            }
        }) {
            let _ = blocker;
            if !snipe {
                return Ok(false);
            }
            screens += 1;
            if screens > 1 {
                return Ok(false);
            }
        }
        if let Some(exit) = exit {
            if exit == target {
                return Ok(true);
            }
            if state
                .at(exit)
                .is_some_and(|blocker| !ray_transparent(state, piece, blocker, exit))
            {
                return Ok(false);
            }
            portal_transit = true;
            row = i16::from(exit.row) + step_r;
            col = i16::from(exit.col) + step_c;
        } else {
            row += step_r;
            col += step_c;
        }
    }
    Err(EngineError::InvalidState(
        "v7 attack ray exceeded the finite source board path".into(),
    ))
}

fn attack_highway_reaches(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
    victim: Option<&Piece>,
) -> Result<bool> {
    let highway_cell = |square: Square| {
        crate::observation::truth(state.extra.get("highway")) && [1, 6].contains(&square.col)
            || state
                .extra
                .get("highwayCells")
                .and_then(Value::as_array)
                .is_some_and(|cells| cells.iter().any(|cell| matches_square(cell, square)))
    };
    if !highway_cell(from)
        || piece.is_large()
        || matches!(
            piece.kind.as_str(),
            "wall"
                | "football"
                | "guard"
                | "recruiter"
                | "wizard"
                | "herald"
                | "merchant"
                | "coffin"
                | "scarecrow"
                | "blackHole"
                | "timeAfterimage"
        )
        || piece.kind == "jester"
            && !victim.is_some_and(|victim| {
                matches!(
                    victim.kind.as_str(),
                    "king" | "royalKnight" | "shotgunKing" | "merchant"
                )
            })
        || from == target
        || from.row != target.row && from.col != target.col
    {
        return Ok(false);
    }
    let r = (i16::from(target.row) - i16::from(from.row)).signum() as i8;
    let c = (i16::from(target.col) - i16::from(from.col)).signum() as i8;
    let mut at = from;
    for _ in 0..8 {
        let Some(next) = at.offset(r, c).filter(|next| highway_cell(*next)) else {
            return Ok(false);
        };
        at = next;
        if at == target {
            return Ok(true);
        }
        if state.at(at).is_some_and(|blocker| {
            !(time_phase_transparent(state, piece, blocker)
                || ghost_transparent(state, piece, blocker, at, true))
        }) {
            return Ok(false);
        }
    }
    Ok(false)
}

fn blood_moon_night_movement(state: &GameState, piece: &Piece) -> bool {
    let Some(campaign) = state
        .extra
        .get("campaign")
        .filter(|campaign| campaign.get("setup").and_then(Value::as_str) == Some("bloodMoon"))
    else {
        return false;
    };
    let shared_turns = state.turns_taken.white.min(state.turns_taken.black);
    shared_turns % 7 >= 4
        || campaign
            .get("bloodMoon")
            .and_then(|blood| blood.get("sunlightOverride"))
            .and_then(Value::as_str)
            == Some(piece.color.as_str())
}

fn variant_attack_contains(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
) -> Result<bool> {
    // These branches call their raw movement kernel in `pieceAttacksSquare`.
    // They cannot borrow a UI or forced-turn list. Portal-decorated moves and
    // large attack footprints require their source-specific attack projection.
    if piece.kind == "colossus" {
        return Ok(colossus_attacks_square(state, piece, from, target));
    }
    if matches!(piece.kind.as_str(), "bigRook" | "bigBishop") {
        return big_piece_attacks_square(state, piece, from, target);
    }
    let moves = if piece.kind == "grasshopper" {
        crate::movement::v7_raw_grasshopper_moves(state, piece, from)?
    } else if piece.kind == "fanatic" {
        crate::movement::v7_raw_fanatic_moves(state, piece, from)?
    } else if piece.kind == "primeMinister" {
        crate::v7_special_piece_moves::raw_prime_minister_moves(state, piece, from)?
    } else if piece.kind == "cannon" {
        crate::movement::v7_raw_cannon_moves(state, piece, from)?
    } else {
        crate::variant_movement::base_moves(state, piece, from)?.ok_or_else(|| {
            EngineError::UnsupportedFeature(format!(
                "v7 pieceAttacksSquare {} raw attack kernel",
                piece.kind
            ))
        })?
    };
    let portal_projection = matches!(
        piece.kind.as_str(),
        "fanatic" | "protestant" | "cannon" | "hook"
    );
    let moves = if portal_projection {
        crate::movement::v7_apply_portal_moves(state, piece, moves)?
    } else {
        moves
    };
    Ok(moves.iter().any(|move_| {
        // main98698: Trickster inspects the raw tricksterMoves result; it
        // does not decorate that result with applyPortalMoves a second time.
        // Its selected movement uses the source whitelist/default queen.
        if piece.kind == "siegeRam"
            || piece.kind == "trickster"
                && crate::variant_movement::v7_trickster_move_type(piece) == "siegeRam"
        {
            move_
                .flags
                .get("highlightCells")
                .and_then(Value::as_array)
                .is_some_and(|cells| cells.iter().any(|cell| matches_square(cell, target)))
        } else if portal_projection {
            source_move_threatens_square(move_, target)
        } else {
            move_.square() == target
        }
    }))
}

fn normalize_piece_square_v7(state: &GameState, piece: &Piece, from: Square) -> Square {
    if !piece.is_large() {
        return from;
    }
    piece
        .extra
        .get("anchorRow")
        .and_then(Value::as_i64)
        .zip(piece.extra.get("anchorCol").and_then(Value::as_i64))
        .filter(|(row, col)| (0..8).contains(row) && (0..8).contains(col))
        .map(|(row, col)| Square {
            row: row as u8,
            col: col as u8,
        })
        .filter(|at| {
            state.at(*at).is_some_and(|anchor| {
                anchor.is_large()
                    && (!piece.id.is_empty() && anchor.id == piece.id || anchor == piece)
            })
        })
        .unwrap_or(from)
}

fn colossus_attacks_square(state: &GameState, piece: &Piece, from: Square, target: Square) -> bool {
    let origin = normalize_piece_square_v7(state, piece, from);
    let sector_row =
        (i16::from(origin.row) + if piece.color == Color::White { -3 } else { 3 }).clamp(0, 6);
    let left = (i16::from(origin.col) - 3).clamp(0, 6);
    let right = (i16::from(origin.col) + 3).clamp(0, 6);
    (sector_row..=sector_row + 1).contains(&i16::from(target.row))
        && ((left..=left + 1).contains(&i16::from(target.col))
            || (right..=right + 1).contains(&i16::from(target.col)))
}

fn big_piece_attacks_square(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
) -> Result<bool> {
    let directions = if piece.kind == "bigBishop" {
        crate::movement::DIAG
    } else {
        crate::movement::ORTHO
    };
    let limit = if piece.kind == "bigBishop" { 3 } else { 2 };
    for &(dr, dc) in directions {
        for distance in 1..8 {
            let row = i16::from(from.row) + i16::from(dr) * distance;
            let col = i16::from(from.col) + i16::from(dc) * distance;
            if !(0..7).contains(&row) || !(0..7).contains(&col) {
                break;
            }
            let cells = [
                Square {
                    row: row as u8,
                    col: col as u8,
                },
                Square {
                    row: row as u8,
                    col: (col + 1) as u8,
                },
                Square {
                    row: (row + 1) as u8,
                    col: col as u8,
                },
                Square {
                    row: (row + 1) as u8,
                    col: (col + 1) as u8,
                },
            ];
            let occupants: Vec<_> = cells
                .iter()
                .filter_map(|at| state.at(*at).map(|victim| (*at, victim)))
                .filter(|(_, victim)| {
                    if piece.id.is_empty() {
                        *victim != piece
                    } else {
                        victim.id != piece.id
                    }
                })
                .collect();
            let transparent = occupants
                .iter()
                .filter(|(at, victim)| ghost_transparent(state, piece, victim, *at, true))
                .count();
            if transparent > 0 {
                if transparent == occupants.len() {
                    continue;
                }
                break;
            }
            let Some(captured) =
                crate::movement::v7_large_landing_captures(state, piece, &cells, limit, true)?
            else {
                break;
            };
            if cells.contains(&target) {
                return Ok(true);
            }
            if !captured.is_empty() {
                break;
            }
        }
    }
    Ok(false)
}

fn shotgun_direction_attacks_square(
    from: Square,
    target: Square,
    direction: &Value,
) -> Result<bool> {
    let (dr, dc) = if let Some(parts) = direction.as_array() {
        let pair = parts
            .first()
            .and_then(Value::as_i64)
            .zip(parts.get(1).and_then(Value::as_i64))
            .filter(|(dr, dc)| {
                (-1..=1).contains(dr) && (-1..=1).contains(dc) && (*dr != 0 || *dc != 0)
            })
            .ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 shotgun threat direction must be a unit board vector".into(),
                )
            })?;
        (pair.0 as i8, pair.1 as i8)
    } else {
        match direction.as_str() {
            Some("down") => (1, 0),
            Some("left") => (0, -1),
            Some("right") => (0, 1),
            _ => (-1, 0),
        }
    };
    Ok(crate::v7_special_piece_moves::shotgun_blast_cells(from, [dr, dc]).contains(&target))
}

fn shotgun_attacks_square(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: Square,
) -> Result<bool> {
    let ammo = source_counter(piece.extra.get("ammo")).max(0.0);
    if ammo >= 2.0 {
        for &(dr, dc) in crate::movement::KING {
            if crate::v7_special_piece_moves::shotgun_blast_cells(from, [dr, dc]).contains(&target)
            {
                return Ok(true);
            }
        }
    }
    Ok(ammo >= 3.0 && attack_rays_reach(state, piece, from, target, crate::movement::KING)?)
}

pub(crate) fn break_initiative_by_check_v7(state: &mut GameState, attacker: Color) -> Result<bool> {
    crate::legal_profile::measure("threat_initiative", || {
        break_initiative_by_check_v7_profiled(state, attacker)
    })
}

pub(crate) fn break_initiative_by_check_v7_profiled(
    state: &mut GameState,
    attacker: Color,
) -> Result<bool> {
    let defender = attacker.opponent();
    if state
        .extra
        .get("initiative")
        .and_then(|sides| sides.get(defender.as_str()))
        .and_then(|entry| entry.get("by"))
        .and_then(Value::as_str)
        != Some(attacker.as_str())
    {
        return Ok(false);
    }
    let mut checked = false;
    let mut seen = BTreeSet::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            let Some(piece) = piece
                .as_ref()
                .filter(|piece| piece.color == defender && is_v7_threat_royal(state, piece))
            else {
                continue;
            };
            if !seen.insert(&piece.id) {
                continue;
            }
            if is_square_attacked_v7(
                state,
                Square {
                    row: row as u8,
                    col: col as u8,
                },
                attacker,
                None,
                None,
            )? {
                checked = true;
                break;
            }
        }
        if checked {
            break;
        }
    }
    if !checked {
        return Ok(false);
    }
    let mut working = state.clone();
    working
        .extra
        .get_mut("initiative")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState("v7 initiative must be a color map".into()))?
        .insert(defender.as_str().into(), Value::Null);
    crate::replay::add_log(&mut working, "선공권 제한이 해제되었습니다.".into())?;
    *state = working;
    Ok(true)
}

/// The source records removal causes only in its private royal-threat
/// simulation. A live-game removal must not add these diagnostic fields to
/// the source DTO. Capture, hazard and force-removal owners share this single
/// encoder and pass the execution context explicitly.
pub(crate) fn mark_king_threat_removal_cause(
    state: &mut GameState,
    captured: &Piece,
    at: Square,
    source: &Value,
    threat_probe: bool,
) -> Result<()> {
    if !threat_probe || !is_v7_threat_royal(state, captured) {
        return Ok(());
    }
    let terminal = crate::observation::truth(source.get("terminal"));
    let alive = if captured.id.is_empty() {
        state
            .at(at)
            .is_some_and(|piece| piece.color == captured.color && is_v7_threat_royal(state, piece))
    } else {
        state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.id == captured.id && piece.color == captured.color)
    };
    if alive && !terminal {
        return Ok(());
    }
    let string_field = |field: &str| -> Result<&str> {
        match source.get(field) {
            None | Some(Value::Null) => Ok(""),
            Some(Value::String(value)) => Ok(value.as_str()),
            Some(value) if !crate::observation::truth(Some(value)) => Ok(""),
            Some(_) => Err(EngineError::InvalidState(format!(
                "v7 royal threat removal {field} must be a string"
            ))),
        }
    };
    let attacker = source
        .get("attacker")
        .filter(|value| crate::observation::truth(Some(value)));
    let attacker_id = attacker
        .and_then(|piece| piece.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let attacker_type = attacker
        .and_then(|piece| piece.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let label = string_field("label")?;
    let named_effect = string_field("effect")?;
    let effect = if !named_effect.is_empty() {
        named_effect
    } else if !label.is_empty() {
        label
    } else {
        "piece-capture"
    };
    let default_priority = if !named_effect.is_empty() || !label.is_empty() {
        3
    } else if attacker.is_some() {
        1
    } else {
        0
    };
    let priority = source
        .get("priority")
        .filter(|value| !value.is_null())
        .cloned()
        .unwrap_or_else(|| json!(default_priority));
    let numeric_priority = crate::observation::number(Some(&priority)).ok_or_else(|| {
        EngineError::InvalidState("v7 royal threat removal priority must be finite".into())
    })?;
    let origin = source
        .get("origin")
        .filter(|value| crate::observation::truth(Some(value)))
        .and_then(|value| {
            let row = crate::observation::number(value.get("row"))?;
            let col = crate::observation::number(value.get("col"))?;
            (row.fract() == 0.0 && col.fract() == 0.0).then_some((row, col))
        })
        .or_else(|| {
            if source
                .get("origin")
                .is_some_and(|value| crate::observation::truth(Some(value)))
                || attacker_id.is_empty()
            {
                return None;
            }
            state.board.iter().enumerate().find_map(|(row, cells)| {
                cells.iter().enumerate().find_map(|(col, cell)| {
                    let piece = cell.as_ref().filter(|piece| piece.id == attacker_id)?;
                    if piece.is_large() {
                        let anchor_row = crate::observation::number(piece.extra.get("anchorRow"));
                        let anchor_col = crate::observation::number(piece.extra.get("anchorCol"));
                        if let Some((r, c)) = anchor_row.zip(anchor_col).filter(|(r, c)| {
                            r.fract() == 0.0
                                && c.fract() == 0.0
                                && (0.0..8.0).contains(r)
                                && (0.0..8.0).contains(c)
                        }) && state
                            .at(Square {
                                row: r as u8,
                                col: c as u8,
                            })
                            .is_some_and(|anchor| anchor.id == piece.id && anchor.is_large())
                        {
                            return Some((r, c));
                        }
                    }
                    Some((row as f64, col as f64))
                })
            })
        });
    let royal_key = if captured.id.is_empty() {
        format!("{}:{}:{}", captured.kind, at.row, at.col)
    } else {
        captured.id.clone()
    };
    let royal_ref = json!({"id":captured.id,"key":royal_key,"type":captured.kind,"color":captured.color,"row":at.row,"col":at.col});
    let spawn = crate::observation::truth(source.get("spawn"));
    let default_label = if attacker.is_none() {
        "기물 제거"
    } else {
        ""
    };
    let notation_label = if label.is_empty() {
        default_label
    } else {
        label
    };
    let to = format!("{}{}", char::from(b'a' + at.col), 8 - at.row);
    let notation_type = string_field("notationType")?;
    let notation_type = if notation_type.is_empty() {
        if attacker_type.is_empty() {
            "pawn"
        } else {
            attacker_type
        }
    } else {
        notation_type
    };
    let notation = if notation_label.is_empty() {
        format!(
            "{}{}{to}",
            crate::replay::piece_code(RULES_VERSION_V7, notation_type),
            if spawn { "" } else { "x" }
        )
    } else {
        format!("{notation_label} ({to})")
    };
    let entry = json!({
        "key":format!("check:{royal_key}:{effect}:{attacker_id}:{}:{}", at.row, at.col),
        "kind":"check", "royalId":royal_key,
        "attackerId":if attacker_id.is_empty() { effect } else { attacker_id },
        "attackerType":attacker_type,
        "attackerPieceId":if spawn { "" } else { attacker_id },
        "attackerOriginKey":if spawn { String::new() } else { origin.map_or_else(String::new, |(row, col)| format!("{row}-{col}")) },
        "cardId":"", "effect":effect,
        "direct":named_effect.is_empty() && label.is_empty() && attacker.is_some(),
        "terminal":terminal, "priority":priority, "notation":notation, "path":[],
    });
    let cause = json!({"royalRef":royal_ref,"entry":entry});
    if !state
        .extra
        .get("kingThreatCaptureCauses")
        .is_some_and(Value::is_array)
    {
        state
            .extra
            .insert("kingThreatCaptureCauses".into(), json!([]));
    }
    let causes = state
        .extra
        .get_mut("kingThreatCaptureCauses")
        .and_then(Value::as_array_mut)
        .expect("array initialized");
    if let Some(index) = causes.iter().position(|cause| {
        cause.get("entry").and_then(|entry| entry.get("royalId")) == Some(&json!(royal_key))
    }) {
        let previous_priority = crate::observation::number(
            causes[index]
                .get("entry")
                .and_then(|entry| entry.get("priority")),
        )
        .ok_or_else(|| {
            EngineError::InvalidState("v7 recorded royal threat priority must be finite".into())
        })?;
        if previous_priority <= numeric_priority {
            causes[index] = cause;
        }
    } else {
        causes.push(cause);
    }
    Ok(())
}

pub(crate) fn mark_king_threat_effect_cause(
    state: &mut GameState,
    label: &str,
    threat_probe: bool,
) -> Result<()> {
    if !threat_probe {
        return Ok(());
    }
    let mut seen = BTreeSet::new();
    let mut labels = Vec::new();
    for value in state
        .extra
        .get("kingThreatEffectCauses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let value = value
            .as_str()
            .ok_or_else(|| {
                EngineError::InvalidState("v7 royal threat effect cause must be a string".into())
            })?
            .trim();
        if !value.is_empty() && seen.insert(value.to_owned()) {
            labels.push(json!(value));
        }
    }
    let label = label.trim();
    if !label.is_empty() && seen.insert(label.to_owned()) {
        labels.push(json!(label));
    }
    state
        .extra
        .insert("kingThreatEffectCauses".into(), json!(labels));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameConfig, MoveTarget, Square};

    #[test]
    fn source_initial_play_has_no_relevant_capture_and_preserves_pending_capture() {
        let config = GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        };
        let mut state = crate::v7_new_game::new_game(config, 19).unwrap();
        crate::replay::begin_move(&mut state, Color::White).unwrap();
        let before = state.clone();
        assert_eq!(
            probe_royal_capture(&mut state, Color::White, false).unwrap(),
            RoyalThreatProbe {
                check: false,
                danger: false,
                simulated: false,
                examined: 0,
            }
        );
        assert_eq!(state, before);
        assert!(
            state.move_replay_scope.is_none(),
            "a private probe scope must not escape into the live host"
        );
    }

    #[test]
    fn private_threat_move_sound_keeps_its_cue_without_another_probe() {
        // main65609 returns the default sound before touching the board when
        // kingThreatProbeDepth is active. The malformed live-probe queue
        // makes accidental recursion observable as an error as well as RNG.
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        state
            .extra
            .insert("delayedHazards".into(), json!("must-not-be-read"));
        state.extra.insert(
            "lastMove".into(),
            json!({"soundName":"capture","soundColor":"white"}),
        );
        for (ai_depth, threat_depth) in [(0, 1), (1, 0)] {
            state.ai_simulation_depth = ai_depth;
            state.threat_probe_depth = threat_depth;
            let before = state.clone();
            reconcile_move_replay_capture_v7(&mut state).unwrap();
            reconcile_move_replay_capture_v7(&mut state).unwrap();
            assert!(!crate::threat::play_move_sound(&mut state, "capture", Color::White).unwrap());
            assert_eq!(state, before);
        }
    }

    #[test]
    fn regency_queen_suppresses_native_king_and_promotes_marked_heir() {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.set_flag("regency", Color::White, true);
        let king_id = state
            .board
            .iter()
            .flatten()
            .flatten()
            .find(|piece| piece.color == Color::White && piece.kind == "king")
            .unwrap()
            .id
            .clone();
        assert!(!royal_ids(&state, Color::White).unwrap().contains(&king_id));
        state.set_flag("kingDead", Color::White, true);
        let queen = state
            .board
            .iter_mut()
            .flatten()
            .flatten()
            .find(|piece| piece.color == Color::White && piece.kind == "queen")
            .unwrap();
        queen.extra.insert("regencyHeir".into(), json!(true));
        let queen_id = queen.id.clone();
        let royals = royal_ids(&state, Color::White).unwrap();
        assert!(royals.contains(&queen_id));
        assert!(!royals.contains(&king_id));
        assert!(validate_automatic_reaction_schema(&state).is_ok());
    }

    #[test]
    fn empty_royal_set_is_not_a_capture_and_does_not_advance_rng() {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        for row in &mut state.board {
            for cell in row {
                *cell = None;
            }
        }
        state.mode = "play".into();
        let before = state.clone();
        let probe = probe_royal_capture(&mut state, Color::White, false).unwrap();
        assert_eq!(
            probe,
            RoyalThreatProbe {
                check: false,
                danger: false,
                simulated: false,
                examined: 0
            }
        );
        assert_eq!(state, before);
    }

    #[test]
    fn pending_trolley_has_no_threat_action_stream_even_with_automatic_reactions() {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.mode = "play".into();
        state.extra.insert(
            "activeTrolley".into(),
            json!({"id":"trolley-choice","color":"white","choices":[]}),
        );
        state
            .extra
            .insert("pendingGales".into(), json!([{"row":0,"col":0}]));
        let before = state.clone();
        assert_eq!(
            probe_royal_capture(&mut state, Color::White, true).unwrap(),
            RoyalThreatProbe {
                check: false,
                danger: false,
                simulated: false,
                examined: 0,
            }
        );
        assert_eq!(state, before);
    }

    #[test]
    fn empty_attacker_hand_has_no_card_mediated_danger() {
        fn source_instance(state: &mut GameState, id: &str) -> crate::CardSlot {
            let definition = crate::card_registry::definition_for(RULES_VERSION_V7, id).unwrap();
            let cloned = crate::draft::clone_card(state, &definition.source_definition).unwrap();
            serde_json::from_value(cloned).unwrap()
        }

        // A null/empty deck slot has no definition or instance identity. It
        // cannot be revived into a used/recovering card by toggling vacant.
        // Keep the v7 initial pools/profile, then make a synthetic empty hand.
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        state.extra.insert("draftDelete".into(), json!(false));
        state.deck_slots.black.clear();
        let royals = royal_ids(&state, Color::White).unwrap();
        let window = source_window(&state, Color::White).unwrap();
        let before = state.clone();
        let probe = probe_ordered_candidates(&mut state, &window, Color::White, true, &royals, &[])
            .unwrap();
        assert!(!probe.check);
        assert!(!probe.danger);
        assert_eq!(probe.examined, 0);
        assert_eq!(state, before);

        // The source also excludes cards from a game-wide draft-delete game,
        // even if a saved position happens to retain populated slots.
        let mut disabled = before.clone();
        let ready = source_instance(&mut disabled, "berserker");
        disabled.deck_slots.black.push(ready);
        let mut enabled_window = source_window(&disabled, Color::White).unwrap();
        assert!(
            !crate::v7_ai_card_candidates::collect_v7_ai_card_actions(
                &mut enabled_window,
                Color::Black,
                true,
            )
            .unwrap()
            .is_empty()
        );
        disabled.extra.insert("draftDelete".into(), json!(true));
        let mut disabled_window = source_window(&disabled, Color::White).unwrap();
        assert!(
            crate::v7_ai_card_candidates::collect_v7_ai_card_actions(
                &mut disabled_window,
                Color::Black,
                true,
            )
            .unwrap()
            .is_empty()
        );
        let disabled_before = disabled.clone();
        let report = probe_ordered_candidates(
            &mut disabled,
            &disabled_window,
            Color::White,
            true,
            &royals,
            &[],
        )
        .unwrap();
        assert!(!report.check);
        assert!(!report.danger);
        assert_eq!(report.examined, 0);
        assert_eq!(disabled, disabled_before);

        // Frozen collectValidAiActions(cardsOnly:true) skips used and
        // recovering instances before it asks for any card targets.
        let mut exhausted = before.clone();
        let mut used = source_instance(&mut exhausted, "berserker");
        let mut recovering = source_instance(&mut exhausted, "bribe");
        used.used = true;
        recovering.recovering = true;
        assert_ne!(used.instance_id, recovering.instance_id);
        exhausted.deck_slots.black = vec![used, recovering];
        for card in &exhausted.deck_slots.black {
            crate::card_registry::validate_instance(&exhausted, card).unwrap();
        }
        let exhausted_window = source_window(&exhausted, Color::White).unwrap();
        let exhausted_before = exhausted.clone();
        let report = probe_ordered_candidates(
            &mut exhausted,
            &exhausted_window,
            Color::White,
            true,
            &royals,
            &[],
        )
        .unwrap();
        assert!(!report.check);
        assert!(!report.danger);
        assert_eq!(report.examined, 0);
        assert_eq!(exhausted, exhausted_before);
    }

    #[test]
    fn non_move_ai_actions_do_not_enter_royal_capture_simulation() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        let window = source_window(&state, Color::White).unwrap();
        let royals = royal_ids(&window, Color::White).unwrap();
        let mut action = Action::movement(
            Color::Black,
            Square { row: 0, col: 0 },
            crate::MoveTarget::at(Square { row: 1, col: 0 }),
        );
        action.kind = crate::ActionKind::ShotgunReload;
        action.destination = None;
        let before = state.clone();
        let report =
            probe_ordered_candidates(&mut state, &window, Color::White, false, &royals, &[action])
                .unwrap();
        assert_eq!(
            report,
            RoyalThreatProbe {
                check: false,
                danger: false,
                simulated: false,
                examined: 0
            }
        );
        assert_eq!(state, before);
    }

    #[test]
    fn malformed_reaction_reports_exact_error_without_mutating_the_input() {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.mode = "play".into();
        state
            .extra
            .insert("delayedHazards".into(), json!("invalid-array"));
        let before = state.clone();
        assert!(matches!(
            probe_royal_capture(&mut state, Color::White, false),
            Err(EngineError::InvalidState(message)) if message == "v7 delayedHazards must be an array"
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn location_bound_reactions_use_the_common_probe_callback() {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.mode = "play".into();
        let (row, col) = state
            .board
            .iter()
            .enumerate()
            .find_map(|(row, cells)| {
                cells.iter().enumerate().find_map(|(col, cell)| {
                    cell.as_ref()
                        .filter(|piece| piece.color == Color::White && piece.kind == "king")
                        .map(|_| (row, col))
                })
            })
            .unwrap();
        state.extra.insert(
            "delayedHazards".into(),
            json!([{"cells":[{"row":0,"col":0}]}]),
        );
        state.extra.insert(
            "pendingOtherworld".into(),
            json!([{"row":row,"col":col,"remainingHalfTurns":3}]),
        );
        assert!(validate_automatic_reaction_schema(&state).is_ok());
        state.extra.insert(
            "pendingOtherworld".into(),
            json!([{"row":row,"col":col,"remainingHalfTurns":1}]),
        );
        assert!(validate_automatic_reaction_schema(&state).is_ok());
    }

    #[test]
    fn source_queen_capture_probe_reports_check_without_committing_child_state() {
        // Pinned source newGame({draftDelete:true},19), then isolate its white
        // king e1 and black queen on e2. evaluateKingThreatReport("white",
        // {includeDanger:false}) reports one `Qxe1#` check, leaving full state,
        // RNG cursor 32/state 4163866163, and history unchanged. The ordered
        // AiNoCards candidate itself is supplied here while general raw move
        // enumeration remains a separately owned completeness gate.
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        let king = state.board[7][4].clone().unwrap();
        let queen = state.board[0][3].clone().unwrap();
        for row in &mut state.board {
            for cell in row {
                *cell = None;
            }
        }
        state.board[7][4] = Some(king.clone());
        state.board[6][4] = Some(queen);
        crate::replay::begin_move(&mut state, Color::White).unwrap();
        let before = state.clone();
        let window = source_window(&state, Color::White).unwrap();
        let royals = royal_ids(&window, Color::White).unwrap();
        assert_eq!(royals, BTreeSet::from([king.id]));
        let candidates = [Action::movement(
            Color::Black,
            Square { row: 6, col: 4 },
            MoveTarget::at(Square { row: 7, col: 4 }),
        )];
        let report = probe_ordered_candidates(
            &mut state,
            &window,
            Color::White,
            false,
            &royals,
            &candidates,
        )
        .unwrap();
        assert_eq!(
            report,
            RoyalThreatProbe {
                check: true,
                danger: false,
                simulated: true,
                examined: 1,
            }
        );
        assert_probe_capture_control_only(&state, &before, Color::Black);
    }

    #[test]
    fn threat_probe_visits_later_direct_capture_candidates_after_first_check() {
        // Frozen main-OahWs0tU.js SHA e5ed84fc...: seed-19 draftDelete
        // first-play state, isolate White king e1 and Black queens d2/e2.
        // evaluateKingThreatReport("white", {includeDanger:false}) returns
        // two check entries (Qdxe1#, Qexe1#) and leaves full state unchanged.
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        let king = state.board[7][4].clone().unwrap();
        let queen = state.board[0][3].clone().unwrap();
        for row in &mut state.board {
            for cell in row {
                *cell = None;
            }
        }
        state.board[7][4] = Some(king.clone());
        state.board[6][4] = Some(queen);
        state.board[6][3] = Some(Piece::new("queen", Color::Black, "second-black-queen"));
        crate::replay::begin_move(&mut state, Color::White).unwrap();
        let before = state.clone();
        let window = source_window(&state, Color::White).unwrap();
        let royals = royal_ids(&window, Color::White).unwrap();
        let actions = [
            Action::movement(
                Color::Black,
                Square { row: 6, col: 4 },
                MoveTarget::at(Square { row: 7, col: 4 }),
            ),
            Action::movement(
                Color::Black,
                Square { row: 6, col: 3 },
                MoveTarget::at(Square { row: 7, col: 4 }),
            ),
        ];
        let report =
            probe_ordered_candidates(&mut state, &window, Color::White, false, &royals, &actions)
                .unwrap();
        assert!(report.check);
        assert_eq!(report.examined, 2);
        assert_probe_capture_control_only(&state, &before, Color::Black);
    }

    fn assert_probe_capture_control_only(state: &GameState, before: &GameState, actor: Color) {
        // Source state restoration includes its entire serializable DTO, RNG
        // and history. Its separate activeMoveReplayCapture is overwritten by
        // the simulated terminal move's begin and survives the state restore.
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        assert!(
            state.move_replay_scope.is_none(),
            "a private probe journal must not escape"
        );
        let capture = crate::replay::active_move_capture(state)
            .unwrap()
            .expect("terminal threat begin leaves a pending replay capture");
        assert_eq!(capture.actor, actor);
        assert_eq!(capture.before.board, before.board);
        assert!(capture.before.active_move_replay_before.is_none());
        assert!(capture.before.move_replay_scope.is_none());
    }

    fn source_sparse_play() -> GameState {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        for row in &mut state.board {
            row.fill(None);
        }
        state
    }

    #[test]
    fn common_probe_dispatch_uses_v7_log_and_delayed_hazard_reactions() {
        for reaction in ["log", "lightning", "meteor"] {
            let mut state = source_sparse_play();
            state.move_count = 4;
            state.full_move = 3;
            state.turns_taken.white = 2;
            state.turns_taken.black = 2;
            state.board[7][4] = Some(Piece::new("king", Color::White, "white-king"));
            state.board[0][0] = Some(Piece::new("king", Color::Black, "black-king"));
            if reaction == "log" {
                let mut log = Piece::new("log", Color::Black, "rolling-log");
                log.moved = true;
                log.extra.insert("logDir".into(), json!({"dr":1,"dc":0}));
                state.board[6][4] = Some(log);
            } else {
                state.extra.insert(
                    "delayedHazards".into(),
                    json!([{
                        "triggerAfter":"black", "owner":"black", "type":reaction,
                        "cells":[{"row":7,"col":4}],
                    }]),
                );
            }
            let before = state.clone();
            let mut direct = state.clone();
            let report = probe_royal_capture(&mut direct, Color::White, false).unwrap();
            assert!(
                report.check && report.simulated,
                "{reaction}: automatic royal removal must be simulated"
            );
            assert_eq!(
                crate::threat::evaluate_royal_capture(&mut state, Color::White).unwrap(),
                (report.check, report.simulated),
                "{reaction}",
            );
            assert_eq!(
                state, direct,
                "{reaction}: shared entry must preserve the probe's RNG and private-board boundary"
            );
            assert_eq!(
                serde_json::to_value(&state).unwrap(),
                serde_json::to_value(&before).unwrap(),
                "{reaction}: deterministic probes preserve the entire serializable state, RNG and history"
            );
            assert!(
                state.move_replay_scope.is_none(),
                "{reaction}: private journal must not escape"
            );
        }
    }

    #[test]
    fn corner_kick_attack_uses_the_source_two_by_two_corner_region() {
        let mut state = source_sparse_play();
        state.set_flag("cornerKick", Color::White, true);
        let knight = Piece::new("knight", Color::White, "corner-knight");
        state.board[7][1] = Some(knight.clone());
        assert!(
            piece_attacks_square_v7(
                &state,
                &knight,
                Square { row: 7, col: 1 },
                Square { row: 6, col: 2 }
            )
            .unwrap()
        );
        state.board[7][1] = None;
        state.board[5][1] = Some(knight.clone());
        assert!(
            !piece_attacks_square_v7(
                &state,
                &knight,
                Square { row: 5, col: 1 },
                Square { row: 4, col: 2 }
            )
            .unwrap()
        );
    }

    #[test]
    fn quiet_file_surge_and_rook_lift_follow_the_retained_turn_to_royal_capture() {
        // main85475/92560: the initial quiet move is relevant only when its
        // rule grants a continuation; the royal is captured on a later move.
        for (field, kind, from, royal_at) in [
            (
                "fileSurge",
                "knight",
                Square { row: 4, col: 2 },
                Square { row: 1, col: 1 },
            ),
            (
                "rookLift",
                "rook",
                Square { row: 2, col: 0 },
                Square { row: 0, col: 4 },
            ),
        ] {
            let mut base = source_sparse_play();
            base.move_count = 4;
            base.full_move = 3;
            base.turns_taken.white = 2;
            base.turns_taken.black = 2;
            let mut attacker = Piece::new(kind, Color::Black, "quiet-attacker");
            attacker.moved = true;
            base.board[from.row as usize][from.col as usize] = Some(attacker);
            base.board[royal_at.row as usize][royal_at.col as usize] =
                Some(Piece::new("king", Color::White, "white-king"));
            base.board[7][7] = Some(Piece::new("king", Color::Black, "black-king"));
            for enabled in [false, true] {
                let mut state = base.clone();
                state.set_flag(field, Color::Black, enabled);
                let before = state.clone();
                let report = probe_royal_capture(&mut state, Color::White, false).unwrap();
                assert_eq!(report.check, enabled, "{field}={enabled}");
                assert_eq!(
                    serde_json::to_value(&state).unwrap(),
                    serde_json::to_value(&before).unwrap(),
                    "{field}: all simulated board, RNG and history values stay private"
                );
                assert!(
                    state.move_replay_scope.is_none(),
                    "{field}: private journal must not escape"
                );
            }
        }
    }

    #[test]
    fn trickster_remembered_hook_keeps_raw_coordinates_when_a_portal_is_active() {
        // main95520 omits portalRule from its parrotBaseMoves call. The
        // physical Hook branch does pass Portal and projects its exit cell.
        let from = Square { row: 5, col: 0 };
        let target = Square { row: 2, col: 5 };
        let mut state = source_sparse_play();
        state.extra.insert(
            "portalRule".into(),
            json!({"enabled":true,"cells":[{"row":5,"col":2},{"row":2,"col":5}]}),
        );
        let mut trickster = Piece::new("trickster", Color::White, "memory-trickster");
        trickster
            .extra
            .insert("tricksterMoveType".into(), json!("brutus"));
        state.board[5][0] = Some(trickster.clone());
        state.board[5][3] = Some(Piece::new("pawn", Color::White, "east-blocker"));
        state.board[4][0] = Some(Piece::new("pawn", Color::White, "north-blocker"));
        state.board[6][0] = Some(Piece::new("pawn", Color::White, "south-blocker"));
        let before = state.clone();
        assert!(!piece_attacks_square_v7(&state, &trickster, from, target).unwrap());
        assert!(
            piece_attacks_square_v7(&state, &trickster, from, Square { row: 5, col: 2 }).unwrap()
        );
        assert_eq!(state, before);
    }

    #[test]
    fn replay_capture_probe_does_not_change_the_recorded_capture_cue() {
        let mut state = source_sparse_play();
        state.board[7][4] = Some(Piece::new("king", Color::White, "white-king"));
        state.board[6][4] = Some(Piece::new("queen", Color::Black, "black-queen"));
        state.extra.insert(
            "lastMove".into(),
            json!({"soundName":"capture","soundColor":"white"}),
        );
        let before = state.clone();
        reconcile_move_replay_capture_v7(&mut state).unwrap();
        assert_probe_capture_control_only(&state, &before, Color::Black);
        reconcile_move_replay_capture_v7(&mut state).unwrap();
        assert_eq!(state.extra["lastMove"]["soundName"], json!("capture"));
        assert!(state.move_replay_scope.is_none());
    }

    #[test]
    fn replay_capture_probe_can_change_the_next_committed_undo_frame() {
        // Black's simulated queen capture replaces the pending capture actor.
        // The next black commit consumes that journal; without the probe it
        // cannot create the same moveReplay.black frame.
        let mut state = source_sparse_play();
        state.board[7][4] = Some(Piece::new("king", Color::White, "white-king"));
        state.board[6][4] = Some(Piece::new("queen", Color::Black, "black-queen"));
        let mut without_probe = state.clone();
        reconcile_move_replay_capture_v7(&mut state).unwrap();
        let queen = state.board[6][4].take();
        state.board[5][4] = queen.clone();
        without_probe.board[6][4] = None;
        without_probe.board[5][4] = queen;
        crate::replay::commit_active_move(&mut state, Color::Black).unwrap();
        crate::replay::commit_active_move(&mut without_probe, Color::Black).unwrap();
        assert!(state.extra["moveReplay"]["black"].is_object());
        assert!(without_probe.extra["moveReplay"]["black"].is_null());
    }

    #[test]
    fn source_attack_predicate_distinguishes_capture_locks_from_target_protection() {
        // main-OahWs0tU.js98590: ordinary geometric attacks do not invoke the
        // full canCaptureTarget protection predicate. Fresh/Card no-capture,
        // frozen/staked/stunned and Initiative still gate the attacker.
        let mut state = source_sparse_play();
        let from = Square { row: 4, col: 0 };
        let target = Square { row: 4, col: 4 };
        let rook = Piece::new("rook", Color::White, "white-rook");
        let mut king = Piece::new("king", Color::Black, "black-king");
        king.extra.insert("protected".into(), json!(true));
        state.board[4][0] = Some(rook);
        state.board[4][4] = Some(king);
        state
            .extra
            .insert("campaign".into(), json!({"setup":"fogWar"}));
        assert!(is_square_attacked_v7(&state, target, Color::White, None, None).unwrap());
        for (field, value) in [
            ("freshNoCaptureUntil", json!(1)),
            ("cardNoCaptureUntil", json!(1)),
            ("promotionRushUntil", json!(1)),
            ("poisonStunTurns", json!(1)),
            ("staked", json!({"remaining":1})),
            ("frozen", json!(true)),
        ] {
            let mut locked = state.clone();
            locked.board[4][0]
                .as_mut()
                .unwrap()
                .extra
                .insert(field.into(), value);
            assert!(
                !piece_attacks_square_v7(&locked, locked.at(from).unwrap(), from, target).unwrap(),
                "{field}"
            );
        }
        state.board[4][0]
            .as_mut()
            .unwrap()
            .extra
            .insert("freshNoCaptureUntil".into(), json!(1));
        state.turns_taken.white = 1;
        assert!(is_square_attacked_v7(&state, target, Color::White, None, None).unwrap());
    }

    #[test]
    fn source_attack_honors_target_override_and_ignored_attacker_identity() {
        let mut state = source_sparse_play();
        let target = Square { row: 4, col: 4 };
        state.board[4][0] = Some(Piece::new("rook", Color::White, "white-rook"));
        let mut override_piece = Piece::new("king", Color::Black, "virtual-king");
        override_piece.extra.insert("frozen".into(), json!(true));
        assert!(
            !is_square_attacked_v7(&state, target, Color::White, Some(&override_piece), None)
                .unwrap()
        );
        override_piece.extra.shift_remove("frozen");
        assert!(
            is_square_attacked_v7(&state, target, Color::White, Some(&override_piece), None)
                .unwrap()
        );
        let ignored = BTreeSet::from(["white-rook".to_owned()]);
        assert!(
            !is_square_attacked_v7(
                &state,
                target,
                Color::White,
                Some(&override_piece),
                Some(&ignored)
            )
            .unwrap()
        );
    }

    #[test]
    fn source_portal_attack_ray_can_reach_a_different_rank() {
        // rayReaches98778 traverses at most one Portal pair. Initial target
        // alignment is not required: a horizontal rook ray entering c3
        // continues horizontally from f6 and reaches h6.
        let mut state = source_sparse_play();
        state.extra.insert(
            "portalRule".into(),
            json!({"enabled":true,"cells":[{"row":5,"col":2},{"row":2,"col":5}]}),
        );
        state.board[5][0] = Some(Piece::new("rook", Color::White, "white-rook"));
        assert!(
            is_square_attacked_v7(&state, Square { row: 2, col: 7 }, Color::White, None, None)
                .unwrap()
        );
        state.board[2][6] = Some(Piece::new("pawn", Color::Black, "black-blocker"));
        assert!(
            !is_square_attacked_v7(&state, Square { row: 2, col: 7 }, Color::White, None, None)
                .unwrap()
        );
        // main98671 keeps Corner Kick's final diagonal constraint even when
        // rayReaches could arrive off-axis through a portal pair.
        let mut corner = source_sparse_play();
        corner.board[0][0] = Some(Piece::new("knight", Color::White, "corner-knight"));
        corner.board[7][3] = Some(Piece::new("king", Color::Black, "black-king"));
        corner.set_flag("cornerKick", Color::White, true);
        corner.extra.insert(
            "portalRule".into(),
            json!({"enabled":true,"cells":[{"row":1,"col":1},{"row":6,"col":2}]}),
        );
        assert!(
            !is_square_attacked_v7(&corner, Square { row: 7, col: 3 }, Color::White, None, None)
                .unwrap()
        );
    }

    #[test]
    fn source_herald_precedes_racing_and_uses_explicit_probe_suppression() {
        let mut state = source_sparse_play();
        let mut herald = Piece::new("herald", Color::White, "white-herald");
        herald
            .extra
            .insert("kingThreatSuppressed".into(), json!(true));
        state.board[1][4] = Some(herald);
        state.board[0][4] = Some(Piece::new("king", Color::Black, "black-king"));
        state.board[0][0] = Some(Piece::new("king", Color::White, "white-king"));
        state.set_flag("racingKing", Color::White, true);
        let mut regular = state.clone();
        assert!(resolve_herald_threats_v7(&mut regular, Color::White).unwrap());
        assert_eq!(
            regular.extra["replayEndReason"],
            json!("전령이 상대 킹과 협정을 이끌어냈습니다.")
        );
        let mut probe = state;
        probe.threat_probe_depth = 1;
        assert!(!resolve_herald_for_color_v7(&mut probe, Color::White).unwrap());
        assert!(resolve_herald_threats_v7(&mut probe, Color::White).unwrap());
        assert_eq!(
            probe.extra["replayEndReason"],
            json!("레이싱 킹이 목표 랭크에 도달했습니다.")
        );
    }

    #[test]
    fn source_racing_king_draw_and_collapsed_goal_use_board_order() {
        let mut state = source_sparse_play();
        state.set_flag("racingKing", Color::White, true);
        state.set_flag("racingKing", Color::Black, true);
        state.board[0][4] = Some(Piece::new("king", Color::White, "white-king"));
        state.board[7][4] = Some(Piece::new("king", Color::Black, "black-king"));
        assert!(check_racing_kings_v7(&mut state).unwrap());
        assert_eq!(state.winner, None);
        assert_eq!(
            state.extra["replayEndReason"],
            json!("양쪽 킹이 동시에 목표 랭크에 도달하여 무승부입니다.")
        );

        let mut state = source_sparse_play();
        state.set_flag("racingKing", Color::White, true);
        state.extra.insert("collapsed".into(), json!(true));
        state.extra.insert("collapseDepth".into(), json!(0));
        state.board[1][4] = Some(Piece::new("king", Color::White, "white-king"));
        assert!(check_racing_kings_v7(&mut state).unwrap());
        assert_eq!(state.winner.as_deref(), Some("white"));

        let mut state = source_sparse_play();
        state.set_flag("racingKing", Color::White, true);
        state.extra.insert("collapsed".into(), json!(true));
        state.extra.insert("collapseDepth".into(), json!(2));
        state.board[1][4] = Some(Piece::new("king", Color::White, "earlier-royal"));
        state.board[2][4] = Some(Piece::new("king", Color::White, "later-royal"));
        assert!(!check_racing_kings_v7(&mut state).unwrap());
        assert_eq!(state.mode, "play");
    }

    #[test]
    fn source_initiative_break_uses_geometric_check_and_preserves_rng() {
        let mut state = source_sparse_play();
        state.board[4][0] = Some(Piece::new("rook", Color::White, "white-rook"));
        state.board[4][4] = Some(Piece::new("king", Color::Black, "black-king"));
        state.extra.insert(
            "initiative".into(),
            json!({"white":null,"black":{"by":"white","startTurn":0,"limit":7}}),
        );
        let rng = state.rng.clone();
        assert!(break_initiative_by_check_v7(&mut state, Color::White).unwrap());
        assert_eq!(state.extra["initiative"]["black"], Value::Null);
        assert_eq!(state.rng, rng);
        assert_eq!(
            state.extra["logs"][0],
            json!("선공권 제한이 해제되었습니다.")
        );
    }

    #[test]
    fn source_double_check_geometry_precedes_capture_locks_and_counts_unique_visible_attackers() {
        // main98748 returns geometry before Frozen/Disarm/protection policy;
        // main109667 still requires two distinct, visible physical identities.
        let mut state = source_sparse_play();
        state.set_flag("binaMate", Color::White, true);
        let mut rook = Piece::new("rook", Color::White, "white-rook");
        rook.extra.insert("frozen".into(), json!(true));
        rook.extra.insert("disarmed".into(), json!({"remaining":2}));
        let mut royal = Piece::new("king", Color::Black, "black-king");
        royal.extra.insert("protected".into(), json!(true));
        state.board[4][0] = Some(rook);
        state.board[1][1] = Some(Piece::new("bishop", Color::White, "white-bishop"));
        state.board[4][4] = Some(royal);
        assert!(check_double_check_victory_v7(&state, Color::White, false).unwrap());
        assert!(
            !piece_attacks_square_v7(
                &state,
                state.board[4][0].as_ref().unwrap(),
                Square { row: 4, col: 0 },
                Square { row: 4, col: 4 }
            )
            .unwrap()
        );
        state.board[1][1]
            .as_mut()
            .unwrap()
            .extra
            .insert("hiddenFrom".into(), json!("black"));
        assert!(!check_double_check_victory_v7(&state, Color::White, false).unwrap());
        state.board[1][1]
            .as_mut()
            .unwrap()
            .extra
            .shift_remove("hiddenFrom");
        state
            .extra
            .insert("scarecrowPieceReservation".into(), json!(false));
        assert!(!check_double_check_victory_v7(&state, Color::White, false).unwrap());
    }

    #[test]
    fn source_killer_king_and_imperial_attacks_observe_their_distinct_capture_policies() {
        let mut state = source_sparse_play();
        state.board[7][4] = Some(Piece::new("king", Color::White, "white-king"));
        state.board[4][4] = Some(Piece::new("king", Color::Black, "black-king"));
        state.set_flag("killerKing", Color::White, true);
        assert!(
            is_square_attacked_v7(&state, Square { row: 4, col: 4 }, Color::White, None, None)
                .unwrap()
        );
        state.board[4][4]
            .as_mut()
            .unwrap()
            .extra
            .insert("protected".into(), json!(true));
        assert!(
            !is_square_attacked_v7(&state, Square { row: 4, col: 4 }, Color::White, None, None)
                .unwrap()
        );
        state.set_flag("killerKing", Color::White, false);
        state.set_flag("imperialStudies", Color::White, true);
        state.board[7][4]
            .as_mut()
            .unwrap()
            .extra
            .insert("imperialMoves".into(), json!(["bishop", "king", "bishop"]));
        state.board[4][1] = state.board[4][4].take();
        let before = state.clone();
        assert!(
            is_square_attacked_v7(&state, Square { row: 4, col: 1 }, Color::White, None, None)
                .unwrap()
        );
        assert_eq!(state, before);
    }

    #[test]
    fn source_large_attacks_use_sector_anchors_and_raw_body_origin_projection() {
        let mut state = source_sparse_play();
        let mut large = Piece::new("colossus", Color::White, "white-large");
        large.extra.insert("anchorRow".into(), json!(4));
        large.extra.insert("anchorCol".into(), json!(4));
        for row in 4..=5 {
            for col in 4..=5 {
                state.board[row][col] = Some(large.clone());
            }
        }
        assert!(
            piece_attacks_square_v7(
                &state,
                state.board[5][5].as_ref().unwrap(),
                Square { row: 5, col: 5 },
                Square { row: 1, col: 1 }
            )
            .unwrap()
        );
        assert!(
            !piece_attacks_square_v7(
                &state,
                state.board[5][5].as_ref().unwrap(),
                Square { row: 5, col: 5 },
                Square { row: 3, col: 1 }
            )
            .unwrap()
        );

        let mut state = source_sparse_play();
        let mut rook = Piece::new("bigRook", Color::White, "white-big-rook");
        rook.extra.insert("anchorRow".into(), json!(5));
        rook.extra.insert("anchorCol".into(), json!(3));
        for row in 5..=6 {
            for col in 3..=4 {
                state.board[row][col] = Some(rook.clone());
            }
        }
        state.board[5][5] = Some(Piece::new("pawn", Color::White, "friendly-landing"));
        let from = Square { row: 5, col: 3 };
        let target = Square { row: 5, col: 5 };
        assert!(piece_attacks_square_v7(&state, &rook, from, target).unwrap());
        state.board[5][5]
            .as_mut()
            .unwrap()
            .extra
            .insert("protected".into(), json!(true));
        assert!(!piece_attacks_square_v7(&state, &rook, from, target).unwrap());
    }

    #[test]
    fn source_probe_removal_metadata_is_private_and_replaces_only_by_priority() {
        // main85285..85335: live callbacks never create this diagnostic DTO;
        // a still-alive royal needs terminal=true, then equal priority is last-wins.
        let mut state = source_sparse_play();
        let at = Square { row: 4, col: 4 };
        let royal = Piece::new("king", Color::Black, "black-king");
        let attacker = Piece::new("queen", Color::White, "white-queen");
        state.board[4][4] = Some(royal.clone());
        state.board[4][0] = Some(attacker.clone());
        let direct = json!({"attacker":attacker,"origin":{"row":4,"col":0}});
        let before = state.clone();
        mark_king_threat_removal_cause(&mut state, &royal, at, &direct, false).unwrap();
        assert_eq!(state, before);
        mark_king_threat_removal_cause(&mut state, &royal, at, &direct, true).unwrap();
        assert_eq!(state, before);
        state.board[4][4] = None;
        mark_king_threat_removal_cause(&mut state, &royal, at, &direct, true).unwrap();
        let cause = &state.extra["kingThreatCaptureCauses"][0];
        assert_eq!(cause["entry"]["attackerOriginKey"], json!("4-0"));
        assert_eq!(cause["entry"]["notation"], json!("Qxe4"));
        assert_eq!(cause["entry"]["direct"], json!(true));
        mark_king_threat_removal_cause(&mut state, &royal, at, &json!({"label":"폭탄"}), true)
            .unwrap();
        mark_king_threat_removal_cause(&mut state, &royal, at, &direct, true).unwrap();
        assert_eq!(
            state.extra["kingThreatCaptureCauses"][0]["entry"]["notation"],
            json!("폭탄 (e4)")
        );
        mark_king_threat_removal_cause(&mut state, &royal, at, &json!({"label":"낙뢰"}), true)
            .unwrap();
        assert_eq!(
            state.extra["kingThreatCaptureCauses"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            state.extra["kingThreatCaptureCauses"][0]["entry"]["effect"],
            json!("낙뢰")
        );
        let rng = state.rng.clone();
        mark_king_threat_effect_cause(&mut state, " 폭탄 ", true).unwrap();
        mark_king_threat_effect_cause(&mut state, "폭탄", true).unwrap();
        assert_eq!(state.extra["kingThreatEffectCauses"], json!(["폭탄"]));
        assert_eq!(state.rng, rng);
    }

    /// 합성 중간 callback 입력은 topology 정리 이전의 기물을 포함할 수 있다.
    /// 내부 fixture importer에서 전체 envelope identity/DTO/RNG/history를
    /// 검증한 뒤 실제 callback과 snapshot settle을 비교한다. 이 결과는
    /// 공개 V7HostPosition의 spatial admission 성공을 의미하지 않는다.
    #[test]
    #[ignore = "external frozen-source receipt is generated outside Git"]
    fn frozen_threat_flow_when_receipts_are_supplied() {
        use crate::tests::source_callback_fixture::{
            collect_case_diagnostics, compare_callback_envelope, compare_value,
            source_callback_state,
        };
        let path = std::env::var("ACCELERATE_V7_THREAT_FLOW_CASES").unwrap();
        let input = std::fs::read_to_string(path).unwrap();
        let mut failures = Vec::new();
        let mut count = 0;
        for line in input.lines().filter(|line| !line.trim().is_empty()) {
            count += 1;
            assert!(
                count <= 128,
                "frozen threat receipt exceeds its bounded case set"
            );
            let case: Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                case["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            let name = case["id"].as_str().unwrap();
            collect_case_diagnostics(name, &mut failures, |mismatches| {
                if case["fixtureKind"] != "synthetic-contract-source-probe" {
                    return Err(EngineError::InvalidState(
                        "threat receipt requires an explicit internal callback fixture boundary"
                            .into(),
                    ));
                }
                let mut state = source_callback_state(&case["before"])?;
                let (previous_ai, previous_threat) =
                    (state.ai_simulation_depth, state.threat_probe_depth);
                if case["fixture"]["probe"].as_bool() == Some(true) {
                    state.ai_simulation_depth = 1;
                    state.threat_probe_depth = 1;
                }
                let returned = run_frozen_threat_operation(&mut state, &case["fixture"]);
                // The source invoke's finally restores both module globals
                // before snapshot() settles deferred replay microtasks.
                state.ai_simulation_depth = previous_ai;
                state.threat_probe_depth = previous_threat;
                compare_value(
                    &case["returned"],
                    &returned?,
                    &format!("{name}.returned"),
                    mismatches,
                )?;
                if let Some(raw_after) = case.get("rawAfter") {
                    compare_callback_envelope(
                        &state,
                        raw_after,
                        &format!("{name}/callback"),
                        mismatches,
                    )?;
                }
                crate::replay::settle(&mut state)?;
                compare_callback_envelope(
                    &state,
                    &case["after"],
                    &format!("{name}/settled"),
                    mismatches,
                )
            });
        }
        assert!(count > 0, "frozen threat receipt contains no cases");
        eprintln!(
            "compared {count} source royal-threat/turn-flow callback cases including full state, RNG, history and envelope identity"
        );
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    fn run_frozen_threat_operation(state: &mut GameState, fixture: &Value) -> Result<Value> {
        let args = &fixture["args"];
        let actor = match args["actor"].as_str() {
            Some("black") => Color::Black,
            Some("white") | None => Color::White,
            Some(other) => {
                return Err(EngineError::InvalidState(format!(
                    "unknown threat receipt actor {other}"
                )));
            }
        };
        let square = |field: &str| {
            serde_json::from_value::<Square>(args[field].clone())
                .map_err(EngineError::serialization)
        };
        let returned = match fixture["op"].as_str().unwrap() {
            "square_attack" => {
                let target = square("target")?;
                let override_piece = args
                    .get("override")
                    .map(|value| {
                        serde_json::from_value::<Piece>(value.clone())
                            .map_err(EngineError::serialization)
                    })
                    .transpose()?;
                let ignored = if args.get("ignoreFrom").is_some() {
                    Some(BTreeSet::from([state
                        .at(square("ignoreFrom")?)
                        .ok_or(EngineError::IllegalAction)?
                        .id
                        .clone()]))
                } else {
                    None
                };
                json!(is_square_attacked_v7(
                    state,
                    target,
                    actor,
                    override_piece.as_ref(),
                    ignored.as_ref()
                )?)
            }
            "piece_attack" => {
                let from = square("from")?;
                let piece = state.at(from).ok_or(EngineError::IllegalAction)?;
                json!(piece_attacks_square_v7(
                    state,
                    piece,
                    from,
                    square("target")?
                )?)
            }
            "double_check" => json!(check_double_check_victory_v7(
                state,
                actor,
                state.threat_probe_depth > 0
            )?),
            "herald" => json!(resolve_herald_threats_v7(state, actor)?),
            "racing" => json!(check_racing_kings_v7(state)?),
            "initiative" => json!(break_initiative_by_check_v7(state, actor)?),
            "no_action" => json!(crate::v7_turn_flow::check_no_action_loss_for_color_v7(
                state, actor
            )?),
            "piece_action" => json!(crate::v7_turn_flow::has_available_piece_action(
                state, actor
            )),
            "sound" => {
                reconcile_move_replay_capture_v7(state)?;
                Value::Null
            }
            "crown_capture_sound" => {
                reconcile_move_replay_capture_v7(state)?;
                Value::Null
            }
            "royal_check" => json!(probe_royal_capture(state, actor, false)?.check),
            "royal_level" => {
                let report = probe_royal_capture(state, actor, true)?;
                json!(if report.check {
                    "check"
                } else if report.danger {
                    "danger"
                } else {
                    ""
                })
            }
            "removal_cause" => {
                let at = square("victim")?;
                let victim = state.board[at.row as usize][at.col as usize]
                    .take()
                    .ok_or(EngineError::IllegalAction)?;
                let probe = state.threat_probe_depth > 0;
                for source in args["sources"].as_array().unwrap() {
                    let mut metadata = source.clone();
                    if let Some(from) = metadata
                        .as_object_mut()
                        .unwrap()
                        .shift_remove("attackerFrom")
                    {
                        let from = serde_json::from_value::<Square>(from)
                            .map_err(EngineError::serialization)?;
                        metadata["attacker"] =
                            serde_json::to_value(state.at(from).ok_or(EngineError::IllegalAction)?)
                                .map_err(EngineError::serialization)?;
                    }
                    mark_king_threat_removal_cause(state, &victim, at, &metadata, probe)?;
                }
                for label in args["effects"].as_array().into_iter().flatten() {
                    mark_king_threat_effect_cause(state, label.as_str().unwrap(), probe)?;
                }
                Value::Null
            }
            other => {
                return Err(EngineError::InvalidState(format!(
                    "unknown threat receipt operation {other}"
                )));
            }
        };
        Ok(returned)
    }
}
