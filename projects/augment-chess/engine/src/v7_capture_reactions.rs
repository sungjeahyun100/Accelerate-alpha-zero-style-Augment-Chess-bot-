//! 동결 v7의 일반 포획과 반격. 강제 제거·희생·턴 진행은 공통 owner를 호출한다.
//!
//! main108107 `capturePieceAt`와 main108362 `damageHealthPiece`는 서로 다른
//! 반응 순서를 가진다. 공격자 객체는 착지 전에도 변하므로 호출자가 같은 객체를
//! 계속 사용해야 한다. 포획·방어 공격·반격 진입점은 실패 시 상태와 공격자를
//! 함께 보존하고, 공유 반응 helper는 호출자가 소유한 작업 상태에서 실행한다.

use crate::observation::truth;
use crate::{Color, EngineError, GameState, Piece, PieceColor, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub(crate) struct CaptureOptions {
    pub(crate) force_capture: bool,
    pub(crate) allow_jester: bool,
    pub(crate) suppress_feudal: bool,
    pub(crate) attacker_landing: Option<Square>,
    // 원문의 명시적 빈 배열은 attackerLanding보다 우선한다.
    pub(crate) attacker_landing_cells: Option<Vec<Square>>,
    pub(crate) defer_notation: bool,
    pub(crate) threat_probe: bool,
    pub(crate) threat_source: Option<Value>,
    // withSaturationAttack snapshots the lock for an entire multi-target action.
    pub(crate) saturation_locked: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DefendedAttack {
    NotDefended,
    Protected,
    ShieldBroken,
    Health { removed: Option<Piece> },
}

fn require_v7(state: &GameState, callback: &str) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 {callback} requires the pinned ruleset"
        )));
    }
    Ok(())
}

fn number(value: Option<&Value>) -> Option<f64> {
    crate::card_effects::js_number(value, 0).filter(|value| value.is_finite())
}

fn piece_square(state: &GameState, id: &str) -> Option<Square> {
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .find(|&square| state.at(square).is_some_and(|piece| piece.id == id))
}

fn replace_live_piece(state: &mut GameState, piece: &Piece) {
    for cell in state.board.iter_mut().flatten() {
        if cell.as_ref().is_some_and(|item| item.id == piece.id) {
            *cell = Some(piece.clone());
        }
    }
}

fn replace_captured_piece(state: &mut GameState, owner: Color, piece: &Piece) {
    if piece.id.is_empty() {
        return;
    }
    if let Some(captured) = state
        .captures
        .get_mut(owner)
        .iter_mut()
        .rev()
        .find(|captured| captured.id == piece.id)
    {
        *captured = piece.clone();
    }
}

fn square_name(square: Square) -> String {
    format!("{}{}", char::from(b'a' + square.col), 8 - square.row)
}

fn parse_square(value: Option<&Value>) -> Option<Square> {
    let value = value?;
    let coordinate = |name: &str| {
        number(value.get(name))
            .filter(|n| n.fract() == 0.0 && (0.0..8.0).contains(n))
            .map(|n| n as u8)
    };
    Some(Square {
        row: coordinate("row")?,
        col: coordinate("col")?,
    })
}

fn piece_cells(state: &GameState, piece: &Piece) -> Vec<Square> {
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter(|&square| state.at(square).is_some_and(|item| item.id == piece.id))
        .collect()
}

fn footprint(piece: &Piece, anchor: Square) -> Vec<Square> {
    if piece.is_large() {
        if anchor.row >= 7 || anchor.col >= 7 {
            return Vec::new();
        }
        vec![
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
        ]
    } else {
        vec![anchor]
    }
}

fn install_piece(state: &mut GameState, piece: &mut Piece, anchor: Square) -> Result<()> {
    let cells = footprint(piece, anchor);
    if cells.is_empty() {
        return Err(EngineError::InvalidState(
            "v7 incomplete capture-reaction footprint".into(),
        ));
    }
    if piece.is_large() {
        piece.extra.insert("anchorRow".into(), json!(anchor.row));
        piece.extra.insert("anchorCol".into(), json!(anchor.col));
    }
    for square in cells {
        state.board[square.row as usize][square.col as usize] = Some(piece.clone());
    }
    Ok(())
}

fn privacy_snapshot(state: &GameState, piece: &Piece, square: Square) -> Result<Value> {
    let mut privacy = json!({});
    for color in [Color::White, Color::Black] {
        let visible =
            crate::observation::piece_visible_to_color_at_v7(state, piece, square, color)?;
        privacy[color.as_str()] = json!({"originVisible":visible,
            "typeKnown":visible || piece.color==color || piece.extra.get("hiddenFrom")==Some(&json!(color))});
    }
    Ok(privacy)
}

fn concealed(
    state: &GameState,
    piece: &Piece,
    square: Option<Square>,
    privacy: &Value,
) -> Result<bool> {
    let Some(owner) = piece.color.owner() else {
        return Ok(false);
    };
    let viewer = owner.opponent();
    let Some(square) = square else {
        return Ok(true);
    };
    Ok(privacy[viewer.as_str()]["originVisible"] == json!(false)
        || !crate::observation::piece_visible_to_color_at_v7(state, piece, square, viewer)?)
}

fn combat_concealed(
    state: &GameState,
    victim: &Piece,
    square: Square,
    victim_privacy: &Value,
    attacker: Option<&Piece>,
    attacker_square: Option<Square>,
    attacker_privacy: &Value,
) -> Result<bool> {
    if concealed(state, victim, Some(square), victim_privacy)? {
        return Ok(true);
    }
    if let Some(attacker) = attacker {
        concealed(state, attacker, attacker_square, attacker_privacy)
    } else {
        Ok(false)
    }
}

fn cancel_prophecies(state: &mut GameState) -> Result<()> {
    let Some(prophecies) = state
        .extra
        .get_mut("prophecy")
        .filter(|value| truth(Some(value)))
    else {
        return Ok(());
    };
    let prophecies = prophecies
        .as_object_mut()
        .ok_or_else(|| EngineError::InvalidState("v7 prophecy must be a player map".into()))?;
    for color in [Color::White, Color::Black] {
        if truth(prophecies.get(color.as_str())) {
            prophecies.insert(color.as_str().into(), Value::Null);
        }
    }
    Ok(())
}

pub(crate) fn break_initiative_by_attack(
    state: &mut GameState,
    victim: &Piece,
    capturer: Color,
) -> Result<()> {
    if state
        .extra
        .get("initiative")
        .and_then(|v| v.get(victim.color.as_str()))
        .is_some_and(|entry| truth(Some(entry)) && entry["by"] == json!(capturer))
    {
        state
            .extra
            .get_mut("initiative")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| EngineError::InvalidState("v7 initiative must be a player map".into()))?
            .insert(victim.color.as_str().into(), Value::Null);
        crate::replay::add_log(state, "선공권 제한이 해제되었습니다.".into())?;
    }
    Ok(())
}

/// Source main95126. Ordinary capture grants mana before removal; HP removal
/// grants it afterwards. In particular a captured Wizard receives only the former.
pub(crate) fn grant_wizard_mana(
    state: &mut GameState,
    color: PieceColor,
    amount: i64,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    let wizards = state
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|piece| {
            piece.color == color
                && piece.ability_kind() == "wizard"
                && seen.insert(piece.id.clone())
        })
        .cloned()
        .collect::<Vec<_>>();
    for mut wizard in wizards {
        let maximum = match wizard.extra.get("maxMana") {
            None | Some(Value::Null) => 5.0,
            Some(value) => crate::card_effects::js_number(Some(value), 0).ok_or_else(|| {
                EngineError::InvalidState("v7 Wizard maxMana is not a finite source number".into())
            })?,
        };
        let value = match wizard.extra.get("mana") {
            None | Some(Value::Null) => amount as f64,
            Some(Value::String(text)) => {
                crate::card_effects::js_number(Some(&json!(format!("{text}{amount}"))), 0)
                    .ok_or_else(|| {
                        EngineError::InvalidState(
                            "v7 Wizard mana string addition is not a finite source number".into(),
                        )
                    })?
            }
            Some(Value::Number(value)) => {
                value.as_f64().ok_or_else(|| {
                    EngineError::InvalidState(
                        "v7 Wizard mana cannot be represented as a finite number".into(),
                    )
                })? + amount as f64
            }
            Some(Value::Bool(value)) => f64::from(u8::from(*value)) + amount as f64,
            Some(Value::Array(_) | Value::Object(_)) => {
                return Err(EngineError::UnsupportedFeature(
                    "v7 Wizard mana object ToPrimitive addition".into(),
                ));
            }
        };
        let next = maximum.min(value);
        if !next.is_finite() {
            return Err(EngineError::InvalidState(
                "v7 Wizard mana must be finite".into(),
            ));
        }
        wizard.extra.insert("mana".into(), json!(next));
        replace_live_piece(state, &wizard);
        let owner = color
            .owner()
            .ok_or_else(|| EngineError::InvalidState("v7 Wizard must belong to a player".into()))?;
        crate::replay::add_piece_action_log(
            state,
            &wizard,
            None,
            None,
            format!(
                "{} 마법사가 {}마나를 얻었습니다. ({})",
                crate::replay::label(owner),
                amount,
                next
            ),
        )?;
    }
    Ok(())
}

const IMPERIAL_EXCLUDED: &[&str] = &[
    "bigBishop",
    "",
    "king",
    "royalKnight",
    "shotgunKing",
    "merchant",
    "recruiter",
    "wall",
    "scarecrow",
    "football",
    "blackHole",
    "colossus",
    "bigRook",
    "coffin",
    "log",
    "timeTraveler",
    "wizard",
];

pub(crate) fn learn_imperial_study(
    state: &mut GameState,
    attacker: &mut Piece,
    captured: &Piece,
) -> Result<()> {
    if !state.flag("imperialStudies", attacker.color)
        || !crate::v7_threat::is_royal_identity_v7(state, attacker)
    {
        return Ok(());
    }
    let learned = if captured.kind == "windmill" {
        if captured.extra.get("windmillMode").and_then(Value::as_str) == Some("rook") {
            "rook"
        } else {
            "bishop"
        }
    } else {
        &captured.kind
    };
    if IMPERIAL_EXCLUDED.contains(&learned) {
        return Ok(());
    }
    let mut moves = Vec::<String>::new();
    for value in attacker
        .extra
        .get("imperialMoves")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let text = match value {
            Value::String(text) => text.clone(),
            Value::Null => "null".into(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_string(),
            _ => {
                return Err(EngineError::UnsupportedFeature(
                    "v7 imperialMoves contains an object needing source String coercion".into(),
                ));
            }
        };
        if !IMPERIAL_EXCLUDED.contains(&text.as_str()) && !moves.contains(&text) {
            moves.push(text);
        }
    }
    if moves.iter().any(|value| value == learned) {
        return Ok(());
    }
    moves.push(learned.into());
    attacker.extra.insert("imperialMoves".into(), json!(moves));
    replace_live_piece(state, attacker);
    crate::card_effects::mark_animation(state, attacker)?;
    crate::replay::add_piece_action_log(
        state,
        attacker,
        None,
        None,
        format!(
            "제왕학: {}이 {}의 행마법을 습득했습니다.",
            source_label_or(&attacker.kind, "킹"),
            source_label_or(learned, learned)
        ),
    )
}

pub(crate) fn saturation_locked(state: &GameState, attacker: &Piece) -> bool {
    if let Some((mover_id, locked)) = state.active_v7_saturation_attack.as_ref()
        && mover_id == &attacker.id
    {
        return *locked;
    }
    (truth(state.extra.get("saturationRule")) || truth(attacker.extra.get("potionSaturation")))
        && number(attacker.extra.get("capturesMade"))
            .unwrap_or(0.0)
            .max(0.0)
            >= 3.0
}

fn record_capture_attempt(
    state: &mut GameState,
    attacker: &mut Piece,
    captured: &Piece,
) -> Result<()> {
    if attacker.id == captured.id {
        return Ok(());
    }
    let previous = number(attacker.extra.get("capturesMade"))
        .unwrap_or(0.0)
        .max(0.0);
    let count = previous.floor() + 1.0;
    attacker.extra.insert("capturesMade".into(), json!(count));
    replace_live_piece(state, attacker);
    if truth(state.extra.get("saturationRule")) && previous < 3.0 && count >= 3.0 {
        crate::replay::add_piece_action_log(
            state,
            attacker,
            None,
            None,
            format!(
                "포화: {}이 3개를 잡아 더 이상 기물을 잡을 수 없습니다.",
                source_label_or(&attacker.kind, &attacker.kind)
            ),
        )?;
    }
    Ok(())
}

fn counter_limit(piece: &Piece) -> u8 {
    if matches!(piece.ability_kind(), "bear" | "hedgehog") {
        2
    } else {
        0
    }
}

