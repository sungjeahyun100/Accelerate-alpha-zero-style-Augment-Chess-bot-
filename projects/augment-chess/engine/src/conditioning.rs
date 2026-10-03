//! Conditional initial chance and public identity binding. This module accepts
//! public frames only; particle filtering, resampling and tree search are callers'
//! responsibilities. No actual private position, seed or future RNG is an input.
use crate::*;
use serde_json::Value;
use std::collections::BTreeMap;

fn same_content<T: serde::Serialize>(left: &T, right: &T) -> Result<bool> {
    let canonical = |value| {
        serde_jcs::to_vec(value).map_err(|error| {
            EngineError::Serialization(format!("public canonicalization: {error}"))
        })
    };
    Ok(canonical(left)? == canonical(right)?)
}

fn checked_observation(value: Value) -> Result<Observation> {
    crate::state::validate_json_value(&value, 0)?;
    let object = value
        .as_object()
        .ok_or_else(|| EngineError::InvalidState("public observation must be an object".into()))?;
    let fields = [
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
    if object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field)) {
        return Err(EngineError::InvalidState(
            "public observation has unknown or missing fields".into(),
        ));
    }
    let mut observation: Observation =
        serde_json::from_value(value).map_err(EngineError::serialization)?;
    if observation.protocol_version != crate::state::observation_protocol()
        || observation
            .public_state
            .get("projectionVersion")
            .and_then(Value::as_str)
            != Some(crate::state::observation_projection())
        || observation
            .public_state
            .get("observationPolicyHash")
            .and_then(Value::as_str)
            != Some(crate::state::observation_policy_hash())
        || observation.board.len() != 8
        || observation.board.iter().any(|row| row.len() != 8)
    {
        return Err(EngineError::InvalidState(
            "invalid public observation protocol or board".into(),
        ));
    }
    crate::observation::validate_projection(&observation)?;
    let supplied = observation.information_state_key.clone();
    observation.refresh_key();
    if observation.information_state_key != supplied {
        return Err(EngineError::InvalidState(
            "public observation informationStateKey mismatch".into(),
        ));
    }
    Ok(observation)
}

pub(crate) fn sample_initial(config: GameConfig, expected: Value, seed: u32) -> Result<Position> {
    let expected = checked_observation(expected)?;
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
            "conditioning requires the initial public frame, before any card acquisition".into(),
        ));
    }
    let sampled = Position::new_game(config.clone(), u64::from(seed))?;
    let mut state = sampled.state().clone();
    if !config.draft_delete {
        if config.game_style == "grand" {
            let draft = expected.public_state.get("draft").ok_or_else(|| {
                EngineError::InvalidState("initial grand pool must be public".into())
            })?;
            let choices = draft
                .get("choices")
                .and_then(Value::as_array)
                .ok_or_else(|| EngineError::InvalidState("public grand pool is missing".into()))?;
            crate::draft::condition_grand_initial_choices(&mut state, choices)?;
        } else {
            if let Some(draft) = expected.public_state.get("draft") {
                if draft.get("color").and_then(Value::as_str) != Some(expected.viewer.as_str()) {
                    return Err(EngineError::InvalidState(
                        "initial private opposing draft may not be supplied as public".into(),
                    ));
                }
                let choices = draft
                    .get("choices")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        EngineError::InvalidState("own initial draft choices missing".into())
                    })?;
                crate::draft::condition_initial_offer(&mut state, choices)?;
            } else if expected.viewer == state.decision_actor() {
                return Err(EngineError::InvalidState(
                    "own initial draft phase must be public".into(),
                ));
            }
        }
    }
    let conditioned = sampled.with_state(state)?;
    let actual = conditioned.try_observe(expected.viewer)?;
    if !same_content(&actual, &expected)? {
        return Err(EngineError::ConditioningMismatch(
            "initial public frame is impossible under the supplied source configuration".into(),
        ));
    }
    Ok(conditioned)
}

