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
        assert!(page.actions.len() <= page.examined && page.examined <= 3);
        assert!(page.exhausted || page.examined > 0);
        streamed.extend(page.actions);
        if page.exhausted {
            break;
        }
    }
    assert_eq!(streamed, root.legal_actions().unwrap());
    let terminal_page = stream.next_page(3).unwrap();
    assert!(terminal_page.actions.is_empty());
    assert_eq!(terminal_page.examined, 0);
    assert!(terminal_page.exhausted);
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
    let replay = &step.position.state().extra["moveReplay"]["white"];
    assert_eq!(replay["delta"].as_array().unwrap().len(), 2);
    assert_eq!(replay["recordedMoveCount"], 1);
    assert_eq!(step.position.state().extra["notationEvent"]["text"], "e4");
    assert_eq!(
        step.position.state().extra["replayEvents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(step.position.legal_actions().unwrap().len(), 20);
    assert_eq!(
        step.position.apply(&action).unwrap_err(),
        EngineError::StaleAction
    );
}

#[test]
fn ordered_cleanup_stream_is_bounded_and_publicly_bindable() {
    let mut state = empty();
    state.mode = "play".into();
    put(&mut state, "king", Color::White, 7, 7);
    put(&mut state, "king", Color::Black, 0, 7);
    let a = Square { row: 3, col: 2 };
    let b = Square { row: 4, col: 5 };
    put(&mut state, "pawn", Color::White, a.row, a.col);
    put(&mut state, "pawn", Color::White, b.row, b.col);
    let mut cleanup: CardSlot = serde_json::from_value(
        crate::draft::definitions()
            .definitions
            .iter()
            .find(|card| card["id"] == "cleanup")
            .unwrap()
            .clone(),
    )
    .unwrap();
    cleanup.instance_id = "cleanup-stream".into();
    state.deck_slots.white = vec![cleanup];
    let position = Position::from_state(state).unwrap();
    assert!(matches!(
        position.legal_actions(),
        Err(EngineError::UnsupportedFeature(_))
    ));
    let before = position.to_json().unwrap();
    let mut stream = position.action_stream().unwrap();
    let mut cards = Vec::new();
    let mut empty_nonterminal = false;
    let mut reached_end = false;
    for _ in 0..256 {
        let page = stream.next_page(1).unwrap();
        assert!(page.actions.len() <= page.examined && page.examined <= 1);
        if page.actions.is_empty() && !page.exhausted {
            empty_nonterminal = true;
        }
        assert!(page.exhausted || page.examined == 1);
        for action in page.actions {
            if action.kind == ActionKind::Card {
                let intent = position.public_intent(&action).unwrap();
                assert_eq!(position.bind_public_intent(intent).unwrap(), action);
                cards.push(action);
            }
        }
        if page.exhausted {
            reached_end = true;
            break;
        }
    }
    assert!(reached_end && empty_nonterminal);
    assert_eq!(
        cards
            .iter()
            .map(|action| action.target.as_ref().unwrap()["selections"].clone())
            .collect::<Vec<_>>(),
        vec![json!([a]), json!([b]), json!([a, b]), json!([b, a])]
    );
    assert!(position.apply(&cards[3]).is_ok());
    assert_eq!(position.to_json().unwrap(), before);
}

#[test]
fn ordered_hypocrisy_stream_resumes_without_materializing_the_family() {
    let mut state = empty();
    state.mode = "play".into();
    put(&mut state, "king", Color::White, 7, 7);
    put(&mut state, "king", Color::Black, 0, 7);
    let mut hypocrisy: CardSlot = serde_json::from_value(
        crate::draft::definitions()
            .definitions
            .iter()
            .find(|card| card["id"] == "hypocrisy")
            .unwrap()
            .clone(),
    )
    .unwrap();
    hypocrisy.instance_id = "hypocrisy-stream".into();
    state.deck_slots.white = vec![hypocrisy];
    let position = Position::from_state(state).unwrap();
    let mut stream = position.action_stream().unwrap();
    let mut cards = Vec::new();
    for _ in 0..128 {
        let page = stream.next_page(2).unwrap();
        assert!(page.actions.len() <= page.examined && page.examined <= 2);
        assert!(page.exhausted || page.examined > 0);
        cards.extend(
            page.actions
                .into_iter()
                .filter(|action| action.kind == ActionKind::Card),
        );
        if cards.len() >= 2 {
            assert!(!page.exhausted);
            break;
        }
    }
    assert!(cards.len() >= 2);
    assert_eq!(
        cards[0].target.as_ref().unwrap()["selections"],
        json!([
            {"row":0,"col":0},
            {"row":0,"col":1},
            {"row":0,"col":2},
            {"row":0,"col":3}
        ])
    );
    assert_eq!(
        cards[1].target.as_ref().unwrap()["selections"],
        json!([
            {"row":0,"col":0},
            {"row":0,"col":1},
            {"row":0,"col":2},
            {"row":0,"col":4}
        ])
    );
}

#[test]
fn portal_stream_remains_unsupported_without_its_future_rule() {
    let mut state = empty();
    state.mode = "play".into();
    put(&mut state, "king", Color::White, 7, 7);
    put(&mut state, "king", Color::Black, 0, 7);
    let mut portal: CardSlot = serde_json::from_value(
        crate::draft::definitions()
            .definitions
            .iter()
            .find(|card| card["id"] == "portal-gun")
            .unwrap()
            .clone(),
    )
    .unwrap();
    portal.instance_id = "portal-stream".into();
    state.deck_slots.white = vec![portal];
    let position = Position::from_state(state).unwrap();
    let before = position.to_json().unwrap();
    let mut stream = position.action_stream().unwrap();
    for _ in 0..2 {
        assert!(matches!(
            stream.next_page(4096),
            Err(EngineError::UnsupportedFeature(_))
        ));
    }
    assert_eq!(position.to_json().unwrap(), before);
}

#[test]
fn portal_card_then_move_preserves_source_replay_frame_boundary() {
    let mut state = empty();
    state.mode = "play".into();
    put(&mut state, "king", Color::White, 7, 7);
    put(&mut state, "king", Color::Black, 0, 7);
    let mut portal: CardSlot = serde_json::from_value(
        crate::draft::definitions()
            .definitions
            .iter()
            .find(|card| card["id"] == "portal-gun")
            .unwrap()
            .clone(),
    )
    .unwrap();
    portal.instance_id = "portal-replay-boundary".into();
    state.deck_slots.white = vec![portal];
    let position = Position::from_state(state).unwrap();
    let action = position
        .bind_payload(json!({"type":"card","color":"white","cardId":"portal-gun","cardInstanceId":"portal-replay-boundary","target":{"selections":[{"row":1,"col":1},{"row":2,"col":2}]}}))
        .unwrap();
    let after_card = position.apply(&action).unwrap().position;
    let after_move = after_card
        .apply(&movement(
            &after_card,
            Square { row: 7, col: 7 },
            Square { row: 6, col: 7 },
        ))
        .unwrap()
        .position;
    let keys = after_move.state().extra["replayEvents"][1]["delta"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|field| field["key"].as_str())
        .collect::<Vec<_>>();
    assert!(keys.contains(&"cardState"));
    assert!(keys.contains(&"pendingPortals"));
    assert_eq!(after_move.state().turn, Color::Black);
    assert_eq!(
        after_move.state().rng.cursor,
        after_card.state().rng.cursor + 1
    );
}

#[test]
fn direct_guard_card_cannot_explain_a_first_move_that_auto_uses_guard() {
    let mut state = GameState::new(kernel_config(), 37).unwrap();
    let mut guard: CardSlot = serde_json::from_value(
        crate::draft::definitions()
            .definitions
            .iter()
            .find(|card| card["id"] == "guard")
            .unwrap()
            .clone(),
    )
    .unwrap();
    guard.instance_id = "guard-first-move".into();
    guard.extra.insert("firstTurnCard".into(), json!(true));
    state.deck_slots.white[0] = guard;
    state.extra.insert(
        "firstMoveCardsForced".into(),
        json!({"white":false,"black":false}),
    );
    let position = Position::from_state(state).unwrap();
    let card = position
        .bind_payload(json!({"type":"card","color":"white",
            "cardId":"guard","cardInstanceId":"guard-first-move"}))
        .unwrap();
    let direct = position.apply(&card).unwrap().position;
    assert_eq!(
        (direct.state().turn, direct.state().move_count),
        (Color::White, 0)
    );
    let direct_public = serde_json::to_value(direct.try_observe(Color::Black).unwrap()).unwrap();
    assert!(
        position
            .public_transition_compatible(&card, direct_public)
            .unwrap()
    );

    let first_move = movement(
        &position,
        Square { row: 6, col: 0 },
        Square { row: 5, col: 0 },
    );
    let moved = position.apply(&first_move).unwrap().position;
    assert_eq!(
        (moved.state().turn, moved.state().move_count),
        (Color::Black, 1)
    );
    assert!(moved.state().deck_slots.white[0].used);
    let moved_public = serde_json::to_value(moved.try_observe(Color::Black).unwrap()).unwrap();
    assert!(
        !position
            .public_transition_compatible(&card, moved_public)
            .unwrap()
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
    let mut democracy = root.state().clone();
    democracy.set_flag("democracy", Color::Black, true);
    put(&mut democracy, "pawn", Color::Black, 0, 1);
    let democracy = Position::from_state(democracy).unwrap();
    assert!(!crate::threat::has_royal_capture(democracy.state(), Color::Black).unwrap());
    let alive = democracy
        .apply(&movement(
            &democracy,
            Square { row: 1, col: 0 },
            Square { row: 0, col: 0 },
        ))
        .unwrap();
    assert_eq!(alive.result, None);
    assert!(alive.position.state().flag("kingDead", Color::Black));
    assert!(democracy.state().at(Square { row: 0, col: 0 }).is_some());
    let mut last_pawn = alive.position.state().clone();
    last_pawn.turn = Color::White;
    let last_pawn = Position::from_state(last_pawn).unwrap();
    let defeated = last_pawn
        .apply(&movement(
            &last_pawn,
            Square { row: 0, col: 0 },
            Square { row: 0, col: 1 },
        ))
        .unwrap();
    assert_eq!(defeated.result, Some(GameResult::White));
    assert_eq!(
        defeated.position.state().extra["replayEndReason"],
        "흑의 모든 폰이 잡혔습니다."
    );
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
    let mut leap = empty();
    put(&mut leap, "pawn", Color::White, 4, 3);
    put(&mut leap, "bishop", Color::Black, 3, 3);
    leap.set_flag("pawnLeap", Color::White, true);
    let leap = Position::from_state(leap).unwrap();
    let action = movement(&leap, Square { row: 4, col: 3 }, Square { row: 2, col: 3 });
    assert!(action.destination.as_ref().unwrap().flag("pawnLeap"));
    let leaped = leap.apply(&action).unwrap();
    assert!(leaped.captures.is_empty());
    assert!(
        leaped
            .position
            .state()
            .at(Square { row: 3, col: 3 })
            .is_some()
    );
    assert!(leaped.position.state().en_passant.is_none());
    assert!(leap.state().at(Square { row: 4, col: 3 }).is_some());
}

#[test]
fn en_passant_frenzy_prioritizes_two_victims_and_expires_on_completed_turn() {
    let mut state = empty();
    put(&mut state, "king", Color::White, 7, 7);
    put(&mut state, "king", Color::Black, 0, 0);
    put(&mut state, "pawn", Color::White, 4, 3);
    put(&mut state, "rook", Color::Black, 4, 4);
    put(&mut state, "bishop", Color::Black, 3, 4);
    state.set_flag("enPassantFrenzy", Color::White, true);
    let position = Position::from_state(state).unwrap();
    let original = position.to_json().unwrap();
    let action = movement(
        &position,
        Square { row: 4, col: 3 },
        Square { row: 3, col: 4 },
    );
    let target = action.destination.as_ref().unwrap();
    assert!(target.flag("enPassant") && target.flag("enPassantFrenzy"));
    assert_eq!(target.flags["capturedRow"], 4);
    assert_eq!(target.flags["capturedCol"], 4);
    let step = position.apply(&action).unwrap();
    assert_eq!(
        step.captures
            .iter()
            .map(|piece| piece.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["rook", "bishop"]
    );
    assert!(step.position.state().board[4][4].is_none());
    assert_eq!(
        step.position.state().board[3][4].as_ref().unwrap().kind,
        "pawn"
    );
    assert!(!step.position.state().flag("enPassantFrenzy", Color::White));
    assert_eq!(position.to_json().unwrap(), original);
}

#[test]
fn owner_turn_effects_tick_once_and_incoming_protection_expires_without_losing_aliases() {
    let mut state = empty();
    put(&mut state, "king", Color::White, 7, 7);
    put(&mut state, "king", Color::Black, 0, 7);
    put(&mut state, "bishop", Color::White, 4, 3);
    let piece = state.board[4][3].as_mut().unwrap();
    for field in ["sacrificeProtection", "lastResistance"] {
        piece.extra.insert(
            field.into(),
            json!({"remaining":3,"previousProtected":false}),
        );
    }
    for (field, remaining) in [
        ("witchTrial", 2),
        ("staked", 4),
        ("severed", 2),
        ("disarmed", 1),
    ] {
        piece
            .extra
            .insert(field.into(), json!({"remaining":remaining}));
    }
    piece.extra.insert("frozen".into(), json!(true));
    piece.extra.insert(
        "frozenByCard".into(),
        json!({"remaining":3,"source":"black"}),
    );
    piece.extra.insert("poisonStunTurns".into(), json!(3));
    piece.extra.insert("poisonStunColor".into(), json!("white"));
    piece.extra.insert("protected".into(), json!(true));
    piece
        .extra
        .insert("grapplerBound".into(), json!({"untilColor":"white"}));
    let mut enemy = state.board[0][7].as_ref().unwrap().clone();
    enemy.extra.insert(
        "coronationProtection".into(),
        json!({"previousProtected":false}),
    );
    enemy.extra.insert("protected".into(), json!(true));
    state.board[0][7] = Some(enemy);
    let position = Position::from_state(state).unwrap();
    let action = movement(
        &position,
        Square { row: 7, col: 7 },
        Square { row: 7, col: 6 },
    );
    let next = position.apply(&action).unwrap().position;
    let piece = next.state().board[4][3].as_ref().unwrap();
    for (field, remaining) in [
        ("sacrificeProtection", 2),
        ("lastResistance", 2),
        ("witchTrial", 1),
        ("staked", 3),
        ("severed", 1),
        ("frozenByCard", 2),
    ] {
        assert_eq!(
            crate::observation::number(
                piece
                    .extra
                    .get(field)
                    .and_then(|entry| entry.get("remaining"))
            ),
            Some(f64::from(remaining))
        );
    }
    assert_eq!(
        crate::observation::number(piece.extra.get("poisonStunTurns")),
        Some(2.0)
    );
    assert!(!piece.extra.contains_key("disarmed"));
    assert!(!piece.extra.contains_key("grapplerBound"));
    let king = next.state().board[0][7].as_ref().unwrap();
    assert!(!king.extra.contains_key("coronationProtection"));
    assert!(!king.extra.contains_key("protected"));
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
        source_order: Vec::new(),
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
        source_order: Vec::new(),
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
fn source_visible_badges_and_hidden_siren_surface_preserve_public_information() {
    let mut state = empty();
    put(&mut state, "pawn", Color::White, 5, 0);
    put(&mut state, "siren", Color::Black, 2, 4);
    let pawn = state.board[5][0].as_mut().unwrap();
    pawn.extra.insert(
        "holdoutPromotion".into(),
        json!({"readyTurn":120,"by":"private-owner"}),
    );
    pawn.extra.insert(
        "vipInvitation".into(),
        json!({"triggerTurn":4,"pieceId":"private-plan"}),
    );
    state.board[2][4]
        .as_mut()
        .unwrap()
        .extra
        .insert("hiddenFrom".into(), json!("white"));
    state.turns_taken = Sides::new(3, 2);
    let view = Position::from_state(state).unwrap().observe(Color::White);
    assert_eq!(
        view.board[5][0].as_ref().unwrap()["status"]["holdoutRemaining"],
        json!(118.0)
    );
    assert_eq!(
        view.board[5][0].as_ref().unwrap()["status"]["vipRemaining"],
        json!(1.0)
    );
    assert!(view.board[2][4].is_none());
    assert_eq!(
        view.public_state["boardMarks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|mark| mark["kind"] == "sirenAura")
            .count(),
        9
    );
    let json = serde_json::to_string(&view).unwrap();
    assert!(
        !json.contains("private-owner")
            && !json.contains("private-plan")
            && !json.contains("black-2-4")
    );
    crate::observation::validate_projection(&view).unwrap();
    let mut malformed = view.clone();
    malformed.public_state.insert(
        "armistice".into(),
        json!({"remaining":2,"pieceId":"private-id"}),
    );
    assert!(matches!(
        crate::observation::validate_projection(&malformed),
        Err(EngineError::InvalidState(_))
    ));
    let mut malformed = view.clone();
    malformed.board[5][0].as_mut().unwrap()["status"]["privateDeadline"] = json!(50);
    assert!(matches!(
        crate::observation::validate_projection(&malformed),
        Err(EngineError::InvalidState(_))
    ));
    // Semantic JSON integers remain equivalent after canonical transport.
    let mut canonical = view.clone();
    canonical.board[5][0].as_mut().unwrap()["status"]["holdoutRemaining"] = json!(118);
    crate::observation::validate_projection(&canonical).unwrap();
    let mut state = empty();
    state.mode = "draft".into();
    state.extra.insert("draft".into(), Value::Null);
    put(&mut state, "trickster", Color::White, 4, 3);
    state.board[4][3]
        .as_mut()
        .unwrap()
        .extra
        .insert("tricksterMoveType".into(), json!("wizard"));
    let before = state.observe(Color::White);
    assert!(
        before.board[4][3].as_ref().unwrap()["status"]
            .get("tricksterMovement")
            .is_none()
    );
    state.mode = "play".into();
    let own = state.observe(Color::White);
    assert_eq!(
        own.board[4][3].as_ref().unwrap()["status"]["tricksterMovement"],
        json!("wizard")
    );
    assert!(
        state.observe(Color::Black).board[4][3].as_ref().unwrap()["status"]
            .get("tricksterMovement")
            .is_none()
    );
    state.deck_slots.white.push(serde_json::from_value(json!({"id":"random-roulette","effect":"randomRoulette","instanceId":"public-card","revealed":{"privateSeed":71}})).unwrap());
    assert!(
        state.observe(Color::White).own_cards[0]
            .get("revealed")
            .is_none()
    );
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
fn raw_card_acceptance_does_not_expand_public_source_selection() {
    let mut state = empty();
    let mut piece = Piece::new("bigRook", Color::White, "large");
    piece.extra.insert("anchorRow".into(), json!(3));
    piece.extra.insert("anchorCol".into(), json!(3));
    for row in 3..5 {
        for col in 3..5 {
            state.board[row][col] = Some(piece.clone());
        }
    }
    let mut card: CardSlot = serde_json::from_value(
        crate::draft::definitions()
            .definitions
            .iter()
            .find(|c| c["id"] == "outpost")
            .unwrap()
            .clone(),
    )
    .unwrap();
    card.instance_id = "outpost-test".into();
    state.deck_slots.white[0] = card;
    let position = Position::from_state(state).unwrap();
    let original = position.to_json().unwrap();
    let payload = json!({"type":"card","color":"white","cardId":"outpost","cardInstanceId":"outpost-test","target":{"row":4,"col":4}});
    let raw = position.bind_payload(payload.clone()).unwrap();
    assert_eq!(
        position.public_intent(&raw).unwrap_err(),
        EngineError::IllegalAction
    );
    assert_eq!(
        position.bind_public_intent(payload).unwrap_err(),
        EngineError::IllegalAction
    );
    assert!(
        !position
            .legal_actions()
            .unwrap()
            .iter()
            .any(|a| a.target == raw.target && a.kind == ActionKind::Card)
    );
    assert_eq!(position.to_json().unwrap(), original);
    assert!(
        position.apply(&raw).unwrap().position.state().board[3][3]
            .as_ref()
            .unwrap()
            .flag("outpostProtected")
    );

    let mut state = GameState::new(kernel_config(), 37).unwrap();
    let mut card: CardSlot = serde_json::from_value(
        crate::draft::definitions()
            .definitions
            .iter()
            .find(|card| card["id"] == "nullification")
            .unwrap()
            .clone(),
    )
    .unwrap();
    card.instance_id = "nullification-test".into();
    state.deck_slots.white[0] = card;
    let position = Position::from_state(state).unwrap();
    let nullification = position
        .bind_payload(json!({"type":"card","color":"white",
        "cardId":"nullification","cardInstanceId":"nullification-test",
        "target":{"row":6,"col":0}}))
        .unwrap();
    let moved = position
        .apply(&movement(
            &position,
            Square { row: 6, col: 0 },
            Square { row: 4, col: 0 },
        ))
        .unwrap();
    let moved_public =
        serde_json::to_value(moved.position.try_observe(Color::Black).unwrap()).unwrap();
    assert!(
        !position
            .public_transition_compatible(&nullification, moved_public)
            .unwrap()
    );
    let activated = position.apply(&nullification).unwrap();
    let public =
        serde_json::to_value(activated.position.try_observe(Color::Black).unwrap()).unwrap();
    assert!(
        position
            .public_transition_compatible(&nullification, public.clone())
            .unwrap()
    );
    let weighted = position
        .apply_weighted_conditioned_public(&nullification, public, 91)
        .unwrap();
    assert_eq!(
        weighted.step.position.try_observe(Color::Black).unwrap(),
        activated.position.try_observe(Color::Black).unwrap()
    );
    assert_eq!(
        (
            weighted.source_probability,
            weighted.proposal_probability,
            weighted.importance_weight
        ),
        (1.0, 1.0, 1.0)
    );
    assert!(!position.state().deck_slots.white[0].used);

    // An actual semantic draw has p=q=1/N under an ordinary source proposal.
    // Its opaque plan/notation identities consume RNG without adding another
    // outcome factor, and the execution-only trace cannot leak into snapshots.
    let mut state = empty();
    put(&mut state, "king", Color::White, 7, 7);
    put(&mut state, "king", Color::Black, 0, 7);
    put(&mut state, "pawn", Color::White, 6, 0);
    put(&mut state, "pawn", Color::White, 6, 1);
    state.extra.insert("draftDelete".into(), json!(false));
    let mut card: CardSlot = serde_json::from_value(
        crate::draft::definitions()
            .definitions
            .iter()
            .find(|card| card["id"] == "otherworld")
            .unwrap()
            .clone(),
    )
    .unwrap();
    card.instance_id = "otherworld-test".into();
    state.deck_slots.white[0] = card;
    let position = Position::from_state(state).unwrap();
    let original = position.to_json().unwrap();
    let action = position
        .bind_payload(json!({"type":"card","color":"white",
        "cardId":"otherworld","cardInstanceId":"otherworld-test"}))
        .unwrap();
    let activated = position.apply(&action).unwrap();
    let observed =
        serde_json::to_value(activated.position.try_observe(Color::Black).unwrap()).unwrap();
    let proposal = position
        .apply_weighted_conditioned_public(&action, observed, 91)
        .unwrap();
    assert_eq!(
        (
            proposal.source_probability,
            proposal.proposal_probability,
            proposal.importance_weight
        ),
        (0.5, 0.5, 1.0)
    );
    assert!(
        proposal
            .step
            .position
            .state()
            .semantic_chance_probability
            .is_none()
    );
    assert_eq!(position.to_json().unwrap(), original);

    // The source's availability simulation consumes both random draws while
    // discarding its board, plan, animation and semantic outcome trace.
    let mut probe = position.state().clone();
    let before_board = probe.board.clone();
    let before_cursor = probe.rng.cursor;
    probe.semantic_chance_probability = Some(1.0);
    assert!(crate::transition::available_card_action(&mut probe, Color::White).unwrap());
    assert_eq!(probe.rng.cursor, before_cursor + 2);
    assert_eq!(probe.board, before_board);
    assert_eq!(probe.semantic_chance_probability, Some(1.0));
    let mut no_draft_cards = position.state().clone();
    no_draft_cards
        .extra
        .insert("draftDelete".into(), json!(true));
    assert!(!crate::transition::available_card_action(&mut no_draft_cards, Color::White).unwrap());
    assert_eq!(no_draft_cards.rng.cursor, before_cursor);
    assert!(
        probe.extra["pendingOtherworld"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let before_move = &activated.position;
    let moved = before_move
        .apply(&movement(
            before_move,
            Square { row: 7, col: 7 },
            Square { row: 7, col: 6 },
        ))
        .unwrap();
    assert_eq!(
        moved.position.state().extra["pendingOtherworld"][0]["remainingHalfTurns"],
        // The frozen source schedules 28 half-turns and the next completed
        // board move advances that countdown once.
        json!(27)
    );
    let mut due = activated.position.state().clone();
    due.extra.get_mut("pendingOtherworld").unwrap()[0]["remainingHalfTurns"] = json!(1);
    let due = Position::from_state(due).unwrap();
    let snapshot = due.to_json().unwrap();
    assert!(matches!(
        due.apply(&movement(
            &due,
            Square { row: 7, col: 7 },
            Square { row: 7, col: 6 }
        ))
        .unwrap_err(),
        EngineError::UnsupportedFeature(_)
    ));
    assert_eq!(due.to_json().unwrap(), snapshot);

    let attacker = Piece::new("darkWizard", Color::White, "royal-attacker");
    let mut target = Piece::new("queen", Color::Black, "inactive-regency");
    target.extra.insert("regencyHeir".into(), json!(true));
    target.extra.insert("nullification".into(), json!(true));
    assert!(!crate::movement::can_capture(
        position.state(),
        &attacker,
        &target
    ));
    target.kind = "scarecrow".into();
    assert!(crate::movement::can_capture(
        position.state(),
        &attacker,
        &target
    ));
}

#[test]
fn ordered_card_clicks_bind_without_materializing_incomplete_action_lists() {
    let mut state = empty();
    let first = Square { row: 3, col: 2 };
    let second = Square { row: 4, col: 5 };
    put(&mut state, "pawn", Color::White, first.row, first.col);
    put(&mut state, "pawn", Color::White, second.row, second.col);
    let mut card: CardSlot = serde_json::from_value(
        crate::draft::definitions()
            .definitions
            .iter()
            .find(|candidate| candidate["id"] == "cleanup")
            .unwrap()
            .clone(),
    )
    .unwrap();
    card.instance_id = "cleanup-order-test".into();
    state.deck_slots.white[0] = card;
    let position = Position::from_state(state).unwrap();
    let before = position.to_json().unwrap();
    let reversed = json!({
        "type":"card","color":"white","cardId":"cleanup",
        "cardInstanceId":"cleanup-order-test",
        "target":{"selections":[second,first]}
    });
    let bound = position.bind_public_intent(reversed.clone()).unwrap();
    assert_eq!(position.public_intent(&bound).unwrap(), reversed);
    assert!(matches!(
        position.legal_actions(),
        Err(EngineError::UnsupportedFeature(_))
    ));

    let duplicate = json!({
        "type":"card","color":"white","cardId":"cleanup",
        "cardInstanceId":"cleanup-order-test",
        "target":{"selections":[first,first]}
    });
    assert!(position.bind_payload(duplicate.clone()).is_ok());
    assert_eq!(
        position.bind_public_intent(duplicate).unwrap_err(),
        EngineError::IllegalAction
    );
    assert_eq!(position.to_json().unwrap(), before);
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
    let mut external = source;
    external["gameoverReplayPending"] = json!(true);
    external["semanticChanceProbability"] = json!(0.125);
    let imported = Position::from_snapshot_value(external.clone()).unwrap();
    assert_eq!(imported.export_state().unwrap(), external);
    assert!(!imported.state().gameover_replay_pending);
    assert!(imported.state().semantic_chance_probability.is_none());
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
        source_order: Vec::new(),
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
    state.extra.insert(
        "deathmatch".into(),
        json!({"active":true,
        "intervalHalfTurns":6,"halfTurnsSinceProgress":2,"progressThisTurn":false}),
    );
    let early = state.try_observe(Color::White).unwrap();
    assert_eq!(
        early.public_state["deathmatchStatus"],
        json!({"active":true,"warning":false})
    );
    state.extra.get_mut("deathmatch").unwrap()["halfTurnsSinceProgress"] = json!(4);
    let warning = state.try_observe(Color::White).unwrap();
    assert_eq!(
        warning.public_state["deathmatchStatus"],
        json!({"active":true,"warning":true})
    );
    assert_ne!(early.information_state_key, warning.information_state_key);
    assert_eq!(
        state.try_observe(Color::Black).unwrap().public_state["deathmatchStatus"],
        warning.public_state["deathmatchStatus"]
    );
    state.extra.get_mut("deathmatch").unwrap()["halfTurnsSinceProgress"] = json!(5);
    assert_eq!(
        state
            .try_observe(Color::White)
            .unwrap()
            .information_state_key,
        warning.information_state_key
    );
    state.extra.get_mut("deathmatch").unwrap()["progressThisTurn"] = json!(true);
    assert!(
        !state.try_observe(Color::White).unwrap().public_state["deathmatchStatus"]["warning"]
            .as_bool()
            .unwrap()
    );
    state.extra.insert("deathmatch".into(), Value::Null);
    state.turns_taken = Sides {
        white: 45,
        black: 45,
        white_first: true,
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
    // Choice-window identity stays in exact execution history. Its random
    // spelling cannot change either viewer's public trace or information key.
    let mut private_choice = next.state().clone();
    private_choice.history[0]["action"] = json!({"type":"trolleyChoice","color":"black","doomedIndex":0,"windowId":"private-window-one"});
    let one = Position::from_state(private_choice.clone()).unwrap();
    private_choice.history[0]["action"]["windowId"] = json!("private-window-two");
    let two = Position::from_state(private_choice).unwrap();
    for viewer in [Color::White, Color::Black] {
        assert_eq!(one.observe(viewer), two.observe(viewer));
        assert!(one.observe(viewer).history[0].get("windowId").is_none());
    }
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
    let second_action = first.position.legal_actions().unwrap()[0].clone();
    let second = first.position.apply(&second_action).unwrap();
    for viewer in [Color::White, Color::Black] {
        let expected = second.position.try_observe(viewer).unwrap();
        let proposal = first
            .position
            .apply_weighted_conditioned_public(
                &second_action,
                serde_json::to_value(&expected).unwrap(),
                71,
            )
            .unwrap();
        assert_eq!(
            proposal.step.position.try_observe(viewer).unwrap(),
            expected
        );
        assert_eq!(
            (
                proposal.source_probability,
                proposal.proposal_probability,
                proposal.importance_weight
            ),
            (1.0, 1.0, 1.0)
        );
    }
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
fn explicit_rules_version_preserves_v6_and_refuses_unported_v7() {
    let source = Position::new_game(kernel_config(), 11).unwrap();
    let raw = source.export_state().unwrap();
    assert!(raw.get("rulesetId").is_none());
    let imported =
        Position::from_snapshot_value_with_rules_version(raw.clone(), RULES_VERSION_V6).unwrap();
    assert_eq!(imported.rules_version(), RULES_VERSION_V6);
    assert_eq!(imported.export_state().unwrap(), raw);
    assert_eq!(
        imported.try_observe(Color::White).unwrap(),
        source.try_observe(Color::White).unwrap()
    );

    assert!(matches!(
        Position::from_snapshot_value_with_rules_version(raw.clone(), RULES_VERSION_V7),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        Position::from_snapshot_value_with_rules_version(raw.clone(), "unknown-rules"),
        Err(EngineError::InvalidConfig(_))
    ));
    let mut marked = raw;
    marked["rulesetId"] = json!(RULES_VERSION_V7);
    assert!(matches!(
        Position::from_snapshot_value_with_rules_version(marked.clone(), RULES_VERSION_V6),
        Err(EngineError::InvalidState(_))
    ));
    assert!(matches!(
        Position::from_snapshot_value_with_rules_version(marked, RULES_VERSION_V7),
        Err(EngineError::UnsupportedFeature(_))
    ));
    let mut state = source.state().clone();
    state.ruleset_id = RULES_VERSION_V7.into();
    assert!(matches!(
        Position::from_state(state),
        Err(EngineError::UnsupportedFeature(_))
    ));
}

#[test]
fn v7_public_history_projection_rejects_missing_viewer_without_panicking() {
    let mut state =
        crate::draft::initialize_for_ruleset(GameConfig::default(), 37, RULES_VERSION_V7).unwrap();
    state.history.push(json!({"public":{"white":{}}}));
    assert!(matches!(
        state.try_observe(Color::Black),
        Err(EngineError::InvalidState(message))
            if message.contains("black public projection")
    ));
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
        source_order: Vec::new(),
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
    let sampled = Position::sample_initial_public(
        GameConfig::default(),
        serde_json::to_value(initial.try_observe(Color::White).unwrap()).unwrap(),
        71,
    )
    .unwrap();
    let choice_id = initial.state().extra["draft"]["choices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["id"] == "corner-kick")
        .unwrap()["instanceId"]
        .clone();
    let action = sampled
        .bind_payload(json!({"type":"draftPick","color":"white","cardInstanceId":choice_id}))
        .unwrap();
    let expected = black_offer.try_observe(Color::Black).unwrap();
    let spectator = Position::sample_initial_public(
        GameConfig::default(),
        serde_json::to_value(initial.try_observe(Color::Black).unwrap()).unwrap(),
        71,
    )
    .unwrap();
    let previous = spectator.try_observe(Color::Black).unwrap();
    let proposal = spectator
        .condition_hidden_opening_draft(serde_json::to_value(&expected).unwrap(), 117)
        .unwrap();
    assert_eq!(
        proposal.position.try_observe(Color::Black).unwrap(),
        previous
    );
    assert!(proposal.source_probability.is_finite() && proposal.source_probability > 0.0);
    assert!(proposal.proposal_probability.is_finite() && proposal.proposal_probability > 0.0);
    assert_eq!(
        proposal.importance_weight,
        proposal.source_probability / proposal.proposal_probability
    );
    let actions = proposal.position.legal_actions().unwrap();
    let compatible = actions
        .iter()
        .filter(|action| {
            proposal
                .position
                .public_transition_compatible(action, serde_json::to_value(&expected).unwrap())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(compatible.len(), 1);
    let conditioned_hidden = proposal
        .position
        .apply_conditioned_public(compatible[0], serde_json::to_value(&expected).unwrap(), 93)
        .unwrap();
    assert_eq!(
        conditioned_hidden
            .position
            .try_observe(Color::Black)
            .unwrap(),
        expected
    );
    assert_eq!(spectator.try_observe(Color::Black).unwrap(), previous);
    let conditioned = sampled
        .apply_conditioned_public(&action, serde_json::to_value(&expected).unwrap(), 93)
        .unwrap();
    assert_eq!(
        conditioned.position.try_observe(Color::Black).unwrap(),
        expected
    );
    assert_eq!(conditioned.position.state().rng, RngState::seeded(93));
    assert_ne!(conditioned.position.state().rng, black_offer.state().rng);
    assert!(
        sampled
            .state()
            .deck_slots
            .white
            .iter()
            .all(|card| card.vacant)
    );
    let mut weighted = None;
    for seed in 0..8 {
        match sampled.apply_weighted_conditioned_public(
            &action,
            serde_json::to_value(&expected).unwrap(),
            seed,
        ) {
            Ok(proposal) => {
                weighted = Some(proposal);
                break;
            }
            Err(EngineError::ConditioningMismatch(_)) => {}
            Err(error) => panic!("unexpected weighted transition error: {error}"),
        }
    }
    let weighted = weighted.expect("bounded proposal includes a supported balancing component");
    assert_eq!(
        weighted.step.position.try_observe(Color::Black).unwrap(),
        expected
    );
    assert!(weighted.source_probability > 0.0 && weighted.source_probability < 1.0);
    assert!(weighted.proposal_probability > 0.0 && weighted.proposal_probability <= 1.0);
    assert_eq!(
        weighted.importance_weight,
        weighted.source_probability / weighted.proposal_probability
    );
    assert_ne!(weighted.importance_weight, 1.0);
    let unobserved = sampled
        .apply_weighted_conditioned_public(
            &action,
            serde_json::to_value(black_offer.try_observe(Color::White).unwrap()).unwrap(),
            0,
        )
        .unwrap();
    assert_eq!(
        unobserved.step.position.try_observe(Color::White).unwrap(),
        black_offer.try_observe(Color::White).unwrap()
    );
    assert!(unobserved.source_probability > 0.0 && unobserved.source_probability < 1.0);
    assert_eq!(
        unobserved.source_probability,
        unobserved.proposal_probability
    );
    assert_eq!(unobserved.importance_weight, 1.0);
    assert_eq!(
        sampled.try_observe(Color::White).unwrap(),
        initial.try_observe(Color::White).unwrap()
    );
    let play = choose(&black_offer, "reaper");
    let white_play = play.try_observe(Color::White).unwrap();
    let black_proposal = black_offer
        .condition_hidden_opening_draft(serde_json::to_value(&white_play).unwrap(), 117)
        .unwrap();
    assert_eq!(
        black_proposal.position.try_observe(Color::White).unwrap(),
        black_offer.try_observe(Color::White).unwrap()
    );
    let matching = black_proposal
        .position
        .legal_actions()
        .unwrap()
        .into_iter()
        .filter(|action| {
            black_proposal
                .position
                .public_transition_compatible(action, serde_json::to_value(&white_play).unwrap())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1);
    let proposed_play = black_proposal
        .position
        .apply_conditioned_public(&matching[0], serde_json::to_value(&white_play).unwrap(), 93)
        .unwrap();
    assert_eq!(
        proposed_play.position.try_observe(Color::White).unwrap(),
        white_play
    );
    assert_eq!(
        black_proposal.importance_weight,
        black_proposal.source_probability / black_proposal.proposal_probability
    );
    let weighted_play = black_proposal
        .position
        .apply_weighted_conditioned_public(
            &matching[0],
            serde_json::to_value(&white_play).unwrap(),
            93,
        )
        .unwrap();
    assert_eq!(
        weighted_play
            .step
            .position
            .try_observe(Color::White)
            .unwrap(),
        white_play
    );
    assert_eq!(
        (
            weighted_play.source_probability,
            weighted_play.proposal_probability,
            weighted_play.importance_weight
        ),
        (1.0, 1.0, 1.0)
    );
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

    // A CHAOS proposal covers the actual two-card acquisition and the entire
    // newly exposed offer. Both spectator and acting-player public histories
    // must match, while their previous public frame is immutable.
    let chaos_config = GameConfig {
        game_style: "chaos".into(),
        ..GameConfig::default()
    };
    let chaos = Position::new_game(chaos_config.clone(), 37).unwrap();
    let choice = chaos.legal_actions().unwrap()[0].clone();
    let next = chaos.apply(&choice).unwrap().position;
    for viewer in [Color::White, Color::Black] {
        let previous = chaos.try_observe(viewer).unwrap();
        let expected = next.try_observe(viewer).unwrap();
        let sampled = Position::sample_initial_public(
            chaos_config.clone(),
            serde_json::to_value(&previous).unwrap(),
            71,
        )
        .unwrap();
        let candidate = if viewer == Color::Black {
            let hidden = sampled
                .condition_hidden_opening_draft(serde_json::to_value(&expected).unwrap(), 117)
                .unwrap();
            assert_eq!(hidden.position.try_observe(viewer).unwrap(), previous);
            assert!(hidden.source_probability > 0.0 && hidden.source_probability < 1.0);
            let action = hidden
                .position
                .legal_actions()
                .unwrap()
                .into_iter()
                .find(|action| {
                    hidden
                        .position
                        .public_transition_compatible(
                            action,
                            serde_json::to_value(&expected).unwrap(),
                        )
                        .unwrap()
                })
                .unwrap();
            (hidden.position, action)
        } else {
            let action = sampled
                .bind_public_intent(chaos.public_intent(&choice).unwrap())
                .unwrap();
            (sampled, action)
        };
        let mut success = None;
        for seed in 0..16 {
            match candidate.0.apply_weighted_conditioned_public(
                &candidate.1,
                serde_json::to_value(&expected).unwrap(),
                seed,
            ) {
                Ok(proposal) => {
                    success = Some(proposal);
                    break;
                }
                Err(EngineError::ConditioningMismatch(_)) => {}
                Err(error) => panic!("unexpected CHAOS proposal error: {error}"),
            }
        }
        let proposal = success.expect("finite source-valid CHAOS conditional trace");
        assert_eq!(
            proposal.step.position.try_observe(viewer).unwrap(),
            expected
        );
        assert_eq!(candidate.0.try_observe(viewer).unwrap(), previous);
        assert!(proposal.source_probability > 0.0 && proposal.source_probability < 1.0);
        if viewer == Color::White {
            assert_eq!(proposal.source_probability, proposal.proposal_probability);
            assert_eq!(proposal.importance_weight, 1.0);
        } else {
            assert!(proposal.proposal_probability > 0.0 && proposal.proposal_probability <= 1.0);
            assert_eq!(
                proposal.importance_weight,
                proposal.source_probability / proposal.proposal_probability
            );
        }
    }
}

#[test]
fn royal_sound_probes_execute_owned_captures_and_source_protection_window() {
    let mut state = empty();
    put(&mut state, "king", Color::White, 6, 6);
    put(&mut state, "king", Color::Black, 0, 7);
    put(&mut state, "bishop", Color::Black, 4, 4);
    let original = state.clone();
    assert!(crate::threat::has_royal_capture(&state, Color::White).unwrap());
    assert_eq!(state, original);
    let king = state.board[6][6].as_mut().unwrap();
    king.extra.insert("protected".into(), json!(true));
    king.extra.insert(
        "lastResistance".into(),
        json!({"remaining":2,"previousProtected":false}),
    );
    assert!(!crate::threat::has_royal_capture(&state, Color::White).unwrap());
    state.board[6][6]
        .as_mut()
        .unwrap()
        .extra
        .get_mut("lastResistance")
        .unwrap()["remaining"] = json!(1);
    assert!(crate::threat::has_royal_capture(&state, Color::White).unwrap());
    state.extra.insert(
        "lastMove".into(),
        json!({"soundName":"moveSelf","soundColor":"white"}),
    );
    let rng = state.rng.clone();
    crate::threat::play_move_sound(&mut state, "moveSelf", Color::White).unwrap();
    assert_eq!(state.extra["lastMove"]["soundName"], "checkDanger");
    assert_eq!(state.rng, rng);
    state.board[4][4]
        .as_mut()
        .unwrap()
        .extra
        .insert("hiddenFrom".into(), json!("white"));
    assert!(!crate::threat::has_royal_capture(&state, Color::White).unwrap());
    state.extra.insert(
        "delayedHazards".into(),
        json!([{"cells":[{"row":6,"col":6}]}]),
    );
    assert!(matches!(
        crate::threat::has_royal_capture(&state, Color::White),
        Err(EngineError::UnsupportedFeature(_))
    ));

    let mut fianchetto = empty();
    put(&mut fianchetto, "bishop", Color::Black, 0, 7);
    put(&mut fianchetto, "pawn", Color::White, 5, 3);
    fianchetto.set_flag("fianchetto", Color::Black, true);
    let pawn = fianchetto.board[5][3].as_ref().unwrap();
    assert!(!crate::movement::fianchetto_destination_allowed(
        &fianchetto,
        pawn,
        Square { row: 5, col: 3 },
        &[Square { row: 4, col: 3 }]
    ));
    assert!(
        crate::movement::piece_moves(&fianchetto, pawn, Square { row: 5, col: 3 })
            .unwrap()
            .is_empty()
    );
    assert!(crate::movement::fianchetto_destination_allowed(
        &fianchetto,
        pawn,
        Square { row: 4, col: 3 },
        &[Square { row: 3, col: 4 }]
    ));
}
