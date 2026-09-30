//! Source-ordered incoming-side callbacks of frozen v7 `completeTurnAfterMove`.
//!
//! The integration owner calls these individually at lines 93740, 93755-60,
//! and 93771 of the pinned client. A callback may be inert for a particular
//! state, but active unported branches must fail before a transaction commits.

use crate::v7_turn_flow::V7FlowControl;
use crate::{CardSlot, Color, EngineError, GameState, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn unsupported(callback: &str, field: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!("v7 {callback} requires unported active {field}"))
}

fn boundary(state: &GameState, incoming: Color) -> Result<Option<V7FlowControl>> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 incoming reactions on rules version {}",
            state.ruleset_id
        )));
    }
    if state.mode == "gameover" {
        return Ok(Some(V7FlowControl::Terminal));
    }
    if state.mode != "play" || state.turn != incoming {
        return Err(EngineError::WrongActor);
    }
    Ok(None)
}

fn herald_threats(state: &mut GameState, moving: Color) -> Result<V7FlowControl> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 resolveHeraldThreats on rules version {}",
            state.ruleset_id
        )));
    }
    if state.mode == "gameover" {
        return Ok(V7FlowControl::Terminal);
    }
    if state.mode != "play" {
        return Err(EngineError::WrongActor);
    }
    // The shared v7 resolver preserves Herald -> racing -> double-check
    // source order even when no Herald remains on this board.
    let mut next = state.clone();
    let ended = crate::transition::resolve_herald_threats(&mut next, moving)?;
    *state = next;
    Ok(if ended {
        V7FlowControl::Terminal
    } else {
        V7FlowControl::Continue
    })
}

/// Source 93739-42: only after Siren, and only when the actor switched.
pub(crate) fn herald_after_siren(state: &mut GameState, moving: Color) -> Result<V7FlowControl> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 resolveHeraldThreats on rules version {}",
            state.ruleset_id
        )));
    }
    if state.mode == "gameover" {
        return Ok(V7FlowControl::Terminal);
    }
    if state.mode != "play" {
        return Err(EngineError::WrongActor);
    }
    if state.turn == moving {
        return Ok(V7FlowControl::Continue);
    }
    herald_threats(state, moving)
}

/// Source 93771-72: after merchant/conscription/palace effects and the clock.
pub(crate) fn herald_after_economy(state: &mut GameState, moving: Color) -> Result<V7FlowControl> {
    herald_threats(state, moving)
}

fn vacant_card() -> CardSlot {
    CardSlot {
        id: String::new(),
        effect: String::new(),
        instance_id: String::new(),
        stars: 0.0,
        used: false,
        recovering: false,
        vacant: true,
        extra: Default::default(),
        source_order: Vec::new(),
    }
}

fn player_deck(state: &mut GameState, color: Color) -> Result<&mut Vec<CardSlot>> {
    let slot_count = match state.extra.get("gameStyle").and_then(Value::as_str) {
        None | Some("normal") => 3,
        Some("chaos" | "grand") => 6,
        Some(other) => {
            return Err(unsupported("playerDeck", &format!("gameStyle={other}")));
        }
    };
    let deck = state.deck_slots.get_mut(color);
    if deck.len() < slot_count {
        deck.resize_with(slot_count, vacant_card);
    }
    Ok(deck)
}

/// Source 93756/87117. Time-traveler campaign cards recover at the incoming
/// turn boundary; their `used` field remains present, while `recovering` and
/// `usedAt` are deleted. No RNG or replay event is consumed by this callback.
pub(crate) fn reset_time_traveler_cards_for_turn(
    state: &mut GameState,
    incoming: Color,
) -> Result<V7FlowControl> {
    if let Some(flow) = boundary(state, incoming)? {
        return Ok(flow);
    }
    if state
        .extra
        .get("campaign")
        .and_then(|campaign| campaign.get("setup"))
        .and_then(Value::as_str)
        != Some("timeTraveler")
    {
        return Ok(V7FlowControl::Continue);
    }
    let mut next = state.clone();
    for card in player_deck(&mut next, incoming)?.iter_mut() {
        if card.vacant
            || !crate::observation::truth(card.extra.get("campaignCard"))
            || !card.effect.starts_with("time")
        {
            continue;
        }
        card.used = false;
        if !card.source_order.iter().any(|field| field == "used") {
            card.source_order.push("used".into());
        }
        card.extra.shift_remove("usedAt");
        card.recovering = false;
        card.source_order.retain(|field| field != "recovering");
    }
    *state = next;
    Ok(V7FlowControl::Continue)
}

