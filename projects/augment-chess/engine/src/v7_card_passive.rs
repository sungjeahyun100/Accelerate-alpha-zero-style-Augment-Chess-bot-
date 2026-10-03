//! Source-pinned acquisition and first-move scheduling for the v7 card deck.
//!
//! `IDS` owns the 66 public PASSIVE cards in site-20260928.json. White Box is
//! intentionally owned by the choice-card object: its revealed card is a
//! second, ordered card application. This object owns acquisition, Clone
//! distribution, reservations, and ordered automatic target selection. Shared
//! board-action reconciliation and hazards execute at their source boundaries.
//! Errors roll back the acquisition instead of committing a partial draft.

use crate::card_registry::{CardActType, CardType};
use crate::{CardSlot, Color, EngineError, GameState, Piece, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};
use std::collections::BTreeSet;

// Frozen client main-OahWs0tU.js PASSIVE_CARD_IDS, intersected with the v7
// public catalog. White Box is owned by v7_card_choice.
pub(super) const IDS: &[&str] = &[
    "apprentice-knights",
    "backward-knight",
    "big-rook",
    "bina-mate",
    "blue-jeans",
    "checker",
    "clone",
    "conversion",
    "corner-kick",
    "coronation",
    "democracy",
    "dutch",
    "early-promotion",
    "elephant-escape",
    "encouragement",
    "exhaustion",
    "fast-growth",
    "fianchetto",
    "file-surge",
    "final-weapon",
    "ghost",
    "horde",
    "horse-riding",
    "imperial-studies",
    "initiative",
    "injury",
    "iron-monarch",
    "last-stand",
    "leap",
    "london-system",
    "mad-horse",
    "mongolian-gambit",
    "moving",
    "overwhelm",
    "pawn-conversion",
    "queen-afterimage",
    "racing-king",
    "radical-charge",
    "religious-victory",
    "retreat",
    "rook-lift",
    "sprint",
    "underground-bunker",
    "underpromotion",
    "king-of-the-hill",
    "geneva-convention",
    "field-promotion",
    "frontline-response",
    "gomoku",
    "majesty",
    "infiltration",
    "killer-king",
    "resolve",
    "vanguard",
    "big-bishop",
    "highlander",
    "false-start",
    "proficiency",
    "locust-swarm",
    "long-en-passant",
    "brutus",
    "mutation",
    "d4",
    "synchronization",
    "assembly",
    "vigilance",
];

pub(super) fn owns(id: &str) -> bool {
    IDS.contains(&id)
}

/// Source `openingPassiveSettlementPriority` (main:3781). Bundle and grand
/// settlement preserve acquisition order for equal priorities.
pub(super) fn acquisition_priority(id: &str) -> u8 {
    match id {
        "london-system" => 0,
        "horde" => 1,
        "big-rook" | "big-bishop" => 2,
        "false-start" => 4,
        "locust-swarm" => 5,
        _ => 3,
    }
}

/// `addCardToPlayerDeck`/`addGrandCardToPlayerDeck` reservation fields
/// (main:66820 and 69008). The caller assigns the deck slot and acquisition
/// order, then queues gain notation in that order.
pub(super) fn register_acquired_card(
    state: &GameState,
    color: Color,
    card: &mut CardSlot,
    phase: &str,
    grand: bool,
) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::InvalidState(
            "v7 card registration requires v7 rules".into(),
        ));
    }
    let definition = crate::card_registry::definition_for(&state.ruleset_id, &card.id)?;
    if definition.effect != card.effect {
        return Err(EngineError::InvalidState(format!(
            "card {} effect differs from frozen catalog",
            card.id
        )));
    }
    let category = definition.card_type;
    if card.id == "black-tower-legacy-magic" {
        crate::v7_campaign::validate_black_tower_card_instance(state, card)?;
        if category.is_some()
            || definition.activation != Some(CardActType::Active)
            || definition.source_definition.get("phase") != Some(&json!("???"))
            || definition.effect != "blackTowerLegacyMagic"
        {
            return Err(EngineError::InvalidState(
                "v7 Black Tower registration requires its exact dynamic definition".into(),
            ));
        }
    } else if category.is_none() {
        return Err(EngineError::InvalidState(format!(
            "card {} type missing",
            card.id
        )));
    }
    let delayed = matches!(category, Some(CardType::Middle | CardType::End));
    let pending = if grand {
        category == Some(CardType::End)
    } else {
        state.mode == "draft" && matches!(phase, "MIDDLE" | "END") && delayed
    };
    card.extra.insert("deckCard".into(), json!(true));
    if pending {
        card.extra.insert("nextTurnPending".into(), json!(true));
        card.extra.insert(
            "nextTurnPendingSinceTurn".into(),
            json!(if grand {
                0
            } else {
                *state.turns_taken.get(color)
            }),
        );
    } else {
        if grand {
            card.extra.shift_remove("nextTurnPending");
        } else {
            card.extra.insert("nextTurnPending".into(), json!(false));
        }
        card.extra.shift_remove("nextTurnPendingSinceTurn");
    }
    let first_turn = state.move_count == 0
        && (grand || (state.mode == "draft" && phase == "OPENING"))
        && opening_auto_card(state, card)?;
    card.extra.insert("firstTurnCard".into(), json!(first_turn));
    Ok(())
}

fn opening_auto_card(state: &GameState, card: &CardSlot) -> Result<bool> {
    if matches!(
        card.id.as_str(),
        "shotgun-king" | "black-tower-legacy-magic"
    ) {
        return Ok(true);
    }
    let definition = crate::card_registry::definition_for(&state.ruleset_id, &card.id)?;
    Ok(definition.card_type == Some(CardType::Opening))
}

/// Source `isFirstMoveAutoCard` and `playerDeck().filter()` preserve slot order.
pub(super) fn first_move_auto_slots(state: &GameState, color: Color) -> Result<Vec<usize>> {
    if state.flag("firstMoveCardsForced", color) || *state.turns_taken.get(color) != 0 {
        return Ok(Vec::new());
    }
    let mut slots = Vec::new();
    for (slot, card) in state.deck_slots.get(color).iter().enumerate() {
        if !card.vacant
            && !card.used
            && card.extra.get("firstTurnCard") == Some(&json!(true))
            && opening_auto_card(state, card)?
        {
            slots.push(slot);
        }
    }
    Ok(slots)
}

/// Source `randomSquare` (main:88130): enumerate every matching board cell in
/// row-major order, including aliases, and consume one draw only when a
/// candidate exists. The caller keeps the source's ordered compound searches.
fn random_first_move_square(
    state: &mut GameState,
    predicate: impl Fn(&GameState, &Piece) -> bool,
) -> Result<Option<Square>> {
    let squares = (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter(|&square| {
            state
                .at(square)
                .is_some_and(|piece| predicate(state, piece))
        })
        .collect::<Vec<_>>();
    if squares.is_empty() {
        return Ok(None);
    }
    let index = crate::transition::sample_choice(state, squares.len())?;
    Ok(Some(squares[index]))
}

/// Source `autoTargetForFirstMoveCard` (main:88065). Compound cards consume
/// separate draws in their explicit source order; grappler searches for its
/// minor even when no queen exists. A declined target preserves draws already
/// consumed so a first-move rollback or random-box retry sees the source RNG.
pub(super) fn first_move_target(state: &mut GameState, card: &CardSlot) -> Result<Option<Value>> {
    if !crate::observation::truth(card.extra.get("target")) {
        return Ok(None);
    }
    let color = state.turn;
    if card.id == "grappler" && crate::card_effects::september26(state) {
        let queen = random_first_move_square(state, |state, piece| {
            piece.color == color
                && piece.kind == "queen"
                && !crate::v7_threat::is_royal_identity_v7(state, piece)
        })?;
        let minor = random_first_move_square(state, |state, piece| {
            piece.color == color
                && crate::card_effects::minor(state, piece)
                && !crate::v7_threat::is_royal_identity_v7(state, piece)
        })?;
        let (Some(queen), Some(minor)) = (queen, minor) else {
            return Err(EngineError::IllegalAction);
        };
        return Ok(Some(json!({"row":queen.row,"col":queen.col,"minor":minor})));
    }
    let first = match card.effect.as_str() {
        "amazon" | "hook" => random_first_move_square(state, |_, piece| {
            piece.color == color
                && piece.kind == "queen"
                && piece.extra.get("regencyHeir") != Some(&Value::Bool(true))
        })?,
        "windmill" => random_first_move_square(state, |_, piece| {
            piece.color == color && piece.kind == "bishop"
        })?,
        "feudalContract" => random_first_move_square(state, |_, piece| {
            piece.color == color
                && ["pawn", "fanatic"].contains(&piece.kind.as_str())
                && !crate::observation::truth(piece.extra.get("explosive"))
                && !crate::observation::truth(piece.extra.get("feudalContractId"))
        })?,
        _ => {
            let squares = crate::card_effects::target_squares(state, card)?.ok_or_else(|| {
                EngineError::UnsupportedFeature(format!(
                    "first-move target enumeration {}",
                    card.id
                ))
            })?;
            if squares.is_empty() {
                return Err(EngineError::IllegalAction);
            }
            let index = crate::transition::sample_choice(state, squares.len())?;
            return Ok(Some(json!(squares[index])));
        }
    }
    .ok_or(EngineError::IllegalAction)?;
    let (second, field, target) = match card.effect.as_str() {
        "amazon" => (
            random_first_move_square(state, |_, piece| {
                piece.color == color && piece.kind == "knight"
            })?,
            "knight",
            first,
        ),
        "hook" => (
            random_first_move_square(state, |_, piece| {
                piece.color == color && piece.kind == "rook"
            })?,
            "rook",
            first,
        ),
        "windmill" => (
            random_first_move_square(state, |_, piece| {
                piece.color == color && piece.kind == "rook"
            })?,
            "bishop",
            first,
        ),
        "feudalContract" => (
            random_first_move_square(state, |_, piece| {
                piece.color == color
                    && ![
                        "pawn",
                        "fanatic",
                        "wall",
                        "colossus",
                        "bigRook",
                        "bigBishop",
                    ]
                    .contains(&piece.kind.as_str())
            })?,
            "pawn",
            first,
        ),
        _ => unreachable!("the ordinary target branch returns above"),
    };
    let second = second.ok_or(EngineError::IllegalAction)?;
    let (primary, partner) = if matches!(card.effect.as_str(), "windmill" | "feudalContract") {
        (second, target)
    } else {
        (target, second)
    };
    let mut selected = json!({"row":primary.row,"col":primary.col});
    selected[field] = json!(partner);
    Ok(Some(selected))
}

fn ensure_passive(state: &GameState, card: &CardSlot) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 || !owns(&card.id) {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 passive card {}",
            card.id
        )));
    }
    let definition = crate::card_registry::definition_for(&state.ruleset_id, &card.id)?;
    if definition.activation != Some(CardActType::Passive) || definition.effect != card.effect {
        return Err(EngineError::InvalidState(format!(
            "v7 passive card {} catalog mismatch",
            card.id
        )));
    }
    Ok(())
}

