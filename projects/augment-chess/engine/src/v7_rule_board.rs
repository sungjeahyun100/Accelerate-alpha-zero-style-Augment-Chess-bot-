//! Direct board effects of the source-pinned v7 opening RULE cards.
//!
//! Source: main-OahWs0tU.js, SHA-256 e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c.
//! The caller owns RULE selection, application notices, notation, replay, and
//! the common applyCard reconciliation/settlement. This module owns only the
//! direct effect and its random stream. Failed effects never publish a partial
//! board or consume the caller's RNG.

use crate::{
    Color, EngineError, Fields, GameState, Piece, PieceColor, RULES_VERSION_V7, Result, Square,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

pub(crate) fn apply(state: &mut GameState, card_id: &str) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "opening RULE {card_id} requires frozen v7"
        )));
    }
    let mut next = state.clone();
    match card_id {
        "monochrome-chess" => monochrome(&mut next)?,
        "diagonal-chess" => diagonal(&mut next)?,
        "chess-n-pow-30" => chaos(&mut next)?,
        "chess-344200" => chess_344200(&mut next)?,
        "chess-960" => chess_960(&mut next)?,
        "football" | "monster" => install_center_piece(&mut next, card_id)?,
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "opening RULE board effect {card_id}"
            )));
        }
    }
    *state = next;
    Ok(())
}

fn square_name(row: usize, col: usize) -> String {
    format!("{}{}", char::from(b'a' + col as u8), 8 - row)
}

fn shade(row: usize, col: usize) -> &'static str {
    if (row + col).is_multiple_of(2) {
        "light"
    } else {
        "dark"
    }
}

fn ensure_eight(state: &GameState) -> Result<()> {
    if state.board.len() == 8 && state.board.iter().all(|row| row.len() == 8) {
        Ok(())
    } else {
        Err(EngineError::IllegalAction)
    }
}

fn source_index(state: &mut GameState, len: usize) -> Result<usize> {
    // Source randomChoice samples even for an empty array. All callers check
    // the length before indexing, while preserving that one draw.
    let draw = if len == 0 {
        state
            .rng
            .sample_invariant("source empty board RULE candidate")?
    } else {
        state.rng.sample()?
    };
    if !draw.is_finite() || !(0.0..1.0).contains(&draw) {
        return Err(EngineError::InvalidState(
            "board RULE random draw outside [0,1)".into(),
        ));
    }
    if len > 0 {
        state
            .rng
            .record_last_probability(1.0 / len as f64, "source board RULE candidate index")?;
    }
    Ok((draw * len as f64).floor() as usize)
}

fn shuffle<T>(state: &mut GameState, values: &mut [T]) -> Result<()> {
    for index in (1..values.len()).rev() {
        let other = source_index(state, index + 1)?;
        values.swap(index, other);
    }
    Ok(())
}

fn spawn(state: &mut GameState, color: Color, kind: &str, row: usize, col: usize) -> Result<Piece> {
    let mut piece = crate::opening::spawn(state, color, kind)?;
    piece
        .extra
        .insert("origin".into(), json!(square_name(row, col)));
    Ok(piece)
}

fn mono_enabled(state: &GameState) -> bool {
    state.extra.get("monochromeChess").and_then(Value::as_bool) == Some(true)
}

// main:101187-203. The source uses shared object references for a large
// piece; this DTO stores repeated cells, so update every cell of an identity.
fn monochrome(state: &mut GameState) -> Result<()> {
    ensure_eight(state)?;
    state.extra.insert("monochromeChess".into(), json!(true));
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let Some(original) = state.board[row][col].as_ref() else {
                continue;
            };
            if original.kind == "wall" || !seen.insert(original.id.clone()) {
                continue;
            }
            let id = original.id.clone();
            let mut transformed = original.clone();
            transformed
                .extra
                .insert("monoShade".into(), json!(shade(row, col)));
            if transformed.kind == "knight" {
                transformed.kind = "camel".into();
                transformed.moved = true;
                transformed.extra.shift_remove("vipInvitation");
                transformed.extra.shift_remove("holdoutPromotion");
                transformed
                    .extra
                    .insert("origin".into(), json!(square_name(row, col)));
                if let Some(owner) = transformed.color.owner() {
                    transformed.extra.insert(
                        "freshNoCaptureUntil".into(),
                        json!(state.turns_taken.get(owner) + 1),
                    );
                }
            }
            for cell in state.board.iter_mut().flatten() {
                if cell.as_ref().is_some_and(|piece| piece.id == id) {
                    *cell = Some(transformed.clone());
                }
            }
        }
    }
    Ok(())
}

// main:2230-2261 and 101204-219. Placement order is identity-draw order.
const DIAGONAL_PLACEMENTS: &[(Color, &str, &str)] = &[
    (Color::White, "pawn", "a1"),
    (Color::White, "king", "b1"),
    (Color::White, "bishop", "c1"),
    (Color::White, "rook", "d1"),
    (Color::White, "pawn", "e1"),
    (Color::White, "queen", "a2"),
    (Color::White, "pawn", "b2"),
    (Color::White, "knight", "c2"),
    (Color::White, "pawn", "d2"),
    (Color::White, "bishop", "a3"),
    (Color::White, "knight", "b3"),
    (Color::White, "pawn", "c3"),
    (Color::White, "rook", "a4"),
    (Color::White, "pawn", "b4"),
    (Color::White, "pawn", "a5"),
    (Color::Black, "pawn", "h8"),
    (Color::Black, "queen", "g8"),
    (Color::Black, "bishop", "f8"),
    (Color::Black, "rook", "e8"),
    (Color::Black, "pawn", "d8"),
    (Color::Black, "king", "h7"),
    (Color::Black, "pawn", "g7"),
    (Color::Black, "knight", "f7"),
    (Color::Black, "pawn", "e7"),
    (Color::Black, "bishop", "h6"),
    (Color::Black, "knight", "g6"),
    (Color::Black, "pawn", "f6"),
    (Color::Black, "rook", "h5"),
    (Color::Black, "pawn", "g5"),
    (Color::Black, "pawn", "h4"),
];