fn source_label_or<'a>(kind: &'a str, fallback: &'a str) -> &'a str {
    crate::replay::source_piece_label(kind)
        .filter(|label| !label.is_empty())
        .unwrap_or(fallback)
}

fn record_direct_capture(
    state: &mut GameState,
    attacker: &mut Piece,
    captured: &Piece,
) -> Result<()> {
    record_capture_attempt(state, attacker, captured)?;
    if attacker.id == captured.id {
        return Ok(());
    }
    let surviving_bear = counter_limit(captured) != 0
        && number(captured.extra.get("bearRetaliationsRemaining")).unwrap_or(0.0) > 0.0;
    if !surviving_bear {
        let previous = number(attacker.extra.get("totalCaptures"))
            .unwrap_or(0.0)
            .max(0.0);
        attacker
            .extra
            .insert("totalCaptures".into(), json!(previous + 1.0));
        if state.flag("assembly", attacker.color)
            && attacker
                .color
                .owner()
                .is_some_and(|owner| captured.color == owner.opponent())
            && ((attacker.kind == "rook" && captured.kind == "bishop")
                || (attacker.kind == "bishop" && captured.kind == "rook"))
        {
            attacker
                .extra
                .insert("assemblyPromotionPending".into(), json!(true));
        }
        replace_live_piece(state, attacker);
    }
    Ok(())
}

/// main65792. Bear revival delays this entire bundle; HP removal invokes it
/// immediately. Non-capture callbacks must use their own medium-memory policy.
pub(crate) fn record_new_card_capture_reactions(
    state: &mut GameState,
    captured: &Piece,
    capturer: Color,
    after_turn_boundary: bool,
) -> Result<()> {
    record_new_card_capture_reactions_with_options(
        state,
        captured,
        capturer,
        false,
        after_turn_boundary,
    )
}

pub(crate) fn record_new_card_capture_reactions_with_options(
    state: &mut GameState,
    captured: &Piece,
    capturer: Color,
    non_capture: bool,
    after_turn_boundary: bool,
) -> Result<()> {
    if !non_capture
        && state.extra.contains_key("mediumMovement")
        && !matches!(
            captured.kind.as_str(),
            "wall" | "football" | "blackHole" | "black-hole"
        )
    {
        let memory = if captured.kind == "medium" {
            state
                .extra
                .get("mediumMovement")
                .cloned()
                .unwrap_or(Value::Null)
        } else if captured.kind == "parrot" {
            state
                .extra
                .get("parrotMovement")
                .and_then(|v| v.get(captured.color.as_str()))
                .cloned()
                .unwrap_or(Value::Null)
        } else {
            remembered_capture_movement(captured)?
        };
        let memory = if truth(Some(&memory)) {
            memory
        } else {
            Value::Null
        };
        state.extra.insert("mediumMovement".into(), memory);
    }
    crate::transition::grant_vigilance_protection(state, captured)?;
    let Some(owner) = captured.color.owner() else {
        return Ok(());
    };
    crate::replay::normalize_color_booleans(state, "magicGirlSurge");
    state.set_flag("magicGirlSurge", owner, true);
    crate::replay::normalize_color_booleans(state, "magicGirlSurgeRefreshPending");
    if owner == state.turn
        && state.board.iter().flatten().flatten().any(|piece| {
            piece.color == owner
                && matches!(piece.ability_kind(), "magicGirl" | "parrot" | "medium")
        })
    {
        state.set_flag("magicGirlSurgeRefreshPending", owner, true);
    }
    crate::v7_piece_lifecycle::schedule_undead_resurrection(
        state,
        captured,
        capturer,
        after_turn_boundary,
    )
}

fn remembered_capture_movement(piece: &Piece) -> Result<Value> {
    let mut kind = String::new();
    for character in piece.ability_kind().chars() {
        if character.is_ascii_uppercase() {
            kind.push('-');
            kind.push(character.to_ascii_lowercase());
        } else {
            kind.push(character);
        }
    }
    // main15919: ability=Parrot returns its explicit previous argument. The
    // capture callback supplies no previous memory; a Trickster with Parrot
    // therefore clears Medium memory rather than reading parrotMovement.
    if kind == "parrot" {
        return Ok(Value::Null);
    }
    let mut memory = json!({"type":kind});
    for key in ["logDirection", "windmillMode"] {
        if let Some(value) = piece.extra.get(key).filter(|v| truth(Some(v))) {
            memory[key] = value.clone();
        }
    }
    if kind == "trickster"
        && let Some(value) = piece
            .extra
            .get("tricksterMoveType")
            .filter(|v| truth(Some(v)))
    {
        let value = value.as_str().ok_or_else(|| {
            EngineError::UnsupportedFeature(
                "v7 remembered Trickster movement requires source String coercion".into(),
            )
        })?;
        let mut type_name = String::new();
        for character in value.chars() {
            if character.is_ascii_uppercase() {
                type_name.push('-');
                type_name.push(character.to_ascii_lowercase());
            } else {
                type_name.push(character);
            }
        }
        memory["type"] = json!(type_name);
    }
    Ok(memory)
}

/// Original ordinary-capture helper, called before the mover lands. None can
/// mean a block, an evasion, or armed parry; callers must inspect the settled
/// target cell rather than turn None into a fabricated captured piece.
pub(crate) fn capture_at(
    state: &mut GameState,
    attacker: &mut Piece,
    square: Square,
    options: &CaptureOptions,
) -> Result<Option<Piece>> {
    crate::legal_profile::measure("move_capture", || {
        capture_at_profiled(state, attacker, square, options)
    })
}

pub(crate) fn capture_at_profiled(
    state: &mut GameState,
    attacker: &mut Piece,
    square: Square,
    options: &CaptureOptions,
) -> Result<Option<Piece>> {
    let capturer = attacker.color.owner().ok_or(EngineError::WrongActor)?;
    capture_at_with_optional_attacker(state, square, capturer, Some(attacker), options)
}

pub(crate) fn capture_at_with_optional_attacker(
    state: &mut GameState,
    square: Square,
    capturer: Color,
    attacker: Option<&mut Piece>,
    options: &CaptureOptions,
) -> Result<Option<Piece>> {
    crate::legal_profile::measure("move_capture_inner", || {
        capture_at_with_optional_attacker_profiled(state, square, capturer, attacker, options)
    })
}

pub(crate) fn capture_at_with_optional_attacker_profiled(
    state: &mut GameState,
    square: Square,
    capturer: Color,
    attacker: Option<&mut Piece>,
    options: &CaptureOptions,
) -> Result<Option<Piece>> {
    require_v7(state, "capturePieceAt")?;
    let mut next = state.clone();
    let mut active = attacker.as_deref().cloned();
    let captured =
        capture_at_inner_optional(&mut next, square, capturer, active.as_mut(), options)?;
    *state = next;
    if let Some(attacker) = attacker
        && let Some(active) = active
    {
        *attacker = active;
    }
    Ok(captured)
}

fn capture_at_inner(
    state: &mut GameState,
    attacker: &mut Piece,
    square: Square,
    options: &CaptureOptions,
) -> Result<Option<Piece>> {
    let capturer = attacker.color.owner().ok_or(EngineError::WrongActor)?;
    capture_at_inner_optional(state, square, capturer, Some(attacker), options)
}

fn capture_at_inner_optional(
    state: &mut GameState,
    square: Square,
    capturer: Color,
    mut attacker: Option<&mut Piece>,
    options: &CaptureOptions,
) -> Result<Option<Piece>> {
    let mut context = options.clone();
    context.threat_probe |= state.threat_probe_depth > 0;
    let options = &context;
    if square.row >= 8 || square.col >= 8 {
        return Err(EngineError::IllegalAction);
    }
    observe_capture_quantum(state, square, capturer)?;
    let Some(mut captured) = state.at(square).cloned() else {
        return Ok(None);
    };
    if !options.force_capture
        && direct_capture_blocked(state, attacker.as_deref(), &captured, options)
    {
        return Ok(None);
    }
    if !options.force_capture
        && attacker.is_some()
        && try_evade_capture(state, &mut captured, square, capturer, options)?
    {
        return Ok(None);
    }
    let parry_armed = if !options.force_capture
        && let Some(active) = attacker.as_deref()
    {
        arm_parry(state, &mut captured, square, capturer, active, options)?
    } else {
        false
    };
    if parry_armed {
        if let Some(active) = attacker.as_deref_mut() {
            record_capture_attempt(state, active, &captured)?;
        }
        crate::transition::clear_piece(state, &captured.id);
        if options.attacker_landing.is_none() {
            let color = attacker.as_deref().and_then(|piece| piece.color.owner());
            let id = attacker.as_deref().map(|piece| piece.id.clone());
            resolve_pending_bear_retaliations(state, color, id.as_deref())?;
        }
        return Ok(None);
    }
    break_initiative_by_attack(state, &captured, capturer)?;
    crate::transition::add_capture_type(state, "capturedTypes", capturer, &captured.kind)?;
    crate::transition::add_capture_type(state, "turnCaptures", capturer, &captured.kind)?;
    if let Some(active) = attacker.as_deref_mut() {
        learn_imperial_study(state, active, &captured)?;
    }
    grant_wizard_mana(state, captured.color, 1)?;
    if let Some(active) = attacker.as_deref_mut()
        && let Some(live) = piece_square(state, &active.id)
            .and_then(|at| state.at(at))
            .cloned()
    {
        *active = live;
    }
    // JS captured aliases the still-live Wizard whose mana changed above.
    if let Some(live) = state.at(square).filter(|piece| piece.id == captured.id) {
        captured = live.clone();
    }
    crate::transition::clear_piece(state, &captured.id);
    let threat = options
        .threat_source
        .clone()
        .unwrap_or_else(|| json!({"attacker":attacker.as_deref()}));
    crate::v7_threat::mark_king_threat_removal_cause(
        state,
        &captured,
        square,
        &threat,
        options.threat_probe,
    )?;
    if !options.suppress_feudal {
        trigger_feudal_contract(state, &captured, square, attacker.as_deref(), options)?;
    }
    let bear_armed = if let Some(active) = attacker.as_deref() {
        arm_bear(state, &captured, square, capturer, active)?
    } else {
        false
    };
    let trojan_armed = if let Some(active) = attacker.as_deref() {
        arm_trojan(state, &mut captured, square, active, options)?
    } else {
        false
    };
    grant_blood_for_direct_capture(state, &captured, attacker.as_deref(), square, options)?;
    if let Some(active) = attacker.as_deref_mut() {
        record_direct_capture(state, active, &captured)?;
    }
    if truth(captured.extra.get("poisonedPawn"))
        && let Some(active) = attacker
            .as_deref_mut()
            .filter(|active| captured.color != active.color)
    {
        active.extra.insert("poisonStunTurns".into(), json!(3));
        active
            .extra
            .insert("poisonStunColor".into(), json!(capturer));
        replace_live_piece(state, active);
        crate::replay::add_piece_action_log(
            state,
            active,
            None,
            None,
            format!(
                "독이 든 폰: {}이 2수 동안 움직일 수 없습니다.",
                source_label_or(&active.kind, &active.kind)
            ),
        )?;
    }
    if crate::v7_board_hazards::try_revive_blood_moon_lord(state, &captured)? {
        crate::v7_capture_objectives::check_campaign_objectives(state)?;
        return Ok(Some(captured));
    }
    if !bear_armed {
        record_new_card_capture_reactions(state, &captured, capturer, false)?;
    }
    cancel_prophecies(state)?;
    state.captures.get_mut(capturer).push(captured.clone());
    crate::flow::mark_progress(state);
    let winner = if captured.color == capturer {
        capturer.opponent()
    } else {
        capturer
    };
    crate::transition::resolve_royal_capture(state, &captured, winner)?;
    if let Some(owner) = captured.color.owner() {
        crate::flow::check_democracy_defeat(state, owner, winner, "모든 폰이 잡혔습니다.")?;
    }
    if !bear_armed {
        crate::v7_board_hazards::resolve_reaper_nearby_deaths_with_context(
            state,
            &[crate::v7_board_hazards::EnvironmentalRemoval {
                piece: captured.clone(),
                square,
                capture_owner: capturer,
            }],
            attacker.as_deref_mut(),
            options.attacker_landing,
            options.defer_notation,
        )?;
    }
    let attacker_color = attacker.as_deref().and_then(|piece| piece.color.owner());
    let attacker_id = attacker.as_deref().map(|piece| piece.id.clone());
    if bear_armed && options.attacker_landing.is_none() {
        resolve_pending_bear_retaliations(state, attacker_color, attacker_id.as_deref())?;
    }
    if trojan_armed && options.attacker_landing.is_none() {
        resolve_pending_trojan_horse_retaliations(state, attacker_color, attacker_id.as_deref())?;
    }
    resolve_calling_card_capture(state, &captured, attacker.as_deref(), square)?;
    let landing_cells = options
        .attacker_landing_cells
        .clone()
        .unwrap_or_else(|| options.attacker_landing.into_iter().collect());
    crate::v7_board_automata::transfer_crown_after_capture(
        state,
        &mut captured,
        attacker.as_deref(),
        &landing_cells,
    )?;
    replace_captured_piece(state, capturer, &captured);
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    if let Some(active) = attacker
        && let Some(live) = piece_square(state, &active.id)
            .and_then(|at| state.at(at))
            .cloned()
    {
        *active = live;
    }
    Ok(Some(captured))
}

