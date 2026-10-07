//! Public-only, source-conditioned v7 particle proposals.
//!
//! This boundary never receives the hidden source game or its seed. A proposal
//! owns an independent RNG stream, retains the full v7 event history, and is
//! accepted only when its complete viewer projection matches the signed frame.

use crate::v7_action_admission::AdmittedV7Action;
use crate::v7_adapter_actions::{self, AppliedV7Action, V7ActionHostResult};
use crate::{
    ActionKind, Color, EngineError, GameConfig, GameState, Observation, PieceColor,
    RULES_VERSION_V7, Result, RngState, V7HostPosition,
};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct V7HiddenDraftProposal {
    pub position: V7HostPosition,
    pub importance_weight: f64,
    pub source_probability: f64,
    pub proposal_probability: f64,
}

#[derive(Debug)]
pub struct V7ConditionedStepProposal {
    pub applied: AppliedV7Action,
    pub importance_weight: f64,
    pub source_probability: f64,
    pub proposal_probability: f64,
}

fn same_content<T: serde::Serialize + ?Sized>(left: &T, right: &T) -> Result<bool> {
    let bytes = |value| {
        serde_jcs::to_vec(value).map_err(|error| {
            EngineError::Serialization(format!("public canonicalization: {error}"))
        })
    };
    Ok(bytes(left)? == bytes(right)?)
}

/// 불일치한 공개 필드의 경로만 반환한다. 원문 오류를 보강하기 위한
/// 진단이며 실제 상태·RNG·관측 값은 반환하지 않는다. 공개 DTO의 두
/// object 계층만 검사하므로 별도의 재귀나 무한 순회가 없다.
fn differing_public_field(left: &Observation, right: &Observation) -> Result<String> {
    let left = serde_json::to_value(left).map_err(EngineError::serialization)?;
    let right = serde_json::to_value(right).map_err(EngineError::serialization)?;
    let differs = |left: Option<&Value>, right: Option<&Value>| -> Result<bool> {
        match (left, right) {
            (Some(left), Some(right)) => Ok(!same_content(left, right)?),
            (None, None) => Ok(false),
            _ => Ok(true),
        }
    };
    let left_fields = left.as_object().ok_or(EngineError::IllegalAction)?;
    let right_fields = right.as_object().ok_or(EngineError::IllegalAction)?;
    for field in left_fields.keys().chain(right_fields.keys()) {
        if !differs(left.get(field), right.get(field))? {
            continue;
        }
        if field == "publicState"
            && let (Some(left), Some(right)) = (
                left.get(field).and_then(Value::as_object),
                right.get(field).and_then(Value::as_object),
            )
        {
            for nested in left.keys().chain(right.keys()) {
                if differs(left.get(nested), right.get(nested))? {
                    return Ok(format!("$.publicState.{nested}"));
                }
            }
        }
        return Ok(format!("$.{field}"));
    }
    Ok("$".into())
}

fn checked_observation(value: Value) -> Result<Observation> {
    crate::state::validate_json_value(&value, 0)?;
    let object = value.as_object().ok_or_else(|| {
        EngineError::InvalidState("v7 public observation must be an object".into())
    })?;
    const FIELDS: [&str; 9] = [
        "protocolVersion",
        "viewer",
        "turn",
        "board",
        "ownCards",
        "opponentHandCount",
        "publicState",
        "history",
        "informationStateKey",
    ];
    if object.len() != FIELDS.len() || FIELDS.iter().any(|field| !object.contains_key(*field)) {
        return Err(EngineError::InvalidState(
            "v7 public observation has unknown or missing fields".into(),
        ));
    }
    let mut observation: Observation =
        serde_json::from_value(value).map_err(EngineError::serialization)?;
    crate::observation::validate_projection_for_ruleset(&observation, RULES_VERSION_V7)?;
    let supplied = observation.information_state_key.clone();
    observation.refresh_key();
    if observation.information_state_key != supplied {
        return Err(EngineError::InvalidState(
            "v7 public observation informationStateKey mismatch".into(),
        ));
    }
    Ok(observation)
}

fn checked_density(p: f64, q: f64) -> Result<f64> {
    if !p.is_finite() || p <= 0.0 || p > 1.0 {
        return Err(EngineError::InvalidState(format!(
            "v7 source trace density p must be finite in (0, 1], got {p}"
        )));
    }
    if !q.is_finite() || q <= 0.0 || q > 1.0 {
        return Err(EngineError::InvalidState(format!(
            "v7 proposal trace density q must be finite in (0, 1], got {q}"
        )));
    }
    let weight = p / q;
    if !weight.is_finite() || weight <= 0.0 {
        return Err(EngineError::InvalidState(format!(
            "v7 trace importance ratio p/q is not finite and positive (p={p}, q={q})"
        )));
    }
    Ok(weight)
}

