//! Frozen card metadata and the initial/draft phase state machine.
use crate::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    sync::OnceLock,
};

#[derive(Deserialize)]
pub(crate) struct Definitions {
    pub(crate) definitions: Vec<Value>,
    pub(crate) constants: Value,
}
pub(crate) fn definitions() -> &'static Definitions {
    static DATA: OnceLock<Definitions> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../bridge/catalog/card-definitions-20260927.json"
        ))
        .expect("adopted card definitions")
    })
}

/// Explicit source selection for callers entering the v7 rule path. The
/// legacy helper above keeps existing v6 draw and replay behavior unchanged.
pub(crate) fn definitions_for_ruleset(rules_version: &str) -> Result<&'static Definitions> {
    static V7: OnceLock<Result<Definitions>> = OnceLock::new();
    match rules_version {
        RULES_VERSION_V6 => Ok(definitions()),
        RULES_VERSION_V7 => V7
            .get_or_init(|| {
                let registry = crate::card_registry::registry_for(RULES_VERSION_V7)?;
                let mut source: Definitions = serde_json::from_str(include_str!(
                    "../../bridge/catalog/card-definitions-20260928.json"
                ))
                .map_err(EngineError::serialization)?;
                if source.definitions.len() != registry.cards.len() {
                    return Err(EngineError::InvalidState(
                        "v7 presented card count mismatch".into(),
                    ));
                }
                for card in &mut source.definitions {
                    let id = card["id"].as_str().ok_or_else(|| {
                        EngineError::InvalidState("v7 card definition ID missing".into())
                    })?;
                    *card = registry.get(id)?.source_definition.clone();
                }
                Ok(source)
            })
            .as_ref()
            .map_err(Clone::clone),
        other => Err(EngineError::UnsupportedFeature(format!(
            "card definitions for rules version {other}"
        ))),
    }
}
fn timestamp_from_catalog(source: &str) -> std::result::Result<i64, String> {
    let catalog: Value = serde_json::from_str(source).map_err(|e| e.to_string())?;
    let text = catalog["source"]["frozenAt"]
        .as_str()
        .ok_or("freeze timestamp missing")?;
    let number = |start: usize, end: usize| {
        text.get(start..end)
            .ok_or("invalid freeze timestamp")?
            .parse::<i64>()
            .map_err(|e| e.to_string())
    };
    let year = number(0, 4)?;
    let month = number(5, 7)?;
    let day = number(8, 10)?;
    let adjusted = year - i64::from(month <= 2);
    let era = adjusted.div_euclid(400);
    let yoe = adjusted - era * 400;
    let shifted = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * shifted + 2) / 5 + day - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    Ok(
        (((days * 24 + number(11, 13)?) * 60 + number(14, 16)?) * 60 + number(17, 19)?) * 1000
            + number(20, 23)?,
    )
}
pub(crate) fn frozen_timestamp_for_ruleset(rules_version: &str) -> Result<i64> {
    static V6: OnceLock<std::result::Result<i64, String>> = OnceLock::new();
    static V7: OnceLock<std::result::Result<i64, String>> = OnceLock::new();
    let timestamp = match rules_version {
        RULES_VERSION_V6 => V6.get_or_init(|| {
            timestamp_from_catalog(include_str!("../../bridge/catalog/site-20260927.json"))
        }),
        RULES_VERSION_V7 => V7.get_or_init(|| {
            timestamp_from_catalog(include_str!("../../bridge/catalog/site-20260928.json"))
        }),
        other => {
            return Err(EngineError::UnsupportedFeature(format!(
                "frozen timestamp for rules version {other}"
            )));
        }
    };
    timestamp.clone().map_err(EngineError::InvalidState)
}
#[derive(Deserialize)]
struct Weights {
    constants: Value,
    weights: Vec<Weight>,
}
#[derive(Deserialize)]
struct Weight {
    id: String,
    phase: String,
    weight: f64,
    #[serde(rename = "openingWeight")]
    opening_weight: f64,
}
fn weights() -> &'static Weights {
    static DATA: OnceLock<Weights> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../bridge/catalog/draft-20260927.json"))
            .expect("adopted draft weights")
    })
}
fn weights_for_ruleset(rules_version: &str) -> Result<&'static Weights> {
    static V7: OnceLock<Weights> = OnceLock::new();
    match rules_version {
        RULES_VERSION_V6 => Ok(weights()),
        RULES_VERSION_V7 => Ok(V7.get_or_init(|| {
            serde_json::from_str(include_str!("../../bridge/catalog/draft-20260928.json"))
                .expect("adopted v7 draft weights")
        })),
        other => Err(EngineError::UnsupportedFeature(format!(
            "draft weights for rules version {other}"
        ))),
    }
}

/// The weight table is source data, separate from the hand-card definition.
/// A missing ID is rejected rather than silently assigned zero draw weight.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "typed draft weight query awaits v7 RULE selection integration"
    )
)]
pub(crate) fn draft_weight_for_ruleset(
    rules_version: &str,
    card_id: &str,
    opening: bool,
) -> Result<(String, f64)> {
    let entry = weights_for_ruleset(rules_version)?
        .weights
        .iter()
        .find(|entry| entry.id == card_id)
        .ok_or_else(|| EngineError::InvalidState(format!("draft weight missing for {card_id}")))?;
    let weight = if opening {
        entry.opening_weight
    } else {
        entry.weight
    };
    if !weight.is_finite() || weight < 0.0 {
        return Err(EngineError::InvalidState(format!(
            "draft weight invalid for {card_id}"
        )));
    }
    Ok((entry.phase.clone(), weight))
}
fn weights_by_id() -> &'static HashMap<&'static str, &'static Weight> {
    static DATA: OnceLock<HashMap<&'static str, &'static Weight>> = OnceLock::new();
    DATA.get_or_init(|| {
        let mut by_id = HashMap::with_capacity(weights().weights.len());
        for weight in &weights().weights {
            // The source lookup uses find(), so duplicate IDs keep their first entry.
            by_id.entry(weight.id.as_str()).or_insert(weight);
        }
        by_id
    })
}
fn weights_by_id_v7() -> &'static HashMap<&'static str, &'static Weight> {
    static DATA: OnceLock<HashMap<&'static str, &'static Weight>> = OnceLock::new();
    DATA.get_or_init(|| {
        let weights = weights_for_ruleset(RULES_VERSION_V7).expect("adopted v7 draft weights");
        let mut by_id = HashMap::with_capacity(weights.weights.len());
        for weight in &weights.weights {
            // Source find() chooses the first definition if IDs repeat.
            by_id.entry(weight.id.as_str()).or_insert(weight);
        }
        by_id
    })
}
fn weight_for_card(rules_version: &str, card: &Value) -> Option<&'static Weight> {
    card.get("id")
        .and_then(Value::as_str)
        .and_then(|id| match rules_version {
            RULES_VERSION_V6 => weights_by_id().get(id).copied(),
            RULES_VERSION_V7 => weights_by_id_v7().get(id).copied(),
            _ => None,
        })
}
pub(crate) fn category_with_ruleset(rules_version: &str, card: &Value) -> &'static str {
    weight_for_card(rules_version, card)
        .map(|weight| weight.phase.as_str())
        .unwrap_or("")
}
fn weight_with_ruleset(rules_version: &str, card: &Value, opening: bool) -> f64 {
    weight_for_card(rules_version, card)
        .map(|weight| {
            if opening {
                weight.opening_weight
            } else {
                weight.weight
            }
        })
        .unwrap_or(0.0)
}

// Bounded fractional radix conversion uses the f64 interval and rounds the
// last digit to even, matching Number.toString(36) for the RNG domain [0,1).
// Mathematical reference: V8 src/numbers/conversions.cc DoubleToRadixStringView.
pub(crate) fn random_suffix(value: f64) -> Result<String> {
    if !value.is_finite() || !(0.0..1.0).contains(&value) {
        return Err(EngineError::InvalidState(
            "random identity value outside [0,1)".into(),
        ));
    }
    if value == 0.0 {
        return Ok(String::new());
    }
    let mut fraction = value;
    let mut delta = ((f64::from_bits(value.to_bits() + 1) - value) * 0.5).max(f64::from_bits(1));
    let mut digits = Vec::<u8>::new();
    for _ in 0..2200 {
        fraction *= 36.0;
        delta *= 36.0;
        let digit = fraction.floor() as u8;
        digits.push(digit);
        fraction -= f64::from(digit);
        if (fraction > 0.5 || (fraction == 0.5 && digit % 2 == 1)) && fraction + delta > 1.0 {
            while let Some(last) = digits.pop() {
                if last < 35 {
                    digits.push(last + 1);
                    break;
                }
            }
            break;
        }
        if fraction < delta {
            break;
        }
    }
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
    Ok(digits
        .into_iter()
        .map(|digit| char::from(alphabet[usize::from(digit)]))
        .collect())
}

