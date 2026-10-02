//! 동결 v7의 승급 선택, 강제 승급, 재활용과 존버 공통 콜백.
//!
//! 직접 기물 변경과 승급 부수 효과만 소유한다. 실제 이동 재개, 추가 이동과
//! endMove는 전이 소유자가 `V7PromotionContinuation`에 따라 이어서 처리한다.
//! Source: main-OahWs0tU.js / SHA-256 e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c.

use crate::{
    Action, ActionKind, Color, EngineError, GameState, MoveTarget, Piece, RULES_VERSION_V7, Result,
    Square,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const PROMOTION_TYPES: &[&str] = &["queen", "rook", "bishop", "knight"];
const MINOR_PROMOTION_TYPES: &[&str] = &["bishop", "knight"];
// main:49264-49286. These are source piece types, not movement abilities.
const RECYCLING_EXCLUDED_TYPES: &[&str] = &[
    "bigBishop",
    "king",
    "royalKnight",
    "shotgunKing",
    "darkWizard",
    "merchant",
    "timeTraveler",
    "vampireLord",
    "colossus",
    "bigRook",
    "wall",
    "scarecrow",
    "football",
    "monster",
    "blackHole",
    "coffin",
    "crown",
    "siegeRam",
    "magicGirl",
    "berserker",
];
// main:3031-3056, used by promotionChoicesFor independently of the roulette
// balance pools. Keeping the original type order matters only in capture pools.
const MAJOR_PIECE_TYPES: &[&str] = &[
    "octopus",
    "grappler",
    "hedgehog",
    "princess",
    "bigBishop",
    "queen",
    "rook",
    "amazon",
    "man",
    "colossus",
    "bigRook",
    "herald",
    "jester",
    "hook",
    "primeMinister",
    "assassin",
    "windmill",
    "crown",
    "bear",
    "magicGirl",
    "berserker",
    "siren",
    "reaper",
    "undead",
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum V7PromotionContinuation {
    /// Source choosePromotion has performed direct effects and its religious
    /// check. The transition owner next checks Reposition, underpromotion and
    /// finally endMove using the original pending color.
    Normal {
        actor: Color,
        square: Square,
        piece_id: String,
        promoted_type: String,
        field_promotion: bool,
        terminal: bool,
    },
    /// Atomic base promotion still needs the common movePiece kernel. The
    /// pending window and selection are already cleared, as in source 92946.
    AtomicMove {
        action: Box<Action>,
        piece_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum V7LandingPromotionControl {
    /// The caller returns from movePiece and keeps its active replay capture.
    Window,
    /// Only a promotion performed at this late boundary is reported here.
    /// The caller combines it with an earlier atomic landing promotion.
    Continued { promoted: bool },
}

fn boundary(state: &GameState) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 promotion callback on rules version {}",
            state.ruleset_id
        )));
    }
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::InvalidState(
            "v7 promotion requires the adopted 8x8 board".into(),
        ));
    }
    Ok(())
}

fn owner(piece: &Piece) -> Result<Color> {
    piece.color.owner().ok_or_else(|| {
        EngineError::InvalidState(format!("v7 promotion piece {} has neutral color", piece.id))
    })
}

fn side_truth(state: &GameState, field: &str, color: Color) -> bool {
    crate::observation::truth(
        state
            .extra
            .get(field)
            .and_then(|value| value.get(color.as_str())),
    )
}

fn square_name(square: Square) -> String {
    format!("{}{}", char::from(b'a' + square.col), 8 - square.row)
}

fn collapse_depth(state: &GameState) -> u8 {
    let fallback = if crate::observation::truth(state.extra.get("collapsed")) {
        1.0
    } else {
        0.0
    };
    let depth = crate::card_effects::js_number(state.extra.get("collapseDepth"), 0)
        .filter(|value| *value != 0.0)
        .unwrap_or(fallback);
    depth.floor().clamp(0.0, 4.0) as u8
}

fn active_promotion_row(state: &GameState, color: Color) -> u8 {
    let advance = u8::from(side_truth(state, "earlyPromotion", color)) * 2
        + u8::from(side_truth(state, "fastGrowth", color)) * 3;
    if color == Color::White {
        advance.min(7)
    } else {
        7 - advance.min(7)
    }
}

fn has_reached(color: Color, row: u8, target: u8) -> bool {
    if color == Color::White {
        row <= target
    } else {
        row >= target
    }
}

/// Source 109558. The caller selects pawn-family pieces; this predicate keeps
/// the source's fanatics/noPromotion, final-weapon and collapsed-row ordering.
pub(crate) fn should_promote_v7(state: &GameState, piece: &Piece, square: Square) -> Result<bool> {
    boundary(state)?;
    let color = owner(piece)?;
    if square.row >= 8 || square.col >= 8 {
        return Err(EngineError::InvalidState(
            "v7 promotion square outside board".into(),
        ));
    }
    if piece.kind == "fanatic" || crate::observation::truth(piece.extra.get("noPromotion")) {
        return Ok(false);
    }
    let collapsed = crate::observation::truth(state.extra.get("collapsed"));
    if side_truth(state, "finalWeapon", color) {
        return Ok(!collapsed && square.row == color.promotion_row());
    }
    let depth = collapse_depth(state);
    let collapsed_row = if color == Color::White {
        depth
    } else {
        7 - depth
    };
    if (!collapsed && square.row == color.promotion_row())
        || (collapsed && square.row == collapsed_row)
    {
        return Ok(true);
    }
    Ok(
        (side_truth(state, "earlyPromotion", color) || side_truth(state, "fastGrowth", color))
            && has_reached(color, square.row, active_promotion_row(state, color)),
    )
}

/// Source 86926. This is the pre-move window predicate, independent of the
/// requested atomicBasePromotion flag and chosen promotion type. The move
/// kernel separately validates those fields with atomic_base_promotion_type_v7.
pub(crate) fn is_atomic_base_promotion_move_v7(
    state: &GameState,
    piece: &Piece,
    to: Square,
    target: &MoveTarget,
) -> Result<bool> {
    boundary(state)?;
    if !crate::observation::truth(state.extra.get("draftDelete"))
        || piece.kind != "pawn"
        || crate::observation::truth(state.extra.get("collapsed"))
    {
        return Ok(false);
    }
    let color = owner(piece)?;
    if to.row != color.promotion_row()
        || !should_promote_v7(state, piece, to)?
        || state.free_move_resolution == Some(color)
        || crate::observation::truth(state.extra.get("recycling"))
    {
        return Ok(false);
    }
    if let Some(occupant) = state.at(to) {
        if occupant.color == color.opponent()
            && crate::observation::truth(occupant.extra.get("shielded"))
        {
            return Ok(false);
        }
        if occupant.kind == "vip"
            || crate::v7_board_hazards::source_royal_identity(state, occupant)?
        {
            return Ok(false);
        }
    }
    if [
        "portalLanding",
        "portalThrough",
        "jumpCapture",
        "mistakeReverse",
    ]
    .iter()
    .any(|field| target.flag(field))
    {
        return Ok(false);
    }
    let choices = promotion_choices_for_v7(state, piece, to)?;
    Ok(choices.len() == PROMOTION_TYPES.len()
        && PROMOTION_TYPES
            .iter()
            .all(|kind| choices.iter().any(|candidate| candidate.as_str() == *kind)))
}

/// Source 91500. Only boolean true enables an atomic request; a truthy number
/// or string does not. An invalid type returns None and the caller rejects it
/// only when the request's atomicBasePromotion field is exactly true.
pub(crate) fn atomic_base_promotion_type_v7(target: &MoveTarget) -> Option<&str> {
    if target.flags.get("atomicBasePromotion") != Some(&Value::Bool(true)) {
        return None;
    }
    target
        .flags
        .get("promotion")
        .and_then(Value::as_str)
        .filter(|kind| PROMOTION_TYPES.contains(kind))
}

fn mutation_promotion(state: &GameState, piece: &Piece, square: Square) -> Result<bool> {
    let color = owner(piece)?;
    Ok(piece.kind == "pawn"
        && state
            .extra
            .get("mutation")
            .and_then(|value| value.get(color.as_str()))
            == Some(&Value::Bool(true))
        && square.row == color.promotion_row())
}

fn monochrome_type(state: &GameState, kind: &str) -> String {
    if kind == "knight" && crate::observation::truth(state.extra.get("monochromeChess")) {
        "camel".into()
    } else {
        kind.into()
    }
}

fn monochrome_choices(state: &GameState, choices: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    choices
        .into_iter()
        .map(|kind| monochrome_type(state, &kind))
        .filter(|kind| seen.insert(kind.clone()))
        .collect()
}

fn recycling_entries(state: &GameState, color: Color) -> Vec<(usize, String)> {
    state
        .captures
        .get(color.opponent())
        .iter()
        .enumerate()
        .map(|(index, piece)| (index, monochrome_type(state, &piece.kind)))
        .filter(|(_, kind)| !kind.is_empty() && !RECYCLING_EXCLUDED_TYPES.contains(&kind.as_str()))
        .collect()
}

