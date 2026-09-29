//! Explicit CPU backends behind a validated common ONNX artifact contract.
mod manifest;
use anyhow::{Context, Result, bail, ensure};
pub use manifest::{Axis, Bundle, ModelConfig, TensorDtype, TypedBundle};
use ort::{
    session::{Session, SessionInputValue},
    value::Tensor as OrtTensor,
};
use std::collections::BTreeMap;
use std::path::Path;
use tract::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Ort,
    Tract,
}
impl Backend {
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "ort" => Ok(Self::Ort),
            "tract" => Ok(Self::Tract),
            _ => bail!("unsupported backend {name}; select ort or tract explicitly"),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Ort => "ort",
            Self::Tract => "tract",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_batch: usize,
    pub max_actions: usize,
    pub max_input_elements: usize,
    pub threads: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_batch: 64,
            max_actions: 4096,
            max_input_elements: 16_777_216,
            threads: 1,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=1024).contains(&self.max_batch)
                && (1..=65536).contains(&self.max_actions)
                && (1..=134_217_728).contains(&self.max_input_elements)
                && (1..=64).contains(&self.threads),
            "invalid inference limits"
        );
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Input {
    pub shape: Vec<usize>,
    pub data: Vec<f32>,
}
impl Input {
    pub fn new(shape: Vec<usize>, data: Vec<f32>) -> Result<Self> {
        let elements = shape
            .iter()
            .try_fold(1usize, |n, dim| n.checked_mul(*dim))
            .context("tensor shape overflow")?;
        ensure!(
            elements == data.len() && data.iter().all(|v| v.is_finite()),
            "tensor elements/finite values mismatch"
        );
        Ok(Self { shape, data })
    }
}
#[derive(Debug)]
pub struct Output {
    pub policy_logits: Input,
    pub value: Input,
}
enum Runner {
    Ort(Session),
    Tract(tract::Runnable),
    TractTyped(Vec<u8>),
}
impl Runner {
    fn load(bytes: &[u8], backend: Backend, limits: &Limits) -> Result<Self> {
        match backend {
            Backend::Ort => Ok(Self::Ort(
                Session::builder()?
                    .with_intra_threads(limits.threads)
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .with_inter_threads(1)
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .commit_from_memory(bytes)?,
            )),
            Backend::Tract => {
                ensure!(
                    limits.threads == 1,
                    "tract CPU runtime currently requires threads=1"
                );
                let model = tract::onnx()?.load_buffer(bytes)?.into_model()?;
                Ok(Self::Tract(
                    tract::runtime_for_name("default")?.prepare(model)?,
                ))
            }
        }
    }
}
pub struct InferenceSession {
    pub bundle: Bundle,
    pub limits: Limits,
    pub backend: Backend,
    runner: Runner,
}
impl InferenceSession {
    pub fn load(
        path: &Path,
        backend: Backend,
        expected_encoder_hash: Option<&str>,
        limits: Limits,
    ) -> Result<Self> {
        limits.validate()?;
        let (bundle, bytes) = Bundle::load(path, expected_encoder_hash)?;
        // Both loaders receive the same already-hashed bytes; no second file
        // open can replace a verified model between validation and execution.
        let runner = Runner::load(&bytes, backend, &limits)?;
        Ok(Self {
            bundle,
            backend,
            limits,
            runner,
        })
    }
    pub fn validate_inputs(
        &self,
        board: &Input,
        condition: &Input,
        actions: &Input,
    ) -> Result<(usize, usize)> {
        let config = &self.bundle.model_config;
        let batch = board.shape.first().copied().unwrap_or(0);
        let candidates = actions.shape.get(1).copied().unwrap_or(0);
        ensure!(
            (1..=self.limits.max_batch).contains(&batch)
                && (1..=self.limits.max_actions).contains(&candidates),
            "batch or candidate count exceeds inference limits"
        );
        ensure!(
            board.shape == [batch, config.board_channels, 8, 8]
                && condition.shape == [batch, config.condition_dim]
                && actions.shape == [batch, candidates, config.action_dim],
            "input tensor shape mismatch"
        );
        let mut total = 0usize;
        for input in [board, condition, actions] {
            let elements = input
                .shape
                .iter()
                .try_fold(1usize, |n, d| n.checked_mul(*d))
                .context("tensor shape overflow")?;
            total = total
                .checked_add(elements)
                .context("tensor element overflow")?;
            ensure!(
                elements == input.data.len() && input.data.iter().all(|v| v.is_finite()),
                "tensor elements or finite values mismatch"
            );
        }
        ensure!(
            total <= self.limits.max_input_elements,
            "input element count exceeds inference limits"
        );
        let workspace = batch as u128 * config.channels as u128 * (256 + 2 * candidates as u128);
        ensure!(
            total as u128 + workspace <= 67_108_864,
            "model input and activation working-set estimate exceeds 256 MiB buffer limit"
        );
        Ok((batch, candidates))
    }
    pub fn evaluate(&mut self, board: Input, condition: Input, actions: Input) -> Result<Output> {
        let (batch, candidates) = self.validate_inputs(&board, &condition, &actions)?;
        let outputs = match &mut self.runner {
            Runner::Ort(session) => {
                let b = OrtTensor::from_array((board.shape, board.data))?;
                let c = OrtTensor::from_array((condition.shape, condition.data))?;
                let a = OrtTensor::from_array((actions.shape, actions.data))?;
                let outputs = session
                    .run(ort::inputs!["board" => b, "condition" => c, "action_features" => a])?;
                let (policy_shape, policy) =
                    outputs["policy_logits"].try_extract_tensor::<f32>()?;
                let (value_shape, value) = outputs["value"].try_extract_tensor::<f32>()?;
                [
                    Input::new(
                        policy_shape.iter().map(|v| *v as usize).collect(),
                        policy.to_vec(),
                    )?,
                    Input::new(
                        value_shape.iter().map(|v| *v as usize).collect(),
                        value.to_vec(),
                    )?,
                ]
            }
            Runner::Tract(runnable) => {
                fn tensor(input: &Input) -> Result<tract::Tensor> {
                    let bytes: Vec<u8> = input.data.iter().flat_map(|v| v.to_ne_bytes()).collect();
                    tract::Tensor::from_bytes(DatumType::F32, &input.shape, &bytes)
                }
                let outputs =
                    runnable.run([tensor(&board)?, tensor(&condition)?, tensor(&actions)?])?;
                ensure!(outputs.len() == 2, "tract output count mismatch");
                fn output(tensor: &tract::Tensor) -> Result<Input> {
                    let (dtype, shape, bytes) = tensor.as_bytes()?;
                    ensure!(
                        dtype == DatumType::F32 && bytes.len().is_multiple_of(4),
                        "tract output dtype mismatch"
                    );
                    Input::new(
                        shape.to_vec(),
                        bytes
                            .chunks_exact(4)
                            .map(|b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
                            .collect(),
                    )
                }
                [output(&outputs[0])?, output(&outputs[1])?]
            }
            Runner::TractTyped(_) => bail!("typed tract runner cannot evaluate legacy inputs"),
        };
        let [policy_logits, value] = outputs;
        ensure!(
            policy_logits.shape == [batch, candidates] && value.shape == [batch, 1],
            "runtime output shape mismatch"
        );
        ensure!(
            value.data.iter().all(|v| (-1.00001..=1.00001).contains(v)),
            "runtime value outside [-1,1]"
        );
        Ok(Output {
            policy_logits,
            value,
        })
    }
}

#[derive(Clone, Debug)]
pub enum TypedTensorData {
    Float32(Vec<f32>),
    Int64(Vec<i64>),
    Bool(Vec<bool>),
}
impl TypedTensorData {
    fn len(&self) -> usize {
        match self {
            Self::Float32(data) => data.len(),
            Self::Int64(data) => data.len(),
            Self::Bool(data) => data.len(),
        }
    }
    fn dtype(&self) -> TensorDtype {
        match self {
            Self::Float32(_) => TensorDtype::Float32,
            Self::Int64(_) => TensorDtype::Int64,
            Self::Bool(_) => TensorDtype::Bool,
        }
    }
}

#[derive(Clone, Debug)]
pub struct NamedInput {
    pub name: String,
    pub shape: Vec<usize>,
    pub data: TypedTensorData,
}

/// Borrowed metadata for validating a typed request before copying array data.
#[derive(Clone, Debug)]
pub struct TypedInputShape<'a> {
    pub name: &'a str,
    pub dtype: TensorDtype,
    pub shape: &'a [usize],
}

