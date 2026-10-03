use super::*;
use crate::geometry::BoardGeometry;
use crate::spatial_state::{SpatialPiece, SpatialState};
use crate::state::PieceColor;
use serde_json::json;
use std::cell::Cell;

struct TestPiece {
    anchor: Coord,
    footprint: BTreeSet<Offset>,
    side: char,
}

struct TestBoard {
    geometry: BoardGeometry,
    revision: Cell<u64>,
    revise_on_capture: Cell<bool>,
    unusable: BTreeSet<Coord>,
    pieces: BTreeMap<String, TestPiece>,
    occupied: BTreeMap<Coord, String>,
}

impl TestBoard {
    fn new(height: u16, width: u16) -> Self {
        Self {
            geometry: BoardGeometry::new(0, 0, height, width).unwrap(),
            revision: Cell::new(0),
            revise_on_capture: Cell::new(false),
            unusable: BTreeSet::new(),
            pieces: BTreeMap::new(),
            occupied: BTreeMap::new(),
        }
    }

    fn add(&mut self, id: &str, side: char, anchor: (i32, i32), offsets: &[(i32, i32)]) {
        let anchor = Coord::new(anchor.0, anchor.1);
        let footprint: BTreeSet<_> = offsets
            .iter()
            .map(|&(row, col)| Offset::new(row, col))
            .collect();
        for offset in &footprint {
            let square = anchor.offset(*offset).unwrap();
            assert!(self.geometry.contains(square));
            assert!(self.occupied.insert(square, id.to_owned()).is_none());
        }
        assert!(
            self.pieces
                .insert(
                    id.to_owned(),
                    TestPiece {
                        anchor,
                        footprint,
                        side,
                    }
                )
                .is_none()
        );
    }
}

impl MoveBoard for TestBoard {
    fn revision(&self) -> Option<u64> {
        Some(self.revision.get())
    }

    fn is_usable(&self, square: Coord) -> bool {
        self.geometry.contains(square) && !self.unusable.contains(&square)
    }

    fn occupant(&self, square: Coord) -> Option<&str> {
        self.occupied.get(&square).map(String::as_str)
    }

    fn piece(&self, id: &str) -> Option<MovePiece<'_>> {
        self.pieces.get_key_value(id).map(|(id, piece)| MovePiece {
            id,
            anchor: piece.anchor,
            footprint: &piece.footprint,
        })
    }

    fn can_capture(&self, mover: &str, victim: &str, allow_friendly: bool) -> bool {
        if self.revise_on_capture.get() {
            self.revision.set(self.revision.get() + 1);
        }
        let mover = self.pieces.get(mover).unwrap();
        let victim = self.pieces.get(victim).unwrap();
        allow_friendly || mover.side != victim.side
    }
}

fn node(primitive: Primitive, direction: (i32, i32), max_distance: u32) -> MoveNode {
    MoveNode {
        primitive,
        direction: Offset::new(direction.0, direction.1),
        max_distance: Some(max_distance),
        activation_condition: ActivationCondition::Any,
        activate_at_parent_distance: None,
        children: Vec::new(),
    }
}

fn program(roots: Vec<MoveNode>) -> MoveProgram {
    MoveProgram {
        source_id: "test-program".into(),
        roots,
    }
}

fn evaluate(board: &TestBoard, roots: Vec<MoveNode>) -> MoveProgramOutput {
    program(roots)
        .evaluate(board, "mover", MoveProgramLimits::default())
        .unwrap()
}

#[test]
fn child_search_uses_parent_square_but_preserves_original_origin_and_board() {
    let mut board = TestBoard::new(5, 7);
    board.add("mover", 'w', (1, 0), &[(0, 0)]);
    let mut parent = node(Primitive::Move, (0, 1), 3);
    let mut child = node(Primitive::Move, (1, 0), 1);
    child.activate_at_parent_distance = Some(2);
    parent.children.push(child);
    let output = evaluate(&board, vec![parent]);
    assert_eq!(output.raw.len(), 4);
    let child_move = output
        .raw
        .iter()
        .find(|raw| raw.intent.selected == Coord::new(2, 2))
        .unwrap();
    assert_eq!(child_move.intent.original_origin, Coord::new(1, 0));
    assert_eq!(child_move.provenance.len(), 2);
    assert_eq!(child_move.provenance[0].distance, 2);
    assert_eq!(child_move.provenance[1].current_origin, Coord::new(1, 2));
    assert_eq!(board.occupant(Coord::new(1, 0)), Some("mover"));
    assert_eq!(board.occupant(Coord::new(1, 2)), None);
}

