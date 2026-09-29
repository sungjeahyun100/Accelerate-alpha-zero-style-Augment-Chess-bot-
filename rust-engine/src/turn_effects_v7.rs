//! Source-pinned v7 effects that settle at a completed turn boundary.
//!
//! The frozen client calls `resolveOthelloAll` when the Othello card is played
//! and once more after `turnsTaken[movingColor]` increases and the source's
//! empty-lunchbox, platform, crown and mistake-card callbacks run, if
//! `othelloPending[movingColor]` is set. The client clears that latch before
//! the second scan. Neither scan consumes RNG or creates a capture record.

use crate::{
    Color, EngineError, GameState, Piece, RULES_VERSION_V6, RULES_VERSION_V7, Result, Square,
};
use serde_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};

const KING_DIRECTIONS: [(isize, isize); 8] = [
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -1),
    (0, 1),
    (1, -1),
    (1, 0),
    (1, 1),
];

fn v7_only(state: &GameState) -> Result<()> {
    match state.ruleset_id.as_str() {
        RULES_VERSION_V7 => Ok(()),
        other => Err(EngineError::UnsupportedFeature(format!(
            "v7 Othello on rules version {other}"
        ))),
    }
}

fn cell(state: &GameState, row: usize, col: usize) -> Option<&Piece> {
    state.board.get(row)?.get(col)?.as_ref()
}

fn ally_at_offset(
    state: &GameState,
    row: usize,
    col: usize,
    dr: isize,
    dc: isize,
    actor: Color,
) -> bool {
    row.checked_add_signed(dr)
        .zip(col.checked_add_signed(dc))
        .and_then(|(r, c)| cell(state, r, c))
        .is_some_and(|piece| piece.color == actor)
}

/// The source asks whether one enemy piece is flanked by friendly pieces on
/// opposite king-neighbor squares. A large piece is never an Othello target.
fn is_othello_target(state: &GameState, actor: Color, row: usize, col: usize) -> bool {
    let Some(piece) = cell(state, row, col) else {
        return false;
    };
    if piece.color != actor.opponent()
        || matches!(piece.kind.as_str(), "wall" | "football" | "blackHole")
        || piece.is_large()
        || (piece.kind == "shotgunKing"
            && state
                .extra
                .get("campaign")
                .and_then(|value| value.get("setup"))
                .and_then(Value::as_str)
                == Some("shotgunKing"))
    {
        return false;
    }
    KING_DIRECTIONS.iter().any(|&(dr, dc)| {
        ally_at_offset(state, row, col, dr, dc, actor)
            && ally_at_offset(state, row, col, -dr, -dc, actor)
    })
}

/// Collect every target before converting any piece. This preserves the
/// client's row-major identity de-duplication and prevents an earlier flip
/// from making a new target eligible in the same pass.
fn targets(state: &GameState, actor: Color) -> Result<Vec<(Square, String)>> {
    let mut seen = BTreeSet::new();
    let mut selected = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, cell) in cells.iter().enumerate() {
            let Some(piece) = cell.as_ref() else { continue };
            if seen.contains(&piece.id) || !is_othello_target(state, actor, row, col) {
                continue;
            }
            let row = u8::try_from(row).map_err(|_| {
                EngineError::UnsupportedFeature("Othello row exceeds wire square".into())
            })?;
            let col = u8::try_from(col).map_err(|_| {
                EngineError::UnsupportedFeature("Othello column exceeds wire square".into())
            })?;
            seen.insert(piece.id.clone());
            selected.push((Square { row, col }, piece.id.clone()));
        }
    }
    Ok(selected)
}