fn diagonal(state: &mut GameState) -> Result<()> {
    ensure_eight(state)?;
    let mut board = vec![vec![None; 8]; 8];
    for &(color, kind, name) in DIAGONAL_PLACEMENTS {
        let bytes = name.as_bytes();
        let row = usize::from(b'8' - bytes[1]);
        let col = usize::from(bytes[0] - b'a');
        let kind = if kind == "knight" && mono_enabled(state) {
            "camel"
        } else {
            kind
        };
        let mut piece = spawn(state, color, kind, row, col)?;
        if mono_enabled(state) {
            piece
                .extra
                .insert("monoShade".into(), json!(shade(row, col)));
        }
        board[row][col] = Some(piece);
    }
    state.board = board;
    state.extra.insert("selected".into(), Value::Null);
    state.extra.insert("legalMoves".into(), json!([]));
    state.en_passant = None;
    Ok(())
}

// main:101003-150. The 160-attempt loop and the visual-change retry are
// observable random-stream behavior, even when equivalent pieces exchange IDs.
fn home_entries(state: &GameState, color: Color, row: usize) -> Vec<(usize, Piece)> {
    let mut seen = BTreeSet::new();
    state.board[row]
        .iter()
        .enumerate()
        .filter_map(|(col, slot)| {
            let piece = slot.as_ref()?;
            if piece.color != color
                || piece.kind == "pawn"
                || piece.kind == "wall"
                || piece.is_large()
                || !seen.insert(piece.id.clone())
            {
                return None;
            }
            Some((col, piece.clone()))
        })
        .collect()
}

fn piece_counts(values: &[Piece]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for piece in values {
        *counts.entry(piece.kind.clone()).or_insert(0) += 1;
    }
    counts
}

fn visual_key(piece: Option<&Piece>) -> String {
    let Some(piece) = piece else {
        return String::new();
    };
    format!(
        "{}:{}:{}:{}",
        piece.color.as_str(),
        piece.kind,
        piece
            .extra
            .get("windmillMode")
            .and_then(Value::as_str)
            .unwrap_or(""),
        piece
            .extra
            .get("monoShade")
            .and_then(Value::as_str)
            .unwrap_or("")
    )
}

fn choose<T: Clone>(state: &mut GameState, values: &[T]) -> Result<Option<T>> {
    let index = source_index(state, values.len())?;
    Ok(values.get(index).cloned())
}

fn take_type(
    remaining: &mut Vec<String>,
    layout: &mut [Option<String>],
    index: usize,
    kind: &str,
) -> bool {
    if layout[index].is_some() {
        return false;
    }
    let Some(position) = remaining.iter().position(|candidate| candidate == kind) else {
        return false;
    };
    layout[index] = Some(remaining.remove(position));
    true
}

fn build_960_layout(
    state: &mut GameState,
    types: &[String],
    cols: &[usize],
    row: usize,
) -> Result<Option<Vec<String>>> {
    let mut layout = vec![None; types.len()];
    let mut remaining = types.to_vec();
    if remaining
        .iter()
        .filter(|kind| kind.as_str() == "bishop")
        .count()
        >= 2
    {
        let first_shade = if state.rng.sample()? < 0.5 { 0 } else { 1 };
        state
            .rng
            .record_last_probability(0.5, "source Chess960 first bishop shade")?;
        let first_candidates = cols
            .iter()
            .enumerate()
            .filter_map(|(index, &col)| ((row + col) % 2 == first_shade).then_some(index))
            .collect::<Vec<_>>();
        let second_candidates = cols
            .iter()
            .enumerate()
            .filter_map(|(index, &col)| ((row + col) % 2 != first_shade).then_some(index))
            .collect::<Vec<_>>();
        let first = choose(state, &first_candidates)?;
        let second = choose(state, &second_candidates)?;
        let (Some(first), Some(second)) = (first, second) else {
            return Ok(None);
        };
        if !take_type(&mut remaining, &mut layout, first, "bishop")
            || !take_type(&mut remaining, &mut layout, second, "bishop")
        {
            return Ok(None);
        }
    }
    if remaining.iter().any(|kind| kind == "king")
        && remaining
            .iter()
            .filter(|kind| kind.as_str() == "rook")
            .count()
            >= 2
    {
        let open = layout
            .iter()
            .enumerate()
            .filter_map(|(index, kind)| kind.is_none().then_some(index))
            .collect::<Vec<_>>();
        let king_candidates = open
            .iter()
            .copied()
            .filter(|&index| {
                open.iter().any(|&other| other < index) && open.iter().any(|&other| other > index)
            })
            .collect::<Vec<_>>();
        let Some(king) = choose(state, &king_candidates)? else {
            return Ok(None);
        };
        let left_candidates = open
            .iter()
            .copied()
            .filter(|&index| index < king)
            .collect::<Vec<_>>();
        let right_candidates = open
            .iter()
            .copied()
            .filter(|&index| index > king)
            .collect::<Vec<_>>();
        let left = choose(state, &left_candidates)?;
        let right = choose(state, &right_candidates)?;
        let (Some(left), Some(right)) = (left, right) else {
            return Ok(None);
        };
        if !take_type(&mut remaining, &mut layout, left, "rook")
            || !take_type(&mut remaining, &mut layout, king, "king")
            || !take_type(&mut remaining, &mut layout, right, "rook")
        {
            return Ok(None);
        }
    }
    shuffle(state, &mut remaining)?;
    let mut shuffled = remaining.into_iter();
    for slot in &mut layout {
        if slot.is_none() {
            *slot = shuffled.next();
        }
    }
    Ok(layout.into_iter().collect())
}

