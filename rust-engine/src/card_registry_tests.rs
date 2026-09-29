use super::*;
use crate::GameConfig;
use serde_json::json;

fn state() -> GameState {
    let mut state = GameState::new(GameConfig::default(), 7).expect("v6 fixture");
    state.ruleset_id = RULES_VERSION_V7.into();
    state
}

fn hand_card(state: &mut GameState, id: &str, color: Color) -> CardSlot {
    let definition = definition_for(RULES_VERSION_V7, id).expect("definition");
    let mut card: CardSlot =
        serde_json::from_value(definition.source_definition.clone()).expect("source CardSlot");
    card.instance_id = format!("test-{id}");
    card.extra.insert("slot".into(), json!(0));
    state.deck_slots.get_mut(color)[0] = card.clone();
    card
}

#[test]
fn source_pinned_registry_covers_public_and_auxiliary_definitions() {
    for rules_version in [RULES_VERSION_V6, RULES_VERSION_V7] {
        let registry = registry_for(rules_version).expect("frozen catalog");
        assert_eq!(registry.cards.len(), 257);
        assert_eq!(
            registry
                .cards
                .values()
                .filter(|card| card.card_type.is_some())
                .count(),
            256
        );
        assert_eq!(
            registry
                .cards
                .values()
                .filter(|card| card.card_type.is_none())
                .count(),
            1
        );
        assert_eq!(registry.get("shotgun-king").unwrap().card_type, None);
        assert_eq!(
            registry
                .cards
                .values()
                .filter(|card| card.activation == Some(CardActType::Passive))
                .count(),
            67
        );
        assert_eq!(
            registry
                .cards
                .values()
                .filter(|card| card.activation == Some(CardActType::Active))
                .count(),
            189
        );
        assert_eq!(
            registry
                .cards
                .values()
                .filter(|card| card.turn_policy == CardTurnPolicy::EndTurn)
                .count(),
            4
        );
    }
    assert_eq!(
        crate::draft::definitions_for_ruleset(RULES_VERSION_V7)
            .unwrap()
            .definitions
            .len(),
        257
    );
    assert_eq!(
        crate::draft::draft_weight_for_ruleset(RULES_VERSION_V7, "guard", false)
            .unwrap()
            .0,
        "OPENING"
    );
}

