//! Source-shaped v7 public event construction and structural replay admission.
//!
//! The frozen JS adapter records one event after a successful action: the raw
//! action payload, turn-change flag, and two viewer-specific public deltas.
//! Position identity is the host's responsibility and includes the appended
//! history. This module never infers private state from a public replay.

use crate::state::{
    Action, BoardChange, Color, EngineError, GameResult, GameState, Observation, PublicEvent,
    PublicTransition, RULES_VERSION_V7, Result, ResultRecord, Sides, Square, validate_json_value,
};
use serde_json::Value;

const EVENT_PROTOCOL: &str = "accelerate-game-event-v1";
const RESULT_PROTOCOL: &str = "accelerate-result-v1";

/// Append one source-shaped public event to an already settled v7 working
/// state. `after.history` must still equal `before.history`; the caller commits
/// the working state with `V7HostPosition` only after this succeeds. The
/// source action must already be bound and executed by the game host.
#[allow(
    dead_code,
    reason = "root wires the v7 transition after independent review"
)]
pub(crate) fn append_event(
    before: &GameState,
    after: &mut GameState,
    source_action: &Action,
) -> Result<PublicEvent> {
    if before.ruleset_id != RULES_VERSION_V7 || after.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 public event requires the pinned v7 rules version".into(),
        ));
    }
    if before.history != after.history {
        return Err(EngineError::InvalidState(
            "v7 event construction requires unchanged prior history".into(),
        ));
    }
    if source_action.position_key.is_some() || source_action.color != before.decision_actor() {
        return Err(EngineError::InvalidState(
            "v7 event action has a private position binding or wrong decision actor".into(),
        ));
    }
    let before_views = observe_pair(before)?;
    let after_views = observe_pair(after)?;
    let result = result_value(after)?;
    let event = PublicEvent {
        protocol_version: EVENT_PROTOCOL.into(),
        actor: source_action.color,
        action: source_action.clone(),
        turn_changed: before.turn != after.turn,
        public: Sides::new(
            transition(
                &before_views.white,
                &after_views.white,
                after,
                source_action.color,
                &result,
            )?,
            transition(
                &before_views.black,
                &after_views.black,
                after,
                source_action.color,
                &result,
            )?,
        ),
    };
    validate_event_shape(&event)?;
    let raw = serde_json::to_value(&event).map_err(EngineError::serialization)?;
    validate_json_value(&raw, 2)?;
    // Validate both public views including the new transition before exposing
    // the event in the host-owned history. Errors leave the working state as it
    // was so a host transaction can roll back without partial history.
    for (viewer, view) in [
        (Color::White, &after_views.white),
        (Color::Black, &after_views.black),
    ] {
        let mut projected = view.clone();
        projected.history = vec![
            serde_json::to_value(event.public.get(viewer)).map_err(EngineError::serialization)?,
        ];
        crate::observation::validate_projection_for_ruleset(&projected, RULES_VERSION_V7)?;
    }
    after.history.push(raw);
    Ok(event)
}

/// Validate the v7 event envelope and both public projections in an imported
/// state. This is a structural/visibility check, not proof that historic
/// actions could be executed from an unavailable private initial position.
#[allow(
    dead_code,
    reason = "root wires v7 history admission after independent review"
)]
pub(crate) fn validate_history(state: &GameState) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 history validation requires the pinned v7 rules version".into(),
        ));
    }
    for (index, raw) in state.history.iter().enumerate() {
        validate_json_value(raw, 2)?;
        let event: PublicEvent = serde_json::from_value(raw.clone()).map_err(|error| {
            EngineError::InvalidState(format!("v7 history[{index}] is invalid: {error}"))
        })?;
        validate_event_shape(&event).map_err(|error| {
            EngineError::InvalidState(format!(
                "v7 history[{index}] has invalid public event: {error}"
            ))
        })?;
    }
    for viewer in [Color::White, Color::Black] {
        let observation = state.observe_checked(viewer)?;
        crate::observation::validate_projection_for_ruleset(&observation, RULES_VERSION_V7)?;
    }
    Ok(())
}

fn observe_pair(state: &GameState) -> Result<Sides<Observation>> {
    let white = state.observe_checked(Color::White)?;
    let black = state.observe_checked(Color::Black)?;
    crate::observation::validate_projection_for_ruleset(&white, RULES_VERSION_V7)?;
    crate::observation::validate_projection_for_ruleset(&black, RULES_VERSION_V7)?;
    Ok(Sides::new(white, black))
}

