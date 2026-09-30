//! Statically registered, game-owned movement objects.
//!
//! Each object owns a piece's ordered base candidates. The host applies global
//! modifiers, movement restrictions, selection policy and execution. This is
//! deliberately a typed game layer; the reusable adapter contract does not
//! learn about GameState, Piece or Square.

use super::{
    CAMEL, DIAG, EAGLE, KING, ORTHO, cannon, castling, checker, grasshopper, large_rays, leaps,
    missionary, pawn_moves, rays, v7_jump_leaps, v7_large_moves,
};
use crate::{EngineError, GameState, MoveTarget, Piece, Result, Square};
use std::collections::BTreeSet;

type Enumerator = fn(&GameState, &Piece, Square) -> Result<Vec<MoveTarget>>;

/// Immutable source-kind registration. The rule ID stays stable when a rule's
/// implementation changes; the selected ruleset/source pin belongs to host.
pub(crate) struct MovementObject {
    pub(crate) rule_id: &'static str,
    pub(crate) source_kind: &'static str,
    enumerate: Enumerator,
}

impl MovementObject {
    pub(crate) fn enumerate_base(
        &self,
        state: &GameState,
        piece: &Piece,
        from: Square,
    ) -> Result<Vec<MoveTarget>> {
        if piece.kind != self.source_kind {
            return Err(EngineError::InvalidState(format!(
                "movement object {} received source kind {}",
                self.rule_id, piece.kind
            )));
        }
        (self.enumerate)(state, piece, from)
    }
}

macro_rules! rule {
    ($kind:literal, $enumerator:ident) => {
        MovementObject {
            rule_id: concat!("piece/movement/", $kind),
            source_kind: $kind,
            enumerate: $enumerator,
        }
    };
}

// Source switch kinds are explicit. A new kind cannot silently inherit queen
// movement or fall through to a generic variant implementation.
static MOVEMENT_OBJECTS: &[MovementObject] = &[
    rule!("pawn", pawn),
    rule!("squire", pawn),
    rule!("standardBearer", pawn),
    rule!("rook", rook),
    rule!("bishop", bishop),
    rule!("queen", queen),
    rule!("king", king),
    rule!("knight", knight),
    rule!("royalKnight", knight),
    rule!("unicorn", knight),
    rule!("amazon", amazon),
    rule!("man", king_step),
    rule!("guard", king_step),
    rule!("camel", camel),
    rule!("alfil", alfil),
    rule!("ferz", diagonal_step),
    rule!("eagle", eagle),
    rule!("alibaba", eagle),
    rule!("knightmaster", diagonal_step),
    rule!("cannon", cannon_base),
    rule!("grasshopper", grasshopper_base),
    rule!("princess", princess),
    rule!("clockwork", clockwork),
    rule!("campfire", campfire),
    rule!("wall", immobile),
    rule!("coffin", immobile),
    rule!("scarecrow", immobile),
    rule!("checker", checker_base),
    rule!("checkerKing", checker_base),
    rule!("missionary", missionary_base),
    rule!("bigRook", large),
    rule!("bigBishop", large),
    rule!("reaper", variant),
    rule!("hedgehog", variant),
    rule!("undead", variant),
    rule!("vip", variant),
    rule!("crown", variant),
    rule!("octopus", variant),
    rule!("paladin", variant),
    rule!("donQuixote", variant),
    rule!("recruiter", variant),
    rule!("bear", variant),
    rule!("revolvingDoor", variant),
    rule!("babyBear", variant),
    rule!("wizard", variant),
    rule!("idol", variant),
    rule!("darkWizard", variant),
    rule!("windmill", variant),
    rule!("lobster", variant),
    rule!("slime", variant),
    rule!("siren", variant),
    rule!("magicGirl", variant),
    rule!("berserker", variant),
    rule!("pegasus", variant),
    rule!("dragon", variant),
    rule!("assassin", variant),
    rule!("jester", variant),
    rule!("primeMinister", variant),
    rule!("hook", variant),
    rule!("brutus", variant),
    rule!("cardinal", variant),
    rule!("protestant", variant),
    rule!("fanatic", variant),
    rule!("herald", variant),
    rule!("log", variant),
    rule!("thief", variant),
    rule!("siegeRam", variant),
    rule!("shotgunKing", variant),
    rule!("bat", variant),
    rule!("vampireLord", variant),
    rule!("timeTraveler", variant),
    rule!("parrot", variant),
    rule!("medium", variant),
    rule!("merchant", variant),
    rule!("grappler", variant),
    rule!("colossus", variant),
    rule!("football", variant),
    rule!("monster", variant),
    rule!("blackHole", variant),
    rule!("bomb", variant),
    rule!("platform", variant),
    rule!("portal", variant),
    rule!("trickster", variant),
];

