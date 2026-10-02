//! 동결 v7 양자 기물의 관측·실체 재배치·조회용 그림자 경계.
//!
//! 이동 기하와 포획 반응은 각 소유 모듈이 처리한다. 원문 양자 관측은
//! 기물·보드·RNG·애니메이션·관측 로그만 바꾸며 포획, 왕실 판정, 체인 절단,
//! history 콜백을 직접 호출하지 않는다. 소비자는 관측 후 대상 셀을 재조회한다.

use crate::observation::truth;
use crate::{Color, EngineError, GameState, MoveTarget, Piece, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuantumObservation {
    NoQuantum,
    Real,
    Illusion,
}

/// main91603~91608. 관측 표시와 실제 포획 반응은 별도 상태다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QuantumMoveObservation {
    pub(crate) portal_entry: QuantumObservation,
    pub(crate) destination: QuantumObservation,
    pub(crate) own_quantum: bool,
}

impl QuantumMoveObservation {
    pub(crate) fn captured_illusion(self) -> bool {
        self.portal_entry == QuantumObservation::Illusion
            || self.destination == QuantumObservation::Illusion
    }

    #[cfg(test)]
    pub(crate) fn observed(self) -> bool {
        self.portal_entry != QuantumObservation::NoQuantum
            || self.destination != QuantumObservation::NoQuantum
    }
}

#[derive(Clone, Debug)]
pub(crate) struct QuantumPiece {
    pub(crate) piece: Piece,
    pub(crate) origin: Square,
    pub(crate) ghost: Square,
}

#[derive(Clone, Debug)]
pub(crate) struct QuantumShadowPlacement {
    #[cfg(test)]
    pub(crate) square: Square,
    #[cfg(test)]
    shadow_id: String,
    #[cfg(test)]
    source_id: String,
}

fn require_board(state: &GameState) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 quantum state for rules version {}",
            state.ruleset_id,
        )));
    }
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::InvalidState(
            "v7 quantum state requires the frozen 8x8 board".into(),
        ));
    }
    Ok(())
}

fn integer_coordinate(value: Option<&Value>) -> Option<u8> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && number.fract() == 0.0 && (0.0..8.0).contains(number))
        .map(|number| number as u8)
}

fn descriptor_square(value: Option<&Value>) -> Option<Square> {
    value.and_then(|value| {
        Some(Square {
            row: integer_coordinate(value.get("row"))?,
            col: integer_coordinate(value.get("col"))?,
        })
    })
}

pub(crate) fn quantum_anchor(piece: &Piece) -> Option<Square> {
    descriptor_square(piece.extra.get("quantum"))
}

/// main107485. A malformed or clipped large ghost has no occupied cells.
pub(crate) fn quantum_cells_for_item_at(piece: &Piece, anchor: Square) -> Vec<Square> {
    if anchor.row >= 8 || anchor.col >= 8 {
        return Vec::new();
    }
    if !piece.is_large() {
        return vec![anchor];
    }
    if anchor.row >= 7 || anchor.col >= 7 {
        return Vec::new();
    }
    vec![
        anchor,
        Square {
            row: anchor.row,
            col: anchor.col + 1,
        },
        Square {
            row: anchor.row + 1,
            col: anchor.col,
        },
        Square {
            row: anchor.row + 1,
            col: anchor.col + 1,
        },
    ]
}

fn physical_origin(piece: &Piece, fallback: Square) -> Result<Square> {
    if !piece.is_large() {
        return Ok(fallback);
    }
    let origin = integer_coordinate(piece.extra.get("anchorRow"))
        .zip(integer_coordinate(piece.extra.get("anchorCol")))
        .map(|(row, col)| Square { row, col })
        .ok_or_else(|| {
            EngineError::InvalidState(format!(
                "v7 quantum large identity {} lacks a valid physical anchor",
                piece.id
            ))
        })?;
    if quantum_cells_for_item_at(piece, origin).len() != 4 {
        return Err(EngineError::InvalidState(format!(
            "v7 quantum large identity {} has a clipped physical footprint",
            piece.id
        )));
    }
    Ok(origin)
}

fn require_identity(piece: &Piece) -> Result<()> {
    if piece.id.is_empty() {
        return Err(EngineError::InvalidState(
            "v7 quantum state requires source piece identity".into(),
        ));
    }
    Ok(())
}

/// main107542. Source row-major order and identity de-duplication decide which
/// shadow wins if more than one descriptor refers to the same empty square.
pub(crate) fn find_quantum_at(state: &GameState, at: Square) -> Result<Option<QuantumPiece>> {
    require_board(state)?;
    let mut seen = BTreeSet::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, occupant) in cells.iter().enumerate() {
            let Some(piece) = occupant
                .as_ref()
                .filter(|piece| truth(piece.extra.get("quantum")))
            else {
                continue;
            };
            require_identity(piece)?;
            if !seen.insert(piece.id.clone()) {
                continue;
            }
            let Some(ghost) = quantum_anchor(piece) else {
                continue;
            };
            if !quantum_cells_for_item_at(piece, ghost).contains(&at) {
                continue;
            }
            return Ok(Some(QuantumPiece {
                piece: piece.clone(),
                origin: physical_origin(
                    piece,
                    Square {
                        row: row as u8,
                        col: col as u8,
                    },
                )?,
                ghost,
            }));
        }
    }
    Ok(None)
}

/// main107522~107540. 실제 점유자가 없는 클릭에서만 ghost 선택을 만든다.
/// 큰 기물의 ghost body 셀은 같은 physical origin/ghost anchor로 정규화한다.
/// 공개 actor·가시성·선택 창 허용 여부는 호출자의 UI 경계가 판정한다.
pub(crate) fn selection_from_ghost_click(
    state: &GameState,
    at: Square,
) -> Result<Option<QuantumPiece>> {
    require_board(state)?;
    if at.row >= 8 || at.col >= 8 {
        return Err(EngineError::IllegalAction);
    }
    if state.at(at).is_some() {
        return Ok(None);
    }
    find_quantum_at(state, at)
}

fn scarecrow_reserved(state: &GameState, at: Square) -> bool {
    state
        .extra
        .get("pendingScarecrows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|entry| !truth(entry.get("pieceId")) && descriptor_square(Some(entry)) == Some(at))
        || state
            .extra
            .get("pendingLobsters")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|entry| descriptor_square(Some(entry)) == Some(at))
}

