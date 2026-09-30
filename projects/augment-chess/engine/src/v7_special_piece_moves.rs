//! Frozen v7 base candidates for five source-specific piece kinds.
//!
//! Source: main-OahWs0tU.js, SHA-256
//! e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c.
//! This layer preserves getLegalMoves' candidate order and flags. The common
//! movement layer owns global restrictions and transition execution.

use crate::movement::{KING, KNIGHT, can_capture, collapsed};
use crate::{Color, EngineError, GameState, MoveTarget, Piece, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};
use std::collections::BTreeSet;

// queenDirections() is bishopDirections() followed by rookDirections().
const QUEEN: &[(i8, i8)] = &[
    (-1, -1),
    (-1, 1),
    (1, -1),
    (1, 1),
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
];

/// `None` means this object does not own the physical piece kind or ruleset.
/// A source path we cannot reproduce is an error, never a partial candidate list.
pub(crate) fn base_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Option<Vec<MoveTarget>>> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Ok(None);
    }
    if !matches!(
        piece.kind.as_str(),
        "pegasus" | "dragon" | "jester" | "primeMinister" | "shotgunKing"
    ) {
        return Ok(None);
    }
    if from.row >= 8 || from.col >= 8 {
        return Err(EngineError::InvalidState(
            "v7 special piece origin outside 8x8".into(),
        ));
    }
    let actor = piece.color.owner().ok_or_else(|| {
        EngineError::InvalidState(format!("v7 {} has neutral allegiance", piece.kind))
    })?;
    let moves = match piece.kind.as_str() {
        "pegasus" => pegasus(state, piece, from, actor),
        "dragon" => dragon(state, piece, from, actor),
        "jester" => jester(state, piece, from, actor)?,
        "primeMinister" => prime_minister(state, piece, from)?,
        "shotgunKing" => shotgun_king(state, piece, from, actor),
        _ => unreachable!("kind checked above"),
    };
    Ok(Some(moves))
}

/// Source isStealthTransparentFor checks hiddenFrom and then a non-royal
/// enemy's camouflage square. This is deliberately separate from the public
/// observation predicate: camouflageRule uses JS truthiness here, not a
/// viewer-specific side flag, and hiddenFrom is checked before royal status.
fn stealth_transparent(state: &GameState, actor: Color, target: &Piece, at: Square) -> bool {
    if target.color == actor {
        return false;
    }
    if target.extra.get("hiddenFrom").and_then(Value::as_str) == Some(actor.as_str()) {
        return true;
    }
    if !crate::observation::truth(state.extra.get("camouflageRule")) || state.royal_identity(target)
    {
        return false;
    }
    // The source prefers an integer large-piece anchor; otherwise it locates
    // this exact board entry. All callers pass a board entry's coordinate.
    let anchor = target
        .extra
        .get("anchorRow")
        .and_then(Value::as_i64)
        .zip(target.extra.get("anchorCol").and_then(Value::as_i64));
    let (row, col) = anchor.unwrap_or((i64::from(at.row), i64::from(at.col)));
    if !matches!(target.color.owner(), Some(Color::White | Color::Black)) {
        return false;
    }
    let light = (row + col).rem_euclid(2) == 0;
    if target.color == Color::White {
        light
    } else {
        !light
    }
}

/// knightDeltasForMove: the long-axis intervening square is ignored only for
/// a hidden enemy or a friendly Ghost. Passing "rook" to the source helper
/// makes the friendly Ghost transparent for every one of these callers.
fn knight_deltas(state: &GameState, piece: &Piece, from: Square, actor: Color) -> Vec<(i8, i8)> {
    if !state.flag("knightInjury", actor) {
        return KNIGHT.to_vec();
    }
    KNIGHT
        .iter()
        .copied()
        .filter(|&(dr, dc)| {
            let jump = if dr.abs() > dc.abs() {
                from.offset(dr.signum(), 0)
            } else {
                from.offset(0, dc.signum())
            };
            jump.and_then(|at| state.at(at).map(|blocker| (at, blocker)))
                .is_none_or(|(at, blocker)| {
                    stealth_transparent(state, actor, blocker, at)
                        || blocker.color == piece.color
                            && crate::observation::truth(blocker.extra.get("ghost"))
                })
        })
        .collect()
}

