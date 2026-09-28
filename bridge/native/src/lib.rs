//! Owned immutable rules boundary. Rules and hidden-state projection stay in
//! accelerate-engine; this crate handles transport, ownership and validation.
mod conversion;
mod inference;
use accelerate_engine::{
    Action as EngineAction, ActionStream as EngineActionStream, Color, EngineError, GameConfig,
    GameResult, Position as EnginePosition, RngState, StepResult as EngineStepResult,
};
use numpy::PyArray2;
use pyo3::{
    create_exception,
    exceptions::PyValueError,
    prelude::*,
    types::{PyDict, PyModule, PyTuple},
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex, OnceLock};

create_exception!(_native, NativeError, PyValueError);
create_exception!(_native, StaleActionError, NativeError);
create_exception!(_native, UnsupportedFeatureError, NativeError);
create_exception!(_native, ConditioningMismatchError, NativeError);
const POSITION_VERSION: &str = "accelerate-position-v1";
const ACTION_VERSION: &str = "accelerate-action-v1";

fn catalog() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../../catalog/site-20260927.json"))
            .expect("checked source catalog")
    })
}
fn observation_policy() -> &'static Value {
    static POLICY: OnceLock<Value> = OnceLock::new();
    POLICY.get_or_init(|| {
        serde_json::from_str(include_str!("../../catalog/observation-20260927.json"))
            .expect("checked source observation policy")
    })
}
fn error(error: EngineError) -> PyErr {
    match error {
        EngineError::StaleAction => StaleActionError::new_err(error.to_string()),
        EngineError::UnsupportedFeature(_) => UnsupportedFeatureError::new_err(error.to_string()),
        EngineError::ConditioningMismatch(_) => {
            ConditioningMismatchError::new_err(error.to_string())
        }
        _ => NativeError::new_err(error.to_string()),
    }
}
fn proposal_probabilities(weight: f64, source: f64, proposal: f64) -> PyResult<()> {
    let probability = |value: f64| value.is_finite() && value > 0. && value <= 1.;
    if !(weight.is_finite() && weight > 0. && probability(source) && probability(proposal)) {
        return Err(NativeError::new_err(
            "invalid source proposal probabilities",
        ));
    }
    let correction = source / proposal;
    if !correction.is_finite() || (weight - correction).abs() > 1e-10 * weight.max(correction) {
        return Err(NativeError::new_err(
            "source proposal weight does not equal the density correction",
        ));
    }
    Ok(())
}
fn value<T: Serialize>(value: &T) -> PyResult<Value> {
    serde_json::to_value(value).map_err(|e| NativeError::new_err(e.to_string()))
}
fn canonical_bytes(value: &Value) -> PyResult<Vec<u8>> {
    conversion::validate(value)?;
    serde_jcs::to_vec(value).map_err(|e| NativeError::new_err(e.to_string()))
}
fn digest(value: &Value) -> PyResult<String> {
    Ok(format!("{:x}", Sha256::digest(canonical_bytes(value)?)))
}
fn color(color: &str) -> PyResult<Color> {
    match color {
        "white" => Ok(Color::White),
        "black" => Ok(Color::Black),
        _ => Err(PyValueError::new_err("viewer must be white or black")),
    }
}
fn result(result: Option<GameResult>) -> Option<&'static str> {
    result.map(|v| match v {
        GameResult::White => "white",
        GameResult::Black => "black",
        GameResult::Draw => "draw",
    })
}

#[pyclass(
    name = "Action",
    module = "accelerate_chess._native",
    frozen,
    skip_from_py_object
)]
#[derive(Clone)]
pub struct Action {
    inner: EngineAction,
    position: EnginePosition,
    payload: Arc<Value>,
    position_id: String,
    action_id: String,
}
impl Action {
    fn wrap(
        mut inner: EngineAction,
        position_id: &str,
        position: EnginePosition,
    ) -> PyResult<Self> {
        inner.position_key = None;
        let payload = value(&inner)?;
        let action_id = digest(&payload)?;
        Ok(Self {
            inner,
            position,
            payload: Arc::new(payload),
            position_id: position_id.into(),
            action_id,
        })
    }
    fn envelope(&self) -> Value {
        json!({"protocolVersion": ACTION_VERSION, "positionId": self.position_id, "actionId": self.action_id, "payload": &*self.payload})
    }
}