/// Source 92704/109572. Capture-pool insertion order and first occurrence
/// de-duplication are preserved across monochrome conversion and mutation.
pub(crate) fn promotion_choices_for_v7(
    state: &GameState,
    piece: &Piece,
    square: Square,
) -> Result<Vec<String>> {
    boundary(state)?;
    let color = owner(piece)?;
    if square.row >= 8 || square.col >= 8 {
        return Err(EngineError::InvalidState(
            "v7 promotion choices square outside board".into(),
        ));
    }
    let fast_growth = side_truth(state, "fastGrowth", color)
        && has_reached(color, square.row, active_promotion_row(state, color))
        && square.row != color.promotion_row();
    let mutation = mutation_promotion(state, piece, square)?;
    if crate::observation::truth(state.extra.get("recycling")) {
        let mut seen = BTreeSet::new();
        let mut choices = recycling_entries(state, color)
            .into_iter()
            .map(|(_, kind)| kind)
            .filter(|kind| seen.insert(kind.clone()))
            .collect::<Vec<_>>();
        if mutation {
            choices.push("monster".into());
        }
        if fast_growth {
            choices.retain(|kind| !MAJOR_PIECE_TYPES.contains(&kind.as_str()));
        }
        return Ok(choices);
    }
    if side_truth(state, "finalWeapon", color)
        && !crate::observation::truth(state.extra.get("collapsed"))
        && square.row == color.promotion_row()
    {
        let mut choices = PROMOTION_TYPES
            .iter()
            .map(|kind| (*kind).into())
            .collect::<Vec<String>>();
        choices.push("amazon".into());
        if mutation {
            choices.push("monster".into());
        }
        return Ok(monochrome_choices(state, choices));
    }
    if fast_growth {
        return Ok(monochrome_choices(
            state,
            MINOR_PROMOTION_TYPES.iter().map(|kind| (*kind).into()),
        ));
    }
    let mut choices = PROMOTION_TYPES
        .iter()
        .map(|kind| (*kind).into())
        .collect::<Vec<String>>();
    if mutation {
        choices.push("monster".into());
    }
    Ok(monochrome_choices(state, choices))
}

pub(crate) fn is_minor_promotion_result_v7(state: &GameState, kind: &str) -> bool {
    MINOR_PROMOTION_TYPES.contains(&kind)
        || (kind == "camel" && crate::observation::truth(state.extra.get("monochromeChess")))
}

/// Source 92713. Removing exactly the first compatible entry is observable
/// when the capture pool contains multiple copies or a monochrome knight.
pub(crate) fn consume_recycled_promotion_piece_v7(
    state: &mut GameState,
    color: Color,
    kind: &str,
) -> Result<bool> {
    boundary(state)?;
    let Some(index) = recycling_entries(state, color)
        .into_iter()
        .find_map(|(index, candidate)| (candidate == kind).then_some(index))
    else {
        return Ok(false);
    };
    state.captures.get_mut(color.opponent()).remove(index);
    Ok(true)
}

/// Source 92710/85074. FreeMove uses the highest AI-valued recycled type;
/// stable ordering preserves the capture pool's first choice on value ties.
pub(crate) fn best_recycling_promotion_type_v7(
    state: &GameState,
    color: Color,
) -> Result<Option<String>> {
    boundary(state)?;
    let mut seen = BTreeSet::new();
    let mut candidates = recycling_entries(state, color)
        .into_iter()
        .map(|(_, kind)| kind)
        .filter(|kind| seen.insert(kind.clone()))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| ai_type_value(right).total_cmp(&ai_type_value(left)));
    Ok(candidates.into_iter().next())
}

fn ai_type_value(kind: &str) -> f64 {
    match kind {
        "pawn" | "coffin" | "babyBear" => 1.0,
        "squire" | "fanatic" => 1.2,
        "guard" | "log" => 1.5,
        "checkerKing" | "alfil" | "ferz" | "missionary" | "idol" | "lobster" => 2.0,
        "standardBearer" => 2.2,
        "checker" => 2.4,
        "camel" => 2.6,
        "knight" | "bishop" | "bat" => 3.0,
        "protestant" => 3.2,
        "recruiter" | "man" | "cannon" | "grasshopper" | "campfire" | "eagle" => 4.0,
        "knightmaster" | "windmill" => 4.5,
        "rook" | "herald" | "jester" => 5.0,
        "assassin" => 5.5,
        "princess" | "dragon" | "reaper" | "pegasus" => 6.0,
        "cardinal" | "primeMinister" => 7.0,
        "bigRook" | "bigBishop" => 8.0,
        "queen" | "hedgehog" => 9.0,
        "crown" => 9.4,
        "wizard" => 9.5,
        "hook" => 10.0,
        "bear" => 11.0,
        "colossus" | "shotgunKing" => 12.0,
        "amazon" => 14.0,
        "king" | "royalKnight" | "darkWizard" | "merchant" | "timeTraveler" | "vampireLord"
        | "vip" => 100.0,
        _ => 3.0,
    }
}

/// Source 93069. This is shared by card transformations and all promotion
/// branches, and does not remove unrelated persistent status fields.
pub(crate) fn clear_promotion_inherited_traits_v7(
    state: &mut GameState,
    piece: &mut Piece,
) -> Result<()> {
    if let Some(contract_id) = piece
        .extra
        .get("feudalContractId")
        .filter(|value| crate::observation::truth(Some(value)))
        && let Some(contracts) = state
            .extra
            .get_mut("feudalContracts")
            .and_then(Value::as_array_mut)
    {
        contracts.retain(|entry| {
            entry.get("id") != Some(contract_id)
                && entry.get("pawnId").and_then(Value::as_str) != Some(piece.id.as_str())
        });
    }
    if crate::observation::truth(piece.extra.get("potionBasicTraining")) {
        piece.extra.shift_remove("basicTraining");
    }
    if crate::observation::truth(piece.extra.get("potionManner")) {
        piece
            .extra
            .insert("coolGuyCapturedLast".into(), json!(false));
    }
    if crate::observation::truth(piece.extra.get("potionSaturation")) {
        piece.extra.insert("capturesMade".into(), json!(0));
    }
    for field in [
        "feudalContractId",
        "shielded",
        "explosive",
        "chimera",
        "witchTrial",
        "severed",
        "potionBasicTraining",
        "potionManner",
        "potionSaturation",
        "chargeRush",
        "lastSprintPending",
        "trojanHorse",
        "vipInvitation",
    ] {
        piece.extra.shift_remove(field);
    }
    Ok(())
}

/// Source 93092-93112. Coronation uses the original allegiance, then the spy
/// changes allegiance; recomputing coronation after defection loses provenance.
pub(crate) fn apply_promotion_card_reactions_v7(
    state: &GameState,
    piece: &mut Piece,
) -> Result<bool> {
    let color = owner(piece)?;
    if side_truth(state, "coronation", color) && piece.kind == "queen" {
        let previous = crate::observation::truth(piece.extra.get("protected"));
        piece.extra.insert("protected".into(), json!(true));
        piece.extra.insert(
            "coronationProtection".into(),
            json!({
                "color":color,"startTurn":state.turns_taken.get(color),"previousProtected":previous
            }),
        );
    }
    let Some(spy) = piece
        .extra
        .get("spyOwner")
        .filter(|value| crate::observation::truth(Some(value)))
    else {
        return Ok(false);
    };
    let spy = match spy.as_str() {
        Some("white") => Color::White,
        Some("black") => Color::Black,
        _ => {
            return Err(EngineError::InvalidState(format!(
                "v7 promotion piece {} spyOwner must be white or black",
                piece.id
            )));
        }
    };
    if spy == color {
        return Ok(false);
    }
    piece.color = spy.into();
    piece.extra.shift_remove("spyOwner");
    Ok(true)
}

fn write_identity(state: &mut GameState, square: Square, piece: &Piece) -> Result<()> {
    if piece.id.is_empty() {
        state.board[usize::from(square.row)][usize::from(square.col)] = Some(piece.clone());
        return Ok(());
    }
    let mut written = false;
    for candidate in state.board.iter_mut().flatten().flatten() {
        if candidate.id == piece.id {
            *candidate = piece.clone();
            written = true;
        }
    }
    if !written {
        return Err(EngineError::InvalidState(format!(
            "v7 promotion piece {} disappeared before identity update",
            piece.id
        )));
    }
    Ok(())
}

/// Source 93836. This does not set promotedFromPawn or moved: those writes
/// belong to other source branches/callers and alter canonical identity.
pub(crate) fn auto_promote_forced_pawn_v7(state: &mut GameState, square: Square) -> Result<()> {
    boundary(state)?;
    let mut next = state.clone();
    let mut piece = next.at(square).cloned().ok_or_else(|| {
        EngineError::InvalidState("v7 forced promotion square has no piece".into())
    })?;
    let color = owner(&piece)?;
    let choices = promotion_choices_for_v7(&next, &piece, square)?;
    let kind = choices
        .iter()
        .find(|kind| kind.as_str() == "queen")
        .or_else(|| choices.first())
        .cloned()
        .unwrap_or_else(|| "queen".into());
    if crate::observation::truth(next.extra.get("recycling"))
        && !consume_recycled_promotion_piece_v7(&mut next, color, &kind)?
    {
        piece.extra.insert("noPromotion".into(), json!(true));
        write_identity(&mut next, square, &piece)?;
        crate::replay::add_piece_action_log(
            &mut next,
            &piece,
            Some(square),
            None,
            format!("{}의 폰이 프로모션을 포기했습니다.", square_name(square)),
        )?;
        *state = next;
        return Ok(());
    }
    piece.kind = kind.clone();
    if kind == "pawn" {
        piece.extra.insert("noPromotion".into(), json!(true));
    } else {
        piece.extra.shift_remove("noPromotion");
    }
    piece.extra.shift_remove("holdoutPromotion");
    clear_promotion_inherited_traits_v7(&mut next, &mut piece)?;
    apply_promotion_card_reactions_v7(&next, &mut piece)?;
    crate::card_effects::mark_transformed_origin(&next, &mut piece, square)?;
    write_identity(&mut next, square, &piece)?;
    crate::replay::add_piece_action_log(
        &mut next,
        &piece,
        Some(square),
        None,
        format!(
            "{} 프로모션: {}",
            square_name(square),
            crate::replay::source_piece_label(&kind).unwrap_or(&kind)
        ),
    )?;
    crate::v7_passive_terminal::after_promotion(&mut next, "종교 승리")?;
    *state = next;
    Ok(())
}

