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

fn write_card_dispatch_diagnostic(path: &std::path::Path, cases: &[Value]) {
    assert!(
        path.is_absolute(),
        "card dispatch diagnostic path must be absolute"
    );
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("engine manifest must be inside the project workspace")
        .canonicalize()
        .expect("workspace path must resolve");
    let parent = path
        .parent()
        .expect("diagnostic path must have a parent")
        .canonicalize()
        .expect("diagnostic parent must already exist");
    assert!(
        !parent.starts_with(&workspace),
        "diagnostic output must remain outside Git"
    );
    if path.exists() {
        assert!(
            !path
                .canonicalize()
                .expect("diagnostic path must resolve")
                .starts_with(&workspace),
            "diagnostic output must not link into the workspace"
        );
    }
    let execution_profile: Value = serde_json::from_str(include_str!(
        "../../contracts/catalog/execution-profile-20260928.json"
    ))
    .expect("compiled v7 execution profile must be valid JSON");
    let report = json!({
        "schemaVersion":1,
        "sourceMainSha256":"e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c",
        "recipe":"seed37-first-draft-offer-then-first-source-card-target",
        "compiledExecutionProfile":{
            "profileVersion":execution_profile["profileVersion"],
            "sha256":event_digest(&execution_profile),
            "initializerCount":execution_profile["initializers"].as_array().unwrap().len(),
            "initializersSha256":execution_profile["initializersSha256"],
            "replayMetadataSha256":execution_profile["replayMetadataSha256"],
        },
        "cases":cases,
    });
    std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap())
        .expect("card dispatch diagnostic write must succeed");
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
        "17f03e0add054f3b231f4f3943c78dcd358ed18f815ab368b9d1879869072075"
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
fn v7_shotgun_penalty_query_preserves_source_short_circuit_and_deck_side_effects() {
    let mut base = crate::draft::initialize_for_ruleset(
        GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        },
        19,
        RULES_VERSION_V7,
    )
    .unwrap();
    base.mode = "play".into();
    base.extra.insert("gameStyle".into(), json!("normal"));
    base.board = vec![vec![None; 8]; 8];
    base.deck_slots.white.clear();
    base.deck_slots.black.clear();
    let import = |state: &GameState, present: bool| {
        let mut raw = serde_json::to_value(state).unwrap();
        let fields = raw.as_object_mut().unwrap();
        for key in ["rulesetId", "rng", "history"] {
            fields.shift_remove(key);
        }
        if !present {
            fields.shift_remove("deckSlots");
        }
        crate::V7HostPosition::from_parts(raw, state.rng.clone(), state.history.clone())
            .expect("source deck presence must be admitted without normalization")
    };

    let mut none = import(&base, true).state().clone();
    assert!(!none.source_deck_slots_absent);
    assert_eq!(v7_shotgun_penalty_color(&mut none).unwrap(), None);
    assert_eq!(
        (none.deck_slots.white.len(), none.deck_slots.black.len()),
        (3, 3)
    );
    assert!(
        none.deck_slots
            .white
            .iter()
            .chain(&none.deck_slots.black)
            .all(|card| card.vacant)
    );
    assert_eq!(none.rng, base.rng);
    assert_eq!(
        none.extra, base.extra,
        "deck reads do not record a replay or card event"
    );

    let mut board_white = base.clone();
    board_white.board[7][4] = Some(crate::Piece::new(
        "shotgunKing",
        Color::White,
        "white-shotgun",
    ));
    let absent_white = import(&board_white, false);
    board_white = import(&board_white, true).state().clone();
    let before = board_white.clone();
    assert_eq!(
        v7_shotgun_penalty_color(&mut board_white).unwrap(),
        Some(Color::White)
    );
    assert_eq!(
        board_white, before,
        "a White board hit does not read either deck"
    );
    let original = absent_white.export_envelope().unwrap();
    let (unread, color) = absent_white
        .transact(absent_white.position_id(), v7_shotgun_penalty_color)
        .unwrap();
    assert_eq!(color, Some(Color::White));
    assert!(
        unread.state().source_deck_slots_absent,
        "a White board hit must not create a missing global deck"
    );
    assert_eq!(unread.export_envelope().unwrap(), original);

    let mut board_black = base.clone();
    board_black.board[0][4] = Some(crate::Piece::new(
        "shotgunKing",
        Color::Black,
        "black-shotgun",
    ));
    let absent_black = import(&board_black, false);
    board_black = import(&board_black, true).state().clone();
    assert_eq!(
        v7_shotgun_penalty_color(&mut board_black).unwrap(),
        Some(Color::Black)
    );
    assert_eq!(
        (
            board_black.deck_slots.white.len(),
            board_black.deck_slots.black.len()
        ),
        (3, 0)
    );
    assert_eq!(board_black.rng, base.rng);
    let (created, color) = absent_black
        .transact(absent_black.position_id(), v7_shotgun_penalty_color)
        .unwrap();
    assert_eq!(color, Some(Color::Black));
    assert!(!created.state().source_deck_slots_absent);
    assert_eq!(
        (
            created.state().deck_slots.white.len(),
            created.state().deck_slots.black.len()
        ),
        (3, 3),
        "White playerDeck creates both missing sides before the Black board hit"
    );
    assert!(created.state().deck_slots.white_first);
    assert_eq!(created.state().rng, absent_black.state().rng);
    assert_eq!(created.state().history, absent_black.state().history);
    let exported = created.export_envelope().unwrap();
    assert!(
        exported["state"]["deckSlots"]["white"]
            .as_array()
            .unwrap()
            .iter()
            .all(Value::is_null)
    );
    assert!(
        exported["state"]["deckSlots"]["black"]
            .as_array()
            .unwrap()
            .iter()
            .all(Value::is_null)
    );
    assert!(
        !crate::V7HostPosition::from_envelope(exported)
            .unwrap()
            .state()
            .source_deck_slots_absent
    );

    let mut card_white = base.clone();
    let mut card: CardSlot = serde_json::from_value(
        definition_for(RULES_VERSION_V7, "shotgun-king")
            .unwrap()
            .source_definition
            .clone(),
    )
    .unwrap();
    card.instance_id = "white-shotgun-card".into();
    // Used cards still count for the source's long-game penalty.
    card.used = true;
    card_white.deck_slots.white.push(card.clone());
    card_white = import(&card_white, true).state().clone();
    let before_card = card_white.deck_slots.white[0].clone();
    assert_eq!(
        v7_shotgun_penalty_color(&mut card_white).unwrap(),
        Some(Color::White)
    );
    assert_eq!(
        (
            card_white.deck_slots.white.len(),
            card_white.deck_slots.black.len()
        ),
        (3, 0)
    );
    assert_eq!(card_white.deck_slots.white[0], before_card);
    assert_eq!(card_white.rng, base.rng);

    let mut legacy = base;
    legacy.ruleset_id = RULES_VERSION_V6.into();
    let before = legacy.clone();
    assert!(matches!(
        v7_shotgun_penalty_color(&mut legacy),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert_eq!(legacy, before);
}

#[test]
fn v7_owned_card_without_legacy_plan_validates_without_mutating_the_position() {
    let mut state = state();
    let card = hand_card(&mut state, "armistice", Color::White);
    let before = state.clone();
    let action = Action::card(Color::White, &card, None);
    assert_eq!(
        crate::card_effects::validate(&state, &card, &action).unwrap(),
        Some(true)
    );
    assert_eq!(state, before);

    let canonical_null = Action::card(Color::White, &card, Some(Value::Null));
    assert_eq!(
        crate::card_effects::validate(&state, &card, &canonical_null).unwrap(),
        Some(true),
        "source canonical null is the same no-selection effect input"
    );
    assert_eq!(state, before);

    let bad = Action::card(Color::White, &card, Some(json!({"row":7,"col":4})));
    assert_eq!(
        crate::card_effects::validate(&state, &card, &bad).unwrap(),
        Some(false)
    );
    assert_eq!(state, before);
}

#[test]
fn finish_card_consumes_only_the_roulette_event_payload() {
    for (id, effect_id) in [
        ("random-roulette", None),
        ("black-box", Some("random-roulette")),
    ] {
        let mut state = state();
        let mut card = hand_card(&mut state, id, Color::White);
        if let Some(effect_id) = effect_id {
            card.extra
                .insert("boxRevealedCardId".into(), json!(effect_id));
        }
        card.extra
            .insert("randomRouletteResultType".into(), json!("wizard"));
        let event = json!({"color":"white","targetColor":"black","previousType":"rook",
            "resultType":"wizard","row":0,"col":0});
        card.extra
            .insert("randomRouletteResult".into(), event.clone());
        state.deck_slots.white[0] = card.clone();
        let rng = state.rng.clone();
        let (settled, copied_event) = consume_finish_card_transient(&mut state, &card, 0).unwrap();
        assert_eq!(copied_event, Some(event), "{id}");
        assert_eq!(settled, state.deck_slots.white[0]);
        assert!(!settled.extra.contains_key("randomRouletteResult"), "{id}");
        assert_eq!(
            settled.extra.get("randomRouletteResultType"),
            Some(&json!("wizard"))
        );
        assert_eq!(state.rng, rng);
        assert_eq!(
            consume_finish_card_transient(&mut state, &settled, 0)
                .unwrap()
                .1,
            None,
            "{id} must not emit the same effect twice"
        );
    }
}

#[test]
fn v7_public_cards_have_exclusive_effect_ownership_or_fail_closed() {
    let registry = registry_for(RULES_VERSION_V7).unwrap();
    let mut public = 0;
    let mut active_without_owner = Vec::new();
    for definition in registry
        .cards
        .values()
        .filter(|card| card.card_type.is_some())
    {
        public += 1;
        let owners = crate::card_effects::v7_manual_effect_owner_count(&definition.id);
        assert!(
            owners <= 1,
            "{} has overlapping effect owners",
            definition.id
        );
        match (definition.card_type, definition.activation) {
            (Some(CardType::Rule), Some(CardActType::Active)) => {
                assert_eq!(
                    owners, 0,
                    "RULE {} entered the hand effect registry",
                    definition.id
                );
                assert!(crate::card_effects::opening_rule_effect_name(&definition.id).is_some());
            }
            (_, Some(CardActType::Passive)) => {
                assert!(
                    crate::v7_card_passive::owns(&definition.id) || definition.id == "white-box",
                    "PASSIVE {} has no source acquisition owner",
                    definition.id
                );
                assert_eq!(owners, usize::from(definition.id == "white-box"));
            }
            (_, Some(CardActType::Active)) if owners == 0 => {
                active_without_owner.push(definition.id.as_str());
                // A catalog entry alone must not admit an old effect path.
                let mut state = state();
                let card = hand_card(&mut state, &definition.id, Color::White);
                assert!(
                    matches!(
                        action_policy(&state, &card),
                        Err(EngineError::UnsupportedFeature(_))
                    ),
                    "{} admitted without a v7 effect owner",
                    definition.id
                );
                let before = state.clone();
                assert!(matches!(
                    crate::card_effects::apply(
                        &mut state,
                        &card,
                        &Action::card(Color::White, &card, None)
                    ),
                    Err(EngineError::UnsupportedFeature(_))
                ));
                assert_eq!(
                    state, before,
                    "{} changed state before rejecting",
                    definition.id
                );
            }
            (_, Some(CardActType::Active)) => {}
            _ => panic!("{} has an invalid public source category", definition.id),
        }
    }
    assert_eq!(public, 256);
    assert!(
        active_without_owner.is_empty(),
        "source ACTIVE cards lack effect objects: {active_without_owner:?}"
    );
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
fn dynamic_blood_registry_is_separate_from_draft_and_authenticates_campaign_cards() {
    let registry = registry_for(RULES_VERSION_V7).unwrap();
    let before_count = registry.cards.len();
    let definition = definition_for(RULES_VERSION_V7, "blood").unwrap();
    assert_eq!(definition.effect, "bloodCard");
    assert_eq!(definition.card_type, None);
    assert_eq!(definition.activation, Some(CardActType::Active));
    assert_eq!(definition.turn_policy, CardTurnPolicy::PreserveTurn);
    assert_eq!(definition.selection.kind, SelectionKind::None);
    assert_eq!(registry.cards.len(), before_count);
    assert!(!registry.cards.contains_key("blood"));
    assert!(
        !crate::draft::definitions_for_ruleset(RULES_VERSION_V7)
            .unwrap()
            .definitions
            .iter()
            .any(|card| card["id"] == "blood")
    );
    assert_eq!(
        crate::card_effects::v7_manual_effect_owner_count("blood"),
        1
    );
    assert!(definition_for(RULES_VERSION_V6, "blood").is_err());

    let mut state = state();
    let blood = hand_card(&mut state, "blood", Color::White);
    assert!(matches!(
        validate_instance(&state, &blood),
        Err(EngineError::InvalidState(_))
    ));
    state
        .extra
        .insert("campaign".into(), json!({"setup":"bloodMoon"}));
    validate_instance(&state, &blood).unwrap();
    assert!(source_candidate_available(&state, &blood).unwrap());
    assert_eq!(action_policy(&state, &blood).unwrap().slot, 0);
    let mut forged = blood.clone();
    forged.extra.insert("actType".into(), json!("PASSIVE"));
    assert!(
        validate_instance(&state, &forged)
            .unwrap_err()
            .to_string()
            .contains("unknown Blood card metadata")
    );
}

#[test]
fn dynamic_black_tower_has_authenticated_manual_and_special_first_move_entries() {
    let definition = definition_for(RULES_VERSION_V7, "black-tower-legacy-magic").unwrap();
    assert_eq!(definition.effect, "blackTowerLegacyMagic");
    assert_eq!(definition.card_type, None);
    assert_eq!(definition.activation, Some(CardActType::Active));
    assert_eq!(definition.source_definition["phase"], json!("???"));
    assert_eq!(definition.turn_policy, CardTurnPolicy::PreserveTurn);
    assert_eq!(definition.selection.kind, SelectionKind::None);
    let catalog = registry_for(RULES_VERSION_V7).unwrap();
    assert_eq!(catalog.cards.len(), 257);
    assert!(!catalog.cards.contains_key("black-tower-legacy-magic"));
    assert!(
        !crate::draft::definitions_for_ruleset(RULES_VERSION_V7)
            .unwrap()
            .definitions
            .iter()
            .any(|card| card["id"] == "black-tower-legacy-magic")
    );
    assert_eq!(
        crate::card_effects::v7_manual_effect_owner_count("black-tower-legacy-magic"),
        1
    );
    assert!(definition_for(RULES_VERSION_V6, "black-tower-legacy-magic").is_err());

    let mut state = state();
    state.mode = "play".into();
    state.turn = Color::Black;
    let mut card = hand_card(&mut state, "black-tower-legacy-magic", Color::Black);
    assert!(matches!(
        validate_instance(&state, &card),
        Err(EngineError::InvalidState(_))
    ));
    state
        .extra
        .insert("campaign".into(), json!({"setup":"blackTower"}));
    validate_instance(&state, &card).unwrap();
    assert!(source_candidate_available(&state, &card).unwrap());
    assert_eq!(
        action_policy(&state, &card).unwrap().activation,
        CardActType::Active
    );
    assert_eq!(
        forced_first_move_policy(&state, &card, Color::Black)
            .unwrap()
            .activation,
        CardActType::ActiveForced
    );
    card.extra.insert("firstTurnCard".into(), json!(false));
    state.deck_slots.black[0] = card.clone();
    validate_instance(&state, &card).unwrap();
    assert!(matches!(
        forced_first_move_policy(&state, &card, Color::Black),
        Err(EngineError::IllegalAction)
    ));
    let mut forged = card.clone();
    forged.extra.insert("phase".into(), json!("MIDDLE"));
    assert!(matches!(
        validate_instance(&state, &forged),
        Err(EngineError::InvalidState(_))
    ));
}

#[test]
fn auxiliary_shotgun_has_source_manual_and_forced_policies_without_public_activation() {
    let mut state = state();
    state.mode = "play".into();
    let mut card = hand_card(&mut state, "shotgun-king", Color::White);
    let definition = definition_for(RULES_VERSION_V7, &card.id).unwrap();
    assert_eq!(definition.card_type, None);
    assert_eq!(definition.activation, None);
    assert_eq!(definition.source_definition["phase"], json!("GUN"));
    assert_eq!(
        crate::card_effects::v7_manual_effect_owner_count(&card.id),
        1
    );
    assert_eq!(
        action_policy(&state, &card).unwrap().activation,
        CardActType::Active
    );
    assert!(settle_policy(&state, &card).unwrap().end_move);
    assert!(matches!(
        forced_first_move_policy(&state, &card, Color::White),
        Err(EngineError::IllegalAction)
    ));
    card.extra.insert("firstTurnCard".into(), json!(true));
    state.deck_slots.white[0] = card.clone();
    assert_eq!(
        forced_first_move_policy(&state, &card, Color::White)
            .unwrap()
            .activation,
        CardActType::ActiveForced
    );
    state.turns_taken.white = 1;
    assert!(matches!(
        forced_first_move_policy(&state, &card, Color::White),
        Err(EngineError::IllegalAction)
    ));
    state.deck_slots.white[0].used = true;
    let used = state.deck_slots.white[0].clone();
    assert!(matches!(
        action_policy(&state, &used),
        Err(EngineError::IllegalAction)
    ));
}

#[test]
fn v7_presentation_catalog_mismatch_fails_closed() {
    let mut presentation: Value = serde_json::from_str(include_str!(
        "../../contracts/catalog/card-presentation-20260928.json"
    ))
    .unwrap();
    presentation["definitions"][0]["stars"] = json!(99);
    let altered = serde_json::to_string(&presentation).unwrap();
    assert!(matches!(
        CardRegistry::load(
            RULES_VERSION_V7,
            V7_MAIN,
            include_str!("../../contracts/catalog/site-20260928.json"),
            include_str!("../../contracts/catalog/card-definitions-20260928.json"),
            include_str!("../../contracts/catalog/draft-20260928.json"),
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
                "quantum-mechanics",
                "princess",
                "submerge",
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
                "siren",
                "exile",
                "reverse-pawns",
                "empty-lunchbox",
                "baby-bear",
                "vanish",
                "coronation",
                "charge",
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
            "ec494d043ba3c535229308b391de4660beff1961c3eabb0fe0bd06881f05f8ff",
            "8d87f10b832cfed5d24d402da8a0b9c48a3b591603f93c8fa3ab1699e0a9fe58",
            "adb503b71ffbeb84a1080d9fc386856857bae9a4feb5a26bd6c24e9a8bde4765",
            "ba5484f8cb72a238b8297d51e6ae2821a69903b863f7cb2bf9b3c0cf4c28692e",
            3,
            214,
            1029675283,
        ),
        (
            "chaos",
            "a8b4d9a7bb518eb235868276f67c9cf27e45ea325ff8b1cbafc3674f00a82aae",
            "9f836eb107c5676c4d1252f22afa5d14833220c4617ea6331eb7cfcf6857e966",
            "52769b28d85171e9b6b073e2c451de68f8dcfae691c5cee28e9ec27a96832bb8",
            "ddf03a0e97f4bdcb1b923baa80f0ddeb864eaf0d4695aaa1cf1497e1cfa59593",
            3,
            396,
            1564888433,
        ),
        (
            "grand",
            "7243871ce98ae31a6666c0c6e9748c3d43184d527cf626b5f2e992472ba0b300",
            "819614db44b2c5d0fe67ccdaa9567630299665c987f0f01c31ad98d98cb4a39c",
            "97ad964e67aa761da85692e20e27d1a860ccee21b3f2f3d68a112ec8325882fe",
            "242631d70cc7665155c09c2435b908bcc5cf8f0dd6d67529917fda9c45d2f319",
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
fn v7_seed37_actual_cards_bind_unique_owners_and_preserve_full_transition_gate() {
    // Frozen v7 faithful-init 175 원문, seed 37, 매 draft의 첫 선택으로 만든
    // 실제 손패에서 첫 source-legal 행동을 실행한다. 원문 자료는 생성물의
    // reports/card-dispatch-source-faithful175.json에 보관한다.
    // Roulette 전체 상태는 별도 VM에서 같은 Position을 restore 후 apply한
    // reports/card-dispatch-roulette-cold-warm-source.json의 cold 결과다.
    // 이 검사는 후보 열거의 presentation 전이를 원본에 실행하지 않는다.
    // 동일 VM의 actions→apply는 비공개 randomRouletteClockPause가 남아
    // 시계를 다시 정지하지 않는 warm 3cef5568… 결과이므로 섞지 않는다.
    // opt-in 진단은 원문 assertion을 대체하지 않으며, 출력 디렉터리는
    // 호출자가 미리 준비한 저장소 외부의 보고서 경로여야 한다.
    let diagnostic_path =
        std::env::var_os("ACCELERATE_V7_CARD_DISPATCH_DIAGNOSTIC").map(std::path::PathBuf::from);
    let mut diagnostic_cases = Vec::new();
    for (style, id, target, legal_count, state_hash, cursor, rng_state, event_hash) in [
        (
            "normal",
            "nullification",
            json!({"row":6,"col":0}),
            16,
            "965ccfe42210805646d7a55726563a54bfd0f338f84fd8b32c4d88eac25989ad",
            217,
            3923168504,
            "be39663feb6a45fbb8f8d90836ccf2605971af116374f1b4ce338c00774c8c19",
        ),
        (
            "grand",
            "random-roulette",
            json!({"row":0,"col":0}),
            6,
            "560912c32ac192298909c3ec35d65a13a74ed968c9469057ac81b8ac82baeb29",
            129,
            314049984,
            "90b3178259a5fcec8759649e937d1ec9a4a981839e481d23d1a8bd712fd5a4c2",
        ),
        (
            "grand",
            "feudal-contract",
            json!({"row":7,"col":0,"pawn":{"row":6,"col":0}}),
            64,
            "dd712dc70d80e04539fcf60b7cda227193a5dad0a0417378bd6bacc01c7ea156",
            126,
            894305467,
            "d7f70b6198ed6a2d15f449545bf1f4f132b31fc8544cef179ec58f2596ab749f",
        ),
        (
            "grand",
            "panic",
            json!({"selections":[{"row":0,"col":0},{"row":0,"col":1}]}),
            210,
            "b9bea3654dc15b7176bd5a772ec5c00c26f5ef58d9f8f6fc8a7391c3736de38b",
            125,
            2854222796,
            "94487d6fa04ebb935625f52376dc8bd23ffe596e7a86ada9dc822bc4fd3e8d71",
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
            let draft = crate::draft::legal_actions(&state).unwrap().remove(0);
            crate::draft::apply_pick(&mut state, &draft).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        assert_eq!(state.mode, "play", "{style}");
        let card = state
            .deck_slots
            .white
            .iter()
            .find(|card| card.id == id)
            .unwrap()
            .clone();
        assert_eq!(
            crate::card_effects::v7_manual_effect_owner_count(id),
            1,
            "{id}"
        );
        let action = Action::card(Color::White, &card, Some(target));
        let legal = crate::card_effects::actions(&state, &card)
            .unwrap()
            .unwrap();
        assert_eq!(legal.len(), legal_count, "{id} source action count");
        assert_eq!(legal[0], action, "{id} first source action");
        let before = state.clone();
        assert_eq!(
            crate::card_effects::validate(&state, &card, &action).unwrap(),
            Some(true),
            "{id} clone-apply binding"
        );
        assert_eq!(
            state, before,
            "{id} binding must preserve the original state"
        );
        assert_eq!(
            settle_policy(&state, &card).unwrap(),
            CardSettlePolicy {
                end_move: false,
                clear_extra_actions: false,
            },
            "{id} source finishCard policy"
        );
        let outcome = crate::transition::apply(&mut state, &action);
        if let Some(path) = &diagnostic_path {
            let before_value = serde_json::to_value(&before).unwrap();
            let after_value = serde_json::to_value(&state).unwrap();
            diagnostic_cases.push(json!({
                "style":style,"id":id,"action":action,
                "before":before_value,"after":after_value,
                "beforeStateSha256":event_digest(&before_value),
                "afterStateSha256":event_digest(&after_value),
                "beforeSourceStateSha256":source_state_digest(&before),
                "afterSourceStateSha256":source_state_digest(&state),
                "expected":{"stateSha256":state_hash,"rngCursor":cursor,
                    "rngState":rng_state,"eventSha256":event_hash},
                "outcome":match &outcome {
                    Ok(captures) => json!({"ok":true,"captures":captures}),
                    Err(error) => json!({"ok":false,"error":error.to_string()}),
                },
            }));
            write_card_dispatch_diagnostic(path, &diagnostic_cases);
        }
        match outcome {
            Ok(_) => {
                // 카드 효과와 후속 no-action 정산을 포함한 전체 원문 전이를 검사한다.
                assert_eq!(
                    source_state_digest(&state),
                    state_hash,
                    "{id} full source state"
                );
                assert_eq!(
                    (state.rng.cursor, state.rng.state),
                    (cursor, rng_state),
                    "{id} RNG"
                );
                assert_eq!(state.history.len(), 1, "{id} public event count");
                assert_eq!(event_digest(&state.history[0]), event_hash, "{id} event");
                let settled = state
                    .deck_slots
                    .white
                    .iter()
                    .find(|candidate| candidate.instance_id == card.instance_id)
                    .unwrap();
                assert!(settled.used, "{id} must spend its source instance");
            }
            Err(error) => panic!("{id} unexpected transition failure: {error}"),
        }
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
        "be467ad1b1073ec421767009ca26f2f2943e672d71a10244675d55e1e4953ac5"
    );
    assert_eq!(full_transition.history.len(), 1);
    assert_eq!(
        event_digest(&full_transition.history[0]),
        "6d2fe3042a755f92d6e223798b71bb2dfae3d7905c22ee38073fe59bff352d85"
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
fn v7_grand_first_play_targeted_cards_match_source_candidates_and_direct_rng() {
    // Frozen v7 seed 19 grand: all twelve draft picks are real source offers.
    // Each row is the first legal action of an independently reachable card.
    let initial = seed19_grand_active_only();
    for (id, target, count) in [
        ("inertia", json!({"row":0,"col":0}), 5),
        ("trojan-horse", json!({"row":7,"col":1}), 2),
        ("grasshopper", json!({"row":7,"col":1}), 4),
        (
            "grappler",
            json!({"row":7,"col":3,"minor":{"row":7,"col":1}}),
            4,
        ),
    ] {
        let card = initial
            .deck_slots
            .white
            .iter()
            .find(|card| card.id == id)
            .unwrap();
        let expected = Action::card(Color::White, card, Some(target));
        let legal = crate::card_effects::actions(&initial, card)
            .unwrap()
            .unwrap();
        assert_eq!(legal.len(), count, "{id} legal target count");
        assert_eq!(legal[0], expected, "{id} first source payload");
        let before = initial.clone();
        assert_eq!(
            crate::card_effects::validate(&initial, card, &expected).unwrap(),
            Some(true),
            "{id} source payload binding"
        );
        assert_eq!(initial, before, "{id} binding must not mutate state");
        let mut effect_only = initial.clone();
        crate::card_effects::apply(&mut effect_only, card, &expected)
            .unwrap()
            .unwrap();
        assert_eq!(effect_only.rng, initial.rng, "{id} direct effect RNG");
    }
}

#[test]
fn v7_grand_black_symmetry_uses_source_flag_and_rejects_reactivation() {
    // Source legal after White's a2-a3; the public v7 movement boundary is
    // still closed, so only the card-local effect contract is exercised.
    let mut state = seed19_grand_active_only();
    state.turn = Color::Black;
    let card = state
        .deck_slots
        .black
        .iter()
        .find(|card| card.id == "symmetry")
        .unwrap()
        .clone();
    let action = Action::card(Color::Black, &card, None);
    assert_eq!(state.extra.get("symmetry"), None);
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
    assert_eq!(effect_only.extra["symmetry"], json!({"black":true}));
    assert_eq!(effect_only.rng, state.rng);
    assert_eq!(
        crate::card_effects::actions(&effect_only, &card).unwrap(),
        Some(Vec::new())
    );
    assert_eq!(
        crate::card_effects::validate(&effect_only, &card, &action).unwrap(),
        Some(false)
    );
    let mut v6 = GameState::new(GameConfig::default(), 7).unwrap();
    assert_eq!(crate::card_effects::actions(&v6, &card).unwrap(), None);
    assert_eq!(
        crate::card_effects::apply(&mut v6, &card, &action).unwrap(),
        None
    );
}

#[test]
fn v7_normal_black_ice_sheet_marks_five_targets_without_effect_rng() {
    // Recreate the source opening board after White's a2-a3, then install the
    // pinned Ice Sheet definition. Corrected seed-19 offers omit this card.
    let mut state =
        crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7).unwrap();
    let white_pick = crate::draft::legal_actions(&state).unwrap().remove(1);
    crate::draft::apply_pick(&mut state, &white_pick).unwrap();
    crate::replay::canonicalize_position_frames(&mut state).unwrap();
    let black_pick = crate::draft::legal_actions(&state).unwrap().remove(0);
    crate::transition::apply(&mut state, &black_pick).unwrap();
    assert_eq!(state.mode, "play");
    let mut pawn = state.board[6][0].take().unwrap();
    pawn.moved = true;
    state.board[5][0] = Some(pawn);
    state.turn = Color::Black;
    let card = hand_card(&mut state, "ice-sheet", Color::Black);
    let action = Action::card(Color::Black, &card, None);
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
    let iced = effect_only
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|piece| piece.extra.get("iceSheet").is_some())
        .collect::<Vec<_>>();
    assert_eq!(iced.len(), 5);
    assert_eq!(
        iced.iter()
            .map(|piece| piece.id.as_str())
            .collect::<Vec<_>>(),
        [
            "white-rook-02ifugfz7b6v",
            "white-bishop-tthtdsu1jc",
            "white-queen-1ou4c52pl2n",
            "white-bishop-qyp71rr4ag",
            "white-rook-dc8ui3a6p5m",
        ]
    );
    assert!(
        iced.iter()
            .all(|piece| piece.extra["iceSheet"] == json!({"by":"black","remaining":3}))
    );
    assert_eq!(effect_only.rng, state.rng);
    let mut special_targets = state.clone();
    for (col, kind) in [(0, "magicGirl"), (2, "berserker"), (3, "trickster")] {
        special_targets.board[7][col].as_mut().unwrap().kind = kind.into();
    }
    assert!(
        !crate::card_effects::ranged_piece(
            &special_targets,
            special_targets.board[7][0].as_ref().unwrap()
        ) && !crate::card_effects::ranged_piece(
            &special_targets,
            special_targets.board[7][2].as_ref().unwrap()
        ) && !crate::card_effects::ranged_piece(
            &special_targets,
            special_targets.board[7][3].as_ref().unwrap()
        )
    );
    crate::card_effects::apply(&mut special_targets, &card, &action)
        .unwrap()
        .unwrap();
    assert_eq!(
        special_targets
            .board
            .iter()
            .flatten()
            .flatten()
            .filter(|piece| piece.extra.get("iceSheet").is_some())
            .count(),
        5
    );
    let mut idless = state.clone();
    for (row, col) in [(7, 0), (7, 7), (6, 1)] {
        idless.board[row][col].as_mut().unwrap().id.clear();
    }
    crate::card_effects::apply(&mut idless, &card, &action)
        .unwrap()
        .unwrap();
    assert!(
        idless.board[7][0]
            .as_ref()
            .unwrap()
            .extra
            .contains_key("iceSheet")
    );
    assert!(
        idless.board[7][7]
            .as_ref()
            .unwrap()
            .extra
            .contains_key("iceSheet")
    );
    assert!(
        !idless.board[6][1]
            .as_ref()
            .unwrap()
            .extra
            .contains_key("iceSheet")
    );
    let mut no_targets = state.clone();
    for cells in &mut no_targets.board {
        for piece in cells {
            if piece.as_ref().is_some_and(|piece| {
                ["rook", "bishop", "queen"].contains(&piece.kind.as_str())
                    && piece.color == Color::White
            }) {
                *piece = None;
            }
        }
    }
    assert_eq!(
        crate::card_effects::actions(&no_targets, &card).unwrap(),
        Some(Vec::new())
    );
    assert_eq!(
        crate::card_effects::validate(&no_targets, &card, &action).unwrap(),
        Some(false)
    );
    let mut v6 = GameState::new(GameConfig::default(), 7).unwrap();
    assert_eq!(crate::card_effects::actions(&v6, &card).unwrap(), None);
    assert_eq!(
        crate::card_effects::apply(&mut v6, &card, &action).unwrap(),
        None
    );
}

#[test]
fn v7_normal_black_quantum_mechanics_sets_pending_without_effect_rng() {
    // Recreate the source opening board but install the pinned Quantum
    // Mechanics definition: corrected seed-19 offers omit this card.
    // Its later move resolution remains in the separate movement boundary.
    let mut state =
        crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7).unwrap();
    let white_pick = crate::draft::legal_actions(&state).unwrap().remove(1);
    crate::draft::apply_pick(&mut state, &white_pick).unwrap();
    crate::replay::canonicalize_position_frames(&mut state).unwrap();
    let black_pick = crate::draft::legal_actions(&state).unwrap().remove(1);
    crate::transition::apply(&mut state, &black_pick).unwrap();
    let mut pawn = state.board[6][0].take().unwrap();
    pawn.moved = true;
    state.board[5][0] = Some(pawn);
    state.turn = Color::Black;
    let card = hand_card(&mut state, "quantum-mechanics", Color::Black);
    assert_eq!(card.id, "quantum-mechanics");
    assert_eq!(
        state.extra["quantumPending"],
        json!({"black":false,"white":false})
    );
    assert_eq!(
        action_policy(&state, &card).unwrap(),
        CardActionPolicy {
            actor: Color::Black,
            slot: 0,
            activation: CardActType::Active,
            use_cost: CardUseCost::SpendInstance,
            turn_policy: CardTurnPolicy::PreserveTurn,
        }
    );
    let action = Action::card(Color::Black, &card, None);
    assert_eq!(
        crate::card_effects::actions(&state, &card).unwrap(),
        Some(vec![action.clone()])
    );
    let invalid = Action::card(Color::Black, &card, Some(json!({"row":0,"col":0})));
    assert_eq!(
        crate::card_effects::validate(&state, &card, &invalid).unwrap(),
        Some(false)
    );
    let original = state.clone();
    crate::card_effects::apply(&mut state, &card, &action)
        .unwrap()
        .unwrap();
    assert_eq!(
        state.extra["quantumPending"],
        json!({"black":true,"white":false})
    );
    assert_eq!(state.rng, original.rng);
    assert_eq!(state.history, original.history);
    assert_eq!(
        crate::card_effects::actions(&state, &card).unwrap(),
        Some(vec![action.clone()]),
        "the source still considers an already pending Quantum card playable"
    );
    let once = state.clone();
    crate::card_effects::apply(&mut state, &card, &action)
        .unwrap()
        .unwrap();
    assert_eq!(state, once);
    let mut v6 = GameState::new(GameConfig::default(), 7).unwrap();
    assert_eq!(crate::card_effects::actions(&v6, &card).unwrap(), None);
    assert_eq!(
        crate::card_effects::apply(&mut v6, &card, &action).unwrap(),
        None
    );
}

#[test]
fn v7_selected_opening_rule_preserves_source_event_rng_and_draft_order() {
    // Independent SHA-pinned OracleRuntime.newGame({ruleCardIds}, 19) results.
    for (style, expected_cursor, expected_rng_state, expected_offer) in [
        (
            "normal",
            125,
            3595483778_u32,
            vec!["holdout", "feudal-contract", "loyalist"],
        ),
        (
            "chaos",
            215,
            4043093436_u32,
            vec![
                "holdout",
                "feudal-contract",
                "loyalist",
                "panic",
                "en-passant-bang",
                "e4",
            ],
        ),
        (
            "grand",
            115,
            911746088_u32,
            vec!["merchant-guild", "locust-swarm", "calling-card"],
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
                "card":{"art":"blank","effect":"saturation","help":"","helpIcons":[],"helpItems":[],"id":"saturation","instanceId":"saturation-jhjnxgjtixe","name":"포화","phase":"RULE","stars":null,"text":"기물을 3개 이상 잡은 기물은 더 이상 기물을 잡을 수 없습니다."}
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
        json!({"art":"time","effect":"acceleration","help":"","helpIcons":[],"helpItems":[],"id":"acceleration","instanceId":"acceleration-jhjnxgjtixe","name":"가속","phase":"RULE","stars":null,"text":"적용 뒤 세번째 흑 차례부터 모든 플레이어가 한 턴에 2번 행동합니다."})
    );
    assert_eq!(acceleration.rng.cursor, 125);
    assert_eq!(acceleration.rng.state, 3595483778);
    let saturation = new_game(&["acceleration", "saturation"], RULES_VERSION_V7).unwrap();
    assert_eq!(saturation.extra["appliedRuleCard"]["id"], "saturation");
    assert!(matches!(
        new_game(&["saturation"], RULES_VERSION_V6),
        Err(EngineError::UnsupportedFeature(_))
    ));
    let flag = new_game(&["capture-the-flag"], RULES_VERSION_V7).unwrap();
    assert_eq!(flag.extra["appliedRuleCard"]["id"], "capture-the-flag");
    assert_eq!(
        flag.extra["captureTheFlag"]["occupations"],
        json!({"white":null,"black":null})
    );
    let portal = new_game(&["portal"], RULES_VERSION_V7).unwrap();
    assert_eq!(portal.extra["appliedRuleCard"]["id"], "portal");
    assert_eq!(portal.extra["portalRule"]["enabled"], true);
    assert!(matches!(
        new_game(&["saturation", "saturation"], RULES_VERSION_V7),
        Err(EngineError::InvalidConfig(_))
    ));
}

#[test]
fn v7_source_rule_pool_has_one_executable_object_per_definition() {
    let draft: Value =
        serde_json::from_str(include_str!("../../contracts/catalog/draft-20260928.json")).unwrap();
    let pool = draft["constants"]["CARD_CATEGORY_GROUPS"]["RULE"]
        .as_array()
        .unwrap();
    assert_eq!(pool.len(), 27);
    let mut seen = std::collections::BTreeSet::new();
    for id in pool {
        let id = id.as_str().unwrap();
        assert!(seen.insert(id), "duplicate source RULE {id}");
        let object_effect = crate::card_effects::opening_rule_effect_name(id)
            .unwrap_or_else(|| panic!("source RULE {id} lacks an object"));
        assert_eq!(
            definition_for(RULES_VERSION_V7, id).unwrap().effect,
            object_effect
        );
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
        ["berserker", "chameleon-mutation", "qxe1"]
    );
    assert_eq!(state.rng.cursor, 290);
    assert_eq!(state.rng.state, 1623078365);
}

#[test]
fn v7_relay_first_play_effect_sets_source_flag_without_rng() {
    // Frozen OracleRuntime seed 19: relay is the second white offer, followed
    // by the first black offer (berserker). The no-target relay card is then
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
    assert_eq!(state.rng.cursor, 292);
    assert_eq!(state.rng.state, 596976663);
    assert_eq!(
        source_state_digest(&state),
        "8ea26cf766f9fee2e04804f0cea78c07828d6749e14873578be3ab3848b868f8"
    );
    assert_eq!(state.history.len(), 1);
    // Frozen OracleRuntime records the same black draft action and both
    // viewer projections when recordHistory=true.
    assert_eq!(
        event_digest(&state.history[0]),
        "8ae6ee2d3e189a13605f5a00d1982fc27f0762da9f44c98256511b8000ef6785"
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
    assert_eq!(state.rng.cursor, 292);
    assert_eq!(state.rng.state, 596976663);
}

#[test]
fn v7_chaos_first_play_quantum_mechanics_matches_source_without_effect_rng() {
    // Corrected frozen v7 seed 19 chaos: white receives queens-gambit and
    // quantum-mechanics, then black receives qxe1 and promotion-rush.
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
        "3b8fd9fae0404a6032c8f5daa7bbdecc03cac93730355f13ebb6d97a2f290793"
    );
    let card = state.deck_slots.white[1].clone();
    assert_eq!(card.id, "quantum-mechanics");
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
    assert_eq!(
        state.extra["quantumPending"],
        json!({"white":true,"black":false})
    );
    assert_eq!(state.rng.cursor, 400);
    assert_eq!(state.rng.state, 185085603);
    crate::transition::apply(&mut full_transition, &action).unwrap();
    assert_eq!(full_transition.rng.cursor, 401);
    assert_eq!(full_transition.rng.state, 2623095718);
    assert_eq!(
        source_state_digest(&full_transition),
        "c0177e32da59799b4dd2a5ffb877d6358baf130efff6b185aefa15f4e7002f99"
    );
    assert_eq!(full_transition.history.len(), 1);
    assert_eq!(
        event_digest(&full_transition.history[0]),
        "69993b75e4191785c131393aec296014567104e1f938009c49bb00f18bb1e075"
    );
}

#[test]
fn v7_chaos_first_play_queens_gambit_preserves_source_capture_and_random_file() {
    // Frozen v7 seed 19 chaos. This is the one legal queens-gambit card
    // action after white takes queens-gambit + quantum-mechanics and black takes
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
        "3b8fd9fae0404a6032c8f5daa7bbdecc03cac93730355f13ebb6d97a2f290793"
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
        "0406e7cc7fe6131f9d8bc70a4ff1df402be55809be42918964b311ac4a6bd1f9"
    );
    assert_eq!(full_transition.history.len(), 1);
    assert_eq!(
        event_digest(&full_transition.history[0]),
        "6360293e0efbf482e0de3a3af97f308dcd81f439c5539303927cb18ddef5a012"
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
fn v7_source_card_gate_uses_category_delay_and_exclusive_turn_rules() {
    let mut state = state();
    let mut opening = hand_card(&mut state, "guard", Color::White);
    // Source libraryCardPhase prefers CARD_CATEGORY_BY_ID to the instance's
    // mutable phase. OPENING cards do not acquire the Middle/End delay.
    opening.extra.insert("phase".into(), json!("END"));
    opening
        .extra
        .insert("nextTurnPending".into(), json!("pending"));
    state.deck_slots.white[0] = opening.clone();
    assert!(source_candidate_available(&state, &opening).unwrap());
    assert!(action_policy(&state, &opening).is_ok());

    let mut delayed = hand_card(&mut state, "armistice", Color::White);
    delayed
        .extra
        .insert("nextTurnPending".into(), json!("pending"));
    state.deck_slots.white[0] = delayed.clone();
    assert!(!source_candidate_available(&state, &delayed).unwrap());
    assert_eq!(
        require_source_effect_window(&state, &delayed),
        Err(EngineError::IllegalAction)
    );
    assert_eq!(
        action_policy(&state, &delayed).unwrap_err(),
        EngineError::IllegalAction
    );
    delayed.extra.insert("devCard".into(), json!(true));
    state.deck_slots.white[0] = delayed.clone();
    assert!(source_candidate_available(&state, &delayed).unwrap());
    assert!(action_policy(&state, &delayed).is_ok());
    delayed.used = true;
    assert!(!source_candidate_available(&state, &delayed).unwrap());
    // Direct virtual execution has no hand-use check; only the source
    // pending/exclusive window applies to a cloned effect object.
    assert!(require_source_effect_window(&state, &delayed).is_ok());

    let mut exclusive = hand_card(&mut state, "brainwash", Color::White);
    exclusive.extra.insert("devCard".into(), json!(true));
    state.deck_slots.white[0] = exclusive.clone();
    state.cards_used_this_turn.black = 4;
    assert!(source_candidate_available(&state, &exclusive).unwrap());
    state.cards_used_this_turn.white = 1;
    let before = state.clone();
    assert!(!source_candidate_available(&state, &exclusive).unwrap());
    assert_eq!(
        require_source_effect_window(&state, &exclusive),
        Err(EngineError::IllegalAction)
    );
    assert_eq!(
        action_policy(&state, &exclusive).unwrap_err(),
        EngineError::IllegalAction
    );
    assert_eq!(state, before);

    let zugzwang = hand_card(&mut state, "zugzwang", Color::White);
    state
        .extra
        .insert("zugzwangConsumesTurn".into(), json!(false));
    assert!(source_candidate_available(&state, &zugzwang).unwrap());
    state
        .extra
        .insert("profile".into(), json!({"catalogHash":CATALOG_VERSION}));
    assert!(!source_candidate_available(&state, &zugzwang).unwrap());
    state.extra.insert(
        "cardState".into(),
        json!({"profile":{"catalogHash":"unknown"}}),
    );
    assert!(source_candidate_available(&state, &zugzwang).unwrap());

    let passive = hand_card(&mut state, "apprentice-knights", Color::White);
    assert!(source_candidate_available(&state, &passive).unwrap());
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
