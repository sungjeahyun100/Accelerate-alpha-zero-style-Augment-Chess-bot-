//! Explicit, thread-local timing for one legal-action diagnostic run.
//! No source action, private state, or RNG value is retained in the report.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::Serialize;

const KEYS: [&str; 51] = [
    "cursor_init",
    "movement_generation",
    "movement_targets",
    "card_generation",
    "card_effect_apply",
    "state_clone",
    "transition_apply",
    "canonicalization",
    "public_projection",
    "public_deduplication",
    "cursor_page",
    "total_legal",
    "move_execute",
    "move_prepare",
    "move_core",
    "move_capture",
    "move_capture_inner",
    "move_landing",
    "move_after_placement",
    "move_after_surviving",
    "move_finish_control",
    "move_finish_removed_mover",
    "move_before_middle_callbacks",
    "move_after_middle_callbacks",
    "replay_begin_move",
    "replay_state_clone",
    "replay_commit_active_move",
    "replay_commit_move",
    "replay_record",
    "replay_record_pipeline",
    "replay_frame",
    "replay_delta",
    "replay_board_delta",
    "replay_json_compare",
    "replay_record_update",
    "end_move_for_decision",
    "finish_move_with_history",
    "finish_move_with_count_inner",
    "settle_end_move_before_count",
    "settle_end_move_after_count",
    "end_move_piece_effects",
    "end_move_auto_effects",
    "no_action_loss",
    "threat_probe_royal_capture",
    "threat_square_attacked",
    "threat_herald",
    "threat_initiative",
    "game_over_democracy_check",
    "game_over_racing_kings_check",
    "game_over_gomoku_check",
    "game_over_apply",
];

#[derive(Clone, Default, Serialize)]
pub struct Timing {
    pub calls: u64,
    pub total_ms: f64,
    pub exclusive_ms: f64,
    pub mean_ms: Option<f64>,
    pub min_ms: Option<f64>,
    pub max_ms: Option<f64>,
}

impl Timing {
    fn add(&mut self, elapsed: Duration, exclusive: Duration) {
        let ms = elapsed.as_secs_f64() * 1000.0;
        self.calls += 1;
        self.total_ms += ms;
        self.exclusive_ms += exclusive.as_secs_f64() * 1000.0;
        self.mean_ms = Some(self.total_ms / self.calls as f64);
        self.min_ms = Some(self.min_ms.map_or(ms, |old| old.min(ms)));
        self.max_ms = Some(self.max_ms.map_or(ms, |old| old.max(ms)));
    }
}

#[derive(Clone, Serialize)]
pub struct SlowCandidate {
    pub ordinal: u64,
    pub elapsed_ms: f64,
}

#[derive(Default, Serialize)]
pub struct LegalProfile {
    pub timing_ms: BTreeMap<&'static str, Timing>,
    pub counts: BTreeMap<&'static str, u64>,
    pub slowest_candidates: Vec<SlowCandidate>,
    pub unclassified_ms: f64,
    /// transition_apply exclusive time: work outside its measured child spans.
    pub transition_unclassified_ms: f64,
    /// Nearest instrumented ancestor of each movement target calculation.
    pub movement_targets_callers: BTreeMap<&'static str, u64>,
    #[serde(skip)]
    started: Option<Instant>,
    #[serde(skip)]
    stack: Vec<Frame>,
    #[serde(skip)]
    root_measured: Duration,
}

struct Frame {
    key: &'static str,
    children: Duration,
}

thread_local! {
    static ACTIVE: RefCell<Option<LegalProfile>> = const { RefCell::new(None) };
}

pub fn start() -> Result<(), &'static str> {
    ACTIVE.with(|cell| {
        let mut active = cell.borrow_mut();
        if active.is_some() {
            return Err("legal profile already active on this thread");
        }
        let mut profile = LegalProfile::default();
        for key in KEYS {
            profile.timing_ms.insert(key, Timing::default());
        }
        for key in [
            "movement_candidates_generated",
            "card_candidates_generated",
            "candidates_examined",
            "candidates_accepted",
            "candidates_rejected",
            "state_clones",
            "transition_applies",
            "canonicalizations",
        ] {
            profile.counts.insert(key, 0);
        }
        profile.started = Some(Instant::now());
        *active = Some(profile);
        Ok(())
    })
}

