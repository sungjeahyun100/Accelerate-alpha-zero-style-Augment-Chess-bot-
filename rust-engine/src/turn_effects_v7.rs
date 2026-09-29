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
use std::collections::BTreeSet;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;

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