/// movePiece's defense branch is a stationary action. The caller owns its
/// clock, move notation, extra-action settlement, and endMove invocation.
pub(crate) fn attack_defended_piece(
    state: &mut GameState,
    attacker: &mut Piece,
    square: Square,
    source_label: &str,
    options: &CaptureOptions,
) -> Result<DefendedAttack> {
    require_v7(state, "damageHealthPiece/breakShield")?;
    let mut next = state.clone();
    let mut active = attacker.clone();
    let Some(mut victim) = next.at(square).cloned() else {
        return Ok(DefendedAttack::NotDefended);
    };
    if options.force_capture {
        return Ok(DefendedAttack::NotDefended);
    }
    let protected = is_protected_piece(&next, &victim, &active);
    if protected || is_hp_piece(&next, &victim) {
        break_initiative_by_attack(
            &mut next,
            &victim,
            active.color.owner().ok_or(EngineError::WrongActor)?,
        )?;
    }
    let result = if protected && truth(victim.extra.get("shielded")) {
        break_shield(
            &mut next,
            &mut victim,
            active.color.owner().ok_or(EngineError::WrongActor)?,
        )?;
        DefendedAttack::ShieldBroken
    } else if protected {
        DefendedAttack::Protected
    } else if is_hp_piece(&next, &victim) {
        let removed = damage_health_piece(
            &mut next,
            &mut active,
            &mut victim,
            square,
            source_label,
            options,
        )?;
        DefendedAttack::Health { removed }
    } else {
        DefendedAttack::NotDefended
    };
    *state = next;
    *attacker = active;
    Ok(result)
}

pub(crate) fn is_hp_piece(state: &GameState, piece: &Piece) -> bool {
    matches!(
        piece.kind.as_str(),
        "colossus" | "bigRook" | "bigBishop" | "shotgunKing"
    ) || crate::v7_threat::is_royal_identity_v7(state, piece)
        && truth(piece.extra.get("undergroundBunker"))
        && number(piece.extra.get("hp")).is_some()
}

/// Source isProtectedFromDirectCapture. Callers with an encouragement gate
/// check protection first, encouragement second, and HP damage afterwards.
pub(crate) fn is_protected_piece(_state: &GameState, victim: &Piece, attacker: &Piece) -> bool {
    victim.kind != "scarecrow"
        && (truth(victim.extra.get("shielded"))
            || truth(victim.extra.get("protected"))
            || number(
                victim
                    .extra
                    .get("vigilanceProtection")
                    .and_then(|v| v.get("remaining")),
            )
            .unwrap_or(0.0)
                > 0.0
            || victim.kind == "football"
            || victim.kind == "monster" && attacker.kind != "darkWizard")
}

/// main109278. Only the shield is changed; clock and attack settlement belong
/// to the caller. The visual uses hiddenFrom/camouflage, independently of fog.
pub(crate) fn break_shield(
    state: &mut GameState,
    victim: &mut Piece,
    sound_color: Color,
) -> Result<()> {
    if !truth(victim.extra.get("shielded")) {
        return Ok(());
    }
    let cells = piece_cells(state, victim);
    let reference = cells.first().copied();
    let hidden = reference
        .and_then(|at| crate::observation::piece_hidden_from_v7(state, victim, at))
        .map_or("", Color::as_str);
    crate::replay::queue_visual(
        state,
        json!({"type":"shield-break","color":sound_color,
        "targetColor":victim.color,"targetType":victim.kind,"targetId":victim.id,"cells":cells,"to":reference,"hiddenFrom":hidden}),
    )?;
    victim.extra.insert("shielded".into(), json!(false));
    replace_live_piece(state, victim);
    Ok(())
}

fn damage_health_piece(
    state: &mut GameState,
    attacker: &mut Piece,
    victim: &mut Piece,
    square: Square,
    source: &str,
    options: &CaptureOptions,
) -> Result<Option<Piece>> {
    let capturer = attacker.color.owner().ok_or(EngineError::WrongActor)?;
    damage_health_piece_inner(
        state,
        victim,
        square,
        capturer,
        Some(attacker),
        source,
        options,
    )
}

pub(crate) fn damage_health_piece_with_optional_attacker(
    state: &mut GameState,
    square: Square,
    capturer: Color,
    attacker: Option<&mut Piece>,
    source: &str,
    options: &CaptureOptions,
) -> Result<Option<Piece>> {
    require_v7(state, "damageHealthPiece")?;
    let mut next = state.clone();
    let mut active = attacker.as_deref().cloned();
    let Some(mut victim) = next.at(square).cloned() else {
        return Ok(None);
    };
    let removed = damage_health_piece_inner(
        &mut next,
        &mut victim,
        square,
        capturer,
        active.as_mut(),
        source,
        options,
    )?;
    *state = next;
    if let Some(attacker) = attacker
        && let Some(active) = active
    {
        *attacker = active;
    }
    Ok(removed)
}

fn damage_health_piece_inner(
    state: &mut GameState,
    victim: &mut Piece,
    square: Square,
    capturer: Color,
    mut attacker: Option<&mut Piece>,
    source: &str,
    options: &CaptureOptions,
) -> Result<Option<Piece>> {
    let mut context = options.clone();
    context.threat_probe |= state.threat_probe_depth > 0;
    let options = &context;
    if attacker.as_deref().is_some_and(|active| {
        !options.force_capture && nullification_blocks(victim, active)
            || options
                .saturation_locked
                .unwrap_or_else(|| saturation_locked(state, active))
    }) {
        return Ok(None);
    }
    state.extra.insert("lastMove".into(), Value::Null);
    state.extra.insert("accelerationTrail".into(), Value::Null);
    let victim_privacy = privacy_snapshot(state, victim, square)?;
    let attacker_square = attacker
        .as_deref()
        .and_then(|active| piece_square(state, &active.id));
    let attacker_privacy = attacker_square
        .zip(attacker.as_deref())
        .map(|(at, active)| privacy_snapshot(state, active, at))
        .transpose()?
        .unwrap_or(Value::Null);
    let source_hidden = if let (Some(at), Some(active), Some(owner)) =
        (attacker_square, attacker.as_deref(), victim.color.owner())
    {
        !crate::observation::piece_visible_to_color_at_v7(state, active, at, owner)?
    } else {
        false
    };
    let source_label = if source_hidden { "공격" } else { source };
    let hp = number(victim.extra.get("hp").filter(|v| !v.is_null()))
        .or_else(|| number(victim.extra.get("maxHp").filter(|v| !v.is_null())))
        .unwrap_or(1.0);
    if hp - 1.0 <= 0.0
        && attacker.is_some()
        && try_evade_capture(state, victim, square, capturer, &CaptureOptions::default())?
    {
        let combat_hidden = combat_concealed(
            state,
            victim,
            square,
            &victim_privacy,
            attacker.as_deref(),
            attacker_square,
            &attacker_privacy,
        )?;
        crate::replay::add_log(
            state,
            if combat_hidden {
                "전투가 발생했습니다.".into()
            } else {
                format!(
                    "{}: {}이 치명타를 회피했습니다.",
                    source_label,
                    source_label_or(&victim.kind, &victim.kind)
                )
            },
        )?;
        return Ok(None);
    }
    let remaining = (hp - 1.0).max(0.0);
    victim.extra.insert("hp".into(), json!(remaining));
    replace_live_piece(state, victim);
    let maximum = victim
        .extra
        .get("maxHp")
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string())
        })
        .unwrap_or_else(|| "undefined".into());
    let combat_hidden = combat_concealed(
        state,
        victim,
        square,
        &victim_privacy,
        attacker.as_deref(),
        attacker_square,
        &attacker_privacy,
    )?;
    crate::replay::add_log(
        state,
        if combat_hidden {
            "전투가 발생했습니다.".into()
        } else {
            format!(
                "{}: {} HP -1 ({}/{})",
                source_label,
                crate::replay::source_piece_label(&victim.kind).unwrap_or("undefined"),
                remaining,
                maximum
            )
        },
    )?;
    if remaining > 0.0 {
        return Ok(None);
    }
    crate::transition::clear_piece(state, &victim.id);
    cancel_prophecies(state)?;
    state.captures.get_mut(capturer).push(victim.clone());
    crate::transition::add_capture_type(state, "capturedTypes", capturer, &victim.kind)?;
    crate::transition::add_capture_type(state, "turnCaptures", capturer, &victim.kind)?;
    if let Some(active) = attacker.as_deref_mut() {
        learn_imperial_study(state, active, victim)?;
    }
    grant_wizard_mana(state, victim.color, 1)?;
    if let Some(active) = attacker.as_deref_mut() {
        if let Some(live) = piece_square(state, &active.id)
            .and_then(|at| state.at(at))
            .cloned()
        {
            *active = live;
        }
        record_direct_capture(state, active, victim)?;
    }
    record_new_card_capture_reactions(state, victim, capturer, false)?;
    let threat = json!({"attacker":attacker.as_deref(),"label":source});
    crate::v7_threat::mark_king_threat_removal_cause(
        state,
        victim,
        square,
        &threat,
        options.threat_probe,
    )?;
    let winner = if victim.color == capturer {
        capturer.opponent()
    } else {
        capturer
    };
    crate::transition::resolve_royal_capture(state, victim, winner)?;
    crate::v7_board_hazards::resolve_reaper_nearby_deaths_with_context(
        state,
        &[crate::v7_board_hazards::EnvironmentalRemoval {
            piece: victim.clone(),
            square,
            capture_owner: capturer,
        }],
        attacker.as_deref_mut(),
        None,
        options.defer_notation,
    )?;
    resolve_calling_card_capture(state, victim, attacker.as_deref(), square)?;
    crate::flow::mark_progress(state);
    let color_label = victim
        .color
        .owner()
        .map(crate::replay::label)
        .unwrap_or("undefined");
    let combat_hidden = combat_concealed(
        state,
        victim,
        square,
        &victim_privacy,
        attacker.as_deref(),
        attacker_square,
        &attacker_privacy,
    )?;
    crate::replay::add_log(
        state,
        if combat_hidden {
            "기물이 쓰러졌습니다.".into()
        } else {
            format!(
                "{} {}이 쓰러졌습니다.",
                color_label,
                crate::replay::source_piece_label(&victim.kind).unwrap_or("undefined")
            )
        },
    )?;
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(Some(victim.clone()))
}

fn observe_capture_quantum(state: &mut GameState, square: Square, observer: Color) -> Result<()> {
    // capture_at의 소유 clone에서 실행하므로 내부 관측 함수를 재사용한다.
    // 성공 여부로 원래 기물을 추정하지 않고 이후 board cell을 다시 읽는다.
    crate::v7_quantum_state::observe_quantum_at_in_place(state, square, Some(observer))?;
    Ok(())
}

fn nullification_identity(piece: &Piece) -> String {
    let mut kind = String::new();
    let mut capitalize = false;
    for letter in piece.kind.chars() {
        if letter == '-' {
            capitalize = true;
        } else if capitalize {
            kind.extend(letter.to_uppercase());
            capitalize = false;
        } else {
            kind.push(letter);
        }
    }
    if truth(piece.extra.get("regencyHeir"))
        || truth(piece.extra.get("crownRoyal"))
        || truth(piece.extra.get("editorRoyal"))
        || matches!(
            kind.as_str(),
            "king" | "royalKnight" | "shotgunKing" | "darkWizard" | "merchant"
        )
    {
        "king".into()
    } else {
        kind
    }
}

fn nullification_blocks(victim: &Piece, attacker: &Piece) -> bool {
    victim.kind != "scarecrow"
        && truth(victim.extra.get("nullification"))
        && victim.color != attacker.color
        && nullification_identity(victim) == nullification_identity(attacker)
}

pub(crate) fn nullification_blocks_optional(victim: &Piece, attacker: Option<&Piece>) -> bool {
    attacker.is_some_and(|active| nullification_blocks(victim, active))
}

fn overwhelm_king(piece: &Piece) -> bool {
    truth(piece.extra.get("regencyHeir"))
        || piece.extra.get("crownRoyal") == Some(&json!(true))
        || matches!(
            piece.kind.as_str(),
            "king" | "royalKnight" | "shotgunKing" | "merchant" | "timeTraveler" | "vampireLord"
        )
}

