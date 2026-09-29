//! A small test-only N-version model for spatial state and composite moves.
//!
//! Its coordinate-keyed cells and pieces are independent of the engine's flat
//! occupancy index. These synthetic cases test internal contracts; they are
//! not evidence of source-v7 site parity or complete rule coverage.

use accelerate_engine::move_program::{
    ActivationCondition, ActivationStep, MoveNode, MoveProgram, MoveProgramLimits, MoveResolution,
    Primitive, PublicMoveIntent, PublicMoveKind, RawMove, SpatialMoveBoard,
};
use accelerate_engine::{
    BoardGeometry, CellState, Coord, Offset, PieceColor, ResizePolicy, ResizeRequest, SpatialPiece,
    SpatialProfile, SpatialState,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct RefPiece {
    anchor: Coord,
    offsets: BTreeSet<Offset>,
    color: PieceColor,
}

/// Cells are addressed by signed coordinate; occupation is derived by scanning
/// physical pieces rather than stored in a row-major array.
#[derive(Clone)]
struct RefWorld {
    geometry: BoardGeometry,
    profile: SpatialProfile,
    cells: BTreeMap<Coord, CellState>,
    pieces: BTreeMap<String, RefPiece>,
}

impl RefWorld {
    fn new(geometry: BoardGeometry, profile: SpatialProfile) -> Self {
        let mut cells = BTreeMap::new();
        for row in geometry.min_row()..geometry.min_row() + i32::from(geometry.height()) {
            for col in geometry.min_col()..geometry.min_col() + i32::from(geometry.width()) {
                cells.insert(Coord::new(row, col), CellState::open());
            }
        }
        Self {
            geometry,
            profile,
            cells,
            pieces: BTreeMap::new(),
        }
    }

    fn add(&mut self, id: &str, color: PieceColor, anchor: Coord, offsets: &[(i32, i32)]) {
        let offsets = offsets
            .iter()
            .map(|&(row, col)| Offset::new(row, col))
            .collect::<BTreeSet<_>>();
        assert!(!offsets.is_empty());
        for &offset in &offsets {
            let at = anchor.offset(offset).unwrap();
            assert!(self.is_usable(at));
            assert!(self.occupant(at).is_none());
        }
        assert!(
            self.pieces
                .insert(
                    id.into(),
                    RefPiece {
                        anchor,
                        offsets,
                        color,
                    },
                )
                .is_none()
        );
    }

    fn is_usable(&self, at: Coord) -> bool {
        self.cells.get(&at).is_some_and(|cell| cell.usable)
    }

    fn cells_of(&self, piece: &RefPiece, anchor: Coord) -> BTreeSet<Coord> {
        piece
            .offsets
            .iter()
            .map(|&offset| anchor.offset(offset).unwrap())
            .collect()
    }

    fn occupant(&self, at: Coord) -> Option<&str> {
        self.pieces.iter().find_map(|(id, piece)| {
            self.cells_of(piece, piece.anchor)
                .contains(&at)
                .then_some(id.as_str())
        })
    }

    fn to_engine(&self) -> SpatialState {
        let mut state = SpatialState::new_with_profile(self.profile, self.geometry).unwrap();
        for (&at, cell) in &self.cells {
            if *cell != CellState::open() {
                state = state.with_cell(at, cell.clone()).unwrap();
            }
        }
        for (id, piece) in &self.pieces {
            state = state
                .with_piece(SpatialPiece::new(
                    id,
                    "probe",
                    piece.color,
                    piece.anchor,
                    piece.offsets.clone(),
                ))
                .unwrap();
        }
        state
    }

    fn collapse(&mut self, collapsed: &BTreeSet<Coord>) {
        for at in collapsed {
            assert!(self.cells.get_mut(at).unwrap().usable);
            self.cells.get_mut(at).unwrap().usable = false;
        }
        self.pieces.retain(|_, piece| {
            piece
                .offsets
                .retain(|&offset| !collapsed.contains(&piece.anchor.offset(offset).unwrap()));
            !piece.offsets.is_empty()
        });
    }

    fn resize_removing_affected(
        &mut self,
        new_geometry: BoardGeometry,
    ) -> BTreeMap<Coord, CellState> {
        let mut added = BTreeMap::new();
        let mut next_cells = BTreeMap::new();
        for row in new_geometry.min_row()..new_geometry.min_row() + i32::from(new_geometry.height())
        {
            for col in
                new_geometry.min_col()..new_geometry.min_col() + i32::from(new_geometry.width())
            {
                let at = Coord::new(row, col);
                let state = self.cells.get(&at).cloned().unwrap_or_else(|| {
                    let cell = CellState::open();
                    added.insert(at, cell.clone());
                    cell
                });
                next_cells.insert(at, state);
            }
        }
        self.pieces.retain(|_, piece| {
            next_cells.contains_key(&piece.anchor)
                && piece
                    .offsets
                    .iter()
                    .all(|&offset| next_cells.contains_key(&piece.anchor.offset(offset).unwrap()))
        });
        self.geometry = new_geometry;
        self.cells = next_cells;
        added
    }
}

fn assert_same_world(reference: &RefWorld, actual: &SpatialState) {
    assert_eq!(actual.geometry(), reference.geometry);
    assert_eq!(actual.profile(), reference.profile);
    assert_eq!(actual.pieces().len(), reference.pieces.len());
    for (&at, expected_cell) in &reference.cells {
        assert_eq!(actual.cell(at), Some(expected_cell), "cell {at:?}");
        assert_eq!(
            actual.piece_at(at).map(|piece| piece.id.as_str()),
            reference.occupant(at),
            "occupancy {at:?}"
        );
    }
    for (id, expected) in &reference.pieces {
        let piece = actual.piece(id).unwrap();
        assert_eq!(piece.anchor, expected.anchor, "anchor {id}");
        assert_eq!(piece.footprint, expected.offsets, "footprint {id}");
    }
}

#[test]
fn signed_rectangle_index_matches_coordinate_keyed_enumeration() {
    for (origin, size) in [((-3, 5), (1, 4)), ((2, -4), (5, 3)), ((-9, -7), (7, 6))] {
        let geometry = BoardGeometry::new(origin.0, origin.1, size.0, size.1).unwrap();
        let reference = RefWorld::new(geometry, SpatialProfile::SyntheticGeometryV1);
        for (expected_index, &at) in reference.cells.keys().enumerate() {
            assert_eq!(geometry.index(at), Some(expected_index));
            assert_eq!(geometry.coord_at(expected_index), Some(at));
        }
        assert_eq!(geometry.index(Coord::new(origin.0 - 1, origin.1)), None);
        assert_eq!(geometry.index(Coord::new(origin.0, origin.1 - 1)), None);
        assert_eq!(geometry.coord_at(reference.cells.len()), None);
    }
}

#[test]
fn source_collapse_keeps_extent_and_one_identity_per_remaining_footprint() {
    let geometry = BoardGeometry::new(0, 0, 8, 8).unwrap();
    let mut reference = RefWorld::new(geometry, SpatialProfile::SourceV7);
    reference.add(
        "large",
        PieceColor::White,
        Coord::new(2, 2),
        &[(0, 0), (0, 1), (1, 0), (2, 2)],
    );
    reference.add("single", PieceColor::Black, Coord::new(5, 5), &[(0, 0)]);
    let initial = reference.to_engine();
    assert_same_world(&reference, &initial);

    let collapsed = [Coord::new(2, 2), Coord::new(5, 5)]
        .into_iter()
        .collect::<BTreeSet<_>>();
    reference.collapse(&collapsed);
    let actual = initial.collapse_cells(&collapsed).unwrap();
    assert_same_world(&reference, &actual);
    assert_eq!(actual.geometry(), geometry);
    assert!(actual.piece("single").is_none());
    assert_eq!(actual.piece("large").unwrap().anchor, Coord::new(2, 2));
}

#[test]
fn synthetic_resize_matches_coordinate_map_and_is_atomic_on_rejection() {
    let initial_geometry = BoardGeometry::new(-1, -2, 3, 4).unwrap();
    let mut reference = RefWorld::new(initial_geometry, SpatialProfile::SyntheticGeometryV1);
    reference.add(
        "keep",
        PieceColor::White,
        Coord::new(0, 0),
        &[(0, 0), (1, 0)],
    );
    reference.add(
        "clip",
        PieceColor::Black,
        Coord::new(-1, -2),
        &[(0, 0), (0, 1)],
    );
    reference
        .cells
        .get_mut(&Coord::new(1, 1))
        .unwrap()
        .terrain
        .insert("marker".into());
    let initial = reference.to_engine();
    assert_same_world(&reference, &initial);
    let before = initial.position_key().unwrap();

    let next_geometry = BoardGeometry::new(0, -1, 4, 4).unwrap();
    let mut expected = reference.clone();
    let added = expected.resize_removing_affected(next_geometry);
    assert!(
        initial
            .resize_geometry(ResizeRequest {
                geometry: next_geometry,
                added_cells: added.clone(),
                policy: ResizePolicy::RejectAffected,
            })
            .is_err()
    );
    assert_eq!(initial.position_key().unwrap(), before);

    let actual = initial
        .resize_geometry(ResizeRequest {
            geometry: next_geometry,
            added_cells: added,
            policy: ResizePolicy::RemoveAffectedEntities,
        })
        .unwrap();
    assert_same_world(&expected, &actual);
    assert!(actual.piece("clip").is_none());
    assert_eq!(actual.piece_at(Coord::new(1, 0)).unwrap().id, "keep");
    assert!(actual.check_binding(&before).is_err());
}

fn node(primitive: Primitive, direction: (i32, i32), distance: u32) -> MoveNode {
    MoveNode {
        primitive,
        direction: Offset::new(direction.0, direction.1),
        max_distance: Some(distance),
        activation_condition: ActivationCondition::Any,
        activate_at_parent_distance: None,
        children: vec![],
    }
}

fn reference_placement(
    world: &RefWorld,
    piece: &RefPiece,
    anchor: Coord,
    ignored: &[&str],
) -> bool {
    world
        .cells_of(piece, anchor)
        .iter()
        .all(|&at| world.is_usable(at) && world.occupant(at).is_none_or(|id| ignored.contains(&id)))
}

fn reference_shift(world: &RefWorld, mover: &str, target: &str) -> bool {
    let mover_piece = &world.pieces[mover];
    let target_piece = &world.pieces[target];
    let mover_after = world.cells_of(mover_piece, target_piece.anchor);
    let target_after = world.cells_of(target_piece, mover_piece.anchor);
    mover_after.is_disjoint(&target_after)
        && mover_after.iter().chain(&target_after).all(|&at| {
            world.is_usable(at)
                && world
                    .occupant(at)
                    .is_none_or(|id| id == mover || id == target)
        })
}

/// Recursive, coordinate-keyed specification for the three primitives under
/// comparison. Its traversal is intentionally different from the runtime's
/// stack and raw ordering is compared as a multiset.
fn reference_moves(world: &RefWorld, program: &MoveProgram, mover: &str) -> Vec<RawMove> {
    fn visit(
        world: &RefWorld,
        program: &MoveProgram,
        mover_id: &str,
        node: &MoveNode,
        origin: Coord,
        path: Vec<usize>,
        ancestry: Vec<ActivationStep>,
        result: &mut Vec<RawMove>,
    ) {
        let mover = &world.pieces[mover_id];
        let mut jumped = false;
        for distance in 1..=node.max_distance.unwrap() {
            let selected = Coord::new(
                origin.row + node.direction.row * distance as i32,
                origin.col + node.direction.col * distance as i32,
            );
            if !world.is_usable(selected) {
                break;
            }
            let hit = world.occupant(selected).filter(|&id| id != mover_id);
            let activation = match node.primitive {
                Primitive::Move if hit.is_none() => {
                    if reference_placement(world, mover, selected, &[mover_id]) {
                        Some((PublicMoveKind::Travel, selected, None))
                    } else {
                        break;
                    }
                }
                Primitive::Move => break,
                Primitive::Jump => match hit {
                    Some(id) if !jumped && world.pieces[id].color != mover.color => {
                        jumped = true;
                        None
                    }
                    Some(_) => break,
                    None if jumped => {
                        if reference_placement(world, mover, selected, &[mover_id]) {
                            Some((PublicMoveKind::Travel, selected, None))
                        } else {
                            break;
                        }
                    }
                    None => None,
                },
                Primitive::Shift => match hit {
                    Some(id) if reference_shift(world, mover_id, id) => {
                        Some((PublicMoveKind::Shift, world.pieces[id].anchor, Some(id)))
                    }
                    Some(_) => break,
                    None => None,
                },
                _ => unreachable!("N-version scope uses MOVE, JUMP and SHIFT"),
            };
            if let Some((kind, new_anchor, shifted_id)) = activation {
                let mut provenance = ancestry.clone();
                provenance.push(ActivationStep {
                    node_path: path.clone(),
                    current_origin: origin,
                    activated: selected,
                    distance,
                });
                result.push(RawMove {
                    intent: PublicMoveIntent {
                        piece_id: mover_id.into(),
                        original_origin: mover.anchor,
                        selected,
                        kind,
                    },
                    resolution: MoveResolution {
                        new_anchor,
                        captured_id: None,
                        shifted_id: shifted_id.map(str::to_owned),
                    },
                    source_id: program.source_id.clone(),
                    modifier: None,
                    provenance: provenance.clone(),
                });
                for (child_index, child) in node.children.iter().enumerate() {
                    if child
                        .activate_at_parent_distance
                        .is_none_or(|required| required == distance)
                    {
                        let mut child_path = path.clone();
                        child_path.push(child_index);
                        visit(
                            world,
                            program,
                            mover_id,
                            child,
                            selected,
                            child_path,
                            provenance.clone(),
                            result,
                        );
                    }
                }
            }
            if node.primitive == Primitive::Shift && hit.is_some() {
                break;
            }
        }
    }

    let mut result = Vec::new();
    for (root_index, root) in program.roots.iter().enumerate() {
        visit(
            world,
            program,
            mover,
            root,
            world.pieces[mover].anchor,
            vec![root_index],
            vec![],
            &mut result,
        );
    }
    result
}

type RawKey = (
    PublicMoveIntent,
    Coord,
    Option<String>,
    Option<String>,
    String,
    Vec<(Vec<usize>, Coord, Coord, u32)>,
);

fn raw_multiset(raw: &[RawMove]) -> BTreeMap<RawKey, usize> {
    let mut counts = BTreeMap::new();
    for action in raw {
        assert!(action.modifier.is_none(), "base program has no modifier");
        let key = (
            action.intent.clone(),
            action.resolution.new_anchor,
            action.resolution.captured_id.clone(),
            action.resolution.shifted_id.clone(),
            action.source_id.clone(),
            action
                .provenance
                .iter()
                .map(|step| {
                    (
                        step.node_path.clone(),
                        step.current_origin,
                        step.activated,
                        step.distance,
                    )
                })
                .collect(),
        );
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

#[test]
fn composite_paths_and_footprint_shift_match_independent_specification() {
    let geometry = BoardGeometry::new(0, 0, 8, 8).unwrap();
    let mut parent = node(Primitive::Move, (0, 1), 3);
    let mut child = node(Primitive::Jump, (1, 0), 3);
    child.activate_at_parent_distance = Some(2);
    parent.children.push(child);
    let program = MoveProgram {
        source_id: "synthetic-forest".into(),
        roots: vec![
            parent,
            node(Primitive::Shift, (0, 1), 4),
            node(Primitive::Move, (-1, 0), 1),
            node(Primitive::Move, (0, 1), 2),
        ],
    };

    // The last layout swaps into overlapping footprints even though its two
    // starting bodies do not overlap.
    for (target_offsets, blocked_swap, blocked_travel, overlapping_swap) in [
        (&[(0, 0), (0, 1)][..], false, false, false),
        (&[(0, 0), (0, 1)][..], true, false, false),
        (&[(0, 0), (0, 1)][..], false, true, false),
        (&[(0, 0), (0, 3)][..], false, false, true),
    ] {
        let mut reference = RefWorld::new(geometry, SpatialProfile::SourceV7);
        reference.add(
            "mover",
            PieceColor::White,
            Coord::new(1, 1),
            &[(0, 0), (1, 0)],
        );
        reference.add("jumped", PieceColor::Black, Coord::new(3, 3), &[(0, 0)]);
        reference.add(
            "target",
            PieceColor::White,
            Coord::new(1, 4),
            target_offsets,
        );
        if blocked_swap {
            reference.add("third", PieceColor::Black, Coord::new(2, 4), &[(0, 0)]);
        }
        if blocked_travel {
            reference.add("sideblock", PieceColor::Black, Coord::new(2, 2), &[(0, 0)]);
        }
        let state = reference.to_engine();
        assert_same_world(&reference, &state);
        let board = SpatialMoveBoard {
            state: &state,
            capture: |state: &SpatialState, mover: &str, victim: &str, allow_friendly: bool| {
                allow_friendly
                    || state.piece(mover).unwrap().color != state.piece(victim).unwrap().color
            },
            shift: |_state: &SpatialState, _mover: &str, _target: &str| true,
        };
        let actual = program
            .evaluate(&board, "mover", MoveProgramLimits::default())
            .unwrap();
        let expected = reference_moves(&reference, &program, "mover");
        assert_eq!(
            raw_multiset(&actual.raw),
            raw_multiset(&expected),
            "blocked_swap={blocked_swap}, blocked_travel={blocked_travel}, overlapping_swap={overlapping_swap}"
        );

        let mut expected_public = BTreeMap::<PublicMoveIntent, usize>::new();
        for raw in &expected {
            *expected_public.entry(raw.intent.clone()).or_default() += 1;
        }
        let actual_public = actual
            .public_candidates()
            .into_iter()
            .map(|candidate| (candidate.intent, candidate.raw_indices.len()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(actual_public, expected_public);
        assert!(actual.raw.iter().any(|raw| {
            raw.intent.selected == Coord::new(0, 1)
                && raw.intent.original_origin == Coord::new(1, 1)
                && raw.provenance.len() == 1
                && raw.provenance[0].node_path == vec![2]
                && raw.provenance[0].current_origin == Coord::new(1, 1)
        }));
        assert_eq!(
            actual.raw.iter().any(|raw| {
                raw.intent.selected == Coord::new(4, 3)
                    && raw.intent.original_origin == Coord::new(1, 1)
                    && raw.provenance.len() == 2
                    && raw.provenance[0].distance == 2
                    && raw.provenance[1].current_origin == Coord::new(1, 3)
            }),
            !blocked_travel
        );
        if !blocked_travel {
            let duplicated = PublicMoveIntent {
                piece_id: "mover".into(),
                original_origin: Coord::new(1, 1),
                selected: Coord::new(1, 2),
                kind: PublicMoveKind::Travel,
            };
            assert_eq!(actual_public.get(&duplicated), Some(&2));
        }
        assert_eq!(
            actual
                .raw
                .iter()
                .filter(|raw| raw.intent.kind == PublicMoveKind::Shift)
                .count(),
            usize::from(!blocked_swap && !overlapping_swap)
        );
    }
}
