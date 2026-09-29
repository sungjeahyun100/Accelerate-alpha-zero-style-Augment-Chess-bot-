//! NumPy ownership boundary for the independent CPU runtime.
use accelerate_runtime::{
    Backend, InferenceSession as Session, Input, Limits, ModelConfig, NamedInput, TensorDtype,
    TypedInferenceSession, TypedTensorData,
};
use numpy::{
    PyArray2, PyReadonlyArray2, PyReadonlyArray3, PyReadonlyArray4, PyReadonlyArrayDyn,
    PyUntypedArrayMethods, ndarray::Array2,
};
use pyo3::{exceptions::PyValueError, prelude::*, types::PyDict};
use std::{fs::File, io::Read, path::PathBuf, sync::Mutex};

type Arrays<'py> = (Bound<'py, PyArray2<f32>>, Bound<'py, PyArray2<f32>>);

fn error(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

#[pyclass(name = "InferenceSession", module = "accelerate_chess._native", frozen)]
pub struct InferenceSession {
    session: Mutex<SessionVariant>,
    config: Option<ModelConfig>,
    limits: Limits,
    backend: String,
    architecture_family: String,
    encoder_hash: String,
    model_sha256: String,
}

enum SessionVariant {
    Legacy(Session),
    Typed(TypedInferenceSession),
}

fn manifest_version(path: &PathBuf) -> PyResult<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(error)?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(error("artifact exceeds byte limit"));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(error)?;
    value["version"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| error("manifest version missing"))
}

