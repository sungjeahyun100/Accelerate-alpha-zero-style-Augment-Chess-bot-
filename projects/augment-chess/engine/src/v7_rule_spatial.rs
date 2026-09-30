//! Immediate spatial effects of the pinned September 28 v7 RULE cards.
//!
//! Selection, event identity, notation and settlement are owned by the draft
//! host. This module changes only the direct game state named by each effect.
//! Source: SHA-256 e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c,
//! main-OahWs0tU.js `enablePlatformRule`, `highGround`, `installRuleBombs`,
//! `enablePortalRule` and `captureTheFlag`.

use serde_json::{Value, json};

use crate::state::{EngineError, GameState, RULES_VERSION_V7, Result, Square};

/// Apply one verified direct effect atomically, including its RNG and log
/// changes. An error leaves the caller's state untouched.
pub(crate) fn apply(state: &mut GameState, card_id: &str) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "spatial RULE {card_id} outside pinned v7 ruleset"
        )));
    }
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::UnsupportedFeature(
            "spatial RULE requires the pinned 8x8 board".into(),
        ));
    }
    let mut next = state.clone();
    match card_id {
        "platform" => platform(&mut next)?,
        "high-ground" => high_ground(&mut next)?,
        "rule-bombs" => rule_bombs(&mut next)?,
        "portal" => portal(&mut next),
        "capture-the-flag" => capture_the_flag(&mut next)?,
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "spatial RULE {card_id}"
            )));
        }
    }
    *state = next;
    Ok(())
}

fn shuffle<T>(state: &mut GameState, values: &mut [T]) -> Result<()> {
    // Source shuffle copies the list, then performs descending Fisher-Yates.
    // All callers own a fresh list, so the copy would add no observable state.
    for index in (1..values.len()).rev() {
        let destination = sample_index(state, index + 1)?;
        values.swap(index, destination);
    }
    Ok(())
}

fn sample_index(state: &mut GameState, len: usize) -> Result<usize> {
    let roll = if len == 0 {
        state
            .rng
            .sample_invariant("source empty spatial RULE candidate")?
    } else {
        state.rng.sample()?
    };
    if !roll.is_finite() || !(0.0..1.0).contains(&roll) {
        return Err(EngineError::InvalidState(
            "spatial RULE random draw outside [0,1)".into(),
        ));
    }
    if len > 0 {
        state
            .rng
            .record_last_probability(1.0 / len as f64, "source spatial RULE candidate index")?;
    }
    Ok((roll * len as f64).floor() as usize)
}

fn platform(state: &mut GameState) -> Result<()> {
    // `platformTurnCount(..., {countUnit:"ply"})` adds both sides before the
    // first spawn. The direct effect replaces any prior platform rule.
    let current_turn = u64::from(state.turns_taken.white)
        .checked_add(u64::from(state.turns_taken.black))
        .ok_or_else(|| EngineError::InvalidState("platform turn count overflow".into()))?;
    let next_at = current_turn
        .checked_add(10)
        .ok_or_else(|| EngineError::InvalidState("platform nextAt overflow".into()))?;
    let mut candidates = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            if crate::movement::open_placement(state, at, None)?
                && !crate::movement::portal_installation_hazard(state, at)
            {
                candidates.push(at);
            }
        }
    }
    // The source calls `randomChoice(candidates)` even when no cell exists.
    let selected = sample_index(state, candidates.len())?;
    let cell = candidates.get(selected).copied();
    state.extra.insert(
        "platformRule".into(),
        json!({
            "enabled": true,
            "countUnit": "ply",
            "cadence": "full-turn",
            "nextAt": next_at,
            "cell": cell,
            "cells": cell.into_iter().collect::<Vec<_>>(),
            "fixed": false,
            "spawnedAt": current_turn,
            "nonce": 1,
            "triggeredIds": [],
        }),
    );
    let message = match cell {
        Some(at) => format!(
            "발판: {}{}에 새 발판이 생성되었습니다.",
            char::from(b'a' + at.col),
            8 - at.row
        ),
        None => "발판: 설치할 빈칸이 없어 이번 발판이 생성되지 않았습니다.".into(),
    };
    crate::replay::add_log(state, message)
}

fn centered_rectangle(row_span: u8, col_span: u8) -> Vec<Square> {
    let rows = row_span.clamp(1, 8);
    let cols = col_span.clamp(1, 8);
    let row_start = (8 - rows) / 2;
    let col_start = (8 - cols) / 2;
    let mut cells = Vec::with_capacity(rows as usize * cols as usize);
    for row in row_start..row_start + rows {
        for col in col_start..col_start + cols {
            cells.push(Square { row, col });
        }
    }
    cells
}