fn direct_capture_blocked(
    state: &GameState,
    attacker: Option<&Piece>,
    victim: &Piece,
    options: &CaptureOptions,
) -> bool {
    if truth(victim.extra.get("submerged")) || crate::movement::frozen(victim) {
        return true;
    }
    let Some(attacker) = attacker else {
        return victim.kind == "football"
            || victim.kind == "monster"
            || victim.kind == "jester" && !options.allow_jester;
    };
    if nullification_blocks(victim, attacker)
        || options
            .saturation_locked
            .unwrap_or_else(|| saturation_locked(state, attacker))
    {
        return true;
    }
    let armistice = state.extra.get("armistice");
    let remaining = armistice.and_then(|v| v.get("remaining")).or(armistice);
    if number(remaining).unwrap_or(0.0).floor() > 0.0
        && victim.color.owner().is_some()
        && victim.color != attacker.color
    {
        return true;
    }
    if victim.color != attacker.color
        && state.flag("overwhelm", victim.color)
        && overwhelm_king(attacker)
        && (overwhelm_king(victim)
            || victim.kind == "queen" && victim.extra.get("regencyHeir") != Some(&json!(true)))
    {
        return true;
    }
    let time_campaign = state
        .extra
        .get("campaign")
        .and_then(|v| v.get("setup"))
        .and_then(Value::as_str)
        == Some("timeTraveler");
    let phase = |piece: &Piece| {
        if piece.extra.get("timePhase").and_then(Value::as_str) == Some("past") {
            "past"
        } else {
            "future"
        }
    };
    if time_campaign
        && !matches!(victim.kind.as_str(), "wall" | "football")
        && phase(attacker) != phase(victim)
    {
        return true;
    }
    if attacker.kind == "timeTraveler"
        && state
            .extra
            .get("campaign")
            .and_then(|v| v.get("timeTraveler"))
            .and_then(|v| v.get("attackEnabledFor"))
            .and_then(Value::as_str)
            != Some(attacker.color.as_str())
    {
        return true;
    }
    victim.kind == "football"
        || victim.kind == "monster" && attacker.kind != "darkWizard"
        || victim.kind == "jester" && !options.allow_jester
}