/// Source jumpMoves has no collapsed-square filter. A later shared movement
/// restriction owns that decision; filtering here could suppress a route that
/// is otherwise source-reachable.
fn jump_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    actor: Color,
    deltas: &[(i8, i8)],
) -> Vec<MoveTarget> {
    deltas
        .iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .filter(|&to| {
            state.at(to).is_none_or(|target| {
                stealth_transparent(state, actor, target, to) || can_capture(state, piece, target)
            })
        })
        .map(MoveTarget::at)
        .collect()
}

/// pegasusMoves visits every non-origin square in row-major order. Occupied
/// visible squares are only available as injured-knight captures.
fn pegasus(state: &GameState, piece: &Piece, from: Square, actor: Color) -> Vec<MoveTarget> {
    let knight = knight_deltas(state, piece, from, actor)
        .into_iter()
        .filter_map(|(dr, dc)| from.offset(dr, dc))
        .collect::<BTreeSet<_>>();
    let mut moves = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let to = Square { row, col };
            if to == from {
                continue;
            }
            let available = state.at(to).is_none_or(|target| {
                stealth_transparent(state, actor, target, to)
                    || knight.contains(&to) && can_capture(state, piece, target)
            });
            if available {
                moves.push(MoveTarget::at(to));
            }
        }
    }
    moves
}

/// dragonMoves appends source forEachSquare ally swaps after ordered jumps.
/// uniqueMoves keeps the first candidate for each coordinate.
fn dragon(state: &GameState, piece: &Piece, from: Square, actor: Color) -> Vec<MoveTarget> {
    let deltas = knight_deltas(state, piece, from, actor);
    let mut moves = jump_moves(state, piece, from, actor, &deltas);
    for row in 0..8 {
        for col in 0..8 {
            let to = Square { row, col };
            if to == from {
                continue;
            }
            if state.at(to).is_some_and(|ally| {
                ally.color == piece.color
                    && ally.ability_kind() != "slime"
                    && ally.kind != "wall"
                    && !ally.is_large()
            }) {
                let mut target = MoveTarget::at(to);
                target.flags.insert("dragonSwap".into(), json!(true));
                moves.push(target);
            }
        }
    }
    unique(moves)
}

/// jesterMoves uses source queen-ray order before its royal/merchant filter.
fn jester(state: &GameState, piece: &Piece, from: Square, actor: Color) -> Result<Vec<MoveTarget>> {
    let rays = crate::movement::rays(state, piece, from, QUEEN, 7);
    Ok(unique(rays)
        .into_iter()
        .filter(|candidate| {
            let to = candidate.square();
            let Some(target) = state.at(to) else {
                return true;
            };
            if stealth_transparent(state, actor, target, to) {
                return true;
            }
            target.color != piece.color
                && target.kind != "jester"
                && (state.royal_identity(target) || target.kind == "merchant")
                && can_capture(state, piece, target)
        })
        .collect())
}