/// A caller-owned particle executes one source action. Only a genuinely new
/// public OPENING offer can change its draw outcome here; existing public
/// cards/offers and ordinary action effects remain exact. Full public-frame
/// comparison still decides acceptance, and future randomness is independent.
pub(crate) fn apply_conditioned(
    position: &Position,
    action: &Action,
    expected: Value,
    seed: u32,
) -> Result<StepResult> {
    let expected = checked_observation(expected)?;
    let mut step = position.apply(action)?;
    let conditioned = match condition_identities_checked(&step.position, &expected) {
        Ok(position) => position,
        Err(EngineError::ConditioningMismatch(_)) => {
            let old_draft = position.state().extra.get("draft");
            let new_draft = step.position.state().extra.get("draft");
            let visible_draft = expected.public_state.get("draft");
            let new_opening = matches!(
                action.kind,
                ActionKind::DraftPick | ActionKind::DraftBundlePick
            ) && position.state().mode == "draft"
                && step.position.state().mode == "draft"
                && old_draft
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
                && visible_draft
                    .and_then(|draft| draft.get("color"))
                    .and_then(Value::as_str)
                    == Some(expected.viewer.as_str())
                && expected.viewer == Color::Black;
            if !new_opening {
                return Err(EngineError::ConditioningMismatch(
                    "past public transition differs beyond supported identities/new OPENING draw"
                        .into(),
                ));
            }
            let choices = visible_draft
                .and_then(|draft| draft.get("choices"))
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    EngineError::InvalidState("public opening choices missing".into())
                })?;
            let mut state = step.position.state().clone();
            crate::draft::condition_opening_offer(&mut state, choices, Color::Black)?;
            let proposed = step.position.with_state(state)?;
            condition_identities_checked(&proposed, &expected)?
        }
        Err(error) => return Err(error),
    };
    let mut state = conditioned.state().clone();
    state.rng = RngState::seeded(u64::from(seed));
    step.position = conditioned.with_state(state)?;
    Ok(step)
}

/// The weighted boundary never substitutes a compatibility-only forced draw
/// for a density. Ordinary supported transitions are sampled from the source
/// kernel, so their trace p/q is one. The observed normal Black initial offer
/// uses an explicit latent balancing-trace mixture with source probabilities.
pub(crate) fn apply_weighted_conditioned(
    position: &Position,
    action: &Action,
    expected: Value,
    seed: u32,
) -> Result<ConditionedStepProposal> {
    let expected = checked_observation(expected)?;
    if action.kind == ActionKind::Card {
        position.validate_action(action)?;
        let card = position
            .state()
            .deck_slots
            .get(action.color)
            .iter()
            .find(|card| action.card_instance_id.as_deref() == Some(card.instance_id.as_str()))
            .ok_or(EngineError::IllegalAction)?;
        // These canonical effects record every semantic choice via sample_choice.
        // Source identities and outcome-invariant availability probes are kept
        // in the RNG stream but marginalized from the semantic event density.
        if !matches!(
            card.effect.as_str(),
            "nullification" | "otherworld" | "suspiciousPotion" | "enPassantBang"
        ) {
            return Err(EngineError::UnsupportedFeature(
                "weighted card chance trace".into(),
            ));
        }
    }
    let mut traced_state = position.state().clone();
    traced_state.semantic_chance_probability = Some(1.0);
    let mut step = position.with_state(traced_state)?.apply(action)?;
    let semantic_probability = step
        .position
        .state()
        .semantic_chance_probability
        .ok_or_else(|| EngineError::InvalidState("missing owned semantic chance trace".into()))?;
    let mut completed = step.position.state().clone();
    completed.semantic_chance_probability = None;
    step.position = step.position.with_state(completed)?;
    let old_draft = position.state().extra.get("draft");
    let new_draft = step.position.state().extra.get("draft");
    let visible_draft = expected.public_state.get("draft");
    let new_offer = matches!(
        action.kind,
        ActionKind::DraftPick | ActionKind::DraftBundlePick
    ) && position.state().mode == "draft"
        && step.position.state().mode == "draft"
        && old_draft
            .and_then(|draft| draft.get("kind"))
            .and_then(Value::as_str)
            != Some("grand")
        && new_draft
            .and_then(|draft| draft.get("kind"))
            .and_then(Value::as_str)
            != Some("grand")
        && old_draft
            .and_then(|draft| draft.get("color"))
            .and_then(Value::as_str)
            == Some("white")
        && new_draft
            .and_then(|draft| draft.get("color"))
            .and_then(Value::as_str)
            == Some("black");
    let (conditioned, source_probability, proposal_probability) = if new_offer {
        if new_draft
            .and_then(|draft| draft.get("phase"))
            .and_then(Value::as_str)
            != Some("OPENING")
            || position.state().move_count != 0
        {
            return Err(EngineError::UnsupportedFeature(
                "weighted noninitial draft trace".into(),
            ));
        }
        let chaos = step
            .position
            .state()
            .extra
            .get("gameStyle")
            .and_then(Value::as_str)
            == Some("chaos");
        check_standard_opening_board(step.position.state())?;
        let mut state = step.position.state().clone();
        state.rng = RngState::seeded(u64::from(seed));
        let (p, q) = if expected.viewer == Color::Black {
            let choices = visible_draft
                .filter(|draft| draft["color"] == "black")
                .and_then(|draft| draft.get("choices"))
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    EngineError::ConditioningMismatch("new own opening offer must be public".into())
                })?;
            if chaos {
                crate::draft::propose_observed_chaos_offer(&mut state, choices, Color::Black)?
            } else {
                crate::draft::propose_observed_normal_offer(&mut state, choices, Color::Black)?
            }
        } else {
            if chaos {
                crate::draft::propose_unobserved_chaos_offer(&mut state, Color::Black)?
            } else {
                crate::draft::propose_unobserved_normal_offer(&mut state, Color::Black)?
            }
        };
        let proposed = step.position.with_state(state)?;
        // This compares every public field and every viewer-scoped history
        // event, so a forced component selecting a different best attempt is
        // rejected rather than changing the source's balance decision.
        (condition_identities_checked(&proposed, &expected)?, p, q)
    } else {
        // The ordinary proposal samples the same source semantic kernel. Its
        // realized outcome density appears in both p and q, including cards.
        (
            condition_identities_checked(&step.position, &expected)?,
            semantic_probability,
            semantic_probability,
        )
    };
    let importance_weight = checked_density(source_probability, proposal_probability)?;
    let mut state = conditioned.state().clone();
    state.rng = RngState::seeded(u64::from(seed));
    step.position = conditioned.with_state(state)?;
    Ok(ConditionedStepProposal {
        step,
        importance_weight,
        source_probability,
        proposal_probability,
    })
}