fn resolve_all_on_owned_state(state: &mut GameState, actor: Color) -> Result<usize> {
    let selected = targets(state, actor)?;
    for (square, id) in &selected {
        let previous = state
            .at(*square)
            .cloned()
            .filter(|piece| piece.id == *id)
            .ok_or_else(|| {
                EngineError::InvalidState("Othello target changed during resolution".into())
            })?;
        let mut converted = previous.clone();
        converted.color = actor.into();
        converted.moved = true;
        crate::card_effects::mark_transformed_origin_with_options(
            state,
            &mut converted,
            *square,
            true,
        )?;
        converted.extra.shift_remove("freshNoCaptureUntil");
        converted
            .extra
            .insert("coolGuyCapturedLast".into(), json!(false));
        state.board[square.row as usize][square.col as usize] = Some(converted.clone());
        crate::card_effects::mark_animation(state, &converted)?;
        // The source invokes the royal-loss callback with the pre-flip piece,
        // even though the board now contains its converted identity.
        crate::transition::resolve_royal_capture(state, &previous, actor)?;
    }
    Ok(selected.len())
}

fn pending_object(state: &mut GameState) -> Result<&mut serde_json::Map<String, Value>> {
    if state.extra.get("othelloPending").is_none_or(Value::is_null) {
        state.extra.insert(
            "othelloPending".into(),
            json!({"white":false,"black":false}),
        );
    }
    state
        .extra
        .get_mut("othelloPending")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState("othelloPending must be a color map".into()))
}

/// Apply the v7 Othello card's immediate branch. The card handler owns its
/// selection and cost policy; this function owns the latch and source flip.
pub(crate) fn activate_othello(state: &mut GameState, actor: Color) -> Result<usize> {
    v7_only(state)?;
    if state.mode == "gameover" {
        return Err(EngineError::Terminal);
    }
    if state.mode != "play" || state.turn != actor {
        return Err(EngineError::WrongActor);
    }
    let mut next = state.clone();
    pending_object(&mut next)?.insert(actor.as_str().into(), json!(true));
    let converted = resolve_all_on_owned_state(&mut next, actor)?;
    *state = next;
    Ok(converted)
}

/// The source order is `turnsTaken[actor]++`, empty-lunchbox cleanup, platform
/// tick, crown adjudication, `mistakeCard[actor]=false`, Othello recheck, then
/// pending gales. A missing or false latch is a no-op. Until the preceding
/// effects are ported, active cases that could change this scan fail closed.
/// v6 uses its existing transition path unchanged.
pub(crate) fn settle_after_completed_turn(state: &mut GameState, actor: Color) -> Result<usize> {
    if state.ruleset_id == RULES_VERSION_V6 || state.mode == "gameover" {
        return Ok(0);
    }
    v7_only(state)?;
    let pending = match state.extra.get("othelloPending") {
        None | Some(Value::Null) => false,
        Some(Value::Object(sides)) => match sides.get(actor.as_str()) {
            None | Some(Value::Null | Value::Bool(false)) => false,
            Some(Value::Bool(true)) => true,
            _ => {
                return Err(EngineError::InvalidState(
                    "othelloPending owner latch must be boolean".into(),
                ));
            }
        },
        _ => {
            return Err(EngineError::InvalidState(
                "othelloPending must be a color map".into(),
            ));
        }
    };
    if !pending {
        return Ok(0);
    }
    ensure_preceding_source_effects_inert(state, actor)?;
    let mut next = state.clone();
    pending_object(&mut next)?.insert(actor.as_str().into(), json!(false));
    let converted = resolve_all_on_owned_state(&mut next, actor)?;
    *state = next;
    Ok(converted)
}

