use super::{card_revelation_for_ruleset, public_hints_v7, validate_projection_for_ruleset};
use crate::state::{
    Color, EngineError, GameConfig, GameState, ObservationPolicy, RULES_VERSION_V6,
    RULES_VERSION_V7, observation_policy_for_ruleset, observation_policy_hash_for_ruleset,
    observation_projection_for_ruleset,
};
use serde_json::json;
use sha2::{Digest, Sha256};

#[test]
fn pinned_observation_metadata_matches_both_js_contract_digests() {
    let versions: [(&str, &str, &str); 2] = [
        (
            RULES_VERSION_V6,
            "source-visible-20260927-v3",
            "5e17b5622f1e761d6e0719187006aaaac336150c4ae2372f76ef0080da0ab037",
        ),
        (
            RULES_VERSION_V7,
            "source-visible-20260928-v1",
            "baa57576dd60387813e87fc0dbfc095ce68c951fc51797031b22be62a419c270",
        ),
    ];
    for (ruleset, projection, hash) in versions {
        let _: &ObservationPolicy = observation_policy_for_ruleset(ruleset).unwrap();
        assert_eq!(
            observation_projection_for_ruleset(ruleset).unwrap(),
            projection
        );
        assert_eq!(observation_policy_hash_for_ruleset(ruleset).unwrap(), hash);
    }
    assert!(matches!(
        observation_policy_for_ruleset("unknown-ruleset"),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        observation_policy_hash_for_ruleset("unknown-ruleset"),
        Err(EngineError::UnsupportedFeature(_))
    ));
}

#[test]
fn box_revelation_uses_the_selected_catalog_and_card_lifecycle() {
    let card = serde_json::from_value(json!({
        "id": "black-box",
        "effect": "blackBox",
        "used": true,
        "boxRevealedCardId": "random-roulette"
    }))
    .unwrap();
    for version in [RULES_VERSION_V6, RULES_VERSION_V7] {
        assert_eq!(
            card_revelation_for_ruleset(&card, version).unwrap(),
            Some(json!({"boxCardId": "random-roulette"}))
        );
    }
    let mut unresolved = card.clone();
    unresolved.used = false;
    assert_eq!(
        card_revelation_for_ruleset(&unresolved, RULES_VERSION_V7).unwrap(),
        None
    );
    unresolved.used = true;
    unresolved
        .extra
        .insert("boxRevealedCardId".into(), json!("unknown-card"));
    assert_eq!(
        card_revelation_for_ruleset(&unresolved, RULES_VERSION_V7).unwrap(),
        None
    );
    assert!(matches!(
        card_revelation_for_ruleset(&card, "unknown-ruleset"),
        Err(EngineError::UnsupportedFeature(_))
    ));
}

#[test]
fn v7_empty_hint_windows_use_their_own_policy_and_unverified_play_stays_closed() {
    let mut state = GameState::new(GameConfig::default(), 41).unwrap();
    let v6 = state.try_observe(Color::White).unwrap();
    assert_eq!(
        v6.public_state["observationPolicyHash"],
        observation_policy_hash_for_ruleset(RULES_VERSION_V6).unwrap()
    );
    validate_projection_for_ruleset(&v6, RULES_VERSION_V6).unwrap();
    assert!(matches!(
        validate_projection_for_ruleset(&v6, RULES_VERSION_V7),
        Err(EngineError::InvalidState(_))
    ));
    state.ruleset_id = RULES_VERSION_V7.into();
    state
        .extra
        .insert("othelloPending".into(), json!({"white":true,"black":false}));
    let draft = state.try_observe(Color::White).unwrap();
    assert_eq!(
        draft.public_state["observationPolicyHash"],
        observation_policy_hash_for_ruleset(RULES_VERSION_V7).unwrap()
    );
    assert!(!draft.public_state.contains_key("othelloPending"));
    validate_projection_for_ruleset(&draft, RULES_VERSION_V7).unwrap();
    assert!(matches!(
        validate_projection_for_ruleset(&draft, RULES_VERSION_V6),
        Err(EngineError::InvalidState(_))
    ));
    assert!(std::panic::catch_unwind(|| state.observe(Color::White)).is_err());
    state.mode = "play".into();
    assert!(matches!(
        state.try_observe(Color::White),
        Err(EngineError::UnsupportedFeature(_))
    ));
    state.mode = "gameover".into();
    let terminal = state.try_observe(Color::White).unwrap();
    assert_eq!(
        terminal.public_state["legalHints"],
        json!({"moves":[],"cardTargets":[]})
    );
    validate_projection_for_ruleset(&terminal, RULES_VERSION_V7).unwrap();
    state.ruleset_id = "unknown-ruleset".into();
    assert!(matches!(
        state.try_observe(Color::White),
        Err(EngineError::UnsupportedFeature(_))
    ));
}