#[test]
fn must_capture_filters_own_output_without_disabling_children() {
    let mut board = TestBoard::new(4, 5);
    board.add("mover", 'w', (1, 1), &[(0, 0)]);
    let mut parent = node(Primitive::TakeMove, (0, 1), 1);
    parent.activation_condition = ActivationCondition::MustCapture;
    parent.children.push(node(Primitive::Move, (1, 0), 1));
    let output = evaluate(&board, vec![parent]);
    assert_eq!(output.raw.len(), 1);
    assert_eq!(output.raw[0].intent.selected, Coord::new(2, 2));
    assert_eq!(output.raw[0].provenance.len(), 2);
}

#[test]
fn no_capture_blocks_capture_activation_and_its_children() {
    let mut board = TestBoard::new(4, 5);
    board.add("mover", 'w', (1, 1), &[(0, 0)]);
    board.add("enemy", 'b', (1, 2), &[(0, 0)]);
    let mut parent = node(Primitive::TakeMove, (0, 1), 2);
    parent.activation_condition = ActivationCondition::NoCapture;
    parent.children.push(node(Primitive::Move, (1, 0), 1));
    assert!(evaluate(&board, vec![parent]).raw.is_empty());
}

#[test]
fn failed_child_does_not_suppress_a_sibling() {
    let mut board = TestBoard::new(4, 5);
    board.add("mover", 'w', (1, 1), &[(0, 0)]);
    let mut parent = node(Primitive::Move, (0, 1), 1);
    parent.children.push(node(Primitive::Take, (0, 1), 1));
    parent.children.push(node(Primitive::Move, (1, 0), 1));
    let output = evaluate(&board, vec![parent]);
    assert_eq!(output.raw.len(), 2);
    assert_eq!(output.raw[1].intent.selected, Coord::new(2, 2));
    assert_eq!(output.raw[1].provenance[1].node_path, [0, 1]);
}

#[test]
fn multi_cell_movement_checks_entire_destination_footprint() {
    let mut board = TestBoard::new(2, 6);
    board.add("mover", 'w', (0, 0), &[(0, 0), (0, 1)]);
    board.add("blocker", 'b', (0, 3), &[(0, 0)]);
    board.add("far", 'b', (0, 5), &[(0, 0)]);
    let moved = evaluate(&board, vec![node(Primitive::Move, (0, 1), 5)]);
    assert_eq!(moved.raw.len(), 1);
    assert_eq!(moved.raw[0].intent.selected, Coord::new(0, 1));
    assert!(
        evaluate(&board, vec![node(Primitive::Take, (0, 1), 5)])
            .raw
            .is_empty()
    );
}

#[test]
fn duplicate_paths_keep_provenance_but_share_public_choice() {
    let mut board = TestBoard::new(3, 4);
    board.add("mover", 'w', (1, 1), &[(0, 0)]);
    let output = evaluate(
        &board,
        vec![
            node(Primitive::Move, (0, 1), 1),
            node(Primitive::TakeMove, (0, 1), 1),
        ],
    );
    assert_eq!(output.raw.len(), 2);
    assert_ne!(output.raw[0].provenance, output.raw[1].provenance);
    assert_eq!(output.public_candidates().len(), 1);
    assert_eq!(output.public_candidates()[0].raw_indices, [0, 1]);
}

