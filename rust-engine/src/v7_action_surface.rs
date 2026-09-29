//! Complete ordered candidate actions for three frozen v7 first-play snapshots.
//! This private probe does not admit a `Position` or authorize binding or apply.

use crate::{Action, Color, EngineError, GameState, RULES_VERSION_V7, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

struct SourceOpening {
    state_digest: &'static str,
    rng_cursor: usize,
    rng_state: u32,
    card_counts: &'static [(&'static str, usize)],
    legal_count: usize,
    legal_digest: &'static str,
}

// %APPDATA%/Accelerate/reports/v7-opening-source-probe/seed19-active-only/
// manifest.json, generated from the frozen main-OahWs0tU.js (SHA-256
// e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c).
// The state digest is JCS SHA-256 of state without the outer Position's
// rulesetId, rng and history metadata. The action digest covers the complete
// ordered payload array, not a prefix or just its first card action.
fn opening_for(style: &str) -> Result<SourceOpening> {
    let opening = match style {
        "normal" => SourceOpening {
            state_digest: "a106b0ce0aeb64b5301a3f90a3fb1c0c32c4a6d4e504362d311587800c4a072c",
            rng_cursor: 216,
            rng_state: 3_909_686_763,
            card_counts: &[("relay", 1)],
            legal_count: 21,
            legal_digest: "a8439b7876ea8387abe35d36f7036f705cd8ac042ed1db534dc30c1acf70a831",
        },
        "chaos" => SourceOpening {
            state_digest: "bf4efad54e5815e3e401e87adeab5ddce665484422d45e51d1fc6dec032fe62a",
            rng_cursor: 400,
            rng_state: 185_085_603,
            card_counts: &[("queens-gambit", 1), ("reposition", 1)],
            legal_count: 22,
            legal_digest: "f49b3b3c573c45f025dee56f63af2502b537072fc061ae8d4e74ea4f3090cac8",
        },
        "grand" => SourceOpening {
            state_digest: "6ccd5c79e3de0da607f36542da8d8640c8bb6b5892638e12c5b895e0cd5788ac",
            rng_cursor: 124,
            rng_state: 1_313_359_343,
            card_counts: &[
                ("inertia", 5),
                ("miracle", 0),
                ("taunt", 1),
                ("trojan-horse", 2),
                ("grasshopper", 4),
                ("grappler", 4),
            ],
            legal_count: 36,
            legal_digest: "e3d592dd54d9a53bfee69101f58ed933ca15acf0042d6fc02f3b7af6d875a841",
        },
        _ => {
            return Err(EngineError::UnsupportedFeature(
                "v7 action candidates outside source-verified seed-19 styles".into(),
            ));
        }
    };
    Ok(opening)
}

fn unsupported(reason: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!(
        "v7 action candidates outside source-verified seed-19 first play: {reason}"
    ))
}

