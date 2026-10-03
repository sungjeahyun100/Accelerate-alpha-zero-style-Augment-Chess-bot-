//! Frozen v7 `resolveRuleBombsUnderPieces` board callback.
//!
//! Source: `main-OahWs0tU.js`, SHA-256
//! `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`,
//! lines 102253-102326. The source invokes this after a board move, at the
//! start of `endMove`, and after a conveyor move (with its turn boundary set).

use crate::{Color, EngineError, GameState, Piece, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct Victim {
    piece: Piece,
    square: Square,
    capture_owner: Color,
}

/// Resolve an entire source bomb sweep atomically. The returned count is the
/// number of triggered bombs, not the number of victims; callers must inspect
/// `state.mode` because an environmental defeat may end the game.
pub(crate) fn resolve_under_pieces(
    state: &mut GameState,
    cause_color: Color,
    after_turn_boundary: bool,
) -> Result<usize> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 resolveRuleBombsUnderPieces on rules version {}",
            state.ruleset_id
        )));
    }
    let mut next = state.clone();
    let count = resolve_inner(&mut next, cause_color, after_turn_boundary)?;
    *state = next;
    Ok(count)
}

fn resolve_inner(state: &mut GameState, cause: Color, after_boundary: bool) -> Result<usize> {
    let bombs = normalize_bombs(state)?;
    if bombs.is_empty() {
        // The source returns before replacing the original ruleBombs value.
        return Ok(0);
    }
    let mut triggered = Vec::new();
    let mut retained = Vec::new();
    for mut bomb in bombs {
        let square = bomb_square(&bomb)?;
        let occupant = at(state, square);
        if bomb.get("ignorePieceId").and_then(Value::as_str)
            == occupant.map(|piece| piece.id.as_str())
            && bomb.get("ignorePieceId").is_some()
        {
            retained.push(bomb);
            continue;
        }
        bomb.as_object_mut()
            .expect("normalized bomb")
            .remove("ignorePieceId");
        if occupant.is_some() {
            triggered.push(bomb);
        } else {
            retained.push(bomb);
        }
    }
    state.extra.insert("ruleBombs".into(), json!(retained));
    if triggered.is_empty() {
        return Ok(0);
    }

    let threat_probe = state.threat_probe_depth > 0;
    crate::v7_threat::mark_king_threat_effect_cause(state, "폭탄", threat_probe)?;

    let impacted = impacted_cells(state, &triggered)?;
    let mut victims = collect_victims(state, cause, &impacted)?;
    for victim in &mut victims {
        // Source removed[] retains live object references. Earlier victims
        // can grant Vigilance to a royal that this same sweep removes later.
        if let Some(current) = state
            .board
            .iter()
            .flatten()
            .flatten()
            .find(|piece| piece.id == victim.piece.id)
        {
            victim.piece = current.clone();
        }
        remove_piece(state, victim);
        crate::transition::grant_vigilance_protection(state, &victim.piece)?;
        crate::v7_piece_lifecycle::schedule_undead_resurrection(
            state,
            &victim.piece,
            victim.capture_owner,
            after_boundary,
        )?;
        cancel_prophecies(state)?;
        if victim.piece.color.owner().is_some() {
            state
                .captures
                .get_mut(victim.capture_owner)
                .push(victim.piece.clone());
        }
    }

    let bomb_cells = triggered
        .iter()
        .map(|bomb| json!({"row":bomb["row"],"col":bomb["col"]}))
        .collect::<Vec<_>>();
    if !threat_probe {
        crate::replay::queue_visual(
            state,
            json!({"type":"rule-bomb-sweep","color":cause,"bombs":bomb_cells,"cells":impacted}),
        )?;
        crate::replay::queue_special_effect_notation(
            state,
            cause,
            &format!("폭탄×{}", triggered.len()),
            &format!(
                "폭탄 {}개 폭발로 기물 {}개 제거",
                triggered.len(),
                victims.len()
            ),
        )?;
    }
    crate::replay::add_log(
        state,
        format!(
            "폭탄 {}개가 폭발해 {}개의 기물이 사라졌습니다.",
            triggered.len(),
            victims.len()
        ),
    )?;
    if !victims.is_empty() {
        mark_deathmatch_progress(state)?;
    }
    resolve_environmental_defeats(state, &victims)?;
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(triggered.len())
}

