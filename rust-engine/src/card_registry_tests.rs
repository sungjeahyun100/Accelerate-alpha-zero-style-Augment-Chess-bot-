use super::*;
use crate::{Action, GameConfig};
use serde_json::json;
use sha2::{Digest, Sha256};

fn source_state_digest(state: &GameState) -> String {
    let mut value = serde_json::to_value(state).unwrap();
    for field in ["rulesetId", "rng", "history"] {
        value.as_object_mut().unwrap().remove(field);
    }
    format!("{:x}", Sha256::digest(serde_jcs::to_vec(&value).unwrap()))
}

fn event_digest(event: &Value) -> String {
    format!("{:x}", Sha256::digest(serde_jcs::to_vec(event).unwrap()))
}

fn seed19_grand_active_only() -> GameState {
    let mut state = crate::draft::initialize_for_ruleset(
        GameConfig {
            game_style: "grand".into(),
            ..GameConfig::default()
        },
        19,
        RULES_VERSION_V7,
    )
    .unwrap();
    for (offer_index, card_id) in [
        (1, "princess"),
        (3, "inertia"),
        (3, "suicide-bomber"),
        (3, "miracle"),
        (3, "suspicious-potion"),
        (3, "taunt"),
        (3, "symmetry"),
        (3, "trojan-horse"),
        (5, "chimera"),
        (5, "grasshopper"),
        (5, "missionary"),
        (5, "grappler"),
    ] {
        let action = crate::draft::legal_actions(&state)
            .unwrap()
            .remove(offer_index);
        let chosen = state.extra["draft"]["choices"]
            .as_array()
            .unwrap()
            .iter()
            .find(|card| card["instanceId"].as_str() == action.card_instance_id.as_deref())
            .unwrap();
        assert_eq!(chosen["id"], card_id);
        crate::draft::apply_pick(&mut state, &action).unwrap();
        crate::replay::canonicalize_position_frames(&mut state).unwrap();
    }
    assert_eq!(state.mode, "play");
    assert_eq!(state.rng.cursor, 124);
    assert_eq!(state.rng.state, 1313359343);
    assert_eq!(
        source_state_digest(&state),
        "6ccd5c79e3de0da607f36542da8d8640c8bb6b5892638e12c5b895e0cd5788ac"
    );
    state
}

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
    let relay = definition_for(RULES_VERSION_V7, "relay").unwrap();
    assert_eq!(relay.source_definition["name"], "교대");
    assert_eq!(relay.source_definition["art"], "relay");
    assert_eq!(relay.source_definition["stars"], json!(2.5));
    assert_eq!(
        crate::draft::definitions_for_ruleset(RULES_VERSION_V7)
            .unwrap()
            .definitions
            .iter()
            .find(|card| card["id"] == "relay")
            .unwrap()["text"],
        "이번 턴 동안 아군 기물이 턴을 소모하여 같은 행 또는 열의 아군과 위치를 바꿀 수 있습니다."
    );
}