fn checked_density(p: f64, q: f64) -> Result<f64> {
    let weight = p / q;
    if !p.is_finite()
        || !q.is_finite()
        || p <= 0.0
        || q <= 0.0
        || p > 1.0
        || q > 1.0
        || !weight.is_finite()
        || weight <= 0.0
    {
        return Err(EngineError::InvalidState(
            "invalid source/proposal trace density".into(),
        ));
    }
    Ok(weight)
}

fn check_standard_opening_board(state: &GameState) -> Result<()> {
    // Initial standard pieces make source's stochastic trolley/black-box pool
    // eligibility tests outcome-invariant. Edited or progressed boards require
    // their additional latent predicate trace and are not given guessed p/q.
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
            match (
                state.at(Square {
                    row: row as u8,
                    col: col as u8,
                }),
                kind,
            ) {
                (None, None) => {}
                (Some(piece), Some(kind))
                    if piece.kind == kind
                        && piece.color
                            == if row < 2 {
                                PieceColor::Black
                            } else {
                                PieceColor::White
                            }
                        && !piece.moved => {}
                _ => {
                    return Err(EngineError::UnsupportedFeature(
                        "initial-offer density on a modified board".into(),
                    ));
                }
            }
        }
    }
    Ok(())
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
            .ok_or_else(|| EngineError::InvalidState("public opposing cards missing".into()))
    }
}

