//! Per-session Rust-only replay cost experiment. The setting is intentionally
//! absent from the source wire format and travels with owned GameState clones.
use crate::GameState;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMode {
    #[default]
    FullReplay,
    NoHistoryReplay,
    NoReplay,
}

impl ReplayMode {
    pub const fn keeps_history(self) -> bool {
        matches!(self, Self::FullReplay)
    }

    pub const fn keeps_move_capture(self) -> bool {
        !matches!(self, Self::NoReplay)
    }
}

/// Strip historical payloads before the first timed clone. Retain compact
/// counters for rule branches that inspect the number of recorded events.
pub(crate) fn prepare_state(state: &mut GameState, mode: ReplayMode) {
    state.replay_mode = mode;
    if mode.keeps_history() {
        return;
    }
    for (name, counter) in [
        ("replayEvents", "replayExperimentEventCount"),
        ("boardHistory", "replayExperimentBoardCount"),
    ] {
        let count = state
            .extra
            .get(name)
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        state.extra.insert(counter.into(), json!(count));
        state.extra.insert(name.into(), json!([]));
    }
    for name in ["replayBaseFrame", "replayTailFrame"] {
        state.extra.insert(name.into(), Value::Null);
    }
    state.extra.insert("notationTimeline".into(), json!([]));
    if mode == ReplayMode::NoReplay {
        state
            .extra
            .insert("moveReplay".into(), json!({"white":null,"black":null}));
        state.active_move_replay_before = None;
        state.move_replay_scope = None;
    }
}

pub(crate) fn history_count(state: &GameState, field: &str) -> usize {
    let counter = match field {
        "replayEvents" => "replayExperimentEventCount",
        "boardHistory" => "replayExperimentBoardCount",
        _ => {
            return state
                .extra
                .get(field)
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
        }
    };
    if state.replay_mode.keeps_history() {
        state
            .extra
            .get(field)
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    } else {
        state
            .extra
            .get(counter)
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize
    }
}

pub(crate) fn increment_history_count(state: &mut GameState, field: &str) {
    let counter = if field == "replayEvents" {
        "replayExperimentEventCount"
    } else {
        "replayExperimentBoardCount"
    };
    let next = history_count(state, field).saturating_add(1);
    let next = if field == "boardHistory" {
        next.min(12)
    } else {
        next
    };
    state.extra.insert(counter.into(), json!(next));
}

/// Separate diagnostic probe. `record` is applied to an already settled
/// action state; `commit_move` compares the shared root with that action state.
/// These durations are not added to the throughput timing loops.
pub fn probe_record_and_delta(before: &GameState, after: &GameState) -> crate::Result<Value> {
    let mut delta_state = after.clone();
    let start = Instant::now();
    crate::replay::commit_move(&mut delta_state, before, before.decision_actor())?;
    let delta_ns = start.elapsed().as_nanos();
    let mut record_state = after.clone();
    let start = Instant::now();
    crate::replay::record(&mut record_state, "experiment probe")?;
    let record_ns = start.elapsed().as_nanos();
    Ok(json!({"deltaGenerationNs":delta_ns,"recordNs":record_ns,
        "definition":"separate post-action probe on owned clones; record may suppress an unchanged replay event"}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_removes_existing_historical_payload_before_cloning() {
        let mut state = crate::v7_new_game::new_game(
            crate::GameConfig {
                draft_delete: true,
                ..Default::default()
            },
            19,
        )
        .unwrap();
        let event_count = state.extra["replayEvents"].as_array().unwrap().len();
        prepare_state(&mut state, ReplayMode::NoHistoryReplay);
        assert_eq!(history_count(&state, "replayEvents"), event_count);
        assert_eq!(state.extra["replayEvents"], json!([]));
        assert!(state.extra["replayBaseFrame"].is_null());
        let copy = state.clone();
        assert_eq!(copy.replay_mode, ReplayMode::NoHistoryReplay);
        assert_eq!(copy.extra["replayEvents"], json!([]));
    }

    #[test]
    fn no_replay_does_not_create_move_capture_snapshot() {
        let mut state = crate::v7_new_game::new_game(
            crate::GameConfig {
                draft_delete: true,
                ..Default::default()
            },
            19,
        )
        .unwrap();
        prepare_state(&mut state, ReplayMode::NoReplay);
        let before = crate::replay::begin_move(&mut state, crate::Color::White).unwrap();
        assert!(before.is_none());
        assert!(state.active_move_replay_before.is_none());
    }

    #[test]
    fn host_transaction_keeps_session_mode() {
        let initial = crate::V7HostPosition::new_replay_experiment(
            crate::GameConfig {
                draft_delete: true,
                ..Default::default()
            },
            19,
        )
        .unwrap();
        let session = initial
            .with_replay_mode(ReplayMode::NoHistoryReplay)
            .unwrap();
        let (next, ()) = session
            .transact(session.position_id(), |_working| Ok(()))
            .unwrap();
        assert_eq!(next.state().replay_mode, ReplayMode::NoHistoryReplay);
        assert_eq!(next.state().extra["replayEvents"], json!([]));
    }
}