#[test]
fn unpinned_draft_source_is_rejected_before_pool_selection() {
    assert!(matches!(
        crate::draft::definitions_for_ruleset("unknown"),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        crate::draft::draft_weight_for_ruleset("unknown", "guard", false),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert_eq!(
        crate::draft::category_with_ruleset("unknown", &json!({"id":"guard"})),
        ""
    );
    assert!(crate::draft::conflicts_with_ruleset(
        "unknown",
        "guard",
        &std::collections::BTreeSet::new()
    ));
}

#[test]
fn v7_initialization_selects_pinned_source_and_clock() {
    let v6_time = crate::draft::frozen_timestamp_for_ruleset(RULES_VERSION_V6).unwrap();
    let v7_time = crate::draft::frozen_timestamp_for_ruleset(RULES_VERSION_V7).unwrap();
    assert!(v7_time > v6_time);
    // Independent frozen v7 OracleRuntime.newGame({gameStyle}, 19) observations.
    for (style, expected_ids, expected_cursor, expected_rng_state) in [
        (
            "normal",
            vec!["pawn-conversion", "relay", "queens-gambit"],
            122,
            1316640757,
        ),
        (
            "chaos",
            vec![
                "pawn-conversion",
                "relay",
                "queens-gambit",
                "reposition",
                "princess",
                "gale",
            ],
            212,
            39465415,
        ),
        (
            "grand",
            vec![
                "fast-growth",
                "princess",
                "apprentice-knights",
                "initiative",
                "inertia",
                "suicide-bomber",
                "miracle",
                "suspicious-potion",
                "taunt",
                "symmetry",
                "trojan-horse",
                "ghost",
                "exhaustion",
                "chimera",
                "grasshopper",
                "missionary",
                "grappler",
                "constitutional-monarchy",
                "dragon",
                "don-quixote",
                "slime",
                "exile",
                "hallucination",
                "empty-lunchbox",
                "baby-bear",
                "substitution",
                "underpromotion",
                "traitor",
            ],
            112,
            1021441923,
        ),
    ] {
        let state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        assert_eq!(state.ruleset_id, RULES_VERSION_V7);
        assert_eq!(state.mode, "draft");
        let draft = &state.extra["draft"];
        let color = draft["color"].as_str().unwrap();
        assert_eq!(
            state.extra["draftClock"][format!("{color}StartedAt")],
            json!(v7_time)
        );
        assert_eq!(
            draft["choices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|choice| choice["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected_ids,
            "{style} source offer order"
        );
        assert_eq!(state.rng.cursor, expected_cursor, "{style} RNG cursor");
        assert_eq!(state.rng.state, expected_rng_state, "{style} RNG state");
    }
}

#[test]
fn v7_first_regular_pick_keeps_source_offer_and_rng_order() {
    // Frozen v7 oracle: normal seed 19, choose the second offered card (relay).
    let mut state =
        crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7).unwrap();
    let action = crate::draft::legal_actions(&state).unwrap().remove(1);
    crate::draft::apply_pick(&mut state, &action).unwrap();
    assert_eq!(state.mode, "draft");
    assert_eq!(state.turn, Color::Black);
    assert_eq!(state.deck_slots.white[0].id, "relay");
    assert_eq!(state.extra["draft"]["color"], "black");
    assert_eq!(
        state.extra["draft"]["choices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|choice| choice["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["ice-sheet", "quantum-mechanics", "disassembly"]
    );
    assert_eq!(state.rng.cursor, 214);
    assert_eq!(state.rng.state, 54954385);
}

#[test]
fn manual_policy_is_owned_by_definition_and_rejects_invalid_instances() {
    let mut state = state();
    state.turn = Color::White;
    let brainwash = hand_card(&mut state, "brainwash", Color::White);
    assert_eq!(
        action_policy(&state, &brainwash).unwrap(),
        CardActionPolicy {
            actor: Color::White,
            slot: 0,
            activation: CardActType::Active,
            use_cost: CardUseCost::SpendInstance,
            turn_policy: CardTurnPolicy::EndTurn,
        }
    );
    let mut developer_card = brainwash.clone();
    developer_card.extra.insert("devCard".into(), json!(true));
    state.deck_slots.white[0] = developer_card.clone();
    assert_eq!(
        action_policy(&state, &developer_card).unwrap().use_cost,
        CardUseCost::Exempt
    );
    state.deck_slots.white[0] = brainwash.clone();
    let mut forged = brainwash.clone();
    forged.effect = "guard".into();
    assert!(matches!(
        validate_instance(&state, &forged),
        Err(EngineError::InvalidState(_))
    ));
    let mut used = brainwash.clone();
    used.used = true;
    assert_eq!(
        action_policy(&state, &used).unwrap_err(),
        EngineError::IllegalAction
    );
    state.turn = Color::Black;
    assert_eq!(
        action_policy(&state, &brainwash).unwrap_err(),
        EngineError::IllegalAction
    );
    state.turn = Color::White;
    state.deck_slots.white[0].used = true;
    assert_eq!(
        action_policy(&state, &brainwash).unwrap_err(),
        EngineError::IllegalAction
    );
    state.deck_slots.white[0].used = false;
    let passive = hand_card(&mut state, "apprentice-knights", Color::White);
    assert_eq!(
        action_policy(&state, &passive).unwrap_err(),
        EngineError::IllegalAction
    );
}

#[test]
fn local_effect_handler_checks_v7_definition_before_applying() {
    let mut state = state();
    let card = hand_card(&mut state, "guard", Color::White);
    let mut forged = card.clone();
    forged.effect = "portalGun".into();
    let action = crate::Action::card(Color::White, &forged, None);
    assert!(matches!(
        crate::card_effects::validate(&state, &forged, &action),
        Err(EngineError::InvalidState(_))
    ));
}

#[test]
fn v7_othello_card_uses_no_target_and_sets_the_resolution_latch() {
    let mut state = GameState::new(
        GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        },
        7,
    )
    .unwrap();
    state.ruleset_id = RULES_VERSION_V7.into();
    let card = hand_card(&mut state, "othello", Color::White);
    let actions = crate::card_effects::actions(&state, &card)
        .unwrap()
        .unwrap();
    assert_eq!(
        actions,
        vec![crate::Action::card(Color::White, &card, None)]
    );
    let mut invalid = actions[0].clone();
    invalid.target = Some(json!({"row":4,"col":4}));
    assert_eq!(
        crate::card_effects::validate(&state, &card, &invalid).unwrap(),
        Some(false)
    );
    let original_rng = state.rng.clone();
    assert!(
        crate::card_effects::apply(&mut state, &card, &actions[0])
            .unwrap()
            .is_some()
    );
    assert_eq!(state.extra["othelloPending"]["white"], true);
    assert_eq!(state.rng, original_rng);
}

#[test]
fn live_settlement_branch_distinguishes_acceleration_cleanup() {
    for (id, end_move, clear_extra_actions) in [
        ("summon-colossus", true, false),
        ("zugzwang", true, true),
        ("miracle", true, true),
        ("brainwash", true, true),
        ("trolley", true, true),
        ("premove", true, true),
        ("guard", false, false),
    ] {
        let mut state = state();
        state.mode = "play".into();
        let card = hand_card(&mut state, id, Color::White);
        assert_eq!(
            settle_policy(&state, &card).unwrap(),
            CardSettlePolicy {
                end_move,
                clear_extra_actions
            },
            "{id}"
        );
    }
}

#[test]
fn zugzwang_settlement_follows_profile_or_fallback_and_editor_mode() {
    let mut state = state();
    state.mode = "play".into();
    let card = hand_card(&mut state, "zugzwang", Color::White);
    state
        .extra
        .insert("zugzwangConsumesTurn".into(), json!(false));
    assert!(!settle_policy(&state, &card).unwrap().end_move);
    state
        .extra
        .insert("profile".into(), json!({"catalogHash":CATALOG_VERSION}));
    assert!(settle_policy(&state, &card).unwrap().end_move);
    state.extra.insert(
        "cardState".into(),
        json!({"profile":{"catalogHash":"unknown"}}),
    );
    assert!(!settle_policy(&state, &card).unwrap().end_move);
    state.extra.insert("cardState".into(), Value::Null);
    state
        .extra
        .insert("simpleBoardEditorCardOverride".into(), json!({}));
    assert!(!settle_policy(&state, &card).unwrap().end_move);
    state
        .extra
        .insert("simpleBoardEditorCardOverride".into(), Value::Null);
    state.mode = "gameover".into();
    assert!(!settle_policy(&state, &card).unwrap().end_move);
}

#[test]
fn black_box_revealing_zugzwang_uses_the_same_dynamic_branch() {
    let mut state = state();
    state.mode = "play".into();
    let mut card = hand_card(&mut state, "black-box", Color::White);
    card.extra
        .insert("boxRevealedCardId".into(), json!("zugzwang"));
    state.deck_slots.white[0] = card.clone();
    assert_eq!(
        settle_policy(&state, &card).unwrap(),
        CardSettlePolicy {
            end_move: true,
            clear_extra_actions: true
        }
    );
}

#[test]
fn forced_first_move_is_an_explicit_activation_context() {
    let mut state = state();
    let mut guard = hand_card(&mut state, "guard", Color::White);
    guard.extra.insert("firstTurnCard".into(), json!(true));
    state.deck_slots.white[0] = guard.clone();
    assert_eq!(
        forced_first_move_policy(&state, &guard, Color::White).unwrap(),
        CardActionPolicy {
            actor: Color::White,
            slot: 0,
            activation: CardActType::ActiveForced,
            use_cost: CardUseCost::SpendInstance,
            turn_policy: CardTurnPolicy::NotApplicable,
        }
    );
    state.set_flag("firstMoveCardsForced", Color::White, true);
    assert_eq!(
        forced_first_move_policy(&state, &guard, Color::White).unwrap_err(),
        EngineError::IllegalAction
    );
}

#[test]
fn active_rules_are_distinct_from_hand_cards_and_keep_source_order() {
    let mut state = state();
    hand_card(&mut state, "guard", Color::White);
    state.extra.insert(
        "appliedRuleCard".into(),
        json!({"id":"acceleration","effect":"acceleration"}),
    );
    state.extra.insert(
        "additionalRuleCards".into(),
        json!([
            {"id":"platform","effect":"platformRule"},
            {"id":"revelation","effect":"revelation"}
        ]),
    );
    let typed = CardState::from_legacy(&state).unwrap();
    assert_eq!(typed.hands.white.len(), 1);
    assert_eq!(
        typed
            .active_rules
            .iter()
            .map(|rule| rule.definition_id.as_str())
            .collect::<Vec<_>>(),
        ["acceleration", "platform", "revelation"]
    );
    state.extra.insert(
        "additionalRuleCards".into(),
        json!([{"id":"guard","effect":"guard"}]),
    );
    assert!(matches!(
        CardState::from_legacy(&state),
        Err(EngineError::InvalidState(_))
    ));
}