fn high_ground(state: &mut GameState) -> Result<()> {
    let mut center = centered_rectangle(2, 4);
    let mut outer: Vec<_> = centered_rectangle(4, 6)
        .into_iter()
        .filter(|cell| !center.contains(cell))
        .collect();
    shuffle(state, &mut center)?;
    let mut selected: Vec<Square> = center.into_iter().take(7).collect();
    // Source shuffles the *whole* outer ring, consuming every draw before
    // `.find` returns the first outside cell.
    shuffle(state, &mut outer)?;
    if let Some(at) = outer.into_iter().find(|cell| !selected.contains(cell)) {
        selected.push(at);
    }
    state.extra.insert("highGround".into(), json!(selected));
    Ok(())
}

fn rule_bombs(state: &mut GameState) -> Result<()> {
    // `ruleBombCandidateCells(8,8)` iterates row 4 before row 3 and omits
    // column 4. Shuffle precedes the empty/occupied stable partition.
    let mut candidates = Vec::with_capacity(14);
    for row in [4, 3] {
        for col in 0..8 {
            if col != 4 {
                candidates.push(Square { row, col });
            }
        }
    }
    shuffle(state, &mut candidates)?;
    let mut vacant = Vec::new();
    let mut occupied = Vec::new();
    for at in candidates {
        if state.at(at).is_none() {
            vacant.push(at);
        } else {
            occupied.push(at);
        }
    }
    let selected: Vec<_> = vacant.into_iter().chain(occupied).take(3).collect();
    if selected.len() < 3 {
        return Err(EngineError::InvalidState(
            "RULE bombs require three eligible fourth/fifth-rank cells".into(),
        ));
    }
    // The offline source fixes Date.now() at the catalog timestamp. It is
    // shared by all three bombs; no RNG draw occurs for these identities.
    let timestamp = crate::draft::frozen_timestamp_for_ruleset(RULES_VERSION_V7)?;
    let bombs: Vec<Value> = selected
        .into_iter()
        .enumerate()
        .map(|(index, at)| {
            let mut bomb = json!({
                "id": format!("rule-bomb-{timestamp}-{index}-{}-{}", at.row, at.col),
                "row": at.row,
                "col": at.col,
            });
            if let Some(piece) = state.at(at).filter(|piece| !piece.id.is_empty()) {
                bomb["ignorePieceId"] = json!(piece.id);
            }
            bomb
        })
        .collect();
    state.extra.insert("ruleBombs".into(), json!(bombs));
    Ok(())
}

fn portal(state: &mut GameState) {
    // c3 and f6 under source algebraic-to-row conversion on an 8x8 board.
    state.extra.insert(
        "portalRule".into(),
        json!({"enabled":true,"cells":[{"row":5,"col":2},{"row":2,"col":5}]}),
    );
}

