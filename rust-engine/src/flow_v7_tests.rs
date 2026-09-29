use super::*;

#[test]
fn source_v7_normal_and_chaos_milestones_match_full_state_and_rng() {
    use sha2::{Digest, Sha256};
    let digest = |state: &GameState| {
        let mut value = serde_json::to_value(state).unwrap();
        for key in ["rulesetId", "rng", "history"] {
            value.as_object_mut().unwrap().remove(key);
        }
        format!("{:x}", Sha256::digest(serde_jcs::to_vec(&value).unwrap()))
    };
    // SHA-pinned main-OahWs0tU.js seed 37 first-play states with a synthetic
    // completed-turn count. These are direct maybeStartMilestoneDraft probes,
    // not evidence that the intervening 10/20 natural turns are supported.
    for (style, phase, turns, middle_done, expected_digest, cursor, rng_state) in [
        (
            "normal",
            "MIDDLE",
            10,
            false,
            "448d844ac5ddf083b31e9dfdac39c98f7328cbc572a85178f7dcbd66a5b7bafd",
            278,
            4_230_071_635,
        ),
        (
            "normal",
            "END",
            20,
            true,
            "ba80d75b821c1424bcb803c4944266fa3655abbafe33a6bcd8765b438265fa02",
            306,
            3_997_385_423,
        ),
        (
            "chaos",
            "MIDDLE",
            10,
            false,
            "f59fdc0af589a67ed10ce1d104e83d3659d190630504e34b3637482baf47eb03",
            526,
            3_751_996_747,
        ),
        (
            "chaos",
            "END",
            20,
            true,
            "0c3139e1a5aa15fd3fcd49f7cafe50b9263960daa5559f893766578d682c882b",
            582,
            768_950_659,
        ),
    ] {
        let mut opening = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            37,
            RULES_VERSION_V7,
        )
        .unwrap();
        for _ in 0..2 {
            let action = crate::draft::legal_actions(&opening).unwrap().remove(0);
            crate::transition::apply(&mut opening, &action).unwrap();
            crate::replay::canonicalize_position_frames(&mut opening).unwrap();
        }
        assert_eq!(opening.mode, "play");
        let mut state = opening;
        state.turns_taken = Sides::new(turns, turns);
        state.move_count = turns * 2;
        state.full_move = turns + 1;
        state
            .extra
            .insert("middleDraftDone".into(), json!(middle_done));
        state.extra.insert("endDraftDone".into(), json!(false));
        if style == "normal" && phase == "MIDDLE" {
            // Direct checkRepetitionOrStarLimit on this same seed-37 opening
            // state, with only the completed-turn count and limit settings
            // injected, matches the frozen v7 client over the whole state.
            let mut overtime = state.clone();
            overtime.extra.insert("starWinLimit".into(), json!(10));
            overtime
                .extra
                .insert("deathmatchEnabled".into(), json!(true));
            overtime
                .extra
                .insert("deathmatchLimitTurns".into(), json!(1.6));
            assert!(!check_termination(&mut overtime).unwrap());
            assert_eq!(
                digest(&overtime),
                "c2af18ebaf3810e8e960df46a7c28418677abf7dd6ec1a8382e1c686ea7789c8",
                "normal MIDDLE overtime entry full state"
            );
            assert_eq!(overtime.rng.cursor, 216);
            assert_eq!(overtime.rng.state, 1_469_516_477);
        }
        assert!(
            maybe_start_milestone_draft(&mut state).unwrap(),
            "{style} {phase}"
        );
        assert_eq!(state.mode, "draft");
        assert_eq!(state.turn, Color::White);
        assert_eq!(state.extra["draft"]["phase"], phase);
        assert_eq!(
            digest(&state),
            expected_digest,
            "{style} {phase} full state"
        );
        assert_eq!(state.rng.cursor, cursor, "{style} {phase} RNG cursor");
        assert_eq!(state.rng.state, rng_state, "{style} {phase} RNG state");
    }
}