#[test]
fn v7_hints_reject_unverified_active_windows() {
    let mut state = GameState::new(GameConfig::default(), 41).unwrap();
    state.ruleset_id = RULES_VERSION_V7.into();
    let empty = json!({"moves":[],"cardTargets":[]});
    assert_eq!(state.mode, "draft");
    assert_eq!(public_hints_v7(&state, Color::White).unwrap(), empty);

    state.mode = "gameover".into();
    assert_eq!(public_hints_v7(&state, Color::White).unwrap(), empty);
    state.mode = "play".into();
    state.turn = Color::White;
    assert_eq!(public_hints_v7(&state, Color::Black).unwrap(), empty);

    state
        .extra
        .insert("pendingPromotion".into(), json!({"color":"white"}));
    assert_eq!(public_hints_v7(&state, Color::White).unwrap(), empty);
    state.extra.remove("pendingPromotion");
    state
        .extra
        .insert("activeTrolley".into(), json!({"color":"white"}));
    assert_eq!(public_hints_v7(&state, Color::White).unwrap(), empty);
    state.extra.remove("activeTrolley");
    assert!(matches!(
        public_hints_v7(&state, Color::White),
        Err(EngineError::UnsupportedFeature(_))
    ));
    state.ruleset_id = RULES_VERSION_V6.into();
    assert!(matches!(
        public_hints_v7(&state, Color::White),
        Err(EngineError::UnsupportedFeature(_))
    ));
}

#[test]
fn v7_seed19_normal_first_play_hints_match_source_ordered_destinations() {
    // The frozen v7 client shows 10 origins/20 destinations for the acting
    // white viewer after Relay and Ice Sheet are selected at seed 19. Relay
    // has no target hint; the non-acting viewer has no hints at all.
    let mut state =
        crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7).unwrap();
    let white_pick = crate::draft::legal_actions(&state).unwrap().remove(1);
    crate::draft::apply_pick(&mut state, &white_pick).unwrap();
    crate::replay::canonicalize_position_frames(&mut state).unwrap();
    let black_pick = crate::draft::legal_actions(&state).unwrap().remove(0);
    crate::transition::apply(&mut state, &black_pick).unwrap();
    assert_eq!(state.mode, "play");
    assert_eq!(state.deck_slots.white[0].id, "relay");
    assert_eq!(state.deck_slots.black[0].id, "ice-sheet");

    let observed = state.try_observe(Color::White).unwrap();
    let hints = &observed.public_state["legalHints"];
    assert_eq!(hints["cardTargets"], json!([]));
    let moves = hints["moves"].as_array().unwrap();
    assert_eq!(moves.len(), 10);
    let mut pairs = Vec::new();
    for move_hint in moves {
        let from = &move_hint["from"];
        for destination in move_hint["destinations"].as_array().unwrap() {
            pairs.push(format!(
                "{},{}->{},{}",
                from["row"], from["col"], destination["row"], destination["col"]
            ));
        }
    }
    assert_eq!(
        pairs,
        [
            "6,0->5,0", "6,0->4,0", "6,1->5,1", "6,1->4,1", "6,2->5,2", "6,2->4,2", "6,3->5,3",
            "6,3->4,3", "6,4->5,4", "6,4->4,4", "6,5->5,5", "6,5->4,5", "6,6->5,6", "6,6->4,6",
            "6,7->5,7", "6,7->4,7", "7,1->5,0", "7,1->5,2", "7,6->5,5", "7,6->5,7",
        ]
    );
    assert_eq!(
        state.try_observe(Color::Black).unwrap().public_state["legalHints"],
        json!({"moves":[], "cardTargets":[]})
    );
}