fn has_piece(state: &GameState, color: Color, kind: &str) -> bool {
    state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|piece| piece.color == color && piece.kind == kind)
}

fn has_royal(state: &GameState, color: Color) -> bool {
    state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|piece| piece.color == color && king_augment_recipient(state, piece))
}

fn set_normalized_color_flag(state: &mut GameState, field: &str, color: Color) {
    // normalizeColorBooleans discards malformed/unrecognized keys and uses
    // white-then-black key order. Both source serializations are observable.
    let prior = state.extra.get(field);
    let white = prior.and_then(|v| v.get("white")).and_then(Value::as_bool) == Some(true);
    let black = prior.and_then(|v| v.get("black")).and_then(Value::as_bool) == Some(true);
    state.extra.insert(field.into(), json!({"white": if color == Color::White {true} else {white}, "black": if color == Color::Black {true} else {black}}));
}

fn set_existing_color_flag(state: &mut GameState, field: &str, color: Color) -> Result<()> {
    let sides = state
        .extra
        .get_mut(field)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState(format!("{field} must be a color map")))?;
    sides.insert(color.as_str().into(), json!(true));
    Ok(())
}

fn apply_idempotent_color_flag(state: &mut GameState, field: &str, color: Color) -> Result<bool> {
    // The September 26 and internal eight/five helpers decline a duplicate
    // instead of returning a second successful passive application.
    if state.flag(field, color) {
        return Ok(false);
    }
    let entry = state
        .extra
        .entry(field.to_owned())
        .or_insert_with(|| json!({}));
    let sides = entry
        .as_object_mut()
        .ok_or_else(|| EngineError::InvalidState(format!("{field} must be a color map")))?;
    sides.insert(color.as_str().into(), json!(true));
    Ok(true)
}

fn board_entries(state: &GameState, color: Color, kind: &str) -> Vec<(Square, Piece)> {
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(piece) = state.at(square) else {
                continue;
            };
            if piece.color != color || piece.kind != kind {
                continue;
            }
            // The client board can have several cells that refer to the same
            // piece object. Rust's snapshot stores copies, so deduplicate by
            // the source ID and update every corresponding cell together.
            let key = if piece.id.is_empty() {
                format!("@{row}:{col}")
            } else {
                piece.id.clone()
            };
            if seen.insert(key) {
                entries.push((square, piece.clone()));
            }
        }
    }
    entries
}

fn replace_aliases(
    state: &mut GameState,
    at: Square,
    original_id: &str,
    replacement: Option<Piece>,
) {
    if original_id.is_empty() {
        state.board[at.row as usize][at.col as usize] = replacement;
        return;
    }
    for cell in state.board.iter_mut().flatten() {
        if cell.as_ref().is_some_and(|piece| piece.id == original_id) {
            *cell = replacement.clone();
        }
    }
}

fn convert_knights(state: &mut GameState, color: Color) -> Result<bool> {
    let candidates = board_entries(state, color, "knight");
    if candidates.is_empty() {
        return Ok(false);
    }
    let context = state.clone();
    for (square, original) in candidates {
        let mut updated = original.clone();
        updated.kind = "bishop".into();
        crate::card_effects::mark_transformed_origin(&context, &mut updated, square)?;
        updated.moved = true;
        crate::card_effects::mark_animation(state, &updated)?;
        replace_aliases(state, square, &original.id, Some(updated));
    }
    Ok(true)
}

fn king_augment_recipient(state: &GameState, piece: &Piece) -> bool {
    crate::v7_threat::is_royal_identity_v7(state, piece)
}

fn underground_bunker(state: &mut GameState, color: Color) -> Result<bool> {
    let target = (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .find(|&square| {
            state.at(square).is_some_and(|piece| {
                piece.color == color
                    && king_augment_recipient(state, piece)
                    && !crate::observation::truth(piece.extra.get("undergroundBunker"))
            })
        });
    let Some(square) = target else {
        return Ok(false);
    };
    let original = state.at(square).ok_or(EngineError::IllegalAction)?.clone();
    let mut updated = original.clone();
    updated
        .extra
        .insert("undergroundBunker".into(), json!(true));
    updated.extra.insert("hp".into(), json!(5));
    updated.extra.insert("maxHp".into(), json!(5));
    crate::card_effects::mark_animation(state, &updated)?;
    replace_aliases(state, square, &original.id, Some(updated));
    Ok(true)
}

fn horse_riding(state: &mut GameState, color: Color) -> Result<bool> {
    if !state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|piece| piece.color == color && king_augment_recipient(state, piece))
    {
        return Ok(false);
    }
    let knights = board_entries(state, color, "knight");
    let erased_knights = !knights.is_empty();
    for (square, piece) in knights {
        crate::card_effects::mark_vanish_animation(state, &piece, square)?;
        replace_aliases(state, square, &piece.id, None);
    }
    if erased_knights {
        crate::v7_capture_objectives::check_campaign_objectives(state)?;
    }
    let flag = if state.flag("knightmate", color) {
        "royalKnightKing"
    } else {
        "kingKnight"
    };
    let side = state.extra.entry(flag).or_insert_with(|| json!({}));
    side.as_object_mut()
        .ok_or_else(|| EngineError::InvalidState(format!("{flag} must be a color map")))?
        .insert(color.as_str().into(), json!(true));
    Ok(true)
}

fn passive_placement_open(state: &GameState, color: Color, square: Square) -> Result<bool> {
    Ok(state.at(square).is_none()
        && !false_start_blocked(state, square)?
        && crate::movement::expansion_destination_allowed(state, color.into(), &[square]))
}

fn summon_passive_piece(
    state: &mut GameState,
    color: Color,
    kind: &str,
    square: Square,
) -> Result<()> {
    let mut piece = crate::opening::spawn(state, color, kind)?;
    piece.extra.insert(
        "origin".into(),
        json!(format!(
            "{}{}",
            char::from(b'a' + square.col),
            8 - square.row
        )),
    );
    piece.moved = true;
    let lock = state.turns_taken.get(color).checked_add(1).ok_or_else(|| {
        EngineError::InvalidState("passive summoning capture lock overflow".into())
    })?;
    piece
        .extra
        .insert("freshNoCaptureUntil".into(), json!(lock));
    state.board[square.row as usize][square.col as usize] = Some(piece);
    Ok(())
}

fn checker(state: &mut GameState, color: Color) -> Result<bool> {
    // Frozen main:103327. Current v7 profiles take the September 26 four-cell
    // summoning branch; legacy catalog hashes take a separate pawn transform.
    let catalog_hash = state
        .extra
        .get("cardState")
        .and_then(|cards| cards.get("profile"))
        .and_then(|profile| profile.get("catalogHash"))
        .and_then(Value::as_str);
    if catalog_hash.is_some_and(|hash| {
        ![
            "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
            "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
            "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs",
        ]
        .contains(&hash)
    }) {
        return Err(EngineError::UnsupportedFeature(
            "checker legacy catalog transform".into(),
        ));
    }
    let row = if color == Color::White { 5 } else { 2 };
    let cells = [0, 1, 6, 7].map(|col| Square { row, col });
    for square in cells {
        if !passive_placement_open(state, color, square)? {
            return Ok(false);
        }
    }
    for square in cells {
        summon_passive_piece(state, color, "checker", square)?;
    }
    Ok(true)
}

fn diagonal_chess_active(state: &GameState) -> bool {
    state.extra["appliedRuleCard"]["id"] == "diagonal-chess"
        || state
            .extra
            .get("additionalRuleCards")
            .and_then(Value::as_array)
            .is_some_and(|cards| cards.iter().any(|card| card["id"] == "diagonal-chess"))
}

fn elephant_escape(state: &mut GameState, color: Color) -> Result<bool> {
    // Frozen main:103025; unlike Checker this source branch checks occupation
    // only, and later post-card environmental reactions own hazard removal.
    let cells = if diagonal_chess_active(state) {
        match color {
            Color::White => [Square { row: 2, col: 0 }, Square { row: 7, col: 5 }],
            Color::Black => [Square { row: 5, col: 7 }, Square { row: 0, col: 2 }],
        }
    } else {
        let row = if color == Color::White { 5 } else { 2 };
        [Square { row, col: 0 }, Square { row, col: 7 }]
    };
    if cells.iter().any(|&square| state.at(square).is_some()) {
        return Ok(false);
    }
    for square in cells {
        summon_passive_piece(state, color, "alfil", square)?;
    }
    Ok(true)
}

fn last_stand(state: &mut GameState, color: Color) -> Result<bool> {
    let candidates = board_entries(state, color, "pawn");
    if candidates.is_empty() {
        return Ok(false);
    }
    let context = state.clone();
    for (square, original) in candidates {
        let mut updated = original.clone();
        updated.kind = "fanatic".into();
        crate::card_effects::mark_transformed_origin(&context, &mut updated, square)?;
        replace_aliases(state, square, &original.id, Some(updated));
    }
    Ok(true)
}