/// The stream owns an immutable position inside the engine. Its cursor is the
/// only mutable value; a mutex serializes shared callers while detached from
/// Python. Pages own their actions and remain valid after this object is gone.
#[pyclass(name = "ActionStream", module = "accelerate_chess._native", frozen)]
pub struct ActionStream {
    inner: Arc<Mutex<EngineActionStream>>,
    position: EnginePosition,
    position_id: String,
}
#[pymethods]
impl ActionStream {
    #[pyo3(signature = (limit=256))]
    fn next_page<'py>(&self, py: Python<'py>, limit: usize) -> PyResult<Bound<'py, PyDict>> {
        let stream = self.inner.clone();
        let page = py.detach(move || {
            let mut stream = stream
                .lock()
                .map_err(|_| NativeError::new_err("action stream lock poisoned"))?;
            stream.next_page(limit).map_err(error)
        })?;
        let actions = page
            .actions
            .into_iter()
            .map(|action| {
                Action::wrap(action, &self.position_id, self.position.clone())
                    .and_then(|a| Py::new(py, a))
            })
            .collect::<PyResult<Vec<_>>>()?;
        let result = PyDict::new(py);
        result.set_item("actions", PyTuple::new(py, actions)?)?;
        result.set_item("exhausted", page.exhausted)?;
        result.set_item("examined", page.examined)?;
        Ok(result)
    }
}
#[pymethods]
impl Action {
    #[getter]
    fn position_id(&self) -> &str {
        &self.position_id
    }
    #[getter]
    fn action_id(&self) -> &str {
        &self.action_id
    }
    #[getter]
    fn actor(&self) -> &'static str {
        self.inner.color.as_str()
    }
    fn as_payload(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        conversion::to_python(py, &self.payload)
    }
    fn public_intent(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let position = self.position.clone();
        let action = self.inner.clone();
        let intent = py
            .detach(move || position.public_intent(&action))
            .map_err(error)?;
        conversion::validate(&intent)?;
        conversion::to_python(py, &intent)
    }
    fn snapshot(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        conversion::to_python(py, &self.envelope())
    }
    fn __repr__(&self) -> String {
        format!(
            "Action(actor={:?}, action_id={:?})",
            self.inner.color.as_str(),
            self.action_id
        )
    }
}

