//! Source-pinned choice-card objects for the frozen v7 client.
//!
//! Card application is separate from the host's cost, used-card settlement,
//! turn advancement and replay commit. A UI choice's order and identity are
//! part of its action; this module does not turn a partial choice into an
//! apparently complete action. Source: main-OahWs0tU.js, SHA-256
//! e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c.

use crate::{
    Action, ActionKind, CardSlot, EngineError, GameState, Piece, RULES_VERSION_V7, Result, Square,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const IDS: &[&str] = &[
    "barricade",
    "black-box",
    "cleanup",
    "cleanup-sacrifice",
    "hypocrisy",
    "joker",
    "miracle",
    "portal-gun",
    "rule-ticket",
    "taboo",
    "trolley",
    "white-box",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ChoiceStage {
    /// Direction choice precedes a center square; their order cannot invert.
    DirectionThenCenter,
    /// The source UI toggles a picked square and preserves click order.
    OrderedSquares { min: usize, max: usize },
    /// A single piece square is the source's first and final choice.
    SinglePiece,
    /// The source UI selects a previously used card instance, not a card ID.
    CardInstance,
    /// The source UI selects one eligible RULE card ID.
    RuleId,
    /// The card queues an opponent dilemma chosen at the next turn entry.
    DeferredDilemma,
    /// The source automatically shuffles and attempts a nested card effect.
    AutomaticRandom,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ChoiceObject {
    pub(super) id: &'static str,
    pub(super) effect: &'static str,
    pub(super) stage: ChoiceStage,
}

pub(super) const OBJECTS: &[ChoiceObject] = &[
    ChoiceObject {
        id: "barricade",
        effect: "barricade",
        stage: ChoiceStage::DirectionThenCenter,
    },
    ChoiceObject {
        id: "black-box",
        effect: "blackBox",
        stage: ChoiceStage::AutomaticRandom,
    },
    ChoiceObject {
        id: "cleanup",
        effect: "cleanupPieces",
        stage: ChoiceStage::OrderedSquares { min: 1, max: 3 },
    },
    ChoiceObject {
        id: "cleanup-sacrifice",
        effect: "cleanupSacrifice",
        stage: ChoiceStage::SinglePiece,
    },
    ChoiceObject {
        id: "hypocrisy",
        effect: "hypocrisy",
        stage: ChoiceStage::OrderedSquares { min: 4, max: 4 },
    },
    ChoiceObject {
        id: "joker",
        effect: "joker",
        stage: ChoiceStage::CardInstance,
    },
    ChoiceObject {
        id: "miracle",
        effect: "miracle",
        stage: ChoiceStage::SinglePiece,
    },
    ChoiceObject {
        id: "portal-gun",
        effect: "portalGun",
        stage: ChoiceStage::OrderedSquares { min: 2, max: 2 },
    },
    ChoiceObject {
        id: "rule-ticket",
        effect: "ruleTicket",
        stage: ChoiceStage::RuleId,
    },
    ChoiceObject {
        id: "taboo",
        effect: "taboo",
        stage: ChoiceStage::OrderedSquares { min: 2, max: 2 },
    },
    ChoiceObject {
        id: "trolley",
        effect: "trolley",
        stage: ChoiceStage::DeferredDilemma,
    },
    ChoiceObject {
        id: "white-box",
        effect: "whiteBox",
        stage: ChoiceStage::AutomaticRandom,
    },
];

fn object(card_id: &str) -> Option<&'static ChoiceObject> {
    OBJECTS.iter().find(|object| object.id == card_id)
}

fn admit(state: &GameState, card: &CardSlot, action: &Action) -> Result<&'static ChoiceObject> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 choice card {} on rules version {}",
            card.id, state.ruleset_id
        )));
    }
    let object = object(&card.id)
        .ok_or_else(|| EngineError::UnsupportedFeature(format!("v7 choice card {}", card.id)))?;
    let source = crate::card_registry::definition_for(RULES_VERSION_V7, object.id)?;
    if source.effect != object.effect || card.effect != object.effect {
        return Err(EngineError::InvalidState(format!(
            "v7 choice card catalog effect drift for {}",
            object.id
        )));
    }
    if action.kind != ActionKind::Card
        || action.color != state.turn
        || action.card_id.as_deref() != Some(card.id.as_str())
        || action.card_instance_id.as_deref() != Some(card.instance_id.as_str())
        || action.from.is_some()
        || action.destination.is_some()
        || !action.extra.is_empty()
    {
        return Err(EngineError::IllegalAction);
    }
    Ok(object)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum FirstChoice {
    Direction(&'static str),
    Square(Square),
    CardInstance(String),
    RuleId(String),
}

/// The first interactive choices in frozen UI order. Empty is a verified
/// empty surface; an unported predicate returns UnsupportedFeature instead.
pub(super) fn first_choices(state: &GameState, card: &CardSlot) -> Result<Vec<FirstChoice>> {
    let object = object(&card.id)
        .ok_or_else(|| EngineError::UnsupportedFeature(format!("v7 choice card {}", card.id)))?;
    if state.ruleset_id != RULES_VERSION_V7 || card.effect != object.effect {
        return Err(EngineError::InvalidState(format!(
            "v7 choice first stage identity mismatch for {}",
            card.id
        )));
    }
    match object.stage {
        ChoiceStage::DirectionThenCenter => Ok(vec![
            FirstChoice::Direction("horizontal"),
            FirstChoice::Direction("vertical"),
        ]),
        ChoiceStage::CardInstance => Ok(reusable_active_cards(state, card)?
            .into_iter()
            .map(|candidate| FirstChoice::CardInstance(candidate.instance_id.clone()))
            .collect()),
        ChoiceStage::RuleId => Ok(rule_ticket_candidates(state)?
            .into_iter()
            .map(|id| FirstChoice::RuleId(id.to_owned()))
            .collect()),
        ChoiceStage::OrderedSquares { min, max }
            if matches!(object.id, "cleanup" | "hypocrisy" | "portal-gun" | "taboo") =>
        {
            let selection =
                &crate::card_registry::definition_for(RULES_VERSION_V7, object.id)?.selection;
            if selection.min_squares != min || selection.max_squares != max {
                return Err(EngineError::InvalidState(format!(
                    "v7 choice selection contract drift for {}",
                    object.id
                )));
            }
            let squares = super::target_squares(state, card)?.ok_or_else(|| {
                EngineError::InvalidState(format!(
                    "v7 choice first-click target missing for {}",
                    card.id
                ))
            })?;
            Ok(squares.into_iter().map(FirstChoice::Square).collect())
        }
        ChoiceStage::SinglePiece if object.id == "cleanup-sacrifice" => {
            Ok(cleanup_sacrifice_first_choices(state)?
                .into_iter()
                .map(FirstChoice::Square)
                .collect())
        }
        ChoiceStage::SinglePiece if object.id == "miracle" => Ok(miracle_bishop_choices(state)?
            .into_iter()
            .map(FirstChoice::Square)
            .collect()),
        ChoiceStage::AutomaticRandom | ChoiceStage::DeferredDilemma => Ok(Vec::new()),
        _ => Err(EngineError::UnsupportedFeature(format!(
            "v7 choice first-click source predicate for {}",
            card.id
        ))),
    }
}

/// Bounded complete actions exist for Joker's at-most-hand-size choice.
/// Ordered square families use the card kernel's paged staged cursor; the
/// other families stay closed until their source predicates are admitted.
pub(super) fn bounded_actions(state: &GameState, card: &CardSlot) -> Result<Vec<Action>> {
    match card.id.as_str() {
        "white-box" => Ok(Vec::new()),
        "black-box" => {
            let mut probe = state.clone();
            Ok(
                if crate::eligibility::v7_random_box_candidates(&mut probe, state.turn, false)?
                    .is_empty()
                {
                    Vec::new()
                } else {
                    vec![Action::card(state.turn, card, None)]
                },
            )
        }
        "joker" => Ok(reusable_active_cards(state, card)?
            .into_iter()
            .map(|candidate| {
                Action::card(
                    state.turn,
                    card,
                    Some(json!({"cardInstanceId":candidate.instance_id})),
                )
            })
            .collect()),
        "rule-ticket" => Ok(rule_ticket_candidates(state)?
            .into_iter()
            .map(|id| Action::card(state.turn, card, Some(json!({"ruleId":id}))))
            .collect()),
        "barricade" => {
            let mut actions = Vec::new();
            for direction in ["horizontal", "vertical"] {
                for center in barricade_centers(state, direction) {
                    actions.push(Action::card(
                        state.turn,
                        card,
                        Some(json!({"row":center.row,"col":center.col,"direction":direction})),
                    ));
                }
            }
            Ok(actions)
        }
        "miracle" => Ok(miracle_bishop_choices(state)?
            .into_iter()
            .map(|square| Action::card(state.turn, card, Some(json!(square))))
            .collect()),
        "cleanup-sacrifice" => Ok(cleanup_sacrifice_first_choices(state)?
            .into_iter()
            .map(|square| Action::card(state.turn, card, Some(json!(square))))
            .collect()),
        "trolley" => {
            let mut probe = state.clone();
            Ok(
                if crate::eligibility::v7_trolley_candidate(&mut probe, state.turn.opponent())? {
                    vec![Action::card(state.turn, card, None)]
                } else {
                    Vec::new()
                },
            )
        }
        other => Err(EngineError::UnsupportedFeature(format!(
            "v7 complete choice action enumeration for {other}"
        ))),
    }
}

/// Direct effect on a private working state. The four source-reviewed legacy
/// helpers remain the single implementation of their board mutation; the
/// choice object owns admission and delegates to them without duplication.
pub(super) fn apply(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let object = admit(state, card, action)?;
    if state.is_ai_simulation() {
        return apply_direct_effect(state, card, action, object);
    }
    let mut next = state.clone();
    let captures = apply_direct_effect(&mut next, card, action, object)?;
    *state = next;
    Ok(captures)
}

/// A source-declined nested choice retains the attempted effect's RNG and
/// partial mutations. The external host/public-card transaction owns rollback
/// for propagated errors; applyRandomBoxCard owns source-false retry.
pub(super) fn apply_virtual_effect(
    state: &mut GameState,
    card: &mut CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    if card.extra.get("devCard") != Some(&Value::Bool(true)) {
        return Err(EngineError::InvalidState(
            "v7 virtual choice card requires devCard:true".into(),
        ));
    }
    let object = admit(state, card, action).map_err(|error| match error {
        EngineError::IllegalAction => EngineError::InvalidState(
            "v7 virtual choice card action identity or actor mismatch".into(),
        ),
        other => other,
    })?;
    apply_direct_effect(state, card, action, object)
}

/// These raw kernels map only reviewed source `ok:false` guards to
/// IllegalAction. Source main769,66323,100031,100050,100659,103506,
/// 104159,104723,104949,105153. Unknown state/callback errors propagate.
fn source_decline_retry_safe(card: &CardSlot) -> bool {
    card.extra.get("devCard") == Some(&Value::Bool(true))
        && object(&card.id).is_some_and(|object| object.effect == card.effect)
        && matches!(
            card.id.as_str(),
            "barricade"
                | "cleanup"
                | "cleanup-sacrifice"
                | "hypocrisy"
                | "joker"
                | "miracle"
                | "portal-gun"
                | "rule-ticket"
                | "taboo"
                | "trolley"
        )
}

fn apply_direct_effect(
    state: &mut GameState,
    card: &CardSlot,
    action: &Action,
    object: &ChoiceObject,
) -> Result<Vec<Piece>> {
    let captures = match object.id {
        "cleanup" => {
            super::apply_cleanup(state, action)?;
            Vec::new()
        }
        "hypocrisy" => {
            super::apply_hypocrisy(state, action)?;
            Vec::new()
        }
        "portal-gun" => {
            super::apply_portal_gun(state, action)?;
            Vec::new()
        }
        "taboo" => {
            super::apply_taboo(state, action)?;
            Vec::new()
        }
        "joker" => {
            apply_joker(state, card, action)?;
            Vec::new()
        }
        "rule-ticket" => {
            apply_rule_ticket(state, action)?;
            Vec::new()
        }
        "trolley" => {
            apply_trolley(state, action)?;
            Vec::new()
        }
        "barricade" => apply_barricade(state, card, action)?,
        "miracle" => apply_miracle(state, action)?,
        "cleanup-sacrifice" => apply_cleanup_sacrifice(state, action)?,
        "black-box" => apply_black_box(state, card, action)?,
        other => {
            return Err(EngineError::UnsupportedFeature(format!(
                "v7 choice card direct effect {other}"
            )));
        }
    };
    Ok(captures)
}

fn require_white_box(state: &GameState, card: &CardSlot) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 white-box effect on other rules version".into(),
        ));
    }
    if card.id != "white-box" || card.effect != "whiteBox" || card.vacant {
        return Err(EngineError::IllegalAction);
    }
    let source = crate::card_registry::definition_for(RULES_VERSION_V7, "white-box")?;
    if source.effect != "whiteBox"
        || source.activation != Some(crate::card_registry::CardActType::Passive)
    {
        return Err(EngineError::InvalidState(
            "v7 white-box passive catalog identity drift".into(),
        ));
    }
    Ok(())
}