/// primeMinisterMoves visits the first king step and then second steps when
/// the intermediate square is empty. Its portal entry/exit action flags are
/// distinct routes and must not be replaced by ordinary two-step moves.
fn prime_minister(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    let mut moves = Vec::new();
    for &(dr, dc) in KING {
        let Some(mid) = from.offset(dr, dc).filter(|&cell| !collapsed(state, cell)) else {
            continue;
        };
        let mid_target = state.at(mid);
        if let Some(exit) = crate::movement::v7_portal_exit_at(state, mid) {
            if crate::movement::v7_can_portal_land_on(state, piece, mid, false)?
                && crate::movement::v7_can_portal_land_on(state, piece, exit, false)?
            {
                let mut target = MoveTarget::at(mid);
                target.flags.insert("primeMinisterMove".into(), json!(true));
                target
                    .flags
                    .insert("primeMinisterPortalEntry".into(), json!(true));
                target.flags.insert("portalLanding".into(), json!(true));
                target.flags.insert("portalEntry".into(), json!(mid));
                target.flags.insert("portalExit".into(), json!(exit));
                moves.push(target);
            }
            if piece.kind == "primeMinister"
                && mid_target.is_none()
                && state.at(exit).is_none()
                && !collapsed(state, mid)
                && !collapsed(state, exit)
            {
                for &(dr, dc) in KING {
                    let Some(to) = exit.offset(dr, dc).filter(|&to| !collapsed(state, to)) else {
                        continue;
                    };
                    if state
                        .at(to)
                        .is_some_and(|victim| !can_capture(state, piece, victim))
                    {
                        continue;
                    }
                    let mut target = MoveTarget::at(to);
                    target.flags.insert("primeMinisterMove".into(), json!(true));
                    target
                        .flags
                        .insert("primeMinisterPortalExit".into(), json!(true));
                    target.flags.insert("portalThrough".into(), json!(true));
                    target.flags.insert("portalEntry".into(), json!(mid));
                    target.flags.insert("portalExit".into(), json!(exit));
                    moves.push(target);
                }
            }
            continue;
        }
        if mid_target.is_none_or(|target| can_capture(state, piece, target)) {
            let mut move_target = MoveTarget::at(mid);
            move_target
                .flags
                .insert("primeMinisterMove".into(), json!(true));
            moves.push(move_target);
        }
        if mid_target.is_some() {
            continue;
        }
        for &(next_dr, next_dc) in KING {
            let Some(to) = mid
                .offset(next_dr, next_dc)
                .filter(|&cell| cell != from && !collapsed(state, cell))
            else {
                continue;
            };
            if let Some(exit) = crate::movement::v7_portal_exit_at(state, to) {
                if crate::movement::v7_can_portal_land_on(state, piece, to, false)?
                    && crate::movement::v7_can_portal_land_on(state, piece, exit, false)?
                {
                    let mut target = MoveTarget::at(to);
                    target.flags.insert("primeMinisterMove".into(), json!(true));
                    target
                        .flags
                        .insert("primeMinisterPortalSecondEntry".into(), json!(true));
                    target.flags.insert("portalLanding".into(), json!(true));
                    target.flags.insert("portalEntry".into(), json!(to));
                    target.flags.insert("portalExit".into(), json!(exit));
                    moves.push(target);
                }
                continue;
            }
            if state
                .at(to)
                .is_none_or(|target| can_capture(state, piece, target))
            {
                let mut move_target = MoveTarget::at(to);
                move_target
                    .flags
                    .insert("primeMinisterMove".into(), json!(true));
                moves.push(move_target);
            }
        }
    }
    Ok(unique(moves))
}

/// Raw source kernel for nighttime Bat attack/movement queries. This bypasses
/// physical-type dispatch and the unrelated campaign gate of base_moves.
pub(crate) fn raw_prime_minister_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    if from.row >= 8 || from.col >= 8 {
        return Err(EngineError::InvalidState(
            "v7 primeMinister origin outside 8x8".into(),
        ));
    }
    prime_minister(state, piece, from)
}

