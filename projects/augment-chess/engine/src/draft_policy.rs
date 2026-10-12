//! Draft transition policies, separate from the card catalog and predicates.
use crate::{Color, EngineError, GameState, RULES_VERSION_V6, RULES_VERSION_V7, Result};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DraftMode {
    Normal,
    Chaos,
    Grand,
}

impl DraftMode {
    pub(crate) fn from_state(state: &GameState) -> Result<Self> {
        match state
            .extra
            .get("gameStyle")
            .and_then(Value::as_str)
            .unwrap_or("normal")
        {
            "normal" => Ok(Self::Normal),
            "chaos" => Ok(Self::Chaos),
            "grand" => Ok(Self::Grand),
            other => Err(EngineError::InvalidState(format!(
                "unknown draft mode {other}"
            ))),
        }
    }
    pub(crate) fn offer_count(self) -> usize {
        match self {
            Self::Chaos => 6,
            Self::Normal | Self::Grand => 3,
        }
    }
    pub(crate) fn black_balance_retries(self) -> usize {
        match self {
            Self::Normal | Self::Chaos => 2,
            Self::Grand => 0,
        }
    }
    pub(crate) fn draw_offer(
        self,
        state: &mut GameState,
        phase: &str,
        color: Color,
    ) -> Result<Vec<Value>> {
        match self {
            Self::Normal => normal_offer(state, phase, 3, color),
            Self::Chaos => {
                let cards = normal_offer(state, phase, 6, color)?;
                crate::draft::arrange_chaos(state, cards, color)
            }
            Self::Grand => Err(EngineError::InvalidState(
                "grand uses a category pool".into(),
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftProfile {
    September27,
    September28,
    October07Probe,
}
impl DraftProfile {
    pub fn for_ruleset(ruleset: &str) -> Result<Self> {
        match ruleset {
            RULES_VERSION_V6 => Ok(Self::September27),
            RULES_VERSION_V7 => Ok(Self::September28),
            "augment-site-20261007-probe" => Ok(Self::October07Probe),
            other => Err(EngineError::UnsupportedFeature(format!(
                "draft profile {other}"
            ))),
        }
    }
    pub fn executable(self) -> Result<()> {
        if self == Self::October07Probe {
            Err(EngineError::UnsupportedFeature(
                "October complete candidate catalog and execution profile are unverified".into(),
            ))
        } else {
            Ok(())
        }
    }
}

/// P and Q are densities for the same realized semantic chance trace.
#[derive(Clone, Copy, Debug)]
pub struct ChanceDensity {
    pub game_p: f64,
    pub proposal_q: f64,
}
impl ChanceDensity {
    pub fn new(game_p: f64, proposal_q: f64) -> Result<Self> {
        if !game_p.is_finite()
            || !proposal_q.is_finite()
            || !(0.0..=1.0).contains(&game_p)
            || !(0.0..=1.0).contains(&proposal_q)
        {
            return Err(EngineError::InvalidState(
                "invalid draft chance density".into(),
            ));
        }
        Ok(Self { game_p, proposal_q })
    }
}

fn normal_offer(
    state: &mut GameState,
    phase: &str,
    count: usize,
    color: Color,
) -> Result<Vec<Value>> {
    match phase {
        "OPENING" => crate::draft::draw_mixed(
            state,
            &["OPENING", "MIDDLE", "PIECE"],
            count,
            color,
            &BTreeSet::new(),
            true,
        ),
        "MIDDLE" => middle_offer(state, count, color),
        "END" => crate::draft::draw_mixed(
            state,
            &["MIDDLE", "END"],
            count,
            color,
            &BTreeSet::new(),
            false,
        ),
        other => crate::draft::draw_mixed(state, &[other], count, color, &BTreeSet::new(), false),
    }
}
fn middle_offer(state: &mut GameState, count: usize, color: Color) -> Result<Vec<Value>> {
    let pieces = ((count as f64 / 3.0).round() as usize).max(1);
    let mut excluded = BTreeSet::new();
    let mut choices = Vec::new();
    for (category, amount) in [("MIDDLE", count - pieces), ("PIECE", pieces)] {
        for card in crate::draft::draw_mixed(state, &[category], amount, color, &excluded, false)? {
            excluded.insert(card["id"].as_str().expect("catalog id").into());
            choices.push(card);
        }
    }
    if choices.len() < count {
        choices.extend(crate::draft::draw_mixed(
            state,
            &["MIDDLE", "PIECE"],
            count - choices.len(),
            color,
            &excluded,
            false,
        )?);
    }
    Ok(choices)
}

/// October source rarity weights are relative units, not percentages.
pub fn october_rarity_weight(rarity: &str) -> Result<u32> {
    match rarity.to_ascii_lowercase().as_str() {
        "common" => Ok(85),
        "rare" => Ok(70),
        "epic" => Ok(58),
        "legendary" => Ok(44),
        _ => Err(EngineError::InvalidState(format!(
            "unknown October rarity {rarity}"
        ))),
    }
}

/// October completeRandom removes a weighted choice before compatibility is
/// checked. Rejected choices consume probability mass and cannot be redrawn.
pub fn october_complete_random_step(
    remaining: &mut Vec<(String, u32)>,
    selected: &mut Vec<String>,
    roll: f64,
    conflicts: impl Fn(&str, &[String]) -> bool,
) -> Result<ChanceDensity> {
    if !roll.is_finite() || !(0.0..1.0).contains(&roll) || remaining.is_empty() {
        return Err(EngineError::InvalidState(
            "invalid October draw input".into(),
        ));
    }
    let mut ids = BTreeSet::new();
    if remaining
        .iter()
        .any(|(id, _)| id.is_empty() || !ids.insert(id))
    {
        return Err(EngineError::InvalidState(
            "October draw pool has missing or duplicate IDs".into(),
        ));
    }
    let total = remaining.iter().try_fold(0_u64, |sum, (_, weight)| {
        sum.checked_add(u64::from(*weight))
            .ok_or_else(|| EngineError::InvalidState("October weighted mass overflow".into()))
    })?;
    if total == 0 {
        return Err(EngineError::InvalidState(
            "empty October weighted mass".into(),
        ));
    }
    let mut threshold = roll * total as f64;
    let mut index = remaining
        .iter()
        .rposition(|(_, weight)| *weight > 0)
        .ok_or_else(|| EngineError::InvalidState("empty October weighted mass".into()))?;
    for (candidate, (_, weight)) in remaining.iter().enumerate() {
        if *weight == 0 {
            continue;
        }
        threshold -= f64::from(*weight);
        if threshold <= 0.0 {
            index = candidate;
            break;
        }
    }
    let (id, weight) = remaining.remove(index);
    if !conflicts(&id, selected) {
        selected.push(id);
    }
    ChanceDensity::new(weight as f64 / total as f64, weight as f64 / total as f64)
}