fn shuffled_templates(state: &mut GameState, mut candidates: Vec<Value>) -> Result<Vec<Value>> {
    // main:110015: Fisher-Yates uses exactly n - 1 random draws, including
    // swaps whose sampled index equals their original index.
    for index in (1..candidates.len()).rev() {
        let sampled = crate::transition::sample_choice(state, index + 1)?;
        candidates.swap(index, sampled);
    }
    Ok(candidates)
}

fn random_auto_target(state: &mut GameState, card: &CardSlot) -> Result<Option<Value>> {
    if !crate::observation::truth(card.extra.get("target")) {
        return Ok(None);
    }
    if !matches!(
        card.effect.as_str(),
        "emergencyEvacuation" | "panic" | "spy" | "pawnStorm"
    ) {
        return crate::v7_card_passive::first_move_target(state, card);
    }
    let squares = super::target_squares(state, card)?.ok_or_else(|| {
        EngineError::InvalidState(format!(
            "v7 random target source surface missing: {}",
            card.id
        ))
    })?;
    let mut identities = BTreeSet::new();
    let squares = squares
        .into_iter()
        .filter(|square| {
            let key = state
                .at(*square)
                .filter(|piece| !piece.id.is_empty())
                .map_or_else(
                    || format!("square:{}-{}", square.row, square.col),
                    |piece| format!("piece:{}", piece.id),
                );
            identities.insert(key)
        })
        .map(|square| json!(square))
        .collect();
    // Source always shuffles the whole unique target list before checking
    // the required size or choosing a random subset count.
    let mut shuffled = shuffled_templates(state, squares)?;
    let count = match card.effect.as_str() {
        "panic" if shuffled.len() < 2 => return Err(EngineError::IllegalAction),
        "panic" => 2,
        "spy" if shuffled.is_empty() => return Err(EngineError::IllegalAction),
        "spy" => 2.min(shuffled.len()),
        "emergencyEvacuation" if shuffled.is_empty() => return Err(EngineError::IllegalAction),
        "emergencyEvacuation" => {
            crate::transition::sample_choice(state, 3.min(shuffled.len()))? + 1
        }
        "pawnStorm" if shuffled.is_empty() => return Err(EngineError::IllegalAction),
        "pawnStorm" => crate::transition::sample_choice(state, shuffled.len())? + 1,
        _ => {
            return Err(EngineError::InvalidState(
                "v7 random target effect mismatch".into(),
            ));
        }
    };
    shuffled.truncate(count);
    Ok(Some(json!({"selections":shuffled})))
}

fn apply_black_box(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    crate::v7_card_context::with_virtual_card_limit(state, |state| {
        apply_black_box_raw(state, card, action)
    })
}

/// The generic random target supplies one square, not a staged UI choice.
/// These source handlers reject that incomplete shape before any mutation or
/// RNG draw (main:759/769,100031/100050,100659,103129,104159). A Box must retain
/// its prior shuffle/clone/target draws and try the next shuffled template.
fn pure_incomplete_box_choice(card: &CardSlot, target: Option<&Value>) -> bool {
    match card.id.as_str() {
        "brainwash" | "taboo" | "portal-gun" => target
            .and_then(|target| target.get("selections"))
            .and_then(Value::as_array)
            .is_none_or(|selections| selections.len() != 2),
        "hypocrisy" => target
            .and_then(|target| target.get("selections"))
            .and_then(Value::as_array)
            .is_none_or(|selections| selections.len() != 4),
        "cleanup" => target
            .and_then(|target| target.get("selections"))
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty),
        "joker" => target
            .and_then(|target| target.get("cardInstanceId"))
            .and_then(Value::as_str)
            .is_none(),
        "rule-ticket" => target
            .and_then(|target| target.get("ruleId"))
            .and_then(Value::as_str)
            .is_none(),
        _ => false,
    }
}

fn apply_black_box_raw(
    state: &mut GameState,
    card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    if action
        .target
        .as_ref()
        .is_some_and(|target| !target.is_null())
    {
        return Err(EngineError::IllegalAction);
    }
    let actor = state.turn;
    let candidates = crate::eligibility::v7_random_box_candidates(state, actor, false)?;
    for mut template in shuffled_templates(state, candidates)? {
        let id = template["id"]
            .as_str()
            .ok_or_else(|| EngineError::InvalidState("v7 black-box candidate ID missing".into()))?
            .to_owned();
        let name = template["name"].as_str().unwrap_or("undefined").to_owned();
        template["instanceId"] = json!(format!(
            "{id}-{}",
            crate::draft::random_suffix(
                state
                    .rng
                    .sample_opaque("source black box virtual card identity")?
            )?
        ));
        template["devCard"] = json!(true);
        let mut virtual_card: CardSlot =
            serde_json::from_value(template).map_err(EngineError::serialization)?;
        let target = match random_auto_target(state, &virtual_card) {
            Ok(target) => target,
            // A source-declined target keeps the clone/shuffle RNG draws.
            Err(EngineError::IllegalAction) => continue,
            Err(error) => return Err(error),
        };
        let nested_action = Action::card(actor, &virtual_card, target.clone());
        let application = crate::transition::begin_v7_card(state)?;
        match crate::card_registry::require_source_effect_window(state, &virtual_card) {
            Ok(()) => {}
            // Pending/exclusive source checks run before the effect and can
            // decline a virtual candidate without consuming another draw.
            Err(EngineError::IllegalAction) => continue,
            Err(error) => return Err(error),
        }
        if pure_incomplete_box_choice(&virtual_card, target.as_ref()) {
            continue;
        }
        let captures = match super::apply_virtual_effect(state, &mut virtual_card, &nested_action) {
            Ok(captures) => captures,
            Err(EngineError::IllegalAction)
                if source_decline_retry_safe(&virtual_card)
                    || super::v7_card_piece::source_decline_retry_safe(&virtual_card) =>
            {
                state.turn = actor;
                continue;
            }
            Err(EngineError::IllegalAction) => {
                // Only attested raw source declines can be retried. An
                // unaudited family's failed-effect RNG or partial mutations
                // may differ, so its error cannot choose another card.
                return Err(EngineError::UnsupportedFeature(format!(
                    "v7 black-box declined nested effect {id}: source failure mutation/RNG retry"
                )));
            }
            Err(error) => return Err(error),
        };
        crate::transition::finish_v7_card(state, application)?;
        state.turn = actor;
        if virtual_card.effect == "desperado"
            && let Some(target) = target.as_ref()
            && let (Some(row), Some(col)) = (
                target.get("row").and_then(Value::as_u64),
                target.get("col").and_then(Value::as_u64),
            )
            && row < 8
            && col < 8
            && let Some(piece) = state.at_mut(Square {
                row: row as u8,
                col: col as u8,
            })
            && let Some(desperado) = piece
                .extra
                .get_mut("desperado")
                .and_then(Value::as_object_mut)
        {
            desperado.insert("fromBlackBox".into(), json!(true));
        }
        crate::v7_board_hazards::post_card(state, actor)?;
        let live = state
            .deck_slots
            .get_mut(actor)
            .iter_mut()
            .find(|live| live.id == card.id && live.instance_id == card.instance_id)
            .ok_or(EngineError::IllegalAction)?;
        live.extra.insert("boxRevealedCardId".into(), json!(id));
        live.extra.insert("boxRevealedCardName".into(), json!(name));
        for field in [
            "randomRouletteResultType",
            "randomRouletteResult",
            "suspiciousPotionResultId",
        ] {
            if let Some(value) = virtual_card
                .extra
                .get(field)
                .filter(|value| crate::observation::truth(Some(value)))
            {
                live.extra.insert(field.into(), value.clone());
            }
        }
        crate::replay::add_log(state, format!("검은 상자: {name} 카드가 발동되었습니다."))?;
        return Ok(captures);
    }
    state.turn = actor;
    Err(EngineError::IllegalAction)
}