#[test]
fn v7_milestone_skips_source_exclusions_and_rejects_unported_return_effects() {
    let mut state = play_state(RULES_VERSION_V7);
    state.turns_taken = Sides::new(10, 10);
    state.extra.insert("draftDelete".into(), json!(false));
    state.extra.insert("middleDraftDone".into(), json!(false));
    state.extra.insert("endDraftDone".into(), json!(false));
    state.extra.insert("gameStyle".into(), json!("normal"));
    let mut below_threshold = state.clone();
    below_threshold.turns_taken.black = 9;
    let before = below_threshold.clone();
    assert!(!maybe_start_milestone_draft(&mut below_threshold).unwrap());
    assert_eq!(below_threshold, before);

    let mut grand = state.clone();
    grand.extra.insert("gameStyle".into(), json!("grand"));
    let before = grand.clone();
    assert!(!maybe_start_milestone_draft(&mut grand).unwrap());
    assert_eq!(grand, before);

    let mut forced = state.clone();
    place_king(&mut forced, Color::White, 7, 4);
    forced.board[7][4]
        .as_mut()
        .unwrap()
        .extra
        .insert("thiefSecondMove".into(), json!(true));
    let before = forced.clone();
    assert!(!maybe_start_milestone_draft(&mut forced).unwrap());
    assert_eq!(forced, before);

    state.extra.insert(
        "judgmentExiles".into(),
        json!([{"returnPhase":"MIDDLE","piece":{"color":"white","type":"pawn"}}]),
    );
    let before = state.clone();
    assert!(matches!(
        maybe_start_milestone_draft(&mut state),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert_eq!(state, before);
}

fn play_state(rules_version: &str) -> GameState {
    let mut state = GameState::new(
        GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        },
        17,
    )
    .expect("local deterministic game");
    // The v7 Position execution guard remains closed. These focused tests
    // exercise only the source-identical flow kernel on an owned rule state.
    state.ruleset_id = rules_version.into();
    state.board = vec![vec![None; 8]; 8];
    state.extra.insert("logs".into(), json!([]));
    state.extra.insert(
        "positionCounts".into(),
        json!({"__simType":"Map","entries":[]}),
    );
    state
}

fn place_king(state: &mut GameState, color: Color, row: usize, col: usize) {
    state.board[row][col] = Some(Piece::new(
        "king",
        color,
        format!("{}-king", color.as_str()),
    ));
}

#[test]
fn relay_first_play_no_action_probe_uses_source_witness_after_card_trial() {
    // Source seed 19 normal: second white offer is Relay and the first black
    // offer enters play. Relay adds swap moves, but a2-a3 alone proves that
    // checkNoActionLoss must not declare a loss after the used card probe.
    let mut state =
        crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7).unwrap();
    let white_pick = crate::draft::legal_actions(&state).unwrap().remove(1);
    crate::draft::apply_pick(&mut state, &white_pick).unwrap();
    crate::replay::canonicalize_position_frames(&mut state).unwrap();
    let black_pick = crate::draft::legal_actions(&state).unwrap().remove(0);
    crate::transition::apply(&mut state, &black_pick).unwrap();
    assert_eq!(state.mode, "play");
    assert_eq!(state.turn, Color::White);
    let card = state.deck_slots.white[0].clone();
    assert_eq!(card.id, "relay");
    let action = crate::Action::card(Color::White, &card, None);
    // Position's v7 public-action gate remains closed. Construct only the
    // source-probed post-card state needed by this no-action flow check.
    crate::card_effects::apply(&mut state, &card, &action).unwrap();
    state.deck_slots.white[0].used = true;
    state.deck_slots.white[0].extra.insert(
        "usedAt".into(),
        json!(crate::draft::frozen_timestamp_for_ruleset(RULES_VERSION_V7).unwrap()),
    );
    state.cards_used_this_turn.white = 1;
    note_card_event(&mut state).unwrap();
    crate::replay::queue_card(&mut state, Color::White, &card).unwrap();
    crate::replay::record(&mut state, "card").unwrap();
    assert_eq!(state.rng.cursor, 217);
    assert_eq!(state.extra["replayEventNonce"], 3);
    assert_eq!(state.extra["relay"], json!({"white":true,"black":false}));
    let before = state.clone();
    assert_eq!(
        crate::movement::v7_first_play_has_legal_move_witness(&state, Color::White).unwrap(),
        Some(true)
    );
    assert!(!check_no_action_loss(&mut state).unwrap());
    assert_eq!(state, before);
}

#[test]
fn end_game_uses_the_selected_frozen_client_time_and_keeps_v6_distinct() {
    let mut v6 = play_state(RULES_VERSION_V6);
    let mut v7 = play_state(RULES_VERSION_V7);
    end_game(&mut v6, Some(Color::White), "백 킹이 잡혔습니다.").unwrap();
    end_game(&mut v7, Some(Color::White), "백 킹이 잡혔습니다.").unwrap();
    assert_eq!(v6.extra["replayEndedAt"], "2026-09-27T14:37:10.842Z");
    assert_eq!(v7.extra["replayEndedAt"], "2026-09-28T07:41:32.828Z");
    assert_eq!(v7.extra["replayEndReason"], "백 킹이 잡혔습니다.");
    assert_eq!(v7.extra["logs"][0], "백 승리: 백 킹이 잡혔습니다.");
    assert_eq!(v7.mode, "gameover");
    assert_eq!(v7.winner.as_deref(), Some("white"));
    assert_eq!(v7.extra["legalMoves"], json!([]));
}