fn capture_the_flag(state: &mut GameState) -> Result<()> {
    if state
        .extra
        .get("captureTheFlag")
        .is_some_and(|value| crate::observation::truth(Some(value)))
    {
        return Err(EngineError::IllegalAction);
    }
    // The source selects the white home-rank file first, then black.
    let white = sample_index(state, 8)? as u8;
    let black = sample_index(state, 8)? as u8;
    state.extra.insert(
        "captureTheFlag".into(),
        json!({
            "flags": {"white":{"row":7,"col":white},"black":{"row":0,"col":black}},
            "occupations": {"white":null,"black":null},
        }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Color, Piece, RngState};

    fn initial(seed: u64) -> GameState {
        crate::draft::initialize_for_ruleset(
            crate::state::GameConfig {
                draft_delete: true,
                ..Default::default()
            },
            seed,
            RULES_VERSION_V7,
        )
        .expect("supported v7 initial state")
    }

    #[test]
    fn spatial_rules_keep_source_candidate_order_and_rng_budget() {
        let base = initial(19);
        let cursor = base.rng.cursor;
        let mut high = base.clone();
        apply(&mut high, "high-ground").unwrap();
        assert_eq!(high.rng.cursor - cursor, 22);
        assert_eq!(high.extra["highGround"].as_array().unwrap().len(), 8);
        let mut bombs = base.clone();
        apply(&mut bombs, "rule-bombs").unwrap();
        assert_eq!(bombs.rng.cursor - cursor, 13);
        assert_eq!(bombs.extra["ruleBombs"].as_array().unwrap().len(), 3);
        let mut flags = base;
        apply(&mut flags, "capture-the-flag").unwrap();
        assert_eq!(flags.rng.cursor - cursor, 2);
        assert_eq!(flags.extra["captureTheFlag"]["flags"]["white"]["row"], 7);
        assert_eq!(flags.extra["captureTheFlag"]["flags"]["black"]["row"], 0);
    }

    #[test]
    fn failure_leaves_state_and_rng_unchanged() {
        let mut state = initial(19);
        let before = state.clone();
        assert!(matches!(
            apply(&mut state, "not-a-source-rule"),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert_eq!(state, before);
        state.rng = RngState {
            algorithm: "lcg32-v1".into(),
            tape: vec![1.0],
            cursor: 0,
            ..state.rng
        };
        let before = state.clone();
        assert!(matches!(
            apply(&mut state, "high-ground"),
            Err(EngineError::InvalidState(_))
        ));
        assert_eq!(state, before);
        state.rng = RngState {
            algorithm: "unknown".into(),
            ..state.rng
        };
        let before = state.clone();
        assert!(matches!(
            apply(&mut state, "high-ground"),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn platform_uses_source_ply_count_and_logs_first_spawn() {
        let mut state = initial(19);
        state.turns_taken.white = 2;
        state.turns_taken.black = 1;
        state.turn = Color::White;
        let prior = state.rng.cursor;
        apply(&mut state, "platform").unwrap();
        assert_eq!(state.rng.cursor, prior + 1);
        assert_eq!(state.extra["platformRule"]["spawnedAt"], 3);
        assert_eq!(state.extra["platformRule"]["nextAt"], 13);
        assert_eq!(state.extra["platformRule"]["nonce"], 1);
        assert!(state.extra["platformRule"]["cell"].is_object());
        assert!(
            state.extra["logs"].as_array().unwrap()[0]
                .as_str()
                .unwrap()
                .starts_with("발판:")
        );
    }

    #[test]
    fn platform_no_empty_cell_still_consumes_source_draw() {
        let mut state = initial(19);
        for row in 0..8 {
            for col in 0..8 {
                if state.board[row][col].is_none() {
                    state.board[row][col] = Some(Piece::new(
                        "pawn",
                        Color::White,
                        format!("fill-{row}-{col}"),
                    ));
                }
            }
        }
        let cursor = state.rng.cursor;
        apply(&mut state, "platform").unwrap();
        assert_eq!(state.rng.cursor, cursor + 1);
        assert!(state.extra["platformRule"]["cell"].is_null());
        assert_eq!(state.extra["platformRule"]["cells"], json!([]));
    }

    #[test]
    fn pinned_source_seed19_rule_activation_matches_direct_effect_and_rng() {
        // FrozenClientSource/OracleRuntime.newGame({draftDelete:true,
        // ruleCardIds:[id]}, 19), pinned client SHA above. Check the chosen
        // effect and the whole RNG position; shuffle ordering errors
        // otherwise remain invisible.
        let cases = [
            (
                "platform",
                "platformRule",
                json!({
                    "enabled":true,"countUnit":"ply","cadence":"full-turn",
                    "nextAt":10,"cell":{"row":4,"col":1},
                    "cells":[{"row":4,"col":1}],"fixed":false,
                    "spawnedAt":0,"nonce":1,"triggeredIds":[],
                }),
                36,
                676_586_263,
            ),
            (
                "high-ground",
                "highGround",
                json!([
                    {"row":3,"col":4},{"row":4,"col":5},
                    {"row":4,"col":4},{"row":3,"col":2},
                    {"row":3,"col":5},{"row":4,"col":3},
                    {"row":3,"col":3},{"row":5,"col":5},
                ]),
                57,
                3_251_036_206,
            ),
            (
                "rule-bombs",
                "ruleBombs",
                json!([
                    {"id":"rule-bomb-1790581292828-0-3-5","row":3,"col":5},
                    {"id":"rule-bomb-1790581292828-1-4-5","row":4,"col":5},
                    {"id":"rule-bomb-1790581292828-2-3-2","row":3,"col":2},
                ]),
                48,
                2_271_779_651,
            ),
            (
                "portal",
                "portalRule",
                json!({"enabled":true,"cells":[{"row":5,"col":2},{"row":2,"col":5}]}),
                35,
                2_324_936_856,
            ),
            (
                "capture-the-flag",
                "captureTheFlag",
                json!({
                    "flags":{"white":{"row":7,"col":4},"black":{"row":0,"col":1}},
                    "occupations":{"white":null,"black":null},
                }),
                37,
                3_798_705_546,
            ),
        ];
        for (id, field, expected, cursor, rng_state) in cases {
            let state = crate::draft::initialize_for_ruleset(
                crate::state::GameConfig {
                    draft_delete: true,
                    rule_card_ids: vec![id.into()],
                    ..Default::default()
                },
                19,
                RULES_VERSION_V7,
            )
            .expect(id);
            assert_eq!(state.extra[field], expected, "{id} direct effect");
            assert_eq!(state.rng.cursor, cursor, "{id} RNG cursor");
            assert_eq!(state.rng.state, rng_state, "{id} RNG state");
        }
    }
}
