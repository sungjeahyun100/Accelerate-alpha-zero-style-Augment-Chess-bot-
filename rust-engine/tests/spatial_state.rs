use accelerate_engine::{
    BoardGeometry, CellState, Color, Coord, EngineError, GameConfig, GameState, Offset, Piece,
    PieceColor, Position, RULES_VERSION_V7, ResizePolicy, ResizeRequest, SpatialPiece,
    SpatialProfile, SpatialReference, SpatialState,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn offsets(values: &[(i32, i32)]) -> BTreeSet<Offset> {
    values
        .iter()
        .map(|&(row, col)| Offset::new(row, col))
        .collect()
}

fn coords(values: &[(i32, i32)]) -> BTreeSet<Coord> {
    values
        .iter()
        .map(|&(row, col)| Coord::new(row, col))
        .collect()
}

fn piece(id: &str, anchor: Coord, footprint: &[(i32, i32)]) -> SpatialPiece {
    SpatialPiece::new(id, "bigRook", PieceColor::White, anchor, offsets(footprint))
}

#[test]
fn geometry_maps_rectangles_and_signed_extents_without_relabeling_coordinates() {
    for (height, width) in [(1, 1), (5, 7), (8, 8), (10, 12)] {
        let geometry = BoardGeometry::new(-3, -5, height, width).unwrap();
        assert_eq!(geometry.area(), usize::from(height) * usize::from(width));
        for (index, coord) in geometry.coordinates().enumerate() {
            assert_eq!(geometry.index(coord), Some(index));
            assert_eq!(geometry.coord_at(index), Some(coord));
        }
        assert_eq!(geometry.index(Coord::new(-4, -5)), None);
        assert_eq!(geometry.index(Coord::new(-3, -6)), None);
    }
    assert!(BoardGeometry::new(0, 0, 0, 1).is_err());
    assert!(BoardGeometry::new(0, 0, 65, 65).is_err());
    assert!(BoardGeometry::new(i32::MAX, 0, 2, 1).is_err());
    assert!(
        serde_json::from_value::<BoardGeometry>(json!({
            "minRow": i32::MAX,
            "minCol": 0,
            "height": 2,
            "width": 1
        }))
        .is_err()
    );
    assert_eq!(Coord::new(i32::MAX, 0).offset(Offset::new(1, 0)), None);
}

#[test]
fn one_piece_owns_disconnected_cells_and_collision_is_rejected_atomically() {
    let state = SpatialState::new(BoardGeometry::new(0, 0, 5, 7).unwrap()).unwrap();
    let first_piece = piece("one", Coord::new(1, 1), &[(0, 0), (2, 3)]);
    let state = state.with_piece(first_piece).unwrap();
    assert_eq!(state.pieces().len(), 1);
    assert_eq!(state.piece_at(Coord::new(1, 1)).unwrap().id, "one");
    assert_eq!(state.piece_at(Coord::new(3, 4)).unwrap().id, "one");
    assert!(state.piece_at(Coord::new(2, 2)).is_none());

    let before = state.position_key().unwrap();
    let collision = piece("other", Coord::new(3, 4), &[(0, 0)]);
    assert!(matches!(
        state.with_piece(collision),
        Err(EngineError::InvalidState(_))
    ));
    assert_eq!(state.position_key().unwrap(), before);
    assert_eq!(state.pieces().len(), 1);
}

#[test]
fn synthetic_resize_requires_new_cells_and_an_explicit_clipping_policy() {
    let initial = SpatialState::new(BoardGeometry::new(0, 0, 5, 7).unwrap()).unwrap();
    let initial = initial
        .with_piece(piece("edge", Coord::new(4, 6), &[(0, 0)]))
        .unwrap();
    let initial = initial
        .with_link(SpatialReference {
            id: "portal".into(),
            cells: coords(&[(0, 0), (4, 6)]),
        })
        .unwrap();
    let initial = initial
        .with_scheduled_effect(SpatialReference {
            id: "due".into(),
            cells: coords(&[(4, 6)]),
        })
        .unwrap();
    let before = initial.position_key().unwrap();

    let smaller = BoardGeometry::new(0, 0, 4, 6).unwrap();
    let reject = ResizeRequest {
        geometry: smaller,
        added_cells: BTreeMap::new(),
        policy: ResizePolicy::RejectAffected,
    };
    assert!(initial.resize_geometry(reject).is_err());
    assert_eq!(initial.position_key().unwrap(), before);
    let removed = initial
        .resize_geometry(ResizeRequest {
            geometry: smaller,
            added_cells: BTreeMap::new(),
            policy: ResizePolicy::RemoveAffectedEntities,
        })
        .unwrap();
    assert!(removed.pieces().is_empty());
    assert!(removed.links().is_empty());
    assert!(removed.scheduled_effects().is_empty());
    assert_eq!(removed.geometry(), smaller);
    assert!(matches!(
        removed.check_binding(&before),
        Err(EngineError::StaleAction)
    ));

    let larger = BoardGeometry::new(-1, -2, 7, 9).unwrap();
    let mut added_cells = BTreeMap::new();
    for coord in larger
        .coordinates()
        .filter(|&coord| !smaller.contains(coord))
    {
        added_cells.insert(coord, CellState::open());
    }
    let incomplete = ResizeRequest {
        geometry: larger,
        added_cells: BTreeMap::new(),
        policy: ResizePolicy::RejectAffected,
    };
    assert!(removed.resize_geometry(incomplete).is_err());
    let expanded = removed
        .resize_geometry(ResizeRequest {
            geometry: larger,
            added_cells,
            policy: ResizePolicy::RejectAffected,
        })
        .unwrap();
    assert_eq!(expanded.geometry().index(Coord::new(0, 0)), Some(11));
    assert!(expanded.is_usable(Coord::new(-1, -2)));
    assert!(expanded.is_usable(Coord::new(3, 5)));

    let mut marked = CellState::open();
    marked.terrain.insert("marker".into());
    let marked_state = SpatialState::new(BoardGeometry::new(0, 0, 2, 2).unwrap())
        .unwrap()
        .with_cell(Coord::new(1, 1), marked)
        .unwrap();
    assert!(
        marked_state
            .resize_geometry(ResizeRequest {
                geometry: BoardGeometry::new(0, 0, 1, 2).unwrap(),
                added_cells: BTreeMap::new(),
                policy: ResizePolicy::RejectAffected,
            })
            .is_err()
    );
}

#[test]
fn v7_collapse_keeps_extent_and_only_removes_affected_footprint_offsets() {
    let geometry = BoardGeometry::new(0, 0, 8, 8).unwrap();
    let state = SpatialState::new_with_profile(SpatialProfile::SourceV7, geometry)
        .unwrap()
        .with_piece(piece("large", Coord::new(2, 2), &[(0, 0), (0, 1), (1, 0)]))
        .unwrap();
    let collapsed = state.collapse_cells(&coords(&[(2, 2)])).unwrap();
    assert_eq!(collapsed.geometry(), geometry);
    assert!(!collapsed.is_usable(Coord::new(2, 2)));
    assert!(collapsed.piece_at(Coord::new(2, 2)).is_none());
    assert_eq!(collapsed.piece_at(Coord::new(2, 3)).unwrap().id, "large");
    assert_eq!(collapsed.piece("large").unwrap().anchor, Coord::new(2, 2));
    assert_eq!(collapsed.piece("large").unwrap().footprint.len(), 2);
    let linked = state
        .with_link(SpatialReference {
            id: "link".into(),
            cells: coords(&[(2, 2), (5, 5)]),
        })
        .unwrap();
    assert!(matches!(
        linked.collapse_cells(&coords(&[(2, 2)])),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        state.resize_geometry(ResizeRequest {
            geometry: BoardGeometry::new(0, 0, 7, 8).unwrap(),
            added_cells: BTreeMap::new(),
            policy: ResizePolicy::RemoveAffectedEntities,
        }),
        Err(EngineError::UnsupportedFeature(_))
    ));
}

#[test]
fn spatial_snapshot_roundtrip_rebuilds_occupancy_and_rejects_bad_shape() {
    let state = SpatialState::new(BoardGeometry::new(-2, 3, 5, 7).unwrap())
        .unwrap()
        .with_piece(piece("p", Coord::new(-1, 4), &[(0, 0), (1, 2)]))
        .unwrap();
    let snapshot = state.to_snapshot_value().unwrap();
    let restored = SpatialState::from_snapshot_value(snapshot.clone()).unwrap();
    assert_eq!(restored, state);
    assert_eq!(restored.piece_at(Coord::new(0, 6)).unwrap().id, "p");
    assert_eq!(
        restored.position_key().unwrap(),
        state.position_key().unwrap()
    );

    let mut bad = snapshot;
    bad["board"]["cells"].as_array_mut().unwrap().pop();
    assert!(matches!(
        SpatialState::from_snapshot_value(bad),
        Err(EngineError::InvalidState(_))
    ));
}

#[test]
fn legacy_board_roundtrips_one_disconnected_large_piece_identity() {
    let mut source = GameState::new(
        GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        },
        17,
    )
    .unwrap();
    source.board = vec![vec![None; 8]; 8];
    let mut original = Piece::new("bigRook", Color::White, "disconnected");
    original.extra.insert("anchorRow".into(), json!(2));
    original.extra.insert("anchorCol".into(), json!(2));
    source.board[2][2] = Some(original.clone());
    source.board[4][5] = Some(original);

    let spatial = SpatialState::from_legacy(&source).unwrap();
    assert_eq!(spatial.pieces().len(), 1);
    assert_eq!(spatial.piece("disconnected").unwrap().footprint.len(), 2);
    assert_eq!(
        spatial.piece_at(Coord::new(4, 5)).unwrap().id,
        "disconnected"
    );
    let exported = spatial.to_legacy(&source).unwrap();
    assert_eq!(exported.board, source.board);
    assert_eq!(exported.ruleset_id, source.ruleset_id);
    assert_eq!(exported.rng, source.rng);

    source.board[2][2]
        .as_mut()
        .unwrap()
        .extra
        .remove("anchorRow");
    source.board[2][2]
        .as_mut()
        .unwrap()
        .extra
        .remove("anchorCol");
    source.board[4][5]
        .as_mut()
        .unwrap()
        .extra
        .remove("anchorRow");
    source.board[4][5]
        .as_mut()
        .unwrap()
        .extra
        .remove("anchorCol");
    let no_anchor = SpatialState::from_legacy(&source).unwrap();
    assert_eq!(no_anchor.pieces().len(), 1);
    assert_eq!(
        no_anchor.piece("disconnected").unwrap().anchor,
        Coord::new(2, 2)
    );
}