#[test]
fn active_clock_uses_its_own_ruleset_freeze_time() {
    let mut v6 = play_state(RULES_VERSION_V6);
    let mut v7 = play_state(RULES_VERSION_V7);
    for state in [&mut v6, &mut v7] {
        state.extra.insert(
            "clock".into(),
            json!({"enabled":true,"whiteMs":60000,"blackMs":60000,"runningColor":null,"lastStartedAt":null,"incrementMs":10000}),
        );
        start_clock(state).unwrap();
        assert_eq!(state.extra["clock"]["runningColor"], "white");
    }
    assert_eq!(
        v6.extra["clock"]["lastStartedAt"],
        json!(crate::draft::frozen_timestamp_for_ruleset(RULES_VERSION_V6).unwrap())
    );
    assert_eq!(
        v7.extra["clock"]["lastStartedAt"],
        json!(crate::draft::frozen_timestamp_for_ruleset(RULES_VERSION_V7).unwrap())
    );
    assert_ne!(
        v6.extra["clock"]["lastStartedAt"],
        v7.extra["clock"]["lastStartedAt"]
    );
}

#[test]
fn repetition_key_uses_derived_board_extent_and_one_piece_identity() {
    let mut state = play_state(RULES_VERSION_V7);
    state.board = vec![vec![None; 5]; 3];
    let large = Piece::new("colossus", Color::White, "large-1");
    state.board[2][3] = Some(large.clone());
    state.board[2][4] = Some(large);
    assert_eq!(record_position(&mut state).unwrap(), 1);
    assert_eq!(record_position(&mut state).unwrap(), 2);
    let entries = state.extra["positionCounts"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0][0], "white|0|2,3:white:colossus:::0");
}

#[test]
fn repetition_and_star_limit_write_the_exact_terminal_reason() {
    let mut repeated = play_state(RULES_VERSION_V7);
    place_king(&mut repeated, Color::White, 7, 7);
    place_king(&mut repeated, Color::Black, 0, 0);
    assert!(!check_termination(&mut repeated).unwrap());
    assert!(!check_termination(&mut repeated).unwrap());
    assert!(check_termination(&mut repeated).unwrap());
    assert_eq!(repeated.result(), Some(GameResult::Draw));
    assert_eq!(
        repeated.extra["replayEndReason"],
        "3회 동형반복: 별 합계가 같아 무승부입니다. (0 : 0)"
    );

    let mut limit = play_state(RULES_VERSION_V7);
    limit.turns_taken.white = 2;
    limit.turns_taken.black = 2;
    limit.extra.insert("starWinLimit".into(), json!(2));
    limit.extra.insert("deathmatchEnabled".into(), json!(false));
    assert!(check_star_limit(&mut limit).unwrap());
    assert_eq!(
        limit.extra["replayEndReason"],
        "2수: 별 합계가 같아 무승부입니다. (0 : 0)"
    );
}

#[test]
fn v7_deathmatch_entry_normalizes_the_stored_turn_limit() {
    // Frozen main-OahWs0tU.js startDeathmatch calls
    // normalizeDeathmatchLimitTurns and writes its result to the state before
    // deriving intervalHalfTurns. These synthetic limit-entry probes do not
    // claim that the intervening two natural turns are executable in Rust.
    for (input, normalized, interval) in [
        (json!(1.6), 2, 4),
        (json!("4"), 4, 8),
        (Value::Null, 10, 20),
    ] {
        let mut state = play_state(RULES_VERSION_V7);
        state.turns_taken = Sides::new(2, 2);
        state.extra.insert("starWinLimit".into(), json!(2));
        state.extra.insert("deathmatchEnabled".into(), json!(true));
        state.extra.insert("deathmatchLimitTurns".into(), input);
        let rng = state.rng.clone();
        assert!(!check_star_limit(&mut state).unwrap());
        assert_eq!(state.mode, "play");
        assert_eq!(state.extra["deathmatchLimitTurns"], normalized);
        assert_eq!(state.extra["deathmatch"]["intervalHalfTurns"], interval);
        assert_eq!(state.extra["deathmatch"]["startedAtTurn"], 2);
        assert_eq!(state.extra["endPhaseStartMove"], 2);
        assert_eq!(state.rng, rng);
        assert_eq!(
            state.extra["logs"].as_array().unwrap().last().unwrap(),
            &json!(format!(
                "연장전 시작: 2수 이후 {normalized}수 동안 폰 이동, 포획, 액티브 카드 사용이 없으면 별이 더 적은 쪽이 승리합니다."
            ))
        );
    }

    let mut v6 = play_state(RULES_VERSION_V6);
    v6.turns_taken = Sides::new(2, 2);
    v6.extra.insert("starWinLimit".into(), json!(2));
    v6.extra.insert("deathmatchEnabled".into(), json!(true));
    v6.extra.insert("deathmatchLimitTurns".into(), json!(1.6));
    assert!(!check_star_limit(&mut v6).unwrap());
    assert_eq!(v6.extra["deathmatchLimitTurns"], 1.6);
    assert_eq!(v6.extra["deathmatch"]["intervalHalfTurns"], 20);

    let mut out_of_range = play_state(RULES_VERSION_V7);
    out_of_range.turns_taken = Sides::new(2, 2);
    out_of_range.extra.insert("starWinLimit".into(), json!(2));
    out_of_range
        .extra
        .insert("deathmatchEnabled".into(), json!(true));
    out_of_range
        .extra
        .insert("deathmatchLimitTurns".into(), json!(1_u64 << 52));
    let before = out_of_range.clone();
    assert!(matches!(
        check_star_limit(&mut out_of_range),
        Err(EngineError::InvalidState(_))
    ));
    assert_eq!(out_of_range, before);
}