fn require_standard_opening_board(state: &GameState) -> Result<()> {
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::UnsupportedFeature(
            "v7 opening density on a resized board".into(),
        ));
    }
    for row in 0..8 {
        for col in 0..8 {
            let kind = match row {
                0 | 7 => Some(
                    [
                        "rook", "knight", "bishop", "queen", "king", "bishop", "knight", "rook",
                    ][col],
                ),
                1 | 6 => Some("pawn"),
                _ => None,
            };
            match (state.board[row][col].as_ref(), kind) {
                (None, None) => {}
                (Some(piece), Some(expected))
                    if piece.kind == expected
                        && piece.color
                            == if row < 2 {
                                PieceColor::Black
                            } else {
                                PieceColor::White
                            }
                        && !piece.moved => {}
                _ => {
                    return Err(EngineError::UnsupportedFeature(
                        "v7 opening density on a modified board".into(),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Generate a private v7 initial state from an independent seed and bind only
/// the offer that this viewer could see. No source private seed or opponent
/// choice enters this function.
pub fn sample_initial_public(
    config: GameConfig,
    public: Value,
    seed: u32,
) -> Result<V7HostPosition> {
    let expected = checked_observation(public)?;
    if !expected.history.is_empty()
        || !expected.own_cards.is_empty()
        || expected.opponent_hand_count != 0
        || expected
            .public_state
            .get("revealedOpponentCards")
            .and_then(Value::as_array)
            .is_none_or(|cards| !cards.is_empty())
    {
        return Err(EngineError::InvalidState(
            "v7 initial conditioning requires a frame before card acquisition".into(),
        ));
    }
    let mut state = crate::v7_new_game::new_game(config.clone(), u64::from(seed))?;
    if !config.draft_delete {
        if config.game_style == "grand" {
            let choices = expected
                .public_state
                .get("draft")
                .and_then(|draft| draft.get("choices"))
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    EngineError::InvalidState("v7 initial grand pool is missing".into())
                })?;
            crate::draft::condition_grand_initial_choices(&mut state, choices)?;
        } else if let Some(draft) = expected.public_state.get("draft") {
            if draft.get("color").and_then(Value::as_str) != Some(expected.viewer.as_str()) {
                return Err(EngineError::InvalidState(
                    "v7 opposing initial draft cannot be supplied as public".into(),
                ));
            }
            let choices = draft
                .get("choices")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    EngineError::InvalidState("v7 initial draft choices are missing".into())
                })?;
            crate::draft::condition_initial_offer(&mut state, choices)?;
        } else if expected.viewer == state.decision_actor() {
            return Err(EngineError::InvalidState(
                "v7 acting side's initial draft must be public".into(),
            ));
        }
    }
    let position = V7HostPosition::from_state(state)?;
    if !same_content(&position.state().try_observe(expected.viewer)?, &expected)? {
        return Err(EngineError::ConditioningMismatch(
            "v7 initial public frame is impossible under the supplied source configuration".into(),
        ));
    }
    Ok(position)
}

fn cards_for_actor(observation: &Observation, actor: Color) -> Result<&[Value]> {
    if observation.viewer == actor {
        Ok(&observation.own_cards)
    } else {
        observation
            .public_state
            .get("revealedOpponentCards")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .ok_or_else(|| EngineError::InvalidState("v7 public opposing cards are missing".into()))
    }
}

/// Tilt only a still-private initial OPENING offer toward the subsequently
/// observed acquisition. The complete prior viewer frame must remain equal.
pub fn condition_hidden_opening_draft(
    position: &V7HostPosition,
    expected_next_public: Value,
    independent_seed: u32,
) -> Result<V7HiddenDraftProposal> {
    let expected = checked_observation(expected_next_public)?;
    let state = position.state();
    let actor = state.decision_actor();
    let draft = state.extra.get("draft").ok_or(EngineError::IllegalAction)?;
    let chaos = state.extra.get("gameStyle").and_then(Value::as_str) == Some("chaos");
    if state.mode != "draft"
        || draft["phase"] != "OPENING"
        || draft["kind"] == "grand"
        || expected.viewer != actor.opponent()
        || state.move_count != 0
        || state.turns_taken.white != 0
        || state.turns_taken.black != 0
        || state.deck_slots.get(actor).iter().any(|card| !card.vacant)
        || if actor == Color::White {
            !state.history.is_empty() || state.deck_slots.black.iter().any(|card| !card.vacant)
        } else {
            state.history.len() != 1
                || state
                    .deck_slots
                    .white
                    .iter()
                    .filter(|card| !card.vacant)
                    .count()
                    != if chaos { 2 } else { 1 }
        }
    {
        return Err(EngineError::UnsupportedFeature(
            "v7 hidden offer conditioning requires an initial OPENING decision".into(),
        ));
    }
    require_standard_opening_board(state)?;
    let before = state.try_observe(expected.viewer)?;
    if before.public_state.contains_key("draft") {
        return Err(EngineError::ConditioningMismatch(
            "already-public v7 draft may not be proposed again".into(),
        ));
    }
    let acquired = cards_for_actor(&expected, actor)?;
    if acquired.len() != if chaos { 2 } else { 1 }
        || acquired
            .iter()
            .enumerate()
            .any(|(slot, card)| crate::observation::number(card.get("slot")) != Some(slot as f64))
    {
        return Err(EngineError::ConditioningMismatch(
            "v7 initial acquisition violates source slot count or order".into(),
        ));
    }
    let required = acquired
        .iter()
        .map(|card| {
            card.get("id").and_then(Value::as_str).ok_or_else(|| {
                EngineError::InvalidState("v7 acquired card definition is missing".into())
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let (proposed, (source_probability, proposal_probability)) =
        position.transact(position.position_id(), |working| {
            working.rng = RngState::seeded(u64::from(independent_seed));
            if chaos {
                crate::draft::propose_hidden_chaos_offer(working, &required, actor)
            } else {
                crate::draft::propose_hidden_normal_offer(working, required[0], actor)
            }
        })?;
    let importance_weight = checked_density(source_probability, proposal_probability)?;
    if !same_content(&proposed.state().try_observe(expected.viewer)?, &before)? {
        return Err(EngineError::ConditioningMismatch(
            "v7 hidden proposal changed previously public information".into(),
        ));
    }
    Ok(V7HiddenDraftProposal {
        position: proposed,
        importance_weight,
        source_probability,
        proposal_probability,
    })
}

/// Tilt an unobserved MIDDLE/END offer toward the one newly revealed
/// acquisition. The previous opposing projection is immutable; source pool and
/// balancing-trace p/q remain owned by the draft engine.
pub fn condition_hidden_stage_draft(
    position: &V7HostPosition,
    expected_next_public: Value,
    independent_seed: u32,
) -> Result<V7HiddenDraftProposal> {
    let expected = checked_observation(expected_next_public)?;
    let state = position.state();
    let actor = state.decision_actor();
    let phase = state
        .extra
        .get("draft")
        .and_then(|draft| draft.get("phase"))
        .and_then(Value::as_str)
        .ok_or(EngineError::IllegalAction)?;
    if state.mode != "draft"
        || !matches!(phase, "MIDDLE" | "END")
        || expected.viewer != actor.opponent()
        || state.extra.get("gameStyle").and_then(Value::as_str) != Some("normal")
    {
        return Err(EngineError::UnsupportedFeature(
            "hidden stage proposal requires a normal opposing MIDDLE/END decision".into(),
        ));
    }
    let before = state.try_observe(expected.viewer)?;
    if before.public_state.contains_key("draft")
        || expected.history.len() != before.history.len() + 1
        || !same_content(&expected.history[..before.history.len()], &before.history)?
    {
        return Err(EngineError::ConditioningMismatch(
            "hidden stage proposal changed or omitted prior public history".into(),
        ));
    }
    let prior_cards = cards_for_actor(&before, actor)?;
    let next_cards = cards_for_actor(&expected, actor)?;
    let known = prior_cards
        .iter()
        .filter_map(|card| card.get("instanceId").and_then(Value::as_str))
        .collect::<std::collections::BTreeSet<_>>();
    let added = next_cards
        .iter()
        .filter(|card| {
            card.get("instanceId")
                .and_then(Value::as_str)
                .is_some_and(|id| !known.contains(id))
        })
        .collect::<Vec<_>>();
    if next_cards.len() != prior_cards.len() + 1 || added.len() != 1 {
        return Err(EngineError::ConditioningMismatch(
            "hidden stage acquisition must reveal exactly one new card".into(),
        ));
    }
    let required = added[0]
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| EngineError::InvalidState("revealed stage card has no definition".into()))?;
    let (proposed, (source_probability, proposal_probability)) =
        position.transact(position.position_id(), |working| {
            working.rng = RngState::seeded(u64::from(independent_seed));
            crate::draft::propose_hidden_stage_offer(working, phase, required, actor)
        })?;
    let importance_weight = checked_density(source_probability, proposal_probability)?;
    if !same_content(&proposed.state().try_observe(expected.viewer)?, &before)? {
        return Err(EngineError::ConditioningMismatch(
            "hidden stage proposal changed previously public information".into(),
        ));
    }
    Ok(V7HiddenDraftProposal {
        position: proposed,
        importance_weight,
        source_probability,
        proposal_probability,
    })
}

/// source가 증명하는 필요조건만 사용하는 prefilter. true는 도달 가능성의
/// 증명이 아니며 play의 우연한 한 sample 불일치로 합법 intent를 제외하지 않는다.
pub fn public_transition_compatible(
    position: &V7HostPosition,
    admitted: &AdmittedV7Action,
    public: Value,
) -> V7ActionHostResult<bool> {
    let expected = checked_observation(public)?;
    admitted.revalidate(position)?;
    if position.state().mode == "play" {
        let before = position.state().try_observe(expected.viewer)?;
        return Ok(expected.history.len() == before.history.len() + 1
            && same_content(&expected.history[..before.history.len()], &before.history)?);
    }
    if position.state().mode != "draft" {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 public transition compatibility in {} mode",
            position.state().mode
        ))
        .into());
    }
    let action = admitted.action();
    if !matches!(
        action.kind,
        ActionKind::DraftPick | ActionKind::DraftBundlePick
    ) {
        return Err(EngineError::IllegalAction.into());
    }
    let selected = crate::draft::selected_draft_cards(position.state(), action)?;
    let before = position.state().deck_slots.get(action.color);
    let after = cards_for_actor(&expected, action.color)?;
    let slots = before
        .iter()
        .enumerate()
        .filter(|(_, card)| card.vacant)
        .map(|(index, _)| index)
        .take(selected.len())
        .collect::<Vec<_>>();
    if slots.len() != selected.len()
        || after.len() != before.iter().filter(|card| !card.vacant).count() + selected.len()
    {
        return Ok(false);
    }
    let grand = position
        .state()
        .extra
        .get("draft")
        .and_then(|draft| draft.get("kind"))
        .and_then(Value::as_str)
        == Some("grand");
    for (definition, slot) in selected.iter().zip(slots) {
        let Some(card) = after
            .iter()
            .find(|card| crate::observation::number(card.get("slot")) == Some(slot as f64))
        else {
            return Ok(false);
        };
        if card["id"] != definition["id"] || grand && card["instanceId"] != definition["instanceId"]
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn identity_map(
    current: &[Value],
    expected: &[Value],
    map: &mut BTreeMap<String, String>,
) -> Result<()> {
    if current.len() != expected.len() {
        return Err(EngineError::ConditioningMismatch(
            "v7 public card counts changed during identity binding".into(),
        ));
    }
    for (current, expected) in current.iter().zip(expected) {
        let old = current
            .get("instanceId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                EngineError::InvalidState("v7 sampled public card identity is missing".into())
            })?;
        let new = expected
            .get("instanceId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 conditioned public card identity is missing or oversized".into(),
                )
            })?;
        let mut left = current.clone();
        let mut right = expected.clone();
        left.as_object_mut()
            .ok_or(EngineError::IllegalAction)?
            .shift_remove("instanceId");
        right
            .as_object_mut()
            .ok_or(EngineError::IllegalAction)?
            .shift_remove("instanceId");
        if !same_content(&left, &right)? {
            return Err(EngineError::ConditioningMismatch(
                "v7 identity binding would alter card semantics".into(),
            ));
        }
        if map.get(old).is_some_and(|prior| prior != new)
            || map.iter().any(|(key, value)| key != old && value == new)
        {
            return Err(EngineError::InvalidState(
                "v7 public card identity binding must be one-to-one".into(),
            ));
        }
        map.insert(old.into(), new.into());
    }
    Ok(())
}

fn remap(value: &mut Value, map: &BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            if let Some(replacement) = map.get(text) {
                *text = replacement.clone();
            }
        }
        Value::Array(items) => {
            for item in items {
                remap(item, map);
            }
        }
        Value::Object(items) => {
            for item in items.values_mut() {
                remap(item, map);
            }
        }
        _ => {}
    }
}