fn chain_allows(state: &GameState, piece: &Piece, destination: Square) -> Result<bool> {
    if piece.id.is_empty() {
        return Ok(true);
    }
    for bond in crate::card_effects::normalize_chain_bonds(state.extra.get("chainBonds"))? {
        let first = bond["aId"].as_str().unwrap_or("");
        let second = bond["bId"].as_str().unwrap_or("");
        let partner = if first == piece.id {
            second
        } else if second == piece.id {
            first
        } else {
            continue;
        };
        if piece_square(state, partner).is_some_and(|square| {
            destination.row.abs_diff(square.row) > 2 || destination.col.abs_diff(square.col) > 2
        }) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn reserved_evasion_square(state: &GameState, square: Square) -> bool {
    state
        .extra
        .get("pendingScarecrows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|entry| !truth(entry.get("pieceId")) && parse_square(Some(entry)) == Some(square))
        || state
            .extra
            .get("pendingLobsters")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|entry| parse_square(Some(entry)) == Some(square))
}

fn quantum_destination_available(state: &GameState, piece: &Piece, destination: Square) -> bool {
    let cells = footprint(piece, destination);
    if cells.is_empty() {
        return false;
    }
    cells.into_iter().all(|square| {
        let same = state.at(square).is_some_and(|item| item.id == piece.id);
        if (reserved_evasion_square(state, square) || state.at(square).is_some()) && !same {
            return false;
        }
        !state.board.iter().flatten().flatten().any(|other| {
            other.id != piece.id
                && parse_square(other.extra.get("quantum"))
                    .is_some_and(|anchor| footprint(other, anchor).contains(&square))
        })
    })
}

fn reset_moving_after_evasion(state: &mut GameState, piece: &Piece) -> Result<()> {
    let Some(owner) = piece.color.owner().filter(|_| !piece.id.is_empty()) else {
        return Ok(());
    };
    if !truth(state.extra.get("moving")) {
        state.extra.insert("moving".into(),json!({"white":{"enabled":false,"pieceId":"","count":0},"black":{"enabled":false,"pieceId":"","count":0}}));
    }
    let old = state
        .extra
        .get("moving")
        .and_then(|v| v.get(owner.as_str()));
    let enabled = truth(old.and_then(|v| v.get("enabled")));
    let piece_id = old
        .and_then(|v| v.get("pieceId"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let count = number(old.and_then(|v| v.get("count")))
        .unwrap_or(0.0)
        .floor()
        .clamp(0.0, 4.0);
    let entry = if enabled && piece_id == piece.id {
        json!({"enabled":true,"pieceId":"","count":0})
    } else {
        json!({"enabled":enabled,"pieceId":piece_id,"count":count})
    };
    state
        .extra
        .get_mut("moving")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState("v7 moving must be a color map".into()))?
        .insert(owner.as_str().into(), entry);
    Ok(())
}

pub(crate) fn evasion_destination_candidates(
    state: &GameState,
    victim: &Piece,
    square: Square,
    options: &CaptureOptions,
) -> Result<Vec<Square>> {
    let origin = if victim.is_large() {
        parse_square(Some(
            &json!({"row":victim.extra.get("anchorRow"),"col":victim.extra.get("anchorCol")}),
        ))
        .ok_or_else(|| EngineError::InvalidState("v7 evasion large piece has no anchor".into()))?
    } else {
        square
    };
    let blocked = options
        .attacker_landing_cells
        .clone()
        .unwrap_or_else(|| options.attacker_landing.into_iter().collect());
    let black_holes = state
        .extra
        .get("blackHole")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| parse_square(Some(v)))
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    for dr in -1_i8..=1 {
        for dc in -1_i8..=1 {
            if dr == 0 && dc == 0 {
                continue;
            }
            let Some(destination) = origin.offset(dr, dc) else {
                continue;
            };
            if !chain_allows(state, victim, destination)?
                || !quantum_destination_available(state, victim, destination)
            {
                continue;
            }
            let cells = footprint(victim, destination);
            if cells
                .iter()
                .any(|&cell| crate::movement::collapsed(state, cell))
                || black_holes.contains(&destination)
                || cells.iter().any(|cell| blocked.contains(cell))
            {
                continue;
            }
            if truth(state.extra.get("monochromeChess")) && truth(victim.extra.get("monoShade")) {
                let shade = if (destination.row + destination.col).is_multiple_of(2) {
                    "light"
                } else {
                    "dark"
                };
                if victim.extra.get("monoShade").and_then(Value::as_str) != Some(shade) {
                    continue;
                }
            }
            candidates.push(destination);
        }
    }
    Ok(candidates)
}

fn try_evade_capture(
    state: &mut GameState,
    victim: &mut Piece,
    square: Square,
    capturer: Color,
    options: &CaptureOptions,
) -> Result<bool> {
    if victim.kind == "scarecrow" || !truth(victim.extra.get("evasion")) || victim.color == capturer
    {
        return Ok(false);
    }
    let origin = if victim.is_large() {
        parse_square(Some(
            &json!({"row":victim.extra.get("anchorRow"),"col":victim.extra.get("anchorCol")}),
        ))
        .ok_or_else(|| EngineError::InvalidState("v7 evasion large piece has no anchor".into()))?
    } else {
        piece_square(state, &victim.id).unwrap_or(square)
    };
    let candidates = evasion_destination_candidates(state, victim, origin, options)?;
    if candidates.is_empty() {
        return Ok(false);
    }
    let privacy = privacy_snapshot(state, victim, origin)?;
    let selected = (state.rng.sample()? * candidates.len() as f64).floor() as usize;
    let destination = *candidates.get(selected).ok_or_else(|| {
        EngineError::InvalidState("v7 evasion RNG selection out of bounds".into())
    })?;
    state.rng.record_last_probability(
        1.0 / candidates.len() as f64,
        "source Evasion escape destination",
    )?;
    record_semantic_probability(state, 1.0 / candidates.len() as f64)?;
    victim.extra.shift_remove("evasion");
    reset_moving_after_evasion(state, victim)?;
    crate::transition::clear_piece(state, &victim.id);
    victim.moved = true;
    crate::card_effects::note_ultimatum_movement(state, victim)?;
    install_piece(state, victim, destination)?;
    crate::card_effects::mark_animation(state, victim)?;
    crate::replay::queue_visual(
        state,
        json!({"type":"evasion","color":victim.color,"from":origin,"to":destination}),
    )?;
    victim
        .color
        .owner()
        .ok_or_else(|| EngineError::InvalidState("v7 evasion piece lacks a player color".into()))?;
    crate::v7_threat::reconcile_move_replay_capture_v7(state)?;
    crate::replay::add_piece_action_log(
        state,
        victim,
        Some(destination),
        Some(&privacy),
        format!(
            "회피: {}의 {}이 {}로 피했습니다.",
            square_name(origin),
            source_label_or(&victim.kind, &victim.kind),
            square_name(destination)
        ),
    )?;
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(true)
}

fn queue_entry(state: &mut GameState, field: &str, entry: Value) -> Result<()> {
    let entries = state.extra.entry(field).or_insert_with(|| json!([]));
    if !entries.is_array() {
        *entries = json!([]);
    }
    let entries = entries
        .as_array_mut()
        .ok_or_else(|| EngineError::InvalidState(format!("v7 {field} must be an array")))?;
    if entries.len() >= 256 {
        return Err(EngineError::InvalidState(format!(
            "v7 {field} exceeds 256 entries"
        )));
    }
    entries.push(entry);
    Ok(())
}

fn reaction_piece_id(state: &GameState, piece: &Piece) -> Result<String> {
    if piece.id.is_empty() {
        Ok(crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?.to_string())
    } else {
        Ok(piece.id.clone())
    }
}

fn arm_parry(
    state: &mut GameState,
    victim: &mut Piece,
    square: Square,
    capturer: Color,
    attacker: &Piece,
    options: &CaptureOptions,
) -> Result<bool> {
    if victim.kind == "scarecrow"
        || options.threat_probe
        || !truth(victim.extra.get("parry"))
        || victim.color.owner().is_none()
        || attacker.id.is_empty()
        || attacker.color.owner().is_none()
        || attacker.color == victim.color
    {
        return Ok(false);
    }
    let chance = number(victim.extra.get("parry").and_then(|v| v.get("chance")))
        .filter(|chance| *chance != 0.0)
        .unwrap_or(0.4);
    victim.extra.shift_remove("parry");
    replace_live_piece(state, victim);
    let roll = state.rng.sample()?;
    if !roll.is_finite() || roll < 0.0 || roll >= chance.clamp(0.0, 1.0) {
        state
            .rng
            .record_last_probability(1.0 - chance.clamp(0.0, 1.0), "source Parry failure")?;
        record_semantic_probability(state, 1.0 - chance.clamp(0.0, 1.0))?;
        crate::replay::add_piece_action_log(
            state,
            victim,
            None,
            None,
            "패링에 실패했습니다.".into(),
        )?;
        return Ok(false);
    }
    state
        .rng
        .record_last_probability(chance.clamp(0.0, 1.0), "source Parry success")?;
    record_semantic_probability(state, chance.clamp(0.0, 1.0))?;
    let origin = piece_square(state, &attacker.id).unwrap_or(square);
    let restore_cells = if victim.is_large() {
        piece_cells(state, victim)
    } else {
        Vec::new()
    };
    queue_entry(
        state,
        "pendingBearRetaliations",
        json!({
            "id":format!("parry-{}-{}",reaction_piece_id(state,victim)?,attacker.id),"parry":true,
            "color":victim.color,"square":square,"attackerId":attacker.id,"attackerColor":attacker.color,
            "capturedBy":capturer,"counterDestination":origin,"restoreCells":restore_cells,"bear":victim,"remaining":0
        }),
    )?;
    Ok(true)
}

fn arm_bear(
    state: &mut GameState,
    victim: &Piece,
    square: Square,
    capturer: Color,
    attacker: &Piece,
) -> Result<bool> {
    let remaining = number(victim.extra.get("bearRetaliationsRemaining"))
        .unwrap_or(0.0)
        .max(0.0);
    if counter_limit(victim) == 0
        || remaining <= 0.0
        || victim.color.owner().is_none()
        || attacker.id.is_empty()
        || attacker.color.owner().is_none()
        || attacker.color == victim.color
    {
        return Ok(false);
    }
    let origin = piece_square(state, &attacker.id);
    queue_entry(
        state,
        "pendingBearRetaliations",
        json!({
            "id":format!("bear-{}-{}",reaction_piece_id(state,victim)?,attacker.id),"color":victim.color,
            "square":square,"attackerId":attacker.id,"attackerColor":attacker.color,"capturedBy":capturer,
            "counterDestination":origin,"bear":victim,"remaining":remaining-1.0
        }),
    )?;
    Ok(true)
}

fn suppressed(piece: &Piece, options: &CaptureOptions) -> bool {
    options.threat_probe && truth(piece.extra.get("kingThreatSuppressed"))
}

fn arm_trojan(
    state: &mut GameState,
    victim: &mut Piece,
    square: Square,
    attacker: &Piece,
    options: &CaptureOptions,
) -> Result<bool> {
    if suppressed(victim, options)
        || victim.kind != "knight"
        || victim.color.owner().is_none()
        || !truth(victim.extra.get("trojanHorse"))
        || attacker.id.is_empty()
        || attacker.color.owner().is_none()
        || attacker.color == victim.color
    {
        return Ok(false);
    }
    victim.extra.shift_remove("trojanHorse");
    queue_entry(
        state,
        "pendingTrojanHorse",
        json!({
            "id":format!("trojan-{}-{}",reaction_piece_id(state,victim)?,attacker.id),"color":victim.color,
            "square":square,"attackerId":attacker.id,"attackerColor":attacker.color
        }),
    )?;
    Ok(true)
}

fn trigger_feudal_contract(
    state: &mut GameState,
    victim: &Piece,
    square: Square,
    attacker: Option<&Piece>,
    options: &CaptureOptions,
) -> Result<()> {
    if suppressed(victim, options) || !truth(victim.extra.get("feudalContractId")) {
        return Ok(());
    }
    let Some(contracts) = state
        .extra
        .get("feudalContracts")
        .and_then(Value::as_array)
        .cloned()
        .filter(|v| !v.is_empty())
    else {
        return Ok(());
    };
    let contract = contracts
        .iter()
        .find(|entry| {
            entry.get("id") == victim.extra.get("feudalContractId")
                && entry["pawnId"] == json!(victim.id)
        })
        .cloned();
    let remaining = contracts
        .into_iter()
        .filter(|entry| entry.get("id") != victim.extra.get("feudalContractId"))
        .collect::<Vec<_>>();
    state
        .extra
        .insert("feudalContracts".into(), json!(remaining));
    let Some(contract) = contract else {
        return Ok(());
    };
    let Some(origin) = contract["guardianId"]
        .as_str()
        .and_then(|id| piece_square(state, id))
    else {
        return Ok(());
    };
    let mut guardian = state
        .at(origin)
        .cloned()
        .ok_or_else(|| EngineError::InvalidState("v7 feudal guardian disappeared".into()))?;
    if guardian.color != victim.color
        || guardian.is_large()
        || guardian.kind == "wall"
        || suppressed(&guardian, options)
    {
        return Ok(());
    }
    if let Some(landing) = options.attacker_landing
        && let Some(attacker) = attacker
    {
        state.extra.insert(
            "pendingFeudalStrike".into(),
            json!({"guardianId":guardian.id,"pawnSquare":square,
            "attackerId":attacker.id,"attackerLanding":landing}),
        );
        return Ok(());
    }
    let privacy = privacy_snapshot(state, &guardian, origin)?;
    crate::transition::clear_piece(state, &guardian.id);
    guardian.moved = true;
    crate::card_effects::note_ultimatum_movement(state, &mut guardian)?;
    install_piece(state, &mut guardian, square)?;
    crate::replay::add_piece_action_log(
        state,
        &guardian,
        Some(square),
        Some(&privacy),
        format!(
            "봉건 계약: {}이 {}로 이동했습니다.",
            crate::replay::source_piece_label(&guardian.kind).unwrap_or("undefined"),
            square_name(square)
        ),
    )
}

/// movePiece main92342, before Trojan's immediate landing recapture.
pub(crate) fn resolve_pending_feudal_strike(
    state: &mut GameState,
    attacker: &mut Piece,
    landing: Square,
) -> Result<()> {
    let Some(strike) = state
        .extra
        .get("pendingFeudalStrike")
        .cloned()
        .filter(|entry| {
            entry["attackerId"] == json!(attacker.id)
                && parse_square(entry.get("attackerLanding")) == Some(landing)
        })
    else {
        return Ok(());
    };
    let mut next = state.clone();
    next.extra.insert("pendingFeudalStrike".into(), Value::Null);
    let Some(origin) = strike["guardianId"]
        .as_str()
        .and_then(|id| piece_square(&next, id))
    else {
        *state = next;
        return Ok(());
    };
    let mut guardian = next.at(origin).cloned().ok_or_else(|| {
        EngineError::InvalidState("v7 pending feudal guardian disappeared".into())
    })?;
    if guardian.color == attacker.color
        || guardian.is_large()
        || guardian.kind == "wall"
        || next.threat_probe_depth > 0 && truth(guardian.extra.get("kingThreatSuppressed"))
    {
        *state = next;
        return Ok(());
    }
    let guardian_privacy = privacy_snapshot(&next, &guardian, origin)?;
    let attacker_privacy = privacy_snapshot(&next, attacker, landing)?;
    if saturation_locked(&next, &guardian) {
        crate::replay::add_piece_action_log(
            &mut next,
            &guardian,
            Some(origin),
            Some(&guardian_privacy),
            format!(
                "포화: {}이 포화되어 봉건 계약의 반격을 수행하지 못했습니다.",
                source_label_or(&guardian.kind, &guardian.kind)
            ),
        )?;
        *state = next;
        return Ok(());
    }
    crate::transition::clear_piece(&mut next, &guardian.id);
    let options = CaptureOptions {
        attacker_landing: Some(landing),
        allow_jester: true,
        suppress_feudal: true,
        threat_source: Some(json!({"attacker":guardian,"origin":origin,"label":"봉건 계약"})),
        ..CaptureOptions::default()
    };
    let removed = capture_at_inner(&mut next, &mut guardian, landing, &options)?;
    let moved_as_type = guardian.kind.clone();
    if !crate::transition::finalize_immediate_reaper_execution(
        &mut next,
        &mut guardian,
        origin,
        &moved_as_type,
    )? {
        install_piece(&mut next, &mut guardian, landing)?;
    }
    guardian.moved = true;
    crate::card_effects::note_ultimatum_movement(&mut next, &mut guardian)?;
    replace_live_piece(&mut next, &guardian);
    crate::card_effects::mark_animation(&mut next, &guardian)?;
    let hidden = concealed(&next, &guardian, Some(landing), &guardian_privacy)?
        || concealed(&next, attacker, Some(landing), &attacker_privacy)?;
    let message = if hidden {
        "기물이 행동했습니다.".into()
    } else if removed.is_some() {
        format!(
            "봉건 계약: {}이 {}을 자동으로 잡았습니다.",
            crate::replay::source_piece_label(&guardian.kind).unwrap_or("undefined"),
            crate::replay::source_piece_label(&attacker.kind).unwrap_or("undefined")
        )
    } else {
        format!(
            "봉건 계약: {}이 회피해 {}이 빈칸으로 이동했습니다.",
            crate::replay::source_piece_label(&attacker.kind).unwrap_or("undefined"),
            crate::replay::source_piece_label(&guardian.kind).unwrap_or("undefined")
        )
    };
    crate::replay::add_log(&mut next, message)?;
    if removed.as_ref().is_some_and(|piece| {
        truth(piece.extra.get("explosive"))
            && !(next.threat_probe_depth > 0 && truth(piece.extra.get("kingThreatSuppressed")))
    }) {
        crate::v7_board_hazards::explode_at(&mut next, landing, "자폭병")?;
    }
    crate::v7_threat::check_racing_kings_v7(&mut next)?;
    *state = next;
    Ok(())
}

fn grant_blood_for_direct_capture(
    state: &mut GameState,
    captured: &Piece,
    attacker: Option<&Piece>,
    square: Square,
    options: &CaptureOptions,
) -> Result<()> {
    crate::v7_campaign::grant_blood_for_direct_capture(
        state,
        captured,
        attacker,
        square,
        options.attacker_landing,
    )
}

fn guard_like(state: &GameState, piece: &Piece) -> bool {
    piece.ability_kind() == "guard"
        || piece.ability_kind() == "revolvingDoor"
            && crate::movement::v7_uses_revolving_door_guard(state)
}

pub(crate) fn resolve_calling_card_capture(
    state: &mut GameState,
    captured: &Piece,
    attacker: Option<&Piece>,
    _origin: Square,
) -> Result<Option<Piece>> {
    let Some(notice) = captured
        .extra
        .get("callingCard")
        .filter(|notice| truth(Some(notice)))
    else {
        return Ok(None);
    };
    if state.mode == "gameover" {
        return Ok(None);
    }
    let mut seen = BTreeSet::new();
    let remaining = (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter_map(|square| state.at(square).map(|piece| (square, piece.clone())))
        .filter(|(_, piece)| {
            piece.color == captured.color
                && piece.id != captured.id
                && seen.insert(piece.id.clone())
        })
        .collect::<Vec<_>>();
    let non_royal = remaining
        .iter()
        .filter(|(_, piece)| {
            attacker.is_none_or(|active| piece.id != active.id)
                && !crate::v7_threat::is_royal_identity_v7(state, piece)
                && !matches!(piece.kind.as_str(), "wall" | "football" | "blackHole")
                && !guard_like(state, piece)
                && piece.ability_kind() != "jester"
        })
        .cloned()
        .collect::<Vec<_>>();
    let candidates = if !non_royal.is_empty() {
        non_royal
    } else if remaining
        .iter()
        .any(|(_, piece)| !crate::v7_threat::is_royal_identity_v7(state, piece))
    {
        Vec::new()
    } else {
        remaining
            .into_iter()
            .filter(|(_, piece)| attacker.is_none_or(|active| piece.id != active.id))
            .collect()
    };
    if candidates.is_empty() {
        return Ok(None);
    }
    let selected = (state.rng.sample()? * candidates.len() as f64).floor() as usize;
    let (square, _) = candidates.get(selected).ok_or_else(|| {
        EngineError::InvalidState("v7 Calling Card RNG selection out of bounds".into())
    })?;
    state
        .rng
        .record_last_probability(1.0 / candidates.len() as f64, "source Calling Card victim")?;
    record_semantic_probability(state, 1.0 / candidates.len() as f64)?;
    let owner = parse_color(notice.get("by"), "callingCard.by")?;
    let threat = json!({"label":"예고장"});
    let options = crate::transition::ForceRemovalOptions {
        suppress_calling_card: true,
        threat_source: Some(&threat),
        ..Default::default()
    };
    let removed =
        crate::transition::force_remove_piece_at_with_options(state, *square, owner, &options)?;
    if let Some(removed) = &removed {
        let name = square_name(*square);
        let label = source_label_or(&removed.kind, &removed.kind);
        crate::replay::add_piece_action_log(
            state,
            removed,
            Some(*square),
            None,
            format!("예고장: {}의 {}이 제거되었습니다.", name, label),
        )?;
        crate::replay::queue_special_effect_notation(
            state,
            owner,
            &format!("예고장 {name}"),
            &format!(
                "{} 예고장으로 {}의 {} 제거",
                crate::replay::label(owner),
                name,
                label
            ),
        )?;
    }
    Ok(removed)
}

fn parse_color(value: Option<&Value>, field: &str) -> Result<Color> {
    match value.and_then(Value::as_str) {
        Some("white") => Ok(Color::White),
        Some("black") => Ok(Color::Black),
        _ => Err(EngineError::InvalidState(format!(
            "v7 {field} must name a player"
        ))),
    }
}

fn take_due_reactions(
    state: &mut GameState,
    field: &str,
    attacker_color: Option<Color>,
    attacker_id: Option<&str>,
) -> Result<Vec<Value>> {
    let Some(entries) = state.extra.get(field).and_then(Value::as_array).cloned() else {
        return Ok(Vec::new());
    };
    if entries.len() > 256 {
        return Err(EngineError::InvalidState(format!(
            "v7 {field} exceeds 256 entries"
        )));
    }
    let (due, future): (Vec<_>, Vec<_>) = entries.into_iter().partition(|entry| {
        attacker_color.is_none_or(|color| entry["attackerColor"] == json!(color))
            && attacker_id
                .filter(|id| !id.is_empty())
                .is_none_or(|id| entry["attackerId"].as_str() == Some(id))
    });
    if !due.is_empty() {
        state.extra.insert(field.into(), json!(future));
    }
    Ok(due)
}

/// main107855. Remove each attacker at most once but allocate one pawn for
/// every open vacated Knight square, including multiple entries for one ID.
pub(crate) fn resolve_pending_trojan_horse_retaliations(
    state: &mut GameState,
    attacker_color: Option<Color>,
    attacker_id: Option<&str>,
) -> Result<usize> {
    require_v7(state, "resolvePendingTrojanHorseRetaliations")?;
    let mut next = state.clone();
    let due = take_due_reactions(&mut next, "pendingTrojanHorse", attacker_color, attacker_id)?;
    let mut removed_attackers = BTreeSet::new();
    let mut resolved = 0;
    for entry in due {
        let owner = parse_color(entry.get("color"), "pendingTrojanHorse.color")?;
        let target_color = parse_color(
            entry.get("attackerColor"),
            "pendingTrojanHorse.attackerColor",
        )?;
        let id = entry["attackerId"].as_str().unwrap_or("");
        if removed_attackers.insert(id.to_owned())
            && let Some(square) = piece_square(&next, id)
            && next.at(square).is_some_and(|piece| {
                piece.color == target_color
                    && !matches!(piece.kind.as_str(), "colossus" | "shotgunKing")
            })
        {
            let threat = json!({"label":"트로이 목마"});
            let options = crate::transition::ForceRemovalOptions {
                threat_source: Some(&threat),
                ..Default::default()
            };
            crate::transition::force_remove_piece_at_with_options(
                &mut next, square, owner, &options,
            )?;
        }
        let Some(square) = parse_square(entry.get("square")) else {
            continue;
        };
        if !crate::movement::open_placement(&next, square, Some(owner))? {
            continue;
        }
        let mut pawn = crate::opening::spawn(&mut next, owner, "pawn")?;
        pawn.moved = true;
        pawn.extra
            .insert("origin".into(), json!(square_name(square)));
        install_piece(&mut next, &mut pawn, square)?;
        crate::card_effects::mark_animation(&mut next, &pawn)?;
        crate::replay::add_piece_action_log(
            &mut next,
            &pawn,
            Some(square),
            None,
            format!(
                "트로이 목마: {}에 폰이 나타나 공격자를 되잡았습니다.",
                square_name(square)
            ),
        )?;
        resolved += 1;
    }
    *state = next;
    Ok(resolved)
}

/// main107713. Parry uses the Bear queue but has separate restoration,
/// capture credit, movement lock, Crown and notation rules.
pub(crate) fn resolve_pending_bear_retaliations(
    state: &mut GameState,
    attacker_color: Option<Color>,
    attacker_id: Option<&str>,
) -> Result<usize> {
    require_v7(state, "resolvePendingBearRetaliations")?;
    let mut next = state.clone();
    let due = take_due_reactions(
        &mut next,
        "pendingBearRetaliations",
        attacker_color,
        attacker_id,
    )?;
    let mut removed_attackers = BTreeSet::<String>::new();
    let mut removed_pieces = BTreeMap::<String, Piece>::new();
    let mut restored = 0;
    for entry in &due {
        let color = parse_color(entry.get("color"), "pendingBearRetaliations.color")?;
        let target_color = parse_color(
            entry.get("attackerColor"),
            "pendingBearRetaliations.attackerColor",
        )?;
        let captured_by = parse_color(
            entry.get("capturedBy"),
            "pendingBearRetaliations.capturedBy",
        )?;
        let snapshot: Piece =
            serde_json::from_value(entry["bear"].clone()).map_err(EngineError::serialization)?;
        if !snapshot.id.is_empty() {
            next.captures
                .get_mut(captured_by)
                .retain(|piece| piece.id != snapshot.id);
        }
        let source = parse_square(entry.get("square"));
        let id = entry["attackerId"].as_str().unwrap_or("").to_owned();
        let attacker_square = piece_square(&next, &id);
        let mut destination = parse_square(entry.get("counterDestination")).or(attacker_square);
        let parry = truth(entry.get("parry"));
        if removed_attackers.insert(id.clone())
            && let Some(square) = attacker_square
            && let Some(attacker) = next
                .at(square)
                .cloned()
                .filter(|piece| piece.color == target_color)
        {
            removed_pieces.insert(id.clone(), attacker.clone());
            let effect_label = if parry {
                "패링".into()
            } else {
                format!("{} 역습", source_label_or(&snapshot.kind, "기물"))
            };
            let threat = json!({"attacker":snapshot,"origin":source,"label":effect_label});
            let options = crate::transition::ForceRemovalOptions {
                count_as_capture: true,
                attacker: Some(&attacker),
                threat_source: Some(&threat),
                ..Default::default()
            };
            crate::transition::force_remove_piece_at_with_options(
                &mut next, square, color, &options,
            )?;
            if !parry
                && crate::v7_threat::is_v7_threat_royal(&next, &attacker)
                && piece_square(&next, &id).is_none()
            {
                let probe = next.threat_probe_depth > 0;
                crate::v7_threat::mark_king_threat_effect_cause(&mut next, &effect_label, probe)?;
            }
        }
        let original_cells = if parry && snapshot.is_large() {
            entry
                .get("restoreCells")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|value| parse_square(Some(value)))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let destination_cells = destination
            .filter(|_| original_cells.len() == 4)
            .map(|anchor| footprint(&snapshot, anchor))
            .unwrap_or_default();
        let large_cells = if destination_cells.len() == 4
            && destination_cells
                .iter()
                .all(|&square| next.at(square).is_none())
        {
            destination_cells
        } else {
            original_cells
        };
        if large_cells.len() == 4 {
            if large_cells.iter().any(|&square| next.at(square).is_some()) {
                continue;
            }
            let anchor = Square {
                row: large_cells
                    .iter()
                    .map(|square| square.row)
                    .min()
                    .ok_or_else(|| EngineError::InvalidState("v7 empty parry footprint".into()))?,
                col: large_cells
                    .iter()
                    .map(|square| square.col)
                    .min()
                    .ok_or_else(|| EngineError::InvalidState("v7 empty parry footprint".into()))?,
            };
            let Some(source) = source else {
                return Err(EngineError::InvalidState(
                    "v7 large parry has invalid source square".into(),
                ));
            };
            let mut bear = snapshot.clone();
            bear.extra.insert("anchorRow".into(), json!(anchor.row));
            bear.extra.insert("anchorCol".into(), json!(anchor.col));
            increment_counter(&mut bear, "totalCaptures");
            bear.moved = true;
            for &square in &large_cells {
                next.board[square.row as usize][square.col as usize] = Some(bear.clone());
            }
            crate::card_effects::mark_animation(&mut next, &bear)?;
            crate::card_effects::set_last_move(
                &mut next, source, anchor, "capture", color, "", None,
            )?;
            crate::replay::queue_visual(
                &mut next,
                json!({"type":"parry","color":bear.color,"cells":[source,anchor],"from":source,"to":anchor}),
            )?;
            crate::replay::queue_special_effect_notation(
                &mut next,
                color,
                "패링",
                &format!(
                    "{} 기물이 패링으로 공격자를 제거",
                    crate::replay::label(color)
                ),
            )?;
            if let Some(removed) = removed_pieces.get(&id) {
                learn_imperial_study(&mut next, &mut bear, removed)?;
            }
            replace_live_piece(&mut next, &bear);
            crate::replay::add_piece_action_log(
                &mut next,
                &bear,
                Some(anchor),
                None,
                format!(
                    "패링 성공: 공격자를 제거하고 {}에 남았습니다.",
                    square_name(anchor)
                ),
            )?;
            restored += 1;
            continue;
        }
        if destination.is_none_or(|square| next.at(square).is_some())
            && source.is_some_and(|square| next.at(square).is_none())
        {
            destination = source;
        }
        let Some(destination) = destination.filter(|&square| next.at(square).is_none()) else {
            continue;
        };
        let Some(source) = source else {
            return Err(EngineError::InvalidState(
                "v7 Bear retaliation has invalid source square".into(),
            ));
        };
        let mut bear = snapshot.clone();
        if !parry && !matches!(bear.kind.as_str(), "trickster" | "hedgehog") {
            bear.kind = "bear".into();
        }
        increment_counter(&mut bear, "totalCaptures");
        if !parry {
            let remaining = number(entry.get("remaining")).unwrap_or(0.0).max(0.0);
            bear.extra
                .insert("bearRetaliationsRemaining".into(), json!(remaining));
            bear.extra.insert(
                "bearMoveLockedUntilTurn".into(),
                json!(u64::from(*next.turns_taken.get(color)) + 1),
            );
        }
        bear.moved = true;
        // Source non-large branch assigns precisely one cell.
        next.board[destination.row as usize][destination.col as usize] = Some(bear.clone());
        crate::v7_board_automata::restore_crown_holder_after_retaliation(
            &mut next,
            &mut bear,
            destination,
        )?;
        crate::card_effects::mark_animation(&mut next, &bear)?;
        crate::card_effects::set_last_move(
            &mut next,
            source,
            destination,
            "capture",
            color,
            "",
            None,
        )?;
        let cells = if source == destination {
            vec![source]
        } else {
            vec![source, destination]
        };
        crate::card_effects::track_acceleration_trail(&mut next, color, &cells, true, "")?;
        if parry {
            crate::replay::queue_visual(
                &mut next,
                json!({"type":"parry","color":bear.color,"cells":[source,destination],"from":source,"to":destination}),
            )?;
            if let Some(removed) = removed_pieces.get(&id) {
                learn_imperial_study(&mut next, &mut bear, removed)?;
            }
        } else {
            crate::replay::queue_visual(
                &mut next,
                json!({"type":"mistake","label":"COUNTER","color":bear.color,"cells":[source,destination],"from":source,"to":destination}),
            )?;
        }
        let description = if parry {
            format!(
                "{} 기물이 패링으로 공격자를 제거",
                crate::replay::label(color)
            )
        } else {
            format!(
                "{} {}이 {}에서 {}로 이동해 공격자를 역습",
                crate::replay::label(color),
                source_label_or(&bear.kind, "곰"),
                square_name(source),
                square_name(destination)
            )
        };
        crate::replay::queue_special_effect_notation(
            &mut next,
            color,
            if parry { "패링" } else { "역습" },
            &description,
        )?;
        let message = if parry {
            format!(
                "패링 성공: {}로 이동해 공격자를 제거했습니다.",
                square_name(destination)
            )
        } else {
            format!(
                "{}의 역습: {}로 이동해 공격자를 제거했습니다. ({}/{}회 남음)",
                source_label_or(&bear.kind, "곰"),
                square_name(destination),
                number(bear.extra.get("bearRetaliationsRemaining")).unwrap_or(0.0),
                counter_limit(&bear)
            )
        };
        replace_live_piece(&mut next, &bear);
        crate::replay::add_piece_action_log(&mut next, &bear, Some(destination), None, message)?;
        restored += 1;
    }
    for entry in &due {
        if truth(entry.get("parry")) {
            continue;
        }
        let captured: Piece =
            serde_json::from_value(entry["bear"].clone()).map_err(EngineError::serialization)?;
        if piece_square(&next, &captured.id).is_some() {
            continue;
        }
        let capturer = parse_color(
            entry.get("capturedBy"),
            "pendingBearRetaliations.capturedBy",
        )?;
        record_new_card_capture_reactions(&mut next, &captured, capturer, false)?;
    }
    if restored > 0 {
        crate::v7_board_automata::break_out_of_range_chain_bonds(&mut next)?;
    }
    *state = next;
    Ok(restored)
}

fn increment_counter(piece: &mut Piece, name: &str) {
    let previous = number(piece.extra.get(name)).unwrap_or(0.0).max(0.0);
    piece.extra.insert(name.into(), json!(previous + 1.0));
}

fn record_semantic_probability(state: &mut GameState, mass: f64) -> Result<()> {
    if let Some(probability) = &mut state.semantic_chance_probability {
        if !mass.is_finite() || mass <= 0.0 || mass > 1.0 {
            return Err(EngineError::InvalidState(
                "v7 capture reaction realized an invalid semantic probability".into(),
            ));
        }
        *probability *= mass;
        if !probability.is_finite() || *probability <= 0.0 {
            return Err(EngineError::InvalidState(
                "v7 capture reaction semantic probability underflow".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;

    fn callback_return(kind: &str, value: Value) -> Value {
        json!({"kind":kind,"value":value})
    }

    fn run_receipt_stage(state: &mut GameState, stage: &str) -> Result<Value> {
        let landing = Square { row: 3, col: 3 };
        match stage {
            "capture" => {
                let at = piece_square(state, "attacker").ok_or_else(|| {
                    EngineError::InvalidState("capture receipt has no attacker".into())
                })?;
                let mut attacker = state.at(at).cloned().ok_or_else(|| {
                    EngineError::InvalidState("capture receipt attacker disappeared".into())
                })?;
                let result = capture_at(
                    state,
                    &mut attacker,
                    landing,
                    &CaptureOptions {
                        attacker_landing: Some(landing),
                        ..Default::default()
                    },
                )?;
                let kind = if result.is_some() { "object" } else { "null" };
                Ok(callback_return(
                    kind,
                    serde_json::to_value(result).map_err(EngineError::serialization)?,
                ))
            }
            "break-shield" => {
                let mut victim = state.at(landing).cloned().ok_or_else(|| {
                    EngineError::InvalidState("shield receipt has no victim".into())
                })?;
                break_shield(state, &mut victim, Color::White)?;
                Ok(callback_return("undefined", Value::Null))
            }
            "damage-health" => {
                let at = piece_square(state, "attacker").ok_or_else(|| {
                    EngineError::InvalidState("HP receipt has no attacker".into())
                })?;
                let mut attacker = state.at(at).cloned().ok_or_else(|| {
                    EngineError::InvalidState("HP receipt attacker disappeared".into())
                })?;
                let removed = damage_health_piece_with_optional_attacker(
                    state,
                    landing,
                    Color::White,
                    Some(&mut attacker),
                    "룩의 공격",
                    &CaptureOptions {
                        attacker_landing: Some(landing),
                        ..Default::default()
                    },
                )?;
                Ok(callback_return("boolean", json!(removed.is_some())))
            }
            "manual-landing" => {
                let from = piece_square(state, "attacker").ok_or_else(|| {
                    EngineError::InvalidState("manual landing receipt has no attacker".into())
                })?;
                let attacker = state.at(from).cloned().ok_or_else(|| {
                    EngineError::InvalidState("manual landing attacker disappeared".into())
                })?;
                if state.at(landing).is_some() {
                    return Err(EngineError::InvalidState(
                        "manual receipt landing is occupied".into(),
                    ));
                }
                state.board[from.row as usize][from.col as usize] = None;
                state.board[landing.row as usize][landing.col as usize] = Some(attacker.clone());
                Ok(callback_return(
                    "object",
                    serde_json::to_value(attacker).map_err(EngineError::serialization)?,
                ))
            }
            "resolve-parry" | "resolve-bear" => {
                let count =
                    resolve_pending_bear_retaliations(state, Some(Color::White), Some("attacker"))?;
                Ok(callback_return("number", json!(count)))
            }
            "resolve-trojan" => {
                let count = resolve_pending_trojan_horse_retaliations(
                    state,
                    Some(Color::White),
                    Some("attacker"),
                )?;
                Ok(callback_return("number", json!(count)))
            }
            "resolve-feudal" => {
                let at = piece_square(state, "attacker").ok_or_else(|| {
                    EngineError::InvalidState("feudal receipt has no attacker".into())
                })?;
                let mut attacker = state.at(at).cloned().ok_or_else(|| {
                    EngineError::InvalidState("feudal receipt attacker disappeared".into())
                })?;
                resolve_pending_feudal_strike(state, &mut attacker, landing)?;
                Ok(callback_return("undefined", Value::Null))
            }
            "external-attacker-removal-and-blockers" => {
                // 명시적인 합성 준비 단계다. 정상 이동/강제 제거를 증명하지 않는다.
                let at = piece_square(state, "attacker").ok_or_else(|| {
                    EngineError::InvalidState("synthetic removal receipt has no attacker".into())
                })?;
                state.board[at.row as usize][at.col as usize] = None;
                for (square, id, origin) in [
                    (Square { row: 2, col: 2 }, "origin-blocker", "c6"),
                    (landing, "capture-blocker", "d5"),
                ] {
                    let mut pawn = crate::opening::spawn(state, Color::Black, "pawn")?;
                    pawn.id = id.into();
                    pawn.extra.insert("origin".into(), json!(origin));
                    state.board[square.row as usize][square.col as usize] = Some(pawn);
                }
                Ok(callback_return(
                    "object",
                    json!({"removedAttackerId":"attacker"}),
                ))
            }
            other => Err(EngineError::InvalidState(format!(
                "unreviewed capture receipt stage {other}"
            ))),
        }
    }

    fn first_receipt_difference(
        expected: &Value,
        actual: &Value,
        path: &str,
        depth: usize,
    ) -> Option<String> {
        if depth > 64 {
            return Some(format!("{path}: comparison depth exceeds 64"));
        }
        let left = match serde_jcs::to_vec(expected) {
            Ok(value) => value,
            Err(error) => return Some(format!("{path}: source JCS error: {error}")),
        };
        let right = match serde_jcs::to_vec(actual) {
            Ok(value) => value,
            Err(error) => return Some(format!("{path}: native JCS error: {error}")),
        };
        if left == right {
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
                    let next = format!("{path}.{key}");
                    match (left.get(key), right.get(key)) {
                        (Some(a), Some(b)) => {
                            if let Some(difference) =
                                first_receipt_difference(a, b, &next, depth + 1)
                            {
                                return Some(difference);
                            }
                        }
                        (Some(_), None) => return Some(format!("{next}: missing native field")),
                        (None, Some(_)) => return Some(format!("{next}: extra native field")),
                        (None, None) => {}
                    }
                }
                None
            }
            (Value::Array(left), Value::Array(right)) => {
                if left.len() != right.len() {
                    return Some(format!(
                        "{path}: source length {}, native length {}",
                        left.len(),
                        right.len()
                    ));
                }
                left.iter()
                    .zip(right)
                    .enumerate()
                    .find_map(|(index, (a, b))| {
                        first_receipt_difference(a, b, &format!("{path}[{index}]"), depth + 1)
                    })
            }
            _ => Some(format!("{path}: source {expected}, native {actual}")),
        }
    }

    /// 외부 원문 기록을 명시적으로 제공할 때만 실행한다. 각 포획·수동 착지·
    /// 반격 단계는 독립 import하므로 앞 단계의 실패가 다른 비교를 가리지 않는다.
    /// 합성 callback 검증은 completeMove나 공개 legal/bind/apply의 증거가 아니다.
    #[test]
    #[ignore = "requires ACCELERATE_V7_CAPTURE_SOURCE_RECEIPT with frozen callback source evidence"]
    fn frozen_capture_callbacks_when_source_receipt_supplied() {
        let path = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_CAPTURE_SOURCE_RECEIPT")
                .expect("capture source receipt path"),
        );
        assert!(
            path.is_absolute(),
            "capture receipt must use an absolute external path"
        );
        let receipt: Value =
            serde_json::from_slice(&std::fs::read(path).expect("capture source receipt"))
                .expect("capture source JSON");
        assert_eq!(receipt["schemaVersion"], json!(1));
        assert_eq!(
            receipt["sourceMainSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        assert_eq!(
            receipt["oracleProfileVersion"],
            "accelerate-headless-semantic-v7-faithful-init-v1"
        );
        assert_eq!(receipt["failures"], json!(0));
        let cases = receipt["cases"].as_array().expect("source cases");
        assert_eq!(cases.len(), 10, "reviewed bounded source case count");
        let reviewed: [(&str, &[&str]); 10] = [
            ("ordinary-wizard-mana-before-victim-removal", &["capture"]),
            ("shield-break-stationary", &["break-shield"]),
            ("hp-request-nine-source-damage-one", &["damage-health"]),
            (
                "parry-capture-landing-retaliation",
                &["capture", "manual-landing", "resolve-parry"],
            ),
            ("evasion-row-major-source-rng", &["capture"]),
            (
                "bear-capture-landing-restore",
                &["capture", "manual-landing", "resolve-bear"],
            ),
            (
                "bear-failed-restore-recurrence",
                &[
                    "capture",
                    "manual-landing",
                    "external-attacker-removal-and-blockers",
                    "resolve-bear",
                ],
            ),
            (
                "trojan-two-entries-one-attacker-removal",
                &["resolve-trojan"],
            ),
            (
                "feudal-capture-landing-guardian-strike",
                &["capture", "manual-landing", "resolve-feudal"],
            ),
            ("calling-card-imperial-followup", &["capture"]),
        ];
        let mut failures = Vec::new();
        let mut compared = 0;
        for (case, (expected_name, expected_stages)) in cases.iter().zip(reviewed) {
            assert_eq!(case["scope"], "synthetic-source-callback");
            assert_eq!(case["sourceReachabilityProven"], json!(false));
            assert!(
                case.get("error").is_none(),
                "source callback error: {}",
                case["error"]
            );
            let name = case["name"].as_str().expect("source case name");
            assert_eq!(name, expected_name, "reviewed source case identity");
            let stages = case["stages"].as_array().expect("source callback stages");
            assert_eq!(
                stages.len(),
                expected_stages.len(),
                "{name} reviewed callback count"
            );
            for (stage, expected_stage) in stages.iter().zip(expected_stages) {
                let stage_name = stage["name"].as_str().expect("source stage name");
                assert_eq!(
                    stage_name, *expected_stage,
                    "{name} reviewed callback order"
                );
                assert_eq!(
                    stage["outerHistoryUnchanged"],
                    json!(true),
                    "{name}/{stage_name} source callback history"
                );
                let label = format!("{name}/{stage_name}");
                let host = crate::V7HostPosition::from_envelope(stage["before"].clone())
                    .expect("source before import");
                match host.transact(host.position_id(), |state| {
                    run_receipt_stage(state, stage_name)
                }) {
                    Ok((after, returned)) => {
                        let native = after.export_envelope().expect("native callback export");
                        if let Some(difference) =
                            first_receipt_difference(&stage["after"], &native, "$.after", 0)
                        {
                            failures.push(format!("{label}: {difference}"));
                        }
                        if let Some(difference) =
                            first_receipt_difference(&stage["returned"], &returned, "$.returned", 0)
                        {
                            failures.push(format!("{label}: {difference}"));
                        }
                        assert_eq!(
                            host.export_envelope().expect("unchanged before import"),
                            stage["before"],
                            "{label} original position changed"
                        );
                    }
                    Err(error) => failures.push(format!("{label}: native callback error: {error}")),
                }
                compared += 1;
            }
        }
        assert_eq!(compared, 19, "reviewed bounded callback stage count");
        assert!(
            failures.is_empty(),
            "capture source callback differences:\n{}",
            failures.join("\n")
        );
    }

    fn bare_v7() -> GameState {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.mode = "play".into();
        state.turn = Color::White;
        state.board = vec![vec![None; 8]; 8];
        state.board[7][7] = Some(Piece::new("king", Color::White, "white-king"));
        state.board[0][7] = Some(Piece::new("king", Color::Black, "black-king"));
        state.extra.insert(
            "capturedTypes".into(),
            json!({"white":{"values":[]},"black":{"values":[]}}),
        );
        state.extra.insert(
            "turnCaptures".into(),
            json!({"white":{"values":[]},"black":{"values":[]}}),
        );
        state
    }

    fn attacker(state: &mut GameState) -> Piece {
        let piece = Piece::new("rook", Color::White, "attacker");
        state.board[2][2] = Some(piece.clone());
        piece
    }

    fn control_next_draws(state: &mut GameState, values: &[f64]) {
        // Source tape is indexed by the absolute cursor. Initial-game draws
        // have already advanced it; keep that seed/cursor and set only future
        // draws rather than treating a fresh two-item tape as cursor zero.
        state.rng.tape.resize(state.rng.cursor, 0.5);
        state.rng.tape.extend_from_slice(values);
    }

    #[test]
    fn saturation_attack_snapshot_survives_the_capture_limit_crossing() {
        // Frozen main65752 withSaturationAttack preserves its initial lock for
        // the same active mover; another piece still uses its live count.
        let mut state = bare_v7();
        let mut active = attacker(&mut state);
        state.extra.insert("saturationRule".into(), json!(true));
        active.extra.insert("capturesMade".into(), json!(2));
        assert!(!saturation_locked(&state, &active));
        state.active_v7_saturation_attack = Some((active.id.clone(), false));
        active.extra.insert("capturesMade".into(), json!(3));
        assert!(!saturation_locked(&state, &active));
        let mut other = active.clone();
        other.id = "another-attacker".into();
        assert!(saturation_locked(&state, &other));
        state.active_v7_saturation_attack = Some((active.id.clone(), true));
        active.extra.insert("capturesMade".into(), json!(0));
        assert!(saturation_locked(&state, &active));
        state.active_v7_saturation_attack = None;
        assert!(!saturation_locked(&state, &active));
    }

    #[test]
    fn ordinary_capture_grants_victim_wizard_mana_before_removal() {
        let mut state = bare_v7();
        let mut active = attacker(&mut state);
        let mut victim = Piece::new("wizard", Color::Black, "victim-wizard");
        victim.extra.insert("mana".into(), json!(0));
        victim.extra.insert("maxMana".into(), json!(5));
        let mut friend = victim.clone();
        friend.id = "surviving-wizard".into();
        state.board[3][3] = Some(victim);
        state.board[4][4] = Some(friend);
        let square = Square { row: 3, col: 3 };
        let captured = capture_at(
            &mut state,
            &mut active,
            square,
            &CaptureOptions {
                attacker_landing: Some(square),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(captured.extra["mana"].as_f64(), Some(1.0));
        assert_eq!(state.captures.white[0].extra["mana"].as_f64(), Some(1.0));
        assert_eq!(
            state.board[4][4].as_ref().unwrap().extra["mana"].as_f64(),
            Some(1.0)
        );
        assert_eq!(active.extra["capturesMade"].as_f64(), Some(1.0));
        assert_eq!(active.extra["totalCaptures"].as_f64(), Some(1.0));
        assert!(state.at(square).is_none());
    }

    #[test]
    fn capture_memory_distinguishes_physical_parrot_from_inherited_parrot_ability() {
        // Frozen main601 chooses physical Medium/Parrot first; main15919
        // rememberedBaseMovement treats inherited Parrot with previous=null.
        for (physical, ability, expected) in [
            ("parrot", None, json!({"type":"knight"})),
            ("medium", None, json!({"type":"queen"})),
            ("trickster", Some("parrot"), Value::Null),
            ("trickster", Some("medium"), json!({"type":"medium"})),
        ] {
            let mut state = bare_v7();
            let mut active = attacker(&mut state);
            let mut victim = Piece::new(physical, Color::Black, "memory-target");
            if let Some(ability) = ability {
                victim
                    .extra
                    .insert("tricksterMoveType".into(), json!(ability));
            }
            state
                .extra
                .insert("mediumMovement".into(), json!({"type":"queen"}));
            state.extra.insert(
                "parrotMovement".into(),
                json!({"white":null,"black":{"type":"knight"}}),
            );
            state.board[3][3] = Some(victim);
            let square = Square { row: 3, col: 3 };
            capture_at(
                &mut state,
                &mut active,
                square,
                &CaptureOptions {
                    attacker_landing: Some(square),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                state.extra["mediumMovement"], expected,
                "{physical}/{ability:?}"
            );
        }
    }

    #[test]
    fn wizard_mana_string_addition_matches_source_concat_then_math_min() {
        let mut state = bare_v7();
        let mut wizard = Piece::new("wizard", Color::White, "wizard");
        wizard.extra.insert("mana".into(), json!("1"));
        wizard.extra.insert("maxMana".into(), json!(5));
        state.board[2][2] = Some(wizard);
        grant_wizard_mana(&mut state, PieceColor::White, 1).unwrap();
        assert_eq!(
            state.board[2][2].as_ref().unwrap().extra["mana"].as_f64(),
            Some(5.0)
        );
    }

    #[test]
    fn hp_removal_grants_mana_only_to_surviving_wizards() {
        // Direct damageHealthPiece accepts any item; only movePiece's dispatch
        // limits it to HP types. This checks the different removal callback order.
        let mut state = bare_v7();
        let mut active = attacker(&mut state);
        let mut victim = Piece::new("wizard", Color::Black, "victim-wizard");
        victim.extra.insert("mana".into(), json!(0));
        victim.extra.insert("maxMana".into(), json!(5));
        victim.extra.insert("hp".into(), json!(1));
        victim.extra.insert("maxHp".into(), json!(1));
        let mut friend = victim.clone();
        friend.id = "surviving-wizard".into();
        state.board[3][3] = Some(victim.clone());
        state.board[4][4] = Some(friend);
        let removed = damage_health_piece(
            &mut state,
            &mut active,
            &mut victim,
            Square { row: 3, col: 3 },
            "룩의 공격",
            &CaptureOptions::default(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(removed.extra["mana"], json!(0));
        assert_eq!(state.captures.white[0].extra["mana"], json!(0));
        assert_eq!(
            state.board[4][4].as_ref().unwrap().extra["mana"].as_f64(),
            Some(1.0)
        );
        assert_eq!(removed.extra["hp"].as_f64(), Some(0.0));
    }

    #[test]
    fn hp_attack_damages_one_and_keeps_attacker_at_origin() {
        let mut state = bare_v7();
        let mut active = attacker(&mut state);
        let mut victim = Piece::new("shotgunKing", Color::Black, "hp-target");
        victim.extra.insert("hp".into(), json!(3));
        victim.extra.insert("maxHp".into(), json!(3));
        state.board[3][3] = Some(victim);
        assert_eq!(
            attack_defended_piece(
                &mut state,
                &mut active,
                Square { row: 3, col: 3 },
                "룩의 공격",
                &CaptureOptions::default()
            )
            .unwrap(),
            DefendedAttack::Health { removed: None }
        );
        assert_eq!(
            state.board[3][3].as_ref().unwrap().extra["hp"].as_f64(),
            Some(2.0)
        );
        assert_eq!(
            piece_square(&state, &active.id),
            Some(Square { row: 2, col: 2 })
        );
        assert!(state.captures.white.is_empty());
    }

    #[test]
    fn missing_caster_captures_without_fabricating_attacker_reactions() {
        let mut state = bare_v7();
        let mut victim = Piece::new("knight", Color::Black, "victim");
        victim.extra.insert("evasion".into(), json!(true));
        victim.extra.insert("parry".into(), json!({"chance":1}));
        victim.extra.insert("trojanHorse".into(), json!(true));
        state.board[3][3] = Some(victim);
        let before = state.rng.cursor;
        let captured = capture_at_with_optional_attacker(
            &mut state,
            Square { row: 3, col: 3 },
            Color::White,
            None,
            &CaptureOptions::default(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(captured.id, "victim");
        assert_eq!(captured.extra["trojanHorse"], json!(true));
        assert_eq!(captured.extra["parry"], json!({"chance":1}));
        assert_eq!(state.captures.white.len(), 1);
        assert_eq!(state.rng.cursor, before);
        assert!(
            state
                .extra
                .get("pendingBearRetaliations")
                .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
        );
        assert!(
            state
                .extra
                .get("pendingTrojanHorse")
                .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
        );
    }

    #[test]
    fn missing_caster_hp_removal_does_not_evoke_lethal_evasion() {
        let mut state = bare_v7();
        state.extra.insert("logs".into(), json!([]));
        let mut victim = Piece::new("bigRook", Color::Black, "hp-target");
        victim.extra.insert("hp".into(), json!(1));
        victim.extra.insert("maxHp".into(), json!(1));
        victim.extra.insert("evasion".into(), json!(true));
        let square = Square { row: 3, col: 3 };
        install_piece(&mut state, &mut victim, square).unwrap();
        let before = state.rng.cursor;
        let removed = damage_health_piece_with_optional_attacker(
            &mut state,
            square,
            Color::White,
            None,
            "마법",
            &CaptureOptions::default(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(removed.id, "hp-target");
        assert_eq!(removed.extra["hp"].as_f64(), Some(0.0));
        assert!(piece_square(&state, &removed.id).is_none());
        assert_eq!(state.rng.cursor, before);
        // Frozen main50343 adds TYPE_LABELS.bigRook after the base 30 labels.
        // The faithful initializer profile must retain that source mutation.
        assert_eq!(state.extra["logs"][0], json!("흑 빅룩이 쓰러졌습니다."));
        assert_eq!(state.extra["logs"][1], json!("마법: 빅룩 HP -1 (0/1)"));
    }

    #[test]
    fn source_chance_parry_keeps_legacy_density_separate_for_both_capture_branches() {
        // main:108076의 실제 성공/실패 mass를 각각 한 번 기록한다.
        // 기존 legacy accumulator가 활성화돼도 새 owned trace와 이중 곱하지 않는다.
        for (roll, mass, armed) in [(0.1, 0.25, true), (0.8, 0.75, false)] {
            let mut plain = bare_v7();
            let mut plain_active = attacker(&mut plain);
            let mut victim = Piece::new("bishop", Color::Black, "parry-chance-audit");
            victim.extra.insert("parry".into(), json!({"chance":0.25}));
            plain.board[3][3] = Some(victim);
            plain.semantic_chance_probability = Some(1.0);
            control_next_draws(&mut plain, &[roll, 0.4]);
            let mut traced = plain.clone();
            let mut traced_active = plain_active.clone();
            traced.rng.begin_source_trace().unwrap();
            let at = Square { row: 3, col: 3 };
            let options = CaptureOptions {
                attacker_landing: Some(at),
                ..Default::default()
            };
            let expected = capture_at(&mut plain, &mut plain_active, at, &options).unwrap();
            let actual = capture_at(&mut traced, &mut traced_active, at, &options).unwrap();
            assert_eq!(actual.is_none(), armed);
            assert_eq!(traced.rng.finish_source_trace().unwrap(), mass);
            assert_eq!(traced.semantic_chance_probability, Some(mass));
            assert_eq!(actual, expected);
            assert_eq!(traced_active, plain_active);
            assert_eq!(traced, plain);
        }
    }

    #[test]
    fn parry_arms_without_recording_a_normal_capture_and_restores_after_landing() {
        let mut state = bare_v7();
        let mut active = attacker(&mut state);
        let mut victim = Piece::new("bishop", Color::Black, "parrying-bishop");
        victim.extra.insert("parry".into(), json!({"chance":1}));
        state.board[3][3] = Some(victim);
        control_next_draws(&mut state, &[0.2, 0.4]);
        let rng_before = state.rng.cursor;
        let landing = Square { row: 3, col: 3 };
        assert!(
            capture_at(
                &mut state,
                &mut active,
                landing,
                &CaptureOptions {
                    attacker_landing: Some(landing),
                    ..Default::default()
                }
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(state.rng.cursor - rng_before, 1);
        assert!(state.captures.white.is_empty());
        assert!(active.extra.get("totalCaptures").is_none());
        assert_eq!(active.extra["capturesMade"].as_f64(), Some(1.0));
        crate::transition::clear_piece(&mut state, &active.id);
        state.board[3][3] = Some(active.clone());
        assert_eq!(
            resolve_pending_bear_retaliations(&mut state, Some(Color::White), Some(&active.id))
                .unwrap(),
            1
        );
        let restored = state.board[2][2].as_ref().unwrap();
        assert_eq!(restored.id, "parrying-bishop");
        assert!(!restored.extra.contains_key("parry"));
        assert_eq!(restored.extra["totalCaptures"].as_f64(), Some(1.0));
        assert_eq!(state.captures.black[0].id, "attacker");
        assert!(
            state.extra["pendingBearRetaliations"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(state.rng.cursor - rng_before, 2);
    }

    #[test]
    fn trojan_deduplicates_recapture_but_spawns_each_due_pawn() {
        let mut state = bare_v7();
        let active = attacker(&mut state);
        state.extra.insert("pendingTrojanHorse".into(),json!([
            {"color":"black","attackerColor":"white","attackerId":active.id,"square":{"row":3,"col":3}},
            {"color":"black","attackerColor":"white","attackerId":active.id,"square":{"row":4,"col":4}}
        ]));
        let before = state.rng.cursor;
        assert_eq!(
            resolve_pending_trojan_horse_retaliations(
                &mut state,
                Some(Color::White),
                Some(&active.id)
            )
            .unwrap(),
            2
        );
        assert_eq!(state.captures.black.len(), 1);
        assert_eq!(state.captures.black[0].id, active.id);
        assert_eq!(state.board[3][3].as_ref().unwrap().kind, "pawn");
        assert_eq!(state.board[4][4].as_ref().unwrap().kind, "pawn");
        assert_eq!(state.rng.cursor - before, 2);
    }

    #[test]
    fn bear_defers_recurrence_until_failed_restoration() {
        let mut state = bare_v7();
        let mut active = attacker(&mut state);
        let mut victim = Piece::new("bear", Color::Black, "counter-bear");
        victim
            .extra
            .insert("bearRetaliationsRemaining".into(), json!(1));
        victim.extra.insert("recurrence".into(), json!(true));
        state.board[3][3] = Some(victim);
        let landing = Square { row: 3, col: 3 };
        capture_at(
            &mut state,
            &mut active,
            landing,
            &CaptureOptions {
                attacker_landing: Some(landing),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            state
                .extra
                .get("pendingRecurrences")
                .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
        );
        assert!(active.extra.get("totalCaptures").is_none());
        crate::transition::clear_piece(&mut state, &active.id);
        state.board[2][2] = Some(Piece::new("pawn", Color::White, "blocking-origin"));
        state.board[3][3] = Some(Piece::new("pawn", Color::White, "blocking-source"));
        assert_eq!(
            resolve_pending_bear_retaliations(&mut state, Some(Color::White), Some(&active.id))
                .unwrap(),
            0
        );
        assert_eq!(
            state.extra["pendingRecurrences"][0]["piece"]["id"],
            json!("counter-bear")
        );
        assert!(state.captures.white.is_empty());
    }

    #[test]
    fn late_callback_failure_restores_attacker_board_and_rng() {
        let mut state = bare_v7();
        let mut active = attacker(&mut state);
        let mut victim = Piece::new("bishop", Color::Black, "victim");
        victim.extra.insert("parry".into(), json!({"chance":0.01}));
        victim
            .extra
            .insert("callingCard".into(), json!({"by":"invalid-owner"}));
        state.board[3][3] = Some(victim);
        state.board[4][4] = Some(Piece::new("rook", Color::Black, "followup"));
        control_next_draws(&mut state, &[0.9, 0.1]);
        let before = state.clone();
        let original_active = active.clone();
        assert!(matches!(
            capture_at(
                &mut state,
                &mut active,
                Square { row: 3, col: 3 },
                &CaptureOptions::default()
            ),
            Err(EngineError::InvalidState(_))
        ));
        assert_eq!(state, before);
        assert_eq!(active, original_active);
    }
}
