//! 동결 클라이언트의 AI 카드 후보 정책. 공개 UI/completeCardTargets와 구분한다.
//! main:83145-83220,83773-83904,94725-94752의 순서·정렬·제한을 보존한다.
//! 후보 준비의 임시 turn/targeting은 복구하지만 RNG 소비는 호출자가 소유한
//! 작업 상태에 남긴다. 효과 실행·finishCard·사용 처리·공개 이벤트는 별도 책임이다.

use crate::{
    Action, CardSlot, Color, EngineError, GameState, Piece, RULES_VERSION_V7, Result, Square,
};
use serde_json::{Value, json};
use std::{cmp::Ordering, collections::BTreeSet};

const MAX_AI_CARD_CANDIDATES: usize = 100000;
const NECROMANCY_EXCLUDED: &[&str] = &[
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
    "pawn",
    "squire",
];

fn require_v7(state: &GameState) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "source AI card candidates require frozen v7".into(),
        ));
    }
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::InvalidState(
            "source AI card candidates require the current 8x8 board contract".into(),
        ));
    }
    Ok(())
}

fn require_budget(count: usize, label: &str) -> Result<()> {
    if count > MAX_AI_CARD_CANDIDATES {
        return Err(EngineError::UnsupportedFeature(format!(
            "source AI {label} exceeded its candidate limit ({MAX_AI_CARD_CANDIDATES}); no truncated set was returned"
        )));
    }
    Ok(())
}

/// collectValidAiActions({includeCards:true,exhaustiveCards:true,cardsOnly:true}).
/// king_danger_only은 main85606의 별도 cardFilter를 적용한다. devCard를
/// 제외하는 isCardPlayableNow와 달리 이 stream은 devCard 자체를 제외하지 않는다.
pub(crate) fn collect_v7_ai_card_actions(
    state: &mut GameState,
    color: Color,
    king_danger_only: bool,
) -> Result<Vec<Action>> {
    require_v7(state)?;
    let previous_turn = state.turn;
    state.turn = color;
    let result = (|| {
        if !crate::movement::v7_card_action_window_open(state)? {
            return Ok(Vec::new());
        }
        let cards = state.deck_slots.get(color).clone();
        let mut actions = Vec::new();
        for card in cards {
            if !crate::card_registry::source_candidate_available(state, &card)? {
                continue;
            }
            let definition = crate::card_registry::validate_instance(state, &card)?;
            if king_danger_only
                && (card.extra.get("phase").and_then(Value::as_str) == Some("RULE")
                    || crate::draft::is_passive_definition_for_ruleset(
                        RULES_VERSION_V7,
                        &definition.source_definition,
                    )?
                    || matches!(card.id.as_str(), "shotgun-king" | "summon-colossus"))
            {
                continue;
            }
            if card.effect == "bloodCard" {
                // bloodMoonState normalisation belongs to the mutable prepare.
                actions.extend(crate::v7_campaign::collect_blood_card_actions(
                    state, &card, color,
                )?);
            } else {
                for target in collect_v7_ai_card_targets(state, &card, color, true)? {
                    actions.push(Action::card(color, &card, target));
                    require_budget(actions.len(), "card stream")?;
                }
            }
            require_budget(actions.len(), "card stream")?;
        }
        Ok(actions)
    })();
    state.turn = previous_turn;
    result
}

/// None은 source undefined target이다. UI용 완성 tuple이나 조합 cursor로
/// 바꾸지 않는다. exhaustive는 우선순위 5종의 cap만 해제한다.
pub(crate) fn collect_v7_ai_card_targets(
    state: &mut GameState,
    card: &CardSlot,
    color: Color,
    exhaustive: bool,
) -> Result<Vec<Option<Value>>> {
    require_v7(state)?;
    crate::card_registry::validate_instance(state, card)?;
    let previous_turn = state.turn;
    let previous_targeting = state.extra.insert("targeting".into(), Value::Null);
    state.turn = color;
    let result = collect_targets_inner(state, card, color, exhaustive);
    state.turn = previous_turn;
    if let Some(value) = previous_targeting {
        state.extra.insert("targeting".into(), value);
    } else {
        state.extra.shift_remove("targeting");
    }
    let targets = result?;
    require_budget(targets.len(), "card target family")?;
    Ok(targets)
}

/// Source no-action 가용성. targeted는 후보 존재만 확인하고 효과를 시험하지
/// 않는다. untargeted는 raw applyCard를 복제 상태에서 시험하며 RNG만 회수한다.
pub(crate) fn is_v7_ai_card_playable(
    state: &mut GameState,
    card: &CardSlot,
    color: Color,
) -> Result<bool> {
    require_v7(state)?;
    let previous_turn = state.turn;
    state.turn = color;
    let result = (|| {
        if !crate::card_registry::source_candidate_available(state, card)?
            || crate::observation::truth(card.extra.get("devCard"))
        {
            return Ok(false);
        }
        // Frozen adapter restore pins playMode/localPlayMode='local'
        // (game-adapter.js:309,338). isAiSingleMode is false in this host;
        // source 94729's offline single-AI conditional guard does not run.
        if card.effect == "ruleTicket" {
            return Ok(
                !crate::card_effects::source_card_nonlazy_candidates(state, card)?.is_empty(),
            );
        }
        if crate::observation::truth(card.extra.get("target")) {
            return Ok(!collect_v7_ai_card_targets(state, card, color, false)?.is_empty());
        }
        can_resolve_untargeted_card(state, card, color)
    })();
    state.turn = previous_turn;
    result
}

