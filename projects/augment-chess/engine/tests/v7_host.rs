use augment_chess_engine::{
    Action, Color, EngineError, GameState, MoveTarget, PublicEvent, PublicTransition, RngState,
    Sides, Square, V7HostPosition,
};
use serde_json::{Value, json};

fn source_state_with_unlisted_piece_fields() -> Value {
    let mut board = vec![vec![Value::Null; 8]; 8];
    let piece = json!({
        "type": "bigRook",
        "color": "white",
        "anchorRow": 2,
        "anchorCol": 2,
        "unlistedPieceProperty": {"active": true}
    });
    board[2][2] = piece.clone();
    board[4][5] = piece;
    json!({
        "board": board,
        "turn": "white",
        "mode": "play",
        "unlistedStateProperty": {"nested": [1, 2, 3]}
    })
}

fn public_test_event() -> Value {
    let action = Action::movement(
        Color::White,
        Square { row: 2, col: 2 },
        MoveTarget::at(Square { row: 3, col: 2 }),
    );
    let view = PublicTransition {
        kind: "transition".into(),
        actor: Color::White,
        next_actor: Color::White,
        phase: "play".into(),
        board_changes: Vec::new(),
        own_cards: Vec::new(),
        revealed_opponent_cards: Vec::new(),
        captures: Sides::new(Vec::new(), Vec::new()),
        result: json!({
            "protocolVersion": "accelerate-result-v1",
            "status": "ongoing",
            "winner": null,
            "outcome": null,
            "reason": ""
        }),
    };
    serde_json::to_value(PublicEvent {
        protocol_version: "accelerate-game-event-v1".into(),
        actor: Color::White,
        action,
        turn_changed: false,
        public: Sides::new(view.clone(), view),
    })
    .unwrap()
}

#[test]
fn raw_v7_envelope_preserves_source_shape_and_derives_one_piece_identity() {
    let raw = source_state_with_unlisted_piece_fields();
    let position =
        V7HostPosition::from_parts(raw.clone(), RngState::seeded(7), Vec::new()).unwrap();
    let envelope = position.export_envelope().unwrap();
    assert_eq!(envelope["state"], raw);
    assert_eq!(position.spatial().pieces().len(), 1);
    assert_eq!(
        position.state().board[2][2].as_ref().unwrap().id,
        "white-2-2"
    );
    assert_eq!(
        position.state().board[4][5].as_ref().unwrap().id,
        "white-2-2"
    );
    assert_eq!(
        V7HostPosition::from_envelope(envelope)
            .unwrap()
            .position_id(),
        position.position_id()
    );
}

#[test]
fn transaction_failure_keeps_complete_source_rng_and_history_unchanged() {
    let original = V7HostPosition::from_parts(
        source_state_with_unlisted_piece_fields(),
        RngState::seeded(19),
        Vec::new(),
    )
    .unwrap();
    let before = original.export_envelope().unwrap();

    let rejected: augment_chess_engine::Result<(V7HostPosition, ())> =
        original.transact(original.position_id(), |working| {
            working.rng.sample()?;
            working.history.push(json!({"temporary": "event"}));
            working.extra.insert("temporaryEffect".into(), json!(true));
            Err(EngineError::UnsupportedFeature(
                "source rule is not implemented".into(),
            ))
        });
    assert!(matches!(rejected, Err(EngineError::UnsupportedFeature(_))));
    assert_eq!(original.export_envelope().unwrap(), before);
    assert_eq!(original.revision(), 0);

    let invalid = original.transact(original.position_id(), |working| {
        working.board[4][5].as_mut().unwrap().moved = true;
        Ok(())
    });
    assert!(matches!(invalid, Err(EngineError::InvalidState(_))));
    assert_eq!(original.export_envelope().unwrap(), before);

    let invalid_event = original.transact(original.position_id(), |working| {
        working.rng.sample()?;
        working.history.push(json!({"temporary": "event"}));
        Ok(())
    });
    assert!(matches!(
        invalid_event,
        Err(EngineError::InvalidState(message)) if message.contains("history[0]")
    ));
    assert_eq!(original.export_envelope().unwrap(), before);

    let mut staged = original.state().clone();
    staged.rng.sample().unwrap();
    staged.history.push(json!({"temporary": "event"}));
    let (error, recovered) = original
        .try_commit_working(original.position_id(), staged)
        .unwrap_err();
    let recovered: Box<GameState> = recovered;
    assert!(matches!(error, EngineError::InvalidState(_)));
    assert_eq!(recovered.history.len(), 1);
    assert_ne!(recovered.rng, original.state().rng);
    assert_eq!(original.export_envelope().unwrap(), before);
}

#[test]
fn successful_transaction_advances_owned_state_and_rejects_stale_binding() {
    let original = V7HostPosition::from_parts(
        source_state_with_unlisted_piece_fields(),
        RngState::seeded(41),
        Vec::new(),
    )
    .unwrap();
    let old_id = original.position_id().to_owned();
    let old_rng = original.state().rng.clone();
    let (next, value) = original
        .transact(&old_id, |working| {
            let value = working.rng.sample()?;
            working.extra.insert("settledEffect".into(), json!(true));
            working.history.push(public_test_event());
            Ok(value)
        })
        .unwrap();
    assert!((0.0..1.0).contains(&value));
    assert_eq!(next.revision(), 1);
    assert_eq!(next.spatial().revision(), 1);
    assert_ne!(next.position_id(), old_id);
    assert_ne!(next.state().rng, old_rng);
    assert_eq!(original.state().rng, old_rng);
    assert_eq!(
        next.export_envelope().unwrap()["state"]["settledEffect"],
        true
    );
    assert_eq!(next.state().history.len(), 1);
    assert!(original.state().history.is_empty());
    assert!(matches!(
        next.transact(&old_id, |_| Ok(())),
        Err(EngineError::StaleAction)
    ));
}

#[test]
fn malformed_or_relabelled_envelope_fails_before_host_admission() {
    let position = V7HostPosition::from_parts(
        source_state_with_unlisted_piece_fields(),
        RngState::seeded(0),
        Vec::new(),
    )
    .unwrap();
    let mut corrupted = position.export_envelope().unwrap();
    corrupted["state"]["unlistedStateProperty"]["nested"][0] = json!(900);
    assert!(matches!(
        V7HostPosition::from_envelope(corrupted),
        Err(EngineError::InvalidState(message)) if message.contains("positionId")
    ));
    let mut wrong_version = position.export_envelope().unwrap();
    wrong_version["rulesVersion"] = json!("unknown-rules");
    assert!(matches!(
        V7HostPosition::from_envelope(wrong_version),
        Err(EngineError::InvalidState(message)) if message.contains("version mismatch")
    ));
}