/// A source-proven acquisition constraint, evaluated before any effect. It
/// eliminates unrelated hidden picks without swallowing errors from possibly
/// compatible, still unsupported branches. Other action families are unknown
/// to this predicate and remain candidates.
pub(crate) fn transition_compatible(
    position: &Position,
    action: &Action,
    expected: Value,
) -> Result<bool> {
    let expected = checked_observation(expected)?;
    position.validate_action(action)?;
    if action.kind == ActionKind::Card {
        let (slot, card) = position
            .state()
            .deck_slots
            .get(action.color)
            .iter()
            .enumerate()
            .find(|(_, card)| action.card_instance_id.as_deref() == Some(card.instance_id.as_str()))
            .ok_or(EngineError::IllegalAction)?;
        // A directly played Guard only transforms the pawn in front of the
        // king (main103983). Guard is absent from finishCard's turn-ending
        // list (main86914), so its play-mode card action cannot advance the
        // turn or move count. A first board move may auto-activate that same
        // card (main87905): the used slot alone cannot distinguish the two.
        // Leave editor overrides and non-play outcomes to the full kernel.
        if card.effect == "guard"
            && position.state().mode == "play"
            && expected.public_state.get("mode").and_then(Value::as_str) == Some("play")
            && !crate::observation::truth(
                position.state().extra.get("simpleBoardEditorCardOverride"),
            )
            && (expected.turn != action.color
                || crate::observation::number(expected.public_state.get("moveCount"))
                    .is_some_and(|count| count != position.state().move_count as f64))
        {
            return Ok(false);
        }
        // finishCard86949 marks every successful non-dev instance used before
        // exposing the transition. A play frame leaving that public slot
        // unused proves this candidate impossible; it remains in the caller's
        // uniform intent denominator. Unknown/dev contexts stay candidates.
        if card
            .extra
            .get("devCard")
            .is_none_or(|value| matches!(value, Value::Null | Value::Bool(false)))
        {
            let after = cards_for_actor(&expected, action.color)?;
            return Ok(after.iter().any(|public| {
                public["id"] == card.id
                    && crate::observation::number(public.get("slot")) == Some(slot as f64)
                    && public["used"] == true
            }));
        }
        return Ok(true);
    }
    if !matches!(
        action.kind,
        ActionKind::DraftPick | ActionKind::DraftBundlePick
    ) {
        return Ok(true);
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
        .and_then(|v| v.get("kind"))
        .and_then(Value::as_str)
        == Some("grand");
    for (definition, slot) in selected.iter().zip(slots) {
        let Some(public) = after
            .iter()
            .find(|card| crate::observation::number(card.get("slot")) == Some(slot as f64))
        else {
            return Ok(false);
        };
        if public["id"] != definition["id"]
            || grand && public["instanceId"] != definition["instanceId"]
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Propose a source-compatible latent initial offer for the opposing viewer.
/// Only the not-yet-public offer, its private notice and independent chance
/// stream may change. The complete previous public frame must remain equal.
pub(crate) fn hidden_opening(
    position: &Position,
    expected: Value,
    seed: u32,
) -> Result<HiddenDraftProposal> {
    let expected = checked_observation(expected)?;
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
            "hidden offer conditioning requires an initial OPENING decision".into(),
        ));
    }
    check_standard_opening_board(state)?;
    let before = position.try_observe(expected.viewer)?;
    if before.public_state.contains_key("draft") {
        return Err(EngineError::ConditioningMismatch(
            "already-public draft may not be proposed again".into(),
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
            "initial acquisition must match the source slot count and order".into(),
        ));
    }
    let required = acquired
        .iter()
        .map(|card| {
            card.get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| EngineError::InvalidState("acquired definition missing".into()))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut proposed = state.clone();
    proposed.rng = RngState::seeded(u64::from(seed));
    let (source_probability, proposal_probability) = if chaos {
        crate::draft::propose_hidden_chaos_offer(&mut proposed, &required, actor)?
    } else {
        crate::draft::propose_hidden_normal_offer(&mut proposed, required[0], actor)?
    };
    let importance_weight = checked_density(source_probability, proposal_probability)?;
    let position = position.with_state(proposed)?;
    if !same_content(&position.try_observe(expected.viewer)?, &before)? {
        return Err(EngineError::ConditioningMismatch(
            "hidden proposal altered previously public information".into(),
        ));
    }
    Ok(HiddenDraftProposal {
        position,
        importance_weight,
        source_probability,
        proposal_probability,
    })
}

fn card_identity_map(
    current: &[Value],
    expected: &[Value],
    map: &mut BTreeMap<String, String>,
) -> Result<()> {
    if current.len() != expected.len() {
        return Err(EngineError::ConditioningMismatch(
            "public card counts changed during identity binding".into(),
        ));
    }
    for (current, expected) in current.iter().zip(expected) {
        let old = current
            .get("instanceId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                EngineError::InvalidState("sampled public card identity missing".into())
            })?;
        let new = expected
            .get("instanceId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or_else(|| {
                EngineError::InvalidState(
                    "conditioned public card identity missing or oversized".into(),
                )
            })?;
        let mut left = current.clone();
        let mut right = expected.clone();
        left.as_object_mut()
            .ok_or(EngineError::IllegalAction)?
            .remove("instanceId");
        right
            .as_object_mut()
            .ok_or(EngineError::IllegalAction)?
            .remove("instanceId");
        if !same_content(&left, &right)? {
            return Err(EngineError::ConditioningMismatch(
                "identity binding cannot alter card type, phase, status, slot or other semantics"
                    .into(),
            ));
        }
        if map.get(old).is_some_and(|prior| prior != new)
            || map.iter().any(|(key, value)| key != old && value == new)
        {
            return Err(EngineError::InvalidState(
                "public identity binding must be one-to-one".into(),
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
pub(crate) fn condition_identities(position: &Position, expected: Value) -> Result<Position> {
    let expected = checked_observation(expected)?;
    condition_identities_checked(position, &expected)
}

fn condition_identities_checked(position: &Position, expected: &Observation) -> Result<Position> {
    let current = position.try_observe(expected.viewer)?;
    let mut map = BTreeMap::new();
    card_identity_map(&current.own_cards, &expected.own_cards, &mut map)?;
    let array = |state: &Fields, field: &str| -> Result<Vec<Value>> {
        state
            .get(field)
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| EngineError::InvalidState(format!("public {field} missing")))
    };
    card_identity_map(
        &array(&current.public_state, "revealedOpponentCards")?,
        &array(&expected.public_state, "revealedOpponentCards")?,
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
                    .ok_or_else(|| EngineError::InvalidState("public draft choices missing".into()))
            };
            card_identity_map(&choices(left)?, &choices(right)?, &mut map)?;
        }
        (None, None) => {}
        _ => {
            return Err(EngineError::ConditioningMismatch(
                "identity binding cannot change the public draft phase".into(),
            ));
        }
    }
    // When all public identities already match, the remap is the identity
    // function. Comparing the complete frame still rejects semantic changes,
    // while retaining the caller's immutable Position without a full state
    // serialization, decode, and second observation projection.
    if map.iter().all(|(old, new)| old == new) {
        if same_content(&current, expected)? {
            return Ok(position.clone());
        }
        return Err(EngineError::ConditioningMismatch(
            "public frame differs beyond opaque identities".into(),
        ));
    }
    let mut raw = serde_json::to_value(position.state()).map_err(EngineError::serialization)?;
    // The metadata RNG is never relabeled or sampled here. Only existing opaque
    // identity strings and references to them are changed together.
    let rng = raw
        .as_object_mut()
        .expect("state object")
        .remove("rng")
        .expect("RNG metadata");
    remap(&mut raw, &map);
    raw["rng"] = rng;
    let state: GameState = serde_json::from_value(raw).map_err(EngineError::serialization)?;
    let conditioned = position.with_state(state)?;
    if !same_content(&conditioned.try_observe(expected.viewer)?, expected)? {
        return Err(EngineError::ConditioningMismatch(
            "public frame differs beyond opaque identities".into(),
        ));
    }
    Ok(conditioned)
}

/// A suffix must be a possible Number.toString(36).slice(2) output. This check
/// never recovers or installs the actual generator state behind a public ID.
pub(crate) fn validate_initial_identity(id: &str, identity: &str) -> Result<()> {
    let prefix = format!("{id}-");
    let suffix = identity.strip_prefix(&prefix).ok_or_else(|| {
        EngineError::InvalidState("initial card identity has a different definition prefix".into())
    })?;
    if suffix.len() > 2200
        || !suffix
            .bytes()
            .all(|digit| digit.is_ascii_digit() || digit.is_ascii_lowercase())
    {
        return Err(EngineError::InvalidState(
            "invalid initial random identity suffix".into(),
        ));
    }
    let mut value = 0.0_f64;
    for byte in suffix.bytes().rev() {
        let digit = if byte.is_ascii_digit() {
            byte - b'0'
        } else {
            byte - b'a' + 10
        };
        value = (value + f64::from(digit)) / 36.0;
    }
    let approximate = (value * 4_294_967_296.0).round() / 4_294_967_296.0;
    let mut candidates = vec![value, approximate];
    for offset in 1..=4 {
        if value.to_bits() >= offset {
            candidates.push(f64::from_bits(value.to_bits() - offset));
        }
        candidates.push(f64::from_bits(value.to_bits() + offset));
    }
    if candidates.into_iter().any(|candidate| {
        (0.0..1.0).contains(&candidate)
            && crate::draft::random_suffix(candidate).is_ok_and(|encoded| encoded == suffix)
    }) {
        Ok(())
    } else {
        Err(EngineError::InvalidState(
            "initial opaque identity is not a source random suffix".into(),
        ))
    }
}