/// main:104736 applyRandomBoxCard. The virtual instance consumes a random
/// suffix for every attempted template, including a source-declined effect.
/// It is never added to either deck and never gains a used/usedAt field.
fn apply_white_box_effect(
    state: &mut GameState,
    color: crate::Color,
) -> Result<Option<(String, String)>> {
    crate::v7_card_context::with_virtual_card_limit(state, |state| {
        apply_white_box_effect_raw(state, color)
    })
}

fn apply_white_box_effect_raw(
    state: &mut GameState,
    color: crate::Color,
) -> Result<Option<(String, String)>> {
    let previous_turn = state.turn;
    state.turn = color;
    let candidates = crate::eligibility::v7_random_box_candidates(state, color, true)?;
    for mut template in shuffled_templates(state, candidates)? {
        let id = template["id"]
            .as_str()
            .ok_or_else(|| EngineError::InvalidState("v7 white-box candidate ID missing".into()))?
            .to_owned();
        let name = template["name"].as_str().unwrap_or("undefined").to_owned();
        template["instanceId"] = json!(format!(
            "{id}-{}",
            crate::draft::random_suffix(
                state
                    .rng
                    .sample_opaque("source white box virtual effect identity")?
            )?
        ));
        template["devCard"] = json!(true);
        if crate::observation::truth(template.get("target")) {
            return Err(EngineError::InvalidState(format!(
                "v7 White Box passive candidate {id} unexpectedly requires a target"
            )));
        }
        let virtual_card: CardSlot =
            serde_json::from_value(template).map_err(EngineError::serialization)?;
        let applied = crate::v7_card_passive::apply_virtual_effect(state, color, &virtual_card)?;
        state.turn = color;
        if !applied {
            continue;
        }
        crate::v7_board_hazards::post_card(state, color)?;
        crate::replay::add_log(state, format!("하얀 상자: {name} 카드가 발동되었습니다."))?;
        state.turn = previous_turn;
        return Ok(Some((id, name)));
    }
    state.turn = previous_turn;
    Ok(None)
}

/// Clone replays White Box without inserting it into the recipient's deck.
/// The entire virtual draw is transactional, including RNG on an unsupported
/// nested effect. Clone owns its later ledger/log and passive settlement.
pub(crate) fn apply_virtual_white_box_effect(
    state: &mut GameState,
    color: crate::Color,
    card: &CardSlot,
) -> Result<bool> {
    require_white_box(state, card)?;
    let mut next = state.clone();
    let previous_turn = next.turn;
    next.turn = color;
    let application = crate::transition::begin_v7_card(&mut next)?;
    let applied = apply_white_box_effect(&mut next, color)?.is_some();
    if applied {
        crate::transition::finish_v7_card(&mut next, application)?;
    }
    next.turn = previous_turn;
    *state = next;
    Ok(applied)
}

/// Called after the acquired White Box occupies its exact deck slot. The
/// source treats it as a passive draft effect, which cannot be entered via an
/// ordinary `Action::card`. Its draw and deck settlement share one transaction.
pub(super) fn apply_passive_acquisition(
    state: &mut GameState,
    color: crate::Color,
    slot: usize,
) -> Result<bool> {
    let card = state
        .deck_slots
        .get(color)
        .get(slot)
        .ok_or(EngineError::IllegalAction)?
        .clone();
    require_white_box(state, &card)?;
    if card.used
        || crate::observation::truth(card.extra.get("devCard"))
        || crate::observation::truth(card.extra.get("nextTurnPending"))
        || state
            .extra
            .get("aiSimulationDepth")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
    {
        return Ok(false);
    }
    let mut next = state.clone();
    let previous_turn = next.turn;
    next.turn = color;
    let application = crate::transition::begin_v7_card(&mut next)?;
    let Some((id, name)) = apply_white_box_effect(&mut next, color)? else {
        next.turn = previous_turn;
        *state = next;
        return Ok(false);
    };
    let live = next
        .deck_slots
        .get_mut(color)
        .get_mut(slot)
        .ok_or(EngineError::IllegalAction)?;
    live.extra.insert("boxRevealedCardId".into(), json!(id));
    live.extra.insert("boxRevealedCardName".into(), json!(name));
    crate::transition::finish_v7_card(&mut next, application)?;
    next.turn = previous_turn;
    crate::v7_board_hazards::post_card(&mut next, color)?;
    crate::transition::resolve_herald_threats(&mut next, color)?;
    crate::v7_card_passive::finish_acquisition_after_effect(&mut next, color, slot)?;
    *state = next;
    Ok(true)
}

// Source completeCardTargets iterates direction first, then center row-major.
// Source effect itself uses the same installation predicate for all three
// squares (main:105153-162,106013-32).
fn barricade_cells(center: Square, direction: &str) -> Option<[Square; 3]> {
    let (dr, dc) = match direction {
        "horizontal" => (0, 1),
        "vertical" => (1, 0),
        _ => return None,
    };
    Some([center.offset(-dr, -dc)?, center, center.offset(dr, dc)?])
}

fn barricade_cells_open(state: &GameState, center: Square, direction: &str) -> Option<[Square; 3]> {
    let cells = barricade_cells(center, direction)?;
    cells
        .iter()
        .all(|square| crate::eligibility::barricade_installation_open(state, *square))
        .then_some(cells)
}

fn barricade_centers(state: &GameState, direction: &str) -> Vec<Square> {
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter(|center| barricade_cells_open(state, *center, direction).is_some())
        .collect()
}

fn apply_barricade(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let source_default_direction =
        state.is_ai_simulation() || card.extra.get("devCard") == Some(&Value::Bool(true));
    let target = action
        .target
        .as_ref()
        .and_then(Value::as_object)
        .filter(|target| target.len() == 3 || target.len() == 2 && source_default_direction)
        .ok_or(EngineError::IllegalAction)?;
    let row = target
        .get("row")
        .and_then(Value::as_u64)
        .filter(|value| *value < 8)
        .ok_or(EngineError::IllegalAction)? as u8;
    let col = target
        .get("col")
        .and_then(Value::as_u64)
        .filter(|value| *value < 8)
        .ok_or(EngineError::IllegalAction)? as u8;
    let direction = target
        .get("direction")
        .and_then(Value::as_str)
        .or_else(|| source_default_direction.then_some("horizontal"))
        .ok_or(EngineError::IllegalAction)?;
    let cells = barricade_cells_open(state, Square { row, col }, direction)
        .ok_or(EngineError::IllegalAction)?;
    let actor = state.turn;
    let threat_probe = state.threat_probe_depth > 0;
    for square in cells {
        // main68029: installation crushing is neither an ordinary capture
        // nor a sacrifice. It does not add capturedTypes, capture objects,
        // prophecy cancellation, or Reaper reactions.
        if crate::eligibility::barricade_installation_open(state, square)
            && let Some(occupant) = state.at(square).cloned()
        {
            crate::card_effects::mark_vanish_animation(state, &occupant, square)?;
            crate::transition::remove_piece_from_board_cells(state, &occupant, square)?;
            crate::transition::grant_vigilance_protection(state, &occupant)?;
            crate::v7_rule_bombs::mark_deathmatch_progress(state)?;
            crate::v7_threat::mark_king_threat_removal_cause(
                state,
                &occupant,
                square,
                &json!({"label":"설치물"}),
                threat_probe,
            )?;
            crate::transition::resolve_royal_capture(state, &occupant, actor)?;
            crate::v7_capture_objectives::check_campaign_objectives(state)?;
        }
        let piece: Piece = serde_json::from_value(json!({
            "type":"wall",
            "color":"neutral",
            "id":format!("wall-{}-{}",square.row,square.col),
        }))
        .map_err(|error| EngineError::InvalidState(format!("v7 wall source shape: {error}")))?;
        state.board[usize::from(square.row)][usize::from(square.col)] = Some(piece);
    }
    Ok(Vec::new())
}

