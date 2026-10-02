//! Frozen v7 destination additions for Symmetry, E4, Relay, and Solidarity.
//! State enable/expiry belongs to the card and turn owners. This module only
//! constructs descriptors in the source getLegalMoves prepend order.

use crate::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn ensure_geometry_context(state: &GameState, piece: &Piece, from: Square) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7
        || state.board.len() != 8
        || state.board.iter().any(|row| row.len() != 8)
        || from.row >= 8
        || from.col >= 8
        || piece.is_large() && (from.row >= 7 || from.col >= 7)
    {
        return Err(EngineError::InvalidState(
            "v7 geometry requires an 8x8 v7 board and a normalized piece origin".into(),
        ));
    }
    Ok(())
}

fn same_identity(first: &Piece, second: &Piece) -> bool {
    std::ptr::eq(first, second) || !first.id.is_empty() && first.id == second.id
}

/// Source solidarityPairAllowed uses physical catalog type, not the movement
/// ability of Trickster or a remembered base movement. Royal identity is
/// evaluated against the live Regency/kingDead context.
pub(crate) fn v7_solidarity_pair_allowed(state: &GameState, first: &Piece, second: &Piece) -> bool {
    !same_identity(first, second)
        && state.flag("solidarity", first.color)
        && first.color == second.color
        && [first, second]
            .into_iter()
            .all(|piece| crate::card_effects::minor(state, piece) && !state.royal_identity(piece))
}

fn prepend_source_moves(prefix: Vec<MoveTarget>, moves: Vec<MoveTarget>) -> Vec<MoveTarget> {
    if prefix.is_empty() {
        return moves;
    }
    let mut seen = BTreeSet::new();
    prefix
        .into_iter()
        .chain(moves)
        .filter(|target| seen.insert((target.row, target.col)))
        .collect()
}

fn large_cells(anchor: Square) -> Vec<Square> {
    [
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
    .into_iter()
    .filter(|cell| cell.row < 8 && cell.col < 8)
    .collect()
}

fn symmetry_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target_override: Option<Square>,
) -> Result<Vec<MoveTarget>> {
    if target_override.is_none() && !state.flag("symmetry", piece.color)
        || matches!(
            piece.kind.as_str(),
            "wall" | "football" | "blackHole" | "coffin"
        )
    {
        return Ok(Vec::new());
    }
    let to = target_override.unwrap_or(Square {
        row: from.row,
        col: if piece.is_large() {
            6 - from.col
        } else {
            7 - from.col
        },
    });
    if to == from {
        return Ok(Vec::new());
    }
    let mut target = MoveTarget::at(to);
    target.flags.insert("symmetryMove".into(), json!(true));
    if piece.is_large() {
        let cells = large_cells(to);
        if cells.iter().any(|&cell| {
            state.at(cell).is_some_and(|occupant| {
                occupant.color == piece.color && !same_identity(piece, occupant)
            })
        }) {
            return Ok(Vec::new());
        }
        let capture_limit = match piece.kind.as_str() {
            "bigBishop" => 3,
            "bigRook" => 2,
            _ => usize::MAX,
        };
        let Some(captures) =
            crate::movement::v7_large_landing_captures(state, piece, &cells, capture_limit, false)?
        else {
            return Ok(Vec::new());
        };
        target.flags.insert("anchorRow".into(), json!(to.row));
        target.flags.insert("anchorCol".into(), json!(to.col));
        target.flags.insert("highlightCells".into(), json!(cells));
        let (move_key, capture_key) = if piece.kind == "colossus" {
            ("colossusMove", "colossusLandingCaptures")
        } else {
            ("bigRookMove", "bigRookLandingCaptures")
        };
        target.flags.insert(move_key.into(), json!(true));
        target.flags.insert(capture_key.into(), json!(captures));
    } else if let Some(occupant) = state.at(to) {
        if occupant.color == piece.color
            || !crate::movement::v7_can_capture_target(state, piece, occupant, false, false)?
        {
            return Ok(Vec::new());
        }
        target.flags.insert("capture".into(), json!(true));
    }
    Ok(vec![target])
}

fn relay_recipient(piece: &Piece) -> bool {
    piece.ability_kind() != "slime"
        && !piece.is_large()
        && !matches!(
            piece.kind.as_str(),
            "wall" | "football" | "blackHole" | "monster" | "coffin"
        )
}

fn relay_moves(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let relay = state.flag("relay", piece.color);
    if !relay && !state.flag("solidarity", piece.color) || !relay_recipient(piece) {
        return Vec::new();
    }
    let mut moves = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let to = Square { row, col };
            let Some(candidate) = state.at(to) else {
                continue;
            };
            if same_identity(piece, candidate)
                || candidate.color != piece.color
                || !relay_recipient(candidate)
            {
                continue;
            }
            let solidarity = v7_solidarity_pair_allowed(state, piece, candidate);
            if !solidarity && (!relay || row != from.row && col != from.col) {
                continue;
            }
            let mut target = MoveTarget::at(to);
            target.flags.insert("relaySwap".into(), json!(true));
            if solidarity {
                target.flags.insert("solidaritySwap".into(), json!(true));
            }
            moves.push(target);
        }
    }
    moves
}

/// Source getLegalMoves prepends Symmetry, then E4, then all Relay/Solidarity
/// targets before Loyalist, Portal, and restrictions. Coordinates are deduped
/// after each nonempty prefix, keeping the first complete descriptor.
///
/// The population argument belongs to relaxed free-move probing: Princess and
/// Berserker use that board elsewhere. These four destination additions read
/// the active simulation board for occupants and do not use the population.
pub(crate) fn v7_apply_population_movement_modifiers(
    state: &GameState,
    piece: &Piece,
    from: Square,
    moves: Vec<MoveTarget>,
    _population: Option<&GameState>,
) -> Result<Vec<MoveTarget>> {
    ensure_geometry_context(state, piece, from)?;
    if piece.ability_kind() == "slime" {
        return Ok(moves);
    }
    let moves = prepend_source_moves(symmetry_moves(state, piece, from, None)?, moves);
    let moves = if state.flag("e4", piece.color) {
        // expansionNamedSquare("e", color, 8, 8): white e4, black e5.
        let to = Square {
            row: if piece.color == Color::Black { 3 } else { 4 },
            col: 4,
        };
        prepend_source_moves(symmetry_moves(state, piece, from, Some(to))?, moves)
    } else {
        moves
    };
    Ok(prepend_source_moves(relay_moves(state, piece, from), moves))
}