fn mongolian_gambit(state: &mut GameState, color: Color) -> Result<bool> {
    let monochrome = crate::observation::truth(state.extra.get("monochromeChess"));
    let kind = if monochrome { "camel" } else { "knight" };
    let mut seen = BTreeSet::new();
    let mut targets = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(piece) = state.at(square) else {
                continue;
            };
            if piece.color != color
                || crate::v7_threat::is_royal_identity_v7(state, piece)
                || matches!(piece.kind.as_str(), "merchant" | "wall")
                || !seen.insert(if piece.id.is_empty() {
                    format!("@{row}:{col}")
                } else {
                    piece.id.clone()
                })
            {
                continue;
            }
            targets.push((square, piece.clone()));
        }
    }
    if targets.is_empty() {
        return Ok(false);
    }
    for (square, original) in targets {
        if original.is_large() {
            let cells = (0..8)
                .flat_map(|row| (0..8).map(move |col| Square { row, col }))
                .filter(|&cell| state.at(cell).is_some_and(|piece| piece.id == original.id))
                .collect::<Vec<_>>();
            if cells.is_empty() {
                return Err(EngineError::InvalidState(
                    "mongolian large target lost".into(),
                ));
            }
            replace_aliases(state, square, &original.id, None);
            if matches!(original.kind.as_str(), "bigRook" | "bigBishop") && cells.len() == 4 {
                for cell in cells {
                    let mut spawned = crate::opening::spawn(state, color, kind)?;
                    crate::card_effects::mark_transformed_origin(state, &mut spawned, cell)?;
                    spawned.moved = true;
                    crate::card_effects::mark_animation(state, &spawned)?;
                    state.board[cell.row as usize][cell.col as usize] = Some(spawned);
                }
            } else {
                let mut spawned = crate::opening::spawn(state, color, kind)?;
                for field in ["capturesMade", "totalCaptures"] {
                    if let Some(value) = original
                        .extra
                        .get(field)
                        .filter(|value| value.as_i64().is_some_and(|number| number > 0))
                    {
                        spawned.extra.insert(field.into(), value.clone());
                    }
                }
                crate::card_effects::mark_transformed_origin(state, &mut spawned, square)?;
                spawned.moved = true;
                state.board[square.row as usize][square.col as usize] = Some(spawned);
            }
            continue;
        }
        let mut updated = original.clone();
        let already_kind = original.kind == kind || (monochrome && original.kind == "knight");
        updated.kind = kind.into();
        crate::card_effects::mark_transformed_origin_with_options(
            state,
            &mut updated,
            square,
            already_kind,
        )?;
        updated.extra.insert("shielded".into(), json!(false));
        updated.extra.insert("logDir".into(), Value::Null);
        updated.moved = true;
        replace_aliases(state, square, &original.id, Some(updated));
    }
    Ok(true)
}

/// Source isMajorPiece (main:68493), shared by removal and Majesty geometry.
pub(crate) fn is_major_piece(state: &GameState, piece: &Piece) -> bool {
    const MAJORS: &[&str] = &[
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
    if piece.extra.get("regencyHeir") == Some(&Value::Bool(true)) {
        return false;
    }
    // isMajorPiece requires a truthy source color, including neutral. The
    // current and historical catalog predicates select different type sets.
    if crate::card_effects::september26(state) {
        MAJORS.contains(&piece.kind.as_str())
    } else {
        (MAJORS.contains(&piece.kind.as_str())
            && !["octopus", "grappler"].contains(&piece.kind.as_str()))
            || [
                "clockwork",
                "octopus",
                "parrot",
                "recruiter",
                "wizard",
                "trickster",
            ]
            .contains(&piece.kind.as_str())
    }
}

fn blue_jeans(state: &mut GameState) -> Result<bool> {
    let mut seen = BTreeSet::new();
    let mut targets = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(piece) = state.at(square) else {
                continue;
            };
            if !is_major_piece(state, piece) {
                continue;
            }
            let key = if piece.id.is_empty() {
                format!("@{row}:{col}")
            } else {
                piece.id.clone()
            };
            if seen.insert(key) {
                targets.push((square, piece.clone()));
            }
        }
    }
    if targets.is_empty() {
        return Ok(false);
    }
    for (square, piece) in targets {
        if piece.is_large() && piece.id.is_empty() {
            return Err(EngineError::InvalidState(
                "blue-jeans large piece lacks identity".into(),
            ));
        }
        replace_aliases(state, square, &piece.id, None);
        if let Some(owner) = piece.color.owner() {
            crate::transition::resolve_royal_capture(state, &piece, owner.opponent())?;
        }
    }
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(true)
}

fn cloneable_passive(card: &CardSlot) -> bool {
    card.id != "clone"
        && !["big-bishop", "horde", "big-rook", "london-system"].contains(&card.id.as_str())
        && (owns(&card.id) || card.id == "white-box")
}

fn cloned_ledger(state: &mut GameState, color: Color) -> Result<&mut Vec<Value>> {
    let root = state
        .extra
        .entry("clonedPassiveCards")
        .or_insert_with(|| json!({"white":[],"black":[]}));
    let sides = root.as_object_mut().ok_or_else(|| {
        EngineError::InvalidState("clonedPassiveCards must be a color map".into())
    })?;
    let ledger = sides.entry(color.as_str()).or_insert_with(|| json!([]));
    ledger.as_array_mut().ok_or_else(|| {
        EngineError::InvalidState("clonedPassiveCards owner ledger must be an array".into())
    })
}

fn clone_passive_for_recipient(
    state: &mut GameState,
    recipient: Color,
    source: Color,
    card: &CardSlot,
) -> Result<bool> {
    if !cloneable_passive(card) {
        return Ok(false);
    }
    let key = format!(
        "{}:{}",
        source.as_str(),
        if card.instance_id.is_empty() {
            &card.id
        } else {
            &card.instance_id
        }
    );
    if cloned_ledger(state, recipient)?
        .iter()
        .any(|entry| entry.as_str() == Some(&key))
    {
        return Ok(false);
    }
    let suffix =
        crate::draft::random_suffix(state.rng.sample_opaque("source passive clone identity")?)?
            .chars()
            .take(5)
            .collect::<String>();
    let timestamp = crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?;
    let mut shared = card.clone();
    shared.instance_id = format!(
        "clone-{}-{}-{timestamp}-{suffix}",
        recipient.as_str(),
        card.id
    );
    shared.used = false;
    shared.recovering = false;
    shared.extra.insert("passiveApplied".into(), json!(false));
    shared.extra.insert("devCard".into(), json!(false));
    let result = if shared.id == "white-box" {
        crate::card_effects::apply_v7_virtual_white_box(state, recipient, &shared)
    } else {
        apply_virtual_effect(state, recipient, &shared)
    };
    if !result? {
        return Ok(false);
    }
    // main:69138 records the successful clone before hazards and Herald can
    // end the game. A terminal reaction does not discard this source ledger.
    let ledger = cloned_ledger(state, recipient)?;
    ledger.push(json!(key));
    if ledger.len() > 64 {
        ledger.drain(..ledger.len() - 64);
    }
    crate::v7_board_hazards::post_card(state, recipient)?;
    crate::transition::resolve_herald_threats(state, recipient)?;
    crate::replay::add_log(
        state,
        format!(
            "{} 복제: {}",
            crate::replay::label(recipient),
            card.extra
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("undefined")
        ),
    )?;
    Ok(true)
}

fn clone_existing_opponent_passives(state: &mut GameState, recipient: Color) -> Result<()> {
    let source = recipient.opponent();
    let existing = state
        .deck_slots
        .get(source)
        .iter()
        .filter(|card| {
            cloneable_passive(card)
                && (card.used || card.extra.get("passiveApplied") == Some(&json!(true)))
        })
        .cloned()
        .collect::<Vec<_>>();
    for card in existing {
        let _ = clone_passive_for_recipient(state, recipient, source, &card)?;
    }
    Ok(())
}

fn apply_clone_passive(state: &mut GameState, color: Color) -> Result<bool> {
    let sides = state
        .extra
        .entry("clonePassive")
        .or_insert_with(|| json!({"white":false,"black":false}));
    sides
        .as_object_mut()
        .ok_or_else(|| EngineError::InvalidState("clonePassive must be a color map".into()))?
        .insert(color.as_str().into(), json!(true));
    clone_existing_opponent_passives(state, color)?;
    Ok(true)
}