fn generate_960_layout(
    state: &mut GameState,
    types: &[String],
    cols: &[usize],
    row: usize,
) -> Result<Option<Vec<String>>> {
    let mut fallback = None;
    for _ in 0..160 {
        let Some(layout) = build_960_layout(state, types, cols, row)? else {
            continue;
        };
        if fallback.is_none() {
            fallback = Some(layout.clone());
        }
        if layout != types {
            return Ok(Some(layout));
        }
    }
    Ok(fallback)
}

fn place_960_layout(state: &mut GameState, cols: &[usize], layout: &[String]) -> Result<()> {
    let mut white = BTreeMap::<String, Vec<Piece>>::new();
    let mut black = BTreeMap::<String, Vec<Piece>>::new();
    for &col in cols {
        let first = state.board[7][col]
            .take()
            .ok_or_else(|| EngineError::InvalidState("960 white piece disappeared".into()))?;
        let second = state.board[0][col]
            .take()
            .ok_or_else(|| EngineError::InvalidState("960 black piece disappeared".into()))?;
        white.entry(first.kind.clone()).or_default().push(first);
        black.entry(second.kind.clone()).or_default().push(second);
    }
    for (index, kind) in layout.iter().enumerate() {
        let col = cols[index];
        for (row, groups) in [(7, &mut white), (0, &mut black)] {
            let mut piece = groups
                .get_mut(kind)
                .and_then(|group| (!group.is_empty()).then(|| group.remove(0)))
                .ok_or_else(|| EngineError::InvalidState("960 type inventory changed".into()))?;
            piece
                .extra
                .insert("origin".into(), json!(square_name(row, col)));
            state.board[row][col] = Some(piece);
        }
    }
    Ok(())
}

fn chess_960(state: &mut GameState) -> Result<()> {
    ensure_eight(state)?;
    let white = home_entries(state, Color::White, 7);
    let black = home_entries(state, Color::Black, 0);
    let black_cols = black.iter().map(|(col, _)| *col).collect::<BTreeSet<_>>();
    let cols = white
        .iter()
        .map(|(col, _)| *col)
        .filter(|col| black_cols.contains(col))
        .collect::<Vec<_>>();
    let white_pieces = cols
        .iter()
        .map(|col| {
            white
                .iter()
                .find(|(candidate, _)| candidate == col)
                .unwrap()
                .1
                .clone()
        })
        .collect::<Vec<_>>();
    let black_pieces = cols
        .iter()
        .map(|col| {
            black
                .iter()
                .find(|(candidate, _)| candidate == col)
                .unwrap()
                .1
                .clone()
        })
        .collect::<Vec<_>>();
    if cols.len() < 3
        || cols.len() != white.len()
        || cols.len() != black.len()
        || piece_counts(&white_pieces) != piece_counts(&black_pieces)
    {
        return Err(EngineError::IllegalAction);
    }
    let original_types = white_pieces
        .iter()
        .map(|piece| piece.kind.clone())
        .collect::<Vec<_>>();
    let Some(layout) = generate_960_layout(state, &original_types, &cols, 7)? else {
        return Err(EngineError::IllegalAction);
    };
    let before = cols
        .iter()
        .flat_map(|&col| [(7, col), (0, col)])
        .map(|(row, col)| visual_key(state.board[row][col].as_ref()))
        .collect::<Vec<_>>();
    place_960_layout(state, &cols, &layout)?;
    let after = cols
        .iter()
        .flat_map(|&col| [(7, col), (0, col)])
        .map(|(row, col)| visual_key(state.board[row][col].as_ref()))
        .collect::<Vec<_>>();
    if before == after
        && let Some(retry) = generate_960_layout(state, &layout, &cols, 7)?
        && retry != layout
    {
        place_960_layout(state, &cols, &retry)?;
    }
    Ok(())
}

// main:101251-295, 109954-961. Empty-cell selection shuffles the entire
// candidate list on every placement; selecting a single random index would
// change all later identities and layouts.
fn random_empty_in_rows(state: &mut GameState, rows: &[usize]) -> Result<Option<(usize, usize)>> {
    let mut empty = rows
        .iter()
        .flat_map(|&row| (0..8).map(move |col| (row, col)))
        .filter(|&(row, col)| state.board[row][col].is_none())
        .collect::<Vec<_>>();
    shuffle(state, &mut empty)?;
    Ok(empty.into_iter().next())
}

fn clear_identity(state: &mut GameState, id: &str) {
    for slot in state.board.iter_mut().flatten() {
        if slot.as_ref().is_some_and(|piece| piece.id == id) {
            *slot = None;
        }
    }
}