fn descriptor_square(value: &Value) -> Option<Square> {
    let row = value.get("row")?.as_f64()?;
    let col = value.get("col")?.as_f64()?;
    ((0.0..8.0).contains(&row)
        && (0.0..8.0).contains(&col)
        && row.fract() == 0.0
        && col.fract() == 0.0)
        .then_some(Square {
            row: row as u8,
            col: col as u8,
        })
}

fn source_destination(target: &MoveTarget) -> Option<Square> {
    if target.flag("portalLanding")
        && let Some(exit) = target.flags.get("portalExit").and_then(descriptor_square)
    {
        return Some(exit);
    }
    if let Some((row, col)) = target
        .flags
        .get("anchorRow")
        .and_then(Value::as_f64)
        .zip(target.flags.get("anchorCol").and_then(Value::as_f64))
        && (0.0..8.0).contains(&row)
        && (0.0..8.0).contains(&col)
        && row.fract() == 0.0
        && col.fract() == 0.0
    {
        return Some(Square {
            row: row as u8,
            col: col as u8,
        });
    }
    Square::new(target.row, target.col).ok()
}

fn unique_entries(state: &GameState) -> Vec<(Square, &Piece)> {
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            if let Some(piece) = state.at(at)
                && (piece.id.is_empty() || seen.insert(piece.id.as_str()))
            {
                entries.push((at, piece));
            }
        }
    }
    entries
}

fn piece_entry_by_id<'a>(state: &'a GameState, id: &str) -> Option<(Square, &'a Piece)> {
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            if let Some(piece) = state.at(at).filter(|piece| piece.id == id) {
                if piece.is_large()
                    && let Some((anchor_row, anchor_col)) =
                        crate::card_effects::js_number(piece.extra.get("anchorRow"), 0).zip(
                            crate::card_effects::js_number(piece.extra.get("anchorCol"), 0),
                        )
                    && (0.0..8.0).contains(&anchor_row)
                    && (0.0..8.0).contains(&anchor_col)
                    && anchor_row.fract() == 0.0
                    && anchor_col.fract() == 0.0
                {
                    let anchor = Square {
                        row: anchor_row as u8,
                        col: anchor_col as u8,
                    };
                    if state
                        .at(anchor)
                        .is_some_and(|part| part.id == piece.id && part.is_large())
                    {
                        return Some((anchor, piece));
                    }
                }
                return Some((at, piece));
            }
        }
    }
    None
}

fn move_destination_cells(target: &MoveTarget) -> Vec<Square> {
    if (target.flag("colossusMove") || target.flag("bigRookMove"))
        && let Some(cells) = target.flags.get("highlightCells").and_then(Value::as_array)
    {
        return cells.iter().filter_map(descriptor_square).collect();
    }
    source_destination(target).into_iter().collect()
}

/// Source Majesty destination predicate shared by moves and Evacuation.
/// Callers provide the actual projected destination cells for the same board.
pub(crate) fn majesty_destination_blocked(
    state: &GameState,
    piece: &Piece,
    cells: &[Square],
) -> bool {
    // source opponent("neutral") is white; imported neutral types cannot be
    // Major, but retain the original color fallback in this shared predicate.
    let enemy = if piece.color == Color::White {
        Color::Black
    } else {
        Color::White
    };
    state.flag("majesty", enemy)
        && crate::v7_card_passive::is_major_piece(state, piece)
        && unique_entries(state).iter().any(|(royal_at, royal)| {
            royal.color == enemy
                && state.royal_identity(royal)
                && cells.iter().any(|to| {
                    royal_at
                        .row
                        .abs_diff(to.row)
                        .max(royal_at.col.abs_diff(to.col))
                        == 1
                })
        })
}