/// main97367 shotgunBlastCells visits each depth's side offsets in source
/// order, then retains the first occurrence of every in-bounds cell.
pub(crate) fn shotgun_blast_cells(from: Square, direction: [i8; 2]) -> Vec<Square> {
    let [dr, dc] = direction;
    if from.row >= 8
        || from.col >= 8
        || !(-1..=1).contains(&dr)
        || !(-1..=1).contains(&dc)
        || dr == 0 && dc == 0
    {
        return Vec::new();
    }
    let offsets = if dr == 0 {
        [(-1, 0), (0, 0), (1, 0)]
    } else if dc == 0 {
        [(0, -1), (0, 0), (0, 1)]
    } else {
        [(0, 0), (-dr, 0), (0, -dc)]
    };
    let mut cells = Vec::new();
    let mut seen = BTreeSet::new();
    for distance in 1..=if dr == 0 || dc == 0 { 3 } else { 2 } {
        let current = if distance == 3 {
            &offsets[1..2]
        } else {
            &offsets[..]
        };
        for &(sr, sc) in current {
            if let Some(cell) = from.offset(dr * distance + sr, dc * distance + sc)
                && seen.insert(cell)
            {
                cells.push(cell);
            }
        }
    }
    cells
}

fn shotgun_king(state: &GameState, piece: &Piece, from: Square, actor: Color) -> Vec<MoveTarget> {
    match state.extra.get("shotgunAction").and_then(Value::as_str) {
        Some("shotgun") => {
            let mut moves = Vec::new();
            for row in 0..8 {
                for col in 0..8 {
                    let to = Square { row, col };
                    if to == from {
                        continue;
                    }
                    let mut target = MoveTarget::at(to);
                    target.flags.insert("shotgunBlast".into(), json!(true));
                    target.flags.insert(
                        "shotgunDirection".into(),
                        json!([
                            (i16::from(row) - i16::from(from.row)).signum(),
                            (i16::from(col) - i16::from(from.col)).signum(),
                        ]),
                    );
                    moves.push(target);
                }
            }
            moves
        }
        Some("snipe") => {
            if piece.number("ammo") < 3 {
                return Vec::new();
            }
            let mut moves = Vec::new();
            for &(dr, dc) in QUEEN {
                let mut cursor = from;
                for _ in 0..7 {
                    let Some(to) = cursor.offset(dr, dc) else {
                        break;
                    };
                    cursor = to;
                    if let Some(victim) = state.at(to) {
                        if !matches!(victim.ability_kind(), "guard" | "jester" | "revolvingDoor")
                            && can_capture(state, piece, victim)
                        {
                            let mut target = MoveTarget::at(to);
                            target.flags.insert("shotgunSnipe".into(), json!(true));
                            moves.push(target);
                        }
                        break;
                    }
                }
            }
            moves
        }
        _ => KING
            .iter()
            .filter_map(|&(dr, dc)| from.offset(dr, dc))
            .filter(|&to| {
                state
                    .at(to)
                    .is_none_or(|target| stealth_transparent(state, actor, target, to))
            })
            .map(MoveTarget::at)
            .collect(),
    }
}