pub fn finish() -> Result<LegalProfile, &'static str> {
    ACTIVE.with(|cell| {
        let mut profile = cell
            .borrow_mut()
            .take()
            .ok_or("legal profile is not active")?;
        if let Some(started) = profile.started.take() {
            let elapsed = started.elapsed();
            let exclusive = elapsed.saturating_sub(profile.root_measured);
            profile.unclassified_ms = exclusive.as_secs_f64() * 1000.0;
            profile
                .timing_ms
                .entry("total_legal")
                .or_default()
                .add(elapsed, exclusive);
        }
        profile.transition_unclassified_ms = profile
            .timing_ms
            .get("transition_apply")
            .map_or(0.0, |timing| timing.exclusive_ms);
        Ok(profile)
    })
}

pub(crate) fn measure<T>(key: &'static str, operation: impl FnOnce() -> T) -> T {
    let enabled = ACTIVE.with(|cell| {
        let active = cell.borrow();
        let Some(profile) = active.as_ref() else {
            return false;
        };
        // New breakdowns belong to transition_apply. Existing top-level
        // diagnostic keys retain their original whole-run scope.
        let existing = matches!(
            key,
            "cursor_init"
                | "movement_generation"
                | "movement_targets"
                | "card_generation"
                | "card_effect_apply"
                | "state_clone"
                | "transition_apply"
                | "canonicalization"
                | "public_projection"
                | "public_deduplication"
                | "cursor_page"
                | "total_legal"
        );
        existing
            || profile
                .stack
                .iter()
                .any(|frame| frame.key == "transition_apply")
    });
    if !enabled {
        return operation();
    }
    ACTIVE.with(|cell| {
        let mut active = cell.borrow_mut();
        let profile = active.as_mut().unwrap();
        if key == "movement_targets" {
            let parent = profile
                .stack
                .last()
                .map_or("unclassified", |frame| frame.key);
            *profile.movement_targets_callers.entry(parent).or_default() += 1;
        }
        profile.stack.push(Frame {
            key,
            children: Duration::ZERO,
        });
    });
    let start = Instant::now();
    let result = operation();
    let elapsed = start.elapsed();
    ACTIVE.with(|cell| {
        if let Some(profile) = cell.borrow_mut().as_mut() {
            let child = profile
                .stack
                .pop()
                .map_or(Duration::ZERO, |frame| frame.children);
            if let Some(parent) = profile.stack.last_mut() {
                parent.children += elapsed;
            } else {
                profile.root_measured += elapsed;
            }
            profile
                .timing_ms
                .entry(key)
                .or_default()
                .add(elapsed, elapsed.saturating_sub(child));
            let counter = match key {
                "state_clone" => Some("state_clones"),
                "transition_apply" => Some("transition_applies"),
                "canonicalization" => Some("canonicalizations"),
                _ => None,
            };
            if let Some(counter) = counter {
                *profile.counts.entry(counter).or_default() += 1;
            }
        }
    });
    result
}

pub(crate) fn count(key: &'static str, amount: u64) {
    ACTIVE.with(|cell| {
        if let Some(profile) = cell.borrow_mut().as_mut() {
            *profile.counts.entry(key).or_default() += amount;
        }
    });
}

pub(crate) fn candidate<T>(operation: impl FnOnce() -> T) -> T {
    let enabled = ACTIVE.with(|cell| cell.borrow().is_some());
    if !enabled {
        return operation();
    }
    let start = Instant::now();
    let result = operation();
    ACTIVE.with(|cell| {
        if let Some(profile) = cell.borrow_mut().as_mut() {
            let ordinal = profile.counts.entry("candidates_examined").or_default();
            *ordinal += 1;
            let candidate = SlowCandidate {
                ordinal: *ordinal,
                elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            };
            profile.slowest_candidates.push(candidate);
            profile
                .slowest_candidates
                .sort_by(|a, b| b.elapsed_ms.total_cmp(&a.elapsed_ms));
            profile.slowest_candidates.truncate(5);
        }
    });
    result
}
