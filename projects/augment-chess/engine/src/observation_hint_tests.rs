use super::{
    board_surface, board_surface_v7_with_fog, fog_visible_squares_v7, piece_visible_to_color_at_v7,
    piece_visible_to_color_at_v7_with_fog, validate_ui_hints,
};
use crate::{Color, EngineError, Fields, GameConfig, GameState, Observation, Piece, Square};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn observation(hints: Value) -> Observation {
    let mut public_state = Fields::new();
    public_state.insert("mode".into(), json!("play"));
    public_state.insert("selectionPhase".into(), Value::Null);
    public_state.insert("legalHints".into(), hints);
    Observation {
        protocol_version: "test".into(),
        viewer: Color::White,
        turn: Color::White,
        board: vec![
            vec![Some(json!({"color":"white"})), None],
            vec![None, Some(json!({"color":"black"}))],
        ],
        own_cards: vec![json!({"instanceId":"own-card"})],
        opponent_hand_count: 0,
        public_state,
        history: Vec::new(),
        information_state_key: String::new(),
    }
}

#[test]
fn ui_hints_keep_empty_targeted_cards_but_reject_private_or_foreign_fields() {
    let valid = observation(json!({
        "moves":[{"from":{"row":0.0,"col":0},"destinations":[{"row":0,"col":1}]}],
        "cardTargets":[{"cardInstanceId":"own-card","targets":[]}]
    }));
    validate_ui_hints(&valid).unwrap();

    let mut unknown_card = valid.clone();
    unknown_card.public_state["legalHints"]["cardTargets"][0]["cardInstanceId"] =
        json!("opponent-card");
    assert!(matches!(
        validate_ui_hints(&unknown_card),
        Err(EngineError::InvalidState(_))
    ));

    let mut private_flag = valid.clone();
    private_flag.public_state["legalHints"]["moves"][0]["destinations"][0]["pieceId"] =
        json!("private-id");
    assert!(matches!(
        validate_ui_hints(&private_flag),
        Err(EngineError::InvalidState(_))
    ));
}

#[test]
fn ui_hints_reject_inactive_viewer_hidden_origin_and_duplicate_cells() {
    let hints = json!({
        "moves":[{"from":{"row":0,"col":0},"destinations":[{"row":0,"col":1}]}],
        "cardTargets":[]
    });
    let mut inactive = observation(hints.clone());
    inactive.turn = Color::Black;
    assert!(matches!(
        validate_ui_hints(&inactive),
        Err(EngineError::InvalidState(_))
    ));

    let mut hidden = observation(hints.clone());
    hidden.board[0][0] = None;
    assert!(matches!(
        validate_ui_hints(&hidden),
        Err(EngineError::InvalidState(_))
    ));

    let mut duplicate = observation(hints);
    duplicate.public_state["legalHints"]["moves"][0]["destinations"] =
        json!([{"row":0,"col":1},{"row":0,"col":1}]);
    assert!(matches!(
        validate_ui_hints(&duplicate),
        Err(EngineError::InvalidState(_))
    ));
}

