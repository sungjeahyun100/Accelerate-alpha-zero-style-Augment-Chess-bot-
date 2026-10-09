//! Frozen v7 completed-turn flow guard and ordered project-local stages.
//!
//! `completeTurnAfterMove` has callbacks on both sides of `turnsTaken++` and
//! `state.turn = opponent(movingColor)`. Those boundaries determine RNG,
//! capture, replay and terminal ordering. The host invokes this guard before
//! mutating its transaction copy; unsupported active callbacks are explicit
//! errors rather than silently skipped by a shorter turn path.

use crate::{Color, EngineError, GameState, RULES_VERSION_V7, Result};
use serde_json::Value;

#[allow(
    dead_code,
    reason = "integration calls the ordered source stages after the transition split"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum V7TurnStage {
    CommitClock,
    ActorBeforeCount,
    IncrementTurnsTaken,
    ActorAfterCount,
    ActorCleanup,
    SwitchActor,
    IncomingReservations,
    IncomingAutomaticEffects,
    NextActionAndClock,
    TerminalDraftAndNoAction,
}

/// Source order anchors for the integration owner. Sub-stages retain the
/// source's terminal short-circuits; callers must not reorder them merely
/// because a callback appears inert in one fixture.
#[allow(dead_code, reason = "source order contract for transition integration")]
pub(crate) const V7_SOURCE_TURN_STAGES: [V7TurnStage; 10] = [
    V7TurnStage::CommitClock,
    V7TurnStage::ActorBeforeCount,
    V7TurnStage::IncrementTurnsTaken,
    V7TurnStage::ActorAfterCount,
    V7TurnStage::ActorCleanup,
    V7TurnStage::SwitchActor,
    V7TurnStage::IncomingReservations,
    V7TurnStage::IncomingAutomaticEffects,
    V7TurnStage::NextActionAndClock,
    V7TurnStage::TerminalDraftAndNoAction,
];

/// Source callbacks may finish normally, retain the moving side's turn, or
/// finish the game. The transition owner handles these outcomes at the source
/// stage boundary instead of inferring control flow from an effect's fields.
#[allow(
    dead_code,
    reason = "shared result contract for separately owned v7 callbacks"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum V7FlowControl {
    Continue,
    RetainTurn,
    Terminal,
}

#[derive(Clone, Copy, Debug, Default)]
/// Internal completed-turn callbacks use source `movingColor`, which may
/// differ from state.turn after automatic Spy promotion. Public action actor
/// authentication belongs to admission; these stages never reassign turn.
pub(crate) struct V7TurnFlowRule;

impl V7TurnFlowRule {
    /// Clear actor-owned one-turn flags before the source increments
    /// `turnsTaken`. This runs after `preflight` inside a host transaction.
    pub(crate) fn clear_actor_pre_count_flags(
        self,
        state: &mut GameState,
        actor: Color,
    ) -> Result<()> {
        require_live_completed_turn_state(state)?;
        reject_draw_offer_if_recipient_moved(state, actor)?;
        for field in ["symmetry", "e4", "solidarity", "roller", "substitution"] {
            let Some(value) = state.extra.get_mut(field) else {
                continue;
            };
            if value.is_null() || value.as_bool() == Some(false) {
                continue;
            }
            let sides = value.as_object_mut().ok_or_else(|| {
                EngineError::InvalidState(format!("v7 {field} must be a color map"))
            })?;
            sides.insert(actor.as_str().into(), Value::Bool(false));
        }
        if let Some(value) = state.extra.get_mut("bishopInfiltration")
            && !value.is_null()
        {
            let sides = value.as_object_mut().ok_or_else(|| {
                EngineError::InvalidState("v7 bishopInfiltration must be a color map".into())
            })?;
            if let Some(current) = sides.get(actor.as_str()).and_then(Value::as_i64) {
                sides.insert(
                    actor.as_str().into(),
                    serde_json::json!(current.saturating_sub(1).max(0)),
                );
            } else if let Some(current) = sides.get(actor.as_str()).and_then(Value::as_f64) {
                sides.insert(
                    actor.as_str().into(),
                    serde_json::json!((current - 1.0).max(0.0)),
                );
            }
        }
        clear_royal_commands(state, actor)?;
        Ok(())
    }