fn zone_visual(state: &GameState, cells: &[(usize, usize)]) -> Vec<String> {
    cells
        .iter()
        .map(|&(row, col)| visual_key(state.board[row][col].as_ref()))
        .collect()
}

fn place_colossus(state: &mut GameState, mut piece: Piece, row: usize, col: usize) {
    piece.extra.insert("anchorRow".into(), json!(row));
    piece.extra.insert("anchorCol".into(), json!(col));
    if mono_enabled(state)
        && piece
            .extra
            .get("monoShade")
            .and_then(Value::as_str)
            .unwrap_or("")
            .is_empty()
    {
        piece
            .extra
            .insert("monoShade".into(), json!(shade(row, col)));
    }
    for r in row..=row + 1 {
        for c in col..=col + 1 {
            state.board[r][c] = Some(piece.clone());
        }
    }
}

fn force_visible_zone_swap(state: &mut GameState, cells: &[(usize, usize)], color: Color) {
    let entries = cells
        .iter()
        .filter_map(|&(row, col)| {
            let piece = state.board[row][col].as_ref()?;
            if piece.color != color
                || matches!(
                    piece.kind.as_str(),
                    "king" | "merchant" | "wall" | "colossus"
                )
            {
                return None;
            }
            Some((row, col, piece.clone()))
        })
        .collect::<Vec<_>>();
    for i in 0..entries.len() {
        for j in i + 1..entries.len() {
            if visual_key(Some(&entries[i].2)) == visual_key(Some(&entries[j].2)) {
                continue;
            }
            let (first_row, first_col, mut first) = entries[i].clone();
            let (second_row, second_col, mut second) = entries[j].clone();
            first
                .extra
                .insert("origin".into(), json!(square_name(second_row, second_col)));
            second
                .extra
                .insert("origin".into(), json!(square_name(first_row, first_col)));
            state.board[first_row][first_col] = Some(second);
            state.board[second_row][second_col] = Some(first);
            return;
        }
    }
}

fn chess_344200(state: &mut GameState) -> Result<()> {
    ensure_eight(state)?;
    for color in [Color::White, Color::Black] {
        let (zone_rows, home_row) = if color == Color::White {
            ([6, 7], 7)
        } else {
            ([0, 1], 0)
        };
        let zone_cells = zone_rows
            .iter()
            .flat_map(|&row| (0..8).map(move |col| (row, col)))
            .collect::<Vec<_>>();
        let before = zone_visual(state, &zone_cells);
        let mut seen = BTreeSet::new();
        let mut pieces = Vec::new();
        for row in 0..8 {
            for col in 0..8 {
                if let Some(piece) = state.board[row][col].as_ref()
                    && piece.color == color
                    && zone_rows.contains(&row)
                    && seen.insert(piece.id.clone())
                {
                    pieces.push(piece.clone());
                }
            }
        }
        for piece in &pieces {
            clear_identity(state, &piece.id);
        }
        for piece in pieces
            .iter()
            .filter(|piece| matches!(piece.kind.as_str(), "king" | "merchant"))
        {
            if let Some((row, col)) = random_empty_in_rows(state, &[home_row])? {
                let mut piece = piece.clone();
                piece
                    .extra
                    .insert("origin".into(), json!(square_name(row, col)));
                state.board[row][col] = Some(piece);
            }
        }
        let mut others = pieces
            .into_iter()
            .filter(|piece| !matches!(piece.kind.as_str(), "king" | "merchant"))
            .collect::<Vec<_>>();
        shuffle(state, &mut others)?;
        for piece in others {
            if piece.kind == "colossus" {
                let mut anchors = (0..7)
                    .flat_map(|row| (0..7).map(move |col| (row, col)))
                    .collect::<Vec<_>>();
                shuffle(state, &mut anchors)?;
                if let Some((row, col)) = anchors.into_iter().find(|&(row, col)| {
                    [row, row + 1].iter().all(|r| zone_rows.contains(r))
                        && [row, row + 1]
                            .iter()
                            .all(|&r| [col, col + 1].iter().all(|&c| state.board[r][c].is_none()))
                }) {
                    place_colossus(state, piece, row, col);
                }
            } else if let Some((row, col)) = random_empty_in_rows(state, &zone_rows)? {
                let mut piece = piece;
                piece
                    .extra
                    .insert("origin".into(), json!(square_name(row, col)));
                state.board[row][col] = Some(piece);
            }
        }
        if before == zone_visual(state, &zone_cells) {
            force_visible_zone_swap(state, &zone_cells, color);
        }
    }
    Ok(())
}

fn open_installation_cell(state: &GameState, row: usize, col: usize) -> Result<bool> {
    crate::movement::open_installation(
        state,
        Square {
            row: row as u8,
            col: col as u8,
        },
        state.turn,
    )
}

