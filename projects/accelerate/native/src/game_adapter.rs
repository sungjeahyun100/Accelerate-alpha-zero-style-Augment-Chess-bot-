//! Python transport for the game-owned adapter facade. The registry and all
//! game state stay in augment-chess-engine; this module only converts bounded
//! values and forwards exact versioned requests.

use std::collections::BTreeSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use adapter_runtime::{
    AdapterError, AdapterErrorKind, AdapterRequest, Cancellation, InvocationControl,
};
use augment_chess_engine::adapter::{
    GameAdapterPayload, GameAdapterSession as EngineGameAdapterSession,
};
use augment_chess_engine::legal_profile;
use augment_chess_engine::v7_action_admission::AdmissionErrorKind;
use augment_chess_engine::v7_adapter_actions::{self, V7ActionHostError};
use augment_chess_engine::v7_conditioning;
use augment_chess_engine::{EngineError, GameConfig, V7HostPosition};
use pyo3::{
    exceptions::PyValueError,
    prelude::*,
    types::{PyAny, PyDict},
};
use serde::Serialize;
use serde_json::Value;

use crate::{
    NativeError, StaleActionError, UnsupportedFeatureError, conversion, proposal_probabilities,
    require_executable_rules_version, result,
};

fn adapter_error(error: AdapterError) -> PyErr {
    let detail = format!(
        "{}; adapterId={:?}; capabilityId={:?}",
        error, error.adapter_id, error.capability_id
    );
    match error.kind {
        AdapterErrorKind::Unsupported => UnsupportedFeatureError::new_err(detail),
        AdapterErrorKind::StaleRevision => StaleActionError::new_err(detail),
        _ => NativeError::new_err(detail),
    }
}

fn v7_action_error(error: V7ActionHostError) -> PyErr {
    match error {
        V7ActionHostError::Engine(engine_error) => crate::error(engine_error),
        V7ActionHostError::Admission(admission) => match admission.kind {
            AdmissionErrorKind::Unsupported => {
                UnsupportedFeatureError::new_err(admission.to_string())
            }
            AdmissionErrorKind::StalePosition => StaleActionError::new_err(admission.to_string()),
            _ => NativeError::new_err(admission.to_string()),
        },
    }
}

fn json_value(value: impl Serialize) -> PyResult<Value> {
    let value =
        serde_json::to_value(value).map_err(|error| NativeError::new_err(error.to_string()))?;
    conversion::validate(&value)?;
    Ok(value)
}

const MAX_FROZEN_SOURCE_PROBE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default)]
struct FrozenSourceProbeByteBudget {
    bytes: usize,
}

impl std::io::Write for FrozenSourceProbeByteBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self.bytes.saturating_add(bytes.len());
        if self.bytes > MAX_FROZEN_SOURCE_PROBE_BYTES {
            return Err(std::io::Error::other(
                "frozen source probe byte limit exceeded",
            ));
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// An actual-frame export failure may name private identities. Keep its class
/// and precise failed phase; expected comparison diagnostics remain unchanged.
fn frozen_source_probe_export_error(error: EngineError) -> PyErr {
    let detail = "frozen source probe actual envelope export failed".into();
    let safe = match error {
        EngineError::InvalidState(_) => EngineError::InvalidState(detail),
        EngineError::InvalidConfig(_) => EngineError::InvalidConfig(detail),
        EngineError::Serialization(_) => EngineError::Serialization(detail),
        EngineError::UnsupportedFeature(_) => EngineError::UnsupportedFeature(detail),
        EngineError::ConditioningMismatch(_) => EngineError::ConditioningMismatch(detail),
        other => other,
    };
    crate::error(safe)
}

fn verify_frozen_source_probe_position(
    position: &V7HostPosition,
    expected_envelope: &Value,
    expected_actions: Option<&[Value]>,
) -> PyResult<()> {
    let mut budget = FrozenSourceProbeByteBudget::default();
    serde_json::to_writer(&mut budget, expected_envelope).map_err(|_| {
        PyValueError::new_err("frozen source probe envelope/actions JSON exceeds 16 MiB")
    })?;
    if let Some(actions) = expected_actions {
        serde_json::to_writer(&mut budget, actions).map_err(|_| {
            PyValueError::new_err("frozen source probe envelope/actions JSON exceeds 16 MiB")
        })?;
    }
    let actual = position
        .export_envelope()
        .map_err(frozen_source_probe_export_error)?;
    let expected_bytes = crate::canonical_bytes(expected_envelope).map_err(|_| {
        NativeError::new_err("frozen source probe expected envelope cannot be canonicalized")
    })?;
    let actual_bytes = crate::canonical_bytes(&actual).map_err(|_| {
        NativeError::new_err("frozen source probe actual envelope cannot be canonicalized")
    })?;
    if expected_bytes != actual_bytes {
        let path = frozen_source_envelope_path(expected_envelope, &actual)?;
        return Err(NativeError::new_err(format!(
            "frozen source probe canonical envelope mismatch at {path}"
        )));
    }
    if let Some(actions) = expected_actions {
        v7_adapter_actions::verify_source_action_envelopes(position, actions)
            .map_err(v7_action_error)?;
    }
    Ok(())
}

fn frozen_source_envelope_path(expected: &Value, actual: &Value) -> PyResult<&'static str> {
    // These names are fixed protocol fields. Descending through arbitrary
    // source maps could reveal private identities in dynamic object keys.
    for (field, path) in [
        ("protocolVersion", "$.protocolVersion"),
        ("rulesVersion", "$.rulesVersion"),
        ("catalogVersion", "$.catalogVersion"),
        ("state", "$.state"),
        ("rng", "$.rng"),
        ("history", "$.history"),
        ("positionId", "$.positionId"),
    ] {
        let expected = expected.get(field);
        let actual = actual.get(field);
        if expected.is_some() != actual.is_some()
            || serde_jcs::to_vec(&expected).map_err(|_| {
                NativeError::new_err("frozen source probe expected field cannot be canonicalized")
            })? != serde_jcs::to_vec(&actual).map_err(|_| {
                NativeError::new_err("frozen source probe actual field cannot be canonicalized")
            })?
        {
            return Ok(path);
        }
    }
    Ok("$")
}