fn unique(moves: Vec<MoveTarget>) -> Vec<MoveTarget> {
    let mut seen = BTreeSet::new();
    moves
        .into_iter()
        .filter(|target| seen.insert(target.square()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameConfig, PieceColor};
    use sha2::{Digest, Sha256};

    fn empty(kind: &str) -> (GameState, Piece, Square) {
        let mut state = GameState::new(GameConfig::default(), 17).unwrap();
        state.board = vec![vec![None; 8]; 8];
        state.ruleset_id = RULES_VERSION_V7.into();
        let piece = Piece::new(kind, Color::White, "source-piece");
        let from = Square { row: 4, col: 4 };
        state.board[4][4] = Some(piece.clone());
        (state, piece, from)
    }

    fn squares(moves: &[MoveTarget]) -> Vec<Square> {
        moves.iter().map(MoveTarget::square).collect()
    }

    // These digests were measured by invoking the named move functions in
    // the verified frozen-client VM at the SHA pinned above. They cover the
    // entire ordered candidate list and each execution flag, not just counts.
    fn source_digest(moves: &[MoveTarget]) -> String {
        format!("{:x}", Sha256::digest(serde_json::to_vec(moves).unwrap()))
    }

    #[test]
    fn v7_only_and_unowned_kind_are_none() {
        let (mut state, piece, from) = empty("pegasus");
        state.ruleset_id = crate::RULES_VERSION_V6.into();
        assert!(base_moves(&state, &piece, from).unwrap().is_none());
        state.ruleset_id = RULES_VERSION_V7.into();
        assert!(
            base_moves(&state, &Piece::new("rook", Color::White, "other"), from)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn pegasus_scans_row_major_and_hidden_enemy_is_empty_for_move() {
        let (mut state, piece, from) = empty("pegasus");
        let mut hidden = Piece::new("rook", Color::Black, "hidden");
        hidden.extra.insert("hiddenFrom".into(), json!("white"));
        state.board[0][0] = Some(hidden);
        state.board[0][1] = Some(Piece::new("rook", Color::Black, "visible"));
        let moves = base_moves(&state, &piece, from).unwrap().unwrap();
        assert_eq!(moves.first().unwrap().square(), Square { row: 0, col: 0 });
        assert!(!squares(&moves).contains(&Square { row: 0, col: 1 }));
        assert_eq!(moves.len(), 62);
        assert_eq!(
            source_digest(&moves),
            "9ab931b643e649724bc6b6eec81ca2290097f8ee4939ef058106cccc35ec620f"
        );
    }

    #[test]
    fn pegasus_uses_source_camouflage_truthiness_and_injured_knight_filter() {
        let (mut state, piece, from) = empty("pegasus");
        state.board[0][0] = Some(Piece::new("rook", Color::Black, "visible"));
        state.board[0][1] = Some(Piece::new("rook", Color::Black, "camouflaged"));
        state.board[2][3] = Some(Piece::new("king", Color::Black, "injured-capture"));
        state.board[3][4] = Some(Piece::new("wall", PieceColor::Neutral, "blocker"));
        state.extra.insert(
            "camouflageRule".into(),
            json!({"white":false,"black":false}),
        );
        state
            .extra
            .insert("knightInjury".into(), json!({"white":true,"black":false}));
        let moves = base_moves(&state, &piece, from).unwrap().unwrap();
        assert_eq!(moves.first().unwrap().square(), Square { row: 0, col: 1 });
        assert!(!squares(&moves).contains(&Square { row: 2, col: 3 }));
        assert_eq!(
            source_digest(&moves),
            "df3f400f713c259f56d291867f36eb3fac439e34218ef15abb078af69c62edf1"
        );
    }

    #[test]
    fn dragon_keeps_knight_order_then_row_major_swap_flags() {
        let (mut state, piece, from) = empty("dragon");
        state.board[0][0] = Some(Piece::new("pawn", Color::White, "ally"));
        state.board[4][5] = Some(Piece::new("wall", PieceColor::Neutral, "wall"));
        let moves = base_moves(&state, &piece, from).unwrap().unwrap();
        assert_eq!(
            &squares(&moves)[..8],
            &KNIGHT
                .iter()
                .map(|&(dr, dc)| from.offset(dr, dc).unwrap())
                .collect::<Vec<_>>()
        );
        let swap = moves.last().unwrap();
        assert_eq!(swap.square(), Square { row: 0, col: 0 });
        assert_eq!(swap.flags.get("dragonSwap"), Some(&json!(true)));
        assert_eq!(
            source_digest(&moves),
            "a6b17e27a268f977025dfc4d39c92e4e551ad4eaa4b90f46f58627c3131ea015"
        );
    }

    #[test]
    fn jester_filters_nonroyal_capture_and_preserves_unrelated_portal_state() {
        let (mut state, piece, from) = empty("jester");
        state.board[4][5] = Some(Piece::new("pawn", Color::Black, "blocker"));
        state.board[4][6] = Some(Piece::new("king", Color::Black, "behind"));
        state.board[2][4] = Some(Piece::new("king", Color::Black, "royal"));
        let moves = base_moves(&state, &piece, from).unwrap().unwrap();
        assert!(!squares(&moves).contains(&Square { row: 4, col: 5 }));
        assert!(!squares(&moves).contains(&Square { row: 4, col: 6 }));
        assert!(squares(&moves).contains(&Square { row: 2, col: 4 }));
        assert_eq!(
            source_digest(&moves),
            "8fe27a7421d0940e66445a55491f793a501da2e2aff27c68fb8b29993ad98ca9"
        );
        state.extra.insert("portalRule".into(), json!(true));
        assert_eq!(base_moves(&state, &piece, from).unwrap().unwrap(), moves);
    }

    #[test]
    fn prime_minister_second_step_requires_empty_mid_and_preserves_flag() {
        let (mut state, piece, from) = empty("primeMinister");
        state.board[3][3] = Some(Piece::new("pawn", Color::Black, "capture"));
        let moves = base_moves(&state, &piece, from).unwrap().unwrap();
        assert!(moves.iter().all(|target| target.flag("primeMinisterMove")));
        assert_eq!(
            moves
                .iter()
                .filter(|target| target.square() == Square { row: 3, col: 3 })
                .count(),
            1
        );
        assert_eq!(
            source_digest(&moves),
            "aa1b32973058998175da6228f0398fbd48a2af2718e76c5e5762b180e41f7656"
        );
        state
            .extra
            .insert("portalRule".into(), json!({"enabled": true}));
        let portal = base_moves(&state, &piece, from).unwrap().unwrap();
        assert!(
            portal
                .iter()
                .any(|m| m.flag("portalLanding") && m.flags.contains_key("portalExit"))
        );
    }

    #[test]
    fn shotgun_modes_preserve_order_and_indirect_immunity() {
        let (mut state, mut piece, from) = empty("shotgunKing");
        state.extra.insert("shotgunAction".into(), json!("shotgun"));
        let blast = base_moves(&state, &piece, from).unwrap().unwrap();
        assert_eq!(blast.len(), 63);
        assert_eq!(blast[0].square(), Square { row: 0, col: 0 });
        assert_eq!(
            blast[0].flags.get("shotgunDirection"),
            Some(&json!([-1, -1]))
        );
        assert_eq!(
            source_digest(&blast),
            "320145ad81ee24bb6d04f5f9e19f1cba0839ed85490fb2aeb52b9bd531fb0cf6"
        );
        piece.extra.insert("ammo".into(), json!(3));
        state.board[4][4] = Some(piece.clone());
        state.extra.insert("shotgunAction".into(), json!("snipe"));
        state.board[2][2] = Some(Piece::new("king", Color::Black, "royal"));
        state.board[2][4] = Some(Piece::new("jester", Color::Black, "immune"));
        let snipe = base_moves(&state, &piece, from).unwrap().unwrap();
        assert!(
            snipe
                .iter()
                .any(|target| target.square() == Square { row: 2, col: 2 }
                    && target.flag("shotgunSnipe"))
        );
        assert!(
            !snipe
                .iter()
                .any(|target| target.square() == Square { row: 2, col: 4 })
        );
        assert_eq!(
            source_digest(&snipe),
            "acb8dbbde99ea34cac8926595516f1fe6c35ca4e793e2e8152989bf67c1019d7"
        );
    }

    #[test]
    fn missing_time_phase_uses_source_future_in_raw_candidate_queries() {
        let (mut state, pegasus, from) = empty("pegasus");
        state
            .extra
            .insert("campaign".into(), json!({"setup":"timeTraveler"}));
        assert_eq!(
            base_moves(&state, &pegasus, from).unwrap().unwrap().len(),
            63
        );
        let king = Piece::new("shotgunKing", Color::White, "source-piece");
        state.board[4][4] = Some(king.clone());
        state.extra.insert("shotgunAction".into(), json!("snipe"));
        assert!(base_moves(&state, &king, from).unwrap().unwrap().is_empty());
        state.extra.insert("shotgunAction".into(), json!("shotgun"));
        assert_eq!(base_moves(&state, &king, from).unwrap().unwrap().len(), 63);
    }
}