pub(crate) fn can_resolve_untargeted_card(
    state: &mut GameState,
    card: &CardSlot,
    color: Color,
) -> Result<bool> {
    require_v7(state)?;
    let mut probe = state.clone();
    probe.turn = color;
    probe.extra.insert("selected".into(), Value::Null);
    probe.extra.insert("legalMoves".into(), json!([]));
    probe.extra.insert("targeting".into(), Value::Null);
    let simulated_card = probe
        .deck_slots
        .white
        .iter()
        .chain(&probe.deck_slots.black)
        .find(|candidate| candidate.instance_id == card.instance_id)
        .cloned()
        .unwrap_or_else(|| card.clone());
    let action = Action::card(color, &simulated_card, None);
    let result = crate::v7_card_context::with_ai_simulation(&mut probe, |working| {
        crate::transition::apply_card_raw(working, &simulated_card, &action)
    });
    // Math.random is outside the source state clone. False probes consume RNG
    // too; caller-owned prepare must not restore those draws with its board.
    state.rng = probe.rng;
    match result {
        Ok(_) => Ok(true),
        Err(EngineError::IllegalAction) => Ok(false),
        Err(error) => Err(error),
    }
}

fn raw_squares(state: &GameState, card: &CardSlot) -> Result<Vec<Square>> {
    crate::card_effects::target_squares(state, card)?.ok_or_else(|| {
        EngineError::UnsupportedFeature(format!(
            "source AI getTargetSquares predicate for {}",
            card.id
        ))
    })
}

fn unique_squares(state: &GameState, squares: Vec<Square>) -> Vec<Square> {
    let mut seen = BTreeSet::new();
    squares
        .into_iter()
        .filter(|square| {
            seen.insert(
                state
                    .at(*square)
                    .filter(|piece| !piece.id.is_empty())
                    .map_or_else(
                        || format!("square:{},{}", square.row, square.col),
                        |piece| format!("piece:{}", piece.id),
                    ),
            )
        })
        .collect()
}

fn sorted_by_score<T>(entries: Vec<T>, mut score: impl FnMut(&T) -> Result<f64>) -> Result<Vec<T>> {
    let mut scored = Vec::with_capacity(entries.len());
    for entry in entries {
        let value = score(&entry)?;
        if !value.is_finite() {
            return Err(EngineError::InvalidState(
                "source AI card target score is non-finite".into(),
            ));
        }
        scored.push((entry, value));
    }
    // Rust stable sort preserves row/deck/tuple order for the source JS ties.
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    Ok(scored.into_iter().map(|(entry, _)| entry).collect())
}

fn selections(squares: Vec<Square>, minimum: usize) -> Vec<Option<Value>> {
    if squares.len() < minimum {
        Vec::new()
    } else {
        vec![Some(json!({"selections":squares}))]
    }
}

fn collect_targets_inner(
    state: &mut GameState,
    card: &CardSlot,
    color: Color,
    exhaustive: bool,
) -> Result<Vec<Option<Value>>> {
    if card.effect == "collapse" && !can_ai_use_collapse_safely(state, color)? {
        return Ok(Vec::new());
    }
    if matches!(card.id.as_str(), "brainwash" | "taboo") {
        return compound_targets(state, card);
    }
    if card.effect == "ruleTicket" {
        let mut candidates = crate::card_effects::source_card_nonlazy_candidates(state, card)?;
        candidates.truncate(12);
        return candidates.into_iter().map(|candidate| {
            let id = candidate.target.as_ref().and_then(|target| target.get("ruleId"))
                .and_then(Value::as_str).ok_or_else(|| EngineError::InvalidState("source AI rule ticket pool lost its rule id".into()))?;
            let rule = crate::card_registry::definition_for(RULES_VERSION_V7, id)?;
            Ok(Some(json!({"ruleId":id,"ruleEffect":rule.effect,
                "ruleStars":crate::card_effects::js_number(rule.source_definition.get("stars"),0).unwrap_or(0.0)})))
        }).collect();
    }
    if matches!(card.effect.as_str(), "switcheroo" | "iceSheet")
        || !crate::observation::truth(card.extra.get("target"))
    {
        return Ok(if can_resolve_untargeted_card(state, card, color)? {
            vec![None]
        } else {
            Vec::new()
        });
    }
    if card.id == "grappler" && crate::v7_queued_effects::uses_september26_rebalance(state)? {
        return grappler_targets(state, color);
    }
    match card.effect.as_str() {
        "windmill" | "feudalContract" | "hook" => {
            return compound_piece_targets(state, card, color);
        }
        "portalGun" => {
            let mut preferred = sorted_by_score(raw_squares(state, card)?, |square| {
                Ok(center_score(state, *square))
            })?;
            preferred.truncate(8);
            let mut targets = Vec::new();
            for first in 0..preferred.len() {
                for second in first + 1..preferred.len() {
                    targets.push(Some(
                        json!({"selections":[preferred[first],preferred[second]]}),
                    ));
                }
            }
            targets.truncate(16);
            return Ok(targets);
        }
        "twins" | "chain" => {
            let pairs = crate::card_effects::source_card_nonlazy_candidates(state, card)?;
            let mut sorted = sorted_by_score(pairs, |action| {
                let cells = action
                    .target
                    .as_ref()
                    .and_then(|target| target.get("selections"))
                    .and_then(Value::as_array)
                    .filter(|cells| cells.len() == 2)
                    .ok_or_else(|| {
                        EngineError::InvalidState(
                            "source AI pair candidate lost its two coordinates".into(),
                        )
                    })?;
                let mut value = 0.0;
                for cell in cells {
                    let square: Square =
                        serde_json::from_value(cell.clone()).map_err(EngineError::serialization)?;
                    value += piece_value(state, state.at(square))?;
                }
                Ok(value)
            })?;
            sorted.truncate(12);
            return Ok(sorted.into_iter().map(|action| action.target).collect());
        }
        "hypocrisy" => {
            let mut selected = sorted_by_score(raw_squares(state, card)?, |square| {
                Ok(center_score(state, *square))
            })?;
            selected.truncate(4);
            return Ok(selections(selected, 4));
        }
        "freeMove" => return free_move_targets(state, color),
        "chameleonMutation" => {
            let squares = unique_squares(state, raw_squares(state, card)?);
            let mut selected =
                sorted_by_score(squares, |square| piece_value(state, state.at(*square)))?;
            selected.truncate(3);
            return Ok(selections(selected, 1));
        }
        _ => {}
    }
    if matches!(
        card.effect.as_str(),
        "scarecrow" | "necromancy" | "othello" | "suicideBomber" | "witchTrial"
    ) {
        let squares = unique_squares(state, raw_squares(state, card)?);
        let mut sorted = sorted_by_score(squares, |square| {
            card_target_score(state, card, *square, color)
        })?;
        if !exhaustive {
            sorted.truncate(match card.effect.as_str() {
                "scarecrow" => 10,
                "othello" => 8,
                _ => 6,
            });
        }
        return Ok(sorted
            .into_iter()
            .map(|square| Some(json!(square)))
            .collect());
    }
    if let Some((maximum, minimum)) = match card.effect.as_str() {
        "emergencyEvacuation" => Some((3, 1)),
        "panic" => Some((2, 2)),
        "spy" => Some((2, 1)),
        "pawnStorm" => Some((4, 1)),
        _ => None,
    } {
        let mut selected = unique_squares(state, raw_squares(state, card)?);
        selected.truncate(maximum);
        return Ok(selections(selected, minimum));
    }
    if card.effect == "barricade" {
        // collectAiCardTargets clears targeting, but keeps barricadePreview.
        // The AI raw target still omits direction, unlike the complete UI tuple.
        let direction = if state
            .extra
            .get("barricadePreview")
            .and_then(|preview| preview.get("direction"))
            .and_then(Value::as_str)
            == Some("vertical")
        {
            "vertical"
        } else {
            "horizontal"
        };
        return Ok(
            crate::card_effects::source_card_nonlazy_candidates(state, card)?
                .into_iter()
                .filter_map(|action| action.target)
                .filter(|target| target.get("direction").and_then(Value::as_str) == Some(direction))
                .map(|target| Some(json!({"row":target["row"],"col":target["col"]})))
                .collect(),
        );
    }
    Ok(raw_squares(state, card)?
        .into_iter()
        .map(|square| Some(json!(square)))
        .collect())
}