struct InvocationCancellation(Arc<AtomicBool>);

impl Cancellation for InvocationCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

struct ActiveInvocation {
    current: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    cancelled: Arc<AtomicBool>,
}

impl ActiveInvocation {
    fn begin(current: Arc<Mutex<Option<Arc<AtomicBool>>>>) -> Result<Self, AdapterError> {
        let cancelled = Arc::new(AtomicBool::new(false));
        *current.lock().map_err(|_| {
            AdapterError::execution_failed(
                "game_adapter_cancel_lock_poisoned",
                "game adapter cancellation lock poisoned",
            )
        })? = Some(cancelled.clone());
        Ok(Self { current, cancelled })
    }
}

impl Drop for ActiveInvocation {
    fn drop(&mut self) {
        if let Ok(mut current) = self.current.lock()
            && current
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, &self.cancelled))
        {
            *current = None;
        }
    }
}

/// The Python object owns a game adapter session but never exposes GameState.
/// Calls are serialized; cancellation is best effort at the runtime's metered
/// checkpoints and applies only to a call currently running on this object.
#[pyclass(
    name = "GameAdapterSession",
    module = "accelerate_chess._native",
    frozen
)]
pub struct GameAdapterSession {
    inner: Arc<Mutex<EngineGameAdapterSession>>,
    active: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}

impl GameAdapterSession {
    fn from_position(position: V7HostPosition) -> PyResult<Self> {
        let inner = EngineGameAdapterSession::new(position).map_err(adapter_error)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
            active: Arc::new(Mutex::new(None)),
        })
    }

    fn cloned_position(
        &self,
        py: Python<'_>,
        expected_revision: Option<&str>,
    ) -> PyResult<V7HostPosition> {
        let inner = self.inner.clone();
        let expected_revision = expected_revision.map(str::to_owned);
        py.detach(move || {
            let guard = inner
                .lock()
                .map_err(|_| NativeError::new_err("game adapter session lock poisoned"))?;
            if let Some(expected) = expected_revision
                && expected != guard.position().position_id()
            {
                return Err(StaleActionError::new_err(format!(
                    "game adapter source revision changed; expected={expected:?}; actual={:?}",
                    guard.position().position_id()
                )));
            }
            Ok(guard.position().clone())
        })
    }
}