fn transition(
    before: &Observation,
    after: &Observation,
    state: &GameState,
    actor: Color,
    result: &Value,
) -> Result<PublicTransition> {
    if before.viewer != after.viewer || before.board.len() != 8 || after.board.len() != 8 {
        return Err(EngineError::UnsupportedFeature(
            "v7 public event across changed board geometry or viewer".into(),
        ));
    }
    let mut board_changes = Vec::new();
    for row in 0..8 {
        if before.board[row].len() != 8 || after.board[row].len() != 8 {
            return Err(EngineError::UnsupportedFeature(
                "v7 public event across changed board geometry".into(),
            ));
        }
        for col in 0..8 {
            if before.board[row][col] != after.board[row][col] {
                board_changes.push(BoardChange {
                    square: Square {
                        row: row as u8,
                        col: col as u8,
                    },
                    before: before.board[row][col].clone(),
                    after: after.board[row][col].clone(),
                });
            }
        }
    }
    let revealed_opponent_cards = after
        .public_state
        .get("revealedOpponentCards")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineError::UnsupportedFeature(
                "v7 public transition has no reviewed revealedOpponentCards surface".into(),
            )
        })?
        .clone();
    let captures: Sides<Vec<Value>> = serde_json::from_value(
        after
            .public_state
            .get("captures")
            .ok_or_else(|| {
                EngineError::UnsupportedFeature(
                    "v7 public transition has no reviewed captures surface".into(),
                )
            })?
            .clone(),
    )
    .map_err(|error| {
        EngineError::InvalidState(format!("invalid v7 public captures surface: {error}"))
    })?;
    Ok(PublicTransition {
        kind: "transition".into(),
        actor,
        next_actor: state.decision_actor(),
        phase: state.mode.clone(),
        board_changes,
        own_cards: after.own_cards.clone(),
        revealed_opponent_cards,
        captures,
        result: result.clone(),
    })
}

fn result_value(state: &GameState) -> Result<Value> {
    let terminal = state.mode == "gameover";
    let (winner, outcome) = if terminal {
        match state.winner.as_deref() {
            Some("white") => (Some(Color::White), Some(GameResult::White)),
            Some("black") => (Some(Color::Black), Some(GameResult::Black)),
            None | Some("") | Some("draw") => (None, Some(GameResult::Draw)),
            Some(other) => {
                return Err(EngineError::InvalidState(format!(
                    "invalid v7 terminal winner {other}"
                )));
            }
        }
    } else {
        (None, None)
    };
    let reason = if terminal {
        match state.extra.get("replayEndReason") {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(reason)) if reason.len() <= 1024 => reason.clone(),
            _ => {
                return Err(EngineError::InvalidState(
                    "v7 terminal replayEndReason must be text within 1024 bytes".into(),
                ));
            }
        }
    } else {
        String::new()
    };
    serde_json::to_value(ResultRecord {
        protocol_version: RESULT_PROTOCOL.into(),
        status: if terminal { "terminal" } else { "ongoing" }.into(),
        winner,
        outcome,
        reason,
    })
    .map_err(EngineError::serialization)
}

fn validate_event_shape(event: &PublicEvent) -> Result<()> {
    if event.protocol_version != EVENT_PROTOCOL
        || event.actor != event.action.color
        || event.action.position_key.is_some()
    {
        return Err(EngineError::InvalidState(
            "v7 event protocol, actor, or action binding mismatch".into(),
        ));
    }
    let first = &event.public.white;
    let second = &event.public.black;
    if first.result != second.result
        || first.next_actor != second.next_actor
        || first.phase != second.phase
    {
        return Err(EngineError::InvalidState(
            "v7 viewer events disagree on public result or phase".into(),
        ));
    }
    for transition in [first, second] {
        if transition.kind != "transition"
            || transition.actor != event.actor
            || transition.phase.is_empty()
        {
            return Err(EngineError::InvalidState(
                "v7 public transition identity is invalid".into(),
            ));
        }
        let mut prior = None;
        for change in &transition.board_changes {
            if change.square.row >= 8 || change.square.col >= 8 {
                return Err(EngineError::InvalidState(
                    "v7 public board change leaves the 8x8 source board".into(),
                ));
            }
            let index = u16::from(change.square.row) * 8 + u16::from(change.square.col);
            if prior.is_some_and(|prior| index <= prior) {
                return Err(EngineError::InvalidState(
                    "v7 public board changes must be unique and row-major".into(),
                ));
            }
            prior = Some(index);
        }
        let result: ResultRecord =
            serde_json::from_value(transition.result.clone()).map_err(|error| {
                EngineError::InvalidState(format!("invalid v7 event result: {error}"))
            })?;
        if result.protocol_version != RESULT_PROTOCOL
            || !matches!(result.status.as_str(), "ongoing" | "terminal")
            || (result.status == "ongoing"
                && (result.winner.is_some()
                    || result.outcome.is_some()
                    || !result.reason.is_empty()))
            || (result.status == "terminal"
                && match result.outcome {
                    Some(GameResult::White) => result.winner != Some(Color::White),
                    Some(GameResult::Black) => result.winner != Some(Color::Black),
                    Some(GameResult::Draw) => result.winner.is_some(),
                    None => true,
                })
        {
            return Err(EngineError::InvalidState(
                "v7 event result has inconsistent terminal fields".into(),
            ));
        }
        if result.reason.len() > 1024 {
            return Err(EngineError::InvalidState(
                "v7 event result reason exceeds 1024 bytes".into(),
            ));
        }
        for captures in [&transition.captures.white, &transition.captures.black] {
            if captures.len() > 12 {
                return Err(EngineError::InvalidState(
                    "v7 public captures exceed the source display window".into(),
                ));
            }
            for piece in captures {
                validate_public_capture(piece)?;
            }
        }
    }
    Ok(())
}