fn compound_targets(state: &GameState, card: &CardSlot) -> Result<Vec<Option<Value>>> {
    let actions = crate::card_effects::source_card_nonlazy_candidates(state, card)?;
    require_budget(actions.len(), "compound card family")?;
    Ok(actions.into_iter().map(|action| action.target).collect())
}

fn piece_entries(
    state: &GameState,
    mut predicate: impl FnMut(&Piece) -> Result<bool>,
) -> Result<Vec<(Square, Piece)>> {
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(piece) = state.at(square) else {
                continue;
            };
            if seen.contains(&piece.id) || !predicate(piece)? {
                continue;
            }
            seen.insert(piece.id.clone());
            entries.push((square, piece.clone()));
        }
    }
    Ok(entries)
}

fn grappler_targets(state: &GameState, color: Color) -> Result<Vec<Option<Value>>> {
    let queens = piece_entries(state, |piece| {
        Ok(piece.color == color
            && piece.kind == "queen"
            && !crate::v7_board_hazards::source_royal_identity(state, piece)?)
    })?;
    let minors = piece_entries(state, |piece| {
        Ok(piece.color == color
            && [
                "knight",
                "bishop",
                "camel",
                "clockwork",
                "parrot",
                "wizard",
                "recruiter",
                "trickster",
            ]
            .contains(&piece.kind.as_str())
            && !crate::v7_board_hazards::source_royal_identity(state, piece)?)
    })?;
    Ok(queens
        .iter()
        .flat_map(|(queen, _)| {
            minors
                .iter()
                .map(move |(minor, _)| Some(json!({"row":queen.row,"col":queen.col,"minor":minor})))
        })
        .collect())
}

fn compound_piece_targets(
    state: &GameState,
    card: &CardSlot,
    color: Color,
) -> Result<Vec<Option<Value>>> {
    let mut targets = Vec::new();
    if card.effect == "feudalContract" {
        let mut pawns = Vec::new();
        let mut guardians = Vec::new();
        // This source branch uses forEachSquare directly, without seen IDs.
        for row in 0..8 {
            for col in 0..8 {
                let square = Square { row, col };
                let Some(piece) = state.at(square).filter(|piece| piece.color == color) else {
                    continue;
                };
                if matches!(piece.kind.as_str(), "pawn" | "fanatic")
                    && !crate::observation::truth(piece.extra.get("explosive"))
                    && !crate::observation::truth(piece.extra.get("feudalContractId"))
                {
                    pawns.push(square);
                } else if ![
                    "pawn",
                    "fanatic",
                    "wall",
                    "colossus",
                    "bigRook",
                    "bigBishop",
                ]
                .contains(&piece.kind.as_str())
                {
                    guardians.push(square);
                }
            }
        }
        for pawn in pawns {
            for guardian in &guardians {
                targets.push(Some(
                    json!({"row":guardian.row,"col":guardian.col,"pawn":pawn}),
                ));
            }
        }
    } else {
        let first_kind = if card.effect == "windmill" {
            "bishop"
        } else {
            "queen"
        };
        let first = piece_entries(state, |piece| {
            Ok(piece.color == color
                && piece.kind == first_kind
                && (card.effect != "hook"
                    || piece.extra.get("regencyHeir") != Some(&Value::Bool(true))))
        })?;
        let rooks = piece_entries(state, |piece| {
            Ok(piece.color == color && piece.kind == "rook")
        })?;
        for (origin, _) in first {
            for (rook, _) in &rooks {
                targets.push(Some(if card.effect == "windmill" {
                    json!({"row":rook.row,"col":rook.col,"bishop":origin})
                } else {
                    json!({"row":origin.row,"col":origin.col,"rook":rook})
                }));
            }
        }
    }
    require_budget(targets.len(), "compound piece family")?;
    Ok(targets)
}