// Frozen ruleCardPool() sorts by the Korean presentation name, with an English
// ID tie-breaker (main:66221-23, 69498-509). Pinning this order keeps the
// complete public choice surface stable without a platform locale dependency.
const SOURCE_RULE_ORDER: &[&str] = &[
    "acceleration",
    "winter-kingdom",
    "revelation",
    "highway",
    "high-ground",
    "monster",
    "capture-the-flag",
    "monochrome-chess",
    "diagonal-chess",
    "cool-guy",
    "platform",
    "periodic-collapse",
    "black-hole",
    "macho-chess",
    "mistake",
    "crown",
    "camouflage-color",
    "recycling",
    "chess-960",
    "chess-344200",
    "transcendence",
    "football",
    "conveyor",
    "portal",
    "saturation",
    "rule-bombs",
    "chess-n-pow-30",
];
const TICKET_EXCLUDED: &[&str] = &[
    "chess-960",
    "chess-344200",
    "chess-n-pow-30",
    "diagonal-chess",
];

// main340,68949,73417: this conflict uses both actual player decks, including
// used cards. Revealed Box outcomes and acquisition/archive ledgers do not
// supply selectedCardIds. The pinned client's deleted-card set is empty.
fn rule_ticket_pawn_direction_conflict(state: &GameState, id: &str) -> bool {
    if !super::september18(state) {
        return false;
    }
    let selected = |wanted: &str| {
        state
            .deck_slots
            .white
            .iter()
            .chain(&state.deck_slots.black)
            .any(|card| !card.vacant && card.id == wanted)
    };
    match id {
        "reverse-pawns" => selected("rule-ticket") || selected("macho-chess"),
        "rule-ticket" | "macho-chess" => selected("reverse-pawns"),
        _ => false,
    }
}

fn rule_ticket_candidates(state: &GameState) -> Result<Vec<&'static str>> {
    let catalog = crate::card_registry::registry_for(RULES_VERSION_V7)?;
    let current = state
        .extra
        .get("appliedRuleCard")
        .and_then(|card| card.get("id"))
        .and_then(Value::as_str);
    // The board editor can display a RULE card without actually applying it.
    let editor_presentation = state.extra.get("simpleBoardEditor").is_some_and(|editor| {
        editor.get("enabled") == Some(&Value::Bool(true))
            && editor.get("ruleApplicationDisabled") == Some(&Value::Bool(true))
            && current.is_some_and(|id| {
                editor.get("ruleId").and_then(Value::as_str) == Some(id)
                    || editor
                        .get("startConfig")
                        .and_then(|value| value.get("ruleId"))
                        .and_then(Value::as_str)
                        == Some(id)
            })
    });
    let mut active = BTreeSet::new();
    if let Some(id) = current.filter(|_| !editor_presentation) {
        active.insert(id);
    }
    for (field, id_field) in [
        ("additionalRuleCards", "id"),
        ("pendingRuleTickets", "ruleId"),
    ] {
        if let Some(entries) = state.extra.get(field).filter(|value| !value.is_null()) {
            let entries = entries.as_array().ok_or_else(|| {
                EngineError::InvalidState(format!("{field} must be a source array"))
            })?;
            if entries.len() > 4096 {
                return Err(EngineError::UnsupportedFeature(format!("{field} capacity")));
            }
            for entry in entries {
                if let Some(id) = entry.get(id_field).and_then(Value::as_str) {
                    active.insert(id);
                }
            }
        }
    }
    let deathmatch_enabled = state.extra.get("deathmatchEnabled") != Some(&Value::Bool(false));
    let mut choices = Vec::new();
    for &id in SOURCE_RULE_ORDER {
        let definition = catalog.cards.get(id).ok_or_else(|| {
            EngineError::InvalidState(format!("source RULE catalog missing {id}"))
        })?;
        if definition.card_type != Some(crate::card_registry::CardType::Rule) {
            return Err(EngineError::InvalidState(format!(
                "source RULE catalog category drift for {id}"
            )));
        }
        if !TICKET_EXCLUDED.contains(&id)
            && !active.contains(id)
            && (id != "revelation" || deathmatch_enabled)
            && !rule_ticket_pawn_direction_conflict(state, id)
        {
            choices.push(id);
        }
    }
    Ok(choices)
}

// main:66323-46,103129-31. A ticket is reserved now and activated by the
// separate turn-entry callback after this player's next completed turn.
fn apply_rule_ticket(state: &mut GameState, action: &Action) -> Result<()> {
    let target = action
        .target
        .as_ref()
        .and_then(Value::as_object)
        .filter(|target| matches!(target.len(), 1 | 3))
        .ok_or(EngineError::IllegalAction)?;
    let rule_id = target
        .get("ruleId")
        .and_then(Value::as_str)
        .ok_or(EngineError::IllegalAction)?;
    if !rule_ticket_candidates(state)?.contains(&rule_id) {
        return Err(EngineError::IllegalAction);
    }
    // main83789: the source AI carries two catalog fields alongside ruleId.
    // Public intent admission still requires the UI's exact {ruleId} target.
    // Only this known internal shape may reach the same direct effect kernel.
    if target.len() == 3 {
        let definition = crate::card_registry::definition_for(RULES_VERSION_V7, rule_id)?;
        let stars = super::js_number(definition.source_definition.get("stars"), 0).unwrap_or(0.0);
        if target.get("ruleEffect").and_then(Value::as_str) != Some(definition.effect.as_str())
            || target.get("ruleStars").and_then(Value::as_f64) != Some(stars)
        {
            return Err(EngineError::IllegalAction);
        }
    }
    let timestamp = crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?;
    let suffix =
        crate::draft::random_suffix(state.rng.sample_opaque("source rule ticket identity")?)?;
    let entry = json!({
        "id":format!("rule-ticket-{timestamp}-{suffix}"),
        "ruleId":rule_id,
        "color":state.turn,
        "startTurnCount":state.turns_taken.get(state.turn),
    });
    let pending = state
        .extra
        .entry("pendingRuleTickets")
        .or_insert_with(|| json!([]));
    if !pending.is_array() {
        *pending = json!([]);
    }
    let pending = pending
        .as_array_mut()
        .ok_or_else(|| EngineError::InvalidState("pendingRuleTickets must be an array".into()))?;
    if pending.len() >= 4096 {
        return Err(EngineError::UnsupportedFeature(
            "pendingRuleTickets capacity".into(),
        ));
    }
    pending.push(entry);
    Ok(())
}

// main:104723-34. The source's availability predicate shuffles candidate
// pieces even with `randomized=false`; the mutable probe must run exactly once
// on the working state before the identity draw.
fn apply_trolley(state: &mut GameState, action: &Action) -> Result<()> {
    if super::has_card_selection(action) {
        return Err(EngineError::IllegalAction);
    }
    let target = state.turn.opponent();
    if !crate::eligibility::v7_trolley_candidate(state, target)? {
        return Err(EngineError::IllegalAction);
    }
    let timestamp = crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?;
    let suffix =
        crate::draft::random_suffix(state.rng.sample_opaque("source pending trolley identity")?)?;
    let entry = json!({
        "id":format!("trolley-pending-{timestamp}-{suffix}"),
        "color":target,
        "by":state.turn,
    });
    let pending = state
        .extra
        .entry("pendingTrolley")
        .or_insert_with(|| json!([]));
    if !pending.is_array() {
        *pending = json!([]);
    }
    let pending = pending
        .as_array_mut()
        .ok_or_else(|| EngineError::InvalidState("pendingTrolley must be an array".into()))?;
    if pending.len() >= 4096 {
        return Err(EngineError::UnsupportedFeature(
            "pendingTrolley capacity".into(),
        ));
    }
    pending.push(entry);
    Ok(())
}

// Frozen DISPLAY_STAR_CATALOG_HASHES plus the fifteen internal hashes in
// usesSelectedMiracle(main517). This is the effect's source feature gate;
// the host separately owns source/catalog authority for imported positions.
const SELECTED_MIRACLE_HASHES: &[&str] = &[
    "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
    "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
    "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs",
    "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc",
    "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g",
    "HVzv4vxzNKDRi-ylshTw9HnLmIlyiQhMScQ4caG_aSI",
    "jkECTP8OBtMeXmZF7wmz1dmgw0mgTFn8jdD-d9LZhxU",
    "s0_j9SmUy1tSUcko_X3I32akB93Iz2bvV1Cn7Nw4Uoo",
    "_Hw8otJVztzWQyIN69bg5khIqcJv-au6UwAcOQhtKcM",
    "Vj80kM6RlfZvbMo9kevioRks2VbcDGk8bASi5uz0yNA",
    "MM-bvPG6PYiUQ0-UrCM3GmEbFfBMTyPxjKk0vFXbXj0",
    "abhSgcd4RVrr2b-bzPtaJo6gbqfUrYKk4IqsrW7oCO0",
    "jvr27l0Kk-YFld45zoyTV-MMOIAXuIXtubma6eKOPL4",
    "uIkywAndqkm8YawR1KMkyw_GROmXd4h1wpCXwH-0oYM",
    "RzvDge9_4q7il_D_pgjgO-vg2cu7Njx-VGcV_0ZeRLA",
    "Rq32ku1EGlC0GbWIZx5RTtxP43JwU2dz1dIrPWElXRM",
    "DxKZRNW24FynvEKBMzBDT7B1AH1e0BZFU-Ixpm1LtoM",
    "FMXnrO0g9t3Yd2TMDS2I9bbNBhbVEZaz7V93GzBAvOE",
    "2ltd5-M692bro1FgeSC0N8MnEEYR_QTEcxJwurJj2ME",
    "WaZMsX0HrVmtxwAakiwEjDS56uQtQqztvwLnJrv-sHE",
    "IFEPd1kgPLE5sPYp8yeRI_45n3sVsZZ4h3Y0MLemuyg",
    "OcuYVKEgBuAf8Pj5oy22E_YKE_1TeNMYdK3wMRWhnKU",
    "kf6NclPPEjBozgM7tKI4l0uSWrN2xvEmU7LphuWuTOU",
    "cK0OqbPFiHuGC_mnzI3hnbhkfFCNwI095p6BJ7ArWDA",
    "Fg_7NYite2mD8-JHvLIZ3hLWQ05d5S_DTriL5oUEgac",
];

