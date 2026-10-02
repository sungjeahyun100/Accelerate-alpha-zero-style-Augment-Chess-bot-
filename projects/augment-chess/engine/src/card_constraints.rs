//! Typed v7 capture constraints created by card and RULE effects. The source
//! stores their live counters in the legacy piece/game DTO, so conversion is
//! explicit at the execution boundary and leaves the v6 path untouched.
#[cfg(test)]
use crate::Color;
use crate::{EngineError, GameState, Piece, RULES_VERSION_V7, Result, observation};
use serde_json::Value;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CaptureLockSource {
    FreshPiece,
    CardEffect,
    PromotionRush,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PieceConstraint {
    CaptureLockedUntil {
        source: CaptureLockSource,
        owner_turn: f64,
    },
    PotionSaturation {
        max_captures: u32,
    },
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GameConstraint {
    SaturationRule { max_captures: u32 },
    GenevaConvention { protected_side: Color },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CaptureConstraints {
    #[cfg(test)]
    pub(crate) piece: Vec<PieceConstraint>,
    #[cfg(test)]
    pub(crate) game: Vec<GameConstraint>,
    #[cfg(test)]
    owner_turns: u32,
    #[cfg(test)]
    captures_made: f64,
}

fn source_number(value: Option<&Value>, field: &str) -> Result<f64> {
    match value {
        None => Ok(0.0),
        Some(value) => observation::number(Some(value)).ok_or_else(|| {
            EngineError::InvalidState(format!("non-finite or invalid {field} card constraint"))
        }),
    }
}

impl CaptureConstraints {
    /// Read only the source fields that veto a capture. Other capture rules
    /// (protection, piece abilities, allegiance) remain in their own owners.
    pub(crate) fn from_source_state(state: &GameState, attacker: &Piece) -> Result<Self> {
        if state.ruleset_id != RULES_VERSION_V7 {
            return Err(EngineError::UnsupportedFeature(
                "typed capture constraints require v7".into(),
            ));
        }
        let actor = attacker.color.owner().ok_or(EngineError::IllegalAction)?;
        // The production decoder validates the live counters. The isolated
        // veto projection belongs to the tests; source movement keeps its own
        // ordered ability, protection and option checks.
        let _owner_turns = *state.turns_taken.get(actor);
        let _captures_made = source_number(attacker.extra.get("capturesMade"), "capturesMade")?;
        #[cfg(test)]
        let mut piece = Vec::new();
        for field in [
            "freshNoCaptureUntil",
            "cardNoCaptureUntil",
            "promotionRushUntil",
        ] {
            if let Some(value) = attacker.extra.get(field) {
                // Parsing stays in production even though only tests retain
                // the isolated veto projection of these validated values.
                let _owner_turn = source_number(Some(value), field)?;
                #[cfg(test)]
                piece.push(PieceConstraint::CaptureLockedUntil {
                    source: match field {
                        "freshNoCaptureUntil" => CaptureLockSource::FreshPiece,
                        "cardNoCaptureUntil" => CaptureLockSource::CardEffect,
                        _ => CaptureLockSource::PromotionRush,
                    },
                    owner_turn: _owner_turn,
                });
            }
        }
        #[cfg(test)]
        if observation::truth(attacker.extra.get("potionSaturation")) {
            piece.push(PieceConstraint::PotionSaturation { max_captures: 3 });
        }
        #[cfg(test)]
        let mut game = Vec::new();
        #[cfg(test)]
        if observation::truth(state.extra.get("saturationRule")) {
            game.push(GameConstraint::SaturationRule { max_captures: 3 });
        }
        #[cfg(test)]
        for protected_side in [Color::White, Color::Black] {
            if state.flag("genevaConvention", protected_side) {
                game.push(GameConstraint::GenevaConvention { protected_side });
            }
        }
        Ok(Self {
            #[cfg(test)]
            piece,
            #[cfg(test)]
            game,
            #[cfg(test)]
            owner_turns: _owner_turns,
            #[cfg(test)]
            captures_made: _captures_made,
        })
    }

    /// Call at the source's early lock check, before target-side protection.
    #[cfg(test)]
    pub(crate) fn piece_veto(&self) -> bool {
        self.piece.iter().any(|constraint| {
            matches!(constraint, PieceConstraint::CaptureLockedUntil { owner_turn, .. }
                if *owner_turn > f64::from(self.owner_turns))
        })
    }

    /// Call after ability/protection checks, in place of the source saturation
    /// and Geneva Convention branches. It has no RNG or state mutation.
    #[cfg(test)]
    pub(crate) fn game_veto(&self, attacker: &Piece, target: &Piece) -> bool {
        let saturation_cap = self
            .piece
            .iter()
            .filter_map(|constraint| match constraint {
                PieceConstraint::PotionSaturation { max_captures } => Some(*max_captures),
                _ => None,
            })
            .chain(self.game.iter().filter_map(|constraint| match constraint {
                GameConstraint::SaturationRule { max_captures } => Some(*max_captures),
                _ => None,
            }))
            .min();
        if saturation_cap.is_some_and(|cap| self.captures_made >= f64::from(cap)) {
            return true;
        }
        self.game.iter().any(|constraint| {
            matches!(constraint, GameConstraint::GenevaConvention { protected_side }
                if target.color == *protected_side
                    && attacker.kind == "queen"
                    && attacker.extra.get("regencyHeir") != Some(&Value::Bool(true))
                    && target.kind == "pawn")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;
    use serde_json::json;

    #[test]
    fn owner_turn_lock_and_card_lock_expire_at_exact_source_turn() {
        let mut state = GameState::new(GameConfig::default(), 8).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        let mut attacker = Piece::new("rook", Color::White, "rook-test");
        attacker
            .extra
            .insert("freshNoCaptureUntil".into(), json!(2));
        attacker.extra.insert("cardNoCaptureUntil".into(), json!(3));
        attacker.extra.insert("promotionRushUntil".into(), json!(4));
        *state.turns_taken.get_mut(Color::White) = 2;
        let constraints = CaptureConstraints::from_source_state(&state, &attacker).unwrap();
        assert_eq!(constraints.piece.len(), 3);
        assert!(constraints.piece_veto());
        *state.turns_taken.get_mut(Color::White) = 3;
        assert!(
            CaptureConstraints::from_source_state(&state, &attacker)
                .unwrap()
                .piece_veto()
        );
        *state.turns_taken.get_mut(Color::White) = 4;
        assert!(
            !CaptureConstraints::from_source_state(&state, &attacker)
                .unwrap()
                .piece_veto()
        );
    }

    #[test]
    fn saturation_and_geneva_keep_distinct_piece_and_game_provenance() {
        let mut state = GameState::new(GameConfig::default(), 8).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        let mut attacker = Piece::new("queen", Color::White, "queen-test");
        let pawn = Piece::new("pawn", Color::Black, "pawn-test");
        let knight = Piece::new("knight", Color::Black, "knight-test");
        state.set_flag("genevaConvention", Color::Black, true);
        let constraints = CaptureConstraints::from_source_state(&state, &attacker).unwrap();
        assert_eq!(
            constraints.game,
            vec![GameConstraint::GenevaConvention {
                protected_side: Color::Black
            }]
        );
        assert!(constraints.game_veto(&attacker, &pawn));
        assert!(!constraints.game_veto(&attacker, &knight));
        attacker.extra.insert("regencyHeir".into(), json!(true));
        assert!(!constraints.game_veto(&attacker, &pawn));
        attacker.extra.insert("regencyHeir".into(), json!(false));
        attacker
            .extra
            .insert("potionSaturation".into(), json!(true));
        attacker.extra.insert("capturesMade".into(), json!(3));
        let constraints = CaptureConstraints::from_source_state(&state, &attacker).unwrap();
        assert!(
            constraints
                .piece
                .contains(&PieceConstraint::PotionSaturation { max_captures: 3 })
        );
        assert!(constraints.game_veto(&attacker, &knight));
        attacker
            .extra
            .insert("potionSaturation".into(), json!(false));
        state.extra.insert("saturationRule".into(), json!(true));
        let constraints = CaptureConstraints::from_source_state(&state, &attacker).unwrap();
        assert!(
            constraints
                .game
                .contains(&GameConstraint::SaturationRule { max_captures: 3 })
        );
        assert!(constraints.game_veto(&attacker, &knight));
    }

    #[test]
    fn malformed_v7_counter_fails_closed_before_capture() {
        let mut state = GameState::new(GameConfig::default(), 8).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        let mut attacker = Piece::new("rook", Color::White, "rook-test");
        attacker
            .extra
            .insert("cardNoCaptureUntil".into(), json!({"bad":true}));
        assert!(matches!(
            CaptureConstraints::from_source_state(&state, &attacker),
            Err(EngineError::InvalidState(_))
        ));
        state.ruleset_id = crate::RULES_VERSION_V6.into();
        assert!(matches!(
            CaptureConstraints::from_source_state(&state, &attacker),
            Err(EngineError::UnsupportedFeature(_))
        ));
    }
}