fn at(state: &GameState, square: Square) -> Option<&Piece> {
    state
        .board
        .get(square.row as usize)?
        .get(square.col as usize)?
        .as_ref()
}

fn unsupported(detail: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!("v7 resolveRuleBombsUnderPieces: {detail}"))
}

fn board_dimensions(state: &GameState) -> Result<(usize, usize)> {
    // The source defaults to 8×8 when the board array is absent or empty and
    // otherwise takes the maximum row width, including for jagged boards.
    let rows = state.board.len().max(1);
    let rows = if state.board.is_empty() { 8 } else { rows };
    let cols = state.board.iter().map(Vec::len).max().unwrap_or(8).max(1);
    if rows > usize::from(u8::MAX) || cols > usize::from(u8::MAX) {
        return Err(unsupported("board dimensions exceed native Square range"));
    }
    Ok((rows, cols))
}

fn source_number(value: Option<&Value>, field: &str) -> Result<Option<f64>> {
    match value {
        Some(Value::Array(_) | Value::Object(_)) => Err(unsupported(&format!(
            "{field} uses unported JavaScript object coercion"
        ))),
        _ => Ok(crate::card_effects::js_number(value, 0)),
    }
}

fn truncate_source_text(value: &str) -> Result<String> {
    String::from_utf16(&value.encode_utf16().take(160).collect::<Vec<_>>())
        .map_err(|_| unsupported("rule bomb text ends in a lone surrogate"))
}

fn normalize_bombs(state: &GameState) -> Result<Vec<Value>> {
    let Some(input) = state.extra.get("ruleBombs").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let (rows, cols) = board_dimensions(state)?;
    let limit = input.len().max(3).min(rows.saturating_mul(cols));
    let mut seen = BTreeSet::new();
    let mut bombs = Vec::new();
    for (index, entry) in input.iter().enumerate() {
        if bombs.len() == limit {
            break;
        }
        let Some(row) = source_number(entry.get("row"), "ruleBombs.row")? else {
            continue;
        };
        let Some(col) = source_number(entry.get("col"), "ruleBombs.col")? else {
            continue;
        };
        if row.fract() != 0.0
            || col.fract() != 0.0
            || !(0.0..rows as f64).contains(&row)
            || !(0.0..cols as f64).contains(&col)
        {
            continue;
        }
        let square = Square {
            row: row as u8,
            col: col as u8,
        };
        if !seen.insert(square) {
            continue;
        }
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(truncate_source_text)
            .transpose()?
            .unwrap_or_else(|| format!("rule-bomb-{index}-{}-{}", square.row, square.col));
        let mut bomb = json!({"id":id,"row":square.row,"col":square.col});
        if let Some(ignore) = entry
            .get("ignorePieceId")
            .and_then(Value::as_str)
            .filter(|ignore| !ignore.is_empty())
        {
            bomb["ignorePieceId"] = json!(truncate_source_text(ignore)?);
        }
        bombs.push(bomb);
    }
    Ok(bombs)
}

fn bomb_square(bomb: &Value) -> Result<Square> {
    Ok(Square {
        row: bomb["row"]
            .as_u64()
            .ok_or_else(|| unsupported("normalized bomb row"))? as u8,
        col: bomb["col"]
            .as_u64()
            .ok_or_else(|| unsupported("normalized bomb col"))? as u8,
    })
}

fn impacted_cells(state: &GameState, bombs: &[Value]) -> Result<Vec<Square>> {
    let (rows, cols) = board_dimensions(state)?;
    let mut seen = BTreeSet::new();
    let mut cells = Vec::new();
    for bomb in bombs {
        let at = bomb_square(bomb)?;
        for row in 0..rows {
            let cell = Square {
                row: row as u8,
                col: at.col,
            };
            if seen.insert(cell) {
                cells.push(cell);
            }
        }
        for col in 0..cols {
            if col == at.col as usize {
                continue;
            }
            let cell = Square {
                row: at.row,
                col: col as u8,
            };
            if seen.insert(cell) {
                cells.push(cell);
            }
        }
    }
    Ok(cells)
}