// A truthy cardState supplies the profile even if it has no profile/hash.
// A falsy hash falls back to the exact `!== false` feature flag; a truthy
// unlisted value selects all bishops, as in the source's strict includes.
fn miracle_selected_mode(state: &GameState) -> Result<bool> {
    let profile = match state.extra.get("cardState") {
        Some(card_state) if crate::observation::truth(Some(card_state)) => {
            card_state.get("profile")
        }
        _ => state.extra.get("profile"),
    };
    let hash = profile.and_then(|profile| profile.get("catalogHash"));
    if crate::observation::truth(hash) {
        Ok(hash
            .and_then(Value::as_str)
            .is_some_and(|hash| SELECTED_MIRACLE_HASHES.contains(&hash)))
    } else {
        Ok(state.extra.get("miracleSelectsBishop") != Some(&Value::Bool(false)))
    }
}

fn miracle_capture_targets(
    state: &GameState,
    selected: Option<Square>,
) -> Result<Vec<(Square, Piece)>> {
    miracle_selected_mode(state)?;
    let mut indices = BTreeMap::new();
    let mut victims = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let from = Square { row, col };
            if selected.is_some_and(|wanted| wanted != from) {
                continue;
            }
            let Some(bishop) = state.at(from) else {
                continue;
            };
            if bishop.color != state.turn || bishop.kind != "bishop" {
                continue;
            }
            for target in crate::movement::v7_legal_move_targets(
                state,
                bishop,
                from,
                crate::movement::V7MoveOptions::default(),
            )? {
                let square = target.square();
                let Some(victim) = state.at(square) else {
                    continue;
                };
                if victim.color != state.turn.opponent()
                    || !crate::movement::v7_can_capture_target(state, bishop, victim, false, false)?
                    || crate::movement::v7_encouraged_at(state, victim, square)
                {
                    continue;
                }
                if victim.id.is_empty() {
                    return Err(EngineError::UnsupportedFeature(
                        "miracle target without stable source identity".into(),
                    ));
                }
                // main104944: Map.set keeps the first insertion order but
                // replaces its value on a later hit of the same object. A
                // large body's last reachable cell therefore owns origin.
                if let Some(index) = indices.get(&victim.id).copied() {
                    victims[index] = (square, victim.clone());
                } else {
                    indices.insert(victim.id.clone(), victims.len());
                    victims.push((square, victim.clone()));
                }
            }
        }
    }
    Ok(victims)
}

fn miracle_bishop_choices(state: &GameState) -> Result<Vec<Square>> {
    let mut choices = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if state
                .at(square)
                .is_some_and(|piece| piece.color == state.turn && piece.kind == "bishop")
                && !miracle_capture_targets(state, Some(square))?.is_empty()
            {
                choices.push(square);
            }
        }
    }
    Ok(choices)
}

fn apply_miracle(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    let selected_mode = miracle_selected_mode(state)?;
    let target = if selected_mode {
        let target: Square =
            serde_json::from_value(action.target.clone().ok_or(EngineError::IllegalAction)?)
                .map_err(|_| EngineError::IllegalAction)?;
        if json!(target)
            != action
                .target
                .as_ref()
                .cloned()
                .ok_or(EngineError::IllegalAction)?
            || !miracle_bishop_choices(state)?.contains(&target)
        {
            return Err(EngineError::IllegalAction);
        }
        Some(target)
    } else {
        // Source's unselected profile ignores the input and acts on all
        // bishops. Public UI admission still owns its advertised choice shape.
        None
    };
    let victims = miracle_capture_targets(state, target)?;
    if victims.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let bishops = (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter_map(|square| {
            if target.is_some_and(|target| square != target) {
                return None;
            }
            state
                .at(square)
                .filter(|piece| piece.color == state.turn && piece.kind == "bishop")
                .cloned()
                .map(|piece| (square, piece))
        })
        .collect::<Vec<_>>();
    let actor = state.turn;
    let threat_probe = state.threat_probe_depth > 0;
    for (square, bishop) in &bishops {
        crate::card_effects::mark_vanish_animation(state, bishop, *square)?;
    }
    crate::replay::queue_visual(
        state,
        json!({
            "type":"board-change",
            "effect":"miracle",
            "color":state.turn,
            "removals":bishops.iter().map(|(square,piece)| json!({"square":square,"color":piece.color,"pieceType":piece.kind})).collect::<Vec<_>>(),
            "relocations":[],
            "transformations":[],
            "spawns":[],
        }),
    )?;
    for (square, former) in &victims {
        let mut converted = former.clone();
        converted.color = state.turn.into();
        converted.moved = true;
        // main:3792 resetConvertedLargePieceHealth changes only HP, preserving
        // the source identity, body cells, and anchor through the conversion.
        if let Some(hp) = match converted.kind.as_str() {
            "colossus" => Some(3),
            "bigRook" | "bigBishop" | "big-rook" | "big-bishop" => Some(2),
            _ => None,
        } {
            converted.extra.insert("hp".into(), json!(hp));
            converted.extra.insert("maxHp".into(), json!(hp));
        }
        crate::card_effects::mark_transformed_origin_with_options(
            state,
            &mut converted,
            *square,
            true,
        )?;
        converted.extra.shift_remove("freshNoCaptureUntil");
        converted
            .extra
            .insert("coolGuyCapturedLast".into(), json!(false));
        for cell in state.board.iter_mut().flatten() {
            if cell.as_ref().is_some_and(|piece| piece.id == former.id) {
                *cell = Some(converted.clone());
            }
        }
        crate::card_effects::mark_animation(state, &converted)?;
    }
    let enemy = actor.opponent();
    let mut captured = Vec::with_capacity(bishops.len());
    for (square, bishop) in &bishops {
        crate::transition::remove_piece_from_board_cells(state, bishop, *square)?;
        state.captures.get_mut(enemy).push(bishop.clone());
        crate::transition::record_new_card_capture_reactions(state, bishop, enemy, true)?;
        captured.push(bishop.clone());
    }
    if !bishops.is_empty() {
        crate::transition::cancel_prophecies_by_capture(state)?;
    }
    for (square, former) in &victims {
        crate::v7_threat::mark_king_threat_removal_cause(
            state,
            former,
            *square,
            &json!({"label":"기적"}),
            threat_probe,
        )?;
        crate::transition::resolve_royal_capture(state, former, actor)?;
    }
    crate::v7_board_hazards::resolve_reaper_nearby_deaths(
        state,
        &bishops
            .iter()
            .map(
                |(square, piece)| crate::v7_board_hazards::EnvironmentalRemoval {
                    piece: piece.clone(),
                    square: *square,
                    capture_owner: enemy,
                },
            )
            .collect::<Vec<_>>(),
    )?;
    Ok(captured)
}

fn reusable_active_cards<'a>(state: &'a GameState, joker: &CardSlot) -> Result<Vec<&'a CardSlot>> {
    let mut candidates = Vec::new();
    for candidate in state.deck_slots.get(state.turn) {
        if candidate.vacant
            || candidate.instance_id == joker.instance_id
            || matches!(candidate.effect.as_str(), "joker" | "bloodCard")
            || !candidate.used
            || candidate.recovering
            || crate::observation::truth(candidate.extra.get("passiveApplied"))
            || crate::observation::truth(candidate.extra.get("devCard"))
        {
            continue;
        }
        let definition = crate::card_registry::definition_for(RULES_VERSION_V7, &candidate.id)?;
        if definition.card_type == Some(crate::card_registry::CardType::Rule)
            || definition.activation != Some(crate::card_registry::CardActType::Active)
        {
            continue;
        }
        if candidate.instance_id.is_empty() {
            return Err(EngineError::InvalidState(
                "reusable card instance identity missing".into(),
            ));
        }
        candidates.push(candidate);
    }
    Ok(candidates)
}

fn apply_joker(state: &mut GameState, joker: &CardSlot, action: &Action) -> Result<()> {
    let target = action
        .target
        .as_ref()
        .and_then(Value::as_object)
        .ok_or(EngineError::IllegalAction)?;
    if target.len() != 1 {
        return Err(EngineError::IllegalAction);
    }
    let selected_id = target
        .get("cardInstanceId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(EngineError::IllegalAction)?;
    let candidates = reusable_active_cards(state, joker)?;
    if !candidates
        .iter()
        .any(|candidate| candidate.instance_id == selected_id)
    {
        return Err(EngineError::IllegalAction);
    }
    let selected = state
        .deck_slots
        .get_mut(state.turn)
        .iter_mut()
        .find(|candidate| candidate.instance_id == selected_id)
        .ok_or(EngineError::IllegalAction)?;
    selected.used = false;
    if selected.effect == "blackBox" {
        for field in [
            "boxRevealedCardId",
            "boxRevealedCardName",
            "randomRouletteResultType",
            "randomRouletteResult",
            "suspiciousPotionResultId",
        ] {
            selected.extra.shift_remove(field);
            selected.source_order.retain(|name| name != field);
        }
    }
    for field in ["usedAt", "recovering", "passiveApplied"] {
        selected.extra.shift_remove(field);
        selected.source_order.retain(|name| name != field);
    }
    selected.recovering = false;
    Ok(())
}

fn cleanup_sacrifice_first_choices(state: &GameState) -> Result<Vec<Square>> {
    let mut eligible_enemy_types = BTreeSet::new();
    for piece in state.board.iter().flatten().flatten() {
        if piece.color == state.turn.opponent() && !cleanup_sacrifice_excluded(state, piece) {
            eligible_enemy_types.insert(piece.kind.as_str());
        }
    }
    let mut seen = BTreeSet::new();
    let mut choices = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if let Some(piece) = state.at(square)
                && piece.color == state.turn
                && !cleanup_sacrifice_excluded(state, piece)
                && eligible_enemy_types.contains(piece.kind.as_str())
                && seen.insert(if piece.id.is_empty() {
                    format!("square:{row}:{col}")
                } else {
                    format!("piece:{}", piece.id)
                })
            {
                choices.push(square);
            }
        }
    }
    Ok(choices)
}