#[pymethods]
impl InferenceSession {
    #[new]
    #[pyo3(signature=(manifest_path, backend="ort", expected_encoder_hash=None, *, max_batch=64, max_actions=4096, max_input_elements=16777216, threads=1))]
    // Keep independently meaningful resource limits explicit in the Python FFI
    // signature, rather than hiding them in an unchecked options dictionary.
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        manifest_path: PathBuf,
        backend: &str,
        expected_encoder_hash: Option<String>,
        max_batch: usize,
        max_actions: usize,
        max_input_elements: usize,
        threads: usize,
    ) -> PyResult<Self> {
        let backend = Backend::parse(backend).map_err(error)?;
        let limits = Limits {
            max_batch,
            max_actions,
            max_input_elements,
            threads,
        };
        let version = manifest_version(&manifest_path)?;
        if version != "onnx-policy-value-v2" && version != "onnx-policy-value-v3" {
            return Err(error("unsupported deployment manifest version"));
        }
        let session = py
            .detach(move || {
                if version == "onnx-policy-value-v2" {
                    Session::load(
                        &manifest_path,
                        backend,
                        expected_encoder_hash.as_deref(),
                        limits,
                    )
                    .map(SessionVariant::Legacy)
                    .map_err(|e| e.to_string())
                } else {
                    TypedInferenceSession::load(
                        &manifest_path,
                        backend,
                        expected_encoder_hash.as_deref(),
                        limits,
                    )
                    .map(SessionVariant::Typed)
                    .map_err(|e| e.to_string())
                }
            })
            .map_err(error)?;
        let (config, limits, family, encoder_hash, model_sha256) = match &session {
            SessionVariant::Legacy(value) => (
                Some(value.bundle.model_config.clone()),
                value.limits.clone(),
                "legacy-resnet".to_owned(),
                value.bundle.encoder_hash.clone(),
                value.bundle.model_sha256.clone(),
            ),
            SessionVariant::Typed(value) => (
                None,
                value.limits.clone(),
                value.bundle.architecture_family.clone(),
                value.bundle.encoder_hash.clone(),
                value.bundle.model_sha256.clone(),
            ),
        };
        Ok(Self {
            config,
            limits,
            backend: backend.name().into(),
            architecture_family: family,
            encoder_hash,
            model_sha256,
            session: Mutex::new(session),
        })
    }
    #[getter]
    fn backend(&self) -> &str {
        &self.backend
    }
    #[getter]
    fn encoder_hash(&self) -> &str {
        &self.encoder_hash
    }
    #[getter]
    fn model_sha256(&self) -> &str {
        &self.model_sha256
    }
    #[getter]
    fn architecture_family(&self) -> &str {
        &self.architecture_family
    }
    fn evaluate<'py>(
        &self,
        py: Python<'py>,
        board: PyReadonlyArray4<'py, f32>,
        condition: PyReadonlyArray2<'py, f32>,
        action_features: PyReadonlyArray3<'py, f32>,
    ) -> PyResult<Arrays<'py>> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| error("typed bundle requires evaluate_typed"))?;
        let batch = board.shape()[0];
        let actions = action_features.shape()[1];
        if !(1..=self.limits.max_batch).contains(&batch)
            || !(1..=self.limits.max_actions).contains(&actions)
        {
            return Err(PyValueError::new_err(
                "batch or candidate count exceeds inference limits",
            ));
        }
        if board.shape() != [batch, config.board_channels, 8, 8]
            || condition.shape() != [batch, config.condition_dim]
            || action_features.shape() != [batch, actions, config.action_dim]
        {
            return Err(PyValueError::new_err("input tensor shape mismatch"));
        }
        let input_elements = board
            .len()
            .checked_add(condition.len())
            .and_then(|n| n.checked_add(action_features.len()))
            .ok_or_else(|| PyValueError::new_err("input element count overflow"))?;
        if input_elements > self.limits.max_input_elements {
            return Err(PyValueError::new_err(
                "input element count exceeds inference limits",
            ));
        }
        let workspace = batch as u128 * config.channels as u128 * (256 + 2 * actions as u128);
        if input_elements as u128 + workspace > 67_108_864 {
            return Err(PyValueError::new_err(
                "model input and activation working set exceeds 256 MiB",
            ));
        }
        // All array copies finish before detaching. No NumPy borrow or Python
        // reference enters the inference engine; inputs are logical C-order.
        let b = Input::new(
            board.shape().to_vec(),
            board.as_array().iter().copied().collect(),
        )
        .map_err(error)?;
        let c = Input::new(
            condition.shape().to_vec(),
            condition.as_array().iter().copied().collect(),
        )
        .map_err(error)?;
        let a = Input::new(
            action_features.shape().to_vec(),
            action_features.as_array().iter().copied().collect(),
        )
        .map_err(error)?;
        let outputs = py.detach(move || {
            let mut guard = self
                .session
                .lock()
                .map_err(|_| error("inference session poisoned"))?;
            match &mut *guard {
                SessionVariant::Legacy(session) => session.evaluate(b, c, a).map_err(error),
                SessionVariant::Typed(_) => Err(error("typed bundle requires evaluate_typed")),
            }
        })?;
        let policy =
            Array2::from_shape_vec((batch, actions), outputs.policy_logits.data).map_err(error)?;
        let value = Array2::from_shape_vec((batch, 1), outputs.value.data).map_err(error)?;
        Ok((
            PyArray2::from_owned_array(py, policy),
            PyArray2::from_owned_array(py, value),
        ))
    }
    fn evaluate_typed<'py>(
        &self,
        py: Python<'py>,
        inputs: &Bound<'py, PyDict>,
    ) -> PyResult<Arrays<'py>> {
        let specs = {
            let guard = self
                .session
                .lock()
                .map_err(|_| error("inference session poisoned"))?;
            match &*guard {
                SessionVariant::Typed(session) => session.bundle.onnx.inputs.clone(),
                SessionVariant::Legacy(_) => return Err(error("legacy bundle requires evaluate")),
            }
        };
        if inputs.len() != specs.len() {
            return Err(error("typed input count mismatch"));
        }
        let mut owned = Vec::with_capacity(specs.len());
        for spec in &specs {
            let value = inputs
                .get_item(&spec.name)?
                .ok_or_else(|| error(format!("typed input {} missing", spec.name)))?;
            let (shape, data) = match spec.dtype {
                TensorDtype::Float32 => {
                    let array = value.extract::<PyReadonlyArrayDyn<'_, f32>>()?;
                    (
                        array.shape().to_vec(),
                        TypedTensorData::Float32(array.as_array().iter().copied().collect()),
                    )
                }
                TensorDtype::Int64 => {
                    let array = value.extract::<PyReadonlyArrayDyn<'_, i64>>()?;
                    (
                        array.shape().to_vec(),
                        TypedTensorData::Int64(array.as_array().iter().copied().collect()),
                    )
                }
                TensorDtype::Bool => {
                    let array = value.extract::<PyReadonlyArrayDyn<'_, bool>>()?;
                    (
                        array.shape().to_vec(),
                        TypedTensorData::Bool(array.as_array().iter().copied().collect()),
                    )
                }
            };
            owned.push(NamedInput {
                name: spec.name.clone(),
                shape,
                data,
            });
        }
        let outputs = py.detach(move || {
            let mut guard = self
                .session
                .lock()
                .map_err(|_| error("inference session poisoned"))?;
            match &mut *guard {
                SessionVariant::Typed(session) => session.evaluate(owned).map_err(error),
                SessionVariant::Legacy(_) => Err(error("legacy bundle requires evaluate")),
            }
        })?;
        let batch = outputs.value.shape[0];
        let actions = outputs.policy_logits.shape[1];
        let policy =
            Array2::from_shape_vec((batch, actions), outputs.policy_logits.data).map_err(error)?;
        let value = Array2::from_shape_vec((batch, 1), outputs.value.data).map_err(error)?;
        Ok((
            PyArray2::from_owned_array(py, policy),
            PyArray2::from_owned_array(py, value),
        ))
    }
    fn __repr__(&self) -> String {
        format!(
            "InferenceSession(backend={:?}, model_sha256={:?})",
            self.backend, self.model_sha256
        )
    }
}