pub struct TypedInferenceSession {
    pub bundle: TypedBundle,
    pub limits: Limits,
    pub backend: Backend,
    runner: Runner,
}

impl TypedInferenceSession {
    pub fn load(
        path: &Path,
        backend: Backend,
        expected_encoder_hash: Option<&str>,
        limits: Limits,
    ) -> Result<Self> {
        limits.validate()?;
        let (bundle, bytes) = TypedBundle::load(path, expected_encoder_hash)?;
        let runner = match backend {
            Backend::Ort => Runner::load(&bytes, backend, &limits)?,
            Backend::Tract => {
                ensure!(
                    limits.threads == 1,
                    "tract CPU runtime currently requires threads=1"
                );
                // Symbolic record and candidate axes are bound to each validated
                // request before tract prepares the graph; the hashed bytes stay
                // owned by this session and are never reopened from disk.
                let _ = tract::onnx()?.load_buffer(&bytes)?;
                Runner::TractTyped(bytes)
            }
        };
        Ok(Self {
            bundle,
            limits,
            backend,
            runner,
        })
    }

    /// Enforces input shape, element, and byte limits before an owned copy.
    /// The intermediate-memory check is an estimate for exporter-produced
    /// model families, not a hard bound for an arbitrary ONNX graph.
    pub fn validate_shapes(
        &self,
        inputs: &[TypedInputShape<'_>],
    ) -> Result<(usize, usize, BTreeMap<String, usize>)> {
        let specs = &self.bundle.onnx.inputs;
        ensure!(inputs.len() == specs.len(), "typed input count mismatch");
        let mut symbols: BTreeMap<String, usize> = BTreeMap::new();
        let mut total_elements = 0usize;
        let mut total_bytes = 0usize;
        for (input, spec) in inputs.iter().zip(specs) {
            ensure!(
                input.name == spec.name && input.dtype == spec.dtype,
                "typed input name/dtype mismatch"
            );
            ensure!(
                input.shape.len() == spec.shape.len(),
                "typed input rank mismatch"
            );
            let elements = input
                .shape
                .iter()
                .try_fold(1usize, |n, dim| n.checked_mul(*dim))
                .context("typed shape overflow")?;
            total_elements = total_elements
                .checked_add(elements)
                .context("typed input element overflow")?;
            for (actual, required) in input.shape.iter().zip(&spec.shape) {
                match required {
                    Axis::Fixed(expected) => {
                        ensure!(actual == expected, "typed input fixed dimension mismatch")
                    }
                    Axis::Dynamic(name) => {
                        ensure!(
                            *actual > 0 || name == "relations",
                            "typed input contains empty axis"
                        );
                        let maximum = self
                            .bundle
                            .resource_limits
                            .axis_maxima
                            .get(name)
                            .context("unbounded typed axis")?;
                        ensure!(
                            actual <= maximum,
                            "typed axis {name} size {actual} exceeds limit {maximum}"
                        );
                        if let Some(previous) = symbols.insert(name.clone(), *actual) {
                            ensure!(previous == *actual, "typed shared dynamic axis mismatch");
                        }
                    }
                }
            }
            total_bytes = total_bytes
                .checked_add(
                    elements
                        .checked_mul(spec.dtype.element_bytes())
                        .context("typed tensor byte overflow")?,
                )
                .context("typed input byte overflow")?;
        }
        let batch = *symbols.get("batch").context("typed batch axis missing")?;
        let actions = *symbols
            .get("actions")
            .context("typed actions axis missing")?;
        ensure!(
            batch <= self.limits.max_batch && actions <= self.limits.max_actions,
            "batch or candidate count exceeds inference limits"
        );
        ensure!(
            total_elements <= self.limits.max_input_elements,
            "typed input element count exceeds inference limits"
        );
        ensure!(
            total_bytes <= self.bundle.resource_limits.max_input_bytes,
            "typed input bytes exceed inference limits"
        );
        let feature_limits = &self.bundle.encoder["feature_schema"]["limits"];
        ensure!(
            total_bytes as u64
                <= feature_limits["max_input_bytes"]
                    .as_u64()
                    .context("typed input feature byte limit missing")?,
            "typed inputs exceed encoder byte limit"
        );
        for (axis, limit) in [
            ("batch", "max_batch"),
            ("records", "max_records"),
            ("relations", "max_relations"),
            ("actions", "max_candidates"),
            ("nodes", "max_candidate_nodes"),
            ("height", "max_board_axis"),
            ("width", "max_board_axis"),
        ] {
            if let Some(actual) = symbols.get(axis) {
                ensure!(
                    *actual as u64
                        <= feature_limits[limit]
                            .as_u64()
                            .context("typed encoder axis limit missing")?,
                    "typed {axis} exceeds encoder limit"
                );
            }
        }
        let config = &self.bundle.model_config;
        let records = symbols.get("records").copied().unwrap_or(1) as u128;
        let relations = symbols.get("relations").copied().unwrap_or(0) as u128;
        let nodes = symbols.get("nodes").copied().unwrap_or(1) as u128;
        let hidden = config
            .get("typed")
            .or_else(|| config.get("typed_context"))
            .and_then(|value| value.get("hidden_dim"))
            .and_then(|value| value.as_u64())
            .context("typed model hidden dimension missing")? as u128;
        let typed_work = batch as u128
            * hidden
            * (8 * records + 6 * relations + 6 * actions as u128 * nodes)
            * 4;
        let intermediate = if self.bundle.architecture_family == "entity-transformer" {
            let heads = config
                .get("heads")
                .and_then(|value| value.as_u64())
                .context("typed Transformer head count missing")? as u128;
            typed_work + 3u128 * batch as u128 * heads * records * records * 4
        } else {
            let height = symbols.get("height").copied().unwrap_or(1) as u128;
            let width = symbols.get("width").copied().unwrap_or(1) as u128;
            let channels = config
                .get("channels")
                .and_then(|value| value.as_u64())
                .context("typed ResNet channels missing")? as u128;
            ensure!(
                batch as u64
                    <= config["max_batch"]
                        .as_u64()
                        .context("typed ResNet batch limit missing")?
                    && actions as u64
                        <= config["max_candidates"]
                            .as_u64()
                            .context("typed ResNet candidate limit missing")?
                    && height
                        <= config["max_board_axis"]
                            .as_u64()
                            .context("typed ResNet board limit missing")?
                            as u128
                    && width
                        <= config["max_board_axis"]
                            .as_u64()
                            .context("typed ResNet board limit missing")?
                            as u128,
                "typed input exceeds model geometry limits"
            );
            typed_work + 6u128 * batch as u128 * channels * height * width * 4
        };
        ensure!(
            total_bytes as u128 + intermediate
                <= self.bundle.resource_limits.max_intermediate_bytes as u128,
            "typed inference intermediate buffer estimate exceeds limit"
        );
        Ok((batch, actions, symbols))
    }

    pub fn validate_inputs(
        &self,
        inputs: &[NamedInput],
    ) -> Result<(usize, usize, BTreeMap<String, usize>)> {
        let shapes = inputs
            .iter()
            .map(|input| TypedInputShape {
                name: &input.name,
                dtype: input.data.dtype(),
                shape: &input.shape,
            })
            .collect::<Vec<_>>();
        let (batch, actions, symbols) = self.validate_shapes(&shapes)?;
        for input in inputs {
            let elements = input
                .shape
                .iter()
                .try_fold(1usize, |count, dim| count.checked_mul(*dim))
                .context("typed shape overflow")?;
            ensure!(
                elements == input.data.len(),
                "typed input element count mismatch"
            );
            if let TypedTensorData::Float32(data) = &input.data {
                ensure!(
                    data.iter().all(|value| value.is_finite()),
                    "typed input non-finite float32 value"
                );
            }
        }
        validate_typed_semantics(
            inputs,
            &symbols,
            self.bundle.encoder["feature_schema"]["category_vocabulary"]
                .as_array()
                .context("typed category vocabulary missing")?
                .len(),
        )?;
        Ok((batch, actions, symbols))
    }

    pub fn evaluate(&mut self, inputs: Vec<NamedInput>) -> Result<Output> {
        let (batch, actions, _symbols) = self.validate_inputs(&inputs)?;
        let outputs = match &mut self.runner {
            Runner::Ort(session) => {
                let mut named = Vec::<(String, SessionInputValue<'_>)>::with_capacity(inputs.len());
                for input in inputs {
                    let value: SessionInputValue<'_> = match input.data {
                        TypedTensorData::Float32(data) => {
                            OrtTensor::from_array((input.shape, data))?.into()
                        }
                        TypedTensorData::Int64(data) => {
                            OrtTensor::from_array((input.shape, data))?.into()
                        }
                        TypedTensorData::Bool(data) => {
                            OrtTensor::from_array((input.shape, data))?.into()
                        }
                    };
                    named.push((input.name, value));
                }
                let output = session.run(named)?;
                let (policy_shape, policy) = output["policy_logits"].try_extract_tensor::<f32>()?;
                let (value_shape, value) = output["value"].try_extract_tensor::<f32>()?;
                [
                    Input::new(
                        policy_shape.iter().map(|v| *v as usize).collect(),
                        policy.to_vec(),
                    )?,
                    Input::new(
                        value_shape.iter().map(|v| *v as usize).collect(),
                        value.to_vec(),
                    )?,
                ]
            }
            Runner::Tract(runnable) => {
                let tensors = inputs
                    .iter()
                    .map(|input| {
                        let (dtype, bytes): (DatumType, Vec<u8>) = match &input.data {
                            TypedTensorData::Float32(data) => (
                                DatumType::F32,
                                data.iter().flat_map(|v| v.to_ne_bytes()).collect(),
                            ),
                            TypedTensorData::Int64(data) => (
                                DatumType::I64,
                                data.iter().flat_map(|v| v.to_ne_bytes()).collect(),
                            ),
                            TypedTensorData::Bool(data) => {
                                (DatumType::Bool, data.iter().map(|v| u8::from(*v)).collect())
                            }
                        };
                        tract::Tensor::from_bytes(dtype, &input.shape, &bytes)
                    })
                    .collect::<Result<Vec<_>>>()?;
                let output = runnable.run(tensors)?;
                ensure!(output.len() == 2, "tract typed output count mismatch");
                let mut converted = Vec::with_capacity(2);
                for value in &output {
                    let (dtype, shape, bytes) = value.as_bytes()?;
                    ensure!(
                        dtype == DatumType::F32 && bytes.len().is_multiple_of(4),
                        "tract typed output dtype mismatch"
                    );
                    converted.push(Input::new(
                        shape.to_vec(),
                        bytes
                            .chunks_exact(4)
                            .map(|b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
                            .collect(),
                    )?);
                }
                [converted.remove(0), converted.remove(0)]
            }
            Runner::TractTyped(bytes) => {
                let mut inference = tract::onnx()?.load_buffer(bytes)?;
                for (index, input) in inputs.iter().enumerate() {
                    let dtype = match input.data {
                        TypedTensorData::Float32(_) => "f32",
                        TypedTensorData::Int64(_) => "i64",
                        TypedTensorData::Bool(_) => "bool",
                    };
                    let fact = format!(
                        "{},{}",
                        input
                            .shape
                            .iter()
                            .map(usize::to_string)
                            .collect::<Vec<_>>()
                            .join(","),
                        dtype
                    );
                    inference.set_input_fact(index, fact.as_str())?;
                }
                let model = inference.into_model()?;
                let runnable = tract::runtime_for_name("default")?.prepare(model)?;
                let tensors = inputs
                    .iter()
                    .map(|input| {
                        let (dtype, data): (DatumType, Vec<u8>) = match &input.data {
                            TypedTensorData::Float32(values) => (
                                DatumType::F32,
                                values.iter().flat_map(|v| v.to_ne_bytes()).collect(),
                            ),
                            TypedTensorData::Int64(values) => (
                                DatumType::I64,
                                values.iter().flat_map(|v| v.to_ne_bytes()).collect(),
                            ),
                            TypedTensorData::Bool(values) => (
                                DatumType::Bool,
                                values.iter().map(|v| u8::from(*v)).collect(),
                            ),
                        };
                        tract::Tensor::from_bytes(dtype, &input.shape, &data)
                    })
                    .collect::<Result<Vec<_>>>()?;
                let output = runnable.run(tensors)?;
                ensure!(output.len() == 2, "tract typed output count mismatch");
                let mut converted = Vec::with_capacity(2);
                for value in &output {
                    let (dtype, shape, data) = value.as_bytes()?;
                    ensure!(
                        dtype == DatumType::F32 && data.len().is_multiple_of(4),
                        "tract typed output dtype mismatch"
                    );
                    converted.push(Input::new(
                        shape.to_vec(),
                        data.chunks_exact(4)
                            .map(|v| f32::from_ne_bytes([v[0], v[1], v[2], v[3]]))
                            .collect(),
                    )?);
                }
                [converted.remove(0), converted.remove(0)]
            }
        };
        let [policy_logits, value] = outputs;
        ensure!(
            policy_logits.shape == [batch, actions] && value.shape == [batch, 1],
            "typed runtime output shape mismatch"
        );
        ensure!(
            policy_logits.data.iter().all(|value| value.is_finite()),
            "typed runtime policy logits must be finite"
        );
        ensure!(
            value.data.iter().all(|v| (-1.00001..=1.00001).contains(v)),
            "typed runtime value outside [-1,1]"
        );
        Ok(Output {
            policy_logits,
            value,
        })
    }
}

fn bool_input<'a>(inputs: &'a [NamedInput], name: &str) -> Result<&'a [bool]> {
    match &inputs
        .iter()
        .find(|input| input.name == name)
        .context("typed bool input missing")?
        .data
    {
        TypedTensorData::Bool(values) => Ok(values),
        _ => anyhow::bail!("typed input {name} must be bool"),
    }
}

