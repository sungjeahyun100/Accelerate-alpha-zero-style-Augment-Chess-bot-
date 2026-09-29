//! Source-visible card target squares for three frozen v7 first-play states.
//! These are UI selection hints, not the legal card action stream.

use crate::{Color, EngineError, GameState, Piece, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

struct SourceOpening {
    state_digest: &'static str,
    history_entry_digest: &'static str,
    rng_cursor: usize,
    rng_state: u32,
    display_slots: &'static [usize],
}

// %APPDATA%/Accelerate/reports/v7-public-hints/report.json, from the frozen
// main-OahWs0tU.js (SHA-256 e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c).
// The state digests omit the outer Position's rulesetId/rng/history, as the
// GameState serializer includes those three fields. A source-recorded last
// draft pick changes only outer history, not state or RNG.
fn opening_for(style: &str) -> Result<SourceOpening> {
    let profile = match style {
        "normal" => SourceOpening {
            state_digest: "a106b0ce0aeb64b5301a3f90a3fb1c0c32c4a6d4e504362d311587800c4a072c",
            history_entry_digest: "8606504fac022613b0232a4b26e22321249edd9708059837d732c1ce9932782b",
            rng_cursor: 216,
            rng_state: 3_909_686_763,
            display_slots: &[0, 1, 2],
        },
        "chaos" => SourceOpening {
            state_digest: "bf4efad54e5815e3e401e87adeab5ddce665484422d45e51d1fc6dec032fe62a",
            history_entry_digest: "c93fe4c6a24ed843fa1e70daedbc7795ac6422565666b7f5b649f2cd4de8086b",
            rng_cursor: 400,
            rng_state: 185_085_603,
            // Frozen deckSlotsForDisplay transposes the three chaos pairs.
            display_slots: &[0, 2, 4, 1, 3, 5],
        },
        "grand" => SourceOpening {
            state_digest: "6ccd5c79e3de0da607f36542da8d8640c8bb6b5892638e12c5b895e0cd5788ac",
            history_entry_digest: "283eab9eeeea6b81f670e616e5aa951b9fec8d8f2e41356f8fd248eff508b283",
            rng_cursor: 124,
            rng_state: 1_313_359_343,
            display_slots: &[0, 1, 2, 3, 4, 5],
        },
        _ => {
            return Err(EngineError::UnsupportedFeature(
                "v7 card target hints outside verified first-play styles".into(),
            ));
        }
    };
    Ok(profile)
}

fn source_digest(value: &Value) -> Result<String> {
    let canonical = serde_jcs::to_vec(value).map_err(EngineError::serialization)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

fn require_source_opening(state: &GameState) -> Result<SourceOpening> {
    let unsupported = || {
        EngineError::UnsupportedFeature(
            "v7 card targets outside source-verified seed-19 opening".into(),
        )
    };
    let style = state
        .extra
        .get("gameStyle")
        .and_then(Value::as_str)
        .ok_or_else(unsupported)?;
    let profile = opening_for(style)?;
    if state.rng.algorithm != "lcg32-v1"
        || !state.rng.tape.is_empty()
        || state.rng.cursor != profile.rng_cursor
        || state.rng.state != profile.rng_state
        || state.deck_slots.white.len() != profile.display_slots.len()
    {
        return Err(unsupported());
    }
    match state.history.as_slice() {
        [] => {}
        [event] if source_digest(event)? == profile.history_entry_digest => {}
        _ => return Err(unsupported()),
    }
    let mut serialized = serde_json::to_value(state).map_err(EngineError::serialization)?;
    let fields = serialized.as_object_mut().ok_or_else(unsupported)?;
    for outer_field in ["rulesetId", "rng", "history"] {
        if fields.remove(outer_field).is_none() {
            return Err(unsupported());
        }
    }
    if source_digest(&serialized)? != profile.state_digest {
        return Err(unsupported());
    }
    Ok(profile)
}

fn matches_target(
    card_id: &str,
    target: &str,
    piece: &Piece,
    viewer: Color,
    state: &GameState,
) -> Result<bool> {
    let own = piece.color == viewer;
    let enemy = piece.color == viewer.opponent();
    let nonroyal = !state.royal_identity(piece);
    let eligible = match (card_id, target) {
        ("queens-gambit", "own-queen") | ("grappler", "own-queen-and-minor") => {
            own && nonroyal && piece.kind == "queen"
        }
        ("inertia", "enemy-ranged") => {
            enemy && nonroyal && matches!(piece.kind.as_str(), "rook" | "bishop" | "queen")
        }
        ("miracle", "own-plain-bishop") => {
            // In these exact openings no bishop can capture. The frozen
            // miracleTargets predicate therefore yields no source square.
            false
        }
        ("trojan-horse", "own-knight") => {
            own && piece.kind == "knight" && !piece.flag("trojanHorse")
        }
        ("grasshopper", "own-minor") => {
            own && nonroyal && matches!(piece.kind.as_str(), "knight" | "bishop")
        }
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "v7 unverified card target predicate for {card_id}"
            )));
        }
    };
    Ok(eligible)
}