fn clone_card(state: &mut GameState, card: &Value) -> Result<Value> {
    let mut card = card.clone();
    let suffix = random_suffix(state.rng.sample()?)?;
    card["instanceId"] = json!(format!(
        "{}-{suffix}",
        card["id"].as_str().expect("definition id")
    ));
    Ok(card)
}
fn weighted_pick<'a>(
    state: &mut GameState,
    pool: &'a [&'a Value],
    opening: bool,
) -> Result<Option<&'a Value>> {
    if pool.is_empty() {
        return Ok(None);
    }
    let rules_version = state.ruleset_id.clone();
    let sum = pool
        .iter()
        .map(|card| weight_with_ruleset(&rules_version, card, opening))
        .sum::<f64>();
    let random = state.rng.sample()?;
    if sum <= 0.0 {
        return Ok(pool
            .get((random * pool.len() as f64).floor() as usize)
            .copied());
    }
    let mut roll = random * sum;
    for &card in pool {
        roll -= weight_with_ruleset(&rules_version, card, opening);
        if roll <= 0.0 {
            return Ok(Some(card));
        }
    }
    Ok(pool.last().copied())
}
pub(crate) fn conflicts_with_ruleset(
    rules_version: &str,
    id: &str,
    unavailable: &BTreeSet<String>,
) -> bool {
    let definitions = match rules_version {
        RULES_VERSION_V6 => definitions(),
        RULES_VERSION_V7 => {
            definitions_for_ruleset(RULES_VERSION_V7).expect("adopted v7 definitions")
        }
        _ => return true,
    };
    definitions.constants["LATEST_MUTUALLY_EXCLUSIVE_DRAFT_CARD_GROUPS"]
        .as_array()
        .expect("conflict groups")
        .iter()
        .any(|group| {
            group
                .as_array()
                .expect("conflict group")
                .iter()
                .any(|value| value.as_str() == Some(id))
                && group
                    .as_array()
                    .expect("group")
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|other| other != id && unavailable.contains(other))
        })
}
fn grand_conflicts(rules_version: &str, id: &str, unavailable: &BTreeSet<String>) -> bool {
    conflicts_with_ruleset(rules_version, id, unavailable)
        || matches!(id, "democracy" | "queens-gambit")
            && unavailable.contains(if id == "democracy" {
                "queens-gambit"
            } else {
                "democracy"
            })
}
fn shuffled(state: &mut GameState, items: &mut [Value]) -> Result<()> {
    for index in (1..items.len()).rev() {
        let destination = (state.rng.sample()? * (index + 1) as f64).floor() as usize;
        items.swap(index, destination);
    }
    Ok(())
}
fn grand_pool(state: &mut GameState) -> Result<Vec<Value>> {
    let mut unavailable = BTreeSet::new();
    let mut choices = Vec::new();
    for (phase, count) in [("OPENING", 4), ("MIDDLE", 10), ("PIECE", 7), ("END", 7)] {
        let mut picked = Vec::new();
        for _ in 0..count {
            let pool = definitions_for_ruleset(&state.ruleset_id)?
                .definitions
                .iter()
                .filter(|card| {
                    category_with_ruleset(&state.ruleset_id, card) == phase
                        && card["id"] != "shotgun-king"
                        && !unavailable.contains(card["id"].as_str().expect("card id"))
                        && !grand_conflicts(
                            &state.ruleset_id,
                            card["id"].as_str().expect("card id"),
                            &unavailable,
                        )
                })
                .collect::<Vec<_>>();
            let Some(card) = weighted_pick(state, &pool, false)? else {
                break;
            };
            picked.push(clone_card(state, card)?);
            unavailable.insert(card["id"].as_str().expect("card id").into());
        }
        shuffled(state, &mut picked)?;
        choices.extend(picked);
    }
    Ok(choices)
}

fn acquired_ids(state: &GameState) -> BTreeSet<String> {
    [Color::White, Color::Black]
        .into_iter()
        .flat_map(|color| state.deck_slots.get(color))
        .filter(|card| !card.vacant)
        .map(|card| card.id.clone())
        .collect()
}
fn draw_mixed(
    state: &mut GameState,
    categories: &[&str],
    count: usize,
    color: Color,
    excluded: &BTreeSet<String>,
    opening: bool,
) -> Result<Vec<Value>> {
    let owned = acquired_ids(state);
    let mut unavailable = owned.clone();
    unavailable.extend(excluded.iter().cloned());
    let mut picked = Vec::new();
    for _ in 0..count {
        let pool = mixed_pool(state, categories, color, &unavailable)?;
        let Some(card) = weighted_pick(state, &pool, opening)? else {
            break;
        };
        picked.push(clone_card(state, card)?);
        unavailable.insert(card["id"].as_str().expect("id").into());
    }
    Ok(picked)
}

fn mixed_pool(
    state: &mut GameState,
    categories: &[&str],
    color: Color,
    unavailable: &BTreeSet<String>,
) -> Result<Vec<&'static Value>> {
    let mut pool = Vec::new();
    for card in &definitions_for_ruleset(&state.ruleset_id)?.definitions {
        let id = card["id"].as_str().expect("id");
        if id == "shotgun-king"
            || unavailable.contains(id)
            || conflicts_with_ruleset(&state.ruleset_id, id, unavailable)
            || !categories.contains(&category_with_ruleset(&state.ruleset_id, card))
        {
            continue;
        }
        if crate::eligibility::draft_drawable(state, card, color)? {
            pool.push(card);
        }
    }
    Ok(pool)
}

/// Normal initial OPENING importance proposal. Every sequence containing the
/// required acquired definition has positive proposal probability: early draws
/// exclude its mutually exclusive alternatives, and the last draw requires it
/// only when it has not already occurred. Source denominators retain those
/// alternatives. Thus p/q corrects the proposal without an unknown event
/// normalizing constant. This is not a uniform finite-LCG-seed posterior claim.
pub(crate) fn propose_hidden_normal_offer(
    state: &mut GameState,
    required: &str,
    color: Color,
) -> Result<(f64, f64)> {
    balanced_normal_proposal(state, color, |state, force| {
        normal_offer_trace(state, required, color, force)
    })
}

/// A proposal on the complete realized balancing trace. A uniformly chosen
/// latent attempt is forced to the observed ordered offer; all other attempts
/// are drawn from the source prior. The returned q is the mixture over all
/// three components, including components not reached after an early stop.
/// Every source trace selecting the observed offer has positive q, because
/// its selected attempt is one of these components. No best-of-three marginal
/// or unknown event-normalizing constant is substituted for this trace density.
pub(crate) fn propose_observed_normal_offer(
    state: &mut GameState,
    public: &[Value],
    color: Color,
) -> Result<(f64, f64)> {
    if public.len() != 3 {
        return Err(EngineError::InvalidState(
            "normal OPENING offer must contain three public cards".into(),
        ));
    }
    balanced_normal_proposal(state, color, |state, force| {
        observed_normal_trace(state, public, color, force)
    })
}

pub(crate) fn propose_unobserved_normal_offer(
    state: &mut GameState,
    color: Color,
) -> Result<(f64, f64)> {
    let (p, _) = balanced_normal_proposal(state, color, |state, _| {
        let mut unavailable = acquired_ids(state);
        let mut picked = Vec::new();
        let mut p = 1.0;
        for _ in 0..3 {
            let pool = mixed_pool(state, &["OPENING", "MIDDLE", "PIECE"], color, &unavailable)?;
            let total = pool
                .iter()
                .map(|card| weight_with_ruleset(&state.ruleset_id, card, true))
                .sum::<f64>();
            let selected = weighted_pick(state, &pool, true)?.ok_or_else(|| {
                EngineError::ConditioningMismatch("empty unconditional normal offer".into())
            })?;
            p *= weight_with_ruleset(&state.ruleset_id, selected, true) / total;
            picked.push(clone_card(state, selected)?);
            unavailable.insert(selected["id"].as_str().expect("id").to_owned());
        }
        Ok((picked, p, p))
    })?;
    // All latent components are exactly the same source prior, so this is
    // their algebraic mixture density, without an avoidable 3*p/3 rounding.
    Ok((p, p))
}