#[test]
fn v7_source_board_projects_one_exact_noncontiguous_piece_without_enabling_execution() {
    let mut source = GameState::new(
        GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        },
        29,
    )
    .unwrap();
    source.ruleset_id = RULES_VERSION_V7.into();
    source.board = vec![vec![None; 8]; 8];
    let mut original = Piece::new("bigRook", Color::White, "large-v7");
    original.extra.insert("anchorRow".into(), json!(2));
    original.extra.insert("anchorCol".into(), json!(2));
    source.board[2][2] = Some(original.clone());
    source.board[4][5] = Some(original);
    let unchanged = source.clone();

    let spatial = SpatialState::from_v7_source(&source).unwrap();
    assert_eq!(source, unchanged);
    assert_eq!(spatial.profile(), SpatialProfile::SourceV7);
    assert_eq!(spatial.geometry(), BoardGeometry::new(0, 0, 8, 8).unwrap());
    assert_eq!(spatial.pieces().len(), 1);
    assert_eq!(spatial.piece("large-v7").unwrap().anchor, Coord::new(2, 2));
    assert_eq!(
        spatial.piece("large-v7").unwrap().footprint,
        offsets(&[(0, 0), (2, 3)])
    );
    assert_eq!(spatial.piece_at(Coord::new(2, 2)).unwrap().id, "large-v7");
    assert_eq!(spatial.piece_at(Coord::new(4, 5)).unwrap().id, "large-v7");
    assert!(spatial.piece_at(Coord::new(3, 4)).is_none());
    assert!(matches!(
        spatial.resize_geometry(ResizeRequest {
            geometry: BoardGeometry::new(0, 0, 7, 8).unwrap(),
            added_cells: BTreeMap::new(),
            policy: ResizePolicy::RemoveAffectedEntities,
        }),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        spatial.to_legacy(&source),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        Position::from_state(source.clone()),
        Err(EngineError::UnsupportedFeature(_))
    ));

    let mut collapsed_source = source.clone();
    collapsed_source
        .extra
        .insert("collapsedCells".into(), json!([{"row": 3, "col": 3}]));
    collapsed_source
        .extra
        .insert("collapsed".into(), json!(true));
    collapsed_source
        .extra
        .insert("collapseDepth".into(), json!(1));
    let collapsed_spatial = SpatialState::from_v7_source(&collapsed_source).unwrap();
    assert_eq!(collapsed_spatial.geometry(), spatial.geometry());
    assert!(!collapsed_spatial.is_usable(Coord::new(3, 3)));
    assert!(!collapsed_spatial.is_usable(Coord::new(7, 3)));
    assert!(collapsed_spatial.is_usable(Coord::new(4, 5)));
    assert_eq!(
        collapsed_spatial.piece_at(Coord::new(4, 5)).unwrap().id,
        "large-v7"
    );

    let mut wrong_version = source.clone();
    wrong_version.ruleset_id = "unknown-version".into();
    assert!(matches!(
        SpatialState::from_v7_source(&wrong_version),
        Err(EngineError::InvalidState(_))
    ));
    source.board[4][5].as_mut().unwrap().moved = true;
    assert!(matches!(
        SpatialState::from_v7_source(&source),
        Err(EngineError::InvalidState(_))
    ));
}

#[test]
fn v7_shape_validation_does_not_enable_the_legacy_execution_path() {
    let mut source = GameState::new(GameConfig::default(), 23).unwrap();
    source.ruleset_id = RULES_VERSION_V7.into();
    let encoded = serde_json::to_value(&source).unwrap();
    let mut decoded: GameState = serde_json::from_value(encoded).unwrap();
    decoded.validate_v7_snapshot_shape_and_identify().unwrap();
    assert_eq!(decoded.board, source.board);
    assert!(matches!(
        decoded.validate_and_identify(),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        Position::from_state(decoded),
        Err(EngineError::UnsupportedFeature(_))
    ));
}