pub(crate) fn for_piece(piece: &Piece) -> Result<&'static MovementObject> {
    MOVEMENT_OBJECTS
        .iter()
        .find(|object| object.source_kind == piece.kind)
        .ok_or_else(|| {
            EngineError::UnsupportedFeature(format!(
                "movement object for source kind {}",
                piece.kind
            ))
        })
}

#[cfg(test)]
pub(crate) fn objects() -> &'static [MovementObject] {
    MOVEMENT_OBJECTS
}

fn pawn(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id == crate::RULES_VERSION_V7 {
        super::v7_pawn_moves(state, piece, from)
    } else {
        Ok(pawn_moves(state, piece, from))
    }
}
fn rook(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    Ok(rays(
        state,
        piece,
        from,
        if state.flag("reversal", piece.color) {
            DIAG
        } else {
            ORTHO
        },
        7,
    ))
}
fn bishop(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    let mut moves = rays(
        state,
        piece,
        from,
        if state.flag("reversal", piece.color) {
            ORTHO
        } else {
            DIAG
        },
        7,
    );
    if state.ruleset_id == crate::RULES_VERSION_V7 && state.flag("bishopSnipe", piece.color) {
        moves.extend(super::v7_raw_bishop_snipe_moves(state, piece, from)?);
        moves = unique_squares(moves);
    }
    Ok(moves)
}
fn queen(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    Ok(queen_rays(state, piece, from))
}
fn queen_rays(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    // Frozen source queenDirections emits diagonal rays before orthogonal rays.
    let mut moves = rays(state, piece, from, DIAG, 7);
    moves.extend(rays(state, piece, from, ORTHO, 7));
    moves
}
fn king(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    let mut moves = v7_jump_leaps(state, piece, from, KING);
    if state.flag("hillKing", piece.color)
        && (3..=4).contains(&from.row)
        && (3..=4).contains(&from.col)
    {
        moves.extend(queen_rays(state, piece, from));
    }
    if state.flag("kingKnight", piece.color) {
        let deltas = crate::variant_movement::knight_deltas_for_move(state, piece, from)?;
        moves.extend(v7_jump_leaps(state, piece, from, &deltas));
    }
    // v7 castling needs the caller's fog probe context, so the common source
    // pipeline appends its castle descriptors after this base king family.
    if state.ruleset_id != crate::RULES_VERSION_V7 {
        moves.extend(castling(state, piece, from));
    }
    Ok(moves)
}
fn knight(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id == crate::RULES_VERSION_V7 && piece.kind == "unicorn" {
        // The v7 source switch has no unicorn movement case.
        return Ok(Vec::new());
    }
    let deltas = crate::variant_movement::knight_deltas_for_move(state, piece, from)?;
    let mut moves = if state.ruleset_id == crate::RULES_VERSION_V7 && piece.kind == "knight" {
        super::v7_raw_knight_moves(state, piece, from, false)?
    } else {
        v7_jump_leaps(state, piece, from, &deltas)
    };
    let corner = if state.ruleset_id == crate::RULES_VERSION_V7 {
        super::v7_is_board_corner(from)
    } else {
        (from.row == 0 || from.row == 7) && (from.col == 0 || from.col == 7)
    };
    if piece.kind == "knight" && state.flag("cornerKick", piece.color) && corner {
        moves.extend(rays(state, piece, from, DIAG, 7));
    }
    if piece.kind == "royalKnight" && state.flag("royalKnightKing", piece.color) {
        moves.extend(v7_jump_leaps(state, piece, from, KING));
    }
    if piece.kind == "royalKnight"
        && state.flag("hillKing", piece.color)
        && (3..=4).contains(&from.row)
        && (3..=4).contains(&from.col)
    {
        moves.extend(queen_rays(state, piece, from));
    }
    Ok(unique_squares(moves))
}
fn amazon(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    let mut moves = queen_rays(state, piece, from);
    let deltas = crate::variant_movement::knight_deltas_for_move(state, piece, from)?;
    moves.extend(v7_jump_leaps(state, piece, from, &deltas));
    Ok(unique_squares(moves))
}
fn king_step(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    Ok(v7_jump_leaps(state, piece, from, KING))
}
fn camel(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    Ok(v7_jump_leaps(state, piece, from, CAMEL))
}
fn alfil(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    Ok(v7_jump_leaps(
        state,
        piece,
        from,
        &[(-2, -2), (-2, 2), (2, -2), (2, 2)],
    ))
}
fn diagonal_step(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    Ok(v7_jump_leaps(state, piece, from, DIAG))
}
fn eagle(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id == crate::RULES_VERSION_V7 && piece.kind == "alibaba" {
        // The frozen v7 getLegalMoves switch has no alibaba base case.
        return Ok(Vec::new());
    }
    Ok(v7_jump_leaps(state, piece, from, EAGLE))
}
fn cannon_base(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    Ok(cannon(state, piece, from))
}
fn grasshopper_base(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    Ok(grasshopper(state, piece, from))
}
fn princess(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    if state.board.iter().flatten().flatten().any(|candidate| {
        candidate.color == piece.color
            && candidate.kind == "queen"
            && !candidate.flag("regencyHeir")
    }) {
        Ok(leaps(state, piece, from, DIAG))
    } else {
        Ok(queen_rays(state, piece, from))
    }
}
fn clockwork(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    if KING
        .iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .any(|square| {
            state
                .at(square)
                .is_some_and(|neighbor| neighbor.color == piece.color)
        })
    {
        Ok(queen_rays(state, piece, from))
    } else {
        Ok(Vec::new())
    }
}
fn campfire(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id == crate::RULES_VERSION_V7 {
        // 원문 wizardMoves의 kingDeltas 순서를 보존한 뒤 직교 이동만 남긴다.
        // quiet 원점은 movementOccupant와 같이 숨은 적을 빈 칸으로 읽는다.
        return Ok(KING
            .iter()
            .filter_map(|&(dr, dc)| from.offset(dr, dc))
            .filter(|to| {
                state
                    .at(*to)
                    .is_none_or(|target| super::v7_stealth_transparent(state, piece, target, *to))
            })
            .filter(|to| to.row == from.row || to.col == from.col)
            .map(MoveTarget::at)
            .collect());
    }
    Ok(leaps(state, piece, from, ORTHO)
        .into_iter()
        .filter(|target| state.at(target.square()).is_none())
        .collect())
}
fn immobile(_: &GameState, _: &Piece, _: Square) -> Result<Vec<MoveTarget>> {
    Ok(Vec::new())
}
fn checker_base(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    Ok(checker(state, piece, from))
}
fn missionary_base(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    Ok(missionary(state, piece, from))
}
fn large(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id == crate::RULES_VERSION_V7 {
        v7_large_moves(state, piece, from)
    } else {
        Ok(large_rays(state, piece, from))
    }
}
fn variant(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    if let Some(moves) = crate::v7_special_piece_moves::base_moves(state, piece, from)? {
        return Ok(moves);
    }
    crate::variant_movement::base_moves(state, piece, from)?
        .ok_or_else(|| EngineError::UnsupportedFeature(format!("variant movement {}", piece.kind)))
}