fn balanced_normal_proposal(
    state: &mut GameState,
    color: Color,
    mut draw: impl FnMut(&mut GameState, bool) -> Result<(Vec<Value>, f64, f64)>,
) -> Result<(f64, f64)> {
    let target = state
        .extra
        .get("draftBalance")
        .filter(|balance| color == Color::Black && balance["phase"] == "OPENING")
        .and_then(|balance| balance["averageScore"].as_f64());
    // For source's up-to-three balancing attempts, choose one latent attempt
    // to tilt. q is the MIXTURE density, not merely the density of the chosen
    // component: branches whose attempt is never reached are ordinary source
    // draws. This keeps every successful source acquisition trace in support.
    let force_index = if target.is_some() {
        (state.rng.sample()? * 3.0).floor() as usize
    } else {
        0
    };
    let mut traces = Vec::new();
    let mut best = Vec::new();
    let mut best_gap = f64::INFINITY;
    for attempt in 0..if target.is_some() { 3 } else { 1 } {
        let (candidate, p, q) = draw(state, attempt == force_index)?;
        traces.push((p, q));
        let gap = target
            .map(|target| (average_score(&candidate).unwrap_or(target) - target).abs())
            .unwrap_or(0.0);
        if gap < best_gap {
            best = candidate;
            best_gap = gap;
        }
        if best_gap <= 2.0 {
            break;
        }
    }
    let source_probability = traces.iter().map(|(p, _)| p).product::<f64>();
    let proposal_probability = if target.is_some() {
        (0..3)
            .map(|component| {
                traces
                    .iter()
                    .enumerate()
                    .map(|(attempt, (p, q))| if attempt == component { *q } else { *p })
                    .product::<f64>()
            })
            .sum::<f64>()
            / 3.0
    } else {
        traces[0].1
    };
    state
        .extra
        .get_mut("draft")
        .ok_or(EngineError::IllegalAction)?["choices"] = json!(best);
    state
        .extra
        .get_mut("openingAutoNoticeShown")
        .ok_or_else(|| EngineError::InvalidState("opening notice state missing".into()))?
        [color.as_str()] = json!(
        best.iter()
            .any(|card| category_with_ruleset(&state.ruleset_id, card) == "OPENING")
    );
    Ok((source_probability, proposal_probability))
}

fn observed_normal_trace(
    state: &mut GameState,
    public: &[Value],
    color: Color,
    force: bool,
) -> Result<(Vec<Value>, f64, f64)> {
    let mut unavailable = acquired_ids(state);
    let mut picked = Vec::new();
    let mut source_probability = 1.0;
    let mut matches = true;
    for observed in public {
        let source = mixed_pool(state, &["OPENING", "MIDDLE", "PIECE"], color, &unavailable)?;
        let source_sum = source
            .iter()
            .map(|card| weight_with_ruleset(&state.ruleset_id, card, true))
            .sum::<f64>();
        if !source_sum.is_finite() || source_sum <= 0.0 {
            return Err(EngineError::ConditioningMismatch(
                "empty observed OPENING source pool".into(),
            ));
        }
        let selected = if force {
            // The proposal fixes the semantic outcome; this ancillary uniform
            // draw keeps source clone/availability invocation order intact.
            state.rng.sample()?;
            source
                .iter()
                .copied()
                .find(|card| {
                    card["id"] == observed["id"]
                        && weight_with_ruleset(&state.ruleset_id, card, true) > 0.0
                })
                .ok_or_else(|| {
                    EngineError::ConditioningMismatch(
                        "observed OPENING sequence violates source eligibility or exclusives"
                            .into(),
                    )
                })?
        } else {
            weighted_pick(state, &source, true)?.ok_or_else(|| {
                EngineError::ConditioningMismatch("empty observed offer proposal".into())
            })?
        };
        source_probability *= weight_with_ruleset(&state.ruleset_id, selected, true) / source_sum;
        matches &= selected["id"] == observed["id"];
        picked.push(clone_card(state, selected)?);
        unavailable.insert(selected["id"].as_str().expect("id").to_owned());
    }
    Ok((picked, source_probability, if matches { 1.0 } else { 0.0 }))
}

fn normal_offer_trace(
    state: &mut GameState,
    required: &str,
    color: Color,
    force: bool,
) -> Result<(Vec<Value>, f64, f64)> {
    let mut unavailable = acquired_ids(state);
    let mut picked = Vec::new();
    let mut source_probability = 1.0;
    let mut proposal_probability = 1.0;
    for index in 0..3 {
        let source = mixed_pool(state, &["OPENING", "MIDDLE", "PIECE"], color, &unavailable)?;
        let source_sum = source
            .iter()
            .map(|card| weight_with_ruleset(&state.ruleset_id, card, true))
            .sum::<f64>();
        if !source_sum.is_finite() || source_sum <= 0.0 {
            return Err(EngineError::ConditioningMismatch(
                "empty hidden OPENING source pool".into(),
            ));
        }
        let already_selected = unavailable.contains(required);
        let proposal = source
            .iter()
            .copied()
            .filter(|card| {
                let id = card["id"].as_str().expect("id");
                if already_selected {
                    return true;
                }
                if index == 2 {
                    return id == required;
                }
                !conflicts_with_ruleset(
                    &state.ruleset_id,
                    required,
                    &BTreeSet::from([id.to_owned()]),
                )
            })
            .collect::<Vec<_>>();
        let proposal_sum = proposal
            .iter()
            .map(|card| weight_with_ruleset(&state.ruleset_id, card, true))
            .sum::<f64>();
        if force
            && (!proposal_sum.is_finite()
                || proposal_sum <= 0.0
                || !already_selected && !source.iter().any(|card| card["id"] == required))
        {
            return Err(EngineError::ConditioningMismatch(
                "observed acquisition has no source-valid hidden offer".into(),
            ));
        }
        let selected = weighted_pick(state, if force { &proposal } else { &source }, true)?
            .ok_or_else(|| {
                EngineError::ConditioningMismatch("empty hidden offer proposal".into())
            })?;
        let selected_weight = weight_with_ruleset(&state.ruleset_id, selected, true);
        source_probability *= selected_weight / source_sum;
        proposal_probability *= if proposal_sum > 0.0 && proposal.contains(&selected) {
            selected_weight / proposal_sum
        } else {
            0.0
        };
        picked.push(clone_card(state, selected)?);
        unavailable.insert(selected["id"].as_str().expect("id").to_owned());
    }
    Ok((picked, source_probability, proposal_probability))
}

// Keep a positive source-prior component: an observed final pair can originate
// from a replacement or deterministic swap, rather than the first two raw
// draws. Its source trace must never lose proposal support. The tilted branch
// is an efficiency choice; this exact mixture density corrects its bias.
const CHAOS_TILT_PROBABILITY: f64 = 0.95;

pub(crate) fn propose_hidden_chaos_offer(
    state: &mut GameState,
    required: &[&str],
    color: Color,
) -> Result<(f64, f64)> {
    if required.len() != 2 || required[0] == required[1] {
        return Err(EngineError::ConditioningMismatch(
            "CHAOS acquisition must contain two distinct definitions".into(),
        ));
    }
    balanced_normal_proposal(state, color, |state, force| {
        chaos_offer_trace(state, color, required, force)
    })
}

pub(crate) fn propose_observed_chaos_offer(
    state: &mut GameState,
    public: &[Value],
    color: Color,
) -> Result<(f64, f64)> {
    if public.len() != 6 {
        return Err(EngineError::InvalidState(
            "CHAOS OPENING offer must contain six cards".into(),
        ));
    }
    let ids = public
        .iter()
        .map(|card| {
            card["id"].as_str().ok_or_else(|| {
                EngineError::InvalidState("public CHAOS card definition missing".into())
            })
        })
        .collect::<Result<Vec<_>>>()?;
    balanced_normal_proposal(state, color, |state, force| {
        chaos_offer_trace(state, color, &ids, force)
    })
}

pub(crate) fn propose_unobserved_chaos_offer(
    state: &mut GameState,
    color: Color,
) -> Result<(f64, f64)> {
    let (p, _) = balanced_normal_proposal(state, color, |state, _| {
        chaos_offer_trace(state, color, &[], false)
    })?;
    Ok((p, p))
}

/// Full realized semantic draw trace, including every source replacement.
/// At most six original and six replacement draws occur in one attempt; the
/// source balancing loop reaches at most three attempts. No outcome-dependent
/// unbounded retries, seed search, or conditional-event normalizer is used.
fn chaos_offer_trace(
    state: &mut GameState,
    color: Color,
    ids: &[&str],
    force_attempt: bool,
) -> Result<(Vec<Value>, f64, f64)> {
    let force = force_attempt && !ids.is_empty() && state.rng.sample()? < CHAOS_TILT_PROBABILITY;
    let (choices, mut p, mut tilted) = mixed_draw_trace(
        state,
        &["OPENING", "MIDDLE", "PIECE"],
        6,
        color,
        &BTreeSet::new(),
        true,
        DrawTilt { ids, active: force },
    )?;
    let choices = arrange_chaos_with(state, choices, |state, excluded| {
        let (mut choices, source, _) = mixed_draw_trace(
            state,
            &["MIDDLE", "PIECE"],
            1,
            color,
            excluded,
            false,
            DrawTilt {
                ids: &[],
                active: false,
            },
        )?;
        p *= source;
        tilted *= source;
        Ok(choices.pop())
    })?;
    let q = if ids.is_empty() {
        p
    } else {
        CHAOS_TILT_PROBABILITY * tilted + (1.0 - CHAOS_TILT_PROBABILITY) * p
    };
    Ok((choices, p, q))
}