#[test]
fn v7_seed19_chaos_target_hints_match_source_ui_selection() {
    // Frozen v7 first-play source: one primary Queen's Gambit target, while
    // no-target Reposition has no hint entry. These are UI hints, not the
    // complete legal card action stream.
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
    crate::draft::apply_pick(&mut state, &black_pick).unwrap();
    crate::replay::canonicalize_position_frames(&mut state).unwrap();
    assert_eq!(state.mode, "play");
    assert_eq!(state.deck_slots.white[0].id, "queens-gambit");
    let hints = &state.try_observe(Color::White).unwrap().public_state["legalHints"];
    assert_eq!(hints["moves"].as_array().unwrap().len(), 10);
    assert_eq!(
        hints["cardTargets"],
        json!([{"cardInstanceId":"queens-gambit-5iuubljvcig", "targets":[{"row":7,"col":3}]}])
    );
    assert_eq!(
        state.try_observe(Color::Black).unwrap().public_state["legalHints"],
        json!({"moves":[], "cardTargets":[]})
    );
}

#[test]
fn v7_seed19_full_observations_match_frozen_source_for_both_viewers() {
    // These SHA-256/JCS digests and informationStateKeys come from
    // FrozenClientSource main-OahWs0tU.js at SHA-256
    // e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c.
    // Full source/native JCS comparison evidence stays outside Git at
    // %APPDATA%/Accelerate/reports/v7-public-hints/full-observation.
    type ViewerDigests = (&'static str, &'static str);
    type OpeningCase = (&'static str, &'static [usize], [ViewerDigests; 2]);
    let cases: [OpeningCase; 3] = [
        (
            "normal",
            &[1, 0],
            [
                (
                    "62ded9712b0349fa2b953996f3d9d4f00232b307195d044beefadf0774d105c6",
                    "f74153deb3b6032116013c1c6b368d6c84ae1db1f5b155b188fb6398f1fb0e11",
                ),
                (
                    "d111835859de1b0d1abfec30f1b5db997fa025d3355d05f5819d1e16146f4021",
                    "685cc64a74366c9b23c5554a10ff05b108a3de613ccf6fb10f33dc225f38ef42",
                ),
            ],
        ),
        (
            "chaos",
            &[1, 2],
            [
                (
                    "4676e57945b2461223d381152b3bc989daad083f65af67940656eede92d1d8e7",
                    "411425556177b08f6993d2d3ce7eb3e7065a3d3d7d0fbe0372ce82aae270d476",
                ),
                (
                    "b770867a169b1bf6829b5b7c03db982cabff74b214a7b8ffce0fdc9ae4d654d7",
                    "cc7ce0a29e338bb702f5cac421cf2d13b6916fe3d787761df5fc160cc74d5afc",
                ),
            ],
        ),
        (
            "grand",
            &[1, 3, 3, 3, 3, 3, 3, 3, 5, 5, 5, 5],
            [
                (
                    "a56350e04aefaf95ad597782f58c5ec7494a228d6c4b5ff309c17fd5d52dc094",
                    "0fa0a889035dd380832e014dadcc9fd0b4773e03d4f989dc831cc789004e03a3",
                ),
                (
                    "d9a0cadb3b8121f613be74890405de6adac2399b268936a29816e80b7e61e600",
                    "70fce168ad8d16685708d99215b45fc26faceddcf7be79565249f5162abc444a",
                ),
            ],
        ),
    ];
    for (style, offers, expected) in cases {
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        for &index in offers {
            let pick = crate::draft::legal_actions(&state).unwrap().remove(index);
            crate::draft::apply_pick(&mut state, &pick).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        for (viewer, (full_digest, information_key)) in
            [Color::White, Color::Black].into_iter().zip(expected)
        {
            let observation = state.try_observe(viewer).unwrap();
            let canonical = serde_jcs::to_vec(&observation).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(canonical)),
                full_digest,
                "{style} {} full Observation v2",
                viewer.as_str()
            );
            assert_eq!(
                observation.information_state_key,
                information_key,
                "{style} {} informationStateKey",
                viewer.as_str()
            );
        }
    }
}