/// Source main68029. The caller owns the surrounding rule transaction.
/// Installation crushes are removals rather than captures: they do not add
/// captured types, clear prophecy, or invoke Calling Card or Reaper reactions.
pub(super) fn crush_concealed_installation_occupant(
    state: &mut GameState,
    square: Square,
    owner: Color,
) -> Result<bool> {
    let Some(piece) = state.at(square).cloned().filter(|piece| {
        piece.color == owner.opponent()
            && crate::observation::piece_hidden_from_v7(state, piece, square) == Some(owner)
    }) else {
        return Ok(false);
    };
    crate::card_effects::mark_vanish_animation(state, &piece, square)?;
    crate::transition::remove_piece_from_board_cells(state, &piece, square)?;
    crate::transition::grant_vigilance_protection(state, &piece)?;
    crate::flow::mark_progress(state);
    crate::v7_threat::mark_king_threat_removal_cause(
        state,
        &piece,
        square,
        &json!({"label":"설치물"}),
        state.threat_probe_depth > 0,
    )?;
    crate::transition::resolve_royal_capture(state, &piece, owner)?;
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(true)
}

fn neutral_piece(state: &mut GameState, kind: &str, row: usize, col: usize) -> Result<Piece> {
    let suffix = crate::draft::random_suffix(
        state
            .rng
            .sample_opaque("source neutral rule piece identity")?,
    )?;
    let mut extra = Fields::new();
    extra.insert("shielded".into(), json!(false));
    extra.insert("origin".into(), json!(square_name(row, col)));
    Ok(Piece {
        kind: kind.into(),
        color: PieceColor::Neutral,
        moved: true,
        id: format!("neutral-{kind}-{suffix}"),
        extra,
        source_order: ["color", "type", "moved", "shielded", "id"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    })
}

fn force_animate(state: &mut GameState, id: &str) -> Result<()> {
    let entry = state
        .extra
        .entry("forceAnimatedPieceIds")
        .or_insert_with(|| json!({"__simType":"Set","values":[]}));
    if entry.get("__simType").and_then(Value::as_str) != Some("Set") {
        return Err(EngineError::InvalidState(
            "forceAnimatedPieceIds must be a Set".into(),
        ));
    }
    let values = entry
        .get_mut("values")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            EngineError::InvalidState("forceAnimatedPieceIds.values must be an array".into())
        })?;
    if !values.iter().any(|value| value.as_str() == Some(id)) {
        values.push(json!(id));
    }
    Ok(())
}

// main:101325-360 and 101588-606. The 2x2 Fisher-Yates draw happens even
// when no central cell can be used; fallback consumes one tie draw per open
// square before stable distance/tie sorting.
fn install_center_piece(state: &mut GameState, card_id: &str) -> Result<()> {
    ensure_eight(state)?;
    let mut centers = vec![(3, 3), (3, 4), (4, 3), (4, 4)];
    shuffle(state, &mut centers)?;
    let mut selected = None;
    for &(row, col) in &centers {
        if open_installation_cell(state, row, col)? {
            selected = Some((row, col));
            break;
        }
    }
    if selected.is_none() {
        let rank_start_cursor = state.rng.cursor;
        let mut options = Vec::new();
        for row in 0..8 {
            for col in 0..8 {
                if !open_installation_cell(state, row, col)? {
                    continue;
                }
                let distance = ((row as f64 - 3.5).abs() + (col as f64 - 3.5).abs()) as u8;
                options.push((row, col, distance, state.rng.sample()?));
            }
        }
        options.sort_by(|left, right| {
            left.2
                .cmp(&right.2)
                .then_with(|| left.3.total_cmp(&right.3))
        });
        if let Some(first) = options.first() {
            let nearest_count = options.iter().filter(|entry| entry.2 == first.2).count();
            // main:101601의 tie keys는 winner 이외에 저장·재사용되지 않는다.
            // IID 연속 source prior에서 최근접 m칸의 winner만 marginalize한다.
            // 유한 LCG seed posterior 질량을 뜻하지 않으며 모든 실제 key 소비는 유지한다.
            state.rng.record_group_probability(
                rank_start_cursor,
                1.0 / nearest_count as f64,
                "source nearest center fallback winner",
            )?;
        }
        selected = options.first().map(|&(row, col, _, _)| (row, col));
    }
    let Some((row, col)) = selected else {
        return Err(EngineError::IllegalAction);
    };
    crush_concealed_installation_occupant(
        state,
        Square {
            row: row as u8,
            col: col as u8,
        },
        state.turn,
    )?;
    let kind = if card_id == "football" {
        "football"
    } else {
        "monster"
    };
    let piece = neutral_piece(state, kind, row, col)?;
    force_animate(state, &piece.id)?;
    state.board[row][col] = Some(piece);
    Ok(())
}