fn holdout_ready_turn(piece: &Piece) -> f64 {
    let pending = piece.extra.get("holdoutPromotion");
    crate::card_effects::js_number(pending.and_then(|value| value.get("readyTurn")), 0)
        .or_else(|| {
            crate::card_effects::js_number(pending.and_then(|value| value.get("readyMove")), 0)
        })
        .unwrap_or(0.0)
}

/// Source 93128. This raw callback is also invoked by recycling queen loss,
/// so it has no incoming-turn/mode restriction and no additional win check.
pub(crate) fn resolve_holdout_promotions_v7(state: &mut GameState, color: Color) -> Result<usize> {
    boundary(state)?;
    let mut next = state.clone();
    let due_turn = f64::from(next.turns_taken.white.min(next.turns_taken.black));
    let mut seen = BTreeSet::new();
    let mut promoted = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(mut piece) = next.at(square).cloned() else {
                continue;
            };
            if piece.kind != "pawn"
                || piece.color != color
                || !crate::observation::truth(piece.extra.get("holdoutPromotion"))
            {
                continue;
            }
            let key = if piece.id.is_empty() {
                format!("{row}:{col}")
            } else {
                piece.id.clone()
            };
            if !seen.insert(key) || due_turn < holdout_ready_turn(&piece) {
                continue;
            }
            piece.extra.shift_remove("holdoutPromotion");
            piece.kind = "queen".into();
            piece.extra.insert("promotedFromPawn".into(), json!(true));
            piece.extra.shift_remove("noPromotion");
            clear_promotion_inherited_traits_v7(&mut next, &mut piece)?;
            piece.moved = true;
            apply_promotion_card_reactions_v7(&next, &mut piece)?;
            if !crate::observation::truth(piece.extra.get("origin")) {
                piece
                    .extra
                    .insert("origin".into(), json!(square_name(square)));
            }
            if crate::observation::truth(next.extra.get("monochromeChess")) {
                piece.extra.insert(
                    "monoShade".into(),
                    json!(if (row + col).is_multiple_of(2) {
                        "light"
                    } else {
                        "dark"
                    }),
                );
            }
            write_identity(&mut next, square, &piece)?;
            crate::card_effects::mark_animation(&mut next, &piece)?;
            promoted.push((square, piece));
        }
    }
    for (square, piece) in &promoted {
        let description =
            if crate::observation::truth(piece.extra.get("noPromotion")) && piece.kind == "pawn" {
                "프로모션을 포기".into()
            } else {
                format!(
                    "{}으로 프로모션",
                    crate::replay::source_piece_label(&piece.kind).unwrap_or(&piece.kind)
                )
            };
        crate::replay::add_piece_action_log(
            &mut next,
            piece,
            Some(*square),
            None,
            format!(
                "존버: {}의 폰이 {description}했습니다.",
                square_name(*square)
            ),
        )?;
    }
    let count = promoted.len();
    *state = next;
    Ok(count)
}

/// Source 109239. Queen-loss recycling only runs once the removed queen is
/// actually in its owner's opposing capture pool. A regency heir of queen
/// type does not satisfy the source's strict queen-identity predicate.
pub(crate) fn resolve_recycling_holdout_after_queen_loss_v7(
    state: &mut GameState,
    captured: &Piece,
) -> Result<usize> {
    boundary(state)?;
    let Some(color) = captured.color.owner() else {
        return Ok(0);
    };
    if !crate::observation::truth(state.extra.get("recycling"))
        || captured.kind != "queen"
        || captured.extra.get("regencyHeir") == Some(&Value::Bool(true))
        || !state.captures.get(color.opponent()).contains(captured)
    {
        return Ok(0);
    }
    resolve_holdout_promotions_v7(state, color)
}

/// Source 1426, used by deferred promotion and the common movement boundary.
pub(crate) fn is_field_promotion_recycling_ready_v7(state: &GameState, piece: &Piece) -> bool {
    crate::observation::truth(state.extra.get("recycling"))
        && field_capture_promotion_ready_v7(state, piece)
}

fn field_capture_promotion_ready_v7(state: &GameState, piece: &Piece) -> bool {
    let Some(color) = piece.color.owner() else {
        return false;
    };
    piece.kind == "pawn"
        && side_truth(state, "fieldPromotion", color)
        && !crate::observation::truth(piece.extra.get("specialPromotionUsed"))
        && crate::card_effects::js_number(piece.extra.get("totalCaptures"), 0)
            .unwrap_or(0.0)
            .max(0.0)
            >= 2.0
}

/// Source 92264-92283, immediately after ordinary board insertion. The move
/// kernel authenticates atomicBasePromotion at its origin; this callback does
/// not repeat that check against the already-landed/transformed piece.
pub(crate) fn apply_atomic_landing_promotion_v7(
    state: &mut GameState,
    square: Square,
    promotion_type: &str,
    privacy: Option<&Value>,
) -> Result<()> {
    boundary(state)?;
    if !PROMOTION_TYPES.contains(&promotion_type) {
        return Err(EngineError::IllegalAction);
    }
    let mut piece = state.at(square).cloned().ok_or_else(|| {
        EngineError::InvalidState("v7 atomic landing promotion has no landed piece".into())
    })?;
    owner(&piece)?;
    piece.kind = promotion_type.into();
    piece.extra.insert("promotedFromPawn".into(), json!(true));
    piece.extra.shift_remove("noPromotion");
    piece.extra.shift_remove("holdoutPromotion");
    clear_promotion_inherited_traits_v7(state, &mut piece)?;
    apply_promotion_card_reactions_v7(state, &mut piece)?;
    crate::card_effects::mark_transformed_origin(state, &mut piece, square)?;
    write_identity(state, square, &piece)?;
    crate::replay::add_piece_action_log(
        state,
        &piece,
        Some(square),
        privacy,
        format!(
            "{} 프로모션: {}",
            square_name(square),
            crate::replay::source_piece_label(promotion_type).unwrap_or(promotion_type)
        ),
    )?;
    Ok(())
}

/// Source 92434-92502. Call after movement status, notation, submerged reveal
/// and explosions, before Last Sprint and the other move continuations. These
/// effects run inside the transition owner's transaction; this helper neither
/// begins a replay move nor commits history/endMove nor adds a religious check.
pub(crate) fn resolve_landing_promotion_v7(
    state: &mut GameState,
    square: Square,
    moved_as_type: &str,
    privacy: Option<&Value>,
) -> Result<V7LandingPromotionControl> {
    boundary(state)?;
    let mut piece = state.at(square).cloned().ok_or_else(|| {
        EngineError::InvalidState("v7 landing promotion has no surviving landed piece".into())
    })?;
    let recycling = crate::observation::truth(state.extra.get("recycling"));
    if state.mode != "gameover"
        && moved_as_type == "pawn"
        && field_capture_promotion_ready_v7(state, &piece)
        && !recycling
    {
        piece
            .extra
            .insert("specialPromotionUsed".into(), json!(true));
        write_identity(state, square, &piece)?;
        // SPECIAL_PROMOTION_TYPES equals the four PROMOTION_TYPES in the
        // pinned source, independently of mutation/final-weapon choices.
        let choices = PROMOTION_TYPES
            .iter()
            .map(|kind| monochrome_type(state, kind))
            .collect::<Vec<_>>();
        state.extra.insert(
            "pendingPromotion".into(),
            json!({
                "row":square.row,"col":square.col,"color":piece.color,"choices":choices,
                "privacy":privacy,"fieldPromotion":true,
            }),
        );
        crate::flow::start_clock_for(state, owner(&piece)?)?;
        state.extra.insert("selected".into(), Value::Null);
        state.extra.insert("legalMoves".into(), json!([]));
        return Ok(V7LandingPromotionControl::Window);
    }
    if !matches!(piece.kind.as_str(), "pawn" | "squire" | "standardBearer") {
        return Ok(V7LandingPromotionControl::Continued { promoted: false });
    }
    if crate::observation::truth(piece.extra.get("desperado"))
        && should_promote_v7(state, &piece, square)?
    {
        piece.extra.insert("noPromotion".into(), json!(true));
        write_identity(state, square, &piece)?;
    }
    if !should_promote_v7(state, &piece, square)? {
        return Ok(V7LandingPromotionControl::Continued { promoted: false });
    }
    let color = owner(&piece)?;
    if state.free_move_resolution == Some(color) {
        let choices = promotion_choices_for_v7(state, &piece, square)?;
        let recycled = if recycling {
            best_recycling_promotion_type_v7(state, color)?
        } else {
            None
        };
        let kind = recycled
            .filter(|kind| choices.contains(kind))
            .or_else(|| {
                choices
                    .iter()
                    .find(|kind| kind.as_str() == "queen")
                    .cloned()
            })
            .or_else(|| choices.first().cloned())
            .unwrap_or_else(|| monochrome_type(state, "knight"));
        let mutation = kind == "monster" && mutation_promotion(state, &piece, square)?;
        if recycling && !mutation && !consume_recycled_promotion_piece_v7(state, color, &kind)? {
            piece.extra.insert("noPromotion".into(), json!(true));
            write_identity(state, square, &piece)?;
            return Ok(V7LandingPromotionControl::Continued { promoted: false });
        }
        piece.kind = kind.clone();
        piece.extra.shift_remove("noPromotion");
        clear_promotion_inherited_traits_v7(state, &mut piece)?;
        if kind == "monster" {
            piece.extra.insert("alliedMonster".into(), json!(true));
            piece.extra.insert("blackMagicMonster".into(), json!(true));
            piece
                .extra
                .insert("blackMagicOwner".into(), json!(piece.color));
        }
        piece.extra.insert("promotedFromPawn".into(), json!(true));
        apply_promotion_card_reactions_v7(state, &mut piece)?;
        crate::card_effects::mark_transformed_origin(state, &mut piece, square)?;
        write_identity(state, square, &piece)?;
        crate::replay::amend_pending_promotion_notation(state, &kind)?;
        crate::replay::add_piece_action_log(
            state,
            &piece,
            Some(square),
            privacy,
            format!(
                "프리 무브 프로모션: {}의 기물이 {}(으)로 승진했습니다.",
                square_name(square),
                crate::replay::source_piece_label(&kind).unwrap_or(&kind)
            ),
        )?;
        return Ok(V7LandingPromotionControl::Continued { promoted: true });
    }
    crate::flow::pause_clock(state)?;
    let choices = promotion_choices_for_v7(state, &piece, square)?;
    state.extra.insert("pendingPromotion".into(), json!({
        "row":square.row,"col":square.col,"color":piece.color,"choices":choices,"privacy":privacy,
    }));
    state.extra.insert("selected".into(), Value::Null);
    state.extra.insert("legalMoves".into(), json!([]));
    Ok(V7LandingPromotionControl::Window)
}

