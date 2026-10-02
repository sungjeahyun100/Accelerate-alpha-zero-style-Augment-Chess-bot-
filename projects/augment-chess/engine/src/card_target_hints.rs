//! Source-visible card target squares for frozen v7 play states.
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
            state_digest: "8ea26cf766f9fee2e04804f0cea78c07828d6749e14873578be3ab3848b868f8",
            history_entry_digest: "8ae6ee2d3e189a13605f5a00d1982fc27f0762da9f44c98256511b8000ef6785",
            rng_cursor: 292,
            rng_state: 596_976_663,
            display_slots: &[0, 1, 2],
        },
        "chaos" => SourceOpening {
            state_digest: "3b8fd9fae0404a6032c8f5daa7bbdecc03cac93730355f13ebb6d97a2f290793",
            history_entry_digest: "ee17295ce7b6630b675e41b739a32b36b3b262def49e8480456d9e61af2278ef",
            rng_cursor: 400,
            rng_state: 185_085_603,
            // Frozen deckSlotsForDisplay transposes the three chaos pairs.
            display_slots: &[0, 2, 4, 1, 3, 5],
        },
        "grand" => SourceOpening {
            state_digest: "17f03e0add054f3b231f4f3943c78dcd358ed18f815ab368b9d1879869072075",
            history_entry_digest: "512035c7c2d42c5dab0c73f6122218bdb22668a5f9ae6e507973eda2d8c8d7bc",
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

/// Project the first-click card targets in the frozen client's visible deck
/// order. This is deliberately separate from `card_effects::actions`: a card
/// can have an empty hint and still have an effect-specific action protocol.
/// An unimplemented target predicate is an error, never an empty hint.
pub(crate) fn public_card_target_hints_v7(state: &GameState, viewer: Color) -> Result<Vec<Value>> {
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
        || state.extra.get("draftDelete") == Some(&Value::Bool(true))
    {
        return Ok(Vec::new());
    }

    // The independently recorded seed-19 openings include UI predicates
    // whose execution objects are still being migrated. Preserve their
    // complete source result while the general path expands card coverage.
    if state
        .extra
        .get("gameStyle")
        .and_then(Value::as_str)
        .is_some_and(|style| opening_for(style).is_ok())
        && require_source_opening(state).is_ok()
    {
        return v7_seed19_first_play_card_target_hints(state, viewer);
    }

    let deck = state.deck_slots.get(viewer);
    let chaos = state.extra.get("gameStyle").and_then(Value::as_str) == Some("chaos");
    let display_slots: Vec<usize> = if chaos && deck.len() == 6 {
        vec![0, 2, 4, 1, 3, 5]
    } else {
        (0..deck.len()).collect()
    };
    let mut hints = Vec::new();
    let mut fog = None;
    for slot in display_slots {
        let card = &deck[slot];
        if card.vacant
            || card.used
            || card.recovering
            || !crate::observation::truth(card.extra.get("target"))
        {
            continue;
        }
        if fog.is_none() {
            // getVisibleCardTargetSquares masks empty destination cells as
            // well as hidden pieces. The shared source visibility owner
            // computes one immutable fog probe for this board and viewer.
            fog = Some(crate::observation::fog_visible_squares_v7(state, viewer)?);
        }
        let visible_fog = fog.as_ref().and_then(Option::as_ref);
        let targets = crate::card_effects::target_squares(state, card)?.ok_or_else(|| {
            EngineError::UnsupportedFeature(format!(
                "v7 UI target predicate for card {} ({})",
                card.id, card.effect
            ))
        })?;
        let mut visible = Vec::with_capacity(targets.len());
        for square in targets {
            if visible_fog.is_some_and(|fog| !fog.contains(&square)) {
                continue;
            }
            if let Some(piece) = state.at(square)
                && !crate::observation::piece_visible_to_color_at_v7_with_fog(
                    state,
                    piece,
                    square,
                    viewer,
                    visible_fog,
                )?
            {
                continue;
            }
            visible.push(square);
        }
        hints.push(json!({"cardInstanceId": card.instance_id, "targets": visible}));
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
    fn general_card_hints_keep_source_display_order_after_choice_ownership() {
        let mut chaos = first_play("chaos");
        chaos.rng.state ^= 1; // bypass the exact-state observation shortcut
        assert_eq!(
            json!(public_card_target_hints_v7(&chaos, Color::White).unwrap()),
            json!([{"cardInstanceId":"queens-gambit-5iuubljvcig", "targets":[{"row":7,"col":3}]}])
        );

        let mut grand = first_play("grand");
        grand.rng.state ^= 1;
        assert_eq!(
            json!(public_card_target_hints_v7(&grand, Color::White).unwrap()),
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
            ])
        );
    }

    #[test]
    fn fog_hints_hide_empty_portal_cells_and_keep_pending_card_targets() {
        // main105844-105850 masks every first-click square with fog before
        // examining its occupant. The far empty corner is a valid portal
        // reservation, yet only this king's five nearby cells are visible.
        let mut state = GameState::new(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            7,
        )
        .unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.extra.insert("draftDelete".into(), json!(false));
        state
            .extra
            .insert("campaign".into(), json!({"setup":"fogWar"}));
        state.board = vec![vec![None; 8]; 8];
        state.board[7][4] = Some(Piece::new("king", Color::White, "white-king"));
        state.board[0][4] = Some(Piece::new("king", Color::Black, "black-king"));
        let definition =
            crate::card_registry::definition_for(RULES_VERSION_V7, "portal-gun").unwrap();
        let mut card: crate::CardSlot =
            serde_json::from_value(definition.source_definition.clone()).unwrap();
        card.instance_id = "fog-portal-card".into();
        // UI hint projection does not use the raw collector's pending gate.
        // Source getVisibleCards retains the pending instance's target hints.
        card.extra.insert("nextTurnPending".into(), json!(true));
        state.deck_slots.white[0] = card.clone();
        assert!(
            crate::card_effects::target_squares(&state, &card)
                .unwrap()
                .unwrap()
                .contains(&Square { row: 0, col: 0 })
        );
        assert!(!crate::card_registry::source_candidate_available(&state, &card).unwrap());
        let before = state.clone();
        assert_eq!(
            json!(public_card_target_hints_v7(&state, Color::White).unwrap()),
            json!([{"cardInstanceId":"fog-portal-card","targets":[
                {"row":6,"col":3}, {"row":6,"col":4}, {"row":6,"col":5},
                {"row":7,"col":3}, {"row":7,"col":5}
            ]}])
        );
        assert_eq!(state, before);
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

    #[test]
    fn seed37_actual_draft_card_targets_match_frozen_source_order_and_empty_surface() {
        // Independently recorded from the frozen v7 GameAdapter publicHints
        // after selecting the first source offer at each draft decision.
        // %APPDATA%/Accelerate/reports/v7-card-targets-seed37.json. The source
        // state digest prevents a plausible target list on a different board
        // or hand from passing this comparison.
        for (style, source_state, expected) in [
            (
                "normal",
                "2e26a55ba57595745330caf14cf497fbfa724404b2e5b015f3553f54a6f66237",
                json!([{"cardInstanceId":"nullification-6aoq5pfkhih",
                    "targets":(6..=7).flat_map(|row| (0..8).map(move |col| json!({"row":row,"col":col}))).collect::<Vec<_>>() }]),
            ),
            (
                "chaos",
                "a6a96f58c84923353e4f32a8170526d73104b4db3c9af11ed26a47878adc3ac1",
                json!([]),
            ),
            (
                "grand",
                "94cd6723db21c6e1911e94dbe4587f152d168e44d4f948185c5d85a78b35b389",
                json!([
                    {"cardInstanceId":"random-roulette-cdiswtbmz2","targets":[
                        {"row":0,"col":0},{"row":0,"col":3},{"row":0,"col":7},
                        {"row":7,"col":0},{"row":7,"col":3},{"row":7,"col":7}]},
                    {"cardInstanceId":"feudal-contract-95egdth063n","targets":
                        (0..8).map(|col| json!({"row":6,"col":col})).collect::<Vec<_>>()},
                    {"cardInstanceId":"panic-4oibaqnxzz8","targets":
                        (0..=1).flat_map(|row| (0..8)
                            .filter(move |&col| row != 0 || col != 4)
                            .map(move |col| json!({"row":row,"col":col})))
                            .collect::<Vec<_>>()},
                ]),
            ),
        ] {
            let mut state = crate::draft::initialize_for_ruleset(
                GameConfig {
                    game_style: style.into(),
                    ..GameConfig::default()
                },
                37,
                RULES_VERSION_V7,
            )
            .unwrap();
            for _ in 0..if style == "grand" { 12 } else { 2 } {
                let action = crate::draft::legal_actions(&state).unwrap().remove(0);
                crate::draft::apply_pick(&mut state, &action).unwrap();
                crate::replay::canonicalize_position_frames(&mut state).unwrap();
            }
            assert_eq!(state.mode, "play", "{style}");
            let mut raw = serde_json::to_value(&state).unwrap();
            for field in ["rulesetId", "rng", "history"] {
                raw.as_object_mut().unwrap().remove(field);
            }
            assert_eq!(
                source_digest(&raw).unwrap(),
                source_state,
                "{style} source state"
            );
            assert_eq!(
                json!(public_card_target_hints_v7(&state, Color::White).unwrap()),
                expected,
                "{style} ordered source targets"
            );
            assert_eq!(
                json!(public_card_target_hints_v7(&state, Color::Black).unwrap()),
                json!([]),
                "{style} non-acting viewer"
            );
        }
    }
}
