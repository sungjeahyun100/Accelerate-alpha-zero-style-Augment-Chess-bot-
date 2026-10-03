use super::*;

fn sparse_royal_capture(ruleset: &str) -> GameState {
    let config = GameConfig {
        draft_delete: true,
        ..GameConfig::default()
    };
    let mut state = if ruleset == RULES_VERSION_V7 {
        crate::v7_new_game::new_game(config, 7).unwrap()
    } else {
        GameState::new(config, 7).unwrap()
    };
    assert_eq!(state.mode, "play");
    state.board = vec![vec![None; 8]; 8];
    for (row, color, kind, id) in [
        (7, Color::White, "king", "white-king-43rz584v7ai"),
        (1, Color::White, "rook", "white-rook-rtl3lvpr0xh"),
        (0, Color::Black, "king", "black-king-mdl71nrt2k"),
    ] {
        let mut piece = Piece::new(kind, color, id);
        piece.extra.insert("shielded".into(), json!(false));
        state.board[row][4] = Some(piece);
    }
    if ruleset == RULES_VERSION_V7 {
        // Source newGame({draftDelete:true}, 7) with three source-created
        // pieces consumes 35 draws before the capture and one for notation.
        state.rng.cursor = 35;
        state.rng.state = 3_319_274_396;
    }
    state
}

fn capture_action() -> Action {
    Action::movement(
        Color::White,
        Square::new(1, 4).unwrap(),
        MoveTarget::at(Square::new(0, 4).unwrap()),
    )
}

#[test]
fn v7_terminal_royal_capture_keeps_source_moved_flag_in_state_history_and_observation() {
    // Frozen main-OahWs0tU.js SHA-256 e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c,
    // source-pinned royal-capture-before sample[0]. The source returns before
    // its moved=true stage after the captured king ends the game.
    let mut state = sparse_royal_capture(RULES_VERSION_V7);
    apply(&mut state, &capture_action()).unwrap();
    assert_eq!(state.mode, "gameover");
    assert_eq!(state.winner.as_deref(), Some("white"));
    assert!(!state.board[0][4].as_ref().unwrap().moved);
    assert_eq!(state.rng.cursor, 36);
    assert_eq!(state.rng.state, 363_102_795);
    assert_eq!(state.history.len(), 1);
    assert_eq!(
        state.history[0]["public"]["black"]["boardChanges"][0]["after"]["moved"],
        json!(false)
    );
    for viewer in [Color::White, Color::Black] {
        let observation = state.observe_checked(viewer).unwrap();
        assert_eq!(
            observation.board[0][4].as_ref().unwrap()["moved"],
            json!(false)
        );
    }
}

#[test]
fn v6_terminal_royal_capture_preserves_existing_moved_behavior() {
    let mut state = sparse_royal_capture(RULES_VERSION_V6);
    apply(&mut state, &capture_action()).unwrap();
    assert_eq!(state.mode, "gameover");
    assert!(state.board[0][4].as_ref().unwrap().moved);
}