/// Source 109864. Promotion stores the move's original visibility rather than
/// recomputing it after a spy changes allegiance or the chosen type changes fog.
fn promotion_privacy_v7(state: &GameState, piece: &Piece, square: Square) -> Result<Value> {
    let mut privacy = serde_json::Map::new();
    for viewer in [Color::White, Color::Black] {
        let fog = crate::observation::fog_visible_squares_v7(state, viewer)?;
        let visible = crate::observation::piece_visible_to_color_at_v7_with_fog(
            state,
            piece,
            square,
            viewer,
            fog.as_ref(),
        )?;
        let type_known = visible
            || piece.color == viewer
            || piece.extra.get("hiddenFrom").and_then(Value::as_str) == Some(viewer.as_str());
        privacy.insert(
            viewer.as_str().into(),
            json!({"originVisible":visible,"typeKnown":type_known}),
        );
    }
    Ok(Value::Object(privacy))
}

/// Source 93030 `startDeferredPromotionIfPossible`. The reviewed oracle's
/// local-explicit-decisions profile replaces chooseAiPromotion with a no-op,
/// so raw `promotion` also stops at this window and awaits promotionChoice.
pub(crate) fn start_deferred_promotion_v7(state: &mut GameState, action: &Action) -> Result<()> {
    boundary(state)?;
    if action.kind != ActionKind::Promotion
        || action.destination.is_some()
        || action.card_id.is_some()
        || action.card_instance_id.is_some()
        || crate::card_effects::has_card_selection(action)
        || !action.extra.is_empty()
    {
        return Err(EngineError::IllegalAction);
    }
    if action.color != state.turn {
        return Err(EngineError::WrongActor);
    }
    if state.mode != "play"
        || crate::observation::truth(state.extra.get("pendingPromotion"))
        || crate::observation::truth(state.extra.get("targeting"))
    {
        return Err(EngineError::IllegalAction);
    }
    let square = action.from.ok_or(EngineError::IllegalAction)?;
    let piece = state
        .at(square)
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    if piece.color != state.turn {
        return Err(EngineError::WrongActor);
    }
    let reaches_rank = matches!(piece.kind.as_str(), "pawn" | "squire" | "standardBearer")
        && should_promote_v7(state, &piece, square)?;
    let field_promotion = is_field_promotion_recycling_ready_v7(state, &piece);
    if !reaches_rank && !field_promotion {
        return Err(EngineError::IllegalAction);
    }
    let choices = promotion_choices_for_v7(state, &piece, square)?;
    if field_promotion && !should_promote_v7(state, &piece, square)? && choices.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let mut next = state.clone();
    crate::flow::pause_clock(&mut next)?;
    let privacy = promotion_privacy_v7(&next, &piece, square)?;
    next.extra.insert(
        "pendingPromotion".into(),
        json!({
            "row":square.row,"col":square.col,"color":piece.color,"choices":choices,
            "deferred":true,"fieldPromotion":field_promotion,"privacy":privacy,
        }),
    );
    next.extra.insert("selected".into(), json!(square));
    next.extra.insert("legalMoves".into(), json!([]));
    next.extra.insert("shotgunAction".into(), json!("move"));
    next.extra.insert("shotgunPreview".into(), json!([]));
    next.extra.insert("bigRookPreview".into(), Value::Null);
    *state = next;
    Ok(())
}

fn pending_square(pending: &Value, row: &str, col: &str) -> Result<Square> {
    serde_json::from_value(json!({"row":pending.get(row),"col":pending.get(col)})).map_err(|_| {
        EngineError::InvalidState(format!("v7 pending promotion {row}/{col} outside board"))
    })
}

fn clear_promotion_selection(state: &mut GameState) {
    state.extra.insert("pendingPromotion".into(), Value::Null);
    state.extra.insert("selected".into(), Value::Null);
    state.extra.insert("legalMoves".into(), json!([]));
}