// main:1443-1504 and 1514-1582. This is Object.keys order, which controls
// every DP summation and weighted draw. Values are frozen source constants.
const CHAOS_TYPES: &[(&str, u8)] = &[
    ("queen", 9),
    ("rook", 5),
    ("bishop", 3),
    ("missionary", 2),
    ("knight", 3),
    ("pawn", 1),
    ("protestant", 3),
    ("herald", 5),
    ("cannon", 4),
    ("fanatic", 1),
    ("primeMinister", 9),
    ("eagle", 2),
    ("amazon", 13),
    ("cardinal", 7),
    ("pegasus", 5),
    ("jester", 9),
    ("camel", 2),
    ("log", 2),
    ("hook", 15),
    ("grasshopper", 4),
    ("dragon", 5),
    ("man", 4),
    ("assassin", 4),
    ("reaper", 9),
    ("knightmaster", 3),
    ("standardBearer", 2),
    ("guard", 2),
    ("recruiter", 9),
    ("squire", 1),
    ("checker", 1),
    ("checkerKing", 2),
    ("wizard", 9),
    ("alfil", 1),
    ("windmill", 4),
    ("idol", 9),
    ("lobster", 2),
    ("babyBear", 4),
    ("bear", 17),
    ("siegeRam", 5),
    ("magicGirl", 6),
    ("berserker", 7),
    ("slime", 5),
    ("siren", 9),
    ("trickster", 5),
    ("undead", 4),
    ("campfire", 4),
    ("hedgehog", 9),
    ("princess", 6),
    ("thief", 9),
    ("paladin", 4),
    ("octopus", 5),
    ("parrot", 5),
    ("clockwork", 5),
    ("brutus", 10),
    ("grappler", 13),
    ("revolvingDoor", 5),
    ("donQuixote", 7),
    ("medium", 3),
];
const RANGED_CHAOS_TYPES: &[&str] = &[
    "brutus",
    "clockwork",
    "rook",
    "bishop",
    "queen",
    "bear",
    "amazon",
    "cardinal",
    "cannon",
    "herald",
    "hook",
    "protestant",
    "windmill",
    "jester",
    "idol",
];
const CHAOS_SQUARES: [[(usize, usize); 15]; 2] = [
    [
        (7, 0),
        (7, 1),
        (7, 2),
        (7, 3),
        (7, 5),
        (7, 6),
        (7, 7),
        (6, 0),
        (6, 1),
        (6, 2),
        (6, 3),
        (6, 4),
        (6, 5),
        (6, 6),
        (6, 7),
    ],
    [
        (0, 0),
        (0, 1),
        (0, 2),
        (0, 3),
        (0, 5),
        (0, 6),
        (0, 7),
        (1, 0),
        (1, 1),
        (1, 2),
        (1, 3),
        (1, 4),
        (1, 5),
        (1, 6),
        (1, 7),
    ],
];

fn chaos_ways() -> &'static Vec<Vec<f64>> {
    static WAYS: OnceLock<Vec<Vec<f64>>> = OnceLock::new();
    WAYS.get_or_init(|| {
        let mut ways = vec![vec![0.0; 71]; 16];
        ways[0][0] = 1.0;
        for slots in 1..=15 {
            for total in 0..=70 {
                let mut sum = 0.0;
                for &(_, value) in CHAOS_TYPES {
                    if usize::from(value) <= total {
                        sum += ways[slots - 1][total - usize::from(value)];
                    }
                }
                ways[slots][total] = sum;
            }
        }
        ways
    })
}

fn chaos_lineup(
    state: &mut GameState,
    total: usize,
    ways: &[Vec<f64>],
) -> Result<Option<Vec<&'static str>>> {
    if !(39..=70).contains(&total) || ways[15][total] <= 0.0 {
        return Ok(None);
    }
    let mut lineup = Vec::with_capacity(15);
    let mut remaining = total;
    for slots in (1..=15).rev() {
        let mut total_weight = 0.0;
        let mut options = Vec::new();
        for &(kind, value) in CHAOS_TYPES {
            if usize::from(value) <= remaining {
                let weight = ways[slots - 1][remaining - usize::from(value)];
                if weight > 0.0 {
                    options.push((kind, value, weight));
                    total_weight += weight;
                }
            }
        }
        if options.is_empty() || total_weight <= 0.0 {
            return Ok(None);
        }
        let mut threshold = state.rng.sample()? * total_weight;
        let mut chosen = *options.last().expect("nonempty options");
        for option in options {
            if threshold < option.2 {
                chosen = option;
                break;
            }
            threshold -= option.2;
        }
        state.rng.record_last_probability(
            chosen.2 / total_weight,
            "source ChessN30 conditional lineup type",
        )?;
        lineup.push(chosen.0);
        remaining -= usize::from(chosen.1);
    }
    Ok((remaining == 0).then_some(lineup))
}

fn chaos_home_weight(kind: &str) -> f64 {
    let value = CHAOS_TYPES
        .iter()
        .find(|&&(name, _)| name == kind)
        .expect("lineup type")
        .1;
    0.5 + if RANGED_CHAOS_TYPES.contains(&kind) {
        2.0
    } else {
        0.0
    } + f64::from(value) * 0.1
}

fn arrange_chaos(
    state: &mut GameState,
    lineup: Vec<&'static str>,
) -> Result<Option<Vec<&'static str>>> {
    if lineup.len() != 15 {
        return Ok(None);
    }
    let mut remaining = lineup
        .into_iter()
        .map(|kind| (kind, chaos_home_weight(kind)))
        .collect::<Vec<_>>();
    let mut home = Vec::with_capacity(7);
    while home.len() < 7 {
        let total = remaining.iter().map(|entry| entry.1).sum::<f64>();
        let mut threshold = state.rng.sample()? * total;
        let mut selected = remaining.len() - 1;
        for (index, entry) in remaining.iter().enumerate() {
            if threshold < entry.1 {
                selected = index;
                break;
            }
            threshold -= entry.1;
        }
        state.rng.record_last_probability(
            remaining[selected].1 / total,
            "source ChessN30 weighted home entry",
        )?;
        home.push(remaining.remove(selected).0);
    }
    let mut tail = remaining
        .into_iter()
        .map(|entry| entry.0)
        .collect::<Vec<_>>();
    shuffle(state, &mut home)?;
    shuffle(state, &mut tail)?;
    home.extend(tail);
    for forbidden in [3, 4, 11] {
        if home[forbidden] != "brutus" {
            continue;
        }
        let candidates = home
            .iter()
            .enumerate()
            .filter_map(|(index, kind)| {
                (![3, 4, 11].contains(&index) && *kind != "brutus").then_some(index)
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(None);
        }
        let replacement = source_index(state, candidates.len())?;
        home.swap(forbidden, candidates[replacement]);
    }
    Ok(Some(home))
}

