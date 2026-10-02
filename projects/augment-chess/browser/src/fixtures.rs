//! Fixed native/WASM transport scenarios. Compiled only for Rust unit tests or
//! the browser-test-fixtures feature; never part of the production WASM API.
use adapter_runtime::AdapterError;
use augment_chess_engine::{
    CardSlot, Color, GameConfig, GameState, Piece, V7HostPosition, adapter::GameAdapterSession,
    v7_adapter_actions,
};
use serde_json::{Value, json};

pub const CASE_IDS: [&str; 4] = [
    "promotion-choice",
    "ordered-pawn-storm",
    "large-piece-move",
    "private-projection",
];

fn baseline() -> Result<GameState, AdapterError> {
    let session = GameAdapterSession::new_game(
        GameConfig {
            draft_delete: true,
            ..Default::default()
        },
        19,
    )?;
    let mut state = session.position().state().clone();
    state.board = vec![vec![None; 8]; 8];
    state.board[7][4] = Some(Piece::new("king", Color::White, "test-white-king"));
    state.board[0][4] = Some(Piece::new("king", Color::Black, "test-black-king"));
    state.turn = Color::White;
    state.deck_slots.white.clear();
    state.deck_slots.black.clear();
    state.extra.insert("middleDraftDone".into(), json!(true));
    state.extra.insert("endDraftDone".into(), json!(false));
    Ok(state)
}

fn ordered_baseline() -> Result<GameState, AdapterError> {
    // Preserve the successful engine regression's real normal seed-19 draft
    // path and source-created pawn/king properties. draftDelete is a different
    // game setup and must not stand in for the card-enabled source position.
    let mut session = GameAdapterSession::new_game(GameConfig::default(), 19)?;
    for index in [1, 0] {
        let intents =
            v7_adapter_actions::legal_public_intents(session.position()).map_err(|error| {
                AdapterError::execution_failed("browser_test_fixture_invalid", error.to_string())
            })?;
        let intent = intents.get(index).cloned().ok_or_else(|| {
            AdapterError::execution_failed(
                "browser_test_fixture_invalid",
                "fixed source draft choice missing",
            )
        })?;
        let admitted = v7_adapter_actions::bind_public_intent(session.position(), intent).map_err(
            |error| {
                AdapterError::execution_failed("browser_test_fixture_invalid", error.to_string())
            },
        )?;
        let applied =
            v7_adapter_actions::apply_admitted(session.position(), &admitted).map_err(|error| {
                AdapterError::execution_failed("browser_test_fixture_invalid", error.to_string())
            })?;
        session = GameAdapterSession::new(applied.position)?;
    }
    let state = session.position().state().clone();
    if state.mode != "play" || state.turn != Color::White {
        return Err(AdapterError::execution_failed(
            "browser_test_fixture_invalid",
            "fixed normal draft path did not enter white play",
        ));
    }
    Ok(state)
}