/// Source 93755/102477. The shared campaign owner preserves period marking,
/// blood card identity, acquisition ordering, and replay/RNG as one callback.
pub(crate) fn maybe_grant_night_blood_for_turn(
    state: &mut GameState,
    incoming: Color,
) -> Result<V7FlowControl> {
    if let Some(flow) = boundary(state, incoming)? {
        return Ok(flow);
    }
    crate::v7_campaign::maybe_grant_night_blood_for_turn(state, incoming)?;
    Ok(if state.mode == "gameover" {
        V7FlowControl::Terminal
    } else {
        V7FlowControl::Continue
    })
}
/// Source 93759/93128. Promotions occur after incoming coronation protection
/// has expired and before local Brutus/Don Quixote. Row-major identity de-
/// duplication, side effects, animation, and concealed logs stay in one clone.
pub(crate) fn resolve_holdout_promotions(
    state: &mut GameState,
    incoming: Color,
) -> Result<V7FlowControl> {
    if let Some(flow) = boundary(state, incoming)? {
        return Ok(flow);
    }
    crate::v7_promotion::resolve_holdout_promotions_v7(state, incoming)?;
    Ok(if state.mode == "gameover" {
        V7FlowControl::Terminal
    } else {
        V7FlowControl::Continue
    })
}

