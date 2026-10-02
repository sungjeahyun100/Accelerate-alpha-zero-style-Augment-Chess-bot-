use crate::*;
#[path = "movement_objects.rs"]
mod objects;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(crate) const ORTHO: &[(i8, i8)] = &[(-1, 0), (1, 0), (0, -1), (0, 1)];
pub(crate) const DIAG: &[(i8, i8)] = &[(-1, -1), (-1, 1), (1, -1), (1, 1)];
pub(crate) const KING: &[(i8, i8)] = &[
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -1),
    (0, 1),
    (1, -1),
    (1, 0),
    (1, 1),
];
pub(crate) const KNIGHT: &[(i8, i8)] = &[
    (-2, -1),
    (-2, 1),
    (-1, -2),
    (-1, 2),
    (1, -2),
    (1, 2),
    (2, -1),
    (2, 1),
];
const CAMEL: &[(i8, i8)] = &[
    (-3, -1),
    (-3, 1),
    (-1, -3),
    (-1, 3),
    (1, -3),
    (1, 3),
    (3, -1),
    (3, 1),
];
const EAGLE: &[(i8, i8)] = &[
    (-2, -2),
    (-2, 0),
    (-2, 2),
    (0, -2),
    (0, 2),
    (2, -2),
    (2, 0),
    (2, 2),
];

/// main3469 isBoardCorner는 8×8의 네 모서리 2×2 영역을 뜻한다.
/// b1/g1 및 b8/g8도 CornerKick 이동·공격의 source 영역에 포함된다.
pub(crate) fn v7_is_board_corner(from: Square) -> bool {
    from.row < 8
        && from.col < 8
        && (from.row < 2 || from.row >= 6)
        && (from.col < 2 || from.col >= 6)
}

pub fn implemented_piece_types() -> &'static [&'static str] {
    &[
        "pawn",
        "squire",
        "standardBearer",
        "rook",
        "bishop",
        "queen",
        "king",
        "knight",
        "royalKnight",
        "unicorn",
        "amazon",
        "man",
        "guard",
        "camel",
        "alfil",
        "ferz",
        "eagle",
        "alibaba",
        "knightmaster",
        "cannon",
        "grasshopper",
        "princess",
        "clockwork",
        "campfire",
        "wall",
        "coffin",
        "scarecrow",
        "checker",
        "checkerKing",
        "missionary",
        "bigRook",
        "bigBishop",
    ]
}

pub(crate) fn legal_actions(state: &GameState) -> Result<Vec<Action>> {
    if state.result().is_some() || state.mode == "gameover" {
        return Ok(Vec::new());
    }
    if state.mode == "draft" {
        return crate::draft::legal_actions(state);
    }
    if let Some(pending) = state
        .extra
        .get("pendingPromotion")
        .filter(|window| !window.is_null())
    {
        let color: Color = serde_json::from_value(
            pending
                .get("color")
                .cloned()
                .ok_or_else(|| EngineError::InvalidState("promotion color missing".into()))?,
        )
        .map_err(EngineError::serialization)?;
        let choices = pending
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| EngineError::InvalidState("promotion choices missing".into()))?;
        return choices
            .iter()
            .map(|choice| {
                let kind = choice.as_str().ok_or_else(|| {
                    EngineError::UnsupportedFeature("compound promotion choice".into())
                })?;
                if !implemented_piece_types().contains(&kind) {
                    return Err(EngineError::UnsupportedFeature(format!(
                        "promotion result {kind}"
                    )));
                }
                let mut action = Action::movement(
                    color,
                    Square { row: 0, col: 0 },
                    MoveTarget::at(Square { row: 0, col: 0 }),
                );
                action.kind = ActionKind::PromotionChoice;
                action.from = None;
                action.destination = None;
                action.extra.insert("promotionType".into(), json!(kind));
                Ok(action)
            })
            .collect();
    }
    if state.mode != "play" {
        return Err(EngineError::UnsupportedFeature(format!(
            "game mode {}",
            state.mode
        )));
    }
    let mut actions = legal_move_actions(state)?;
    if !has_checker_capture(state)? && forced_piece_id(state).is_none() {
        for card in state.deck_slots.get(state.turn) {
            if usable_card(card) {
                actions.extend(crate::transition::card_actions(state, card)?);
            }
        }
    }
    Ok(actions)
}

/// Source collectValidAiActions(includeCards:false) has a separate movement
/// surface. Unrelated unknown card families must not poison royal threat probes.
pub(crate) fn legal_move_actions(state: &GameState) -> Result<Vec<Action>> {
    ensure_supported(state)?;
    legal_move_candidates(state)
}

/// Movement-only first-play profile observed in the frozen v7 headless
/// adapter. This does not enumerate cards or authorize Position execution.
/// The three source-reachable active-only draft samples had the same 20 raw
/// moves, and all 20 survived actionStream(legal=true). The complete public
/// action stream and post-apply state/RNG still belong to the Position gate.
pub(crate) fn v7_opening_legal_move_actions(state: &GameState) -> Result<Vec<Action>> {
    ensure_v7_orthodox_opening(state)?;
    let moves = legal_move_candidates(state)?;
    if moves != v7_expected_orthodox_first_play_moves() {
        return Err(EngineError::UnsupportedFeature(
            "v7 opening movement differs from the source-verified orthodox profile".into(),
        ));
    }
    Ok(moves)
}

/// Frozen v7 actionStream movement prefix, shared by the three verified
/// active-only first-play samples. Compare the complete ordered payload so a
/// future candidate change cannot silently preserve only its count and flags.
fn v7_expected_orthodox_first_play_moves() -> Vec<Action> {
    let mut actions = Vec::with_capacity(20);
    for col in 0..8 {
        let from = Square { row: 6, col };
        actions.push(Action::movement(
            Color::White,
            from,
            MoveTarget::at(Square { row: 5, col }),
        ));
        let mut double = MoveTarget::at(Square { row: 4, col });
        double
            .flags
            .insert("standardPawnDoubleStep".into(), json!(true));
        actions.push(Action::movement(Color::White, from, double));
    }
    for (from_col, to_col) in [(1, 0), (1, 2), (6, 5), (6, 7)] {
        actions.push(Action::movement(
            Color::White,
            Square {
                row: 7,
                col: from_col,
            },
            MoveTarget::at(Square {
                row: 5,
                col: to_col,
            }),
        ));
    }
    actions
}

/// Validate an unbound movement payload against the same narrow source
/// profile. Binding, card actions, and execution remain separate boundaries.
#[cfg(test)]
pub(crate) fn v7_opening_validate_move(state: &GameState, action: &Action) -> Result<()> {
    if action.position_key.is_some() {
        return Err(EngineError::IllegalAction);
    }
    if v7_opening_legal_move_actions(state)?.contains(action) {
        Ok(())
    } else {
        Err(EngineError::IllegalAction)
    }
}

/// Ordered movement-only actions for the source-verified seed-19 normal
/// first-play position after White uses Relay. This is a staged candidate
/// surface: Position binding, swap execution, cards and general v7 play remain
/// behind their separate guards.
pub(crate) fn v7_opening_relay_legal_move_actions(state: &GameState) -> Result<Vec<Action>> {
    ensure_v7_relay_after_opening(state)?;
    let moves = legal_move_candidates_with_relay(state)?;
    let swaps = moves
        .iter()
        .filter(|action| {
            action
                .destination
                .as_ref()
                .is_some_and(|to| to.flag("relaySwap"))
        })
        .count();
    let base_moves = moves
        .iter()
        .filter(|action| {
            action
                .destination
                .as_ref()
                .is_none_or(|to| !to.flag("relaySwap"))
        })
        .cloned()
        .collect::<Vec<_>>();
    if moves.len() != 148 || swaps != 128 || base_moves != v7_expected_orthodox_first_play_moves() {
        return Err(EngineError::UnsupportedFeature(
            "v7 Relay first-play candidates differ from source-verified 148 moves".into(),
        ));
    }
    Ok(moves)
}

/// The movement-only source profiles verified from reachable first-play
/// Positions. Card actions, Position binding and applying a move are separate
/// contracts; none of these variants opens the general v7 public legal gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum V7VerifiedFirstPlayProfile {
    Normal,
    Chaos,
    Grand,
    NormalRelayAfter,
}

/// Source AiNoCards는 모든 행마 후보가 빈 경우에만 friendly-crush를
/// 재조회한다. UI 관측 및 공개 행동 bind는 각자의 별도 계약을 쓴다.
pub(crate) fn v7_ai_no_cards_move_candidates(
    state: &GameState,
    color: Color,
) -> Result<Vec<Action>> {
    if state.ruleset_id != RULES_VERSION_V7 || state.mode != "play" || state.turn != color {
        return Err(EngineError::UnsupportedFeature(
            "v7 AiNoCards movement requires an active matching play actor".into(),
        ));
    }
    let actions = v7_collect_movement_candidates(state, false)?;
    if actions.is_empty() {
        v7_collect_movement_candidates(state, true)
    } else {
        Ok(actions)
    }
}

/// 동결 원본 iterateCandidatePayloads의 행마 prefix다. 행마 후보와
/// 승급·Shotgun·Wizard·추가 이동을 원본 순서로 모으며 활성 미지원 분기는
/// 부분 목록을 내보내지 않고 정확한 오류를 반환한다.
pub(crate) fn v7_source_ordered_move_candidates(
    state: &GameState,
    allow_friendly_crush: bool,
) -> Result<Vec<Action>> {
    if state.ruleset_id != RULES_VERSION_V7 || state.mode != "play" || state.result().is_some() {
        return Err(EngineError::UnsupportedFeature(
            "v7 ordered movement requires a nonterminal play Position".into(),
        ));
    }
    if state
        .extra
        .get("pendingPromotion")
        .is_some_and(|window| !window.is_null())
        || state
            .extra
            .get("activeTrolley")
            .is_some_and(|window| !window.is_null())
    {
        return Err(EngineError::UnsupportedFeature(
            "v7 ordered movement is superseded by a pending choice".into(),
        ));
    }
    v7_collect_movement_candidates(state, allow_friendly_crush)
}

pub(crate) fn v7_normalize_origin(piece: &Piece, square: Square) -> Result<Square> {
    if !piece.is_large() {
        return Ok(square);
    }
    let row = piece
        .extra
        .get("anchorRow")
        .and_then(Value::as_f64)
        .filter(|v| v.fract() == 0.0 && (0.0..7.0).contains(v));
    let col = piece
        .extra
        .get("anchorCol")
        .and_then(Value::as_f64)
        .filter(|v| v.fract() == 0.0 && (0.0..7.0).contains(v));
    match (row, col) {
        (Some(row), Some(col)) => Ok(Square {
            row: row as u8,
            col: col as u8,
        }),
        _ => Err(EngineError::InvalidState(format!(
            "v7 {} lacks a valid 2x2 anchor",
            piece.kind
        ))),
    }
}

fn v7_collect_movement_candidates(
    state: &GameState,
    allow_friendly_crush: bool,
) -> Result<Vec<Action>> {
    if state.mode != "play" || crate::observation::truth(state.extra.get("activeTrolley")) {
        return Ok(Vec::new());
    }
    let forced = v7_forced_piece_id(state, state.turn);
    let mut actions = Vec::new();
    if let Some(piece) = forced.and_then(|id| {
        state
            .board
            .iter()
            .flatten()
            .flatten()
            .find(|piece| piece.id == id)
    }) && [
        "ironMonarchExtraMove",
        "rookLiftSecondMove",
        "fileSurgeSecondMove",
        "madHorseSecondMove",
        "platformExtraMove",
    ]
    .iter()
    .any(|field| crate::observation::truth(piece.extra.get(*field)))
    {
        let from = find_square(state, &piece.id)
            .ok_or_else(|| EngineError::InvalidState("v7 forced mover absent".into()))?;
        actions.push(v7_piece_action(state.turn, from, ActionKind::FileSurgeSkip));
    }
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            let Some(piece) = state.at(at) else {
                continue;
            };
            if piece.color != state.turn && piece.kind != "football"
                || piece.kind == "wall"
                || seen.contains(&piece.id)
                || forced.is_some_and(|id| id != piece.id)
                || piece.color == state.turn && frozen(piece)
            {
                continue;
            }
            seen.insert(piece.id.clone());
            let from = v7_normalize_origin(piece, at)?;
            if forced.is_none() && piece.color == state.turn {
                let field_promotion =
                    crate::v7_promotion::is_field_promotion_recycling_ready_v7(state, piece);
                let reached_rank =
                    matches!(piece.kind.as_str(), "pawn" | "squire" | "standardBearer")
                        && v7_should_promote(state, piece, from.row)?;
                if reached_rank
                    || field_promotion
                        && !crate::v7_promotion::promotion_choices_for_v7(state, piece, from)?
                            .is_empty()
                {
                    actions.push(v7_piece_action(state.turn, from, ActionKind::Promotion));
                }
            }
            let targets = v7_collect_ai_moves_for_piece(state, piece, from)?;
            for target in targets {
                if !v7_ai_useful_move(state, piece, from, &target, allow_friendly_crush)? {
                    continue;
                }
                let mut action = Action::movement(state.turn, from, target);
                if allow_friendly_crush {
                    action
                        .extra
                        .insert("forcedFriendlyCrush".into(), json!(true));
                }
                actions.push(action);
            }
            if forced.is_none() {
                if piece.kind == "shotgunKing"
                    && crate::observation::number(piece.extra.get("ammo")).unwrap_or(0.0)
                        < crate::observation::number(piece.extra.get("maxAmmo")).unwrap_or(3.0)
                {
                    actions.push(v7_piece_action(state.turn, from, ActionKind::ShotgunReload));
                }
                if piece.ability_kind() == "wizard" {
                    let zugzwang = state.flag("zugzwang", state.turn)
                        && (!state.flag("democracy", state.turn)
                            || state.board.iter().flatten().flatten().any(|royal| {
                                royal.color == state.turn && state.royal_identity(royal)
                            }));
                    if !zugzwang {
                        let mana =
                            crate::observation::number(piece.extra.get("mana")).unwrap_or(0.0);
                        // The adapter's completeWizardActions expands the UI
                        // spell selection space before transition admission.
                        for (spell, cost) in [
                            ("meteor", 3.0),
                            ("lightning", 1.0),
                            ("shield", 2.0),
                            ("timeStop", 5.0),
                        ] {
                            if mana < cost {
                                continue;
                            }
                            if spell == "timeStop" {
                                let mut action =
                                    v7_piece_action(state.turn, from, ActionKind::WizardSpell);
                                action.extra.insert("spellId".into(), json!(spell));
                                action.target = Some(json!(from));
                                actions.push(action);
                            } else {
                                let edge = if spell == "meteor" { 7 } else { 8 };
                                for row in 0..edge {
                                    for col in 0..edge {
                                        let mut action = v7_piece_action(
                                            state.turn,
                                            from,
                                            ActionKind::WizardSpell,
                                        );
                                        action.extra.insert("spellId".into(), json!(spell));
                                        action.target = Some(json!({"row":row,"col":col}));
                                        actions.push(action);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if actions.len() > 100_000 {
                return Err(EngineError::InvalidState(
                    "v7 movement family exceeds source default 100000 candidate budget".into(),
                ));
            }
        }
    }
    Ok(actions)
}

/// Source collectAiMovesForPiece expands the three shotgun selection modes
/// before the separate usefulness policy. The caller's Position is unchanged.
pub(crate) fn v7_collect_ai_moves_for_piece(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    if piece.kind != "shotgunKing" {
        return v7_legal_move_targets(state, piece, from, V7MoveOptions::default());
    }
    let mut targets = Vec::new();
    for mode in ["move", "shotgun", "snipe"] {
        let mut probe = state.clone();
        probe.extra.insert("shotgunAction".into(), json!(mode));
        targets.extend(v7_legal_move_targets(
            &probe,
            piece,
            from,
            V7MoveOptions::default(),
        )?);
    }
    Ok(targets)
}

fn v7_piece_action(color: Color, from: Square, kind: ActionKind) -> Action {
    let mut action = Action::movement(color, from, MoveTarget::at(from));
    action.kind = kind;
    action.destination = None;
    action
}

fn v7_should_promote(state: &GameState, piece: &Piece, row: u8) -> Result<bool> {
    if piece.flag("noPromotion") || piece.kind == "fanatic" {
        return Ok(false);
    }
    let Some(actor) = piece.color.owner() else {
        return Ok(false);
    };
    let collapsed = crate::observation::truth(state.extra.get("collapsed"));
    let promotion_row = if actor == Color::White { 0 } else { 7 };
    if state.flag("finalWeapon", actor) {
        return Ok(!collapsed && row == promotion_row);
    }
    if !collapsed && row == promotion_row {
        return Ok(true);
    }
    let depth = v7_collapse_depth(state);
    if collapsed
        && row
            == if actor == Color::White {
                depth
            } else {
                7 - depth
            }
    {
        return Ok(true);
    }
    let advance = u8::from(state.flag("earlyPromotion", actor)) * 2
        + u8::from(state.flag("fastGrowth", actor)) * 3;
    let accelerated = advance > 0;
    let accelerated_row = if actor == Color::White {
        advance.min(7)
    } else {
        7 - advance.min(7)
    };
    Ok(accelerated
        && if actor == Color::White {
            row <= accelerated_row
        } else {
            row >= accelerated_row
        })
}

fn v7_ai_useful_move(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
    allow_friendly_crush: bool,
) -> Result<bool> {
    if target.flag("colossusBody") {
        return Ok(false);
    }
    if target.flag("bigRookMove") && !allow_friendly_crush {
        for row in target.row..target.row.saturating_add(2).min(8) {
            for col in target.col..target.col.saturating_add(2).min(8) {
                if state
                    .at(Square { row, col })
                    .is_some_and(|other| other.id != piece.id && other.color == piece.color)
                {
                    return Ok(false);
                }
            }
        }
    }
    if v7_black_hole_cells(state)?.contains(&target.square()) {
        return Ok(false);
    }
    let useful_victim = |at: Square| {
        state.at(at).is_some_and(|victim| {
            victim.color == state.turn.opponent()
                && !matches!(victim.ability_kind(), "guard" | "jester" | "revolvingDoor")
                && can_capture(state, piece, victim)
        })
    };
    if target.flag("colossusAttack") {
        return Ok(useful_victim(target.square()));
    }
    if target.flag("shotgunSnipe") {
        return Ok(piece.number("ammo") >= 3);
    }
    if target.flag("shotgunBlast") {
        if piece.number("ammo") < 1 {
            return Ok(false);
        }
        let direction = target
            .flags
            .get("shotgunDirection")
            .and_then(Value::as_array)
            .filter(|values| values.len() == 2)
            .ok_or_else(|| {
                EngineError::InvalidState("v7 shotgun candidate lacks direction".into())
            })?;
        let dr = direction[0]
            .as_i64()
            .filter(|value| (-1..=1).contains(value))
            .ok_or(EngineError::IllegalAction)? as i8;
        let dc = direction[1]
            .as_i64()
            .filter(|value| (-1..=1).contains(value))
            .ok_or(EngineError::IllegalAction)? as i8;
        let offsets = if dr == 0 {
            [(-1, 0), (0, 0), (1, 0)]
        } else if dc == 0 {
            [(0, -1), (0, 0), (0, 1)]
        } else {
            [(0, 0), (-dr, 0), (0, -dc)]
        };
        let depth = if dr == 0 || dc == 0 { 3 } else { 2 };
        for distance in 1..=depth {
            let selected = if distance == 3 {
                &offsets[1..2]
            } else {
                &offsets[..]
            };
            if selected.iter().any(|&(sr, sc)| {
                from.offset(dr * distance + sr, dc * distance + sc)
                    .is_some_and(useful_victim)
            }) {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    Ok(true)
}

/// main61421 normalizeBlackHole는 배열만 읽고 좌표에 Number를 적용한다.
/// 호출자는 정수 보드 칸의 membership을 조회하므로 소수 좌표는 일치하지
/// 않으며, 단일 객체를 임의로 한 칸짜리 배열로 확대하지 않는다.
pub(crate) fn v7_black_hole_cells(state: &GameState) -> Result<Vec<Square>> {
    let Some(cells) = state.extra.get("blackHole").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    Ok(cells
        .iter()
        .filter_map(|cell| {
            let row = crate::card_effects::js_number(cell.get("row"), 0)?;
            let col = crate::card_effects::js_number(cell.get("col"), 0)?;
            (row.is_finite()
                && col.is_finite()
                && row.fract() == 0.0
                && col.fract() == 0.0
                && (0.0..8.0).contains(&row)
                && (0.0..8.0).contains(&col))
            .then_some(Square {
                row: row as u8,
                col: col as u8,
            })
        })
        .collect())
}

/// Source `collectValidAiActions(cardsOnly:true)` adds cards only while no
/// forced extra move is active. Checker captures and other move constraints
/// are evaluated later by the source transition; they do not erase the card
/// candidate family. This reports only that collection window, not legality.
pub(crate) fn v7_card_action_window_open(state: &GameState) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 card candidate window requires the v7 rules version".into(),
        ));
    }
    if state.mode != "play"
        || state.result().is_some()
        || crate::observation::truth(state.extra.get("draftDelete"))
        || crate::observation::truth(state.extra.get("activeTrolley"))
    {
        return Ok(false);
    }
    const FORCED_EXTRA_MOVE: &[&str] = &[
        "thiefSecondMove",
        "frenzyExtraMove",
        "fileSurgeSecondMove",
        "rookLiftSecondMove",
        "ironMonarchExtraMove",
        "underpromotionSecondMove",
        "checkerChainCapture",
        "madHorseSecondMove",
        "platformExtraMove",
        "desperado",
    ];
    Ok(!state.board.iter().flatten().flatten().any(|piece| {
        piece.color == state.turn
            && (piece
                .extra
                .get("repositionSecondMove")
                .is_some_and(|reposition| {
                    crate::observation::truth(reposition.get("used"))
                        || crate::observation::truth(reposition.get("forced"))
                })
                || FORCED_EXTRA_MOVE
                    .iter()
                    .any(|field| crate::observation::truth(piece.extra.get(*field))))
    }))
}

/// Frozen publicHints는 실제 보드 셀 순서와 각 body alias를 보존한다.
/// AI의 기물 ID 중복 제거·중립 Football 규약과 별도인 공개 관측 투영이다.
pub(crate) fn v7_ui_move_hints(
    state: &GameState,
    viewer: Color,
) -> Result<Vec<(Square, Vec<Square>)>> {
    if state.ruleset_id != RULES_VERSION_V7 || state.mode != "play" || state.turn != viewer {
        return Err(EngineError::UnsupportedFeature(
            "v7 UI movement hints require an active matching play viewer".into(),
        ));
    }
    let mut grouped = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            let Some(piece) = state.at(at) else {
                continue;
            };
            if piece.color != viewer
                || !crate::observation::piece_visible_to_color_at_v7(state, piece, at, viewer)?
            {
                continue;
            }
            let mut destinations = Vec::new();
            for target in v7_legal_move_targets(state, piece, at, V7MoveOptions::default())? {
                for square in highlight_cells(&target)? {
                    if !destinations.contains(&square) {
                        destinations.push(square);
                    }
                }
            }
            grouped.push((at, destinations));
        }
    }
    grouped.retain(|(_, destinations)| !destinations.is_empty());
    Ok(grouped)
}

pub(crate) fn v7_verified_first_play_movement_actions(
    state: &GameState,
) -> Result<(V7VerifiedFirstPlayProfile, Vec<Action>)> {
    let relay = state.extra.get("relay");
    if relay == Some(&json!({"black":false,"white":true})) {
        return Ok((
            V7VerifiedFirstPlayProfile::NormalRelayAfter,
            v7_opening_relay_legal_move_actions(state)?,
        ));
    }
    if relay != Some(&json!({"black":false,"white":false})) {
        return Err(EngineError::UnsupportedFeature(
            "v7 first-play Relay/Solidarity profile".into(),
        ));
    }
    let actions = v7_opening_legal_move_actions(state)?;
    let profile = match state.extra.get("gameStyle").and_then(Value::as_str) {
        Some("normal") => V7VerifiedFirstPlayProfile::Normal,
        Some("chaos") => V7VerifiedFirstPlayProfile::Chaos,
        Some("grand") => V7VerifiedFirstPlayProfile::Grand,
        _ => {
            return Err(EngineError::UnsupportedFeature(
                "v7 first-play game style".into(),
            ));
        }
    };
    Ok((profile, actions))
}

/// In the verified orthodox profile, a Relay exchange selects a different
/// allied single-cell piece on the same row or column. This descriptor pins
/// the target identity so an eventual executor can revalidate it against the
/// action's position; Solidarity and altered footprints are outside the gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RelaySwap {
    #[cfg(test)]
    pub(crate) from: Square,
    pub(crate) to: Square,
    #[cfg(test)]
    mover_id: String,
    #[cfg(test)]
    pub(crate) target_id: String,
}

impl RelaySwap {
    fn for_pair(state: &GameState, mover: &Piece, from: Square, to: Square) -> Option<Self> {
        let current = state.at(from)?;
        let target = state.at(to)?;
        if current.id != mover.id
            || !state.flag("relay", mover.color)
            || state.flag("solidarity", mover.color)
            || mover.id == target.id
            || mover.color != target.color
            || from.row != to.row && from.col != to.col
            || [mover, target].into_iter().any(|piece| {
                piece.is_large()
                    || piece.ability_kind() == "slime"
                    || matches!(
                        piece.kind.as_str(),
                        "wall" | "football" | "blackHole" | "monster" | "coffin"
                    )
            })
        {
            return None;
        }
        Some(Self {
            #[cfg(test)]
            from,
            to,
            #[cfg(test)]
            mover_id: mover.id.clone(),
            #[cfg(test)]
            target_id: target.id.clone(),
        })
    }

    #[cfg(test)]
    pub(crate) fn from_target(
        state: &GameState,
        mover: &Piece,
        from: Square,
        target: &MoveTarget,
    ) -> Result<Option<Self>> {
        if !target.flag("relaySwap") {
            return Ok(None);
        }
        if target.flags.len() != 1 || target.flags.get("relaySwap") != Some(&json!(true)) {
            return Err(EngineError::IllegalAction);
        }
        Self::for_pair(state, mover, from, target.square())
            .map(Some)
            .ok_or(EngineError::IllegalAction)
    }

    fn target(&self) -> MoveTarget {
        let mut target = MoveTarget::at(self.to);
        target.flags.insert("relaySwap".into(), json!(true));
        target
    }

    /// Exchange two occupied cells without changing either identity or the
    /// target piece. The source marks only the acting piece as moved. A stale
    /// descriptor must not be applied to a different occupant.
    #[cfg(test)]
    fn board_after(&self, state: &GameState) -> Result<Vec<Vec<Option<Piece>>>> {
        let mover = state.at(self.from).ok_or(EngineError::IllegalAction)?;
        if Self::for_pair(state, mover, self.from, self.to).as_ref() != Some(self) {
            return Err(EngineError::IllegalAction);
        }
        let mut board = state.board.clone();
        let mut mover = board[self.from.row as usize][self.from.col as usize]
            .take()
            .ok_or(EngineError::IllegalAction)?;
        let other = board[self.to.row as usize][self.to.col as usize]
            .take()
            .ok_or(EngineError::IllegalAction)?;
        if mover.id != self.mover_id || other.id != self.target_id {
            return Err(EngineError::IllegalAction);
        }
        mover.moved = true;
        board[self.from.row as usize][self.from.col as usize] = Some(other);
        board[self.to.row as usize][self.to.col as usize] = Some(mover);
        Ok(board)
    }
}

fn relay_swap_targets(state: &GameState, mover: &Piece, from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let to = Square { row, col };
            if let Some(swap) = RelaySwap::for_pair(state, mover, from, to) {
                moves.push(swap.target());
            }
        }
    }
    moves
}

/// Proves only that the source-shaped first white play has a legal pawn move.
/// This witness neither enumerates nor authorizes actions. `None` means the
/// position is outside the proven profile and the general v7 guard stays closed.
pub(crate) fn v7_first_play_has_legal_move_witness(
    state: &GameState,
    color: Color,
) -> Result<Option<bool>> {
    if color != Color::White || state.ruleset_id != RULES_VERSION_V7 {
        return Ok(None);
    }
    let verified = if state.extra.get("relay") == Some(&json!({"black":false,"white":true})) {
        ensure_v7_relay_after_opening(state)
    } else if state.extra.get("relay") == Some(&json!({"black":false,"white":false})) {
        ensure_v7_orthodox_opening(state)
    } else {
        return Ok(None);
    };
    match verified {
        Ok(()) => {}
        Err(EngineError::UnsupportedFeature(_)) => return Ok(None),
        Err(error) => return Err(error),
    }
    let from = Square { row: 6, col: 0 };
    let pawn = state
        .at(from)
        .ok_or_else(|| EngineError::InvalidState("source-shaped first-play pawn missing".into()))?;
    let forward = MoveTarget::at(Square { row: 5, col: 0 });
    if piece_moves(state, pawn, from)?.contains(&forward) {
        Ok(Some(true))
    } else {
        Err(EngineError::InvalidState(
            "source-shaped first-play pawn witness disappeared".into(),
        ))
    }
}

fn legal_move_candidates(state: &GameState) -> Result<Vec<Action>> {
    legal_move_candidates_inner(state, false)
}

fn legal_move_candidates_with_relay(state: &GameState) -> Result<Vec<Action>> {
    legal_move_candidates_inner(state, true)
}

fn legal_move_candidates_inner(state: &GameState, relay: bool) -> Result<Vec<Action>> {
    let mut actions = Vec::new();
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let from = Square { row, col };
            let Some(piece) = state.at(from) else {
                continue;
            };
            if piece.color != state.turn || !seen.insert(piece.id.clone()) {
                continue;
            }
            if frozen(piece)
                || piece
                    .extra
                    .get("staked")
                    .and_then(|s| s.get("remaining"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    > 0
            {
                continue;
            }
            if relay {
                for target in relay_swap_targets(state, piece, from) {
                    actions.push(Action::movement(state.turn, from, target));
                }
            }
            for target in piece_moves(state, piece, from)? {
                actions.push(Action::movement(state.turn, from, target));
            }
        }
    }
    let has_checker_capture = actions.iter().any(|action| {
        action
            .destination
            .as_ref()
            .is_some_and(|target| target.flag("checkerCapture"))
    });
    if has_checker_capture {
        actions.retain(|action| {
            action
                .destination
                .as_ref()
                .is_some_and(|target| target.flag("checkerCapture"))
        });
    }
    if let Some(forced_id) = forced_piece_id(state) {
        actions.retain(|action| {
            action
                .from
                .and_then(|square| state.at(square))
                .is_some_and(|piece| piece.id == forced_id)
        });
    }
    Ok(actions)
}

fn ensure_v7_relay_after_opening(state: &GameState) -> Result<()> {
    let unsupported = |reason: &str| {
        EngineError::UnsupportedFeature(format!("v7 Relay first-play profile: {reason}"))
    };
    if state.extra.get("relay") != Some(&json!({"black":false,"white":true}))
        || state.extra.get("gameStyle") != Some(&json!("normal"))
        || state.cards_used_this_turn.white != 1
        || state.cards_used_this_turn.black != 0
        || state.extra.get("replayEventNonce") != Some(&json!(3))
        || state
            .extra
            .get("replayEvents")
            .and_then(Value::as_array)
            .is_none_or(|events| events.len() != 3)
    {
        return Err(unsupported("effect or replay counters"));
    }
    let used: Vec<_> = state
        .deck_slots
        .white
        .iter()
        .filter(|card| !card.vacant && card.used)
        .collect();
    if used.len() != 1
        || used[0].id != "relay"
        || used[0].effect != "relay"
        || used[0].recovering
        || used[0]
            .extra
            .get("usedAt")
            .and_then(Value::as_i64)
            .is_none_or(|stamp| stamp <= 0)
    {
        return Err(unsupported("used Relay card identity"));
    }
    // The source Relay action changes no board piece. Restore only its known
    // effect/counters in a private clone, then use the full orthodox profile
    // check to reject any other changed rule state or footprint.
    let mut baseline = state.clone();
    baseline.cards_used_this_turn.white = 0;
    baseline
        .extra
        .insert("relay".into(), json!({"black":false,"white":false}));
    baseline.extra.insert("replayEventNonce".into(), json!(2));
    if let Some(card) = baseline.deck_slots.white.iter_mut().find(|card| card.used) {
        card.used = false;
        card.extra.shift_remove("usedAt");
    }
    if let Some(events) = baseline
        .extra
        .get_mut("replayEvents")
        .and_then(Value::as_array_mut)
    {
        events.pop();
    }
    ensure_v7_orthodox_opening(&baseline)
}

fn ensure_v7_orthodox_opening(state: &GameState) -> Result<()> {
    ensure_v7_orthodox_opening_inner(state, false)
}

fn ensure_v7_orthodox_opening_inner(
    state: &GameState,
    allow_cleared_virtual_replay: bool,
) -> Result<()> {
    // Presentation, draft, clock, and replay fields may vary across the three
    // verified seed-19 first-play Positions. Every other source extra field,
    // including inactive rule fields, must retain its source default.
    const VARIABLE_EXTRAS: &[&str] = &[
        "boardHistory",
        "cardAcquisitionNonce",
        "clock",
        "draft",
        "draftClock",
        "endDraftDone",
        "endPhaseStartMove",
        "gameStyle",
        "logs",
        "middleDraftDone",
        "notationEvent",
        "notationEvents",
        "notationTimeline",
        "openingAutoNoticeShown",
        "positionCounts",
        "repetitionSalt",
        "replayBaseFrame",
        "replayEventNonce",
        "replayEvents",
        "replayStartedAt",
        "replayTailFrame",
    ];
    // SHA-256 of JCS({ stable source extra fields }), shared by normal,
    // chaos, and grand active-only first-play oracle snapshots. This compact
    // profile check also rejects added, removed, or changed inactive fields.
    const STABLE_EXTRAS_SHA256: &str =
        "a4bf8022aca2454da4c23a4e948a3bc02fde9beefe8dbd752411c22b8d275534";
    let unsupported = |reason: &str| {
        EngineError::UnsupportedFeature(format!("v7 orthodox first-play profile: {reason}"))
    };
    if state.ruleset_id != RULES_VERSION_V7
        || state.mode != "play"
        || state.result().is_some()
        || state.turn != Color::White
        || state.actions_remaining != 1
        || state.move_count != 0
        || state.full_move != 1
        || state.turns_taken.white != 0
        || state.turns_taken.black != 0
        || state.cards_used_this_turn.white != 0
        || state.cards_used_this_turn.black != 0
        || state.en_passant.is_some()
        || !state.captures.white.is_empty()
        || !state.captures.black.is_empty()
    {
        return Err(unsupported("turn or capture state"));
    }
    let style = state
        .extra
        .get("gameStyle")
        .and_then(Value::as_str)
        .ok_or_else(|| unsupported("game style"))?;
    let (cards_per_side, slots_per_side, draft_events) = match style {
        "normal" => (1, 3, 2),
        "chaos" => (2, 6, 2),
        "grand" => (6, 6, 12),
        _ => return Err(unsupported("game style")),
    };
    let stable = state
        .extra
        .iter()
        .filter(|(name, _)| !VARIABLE_EXTRAS.contains(&name.as_str()))
        .map(|(name, value)| (name.as_str(), value))
        .collect::<BTreeMap<_, _>>();
    let stable_json = serde_jcs::to_vec(&stable).map_err(EngineError::serialization)?;
    if format!("{:x}", Sha256::digest(stable_json)) != STABLE_EXTRAS_SHA256 {
        return Err(unsupported("unverified rule-state defaults"));
    }
    let clock = state
        .extra
        .get("clock")
        .ok_or_else(|| unsupported("clock state"))?;
    if clock.get("enabled") != Some(&json!(true))
        || clock.get("runningColor") != Some(&json!("white"))
        || clock.get("timeoutLoser") != Some(&Value::Null)
        || clock.get("timeoutWinner") != Some(&Value::Null)
        || ["whiteMs", "blackMs"].into_iter().any(|name| {
            clock
                .get(name)
                .and_then(Value::as_f64)
                .is_none_or(|ms| ms <= 0.0)
        })
    {
        return Err(unsupported("active clock window"));
    }
    if state
        .extra
        .get("replayEvents")
        .and_then(Value::as_array)
        .is_none_or(|events| {
            events.len() != draft_events && !(allow_cleared_virtual_replay && events.is_empty())
        })
        || state.extra.get("replayEventNonce") != Some(&json!(draft_events))
        || state.extra.get("cardAcquisitionNonce") != Some(&json!(cards_per_side * 2))
        || state.extra.get("endDraftDone") != Some(&json!(style == "grand"))
        || state.extra.get("middleDraftDone") != Some(&json!(style == "grand"))
    {
        return Err(unsupported("draft provenance counters"));
    }
    let draft = state
        .extra
        .get("draft")
        .ok_or_else(|| unsupported("draft state"))?;
    if style == "grand" {
        if draft.get("kind").and_then(Value::as_str) != Some("grand")
            || draft.get("phase").and_then(Value::as_str) != Some("GRAND")
            || draft.get("color").and_then(Value::as_str) != Some("white")
            || draft.get("version") != Some(&json!(1))
            || draft.get("pickIndex") != Some(&json!(12))
            || draft
                .get("picks")
                .and_then(Value::as_array)
                .is_none_or(|picks| picks.len() != 12)
            || state.extra.get("endPhaseStartMove") != Some(&json!(0))
        {
            return Err(unsupported("grand draft completion"));
        }
    } else if draft.get("phase").and_then(Value::as_str) != Some("OPENING")
        || draft.get("color").and_then(Value::as_str) != Some("black")
        || draft.get("tutorial") != Some(&json!(false))
        || draft
            .get("choices")
            .and_then(Value::as_array)
            .is_none_or(|choices| !choices.is_empty())
        || state.extra.get("endPhaseStartMove") != Some(&Value::Null)
    {
        return Err(unsupported("opening draft completion"));
    }
    for side in [Color::White, Color::Black] {
        let deck = state.deck_slots.get(side);
        if deck.len() != slots_per_side
            || deck.iter().filter(|card| !card.vacant).count() != cards_per_side
            || deck
                .iter()
                .any(|card| !card.vacant && (card.used || card.recovering))
        {
            return Err(unsupported("selected active card count"));
        }
        for card in deck.iter().filter(|card| !card.vacant) {
            let definition = crate::card_registry::definition_for(RULES_VERSION_V7, &card.id)?;
            if definition.activation != Some(crate::card_registry::CardActType::Active)
                || definition.effect != card.effect
            {
                return Err(unsupported("selected card can alter first-play movement"));
            }
        }
    }
    let spatial = crate::SpatialState::from_v7_source(state)?;
    if spatial.geometry() != crate::BoardGeometry::new(0, 0, 8, 8)? || spatial.pieces().len() != 32
    {
        return Err(unsupported("board geometry or identity count"));
    }
    const BACK_RANK: [&str; 8] = [
        "rook", "knight", "bishop", "queen", "king", "bishop", "knight", "rook",
    ];
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let coord = crate::Coord::new(i32::from(row), i32::from(col));
            if !spatial.is_usable(coord) {
                return Err(unsupported("collapsed opening cell"));
            }
            let expected = match row {
                0 | 7 => Some(BACK_RANK[col as usize]),
                1 | 6 => Some("pawn"),
                _ => None,
            };
            let Some(kind) = expected else {
                if state.at(square).is_some() || spatial.piece_at(coord).is_some() {
                    return Err(unsupported("occupied middle rank"));
                }
                continue;
            };
            let piece = state
                .at(square)
                .ok_or_else(|| unsupported("missing initial piece"))?;
            let spatial_piece = spatial
                .piece_at(coord)
                .ok_or_else(|| unsupported("missing spatial occupancy"))?;
            let owner = if row <= 1 { Color::Black } else { Color::White };
            let origin = format!("{}{}", char::from(b'a' + col), 8 - row);
            if piece.kind != kind
                || piece.color != owner
                || piece.moved
                || piece.extra.len() != 2
                || piece.extra.get("origin") != Some(&json!(origin))
                || piece.extra.get("shielded") != Some(&json!(false))
                || spatial_piece.id != piece.id
                || spatial_piece.anchor != coord
                || spatial_piece.footprint.len() != 1
                || !spatial_piece.footprint.contains(&crate::Offset::new(0, 0))
            {
                return Err(unsupported("piece identity or source attributes"));
            }
        }
    }
    Ok(())
}

/// 원문 hasAnyLegalMove(main94765)는 turn을 임시 변경하고 첫 성공까지
/// 모든 물리 칸을 row-major로 조회한다. 큰 기물의 body alias도 생략하지
/// 않으며, Football은 소유 색상과 관계없이 검사한다. 이 API는 순수 조회다.
pub(crate) fn has_any_legal_move_v7_with(
    state: &GameState,
    color: Color,
    mut has_allowed_move: impl FnMut(&GameState, &Piece, Square) -> Result<bool>,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 legal-move probe requires v7 rules profile".into(),
        ));
    }
    let mut probe = state.clone();
    probe.validate_v7_snapshot_shape_and_identify()?;
    v7_visit_no_action_move_queries(&mut probe, color, |probe, piece, from| {
        has_allowed_move(probe, piece, from)
    })
}

fn v7_visit_no_action_move_queries(
    state: &mut GameState,
    color: Color,
    mut has_allowed_move: impl FnMut(&mut GameState, &Piece, Square) -> Result<bool>,
) -> Result<bool> {
    let previous_turn = state.turn;
    state.turn = color;
    let verdict = (|| {
        for row in 0..8 {
            for col in 0..8 {
                let from = Square { row, col };
                let Some(piece) = state.at(from).cloned() else {
                    continue;
                };
                if piece.kind == "wall" || piece.color != color && piece.kind != "football" {
                    continue;
                }
                if has_allowed_move(state, &piece, from)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    })();
    // 원문 finally에 대응한다. 실제 조회 효과는 남기고 turn만 복원한다.
    state.turn = previous_turn;
    verdict
}

/// A negative verdict is returned only after every source-reachable piece
/// query on this board succeeds with no move. Each unported live branch in
/// the shared kernel returns its concrete error; no broad opening witness or
/// approximate fallback can authorize a no-action loss.
pub(crate) fn v7_has_any_legal_move(state: &GameState, color: Color) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 legal-move probe requires v7 rules profile".into(),
        ));
    }
    has_any_legal_move_v7_with(state, color, |probe, piece, from| {
        Ok(!v7_legal_move_targets(probe, piece, from, V7MoveOptions::default())?.is_empty())
    })
}

/// 실제 checkNoActionLoss가 이동 단계까지 도달했을 때만 호출한다.
/// main94765의 조회 순서와 중단 지점을 공유하고 main98212에 실제 도달한
/// monoShade 기록만 보드에 반영한다. 공개 관측과 합법성 조회는 pure API를 쓴다.
pub(crate) fn v7_has_any_legal_move_live(state: &mut GameState, color: Color) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 legal-move probe requires v7 rules profile".into(),
        ));
    }
    // 검증 중 ID/winner 정규화를 live 상태에 섞지 않는다. 상태를 실행하는
    // 상위 경계는 이미 source identity를 보존한 Position을 전달해야 한다.
    let mut validated = state.clone();
    validated.validate_v7_snapshot_shape_and_identify()?;
    v7_visit_no_action_move_queries(state, color, |probe, _, from| {
        Ok(!v7_legal_move_targets_live(probe, from, V7MoveOptions::default())?.is_empty())
    })
}

/// The typed card/RULE veto portion of a v7 movement capture. The caller must
/// also apply the source target, ability, terrain and action-option policies;
/// a `true` result alone does not authorize a capture. Malformed live counters
/// are returned as errors rather than quietly producing a legal move.
#[cfg(test)]
pub(crate) fn v7_movement_capture_constraints_allow(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
) -> Result<bool> {
    let constraints =
        crate::card_constraints::CaptureConstraints::from_source_state(state, attacker)?;
    Ok(!constraints.piece_veto() && !constraints.game_veto(attacker, target))
}

/// A cursor stores one piece/card family at a time. Enumeration never allocates
/// the complete Cartesian action space; a family's dedicated cursor can replace
/// its bounded batch as compound cards are ported.
#[derive(Clone)]
pub(crate) struct ActionCursor {
    pending: VecDeque<Action>,
    active_staged: Option<(usize, crate::card_effects::OrderedSelectionCursor)>,
    board_index: usize,
    card_index: usize,
    seen: BTreeSet<String>,
    forced_id: Option<String>,
    checker_capture: bool,
    finished: bool,
}
pub(crate) struct CursorWork {
    pub(crate) action: Option<Action>,
    pub(crate) staged: Option<crate::card_effects::StagedActionPage>,
}
impl CursorWork {
    fn one(action: Option<Action>) -> Self {
        Self {
            action,
            staged: None,
        }
    }
    fn staged(page: crate::card_effects::StagedActionPage) -> Self {
        Self {
            action: None,
            staged: Some(page),
        }
    }
}
impl ActionCursor {
    pub(crate) fn new(state: &GameState) -> Result<Self> {
        let special = state.result().is_some()
            || state.mode != "play"
            || state
                .extra
                .get("pendingPromotion")
                .is_some_and(|v| !v.is_null());
        if special {
            return Ok(Self {
                pending: legal_actions(state)?.into(),
                active_staged: None,
                board_index: 64,
                card_index: state.deck_slots.get(state.decision_actor()).len(),
                seen: BTreeSet::new(),
                forced_id: None,
                checker_capture: false,
                finished: true,
            });
        }
        ensure_supported(state)?;
        Ok(Self {
            pending: VecDeque::new(),
            active_staged: None,
            board_index: 0,
            card_index: 0,
            seen: BTreeSet::new(),
            forced_id: forced_piece_id(state).map(str::to_owned),
            checker_capture: has_checker_capture(state)?,
            finished: false,
        })
    }
    /// This is a structural check only. It does not probe another candidate
    /// outside the caller's examination budget to determine exhaustion.
    pub(crate) fn is_exhausted(&self, state: &GameState) -> bool {
        self.pending.is_empty()
            && self.active_staged.is_none()
            && (self.finished
                || self.board_index >= 64
                    && (self.checker_capture
                        || self.forced_id.is_some()
                        || self.card_index >= state.deck_slots.get(state.turn).len()))
    }

    /// A normal piece, pending action, or deck slot consumes one unit of work.
    /// An ordered card family may consume a bounded batch of raw UI tuples;
    /// rejected tuples still count toward the caller's budget.
    pub(crate) fn examine(&mut self, state: &GameState, budget: usize) -> Result<CursorWork> {
        if let Some(action) = self.pending.pop_front() {
            return Ok(CursorWork::one(Some(action)));
        }
        if let Some((slot, cursor)) = &mut self.active_staged {
            let page = cursor.next_public_page(state, *slot, budget, budget)?;
            if page.examined == 0 || page.examined > budget || page.actions.len() > page.examined {
                return Err(EngineError::InvalidState(
                    "staged action cursor exceeded or failed its examination budget".into(),
                ));
            }
            if page.exhausted {
                self.active_staged = None;
            }
            return Ok(CursorWork::staged(page));
        }
        if self.board_index < 64 {
            let from = Square {
                row: (self.board_index / 8) as u8,
                col: (self.board_index % 8) as u8,
            };
            self.board_index += 1;
            if let Some(piece) = state.at(from)
                && piece.color == state.turn
                && self.seen.insert(piece.id.clone())
                && mobile(piece)
                && self.forced_id.as_ref().is_none_or(|id| id == &piece.id)
            {
                self.pending.extend(
                    piece_moves(state, piece, from)?
                        .into_iter()
                        .filter(|target| !self.checker_capture || target.flag("checkerCapture"))
                        .map(|target| Action::movement(state.turn, from, target)),
                );
            }
            return Ok(CursorWork::one(self.pending.pop_front()));
        }
        if !self.checker_capture
            && self.forced_id.is_none()
            && self.card_index < state.deck_slots.get(state.turn).len()
        {
            let slot = self.card_index;
            let card = &state.deck_slots.get(state.turn)[slot];
            self.card_index += 1;
            if usable_card(card) {
                if matches!(card.effect.as_str(), "cleanupPieces" | "hypocrisy") {
                    if let Some(cursor) = crate::card_effects::staged_cursor_for_slot(state, slot)?
                    {
                        if !cursor.is_exhausted() {
                            self.active_staged = Some((slot, cursor));
                        }
                    } else {
                        self.pending
                            .extend(crate::transition::card_actions(state, card)?);
                    }
                } else {
                    // Portal Gun still needs its future rule lifecycle before
                    // its staged candidates can be exposed as legal actions.
                    self.pending
                        .extend(crate::transition::card_actions(state, card)?);
                }
            }
            return Ok(CursorWork::one(self.pending.pop_front()));
        }
        Err(EngineError::InvalidState(
            "action cursor examined after exhaustion".into(),
        ))
    }
}
fn mobile(piece: &Piece) -> bool {
    !frozen(piece)
        && piece
            .extra
            .get("staked")
            .and_then(|s| s.get("remaining"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            <= 0
}
fn usable_card(card: &CardSlot) -> bool {
    !card.vacant
        && !card.used
        && !card.recovering
        && !card
            .extra
            .get("nextTurnPending")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}
fn has_checker_capture(state: &GameState) -> Result<bool> {
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let from = Square { row, col };
            let Some(piece) = state.at(from) else {
                continue;
            };
            if piece.color == state.turn
                && matches!(piece.kind.as_str(), "checker" | "checkerKing")
                && mobile(piece)
                && seen.insert(piece.id.clone())
                && piece_moves(state, piece, from)?
                    .iter()
                    .any(|target| target.flag("checkerCapture"))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Validate the selected semantic action directly, rather than enumerating all
/// other card combinations to locate it.
pub(crate) fn validate_action(state: &GameState, action: &Action) -> Result<()> {
    if state.result().is_some() {
        return Err(EngineError::Terminal);
    }
    if action.color != state.decision_actor() {
        return Err(EngineError::WrongActor);
    }
    if state.mode != "play"
        || state
            .extra
            .get("pendingPromotion")
            .is_some_and(|v| !v.is_null())
    {
        return if legal_actions(state)?.contains(action) {
            Ok(())
        } else {
            Err(EngineError::IllegalAction)
        };
    }
    ensure_supported(state)?;
    let checker_capture = has_checker_capture(state)?;
    let forced = forced_piece_id(state);
    match action.kind {
        ActionKind::Move => {
            let from = action.from.ok_or(EngineError::IllegalAction)?;
            let piece = state.at(from).ok_or(EngineError::IllegalAction)?;
            if piece.color != state.turn
                || !mobile(piece)
                || forced.is_some_and(|id| id != piece.id)
                || checker_capture
                    && !action
                        .destination
                        .as_ref()
                        .is_some_and(|m| m.flag("checkerCapture"))
            {
                return Err(EngineError::IllegalAction);
            }
            if piece_moves(state, piece, from)?
                .into_iter()
                .any(|target| Action::movement(state.turn, from, target) == *action)
            {
                Ok(())
            } else {
                Err(EngineError::IllegalAction)
            }
        }
        ActionKind::Card if !checker_capture && forced.is_none() => {
            let card = state
                .deck_slots
                .get(state.turn)
                .iter()
                .find(|card| {
                    usable_card(card)
                        && Some(&card.id) == action.card_id.as_ref()
                        && Some(&card.instance_id) == action.card_instance_id.as_ref()
                })
                .ok_or(EngineError::IllegalAction)?;
            if crate::transition::validate_card_action(state, card, action)? {
                Ok(())
            } else {
                Err(EngineError::IllegalAction)
            }
        }
        _ => Err(EngineError::IllegalAction),
    }
}

pub(crate) fn public_hints(state: &GameState, viewer: Color) -> Result<Value> {
    if state.mode != "play"
        || state.turn != viewer
        || state
            .extra
            .get("pendingPromotion")
            .is_some_and(|v| !v.is_null())
        || state
            .extra
            .get("activeTrolley")
            .is_some_and(|v| !v.is_null())
    {
        return Ok(json!({"moves":[],"cardTargets":[]}));
    }
    ensure_supported(state)?;
    let checker_capture = has_checker_capture(state)?;
    let forced = forced_piece_id(state);
    let mut hints = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let from = Square { row, col };
            let Some(piece) = state.at(from) else {
                continue;
            };
            if piece.color != viewer
                || !mobile(piece)
                || !state.piece_visible(piece, from, viewer)
                || forced.is_some_and(|id| id != piece.id)
            {
                continue;
            }
            let mut destinations = Vec::<Square>::new();
            for target in piece_moves(state, piece, from)? {
                if checker_capture && !target.flag("checkerCapture") {
                    continue;
                }
                for square in highlight_cells(&target)? {
                    if !destinations.contains(&square) {
                        destinations.push(square);
                    }
                }
            }
            if !destinations.is_empty() {
                hints.push(json!({"from":from,"destinations":destinations}));
            }
        }
    }
    let mut card_targets = Vec::new();
    if !state
        .extra
        .get("draftDelete")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        for card in state
            .deck_slots
            .get(viewer)
            .iter()
            .filter(|card| !card.vacant && !card.used && !card.recovering)
        {
            if card
                .extra
                .get("target")
                .is_none_or(|target| target.is_null())
            {
                continue;
            }
            let targets = if let Some(targets) = crate::card_effects::target_squares(state, card)? {
                targets
            } else {
                crate::transition::card_ui_actions(state, card)?
                    .into_iter()
                    .map(|a| {
                        a.target
                            .ok_or_else(|| {
                                EngineError::UnsupportedFeature(format!(
                                    "card target surface {}",
                                    card.effect
                                ))
                            })
                            .and_then(|v| {
                                serde_json::from_value::<Square>(v).map_err(|_| {
                                    EngineError::UnsupportedFeature(format!(
                                        "compound card target surface {}",
                                        card.effect
                                    ))
                                })
                            })
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            let targets = targets
                .into_iter()
                .filter(|&square| {
                    state
                        .at(square)
                        .is_none_or(|piece| state.piece_visible(piece, square, viewer))
                })
                .collect::<Vec<_>>();
            card_targets.push(json!({"cardInstanceId":card.instance_id,"targets":targets}));
        }
    }
    Ok(json!({"moves":hints,"cardTargets":card_targets}))
}
fn highlight_cells(target: &MoveTarget) -> Result<Vec<Square>> {
    if target.flags.contains_key("bodyCells") || target.flag("shotgunBlast") {
        return Ok(Vec::new());
    }
    let read = |name: &str| -> Result<Option<Vec<Square>>> {
        target
            .flags
            .get(name)
            .map(|value| serde_json::from_value(value.clone()).map_err(EngineError::serialization))
            .transpose()
    };
    if let Some(highlights) = read("highlightCells")? {
        if let Some(display) = read("displayCells")? {
            return Ok(display);
        }
        let excluded = read("excludeHighlightCells")?.unwrap_or_default();
        return Ok(highlights
            .into_iter()
            .filter(|square| !excluded.contains(square))
            .collect());
    }
    Ok(read("sectorCells")?.unwrap_or_else(|| vec![target.square()]))
}
pub(crate) fn public_move_intent(state: &GameState, action: &Action) -> Result<Value> {
    validate_action(state, action)?;
    let target = action
        .destination
        .as_ref()
        .ok_or(EngineError::IllegalAction)?;
    if !highlight_cells(target)?.contains(&target.square()) {
        return Err(EngineError::UnsupportedFeature(
            "public move intent for a non-coordinate action mode".into(),
        ));
    }
    Ok(json!({"type":"move","color":action.color,"from":action.from,"destination":target.square()}))
}

/// Project one source descriptor to its clickable public cells. Host capture
/// flags, victim identities and Position binding stay in the source candidate.
/// This consumes an already generated action and never enumerates candidates.
pub(crate) fn v7_public_move_intents(state: &GameState, action: &Action) -> Result<Vec<Value>> {
    if state.ruleset_id != RULES_VERSION_V7
        || action.kind != ActionKind::Move
        || state.mode != "play"
        || action.color != state.turn
    {
        return Err(EngineError::IllegalAction);
    }
    let from = action.from.ok_or(EngineError::IllegalAction)?;
    let piece = state.at(from).ok_or(EngineError::IllegalAction)?;
    if piece.color != action.color && piece.kind != "football" {
        return Err(EngineError::WrongActor);
    }
    let target = action
        .destination
        .as_ref()
        .ok_or(EngineError::IllegalAction)?;
    let mode = if target.flag("shotgunBlast") {
        Some("shotgun")
    } else if target.flag("shotgunSnipe") {
        Some("snipe")
    } else if target.flag("setLogDirection") {
        Some("log-direction")
    } else {
        None
    };
    v7_click_cells(target)?
        .into_iter()
        .map(|destination| {
            let mut intent =
                json!({"type":"move","color":action.color,"from":from,"destination":destination});
            if let Some(mode) = mode {
                intent["selectionMode"] = json!(mode);
            }
            Ok(intent)
        })
        .collect()
}

/// Rebind a public click against the single complete source candidate set
/// already retained by the host. Source handleSquareClick picks the first
/// descriptor containing the clicked cell; later same-click descriptors must
/// not change the meaning of that public intent.
pub(crate) fn v7_resolve_public_move_from_candidates(
    state: &GameState,
    intent: &Value,
    candidates: &[Action],
) -> Result<Action> {
    let object = intent.as_object().ok_or(EngineError::IllegalAction)?;
    let has_mode = object.contains_key("selectionMode");
    if state.ruleset_id != RULES_VERSION_V7
        || object.len() != if has_mode { 5 } else { 4 }
        || !["type", "color", "from", "destination"]
            .iter()
            .all(|field| object.contains_key(*field))
        || intent.get("type").and_then(Value::as_str) != Some("move")
    {
        return Err(EngineError::IllegalAction);
    }
    if has_mode
        && !intent
            .get("selectionMode")
            .and_then(Value::as_str)
            .is_some_and(|mode| matches!(mode, "shotgun" | "snipe" | "log-direction"))
    {
        return Err(EngineError::IllegalAction);
    }
    for field in ["from", "destination"] {
        let square = intent[field]
            .as_object()
            .ok_or(EngineError::IllegalAction)?;
        if square.len() != 2 || !square.contains_key("row") || !square.contains_key("col") {
            return Err(EngineError::IllegalAction);
        }
    }
    let color: Color =
        serde_json::from_value(intent["color"].clone()).map_err(EngineError::serialization)?;
    let from: Square =
        serde_json::from_value(intent["from"].clone()).map_err(EngineError::serialization)?;
    let click: Square = serde_json::from_value(intent["destination"].clone())
        .map_err(EngineError::serialization)?;
    if color != state.turn {
        return Err(EngineError::WrongActor);
    }
    if from.row >= 8 || from.col >= 8 || click.row >= 8 || click.col >= 8 {
        return Err(EngineError::IllegalAction);
    }
    for action in candidates {
        if action.kind == ActionKind::Move
            && action.color == color
            && action.from == Some(from)
            && v7_public_move_intents(state, action)?
                .iter()
                .any(|candidate| candidate == intent)
        {
            return Ok(action.clone());
        }
    }
    Err(EngineError::IllegalAction)
}

fn v7_click_cells(target: &MoveTarget) -> Result<Vec<Square>> {
    let cells = if let Some(body) = target.flags.get("bodyCells") {
        serde_json::from_value::<Vec<Square>>(body.clone()).map_err(EngineError::serialization)?
    } else if let Some(display) = target.flags.get("displayCells") {
        serde_json::from_value::<Vec<Square>>(display.clone())
            .map_err(EngineError::serialization)?
    } else if let Some(highlight) = target.flags.get("highlightCells") {
        serde_json::from_value::<Vec<Square>>(highlight.clone())
            .map_err(EngineError::serialization)?
    } else if let Some(sector) = target.flags.get("sectorCells") {
        serde_json::from_value::<Vec<Square>>(sector.clone()).map_err(EngineError::serialization)?
    } else {
        vec![target.square()]
    };
    let mut seen = BTreeSet::new();
    if cells.iter().any(|cell| cell.row >= 8 || cell.col >= 8) {
        return Err(EngineError::InvalidState(
            "v7 public click descriptor outside 8x8".into(),
        ));
    }
    Ok(cells
        .into_iter()
        .filter(|cell| seen.insert(*cell))
        .collect())
}
pub(crate) fn resolve_move_intent(state: &GameState, value: &Value) -> Result<Action> {
    let fields = ["type", "color", "from", "destination"];
    let object = value.as_object().ok_or(EngineError::IllegalAction)?;
    if object.len() != fields.len() || fields.iter().any(|name| !object.contains_key(*name)) {
        return Err(EngineError::IllegalAction);
    }
    for name in ["from", "destination"] {
        let square = value[name].as_object().ok_or(EngineError::IllegalAction)?;
        if square.len() != 2 || !square.contains_key("row") || !square.contains_key("col") {
            return Err(EngineError::IllegalAction);
        }
    }
    let color: Color =
        serde_json::from_value(value["color"].clone()).map_err(EngineError::serialization)?;
    if color != state.decision_actor() {
        return Err(EngineError::WrongActor);
    }
    let from: Square =
        serde_json::from_value(value["from"].clone()).map_err(EngineError::serialization)?;
    let click: Square =
        serde_json::from_value(value["destination"].clone()).map_err(EngineError::serialization)?;
    if from.row >= 8 || from.col >= 8 || click.row >= 8 || click.col >= 8 {
        return Err(EngineError::IllegalAction);
    }
    if state.mode != "play"
        || state
            .extra
            .get("pendingPromotion")
            .is_some_and(|v| !v.is_null())
    {
        return Err(EngineError::IllegalAction);
    }
    ensure_supported(state)?;
    let piece = state.at(from).ok_or(EngineError::IllegalAction)?;
    if piece.color != color || !mobile(piece) {
        return Err(EngineError::IllegalAction);
    }
    for target in piece_moves(state, piece, from)? {
        // Actual handleSquareClick selects the first legal move containing the
        // clicked public display cell. Internal capture IDs/flags stay here.
        let cells = if let Some(body) = target.flags.get("bodyCells") {
            serde_json::from_value::<Vec<Square>>(body.clone())
                .map_err(EngineError::serialization)?
        } else if let Some(display) = target.flags.get("displayCells") {
            serde_json::from_value::<Vec<Square>>(display.clone())
                .map_err(EngineError::serialization)?
        } else {
            highlight_cells(&target)?
        };
        if cells.contains(&click) {
            let action = Action::movement(color, from, target);
            match validate_action(state, &action) {
                Ok(()) => return Ok(action),
                Err(EngineError::IllegalAction) => continue,
                Err(error) => return Err(error),
            }
        }
    }
    Err(EngineError::IllegalAction)
}

fn ensure_supported(state: &GameState) -> Result<()> {
    if state.ruleset_id == RULES_VERSION_V7 {
        if state.board.len() != 8 || state.board.iter().any(|line| line.len() != 8) {
            return Err(EngineError::InvalidState(
                "v7 movement requires the adopted 8x8 board".into(),
            ));
        }
        // The per-piece source pipeline owns exact live-branch restrictions.
        // Active unsupported geometry still errors when the query reaches it.
        return ensure_v7_capture_policy_scope(state);
    }
    ensure_supported_interactions(state, true)
}
fn ensure_supported_interactions(state: &GameState, require_execution_support: bool) -> Result<()> {
    // Preserving an unknown JSON field does not prove that its rules are executed.
    // Active effects outside the implemented set are explicit errors during porting.
    const PENDING: &[&str] = &[
        "activeTrolley",
        "ruleTicketChoice",
        "jokerChoice",
        "barricadeDirectionChoice",
        "earlyPromotion",
        "fastGrowth",
        "afterimageQueen",
        "initiative",
        "moving",
        "coronation",
        "majesty",
        "infiltration",
        "killerKing",
        "resolve",
        "vanguard",
        "e4",
        "solidarity",
        "bishopInfiltration",
        "assembly",
        "vigilance",
        "roller",
        "binaMate",
        "overwhelm",
        "regency",
        "racingKing",
        "radicalCharge",
        "ironMonarch",
        "imperialStudies",
        "religiousVictory",
        "backwardKnight",
        "trojanHorse",
        "madHorse",
        "clonePassive",
        "frontlineResponse",
        "relay",
        "fieldPromotion",
        "gomoku",
        "vanishing",
        "knightInjury",
        "pawnConversion",
        "fileSurge",
        "rookLift",
        "underpromotion",
        "finalWeapon",
        "highway",
        "recycling",
        "coolGuy",
        "manner",
        "switcheroo",
        "substitution",
        "chainBonds",
        "highGround",
        "platformRule",
        "portalRule",
        "crownRule",
        "conveyorRule",
        "periodicCollapse",
        "ruleBombs",
        "exhaustion",
        "camouflageRule",
        "transcendenceRule",
        "captureTheFlag",
        "idolEncorePending",
        "sirenExposure",
        "necromancy",
        "temporaryQueens",
        "timeStop",
        "skipTurn",
        "pendingIcbm",
        "pendingLobsters",
        "pendingScarecrows",
        "quantumMechanics",
        "monochromeChess",
        "symmetry",
        "locustSwarm",
        "zugzwang",
        "freeMoveCaptureLock",
        "captureLock",
        "taunt",
        "monsterRule",
    ];
    if let Some(pending) = state
        .extra
        .get("pendingPortals")
        .filter(|value| active(value))
    {
        let entries = pending.as_array().ok_or_else(|| {
            EngineError::UnsupportedFeature("non-array portal movement ledger".into())
        })?;
        if entries.len() > 4096 {
            return Err(EngineError::UnsupportedFeature(
                "portal pending ledger capacity".into(),
            ));
        }
        if entries
            .iter()
            .any(|entry| entry.get("blocksMovement") == Some(&Value::Bool(true)))
        {
            return Err(EngineError::UnsupportedFeature(
                "movement-blocking portal reservation".into(),
            ));
        }
    }
    for name in PENDING {
        if state.extra.get(*name).is_some_and(active) {
            return Err(EngineError::UnsupportedFeature(format!(
                "active state effect {name}"
            )));
        }
    }
    for piece in state.board.iter().flatten().flatten() {
        if require_execution_support && !implemented_piece_types().contains(&piece.kind.as_str()) {
            return Err(EngineError::UnsupportedFeature(format!(
                "piece {}",
                piece.kind
            )));
        }
        for flag in [
            "quantum",
            "frenzy",
            "desperado",
            "stealth",
            "poisoned",
            "hedgehog",
            "crownBearer",
            "crownRoyal",
            "metalized",
            "repositionSecondMove",
            "thiefSecondMove",
            "rookLiftSecondMove",
            "ironMonarchExtraMove",
            "madHorseSecondMove",
            "fileSurgeSecondMove",
            "promotionRush",
            "royalCommand",
        ] {
            if piece.extra.get(flag).is_some_and(active) {
                return Err(EngineError::UnsupportedFeature(format!(
                    "piece effect {flag}"
                )));
            }
        }
    }
    Ok(())
}
fn active(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => v
            .iter()
            .any(|(key, value)| !key.starts_with("__") && active(value)),
    }
}
pub(crate) fn frozen(piece: &Piece) -> bool {
    piece.kind != "scarecrow"
        && piece.extra.get("frozen").is_some_and(|v| match v {
            Value::Null => false,
            Value::Bool(v) => *v,
            Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
            Value::String(v) => !v.is_empty(),
            _ => true,
        })
}

/// Source placement and relocation differ: placement checks opponent d4 and
/// all reserved portal cells; relocation checks only movement-blocking portals.
pub(crate) fn open_placement(
    state: &GameState,
    square: Square,
    color: Option<Color>,
) -> Result<bool> {
    if square.row >= 8
        || square.col >= 8
        || state.at(square).is_some()
        || quantum_occupied(state, square)?
    {
        return Ok(false);
    }
    if let Some(color) = color {
        let enemy = color.opponent();
        if state.flag("d4", enemy)
            && square
                == (Square {
                    row: if enemy == Color::Black { 3 } else { 4 },
                    col: 3,
                })
        {
            return Ok(false);
        }
    }
    Ok(!reserved(state, square, false)?)
}
pub(crate) fn open_relocation(state: &GameState, square: Square) -> Result<bool> {
    Ok(square.row < 8
        && square.col < 8
        && state.at(square).is_none()
        && !quantum_occupied(state, square)?
        && !reserved(state, square, true)?)
}
/// Source isSquareOpenForInstallation does not include terrain, black holes
/// or expansion restrictions. The installing caller applies those policies.
pub(crate) fn open_installation(state: &GameState, square: Square, owner: Color) -> Result<bool> {
    if square.row >= 8
        || square.col >= 8
        || quantum_occupied(state, square)?
        || crown_ground_from_state(state, square)
    {
        return Ok(false);
    }
    if state.at(square).is_some_and(|occupant| {
        occupant.color != owner.opponent()
            || crate::observation::piece_hidden_from_v7(state, occupant, square) != Some(owner)
    }) {
        return Ok(false);
    }
    Ok(!reserved(state, square, false)?)
}
pub(crate) fn open_alibaba_placement(
    state: &GameState,
    square: Square,
    owner: Color,
) -> Result<bool> {
    if !open_placement(state, square, Some(owner))? || collapsed(state, square) {
        return Ok(false);
    }
    if state.ruleset_id == RULES_VERSION_V7 {
        if crate::v7_queued_effects::crown_ground_at(state, square) {
            return Ok(false);
        }
        return Ok(!v7_black_hole_cells(state)?.contains(&square));
    }
    if state.extra.get("crownRule").is_some_and(active) {
        return Err(EngineError::UnsupportedFeature(
            "placement on ground crowns".into(),
        ));
    }
    let holes = state.extra.get("blackHole").filter(|v| !v.is_null());
    if let Some(holes) = holes {
        let holes = holes.as_array().ok_or_else(|| {
            EngineError::UnsupportedFeature("legacy black-hole placement shape".into())
        })?;
        if holes.iter().any(|cell| {
            cell.get("row").and_then(Value::as_u64) == Some(u64::from(square.row))
                && cell.get("col").and_then(Value::as_u64) == Some(u64::from(square.col))
        }) {
            return Ok(false);
        }
    }
    Ok(true)
}
/// main68010 canReservePortalSquare first rejects every occupied board cell,
/// so the concealed-occupant branch of isSquareOpenForInstallation cannot
/// admit a portal reservation. Installation reservations and crown ground are
/// still distinct from ordinary piece placement.
pub(crate) fn open_portal_reservation(
    state: &GameState,
    square: Square,
    _owner: Color,
) -> Result<bool> {
    if square.row >= 8
        || square.col >= 8
        || state.at(square).is_some()
        || quantum_occupied(state, square)?
        || reserved(state, square, false)?
    {
        return Ok(false);
    }
    Ok(!portal_installation_hazard(state, square))
}
/// The due Portal Gun callback checks only terrain hazards. Occupancy,
/// quantum bodies and other reservations matter when selecting the cells,
/// but are not checked again when the portal is installed (main74148-63).
pub(crate) fn portal_installation_hazard(state: &GameState, square: Square) -> bool {
    if square.row >= 8 || square.col >= 8 || collapsed(state, square) {
        return true;
    }
    let crown = state.extra.get("crownRule").unwrap_or(&Value::Null);
    let entries = crown
        .get("crowns")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty());
    let crown_ground = |entry: &Value| {
        if !crate::observation::truth(Some(entry))
            || crate::observation::truth(entry.get("removed"))
        {
            return false;
        }
        if entry == &Value::Bool(true) {
            return square == (Square { row: 3, col: 3 });
        }
        let ground = entry.get("ground");
        let row = ground
            .and_then(|ground| ground.get("row"))
            .and_then(Value::as_f64);
        let col = ground
            .and_then(|ground| ground.get("col"))
            .and_then(Value::as_f64);
        row == Some(f64::from(square.row)) && col == Some(f64::from(square.col))
    };
    if entries.is_some_and(|entries| entries.iter().any(crown_ground))
        || entries.is_none() && crown_ground(crown)
    {
        return true;
    }
    state
        .extra
        .get("blackHole")
        .and_then(Value::as_array)
        .is_some_and(|holes| {
            holes.iter().any(|cell| {
                crate::observation::number(cell.get("row")) == Some(f64::from(square.row))
                    && crate::observation::number(cell.get("col")) == Some(f64::from(square.col))
            })
        })
}
pub(crate) fn quantum_occupied(state: &GameState, square: Square) -> Result<bool> {
    for piece in state.board.iter().flatten().flatten() {
        let Some(quantum) = piece.extra.get("quantum").filter(|q| !q.is_null()) else {
            continue;
        };
        if state.ruleset_id == RULES_VERSION_V7 {
            // Source quantumCellsForItemAt requires an integer, in-bounds
            // anchor and a complete footprint. A large shadow at (7,7)
            // occupies no cells, rather than a clipped one-cell fragment.
            let Some(anchor) = v7_descriptor_square(quantum) else {
                continue;
            };
            let size = if piece.is_large() { 2 } else { 1 };
            if anchor.row.saturating_add(size) > 8 || anchor.col.saturating_add(size) > 8 {
                continue;
            }
            if square.row >= anchor.row
                && square.col >= anchor.col
                && square.row - anchor.row < size
                && square.col - anchor.col < size
            {
                return Ok(true);
            }
            continue;
        }
        let anchor: Square =
            serde_json::from_value(quantum.clone()).map_err(EngineError::serialization)?;
        let size = if piece.is_large() { 2 } else { 1 };
        if square.row >= anchor.row
            && square.col >= anchor.col
            && square.row - anchor.row < size
            && square.col - anchor.col < size
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn reserved(state: &GameState, square: Square, relocation: bool) -> Result<bool> {
    let same_square = |value: &Value| {
        value.get("row").and_then(Value::as_f64) == Some(f64::from(square.row))
            && value.get("col").and_then(Value::as_f64) == Some(f64::from(square.col))
    };
    for name in ["pendingScarecrows", "pendingLobsters", "pendingPortals"] {
        let Some(entries) = state.extra.get(name).filter(|v| !v.is_null()) else {
            continue;
        };
        let Some(entries) = entries.as_array() else {
            if name == "pendingPortals" {
                // main67957 treats a non-array portal ledger as no active
                // reservations; portalGun later replaces it with an array.
                continue;
            }
            return Err(EngineError::InvalidState(format!(
                "{name} must be an array"
            )));
        };
        for entry in entries {
            let matches = match name {
                "pendingScarecrows" => {
                    !crate::observation::truth(entry.get("pieceId")) && same_square(entry)
                }
                "pendingLobsters" => same_square(entry),
                _ => {
                    (!relocation || entry.get("blocksMovement") == Some(&json!(true)))
                        && entry
                            .get("cells")
                            .and_then(Value::as_array)
                            .is_some_and(|cells| cells.iter().any(&same_square))
                }
            };
            if matches {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn forced_piece_id(state: &GameState) -> Option<&str> {
    state
        .board
        .iter()
        .flatten()
        .flatten()
        .find(|piece| piece.color == state.turn && piece.flag("checkerChainCapture"))
        .map(|piece| piece.id.as_str())
}

pub(crate) fn desperado_royal_capture_blocked(attacker: &Piece, target: &Piece) -> bool {
    crate::observation::truth(attacker.extra.get("desperado"))
        && (crate::observation::truth(target.extra.get("regencyHeir"))
            || crate::observation::truth(target.extra.get("crownRoyal"))
            || matches!(
                target.kind.as_str(),
                "king" | "royalKnight" | "shotgunKing" | "darkWizard"
            ))
}

/// The card temporarily sets desperado before calling source getLegalMoves.
/// Its movement stays unchanged. This owned probe applies the ordinary kernel
/// and source forced-piece/royal-capture guards without changing the caller.
pub(crate) fn desperado_has_legal_move(
    state: &GameState,
    piece: &Piece,
    square: Square,
) -> Result<bool> {
    let Some(actor) = piece.color.owner() else {
        return Ok(false);
    };
    let socialism = crate::observation::number(
        state
            .extra
            .get("socialism")
            .and_then(|value| value.get(actor.as_str())),
    )
    .unwrap_or(0.0)
        > 0.0;
    if frozen(piece)
        || crate::observation::number(piece.extra.get("staked").and_then(|v| v.get("remaining")))
            .unwrap_or(0.0)
            > 0.0
        || crate::observation::number(piece.extra.get("poisonStunTurns"))
            .unwrap_or(0.0)
            .floor()
            > 0.0
        || (matches!(piece.kind.as_str(), "hedgehog" | "bear")
            || piece.ability_kind() == "hedgehog")
            && crate::observation::number(piece.extra.get("bearMoveLockedUntilTurn")).unwrap_or(0.0)
                > f64::from(*state.turns_taken.get(actor))
        || piece.kind == "babyBear" && !socialism
        || piece.kind == "medium"
            && state
                .extra
                .get("mediumMovement")
                .and_then(|v| v.get("type"))
                .is_none_or(|v| !crate::observation::truth(Some(v)))
            && !socialism
    {
        return Ok(false);
    }
    let mut trial = state.clone();
    let mut candidate = piece.clone();
    candidate
        .extra
        .insert("desperado".into(), serde_json::json!({"remaining":2}));
    if state.at(square).is_none_or(|at| at.id != piece.id) {
        return Err(EngineError::IllegalAction);
    }
    for cell in trial.board.iter_mut().flatten().flatten() {
        if cell.id == piece.id {
            *cell = candidate.clone();
        }
    }
    let reposition = trial.board.iter().flatten().flatten().find(|at| {
        at.color == actor && crate::observation::truth(at.extra.get("repositionSecondMove"))
    });
    let forced = reposition.or_else(|| {
        trial.board.iter().flatten().flatten().find(|at| {
            at.color == actor
                && [
                    "thiefSecondMove",
                    "frenzyExtraMove",
                    "fileSurgeSecondMove",
                    "rookLiftSecondMove",
                    "ironMonarchExtraMove",
                    "underpromotionSecondMove",
                    "checkerChainCapture",
                    "madHorseSecondMove",
                    "platformExtraMove",
                    "desperado",
                ]
                .iter()
                .any(|key| crate::observation::truth(at.extra.get(*key)))
        })
    });
    if forced.is_some_and(|forced| forced.id != candidate.id) {
        return Ok(false);
    }
    // The temporary marker is the only newly implemented trial effect. All
    // other global interactions still pass the common support guard.
    let mut checked = trial.clone();
    for cell in checked.board.iter_mut().flatten().flatten() {
        cell.extra.shift_remove("desperado");
    }
    // Basic movement can be proven before that piece's execution/capture
    // reactions are complete. The ordinary support registry is unchanged.
    ensure_supported_interactions(&checked, false)?;
    let moves = piece_moves(&trial, &candidate, square)?;
    for target in moves {
        if target.flag("setLogDirection") {
            return Ok(true);
        }
        let mut cells = vec![target.square()];
        if target.flag("enPassant")
            && let (Some(row), Some(col)) = (
                target.flags.get("capturedRow").and_then(Value::as_u64),
                target.flags.get("capturedCol").and_then(Value::as_u64),
            )
            && row < 8
            && col < 8
        {
            cells.push(Square {
                row: row as u8,
                col: col as u8,
            });
        }
        for key in ["jumpCapture"] {
            if let Some(value) = target.flags.get(key) {
                cells.push(
                    serde_json::from_value(value.clone()).map_err(EngineError::serialization)?,
                );
            }
        }
        for key in [
            "sectorCells",
            "colossusLandingCaptures",
            "bigRookLandingCaptures",
        ] {
            if let Some(value) = target.flags.get(key) {
                cells.extend(
                    serde_json::from_value::<Vec<Square>>(value.clone())
                        .map_err(EngineError::serialization)?,
                );
            }
        }
        if (target.flag("shotgunBlast")
            || target.flag("colossusAttack")
            || target.flag("siegeRamMove"))
            && let Some(value) = target.flags.get("highlightCells")
        {
            cells.extend(
                serde_json::from_value::<Vec<Square>>(value.clone())
                    .map_err(EngineError::serialization)?,
            );
        }
        if !cells.into_iter().any(|cell| {
            trial.at(cell).is_some_and(|victim| {
                victim.color != candidate.color
                    && desperado_royal_capture_blocked(&candidate, victim)
            })
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn v7_chaos_capture_locked(state: &GameState) -> bool {
    let Some(deadline) = crate::observation::number(state.extra.get("chaosNoCaptureUntilHalfTurn"))
    else {
        return false;
    };
    deadline > f64::from(state.turns_taken.white) + f64::from(state.turns_taken.black)
}

fn v7_truth(value: &Value) -> bool {
    crate::observation::truth(Some(value))
}

fn v7_effective_catalog_profile(state: &GameState) -> Option<&Value> {
    state
        .extra
        .get("cardState")
        .filter(|value| v7_truth(value))
        .map_or_else(
            || state.extra.get("profile"),
            |card_state| card_state.get("profile"),
        )
}

/// 원문의 (cardState || state)는 truthy cardState가 profile을 갖지 않아도
/// 상위 profile로 되돌아가지 않는다. 행마·포획은 같은 선택 경계를 쓴다.
pub(crate) fn v7_effective_catalog_hash(state: &GameState) -> Option<&Value> {
    v7_effective_catalog_profile(state)
        .and_then(|profile| profile.get("catalogHash"))
        .filter(|hash| v7_truth(hash))
}

pub(crate) fn v7_armistice_active(state: &GameState) -> bool {
    let Some(value) = state.extra.get("armistice") else {
        return false;
    };
    let remaining = value.get("remaining").unwrap_or(value);
    crate::observation::number(Some(remaining)).is_some_and(|number| number.floor() > 0.0)
}

fn v7_overwhelm_king(piece: &Piece) -> bool {
    piece.flag("regencyHeir")
        || piece.flag("crownRoyal")
        || matches!(
            piece.kind.as_str(),
            "king" | "royalKnight" | "shotgunKing" | "merchant" | "timeTraveler" | "vampireLord"
        )
}

pub(crate) fn v7_overwhelm_capture_blocked(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
) -> bool {
    attacker.color != target.color
        && state.flag("overwhelm", target.color)
        && v7_overwhelm_king(attacker)
        && (v7_overwhelm_king(target) || target.kind == "queen" && !target.flag("regencyHeir"))
}

fn v7_time_traveler_campaign(state: &GameState) -> bool {
    state
        .extra
        .get("campaign")
        .and_then(|campaign| campaign.get("setup"))
        .and_then(Value::as_str)
        == Some("timeTraveler")
}

fn v7_time_phase_interacts(state: &GameState, attacker: &Piece, target: &Piece) -> bool {
    if !v7_time_traveler_campaign(state) {
        return true;
    }
    // time_phase_of의 v7 경계는 호출자가 검사하고 이 getter는 malformed
    // phase에도 future를 반환한다. None은 wall/football의 비위상 의미다.
    let a = crate::v7_campaign::time_phase_of(state, attacker)
        .ok()
        .flatten();
    let b = crate::v7_campaign::time_phase_of(state, target)
        .ok()
        .flatten();
    a.is_none() || b.is_none() || a == b
}

pub(crate) fn v7_uses_revolving_door_guard(state: &GameState) -> bool {
    if let Some(hash) = v7_effective_catalog_hash(state) {
        return matches!(
            hash.as_str(),
            Some(
                "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4"
                    | "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI"
            )
        );
    }
    if crate::observation::truth(state.extra.get("campaign")) {
        state.extra.get("revolvingDoorGuard") == Some(&json!(true))
    } else {
        state.extra.get("revolvingDoorGuard") != Some(&json!(false))
    }
}

fn v7_positive_side_counter(state: &GameState, field: &str, color: Color) -> bool {
    crate::observation::number(
        state
            .extra
            .get(field)
            .and_then(|sides| sides.get(color.as_str())),
    )
    .is_some_and(|number| number > 0.0)
}

fn v7_royal_command_capture_access(state: &GameState, attacker: &Piece) -> bool {
    if attacker.flag("crownBearer") {
        return true;
    }
    let Some(actor) = attacker.color.owner() else {
        return false;
    };
    if v7_positive_side_counter(state, "socialism", actor)
        && attacker.kind != "crown"
        && !state.royal_identity(attacker)
    {
        return false;
    }
    if attacker.flag("royalCommand") {
        return true;
    }
    let Some(window) = state
        .extra
        .get("royalCommand")
        .and_then(|entry| entry.get(actor.as_str()))
    else {
        return false;
    };
    let Some(active_turn) = window.get("activeTurn").and_then(Value::as_u64) else {
        return false;
    };
    let expires = window
        .get("expiresTurn")
        .and_then(Value::as_u64)
        .filter(|value| *value > active_turn)
        .unwrap_or(active_turn.saturating_add(1));
    let current = u64::from(*state.turns_taken.get(actor));
    active_turn <= current && current < expires
}

/// A raw v7 movement object may be used by staging probes before the public
/// legal-action gate opens. Reject capture modifiers whose source geometry or
/// catalog policy has not been ported instead of returning a partial list.
fn ensure_v7_capture_policy_scope(state: &GameState) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Ok(());
    }
    if let Some(profile) = v7_effective_catalog_profile(state).filter(|profile| v7_truth(profile)) {
        if let Some(hash) = profile.get("catalogHash").filter(|value| v7_truth(value)) {
            if hash.as_str() != Some("yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4") {
                return Err(EngineError::UnsupportedFeature(format!(
                    "v7 capture policy unclassified catalog profile {hash}"
                )));
            }
        } else if !profile.is_object() {
            return Err(EngineError::InvalidState(
                "v7 capture policy catalog profile must be an object".into(),
            ));
        }
    }
    if !crate::card_effects::september18(state) {
        return Err(EngineError::UnsupportedFeature(
            "v7 capture policy pre-September18 royal identity".into(),
        ));
    }
    if state
        .extra
        .get("chaosNoCaptureUntilHalfTurn")
        .is_some_and(|value| !value.is_null() && !value.is_number())
    {
        return Err(EngineError::UnsupportedFeature(
            "v7 chaos capture deadline must be numeric".into(),
        ));
    }
    if let Some(value) = state.extra.get("armistice")
        && !value.is_null()
        && !value.get("remaining").unwrap_or(value).is_number()
    {
        return Err(EngineError::UnsupportedFeature(
            "v7 armistice remaining must be numeric".into(),
        ));
    }
    if v7_time_traveler_campaign(state) {
        // source getter는 missing phase를 future로 읽으며 상태를 수정하지 않는다.
        for actor in [Color::White, Color::Black] {
            crate::v7_campaign::time_traveler_attack_enabled_for(state, actor)?;
        }
    }
    for piece in state.board.iter().flatten().flatten() {
        if piece
            .extra
            .get("vigilanceProtection")
            .and_then(|protection| protection.get("remaining"))
            .is_some_and(|remaining| !remaining.is_number())
        {
            return Err(EngineError::UnsupportedFeature(
                "v7 vigilance protection counter must be numeric".into(),
            ));
        }
        if piece
            .extra
            .get("promotionRushUntil")
            .is_some_and(|until| !until.is_null() && !until.is_number())
        {
            return Err(EngineError::UnsupportedFeature(
                "v7 promotion rush deadline must be numeric".into(),
            ));
        }
    }
    Ok(())
}

/// The pinned client treats an enemy concealed by `hiddenFrom`, or by its
/// non-royal camouflage square, as an empty movement occupant. This is a raw
/// movement predicate; it does not grant capture permission. Source
/// isStealthTransparentFor first checks `hiddenFrom`, even for a royal piece.
pub(crate) fn v7_stealth_transparent(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
    at: Square,
) -> bool {
    if state.ruleset_id != RULES_VERSION_V7 || attacker.color == target.color {
        return false;
    }
    let Some(actor) = attacker.color.owner() else {
        return false;
    };
    if target.extra.get("hiddenFrom").and_then(Value::as_str) == Some(actor.as_str()) {
        return true;
    }
    if !crate::observation::truth(state.extra.get("camouflageRule"))
        || state.royal_identity(target)
        || target.color.owner().is_none()
    {
        return false;
    }
    let (row, col) = target
        .extra
        .get("anchorRow")
        .and_then(Value::as_i64)
        .zip(target.extra.get("anchorCol").and_then(Value::as_i64))
        .unwrap_or((i64::from(at.row), i64::from(at.col)));
    let light = (row + col) % 2 == 0;
    if target.color == Color::White {
        light
    } else {
        !light
    }
}

pub(crate) fn can_capture(state: &GameState, attacker: &Piece, target: &Piece) -> bool {
    can_capture_with_options(state, attacker, target, CaptureOptions::default())
}

/// Fallible source capture boundary for non-movement consumers, including
/// square threat probes and large-piece landings. The two source options are
/// explicit so a basic-training capture cannot inherit ordinary permissions.
pub(crate) fn v7_can_capture_target(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
    allow_friendly: bool,
    allow_basic_training_capture: bool,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 capture target requires v7 rules".into(),
        ));
    }
    ensure_v7_capture_policy_scope(state)?;
    if attacker.color.owner().is_some() {
        crate::card_constraints::CaptureConstraints::from_source_state(state, attacker)?;
    }
    Ok(can_capture_with_options(
        state,
        attacker,
        target,
        CaptureOptions {
            allow_friendly,
            allow_basic_training_capture,
            ..CaptureOptions::default()
        },
    ))
}

/// Football은 실제 중립 기물의 속성은 유지하고 별도 color 인자만 사용한다.
/// 기물의 color를 복사본에서 바꾸면 휴전·시간 위상 등의 포획 정책이 달라진다.
pub(crate) fn v7_can_capture_target_as(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
    actor: Color,
    allow_friendly: bool,
    allow_basic_training_capture: bool,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 capture color override requires v7 rules".into(),
        ));
    }
    ensure_v7_capture_policy_scope(state)?;
    if attacker.color.owner().is_some() {
        crate::card_constraints::CaptureConstraints::from_source_state(state, attacker)?;
    }
    Ok(can_capture_with_options(
        state,
        attacker,
        target,
        CaptureOptions {
            capture_color: Some(actor.into()),
            allow_friendly,
            allow_basic_training_capture,
            ..CaptureOptions::default()
        },
    ))
}

/// main98520 canCaptureTarget의 attacker=null 경계다. color/type만 읽으며
/// 보드의 기물 ID·VIP·속성을 가진 임시 Piece를 만들지 않는다. Socialism,
/// Frontline, Saturation, Desperado, 시간 위상 등 실제 attacker 조건은 생략한다.
pub(crate) fn v7_can_capture_target_without_attacker(
    state: &GameState,
    actor: Color,
    target: &Piece,
    attacker_kind: &str,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 capture without attacker requires v7 rules".into(),
        ));
    }
    ensure_v7_capture_policy_scope(state)?;
    let target_truth = |field| crate::observation::truth(target.extra.get(field));
    if target.kind == "wall" || target_truth("submerged") || target.color == actor {
        return Ok(false);
    }
    // 없는 attacker는 Socialism·왕명·BasicTraining 접근을 얻을 수 없다.
    if matches!(
        attacker_kind,
        "campfire" | "paladin" | "revolvingDoor" | "idol" | "recruiter" | "guard"
    ) {
        return Ok(false);
    }
    if target.kind != "scarecrow" && target_truth("metalized") {
        return Ok(false);
    }
    let target_type = if ["regencyHeir", "crownRoyal", "editorRoyal"]
        .iter()
        .any(|field| target_truth(field))
    {
        "king".into()
    } else {
        v7_nominal_nullification_type(&target.kind)
    };
    if target.kind != "scarecrow"
        && target_truth("nullification")
        && target_type == v7_nominal_nullification_type(attacker_kind)
    {
        return Ok(false);
    }
    if v7_chaos_capture_locked(state) {
        return Ok(false);
    }
    if attacker_kind == "monster" && matches!(target.kind.as_str(), "darkWizard" | "scarecrow") {
        return Ok(false);
    }
    if attacker_kind == "queen"
        && target.kind == "pawn"
        && crate::observation::truth(
            state
                .extra
                .get("genevaConvention")
                .and_then(|sides| sides.get(target.color.as_str())),
        )
    {
        return Ok(false);
    }
    if v7_armistice_active(state) && target.color == actor.opponent() {
        return Ok(false);
    }
    if state
        .extra
        .get("freeMoveCaptureLock")
        .and_then(|sides| sides.get(actor.as_str()))
        == Some(&Value::Bool(true))
        && target.color == actor.opponent()
    {
        return Ok(false);
    }
    let nominal_overwhelm_king = matches!(
        attacker_kind,
        "king" | "royalKnight" | "shotgunKing" | "merchant" | "timeTraveler" | "vampireLord"
    );
    let target_overwhelm_king = target_truth("regencyHeir")
        || target.extra.get("crownRoyal") == Some(&Value::Bool(true))
        || matches!(
            target.kind.as_str(),
            "king" | "royalKnight" | "shotgunKing" | "merchant" | "timeTraveler" | "vampireLord"
        );
    let target_overwhelm_royal = target_overwhelm_king
        || target.kind == "queen" && target.extra.get("regencyHeir") != Some(&Value::Bool(true));
    if nominal_overwhelm_king
        && target_overwhelm_royal
        && crate::observation::truth(
            state
                .extra
                .get("overwhelm")
                .and_then(|sides| sides.get(target.color.as_str())),
        )
    {
        return Ok(false);
    }
    if target.ability_kind() == "guard"
        || target.ability_kind() == "revolvingDoor" && v7_uses_revolving_door_guard(state)
        || target
            .extra
            .get("captureRestriction")
            .and_then(Value::as_str)
            == Some("immune")
        || frozen(target)
    {
        return Ok(false);
    }
    if target.kind != "scarecrow"
        && (target_truth("protected")
            || crate::card_effects::js_number(
                target
                    .extra
                    .get("vigilanceProtection")
                    .and_then(|window| window.get("remaining")),
                0,
            )
            .unwrap_or(0.0)
                > 0.0)
    {
        return Ok(false);
    }
    if target.kind == "football" || target.kind == "monster" && attacker_kind != "darkWizard" {
        return Ok(false);
    }
    // 명목상 king 종류라도 isRoyalIdentityPiece(null)은 false다. VIP나
    // Regency 상태를 임시 공격자에 붙여 Jester 허가를 만들면 안 된다.
    if target.ability_kind() == "jester"
        || target
            .extra
            .get("captureRestriction")
            .and_then(Value::as_str)
            == Some("royal-only")
    {
        return Ok(false);
    }
    Ok(true)
}

fn v7_nominal_nullification_type(kind: &str) -> String {
    let mut chars = kind.chars().peekable();
    let mut normalized = String::with_capacity(kind.len());
    while let Some(letter) = chars.next() {
        if letter == '-' && chars.peek().is_some_and(char::is_ascii_lowercase) {
            normalized.push(
                chars
                    .next()
                    .expect("checked next letter")
                    .to_ascii_uppercase(),
            );
        } else {
            normalized.push(letter);
        }
    }
    if matches!(
        normalized.as_str(),
        "king" | "royalKnight" | "shotgunKing" | "darkWizard" | "merchant"
    ) {
        "king".into()
    } else {
        normalized
    }
}

/// 재관측 후 확보한 목적지·portal 점유자와 Mad Horse 분기다.
pub(crate) struct V7CaptureLanding<'a> {
    pub(crate) actual_destination: Square,
    pub(crate) landing_target: Option<&'a Piece>,
    pub(crate) portal_entry_target: Option<&'a Piece>,
    pub(crate) mad_horse_entry: bool,
    pub(crate) mad_horse_exit: bool,
}

/// applyMove91949의 재관측 이후 시도 가드. false는 source의 취소/return이며,
/// 지원하지 않는 상태와 잘못된 입력은 Result 오류로 구별한다.
pub(crate) fn v7_move_attempt_capture_allowed(
    state: &GameState,
    moving: &Piece,
    from: Square,
    target: &MoveTarget,
    landing: V7CaptureLanding<'_>,
) -> Result<bool> {
    let V7CaptureLanding {
        actual_destination,
        landing_target,
        portal_entry_target,
        mad_horse_entry,
        mad_horse_exit,
    } = landing;
    if state.ruleset_id != RULES_VERSION_V7
        || from.row >= 8
        || from.col >= 8
        || actual_destination.row >= 8
        || actual_destination.col >= 8
    {
        return Err(EngineError::InvalidState(
            "v7 move-attempt capture query requires bounded v7 coordinates".into(),
        ));
    }
    ensure_v7_capture_policy_scope(state)?;
    if target.flag("mistakeReverse") {
        return Ok(true);
    }
    let mut candidates: Vec<&Piece> = Vec::new();
    for victim in [
        landing_target,
        portal_entry_target,
        target
            .flags
            .get("capturedRow")
            .and_then(Value::as_u64)
            .zip(target.flags.get("capturedCol").and_then(Value::as_u64))
            .filter(|&(r, c)| target.flag("enPassant") && r < 8 && c < 8)
            .and_then(|(row, col)| {
                state.at(Square {
                    row: row as u8,
                    col: col as u8,
                })
            }),
        target
            .flags
            .get("jumpCapture")
            .and_then(v7_descriptor_square)
            .filter(|_| target.flag("jumpCapture"))
            .and_then(|at| state.at(at)),
    ]
    .into_iter()
    .flatten()
    {
        if candidates.iter().any(|p| p.id == victim.id) {
            continue;
        }
        let friendly = mad_horse_entry && portal_entry_target.is_some_and(|p| p.id == victim.id)
            || mad_horse_exit && landing_target.is_some_and(|p| p.id == victim.id);
        if v7_can_capture_target(
            state,
            moving,
            victim,
            friendly,
            target.flag("basicTrainingCapture"),
        )? {
            candidates.push(victim);
        }
    }
    let saturated = crate::v7_capture_reactions::saturation_locked(state, moving);
    if saturated && !candidates.is_empty() {
        return Ok(false);
    }
    if landing_target.is_some_and(|p| p.color == moving.color)
        && !mad_horse_exit
        && !v7_can_resolve_siege_ram_move(moving, target)
    {
        return Ok(false);
    }
    if [landing_target, portal_entry_target]
        .into_iter()
        .flatten()
        .any(frozen)
    {
        return Ok(false);
    }
    let entry_at = target
        .flags
        .get("portalEntry")
        .and_then(v7_descriptor_square)
        .unwrap_or(actual_destination);
    let visible = [
        (landing_target, actual_destination),
        (portal_entry_target, entry_at),
    ]
    .into_iter()
    .any(|(p, at)| p.is_some_and(|p| !v7_stealth_transparent(state, moving, p, at)));
    if visible
        && moving.ability_kind() == "recruiter"
        && !target.flag("basicTrainingCapture")
        && !v7_royal_command_capture_access(state, moving)
    {
        return Ok(false);
    }
    if (visible || target.flag("enPassant") || target.flag("jumpCapture"))
        && (v7_manner_capture_locked(state, moving)
            || saturated
            || v7_initiative_capture_locked(state, moving.color)
            || crate::observation::truth(moving.extra.get("repositionSecondMove")))
    {
        return Ok(false);
    }
    let mut actual = target.clone();
    actual.row = actual_destination.row;
    actual.col = actual_destination.col;
    if (landing_target.is_some() || portal_entry_target.is_some())
        && !crate::v7_rule_geometry::v7_high_ground_capture_allowed(state, moving, from, &actual)?
    {
        return Ok(false);
    }
    if [landing_target, portal_entry_target]
        .into_iter()
        .flatten()
        .any(|p| p.kind == "jester")
        && (moving.kind == "jester"
            || !crate::observation::truth(moving.extra.get("crownBearer"))
                && !state.royal_identity(moving))
    {
        return Ok(false);
    }
    if matches!(moving.kind.as_str(), "bishop" | "rook" | "queen")
        && !target.flag("substitutionSwap")
    {
        for (victim, at) in [
            (landing_target, actual_destination),
            (portal_entry_target, entry_at),
        ] {
            if let Some(victim) = victim
                && victim.color != moving.color
                && from.row.abs_diff(at.row).max(from.col.abs_diff(at.col)) >= 2
                && crate::v7_campaign::is_blood_veil_active(state, victim)?
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum V7MoveOriginVerdict {
    Allowed,
    Cancel,
    ClearSelection,
}

pub(crate) fn v7_move_attempt_origin_verdict(
    state: &GameState,
    moving: &Piece,
    from: Square,
    target: &MoveTarget,
) -> Result<V7MoveOriginVerdict> {
    if state.ruleset_id != RULES_VERSION_V7
        || from.row >= 8
        || from.col >= 8
        || target.row >= 8
        || target.col >= 8
    {
        return Err(EngineError::InvalidState(
            "v7 move-attempt origin query requires bounded v7 coordinates".into(),
        ));
    }
    let cancel = V7MoveOriginVerdict::Cancel;
    let stationary = [
        "setLogDirection",
        "colossusBody",
        "colossusAttack",
        "shotgunBlast",
        "shotgunSnipe",
        "merchantBuy",
    ]
    .iter()
    .any(|f| target.flag(f));
    if crate::observation::truth(moving.extra.get("grapplerBound"))
        && !stationary
        && target.square() != from
    {
        return Ok(cancel);
    }
    if [
        "relaySwap",
        "substitutionSwap",
        "dragonSwap",
        "solidaritySwap",
    ]
    .iter()
    .any(|f| target.flag(f))
        && state
            .at(target.square())
            .is_some_and(|p| crate::observation::truth(p.extra.get("grapplerBound")))
    {
        return Ok(cancel);
    }
    if target.flag("enPassant") {
        let victim = target
            .flags
            .get("capturedRow")
            .and_then(Value::as_u64)
            .zip(target.flags.get("capturedCol").and_then(Value::as_u64))
            .filter(|&(r, c)| r < 8 && c < 8)
            .and_then(|(row, col)| {
                state.at(Square {
                    row: row as u8,
                    col: col as u8,
                })
            });
        let Some(victim) = victim else {
            return Ok(V7MoveOriginVerdict::ClearSelection);
        };
        if !v7_can_capture_target(state, moving, victim, false, false)? {
            return Ok(V7MoveOriginVerdict::ClearSelection);
        }
        if let Some(landing) = state.at(target.square())
            && !v7_can_capture_target(state, moving, landing, false, false)?
        {
            return Ok(cancel);
        }
    }
    if v7_large_move_lands_on_protected(state, moving, target) {
        return Ok(cancel);
    }
    if target.flags.get("atomicBasePromotion") == Some(&json!(true))
        && (crate::v7_promotion::atomic_base_promotion_type_v7(target).is_none()
            || !crate::v7_promotion::is_atomic_base_promotion_move_v7(
                state,
                moving,
                target.square(),
                target,
            )?)
    {
        return Ok(cancel);
    }
    if let Some(actor) = moving.color.owner() {
        let turn = f64::from(*state.turns_taken.get(actor));
        if state.turn == actor
            && crate::card_effects::js_number(moving.extra.get("idolEncoreRestTurn"), 0)
                .is_some_and(|rest| rest.is_finite() && rest == turn)
        {
            return Ok(cancel);
        }
        if crate::card_effects::js_number(moving.extra.get("poisonStunTurns"), 0)
            .unwrap_or(0.0)
            .floor()
            > 0.0
        {
            return Ok(cancel);
        }
        if !crate::observation::truth(moving.extra.get("regencyHeir"))
            && !crate::observation::truth(moving.extra.get("crownRoyal"))
            && !matches!(
                moving.kind.as_str(),
                "king" | "royalKnight" | "shotgunKing" | "darkWizard"
            )
            && let Some(entry) = state
                .extra
                .get("exhaustion")
                .and_then(|v| v.get(actor.as_str()))
            && entry.get("enabled").is_some_and(v7_truth)
            && !moving.id.is_empty()
            && entry.get("pieceId").and_then(Value::as_str) == Some(moving.id.as_str())
            && crate::card_effects::js_number(entry.get("count"), 0)
                .unwrap_or(0.0)
                .max(0.0)
                .floor()
                >= 3.0
        {
            return Ok(cancel);
        }
    } else if crate::card_effects::js_number(moving.extra.get("poisonStunTurns"), 0)
        .unwrap_or(0.0)
        .floor()
        > 0.0
    {
        return Ok(cancel);
    }
    if frozen(moving) {
        return Ok(cancel);
    }
    // Football은 원문91522에서 자신의 실행 함수로 먼저 분기한다.
    if moving.kind != "football"
        && !target.flag("mistakeReverse")
        && v7_move_crosses_scarecrow_reservation(state, moving, from, target)?
    {
        return Ok(V7MoveOriginVerdict::ClearSelection);
    }
    Ok(V7MoveOriginVerdict::Allowed)
}

fn v7_large_move_lands_on_protected(state: &GameState, piece: &Piece, target: &MoveTarget) -> bool {
    if !piece.is_large() || target.flag("colossusBody") || target.flag("colossusAttack") {
        return false;
    }
    let row = target
        .flags
        .get("anchorRow")
        .filter(|v| !v.is_null())
        .unwrap_or(&json!(target.row))
        .as_f64();
    let col = target
        .flags
        .get("anchorCol")
        .filter(|v| !v.is_null())
        .unwrap_or(&json!(target.col))
        .as_f64();
    let Some((row, col)) = row
        .zip(col)
        .filter(|(r, c)| r.fract() == 0.0 && c.fract() == 0.0)
    else {
        return false;
    };
    for dr in 0..2 {
        for dc in 0..2 {
            let (r, c) = (row + f64::from(dr), col + f64::from(dc));
            if !(0.0..8.0).contains(&r) || !(0.0..8.0).contains(&c) {
                continue;
            }
            let at = Square {
                row: r as u8,
                col: c as u8,
            };
            if state.at(at).is_some_and(|p| {
                p.id != piece.id
                    && (crate::observation::truth(p.extra.get("protected"))
                        || crate::card_effects::js_number(
                            p.extra
                                .get("vigilanceProtection")
                                .and_then(|v| v.get("remaining")),
                            0,
                        )
                        .unwrap_or(0.0)
                            > 0.0
                        || v7_encouraged_at(state, p, at))
            }) {
                return true;
            }
        }
    }
    false
}

/// Automatic Logs and neutral rule monsters use the source capture predicate
/// with saturation ignored; turn movement locks remain the caller's stage.
pub(crate) fn v7_can_capture_target_automatic(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
    ignore_saturation: bool,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 automatic capture predicate requires v7".into(),
        ));
    }
    ensure_v7_capture_policy_scope(state)?;
    if attacker.color.owner().is_some() {
        crate::card_constraints::CaptureConstraints::from_source_state(state, attacker)?;
    }
    Ok(can_capture_with_options(
        state,
        attacker,
        target,
        CaptureOptions {
            ignore_saturation,
            ..CaptureOptions::default()
        },
    ))
}

pub(crate) fn v7_double_check_geometry(
    state: &GameState,
    piece: &Piece,
    from: Square,
    royal: Square,
) -> Result<bool> {
    crate::variant_movement::v7_double_check_geometry(state, piece, from, royal)
}

fn v7_descriptor_square(value: &Value) -> Option<Square> {
    let row = value.get("row")?.as_f64()?;
    let col = value.get("col")?.as_f64()?;
    if row.is_finite()
        && col.is_finite()
        && row.fract() == 0.0
        && col.fract() == 0.0
        && (0.0..8.0).contains(&row)
        && (0.0..8.0).contains(&col)
    {
        Some(Square {
            row: row as u8,
            col: col as u8,
        })
    } else {
        None
    }
}

fn v7_portal_capture_cells(target: &MoveTarget) -> Vec<Square> {
    let mut cells = Vec::with_capacity(2);
    if target.flag("portalLanding")
        && let Some(entry) = target
            .flags
            .get("portalEntry")
            .and_then(v7_descriptor_square)
    {
        cells.push(entry);
    }
    let portal_destination = target
        .flag("portalLanding")
        .then(|| {
            target
                .flags
                .get("portalExit")
                .and_then(v7_descriptor_square)
        })
        .flatten();
    let anchor_destination = target
        .flags
        .get("anchorRow")
        .and_then(Value::as_f64)
        .zip(target.flags.get("anchorCol").and_then(Value::as_f64))
        .filter(|(row, col)| {
            row.fract() == 0.0
                && col.fract() == 0.0
                && (0.0..8.0).contains(row)
                && (0.0..8.0).contains(col)
        })
        .map(|(row, col)| Square {
            row: row as u8,
            col: col as u8,
        });
    let direct = Square::new(target.row, target.col).ok();
    if let Some(destination) = portal_destination.or(anchor_destination).or(direct) {
        cells.push(destination);
    }
    cells.dedup();
    cells
}

/// Frozen `moveCaptureTargetCells`: source-ordered, in-bounds target cells of
/// an internal move descriptor. The royal-ID threat check uses this before
/// capture classification, so protected or hidden occupants are retained.
pub(crate) fn v7_move_capture_target_cells(target: &MoveTarget) -> Result<Vec<Square>> {
    if target.flag("setLogDirection") {
        return Ok(Vec::new());
    }
    let mut cells = Vec::with_capacity(16);
    if target.flag("enPassant")
        && let (Some(row), Some(col)) = (
            target.flags.get("capturedRow").and_then(Value::as_i64),
            target.flags.get("capturedCol").and_then(Value::as_i64),
        )
        && (0..8).contains(&row)
        && (0..8).contains(&col)
    {
        cells.push(Square {
            row: row as u8,
            col: col as u8,
        });
    }
    cells.extend(v7_portal_capture_cells(target));
    if let Some(square) = target
        .flags
        .get("jumpCapture")
        .and_then(v7_descriptor_square)
    {
        cells.push(square);
    }
    for field in [
        "sectorCells",
        "colossusLandingCaptures",
        "bigRookLandingCaptures",
    ] {
        if let Some(entries) = target.flags.get(field).and_then(Value::as_array) {
            if entries.len() > 4096 {
                return Err(EngineError::UnsupportedFeature(format!(
                    "v7 {field} target cell capacity"
                )));
            }
            cells.extend(entries.iter().filter_map(v7_descriptor_square));
        }
    }
    if (target.flag("shotgunBlast") || target.flag("colossusAttack") || target.flag("siegeRamMove"))
        && let Some(entries) = target.flags.get("highlightCells").and_then(Value::as_array)
    {
        if entries.len() > 4096 {
            return Err(EngineError::UnsupportedFeature(
                "v7 highlight target cell capacity".into(),
            ));
        }
        cells.extend(entries.iter().filter_map(v7_descriptor_square));
    }
    let mut seen = BTreeSet::new();
    cells.retain(|square| seen.insert(*square));
    Ok(cells)
}

#[derive(Default, Clone, Copy)]
struct CaptureOptions {
    capture_color: Option<PieceColor>,
    allow_basic_training_capture: bool,
    allow_friendly: bool,
    ignore_chaos_capture_lock: bool,
    ignore_free_move_lock: bool,
    ignore_saturation: bool,
}

fn can_capture_with_options(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
    options: CaptureOptions,
) -> bool {
    let v7 = state.ruleset_id == RULES_VERSION_V7;
    if desperado_royal_capture_blocked(attacker, target) {
        return false;
    }
    let ability = attacker.ability_kind();
    let actor = attacker.color.owner();
    let capture_color = options.capture_color.unwrap_or(attacker.color);
    if !v7 && actor.is_none() {
        return false;
    }
    let owner_turns = actor
        .map(|color| *state.turns_taken.get(color))
        .unwrap_or(0);
    if !v7
        && (crate::observation::number(
            attacker
                .extra
                .get("disarmed")
                .and_then(|entry| entry.get("remaining")),
        )
        .unwrap_or(0.0)
            > 0.0
            || attacker.kind != "monster"
                && crate::observation::truth(attacker.extra.get("potionManner"))
                && crate::observation::truth(attacker.extra.get("coolGuyCapturedLast")))
    {
        return false;
    }
    for lock in ["freshNoCaptureUntil", "cardNoCaptureUntil"] {
        if v7 && lock == "freshNoCaptureUntil" {
            continue;
        }
        if crate::observation::number(attacker.extra.get(lock)).unwrap_or(0.0)
            > f64::from(owner_turns)
        {
            return false;
        }
    }
    if v7
        && crate::observation::number(attacker.extra.get("promotionRushUntil"))
            .is_some_and(|until| until > f64::from(owner_turns))
    {
        return false;
    }
    if v7
        && ((target.flag("metalized") && target.kind != "scarecrow")
            || (ability == "monster" && matches!(target.kind.as_str(), "darkWizard" | "scarecrow"))
            || (target.kind == "monster" && ability != "darkWizard")
            || !options.ignore_chaos_capture_lock
                && attacker.color == capture_color
                && v7_chaos_capture_locked(state)
            || v7_armistice_active(state)
                && actor.is_some_and(|actor| target.color == actor.opponent())
            || !options.ignore_free_move_lock
                && state
                    .extra
                    .get("freeMoveCaptureLock")
                    .and_then(|lock| lock.get(attacker.color.as_str()))
                    == Some(&json!(true))
                && actor.is_some_and(|actor| target.color == actor.opponent())
            || v7_overwhelm_capture_blocked(state, attacker, target)
            || !v7_time_phase_interacts(state, attacker, target)
            || attacker.kind == "timeTraveler"
                && attacker.color.owner().is_none_or(|actor| {
                    !crate::v7_campaign::time_traveler_attack_enabled_for(state, actor)
                        .unwrap_or(false)
                })
            || v7_frontline_capture_blocked(state, attacker, target))
    {
        return false;
    }
    if target.color == capture_color && !(v7 && options.allow_friendly)
        || matches!(target.kind.as_str(), "wall" | "football" | "guard")
        || !v7 && target.kind == "blackHole"
        || if v7 {
            target.kind != "scarecrow"
                && (target.flag("protected")
                    || crate::observation::number(
                        target
                            .extra
                            .get("vigilanceProtection")
                            .and_then(|protection| protection.get("remaining")),
                    )
                    .unwrap_or(0.0)
                        > 0.0)
        } else {
            target.flag("protected")
        }
        || frozen(target)
        || target.flag("submerged")
        || target.ability_kind() == "guard"
        || target.ability_kind() == "revolvingDoor" && (!v7 || v7_uses_revolving_door_guard(state))
        || !v7 && encouraged(state, target)
    {
        return false;
    }
    // main1193: physical royal augmentation identity is used here, including
    // inactive regency/editor side flags. Copied movement is not that identity.
    fn nullification_identity(piece: &Piece) -> &str {
        if ["regencyHeir", "crownRoyal", "editorRoyal"]
            .iter()
            .any(|field| crate::observation::truth(piece.extra.get(*field)))
            || matches!(
                piece.kind.as_str(),
                "king" | "royalKnight" | "shotgunKing" | "darkWizard" | "merchant"
            )
        {
            "king"
        } else {
            piece.kind.as_str()
        }
    }
    if target.kind != "scarecrow"
        && crate::observation::truth(target.extra.get("nullification"))
        && target.color != attacker.color
        && nullification_identity(target) == nullification_identity(attacker)
    {
        return false;
    }
    let socialist = (if v7 {
        capture_color
            .owner()
            .is_some_and(|actor| v7_positive_side_counter(state, "socialism", actor))
    } else {
        state.flag("socialism", attacker.color)
    }) && !state.royal_identity(attacker)
        && attacker.kind != "crown"
        && ability != "slime";
    let v7_royal_command = v7 && v7_royal_command_capture_access(state, attacker);
    let basic_training_access =
        v7 && options.allow_basic_training_capture && attacker.flag("basicTraining");
    if matches!(ability, "campfire" | "paladin") && !socialist
        || matches!(ability, "recruiter" | "guard")
            && !socialist
            && !basic_training_access
            && !v7_royal_command
        || ability == "revolvingDoor"
            && !socialist
            && (!v7
                || !v7_uses_revolving_door_guard(state)
                || !basic_training_access && !v7_royal_command)
    {
        return false;
    }
    if ability == "idol" && !attacker.flag("crownBearer") && !socialist {
        return false;
    }
    if !options.ignore_saturation
        && (if v7 {
            crate::v7_capture_reactions::saturation_locked(state, attacker)
        } else {
            (crate::observation::truth(state.extra.get("saturationRule"))
                || crate::observation::truth(attacker.extra.get("potionSaturation")))
                && crate::observation::number(attacker.extra.get("capturesMade")).unwrap_or(0.0)
                    >= 3.0
        })
    {
        return false;
    }
    if state.flag("genevaConvention", target.color)
        && attacker.kind == "queen"
        && (!v7 || !attacker.flag("regencyHeir"))
        && target.kind == "pawn"
    {
        return false;
    }
    if target
        .extra
        .get("captureRestriction")
        .and_then(Value::as_str)
        == Some("immune")
    {
        return false;
    }
    if (target.ability_kind() == "jester"
        || target
            .extra
            .get("captureRestriction")
            .and_then(Value::as_str)
            == Some("royal-only"))
        && (ability == "jester"
            || !(attacker.flag("crownBearer") || state.royal_identity(attacker)))
    {
        return false;
    }
    true
}

/// Source isCaptureMove는 이미 열거한 후보를 포획 행동으로 분류한다.
/// 별도 단계의 Chaos·FreeMove·Saturation 제한은 분류에서 제외하며,
/// 경로 포획은 원본 descriptor에 실린 실제 점유 셀로 판정한다.
pub(crate) fn v7_is_capture_move(
    state: &GameState,
    attacker: &Piece,
    target: &MoveTarget,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 capture classification requires v7 rules profile".into(),
        ));
    }
    ensure_v7_capture_policy_scope(state)?;
    if target.flag("brutusBetrayal") {
        return Ok(true);
    }
    if ["setLogDirection", "substitutionSwap", "relaySwap"]
        .into_iter()
        .any(|flag| target.flag(flag))
    {
        return Ok(false);
    }
    if [
        "missionaryConvert",
        "madHorseCapture",
        "madHorsePortalEntryCapture",
        "madHorsePortalExitCapture",
    ]
    .into_iter()
    .any(|flag| target.flag(flag))
    {
        return Ok(true);
    }
    if target.flag("siegeRamMove") {
        // Source siegeRamPathEntries reads the supplied ordered path cells;
        // capture classification counts every occupied identity, including
        // allied/neutral pieces and victims ordinary capture would reject.
        let cells = target.flags.get("highlightCells").and_then(Value::as_array);
        return Ok(cells.is_some_and(|cells| {
            cells
                .iter()
                .filter_map(v7_descriptor_square)
                .any(|at| state.at(at).is_some())
        }));
    }
    if [
        "enPassant",
        "jumpCapture",
        "colossusAttack",
        "shotgunBlast",
        "shotgunSnipe",
        "merchantBuy",
    ]
    .into_iter()
    .any(|flag| target.flag(flag))
        || ["colossusLandingCaptures", "bigRookLandingCaptures"]
            .into_iter()
            .any(|field| {
                target
                    .flags
                    .get(field)
                    .and_then(Value::as_array)
                    .is_some_and(|cells| !cells.is_empty())
            })
    {
        return Ok(true);
    }
    let capture_cells = v7_portal_capture_cells(target);
    let options = CaptureOptions {
        allow_basic_training_capture: target.flag("basicTrainingCapture"),
        ignore_chaos_capture_lock: true,
        ignore_free_move_lock: true,
        ignore_saturation: true,
        ..CaptureOptions::default()
    };
    Ok(capture_cells.into_iter().any(|square| {
        state.at(square).is_some_and(|victim| {
            victim.color != attacker.color
                && !matches!(victim.kind.as_str(), "wall" | "football")
                && !frozen(victim)
                && can_capture_with_options(state, attacker, victim, options)
        })
    }))
}
pub(crate) fn encouraged(state: &GameState, target: &Piece) -> bool {
    if state.ruleset_id == RULES_VERSION_V7 {
        return find_square(state, &target.id)
            .is_some_and(|cell| v7_encouraged_at(state, target, cell));
    }
    if target.kind == "scarecrow" {
        return false;
    }
    if target.flag("outpostProtected") {
        return true;
    }
    let Some(cell) = find_square(state, &target.id) else {
        return false;
    };
    if !(if state.ruleset_id == RULES_VERSION_V7 {
        v7_king_augment_recipient(state, target)
    } else {
        state.royal_identity(target)
    }) && ORTHO.iter().any(|delta| {
        cell.offset(delta.0, delta.1)
            .and_then(|square| state.at(square))
            .is_some_and(|piece| piece.kind == "campfire" && piece.color == target.color)
    }) {
        return true;
    }
    if !state.flag("encouragement", target.color) {
        return false;
    }
    for row in 0..8 {
        for col in 0..8 {
            let king = Square { row, col };
            if state.at(king).is_some_and(|piece| {
                piece.color == target.color && (piece.is_royal() || piece.flag("regencyHeir"))
            }) {
                return row.abs_diff(cell.row) + col.abs_diff(cell.col) == 1;
            }
        }
    }
    false
}

/// Campfire uses the complete logical body, while Encouragement measures
/// distance from the actual capture cell passed by the source caller.
pub(crate) fn v7_encouraged_at(state: &GameState, target: &Piece, at: Square) -> bool {
    if target.kind == "scarecrow" {
        return false;
    }
    if target.flag("outpostProtected") {
        return true;
    }
    let mut body = Vec::new();
    let mut fires = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let cell = Square { row, col };
            let Some(item) = state.at(cell) else {
                continue;
            };
            if item.id == target.id {
                body.push(cell);
            }
            if item.id != target.id
                && item.color == target.color
                && (item.kind == "campfire"
                    || item.kind == "trickster"
                        && item.extra.get("tricksterMoveType").and_then(Value::as_str)
                            == Some("campfire"))
            {
                fires.push(cell);
            }
        }
    }
    if !v7_king_augment_recipient(state, target)
        && body.iter().any(|cell| {
            fires
                .iter()
                .any(|fire| cell.row.abs_diff(fire.row) + cell.col.abs_diff(fire.col) == 1)
        })
    {
        return true;
    }
    if !state.flag("encouragement", target.color) {
        return false;
    }
    for row in 0..8 {
        for col in 0..8 {
            if state.at(Square { row, col }).is_some_and(|king| {
                king.color == target.color && (king.is_royal() || king.flag("regencyHeir"))
            }) {
                return row.abs_diff(at.row) + col.abs_diff(at.col) == 1;
            }
        }
    }
    false
}
pub(crate) fn find_square(state: &GameState, id: &str) -> Option<Square> {
    for row in 0..8 {
        for col in 0..8 {
            if state.at(Square { row, col }).is_some_and(|p| p.id == id) {
                return Some(Square { row, col });
            }
        }
    }
    None
}
pub(crate) fn collapsed(state: &GameState, square: Square) -> bool {
    if square.row >= 8 || square.col >= 8 {
        return false;
    }
    if state.ruleset_id == RULES_VERSION_V7 {
        let depth = v7_collapse_depth(state);
        if depth > 0
            && (square.row < depth
                || square.col < depth
                || square.row >= 8 - depth
                || square.col >= 8 - depth)
        {
            return true;
        }
    } else if state
        .extra
        .get("collapsed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let depth = state
            .extra
            .get("collapseDepth")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        if u64::from(square.row) < depth
            || u64::from(square.col) < depth
            || u64::from(7 - square.row) < depth
            || u64::from(7 - square.col) < depth
        {
            return true;
        }
    }
    state
        .extra
        .get("collapsedCells")
        .and_then(Value::as_array)
        .is_some_and(|cells| {
            cells.iter().any(|cell| {
                cell.get("row").and_then(Value::as_u64) == Some(u64::from(square.row))
                    && cell.get("col").and_then(Value::as_u64) == Some(u64::from(square.col))
            })
        })
}

fn v7_collapse_depth(state: &GameState) -> u8 {
    let fallback = if crate::observation::truth(state.extra.get("collapsed")) {
        1.0
    } else {
        0.0
    };
    // Source normalizeCollapseDepth uses Number(value) || legacy fallback.
    let value = state.extra.get("collapseDepth");
    let number = crate::card_effects::js_number(value, 0).or_else(|| {
        fn infinity(value: &Value, depth: usize) -> Option<f64> {
            if depth > 64 {
                return None;
            }
            match value {
                Value::String(text) if matches!(text.trim(), "Infinity" | "+Infinity") => {
                    Some(f64::INFINITY)
                }
                Value::String(text) if text.trim() == "-Infinity" => Some(f64::NEG_INFINITY),
                Value::Array(values) if values.len() == 1 => infinity(&values[0], depth + 1),
                _ => None,
            }
        }
        value.and_then(|value| infinity(value, 0))
    });
    number
        .filter(|value| *value != 0.0)
        .unwrap_or(fallback)
        .floor()
        .clamp(0.0, 4.0) as u8
}
pub(crate) fn landing(state: &GameState, piece: &Piece, square: Square) -> bool {
    !collapsed(state, square)
        && state
            .at(square)
            .is_none_or(|target| can_capture(state, piece, target))
}
pub(crate) fn leaps(
    state: &GameState,
    piece: &Piece,
    from: Square,
    deltas: &[(i8, i8)],
) -> Vec<MoveTarget> {
    deltas
        .iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .filter(|&to| landing(state, piece, to))
        .map(MoveTarget::at)
        .collect()
}

/// Source jumpMoves accepts a concealed enemy square as a movement candidate
/// before evaluating canCaptureTarget. Keep this distinct from the ordinary
/// knightMoves and slimeMoves kernels, which do not use that shortcut.
pub(crate) fn v7_jump_leaps(
    state: &GameState,
    piece: &Piece,
    from: Square,
    deltas: &[(i8, i8)],
) -> Vec<MoveTarget> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return leaps(state, piece, from, deltas);
    }
    deltas
        .iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .filter(|&to| {
            !collapsed(state, to)
                && state.at(to).is_none_or(|target| {
                    v7_stealth_transparent(state, piece, target, to)
                        || can_capture(state, piece, target)
                })
        })
        .map(MoveTarget::at)
        .collect()
}
pub(crate) fn rays(
    state: &GameState,
    piece: &Piece,
    from: Square,
    directions: &[(i8, i8)],
    limit: u8,
) -> Vec<MoveTarget> {
    if state.ruleset_id == RULES_VERSION_V7 && limit == 7 {
        return v7_source_ray_moves(state, piece, from, directions);
    }
    if state.ruleset_id == RULES_VERSION_V7 {
        let mut moves = Vec::new();
        for &(dr, dc) in directions {
            for distance in 1..=limit {
                let Some(to) = from.offset(dr * distance as i8, dc * distance as i8) else {
                    break;
                };
                if let Some(victim) = state.at(to) {
                    if can_capture(state, piece, victim) {
                        moves.push(MoveTarget::at(to));
                    }
                    break;
                }
                moves.push(MoveTarget::at(to));
            }
        }
        return unique_v7_coordinates(moves);
    }
    let mut moves = Vec::new();
    for &(dr, dc) in directions {
        let mut cursor = from;
        for _ in 0..limit {
            let Some(to) = cursor.offset(dr, dc) else {
                break;
            };
            cursor = to;
            if collapsed(state, to) {
                break;
            }
            match state.at(to) {
                None => moves.push(MoveTarget::at(to)),
                Some(target) => {
                    if v7_stealth_transparent(state, piece, target, to) {
                        moves.push(MoveTarget::at(to));
                        continue;
                    }
                    if state.ruleset_id == RULES_VERSION_V7
                        && v7_time_traveler_campaign(state)
                        && !matches!(target.kind.as_str(), "wall" | "football")
                        && !(piece.color == Color::Black && target.color == Color::Black)
                        && !v7_time_phase_interacts(state, piece, target)
                    {
                        continue;
                    }
                    if target.color == piece.color
                        && target.flag("ghost")
                        && piece.kind != "cannon"
                        && crate::card_effects::ranged_piece(state, piece)
                    {
                        continue;
                    }
                    if can_capture(state, piece, target) {
                        moves.push(MoveTarget::at(to));
                    }
                    break;
                }
            }
        }
    }
    moves
}

/// Source normalizePortalRule accepts exactly two distinct valid custom
/// Number-coerced cells; otherwise the frozen c3/f6 defaults are used.
pub(crate) fn v7_portal_cells(state: &GameState) -> Option<[Square; 2]> {
    let value = state.extra.get("portalRule")?;
    if value != &Value::Bool(true) && !crate::observation::truth(value.get("enabled")) {
        return None;
    }
    let custom = value
        .get("cells")
        .and_then(Value::as_array)
        .map(|cells| {
            cells
                .iter()
                .filter_map(|cell| {
                    let row = crate::card_effects::js_number(cell.get("row"), 0)?;
                    let col = crate::card_effects::js_number(cell.get("col"), 0)?;
                    (row.is_finite()
                        && col.is_finite()
                        && row.fract() == 0.0
                        && col.fract() == 0.0
                        && (0.0..8.0).contains(&row)
                        && (0.0..8.0).contains(&col))
                    .then_some(Square {
                        row: row as u8,
                        col: col as u8,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if custom.len() == 2 && custom[0] != custom[1] {
        Some([custom[0], custom[1]])
    } else {
        Some([Square { row: 5, col: 2 }, Square { row: 2, col: 5 }])
    }
}
pub(crate) fn v7_portal_exit_at(state: &GameState, at: Square) -> Option<Square> {
    let cells = v7_portal_cells(state)?;
    if at == cells[0] {
        Some(cells[1])
    } else if at == cells[1] {
        Some(cells[0])
    } else {
        None
    }
}

fn v7_frontline_capture_blocked(state: &GameState, attacker: &Piece, target: &Piece) -> bool {
    if !state.flag("frontlineResponse", target.color) || target.kind != "rook" {
        return false;
    }
    let memory = crate::card_effects::current_base_movement(state, attacker);
    let kind = if attacker.ability_kind() == "brutus" {
        "hook"
    } else if matches!(attacker.ability_kind(), "parrot" | "medium") {
        memory
            .as_ref()
            .and_then(|memory| memory.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("")
    } else {
        attacker.ability_kind()
    };
    let from = find_square(state, &attacker.id);
    let to = find_square(state, &target.id);
    let blocked = kind == "hook"
        || from
            .zip(to)
            .is_some_and(|(from, to)| from.row == to.row || from.col == to.col);
    if !blocked || attacker.ability_kind() != "primeMinister" {
        return blocked;
    }
    let Some((from, to)) = from.zip(to) else {
        return blocked;
    };
    let dr = from.row.abs_diff(to.row);
    let dc = from.col.abs_diff(to.col);
    if dr == 1 && dc == 1 {
        return false;
    }
    if dr.max(dc) > 2 || dr.max(dc) < 1 {
        return true;
    }
    !KING
        .iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .any(|mid| {
            mid.row.abs_diff(to.row) == 1
                && mid.col.abs_diff(to.col) == 1
                && state.at(mid).is_none()
                && !collapsed(state, mid)
                && v7_portal_exit_at(state, mid).is_none()
        })
}

pub(crate) fn v7_time_phase_transparent_blocker(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
) -> bool {
    if !v7_time_traveler_campaign(state) {
        return false;
    }
    if matches!(target.kind.as_str(), "wall" | "football") {
        return false;
    }
    if attacker.color == Color::Black && target.color == Color::Black {
        return false;
    }
    !v7_time_phase_interacts(state, attacker, target)
}

/// 원문의 attackerType 인자는 실제 기물의 포획 정책을 바꾸지 않고
/// Ghost 투명성의 행마 종류만 선택한다. Cannon의 Ghost는 기본적으로 받침이다.
pub(crate) fn v7_ghost_transparent_for(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
    at: Square,
    attacker_type: &str,
) -> bool {
    if v7_stealth_transparent(state, attacker, target, at) {
        return true;
    }
    if target.color != attacker.color || !crate::observation::truth(target.extra.get("ghost")) {
        return false;
    }
    let hash = v7_effective_catalog_hash(state);
    let cannon_screen =
        hash.is_some() || state.extra.get("cannonGhostScreen") != Some(&json!(false));
    if attacker_type == "cannon" && cannon_screen {
        return false;
    }
    let mut movement_identity = attacker.clone();
    movement_identity.kind = attacker_type.into();
    crate::card_effects::ranged_piece(state, &movement_identity)
}

fn v7_ray_transparent(state: &GameState, piece: &Piece, target: &Piece, at: Square) -> bool {
    v7_stealth_transparent(state, piece, target, at)
        || state.flag("overtake", piece.color)
            && piece.kind == "rook"
            && target.color == piece.color
            && !matches!(
                target.kind.as_str(),
                "wall" | "football" | "monster" | "blackHole" | "coffin"
            )
        || v7_time_phase_transparent_blocker(state, piece, target)
        || v7_ghost_transparent_for(state, piece, target, at, &piece.kind)
}

fn v7_source_ray_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    directions: &[(i8, i8)],
) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(dr, dc) in directions {
        let mut next = from.offset(dr, dc);
        let mut transit = None;
        // A source ray can use only one portal transit. Two finite board
        // segments contain at most 14 steps on an 8x8 board.
        for _ in 0..16 {
            let Some(to) = next else {
                break;
            };
            let exit = v7_portal_exit_at(state, to);
            if exit.is_some() && transit.is_some() {
                break;
            }
            let mut descriptor = MoveTarget::at(to);
            if let Some((entry, exit)) = transit {
                descriptor.flags.insert("portalThrough".into(), json!(true));
                descriptor.flags.insert("portalEntry".into(), json!(entry));
                descriptor.flags.insert("portalExit".into(), json!(exit));
            }
            if let Some(victim) = state.at(to) {
                if !v7_ray_transparent(state, piece, victim, to) {
                    if can_capture(state, piece, victim) {
                        moves.push(descriptor);
                    }
                    break;
                }
                if exit.is_none() && v7_stealth_transparent(state, piece, victim, to) {
                    moves.push(descriptor);
                }
                if let Some(exit) = exit {
                    let mut descriptor = MoveTarget::at(to);
                    descriptor
                        .flags
                        .insert("portalTransparentEntry".into(), json!(true));
                    moves.push(descriptor);
                    if state
                        .at(exit)
                        .is_none_or(|victim| v7_ray_transparent(state, piece, victim, exit))
                        && !collapsed(state, to)
                        && !collapsed(state, exit)
                    {
                        transit = Some((to, exit));
                        next = exit.offset(dr, dc);
                        continue;
                    }
                    break;
                }
            } else {
                moves.push(descriptor);
                if let Some(exit) = exit {
                    if state
                        .at(exit)
                        .is_none_or(|victim| v7_ray_transparent(state, piece, victim, exit))
                        && !collapsed(state, to)
                        && !collapsed(state, exit)
                    {
                        transit = Some((to, exit));
                        next = exit.offset(dr, dc);
                        continue;
                    }
                    break;
                }
            }
            next = to.offset(dr, dc);
        }
    }
    unique_v7_coordinates(moves)
}

pub(crate) fn v7_can_portal_land_on(
    state: &GameState,
    piece: &Piece,
    at: Square,
    allow_basic_training: bool,
) -> Result<bool> {
    if at.row >= 8 || at.col >= 8 || collapsed(state, at) {
        return Ok(false);
    }
    let Some(target) = state.at(at) else {
        return Ok(!v7_scarecrow_reserved_square(state, at));
    };
    if target.id == piece.id || target.color == piece.color {
        return Ok(false);
    }
    let ability = piece.ability_kind();
    if ability == "missionary" || ability.is_empty() {
        return Ok(false);
    }
    if matches!(ability, "recruiter" | "guard")
        || ability == "revolvingDoor" && v7_uses_revolving_door_guard(state)
    {
        if !(allow_basic_training && piece.flag("basicTraining")
            || v7_royal_command_capture_access(state, piece))
        {
            return Ok(false);
        }
    } else if matches!(
        ability,
        "wizard" | "herald" | "merchant" | "coffin" | "scarecrow" | "blackHole"
    ) || ability == "jester"
        && !matches!(
            target.kind.as_str(),
            "king" | "royalKnight" | "shotgunKing" | "merchant"
        )
    {
        return Ok(false);
    }
    Ok(can_capture_with_options(
        state,
        piece,
        target,
        CaptureOptions {
            allow_basic_training_capture: allow_basic_training,
            ..CaptureOptions::default()
        },
    ))
}

pub(crate) fn v7_apply_portal_moves(
    state: &GameState,
    piece: &Piece,
    moves: Vec<MoveTarget>,
) -> Result<Vec<MoveTarget>> {
    if v7_portal_cells(state).is_none() {
        return Ok(moves);
    }
    let mut output = Vec::new();
    for mut target in moves {
        let excluded = piece.is_large()
            || matches!(piece.kind.as_str(), "football" | "wall" | "blackHole")
            || [
                "grapplePull",
                "portalThrough",
                "primeMinisterPortalEntry",
                "primeMinisterPortalSecondEntry",
                "dragonSwap",
                "switcherooMove",
                "substitutionSwap",
                "colossusBody",
                "colossusMove",
                "colossusAttack",
                "bigRookMove",
                "siegeRamMove",
                "shotgunBlast",
                "shotgunSnipe",
                "merchantBuy",
                "setLogDirection",
                "footballKick",
                "footballCornerMove",
            ]
            .iter()
            .any(|field| target.flag(field))
            || target.flags.get("castle").is_some_and(v7_truth);
        let Some(exit) = (!excluded)
            .then(|| v7_portal_exit_at(state, target.square()))
            .flatten()
        else {
            output.push(target);
            continue;
        };
        if target.flag("thiefQuietJump") && state.at(exit).is_some()
            || collapsed(state, target.square())
        {
            continue;
        }
        let entry_friendly = target.flag("madHorseCapture")
            && state.at(target.square()).is_some_and(|victim| {
                v7_mad_horse_friendly_target(state, piece, victim, target.square())
            });
        let exit_friendly = state
            .at(exit)
            .is_some_and(|victim| v7_mad_horse_friendly_target(state, piece, victim, exit));
        if !exit_friendly
            && !v7_can_portal_land_on(state, piece, exit, target.flag("basicTrainingCapture"))?
        {
            continue;
        }
        target.flags.insert(
            "madHorseCapture".into(),
            json!(entry_friendly || exit_friendly),
        );
        target
            .flags
            .insert("madHorsePortalEntryCapture".into(), json!(entry_friendly));
        target
            .flags
            .insert("madHorsePortalExitCapture".into(), json!(exit_friendly));
        target.flags.insert("portalLanding".into(), json!(true));
        target
            .flags
            .insert("portalEntry".into(), json!(target.square()));
        target.flags.insert("portalExit".into(), json!(exit));
        output.push(target);
    }
    Ok(output)
}

pub(crate) fn v7_scarecrow_reserved_square(state: &GameState, at: Square) -> bool {
    state
        .extra
        .get("pendingScarecrows")
        .and_then(Value::as_array)
        .is_some_and(|entries| {
            entries.iter().any(|entry| {
                !crate::observation::truth(entry.get("pieceId"))
                    && v7_descriptor_square(entry) == Some(at)
            })
        })
        || state
            .extra
            .get("pendingLobsters")
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries
                    .iter()
                    .any(|entry| v7_descriptor_square(entry) == Some(at))
            })
}

pub(crate) use objects::MovementObject;

/// Resolve the physical piece kind to its immutable game-owned movement rule.
pub(crate) fn movement_object_for(piece: &Piece) -> Result<&'static MovementObject> {
    objects::for_piece(piece)
}

pub(crate) fn piece_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id == RULES_VERSION_V7 {
        return v7_legal_move_targets(state, piece, from, V7MoveOptions::default());
    }
    legacy_piece_moves(state, piece, from)
}

/// Frozen `getLegalMoves` has several independent probe contexts. Keeping
/// those flags on the call, instead of inserting transient fields into the
/// Position, preserves its identity and prevents probe state leaking to IR.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct V7MoveOptions {
    pub(crate) ignore_forced_turn_move: bool,
    pub(crate) ignore_global_capture_force: bool,
    pub(crate) ignore_scarecrow_reservation: bool,
    pub(crate) ignore_scarecrow_force: bool,
    pub(crate) ignore_quantum_counterpart: bool,
    pub(crate) fog_visibility_probe: bool,
}

/// main98212의 실제 기록 순서다. source clone/shadow의 기물은 live 보드에
/// 동일 identity가 없으므로 적용되지 않는다. 공개 action/IR에 직렬화하지 않는다.
#[derive(Debug)]
pub(crate) struct V7MonoShadeWrite {
    pub(crate) piece_id: String,
    pub(crate) square: Square,
    pub(crate) shade: &'static str,
}

#[derive(Debug, Default)]
pub(crate) struct V7MoveQueryEffects {
    /// 이 조회의 applyMoveRestrictions 도달 여부다. 후보 수와 무관하다.
    pub(crate) apply_move_restrictions_reached: bool,
    /// 원문 후속 Checker/Scarecrow 조회도 실제 호출 순서대로 누적한다.
    pub(crate) mono_shades: Vec<V7MonoShadeWrite>,
}

#[derive(Debug)]
pub(crate) struct V7MoveQuery {
    pub(crate) targets: Vec<MoveTarget>,
    pub(crate) effects: V7MoveQueryEffects,
}

fn v7_apply_move_query_effects(state: &mut GameState, effects: &V7MoveQueryEffects) {
    if !effects.apply_move_restrictions_reached {
        return;
    }
    for write in &effects.mono_shades {
        let Some(mut piece) = state
            .at(write.square)
            .filter(|piece| piece.id == write.piece_id)
            .cloned()
        else {
            continue;
        };
        piece.extra.insert("monoShade".into(), json!(write.shade));
        if piece.id.is_empty() {
            // 이미 식별된 host Position은 이 경로에 들어오지 않는다. 빈 ID를
            // 공유 identity로 오인하여 관련 없는 기물을 갱신하지 않는다.
            state.board[write.square.row as usize][write.square.col as usize] = Some(piece);
        } else {
            crate::transition::update_piece(state, &piece);
        }
    }
}

/// source getLegalMoves의 한 계산에서 후보와 실제 도달 효과를 함께 얻는다.
/// 입력은 변경하지 않으며 caller가 명시적으로 live 효과를 선택한다.
pub(crate) fn v7_legal_move_query(
    state: &GameState,
    piece: &Piece,
    from: Square,
    options: V7MoveOptions,
) -> Result<V7MoveQuery> {
    v7_legal_move_query_with_population(state, piece, from, options, None)
}

/// 실제 보드의 기물 한 개를 조회하고 원문 조회 효과만 적용한다.
/// applyMoveRestrictions 이전 조기 반환 및 Football의 임시 clone은
/// physical monoShade를 쓰지 않는다. 후보를 재계산하거나 전 보드를 stamp하지 않는다.
pub(crate) fn v7_legal_move_targets_live(
    state: &mut GameState,
    from: Square,
    options: V7MoveOptions,
) -> Result<Vec<MoveTarget>> {
    v7_validate_move_query_geometry(state, from)?;
    let Some(piece) = state.at(from).cloned() else {
        return Ok(Vec::new());
    };
    let query = v7_legal_move_query(state, &piece, from, options)?;
    v7_apply_move_query_effects(state, &query.effects);
    Ok(query.targets)
}

fn v7_validate_move_query_geometry(state: &GameState, from: Square) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7
        || from.row >= 8
        || from.col >= 8
        || state.board.len() != 8
        || state.board.iter().any(|row| row.len() != 8)
    {
        return Err(EngineError::InvalidState(
            "v7 legal movement requires an 8x8 v7 Position".into(),
        ));
    }
    Ok(())
}

/// Ordered per-piece source movement. Consumers may ask about a piece whose
/// color differs from state.turn (for example Zugzwang or Double Check).
/// The actor is not rewritten here; source gates which depend on the current
/// turn, such as Idol Encore, must see the caller's original turn.
pub(crate) fn v7_legal_move_targets(
    state: &GameState,
    piece: &Piece,
    from: Square,
    options: V7MoveOptions,
) -> Result<Vec<MoveTarget>> {
    Ok(v7_legal_move_query(state, piece, from, options)?.targets)
}

fn v7_legal_move_targets_with_population(
    state: &GameState,
    piece: &Piece,
    from: Square,
    options: V7MoveOptions,
    population: Option<&GameState>,
) -> Result<Vec<MoveTarget>> {
    Ok(v7_legal_move_query_with_population(state, piece, from, options, population)?.targets)
}

fn v7_legal_move_query_with_population(
    state: &GameState,
    piece: &Piece,
    from: Square,
    options: V7MoveOptions,
    population: Option<&GameState>,
) -> Result<V7MoveQuery> {
    let mut effects = V7MoveQueryEffects::default();
    let targets =
        v7_compute_legal_move_targets(state, piece, from, options, population, &mut effects)?;
    Ok(V7MoveQuery { targets, effects })
}

fn v7_compute_legal_move_targets(
    state: &GameState,
    piece: &Piece,
    from: Square,
    options: V7MoveOptions,
    population: Option<&GameState>,
    effects: &mut V7MoveQueryEffects,
) -> Result<Vec<MoveTarget>> {
    v7_validate_move_query_geometry(state, from)?;
    let from = v7_normalize_origin(piece, from)?;
    if piece.kind == "football" && piece.color != state.turn {
        let mut football = piece.clone();
        football.color = state.turn.into();
        return v7_compute_legal_move_targets(state, &football, from, options, population, effects);
    }
    if piece.kind == "wall" || v7_piece_move_blocked(state, piece, options) {
        return Ok(Vec::new());
    }
    // Source getLegalMoves temporarily materializes every available quantum
    // shadow. The disposable board also serves capture/restriction probes.
    let materialized = if state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|item| item.extra.get("quantum").is_some_and(v7_truth))
    {
        let mut probe = state.clone();
        crate::v7_quantum_state::materialize_quantum_shadows(&mut probe)?;
        Some(probe)
    } else {
        None
    };
    let state = materialized.as_ref().unwrap_or(state);
    ensure_v7_capture_policy_scope(state)?;
    if piece.color.owner().is_some() {
        crate::card_constraints::CaptureConstraints::from_source_state(state, piece)?;
    }
    let slime_locked = piece.ability_kind() == "slime";
    let mut moves = if piece.kind == "pawn"
        && state
            .extra
            .get("effects")
            .and_then(|effects| effects.get("pawnQueen"))
            .and_then(Value::as_str)
            == piece.color.owner().map(Color::as_str)
    {
        let mut moves = rays(state, piece, from, DIAG, 7);
        moves.extend(rays(state, piece, from, ORTHO, 7));
        moves
    } else if piece.kind == "berserker" && population.is_some() {
        crate::variant_movement::v7_berserker_with_population(
            state,
            piece,
            from,
            population.expect("checked population"),
        )?
    } else if piece.kind == "princess" && population.is_some() {
        let has_queen = population.is_some_and(|original| {
            original.board.iter().flatten().flatten().any(|other| {
                other.color == piece.color && other.kind == "queen" && !other.flag("regencyHeir")
            })
        });
        if has_queen {
            v7_jump_leaps(state, piece, from, DIAG)
        } else {
            let mut moves = rays(state, piece, from, DIAG, 7);
            moves.extend(rays(state, piece, from, ORTHO, 7));
            moves
        }
    } else {
        movement_object_for(piece)?.enumerate_base(state, piece, from)?
    };
    if piece.kind == "king" {
        moves.extend(v7_standard_castling(state, piece, from, options)?);
        if state.flag("freeCastling", piece.color) {
            moves.extend(v7_free_castling(state, piece, from));
        }
        moves.extend(v7_switcheroo_moves(state, piece));
    }
    if piece.kind == "bishop" && state.flag("bishopInfiltration", piece.color) {
        for &(dr, dc) in DIAG {
            let mut cursor = from;
            while let Some(to) = cursor.offset(dr, dc) {
                cursor = to;
                if state.at(to).is_none() {
                    moves.push(MoveTarget::at(to));
                }
            }
        }
        moves = unique_v7_coordinates(moves);
    }
    // The pawn-queen source branch returns before all augment additions.
    let pawn_queen = piece.kind == "pawn"
        && state
            .extra
            .get("effects")
            .and_then(|effects| effects.get("pawnQueen"))
            .and_then(Value::as_str)
            == piece.color.owner().map(Color::as_str);
    if !pawn_queen && !slime_locked {
        if piece.flag("regencyHeir") && v7_king_augment_recipient(state, piece) {
            if state.flag("freeCastling", piece.color) {
                moves.extend(v7_free_castling(state, piece, from));
            }
            moves.extend(v7_switcheroo_moves(state, piece));
            moves = unique_v7_coordinates(moves);
        }
        if v7_king_augment_recipient(state, piece)
            && state.flag("hillKing", piece.color)
            && (3..=4).contains(&from.row)
            && (3..=4).contains(&from.col)
        {
            let deltas = crate::variant_movement::knight_deltas_for_move(state, piece, from)?;
            moves.extend(v7_jump_leaps(state, piece, from, &deltas));
            moves = unique_v7_coordinates(moves);
        }
        if v7_king_augment_recipient(state, piece)
            && piece.kind != "king"
            && state.flag("kingKnight", piece.color)
        {
            let deltas = crate::variant_movement::knight_deltas_for_move(state, piece, from)?;
            moves.extend(v7_jump_leaps(state, piece, from, &deltas));
            moves = unique_v7_coordinates(moves);
        }
        if v7_king_augment_recipient(state, piece) && state.flag("imperialStudies", piece.color) {
            moves.extend(crate::variant_movement::v7_imperial_study_moves(
                state,
                piece,
                from,
                population.unwrap_or(state),
            )?);
            moves = unique_v7_coordinates(moves);
        }
        if v7_king_augment_recipient(state, piece) && state.flag("killerKing", piece.color) {
            for &(dr, dc) in ORTHO {
                let mut cursor = from;
                while let Some(to) = cursor.offset(dr, dc) {
                    cursor = to;
                    if let Some(victim) = state.at(to) {
                        if victim.color != piece.color
                            && state.royal_identity(victim)
                            && can_capture(state, piece, victim)
                        {
                            moves.push(MoveTarget::at(to));
                        }
                        break;
                    }
                }
            }
            moves = unique_v7_coordinates(moves);
        }
        if piece
            .color
            .owner()
            .is_some_and(|actor| v7_positive_side_counter(state, "socialism", actor))
            && !state.royal_identity(piece)
            && piece.kind != "crown"
        {
            moves = if piece.is_large() {
                v7_large_modifier_moves(state, piece, from, V7LargeModifier::Socialism)?
            } else if matches!(piece.kind.as_str(), "pawn" | "squire" | "standardBearer") {
                v7_pawn_moves(state, piece, from)?
            } else {
                v7_socialist_pawn_moves(state, piece, from)
            };
            if piece.kind == "merchant" {
                let mut merchant =
                    movement_object_for(piece)?.enumerate_base(state, piece, from)?;
                merchant.extend(moves);
                moves = unique_v7_coordinates(merchant);
            }
        }
        if v7_has_basic_training(piece) {
            moves.extend(v7_basic_training_moves(state, piece, from));
            moves = unique_v7_coordinates(moves);
        }
        if v7_royal_command_capture_access(state, piece) {
            if piece.is_large() {
                moves.extend(v7_large_modifier_moves(
                    state,
                    piece,
                    from,
                    V7LargeModifier::Crown,
                )?);
            } else {
                moves.extend(v7_jump_leaps(state, piece, from, KING));
            }
            moves = unique_v7_coordinates(moves);
        }
        if v7_highway_cell(state, from)
            && !matches!(
                piece.kind.as_str(),
                "wall" | "football" | "colossus" | "bigRook" | "bigBishop"
            )
        {
            moves.extend(v7_highway_moves(state, piece, from));
            moves = unique_v7_coordinates(moves);
        }
        if piece.color.owner().is_some_and(|actor| {
            crate::observation::number(piece.extra.get("promotionRushUntil"))
                .is_some_and(|until| f64::from(*state.turns_taken.get(actor)) < until)
        }) {
            if piece.is_large() {
                moves.extend(v7_large_modifier_moves(
                    state,
                    piece,
                    from,
                    V7LargeModifier::PromotionRush,
                )?);
            } else {
                let mut rush = rays(state, piece, from, DIAG, 7);
                rush.extend(rays(state, piece, from, ORTHO, 7));
                moves.extend(
                    rush.into_iter()
                        .filter(|target| {
                            state.at(target.square()).is_none_or(|occupant| {
                                v7_stealth_transparent(state, piece, occupant, target.square())
                            })
                        })
                        .map(|mut target| {
                            target.flags.insert("promotionRushMove".into(), json!(true));
                            target
                        }),
                );
            }
            moves = unique_v7_coordinates(moves);
        }
        if state.flag("locustSwarm", piece.color)
            && matches!(piece.kind.as_str(), "knight" | "bishop" | "camel" | "rook")
            && !piece.moved
            && !piece.extra.get("locustUsed").is_some_and(v7_truth)
            && piece
                .extra
                .get("locustOrigin")
                .and_then(v7_descriptor_square)
                == Some(from)
        {
            moves.extend(v7_raw_grasshopper_moves(state, piece, from)?);
            moves = unique_v7_coordinates(moves);
        }
        let swaps = v7_substitution_moves(state, piece)?;
        if !swaps.is_empty() {
            let mut prefix = swaps;
            prefix.extend(moves);
            moves = unique_v7_coordinates(prefix);
        }
        moves = crate::v7_rule_geometry::v7_apply_population_movement_modifiers(
            state, piece, from, moves, population,
        )?;
        if piece.flag("loyalist") {
            moves.extend(v7_loyalist_moves(state, piece));
            moves = unique_v7_coordinates(moves);
        }
    }
    if pawn_queen {
        v7_filter_move_targets_with_query_effects(
            state,
            piece,
            from,
            v7_mark_crown_moves(state, piece, from, moves),
            options,
            V7MoveFilterPath::PawnQueen,
            effects,
        )
    } else {
        let moves = v7_mark_crown_moves(
            state,
            piece,
            from,
            v7_apply_portal_moves(state, piece, moves)?,
        );
        v7_restrict_and_finalize_moves(state, piece, from, moves, options, effects)
    }
}

/// Source declaration = current source moves, a board containing only the
/// selected piece, then diagonal pawn probes. The original board is kept as
/// a separate population context for Princess and future symmetry helpers.
/// No declaration probe consumes RNG or mutates the caller's Position.
pub(crate) fn v7_free_move_declaration_targets(
    state: &GameState,
    piece: &Piece,
    square: Square,
) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id != RULES_VERSION_V7
        || state.board.len() != 8
        || state.board.iter().any(|row| row.len() != 8)
        || square.row >= 8
        || square.col >= 8
    {
        return Err(EngineError::InvalidState(
            "v7 free-move declaration requires an in-bounds 8x8 Position".into(),
        ));
    }
    let Some(actor) = piece.color.owner() else {
        return Ok(Vec::new());
    };
    if matches!(
        piece.kind.as_str(),
        "wall" | "football" | "blackHole" | "coffin"
    ) {
        return Ok(Vec::new());
    }
    let origin = v7_normalize_origin(piece, square)?;
    let options = V7MoveOptions {
        ignore_scarecrow_reservation: true,
        ignore_scarecrow_force: true,
        ignore_quantum_counterpart: true,
        ..V7MoveOptions::default()
    };
    let mut current = state.clone();
    current.turn = actor;
    let mut moves = v7_legal_move_targets(&current, piece, origin, options)?;
    let mut relaxed = current.clone();
    for row in &mut relaxed.board {
        row.fill(None);
    }
    let mut simulated = piece.clone();
    simulated.extra.shift_remove("quantum");
    simulated.extra.shift_remove("quantumFirstObservationFails");
    if simulated.is_large() {
        simulated
            .extra
            .insert("anchorRow".into(), json!(origin.row));
        simulated
            .extra
            .insert("anchorCol".into(), json!(origin.col));
        for row in origin.row..origin.row + 2 {
            for col in origin.col..origin.col + 2 {
                relaxed.board[row as usize][col as usize] = Some(simulated.clone());
            }
        }
    } else {
        relaxed.board[origin.row as usize][origin.col as usize] = Some(simulated.clone());
    }
    moves.extend(v7_legal_move_targets_with_population(
        &relaxed,
        &simulated,
        origin,
        options,
        Some(state),
    )?);
    if matches!(
        piece.kind.as_str(),
        "pawn" | "squire" | "standardBearer" | "fanatic"
    ) {
        for dc in [-1, 1] {
            if let Some(to) = origin.offset(actor.pawn_dir(), dc) {
                let mut target = MoveTarget::at(to);
                target.flags.insert("freeMoveProbe".into(), json!(true));
                moves.push(target);
            }
        }
    }
    let mut seen = BTreeSet::new();
    Ok(moves
        .into_iter()
        .filter(|target| v7_free_move_executable(target, origin))
        .filter(|target| {
            let coordinate = |name: &str| {
                crate::observation::number(target.flags.get(name))
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "NaN".into())
            };
            let key = format!(
                "{}:{}:{}:{}:{}:{}:{}",
                target.row,
                target.col,
                coordinate("anchorRow"),
                coordinate("anchorCol"),
                target
                    .flags
                    .get("castle")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                if target.flag("colossusMove") { "C" } else { "" },
                if target.flag("bigRookMove") { "B" } else { "" }
            );
            seen.insert(key)
        })
        .collect())
}

fn v7_free_move_executable(target: &MoveTarget, from: Square) -> bool {
    target.square() != from
        && ![
            "colossusBody",
            "colossusAttack",
            "shotgunBlast",
            "shotgunSnipe",
            "merchantBuy",
            "setLogDirection",
            "dragonSwap",
            "substitutionSwap",
        ]
        .iter()
        .any(|field| target.flag(field))
}

fn legacy_piece_moves(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    if frozen(piece)
        || crate::observation::number(
            piece
                .extra
                .get("staked")
                .and_then(|entry| entry.get("remaining")),
        )
        .unwrap_or(0.0)
            > 0.0
        || crate::observation::number(piece.extra.get("poisonStunTurns"))
            .unwrap_or(0.0)
            .floor()
            > 0.0
    {
        return Ok(Vec::new());
    }
    ensure_v7_capture_policy_scope(state)?;
    let mut moves = movement_object_for(piece)?.enumerate_base(state, piece, from)?;
    if (if state.ruleset_id == RULES_VERSION_V7 {
        piece
            .color
            .owner()
            .is_some_and(|actor| v7_positive_side_counter(state, "socialism", actor))
    } else {
        state.flag("socialism", piece.color)
    }) && !piece.is_royal()
        && !matches!(
            piece.kind.as_str(),
            "wall" | "coffin" | "scarecrow" | "bigRook" | "bigBishop"
        )
    {
        moves = pawn_moves(state, piece, from)
            .into_iter()
            .filter(|target| !target.flag("standardPawnDoubleStep"))
            .collect();
    }
    if piece.flag("basicTraining")
        && !piece.is_large()
        && !matches!(piece.kind.as_str(), "wall" | "football" | "blackHole")
        && piece.ability_kind() != "slime"
        && (piece.flag("potionBasicTraining")
            || !matches!(
                piece.kind.as_str(),
                "pawn"
                    | "king"
                    | "queen"
                    | "primeMinister"
                    | "jester"
                    | "guard"
                    | "amazon"
                    | "man"
                    | "idol"
                    | "babyBear"
                    | "bear"
            ))
        && let Some(actor) = piece.color.owner()
    {
        for direction in [actor.pawn_dir(), -actor.pawn_dir()] {
            if direction != actor.pawn_dir() && !state.flag("retreat", actor) {
                continue;
            }
            if let Some(to) = from.offset(direction, 0)
                && state.at(to).is_none()
            {
                moves.push(MoveTarget::at(to));
            }
            if piece.ability_kind() != "missionary" {
                for dc in [-1, 1] {
                    if let Some(to) = from.offset(direction, dc)
                        && state.at(to).is_some_and(|victim| {
                            can_capture_with_options(
                                state,
                                piece,
                                victim,
                                CaptureOptions {
                                    allow_basic_training_capture: true,
                                    ..CaptureOptions::default()
                                },
                            )
                        })
                    {
                        let mut target = MoveTarget::at(to);
                        target
                            .flags
                            .insert("basicTrainingCapture".into(), json!(true));
                        moves.push(target);
                    }
                }
            }
        }
    }
    if piece.flag("loyalist") && piece.ability_kind() != "slime" {
        for row in 0..8 {
            for col in 0..8 {
                let anchor = Square { row, col };
                if state.at(anchor).is_some_and(|candidate| {
                    candidate.color == piece.color
                        && (state.royal_identity(candidate) || candidate.kind == "merchant")
                }) {
                    for &(dr, dc) in KING {
                        if let Some(to) = anchor.offset(dr, dc)
                            && open_relocation(state, to)?
                        {
                            let mut target = MoveTarget::at(to);
                            target.flags.insert("loyalistMove".into(), json!(true));
                            moves.push(target);
                        }
                    }
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut allowed = Vec::new();
    for target in moves {
        let stationary = [
            "colossusBody",
            "colossusAttack",
            "shotgunBlast",
            "shotgunSnipe",
            "merchantBuy",
            "setLogDirection",
        ]
        .iter()
        .any(|field| target.flag(field));
        if crate::observation::truth(piece.extra.get("grapplerBound"))
            && !stationary
            && target.square() != from
        {
            continue;
        }
        let severed = crate::observation::number(
            piece
                .extra
                .get("severed")
                .and_then(|entry| entry.get("remaining")),
        )
        .unwrap_or(0.0)
            > 0.0;
        let exempt = target.flag("castle") || target.flag("colossusMove") || stationary;
        if severed && !exempt {
            let memory = crate::card_effects::current_base_movement(state, piece);
            let kind = memory
                .as_ref()
                .and_then(|memory| memory.get("type"))
                .and_then(Value::as_str)
                .unwrap_or(piece.kind.as_str());
            let dr = from.row.abs_diff(target.row);
            let dc = from.col.abs_diff(target.col);
            let distance = if matches!(kind, "hook" | "brutus") {
                dr + dc
            } else {
                dr.max(dc)
            };
            if distance > 1 {
                continue;
            }
        }
        if piece.flag("inertia")
            && !exempt
            && !(matches!(piece.kind.as_str(), "hook" | "brutus")
                && (target.flag("bent") || target.flag("portalThrough")))
            && from
                .row
                .abs_diff(target.row)
                .max(from.col.abs_diff(target.col))
                == 1
        {
            continue;
        }
        if expansion_move_allowed(state, piece, from, &target)?
            && fianchetto_move_allowed(state, piece, from, &target)?
            && seen.insert(serde_json::to_string(&target).expect("move serializes"))
        {
            allowed.push(target);
        }
    }
    let moves = allowed;
    Ok(moves)
}

fn unique_v7_coordinates(moves: Vec<MoveTarget>) -> Vec<MoveTarget> {
    let mut seen = BTreeSet::new();
    moves
        .into_iter()
        .filter(|target| seen.insert(target.square()))
        .collect()
}

pub(crate) fn v7_king_augment_recipient(state: &GameState, piece: &Piece) -> bool {
    piece.is_royal()
        || piece.flag("regencyHeir")
            && state.flag("kingDead", piece.color)
            && state.flag("regency", piece.color)
}

fn v7_forced_piece_id(state: &GameState, actor: Color) -> Option<&str> {
    let pieces = || {
        state
            .board
            .iter()
            .flatten()
            .flatten()
            .filter(|piece| piece.color == actor)
    };
    pieces()
        .find(|piece| {
            piece
                .extra
                .get("repositionSecondMove")
                .is_some_and(|window| {
                    crate::observation::truth(window.get("used"))
                        || crate::observation::truth(window.get("forced"))
                })
        })
        .or_else(|| {
            pieces().find(|piece| {
                [
                    "thiefSecondMove",
                    "frenzyExtraMove",
                    "fileSurgeSecondMove",
                    "rookLiftSecondMove",
                    "ironMonarchExtraMove",
                    "underpromotionSecondMove",
                    "checkerChainCapture",
                    "madHorseSecondMove",
                    "platformExtraMove",
                    "desperado",
                ]
                .iter()
                .any(|field| crate::observation::truth(piece.extra.get(*field)))
            })
        })
        .map(|piece| piece.id.as_str())
}

fn v7_piece_move_blocked(state: &GameState, piece: &Piece, options: V7MoveOptions) -> bool {
    let Some(actor) = piece.color.owner() else {
        return piece.kind != "football";
    };
    let counter =
        |field: &str| crate::card_effects::js_number(piece.extra.get(field), 0).unwrap_or(0.0);
    let current_turn = f64::from(*state.turns_taken.get(actor));
    if piece.ability_kind() == "slime" && v7_positive_side_counter(state, "socialism", actor) {
        return true;
    }
    if state.turn == actor
        && crate::card_effects::js_number(piece.extra.get("idolEncoreRestTurn"), 0)
            .is_some_and(|rest| rest.is_finite() && rest == current_turn)
    {
        return true;
    }
    // Source clears a stale Democracy/Zugzwang lock when no royal identity
    // remains. The pure kernel computes that effective lock without mutating.
    let zugzwang = state.flag("zugzwang", actor)
        && (!state.flag("democracy", actor)
            || state
                .board
                .iter()
                .flatten()
                .flatten()
                .any(|other| other.color == actor && state.royal_identity(other)));
    if !options.ignore_forced_turn_move
        && (zugzwang && !state.royal_identity(piece)
            || v7_forced_piece_id(state, actor).is_some_and(|id| id != piece.id))
    {
        return true;
    }
    if frozen(piece)
        || counter("poisonStunTurns").floor() > 0.0
        || crate::card_effects::js_number(
            piece
                .extra
                .get("staked")
                .and_then(|window| window.get("remaining")),
            0,
        )
        .unwrap_or(0.0)
            > 0.0
        || (matches!(piece.kind.as_str(), "bear" | "hedgehog")
            || piece.ability_kind() == "hedgehog")
            && current_turn < counter("bearMoveLockedUntilTurn")
    {
        return true;
    }
    if !crate::observation::truth(piece.extra.get("regencyHeir"))
        && !crate::observation::truth(piece.extra.get("crownRoyal"))
        && !matches!(
            piece.kind.as_str(),
            "king" | "royalKnight" | "shotgunKing" | "darkWizard"
        )
        && let Some(exhaustion) = state
            .extra
            .get("exhaustion")
            .and_then(|entry| entry.get(actor.as_str()))
        && crate::observation::truth(exhaustion.get("enabled"))
        && !piece.id.is_empty()
        && exhaustion.get("pieceId").and_then(Value::as_str) == Some(piece.id.as_str())
        && crate::card_effects::js_number(exhaustion.get("count"), 0)
            .unwrap_or(0.0)
            .max(0.0)
            .floor()
            >= 3.0
    {
        return true;
    }
    if piece.ability_kind() == "clockwork"
        && !KING
            .iter()
            .filter_map(|&(dr, dc)| find_square(state, &piece.id)?.offset(dr, dc))
            .any(|square| {
                state
                    .at(square)
                    .is_some_and(|other| other.color == piece.color)
            })
    {
        return true;
    }
    let socialism = v7_positive_side_counter(state, "socialism", actor);
    if (piece.kind == "medium"
        && !crate::observation::truth(
            state
                .extra
                .get("mediumMovement")
                .and_then(|memory| memory.get("type")),
        )
        || piece.kind == "babyBear")
        && !socialism
    {
        return true;
    }
    if piece.kind != "football" {
        if v7_dice_locked(state, piece) {
            return true;
        }
        if v7_king_augment_recipient(state, piece)
            && crate::observation::truth(piece.extra.get("undergroundBunker"))
            && crate::observation::number(piece.extra.get("hp")).is_some_and(f64::is_finite)
        {
            return true;
        }
    }
    false
}

pub(crate) fn v7_dice_locked(state: &GameState, piece: &Piece) -> bool {
    let Some(lock) = state
        .extra
        .get("diceLocks")
        .and_then(|locks| locks.get(piece.color.as_str()))
        .filter(|lock| v7_truth(lock))
    else {
        return false;
    };
    if crate::card_effects::js_number(lock.get("remaining"), 0)
        .is_some_and(|remaining| remaining <= 0.0)
    {
        return false;
    }
    lock.get("type").and_then(Value::as_str) == Some(piece.kind.as_str())
        || lock.get("type").and_then(Value::as_str) == Some("king")
            && v7_king_augment_recipient(state, piece)
}

fn v7_has_basic_training(piece: &Piece) -> bool {
    piece.flag("basicTraining")
        && !piece.is_large()
        && !matches!(piece.kind.as_str(), "wall" | "football" | "blackHole")
        && (piece.flag("potionBasicTraining")
            || !matches!(
                piece.kind.as_str(),
                "pawn"
                    | "king"
                    | "queen"
                    | "primeMinister"
                    | "jester"
                    | "guard"
                    | "amazon"
                    | "man"
                    | "idol"
                    | "babyBear"
                    | "bear"
            ))
}
pub(crate) fn v7_can_substitute_pieces(
    _state: &GameState,
    moving: &Piece,
    target: &Piece,
    enabled: bool,
) -> bool {
    enabled
        && moving.id != target.id
        && moving.color.owner().is_some()
        && target.color.owner().is_some()
        && moving.color != target.color
        && moving.kind == target.kind
        && moving.ability_kind() != "slime"
        && target.ability_kind() != "slime"
        && !matches!(
            moving.kind.as_str(),
            "wall" | "football" | "monster" | "blackHole"
        )
}
fn v7_substitution_moves(state: &GameState, piece: &Piece) -> Result<Vec<MoveTarget>> {
    let enabled = state
        .extra
        .get("substitution")
        .and_then(|sides| sides.get(piece.color.as_str()))
        .is_some_and(v7_truth);
    if !enabled {
        return Ok(Vec::new());
    }
    let mut seen = BTreeSet::new();
    let mut moves = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            let Some(target) = state.at(at) else {
                continue;
            };
            if !v7_can_substitute_pieces(state, piece, target, enabled)
                || !seen.insert(target.id.clone())
            {
                continue;
            }
            let to = v7_normalize_origin(target, at)?;
            let mut target = MoveTarget::at(to);
            target.flags.insert("substitutionSwap".into(), json!(true));
            moves.push(target);
        }
    }
    Ok(moves)
}

pub(crate) fn v7_raw_bishop_snipe_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    ensure_v7_capture_policy_scope(state)?;
    let directions = if piece.kind == "bishop" && state.flag("reversal", piece.color) {
        ORTHO
    } else {
        DIAG
    };
    let nominal = Piece::new("bishop", piece.color, "v7-source-bishop-capture-predicate");
    let mut moves = Vec::new();
    for &(dr, dc) in directions {
        let mut cursor = from;
        let mut screens = 0;
        while let Some(to) = cursor.offset(dr, dc) {
            cursor = to;
            let Some(victim) = state.at(to) else {
                continue;
            };
            if v7_stealth_transparent(state, piece, victim, to)
                || victim.color == piece.color && victim.flag("ghost")
            {
                continue;
            }
            if screens == 1 && can_capture(state, &nominal, victim) {
                let mut target = MoveTarget::at(to);
                target.flags.insert("bishopSnipe".into(), json!(true));
                moves.push(target);
            }
            screens += 1;
            if screens > 1 {
                break;
            }
        }
    }
    Ok(unique_v7_coordinates(moves))
}

fn v7_basic_training_moves(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let Some(actor) = piece.color.owner() else {
        return Vec::new();
    };
    let mut moves = Vec::new();
    for direction in [actor.pawn_dir(), -actor.pawn_dir()] {
        if direction != actor.pawn_dir() && !state.flag("retreat", actor) {
            continue;
        }
        if let Some(to) = from.offset(direction, 0)
            && state
                .at(to)
                .is_none_or(|target| v7_stealth_transparent(state, piece, target, to))
        {
            moves.push(MoveTarget::at(to));
        }
        if piece.ability_kind() == "missionary" {
            continue;
        }
        for dc in [-1, 1] {
            if let Some(to) = from.offset(direction, dc)
                && state.at(to).is_some_and(|victim| {
                    !v7_stealth_transparent(state, piece, victim, to)
                        && can_capture_with_options(
                            state,
                            piece,
                            victim,
                            CaptureOptions {
                                allow_basic_training_capture: true,
                                ..CaptureOptions::default()
                            },
                        )
                })
            {
                let mut target = MoveTarget::at(to);
                target
                    .flags
                    .insert("basicTrainingCapture".into(), json!(true));
                moves.push(target);
            }
        }
    }
    moves
}

fn v7_socialist_pawn_moves(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let Some(actor) = piece.color.owner() else {
        return Vec::new();
    };
    let direction = actor.pawn_dir();
    let mut moves = Vec::new();
    if let Some(to) = from.offset(direction, 0)
        && state
            .at(to)
            .is_none_or(|target| v7_stealth_transparent(state, piece, target, to))
    {
        moves.push(MoveTarget::at(to));
    }
    for dc in [-1, 1] {
        if let Some(to) = from.offset(direction, dc)
            && state.at(to).is_some_and(|target| {
                !v7_stealth_transparent(state, piece, target, to)
                    && can_capture(state, piece, target)
            })
        {
            moves.push(MoveTarget::at(to));
        }
    }
    moves
}

fn v7_highway_cell(state: &GameState, square: Square) -> bool {
    crate::observation::truth(state.extra.get("highway")) && (square.col == 1 || square.col == 6)
        || state
            .extra
            .get("highwayCells")
            .and_then(Value::as_array)
            .is_some_and(|cells| {
                cells
                    .iter()
                    .any(|cell| v7_descriptor_square(cell) == Some(square))
            })
}

pub(crate) fn v7_normalized_highway_cells(state: &GameState) -> Result<Vec<Square>> {
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::InvalidState(
            "v7 highway query requires an 8x8 board".into(),
        ));
    }
    Ok(state
        .extra
        .get("highwayCells")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(v7_descriptor_square)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect())
}

pub(crate) fn v7_roller_travel_cells(
    from: Square,
    to: Square,
    target: &MoveTarget,
) -> Result<Vec<Square>> {
    if [from, to].iter().any(|s| s.row >= 8 || s.col >= 8) {
        return Err(EngineError::InvalidState(
            "v7 Roller travel coordinate outside 8x8".into(),
        ));
    }
    if from == to {
        return Ok(Vec::new());
    }
    let segment = |a: Square, b: Square, teleport: bool| {
        let mut cells = vec![a];
        if teleport || a.row != b.row && a.col != b.col {
            cells.push(b);
            return cells;
        }
        let dr = (i16::from(b.row) - i16::from(a.row)).signum() as i8;
        let dc = (i16::from(b.col) - i16::from(a.col)).signum() as i8;
        let mut cursor = a;
        for _ in 0..7 {
            if cursor == b {
                break;
            }
            let Some(at) = cursor.offset(dr, dc) else {
                break;
            };
            cells.push(at);
            cursor = at;
        }
        cells
    };
    let cells = if target.flags.get("portalEntry").is_some_and(v7_truth)
        && target.flags.get("portalExit").is_some_and(v7_truth)
    {
        let entry = target
            .flags
            .get("portalEntry")
            .and_then(v7_descriptor_square)
            .ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 Roller portalEntry must be a bounded integer coordinate".into(),
                )
            })?;
        let exit = target
            .flags
            .get("portalExit")
            .and_then(v7_descriptor_square)
            .ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 Roller portalExit must be a bounded integer coordinate".into(),
                )
            })?;
        let mut cells = segment(from, entry, false);
        cells.extend(segment(exit, to, false));
        cells
    } else {
        segment(
            from,
            to,
            [
                "symmetryMove",
                "relaySwap",
                "substitutionSwap",
                "switcherooMove",
                "switcheroo",
                "dragonSwap",
            ]
            .iter()
            .any(|f| target.flag(f)),
        )
    };
    Ok(cells
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn v7_highway_moves(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(dr, dc) in ORTHO {
        let mut current = from;
        while let Some(to) = current
            .offset(dr, dc)
            .filter(|&cell| v7_highway_cell(state, cell))
        {
            current = to;
            if let Some(occupant) = state.at(to) {
                if occupant.color == piece.color && occupant.flag("ghost")
                    || v7_time_traveler_campaign(state)
                        && !v7_time_phase_interacts(state, piece, occupant)
                {
                    continue;
                }
                if !matches!(
                    piece.kind.as_str(),
                    "guard"
                        | "recruiter"
                        | "wizard"
                        | "herald"
                        | "merchant"
                        | "coffin"
                        | "scarecrow"
                        | "blackHole"
                        | "timeAfterimage"
                ) && (piece.kind != "jester"
                    || matches!(
                        occupant.kind.as_str(),
                        "king" | "royalKnight" | "shotgunKing" | "merchant"
                    ))
                    && can_capture(state, piece, occupant)
                {
                    let mut target = MoveTarget::at(to);
                    target.flags.insert("highwayMove".into(), json!(true));
                    moves.push(target);
                }
                break;
            }
            let mut target = MoveTarget::at(to);
            target.flags.insert("highwayMove".into(), json!(true));
            moves.push(target);
        }
    }
    moves
}

fn v7_loyalist_moves(state: &GameState, piece: &Piece) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let origin = Square { row, col };
            if state.at(origin).is_some_and(|royal| {
                royal.color == piece.color
                    && (state.royal_identity(royal) || royal.kind == "merchant")
            }) {
                for &(dr, dc) in KING {
                    if let Some(to) = origin
                        .offset(dr, dc)
                        .filter(|&cell| state.at(cell).is_none() && !collapsed(state, cell))
                    {
                        let mut target = MoveTarget::at(to);
                        target.flags.insert("loyalistMove".into(), json!(true));
                        moves.push(target);
                    }
                }
            }
        }
    }
    unique_v7_coordinates(moves)
}

fn v7_restrict_and_finalize_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    moves: Vec<MoveTarget>,
    options: V7MoveOptions,
    effects: &mut V7MoveQueryEffects,
) -> Result<Vec<MoveTarget>> {
    v7_filter_move_targets_with_query_effects(
        state,
        piece,
        from,
        moves,
        options,
        V7MoveFilterPath::Legal,
        effects,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum V7MoveFilterPath {
    Legal,
    BabyAutomatic,
    Brutus,
    PawnQueen,
    CheckerCaptureProbe,
}

fn v7_terrain_move_allowed(state: &GameState, target: &MoveTarget) -> Result<bool> {
    if target.flag("crownGroundCapture")
        && v7_portal_capture_cells(target)
            .last()
            .copied()
            .and_then(|at| state.at(at))
            .is_some_and(|occupant| {
                occupant.color.owner().is_none()
                    || matches!(
                        occupant.kind.as_str(),
                        "wall" | "football" | "blackHole" | "monster"
                    )
            })
    {
        return Ok(false);
    }
    if target.flag("colossusBody") || target.flag("setLogDirection") || target.flag("castle") {
        return Ok(true);
    }
    if target.flag("portalLanding") || target.flag("portalThrough") {
        let mut cells = v7_portal_capture_cells(target);
        cells.extend(
            ["portalEntry", "portalExit"]
                .into_iter()
                .filter_map(|field| target.flags.get(field).and_then(v7_descriptor_square)),
        );
        return Ok(cells.into_iter().all(|at| !collapsed(state, at)));
    }
    if let Some(value) = target
        .flags
        .get("highlightCells")
        .filter(|value| v7_truth(value))
    {
        let entries = value.as_array().ok_or_else(|| {
            EngineError::InvalidState("v7 terrain highlightCells must be an array".into())
        })?;
        for entry in entries {
            let at = v7_descriptor_square(entry).ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 terrain highlight cell must be a bounded integer coordinate".into(),
                )
            })?;
            if collapsed(state, at) {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    if target.flags.get("sectorCells").is_some_and(v7_truth) {
        return Ok(true);
    }
    Ok(!collapsed(state, target.square()))
}

fn v7_filter_move_targets(
    state: &GameState,
    piece: &Piece,
    from: Square,
    moves: Vec<MoveTarget>,
    options: V7MoveOptions,
    path: V7MoveFilterPath,
) -> Result<Vec<MoveTarget>> {
    v7_filter_move_targets_with_query_effects(
        state,
        piece,
        from,
        moves,
        options,
        path,
        &mut V7MoveQueryEffects::default(),
    )
}

fn v7_filter_move_targets_with_query_effects(
    state: &GameState,
    piece: &Piece,
    from: Square,
    moves: Vec<MoveTarget>,
    options: V7MoveOptions,
    path: V7MoveFilterPath,
    effects: &mut V7MoveQueryEffects,
) -> Result<Vec<MoveTarget>> {
    let finalize = matches!(
        path,
        V7MoveFilterPath::Legal | V7MoveFilterPath::Brutus | V7MoveFilterPath::PawnQueen
    );
    let forced_en_passant = !options.ignore_global_capture_force
        && piece.color.owner().is_some()
        && crate::observation::truth(state.extra.get("machoChess"))
        && v7_has_forced_en_passant(state, piece.color.owner().ok_or(EngineError::WrongActor)?)?;
    let mut filtered = Vec::new();
    for target in moves {
        let special = [
            "colossusAttack",
            "shotgunBlast",
            "shotgunSnipe",
            "merchantBuy",
            "setLogDirection",
        ]
        .iter()
        .any(|field| target.flag(field));
        let stationary = special || target.flag("colossusBody");
        if path == V7MoveFilterPath::Legal
            && piece.extra.get("rookLiftSecondMove").is_some_and(v7_truth)
            && piece
                .extra
                .get("rookLiftChain")
                .and_then(|chain| {
                    let row = chain.get("blockedCornerRow")?.as_f64()?;
                    let col = chain.get("blockedCornerCol")?.as_f64()?;
                    (row.fract() == 0.0 && col.fract() == 0.0).then_some((row, col))
                })
                .is_some_and(|(row, col)| {
                    let destination = v7_portal_capture_cells(&target)
                        .last()
                        .copied()
                        .unwrap_or(target.square());
                    f64::from(destination.row) == row && f64::from(destination.col) == col
                })
        {
            continue;
        }
        let capture = v7_is_capture_move(state, piece, &target)?;
        let protection_exempt = special
            || ["dragonSwap", "substitutionSwap", "relaySwap"]
                .iter()
                .any(|field| target.flag(field));
        if path == V7MoveFilterPath::Legal
            && !protection_exempt
            && v7_portal_capture_cells(&target).iter().any(|&at| {
                state.at(at).is_some_and(|victim| {
                    victim.color != piece.color
                        && victim.kind != "scarecrow"
                        && (frozen(victim)
                            || v7_encouraged_at(state, victim, at)
                            || crate::observation::truth(victim.extra.get("protected"))
                            || crate::card_effects::js_number(
                                victim
                                    .extra
                                    .get("vigilanceProtection")
                                    .and_then(|window| window.get("remaining")),
                                0,
                            )
                            .unwrap_or(0.0)
                                > 0.0)
                })
            })
        {
            continue;
        }
        if matches!(
            path,
            V7MoveFilterPath::Legal
                | V7MoveFilterPath::Brutus
                | V7MoveFilterPath::CheckerCaptureProbe
        ) && capture
            && v7_disarm_capture_locked(state, piece)
        {
            continue;
        }
        if crate::observation::truth(piece.extra.get("grapplerBound"))
            && !stationary
            && target.square() != from
        {
            continue;
        }
        if [
            "relaySwap",
            "substitutionSwap",
            "dragonSwap",
            "solidaritySwap",
        ]
        .iter()
        .any(|f| target.flag(f))
            && state
                .at(target.square())
                .is_some_and(|p| crate::observation::truth(p.extra.get("grapplerBound")))
        {
            continue;
        }
        if !expansion_move_allowed(state, piece, from, &target)? {
            continue;
        }
        if piece.ability_kind() == "thief" && crate::v7_move_execution::uses_thief_remake_v7(state)
        {
            let entry = target
                .flags
                .get("portalEntry")
                .and_then(v7_descriptor_square)
                .unwrap_or(target.square());
            let direction = format!(
                "{},{}",
                (i16::from(from.row) - i16::from(entry.row)).signum(),
                (i16::from(from.col) - i16::from(entry.col)).signum()
            );
            if piece
                .extra
                .get("thiefLastDirection")
                .and_then(Value::as_str)
                == Some(direction.as_str())
                || [
                    Some(target.square()),
                    target
                        .flags
                        .get("portalEntry")
                        .and_then(v7_descriptor_square),
                    target
                        .flags
                        .get("portalExit")
                        .and_then(v7_descriptor_square),
                ]
                .into_iter()
                .flatten()
                .any(|at| {
                    piece
                        .extra
                        .get("thiefVisited")
                        .and_then(Value::as_array)
                        .is_some_and(|visited| {
                            visited.iter().any(|cell| {
                                cell.as_str() == Some(format!("{},{}", at.row, at.col).as_str())
                            })
                        })
                })
            {
                continue;
            }
        }
        if !crate::v7_rule_geometry::v7_three_move_allowed(state, piece, from, &target)? {
            continue;
        }
        let exempt = [
            "castle",
            "colossusAttack",
            "colossusMove",
            "shotgunBlast",
            "shotgunSnipe",
            "setLogDirection",
        ]
        .iter()
        .any(|field| target.flag(field));
        if !v7_terrain_move_allowed(state, &target)? {
            continue;
        }
        if !crate::v7_rule_geometry::v7_high_ground_capture_allowed(state, piece, from, &target)? {
            continue;
        }
        if v7_time_traveler_campaign(state)
            && !target.flag("substitutionSwap")
            && !target.flag("relaySwap")
            && !target.flag("colossusBody")
            && !target.flag("setLogDirection")
            && !target.flags.get("castle").is_some_and(v7_truth)
        {
            let mut phase_blocked = false;
            for at in v7_portal_capture_cells(&target) {
                if let Some(victim) = state.at(at)
                    && (!v7_time_phase_interacts(state, piece, victim)
                        || !v7_can_capture_target(
                            state,
                            piece,
                            victim,
                            false,
                            target.flag("basicTrainingCapture"),
                        )?)
                {
                    phase_blocked = true;
                    break;
                }
            }
            if phase_blocked {
                continue;
            }
        }
        if state.royal_identity(piece)
            && !target.flag("shotgunBlast")
            && !target.flag("shotgunSnipe")
            && let Some(palace) = state
                .extra
                .get("palaces")
                .and_then(Value::as_array)
                .and_then(|palaces| {
                    palaces.iter().find(|palace| {
                        palace.get("color").and_then(Value::as_str) == Some(piece.color.as_str())
                    })
                })
            && let Some(cells) = palace.get("cells").and_then(Value::as_array)
            && cells
                .iter()
                .any(|cell| v7_descriptor_square(cell) == Some(from))
            && !cells.iter().any(|cell| {
                v7_descriptor_square(cell) == v7_portal_capture_cells(&target).last().copied()
            })
        {
            continue;
        }
        if piece.kind == "darkWizard" && piece.extra.get("darkMagicCircle").is_some_and(v7_truth) {
            let center = piece.extra.get("darkMagicCircle");
            let integer = |field| {
                center
                    .and_then(|center| center.get(field))
                    .and_then(Value::as_f64)
                    .filter(|value| value.fract() == 0.0)
            };
            let row = integer("centerRow").unwrap_or(f64::from(from.row));
            let col = integer("centerCol").unwrap_or(f64::from(from.col));
            let mut destinations = vec![
                target
                    .flags
                    .get("portalExit")
                    .filter(|_| target.flag("portalLanding"))
                    .and_then(v7_descriptor_square)
                    .unwrap_or(target.square()),
            ];
            destinations.extend(
                ["portalEntry", "portalExit"]
                    .iter()
                    .filter_map(|field| target.flags.get(*field).and_then(v7_descriptor_square)),
            );
            if destinations.iter().any(|cell| {
                (f64::from(cell.row) - row).abs() > 1.0 || (f64::from(cell.col) - col).abs() > 1.0
            }) {
                continue;
            }
        }
        if crate::observation::truth(state.extra.get("machoChess")) && piece.kind != "football" {
            if forced_en_passant && !target.flag("enPassant") {
                continue;
            }
            let direction = piece.color.owner().map(Color::pawn_dir).unwrap_or(0);
            let destination = v7_portal_capture_cells(&target)
                .last()
                .copied()
                .unwrap_or(target.square());
            let dr = i16::from(destination.row) - i16::from(from.row);
            if !forced_en_passant
                && !special
                && (dr * i16::from(direction) < 0
                    || dr == 0 && destination.col != from.col && !capture)
            {
                continue;
            }
        }
        if piece.kind == "pawn"
            && crate::observation::number(
                state
                    .extra
                    .get("effects")
                    .and_then(|effects| effects.get("pawnReverse"))
                    .and_then(|sides| sides.get(piece.color.as_str())),
            )
            .unwrap_or(0.0)
                > 0.0
            && (i16::from(
                v7_portal_capture_cells(&target)
                    .last()
                    .copied()
                    .unwrap_or(target.square())
                    .row,
            ) - i16::from(from.row))
                * i16::from(piece.color.owner().map(Color::pawn_dir).unwrap_or(0))
                > 0
        {
            continue;
        }
        if piece.kind != "football"
            && crate::card_effects::js_number(
                state
                    .extra
                    .get("taunt")
                    .and_then(|sides| sides.get(piece.color.as_str())),
                0,
            )
            .unwrap_or(0.0)
                > 0.0
            && !stationary
            && (i16::from(
                v7_portal_capture_cells(&target)
                    .last()
                    .copied()
                    .unwrap_or(target.square())
                    .row,
            ) - i16::from(from.row))
                * i16::from(piece.color.owner().map(Color::pawn_dir).unwrap_or(0))
                < 0
        {
            continue;
        }
        let destination = v7_portal_capture_cells(&target)
            .last()
            .copied()
            .unwrap_or(target.square());
        if crate::v7_campaign::is_blood_curse_active(state, piece)?
            && from
                .row
                .abs_diff(destination.row)
                .max(from.col.abs_diff(destination.col))
                > 1
        {
            continue;
        }
        if crate::observation::number(
            piece
                .extra
                .get("severed")
                .and_then(|window| window.get("remaining")),
        )
        .unwrap_or(0.0)
            > 0.0
            && !exempt
        {
            let memory = crate::card_effects::current_base_movement(state, piece);
            let kind = memory
                .as_ref()
                .and_then(|entry| entry.get("type"))
                .and_then(Value::as_str)
                .unwrap_or(piece.kind.as_str());
            let dr = from.row.abs_diff(destination.row);
            let dc = from.col.abs_diff(destination.col);
            if (if matches!(kind, "hook" | "brutus") {
                dr + dc
            } else {
                dr.max(dc)
            }) > 1
            {
                continue;
            }
        }
        if crate::observation::truth(piece.extra.get("inertia"))
            && ![
                "colossusBody",
                "castle",
                "colossusAttack",
                "shotgunBlast",
                "shotgunSnipe",
                "setLogDirection",
            ]
            .iter()
            .any(|field| target.flag(field))
            && !(matches!(piece.kind.as_str(), "hook" | "brutus")
                && (target.flags.get("bent") == Some(&json!(true))
                    || target.flags.get("portalThrough") == Some(&json!(true))))
        {
            let anchor = target
                .flags
                .get("anchorRow")
                .and_then(Value::as_f64)
                .zip(target.flags.get("anchorCol").and_then(Value::as_f64))
                .filter(|(row, col)| row.fract() == 0.0 && col.fract() == 0.0);
            let (row, col) = anchor.unwrap_or((f64::from(target.row), f64::from(target.col)));
            if (row - f64::from(from.row))
                .abs()
                .max((col - f64::from(from.col)).abs())
                == 1.0
            {
                continue;
            }
        }
        if matches!(piece.kind.as_str(), "bishop" | "rook" | "queen")
            && !target.flag("substitutionSwap")
        {
            let mut blocked = false;
            for at in v7_portal_capture_cells(&target) {
                if let Some(victim) = state.at(at)
                    && victim.color != piece.color
                    && from.row.abs_diff(at.row).max(from.col.abs_diff(at.col)) >= 2
                    && crate::v7_campaign::is_blood_veil_active(state, victim)?
                {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                continue;
            }
        }
        let destination = v7_portal_capture_cells(&target)
            .last()
            .copied()
            .unwrap_or(target.square());
        if crate::observation::truth(state.extra.get("monochromeChess"))
            && !special
            && !target.flag("castle")
            && (from.row + from.col) % 2 != (destination.row + destination.col) % 2
        {
            continue;
        }
        filtered.push(target);
    }
    // 원문 applyMoveRestrictions(main98132)는 필터 결과가 비어도 return하지
    // 않고 main98212를 실행한다. 이 marker는 같은 실제 계산의 prefix에 두며
    // 이후 Checker/Scarecrow가 전체 후보를 취소해도 지우지 않는다.
    effects.apply_move_restrictions_reached = true;
    if crate::observation::truth(state.extra.get("monochromeChess"))
        && piece.kind != "wall"
        && piece.kind != "football"
    {
        effects.mono_shades.push(V7MonoShadeWrite {
            piece_id: piece.id.clone(),
            square: from,
            shade: if (from.row + from.col).is_multiple_of(2) {
                "light"
            } else {
                "dark"
            },
        });
    }
    // 원문 집합 필터의 순서를 보존한다. 마지막 빙판 칸이 이후 Majesty로
    // 거절되어도 같은 방향의 가까운 후보를 새로 허용하지 않는다.
    if crate::card_effects::js_number(
        piece
            .extra
            .get("iceSheet")
            .and_then(|ice| ice.get("remaining")),
        0,
    )
    .unwrap_or(0.0)
        > 0.0
    {
        filtered =
            crate::variant_movement::v7_filter_ice_sheet_moves(state, piece, from, filtered)?;
    }
    let mut geometry_filtered = Vec::new();
    for target in filtered {
        if crate::v7_rule_geometry::v7_majesty_move_allowed(state, piece, from, &target)?
            && fianchetto_move_allowed(state, piece, from, &target)?
            && crate::v7_rule_geometry::v7_chain_move_allowed(state, piece, from, &target)?
        {
            geometry_filtered.push(target);
        }
    }
    let mut filtered = geometry_filtered;
    let own_checker_forced = matches!(piece.kind.as_str(), "checker" | "checkerKing")
        && (piece.flag("checkerChainCapture")
            || filtered.iter().any(|target| target.flag("checkerCapture")));
    if path == V7MoveFilterPath::Legal
        && !forced_en_passant
        && matches!(piece.kind.as_str(), "checker" | "checkerKing")
    {
        let has_captures = filtered.iter().any(|target| target.flag("checkerCapture"));
        if piece.flag("checkerChainCapture") || has_captures {
            filtered.retain(|target| target.flag("checkerCapture"));
        }
    }
    if path == V7MoveFilterPath::Legal
        && !forced_en_passant
        && !options.ignore_global_capture_force
        && !own_checker_forced
        && piece.color.owner().is_some()
    {
        for row in 0..8 {
            for col in 0..8 {
                let origin = Square { row, col };
                if let Some(checker) = state.at(origin)
                    && checker.color == piece.color
                    && matches!(checker.kind.as_str(), "checker" | "checkerKing")
                    && !v7_legal_checker_capture_moves(state, checker, origin, effects)?.is_empty()
                {
                    return Ok(Vec::new());
                }
            }
        }
    }
    if path == V7MoveFilterPath::Legal {
        let mut journey_filtered = Vec::new();
        for target in filtered {
            if v7_knight_journey_move_allowed(state, piece, &target)?
                && (!state.flag("zugzwang", piece.color)
                    || ![
                        "colossusAttack",
                        "shotgunBlast",
                        "shotgunSnipe",
                        "merchantBuy",
                        "setLogDirection",
                    ]
                    .iter()
                    .any(|field| target.flag(field)))
            {
                journey_filtered.push(target);
            }
        }
        filtered = journey_filtered;
    }
    if finalize {
        let mut final_moves = Vec::new();
        for target in filtered {
            let capture_cells = v7_move_capture_target_cells(&target)?;
            if !options.ignore_scarecrow_reservation
                && v7_move_crosses_scarecrow_reservation(state, piece, from, &target)?
            {
                continue;
            }
            if v7_armistice_active(state)
                && !target.flag("colossusBody")
                && !target.flag("setLogDirection")
                && !target.flags.get("castle").is_some_and(v7_truth)
                && !["substitutionSwap", "relaySwap", "dragonSwap"]
                    .iter()
                    .any(|flag| target.flag(flag))
                && capture_cells.iter().any(|&at| {
                    state.at(at).is_some_and(|victim| {
                        victim.color != piece.color
                            && victim.color.owner().is_some()
                            && piece.color.owner().is_some()
                            && !matches!(victim.kind.as_str(), "wall" | "football" | "scarecrow")
                            && !v7_stealth_transparent(state, piece, victim, at)
                    })
                })
            {
                continue;
            }
            if v7_large_move_lands_on_protected(state, piece, &target) {
                continue;
            }
            if target.flag("siegeRamMove")
                && crate::v7_capture_reactions::saturation_locked(state, piece)
                && capture_cells.iter().any(|&at| state.at(at).is_some())
            {
                continue;
            }
            if piece.extra.get("desperado").is_some_and(v7_truth)
                && capture_cells.iter().any(|&at| {
                    state
                        .at(at)
                        .is_some_and(|victim| desperado_royal_capture_blocked(piece, victim))
                })
            {
                continue;
            }
            if !options.ignore_quantum_counterpart
                && crate::v7_quantum_state::move_lands_on_quantum_counterpart(piece, &target, None)?
            {
                continue;
            }
            final_moves.push(target);
        }
        filtered = final_moves;
    }
    if finalize && !options.ignore_scarecrow_force && piece.color.owner().is_some() {
        let mut own_captures = Vec::new();
        for target in &filtered {
            if v7_move_captures_scarecrow(state, piece, target)? {
                own_captures.push(target.clone());
            }
        }
        if !own_captures.is_empty() {
            filtered = own_captures;
        } else if state.board.iter().flatten().flatten().any(|victim| {
            victim.kind == "scarecrow"
                && victim.color != piece.color
                && !victim.flag("scarecrowReserved")
        }) {
            let mut probe_options = options;
            probe_options.ignore_scarecrow_force = true;
            // playerHasScarecrowCapture(main96043)는 actor turn으로 바꾸고
            // identity를 한 번씩 조회한다. 자기 shade와 선행 Checker 기록을
            // 같은 disposable 보드에 반영해 다음 조회가 실제 prefix를 읽게 한다.
            let mut probe = state.clone();
            probe.turn = piece.color.owner().ok_or(EngineError::WrongActor)?;
            v7_apply_move_query_effects(&mut probe, effects);
            let mut seen = BTreeSet::new();
            for row in 0..8 {
                for col in 0..8 {
                    let origin = Square { row, col };
                    if let Some(other) = probe.at(origin).cloned()
                        && other.color == piece.color
                        && other.id != piece.id
                        && other.kind != "wall"
                        && seen.insert(other.id.clone())
                    {
                        let targets = v7_compute_legal_move_targets(
                            &probe,
                            &other,
                            origin,
                            probe_options,
                            None,
                            effects,
                        )?;
                        v7_apply_move_query_effects(&mut probe, effects);
                        for target in targets {
                            if v7_move_captures_scarecrow(&probe, &other, &target)? {
                                return Ok(Vec::new());
                            }
                        }
                    }
                }
            }
        }
    }
    if finalize {
        Ok(v7_mark_crown_moves(state, piece, from, filtered))
    } else {
        Ok(filtered)
    }
}

fn v7_legal_checker_capture_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    effects: &mut V7MoveQueryEffects,
) -> Result<Vec<MoveTarget>> {
    if !matches!(piece.kind.as_str(), "checker" | "checkerKing")
        || frozen(piece)
        || crate::card_effects::js_number(
            piece
                .extra
                .get("staked")
                .and_then(|window| window.get("remaining")),
            0,
        )
        .unwrap_or(0.0)
            > 0.0
        || v7_dice_locked(state, piece)
    {
        return Ok(Vec::new());
    }
    let captures = v7_checker_raw_moves(state, piece, from)
        .into_iter()
        .filter(|target| target.flag("checkerCapture"))
        .collect();
    v7_filter_move_targets_with_query_effects(
        state,
        piece,
        from,
        captures,
        V7MoveOptions::default(),
        V7MoveFilterPath::CheckerCaptureProbe,
        effects,
    )
}

fn v7_knight_journey_move_allowed(
    state: &GameState,
    piece: &Piece,
    target: &MoveTarget,
) -> Result<bool> {
    let Some(campaign) = state.extra.get("campaign") else {
        return Ok(true);
    };
    if campaign.get("setup").and_then(Value::as_str) != Some("knightJourney") {
        return Ok(true);
    }
    let hero = if campaign.get("playerColor").and_then(Value::as_str) == Some("black") {
        Color::Black
    } else {
        Color::White
    };
    if piece.color != hero || piece.kind != "knight" {
        return Ok(true);
    }
    let Some(royal_square) = crate::v7_campaign::campaign_king_square(state, hero.opponent())
    else {
        return Ok(true);
    };
    Ok(crate::v7_campaign::knight_journey_ready_to_capture(state)?
        || target.square() != royal_square)
}

/// Baby Bear auto movement has no general getLegalMoves frozen/disarm/forced
/// turn/finalizer gates. Its empty raw jumps only pass applyMoveRestrictions.
pub(crate) fn v7_baby_bear_automatic_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id != RULES_VERSION_V7 || from.row >= 8 || from.col >= 8 {
        return Err(EngineError::InvalidState(
            "v7 Baby Bear automatic origin outside 8x8".into(),
        ));
    }
    if piece.ability_kind() != "babyBear" {
        return Ok(Vec::new());
    }
    let raw = v7_jump_leaps(state, piece, from, KING)
        .into_iter()
        .filter(|target| state.at(target.square()).is_none())
        .collect();
    let raw = v7_apply_portal_moves(state, piece, raw)?;
    let restricted = v7_filter_move_targets(
        state,
        piece,
        from,
        raw,
        V7MoveOptions::default(),
        V7MoveFilterPath::BabyAutomatic,
    )?;
    let holes = v7_black_hole_cells(state)?;
    Ok(restricted
        .into_iter()
        .filter(|target| {
            state.at(target.square()).is_none()
                && !holes.contains(&target.square())
                && v7_portal_capture_cells(target)
                    .iter()
                    .all(|&cell| state.at(cell).is_none())
        })
        .collect())
}

/// Brutus uses a royal-friendly raw Hook witness and its own source early
/// blockers, not the current turn's forced ordinary-move selection.
pub(crate) fn v7_brutus_royal_capture_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id != RULES_VERSION_V7 || from.row >= 8 || from.col >= 8 {
        return Err(EngineError::InvalidState(
            "v7 Brutus origin outside 8x8".into(),
        ));
    }
    if piece.ability_kind() != "brutus"
        || state.threat_probe_depth > 0 && piece.flag("kingThreatSuppressed")
        || collapsed(state, from)
        || state.flag("zugzwang", piece.color) && !state.royal_identity(piece)
    {
        return Ok(Vec::new());
    }
    let options = V7MoveOptions {
        ignore_forced_turn_move: true,
        ignore_global_capture_force: true,
        ignore_scarecrow_force: true,
        ..V7MoveOptions::default()
    };
    if v7_piece_move_blocked(state, piece, options) {
        return Ok(Vec::new());
    }
    let mut raw = crate::variant_movement::v7_brutus_base_moves(state, piece, from)?;
    for target in &mut raw {
        target.flags.insert("brutusBetrayal".into(), json!(true));
    }
    v7_filter_move_targets(state, piece, from, raw, options, V7MoveFilterPath::Brutus)
}

pub(crate) fn v7_manner_capture_locked(state: &GameState, piece: &Piece) -> bool {
    let turns = piece
        .color
        .owner()
        .map(|actor| f64::from(*state.turns_taken.get(actor)))
        .unwrap_or(0.0);
    let fresh =
        crate::observation::number(piece.extra.get("freshNoCaptureUntil")).unwrap_or(0.0) > turns;
    let card =
        crate::observation::number(piece.extra.get("cardNoCaptureUntil")).unwrap_or(0.0) > turns;
    if fresh || crate::observation::truth(piece.extra.get("repositionSecondMove")) {
        return true;
    }
    if piece.kind == "monster" {
        return false;
    }
    let free_move = crate::observation::truth(
        state
            .extra
            .get("freeMoveCaptureLock")
            .and_then(|sides| sides.get(piece.color.as_str())),
    );
    let rush =
        crate::observation::number(piece.extra.get("promotionRushUntil")).unwrap_or(0.0) > turns;
    if free_move
        || card
        || rush
        || (crate::observation::truth(state.extra.get("coolGuy")) || piece.flag("potionManner"))
            && crate::observation::truth(piece.extra.get("coolGuyCapturedLast"))
    {
        return true;
    }
    if matches!(piece.kind.as_str(), "checker" | "checkerKing") {
        return false;
    }
    crate::observation::truth(
        state
            .extra
            .get("quantumPending")
            .and_then(|sides| sides.get(piece.color.as_str())),
    ) || crate::observation::number(piece.extra.get("quantumNoCaptureUntil")).unwrap_or(0.0) > turns
}

pub(crate) fn v7_initiative_capture_locked(
    state: &GameState,
    color: impl Into<crate::PieceColor>,
) -> bool {
    let color = color.into();
    let turns = color
        .owner()
        .map(|actor| f64::from(*state.turns_taken.get(actor)))
        .unwrap_or(0.0);
    state
        .extra
        .get("initiative")
        .and_then(|sides| sides.get(color.as_str()))
        .filter(|entry| v7_truth(entry))
        .is_some_and(|entry| {
            turns - crate::card_effects::js_number(entry.get("startTurn"), 0).unwrap_or(0.0)
                < crate::card_effects::js_number(entry.get("limit"), 0)
                    .filter(|limit| *limit != 0.0)
                    .unwrap_or(7.0)
        })
}

fn v7_disarm_capture_locked(state: &GameState, piece: &Piece) -> bool {
    let free_move = crate::observation::truth(
        state
            .extra
            .get("freeMoveCaptureLock")
            .and_then(|sides| sides.get(piece.color.as_str())),
    );
    crate::observation::number(
        piece
            .extra
            .get("disarmed")
            .and_then(|window| window.get("remaining")),
    )
    .unwrap_or(0.0)
        > 0.0
        || v7_manner_capture_locked(state, piece)
        || v7_initiative_capture_locked(state, piece.color)
        || free_move
        || v7_chaos_capture_locked(state)
        || crate::v7_capture_reactions::saturation_locked(state, piece)
}

fn v7_move_captures_scarecrow(
    state: &GameState,
    piece: &Piece,
    target: &MoveTarget,
) -> Result<bool> {
    if piece.kind == "log" {
        return Ok(false);
    }
    Ok(v7_move_capture_target_cells(target)?.iter().any(|&cell| {
        state.at(cell).is_some_and(|victim| {
            victim.kind == "scarecrow"
                && victim.color != piece.color
                && !victim.flag("scarecrowReserved")
                && can_capture_with_options(
                    state,
                    piece,
                    victim,
                    CaptureOptions {
                        allow_basic_training_capture: target.flag("basicTrainingCapture"),
                        ..CaptureOptions::default()
                    },
                )
        })
    }))
}

fn v7_move_crosses_scarecrow_reservation(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
) -> Result<bool> {
    let mut blocked = state
        .extra
        .get("pendingScarecrows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for row in 0..8 {
        for col in 0..8 {
            if state
                .at(Square { row, col })
                .is_some_and(|entry| entry.kind == "scarecrow" && !entry.flag("scarecrowReserved"))
            {
                blocked.push(json!({"row":row,"col":col,"solid":true}));
            }
        }
    }
    let stationary = [
        "colossusBody",
        "colossusAttack",
        "shotgunBlast",
        "shotgunSnipe",
        "setLogDirection",
        "merchantBuy",
    ]
    .iter()
    .any(|field| target.flag(field));
    let swap = [
        "dragonSwap",
        "substitutionSwap",
        "relaySwap",
        "switcherooMove",
        "symmetryMove",
    ]
    .iter()
    .any(|field| target.flag(field));
    if !stationary {
        if swap {
            if blocked.iter().any(|entry| {
                crate::observation::truth(entry.get("reserved"))
                    && v7_descriptor_square(entry) == Some(target.square())
            }) {
                return Ok(true);
            }
        } else if !target.flag("bent") {
            let segment_blocked = |start: Square, end: Square| {
                let dr = i16::from(end.row) - i16::from(start.row);
                let dc = i16::from(end.col) - i16::from(start.col);
                let steps = dr.abs().max(dc.abs());
                if steps == 0 || dr != 0 && dc != 0 && dr.abs() != dc.abs() {
                    return false;
                }
                blocked
                    .iter()
                    .filter(|entry| {
                        (crate::observation::truth(entry.get("reserved"))
                            || crate::observation::truth(entry.get("solid")))
                            && !crate::observation::truth(entry.get("pieceId"))
                    })
                    .any(|entry| {
                        let last = if crate::observation::truth(entry.get("solid")) {
                            steps - 1
                        } else {
                            steps
                        };
                        (1..=last).any(|step| {
                            v7_descriptor_square(entry).is_some_and(|cell| {
                                i16::from(cell.row) == i16::from(start.row) + dr.signum() * step
                                    && i16::from(cell.col)
                                        == i16::from(start.col) + dc.signum() * step
                            })
                        })
                    })
            };
            if let Some(entry) = target
                .flags
                .get("portalEntry")
                .and_then(v7_descriptor_square)
                && let Some(exit) = target
                    .flags
                    .get("portalExit")
                    .and_then(v7_descriptor_square)
            {
                if segment_blocked(from, entry)
                    || segment_blocked(exit, target.square())
                    || blocked
                        .iter()
                        .any(|entry| v7_descriptor_square(entry) == Some(exit))
                {
                    return Ok(true);
                }
            } else if segment_blocked(from, target.square()) {
                return Ok(true);
            }
        }
    }
    Ok(v7_landing_cells_for_reservation(target)?
        .iter()
        .any(|&cell| {
            v7_movement_reserved_square(state, cell)
                && state
                    .at(cell)
                    .is_none_or(|occupant| occupant.id != piece.id)
        }))
}

fn v7_landing_cells_for_reservation(target: &MoveTarget) -> Result<Vec<Square>> {
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
        if let Some(rook_to) = target.flags.get("rookTo").and_then(v7_descriptor_square)
            && !cells.contains(&rook_to)
        {
            cells.push(rook_to);
        }
        return Ok(cells);
    }
    if (target.flag("colossusMove") || target.flag("bigRookMove"))
        && target.flags.contains_key("highlightCells")
    {
        return highlight_cells(target);
    }
    if target.flag("portalLanding")
        && let Some(exit) = target
            .flags
            .get("portalExit")
            .and_then(v7_descriptor_square)
    {
        return Ok(vec![exit]);
    }
    Ok(vec![target.square()])
}

fn v7_movement_reserved_square(state: &GameState, cell: Square) -> bool {
    state
        .extra
        .get("pendingScarecrows")
        .and_then(Value::as_array)
        .is_some_and(|entries| {
            entries.iter().any(|entry| {
                !crate::observation::truth(entry.get("pieceId"))
                    && v7_descriptor_square(entry) == Some(cell)
            })
        })
        || state
            .extra
            .get("pendingLobsters")
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries
                    .iter()
                    .any(|entry| v7_descriptor_square(entry) == Some(cell))
            })
        || state
            .extra
            .get("pendingPortals")
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry.get("blocksMovement") == Some(&json!(true))
                        && entry
                            .get("cells")
                            .and_then(Value::as_array)
                            .is_some_and(|cells| {
                                cells
                                    .iter()
                                    .any(|entry| v7_descriptor_square(entry) == Some(cell))
                            })
                })
            })
}

fn diagonal_key(square: Square) -> Option<bool> {
    if square.row == square.col {
        Some(false)
    } else if square.row + square.col == 7 {
        Some(true)
    } else {
        None
    }
}

/// Current catalog restricts pawns entering a new main diagonal guarded by an
/// opposing physical bishop. Existing diagonal occupancy, swaps and twins use
/// the source origin of each moving piece separately.
pub(crate) fn fianchetto_destination_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    destinations: &[Square],
) -> bool {
    let Some(owner) = piece.color.owner() else {
        return true;
    };
    if piece.kind != "pawn" || !state.flag("fianchetto", owner.opponent()) {
        return true;
    }
    let origin = diagonal_key(from);
    !destinations.iter().any(|destination| {
        let Some(key) = diagonal_key(*destination).filter(|key| Some(*key) != origin) else {
            return false;
        };
        (0..8).any(|row| {
            (0..8).any(|col| {
                let square = Square { row, col };
                diagonal_key(square) == Some(key)
                    && state.at(square).is_some_and(|bishop| {
                        bishop.color == owner.opponent() && bishop.kind == "bishop"
                    })
            })
        })
    })
}

fn fianchetto_move_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
) -> Result<bool> {
    if [
        "colossusAttack",
        "shotgunBlast",
        "shotgunSnipe",
        "setLogDirection",
    ]
    .into_iter()
    .any(|flag| target.flag(flag))
    {
        return Ok(true);
    }
    let destination = if target.flag("portalLanding") || target.flag("portalThrough") {
        target
            .flags
            .get("portalExit")
            .map(|value| serde_json::from_value(value.clone()).map_err(EngineError::serialization))
            .transpose()?
            .unwrap_or(target.square())
    } else {
        target.square()
    };
    let cells = if target.flag("colossusMove") || target.flag("bigRookMove") {
        target
            .flags
            .get("highlightCells")
            .map(|value| {
                serde_json::from_value::<Vec<Square>>(value.clone())
                    .map_err(EngineError::serialization)
            })
            .transpose()?
            .unwrap_or_else(|| vec![destination])
    } else {
        vec![destination]
    };
    if !fianchetto_destination_allowed(state, piece, from, &cells) {
        return Ok(false);
    }
    let swapped = ["dragonSwap", "substitutionSwap", "relaySwap"]
        .into_iter()
        .any(|flag| target.flag(flag))
        .then(|| state.at(target.square()))
        .flatten();
    if let Some(swapped) = swapped
        && !fianchetto_destination_allowed(state, swapped, target.square(), &[from])
    {
        return Ok(false);
    }
    if crate::observation::truth(piece.extra.get("twinBondId"))
        && let Some(id) = piece.extra.get("twinPartnerId").and_then(Value::as_str)
    {
        for row in 0..8 {
            for col in 0..8 {
                let square = Square { row, col };
                if let Some(partner) = state
                    .at(square)
                    .filter(|partner| partner.id == id && partner.color == piece.color)
                    && swapped.is_none_or(|swapped| swapped.id != partner.id)
                    && !fianchetto_destination_allowed(state, partner, square, &cells)
                {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

pub(crate) fn expansion_destination_allowed(
    state: &GameState,
    color: PieceColor,
    cells: &[Square],
) -> bool {
    if !d4_destination_allowed(state, color, cells) {
        return false;
    }
    let Some(owner) = color.owner() else {
        return true;
    };
    let enemy = owner.opponent();
    if state.flag("synchronization", enemy) {
        let mut parity = None;
        let mut uniform = true;
        for row in 0..8 {
            for col in 0..8 {
                if state
                    .at(Square { row, col })
                    .is_some_and(|piece| piece.color == enemy)
                {
                    let next = (row + col) % 2;
                    if parity.is_some_and(|old| old != next) {
                        uniform = false;
                        break;
                    }
                    parity = Some(next);
                }
            }
        }
        if uniform
            && parity.is_some_and(|required| {
                cells
                    .iter()
                    .any(|cell| (cell.row + cell.col) % 2 != required)
            })
        {
            return false;
        }
    }
    true
}

/// Some source installation callers pass d4 but deliberately omit the
/// synchronization argument. Keep that policy distinct from move placement.
pub(crate) fn d4_destination_allowed(
    state: &GameState,
    color: PieceColor,
    cells: &[Square],
) -> bool {
    if cells.is_empty() || cells.iter().any(|cell| cell.row >= 8 || cell.col >= 8) {
        return false;
    }
    color.owner().is_none_or(|owner| {
        let enemy = owner.opponent();
        !state.flag("d4", enemy)
            || !cells.contains(&Square {
                row: if enemy == Color::Black { 3 } else { 4 },
                col: 3,
            })
    })
}
fn expansion_move_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
) -> Result<bool> {
    if ![Color::White, Color::Black]
        .into_iter()
        .any(|side| state.flag("d4", side) || state.flag("synchronization", side))
    {
        return Ok(true);
    }
    if [
        "grapplePull",
        "setLogDirection",
        "colossusBody",
        "colossusAttack",
        "shotgunBlast",
        "shotgunSnipe",
        "merchantBuy",
    ]
    .into_iter()
    .any(|flag| target.flag(flag))
    {
        return Ok(true);
    }
    let read = |name: &str| {
        target
            .flags
            .get(name)
            .map(|v| {
                serde_json::from_value::<Square>(v.clone()).map_err(EngineError::serialization)
            })
            .transpose()
    };
    let destination = if target.flag("portalLanding") {
        read("portalExit")?.unwrap_or(target.square())
    } else {
        target.square()
    };
    let cells = target
        .flags
        .get("highlightCells")
        .map(|v| {
            serde_json::from_value::<Vec<Square>>(v.clone()).map_err(EngineError::serialization)
        })
        .transpose()?
        .filter(|cells| !cells.is_empty())
        .unwrap_or_else(|| vec![destination]);
    if !expansion_destination_allowed(state, piece.color, &cells) {
        return Ok(false);
    }
    for name in ["portalEntry", "portalExit"] {
        if let Some(cell) = read(name)?
            && !expansion_destination_allowed(state, piece.color, &[cell])
        {
            return Ok(false);
        }
    }
    if target
        .flags
        .get("castle")
        .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
        && let (Some(rook_from), Some(rook_to)) = (read("rookFrom")?, read("rookTo")?)
        && let Some(rook) = state.at(rook_from)
    {
        let landing = target
            .flags
            .get("rookHighlightCells")
            .map(|v| {
                serde_json::from_value::<Vec<Square>>(v.clone()).map_err(EngineError::serialization)
            })
            .transpose()?
            .unwrap_or_else(|| vec![rook_to]);
        if !expansion_destination_allowed(state, rook.color, &landing) {
            return Ok(false);
        }
    }
    if [
        "relaySwap",
        "substitutionSwap",
        "dragonSwap",
        "solidaritySwap",
    ]
    .into_iter()
    .any(|flag| target.flag(flag))
        && let Some(other) = state.at(destination)
        && !expansion_destination_allowed(state, other.color, &[from])
    {
        return Ok(false);
    }
    Ok(true)
}

#[derive(Clone, Copy)]
struct V7EnPassantRight {
    row: u8,
    col: u8,
    captured_row: u8,
    captured_col: u8,
    color: Color,
}

/// Number.isInteger does not coerce strings. Valid out-of-board rights have
/// no reachable landing/victim in this 8x8 host, so they can be discarded.
fn v7_en_passant_right(value: &Value) -> Option<V7EnPassantRight> {
    let integer = |field| {
        let number = value.get(field)?.as_f64()?;
        (number.is_finite() && number.fract() == 0.0 && (0.0..=255.0).contains(&number))
            .then_some(number as u8)
    };
    Some(V7EnPassantRight {
        row: integer("row")?,
        col: integer("col")?,
        captured_row: integer("capturedRow")?,
        captured_col: integer("capturedCol")?,
        color: match value.get("color")?.as_str()? {
            "white" => Color::White,
            "black" => Color::Black,
            _ => return None,
        },
    })
}

fn v7_active_en_passant_states(state: &GameState) -> Result<Vec<V7EnPassantRight>> {
    let primary = state
        .en_passant
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(EngineError::serialization)?;
    let mut entries = Vec::new();
    let mut append = |value: &Value| {
        if let Some(right) = v7_en_passant_right(value) {
            entries.push(right);
        }
        if let Some(additional) = value.get("additional").and_then(Value::as_array) {
            entries.extend(additional.iter().filter_map(v7_en_passant_right));
        }
    };
    if let Some(primary) = &primary {
        append(primary);
    }
    if let Some(editor) = state.extra.get("simpleBoardEditor")
        && crate::observation::truth(editor.get("enabled"))
        && let Some(primary) = &primary
        && let Some(anchor) = editor.get("enPassantAnchor")
        && ["row", "col", "capturedRow", "capturedCol"]
            .iter()
            .all(|field| {
                primary.get(*field).and_then(Value::as_f64)
                    == anchor.get(*field).and_then(Value::as_f64)
            })
        && primary.get("color") == anchor.get("color")
        && let Some(editor_entries) = editor.get("enPassantStates").and_then(Value::as_array)
    {
        for entry in editor_entries {
            append(entry);
        }
    }
    let mut seen = BTreeSet::new();
    entries.retain(|right| {
        seen.insert((
            right.row,
            right.col,
            right.captured_row,
            right.captured_col,
            right.color.as_str(),
        ))
    });
    Ok(entries)
}

fn v7_fresh_capture_locked(state: &GameState, piece: &Piece) -> bool {
    let Some(actor) = piece.color.owner() else {
        return false;
    };
    let value = piece.extra.get("freshNoCaptureUntil");
    crate::observation::truth(value)
        && crate::card_effects::js_number(value, 0)
            .is_some_and(|until| f64::from(*state.turns_taken.get(actor)) < until)
}

/// 같은 pawn 기하 분기의 방향과 허용 조건이다. en passant 권리는
/// 해당 관측에서 준비한 slice를 그대로 사용한다.
struct V7PawnDirection {
    direction: i8,
    double_step: bool,
    allow_en_passant: bool,
    converted: bool,
}

fn v7_pawn_directional_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    options: V7PawnDirection,
    rights: &[V7EnPassantRight],
) -> Vec<MoveTarget> {
    let V7PawnDirection {
        direction,
        double_step,
        allow_en_passant,
        converted,
    } = options;
    let Some(actor) = piece.color.owner() else {
        return Vec::new();
    };
    let occupant = |at| {
        state
            .at(at)
            .filter(|victim| !v7_stealth_transparent(state, piece, victim, at))
    };
    let mut moves = Vec::new();
    let home = if actor == Color::White { 6 } else { 1 };
    let start_row = double_step && (from.row == home || from.row == actor.home_row());
    if converted {
        for dc in [-1, 1] {
            let Some(one) = from.offset(direction, dc) else {
                continue;
            };
            if crown_ground_from_state(state, one) {
                continue;
            }
            if let Some(victim) = occupant(one) {
                if piece.kind == "pawn"
                    && state.flag("pawnLeap", actor)
                    && direction == actor.pawn_dir()
                    && victim.color == actor.opponent()
                    && let Some(two) = from.offset(direction * 2, dc * 2)
                    && occupant(two).is_none()
                {
                    let mut target = MoveTarget::at(two);
                    target.flags.insert("pawnLeap".into(), json!(true));
                    moves.push(target);
                }
                continue;
            }
            moves.push(MoveTarget::at(one));
            if !piece.moved
                && start_row
                && let Some(two) = from.offset(direction * 2, dc * 2)
                && occupant(two).is_none()
                && !crown_ground_from_state(state, two)
            {
                moves.push(MoveTarget::at(two));
                if piece.kind == "pawn"
                    && state.flag("pawnSprint", actor)
                    && let Some(three) = from.offset(direction * 3, dc * 3)
                    && occupant(three).is_none()
                    && !crown_ground_from_state(state, three)
                {
                    moves.push(MoveTarget::at(three));
                }
            }
            if piece.kind == "pawn"
                && state.flag("pawnSprint", actor)
                && piece.flag("londonSystemPawn")
                && from.row == home
                && direction == actor.pawn_dir()
            {
                for distance in 2..=3 {
                    let Some(to) = from.offset(direction * distance, dc * distance) else {
                        break;
                    };
                    if occupant(to).is_some() || crown_ground_from_state(state, to) {
                        break;
                    }
                    if !moves.iter().any(|target| target.square() == to) {
                        moves.push(MoveTarget::at(to));
                    }
                }
            }
        }
        if let Some(one) = from.offset(direction, 0) {
            if crown_ground_from_state(state, one) {
                let mut target = MoveTarget::at(one);
                target
                    .flags
                    .insert("crownGroundCapture".into(), json!(true));
                moves.push(target);
            } else if occupant(one).is_some_and(|victim| can_capture(state, piece, victim)) {
                moves.push(MoveTarget::at(one));
            }
        }
        return moves;
    }
    if let Some(one) = from.offset(direction, 0) {
        if occupant(one).is_none() {
            let crown = crown_ground_from_state(state, one);
            let mut target = MoveTarget::at(one);
            if crown {
                target
                    .flags
                    .insert("crownGroundCapture".into(), json!(true));
            }
            moves.push(target);
            if !crown {
                if !piece.moved
                    && start_row
                    && let Some(exit) = v7_portal_exit_at(state, one)
                {
                    if let Some(to) = exit.offset(direction, 0)
                        && occupant(exit).is_none()
                        && !collapsed(state, to)
                        && occupant(to).is_none()
                        && !crown_ground_from_state(state, to)
                        && !v7_scarecrow_reserved_square(state, to)
                    {
                        let mut target = MoveTarget::at(to);
                        target
                            .flags
                            .insert("pawnPortalDoubleStep".into(), json!(true));
                        target.flags.insert("portalThrough".into(), json!(true));
                        target.flags.insert("portalEntry".into(), json!(one));
                        target.flags.insert("portalExit".into(), json!(exit));
                        moves.push(target);
                    }
                } else if !piece.moved
                    && start_row
                    && let Some(two) = from.offset(direction * 2, 0)
                    && occupant(two).is_none()
                    && !crown_ground_from_state(state, two)
                {
                    let mut target = MoveTarget::at(two);
                    target
                        .flags
                        .insert("standardPawnDoubleStep".into(), json!(true));
                    moves.push(target);
                    if piece.kind == "pawn"
                        && state.flag("pawnSprint", actor)
                        && let Some(three) = from.offset(direction * 3, 0)
                        && occupant(three).is_none()
                        && !crown_ground_from_state(state, three)
                    {
                        let mut target = MoveTarget::at(three);
                        target
                            .flags
                            .insert("pawnSprintTripleStep".into(), json!(true));
                        moves.push(target);
                    }
                }
                if piece.kind == "pawn"
                    && state.flag("pawnSprint", actor)
                    && piece.flag("londonSystemPawn")
                    && from.row == home
                    && direction == actor.pawn_dir()
                {
                    for distance in 2..=3 {
                        let Some(to) = from.offset(direction * distance, 0) else {
                            break;
                        };
                        if occupant(to).is_some() || crown_ground_from_state(state, to) {
                            break;
                        }
                        if !moves.iter().any(|target| target.square() == to) {
                            let mut target = MoveTarget::at(to);
                            if distance == 3 {
                                target
                                    .flags
                                    .insert("pawnSprintTripleStep".into(), json!(true));
                            }
                            moves.push(target);
                        }
                    }
                }
                if piece.kind == "pawn" && piece.flag("chargeRush") && direction == actor.pawn_dir()
                {
                    for distance in 2..=3 {
                        let Some(to) = from.offset(direction * distance, 0) else {
                            break;
                        };
                        if occupant(to).is_some() || crown_ground_from_state(state, to) {
                            break;
                        }
                        if !moves.iter().any(|target| target.square() == to) {
                            let mut target = MoveTarget::at(to);
                            target.flags.insert("chargeRush".into(), json!(true));
                            moves.push(target);
                        }
                    }
                }
            }
        } else if let Some(victim) = occupant(one) {
            if state.flag("breakthroughPawns", actor) && can_capture(state, piece, victim) {
                moves.push(MoveTarget::at(one));
            }
            if piece.kind == "pawn"
                && state.flag("pawnLeap", actor)
                && direction == actor.pawn_dir()
                && victim.color == actor.opponent()
                && let Some(two) = from.offset(direction * 2, 0)
                && occupant(two).is_none()
                && !crown_ground_from_state(state, two)
            {
                let mut target = MoveTarget::at(two);
                target.flags.insert("pawnLeap".into(), json!(true));
                moves.push(target);
            }
        }
    }
    for dc in [-1, 1] {
        let Some(to) = from.offset(direction, dc) else {
            continue;
        };
        if crown_ground_from_state(state, to) && piece.kind != "pawn" {
            let mut target = MoveTarget::at(to);
            target
                .flags
                .insert("crownGroundCapture".into(), json!(true));
            moves.push(target);
        } else if occupant(to).is_some_and(|victim| can_capture(state, piece, victim)) {
            moves.push(MoveTarget::at(to));
        }
        if allow_en_passant
            && let Some(right) = rights.iter().find(|right| {
                right.color != actor
                    && right.row == to.row
                    && right.col == to.col
                    && right.captured_col == to.col
            })
        {
            let captured = Square {
                row: right.captured_row,
                col: right.captured_col,
            };
            if state
                .at(to)
                .is_none_or(|victim| can_capture(state, piece, victim))
                && state.at(captured).is_some_and(|victim| {
                    victim.kind == "pawn" && can_capture(state, piece, victim)
                })
            {
                let mut target = MoveTarget::at(to);
                target.flags.insert("enPassant".into(), json!(true));
                target
                    .flags
                    .insert("capturedRow".into(), json!(captured.row));
                target
                    .flags
                    .insert("capturedCol".into(), json!(captured.col));
                moves.push(target);
            }
        }
    }
    if allow_en_passant && piece.kind == "pawn" && state.flag("enPassantFrenzy", actor) {
        for dc in [-1, 1] {
            let Some(to) = from.offset(direction, dc) else {
                continue;
            };
            let Some(captured) = from.offset(0, dc) else {
                continue;
            };
            if state
                .at(to)
                .is_none_or(|victim| can_capture(state, piece, victim))
                && state
                    .at(captured)
                    .is_some_and(|victim| can_capture(state, piece, victim))
                && !moves.iter().any(|target| {
                    target.flag("enPassant")
                        && target.square() == to
                        && target.flags.get("capturedRow") == Some(&json!(captured.row))
                        && target.flags.get("capturedCol") == Some(&json!(captured.col))
                })
            {
                let mut target = MoveTarget::at(to);
                target.flags.insert("enPassant".into(), json!(true));
                target.flags.insert("enPassantFrenzy".into(), json!(true));
                target
                    .flags
                    .insert("capturedRow".into(), json!(captured.row));
                target
                    .flags
                    .insert("capturedCol".into(), json!(captured.col));
                moves.push(target);
            }
        }
    }
    moves
}

fn v7_pawn_moves(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    let Some(actor) = piece.color.owner() else {
        return Ok(Vec::new());
    };
    let rights = v7_active_en_passant_states(state)?;
    let masters = KING
        .iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .filter_map(|at| state.at(at))
        .filter(|master| master.color == actor && master.ability_kind() == "knightmaster")
        .collect::<Vec<_>>();
    let influenced = piece.kind == "pawn" && !masters.is_empty();
    let converted = piece.kind == "pawn" && state.flag("pawnConversion", actor);
    let mut moves = if influenced {
        let mut moves = v7_raw_knight_moves(state, piece, from, true)?;
        if !masters
            .iter()
            .any(|master| !v7_fresh_capture_locked(state, master))
        {
            let mut quiet = Vec::new();
            for target in moves {
                if !v7_is_capture_move(state, piece, &target)? {
                    quiet.push(target);
                }
            }
            moves = quiet;
        }
        moves
    } else {
        let mut moves = v7_pawn_directional_moves(
            state,
            piece,
            from,
            V7PawnDirection {
                direction: actor.pawn_dir(),
                double_step: true,
                allow_en_passant: true,
                converted,
            },
            &rights,
        );
        if state.flag("retreat", actor) {
            moves.extend(v7_pawn_directional_moves(
                state,
                piece,
                from,
                V7PawnDirection {
                    direction: -actor.pawn_dir(),
                    double_step: false,
                    allow_en_passant: false,
                    converted,
                },
                &rights,
            ));
        }
        let bearer = piece.ability_kind() == "standardBearer"
            || piece.kind == "pawn"
                && state.board[usize::from(from.row)]
                    .iter()
                    .flatten()
                    .any(|bearer| {
                        bearer.color == actor && bearer.ability_kind() == "standardBearer"
                    });
        if bearer {
            let ready = state.board[usize::from(from.row)]
                .iter()
                .flatten()
                .any(|bearer| {
                    bearer.color == actor
                        && bearer.ability_kind() == "standardBearer"
                        && !v7_fresh_capture_locked(state, bearer)
                });
            for dc in [-1, 1] {
                if let Some(to) = from.offset(0, dc)
                    && state
                        .at(to)
                        .filter(|victim| !v7_stealth_transparent(state, piece, victim, to))
                        .is_none_or(|victim| ready && can_capture(state, piece, victim))
                {
                    moves.push(MoveTarget::at(to));
                }
            }
        }
        moves
    };
    if piece.kind == "pawn" && !converted && !influenced {
        for dc in [-1, 1] {
            let Some(to) = from.offset(actor.pawn_dir(), dc) else {
                continue;
            };
            let right = rights.iter().find(|right| {
                right.color != actor
                    && right.row == to.row
                    && right.col == to.col
                    && state
                        .at(Square {
                            row: right.captured_row,
                            col: right.captured_col,
                        })
                        .is_some_and(|victim| {
                            victim.kind == "pawn" && victim.color == actor.opponent()
                        })
            });
            let side = from.offset(0, dc).unwrap();
            let frenzy = right.is_none()
                && state.flag("enPassantFrenzy", actor)
                && state
                    .at(side)
                    .is_some_and(|victim| victim.color == actor.opponent());
            if right.is_none() && !frenzy {
                continue;
            }
            let captured = right.map_or(side, |right| Square {
                row: right.captured_row,
                col: right.captured_col,
            });
            moves.retain(|target| target.square() != to);
            if state
                .at(captured)
                .is_some_and(|victim| can_capture(state, piece, victim))
                && state
                    .at(to)
                    .is_none_or(|victim| can_capture(state, piece, victim))
            {
                let mut target = MoveTarget::at(to);
                target.flags.insert("enPassant".into(), json!(true));
                target
                    .flags
                    .insert("capturedRow".into(), json!(captured.row));
                target
                    .flags
                    .insert("capturedCol".into(), json!(captured.col));
                if frenzy {
                    target.flags.insert("enPassantFrenzy".into(), json!(true));
                }
                moves.push(target);
            }
        }
    }
    if piece.kind == "pawn" {
        if let Some(to) = from.offset(actor.pawn_dir(), 0)
            && state.flag("proficiency", actor)
            && v7_should_promote(state, piece, to.row)?
            && !v7_should_promote(state, piece, from.row)?
            && state
                .at(to)
                .is_some_and(|victim| can_capture(state, piece, victim))
            && !moves.iter().any(|target| target.square() == to)
        {
            let mut target = MoveTarget::at(to);
            target.flags.insert("capture".into(), json!(true));
            moves.push(target);
        }
        if state.flag("longEnPassant", actor) {
            let mut seen = BTreeSet::new();
            for right in &rights {
                if right.color == actor
                    || i16::from(right.row) != i16::from(from.row) + i16::from(actor.pawn_dir())
                    || right.col == from.col
                    || right.col != right.captured_col
                {
                    continue;
                }
                let to = Square {
                    row: right.row,
                    col: right.col,
                };
                let captured = Square {
                    row: right.captured_row,
                    col: right.captured_col,
                };
                if to.row >= 8
                    || to.col >= 8
                    || state.at(to).is_some()
                    || !state.at(captured).is_some_and(|victim| {
                        victim.kind == "pawn"
                            && victim.color == right.color
                            && can_capture(state, piece, victim)
                    })
                    || !seen.insert((to, captured))
                {
                    continue;
                }
                moves.retain(|target| target.square() != to);
                let mut target = MoveTarget::at(to);
                target.flags.insert("enPassant".into(), json!(true));
                target.flags.insert("capture".into(), json!(true));
                target.flags.insert("longEnPassant".into(), json!(true));
                target
                    .flags
                    .insert("capturedRow".into(), json!(captured.row));
                target
                    .flags
                    .insert("capturedCol".into(), json!(captured.col));
                moves.push(target);
            }
        }
        let pawns = state
            .board
            .iter()
            .enumerate()
            .flat_map(|(row, line)| line.iter().flatten().map(move |pawn| (row, pawn)))
            .filter(|(_, pawn)| pawn.kind == "pawn" && pawn.color == actor)
            .collect::<Vec<_>>();
        let vanguard = state.flag("vanguard", actor)
            && pawns
                .iter()
                .filter(|(row, _)| *row == usize::from(from.row))
                .count()
                == 1
            && !pawns.iter().any(|(row, _)| {
                if actor == Color::White {
                    *row < usize::from(from.row)
                } else {
                    *row > usize::from(from.row)
                }
            });
        if vanguard {
            let diagonal_only = state.extra.get("vanguardDiagonalOnly") != Some(&json!(false));
            for dc in [-1, 0, 1] {
                let Some(to) = from.offset(actor.pawn_dir(), dc) else {
                    continue;
                };
                if diagonal_only && (dc == 0 || state.at(to).is_some())
                    || moves.iter().any(|target| target.square() == to)
                {
                    continue;
                }
                if state.at(to).is_some_and(|victim| {
                    !v7_stealth_transparent(state, piece, victim, to)
                        && !can_capture(state, piece, victim)
                }) {
                    continue;
                }
                let side = from.offset(0, dc).and_then(|at| state.at(at));
                let special = dc != 0
                    && side.is_some_and(|victim| {
                        victim.color == actor.opponent()
                            && (state.flag("enPassantFrenzy", actor)
                                || victim.kind == "pawn"
                                    && rights.iter().any(|right| {
                                        right.color != actor
                                            && right.row == to.row
                                            && right.col == to.col
                                    }))
                    });
                if !special {
                    moves.push(MoveTarget::at(to));
                }
            }
        }
    }
    Ok(moves)
}

fn v7_has_forced_en_passant(state: &GameState, actor: Color) -> Result<bool> {
    if !crate::observation::truth(state.extra.get("machoChess"))
        || state.flag("pawnConversion", actor)
    {
        return Ok(false);
    }
    let rights = v7_active_en_passant_states(state)?;
    if !rights.iter().any(|right| right.color != actor) {
        return Ok(false);
    }
    for row in 0..8 {
        for col in 0..8 {
            let from = Square { row, col };
            let Some(piece) = state
                .at(from)
                .filter(|piece| piece.kind == "pawn" && piece.color == actor)
            else {
                continue;
            };
            if v7_disarm_capture_locked(state, piece) {
                continue;
            }
            for target in v7_pawn_directional_moves(
                state,
                piece,
                from,
                V7PawnDirection {
                    direction: actor.pawn_dir(),
                    double_step: true,
                    allow_en_passant: true,
                    converted: false,
                },
                &rights,
            ) {
                if target.flag("enPassant")
                    && target
                        .flags
                        .get("capturedRow")
                        .and_then(Value::as_u64)
                        .zip(target.flags.get("capturedCol").and_then(Value::as_u64))
                        .and_then(|(row, col)| {
                            u8::try_from(row)
                                .ok()
                                .zip(u8::try_from(col).ok())
                                .map(|(row, col)| Square { row, col })
                        })
                        .and_then(|at| state.at(at))
                        .is_some_and(|victim| can_capture(state, piece, victim))
                {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

/// knightMoves uses its literal occupant, unlike jumpMoves' concealed target
/// projection. The two policies share deltas, but not candidate eligibility.
pub(crate) fn v7_raw_knight_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    disable_radical_charge: bool,
) -> Result<Vec<MoveTarget>> {
    let deltas = crate::variant_movement::knight_deltas_for_move(state, piece, from)?;
    let mut moves = Vec::new();
    for (dr, dc) in deltas {
        let Some(to) = from.offset(dr, dc) else {
            continue;
        };
        let victim = state.at(to);
        let friendly =
            victim.is_some_and(|victim| v7_mad_horse_friendly_target(state, piece, victim, to));
        if victim.is_some_and(|victim| !friendly && !can_capture(state, piece, victim)) {
            continue;
        }
        let mut target = MoveTarget::at(to);
        if friendly {
            target.flags.insert("madHorseCapture".into(), json!(true));
        }
        if !disable_radical_charge
            && (state.flag("radicalCharge", piece.color) || piece.flag("brutalKnight"))
        {
            let vertical = dr.abs() > dc.abs();
            let jump = if vertical {
                from.offset(dr.signum(), 0)
            } else {
                from.offset(0, dc.signum())
            };
            if let Some(jump) = jump
                && let Some(jumped) = state.at(jump)
                && !v7_manner_capture_locked(state, piece)
                && can_capture(state, piece, jumped)
                && jumped.ability_kind() != "jester"
                && !v7_encouraged_at(state, jumped, jump)
            {
                target.flags.insert("jumpCapture".into(), json!(jump));
                if crate::v7_capture_reactions::is_hp_piece(state, jumped) {
                    let second = if vertical {
                        from.offset(dr.signum(), dc)
                    } else {
                        from.offset(dr, dc.signum())
                    };
                    let hits = 1 + usize::from(
                        second
                            .and_then(|at| state.at(at))
                            .is_some_and(|other| other.id == jumped.id),
                    );
                    target.flags.insert("jumpCaptureHits".into(), json!(hits));
                }
            }
        }
        moves.push(target);
    }
    Ok(moves)
}

pub(crate) fn v7_mad_horse_friendly_target(
    state: &GameState,
    piece: &Piece,
    victim: &Piece,
    at: Square,
) -> bool {
    piece.kind == "knight"
        && state.flag("madHorse", piece.color)
        && victim.id != piece.id
        && victim.color == piece.color
        && !state.royal_identity(victim)
        && !victim.is_large()
        && !crate::v7_capture_reactions::is_hp_piece(state, victim)
        && !matches!(
            victim.kind.as_str(),
            "merchant"
                | "timeTraveler"
                | "vampireLord"
                | "guard"
                | "wall"
                | "football"
                | "blackHole"
                | "coffin"
        )
        && !v7_manner_capture_locked(state, piece)
        && !((crate::observation::truth(state.extra.get("saturationRule"))
            || piece.flag("potionSaturation"))
            && crate::observation::number(piece.extra.get("capturesMade")).unwrap_or(0.0) >= 3.0)
        && !v7_initiative_capture_locked(state, piece.color)
        && !piece
            .extra
            .get("repositionSecondMove")
            .is_some_and(v7_truth)
        && !victim.flag("shielded")
        && !victim.flag("protected")
        && crate::observation::number(
            victim
                .extra
                .get("vigilanceProtection")
                .and_then(|v| v.get("remaining")),
        )
        .unwrap_or(0.0)
            <= 0.0
        && !v7_encouraged_at(state, victim, at)
        && can_capture_with_options(
            state,
            piece,
            victim,
            CaptureOptions {
                allow_friendly: true,
                ..CaptureOptions::default()
            },
        )
}

pub(crate) fn pawn_moves(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let Some(actor) = piece.color.owner() else {
        return Vec::new();
    };
    let movement_target = |square| {
        state
            .at(square)
            .filter(|target| !v7_stealth_transparent(state, piece, target, square))
    };
    let dir = if state.flag("reversePawns", piece.color) {
        -actor.pawn_dir()
    } else {
        actor.pawn_dir()
    };
    if piece.kind == "pawn" {
        let neighboring_master =
            KING.iter()
                .filter_map(|&(dr, dc)| from.offset(dr, dc))
                .any(|square| {
                    state
                        .at(square)
                        .is_some_and(|p| p.color == piece.color && p.kind == "knightmaster")
                });
        if neighboring_master {
            return leaps(state, piece, from, KNIGHT);
        }
    }
    let mut moves = if state.ruleset_id == RULES_VERSION_V7
        && piece.kind == "pawn"
        && state.flag("pawnConversion", piece.color)
    {
        v7_converted_base_pawn_moves(state, piece, from, actor, dir)
    } else {
        let mut moves = Vec::new();
        for forward in [dir, -dir] {
            if forward != dir && !state.flag("retreat", piece.color) {
                continue;
            }
            if let Some(one) = from.offset(forward, 0)
                && movement_target(one).is_none()
                && !collapsed(state, one)
            {
                moves.push(MoveTarget::at(one));
                let start = if piece.color == Color::White { 6 } else { 1 };
                if forward == dir
                    && !piece.moved
                    && (from.row == start || from.row == actor.home_row())
                    && let Some(two) = one.offset(forward, 0)
                    && movement_target(two).is_none()
                    && !collapsed(state, two)
                {
                    let mut target = MoveTarget::at(two);
                    target
                        .flags
                        .insert("standardPawnDoubleStep".into(), json!(true));
                    moves.push(target);
                    if piece.kind == "pawn"
                        && state.flag("pawnSprint", piece.color)
                        && let Some(three) = two.offset(forward, 0)
                        && movement_target(three).is_none()
                        && !collapsed(state, three)
                    {
                        let mut target = MoveTarget::at(three);
                        target
                            .flags
                            .insert("pawnSprintTripleStep".into(), json!(true));
                        moves.push(target);
                    }
                }
            } else if piece.kind == "pawn"
                && state.flag("pawnLeap", piece.color)
                && forward == actor.pawn_dir()
                && let Some(one) = from.offset(forward, 0)
                && movement_target(one).is_some_and(|piece| piece.color == actor.opponent())
                && let Some(two) = one.offset(forward, 0)
                && movement_target(two).is_none()
                && !collapsed(state, two)
            {
                let mut target = MoveTarget::at(two);
                target.flags.insert("pawnLeap".into(), json!(true));
                moves.push(target);
            }
            for dc in [-1, 1] {
                if let Some(to) = from.offset(forward, dc)
                    && movement_target(to).is_some_and(|target| can_capture(state, piece, target))
                    && !collapsed(state, to)
                {
                    moves.push(MoveTarget::at(to));
                }
            }
        }
        moves
    };
    if let Some(right) = &state.en_passant
        && right.color != piece.color
        && right.row as i16 == from.row as i16 + i16::from(dir)
        && (right.col as i16 - from.col as i16).abs() == 1
    {
        let captured = Square {
            row: right.captured_row,
            col: right.captured_col,
        };
        let to = Square {
            row: right.row,
            col: right.col,
        };
        if state
            .at(captured)
            .is_some_and(|p| p.kind == "pawn" && can_capture(state, piece, p))
            && landing(state, piece, to)
        {
            moves.retain(|target| target.square() != to);
            let mut target = MoveTarget::at(to);
            target.flags.insert("enPassant".into(), json!(true));
            target
                .flags
                .insert("capturedRow".into(), json!(captured.row));
            target
                .flags
                .insert("capturedCol".into(), json!(captured.col));
            moves.push(target);
        }
    }
    let bearer = piece.ability_kind() == "standardBearer"
        || piece.kind == "pawn"
            && state.board[from.row as usize]
                .iter()
                .flatten()
                .any(|target| {
                    target.color == piece.color && target.ability_kind() == "standardBearer"
                });
    if bearer {
        let capture_ready = state.board[from.row as usize]
            .iter()
            .flatten()
            .any(|target| {
                target.color == piece.color
                    && target.ability_kind() == "standardBearer"
                    && target.number("freshNoCaptureUntil")
                        <= i64::from(*state.turns_taken.get(actor))
            });
        for dc in [-1, 1] {
            if let Some(cell) = from.offset(0, dc)
                && state
                    .at(cell)
                    .is_none_or(|target| capture_ready && can_capture(state, piece, target))
                && !collapsed(state, cell)
            {
                moves.push(MoveTarget::at(cell));
            }
        }
    }
    // main96225: the source replaces a diagonal landing with the prioritized
    // side capture. The destination can itself contain a second victim.
    if piece.kind == "pawn" && state.flag("enPassantFrenzy", actor) {
        for dc in [-1, 1] {
            let Some(to) = from.offset(dir, dc) else {
                continue;
            };
            let Some(side) = from.offset(0, dc) else {
                continue;
            };
            let ordinary_right = state.en_passant.as_ref().is_some_and(|right| {
                right.color != actor
                    && right.row == to.row
                    && right.col == to.col
                    && state
                        .at(Square {
                            row: right.captured_row,
                            col: right.captured_col,
                        })
                        .is_some_and(|victim| {
                            victim.kind == "pawn" && victim.color == actor.opponent()
                        })
            });
            if ordinary_right
                || state
                    .at(side)
                    .is_none_or(|victim| victim.color != actor.opponent())
            {
                continue;
            }
            moves.retain(|target| target.square() != to);
            if state
                .at(side)
                .is_some_and(|victim| can_capture(state, piece, victim))
                && state
                    .at(to)
                    .is_none_or(|victim| can_capture(state, piece, victim))
            {
                let mut target = MoveTarget::at(to);
                target.flags.insert("enPassant".into(), json!(true));
                target.flags.insert("capturedRow".into(), json!(side.row));
                target.flags.insert("capturedCol".into(), json!(side.col));
                target.flags.insert("enPassantFrenzy".into(), json!(true));
                moves.push(target);
            }
        }
    }
    moves
}

/// Frozen basePawnMoves delegates to addConvertedPawnDirectionalMoves when
/// pawnConversion is active. Empty diagonals, not empty forward cells, are the
/// ordinary advance; the forward cell is a capture only. Its double diagonal
/// has no standardPawnDoubleStep flag in the source.
fn v7_converted_base_pawn_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    actor: Color,
    dir: i8,
) -> Vec<MoveTarget> {
    let movement_target = |square| {
        state
            .at(square)
            .filter(|target| !v7_stealth_transparent(state, piece, target, square))
    };
    let mut moves = Vec::new();
    for forward in [dir, -dir] {
        if forward != dir && !state.flag("retreat", piece.color) {
            continue;
        }
        let Some(one) = from.offset(forward, 0) else {
            continue;
        };
        for dc in [-1, 1] {
            let Some(diagonal) = from.offset(forward, dc) else {
                continue;
            };
            if crown_ground_from_state(state, diagonal) {
                continue;
            }
            if let Some(blocker) = movement_target(diagonal) {
                let Some(landing) = from.offset(forward * 2, dc * 2) else {
                    continue;
                };
                if forward == actor.pawn_dir()
                    && state.flag("pawnLeap", piece.color)
                    && blocker.color == actor.opponent()
                    && (state.extra.get("september26Balance") == Some(&json!(true))
                        || blocker.kind == "pawn")
                    && movement_target(landing).is_none()
                {
                    let mut target = MoveTarget::at(landing);
                    target.flags.insert("pawnLeap".into(), json!(true));
                    moves.push(target);
                }
                continue;
            }
            moves.push(MoveTarget::at(diagonal));
            if !piece.moved
                && forward == dir
                && (from.row == if actor == Color::White { 6 } else { 1 }
                    || from.row == actor.home_row())
                && let Some(two) = from.offset(forward * 2, dc * 2)
                && movement_target(two).is_none()
                && !crown_ground_from_state(state, two)
            {
                moves.push(MoveTarget::at(two));
                if state.flag("pawnSprint", piece.color)
                    && let Some(three) = from.offset(forward * 3, dc * 3)
                    && movement_target(three).is_none()
                    && !crown_ground_from_state(state, three)
                {
                    moves.push(MoveTarget::at(three));
                }
            }
        }
        if crown_ground_from_state(state, one) {
            let mut target = MoveTarget::at(one);
            target
                .flags
                .insert("crownGroundCapture".into(), json!(true));
            moves.push(target);
        } else if movement_target(one).is_some_and(|target| can_capture(state, piece, target)) {
            moves.push(MoveTarget::at(one));
        }
    }
    moves
}

fn crown_ground_from_state(state: &GameState, square: Square) -> bool {
    let crown = state.extra.get("crownRule").unwrap_or(&Value::Null);
    let entries = crown
        .get("crowns")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty());
    let matches = |entry: &Value| {
        if entry == &Value::Bool(true) {
            return square == (Square { row: 3, col: 3 });
        }
        crate::observation::truth(Some(entry))
            && !crate::observation::truth(entry.get("removed"))
            && entry.get("ground").is_some_and(|ground| {
                ground.get("row").and_then(Value::as_f64) == Some(f64::from(square.row))
                    && ground.get("col").and_then(Value::as_f64) == Some(f64::from(square.col))
            })
    };
    entries.is_some_and(|items| items.iter().any(matches)) || entries.is_none() && matches(crown)
}
fn v7_mark_crown_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    moves: Vec<MoveTarget>,
) -> Vec<MoveTarget> {
    moves
        .into_iter()
        .filter_map(|mut target| {
            let destination = v7_portal_capture_cells(&target)
                .last()
                .copied()
                .unwrap_or(target.square());
            if crown_ground_from_state(state, destination) {
                if piece.kind == "pawn"
                    && (destination.col != from.col
                        || i16::from(destination.row)
                            != i16::from(from.row) + i16::from(piece.color.owner()?.pawn_dir()))
                {
                    return None;
                }
                target
                    .flags
                    .insert("crownGroundCapture".into(), json!(true));
            }
            Some(target)
        })
        .collect()
}

pub(crate) fn missionary(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    DIAG.iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .filter(|&to| {
            !collapsed(state, to)
                && state.at(to).is_none_or(|target| {
                    target.color != piece.color
                        && target.color.owner().is_some()
                        && !matches!(
                            target.kind.as_str(),
                            "wall" | "football" | "monster" | "blackHole"
                        )
                        && !desperado_royal_capture_blocked(piece, target)
                })
        })
        .map(|to| {
            let mut target = MoveTarget::at(to);
            if state.at(to).is_some() {
                target.flags.insert("missionaryConvert".into(), json!(true));
            }
            target
        })
        .collect()
}

/// Raw Fanatic source kernel: occupied first step ends the route; an empty
/// first step allows the second step. Terrain filtering belongs to its caller.
pub(crate) fn v7_raw_fanatic_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id != RULES_VERSION_V7 || from.row >= 8 || from.col >= 8 {
        return Err(EngineError::InvalidState(
            "v7 Fanatic origin outside 8x8".into(),
        ));
    }
    let actor = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let Some(first) = from.offset(actor.pawn_dir(), 0) else {
        return Ok(Vec::new());
    };
    if let Some(target) = state.at(first) {
        return Ok(
            if v7_can_capture_target(state, piece, target, false, false)? {
                vec![MoveTarget::at(first)]
            } else {
                Vec::new()
            },
        );
    }
    let mut moves = vec![MoveTarget::at(first)];
    if let Some(exit) = v7_portal_exit_at(state, first) {
        if let Some(second) = exit.offset(actor.pawn_dir(), 0)
            && state.at(exit).is_none()
            && !collapsed(state, first)
            && !collapsed(state, exit)
            && !collapsed(state, second)
            && (state.at(second).is_some() || !v7_scarecrow_reserved_square(state, second))
            && state
                .at(second)
                .is_none_or(|victim| can_capture(state, piece, victim))
        {
            let mut target = MoveTarget::at(second);
            target.flags.insert("portalThrough".into(), json!(true));
            target.flags.insert("portalEntry".into(), json!(first));
            target.flags.insert("portalExit".into(), json!(exit));
            moves.push(target);
        }
        return Ok(moves);
    }
    if let Some(second) = from.offset(actor.pawn_dir() * 2, 0)
        && state
            .at(second)
            .is_none_or(|target| can_capture(state, piece, target))
    {
        moves.push(MoveTarget::at(second));
    }
    Ok(moves)
}

/// Source thiefJumpedPiece is a boolean probe of physical intermediate
/// occupants. It intentionally does not use movement transparency.
pub(crate) fn v7_thief_jumped_piece(state: &GameState, from: Square, to: Square) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7
        || from.row >= 8
        || from.col >= 8
        || to.row >= 8
        || to.col >= 8
    {
        return Err(EngineError::InvalidState(
            "v7 Thief jump probe outside 8x8".into(),
        ));
    }
    let dr = i16::from(to.row) - i16::from(from.row);
    let dc = i16::from(to.col) - i16::from(from.col);
    let distance = dr.abs().max(dc.abs());
    if (dr == 0) == (dc == 0) || distance > 3 {
        return Ok(false);
    }
    Ok((1..distance).any(|step| {
        from.offset((dr.signum() * step) as i8, (dc.signum() * step) as i8)
            .is_some_and(|cell| state.at(cell).is_some())
    }))
}

fn v7_standard_castling(
    state: &GameState,
    king: &Piece,
    from: Square,
    options: V7MoveOptions,
) -> Result<Vec<MoveTarget>> {
    let Some(actor) = king.color.owner() else {
        return Ok(Vec::new());
    };
    let authoritative = state
        .extra
        .get("castlingMoved")
        .and_then(|sides| sides.get(actor.as_str()));
    if state.flag("castlingCanceled", actor)
        || king.moved
        || crate::observation::truth(authoritative.and_then(|side| side.get("king")))
        || from.row != actor.home_row()
        || from.col != 4
    {
        return Ok(Vec::new());
    }
    let ignored = if options.fog_visibility_probe {
        None
    } else {
        Some(crate::observation::v7_hidden_opponent_piece_ids(
            state, actor,
        )?)
    };
    if crate::v7_threat::is_square_attacked_v7(
        state,
        from,
        actor.opponent(),
        None,
        ignored.as_ref(),
    )? || state
        .extra
        .get("delayedHazards")
        .and_then(Value::as_array)
        .is_some_and(|hazards| {
            hazards.iter().any(|hazard| {
                hazard.get("owner").and_then(Value::as_str) == Some(actor.opponent().as_str())
                    && hazard.get("triggerAfter").and_then(Value::as_str) == Some(actor.as_str())
                    && hazard
                        .get("cells")
                        .and_then(Value::as_array)
                        .is_some_and(|cells| {
                            cells
                                .iter()
                                .any(|cell| v7_descriptor_square(cell) == Some(from))
                        })
            })
        })
    {
        return Ok(Vec::new());
    }
    let mut moves = Vec::new();
    for rook_col in [7, 0] {
        let direction = if rook_col > from.col { 1 } else { -1 };
        let Some(king_to) = from.offset(0, 2 * direction) else {
            continue;
        };
        let Some(rook_step) = from.offset(0, direction) else {
            continue;
        };
        let rook_key = if direction > 0 {
            "kingSideRook"
        } else {
            "queenSideRook"
        };
        if crate::observation::truth(authoritative.and_then(|side| side.get(rook_key))) {
            continue;
        }
        let rook_cell = Square {
            row: actor.home_row(),
            col: rook_col,
        };
        let Some(rook) = state.at(rook_cell) else {
            continue;
        };
        if rook.color != king.color
            || !matches!(rook.kind.as_str(), "rook" | "bigRook")
            || rook.moved
        {
            continue;
        }
        if ((from.col.min(rook_col) + 1)..from.col.max(rook_col)).any(|col| {
            state
                .at(Square { row: from.row, col })
                .is_some_and(|blocker| blocker.id != rook.id)
        }) {
            continue;
        }
        if crate::v7_threat::is_square_attacked_v7(
            state,
            rook_step,
            actor.opponent(),
            Some(king),
            ignored.as_ref(),
        )? || crate::v7_threat::is_square_attacked_v7(
            state,
            king_to,
            actor.opponent(),
            Some(king),
            ignored.as_ref(),
        )? {
            continue;
        }
        let big_rook = rook.kind == "bigRook";
        let rook_to = if big_rook {
            Square {
                row: if actor == Color::White {
                    from.row - 1
                } else {
                    from.row
                },
                col: if king_to.col < 4 {
                    king_to.col + 1
                } else {
                    king_to.col - 2
                },
            }
        } else {
            rook_step
        };
        if crate::observation::truth(state.extra.get("monochromeChess"))
            && ((from.row + from.col) % 2 != (king_to.row + king_to.col) % 2
                || (rook_cell.row + rook_cell.col) % 2 != (rook_to.row + rook_to.col) % 2)
        {
            continue;
        }
        if big_rook
            && (rook_to.row >= 7
                || rook_to.col >= 7
                || (rook_to.row..rook_to.row + 2).any(|row| {
                    (rook_to.col..rook_to.col + 2).any(|col| {
                        state.at(Square { row, col }).is_some_and(|occupant| {
                            occupant.id != rook.id
                                && occupant.id != king.id
                                && occupant.color != king.color
                        })
                    })
                }))
        {
            continue;
        }
        let rook_from = if big_rook {
            v7_normalize_origin(rook, rook_cell)?
        } else {
            rook_cell
        };
        let mut target = MoveTarget::at(king_to);
        target.flags.insert(
            "castle".into(),
            json!(if direction > 0 {
                "킹사이드"
            } else {
                "퀸사이드"
            }),
        );
        target.flags.insert("rookFrom".into(), json!(rook_from));
        target.flags.insert("rookTo".into(), json!(rook_to));
        target.flags.insert("bigRookCastle".into(), json!(big_rook));
        moves.push(target);
    }
    Ok(moves)
}

fn v7_free_castling(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    if state.flag("castlingCanceled", piece.color) {
        return Vec::new();
    }
    let mut moves = Vec::new();
    for &(dr, dc) in DIAG.iter().chain(ORTHO.iter()) {
        for distance in 3..=7 {
            let Some(rook_at) = from.offset(dr * distance, dc * distance) else {
                break;
            };
            if !state
                .at(rook_at)
                .is_some_and(|rook| rook.kind == "rook" && rook.color == piece.color)
            {
                continue;
            }
            let Some(to) = from.offset(dr * 2, dc * 2) else {
                continue;
            };
            let Some(rook_to) = from.offset(dr, dc) else {
                continue;
            };
            let mut target = MoveTarget::at(to);
            target.flags.insert("castle".into(), json!("카드 캐슬링"));
            target.flags.insert("freeCastling".into(), json!(true));
            target.flags.insert("rookFrom".into(), json!(rook_at));
            target.flags.insert("rookTo".into(), json!(rook_to));
            moves.push(target);
        }
    }
    moves
}

fn v7_switcheroo_moves(state: &GameState, piece: &Piece) -> Vec<MoveTarget> {
    if !state.flag("switcheroo", piece.color) || !v7_king_augment_recipient(state, piece) {
        return Vec::new();
    }
    let mut moves = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            if state
                .at(Square { row, col })
                .is_some_and(|target| target.kind == "pawn" && target.color == piece.color)
            {
                let mut target = MoveTarget::at(Square { row, col });
                target.flags.insert("switcherooMove".into(), json!(true));
                moves.push(target);
            }
        }
    }
    moves
}

pub(crate) fn v7_can_resolve_switcheroo_move(
    state: &GameState,
    moving: &Piece,
    target: &Piece,
) -> bool {
    state.flag("switcheroo", moving.color)
        && moving.kind == "king"
        && !(moving.flag("undergroundBunker")
            && crate::card_effects::js_number(moving.extra.get("hp"), 0)
                .is_some_and(f64::is_finite))
        && target.color == moving.color
        && target.kind == "pawn"
}

pub(crate) fn v7_can_resolve_siege_ram_move(piece: &Piece, target: &MoveTarget) -> bool {
    crate::variant_movement::v7_can_resolve_siege_ram_move(piece, target)
}

pub(crate) fn v7_highway_capture_allowed(attacker: &Piece, target: &Piece) -> bool {
    !matches!(
        attacker.kind.as_str(),
        "guard"
            | "recruiter"
            | "wizard"
            | "herald"
            | "merchant"
            | "coffin"
            | "scarecrow"
            | "blackHole"
            | "timeAfterimage"
    ) && (attacker.kind != "jester"
        || matches!(
            target.kind.as_str(),
            "king" | "royalKnight" | "shotgunKing" | "merchant"
        ))
}

pub(crate) fn v7_checker_capture_target_allowed(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
    at: Square,
) -> Result<bool> {
    Ok(
        v7_can_capture_target(state, attacker, target, false, false)?
            && !v7_encouraged_at(state, target, at),
    )
}

pub(crate) fn v7_radical_charge_capture_target_allowed(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
    at: Square,
) -> Result<bool> {
    Ok(!v7_manner_capture_locked(state, attacker)
        && v7_can_capture_target(state, attacker, target, false, false)?
        && target.ability_kind() != "jester"
        && !v7_encouraged_at(state, target, at))
}

fn castling(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let Some(actor) = piece.color.owner() else {
        return Vec::new();
    };
    if piece.kind != "king"
        || piece.moved
        || from.row != actor.home_row()
        || state.flag("castlingCanceled", piece.color)
    {
        return Vec::new();
    }
    let mut moves = Vec::new();
    for rook_col in [0, 7] {
        let rook_square = Square {
            row: from.row,
            col: rook_col,
        };
        let Some(rook) = state.at(rook_square) else {
            continue;
        };
        if rook.color != piece.color || rook.kind != "rook" || rook.moved {
            continue;
        }
        let dc = if rook_col < from.col { -1 } else { 1 };
        let Some(to) = from.offset(0, 2 * dc) else {
            continue;
        };
        let Some(rook_to) = from.offset(0, dc) else {
            continue;
        };
        let low = rook_col.min(from.col) + 1;
        let high = rook_col.max(from.col);
        if (low..high).any(|col| {
            state.at(Square { row: from.row, col }).is_some()
                || collapsed(state, Square { row: from.row, col })
        }) {
            continue;
        }
        let mut target = MoveTarget::at(to);
        target.flags.insert(
            "castle".into(),
            json!(if dc > 0 {
                "킹사이드"
            } else {
                "퀸사이드"
            }),
        );
        target.flags.insert("rookFrom".into(), json!(rook_square));
        target.flags.insert("rookTo".into(), json!(rook_to));
        target.flags.insert("bigRookCastle".into(), json!(false));
        moves.push(target);
    }
    moves
}
pub(crate) fn cannon(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    if state.ruleset_id == RULES_VERSION_V7 {
        return v7_cannon_path_moves(state, piece, from);
    }
    let mut moves = Vec::new();
    for &(dr, dc) in ORTHO {
        let mut cursor = from;
        let mut screen = false;
        while let Some(to) = cursor.offset(dr, dc) {
            cursor = to;
            if collapsed(state, to) {
                break;
            }
            if let Some(target) = state.at(to) {
                if !screen {
                    if target.kind == "cannon" {
                        break;
                    }
                    screen = true;
                } else {
                    if target.kind != "cannon" && can_capture(state, piece, target) {
                        moves.push(MoveTarget::at(to));
                    }
                    break;
                }
            } else if screen {
                moves.push(MoveTarget::at(to));
            }
        }
    }
    moves
}

pub(crate) fn v7_raw_cannon_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id != RULES_VERSION_V7 || from.row >= 8 || from.col >= 8 {
        return Err(EngineError::InvalidState(
            "v7 Cannon origin outside 8x8".into(),
        ));
    }
    ensure_v7_capture_policy_scope(state)?;
    if piece.color.owner().is_some() {
        crate::card_constraints::CaptureConstraints::from_source_state(state, piece)?;
    }
    Ok(v7_cannon_path_moves(state, piece, from))
}

/// cannonPathMoves: 받침 앞·뒤의 상태는 포털을 통과해도 유지된다.
/// 일반 착지의 지형 필터는 호출자가 적용하고 포털 입구·출구만 여기서 검사한다.
fn v7_cannon_path_moves(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    v7_cannon_kernel(state, piece, from, true, false, true)
}

pub(crate) fn v7_memory_cannon_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    geometry: bool,
    portal: bool,
) -> Vec<MoveTarget> {
    v7_cannon_kernel(state, piece, from, false, geometry, portal)
}

fn v7_cannon_kernel(
    state: &GameState,
    piece: &Piece,
    from: Square,
    physical: bool,
    geometry: bool,
    portal: bool,
) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    let occupant = |at| {
        state
            .at(at)
            .filter(|target| !physical || !v7_stealth_transparent(state, piece, target, at))
    };
    for &(dr, dc) in ORTHO {
        let mut next = from.offset(dr, dc);
        let mut jumped = false;
        let mut transit: Option<(Square, Square)> = None;
        for _ in 0..16 {
            let Some(at) = next else {
                break;
            };
            let target = occupant(at);
            let transparent = physical
                && target.is_some_and(|target| {
                    v7_ghost_transparent_for(state, piece, target, at, "cannon")
                });
            let exit = if portal {
                v7_portal_exit_at(state, at)
            } else {
                None
            };
            if exit.is_some() && transit.is_some() {
                break;
            }
            let mut descriptor = MoveTarget::at(at);
            if let Some((entry, exit)) = transit {
                descriptor.flags.insert("portalThrough".into(), json!(true));
                descriptor.flags.insert("portalEntry".into(), json!(entry));
                descriptor.flags.insert("portalExit".into(), json!(exit));
            }
            if let Some(target) = target.filter(|_| !transparent) {
                if !jumped {
                    if target.kind == "cannon" || !v7_cannon_screen_allowed(state, target) {
                        break;
                    }
                    jumped = true;
                    next = at.offset(dr, dc);
                    continue;
                }
                let allowed = if geometry {
                    target.color != piece.color
                        && !matches!(target.kind.as_str(), "wall" | "football" | "blackHole")
                } else {
                    can_capture(state, piece, target)
                };
                if target.kind != "cannon" && allowed {
                    moves.push(descriptor);
                }
                break;
            }
            if jumped && target.is_none() {
                moves.push(descriptor);
            }
            if jumped && target.is_some() && transparent && exit.is_some() {
                let mut entry = MoveTarget::at(at);
                entry
                    .flags
                    .insert("portalTransparentEntry".into(), json!(true));
                moves.push(entry);
            }
            if let Some(exit) = exit {
                if (physical || !geometry) && (collapsed(state, at) || collapsed(state, exit))
                    || occupant(exit).is_some_and(|target| {
                        !physical || !v7_ghost_transparent_for(state, piece, target, exit, "cannon")
                    })
                {
                    break;
                }
                transit = Some((at, exit));
                next = exit.offset(dr, dc);
            } else {
                next = at.offset(dr, dc);
            }
        }
    }
    moves
}

fn v7_cannon_screen_allowed(state: &GameState, target: &Piece) -> bool {
    let hash = v7_effective_catalog_hash(state);
    let unified = hash.is_some()
        || state.extra.get("unifiedJumpObstacles") == Some(&json!(true))
        || !state.extra.contains_key("unifiedJumpObstacles")
            && !state.extra.contains_key("cannonGhostScreen");
    if unified {
        return target.kind != "cannon";
    }
    let scarecrow =
        hash.is_some() || state.extra.get("cannonScarecrowScreen") != Some(&json!(false));
    let ghost_screen =
        hash.is_some() || state.extra.get("cannonGhostScreen") != Some(&json!(false));
    (target.kind == "wall"
        || target.kind == "scarecrow" && scarecrow
        || !crate::observation::truth(target.extra.get("installationId")))
        && target.kind != "football"
        && (target.kind != "cannon" || !ghost_screen)
        && (target.kind != "scarecrow" || scarecrow)
        && (target.kind != "monster"
            || crate::observation::truth(target.extra.get("blackMagicMonster"))
            || crate::observation::truth(target.extra.get("blackMagicOwner")))
}
pub(crate) fn grasshopper(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(dr, dc) in DIAG.iter().chain(ORTHO.iter()) {
        let mut cursor = from;
        while let Some(to) = cursor.offset(dr, dc) {
            cursor = to;
            if state.ruleset_id != RULES_VERSION_V7 && collapsed(state, to) {
                break;
            }
            if state.at(to).is_some() {
                if let Some(landing_square) = to.offset(dr, dc)
                    && (if state.ruleset_id == RULES_VERSION_V7 {
                        state
                            .at(landing_square)
                            .is_none_or(|victim| can_capture(state, piece, victim))
                    } else {
                        landing(state, piece, landing_square)
                    })
                {
                    moves.push(MoveTarget::at(landing_square));
                }
                break;
            }
        }
    }
    moves
}

pub(crate) fn v7_raw_grasshopper_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id != RULES_VERSION_V7 || from.row >= 8 || from.col >= 8 {
        return Err(EngineError::InvalidState(
            "v7 Grasshopper origin outside 8x8".into(),
        ));
    }
    Ok(grasshopper(state, piece, from))
}
pub(crate) fn checker(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    if state.ruleset_id == RULES_VERSION_V7 {
        return v7_checker_raw_moves(state, piece, from);
    }
    let Some(actor) = piece.color.owner() else {
        return Vec::new();
    };
    let dir = actor.pawn_dir();
    let dirs = if piece.kind == "checkerKing" {
        DIAG.to_vec()
    } else {
        vec![(dir, -1), (dir, 1)]
    };
    let mut moves = Vec::new();
    for (dr, dc) in dirs {
        if let Some(adjacent) = from.offset(dr, dc) {
            if state.at(adjacent).is_none() && !collapsed(state, adjacent) {
                if !piece.flag("checkerChainCapture") {
                    moves.push(MoveTarget::at(adjacent));
                }
            } else if state.at(adjacent).is_some_and(|target| {
                can_capture(state, piece, target)
                    && (state.ruleset_id != RULES_VERSION_V7 || !encouraged(state, target))
            }) && let Some(to) = adjacent.offset(dr, dc)
                && state.at(to).is_none()
                && !collapsed(state, to)
            {
                let mut target = MoveTarget::at(to);
                target.flags.insert("checkerCapture".into(), json!(true));
                target.flags.insert("jumpCapture".into(), json!(adjacent));
                moves.push(target);
            }
        }
    }
    let capture = moves.iter().any(|m| m.flag("checkerCapture"));
    if capture {
        moves.retain(|m| m.flag("checkerCapture"));
    }
    moves
}

fn v7_checker_raw_moves(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    v7_checker_moves_for_type(state, piece, from, &piece.kind)
}

pub(crate) fn v7_checker_moves_for_type(
    state: &GameState,
    piece: &Piece,
    from: Square,
    kind: &str,
) -> Vec<MoveTarget> {
    let Some(actor) = piece.color.owner() else {
        return Vec::new();
    };
    let directions = if kind == "checkerKing" {
        DIAG.to_vec()
    } else {
        vec![(actor.pawn_dir(), -1), (actor.pawn_dir(), 1)]
    };
    let occupant = |at| {
        state
            .at(at)
            .filter(|victim| !v7_stealth_transparent(state, piece, victim, at))
    };
    let mut moves = Vec::new();
    for &(dr, dc) in &directions {
        if let Some(at) = from.offset(dr, dc).filter(|&at| occupant(at).is_none()) {
            moves.push(MoveTarget::at(at));
        }
    }
    for &(dr, dc) in &directions {
        let Some(mid) = from.offset(dr, dc) else {
            continue;
        };
        let Some(landing) = mid.offset(dr, dc).filter(|&at| occupant(at).is_none()) else {
            continue;
        };
        if occupant(mid).is_some_and(|victim| {
            can_capture(state, piece, victim) && !v7_encouraged_at(state, victim, mid)
        }) {
            let mut target = MoveTarget::at(landing);
            target.flags.insert("jumpCapture".into(), json!(mid));
            target.flags.insert("checkerCapture".into(), json!(true));
            moves.push(target);
        }
    }
    unique_v7_coordinates(moves)
}
/// Source bigRookMoves in v7 emits a stationary body first, then each ray's
/// anchors with row-major 2x2 cells. Landing captures may include allies.
/// The v6 large-rules reader below remains independent.
/// The shared source colossusLandingCapturesForCells contract. A blocked
/// footprint is None; a clear footprint is Some, including an empty list.
pub(crate) fn v7_large_landing_captures(
    state: &GameState,
    piece: &Piece,
    cells: &[Square],
    capture_limit: usize,
    allow_friendly: bool,
) -> Result<Option<Vec<Square>>> {
    if state.ruleset_id != RULES_VERSION_V7
        || cells.iter().any(|cell| cell.row >= 8 || cell.col >= 8)
    {
        return Err(EngineError::InvalidState(
            "v7 large capture footprint outside 8x8".into(),
        ));
    }
    ensure_v7_capture_policy_scope(state)?;
    let saturated = crate::v7_capture_reactions::saturation_locked(state, piece);
    let limit = if saturated { 0 } else { capture_limit };
    let mut captures = Vec::new();
    let mut seen = BTreeSet::new();
    for &cell in cells {
        let Some(victim) = state.at(cell) else {
            continue;
        };
        if victim.id == piece.id {
            continue;
        }
        if v7_encouraged_at(state, victim, cell)
            || !v7_can_capture_target(state, piece, victim, allow_friendly, false)?
        {
            return Ok(None);
        }
        let identity = if victim.id.is_empty() {
            format!("{},{}", cell.row, cell.col)
        } else {
            victim.id.clone()
        };
        if seen.insert(identity) {
            captures.push(cell);
            if captures.len() > limit {
                return Ok(None);
            }
        }
    }
    Ok(Some(captures))
}

#[derive(Clone, Copy)]
enum V7LargeModifier {
    Socialism,
    Crown,
    PromotionRush,
}

fn v7_full_large_cells(anchor: Square) -> Option<[Square; 4]> {
    (anchor.row < 7 && anchor.col < 7).then(|| {
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
    })
}

fn v7_colossus_display_cells(from: Square, dr: i8, dc: i8) -> Vec<Square> {
    let row = i16::from(from.row) + if dr < 0 { -1 } else { 2 };
    let col = i16::from(from.col) + if dc < 0 { -1 } else { 2 };
    let raw = if dr != 0 && dc != 0 {
        vec![(row, col)]
    } else if dr != 0 {
        vec![(row, i16::from(from.col)), (row, i16::from(from.col) + 1)]
    } else {
        vec![(i16::from(from.row), col), (i16::from(from.row) + 1, col)]
    };
    raw.into_iter()
        .filter(|&(r, c)| (0..8).contains(&r) && (0..8).contains(&c))
        .map(|(row, col)| Square {
            row: row as u8,
            col: col as u8,
        })
        .collect()
}

fn v7_large_modifier_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    modifier: V7LargeModifier,
) -> Result<Vec<MoveTarget>> {
    let actor = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let big = matches!(piece.kind.as_str(), "bigRook" | "bigBishop");
    let capture_limit = if piece.kind == "bigBishop" {
        3
    } else if piece.kind == "bigRook" {
        2
    } else {
        usize::MAX
    };
    let directions = match modifier {
        V7LargeModifier::Socialism => vec![
            (actor.pawn_dir(), 0),
            (actor.pawn_dir(), -1),
            (actor.pawn_dir(), 1),
        ],
        _ => DIAG.iter().chain(ORTHO.iter()).copied().collect(),
    };
    let mut moves = Vec::new();
    for (dr, dc) in directions {
        let mut anchor = from;
        for _ in 0..7 {
            let Some(next) = anchor.offset(dr, dc) else {
                break;
            };
            anchor = next;
            let Some(cells) = v7_full_large_cells(anchor) else {
                break;
            };
            let rush = matches!(modifier, V7LargeModifier::PromotionRush);
            if rush
                && cells
                    .iter()
                    .any(|&at| state.at(at).is_some_and(|p| p.id != piece.id))
            {
                break;
            }
            let captures = if rush {
                Vec::new()
            } else {
                if matches!(modifier, V7LargeModifier::Socialism) && dc != 0 {
                    let attack_row = i16::from(from.row) + if dr < 0 { -1 } else { 2 };
                    let attack_col = i16::from(from.col) + if dc < 0 { -1 } else { 2 };
                    if !(0..8).contains(&attack_row) || !(0..8).contains(&attack_col) {
                        break;
                    }
                    let attack = Square {
                        row: attack_row as u8,
                        col: attack_col as u8,
                    };
                    if !state
                        .at(attack)
                        .is_some_and(|p| p.id != piece.id && can_capture(state, piece, p))
                    {
                        break;
                    }
                }
                let Some(captures) =
                    v7_large_landing_captures(state, piece, &cells, capture_limit, false)?
                else {
                    break;
                };
                if matches!(modifier, V7LargeModifier::Socialism) && big {
                    if dc == 0 && !captures.is_empty() {
                        break;
                    }
                    if dc != 0 {
                        let attack = Square {
                            row: (i16::from(from.row) + if dr < 0 { -1 } else { 2 }) as u8,
                            col: (i16::from(from.col) + if dc < 0 { -1 } else { 2 }) as u8,
                        };
                        if !captures.contains(&attack) {
                            break;
                        }
                    }
                }
                captures
            };
            let mut target = MoveTarget::at(anchor);
            target.flags.insert("anchorRow".into(), json!(anchor.row));
            target.flags.insert("anchorCol".into(), json!(anchor.col));
            target.flags.insert("highlightCells".into(), json!(cells));
            target.flags.insert(
                if big { "bigRookMove" } else { "colossusMove" }.into(),
                json!(true),
            );
            target.flags.insert(
                if big {
                    "bigRookLandingCaptures"
                } else {
                    "colossusLandingCaptures"
                }
                .into(),
                json!(captures),
            );
            if rush {
                target.flags.insert("promotionRushMove".into(), json!(true));
            } else if !big || matches!(modifier, V7LargeModifier::Socialism) && dc != 0 {
                target.flags.insert(
                    "displayCells".into(),
                    json!(v7_colossus_display_cells(from, dr, dc)),
                );
            }
            moves.push(target);
            if !rush {
                break;
            }
        }
    }
    Ok(unique_v7_coordinates(moves))
}

pub(crate) fn v7_large_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    if !matches!(piece.kind.as_str(), "bigRook" | "bigBishop") || from.row >= 7 || from.col >= 7 {
        return Err(EngineError::InvalidState(
            "v7 large movement requires an in-bounds bigRook or bigBishop anchor".into(),
        ));
    }
    let cells_at = |anchor: Square| -> Option<[Square; 4]> {
        if anchor.row >= 7 || anchor.col >= 7 {
            return None;
        }
        Some([
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
        ])
    };
    let body = cells_at(from).expect("checked large-piece anchor");
    if body
        .iter()
        .any(|&cell| state.at(cell).is_none_or(|part| part.id != piece.id))
    {
        return Err(EngineError::InvalidState(
            "v7 large movement requires a complete 2x2 identity footprint".into(),
        ));
    }
    let mut stationary = MoveTarget::at(from);
    stationary.flags.insert("bodyCells".into(), json!(body));
    stationary.flags.insert("colossusBody".into(), json!(true));
    let mut moves = vec![stationary];
    let directions = if piece.kind == "bigBishop" {
        DIAG
    } else {
        ORTHO
    };
    let capture_limit = if piece.kind == "bigBishop" { 3 } else { 2 };
    for &(dr, dc) in directions {
        let mut anchor = from;
        while let Some(next) = anchor.offset(dr, dc) {
            anchor = next;
            let Some(cells) = cells_at(anchor) else {
                break;
            };
            let occupants = cells
                .iter()
                .filter_map(|&at| state.at(at).filter(|p| p.id != piece.id).map(|p| (at, p)))
                .collect::<Vec<_>>();
            let transparent = occupants
                .iter()
                .filter(|&&(at, p)| v7_ghost_transparent_for(state, piece, p, at, &piece.kind))
                .count();
            if transparent > 0 {
                if transparent == occupants.len() {
                    continue;
                }
                break;
            }
            let Some(captures) =
                v7_large_landing_captures(state, piece, &cells, capture_limit, true)?
            else {
                break;
            };
            if cells.iter().all(|&cell| !collapsed(state, cell)) {
                let mut target = MoveTarget::at(anchor);
                target.flags.insert("anchorRow".into(), json!(anchor.row));
                target.flags.insert("anchorCol".into(), json!(anchor.col));
                target.flags.insert("highlightCells".into(), json!(cells));
                target
                    .flags
                    .insert("bigRookLandingCaptures".into(), json!(captures));
                target.flags.insert("bigRookMove".into(), json!(true));
                moves.push(target);
            }
            if !captures.is_empty() {
                break;
            }
        }
    }
    Ok(moves)
}

fn large_rays(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let dirs = if piece.kind == "bigBishop" {
        DIAG
    } else {
        ORTHO
    };
    let mut moves = Vec::new();
    for &(dr, dc) in dirs {
        let mut cursor = from;
        while let Some(anchor) = cursor.offset(dr, dc) {
            cursor = anchor;
            if anchor.row > 6 || anchor.col > 6 {
                break;
            }
            let cells = [
                anchor,
                Square {
                    row: anchor.row + 1,
                    col: anchor.col,
                },
                Square {
                    row: anchor.row,
                    col: anchor.col + 1,
                },
                Square {
                    row: anchor.row + 1,
                    col: anchor.col + 1,
                },
            ];
            if cells.iter().any(|&cell| {
                collapsed(state, cell)
                    || state.at(cell).is_some_and(|target| {
                        target.id != piece.id && !can_capture(state, piece, target)
                    })
            }) {
                break;
            }
            let occupied = cells
                .iter()
                .any(|&cell| state.at(cell).is_some_and(|target| target.id != piece.id));
            let captures: Vec<Square> = cells
                .iter()
                .copied()
                .filter(|&cell| state.at(cell).is_some_and(|target| target.id != piece.id))
                .collect();
            let mut target = MoveTarget::at(anchor);
            target.flags.insert("bigRookMove".into(), json!(true));
            target.flags.insert("anchorRow".into(), json!(anchor.row));
            target.flags.insert("anchorCol".into(), json!(anchor.col));
            target.flags.insert("highlightCells".into(), json!(cells));
            target
                .flags
                .insert("bigRookLandingCaptures".into(), json!(captures));
            moves.push(target);
            if occupied {
                break;
            }
        }
    }
    moves
}

#[cfg(test)]
mod v7_movement_tests {
    use super::*;

    #[test]
    #[ignore = "로컬 8×8 장기 조회는 별도 외부 원문 연구 영수증으로 비교한다"]
    fn frozen_v7_local_janggi_move_capture_and_attack_queries_match_source() {
        let path = std::env::var("ACCELERATE_V7_JANGGI_QUERY_RECEIPTS").unwrap();
        let input = std::fs::read_to_string(path).unwrap();
        let mut examined = 0;
        let mut failures = Vec::new();
        for line in input.lines().filter(|line| !line.trim().is_empty()) {
            let receipt: Value = serde_json::from_str(line).unwrap();
            let name = receipt["name"].as_str().unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            examined += 1;
            let checked = (|| -> Result<Vec<String>> {
                let host = crate::V7HostPosition::from_envelope(receipt["before"].clone())?;
                let state = host.state();
                if state
                    .extra
                    .get("campaign")
                    .and_then(|campaign| campaign.get("setup"))
                    .and_then(Value::as_str)
                    != Some("janggi")
                {
                    return Err(EngineError::InvalidState(
                        "local Janggi receipt requires campaign.setup=janggi".into(),
                    ));
                }
                let before = serde_jcs::to_vec(state).map_err(EngineError::serialization)?;
                let actor: Color = serde_json::from_value(receipt["actor"].clone())
                    .map_err(EngineError::serialization)?;
                let from =
                    v7_descriptor_square(&receipt["from"]).ok_or(EngineError::IllegalAction)?;
                let piece = state.at(from).ok_or(EngineError::IllegalAction)?;
                let mut mismatches = Vec::new();
                let moves = serde_json::to_value(v7_legal_move_targets(
                    state,
                    piece,
                    from,
                    V7MoveOptions::default(),
                )?)
                .map_err(EngineError::serialization)?;
                let hints: Vec<_> = v7_ui_move_hints(state, actor)?
                    .into_iter()
                    .map(|(from, destinations)| json!({"from":from,"destinations":destinations}))
                    .collect();
                for (label, actual, expected) in [
                    ("moves", moves, &receipt["moves"]),
                    ("hints", json!(hints), &receipt["hints"]),
                ] {
                    if &actual == expected {
                        continue;
                    }
                    let actual = actual.as_array().unwrap();
                    let expected = expected.as_array().unwrap();
                    let first = actual
                        .iter()
                        .zip(expected)
                        .position(|(a, b)| a != b)
                        .unwrap_or(actual.len().min(expected.len()));
                    mismatches.push(format!(
                        "{name} {label}: actual {} expected {} first {first} actual={} expected={}",
                        actual.len(),
                        expected.len(),
                        actual.get(first).unwrap_or(&Value::Null),
                        expected.get(first).unwrap_or(&Value::Null)
                    ));
                }
                if state.royal_identity(piece) != receipt["royalIdentity"].as_bool().unwrap() {
                    mismatches.push(format!(
                        "{name} royal identity: actual={} expected={}",
                        state.royal_identity(piece),
                        receipt["royalIdentity"]
                    ));
                }
                let attacks = receipt["attacks"].as_array().unwrap();
                assert_eq!(attacks.len(), 64, "Janggi attack probe count for {name}");
                for probe in attacks {
                    let to =
                        v7_descriptor_square(&probe["to"]).ok_or(EngineError::IllegalAction)?;
                    let actual = crate::v7_threat::piece_attacks_square_v7(state, piece, from, to)?;
                    let expected = probe["expected"].as_bool().unwrap();
                    if actual != expected {
                        mismatches.push(format!(
                            "{name} attack {to:?}: actual={actual} expected={expected}"
                        ));
                    }
                }
                let captures = receipt["captures"].as_array().unwrap();
                assert!(
                    captures.len() <= 64,
                    "Janggi capture probe count for {name}"
                );
                for probe in captures {
                    let to =
                        v7_descriptor_square(&probe["to"]).ok_or(EngineError::IllegalAction)?;
                    let target = state.at(to).ok_or(EngineError::IllegalAction)?;
                    let actual = v7_can_capture_target(state, piece, target, false, false)?;
                    let expected = probe["expected"].as_bool().unwrap();
                    if actual != expected {
                        mismatches.push(format!(
                            "{name} capture {to:?}: actual={actual} expected={expected}"
                        ));
                    }
                }
                assert_eq!(
                    serde_jcs::to_vec(state).unwrap(),
                    before,
                    "query mutated {name}"
                );
                Ok(mismatches)
            })();
            match checked {
                Ok(mismatches) => failures.extend(mismatches),
                Err(error) => failures.push(format!("{name}: {error}")),
            }
        }
        assert_eq!(examined, 42, "local Janggi receipt count");
        assert!(
            failures.is_empty(),
            "{} local Janggi query mismatches of {examined}:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    #[ignore = "공격자 없는 포획·Alibaba 배치는 외부 원문 연구 영수증으로 비교한다"]
    fn frozen_v7_nominal_capture_and_placement_queries_match_source() {
        let path = std::env::var("ACCELERATE_V7_NOMINAL_PLACEMENT_RECEIPTS").unwrap();
        let input = std::fs::read_to_string(path).unwrap();
        let mut examined = 0;
        let mut failures = Vec::new();
        for line in input.lines().filter(|line| !line.trim().is_empty()) {
            let receipt: Value = serde_json::from_str(line).unwrap();
            let name = receipt["name"].as_str().unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            let expected = receipt["expected"].as_bool().unwrap();
            examined += 1;
            let checked = (|| -> Result<bool> {
                let host = crate::V7HostPosition::from_envelope(receipt["before"].clone())?;
                let state = host.state();
                let before = serde_jcs::to_vec(state).map_err(EngineError::serialization)?;
                let query = &receipt["query"];
                let actor: Color = serde_json::from_value(query["actor"].clone())
                    .map_err(EngineError::serialization)?;
                let result = match query["kind"].as_str() {
                    Some("capture-without-attacker") => {
                        let at = v7_descriptor_square(&query["target"])
                            .ok_or(EngineError::IllegalAction)?;
                        let target = state.at(at).ok_or(EngineError::IllegalAction)?;
                        let kind = query["attackerType"]
                            .as_str()
                            .ok_or(EngineError::IllegalAction)?;
                        v7_can_capture_target_without_attacker(state, actor, target, kind)?
                    }
                    Some("alibaba-placement") => {
                        let at = v7_descriptor_square(&query["square"])
                            .ok_or(EngineError::IllegalAction)?;
                        open_alibaba_placement(state, at, actor)?
                    }
                    _ => {
                        return Err(EngineError::InvalidState(
                            "unknown nominal/placement receipt query".into(),
                        ));
                    }
                };
                assert_eq!(
                    serde_jcs::to_vec(state).unwrap(),
                    before,
                    "query mutated {name}"
                );
                Ok(result)
            })();
            match checked {
                Err(error) => failures.push(format!("{name}: {error}")),
                Ok(actual) if actual != expected => {
                    failures.push(format!("{name}: actual={actual} expected={expected}"))
                }
                Ok(_) => {}
            }
        }
        assert_eq!(examined, 48, "nominal capture / placement receipt count");
        assert!(
            failures.is_empty(),
            "{} nominal capture / placement mismatches of {examined}:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    #[ignore = "source movement queries are supplied from an external research receipt"]
    fn frozen_v7_movement_queries_match_complete_source_payloads() {
        let path = std::env::var("ACCELERATE_V7_MOVEMENT_RECEIPTS").unwrap();
        let input = std::fs::read_to_string(path).unwrap();
        let mut examined = 0;
        let mut failures = Vec::new();
        for line in input.lines().filter(|line| !line.trim().is_empty()) {
            let receipt: Value = serde_json::from_str(line).unwrap();
            let name = receipt["name"].as_str().unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            examined += 1;
            let checked = (|| -> Result<(Value, Value)> {
                let host = crate::V7HostPosition::from_envelope(receipt["before"].clone())?;
                let state = host.state();
                let from =
                    v7_descriptor_square(&receipt["from"]).ok_or(EngineError::IllegalAction)?;
                let piece = state.at(from).ok_or(EngineError::IllegalAction)?;
                let before = serde_jcs::to_vec(state).map_err(EngineError::serialization)?;
                let moves = v7_legal_move_targets(state, piece, from, V7MoveOptions::default())?;
                let hints: Vec<_> = v7_ui_move_hints(state, Color::White)?
                    .into_iter()
                    .map(|(from, destinations)| json!({"from":from,"destinations":destinations}))
                    .collect();
                assert_eq!(
                    serde_jcs::to_vec(state).unwrap(),
                    before,
                    "query mutated {name}"
                );
                Ok((
                    serde_json::to_value(moves).map_err(EngineError::serialization)?,
                    json!(hints),
                ))
            })();
            match checked {
                Err(error) => failures.push(format!("{name}: {error}")),
                Ok((moves, hints)) => {
                    for (label, actual, expected) in [
                        ("moves", moves, &receipt["moves"]),
                        ("hints", hints, &receipt["hints"]),
                    ] {
                        if &actual == expected {
                            continue;
                        }
                        let actual = actual.as_array().unwrap();
                        let expected = expected.as_array().unwrap();
                        let first = actual
                            .iter()
                            .zip(expected)
                            .position(|(a, b)| a != b)
                            .unwrap_or(actual.len().min(expected.len()));
                        failures.push(format!("{name} {label}: actual {} expected {} first {first} actual={} expected={}",
                            actual.len(),expected.len(),actual.get(first).unwrap_or(&Value::Null),expected.get(first).unwrap_or(&Value::Null)));
                    }
                }
            }
        }
        assert!(
            examined > 0 && examined <= 512,
            "movement receipt query count {examined}"
        );
        assert!(
            failures.is_empty(),
            "{} movement query mismatches of {examined}:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    #[ignore = "one-off differential diagnosis using an external source receipt"]
    fn diagnose_v7_source_move_candidates() {
        let path = std::env::var("ACCELERATE_V7_SOURCE_CASES").unwrap();
        let source = std::fs::read_to_string(path).unwrap();
        for line in source.lines() {
            let case: Value = serde_json::from_str(line).unwrap();
            let name = case["name"].as_str().unwrap();
            let host = crate::V7HostPosition::from_envelope(case["position"].clone()).unwrap();
            let state = host.state();
            if state.mode != "play" || state.result().is_some() {
                continue;
            }
            let expected = case["actions"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|entry| entry["payload"]["type"] == "move")
                .map(|entry| entry["payload"].clone())
                .collect::<Vec<_>>();
            match legal_move_candidates(state) {
                Ok(actions) => {
                    let actual = actions
                        .iter()
                        .map(|action| serde_json::to_value(action).unwrap())
                        .collect::<Vec<_>>();
                    let first = actual.iter().zip(expected.iter()).position(|(a, b)| a != b);
                    println!(
                        "{name}: actual {} expected {} first mismatch {first:?}",
                        actual.len(),
                        expected.len()
                    );
                    if let Some(index) = first {
                        println!("  actual: {}", actual[index]);
                        println!("  expect: {}", expected[index]);
                    }
                }
                Err(error) => println!("{name}: error {error}"),
            }
        }
        if let Ok(directory) = std::env::var("ACCELERATE_V7_CONDITIONING_REPORT") {
            for name in ["normal-seed19-second-pick", "chaos-seed19-second-pick"] {
                let path = std::path::Path::new(&directory).join(format!("{name}.json"));
                let envelope: Value =
                    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
                let host = crate::V7HostPosition::from_envelope(envelope).unwrap();
                let state = host.state();
                let from = Square { row: 6, col: 0 };
                let pawn = state.at(from).unwrap();
                println!("{name} pawn {}", serde_json::to_value(pawn).unwrap());
                println!("{name} raw {:?}", piece_moves(state, pawn, from));
                println!("{name} legal {:?}", legal_move_candidates(state));
            }
        }
    }

    #[test]
    #[ignore = "one-off diagnosis from an external frozen source receipt"]
    fn diagnose_v7_first_move_receipt() {
        let path = std::env::var("ACCELERATE_V7_FIRST_MOVE_RECEIPT").unwrap();
        let receipt: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let envelope = receipt.get("before").unwrap_or(&receipt).clone();
        let host = crate::V7HostPosition::from_envelope(envelope).unwrap();
        let state = host.state();
        println!("opening: {:?}", ensure_v7_orthodox_opening(state));
        let moves = legal_move_candidates(state);
        println!("moves: {:?}", moves.as_ref().map(|items| items.len()));
        println!(
            "first moves: {:?}",
            moves.map(|items| items.into_iter().take(8).collect::<Vec<_>>())
        );
    }

    #[test]
    #[ignore = "source threat windows are supplied from an external research receipt"]
    fn diagnose_v7_qg_threat_windows() {
        let path = std::env::var("ACCELERATE_V7_QG_THREAT_WINDOWS").unwrap();
        let receipt: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for window in receipt["windows"].as_array().unwrap() {
            let mut state: GameState = serde_json::from_value(window["state"].clone()).unwrap();
            state.ruleset_id = RULES_VERSION_V7.into();
            let expected = window["sourceAiNoCards"].as_array().unwrap();
            match legal_move_candidates(&state) {
                Ok(actual) => {
                    let actual = actual
                        .iter()
                        .map(|action| serde_json::to_value(action).unwrap())
                        .collect::<Vec<_>>();
                    let first = actual
                        .iter()
                        .zip(expected)
                        .position(|(left, right)| left != right);
                    println!(
                        "sound {} attacker {}: native {} source {} mismatch {first:?}",
                        window["soundIndex"],
                        window["attacker"],
                        actual.len(),
                        expected.len()
                    );
                    if let Some(index) = first {
                        println!("  native {}", actual[index]);
                        println!("  source {}", expected[index]);
                    }
                }
                Err(error) => println!(
                    "sound {} attacker {}: error {error}",
                    window["soundIndex"], window["attacker"]
                ),
            }
        }
    }
    use crate::geometry::Offset;
    use crate::move_program::{
        ActivationCondition, MoveNode, MoveProgram, MoveProgramLimits, MoveProgramSet, Primitive,
        SpatialMoveBoard,
    };

    fn empty_v7() -> GameState {
        let mut state = GameState::new(GameConfig::default(), 17).unwrap();
        state.board = vec![vec![None; 8]; 8];
        state.ruleset_id = RULES_VERSION_V7.into();
        state
    }

    fn capture_pair(attacker_kind: &str, target_kind: &str) -> (GameState, Piece, Piece) {
        let mut state = empty_v7();
        let attacker = Piece::new(attacker_kind, Color::White, "attacker");
        let target = Piece::new(target_kind, Color::Black, "target");
        state.board[4][4] = Some(attacker.clone());
        state.board[4][5] = Some(target.clone());
        (state, attacker, target)
    }

    #[test]
    fn v7_nominal_capture_does_not_inherit_board_attacker_or_royal_access() {
        let (mut state, mut don, target) = capture_pair("donQuixote", "pawn");
        don.extra.insert("capturesMade".into(), json!(3));
        don.extra.insert("cardNoCaptureUntil".into(), json!(7));
        don.extra.insert("desperado".into(), json!({"remaining":1}));
        state.board[4][4] = Some(don.clone());
        state.extra.insert("saturationRule".into(), json!(true));
        assert!(!v7_can_capture_target(&state, &don, &target, false, false).unwrap());
        assert!(
            v7_can_capture_target_without_attacker(&state, Color::White, &target, "donQuixote")
                .unwrap()
        );
        state.extra.insert(
            "freeMoveCaptureLock".into(),
            json!({"white":true,"black":false}),
        );
        assert!(
            !v7_can_capture_target_without_attacker(&state, Color::White, &target, "donQuixote")
                .unwrap()
        );

        let (state, king, jester) = capture_pair("king", "jester");
        assert!(v7_can_capture_target(&state, &king, &jester, false, false).unwrap());
        assert!(
            !v7_can_capture_target_without_attacker(&state, Color::White, &jester, "king").unwrap()
        );
        let vip = Piece::new("vip", Color::White, "vip-attacker");
        assert!(!v7_can_capture_target(&state, &vip, &jester, false, false).unwrap());
        assert!(
            !v7_can_capture_target_without_attacker(&state, Color::White, &jester, "vip").unwrap()
        );
        let mut restricted = Piece::new("pawn", Color::Black, "royal-only");
        restricted
            .extra
            .insert("captureRestriction".into(), json!("royal-only"));
        assert!(
            !v7_can_capture_target_without_attacker(&state, Color::White, &restricted, "king")
                .unwrap()
        );

        let (mut state, mut don, mut target) = capture_pair("donQuixote", "pawn");
        state.extra.insert(
            "campaign".into(),
            json!({"setup":"timeTraveler","timeTraveler":{"phase":"past","attackEnabledFor":null}}),
        );
        don.extra.insert("timePhase".into(), json!("future"));
        target.extra.insert("timePhase".into(), json!("past"));
        state.board[4][4] = Some(don.clone());
        state.board[4][5] = Some(target.clone());
        assert!(!v7_can_capture_target(&state, &don, &target, false, false).unwrap());
        assert!(
            v7_can_capture_target_without_attacker(&state, Color::White, &target, "donQuixote")
                .unwrap()
        );
    }

    #[test]
    fn v7_alibaba_placement_reserves_only_live_ground_crowns_and_preserves_v6() {
        let mut state = empty_v7();
        let ground = Square { row: 3, col: 3 };
        let open = Square { row: 4, col: 3 };
        state.extra.insert("crownRule".into(), json!(true));
        assert!(!open_alibaba_placement(&state, ground, Color::White).unwrap());
        assert!(open_alibaba_placement(&state, open, Color::White).unwrap());
        state.extra.insert(
            "crownRule".into(),
            json!({"crowns":[
                {"ground":{"row":3.0,"col":3.0},"removed":false},
                {"ground":{"row":4,"col":3},"removed":true},
                {"carrierId":"carried-crown","ground":null}
            ]}),
        );
        assert!(!open_alibaba_placement(&state, ground, Color::White).unwrap());
        assert!(open_alibaba_placement(&state, open, Color::White).unwrap());
        state
            .extra
            .insert("blackHole".into(), json!([{"row":4.0,"col":3.0}]));
        assert!(!open_alibaba_placement(&state, open, Color::White).unwrap());
        assert!(open_alibaba_placement(&state, Square { row: 4, col: 4 }, Color::White).unwrap());
        state
            .extra
            .insert("blackHole".into(), json!({"row":4,"col":3}));
        assert!(open_alibaba_placement(&state, open, Color::White).unwrap());
        state
            .extra
            .insert("blackHole".into(), json!([{"row":"4","col":"3"}]));
        assert!(!open_alibaba_placement(&state, open, Color::White).unwrap());
        state.extra.insert(
            "blackHole".into(),
            json!([{"row":4.5,"col":3},{"row":8,"col":3},{}]),
        );
        assert!(open_alibaba_placement(&state, open, Color::White).unwrap());
        state.ruleset_id = RULES_VERSION_V6.into();
        assert!(
            matches!(open_alibaba_placement(&state,open,Color::White),Err(EngineError::UnsupportedFeature(reason)) if reason=="placement on ground crowns")
        );
    }

    #[test]
    fn v7_local_janggi_uses_source_piece_moves_and_royal_palace_landing() {
        let mut state = empty_v7();
        state.mode = "play".into();
        state.extra.insert(
            "campaign".into(),
            json!({"id":"janggi","setup":"janggi","playerColor":"white"}),
        );
        state.extra.insert("palaces".into(),json!([
            {"color":"black","center":{"row":0,"col":4},"cells":[{"row":0,"col":3},{"row":0,"col":4},{"row":1,"col":3},{"row":1,"col":4}]},
            {"color":"white","center":{"row":7,"col":4},"cells":[{"row":7,"col":3},{"row":7,"col":4},{"row":6,"col":3},{"row":6,"col":4}]}
        ]));
        let king = Piece::new("king", Color::White, "janggi-king");
        let from = Square { row: 7, col: 4 };
        state.board[7][4] = Some(king.clone());
        let targets = v7_legal_move_targets(&state, &king, from, V7MoveOptions::default()).unwrap();
        assert_eq!(
            targets.iter().map(MoveTarget::square).collect::<Vec<_>>(),
            [
                Square { row: 6, col: 3 },
                Square { row: 6, col: 4 },
                Square { row: 7, col: 3 }
            ]
        );
        // 원문 공격 조회는 궁성의 legal 착지 필터를 경유하지 않는다.
        assert!(
            crate::v7_threat::piece_attacks_square_v7(
                &state,
                &king,
                from,
                Square { row: 6, col: 5 }
            )
            .unwrap()
        );
        let man = Piece::new("man", Color::White, "janggi-man");
        state.board[6][3] = Some(man.clone());
        let targets = v7_legal_move_targets(
            &state,
            &man,
            Square { row: 6, col: 3 },
            V7MoveOptions::default(),
        )
        .unwrap();
        assert!(
            targets
                .iter()
                .any(|target| target.square() == Square { row: 5, col: 2 })
        );
        let camel = Piece::new("camel", Color::White, "janggi-camel");
        state.board[7][2] = Some(camel.clone());
        state.board[5][2] = Some(Piece::new("pawn", Color::White, "intermediate-occupant"));
        let targets = v7_legal_move_targets(
            &state,
            &camel,
            Square { row: 7, col: 2 },
            V7MoveOptions::default(),
        )
        .unwrap();
        assert!(
            targets
                .iter()
                .any(|target| target.square() == Square { row: 4, col: 1 })
        );
    }

    #[test]
    fn v7_camouflage_is_movement_transparency_not_capture_permission() {
        // Frozen e5ed84fc direct probes: with a protected black pawn on the
        // black-matching dark square (4,5), rayMoves includes the square and
        // both squares beyond it, while canCaptureTarget remains false. A
        // white pawn/king on (5,3) may also enter concealed (4,3).
        let mut state = empty_v7();
        state.extra.insert("camouflageRule".into(), json!(true));
        let rook = Piece::new("rook", Color::White, "rook");
        let mut concealed = Piece::new("pawn", Color::Black, "concealed");
        concealed.extra.insert("protected".into(), json!(true));
        state.board[4][4] = Some(rook.clone());
        state.board[4][5] = Some(concealed.clone());
        let ray = rays(&state, &rook, Square { row: 4, col: 4 }, &[(0, 1)], 7)
            .into_iter()
            .map(|target| target.square())
            .collect::<Vec<_>>();
        assert_eq!(
            ray,
            vec![
                Square { row: 4, col: 5 },
                Square { row: 4, col: 6 },
                Square { row: 4, col: 7 }
            ]
        );
        assert!(!can_capture(&state, &rook, &concealed));

        state.board[4][4] = None;
        state.board[4][5] = None;
        state.board[4][3] = Some(concealed.clone());
        let from = Square { row: 5, col: 3 };
        let to = Square { row: 4, col: 3 };
        let pawn = Piece::new("pawn", Color::White, "pawn");
        state.board[5][3] = Some(pawn.clone());
        assert!(pawn_moves(&state, &pawn, from).contains(&MoveTarget::at(to)));
        let king = Piece::new("king", Color::White, "king");
        state.board[5][3] = Some(king.clone());
        assert!(v7_jump_leaps(&state, &king, from, &[(-1, 0)]).contains(&MoveTarget::at(to)));
        assert!(!leaps(&state, &king, from, &[(-1, 0)]).contains(&MoveTarget::at(to)));
    }

    #[test]
    fn v7_capture_classification_uses_source_action_options_and_portal_cells() {
        let (mut state, mut rook, pawn) = capture_pair("rook", "pawn");
        rook.extra.insert("capturesMade".into(), json!(3));
        state.board[4][4] = Some(rook.clone());
        state.extra.insert("saturationRule".into(), json!(true));
        state
            .extra
            .insert("freeMoveCaptureLock".into(), json!({"white":true}));
        state
            .extra
            .insert("chaosNoCaptureUntilHalfTurn".into(), json!(1));
        assert!(!can_capture(&state, &rook, &pawn));
        assert!(
            v7_is_capture_move(&state, &rook, &MoveTarget::at(Square { row: 4, col: 5 })).unwrap()
        );
        let mut log = MoveTarget::at(Square { row: 4, col: 5 });
        log.flags.insert("setLogDirection".into(), json!(true));
        assert!(!v7_is_capture_move(&state, &rook, &log).unwrap());

        let mut portal = MoveTarget::at(Square { row: 3, col: 3 });
        portal.flags.insert("portalLanding".into(), json!(true));
        portal
            .flags
            .insert("portalEntry".into(), json!({"row":4,"col":5}));
        portal
            .flags
            .insert("portalExit".into(), json!({"row":2,"col":2}));
        assert!(v7_is_capture_move(&state, &rook, &portal).unwrap());
        let mut siege = MoveTarget::at(Square { row: 4, col: 5 });
        siege.flags.insert("siegeRamMove".into(), json!(true));
        assert!(!v7_is_capture_move(&state, &rook, &siege).unwrap());
        siege
            .flags
            .insert("highlightCells".into(), json!([{"row":4,"col":5}]));
        assert!(v7_is_capture_move(&state, &rook, &siege).unwrap());
    }

    #[test]
    fn v7_source_capture_matrix_preserves_v6_reader_policy() {
        // Frozen e5ed84fc canCaptureTarget direct probes: see the out-of-Git
        // v7-capture-policy/source-cases.json research receipt.
        let (state, attacker, target) = capture_pair("rook", "pawn");
        assert!(can_capture(&state, &attacker, &target));

        let (mut state, attacker, mut target) = capture_pair("rook", "pawn");
        target.extra.insert("metalized".into(), json!(true));
        state.board[4][5] = Some(target.clone());
        assert!(!can_capture(&state, &attacker, &target));
        state.ruleset_id = RULES_VERSION_V6.into();
        assert!(can_capture(&state, &attacker, &target));

        let (mut state, attacker, mut target) = capture_pair("rook", "scarecrow");
        target.extra.insert("metalized".into(), json!(true));
        state.board[4][5] = Some(target.clone());
        assert!(can_capture(&state, &attacker, &target));

        let (state, attacker, target) = capture_pair("rook", "monster");
        assert!(!can_capture(&state, &attacker, &target));
        let (state, attacker, target) = capture_pair("darkWizard", "monster");
        assert!(can_capture(&state, &attacker, &target));
        for target_kind in ["darkWizard", "scarecrow"] {
            let (state, attacker, target) = capture_pair("monster", target_kind);
            assert!(!can_capture(&state, &attacker, &target));
        }
        let (state, attacker, target) = capture_pair("monster", "pawn");
        assert!(can_capture(&state, &attacker, &target));

        let (mut state, attacker, target) = capture_pair("rook", "revolvingDoor");
        state.extra.insert("revolvingDoorGuard".into(), json!(true));
        assert!(!can_capture(&state, &attacker, &target));
        state
            .extra
            .insert("revolvingDoorGuard".into(), json!(false));
        assert!(can_capture(&state, &attacker, &target));
        state.ruleset_id = RULES_VERSION_V6.into();
        assert!(!can_capture(&state, &attacker, &target));

        let (mut state, attacker, target) = capture_pair("rook", "blackHole");
        assert!(can_capture(&state, &attacker, &target));
        state.ruleset_id = RULES_VERSION_V6.into();
        assert!(!can_capture(&state, &attacker, &target));

        let (mut state, attacker, mut target) = capture_pair("rook", "pawn");
        target.extra.insert("outpostProtected".into(), json!(true));
        state.board[4][5] = Some(target.clone());
        assert!(can_capture(&state, &attacker, &target));
        state.ruleset_id = RULES_VERSION_V6.into();
        assert!(!can_capture(&state, &attacker, &target));
    }

    #[test]
    fn v7_time_phase_capture_and_ray_transparency_match_source_order() {
        let (mut state, mut attacker, mut target) = capture_pair("rook", "pawn");
        state.extra.insert(
            "campaign".into(),
            json!({
                "setup":"timeTraveler",
                "timeTraveler":{"phase":"future","visited":[],"attackEnabledFor":null}
            }),
        );
        attacker.extra.insert("timePhase".into(), json!("past"));
        target.extra.insert("timePhase".into(), json!("future"));
        state.board[4][4] = Some(attacker.clone());
        state.board[4][5] = Some(target.clone());
        assert!(!can_capture(&state, &attacker, &target));
        let right = |state: &GameState| {
            rays(state, &attacker, Square { row: 4, col: 4 }, ORTHO, 7)
                .into_iter()
                .map(|target| target.square())
                .filter(|square| square.row == 4 && square.col >= 5)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            right(&state),
            vec![Square { row: 4, col: 6 }, Square { row: 4, col: 7 }]
        );
        target.extra.insert("timePhase".into(), json!("past"));
        state.board[4][5] = Some(target.clone());
        assert!(can_capture(&state, &attacker, &target));
        assert_eq!(right(&state), vec![Square { row: 4, col: 5 }]);
    }

    #[test]
    fn v7_source_capture_locks_and_royal_exceptions() {
        // These direct-predicate expectations were checked against the frozen
        // e5ed84fc client with the same synthetic attacker and target squares.
        for (name, field, value) in [
            (
                "armistice",
                "armistice",
                json!({"remaining":1,"by":"white","actedColors":[]}),
            ),
            (
                "free-move-lock",
                "freeMoveCaptureLock",
                json!({"white":true,"black":false}),
            ),
            ("chaos-lock", "chaosNoCaptureUntilHalfTurn", json!(4)),
        ] {
            let (mut state, attacker, target) = capture_pair("rook", "pawn");
            state.extra.insert(field.into(), value);
            assert!(!can_capture(&state, &attacker, &target), "{name}");
        }

        let (mut state, attacker, target) = capture_pair("rook", "pawn");
        state.extra.insert(
            "freeMoveCaptureLock".into(),
            json!({"white":1,"black":false}),
        );
        assert!(can_capture(&state, &attacker, &target));

        let (mut state, attacker, target) = capture_pair("guard", "pawn");
        state
            .extra
            .insert("socialism".into(), json!({"white":1,"black":0}));
        assert!(can_capture(&state, &attacker, &target));
        state
            .extra
            .insert("socialism".into(), json!({"white":-1,"black":0}));
        assert!(!can_capture(&state, &attacker, &target));
        state
            .extra
            .insert("socialism".into(), json!({"white":0,"black":0}));
        state.extra.insert(
            "royalCommand".into(),
            json!({
                "white":{"activeTurn":0,"expiresTurn":1},"black":null
            }),
        );
        assert!(can_capture(&state, &attacker, &target));

        let (mut state, attacker, target) = capture_pair("king", "queen");
        state
            .extra
            .insert("overwhelm".into(), json!({"white":false,"black":true}));
        assert!(!can_capture(&state, &attacker, &target));

        let (mut state, attacker, mut target) = capture_pair("rook", "pawn");
        target
            .extra
            .insert("vigilanceProtection".into(), json!({"remaining":1}));
        state.board[4][5] = Some(target.clone());
        assert!(!can_capture(&state, &attacker, &target));

        let (mut state, mut attacker, target) = capture_pair("queen", "pawn");
        attacker.extra.insert("regencyHeir".into(), json!(true));
        state.board[4][4] = Some(attacker.clone());
        state.extra.insert(
            "genevaConvention".into(),
            json!({"white":false,"black":true}),
        );
        assert!(can_capture(&state, &attacker, &target));

        let (mut state, mut attacker, target) = capture_pair("revolvingDoor", "pawn");
        attacker.extra.insert("crownBearer".into(), json!(true));
        state.board[4][4] = Some(attacker.clone());
        state.extra.insert("revolvingDoorGuard".into(), json!(true));
        assert!(can_capture(&state, &attacker, &target));
        state
            .extra
            .insert("revolvingDoorGuard".into(), json!(false));
        assert!(!can_capture(&state, &attacker, &target));

        let (mut state, mut attacker, target) = capture_pair("rook", "pawn");
        attacker.extra.insert("promotionRushUntil".into(), json!(1));
        state.board[4][4] = Some(attacker.clone());
        assert!(!can_capture(&state, &attacker, &target));

        for kind in ["guard", "recruiter", "revolvingDoor"] {
            let (mut state, mut attacker, target) = capture_pair(kind, "pawn");
            attacker.extra.insert("basicTraining".into(), json!(true));
            state.board[4][4] = Some(attacker.clone());
            state.extra.insert("revolvingDoorGuard".into(), json!(true));
            assert!(!can_capture(&state, &attacker, &target), "{kind} default");
            assert!(
                can_capture_with_options(
                    &state,
                    &attacker,
                    &target,
                    CaptureOptions {
                        allow_basic_training_capture: true,
                        ..CaptureOptions::default()
                    }
                ),
                "{kind} basic training"
            );
            if kind == "revolvingDoor" {
                state
                    .extra
                    .insert("revolvingDoorGuard".into(), json!(false));
                assert!(!can_capture_with_options(
                    &state,
                    &attacker,
                    &target,
                    CaptureOptions {
                        allow_basic_training_capture: true,
                        ..CaptureOptions::default()
                    }
                ));
            }
        }
    }

    #[test]
    fn v7_checker_keeps_source_specific_outpost_rejection() {
        // Source canCaptureTarget alone accepts outpostProtected, while
        // checkerCaptureMoves applies isEncouraged before emitting the jump.
        let (mut state, attacker, mut target) = capture_pair("checker", "pawn");
        state.board[4][5] = None;
        state.board[3][5] = Some(target.clone());
        let from = Square { row: 4, col: 4 };
        let mut capture = MoveTarget::at(Square { row: 2, col: 6 });
        capture
            .flags
            .insert("jumpCapture".into(), json!({"row":3,"col":5}));
        capture.flags.insert("checkerCapture".into(), json!(true));
        assert!(checker(&state, &attacker, from).contains(&capture));
        target.extra.insert("outpostProtected".into(), json!(true));
        state.board[3][5] = Some(target.clone());
        assert!(can_capture(&state, &attacker, &target));
        assert!(!checker(&state, &attacker, from).contains(&capture));
    }

    #[test]
    fn v7_large_piece_body_order_and_allied_landing_match_source() {
        // Frozen bigRookMoves directly yielded 13 empty-board candidates,
        // then 11 when the first landing contains an allied pawn. The ordered
        // arrays and flag payloads are in the out-of-Git large-cases receipt.
        let from = Square { row: 3, col: 3 };
        let setup = |kind: &str, ally: Option<Square>| {
            let mut state = empty_v7();
            let mut piece = Piece::new(kind, Color::White, "large");
            piece.extra.insert("anchorRow".into(), json!(3));
            piece.extra.insert("anchorCol".into(), json!(3));
            for (row, col) in [(3, 3), (3, 4), (4, 3), (4, 4)] {
                state.board[row][col] = Some(piece.clone());
            }
            if let Some(cell) = ally {
                state.board[cell.row as usize][cell.col as usize] =
                    Some(Piece::new("pawn", Color::White, "ally"));
            }
            (state, piece)
        };
        let (state, piece) = setup("bigRook", None);
        let rook = v7_large_moves(&state, &piece, from).unwrap();
        assert_eq!(
            rook.iter().map(MoveTarget::square).collect::<Vec<_>>(),
            [
                (3, 3),
                (2, 3),
                (1, 3),
                (0, 3),
                (4, 3),
                (5, 3),
                (6, 3),
                (3, 2),
                (3, 1),
                (3, 0),
                (3, 4),
                (3, 5),
                (3, 6)
            ]
            .map(|(row, col)| Square { row, col })
        );
        assert_eq!(
            serde_json::to_value(&rook[0]).unwrap(),
            json!({
            "row":3,"col":3,"bodyCells":[
                {"row":3,"col":3},{"row":3,"col":4},
                {"row":4,"col":3},{"row":4,"col":4}],"colossusBody":true})
        );
        assert_eq!(
            serde_json::to_value(&rook[1]).unwrap(),
            json!({
            "row":2,"col":3,"anchorRow":2,"anchorCol":3,
            "highlightCells":[{"row":2,"col":3},{"row":2,"col":4},
                {"row":3,"col":3},{"row":3,"col":4}],
            "bigRookLandingCaptures":[],"bigRookMove":true})
        );

        let (mut state, piece) = setup("bigRook", Some(Square { row: 2, col: 3 }));
        let allied = v7_large_moves(&state, &piece, from).unwrap();
        assert_eq!(
            allied.iter().map(MoveTarget::square).collect::<Vec<_>>(),
            [
                (3, 3),
                (2, 3),
                (4, 3),
                (5, 3),
                (6, 3),
                (3, 2),
                (3, 1),
                (3, 0),
                (3, 4),
                (3, 5),
                (3, 6)
            ]
            .map(|(row, col)| Square { row, col })
        );
        assert_eq!(
            allied[1].flags.get("bigRookLandingCaptures"),
            Some(&json!([{"row":2,"col":3}]))
        );
        state.ruleset_id = RULES_VERSION_V6.into();
        assert!(
            !large_rays(&state, &piece, from)
                .iter()
                .any(|target| target.square() == Square { row: 2, col: 3 })
        );

        let (state, piece) = setup("bigBishop", Some(Square { row: 2, col: 2 }));
        let bishop = v7_large_moves(&state, &piece, from).unwrap();
        assert_eq!(
            bishop.iter().map(MoveTarget::square).collect::<Vec<_>>(),
            [
                (3, 3),
                (2, 2),
                (2, 4),
                (1, 5),
                (0, 6),
                (4, 2),
                (5, 1),
                (6, 0),
                (4, 4),
                (5, 5),
                (6, 6)
            ]
            .map(|(row, col)| Square { row, col })
        );
        assert_eq!(
            bishop[1].flags.get("bigRookLandingCaptures"),
            Some(&json!([{"row":2,"col":2}]))
        );
    }

    #[test]
    fn v7_frontline_blocks_orthogonal_rook_capture_without_mutating_state() {
        let (mut state, attacker, _) = capture_pair("rook", "rook");
        state.extra.insert(
            "frontlineResponse".into(),
            json!({"white":false,"black":true}),
        );
        let before = serde_jcs::to_vec(&state).unwrap();
        let moves = piece_moves(&state, &attacker, Square { row: 4, col: 4 }).unwrap();
        assert!(
            !moves
                .iter()
                .any(|target| target.square() == Square { row: 4, col: 5 })
        );
        assert_eq!(serde_jcs::to_vec(&state).unwrap(), before);
    }

    #[test]
    fn v7_missing_time_phase_uses_source_future_without_query_mutation() {
        let (mut state, attacker, _) = capture_pair("rook", "pawn");
        state.extra.insert(
            "campaign".into(),
            json!({
                "setup":"timeTraveler",
                "timeTraveler":{"phase":"future","visited":[],"attackEnabledFor":null}
            }),
        );
        let before = serde_jcs::to_vec(&state).unwrap();
        let moves = piece_moves(&state, &attacker, Square { row: 4, col: 4 }).unwrap();
        assert!(!moves.is_empty());
        assert_eq!(serde_jcs::to_vec(&state).unwrap(), before);
    }

    #[test]
    fn no_action_loss_probe_considers_football_under_requested_turn() {
        let mut state = empty_v7();
        state.turn = Color::White;
        state.board[2][0] = Some(Piece::new("rook", Color::White, "opponent"));
        state.board[3][2] = Some(Piece::new("rook", Color::Black, "kicker"));
        state.board[3][3] = Some(Piece::new("football", PieceColor::Neutral, "ball"));
        state.board[3][4] = Some(Piece::new("wall", PieceColor::Neutral, "wall"));
        let mut probed = Vec::new();
        let has_move = has_any_legal_move_v7_with(&state, Color::Black, |probe, piece, square| {
            assert_eq!(probe.turn, Color::Black);
            probed.push((piece.id.clone(), square));
            Ok(piece.kind == "football")
        })
        .unwrap();
        assert!(has_move);
        assert_eq!(state.turn, Color::White);
        assert_eq!(
            probed,
            vec![
                ("kicker".into(), Square { row: 3, col: 2 }),
                ("ball".into(), Square { row: 3, col: 3 }),
            ]
        );
    }

    #[test]
    fn v7_live_move_query_records_restriction_reach_even_without_moves() {
        let mut state = empty_v7();
        let from = Square { row: 4, col: 3 };
        let mut coffin = Piece::new("coffin", Color::White, "empty-query");
        coffin.extra.insert("monoShade".into(), json!("light"));
        state.board[4][3] = Some(coffin.clone());
        state.extra.insert("monochromeChess".into(), json!(true));
        let before = serde_jcs::to_vec(&state).unwrap();
        let query = v7_legal_move_query(&state, &coffin, from, V7MoveOptions::default()).unwrap();
        assert!(query.targets.is_empty());
        assert!(query.effects.apply_move_restrictions_reached);
        assert_eq!(query.effects.mono_shades.len(), 1);
        assert_eq!(query.effects.mono_shades[0].shade, "dark");
        assert_eq!(serde_jcs::to_vec(&state).unwrap(), before);
        assert!(
            v7_legal_move_targets(&state, &coffin, from, V7MoveOptions::default())
                .unwrap()
                .is_empty()
        );
        assert_eq!(serde_jcs::to_vec(&state).unwrap(), before);
        assert!(
            v7_legal_move_targets_live(&mut state, from, V7MoveOptions::default())
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            state.at(from).unwrap().extra.get("monoShade"),
            Some(&json!("dark"))
        );

        // main95629의 PawnQueen 조기 return도 restrictions를 실행한다.
        // 사방이 wall이라 queen 후보가 비어도 main98212를 기록한다.
        let mut pawn = Piece::new("pawn", Color::White, "empty-pawn-queen");
        pawn.extra.insert("monoShade".into(), json!("light"));
        for row in 0..8 {
            for col in 0..8 {
                state.board[row][col] = Some(Piece::new(
                    "wall",
                    PieceColor::Neutral,
                    format!("wall-{row}-{col}"),
                ));
            }
        }
        state.board[4][3] = Some(pawn.clone());
        state.extra.insert(
            "effects".into(),
            json!({"pawnQueen":"white","pawnReverse":{"white":0,"black":0}}),
        );
        let query = v7_legal_move_query(&state, &pawn, from, V7MoveOptions::default()).unwrap();
        assert!(query.targets.is_empty());
        assert!(query.effects.apply_move_restrictions_reached);
        assert_eq!(query.effects.mono_shades[0].shade, "dark");
    }

    #[test]
    fn v7_live_move_query_does_not_record_source_early_returns() {
        for (kind, attributes) in [
            ("rook", json!({"frozen":{"remaining":2}})),
            ("rook", json!({"poisonStunTurns":1})),
            ("medium", json!({})),
            ("babyBear", json!({})),
            ("clockwork", json!({})),
            ("wall", json!({})),
        ] {
            let mut state = empty_v7();
            state.extra.insert("monochromeChess".into(), json!(true));
            let from = Square { row: 4, col: 3 };
            let mut piece = Piece::new(kind, Color::White, "early-query");
            piece.extra.extend(attributes.as_object().unwrap().clone());
            state.board[4][3] = Some(piece.clone());
            let before = serde_jcs::to_vec(&state).unwrap();
            let query =
                v7_legal_move_query(&state, &piece, from, V7MoveOptions::default()).unwrap();
            assert!(query.targets.is_empty(), "{kind}");
            assert!(!query.effects.apply_move_restrictions_reached, "{kind}");
            assert!(query.effects.mono_shades.is_empty(), "{kind}");
            assert!(
                v7_legal_move_targets_live(&mut state, from, V7MoveOptions::default())
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(serde_jcs::to_vec(&state).unwrap(), before, "{kind}");
        }
    }

    #[test]
    fn v7_live_move_query_keeps_football_clone_shade_off_the_board() {
        for color in [PieceColor::White, PieceColor::Black, PieceColor::Neutral] {
            let mut state = empty_v7();
            state.turn = Color::White;
            state.extra.insert("monochromeChess".into(), json!(true));
            let from = Square { row: 3, col: 3 };
            let mut ball = Piece::new("football", color, "ball-query");
            ball.extra.insert("monoShade".into(), json!("dark"));
            state.board[3][3] = Some(ball.clone());
            let before = serde_jcs::to_vec(&state).unwrap();
            let query = v7_legal_move_query(&state, &ball, from, V7MoveOptions::default()).unwrap();
            assert!(query.effects.apply_move_restrictions_reached);
            assert!(query.effects.mono_shades.is_empty());
            v7_legal_move_targets_live(&mut state, from, V7MoveOptions::default()).unwrap();
            assert_eq!(serde_jcs::to_vec(&state).unwrap(), before);
        }
    }

    #[test]
    fn v7_live_move_query_normalizes_large_alias_and_updates_shared_identity() {
        let mut state = empty_v7();
        state.extra.insert("monochromeChess".into(), json!(true));
        let anchor = Square { row: 4, col: 3 };
        let alias = Square { row: 5, col: 3 };
        let mut large = Piece::new("bigRook", Color::White, "large-query");
        large.extra.insert("anchorRow".into(), json!(anchor.row));
        large.extra.insert("anchorCol".into(), json!(anchor.col));
        let body = [(4, 3), (4, 4), (5, 3), (5, 4)].map(|(row, col)| Square { row, col });
        for at in body {
            state.board[at.row as usize][at.col as usize] = Some(large.clone());
        }
        state.board[0][0] = Some(Piece::new("coffin", Color::White, "unqueried"));
        let query = v7_legal_move_query(&state, &large, alias, V7MoveOptions::default()).unwrap();
        assert_eq!(query.effects.mono_shades[0].square, anchor);
        assert_eq!(query.effects.mono_shades[0].shade, "dark");
        v7_legal_move_targets_live(&mut state, alias, V7MoveOptions::default()).unwrap();
        for at in body {
            assert_eq!(
                state.at(at).unwrap().extra.get("monoShade"),
                Some(&json!("dark"))
            );
        }
        assert!(
            !state
                .at(Square { row: 0, col: 0 })
                .unwrap()
                .extra
                .contains_key("monoShade")
        );
    }

    #[test]
    fn v7_live_no_action_query_keeps_row_major_stop_and_restores_turn() {
        let mut state = empty_v7();
        state.turn = Color::Black;
        state.extra.insert("monochromeChess".into(), json!(true));
        state.board[0][0] = Some(Piece::new("coffin", Color::White, "first-empty"));
        let mut frozen_rook = Piece::new("rook", Color::White, "early-blocked");
        frozen_rook
            .extra
            .insert("frozen".into(), json!({"remaining":2}));
        state.board[0][1] = Some(frozen_rook);
        state.board[0][2] = Some(Piece::new("rook", Color::White, "first-mobile"));
        state.board[0][3] = Some(Piece::new("coffin", Color::White, "after-stop"));
        state.board[1][0] = Some(Piece::new("rook", Color::Black, "opponent"));
        let before = serde_jcs::to_vec(&state).unwrap();
        assert!(v7_has_any_legal_move(&state, Color::White).unwrap());
        assert_eq!(serde_jcs::to_vec(&state).unwrap(), before);
        assert!(v7_has_any_legal_move_live(&mut state, Color::White).unwrap());
        assert_eq!(state.turn, Color::Black);
        for col in [0, 2] {
            assert_eq!(
                state
                    .at(Square { row: 0, col })
                    .unwrap()
                    .extra
                    .get("monoShade"),
                Some(&json!("light"))
            );
        }
        for at in [
            Square { row: 0, col: 1 },
            Square { row: 0, col: 3 },
            Square { row: 1, col: 0 },
        ] {
            assert!(!state.at(at).unwrap().extra.contains_key("monoShade"));
        }
        state.board[0][2] = Some(Piece::new("unported-query-kind", Color::White, "error"));
        assert!(matches!(
            v7_has_any_legal_move_live(&mut state, Color::White),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert_eq!(state.turn, Color::Black);
    }

    #[test]
    fn v7_live_move_query_retains_nested_checker_and_scarecrow_prefix_writes() {
        let mut state = empty_v7();
        let from = Square { row: 0, col: 0 };
        let coffin = Piece::new("coffin", Color::White, "parent-empty");
        state.board[0][0] = Some(coffin.clone());
        state.board[4][4] = Some(Piece::new("checker", Color::White, "checker-probe"));
        state.extra.insert("monochromeChess".into(), json!(true));
        let query = v7_legal_move_query(&state, &coffin, from, V7MoveOptions::default()).unwrap();
        assert_eq!(
            query
                .effects
                .mono_shades
                .iter()
                .map(|write| write.piece_id.as_str())
                .collect::<Vec<_>>(),
            ["parent-empty", "checker-probe"]
        );
        assert!(
            !state
                .at(Square { row: 4, col: 4 })
                .unwrap()
                .extra
                .contains_key("monoShade")
        );
        v7_legal_move_targets_live(&mut state, from, V7MoveOptions::default()).unwrap();
        assert_eq!(
            state
                .at(Square { row: 4, col: 4 })
                .unwrap()
                .extra
                .get("monoShade"),
            Some(&json!("light"))
        );

        state.board[4][4] = Some(Piece::new("rook", Color::White, "scarecrow-probe"));
        state.board[0][1] = Some(Piece::new("coffin", Color::White, "nested-empty"));
        state.board[4][6] = Some(Piece::new("scarecrow", Color::Black, "enemy-scarecrow"));
        let query = v7_legal_move_query(&state, &coffin, from, V7MoveOptions::default()).unwrap();
        assert!(query.targets.is_empty());
        assert_eq!(
            query
                .effects
                .mono_shades
                .iter()
                .map(|write| write.piece_id.as_str())
                .collect::<Vec<_>>(),
            ["parent-empty", "nested-empty", "scarecrow-probe"]
        );
        assert!(
            !state
                .at(Square { row: 0, col: 1 })
                .unwrap()
                .extra
                .contains_key("monoShade")
        );
        v7_legal_move_targets_live(&mut state, from, V7MoveOptions::default()).unwrap();
        assert_eq!(
            state
                .at(Square { row: 0, col: 1 })
                .unwrap()
                .extra
                .get("monoShade"),
            Some(&json!("dark"))
        );
        assert_eq!(
            state
                .at(Square { row: 4, col: 4 })
                .unwrap()
                .extra
                .get("monoShade"),
            Some(&json!("light"))
        );
    }

    #[test]
    fn v7_capture_constraint_projection_propagates_invalid_counters() {
        let mut state = empty_v7();
        let mut queen = Piece::new("queen", Color::White, "attacker");
        let pawn = Piece::new("pawn", Color::Black, "target");
        state.set_flag("genevaConvention", Color::Black, true);
        assert!(!v7_movement_capture_constraints_allow(&state, &queen, &pawn).unwrap());
        queen.extra.insert("regencyHeir".into(), json!(true));
        assert!(v7_movement_capture_constraints_allow(&state, &queen, &pawn).unwrap());
        queen.extra.insert("promotionRushUntil".into(), json!(2));
        assert!(!v7_movement_capture_constraints_allow(&state, &queen, &pawn).unwrap());
        *state.turns_taken.get_mut(Color::White) = 2;
        assert!(v7_movement_capture_constraints_allow(&state, &queen, &pawn).unwrap());
        queen
            .extra
            .insert("capturesMade".into(), json!("not-a-number"));
        assert!(matches!(
            v7_movement_capture_constraints_allow(&state, &queen, &pawn),
            Err(EngineError::InvalidState(_))
        ));
    }

    #[test]
    fn v7_public_enumeration_uses_per_piece_source_and_rejects_unknown_catalog_policy() {
        let mut state = empty_v7();
        let rook = Piece::new("rook", Color::White, "rook");
        let from = Square { row: 4, col: 4 };
        state.board[4][4] = Some(rook.clone());
        assert!(!legal_move_actions(&state).unwrap().is_empty());
        state.extra.insert(
            "profile".into(),
            json!({"catalogHash":"unknown-capture-profile"}),
        );
        assert!(
            matches!(legal_move_actions(&state),Err(EngineError::UnsupportedFeature(reason)) if reason.contains("catalog profile"))
        );
        state.extra.remove("profile");
        assert!(!piece_moves(&state, &rook, from).unwrap().is_empty());
        state.ruleset_id = RULES_VERSION_V6.into();
        assert!(!piece_moves(&state, &rook, from).unwrap().is_empty());
    }

    #[test]
    fn v7_effective_catalog_profile_masks_top_level_and_overrides_stale_flags() {
        let (mut state, attacker, target) = capture_pair("rook", "revolvingDoor");
        state
            .extra
            .insert("campaign".into(), json!({"setup":"fogWar"}));
        state
            .extra
            .insert("revolvingDoorGuard".into(), json!(false));
        state
            .extra
            .insert("september18Balance".into(), json!(false));
        state.extra.insert(
            "profile".into(),
            json!({"catalogHash":"yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4"}),
        );
        assert!(v7_uses_revolving_door_guard(&state));
        assert!(!v7_can_capture_target(&state, &attacker, &target, false, false).unwrap());

        state
            .extra
            .insert("profile".into(), json!({"catalogHash":"ignored-top-level"}));
        state.extra.insert("cardState".into(), json!({}));
        state.extra.insert("september18Balance".into(), json!(true));
        assert!(v7_effective_catalog_hash(&state).is_none());
        assert!(!v7_uses_revolving_door_guard(&state));
        assert!(v7_can_capture_target(&state, &attacker, &target, false, false).unwrap());

        state.extra.insert("cardState".into(), json!(false));
        assert!(
            matches!(v7_can_capture_target(&state, &attacker, &target, false, false),
            Err(EngineError::UnsupportedFeature(reason)) if reason.contains("ignored-top-level"))
        );

        state.extra.insert("cardState".into(), json!({}));
        state.extra.insert("cannonGhostScreen".into(), json!(false));
        state
            .extra
            .insert("unifiedJumpObstacles".into(), json!(false));
        let cannon = Piece::new("cannon", Color::White, "cannon");
        let mut ghost = Piece::new("rook", Color::White, "ghost");
        ghost.extra.insert("ghost".into(), json!(true));
        assert!(v7_ghost_transparent_for(
            &state,
            &cannon,
            &ghost,
            Square { row: 4, col: 5 },
            "cannon"
        ));
        let mut installation = Piece::new("pawn", Color::Black, "installation");
        installation
            .extra
            .insert("installationId".into(), json!("installed-pawn"));
        assert!(!v7_cannon_screen_allowed(&state, &installation));
        state.extra.insert(
            "cardState".into(),
            json!({"profile":{"catalogHash":"yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4"}}),
        );
        assert!(!v7_ghost_transparent_for(
            &state,
            &cannon,
            &ghost,
            Square { row: 4, col: 5 },
            "cannon"
        ));
        assert!(v7_cannon_screen_allowed(&state, &installation));
    }

    #[test]
    fn orthodox_opening_candidate_reuse_keeps_source_order_and_flag_payloads() {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.mode = "play".into();
        let moves = legal_move_candidates(&state).unwrap();
        let expected = (0..8)
            .flat_map(|col| {
                let from = Square { row: 6, col };
                let single =
                    Action::movement(Color::White, from, MoveTarget::at(Square { row: 5, col }));
                let mut double = MoveTarget::at(Square { row: 4, col });
                double
                    .flags
                    .insert("standardPawnDoubleStep".into(), json!(true));
                [single, Action::movement(Color::White, from, double)]
            })
            .chain(
                [(1, 0), (1, 2), (6, 5), (6, 7)]
                    .into_iter()
                    .map(|(from_col, to_col)| {
                        Action::movement(
                            Color::White,
                            Square {
                                row: 7,
                                col: from_col,
                            },
                            MoveTarget::at(Square {
                                row: 5,
                                col: to_col,
                            }),
                        )
                    }),
            )
            .collect::<Vec<_>>();
        assert_eq!(moves, expected);
        let mut bound = moves[1].clone();
        bound.position_key = Some("not-an-unbound-action".into());
        assert!(matches!(
            v7_opening_validate_move(&state, &bound),
            Err(EngineError::IllegalAction)
        ));
        state.ruleset_id = RULES_VERSION_V7.into();
        state
            .extra
            .insert("cornerKick".into(), json!({"white":true,"black":false}));
        assert!(matches!(
            v7_opening_legal_move_actions(&state),
            Err(EngineError::UnsupportedFeature(_))
        ));
    }

    #[test]
    fn staged_relay_swaps_precede_base_moves_in_source_order() {
        // Frozen seed-19 normal first-play after White uses Relay has 128
        // exchanges followed per piece by the existing 20 orthodox moves.
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.mode = "play".into();
        state.set_flag("relay", Color::White, true);
        let staged = legal_move_candidates_with_relay(&state).unwrap();
        assert_eq!(staged.len(), 148);
        assert_eq!(
            staged
                .iter()
                .filter(|action| action
                    .destination
                    .as_ref()
                    .is_some_and(|to| to.flag("relaySwap")))
                .count(),
            128
        );
        let from = Square { row: 6, col: 0 };
        let mut prefix = (1..8)
            .map(|col| {
                let mut target = MoveTarget::at(Square { row: 6, col });
                target.flags.insert("relaySwap".into(), json!(true));
                Action::movement(Color::White, from, target)
            })
            .collect::<Vec<_>>();
        let mut rook = MoveTarget::at(Square { row: 7, col: 0 });
        rook.flags.insert("relaySwap".into(), json!(true));
        prefix.push(Action::movement(Color::White, from, rook));
        prefix.push(Action::movement(
            Color::White,
            from,
            MoveTarget::at(Square { row: 5, col: 0 }),
        ));
        let mut double = MoveTarget::at(Square { row: 4, col: 0 });
        double
            .flags
            .insert("standardPawnDoubleStep".into(), json!(true));
        prefix.push(Action::movement(Color::White, from, double));
        assert_eq!(&staged[..prefix.len()], prefix.as_slice());
        let pawn = state.at(from).unwrap();
        let swap =
            RelaySwap::from_target(&state, pawn, from, staged[0].destination.as_ref().unwrap())
                .unwrap()
                .unwrap();
        assert_eq!(swap.from, from);
        assert_eq!(swap.to, Square { row: 6, col: 1 });
        assert_eq!(swap.target_id, state.at(swap.to).unwrap().id);
        let original_board = state.board.clone();
        let board = swap.board_after(&state).unwrap();
        let mut expected = original_board.clone();
        let mut acting_pawn = state.at(from).unwrap().clone();
        acting_pawn.moved = true;
        expected[from.row as usize][from.col as usize] = state.at(swap.to).cloned();
        expected[swap.to.row as usize][swap.to.col as usize] = Some(acting_pawn);
        assert_eq!(board, expected);
        assert_eq!(state.board, original_board);
        let reverse_from = Square { row: 7, col: 0 };
        let reverse_to = Square { row: 6, col: 0 };
        let reverse_mover = state.at(reverse_from).unwrap();
        let mut reverse_target = MoveTarget::at(reverse_to);
        reverse_target.flags.insert("relaySwap".into(), json!(true));
        let reverse = RelaySwap::from_target(&state, reverse_mover, reverse_from, &reverse_target)
            .unwrap()
            .unwrap();
        let reverse_board = reverse.board_after(&state).unwrap();
        assert_eq!(reverse_board[6][0].as_ref().unwrap().id, reverse_mover.id);
        assert!(reverse_board[6][0].as_ref().unwrap().moved);
        assert_eq!(reverse_board[7][0].as_ref().unwrap().id, pawn.id);
        assert!(!reverse_board[7][0].as_ref().unwrap().moved);
        let mut replaced_target = state.clone();
        replaced_target.board[swap.to.row as usize][swap.to.col as usize] =
            Some(Piece::new("pawn", Color::White, "replacement"));
        assert!(matches!(
            swap.board_after(&replaced_target),
            Err(EngineError::IllegalAction)
        ));
        let mut forged = staged[0].destination.clone().unwrap();
        forged.flags.insert("capture".into(), json!(true));
        assert!(matches!(
            RelaySwap::from_target(&state, pawn, from, &forged),
            Err(EngineError::IllegalAction)
        ));
        let mut off_axis = MoveTarget::at(Square { row: 7, col: 1 });
        off_axis.flags.insert("relaySwap".into(), json!(true));
        assert!(matches!(
            RelaySwap::from_target(&state, pawn, from, &off_axis),
            Err(EngineError::IllegalAction)
        ));
        let impostor = Piece::new("pawn", Color::White, "different-origin-id");
        assert!(matches!(
            RelaySwap::from_target(
                &state,
                &impostor,
                from,
                staged[0].destination.as_ref().unwrap()
            ),
            Err(EngineError::IllegalAction)
        ));
        // The older v6 candidate path does not inherit the staged v7 swaps.
        assert_eq!(legal_move_candidates(&state).unwrap().len(), 20);
    }

    #[test]
    fn v7_spatial_projection_can_feed_a_bounded_program_cursor() {
        fn deny_capture(_: &crate::SpatialState, _: &str, _: &str, _: bool) -> bool {
            false
        }
        fn deny_shift(_: &crate::SpatialState, _: &str, _: &str) -> bool {
            false
        }
        let mut state = empty_v7();
        state.board[4][4] = Some(Piece::new("rook", Color::White, "rook"));
        let program = MoveProgramSet {
            base: MoveProgram {
                source_id: "probe-only".into(),
                roots: vec![MoveNode {
                    primitive: Primitive::Move,
                    direction: Offset::new(0, 1),
                    max_distance: Some(1),
                    activation_condition: ActivationCondition::Any,
                    activate_at_parent_distance: None,
                    children: Vec::new(),
                }],
            },
            modifiers: Vec::new(),
        };
        let spatial = crate::SpatialState::from_v7_source(&state).unwrap();
        let board = SpatialMoveBoard {
            state: &spatial,
            capture: deny_capture,
            shift: deny_shift,
        };
        let mut cursor = program
            .cursor(&board, "rook", MoveProgramLimits::default())
            .unwrap();
        let page = cursor.next_page(1, 1).unwrap();
        assert_eq!(page.examined, 1);
        assert_eq!(page.raw.len(), 1);
        assert_eq!(page.raw[0].intent.selected, crate::Coord::new(4, 5));

        state
            .extra
            .insert("collapsedCells".into(), json!([{"row":4,"col":5}]));
        let spatial = crate::SpatialState::from_v7_source(&state).unwrap();
        let board = SpatialMoveBoard {
            state: &spatial,
            capture: deny_capture,
            shift: deny_shift,
        };
        let mut cursor = program
            .cursor(&board, "rook", MoveProgramLimits::default())
            .unwrap();
        let page = cursor.next_page(1, 1).unwrap();
        assert!(page.raw.is_empty());
    }

    #[test]
    fn v7_arbitrary_moved_pawn_uses_current_geometry_without_opening_whitelist() {
        let mut state = empty_v7();
        state.mode = "play".into();
        let mut pawn = Piece::new("pawn", Color::White, "relocated-pawn");
        pawn.moved = true;
        state.board[4][3] = Some(pawn);
        state.extra.insert(
            "lastMove".into(),
            json!({"from":{"row":6,"col":7},"to":{"row":4,"col":7}}),
        );
        let before = serde_jcs::to_vec(&state).unwrap();
        let actions = v7_source_ordered_move_candidates(&state, false).unwrap();
        assert_eq!(
            actions,
            vec![Action::movement(
                Color::White,
                Square { row: 4, col: 3 },
                MoveTarget::at(Square { row: 3, col: 3 })
            )]
        );
        assert_eq!(serde_jcs::to_vec(&state).unwrap(), before);
    }

    #[test]
    fn v7_double_check_geometry_precedes_frozen_and_disarm_legal_gates() {
        let mut state = empty_v7();
        let from = Square { row: 4, col: 0 };
        let to = Square { row: 4, col: 7 };
        let mut rook = Piece::new("rook", Color::White, "raw-geometry");
        rook.extra.insert("frozen".into(), json!({"remaining":2}));
        rook.extra.insert("disarmed".into(), json!({"remaining":2}));
        state.board[4][0] = Some(rook.clone());
        state.board[4][7] = Some(Piece::new("king", Color::Black, "enemy-king"));
        assert!(v7_double_check_geometry(&state, &rook, from, to).unwrap());
        assert!(
            v7_legal_move_targets(&state, &rook, from, V7MoveOptions::default())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn v7_automatic_baby_bear_bypasses_ordinary_frozen_gate() {
        let mut state = empty_v7();
        let from = Square { row: 4, col: 4 };
        let mut bear = Piece::new("babyBear", Color::White, "automatic-baby");
        bear.extra.insert("frozen".into(), json!({"remaining":2}));
        state.board[4][4] = Some(bear.clone());
        assert!(
            v7_legal_move_targets(&state, &bear, from, V7MoveOptions::default())
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            v7_baby_bear_automatic_moves(&state, &bear, from)
                .unwrap()
                .len(),
            8
        );
    }

    #[test]
    fn v7_ice_terminal_is_selected_before_majesty_filters_the_destination() {
        let mut state = empty_v7();
        let from = Square { row: 4, col: 4 };
        let mut rook = Piece::new("rook", Color::White, "ice-rook");
        rook.extra.insert("iceSheet".into(), json!({"remaining":3}));
        state.board[4][4] = Some(rook.clone());
        state.board[5][7] = Some(Piece::new("king", Color::Black, "majesty-king"));
        state.set_flag("majesty", Color::Black, true);
        let moves = v7_legal_move_targets(&state, &rook, from, V7MoveOptions::default()).unwrap();
        assert!(!moves.iter().any(|target| target.row == 4 && target.col > 4));
        assert!(
            moves
                .iter()
                .any(|target| target.square() == Square { row: 4, col: 0 })
        );
        state.set_flag("majesty", Color::Black, false);
        let moves = v7_legal_move_targets(&state, &rook, from, V7MoveOptions::default()).unwrap();
        assert!(
            moves
                .iter()
                .any(|target| target.square() == Square { row: 4, col: 7 })
        );
        assert!(
            !moves
                .iter()
                .any(|target| target.square() == Square { row: 4, col: 5 })
        );
    }

    #[test]
    fn v7_inactive_initiative_entries_do_not_remove_ordinary_captures() {
        let mut state = empty_v7();
        let from = Square { row: 4, col: 4 };
        let to = Square { row: 4, col: 5 };
        let attacker = Piece::new("rook", Color::White, "initiative-rook");
        state.board[4][4] = Some(attacker.clone());
        state.board[4][5] = Some(Piece::new("pawn", Color::Black, "initiative-target"));
        for entry in [Value::Null, json!(false), json!(0), json!("")] {
            state
                .extra
                .insert("initiative".into(), json!({"white":entry,"black":null}));
            let before = serde_json::to_value(&state).unwrap();
            assert!(!v7_initiative_capture_locked(&state, Color::White));
            assert!(
                v7_legal_move_targets(&state, &attacker, from, V7MoveOptions::default())
                    .unwrap()
                    .iter()
                    .any(|target| target.square() == to)
            );
            assert_eq!(serde_json::to_value(&state).unwrap(), before);
        }
        state.extra.insert(
            "initiative".into(),
            json!({"white":{"startTurn":0,"limit":7},"black":null}),
        );
        assert!(v7_initiative_capture_locked(&state, Color::White));
        assert!(
            !v7_legal_move_targets(&state, &attacker, from, V7MoveOptions::default())
                .unwrap()
                .iter()
                .any(|target| target.square() == to)
        );
        for entry in [json!({}), json!([])] {
            state
                .extra
                .insert("initiative".into(), json!({"white":entry,"black":null}));
            assert!(v7_initiative_capture_locked(&state, Color::White));
        }
    }

    #[test]
    fn v7_public_hints_keep_physical_large_aliases_and_exclude_neutral_football() {
        let mut state = empty_v7();
        state.mode = "play".into();
        let mut large = Piece::new("bigRook", Color::White, "hint-large");
        large.extra.insert("anchorRow".into(), json!(3.0));
        large.extra.insert("anchorCol".into(), json!(3.0));
        let aliases = [(3, 3), (3, 4), (4, 3), (4, 4)].map(|(row, col)| Square { row, col });
        for at in aliases {
            state.board[at.row as usize][at.col as usize] = Some(large.clone());
        }
        state.board[0][0] = Some(Piece::new("football", PieceColor::Neutral, "hint-ball"));
        state.board[0][1] = Some(Piece::new("rook", Color::White, "hint-kicker"));
        let before = serde_json::to_value(&state).unwrap();
        let hints = v7_ui_move_hints(&state, Color::White).unwrap();
        let body_hints: Vec<_> = hints
            .iter()
            .filter(|(at, _)| aliases.contains(at))
            .collect();
        assert_eq!(
            body_hints.iter().map(|(at, _)| *at).collect::<Vec<_>>(),
            aliases
        );
        assert!(body_hints.windows(2).all(|pair| pair[0].1 == pair[1].1));
        assert!(!hints.iter().any(|(at, _)| *at == Square { row: 0, col: 0 }));
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    fn v7_origin_guard_honors_object_grappler_binding_and_stationary_actions() {
        let mut state = empty_v7();
        let from = Square { row: 4, col: 4 };
        let mut moving = Piece::new("rook", Color::White, "origin-bound");
        moving
            .extra
            .insert("grapplerBound".into(), json!({"untilColor":"black"}));
        state.board[4][4] = Some(moving.clone());
        let mut target = MoveTarget::at(Square { row: 4, col: 5 });
        assert_eq!(
            v7_move_attempt_origin_verdict(&state, &moving, from, &target).unwrap(),
            V7MoveOriginVerdict::Cancel
        );
        target
            .flags
            .insert("setLogDirection".into(), json!({"dr":0,"dc":1}));
        assert_eq!(
            v7_move_attempt_origin_verdict(&state, &moving, from, &target).unwrap(),
            V7MoveOriginVerdict::Allowed
        );
        target.flags.clear();
        target.flags.insert("relaySwap".into(), json!(true));
        moving.extra.remove("grapplerBound");
        let mut other = Piece::new("pawn", Color::White, "recipient-bound");
        other
            .extra
            .insert("grapplerBound".into(), json!({"untilColor":"black"}));
        state.board[4][5] = Some(other);
        assert_eq!(
            v7_move_attempt_origin_verdict(&state, &moving, from, &target).unwrap(),
            V7MoveOriginVerdict::Cancel
        );
    }

    #[test]
    fn v7_castling_checks_pass_threats_and_retains_source_rook_order() {
        let mut state = empty_v7();
        let from = Square { row: 7, col: 4 };
        let king = Piece::new("king", Color::White, "castle-king");
        state.board[7][4] = Some(king.clone());
        state.board[7][0] = Some(Piece::new("rook", Color::White, "queen-rook"));
        state.board[7][7] = Some(Piece::new("rook", Color::White, "king-rook"));
        let options = V7MoveOptions {
            fog_visibility_probe: true,
            ..V7MoveOptions::default()
        };
        let castles = v7_standard_castling(&state, &king, from, options).unwrap();
        assert_eq!(
            castles.iter().map(MoveTarget::square).collect::<Vec<_>>(),
            vec![Square { row: 7, col: 6 }, Square { row: 7, col: 2 }]
        );
        state.board[0][5] = Some(Piece::new("rook", Color::Black, "pass-attacker"));
        assert_eq!(
            v7_standard_castling(&state, &king, from, options)
                .unwrap()
                .iter()
                .map(MoveTarget::square)
                .collect::<Vec<_>>(),
            vec![Square { row: 7, col: 2 }]
        );
    }

    #[test]
    fn v7_campfire_protects_whole_body_and_encouragement_uses_capture_cell() {
        let mut state = empty_v7();
        let target = Piece::new("bigRook", Color::White, "large-body");
        for row in 4..6 {
            for col in 4..6 {
                state.board[row][col] = Some(target.clone());
            }
        }
        let mut fire = Piece::new("trickster", Color::White, "copied-fire");
        fire.extra
            .insert("tricksterMoveType".into(), json!("campfire"));
        state.board[5][6] = Some(fire);
        assert!(v7_encouraged_at(&state, &target, Square { row: 4, col: 4 }));
        state.board[5][6] = None;
        state.board[5][6] = Some(Piece::new("king", Color::White, "encouraging-king"));
        state.set_flag("encouragement", Color::White, true);
        assert!(!v7_encouraged_at(
            &state,
            &target,
            Square { row: 4, col: 4 }
        ));
        assert!(v7_encouraged_at(&state, &target, Square { row: 5, col: 5 }));
    }

    #[test]
    fn v7_public_click_mode_rebinds_without_leaking_private_capture_flags() {
        let mut state = empty_v7();
        state.mode = "play".into();
        let from = Square { row: 4, col: 4 };
        state.board[4][4] = Some(Piece::new("shotgunKing", Color::White, "private-id"));
        let to = Square { row: 2, col: 4 };
        let ordinary = Action::movement(Color::White, from, MoveTarget::at(to));
        let mut shotgun = ordinary.clone();
        let target = shotgun.destination.as_mut().unwrap();
        target.flags.insert("shotgunBlast".into(), json!(true));
        target
            .flags
            .insert("shotgunDirection".into(), json!([-1, 0]));
        target
            .flags
            .insert("captureVictimId".into(), json!("private-victim"));
        let intent = v7_public_move_intents(&state, &shotgun).unwrap().remove(0);
        assert_eq!(
            intent,
            json!({"type":"move","color":"white","from":from,"destination":to,"selectionMode":"shotgun"})
        );
        assert_eq!(
            v7_resolve_public_move_from_candidates(&state, &intent, &[ordinary, shotgun.clone()])
                .unwrap(),
            shotgun
        );
        let mut invalid = intent;
        invalid["selectionMode"] = json!("reload");
        assert!(v7_resolve_public_move_from_candidates(&state, &invalid, &[]).is_err());
    }

    #[test]
    fn v7_additional_and_editor_en_passant_rights_preserve_first_source_match() {
        // Frozen main96367 merges primary/additional before editor entries.
        // Pawn prioritization removes ordinary diagonal moves and appends
        // the two capture descriptors in left/right source order.
        let mut state = empty_v7();
        state.mode = "play".into();
        let from = Square { row: 3, col: 3 };
        let pawn = Piece::new("pawn", Color::White, "pawn");
        state.board[3][3] = Some(pawn.clone());
        state.board[3][2] = Some(Piece::new("pawn", Color::Black, "left"));
        state.board[3][4] = Some(Piece::new("pawn", Color::Black, "right"));
        state.en_passant = Some(
            serde_json::from_value(
                json!({"row":2,"col":2,"capturedRow":3,"capturedCol":2,"color":"black",
            "additional":[{"row":2,"col":2,"capturedRow":3,"capturedCol":2,"color":"black"}]}),
            )
            .unwrap(),
        );
        state.extra.insert(
            "simpleBoardEditor".into(),
            json!({"enabled":true,
            "enPassantAnchor":{"row":2,"col":2,"capturedRow":3,"capturedCol":2,"color":"black"},
            "enPassantStates":[{"row":2,"col":4,"capturedRow":3,"capturedCol":4,"color":"black"}]}),
        );
        let moves = v7_pawn_moves(&state, &pawn, from).unwrap();
        assert_eq!(
            moves.iter().map(MoveTarget::square).collect::<Vec<_>>(),
            vec![
                Square { row: 2, col: 3 },
                Square { row: 2, col: 2 },
                Square { row: 2, col: 4 }
            ]
        );
        assert_eq!(
            moves
                .iter()
                .filter(|target| target.flag("enPassant"))
                .count(),
            2
        );
        state.extra.get_mut("simpleBoardEditor").unwrap()["enPassantAnchor"]["col"] = json!(1);
        assert_eq!(
            v7_pawn_moves(&state, &pawn, from)
                .unwrap()
                .iter()
                .filter(|target| target.flag("enPassant"))
                .count(),
            1
        );
    }

    #[test]
    fn v7_macho_en_passant_preempts_checker_and_brutus_keeps_friendly_royal_witness() {
        let mut state = empty_v7();
        state.mode = "play".into();
        state.extra.insert("machoChess".into(), json!(true));
        let from = Square { row: 3, col: 3 };
        let pawn = Piece::new("pawn", Color::White, "pawn");
        state.board[3][3] = Some(pawn.clone());
        state.board[3][2] = Some(Piece::new("pawn", Color::Black, "victim"));
        state.board[5][7] = Some(Piece::new("checker", Color::White, "checker"));
        state.board[4][6] = Some(Piece::new("rook", Color::Black, "checker-victim"));
        state.en_passant = Some(
            serde_json::from_value(
                json!({"row":2,"col":2,"capturedRow":3,"capturedCol":2,"color":"black"}),
            )
            .unwrap(),
        );
        let moves = v7_legal_move_targets(&state, &pawn, from, V7MoveOptions::default()).unwrap();
        assert_eq!(moves.len(), 1);
        assert!(moves[0].flag("enPassant"));
        let mut betrayal = empty_v7();
        betrayal.mode = "play".into();
        let brutus = Piece::new("brutus", Color::White, "brutus");
        betrayal.board[4][4] = Some(brutus.clone());
        betrayal.board[4][5] = Some(Piece::new("king", Color::White, "king"));
        assert!(
            v7_brutus_royal_capture_moves(&betrayal, &brutus, Square { row: 4, col: 4 })
                .unwrap()
                .iter()
                .any(|target| target.square() == Square { row: 4, col: 5 }
                    && target.flag("brutusBetrayal"))
        );
    }

    #[test]
    fn v7_collapse_depth_and_clipped_quantum_footprint_follow_source_normalization() {
        let mut state = empty_v7();
        state.extra.insert("collapsed".into(), json!(false));
        state.extra.insert("collapseDepth".into(), json!("2.9"));
        assert!(collapsed(&state, Square { row: 1, col: 4 }));
        assert!(!collapsed(&state, Square { row: 2, col: 4 }));
        state.extra.insert("collapseDepth".into(), json!(["1"]));
        assert!(collapsed(&state, Square { row: 0, col: 4 }));
        assert!(!collapsed(&state, Square { row: 1, col: 4 }));
        state
            .extra
            .insert("collapseDepth".into(), json!("Infinity"));
        assert!(collapsed(&state, Square { row: 3, col: 4 }));
        let mut large = Piece::new("bigRook", Color::White, "quantum-body");
        large
            .extra
            .insert("quantum".into(), json!({"row":7,"col":7}));
        state.board[3][3] = Some(large.clone());
        assert!(!quantum_occupied(&state, Square { row: 7, col: 7 }).unwrap());
        large
            .extra
            .insert("quantum".into(), json!({"row":6,"col":6}));
        state.board[3][3] = Some(large);
        assert!(quantum_occupied(&state, Square { row: 7, col: 7 }).unwrap());
    }
}