fn false_start_quantum_at(state: &GameState, square: Square) -> Result<bool> {
    for piece in state.board.iter().flatten().flatten() {
        let Some(quantum) = piece.extra.get("quantum").filter(|value| !value.is_null()) else {
            continue;
        };
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

fn false_start_reserved(state: &GameState, square: Square) -> Result<bool> {
    let matches = |entry: &Value| {
        entry.get("row").and_then(Value::as_u64) == Some(u64::from(square.row))
            && entry.get("col").and_then(Value::as_u64) == Some(u64::from(square.col))
    };
    for field in ["pendingScarecrows", "pendingLobsters"] {
        if let Some(entries) = state.extra.get(field).filter(|value| !value.is_null()) {
            let entries = entries
                .as_array()
                .ok_or_else(|| EngineError::InvalidState(format!("{field} must be an array")))?;
            if entries.iter().any(|entry| {
                matches(entry)
                    && (field == "pendingLobsters"
                        || !crate::observation::truth(entry.get("pieceId")))
            }) {
                return Ok(true);
            }
        }
    }
    if let Some(entries) = state
        .extra
        .get("pendingPortals")
        .filter(|value| !value.is_null())
    {
        let entries = entries
            .as_array()
            .ok_or_else(|| EngineError::InvalidState("pendingPortals must be an array".into()))?;
        for entry in entries {
            if entry.get("blocksMovement") != Some(&Value::Bool(true)) {
                continue;
            }
            let cells = entry
                .get("cells")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    EngineError::InvalidState("pendingPortals.cells must be an array".into())
                })?;
            if cells.iter().any(matches) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn false_start_blocked(state: &GameState, square: Square) -> Result<bool> {
    Ok(crate::movement::portal_installation_hazard(state, square)
        || false_start_reserved(state, square)?
        || false_start_quantum_at(state, square)?)
}

fn union_root(parent: &mut [usize; 8], column: usize) -> usize {
    if parent[column] != column {
        let next = parent[column];
        parent[column] = union_root(parent, next);
    }
    parent[column]
}

fn false_start(state: &mut GameState, color: Color) -> Result<bool> {
    if !apply_idempotent_color_flag(state, "falseStart", color)? {
        return Ok(false);
    }
    let mut seen = BTreeSet::new();
    let mut pieces = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(piece) = state.at(square) else {
                continue;
            };
            if piece.color != color {
                continue;
            }
            let key = if piece.id.is_empty() {
                format!("@{row}:{col}")
            } else {
                piece.id.clone()
            };
            if !seen.insert(key) {
                continue;
            }
            let cells = if piece.id.is_empty() {
                vec![square]
            } else {
                (0..8)
                    .flat_map(|r| (0..8).map(move |c| Square { row: r, col: c }))
                    .filter(|&at| state.at(at).is_some_and(|found| found.id == piece.id))
                    .collect::<Vec<_>>()
            };
            pieces.push((square, piece.clone(), cells));
        }
    }
    let mut parent = [0, 1, 2, 3, 4, 5, 6, 7];
    for (from, _, cells) in &pieces {
        for cell in cells {
            let root = union_root(&mut parent, from.col as usize);
            let group = union_root(&mut parent, cell.col as usize);
            parent[group] = root;
        }
    }
    let enemy = (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter(|&square| state.at(square).is_some_and(|piece| piece.color != color))
        .collect::<BTreeSet<_>>();
    let direction: i8 = if color == Color::White { -1 } else { 1 };
    let mut stopped = BTreeSet::new();
    let mut shifts = std::collections::BTreeMap::new();
    for step in 1..=2i8 {
        for (from, _, cells) in &pieces {
            let group = union_root(&mut parent, from.col as usize);
            if stopped.contains(&group) {
                continue;
            }
            let mut blocked = false;
            for cell in cells {
                let Some(arrival) = cell.offset(direction * step, 0) else {
                    blocked = true;
                    break;
                };
                if enemy.contains(&arrival) || false_start_blocked(state, arrival)? {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                stopped.insert(group);
            }
        }
        let mut denied = BTreeSet::new();
        for (from, _, cells) in &pieces {
            let group = union_root(&mut parent, from.col as usize);
            if cells.iter().any(|cell| {
                cell.offset(direction * step, 0).is_some_and(|arrival| {
                    !crate::movement::expansion_destination_allowed(state, color.into(), &[arrival])
                })
            }) {
                denied.insert(group);
            }
        }
        for (from, _, _) in &pieces {
            let group = union_root(&mut parent, from.col as usize);
            if !stopped.contains(&group) && !denied.contains(&group) {
                shifts.insert(group, step);
            }
        }
    }
    let plan = pieces
        .into_iter()
        .filter_map(|(from, piece, cells)| {
            let group = union_root(&mut parent, from.col as usize);
            shifts
                .get(&group)
                .copied()
                .map(|step| (from, piece, cells, direction * step))
        })
        .collect::<Vec<_>>();
    for (_, _, cells, _) in &plan {
        for cell in cells {
            state.board[cell.row as usize][cell.col as usize] = None;
        }
    }
    for (from, mut piece, cells, delta) in plan {
        if let Some(anchor) = piece.extra.get("anchorRow").and_then(Value::as_i64) {
            piece
                .extra
                .insert("anchorRow".into(), json!(anchor + i64::from(delta)));
        }
        if crate::observation::truth(piece.extra.get("locustOrigin")) {
            piece.extra.insert(
                "locustOrigin".into(),
                json!({"row":i16::from(from.row)+i16::from(delta),"col":from.col}),
            );
        }
        for cell in cells {
            let arrival = cell
                .offset(delta, 0)
                .ok_or_else(|| EngineError::InvalidState("false-start plan left board".into()))?;
            state.board[arrival.row as usize][arrival.col as usize] = Some(piece.clone());
        }
    }
    state.en_passant = None;
    Ok(true)
}

/// The effect kernel is intentionally narrow. `false` is a source-declined
/// passive (no matching piece or already active); an unported active branch is
/// never treated as success. The caller has not marked the deck card used yet.
fn apply_direct_effect(state: &mut GameState, color: Color, card: &CardSlot) -> Result<bool> {
    match card.id.as_str() {
        "false-start" => false_start(state, color),
        "checker" => checker(state, color),
        "elephant-escape" => elephant_escape(state, color),
        "last-stand" => last_stand(state, color),
        "mongolian-gambit" => mongolian_gambit(state, color),
        "conversion" => convert_knights(state, color),
        "underground-bunker" => underground_bunker(state, color),
        "horse-riding" => horse_riding(state, color),
        "blue-jeans" => blue_jeans(state),
        "clone" => apply_clone_passive(state, color),
        "field-promotion" | "frontline-response" | "majesty" | "infiltration" | "killer-king"
        | "resolve" | "vanguard" => {
            // main:100144/100199/100209. Later movement or capture callbacks
            // belong to their respective owners.
            set_normalized_color_flag(state, &card.effect, color);
            Ok(true)
        }
        "gomoku" => {
            set_normalized_color_flag(state, "gomoku", color);
            Ok(true)
        }
        "d4" | "synchronization" | "assembly" | "vigilance" | "highlander" | "proficiency"
        | "long-en-passant" | "mutation" => {
            // main:633, 15942, 20823. The highlander win check is part of
            // `checkReligiousVictory`, which runs after acquisition.
            apply_idempotent_color_flag(state, &card.effect, color)
        }
        "locust-swarm" => {
            if !apply_idempotent_color_flag(state, &card.effect, color)? {
                return Ok(false);
            }
            let mut seen = BTreeSet::new();
            for row in 0..8 {
                for col in 0..8 {
                    let square = Square { row, col };
                    if let Some(piece) = state.at_mut(square)
                        && piece.color == color
                        && piece.kind != "pawn"
                        && !piece.moved
                        && piece.extra.get("locustUsed") != Some(&json!(true))
                        && seen.insert(piece.id.clone())
                    {
                        piece.extra.insert("locustOrigin".into(), json!(square));
                    }
                }
            }
            Ok(true)
        }
        "geneva-convention" | "king-of-the-hill" => {
            set_normalized_color_flag(
                state,
                if card.id == "king-of-the-hill" {
                    "hillKing"
                } else {
                    "genevaConvention"
                },
                color,
            );
            Ok(true)
        }
        "early-promotion" | "fast-growth" | "backward-knight" | "iron-monarch" | "coronation"
        | "mad-horse" | "overwhelm" | "underpromotion" | "final-weapon" | "file-surge"
        | "rook-lift" | "encouragement" | "imperial-studies" | "bina-mate" | "racing-king"
        | "radical-charge" | "retreat" | "fianchetto" | "pawn-conversion" | "sprint" | "leap"
        | "corner-kick" | "injury" | "queen-afterimage" => {
            let needed = match card.id.as_str() {
                "retreat" | "pawn-conversion" | "sprint" | "leap" => Some("pawn"),
                "corner-kick" | "radical-charge" => Some("knight"),
                "fianchetto" => Some("bishop"),
                _ => None,
            };
            if needed.is_some_and(|kind| !has_piece(state, color, kind)) {
                return Ok(false);
            }
            if matches!(
                card.id.as_str(),
                "encouragement" | "imperial-studies" | "racing-king"
            ) && !has_royal(state, color)
            {
                return Ok(false);
            }
            if card.id == "corner-kick" && state.flag("cornerKick", color) {
                return Ok(false);
            }
            let (field, side) = match card.id.as_str() {
                "bina-mate" => ("binaMate", color),
                "injury" => ("knightInjury", color.opponent()),
                "king-of-the-hill" => ("hillKing", color),
                "queen-afterimage" => ("afterimageQueen", color),
                _ => (card.effect.as_str(), color),
            };
            // Some older functions create the map; the source-initialized
            // default covers the rest. Invalid shapes are never repaired here.
            if state.extra.get(field).is_none() {
                set_normalized_color_flag(state, field, side);
            } else {
                set_existing_color_flag(state, field, side)?;
            }
            if card.id == "racing-king" {
                // Source racingKing (main:105554) adjudicates immediately,
                // before applyCard's reconciliation and the acquisition log.
                crate::v7_threat::check_racing_kings_v7(state)?;
            }
            Ok(true)
        }
        "democracy" => {
            if state.flag("democracy", color) || !has_piece(state, color, "pawn") {
                return Ok(false);
            }
            set_normalized_color_flag(state, "democracy", color);
            Ok(true)
        }
        "religious-victory" => {
            // Its immediate win probe is checked after the effect, before the
            // deck instance is consumed.
            set_normalized_color_flag(state, "religiousVictory", color);
            crate::v7_passive_terminal::after_religious_card_effect(state)?;
            Ok(true)
        }
        "ghost" => {
            let mut changed = false;
            let mut seen = BTreeSet::new();
            let mut animated = Vec::new();
            for row in 0..8 {
                for col in 0..8 {
                    if let Some(piece) = state.at_mut(Square { row, col })
                        && piece.color == color
                        && piece.kind == "pawn"
                        && piece.extra.get("ghost") != Some(&json!(true))
                        && seen.insert(piece.id.clone())
                    {
                        piece.extra.insert("ghost".into(), json!(true));
                        animated.push(piece.clone());
                        changed = true;
                    }
                }
            }
            for piece in animated {
                crate::card_effects::mark_animation(state, &piece)?;
            }
            Ok(changed)
        }
        "exhaustion" => {
            let prior = state
                .extra
                .get("exhaustion")
                .ok_or_else(|| EngineError::InvalidState("exhaustion state missing".into()))?;
            if !prior["white"].is_object() || !prior["black"].is_object() {
                return Err(EngineError::InvalidState(
                    "exhaustion must be a color map".into(),
                ));
            }
            let mut sides = json!({"white":prior["white"],"black":prior["black"]});
            sides[color.opponent().as_str()] = json!({"enabled":true,"pieceId":"","count":0});
            state.extra.insert("exhaustion".into(), sides);
            Ok(true)
        }
        "moving" => {
            if state.extra["moving"][color.as_str()]["enabled"] == json!(true) {
                return Ok(false);
            }
            let available = state.board.iter().flatten().flatten().any(|piece| {
                piece.color == color
                    && !["wall", "football", "blackHole", "scarecrow"]
                        .contains(&piece.kind.as_str())
            });
            if !available {
                return Ok(false);
            }
            let prior = state
                .extra
                .get("moving")
                .ok_or_else(|| EngineError::InvalidState("moving state missing".into()))?;
            if !prior["white"].is_object() || !prior["black"].is_object() {
                return Err(EngineError::InvalidState(
                    "moving must be a color map".into(),
                ));
            }
            let mut sides = json!({"white":prior["white"],"black":prior["black"]});
            sides[color.as_str()] = json!({"enabled":true,"pieceId":"","count":0});
            state.extra.insert("moving".into(), sides);
            Ok(true)
        }
        "brutus" => {
            // main:15949-15960 uses board.flat() candidate order and draws
            // once only when an eligible non-royal rook exists.
            let candidates = state
                .board
                .iter()
                .enumerate()
                .flat_map(|(row, line)| {
                    line.iter().enumerate().filter_map(move |(col, cell)| {
                        cell.as_ref().map(|_| Square {
                            row: row as u8,
                            col: col as u8,
                        })
                    })
                })
                .filter(|square| {
                    state.at(*square).is_some_and(|piece| {
                        piece.color == color
                            && piece.kind == "rook"
                            && !crate::v7_threat::is_royal_identity_v7(state, piece)
                    })
                })
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Ok(false);
            }
            // The same source floor(sample * count) draw also contributes its
            // probability to conditioned hidden-draft transitions.
            let index = crate::transition::sample_choice(state, candidates.len())?;
            let square = candidates[index];
            let lock =
                state.turns_taken.get(color).checked_add(1).ok_or_else(|| {
                    EngineError::InvalidState("brutus capture lock overflow".into())
                })?;
            let piece = state
                .at_mut(square)
                .ok_or_else(|| EngineError::InvalidState("brutus rook disappeared".into()))?;
            piece.kind = "brutus".into();
            piece.moved = true;
            piece
                .extra
                .insert("freshNoCaptureUntil".into(), json!(lock));
            Ok(true)
        }
        "initiative" => {
            let start_turn = *state.turns_taken.get(color.opponent());
            let sides = state
                .extra
                .get_mut("initiative")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| {
                    EngineError::InvalidState("initiative must be a color map".into())
                })?;
            sides.insert(
                color.opponent().as_str().into(),
                json!({"by":color,"startTurn":start_turn,"limit":10}),
            );
            Ok(true)
        }
        "london-system" | "horde" | "big-rook" | "big-bishop" | "dutch" | "apprentice-knights" => {
            crate::opening::apply(state, color, &card.effect)?.ok_or_else(|| {
                EngineError::UnsupportedFeature(format!("passive opening effect {}", card.id))
            })
        }
        "white-box" => unreachable!("choice-card object owns white-box"),
        _ => Err(EngineError::UnsupportedFeature(format!(
            "acquired passive {}",
            card.id
        ))),
    }
}

/// Apply one source-pinned passive effect selected by a virtual card such as
/// White Box. This is the source `applyCard` boundary: direct effect plus its
/// successful type/crown/capture/check reconciliation. The virtual instance
/// is not a deck card; its caller owns candidate order, post-card hazards,
/// Herald, and the box log. A declined effect changes no snapshot field or
/// RNG, but keeps the source's freshly captured board-action origins. That
/// execution context can affect infiltration later in the enclosing move.
/// Successful-card reconciliation never runs for a declined effect.
pub(crate) fn apply_virtual_effect(
    state: &mut GameState,
    color: Color,
    card: &CardSlot,
) -> Result<bool> {
    ensure_passive(state, card)?;
    let before = state.clone();
    state.turn = color;
    let result = (|| {
        let context = crate::transition::begin_v7_card(state)?;
        let applied = apply_direct_effect(state, color, card)?;
        if applied {
            crate::transition::finish_v7_card(state, context)?;
        }
        Ok(applied)
    })();
    state.turn = before.turn;
    match result {
        Ok(true) => Ok(true),
        Ok(false) => Ok(false),
        Err(error) => {
            *state = before;
            Err(error)
        }
    }
}

/// Finish a successfully applied PASSIVE deck card, including White Box.
/// The caller has already run the effect and the source's post-card board
/// reactions. This boundary owns only the deck instance, terminal probe,
/// passive log, and ordered Clone sharing.
pub(crate) fn finish_acquisition_after_effect(
    state: &mut GameState,
    color: Color,
    slot: usize,
) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::InvalidState(
            "v7 passive settlement requires v7 rules".into(),
        ));
    }
    let card = state
        .deck_slots
        .get(color)
        .get(slot)
        .ok_or(EngineError::IllegalAction)?
        .clone();
    let definition = crate::card_registry::definition_for(&state.ruleset_id, &card.id)?;
    if definition.activation != Some(CardActType::Passive)
        || definition.effect != card.effect
        || card.vacant
        || card.used
    {
        return Err(EngineError::IllegalAction);
    }
    let before = state.clone();
    let result = (|| {
        let used_at = crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?;
        let live = &mut state.deck_slots.get_mut(color)[slot];
        live.used = true;
        live.recovering = false;
        live.source_order.retain(|field| field != "recovering");
        live.extra.insert("passiveApplied".into(), json!(true));
        live.extra.insert("usedAt".into(), json!(used_at));
        crate::v7_passive_terminal::after_passive_acquisition(state)?;
        crate::replay::add_log(
            state,
            format!(
                "{} 패시브: {}",
                crate::replay::label(color),
                card.extra
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("undefined")
            ),
        )?;
        if state.flag("clonePassive", color.opponent()) {
            clone_passive_for_recipient(state, color.opponent(), color, &card)?;
        }
        Ok(())
    })();
    if result.is_err() {
        *state = before;
    }
    result
}