    /// Source callbacks immediately after the black conveyor and before
    /// `turnsTaken[actor]++`: turn-only reversal/overtake and the September
    /// resolve-credit window. The v6 late reversal reset must not also run.
    pub(crate) fn after_board_before_count(
        self,
        state: &mut GameState,
        actor: Color,
    ) -> Result<()> {
        require_live_completed_turn_state(state)?;
        let active_overtake = uses_active_overtake(state)?;
        for field in [
            "reversal",
            "overtake",
            "resolveCreditPieceId",
            "resolveReady",
            "resolveMoveCredit",
        ] {
            if state
                .extra
                .get(field)
                .is_some_and(|value| crate::observation::truth(Some(value)) && !value.is_object())
            {
                return Err(EngineError::InvalidState(format!(
                    "v7 {field} must be a color map"
                )));
            }
        }
        clear_actor_if_truthy(state, "reversal", actor);
        if active_overtake {
            clear_actor_if_truthy(state, "overtake", actor);
        }
        if let Some(credits) = state
            .extra
            .get_mut("resolveCreditPieceId")
            .and_then(Value::as_object_mut)
        {
            credits.shift_remove(actor.as_str());
        }
        for field in ["resolveReady", "resolveMoveCredit"] {
            clear_actor_if_truthy(state, field, actor);
        }
        Ok(())
    }

    /// Check source-reachable hooks that the current native transition does
    /// not yet settle. This is a necessary guard, not a declaration that the
    /// remaining callbacks have reached full source parity.
    pub(crate) fn preflight(self, state: &GameState, _actor: Color) -> Result<()> {
        if state.ruleset_id == RULES_VERSION_V7 && state.mode == "gameover" {
            return Ok(());
        }
        require_live_completed_turn_state(state)?;

        // Source: rejectDrawOfferIfRecipientMoved and the actor's own
        // one-turn state are processed immediately after the clock commits.
        for (field, callback) in [("activeTrolley", "activatePendingTrolleyForTurn")] {
            if state.extra.get(field).is_some_and(active) {
                return Err(unsupported(field, callback));
            }
        }
        // Herald, Racing King and Double Check settle at their exact source
        // callback. The predicate itself reports any unsupported active
        // attack branch instead of rejecting every state with its rule flag.
        // Shotgun `snipeCooldown`, Blood Moon's deterministic day/cycle
        // branches, and Time Traveler card recovery are handled at their
        // respective source stages. The first unsupported random Blood Moon
        // grant still fails explicitly in maybe_grant_night_blood_for_turn.
        Ok(())
    }
}

fn require_live_completed_turn_state(state: &GameState) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 completed turn on rules version {}",
            state.ruleset_id,
        )));
    }
    if state.mode != "play" {
        return Err(EngineError::InvalidState(format!(
            "v7 completed-turn callback requires play mode, found {}",
            state.mode,
        )));
    }
    Ok(())
}

/// Frozen `checkNoActionLoss`: the source probes playable cards before raw
/// movement, then piece actions. A partial move list may prove that a move
/// exists, but it cannot establish that the player is immobile. Work on an
/// owned copy so an unsupported predicate never commits the card probe's RNG
/// or a terminal event to the caller's state.
pub(crate) fn check_no_action_loss_v7(state: &mut GameState) -> Result<bool> {
    let actor = state.turn;
    check_no_action_loss_for_color_v7(state, actor)
}

/// Source checkNoActionLoss(color)도 completed-move의 현재 기물 색을 받는다.
/// Spy 자동 승급 뒤에는 이 색과 state.turn이 다를 수 있다. 공개 행동 인증은
/// admission에서 유지하고, 이 내부 조회는 source turn을 재배정하지 않는다.
pub(crate) fn check_no_action_loss_for_color_v7(
    state: &mut GameState,
    actor: Color,
) -> Result<bool> {
    crate::legal_profile::measure("no_action_loss", || {
        check_no_action_loss_for_color_v7_profiled(state, actor)
    })
}