#[test]
fn capture_primitives_preserve_their_distinct_effects() {
    let mut board = TestBoard::new(4, 5);
    board.add("mover", 'w', (1, 1), &[(0, 0)]);
    board.add("friend", 'w', (1, 2), &[(0, 0)]);
    board.add("enemy", 'b', (2, 1), &[(0, 0)]);
    assert!(
        evaluate(&board, vec![node(Primitive::Move, (0, 1), 2)])
            .raw
            .is_empty()
    );
    assert!(
        evaluate(&board, vec![node(Primitive::Take, (0, 1), 2)])
            .raw
            .is_empty()
    );
    assert!(
        evaluate(&board, vec![node(Primitive::TakeMove, (0, 1), 2)])
            .raw
            .is_empty()
    );
    let both = evaluate(&board, vec![node(Primitive::BothTakeMove, (0, 1), 2)]);
    assert_eq!(
        both.raw[0].resolution.captured_id.as_deref(),
        Some("friend")
    );
    let catch = evaluate(&board, vec![node(Primitive::Catch, (1, 0), 1)]);
    assert_eq!(catch.raw[0].intent.kind, PublicMoveKind::Catch);
    assert_eq!(catch.raw[0].intent.selected, Coord::new(2, 1));
    assert_eq!(catch.raw[0].resolution.new_anchor, Coord::new(1, 1));
    assert_eq!(
        catch.raw[0].resolution.captured_id.as_deref(),
        Some("enemy")
    );
    let take = evaluate(&board, vec![node(Primitive::Take, (1, 0), 1)]);
    assert_eq!(take.raw[0].resolution.new_anchor, Coord::new(2, 1));
}

#[test]
fn jump_resets_per_sibling_and_lands_only_after_eligible_enemy() {
    let mut board = TestBoard::new(5, 6);
    board.add("mover", 'w', (0, 0), &[(0, 0)]);
    board.add("enemy", 'b', (1, 2), &[(0, 0)]);
    let mut parent = node(Primitive::Move, (1, 0), 2);
    parent.children.push(node(Primitive::Jump, (0, 1), 4));
    let output = evaluate(&board, vec![parent]);
    let jump_landing: Vec<_> = output
        .raw
        .iter()
        .filter(|raw| raw.provenance.len() == 2)
        .map(|raw| raw.intent.selected)
        .collect();
    assert_eq!(jump_landing, [Coord::new(1, 3), Coord::new(1, 4)]);
    assert!(
        output
            .raw
            .iter()
            .all(|raw| raw.intent.selected != Coord::new(2, 3))
    );
}

#[test]
fn shift_uses_anchors_even_when_a_body_cell_is_selected() {
    let mut board = TestBoard::new(2, 7);
    board.add("mover", 'w', (0, 0), &[(0, 0), (0, 1)]);
    board.add("target", 'b', (0, 3), &[(0, 0), (0, 1)]);
    let output = evaluate(&board, vec![node(Primitive::Shift, (0, 2), 3)]);
    assert_eq!(output.raw.len(), 1);
    assert_eq!(output.raw[0].intent.selected, Coord::new(0, 4));
    assert_eq!(output.raw[0].resolution.new_anchor, Coord::new(0, 3));
    assert_eq!(
        output.raw[0].resolution.shifted_id.as_deref(),
        Some("target")
    );
}

#[test]
fn shift_checks_third_party_collision_mutual_overlap_and_stops_on_first_piece() {
    let mut third_party = TestBoard::new(2, 8);
    third_party.add("mover", 'w', (0, 0), &[(0, 0), (0, 1)]);
    third_party.add("target", 'b', (0, 3), &[(0, 0)]);
    third_party.add("blocker", 'b', (0, 4), &[(0, 0)]);
    third_party.add("far", 'b', (0, 6), &[(0, 0)]);
    assert!(
        evaluate(&third_party, vec![node(Primitive::Shift, (0, 1), 7)])
            .raw
            .is_empty()
    );

    let mut overlap = TestBoard::new(2, 8);
    overlap.add("mover", 'w', (0, 0), &[(0, 0), (0, 1)]);
    overlap.add("target", 'b', (0, 3), &[(0, 0), (0, 4)]);
    assert!(
        evaluate(&overlap, vec![node(Primitive::Shift, (0, 1), 7)])
            .raw
            .is_empty()
    );

    let mut out_of_bounds = TestBoard::new(2, 5);
    out_of_bounds.add("mover", 'w', (0, 0), &[(0, 0), (0, 1)]);
    out_of_bounds.add("target", 'b', (0, 4), &[(0, 0)]);
    assert!(
        evaluate(&out_of_bounds, vec![node(Primitive::Shift, (0, 1), 7)])
            .raw
            .is_empty()
    );
}