/// Source `applyPassiveCardOnDraft` (main:69165). `false` covers both a
/// non-applicable callback and a source-declined effect. The caller owns
/// `noteCardEvent`, gain notation, replay capture, and draft phase changes.
pub(super) fn apply_acquisition(state: &mut GameState, color: Color, slot: usize) -> Result<bool> {
    let card = state
        .deck_slots
        .get(color)
        .get(slot)
        .ok_or(EngineError::IllegalAction)?
        .clone();
    ensure_passive(state, &card)?;
    if card.vacant
        || card.used
        || card.extra.get("devCard") == Some(&json!(true))
        || card.extra.get("nextTurnPending") == Some(&json!(true))
    {
        return Ok(false);
    }
    if state
        .extra
        .get("aiSimulationDepth")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        return Ok(false);
    }
    let before = state.clone();
    let effect = apply_virtual_effect(state, color, &card);
    let applied = match effect {
        Ok(value) => value,
        Err(error) => {
            *state = before;
            return Err(error);
        }
    };
    if !applied {
        return Ok(false);
    }
    if let Err(error) = crate::v7_board_hazards::post_card(state, color) {
        *state = before;
        return Err(error);
    }
    if let Err(error) = crate::transition::resolve_herald_threats(state, color) {
        *state = before;
        return Err(error);
    }
    if let Err(error) = finish_acquisition_after_effect(state, color, slot) {
        *state = before;
        return Err(error);
    }
    Ok(true)
}

/// Before applying first-move effects, check that every scheduled card has
/// exactly one source effect owner. The frozen auxiliary GUN and dynamic
/// campaign definitions are admitted separately from the public catalog.
pub(super) fn first_move_preflight(state: &GameState, color: Color) -> Result<Vec<usize>> {
    let slots = first_move_auto_slots(state, color)?;
    for &slot in &slots {
        let card = &state.deck_slots.get(color)[slot];
        let definition = crate::card_registry::definition_for(&state.ruleset_id, &card.id)?;
        if card.id == "black-tower-legacy-magic" {
            crate::v7_campaign::validate_black_tower_card_instance(state, card)?;
            if definition.card_type.is_none()
                && definition.activation == Some(CardActType::Active)
                && definition.source_definition.get("phase") == Some(&json!("???"))
                && definition.effect == "blackTowerLegacyMagic"
                && definition.effect == card.effect
                && crate::card_effects::v7_manual_effect_owner_count(&card.id) == 1
            {
                continue;
            }
            return Err(EngineError::UnsupportedFeature(
                "first-move Black Tower card has no exact dynamic definition and unique effect owner".into(),
            ));
        }
        if card.id == "shotgun-king" {
            if definition.card_type.is_none()
                && definition.activation.is_none()
                && definition.source_definition.get("phase") == Some(&json!("GUN"))
                && definition.effect == "shotgunKing"
                && definition.effect == card.effect
                && crate::card_effects::v7_manual_effect_owner_count(&card.id) == 1
            {
                continue;
            }
            return Err(EngineError::UnsupportedFeature(
                "first-move auxiliary shotgun-king has no exact GUN definition and unique effect owner".into(),
            ));
        }
        let owned = match definition.activation {
            Some(CardActType::Passive) => owns(&card.id),
            Some(CardActType::Active | CardActType::ActiveForced) => {
                crate::card_effects::v7_manual_effect_owner_count(&card.id) == 1
            }
            None => false,
        };
        if definition.card_type != Some(CardType::Opening)
            || definition.effect != card.effect
            || !owned
        {
            return Err(EngineError::UnsupportedFeature(format!(
                "first-move automatic v7 card {} has no unique source effect owner",
                card.id
            )));
        }
    }
    Ok(slots)
}