/// Source 93760/115340. Brutus tests own royals in source board order rather
/// than move-generator order, performs the shared ordinary capture, and only
/// moves if that same attacker is still present at its original cell.
pub(crate) fn resolve_local_brutus(
    state: &mut GameState,
    incoming: Color,
) -> Result<V7FlowControl> {
    if let Some(flow) = boundary(state, incoming)? {
        return Ok(flow);
    }
    let mut next = state.clone();
    let mut seen = BTreeSet::new();
    let mut brutuses = Vec::new();
    for (row, cells) in next.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            if let Some(piece) = piece
                && piece.color == incoming
                && piece.ability_kind() == "brutus"
                && seen.insert(piece.id.clone())
            {
                brutuses.push((Square::new(row as u8, col as u8)?, piece.clone()));
            }
        }
    }
    for (origin, saved) in brutuses {
        if next.mode == "gameover" {
            break;
        }
        let mut active = next
            .board
            .iter()
            .flatten()
            .flatten()
            .find(|piece| piece.id == saved.id)
            .cloned()
            .unwrap_or(saved);
        if active.color != incoming || active.ability_kind() != "brutus" {
            continue;
        }
        let moves = crate::movement::v7_brutus_royal_capture_moves(&next, &active, origin)?;
        let mut seen_royals = BTreeSet::new();
        let mut royals = Vec::new();
        for (row, cells) in next.board.iter().enumerate() {
            for (col, piece) in cells.iter().enumerate() {
                if let Some(piece) = piece
                    && piece.color == incoming
                    && piece.id != active.id
                    && crate::v7_threat::is_royal_identity_v7(&next, piece)
                    && seen_royals.insert(piece.id.clone())
                {
                    royals.push((Square::new(row as u8, col as u8)?, piece.clone()));
                }
            }
        }
        for (at, royal) in royals {
            if !moves.iter().any(|target| target.square() == at) || royal.flag("shielded") {
                continue;
            }
            let captured = crate::v7_capture_reactions::capture_at(
                &mut next,
                &mut active,
                at,
                &crate::v7_capture_reactions::CaptureOptions {
                    attacker_landing: Some(at),
                    ..Default::default()
                },
            )?;
            let Some(captured) = captured else { continue };
            let threat_probe = next.threat_probe_depth > 0;
            crate::v7_threat::mark_king_threat_removal_cause(
                &mut next,
                &captured,
                at,
                &json!({"attacker":active,"origin":origin,"effect":"brutus-betrayal","notationType":"brutus"}),
                threat_probe,
            )?;
            if next.at(origin).is_some_and(|piece| piece.id == active.id) {
                next.board[origin.row as usize][origin.col as usize] = None;
                active.moved = true;
                next.board[at.row as usize][at.col as usize] = Some(active.clone());
            }
            crate::card_effects::remember_local_movement(&mut next, &active)?;
            let move_number =
                crate::card_effects::js_number(next.extra.get("activeHistoryMoveNumber"), 0)
                    .unwrap_or(f64::from(next.full_move))
                    .max(0.0)
                    .floor();
            crate::replay::queue_notation(
                &mut next,
                "special",
                incoming,
                format!("!브루투스 {}{}", char::from(b'a' + at.col), 8 - at.row),
                "브루투스가 아군 킹을 잡았습니다.".into(),
                move_number as u64,
            )?;
            crate::replay::add_log(&mut next, "브루투스가 아군 킹을 잡았습니다.".into())?;
            let flow = if next.mode == "gameover" {
                V7FlowControl::Terminal
            } else {
                V7FlowControl::Continue
            };
            *state = next;
            return Ok(flow);
        }
    }
    let flow = if next.mode == "gameover" {
        V7FlowControl::Terminal
    } else {
        V7FlowControl::Continue
    };
    *state = next;
    Ok(flow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameConfig, Piece};

    fn empty_play() -> GameState {
        let mut state = GameState::new(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.mode = "play".into();
        state.board = vec![vec![None; 8]; 8];
        state.extra.insert("logs".into(), json!([]));
        state
    }

    #[test]
    fn herald_is_conditional_after_siren_and_terminal_after_economy() {
        let mut state = empty_play();
        state.turn = Color::Black;
        state.board[4][3] = Some(Piece::new("herald", Color::White, "h"));
        state.board[4][4] = Some(Piece::new("king", Color::Black, "k"));
        let rng = state.rng.clone();
        assert_eq!(
            herald_after_siren(&mut state, Color::Black).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(
            herald_after_economy(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal
        );
        assert_eq!(state.winner.as_deref(), Some("white"));
        assert_eq!(
            state.extra["replayEndReason"],
            "전령이 상대 킹과 협정을 이끌어냈습니다."
        );
        assert_eq!(
            state.extra["logs"][0],
            "백 승리: 전령이 상대 킹과 협정을 이끌어냈습니다."
        );
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn time_traveler_recovery_deletes_optional_fields_without_rng_or_history() {
        let mut state = empty_play();
        state
            .extra
            .insert("campaign".into(), json!({"setup":"timeTraveler"}));
        let card: CardSlot = serde_json::from_value(json!({
            "id":"time-shift","effect":"timeShift","instanceId":"t1",
            "campaignCard":true,"used":true,"recovering":true,"usedAt":123
        }))
        .unwrap();
        state.deck_slots.white[0] = card;
        let rng = state.rng.clone();
        let history = state.history.clone();
        reset_time_traveler_cards_for_turn(&mut state, Color::White).unwrap();
        let result = serde_json::to_value(&state.deck_slots.white[0]).unwrap();
        assert_eq!(result["used"], false);
        assert!(result.get("recovering").is_none());
        assert!(result.get("usedAt").is_none());
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn holdout_promotion_is_due_on_shared_turn_and_keeps_rng_history() {
        let mut state = empty_play();
        state.turns_taken.white = 14;
        state.turns_taken.black = 14;
        let mut pawn = Piece::new("pawn", Color::White, "p");
        pawn.extra
            .insert("holdoutPromotion".into(), json!({"readyTurn":14}));
        pawn.extra.insert("noPromotion".into(), json!(true));
        state.board[4][4] = Some(pawn);
        let rng = state.rng.clone();
        let history = state.history.clone();
        resolve_holdout_promotions(&mut state, Color::White).unwrap();
        let queen = state.board[4][4].as_ref().unwrap();
        assert_eq!(queen.kind, "queen");
        assert!(queen.moved);
        assert_eq!(queen.extra["origin"], "e4");
        assert_eq!(queen.extra["promotedFromPawn"], true);
        assert!(!queen.extra.contains_key("holdoutPromotion"));
        assert!(!queen.extra.contains_key("noPromotion"));
        assert_eq!(
            state.extra["logs"][0],
            "존버: e4의 폰이 퀸으로 프로모션했습니다."
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn holdout_spy_and_coronation_follow_source_reaction_order() {
        let mut state = empty_play();
        state.turns_taken.white = 14;
        state.turns_taken.black = 14;
        state
            .extra
            .insert("coronation".into(), json!({"white":true,"black":false}));
        let mut pawn = Piece::new("pawn", Color::White, "p");
        pawn.extra
            .insert("holdoutPromotion".into(), json!({"readyTurn":14}));
        pawn.extra.insert("spyOwner".into(), json!("black"));
        pawn.extra.insert("protected".into(), json!(false));
        state.board[4][4] = Some(pawn);
        resolve_holdout_promotions(&mut state, Color::White).unwrap();
        let queen = state.board[4][4].as_ref().unwrap();
        assert_eq!(queen.kind, "queen");
        assert_eq!(queen.color, Color::Black);
        assert_eq!(
            queen.extra["coronationProtection"],
            json!({"color":"white","startTurn":14,"previousProtected":false})
        );
        assert_eq!(queen.extra["protected"], true);
        assert!(!queen.extra.contains_key("spyOwner"));
    }

    #[test]
    fn first_night_grants_blood_once_with_source_identity_and_notation_draws() {
        let mut state = empty_play();
        state.turns_taken.white = 4;
        state.turns_taken.black = 4;
        state
            .extra
            .insert("campaign".into(), json!({"setup":"bloodMoon"}));
        state.board[4][4] = Some(Piece::new("vampireLord", Color::White, "v"));
        let before = state.clone();
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        expected_rng.sample().unwrap();
        maybe_grant_night_blood_for_turn(&mut state, Color::White).unwrap();
        assert_eq!(
            state.extra["campaign"]["bloodMoon"]["nightBloodPeriods"]["white"],
            json!(1)
        );
        let blood = state
            .deck_slots
            .white
            .iter()
            .find(|card| !card.vacant && card.id == "blood")
            .unwrap();
        assert!(blood.instance_id.starts_with("blood-"));
        assert_eq!(blood.extra["campaignCard"], json!(true));
        assert_eq!(state.rng, expected_rng);
        assert_eq!(state.history, before.history);
        let once = state.clone();
        maybe_grant_night_blood_for_turn(&mut state, Color::White).unwrap();
        assert_eq!(state, once);
    }

    #[test]
    fn blood_moon_without_vampire_normalizes_only_campaign_data() {
        let mut state = empty_play();
        state.turns_taken.white = 1;
        state.turns_taken.black = 1;
        state
            .extra
            .insert("campaign".into(), json!({"setup":"bloodMoon"}));
        let rng = state.rng.clone();
        let history = state.history.clone();
        maybe_grant_night_blood_for_turn(&mut state, Color::White).unwrap();
        assert_eq!(
            state.extra["campaign"],
            json!({"setup":"bloodMoon","bloodMoon":{
                "nightBloodPeriods":{"white":null,"black":null},"veilUntil":null,
                "sunlightOverride":null,"coffinId":null,"coffinIds":[],"lastPeriod":0
            }})
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn brutus_captures_first_friendly_royal_and_records_source_special_notation() {
        let mut state = empty_play();
        state.board[4][4] = Some(Piece::new("brutus", Color::White, "b"));
        state.board[4][5] = Some(Piece::new("king", Color::White, "k"));
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().unwrap();
        assert_eq!(
            resolve_local_brutus(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal
        );
        assert!(state.board[4][4].is_none());
        let active = state.board[4][5].as_ref().unwrap();
        assert_eq!(active.id, "b");
        assert!(active.moved);
        assert_eq!(state.captures.white[0].id, "k");
        assert_eq!(state.winner.as_deref(), Some("black"));
        assert_eq!(
            state.extra["pendingNotations"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["kind"],
            json!("special")
        );
        assert_eq!(
            state.extra["pendingNotations"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["text"],
            json!("!브루투스 f4")
        );
        assert_eq!(state.rng, expected_rng);
    }

    #[test]
    fn brutus_without_friendly_royal_is_inert() {
        let mut state = empty_play();
        state.board[4][4] = Some(Piece::new("brutus", Color::White, "b"));
        let before = state.clone();
        assert_eq!(
            resolve_local_brutus(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state, before);
    }
}