fn bind_public_identities(
    working: &mut GameState,
    expected: &Observation,
    prior: Option<&Observation>,
) -> Result<()> {
    let current = working.try_observe(expected.viewer)?;
    let mut map = BTreeMap::new();
    identity_map(&current.own_cards, &expected.own_cards, &mut map)?;
    let public_array = |observation: &Observation, field: &str| -> Result<Vec<Value>> {
        observation
            .public_state
            .get(field)
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| EngineError::InvalidState(format!("v7 public {field} is missing")))
    };
    identity_map(
        &public_array(&current, "revealedOpponentCards")?,
        &public_array(expected, "revealedOpponentCards")?,
        &mut map,
    )?;
    match (
        current.public_state.get("draft"),
        expected.public_state.get("draft"),
    ) {
        (Some(left), Some(right)) => {
            let choices = |draft: &Value| -> Result<Vec<Value>> {
                draft
                    .get("choices")
                    .and_then(Value::as_array)
                    .cloned()
                    .ok_or_else(|| {
                        EngineError::InvalidState("v7 public draft choices missing".into())
                    })
            };
            identity_map(&choices(left)?, &choices(right)?, &mut map)?;
        }
        (None, None) => {}
        _ => {
            return Err(EngineError::ConditioningMismatch(
                "v7 identity binding cannot change the public draft phase".into(),
            ));
        }
    }
    if let Some(prior) = prior {
        let mut known = std::collections::BTreeSet::new();
        let mut add = |cards: &[Value]| {
            for card in cards {
                if let Some(id) = card.get("instanceId").and_then(Value::as_str) {
                    known.insert(id.to_owned());
                }
            }
        };
        add(&prior.own_cards);
        add(&public_array(prior, "revealedOpponentCards")?);
        if let Some(choices) = prior
            .public_state
            .get("draft")
            .and_then(|draft| draft.get("choices"))
            .and_then(Value::as_array)
        {
            add(choices);
        }
        if map
            .iter()
            .any(|(old, new)| old != new && known.contains(old))
        {
            return Err(EngineError::ConditioningMismatch(
                "v7 play proposal cannot change a previously public card identity".into(),
            ));
        }
    }
    if map.iter().any(|(old, new)| old != new) {
        let rng = working.rng.clone();
        let mut raw = serde_json::to_value(&*working).map_err(EngineError::serialization)?;
        raw.as_object_mut()
            .expect("GameState is an object")
            .shift_remove("rng");
        remap(&mut raw, &map);
        raw["rng"] = serde_json::to_value(rng).map_err(EngineError::serialization)?;
        *working = serde_json::from_value(raw).map_err(EngineError::serialization)?;
    }
    let actual = working.try_observe(expected.viewer)?;
    if !same_content(&actual, expected)? {
        let field = differing_public_field(&actual, expected)?;
        return Err(EngineError::ConditioningMismatch(format!(
            "v7 public frame differs beyond opaque identities at {field}"
        )));
    }
    Ok(())
}

