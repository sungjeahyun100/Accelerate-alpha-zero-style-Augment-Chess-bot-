//! Opening formations and automatic acquisition effects. Piece creation shares
//! the source random stream; multi-cell pieces retain a single identity.
use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn name(square: Square) -> String {
    format!("{}{}", char::from(b'a' + square.col), 8 - square.row)
}
fn square(value: &str) -> Square {
    let bytes = value.as_bytes();
    Square {
        row: 8 - (bytes[1] - b'0'),
        col: bytes[0] - b'a',
    }
}
fn entries(state: &GameState, color: Color) -> Vec<(Square, Piece)> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let cell = Square { row, col };
            if let Some(piece) = state.at(cell)
                && piece.color == color
                && seen.insert(piece.id.clone())
            {
                result.push((cell, piece.clone()));
            }
        }
    }
    result
}
fn royal(state: &GameState, color: Color) -> Option<(Square, Piece)> {
    entries(state, color)
        .into_iter()
        .find(|(_, piece)| state.royal_identity(piece))
}
pub(crate) fn spawn(state: &mut GameState, color: Color, kind: &str) -> Result<Piece> {
    let id = format!(
        "{}-{kind}-{}",
        color.as_str(),
        crate::draft::random_suffix(state.rng.sample()?)?
    );
    let mut extra = Fields::new();
    extra.insert("shielded".into(), json!(false));
    if kind == "thief" {
        extra.insert("submerged".into(), json!(true));
    }
    let hp = match kind {
        "colossus" => Some(3),
        "bigRook" | "bigBishop" => Some(2),
        "shotgunKing" => Some(4),
        _ => None,
    };
    if let Some(hp) = hp {
        extra.insert("hp".into(), json!(hp));
        extra.insert("maxHp".into(), json!(hp));
    }
    if kind == "shotgunKing" {
        extra.insert(
            "facing".into(),
            json!(if color == Color::White { "up" } else { "down" }),
        );
        extra.insert("ammo".into(), json!(3));
        extra.insert("maxAmmo".into(), json!(3));
    }
    Ok(Piece {
        kind: kind.into(),
        color: color.into(),
        moved: false,
        id,
        extra,
        source_order: ["color", "type", "moved", "shielded", "id"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    })
}

fn place(state: &mut GameState, piece: Piece, cell: Square) {
    state.board[cell.row as usize][cell.col as usize] = Some(piece);
}
fn place_large(state: &mut GameState, mut piece: Piece, anchor: Square) {
    piece.extra.insert("anchorRow".into(), json!(anchor.row));
    piece.extra.insert("anchorCol".into(), json!(anchor.col));
    for row in anchor.row..=anchor.row + 1 {
        for col in anchor.col..=anchor.col + 1 {
            place(state, piece.clone(), Square { row, col });
        }
    }
}
fn cancel_prophecies(state: &mut GameState) {
    if let Some(prophecy) = state
        .extra
        .get_mut("prophecy")
        .and_then(Value::as_object_mut)
    {
        for color in [Color::White, Color::Black] {
            if prophecy.get(color.as_str()).is_some_and(|v| !v.is_null()) {
                prophecy.insert(color.as_str().into(), Value::Null);
            }
        }
    }
}
fn interaction_guard(state: &GameState) -> Result<()> {
    for field in [
        "monochromeChess",
        "democracy",
        "regency",
        "recurrence",
        "soulRequiem",
        "crownRule",
    ] {
        if state.flag(field, Color::White) || state.flag(field, Color::Black) {
            return Err(EngineError::UnsupportedFeature(format!(
                "opening interaction {field}"
            )));
        }
    }
    if state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|p| p.ability_kind() == "reaper")
    {
        return Err(EngineError::UnsupportedFeature(
            "opening removal reaper nearby deaths".into(),
        ));
    }
    Ok(())
}
fn environmental_defeats(
    state: &mut GameState,
    removed: &[(Piece, Color)],
    cause: &str,
) -> Result<()> {
    let mut defeats = Vec::new();
    for (piece, winner) in removed {
        let Some(color) = piece.color.owner() else {
            continue;
        };
        if piece.is_royal() {
            state.set_flag("kingDead", color, true);
        }
        if !piece.is_defeat_royal() {
            continue;
        }
        let label = if color == Color::White { "백" } else { "흑" };
        let kind = match piece.kind.as_str() {
            "vip" => "귀빈",
            "merchant" => "상인",
            _ => "킹",
        };
        defeats.push((
            color,
            *winner,
            format!("{label} {kind}이 {cause}에 휘말렸습니다."),
            piece.is_royal(),
        ));
    }
    if defeats.iter().any(|entry| entry.0 == Color::White)
        && defeats.iter().any(|entry| entry.0 == Color::Black)
    {
        let kings = defeats
            .iter()
            .filter(|entry| entry.3)
            .map(|entry| entry.0)
            .collect::<BTreeSet<_>>()
            .len()
            == 2;
        crate::flow::end_game(
            state,
            None,
            &format!(
                "{cause}으로 양쪽 {} 함께 {}.",
                if kings { "킹이" } else { "왕권이" },
                if kings {
                    "쓰러졌습니다"
                } else {
                    "무너졌습니다"
                }
            ),
        )?;
    } else if let Some((_, winner, reason, _)) = defeats.first() {
        crate::flow::end_game(state, Some(*winner), reason)?;
    }
    Ok(())
}
fn remove_collapsed(state: &mut GameState, color: Color) -> Result<()> {
    let removed = entries(state, color)
        .into_iter()
        .filter(|(cell, _)| crate::movement::collapsed(state, *cell))
        .map(|(_, piece)| piece)
        .collect::<Vec<_>>();
    if removed
        .iter()
        .any(|p| p.kind == "undead" || p.flag("shielded") || p.number("hp") > 1)
    {
        return Err(EngineError::UnsupportedFeature(
            "collapsed opening shield/damage/resurrection".into(),
        ));
    }
    let mut defeats = Vec::new();
    for piece in removed {
        crate::transition::clear_piece(state, &piece.id);
        state.captures.get_mut(color.opponent()).push(piece.clone());
        cancel_prophecies(state);
        defeats.push((piece, color.opponent()));
    }
    environmental_defeats(state, &defeats, "붕괴")
}