#[test]
fn invalid_programs_and_work_exhaustion_fail_explicitly() {
    let mut board = TestBoard::new(2, 3);
    board.add("mover", 'w', (0, 0), &[(0, 0)]);
    let invalid = program(vec![node(Primitive::Move, (0, 0), 1)]);
    assert!(matches!(
        invalid.evaluate(&board, "mover", MoveProgramLimits::default()),
        Err(MoveProgramError::InvalidProgram("zero direction"))
    ));
    let limited = MoveProgramLimits {
        max_examined: 1,
        ..MoveProgramLimits::default()
    };
    assert!(matches!(
        program(vec![node(Primitive::Move, (0, 1), 2)]).evaluate(&board, "mover", limited),
        Err(MoveProgramError::LimitExceeded {
            resource: "examined cells",
            limit: 1,
        })
    ));
    board.occupied.insert(Coord::new(0, 0), "unindexed".into());
    assert!(matches!(
        program(vec![node(Primitive::Move, (0, 1), 1)]).evaluate(
            &board,
            "mover",
            MoveProgramLimits::default()
        ),
        Err(MoveProgramError::InvalidBoard(_))
    ));
}

#[test]
fn typed_program_rejects_unknown_primitives_and_fields() {
    let source = json!({
        "sourceId": "base-rook",
        "roots": [{
            "primitive": "TAKEMOVE",
            "direction": {"row": 0, "col": 1},
            "maxDistance": 3,
            "activationCondition": "NoCapture",
            "children": [{
                "primitive": "SHIFT",
                "direction": {"row": 1, "col": 0},
                "maxDistance": 1,
                "activateAtParentDistance": 2
            }]
        }]
    });
    let parsed: MoveProgram = serde_json::from_value(source.clone()).unwrap();
    parsed.validate(MoveProgramLimits::default()).unwrap();
    assert_eq!(
        serde_json::to_value(&parsed).unwrap()["roots"][0]["primitive"],
        "TAKEMOVE"
    );
    let mut bad = source.clone();
    bad["roots"][0]["primitive"] = json!("WARP");
    assert!(serde_json::from_value::<MoveProgram>(bad).is_err());
    let mut bad = source;
    bad["roots"][0]["unmodeledEffect"] = json!(true);
    assert!(serde_json::from_value::<MoveProgram>(bad).is_err());
}

#[test]
fn base_and_active_modifiers_share_budget_and_preserve_sources() {
    let mut board = TestBoard::new(2, 5);
    board.add("mover", 'w', (0, 0), &[(0, 0)]);
    let add = |id: &str, remaining| MoveModifier {
        modifier_id: id.into(),
        source: "test-card".into(),
        program: MoveProgram {
            source_id: id.into(),
            roots: vec![node(Primitive::Move, (0, 1), 1)],
        },
        expiration: ModifierExpiration::Actions { remaining },
    };
    let programs = MoveProgramSet {
        base: MoveProgram {
            source_id: "base".into(),
            roots: vec![node(Primitive::Move, (0, 1), 1)],
        },
        modifiers: vec![add("active", 1), add("expired", 0)],
    };
    let output = programs
        .evaluate(&board, "mover", MoveProgramLimits::default())
        .unwrap();
    assert_eq!(output.raw.len(), 2);
    assert_eq!(output.raw[0].source_id, "base");
    assert_eq!(output.raw[1].source_id, "active");
    assert_eq!(output.raw[0].modifier, None);
    assert_eq!(output.raw[1].modifier.as_ref().unwrap().source, "test-card");
    assert_eq!(output.public_candidates()[0].raw_indices, [0, 1]);
    let mut cursor = programs
        .cursor(&board, "mover", MoveProgramLimits::default())
        .unwrap();
    let mut paged = MoveProgramOutput {
        raw: Vec::new(),
        examined: 0,
    };
    loop {
        let page = cursor.next_page(1, 1).unwrap();
        paged.examined += page.examined;
        paged.raw.extend(page.raw);
        if page.exhausted {
            break;
        }
    }
    assert_eq!(paged, output);
    assert!(matches!(
        programs.evaluate(
            &board,
            "mover",
            MoveProgramLimits {
                max_nodes: 1,
                ..MoveProgramLimits::default()
            }
        ),
        Err(MoveProgramError::LimitExceeded {
            resource: "nodes",
            limit: 1,
        })
    ));
    let limits = MoveProgramLimits {
        max_examined: 1,
        ..MoveProgramLimits::default()
    };
    let mut cursor = programs.cursor(&board, "mover", limits).unwrap();
    assert_eq!(cursor.next_page(1, 1).unwrap().raw.len(), 1);
    assert!(matches!(
        cursor.next_page(1, 1),
        Err(MoveProgramError::LimitExceeded {
            resource: "examined cells",
            limit: 1,
        })
    ));
    assert!(matches!(
        programs.evaluate(&board, "mover", limits),
        Err(MoveProgramError::LimitExceeded {
            resource: "examined cells",
            limit: 1,
        })
    ));
    let limits = MoveProgramLimits {
        max_raw_actions: 1,
        ..MoveProgramLimits::default()
    };
    let mut cursor = programs.cursor(&board, "mover", limits).unwrap();
    assert_eq!(cursor.next_page(1, 1).unwrap().raw.len(), 1);
    assert!(matches!(
        cursor.next_page(1, 1),
        Err(MoveProgramError::LimitExceeded {
            resource: "raw actions",
            limit: 1,
        })
    ));
}