fn int_input<'a>(inputs: &'a [NamedInput], name: &str) -> Result<&'a [i64]> {
    match &inputs
        .iter()
        .find(|input| input.name == name)
        .context("typed int64 input missing")?
        .data
    {
        TypedTensorData::Int64(values) => Ok(values),
        _ => anyhow::bail!("typed input {name} must be int64"),
    }
}

fn validate_typed_semantics(
    inputs: &[NamedInput],
    symbols: &BTreeMap<String, usize>,
    vocabulary: usize,
) -> Result<()> {
    let batch = symbols["batch"];
    let records = symbols["records"];
    let relations = symbols["relations"];
    let actions = symbols["actions"];
    let nodes = symbols["nodes"];
    let record_mask = bool_input(inputs, "record_mask")?;
    let record_spatial_valid = bool_input(inputs, "record_spatial_valid")?;
    let relation_mask = bool_input(inputs, "relation_mask")?;
    let candidate_mask = bool_input(inputs, "candidate_mask")?;
    let candidate_node_mask = bool_input(inputs, "candidate_node_mask")?;
    let candidate_coord_valid = bool_input(inputs, "candidate_coord_valid")?;
    let relation_index = int_input(inputs, "relation_index")?;
    let parent = int_input(inputs, "candidate_parent")?;
    let order = int_input(inputs, "candidate_order")?;
    let target = int_input(inputs, "candidate_target_index")?;
    for b in 0..batch {
        ensure!(
            record_mask[b * records],
            "typed global record must be active"
        );
        for r in 0..records {
            let at = b * records + r;
            ensure!(
                !record_spatial_valid[at] || record_mask[at],
                "typed spatial record is padded"
            );
        }
        for r in 0..relations {
            if relation_mask[b * relations + r] {
                for edge in 0..2 {
                    let endpoint = relation_index[(b * relations + r) * 2 + edge];
                    ensure!(
                        endpoint >= 0
                            && (endpoint as usize) < records
                            && record_mask[b * records + endpoint as usize],
                        "typed relation endpoint is invalid"
                    );
                }
            }
        }
        for action in 0..actions {
            let candidate_at = b * actions + action;
            let root_at = candidate_at * nodes;
            ensure!(
                !candidate_mask[candidate_at] || candidate_node_mask[root_at],
                "typed active candidate has no root"
            );
            for node in 0..nodes {
                let at = root_at + node;
                ensure!(
                    !candidate_node_mask[at] || candidate_mask[candidate_at],
                    "typed node belongs to a padded candidate"
                );
                ensure!(
                    !candidate_coord_valid[at] || candidate_node_mask[at],
                    "typed coordinate belongs to a padded node"
                );
                if !candidate_node_mask[at] {
                    continue;
                }
                ensure!(
                    (-1..=4096).contains(&order[at]),
                    "typed candidate order is invalid"
                );
                let target_record = target[at];
                ensure!(
                    target_record >= -1
                        && (target_record < 0
                            || (target_record as usize) < records
                                && record_mask[b * records + target_record as usize]),
                    "typed candidate target is invalid"
                );
                if node == 0 {
                    ensure!(parent[at] == -1, "typed candidate root parent must be -1");
                } else {
                    ensure!(
                        parent[at] >= 0
                            && (parent[at] as usize) < node
                            && candidate_node_mask[root_at + parent[at] as usize],
                        "typed candidate parent is invalid"
                    );
                }
            }
        }
    }
    for (name, mask, width) in [
        ("record_category", record_mask, 4usize),
        ("relation_category", relation_mask, 2usize),
        ("candidate_category", candidate_node_mask, 4usize),
    ] {
        let categories = int_input(inputs, name)?;
        for (at, active) in mask.iter().enumerate() {
            if *active {
                ensure!(
                    categories[at * width..(at + 1) * width]
                        .iter()
                        .all(|value| *value >= 0 && (*value as usize) < vocabulary),
                    "typed category is outside the versioned vocabulary"
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use manifest::{TensorSpec, TypedOnnxContract, TypedResourceLimits};
    use serde_json::json;

    #[test]
    fn mixed_dtype_element_limit_is_independent_of_byte_limit() {
        let bundle = TypedBundle {
            version: String::new(),
            model_io_version: String::new(),
            architecture_family: "entity-transformer".into(),
            model_file: String::new(),
            model_sha256: String::new(),
            base_hash: String::new(),
            adapter_hash: None,
            adapter: None,
            model_config: json!({"typed": {"hidden_dim": 1}, "heads": 1}),
            model_config_hash: String::new(),
            encoder: json!({"feature_schema": {"limits": {
                "max_input_bytes": 1024, "max_batch": 64,
                "max_candidates": 4096, "max_relations": 8192
            }}}),
            encoder_hash: String::new(),
            onnx: TypedOnnxContract {
                opset: 18,
                inputs: vec![
                    TensorSpec {
                        name: "candidate_mask".into(),
                        dtype: TensorDtype::Bool,
                        shape: vec![
                            Axis::Dynamic("batch".into()),
                            Axis::Dynamic("actions".into()),
                        ],
                    },
                    TensorSpec {
                        name: "relation_index".into(),
                        dtype: TensorDtype::Int64,
                        shape: vec![
                            Axis::Dynamic("batch".into()),
                            Axis::Dynamic("relations".into()),
                            Axis::Fixed(2),
                        ],
                    },
                ],
                outputs: Vec::new(),
            },
            resource_limits: TypedResourceLimits {
                axis_maxima: [
                    ("batch".into(), 64),
                    ("actions".into(), 4096),
                    ("relations".into(), 8192),
                ]
                .into(),
                max_input_bytes: 1024,
                max_intermediate_bytes: 1024 * 1024,
            },
            numerical_tolerance: json!({}),
        };
        let mut session = TypedInferenceSession {
            bundle,
            limits: Limits {
                max_input_elements: 65,
                ..Limits::default()
            },
            backend: Backend::Tract,
            runner: Runner::TractTyped(Vec::new()),
        };
        let inputs = [
            TypedInputShape {
                name: "candidate_mask",
                dtype: TensorDtype::Bool,
                shape: &[1, 64],
            },
            TypedInputShape {
                name: "relation_index",
                dtype: TensorDtype::Int64,
                shape: &[1, 1, 2],
            },
        ];
        // 66 elements use only 80 bytes, below both byte ceilings. The
        // bool-heavy request still exceeds an element limit of 65.
        assert!(
            session
                .validate_shapes(&inputs)
                .unwrap_err()
                .to_string()
                .contains("element count")
        );
        session.limits.max_input_elements = 66;
        assert!(session.validate_shapes(&inputs).is_ok());
        session.bundle.resource_limits.max_input_bytes = 79;
        assert!(
            session
                .validate_shapes(&inputs)
                .unwrap_err()
                .to_string()
                .contains("input bytes")
        );

        // Here 65 elements require 513 bytes because 64 are int64. The
        // element limit is satisfied and the explicit byte budget admits it.
        let int64_heavy = [
            TypedInputShape {
                name: "candidate_mask",
                dtype: TensorDtype::Bool,
                shape: &[1, 1],
            },
            TypedInputShape {
                name: "relation_index",
                dtype: TensorDtype::Int64,
                shape: &[1, 32, 2],
            },
        ];
        session.limits.max_input_elements = 65;
        session.bundle.resource_limits.max_input_bytes = 1024;
        assert!(session.validate_shapes(&int64_heavy).is_ok());
        session.bundle.resource_limits.max_input_bytes = 512;
        assert!(
            session
                .validate_shapes(&int64_heavy)
                .unwrap_err()
                .to_string()
                .contains("input bytes")
        );
    }
}