fn ensure_preceding_source_effects_inert(state: &GameState, actor: Color) -> Result<()> {
    // completeTurnAfterMove runs the conveyor before incrementing turnsTaken.
    // A factory campaign can have moving belts without conveyorRule enabled.
    if actor == Color::Black
        && (crate::observation::truth(state.extra.get("conveyorRule"))
            || state
                .extra
                .get("campaign")
                .and_then(|campaign| campaign.get("setup"))
                .and_then(Value::as_str)
                == Some("conveyorFactory"))
    {
        return Err(EngineError::UnsupportedFeature(
            "v7 Othello after conveyor turn movement".into(),
        ));
    }
    if state.board.iter().flatten().flatten().any(|piece| {
        piece.color == actor && crate::observation::truth(piece.extra.get("emptyLunchbox"))
    }) {
        return Err(EngineError::UnsupportedFeature(
            "v7 Othello after empty-lunchbox cleanup".into(),
        ));
    }
    if crate::observation::truth(
        state
            .extra
            .get("platformRule")
            .and_then(|rule| rule.get("enabled")),
    ) {
        return Err(EngineError::UnsupportedFeature(
            "v7 Othello after platform rule tick".into(),
        ));
    }
    if crate::observation::truth(state.extra.get("crownRule")) {
        return Err(EngineError::UnsupportedFeature(
            "v7 Othello after crown adjudication".into(),
        ));
    }
    // The source resolves pending gales immediately *after* Othello, even if
    // the conversion ended the game. The current transition path does not
    // settle that queue, so returning a seemingly complete position here
    // would also violate the next source callback boundary.
    if state
        .extra
        .get("pendingGales")
        .and_then(Value::as_array)
        .is_some_and(|entries| !entries.is_empty())
    {
        return Err(EngineError::UnsupportedFeature(
            "v7 Othello with pending gale settlement".into(),
        ));
    }
    // resolveEmptyLunchboxesAfterTurn calls resolveDemocracyDefeats even when
    // no piece bears an emptyLunchbox. That callback can end the game before
    // Othello if an active democracy has lost its final pawn.
    for color in [Color::White, Color::Black] {
        if !state.flag("democracy", color) {
            continue;
        }
        let board_pawn = state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.color == color && piece.kind == "pawn");
        let pending_pawn = state
            .extra
            .get("pendingRecurrences")
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry["piece"]["color"] == color.as_str() && entry["piece"]["type"] == "pawn"
                })
            });
        if !board_pawn && !pending_pawn {
            return Err(EngineError::UnsupportedFeature(
                "v7 Othello after democracy defeat check".into(),
            ));
        }
    }
    Ok(())
}

// The v7 client uses this order both in fastDonQuixoteRampage and in the
// shortest-path planner (frozen main-OahWs0tU.js:644,4329,4423).
const DON_KNIGHT_OFFSETS: [(isize, isize); 8] = [
    (-2, -1),
    (-2, 1),
    (-1, -2),
    (-1, 2),
    (1, -2),
    (1, 2),
    (2, -1),
    (2, 1),
];

fn don_unsupported(reason: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!("v7 Don Quixote turn entry: {reason}"))
}

/// Reproduce the source planner's first shortest route to one windmill while
/// excluding defeat-royal squares. The caller admits only the one source
/// probed board shape; other routes remain unsupported even if BFS finds one.
fn don_safe_route(state: &GameState, from: Square, target: Square) -> Result<Vec<Square>> {
    let rows = state.board.len();
    let cols = state.board.first().map_or(0, Vec::len);
    if rows == 0
        || cols == 0
        || rows.checked_mul(cols).is_none_or(|cells| cells > 256)
        || state.board.iter().any(|row| row.len() != cols)
    {
        return Err(don_unsupported("route board dimensions"));
    }
    let index = |square: Square| usize::from(square.row) * cols + usize::from(square.col);
    let mut parent = vec![None; rows * cols];
    let mut queue = VecDeque::from([from]);
    parent[index(from)] = Some(from);
    while let Some(current) = queue.pop_front() {
        if current == target {
            break;
        }
        for (dr, dc) in DON_KNIGHT_OFFSETS {
            let Some((row, col)) = usize::from(current.row)
                .checked_add_signed(dr)
                .zip(usize::from(current.col).checked_add_signed(dc))
            else {
                continue;
            };
            if row >= rows || col >= cols || row > u8::MAX as usize || col > u8::MAX as usize {
                continue;
            }
            let next = Square {
                row: row as u8,
                col: col as u8,
            };
            if state.at(next).is_some_and(Piece::is_defeat_royal) || parent[index(next)].is_some() {
                continue;
            }
            parent[index(next)] = Some(current);
            queue.push_back(next);
        }
    }
    if parent[index(target)].is_none() {
        return Err(don_unsupported(
            "royal-safe route unavailable; source fallback unverified",
        ));
    }
    let mut route = Vec::new();
    let mut cursor = target;
    while cursor != from {
        route.push(cursor);
        cursor = parent[index(cursor)].ok_or_else(|| don_unsupported("route parent missing"))?;
    }
    route.reverse();
    Ok(route)
}