fn cleanup_sacrifice_excluded(state: &GameState, piece: &Piece) -> bool {
    state.royal_identity(piece)
        || matches!(
            piece.kind.as_str(),
            "merchant" | "timeTraveler" | "vampireLord" | "wall" | "football" | "blackHole"
        )
}

// main103506: unlike removeSacrificedPiece, the forced-exchange victim
// clears an attacker's initiative and records the captured movement types,
// while recycling remains a later resolveRoyalCapture responsibility.
fn remove_cleanup_victim(
    state: &mut GameState,
    square: Square,
    capturer: crate::Color,
) -> Result<Option<Piece>> {
    let origin = crate::transition::normalize_piece_square(state, square)?;
    crate::v7_quantum_state::observe_quantum_at_in_place(state, origin, Some(capturer))?;
    let Some(victim) = state.at(origin).cloned() else {
        return Ok(None);
    };
    crate::v7_capture_reactions::break_initiative_by_attack(state, &victim, capturer)?;
    if victim.color.owner().is_some() {
        crate::transition::add_capture_type(state, "capturedTypes", capturer, &victim.kind)?;
        crate::transition::add_capture_type(state, "turnCaptures", capturer, &victim.kind)?;
    }
    crate::transition::remove_piece_from_board_cells(state, &victim, origin)?;
    crate::transition::grant_vigilance_protection(state, &victim)?;
    crate::v7_board_hazards::resolve_reaper_nearby_deaths(
        state,
        &[crate::v7_board_hazards::EnvironmentalRemoval {
            piece: victim.clone(),
            square: origin,
            capture_owner: capturer,
        }],
    )?;
    crate::transition::cancel_prophecies_by_capture(state)?;
    state.captures.get_mut(capturer).push(victim.clone());
    crate::v7_rule_bombs::mark_deathmatch_progress(state)?;
    Ok(Some(victim))
}

