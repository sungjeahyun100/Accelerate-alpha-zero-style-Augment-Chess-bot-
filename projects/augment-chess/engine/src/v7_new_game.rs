//! Source-pinned v7 new-game admission for the game adapter host.
//! Admit source-shaped opening RULE pools after validating the complete setup.

use crate::{Color, EngineError, GameConfig, GameState, RULES_VERSION_V7, Result};
use serde_json::Value;
use std::collections::BTreeSet;

pub(crate) fn new_game(config: GameConfig, seed: u64) -> Result<GameState> {
    if seed > u64::from(u32::MAX)
        || !matches!(config.game_style.as_str(), "normal" | "chaos" | "grand")
        || config.star_win_limit == 0
        || config.deathmatch_limit_turns == 0
    {
        return Err(EngineError::InvalidConfig(
            "invalid v7 initial game configuration".into(),
        ));
    }
    // A nonempty request is a source selection pool: resetGame installs one
    // randomly chosen RULE, not every requested card. Preserve input order;
    // it affects the selected index and therefore the complete source state.
    let requested_rules = config.rule_card_ids.clone();
    let expected_mode = if config.draft_delete { "play" } else { "draft" };
    let expected_offer_count = match config.game_style.as_str() {
        "normal" => 3,
        "chaos" => 6,
        "grand" => 28,
        _ => unreachable!("configuration was checked above"),
    };
    // Chaos may replace forbidden opening pairs with additional source draws;
    // its cursor depends on the sampled offer, unlike normal and grand.
    let expected_rng_cursor = if !requested_rules.is_empty() {
        None
    } else if config.draft_delete {
        Some(32)
    } else {
        match config.game_style.as_str() {
            "normal" => Some(122),
            "chaos" => None,
            "grand" => Some(112),
            _ => unreachable!("configuration was checked above"),
        }
    };
    let expected_draft_actor = if config.game_style == "grand" {
        "black"
    } else {
        "white"
    };
    let expected_style = config.game_style.clone();
    let expected_draft_delete = config.draft_delete;
    let expected_stars = u64::from(config.star_win_limit);
    let expected_deathmatch_enabled = config.deathmatch_enabled;
    let expected_deathmatch_limit = u64::from(config.deathmatch_limit_turns);
    let mut state = candidate_new_game(config, seed)?;
    state.validate_v7_snapshot_shape_and_identify()?;
    let selected_rule = if requested_rules.is_empty() {
        None
    } else {
        let id = state
            .extra
            .get("appliedRuleCard")
            .and_then(|card| card.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::InvalidState("v7 opening RULE was not applied".into()))?;
        if !requested_rules.iter().any(|requested| requested == id) {
            return Err(EngineError::InvalidState(
                "v7 opening RULE is outside the requested source pool".into(),
            ));
        }
        Some(id.to_owned())
    };
    // revelation changes the live limit after resetGame records the initial
    // replay frames; those frames retain the configured limit.
    let expected_active_deathmatch_limit = if selected_rule.as_deref() == Some("revelation") {
        5
    } else {
        expected_deathmatch_limit
    };
    let opening_notation_id = selected_rule
        .as_deref()
        .map(|rule| format!("opening-rule-{rule}"));
    if state.mode != expected_mode
        || !state.history.is_empty()
        || state.rng.algorithm != "lcg32-v1"
        || expected_rng_cursor.is_some_and(|cursor| state.rng.cursor != cursor)
        || !state.rng.tape.is_empty()
        || state.extra.get("gameStyle").and_then(Value::as_str) != Some(expected_style.as_str())
        || state.extra.get("draftDelete").and_then(Value::as_bool) != Some(expected_draft_delete)
        || state.extra.get("starWinLimit").and_then(Value::as_u64) != Some(expected_stars)
        || state
            .extra
            .get("deathmatchEnabled")
            .and_then(Value::as_bool)
            != Some(expected_deathmatch_enabled)
        || state
            .extra
            .get("deathmatchLimitTurns")
            .and_then(Value::as_u64)
            != Some(expected_active_deathmatch_limit)
        || state
            .extra
            .get("replayEvents")
            .and_then(Value::as_array)
            .is_none_or(|events| !events.is_empty())
        || state
            .extra
            .get("pendingNotations")
            .and_then(Value::as_array)
            .is_none_or(|entries| match opening_notation_id.as_deref() {
                Some(expected_id) => {
                    entries.len() != 1
                        || entries[0].get("id").and_then(Value::as_str) != Some(expected_id)
                }
                None => !entries.is_empty(),
            })
        || state
            .extra
            .get("boardHistory")
            .and_then(Value::as_array)
            .is_none_or(|entries| {
                entries.len() != 1
                    || entries[0].get("label").and_then(Value::as_str) != Some("initial")
            })
    {
        return Err(EngineError::InvalidState(
            "v7 source initial setup shape is invalid".into(),
        ));
    }
    if let Some(rule) = selected_rule.as_deref()
        && (state.extra.get("ruleSelectionEnabled") != Some(&Value::Bool(true))
            || state.extra.get("selectedRuleCardIds") != Some(&serde_json::json!(requested_rules))
            || state
                .extra
                .get("selectedRuleCardId")
                .and_then(Value::as_str)
                != Some(if requested_rules.len() == 1 { rule } else { "" })
            || state
                .extra
                .get("appliedRuleCard")
                .and_then(|card| card.get("id"))
                .and_then(Value::as_str)
                != Some(rule)
            || state
                .extra
                .get("ruleOpeningEvent")
                .and_then(|event| event.get("status"))
                .and_then(Value::as_str)
                != Some("hit")
            || state
                .extra
                .get("ruleOpeningEvent")
                .and_then(|event| event.get("card"))
                .and_then(|card| card.get("id"))
                .and_then(Value::as_str)
                != Some(rule)
            || state
                .extra
                .get("pendingNotation")
                .and_then(|entry| entry.get("id"))
                .and_then(Value::as_str)
                != opening_notation_id.as_deref())
    {
        return Err(EngineError::InvalidState(
            "v7 source opening RULE selection or notation is invalid".into(),
        ));
    }
    for frame_name in ["replayBaseFrame", "replayTailFrame"] {
        let frame = state.extra.get(frame_name).ok_or_else(|| {
            EngineError::InvalidState(format!("v7 initial {frame_name} is missing"))
        })?;
        if frame.get("draftDelete").and_then(Value::as_bool) != Some(expected_draft_delete)
            || frame.get("starWinLimit").and_then(Value::as_u64) != Some(expected_stars)
            || frame.get("deathmatchEnabled").and_then(Value::as_bool)
                != Some(expected_deathmatch_enabled)
            || frame.get("deathmatchLimitTurns").and_then(Value::as_u64)
                != Some(expected_deathmatch_limit)
        {
            return Err(EngineError::InvalidState(format!(
                "v7 initial {frame_name} configuration differs from source setup"
            )));
        }
    }
    let draft = state
        .extra
        .get("draft")
        .ok_or_else(|| EngineError::InvalidState("v7 initial draft offer is missing".into()))?;
    if expected_draft_delete {
        if draft.get("color").and_then(Value::as_str) != Some("white")
            || draft
                .get("choices")
                .and_then(Value::as_array)
                .is_none_or(|choices| !choices.is_empty())
        {
            return Err(EngineError::InvalidState(
                "v7 draftDelete initial draft is invalid".into(),
            ));
        }
    } else {
        let choices = draft
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                EngineError::InvalidState("v7 initial draft choices are missing".into())
            })?;
        let mut instances = BTreeSet::new();
        if choices.len() != expected_offer_count
            || draft.get("color").and_then(Value::as_str) != Some(expected_draft_actor)
            || choices.iter().any(|card| {
                let id = card.get("id").and_then(Value::as_str);
                let instance_id = card.get("instanceId").and_then(Value::as_str);
                id.is_none_or(str::is_empty)
                    || instance_id.is_none_or(str::is_empty)
                    || instance_id.is_some_and(|id| !instances.insert(id))
            })
        {
            return Err(EngineError::InvalidState(
                "v7 initial draft offer identity or order is invalid".into(),
            ));
        }
    }
    Ok(state)
}