/// q_tilt is evaluated on this realized trace even when the sampled component
/// was the source prior. Forced outcomes consume their ancillary uniform draw
/// and clone nonce at the same source call boundary; those variables have the
/// same conditional density in both kernels and cancel in the ratio.
struct DrawTilt<'a> {
    ids: &'a [&'a str],
    active: bool,
}

fn mixed_draw_trace(
    state: &mut GameState,
    categories: &[&str],
    count: usize,
    color: Color,
    excluded: &BTreeSet<String>,
    opening: bool,
    tilt: DrawTilt<'_>,
) -> Result<(Vec<Value>, f64, f64)> {
    let mut unavailable = acquired_ids(state);
    unavailable.extend(excluded.iter().cloned());
    let mut choices = Vec::new();
    let mut p = 1.0;
    let mut tilted = 1.0;
    for index in 0..count {
        let pool = mixed_pool(state, categories, color, &unavailable)?;
        if pool.is_empty() {
            break;
        }
        let total = pool
            .iter()
            .map(|card| weight_with_ruleset(&state.ruleset_id, card, opening))
            .sum::<f64>();
        if !total.is_finite() || total <= 0.0 {
            return Err(EngineError::UnsupportedFeature(
                "nonpositive weighted CHAOS source pool".into(),
            ));
        }
        let selected = if tilt.active && index < tilt.ids.len() {
            state.rng.sample()?;
            pool.iter()
                .copied()
                .find(|card| {
                    card["id"] == tilt.ids[index]
                        && weight_with_ruleset(&state.ruleset_id, card, opening) > 0.0
                })
                .ok_or_else(|| {
                    EngineError::ConditioningMismatch(
                        "conditioned CHAOS sequence violates source draw predicates".into(),
                    )
                })?
        } else {
            weighted_pick(state, &pool, opening)?.expect("nonempty source pool")
        };
        let chance = weight_with_ruleset(&state.ruleset_id, selected, opening) / total;
        p *= chance;
        tilted *= if index < tilt.ids.len() {
            if selected["id"] == tilt.ids[index] {
                1.0
            } else {
                0.0
            }
        } else {
            chance
        };
        choices.push(clone_card(state, selected)?);
        unavailable.insert(selected["id"].as_str().expect("id").into());
    }
    if choices.len() < tilt.ids.len() {
        tilted = 0.0;
    }
    Ok((choices, p, tilted))
}

pub(crate) fn selected_draft_cards(state: &GameState, action: &Action) -> Result<Vec<Value>> {
    let choices = state
        .extra
        .get("draft")
        .and_then(|v| v.get("choices"))
        .and_then(Value::as_array)
        .ok_or(EngineError::IllegalAction)?;
    if action.kind == ActionKind::DraftBundlePick {
        let index = action
            .extra
            .get("bundleIndex")
            .and_then(Value::as_u64)
            .filter(|index| *index < 3)
            .ok_or(EngineError::IllegalAction)? as usize;
        return choices
            .get(index * 2..index * 2 + 2)
            .map(<[Value]>::to_vec)
            .ok_or(EngineError::IllegalAction);
    }
    choices
        .iter()
        .find(|card| card["instanceId"].as_str() == action.card_instance_id.as_deref())
        .map(|card| vec![card.clone()])
        .ok_or(EngineError::IllegalAction)
}
fn exclusive_opening(rules_version: &str, card: &Value) -> bool {
    let definitions = match rules_version {
        RULES_VERSION_V6 => definitions(),
        RULES_VERSION_V7 => {
            definitions_for_ruleset(RULES_VERSION_V7).expect("adopted v7 definitions")
        }
        _ => return true,
    };
    definitions.constants["EXCLUSIVE_OPENING_CARD_IDS"]
        .as_array()
        .expect("ids")
        .contains(&card["id"])
}
fn forbidden_bundle(rules_version: &str, first: &Value, second: &Value) -> bool {
    (category_with_ruleset(rules_version, first) == "OPENING"
        && category_with_ruleset(rules_version, second) == "OPENING")
        || (first["id"] == "democracy" && second["id"] == "queens-gambit")
        || (first["id"] == "queens-gambit" && second["id"] == "democracy")
}
fn arrange_chaos(state: &mut GameState, choices: Vec<Value>, color: Color) -> Result<Vec<Value>> {
    arrange_chaos_with(state, choices, |state, excluded| {
        Ok(draw_mixed(state, &["MIDDLE", "PIECE"], 1, color, excluded, false)?.pop())
    })
}