pub(crate) fn check_no_action_loss_for_color_v7_profiled(
    state: &mut GameState,
    actor: Color,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 no-action verdict on rules version {}",
            state.ruleset_id
        )));
    }
    if state.mode != "play"
        || crate::observation::truth(state.extra.get("pendingPromotion"))
        || crate::observation::truth(state.extra.get("targeting"))
    {
        return Ok(false);
    }
    let mut working = state.clone();
    break_out_of_range_chain_bonds(&mut working)?;
    let forced_extra_move = has_forced_extra_move(&working, actor);
    if !forced_extra_move && crate::transition::available_card_action(&mut working, actor)? {
        *state = working;
        return Ok(false);
    }
    // available_card_action now enumerates the entire source deck through
    // is_v7_ai_card_playable. An incomplete predicate returns its own error;
    // an exhausted populated hand may proceed to the movement verdict.
    if crate::movement::v7_has_any_legal_move_live(&mut working, actor)? {
        *state = working;
        return Ok(false);
    }
    // A false move answer is only valid when the movement module has proved
    // complete source getLegalMoves coverage, including the neutral football.
    if has_available_piece_action(&working, actor) {
        *state = working;
        return Ok(false);
    }
    crate::flow::end_game(
        &mut working,
        Some(actor.opponent()),
        &format!(
            "{}은 사용할 카드와 움직일 수 있는 기물이 없습니다.",
            crate::replay::label(actor)
        ),
    )?;
    *state = working;
    Ok(true)
}

/// Source `breakOutOfRangeChainBonds` runs before card availability. The
/// normalizer owns its 64-entry cap, pair deduplication, ID truncation and
/// default fields; this stage only partitions its canonical bonds by the
/// source's Chebyshev distance of two. Both the normalized array and any log
/// remain private until the whole no-action probe succeeds.
fn break_out_of_range_chain_bonds(state: &mut GameState) -> Result<()> {
    crate::v7_board_automata::break_out_of_range_chain_bonds(state).map(|_| ())
}

fn has_forced_extra_move(state: &GameState, actor: Color) -> bool {
    state.board.iter().flatten().flatten().any(|piece| {
        piece.color == actor
            && [
                "thiefSecondMove",
                "frenzyExtraMove",
                "fileSurgeSecondMove",
                "rookLiftSecondMove",
                "ironMonarchExtraMove",
                "underpromotionSecondMove",
                "checkerChainCapture",
                "madHorseSecondMove",
                "platformExtraMove",
                "desperado",
            ]
            .into_iter()
            .any(|field| crate::observation::truth(piece.extra.get(field)))
    })
}

pub(crate) fn has_available_piece_action(state: &GameState, actor: Color) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    state.board.iter().flatten().flatten().any(|piece| {
        if piece.color != actor || !seen.insert(piece.id.as_str()) {
            return false;
        }
        // main94754 uses nullish defaults, then JS relational comparison.
        // An invalid number is NaN and compares false; it must not become a
        // default reload action. Two string operands compare lexically.
        if piece.kind == "shotgunKing" {
            let ammo = piece.extra.get("ammo").filter(|value| !value.is_null());
            let maximum = piece.extra.get("maxAmmo").filter(|value| !value.is_null());
            let reload = match (ammo, maximum) {
                (Some(Value::String(left)), Some(Value::String(right))) => {
                    left.encode_utf16().cmp(right.encode_utf16()).is_lt()
                }
                _ => ammo
                    .map_or(Some(0.0), |value| {
                        crate::card_effects::js_number(Some(value), 0)
                    })
                    .zip(maximum.map_or(Some(3.0), |value| {
                        crate::card_effects::js_number(Some(value), 0)
                    }))
                    .is_some_and(|(ammo, maximum)| ammo < maximum),
            };
            if reload {
                return true;
            }
        }
        piece.ability_kind() == "wizard"
            && piece
                .extra
                .get("mana")
                .filter(|value| !value.is_null())
                .map_or(Some(0.0), |value| {
                    crate::card_effects::js_number(Some(value), 0)
                })
                .is_some_and(|mana| mana >= 1.0)
    })
}

fn clear_actor_if_truthy(state: &mut GameState, field: &str, actor: Color) {
    if let Some(value) = state.extra.get_mut(field)
        && crate::observation::truth(Some(value))
    {
        value
            .as_object_mut()
            .expect("truthy color maps were validated")
            .insert(actor.as_str().into(), Value::Bool(false));
    }
}