/// Execute one complete draft action, then condition any newly visible initial
/// Black offer using the bounded source draw trace. For ordinary transitions
/// the proposal is the source kernel itself, so its p/q correction is one.
pub fn apply_weighted_conditioned_public(
    position: &V7HostPosition,
    admitted: &AdmittedV7Action,
    public: Value,
    independent_seed: u32,
) -> V7ActionHostResult<V7ConditionedStepProposal> {
    let expected = checked_observation(public)?;
    admitted.revalidate(position)?;
    if position.state().mode == "play" {
        return apply_source_prior_play(position, admitted, expected, independent_seed);
    }
    if position.state().mode != "draft" {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 weighted public transition in {} mode",
            position.state().mode
        ))
        .into());
    }
    if !public_transition_compatible(
        position,
        admitted,
        serde_json::to_value(&expected).map_err(EngineError::serialization)?,
    )? {
        return Err(EngineError::ConditioningMismatch(
            "v7 draft acquisition contradicts the observed public cards".into(),
        )
        .into());
    }
    let payload = serde_json::to_value(admitted.action()).map_err(EngineError::serialization)?;
    if admitted.source_payload() != &payload
        || !v7_adapter_actions::legal_action_envelopes(position)?
            .iter()
            .any(|candidate| candidate.get("payload") == Some(&payload))
    {
        return Err(EngineError::IllegalAction.into());
    }
    let actor = position.state().decision_actor();
    let old_turn = position.state().turn;
    let old_history_len = position.state().history.len();
    let old_draft = position.state().extra.get("draft").cloned();
    let visible_stage = expected.public_state.get("draft").and_then(|draft| {
        let phase = draft.get("phase")?.as_str()?;
        let color = draft.get("color")?.as_str()?;
        if actor == Color::White
            && expected.viewer == Color::Black
            && color == "black"
            && matches!(phase, "MIDDLE" | "END")
            && old_draft.as_ref()?.get("phase")?.as_str()? == phase
        {
            Some((phase.to_owned(), draft.get("choices")?.as_array()?.clone()))
        } else {
            None
        }
    });
    let (next, (captures, source_probability, proposal_probability)) =
        position.transact(position.position_id(), |working| {
            working.semantic_chance_probability = Some(1.0);
            if let Some((phase, choices)) = &visible_stage {
                working.source_offer_condition = Some(crate::draft::SourceOfferCondition {
                    phase: phase.clone(),
                    color: Color::Black,
                    choices: choices.clone(),
                    density: None,
                });
            }
            let captures = crate::transition::apply(working, admitted.action())?;
            crate::replay::canonicalize_position_frames(working)?;
            if working.history.len() != old_history_len + 1 {
                return Err(EngineError::InvalidState(
                    "v7 weighted draft transition must append exactly one event".into(),
                ));
            }
            let semantic_probability =
                working.semantic_chance_probability.take().ok_or_else(|| {
                    EngineError::InvalidState("v7 semantic chance trace was lost".into())
                })?;
            let stage_density = working.source_offer_condition.take();
            let new_draft = working.extra.get("draft");
            let new_initial_offer = old_draft
                .as_ref()
                .and_then(|draft| draft.get("kind"))
                .and_then(Value::as_str)
                != Some("grand")
                && old_draft
                    .as_ref()
                    .and_then(|draft| draft.get("color"))
                    .and_then(Value::as_str)
                    == Some("white")
                && new_draft
                    .and_then(|draft| draft.get("color"))
                    .and_then(Value::as_str)
                    == Some("black")
                && new_draft
                    .and_then(|draft| draft.get("phase"))
                    .and_then(Value::as_str)
                    == Some("OPENING")
                && working.mode == "draft"
                && working.move_count == 0;
            let (p, q) = if let Some(stage) = stage_density {
                let (offer_p, offer_q) = stage.density.ok_or_else(|| {
                    EngineError::ConditioningMismatch(
                        "observed stage offer was not produced by the source draft".into(),
                    )
                })?;
                (
                    semantic_probability * offer_p,
                    semantic_probability * offer_q,
                )
            } else if visible_stage.is_some() {
                return Err(EngineError::ConditioningMismatch(
                    "observed stage offer was not reached by the source draft".into(),
                ));
            } else if new_initial_offer {
                require_standard_opening_board(working)?;
                working.rng = RngState::seeded(u64::from(independent_seed));
                let (offer_p, offer_q) = if expected.viewer == Color::Black {
                    let choices = expected
                        .public_state
                        .get("draft")
                        .filter(|draft| draft.get("color").and_then(Value::as_str) == Some("black"))
                        .and_then(|draft| draft.get("choices"))
                        .and_then(Value::as_array)
                        .ok_or_else(|| {
                            EngineError::ConditioningMismatch(
                                "new v7 own opening offer must be public".into(),
                            )
                        })?;
                    if working.extra.get("gameStyle").and_then(Value::as_str) == Some("chaos") {
                        crate::draft::propose_observed_chaos_offer(working, choices, Color::Black)?
                    } else {
                        crate::draft::propose_observed_normal_offer(working, choices, Color::Black)?
                    }
                } else if working.extra.get("gameStyle").and_then(Value::as_str) == Some("chaos") {
                    crate::draft::propose_unobserved_chaos_offer(working, Color::Black)?
                } else {
                    crate::draft::propose_unobserved_normal_offer(working, Color::Black)?
                };
                // The offer is sampled after the admitted White acquisition.
                // A semantic chance effect in that acquisition contributes to
                // the complete transition density even though its p/q cancels.
                (
                    semantic_probability * offer_p,
                    semantic_probability * offer_q,
                )
            } else {
                (semantic_probability, semantic_probability)
            };
            bind_public_identities(working, &expected, None)?;
            // An observed-offer proposal already consumed its independent
            // stream. Keep that advanced stream so future draws cannot replay
            // the same proposal's random choices. For an ordinary source
            // transition, install the caller's fresh future stream now.
            if !new_initial_offer {
                working.rng = RngState::seeded(u64::from(independent_seed));
            }
            Ok((captures, p, q))
        })?;
    let importance_weight = checked_density(source_probability, proposal_probability)?;
    let event = next.state().history.last().cloned().ok_or_else(|| {
        EngineError::InvalidState("v7 weighted draft transition omitted its public event".into())
    })?;
    if event.get("actor") != Some(&serde_json::json!(actor))
        || event.get("action") != Some(admitted.source_payload())
    {
        return Err(EngineError::InvalidState(
            "v7 weighted draft event differs from admitted action".into(),
        )
        .into());
    }
    let applied = AppliedV7Action {
        turn_changed: old_turn != next.state().turn,
        result: next.state().result(),
        position: next,
        actor,
        captures,
        event,
        action_id: admitted.action_id().to_owned(),
    };
    Ok(V7ConditionedStepProposal {
        applied,
        importance_weight,
        source_probability,
        proposal_probability,
    })
}