/// main107499. This is the source's narrow quantum availability predicate;
/// d4, portal reservations, crown ground and chain reach are independent gates.
pub(crate) fn quantum_destination_available(
    state: &GameState,
    piece: &Piece,
    anchor: Square,
    allow_item_occupancy: bool,
) -> Result<bool> {
    require_board(state)?;
    require_identity(piece)?;
    let cells = quantum_cells_for_item_at(piece, anchor);
    if cells.is_empty() {
        return Ok(false);
    }
    for at in cells {
        let occupant = state.at(at);
        let same = occupant.is_some_and(|other| other.id == piece.id);
        if (scarecrow_reserved(state, at) || occupant.is_some()) && (!allow_item_occupancy || !same)
        {
            return Ok(false);
        }
        if find_quantum_at(state, at)?.is_some_and(|quantum| quantum.piece.id != piece.id) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn write_identity(state: &mut GameState, piece: &Piece) {
    for occupant in state.board.iter_mut().flatten().flatten() {
        if occupant.id == piece.id {
            *occupant = piece.clone();
        }
    }
}

fn remove_piece_property(piece: &mut Piece, field: &str) {
    piece.extra.shift_remove(field);
    piece.source_order.retain(|key| key != field);
}

/// main107511. Repositioning an observed identity does not constitute a move
/// or capture. Large aliases share the updated anchor and monochrome shade.
pub(crate) fn place_quantum_resolved_item(
    state: &mut GameState,
    piece: &mut Piece,
    from: Square,
    to: Square,
) -> Result<()> {
    require_board(state)?;
    require_identity(piece)?;
    let cells = quantum_cells_for_item_at(piece, to);
    if cells.is_empty() {
        return Err(EngineError::InvalidState(
            "v7 quantum resolution destination has no full footprint".into(),
        ));
    }
    if piece.is_large() {
        crate::transition::clear_piece(state, &piece.id);
        piece.extra.insert("anchorRow".into(), json!(to.row));
        piece.extra.insert("anchorCol".into(), json!(to.col));
        if truth(state.extra.get("monochromeChess")) && !truth(piece.extra.get("monoShade")) {
            piece.extra.insert(
                "monoShade".into(),
                json!(if (to.row + to.col).is_multiple_of(2) {
                    "light"
                } else {
                    "dark"
                }),
            );
        }
        for at in cells {
            state.board[usize::from(at.row)][usize::from(at.col)] = Some(piece.clone());
        }
    } else {
        if from.row < 8 && from.col < 8 && state.at(from).is_some_and(|item| item.id == piece.id) {
            state.board[usize::from(from.row)][usize::from(from.col)] = None;
        }
        state.board[usize::from(to.row)][usize::from(to.col)] = Some(piece.clone());
    }
    Ok(())
}

fn show_observation(state: &mut GameState, at: Square) -> Result<()> {
    // The source's real/illusion DOM toast is identical and has no DTO state.
    if !state.extra.get("logs").is_some_and(Value::is_array) {
        state.extra.insert("logs".into(), json!([]));
    }
    crate::replay::add_log(
        state,
        format!(
            "양자 역학: {}{}에서 관측되었습니다!",
            char::from(b'a' + at.col),
            8 - at.row
        ),
    )
}

fn sample_illusion(state: &mut GameState) -> Result<bool> {
    let roll = state.rng.sample()?;
    if !roll.is_finite() || !(0.0..1.0).contains(&roll) {
        return Err(EngineError::InvalidState(
            "v7 quantum RNG draw must be finite and in [0,1)".into(),
        ));
    }
    let illusion = roll < 0.7;
    state.rng.record_last_probability(
        if illusion { 0.7 } else { 0.3 },
        "source Quantum observation branch",
    )?;
    if let Some(probability) = &mut state.semantic_chance_probability {
        *probability *= if illusion { 0.7 } else { 0.3 };
        if !probability.is_finite() || *probability <= 0.0 {
            return Err(EngineError::InvalidState(
                "v7 quantum semantic chance density is not finite and positive".into(),
            ));
        }
    }
    Ok(illusion)
}

fn resolve_enemy_capture(
    state: &mut GameState,
    mut piece: Piece,
    fallback: Square,
    observed: Square,
    forced: bool,
) -> Result<QuantumObservation> {
    let origin = physical_origin(&piece, fallback)?;
    let ghost = quantum_anchor(&piece).ok_or_else(|| {
        EngineError::InvalidState(format!(
            "v7 quantum identity {} lacks an integer ghost coordinate",
            piece.id
        ))
    })?;
    let observed_actual = quantum_cells_for_item_at(&piece, origin).contains(&observed);
    let observed_ghost = quantum_cells_for_item_at(&piece, ghost).contains(&observed);
    let illusion = forced || sample_illusion(state)?;
    remove_piece_property(&mut piece, "quantum");
    write_identity(state, &piece);
    if illusion {
        let survivor = if observed_actual { ghost } else { origin };
        if !quantum_destination_available(state, &piece, survivor, true)? {
            show_observation(state, observed)?;
            return Ok(QuantumObservation::Real);
        }
        if survivor != origin {
            place_quantum_resolved_item(state, &mut piece, origin, survivor)?;
        }
        crate::card_effects::mark_animation(state, &piece)?;
        show_observation(state, observed)?;
        return Ok(QuantumObservation::Illusion);
    }
    if !observed_actual {
        let destination = if observed_ghost { ghost } else { observed };
        if !quantum_destination_available(state, &piece, destination, true)? {
            show_observation(state, observed)?;
            return Ok(QuantumObservation::Illusion);
        }
        place_quantum_resolved_item(state, &mut piece, origin, destination)?;
        crate::card_effects::mark_animation(state, &piece)?;
    }
    show_observation(state, observed)?;
    Ok(QuantumObservation::Real)
}

/// main107630. Already owned transaction/probe form. Public state boundaries
/// should use the atomic wrapper below or discard their owned state on error.
pub(crate) fn observe_quantum_at_in_place(
    state: &mut GameState,
    at: Square,
    observer: Option<Color>,
) -> Result<QuantumObservation> {
    require_board(state)?;
    if at.row >= 8 || at.col >= 8 {
        return Err(EngineError::IllegalAction);
    }
    if let Some(mut real) = state
        .at(at)
        .cloned()
        .filter(|piece| truth(piece.extra.get("quantum")))
    {
        require_identity(&real)?;
        let forced = truth(real.extra.get("quantumFirstObservationFails"));
        if forced {
            remove_piece_property(&mut real, "quantumFirstObservationFails");
        }
        write_identity(state, &real);
        let origin = physical_origin(&real, at)?;
        if forced || observer.is_some_and(|color| real.color != color) {
            return resolve_enemy_capture(state, real, origin, at, forced);
        }
        remove_piece_property(&mut real, "quantum");
        write_identity(state, &real);
        show_observation(state, at)?;
        return Ok(QuantumObservation::Real);
    }
    let Some(ghost) = find_quantum_at(state, at)? else {
        return Ok(QuantumObservation::NoQuantum);
    };
    let mut piece = ghost.piece;
    let forced = truth(piece.extra.get("quantumFirstObservationFails"));
    if forced {
        remove_piece_property(&mut piece, "quantumFirstObservationFails");
    }
    write_identity(state, &piece);
    if forced || observer.is_some_and(|color| piece.color != color) {
        return resolve_enemy_capture(state, piece, ghost.origin, at, forced);
    }
    remove_piece_property(&mut piece, "quantum");
    write_identity(state, &piece);
    show_observation(state, at)?;
    Ok(QuantumObservation::Illusion)
}

pub(crate) fn observe_quantum_at(
    state: &mut GameState,
    at: Square,
    observer: Color,
) -> Result<QuantumObservation> {
    let mut next = state.clone();
    let observation = observe_quantum_at_in_place(&mut next, at, Some(observer))?;
    *state = next;
    Ok(observation)
}

/// main91605/92309. 삭제만 수행하며 강제 첫 관측 실패 표시를 소비하지 않는다.
pub(crate) fn clear_quantum_for_piece_in_place(
    state: &mut GameState,
    piece: &mut Piece,
) -> Result<()> {
    require_board(state)?;
    require_identity(piece)?;
    remove_piece_property(piece, "quantum");
    write_identity(state, piece);
    Ok(())
}

fn refresh_moving_identity(state: &GameState, piece: &mut Piece) -> Result<()> {
    let current = state
        .board
        .iter()
        .flatten()
        .flatten()
        .find(|current| current.id == piece.id)
        .ok_or_else(|| {
            EngineError::InvalidState(format!(
                "v7 quantum move observation lost moving identity {}",
                piece.id,
            ))
        })?;
    *piece = current.clone();
    Ok(())
}

/// main91603~91608. portalLanding 호출자는 연결 검증 후 출구를 destination으로,
/// 입구를 portal_entry로 전달한다. portalThrough는 입구 관측을 추가하지 않는다.
/// 목적지는 클릭한 정확한 셀이며 관측 전에 물리 anchor로 정규화하지 않는다.
/// 이미 소유한 transaction/probe용으로, 반환 후 친군·방어·HP 가드를 재검사한다.
pub(crate) fn observe_move_landing_in_place(
    state: &mut GameState,
    piece: &mut Piece,
    destination: Square,
    portal_entry: Option<Square>,
) -> Result<QuantumMoveObservation> {
    require_board(state)?;
    require_identity(piece)?;
    let observer = piece.color.owner().ok_or(EngineError::WrongActor)?;
    if destination.row >= 8
        || destination.col >= 8
        || portal_entry.is_some_and(|entry| entry.row >= 8 || entry.col >= 8)
    {
        return Err(EngineError::IllegalAction);
    }
    let portal_observation = if let Some(entry) = portal_entry {
        observe_quantum_at_in_place(state, entry, Some(observer))?
    } else {
        QuantumObservation::NoQuantum
    };
    // Source keeps the moving object by reference. Sync its Rust value clone
    // after an entry observation before testing or writing the counterpart.
    refresh_moving_identity(state, piece)?;
    let own_quantum =
        find_quantum_at(state, destination)?.is_some_and(|ghost| ghost.piece.id == piece.id);
    let destination_observation = if own_quantum {
        clear_quantum_for_piece_in_place(state, piece)?;
        QuantumObservation::NoQuantum
    } else {
        observe_quantum_at_in_place(state, destination, Some(observer))?
    };
    refresh_moving_identity(state, piece)?;
    Ok(QuantumMoveObservation {
        portal_entry: portal_observation,
        destination: destination_observation,
        own_quantum,
    })
}

/// main107554. Only disposable move-generation clones should retain these
/// shadows. Ordinary identities are copied verbatim with the source tags.
pub(crate) fn materialize_quantum_shadows(
    state: &mut GameState,
) -> Result<Vec<QuantumShadowPlacement>> {
    require_board(state)?;
    let mut seen = BTreeSet::new();
    let candidates = state
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|piece| {
            !piece.id.is_empty()
                && truth(piece.extra.get("quantum"))
                && seen.insert(piece.id.clone())
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut inserted = Vec::new();
    for mut piece in candidates {
        let Some(ghost) = quantum_anchor(&piece) else {
            continue;
        };
        if !quantum_destination_available(state, &piece, ghost, false)? {
            continue;
        }
        let source_id = piece.id.clone();
        piece.id = format!("{}:quantum", source_id);
        piece.extra.insert("anchorRow".into(), json!(ghost.row));
        piece.extra.insert("anchorCol".into(), json!(ghost.col));
        piece.extra.insert("isQuantumShadow".into(), json!(true));
        piece
            .extra
            .insert("quantumSourceId".into(), json!(source_id));
        piece.extra.insert("quantum".into(), Value::Null);
        for square in quantum_cells_for_item_at(&piece, ghost) {
            state.board[usize::from(square.row)][usize::from(square.col)] = Some(piece.clone());
            inserted.push(QuantumShadowPlacement {
                #[cfg(test)]
                square,
                #[cfg(test)]
                shadow_id: piece.id.clone(),
                #[cfg(test)]
                source_id: source_id.clone(),
            });
        }
    }
    Ok(inserted)
}

#[cfg(test)]
pub(crate) fn cleanup_quantum_shadows(
    state: &mut GameState,
    inserted: &[QuantumShadowPlacement],
) -> Result<()> {
    require_board(state)?;
    for entry in inserted {
        if state.at(entry.square).is_some_and(|piece| {
            piece.id == entry.shadow_id
                && piece.extra.get("quantumSourceId").and_then(Value::as_str)
                    == Some(entry.source_id.as_str())
                && truth(piece.extra.get("isQuantumShadow"))
        }) {
            state.board[usize::from(entry.square.row)][usize::from(entry.square.col)] = None;
        }
    }
    Ok(())
}

fn move_landing_cells(target: &MoveTarget) -> Result<Vec<Square>> {
    if [
        "colossusBody",
        "colossusAttack",
        "shotgunBlast",
        "shotgunSnipe",
        "setLogDirection",
    ]
    .iter()
    .any(|field| target.flag(field))
    {
        return Ok(Vec::new());
    }
    if target.flag("castle") {
        let mut cells = vec![target.square()];
        if let Some(rook) = descriptor_square(target.flags.get("rookTo"))
            && !cells.contains(&rook)
        {
            cells.push(rook);
        }
        return Ok(cells
            .into_iter()
            .filter(|at| at.row < 8 && at.col < 8)
            .collect());
    }
    if (target.flag("colossusMove") || target.flag("bigRookMove"))
        && let Some(cells) = target.flags.get("highlightCells").and_then(Value::as_array)
    {
        return cells
            .iter()
            .map(|cell| {
                descriptor_square(Some(cell)).ok_or_else(|| {
                    EngineError::InvalidState(
                        "v7 quantum counterpart landing highlight must be on the board".into(),
                    )
                })
            })
            .collect();
    }
    if target.flag("portalLanding")
        && let Some(exit) = descriptor_square(target.flags.get("portalExit"))
    {
        return Ok(vec![exit]);
    }
    Ok(if target.row < 8 && target.col < 8 {
        vec![target.square()]
    } else {
        Vec::new()
    })
}

/// main96005. None selects the source default piece.quantum. Geometry owners
/// keep their move ordering and apply this predicate only at finalization.
pub(crate) fn move_lands_on_quantum_counterpart(
    piece: &Piece,
    target: &MoveTarget,
    counterpart: Option<&Value>,
) -> Result<bool> {
    let Some(anchor) = descriptor_square(counterpart.or_else(|| piece.extra.get("quantum"))) else {
        return Ok(false);
    };
    let counterpart_cells = quantum_cells_for_item_at(piece, anchor);
    if counterpart_cells.is_empty() {
        return Ok(false);
    }
    Ok(move_landing_cells(target)?
        .iter()
        .any(|cell| counterpart_cells.contains(cell)))
}

/// main90869~90897. 조회용 clone에 기물을 ghost로 재배치하되 quantum을
/// 삭제하거나 관측하지 않는다. 원래 physical footprint에 착지하는 후보만
/// 공통 geometry 결과에서 제외한 뒤 내부 quantumFrom descriptor를 붙인다.
/// source AI 후보나 frozen Observation.legalHints는 이 별도 UI 경로를 열거하지 않는다.
pub(crate) fn legal_moves_for_quantum_selection(
    state: &GameState,
    physical_from: Square,
    quantum_from: Square,
) -> Result<Vec<MoveTarget>> {
    require_board(state)?;
    if physical_from.row >= 8
        || physical_from.col >= 8
        || quantum_from.row >= 8
        || quantum_from.col >= 8
    {
        return Err(EngineError::IllegalAction);
    }
    let Some(mut piece) = state.at(physical_from).cloned() else {
        return Ok(Vec::new());
    };
    require_identity(&piece)?;
    let physical_from = physical_origin(&piece, physical_from)?;
    if quantum_anchor(&piece) != Some(quantum_from)
        || !quantum_destination_available(state, &piece, quantum_from, false)?
    {
        return Ok(Vec::new());
    }
    let mut probe = state.clone();
    place_quantum_resolved_item(&mut probe, &mut piece, physical_from, quantum_from)?;
    let targets = crate::movement::v7_legal_move_targets(
        &probe,
        &piece,
        quantum_from,
        crate::movement::V7MoveOptions {
            ignore_quantum_counterpart: true,
            ..Default::default()
        },
    )?;
    let counterpart = json!(physical_from);
    let descriptor = piece.extra.get("quantum").cloned().ok_or_else(|| {
        EngineError::InvalidState(
            "v7 quantum selection lost its source descriptor during query relocation".into(),
        )
    })?;
    let mut allowed = Vec::new();
    for mut target in targets {
        if move_lands_on_quantum_counterpart(&piece, &target, Some(&counterpart))? {
            continue;
        }
        target
            .flags
            .insert("quantumFrom".into(), descriptor.clone());
        allowed.push(target);
    }
    Ok(allowed)
}

/// main90808. The movement owner supplies the source-ordered legal moves from
/// the pre-move board; this function only selects possible quantum endpoints.
pub(crate) fn candidate_moves_for_move(
    state: &GameState,
    piece: &Piece,
    destination: Square,
    legal_moves: &[MoveTarget],
) -> Result<Vec<MoveTarget>> {
    require_board(state)?;
    if !truth(
        state
            .extra
            .get("quantumPending")
            .and_then(|pending| pending.get(piece.color.as_str())),
    ) {
        return Ok(Vec::new());
    }
    let mut candidates = Vec::new();
    for target in legal_moves {
        let Some(anchor) = candidate_anchor(target) else {
            continue;
        };
        if anchor == destination {
            continue;
        }
        if piece.is_large() {
            if !(target.flag("colossusMove") || target.flag("bigRookMove")) {
                continue;
            }
            if quantum_destination_available(state, piece, anchor, false)? {
                candidates.push(target.clone());
            }
            continue;
        }
        if [
            "castle",
            "colossusAttack",
            "colossusMove",
            "bigRookMove",
            "shotgunBlast",
            "shotgunSnipe",
            "merchantBuy",
            "setLogDirection",
            "dragonSwap",
            "quantumFrom",
        ]
        .iter()
        .any(|field| truth(target.flags.get(*field)))
        {
            continue;
        }
        if state.at(target.square()).is_none() && find_quantum_at(state, target.square())?.is_none()
        {
            candidates.push(target.clone());
        }
    }
    Ok(candidates)
}

fn candidate_immediately_attacked(state: &GameState, piece: &Piece, at: Square) -> Result<bool> {
    let owner = piece.color.owner().ok_or(EngineError::WrongActor)?;
    if !quantum_destination_available(state, piece, at, false)? {
        return Ok(true);
    }
    let mut probe = state.clone();
    let mut shadow = piece.clone();
    shadow.extra.insert("anchorRow".into(), json!(at.row));
    shadow.extra.insert("anchorCol".into(), json!(at.col));
    shadow.extra.insert("quantum".into(), Value::Null);
    shadow.extra.insert("isQuantumShadow".into(), json!(true));
    shadow
        .extra
        .insert("quantumSourceId".into(), json!(piece.id));
    for cell in quantum_cells_for_item_at(piece, at) {
        probe.board[usize::from(cell.row)][usize::from(cell.col)] = Some(shadow.clone());
    }
    for cell in quantum_cells_for_item_at(piece, at) {
        if crate::v7_threat::is_square_attacked_v7(&probe, cell, owner.opponent(), None, None)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn candidate_anchor(target: &MoveTarget) -> Option<Square> {
    let coordinate = |value: Option<&Value>, fallback: u8| {
        if let Some(number) = value
            .and_then(Value::as_f64)
            .filter(|number| number.is_finite() && number.fract() == 0.0)
        {
            (0.0..8.0).contains(&number).then_some(number as u8)
        } else {
            (fallback < 8).then_some(fallback)
        }
    };
    Some(Square {
        row: coordinate(target.flags.get("anchorRow"), target.row)?,
        col: coordinate(target.flags.get("anchorCol"), target.col)?,
    })
}

/// main90848. Source chooses safe ghost destinations when any exist and then
/// samples one ordered candidate. Capture lock and first-observation failure
/// are set even when no counterpart can be installed.
#[cfg(test)]
pub(crate) fn apply_after_move(
    state: &mut GameState,
    piece: &mut Piece,
    destination: Square,
    candidates: &[MoveTarget],
) -> Result<Option<Square>> {
    let mut next = state.clone();
    let mut active = piece.clone();
    let result = apply_after_move_in_place(&mut next, &mut active, destination, candidates)?;
    *state = next;
    *piece = active;
    Ok(result)
}

pub(crate) fn apply_after_move_in_place(
    state: &mut GameState,
    piece: &mut Piece,
    destination: Square,
    candidates: &[MoveTarget],
) -> Result<Option<Square>> {
    require_board(state)?;
    let owner = piece.color.owner().ok_or(EngineError::WrongActor)?;
    if !truth(
        state
            .extra
            .get("quantumPending")
            .and_then(|pending| pending.get(owner.as_str())),
    ) {
        return Ok(None);
    }
    state.set_flag("quantumPending", owner, false);
    let until = state.turns_taken.get(owner).checked_add(1).ok_or_else(|| {
        EngineError::InvalidState(
            "v7 quantum capture-lock turn exceeds the typed counter range".into(),
        )
    })?;
    piece
        .extra
        .insert("quantumNoCaptureUntil".into(), json!(until));
    piece
        .extra
        .insert("quantumFirstObservationFails".into(), json!(true));
    write_identity(state, piece);
    let mut available = Vec::new();
    for target in candidates {
        let Some(anchor) = candidate_anchor(target) else {
            continue;
        };
        if anchor != destination && quantum_destination_available(state, piece, anchor, false)? {
            available.push(anchor);
        }
    }
    if available.is_empty() {
        return Ok(None);
    }
    let mut safe = Vec::new();
    for anchor in &available {
        if !candidate_immediately_attacked(state, piece, *anchor)? {
            safe.push(*anchor);
        }
    }
    let pool = if safe.is_empty() { available } else { safe };
    let chosen = pool[crate::transition::sample_choice(state, pool.len())?];
    piece.extra.insert("quantum".into(), json!(chosen));
    write_identity(state, piece);
    crate::card_effects::mark_animation(state, piece)?;
    Ok(Some(chosen))
}

/// Prefix of source moveQuantumPiece. The common transition kernel performs
/// the actual move only after this verified materialization succeeds.
pub(crate) fn prepare_quantum_move(
    state: &mut GameState,
    physical_from: Square,
    quantum_from: Square,
    expected_id: &str,
) -> Result<()> {
    require_board(state)?;
    let mut piece = state
        .at(physical_from)
        .cloned()
        .filter(|piece| piece.id == expected_id)
        .ok_or(EngineError::IllegalAction)?;
    if quantum_anchor(&piece) != Some(quantum_from)
        || !quantum_destination_available(state, &piece, quantum_from, false)?
    {
        return Err(EngineError::IllegalAction);
    }
    place_quantum_resolved_item(state, &mut piece, physical_from, quantum_from)?;
    remove_piece_property(&mut piece, "quantum");
    write_identity(state, &piece);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameConfig, RngState};

    fn state_with_quantum(large: bool) -> (GameState, Square, Square) {
        let mut state =
            crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7)
                .unwrap();
        state.mode = "play".into();
        state.board = vec![vec![None; 8]; 8];
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert(
            "forceAnimatedPieceIds".into(),
            json!({"__simType":"Set","values":[]}),
        );
        state.rng = RngState::seeded(7);
        let from = Square { row: 5, col: 5 };
        let ghost = Square { row: 2, col: 2 };
        let mut piece = Piece::new(
            if large { "bigRook" } else { "rook" },
            Color::White,
            "quantum-white",
        );
        piece.extra.insert("quantum".into(), json!(ghost));
        if large {
            piece.extra.insert("anchorRow".into(), json!(from.row));
            piece.extra.insert("anchorCol".into(), json!(from.col));
        }
        for cell in quantum_cells_for_item_at(&piece, from) {
            state.board[usize::from(cell.row)][usize::from(cell.col)] = Some(piece.clone());
        }
        (state, from, ghost)
    }

    fn force_first_failure(state: &mut GameState, from: Square) {
        let mut piece = state.at(from).unwrap().clone();
        piece
            .extra
            .insert("quantumFirstObservationFails".into(), json!(true));
        write_identity(state, &piece);
    }

    #[test]
    fn forced_actual_observation_relocates_same_identity_without_rng_or_capture_callbacks() {
        let (mut state, from, ghost) = state_with_quantum(false);
        force_first_failure(&mut state, from);
        state.extra.insert(
            "chainBonds".into(),
            json!([{"aId":"quantum-white","bId":"partner","by":"black"}]),
        );
        let before_rng = state.rng.clone();
        let before_captures = state.captures.clone();
        let before_history = state.history.clone();
        let bonds = state.extra["chainBonds"].clone();
        state.rng.begin_source_trace().unwrap();
        assert_eq!(
            observe_quantum_at(&mut state, from, Color::Black).unwrap(),
            QuantumObservation::Illusion
        );
        assert_eq!(state.rng.finish_source_trace().unwrap(), 1.0);
        assert!(state.at(from).is_none());
        let survivor = state.at(ghost).unwrap();
        assert_eq!(survivor.id, "quantum-white");
        assert!(!survivor.extra.contains_key("quantum"));
        assert!(!survivor.extra.contains_key("quantumFirstObservationFails"));
        assert_eq!(state.rng, before_rng);
        assert_eq!(state.captures, before_captures);
        assert_eq!(state.history, before_history);
        assert_eq!(state.extra["chainBonds"], bonds);
        assert_eq!(
            state.extra["forceAnimatedPieceIds"]["values"],
            json!(["quantum-white"])
        );
        assert_eq!(
            state.extra["logs"],
            json!(["양자 역학: f3에서 관측되었습니다!"])
        );
    }

    #[test]
    fn forced_ghost_observation_preserves_actual_cell_and_consumes_no_rng() {
        let (mut state, from, ghost) = state_with_quantum(false);
        force_first_failure(&mut state, from);
        let before_rng = state.rng.clone();
        assert_eq!(
            observe_quantum_at(&mut state, ghost, Color::Black).unwrap(),
            QuantumObservation::Illusion
        );
        assert_eq!(state.at(from).unwrap().id, "quantum-white");
        assert!(state.at(ghost).is_none());
        assert!(state.at(from).unwrap().extra.get("quantum").is_none());
        assert_eq!(state.rng, before_rng);
    }

    #[test]
    fn enemy_observation_uses_strict_point_seven_boundary_and_one_draw() {
        let (mut real, from, ghost) = state_with_quantum(false);
        real.rng.tape = vec![0.7];
        real.semantic_chance_probability = Some(1.0);
        real.rng.begin_source_trace().unwrap();
        assert_eq!(
            observe_quantum_at(&mut real, from, Color::Black).unwrap(),
            QuantumObservation::Real
        );
        assert!(real.at(from).is_some());
        assert!(real.at(ghost).is_none());
        assert_eq!(real.rng.cursor, 1);
        assert_eq!(real.semantic_chance_probability, Some(0.3));
        assert_eq!(real.rng.finish_source_trace().unwrap(), 0.3);
        let (mut illusion, from, ghost) = state_with_quantum(false);
        illusion.rng.tape = vec![0.699999999999];
        illusion.semantic_chance_probability = Some(1.0);
        illusion.rng.begin_source_trace().unwrap();
        assert_eq!(
            observe_quantum_at(&mut illusion, from, Color::Black).unwrap(),
            QuantumObservation::Illusion
        );
        assert!(illusion.at(from).is_none());
        assert!(illusion.at(ghost).is_some());
        assert_eq!(illusion.rng.cursor, 1);
        assert_eq!(illusion.semantic_chance_probability, Some(0.7));
        assert_eq!(illusion.rng.finish_source_trace().unwrap(), 0.7);
    }

    #[test]
    fn own_ghost_collapses_without_random_draw_or_animation_write() {
        let (mut state, from, ghost) = state_with_quantum(false);
        let before_rng = state.rng.clone();
        assert_eq!(
            observe_quantum_at(&mut state, ghost, Color::White).unwrap(),
            QuantumObservation::Illusion
        );
        assert!(state.at(from).is_some());
        assert!(state.at(ghost).is_none());
        assert_eq!(state.extra["forceAnimatedPieceIds"]["values"], json!([]));
        assert_eq!(state.rng, before_rng);
    }

    #[test]
    fn landing_on_own_counterpart_deletes_only_quantum_without_observation() {
        let (mut state, from, ghost) = state_with_quantum(false);
        force_first_failure(&mut state, from);
        let mut moving = state.at(from).unwrap().clone();
        let before_rng = state.rng.clone();
        let outcome = observe_move_landing_in_place(&mut state, &mut moving, ghost, None).unwrap();
        assert!(outcome.own_quantum);
        assert!(!outcome.observed());
        assert!(!outcome.captured_illusion());
        assert!(!moving.extra.contains_key("quantum"));
        assert!(moving.flag("quantumFirstObservationFails"));
        assert_eq!(state.at(from), Some(&moving));
        assert!(state.at(ghost).is_none());
        assert_eq!(state.extra["logs"], json!([]));
        assert_eq!(state.extra["forceAnimatedPieceIds"]["values"], json!([]));
        assert_eq!(state.rng, before_rng);
    }

    #[test]
    fn portal_entry_observation_precedes_destination_reread_and_preserves_capture_flags() {
        let (mut state, entry, destination) = state_with_quantum(false);
        force_first_failure(&mut state, entry);
        let origin = Square { row: 7, col: 0 };
        let mut moving = Piece::new("rook", Color::Black, "observer-black");
        state.board[usize::from(origin.row)][usize::from(origin.col)] = Some(moving.clone());
        let before_captures = state.captures.clone();
        let before_rng = state.rng.clone();
        let outcome =
            observe_move_landing_in_place(&mut state, &mut moving, destination, Some(entry))
                .unwrap();
        assert_eq!(outcome.portal_entry, QuantumObservation::Illusion);
        assert_eq!(outcome.destination, QuantumObservation::NoQuantum);
        assert!(!outcome.own_quantum);
        assert!(outcome.observed());
        assert!(outcome.captured_illusion());
        assert!(state.at(entry).is_none());
        assert_eq!(state.at(destination).unwrap().id, "quantum-white");
        assert_eq!(state.at(origin), Some(&moving));
        assert_eq!(state.captures, before_captures);
        assert_eq!(state.rng, before_rng);
        assert_eq!(
            state.extra["logs"],
            json!(["양자 역학: f3에서 관측되었습니다!"])
        );
    }

    #[test]
    fn blocked_forced_survivor_returns_real_and_removes_superposition() {
        let (mut state, from, ghost) = state_with_quantum(false);
        force_first_failure(&mut state, from);
        state.extra.insert("pendingLobsters".into(), json!([ghost]));
        assert_eq!(
            observe_quantum_at(&mut state, from, Color::Black).unwrap(),
            QuantumObservation::Real
        );
        assert!(state.at(from).is_some());
        assert!(state.at(ghost).is_none());
        assert!(state.at(from).unwrap().extra.get("quantum").is_none());
        assert_eq!(state.rng.cursor, 0);
        assert_eq!(state.extra["forceAnimatedPieceIds"]["values"], json!([]));
    }

    #[test]
    fn large_resolution_updates_all_aliases_anchor_and_monochrome_shade() {
        let (mut state, from, ghost) = state_with_quantum(true);
        force_first_failure(&mut state, from);
        state.extra.insert("monochromeChess".into(), json!(true));
        assert_eq!(
            observe_quantum_at(&mut state, Square { row: 6, col: 6 }, Color::Black).unwrap(),
            QuantumObservation::Illusion
        );
        for cell in [
            from,
            Square { row: 5, col: 6 },
            Square { row: 6, col: 5 },
            Square { row: 6, col: 6 },
        ] {
            assert!(state.at(cell).is_none());
        }
        let survivor = state.at(ghost).unwrap();
        assert_eq!(survivor.extra["anchorRow"], json!(2));
        assert_eq!(survivor.extra["anchorCol"], json!(2));
        assert_eq!(survivor.extra["monoShade"], json!("light"));
        for cell in quantum_cells_for_item_at(survivor, ghost) {
            assert_eq!(state.at(cell).unwrap(), survivor);
        }
    }

    #[test]
    fn materialized_shadows_are_query_only_and_cleanup_restores_every_field() {
        let (mut state, from, ghost) = state_with_quantum(false);
        let before = state.clone();
        let inserted = materialize_quantum_shadows(&mut state).unwrap();
        assert_eq!(inserted.len(), 1);
        let shadow = state.at(ghost).unwrap();
        assert_eq!(shadow.id, "quantum-white:quantum");
        assert_eq!(shadow.extra["quantumSourceId"], json!("quantum-white"));
        assert_eq!(shadow.extra["quantum"], Value::Null);
        assert!(shadow.flag("isQuantumShadow"));
        assert_eq!(state.at(from).unwrap(), before.at(from).unwrap());
        cleanup_quantum_shadows(&mut state, &inserted).unwrap();
        assert_eq!(state, before);
    }

    #[test]
    fn quantum_selection_query_keeps_state_and_excludes_physical_counterpart_landing() {
        let (mut state, from, _) = state_with_quantum(false);
        let ghost = Square {
            row: from.row,
            col: 2,
        };
        state.board[usize::from(from.row)][usize::from(from.col)]
            .as_mut()
            .unwrap()
            .extra
            .insert("quantum".into(), json!(ghost));
        let before = state.clone();
        let selection = selection_from_ghost_click(&state, ghost).unwrap().unwrap();
        assert_eq!(selection.origin, from);
        assert_eq!(selection.ghost, ghost);
        let targets = legal_moves_for_quantum_selection(&state, from, ghost).unwrap();
        assert!(!targets.is_empty());
        assert!(targets.iter().any(|target| target.square()
            == Square {
                row: from.row,
                col: 1
            }));
        assert!(targets.iter().all(|target| target.square() != from));
        assert!(
            targets
                .iter()
                .all(|target| target.flags.get("quantumFrom") == Some(&json!(ghost)))
        );
        assert_eq!(
            state, before,
            "ghost 조회가 board/RNG/identity/history를 변경했습니다"
        );
    }

    #[test]
    fn large_quantum_selection_normalizes_ghost_body_and_physical_occupants_take_priority() {
        let (mut state, from, ghost) = state_with_quantum(true);
        let body = Square {
            row: ghost.row + 1,
            col: ghost.col + 1,
        };
        let selection = selection_from_ghost_click(&state, body).unwrap().unwrap();
        assert_eq!(selection.origin, from);
        assert_eq!(selection.ghost, ghost);
        assert_eq!(selection.piece.id, "quantum-white");
        assert!(selection_from_ghost_click(&state, from).unwrap().is_none());
        state.board[usize::from(body.row)][usize::from(body.col)] =
            Some(Piece::new("pawn", Color::Black, "real-blocker"));
        assert!(selection_from_ghost_click(&state, body).unwrap().is_none());
        let before = state.clone();
        assert!(
            legal_moves_for_quantum_selection(&state, from, ghost)
                .unwrap()
                .is_empty()
        );
        assert_eq!(state, before);
    }

    #[test]
    fn observation_failure_rolls_back_all_aliases_rng_and_flags() {
        let (mut state, from, _) = state_with_quantum(false);
        force_first_failure(&mut state, from);
        state
            .extra
            .insert("forceAnimatedPieceIds".into(), json!([]));
        let before = state.clone();
        assert!(matches!(
            observe_quantum_at(&mut state, from, Color::Black),
            Err(EngineError::InvalidState(_))
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn no_counterpart_after_move_still_sets_capture_lock_and_forced_first_observation() {
        let (mut state, from, _) = state_with_quantum(false);
        let mut piece = state.at(from).unwrap().clone();
        piece.extra.shift_remove("quantum");
        write_identity(&mut state, &piece);
        state.set_flag("quantumPending", Color::White, true);
        state.turns_taken.white = 4;
        let before_rng = state.rng.clone();
        assert_eq!(
            apply_after_move(&mut state, &mut piece, from, &[]).unwrap(),
            None
        );
        assert!(!state.flag("quantumPending", Color::White));
        assert_eq!(piece.extra["quantumNoCaptureUntil"], json!(5));
        assert_eq!(piece.extra["quantumFirstObservationFails"], json!(true));
        assert!(!piece.extra.contains_key("quantum"));
        assert_eq!(state.rng, before_rng);
    }

    #[test]
    fn counterpart_filter_uses_real_landing_cells_and_skips_stationary_attacks() {
        let (state, from, ghost) = state_with_quantum(false);
        let piece = state.at(from).unwrap();
        assert!(move_lands_on_quantum_counterpart(piece, &MoveTarget::at(ghost), None).unwrap());
        let mut shot = MoveTarget::at(ghost);
        shot.flags.insert("shotgunBlast".into(), json!(true));
        assert!(!move_lands_on_quantum_counterpart(piece, &shot, None).unwrap());
        let mut castle = MoveTarget::at(Square { row: 2, col: 0 });
        castle.flags.insert("castle".into(), json!(true));
        castle.flags.insert("rookTo".into(), json!(ghost));
        assert!(move_lands_on_quantum_counterpart(piece, &castle, None).unwrap());
        let mut portal = MoveTarget::at(Square { row: 3, col: 3 });
        portal.flags.insert("portalLanding".into(), json!(true));
        portal.flags.insert("portalExit".into(), json!(ghost));
        assert!(move_lands_on_quantum_counterpart(piece, &portal, None).unwrap());
    }

    #[test]
    #[ignore = "requires external source-pinned quantum JSON and verified SHA256"]
    fn source_pinned_quantum_state_rng_history_and_identity_match() {
        use crate::V7HostPosition;
        use sha2::{Digest, Sha256};

        let path = std::env::var("ACCELERATE_V7_QUANTUM_SOURCE_CASES")
            .expect("set ACCELERATE_V7_QUANTUM_SOURCE_CASES to the source receipt");
        let expected_sha = std::env::var("ACCELERATE_V7_QUANTUM_SOURCE_CASES_SHA256")
            .expect("set ACCELERATE_V7_QUANTUM_SOURCE_CASES_SHA256 to its verified digest");
        let bytes = std::fs::read(path).unwrap();
        assert!(
            bytes.len() <= 4 * 1024 * 1024,
            "quantum receipt exceeds 4 MiB"
        );
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected_sha);
        let receipt: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(receipt["schemaVersion"], 1);
        assert_eq!(
            receipt["sourceSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        assert_eq!(
            receipt["profile"],
            "accelerate-headless-semantic-v7-faithful-init-v1"
        );
        let cases = receipt["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 16, "bounded quantum source corpus changed");
        let mut failures = Vec::new();
        let mut compared = 0;
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let before = V7HostPosition::from_envelope(case["before"].clone())
                .unwrap_or_else(|error| panic!("{name}: source import: {error}"));
            let operation = &case["operation"];
            let executed = before.transact(before.position_id(), |state| {
                match operation["kind"].as_str().unwrap() {
                    "observe" => {
                        let at = serde_json::from_value(operation["at"].clone()).map_err(EngineError::serialization)?;
                        let observer = serde_json::from_value::<Option<Color>>(operation["observer"].clone())
                            .map_err(EngineError::serialization)?;
                        Ok(match observe_quantum_at_in_place(state, at, observer)? {
                            QuantumObservation::NoQuantum => json!(false),
                            QuantumObservation::Real => json!("real"),
                            QuantumObservation::Illusion => json!("illusion"),
                        })
                    }
                    "materializeCleanup" => {
                        let inserted = materialize_quantum_shadows(state)?;
                        let mut cells = Vec::new();
                        for (row, occupants) in state.board.iter().enumerate() {
                            for (col, occupant) in occupants.iter().enumerate() {
                                if let Some(piece) = occupant.as_ref().filter(|piece| piece.flag("isQuantumShadow")) {
                                    cells.push(json!({"row":row,"col":col,"id":piece.id,
                                        "sourceId":piece.extra["quantumSourceId"],"anchorRow":piece.extra["anchorRow"],
                                        "anchorCol":piece.extra["anchorCol"],"quantum":piece.extra["quantum"]}));
                                }
                            }
                        }
                        cleanup_quantum_shadows(state, &inserted)?;
                        Ok(json!(cells))
                    }
                    "afterMove" => {
                        let at = serde_json::from_value(operation["at"].clone()).map_err(EngineError::serialization)?;
                        let mut piece = state.at(at).cloned().ok_or(EngineError::IllegalAction)?;
                        let candidates = serde_json::from_value::<Vec<MoveTarget>>(operation["candidates"].clone())
                            .map_err(EngineError::serialization)?;
                        Ok(json!(apply_after_move_in_place(state, &mut piece, at, &candidates)?))
                    }
                    _ => Err(EngineError::InvalidState("unknown quantum source probe operation".into())),
                }
            });
            let (after, result) = match executed {
                Ok(result) => result,
                Err(error) => {
                    failures.push(format!("{name}: native execution: {error}"));
                    continue;
                }
            };
            if result != case["result"] {
                failures.push(format!(
                    "{name}: outcome differs: source={} native={result}",
                    case["result"]
                ));
                continue;
            }
            let actual = after.export_envelope().unwrap();
            if serde_jcs::to_vec(&actual).unwrap() != serde_jcs::to_vec(&case["after"]).unwrap() {
                failures.push(format!(
                    "{name}: full Position differs at {}",
                    first_source_difference(&case["after"], &actual, "$")
                        .unwrap_or_else(|| "canonical representation".into())
                ));
                continue;
            }
            compared += 1;
        }
        assert!(
            failures.is_empty(),
            "{compared}/16 source quantum cases matched; {} gaps:\n{}",
            failures.len(),
            failures.join("\n")
        );
        assert_eq!(compared, 16);
    }

    #[test]
    #[ignore = "requires external source-pinned quantum UI JSON and verified SHA256"]
    fn source_pinned_quantum_ui_queries_bindings_and_transitions_match() {
        use crate::tests::source_callback_fixture::collect_case_diagnostics;
        use sha2::{Digest, Sha256};

        let path = std::env::var("ACCELERATE_V7_QUANTUM_UI_CASES")
            .expect("set ACCELERATE_V7_QUANTUM_UI_CASES to the source UI receipt");
        let expected_sha = std::env::var("ACCELERATE_V7_QUANTUM_UI_CASES_SHA256")
            .expect("set ACCELERATE_V7_QUANTUM_UI_CASES_SHA256 to its verified digest");
        let bytes = std::fs::read(path).unwrap();
        assert!(
            bytes.len() <= 8 * 1024 * 1024,
            "quantum UI receipt exceeds 8 MiB"
        );
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected_sha);
        let receipt: Value = serde_json::from_slice(&bytes).unwrap();
        crate::state::validate_json_value(&receipt, 0).unwrap();
        assert_eq!(receipt["schemaVersion"], 1);
        assert_eq!(
            receipt["sourceSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        assert_eq!(
            receipt["profile"],
            "accelerate-headless-semantic-v7-faithful-init-v1"
        );
        const CASE_NAMES: [&str; 10] = [
            "query-rook-ghost-ordered",
            "query-rook-physical-counterpart-excluded",
            "query-knight-ghost-ordered",
            "query-large-ghost-body-alias",
            "query-real-occupant-wins-over-ghost",
            "query-pending-lobster-blocks-ghost",
            "ui-knight-from-ghost",
            "ui-metal-roller-rook-wrapper-after-materialization",
            "ui-medium-memory-wrapper-after-materialization",
            "ui-large-from-ghost-body-alias",
        ];
        let cases = receipt["cases"].as_array().unwrap();
        assert_eq!(
            cases.len(),
            CASE_NAMES.len(),
            "bounded quantum UI corpus changed"
        );
        let mut mismatches = Vec::new();
        let mut compared = 0;
        let mut compared_kinds = [0, 0];
        for (case, name) in cases.iter().zip(CASE_NAMES) {
            assert_eq!(
                case["name"], name,
                "quantum UI receipt cases are missing, repeated or reordered"
            );
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                let count_before = mismatches.len();
                let kind = compare_source_quantum_ui_case(case, name, mismatches)?;
                if mismatches.len() == count_before {
                    compared += 1;
                    compared_kinds[kind] += 1;
                }
                Ok(())
            });
        }
        assert!(
            mismatches.is_empty(),
            "{compared}/10 source quantum UI cases matched (query={}, uiMove={}); {} gaps:\n{}",
            compared_kinds[0],
            compared_kinds[1],
            mismatches.len(),
            mismatches.join("\n")
        );
        assert_eq!(
            compared, 10,
            "all ten independent source UI cases must compare successfully"
        );
        assert_eq!(
            compared_kinds,
            [6, 4],
            "six queries and four actual public UI moves must execute"
        );
    }

    fn compare_source_quantum_ui_case(
        case: &Value,
        name: &str,
        mismatches: &mut Vec<String>,
    ) -> Result<usize> {
        use crate::V7HostPosition;
        use crate::tests::source_callback_fixture::compare_value;

        let invalid = |message: &str| {
            EngineError::InvalidState(format!("source quantum UI fixture: {message}"))
        };
        if let Some(error) = case.get("error") {
            return Err(invalid(&format!("source execution error: {error}")));
        }
        let before = V7HostPosition::from_envelope(case["before"].clone())?;
        let source_after = V7HostPosition::from_envelope(case["after"].clone())?;
        compare_value(
            &case["before"],
            &before.export_envelope()?,
            &format!("{name}.input.envelope"),
            mismatches,
        )?;
        let operation = &case["operation"];
        let from = serde_json::from_value::<Square>(operation["from"].clone())
            .map_err(EngineError::serialization)?;
        let source_result = case["result"]
            .as_object()
            .ok_or_else(|| invalid("result must be an object"))?;
        let source_selection = source_result
            .get("selection")
            .ok_or_else(|| invalid("result.selection is missing"))?;
        let source_moves = source_result
            .get("moves")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("result.moves must be an ordered array"))?;
        let selection = selection_from_ghost_click(before.state(), from)?;
        let (actual_selection, moves) = match selection.as_ref() {
            Some(selected) => (
                json!({"origin":selected.origin,"ghost":selected.ghost,"pieceId":selected.piece.id}),
                legal_moves_for_quantum_selection(before.state(), selected.origin, selected.ghost)?,
            ),
            None => (Value::Null, Vec::new()),
        };
        compare_value(
            source_selection,
            &actual_selection,
            &format!("{name}.query.selection"),
            mismatches,
        )?;
        compare_value(
            &Value::Array(source_moves.clone()),
            &json!(moves),
            &format!("{name}.query.moves"),
            mismatches,
        )?;
        let after_query = before.export_envelope()?;
        compare_value(
            &case["before"],
            &after_query,
            &format!("{name}.query.unchangedEnvelope"),
            mismatches,
        )?;

        match operation["kind"].as_str() {
            Some("selectionQuery") => {
                let source_empty = source_moves.is_empty();
                let valid_witness = match name {
                    "query-real-occupant-wins-over-ghost" => {
                        source_selection.is_null() && source_empty
                    }
                    "query-pending-lobster-blocks-ghost" => {
                        !source_selection.is_null() && source_empty
                    }
                    _ => !source_selection.is_null() && !source_empty,
                };
                if !valid_witness {
                    return Err(invalid(
                        "source query did not exercise its selected or blocked ghost boundary",
                    ));
                }
                for field in ["publicIntent", "hostAction"] {
                    if source_result.get(field) != Some(&Value::Null) {
                        return Err(invalid("selectionQuery must not perform a public UI move"));
                    }
                }
                compare_value(
                    &case["before"],
                    &case["after"],
                    &format!("{name}.sourceQuery.unchangedEnvelope"),
                    mismatches,
                )?;
                compare_value(
                    &case["after"],
                    &after_query,
                    &format!("{name}.query.after.envelope"),
                    mismatches,
                )?;
                Ok(0)
            }
            Some("uiMove") => {
                let selected = selection
                    .as_ref()
                    .ok_or_else(|| invalid("native UI click has no quantum selection"))?;
                if source_moves.is_empty() || source_selection.is_null() {
                    return Err(invalid("source UI move has no positive quantum candidate"));
                }
                let destination =
                    serde_json::from_value::<Square>(operation["destination"].clone())
                        .map_err(EngineError::serialization)?;
                let public_intent = source_result
                    .get("publicIntent")
                    .filter(|value| value.is_object())
                    .ok_or_else(|| invalid("source UI publicIntent is missing"))?;
                compare_value(
                    &json!({"type":"move","color":selected.piece.color,"from":from,"destination":destination}),
                    public_intent,
                    &format!("{name}.publicIntent"),
                    mismatches,
                )?;
                let source_action = source_result
                    .get("hostAction")
                    .filter(|value| value.is_object())
                    .ok_or_else(|| invalid("source UI hostAction is missing"))?;
                let action =
                    crate::v7_action_surface::resolve_public_move(before.state(), public_intent)?;
                compare_value(
                    source_action,
                    &serde_json::to_value(&action).map_err(EngineError::serialization)?,
                    &format!("{name}.publicBinding.hostAction"),
                    mismatches,
                )?;
                // 실제 UI는 ghost materialization 후 wrapper의 metal/roller/medium
                // context를 수집한다. 원문 witness만 검사하며 production debug 필드를 만들지 않는다.
                let wrappers = source_result
                    .get("wrapperSnapshots")
                    .and_then(Value::as_array)
                    .filter(|wrappers| wrappers.len() == 1)
                    .ok_or_else(|| {
                        invalid("source UI must enter exactly one actual movePiece wrapper")
                    })?;
                let wrapper = &wrappers[0];
                compare_value(
                    &json!(selected.ghost),
                    &wrapper["from"],
                    &format!("{name}.sourceWrapper.from"),
                    mismatches,
                )?;
                compare_value(
                    &json!(selected.piece.id),
                    &wrapper["moverId"],
                    &format!("{name}.sourceWrapper.moverId"),
                    mismatches,
                )?;
                compare_value(
                    &json!(false),
                    &wrapper["hasQuantum"],
                    &format!("{name}.sourceWrapper.hasQuantum"),
                    mismatches,
                )?;
                compare_value(
                    &source_action["move"]["quantumFrom"],
                    &wrapper["quantumFrom"],
                    &format!("{name}.sourceWrapper.quantumFrom"),
                    mismatches,
                )?;
                if name == "ui-metal-roller-rook-wrapper-after-materialization" {
                    let mover_origin = json!({"id":selected.piece.id,"row":selected.ghost.row,"col":selected.ghost.col});
                    if !wrapper["metalBefore"]
                        .as_array()
                        .is_some_and(|entries| entries.contains(&mover_origin))
                        || !wrapper["roller"]["origins"]
                            .as_array()
                            .is_some_and(|entries| entries.contains(&mover_origin))
                    {
                        return Err(invalid(
                            "source metal and Roller contexts did not capture the materialized ghost origin",
                        ));
                    }
                }
                if name == "ui-medium-memory-wrapper-after-materialization"
                    && (wrapper["medium"]["pieceId"] != json!(selected.piece.id)
                        || wrapper["medium"]
                            .get("baseMemory")
                            .is_none_or(Value::is_null))
                {
                    return Err(invalid(
                        "source Medium wrapper did not capture mover identity and base memory",
                    ));
                }
                let source_target =
                    serde_json::from_value::<MoveTarget>(source_action["move"].clone())
                        .map_err(EngineError::serialization)?;
                if source_after.position_id() == before.position_id()
                    || source_after
                        .state()
                        .at(source_target.square())
                        .is_none_or(|piece| piece.id != selected.piece.id)
                {
                    return Err(invalid(
                        "source public UI move did not reach its declared destination",
                    ));
                }
                let (after, _) = before.transact(before.position_id(), |state| {
                    crate::transition::apply_without_public_event(state, &action)
                })?;
                let actual_after = after.export_envelope()?;
                // 전체 envelope에 버전·state·RNG·history·PositionID가 포함된다.
                // 공통 JCS 진단은 정수/실수 표현 차이와 실제 값 차이를 구분한다.
                compare_value(
                    &case["after"],
                    &actual_after,
                    &format!("{name}.uiMove.after.envelope"),
                    mismatches,
                )?;
                compare_value(
                    &case["before"],
                    &before.export_envelope()?,
                    &format!("{name}.uiMove.originalEnvelope"),
                    mismatches,
                )?;
                Ok(1)
            }
            _ => Err(invalid("unknown bounded quantum UI operation")),
        }
    }

    fn first_source_difference(expected: &Value, actual: &Value, path: &str) -> Option<String> {
        if serde_jcs::to_vec(expected).ok()? == serde_jcs::to_vec(actual).ok()? {
            return None;
        }
        match (expected, actual) {
            (Value::Object(left), Value::Object(right)) => {
                let mut keys = left.keys().chain(right.keys()).collect::<Vec<_>>();
                keys.sort_by(|a, b| {
                    (a.as_str() == "positionId")
                        .cmp(&(b.as_str() == "positionId"))
                        .then_with(|| a.cmp(b))
                });
                keys.dedup();
                for key in keys {
                    let child = format!("{path}.{key}");
                    match (left.get(key), right.get(key)) {
                        (Some(a), Some(b)) => {
                            if let Some(difference) = first_source_difference(a, b, &child) {
                                return Some(difference);
                            }
                        }
                        _ => return Some(child),
                    }
                }
                Some(path.into())
            }
            (Value::Array(left), Value::Array(right)) => {
                for (index, (a, b)) in left.iter().zip(right).enumerate() {
                    if let Some(difference) =
                        first_source_difference(a, b, &format!("{path}[{index}]"))
                    {
                        return Some(difference);
                    }
                }
                Some(format!("{path}[{}]", left.len().min(right.len())))
            }
            _ => Some(path.into()),
        }
    }
}