fn collect_victims(state: &GameState, cause: Color, cells: &[Square]) -> Result<Vec<Victim>> {
    let mut identities = BTreeMap::<String, Piece>::new();
    let mut victims = Vec::new();
    for &square in cells {
        let Some(piece) = at(state, square) else {
            continue;
        };
        if matches!(piece.kind.as_str(), "football" | "monster") {
            continue;
        }
        if piece.id.is_empty() {
            return Err(unsupported("affected piece without source identity"));
        }
        if let Some(earlier) = identities.get(&piece.id) {
            if earlier != piece {
                return Err(unsupported(
                    "inconsistent duplicate affected piece identity",
                ));
            }
            continue;
        }
        identities.insert(piece.id.clone(), piece.clone());
        let origin = if piece.is_large() {
            let row = crate::observation::number(piece.extra.get("anchorRow"));
            let col = crate::observation::number(piece.extra.get("anchorCol"));
            match (row, col) {
                (Some(row), Some(col))
                    if row.fract() == 0.0
                        && col.fract() == 0.0
                        && (0.0..256.0).contains(&row)
                        && (0.0..256.0).contains(&col)
                        && at(
                            state,
                            Square {
                                row: row as u8,
                                col: col as u8,
                            },
                        )
                        .is_some_and(|anchor| anchor.id == piece.id && anchor.is_large()) =>
                {
                    Square {
                        row: row as u8,
                        col: col as u8,
                    }
                }
                _ => square,
            }
        } else {
            square
        };
        victims.push(Victim {
            piece: piece.clone(),
            square: origin,
            capture_owner: piece.color.owner().map_or(cause, Color::opponent),
        });
    }
    Ok(victims)
}

fn remove_piece(state: &mut GameState, victim: &Victim) {
    for row in &mut state.board {
        for cell in row {
            if cell
                .as_ref()
                .is_some_and(|piece| piece.id == victim.piece.id)
            {
                *cell = None;
            }
        }
    }
}

pub(crate) fn cancel_prophecies(state: &mut GameState) -> Result<()> {
    let Some(value) = state.extra.get_mut("prophecy") else {
        return Ok(());
    };
    if value.is_null() {
        return Ok(());
    }
    let object = value
        .as_object_mut()
        .ok_or_else(|| EngineError::InvalidState("v7 prophecy must be an object".into()))?;
    for color in [Color::White, Color::Black] {
        if object
            .get(color.as_str())
            .is_some_and(|value| crate::observation::truth(Some(value)))
        {
            object.insert(color.as_str().into(), Value::Null);
        }
    }
    Ok(())
}

pub(crate) fn mark_deathmatch_progress(state: &mut GameState) -> Result<()> {
    let Some(deathmatch) = state.extra.get("deathmatch") else {
        return Ok(());
    };
    if !crate::observation::truth(deathmatch.get("active")) {
        return Ok(());
    }
    let limit_turns = source_number(
        state.extra.get("deathmatchLimitTurns"),
        "deathmatchLimitTurns",
    )?
    .filter(|turns| *turns > 0.0)
    .map_or(10.0, |turns| (turns + 0.5).floor().max(1.0));
    let default_half_turns = (limit_turns * 2.0).max(1.0);
    let interval = source_number(
        deathmatch.get("intervalHalfTurns"),
        "deathmatch.intervalHalfTurns",
    )?
    .filter(|number| *number != 0.0)
    .unwrap_or(default_half_turns)
    .max(1.0);
    let current = source_number(
        deathmatch.get("halfTurnsSinceProgress"),
        "deathmatch.halfTurnsSinceProgress",
    )?
    .unwrap_or(0.0)
    .clamp(0.0, interval);
    let object = state
        .extra
        .get_mut("deathmatch")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            EngineError::InvalidState("v7 active deathmatch must be an object".into())
        })?;
    object.insert("intervalHalfTurns".into(), json!(interval));
    object.insert("halfTurnsSinceProgress".into(), json!(current));
    if state.mode != "gameover" {
        object.insert("halfTurnsSinceProgress".into(), json!(0));
        object.insert("progressThisTurn".into(), json!(true));
        object.insert("warningKey".into(), json!(""));
    }
    Ok(())
}