fn candidate_new_game(config: GameConfig, seed: u64) -> Result<GameState> {
    crate::draft::initialize_for_ruleset(config, seed, RULES_VERSION_V7)
}

/// 동결 local8×8 장기의 cold `newGame` → `startCampaign` 경계.
/// main80208/80239/71289에 따라 기존 게임 초기화의 난수·입력 검증을 먼저
/// 유지하고, 목표 표시용 장기 보드 → normal fresh reset → 실제 장기 보드
/// 순서로 같은 RNG를 소비한다. 외부 sourceResetPosition을 입력으로 받지 않는다.
/// local mode는 single/offline만 지원하며 warm preview cache·변경된 browser
/// module 설정·온라인 recipientLegalMoves authority를 임의로 추론하지 않는다.
pub(crate) fn new_local_janggi(
    mut config: GameConfig,
    seed: u64,
    player: Color,
    local_mode: &str,
) -> Result<GameState> {
    if !matches!(local_mode, "single" | "offline") {
        return Err(EngineError::InvalidConfig(format!(
            "v7 local Janggi mode {local_mode} requires single or offline"
        )));
    }
    let mut before = new_game(config.clone(), seed)?;
    // campaignDisplayGoal calls createCampaignBoard even when its result is
    // discarded. Keep this source 32-draw preview before resetGame(false).
    let _preview = crate::v7_campaign::campaign_board(&mut before, "janggi", player)?;
    // main71289 ignores the selected normal/chaos/grand UI style while in
    // campaign playMode. The shared reset does not activate selected RULEs.
    config.game_style = "normal".into();
    let mut state = crate::draft::reset_for_ruleset(
        &config,
        before.rng,
        RULES_VERSION_V7,
        Some(crate::draft::LocalCampaignResetContext {
            player_color: player,
            local_mode,
        }),
    )?;
    crate::v7_campaign::initialize_campaign(&mut state, "janggi", player, local_mode)?;
    state.validate_v7_snapshot_shape_and_identify()?;
    // Use the same full JCS DTO boundary as ordinary new_game. The adapter
    // subsequently constructs and checks the source Position envelope.
    serde_json::from_slice(&serde_jcs::to_vec(&state).map_err(EngineError::serialization)?)
        .map_err(EngineError::serialization)
}

#[cfg(test)]
#[path = "v7_new_game_tests.rs"]
mod v7_new_game_tests;