/// Project `getVisibleCards` display order and `getVisibleCardTargetSquares`
/// only for the three exact source-reachable seed-19 openings. The separate
/// card-action kernel owns legality: Grappler has one primary hint but four
/// compound legal actions, Miracle keeps an empty hint entry, and no-target
/// cards do not appear here.
pub(crate) fn v7_seed19_first_play_card_target_hints(
    state: &GameState,
    viewer: Color,
) -> Result<Vec<Value>> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 card target hints for rules version {}",
            state.ruleset_id
        )));
    }
    if state.mode != "play"
        || state.turn != viewer
        || crate::observation::truth(state.extra.get("pendingPromotion"))
        || crate::observation::truth(state.extra.get("activeTrolley"))
    {
        return Ok(Vec::new());
    }
    let profile = require_source_opening(state)?;
    let deck = state.deck_slots.get(viewer);
    let mut hints = Vec::new();
    for &slot in profile.display_slots {
        let card = deck.get(slot).ok_or_else(|| {
            EngineError::InvalidState("v7 opening card display slot missing".into())
        })?;
        if card.vacant || card.used || card.recovering {
            continue;
        }
        let Some(target) = card.extra.get("target").and_then(Value::as_str) else {
            continue;
        };
        if target.is_empty() {
            continue;
        }
        let mut targets = Vec::new();
        for row in 0..8 {
            for col in 0..8 {
                let square = Square { row, col };
                let Some(piece) = state.at(square) else {
                    continue;
                };
                if matches_target(&card.id, target, piece, viewer, state)?
                    && state.piece_visible(piece, square, viewer)
                {
                    targets.push(square);
                }
            }
        }
        hints.push(json!({"cardInstanceId":card.instance_id,"targets":targets}));
    }
    Ok(hints)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;

    fn first_play(style: &str) -> GameState {
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        let offers: &[usize] = match style {
            "normal" => &[1, 0],
            "chaos" => &[1, 2],
            "grand" => &[1, 3, 3, 3, 3, 3, 3, 3, 5, 5, 5, 5],
            _ => unreachable!(),
        };
        for &index in offers {
            let pick = crate::draft::legal_actions(&state).unwrap().remove(index);
            crate::draft::apply_pick(&mut state, &pick).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        assert_eq!(state.mode, "play");
        state
    }

    #[test]
    fn source_seed19_target_hints_preserve_display_order_and_empty_entries() {
        // Independent frozen-client report: report.json under the external
        // v7-public-hints slot. Source state digest/RNG are checked in the
        // helper before any squares are projected.
        let cases = [
            ("normal", json!([])),
            (
                "chaos",
                json!([{"cardInstanceId":"queens-gambit-5iuubljvcig", "targets":[{"row":7,"col":3}]}]),
            ),
            (
                "grand",
                json!([
                    {"cardInstanceId":"inertia-gej1io961xn", "targets":[
                        {"row":0,"col":0}, {"row":0,"col":2}, {"row":0,"col":3},
                        {"row":0,"col":5}, {"row":0,"col":7}]},
                    {"cardInstanceId":"miracle-myuyhn08lv", "targets":[]},
                    {"cardInstanceId":"trojan-horse-k1snppsr2ds", "targets":[
                        {"row":7,"col":1}, {"row":7,"col":6}]},
                    {"cardInstanceId":"grasshopper-jfly9nba6ej", "targets":[
                        {"row":7,"col":1}, {"row":7,"col":2},
                        {"row":7,"col":5}, {"row":7,"col":6}]},
                    {"cardInstanceId":"grappler-xvn13x9he", "targets":[{"row":7,"col":3}]},
                ]),
            ),
        ];
        for (style, expected) in cases {
            let state = first_play(style);
            let source = opening_for(style).unwrap();
            assert_eq!(
                state.rng.cursor, source.rng_cursor,
                "{style} source RNG cursor"
            );
            assert_eq!(
                state.rng.state, source.rng_state,
                "{style} source RNG state"
            );
            assert_eq!(
                json!(v7_seed19_first_play_card_target_hints(&state, Color::White).unwrap()),
                expected,
                "{style}"
            );
            assert_eq!(
                json!(v7_seed19_first_play_card_target_hints(&state, Color::Black).unwrap()),
                json!([]),
                "{style} non-acting viewer"
            );
        }
    }

    #[test]
    fn changed_state_or_pending_window_does_not_emit_verified_opening_targets() {
        let mut state = first_play("grand");
        state.rng.state ^= 1;
        assert!(matches!(
            v7_seed19_first_play_card_target_hints(&state, Color::White),
            Err(EngineError::UnsupportedFeature(_))
        ));
        state
            .extra
            .insert("pendingPromotion".into(), json!({"color":"white"}));
        assert_eq!(
            json!(v7_seed19_first_play_card_target_hints(&state, Color::White).unwrap()),
            json!([])
        );
    }

    #[test]
    fn recorded_final_draft_pick_preserves_the_same_chaos_hint_surface() {
        // The source report re-applies the final pick with recordHistory:true.
        // Its single public event is pinned separately from the play state.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: "chaos".into(),
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        let white_pick = crate::draft::legal_actions(&state).unwrap().remove(1);
        crate::draft::apply_pick(&mut state, &white_pick).unwrap();
        crate::replay::canonicalize_position_frames(&mut state).unwrap();
        let black_pick = crate::draft::legal_actions(&state).unwrap().remove(2);
        crate::transition::apply(&mut state, &black_pick).unwrap();
        assert_eq!(state.history.len(), 1);
        assert_eq!(
            json!(v7_seed19_first_play_card_target_hints(&state, Color::White).unwrap()),
            json!([{"cardInstanceId":"queens-gambit-5iuubljvcig", "targets":[{"row":7,"col":3}]}])
        );
    }
}