/// Source pairing and deterministic swaps are shared by ordinary draws and
/// density-carrying proposals. Only the weighted replacement draw is supplied
/// by the caller; the resulting pair order and exclusions are identical.
fn arrange_chaos_with(
    state: &mut GameState,
    mut choices: Vec<Value>,
    mut replacement: impl FnMut(&mut GameState, &BTreeSet<String>) -> Result<Option<Value>>,
) -> Result<Vec<Value>> {
    let rules_version = state.ruleset_id.clone();
    if choices.len() != 6 {
        return Ok(choices);
    }
    for start in (0..6).step_by(2) {
        if !forbidden_bundle(&rules_version, &choices[start], &choices[start + 1]) {
            continue;
        }
        let swap = (0..6).find(|&index| {
            if index / 2 == start / 2 {
                return false;
            }
            let partner = if index % 2 == 0 { index + 1 } else { index - 1 };
            !forbidden_bundle(&rules_version, &choices[start], &choices[index])
                && !forbidden_bundle(&rules_version, &choices[start + 1], &choices[partner])
        });
        if let Some(swap) = swap {
            choices.swap(start + 1, swap);
        } else {
            let excluded = choices
                .iter()
                .filter_map(|c| c["id"].as_str().map(str::to_owned))
                .collect();
            if let Some(card) = replacement(state, &excluded)? {
                choices[start + 1] = card;
            }
        }
    }
    for start in (0..6).step_by(2) {
        let Some(exclusive) =
            (start..start + 2).find(|&index| exclusive_opening(&rules_version, &choices[index]))
        else {
            continue;
        };
        let partner = if exclusive == start { start + 1 } else { start };
        if category_with_ruleset(&rules_version, &choices[partner]) != "OPENING" {
            continue;
        }
        let partner_exclusive = exclusive_opening(&rules_version, &choices[partner]);
        let swap = (0..6).find(|&index| {
            if index / 2 == start / 2
                || category_with_ruleset(&rules_version, &choices[index]) == "OPENING"
            {
                return false;
            }
            let source = index / 2 * 2;
            if choices[source..source + 2]
                .iter()
                .any(|card| exclusive_opening(&rules_version, card))
            {
                return false;
            }
            let other = if index % 2 == 0 { index + 1 } else { index - 1 };
            !partner_exclusive
                || category_with_ruleset(&rules_version, &choices[other]) != "OPENING"
        });
        if let Some(swap) = swap {
            choices.swap(partner, swap);
        } else {
            let excluded = choices
                .iter()
                .filter_map(|c| c["id"].as_str().map(str::to_owned))
                .collect();
            if let Some(card) = replacement(state, &excluded)? {
                choices[partner] = card;
            }
        }
    }
    Ok(choices)
}
fn draw_raw(state: &mut GameState, phase: &str, count: usize, color: Color) -> Result<Vec<Value>> {
    let mut excluded = BTreeSet::new();
    let mut choices = match phase {
        "OPENING" => draw_mixed(
            state,
            &["OPENING", "MIDDLE", "PIECE"],
            count,
            color,
            &excluded,
            true,
        )?,
        "MIDDLE" => {
            let pieces = ((count as f64 / 3.0).round() as usize).max(1);
            let mut choices = Vec::new();
            for (category, count) in [("MIDDLE", count - pieces), ("PIECE", pieces)] {
                for card in draw_mixed(state, &[category], count, color, &excluded, false)? {
                    excluded.insert(card["id"].as_str().expect("id").into());
                    choices.push(card);
                }
            }
            if choices.len() < count {
                choices.extend(draw_mixed(
                    state,
                    &["MIDDLE", "PIECE"],
                    count - choices.len(),
                    color,
                    &excluded,
                    false,
                )?);
            }
            choices
        }
        "END" => draw_mixed(state, &["MIDDLE", "END"], count, color, &excluded, false)?,
        other => draw_mixed(state, &[other], count, color, &excluded, false)?,
    };
    if state.extra.get("gameStyle").and_then(Value::as_str) == Some("chaos") {
        choices = arrange_chaos(state, choices, color)?;
    }
    Ok(choices)
}
fn average_score(choices: &[Value]) -> Option<f64> {
    (!choices.is_empty()).then(|| {
        choices
            .iter()
            .map(|card| (card["stars"].as_f64().unwrap_or(0.0) * 2.0).round())
            .sum::<f64>()
            / choices.len() as f64
    })
}
fn draw_choices(
    state: &mut GameState,
    phase: &str,
    count: usize,
    color: Color,
) -> Result<Vec<Value>> {
    let mut best = draw_raw(state, phase, count, color)?;
    let target = state
        .extra
        .get("draftBalance")
        .filter(|balance| color == Color::Black && balance["phase"] == phase)
        .and_then(|balance| balance["averageScore"].as_f64());
    let Some(target) = target else {
        return Ok(best);
    };
    if best.len() < 2 {
        return Ok(best);
    }
    let mut best_gap = (average_score(&best).unwrap_or(target) - target).abs();
    if best_gap <= 2.0 {
        return Ok(best);
    }
    for _ in 0..2 {
        let candidate = draw_raw(state, phase, count, color)?;
        if candidate.is_empty() {
            continue;
        }
        let gap = (average_score(&candidate).unwrap_or(target) - target).abs();
        if gap < best_gap {
            best = candidate;
            best_gap = gap;
            if best_gap <= 2.0 {
                break;
            }
        }
    }
    Ok(best)
}
pub(crate) fn start_draft(state: &mut GameState, color: Color, phase: &str) -> Result<()> {
    crate::flow::pause_clock(state)?;
    state.mode = "draft".into();
    state.turn = color;
    for field in ["draftLocked", "draftBoardPreview"] {
        state.extra.insert(field.into(), json!(false));
    }
    for field in [
        "draftPreviewCardId",
        "draftPreviewBundleIndex",
        "selected",
        "targeting",
        "wizardSpell",
    ] {
        state.extra.insert(field.into(), Value::Null);
    }
    for field in ["legalMoves", "wizardPreview", "shotgunPreview"] {
        state.extra.insert(field.into(), json!([]));
    }
    state.extra.insert("shotgunAction".into(), json!("move"));
    let count = if state.extra.get("gameStyle").and_then(Value::as_str) == Some("chaos") {
        6
    } else {
        3
    };
    let choices = draw_choices(state, phase, count, color)?;
    if choices.is_empty() {
        return Err(EngineError::UnsupportedFeature(
            "empty draft skip settlement".into(),
        ));
    }
    if phase == "OPENING"
        && state.move_count == 0
        && choices
            .iter()
            .any(|card| category_with_ruleset(&state.ruleset_id, card) == "OPENING")
    {
        state
            .extra
            .entry("openingAutoNoticeShown")
            .or_insert_with(|| json!({"white":false,"black":false}))[color.as_str()] = json!(true);
    }
    state.extra.insert(
        "draft".into(),
        json!({"color":color,"phase":phase,"choices":choices,"tutorial":false}),
    );
    start_draft_clock(state, color, None)?;
    Ok(())
}
fn start_draft_clock(state: &mut GameState, color: Color, grand_pick: Option<usize>) -> Result<()> {
    let previous = state
        .extra
        .get("draftClock")
        .cloned()
        .unwrap_or(Value::Null);
    let enabled = state
        .extra
        .get("clock")
        .and_then(|clock| clock.get("enabled"))
        .and_then(Value::as_bool)
        == Some(true);
    let configured = previous
        .get("initialMs")
        .and_then(Value::as_u64)
        .filter(|ms| [20000, 30000, 40000, 50000, 60000, 70000].contains(ms))
        .unwrap_or(40000);
    let preserve = grand_pick.is_none()
        && previous["enabled"] == true
        && color == Color::Black
        && previous["whiteSelected"] == true
        && previous["blackSelected"] != true;
    let mut clock = if preserve {
        previous
    } else {
        json!({"enabled":enabled,"initialMs":if enabled{json!(configured)}else{Value::Null},"whiteStartedAt":null,"blackStartedAt":null,"whiteSelected":false,"blackSelected":false,"whiteAutoSelected":false,"blackAutoSelected":false})
    };
    if let Some(pick) = grand_pick {
        clock["grandPickIndex"] = json!(pick.min(12));
    }
    if clock["enabled"] == true {
        clock[format!("{}StartedAt", color.as_str())] =
            json!(frozen_timestamp_for_ruleset(&state.ruleset_id)?);
    }
    state.extra.insert("draftClock".into(), clock);
    Ok(())
}
fn mark_draft_clock_selected(state: &mut GameState, color: Color) {
    if let Some(clock) = state.extra.get_mut("draftClock")
        && clock["enabled"] == true
    {
        clock[format!("{}Selected", color.as_str())] = json!(true);
        clock[format!("{}AutoSelected", color.as_str())] = json!(true);
    }
}
pub(crate) fn condition_grand_initial_choices(
    state: &mut GameState,
    public: &[Value],
) -> Result<()> {
    if public.len() != 28 {
        return Err(EngineError::InvalidState(
            "initial grand pool must contain 28 cards".into(),
        ));
    }
    let mut unavailable = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut conditioned = Vec::new();
    let mut index = 0;
    for (phase, count) in [("OPENING", 4), ("MIDDLE", 10), ("PIECE", 7), ("END", 7)] {
        for _ in 0..count {
            let view = &public[index];
            index += 1;
            let id = view
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| EngineError::InvalidState("public grand card id missing".into()))?;
            let definition = definitions_for_ruleset(&state.ruleset_id)?
                .definitions
                .iter()
                .find(|card| card["id"].as_str() == Some(id))
                .ok_or_else(|| {
                    EngineError::InvalidState(format!("unknown public grand card {id}"))
                })?;
            if id == "shotgun-king"
                || category_with_ruleset(&state.ruleset_id, definition) != phase
                || weight_with_ruleset(&state.ruleset_id, definition, false) <= 0.0
                || unavailable.contains(id)
                || grand_conflicts(&state.ruleset_id, id, &unavailable)
            {
                return Err(EngineError::InvalidState("public grand pool violates source categories, weights, uniqueness or exclusives".into()));
            }
            let identity = view
                .get("instanceId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EngineError::InvalidState("public grand card instance missing".into())
                })?;
            crate::conditioning::validate_initial_identity(id, identity)?;
            if !identities.insert(identity.to_owned()) {
                return Err(EngineError::InvalidState(
                    "duplicate public grand card identity".into(),
                ));
            }
            let mut card = definition.clone();
            card["instanceId"] = json!(identity);
            conditioned.push(card);
            unavailable.insert(id.to_owned());
        }
    }
    state
        .extra
        .get_mut("draft")
        .ok_or_else(|| EngineError::InvalidState("sampled grand phase missing".into()))?["choices"] =
        json!(conditioned);
    Ok(())
}

pub(crate) fn condition_initial_offer(state: &mut GameState, public: &[Value]) -> Result<()> {
    condition_opening_offer(state, public, Color::White)
}
/// Condition a supported past OPENING draw. Source eligibility is evaluated
/// for the actual drafting side, after any acquired opening effects.
pub(crate) fn condition_opening_offer(
    state: &mut GameState,
    public: &[Value],
    color: Color,
) -> Result<()> {
    let chaos = state.extra.get("gameStyle").and_then(Value::as_str) == Some("chaos");
    if public.len() != if chaos { 6 } else { 3 } {
        return Err(EngineError::InvalidState(
            "initial public offer count violates source mode".into(),
        ));
    }
    let mut unavailable = acquired_ids(state);
    let mut identities = BTreeSet::new();
    let mut conditioned = Vec::new();
    let mut probe = state.clone();
    for view in public {
        let id = view["id"]
            .as_str()
            .ok_or_else(|| EngineError::InvalidState("initial offer definition missing".into()))?;
        let definition = definitions_for_ruleset(&state.ruleset_id)?
            .definitions
            .iter()
            .find(|card| card["id"] == id)
            .ok_or_else(|| EngineError::InvalidState(format!("unknown initial offer {id}")))?;
        if id == "shotgun-king"
            || unavailable.contains(id)
            || conflicts_with_ruleset(&state.ruleset_id, id, &unavailable)
            || !matches!(
                category_with_ruleset(&state.ruleset_id, definition),
                "OPENING" | "MIDDLE" | "PIECE"
            )
            || weight_with_ruleset(&state.ruleset_id, definition, true) <= 0.0
            || !crate::eligibility::draft_drawable(&mut probe, definition, color)?
        {
            return Err(EngineError::ConditioningMismatch("initial offer violates source pool availability, categories, weights or exclusives".into()));
        }
        let identity = view["instanceId"]
            .as_str()
            .ok_or_else(|| EngineError::InvalidState("initial offer identity missing".into()))?;
        crate::conditioning::validate_initial_identity(id, identity)?;
        if !identities.insert(identity.to_owned()) {
            return Err(EngineError::InvalidState(
                "duplicate initial offer identity".into(),
            ));
        }
        let mut card = definition.clone();
        card["instanceId"] = json!(identity);
        conditioned.push(card);
        unavailable.insert(id.into());
    }
    if chaos
        && conditioned.chunks(2).any(|cards| {
            forbidden_bundle(&state.ruleset_id, &cards[0], &cards[1])
                || (cards
                    .iter()
                    .any(|card| exclusive_opening(&state.ruleset_id, card))
                    && cards
                        .iter()
                        .all(|card| category_with_ruleset(&state.ruleset_id, card) == "OPENING"))
        })
    {
        return Err(EngineError::ConditioningMismatch(
            "initial public chaos pair violates source bundle settlement".into(),
        ));
    }
    // Public conditioning changes supported past draw outcomes only. Predicate
    // probes use a clone; independent future RNG and hidden opposing offers are
    // not overwritten with actual private metadata.
    let opening = conditioned
        .iter()
        .any(|card| category_with_ruleset(&state.ruleset_id, card) == "OPENING");
    state
        .extra
        .get_mut("draft")
        .ok_or_else(|| EngineError::InvalidState("initial draft missing".into()))?["choices"] =
        json!(conditioned);
    state
        .extra
        .get_mut("openingAutoNoticeShown")
        .ok_or_else(|| EngineError::InvalidState("initial notice state missing".into()))?
        [color.as_str()] = json!(opening);
    Ok(())
}

