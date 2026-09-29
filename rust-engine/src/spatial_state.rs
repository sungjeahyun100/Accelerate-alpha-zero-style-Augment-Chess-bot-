//! Canonical variable-geometry board state for the compositional rules engine.

use crate::geometry::{BoardGeometry, Coord, MAX_ENGINE_CELLS, Offset};
use crate::state::{
    EngineError, Fields, GameState, Piece, PieceColor, RULES_VERSION_V6, Result, Square,
    validate_json_value,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

// This state has its own snapshot protocol; source-shaped GameState continues
// to own the v6 wire format and execution path.
pub const SPATIAL_SNAPSHOT_VERSION: &str = "accelerate-spatial-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpatialProfile {
    #[serde(rename = "source-v6")]
    SourceV6,
    #[serde(rename = "source-v7")]
    SourceV7,
    #[serde(rename = "synthetic-geometry-v1")]
    SyntheticGeometryV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellState {
    /// A collapsed cell remains inside the rectangle but cannot be occupied.
    pub usable: bool,
    pub terrain: BTreeSet<String>,
}

impl CellState {
    pub fn open() -> Self {
        Self {
            usable: true,
            terrain: BTreeSet::new(),
        }
    }
}

impl Default for CellState {
    fn default() -> Self {
        Self::open()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoardState {
    geometry: BoardGeometry,
    cells: Vec<CellState>,
}

impl BoardState {
    pub fn new(geometry: BoardGeometry) -> Result<Self> {
        geometry.validate()?;
        Ok(Self {
            geometry,
            cells: vec![CellState::open(); geometry.area()],
        })
    }

    pub fn geometry(&self) -> BoardGeometry {
        self.geometry
    }

    pub fn cell(&self, at: Coord) -> Option<&CellState> {
        self.cells.get(self.geometry.index(at)?)
    }

    pub fn is_usable(&self, at: Coord) -> bool {
        self.cell(at).is_some_and(|cell| cell.usable)
    }

    fn validate(&self) -> Result<()> {
        self.geometry.validate()?;
        if self.cells.len() != self.geometry.area() {
            return Err(EngineError::InvalidState(
                "board cell count does not match geometry".into(),
            ));
        }
        Ok(())
    }
}

/// One record per physical piece. A footprint contains exactly the occupied
/// offsets, including (0,0) only when the anchor cell is actually occupied.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpatialPiece {
    pub id: String,
    pub kind: String,
    pub color: PieceColor,
    pub moved: bool,
    pub anchor: Coord,
    pub footprint: BTreeSet<Offset>,
    pub attributes: Fields,
    #[serde(default)]
    source_order: Vec<String>,
}

impl SpatialPiece {
    pub fn new(
        id: impl Into<String>,
        kind: impl Into<String>,
        color: PieceColor,
        anchor: Coord,
        footprint: BTreeSet<Offset>,
    ) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            color,
            moved: false,
            anchor,
            footprint,
            attributes: Fields::new(),
            source_order: Vec::new(),
        }
    }

    pub fn occupied_cells(&self) -> Result<Vec<Coord>> {
        if self.footprint.len() > MAX_ENGINE_CELLS {
            return Err(EngineError::InvalidState(format!(
                "piece {} footprint exceeds {MAX_ENGINE_CELLS} cells",
                self.id
            )));
        }
        self.footprint
            .iter()
            .map(|&offset| {
                self.anchor.offset(offset).ok_or_else(|| {
                    EngineError::InvalidState(format!("piece {} footprint overflows i32", self.id))
                })
            })
            .collect()
    }

    fn from_source_board(piece: Piece, anchor: Coord, cells: BTreeSet<Coord>) -> Result<Self> {
        let footprint = cells
            .into_iter()
            .map(|cell| {
                Offset::between(anchor, cell).ok_or_else(|| {
                    EngineError::InvalidState("source footprint offset overflows i32".into())
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            id: piece.id,
            kind: piece.kind,
            color: piece.color,
            moved: piece.moved,
            anchor,
            footprint,
            attributes: piece.extra,
            source_order: piece.source_order,
        })
    }

    fn to_legacy(&self) -> Piece {
        Piece {
            id: self.id.clone(),
            kind: self.kind.clone(),
            color: self.color,
            moved: self.moved,
            extra: self.attributes.clone(),
            source_order: self.source_order.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PieceStore(BTreeMap<String, SpatialPiece>);

impl PieceStore {
    pub fn get(&self, id: &str) -> Option<&SpatialPiece> {
        self.0.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &SpatialPiece> {
        self.0.values()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Rebuilt from PieceStore after every spatial transition. The index is never
/// serialized and cannot independently become the authority for piece state.
#[derive(Clone, Debug, PartialEq)]
pub struct OccupancyIndex {
    cells: Vec<Option<String>>,
}

impl OccupancyIndex {
    fn rebuild(board: &BoardState, pieces: &PieceStore) -> Result<Self> {
        board.validate()?;
        if pieces.len() > MAX_ENGINE_CELLS {
            return Err(EngineError::InvalidState(format!(
                "piece count exceeds {MAX_ENGINE_CELLS}"
            )));
        }
        let mut cells = vec![None; board.geometry.area()];
        for (key, piece) in &pieces.0 {
            if key.is_empty() || piece.id != *key || piece.kind.is_empty() {
                return Err(EngineError::InvalidState(
                    "invalid piece identity or kind".into(),
                ));
            }
            if !board.geometry.contains(piece.anchor)
                || piece.footprint.is_empty()
                || piece.footprint.len() > board.geometry.area()
            {
                return Err(EngineError::InvalidState(format!(
                    "piece {} has invalid anchor or footprint size",
                    piece.id
                )));
            }
            for occupied in piece.occupied_cells()? {
                let index = board.geometry.index(occupied).ok_or_else(|| {
                    EngineError::InvalidState(format!("piece {} is outside geometry", piece.id))
                })?;
                if !board.cells[index].usable {
                    return Err(EngineError::InvalidState(format!(
                        "piece {} occupies an unusable cell",
                        piece.id
                    )));
                }
                if let Some(previous) = &cells[index] {
                    return Err(EngineError::InvalidState(format!(
                        "pieces {previous} and {} overlap",
                        piece.id
                    )));
                }
                cells[index] = Some(piece.id.clone());
            }
        }
        Ok(Self { cells })
    }

    pub fn piece_id_at(&self, geometry: BoardGeometry, at: Coord) -> Option<&str> {
        self.cells.get(geometry.index(at)?)?.as_deref()
    }
}

/// Geometry-bearing relationships need an explicit clipping policy on resize.
/// Their game effects are owned by the upper rule layer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpatialReference {
    pub id: String,
    pub cells: BTreeSet<Coord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizePolicy {
    RejectAffected,
    /// Removes clipped pieces, links, and scheduled effects. Nondefault cell
    /// state must be cleared explicitly before its coordinate is removed.
    RemoveAffectedEntities,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResizeRequest {
    pub geometry: BoardGeometry,
    /// State for every newly added coordinate; retained cells keep their state.
    pub added_cells: BTreeMap<Coord, CellState>,
    pub policy: ResizePolicy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpatialSnapshot {
    protocol_version: String,
    profile: SpatialProfile,
    revision: u64,
    board: BoardState,
    pieces: PieceStore,
    links: BTreeMap<String, SpatialReference>,
    scheduled_effects: BTreeMap<String, SpatialReference>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpatialState {
    profile: SpatialProfile,
    revision: u64,
    board: BoardState,
    pieces: PieceStore,
    occupancy: OccupancyIndex,
    links: BTreeMap<String, SpatialReference>,
    scheduled_effects: BTreeMap<String, SpatialReference>,
}

impl SpatialState {
    pub fn new(geometry: BoardGeometry) -> Result<Self> {
        Self::new_with_profile(SpatialProfile::SyntheticGeometryV1, geometry)
    }

    pub fn new_with_profile(profile: SpatialProfile, geometry: BoardGeometry) -> Result<Self> {
        let board = BoardState::new(geometry)?;
        let pieces = PieceStore::default();
        let occupancy = OccupancyIndex::rebuild(&board, &pieces)?;
        let state = Self {
            profile,
            revision: 0,
            board,
            pieces,
            occupancy,
            links: BTreeMap::new(),
            scheduled_effects: BTreeMap::new(),
        };
        state.validate()?;
        Ok(state)
    }

    pub fn profile(&self) -> SpatialProfile {
        self.profile
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn geometry(&self) -> BoardGeometry {
        self.board.geometry()
    }

    pub fn cell(&self, at: Coord) -> Option<&CellState> {
        self.board.cell(at)
    }

    pub fn is_usable(&self, at: Coord) -> bool {
        self.board.is_usable(at)
    }

    pub fn piece(&self, id: &str) -> Option<&SpatialPiece> {
        self.pieces.get(id)
    }

    pub fn piece_at(&self, at: Coord) -> Option<&SpatialPiece> {
        self.occupancy
            .piece_id_at(self.geometry(), at)
            .and_then(|id| self.pieces.get(id))
    }

    pub fn pieces(&self) -> &PieceStore {
        &self.pieces
    }

    pub fn links(&self) -> &BTreeMap<String, SpatialReference> {
        &self.links
    }

    pub fn scheduled_effects(&self) -> &BTreeMap<String, SpatialReference> {
        &self.scheduled_effects
    }

    fn validate(&self) -> Result<()> {
        if self.revision > 9_007_199_254_740_991 {
            return Err(EngineError::InvalidState(
                "spatial revision exceeds JSON range".into(),
            ));
        }
        if self.profile != SpatialProfile::SyntheticGeometryV1
            && self.geometry() != BoardGeometry::new(0, 0, 8, 8)?
        {
            return Err(EngineError::InvalidState(
                "source spatial profile requires its fixed 8x8 extent".into(),
            ));
        }
        let rebuilt = OccupancyIndex::rebuild(&self.board, &self.pieces)?;
        if rebuilt != self.occupancy {
            return Err(EngineError::InvalidState(
                "occupancy differs from canonical pieces".into(),
            ));
        }
        if self.links.len() > MAX_ENGINE_CELLS || self.scheduled_effects.len() > MAX_ENGINE_CELLS {
            return Err(EngineError::InvalidState(
                "spatial reference count exceeds board execution bound".into(),
            ));
        }
        for reference in self.links.iter().chain(&self.scheduled_effects) {
            let (key, entry) = reference;
            if key.is_empty() || *key != entry.id || entry.cells.is_empty() {
                return Err(EngineError::InvalidState(
                    "invalid spatial reference identity or cells".into(),
                ));
            }
            if entry.cells.iter().any(|&cell| !self.is_usable(cell)) {
                return Err(EngineError::InvalidState(format!(
                    "spatial reference {key} targets an unavailable cell"
                )));
            }
        }
        for piece in self.pieces.iter() {
            for value in piece.attributes.values() {
                validate_json_value(value, 1)?;
            }
        }
        Ok(())
    }

    fn finish_transition(&self, mut next: Self) -> Result<Self> {
        next.revision = self
            .revision
            .checked_add(1)
            .filter(|revision| *revision <= 9_007_199_254_740_991)
            .ok_or_else(|| EngineError::InvalidState("spatial revision overflow".into()))?;
        next.occupancy = OccupancyIndex::rebuild(&next.board, &next.pieces)?;
        next.validate()?;
        Ok(next)
    }

    pub fn with_piece(&self, piece: SpatialPiece) -> Result<Self> {
        let mut next = self.clone();
        next.pieces.0.insert(piece.id.clone(), piece);
        self.finish_transition(next)
    }

    pub fn without_piece(&self, id: &str) -> Result<Self> {
        let mut next = self.clone();
        if next.pieces.0.remove(id).is_none() {
            return Err(EngineError::InvalidState(format!("unknown piece {id}")));
        }
        self.finish_transition(next)
    }

    pub fn with_cell(&self, at: Coord, cell: CellState) -> Result<Self> {
        let mut next = self.clone();
        let index = next
            .geometry()
            .index(at)
            .ok_or_else(|| EngineError::InvalidState("cell outside geometry".into()))?;
        next.board.cells[index] = cell;
        self.finish_transition(next)
    }

    pub fn with_link(&self, reference: SpatialReference) -> Result<Self> {
        if reference.cells.len() < 2 {
            return Err(EngineError::InvalidState(
                "spatial link needs at least two cells".into(),
            ));
        }
        let mut next = self.clone();
        next.links.insert(reference.id.clone(), reference);
        self.finish_transition(next)
    }

    pub fn with_scheduled_effect(&self, reference: SpatialReference) -> Result<Self> {
        let mut next = self.clone();
        next.scheduled_effects
            .insert(reference.id.clone(), reference);
        self.finish_transition(next)
    }

    /// Source v7 collapse changes usability inside the existing extent.
    /// A piece touching a collapsed cell is removed as one identity, including
    /// every other occupied cell. Links/effects need their source rule handler
    /// to decide their fate.
    pub fn collapse_cells(&self, collapsed: &BTreeSet<Coord>) -> Result<Self> {
        if self.profile != SpatialProfile::SourceV7 || collapsed.is_empty() {
            return Err(EngineError::InvalidState(
                "collapse requires source-v7 profile and nonempty cells".into(),
            ));
        }
        for &coord in collapsed {
            if !self.is_usable(coord) {
                return Err(EngineError::InvalidState(
                    "collapse cell is outside geometry or already unusable".into(),
                ));
            }
        }
        if self
            .links
            .values()
            .chain(self.scheduled_effects.values())
            .any(|reference| !reference.cells.is_disjoint(collapsed))
        {
            return Err(EngineError::UnsupportedFeature(
                "collapse of linked or scheduled cells needs its source effect handler".into(),
            ));
        }
        let mut next = self.clone();
        for &coord in collapsed {
            let index = next.geometry().index(coord).expect("checked above");
            next.board.cells[index].usable = false;
        }
        for piece in self.pieces.iter() {
            if piece
                .occupied_cells()?
                .iter()
                .any(|coord| collapsed.contains(coord))
            {
                next.pieces.0.remove(&piece.id);
            }
        }
        self.finish_transition(next)
    }

    /// The synthetic profile changes the outer rectangle. Every new cell is
    /// specified by the request, and affected entities use the named policy.
    pub fn resize_geometry(&self, request: ResizeRequest) -> Result<Self> {
        if self.profile != SpatialProfile::SyntheticGeometryV1 {
            return Err(EngineError::UnsupportedFeature(
                "outer resize is limited to synthetic-geometry-v1".into(),
            ));
        }
        request.geometry.validate()?;
        if request.geometry == self.geometry() {
            return Err(EngineError::InvalidState("geometry is unchanged".into()));
        }
        let added = request
            .geometry
            .coordinates()
            .filter(|&coord| !self.geometry().contains(coord))
            .collect::<BTreeSet<_>>();
        if added != request.added_cells.keys().copied().collect() {
            return Err(EngineError::InvalidState(
                "new cell state must cover exactly the added coordinates".into(),
            ));
        }
        if self.geometry().coordinates().any(|coord| {
            !request.geometry.contains(coord)
                && self
                    .cell(coord)
                    .is_some_and(|cell| *cell != CellState::open())
        }) {
            return Err(EngineError::InvalidState(
                "resize would discard nondefault cell state".into(),
            ));
        }
        let mut board = BoardState::new(request.geometry)?;
        for coord in request.geometry.coordinates() {
            let index = request
                .geometry
                .index(coord)
                .expect("enumerated coordinate");
            board.cells[index] = if let Some(previous) = self.cell(coord) {
                previous.clone()
            } else {
                request.added_cells[&coord].clone()
            };
        }
        let mut next = self.clone();
        next.board = board;
        let affected_piece = |piece: &SpatialPiece| {
            !request.geometry.contains(piece.anchor)
                || piece.occupied_cells().map_or(true, |cells| {
                    cells.iter().any(|&coord| !request.geometry.contains(coord))
                })
        };
        if request.policy == ResizePolicy::RejectAffected && next.pieces.iter().any(affected_piece)
        {
            return Err(EngineError::InvalidState(
                "resize would clip a piece or its anchor".into(),
            ));
        }
        if request.policy == ResizePolicy::RemoveAffectedEntities {
            next.pieces.0.retain(|_, piece| !affected_piece(piece));
        }
        for references in [&mut next.links, &mut next.scheduled_effects] {
            if request.policy == ResizePolicy::RejectAffected
                && references.values().any(|reference| {
                    reference
                        .cells
                        .iter()
                        .any(|&coord| !request.geometry.contains(coord))
                })
            {
                return Err(EngineError::InvalidState(
                    "resize would clip a link or scheduled effect".into(),
                ));
            }
            if request.policy == ResizePolicy::RemoveAffectedEntities {
                references.retain(|_, reference| {
                    reference
                        .cells
                        .iter()
                        .all(|&coord| request.geometry.contains(coord))
                });
            }
        }
        self.finish_transition(next)
    }

    fn snapshot(&self) -> SpatialSnapshot {
        SpatialSnapshot {
            protocol_version: SPATIAL_SNAPSHOT_VERSION.into(),
            profile: self.profile,
            revision: self.revision,
            board: self.board.clone(),
            pieces: self.pieces.clone(),
            links: self.links.clone(),
            scheduled_effects: self.scheduled_effects.clone(),
        }
    }

    pub fn to_snapshot_value(&self) -> Result<Value> {
        self.validate()?;
        serde_json::to_value(self.snapshot()).map_err(EngineError::serialization)
    }

    pub fn from_snapshot_value(value: Value) -> Result<Self> {
        validate_json_value(&value, 0)?;
        let snapshot: SpatialSnapshot = serde_json::from_value(value).map_err(|error| {
            EngineError::InvalidState(format!("invalid spatial snapshot: {error}"))
        })?;
        if snapshot.protocol_version != SPATIAL_SNAPSHOT_VERSION {
            return Err(EngineError::InvalidState(
                "unknown spatial snapshot protocol".into(),
            ));
        }
        let occupancy = OccupancyIndex::rebuild(&snapshot.board, &snapshot.pieces)?;
        let state = Self {
            profile: snapshot.profile,
            revision: snapshot.revision,
            board: snapshot.board,
            pieces: snapshot.pieces,
            occupancy,
            links: snapshot.links,
            scheduled_effects: snapshot.scheduled_effects,
        };
        state.validate()?;
        Ok(state)
    }

    pub fn position_key(&self) -> Result<String> {
        self.validate()?;
        let bytes = serde_jcs::to_vec(&self.snapshot())
            .map_err(|error| EngineError::Serialization(error.to_string()))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }

    pub fn check_binding(&self, position_key: &str) -> Result<()> {
        if self.position_key()? == position_key {
            Ok(())
        } else {
            Err(EngineError::StaleAction)
        }
    }

    /// Convert the validated source v6 board without changing its execution
    /// contract. Repeated board cells with one ID become one piece record.
    pub fn from_legacy(source: &GameState) -> Result<Self> {
        let mut source = source.clone();
        source.validate_and_identify()?;
        Self::from_validated_source_board(&source, SpatialProfile::SourceV6)
    }

    /// Project a source-shaped v7 board into canonical spatial identities.
    /// This only admits the DTO shape; it does not make v7 Position executable.
    pub fn from_v7_source(source: &GameState) -> Result<Self> {
        let mut source = source.clone();
        source.validate_v7_snapshot_shape_and_identify()?;
        Self::from_validated_source_board(&source, SpatialProfile::SourceV7)
    }

    fn from_validated_source_board(source: &GameState, profile: SpatialProfile) -> Result<Self> {
        let geometry = BoardGeometry::new(0, 0, 8, 8)?;
        let mut pieces = BTreeMap::<String, (Piece, Coord, BTreeSet<Coord>)>::new();
        for (row, cells) in source.board.iter().enumerate() {
            for (col, occupied) in cells.iter().enumerate() {
                let Some(piece) = occupied else { continue };
                let at = Coord::new(row as i32, col as i32);
                let parse_anchor = |field: &str, fallback: i32| -> Result<i32> {
                    match piece.extra.get(field) {
                        None => Ok(fallback),
                        Some(value) => value
                            .as_i64()
                            .and_then(|value| i32::try_from(value).ok())
                            .ok_or_else(|| {
                                EngineError::InvalidState(format!(
                                    "piece {} has invalid {field}",
                                    piece.id
                                ))
                            }),
                    }
                };
                let anchor = Coord::new(
                    parse_anchor("anchorRow", at.row)?,
                    parse_anchor("anchorCol", at.col)?,
                );
                match pieces.get_mut(&piece.id) {
                    Some((previous, _, footprint)) => {
                        if previous != piece {
                            return Err(EngineError::InvalidState(format!(
                                "conflicting identity {}",
                                piece.id
                            )));
                        }
                        footprint.insert(at);
                    }
                    None => {
                        pieces.insert(piece.id.clone(), (piece.clone(), anchor, [at].into()));
                    }
                }
            }
        }
        let mut next = Self::new_with_profile(profile, geometry)?;
        if profile == SpatialProfile::SourceV7 {
            // Source normalizeBlackHole ignores non-array values and cells
            // whose Number-coerced coordinates cannot name a board square.
            // The hazard is terrain, not a collapsed/unusable cell: source
            // applyBlackHoleDeaths removes occupants in a separate rule step.
            if let Some(black_holes) = source.extra.get("blackHole").and_then(Value::as_array) {
                for cell in black_holes {
                    let (Some(row), Some(col)) = (
                        crate::observation::number(cell.get("row")),
                        crate::observation::number(cell.get("col")),
                    ) else {
                        continue;
                    };
                    if row.fract() != 0.0
                        || col.fract() != 0.0
                        || !(0.0..8.0).contains(&row)
                        || !(0.0..8.0).contains(&col)
                    {
                        continue;
                    }
                    let index = geometry
                        .index(Coord::new(row as i32, col as i32))
                        .expect("source black-hole square inside 8x8 geometry");
                    next.board.cells[index].terrain.insert("blackHole".into());
                }
            }
            for row in 0..8 {
                for col in 0..8 {
                    let square = Square { row, col };
                    if crate::movement::collapsed(source, square) {
                        let at = Coord::new(i32::from(row), i32::from(col));
                        let index = geometry
                            .index(at)
                            .expect("source square inside 8x8 geometry");
                        next.board.cells[index].usable = false;
                    }
                }
            }
        }
        for (_, (piece, anchor, footprint)) in pieces {
            let spatial_piece = SpatialPiece::from_source_board(piece, anchor, footprint)?;
            next.pieces
                .0
                .insert(spatial_piece.id.clone(), spatial_piece);
        }
        next.occupancy = OccupancyIndex::rebuild(&next.board, &next.pieces)?;
        next.validate()?;
        Ok(next)
    }

    /// Preserve all nonboard v6 fields from the caller's template. This
    /// adapter is only valid for the original source rectangle.
    pub fn to_legacy(&self, template: &GameState) -> Result<GameState> {
        if self.profile != SpatialProfile::SourceV6
            || self.geometry() != BoardGeometry::new(0, 0, 8, 8)?
            || template.ruleset_id != RULES_VERSION_V6
        {
            return Err(EngineError::UnsupportedFeature(
                "legacy export requires source-v6 8x8 state and template".into(),
            ));
        }
        self.validate()?;
        let mut output = template.clone();
        output.board = vec![vec![None; 8]; 8];
        for piece in self.pieces.iter() {
            let source_piece = piece.to_legacy();
            for at in piece.occupied_cells()? {
                output.board[at.row as usize][at.col as usize] = Some(source_piece.clone());
            }
        }
        output.validate_and_identify()?;
        Ok(output)
    }
}