/// Frozen `usesActiveOvertake`. The exact catalog allowlist is part of the
/// source's legacy profile compatibility and does not follow the ruleset ID.
fn uses_active_overtake(state: &GameState) -> Result<bool> {
    const ACTIVE_HASHES: &[&str] = &[
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
    ];
    let card_state = state
        .extra
        .get("cardState")
        .filter(|value| crate::observation::truth(Some(value)));
    let profile = if let Some(card_state) = card_state {
        card_state.get("profile")
    } else {
        state.extra.get("profile")
    };
    let hash = profile
        .and_then(|profile| profile.get("catalogHash"))
        .filter(|hash| crate::observation::truth(Some(hash)));
    if let Some(hash) = hash {
        let hash = hash.as_str().ok_or_else(|| {
            EngineError::InvalidState("v7 profile.catalogHash must be a string".into())
        })?;
        return Ok(ACTIVE_HASHES.contains(&hash));
    }
    Ok(state.extra.get("overtakeTurnOnly") != Some(&Value::Bool(false)))
}

/// Source `rejectDrawOfferIfRecipientMoved` runs before one-turn flags are
/// cleared. A proposal belongs to its offerer; the other side's move rejects
/// it only when the proposal has a source `userId`.
fn reject_draw_offer_if_recipient_moved(state: &mut GameState, actor: Color) -> Result<()> {
    let Some(offer) = state.extra.get("drawOffer") else {
        return Ok(());
    };
    if offer.is_null() {
        return Ok(());
    }
    let offer = offer.as_object().ok_or_else(|| {
        EngineError::InvalidState("v7 drawOffer must be an object or null".into())
    })?;
    if !crate::observation::truth(offer.get("userId")) {
        return Ok(());
    }
    let offer_color = offer.get("color").and_then(Value::as_str).ok_or_else(|| {
        EngineError::InvalidState("v7 drawOffer with userId needs a color".into())
    })?;
    if offer_color != "white" && offer_color != "black" {
        return Err(EngineError::InvalidState(
            "v7 drawOffer color must be white or black".into(),
        ));
    }
    if offer_color == actor.as_str() {
        return Ok(());
    }
    if state
        .extra
        .get("logs")
        .is_some_and(|value| !value.is_array())
    {
        return Err(EngineError::InvalidState("v7 logs must be an array".into()));
    }
    state.extra.insert("drawOffer".into(), Value::Null);
    state
        .extra
        .entry("logs")
        .or_insert_with(|| Value::Array(Vec::new()));
    let label = match actor {
        Color::White => "백",
        Color::Black => "흑",
    };
    crate::replay::add_log(state, format!("{label}이 무승부 요청을 거부했습니다."))
}

/// Source `clearRoyalCommands` expires a window against `turnsTaken + 1`
/// before that counter is incremented. Multi-square identities are separate
/// Rust values, so every occupied square of the actor's identity is cleared.
fn clear_royal_commands(state: &mut GameState, actor: Color) -> Result<()> {
    let window = match state.extra.get("royalCommand") {
        None | Some(Value::Null) => None,
        Some(Value::Object(sides)) => sides.get(actor.as_str()).cloned(),
        Some(_) => {
            return Err(EngineError::InvalidState(
                "v7 royalCommand must be a color map or null".into(),
            ));
        }
    };
    if window
        .as_ref()
        .is_some_and(|window| !window.is_null() && !window.is_object())
    {
        return Err(EngineError::InvalidState(format!(
            "v7 royalCommand.{} must be an object or null",
            actor.as_str()
        )));
    }
    let expire = window
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|window| {
            let active = crate::observation::number(window.get("activeTurn"))
                .filter(|value| value.fract() == 0.0 && *value >= 0.0)?;
            let expires = crate::observation::number(window.get("expiresTurn"))
                .filter(|value| value.fract() == 0.0 && *value > active)
                .unwrap_or(active + 1.0);
            Some(f64::from(*state.turns_taken.get(actor)) + 1.0 >= expires)
        })
        .unwrap_or(false);
    for piece in state.board.iter_mut().flatten().flatten() {
        if piece.color == actor && crate::observation::truth(piece.extra.get("royalCommand")) {
            piece.extra.shift_remove("royalCommand");
        }
    }
    let sides = state
        .extra
        .entry("royalCommand")
        .or_insert_with(|| serde_json::json!({"white":null,"black":null}));
    if sides.is_null() {
        *sides = serde_json::json!({"white":null,"black":null});
    }
    if expire {
        sides
            .as_object_mut()
            .expect("royalCommand map was checked before board mutation")
            .insert(actor.as_str().into(), Value::Null);
    }
    Ok(())
}