#[pymethods]
impl GameAdapterSession {
    /// Explicit diagnostic transport, separate from public adapter responses.
    fn begin_legal_profile(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let inner = self.inner.clone();
        let stats = py.detach(move || {
            let guard = inner.lock().map_err(|_| NativeError::new_err("game adapter session lock poisoned"))?;
            let state = guard.position().state();
            let pieces: BTreeSet<&str> = state.board.iter().flatten().flatten()
                .map(|piece| piece.id.as_str()).collect();
            let serialization_started = Instant::now();
            let serialized_state_bytes = serde_json::to_vec(state)
                .map_err(|error| NativeError::new_err(error.to_string()))?.len();
            let serialization_ms = serialization_started.elapsed().as_secs_f64() * 1000.0;
            let stats = serde_json::json!({
                "board_piece_count": pieces.len(),
                "current_player_card_instances": state.deck_slots.get(state.decision_actor()).iter().filter(|card| !card.vacant).count(),
                "all_card_instances": state.deck_slots.white.iter().chain(&state.deck_slots.black).filter(|card| !card.vacant).count(),
                "history_len": state.history.len(),
                "capture_count": state.captures.white.len() + state.captures.black.len(),
                "serialized_state_bytes": serialized_state_bytes,
                "state_size_serialization_ms": serialization_ms,
            });
            Ok::<_, PyErr>(stats)
        })?;
        let stats = conversion::to_python(py, &stats)?;
        legal_profile::start().map_err(NativeError::new_err)?;
        Ok(stats)
    }

    fn finish_legal_profile(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let profile = legal_profile::finish().map_err(NativeError::new_err)?;
        conversion::to_python(py, &json_value(profile)?)
    }

    /// Independently reconstruct a v7 opening from a signed public frame.
    /// The source environment's hidden seed and position never enter Python.
    #[staticmethod]
    fn sample_initial_public(
        py: Python<'_>,
        config: &Bound<'_, PyAny>,
        public_observation: &Bound<'_, PyAny>,
        independent_seed: u32,
    ) -> PyResult<Self> {
        let config: GameConfig = serde_json::from_value(conversion::from_python(config)?)
            .map_err(|error| PyValueError::new_err(format!("invalid v7 game config: {error}")))?;
        let public = conversion::from_python(public_observation)?;
        let position = py
            .detach(move || {
                v7_conditioning::sample_initial_public(config, public, independent_seed)
            })
            .map_err(crate::error)?;
        Self::from_position(position)
    }