fn big(state: &mut GameState, color: Color, bishop: bool, horde: bool) -> Result<bool> {
    interaction_guard(state)?;
    let minor = if horde { "pawn" } else { "knight" };
    let (anchor, placements): (&str, [(&str, &str); 3]) = match (color, bishop) {
        (Color::White, false) => ("g2", [("f3", minor), ("g3", "pawn"), ("h3", "pawn")]),
        (Color::Black, false) => ("g8", [("f6", minor), ("g6", "pawn"), ("h6", "pawn")]),
        (Color::White, true) => ("f2", [("f3", "pawn"), ("g3", "pawn"), ("h3", minor)]),
        (Color::Black, true) => ("f8", [("f6", "pawn"), ("g6", "pawn"), ("h6", minor)]),
    };
    let anchor_cell = square(anchor);
    let mut cells = Vec::new();
    for row in anchor_cell.row..=anchor_cell.row + 1 {
        for col in anchor_cell.col..=anchor_cell.col + 1 {
            cells.push(Square { row, col });
        }
    }
    cells.extend(placements.iter().map(|(cell, _)| square(cell)));
    let mut seen = BTreeSet::new();
    let mut removed = Vec::new();
    for cell in cells {
        if let Some(piece) = state.at(cell).cloned()
            && seen.insert(piece.id.clone())
        {
            crate::transition::clear_piece(state, &piece.id);
            let owner = if piece.color == color {
                color.opponent()
            } else {
                color
            };
            if piece.color != color {
                cancel_prophecies(state);
                state.captures.get_mut(color).push(piece.clone());
            }
            removed.push((piece, owner));
        }
    }
    for (cell, kind) in placements {
        let mut piece = spawn(state, color, kind)?;
        piece.extra.insert("origin".into(), json!(cell));
        piece.moved = true;
        place(state, piece, square(cell));
    }
    let mut piece = spawn(state, color, if bishop { "bigBishop" } else { "bigRook" })?;
    piece.extra.insert("origin".into(), json!(anchor));
    place_large(state, piece, anchor_cell);
    environmental_defeats(state, &removed, if bishop { "BISHOP" } else { "ROOK" })?;
    Ok(true)
}
fn horde(state: &mut GameState, color: Color) -> Result<bool> {
    interaction_guard(state)?;
    let Some((old, mut king)) = royal(state, color) else {
        return Ok(false);
    };
    let home = Square {
        row: if color == Color::White { 7 } else { 0 },
        col: 4,
    };
    if state.at(home).is_some_and(|p| p.color != color) {
        return Ok(false);
    }
    let preserve_big = entries(state, color)
        .iter()
        .any(|(_, p)| p.kind == "bigRook");
    for (_, piece) in entries(state, color) {
        if piece.id != king.id {
            crate::transition::clear_piece(state, &piece.id);
        }
    }
    state.board[old.row as usize][old.col as usize] = None;
    king.moved = true;
    place(state, king, home);
    let rows = if color == Color::White {
        [7, 6, 5, 4]
    } else {
        [0, 1, 2, 3]
    };
    let mut cells = Vec::new();
    for row in rows {
        for col in 0..8 {
            cells.push(Square { row, col });
        }
    }
    for col in [1, 2, 6, 5] {
        cells.push(Square {
            row: if color == Color::White { 3 } else { 4 },
            col,
        });
    }
    for cell in cells {
        if cell == home || state.at(cell).is_some() {
            continue;
        }
        let mut pawn = spawn(state, color, "pawn")?;
        pawn.extra.insert("origin".into(), json!(name(cell)));
        place(state, pawn, cell);
    }
    if preserve_big {
        big(state, color, false, true)?;
    }
    remove_collapsed(state, color)?;
    Ok(true)
}
fn london(state: &mut GameState, color: Color) -> Result<bool> {
    interaction_guard(state)?;
    const LAYOUT: [(&str, &str); 16] = [
        ("a1", "rook"),
        ("d1", "queen"),
        ("e1", "king"),
        ("h1", "rook"),
        ("f4", "bishop"),
        ("d3", "bishop"),
        ("f3", "knight"),
        ("d2", "knight"),
        ("a2", "pawn"),
        ("b2", "pawn"),
        ("c3", "pawn"),
        ("d4", "pawn"),
        ("e3", "pawn"),
        ("f2", "pawn"),
        ("g2", "pawn"),
        ("h2", "pawn"),
    ];
    let layout = LAYOUT
        .into_iter()
        .map(|(name, kind)| {
            let mut cell = square(name);
            if color == Color::Black {
                cell.row = 7 - cell.row;
            }
            (cell, kind)
        })
        .collect::<Vec<_>>();
    let targets = layout
        .iter()
        .map(|(cell, _)| *cell)
        .collect::<BTreeSet<_>>();
    let mut footballs = Vec::new();
    let mut seen = BTreeSet::new();
    for (cell, _) in &layout {
        let Some(piece) = state.at(*cell).cloned() else {
            continue;
        };
        if piece.kind == "football" {
            footballs.push(*cell);
            continue;
        }
        if piece.kind == "monster" || piece.color == color || !seen.insert(piece.id.clone()) {
            continue;
        }
        crate::transition::clear_piece(state, &piece.id);
        state.captures.get_mut(color).push(piece.clone());
        cancel_prophecies(state);
        if piece.is_defeat_royal() {
            let label = if piece.color == PieceColor::White {
                "백"
            } else {
                "흑"
            };
            let kind = match piece.kind.as_str() {
                "vip" => "귀빈",
                "merchant" => "상인",
                _ => "킹",
            };
            if let Some(side) = piece.color.owner()
                && piece.is_royal()
            {
                state.set_flag("kingDead", side, true);
            }
            crate::flow::end_game(
                state,
                Some(color),
                &format!(
                    "{label} {kind}이 {}.",
                    if kind == "상인" {
                        "쓰러졌습니다"
                    } else {
                        "잡혔습니다"
                    }
                ),
            )?;
        }
    }
    for (_, piece) in entries(state, color) {
        crate::transition::clear_piece(state, &piece.id);
    }
    for from in footballs {
        let Some(mut ball) = state.at(from).cloned() else {
            continue;
        };
        let candidates = [(0, -1), (0, 1)]
            .into_iter()
            .filter_map(|(dr, dc)| from.offset(dr, dc))
            .filter(|cell| state.at(*cell).is_none())
            .collect::<Vec<_>>();
        if let Some(to) = candidates
            .iter()
            .find(|cell| !targets.contains(cell))
            .copied()
            .or_else(|| candidates.first().copied())
        {
            ball.moved = true;
            state.board[from.row as usize][from.col as usize] = None;
            state.extra.insert("lastMove".into(),json!({"from":from,"to":to,"pieceId":"","pieceType":"","soundName":"move","soundColor":color,"hiddenFrom":"","idolEncoreEligible":false,"idolEncoreId":"","idolEncorePieceId":"","idolEncoreConsumed":false}));
            place(state, ball, to);
        }
    }
    for (cell, kind) in layout {
        if state
            .at(cell)
            .is_some_and(|p| matches!(p.kind.as_str(), "football" | "monster"))
        {
            continue;
        }
        let mut piece = spawn(state, color, kind)?;
        piece.extra.insert("origin".into(), json!(name(cell)));
        piece.moved = matches!(cell.row, 2..=5)
            || cell.col == 3 && cell.row == if color == Color::White { 6 } else { 1 };
        if kind == "pawn" {
            piece.extra.insert("londonSystemPawn".into(), json!(true));
        }
        place(state, piece, cell);
    }
    remove_collapsed(state, color)?;
    Ok(true)
}
pub(crate) fn apply(state: &mut GameState, color: Color, effect: &str) -> Result<Option<bool>> {
    let result = match effect {
        "horde" => horde(state, color)?,
        "londonSystem" => london(state, color)?,
        "bigRook" | "bigBishop" => {
            let count = entries(state, color)
                .iter()
                .filter(|(_, p)| p.kind == "pawn")
                .count();
            big(state, color, effect == "bigBishop", count > 8)?
        }
        "horseRiding" => {
            if royal(state, color).is_none() {
                false
            } else {
                for (_, piece) in entries(state, color) {
                    if piece.kind == "knight" {
                        let animated = state
                            .extra
                            .entry("animatedPieceIds")
                            .or_insert_with(|| json!({"__simType":"Set","values":[]}));
                        let ids = animated["values"].as_array_mut().ok_or_else(|| {
                            EngineError::InvalidState("animatedPieceIds must be a Set".into())
                        })?;
                        if !ids.contains(&json!(piece.id)) {
                            ids.push(json!(piece.id));
                        }
                        crate::transition::clear_piece(state, &piece.id);
                    }
                }
                state.set_flag(
                    if state.flag("knightmate", color) {
                        "royalKnightKing"
                    } else {
                        "kingKnight"
                    },
                    color,
                    true,
                );
                true
            }
        }
        "encouragement" => {
            if royal(state, color).is_none() {
                false
            } else {
                state.set_flag(effect, color, true);
                true
            }
        }
        "dutch" | "apprenticeKnights" => {
            let mut changed = 0;
            for (cell, mut piece) in entries(state, color) {
                let origin = piece
                    .extra
                    .get("origin")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| name(cell));
                let valid = if effect == "dutch" {
                    matches!(piece.kind.as_str(), "rook" | "bishop" | "knight")
                } else {
                    piece.kind == "pawn"
                        && [
                            if color == Color::White { "b2" } else { "b7" },
                            if color == Color::White { "g2" } else { "g7" },
                        ]
                        .contains(&origin.as_str())
                };
                if !valid {
                    continue;
                }
                piece.kind = if effect == "dutch" {
                    "windmill"
                } else {
                    "squire"
                }
                .into();
                if effect == "dutch" {
                    piece.extra.insert("windmillMode".into(), json!("bishop"));
                }
                crate::card_effects::mark_transformed_origin_with_options(
                    state,
                    &mut piece,
                    cell,
                    effect == "dutch",
                )?;
                piece.moved = true;
                if effect == "dutch" {
                    crate::card_effects::mark_animation(state, &piece)?;
                }
                for entry in state.board.iter_mut().flatten() {
                    if entry.as_ref().is_some_and(|p| p.id == piece.id) {
                        *entry = Some(piece.clone());
                    }
                }
                changed += 1;
            }
            effect == "dutch" || changed > 0
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn initial() -> GameState {
        GameState::new(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            11,
        )
        .unwrap()
    }
    #[test]
    fn horde_preserves_royal_and_source_spawn_order_without_capture_bookkeeping() {
        let mut state = initial();
        let king = state.at(square("e1")).unwrap().clone();
        let cursor = state.rng.cursor;
        assert_eq!(
            apply(&mut state, Color::White, "horde").unwrap(),
            Some(true)
        );
        assert_eq!(state.rng.cursor - cursor, 35);
        let own = entries(&state, Color::White);
        assert_eq!(own.iter().filter(|(_, p)| p.kind == "pawn").count(), 35);
        assert_eq!(state.at(square("e1")).unwrap().id, king.id);
        assert!(state.at(square("e1")).unwrap().moved);
        assert_eq!(state.captures.white.len() + state.captures.black.len(), 0);
        assert!(state.at(square("g5")).is_some());
        assert!(state.at(square("f5")).is_some());
        assert!(state.at(square("d5")).is_none());
    }
    #[test]
    fn large_opening_has_one_identity_four_cells_and_removes_old_large_footprint() {
        let mut state = initial();
        let cursor = state.rng.cursor;
        assert_eq!(
            apply(&mut state, Color::White, "bigRook").unwrap(),
            Some(true)
        );
        let id = state.at(square("g2")).unwrap().id.clone();
        assert_eq!(state.rng.cursor - cursor, 4);
        assert_eq!(
            state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|piece| piece.id == id)
                .count(),
            4
        );
        assert_eq!(state.at(square("g2")).unwrap().number("hp"), 2);
        assert_eq!(
            apply(&mut state, Color::White, "bigBishop").unwrap(),
            Some(true)
        );
        assert!(
            state
                .board
                .iter()
                .flatten()
                .flatten()
                .all(|piece| piece.id != id)
        );
        let bishop = state.at(square("f2")).unwrap();
        assert_eq!(bishop.kind, "bigBishop");
        assert_eq!(
            state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|piece| piece.id == bishop.id)
                .count(),
            4
        );
    }
    #[test]
    fn opening_capture_locks_and_encouragement_follow_actual_royal_adjacency() {
        let mut state = initial();
        state
            .at_mut(square("b1"))
            .unwrap()
            .extra
            .insert("freshNoCaptureUntil".into(), json!(7));
        apply(&mut state, Color::White, "dutch").unwrap();
        assert_eq!(
            state
                .at(square("b1"))
                .unwrap()
                .number("freshNoCaptureUntil"),
            7
        );
        apply(&mut state, Color::White, "apprenticeKnights").unwrap();
        assert_eq!(state.at(square("b2")).unwrap().kind, "squire");
        assert_eq!(
            state
                .at(square("b2"))
                .unwrap()
                .number("freshNoCaptureUntil"),
            1
        );
        apply(&mut state, Color::White, "encouragement").unwrap();
        assert!(crate::movement::encouraged(
            &state,
            state.at(square("e2")).unwrap()
        ));
        assert!(!crate::movement::encouraged(
            &state,
            state.at(square("a2")).unwrap()
        ));
    }
}