#[pyclass(
    name = "Position",
    module = "accelerate_chess._native",
    frozen,
    skip_from_py_object
)]
#[derive(Clone)]
pub struct Position {
    inner: EnginePosition,
    envelope: Arc<Value>,
    position_id: String,
}
impl Position {
    fn bind_payload(&self, py: Python<'_>, payload: Value) -> PyResult<Action> {
        if payload.get("positionKey").is_some() {
            return Err(PyValueError::new_err(
                "positionKey is private control metadata",
            ));
        }
        let inner = self.inner.clone();
        let requested = payload.clone();
        let action = py
            .detach(move || inner.bind_payload(payload))
            .map_err(error)?;
        let action = Action::wrap(action, &self.position_id, self.inner.clone())?;
        if *action.payload != requested {
            return Err(NativeError::new_err(
                "semantic payload must exactly match a legal action",
            ));
        }
        Ok(action)
    }
    fn wrap(inner: EnginePosition) -> PyResult<Self> {
        if inner.state().ruleset_id
            != catalog()["rulesVersion"]
                .as_str()
                .ok_or_else(|| NativeError::new_err("rules version missing"))?
        {
            return Err(PyValueError::new_err("engine rules version mismatch"));
        }
        let mut state = inner.export_state().map_err(error)?;
        let object = state
            .as_object_mut()
            .ok_or_else(|| NativeError::new_err("state must be object"))?;
        object.remove("rng");
        object.remove("history");
        let rng = value(&inner.state().rng)?;
        let history = value(&inner.state().history)?;
        let mut envelope = json!({"protocolVersion": POSITION_VERSION, "rulesVersion": catalog()["rulesVersion"], "catalogVersion": catalog()["catalogVersion"], "state": state, "rng": rng, "history": history});
        let position_id = digest(&envelope)?;
        envelope["positionId"] = json!(position_id);
        Ok(Self {
            inner,
            envelope: Arc::new(envelope),
            position_id,
        })
    }
    fn import(snapshot: Value) -> PyResult<Self> {
        conversion::validate(&snapshot)?;
        let fields = [
            "protocolVersion",
            "rulesVersion",
            "catalogVersion",
            "state",
            "rng",
            "history",
            "positionId",
        ];
        let object = snapshot
            .as_object()
            .ok_or_else(|| PyValueError::new_err("snapshot must be object"))?;
        if object.len() != fields.len() || fields.iter().any(|key| !object.contains_key(*key)) {
            return Err(PyValueError::new_err("unexpected snapshot fields"));
        }
        if snapshot["protocolVersion"] != POSITION_VERSION
            || snapshot["rulesVersion"] != catalog()["rulesVersion"]
            || snapshot["catalogVersion"] != catalog()["catalogVersion"]
        {
            return Err(PyValueError::new_err("snapshot version mismatch"));
        }
        let mut content = snapshot.clone();
        content.as_object_mut().unwrap().remove("positionId");
        if snapshot["positionId"].as_str() != Some(&digest(&content)?) {
            return Err(PyValueError::new_err("snapshot identity mismatch"));
        }
        let state = snapshot["state"]
            .as_object()
            .ok_or_else(|| PyValueError::new_err("snapshot state must be object"))?;
        if state.contains_key("rng") || state.contains_key("history") {
            return Err(PyValueError::new_err(
                "state must not duplicate outer RNG/history",
            ));
        }
        let rng: RngState = serde_json::from_value(snapshot["rng"].clone())
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        if rng.algorithm != "lcg32-v1" {
            return Err(PyValueError::new_err("unsupported RNG algorithm"));
        }
        let history: Vec<Value> = serde_json::from_value(snapshot["history"].clone())
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        let rules_version = snapshot["rulesVersion"]
            .as_str()
            .ok_or_else(|| PyValueError::new_err("snapshot rules version must be text"))?;
        let imported = Self::wrap(
            EnginePosition::from_snapshot_value_with_rules_version(
                Value::Object(state.clone()),
                rules_version,
            )
            .map_err(error)?
            .with_metadata(rng, history)
            .map_err(error)?,
        )?;
        if canonical_bytes(imported.envelope.as_ref())? != canonical_bytes(&snapshot)? {
            return Err(PyValueError::new_err(
                "snapshot is not canonical engine state; use from_state for explicit initialization/normalization",
            ));
        }
        Ok(imported)
    }
}
#[pymethods]
impl Position {
    #[staticmethod]
    #[pyo3(signature = (config, observation, seed=0))]
    fn sample_initial_public(
        py: Python<'_>,
        config: &Bound<'_, PyAny>,
        observation: &Bound<'_, PyAny>,
        seed: u32,
    ) -> PyResult<Self> {
        let config: GameConfig = serde_json::from_value(conversion::from_python(config)?)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        let observation = conversion::from_python(observation)?;
        Self::wrap(
            py.detach(move || EnginePosition::sample_initial_public(config, observation, seed))
                .map_err(error)?,
        )
    }
    fn condition_public_identities(
        &self,
        py: Python<'_>,
        observation: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let observation = conversion::from_python(observation)?;
        let position = self.inner.clone();
        Self::wrap(
            py.detach(move || position.condition_public_identities(observation))
                .map_err(error)?,
        )
    }
    #[staticmethod]
    #[pyo3(signature = (config=None, seed=0))]
    fn new_game(py: Python<'_>, config: Option<&Bound<'_, PyAny>>, seed: u64) -> PyResult<Self> {
        let config = config
            .map(conversion::from_python)
            .transpose()?
            .unwrap_or_else(|| json!({}));
        let config: GameConfig =
            serde_json::from_value(config).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Self::wrap(
            py.detach(move || EnginePosition::new_game(config, seed))
                .map_err(error)?,
        )
    }
    #[staticmethod]
    fn from_state(py: Python<'_>, state: &Bound<'_, PyAny>) -> PyResult<Self> {
        let state = conversion::from_python(state)?;
        Self::wrap(
            py.detach(move || EnginePosition::from_snapshot_value(state))
                .map_err(error)?,
        )
    }
    #[staticmethod]
    fn from_snapshot(snapshot: &Bound<'_, PyAny>) -> PyResult<Self> {
        Self::import(conversion::from_python(snapshot)?)
    }
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        if json.len() > conversion::MAX_BYTES {
            return Err(PyValueError::new_err("JSON exceeds 8 MiB limit"));
        }
        Self::import(serde_json::from_str(json).map_err(|e| PyValueError::new_err(e.to_string()))?)
    }
    fn snapshot(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        conversion::to_python(py, &self.envelope)
    }
    fn state(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        conversion::to_python(py, &self.envelope["state"])
    }
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&*self.envelope).map_err(|e| NativeError::new_err(e.to_string()))
    }
    #[getter]
    fn position_id(&self) -> &str {
        &self.position_id
    }
    #[getter]
    fn actor(&self) -> &'static str {
        self.inner.actor().as_str()
    }
    #[getter]
    fn decision_actor(&self) -> &'static str {
        self.inner.decision_actor().as_str()
    }
    #[getter]
    fn result(&self) -> Option<&'static str> {
        result(self.inner.result())
    }
    fn legal_actions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        let inner = self.inner.clone();
        let actions = py.detach(move || inner.legal_actions()).map_err(error)?;
        let actions = actions
            .into_iter()
            .map(|a| {
                Action::wrap(a, &self.position_id, self.inner.clone()).and_then(|a| Py::new(py, a))
            })
            .collect::<PyResult<Vec<_>>>()?;
        PyTuple::new(py, actions)
    }
    fn action_stream(&self, py: Python<'_>) -> PyResult<ActionStream> {
        let inner = self.inner.clone();
        Ok(ActionStream {
            position: self.inner.clone(),
            inner: Arc::new(Mutex::new(
                py.detach(move || inner.action_stream()).map_err(error)?,
            )),
            position_id: self.position_id.clone(),
        })
    }
    fn bind_action(&self, py: Python<'_>, payload: &Bound<'_, PyAny>) -> PyResult<Action> {
        self.bind_payload(py, conversion::from_python(payload)?)
    }
    fn bind_public_intent(&self, py: Python<'_>, intent: &Bound<'_, PyAny>) -> PyResult<Action> {
        let intent = conversion::from_python(intent)?;
        let requested = intent.clone();
        let position = self.inner.clone();
        let (action, projected) = py
            .detach(move || {
                let action = position.bind_public_intent(intent)?;
                let projected = position.public_intent(&action)?;
                Ok::<_, EngineError>((action, projected))
            })
            .map_err(error)?;
        if projected != requested {
            return Err(NativeError::new_err(
                "public intent must exactly match a source UI choice",
            ));
        }
        Action::wrap(action, &self.position_id, self.inner.clone())
    }
    fn bind_snapshot(&self, py: Python<'_>, snapshot: &Bound<'_, PyAny>) -> PyResult<Action> {
        let snapshot = conversion::from_python(snapshot)?;
        let object = snapshot
            .as_object()
            .ok_or_else(|| PyValueError::new_err("action snapshot must be object"))?;
        let fields = ["protocolVersion", "positionId", "actionId", "payload"];
        if object.len() != fields.len() || fields.iter().any(|key| !object.contains_key(*key)) {
            return Err(PyValueError::new_err("unexpected action snapshot fields"));
        }
        if snapshot["protocolVersion"] != ACTION_VERSION {
            return Err(PyValueError::new_err("action version mismatch"));
        }
        if snapshot["positionId"].as_str() != Some(&self.position_id) {
            return Err(StaleActionError::new_err(
                "action snapshot belongs to another position",
            ));
        }
        if snapshot["actionId"].as_str() != Some(&digest(&snapshot["payload"])?) {
            return Err(PyValueError::new_err("action identity mismatch"));
        }
        self.bind_payload(py, snapshot["payload"].clone())
    }
    fn apply(&self, py: Python<'_>, action: &Action) -> PyResult<StepResult> {
        if action.position_id != self.position_id {
            return Err(StaleActionError::new_err(
                "action belongs to another position",
            ));
        }
        let inner = self.inner.clone();
        let action = action.inner.clone();
        let step = py.detach(move || inner.apply(&action)).map_err(error)?;
        StepResult::wrap(step)
    }
    /// Reconstruct a supported public draw using an independent particle seed.
    fn apply_conditioned_public(
        &self,
        py: Python<'_>,
        action: &Action,
        expected_observation: &Bound<'_, PyAny>,
        independent_seed: u32,
    ) -> PyResult<StepResult> {
        if action.position_id != self.position_id {
            return Err(StaleActionError::new_err(
                "action belongs to another position",
            ));
        }
        let observation = conversion::from_python(expected_observation)?;
        let inner = self.inner.clone();
        let action = action.inner.clone();
        let step = py
            .detach(move || inner.apply_conditioned_public(&action, observation, independent_seed))
            .map_err(error)?;
        StepResult::wrap(step)
    }
    /// Filter only source-proven public incompatibility before applying effects.
    fn public_transition_compatible(
        &self,
        py: Python<'_>,
        action: &Action,
        expected_observation: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        if action.position_id != self.position_id {
            return Err(StaleActionError::new_err(
                "action belongs to another position",
            ));
        }
        let observation = conversion::from_python(expected_observation)?;
        let inner = self.inner.clone();
        let action = action.inner.clone();
        py.detach(move || inner.public_transition_compatible(&action, observation))
            .map_err(error)
    }
    /// Reconstruct a supported public transition with its source density correction.
    fn apply_weighted_conditioned_public<'py>(
        &self,
        py: Python<'py>,
        action: &Action,
        expected_observation: &Bound<'_, PyAny>,
        independent_seed: u32,
    ) -> PyResult<Bound<'py, PyDict>> {
        if action.position_id != self.position_id {
            return Err(StaleActionError::new_err(
                "action belongs to another position",
            ));
        }
        let observation = conversion::from_python(expected_observation)?;
        let inner = self.inner.clone();
        let action = action.inner.clone();
        let proposal = py
            .detach(move || {
                inner.apply_weighted_conditioned_public(&action, observation, independent_seed)
            })
            .map_err(error)?;
        proposal_probabilities(
            proposal.importance_weight,
            proposal.source_probability,
            proposal.proposal_probability,
        )?;
        let result = PyDict::new(py);
        result.set_item("step", Py::new(py, StepResult::wrap(proposal.step)?)?)?;
        result.set_item("importance_weight", proposal.importance_weight)?;
        result.set_item("source_probability", proposal.source_probability)?;
        result.set_item("proposal_probability", proposal.proposal_probability)?;
        Ok(result)
    }
    /// Propose a source-valid hidden offer with an explicit importance correction.
    fn condition_hidden_opening_draft<'py>(
        &self,
        py: Python<'py>,
        expected_next_public: &Bound<'_, PyAny>,
        independent_seed: u32,
    ) -> PyResult<Bound<'py, PyDict>> {
        let observation = conversion::from_python(expected_next_public)?;
        let inner = self.inner.clone();
        let proposal = py
            .detach(move || inner.condition_hidden_opening_draft(observation, independent_seed))
            .map_err(error)?;
        proposal_probabilities(
            proposal.importance_weight,
            proposal.source_probability,
            proposal.proposal_probability,
        )?;
        let result = PyDict::new(py);
        result.set_item("position", Py::new(py, Self::wrap(proposal.position)?)?)?;
        result.set_item("importance_weight", proposal.importance_weight)?;
        result.set_item("source_probability", proposal.source_probability)?;
        result.set_item("proposal_probability", proposal.proposal_probability)?;
        Ok(result)
    }
    fn observe(&self, py: Python<'_>, viewer: &str) -> PyResult<Py<PyAny>> {
        let viewer = color(viewer)?;
        let inner = self.inner.clone();
        let observation = py
            .detach(move || inner.try_observe(viewer))
            .map_err(error)?;
        conversion::to_python(py, &value(&observation)?)
    }
    /// An owned object array of the public board, never the private board.
    fn board<'py>(
        &self,
        py: Python<'py>,
        viewer: &str,
    ) -> PyResult<Bound<'py, PyArray2<Py<PyAny>>>> {
        let viewer = color(viewer)?;
        let inner = self.inner.clone();
        let observation = py
            .detach(move || inner.try_observe(viewer))
            .map_err(error)?;
        let rows = observation
            .board
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| conversion::to_python(py, cell.as_ref().unwrap_or(&Value::Null)))
                    .collect::<PyResult<Vec<_>>>()
            })
            .collect::<PyResult<Vec<_>>>()?;
        PyArray2::from_vec2(py, &rows).map_err(|e| NativeError::new_err(e.to_string()))
    }
    fn __repr__(&self) -> String {
        format!(
            "Position(actor={:?}, position_id={:?})",
            self.inner.actor().as_str(),
            self.position_id
        )
    }
}