fn require_inert_don_probe_context(state: &GameState) -> Result<(Piece, Piece)> {
    // This is the source-accepted seed-37 synthetic transition, with the pawn
    // already on h6 when the new white turn begins. Exact geometry keeps the
    // unported multi-target planner and force-capture callbacks fail-closed.
    if state.board.len() != 8
        || state.board.iter().any(|row| row.len() != 8)
        || state.turn != Color::White
        || state.mode != "play"
        || state.move_count != 1
        || state.full_move != 2
        || state.turns_taken.black != 1
        || state.turns_taken.white != 0
        || state.actions_remaining != 1
        || state.en_passant.is_some()
        || !state.captures.white.is_empty()
        || !state.captures.black.is_empty()
        || state.rng.cursor != 222
        || state.rng.state != 3_596_032_795
        || state.rng.algorithm != "lcg32-v1"
        || !state.rng.tape.is_empty()
    {
        return Err(don_unsupported("outside source-verified turn and geometry"));
    }
    let expected = [
        (0, 0, "windmill", "neutral"),
        (2, 3, "king", "black"),
        (2, 7, "pawn", "black"),
        (4, 4, "donQuixote", "white"),
        (7, 4, "king", "white"),
    ];
    let mut found = 0;
    for (row, cells) in state.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            let Some(piece) = piece else { continue };
            if !expected.iter().any(|&(r, c, kind, color)| {
                row == r && col == c && piece.kind == kind && piece.color.as_str() == color
            }) {
                return Err(don_unsupported("unverified board occupant"));
            }
            found += 1;
        }
    }
    if found != expected.len()
        || state
            .at(Square { row: 4, col: 4 })
            .is_none_or(|piece| piece.moved)
        || state
            .at(Square { row: 2, col: 7 })
            .is_none_or(|piece| !piece.moved)
    {
        return Err(don_unsupported("source board pieces are incomplete"));
    }
    let don = state.at(Square { row: 4, col: 4 }).expect("checked board");
    let windmill = state.at(Square { row: 0, col: 0 }).expect("checked board");
    let kings_and_pawn_match_source = [
        (Square { row: 2, col: 3 }, false),
        (Square { row: 2, col: 7 }, true),
        (Square { row: 7, col: 4 }, false),
    ]
    .into_iter()
    .all(|(square, moved)| {
        let piece = state.at(square).expect("checked board");
        piece.moved == moved
            && !piece.flag("shielded")
            && (square != (Square { row: 2, col: 7 }) || !piece.flag("coolGuyCapturedLast"))
            && piece.extra.keys().all(|key| {
                key == "shielded"
                    || square == (Square { row: 2, col: 7 }) && key == "coolGuyCapturedLast"
            })
    });
    if don.id.is_empty()
        || windmill.id.is_empty()
        || !kings_and_pawn_match_source
        || don.flag("shielded")
        || windmill.flag("shielded")
        || don.extra.keys().any(|key| key != "shielded")
        || windmill.extra.keys().any(|key| key != "shielded")
        || !state.extra.get("deathmatch").is_none_or(Value::is_null)
        || !state.extra.get("campaign").is_none_or(Value::is_null)
        || !state.extra.get("mediumMovement").is_none_or(Value::is_null)
        || state.extra.get("parrotMovement").is_none_or(|memory| {
            memory.get("black") != Some(&json!({"type":"pawn"}))
                || !memory.get("white").is_none_or(Value::is_null)
        })
        || !state.extra.get("prophecy").is_none_or(|prophecy| {
            ["white", "black"]
                .into_iter()
                .all(|side| prophecy.get(side).is_none_or(Value::is_null))
        })
        || crate::observation::truth(state.extra.get("camouflageRule"))
        || state.extra.get("initiative").is_some_and(|initiative| {
            ["white", "black"]
                .into_iter()
                .any(|side| crate::observation::truth(initiative.get(side)))
        })
        || [
            "pendingPortals",
            "pendingIcbm",
            "pendingGales",
            "pendingPanic",
            "pendingRuleTickets",
            "pendingTrolley",
            "pendingTrojanHorse",
            "pendingFreeMoves",
            "pendingPawnStorm",
            "pendingScarecrows",
            "pendingOtherworld",
            "pendingBearRetaliations",
            "pendingLobsters",
            "pendingWhiteBoxes",
        ]
        .into_iter()
        .any(|field| {
            state.extra.get(field).is_some_and(|value| {
                !value.is_null() && value.as_array().is_none_or(|entries| !entries.is_empty())
            })
        })
        || [
            "collapsePending",
            "accelerationTrail",
            "activeTrolley",
            "ruleTicketChoice",
            "pendingPromotion",
            "periodicCollapse",
            "platformRule",
            "crownRule",
        ]
        .into_iter()
        .any(|field| state.extra.get(field).is_some_and(|value| !value.is_null()))
        || ["vanishing", "coronation", "quantumPending"]
            .into_iter()
            .any(|field| {
                ["white", "black"].into_iter().any(|side| {
                    crate::observation::truth(
                        state.extra.get(field).and_then(|value| value.get(side)),
                    )
                })
            })
        || state
            .deck_slots
            .white
            .iter()
            .chain(state.deck_slots.black.iter())
            .any(|card| {
                !card.vacant && crate::observation::truth(card.extra.get("nextTurnPending"))
            })
    {
        return Err(don_unsupported("unverified capture or concealment effect"));
    }
    Ok((don.clone(), windmill.clone()))
}