fn generate_chaos(state: &mut GameState) -> Result<Option<[Vec<&'static str>; 2]>> {
    // Pinned catalog profile uses the current 58-type pool. A different
    // catalog hash selects historical pools in source; do not guess it here.
    let hash = state
        .extra
        .get("profile")
        .and_then(|profile| profile.get("catalogHash"))
        .and_then(Value::as_str);
    if hash.is_some_and(|hash| {
        !matches!(
            hash,
            "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4"
                | "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI"
        )
    }) || hash.is_none() && state.extra.get("revolvingDoorGuard") == Some(&json!(false))
    {
        return Err(EngineError::UnsupportedFeature(
            "historical chaos pool".into(),
        ));
    }
    let ways = chaos_ways();
    let feasible = (39..=70)
        .filter(|&total| ways[15][total] > 0.0)
        .collect::<Vec<_>>();
    let index = source_index(state, feasible.len())?;
    let total = feasible.get(index).copied().unwrap_or(39);
    let Some(white) = chaos_lineup(state, total, ways)? else {
        return Ok(None);
    };
    let Some(mut black) = chaos_lineup(state, total, ways)? else {
        return Ok(None);
    };
    for _ in 0..8 {
        if white != black {
            break;
        }
        let Some(retry) = chaos_lineup(state, total, ways)? else {
            return Ok(None);
        };
        black = retry;
    }
    let Some(white) = arrange_chaos(state, white)? else {
        return Ok(None);
    };
    let Some(black) = arrange_chaos(state, black)? else {
        return Ok(None);
    };
    Ok(Some([white, black]))
}

fn apply_roulette_defaults(
    state: &mut GameState,
    piece: &mut Piece,
    kind: &str,
    shared_turn: u32,
) -> Result<()> {
    match kind {
        "thief" => {
            piece.extra.insert("submerged".into(), json!(true));
        }
        "colossus" => {
            piece.extra.insert("hp".into(), json!(3));
            piece.extra.insert("maxHp".into(), json!(3));
        }
        "bigRook" | "bigBishop" => {
            piece.extra.insert("hp".into(), json!(2));
            piece.extra.insert("maxHp".into(), json!(2));
        }
        "wizard" => {
            piece.extra.insert("mana".into(), json!(0));
            piece.extra.insert("maxMana".into(), json!(5));
        }
        "windmill" => {
            piece.extra.insert("windmillMode".into(), json!("bishop"));
        }
        "log" => {
            piece.extra.insert("logDir".into(), Value::Null);
        }
        "babyBear" => {
            piece
                .extra
                .insert("babyBearGrowAtTurn".into(), json!(shared_turn + 7));
        }
        "bear" | "hedgehog" => {
            piece
                .extra
                .insert("bearRetaliationsRemaining".into(), json!(2));
        }
        _ => {}
    }
    if kind == "trickster" {
        super::trickster_defaults(state, piece)?;
    }
    Ok(())
}

