//! 소유한 source prior 경로의 난수 분류. wire 상태·원본 RNG와 별도로 추적한다.
//!
//! 의미적 분기, 불투명 identity, 결과 불변 소비를 명시적으로 구분한다.
//! 설명되지 않은 draw가 있으면 밀도를 추측하지 않고 정확한 오류를 반환한다.

use crate::{EngineError, Result, RngState};
use std::panic::Location;

const MAX_SOURCE_TRACE_DRAWS: usize = 16_384;

#[derive(Clone, Debug, PartialEq)]
enum DrawKind {
    Unclassified,
    Opaque(&'static str),
    Invariant(&'static str),
    Semantic { mass: f64, context: &'static str },
    GroupMember(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
struct Draw {
    cursor: usize,
    origin: &'static Location<'static>,
    kind: DrawKind,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SourceChanceTrace {
    draws: Vec<Draw>,
}

fn origin(draw: &Draw) -> String {
    // compiler 경로의 사용자 로컬 prefix를 진단에 복사하지 않는다.
    let name = std::path::Path::new(draw.origin.file())
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("source");
    format!(
        "{name}:{} at RNG cursor {}",
        draw.origin.line(),
        draw.cursor
    )
}

fn checked_mass(mass: f64, context: &str) -> Result<()> {
    if !mass.is_finite() || mass <= 0.0 || mass > 1.0 {
        return Err(EngineError::InvalidState(format!(
            "source prior branch mass for {context} must be finite in (0, 1], got {mass}"
        )));
    }
    Ok(())
}

impl SourceChanceTrace {
    pub(crate) fn record(
        &mut self,
        cursor: usize,
        location: &'static Location<'static>,
    ) -> Result<()> {
        if self.draws.len() >= MAX_SOURCE_TRACE_DRAWS {
            return Err(EngineError::UnsupportedFeature(format!(
                "source prior trace exceeds {MAX_SOURCE_TRACE_DRAWS} draws at RNG cursor {cursor}"
            )));
        }
        self.draws.push(Draw {
            cursor,
            origin: location,
            kind: DrawKind::Unclassified,
        });
        Ok(())
    }

    fn classify_last(&mut self, cursor: usize, kind: DrawKind) -> Result<()> {
        let draw = self.draws.last_mut().ok_or_else(|| {
            EngineError::InvalidState("source prior classification has no preceding draw".into())
        })?;
        if draw.cursor.checked_add(1) != Some(cursor) || draw.kind != DrawKind::Unclassified {
            return Err(EngineError::InvalidState(format!(
                "source prior draw classified twice or after another draw at {}",
                origin(draw)
            )));
        }
        draw.kind = kind;
        Ok(())
    }

    fn classify_group(
        &mut self,
        start: usize,
        end: usize,
        mass: f64,
        context: &'static str,
    ) -> Result<()> {
        checked_mass(mass, context)?;
        if start == end && mass == 1.0 {
            return Ok(());
        }
        let indices = self
            .draws
            .iter()
            .enumerate()
            .filter(|(_, draw)| (start..end).contains(&draw.cursor))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if end < start
            || indices.len() != end.saturating_sub(start)
            || indices.is_empty()
            || indices
                .iter()
                .any(|&index| self.draws[index].kind != DrawKind::Unclassified)
        {
            return Err(EngineError::InvalidState(format!(
                "source prior group {context} has missing or already classified draws ({start}..{end})"
            )));
        }
        for (offset, &index) in indices.iter().enumerate() {
            self.draws[index].kind = if offset == 0 {
                DrawKind::Semantic { mass, context }
            } else {
                DrawKind::GroupMember(context)
            };
        }
        Ok(())
    }

    fn probability(self) -> Result<f64> {
        let mut probability = 1.0;
        for draw in &self.draws {
            match draw.kind {
                DrawKind::Unclassified => {
                    return Err(EngineError::UnsupportedFeature(format!(
                        "unclassified source prior draw at {}",
                        origin(draw)
                    )));
                }
                DrawKind::Semantic { mass, context } => {
                    checked_mass(mass, context)?;
                    probability *= mass;
                    if probability <= 0.0 || !probability.is_finite() {
                        return Err(EngineError::UnsupportedFeature(format!(
                            "source prior path density cannot be represented as a positive f64 for {context} at {}",
                            origin(draw)
                        )));
                    }
                }
                DrawKind::Opaque(_) | DrawKind::Invariant(_) | DrawKind::GroupMember(_) => {}
            }
        }
        Ok(probability)
    }
}

impl RngState {
    pub(crate) fn begin_source_trace(&mut self) -> Result<()> {
        if self.source_chance_trace.is_some() {
            return Err(EngineError::InvalidState(
                "source prior trace is already active".into(),
            ));
        }
        self.source_chance_trace = Some(Box::default());
        Ok(())
    }

    pub(crate) fn has_source_trace(&self) -> bool {
        self.source_chance_trace.is_some()
    }

    pub(crate) fn finish_source_trace(&mut self) -> Result<f64> {
        self.source_chance_trace
            .take()
            .ok_or_else(|| {
                EngineError::InvalidState(
                    "owned source prior trace was lost during transition".into(),
                )
            })?
            .probability()
    }

    #[track_caller]
    pub(crate) fn sample_opaque(&mut self, context: &'static str) -> Result<f64> {
        let value = self.sample()?;
        if let Some(trace) = self.source_chance_trace.as_mut() {
            trace.classify_last(self.cursor, DrawKind::Opaque(context))?;
        }
        Ok(value)
    }

    #[track_caller]
    pub(crate) fn sample_invariant(&mut self, context: &'static str) -> Result<f64> {
        let value = self.sample()?;
        if let Some(trace) = self.source_chance_trace.as_mut() {
            trace.classify_last(self.cursor, DrawKind::Invariant(context))?;
        }
        Ok(value)
    }

    /// 기존 GameState semantic probability는 유지하고 실제 직전 draw만 인증한다.
    pub(crate) fn record_last_probability(
        &mut self,
        mass: f64,
        context: &'static str,
    ) -> Result<()> {
        if let Some(trace) = self.source_chance_trace.as_mut() {
            checked_mass(mass, context)?;
            trace.classify_last(self.cursor, DrawKind::Semantic { mass, context })?;
        }
        Ok(())
    }

    pub(crate) fn record_group_probability(
        &mut self,
        start_cursor: usize,
        mass: f64,
        context: &'static str,
    ) -> Result<()> {
        if let Some(trace) = self.source_chance_trace.as_mut() {
            trace.classify_group(start_cursor, self.cursor, mass, context)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_preserves_rng_bytes_and_accounts_for_each_realized_draw() {
        let mut ordinary = RngState::seeded(37);
        let mut traced = ordinary.clone();
        traced.begin_source_trace().unwrap();
        assert_eq!(
            traced.sample_opaque("piece identity").unwrap(),
            ordinary.sample().unwrap()
        );
        assert_eq!(
            traced.sample_invariant("presentation").unwrap(),
            ordinary.sample().unwrap()
        );
        assert_eq!(traced.sample().unwrap(), ordinary.sample().unwrap());
        traced
            .record_last_probability(0.25, "uniform choice")
            .unwrap();
        let start = traced.cursor;
        for _ in 0..3 {
            assert_eq!(traced.sample().unwrap(), ordinary.sample().unwrap());
        }
        traced
            .record_group_probability(start, 0.5, "ranked candidate winner")
            .unwrap();
        assert_eq!(
            serde_json::to_value(&traced).unwrap(),
            serde_json::to_value(&ordinary).unwrap()
        );
        assert_eq!(traced.finish_source_trace().unwrap(), 0.125);
        assert_eq!(traced, ordinary);
    }

    #[test]
    fn unknown_draw_fails_with_exact_origin_and_does_not_publish_a_density() {
        let mut rng = RngState::seeded(37);
        rng.begin_source_trace().unwrap();
        rng.sample().unwrap();
        let error = rng.finish_source_trace().unwrap_err().to_string();
        assert!(error.contains("unclassified source prior draw"));
        assert!(error.contains("source_chance_trace.rs:"));
        assert!(error.contains("RNG cursor 0"));
        assert!(!rng.has_source_trace());
    }

    #[test]
    fn owned_clone_traces_are_independent_and_never_enter_snapshot_wire() {
        let mut left = RngState::seeded(37);
        left.begin_source_trace().unwrap();
        let mut right = left.clone();
        left.sample().unwrap();
        left.record_last_probability(0.3, "left branch").unwrap();
        right.sample().unwrap();
        right.record_last_probability(0.7, "right branch").unwrap();
        let wire = serde_json::to_value(&left).unwrap();
        assert_eq!(wire.as_object().unwrap().len(), 4);
        let restored: RngState = serde_json::from_value(wire).unwrap();
        assert!(!restored.has_source_trace());
        assert_eq!(left.finish_source_trace().unwrap(), 0.3);
        assert_eq!(right.finish_source_trace().unwrap(), 0.7);
    }

    #[test]
    fn duplicate_and_overlapping_classification_are_errors() {
        let mut rng = RngState::seeded(37);
        rng.begin_source_trace().unwrap();
        assert!(rng.begin_source_trace().is_err());
        rng.sample().unwrap();
        rng.record_last_probability(0.5, "branch").unwrap();
        assert!(rng.record_last_probability(0.5, "duplicate").is_err());
        assert!(rng.record_group_probability(0, 0.5, "overlap").is_err());
    }

    #[test]
    fn density_underflow_is_reported_without_a_unit_weight_fallback() {
        let mut rng = RngState::seeded(37);
        rng.begin_source_trace().unwrap();
        for _ in 0..2 {
            rng.sample().unwrap();
            rng.record_last_probability(1e-200, "small branch").unwrap();
        }
        assert!(
            rng.finish_source_trace()
                .unwrap_err()
                .to_string()
                .contains("positive f64")
        );
    }

    #[test]
    fn typed_host_constructors_reject_a_trace_before_wire_serialization_can_drop_it() {
        let mut state = crate::v7_new_game::new_game(crate::GameConfig::default(), 37).unwrap();
        state.rng.begin_source_trace().unwrap();
        assert!(
            crate::V7HostPosition::from_state(state.clone())
                .unwrap_err()
                .to_string()
                .contains("unsettled source prior trace")
        );
        let raw = serde_json::to_value(&state).unwrap();
        assert!(
            crate::V7HostPosition::from_parts(raw, state.rng, state.history)
                .unwrap_err()
                .to_string()
                .contains("unsettled source prior trace")
        );
    }
}