fn digest<T: serde::Serialize>(value: &T) -> Result<String> {
    let canonical = serde_jcs::to_vec(value).map_err(EngineError::serialization)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

fn require_source_opening(state: &GameState) -> Result<SourceOpening> {
    if state.ruleset_id != RULES_VERSION_V7 || !state.history.is_empty() {
        return Err(unsupported("rules version or history"));
    }
    let style = state
        .extra
        .get("gameStyle")
        .and_then(Value::as_str)
        .ok_or_else(|| unsupported("game style"))?;
    let source = opening_for(style)?;
    if state.rng.algorithm != "lcg32-v1"
        || !state.rng.tape.is_empty()
        || state.rng.cursor != source.rng_cursor
        || state.rng.state != source.rng_state
    {
        return Err(unsupported("RNG"));
    }
    let mut checked = state.clone();
    checked.validate_v7_snapshot_shape_and_identify()?;
    if &checked != state {
        return Err(unsupported("noncanonical state"));
    }
    let mut raw = serde_json::to_value(state).map_err(EngineError::serialization)?;
    let fields = raw
        .as_object_mut()
        .ok_or_else(|| unsupported("state object"))?;
    for name in ["rulesetId", "rng", "history"] {
        if fields.remove(name).is_none() {
            return Err(unsupported("source state envelope"));
        }
    }
    if digest(&raw)? != source.state_digest {
        return Err(unsupported("state digest"));
    }
    Ok(source)
}

/// Return the complete ordered source candidate payloads for the three exact
/// seed-19 openings. The card kernel still owns each supported card's target
/// enumeration. Miracle has zero candidates in only this pinned grand state;
/// its general target predicate remains unsupported.
#[allow(dead_code, reason = "v7 Position execution remains closed")]
pub(crate) fn seed19_first_play_legal_actions(state: &GameState) -> Result<Vec<Action>> {
    let source = require_source_opening(state)?;
    let (_, mut actions) = crate::movement::v7_verified_first_play_movement_actions(state)?;
    if actions.len() != 20 {
        return Err(unsupported("opening movement count"));
    }
    let cards = state
        .deck_slots
        .get(Color::White)
        .iter()
        .filter(|card| !card.vacant)
        .collect::<Vec<_>>();
    if cards.len() != source.card_counts.len()
        || cards
            .iter()
            .zip(source.card_counts)
            .any(|(card, (id, _))| card.id != *id || card.used || card.recovering)
    {
        return Err(unsupported("selected card order"));
    }
    for (card, &(_, expected_count)) in cards.into_iter().zip(source.card_counts) {
        if card.id == "miracle" {
            // Its source first-play capture target set is empty. Keep the
            // unsupported general Miracle rule closed under all other states.
            continue;
        }
        let offered = crate::card_effects::actions(state, card)?
            .ok_or_else(|| unsupported("unimplemented selected card"))?;
        if offered.len() != expected_count {
            return Err(unsupported("card action count"));
        }
        for action in &offered {
            if crate::card_effects::validate(state, card, action)? != Some(true) {
                return Err(unsupported("card action validation"));
            }
        }
        actions.extend(offered);
    }
    if actions.len() != source.legal_count || digest(&actions)? != source.legal_digest {
        return Err(unsupported("ordered legal payload"));
    }
    Ok(actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;
    use serde_json::json;

    fn first_play(style: &str) -> GameState {
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        let offer_indices: &[usize] = match style {
            "normal" => &[1, 0],
            "chaos" => &[1, 2],
            "grand" => &[1, 3, 3, 3, 3, 3, 3, 3, 5, 5, 5, 5],
            _ => unreachable!(),
        };
        for &index in offer_indices {
            let pick = crate::draft::legal_actions(&state).unwrap().remove(index);
            crate::draft::apply_pick(&mut state, &pick).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        assert_eq!(state.mode, "play");
        state
    }

    #[test]
    fn exact_source_openings_have_complete_ordered_candidates() {
        for style in ["normal", "chaos", "grand"] {
            let state = first_play(style);
            let source = opening_for(style).unwrap();
            let actions = seed19_first_play_legal_actions(&state).unwrap();
            assert_eq!(actions.len(), source.legal_count, "{style}");
            assert_eq!(digest(&actions).unwrap(), source.legal_digest, "{style}");
            assert!(actions.iter().all(|action| action.position_key.is_none()));
        }
    }

    #[test]
    fn changed_state_rng_history_and_rules_version_fail_closed() {
        let state = first_play("grand");
        let mut cases = Vec::new();
        let mut changed = state.clone();
        changed.rng.state ^= 1;
        cases.push(changed);
        let mut changed = state.clone();
        changed.history.push(json!({"public":{}}));
        cases.push(changed);
        let mut changed = state.clone();
        changed.ruleset_id = crate::RULES_VERSION_V6.into();
        cases.push(changed);
        let mut changed = state.clone();
        changed
            .extra
            .insert("cornerKick".into(), json!({"white":true,"black":false}));
        cases.push(changed);
        let mut changed = state.clone();
        changed.deck_slots.white[0].id = "unverified".into();
        cases.push(changed);
        for changed in cases {
            assert!(matches!(
                seed19_first_play_legal_actions(&changed),
                Err(EngineError::UnsupportedFeature(_))
            ));
        }
    }
}
