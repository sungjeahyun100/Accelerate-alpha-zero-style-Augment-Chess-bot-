//! Source-pinned v7 reservations settled at distinct completed-turn stages.
//!
//! The frozen client runs the first three callbacks immediately after Othello,
//! scarecrows after deathmatch and prophecy, ultimatum after periodic collapse,
//! and free moves after capture-lock reset. The host must call these
//! entry points at those boundaries, never as one reordered batch. Each entry
//! point owns a transaction copy so an unported active branch cannot expose a
//! partially decremented queue to a caller.

use crate::v7_turn_flow::V7FlowControl;
use crate::{
    Color, EngineError, Fields, GameState, Piece, PieceColor, RULES_VERSION_V7, Result, Square,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn unsupported(callback: &str, reason: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!("v7 queued {callback}: {reason}"))
}

fn run_owned(
    state: &mut GameState,
    actor: Color,
    callback: impl FnOnce(&mut GameState, Color) -> Result<V7FlowControl>,
) -> Result<V7FlowControl> {
    run_owned_with_terminal_admission(state, actor, TerminalAdmission::Stop, callback)
}

enum TerminalAdmission {
    Stop,
    SettleGale,
}

fn run_owned_with_terminal_admission(
    state: &mut GameState,
    actor: Color,
    terminal_admission: TerminalAdmission,
    callback: impl FnOnce(&mut GameState, Color) -> Result<V7FlowControl>,
) -> Result<V7FlowControl> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 queued effects on rules version {}",
            state.ruleset_id
        )));
    }
    let terminal = state.mode == "gameover";
    if terminal && matches!(terminal_admission, TerminalAdmission::Stop) {
        return Ok(V7FlowControl::Terminal);
    }
    // These are internal outgoing-turn callbacks. Spy promotion can settle a
    // different moving color from state.turn; public action admission already
    // checks the submitted actor before reaching this stage.
    if state.mode != "play" && !terminal {
        return Err(EngineError::WrongActor);
    }
    let mut next = state.clone();
    let control = callback(&mut next, actor)?;
    *state = next;
    Ok(control)
}

/// Run only the consecutive source callbacks after Othello and the turn count:
/// gale, Greek Gift, then Taboo. Later queues have intervening source effects
/// and must use their own stage methods below. Source main93671-93676 settles
/// Gale once even if Othello already ended the game, then gates later queues.
pub(crate) fn after_count(state: &mut GameState, actor: Color) -> Result<V7FlowControl> {
    run_owned_with_terminal_admission(
        state,
        actor,
        TerminalAdmission::SettleGale,
        |next, actor| {
            settle_gales(next, actor)?;
            if next.mode == "gameover" {
                return Ok(V7FlowControl::Terminal);
            }
            settle_greek_gift(next, actor)?;
            if next.mode == "gameover" {
                return Ok(V7FlowControl::Terminal);
            }
            settle_taboo(next, actor)?;
            Ok(if next.mode == "gameover" {
                V7FlowControl::Terminal
            } else {
                V7FlowControl::Continue
            })
        },
    )
}

/// Call after the source's deathmatch and prophecy callbacks, before its
/// initiative and dice-lock ticks.
pub(crate) fn after_prophecy(state: &mut GameState, actor: Color) -> Result<V7FlowControl> {
    run_owned(state, actor, |next, actor| {
        tick_scarecrows(next, actor)?;
        Ok(if next.mode == "gameover" {
            V7FlowControl::Terminal
        } else {
            V7FlowControl::Continue
        })
    })
}

/// Call after full-move cleanup, winter cycle and the board automata's
/// periodic-collapse callback. The source may return when Ultimatum ends the
/// game.
pub(crate) fn after_winter(state: &mut GameState, actor: Color) -> Result<V7FlowControl> {
    run_owned(state, actor, |next, _actor| {
        tick_ultimatum(next)?;
        Ok(if next.mode == "gameover" {
            V7FlowControl::Terminal
        } else {
            V7FlowControl::Continue
        })
    })
}

/// Call after the host copies and clears turn captures and unlocks the actor's
/// free-move capture lock, before the source's Magic Girl refresh.
pub(crate) fn after_capture_reset(state: &mut GameState, actor: Color) -> Result<V7FlowControl> {
    run_owned(state, actor, |next, actor| {
        settle_free_moves(next, actor)?;
        Ok(if next.mode == "gameover" {
            V7FlowControl::Terminal
        } else {
            V7FlowControl::Continue
        })
    })
}

/// Source main99418. The host invokes this immediately after `moveCount++`,
/// before previous Trickster ability cleanup and undead resurrection.
pub(crate) fn resolve_pending_lobsters_after_move(state: &mut GameState) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(unsupported(
            "resolvePendingLobstersAfterMove",
            "requires v7",
        ));
    }
    if state.mode == "gameover" {
        return Ok(());
    }
    let Some(entries) = state
        .extra
        .get("pendingLobsters")
        .and_then(Value::as_array)
        .cloned()
    else {
        return Ok(());
    };
    if entries.is_empty() {
        return Ok(());
    }
    if entries.len() > 256 {
        return Err(EngineError::InvalidState(
            "v7 pendingLobsters exceeds 256 entries".into(),
        ));
    }
    let mut next = state.clone();
    let mut due = Vec::new();
    let mut future = Vec::new();
    for mut entry in entries {
        let Some(square) = free_move_plan_square(Some(&entry)) else {
            continue;
        };
        let remaining =
            crate::observation::number(entry.get("remainingHalfTurns")).filter(|value| {
                value.is_finite() && value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_991.0
            });
        let remaining = if let Some(remaining) = remaining.filter(|value| *value >= 0.0) {
            let value = (remaining - 1.0).max(0.0) as i64;
            entry["remainingHalfTurns"] = json!(value);
            Some(value as f64)
        } else {
            remaining
        };
        let is_due = remaining.map_or_else(
            || {
                crate::observation::number(entry.get("dueMoveCount"))
                    .is_some_and(|value| value <= f64::from(next.move_count))
            },
            |value| value <= 0.0,
        );
        if is_due {
            due.push((square, entry));
        } else {
            future.push(entry);
        }
    }
    next.extra.insert("pendingLobsters".into(), json!(future));
    for (square, entry) in due {
        let owner = match entry.get("color").and_then(Value::as_str) {
            Some("white") => Color::White,
            Some("black") => Color::Black,
            _ => {
                return Err(EngineError::InvalidState(
                    "v7 pendingLobsters.color must name a player".into(),
                ));
            }
        };
        if !installation_open(&next, square, owner)?
            || !crate::movement::d4_destination_allowed(&next, owner.into(), &[square])
        {
            crate::replay::add_log(
                &mut next,
                format!(
                    "랍스터: {}이 막혀 소환이 취소되었습니다.",
                    source_square_name(state, square.row.into(), square.col.into())
                ),
            )?;
            continue;
        }
        crush_concealed_installation_occupant(&mut next, square, owner)?;
        let mut summoned = crate::opening::spawn(&mut next, owner, "lobster")?;
        summoned.extra.insert(
            "origin".into(),
            json!(source_square_name(
                state,
                square.row.into(),
                square.col.into()
            )),
        );
        summoned.moved = true;
        summoned.extra.insert(
            "freshNoCaptureUntil".into(),
            json!(next.turns_taken.get(owner).checked_add(1).ok_or_else(|| {
                EngineError::InvalidState("v7 lobster fresh capture lock overflow".into())
            })?),
        );
        next.board[usize::from(square.row)][usize::from(square.col)] = Some(summoned.clone());
        crate::card_effects::mark_animation(&mut next, &summoned)?;
        crate::replay::add_log(
            &mut next,
            format!(
                "랍스터: {}에 소환되었습니다.",
                source_square_name(state, square.row.into(), square.col.into())
            ),
        )?;
    }
    *state = next;
    Ok(())
}

pub(crate) fn crown_ground_at(state: &GameState, square: Square) -> bool {
    let Some(rule) = state.extra.get("crownRule") else {
        return false;
    };
    let entries = rule
        .get("crowns")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty());
    let at = |entry: &Value| {
        if !js_truth(entry) || crate::observation::truth(entry.get("removed")) {
            return false;
        }
        if entry == &Value::Bool(true) {
            return square == (Square { row: 3, col: 3 });
        }
        // main101657 requires Number.isInteger without numeric-string coercion.
        let coordinate = |field| {
            entry
                .get("ground")
                .and_then(|ground| ground.get(field))
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && value.fract() == 0.0)
        };
        coordinate("row") == Some(f64::from(square.row))
            && coordinate("col") == Some(f64::from(square.col))
    };
    entries.map_or_else(|| at(rule), |entries| entries.iter().any(at))
}

fn installation_open(state: &GameState, square: Square, owner: Color) -> Result<bool> {
    if crown_ground_at(state, square) || crate::movement::quantum_occupied(state, square)? {
        return Ok(false);
    }
    let concealed = state.at(square).is_some_and(|piece| {
        piece.color == owner.opponent()
            && crate::observation::piece_hidden_from_v7(state, piece, square) == Some(owner)
    });
    if state.at(square).is_some() && !concealed {
        return Ok(false);
    }
    let mut probe = state.clone();
    probe.board[usize::from(square.row)][usize::from(square.col)] = None;
    crate::movement::open_placement(&probe, square, None)
}

fn crush_concealed_installation_occupant(
    state: &mut GameState,
    square: Square,
    owner: Color,
) -> Result<()> {
    let Some(piece) = state.at(square).cloned().filter(|piece| {
        piece.color == owner.opponent()
            && crate::observation::piece_hidden_from_v7(state, piece, square) == Some(owner)
    }) else {
        return Ok(());
    };
    crate::transition::remove_piece_from_board_cells(state, &piece, square)?;
    crate::transition::grant_vigilance_protection(state, &piece)?;
    crate::flow::mark_progress(state);
    let threat_probe = state.threat_probe_depth > 0;
    crate::v7_threat::mark_king_threat_removal_cause(
        state,
        &piece,
        square,
        &json!({"label":"설치물"}),
        threat_probe,
    )?;
    crate::transition::resolve_royal_capture(state, &piece, owner)?;
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(())
}

