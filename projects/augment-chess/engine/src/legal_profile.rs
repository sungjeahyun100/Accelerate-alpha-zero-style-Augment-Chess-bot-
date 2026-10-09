//! Explicit, thread-local timing for one legal-action diagnostic run.
//! No source action, private state, or RNG value is retained in the report.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::Serialize;

const KEYS: [&str; 12] = [
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
    #[serde(skip)]
    started: Option<Instant>,
    #[serde(skip)]
    stack: Vec<Duration>,
    #[serde(skip)]
    root_measured: Duration,
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
        Ok(profile)
    })
}

pub(crate) fn measure<T>(key: &'static str, operation: impl FnOnce() -> T) -> T {
    let enabled = ACTIVE.with(|cell| cell.borrow().is_some());
    if !enabled {
        return operation();
    }
    ACTIVE.with(|cell| {
        cell.borrow_mut()
            .as_mut()
            .unwrap()
            .stack
            .push(Duration::ZERO)
    });
    let start = Instant::now();
    let result = operation();
    let elapsed = start.elapsed();
    ACTIVE.with(|cell| {
        if let Some(profile) = cell.borrow_mut().as_mut() {
            let child = profile.stack.pop().unwrap_or_default();
            if let Some(parent) = profile.stack.last_mut() {
                *parent += elapsed;
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
