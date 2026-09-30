use augment_chess_engine::{
    Color, Coord, EngineError, GameConfig, GameState, Offset, Piece, RULES_VERSION_V6,
    RULES_VERSION_V7, SpatialState,
};
use serde_json::json;
use std::collections::BTreeSet;

fn source_state(ruleset: &str, explicit_anchor: bool) -> GameState {
    let mut source = GameState::new(
        GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        },
        17,
    )
    .unwrap();
    source.ruleset_id = ruleset.into();
    source.board = vec![vec![None; 8]; 8];
    let mut piece = Piece::new("bigRook", Color::White, "disconnected");
    if explicit_anchor {
        piece.extra.insert("anchorRow".into(), json!(2));
        piece.extra.insert("anchorCol".into(), json!(2));
    }
    source.board[2][2] = Some(piece.clone());
    source.board[4][5] = Some(piece);
    source
}

#[test]
fn source_profiles_reject_an_explicit_anchor_conflict_without_changing_the_prior_state() {
    for ruleset in [RULES_VERSION_V6, RULES_VERSION_V7] {
        let source = source_state(ruleset, true);
        let spatial = if ruleset == RULES_VERSION_V6 {
            SpatialState::from_legacy(&source).unwrap()
        } else {
            SpatialState::from_v7_source(&source).unwrap()
        };
        let before = spatial.position_key().unwrap();
        let mut moved = spatial.piece("disconnected").unwrap().clone();
        moved.anchor = Coord::new(3, 2);
        assert!(matches!(
            spatial.with_piece(moved),
            Err(EngineError::InvalidState(reason)) if reason.contains("anchorRow")
        ));
        assert_eq!(spatial.position_key().unwrap(), before);
        assert_eq!(
            spatial.piece("disconnected").unwrap().anchor,
            Coord::new(2, 2)
        );
    }
}

#[test]
fn legacy_export_preserves_a_moved_explicit_anchor_and_rejects_lossy_implicit_anchor() {
    let source = source_state(RULES_VERSION_V6, true);
    let spatial = SpatialState::from_legacy(&source).unwrap();
    let mut moved = spatial.piece("disconnected").unwrap().clone();
    moved.anchor = Coord::new(3, 2);
    moved.attributes.insert("anchorRow".into(), json!(3));
    moved.attributes.insert("anchorCol".into(), json!(2));
    let moved_state = spatial.with_piece(moved).unwrap();
    let exported = moved_state.to_legacy(&source).unwrap();
    assert!(exported.board[2][2].is_none());
    assert_eq!(exported.board[3][2].as_ref().unwrap().id, "disconnected");
    assert_eq!(exported.board[5][5].as_ref().unwrap().id, "disconnected");
    assert_eq!(exported.board[3][2].as_ref().unwrap().extra["anchorRow"], 3);

    let implicit_source = source_state(RULES_VERSION_V6, false);
    let implicit = SpatialState::from_legacy(&implicit_source).unwrap();
    let mut anchorless = implicit.piece("disconnected").unwrap().clone();
    anchorless.footprint = [Offset::new(0, 1), Offset::new(2, 3)]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let anchorless_state = implicit.with_piece(anchorless).unwrap();
    assert!(matches!(
        anchorless_state.to_legacy(&implicit_source),
        Err(EngineError::UnsupportedFeature(reason)) if reason.contains("anchorRow") || reason.contains("anchorCol")
    ));
}