pub(crate) fn initialize(config: GameConfig, seed: u64) -> Result<GameState> {
    initialize_for_ruleset(config, seed, RULES_VERSION_V6)
}

fn validate_v7_opening_rule_selection(ids: &[String], deathmatch_enabled: bool) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let constants = &weights_for_ruleset(RULES_VERSION_V7)?.constants;
    let pool = constants["CARD_CATEGORY_GROUPS"]["RULE"]
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("v7 RULE selection pool missing".into()))?;
    let deleted = constants["DELETED_CARD_IDS"]
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("v7 deleted card IDs missing".into()))?;
    if pool.len() != 26 {
        return Err(EngineError::InvalidState(
            "v7 RULE selection pool count drift".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id.as_str()) {
            return Err(EngineError::InvalidConfig(format!(
                "duplicate opening RULE selection {id}"
            )));
        }
        if !pool.iter().any(|candidate| candidate == id)
            || deleted.iter().any(|candidate| candidate == id)
            || id == "revelation" && !deathmatch_enabled
        {
            return Err(EngineError::InvalidConfig(format!(
                "opening RULE {id} unavailable in pinned source pool"
            )));
        }
        // An admitted selection may be chosen by the source's one random
        // index. Reject unsupported candidates before consuming live RNG.
        crate::card_registry::opening_rule_source_card(id)?;
    }
    Ok(())
}

fn activate_v7_opening_rule(state: &mut GameState, ids: &[String]) -> Result<()> {
    let created_at = frozen_timestamp_for_ruleset(RULES_VERSION_V7)?;
    let nonce = format!("rule-{created_at}-{}", random_suffix(state.rng.sample()?)?);
    let selected = (state.rng.sample()? * ids.len() as f64).floor() as usize;
    let card_id = ids.get(selected).ok_or_else(|| {
        EngineError::InvalidState("v7 opening RULE random selection outside pool".into())
    })?;
    crate::card_effects::apply_opening_rule_effect(state, card_id)?;
    let mut event_card = crate::card_registry::opening_rule_source_card(card_id)?;
    let card_name = event_card["name"]
        .as_str()
        .ok_or_else(|| EngineError::InvalidState("v7 opening RULE name missing".into()))?
        .to_owned();
    event_card["instanceId"] = json!(format!("{card_id}-{}", random_suffix(state.rng.sample()?)?));
    let mut applied = event_card.clone();
    applied["instanceId"] = json!(format!("applied-rule-{card_id}"));
    state.extra.insert("appliedRuleCard".into(), applied);
    state.extra.insert(
        "ruleOpeningEvent".into(),
        json!({
            "nonce":nonce,
            "createdAt":created_at,
            "dismissAt":created_at+3600,
            "status":"hit",
            "title":"RULE CARD",
            "card":event_card,
            "message":format!("{card_name} 카드가 발동됩니다."),
        }),
    );
    let notation = json!({
        "id":format!("opening-rule-{card_id}"),
        "kind":"card",
        "color":"white",
        "moveNumber":0,
        "text":format!("@{card_name}"),
        "description":format!("RULE {card_name} 적용"),
    });
    state
        .extra
        .get_mut("pendingNotations")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState("initial pending notations missing".into()))?
        .push(notation.clone());
    state.extra.insert("pendingNotation".into(), notation);
    crate::replay::add_log(state, format!("RULE 카드 발동: {card_name}"))?;
    Ok(())
}

pub(crate) fn initialize_for_ruleset(
    config: GameConfig,
    seed: u64,
    rules_version: &str,
) -> Result<GameState> {
    if !matches!(rules_version, RULES_VERSION_V6 | RULES_VERSION_V7) {
        return Err(EngineError::UnsupportedFeature(format!(
            "initial game rules version {rules_version}"
        )));
    }
    if rules_version == RULES_VERSION_V7 {
        crate::card_registry::registry_for(rules_version)?;
    }
    if seed > u64::from(u32::MAX)
        || !matches!(config.game_style.as_str(), "normal" | "chaos" | "grand")
        || config.star_win_limit == 0
        || config.deathmatch_limit_turns == 0
    {
        return Err(EngineError::InvalidConfig(
            "invalid initial game configuration".into(),
        ));
    }
    if !config.rule_card_ids.is_empty() {
        if rules_version == RULES_VERSION_V6 {
            return Err(EngineError::UnsupportedFeature(
                "initial RULE activation".into(),
            ));
        }
        validate_v7_opening_rule_selection(&config.rule_card_ids, config.deathmatch_enabled)?;
    }
    let initial = if rules_version == RULES_VERSION_V7 {
        include_str!("../../bridge/catalog/initial-state-20260928.json")
    } else {
        include_str!("../../bridge/catalog/initial-state-20260927.json")
    };
    let raw: Value = serde_json::from_str(initial).expect("adopted reset defaults");
    if raw["rulesVersion"].as_str() != Some(rules_version) {
        return Err(EngineError::InvalidState(
            "initial source rules version mismatch".into(),
        ));
    }
    let mut state: GameState =
        serde_json::from_value(raw["state"].clone()).map_err(EngineError::serialization)?;
    state.ruleset_id = rules_version.into();
    state.rng = RngState::seeded(seed);
    if rules_version == RULES_VERSION_V7 {
        // resetGame initializes the replay clock before its first frame.
        // The frozen template is an idle state and deliberately has no date.
        let catalog: Value =
            serde_json::from_str(include_str!("../../bridge/catalog/site-20260928.json"))
                .map_err(EngineError::serialization)?;
        let frozen_at = catalog["source"]["frozenAt"]
            .as_str()
            .ok_or_else(|| EngineError::InvalidState("v7 frozen ISO date missing".into()))?;
        state
            .extra
            .insert("replayStartedAt".into(), json!(frozen_at));
    }
    for col in 0..8 {
        for row in [0, 7] {
            let suffix = random_suffix(state.rng.sample()?)?;
            let piece = state.board[row][col].as_mut().expect("initial rank");
            piece.id = format!("{}-{}-{suffix}", piece.color.as_str(), piece.kind);
        }
    }
    for col in 0..8 {
        for row in [1, 6] {
            let suffix = random_suffix(state.rng.sample()?)?;
            let piece = state.board[row][col].as_mut().expect("initial pawn");
            piece.id = format!("{}-{}-{suffix}", piece.color.as_str(), piece.kind);
        }
    }
    state
        .extra
        .insert("gameStyle".into(), json!(config.game_style));
    state.extra.insert("localMode".into(), json!("local"));
    state
        .extra
        .insert("draftDelete".into(), json!(config.draft_delete));
    state
        .extra
        .insert("starWinLimit".into(), json!(config.star_win_limit));
    state
        .extra
        .insert("deathmatchEnabled".into(), json!(config.deathmatch_enabled));
    state.extra.insert(
        "deathmatchLimitTurns".into(),
        json!(config.deathmatch_limit_turns),
    );
    if rules_version == RULES_VERSION_V7 && config.game_style == "grand" {
        // resetGame sets these flags from initialGameStyle before recording
        // its idle replay frame (main-OahWs0tU.js:66120-66121). The frozen
        // headless grand clock starts from GRAND_DRAFT_PICK_MS=20_000.
        state.extra.insert("middleDraftDone".into(), json!(true));
        state.extra.insert("endDraftDone".into(), json!(true));
        state.extra["draftClock"]["initialMs"] = json!(20_000);
    }
    if !config.rule_card_ids.is_empty() {
        state
            .extra
            .insert("ruleSelectionEnabled".into(), json!(true));
        state
            .extra
            .insert("selectedRuleCardIds".into(), json!(config.rule_card_ids));
        state.extra.insert(
            "selectedRuleCardId".into(),
            json!(if config.rule_card_ids.len() == 1 {
                config.rule_card_ids[0].as_str()
            } else {
                ""
            }),
        );
    }
    let slots = if config.game_style == "normal" { 3 } else { 6 };
    state.deck_slots=serde_json::from_value::<GameState>(json!({"board":state.board,"turn":"white","deckSlots":{"white":vec![Value::Null;slots],"black":vec![Value::Null;slots]}})).map_err(EngineError::serialization)?.deck_slots;
    // resetGame records the idle board before beginInitialGameFlow applies
    // runtime configuration and starts the initial decision phase.
    let configured = [
        "draftDelete",
        "starWinLimit",
        "deathmatchEnabled",
        "deathmatchLimitTurns",
    ]
    .into_iter()
    .map(|key| (key.to_owned(), state.extra[key].clone()))
    .collect::<Vec<_>>();
    for (key, _) in &configured {
        state.extra.insert(key.clone(), raw["state"][key].clone());
    }
    crate::replay::record(&mut state, "initial")?;
    for (key, value) in configured {
        state.extra.insert(key, value);
    }
    if !config.rule_card_ids.is_empty() {
        activate_v7_opening_rule(&mut state, &config.rule_card_ids)?;
    }
    if config.draft_delete {
        state.mode = "play".into();
        state.turn = Color::White;
        crate::flow::start_clock(&mut state)?;
        crate::flow::record_position(&mut state)?;
    } else if config.game_style == "grand" {
        state.mode = "draft".into();
        state.turn = Color::Black;
        state.extra.insert("middleDraftDone".into(), json!(true));
        state.extra.insert("endDraftDone".into(), json!(true));
        state.extra.insert("endPhaseStartMove".into(), json!(0));
        let choices = grand_pool(&mut state)?;
        state.extra.insert("draft".into(),json!({"kind":"grand","version":1,"phase":"GRAND","color":"black","choices":choices,"picks":[],"pickIndex":0}));
        start_draft_clock(&mut state, Color::Black, Some(0))?;
    } else {
        start_draft(&mut state, Color::White, "OPENING")?;
    }
    // The source Position contract snapshots through canonical JSON before
    // actions are restored; retain that one-time construction boundary.
    serde_json::from_slice(&serde_jcs::to_vec(&state).map_err(EngineError::serialization)?)
        .map_err(EngineError::serialization)
}