fn active(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => values
            .iter()
            .any(|(key, value)| !key.starts_with("__") && active(value)),
    }
}

fn unsupported(field: &str, callback: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!(
        "v7 completed turn requires unported {callback} for active {field}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;
    use serde_json::json;

    fn play_state() -> GameState {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.mode = "play".into();
        state
    }

    #[test]
    fn pending_retaliation_queues_are_settled_at_their_source_stage() {
        let mut state = play_state();
        state
            .extra
            .insert("pendingTrojanHorse".into(), json!([{"id":"horse-1"}]));
        let before = state.clone();
        V7TurnFlowRule.preflight(&state, Color::White).unwrap();
        assert_eq!(state, before);
    }

    #[test]
    fn active_crown_rule_is_checked_at_its_board_stage() {
        let mut state = play_state();
        state.extra.insert("crownRule".into(), json!(true));
        V7TurnFlowRule.preflight(&state, Color::White).unwrap();
        let stage = crate::v7_board_automata::V7BoardAutomata
            .after_empty_lunchboxes(&mut state, Color::White)
            .unwrap();
        assert_eq!(stage, V7FlowControl::Continue);
    }

    #[test]
    fn source_completed_turn_uses_the_changed_mover_color_without_changing_turn() {
        // main93646 receives movingColor rather than authenticating an
        // Action. Automatic Spy promotion can change that color while the
        // admitted action's state.turn remains White until the switch stage.
        let mut state = play_state();
        state.extra.insert(
            "profile".into(),
            json!({"catalogHash":"yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4"}),
        );
        state
            .extra
            .insert("symmetry".into(), json!({"white":true,"black":true}));
        state
            .extra
            .insert("bishopInfiltration".into(), json!({"white":2,"black":3}));
        state
            .extra
            .insert("reversal".into(), json!({"white":true,"black":true}));
        state
            .extra
            .insert("overtake".into(), json!({"white":true,"black":true}));
        state.extra.insert(
            "resolveCreditPieceId".into(),
            json!({"white":"white-credit","black":"black-credit"}),
        );
        state
            .extra
            .insert("resolveReady".into(), json!({"white":true,"black":true}));
        state.extra.insert(
            "resolveMoveCredit".into(),
            json!({"white":true,"black":true}),
        );
        let turn = state.turn;
        let turns = state.turns_taken.clone();
        let rng = state.rng.clone();
        V7TurnFlowRule.preflight(&state, Color::Black).unwrap();
        V7TurnFlowRule
            .clear_actor_pre_count_flags(&mut state, Color::Black)
            .unwrap();
        assert_eq!(state.extra["symmetry"], json!({"white":true,"black":false}));
        assert_eq!(
            state.extra["bishopInfiltration"],
            json!({"white":2,"black":2})
        );
        V7TurnFlowRule
            .after_board_before_count(&mut state, Color::Black)
            .unwrap();
        for field in ["reversal", "overtake", "resolveReady", "resolveMoveCredit"] {
            assert_eq!(
                state.extra[field],
                json!({"white":true,"black":false}),
                "{field}"
            );
        }
        assert_eq!(
            state.extra["resolveCreditPieceId"],
            json!({"white":"white-credit"})
        );
        assert_eq!(state.turn, turn);
        assert_eq!(state.turns_taken, turns);
        assert_eq!(state.rng, rng);
    }

    #[test]
    fn completed_turn_wrong_mode_is_rejected_before_clearing_mover_flags() {
        let mut state = play_state();
        state.mode = "draft".into();
        state
            .extra
            .insert("symmetry".into(), json!({"white":true,"black":true}));
        let before = state.clone();
        for result in [
            V7TurnFlowRule.preflight(&state, Color::Black),
            V7TurnFlowRule.clear_actor_pre_count_flags(&mut state, Color::Black),
            V7TurnFlowRule.after_board_before_count(&mut state, Color::Black),
        ] {
            assert!(matches!(result, Err(EngineError::InvalidState(message))
                if message == "v7 completed-turn callback requires play mode, found draft"));
            assert_eq!(state, before);
        }
    }

    #[test]
    fn actor_flags_clear_before_turn_count_without_touching_other_side() {
        let mut state = play_state();
        state
            .extra
            .insert("symmetry".into(), json!({"white":true,"black":true}));
        state
            .extra
            .insert("e4".into(), json!({"white":true,"black":false}));
        state
            .extra
            .insert("bishopInfiltration".into(), json!({"white":2,"black":3}));
        let before_turns = state.turns_taken.clone();
        V7TurnFlowRule
            .clear_actor_pre_count_flags(&mut state, Color::White)
            .unwrap();
        assert_eq!(state.extra["symmetry"], json!({"white":false,"black":true}));
        assert_eq!(state.extra["e4"], json!({"white":false,"black":false}));
        assert_eq!(
            state.extra["bishopInfiltration"],
            json!({"white":1,"black":3})
        );
        assert_eq!(state.turns_taken, before_turns);
    }

    #[test]
    fn recipient_move_rejects_offer_and_expires_actor_royal_command() {
        let mut state = play_state();
        state.extra.insert(
            "drawOffer".into(),
            json!({"userId":"remote-player","color":"black"}),
        );
        state.extra.insert("logs".into(), json!(["prior"]));
        state.extra.insert(
            "royalCommand".into(),
            json!({"white":{"activeTurn":0,"expiresTurn":1},"black":{"activeTurn":0,"expiresTurn":2}}),
        );
        for piece in state.board.iter_mut().flatten().flatten() {
            if piece.kind == "king" {
                piece.extra.insert("royalCommand".into(), json!(true));
            }
        }
        let before_turns = state.turns_taken.clone();
        V7TurnFlowRule
            .clear_actor_pre_count_flags(&mut state, Color::White)
            .unwrap();
        assert_eq!(state.extra["drawOffer"], Value::Null);
        assert_eq!(
            state.extra["logs"],
            json!(["백이 무승부 요청을 거부했습니다.", "prior"])
        );
        assert_eq!(state.extra["royalCommand"]["white"], Value::Null);
        assert_eq!(
            state.extra["royalCommand"]["black"],
            json!({"activeTurn":0,"expiresTurn":2})
        );
        for piece in state.board.iter().flatten().flatten() {
            if piece.kind == "king" && piece.color == Color::White {
                assert!(!piece.extra.contains_key("royalCommand"));
            }
            if piece.kind == "king" && piece.color == Color::Black {
                assert_eq!(piece.extra.get("royalCommand"), Some(&json!(true)));
            }
        }
        assert_eq!(state.turns_taken, before_turns);
    }

    #[test]
    fn moving_offer_owner_preserves_offer_and_future_royal_window() {
        let mut state = play_state();
        state.extra.insert(
            "drawOffer".into(),
            json!({"userId":"local-player","color":"white"}),
        );
        state.extra.insert(
            "royalCommand".into(),
            json!({"white":{"activeTurn":0,"expiresTurn":3},"black":null}),
        );
        V7TurnFlowRule
            .clear_actor_pre_count_flags(&mut state, Color::White)
            .unwrap();
        assert_eq!(
            state.extra["drawOffer"],
            json!({"userId":"local-player","color":"white"})
        );
        assert_eq!(
            state.extra["royalCommand"]["white"],
            json!({"activeTurn":0,"expiresTurn":3})
        );
    }

    #[test]
    fn royal_command_cleanup_keeps_falsy_piece_fields() {
        // Frozen source main-OahWs0tU.js SHA e5ed84fc..., seed-19
        // draftDelete first-play: clearRoyalCommands("white") preserves
        // own fields false/null and deletes true, including property presence.
        let mut state = play_state();
        let pawn = state.board[6][0].as_mut().unwrap();
        pawn.extra.insert("royalCommand".into(), json!(false));
        let other_pawn = state.board[6][1].as_mut().unwrap();
        other_pawn.extra.insert("royalCommand".into(), Value::Null);
        state.board[7][4]
            .as_mut()
            .unwrap()
            .extra
            .insert("royalCommand".into(), json!(true));

        V7TurnFlowRule
            .clear_actor_pre_count_flags(&mut state, Color::White)
            .unwrap();
        assert_eq!(
            state.board[6][0].as_ref().unwrap().extra["royalCommand"],
            json!(false)
        );
        assert_eq!(
            state.board[6][1].as_ref().unwrap().extra["royalCommand"],
            Value::Null
        );
        assert!(
            !state.board[7][4]
                .as_ref()
                .unwrap()
                .extra
                .contains_key("royalCommand")
        );
    }

    #[test]
    fn pre_count_resolve_credit_and_turn_only_flags_clear_in_actor_window() {
        let mut state = play_state();
        state.extra.insert(
            "profile".into(),
            json!({"catalogHash":"yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4"}),
        );
        for (field, value) in [
            ("reversal", json!({"white":true,"black":true})),
            ("overtake", json!({"white":true,"black":true})),
            (
                "resolveCreditPieceId",
                json!({"white":"pawn-1","black":"pawn-2"}),
            ),
            ("resolveReady", json!({"white":true,"black":true})),
            ("resolveMoveCredit", json!({"white":true,"black":true})),
        ] {
            state.extra.insert(field.into(), value);
        }
        let turns = state.turns_taken.clone();
        V7TurnFlowRule
            .after_board_before_count(&mut state, Color::White)
            .unwrap();
        assert_eq!(state.extra["reversal"], json!({"white":false,"black":true}));
        assert_eq!(state.extra["overtake"], json!({"white":false,"black":true}));
        assert_eq!(
            state.extra["resolveCreditPieceId"],
            json!({"black":"pawn-2"})
        );
        assert_eq!(
            state.extra["resolveReady"],
            json!({"white":false,"black":true})
        );
        assert_eq!(
            state.extra["resolveMoveCredit"],
            json!({"white":false,"black":true})
        );
        assert_eq!(state.turns_taken, turns);
    }

    #[test]
    fn inactive_legacy_overtake_profile_does_not_clear_active_flag() {
        let mut state = play_state();
        state.extra.insert(
            "profile".into(),
            json!({"catalogHash":"legacy-unknown-hash"}),
        );
        state
            .extra
            .insert("overtake".into(), json!({"white":true,"black":false}));
        V7TurnFlowRule
            .after_board_before_count(&mut state, Color::White)
            .unwrap();
        assert_eq!(state.extra["overtake"]["white"], json!(true));
    }

    #[test]
    fn herald_objective_flags_are_checked_at_their_ordered_callback() {
        let mut state = play_state();
        state
            .extra
            .insert("binaMate".into(), json!({"white":true,"black":false}));
        assert!(V7TurnFlowRule.preflight(&state, Color::White).is_ok());

        state
            .extra
            .insert("binaMate".into(), json!({"white":false,"black":false}));
        state
            .extra
            .insert("racingKing".into(), json!({"white":false,"black":false}));
        assert!(V7TurnFlowRule.preflight(&state, Color::White).is_ok());
    }

    #[test]
    fn implemented_later_turn_callbacks_are_not_rejected_at_preflight() {
        let mut state = play_state();
        state
            .extra
            .insert("campaign".into(), json!({"setup":"timeTraveler"}));
        state.board[7][4]
            .as_mut()
            .unwrap()
            .extra
            .insert("snipeCooldown".into(), json!(2));
        V7TurnFlowRule.preflight(&state, Color::White).unwrap();

        state.extra.insert(
            "campaign".into(),
            json!({"setup":"bloodMoon","bloodMoon":{"lastPeriod":0}}),
        );
        V7TurnFlowRule.preflight(&state, Color::White).unwrap();
    }

    #[test]
    fn source_draft_delete_first_play_no_action_probe_preserves_full_state() {
        // Pinned main-OahWs0tU.js SHA-256 e5ed84fc... / OracleRuntime
        // newGame({draftDelete:true},19), checkNoActionLoss(state.turn):
        // false, RNG cursor 32/state 4163866163 both before and after,
        // identical full state and virtual history, boardHistory length one.
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        assert_eq!(state.mode, "play");
        assert_eq!(state.rng.cursor, 32);
        assert_eq!(state.rng.state, 4_163_866_163);
        let before = state.clone();
        assert!(!check_no_action_loss_v7(&mut state).unwrap());
        assert_eq!(state, before);
    }

    #[test]
    fn no_action_internal_color_can_differ_from_turn_without_reassigning_it() {
        // main93365 uses movingColor after automatic Spy promotion. The
        // caller's turn still points at the original side, while hasAnyLegalMove
        // temporarily selects the requested side and restores the original turn.
        for (turn, actor) in [(Color::White, Color::Black), (Color::Black, Color::White)] {
            let mut state = crate::v7_new_game::new_game(
                GameConfig {
                    draft_delete: true,
                    ..GameConfig::default()
                },
                19,
            )
            .unwrap();
            state.turn = turn;
            for cell in state.board.iter_mut().flatten() {
                if cell.as_ref().is_some_and(|piece| piece.color == turn) {
                    *cell = None;
                }
            }
            let before = state.clone();
            assert!(!check_no_action_loss_for_color_v7(&mut state, actor).unwrap());
            assert_eq!(
                state, before,
                "{actor:?}: querying the completed mover must preserve the source turn"
            );
        }
    }

    #[test]
    fn source_first_play_chain_normalization_is_transactional_until_move_is_proven() {
        // Frozen client e5ed84fc, newGame({draftDelete:true}, 19): injecting
        // a2-h7 and the reversed duplicate, then calling checkNoActionLoss
        // drops the bond and adds one log before the ordinary pawn move
        // establishes that the player can continue.
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        let first = state.board[6][0].as_ref().unwrap().id.clone();
        let far = state.board[1][7].as_ref().unwrap().id.clone();
        assert_eq!(first, "white-pawn-lm81oj7t4fq");
        state.extra.insert(
            "chainBonds".into(),
            json!([
                {"aId":first,"bId":far},
                {"aId":far,"bId":first}
            ]),
        );
        let before = state.clone();
        let mut expected = before.clone();
        expected.extra.insert("chainBonds".into(), json!([]));
        expected
            .extra
            .get_mut("logs")
            .unwrap()
            .as_array_mut()
            .unwrap()
            .insert(0, json!("사슬: 연결된 기물이 사라지거나 사이가 3칸 이상 벌어져 1개의 사슬이 끊어졌습니다."));
        assert!(!check_no_action_loss_v7(&mut state).unwrap());
        assert_eq!(state, expected);
    }

    #[test]
    fn source_chain_partition_keeps_adjacent_pair_and_deduplicates_reversal() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        let first = state.board[6][0].as_ref().unwrap().id.clone();
        let second = state.board[6][1].as_ref().unwrap().id.clone();
        state.extra.insert(
            "chainBonds".into(),
            json!([{"aId":first,"bId":second},{"aId":second,"bId":first}]),
        );
        let before_logs = state.extra["logs"].clone();
        break_out_of_range_chain_bonds(&mut state).unwrap();
        assert_eq!(
            state.extra["chainBonds"],
            json!([{
                "id":"chain-0-white-pawn-lm81oj7t4fq-white-pawn-hrlb0fwnya4",
                "aId":first,"bId":second,"by":"white"
            }])
        );
        assert_eq!(state.extra["logs"], before_logs);
    }

    #[test]
    fn empty_source_board_and_deleted_draft_has_no_available_action() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        for row in &mut state.board {
            for cell in row {
                *cell = None;
            }
        }
        let before_rng = state.rng.clone();
        assert!(check_no_action_loss_v7(&mut state).unwrap());
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner.as_deref(), Some("black"));
        assert_eq!(state.rng, before_rng);
        assert_eq!(
            state.extra["replayEndReason"],
            "백은 사용할 카드와 움직일 수 있는 기물이 없습니다."
        );
    }
}