fn free_move_targets(state: &GameState, color: Color) -> Result<Vec<Option<Value>>> {
    let mut plans = Vec::new();
    for (square, piece) in piece_entries(state, |piece| {
        Ok(piece.color == color
            && !["wall", "football", "blackHole", "coffin"].contains(&piece.kind.as_str()))
    })? {
        let from = crate::transition::normalize_piece_square(state, square)?;
        for target in crate::movement::v7_free_move_declaration_targets(state, &piece, from)? {
            let to = target.square();
            let capture = if state
                .at(to)
                .is_some_and(|victim| victim.color == color.opponent())
            {
                piece_value(state, state.at(to))? * 12.0
            } else {
                0.0
            };
            plans.push((
                piece.id.clone(),
                from,
                to,
                capture + center_score(state, to) * 0.8,
            ));
            require_budget(plans.len(), "free move declaration family")?;
        }
    }
    let sorted = sorted_by_score(plans, |plan| Ok(plan.3))?;
    let mut seen = BTreeSet::new();
    let selected: Vec<_> = sorted
        .into_iter()
        .filter(|plan| seen.insert(plan.0.clone()))
        .take(3)
        .map(|(_, from, to, _)| json!({"from":from,"to":to}))
        .collect();
    Ok(if selected.is_empty() {
        Vec::new()
    } else {
        vec![Some(json!({"selections":selected}))]
    })
}

fn center_score(state: &GameState, square: Square) -> f64 {
    let row = (state.board.len() - 1) as f64 / 2.0;
    let col = (state.board[0].len() - 1) as f64 / 2.0;
    (1.0 - ((f64::from(square.row) - row).abs() + (f64::from(square.col) - col).abs())
        / (row + col).max(1.0))
    .max(0.0)
}

fn pawn_progress(state: &GameState, row: u8, color: Color) -> f64 {
    let denominator = (state.board.len() - 1).max(1) as f64;
    if color == Color::White {
        (denominator - f64::from(row)) / denominator
    } else {
        f64::from(row) / denominator
    }
}

fn kind_value(kind: &str) -> f64 {
    match kind {
        "pawn" | "coffin" | "babyBear" => 1.0,
        "squire" | "fanatic" => 1.2,
        "standardBearer" => 2.2,
        "guard" | "log" => 1.5,
        "checker" => 2.4,
        "checkerKing" | "alfil" | "ferz" | "missionary" | "idol" | "lobster" => 2.0,
        "camel" => 2.6,
        "protestant" => 3.2,
        "recruiter" | "man" | "cannon" | "grasshopper" | "campfire" | "eagle" => 4.0,
        "rook" | "herald" | "jester" => 5.0,
        "bigRook" | "bigBishop" => 8.0,
        "hedgehog" | "queen" => 9.0,
        "princess" | "dragon" | "reaper" | "pegasus" => 6.0,
        "assassin" => 5.5,
        "knightmaster" | "windmill" => 4.5,
        "cardinal" | "primeMinister" => 7.0,
        "amazon" => 14.0,
        "wizard" => 9.5,
        "hook" => 10.0,
        "colossus" | "shotgunKing" => 12.0,
        "timeTraveler" | "vampireLord" | "vip" | "king" | "royalKnight" | "darkWizard"
        | "merchant" => 100.0,
        "bear" => 11.0,
        "crown" => 9.4,
        _ => 3.0,
    }
}

fn piece_value(state: &GameState, piece: Option<&Piece>) -> Result<f64> {
    let Some(piece) = piece else {
        return Ok(0.0);
    };
    if piece.kind == "vip"
        || piece.kind == "merchant"
        || crate::v7_board_hazards::source_royal_identity(state, piece)?
    {
        return Ok(100.0);
    }
    Ok(kind_value(&piece.kind))
}

fn rule_royal(state: &GameState, piece: &Piece) -> Result<bool> {
    let Some(color) = piece.color.owner() else {
        return Ok(false);
    };
    if state.democracy_protects_royal(piece) {
        return Ok(false);
    }
    if piece.kind == "vip" {
        return Ok(true);
    }
    let native = crate::v7_board_hazards::source_royal_king(state, piece)?;
    if !state.flag("regency", color) {
        return Ok(native);
    }
    if crate::observation::truth(piece.extra.get("regencyHeir")) {
        return Ok(state.flag("kingDead", color));
    }
    Ok(native
        && !state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|candidate| candidate.color == color && candidate.kind == "queen"))
}

fn ai_critical(state: &GameState, piece: &Piece) -> Result<bool> {
    Ok(matches!(
        piece.kind.as_str(),
        "merchant" | "timeTraveler" | "vampireLord"
    ) || rule_royal(state, piece)?)
}

fn attackers(state: &GameState, target: Square, color: Color) -> Result<Vec<f64>> {
    let mut values = Vec::new();
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(piece) = state
                .at(square)
                .filter(|piece| piece.color == color && piece.kind != "wall")
            else {
                continue;
            };
            let key = if piece.id.is_empty() {
                format!("{row},{col}")
            } else {
                piece.id.clone()
            };
            if !seen.insert(key) {
                continue;
            }
            let from = crate::transition::normalize_piece_square(state, square)?;
            if crate::v7_threat::piece_attacks_square_v7(state, piece, from, target)? {
                values.push(piece_value(state, Some(piece))?);
            }
        }
    }
    Ok(values)
}