/// Source failure text is observable in the forced-opening log. This lookup
/// consumes no randomness: the caller has already attempted the target/effect
/// and, if permitted, its first-move rollback retry (main:87905-88114).
pub(super) fn first_move_failure_message(
    state: &GameState,
    color: Color,
    card: &CardSlot,
) -> Result<String> {
    let definition = crate::card_registry::definition_for(&state.ruleset_id, &card.id)?;
    if state.ruleset_id != RULES_VERSION_V7 || definition.effect != card.effect {
        return Err(EngineError::InvalidState(format!(
            "v7 first-move failure lookup catalog mismatch for {}",
            card.id
        )));
    }
    let any =
        |predicate: &dyn Fn(&Piece) -> bool| state.board.iter().flatten().flatten().any(predicate);
    let own_queen = || {
        any(&|piece| {
            piece.color == color
                && piece.kind == "queen"
                && piece.extra.get("regencyHeir") != Some(&json!(true))
        })
    };
    let name = card
        .extra
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| {
            definition
                .source_definition
                .get("name")
                .and_then(Value::as_str)
        })
        .unwrap_or("undefined");
    let no_target = || format!("{name} 자동 발동 대상이 없습니다.");
    if crate::observation::truth(card.extra.get("target")) {
        let text = if card.id == "grappler" && crate::card_effects::september26(state) {
            "그래플러에 필요한 아군 퀸과 마이너 피스가 없습니다."
        } else {
            match card.effect.as_str() {
                "amazon" if !own_queen() => "아마존으로 바꿀 퀸이 없습니다.",
                "amazon" => "아마존을 위해 희생할 나이트가 없습니다.",
                "hook" if !own_queen() => "구행으로 바꿀 퀸이 없습니다.",
                "hook" => "구행을 만들기 위해 희생할 룩이 없습니다.",
                "windmill" if !has_piece(state, color, "bishop") => {
                    "풍차로 바꿀 아군 비숍이 없습니다."
                }
                "windmill" => "풍차로 바꿀 아군 룩이 없습니다.",
                "feudalContract"
                    if !any(&|piece| {
                        piece.color == color
                            && ["pawn", "fanatic"].contains(&piece.kind.as_str())
                            && !crate::observation::truth(piece.extra.get("explosive"))
                            && !crate::observation::truth(piece.extra.get("feudalContractId"))
                    }) =>
                {
                    "봉건 계약을 맺을 아군 폰이나 광신도가 없습니다."
                }
                "feudalContract" => "봉건 계약을 맺을 비폰 아군 기물이 없습니다.",
                _ => {
                    let mut targeting = state.clone();
                    targeting.turn = color;
                    let targets = crate::card_effects::target_squares(&targeting, card)?
                        .ok_or_else(|| {
                            EngineError::UnsupportedFeature(format!(
                                "first-move target failure lookup {}",
                                card.id
                            ))
                        })?;
                    if targets.is_empty() {
                        return Ok(no_target());
                    }
                    return if card.effect == "queensGambit" {
                        Ok("두 번째 보호 파일을 정할 수 없습니다.".into())
                    } else {
                        Err(EngineError::UnsupportedFeature(format!(
                            "first-move targeted effect failure {}",
                            card.id
                        )))
                    };
                }
            }
        };
        return Ok(text.into());
    }
    let text = match card.id.as_str() {
        "black-tower-legacy-magic"
            if color != Color::Black
                || state
                    .extra
                    .get("campaign")
                    .and_then(|campaign| campaign.get("setup"))
                    .and_then(Value::as_str)
                    != Some("blackTower") =>
        {
            "검은 마탑 전용 카드입니다."
        }
        "black-tower-legacy-magic"
            if crate::v7_campaign::black_tower_king_square(state).is_none() =>
        {
            "아군 킹이 없습니다."
        }
        "black-tower-legacy-magic" => {
            return Err(EngineError::InvalidState(
                "v7 Black Tower effect declined despite an eligible campaign, actor, and king"
                    .into(),
            ));
        }
        "false-start" | "locust-swarm" | "d4" | "synchronization" | "assembly" | "vigilance"
        | "highlander" | "proficiency" | "long-en-passant" | "mutation" => {
            "이미 적용되어 있습니다."
        }
        "democracy" if state.flag("democracy", color) => "민주주의가 이미 적용되어 있습니다.",
        "democracy" => "민주주의를 지탱할 아군 폰이 없습니다.",
        "checker" if crate::card_effects::september26(state) => {
            "체커가 나타날 네 칸이 비어 있어야 합니다."
        }
        "checker" => "체커로 바꿀 양 끝 파일의 폰이 없습니다.",
        "elephant-escape" => "알필이 나타날 양 끝 파일의 칸이 비어 있어야 합니다.",
        "apprentice-knights" => "종자로 바꿀 양쪽 두 번째 파일의 폰이 없습니다.",
        "horde" if !has_royal(state, color) => "호드 배치를 지킬 킹이 없습니다.",
        "horde" => "호드 배치의 킹 자리에 상대 기물이 있어 전환할 수 없습니다.",
        "last-stand" => "변이시킬 폰이 없습니다.",
        "fianchetto" => "피앙케토 대각선에 들어갈 비숍이 없습니다.",
        "pawn-conversion" => "전환할 아군 폰이 없습니다.",
        "sprint" => "질주할 폰이 없습니다.",
        "leap" => "도약할 폰이 없습니다.",
        "retreat" => "뒤로 움직일 폰이 없습니다.",
        "conversion" => "비숍으로 전도할 아군 나이트가 없습니다.",
        "horse-riding" => "승마를 배울 킹이 없습니다.",
        "encouragement" => "독려할 킹이 없습니다.",
        "imperial-studies" => "제왕학을 배울 킹이 없습니다.",
        "racing-king" => "레이싱 킹을 적용할 킹이 없습니다.",
        "underground-bunker" => "지하벙커를 적용할 아군 킹이 없습니다.",
        "radical-charge" => "난폭한 돌진을 적용할 나이트가 없습니다.",
        "corner-kick" if !has_piece(state, color, "knight") => {
            "코너킥을 익힐 아군 나이트가 없습니다."
        }
        "corner-kick" => "코너킥이 이미 적용되어 있습니다.",
        "ghost" => "고스트 효과를 부여할 아군 폰이 없습니다.",
        "moving" if state.extra["moving"][color.as_str()]["enabled"] == json!(true) => {
            "무빙이 이미 활성화되어 있습니다."
        }
        "moving" => "움직일 아군 기물이 없습니다.",
        "brutus" => "변경할 아군 룩이 없습니다.",
        "blue-jeans" => "추방할 메이저 피스가 없습니다.",
        "mongolian-gambit" => {
            let kind = if crate::observation::truth(state.extra.get("monochromeChess")) {
                "낙타"
            } else {
                "나이트"
            };
            return Ok(format!("{kind}로 바꿀 말이 없습니다."));
        }
        "guard" => "킹 바로 앞에 근위병으로 바꿀 아군 폰이 없습니다.",
        "otherworld" => "이세계로 보낼 아군 폰이 없습니다.",
        "qxe1" => "왕위를 찬탈할 수 있는 아군 퀸이 없습니다.",
        "summon-colossus" => "희생할 폰 6개가 필요합니다.",
        "knightmate" => "로얄 나이트로 바꿀 킹이 없습니다.",
        "thief" => "변경할 아군 퀸이 없습니다.",
        "calling-card" => "예고장을 보낼 상대 비폰 기물이 없습니다.",
        "martyrdom" if !has_piece(state, color, "bishop") => "희생할 아군 비숍이 없습니다.",
        "martyrdom" => "가호를 부여할 아군 폰이 없습니다.",
        "queen-cavalry" => {
            let file = if diagonal_chess_active(state) {
                if color == Color::White { "a" } else { "h" }
            } else {
                "d"
            };
            return Ok(format!(
                "{file}폰이 있어야 퀸의 기병대를 적용할 수 있습니다."
            ));
        }
        "merchant-guild" => {
            let king = board_entries(state, color, "king").into_iter().next();
            return Ok(match king {
                None => "상인으로 바꿀 킹이 없습니다.",
                Some((_, king))
                    if crate::observation::truth(king.extra.get("undergroundBunker")) =>
                {
                    "지하벙커에 들어간 킹은 상인 조합으로 이동할 수 없습니다."
                }
                Some((square, _)) => {
                    let destination = square.offset(if color == Color::White { -2 } else { 2 }, 0);
                    match destination {
                        None => "상인이 이동할 칸이 보드 밖입니다.",
                        Some(square)
                            if state.at(square).is_some_and(|piece| {
                                ["colossus", "bigRook", "bigBishop"].contains(&piece.kind.as_str())
                            }) =>
                        {
                            "상인 조합의 소환 위치가 2x2 기물에 막혀 있습니다."
                        }
                        Some(_) => "상인이 이동할 칸에 상대 기물이 있습니다.",
                    }
                }
            }
            .into());
        }
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "first-move automatic effect failure {}",
                card.id
            )));
        }
    };
    Ok(text.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, GameConfig, MoveTarget};
    use sha2::{Digest, Sha256};

    fn jcs_digest(value: &impl serde::Serialize) -> String {
        format!("{:x}", Sha256::digest(serde_jcs::to_vec(value).unwrap()))
    }

    fn source_state_digest(state: &GameState) -> String {
        let mut value = serde_json::to_value(state).unwrap();
        for field in ["rulesetId", "rng", "history"] {
            value.as_object_mut().unwrap().remove(field);
        }
        jcs_digest(&value)
    }

    fn with_acquired(id: &str) -> GameState {
        let mut state = crate::v7_new_game::new_game(GameConfig::default(), 37).unwrap();
        let mut source = crate::card_registry::definition_for(RULES_VERSION_V7, id)
            .unwrap()
            .source_definition
            .clone();
        source["instanceId"] = json!(format!("{id}-passive-test"));
        source["deckCard"] = json!(true);
        source["nextTurnPending"] = json!(false);
        source["slot"] = json!(0);
        state.deck_slots.white[0] = serde_json::from_value(source).unwrap();
        state
    }
    #[test]
    fn passive_ownership_matches_frozen_public_catalog() {
        let site: Value =
            serde_json::from_str(include_str!("../../contracts/catalog/site-20260928.json"))
                .unwrap();
        let expected = site["cards"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|card| card["activation"] == "PASSIVE" && card["id"] != "white-box")
            .map(|card| card["id"].as_str().unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(IDS.iter().copied().collect::<BTreeSet<_>>(), expected);
        assert_eq!(IDS.len(), expected.len());
    }
    #[test]
    fn acquisition_priorities_preserve_source_order() {
        assert_eq!(
            [
                "london-system",
                "horde",
                "big-rook",
                "big-bishop",
                "field-promotion",
                "false-start",
                "locust-swarm"
            ]
            .map(acquisition_priority),
            [0, 1, 2, 2, 3, 4, 5]
        );
    }
    #[test]
    fn field_promotion_acquisition_preserves_rng_and_history() {
        // Frozen client main:100209 -> 99694 -> 69165. The coverage oracle's
        // chaos seed 37 bundle applies the same field flag and consumes no
        // RNG beyond the surrounding draft offer/notation sequence.
        let mut state = with_acquired("field-promotion");
        let before = state.clone();
        assert!(apply_acquisition(&mut state, Color::White, 0).unwrap());
        assert_eq!(
            state.extra["fieldPromotion"],
            json!({"white":true,"black":false})
        );
        assert!(state.deck_slots.white[0].used);
        assert_eq!(
            state.deck_slots.white[0].extra["passiveApplied"],
            json!(true)
        );
        assert_eq!(state.rng, before.rng);
        assert_eq!(state.history, before.history);
        assert_eq!(state.turn, before.turn);
        assert_eq!(state.board, before.board);
    }
    #[test]
    fn white_box_virtual_passive_does_not_consume_a_deck_instance() {
        let mut state = with_acquired("field-promotion");
        let mut virtual_card = state.deck_slots.white[0].clone();
        virtual_card.extra.insert("devCard".into(), json!(true));
        let before = state.clone();
        assert!(apply_virtual_effect(&mut state, Color::White, &virtual_card).unwrap());
        assert_eq!(state.extra["fieldPromotion"]["white"], true);
        assert_eq!(state.deck_slots, before.deck_slots);
        assert_eq!(state.rng, before.rng);
        assert_eq!(state.history, before.history);
        assert_eq!(state.turn, before.turn);
        assert_eq!(state.extra.get("log"), before.extra.get("log"));
    }
    #[test]
    fn four_simple_first_move_targets_match_frozen_source_draws() {
        // Frozen main-OahWs0tU.js autoTargetForFirstMoveCard, default game
        // seed 37. The source picks exactly one target and advances RNG once.
        for (id, row, col) in [
            ("princess", 7, 0),
            ("loyalist", 6, 2),
            ("holdout", 6, 1),
            ("queens-gambit", 7, 3),
        ] {
            let mut state = with_acquired(id);
            let card = state.deck_slots.white[0].clone();
            assert_eq!(
                first_move_target(&mut state, &card).unwrap(),
                Some(json!({"row":row,"col":col})),
                "{id}"
            );
            assert_eq!(
                (state.rng.cursor, state.rng.state),
                (123, 751244298),
                "{id}"
            );
        }
    }
    #[test]
    fn declined_compound_targets_keep_the_source_draw_order() {
        // Frozen autoTargetForFirstMoveCard has three distinct failure
        // boundaries: grappler always evaluates both searches, amazon stops
        // after a missing queen, and a missing knight follows the queen draw.
        let mut grappler = with_acquired("grappler");
        grappler.board[7][3] = None;
        let card = grappler.deck_slots.white[0].clone();
        assert!(matches!(
            first_move_target(&mut grappler, &card),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!((grappler.rng.cursor, grappler.rng.state), (123, 751244298));

        let mut no_queen = with_acquired("amazon");
        no_queen.board[7][3] = None;
        let before = no_queen.clone();
        let card = no_queen.deck_slots.white[0].clone();
        assert!(matches!(
            first_move_target(&mut no_queen, &card),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(no_queen, before);

        let mut no_knight = with_acquired("amazon");
        no_knight.board[7][1] = None;
        no_knight.board[7][6] = None;
        let card = no_knight.deck_slots.white[0].clone();
        assert!(matches!(
            first_move_target(&mut no_knight, &card),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(
            (no_knight.rng.cursor, no_knight.rng.state),
            (123, 751244298)
        );
    }
    #[test]
    fn racing_passive_adjudicates_in_the_effect_before_deck_settlement() {
        // Frozen racingKing invokes checkRacingKings inside applyCardEffect,
        // before applyCard reconciliation or the later acquisition callback.
        let mut state = with_acquired("racing-king");
        let white_king = state.board[7][4].take().unwrap();
        let black_king = state.board[0][4].replace(white_king).unwrap();
        state.board[7][4] = Some(black_king);
        state.turn = Color::Black;
        let before = state.clone();
        let card = state.deck_slots.white[0].clone();
        assert!(apply_virtual_effect(&mut state, Color::White, &card).unwrap());
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner.as_deref(), Some("white"));
        assert_eq!(
            state.extra["replayEndReason"],
            "레이싱 킹이 목표 랭크에 도달했습니다."
        );
        assert_eq!(state.turn, before.turn);
        assert_eq!(state.deck_slots, before.deck_slots);
        assert_eq!(state.rng, before.rng);
        assert_eq!(state.history, before.history);
    }
    #[test]
    fn every_public_active_opening_has_one_first_move_effect_owner() {
        for id in [
            "calling-card",
            "guard",
            "holdout",
            "knightmate",
            "martyrdom",
            "merchant-guild",
            "otherworld",
            "queen-cavalry",
            "queens-gambit",
            "qxe1",
            "summon-colossus",
            "loyalist",
            "princess",
            "thief",
        ] {
            let mut state = with_acquired(id);
            state.deck_slots.white[0]
                .extra
                .insert("firstTurnCard".into(), json!(true));
            assert_eq!(
                first_move_preflight(&state, Color::White).unwrap(),
                vec![0],
                "{id}"
            );
        }
    }

    #[test]
    fn black_tower_dynamic_card_uses_source_registration_and_first_move_boundary() {
        let mut state = crate::v7_new_game::new_game(GameConfig::default(), 37).unwrap();
        state.extra.insert(
            "campaign".into(),
            json!({"setup":"blackTower","playerColor":"white"}),
        );
        let mut value = crate::v7_campaign::source_black_tower_card_definition();
        value["instanceId"] = json!("black-tower-registration-test");
        value["slot"] = json!(0);
        let mut card: CardSlot = serde_json::from_value(value).unwrap();
        register_acquired_card(&state, Color::Black, &mut card, "OPENING", false).unwrap();
        assert_eq!(card.extra["firstTurnCard"], true);
        assert_eq!(card.extra["nextTurnPending"], false);
        assert!(
            !owns(&card.id),
            "campaign magic never joins PASSIVE ownership"
        );
        state.deck_slots.black[0] = card.clone();
        assert_eq!(first_move_preflight(&state, Color::Black).unwrap(), vec![0]);
        assert_eq!(
            first_move_failure_message(&state, Color::White, &card).unwrap(),
            "검은 마탑 전용 카드입니다."
        );
        state.board[0][4] = None;
        assert_eq!(
            first_move_failure_message(&state, Color::Black, &card).unwrap(),
            "아군 킹이 없습니다."
        );
        state.mode = "play".into();
        register_acquired_card(&state, Color::Black, &mut card, "OPENING", false).unwrap();
        assert_eq!(
            card.extra["firstTurnCard"], false,
            "normal play acquisition cannot claim draft opening"
        );
        register_acquired_card(&state, Color::Black, &mut card, "MIDDLE", false).unwrap();
        assert_eq!(
            card.extra["nextTurnPending"], false,
            "unknown campaign phase has no next-turn delay"
        );
        state.extra.shift_remove("campaign");
        let before = card.clone();
        let error =
            register_acquired_card(&state, Color::Black, &mut card, "OPENING", false).unwrap_err();
        assert!(error.to_string().contains("campaign.setup=blackTower"));
        assert_eq!(
            card, before,
            "invalid dynamic definition cannot mutate reservation metadata"
        );
    }

    #[test]
    fn declined_opening_passive_stays_scheduled_for_the_first_move() {
        // Source isFirstMoveAutoCard tests phase and firstTurnCard without
        // checking activation. Checker can fail during a multi-card draft,
        // then become applicable after the first board move clears its cell.
        let mut state = with_acquired("checker");
        state.deck_slots.white[0]
            .extra
            .insert("firstTurnCard".into(), json!(true));
        state.board[5][0] = state.board[6][0].take();
        let before = state.clone();
        assert!(!apply_acquisition(&mut state, Color::White, 0).unwrap());
        assert_eq!(state, before);
        assert_eq!(first_move_preflight(&state, Color::White).unwrap(), vec![0]);
        state.board[6][0] = state.board[5][0].take();
        let card = state.deck_slots.white[0].clone();
        assert!(apply_virtual_effect(&mut state, Color::White, &card).unwrap());
        assert!(!state.deck_slots.white[0].used);
        assert_ne!(
            state.deck_slots.white[0].extra.get("passiveApplied"),
            Some(&json!(true))
        );
        assert_eq!(state.extra["logs"], before.extra["logs"]);
    }
    #[test]
    fn declined_passive_resets_source_origins_before_later_infiltration() {
        // Source septemberBeginBoardAction updates its WeakMap even when
        // Checker declines. Rolling that execution context back would make
        // the later move settlement grant a false infiltration effect.
        let mut state = with_acquired("checker");
        let pawn = state.at(Square { row: 6, col: 0 }).unwrap().clone();
        let white_king = state.at(Square { row: 7, col: 4 }).unwrap().clone();
        let black_king = state.at(Square { row: 0, col: 4 }).unwrap().clone();
        let blocker = state.at(Square { row: 0, col: 0 }).unwrap().clone();
        state.board = vec![vec![None; 8]; 8];
        state.board[7][7] = Some(white_king);
        state.board[0][7] = Some(black_king);
        state.board[5][0] = Some(blocker);
        state.board[6][0] = Some(pawn);
        state
            .extra
            .insert("infiltration".into(), json!({"white":true,"black":false}));
        crate::v7_card_context::begin_board_action(&mut state);
        let mut pawn = state.board[6][0].take().unwrap();
        let pawn_id = pawn.id.clone();
        pawn.moved = true;
        state.board[0][0] = Some(pawn);
        let snapshot = serde_json::to_value(&state).unwrap();
        let card = state.deck_slots.white[0].clone();
        assert!(!apply_virtual_effect(&mut state, Color::White, &card).unwrap());
        assert_eq!(serde_json::to_value(&state).unwrap(), snapshot);
        assert_eq!(
            state
                .board_action_origins
                .as_ref()
                .unwrap()
                .iter()
                .find(|origin| origin.id == pawn_id)
                .unwrap()
                .cells,
            vec![Square { row: 0, col: 0 }]
        );
        crate::v7_card_context::resolve_board_infiltration(&mut state).unwrap();
        assert!(
            !state
                .at(Square { row: 0, col: 0 })
                .unwrap()
                .flag("submerged")
        );
    }
    #[test]
    fn white_box_acquisition_shares_a_virtual_box_with_clone_owner() {
        // e5ed84fc 원문 faithful-init-v1, normal seed 37의 동일한 합성 입력.
        // reports/v7-passive-differential/source-passive-goldens-faithful.json
        // (bytes SHA256 d2fb18fa...ac29447)의 white-box-clone-box-test.rawPosition.
        // 구 source 자료와 전체 state/RNG/history는 같고 catalog identity만 바뀐다.
        let mut state = with_acquired("white-box");
        state.deck_slots.white[0].instance_id = "white-box-clone-box-test".into();
        let mut source = crate::card_registry::definition_for(RULES_VERSION_V7, "clone")
            .unwrap()
            .source_definition
            .clone();
        source["instanceId"] = json!("clone-clone-box-test");
        source["deckCard"] = json!(true);
        source["nextTurnPending"] = json!(false);
        source["slot"] = json!(0);
        source["used"] = json!(true);
        source["passiveApplied"] = json!(true);
        state.deck_slots.black[0] = serde_json::from_value(source).unwrap();
        state.set_flag("clonePassive", Color::Black, true);
        assert!(
            crate::card_effects::apply_v7_white_box_acquisition(&mut state, Color::White, 0)
                .unwrap()
        );
        assert_eq!(
            state.extra["clonedPassiveCards"]["black"],
            json!(["white:white-box-clone-box-test"])
        );
        assert_eq!(
            state.deck_slots.white[0].extra["boxRevealedCardId"],
            "elephant-escape"
        );
        assert_eq!((state.rng.cursor, state.rng.state), (250, 800394263));
        assert_eq!(
            crate::v7_host::V7HostPosition::from_state(state)
                .unwrap()
                .position_id(),
            "94cf6271bb494657f69f26368e96aa362338c343abc31b424e933fa554be8b1e"
        );
    }
    #[test]
    fn passive_effect_removes_hole_occupant_before_post_card_hazards() {
        // Blue Jeans erases the a8 rook before applyPostCardBoardHazards
        // inspects that square. Its removal must not become a black-hole
        // capture, even when the source rule is active during acquisition.
        let mut state = with_acquired("blue-jeans");
        state
            .extra
            .insert("blackHole".into(), json!([{"row":0,"col":0}]));
        let before = state.clone();
        assert!(apply_acquisition(&mut state, Color::White, 0).unwrap());
        assert!(state.at(Square { row: 0, col: 0 }).is_none());
        assert!(state.deck_slots.white[0].used);
        assert_eq!(state.captures, before.captures);
        assert_eq!(state.rng, before.rng);
        assert_eq!(state.history, before.history);
        assert_eq!(
            state.extra["logs"].as_array().unwrap().len(),
            before.extra["logs"].as_array().unwrap().len() + 1
        );
    }

    #[test]
    fn horse_riding_resolves_knight_campaign_after_erasure() {
        // e5ed84fc 원문, seed 37, horse-riding-passive-test와 knightJourney.
        // faithful-init-v1의 새 catalog identity를 포함하는
        // reports/v7-passive-differential/source-opening-interactions-faithful.json의
        // horse-riding-knight-campaign.rawPosition이 근거다. 종료 이후에도
        // riding flag와 로그를 쓰며, queued terminal replay 정산 전을 비교한다.
        // 정산 후 positionId는 5f4e48dd...로 달라 별도 callback matrix가 검사한다.
        let mut state = with_acquired("horse-riding");
        state.extra.insert(
            "campaign".into(),
            json!({"setup":"knightJourney","playerColor":"white"}),
        );
        assert!(apply_acquisition(&mut state, Color::White, 0).unwrap());
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner.as_deref(), Some("black"));
        assert_eq!(state.extra["kingKnight"]["white"], json!(true));
        assert_eq!(
            crate::v7_host::V7HostPosition::from_state(state)
                .unwrap()
                .position_id(),
            "276a3d06d7e6508e186bd288f3aafd36ff846336808a632eb7e254e170720fb2"
        );
    }

    #[test]
    fn blue_jeans_checks_campaign_without_premature_knight_victory() {
        // 위와 같은 faithful-init-v1/source/seed의 blue-jeans-knight-campaign.rawPosition
        // 근거. 기물 제거 이후 나이트가 남아 draft를 유지하며 raw/settled ID가 같다.
        let mut state = with_acquired("blue-jeans");
        state.extra.insert(
            "campaign".into(),
            json!({"setup":"knightJourney","playerColor":"white"}),
        );
        assert!(apply_acquisition(&mut state, Color::White, 0).unwrap());
        assert_eq!(state.mode, "draft");
        assert_eq!(
            crate::v7_host::V7HostPosition::from_state(state)
                .unwrap()
                .position_id(),
            "1b9a4cd3de49c2118127851917c4e3f53bad2646946f110a2e0f8ce7b8e5356f"
        );
    }

    #[test]
    fn conversion_marks_every_knight_origin_without_consuming_rng() {
        let mut state = with_acquired("conversion");
        let before = state.clone();
        assert!(apply_acquisition(&mut state, Color::White, 0).unwrap());
        for col in [1, 6] {
            let piece = state.at(Square { row: 7, col }).unwrap();
            assert_eq!(piece.kind, "bishop");
            assert!(piece.moved);
            assert_eq!(
                piece.extra["origin"],
                json!(format!("{}1", char::from(b'a' + col)))
            );
            assert_eq!(piece.extra["freshNoCaptureUntil"], json!(1));
        }
        assert_eq!(state.rng, before.rng);
        assert_eq!(state.history, before.history);
    }

    #[test]
    fn checker_reports_malformed_placement_reservation_without_consuming_card() {
        let mut state = with_acquired("checker");
        state
            .extra
            .insert("pendingScarecrows".into(), json!("invalid reservation"));
        let before = state.clone();
        assert!(matches!(
            apply_acquisition(&mut state, Color::White, 0),
            Err(EngineError::InvalidState(message)) if message == "pendingScarecrows must be an array"
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn false_start_moves_two_rows_without_marking_pieces_moved() {
        let mut state = with_acquired("false-start");
        let before = state.clone();
        assert_eq!(state.at(Square { row: 7, col: 0 }).unwrap().kind, "rook");
        assert_eq!(state.at(Square { row: 6, col: 0 }).unwrap().kind, "pawn");
        assert!(apply_acquisition(&mut state, Color::White, 0).unwrap());
        assert_eq!(state.extra["falseStart"]["white"], json!(true));
        assert!(state.at(Square { row: 7, col: 0 }).is_none());
        assert!(state.at(Square { row: 6, col: 0 }).is_none());
        assert_eq!(state.at(Square { row: 5, col: 0 }).unwrap().kind, "rook");
        assert_eq!(state.at(Square { row: 4, col: 0 }).unwrap().kind, "pawn");
        assert!(!state.at(Square { row: 5, col: 0 }).unwrap().moved);
        assert!(!state.at(Square { row: 4, col: 0 }).unwrap().moved);
        assert_eq!(state.rng, before.rng);
        assert_eq!(state.history, before.history);
    }

    #[test]
    fn queens_gambit_automatically_fires_after_first_move_with_source_rng_and_replay() {
        // e5ed84fc 원문 faithful-init-v1, chaos seed 19의 white bundle1/black bundle2.
        // source-passive-goldens-faithful.cjs로 생성한 reports/v7-passive-differential/
        // source-passive-goldens-faithful.json (bytes SHA256 d2fb18fa...ac29447)의
        // chaos-seed19-qg-two-picks-first-move.before/after가 근거다.
        // 구 source와 전체 state/RNG/history가 같아 이들 golden은 보존하며,
        // 새 실행 profile catalog identity를 포함하는 Position ID만 교정한다.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: "chaos".into(),
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        let white_pick = crate::draft::legal_actions(&state).unwrap().remove(1);
        crate::draft::apply_pick(&mut state, &white_pick).unwrap();
        crate::replay::canonicalize_position_frames(&mut state).unwrap();
        let black_pick = crate::draft::legal_actions(&state).unwrap().remove(2);
        crate::draft::apply_pick(&mut state, &black_pick).unwrap();
        crate::replay::canonicalize_position_frames(&mut state).unwrap();
        assert_eq!(state.deck_slots.white[0].id, "queens-gambit");
        assert_eq!(
            source_state_digest(&state),
            "3b8fd9fae0404a6032c8f5daa7bbdecc03cac93730355f13ebb6d97a2f290793"
        );
        assert_eq!(
            (state.rng.cursor, state.rng.state, state.history.len()),
            (400, 185085603, 0)
        );
        assert_eq!(
            crate::v7_host::V7HostPosition::from_state(state.clone())
                .unwrap()
                .position_id(),
            "2c29b52d80aa35b6a4fb6b9e60a24874bae358cb728487aca93719e780ef499c"
        );
        // The source auto-target pass consumes one draw even though only the
        // d1 queen qualifies. Keep this check independent of the public move
        // admission gate, which must separately admit a2-a3 before the full
        // transition can run.
        let mut target_probe = state.clone();
        let opening_card = target_probe.deck_slots.white[0].clone();
        assert_eq!(
            first_move_target(&mut target_probe, &opening_card).unwrap(),
            Some(json!({"row":7,"col":3}))
        );
        assert_eq!(
            (target_probe.rng.cursor, target_probe.rng.state),
            (401, 2623095718)
        );
        assert_eq!(target_probe.history, state.history);
        assert_eq!(target_probe.board, state.board);

        let action = Action::movement(
            Color::White,
            Square { row: 6, col: 0 },
            MoveTarget::at(Square { row: 5, col: 0 }),
        );
        crate::transition::apply(&mut state, &action).unwrap();
        assert_eq!(
            source_state_digest(&state),
            "6dc19204cbf8232650254c06c2a44f74a818c3af16bc371233268288c4a7c61f"
        );
        assert_eq!((state.rng.cursor, state.rng.state), (404, 314299015));
        assert_eq!(state.history.len(), 1);
        assert_eq!(
            jcs_digest(&state.history),
            "827d114126bbd72a39f142ba0169381bb67c9e7c504a9f75e5a0c8da89a3dbc5"
        );
        assert_eq!(
            crate::v7_host::V7HostPosition::from_state(state.clone())
                .unwrap()
                .position_id(),
            "4198d1c72252d7960cb7240a662e203b5e31f5419bffbdb3988988d602cd2370"
        );
        assert_eq!(
            state.extra["queensGambitFiles"],
            json!({"white":{"queenCol":3,"randomCol":1},"black":null})
        );
        assert!(state.deck_slots.white[0].used);
    }

    #[test]
    #[ignore = "external source receipt is generated outside Git"]
    fn temporary_source_passive_matrix() {
        let path = std::env::var("ACCELERATE_PASSIVE_SOURCE_RECEIPT").unwrap();
        let receipt: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let cases =
            crate::tests::source_callback_fixture::validate_source_receipt(&receipt).unwrap();
        if receipt.get("scope").is_some() {
            assert_eq!(
                receipt["scope"],
                "source applyPassiveCardOnDraft interactions; not full move parity"
            );
        }
        eprintln!(
            "PASSIVE comparison scope: rawGameState callbacks, {} cases, boundary={}; public host admission and full moves are not exercised",
            cases.len(),
            receipt["executionBoundary"]
                .as_str()
                .unwrap_or("live-source-callback")
        );
        let mut mismatches = Vec::new();
        for (index, case) in cases.iter().enumerate() {
            let label = case["name"]
                .as_str()
                .or_else(|| case["id"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("case[{index}]"));
            crate::tests::source_callback_fixture::collect_case_diagnostics(
                &label,
                &mut mismatches,
                |mismatches| compare_source_passive_case(case, &label, mismatches),
            );
        }
        for mismatch in &mismatches {
            eprintln!("{mismatch}");
        }
        assert!(
            mismatches.is_empty(),
            "{} source PASSIVE mismatches",
            mismatches.len()
        );
    }

    fn compare_source_passive_case(
        case: &Value,
        label: &str,
        mismatches: &mut Vec<String>,
    ) -> Result<()> {
        use crate::tests::source_callback_fixture::{
            compare_callback_envelope, source_callback_state,
        };
        if let Some(error) = case.get("error") {
            return Err(EngineError::InvalidState(format!(
                "source callback error {error}"
            )));
        }
        let id = case["id"]
            .as_str()
            .ok_or_else(|| EngineError::InvalidState("receipt case.id must be text".into()))?;
        let mut state = if let Some(before) = case.get("before") {
            // These synthetic callback states may intentionally precede
            // topology cleanup. Only the private test fixture importer accepts
            // them; public V7HostPosition admission remains strict.
            source_callback_state(before)?
        } else {
            with_acquired(id)
        };
        let applied = apply_acquisition(&mut state, Color::White, 0)?;
        let expected_applied = case["result"]["ok"].as_bool().ok_or_else(|| {
            EngineError::InvalidState("receipt callback result.ok must be boolean".into())
        })?;
        if applied != expected_applied {
            mismatches.push(format!(
                "{label}: acquisition result source={expected_applied}, native={applied}"
            ));
        }
        let source_position = case.get("rawPosition").unwrap_or(&case["position"]);
        compare_callback_envelope(
            &state,
            source_position,
            &format!("{label}/callback"),
            mismatches,
        )?;
        if case.get("rawPosition").is_some() {
            crate::replay::settle(&mut state)?;
            compare_callback_envelope(
                &state,
                &case["position"],
                &format!("{label}/settled"),
                mismatches,
            )?;
        }
        Ok(())
    }
}