// main67883-67903,103506-103548. The victim-removal and own-sacrifice
// callbacks remain separate because their memory and recycling order differ.
fn apply_cleanup_sacrifice(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    let selected: Square =
        serde_json::from_value(action.target.clone().ok_or(EngineError::IllegalAction)?)
            .map_err(|_| EngineError::IllegalAction)?;
    if json!(selected)
        != action
            .target
            .as_ref()
            .cloned()
            .ok_or(EngineError::IllegalAction)?
        || !cleanup_sacrifice_first_choices(state)?.contains(&selected)
    {
        return Err(EngineError::IllegalAction);
    }
    let own = state
        .at(selected)
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if let Some(piece) = state.at(square)
                && piece.color == state.turn.opponent()
                && piece.kind == own.kind
                && !cleanup_sacrifice_excluded(state, piece)
            {
                let identity = if piece.id.is_empty() {
                    format!("square:{row}:{col}")
                } else {
                    format!("piece:{}", piece.id)
                };
                if seen.insert(identity) {
                    candidates.push((square, piece.clone()));
                }
            }
        }
    }
    if candidates.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let draw = state.rng.sample()?;
    if !draw.is_finite() || !(0.0..1.0).contains(&draw) {
        return Err(EngineError::InvalidState(
            "cleanup-sacrifice victim random index outside [0,1)".into(),
        ));
    }
    let (victim_square, victim) =
        candidates[(draw * candidates.len() as f64).floor() as usize].clone();
    state.rng.record_last_probability(
        1.0 / candidates.len() as f64,
        "source Cleanup sacrifice victim",
    )?;
    crate::card_effects::mark_vanish_animation(state, &own, selected)?;
    crate::card_effects::mark_vanish_animation(state, &victim, victim_square)?;
    let actor = state.turn;
    let removed =
        remove_cleanup_victim(state, victim_square, actor)?.ok_or(EngineError::IllegalAction)?;
    let sacrificed = crate::transition::sacrifice(state, selected, actor.opponent())?
        .ok_or(EngineError::IllegalAction)?;
    crate::transition::resolve_royal_capture(state, &removed, actor)?;
    if state.mode != "gameover" {
        crate::transition::resolve_royal_capture(state, &sacrificed, actor.opponent())?;
    }
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(vec![removed, sacrificed])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, GameConfig};

    fn first_difference(actual: &Value, expected: &Value, path: &str) -> Option<String> {
        if actual == expected {
            return None;
        }
        match (actual, expected) {
            (Value::Object(actual), Value::Object(expected)) => {
                for key in actual.keys().chain(expected.keys()) {
                    match (actual.get(key), expected.get(key)) {
                        (Some(left), Some(right)) => {
                            if let Some(path) =
                                first_difference(left, right, &format!("{path}.{key}"))
                            {
                                return Some(path);
                            }
                        }
                        _ => return Some(format!("{path}.{key}")),
                    }
                }
                None
            }
            (Value::Array(actual), Value::Array(expected)) => {
                for index in 0..actual.len().max(expected.len()) {
                    match (actual.get(index), expected.get(index)) {
                        (Some(left), Some(right)) => {
                            if let Some(path) =
                                first_difference(left, right, &format!("{path}[{index}]"))
                            {
                                return Some(path);
                            }
                        }
                        _ => return Some(format!("{path}[{index}]")),
                    }
                }
                None
            }
            _ => Some(path.to_owned()),
        }
    }

    fn source_card(id: &str, instance_id: &str) -> CardSlot {
        let mut raw = crate::card_registry::definition_for(RULES_VERSION_V7, id)
            .unwrap()
            .source_definition
            .clone();
        raw["instanceId"] = json!(instance_id);
        serde_json::from_value(raw).unwrap()
    }

    fn assert_source_box_receipt(variable: &str, id: &str) {
        let Some(path) = std::env::var_os(variable) else {
            return;
        };
        let receipt: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            receipt["sourceSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        let case = &receipt["case"];
        assert_eq!(case["result"]["ok"], true);
        let before = crate::v7_host::V7HostPosition::from_envelope(case["before"].clone()).unwrap();
        let actor = before.state().turn;
        let slot = before
            .state()
            .deck_slots
            .get(actor)
            .iter()
            .position(|card| card.id == id)
            .unwrap();
        let card = before.state().deck_slots.get(actor)[slot].clone();
        let (after, ()) = before
            .transact(before.position_id(), |state| {
                if id == "white-box" {
                    assert!(apply_passive_acquisition(state, actor, slot)?);
                } else {
                    let action = Action::card(actor, &card, None);
                    apply(state, &card, &action)?;
                }
                Ok(())
            })
            .unwrap();
        let actual = after.export_envelope().unwrap();
        assert_eq!(
            first_difference(&actual, &case["after"], "position"),
            None,
            "{id} full source state, RNG, history, and identity diverged"
        );
    }

    #[test]
    fn white_box_source_acquisition_state_rng_history_and_identity() {
        assert_source_box_receipt("ACCELERATE_V7_WHITE_BOX_SOURCE_RECEIPT", "white-box");
    }

    #[test]
    fn black_box_source_nested_effect_state_rng_history_and_identity() {
        assert_source_box_receipt("ACCELERATE_V7_BLACK_BOX_SOURCE_RECEIPT", "black-box");
    }

    #[test]
    fn source_declined_virtual_trolley_keeps_shuffle_rng_and_public_play_is_atomic() {
        // main68162 and104725: two 9-point queens form no allowed low-score
        // bundle (2..8), but the candidate shuffle consumes one draw before
        // hasTrolleyCandidate returns false. Box retry must keep that draw.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        state.board = vec![vec![None; 8]; 8];
        state.turn = Color::White;
        state.board[0][0] = Some(Piece::new("queen", Color::Black, "black-queen-a"));
        state.board[0][1] = Some(Piece::new("queen", Color::Black, "black-queen-b"));
        let public_card = source_card("trolley", "trolley-1");
        state.deck_slots.white = vec![public_card.clone()];
        let before = state.clone();
        let public_action = Action::card(Color::White, &public_card, None);
        assert!(matches!(
            apply(&mut state, &public_card, &public_action),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(state, before);

        let mut source_ai_probe = before.clone();
        let result = crate::v7_card_context::with_ai_simulation(&mut source_ai_probe, |state| {
            apply(state, &public_card, &public_action)
        });
        assert!(matches!(result, Err(EngineError::IllegalAction)));
        assert_eq!(source_ai_probe.rng.cursor, before.rng.cursor + 1);
        assert!(!source_ai_probe.is_ai_simulation());
        assert_eq!(source_ai_probe.deck_slots, before.deck_slots);
        assert_eq!(source_ai_probe.board, before.board);

        let mut virtual_card = public_card.clone();
        virtual_card.extra.insert("devCard".into(), json!(true));
        let virtual_action = Action::card(Color::White, &virtual_card, None);
        assert!(matches!(
            apply_virtual_effect(&mut state, &mut virtual_card, &virtual_action),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(state.rng.cursor, before.rng.cursor + 1);
        assert_eq!(state.board, before.board);
        assert_eq!(state.captures, before.captures);
        assert_eq!(state.history, before.history);
        assert_eq!(
            state.extra.get("pendingTrolley"),
            before.extra.get("pendingTrolley")
        );

        let attempted = state.clone();
        let mut invalid_action = virtual_action;
        invalid_action.color = Color::Black;
        assert!(matches!(
            apply_virtual_effect(&mut state, &mut virtual_card, &invalid_action),
            Err(EngineError::InvalidState(_))
        ));
        assert_eq!(state, attempted);
    }

    #[test]
    fn miracle_profile_gate_preserves_source_hash_and_card_state_precedence() {
        // main323,403,517: the September26 catalog hash belongs to the
        // selected-bishop allow-list and overrides the false fallback flag.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        state.extra.shift_remove("cardState");
        state
            .extra
            .insert("miracleSelectsBishop".into(), json!(false));
        state.extra.insert(
            "profile".into(),
            json!({
                "catalogHash":"yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
            }),
        );
        assert!(miracle_selected_mode(&state).unwrap());
        state.extra.insert("cardState".into(), json!({}));
        assert!(!miracle_selected_mode(&state).unwrap());
        state.extra.insert("cardState".into(), Value::Null);
        assert!(miracle_selected_mode(&state).unwrap());
        state
            .extra
            .insert("profile".into(), json!({"catalogHash":""}));
        assert!(!miracle_selected_mode(&state).unwrap());
        state
            .extra
            .insert("miracleSelectsBishop".into(), json!(true));
        state.extra.insert(
            "profile".into(),
            json!({"catalogHash":"source-unlisted-profile"}),
        );
        assert!(!miracle_selected_mode(&state).unwrap());
    }

    #[test]
    fn exact_source_choice_id_and_effect_set_is_pinned() {
        assert_eq!(OBJECTS.len(), 12);
        assert_eq!(
            OBJECTS.iter().map(|object| object.id).collect::<Vec<_>>(),
            IDS
        );
        let catalog = crate::card_registry::registry_for(RULES_VERSION_V7).unwrap();
        for object in OBJECTS {
            assert_eq!(catalog.get(object.id).unwrap().effect, object.effect);
        }
    }

    #[test]
    fn barricade_enumerates_direction_then_center_and_installs_source_shaped_walls() {
        // Frozen enumeration: completeCardTargets() loops horizontal before
        // vertical, then centers in row-major order. The effect installs
        // source objects with no `moved` property (main:105153-162).
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        state.board = vec![vec![None; 8]; 8];
        state.turn = Color::White;
        let card = source_card("barricade", "barricade-1");
        state.deck_slots.white = vec![card.clone()];
        assert_eq!(
            first_choices(&state, &card).unwrap(),
            vec![
                FirstChoice::Direction("horizontal"),
                FirstChoice::Direction("vertical"),
            ]
        );
        let actions = bounded_actions(&state, &card).unwrap();
        assert_eq!(actions.len(), 96);
        assert_eq!(
            actions[0].target,
            Some(json!({"row":0,"col":1,"direction":"horizontal"}))
        );
        assert_eq!(
            actions[47].target,
            Some(json!({"row":7,"col":6,"direction":"horizontal"}))
        );
        assert_eq!(
            actions[48].target,
            Some(json!({"row":1,"col":0,"direction":"vertical"}))
        );
        let before = state.clone();
        let selected = actions
            .iter()
            .find(|action| action.target == Some(json!({"row":3,"col":3,"direction":"vertical"})))
            .unwrap();
        assert!(apply(&mut state, &card, selected).unwrap().is_empty());
        assert_eq!(state.rng, before.rng);
        for row in 2..=4 {
            assert_eq!(
                serde_json::to_value(state.at(Square { row, col: 3 }).unwrap()).unwrap(),
                json!({"type":"wall","color":"neutral","id":format!("wall-{row}-3")})
            );
        }
    }

    #[test]
    fn barricade_reservation_is_atomic_and_concealed_crushing_skips_capture_memory() {
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        state.board = vec![vec![None; 8]; 8];
        state.turn = Color::White;
        let card = source_card("barricade", "barricade-1");
        state.deck_slots.white = vec![card.clone()];
        let target = Action::card(
            Color::White,
            &card,
            Some(json!({"row":3,"col":3,"direction":"horizontal"})),
        );
        state.extra.insert(
            "pendingPortals".into(),
            json!([{"cells":[{"row":3,"col":3}]}]),
        );
        let reserved = state.clone();
        assert!(matches!(
            apply(&mut state, &card, &target),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(state, reserved);
        state.extra.insert("pendingPortals".into(), json!([]));
        let mut hidden = Piece::new("pawn", Color::Black, "hidden-pawn");
        hidden.extra.insert("hiddenFrom".into(), json!("white"));
        state.board[3][3] = Some(hidden);
        assert!(
            bounded_actions(&state, &card)
                .unwrap()
                .iter()
                .any(|action| action.target == target.target)
        );
        let before = state.clone();
        assert!(apply(&mut state, &card, &target).unwrap().is_empty());
        assert_eq!(state.rng, before.rng);
        assert_eq!(state.captures, before.captures);
        for field in [
            "capturedTypes",
            "turnCaptures",
            "prophecy",
            "mediumMovement",
        ] {
            assert_eq!(state.extra.get(field), before.extra.get(field), "{field}");
        }
        for col in 2..=4 {
            assert_eq!(state.board[3][col].as_ref().unwrap().kind, "wall");
        }

        let mut ai_probe = before.clone();
        let raw_target = Action::card(Color::White, &card, Some(json!({"row":3,"col":3})));
        assert!(matches!(
            apply(&mut ai_probe, &card, &raw_target),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(ai_probe, before);
        crate::v7_card_context::with_ai_simulation(&mut ai_probe, |state| {
            apply(state, &card, &raw_target)
        })
        .unwrap();
        assert_eq!(ai_probe, state);
    }

    #[test]
    fn frozen_choice_effect_state_rng_and_history_when_receipt_is_supplied() {
        // Generate this receipt from the source-pinned offline runtime, never
        // commit its full game states. It compares the direct effect boundary,
        // before common card-use and turn settlement.
        let Some(path) = std::env::var_os("ACCELERATE_V7_CHOICE_SOURCE_RECEIPT") else {
            return;
        };
        let receipt: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            receipt["sourceSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        let cases = receipt["cases"].as_array().expect("source choice cases");
        assert_eq!(cases.len(), 2);
        for case in cases {
            let id = case["cardId"].as_str().expect("source card ID");
            assert!(matches!(id, "barricade" | "miracle"));
            assert_eq!(case["result"]["ok"], true, "source {id} effect accepted");
            let mut state: GameState =
                serde_json::from_value(case["before"]["state"].clone()).unwrap();
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(case["before"]["rng"].clone()).unwrap();
            let card = source_card(id, &format!("source-{id}-instance"));
            let action = Action::card(state.turn, &card, Some(case["target"].clone()));
            let history = state.history.clone();
            apply(&mut state, &card, &action).unwrap();
            let mut actual = serde_json::to_value(&state).unwrap();
            for envelope in ["rulesetId", "rng", "history"] {
                actual.as_object_mut().unwrap().remove(envelope);
            }
            let expected = &case["after"]["state"];
            assert_eq!(
                first_difference(&actual, expected, "state"),
                None,
                "{id} full source state diverged"
            );
            assert_eq!(
                serde_json::to_value(&state.rng).unwrap(),
                case["after"]["rng"],
                "{id} source RNG diverged"
            );
            assert_eq!(state.history, history, "{id} direct effect changed history");
        }
    }

    #[test]
    fn joker_selects_used_active_instance_in_hand_order_and_preserves_rng() {
        // main-OahWs0tU.js:67936-67940 and 104159-104174. The used freeze
        // instance is offered; a used passive, another joker, and an unused
        // active card are omitted. Applying Joker changes only that instance.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        let joker = source_card("joker", "joker-1");
        let mut freeze = source_card("freeze", "freeze-1");
        freeze.used = true;
        freeze.source_order.push("used".into());
        freeze.extra.insert("usedAt".into(), json!(123));
        freeze.source_order.push("usedAt".into());
        let mut second_joker = source_card("joker", "joker-2");
        second_joker.used = true;
        let unused = source_card("taboo", "taboo-1");
        let mut passive = source_card("ghost", "ghost-1");
        passive.used = true;
        state.deck_slots.white = vec![joker.clone(), freeze, second_joker, unused, passive];
        state.turn = Color::White;
        let before = state.clone();
        let actions = bounded_actions(&state, &joker).unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].target,
            Some(json!({"cardInstanceId":"freeze-1"}))
        );
        assert_eq!(
            first_choices(&state, &joker).unwrap(),
            vec![FirstChoice::CardInstance("freeze-1".into())]
        );
        assert!(apply(&mut state, &joker, &actions[0]).unwrap().is_empty());
        assert!(!state.deck_slots.white[1].used);
        assert!(!state.deck_slots.white[1].extra.contains_key("usedAt"));
        assert_eq!(state.rng, before.rng);
        let mut expected = before;
        expected.deck_slots.white[1] = state.deck_slots.white[1].clone();
        assert_eq!(state, expected);
    }

    #[test]
    fn rejected_joker_choice_does_not_mutate_state() {
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        let joker = source_card("joker", "joker-1");
        state.deck_slots.white = vec![joker.clone()];
        state.turn = Color::White;
        let before = state.clone();
        let action = Action::card(
            Color::White,
            &joker,
            Some(json!({"cardInstanceId":"missing"})),
        );
        assert!(matches!(
            apply(&mut state, &joker, &action),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn rule_ticket_candidates_keep_frozen_korean_order_and_reserve_one_exact_rule() {
        // main:50031,66221-23,66323-46,69498-509,73404-29.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        let ticket = source_card("rule-ticket", "ticket-1");
        state.deck_slots.white = vec![ticket.clone()];
        state.turn = Color::White;
        let all = rule_ticket_candidates(&state).unwrap();
        assert_eq!(SOURCE_RULE_ORDER.len(), 27);
        assert_eq!(all.len(), 23);
        assert_eq!(all[0], "acceleration");
        assert_eq!(all[1], "winter-kingdom");
        assert!(!all.contains(&"chess-960"));
        assert!(!all.contains(&"diagonal-chess"));
        let actions = bounded_actions(&state, &ticket).unwrap();
        assert_eq!(actions.len(), all.len());
        assert_eq!(actions[0].target, Some(json!({"ruleId":"acceleration"})));
        let before = state.clone();
        apply(&mut state, &ticket, &actions[0]).unwrap();
        let definition = crate::card_registry::definition_for(RULES_VERSION_V7, all[0]).unwrap();
        let source_ai_action = Action::card(
            Color::White,
            &ticket,
            Some(json!({
                "ruleId":all[0],
                "ruleEffect":definition.effect,
                "ruleStars":super::super::js_number(definition.source_definition.get("stars"), 0)
                    .unwrap_or(0.0),
            })),
        );
        let mut source_ai_probe = before.clone();
        apply(&mut source_ai_probe, &ticket, &source_ai_action).unwrap();
        assert_eq!(source_ai_probe, state);
        let entry = &state.extra["pendingRuleTickets"][0];
        assert_eq!(entry["ruleId"], "acceleration");
        assert_eq!(entry["color"], "white");
        assert_eq!(entry["startTurnCount"], json!(before.turns_taken.white));
        assert_eq!(state.rng.cursor, before.rng.cursor + 1);
        assert_eq!(rule_ticket_candidates(&state).unwrap().len(), all.len() - 1);

        let mut conflict = before.clone();
        let mut reverse = source_card("reverse-pawns", "reverse-opponent");
        reverse.used = true;
        conflict.deck_slots.black = vec![reverse];
        assert!(
            !rule_ticket_candidates(&conflict)
                .unwrap()
                .contains(&"macho-chess")
        );
        let macho = Action::card(Color::White, &ticket, Some(json!({"ruleId":"macho-chess"})));
        let unchanged = conflict.clone();
        assert!(matches!(
            apply(&mut conflict, &ticket, &macho),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(conflict, unchanged);
        conflict
            .extra
            .insert("september18Balance".into(), json!(false));
        assert!(
            rule_ticket_candidates(&conflict)
                .unwrap()
                .contains(&"macho-chess")
        );
    }

    #[test]
    fn rule_ticket_rejects_repeated_or_malformed_target_without_mutation() {
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        let ticket = source_card("rule-ticket", "ticket-1");
        state.deck_slots.white = vec![ticket.clone()];
        state.turn = Color::White;
        let valid = Action::card(
            Color::White,
            &ticket,
            Some(json!({"ruleId":"acceleration"})),
        );
        apply(&mut state, &ticket, &valid).unwrap();
        for target in [
            json!({"ruleId":"acceleration"}),
            json!({"ruleId":"chess-960"}),
            json!({"ruleId":"winter-kingdom","extra":true}),
            json!({"ruleId":1}),
            json!({"ruleId":"winter-kingdom","ruleEffect":"unknown","ruleStars":0}),
            json!({"ruleId":"winter-kingdom","ruleEffect":"winterKingdom","ruleStars":999}),
        ] {
            let before = state.clone();
            let action = Action::card(Color::White, &ticket, Some(target));
            assert!(matches!(
                apply(&mut state, &ticket, &action),
                Err(EngineError::IllegalAction)
            ));
            assert_eq!(state, before);
        }
    }

    #[test]
    fn trolley_availability_probe_does_not_consume_live_rng() {
        // main:68176-68243 and 104723-34. The source availability scan
        // shuffles, then scheduling draws a pending identity.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        let trolley = source_card("trolley", "trolley-1");
        state.deck_slots.white = vec![trolley.clone()];
        state.turn = Color::White;
        let before = state.clone();
        let choices = bounded_actions(&state, &trolley).unwrap();
        assert_eq!(state, before);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].target, None);
        apply(&mut state, &trolley, &choices[0]).unwrap();
        assert_eq!(state.extra["pendingTrolley"][0]["color"], "black");
        assert_eq!(state.extra["pendingTrolley"][0]["by"], "white");
        assert!(state.rng.cursor > before.rng.cursor + 1);
    }

    #[test]
    fn current_v7_miracle_selects_one_bishop_and_preserves_unselected_bishop() {
        // Frozen new-game state has miracleSelectsBishop=true and no profile
        // hash, so the source sacrifices only the chosen bishop (main:517-19,
        // 104949-55). The target surface still requires a legal capture.
        // The exact frozen Miracle receipt uses this board and null initiative
        // entries; a null entry must not suppress the adjacent rook capture.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        state.board = vec![vec![None; 8]; 8];
        let bishop = Square { row: 4, col: 3 };
        let idle_bishop = Square { row: 7, col: 0 };
        let victim = Square { row: 3, col: 4 };
        state.board[bishop.row as usize][bishop.col as usize] =
            Some(Piece::new("bishop", Color::White, "white-bishop-1"));
        state.board[idle_bishop.row as usize][idle_bishop.col as usize] =
            Some(Piece::new("bishop", Color::White, "white-bishop-2"));
        state.board[victim.row as usize][victim.col as usize] =
            Some(Piece::new("rook", Color::Black, "black-rook-1"));
        state.board[7][7] = Some(Piece::new("king", Color::White, "white-king-1"));
        state.board[0][7] = Some(Piece::new("king", Color::Black, "black-king-1"));
        let card = source_card("miracle", "miracle-1");
        state.deck_slots.white = vec![card.clone()];
        state.turn = Color::White;
        assert_eq!(state.extra.get("miracleSelectsBishop"), Some(&json!(true)));
        assert_eq!(
            miracle_bishop_choices(&state).unwrap(),
            vec![bishop],
            "source bishop moves={:?}, capture={:?}, encouraged={}",
            crate::movement::v7_legal_move_targets(
                &state,
                state.at(bishop).unwrap(),
                bishop,
                crate::movement::V7MoveOptions::default()
            ),
            crate::movement::v7_can_capture_target(
                &state,
                state.at(bishop).unwrap(),
                state.at(victim).unwrap(),
                false,
                false
            ),
            crate::movement::v7_encouraged_at(&state, state.at(victim).unwrap(), victim)
        );
        let actions = bounded_actions(&state, &card).unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].target, Some(json!(bishop)));
        let before = state.clone();
        let mut wrong = Action::card(Color::White, &card, Some(json!(idle_bishop)));
        assert!(matches!(
            apply(&mut state, &card, &wrong),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(state, before);
        wrong.target = Some(json!(bishop));
        let captured = apply(&mut state, &card, &wrong).unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(state.at(bishop), None);
        assert!(state.at(idle_bishop).is_some());
        let converted = state.at(victim).unwrap();
        assert_eq!(converted.color, Color::White);
        assert_eq!(converted.kind, "rook");
        assert_eq!(converted.extra.get("origin"), Some(&json!("e5")));
        assert_eq!(
            converted.extra.get("coolGuyCapturedLast"),
            Some(&json!(false))
        );
        assert_eq!(state.captures.black.len(), 1);
        assert_eq!(state.rng, before.rng);

        let mut all_bishops = before.clone();
        all_bishops
            .extra
            .insert("miracleSelectsBishop".into(), json!(false));
        let captured = apply(&mut all_bishops, &card, &wrong).unwrap();
        assert_eq!(captured.len(), 2);
        assert!(all_bishops.at(idle_bishop).is_none());
    }

    #[test]
    fn cleanup_sacrifice_records_exact_enemy_capture_and_own_sacrifice() {
        // main:103506-548. One enemy candidate still consumes one random
        // draw, and the enemy type enters both source Set ledgers.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        state.board = vec![vec![None; 8]; 8];
        let own = Square { row: 6, col: 1 };
        let enemy = Square { row: 1, col: 1 };
        state.board[own.row as usize][own.col as usize] =
            Some(Piece::new("pawn", Color::White, "white-pawn-1"));
        state.board[enemy.row as usize][enemy.col as usize] =
            Some(Piece::new("pawn", Color::Black, "black-pawn-1"));
        state.board[7][7] = Some(Piece::new("king", Color::White, "white-king-1"));
        state.board[0][7] = Some(Piece::new("king", Color::Black, "black-king-1"));
        let card = source_card("cleanup-sacrifice", "cleanup-sacrifice-1");
        state.deck_slots.white = vec![card.clone()];
        state.turn = Color::White;
        let actions = bounded_actions(&state, &card).unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].target, Some(json!(own)));
        let before = state.clone();
        let captured = apply(&mut state, &card, &actions[0]).unwrap();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].id, "black-pawn-1");
        assert_eq!(captured[1].id, "white-pawn-1");
        assert!(state.at(own).is_none());
        assert!(state.at(enemy).is_none());
        assert_eq!(state.captures.white[0].id, "black-pawn-1");
        assert_eq!(state.captures.black[0].id, "white-pawn-1");
        assert_eq!(
            state.extra["capturedTypes"]["white"]["values"],
            json!(["pawn"])
        );
        assert_eq!(
            state.extra["turnCaptures"]["white"]["values"],
            json!(["pawn"])
        );
        assert_eq!(state.rng.cursor, before.rng.cursor + 1);
    }
}