fn insert_capture_set(state: &mut GameState, field: &str, actor: Color, kind: &str) -> Result<()> {
    let values = state
        .extra
        .get_mut(field)
        .and_then(|map| map.get_mut(actor.as_str()))
        .and_then(|set| set.get_mut("values"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState(format!("{field} source Set missing")))?;
    let kind = json!(kind);
    if !values.contains(&kind) {
        values.push(kind);
    }
    Ok(())
}

/// Source-validated v7 turn-entry Don Quixote rampage for the single
/// seed-37 royal-avoidance probe. The source runs this after Brutus and before
/// pending acceleration on the incoming turn (main:93761,115279-115335).
/// The wider one/multi-windmill and no-windmill branches are intentionally
/// unsupported until their complete state, replay, and RNG effects agree.
pub(crate) fn resolve_don_quixote_turn_entry(
    state: &mut GameState,
    incoming: Color,
) -> Result<usize> {
    if state.ruleset_id == RULES_VERSION_V6 || state.mode == "gameover" {
        return Ok(0);
    }
    v7_only(state)?;
    if state.turn != incoming {
        return Err(EngineError::WrongActor);
    }
    let don_count = state
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|piece| {
            piece.color == incoming && matches!(piece.kind.as_str(), "donQuixote" | "don-quixote")
        })
        .count();
    if don_count == 0 {
        return Ok(0);
    }
    if don_count != 1 {
        return Err(don_unsupported("multiple active pieces"));
    }
    let (don, windmill) = require_inert_don_probe_context(state)?;
    let from = Square { row: 4, col: 4 };
    let target = Square { row: 0, col: 0 };
    let route = don_safe_route(state, from, target)?;
    if route
        != [
            Square { row: 2, col: 5 },
            Square { row: 0, col: 4 },
            Square { row: 1, col: 2 },
            target,
        ]
    {
        return Err(don_unsupported("unverified royal-safe route"));
    }
    if route[..route.len() - 1]
        .iter()
        .any(|&square| state.at(square).is_some())
    {
        return Err(don_unsupported("occupied route step"));
    }
    let mut next = state.clone();
    let mut transitions = Vec::with_capacity(route.len());
    let mut cursor = from;
    for &landing in &route {
        let capture = landing == target;
        crate::replay::queue_don_quixote_rampage(&mut next, &don, cursor, landing, capture)?;
        if capture {
            let memory =
                crate::card_effects::current_base_movement(&next, &windmill).unwrap_or(Value::Null);
            next.extra.insert("mediumMovement".into(), memory);
            insert_capture_set(&mut next, "capturedTypes", incoming, "windmill")?;
            insert_capture_set(&mut next, "turnCaptures", incoming, "windmill")?;
            next.captures.get_mut(incoming).push(windmill.clone());
            next.board[usize::from(landing.row)][usize::from(landing.col)] = None;
        }
        let mut moving = next.board[usize::from(cursor.row)][usize::from(cursor.col)]
            .take()
            .ok_or_else(|| EngineError::InvalidState("Don Quixote route lost its piece".into()))?;
        if moving.id != don.id {
            return Err(EngineError::InvalidState(
                "Don Quixote route identity changed".into(),
            ));
        }
        moving.moved = true;
        if capture {
            moving.extra.insert(
                "totalCaptures".into(),
                json!(moving.number("totalCaptures") + 1),
            );
        }
        next.board[usize::from(landing.row)][usize::from(landing.col)] = Some(moving.clone());
        next.en_passant = None;
        crate::card_effects::remember_local_movement(&mut next, &moving)?;
        transitions.push(json!({
            "pieceId":don.id,"color":incoming,"from":cursor,"to":landing,
            "captured":if capture {json!({"id":windmill.id,"type":"windmill","color":"neutral"})} else {Value::Null},
            "capturedFrom":landing,"hiddenFrom":"","capturedHiddenFrom":"","rampage":true
        }));
        cursor = landing;
    }
    crate::replay::queue_visual(
        &mut next,
        json!({"type":"don-quixote-route","color":incoming,"transitions":transitions}),
    )?;
    *state = next;
    Ok(route.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameConfig, PieceColor, RngState};

    fn bare_state() -> GameState {
        let mut state = GameState::new(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            5,
        )
        .unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.board = vec![vec![None; 8]; 8];
        state
    }

    fn place(state: &mut GameState, color: Color, kind: &str, row: usize, col: usize, id: &str) {
        state.board[row][col] = Some(Piece::new(kind, color, id));
    }

    fn source_don_probe_turn_entry() -> GameState {
        let mut state = bare_state();
        state.turn = Color::White;
        state.mode = "play".into();
        state.move_count = 1;
        state.full_move = 2;
        state.turns_taken.black = 1;
        state.actions_remaining = 1;
        state.rng = RngState {
            algorithm: "lcg32-v1".into(),
            state: 3_596_032_795,
            tape: Vec::new(),
            cursor: 222,
        };
        state.board[0][0] = Some(Piece::new(
            "windmill",
            PieceColor::Neutral,
            "neutral-windmill-ahe4rqzocq6",
        ));
        place(
            &mut state,
            Color::Black,
            "king",
            2,
            3,
            "black-king-pmiwvlwhlr9",
        );
        place(
            &mut state,
            Color::Black,
            "pawn",
            2,
            7,
            "black-pawn-jdnrcu69kmj",
        );
        state.board[2][7].as_mut().unwrap().moved = true;
        place(
            &mut state,
            Color::White,
            "donQuixote",
            4,
            4,
            "white-donQuixote-yli72yexcko",
        );
        place(
            &mut state,
            Color::White,
            "king",
            7,
            4,
            "white-king-wvt63tbibo",
        );
        for field in ["capturedTypes", "turnCaptures"] {
            state.extra.insert(
                field.into(),
                json!({"white":{"__simType":"Set","values":[]},"black":{"__simType":"Set","values":[]}}),
            );
        }
        state.extra.insert("mediumMovement".into(), Value::Null);
        state.extra.insert(
            "parrotMovement".into(),
            json!({"white":null,"black":{"type":"pawn"}}),
        );
        state.extra.insert("deathmatch".into(), Value::Null);
        state.extra.insert("campaign".into(), Value::Null);
        state
            .extra
            .insert("prophecy".into(), json!({"white":null,"black":null}));
        state
            .extra
            .insert("initiative".into(), json!({"white":null,"black":null}));
        state
    }

    #[test]
    fn source_v7_don_probe_avoids_royal_and_preserves_four_step_rng_and_capture() {
        let mut state = source_don_probe_turn_entry();
        let route =
            don_safe_route(&state, Square { row: 4, col: 4 }, Square { row: 0, col: 0 }).unwrap();
        assert_eq!(
            route,
            [
                Square { row: 2, col: 5 },
                Square { row: 0, col: 4 },
                Square { row: 1, col: 2 },
                Square { row: 0, col: 0 },
            ]
        );
        assert_eq!(
            resolve_don_quixote_turn_entry(&mut state, Color::White).unwrap(),
            4
        );
        assert_eq!(state.board[2][3].as_ref().unwrap().kind, "king");
        assert_eq!(state.board[0][0].as_ref().unwrap().kind, "donQuixote");
        assert!(state.board[0][0].as_ref().unwrap().moved);
        assert_eq!(
            state.board[0][0].as_ref().unwrap().number("totalCaptures"),
            1
        );
        assert!(state.board[4][4].is_none());
        assert_eq!(state.captures.white.len(), 1);
        assert_eq!(state.captures.white[0].kind, "windmill");
        assert_eq!(
            state.extra["capturedTypes"]["white"]["values"],
            json!(["windmill"])
        );
        assert_eq!(
            state.extra["turnCaptures"]["white"]["values"],
            json!(["windmill"])
        );
        assert_eq!(state.extra["mediumMovement"], json!({"type":"windmill"}));
        assert_eq!(
            state.extra["parrotMovement"]["white"],
            json!({"type":"don-quixote"})
        );
        assert_eq!(state.rng.cursor, 226);
        assert_eq!(state.rng.state, 1_854_409_343);
        let notations = state.extra["pendingNotations"].as_array().unwrap();
        assert_eq!(notations.len(), 4);
        assert_eq!(
            notations
                .iter()
                .map(|event| event["text"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["DQe4-f6", "DQf6-e8", "DQe8-c7", "DQc7xa8"]
        );
        assert_eq!(
            notations
                .iter()
                .map(|event| event["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "special-1790581292828-h21pcen",
                "special-1790581292828-foaamup",
                "special-1790581292828-5sv39in",
                "special-1790581292828-fjkckyb",
            ]
        );
        assert_eq!(
            state.extra["pendingReplayVisuals"][0]["transitions"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }

    #[test]
    fn unverified_don_context_refuses_without_partial_state_or_rng_change() {
        let mut state = source_don_probe_turn_entry();
        state.board[3][3] = Some(Piece::new("windmill", PieceColor::Neutral, "second"));
        let before = state.clone();
        assert!(matches!(
            resolve_don_quixote_turn_entry(&mut state, Color::White),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn immediate_and_completed_turn_scans_are_distinct_and_latch_is_private() {
        let mut state = bare_state();
        place(&mut state, Color::White, "rook", 4, 2, "left");
        place(&mut state, Color::Black, "pawn", 4, 3, "first");
        place(&mut state, Color::White, "rook", 4, 4, "right");
        assert_eq!(activate_othello(&mut state, Color::White).unwrap(), 1);
        let first = state.board[4][3].as_ref().unwrap();
        assert_eq!(first.color, Color::White);
        assert!(first.moved);
        assert_eq!(first.extra["origin"], "d4");
        assert_eq!(first.extra["coolGuyCapturedLast"], false);
        assert_eq!(state.extra["othelloPending"]["white"], true);
        assert!(
            state.extra["forceAnimatedPieceIds"]["values"]
                .as_array()
                .unwrap()
                .contains(&json!("first"))
        );

        place(&mut state, Color::White, "rook", 2, 2, "later-left");
        place(&mut state, Color::Black, "pawn", 2, 3, "later");
        place(&mut state, Color::White, "rook", 2, 4, "later-right");
        state.turns_taken.white += 1;
        assert_eq!(
            settle_after_completed_turn(&mut state, Color::White).unwrap(),
            1
        );
        assert_eq!(state.board[2][3].as_ref().unwrap().color, Color::White);
        assert_eq!(state.extra["othelloPending"]["white"], false);
        assert_eq!(
            settle_after_completed_turn(&mut state, Color::White).unwrap(),
            0
        );
    }

    #[test]
    fn source_target_filter_rejects_large_pieces_and_shotgun_campaign_king() {
        let mut state = bare_state();
        place(&mut state, Color::White, "rook", 4, 2, "left");
        place(&mut state, Color::Black, "colossus", 4, 3, "large");
        place(&mut state, Color::White, "rook", 4, 4, "right");
        assert!(!is_othello_target(&state, Color::White, 4, 3));
        place(&mut state, Color::Black, "shotgunKing", 4, 3, "shotgun");
        state
            .extra
            .insert("campaign".into(), json!({"setup":"shotgunKing"}));
        assert!(!is_othello_target(&state, Color::White, 4, 3));
    }

    #[test]
    fn unsupported_royal_reaction_cannot_leave_a_partial_flip_or_latch() {
        let mut state = bare_state();
        place(&mut state, Color::White, "rook", 4, 2, "left");
        place(&mut state, Color::Black, "king", 4, 3, "royal");
        place(&mut state, Color::White, "rook", 4, 4, "right");
        state
            .extra
            .insert("regency".into(), json!({"white":false,"black":true}));
        let before = state.clone();
        assert!(matches!(
            activate_othello(&mut state, Color::White),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn pending_preceding_board_effects_fail_before_clearing_the_latch() {
        let mut state = bare_state();
        place(&mut state, Color::White, "rook", 4, 2, "left");
        place(&mut state, Color::Black, "pawn", 4, 3, "victim");
        place(&mut state, Color::White, "rook", 4, 4, "right");
        state
            .extra
            .insert("othelloPending".into(), json!({"white":true,"black":false}));
        for (field, value) in [
            ("platformRule", json!({"enabled":true,"fixed":false})),
            ("crownRule", json!({"holderId":"left"})),
        ] {
            state.extra.insert(field.into(), value);
            let before = state.clone();
            assert!(matches!(
                settle_after_completed_turn(&mut state, Color::White),
                Err(EngineError::UnsupportedFeature(_))
            ));
            assert_eq!(state, before);
            state.extra.shift_remove(field);
        }
        state.board[5][0] = Some(Piece::new("pawn", Color::White, "lunchbox"));
        state.board[5][0]
            .as_mut()
            .unwrap()
            .extra
            .insert("emptyLunchbox".into(), json!({"deadlineTurn":1}));
        let before = state.clone();
        assert!(matches!(
            settle_after_completed_turn(&mut state, Color::White),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert_eq!(state, before);
    }
}