#[test]
fn ui_move_origins_keep_frozen_renderer_row_major_order() {
    let hints = json!({
        "moves":[
            {"from":{"row":0,"col":0},"destinations":[{"row":0,"col":1}]},
            {"from":{"row":1,"col":0},"destinations":[{"row":1,"col":1}]}
        ],
        "cardTargets":[]
    });
    let mut ordered = observation(hints);
    ordered.board[1][0] = Some(json!({"color":"white"}));
    ordered.board[1][1] = None;
    validate_ui_hints(&ordered).unwrap();

    let mut reordered = ordered;
    reordered.public_state["legalHints"]["moves"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert!(matches!(
        validate_ui_hints(&reordered),
        Err(EngineError::InvalidState(_))
    ));
}

#[test]
fn nonacting_viewer_cannot_infer_unchosen_draft_offer_from_full_observation() {
    let state = crate::draft::initialize_for_ruleset(
        GameConfig::default(),
        19,
        crate::state::RULES_VERSION_V7,
    )
    .unwrap();
    assert_eq!(state.mode, "draft");
    assert_eq!(state.turn, Color::White);
    let mut altered = state.clone();
    let canary = "private-draft-offer-canary";
    altered.extra["draft"]["choices"][0]["instanceId"] = json!(canary);

    // The current actor sees the offer, while the other viewer's complete
    // public result and information key remain identical despite the change.
    assert_ne!(
        state.try_observe(Color::White).unwrap(),
        altered.try_observe(Color::White).unwrap()
    );
    let before = state.try_observe(Color::Black).unwrap();
    let after = altered.try_observe(Color::Black).unwrap();
    assert_eq!(before, after);
    assert!(!serde_json::to_string(&after).unwrap().contains(canary));
}

#[test]
fn pre_september18_paladin_aura_respects_the_source_light_square_gate() {
    // Frozen renderer __publicBoardSurface calls usesSeptember18Balance and
    // lightSquare before appending Paladin aura. The latter is row+col even.
    let mut state = GameState::new(GameConfig::default(), 4).unwrap();
    state.board = vec![vec![None; 8]; 8];
    state.board[3][4] = Some(Piece::new("paladin", Color::White, "paladin-dark"));
    state
        .extra
        .insert("september18Balance".into(), json!(false));
    let aura_count = |state: &GameState| {
        board_surface(state, Color::White)["boardMarks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|mark| mark["kind"] == "paladinAura")
            .count()
    };
    assert_eq!(aura_count(&state), 0);

    state.board[3][4] = None;
    state.board[3][3] = Some(Piece::new("paladin", Color::White, "paladin-light"));
    assert_eq!(aura_count(&state), 9);

    state.board[3][3] = None;
    state.board[3][4] = Some(Piece::new("paladin", Color::White, "paladin-dark"));
    state.extra.insert("september18Balance".into(), json!(true));
    assert_eq!(aura_count(&state), 9);
}

fn visibility_state_v7() -> GameState {
    let mut state = crate::draft::initialize_for_ruleset(
        GameConfig::default(),
        19,
        crate::state::RULES_VERSION_V7,
    )
    .unwrap();
    state.mode = "play".into();
    state.board = vec![vec![None; 8]; 8];
    state
}

#[test]
fn v7_visibility_uses_first_identity_cell_and_any_fogged_footprint_cell() {
    // Source main109826 chooses the first physical alias for camouflage and
    // reveals an entire large identity if any occupied cell is in the fog set.
    let mut state = visibility_state_v7();
    let mut large = Piece::new("bigRook", Color::Black, "large-black");
    for row in 2..4 {
        for col in 2..4 {
            state.board[row][col] = Some(large.clone());
        }
    }
    let fog = BTreeSet::from([Square { row: 3, col: 3 }]);
    for at in [Square { row: 2, col: 2 }, Square { row: 2, col: 3 }] {
        assert!(
            piece_visible_to_color_at_v7_with_fog(
                &state,
                state.at(at).unwrap(),
                at,
                Color::White,
                Some(&fog),
            )
            .unwrap()
        );
    }
    state.extra.insert("camouflageRule".into(), json!(true));
    // Black camouflage matches dark cells. Although the supplied cell is
    // dark, the first identity cell is light and therefore remains visible.
    let at = Square { row: 2, col: 3 };
    assert!(
        piece_visible_to_color_at_v7_with_fog(
            &state,
            state.at(at).unwrap(),
            at,
            Color::White,
            Some(&fog),
        )
        .unwrap()
    );
    large.extra.insert("anchorRow".into(), json!(2.0));
    large.extra.insert("anchorCol".into(), json!(3.0));
    state.board = vec![vec![None; 8]; 8];
    for row in 2..4 {
        for col in 3..5 {
            state.board[row][col] = Some(large.clone());
        }
    }
    assert!(
        !piece_visible_to_color_at_v7_with_fog(
            &state,
            state.at(at).unwrap(),
            at,
            Color::White,
            Some(&fog),
        )
        .unwrap()
    );
}

#[test]
fn v7_visibility_preserves_source_short_circuits_and_explicit_hidden_priority() {
    let mut state = visibility_state_v7();
    let at = Square { row: 2, col: 3 };
    let mut piece = Piece::new("rook", Color::Black, "hidden-black");
    // A truthy explicit hiddenFrom suppresses the camouflage fallback even
    // when it names the opposite viewer. It does not skip the later fog test.
    piece.extra.insert("hiddenFrom".into(), json!("black"));
    state.board[2][3] = Some(piece.clone());
    state.extra.insert("camouflageRule".into(), json!(true));
    assert!(
        piece_visible_to_color_at_v7_with_fog(&state, &piece, at, Color::White, None,).unwrap()
    );
    assert!(
        !piece_visible_to_color_at_v7_with_fog(
            &state,
            &piece,
            at,
            Color::White,
            Some(&BTreeSet::new()),
        )
        .unwrap()
    );
    piece.extra.insert("hiddenFrom".into(), json!("white"));
    assert!(!piece_visible_to_color_at_v7(&state, &piece, at, Color::White,).unwrap());
    // Owned pieces and fully revealed offline gameover short circuit even
    // when an unsupported fog movement profile would otherwise fail.
    state
        .extra
        .insert("campaign".into(), json!({"setup":"fogWar"}));
    assert!(piece_visible_to_color_at_v7(&state, &piece, at, Color::Black,).unwrap());
    state.mode = "gameover".into();
    assert!(piece_visible_to_color_at_v7(&state, &piece, at, Color::White,).unwrap());
}

#[test]
fn v7_fog_activation_follows_local_profile_and_draft_player_color() {
    let mut state = visibility_state_v7();
    state.extra.insert(
        "madAi".into(),
        json!({"enabled":true,"options":{"fog":true}}),
    );
    // Position restore fixes the source playMode to local, where Mad-AI UI
    // options are dormant. Inferring activation from the snapshot would leak
    // a different projection policy into the frozen headless profile.
    assert!(
        fog_visible_squares_v7(&state, Color::White)
            .unwrap()
            .is_none()
    );
    state.extra.insert(
        "campaign".into(),
        json!({"setup":"fogWar","playerColor":"white"}),
    );
    state.mode = "draft".into();
    assert!(
        fog_visible_squares_v7(&state, Color::Black)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fog_visible_squares_v7(&state, Color::White).unwrap(),
        Some(BTreeSet::new())
    );
    // 원문 draft 화면은 요청 관측자가 아닌 campaign.playerColor의 안개를 쓴다.
    // 따라서 black 기물 공개용 fog=null이어도 빈 화면의 64칸은 안개 표시를 유지한다.
    let black_surface = board_surface_v7_with_fog(&state, Color::Black, None).unwrap();
    let draft_marks = (0..8)
        .flat_map(|row| {
            (0..8).map(move |col| json!({"kind":"fogHidden","square":{"row":row,"col":col}}))
        })
        .collect::<Vec<_>>();
    assert_eq!(black_surface["boardMarks"], json!(draft_marks));
    let white_fog = BTreeSet::new();
    assert_eq!(
        black_surface,
        board_surface_v7_with_fog(&state, Color::White, Some(&white_fog)).unwrap()
    );
    state.mode = "gameover".into();
    assert!(
        fog_visible_squares_v7(&state, Color::White)
            .unwrap()
            .is_none()
    );
    state.ruleset_id = crate::state::RULES_VERSION_V6.into();
    assert!(matches!(
        fog_visible_squares_v7(&state, Color::White),
        Err(EngineError::UnsupportedFeature(_))
    ));
}

#[test]
fn v7_fogged_surface_keeps_terrain_and_siren_aura_but_masks_local_forecasts() {
    let mut state = visibility_state_v7();
    let hidden = Square { row: 4, col: 3 };
    state.board[4][3] = Some(Piece::new("siren", Color::Black, "siren-hidden"));
    state.extra.insert("blackHole".into(), json!([hidden]));
    state
        .extra
        .insert("d4".into(), json!({"white":true,"black":false}));
    state.extra.insert(
        "pendingScarecrows".into(),
        json!([{"row":4,"col":3,"color":"black","remainingOwnTurns":1}]),
    );
    state.extra.insert(
        "tabooPending".into(),
        json!([{"square":hidden,"color":"black"}]),
    );
    let fog = BTreeSet::new();
    let surface = board_surface_v7_with_fog(&state, Color::White, Some(&fog)).unwrap();
    let marks = surface["boardMarks"].as_array().unwrap();
    let at_hidden = |kind: &str| {
        marks
            .iter()
            .any(|mark| mark["kind"] == kind && mark["square"] == json!(hidden))
    };
    assert!(at_hidden("blackHole"));
    assert!(at_hidden("fogHidden"));
    assert!(at_hidden("scarecrowReserved"));
    assert!(at_hidden("sirenAura"));
    assert!(!at_hidden("scarecrowPreview"));
    assert!(!at_hidden("d4Forbidden"));
    assert!(!at_hidden("taboo"));
}

#[test]
fn v7_quantum_ghost_uses_ghost_cells_visibility_independent_of_physical_cell() {
    let mut state = visibility_state_v7();
    let mut piece = Piece::new("knight", Color::Black, "quantum-black");
    piece
        .extra
        .insert("quantum".into(), json!({"row":1,"col":1}));
    state.board[0][0] = Some(piece);
    let fog = BTreeSet::from([Square { row: 1, col: 1 }]);
    let surface = board_surface_v7_with_fog(&state, Color::White, Some(&fog)).unwrap();
    assert_eq!(surface["overlays"].as_array().unwrap().len(), 1);
    assert_eq!(surface["overlays"][0]["cells"], json!([{"row":1,"col":1}]));
    state.board[0][0]
        .as_mut()
        .unwrap()
        .extra
        .insert("hiddenFrom".into(), json!("white"));
    let concealed = board_surface_v7_with_fog(&state, Color::White, Some(&fog)).unwrap();
    assert!(concealed["overlays"].as_array().unwrap().is_empty());
}