pub fn session(case_id: &str) -> Result<GameAdapterSession, AdapterError> {
    if !CASE_IDS.contains(&case_id) {
        return Err(AdapterError::invalid_input(
            "unknown_browser_test_case",
            format!("unknown fixed browser test case {case_id}"),
        ));
    }
    let mut state = if case_id == "ordered-pawn-storm" {
        ordered_baseline()?
    } else {
        baseline()?
    };
    match case_id {
        "promotion-choice" => {
            state.board[0][0] = Some(Piece::new("pawn", Color::White, "test-promotion-pawn"));
            state.extra.insert(
                "pendingPromotion".into(),
                json!({
                    "color":"white", "row":0, "col":0, "choices":["queen", "rook"],
                }),
            );
        }
        "ordered-pawn-storm" => {
            // Same ten-click boundary as the engine's completed UI pawn-storm
            // regression: source order matters and exceeds AI's depth-eight
            // collector limit. The test does not invent a replacement rule.
            let pawn = state.board[6][0].as_ref().cloned().ok_or_else(|| {
                AdapterError::execution_failed(
                    "browser_test_fixture_invalid",
                    "source pawn missing",
                )
            })?;
            let white_king = state.board[7][4].clone();
            let black_king = state.board[0][4].clone();
            state.board = vec![vec![None; 8]; 8];
            state.board[7][4] = white_king;
            state.board[0][4] = black_king;
            let mut squares = Vec::new();
            for col in 0..8 {
                squares.push(json!({"row":2,"col":col}));
            }
            for col in 0..2 {
                squares.push(json!({"row":4,"col":col}));
            }
            for (index, square) in squares.iter().enumerate() {
                let row = square["row"].as_u64().expect("fixed row") as usize;
                let col = square["col"].as_u64().expect("fixed col") as usize;
                let mut piece = pawn.clone();
                piece.id = format!("test-pawn-{index}");
                state.board[row][col] = Some(piece);
            }
            let definitions: Value = serde_json::from_str(include_str!(
                "../../contracts/catalog/card-definitions-20260928.json"
            ))
            .map_err(|error| {
                AdapterError::execution_failed("browser_test_fixture_invalid", error.to_string())
            })?;
            let mut definition = definitions["definitions"]
                .as_array()
                .and_then(|definitions| definitions.iter().find(|card| card["id"] == "pawn-storm"))
                .cloned()
                .ok_or_else(|| {
                    AdapterError::execution_failed(
                        "browser_test_fixture_invalid",
                        "pinned pawn-storm definition missing",
                    )
                })?;
            let presentation: Value = serde_json::from_str(include_str!(
                "../../contracts/catalog/card-presentation-20260928.json"
            ))
            .map_err(|error| {
                AdapterError::execution_failed("browser_test_fixture_invalid", error.to_string())
            })?;
            let presented = presentation["definitions"]
                .as_array()
                .and_then(|definitions| definitions.iter().find(|card| card["id"] == "pawn-storm"))
                .ok_or_else(|| {
                    AdapterError::execution_failed(
                        "browser_test_fixture_invalid",
                        "pinned pawn-storm presentation missing",
                    )
                })?;
            for field in ["name", "text", "art", "help", "helpItems", "helpIcons"] {
                if let Some(value) = presented.get(field) {
                    definition[field] = value.clone();
                }
            }
            definition["instanceId"] = json!("test-pawn-storm");
            let card: CardSlot = serde_json::from_value(definition).map_err(|error| {
                AdapterError::execution_failed("browser_test_fixture_invalid", error.to_string())
            })?;
            squares.reverse();
            state.deck_slots.white = vec![card.clone()];
            state.deck_slots.black.clear();
            state.extra.insert("targeting".into(), json!({
                "card":serde_json::to_value(card).map_err(|error| AdapterError::execution_failed("browser_test_fixture_invalid", error.to_string()))?,
                "launch":null, "pawnStorm":squares,
            }));
        }
        "large-piece-move" => {
            let mut piece = Piece::new("bigRook", Color::White, "test-large-rook");
            piece.extra.insert("anchorRow".into(), json!(3));
            piece.extra.insert("anchorCol".into(), json!(1));
            for row in 3..5 {
                for col in 1..3 {
                    state.board[row][col] = Some(piece.clone());
                }
            }
        }
        "private-projection" => {
            let mut hidden = Piece::new("rook", Color::Black, "test-hidden-rook");
            hidden.extra.insert("hiddenFrom".into(), json!("white"));
            hidden
                .extra
                .insert("privateFixtureMarker".into(), json!("must-not-be-public"));
            state.board[2][3] = Some(hidden);
            state.board[1][4] = Some(Piece::new("rook", Color::White, "test-capture-rook"));
        }
        _ => unreachable!("CASE_IDS guarded"),
    }
    let position = V7HostPosition::from_state(state).map_err(|error| {
        AdapterError::execution_failed("browser_test_fixture_invalid", error.to_string())
    })?;
    GameAdapterSession::new(position)
}