fn majesty_move_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
) -> bool {
    if [
        "colossusAttack",
        "shotgunBlast",
        "shotgunSnipe",
        "setLogDirection",
    ]
    .into_iter()
    .any(|key| target.flag(key))
    {
        return true;
    }
    let cells = move_destination_cells(target);
    if majesty_destination_blocked(state, piece, &cells) {
        return false;
    }
    let swapped = ["dragonSwap", "substitutionSwap", "relaySwap"]
        .into_iter()
        .any(|key| target.flag(key))
        .then(|| state.at(target.square()))
        .flatten();
    if let Some(swapped) = swapped {
        let destinations = if swapped.is_large() {
            large_cells(from)
        } else {
            vec![from]
        };
        if majesty_destination_blocked(state, swapped, &destinations) {
            return false;
        }
    }
    // source98262 accepts the bond's string identity through JS truthiness.
    if crate::observation::truth(piece.extra.get("twinBondId"))
        && let Some(partner) = piece
            .extra
            .get("twinPartnerId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .and_then(|id| piece_entry_by_id(state, id))
        && partner.1.color == piece.color
        && swapped.is_none_or(|swapped| !same_identity(swapped, partner.1))
        && majesty_destination_blocked(state, partner.1, &cells)
    {
        return false;
    }
    true
}

fn chain_move_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
) -> Result<bool> {
    let bonds = crate::card_effects::normalize_chain_bonds(state.extra.get("chainBonds"))?;
    if bonds.is_empty() {
        return Ok(true);
    }
    let mut projected = BTreeMap::<&str, Option<Square>>::new();
    if !piece.id.is_empty() {
        let stationary = [
            "colossusBody",
            "colossusAttack",
            "shotgunBlast",
            "shotgunSnipe",
            "merchantBuy",
            "setLogDirection",
        ]
        .into_iter()
        .any(|key| target.flag(key));
        projected.insert(
            piece.id.as_str(),
            if stationary {
                Some(from)
            } else {
                source_destination(target)
            },
        );
        if let Some(swapped) = state
            .at(target.square())
            .filter(|_| {
                ["dragonSwap", "substitutionSwap", "relaySwap"]
                    .into_iter()
                    .any(|key| target.flag(key))
            })
            .filter(|swapped| !swapped.id.is_empty())
        {
            projected.insert(swapped.id.as_str(), Some(from));
        }
        if target.flag("switcherooMove")
            && let Some(switched) = state
                .at(target.square())
                .filter(|switched| !switched.id.is_empty())
        {
            projected.insert(switched.id.as_str(), None);
        }
        if target.flag("castle")
            && let Some(rook_from) = target.flags.get("rookFrom").and_then(descriptor_square)
            && let Some(rook_to) = target.flags.get("rookTo").and_then(descriptor_square)
            && let Some(rook) = state.at(rook_from).filter(|rook| !rook.id.is_empty())
        {
            projected.insert(rook.id.as_str(), Some(rook_to));
        }
    }
    for bond in &bonds {
        let (first_id, second_id) = normalized_bond_ids(bond)?;
        let (Some((first, _)), Some((second, _))) = (
            piece_entry_by_id(state, first_id),
            piece_entry_by_id(state, second_id),
        ) else {
            continue;
        };
        let first = projected.get(first_id).copied().unwrap_or(Some(first));
        let second = projected.get(second_id).copied().unwrap_or(Some(second));
        if let Some((first, second)) = first.zip(second)
            && first
                .row
                .abs_diff(second.row)
                .max(first.col.abs_diff(second.col))
                > 2
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn normalized_bond_ids(bond: &Value) -> Result<(&str, &str)> {
    let first = bond.get("aId").and_then(Value::as_str).ok_or_else(|| {
        EngineError::InvalidState("v7 normalized chain bond is missing string aId".into())
    })?;
    let second = bond.get("bId").and_then(Value::as_str).ok_or_else(|| {
        EngineError::InvalidState("v7 normalized chain bond is missing string bId".into())
    })?;
    Ok((first, second))
}

/// Source isChainDestinationAllowed is used by automatic movement before a
/// full descriptor exists. Only bonds involving this object constrain it;
/// a missing partner does not prevent movement, and no bond is mutated here.
pub(crate) fn chain_destination_allowed(
    state: &GameState,
    piece: &Piece,
    destination: Square,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7
        || state.board.len() != 8
        || state.board.iter().any(|row| row.len() != 8)
        || destination.row >= 8
        || destination.col >= 8
    {
        return Err(EngineError::InvalidState(
            "v7 chain destination requires an 8x8 v7 board and in-bounds square".into(),
        ));
    }
    if piece.id.is_empty() {
        return Ok(true);
    }
    for bond in crate::card_effects::normalize_chain_bonds(state.extra.get("chainBonds"))? {
        let (first, second) = normalized_bond_ids(&bond)?;
        let partner = if first == piece.id {
            second
        } else if second == piece.id {
            first
        } else {
            continue;
        };
        if let Some((at, _)) = piece_entry_by_id(state, partner)
            && at
                .row
                .abs_diff(destination.row)
                .max(at.col.abs_diff(destination.col))
                > 2
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Source Majesty precedes Fianchetto and the separate Chain filter.
/// Exposing this stage preserves their short-circuit and error order.
pub(crate) fn v7_majesty_move_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
) -> Result<bool> {
    ensure_geometry_context(state, piece, from)?;
    Ok(majesty_move_allowed(state, piece, from, target))
}

/// Source Chain is evaluated only after Majesty and Fianchetto allow a move.
pub(crate) fn v7_chain_move_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
) -> Result<bool> {
    ensure_geometry_context(state, piece, from)?;
    chain_move_allowed(state, piece, from, target)
}

/// Source Majesty and Chain are the final geometry filters in
/// applyMoveRestrictions. Swap partners and twins are tested as separate
/// relocated objects; ordinary captures keep the source pre-move bond view.
/// Receipt queries combine these stages without a Fianchetto stage.
#[cfg(test)]
pub(crate) fn v7_final_geometry_move_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
) -> Result<bool> {
    ensure_geometry_context(state, piece, from)?;
    Ok(majesty_move_allowed(state, piece, from, target)
        && chain_move_allowed(state, piece, from, target)?)
}

/// Source High Ground tests each large landing cell against the corresponding
/// source footprint cell. An anchor on High Ground alone does not authorize
/// captures by the other body cells.
pub(crate) fn v7_high_ground_capture_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
) -> Result<bool> {
    ensure_geometry_context(state, piece, from)?;
    let Some(high_ground) = state
        .extra
        .get("highGround")
        .and_then(Value::as_array)
        .filter(|cells| !cells.is_empty())
    else {
        return Ok(true);
    };
    let high_ground: BTreeSet<Square> = high_ground.iter().filter_map(descriptor_square).collect();
    if [
        "substitutionSwap",
        "relaySwap",
        "merchantBuy",
        "colossusBody",
        "castle",
        "setLogDirection",
    ]
    .into_iter()
    .any(|key| target.flag(key))
    {
        return Ok(true);
    }
    let large = target.flag("bigRookMove") || target.flag("colossusMove");
    if !large && high_ground.contains(&from) {
        return Ok(true);
    }
    // moveCaptureTargetCells additionally includes Siege Ram highlights;
    // isHighGroundCaptureBlocked does not consume that source list.
    let mut descriptor = target.clone();
    if target.flag("siegeRamMove") && !target.flag("shotgunBlast") && !target.flag("colossusAttack")
    {
        descriptor.flags.remove("highlightCells");
    }
    for cell in crate::movement::v7_move_capture_target_cells(&descriptor)? {
        if !high_ground.contains(&cell) {
            continue;
        }
        let source = if large {
            let row = i16::from(from.row) + i16::from(cell.row) - i16::from(target.row);
            let col = i16::from(from.col) + i16::from(cell.col) - i16::from(target.col);
            ((0..8).contains(&row) && (0..8).contains(&col)).then_some(Square {
                row: row as u8,
                col: col as u8,
            })
        } else {
            Some(from)
        };
        if source.is_some_and(|source| high_ground.contains(&source)) {
            continue;
        }
        let Some(victim) = state.at(cell) else {
            continue;
        };
        if !target.flag("brutusBetrayal") && victim.color == piece.color {
            continue;
        }
        let mut capture_probe = MoveTarget::at(cell);
        if let Some(value) = target.flags.get("brutusBetrayal") {
            capture_probe
                .flags
                .insert("brutusBetrayal".into(), value.clone());
        }
        if crate::movement::v7_is_capture_move(state, piece, &capture_probe)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn movement_name(kind: &str) -> String {
    let mut normalized = String::with_capacity(kind.len());
    for character in kind.chars() {
        if character.is_ascii_uppercase() {
            normalized.push('-');
            normalized.push(character.to_ascii_lowercase());
        } else {
            normalized.push(character);
        }
    }
    normalized
}

fn current_movement_name(state: &GameState, piece: &Piece) -> String {
    let base = crate::card_effects::current_base_movement(state, piece);
    movement_name(
        base.as_ref()
            .and_then(|base| base.get("type"))
            .and_then(Value::as_str)
            .unwrap_or(piece.ability_kind()),
    )
}

fn source_positive(value: Option<&Value>) -> Result<bool> {
    if let Some(number) = crate::card_effects::js_number(value, 0) {
        return Ok(number > 0.0);
    }
    // Number("Infinity") is positive in the source even though JSON numeric
    // literals must be finite. The shared finite-number reader omits it.
    let mut value = value;
    for _ in 0..=64 {
        match value {
            Some(Value::String(text)) => {
                return Ok(matches!(text.trim(), "Infinity" | "+Infinity"));
            }
            Some(Value::Array(entries)) if entries.len() == 1 => value = entries.first(),
            _ => return Ok(false),
        }
    }
    Err(EngineError::UnsupportedFeature(
        "v7 geometry Number coercion nesting exceeds 64".into(),
    ))
}

fn enemy_only_radiance(state: &GameState) -> Result<bool> {
    let source = state
        .extra
        .get("cardState")
        .filter(|value| crate::observation::truth(Some(value)));
    let profile = source.map_or_else(
        || state.extra.get("profile"),
        |source| source.get("profile"),
    );
    if let Some(hash) = profile
        .and_then(|profile| profile.get("catalogHash"))
        .filter(|hash| crate::observation::truth(Some(hash)))
    {
        if hash.as_str() != Some("yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4") {
            return Err(EngineError::UnsupportedFeature(
                "v7 radiance on unverified historical catalog".into(),
            ));
        }
        return Ok(true);
    }
    Ok(state.extra.get("extinctionMinorTargets") != Some(&json!(false)))
}

fn radiance_cells(state: &GameState, moving_color: Option<PieceColor>) -> BTreeSet<Square> {
    let all_squares = crate::card_effects::september18(state);
    let mut lit = BTreeSet::new();
    for (from, piece) in unique_entries(state) {
        if piece.ability_kind() != "paladin"
            || !all_squares && (from.row + from.col) % 2 != 0
            || moving_color.is_some_and(|color| color == piece.color)
        {
            continue;
        }
        if all_squares {
            lit.insert(from);
        }
        for &(dr, dc) in crate::movement::KING {
            if let Some(to) = from.offset(dr, dc) {
                lit.insert(to);
            }
        }
    }
    lit
}

fn straight_witness_clear(
    state: &GameState,
    from: Square,
    to: Square,
    lit: &BTreeSet<Square>,
    include_landing: bool,
) -> bool {
    let dr = i16::from(to.row) - i16::from(from.row);
    let dc = i16::from(to.col) - i16::from(from.col);
    if dr != 0 && dc != 0 {
        return false;
    }
    let distance = dr.abs().max(dc.abs());
    let last = if include_landing {
        distance
    } else {
        distance.saturating_sub(1)
    };
    for step in 1..=last {
        let cell = Square {
            row: (i16::from(from.row) + dr.signum() * step) as u8,
            col: (i16::from(from.col) + dc.signum() * step) as u8,
        };
        if lit.contains(&cell) || state.at(cell).is_some() {
            return false;
        }
    }
    true
}

fn bent_radiance_witness(
    state: &GameState,
    from: Square,
    to: Square,
    lit: &BTreeSet<Square>,
) -> bool {
    if from.row == to.row || from.col == to.col {
        return straight_witness_clear(state, from, to, lit, false);
    }
    [
        Square {
            row: from.row,
            col: to.col,
        },
        Square {
            row: to.row,
            col: from.col,
        },
    ]
    .into_iter()
    .any(|corner| {
        straight_witness_clear(state, from, corner, lit, true)
            && straight_witness_clear(state, corner, to, lit, false)
    })
}

fn cardinal_radiance_witness(
    state: &GameState,
    from: Square,
    to: Square,
    lit: &BTreeSet<Square>,
) -> bool {
    for &(start_dr, start_dc) in crate::movement::DIAG {
        let (mut dr, mut dc) = (i16::from(start_dr), i16::from(start_dc));
        let mut cursor = from;
        let mut seen = BTreeSet::new();
        for _ in 0..256 {
            if !(0..8).contains(&(i16::from(cursor.row) + dr)) {
                dr = -dr;
            }
            if !(0..8).contains(&(i16::from(cursor.col) + dc)) {
                dc = -dc;
            }
            let cell = Square {
                row: (i16::from(cursor.row) + dr) as u8,
                col: (i16::from(cursor.col) + dc) as u8,
            };
            if !seen.insert((cell, dr, dc)) || lit.contains(&cell) {
                break;
            }
            if cell == to {
                return true;
            }
            if state.at(cell).is_some() {
                break;
            }
            cursor = cell;
        }
    }
    false
}

fn three_move_allowed_inner(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
    movement: &str,
    teleport: bool,
    secondary: bool,
) -> Result<bool> {
    let to = if target.flag("portalLanding")
        && target
            .flags
            .get("portalExit")
            .is_some_and(|value| crate::observation::truth(Some(value)))
    {
        let Some(exit) = target.flags.get("portalExit").and_then(descriptor_square) else {
            return Ok(true);
        };
        exit
    } else {
        target.square()
    };
    if to.row >= 8 || to.col >= 8 {
        return Err(EngineError::InvalidState(
            "v7 Three movement destination is out of bounds".into(),
        ));
    }
    let distance = from.row.abs_diff(to.row).max(from.col.abs_diff(to.col));
    let metal_distance = if matches!(movement, "hook" | "brutus") {
        u16::from(from.row.abs_diff(to.row)) + u16::from(from.col.abs_diff(to.col))
    } else {
        u16::from(distance)
    };
    if piece.flag("metalized")
        && (source_positive(piece.extra.get("metalCooldown"))? || metal_distance > 1)
    {
        return Ok(false);
    }
    let occupant = state.at(to);
    if !secondary
        && target.flag("castle")
        && let Some(rook_from) = target.flags.get("rookFrom").and_then(descriptor_square)
        && let Some(rook_to) = target.flags.get("rookTo").and_then(descriptor_square)
        && let Some(rook) = state.at(rook_from)
        && !three_move_allowed_inner(
            state,
            rook,
            rook_from,
            &MoveTarget::at(rook_to),
            &movement_name(rook.ability_kind()),
            false,
            true,
        )?
    {
        return Ok(false);
    }
    if !secondary
        && [
            "relaySwap",
            "substitutionSwap",
            "dragonSwap",
            "switcherooMove",
        ]
        .into_iter()
        .any(|key| target.flag(key))
        && let Some(occupant) = occupant
        && !three_move_allowed_inner(
            state,
            occupant,
            to,
            &MoveTarget::at(from),
            &movement_name(occupant.ability_kind()),
            true,
            true,
        )?
    {
        return Ok(false);
    }
    let socialist_capture = !secondary
        && crate::card_effects::september26(state)
        && source_positive(
            state
                .extra
                .get("socialism")
                .and_then(|sides| sides.get(piece.color.as_str())),
        )?
        && !state.royal_identity(piece)
        && piece.ability_kind() != "slime";
    if piece.ability_kind() == "paladin"
        && !socialist_capture
        && !target.flag("crownGroundCapture")
        && (target.flag("capture")
            || target.flag("jumpCapture")
            || occupant.is_some_and(|occupant| {
                occupant.color != piece.color && !matches!(occupant.kind.as_str(), "crown" | "wall")
            }))
    {
        return Ok(false);
    }
    if (from.row + from.col).is_multiple_of(2)
        || !state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.ability_kind() == "paladin")
    {
        return Ok(true);
    }
    let lit = radiance_cells(state, enemy_only_radiance(state)?.then_some(piece.color));
    let landing = match target
        .flags
        .get("highlightCells")
        .filter(|value| crate::observation::truth(Some(value)))
    {
        Some(Value::Array(cells)) => {
            if cells.len() > 4096 {
                return Err(EngineError::UnsupportedFeature(
                    "v7 radiance highlight cell capacity".into(),
                ));
            }
            cells
                .iter()
                .filter_map(descriptor_square)
                .collect::<Vec<_>>()
        }
        Some(_) => {
            return Err(EngineError::InvalidState(
                "v7 radiance highlightCells must be an array".into(),
            ));
        }
        None => vec![to],
    };
    if landing.iter().any(|cell| lit.contains(cell))
        || ["portalEntry", "portalExit"].into_iter().any(|key| {
            target
                .flags
                .get(key)
                .and_then(descriptor_square)
                .is_some_and(|cell| lit.contains(&cell))
        })
    {
        return Ok(false);
    }
    if distance == 0
        || teleport
        || ["symmetryMove", "substitutionSwap", "relaySwap"]
            .into_iter()
            .any(|key| target.flag(key))
    {
        return Ok(true);
    }
    if matches!(movement, "hook" | "brutus") {
        return Ok(bent_radiance_witness(state, from, to, &lit));
    }
    if movement == "protestant" {
        return Ok(
            from.row.abs_diff(to.row) == from.col.abs_diff(to.col) && (1..=3).contains(&distance)
        );
    }
    if movement == "prime-minister" && distance == 2 {
        return Ok(crate::movement::KING.iter().any(|&(dr, dc)| {
            from.offset(dr, dc).is_some_and(|step| {
                !lit.contains(&step)
                    && state.at(step).is_none()
                    && step.row.abs_diff(to.row).max(step.col.abs_diff(to.col)) == 1
            })
        }));
    }
    if matches!(
        movement,
        "knight"
            | "paladin"
            | "royal-knight"
            | "camel"
            | "alfil"
            | "eagle"
            | "alibaba"
            | "dragon"
            | "assassin"
            | "thief"
            | "herald"
            | "grasshopper"
            | "slime"
            | "pegasus"
            | "checker"
            | "checker-king"
    ) || target.flag("jumpCapture")
        || target.flag("longEnPassant")
    {
        return Ok(true);
    }
    if movement == "cardinal" {
        return Ok(cardinal_radiance_witness(state, from, to, &lit));
    }
    let dr = i16::from(to.row) - i16::from(from.row);
    let dc = i16::from(to.col) - i16::from(from.col);
    if dr == 0 || dc == 0 || dr.abs() == dc.abs() {
        for step in 1..i16::from(distance) {
            let cell = Square {
                row: (i16::from(from.row) + dr.signum() * step) as u8,
                col: (i16::from(from.col) + dc.signum() * step) as u8,
            };
            if lit.contains(&cell) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Source threeMoveAllowed: Metal range/cooldown, both relocated swap/castle
/// objects, Paladin capture exclusions, then dark-square enemy radiance.
/// Secondary recursion is limited to one level, matching the source flag.
pub(crate) fn v7_three_move_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
) -> Result<bool> {
    ensure_geometry_context(state, piece, from)?;
    three_move_allowed_inner(
        state,
        piece,
        from,
        target,
        &current_movement_name(state, piece),
        false,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_state() -> GameState {
        let mut state = GameState::new(GameConfig::default(), 17).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.board = vec![vec![None; 8]; 8];
        state
    }

    #[test]
    fn source_prepend_order_keeps_solidarity_and_symmetry_descriptors() {
        let mut state = empty_state();
        let piece = Piece::new("knight", Color::White, "mover");
        let from = Square { row: 6, col: 1 };
        state.board[6][1] = Some(piece.clone());
        state.board[4][2] = Some(Piece::new("bishop", Color::White, "ally"));
        state.board[4][4] = Some(Piece::new("pawn", Color::Black, "capture"));
        for key in ["symmetry", "e4", "solidarity"] {
            state
                .extra
                .insert(key.into(), json!({"white":true,"black":false}));
        }
        let mut old_mirror = MoveTarget::at(Square { row: 6, col: 6 });
        old_mirror.flags.insert("basicTraining".into(), json!(true));
        let base = vec![
            MoveTarget::at(Square { row: 4, col: 4 }),
            old_mirror,
            MoveTarget::at(Square { row: 5, col: 1 }),
        ];
        let before = serde_jcs::to_vec(&state).unwrap();
        let moves =
            v7_apply_population_movement_modifiers(&state, &piece, from, base, None).unwrap();
        assert_eq!(
            serde_json::to_value(&moves).unwrap(),
            json!([
                {"row":4,"col":2,"relaySwap":true,"solidaritySwap":true},
                {"row":4,"col":4,"symmetryMove":true,"capture":true},
                {"row":6,"col":6,"symmetryMove":true},
                {"row":5,"col":1}
            ])
        );
        assert_eq!(serde_jcs::to_vec(&state).unwrap(), before);
    }

    #[test]
    fn solidarity_uses_catalog_physical_minor_and_live_royal_identity() {
        let mut state = empty_state();
        state
            .extra
            .insert("solidarity".into(), json!({"white":true,"black":false}));
        let first = Piece::new("knight", Color::White, "first");
        let mut trickster = Piece::new("trickster", Color::White, "second");
        trickster
            .extra
            .insert("tricksterMoveType".into(), json!("queen"));
        assert!(v7_solidarity_pair_allowed(&state, &first, &trickster));
        for kind in ["missionary", "cannon", "pawn", "rook"] {
            let second = Piece::new(kind, Color::White, "other");
            assert!(!v7_solidarity_pair_allowed(&state, &first, &second));
        }
        trickster.extra.insert("crownRoyal".into(), json!(true));
        assert!(!v7_solidarity_pair_allowed(&state, &first, &trickster));
        assert!(!v7_solidarity_pair_allowed(&state, &first, &first.clone()));
    }

    #[test]
    fn relay_remains_aligned_and_solidarity_preserves_row_major_targets() {
        let mut state = empty_state();
        let piece = Piece::new("bishop", Color::White, "mover");
        let from = Square { row: 4, col: 4 };
        state.board[4][4] = Some(piece.clone());
        state.board[1][6] = Some(Piece::new("knight", Color::White, "off-line-minor"));
        state.board[2][4] = Some(Piece::new("rook", Color::White, "aligned-major"));
        state.board[3][3] = Some(Piece::new("rook", Color::White, "off-line-major"));
        state
            .extra
            .insert("relay".into(), json!({"white":true,"black":false}));
        state
            .extra
            .insert("solidarity".into(), json!({"white":true,"black":false}));
        let moves =
            v7_apply_population_movement_modifiers(&state, &piece, from, Vec::new(), None).unwrap();
        assert_eq!(
            serde_json::to_value(moves).unwrap(),
            json!([
                {"row":1,"col":6,"relaySwap":true,"solidaritySwap":true},
                {"row":2,"col":4,"relaySwap":true}
            ])
        );
    }

    #[test]
    fn e4_uses_black_e5_and_simulation_occupants() {
        let mut state = empty_state();
        let piece = Piece::new("bishop", Color::Black, "mover");
        let from = Square { row: 6, col: 2 };
        state.board[6][2] = Some(piece.clone());
        state
            .extra
            .insert("e4".into(), json!({"white":false,"black":true}));
        let mut population = state.clone();
        population.board[3][4] = Some(Piece::new("pawn", Color::Black, "population-blocker"));
        let moves = v7_apply_population_movement_modifiers(
            &state,
            &piece,
            from,
            Vec::new(),
            Some(&population),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(moves).unwrap(),
            json!([
                {"row":3,"col":4,"symmetryMove":true}
            ])
        );
        state.board[3][4] = population.board[3][4].clone();
        assert!(
            v7_apply_population_movement_modifiers(
                &state,
                &piece,
                from,
                Vec::new(),
                Some(&population)
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn large_symmetry_uses_source_footprint_and_one_capture_per_identity() {
        let mut state = empty_state();
        let mut piece = Piece::new("bigRook", Color::White, "mover");
        piece.extra.insert("anchorRow".into(), json!(2));
        piece.extra.insert("anchorCol".into(), json!(1));
        let from = Square { row: 2, col: 1 };
        for cell in large_cells(from) {
            state.board[cell.row as usize][cell.col as usize] = Some(piece.clone());
        }
        let mut victim = Piece::new("bigBishop", Color::Black, "victim");
        victim.extra.insert("anchorRow".into(), json!(2));
        victim.extra.insert("anchorCol".into(), json!(5));
        for cell in large_cells(Square { row: 2, col: 5 }) {
            state.board[cell.row as usize][cell.col as usize] = Some(victim.clone());
        }
        state
            .extra
            .insert("symmetry".into(), json!({"white":true,"black":false}));
        let moves =
            v7_apply_population_movement_modifiers(&state, &piece, from, Vec::new(), None).unwrap();
        assert_eq!(
            serde_json::to_value(moves).unwrap(),
            json!([{
                "row":2,"col":5,"anchorRow":2,"anchorCol":5,
                "highlightCells":[{"row":2,"col":5},{"row":2,"col":6},{"row":3,"col":5},{"row":3,"col":6}],
                "symmetryMove":true,"bigRookMove":true,
                "bigRookLandingCaptures":[{"row":2,"col":5}]
            }])
        );
        state.board[2][5] = Some(Piece::new("pawn", Color::White, "ally"));
        assert!(
            v7_apply_population_movement_modifiers(&state, &piece, from, Vec::new(), None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn slime_and_self_destination_do_not_add_modifier_moves() {
        let mut state = empty_state();
        for key in ["symmetry", "e4", "relay", "solidarity"] {
            state
                .extra
                .insert(key.into(), json!({"white":true,"black":false}));
        }
        let slime = Piece::new("slime", Color::White, "slime");
        let from = Square { row: 4, col: 4 };
        let original = vec![MoveTarget::at(Square { row: 5, col: 4 })];
        assert_eq!(
            v7_apply_population_movement_modifiers(&state, &slime, from, original.clone(), None)
                .unwrap(),
            original
        );
        let piece = Piece::new("bishop", Color::White, "mover");
        state
            .extra
            .insert("symmetry".into(), json!({"white":false,"black":false}));
        assert!(
            v7_apply_population_movement_modifiers(&state, &piece, from, Vec::new(), None)
                .unwrap()
                .is_empty()
        );
        state.board.truncate(7);
        assert!(matches!(
            v7_apply_population_movement_modifiers(&state, &piece, from, Vec::new(), None),
            Err(EngineError::InvalidState(_))
        ));
    }

    #[test]
    fn high_ground_large_capture_uses_each_source_body_cell() {
        let mut state = empty_state();
        let piece = Piece::new("bigRook", Color::White, "mover");
        let from = Square { row: 4, col: 1 };
        state.board[4][1] = Some(piece.clone());
        state.board[4][5] = Some(Piece::new("pawn", Color::Black, "victim"));
        let mut target = MoveTarget::at(Square { row: 4, col: 4 });
        target.flags.insert("bigRookMove".into(), json!(true));
        target
            .flags
            .insert("bigRookLandingCaptures".into(), json!([{"row":4,"col":5}]));
        state.extra.insert(
            "highGround".into(),
            json!([{"row":4,"col":1},{"row":4,"col":5}]),
        );
        assert!(!v7_high_ground_capture_allowed(&state, &piece, from, &target).unwrap());
        state.extra.insert(
            "highGround".into(),
            json!([{"row":4,"col":2},{"row":4,"col":5}]),
        );
        assert!(v7_high_ground_capture_allowed(&state, &piece, from, &target).unwrap());
        let mut ordinary = MoveTarget::at(Square { row: 4, col: 5 });
        ordinary.flags.insert("capture".into(), json!(true));
        state.extra.insert(
            "highGround".into(),
            json!([{"row":4,"col":1},{"row":4,"col":5}]),
        );
        assert!(v7_high_ground_capture_allowed(&state, &piece, from, &ordinary).unwrap());
    }

    #[test]
    fn majesty_tests_swap_partner_and_royal_anchor_without_blocking_royal_cell() {
        let mut state = empty_state();
        let piece = Piece::new("knight", Color::White, "mover");
        let from = Square { row: 3, col: 3 };
        state.board[3][3] = Some(piece.clone());
        state.board[6][6] = Some(Piece::new("rook", Color::White, "swap-major"));
        state.board[2][2] = Some(Piece::new("king", Color::Black, "enemy-king"));
        state
            .extra
            .insert("majesty".into(), json!({"white":false,"black":true}));
        let mut target = MoveTarget::at(Square { row: 6, col: 6 });
        target.flags.insert("relaySwap".into(), json!(true));
        assert!(!v7_final_geometry_move_allowed(&state, &piece, from, &target).unwrap());
        state.board[6][6] = Some(Piece::new("bishop", Color::White, "swap-minor"));
        assert!(v7_final_geometry_move_allowed(&state, &piece, from, &target).unwrap());
        let queen = Piece::new("queen", Color::White, "major");
        assert!(
            v7_final_geometry_move_allowed(
                &state,
                &queen,
                from,
                &MoveTarget::at(Square { row: 2, col: 2 })
            )
            .unwrap()
        );
        assert!(
            !v7_final_geometry_move_allowed(
                &state,
                &queen,
                from,
                &MoveTarget::at(Square { row: 2, col: 3 })
            )
            .unwrap()
        );
        let mut twin = piece.clone();
        twin.extra.insert("twinBondId".into(), json!("bond"));
        twin.extra
            .insert("twinPartnerId".into(), json!("twin-major"));
        state.board[3][3] = Some(twin.clone());
        state.board[6][6] = Some(Piece::new("rook", Color::White, "twin-major"));
        let twin_destination = MoveTarget::at(Square { row: 2, col: 3 });
        assert!(!v7_final_geometry_move_allowed(&state, &twin, from, &twin_destination).unwrap());
        twin.extra.insert("twinBondId".into(), json!(""));
        assert!(v7_final_geometry_move_allowed(&state, &twin, from, &twin_destination).unwrap());
    }

    #[test]
    fn chain_projects_swaps_castling_switcheroo_and_automatic_destination() {
        let mut state = empty_state();
        let piece = Piece::new("rook", Color::White, "mover");
        let from = Square { row: 4, col: 4 };
        state.board[4][4] = Some(piece.clone());
        state.board[4][6] = Some(Piece::new("bishop", Color::White, "partner"));
        state.extra.insert(
            "chainBonds".into(),
            json!([{ "aId":"mover","bId":"partner","by":"black" }]),
        );
        assert!(
            !v7_final_geometry_move_allowed(
                &state,
                &piece,
                from,
                &MoveTarget::at(Square { row: 4, col: 2 })
            )
            .unwrap()
        );
        assert!(!chain_destination_allowed(&state, &piece, Square { row: 4, col: 2 }).unwrap());
        let mut swap = MoveTarget::at(Square { row: 4, col: 6 });
        swap.flags.insert("relaySwap".into(), json!(true));
        assert!(v7_final_geometry_move_allowed(&state, &piece, from, &swap).unwrap());
        state.extra.insert(
            "chainBonds".into(),
            json!([{ "aId":"partner","bId":"third" }]),
        );
        state.board[4][7] = Some(Piece::new("pawn", Color::White, "third"));
        assert!(!v7_final_geometry_move_allowed(&state, &piece, from, &swap).unwrap());
        let mut switch = swap.clone();
        switch.flags.remove("relaySwap");
        switch.flags.insert("switcherooMove".into(), json!(true));
        assert!(v7_final_geometry_move_allowed(&state, &piece, from, &switch).unwrap());
        let mut castle = MoveTarget::at(Square { row: 4, col: 3 });
        castle.flags.insert("castle".into(), json!(true));
        castle
            .flags
            .insert("rookFrom".into(), json!({"row":4,"col":6}));
        castle
            .flags
            .insert("rookTo".into(), json!({"row":4,"col":2}));
        assert!(!v7_final_geometry_move_allowed(&state, &piece, from, &castle).unwrap());
        state.board[4][7] = None;
        assert!(v7_final_geometry_move_allowed(&state, &piece, from, &castle).unwrap());
    }

    #[test]
    fn radiance_blocks_dark_rays_but_leaps_and_symmetry_only_check_landing() {
        let mut state = empty_state();
        let from = Square { row: 4, col: 1 };
        let rook = Piece::new("rook", Color::White, "mover");
        state.board[4][1] = Some(rook.clone());
        state.board[3][3] = Some(Piece::new("paladin", Color::Black, "paladin"));
        let to = MoveTarget::at(Square { row: 4, col: 6 });
        assert!(!v7_three_move_allowed(&state, &rook, from, &to).unwrap());
        let knight = Piece::new("knight", Color::White, "mover");
        assert!(v7_three_move_allowed(&state, &knight, from, &to).unwrap());
        let mut symmetry = to.clone();
        symmetry.flags.insert("symmetryMove".into(), json!(true));
        assert!(v7_three_move_allowed(&state, &rook, from, &symmetry).unwrap());
        symmetry.col = 3;
        assert!(!v7_three_move_allowed(&state, &rook, from, &symmetry).unwrap());
        state.board[3][3].as_mut().unwrap().color = Color::White.into();
        assert!(v7_three_move_allowed(&state, &rook, from, &to).unwrap());
    }

    #[test]
    fn metal_rechecks_swap_recipient_and_uses_hook_manhattan_range() {
        let mut state = empty_state();
        let piece = Piece::new("knight", Color::White, "mover");
        let from = Square { row: 4, col: 4 };
        state.board[4][4] = Some(piece.clone());
        let mut recipient = Piece::new("rook", Color::White, "metal-ally");
        recipient.extra.insert("metalized".into(), json!(true));
        recipient.extra.insert("metalCooldown".into(), json!("1"));
        state.board[4][5] = Some(recipient.clone());
        let mut swap = MoveTarget::at(Square { row: 4, col: 5 });
        swap.flags.insert("relaySwap".into(), json!(true));
        assert!(!v7_three_move_allowed(&state, &piece, from, &swap).unwrap());
        state.board[4][5]
            .as_mut()
            .unwrap()
            .extra
            .insert("metalCooldown".into(), json!(0));
        assert!(v7_three_move_allowed(&state, &piece, from, &swap).unwrap());
        recipient.kind = "hook".into();
        recipient.extra.insert("metalCooldown".into(), json!(0));
        assert!(
            !v7_three_move_allowed(
                &state,
                &recipient,
                from,
                &MoveTarget::at(Square { row: 5, col: 5 })
            )
            .unwrap()
        );
        assert!(
            v7_three_move_allowed(
                &state,
                &recipient,
                from,
                &MoveTarget::at(Square { row: 4, col: 5 })
            )
            .unwrap()
        );
    }

    #[test]
    fn paladin_capture_exception_is_specific_to_socialist_primary_or_ground_crown() {
        let mut state = empty_state();
        let piece = Piece::new("paladin", Color::White, "mover");
        let from = Square { row: 4, col: 4 };
        state.board[4][4] = Some(piece.clone());
        state.board[3][4] = Some(Piece::new("pawn", Color::Black, "victim"));
        let mut target = MoveTarget::at(Square { row: 3, col: 4 });
        target.flags.insert("capture".into(), json!(true));
        assert!(!v7_three_move_allowed(&state, &piece, from, &target).unwrap());
        state
            .extra
            .insert("socialism".into(), json!({"white":1,"black":0}));
        assert!(v7_three_move_allowed(&state, &piece, from, &target).unwrap());
        state
            .extra
            .insert("socialism".into(), json!({"white":0,"black":0}));
        target
            .flags
            .insert("crownGroundCapture".into(), json!(true));
        assert!(v7_three_move_allowed(&state, &piece, from, &target).unwrap());
    }

    #[test]
    fn frozen_geometry_queries_when_receipt_is_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_RULE_GEOMETRY_CASES") else {
            eprintln!(
                "source geometry receipt was not supplied; differential cases were not executed"
            );
            return;
        };
        let input = std::fs::read_to_string(path).expect("source geometry receipts");
        let mut count = 0;
        for line in input.lines().filter(|line| !line.trim().is_empty()) {
            let receipt: Value = serde_json::from_str(line).expect("source geometry receipt JSON");
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            let mut state: GameState =
                serde_json::from_value(receipt["before"]["state"].clone()).expect("source state");
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng =
                serde_json::from_value(receipt["before"]["rng"].clone()).expect("source RNG");
            let from: Square =
                serde_json::from_value(receipt["from"].clone()).expect("source origin");
            let piece = state.at(from).expect("source selected piece");
            let before = serde_jcs::to_vec(&state).unwrap();
            let target = || {
                serde_json::from_value::<MoveTarget>(receipt["target"].clone())
                    .expect("source target")
            };
            let result = match receipt["query"].as_str().expect("source query kind") {
                "modifiers" => {
                    let base: Vec<MoveTarget> = serde_json::from_value(receipt["base"].clone())
                        .expect("source input moves");
                    serde_json::to_value(
                        v7_apply_population_movement_modifiers(&state, piece, from, base, None)
                            .unwrap(),
                    )
                    .unwrap()
                }
                "legalMoves" => serde_json::to_value(
                    crate::movement::v7_legal_move_targets(
                        &state,
                        piece,
                        from,
                        crate::movement::V7MoveOptions::default(),
                    )
                    .unwrap(),
                )
                .unwrap(),
                "threeMoveAllowed" => {
                    json!(v7_three_move_allowed(&state, piece, from, &target()).unwrap())
                }
                "highGroundAllowed" => {
                    json!(v7_high_ground_capture_allowed(&state, piece, from, &target()).unwrap())
                }
                "finalGeometryAllowed" => {
                    json!(v7_final_geometry_move_allowed(&state, piece, from, &target()).unwrap())
                }
                "chainDestinationAllowed" => {
                    json!(chain_destination_allowed(&state, piece, target().square()).unwrap())
                }
                query => panic!("unknown geometry query {query}"),
            };
            assert_eq!(
                result, receipt["result"],
                "geometry receipt {}",
                receipt["case"]
            );
            assert_eq!(
                serde_jcs::to_vec(&state).unwrap(),
                before,
                "geometry query mutated state {}",
                receipt["case"]
            );
            assert_eq!(
                receipt["after"]["rng"], receipt["before"]["rng"],
                "source query consumed RNG {}",
                receipt["case"]
            );
            count += 1;
        }
        assert!(count > 0, "source geometry receipts must contain cases");
    }
}