#[pyclass(name = "StepResult", module = "accelerate_chess._native", frozen)]
pub struct StepResult {
    position: Position,
    actor: String,
    turn_changed: bool,
    captures: Arc<Value>,
    result: Option<String>,
}
impl StepResult {
    fn wrap(step: EngineStepResult) -> PyResult<Self> {
        Ok(Self {
            position: Position::wrap(step.position)?,
            actor: step.actor.as_str().into(),
            turn_changed: step.turn_changed,
            captures: Arc::new(value(&step.captures)?),
            result: result(step.result).map(str::to_owned),
        })
    }
}
#[pymethods]
impl StepResult {
    #[getter]
    fn position(&self) -> Position {
        self.position.clone()
    }
    #[getter]
    fn actor(&self) -> &str {
        &self.actor
    }
    #[getter]
    fn turn_changed(&self) -> bool {
        self.turn_changed
    }
    #[getter]
    fn captures(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        conversion::to_python(py, &self.captures)
    }
    #[getter]
    fn result(&self) -> Option<&str> {
        self.result.as_deref()
    }
}

/// Return an owned copy of the same frozen catalog compiled into the engine boundary.
#[pyfunction]
fn site_catalog(py: Python<'_>) -> PyResult<Py<PyAny>> {
    conversion::to_python(py, catalog())
}