    #[staticmethod]
    #[pyo3(signature = (config=None, seed=0, rules_version=None))]
    fn new_game(
        py: Python<'_>,
        config: Option<&Bound<'_, PyAny>>,
        seed: u64,
        rules_version: Option<&str>,
    ) -> PyResult<Self> {
        require_executable_rules_version(rules_version)?;
        let config: GameConfig = serde_json::from_value(
            config
                .map(conversion::from_python)
                .transpose()?
                .unwrap_or_else(|| serde_json::json!({})),
        )
        .map_err(|error| PyValueError::new_err(format!("invalid v7 game config: {error}")))?;
        let inner = py
            .detach(move || EngineGameAdapterSession::new_game(config, seed))
            .map_err(adapter_error)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
            active: Arc::new(Mutex::new(None)),
        })
    }

    #[staticmethod]
    fn from_envelope(py: Python<'_>, envelope: &Bound<'_, PyAny>) -> PyResult<Self> {
        let envelope = conversion::from_python(envelope)?;
        let inner = py
            .detach(move || EngineGameAdapterSession::from_envelope(envelope))
            .map_err(adapter_error)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
            active: Arc::new(Mutex::new(None)),
        })
    }

    fn descriptors(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let inner = self.inner.clone();
        let descriptors = py.detach(move || {
            let guard = inner
                .lock()
                .map_err(|_| NativeError::new_err("game adapter session lock poisoned"))?;
            Ok::<_, PyErr>(guard.descriptors().into_iter().cloned().collect::<Vec<_>>())
        })?;
        conversion::to_python(py, &json_value(descriptors)?)
    }

    #[getter]
    fn snapshot_revision(&self, py: Python<'_>) -> PyResult<String> {
        let inner = self.inner.clone();
        py.detach(move || {
            let guard = inner
                .lock()
                .map_err(|_| NativeError::new_err("game adapter session lock poisoned"))?;
            Ok(guard.position().position_id().to_owned())
        })
    }

    #[getter]
    fn decision_actor(&self, py: Python<'_>) -> PyResult<&'static str> {
        let inner = self.inner.clone();
        py.detach(move || {
            let guard = inner
                .lock()
                .map_err(|_| NativeError::new_err("game adapter session lock poisoned"))?;
            Ok(guard.position().state().decision_actor().as_str())
        })
    }

    #[getter]
    fn result(&self, py: Python<'_>) -> PyResult<Option<&'static str>> {
        let inner = self.inner.clone();
        py.detach(move || {
            let guard = inner
                .lock()
                .map_err(|_| NativeError::new_err("game adapter session lock poisoned"))?;
            Ok(result(guard.position().state().result()))
        })
    }

    /// Branch the source-owned position without exposing its private envelope
    /// to Python. Search particles may advance independently from this point.
    #[pyo3(signature = (*, snapshot_revision=None))]
    fn fork(&self, py: Python<'_>, snapshot_revision: Option<&str>) -> PyResult<Self> {
        let position = self.cloned_position(py, snapshot_revision)?;
        let forked = py
            .detach(move || EngineGameAdapterSession::new(position))
            .map_err(adapter_error)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(forked)),
            active: Arc::new(Mutex::new(None)),
        })
    }

    /// Frozen-source differential receipt verification only. Python supplies
    /// expected private frames; no actual state, RNG or source action accessor
    /// is exposed. Omitting actions checks a sampled transition's full frame.
    #[pyo3(signature = (expected_envelope, expected_actions=None, *, snapshot_revision=None))]
    fn _verify_frozen_source_probe(
        &self,
        py: Python<'_>,
        expected_envelope: &Bound<'_, PyAny>,
        expected_actions: Option<&Bound<'_, PyAny>>,
        snapshot_revision: Option<&str>,
    ) -> PyResult<()> {
        let expected_envelope = conversion::from_python(expected_envelope)?;
        let expected_actions = expected_actions
            .map(conversion::from_python)
            .transpose()?
            .map(|value| match value {
                Value::Array(actions) => Ok(actions),
                _ => Err(PyValueError::new_err(
                    "frozen source probe expected_actions must be an array or None",
                )),
            })
            .transpose()?;
        let position = self
            .cloned_position(py, snapshot_revision)
            .map_err(|error| {
                if error.is_instance_of::<StaleActionError>(py) {
                    StaleActionError::new_err("frozen source probe snapshot revision is stale")
                } else {
                    error
                }
            })?;
        py.detach(move || {
            verify_frozen_source_probe_position(
                &position,
                &expected_envelope,
                expected_actions.as_deref(),
            )
        })
    }

    /// Return a source-valid hidden opening proposal with its p/q correction.
    /// Neither the source position nor an unvalidated probability is exposed.
    #[pyo3(signature = (expected_next_public, independent_seed, *, snapshot_revision=None))]
    fn condition_hidden_opening_draft<'py>(
        &self,
        py: Python<'py>,
        expected_next_public: &Bound<'_, PyAny>,
        independent_seed: u32,
        snapshot_revision: Option<&str>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let expected = conversion::from_python(expected_next_public)?;
        let position = self.cloned_position(py, snapshot_revision)?;
        let proposal = py
            .detach(move || {
                v7_conditioning::condition_hidden_opening_draft(
                    &position,
                    expected,
                    independent_seed,
                )
            })
            .map_err(crate::error)?;
        proposal_probabilities(
            proposal.importance_weight,
            proposal.source_probability,
            proposal.proposal_probability,
        )?;
        let result = PyDict::new(py);
        result.set_item(
            "position",
            Py::new(py, Self::from_position(proposal.position)?)?,
        )?;
        result.set_item("importance_weight", proposal.importance_weight)?;
        result.set_item("source_probability", proposal.source_probability)?;
        result.set_item("proposal_probability", proposal.proposal_probability)?;
        Ok(result)
    }

    #[pyo3(signature = (expected_next_public, independent_seed, *, snapshot_revision=None))]
    fn condition_hidden_stage_draft<'py>(
        &self,
        py: Python<'py>,
        expected_next_public: &Bound<'_, PyAny>,
        independent_seed: u32,
        snapshot_revision: Option<&str>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let expected = conversion::from_python(expected_next_public)?;
        let position = self.cloned_position(py, snapshot_revision)?;
        let proposal = py
            .detach(move || {
                v7_conditioning::condition_hidden_stage_draft(&position, expected, independent_seed)
            })
            .map_err(crate::error)?;
        proposal_probabilities(
            proposal.importance_weight,
            proposal.source_probability,
            proposal.proposal_probability,
        )?;
        let result = PyDict::new(py);
        result.set_item(
            "position",
            Py::new(py, Self::from_position(proposal.position)?)?,
        )?;
        result.set_item("importance_weight", proposal.importance_weight)?;
        result.set_item("source_probability", proposal.source_probability)?;
        result.set_item("proposal_probability", proposal.proposal_probability)?;
        Ok(result)
    }

    /// Count the source public-choice prior in native code and return only
    /// delta-related public candidates. The complete set never crosses into
    /// Python. No source payload or private identity is exported.
    #[pyo3(signature = (origin, destination, *, snapshot_revision=None))]
    fn public_delta_candidate_intents<'py>(
        &self,
        py: Python<'py>,
        origin: &Bound<'_, PyAny>,
        destination: &Bound<'_, PyAny>,
        snapshot_revision: Option<&str>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let origin = conversion::from_python(origin)?;
        let destination = conversion::from_python(destination)?;
        for square in [&origin, &destination] {
            let fields = square
                .as_object()
                .ok_or_else(|| PyValueError::new_err("public delta square must be a coordinate"))?;
            if fields.len() != 2
                || !fields.contains_key("row")
                || !fields.contains_key("col")
                || !matches!(fields.get("row").and_then(Value::as_u64), Some(0..=7))
                || !matches!(fields.get("col").and_then(Value::as_u64), Some(0..=7))
            {
                return Err(PyValueError::new_err("public delta square is outside 8x8"));
            }
        }
        let position = self.cloned_position(py, snapshot_revision)?;
        let (intents, legal_count, examined) = py.detach(move || {
            v7_adapter_actions::public_delta_candidate_intents(&position, &origin, &destination)
                .map_err(v7_action_error)
        })?;
        let result = PyDict::new(py);
        result.set_item("intents", conversion::to_python(py, &json_value(intents)?)?)?;
        result.set_item("legal_count", legal_count)?;
        result.set_item("examined", examined)?;
        Ok(result)
    }

    /// 공개 전이의 source 호환성을 확인한다. Python action ID나 환경의 숨은
    /// Position/RNG를 입력으로 받지 않으며 이 session의 상태를 변경하지 않는다.
    #[pyo3(signature = (public_intent, expected_next_public, *, snapshot_revision=None))]
    fn public_transition_compatible(
        &self,
        py: Python<'_>,
        public_intent: &Bound<'_, PyAny>,
        expected_next_public: &Bound<'_, PyAny>,
        snapshot_revision: Option<&str>,
    ) -> PyResult<bool> {
        let intent = conversion::from_python(public_intent)?;
        let expected = conversion::from_python(expected_next_public)?;
        let position = self.cloned_position(py, snapshot_revision)?;
        py.detach(move || {
            let admitted = v7_adapter_actions::bind_public_intent(&position, intent)
                .map_err(v7_action_error)?;
            v7_conditioning::public_transition_compatible(&position, &admitted, expected)
                .map_err(v7_action_error)
        })
    }

    /// 독립 입자에서 공개 intent를 한 번 실행하고 source/proposal density를 반환한다.
    /// draft는 기존 observed-offer/future-stream 조건부 제안을 유지한다. play는
    /// 이번 전이 전에 독립 seed의 source RNG를 설치하고 소비된 stream을 보존하는
    /// source-prior 시도다. 관측 불일치만 거절하며 미분류 draw나 실행 오류는 전파한다.
    #[pyo3(signature = (public_intent, expected_next_public, independent_seed, *, snapshot_revision=None))]
    fn apply_weighted_conditioned_public<'py>(
        &self,
        py: Python<'py>,
        public_intent: &Bound<'_, PyAny>,
        expected_next_public: &Bound<'_, PyAny>,
        independent_seed: u32,
        snapshot_revision: Option<&str>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let intent = conversion::from_python(public_intent)?;
        let expected = conversion::from_python(expected_next_public)?;
        let position = self.cloned_position(py, snapshot_revision)?;
        let proposal = py.detach(move || {
            let admitted = v7_adapter_actions::bind_public_intent(&position, intent)
                .map_err(v7_action_error)?;
            v7_conditioning::apply_weighted_conditioned_public(
                &position,
                &admitted,
                expected,
                independent_seed,
            )
            .map_err(v7_action_error)
        })?;
        proposal_probabilities(
            proposal.importance_weight,
            proposal.source_probability,
            proposal.proposal_probability,
        )?;
        let result = PyDict::new(py);
        result.set_item(
            "position",
            Py::new(py, Self::from_position(proposal.applied.position)?)?,
        )?;
        result.set_item("importance_weight", proposal.importance_weight)?;
        result.set_item("source_probability", proposal.source_probability)?;
        result.set_item("proposal_probability", proposal.proposal_probability)?;
        Ok(result)
    }

    #[pyo3(signature = (request, deadline_ms=None))]
    fn invoke(
        &self,
        py: Python<'_>,
        request: &Bound<'_, PyAny>,
        deadline_ms: Option<u64>,
    ) -> PyResult<Py<PyAny>> {
        let payload = conversion::from_python(request)?;
        let request: AdapterRequest<GameAdapterPayload> = serde_json::from_value(payload)
            .map_err(|error| PyValueError::new_err(format!("invalid adapter request: {error}")))?;
        let deadline = match deadline_ms {
            None => None,
            Some(milliseconds) if (1..=86_400_000).contains(&milliseconds) => Instant::now()
                .checked_add(Duration::from_millis(milliseconds))
                .ok_or_else(|| PyValueError::new_err("adapter deadline overflow"))
                .map(Some)?,
            Some(_) => {
                return Err(PyValueError::new_err(
                    "adapter deadline_ms must be within 1..=86400000",
                ));
            }
        };
        let inner = self.inner.clone();
        let active = self.active.clone();
        let response = py
            .detach(move || {
                let mut guard = inner.lock().map_err(|_| {
                    AdapterError::execution_failed(
                        "game_adapter_lock_poisoned",
                        "game adapter session lock poisoned",
                    )
                })?;
                let invocation = ActiveInvocation::begin(active)?;
                let cancellation = InvocationCancellation(invocation.cancelled.clone());
                let control = InvocationControl {
                    cancellation: &cancellation,
                    deadline,
                };
                guard.invoke(&request, &control)
            })
            .map_err(adapter_error)?;
        conversion::to_python(py, &json_value(response)?)
    }

    /// Ask the currently running call to stop at its next metered checkpoint.
    /// Returns false when this session has no active call.
    fn cancel_current(&self) -> PyResult<bool> {
        let current = self
            .active
            .lock()
            .map_err(|_| NativeError::new_err("game adapter cancellation lock poisoned"))?
            .clone();
        if let Some(cancelled) = current {
            cancelled.store(true, Ordering::Release);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RULES_VERSION_V7;
    use augment_chess_engine::{RngState, V7HostPosition};
    use serde_json::json;
    use sha2::{Digest, Sha256};

    #[test]
    fn frozen_source_probe_is_read_only_private_and_revision_bound() {
        Python::initialize();
        Python::attach(|py| {
            let engine = EngineGameAdapterSession::new_game(GameConfig::default(), 19).unwrap();
            let position = engine.position().clone();
            let before = position.export_envelope().unwrap();
            // Initial draft choices have identical source/public payloads.
            // The existing engine tests pin their full ordered source digest;
            // this receipt exercises conversion and the diagnostic FFI hook.
            let actions: Vec<Value> = v7_adapter_actions::legal_public_intents(&position)
                .unwrap()
                .into_iter()
                .map(|payload| {
                    let action_id =
                        format!("{:x}", Sha256::digest(serde_jcs::to_vec(&payload).unwrap()));
                    json!({
                        "protocolVersion":"accelerate-action-v1",
                        "positionId":position.position_id(),
                        "actionId":action_id,
                        "payload":payload,
                    })
                })
                .collect();
            let session = GameAdapterSession::from_position(position).unwrap();
            let revision = session.snapshot_revision(py).unwrap();
            let expected = conversion::to_python(py, &before).unwrap();
            let actions = conversion::to_python(py, &Value::Array(actions)).unwrap();
            session
                ._verify_frozen_source_probe(
                    py,
                    expected.bind(py),
                    Some(actions.bind(py)),
                    Some(&revision),
                )
                .unwrap();
            session
                ._verify_frozen_source_probe(py, expected.bind(py), None, None)
                .unwrap();
            let mut numeric = before.clone();
            let cursor = numeric["rng"]["cursor"].as_f64().unwrap();
            numeric["rng"]["cursor"] = json!(cursor);
            let numeric = conversion::to_python(py, &numeric).unwrap();
            session
                ._verify_frozen_source_probe(py, numeric.bind(py), None, Some(&revision))
                .unwrap();

            let mut corrupt = before.clone();
            corrupt["state"]["privateProbeMarker"] = json!("private-value-must-not-be-returned");
            let corrupt = conversion::to_python(py, &corrupt).unwrap();
            let error = session
                ._verify_frozen_source_probe(py, corrupt.bind(py), None, Some(&revision))
                .unwrap_err();
            assert!(error.is_instance_of::<NativeError>(py));
            assert!(
                error
                    .to_string()
                    .contains("canonical envelope mismatch at $.state")
            );
            assert!(
                !error
                    .to_string()
                    .contains("private-value-must-not-be-returned")
            );

            let omitted = conversion::to_python(py, &json!([])).unwrap();
            let error = session
                ._verify_frozen_source_probe(
                    py,
                    expected.bind(py),
                    Some(omitted.bind(py)),
                    Some(&revision),
                )
                .unwrap_err();
            assert!(error.is_instance_of::<NativeError>(py));
            assert!(error.to_string().contains("actions count differs"));
            let error = session
                ._verify_frozen_source_probe(
                    py,
                    expected.bind(py),
                    Some(actions.bind(py)),
                    Some("private-stale-revision"),
                )
                .unwrap_err();
            assert!(error.is_instance_of::<StaleActionError>(py));
            assert!(!error.to_string().contains("private-stale-revision"));
            assert!(!error.to_string().contains(&revision));
            assert_eq!(session.snapshot_revision(py).unwrap(), revision);
            assert_eq!(
                session
                    .cloned_position(py, None)
                    .unwrap()
                    .export_envelope()
                    .unwrap(),
                before,
            );
            session
                ._verify_frozen_source_probe(
                    py,
                    expected.bind(py),
                    Some(actions.bind(py)),
                    Some(&revision),
                )
                .unwrap();
            assert!(!session.cancel_current().unwrap());
        });
    }

    #[test]
    fn frozen_source_probe_combined_json_budget_counts_escaped_envelope_and_actions() {
        Python::initialize();
        Python::attach(|py| {
            let engine = EngineGameAdapterSession::new_game(GameConfig::default(), 19).unwrap();
            let position = engine.position();
            // Each raw string fits the existing 8 MiB conversion budget, but
            // escaping their combined JSON exceeds this probe's 16 MiB cap.
            let expected = json!({"padding":"\n".repeat(MAX_FROZEN_SOURCE_PROBE_BYTES / 4)});
            let actions = vec![json!({"padding":"\n".repeat(MAX_FROZEN_SOURCE_PROBE_BYTES / 4)})];
            let error = verify_frozen_source_probe_position(position, &expected, Some(&actions))
                .unwrap_err();
            assert!(error.is_instance_of::<PyValueError>(py));
            assert!(
                error
                    .to_string()
                    .contains("envelope/actions JSON exceeds 16 MiB")
            );
        });
    }

    #[test]
    fn python_boundary_uses_exact_descriptor_and_preserves_revision_on_rejection() {
        Python::initialize();
        Python::attach(|py| {
            let position = V7HostPosition::from_parts(
                json!({"board": vec![vec![Value::Null; 8]; 8], "turn": "white", "mode": "draft"}),
                RngState::seeded(19),
                Vec::new(),
            )
            .unwrap();
            let envelope = position.export_envelope().unwrap();
            let envelope = conversion::to_python(py, &envelope).unwrap();
            let session = GameAdapterSession::from_envelope(py, envelope.bind(py)).unwrap();
            let descriptors =
                conversion::from_python(session.descriptors(py).unwrap().bind(py)).unwrap();
            let descriptor = descriptors
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["adapterId"] == "public-observation")
                .unwrap();
            let capability = descriptor["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["id"] == "observe")
                .unwrap();
            let revision = session.snapshot_revision(py).unwrap();
            let fork = session.fork(py, Some(&revision)).unwrap();
            assert_eq!(fork.snapshot_revision(py).unwrap(), revision);
            let stale = session
                .fork(py, Some("outdated-source-revision"))
                .err()
                .expect("a stale source revision must not fork the current state");
            assert!(stale.is_instance_of::<StaleActionError>(py));
            assert!(stale.to_string().contains("outdated-source-revision"));
            assert!(stale.to_string().contains(&revision));
            assert_eq!(session.decision_actor(py).unwrap(), "white");
            assert_eq!(session.result(py).unwrap(), None);
            assert!(!fork.cancel_current().unwrap());
            assert!(!session.cancel_current().unwrap());

            let mut request = json!({
                "requestId": "observe-1",
                "projectId": descriptor["projectId"],
                "adapterId": descriptor["adapterId"],
                "contractVersion": descriptor["contractVersion"],
                "implementationVersion": descriptor["implementationVersion"],
                "capabilityId": capability["id"],
                "requestSchema": capability["requestSchema"],
                "responseSchema": capability["responseSchema"],
                "snapshotRevision": revision,
                "limits": {"maxWork": 1000, "maxResults": 1},
                "payload": {"kind": "observe", "viewer": "white"}
            });
            let wire = conversion::to_python(py, &request).unwrap();
            let response = session.invoke(py, wire.bind(py), Some(1000)).unwrap();
            let response = conversion::from_python(response.bind(py)).unwrap();
            assert_eq!(response["requestId"], "observe-1");
            assert_eq!(response["result"]["kind"], "observation");
            assert_eq!(response["snapshotRevision"], revision);

            request["responseSchema"]["sha256"] = json!("0".repeat(64));
            let wire = conversion::to_python(py, &request).unwrap();
            let mismatch = session.invoke(py, wire.bind(py), Some(1000)).unwrap_err();
            assert!(mismatch.is_instance_of::<UnsupportedFeatureError>(py));
            assert!(mismatch.to_string().contains("schema_mismatch"));
            assert_eq!(session.snapshot_revision(py).unwrap(), revision);

            let invalid_deadline = session.invoke(py, wire.bind(py), Some(0)).unwrap_err();
            assert!(invalid_deadline.is_instance_of::<PyValueError>(py));
        });
    }

    #[test]
    fn versioned_new_game_enters_only_verified_v7_host_profile() {
        Python::initialize();
        Python::attach(|py| {
            let missing = GameAdapterSession::new_game(py, None, 19, None)
                .err()
                .expect("missing rules version must reject");
            assert!(missing.is_instance_of::<UnsupportedFeatureError>(py));
            let retired = GameAdapterSession::new_game(py, None, 19, Some(crate::RULES_VERSION_V6))
                .err()
                .expect("retired v6 rules must reject");
            assert!(retired.is_instance_of::<UnsupportedFeatureError>(py));
            let session =
                GameAdapterSession::new_game(py, None, 19, Some(RULES_VERSION_V7)).unwrap();
            assert!(!session.snapshot_revision(py).unwrap().is_empty());
            let descriptors =
                conversion::from_python(session.descriptors(py).unwrap().bind(py)).unwrap();
            assert!(!descriptors.as_array().unwrap().is_empty());
        });
    }

    #[test]
    fn python_action_transport_returns_only_public_intents_and_commit_metadata() {
        Python::initialize();
        Python::attach(|py| {
            let session =
                GameAdapterSession::new_game(py, None, 19, Some(RULES_VERSION_V7)).unwrap();
            let descriptors =
                conversion::from_python(session.descriptors(py).unwrap().bind(py)).unwrap();
            let descriptor = descriptors
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["adapterId"] == "public-actions")
                .unwrap();
            let capability = |id: &str| {
                descriptor["capabilities"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|value| value["id"] == id)
                    .unwrap()
            };
            let initial_revision = session.snapshot_revision(py).unwrap();
            let make_request = |id: &str, cap: &Value, payload: Value, revision: &str| {
                json!({
                    "requestId": id,
                    "projectId": descriptor["projectId"],
                    "adapterId": descriptor["adapterId"],
                    "contractVersion": descriptor["contractVersion"],
                    "implementationVersion": descriptor["implementationVersion"],
                    "capabilityId": cap["id"],
                    "requestSchema": cap["requestSchema"],
                    "responseSchema": cap["responseSchema"],
                    "snapshotRevision": revision,
                    "limits": descriptor["callLimits"],
                    "payload": payload,
                })
            };
            let legal_request = make_request(
                "legal-1",
                capability("legal-actions"),
                json!({"kind": "legal_actions"}),
                &initial_revision,
            );
            let legal_wire = conversion::to_python(py, &legal_request).unwrap();
            let legal = conversion::from_python(
                session
                    .invoke(py, legal_wire.bind(py), None)
                    .unwrap()
                    .bind(py),
            )
            .unwrap();
            assert_eq!(legal["result"]["kind"], "legal_actions");
            let first_intent = legal["result"]["intents"][0].clone();
            assert!(first_intent.is_object());
            assert!(first_intent.get("positionId").is_none());
            assert!(first_intent.get("actionId").is_none());
            let apply_request = make_request(
                "apply-1",
                capability("apply-public-intent"),
                json!({"kind": "apply_public_intent", "intent": first_intent}),
                &initial_revision,
            );
            let apply_wire = conversion::to_python(py, &apply_request).unwrap();
            let applied = conversion::from_python(
                session
                    .invoke(py, apply_wire.bind(py), None)
                    .unwrap()
                    .bind(py),
            )
            .unwrap();
            let result = applied["result"].as_object().unwrap();
            assert_eq!(result.len(), 4);
            assert!(result.contains_key("kind"));
            assert!(result.contains_key("actor"));
            assert!(result.contains_key("turn_changed"));
            assert!(result.contains_key("result"));
            assert!(!result.contains_key("event"));
            assert_ne!(session.snapshot_revision(py).unwrap(), initial_revision);
        });
    }
}