fn resolve_environmental_defeats(state: &mut GameState, victims: &[Victim]) -> Result<()> {
    let removed = victims
        .iter()
        .map(|victim| crate::v7_board_hazards::EnvironmentalRemoval {
            piece: victim.piece.clone(),
            square: victim.square,
            capture_owner: victim.capture_owner,
        })
        .collect::<Vec<_>>();
    crate::v7_board_hazards::resolve_environmental_defeats(state, &removed, "폭탄", false)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameConfig, PieceColor};
    use sha2::{Digest, Sha256};

    fn initial() -> GameState {
        crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..Default::default()
            },
            19,
        )
        .unwrap()
    }

    fn source_state_digest(state: &GameState) -> String {
        let mut raw = serde_json::to_value(state).unwrap();
        let fields = raw.as_object_mut().unwrap();
        for outer in ["rulesetId", "rng", "history"] {
            fields.remove(outer);
        }
        format!("{:x}", Sha256::digest(serde_jcs::to_vec(&raw).unwrap()))
    }

    fn sparse_board() -> GameState {
        let mut state = initial();
        let knight = state.board[7][1].clone();
        for (row, line) in state.board.iter_mut().enumerate() {
            for (col, cell) in line.iter_mut().enumerate() {
                if !((row == 0 || row == 7) && col == 4) {
                    *cell = None;
                }
            }
        }
        state.board[4][0] = knight;
        state
            .extra
            .insert("ruleBombs".into(), json!([{"id":"1","row":4,"col":0}]));
        state
    }

    #[test]
    fn installation_ignore_id_prevents_self_trigger_without_state_or_rng_change() {
        let mut state = initial();
        let pawn_id = state.board[6][0].as_ref().unwrap().id.clone();
        state.extra.insert(
            "ruleBombs".into(),
            json!([{"id":"1","row":6,"col":0,"ignorePieceId":pawn_id}]),
        );
        assert_eq!(
            source_state_digest(&state),
            "c8c4463b721306e147963d0b5cd4950c1c520dd8f0c07832ba85f3dcfcedd9fb"
        );
        let before = serde_json::to_value(&state).unwrap();
        assert_eq!(
            resolve_under_pieces(&mut state, Color::White, false).unwrap(),
            0
        );
        assert_eq!(serde_json::to_value(state).unwrap(), before);
    }

    #[test]
    fn inert_bomb_list_normalizes_once_like_frozen_source() {
        let mut state = initial();
        state.extra.insert(
            "ruleBombs".into(),
            json!([
                {"id":"1","row":4,"col":0,"unexpected":true},
                {"id":"2","row":4,"col":0},
                {"id":"3","row":99,"col":9}
            ]),
        );
        assert_eq!(
            source_state_digest(&state),
            "ef1061aa6cf84aba23f0463669d827429b214228c29c99a15a85b71e1b5b5195"
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert_eq!(
            resolve_under_pieces(&mut state, Color::White, false).unwrap(),
            0
        );
        assert_eq!(
            source_state_digest(&state),
            "229fba0bedd46fba74225ba611b4fdb8e356e88471d0a6ebe21ba8273b246e4f"
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn one_knight_blast_matches_frozen_state_rng_and_history() {
        let mut state = sparse_board();
        assert_eq!(
            source_state_digest(&state),
            "60da522eb2ef1badc71e0ac8d131842bcd45897a5f0e13216426c924ade28776"
        );
        let history = state.history.clone();
        let board_history = state.extra.get("boardHistory").cloned();
        assert_eq!(
            resolve_under_pieces(&mut state, Color::White, false).unwrap(),
            1
        );
        assert_eq!(
            source_state_digest(&state),
            "06978bc69995253a9e1ab87b9fd89972cec36b6a17292fa1e63eca0131645c11"
        );
        assert_eq!(state.rng.state, 2_893_839_862);
        assert_eq!(state.rng.cursor, 33);
        assert_eq!(state.history, history);
        assert_eq!(state.extra.get("boardHistory"), board_history.as_ref());
        assert_eq!(state.captures.black.len(), 1);
        assert_eq!(state.mode, "play");
    }

    #[test]
    fn armed_deathmatch_progress_reset_matches_frozen_state() {
        let mut state = sparse_board();
        state.extra.insert("deathmatchEnabled".into(), json!(true));
        state.extra.insert(
            "deathmatch".into(),
            json!({
                "active":true,
                "startedAtTurn":0,
                "intervalHalfTurns":20,
                "halfTurnsSinceProgress":5,
                "progressThisTurn":false,
                "warningKey":"8"
            }),
        );
        assert_eq!(
            source_state_digest(&state),
            "3665ed346890132408a17ad012fe46a1a5243685d100accc6b6d909e0985fc89"
        );
        let history = state.history.clone();
        assert_eq!(
            resolve_under_pieces(&mut state, Color::White, false).unwrap(),
            1
        );
        assert_eq!(
            source_state_digest(&state),
            "0ba8836e4ebc7801549f50ae3378de71985427bc32f791bf18174b42540f0de8"
        );
        assert_eq!(state.extra["deathmatch"]["halfTurnsSinceProgress"], 0);
        assert_eq!(state.extra["deathmatch"]["progressThisTurn"], true);
        assert_eq!(state.rng.cursor, 33);
        assert_eq!(state.history, history);
    }

    #[test]
    fn neutral_football_triggers_bomb_but_survives() {
        let mut state = sparse_board();
        let football = state.board[4][0].as_mut().unwrap();
        football.kind = "football".into();
        football.color = PieceColor::Neutral;
        assert_eq!(
            source_state_digest(&state),
            "f1243cb1eae0e52a1ca36dd6b53e403b5e8225cf48d7b3ca93195bc3a3a98f9b"
        );
        assert_eq!(
            resolve_under_pieces(&mut state, Color::White, false).unwrap(),
            1
        );
        assert_eq!(
            source_state_digest(&state),
            "e816fdd1917fbbcc56ad88088dccebe2e18ae00ada877ff1cd58f386f68764b0"
        );
        assert_eq!(state.board[4][0].as_ref().unwrap().kind, "football");
        assert_eq!(state.captures.white.len() + state.captures.black.len(), 0);
        assert_eq!(state.rng.cursor, 33);
    }

    #[test]
    fn both_kings_in_cross_end_in_frozen_environmental_draw() {
        let mut state = sparse_board();
        state.board[4][4] = state.board[4][0].take();
        state.extra["ruleBombs"][0]["col"] = json!(4);
        assert_eq!(
            source_state_digest(&state),
            "30537922e53d1cd7aa844e3ca4f912d7d2b67872ba0c8bdeba79d47b3738ce99"
        );
        let history = state.history.clone();
        assert_eq!(
            resolve_under_pieces(&mut state, Color::White, false).unwrap(),
            1
        );
        assert_eq!(
            source_state_digest(&state),
            "70d8705a03e030b4c79327553654c9e6d59fca2916054290e69a57e2591af029"
        );
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner, None);
        assert_eq!(
            state.extra["replayEndReason"],
            "폭탄으로 양쪽 킹이 함께 쓰러졌습니다."
        );
        assert_eq!(state.captures.black.len(), 2);
        assert_eq!(state.captures.white.len(), 1);
        assert_eq!(state.rng.cursor, 33);
        assert_eq!(state.history, history);
    }

    #[test]
    fn unsupported_reaper_count_coercion_rolls_back_the_whole_blast() {
        let mut state = sparse_board();
        let mut reaper = Piece::new("reaper", Color::Black, "reaper-fixture");
        reaper
            .extra
            .insert("reaperCaptures".into(), json!({"unsupported":"coercion"}));
        state.board[5][1] = Some(reaper);
        let before = serde_json::to_value(&state).unwrap();
        let error = resolve_under_pieces(&mut state, Color::White, false).unwrap_err();
        assert!(format!("{error}").contains("reaperCaptures JavaScript object coercion"));
        assert_eq!(serde_json::to_value(state).unwrap(), before);
    }

    #[test]
    fn surviving_adjacent_reaper_receives_a_soul_after_the_environmental_blast() {
        let mut state = sparse_board();
        state.board[5][1] = Some(Piece::new("reaper", Color::Black, "reaper-fixture"));
        assert_eq!(
            source_state_digest(&state),
            "f16bcef180831d89c63f3607e5bb3261536c313f1917ee0451659ebdc78c4f85"
        );
        let history = state.history.clone();
        assert_eq!(
            resolve_under_pieces(&mut state, Color::White, false).unwrap(),
            1
        );
        assert_eq!(
            source_state_digest(&state),
            "1bc49986bf0f00c89d9f3c494880cc3668bac81027d5614aced945170ff06ea5"
        );
        assert_eq!(
            state.board[5][1].as_ref().unwrap().extra["reaperCaptures"],
            json!(1.0)
        );
        assert_eq!(state.mode, "play");
        assert_eq!(state.rng.cursor, 33);
        assert_eq!(state.history, history);
    }

    #[test]
    fn fourth_reaper_soul_executes_then_relocates_like_frozen_source() {
        let mut state = sparse_board();
        let mut reaper = Piece::new("reaper", Color::Black, "reaper-fixture");
        reaper.extra.insert("reaperCaptures".into(), json!(3));
        state.board[5][1] = Some(reaper);
        assert_eq!(
            source_state_digest(&state),
            "328d7a687cb21afd602a757a8d6c83756492c72d976211b8c9744a5df09a4d56"
        );
        let history = state.history.clone();
        let board_history = state.extra.get("boardHistory").cloned();
        assert_eq!(
            resolve_under_pieces(&mut state, Color::White, false).unwrap(),
            1
        );
        assert_eq!(
            source_state_digest(&state),
            "cc460bae8e45b0ac9e5700c7930af4ca75f3ead6cdde1bdddf396fbf3fe2576b"
        );
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner.as_deref(), Some("black"));
        assert_eq!(state.extra["replayEndReason"], "백 킹이 잡혔습니다.");
        assert_eq!(state.board[7][4].as_ref().unwrap().id, "reaper-fixture");
        assert!(state.board[5][1].is_none());
        assert_eq!(state.rng.state, 3_858_193_629);
        assert_eq!(state.rng.cursor, 34);
        assert_eq!(state.history, history);
        assert_eq!(state.extra.get("boardHistory"), board_history.as_ref());
    }

    #[test]
    fn recurring_bomb_victim_revives_before_royal_and_reaper_adjudication() {
        let mut state = sparse_board();
        state.board[4][0]
            .as_mut()
            .unwrap()
            .extra
            .insert("recurrence".into(), json!(true));
        assert_eq!(
            source_state_digest(&state),
            "3fb4eb736ea150167acc51976b6f7674b890d25406c05ba3008c25fa54a71165"
        );
        let history = state.history.clone();
        assert_eq!(
            resolve_under_pieces(&mut state, Color::White, false).unwrap(),
            1
        );
        assert_eq!(
            source_state_digest(&state),
            "b36594972d0048f654194eaa311239fd05503df662d772f55357f55b69e588ae"
        );
        assert_eq!(state.extra["pendingRecurrences"], json!([]));
        assert!(state.captures.black.is_empty());
        assert_eq!(state.mode, "play");
        assert_eq!(state.rng.state, 3_858_193_629);
        assert_eq!(state.rng.cursor, 34);
        assert_eq!(state.history, history);
    }

    #[test]
    fn vigilance_updates_a_later_removed_royal_reference_in_the_same_blast() {
        let mut state = initial();
        state
            .extra
            .insert("vigilance".into(), json!({"white":true,"black":false}));
        state
            .extra
            .insert("ruleBombs".into(), json!([{"id":"1","row":6,"col":4}]));
        assert_eq!(
            source_state_digest(&state),
            "31e477f1c97f02ab8358cd58fa6998be6b6ab5bfbdc6a845a649d0d6d005aaae"
        );
        let history = state.history.clone();
        assert_eq!(
            resolve_under_pieces(&mut state, Color::White, false).unwrap(),
            1
        );
        assert_eq!(
            source_state_digest(&state),
            "04e300b935caed9d7f148a7ed50d21d97613f623a5d0c09367272a211b60a33e"
        );
        let captured_king = state
            .captures
            .black
            .iter()
            .find(|piece| piece.kind == "king")
            .unwrap();
        assert_eq!(
            captured_king.extra["vigilanceProtection"]["countBy"],
            "black"
        );
        assert_eq!(
            captured_king.extra["vigilanceProtection"]["remaining"].as_f64(),
            Some(1.0)
        );
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner, None);
        assert_eq!(state.rng.cursor, 33);
        assert_eq!(state.history, history);
    }
}
