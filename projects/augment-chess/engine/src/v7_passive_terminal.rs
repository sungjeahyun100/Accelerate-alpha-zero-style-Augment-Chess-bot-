//! Source-ordered Highlander and Religious Victory terminal checks.
//!
//! Frozen client: `main-OahWs0tU.js` SHA-256
//! `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`.
//! `checkReligiousVictory` always checks Highlander first, even when neither
//! side owns Religious Victory. Different callers reach it at different
//! mutation stages, so keep their entry points explicit.

use crate::{Color, EngineError, GameState, RULES_VERSION_V7, Result};
use std::collections::BTreeSet;

/// The `religiousVictory` card's direct effect checks immediately after
/// enabling the flag. Its acquisition wrapper will check again after storing
/// `usedAt` and `passiveApplied`; the two source calls are not interchangeable.
pub(crate) fn after_religious_card_effect(state: &mut GameState) -> Result<bool> {
    check_religious_victory(state, "종교 승리")
}

/// `applyPassiveCardOnDraft` checks after the card becomes used, but before
/// writing its passive log and sharing it with a Clone owner.
pub(crate) fn after_passive_acquisition(state: &mut GameState) -> Result<bool> {
    check_religious_victory(state, "종교 승리")
}

/// The active-card wrapper checks after post-card hazards, palaces, and
/// Herald threats, before star-limit/deathmatch and its turn-ending branch.
pub(crate) fn after_active_card_settle(state: &mut GameState) -> Result<bool> {
    check_religious_victory(state, "종교 승리")
}

/// `endMove` checks after Witch Trials and piece-status ticks, before
/// clearing reposition marks and completed-turn status.
pub(crate) fn after_end_move_status(state: &mut GameState) -> Result<bool> {
    check_religious_victory(state, "종교 승리")
}

/// Promotion and forced auto-promotion use the same source callback. The
/// source argument is retained because some source probes supply a different
/// label, which becomes part of the terminal reason.
pub(crate) fn after_promotion(state: &mut GameState, source: &str) -> Result<bool> {
    check_religious_victory(state, source)
}

