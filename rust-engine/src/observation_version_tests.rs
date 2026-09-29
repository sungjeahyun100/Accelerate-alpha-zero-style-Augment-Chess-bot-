use super::{card_revelation_for_ruleset, public_hints_v7, validate_projection_for_ruleset};
use crate::state::{
    Color, EngineError, GameConfig, GameState, ObservationPolicy, RULES_VERSION_V6,
    RULES_VERSION_V7, observation_policy_for_ruleset, observation_policy_hash_for_ruleset,
    observation_projection_for_ruleset,
};
use serde_json::json;

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
fn v7_empty_hint_windows_use_their_own_policy_and_active_play_stays_closed() {
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
fn v7_hints_only_open_the_source_empty_windows() {
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