fn strict_integer(value: Option<&Value>, context: &str) -> Result<i64> {
    value
        .and_then(Value::as_i64)
        .ok_or_else(|| EngineError::InvalidState(format!("{context} must be an integer")))
}

fn color_field<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}

fn source_square_name(state: &GameState, row: usize, col: usize) -> String {
    format!(
        "{}{}",
        char::from(b'a' + col as u8),
        state.board.len() - row
    )
}

fn settle_gales(state: &mut GameState, actor: Color) -> Result<()> {
    let Some(entries) = state.extra.get("pendingGales").and_then(Value::as_array) else {
        return Ok(());
    };
    if entries.is_empty() {
        return Ok(());
    }
    let mut updated = entries.clone();
    let mut due_ids = BTreeSet::new();
    let mut any_due = false;
    for entry in &mut updated {
        let object = entry.as_object_mut().ok_or_else(|| {
            EngineError::InvalidState("pendingGales entry must be an object".into())
        })?;
        let color = object.get("color").and_then(Value::as_str);
        let due = if object.contains_key("remainingOwnTurns") {
            let remaining = strict_integer(
                object.get("remainingOwnTurns"),
                "pendingGales.remainingOwnTurns",
            )?;
            let remaining = if color == Some(actor.opponent().as_str()) {
                remaining.checked_sub(1).ok_or_else(|| {
                    EngineError::InvalidState("pendingGales countdown overflow".into())
                })?
            } else {
                remaining
            };
            if color == Some(actor.opponent().as_str()) {
                object.insert("remainingOwnTurns".into(), json!(remaining));
            }
            remaining <= 0
        } else {
            let trigger = object
                .get("triggerTurn")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            color == Some(actor.as_str()) && i64::from(*state.turns_taken.get(actor)) >= trigger
        };
        if due {
            any_due = true;
            due_ids.insert(object.get("id").cloned().unwrap_or(Value::Null).to_string());
        }
    }
    if !any_due {
        state
            .extra
            .insert("pendingGales".into(), Value::Array(updated));
        return Ok(());
    }
    // The source snapshots every doomed identity before changing any board
    // cell. Preserve that order for captures, vigilance and the one notation.
    let victims = gale_victims(state)?;
    updated.retain(|entry| {
        !due_ids.contains(&entry.get("id").cloned().unwrap_or(Value::Null).to_string())
    });
    state
        .extra
        .insert("pendingGales".into(), Value::Array(updated));
    if victims.is_empty() {
        crate::replay::add_log(state, "강풍: 제거될 기물이 없었습니다.".into())?;
        return Ok(());
    }
    for victim in &victims {
        crate::transition::clear_piece(state, &victim.piece.id);
        crate::transition::grant_vigilance_protection(state, &victim.piece)?;
        state
            .captures
            .get_mut(victim.capture_owner)
            .push(victim.piece.clone());
    }
    // main98947 renders the whole removal batch before progress and notation.
    // Terminal Othello still creates these ghosts and updates animation Sets.
    for victim in &victims {
        crate::card_effects::mark_vanish_animation(state, &victim.piece, victim.square)?;
    }
    crate::flow::mark_progress(state);
    crate::replay::add_log(
        state,
        format!(
            "강풍: 아군과 인접하지 않은 기물 {}개가 날아갔습니다.",
            victims.len()
        ),
    )?;
    crate::replay::queue_special_effect_notation(
        state,
        actor,
        "강풍",
        &format!(
            "{} 강풍으로 기물 {}개 제거",
            crate::replay::label(actor),
            victims.len()
        ),
    )?;
    let removals = victims
        .iter()
        .map(|victim| crate::v7_board_hazards::EnvironmentalRemoval {
            piece: victim.piece.clone(),
            square: victim.square,
            capture_owner: victim.capture_owner,
        })
        .collect::<Vec<_>>();
    crate::v7_board_hazards::resolve_environmental_defeats(state, &removals, "강풍", false)?;
    // The source checks campaign objectives after environmental defeat and
    // only for a gale that removed at least one identity.
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(())
}

struct GaleVictim {
    piece: Piece,
    square: Square,
    capture_owner: Color,
}

fn gale_victims(state: &GameState) -> Result<Vec<GaleVictim>> {
    struct Identity<'a> {
        piece: &'a Piece,
        cells: Vec<(usize, usize)>,
    }
    let september18 = uses_september18_balance(state)?;
    let september26 = uses_september26_rebalance(state)?;
    let mut identities: BTreeMap<&str, Identity<'_>> = BTreeMap::new();
    let mut encounter_order = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            let Some(piece) = piece else { continue };
            if !identities.contains_key(piece.id.as_str()) {
                encounter_order.push(piece.id.as_str());
            }
            identities
                .entry(piece.id.as_str())
                .or_insert_with(|| Identity {
                    piece,
                    cells: Vec::new(),
                })
                .cells
                .push((row, col));
        }
    }
    let major = |piece: &Piece| {
        !piece.flag("regencyHeir")
            && matches!(
                piece.kind.as_str(),
                "octopus"
                    | "grappler"
                    | "hedgehog"
                    | "princess"
                    | "bigBishop"
                    | "queen"
                    | "rook"
                    | "amazon"
                    | "man"
                    | "colossus"
                    | "bigRook"
                    | "herald"
                    | "jester"
                    | "hook"
                    | "primeMinister"
                    | "assassin"
                    | "windmill"
                    | "crown"
                    | "bear"
                    | "magicGirl"
                    | "berserker"
                    | "siren"
                    | "reaper"
                    | "undead"
            )
            || !september26
                && !piece.flag("regencyHeir")
                && matches!(
                    piece.kind.as_str(),
                    "clockwork" | "parrot" | "recruiter" | "wizard" | "trickster"
                )
    };
    let mut victims = Vec::new();
    for id in encounter_order {
        let identity = &identities[id];
        if identity.piece.color.owner().is_none() {
            continue;
        }
        if september18 {
            if !major(identity.piece) {
                continue;
            }
            let adjacent_major = identities.iter().any(|(other_id, other)| {
                *other_id != id
                    && major(other.piece)
                    && identity.cells.iter().any(|&(row, col)| {
                        other.cells.iter().any(|&(other_row, other_col)| {
                            row.abs_diff(other_row).max(col.abs_diff(other_col)) <= 1
                        })
                    })
            });
            if adjacent_major {
                continue;
            }
        } else {
            let adjacent_ally = identities.iter().any(|(other_id, other)| {
                *other_id != id
                    && other.piece.color == identity.piece.color
                    && identity.cells.iter().any(|&(row, col)| {
                        other.cells.iter().any(|&(other_row, other_col)| {
                            row.abs_diff(other_row).max(col.abs_diff(other_col)) == 1
                        })
                    })
            });
            if adjacent_ally {
                continue;
            }
        }
        victims.push(GaleVictim {
            piece: identity.piece.clone(),
            square: {
                let (row, col) = identity.cells[0];
                let first = Square {
                    row: row as u8,
                    col: col as u8,
                };
                free_move_normalized_origin(state, first).unwrap_or(first)
            },
            capture_owner: identity
                .piece
                .color
                .owner()
                .expect("checked owner")
                .opponent(),
        });
    }
    Ok(victims)
}

fn greek_gift_victims(state: &GameState, actor: Color) -> Result<Vec<(Square, Piece)>> {
    struct PawnIdentity {
        piece: Piece,
        cells: Vec<(usize, usize)>,
    }
    let enemy = actor.opponent();
    let mut royal_cells = Vec::new();
    let mut pawns: BTreeMap<&str, PawnIdentity> = BTreeMap::new();
    let mut pawn_order = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            let Some(piece) = piece else { continue };
            if piece.color != enemy {
                continue;
            }
            if state.royal_identity(piece) {
                royal_cells.push((row, col));
            } else if piece.kind == "pawn" {
                if !pawns.contains_key(piece.id.as_str()) {
                    pawn_order.push(piece.id.as_str());
                }
                pawns
                    .entry(piece.id.as_str())
                    .or_insert_with(|| PawnIdentity {
                        piece: piece.clone(),
                        cells: Vec::new(),
                    })
                    .cells
                    .push((row, col));
            }
        }
    }
    let mut victims = Vec::new();
    for id in pawn_order {
        let identity = &pawns[id];
        let cells = &identity.cells;
        if cells.iter().any(|&(row, col)| {
            royal_cells.iter().any(|&(royal_row, royal_col)| {
                row.abs_diff(royal_row).max(col.abs_diff(royal_col)) == 1
            })
        }) {
            if cells.len() != 1 {
                return Err(unsupported(
                    "resolveGreekGift",
                    "multi-cell pawn identity requires expansion removal",
                ));
            }
            let (row, col) = cells[0];
            victims.push((
                Square {
                    row: row as u8,
                    col: col as u8,
                },
                identity.piece.clone(),
            ));
        }
    }
    Ok(victims)
}

fn settle_greek_gift(state: &mut GameState, actor: Color) -> Result<()> {
    let Some(value) = state.extra.get("greekGiftPending") else {
        return Ok(());
    };
    if value.get(actor.as_str()).is_none_or(|flag| !js_truth(flag)) {
        return Ok(());
    }
    let victims = greek_gift_victims(state, actor)?;
    let pending = state
        .extra
        .get_mut("greekGiftPending")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState("greekGiftPending must be a color map".into()))?;
    pending.insert(actor.as_str().into(), Value::Bool(false));
    for (square, victim) in victims {
        if state.at(square).is_some_and(|piece| piece.id == victim.id) {
            crate::transition::expansion_effect_remove(state, square, actor, "그릭 기프트")?;
        }
    }
    Ok(())
}