fn unique_squares(moves: Vec<MoveTarget>) -> Vec<MoveTarget> {
    let mut seen = BTreeSet::new();
    moves
        .into_iter()
        .filter(|target| seen.insert(target.square()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, GameConfig, RULES_VERSION_V7};
    use serde_json::json;

    #[test]
    fn corner_kick_keeps_v7_two_by_two_corner_ray_order_and_v6_scope() {
        // source21 chaos-seed37 sample[1]: Otherworld로 c2 폰이 떠난 뒤
        // b1 나이트는 2개 기본 이동 다음 c2부터 대각 ray를 표시한다.
        let mut state = GameState::new(GameConfig::default(), 37).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.set_flag("cornerKick", Color::White, true);
        state.board[6][2] = None;
        let from = Square { row: 7, col: 1 };
        let knight = state.at(from).unwrap().clone();
        let object = for_piece(&knight).unwrap();
        let v7 = object.enumerate_base(&state, &knight, from).unwrap();
        assert_eq!(
            serde_json::to_value(v7).unwrap(),
            json!([
                {"row":5,"col":0},{"row":5,"col":2},
                {"row":6,"col":2},{"row":5,"col":3},{"row":4,"col":4},
                {"row":3,"col":5},{"row":2,"col":6},{"row":1,"col":7}
            ])
        );
        state.ruleset_id = crate::RULES_VERSION_V6.into();
        let v6 = object.enumerate_base(&state, &knight, from).unwrap();
        assert_eq!(
            serde_json::to_value(v6).unwrap(),
            json!([
                {"row":5,"col":0},{"row":5,"col":2}
            ])
        );
    }

    #[test]
    fn campfire_keeps_source_v7_candidate_order_and_v6_order() {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.board = vec![vec![None; 8]; 8];
        let from = Square { row: 4, col: 3 };
        let campfire = Piece::new("campfire", Color::White, "campfire");
        state.board[4][3] = Some(campfire.clone());
        let object = for_piece(&campfire).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        let v7 = object.enumerate_base(&state, &campfire, from).unwrap();
        assert_eq!(
            serde_json::to_value(v7).unwrap(),
            json!([
                {"row":3,"col":3},{"row":4,"col":2},{"row":4,"col":4},{"row":5,"col":3}
            ])
        );
        state.ruleset_id = crate::RULES_VERSION_V6.into();
        let v6 = object.enumerate_base(&state, &campfire, from).unwrap();
        assert_eq!(
            serde_json::to_value(v6).unwrap(),
            json!([
                {"row":3,"col":3},{"row":5,"col":3},{"row":4,"col":2},{"row":4,"col":4}
            ])
        );
    }

    #[test]
    fn source_kind_and_rule_ids_are_unique() {
        let mut kinds = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for object in objects() {
            assert!(kinds.insert(object.source_kind), "duplicate source kind");
            assert!(ids.insert(object.rule_id), "duplicate rule ID");
            assert_eq!(
                object.rule_id,
                format!("piece/movement/{}", object.source_kind)
            );
        }
        assert!(matches!(
            for_piece(&Piece::new("unknown", Color::White, "unknown")),
            Err(EngineError::UnsupportedFeature(_))
        ));
    }

    #[test]
    fn princess_ignores_regency_heir_queen_for_movement_mode() {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.board = vec![vec![None; 8]; 8];
        let princess = Piece::new("princess", Color::White, "princess");
        let mut queen = Piece::new("queen", Color::White, "queen");
        queen.extra.insert("regencyHeir".into(), json!(true));
        state.board[0][0] = Some(queen);
        state.board[4][4] = Some(princess.clone());
        let from = Square { row: 4, col: 4 };
        let queen_mode = for_piece(&princess)
            .unwrap()
            .enumerate_base(&state, &princess, from)
            .unwrap();
        assert_eq!(queen_mode[0].square(), Square { row: 3, col: 3 });
        assert!(queen_mode.len() > 4);
        state.board[0][0]
            .as_mut()
            .unwrap()
            .extra
            .insert("regencyHeir".into(), json!(false));
        let bishop_step = for_piece(&princess)
            .unwrap()
            .enumerate_base(&state, &princess, from)
            .unwrap();
        assert_eq!(bishop_step.len(), 4);
        assert_eq!(bishop_step[0].square(), Square { row: 3, col: 3 });
    }

    #[test]
    fn injured_knight_jump_order_matches_frozen_source() {
        // Source knightDeltasForMove with a blocking wall at (3,4) removes
        // the first two northward jumps from this ordered candidate list.
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.board = vec![vec![None; 8]; 8];
        state.set_flag("knightInjury", Color::White, true);
        state.board[3][4] = Some(Piece::new("wall", Color::White, "blocker"));
        let from = Square { row: 4, col: 4 };
        let expected =
            [(3, 2), (3, 6), (5, 2), (5, 6), (6, 3), (6, 5)].map(|(row, col)| Square { row, col });
        for kind in ["knight", "royalKnight"] {
            let piece = Piece::new(kind, Color::White, kind);
            state.board[4][4] = Some(piece.clone());
            let actual = for_piece(&piece)
                .unwrap()
                .enumerate_base(&state, &piece, from)
                .unwrap();
            assert_eq!(
                actual.iter().map(MoveTarget::square).collect::<Vec<_>>(),
                expected,
                "{kind}"
            );
        }
        let amazon = Piece::new("amazon", Color::White, "amazon");
        state.board[4][4] = Some(amazon.clone());
        let moves = for_piece(&amazon)
            .unwrap()
            .enumerate_base(&state, &amazon, from)
            .unwrap();
        assert_eq!(
            moves
                .iter()
                .rev()
                .take(6)
                .rev()
                .map(MoveTarget::square)
                .collect::<Vec<_>>(),
            expected,
        );
    }

    #[test]
    fn v7_source_default_has_no_alibaba_or_unicorn_base_moves() {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.board = vec![vec![None; 8]; 8];
        let from = Square { row: 4, col: 4 };
        for kind in ["alibaba", "unicorn"] {
            let piece = Piece::new(kind, Color::White, kind);
            state.board[4][4] = Some(piece.clone());
            assert!(
                for_piece(&piece)
                    .unwrap()
                    .enumerate_base(&state, &piece, from)
                    .unwrap()
                    .is_empty()
            );
        }
    }
}