fn card_target_score(
    state: &GameState,
    card: &CardSlot,
    target: Square,
    color: Color,
) -> Result<f64> {
    match card.effect.as_str() {
        "necromancy" => {
            if state
                .at(target)
                .is_none_or(|piece| piece.color != color || piece.kind != "pawn")
            {
                return Ok(-100.0);
            }
            let values: Vec<_> = state
                .captures
                .get(color.opponent())
                .iter()
                .filter_map(|piece| {
                    let kind = if piece.kind == "knight"
                        && crate::observation::truth(state.extra.get("monochromeChess"))
                    {
                        "camel"
                    } else {
                        piece.kind.as_str()
                    };
                    (piece.color == color
                        && !kind.is_empty()
                        && !NECROMANCY_EXCLUDED.contains(&kind))
                    .then(|| kind_value(kind))
                })
                .collect();
            let average = if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f64>() / values.len() as f64
            };
            let best = values.iter().copied().fold(0.0, f64::max);
            Ok(average * 1.2
                + best * 0.35
                + pawn_progress(state, target.row, color) * 0.8
                + center_score(state, target) * 0.4)
        }
        "othello" => Ok(
            if state
                .at(target)
                .is_some_and(|piece| piece.color == color.opponent())
            {
                piece_value(state, state.at(target))? * 7.0 + center_score(state, target) * 0.6
            } else {
                -100.0
            },
        ),
        "scarecrow" => {
            // main:84839 still scores the installation predicate after the
            // September18 candidate predicate changed to occupied own cells.
            // Those candidates legitimately tie at -100; do not rank them
            // with an assumed candidate-valid score.
            if !crate::movement::open_installation(state, target, state.turn)?
                || crate::movement::collapsed(state, target)
            {
                return Ok(-100.0);
            }
            let mut enemy_value = 0.0;
            let mut enemy_count = 0;
            let mut own_count = 0;
            for row in 0..8 {
                for col in 0..8 {
                    let Some(piece) = state
                        .at(Square { row, col })
                        .filter(|piece| piece.kind != "wall" && !piece.is_large())
                    else {
                        continue;
                    };
                    let distance = target.row.abs_diff(row).max(target.col.abs_diff(col));
                    if distance > 2 {
                        continue;
                    }
                    if piece.color == color.opponent() {
                        enemy_count += 1;
                        enemy_value +=
                            piece_value(state, Some(piece))?.min(9.0) / f64::from(distance.max(1));
                    } else if piece.color == color {
                        own_count += 1;
                    }
                }
            }
            Ok(1.2
                + center_score(state, target) * 1.1
                + pawn_progress(state, target.row, color) * 0.5
                + f64::from(enemy_count) * 0.35
                + enemy_value * 0.28
                - f64::from(own_count) * 0.12)
        }
        "suicideBomber" => suicide_bomber_score(state, target, color),
        "witchTrial" => witch_trial_score(state, target, color),
        _ => Ok(0.0),
    }
}

fn suicide_bomber_score(state: &GameState, target: Square, color: Color) -> Result<f64> {
    let Some(piece) = state.at(target).filter(|piece| {
        piece.color == color
            && matches!(piece.kind.as_str(), "pawn" | "fanatic")
            && !crate::observation::truth(piece.extra.get("explosive"))
            && !crate::observation::truth(piece.extra.get("feudalContractId"))
    }) else {
        return Ok(-2000.0);
    };
    let mut balance = 0.0;
    let mut own_royal = false;
    let mut seen = BTreeSet::new();
    for dr in -1..=1 {
        for dc in -1..=1 {
            if dr == 0 && dc == 0 {
                continue;
            }
            let Some(at) = target.offset(dr, dc) else {
                continue;
            };
            let Some(victim) = state.at(at).filter(|victim| {
                !crate::movement::frozen(victim)
                    && !crate::v7_board_hazards::indirect_attack_immune(state, victim)
                    && !matches!(victim.kind.as_str(), "football" | "monster")
            }) else {
                continue;
            };
            if !seen.insert(victim.id.clone()) {
                continue;
            }
            let value = piece_value(state, Some(victim))? * 100.0;
            if victim.color == color {
                if ai_critical(state, victim)? {
                    own_royal = true;
                } else {
                    balance -= value * if value >= 500.0 { 1.35 } else { 1.0 };
                }
            } else if victim.color == color.opponent() {
                balance += if ai_critical(state, victim)? {
                    value.min(1200.0)
                } else {
                    value * if value >= 500.0 { 1.15 } else { 0.85 }
                };
            }
        }
    }
    if own_royal {
        return Ok(-2000.0);
    }
    let capturer = attackers(state, target, color.opponent())?
        .into_iter()
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let home = if color == Color::White {
        state.board.len() as f64 - 2.0
    } else {
        1.0
    };
    let advance = if color == Color::White {
        home - f64::from(target.row)
    } else {
        f64::from(target.row) - home
    };
    let mut score =
        90.0 + advance.clamp(0.0, 6.0) * 20.0 - piece_value(state, Some(piece))? * 100.0 * 0.12;
    if let Some(value) = capturer {
        score += (value * 100.0 * 0.58).min(700.0);
        score += balance * if balance < 0.0 { 1.62 } else { 0.72 };
    } else {
        score -= 80.0;
        score += balance * if balance < 0.0 { 0.8 } else { 0.32 };
    }
    Ok(score.clamp(-50000.0, 2600.0) / 25.0)
}

fn witch_trial_score(state: &GameState, target: Square, color: Color) -> Result<f64> {
    let Some(piece) = state.at(target).filter(|piece| {
        piece.color == color.opponent()
            && !crate::observation::truth(piece.extra.get("witchTrial"))
            && ![
                "merchant",
                "wall",
                "football",
                "colossus",
                "bigRook",
                "bigBishop",
            ]
            .contains(&piece.kind.as_str())
    }) else {
        return Ok(-2000.0);
    };
    if ai_critical(state, piece)? {
        return Ok(-2000.0);
    }
    let disabled = crate::movement::frozen(piece)
        || source_number(
            piece
                .extra
                .get("staked")
                .and_then(|value| value.get("remaining")),
            0,
        )?
        .unwrap_or(0.0)
            > 0.0
        || crate::observation::truth(piece.extra.get("disarmed"))
        || crate::movement::v7_manner_capture_locked(state, piece)
        || crate::v7_capture_reactions::saturation_locked(state, piece);
    let mut count = 0;
    let mut maximum: f64 = 0.0;
    let mut total = 0.0;
    let mut decisive = false;
    let mut seen = BTreeSet::new();
    if !disabled {
        for row in 0..8 {
            for col in 0..8 {
                let at = Square { row, col };
                let Some(victim) = state
                    .at(at)
                    .filter(|victim| victim.color == color && !seen.contains(&victim.id))
                else {
                    continue;
                };
                if !crate::movement::v7_can_capture_target(state, piece, victim, false, false)?
                    || !crate::v7_threat::piece_attacks_square_v7(state, piece, target, at)?
                {
                    continue;
                }
                seen.insert(victim.id.clone());
                let value = piece_value(state, Some(victim))? * 100.0;
                count += 1;
                maximum = maximum.max(value);
                total += value;
                if rule_royal(state, victim)? {
                    decisive = true;
                }
            }
        }
    }
    if decisive {
        return Ok(-2000.0);
    }
    let value = piece_value(state, Some(piece))? * 100.0;
    let mut score = value * 0.55;
    if count > 0 {
        score -= 420.0 + f64::from(count) * 65.0 + maximum * 0.85 + total * 0.15;
    } else {
        score += if disabled { 210.0 } else { 160.0 };
    }
    if crate::observation::truth(piece.extra.get("shielded")) {
        score -= 180.0;
    }
    if !attackers(state, target, color)?.is_empty() {
        score -= 180.0 + value * 0.25;
    }
    Ok(score.clamp(-50000.0, 2600.0) / 25.0)
}