/// Source 92926. Direct normal-choice effects and atomic-move preparation are
/// shared here; the returned continuation makes movement and endMove ownership
/// explicit instead of invoking a second, partial turn implementation.
pub(crate) fn apply_pending_promotion_choice_v7(
    state: &mut GameState,
    action: &Action,
) -> Result<V7PromotionContinuation> {
    boundary(state)?;
    if action.kind != ActionKind::PromotionChoice
        || action.from.is_some()
        || action.destination.is_some()
        || action.card_id.is_some()
        || action.card_instance_id.is_some()
        || crate::card_effects::has_card_selection(action)
        || action.extra.len() != 1
    {
        return Err(EngineError::IllegalAction);
    }
    let kind = action
        .extra
        .get("promotionType")
        .and_then(Value::as_str)
        .ok_or(EngineError::IllegalAction)?
        .to_owned();
    let pending = state
        .extra
        .get("pendingPromotion")
        .filter(|value| crate::observation::truth(Some(value)))
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    let color = match pending.get("color").and_then(Value::as_str) {
        Some("white") => Color::White,
        Some("black") => Color::Black,
        _ => {
            return Err(EngineError::InvalidState(
                "v7 pending promotion color must be white or black".into(),
            ));
        }
    };
    if color != action.color {
        return Err(EngineError::WrongActor);
    }
    if let Some(choices) = pending
        .get("choices")
        .filter(|value| crate::observation::truth(Some(value)))
    {
        let choices = choices.as_array().ok_or_else(|| {
            EngineError::InvalidState("v7 pending promotion choices must be an array".into())
        })?;
        if !choices
            .iter()
            .any(|choice| choice.as_str() == Some(kind.as_str()))
        {
            return Err(EngineError::IllegalAction);
        }
    }
    let mut next = state.clone();
    if let Some(atomic) = pending
        .get("atomicMove")
        .filter(|value| crate::observation::truth(Some(value)))
    {
        let from = pending_square(atomic, "fromRow", "fromCol")?;
        let to = pending_square(atomic, "toRow", "toCol")?;
        let piece_id = atomic
            .get("sourcePieceId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                EngineError::InvalidState("v7 atomic promotion sourcePieceId missing".into())
            })?
            .to_owned();
        let piece = next
            .at(from)
            .filter(|piece| piece.id == piece_id && piece.color == color)
            .ok_or(EngineError::IllegalAction)?;
        let raw = atomic
            .get("move")
            .filter(|value| value.is_object())
            .cloned()
            .ok_or_else(|| {
                EngineError::InvalidState("v7 atomic promotion move payload missing".into())
            })?;
        let mut destination: MoveTarget =
            serde_json::from_value(raw).map_err(EngineError::serialization)?;
        destination.row = to.row;
        destination.col = to.col;
        destination.flags.insert("promotion".into(), json!(kind));
        destination
            .flags
            .insert("atomicBasePromotion".into(), json!(true));
        if next.mode != "play"
            || next.turn != color
            || atomic_base_promotion_type_v7(&destination).is_none()
            || !is_atomic_base_promotion_move_v7(&next, piece, to, &destination)?
        {
            return Err(EngineError::IllegalAction);
        }
        clear_promotion_selection(&mut next);
        crate::flow::start_clock(&mut next)?;
        *state = next;
        return Ok(V7PromotionContinuation::AtomicMove {
            action: Box::new(Action::movement(color, from, destination)),
            piece_id,
        });
    }
    let square = pending_square(&pending, "row", "col")?;
    let mut piece = next.at(square).cloned().ok_or(EngineError::IllegalAction)?;
    if piece.color != color {
        return Err(EngineError::WrongActor);
    }
    let field_promotion = crate::observation::truth(pending.get("fieldPromotion"));
    let mutation =
        kind == "monster" && !field_promotion && mutation_promotion(&next, &piece, square)?;
    if crate::observation::truth(next.extra.get("recycling"))
        && !mutation
        && !consume_recycled_promotion_piece_v7(&mut next, color, &kind)?
    {
        return Err(EngineError::IllegalAction);
    }
    piece.kind = kind.clone();
    if kind != "pawn" {
        piece.extra.insert("promotedFromPawn".into(), json!(true));
    }
    if field_promotion {
        piece
            .extra
            .insert("specialPromotionUsed".into(), json!(true));
    }
    if kind == "pawn" {
        piece.extra.insert("noPromotion".into(), json!(true));
    } else {
        piece.extra.shift_remove("noPromotion");
    }
    piece.extra.shift_remove("holdoutPromotion");
    clear_promotion_inherited_traits_v7(&mut next, &mut piece)?;
    if kind == "monster" {
        piece.extra.insert("alliedMonster".into(), json!(true));
        piece.extra.insert("blackMagicMonster".into(), json!(true));
        piece
            .extra
            .insert("blackMagicOwner".into(), json!(piece.color));
    }
    if apply_promotion_card_reactions_v7(&next, &mut piece)? {
        write_identity(&mut next, square, &piece)?;
        crate::replay::add_piece_action_log(
            &mut next,
            &piece,
            Some(square),
            pending.get("privacy"),
            format!("{}의 스파이가 정체를 드러냈습니다.", square_name(square)),
        )?;
    }
    crate::card_effects::mark_transformed_origin(&next, &mut piece, square)?;
    piece.moved = true;
    write_identity(&mut next, square, &piece)?;
    crate::replay::add_piece_action_log(
        &mut next,
        &piece,
        Some(square),
        pending.get("privacy"),
        // choosePromotion uses TYPE_LABELS[type] without the fallback used by
        // forced/atomic/free-move promotion. A missing source label formats as
        // undefined; the faithful initialization profile supplies monster.
        format!(
            "{} 프로모션: {}",
            square_name(square),
            crate::replay::source_piece_label(&kind).unwrap_or("undefined")
        ),
    )?;
    crate::replay::amend_pending_promotion_notation(&mut next, &kind)?;
    clear_promotion_selection(&mut next);
    let terminal = crate::v7_passive_terminal::after_promotion(&mut next, "종교 승리")?;
    let piece_id = piece.id;
    *state = next;
    Ok(V7PromotionContinuation::Normal {
        actor: color,
        square,
        piece_id,
        promoted_type: kind,
        field_promotion,
        terminal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;

    fn baseline() -> GameState {
        crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            37,
        )
        .unwrap()
    }

    fn source_body(state: &GameState) -> Value {
        let mut value = serde_json::to_value(state).unwrap();
        for envelope in ["rulesetId", "rng", "history"] {
            value.as_object_mut().unwrap().remove(envelope);
        }
        value
    }

    fn differences(actual: &Value, expected: &Value, at: &str, output: &mut Vec<String>) {
        if output.len() >= 12
            || serde_jcs::to_vec(actual).unwrap() == serde_jcs::to_vec(expected).unwrap()
        {
            return;
        }
        match (actual, expected) {
            (Value::Object(actual), Value::Object(expected)) => {
                for key in actual
                    .keys()
                    .chain(expected.keys())
                    .collect::<BTreeSet<_>>()
                {
                    match (actual.get(key), expected.get(key)) {
                        (Some(actual), Some(expected)) => {
                            differences(actual, expected, &format!("{at}.{key}"), output)
                        }
                        _ => output.push(format!("{at}.{key}: field presence differs")),
                    }
                    if output.len() >= 12 {
                        break;
                    }
                }
            }
            (Value::Array(actual), Value::Array(expected)) => {
                if actual.len() != expected.len() {
                    output.push(format!(
                        "{at}.length: {} != {}",
                        actual.len(),
                        expected.len()
                    ));
                }
                for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                    differences(actual, expected, &format!("{at}[{index}]"), output);
                    if output.len() >= 12 {
                        break;
                    }
                }
            }
            _ => output.push(format!("{at}: {actual} != {expected}")),
        }
    }

    #[test]
    fn rejected_spy_color_preserves_recycled_piece_and_state() {
        let mut state = baseline();
        let at = Square { row: 0, col: 0 };
        let mut pawn = state.at(Square { row: 6, col: 0 }).unwrap().clone();
        state.board[6][0] = None;
        pawn.extra.insert("spyOwner".into(), json!(true));
        state.board[0][0] = Some(pawn);
        state.extra.insert("recycling".into(), json!(true));
        state
            .captures
            .black
            .push(Piece::new("queen", Color::White, "recycled-source-queen"));
        let before = state.clone();
        let error = auto_promote_forced_pawn_v7(&mut state, at).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("spyOwner must be white or black")
        );
        assert_eq!(
            state, before,
            "a failed reaction must retain the capture pool and RNG"
        );
    }

    #[test]
    fn atomic_origin_query_is_pure_and_separate_from_selected_type() {
        let state = baseline();
        let before = state.clone();
        let piece = state.at(Square { row: 6, col: 0 }).unwrap();
        let to = Square { row: 0, col: 1 };
        let mut target = MoveTarget::at(to);
        target
            .flags
            .insert("atomicBasePromotion".into(), json!(true));
        target.flags.insert("promotion".into(), json!("monster"));
        assert!(is_atomic_base_promotion_move_v7(&state, piece, to, &target).unwrap());
        assert_eq!(atomic_base_promotion_type_v7(&target), None);
        target.flags.insert("promotion".into(), json!("knight"));
        assert_eq!(atomic_base_promotion_type_v7(&target), Some("knight"));
        target.flags.insert("atomicBasePromotion".into(), json!(1));
        assert_eq!(atomic_base_promotion_type_v7(&target), None);
        assert!(is_atomic_base_promotion_move_v7(&state, piece, to, &target).unwrap());
        target
            .flags
            .insert("portalThrough".into(), json!("source-portal"));
        assert!(!is_atomic_base_promotion_move_v7(&state, piece, to, &target).unwrap());
        assert_eq!(
            state, before,
            "availability must preserve state, RNG and history"
        );
    }

    #[test]
    fn queen_loss_requires_actual_capture_membership_and_non_heir_identity() {
        let mut state = baseline();
        state.extra.insert("recycling".into(), json!(true));
        state.board[6][0]
            .as_mut()
            .unwrap()
            .extra
            .insert("holdoutPromotion".into(), json!({"readyTurn":0}));
        let mut queen = Piece::new("queen", Color::White, "queen-loss-probe");
        let before = state.clone();
        assert_eq!(
            resolve_recycling_holdout_after_queen_loss_v7(&mut state, &queen).unwrap(),
            0
        );
        assert_eq!(state, before);
        queen.extra.insert("regencyHeir".into(), json!(true));
        state.captures.black.push(queen.clone());
        let before = state.clone();
        assert_eq!(
            resolve_recycling_holdout_after_queen_loss_v7(&mut state, &queen).unwrap(),
            0
        );
        assert_eq!(state, before);
        queen.extra.shift_remove("regencyHeir");
        state.captures.black[0] = queen.clone();
        let rng = state.rng.clone();
        assert_eq!(
            resolve_recycling_holdout_after_queen_loss_v7(&mut state, &queen).unwrap(),
            1
        );
        assert_eq!(state.board[6][0].as_ref().unwrap().kind, "queen");
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn deferred_field_window_keeps_piece_capture_pool_rng_and_history() {
        let mut state = baseline();
        let at = Square { row: 6, col: 0 };
        state.extra.insert("recycling".into(), json!(true));
        state
            .extra
            .insert("fieldPromotion".into(), json!({"white":true,"black":false}));
        state
            .at_mut(at)
            .unwrap()
            .extra
            .insert("totalCaptures".into(), json!(2));
        state.captures.black.push(Piece::new(
            "bishop",
            Color::White,
            "recycling-window-bishop",
        ));
        let action: Action =
            serde_json::from_value(json!({"type":"promotion","color":"white","from":at})).unwrap();
        let piece = state.at(at).cloned().unwrap();
        let captures = state.captures.clone();
        let rng = state.rng.clone();
        let history = state.history.clone();
        start_deferred_promotion_v7(&mut state, &action).unwrap();
        assert_eq!(state.at(at), Some(&piece));
        assert_eq!(state.captures, captures);
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
        let pending = &state.extra["pendingPromotion"];
        assert_eq!(pending["choices"], json!(["bishop"]));
        assert_eq!(pending["deferred"], true);
        assert_eq!(pending["fieldPromotion"], true);
        assert_eq!(state.extra["selected"], json!(at));
        assert_eq!(pending["privacy"]["white"]["typeKnown"], true);
    }

    #[test]
    fn deferred_field_window_with_no_choices_rejects_without_pausing_clock() {
        let mut state = baseline();
        let at = Square { row: 6, col: 0 };
        state.extra.insert("recycling".into(), json!(true));
        state
            .extra
            .insert("fieldPromotion".into(), json!({"white":true,"black":false}));
        state
            .at_mut(at)
            .unwrap()
            .extra
            .insert("totalCaptures".into(), json!(2));
        let action: Action =
            serde_json::from_value(json!({"type":"promotion","color":"white","from":at})).unwrap();
        let before = state.clone();
        assert_eq!(
            start_deferred_promotion_v7(&mut state, &action),
            Err(EngineError::IllegalAction)
        );
        assert_eq!(state, before);
    }

    /// The main integration agent generates this receipt outside Git from
    /// the actual pinned client. An absent receipt is an explicit ignored
    /// check, never a source-parity success.
    #[test]
    #[ignore = "requires root-generated ACCELERATE_V7_PROMOTION_CASES receipt"]
    fn frozen_promotion_callbacks_match_full_state_rng_and_history() {
        let path = std::env::var_os("ACCELERATE_V7_PROMOTION_CASES")
            .expect("main agent must provide a source-pinned promotion receipt");
        let source = std::fs::read_to_string(path).expect("promotion receipt must be readable");
        let mut checked = 0;
        let mut failures = Vec::new();
        for (index, line) in source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .enumerate()
        {
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            let id = receipt["id"].as_str().unwrap();
            let fixture = &receipt["fixture"];
            let square: Square = serde_json::from_value(fixture["square"].clone()).unwrap();
            let color: Color = serde_json::from_value(fixture["color"].clone()).unwrap();
            let mut state: GameState =
                serde_json::from_value(receipt["sourcePosition"]["state"].clone()).unwrap();
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(receipt["sourcePosition"]["rng"].clone()).unwrap();
            state.history = receipt["sourcePosition"]["history"]
                .as_array()
                .unwrap()
                .clone();
            let piece = state.at(square).unwrap();
            let mut mismatch = Vec::new();
            match should_promote_v7(&state, piece, square) {
                Ok(actual) => differences(
                    &json!(actual),
                    &receipt["sourceQuery"]["shouldPromote"],
                    "query.shouldPromote",
                    &mut mismatch,
                ),
                Err(error) => mismatch.push(format!("query.shouldPromote: {error}")),
            }
            match promotion_choices_for_v7(&state, piece, square) {
                Ok(actual) => differences(
                    &json!(actual),
                    &receipt["sourceQuery"]["choices"],
                    "query.choices",
                    &mut mismatch,
                ),
                Err(error) => mismatch.push(format!("query.choices: {error}")),
            }
            let kind = receipt["kind"].as_str().unwrap();
            let operation: Result<()> = match kind {
                "query" => Ok(()),
                "forced" => auto_promote_forced_pawn_v7(&mut state, square),
                "holdout" => resolve_holdout_promotions_v7(&mut state, color).map(|count| {
                    differences(
                        &json!(count),
                        &receipt["sourceResult"],
                        "result.count",
                        &mut mismatch,
                    )
                }),
                "queen-loss" => {
                    let captured = state.captures.get(color.opponent())[0].clone();
                    resolve_recycling_holdout_after_queen_loss_v7(&mut state, &captured).map(
                        |count| {
                            differences(
                                &json!(count),
                                &receipt["sourceResult"],
                                "result.count",
                                &mut mismatch,
                            )
                        },
                    )
                }
                "deferred" => {
                    let action: Action = serde_json::from_value(
                        json!({"type":"promotion","color":color,"from":square}),
                    )
                    .unwrap();
                    let result = start_deferred_promotion_v7(&mut state, &action);
                    differences(
                        &json!(result.is_ok()),
                        &receipt["sourceResult"],
                        "result.started",
                        &mut mismatch,
                    );
                    match result {
                        Ok(()) | Err(EngineError::IllegalAction) => Ok(()),
                        Err(error) => Err(error),
                    }
                }
                "choice" | "atomic" => {
                    let action: Action = serde_json::from_value(json!({"type":"promotionChoice","color":color,"promotionType":fixture["type"]})).unwrap();
                    apply_pending_promotion_choice_v7(&mut state, &action).map(|outcome| {
                        differences(
                            &json!(matches!(
                                outcome,
                                V7PromotionContinuation::AtomicMove { .. }
                            )),
                            &json!(kind == "atomic"),
                            "result.atomicContinuation",
                            &mut mismatch,
                        )
                    })
                }
                other => panic!("unexpected promotion receipt kind {other}"),
            };
            if let Err(error) = operation {
                mismatch.push(format!("operation: {error}"));
            }
            let actual = source_body(&state);
            let expected = &receipt["sourceDirectPosition"]["state"];
            if serde_jcs::to_vec(&actual).unwrap() != serde_jcs::to_vec(expected).unwrap() {
                differences(&actual, expected, "state", &mut mismatch);
            }
            if json!(state.rng) != receipt["sourceDirectPosition"]["rng"] {
                mismatch.push("rng differs".into());
            }
            if json!(state.history) != receipt["sourceDirectPosition"]["history"] {
                mismatch.push("history differs".into());
            }
            if !mismatch.is_empty() {
                failures.push(format!("receipt {index} {id}: {}", mismatch.join("; ")));
            }
            checked += 1;
        }
        assert!(
            checked >= 36,
            "promotion receipt must include the adopted 36 query/effect/window boundaries, got {checked}"
        );
        assert!(
            failures.is_empty(),
            "{} of {checked} promotion receipts differ:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    /// The move owner verifies its surrounding move ordering separately. This
    /// receipt covers the exact atomic/late source blocks and their callbacks,
    /// including execution-only freeMove context, without invoking endMove.
    #[test]
    #[ignore = "requires root-generated ACCELERATE_V7_LANDING_PROMOTION_CASES receipt"]
    fn frozen_landing_promotion_boundaries_match_full_state_rng_and_history() {
        let path = std::env::var_os("ACCELERATE_V7_LANDING_PROMOTION_CASES")
            .expect("main agent must provide a source-pinned landing promotion receipt");
        let source =
            std::fs::read_to_string(path).expect("landing promotion receipt must be readable");
        let mut checked = 0;
        let mut failures = Vec::new();
        for (index, line) in source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .enumerate()
        {
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            assert_eq!(
                receipt["fixtureKind"],
                "synthetic-contract-frozen-source-slice"
            );
            let id = receipt["id"].as_str().unwrap();
            let fixture = &receipt["fixture"];
            let square: Square = serde_json::from_value(fixture["square"].clone()).unwrap();
            let color: Color = serde_json::from_value(fixture["color"].clone()).unwrap();
            let mut state: GameState =
                serde_json::from_value(receipt["sourcePosition"]["state"].clone()).unwrap();
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(receipt["sourcePosition"]["rng"].clone()).unwrap();
            state.history = receipt["sourcePosition"]["history"]
                .as_array()
                .unwrap()
                .clone();
            let free_move = fixture["freeMove"] == true;
            state.free_move_resolution = free_move.then_some(color);
            let privacy = fixture.get("privacy");
            let kind = receipt["kind"].as_str().unwrap();
            assert_eq!(
                receipt["sourceRange"]["name"], kind,
                "{id} source block identity"
            );
            let operation = match kind {
                "atomic-landing" => apply_atomic_landing_promotion_v7(
                    &mut state,
                    square,
                    fixture["type"].as_str().unwrap(),
                    privacy,
                )
                .map(|()| json!({"window":false,"promoted":true})),
                "late-landing" => resolve_landing_promotion_v7(
                    &mut state,
                    square,
                    fixture["movedAsType"].as_str().unwrap_or("pawn"),
                    privacy,
                )
                .map(|control| match control {
                    V7LandingPromotionControl::Window => json!({"window":true}),
                    V7LandingPromotionControl::Continued { promoted } => {
                        json!({"window":false,"promoted":promoted})
                    }
                }),
                other => panic!("unexpected landing promotion receipt kind {other}"),
            };
            let mut mismatch = Vec::new();
            match operation {
                Ok(result) => {
                    differences(&result, &receipt["sourceResult"], "result", &mut mismatch)
                }
                Err(error) => mismatch.push(format!("operation: {error}")),
            }
            let actual = source_body(&state);
            let expected = &receipt["sourceDirectPosition"]["state"];
            if serde_jcs::to_vec(&actual).unwrap() != serde_jcs::to_vec(expected).unwrap() {
                differences(&actual, expected, "state", &mut mismatch);
            }
            if json!(state.rng) != receipt["sourceDirectPosition"]["rng"] {
                mismatch.push("rng differs".into());
            }
            if json!(state.history) != receipt["sourceDirectPosition"]["history"] {
                mismatch.push("history differs".into());
            }
            if state.free_move_resolution != free_move.then_some(color) {
                mismatch.push("free move execution context differs".into());
            }
            if !mismatch.is_empty() {
                failures.push(format!("receipt {index} {id}: {}", mismatch.join("; ")));
            }
            checked += 1;
        }
        assert!(
            checked >= 17,
            "landing promotion receipt must include the adopted 17 atomic/field/normal/free-move boundaries, got {checked}"
        );
        assert!(
            failures.is_empty(),
            "{} of {checked} landing promotion receipts differ:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    #[ignore = "requires root-generated ACCELERATE_V7_ATOMIC_ORIGIN_CASES receipt"]
    fn frozen_atomic_origin_queries_match_source_without_mutation() {
        let path = std::env::var_os("ACCELERATE_V7_ATOMIC_ORIGIN_CASES")
            .expect("main agent must provide a source-pinned atomic origin receipt");
        let source = std::fs::read_to_string(path).expect("atomic origin receipt must be readable");
        let mut checked = 0;
        let mut failures = Vec::new();
        for (index, line) in source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .enumerate()
        {
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            let id = receipt["id"].as_str().unwrap();
            let fixture = &receipt["fixture"];
            let from: Square = serde_json::from_value(fixture["from"].clone()).unwrap();
            let to: Square = serde_json::from_value(fixture["to"].clone()).unwrap();
            let target: MoveTarget = serde_json::from_value(fixture["move"].clone()).unwrap();
            let mut state: GameState =
                serde_json::from_value(receipt["sourcePosition"]["state"].clone()).unwrap();
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(receipt["sourcePosition"]["rng"].clone()).unwrap();
            state.history = receipt["sourcePosition"]["history"]
                .as_array()
                .unwrap()
                .clone();
            if fixture["freeMove"] == true {
                let color: Color = serde_json::from_value(fixture["color"].clone()).unwrap();
                state.free_move_resolution = Some(color);
            }
            let before = state.clone();
            let piece = state.at(from).unwrap();
            let mut mismatch = Vec::new();
            match is_atomic_base_promotion_move_v7(&state, piece, to, &target) {
                Ok(actual) => differences(
                    &json!(actual),
                    &receipt["sourceResult"]["available"],
                    "query.available",
                    &mut mismatch,
                ),
                Err(error) => mismatch.push(format!("query.available: {error}")),
            }
            // The source's empty string and native None both mean no atomic
            // chosen type; the strict boolean flag is checked independently.
            let chosen_type = atomic_base_promotion_type_v7(&target).unwrap_or("");
            differences(
                &json!(chosen_type),
                &receipt["sourceResult"]["type"],
                "query.type",
                &mut mismatch,
            );
            if state != before {
                mismatch.push("native query changed state/RNG/history/context".into());
            }
            let actual = source_body(&state);
            let expected = &receipt["sourceDirectPosition"]["state"];
            if serde_jcs::to_vec(&actual).unwrap() != serde_jcs::to_vec(expected).unwrap() {
                differences(&actual, expected, "state", &mut mismatch);
            }
            if json!(state.rng) != receipt["sourceDirectPosition"]["rng"] {
                mismatch.push("rng differs".into());
            }
            if json!(state.history) != receipt["sourceDirectPosition"]["history"] {
                mismatch.push("history differs".into());
            }
            if !mismatch.is_empty() {
                failures.push(format!("receipt {index} {id}: {}", mismatch.join("; ")));
            }
            checked += 1;
        }
        assert!(
            checked >= 24,
            "atomic origin receipt must include the adopted 24 availability/type guards, got {checked}"
        );
        assert!(
            failures.is_empty(),
            "{} of {checked} atomic origin receipts differ:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    fn spy_private_context(state: &GameState) -> Value {
        json!({
            "replayCaptureColor":state.active_move_replay_before.as_ref().map(|capture| capture.actor),
            "freeMoveColor":state.free_move_resolution,
            "stateTurn":state.turn,
            "pendingColor":state.extra.get("pendingPromotion").and_then(|pending| pending.get("color")).cloned().unwrap_or(Value::Null),
            "activeHistoryMoveNumber":state.extra.get("activeHistoryMoveNumber").cloned().unwrap_or(Value::Null),
        })
    }

    fn assert_spy_source_witness(fixture: &Value, path: &Value) {
        let id = fixture["id"].as_str().unwrap();
        let boundary = path["boundary"].as_str().unwrap();
        let kind = fixture["kind"].as_str().unwrap();
        let steps = path["steps"].as_array().unwrap();
        assert_eq!(
            steps.len(),
            if kind == "normal-promotion" { 2 } else { 1 },
            "{id}/{boundary} source step count"
        );
        let trace = path["trace"].as_array().unwrap();
        let end_move = trace
            .iter()
            .filter(|entry| entry["event"] == "endMove-enter")
            .collect::<Vec<_>>();
        assert_eq!(
            end_move.len(),
            1,
            "{id}/{boundary} exact source endMove call"
        );
        let end_move = end_move[0];
        let expected_actor = if matches!(kind, "due-free-move" | "atomic-promotion") {
            "black"
        } else {
            "white"
        };
        assert_eq!(
            fixture["originalActor"], "white",
            "{id} original action actor"
        );
        assert_eq!(
            fixture["expectedEndMoveActor"], expected_actor,
            "{id} fixture settlement actor"
        );
        assert_eq!(
            end_move["actor"], expected_actor,
            "{id}/{boundary} original source settlement actor"
        );
        assert_eq!(
            end_move["stateTurn"], "white",
            "{id}/{boundary} source must not assign Spy's new color to state.turn before endMove"
        );
        let after = &steps.last().unwrap()["after"];
        let piece_id = fixture["moverId"].as_str().unwrap();
        let mover_cells = after["state"]["board"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .flat_map(|(row, cells)| {
                cells
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .map(move |(col, piece)| (row, col, piece))
            })
            .filter(|(_, _, piece)| piece["id"].as_str() == Some(piece_id))
            .collect::<Vec<_>>();
        assert_eq!(
            mover_cells.len(),
            1,
            "{id}/{boundary} source mover identity"
        );
        let (row, col, mover) = mover_cells[0];
        for step in steps {
            assert_ne!(
                step["before"]["positionId"], step["after"]["positionId"],
                "{id}/{boundary} positive source step"
            );
            if step["api"] == "applyAiAction" {
                assert_eq!(
                    step["result"]["value"]["ok"], true,
                    "{id}/{boundary} source action success"
                );
            }
        }
        if kind == "stationary-hp" {
            assert_eq!(
                mover["color"], "white",
                "{id}/{boundary} Transcendence retains allegiance"
            );
            assert_ne!(
                mover["type"], "pawn",
                "{id}/{boundary} capture Transcendence changed type"
            );
            assert_eq!(
                mover["spyOwner"], "black",
                "{id}/{boundary} HP upgrade does not run promotion Spy reactions"
            );
            assert_eq!(end_move["moverColor"], "white");
            assert_eq!(
                json!({"row":row,"col":col}),
                fixture["from"],
                "{id}/{boundary} HP attack stays at its origin"
            );
            let to: Square = serde_json::from_value(fixture["to"].clone()).unwrap();
            assert!(
                after["state"]["board"][to.row as usize][to.col as usize].is_null(),
                "{id}/{boundary} HP1 target was removed"
            );
        } else {
            assert_eq!(
                mover["color"], "black",
                "{id}/{boundary} Spy changes allegiance"
            );
            assert_eq!(
                mover["type"], "queen",
                "{id}/{boundary} promoted source type"
            );
            assert!(
                mover.get("spyOwner").is_none(),
                "{id}/{boundary} Spy mark consumed"
            );
            assert_eq!(end_move["moverColor"], "black");
            assert_eq!(
                json!({"row":row,"col":col}),
                fixture["to"],
                "{id}/{boundary} source landing"
            );
        }
        match kind {
            "normal-promotion" => {
                assert_eq!(
                    steps[0]["after"]["state"]["pendingPromotion"]["color"], "white",
                    "{id}/{boundary} original pending actor"
                );
                assert_eq!(
                    steps[1]["action"]["color"], "white",
                    "{id}/{boundary} public choice actor"
                );
                assert_eq!(
                    steps[1]["before"]["positionId"], steps[0]["after"]["positionId"],
                    "{id}/{boundary} exact pending continuation"
                );
                assert!(
                    after["state"]["pendingPromotion"].is_null(),
                    "{id}/{boundary} promotion window closed"
                );
                assert_eq!(
                    steps[1]["privateBefore"]["replayCaptureColor"],
                    if boundary == "continuous-vm" {
                        json!("white")
                    } else {
                        Value::Null
                    },
                    "{id}/{boundary} private replay boundary"
                );
            }
            "due-free-move" => {
                assert_eq!(steps[0]["callbackActor"], "black");
                assert_eq!(
                    steps[0]["result"]["value"],
                    json!({"executed":1,"canceled":0}),
                    "{id}/{boundary} exactly one generated plan executes"
                );
                assert_eq!(
                    end_move["freeMoveColor"], "white",
                    "{id}/{boundary} original free-move execution owner"
                );
                assert_eq!(
                    after["state"]["turn"], path["before"]["state"]["turn"],
                    "{id}/{boundary} due callback restores its incoming turn"
                );
            }
            "atomic-promotion" => {
                assert!(
                    trace
                        .iter()
                        .any(|entry| entry["event"] == "commitReplay-enter"
                            && entry["actor"] == "black"
                            && entry["replayCaptureColor"] == "white"),
                    "{id}/{boundary} source replay mismatch witness"
                );
                assert!(
                    trace
                        .iter()
                        .any(|entry| entry["event"] == "commitReplay-exit"
                            && entry["actor"] == "black"
                            && entry["returned"] == false
                            && entry["replayCaptureColor"].is_null()),
                    "{id}/{boundary} mismatched replay is canceled"
                );
                assert!(
                    after["state"]["moveReplay"]["white"].is_null(),
                    "{id}/{boundary} no white replay from a black settlement"
                );
            }
            "stationary-hp" => {}
            other => panic!("unexpected Spy witness kind {other}"),
        }
    }

    fn compare_spy_fresh_step(step: &Value) -> std::result::Result<Vec<String>, String> {
        let before = &step["before"];
        let expected = &step["after"];
        let host = crate::v7_host::V7HostPosition::from_envelope(before.clone())
            .map_err(|error| format!("source-before import: {error:?}"))?;
        // Fresh source VM import has no active replay/free-move scope. A
        // continuous VM's white replay capture is deliberately not fabricated.
        if !step["privateBefore"]["replayCaptureColor"].is_null()
            || !step["privateBefore"]["freeMoveColor"].is_null()
        {
            return Err("fresh source receipt has a live private replay/free-move scope".into());
        }
        let mut mismatch = Vec::new();
        differences(
            &spy_private_context(host.state()),
            &step["privateBefore"],
            "privateBefore",
            &mut mismatch,
        );
        let before_export = host
            .export_envelope()
            .map_err(|error| format!("source-before export: {error:?}"))?;
        if serde_jcs::to_vec(&before_export).unwrap() != serde_jcs::to_vec(before).unwrap() {
            differences(&before_export, before, "before", &mut mismatch);
        }
        // Source after identity is validated independently of the native result.
        crate::v7_host::V7HostPosition::from_envelope(expected.clone())
            .map_err(|error| format!("source-after import: {error:?}"))?;
        let action = if matches!(
            step["api"].as_str(),
            Some("applyAiAction" | "choosePromotion")
        ) {
            Some(
                serde_json::from_value::<Action>(step["action"].clone())
                    .map_err(|error| format!("source action decode: {error}"))?,
            )
        } else {
            None
        };
        if let Some(action) = &action {
            let expected_kind = if step["api"] == "applyAiAction" {
                ActionKind::Move
            } else {
                ActionKind::PromotionChoice
            };
            if action.kind != expected_kind {
                return Err(format!(
                    "source Spy action kind {:?} is not {expected_kind:?}",
                    action.kind
                ));
            }
        }
        let callback_actor = if step["api"] == "resolvePendingFreeMovesAfterTurn" {
            Some(
                serde_json::from_value::<Color>(step["callbackActor"].clone())
                    .map_err(|error| format!("source due actor decode: {error}"))?,
            )
        } else {
            None
        };
        let (next, ()) = host
            .transact(host.position_id(), |state| match step["api"].as_str() {
                Some("applyAiAction" | "choosePromotion") => {
                    crate::transition::apply_without_public_event(state, action.as_ref().unwrap())
                        .map(|_| ())
                }
                Some("resolvePendingFreeMovesAfterTurn") => {
                    crate::v7_queued_effects::after_capture_reset(state, callback_actor.unwrap())
                        .map(|_| ())
                }
                other => Err(EngineError::InvalidState(format!(
                    "unexpected Spy full-source callback {other:?}"
                ))),
            })
            .map_err(|error| format!("native execution/commit: {error:?}"))?;
        let actual = next
            .export_envelope()
            .map_err(|error| format!("native-after export: {error:?}"))?;
        // Every semantic field is compared; no replay/cache/log/history fields
        // are removed to make the fresh import match a continuous source VM.
        if serde_jcs::to_vec(&actual).unwrap() != serde_jcs::to_vec(expected).unwrap() {
            differences(&actual["state"], &expected["state"], "state", &mut mismatch);
            for field in [
                "rng",
                "history",
                "positionId",
                "protocolVersion",
                "rulesVersion",
                "catalogVersion",
            ] {
                if serde_jcs::to_vec(&actual[field]).unwrap()
                    != serde_jcs::to_vec(&expected[field]).unwrap()
                {
                    mismatch.push(format!(
                        "{field} differs: {} != {}",
                        actual[field], expected[field]
                    ));
                }
            }
        }
        differences(
            &spy_private_context(next.state()),
            &step["privateAfter"],
            "privateAfter",
            &mut mismatch,
        );
        if serde_jcs::to_vec(
            &host
                .export_envelope()
                .map_err(|error| format!("retained before export: {error:?}"))?,
        )
        .unwrap()
            != serde_jcs::to_vec(before).unwrap()
        {
            mismatch.push("immutable input Position changed".into());
        }
        Ok(mismatch)
    }

    /// The fresh-VM boundary has an explicit empty private replay scope and a
    /// canonical source Position import. Continuous source VM receipts retain
    /// live insertion order/private globals and are source witnesses only here.
    /// Their four cases are never counted as native parity by this test.
    #[test]
    #[ignore = "requires root-generated ACCELERATE_V7_SPY_MOVEMENT_CASES faithful-init-v1 receipt"]
    fn frozen_spy_full_movement_fresh_import_matches_state_rng_history_and_position_id() {
        let path = std::env::var_os("ACCELERATE_V7_SPY_MOVEMENT_CASES")
            .expect("main agent must provide the faithful Spy full-source receipt");
        let source =
            std::fs::read_to_string(path).expect("Spy full-source receipt must be readable");
        let receipt: Value = serde_json::from_str(&source).unwrap();
        assert_eq!(
            receipt["sourceSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        assert_eq!(
            receipt["oracleProfileVersion"],
            "accelerate-headless-semantic-v7-faithful-init-v1"
        );
        assert_eq!(receipt["count"], 4);
        assert_eq!(receipt["completed"], 4);
        assert_eq!(receipt["failures"], 0);
        let cases = receipt["cases"].as_array().unwrap();
        assert_eq!(
            cases.len(),
            4,
            "all four positive Spy source cases are required"
        );
        let expected_ids = [
            "normal-spy-pawn-window-then-queen",
            "due-free-move-spy-automatic-queen",
            "synthetic-atomic-spy-replay-owner-mismatch",
            "stationary-hp-spy-transcendence-keeps-color",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        let mut actual_ids = BTreeSet::new();
        let mut failures = Vec::new();
        let mut attempted_steps = 0;
        let mut matched_steps = 0;
        let mut matched_cases = 0;
        let mut continuous_source_witnesses = 0;
        for case in cases {
            let id = case["id"].as_str().unwrap();
            assert!(actual_ids.insert(id), "duplicate Spy source case {id}");
            let expected_kind = match id {
                "normal-spy-pawn-window-then-queen" => "normal-promotion",
                "due-free-move-spy-automatic-queen" => "due-free-move",
                "synthetic-atomic-spy-replay-owner-mismatch" => "atomic-promotion",
                "stationary-hp-spy-transcendence-keeps-color" => "stationary-hp",
                other => panic!("unexpected Spy source case {other}"),
            };
            assert_eq!(
                case["fixture"]["kind"], expected_kind,
                "{id} required source contract"
            );
            assert_eq!(
                case["completed"], true,
                "{id} source execution must complete"
            );
            assert_eq!(case["sourceSha256"], receipt["sourceSha256"]);
            assert_eq!(
                case["oracleProfileVersion"],
                receipt["oracleProfileVersion"]
            );
            let paths = case["paths"].as_array().unwrap();
            assert_eq!(paths.len(), 2, "{id} must preserve both source boundaries");
            let fresh = paths
                .iter()
                .filter(|path| path["boundary"] == "fresh-vm-restore-per-step")
                .collect::<Vec<_>>();
            let continuous = paths
                .iter()
                .filter(|path| path["boundary"] == "continuous-vm")
                .collect::<Vec<_>>();
            assert_eq!(fresh.len(), 1);
            assert_eq!(continuous.len(), 1);
            let fresh = fresh[0];
            let continuous = continuous[0];
            assert_spy_source_witness(&case["fixture"], fresh);
            assert_spy_source_witness(&case["fixture"], continuous);
            assert_eq!(
                fresh["before"]["positionId"], continuous["before"]["positionId"],
                "{id} same canonical starting Position"
            );
            // The actual source evidence makes these boundaries observably
            // distinct, including one-step cases with empty initial captures.
            assert_ne!(
                fresh["after"]["positionId"], continuous["after"]["positionId"],
                "{id} retain the distinct live source insertion-order/private boundary"
            );
            continuous_source_witnesses += 1;
            let failures_before = failures.len();
            for step in fresh["steps"].as_array().unwrap() {
                attempted_steps += 1;
                let step_id = step["id"].as_str().unwrap();
                match compare_spy_fresh_step(step) {
                    Ok(mismatch) if mismatch.is_empty() => matched_steps += 1,
                    Ok(mismatch) => {
                        failures.push(format!("{id}/{step_id}: {}", mismatch.join("; ")))
                    }
                    Err(error) => failures.push(format!("{id}/{step_id}: {error}")),
                }
            }
            if failures.len() == failures_before {
                matched_cases += 1;
            }
        }
        assert_eq!(
            actual_ids, expected_ids,
            "the adopted four Spy cases must be present exactly once"
        );
        assert_eq!(
            attempted_steps, 5,
            "all five fresh native steps must execute; errors are never skipped"
        );
        assert_eq!(
            continuous_source_witnesses, 4,
            "continuous VM receipts are preserved as source witnesses only"
        );
        assert!(
            failures.is_empty() && matched_steps == 5 && matched_cases == 4,
            "Spy fresh-import native parity matched {matched_cases}/4 cases and {matched_steps}/5 steps; continuous native parity is not claimed:\n{}",
            failures.join("\n")
        );
    }
}