fn check_religious_victory(state: &mut GameState, source: &str) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 Religious Victory check on rules version {}",
            state.ruleset_id
        )));
    }
    if crate::v7_capture_objectives::check_internal_highlander(state)? {
        return Ok(true);
    }
    if state.mode == "gameover" {
        return Ok(false);
    }
    for color in [Color::White, Color::Black] {
        if !state.flag("religiousVictory", color) {
            continue;
        }
        let own = bishop_count(state, color);
        let enemy = bishop_count(state, color.opponent());
        if own >= enemy + 3 {
            crate::flow::end_game(
                state,
                Some(color),
                &format!("{source}: 비숍이 상대방보다 3개 더 많습니다."),
            )?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// Source `countPiecesOf` uses a separate ID set for each color. A large
/// bishop may occupy multiple cells but counts once for its current owner.
fn bishop_count(state: &GameState, color: Color) -> usize {
    let mut seen = BTreeSet::new();
    state
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|piece| piece.color == color && piece.kind == "bishop")
        .filter(|piece| seen.insert(piece.id.as_str()))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};

    fn initial() -> GameState {
        crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..Default::default()
            },
            19,
        )
        .expect("pinned v7 initial state")
    }

    fn source_state_digest(state: &GameState) -> String {
        let mut raw = serde_json::to_value(state).unwrap();
        let fields = raw.as_object_mut().unwrap();
        for outer in ["rulesetId", "rng", "history"] {
            fields.remove(outer);
        }
        format!("{:x}", Sha256::digest(serde_jcs::to_vec(&raw).unwrap()))
    }

    fn religious_witness() -> GameState {
        let mut state = initial();
        state.board[0][2] = None;
        state.board[0][5].as_mut().unwrap().color = Color::White.into();
        state.extra["religiousVictory"]["white"] = json!(true);
        state
    }

    #[test]
    fn religious_win_counts_piece_ids_once_and_matches_frozen_state() {
        let mut state = religious_witness();
        // The source holds the same object in these two cells. Rust stores
        // copies, so its source ID is the equivalence key.
        state.board[5][0] = state.board[7][2].clone();
        assert_eq!(
            source_state_digest(&state),
            "b39194434e60eca3c0f811b3663122f63398d435b7ee72196016f2c67e4bec4e"
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        let board_history = state.extra.get("boardHistory").cloned();
        assert!(after_active_card_settle(&mut state).unwrap());
        assert_eq!(
            source_state_digest(&state),
            "f7353c2c35e9a00f1480d530c21bf70a2bff35d43f9b3e12e1f2d3460870ccc2"
        );
        assert_eq!(state.winner.as_deref(), Some("white"));
        assert_eq!(
            state.extra["replayEndReason"],
            "종교 승리: 비숍이 상대방보다 3개 더 많습니다."
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
        assert_eq!(state.extra.get("boardHistory"), board_history.as_ref());
        let terminal = source_state_digest(&state);
        assert!(!after_passive_acquisition(&mut state).unwrap());
        assert_eq!(source_state_digest(&state), terminal);
    }

    #[test]
    fn religious_card_effect_checks_after_setting_flag_before_acquisition_bookkeeping() {
        let mut state = initial();
        state.board[0][2] = None;
        state.board[0][5].as_mut().unwrap().color = Color::White.into();
        state.board[5][0] = state.board[7][2].clone();
        assert_eq!(
            source_state_digest(&state),
            "80c7f6e52bfa3309bc28188fb096882a528e148587458b51cc8509ba4b95ebc6"
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        // `religiousVictory()` sets this field, calls the shared terminal
        // callback, then returns to its acquisition wrapper.
        state.extra["religiousVictory"]["white"] = json!(true);
        assert!(after_religious_card_effect(&mut state).unwrap());
        assert_eq!(
            source_state_digest(&state),
            "f7353c2c35e9a00f1480d530c21bf70a2bff35d43f9b3e12e1f2d3460870ccc2"
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn highlander_precedes_religious_win_at_passive_acquisition() {
        let mut state = religious_witness();
        for (row, line) in state.board.iter_mut().enumerate() {
            for (col, cell) in line.iter_mut().enumerate() {
                if !((row == 0 && (col == 4 || col == 5)) || (row == 7 && [2, 4, 5].contains(&col)))
                {
                    *cell = None;
                }
            }
        }
        state
            .extra
            .insert("highlander".into(), json!({"white":false,"black":true}));
        assert_eq!(
            source_state_digest(&state),
            "b7c2bdad49a0f42094aa516335e254ef88dae619eca41b7b831a00c09f7a420f"
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert!(after_passive_acquisition(&mut state).unwrap());
        assert_eq!(
            source_state_digest(&state),
            "685723ea3ceccb25611e4b9e2dbd2eb8388c142d7ec8825a0554167286d46a6d"
        );
        assert_eq!(state.winner.as_deref(), Some("black"));
        assert_eq!(
            state.extra["replayEndReason"],
            "하이랜더: 아군의 기물 종류가 모두 다릅니다."
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn empty_flags_preserve_frozen_full_state() {
        let mut state = initial();
        assert_eq!(
            source_state_digest(&state),
            "56de60cd87f53f39d1baaaba4261e7af96b402786e9c1f34fe52011642edc2e5"
        );
        let before = serde_json::to_value(&state).unwrap();
        assert!(!after_end_move_status(&mut state).unwrap());
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    fn rejects_wrong_rules_version_without_mutation() {
        let mut state = initial();
        state.ruleset_id = "other".into();
        let before: Value = serde_json::to_value(&state).unwrap();
        assert!(matches!(
            after_promotion(&mut state, "promotion probe"),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert_eq!(serde_json::to_value(state).unwrap(), before);
    }
}
