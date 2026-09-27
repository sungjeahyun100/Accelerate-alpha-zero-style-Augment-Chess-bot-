use crate::*;
use serde_json::json;

fn empty() -> GameState {
    let mut state = GameState::new(kernel_config(), 11).unwrap();
    state.board = vec![vec![None; 8]; 8];
    state.extra.insert(
        "positionCounts".into(),
        json!({"__simType":"Map","entries":[]}),
    );
    state
}
fn kernel_config() -> GameConfig {
    GameConfig {
        draft_delete: true,
        ..GameConfig::default()
    }
}
fn put(state: &mut GameState, kind: &str, color: Color, row: u8, col: u8) {
    state.board[row as usize][col as usize] = Some(Piece::new(
        kind,
        color,
        format!("{}-{row}-{col}", color.as_str()),
    ));
}
fn movement(position: &Position, from: Square, to: Square) -> Action {
    position
        .legal_actions()
        .unwrap()
        .into_iter()
        .find(|action| {
            action.from == Some(from)
                && action
                    .destination
                    .as_ref()
                    .is_some_and(|target| target.square() == to)
        })
        .unwrap()
}

#[test]
fn standard_initial_moves_branch_without_mutation() {
    let root = Position::new_game(kernel_config(), 42).unwrap();
    let before = root.to_json().unwrap();
    assert_eq!(root.legal_actions().unwrap().len(), 20);
    let public = root.try_observe(Color::White).unwrap();
    assert_eq!(
        public.public_state["legalHints"]["moves"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hint| hint["destinations"].as_array().unwrap().len())
            .sum::<usize>(),
        20
    );
    assert!(
        !public.public_state["legalHints"]
            .to_string()
            .contains("positionKey")
    );
    assert!(
        root.try_observe(Color::Black).unwrap().public_state["legalHints"]["moves"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut stream = root.action_stream().unwrap();
    let mut streamed = Vec::new();
    loop {
        let page = stream.next_page(3).unwrap();
        assert!(page.actions.len() <= 3);
        streamed.extend(page.actions);
        if page.exhausted {
            break;
        }
    }
    assert_eq!(streamed, root.legal_actions().unwrap());
    assert!(stream.next_page(3).unwrap().actions.is_empty());
    assert!(stream.next_page(0).is_err());
    let mut semantic = streamed[0].clone();
    semantic.position_key = None;
    let bound = root
        .bind_payload(serde_json::to_value(&semantic).unwrap())
        .unwrap();
    assert_eq!(bound, streamed[0]);
    root.validate_action(&bound).unwrap();
    let action = movement(&root, Square { row: 6, col: 4 }, Square { row: 4, col: 4 });
    let step = root.apply(&action).unwrap();
    assert_eq!(before, root.to_json().unwrap());
    assert_eq!(step.position.state().turn, Color::Black);
    assert!(step.turn_changed);
    assert_eq!(step.position.state().en_passant.as_ref().unwrap().row, 5);
    assert_eq!(step.position.legal_actions().unwrap().len(), 20);
    assert_eq!(
        step.position.apply(&action).unwrap_err(),
        EngineError::StaleAction
    );
}

#[test]
fn rejects_bad_flags_and_preserves_original() {
    let position = Position::new_game(kernel_config(), 1).unwrap();
    let before = position.to_json().unwrap();
    let mut action = movement(
        &position,
        Square { row: 6, col: 0 },
        Square { row: 5, col: 0 },
    );
    action
        .destination
        .as_mut()
        .unwrap()
        .flags
        .insert("magicCapture".into(), json!(true));
    assert_eq!(
        position.apply(&action).unwrap_err(),
        EngineError::IllegalAction
    );
    assert_eq!(position.to_json().unwrap(), before);
}

#[test]
fn actual_royal_capture_is_terminal_without_checkmate_assumptions() {
    let mut state = empty();
    put(&mut state, "king", Color::White, 7, 7);
    put(&mut state, "rook", Color::White, 1, 0);
    put(&mut state, "king", Color::Black, 0, 0);
    let root = Position::from_state(state).unwrap();
    let action = movement(&root, Square { row: 1, col: 0 }, Square { row: 0, col: 0 });
    let step = root.apply(&action).unwrap();
    assert_eq!(step.result, Some(GameResult::White));
    assert_eq!(step.captures.len(), 1);
    assert_eq!(step.position.state().mode, "gameover");
    assert!(step.position.legal_actions().unwrap().is_empty());
}

#[test]
fn castle_moves_both_entities_and_cancels_reuse() {
    let mut state = empty();
    put(&mut state, "king", Color::White, 7, 4);
    put(&mut state, "rook", Color::White, 7, 7);
    put(&mut state, "king", Color::Black, 0, 4);
    let position = Position::from_state(state).unwrap();
    let action = movement(
        &position,
        Square { row: 7, col: 4 },
        Square { row: 7, col: 6 },
    );
    let step = position.apply(&action).unwrap();
    assert_eq!(
        step.position
            .state()
            .at(Square { row: 7, col: 5 })
            .unwrap()
            .kind,
        "rook"
    );
    assert!(
        step.position
            .state()
            .at(Square { row: 7, col: 4 })
            .is_none()
    );
    assert!(
        step.position
            .state()
            .at(Square { row: 7, col: 7 })
            .is_none()
    );
}

#[test]
fn en_passant_captures_off_destination() {
    let mut state = empty();
    put(&mut state, "pawn", Color::White, 3, 4);
    put(&mut state, "pawn", Color::Black, 3, 3);
    state.en_passant = Some(EnPassant {
        row: 2,
        col: 3,
        captured_row: 3,
        captured_col: 3,
        color: Color::Black,
    });
    let position = Position::from_state(state).unwrap();
    let action = movement(
        &position,
        Square { row: 3, col: 4 },
        Square { row: 2, col: 3 },
    );
    assert!(action.destination.as_ref().unwrap().flag("enPassant"));
    let intent = position.public_intent(&action).unwrap();
    assert_eq!(
        intent,
        json!({"type":"move","color":"white","from":{"row":3,"col":4},"destination":{"row":2,"col":3}})
    );
    assert_eq!(position.bind_public_intent(intent.clone()).unwrap(), action);
    let mut leaking_intent = intent;
    leaking_intent["destination"]["enPassant"] = json!(true);
    assert!(position.bind_public_intent(leaking_intent).is_err());
    let step = position.apply(&action).unwrap();
    assert!(
        step.position
            .state()
            .at(Square { row: 3, col: 3 })
            .is_none()
    );
    assert_eq!(step.captures.len(), 1);
}

#[test]
fn card_is_free_and_additional_action_changes_actor_only_at_boundary() {
    let mut state = GameState::new(kernel_config(), 0).unwrap();
    state.deck_slots.white[0] = CardSlot {
        id: "geneva-convention".into(),
        effect: "genevaConvention".into(),
        instance_id: "white-geneva-0".into(),
        stars: 2.0,
        used: false,
        recovering: false,
        vacant: false,
        extra: Fields::new(),
    };
    let position = Position::from_state(state).unwrap();
    let action = position
        .legal_actions()
        .unwrap()
        .into_iter()
        .find(|a| a.kind == ActionKind::Card)
        .unwrap();
    let step = position.apply(&action).unwrap();
    assert!(!step.turn_changed);
    assert!(step.position.state().flag("genevaConvention", Color::White));
    let mut state = step.position.state().clone();
    state.actions_remaining = 2;
    state.extra.insert("acceleration".into(), json!(true));
    let position = Position::from_state(state).unwrap();
    let first = position
        .apply(&movement(
            &position,
            Square { row: 6, col: 0 },
            Square { row: 5, col: 0 },
        ))
        .unwrap();
    assert!(!first.turn_changed);
    assert_eq!(first.position.state().move_count, 0);
    let second = first
        .position
        .apply(&movement(
            &first.position,
            Square { row: 6, col: 1 },
            Square { row: 5, col: 1 },
        ))
        .unwrap();
    assert!(second.turn_changed);
    assert_eq!(second.position.state().actions_remaining, 2);
    assert_eq!(second.position.state().move_count, 1);
}

#[test]
fn unknown_state_fields_survive_but_unsupported_effect_is_rejected() {
    let mut state = empty();
    put(&mut state, "rook", Color::White, 5, 5);
    state.extra.insert("auditMarker".into(), json!({"a":[1,2]}));
    let position = Position::from_state(state).unwrap();
    let roundtrip = Position::from_json(&position.to_json().unwrap()).unwrap();
    assert_eq!(position.to_json().unwrap(), roundtrip.to_json().unwrap());
    let mut state = position.state().clone();
    state.extra.insert("quantumMechanics".into(), json!(true));
    assert!(matches!(
        Position::from_state(state).unwrap().legal_actions(),
        Err(EngineError::UnsupportedFeature(_))
    ));
}

#[test]
fn nonpublic_piece_offers_and_rng_do_not_enter_observation_or_its_key() {
    let mut state = empty();
    put(&mut state, "rook", Color::White, 5, 0);
    put(&mut state, "knight", Color::Black, 2, 7);
    state.board[2][7]
        .as_mut()
        .unwrap()
        .extra
        .insert("hiddenFrom".into(), json!("white"));
    state
        .extra
        .insert("futureDraftOffers".into(), json!(["secret-offer"]));
    state.deck_slots.black.push(CardSlot {
        id: "freeze".into(),
        effect: "freeze".into(),
        instance_id: "black-secret".into(),
        stars: 2.0,
        used: false,
        recovering: false,
        vacant: false,
        extra: Fields::new(),
    });
    let first = Position::from_state(state.clone())
        .unwrap()
        .observe(Color::White);
    state.rng = RngState::seeded(999);
    state
        .extra
        .insert("futureDraftOffers".into(), json!(["different-offer"]));
    state.board[2][7].as_mut().unwrap().kind = "amazon".into();
    let second = Position::from_state(state).unwrap().observe(Color::White);
    assert_eq!(first, second);
    assert!(first.board[2][7].is_none());
    assert_eq!(first.opponent_hand_count, 1);
    let serialized = serde_json::to_string(&first).unwrap();
    assert!(serialized.contains("black-secret"));
    assert!(!serialized.contains("secret-offer"));
    assert!(!serialized.contains("lcg32"));
}

#[test]
fn large_identity_moves_and_serializes_as_one_entity() {
    let mut state = empty();
    let mut piece = Piece::new("bigRook", Color::White, "large");
    piece.extra.insert("anchorRow".into(), json!(4));
    piece.extra.insert("anchorCol".into(), json!(3));
    for row in 4..6 {
        for col in 3..5 {
            state.board[row][col] = Some(piece.clone());
        }
    }
    let position = Position::from_state(state).unwrap();
    let restored = Position::from_json(&position.to_json().unwrap()).unwrap();
    let action = movement(
        &restored,
        Square { row: 4, col: 3 },
        Square { row: 3, col: 3 },
    );
    let step = restored.apply(&action).unwrap();
    assert_eq!(
        step.position
            .state()
            .board
            .iter()
            .flatten()
            .flatten()
            .filter(|p| p.id == "large")
            .count(),
        4
    );
    assert!(
        step.position
            .state()
            .at(Square { row: 5, col: 3 })
            .is_none()
    );
}

#[test]
fn seeded_rng_and_tape_are_reproducible_and_bounded() {
    let mut first = RngState::seeded(5);
    let mut second = RngState::seeded(5);
    for _ in 0..10 {
        assert_eq!(first.sample().unwrap(), second.sample().unwrap());
    }
    first = RngState::seeded(5);
    first.tape = vec![0.1, 0.9];
    assert_eq!(first.sample().unwrap(), 0.1);
    assert_eq!(first.sample().unwrap(), 0.9);
    assert!(first.sample().unwrap() >= 0.0);
    assert_eq!(first.cursor, 3);
}

#[test]
fn source_snapshot_preserves_presence_null_slots_and_outer_metadata() {
    let source: serde_json::Value = serde_json::from_str(include_str!(
        "../../bridge/catalog/initial-state-20260927.json"
    ))
    .unwrap();
    let source = source["state"].clone();
    let imported = Position::from_snapshot_value(source.clone()).unwrap();
    assert_eq!(imported.export_state().unwrap(), source);
    let updated = imported
        .with_metadata(RngState::seeded(91), Vec::new())
        .unwrap();
    assert_eq!(updated.export_state().unwrap(), source);
    assert_ne!(updated.key(), imported.key());
    assert_eq!(updated.state().deck_slots.white.len(), 3);
    assert!(
        updated
            .state()
            .deck_slots
            .white
            .iter()
            .all(|slot| slot.vacant)
    );
    assert!(
        updated
            .with_metadata(
                RngState::seeded(0),
                vec![json!({"unknownHistoryEvent":true})]
            )
            .is_err()
    );
    let mut changed = imported.state().clone();
    changed.extra.remove("activeTrolley");
    let changed = Position(std::sync::Arc::new(changed), imported.1.clone());
    let exported = changed.export_state().unwrap();
    assert!(exported.get("activeTrolley").is_none());
    let restored = Position::from_snapshot_value(exported).unwrap();
    assert!(restored.state().extra.get("activeTrolley").is_none());
}

#[test]
fn promotion_waits_for_actual_choice_before_turn_completion() {
    let mut state = empty();
    put(&mut state, "king", Color::White, 7, 7);
    put(&mut state, "king", Color::Black, 0, 7);
    put(&mut state, "pawn", Color::White, 1, 3);
    let initial = Position::from_state(state).unwrap();
    let moved = initial
        .apply(&movement(
            &initial,
            Square { row: 1, col: 3 },
            Square { row: 0, col: 3 },
        ))
        .unwrap();
    assert!(!moved.turn_changed);
    assert_eq!(moved.position.actor(), Color::White);
    assert_eq!(
        moved
            .position
            .state()
            .at(Square { row: 0, col: 3 })
            .unwrap()
            .kind,
        "pawn"
    );
    assert_eq!(moved.position.state().move_count, 0);
    let actions = moved.position.legal_actions().unwrap();
    assert_eq!(actions.len(), 4);
    assert!(
        actions
            .iter()
            .all(|action| action.kind == ActionKind::PromotionChoice)
    );
    let choice = actions
        .iter()
        .find(|action| action.extra.get("promotionType") == Some(&json!("bishop")))
        .unwrap();
    let promoted = moved.position.apply(choice).unwrap();
    assert!(promoted.turn_changed);
    assert_eq!(
        promoted
            .position
            .state()
            .at(Square { row: 0, col: 3 })
            .unwrap()
            .kind,
        "bishop"
    );
    assert_eq!(promoted.position.state().move_count, 1);
}

#[test]
fn repetition_uses_board_identity_and_lower_half_star_total_wins() {
    let mut state = empty();
    put(&mut state, "king", Color::White, 7, 7);
    put(&mut state, "king", Color::Black, 0, 0);
    let mut card = CardSlot {
        id: "known-card".into(),
        effect: "retreat".into(),
        instance_id: "w1".into(),
        stars: 0.0,
        used: true,
        recovering: false,
        vacant: false,
        extra: Fields::new(),
    };
    card.extra.insert("ratingHalfStars".into(), json!(3));
    state.deck_slots.white[0] = card.clone();
    card.instance_id = "b1".into();
    card.extra.insert("ratingHalfStars".into(), json!(4));
    state.deck_slots.black[0] = card;
    assert!(!crate::flow::check_termination(&mut state).unwrap());
    assert!(!crate::flow::check_termination(&mut state).unwrap());
    assert!(crate::flow::check_termination(&mut state).unwrap());
    assert_eq!(state.result(), Some(GameResult::White));
    let mut draw = empty();
    draw.mode = "gameover".into();
    draw.winner = None;
    assert_eq!(draw.result(), Some(GameResult::Draw));
}

#[test]
fn deathmatch_counts_black_boundaries_and_progress_resets_the_window() {
    let mut state = empty();
    state.turns_taken = Sides {
        white: 45,
        black: 45,
    };
    state.extra.insert("deathmatchEnabled".into(), json!(true));
    state.extra.insert("deathmatchLimitTurns".into(), json!(1));
    assert!(!crate::flow::check_termination(&mut state).unwrap());
    assert_eq!(state.extra["deathmatch"]["intervalHalfTurns"], 2);
    crate::flow::mark_progress(&mut state);
    assert!(!crate::flow::tick_deathmatch(&mut state, Color::White).unwrap());
    assert!(!crate::flow::tick_deathmatch(&mut state, Color::Black).unwrap());
    assert_eq!(state.extra["deathmatch"]["halfTurnsSinceProgress"], 0);
    assert!(crate::flow::tick_deathmatch(&mut state, Color::Black).unwrap());
    assert_eq!(state.result(), Some(GameResult::Draw));
}

#[test]
fn private_exact_history_is_separate_from_each_viewers_public_changes() {
    let mut state = empty();
    state.turn = Color::Black;
    put(&mut state, "king", Color::White, 7, 7);
    put(&mut state, "king", Color::Black, 0, 0);
    put(&mut state, "knight", Color::Black, 2, 7);
    state.board[2][7]
        .as_mut()
        .unwrap()
        .extra
        .insert("hiddenFrom".into(), json!("white"));
    let initial = Position::from_state(state).unwrap();
    let next = initial
        .apply(&movement(
            &initial,
            Square { row: 2, col: 7 },
            Square { row: 4, col: 6 },
        ))
        .unwrap()
        .position;
    let private = &next.state().history[0];
    assert_eq!(private["action"]["from"]["row"], 2);
    assert_eq!(private["protocolVersion"], "accelerate-game-event-v1");
    let white = next.observe(Color::White);
    let black = next.observe(Color::Black);
    assert_eq!(
        white.history[0]["boardChanges"].as_array().unwrap().len(),
        0
    );
    assert_eq!(
        black.history[0]["boardChanges"].as_array().unwrap().len(),
        2
    );
    assert!(white.history[0].get("action").is_none());
    let mut malformed = next.state().clone();
    malformed.history[0]["actor"] = json!("white");
    assert!(Position::from_state(malformed).is_err());
    let mut malformed = next.state().clone();
    malformed.history[0]["public"]["white"]["result"]["status"] = json!("terminal");
    assert!(Position::from_state(malformed).is_err());
}

#[test]
fn grand_offers_and_picks_preserve_rng_identity_and_public_card_slots() {
    let initial = Position::new_game(
        GameConfig {
            game_style: "grand".into(),
            ..GameConfig::default()
        },
        11,
    )
    .unwrap();
    assert_eq!(initial.actor(), Color::Black);
    assert_eq!(initial.state().rng.cursor, 112);
    let actions = initial.legal_actions().unwrap();
    assert_eq!(actions.len(), 28);
    assert!(
        actions
            .iter()
            .all(|action| action.kind == ActionKind::DraftPick)
    );
    let first = initial.apply(&actions[0]).unwrap();
    assert_eq!(first.position.actor(), Color::White);
    assert!(first.turn_changed);
    assert_eq!(first.position.state().rng.cursor, 113);
    assert!(!first.position.state().deck_slots.black[0].vacant);
    assert!(first.position.state().deck_slots.black[1].vacant);
    let white = first.position.observe(Color::White);
    assert_eq!(white.opponent_hand_count, 1);
    assert_eq!(
        white.public_state["revealedOpponentCards"][0]["instanceId"],
        json!(actions[0].card_instance_id)
    );
    assert_eq!(initial.state().rng.cursor, 112);
    assert!(initial.state().deck_slots.black[0].vacant);
}

#[test]
fn neutral_obstacle_has_no_player_allegiance_and_is_public_but_not_capturable() {
    let mut state = empty();
    put(&mut state, "king", Color::White, 7, 7);
    put(&mut state, "king", Color::Black, 0, 7);
    put(&mut state, "rook", Color::White, 4, 0);
    state.board[4][3] = Some(Piece::new("wall", PieceColor::Neutral, "wall-4-3"));
    let position = Position::from_state(state).unwrap();
    let wall = position.state().board[4][3].as_ref().unwrap();
    assert_eq!(wall.color.owner(), None);
    assert!(wall.color != Color::White && wall.color != Color::Black);
    for color in [Color::White, Color::Black] {
        assert_eq!(
            position.observe(color).board[4][3].as_ref().unwrap()["color"],
            "neutral"
        );
    }
    assert!(!position.legal_actions().unwrap().iter().any(|action| {
        action.from == Some(Square { row: 4, col: 3 })
            || action
                .destination
                .as_ref()
                .is_some_and(|target| target.row == 4 && target.col >= 3)
    }));
    assert_eq!(
        Position::from_json(&position.to_json().unwrap())
            .unwrap()
            .state()
            .board,
        position.state().board
    );
}

#[test]
fn direct_rust_inputs_enforce_safe_numbers_depth_and_finite_ratings() {
    let mut state = empty();
    state
        .extra
        .insert("unsafeInteger".into(), json!(9_007_199_254_740_992u64));
    assert!(Position::from_state(state).is_err());
    let mut state = empty();
    let mut deep = json!(null);
    for _ in 0..65 {
        deep = json!([deep]);
    }
    state.extra.insert("deep".into(), deep);
    assert!(Position::from_state(state).is_err());
    let mut state = empty();
    state.deck_slots.white[0] = CardSlot {
        id: "invalid".into(),
        effect: "retreat".into(),
        instance_id: "w".into(),
        stars: f64::INFINITY,
        used: false,
        recovering: false,
        vacant: false,
        extra: Fields::new(),
    };
    assert!(Position::from_state(state).is_err());
}

#[test]
fn initial_public_conditioning_preserves_observation_and_independent_future_chance() {
    let config = GameConfig {
        game_style: "grand".into(),
        ..GameConfig::default()
    };
    let actual = Position::new_game(config.clone(), 11).unwrap();
    let expected = actual.try_observe(Color::White).unwrap();
    let sampled = Position::sample_initial_public(
        config.clone(),
        serde_json::to_value(&expected).unwrap(),
        91,
    )
    .unwrap();
    assert_eq!(sampled.try_observe(Color::White).unwrap(), expected);
    assert_ne!(sampled.state().rng.state, actual.state().rng.state);
    assert_ne!(
        sampled.state().board[0][0].as_ref().unwrap().id,
        actual.state().board[0][0].as_ref().unwrap().id
    );
    assert_eq!(sampled.state().rng.cursor, 112);

    let mut bad_hash = serde_json::to_value(&expected).unwrap();
    bad_hash["informationStateKey"] = json!("0".repeat(64));
    assert!(matches!(
        Position::sample_initial_public(config.clone(), bad_hash, 91),
        Err(EngineError::InvalidState(_))
    ));
    let mut impossible = expected.clone();
    impossible.public_state["draft"]["choices"][0]["id"] = json!("nonexistent-definition");
    impossible.refresh_key();
    assert!(
        Position::sample_initial_public(config, serde_json::to_value(impossible).unwrap(), 91)
            .is_err()
    );

    let source = Position::new_game(kernel_config(), 11).unwrap();
    let public = source.try_observe(Color::White).unwrap();
    let independent = Position::sample_initial_public(
        kernel_config(),
        serde_json::to_value(&public).unwrap(),
        91,
    )
    .unwrap();
    assert_eq!(independent.try_observe(Color::White).unwrap(), public);
    assert_ne!(independent.state().rng.state, source.state().rng.state);
    let canonical = serde_json::from_slice(&serde_jcs::to_vec(&public).unwrap()).unwrap();
    let canonical_sample = Position::sample_initial_public(kernel_config(), canonical, 91).unwrap();
    assert_eq!(
        serde_jcs::to_vec(&canonical_sample.try_observe(Color::White).unwrap()).unwrap(),
        serde_jcs::to_vec(&public).unwrap()
    );
    for style in ["normal", "chaos"] {
        let config = GameConfig {
            game_style: style.into(),
            ..GameConfig::default()
        };
        let actual = Position::new_game(config.clone(), 37).unwrap();
        for viewer in [Color::White, Color::Black] {
            let expected = actual.try_observe(viewer).unwrap();
            let canonical = serde_json::from_slice(&serde_jcs::to_vec(&expected).unwrap()).unwrap();
            let sampled = Position::sample_initial_public(config.clone(), canonical, 71).unwrap();
            assert_eq!(
                serde_jcs::to_vec(&sampled.try_observe(viewer).unwrap()).unwrap(),
                serde_jcs::to_vec(&expected).unwrap()
            );
            assert_ne!(sampled.state().rng.state, actual.state().rng.state);
        }
    }
}

#[test]
fn identity_conditioning_relabels_existing_references_and_rejects_semantic_mismatch() {
    let source = Position::new_game(
        GameConfig {
            game_style: "grand".into(),
            ..GameConfig::default()
        },
        11,
    )
    .unwrap();
    let old = source.try_observe(Color::Black).unwrap();
    let mut expected = old.clone();
    let choice = &mut expected.public_state["draft"]["choices"][0];
    let identity = format!(
        "{}-{}",
        choice["id"].as_str().unwrap(),
        crate::draft::random_suffix(0.25).unwrap()
    );
    choice["instanceId"] = json!(identity);
    expected.refresh_key();
    let conditioned = source
        .condition_public_identities(serde_json::to_value(&expected).unwrap())
        .unwrap();
    assert_eq!(conditioned.try_observe(Color::Black).unwrap(), expected);
    assert_eq!(conditioned.state().rng, source.state().rng);
    assert_eq!(source.try_observe(Color::Black).unwrap(), old);
    let action = conditioned
        .legal_actions()
        .unwrap()
        .into_iter()
        .find(|action| action.card_instance_id.as_deref() == Some(identity.as_str()))
        .unwrap();
    assert!(conditioned.apply(&action).is_ok());

    let mut mismatch = expected.clone();
    mismatch.board[0][0].as_mut().unwrap()["type"] = json!("bishop");
    mismatch.refresh_key();
    assert!(matches!(
        conditioned.condition_public_identities(serde_json::to_value(mismatch).unwrap()),
        Err(EngineError::ConditioningMismatch(_))
    ));
    let mut malformed = serde_json::to_value(expected).unwrap();
    malformed["informationStateKey"] = json!("invalid");
    assert!(matches!(
        conditioned.condition_public_identities(malformed),
        Err(EngineError::InvalidState(_))
    ));
}

#[test]
fn draft_pool_predicates_preserve_source_shuffle_side_effects() {
    let source = Position::new_game(kernel_config(), 11).unwrap();
    let mut rejected = Vec::new();
    for card in &crate::draft::definitions().definitions {
        let mut state = source.state().clone();
        let accepted = crate::eligibility::draft_drawable(&mut state, card, Color::White).unwrap();
        let id = card["id"].as_str().unwrap();
        if !accepted {
            rejected.push(id);
        }
        assert_eq!(
            state.rng.cursor - source.state().rng.cursor,
            if matches!(id, "black-box" | "trolley") {
                14
            } else {
                0
            },
            "{id}"
        );
    }
    rejected.sort_unstable();
    assert_eq!(
        rejected,
        vec![
            "conscription",
            "emergency-evacuation",
            "exile",
            "outpost",
            "schrodinger-pawns",
            "traitor"
        ]
    );
    for (style, cursor, count) in [("normal", 122, 3), ("chaos", 212, 6)] {
        let initial = Position::new_game(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            11,
        )
        .unwrap();
        assert_eq!(initial.state().rng.cursor, cursor);
        assert_eq!(initial.actor(), Color::White);
        assert_eq!(
            initial.state().extra["draft"]["choices"]
                .as_array()
                .unwrap()
                .len(),
            count
        );
        assert_eq!(
            initial.state().extra["draft"]["choices"][0]["id"],
            "trickster"
        );
        assert_eq!(
            initial.state().extra["draft"]["choices"][1]["id"],
            "encouragement"
        );
        assert_eq!(initial.state().extra["draft"]["choices"][2]["id"], "roller");
        assert_eq!(initial.legal_actions().unwrap().len(), 3);
    }
}

#[test]
fn regular_draft_acquisition_enters_play_and_clock_commits_at_turn_boundary() {
    let initial = Position::new_game(GameConfig::default(), 37).unwrap();
    let choose = |position: &Position, card_id: &str| {
        let id = position.state().extra["draft"]["choices"]
            .as_array()
            .unwrap()
            .iter()
            .find(|card| card["id"] == card_id)
            .unwrap()["instanceId"]
            .clone();
        let action = position
            .bind_payload(json!({"type":"draftPick","color":position.actor(),"cardInstanceId":id}))
            .unwrap();
        position.apply(&action).unwrap().position
    };
    let black_offer = choose(&initial, "corner-kick");
    assert_eq!(black_offer.actor(), Color::Black);
    assert!(black_offer.state().flag("cornerKick", Color::White));
    assert!(black_offer.state().deck_slots.white[0].used);
    assert_eq!(black_offer.state().rng.cursor, 214);
    let play = choose(&black_offer, "reaper");
    assert_eq!(play.state().mode, "play");
    assert_eq!(play.state().rng.cursor, 216);
    assert_eq!(play.state().extra["clock"]["runningColor"], "white");
    assert_eq!(play.state().extra["clock"]["whiteMs"], 300000);
    let action = play.bind_payload(json!({"type":"move","color":"white","from":{"row":6,"col":0},"move":{"row":5,"col":0}})).unwrap();
    let moved = play.apply(&action).unwrap().position;
    assert_eq!(moved.actor(), Color::Black);
    assert_eq!(moved.state().rng.cursor, 217);
    assert_eq!(moved.state().extra["clock"]["runningColor"], "black");
    assert_eq!(
        moved.state().extra["clock"]["whiteMs"].as_f64(),
        Some(310000.0)
    );
    assert_eq!(
        moved.state().extra["lastMove"]["from"],
        json!({"row":6,"col":0})
    );
    assert_eq!(initial.state().extra["clock"]["runningColor"], Value::Null);
}