pub(crate) fn legal_actions(state: &GameState) -> Result<Vec<Action>> {
    let draft = state
        .extra
        .get("draft")
        .ok_or_else(|| EngineError::InvalidState("draft phase missing offer".into()))?;
    if draft.get("kind").and_then(Value::as_str) != Some("grand") {
        let color: Color =
            serde_json::from_value(draft["color"].clone()).map_err(EngineError::serialization)?;
        let choices = draft["choices"]
            .as_array()
            .ok_or_else(|| EngineError::InvalidState("draft choices missing".into()))?;
        let chaos = state.extra.get("gameStyle").and_then(Value::as_str) == Some("chaos");
        let step = if chaos { 2 } else { 1 };
        return choices
            .chunks(step)
            .enumerate()
            .map(|(index, cards)| {
                let mut action = Action::movement(
                    color,
                    Square { row: 0, col: 0 },
                    MoveTarget::at(Square { row: 0, col: 0 }),
                );
                action.from = None;
                action.destination = None;
                if chaos {
                    action.kind = ActionKind::DraftBundlePick;
                    action.extra.insert("bundleIndex".into(), json!(index));
                    action.extra.insert(
                        "cardInstanceIds".into(),
                        json!(
                            cards
                                .iter()
                                .map(|c| c["instanceId"].clone())
                                .collect::<Vec<_>>()
                        ),
                    );
                } else {
                    action.kind = ActionKind::DraftPick;
                    action.card_instance_id = Some(
                        cards[0]["instanceId"]
                            .as_str()
                            .ok_or(EngineError::IllegalAction)?
                            .into(),
                    );
                }
                Ok(action)
            })
            .collect();
    }
    let color: Color =
        serde_json::from_value(draft["color"].clone()).map_err(EngineError::serialization)?;
    let picks = draft["picks"]
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("grand picks missing".into()))?;
    if picks.len() >= 12 {
        return Ok(Vec::new());
    }
    let owned = picks
        .iter()
        .filter(|pick| pick["color"] == color.as_str())
        .filter_map(|pick| pick["cardId"].as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    if owned.len() >= 6 {
        return Ok(Vec::new());
    }
    let exclusive =
        definitions_for_ruleset(&state.ruleset_id)?.constants["EXCLUSIVE_OPENING_CARD_IDS"]
            .as_array()
            .expect("exclusive opening ids");
    draft["choices"]
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("grand choices missing".into()))?
        .iter()
        .filter(|card| {
            let already_picked = picks
                .iter()
                .any(|pick| pick["instanceId"] == card["instanceId"]);
            let conflicts_with_owned = conflicts_with_ruleset(
                &state.ruleset_id,
                card["id"].as_str().expect("card id"),
                &owned,
            );
            let exclusive_opening_conflict = exclusive.contains(&card["id"])
                && owned.iter().any(|id| exclusive.contains(&json!(id)));
            !(already_picked || conflicts_with_owned || exclusive_opening_conflict)
        })
        .map(|card| {
            let mut action = Action::movement(
                color,
                Square { row: 0, col: 0 },
                MoveTarget::at(Square { row: 0, col: 0 }),
            );
            action.kind = ActionKind::DraftPick;
            action.from = None;
            action.destination = None;
            action.card_instance_id = Some(
                card["instanceId"]
                    .as_str()
                    .ok_or_else(|| EngineError::InvalidState("draft instance missing".into()))?
                    .into(),
            );
            Ok(action)
        })
        .collect()
}

pub(crate) fn apply_pick(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    let draft = state
        .extra
        .get("draft")
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    if draft["kind"] != "grand" {
        return apply_regular_pick(state, action, &draft);
    }
    let card = draft["choices"]
        .as_array()
        .ok_or(EngineError::IllegalAction)?
        .iter()
        .find(|card| card["instanceId"].as_str() == action.card_instance_id.as_deref())
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    let color = action.color;
    let slot = state
        .deck_slots
        .get(color)
        .iter()
        .position(|card| card.vacant)
        .ok_or_else(|| EngineError::InvalidState("grand deck is full".into()))?;
    let nonce = state
        .extra
        .get("cardAcquisitionNonce")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| EngineError::InvalidState("card acquisition order overflow".into()))?;
    let mut stored = card.clone();
    let phase = category_with_ruleset(&state.ruleset_id, &card);
    stored["deckCard"] = json!(true);
    stored["firstTurnCard"] = json!(state.move_count == 0 && phase == "OPENING");
    stored["slot"] = json!(slot);
    stored["acquiredOrder"] = json!(nonce);
    if phase == "END" {
        stored["nextTurnPending"] = json!(true);
        stored["nextTurnPendingSinceTurn"] = json!(0);
    } else {
        stored
            .as_object_mut()
            .expect("card")
            .shift_remove("nextTurnPending");
        stored
            .as_object_mut()
            .expect("card")
            .shift_remove("nextTurnPendingSinceTurn");
    }
    stored
        .as_object_mut()
        .expect("card")
        .shift_remove("passiveApplied");
    state.deck_slots.get_mut(color)[slot] =
        serde_json::from_value(stored).map_err(EngineError::serialization)?;
    state
        .extra
        .insert("cardAcquisitionNonce".into(), json!(nonce));
    // The actual client queues gain notation before noteCardEvent. Its notation
    // identifier consumes Math.random even though it is presentation metadata;
    // omitting that draw would change every subsequent gameplay chance outcome.
    crate::replay::queue_gain(
        state,
        color,
        &serde_json::to_value(&state.deck_slots.get(color)[slot])
            .map_err(EngineError::serialization)?,
        "OPENING",
    )?;
    crate::flow::note_card_event(state)?;
    let mut next_draft = draft;
    let picks = next_draft["picks"]
        .as_array_mut()
        .ok_or(EngineError::IllegalAction)?;
    let order = picks.len() + 1;
    picks.push(
        json!({"instanceId":card["instanceId"],"cardId":card["id"],"color":color,"order":order}),
    );
    next_draft["pickIndex"] = json!(order);
    state.extra.insert("draft".into(), next_draft);
    mark_draft_clock_selected(state, color);
    crate::replay::add_log(
        state,
        format!(
            "{} 그랜드 드래프트 {order}/12: {}",
            crate::replay::label(color),
            card["name"].as_str().unwrap_or("undefined")
        ),
    )?;
    if order < 12 {
        crate::replay::record(state, "draft:grand")?;
        let next = if order % 2 == 0 {
            Color::Black
        } else {
            Color::White
        };
        state.turn = next;
        state.extra.get_mut("draft").expect("draft")["color"] = json!(next);
        state.extra.insert("draftLocked".into(), json!(false));
        start_draft_clock(state, next, Some(order))?;
    } else {
        let mut acquired = state.extra["draft"]["picks"]
            .as_array()
            .expect("grand picks")
            .clone();
        let priority = |id: &str| match id {
            "london-system" => 0,
            "horde" => 1,
            "big-rook" | "big-bishop" => 2,
            "false-start" => 4,
            "locust-swarm" => 5,
            _ => 3,
        };
        acquired.sort_by_key(|pick| {
            (
                priority(pick["cardId"].as_str().unwrap_or("")),
                pick["order"].as_u64().unwrap_or(0),
            )
        });
        for pick in acquired {
            if state.mode == "gameover" {
                break;
            }
            let side: Color = serde_json::from_value(pick["color"].clone())
                .map_err(EngineError::serialization)?;
            let Some(slot) =
                state.deck_slots.get(side).iter().position(|card| {
                    Some(card.instance_id.as_str()) == pick["instanceId"].as_str()
                })
            else {
                continue;
            };
            let card = &state.deck_slots.get(side)[slot];
            if is_passive_definition_for_ruleset(
                &state.ruleset_id,
                &serde_json::to_value(card).map_err(EngineError::serialization)?,
            )? {
                crate::transition::apply_draft_passive(state, side, slot)?;
            }
        }
        crate::flow::note_card_event(state)?;
        crate::replay::record(state, "draft:grand")?;
        if state.mode == "gameover" {
            return Ok(Vec::new());
        }
        state.mode = "play".into();
        state.turn = Color::White;
        state.extra.insert("draftLocked".into(), json!(false));
        state.actions_remaining = 1;
        state.extra.insert("middleDraftDone".into(), json!(true));
        state.extra.insert("endDraftDone".into(), json!(true));
        state.extra.insert("endPhaseStartMove".into(), json!(0));
        crate::flow::start_clock(state)?;
        crate::flow::record_position(state)?;
        crate::flow::check_no_action_loss(state)?;
    }
    Ok(Vec::new())
}

