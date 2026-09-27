//! NumPy ownership boundary for the independent CPU runtime.
use accelerate_runtime::{Backend, InferenceSession as Session, Input, Limits, ModelConfig};
use numpy::{
    PyArray2, PyReadonlyArray2, PyReadonlyArray3, PyReadonlyArray4, PyUntypedArrayMethods,
    ndarray::Array2,
};
use pyo3::{exceptions::PyValueError, prelude::*};
use std::{path::PathBuf, sync::Mutex};

type Arrays<'py> = (Bound<'py, PyArray2<f32>>, Bound<'py, PyArray2<f32>>);

fn error(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

#[pyclass(name = "InferenceSession", module = "accelerate_chess._native", frozen)]
pub struct InferenceSession {
    session: Mutex<Session>,
    config: ModelConfig,
    limits: Limits,
    backend: String,
    encoder_hash: String,
    model_sha256: String,
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
        let session = py
            .detach(move || {
                Session::load(
                    &manifest_path,
                    backend,
                    expected_encoder_hash.as_deref(),
                    limits,
                )
            })
            .map_err(error)?;
        Ok(Self {
            config: session.bundle.model_config.clone(),
            limits: session.limits.clone(),
            backend: backend.name().into(),
            encoder_hash: session.bundle.encoder_hash.clone(),
            model_sha256: session.bundle.model_sha256.clone(),
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
    fn evaluate<'py>(
        &self,
        py: Python<'py>,
        board: PyReadonlyArray4<'py, f32>,
        condition: PyReadonlyArray2<'py, f32>,
        action_features: PyReadonlyArray3<'py, f32>,
    ) -> PyResult<Arrays<'py>> {
        let batch = board.shape()[0];
        let actions = action_features.shape()[1];
        if !(1..=self.limits.max_batch).contains(&batch)
            || !(1..=self.limits.max_actions).contains(&actions)
        {
            return Err(PyValueError::new_err(
                "batch or candidate count exceeds inference limits",
            ));
        }
        if board.shape() != [batch, self.config.board_channels, 8, 8]
            || condition.shape() != [batch, self.config.condition_dim]
            || action_features.shape() != [batch, actions, self.config.action_dim]
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
        let workspace = batch as u128 * self.config.channels as u128 * (256 + 2 * actions as u128);
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
            self.session
                .lock()
                .map_err(|_| error("inference session poisoned"))?
                .evaluate(b, c, a)
                .map_err(error)
        })?;
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