#[test]
fn unbounded_ray_reports_limit_instead_of_silent_truncation() {
    let mut board = TestBoard::new(2, 5);
    board.add("mover", 'w', (0, 0), &[(0, 0)]);
    let mut move_ray = node(Primitive::Move, (0, 1), 4);
    move_ray.max_distance = None;
    assert!(matches!(
        program(vec![move_ray]).evaluate(
            &board,
            "mover",
            MoveProgramLimits {
                max_ray_distance: 2,
                ..MoveProgramLimits::default()
            }
        ),
        Err(MoveProgramError::LimitExceeded {
            resource: "ray distance",
            limit: 2,
        })
    ));
}

#[test]
fn lazy_pages_preserve_eager_order_duplicates_and_examined_work() {
    let mut board = TestBoard::new(4, 6);
    board.add("mover", 'w', (1, 1), &[(0, 0)]);
    let roots = vec![
        node(Primitive::Move, (0, 1), 3),
        node(Primitive::TakeMove, (0, 1), 3),
    ];
    let program = program(roots);
    let limits = MoveProgramLimits::default();
    let eager = program.evaluate(&board, "mover", limits).unwrap();
    let mut cursor = program.cursor(&board, "mover", limits).unwrap();
    let mut paged = MoveProgramOutput {
        raw: Vec::new(),
        examined: 0,
    };
    loop {
        let page = cursor.next_page(1, 1).unwrap();
        assert!(page.examined <= 1 && page.raw.len() <= 1);
        paged.examined += page.examined;
        paged.raw.extend(page.raw);
        if page.exhausted {
            break;
        }
        assert_eq!(page.examined, 1);
    }
    assert_eq!(paged, eager);
    assert_eq!(paged.public_candidates().len(), 3);
    assert_eq!(paged.raw.len(), 6);
}

#[test]
fn empty_page_counts_work_and_cursor_rejects_stale_revision() {
    let mut board = TestBoard::new(3, 5);
    board.add("mover", 'w', (1, 1), &[(0, 0)]);
    board.add("enemy", 'b', (1, 3), &[(0, 0)]);
    let program = program(vec![node(Primitive::Take, (0, 1), 3)]);
    let mut cursor = program
        .cursor(&board, "mover", MoveProgramLimits::default())
        .unwrap();
    let first = cursor.next_page(1, 1).unwrap();
    assert!(first.raw.is_empty());
    assert_eq!(first.examined, 1);
    assert!(!first.exhausted);
    board.revision.set(1);
    assert!(matches!(
        cursor.next_page(1, 1),
        Err(MoveProgramError::StaleCursor)
    ));
}

#[test]
fn revision_change_during_a_page_discards_its_partial_output() {
    let mut board = TestBoard::new(3, 4);
    board.add("mover", 'w', (1, 1), &[(0, 0)]);
    board.add("enemy", 'b', (1, 2), &[(0, 0)]);
    let program = program(vec![node(Primitive::Take, (0, 1), 1)]);
    let mut cursor = program
        .cursor(&board, "mover", MoveProgramLimits::default())
        .unwrap();
    board.revise_on_capture.set(true);
    assert!(matches!(
        cursor.next_page(1, 1),
        Err(MoveProgramError::StaleCursor)
    ));
    assert!(matches!(
        cursor.next_page(1, 1),
        Err(MoveProgramError::InvalidProgram("cursor already failed"))
    ));
}

