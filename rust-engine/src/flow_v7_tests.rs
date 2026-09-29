use super::*;

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