// main:101220-249. The dynamic-programming ways, lineup selection, weighted
// home-rank selection, both shuffles, and trickster ability each consume the
// exact source draw sequence before the new board is published.
fn chaos(state: &mut GameState) -> Result<()> {
    ensure_eight(state)?;
    let Some([white, black]) = generate_chaos(state)? else {
        return Err(EngineError::IllegalAction);
    };
    let mut board = vec![vec![None; 8]; 8];
    let shared_turn = *[state.turns_taken.white, state.turns_taken.black]
        .iter()
        .min()
        .unwrap();
    for (side, color, king_row, lineup) in
        [(0, Color::White, 7, white), (1, Color::Black, 0, black)]
    {
        board[king_row][4] = Some(spawn(state, color, "king", king_row, 4)?);
        for (index, kind) in lineup.iter().enumerate() {
            let (row, col) = CHAOS_SQUARES[side][index];
            let mut piece = spawn(state, color, kind, row, col)?;
            apply_roulette_defaults(state, &mut piece, kind, shared_turn)?;
            board[row][col] = Some(piece);
        }
    }
    state.board = board;
    state.extra.insert(
        "chaosNoCaptureUntilHalfTurn".into(),
        json!(state.turns_taken.white + state.turns_taken.black + 4),
    );
    state.extra.insert("selected".into(), Value::Null);
    state.extra.insert("legalMoves".into(), json!([]));
    state.en_passant = None;
    state.extra.insert("lastMove".into(), Value::Null);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn source_chance_center_fallback_marginalizes_only_rank_keys_and_preserves_rng() {
        // main:101601의 원문 key 3개를 모두 소비한다. 최근접 2칸의 winner
        // 질량1/2와 앞선 center4 순열1/24를 한 번씩 곱하며 먼 key는 marginalize한다.
        let mut plain = GameState::new(Default::default(), 19).unwrap();
        plain.ruleset_id = RULES_VERSION_V7.into();
        plain.rng = crate::RngState::seeded(19);
        for row in 0..8 {
            for col in 0..8 {
                plain.board[row][col] = Some(Piece::new(
                    "wall",
                    PieceColor::Neutral,
                    format!("wall-{row}-{col}"),
                ));
            }
        }
        for (row, col) in [(2, 3), (2, 4), (0, 0)] {
            plain.board[row][col] = None;
        }
        let mut traced = plain.clone();
        traced.rng.begin_source_trace().unwrap();
        install_center_piece(&mut plain, "football").unwrap();
        install_center_piece(&mut traced, "football").unwrap();
        let mass = traced.rng.finish_source_trace().unwrap();
        assert!((mass - 1.0 / 48.0).abs() < 1e-16);
        assert_eq!(traced.rng.cursor, 7);
        assert_eq!(traced, plain);
        assert!(traced.board[0][0].is_none());
    }

    #[test]
    fn diagonal_has_the_pinned_placements_and_exact_identity_draw_count() {
        let mut state = GameState::new(Default::default(), 17).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        let before = state.rng.cursor;
        apply(&mut state, "diagonal-chess").unwrap();
        assert_eq!(state.rng.cursor - before, DIAGONAL_PLACEMENTS.len());
        assert_eq!(
            state
                .board
                .iter()
                .flatten()
                .filter(|piece| piece.is_some())
                .count(),
            30
        );
        assert_eq!(state.board[7][1].as_ref().unwrap().kind, "king");
        assert_eq!(state.board[0][7].as_ref().unwrap().kind, "pawn");
        assert_eq!(state.extra["selected"], Value::Null);
    }

    #[test]
    fn unsupported_rule_preserves_board_and_rng() {
        let mut state = GameState::new(Default::default(), 9).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        let before = state.clone();
        assert!(matches!(
            apply(&mut state, "unknown"),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn seeded_opening_rules_match_frozen_client_board_and_rng() {
        // FrozenClientSource + OracleRuntime.newGame({draftDelete:true,
        // ruleCardIds:[id]}, seed), main-OahWs0tU.js exact SHA above. These
        // source results include the RULE wrapper and first position record.
        for (id, seed, cursor, rng_state, probes) in [
            (
                "monochrome-chess",
                19,
                35,
                2_324_936_856,
                vec![(0, 1, "camel"), (7, 6, "camel")],
            ),
            (
                "diagonal-chess",
                19,
                65,
                2_736_407_958,
                vec![(7, 1, "king"), (0, 7, "pawn")],
            ),
            (
                "chess-n-pow-30",
                0,
                138,
                1_179_547_034,
                vec![(0, 0, "hook"), (6, 0, "princess"), (7, 2, "idol")],
            ),
            (
                "chess-344200",
                19,
                287,
                3_215_866_404,
                vec![(0, 5, "king"), (6, 3, "queen"), (7, 2, "king")],
            ),
            (
                "chess-960",
                19,
                43,
                977_021_760,
                vec![(0, 0, "bishop"), (0, 5, "king"), (7, 1, "rook")],
            ),
            ("football", 19, 39, 202_246_476, vec![(4, 4, "football")]),
            ("monster", 19, 39, 202_246_476, vec![(4, 4, "monster")]),
        ] {
            let config = crate::GameConfig {
                draft_delete: true,
                rule_card_ids: vec![id.into()],
                ..Default::default()
            };
            let state = crate::draft::initialize_for_ruleset(config, seed, RULES_VERSION_V7)
                .unwrap_or_else(|error| panic!("{id}: {error}"));
            assert_eq!(state.extra["appliedRuleCard"]["id"], id, "{id}");
            assert_eq!(
                (state.rng.cursor, state.rng.state),
                (cursor, rng_state),
                "{id}"
            );
            for (row, col, kind) in probes {
                assert_eq!(
                    state.board[row][col]
                        .as_ref()
                        .map(|piece| piece.kind.as_str()),
                    Some(kind),
                    "{id} at {row},{col}"
                );
            }
            assert_eq!(state.extra["positionCounts"]["__simType"], "Map", "{id}");
            let expected_source_state_sha256 = match id {
                "monochrome-chess" => {
                    "1876954d8e2b88c9680afbea4b4aaed25bafa0a2b787f246f25a5dd40c2943e8"
                }
                "diagonal-chess" => {
                    "fd32054eb5511af9bc722d21e893af099ffd0d77aeb8e6ace1d4cf62e6b71ad1"
                }
                "chess-n-pow-30" => {
                    "72e5e57db2085f327dbd55335b3d7f6e7ffbbd69c8d05f97f71110c354807425"
                }
                "chess-344200" => {
                    "5bf7bd4530e996108acbe12cee7b400a4090d220d477a02fd80bf1f8fd1c13e7"
                }
                "chess-960" => "f837a13c1a8a2ad9e4966a6f7ab777922ace61a62b4df1a3ef6eb61105cf5a16",
                "football" => "51e759265f6130c91d16993a4b2c6bb2e9fd7ad1b94b7e09ecb82f013a47b373",
                "monster" => "5a8cb7dd09b8ee677c4ca02886c893165422f22cdf20249dba3b1550ca95e2f4",
                _ => unreachable!("fixed source fixture"),
            };
            let mut source_state = serde_json::to_value(&state).unwrap();
            let fields = source_state.as_object_mut().unwrap();
            // The native DTO keeps these Position-envelope fields internally;
            // the client owns its RNG and version outside state.
            for internal in ["rng", "rulesetId", "history"] {
                fields.remove(internal);
            }
            let canonical = serde_jcs::to_vec(&source_state).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(canonical)),
                expected_source_state_sha256,
                "{id} full source state"
            );
        }
    }
}