/// Return an owned copy of the public projection policy compiled into this wheel.
#[pyfunction]
fn site_observation_policy(py: Python<'_>) -> PyResult<Py<PyAny>> {
    conversion::to_python(py, observation_policy())
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(site_catalog, module)?)?;
    module.add_function(wrap_pyfunction!(site_observation_policy, module)?)?;
    module.add_class::<inference::InferenceSession>()?;
    module.add_class::<Position>()?;
    module.add_class::<Action>()?;
    module.add_class::<ActionStream>()?;
    module.add_class::<StepResult>()?;
    module.add("NativeError", module.py().get_type::<NativeError>())?;
    module.add(
        "StaleActionError",
        module.py().get_type::<StaleActionError>(),
    )?;
    module.add(
        "UnsupportedFeatureError",
        module.py().get_type::<UnsupportedFeatureError>(),
    )?;
    module.add(
        "ConditioningMismatchError",
        module.py().get_type::<ConditioningMismatchError>(),
    )?;
    module.add(
        "RULES_VERSION",
        catalog()["rulesVersion"]
            .as_str()
            .ok_or_else(|| NativeError::new_err("catalog rules version missing"))?,
    )?;
    module.add(
        "CATALOG_VERSION",
        catalog()["catalogVersion"]
            .as_str()
            .ok_or_else(|| NativeError::new_err("catalog version missing"))?,
    )?;
    Ok(())
}