fn insert_capture_type(state: &mut GameState, field: &str, actor: Color, kind: &str) -> Result<()> {
    let Some(entry) = state
        .extra
        .get_mut(field)
        .and_then(|field| field.get_mut(actor.as_str()))
    else {
        return Ok(());
    };
    let values = entry
        .get_mut("values")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState(format!("{field} must contain source Sets")))?;
    let kind = json!(kind);
    if !values.contains(&kind) {
        values.push(kind);
    }
    Ok(())
}

fn settle_taboo(state: &mut GameState, actor: Color) -> Result<()> {
    let Some(entries) = state.extra.get("tabooPending").and_then(Value::as_array) else {
        return Ok(());
    };
    let mut remaining = Vec::with_capacity(entries.len());
    let mut due = Vec::new();
    for entry in entries {
        if entry.as_object().is_none() {
            return Err(EngineError::InvalidState(
                "tabooPending entry must be an object".into(),
            ));
        }
        if color_field(entry, "color") == Some(actor.as_str()) {
            due.push(entry.clone());
        } else {
            remaining.push(entry.clone());
        }
    }
    if due.is_empty() {
        return Ok(());
    }
    for entry in &due {
        if state.mode == "gameover" {
            break;
        }
        let source = entry.get("pieceId").and_then(Value::as_str).and_then(|id| {
            state.board.iter().enumerate().find_map(|(row, cells)| {
                cells.iter().enumerate().find_map(|(col, piece)| {
                    piece.as_ref().filter(|piece| piece.id == id).map(|piece| {
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
        });
        let Some((source_square, source)) = source else {
            continue;
        };
        if source.color != actor {
            continue;
        }
        let removed =
            crate::transition::expansion_sacrifice(state, source_square, actor.opponent())?;
        if state.mode == "gameover" {
            break;
        }
        if removed.is_none() {
            continue;
        }
        let target = entry.get("square").and_then(|square| {
            Some((
                usize::try_from(square.get("row")?.as_u64()?).ok()?,
                usize::try_from(square.get("col")?.as_u64()?).ok()?,
            ))
        });
        let mut seen = BTreeSet::new();
        for (row, col) in [(
            usize::from(source_square.row),
            usize::from(source_square.col),
        )]
        .into_iter()
        .chain(target)
        {
            if !seen.insert((row, col)) {
                continue;
            }
            let Some(occupied) = state
                .board
                .get(row)
                .and_then(|cells| cells.get(col))
                .map(Option::is_some)
            else {
                continue;
            };
            if occupied {
                crate::transition::expansion_effect_remove(
                    state,
                    Square {
                        row: row as u8,
                        col: col as u8,
                    },
                    actor,
                    "뒤틀린 소환",
                )?;
            }
            if !crate::movement::open_placement(
                state,
                Square {
                    row: row as u8,
                    col: col as u8,
                },
                None,
            )? {
                continue;
            }
            let monster = taboo_monster(state, row, col)?;
            state.board[row][col] = Some(monster.clone());
            crate::card_effects::mark_animation(state, &monster)?;
        }
    }
    state
        .extra
        .insert("tabooPending".into(), Value::Array(remaining));
    Ok(())
}

fn taboo_monster(state: &mut GameState, row: usize, col: usize) -> Result<Piece> {
    let id = format!(
        "neutral-monster-{}",
        crate::draft::random_suffix(state.rng.sample_opaque("source taboo monster identity")?)?
    );
    let mut extra = Fields::new();
    extra.insert("shielded".into(), Value::Bool(false));
    extra.insert("origin".into(), json!(source_square_name(state, row, col)));
    Ok(Piece {
        kind: "monster".into(),
        color: PieceColor::Neutral,
        moved: true,
        id,
        extra,
        source_order: ["color", "type", "moved", "shielded", "id", "origin"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    })
}

const SEPTEMBER18_HASHES: &[&str] = &[
    "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
    "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
    "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs",
    "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc",
    "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g",
    "HVzv4vxzNKDRi-ylshTw9HnLmIlyiQhMScQ4caG_aSI",
];

fn source_catalog_hash(state: &GameState) -> Option<&str> {
    match state.extra.get("cardState").filter(|value| js_truth(value)) {
        Some(card_state) => card_state.get("profile"),
        None => state.extra.get("profile"),
    }
    .and_then(|profile| profile.get("catalogHash"))
    .and_then(Value::as_str)
}

pub(crate) fn uses_september18_balance(state: &GameState) -> Result<bool> {
    if let Some(hash) = source_catalog_hash(state) {
        if !SEPTEMBER18_HASHES.contains(&hash) {
            return Err(unsupported(
                "source profile",
                "unrecognized source catalog hash",
            ));
        }
        return Ok(true);
    }
    Ok(state.extra.get("september18Balance") != Some(&Value::Bool(false)))
}

pub(crate) fn uses_september26_rebalance(state: &GameState) -> Result<bool> {
    let Some(hash) = source_catalog_hash(state) else {
        return Ok(true);
    };
    if !SEPTEMBER18_HASHES.contains(&hash) {
        return Err(unsupported(
            "resolvePendingGalesAfterTurn",
            "unrecognized source catalog hash",
        ));
    }
    Ok(SEPTEMBER18_HASHES[..3].contains(&hash))
}

fn tick_scarecrows(state: &mut GameState, actor: Color) -> Result<()> {
    let Some(entries) = state
        .extra
        .get("pendingScarecrows")
        .and_then(Value::as_array)
    else {
        return Ok(());
    };
    if entries.is_empty() {
        return Ok(());
    }
    let entries = entries.clone();
    let september18 = uses_september18_balance(state)?;
    let mut remaining = Vec::with_capacity(entries.len());
    let mut due = Vec::new();
    for entry in &entries {
        let Some(object) = entry.as_object() else {
            // The source discards falsy and non-object entries without a cell.
            continue;
        };
        let by = object.get("by").and_then(Value::as_str);
        if let Some(raw_piece_id) = object.get("pieceId").filter(|id| js_truth(id)) {
            let Some(piece_id) = raw_piece_id.as_str() else {
                // Source identity matching is strict; a non-string cannot
                // match a validated native Piece id and the entry expires.
                continue;
            };
            let found = state.board.iter().enumerate().find_map(|(row, cells)| {
                cells.iter().enumerate().find_map(|(col, cell)| {
                    cell.as_ref()
                        .filter(|piece| piece.id == piece_id)
                        .map(|piece| (row, col, piece))
                })
            });
            let Some((row, col, piece)) = found else {
                continue;
            };
            let previous = piece.clone();
            let decrement_owner = if september18 { actor.opponent() } else { actor };
            let remaining_turns = strict_integer(
                object.get("remainingOwnTurns"),
                "pendingScarecrows.remainingOwnTurns",
            )? - i64::from(by == Some(decrement_owner.as_str()));
            if remaining_turns <= 0 {
                let owner = previous.color.owner().ok_or_else(|| {
                    EngineError::InvalidState("reserved scarecrow must have an owner".into())
                })?;
                crate::transition::clear_piece(state, &previous.id);
                let mut transformed = Piece::new("scarecrow", previous.color, previous.id.clone());
                transformed.moved = true;
                transformed
                    .extra
                    .insert("origin".into(), json!(source_square_name(state, row, col)));
                state.board[row][col] = Some(transformed.clone());
                crate::card_effects::mark_animation(state, &transformed)?;
                crate::transition::resolve_royal_capture(state, &previous, owner.opponent())?;
                continue;
            }
            let mut next = entry.clone();
            let next_object = next.as_object_mut().expect("checked object");
            next_object.insert("row".into(), json!(row));
            next_object.insert("col".into(), json!(col));
            next_object.insert("remainingOwnTurns".into(), json!(remaining_turns));
            remaining.push(next);
            continue;
        }
        let (Some(row), Some(col)) = (
            object.get("row").and_then(Value::as_u64),
            object.get("col").and_then(Value::as_u64),
        ) else {
            continue;
        };
        if row >= state.board.len() as u64 || col >= state.board[row as usize].len() as u64 {
            continue;
        }
        let mut next = entry.clone();
        if object.get("reserved").is_some_and(js_truth) {
            let remaining_turns = strict_integer(
                object.get("remainingOwnTurns"),
                "pendingScarecrows.remainingOwnTurns",
            )? - i64::from(by == Some(actor.opponent().as_str()));
            if remaining_turns <= 0 {
                next["remainingOwnTurns"] = json!(remaining_turns);
                due.push(next);
                continue;
            }
            next["remainingOwnTurns"] = json!(remaining_turns);
        } else {
            let remaining_half_turns = object
                .get("remainingHalfTurns")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .saturating_sub(1)
                .max(0);
            if remaining_half_turns == 0 {
                next["remainingHalfTurns"] = json!(remaining_half_turns);
                due.push(next);
                continue;
            }
            next["remainingHalfTurns"] = json!(remaining_half_turns);
        }
        remaining.push(next);
    }
    state
        .extra
        .insert("pendingScarecrows".into(), Value::Array(remaining));
    let mut installed = Vec::new();
    for entry in due {
        let row = entry["row"]
            .as_u64()
            .ok_or_else(|| EngineError::InvalidState("due scarecrow row must be an index".into()))?
            as usize;
        let col = entry["col"]
            .as_u64()
            .ok_or_else(|| EngineError::InvalidState("due scarecrow col must be an index".into()))?
            as usize;
        let color = match entry["color"].as_str() {
            Some("white") => Color::White,
            Some("black") => Color::Black,
            _ => {
                return Err(unsupported(
                    "tickPendingScarecrowsAfterTurn",
                    "due installation owner is not white or black",
                ));
            }
        };
        let square = Square {
            row: row as u8,
            col: col as u8,
        };
        if !installation_open(state, square, color)? {
            continue;
        }
        // Direct installation crushing does not run ordinary capture credit.
        crush_concealed_installation_occupant(state, square, color)?;
        let mut piece = crate::opening::spawn(state, color, "scarecrow")?;
        piece.moved = true;
        piece
            .extra
            .insert("origin".into(), json!(source_square_name(state, row, col)));
        state.board[row][col] = Some(piece.clone());
        crate::card_effects::mark_animation(state, &piece)?;
        installed.push((row, col));
    }
    for (row, col) in installed {
        crate::replay::add_log(
            state,
            format!(
                "허수아비: {}에 설치되었습니다.",
                source_square_name(state, row, col)
            ),
        )?;
    }
    Ok(())
}

fn js_truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn tick_ultimatum(state: &mut GameState) -> Result<()> {
    let Some(ultimatum) = state.extra.get("ultimatum").filter(|value| js_truth(value)) else {
        return Ok(());
    };
    let object = ultimatum
        .as_object()
        .ok_or_else(|| EngineError::InvalidState("ultimatum must be an object".into()))?;
    for field in ["remainingHalfTurns", "remaining"] {
        if object.contains_key(field) && object.get(field).and_then(Value::as_i64).is_none() {
            return Err(EngineError::InvalidState(format!(
                "ultimatum.{field} must be an integer"
            )));
        }
    }
    if object.contains_key("expiresFullMove")
        && object
            .get("expiresFullMove")
            .and_then(Value::as_f64)
            .is_none()
    {
        return Err(EngineError::InvalidState(
            "ultimatum.expiresFullMove must be numeric".into(),
        ));
    }
    let remaining_half_turns = ultimatum.get("remainingHalfTurns").and_then(Value::as_i64);
    let expires_full_move = ultimatum.get("expiresFullMove").and_then(Value::as_f64);
    let (field, value, remaining) = if let Some(half_turns) = remaining_half_turns {
        let next = half_turns.saturating_sub(1).max(0);
        ("remainingHalfTurns", next, ((next + 1) / 2).clamp(0, 4))
    } else if let Some(expires) = expires_full_move {
        let rem = (expires - f64::from(state.full_move)).clamp(0.0, 4.0);
        if rem <= 0.0 {
            return settle_due_ultimatum(state);
        }
        let ultimatum = state
            .extra
            .get_mut("ultimatum")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| EngineError::InvalidState("ultimatum must be an object".into()))?;
        ultimatum.insert("remaining".into(), json!(rem));
        return Ok(());
    } else {
        let old = ultimatum
            .get("remaining")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let next = old.saturating_sub(1).max(0);
        ("remaining", next, next.clamp(0, 4))
    };
    if value == 0 {
        return settle_due_ultimatum(state);
    }
    let ultimatum = state
        .extra
        .get_mut("ultimatum")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState("ultimatum must be an object".into()))?;
    ultimatum.insert(field.into(), json!(value));
    ultimatum.insert("remaining".into(), json!(remaining));
    Ok(())
}

fn settle_due_ultimatum(state: &mut GameState) -> Result<()> {
    // The source snapshots every first-seen identity before removing board
    // cells. This also gives expanded pieces one capture instead of one per
    // occupied cell. Environmental adjudication runs once after the batch.
    let ultimatum = state
        .extra
        .get("ultimatum")
        .ok_or_else(|| EngineError::InvalidState("ultimatum is missing".into()))?;
    let by = match ultimatum.get("by").and_then(Value::as_str) {
        Some("white") => Color::White,
        Some("black") => Color::Black,
        _ => state.turn,
    };
    let moved = ultimatum.get("movedIds").and_then(Value::as_array);
    let moved = moved
        .into_iter()
        .flatten()
        .map(|id| match id {
            Value::String(id) => id.clone(),
            Value::Number(id) => id.to_string(),
            _ => String::new(),
        })
        .collect::<BTreeSet<_>>();
    let mut victims = Vec::new();
    let mut seen = BTreeSet::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, cell) in cells.iter().enumerate() {
            let Some(piece) = cell else { continue };
            if piece.color.owner().is_some()
                && !state.royal_identity(piece)
                && !matches!(piece.kind.as_str(), "merchant" | "wall" | "football")
                && seen.insert(piece.id.clone())
                && !moved.contains(&piece.id)
            {
                victims.push(crate::v7_board_hazards::EnvironmentalRemoval {
                    piece: piece.clone(),
                    square: Square {
                        row: row as u8,
                        col: col as u8,
                    },
                    capture_owner: if piece.color == by { by.opponent() } else { by },
                });
            }
        }
    }
    for victim in &victims {
        crate::transition::clear_piece(state, &victim.piece.id);
        cancel_prophecies_by_capture(state)?;
        state
            .captures
            .get_mut(victim.capture_owner)
            .push(victim.piece.clone());
        insert_capture_type(
            state,
            "capturedTypes",
            victim.capture_owner,
            &victim.piece.kind,
        )?;
    }
    state.extra.insert("ultimatum".into(), Value::Null);
    crate::replay::add_log(
        state,
        format!("최후 통첩: {}개의 기물이 제거되었습니다.", victims.len()),
    )?;
    if crate::v7_board_hazards::resolve_environmental_defeats(state, &victims, "최후 통첩", false)?
    {
        crate::v7_capture_objectives::check_campaign_objectives(state)?;
        return Ok(());
    }
    for color in [Color::White, Color::Black] {
        crate::v7_turn_entry::check_conscription(state, color)?;
    }
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(())
}

fn cancel_prophecies_by_capture(state: &mut GameState) -> Result<()> {
    let Some(value) = state.extra.get_mut("prophecy") else {
        return Ok(());
    };
    let map = value
        .as_object_mut()
        .ok_or_else(|| EngineError::InvalidState("prophecy must be a color map".into()))?;
    for color in [Color::White, Color::Black] {
        if map.get(color.as_str()).is_some_and(js_truth) {
            map.insert(color.as_str().into(), Value::Null);
        }
    }
    Ok(())
}

pub(crate) fn note_resolve_pawn_capture(state: &mut GameState, piece: &Piece) -> Result<()> {
    let Some(owner) = piece.color.owner() else {
        return Ok(());
    };
    if !state.flag("resolve", owner)
        || state
            .extra
            .get("resolveSpentTurn")
            .and_then(|turns| turns.get(owner.as_str()))
            .and_then(Value::as_i64)
            == Some(i64::from(*state.turns_taken.get(owner)))
    {
        return Ok(());
    }
    if state
        .extra
        .get("resolveReady")
        .is_none_or(|value| !js_truth(value))
    {
        state
            .extra
            .insert("resolveReady".into(), json!({"white":false,"black":false}));
    }
    let ready = state
        .extra
        .get_mut("resolveReady")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState("resolveReady must be a color map".into()))?;
    ready.insert(owner.as_str().into(), Value::Bool(true));
    Ok(())
}

fn settle_free_moves(state: &mut GameState, actor: Color) -> Result<()> {
    let Some(entries) = state
        .extra
        .get("pendingFreeMoves")
        .and_then(Value::as_array)
    else {
        return Ok(());
    };
    if entries.is_empty() {
        return Ok(());
    }
    if entries.len() > 256 {
        return Err(EngineError::InvalidState(
            "v7 pendingFreeMoves exceeds 256 entries".into(),
        ));
    }
    let mut due = Vec::new();
    let mut future = Vec::new();
    for entry in entries {
        let trigger = crate::observation::number(entry.get("triggerTurn"))
            .filter(|number| *number != 0.0)
            .unwrap_or(1.0)
            .floor()
            .max(1.0);
        if color_field(entry, "triggerColor") == Some(actor.as_str())
            && f64::from(*state.turns_taken.get(actor)) >= trigger
        {
            due.push(entry.clone());
        } else {
            future.push(entry.clone());
        }
    }
    if due.is_empty() {
        return Ok(());
    }
    if due.iter().any(|entry| {
        entry
            .get("moves")
            .is_some_and(|moves| !moves.is_null() && !matches!(moves, Value::Array(_)))
    }) {
        return Err(EngineError::InvalidState(
            "due pendingFreeMoves.moves must be an array".into(),
        ));
    }
    state
        .extra
        .insert("pendingFreeMoves".into(), Value::Array(future));
    crate::replay::normalize_color_booleans(state, "freeMoveCaptureLock");
    let previous_turn = state.turn;
    let previous_history_move_number = state.extra.get("activeHistoryMoveNumber").cloned();
    let mut traces = Vec::<Value>::new();
    let mut hidden_from = String::new();
    for entry in &due {
        if state.mode == "gameover" {
            break;
        }
        let color = match entry.get("color").and_then(Value::as_str) {
            Some("white") => Color::White,
            Some("black") => Color::Black,
            _ => {
                return Err(EngineError::InvalidState(
                    "v7 due pendingFreeMoves.color must name a player".into(),
                ));
            }
        };
        state.turn = color;
        state.extra.insert(
            "activeHistoryMoveNumber".into(),
            json!(state.full_move.max(1)),
        );
        for plan in entry
            .get("moves")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if state.mode == "gameover" {
                break;
            }
            let plan_id = plan
                .get("pieceId")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty());
            let planned_from = free_move_plan_square(plan.get("from"));
            let origin = planned_from.and_then(|square| free_move_normalized_origin(state, square));
            let located = origin.and_then(|square| state.at(square).cloned());
            let source_intact = located.as_ref().is_some_and(|piece| {
                plan_id == Some(piece.id.as_str()) && piece.color == color && origin == planned_from
            });
            if !source_intact {
                crate::replay::add_log(state, "프리 무브가 취소되었습니다.".into())?;
                continue;
            }
            let origin = origin.expect("intact source has a board origin");
            let piece = located.expect("intact source has a piece");
            let destination = free_move_plan_square(plan.get("to"));
            let legal = crate::movement::v7_legal_move_targets(
                state,
                &piece,
                origin,
                crate::movement::V7MoveOptions::default(),
            )?
            .into_iter()
            .find(|candidate| {
                Some(candidate.square()) == destination
                    && candidate.square() != origin
                    && ![
                        "colossusBody",
                        "colossusAttack",
                        "shotgunBlast",
                        "shotgunSnipe",
                        "merchantBuy",
                        "setLogDirection",
                        "dragonSwap",
                        "substitutionSwap",
                    ]
                    .into_iter()
                    .any(|flag| candidate.flag(flag))
            });
            let Some(candidate) = legal else {
                crate::replay::add_log(state, "프리 무브가 취소되었습니다.".into())?;
                continue;
            };
            let destination = candidate.square();
            let replay_count = free_move_history_count(state, "replayEvents");
            let history_count = free_move_history_count(state, "boardHistory");
            crate::transition::apply_v7_free_move(state, color, origin, candidate)?;
            state.extra.insert("pendingPromotion".into(), Value::Null);
            state.extra.insert("selected".into(), Value::Null);
            let last_move = state.extra.get("lastMove");
            let from = last_move
                .and_then(|last| last.get("from"))
                .filter(|value| js_truth(value))
                .cloned()
                .unwrap_or_else(|| json!(origin));
            let to = last_move
                .and_then(|last| last.get("to"))
                .filter(|value| js_truth(value))
                .cloned()
                .unwrap_or_else(|| json!(destination));
            let trace_hidden = last_move
                .and_then(|last| last.get("hiddenFrom"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if hidden_from.is_empty() && matches!(trace_hidden.as_str(), "white" | "black") {
                hidden_from = trace_hidden.clone();
            }
            traces.push(json!({"from":from,"to":to}));
            if let Some(last_move) = state
                .extra
                .get_mut("lastMove")
                .filter(|value| js_truth(value))
            {
                let last_move = last_move.as_object_mut().ok_or_else(|| {
                    EngineError::InvalidState("v7 free-move lastMove must be an object".into())
                })?;
                last_move.insert("freeMoveMoves".into(), Value::Array(traces.clone()));
                if !hidden_from.is_empty() {
                    last_move.insert("hiddenFrom".into(), json!(hidden_from));
                    state.extra.insert("accelerationTrail".into(), Value::Null);
                }
            }
            state.extra.insert("legalMoves".into(), json!([]));
            state.extra.insert("targeting".into(), Value::Null);
            if let Some(id) = plan_id {
                for piece in state
                    .board
                    .iter_mut()
                    .flatten()
                    .flatten()
                    .filter(|piece| piece.id == id)
                {
                    for field in [
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
                        "repositionSecondMove",
                    ] {
                        piece.extra.shift_remove(field);
                    }
                }
            }
            crate::replay::add_log(
                state,
                if trace_hidden.is_empty() {
                    format!(
                        "프리 무브 실행: {} → {}",
                        source_square_name(state, usize::from(origin.row), usize::from(origin.col)),
                        source_square_name(
                            state,
                            usize::from(destination.row),
                            usize::from(destination.col)
                        ),
                    )
                } else {
                    "프리 무브가 실행되었습니다.".into()
                },
            )?;
            if free_move_history_count(state, "replayEvents") <= replay_count
                && free_move_history_count(state, "boardHistory") <= history_count
            {
                crate::replay::record(
                    state,
                    if state.mode == "gameover" {
                        "gameover"
                    } else {
                        "free move"
                    },
                )?;
            }
        }
    }
    state.free_move_resolution = None;
    state.turn = previous_turn;
    match previous_history_move_number {
        Some(value) => {
            state.extra.insert("activeHistoryMoveNumber".into(), value);
        }
        None => {
            state.extra.shift_remove("activeHistoryMoveNumber");
        }
    }
    for entry in due {
        if let Some(color) = color_field(&entry, "color")
            && matches!(color, "white" | "black")
        {
            state.extra["freeMoveCaptureLock"][color] = Value::Bool(true);
        }
    }
    state.extra.insert("selected".into(), Value::Null);
    state.extra.insert("legalMoves".into(), json!([]));
    state.extra.insert("targeting".into(), Value::Null);
    Ok(())
}

fn free_move_plan_square(value: Option<&Value>) -> Option<Square> {
    let value = value?;
    Some(Square {
        row: u8::try_from(value.get("row")?.as_u64()?)
            .ok()
            .filter(|row| *row < 8)?,
        col: u8::try_from(value.get("col")?.as_u64()?)
            .ok()
            .filter(|col| *col < 8)?,
    })
}

fn free_move_normalized_origin(state: &GameState, square: Square) -> Option<Square> {
    let piece = state.at(square)?;
    if piece.is_large() {
        let anchor = free_move_plan_square(Some(&json!({
            "row":piece.extra.get("anchorRow"),"col":piece.extra.get("anchorCol"),
        })));
        if let Some(anchor) = anchor
            && state
                .at(anchor)
                .is_some_and(|item| item.id == piece.id && item.is_large())
        {
            return Some(anchor);
        }
    }
    // A quantum image in an empty planned square normalizes to a different
    // physical origin and therefore fails isFreeMoveSourceIntact's equality.
    Some(square)
}

fn free_move_history_count(state: &GameState, field: &str) -> usize {
    crate::replay_experiment::history_count(state, field)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;

    fn play_state() -> GameState {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.mode = "play".into();
        state
    }

    #[test]
    fn crown_ground_requires_source_integer_coordinates_without_string_coercion() {
        let mut state = play_state();
        let square = Square { row: 3, col: 3 };
        for (row, col, expected) in [
            (json!(3), json!(3), true),
            (json!(3.0), json!(3.0), true),
            (json!("3"), json!(3), false),
            (json!(3), json!("3"), false),
            (json!(3.5), json!(3), false),
            (Value::Null, json!(3), false),
            (json!(true), json!(3), false),
        ] {
            state
                .extra
                .insert("crownRule".into(), json!({"ground":{"row":row,"col":col}}));
            assert_eq!(
                crown_ground_at(&state, square),
                expected,
                "ground={}",
                state.extra["crownRule"]
            );
        }
        state.extra.insert(
            "crownRule".into(),
            json!({"crowns":[
                {"ground":{"row":"3","col":3}},
                {"ground":{"row":3.0,"col":3.0}}
            ]}),
        );
        assert!(crown_ground_at(&state, square));
        state.extra["crownRule"]["crowns"][1]["removed"] = json!("false");
        assert!(!crown_ground_at(&state, square));
        state.extra.insert("crownRule".into(), json!(true));
        assert!(crown_ground_at(&state, square));
    }

    #[test]
    fn terminal_after_othello_settles_gale_once_and_preserves_later_queues() {
        // Source main93671-93676 invokes Gale after Othello, then checks
        // gameover before Greek Gift and Taboo. Empty-board Gale consumes no RNG.
        let mut state = play_state();
        state.mode = "gameover".into();
        state.winner = Some("black".into());
        state.turn = Color::Black;
        state.board = vec![vec![None; 8]; 8];
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert(
            "pendingGales".into(),
            json!([
                {"id":"due","color":"black","remainingOwnTurns":1},
                {"id":"future","color":"black","remainingOwnTurns":2}
            ]),
        );
        state.extra.insert(
            "greekGiftPending".into(),
            json!({"white":true,"black":false}),
        );
        state.extra.insert(
            "tabooPending".into(),
            json!([{"color":"white","pieceId":"gone","square":{"row":3,"col":3}}]),
        );
        let mut expected = state.clone();
        expected.extra.insert(
            "pendingGales".into(),
            json!([
                {"id":"future","color":"black","remainingOwnTurns":1}
            ]),
        );
        expected
            .extra
            .insert("logs".into(), json!(["강풍: 제거될 기물이 없었습니다."]));
        assert_eq!(
            after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal
        );
        assert_eq!(state, expected);
    }

    #[test]
    fn other_queued_phases_keep_the_terminal_boundary() {
        let mut state = play_state();
        state.mode = "gameover".into();
        state.turns_taken.white = 1;
        // Active queues make an accidental continuation observable rather
        // than testing a terminal board on which all callbacks are no-ops.
        state.extra.insert(
            "pendingScarecrows".into(),
            json!([{"reserved":true,"by":"black","remainingOwnTurns":2,"row":4,"col":4}]),
        );
        state.extra.insert(
            "ultimatum".into(),
            json!({"color":"white","remainingHalfTurns":2}),
        );
        state.extra.insert(
            "pendingFreeMoves".into(),
            json!([{"color":"white","triggerColor":"white","triggerTurn":1,"moves":[]}]),
        );
        let before = state.clone();
        let phases: [fn(&mut GameState, Color) -> Result<V7FlowControl>; 3] =
            [after_prophecy, after_winter, after_capture_reset];
        for phase in phases {
            assert_eq!(
                phase(&mut state, Color::White).unwrap(),
                V7FlowControl::Terminal
            );
            assert_eq!(state, before);
        }
    }

    #[test]
    fn terminal_gale_rejects_an_invalid_queue_without_partial_countdown() {
        let mut state = play_state();
        state.mode = "gameover".into();
        state.extra.insert(
            "pendingGales".into(),
            json!([
                {"id":"first","color":"black","remainingOwnTurns":2},
                {"id":"invalid","color":"black","remainingOwnTurns":"bad"}
            ]),
        );
        let before = state.clone();
        assert!(matches!(after_count(&mut state, Color::White),
            Err(EngineError::InvalidState(message))
                if message == "pendingGales.remainingOwnTurns must be an integer"));
        assert_eq!(state, before);
    }

    #[test]
    fn lobster_boundary_counts_once_and_preserves_future_reservations() {
        let mut state = play_state();
        state.turns_taken.white = 2;
        state.extra.insert("pendingLobsters".into(),json!([
            {"id":"due","color":"white","row":5,"col":0,"remainingHalfTurns":1,"dueMoveCount":99},
            {"id":"later","color":"black","row":2,"col":1,"remainingHalfTurns":2,"dueMoveCount":0}
        ]));
        resolve_pending_lobsters_after_move(&mut state).unwrap();
        let lobster = state.board[5][0].as_ref().unwrap();
        assert_eq!(lobster.kind, "lobster");
        assert_eq!(lobster.color, Color::White);
        assert_eq!(lobster.extra["freshNoCaptureUntil"], json!(3));
        assert_eq!(
            state.extra["pendingLobsters"],
            json!([
                {"id":"later","color":"black","row":2,"col":1,"remainingHalfTurns":1,"dueMoveCount":0}
            ])
        );
        assert!(state.board[2][1].is_none());
    }

    #[test]
    fn blocked_lobster_consumes_reservation_without_an_identity_draw() {
        let mut state = play_state();
        state.extra.insert("logs".into(), json!([]));
        state.set_flag("d4", Color::Black, true);
        state.extra.insert(
            "pendingLobsters".into(),
            json!([
                {"id":"blocked","color":"white","row":3,"col":3,"remainingHalfTurns":0}
            ]),
        );
        let rng = state.rng.clone();
        resolve_pending_lobsters_after_move(&mut state).unwrap();
        assert_eq!(state.rng, rng);
        assert_eq!(state.extra["pendingLobsters"], json!([]));
        assert!(state.board[3][3].is_none());
        assert_eq!(
            state.extra["logs"][0],
            json!("랍스터: d5이 막혀 소환이 취소되었습니다.")
        );
    }

    #[test]
    fn gale_countdown_and_taboo_cancel_follow_source_order_without_rng() {
        let mut state = play_state();
        // Outgoing callbacks retain the supplied movingColor after an internal
        // Spy promotion, even when the live turn already names the other side.
        state.turn = Color::Black;
        state.board = vec![vec![None; 8]; 8];
        state.turns_taken.white = 1;
        state.extra.insert(
            "pendingGales".into(),
            json!([{"id":"g","color":"black","remainingOwnTurns":2}]),
        );
        state.extra.insert(
            "greekGiftPending".into(),
            json!({"white":true,"black":false}),
        );
        state.extra.insert(
            "tabooPending".into(),
            json!([{"color":"white","pieceId":"gone","square":{"row":3,"col":3}}]),
        );
        // SHA-pinned client callbacks leave every other state field unchanged.
        let mut expected = state.clone();
        expected.extra["pendingGales"][0]["remainingOwnTurns"] = json!(1);
        expected.extra["greekGiftPending"]["white"] = Value::Bool(false);
        expected.extra["tabooPending"] = json!([]);
        let rng = state.rng.clone();
        assert_eq!(
            after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.extra["pendingGales"][0]["remainingOwnTurns"], 1);
        assert_eq!(
            state.extra["greekGiftPending"],
            json!({"white":false,"black":false})
        );
        assert_eq!(state.extra["tabooPending"], json!([]));
        assert_eq!(state.rng, rng);
        assert_eq!(state, expected);
    }

    #[test]
    fn source_due_taboo_sacrifices_queen_and_spawns_two_neutral_monsters() {
        // Frozen resolveTaboo with a white queen at e4, an open f4 target,
        // and tape [0.5, 0.5] yields two `neutral-monster-i` identities. The
        // client uses a Set for animation identities, so that Set has one ID.
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.board[4][4] = Some(Piece::new("queen", Color::White, "q"));
        state.rng = crate::RngState {
            algorithm: "lcg32-v1".into(),
            state: 19,
            tape: vec![0.5, 0.5],
            cursor: 0,
            source_chance_trace: None,
        };
        state.extra.insert(
            "tabooPending".into(),
            json!([{"color":"white","pieceId":"q","square":{"row":4,"col":5}}]),
        );
        state.extra.insert("pendingReplayVisuals".into(), json!([]));
        state.extra.shift_remove("forceAnimatedPieceIds");
        let history = state.history.clone();
        let replay = state.extra.get("replayEvents").cloned();
        assert_eq!(
            after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        for (col, origin) in [(4, "e4"), (5, "f4")] {
            let monster = state.board[4][col].as_ref().unwrap();
            assert_eq!(monster.kind, "monster");
            assert_eq!(monster.color, PieceColor::Neutral);
            assert_eq!(monster.id, "neutral-monster-i");
            assert!(monster.moved);
            assert_eq!(monster.extra["shielded"], false);
            assert_eq!(monster.extra["origin"], origin);
        }
        assert_eq!(state.captures.black.len(), 1);
        assert_eq!(state.captures.black[0].id, "q");
        assert_eq!(state.extra["tabooPending"], json!([]));
        assert_eq!(
            state.extra["forceAnimatedPieceIds"]["values"],
            json!(["neutral-monster-i"])
        );
        assert_eq!(
            state.extra["pendingReplayVisuals"],
            json!([{
                "type":"board-change","effect":"cleanup-sacrifice","color":"black",
                "removals":[{"square":{"row":4,"col":4},"color":"white","pieceType":"queen"}],
                "relocations":[],"transformations":[],"spawns":[]
            }])
        );
        assert_eq!(state.rng.cursor, 2);
        assert_eq!(state.rng.state, 8_325_565);
        assert_eq!(state.history, history);
        assert_eq!(state.extra.get("replayEvents"), replay.as_ref());
    }

    #[test]
    fn due_taboo_occupied_target_crushes_piece_before_monster_spawn() {
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.board[4][4] = Some(Piece::new("queen", Color::White, "q"));
        state.board[4][5] = Some(Piece::new("rook", Color::Black, "r"));
        state.extra.insert(
            "tabooPending".into(),
            json!([{"color":"white","pieceId":"q","square":{"row":4,"col":5}}]),
        );
        state.extra.insert("pendingReplayVisuals".into(), json!([]));
        assert_eq!(
            after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(
            state
                .captures
                .black
                .iter()
                .map(|piece| piece.id.as_str())
                .collect::<Vec<_>>(),
            ["q"]
        );
        assert_eq!(
            state
                .captures
                .white
                .iter()
                .map(|piece| piece.id.as_str())
                .collect::<Vec<_>>(),
            ["r"]
        );
        assert_eq!(state.board[4][4].as_ref().unwrap().kind, "monster");
        assert_eq!(state.board[4][5].as_ref().unwrap().kind, "monster");
        assert_eq!(state.extra["tabooPending"], json!([]));
        assert_eq!(
            state.extra["pendingReplayVisuals"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn synthetic_wide_taboo_keeps_geometry_labels_but_rejects_unadopted_transition_atomically() {
        // D-009 adopts the frozen site's 8x8 normal/chaos/grand modes. A 10x10
        // board is a synthetic geometry input, not the site's grand preset.
        let mut state = play_state();
        state.board = vec![vec![None; 10]; 10];
        state.board[4][4] = Some(Piece::new("queen", Color::White, "q"));
        state.extra.insert(
            "tabooPending".into(),
            json!([{"color":"white","pieceId":"q","square":{"row":4,"col":5}}]),
        );
        assert_eq!(source_square_name(&state, 4, 4), "e6");
        assert_eq!(source_square_name(&state, 4, 5), "f6");
        let before = state.clone();
        assert_eq!(
            after_count(&mut state, Color::White).unwrap_err(),
            EngineError::InvalidState("v7 promotion requires the adopted 8x8 board".into())
        );
        assert_eq!(state, before);
    }

    #[test]
    fn source_due_greek_gift_removes_adjacent_pawn_with_capture_memory_and_visual() {
        // Frozen resolveGreekGift/removeExpansionEffectPiece with Black king
        // on e8 and a Black pawn on d7: White gets one capture, medium remembers
        // pawn movement, and the removal visual is queued without an RNG draw.
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.board[0][4] = Some(Piece::new("king", Color::Black, "bk"));
        state.board[1][3] = Some(Piece::new("pawn", Color::Black, "bp"));
        state.extra.insert(
            "greekGiftPending".into(),
            json!({"white":true,"black":false}),
        );
        state.extra.insert("mediumMovement".into(), Value::Null);
        state.extra.insert(
            "capturedTypes".into(),
            json!({"white":{"__simType":"Set","values":[]},
                "black":{"__simType":"Set","values":[]}}),
        );
        state.extra.insert(
            "turnCaptures".into(),
            json!({"white":{"__simType":"Set","values":[]},
                "black":{"__simType":"Set","values":[]}}),
        );
        state.extra.insert("pendingReplayVisuals".into(), json!([]));
        state.extra.shift_remove("animatedPieceIds");
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert_eq!(
            after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert!(state.board[1][3].is_none());
        assert_eq!(state.board[0][4].as_ref().unwrap().id, "bk");
        assert_eq!(
            state.extra["greekGiftPending"],
            json!({"white":false,"black":false})
        );
        assert_eq!(state.captures.white.len(), 1);
        assert_eq!(state.captures.white[0].id, "bp");
        assert_eq!(state.extra["mediumMovement"], json!({"type":"pawn"}));
        assert_eq!(
            state.extra["capturedTypes"]["white"]["values"],
            json!(["pawn"])
        );
        assert_eq!(
            state.extra["turnCaptures"]["white"]["values"],
            json!(["pawn"])
        );
        assert_eq!(
            state.extra["pendingReplayVisuals"],
            json!([{"type":"board-change","effect":"cleanup-sacrifice",
                "color":"white","removals":[{"square":{"row":1,"col":3},
                "color":"black","pieceType":"pawn"}],"relocations":[],
                "transformations":[],"spawns":[]}])
        );
        assert_eq!(state.extra["animatedPieceIds"]["values"], json!(["bp"]));
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn due_gale_runs_campaign_objective_after_ordinary_environmental_removal() {
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.board[4][4] = Some(Piece::new("rook", Color::White, "white-rook"));
        state.turns_taken.white = 1;
        state
            .extra
            .insert("campaign".into(), json!({"setup":"magicParty"}));
        state.extra.insert(
            "pendingGales".into(),
            json!([{"id":"g","color":"black","remainingOwnTurns":1}]),
        );
        let rng = state.rng.clone();
        assert_eq!(
            after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal
        );
        assert!(state.board[4][4].is_none());
        assert_eq!(state.captures.black[0].id, "white-rook");
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner.as_deref(), Some("black"));
        assert_eq!(state.rng.cursor, rng.cursor + 1);
    }

    #[test]
    fn source_due_gale_removes_an_isolated_major_once_with_notation_and_one_rng_draw() {
        // Frozen resolvePendingGalesAfterTurn: a single white rook at e4,
        // Black-owned gale due after White's turn. Source result is one Black
        // capture, one Korean log, and effect notation `!강풍`.
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.board[4][4] = Some(Piece::new("rook", Color::White, "w-r"));
        state.turns_taken.white = 1;
        state.rng = crate::RngState {
            algorithm: "lcg32-v1".into(),
            state: 19,
            tape: vec![0.5],
            cursor: 0,
            source_chance_trace: None,
        };
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert(
            "pendingGales".into(),
            json!([{"id":"g","color":"black","remainingOwnTurns":1}]),
        );
        let history = state.history.clone();
        let replay = state.extra.get("replayEvents").cloned();
        assert_eq!(
            after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert!(state.board[4][4].is_none());
        assert_eq!(state.captures.white.len(), 0);
        assert_eq!(state.captures.black.len(), 1);
        assert_eq!(state.captures.black[0].id, "w-r");
        assert_eq!(state.extra["pendingGales"], json!([]));
        assert_eq!(
            state.extra["logs"][0],
            "강풍: 아군과 인접하지 않은 기물 1개가 날아갔습니다."
        );
        let notation = &state.extra["pendingNotation"];
        assert_eq!(notation["kind"], "effect");
        assert_eq!(notation["text"], "!강풍");
        assert_eq!(notation["description"], "백 강풍으로 기물 1개 제거");
        assert_eq!(state.extra["pendingNotations"], json!([notation]));
        assert_eq!(state.rng.cursor, 1);
        assert_eq!(state.rng.state, 1_045_530_198);
        assert_eq!(state.history, history);
        assert_eq!(state.extra.get("replayEvents"), replay.as_ref());
    }

    #[test]
    fn due_gale_without_isolated_major_piece_settles_zero_victim_branch() {
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.board[4][4] = Some(Piece::new("rook", Color::White, "r1"));
        state.board[4][5] = Some(Piece::new("rook", Color::White, "r2"));
        state.turns_taken.white = 1;
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert(
            "pendingGales".into(),
            json!([{"id":"g","color":"black","remainingOwnTurns":1}]),
        );
        let mut expected = state.clone();
        expected.extra["pendingGales"] = json!([]);
        expected.extra["logs"] = json!(["강풍: 제거될 기물이 없었습니다."]);
        let rng = state.rng.clone();
        assert_eq!(
            after_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.extra["pendingGales"], json!([]));
        assert_eq!(
            state.extra["logs"],
            json!(["강풍: 제거될 기물이 없었습니다."])
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state, expected);
    }

    #[test]
    fn scarecrow_and_ultimatum_future_ticks_do_not_consume_rng() {
        let mut state = play_state();
        state.extra.insert(
            "pendingScarecrows".into(),
            json!([{"row":4,"col":4,"reserved":true,"by":"black","remainingOwnTurns":3}]),
        );
        state.extra.insert(
            "ultimatum".into(),
            json!({"remainingHalfTurns":4,"remaining":2,"movedIds":[]}),
        );
        let mut expected = state.clone();
        expected.extra["pendingScarecrows"][0]["remainingOwnTurns"] = json!(2);
        expected.extra["ultimatum"]["remainingHalfTurns"] = json!(3);
        let rng = state.rng.clone();
        assert_eq!(
            after_prophecy(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.extra["pendingScarecrows"][0]["remainingOwnTurns"], 2);
        assert_eq!(
            after_winter(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.extra["ultimatum"]["remainingHalfTurns"], 3);
        assert_eq!(state.extra["ultimatum"]["remaining"], 2);
        assert_eq!(state.rng, rng);
        assert_eq!(state, expected);
    }

    #[test]
    fn source_due_ultimatum_without_targets_clears_timer_and_logs_zero() {
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert(
            "ultimatum".into(),
            json!({"remainingHalfTurns":1,"remaining":1,"movedIds":[],"by":"white"}),
        );
        let mut expected = state.clone();
        expected.extra["ultimatum"] = Value::Null;
        expected.extra["logs"] = json!(["최후 통첩: 0개의 기물이 제거되었습니다."]);
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert_eq!(
            after_winter(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.extra["ultimatum"], Value::Null);
        assert_eq!(
            state.extra["logs"][0],
            "최후 통첩: 0개의 기물이 제거되었습니다."
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
        assert_eq!(state, expected);
    }

    #[test]
    fn source_due_ultimatum_removes_unmoved_identities_and_cancels_prophecies() {
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.board[0][4] = Some(Piece::new("king", Color::Black, "bk"));
        state.board[7][4] = Some(Piece::new("king", Color::White, "wk"));
        state.board[3][3] = Some(Piece::new("knight", Color::Black, "bn"));
        state.board[4][4] = Some(Piece::new("pawn", Color::White, "wp"));
        state.set_flag("resolve", Color::White, true);
        state.turns_taken.white = 1;
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert(
            "capturedTypes".into(),
            json!({"white":{"__simType":"Set","values":[]},
                "black":{"__simType":"Set","values":[]}}),
        );
        state.extra.insert(
            "prophecy".into(),
            json!({"white":{"target":"knight"},"black":null}),
        );
        state.extra.insert(
            "ultimatum".into(),
            json!({"remainingHalfTurns":1,"remaining":1,"movedIds":[],"by":"white"}),
        );
        let mut expected = state.clone();
        expected.board[3][3] = None;
        expected.board[4][4] = None;
        expected
            .captures
            .white
            .push(Piece::new("knight", Color::Black, "bn"));
        expected
            .captures
            .black
            .push(Piece::new("pawn", Color::White, "wp"));
        expected.extra["capturedTypes"]["white"]["values"] = json!(["knight"]);
        expected.extra["capturedTypes"]["black"]["values"] = json!(["pawn"]);
        expected.extra["prophecy"]["white"] = Value::Null;
        expected
            .extra
            .insert("resolveReady".into(), json!({"white":true,"black":false}));
        expected.extra["ultimatum"] = Value::Null;
        expected.extra["logs"] = json!(["최후 통첩: 2개의 기물이 제거되었습니다."]);
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert_eq!(
            after_winter(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert!(state.board[3][3].is_none());
        assert!(state.board[4][4].is_none());
        assert_eq!(state.board[0][4].as_ref().unwrap().id, "bk");
        assert_eq!(state.board[7][4].as_ref().unwrap().id, "wk");
        assert_eq!(state.captures.white[0].id, "bn");
        assert_eq!(state.captures.black[0].id, "wp");
        assert_eq!(
            state.extra["capturedTypes"]["white"]["values"],
            json!(["knight"])
        );
        assert_eq!(
            state.extra["capturedTypes"]["black"]["values"],
            json!(["pawn"])
        );
        assert_eq!(state.extra["prophecy"]["white"], Value::Null);
        assert_eq!(state.extra["resolveReady"]["white"], true);
        assert_eq!(state.extra["ultimatum"], Value::Null);
        assert_eq!(
            state.extra["logs"],
            json!(["최후 통첩: 2개의 기물이 제거되었습니다."])
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
        assert_eq!(state, expected);
    }

    #[test]
    fn due_reserved_scarecrow_replaces_identity_at_current_square() {
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        let mut pawn = Piece::new("pawn", Color::White, "p");
        pawn.moved = true;
        pawn.extra.insert("foo".into(), json!(3));
        state.board[4][4] = Some(pawn);
        state.extra.shift_remove("forceAnimatedPieceIds");
        state.extra.insert(
            "pendingScarecrows".into(),
            json!([{"row":4,"col":4,"pieceId":"p","reserved":false,"by":"black","remainingOwnTurns":1}]),
        );
        let mut expected = state.clone();
        let mut transformed = Piece::new("scarecrow", Color::White, "p");
        transformed.moved = true;
        transformed.extra.insert("origin".into(), json!("e4"));
        expected.board[4][4] = Some(transformed);
        expected.extra["pendingScarecrows"] = json!([]);
        expected.extra.insert(
            "forceAnimatedPieceIds".into(),
            json!({"__simType":"Set","values":["p"]}),
        );
        assert_eq!(
            after_prophecy(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state, expected);
    }

    #[test]
    fn source_due_square_scarecrow_installs_with_one_id_draw_and_log() {
        // Frozen tickPendingScarecrowsAfterTurn with one due reservation at
        // e4 and Math.random=0.5 creates white-scarecrow-i in that cell.
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.rng = crate::RngState {
            algorithm: "lcg32-v1".into(),
            state: 19,
            tape: vec![0.5],
            cursor: 0,
            source_chance_trace: None,
        };
        state.extra.shift_remove("forceAnimatedPieceIds");
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert(
            "pendingScarecrows".into(),
            json!([{"row":4,"col":4,"color":"white","by":"black",
                "reserved":true,"remainingOwnTurns":1}]),
        );
        let history = state.history.clone();
        let replay = state.extra.get("replayEvents").cloned();
        assert_eq!(
            after_prophecy(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        let installed = state.board[4][4].as_ref().unwrap();
        assert_eq!(installed.kind, "scarecrow");
        assert_eq!(installed.color, Color::White);
        assert_eq!(installed.id, "white-scarecrow-i");
        assert!(installed.moved);
        assert_eq!(installed.extra["shielded"], false);
        assert_eq!(installed.extra["origin"], "e4");
        assert_eq!(state.extra["pendingScarecrows"], json!([]));
        assert_eq!(
            state.extra["forceAnimatedPieceIds"]["values"],
            json!(["white-scarecrow-i"])
        );
        assert_eq!(state.extra["logs"][0], "허수아비: e4에 설치되었습니다.");
        assert_eq!(state.rng.cursor, 1);
        assert_eq!(state.rng.state, 1_045_530_198);
        assert_eq!(state.history, history);
        assert_eq!(state.extra.get("replayEvents"), replay.as_ref());
    }

    #[test]
    fn due_scarecrow_reservation_skips_friendly_occupied_cell_without_rng() {
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.board[4][4] = Some(Piece::new("rook", Color::White, "r"));
        state.extra.insert(
            "pendingScarecrows".into(),
            json!([{"row":4,"col":4,"color":"white","by":"black",
                "reserved":true,"remainingOwnTurns":1}]),
        );
        let mut expected = state.clone();
        expected.extra["pendingScarecrows"] = json!([]);
        assert_eq!(
            after_prophecy(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state, expected);
    }

    #[test]
    fn due_empty_free_move_plan_clears_ui_state_and_locks_its_owner() {
        let mut state = play_state();
        state.turns_taken.white = 1;
        state.extra.insert(
            "pendingFreeMoves".into(),
            json!([{"color":"black","triggerColor":"white","triggerTurn":1,"moves":[]}]),
        );
        state.extra.insert(
            "freeMoveCaptureLock".into(),
            json!({"white":false,"black":false}),
        );
        let mut expected = state.clone();
        expected.extra["pendingFreeMoves"] = json!([]);
        expected.extra["freeMoveCaptureLock"] = json!({"white":false,"black":true});
        expected.extra.insert("selected".into(), Value::Null);
        expected.extra.insert("legalMoves".into(), json!([]));
        expected.extra.insert("targeting".into(), Value::Null);
        let rng = state.rng.clone();
        assert_eq!(
            after_capture_reset(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.extra["pendingFreeMoves"], json!([]));
        assert_eq!(
            state.extra["freeMoveCaptureLock"],
            json!({"white":false,"black":true})
        );
        assert_eq!(state.extra["selected"], Value::Null);
        assert_eq!(state.extra["legalMoves"], json!([]));
        assert_eq!(state.extra["targeting"], Value::Null);
        assert_eq!(state.rng, rng);
        assert_eq!(state, expected);
    }

    #[test]
    fn source_due_free_move_with_missing_origin_cancels_without_replay_or_rng() {
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.turns_taken.white = 1;
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert(
            "pendingFreeMoves".into(),
            json!([{"color":"black","triggerColor":"white","triggerTurn":1,
                "moves":[{"pieceId":"gone","from":{"row":4,"col":4},"to":{"row":3,"col":4}}]}]),
        );
        state.extra.insert(
            "freeMoveCaptureLock".into(),
            json!({"white":false,"black":false}),
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        let replay = state.extra.get("replayEvents").cloned();
        assert_eq!(
            after_capture_reset(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.extra["pendingFreeMoves"], json!([]));
        assert_eq!(
            state.extra["freeMoveCaptureLock"],
            json!({"white":false,"black":true})
        );
        assert_eq!(state.extra["logs"][0], "프리 무브가 취소되었습니다.");
        assert_eq!(state.extra["selected"], Value::Null);
        assert_eq!(state.extra["legalMoves"], json!([]));
        assert_eq!(state.extra["targeting"], Value::Null);
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
        assert_eq!(state.extra.get("replayEvents"), replay.as_ref());
    }

    #[test]
    fn due_free_move_cancels_when_identity_moved_away_from_planned_origin() {
        // `isFreeMoveSourceIntact` compares the piece at `plan.from`, even if
        // the identity survives elsewhere. A stale plan consumes no replay or
        // RNG and emits one cancellation log.
        let mut state = play_state();
        state.board = vec![vec![None; 8]; 8];
        state.board[4][4] = Some(Piece::new("rook", Color::Black, "black-rook"));
        state.turns_taken.white = 1;
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert(
            "pendingFreeMoves".into(),
            json!([{"color":"black","triggerColor":"white","triggerTurn":1,
                "moves":[{"pieceId":"black-rook","from":{"row":3,"col":4},
                    "to":{"row":2,"col":4}}]}]),
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        let replay = state.extra.get("replayEvents").cloned();
        assert_eq!(
            after_capture_reset(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state.board[4][4].as_ref().unwrap().id, "black-rook");
        assert_eq!(state.extra["pendingFreeMoves"], json!([]));
        assert_eq!(state.extra["freeMoveCaptureLock"]["black"], true);
        assert_eq!(state.extra["logs"], json!(["프리 무브가 취소되었습니다."]));
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
        assert_eq!(state.extra.get("replayEvents"), replay.as_ref());
    }

    #[test]
    fn due_free_move_executes_without_advancing_either_completed_turn() {
        let mut state = play_state();
        state.turns_taken.white = 1;
        let id = state.board[6][0].as_ref().unwrap().id.clone();
        state.extra.insert(
            "pendingFreeMoves".into(),
            json!([{"color":"white","triggerColor":"white","triggerTurn":1,
                "moves":[{"pieceId":id,"from":{"row":6,"col":0},"to":{"row":5,"col":0}}]}]),
        );
        state
            .extra
            .insert("activeHistoryMoveNumber".into(), json!(7));
        let turns = state.turns_taken.clone();
        let moves = state.move_count;
        let full_move = state.full_move;
        let remaining = state.actions_remaining;
        assert_eq!(
            after_capture_reset(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert!(state.board[6][0].is_none());
        assert_eq!(state.board[5][0].as_ref().unwrap().id, id);
        assert!(state.board[5][0].as_ref().unwrap().moved);
        assert_eq!(state.turns_taken, turns);
        assert_eq!(state.move_count, moves);
        assert_eq!(state.full_move, full_move);
        assert_eq!(state.actions_remaining, remaining);
        assert_eq!(state.turn, Color::White);
        assert!(state.free_move_resolution.is_none());
        assert!(state.extra["pendingPromotion"].is_null());
        assert_eq!(state.extra["activeHistoryMoveNumber"], json!(7));
        assert_eq!(state.extra["pendingFreeMoves"], json!([]));
        assert_eq!(state.extra["freeMoveCaptureLock"]["white"], true);
        assert_eq!(
            state.extra["lastMove"]["freeMoveMoves"],
            json!([{"from":{"row":6,"col":0},"to":{"row":5,"col":0}}])
        );
    }

    #[test]
    #[ignore = "requires a pinned source receipt outside Git"]
    fn external_queued_effect_source_receipt_matches_full_position() {
        let path = std::env::var("ACCELERATE_V7_QUEUED_FIXTURE")
            .expect("set ACCELERATE_V7_QUEUED_FIXTURE to the pinned receipt path");
        let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let actor = match fixture.get("actor").and_then(Value::as_str) {
            Some("black") => Color::Black,
            Some("white") | None => Color::White,
            Some(other) => panic!("unrecognized receipt actor {other}"),
        };
        let before = crate::v7_host::V7HostPosition::from_envelope(fixture["before"].clone())
            .expect("frozen source before Position imports losslessly");
        let (after, control) = before
            .transact(before.position_id(), |state| {
                let control = match fixture["callback"].as_str() {
                    Some("resolveTaboo" | "resolveGreekGift" | "resolvePendingGalesAfterTurn") => {
                        after_count(state, actor)
                    }
                    Some("tickPendingScarecrowsAfterTurn") => after_prophecy(state, actor),
                    Some("tickUltimatumCountdown") => after_winter(state, actor),
                    Some("resolveUltimatum") => run_owned(state, actor, |next, _| {
                        settle_due_ultimatum(next)?;
                        Ok(if next.mode == "gameover" {
                            V7FlowControl::Terminal
                        } else {
                            V7FlowControl::Continue
                        })
                    }),
                    Some("resolvePendingFreeMovesAfterTurn") => after_capture_reset(state, actor),
                    Some("resolvePendingLobstersAfterMove") => {
                        run_owned(state, actor, |next, _| {
                            resolve_pending_lobsters_after_move(next)?;
                            Ok(if next.mode == "gameover" {
                                V7FlowControl::Terminal
                            } else {
                                V7FlowControl::Continue
                            })
                        })
                    }
                    other => Err(EngineError::InvalidState(format!(
                        "unsupported queued-effect receipt callback {other:?}"
                    ))),
                }?;
                crate::replay::settle(state)?;
                Ok(control)
            })
            .expect("native queued-effect due settlement");
        assert_eq!(
            control,
            if fixture["after"]["state"]["mode"] == "gameover" {
                V7FlowControl::Terminal
            } else {
                V7FlowControl::Continue
            }
        );
        let actual = after.export_envelope().unwrap();
        let mut mismatches = Vec::new();
        crate::tests::source_callback_fixture::compare_value(
            &fixture["after"],
            &actual,
            "queued.after",
            &mut mismatches,
        )
        .unwrap();
        assert!(
            mismatches.is_empty(),
            "queued callback {} differs:\n{}",
            fixture["callback"],
            mismatches.join("\n")
        );
    }
}