#[test]
fn overtime_finishes_only_at_black_boundary_and_keeps_unprefixed_reason() {
    let mut state = play_state(RULES_VERSION_V7);
    state.extra.insert(
        "deathmatch".into(),
        json!({"active":true,"startedAtTurn":1,"halfTurnsSinceProgress":0,"intervalHalfTurns":2,"progressThisTurn":false,"warningKey":""}),
    );
    assert!(!tick_deathmatch(&mut state, Color::White).unwrap());
    assert_eq!(state.mode, "play");
    assert!(tick_deathmatch(&mut state, Color::Black).unwrap());
    assert_eq!(
        state.extra["replayEndReason"],
        "별 합계가 같아 무승부입니다. (0 : 0)"
    );
}

#[test]
fn no_action_probe_refuses_a_football_false_loss_without_changing_v6() {
    let mut v7 = play_state(RULES_VERSION_V7);
    v7.board[4][4] = Some(Piece::new("football", PieceColor::Neutral, "ball"));
    let before = v7.clone();
    // The v7 movement surface is still globally closed, so it may reject
    // before the football-specific final-verdict guard is reached. Both
    // paths must refuse to report a loss from an incomplete legal search.
    assert!(matches!(
        check_no_action_loss(&mut v7),
        Err(EngineError::UnsupportedFeature(_))
    ));
    // The unsupported result is not a terminal verdict. The outer Position
    // transition owns rollback of any speculative card-probe RNG.
    assert_eq!(v7.mode, before.mode);
    assert_eq!(v7.winner, before.winner);

    let mut v6 = play_state(RULES_VERSION_V6);
    v6.board[4][4] = Some(Piece::new("football", PieceColor::Neutral, "ball"));
    assert!(matches!(
        check_no_action_loss(&mut v6),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert_eq!(v6.mode, "play");
}

#[test]
fn v7_terminal_replay_settles_once_after_end_game() {
    let mut state = GameState::new(
        GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        },
        17,
    )
    .unwrap();
    state.ruleset_id = RULES_VERSION_V7.into();
    state.mode = "play".into();
    let rng = state.rng.clone();
    let events_before = state.extra["replayEvents"].as_array().unwrap().len();
    let history_before = state.extra["boardHistory"].as_array().unwrap().len();
    let nonce_before = state.extra["replayEventNonce"].as_u64().unwrap();

    end_game(&mut state, Some(Color::White), "검증용 종료").unwrap();
    assert!(state.gameover_replay_pending);
    assert_eq!(
        state.extra["replayEvents"].as_array().unwrap().len(),
        events_before
    );
    crate::replay::settle(&mut state).unwrap();
    assert!(!state.gameover_replay_pending);
    assert_eq!(
        state.extra["replayEvents"].as_array().unwrap().len(),
        events_before + 1
    );
    assert_eq!(
        state.extra["boardHistory"].as_array().unwrap().len(),
        history_before + 1
    );
    assert_eq!(state.extra["replayEventNonce"], json!(nonce_before + 1));
    assert_eq!(
        state.extra["replayEvents"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["label"],
        "gameover"
    );
    assert_eq!(state.rng, rng);
    crate::replay::settle(&mut state).unwrap();
    assert_eq!(
        state.extra["replayEvents"].as_array().unwrap().len(),
        events_before + 1
    );
}