fn source_number(value: Option<&Value>, depth: usize) -> Result<Option<f64>> {
    if depth > 64 {
        return Err(EngineError::InvalidState(
            "source AI number coercion exceeds 64 nested arrays".into(),
        ));
    }
    if let Some(number) = crate::card_effects::js_number(value, 0) {
        return Ok(Some(number));
    }
    // normalizeCollapseDepth permits Infinity from JSON strings: Number is
    // coerced before max/floor/min clamps it. The shared finite-number helper
    // intentionally omits this case for callers that require finite values.
    Ok(match value {
        Some(Value::String(text)) => {
            let text = text.trim();
            match text {
                "Infinity" | "+Infinity" => Some(f64::INFINITY),
                "-Infinity" => Some(f64::NEG_INFINITY),
                _ if !text.is_empty()
                    && text
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || b"+-.eE".contains(&byte)) =>
                {
                    text.parse::<f64>()
                        .ok()
                        .filter(|number| number.is_infinite())
                }
                _ => None,
            }
        }
        Some(Value::Array(values))
            if values.len() == 1 && matches!(&values[0], Value::Array(_) | Value::String(_)) =>
        {
            return source_number(values.first(), depth + 1);
        }
        _ => None,
    })
}

fn collapse_would_defeat(state: &GameState, color: Color) -> Result<bool> {
    let fallback = if crate::observation::truth(state.extra.get("collapsed")) {
        1.0
    } else {
        0.0
    };
    let raw = source_number(state.extra.get("collapseDepth"), 0)?.unwrap_or(fallback);
    let depth = (if raw == 0.0 || raw.is_nan() {
        fallback
    } else {
        raw
    })
    .floor()
    .max(0.0)
    .min(4.0) as u8;
    if depth == 4 {
        return Ok(false);
    }
    let mut removed = BTreeSet::new();
    for row in depth..8 - depth {
        for col in depth..8 - depth {
            if (row == depth || col == depth || row == 7 - depth || col == 7 - depth)
                && let Some(piece) = state.at(Square { row, col })
            {
                removed.insert(piece.id.clone());
            }
        }
    }
    let own: Vec<_> = state
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|piece| piece.color == color && removed.contains(&piece.id))
        .collect();
    if own.is_empty() {
        return Ok(false);
    }
    if own
        .iter()
        .any(|piece| matches!(piece.kind.as_str(), "vip" | "merchant"))
    {
        return Ok(true);
    }
    let regency = state.flag("regency", color);
    let queen_alive =
        state.board.iter().flatten().flatten().any(|piece| {
            piece.color == color && piece.kind == "queen" && !removed.contains(&piece.id)
        });
    for piece in &own {
        if crate::v7_board_hazards::source_royal_king(state, piece)?
            && !state.democracy_protects_royal(piece)
            && (!regency || !queen_alive)
        {
            return Ok(true);
        }
    }
    Ok(!state.flag("democracy", color)
        && state.flag("kingDead", color)
        && regency
        && !queen_alive
        && own.iter().any(|piece| {
            crate::observation::truth(piece.extra.get("regencyHeir")) || piece.kind == "queen"
        }))
}

fn simple_move_board(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &crate::MoveTarget,
) -> Result<Option<GameState>> {
    if [
        "colossusAttack",
        "shotgunBlast",
        "shotgunSnipe",
        "merchantBuy",
        "setLogDirection",
    ]
    .iter()
    .any(|field| crate::observation::truth(target.flags.get(*field)))
    {
        return Ok(None);
    }
    let to = target.square();
    let mut next = state.clone();
    let destination = state.at(to).cloned();
    let swap = crate::observation::truth(target.flags.get("substitutionSwap"));
    if swap
        && destination.as_ref().is_none_or(|victim| {
            !crate::movement::v7_can_substitute_pieces(state, piece, victim, true)
        })
    {
        return Ok(None);
    }
    for cell in next.board.iter_mut().flatten() {
        if cell.as_ref().is_some_and(|candidate| {
            candidate.id == piece.id
                || destination
                    .as_ref()
                    .is_some_and(|victim| candidate.id == victim.id)
        }) {
            *cell = None;
        }
    }
    if swap {
        let victim = destination.ok_or_else(|| {
            EngineError::InvalidState("source AI substitution board lost target".into())
        })?;
        if piece.is_large() && victim.is_large() {
            for dr in 0..=1 {
                for dc in 0..=1 {
                    let a = from.offset(dr, dc).ok_or_else(|| {
                        EngineError::InvalidState(
                            "source AI substitution origin footprint exceeds board".into(),
                        )
                    })?;
                    let b = to.offset(dr, dc).ok_or_else(|| {
                        EngineError::InvalidState(
                            "source AI substitution destination footprint exceeds board".into(),
                        )
                    })?;
                    next.board[a.row as usize][a.col as usize] = Some(victim.clone());
                    next.board[b.row as usize][b.col as usize] = Some(piece.clone());
                }
            }
        } else {
            next.board[from.row as usize][from.col as usize] = Some(victim);
            next.board[to.row as usize][to.col as usize] = Some(piece.clone());
        }
    } else {
        next.board[to.row as usize][to.col as usize] = Some(piece.clone());
    }
    Ok(Some(next))
}