#[test]
fn v7_presentation_catalog_mismatch_fails_closed() {
    let mut presentation: Value = serde_json::from_str(include_str!(
        "../../bridge/catalog/card-presentation-20260928.json"
    ))
    .unwrap();
    presentation["definitions"][0]["stars"] = json!(99);
    let altered = serde_json::to_string(&presentation).unwrap();
    assert!(matches!(
        CardRegistry::load(
            RULES_VERSION_V7,
            V7_MAIN,
            include_str!("../../bridge/catalog/site-20260928.json"),
            include_str!("../../bridge/catalog/card-definitions-20260928.json"),
            include_str!("../../bridge/catalog/draft-20260928.json"),
            Some(&altered),
        ),
        Err(EngineError::InvalidState(_))
    ));
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
        assert_eq!(state.extra["replayStartedAt"], "2026-09-28T07:41:32.828Z");
        assert_eq!(
            state.extra["replayBaseFrame"]["replayStartedAt"],
            state.extra["replayStartedAt"]
        );
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
fn v7_seed37_first_draft_pick_keeps_full_source_state_and_public_event() {
    // SHA-pinned OracleRuntime seed 37, with recordHistory=true on the first
    // offered choice. Digests cover all 259 source state fields and both
    // public viewer transitions without storing full Position fixtures.
    for (style, before_digest, after_digest, event_hash, legal_hash, count, cursor, rng_state) in [
        (
            "normal",
            "e94f40dbfbb62a33c2ca79c5bc319a89b754b7ef7506b622dfba554048803f91",
            "3b5f2a42edda10b391d8da1e1db617dadf02d9e53d9464960b79c280ed3760e2",
            "adb503b71ffbeb84a1080d9fc386856857bae9a4feb5a26bd6c24e9a8bde4765",
            "ba5484f8cb72a238b8297d51e6ae2821a69903b863f7cb2bf9b3c0cf4c28692e",
            3,
            214,
            1029675283,
        ),
        (
            "chaos",
            "d57318fa79672c909fb617a6b6af06e5fbf95d802199d1e5b4987800cc12864f",
            "df75d1ded6cc7fb93dfd4085ac38bad2ff76fda666ff3a38cc0e0774c6d59e4b",
            "52769b28d85171e9b6b073e2c451de68f8dcfae691c5cee28e9ec27a96832bb8",
            "fd46b3f4dcf46aa0138d6b3775fdf4cf90d1bc3076fd0e9957b94e44b36b47be",
            3,
            396,
            1564888433,
        ),
        (
            "grand",
            "6733d15f23974181b080684424dadf77dd4639b62b864e7aef64e18254c7a8ff",
            "c0208b8a77a2d20ecfbebc6afd822220ad67637ac427c2056831b9f0a395e19e",
            "97ad964e67aa761da85692e20e27d1a860ccee21b3f2f3d68a112ec8325882fe",
            "de88107652ba7001e3f4f8ec259fce8d6c4a29840a156c066795af0e8753115f",
            28,
            113,
            2091791728,
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
        assert_eq!(
            source_state_digest(&state),
            before_digest,
            "{style} initial"
        );
        let actions = crate::draft::legal_actions(&state).unwrap();
        assert_eq!(actions.len(), count, "{style} ordered draft candidates");
        assert_eq!(
            format!("{:x}", Sha256::digest(serde_jcs::to_vec(&actions).unwrap())),
            legal_hash,
            "{style} complete source candidate order"
        );
        crate::transition::apply(&mut state, &actions[0]).unwrap();
        crate::replay::canonicalize_position_frames(&mut state).unwrap();
        assert_eq!(
            source_state_digest(&state),
            after_digest,
            "{style} first pick"
        );
        assert_eq!(state.rng.cursor, cursor, "{style} RNG cursor");
        assert_eq!(state.rng.state, rng_state, "{style} RNG state");
        assert_eq!(state.history.len(), 1, "{style} public event count");
        assert_eq!(event_digest(&state.history[0]), event_hash, "{style} event");
    }
}

#[test]
fn v7_seed19_grand_twelve_active_picks_reach_source_first_play() {
    let state = seed19_grand_active_only();
    assert_eq!(state.turn, Color::White);
    assert_eq!(state.deck_slots.white.len(), 6);
    assert_eq!(state.deck_slots.black.len(), 6);
}

#[test]
fn v7_grand_first_play_taunt_matches_source_state_and_keeps_v6_fallback() {
    let state = seed19_grand_active_only();
    let card = state
        .deck_slots
        .white
        .iter()
        .find(|card| card.id == "taunt")
        .unwrap()
        .clone();
    assert_eq!(
        action_policy(&state, &card).unwrap(),
        CardActionPolicy {
            actor: Color::White,
            slot: 2,
            activation: CardActType::Active,
            use_cost: CardUseCost::SpendInstance,
            turn_policy: CardTurnPolicy::PreserveTurn,
        }
    );
    let action = Action::card(Color::White, &card, None);
    assert_eq!(
        crate::card_effects::actions(&state, &card).unwrap(),
        Some(vec![action.clone()])
    );
    let mut effect_only = state.clone();
    assert!(
        crate::card_effects::apply(&mut effect_only, &card, &action)
            .unwrap()
            .unwrap()
            .is_empty()
    );
    assert_eq!(effect_only.extra["taunt"], json!({"white":0,"black":1}));
    assert_eq!(effect_only.rng, state.rng);

    let mut full_transition = state;
    crate::transition::apply(&mut full_transition, &action).unwrap();
    assert_eq!(full_transition.extra["taunt"], json!({"white":0,"black":1}));
    assert_eq!(full_transition.rng.cursor, 125);
    assert_eq!(full_transition.rng.state, 3595483778);
    assert_eq!(
        source_state_digest(&full_transition),
        "493580e9470438d5ce361b31d43c9e61c96d7644619e0c4ac95d8dabe68bc646"
    );
    assert_eq!(full_transition.history.len(), 1);
    assert_eq!(
        event_digest(&full_transition.history[0]),
        "bad19539ef31e8256bf2bd9bd51214cbb97137e74b72e45c871abb650d860142"
    );

    let mut v6 = GameState::new(GameConfig::default(), 7).unwrap();
    let original = v6.clone();
    assert_eq!(crate::card_effects::actions(&v6, &card).unwrap(), None);
    assert_eq!(
        crate::card_effects::apply(&mut v6, &card, &action).unwrap(),
        None
    );
    assert_eq!(v6, original);
}

#[test]
fn v7_selected_opening_rule_preserves_source_event_rng_and_draft_order() {
    // Independent SHA-pinned OracleRuntime.newGame({ruleCardIds}, 19) results.
    for (style, expected_cursor, expected_rng_state, expected_offer) in [
        (
            "normal",
            125,
            3595483778_u32,
            vec!["holdout", "imperial-studies", "loyalist"],
        ),
        (
            "chaos",
            215,
            4043093436_u32,
            vec![
                "holdout",
                "imperial-studies",
                "loyalist",
                "scarecrow",
                "evasion",
                "e4",
            ],
        ),
        (
            "grand",
            115,
            911746088_u32,
            vec!["merchant-guild", "false-start", "calling-card"],
        ),
    ] {
        let state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: style.into(),
                rule_card_ids: vec!["saturation".into()],
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        assert_eq!(state.rng.cursor, expected_cursor, "{style} RULE RNG cursor");
        assert_eq!(
            state.rng.state, expected_rng_state,
            "{style} RULE RNG state"
        );
        assert_eq!(
            state.extra["draft"]["choices"]
                .as_array()
                .unwrap()
                .iter()
                .take(expected_offer.len())
                .map(|card| card["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected_offer,
            "{style} RULE first offer"
        );
        assert_eq!(state.extra["saturationRule"], true);
        assert_eq!(state.extra["ruleSelectionEnabled"], true);
        assert_eq!(state.extra["selectedRuleCardIds"], json!(["saturation"]));
        assert_eq!(state.extra["selectedRuleCardId"], "saturation");
        assert_eq!(state.extra["replayBaseFrame"]["ruleSelectionEnabled"], true);
        assert_eq!(
            state.extra["replayBaseFrame"]["selectedRuleCardIds"],
            json!(["saturation"])
        );
        assert_eq!(
            state.extra["replayBaseFrame"]["appliedRuleCard"],
            Value::Null
        );
        assert_eq!(state.extra["replayEvents"], json!([]));
        assert_eq!(state.extra["logs"], json!(["RULE 카드 발동: 포화"]));
        assert_eq!(
            state.extra["pendingNotation"],
            json!({"id":"opening-rule-saturation","kind":"card","color":"white","moveNumber":0,"text":"@포화","description":"RULE 포화 적용"})
        );
        assert_eq!(
            state.extra["pendingNotations"],
            json!([state.extra["pendingNotation"]])
        );
        assert_eq!(
            state.extra["ruleOpeningEvent"],
            json!({
                "createdAt":1790581292828_i64,
                "dismissAt":1790581296428_i64,
                "nonce":"rule-1790581292828-o97mlepxl0n",
                "status":"hit",
                "title":"RULE CARD",
                "message":"포화 카드가 발동됩니다.",
                "card":{"art":"blank","effect":"saturation","id":"saturation","instanceId":"saturation-jhjnxgjtixe","name":"포화","phase":"RULE","stars":null,"text":"기물을 3개 이상 잡은 기물은 더 이상 기물을 잡을 수 없습니다."}
            })
        );
        assert_eq!(
            state.extra["appliedRuleCard"]["instanceId"],
            "applied-rule-saturation"
        );
    }
}

#[test]
fn v7_opening_rule_selection_uses_pool_order_and_explicit_support_boundary() {
    let new_game = |ids: &[&str], rules_version: &str| {
        crate::draft::initialize_for_ruleset(
            GameConfig {
                rule_card_ids: ids.iter().map(|id| (*id).into()).collect(),
                ..GameConfig::default()
            },
            19,
            rules_version,
        )
    };
    let acceleration = new_game(&["saturation", "acceleration"], RULES_VERSION_V7).unwrap();
    assert_eq!(acceleration.extra["appliedRuleCard"]["id"], "acceleration");
    assert_eq!(acceleration.extra["selectedRuleCardId"], "");
    assert_eq!(acceleration.extra["accelerationPendingFor"], "black");
    assert_eq!(acceleration.extra["accelerationPendingTurns"], 2);
    assert_eq!(acceleration.extra["accelerationStartsAfterBlackTurns"], 2);
    assert_eq!(
        acceleration.extra["ruleOpeningEvent"]["card"],
        json!({"art":"time","effect":"acceleration","id":"acceleration","instanceId":"acceleration-jhjnxgjtixe","name":"가속","phase":"OPENING","stars":5,"text":"적용 뒤 세번째 흑 차례부터 모든 플레이어가 한 턴에 2번 행동합니다."})
    );
    assert_eq!(acceleration.rng.cursor, 125);
    assert_eq!(acceleration.rng.state, 3595483778);
    let saturation = new_game(&["acceleration", "saturation"], RULES_VERSION_V7).unwrap();
    assert_eq!(saturation.extra["appliedRuleCard"]["id"], "saturation");
    assert!(matches!(
        new_game(&["saturation"], RULES_VERSION_V6),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        new_game(&["capture-the-flag"], RULES_VERSION_V7),
        Err(EngineError::InvalidConfig(_))
    ));
    assert!(matches!(
        new_game(&["portal"], RULES_VERSION_V7),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        new_game(&["saturation", "saturation"], RULES_VERSION_V7),
        Err(EngineError::InvalidConfig(_))
    ));
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
fn v7_relay_first_play_effect_sets_source_flag_without_rng() {
    // Frozen OracleRuntime seed 19: relay is the second white offer, followed
    // by the first black offer (ice-sheet). The no-target relay card is then
    // accepted without ending white's turn.
    let mut state =
        crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7).unwrap();
    let white_pick = crate::draft::legal_actions(&state).unwrap().remove(1);
    crate::draft::apply_pick(&mut state, &white_pick).unwrap();
    crate::replay::canonicalize_position_frames(&mut state).unwrap();
    let black_pick = crate::draft::legal_actions(&state).unwrap().remove(0);
    crate::transition::apply(&mut state, &black_pick).unwrap();
    assert_eq!(state.mode, "play");
    assert_eq!(state.turn, Color::White);
    assert_eq!(state.rng.cursor, 216);
    assert_eq!(state.rng.state, 3909686763);
    assert_eq!(
        source_state_digest(&state),
        "a106b0ce0aeb64b5301a3f90a3fb1c0c32c4a6d4e504362d311587800c4a072c"
    );
    assert_eq!(state.history.len(), 1);
    // Frozen OracleRuntime records the same black draft action and both
    // viewer projections when recordHistory=true.
    assert_eq!(
        event_digest(&state.history[0]),
        "8606504fac022613b0232a4b26e22321249edd9708059837d732c1ce9932782b"
    );
    let relay = state.deck_slots.white[0].clone();
    assert_eq!(relay.id, "relay");
    assert_eq!(relay.extra["name"], "교대");
    let action = Action::card(Color::White, &relay, None);
    let before = state.clone();
    assert_eq!(
        crate::card_effects::validate(&state, &relay, &action).unwrap(),
        Some(true)
    );
    assert_eq!(state, before);
    let mut availability_probe = state.clone();
    assert!(
        crate::transition::available_card_action(&mut availability_probe, Color::White).unwrap()
    );
    assert_eq!(availability_probe, state);
    assert!(
        crate::card_effects::apply(&mut state, &relay, &action)
            .unwrap()
            .unwrap()
            .is_empty()
    );
    assert_eq!(state.extra["relay"], json!({"white":true,"black":false}));
    assert_eq!(state.turn, Color::White);
    assert_eq!(state.actions_remaining, 1);
    assert_eq!(state.rng.cursor, 216);
    assert_eq!(state.rng.state, 3909686763);
}

#[test]
fn v7_chaos_first_play_reposition_marks_source_order_without_effect_rng() {
    // Frozen v7 seed 19 chaos: white chooses queens-gambit + reposition,
    // black chooses qxe1 + promotion-rush. The source no-target reposition
    // effect marks all 16 white pieces and queues their IDs in board order.
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
    assert_eq!(state.rng.cursor, 400);
    assert_eq!(state.rng.state, 185085603);
    assert_eq!(
        source_state_digest(&state),
        "bf4efad54e5815e3e401e87adeab5ddce665484422d45e51d1fc6dec032fe62a"
    );
    let card = state.deck_slots.white[1].clone();
    assert_eq!(card.id, "reposition");
    let action = Action::card(Color::White, &card, None);
    assert_eq!(
        crate::card_effects::actions(&state, &card).unwrap(),
        Some(vec![action.clone()])
    );
    let before = state.clone();
    assert_eq!(
        crate::card_effects::validate(&state, &card, &action).unwrap(),
        Some(true)
    );
    assert_eq!(state, before);
    let mut full_transition = state.clone();
    assert!(
        crate::card_effects::apply(&mut state, &card, &action)
            .unwrap()
            .unwrap()
            .is_empty()
    );
    let marked = state
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|piece| piece.color == Color::White)
        .collect::<Vec<_>>();
    assert_eq!(marked.len(), 16);
    assert!(
        marked
            .iter()
            .all(|piece| piece.extra["repositionSecondMove"] == json!({"used":false}))
    );
    assert_eq!(
        state.extra["forceAnimatedPieceIds"]["values"],
        json!(marked.iter().map(|piece| &piece.id).collect::<Vec<_>>())
    );
    assert_eq!(state.rng.cursor, 400);
    assert_eq!(state.rng.state, 185085603);
    crate::transition::apply(&mut full_transition, &action).unwrap();
    assert_eq!(full_transition.rng.cursor, 401);
    assert_eq!(full_transition.rng.state, 2623095718);
    assert_eq!(
        source_state_digest(&full_transition),
        "ffb09feb16101e97cb9f2258bc004103afc861313035aea71c98c1951d35aa02"
    );
    assert_eq!(full_transition.history.len(), 1);
    assert_eq!(
        event_digest(&full_transition.history[0]),
        "b373d12b4b3be999be991ea51478692cc0a0190136fade0d69c0fa11f405087c"
    );
}

#[test]
fn v7_chaos_first_play_queens_gambit_preserves_source_capture_and_random_file() {
    // Frozen v7 seed 19 chaos. This is the one legal queens-gambit card
    // action after white takes queens-gambit + reposition and black takes
    // qxe1 + promotion-rush; it sacrifices the white queen at d1.
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
    assert_eq!(
        source_state_digest(&state),
        "bf4efad54e5815e3e401e87adeab5ddce665484422d45e51d1fc6dec032fe62a"
    );
    let card = state.deck_slots.white[0].clone();
    assert_eq!(card.id, "queens-gambit");
    let action = Action::card(Color::White, &card, Some(json!({"row":7,"col":3})));
    assert_eq!(
        crate::card_effects::actions(&state, &card).unwrap(),
        Some(vec![action.clone()])
    );
    let before = state.clone();
    assert_eq!(
        crate::card_effects::validate(&state, &card, &action).unwrap(),
        Some(true)
    );
    assert_eq!(state, before);
    let mut effect_only = state.clone();
    let captures = crate::card_effects::apply(&mut effect_only, &card, &action)
        .unwrap()
        .unwrap();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].id, "white-queen-1ou4c52pl2n");
    assert_eq!(
        effect_only.extra["queensGambitFiles"]["white"],
        json!({"queenCol":3,"randomCol":6})
    );
    assert_eq!(effect_only.rng.cursor, 401);
    assert_eq!(effect_only.rng.state, 2623095718);
    let mut full_transition = state;
    crate::transition::apply(&mut full_transition, &action).unwrap();
    assert_eq!(full_transition.rng.cursor, 402);
    assert_eq!(full_transition.rng.state, 1495369421);
    assert_eq!(
        source_state_digest(&full_transition),
        "99e8597a58674f6c0482616bceef3503540f3e50feaaad91558d89bb4dabbc6e"
    );
    assert_eq!(full_transition.history.len(), 1);
    assert_eq!(
        event_digest(&full_transition.history[0]),
        "0283e6ebb0d124c74da8f544cd465602d6dc09e6e34a9446e858577d312adce2"
    );
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