/// 공개 play의 한 번짜리 forward 제안. fresh source stream을 실제 전이 전에
/// 설치하고, 관측을 맞추기 위한 재시도·강제 draw·hidden 환경 RNG 입력은 사용하지 않는다.
fn apply_source_prior_play(
    position: &V7HostPosition,
    admitted: &AdmittedV7Action,
    expected: Observation,
    independent_seed: u32,
) -> V7ActionHostResult<V7ConditionedStepProposal> {
    let prior = position.state().try_observe(expected.viewer)?;
    if expected.history.len() != prior.history.len() + 1
        || !same_content(&expected.history[..prior.history.len()], &prior.history)?
    {
        return Err(EngineError::ConditioningMismatch(
            "v7 play proposal contradicts the complete prior public history".into(),
        )
        .into());
    }
    let action = admitted.action();
    if admitted.source_payload()
        != &serde_json::to_value(action).map_err(EngineError::serialization)?
    {
        return Err(EngineError::IllegalAction.into());
    }
    let actor = position.state().decision_actor();
    let old_turn = position.state().turn;
    let old_history_len = position.state().history.len();
    let visible_stage = expected.public_state.get("draft").and_then(|draft| {
        let phase = draft.get("phase")?.as_str()?;
        let color = draft.get("color")?.as_str()?;
        if matches!(phase, "MIDDLE" | "END") && color == expected.viewer.as_str() {
            Some((
                phase.to_owned(),
                expected.viewer,
                draft.get("choices")?.as_array()?.clone(),
            ))
        } else {
            None
        }
    });
    let (next, (captures, source_probability, proposal_probability)) =
        position.transact(position.position_id(), |working| {
            working.rng = RngState::seeded(u64::from(independent_seed));
            working.rng.begin_source_trace()?;
            if let Some((phase, color, choices)) = &visible_stage {
                working.source_offer_condition = Some(crate::draft::SourceOfferCondition {
                    phase: phase.clone(),
                    color: *color,
                    choices: choices.clone(),
                    density: None,
                });
            }
            let captures =
                v7_adapter_actions::execute_on_working(working, action, actor, old_history_len)?;
            // 실제 carry된 probe의 분기까지 인증한다. unclassified draw는 정확한
            // source callsite 오류이며 공개 관측 불일치로 바꿔 숨기지 않는다.
            let offer_density = working.source_offer_condition.take();
            let probability = working.rng.finish_source_trace()?;
            let proposal_probability = match (visible_stage.as_ref(), offer_density) {
                (None, None) => probability,
                (
                    Some(_),
                    Some(crate::draft::SourceOfferCondition {
                        density: Some((offer_p, offer_q)),
                        ..
                    }),
                ) if offer_p > 0.0 && offer_q > 0.0 => probability * offer_q / offer_p,
                _ => {
                    return Err(EngineError::ConditioningMismatch(
                        "observed public stage offer was not produced by the source transition"
                            .into(),
                    ));
                }
            };
            bind_public_identities(working, &expected, Some(&prior))?;
            Ok((captures, probability, proposal_probability))
        })?;
    let importance_weight = checked_density(source_probability, proposal_probability)?;
    let event = next.state().history.last().cloned().ok_or_else(|| {
        EngineError::InvalidState("v7 source prior play omitted its public event".into())
    })?;
    if event.get("actor") != Some(&serde_json::json!(actor))
        || event.get("action") != Some(admitted.source_payload())
    {
        return Err(EngineError::InvalidState(
            "v7 source prior play event differs from admitted action".into(),
        )
        .into());
    }
    let applied = AppliedV7Action {
        turn_changed: old_turn != next.state().turn,
        result: next.state().result(),
        position: next,
        actor,
        captures,
        event,
        action_id: admitted.action_id().to_owned(),
    };
    Ok(V7ConditionedStepProposal {
        applied,
        importance_weight,
        source_probability,
        proposal_probability,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard_play_host(style: &str, seed: u64) -> V7HostPosition {
        V7HostPosition::from_state(
            crate::v7_new_game::new_game(
                GameConfig {
                    game_style: style.into(),
                    draft_delete: true,
                    ..GameConfig::default()
                },
                seed,
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn source_prior_play_preserves_full_public_history_for_both_actors_and_viewers() {
        for style in ["normal", "chaos", "grand"] {
            for viewer in [Color::White, Color::Black] {
                let mut current = standard_play_host(style, 19);
                for seed in [71, 72] {
                    assert_eq!(current.state().mode, "play");
                    let original = current.export_envelope().unwrap();
                    let intent = v7_adapter_actions::legal_public_intents(&current)
                        .unwrap()
                        .into_iter()
                        .find(|intent| intent["type"] == "move")
                        .unwrap();
                    let admitted =
                        v7_adapter_actions::bind_public_intent(&current, intent).unwrap();
                    let raw = v7_adapter_actions::apply_admitted(&current, &admitted).unwrap();
                    let public =
                        serde_json::to_value(raw.position.state().try_observe(viewer).unwrap())
                            .unwrap();
                    assert!(
                        public_transition_compatible(&current, &admitted, public.clone()).unwrap()
                    );
                    let proposed = apply_weighted_conditioned_public(
                        &current,
                        &admitted,
                        public.clone(),
                        seed,
                    )
                    .unwrap();
                    let repeated = apply_weighted_conditioned_public(
                        &current,
                        &admitted,
                        public.clone(),
                        seed,
                    )
                    .unwrap();
                    assert_eq!(
                        proposed.source_probability, 1.0,
                        "deterministic {style} move"
                    );
                    assert_eq!(proposed.proposal_probability, 1.0);
                    assert_eq!(proposed.importance_weight, 1.0);
                    assert_eq!(current.export_envelope().unwrap(), original);
                    assert_eq!(
                        proposed.applied.position.export_envelope().unwrap(),
                        repeated.applied.position.export_envelope().unwrap()
                    );
                    assert!(
                        same_content(
                            &proposed
                                .applied
                                .position
                                .state()
                                .try_observe(viewer)
                                .unwrap(),
                            &checked_observation(public.clone()).unwrap()
                        )
                        .unwrap()
                    );
                    assert_eq!(proposed.applied.position.revision(), current.revision() + 1);
                    assert_eq!(
                        proposed.applied.position.state().history.len(),
                        current.state().history.len() + 1
                    );
                    assert_ne!(
                        proposed.applied.position.state().rng,
                        RngState::seeded(u64::from(seed)),
                        "advanced proposal stream must be retained"
                    );
                    assert!(
                        apply_weighted_conditioned_public(
                            &proposed.applied.position,
                            &admitted,
                            public,
                            seed
                        )
                        .is_err()
                    );
                    current = proposed.applied.position;
                }
            }
        }
    }

    #[test]
    fn source_prior_play_rejects_changed_prior_history_and_impossible_latest_event() {
        let host = standard_play_host("normal", 19);
        let (admitted, first) = first_play_step(&host);
        let mut impossible = first.position.state().try_observe(Color::White).unwrap();
        impossible.history.last_mut().unwrap()["actor"] = serde_json::json!("black");
        impossible.refresh_key();
        let before = host.export_envelope().unwrap();
        assert!(matches!(
            apply_weighted_conditioned_public(
                &host,
                &admitted,
                serde_json::to_value(impossible).unwrap(),
                71
            ),
            Err(crate::v7_adapter_actions::V7ActionHostError::Engine(
                EngineError::ConditioningMismatch(_)
            ))
        ));
        assert_eq!(host.export_envelope().unwrap(), before);
        let (second_admitted, second) = first_play_step(&first.position);
        let mut changed = second.position.state().try_observe(Color::White).unwrap();
        changed.history[0]["actor"] = serde_json::json!("black");
        changed.refresh_key();
        let changed = serde_json::to_value(changed).unwrap();
        assert!(
            !public_transition_compatible(&first.position, &second_admitted, changed.clone())
                .unwrap()
        );
        assert!(matches!(
            apply_weighted_conditioned_public(&first.position, &second_admitted, changed, 72),
            Err(crate::v7_adapter_actions::V7ActionHostError::Engine(
                EngineError::ConditioningMismatch(_)
            ))
        ));
    }

    #[test]
    fn source_prior_otherworld_reports_real_uniform_mass_without_forcing_observation() {
        let mut state = standard_play_host("normal", 19).state().clone();
        // draftDelete는 원문에서 카드 사용도 비활성화한다. 카드 전이 검사는
        // 일반 play의 카드 창을 열고 실제 공개 후보를 통해 행동을 결속한다.
        state.extra.insert("draftDelete".into(), Value::Bool(false));
        let mut definition = crate::card_registry::definition_for(RULES_VERSION_V7, "otherworld")
            .unwrap()
            .source_definition
            .clone();
        definition["instanceId"] = serde_json::json!("public-otherworld-card");
        state.deck_slots.white = vec![serde_json::from_value(definition).unwrap()];
        // 기대 관측을 생성하는 독립 raw stream과 제안 stream의 공통 seed만 맞춘다.
        // 실제 소비자는 hidden 환경 RNG를 이 API로 넘기지 않는다.
        state.rng = RngState::seeded(71);
        let host = V7HostPosition::from_state(state).unwrap();
        let intent = v7_adapter_actions::legal_public_intents(&host)
            .unwrap()
            .into_iter()
            .find(|intent| intent["cardId"] == "otherworld")
            .unwrap();
        let admitted = v7_adapter_actions::bind_public_intent(&host, intent).unwrap();
        let raw = v7_adapter_actions::apply_admitted(&host, &admitted).unwrap();
        let public =
            serde_json::to_value(raw.position.state().try_observe(Color::White).unwrap()).unwrap();
        let original = host.export_envelope().unwrap();
        let proposal =
            apply_weighted_conditioned_public(&host, &admitted, public.clone(), 71).unwrap();
        assert_eq!(proposal.source_probability, 1.0 / 8.0);
        assert_eq!(proposal.proposal_probability, 1.0 / 8.0);
        assert_eq!(proposal.importance_weight, 1.0);
        assert_eq!(
            proposal.applied.position.state().rng,
            raw.position.state().rng
        );
        // default seed 0과 위 독립 seed 71의 첫 LCG draw는 서로 다른
        // uniform pawn bucket이다. source-prior는 관측을 강제로 맞추지
        // 않고 다른 결과를 정확히 거절하며 기존 snapshot을 보존한다.
        assert!(matches!(
            apply_weighted_conditioned_public(&host, &admitted, public, 0),
            Err(crate::v7_adapter_actions::V7ActionHostError::Engine(
                EngineError::ConditioningMismatch(message)
            )) if message.contains("at $.board")
        ));
        assert_eq!(host.export_envelope().unwrap(), original);
    }

    fn source_host(style: &str, seed: u64) -> V7HostPosition {
        V7HostPosition::from_state(
            crate::v7_new_game::new_game(
                GameConfig {
                    game_style: style.into(),
                    ..GameConfig::default()
                },
                seed,
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn first_play_step(host: &V7HostPosition) -> (AdmittedV7Action, AppliedV7Action) {
        let intent = v7_adapter_actions::legal_public_intents(host)
            .unwrap()
            .into_iter()
            .find(|intent| intent["type"] == "move")
            .expect("standard play has a legal public move");
        let admitted = v7_adapter_actions::bind_public_intent(host, intent).unwrap();
        let applied = v7_adapter_actions::apply_admitted(host, &admitted).unwrap();
        (admitted, applied)
    }

    fn first_step(host: &V7HostPosition) -> (AdmittedV7Action, AppliedV7Action) {
        let intent =
            v7_adapter_actions::legal_action_envelopes(host).unwrap()[0]["payload"].clone();
        let admitted = v7_adapter_actions::bind_public_intent(host, intent).unwrap();
        let applied = v7_adapter_actions::apply_admitted(host, &admitted).unwrap();
        (admitted, applied)
    }

    #[test]
    fn independent_initial_particle_matches_complete_signed_public_frame() {
        for style in ["normal", "chaos", "grand"] {
            let source = source_host(style, 19);
            let config = GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            };
            for viewer in [Color::White, Color::Black] {
                let expected = source.state().try_observe(viewer).unwrap();
                let position = sample_initial_public(
                    config.clone(),
                    serde_json::to_value(&expected).unwrap(),
                    71,
                )
                .unwrap();
                assert_eq!(position.state().try_observe(viewer).unwrap(), expected);
                assert_eq!(position.revision(), 0);
                assert_ne!(position.state().rng, source.state().rng);
                assert!(position.state().history.is_empty());
            }
        }
    }

    #[test]
    fn initial_particle_respects_source_setup_limits() {
        for style in ["normal", "chaos", "grand"] {
            let config = GameConfig {
                game_style: style.into(),
                star_win_limit: 7,
                deathmatch_limit_turns: 13,
                ..GameConfig::default()
            };
            let source = crate::v7_new_game::new_game(config.clone(), 37).unwrap();
            assert_eq!(source.mode, "draft", "{style}");
            for viewer in [Color::White, Color::Black] {
                let expected = source.try_observe(viewer).unwrap();
                let particle = sample_initial_public(
                    config.clone(),
                    serde_json::to_value(&expected).unwrap(),
                    71,
                )
                .unwrap();
                assert_eq!(particle.state().try_observe(viewer).unwrap(), expected);
                assert_ne!(particle.state().rng, source.rng);
            }
        }
    }

    #[test]
    fn chaos_initial_particle_keeps_complete_public_projection_with_independent_search_seeds() {
        let config = GameConfig {
            game_style: "chaos".into(),
            ..GameConfig::default()
        };
        let source = crate::v7_new_game::new_game(config.clone(), 37).unwrap();
        // 실제 ParticleBelief 71/72 stream의 첫 독립 초기화 seed.
        // source game의 seed/RNG나 private offer를 proposal에 넘기지 않는다.
        for seed in [4_090_132_643, 1_088_403_456, 4_090_557_808, 3_598_481_373] {
            let independent =
                crate::v7_new_game::new_game(config.clone(), u64::from(seed)).unwrap();
            for viewer in [Color::White, Color::Black] {
                let expected = source.try_observe(viewer).unwrap();
                let particle = sample_initial_public(
                    config.clone(),
                    serde_json::to_value(&expected).unwrap(),
                    seed,
                )
                .unwrap_or_else(|error| panic!("viewer {viewer:?}, seed {seed}: {error}"));
                assert_eq!(particle.state().try_observe(viewer).unwrap(), expected);
                assert_eq!(particle.state().rng, independent.rng);
                assert_ne!(particle.state().rng, source.rng);
                assert!(particle.state().history.is_empty());
            }
        }
    }

    #[test]
    fn initial_particle_matches_a_public_single_rule_activation() {
        let config = GameConfig {
            rule_card_ids: vec!["revelation".into()],
            ..GameConfig::default()
        };
        let source = crate::v7_new_game::new_game(config.clone(), 37).unwrap();
        for viewer in [Color::White, Color::Black] {
            let expected = source.try_observe(viewer).unwrap();
            let particle =
                sample_initial_public(config.clone(), serde_json::to_value(&expected).unwrap(), 71)
                    .unwrap();
            assert_eq!(particle.state().try_observe(viewer).unwrap(), expected);
        }
    }

    #[test]
    fn hidden_initial_offer_tilt_preserves_previous_viewer_frame_and_source_history() {
        for style in ["normal", "chaos"] {
            for seed in [19, 37] {
                let source = source_host(style, seed);
                let before = source.state().try_observe(Color::Black).unwrap();
                let original = source.export_envelope().unwrap();
                let (_, applied) = first_step(&source);
                let expected = applied.position.state().try_observe(Color::Black).unwrap();
                let proposal = condition_hidden_opening_draft(
                    &source,
                    serde_json::to_value(&expected).unwrap(),
                    71,
                )
                .unwrap();
                let repeated = condition_hidden_opening_draft(
                    &source,
                    serde_json::to_value(expected).unwrap(),
                    71,
                )
                .unwrap();
                assert_eq!(
                    source.export_envelope().unwrap(),
                    original,
                    "{style} seed {seed}"
                );
                assert_eq!(
                    proposal.position.export_envelope().unwrap(),
                    repeated.position.export_envelope().unwrap(),
                    "{style} seed {seed} proposal and future RNG must be reproducible"
                );
                assert_eq!(proposal.source_probability, repeated.source_probability);
                assert_eq!(proposal.proposal_probability, repeated.proposal_probability);
                assert_eq!(
                    proposal.position.state().try_observe(Color::Black).unwrap(),
                    before,
                    "{style} seed {seed}"
                );
                assert_eq!(proposal.position.state().history, source.state().history);
                assert_eq!(proposal.position.revision(), source.revision() + 1);
                assert_eq!(
                    proposal.importance_weight,
                    proposal.source_probability / proposal.proposal_probability
                );
                assert!(proposal.source_probability > 0.0);
                assert!(proposal.proposal_probability > 0.0);
            }
        }
    }

    #[test]
    fn hidden_black_offer_tilt_reaches_observed_acquisition_without_seed_search() {
        let source = source_host("normal", 37);
        let (_, after_white) = first_step(&source);
        let (_, after_black) = first_step(&after_white.position);
        let expected = after_black
            .position
            .state()
            .try_observe(Color::White)
            .unwrap();
        let acquired = expected.public_state["revealedOpponentCards"][0]["id"]
            .as_str()
            .unwrap();
        let before = after_white
            .position
            .state()
            .try_observe(Color::White)
            .unwrap();
        let mut matching = 0;
        for seed in 0..32 {
            let proposal = condition_hidden_opening_draft(
                &after_white.position,
                serde_json::to_value(&expected).unwrap(),
                seed,
            )
            .unwrap();
            assert_eq!(
                proposal.position.state().try_observe(Color::White).unwrap(),
                before
            );
            assert!(proposal.source_probability.is_finite());
            assert!(proposal.proposal_probability.is_finite());
            assert!(proposal.proposal_probability > 0.0);
            let choices = proposal.position.state().extra["draft"]["choices"]
                .as_array()
                .unwrap();
            matching += usize::from(choices.iter().any(|card| card["id"] == acquired));
        }
        assert!(
            matching >= 24,
            "observed public acquisition should be proposed in most draws"
        );
    }

    #[test]
    fn weighted_draft_step_keeps_source_position_immutable_and_rejects_stale_action() {
        for (style, viewer) in [
            ("normal", Color::Black),
            ("chaos", Color::Black),
            ("grand", Color::White),
        ] {
            let source = source_host(style, 19);
            let before = source.export_envelope().unwrap();
            let (admitted, actual) = first_step(&source);
            let expected =
                serde_json::to_value(actual.position.state().try_observe(viewer).unwrap()).unwrap();
            let proposal =
                apply_weighted_conditioned_public(&source, &admitted, expected.clone(), 93)
                    .unwrap();
            assert_eq!(source.export_envelope().unwrap(), before, "{style}");
            assert_eq!(proposal.applied.position.revision(), source.revision() + 1);
            assert_eq!(proposal.applied.position.state().history.len(), 1);
            assert_eq!(
                proposal
                    .applied
                    .position
                    .state()
                    .try_observe(viewer)
                    .unwrap(),
                serde_json::from_value::<Observation>(expected.clone()).unwrap()
            );
            assert_eq!(
                proposal.importance_weight,
                proposal.source_probability / proposal.proposal_probability
            );
            let stale = apply_weighted_conditioned_public(
                &proposal.applied.position,
                &admitted,
                expected,
                93,
            );
            assert!(matches!(
                stale,
                Err(crate::v7_adapter_actions::V7ActionHostError::Admission(_))
            ));
        }
    }

    fn assert_weighted_second_draft_step(styles: &[&str]) {
        for &style in styles {
            let source = source_host(style, 19);
            let (first, source_first) = first_step(&source);
            let public_first = serde_json::to_value(
                source_first
                    .position
                    .state()
                    .try_observe(Color::Black)
                    .unwrap(),
            )
            .unwrap();
            let proposed_first =
                apply_weighted_conditioned_public(&source, &first, public_first, 93).unwrap();

            // The next public intent must bind against the proposed Position,
            // not carry the source Position ID across a hidden-state boundary.
            let (source_second_intent, source_second) = first_step(&source_first.position);
            let second = v7_adapter_actions::bind_public_intent(
                &proposed_first.applied.position,
                source_second_intent.source_payload().clone(),
            )
            .unwrap();
            let public_second = serde_json::to_value(
                source_second
                    .position
                    .state()
                    .try_observe(Color::Black)
                    .unwrap(),
            )
            .unwrap();
            let proposed_second = apply_weighted_conditioned_public(
                &proposed_first.applied.position,
                &second,
                public_second,
                94,
            )
            .unwrap();
            assert_eq!(
                proposed_second.applied.position.state().rng,
                RngState::seeded(94),
                "{style} must carry the caller's independent future stream"
            );
            assert_eq!(proposed_second.applied.position.state().history.len(), 2);
            if matches!(style, "normal" | "chaos") {
                assert_eq!(source_second.position.state().mode, "play", "{style}");
                assert_eq!(
                    proposed_second.applied.position.state().mode,
                    "play",
                    "{style}"
                );
            }
            for viewer in [Color::White, Color::Black] {
                assert_eq!(
                    proposed_second
                        .applied
                        .position
                        .state()
                        .try_observe(viewer)
                        .unwrap(),
                    source_second.position.state().try_observe(viewer).unwrap(),
                    "{style} viewer {viewer:?}"
                );
            }
            assert_eq!(
                proposed_first.importance_weight * proposed_second.importance_weight,
                (proposed_first.source_probability * proposed_second.source_probability)
                    / (proposed_first.proposal_probability * proposed_second.proposal_probability),
                "{style} two-step density must compose"
            );
        }
    }

    #[test]
    fn weighted_second_grand_draft_step_reuses_public_acquisition_with_a_fresh_future_stream() {
        assert_weighted_second_draft_step(&["grand"]);
    }

    #[test]
    #[ignore = "NO-GO: v7 source-complete getLegalMoves is required for the no-action verdict after normal/chaos second acquisition"]
    fn weighted_second_normal_and_chaos_draft_step_reaches_source_play() {
        assert_weighted_second_draft_step(&["normal", "chaos"]);
    }

    // These receipts enter through the exact faithful175 source envelopes.
    // Source transitions compare every state/RNG/event/identity field, while
    // conditioned particles retain their independent future RNG contract.
    fn faithful_draft_play_receipts() -> Vec<Value> {
        use sha2::{Digest, Sha256};
        use std::collections::BTreeSet;

        let path = std::env::var("ACCELERATE_V7_WEIGHTED_DRAFT_PLAY_CASES")
            .expect("supply the external faithful175 two-draft/first-move receipt");
        assert!(
            std::fs::metadata(&path).unwrap().len() <= 32 * 1024 * 1024,
            "the fixed two-style receipt exceeds 32 MiB"
        );
        let cases = std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            cases.len(),
            2,
            "receipt must contain normal and chaos exactly once"
        );
        let profile: Value = serde_json::from_str(include_str!(
            "../../contracts/catalog/execution-profile-20260928.json"
        ))
        .unwrap();
        let profile_sha = format!("{:x}", Sha256::digest(serde_jcs::to_vec(&profile).unwrap()));
        let catalog: Value =
            serde_json::from_str(include_str!("../../contracts/catalog/site-20260928.json"))
                .unwrap();
        let mut styles = BTreeSet::new();
        for case in &cases {
            assert_eq!(
                case["schemaVersion"],
                "augment-v7-weighted-draft-play-receipt-v1"
            );
            assert_eq!(
                case["fixtureKind"],
                "source-reachable-two-drafts-and-first-move"
            );
            assert_eq!(case["sourceSha256"], profile["sourceMainSha256"]);
            assert_eq!(case["executionProfileVersion"], profile["profileVersion"]);
            assert_eq!(
                case["executionProfileSha256"].as_str(),
                Some(profile_sha.as_str())
            );
            assert_eq!(case["parserSha256"], profile["parserSha256"]);
            assert_eq!(case["catalogVersion"], catalog["catalogVersion"]);
            assert_eq!(case["initial"]["catalogVersion"], catalog["catalogVersion"]);
            assert_eq!(case["seed"], 19);
            assert_eq!(case["independentFutureSeeds"], serde_json::json!([93, 94]));
            let style = case["style"].as_str().unwrap();
            assert!(matches!(style, "normal" | "chaos"));
            assert!(
                styles.insert(style.to_owned()),
                "duplicate receipt style {style}"
            );
            assert_eq!(case["config"]["gameStyle"], style);
            assert_eq!(case["config"]["draftDelete"], false);
            assert_eq!(case["initial"]["state"]["mode"], "draft");
            let steps = case["steps"].as_array().unwrap();
            assert_eq!(
                steps.len(),
                3,
                "{style}: two choices and one actual move are required"
            );
            let draft_kind = if style == "chaos" {
                "draftBundlePick"
            } else {
                "draftPick"
            };
            for (index, step) in steps.iter().enumerate() {
                let expected_before = if index == 0 {
                    &case["initial"]
                } else {
                    &steps[index - 1]["after"]
                };
                assert_receipt_json_eq(
                    &step["before"],
                    expected_before,
                    &format!("{style} source chain/{index}"),
                );
                assert_eq!(step["action"]["positionId"], step["before"]["positionId"]);
                assert_eq!(step["before"]["history"].as_array().unwrap().len(), index);
                assert_eq!(
                    step["after"]["history"].as_array().unwrap().len(),
                    index + 1
                );
                assert_eq!(
                    step["action"]["payload"]["type"],
                    if index < 2 { draft_kind } else { "move" }
                );
                assert_eq!(step["selection"]["totalBudget"], 4096);
                assert!((1..=4096).contains(&step["selection"]["examined"].as_u64().unwrap()));
                assert!((1..=4096).contains(&step["selection"]["pages"].as_u64().unwrap()));
            }
            assert_eq!(steps[0]["action"]["payload"]["color"], "white");
            assert_eq!(steps[1]["action"]["payload"]["color"], "black");
            assert_eq!(steps[0]["after"]["state"]["mode"], "draft");
            assert_eq!(steps[1]["after"]["state"]["mode"], "play");
            assert_eq!(
                steps[2]["action"]["payload"]["color"],
                steps[2]["before"]["state"]["turn"]
            );
            assert!(
                !same_content(
                    &steps[2]["before"]["state"]["board"],
                    &steps[2]["after"]["state"]["board"]
                )
                .unwrap()
            );
        }
        cases
    }

    fn assert_receipt_json_eq(actual: &Value, expected: &Value, context: &str) {
        let mut mismatches = Vec::new();
        crate::tests::source_callback_fixture::compare_value(
            expected,
            actual,
            context,
            &mut mismatches,
        )
        .unwrap();
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    fn apply_faithful_receipt_step(
        host: &V7HostPosition,
        step: &Value,
        context: &str,
    ) -> AppliedV7Action {
        assert_receipt_json_eq(
            &host.export_envelope().unwrap(),
            &step["before"],
            &format!("{context}/before"),
        );
        let admitted = crate::v7_action_admission::admit_v7_action(host, step["action"].clone())
            .unwrap_or_else(|error| panic!("{context}: source action admission failed: {error}"));
        let applied = v7_adapter_actions::apply_admitted(host, &admitted)
            .unwrap_or_else(|error| panic!("{context}: full native transition failed: {error}"));
        assert_receipt_json_eq(
            &applied.position.export_envelope().unwrap(),
            &step["after"],
            &format!("{context}/full envelope"),
        );
        for (viewer, field) in [(Color::White, "white"), (Color::Black, "black")] {
            assert_receipt_json_eq(
                &serde_json::to_value(applied.position.state().try_observe(viewer).unwrap())
                    .unwrap(),
                &step["publicAfter"][field],
                &format!("{context}/public {field}"),
            );
        }
        applied
    }

    #[test]
    #[ignore = "external faithful175 source receipt is generated outside Git by the main agent"]
    fn frozen_weighted_normal_and_chaos_second_acquisition_when_receipts_are_supplied() {
        for case in faithful_draft_play_receipts() {
            let style = case["style"].as_str().unwrap();
            let steps = case["steps"].as_array().unwrap();
            let mut actual = V7HostPosition::from_envelope(case["initial"].clone()).unwrap();
            let mut proposed = actual.clone();
            let (mut product_weight, mut product_p, mut product_q) = (1.0, 1.0, 1.0);
            for (index, step) in steps.iter().take(2).enumerate() {
                let context = format!("{style}/draft {index}");
                let source_applied = apply_faithful_receipt_step(&actual, step, &context);
                let intent = v7_adapter_actions::bind_public_intent(
                    &proposed,
                    step["action"]["payload"].clone(),
                )
                .unwrap_or_else(|error| {
                    panic!("{context}: proposed public intent failed: {error}")
                });
                let seed = case["independentFutureSeeds"][index].as_u64().unwrap() as u32;
                let proposal = apply_weighted_conditioned_public(
                    &proposed,
                    &intent,
                    step["publicAfter"]["black"].clone(),
                    seed,
                )
                .unwrap_or_else(|error| {
                    panic!("{context}: weighted public transition failed: {error}")
                });
                assert_eq!(proposal.applied.position.state().history.len(), index + 1);
                assert_receipt_json_eq(
                    &serde_json::to_value(
                        proposal
                            .applied
                            .position
                            .state()
                            .try_observe(Color::Black)
                            .unwrap(),
                    )
                    .unwrap(),
                    &step["publicAfter"]["black"],
                    &format!("{context}/weighted Black frame"),
                );
                product_weight *= proposal.importance_weight;
                product_p *= proposal.source_probability;
                product_q *= proposal.proposal_probability;
                actual = source_applied.position;
                proposed = proposal.applied.position;
            }
            assert_eq!(actual.state().mode, "play", "{style}");
            assert_eq!(proposed.state().mode, "play", "{style}");
            assert_eq!(
                proposed.state().rng,
                RngState::seeded(94),
                "{style}: independent future RNG"
            );
            for (viewer, field) in [(Color::White, "white"), (Color::Black, "black")] {
                assert_receipt_json_eq(
                    &serde_json::to_value(proposed.state().try_observe(viewer).unwrap()).unwrap(),
                    &steps[1]["publicAfter"][field],
                    &format!("{style}/weighted play-entry {field}"),
                );
            }
            let composed = product_p / product_q;
            assert!(
                (product_weight - composed).abs() <= composed * 1e-12,
                "{style}: two-step p/q composition differs"
            );
        }
    }

    #[test]
    #[ignore = "external faithful175 source receipt is generated outside Git by the main agent"]
    fn frozen_normal_and_chaos_first_play_move_when_receipts_are_supplied() {
        use crate::tests::source_callback_fixture::collect_case_diagnostics;
        let mut failures = Vec::new();
        for case in faithful_draft_play_receipts() {
            let style = case["style"].as_str().unwrap();
            collect_case_diagnostics(style, &mut failures, |_| {
                let step = &case["steps"][2];
                let host = V7HostPosition::from_envelope(step["before"].clone())?;
                assert_eq!(host.state().mode, "play", "{style}");
                assert_eq!(host.state().history.len(), 2, "{style}");
                let applied = apply_faithful_receipt_step(
                    &host,
                    step,
                    &format!("{style}/first actual play move"),
                );
                assert_eq!(applied.position.state().history.len(), 3, "{style}");
                Ok(())
            });
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn hidden_black_offer_uses_independent_future_stream_without_leaking_to_white() {
        for style in ["normal", "chaos"] {
            let source = source_host(style, 19);
            let (first, actual) = first_step(&source);
            let public =
                serde_json::to_value(actual.position.state().try_observe(Color::White).unwrap())
                    .unwrap();
            let proposed =
                apply_weighted_conditioned_public(&source, &first, public.clone(), 93).unwrap();
            let repeat = apply_weighted_conditioned_public(&source, &first, public, 93).unwrap();
            assert_eq!(
                proposed.applied.position.export_envelope().unwrap(),
                repeat.applied.position.export_envelope().unwrap(),
                "{style} same independent seed must reproduce the same proposal"
            );
            assert_eq!(
                proposed
                    .applied
                    .position
                    .state()
                    .try_observe(Color::White)
                    .unwrap(),
                actual.position.state().try_observe(Color::White).unwrap(),
                "{style} White must not observe the hidden Black offer"
            );
            assert_eq!(proposed.importance_weight, 1.0, "{style}");
            assert_eq!(proposed.source_probability, proposed.proposal_probability);
            assert_ne!(
                proposed.applied.position.state().rng,
                source.state().rng,
                "{style} future draws must not inherit the source stream"
            );
            let choices = proposed.applied.position.state().extra["draft"]["choices"]
                .as_array()
                .unwrap();
            assert_eq!(choices.len(), if style == "chaos" { 6 } else { 3 });
        }
    }

    #[test]
    fn hidden_initial_offer_can_continue_into_the_observed_public_acquisition() {
        for style in ["normal", "chaos"] {
            let source = source_host(style, 19);
            let initial_public = source.state().try_observe(Color::Black).unwrap();
            let particle = sample_initial_public(
                GameConfig {
                    game_style: style.into(),
                    ..GameConfig::default()
                },
                serde_json::to_value(initial_public).unwrap(),
                71,
            )
            .unwrap();
            let (source_action, source_after) = first_step(&source);
            let acquired =
                crate::draft::selected_draft_cards(source.state(), source_action.action())
                    .unwrap()
                    .iter()
                    .map(|card| card["id"].clone())
                    .collect::<Vec<_>>();
            let observed = source_after
                .position
                .state()
                .try_observe(Color::Black)
                .unwrap();
            let hidden = condition_hidden_opening_draft(
                &particle,
                serde_json::to_value(&observed).unwrap(),
                93,
            )
            .unwrap();
            let proposal_action = v7_adapter_actions::legal_public_intents(&hidden.position)
                .unwrap()
                .into_iter()
                .find(|intent| {
                    let action: crate::Action = serde_json::from_value(intent.clone()).unwrap();
                    crate::draft::selected_draft_cards(hidden.position.state(), &action)
                        .unwrap()
                        .iter()
                        .map(|card| card["id"].clone())
                        .collect::<Vec<_>>()
                        == acquired
                })
                .expect("hidden proposal must retain a source-valid observed acquisition");
            let admitted =
                v7_adapter_actions::bind_public_intent(&hidden.position, proposal_action).unwrap();
            let applied = apply_weighted_conditioned_public(
                &hidden.position,
                &admitted,
                serde_json::to_value(&observed).unwrap(),
                97,
            )
            .unwrap();
            assert_eq!(
                applied
                    .applied
                    .position
                    .state()
                    .try_observe(Color::Black)
                    .unwrap(),
                observed,
                "{style}"
            );
            assert_eq!(applied.applied.position.state().history.len(), 1);
            let composed = hidden.importance_weight * applied.importance_weight;
            let ratio = (hidden.source_probability * applied.source_probability)
                / (hidden.proposal_probability * applied.proposal_probability);
            assert!(
                (composed - ratio).abs() <= ratio * 1e-12,
                "{style} two-step density differs: {composed} versus {ratio}"
            );
        }
    }
}