fn can_ai_use_collapse_safely(state: &GameState, color: Color) -> Result<bool> {
    if !collapse_would_defeat(state, color)? {
        return Ok(true);
    }
    for (square, piece) in piece_entries(state, |piece| {
        Ok(piece.color == color && !crate::movement::frozen(piece))
    })? {
        let from = crate::transition::normalize_piece_square(state, square)?;
        for target in crate::movement::v7_collect_ai_moves_for_piece(state, &piece, from)? {
            if let Some(board) = simple_move_board(state, &piece, from, &target)?
                && !collapse_would_defeat(&board, color)?
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameConfig, RngState};

    fn empty() -> GameState {
        let mut state =
            crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7)
                .unwrap();
        state.mode = "play".into();
        state.board = vec![vec![None; 8]; 8];
        state.deck_slots.white.clear();
        state.deck_slots.black.clear();
        state
    }

    fn card(id: &str) -> CardSlot {
        let definition = crate::card_registry::definition_for(RULES_VERSION_V7, id).unwrap();
        let mut card: CardSlot =
            serde_json::from_value(definition.source_definition.clone()).unwrap();
        card.instance_id = format!("ai-policy-{id}");
        card
    }

    fn put(state: &mut GameState, kind: &str, color: Color, row: u8, col: u8) {
        state.board[usize::from(row)][usize::from(col)] = Some(Piece::new(
            kind,
            color,
            format!("ai-policy-{}-{kind}-{row}-{col}", color.as_str()),
        ));
    }

    #[test]
    fn portal_and_hypocrisy_keep_source_center_order_and_ai_caps() {
        let mut state = empty();
        state.turn = Color::Black;
        state.extra.insert(
            "targeting".into(),
            json!({"cardId":"ui-progress","selections":[{"row":7,"col":7}]}),
        );
        let before = state.clone();
        let portal =
            collect_v7_ai_card_targets(&mut state, &card("portal-gun"), Color::White, true)
                .unwrap();
        assert_eq!(portal.len(), 16);
        assert_eq!(
            portal[0],
            Some(json!({"selections":[{"row":3,"col":3},{"row":3,"col":4}]}))
        );
        assert_eq!(
            portal[6],
            Some(json!({"selections":[{"row":3,"col":3},{"row":3,"col":5}]}))
        );
        assert_eq!(
            portal[7],
            Some(json!({"selections":[{"row":3,"col":4},{"row":4,"col":3}]}))
        );
        assert_eq!(
            portal[15],
            Some(json!({"selections":[{"row":4,"col":3},{"row":2,"col":4}]}))
        );
        let hypocrisy =
            collect_v7_ai_card_targets(&mut state, &card("hypocrisy"), Color::White, true).unwrap();
        assert_eq!(
            hypocrisy,
            vec![Some(json!({"selections":[
                {"row":3,"col":3},{"row":3,"col":4},{"row":4,"col":3},{"row":4,"col":4},
            ]}))]
        );
        assert_eq!(state, before);
    }

    #[test]
    fn exhaustive_scarecrow_keeps_occupied_candidate_score_ties_in_row_order() {
        let mut state = empty();
        for col in 0..8 {
            put(&mut state, "pawn", Color::White, 2, col);
        }
        for col in 0..4 {
            put(&mut state, "pawn", Color::White, 4, col);
        }
        state.extra.shift_remove("targeting");
        let before = state.clone();
        let card = card("scarecrow");
        let limited = collect_v7_ai_card_targets(&mut state, &card, Color::White, false).unwrap();
        let exhaustive = collect_v7_ai_card_targets(&mut state, &card, Color::White, true).unwrap();
        assert_eq!(limited.len(), 10);
        assert_eq!(exhaustive.len(), 12);
        assert_eq!(limited, exhaustive[..10]);
        // main84839 scores canReserveScarecrow, which is false for these
        // own occupied candidates even though main105919 admits them.
        assert_eq!(exhaustive[0], Some(json!({"row":2,"col":0})));
        assert_eq!(exhaustive[7], Some(json!({"row":2,"col":7})));
        assert_eq!(exhaustive[11], Some(json!({"row":4,"col":3})));
        assert_eq!(state, before);
    }

    #[test]
    fn pawn_storm_ai_selects_first_four_and_does_not_enumerate_ui_permutations() {
        let mut state = empty();
        for col in 0..8 {
            put(&mut state, "pawn", Color::White, 2, col);
        }
        for col in 0..2 {
            put(&mut state, "pawn", Color::White, 4, col);
        }
        let before = state.clone();
        let targets =
            collect_v7_ai_card_targets(&mut state, &card("pawn-storm"), Color::White, true)
                .unwrap();
        assert_eq!(
            targets,
            vec![Some(json!({"selections":[
                {"row":2,"col":0},{"row":2,"col":1},{"row":2,"col":2},{"row":2,"col":3},
            ]}))]
        );
        assert_eq!(state, before);
    }

    #[test]
    fn declined_untargeted_probe_restores_state_but_carries_its_rng_draw() {
        let mut state = empty();
        let thief = card("thief");
        state.deck_slots.white = vec![thief.clone()];
        state.turn = Color::Black;
        state
            .extra
            .insert("targeting".into(), json!({"cardId":"retained-window"}));
        state
            .extra
            .insert("selected".into(), json!({"row":6,"col":0}));
        state
            .extra
            .insert("legalMoves".into(), json!([{"row":5,"col":0}]));
        state.rng = RngState {
            tape: vec![0.25, 0.75],
            ..RngState::seeded(7)
        };
        let mut expected = state.clone();
        expected.rng.sample().unwrap();
        assert!(!is_v7_ai_card_playable(&mut state, &thief, Color::White).unwrap());
        assert_eq!(state, expected);
        expected.rng.sample().unwrap();
        assert!(
            collect_v7_ai_card_targets(&mut state, &thief, Color::White, true)
                .unwrap()
                .is_empty()
        );
        assert_eq!(state, expected);
        assert!(!state.is_ai_simulation());
        assert!(!state.deck_slots.white[0].extra.contains_key("devCard"));
    }

    #[test]
    fn ai_stream_keeps_dev_pending_bypass_and_normal_deck_order() {
        let mut state = empty();
        let mut developer = card("portal-gun");
        developer.extra.insert("devCard".into(), json!(true));
        developer
            .extra
            .insert("nextTurnPending".into(), json!(true));
        let mut delayed = card("hypocrisy");
        delayed.instance_id = "ai-policy-delayed-hypocrisy".into();
        delayed.extra.insert("nextTurnPending".into(), json!(true));
        let ready = card("hypocrisy");
        state.deck_slots.white = vec![developer.clone(), delayed, ready.clone()];
        state.turn = Color::Black;
        let before = state.clone();
        let actions = collect_v7_ai_card_actions(&mut state, Color::White, true).unwrap();
        assert_eq!(actions.len(), 17);
        assert!(actions[..16].iter().all(
            |action| action.card_instance_id.as_deref() == Some(developer.instance_id.as_str())
        ));
        assert_eq!(
            actions[16].card_instance_id.as_deref(),
            Some(ready.instance_id.as_str())
        );
        assert!(!is_v7_ai_card_playable(&mut state, &developer, Color::White).unwrap());
        assert_eq!(state, before);
    }

    #[test]
    fn rule_ticket_ai_targets_include_frozen_metadata_and_cap_twelve() {
        let mut state = empty();
        let targets =
            collect_v7_ai_card_targets(&mut state, &card("rule-ticket"), Color::White, true)
                .unwrap();
        assert_eq!(targets.len(), 12);
        for target in targets {
            let target = target.unwrap();
            assert_eq!(target.as_object().unwrap().len(), 3);
            let definition = crate::card_registry::definition_for(
                RULES_VERSION_V7,
                target["ruleId"].as_str().unwrap(),
            )
            .unwrap();
            assert_eq!(target["ruleEffect"], definition.effect);
            assert!(target["ruleStars"].as_f64().unwrap().is_finite());
        }
    }

    #[test]
    fn source_targeting_and_turn_restore_after_explicit_preparation_error() {
        let mut state = empty();
        state.turn = Color::Black;
        state
            .extra
            .insert("targeting".into(), json!({"cardId":"retained-window"}));
        state
            .extra
            .insert("additionalRuleCards".into(), json!({"unexpected":"object"}));
        let before = state.clone();
        let error =
            collect_v7_ai_card_targets(&mut state, &card("rule-ticket"), Color::White, true)
                .unwrap_err();
        assert!(
            matches!(error, EngineError::InvalidState(message) if message.contains("additionalRuleCards"))
        );
        assert_eq!(state, before);
    }

    #[test]
    fn collapse_candidate_guard_keeps_source_numeric_coercion_and_legacy_fallback() {
        let mut state = empty();
        put(&mut state, "vip", Color::White, 0, 0);
        assert!(collapse_would_defeat(&state, Color::White).unwrap());
        state.extra.insert("collapsed".into(), json!(true));
        state.extra.insert("collapseDepth".into(), json!([]));
        assert!(!collapse_would_defeat(&state, Color::White).unwrap());
        for value in [json!("Infinity"), json!("1e999"), json!(["Infinity"])] {
            state.extra.insert("collapseDepth".into(), value);
            assert!(!collapse_would_defeat(&state, Color::White).unwrap());
        }
        state
            .extra
            .insert("collapseDepth".into(), json!("-Infinity"));
        assert!(collapse_would_defeat(&state, Color::White).unwrap());
    }

    #[test]
    fn frozen_original_ai_policy_matches_targets_state_and_rng_when_receipt_is_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_AI_CARD_CANDIDATE_RECEIPT") else {
            return;
        };
        let receipt: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            receipt["sourceSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        // 공개 Oracle의 완성 후보는 호출 범위 안에서만 보완한다. 이 내부
        // 정책 영수증은 __sourceCollectAiCardTargets와 같은 원문 collector를
        // 기본 전역으로 유지한 뒤 collectValidAiActions를 호출해야 한다.
        assert_eq!(receipt["sourceCollector"], "original-ai-card-policy");
        let cases = receipt["cases"].as_array().expect("source AI policy cases");
        assert!(!cases.is_empty(), "source AI policy receipt is empty");
        for case in cases {
            let label = case["id"].as_str().expect("source case ID");
            let color: Color = serde_json::from_value(case["color"].clone()).unwrap();
            let mut state: GameState =
                serde_json::from_value(case["before"]["state"].clone()).unwrap();
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(case["before"]["rng"].clone()).unwrap();
            let actual = match case["operation"].as_str().expect("source operation") {
                "actions" => serde_json::to_value(
                    collect_v7_ai_card_actions(
                        &mut state,
                        color,
                        case["kingDangerOnly"].as_bool().unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap(),
                "targets" | "playable" => {
                    let instance = case["cardInstanceId"]
                        .as_str()
                        .expect("source card instance");
                    let card = state
                        .deck_slots
                        .get(color)
                        .iter()
                        .find(|card| card.instance_id == instance)
                        .expect("source card instance is absent from owned hand")
                        .clone();
                    if case["operation"] == "playable" {
                        json!(is_v7_ai_card_playable(&mut state, &card, color).unwrap())
                    } else {
                        serde_json::to_value(
                            collect_v7_ai_card_targets(
                                &mut state,
                                &card,
                                color,
                                case["exhaustive"].as_bool().unwrap(),
                            )
                            .unwrap(),
                        )
                        .unwrap()
                    }
                }
                operation => panic!("unknown source AI policy operation {operation}"),
            };
            assert_eq!(
                serde_jcs::to_vec(&actual).unwrap(),
                serde_jcs::to_vec(&case["result"]).unwrap(),
                "{label} source candidate result diverged"
            );
            let mut expected: GameState =
                serde_json::from_value(case["after"]["state"].clone()).unwrap();
            expected.ruleset_id = RULES_VERSION_V7.into();
            expected.rng = serde_json::from_value(case["after"]["rng"].clone()).unwrap();
            assert_eq!(
                state, expected,
                "{label} source preparation state or RNG diverged"
            );
        }
    }
}