fn passive_priority(card: &Value) -> u8 {
    match card["id"].as_str().unwrap_or("") {
        "london-system" => 0,
        "horde" => 1,
        "big-rook" | "big-bishop" => 2,
        "false-start" => 4,
        "locust-swarm" => 5,
        _ => 3,
    }
}
fn apply_regular_pick(state: &mut GameState, action: &Action, draft: &Value) -> Result<Vec<Piece>> {
    let choices = draft["choices"]
        .as_array()
        .ok_or(EngineError::IllegalAction)?;
    let color = action.color;
    let selected = if action.kind == ActionKind::DraftBundlePick {
        let index = action
            .extra
            .get("bundleIndex")
            .and_then(Value::as_u64)
            .ok_or(EngineError::IllegalAction)?;
        if index >= 3 {
            return Err(EngineError::IllegalAction);
        }
        choices
            .get(index as usize * 2..index as usize * 2 + 2)
            .ok_or(EngineError::IllegalAction)?
            .to_vec()
    } else {
        vec![
            choices
                .iter()
                .find(|card| card["instanceId"].as_str() == action.card_instance_id.as_deref())
                .cloned()
                .ok_or(EngineError::IllegalAction)?,
        ]
    };
    if state
        .deck_slots
        .get(color)
        .iter()
        .filter(|card| card.vacant)
        .count()
        < selected.len()
    {
        return Err(EngineError::IllegalAction);
    }
    let phase = draft["phase"].as_str().ok_or(EngineError::IllegalAction)?;
    let mut acquired = Vec::new();
    for card in &selected {
        let slot = state
            .deck_slots
            .get(color)
            .iter()
            .position(|card| card.vacant)
            .ok_or(EngineError::IllegalAction)?;
        let mut stored = clone_card(state, card)?;
        let category = category_with_ruleset(&state.ruleset_id, card);
        let pending = matches!(phase, "MIDDLE" | "END") && matches!(category, "MIDDLE" | "END");
        stored["deckCard"] = json!(true);
        stored["nextTurnPending"] = json!(pending);
        if pending {
            stored["nextTurnPendingSinceTurn"] = json!(state.turns_taken.get(color));
        }
        stored["firstTurnCard"] =
            json!(phase == "OPENING" && state.move_count == 0 && category == "OPENING");
        stored["slot"] = json!(slot);
        let nonce = state
            .extra
            .get("cardAcquisitionNonce")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| EngineError::InvalidState("card acquisition order overflow".into()))?;
        stored["acquiredOrder"] = json!(nonce);
        state
            .extra
            .insert("cardAcquisitionNonce".into(), json!(nonce));
        state.deck_slots.get_mut(color)[slot] =
            serde_json::from_value(stored.clone()).map_err(EngineError::serialization)?;
        // queueCardGainNotation -> createHistoryNotationId consumes one draw.
        crate::replay::queue_gain(state, color, &stored, phase)?;
        if selected.len() == 1 && is_passive_definition_for_ruleset(&state.ruleset_id, card)? {
            crate::transition::apply_draft_passive(state, color, slot)?;
        }
        crate::flow::note_card_event(state)?;
        acquired.push((slot, stored));
    }
    if selected.len() > 1 {
        acquired.sort_by_key(|(_, card)| {
            (
                passive_priority(card),
                card["acquiredOrder"].as_u64().unwrap_or(0),
            )
        });
        for (slot, card) in &acquired {
            if is_passive_definition_for_ruleset(&state.ruleset_id, card)? {
                crate::transition::apply_draft_passive(state, color, *slot)?;
            }
        }
        crate::flow::note_card_event(state)?;
    }
    state.extra.insert("draftLocked".into(), json!(true));
    mark_draft_clock_selected(state, color);
    for field in ["draftPreviewCardId", "draftPreviewBundleIndex"] {
        state.extra.insert(field.into(), Value::Null);
    }
    let names = selected
        .iter()
        .filter_map(|card| card["name"].as_str().or_else(|| card["id"].as_str()))
        .collect::<Vec<_>>()
        .join(" + ");
    crate::replay::add_log(
        state,
        format!("{} 드래프트: {names}", crate::replay::label(color)),
    )?;
    crate::replay::record(state, &format!("draft:{}", phase.to_lowercase()))?;
    if state.mode == "gameover" {
        return Ok(Vec::new());
    }
    if color == Color::White {
        let score = average_score(choices)
            .ok_or_else(|| EngineError::InvalidState("empty completed offer".into()))?;
        state.extra.insert(
            "draftBalance".into(),
            json!({"phase":phase,"averageScore":score,"count":choices.len()}),
        );
        start_draft(state, Color::Black, phase)?;
    } else {
        state.extra.insert("draftBalance".into(), Value::Null);
        state
            .extra
            .get_mut("draft")
            .ok_or(EngineError::IllegalAction)?["choices"] = json!([]);
        state.mode = "play".into();
        state.turn = state
            .extra
            .get("draftResumeTurn")
            .filter(|turn| !turn.is_null())
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(EngineError::serialization)?
            .unwrap_or(Color::White);
        state.extra.insert("draftResumeTurn".into(), Value::Null);
        state.actions_remaining = if state
            .extra
            .get("acceleration")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            2
        } else {
            1
        };
        state.extra.insert("draftLocked".into(), json!(false));
        if phase == "MIDDLE" {
            state.extra.insert("middleDraftDone".into(), json!(true));
        }
        if phase == "END" {
            state.extra.insert("endDraftDone".into(), json!(true));
            state.extra.insert(
                "endPhaseStartMove".into(),
                json!(state.turns_taken.white.min(state.turns_taken.black)),
            );
        }
        if state
            .extra
            .get("pendingWhiteBoxes")
            .and_then(Value::as_array)
            .is_some_and(|plans| !plans.is_empty())
        {
            return Err(EngineError::UnsupportedFeature(
                "white box delayed play-start settlement".into(),
            ));
        }
        crate::flow::start_clock(state)?;
        crate::flow::record_position(state)?;
        crate::flow::check_no_action_loss(state)?;
    }
    Ok(Vec::new())
}
pub(crate) fn is_passive_definition(card: &Value) -> bool {
    static IDS: OnceLock<BTreeSet<String>> = OnceLock::new();
    IDS.get_or_init(|| {
        let catalog: Value =
            serde_json::from_str(include_str!("../../bridge/catalog/site-20260927.json"))
                .expect("catalog");
        catalog["cards"]
            .as_array()
            .expect("cards")
            .iter()
            .filter(|c| c["activation"] == "PASSIVE")
            .map(|c| c["id"].as_str().expect("id").into())
            .collect()
    })
    .contains(card["id"].as_str().unwrap_or(""))
}
pub(crate) fn is_passive_definition_for_ruleset(rules_version: &str, card: &Value) -> Result<bool> {
    if rules_version == RULES_VERSION_V6 {
        return Ok(is_passive_definition(card));
    }
    let id = card
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| EngineError::InvalidState("card definition id missing".into()))?;
    let definition = crate::card_registry::definition_for(rules_version, id)?;
    Ok(definition.activation == Some(crate::card_registry::CardActType::Passive))
}
