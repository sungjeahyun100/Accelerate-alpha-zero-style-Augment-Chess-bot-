//! Versioned card phases and source weight tables.
use crate::*;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, sync::OnceLock};

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
        serde_json::from_str(include_str!("../../contracts/catalog/draft-20260927.json"))
            .expect("adopted draft weights")
    })
}
fn weights_for_ruleset(rules_version: &str) -> Result<&'static Weights> {
    static V7: OnceLock<Weights> = OnceLock::new();
    match rules_version {
        RULES_VERSION_V6 => Ok(weights()),
        RULES_VERSION_V7 => Ok(V7.get_or_init(|| {
            serde_json::from_str(include_str!("../../contracts/catalog/draft-20260928.json"))
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
pub(crate) fn weight_with_ruleset(rules_version: &str, card: &Value, opening: bool) -> f64 {
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

pub(crate) fn constants_for_ruleset(rules_version: &str) -> Result<&'static Value> {
    Ok(&weights_for_ruleset(rules_version)?.constants)
}