#[test]
fn lazy_cursor_reports_action_and_work_limits_without_truncation() {
    let mut board = TestBoard::new(3, 5);
    board.add("mover", 'w', (1, 1), &[(0, 0)]);
    let moves = program(vec![node(Primitive::Move, (0, 1), 2)]);
    let limits = MoveProgramLimits {
        max_raw_actions: 1,
        ..MoveProgramLimits::default()
    };
    let mut cursor = moves.cursor(&board, "mover", limits).unwrap();
    assert_eq!(cursor.next_page(1, 1).unwrap().raw.len(), 1);
    assert!(matches!(
        cursor.next_page(1, 1),
        Err(MoveProgramError::LimitExceeded {
            resource: "raw actions",
            limit: 1,
        })
    ));
    assert!(matches!(
        moves.evaluate(&board, "mover", limits),
        Err(MoveProgramError::LimitExceeded {
            resource: "raw actions",
            limit: 1,
        })
    ));

    let take = program(vec![node(Primitive::Take, (0, 1), 2)]);
    let limits = MoveProgramLimits {
        max_examined: 1,
        ..MoveProgramLimits::default()
    };
    let mut cursor = take.cursor(&board, "mover", limits).unwrap();
    let first = cursor.next_page(1, 1).unwrap();
    assert!(first.raw.is_empty());
    assert_eq!(first.examined, 1);
    assert!(matches!(
        cursor.next_page(1, 1),
        Err(MoveProgramError::LimitExceeded {
            resource: "examined cells",
            limit: 1,
        })
    ));
}

#[test]
fn spatial_state_adapter_reads_canonical_piece_and_occupancy() {
    let footprint = BTreeSet::from([Offset::new(0, 0)]);
    let state = SpatialState::new(BoardGeometry::new(-1, -2, 3, 5).unwrap())
        .unwrap()
        .with_piece(SpatialPiece::new(
            "mover",
            "rook",
            PieceColor::White,
            Coord::new(0, -1),
            footprint.clone(),
        ))
        .unwrap()
        .with_piece(SpatialPiece::new(
            "victim",
            "pawn",
            PieceColor::Black,
            Coord::new(0, 1),
            footprint,
        ))
        .unwrap();
    let board = SpatialMoveBoard {
        state: &state,
        capture: |state: &SpatialState, mover: &str, victim: &str, allow_friendly| {
            allow_friendly
                || state.piece(mover).unwrap().color != state.piece(victim).unwrap().color
        },
        shift: |_state: &SpatialState, _mover: &str, _target: &str| true,
    };
    let output = program(vec![node(Primitive::TakeMove, (0, 1), 3)])
        .evaluate(&board, "mover", MoveProgramLimits::default())
        .unwrap();
    assert_eq!(output.raw.len(), 2);
    assert_eq!(output.raw[0].intent.selected, Coord::new(0, 0));
    assert_eq!(
        output.raw[1].resolution.captured_id.as_deref(),
        Some("victim")
    );
    assert_eq!(state.piece("mover").unwrap().anchor, Coord::new(0, -1));
}

