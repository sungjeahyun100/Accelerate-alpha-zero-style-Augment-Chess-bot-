//! Composite movement for the explicit spatial rules path.
//!
//! The interpreter reads one immutable board throughout a forest. A parent
//! activation changes a child's *search origin*, never the board or the
//! action's original origin. `RawMove` deliberately retains duplicate paths;
//! callers collapse only equal public choices with `public_candidates`.

use crate::geometry::{Coord, Offset};
use crate::spatial_state::SpatialState;
use crate::state::Color;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Primitive {
    #[serde(rename = "MOVE")]
    Move,
    #[serde(rename = "TAKE")]
    Take,
    #[serde(rename = "TAKEMOVE")]
    TakeMove,
    #[serde(rename = "BOTHTAKEMOVE")]
    BothTakeMove,
    #[serde(rename = "CATCH")]
    Catch,
    #[serde(rename = "JUMP")]
    Jump,
    #[serde(rename = "SHIFT")]
    Shift,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ActivationCondition {
    #[serde(rename = "Any")]
    #[default]
    Any,
    #[serde(rename = "NoCapture")]
    NoCapture,
    #[serde(rename = "MustCapture")]
    MustCapture,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveNode {
    pub primitive: Primitive,
    pub direction: Offset,
    /// `None` scans until blocked or the execution limit is reached.
    #[serde(default)]
    pub max_distance: Option<u32>,
    #[serde(default)]
    pub activation_condition: ActivationCondition,
    /// A child runs only at this distance along its immediate parent ray.
    #[serde(default)]
    pub activate_at_parent_distance: Option<u32>,
    #[serde(default)]
    pub children: Vec<MoveNode>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveProgram {
    pub source_id: String,
    pub roots: Vec<MoveNode>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ModifierExpiration {
    Permanent,
    Actions { remaining: u32 },
    OwnerTurns { owner: Color, remaining: u32 },
    Activations { remaining: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveModifier {
    pub modifier_id: String,
    pub source: String,
    pub program: MoveProgram,
    pub expiration: ModifierExpiration,
}

impl MoveModifier {
    pub fn is_active(&self) -> bool {
        match self.expiration {
            ModifierExpiration::Permanent => true,
            ModifierExpiration::Actions { remaining }
            | ModifierExpiration::OwnerTurns { remaining, .. }
            | ModifierExpiration::Activations { remaining } => remaining > 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveProgramSet {
    pub base: MoveProgram,
    pub modifiers: Vec<MoveModifier>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MoveProgramLimits {
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_modifiers: usize,
    pub max_ray_distance: u32,
    pub max_examined: usize,
    pub max_raw_actions: usize,
}

impl Default for MoveProgramLimits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_nodes: 4_096,
            max_modifiers: 4_096,
            max_ray_distance: 4_096,
            max_examined: 100_000,
            max_raw_actions: 4_096,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MoveProgramError {
    InvalidProgram(&'static str),
    InvalidBoard(String),
    StaleCursor,
    LimitExceeded {
        resource: &'static str,
        limit: usize,
    },
}

impl fmt::Display for MoveProgramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProgram(reason) => write!(f, "invalid move program: {reason}"),
            Self::InvalidBoard(reason) => write!(f, "invalid movement board: {reason}"),
            Self::StaleCursor => f.write_str("move cursor belongs to another board revision"),
            Self::LimitExceeded { resource, limit } => {
                write!(f, "move program {resource} limit exceeded ({limit})")
            }
        }
    }
}

impl std::error::Error for MoveProgramError {}

/// Board data needed by the interpreter. Source capture restrictions can be
/// supplied by the selected rules profile without introducing a second board.
pub trait MoveBoard {
    fn is_usable(&self, square: Coord) -> bool;
    fn occupant(&self, square: Coord) -> Option<&str>;
    fn piece(&self, id: &str) -> Option<MovePiece<'_>>;
    fn can_capture(&self, mover: &str, victim: &str, allow_friendly: bool) -> bool;
    fn revision(&self) -> Option<u64> {
        None
    }
    fn can_shift(&self, _mover: &str, _target: &str) -> bool {
        true
    }
}

/// Connects the canonical spatial state to rule-specific capture and swap
/// predicates. The selected rules profile must supply both predicates; this
/// boundary does not guess whether an ally or a protected target is eligible.
pub struct SpatialMoveBoard<'a, Capture, Shift> {
    pub state: &'a SpatialState,
    pub capture: Capture,
    pub shift: Shift,
}

impl<Capture, Shift> MoveBoard for SpatialMoveBoard<'_, Capture, Shift>
where
    Capture: Fn(&SpatialState, &str, &str, bool) -> bool,
    Shift: Fn(&SpatialState, &str, &str) -> bool,
{
    fn is_usable(&self, square: Coord) -> bool {
        self.state.is_usable(square)
    }

    fn occupant(&self, square: Coord) -> Option<&str> {
        self.state.piece_at(square).map(|piece| piece.id.as_str())
    }

    fn piece(&self, id: &str) -> Option<MovePiece<'_>> {
        self.state.piece(id).map(|piece| MovePiece {
            id: &piece.id,
            anchor: piece.anchor,
            footprint: &piece.footprint,
        })
    }

    fn can_capture(&self, mover: &str, victim: &str, allow_friendly: bool) -> bool {
        (self.capture)(self.state, mover, victim, allow_friendly)
    }

    fn can_shift(&self, mover: &str, target: &str) -> bool {
        (self.shift)(self.state, mover, target)
    }

    fn revision(&self) -> Option<u64> {
        Some(self.state.revision())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MovePiece<'a> {
    pub id: &'a str,
    pub anchor: Coord,
    pub footprint: &'a BTreeSet<Offset>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PublicMoveKind {
    Travel,
    Catch,
    Shift,
}

/// A public selection is independent of hidden capture flags and the path
/// that produced it. A shift keeps the clicked body cell as its selection.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct PublicMoveIntent {
    pub piece_id: String,
    pub original_origin: Coord,
    pub selected: Coord,
    pub kind: PublicMoveKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveResolution {
    pub new_anchor: Coord,
    pub captured_id: Option<String>,
    pub shifted_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivationStep {
    pub node_path: Vec<usize>,
    pub current_origin: Coord,
    pub activated: Coord,
    pub distance: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawMove {
    pub intent: PublicMoveIntent,
    pub resolution: MoveResolution,
    pub source_id: String,
    pub modifier: Option<ModifierOrigin>,
    pub provenance: Vec<ActivationStep>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModifierOrigin {
    pub modifier_id: String,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicCandidate {
    pub intent: PublicMoveIntent,
    pub raw_indices: Vec<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveProgramOutput {
    pub raw: Vec<RawMove>,
    pub examined: usize,
}

impl MoveProgramOutput {
    pub fn public_candidates(&self) -> Vec<PublicCandidate> {
        let mut positions = BTreeMap::<PublicMoveIntent, usize>::new();
        let mut candidates = Vec::<PublicCandidate>::new();
        for (raw_index, raw) in self.raw.iter().enumerate() {
            if let Some(&index) = positions.get(&raw.intent) {
                candidates[index].raw_indices.push(raw_index);
            } else {
                positions.insert(raw.intent.clone(), candidates.len());
                candidates.push(PublicCandidate {
                    intent: raw.intent.clone(),
                    raw_indices: vec![raw_index],
                });
            }
        }
        candidates
    }
}

impl MoveProgramSet {
    /// Base movement and live modifiers use the same immutable board. Their
    /// raw paths stay separate; deduplication is available only at the public
    /// intent boundary. Expiration counters are consumed by the transition
    /// layer, which owns action and turn order.
    pub fn cursor<'a, B: MoveBoard>(
        &'a self,
        board: &'a B,
        mover_id: &'a str,
        limits: MoveProgramLimits,
    ) -> Result<MoveProgramSetCursor<'a, B>, MoveProgramError> {
        MoveProgramSetCursor::new(self, board, mover_id, limits)
    }

    pub fn evaluate<B: MoveBoard>(
        &self,
        board: &B,
        mover_id: &str,
        limits: MoveProgramLimits,
    ) -> Result<MoveProgramOutput, MoveProgramError> {
        let mut cursor = self.cursor(board, mover_id, limits)?;
        let mut output = MoveProgramOutput {
            raw: Vec::new(),
            examined: 0,
        };
        loop {
            let page = cursor.next_page(usize::MAX, limits.max_examined)?;
            output.examined += page.examined;
            output.raw.extend(page.raw);
            if page.exhausted {
                return Ok(output);
            }
            if page.examined == 0 {
                return Err(MoveProgramError::InvalidBoard(
                    "move set cursor made no progress".into(),
                ));
            }
        }
    }
}

struct Frame<'a> {
    node: &'a MoveNode,
    node_path: Vec<usize>,
    current_origin: Coord,
    next_distance: u32,
    jumped: bool,
    ancestry: Vec<ActivationStep>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveProgramPage {
    pub raw: Vec<RawMove>,
    pub examined: usize,
    pub exhausted: bool,
}

/// Owns the unfinished activation stack. Each page accounts for every probed
/// cell, including rejected and duplicate paths. The board reference remains
/// fixed for the cursor lifetime and a revision-bearing board is rechecked.
pub struct MoveProgramCursor<'a, B: MoveBoard> {
    program: &'a MoveProgram,
    board: &'a B,
    mover_id: &'a str,
    mover: MovePiece<'a>,
    limits: MoveProgramLimits,
    stack: Vec<Frame<'a>>,
    examined: usize,
    emitted: usize,
    revision: Option<u64>,
    failed: bool,
}

struct ProgramLane<'a, B: MoveBoard> {
    cursor: MoveProgramCursor<'a, B>,
    modifier: Option<ModifierOrigin>,
}

/// Enumerates base movement followed by live modifiers using one shared
/// examination and action budget. The source order matches eager evaluation.
pub struct MoveProgramSetCursor<'a, B: MoveBoard> {
    board: &'a B,
    lanes: Vec<ProgramLane<'a, B>>,
    index: usize,
    limits: MoveProgramLimits,
    revision: Option<u64>,
    examined: usize,
    emitted: usize,
    failed: bool,
}

#[derive(Clone, Copy)]
enum Occupancy<'a> {
    Empty,
    Other(&'a str),
}

impl MoveProgram {
    pub fn validate(&self, limits: MoveProgramLimits) -> Result<(), MoveProgramError> {
        self.validated_nodes(limits).map(|_| ())
    }

    fn validated_nodes(&self, limits: MoveProgramLimits) -> Result<usize, MoveProgramError> {
        if self.source_id.is_empty() {
            return Err(MoveProgramError::InvalidProgram("empty source id"));
        }
        if limits.max_depth == 0
            || limits.max_nodes == 0
            || limits.max_ray_distance == 0
            || limits.max_examined == 0
        {
            return Err(MoveProgramError::InvalidProgram("zero execution limit"));
        }
        let mut count = 0usize;
        let mut stack: Vec<(&MoveNode, usize, bool)> =
            self.roots.iter().map(|node| (node, 1, true)).collect();
        while let Some((node, depth, root)) = stack.pop() {
            count = count
                .checked_add(1)
                .ok_or(MoveProgramError::LimitExceeded {
                    resource: "nodes",
                    limit: limits.max_nodes,
                })?;
            if count > limits.max_nodes {
                return Err(MoveProgramError::LimitExceeded {
                    resource: "nodes",
                    limit: limits.max_nodes,
                });
            }
            if depth > limits.max_depth {
                return Err(MoveProgramError::LimitExceeded {
                    resource: "depth",
                    limit: limits.max_depth,
                });
            }
            if node.direction.row == 0 && node.direction.col == 0 {
                return Err(MoveProgramError::InvalidProgram("zero direction"));
            }
            if node.max_distance == Some(0) {
                return Err(MoveProgramError::InvalidProgram("zero ray distance"));
            }
            if root && node.activate_at_parent_distance.is_some() {
                return Err(MoveProgramError::InvalidProgram("root has parent distance"));
            }
            if node.activate_at_parent_distance == Some(0) {
                return Err(MoveProgramError::InvalidProgram("zero parent distance"));
            }
            stack.extend(node.children.iter().map(|child| (child, depth + 1, false)));
        }
        Ok(count)
    }

    pub fn cursor<'a, B: MoveBoard>(
        &'a self,
        board: &'a B,
        mover_id: &'a str,
        limits: MoveProgramLimits,
    ) -> Result<MoveProgramCursor<'a, B>, MoveProgramError> {
        MoveProgramCursor::new(self, board, mover_id, limits)
    }

    pub fn evaluate<B: MoveBoard>(
        &self,
        board: &B,
        mover_id: &str,
        limits: MoveProgramLimits,
    ) -> Result<MoveProgramOutput, MoveProgramError> {
        let mut cursor = self.cursor(board, mover_id, limits)?;
        let mut output = MoveProgramOutput {
            raw: Vec::new(),
            examined: 0,
        };
        loop {
            let page = cursor.next_page(usize::MAX, limits.max_examined)?;
            output.examined += page.examined;
            output.raw.extend(page.raw);
            if page.exhausted {
                return Ok(output);
            }
            if page.examined == 0 {
                return Err(MoveProgramError::InvalidBoard(
                    "move cursor made no progress".into(),
                ));
            }
        }
    }
}

impl<'a, B: MoveBoard> MoveProgramCursor<'a, B> {
    pub fn new(
        program: &'a MoveProgram,
        board: &'a B,
        mover_id: &'a str,
        limits: MoveProgramLimits,
    ) -> Result<Self, MoveProgramError> {
        program.validate(limits)?;
        let mover = board.piece(mover_id).ok_or_else(|| {
            MoveProgramError::InvalidBoard(format!("missing moving piece {mover_id}"))
        })?;
        if mover.id != mover_id || mover.footprint.is_empty() {
            return Err(MoveProgramError::InvalidBoard(
                "moving piece identity or footprint invalid".into(),
            ));
        }
        for occupied in footprint_at(&mover, mover.anchor)? {
            if !board.is_usable(occupied) || board.occupant(occupied) != Some(mover_id) {
                return Err(MoveProgramError::InvalidBoard(
                    "moving piece footprint differs from board occupancy".into(),
                ));
            }
        }
        let mut stack = Vec::with_capacity(program.roots.len());
        for (root_index, root) in program.roots.iter().enumerate().rev() {
            stack.push(Frame {
                node: root,
                node_path: vec![root_index],
                current_origin: mover.anchor,
                next_distance: 1,
                jumped: false,
                ancestry: Vec::new(),
            });
        }
        Ok(Self {
            program,
            board,
            mover_id,
            mover,
            limits,
            stack,
            examined: 0,
            emitted: 0,
            revision: board.revision(),
            failed: false,
        })
    }

    pub fn is_exhausted(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn next_page(
        &mut self,
        max_actions: usize,
        work_budget: usize,
    ) -> Result<MoveProgramPage, MoveProgramError> {
        if max_actions == 0 || work_budget == 0 {
            return Err(MoveProgramError::InvalidProgram("zero page budget"));
        }
        if self.failed {
            return Err(MoveProgramError::InvalidProgram("cursor already failed"));
        }
        if self.revision != self.board.revision() {
            self.failed = true;
            return Err(MoveProgramError::StaleCursor);
        }
        let examined_before = self.examined;
        let mut raw = Vec::new();
        while raw.len() < max_actions
            && self.examined - examined_before < work_budget
            && !self.stack.is_empty()
        {
            let advanced = self.advance_one();
            if self.revision != self.board.revision() {
                self.failed = true;
                return Err(MoveProgramError::StaleCursor);
            }
            match advanced {
                Ok(Some(action)) => raw.push(action),
                Ok(None) => {}
                Err(error) => {
                    self.failed = true;
                    return Err(error);
                }
            }
        }
        self.drop_completed_frames();
        Ok(MoveProgramPage {
            raw,
            examined: self.examined - examined_before,
            exhausted: self.stack.is_empty(),
        })
    }

    fn drop_completed_frames(&mut self) {
        while self
            .stack
            .last()
            .is_some_and(|frame| frame.next_distance > frame.node.max_distance.unwrap_or(u32::MAX))
        {
            self.stack.pop();
        }
    }

    fn advance_one(&mut self) -> Result<Option<RawMove>, MoveProgramError> {
        let board = self.board;
        let mover_id = self.mover_id;
        let mover = self.mover;
        let mut produced = None;
        while let Some(mut frame) = self.stack.pop() {
            let distance = frame.next_distance;
            if distance > frame.node.max_distance.unwrap_or(u32::MAX) {
                continue;
            }
            if distance > self.limits.max_ray_distance {
                return Err(MoveProgramError::LimitExceeded {
                    resource: "ray distance",
                    limit: self.limits.max_ray_distance as usize,
                });
            }
            if self.examined >= self.limits.max_examined {
                return Err(MoveProgramError::LimitExceeded {
                    resource: "examined cells",
                    limit: self.limits.max_examined,
                });
            }
            self.examined += 1;
            let square = checked_ray_square(frame.current_origin, frame.node.direction, distance)?;
            if !board.is_usable(square) {
                return Ok(None);
            }
            let occupancy = match board.occupant(square) {
                None => Occupancy::Empty,
                Some(id) if id == mover_id => Occupancy::Empty,
                Some(id) => {
                    let occupied_piece = board.piece(id).ok_or_else(|| {
                        MoveProgramError::InvalidBoard(format!(
                            "occupancy names missing piece {id}"
                        ))
                    })?;
                    if occupied_piece.id != id
                        || !footprint_at(&occupied_piece, occupied_piece.anchor)?.contains(&square)
                    {
                        return Err(MoveProgramError::InvalidBoard(
                            "occupancy differs from target footprint".into(),
                        ));
                    }
                    Occupancy::Other(id)
                }
            };
            let mut continue_ray = true;
            let mut activated = None;
            match frame.node.primitive {
                Primitive::Move => match occupancy {
                    Occupancy::Empty => {
                        if placement_valid(board, &mover, square, None)? {
                            activated = Some((PublicMoveKind::Travel, square, None, None));
                        } else {
                            continue_ray = false;
                        }
                    }
                    Occupancy::Other(_) => continue_ray = false,
                },
                Primitive::Take | Primitive::TakeMove | Primitive::BothTakeMove => {
                    match occupancy {
                        Occupancy::Empty => {
                            if placement_valid(board, &mover, square, None)? {
                                if frame.node.primitive != Primitive::Take {
                                    activated = Some((PublicMoveKind::Travel, square, None, None));
                                }
                            } else {
                                continue_ray = false;
                            }
                        }
                        Occupancy::Other(victim) => {
                            continue_ray = false;
                            let friendly = frame.node.primitive == Primitive::BothTakeMove;
                            if board.can_capture(mover_id, victim, friendly)
                                && placement_valid(board, &mover, square, Some(victim))?
                            {
                                activated = Some((
                                    PublicMoveKind::Travel,
                                    square,
                                    Some(victim.to_owned()),
                                    None,
                                ));
                            }
                        }
                    }
                }
                Primitive::Catch => match occupancy {
                    Occupancy::Empty => {}
                    Occupancy::Other(victim) => {
                        continue_ray = false;
                        if board.can_capture(mover_id, victim, false) {
                            activated = Some((
                                PublicMoveKind::Catch,
                                mover.anchor,
                                Some(victim.to_owned()),
                                None,
                            ));
                        }
                    }
                },
                Primitive::Jump => match occupancy {
                    Occupancy::Other(victim) => {
                        if frame.jumped || !board.can_capture(mover_id, victim, false) {
                            continue_ray = false;
                        } else {
                            frame.jumped = true;
                        }
                    }
                    Occupancy::Empty if frame.jumped => {
                        if placement_valid(board, &mover, square, None)? {
                            activated = Some((PublicMoveKind::Travel, square, None, None));
                        } else {
                            continue_ray = false;
                        }
                    }
                    Occupancy::Empty => {}
                },
                Primitive::Shift => match occupancy {
                    Occupancy::Empty => {}
                    Occupancy::Other(target) => {
                        continue_ray = false;
                        if board.can_shift(mover_id, target) && shift_valid(board, &mover, target)?
                        {
                            let target_piece = board.piece(target).ok_or_else(|| {
                                MoveProgramError::InvalidBoard(format!(
                                    "occupancy names missing shift target {target}"
                                ))
                            })?;
                            activated = Some((
                                PublicMoveKind::Shift,
                                target_piece.anchor,
                                None,
                                Some(target.to_owned()),
                            ));
                        }
                    }
                },
            }
            if continue_ray {
                frame.next_distance =
                    distance
                        .checked_add(1)
                        .ok_or(MoveProgramError::LimitExceeded {
                            resource: "ray distance",
                            limit: self.limits.max_ray_distance as usize,
                        })?;
                self.stack.push(Frame {
                    node: frame.node,
                    node_path: frame.node_path.clone(),
                    current_origin: frame.current_origin,
                    next_distance: frame.next_distance,
                    jumped: frame.jumped,
                    ancestry: frame.ancestry.clone(),
                });
            }
            if let Some((kind, new_anchor, captured_id, shifted_id)) = activated {
                let is_capture = captured_id.is_some();
                if !(is_capture
                    && frame.node.activation_condition == ActivationCondition::NoCapture)
                {
                    let step = ActivationStep {
                        node_path: frame.node_path.clone(),
                        current_origin: frame.current_origin,
                        activated: square,
                        distance,
                    };
                    let mut provenance = frame.ancestry;
                    provenance.push(step);
                    if !(frame.node.activation_condition == ActivationCondition::MustCapture
                        && !is_capture)
                    {
                        if self.emitted >= self.limits.max_raw_actions {
                            return Err(MoveProgramError::LimitExceeded {
                                resource: "raw actions",
                                limit: self.limits.max_raw_actions,
                            });
                        }
                        produced = Some(RawMove {
                            intent: PublicMoveIntent {
                                piece_id: mover_id.to_owned(),
                                original_origin: mover.anchor,
                                selected: square,
                                kind,
                            },
                            resolution: MoveResolution {
                                new_anchor,
                                captured_id,
                                shifted_id,
                            },
                            source_id: self.program.source_id.clone(),
                            modifier: None,
                            provenance: provenance.clone(),
                        });
                        self.emitted += 1;
                    }
                    for (child_index, child) in frame.node.children.iter().enumerate().rev() {
                        if child
                            .activate_at_parent_distance
                            .is_some_and(|required| required != distance)
                        {
                            continue;
                        }
                        let mut node_path = frame.node_path.clone();
                        node_path.push(child_index);
                        self.stack.push(Frame {
                            node: child,
                            node_path,
                            current_origin: square,
                            next_distance: 1,
                            jumped: false,
                            ancestry: provenance.clone(),
                        });
                    }
                }
            }
            return Ok(produced);
        }
        Ok(None)
    }
}

impl<'a, B: MoveBoard> MoveProgramSetCursor<'a, B> {
    pub fn new(
        programs: &'a MoveProgramSet,
        board: &'a B,
        mover_id: &'a str,
        limits: MoveProgramLimits,
    ) -> Result<Self, MoveProgramError> {
        if programs.modifiers.len() > limits.max_modifiers {
            return Err(MoveProgramError::LimitExceeded {
                resource: "modifiers",
                limit: limits.max_modifiers,
            });
        }
        let mut modifier_ids = BTreeSet::new();
        for modifier in &programs.modifiers {
            if modifier.modifier_id.is_empty()
                || modifier.source.is_empty()
                || !modifier_ids.insert(&modifier.modifier_id)
            {
                return Err(MoveProgramError::InvalidProgram(
                    "invalid or duplicate modifier identity",
                ));
            }
        }
        let selected = std::iter::once((&programs.base, None)).chain(
            programs
                .modifiers
                .iter()
                .filter(|modifier| modifier.is_active())
                .map(|modifier| (&modifier.program, Some(modifier))),
        );
        let mut lanes = Vec::new();
        let mut remaining_nodes = limits.max_nodes;
        for (program, modifier) in selected {
            let nodes = program.validated_nodes(limits)?;
            if nodes > remaining_nodes {
                return Err(MoveProgramError::LimitExceeded {
                    resource: "nodes",
                    limit: limits.max_nodes,
                });
            }
            remaining_nodes -= nodes;
            lanes.push(ProgramLane {
                cursor: program.cursor(board, mover_id, limits)?,
                modifier: modifier.map(|modifier| ModifierOrigin {
                    modifier_id: modifier.modifier_id.clone(),
                    source: modifier.source.clone(),
                }),
            });
        }
        let mut result = Self {
            board,
            lanes,
            index: 0,
            limits,
            revision: board.revision(),
            examined: 0,
            emitted: 0,
            failed: false,
        };
        result.skip_exhausted_lanes();
        Ok(result)
    }

    pub fn is_exhausted(&self) -> bool {
        self.index == self.lanes.len()
    }

    pub fn next_page(
        &mut self,
        max_actions: usize,
        work_budget: usize,
    ) -> Result<MoveProgramPage, MoveProgramError> {
        if max_actions == 0 || work_budget == 0 {
            return Err(MoveProgramError::InvalidProgram("zero page budget"));
        }
        if self.failed {
            return Err(MoveProgramError::InvalidProgram("cursor already failed"));
        }
        if self.revision != self.board.revision() {
            self.failed = true;
            return Err(MoveProgramError::StaleCursor);
        }
        let mut page = MoveProgramPage {
            raw: Vec::new(),
            examined: 0,
            exhausted: false,
        };
        while page.raw.len() < max_actions && page.examined < work_budget && !self.is_exhausted() {
            let remaining_examined = self.limits.max_examined - self.examined;
            if remaining_examined == 0 {
                self.failed = true;
                return Err(MoveProgramError::LimitExceeded {
                    resource: "examined cells",
                    limit: self.limits.max_examined,
                });
            }
            let lane_page = match self.lanes[self.index].cursor.next_page(
                max_actions - page.raw.len(),
                (work_budget - page.examined).min(remaining_examined),
            ) {
                Ok(lane_page) => lane_page,
                Err(error) => {
                    self.failed = true;
                    return Err(error);
                }
            };
            self.examined += lane_page.examined;
            page.examined += lane_page.examined;
            for mut raw in lane_page.raw {
                if self.emitted >= self.limits.max_raw_actions {
                    self.failed = true;
                    return Err(MoveProgramError::LimitExceeded {
                        resource: "raw actions",
                        limit: self.limits.max_raw_actions,
                    });
                }
                raw.modifier = self.lanes[self.index].modifier.clone();
                self.emitted += 1;
                page.raw.push(raw);
            }
            if lane_page.exhausted {
                self.index += 1;
                self.skip_exhausted_lanes();
            } else if lane_page.examined == 0 {
                self.failed = true;
                return Err(MoveProgramError::InvalidBoard(
                    "move set cursor made no progress".into(),
                ));
            }
        }
        page.exhausted = self.is_exhausted();
        Ok(page)
    }

    fn skip_exhausted_lanes(&mut self) {
        while self.index < self.lanes.len() && self.lanes[self.index].cursor.is_exhausted() {
            self.index += 1;
        }
    }
}

fn checked_ray_square(
    origin: Coord,
    direction: Offset,
    distance: u32,
) -> Result<Coord, MoveProgramError> {
    let row = i64::from(origin.row) + i64::from(direction.row) * i64::from(distance);
    let col = i64::from(origin.col) + i64::from(direction.col) * i64::from(distance);
    Ok(Coord {
        row: i32::try_from(row).map_err(|_| MoveProgramError::InvalidProgram("row overflow"))?,
        col: i32::try_from(col).map_err(|_| MoveProgramError::InvalidProgram("col overflow"))?,
    })
}

fn footprint_at(piece: &MovePiece<'_>, anchor: Coord) -> Result<BTreeSet<Coord>, MoveProgramError> {
    piece
        .footprint
        .iter()
        .map(|offset| {
            Ok(Coord {
                row: anchor.row.checked_add(offset.row).ok_or_else(|| {
                    MoveProgramError::InvalidBoard("footprint row overflow".into())
                })?,
                col: anchor.col.checked_add(offset.col).ok_or_else(|| {
                    MoveProgramError::InvalidBoard("footprint col overflow".into())
                })?,
            })
        })
        .collect()
}

fn placement_valid<B: MoveBoard>(
    board: &B,
    mover: &MovePiece<'_>,
    anchor: Coord,
    removed_id: Option<&str>,
) -> Result<bool, MoveProgramError> {
    for square in footprint_at(mover, anchor)? {
        if !board.is_usable(square)
            || board
                .occupant(square)
                .is_some_and(|id| id != mover.id && Some(id) != removed_id)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn shift_valid<B: MoveBoard>(
    board: &B,
    mover: &MovePiece<'_>,
    target_id: &str,
) -> Result<bool, MoveProgramError> {
    let target = board.piece(target_id).ok_or_else(|| {
        MoveProgramError::InvalidBoard(format!("occupancy names missing shift target {target_id}"))
    })?;
    if target.id != target_id || target.footprint.is_empty() {
        return Err(MoveProgramError::InvalidBoard(
            "shift target identity or footprint invalid".into(),
        ));
    }
    for occupied in footprint_at(&target, target.anchor)? {
        if !board.is_usable(occupied) || board.occupant(occupied) != Some(target_id) {
            return Err(MoveProgramError::InvalidBoard(
                "shift target footprint differs from board occupancy".into(),
            ));
        }
    }
    let mover_after = footprint_at(mover, target.anchor)?;
    let target_after = footprint_at(&target, mover.anchor)?;
    if !mover_after.is_disjoint(&target_after) {
        return Ok(false);
    }
    for square in mover_after.iter().chain(&target_after) {
        if !board.is_usable(*square)
            || board
                .occupant(*square)
                .is_some_and(|id| id != mover.id && id != target_id)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
#[path = "move_program_tests.rs"]
mod tests;