fn validate_public_capture(piece: &Value) -> Result<()> {
    let fields = piece
        .as_object()
        .ok_or_else(|| EngineError::InvalidState("v7 public capture must be an object".into()))?;
    if fields
        .keys()
        .any(|key| !matches!(key.as_str(), "type" | "color" | "logDir" | "windmillMode"))
        || fields
            .get("type")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        || !matches!(
            fields.get("color").and_then(Value::as_str),
            Some("white" | "black" | "neutral")
        )
    {
        return Err(EngineError::InvalidState(
            "v7 public capture contains missing or private piece fields".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    fn first_draft(style: &str) -> (GameState, Action, GameState) {
        let before = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        let action = crate::draft::legal_actions(&before).unwrap().remove(1);
        let mut after = before.clone();
        crate::draft::apply_pick(&mut after, &action).unwrap();
        crate::replay::canonicalize_position_frames(&mut after).unwrap();
        (before, action, after)
    }

    #[test]
    fn seed19_first_draft_events_match_the_pinned_source_in_all_modes() {
        // Frozen client e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c;
        // GameAdapter.newGame({gameStyle}, 19), actions()[1], apply(recordHistory=true).
        for (style, expected_digest) in [
            (
                "normal",
                "1cfbf41fcc51dfcacabb63595faf6bc43d9458428de2692b446b5c47070c67c0",
            ),
            (
                "chaos",
                "c2336a7e6d481b6c1595d1bde88630a8e2235f7e05eda600f390b75f78631f6e",
            ),
            (
                "grand",
                "d533af8e75c363ec1a410e364765ba90dc41cc56424ab0a5dd60630798007e58",
            ),
        ] {
            let (before, action, mut after) = first_draft(style);
            let event = append_event(&before, &mut after, &action).unwrap();
            let digest = format!("{:x}", Sha256::digest(serde_jcs::to_vec(&event).unwrap()));
            assert_eq!(
                digest, expected_digest,
                "source-pinned {style} seed-19 first draft event"
            );
            assert_eq!(after.history.len(), 1);
            validate_history(&after).unwrap();
        }
    }

    #[test]
    fn invalid_action_and_history_leave_working_history_unchanged() {
        let (before, mut action, mut after) = first_draft("normal");
        action.position_key = Some("private-position-id".into());
        let original_history = after.history.clone();
        assert!(matches!(
            append_event(&before, &mut after, &action),
            Err(EngineError::InvalidState(message)) if message.contains("private position binding")
        ));
        assert_eq!(after.history, original_history);

        let (before, action, mut after) = first_draft("normal");
        after.history.push(json!({"unreviewed": "event"}));
        assert!(matches!(
            append_event(&before, &mut after, &action),
            Err(EngineError::InvalidState(message)) if message.contains("unchanged prior history")
        ));
        assert_eq!(after.history.len(), 1);
    }

    #[test]
    fn imported_history_rejects_private_public_card_fields() {
        let (before, action, mut after) = first_draft("normal");
        append_event(&before, &mut after, &action).unwrap();
        after.history[0]["public"]["white"]["ownCards"][0]["privateCardSecret"] = json!(7);
        assert!(matches!(
            validate_history(&after),
            Err(EngineError::InvalidState(message)) if message.contains("public card") || message.contains("history.cards")
        ));
    }

    #[test]
    fn host_transaction_commits_event_and_revision_together() {
        let (before, action, _) = first_draft("normal");
        let host = crate::v7_host::V7HostPosition::from_state(before).unwrap();
        let original_id = host.position_id().to_owned();
        let (next, event) = host
            .transact(&original_id, |working| {
                crate::draft::apply_pick(working, &action)?;
                crate::replay::canonicalize_position_frames(working)?;
                append_event(host.state(), working, &action)
            })
            .unwrap();

        assert_eq!(host.revision(), 0);
        assert!(host.state().history.is_empty());
        assert_ne!(next.position_id(), original_id);
        assert_eq!(next.revision(), 1);
        assert_eq!(next.state().history.len(), 1);
        assert_eq!(
            next.state().history[0],
            serde_json::to_value(event).unwrap()
        );
        validate_history(next.state()).unwrap();
    }
}
