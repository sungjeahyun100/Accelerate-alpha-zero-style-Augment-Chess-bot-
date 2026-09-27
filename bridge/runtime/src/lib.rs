//! Explicit CPU backends behind a validated common ONNX artifact contract.
mod manifest;
use anyhow::{Context, Result, bail, ensure};
pub use manifest::{Bundle, ModelConfig};
use ort::{session::Session, value::Tensor as OrtTensor};
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
        let runner = match backend {
            Backend::Ort => Runner::Ort(
                Session::builder()?
                    .with_intra_threads(limits.threads)
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .with_inter_threads(1)
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .commit_from_memory(&bytes)?,
            ),
            Backend::Tract => {
                ensure!(
                    limits.threads == 1,
                    "tract CPU runtime currently requires threads=1"
                );
                let model = tract::onnx()?.load_buffer(&bytes)?.into_model()?;
                Runner::Tract(tract::runtime_for_name("default")?.prepare(model)?)
            }
        };
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