/// Deliberately separate, small recursive oracle for singleton pieces. It
/// advances one square at a time and never calls the production frame,
/// placement, shift, or public-candidate helpers.
fn reference_simple(board: &TestBoard, program: &MoveProgram) -> Vec<RawMove> {
    fn visit(
        board: &TestBoard,
        program: &MoveProgram,
        node: &MoveNode,
        origin: Coord,
        path: Vec<usize>,
        ancestry: Vec<ActivationStep>,
        out: &mut Vec<RawMove>,
    ) {
        let mut jumped = false;
        for distance in 1..=node.max_distance.unwrap() {
            let selected = Coord::new(
                origin.row + node.direction.row * distance as i32,
                origin.col + node.direction.col * distance as i32,
            );
            if !board.is_usable(selected) {
                break;
            }
            let occupant = board.occupant(selected).filter(|id| *id != "mover");
            let mut stop = false;
            let mut resolution = None;
            let mut kind = PublicMoveKind::Travel;
            match node.primitive {
                Primitive::Move => {
                    if occupant.is_none() {
                        resolution = Some((selected, None, None));
                    } else {
                        stop = true;
                    }
                }
                Primitive::Take => {
                    if let Some(victim) = occupant {
                        stop = true;
                        if board.can_capture("mover", victim, false) {
                            resolution = Some((selected, Some(victim.to_owned()), None));
                        }
                    }
                }
                Primitive::TakeMove | Primitive::BothTakeMove => {
                    if let Some(victim) = occupant {
                        stop = true;
                        if board.can_capture(
                            "mover",
                            victim,
                            node.primitive == Primitive::BothTakeMove,
                        ) {
                            resolution = Some((selected, Some(victim.to_owned()), None));
                        }
                    } else {
                        resolution = Some((selected, None, None));
                    }
                }
                Primitive::Catch => {
                    if let Some(victim) = occupant {
                        stop = true;
                        if board.can_capture("mover", victim, false) {
                            kind = PublicMoveKind::Catch;
                            resolution = Some((Coord::new(1, 1), Some(victim.to_owned()), None));
                        }
                    }
                }
                Primitive::Jump => {
                    if let Some(victim) = occupant {
                        if jumped || !board.can_capture("mover", victim, false) {
                            stop = true;
                        } else {
                            jumped = true;
                        }
                    } else if jumped {
                        resolution = Some((selected, None, None));
                    }
                }
                Primitive::Shift => {
                    if let Some(target) = occupant {
                        stop = true;
                        kind = PublicMoveKind::Shift;
                        resolution =
                            Some((board.pieces[target].anchor, None, Some(target.to_owned())));
                    }
                }
            }
            if let Some((new_anchor, captured_id, shifted_id)) = resolution {
                let capture = captured_id.is_some();
                if node.activation_condition != ActivationCondition::NoCapture || !capture {
                    let mut next_ancestry = ancestry.clone();
                    next_ancestry.push(ActivationStep {
                        node_path: path.clone(),
                        current_origin: origin,
                        activated: selected,
                        distance,
                    });
                    if node.activation_condition != ActivationCondition::MustCapture || capture {
                        out.push(RawMove {
                            intent: PublicMoveIntent {
                                piece_id: "mover".into(),
                                original_origin: Coord::new(1, 1),
                                selected,
                                kind,
                            },
                            resolution: MoveResolution {
                                new_anchor,
                                captured_id,
                                shifted_id,
                            },
                            source_id: program.source_id.clone(),
                            modifier: None,
                            provenance: next_ancestry.clone(),
                        });
                    }
                    for (index, child) in node.children.iter().enumerate() {
                        if child
                            .activate_at_parent_distance
                            .is_none_or(|required| required == distance)
                        {
                            let mut child_path = path.clone();
                            child_path.push(index);
                            visit(
                                board,
                                program,
                                child,
                                selected,
                                child_path,
                                next_ancestry.clone(),
                                out,
                            );
                        }
                    }
                }
            }
            if stop {
                break;
            }
        }
    }

    let mut raw = Vec::new();
    for (index, root) in program.roots.iter().enumerate() {
        visit(
            board,
            program,
            root,
            Coord::new(1, 1),
            vec![index],
            Vec::new(),
            &mut raw,
        );
    }
    raw
}

#[test]
fn explicit_stack_matches_independent_recursive_oracle_for_singletons() {
    for occupant in [None, Some(('b', "enemy")), Some(('w', "friend"))] {
        let mut board = TestBoard::new(4, 5);
        board.add("mover", 'w', (1, 1), &[(0, 0)]);
        if let Some((side, id)) = occupant {
            board.add(id, side, (1, 3), &[(0, 0)]);
        }
        for primitive in [
            Primitive::Move,
            Primitive::Take,
            Primitive::TakeMove,
            Primitive::BothTakeMove,
            Primitive::Catch,
            Primitive::Jump,
            Primitive::Shift,
        ] {
            for condition in [
                ActivationCondition::Any,
                ActivationCondition::NoCapture,
                ActivationCondition::MustCapture,
            ] {
                let mut root = node(primitive, (0, 1), 3);
                root.activation_condition = condition;
                let mut child = node(Primitive::Move, (1, 0), 1);
                child.activate_at_parent_distance = Some(2);
                root.children.push(child);
                let program = program(vec![root]);
                let actual = program
                    .evaluate(&board, "mover", MoveProgramLimits::default())
                    .unwrap();
                let reference = reference_simple(&board, &program);
                assert_eq!(
                    actual.raw, reference,
                    "{primitive:?} {condition:?} {occupant:?}"
                );
            }
        }
    }
}
