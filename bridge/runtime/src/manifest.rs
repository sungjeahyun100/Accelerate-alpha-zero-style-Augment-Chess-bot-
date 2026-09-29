//! Artifact metadata and ONNX boundary validation, independent of a backend.
use anyhow::{Context, Result, ensure};
use prost::Message;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::Path,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    pub board_channels: usize,
    pub condition_dim: usize,
    pub action_dim: usize,
    pub channels: usize,
    pub residual_blocks: usize,
    pub lora_rank: usize,
    pub lora_alpha: f64,
    pub lora_dropout: f64,
    pub architecture_version: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub version: String,
    pub model_file: String,
    pub model_sha256: String,
    pub base_hash: String,
    pub adapter_hash: Option<String>,
    pub adapter: Option<Value>,
    pub model_config: ModelConfig,
    pub model_config_hash: String,
    pub encoder: Value,
    pub encoder_hash: String,
    pub onnx: Value,
    pub numerical_tolerance: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TensorDtype {
    Float32,
    Int64,
    Bool,
}
impl TensorDtype {
    pub fn onnx_id(&self) -> i32 {
        match self {
            Self::Float32 => 1,
            Self::Int64 => 7,
            Self::Bool => 9,
        }
    }
    pub fn element_bytes(&self) -> usize {
        match self {
            Self::Float32 => 4,
            Self::Int64 => 8,
            Self::Bool => 1,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Axis {
    Fixed(usize),
    Dynamic(String),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TensorSpec {
    pub name: String,
    pub dtype: TensorDtype,
    pub shape: Vec<Axis>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypedOnnxContract {
    pub opset: u32,
    pub inputs: Vec<TensorSpec>,
    pub outputs: Vec<TensorSpec>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypedResourceLimits {
    pub axis_maxima: BTreeMap<String, usize>,
    pub max_input_bytes: usize,
    pub max_intermediate_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypedBundle {
    pub version: String,
    pub model_io_version: String,
    pub architecture_family: String,
    pub model_file: String,
    pub model_sha256: String,
    pub base_hash: String,
    pub adapter_hash: Option<String>,
    pub adapter: Option<Value>,
    pub model_config: Value,
    pub model_config_hash: String,
    pub encoder: Value,
    pub encoder_hash: String,
    pub onnx: TypedOnnxContract,
    pub resource_limits: TypedResourceLimits,
    pub numerical_tolerance: Value,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn canonical_hash(value: &Value) -> Result<String> {
    Ok(sha(&serde_jcs::to_vec(value)?))
}
fn json_numbers(value: &Value) -> Result<()> {
    match value {
        Value::Number(number) => {
            let number = number.as_f64().context("JSON number not representable")?;
            ensure!(
                number.is_finite()
                    && !(number.fract() == 0. && number.abs() > 9_007_199_254_740_991.),
                "JSON numbers must be finite and integral values must fit the exact IEEE-754 range"
            );
        }
        Value::Array(values) => {
            for value in values {
                json_numbers(value)?;
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                json_numbers(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn read(path: &Path, max: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .with_context(|| format!("opening {}", path.display()))?
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= max, "artifact exceeds byte limit");
    Ok(bytes)
}
fn fields(value: &Value, expected: &[&str]) -> Result<()> {
    let object = value.as_object().context("expected object")?;
    ensure!(
        object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key)),
        "unexpected artifact contract fields"
    );
    Ok(())
}
fn capacity(value: &Value, name: &str) -> Result<usize> {
    let number = value[name].as_u64().context("expected capacity integer")?;
    ensure!(
        (1..=1_048_576).contains(&number),
        "invalid feature capacity"
    );
    Ok(number as usize)
}
fn catalog_ids(catalog: &Value, key: &str) -> Result<Vec<String>> {
    let mut ids = catalog[key]
        .as_array()
        .context("catalog IDs missing")?
        .iter()
        .map(|v| {
            v.as_str()
                .context("catalog ID not string")
                .map(str::to_owned)
        })
        .collect::<Result<Vec<_>>>()?;
    ids.sort();
    Ok(ids)
}

fn frozen_baseline(rules_version: &str) -> Result<(Value, Value, &'static str)> {
    let (catalog, policy, projection) = match rules_version {
        "augment-site-20260927-abfe01a035813875" => (
            include_str!("../../catalog/site-20260927.json"),
            include_str!("../../catalog/observation-20260927.json"),
            "source-visible-20260927-v3",
        ),
        "augment-site-20260928-e5ed84fcf8e72a24" => (
            include_str!("../../catalog/site-20260928.json"),
            include_str!("../../catalog/observation-20260928.json"),
            "source-visible-20260928-v1",
        ),
        _ => anyhow::bail!("unsupported frozen rules version"),
    };
    Ok((
        serde_json::from_str(catalog)?,
        serde_json::from_str(policy)?,
        projection,
    ))
}

impl Bundle {
    pub fn load(path: &Path, expected_encoder_hash: Option<&str>) -> Result<(Self, Vec<u8>)> {
        ensure!(
            path.file_name().is_some_and(|name| name == "manifest.json"),
            "artifact path must name manifest.json"
        );
        let raw: Value = serde_json::from_slice(&read(path, 2 * 1024 * 1024)?)?;
        json_numbers(&raw)?;
        let bundle: Self = serde_json::from_value(raw.clone())?;
        ensure!(
            bundle.version == "onnx-policy-value-v2" && bundle.model_file == "model.onnx",
            "unsupported deployment manifest"
        );
        ensure!(
            hash(&bundle.model_sha256)
                && hash(&bundle.base_hash)
                && hash(&bundle.model_config_hash)
                && hash(&bundle.encoder_hash),
            "invalid artifact hash"
        );
        ensure!(
            expected_encoder_hash.is_none_or(|expected| expected == bundle.encoder_hash),
            "expected encoder compatibility mismatch"
        );
        ensure!(
            canonical_hash(&raw["model_config"])? == bundle.model_config_hash
                && canonical_hash(&bundle.encoder)? == bundle.encoder_hash,
            "model/encoder contract hash mismatch"
        );
        let config = &bundle.model_config;
        ensure!(
            (1..=1_048_576).contains(&config.board_channels)
                && (1..=1_048_576).contains(&config.condition_dim)
                && (1..=1_048_576).contains(&config.action_dim)
                && (1..=4096).contains(&config.channels)
                && (1..=64).contains(&config.residual_blocks)
                && (1..=4096).contains(&config.lora_rank)
                && config.lora_alpha.is_finite()
                && config.lora_alpha > 0.
                && config.lora_dropout == 0.
                && config.architecture_version == "resnet-film-action-v1",
            "unsupported model architecture"
        );
        // Bound the combined architecture before a backend allocates weights.
        // Include the separate LoRA tensors even though deployment merges them.
        let c = config.channels as u128;
        let r = config.lora_rank as u128;
        let parameters = c * config.board_channels as u128 * 9
            + c * config.condition_dim as u128
            + c * config.action_dim as u128
            + config.residual_blocks as u128 * (20 * c * c + 20 * r * c + 8 * c)
            + 4 * c * c
            + 8 * c;
        ensure!(
            parameters <= 64_000_000,
            "model aggregate parameter budget exceeds 64 million"
        );
        let spec = &bundle.encoder["spec"];
        fields(
            spec,
            &[
                "rules_version",
                "catalog_hash",
                "piece_ids",
                "card_ids",
                "rule_ids",
                "piece_payload_bytes",
                "public_payload_bytes",
                "action_payload_bytes",
                "encoder_version",
                "action_version",
                "condition_version",
                "action_types",
                "catalog_version",
                "history_encoding",
                "action_encoding",
                "observation_policy_hash",
            ],
        )?;
        let (catalog, policy, projection) = frozen_baseline(
            spec["rules_version"]
                .as_str()
                .context("encoder rules version missing")?,
        )?;
        ensure!(
            spec["rules_version"] == catalog["rulesVersion"]
                && spec["catalog_version"] == catalog["catalogVersion"]
                && spec["catalog_hash"].as_str() == Some(&canonical_hash(&catalog)?)
                && spec["observation_policy_hash"].as_str() == Some(&canonical_hash(&policy)?)
                && policy["schemaVersion"] == 2
                && policy["protocolVersion"] == "accelerate-observation-v2"
                && policy["projectionVersion"] == projection
                && policy["rulesVersion"] == catalog["rulesVersion"],
            "frozen rules/catalog compatibility mismatch"
        );
        ensure!(
            spec["piece_ids"] == json!(catalog_ids(&catalog, "pieceTypes")?)
                && spec["action_types"] == catalog["actionTypes"],
            "piece/action catalog ordering mismatch"
        );
        let cards = catalog["cards"]
            .as_array()
            .context("card catalog missing")?;
        let mut card_ids = cards
            .iter()
            .map(|v| {
                v["id"]
                    .as_str()
                    .context("card ID missing")
                    .map(str::to_owned)
            })
            .collect::<Result<Vec<_>>>()?;
        card_ids.sort();
        let mut rule_ids = cards
            .iter()
            .filter(|v| v["draftCategory"] == "RULE")
            .map(|v| {
                v["id"]
                    .as_str()
                    .context("rule ID missing")
                    .map(str::to_owned)
            })
            .collect::<Result<Vec<_>>>()?;
        rule_ids.sort();
        ensure!(
            spec["card_ids"] == json!(card_ids) && spec["rule_ids"] == json!(rule_ids),
            "card/rule catalog ordering mismatch"
        );
        ensure!(
            spec["encoder_version"] == "public-utf8-v2"
                && spec["action_version"] == "candidate-payload-v1"
                && spec["condition_version"] == "public-film-v2",
            "unsupported encoder version"
        );
        let board_channels =
            catalog_ids(&catalog, "pieceTypes")?.len() + 5 + capacity(spec, "piece_payload_bytes")?;
        let condition_dim =
            2 * cards.len() + rule_ids.len() + 5 + capacity(spec, "public_payload_bytes")?;
        let action_dim = catalog["actionTypes"]
            .as_array()
            .context("action types missing")?
            .len()
            + cards.len()
            + 7
            + capacity(spec, "action_payload_bytes")?;
        ensure!(
            (
                config.board_channels,
                config.condition_dim,
                config.action_dim
            ) == (board_channels, condition_dim, action_dim),
            "model/encoder feature dimensions mismatch"
        );
        let history_policy = match spec["history_encoding"].as_str() {
            Some("full") => {
                json!({"mode":"full","version":"public-history-full-v1","recent_events":0,"summary_fields":[],"full_history_owner":"tracker-and-replay"})
            }
            Some("public-history-summary-v1") => {
                json!({"mode":"public-history-summary-v1","version":"public-history-summary-v1","recent_events":8,"summary_fields":["event_count","history_hash","actor_counts","decision_actor_changes","board_change_count","recent_events"],"full_history_owner":"tracker-and-replay"})
            }
            _ => anyhow::bail!("unsupported public history encoding"),
        };
        let action_policy = match spec["action_encoding"].as_str() {
            Some("exact-payload") => {
                json!({"mode":"exact-payload","selection_identity":"canonical-semantic-payload","execution_payload":"lossless"})
            }
            Some("public-decision-intent-v1") => {
                json!({"mode":"public-decision-intent-v1","selection_identity":"source-ui-choice","execution_payload":"native-only"})
            }
            _ => anyhow::bail!("unsupported candidate action encoding"),
        };
        let expected_encoder = json!({
            "spec": spec, "observation_policy": policy, "observation_version": "accelerate-observation-v2", "dtype": "float32", "board_layout": "NCHW", "value_perspective": "observation.viewer",
            "history_policy": history_policy,
            "action_policy": action_policy,
            "board_fields": ["piece-id-onehot", "own", "opponent", "own-known-moved", "occupied", "canonical-json-byte-length/capacity", "canonical-json-utf8-bytes/255"],
            "condition_fields": ["own-card-id-counts", "revealed-opponent-card-id-counts", "rule-id-presence", "viewer-is-white", "actionsRemaining/16", "moveCount/512", "fullMove/256", "canonical-json-byte-length/capacity", "canonical-json-utf8-bytes/255"],
            "action_fields": ["action-type-onehot", "card-id-onehot", "from-row/7", "from-col/7", "to-row/7", "to-col/7", "target-row/7", "target-col/7", "canonical-json-byte-length/capacity", "canonical-json-utf8-bytes/255"],
            "board_channels": board_channels, "condition_dim": condition_dim, "action_dim": action_dim,
            "coordinate_orientation": "viewer-black rotates both axes; JSON tails retain absolute site coordinates"
        });
        ensure!(
            bundle.encoder == expected_encoder,
            "unsupported encoder semantics"
        );
        ensure!(
            bundle.onnx
                == json!({"opset":18,"dtype":"float32","inputs":["board","condition","action_features"],"outputs":["policy_logits","value"],"dynamic_axes":{"batch":["board:0","condition:0","action_features:0","policy_logits:0","value:0"],"actions":["action_features:1","policy_logits:1"]}})
                && bundle.numerical_tolerance == json!({"atol":1e-5,"rtol":1e-4}),
            "unsupported ONNX deployment contract"
        );
        match (&bundle.adapter, &bundle.adapter_hash) {
            (None, None) => {}
            (Some(adapter), Some(adapter_hash)) => {
                fields(
                    adapter,
                    &[
                        "base_hash",
                        "config_hash",
                        "encoder_hash",
                        "generator",
                        "condition_lifetime",
                        "mergeable",
                        "version",
                    ],
                )?;
                ensure!(
                    hash(adapter_hash)
                        && adapter["base_hash"] == bundle.base_hash
                        && adapter["config_hash"] == bundle.model_config_hash
                        && adapter["encoder_hash"] == bundle.encoder_hash
                        && adapter["generator"] == "static-lora"
                        && adapter["condition_lifetime"] == "global"
                        && adapter["mergeable"] == true
                        && adapter["version"] == "lora-convolution-v1",
                    "static adapter compatibility mismatch"
                );
            }
            _ => anyhow::bail!("adapter metadata/hash mismatch"),
        }
        let bytes = read(
            &path
                .parent()
                .context("manifest parent missing")?
                .join("model.onnx"),
            512 * 1024 * 1024,
        )?;
        ensure!(
            sha(&bytes) == bundle.model_sha256,
            "ONNX model file hash mismatch"
        );
        validate_graph(&bytes, config)?;
        Ok((bundle, bytes))
    }
}

impl TypedBundle {
    pub fn load(path: &Path, expected_encoder_hash: Option<&str>) -> Result<(Self, Vec<u8>)> {
        ensure!(
            path.file_name().is_some_and(|name| name == "manifest.json"),
            "artifact path must name manifest.json"
        );
        let raw: Value = serde_json::from_slice(&read(path, 2 * 1024 * 1024)?)?;
        json_numbers(&raw)?;
        let bundle: Self = serde_json::from_value(raw.clone())?;
        ensure!(
            bundle.version == "onnx-policy-value-v3"
                && bundle.model_io_version == "typed-policy-value-v1"
                && bundle.model_file == "model.onnx",
            "unsupported typed deployment manifest"
        );
        ensure!(
            ["mask-resnet", "entity-transformer"].contains(&bundle.architecture_family.as_str()),
            "unsupported architecture family"
        );
        ensure!(
            [
                &bundle.model_sha256,
                &bundle.base_hash,
                &bundle.model_config_hash,
                &bundle.encoder_hash,
            ]
            .iter()
            .all(|value| hash(value)),
            "invalid typed artifact hash"
        );
        ensure!(
            expected_encoder_hash.is_none_or(|expected| expected == bundle.encoder_hash),
            "expected encoder compatibility mismatch"
        );
        ensure!(
            canonical_hash(&bundle.model_config)? == bundle.model_config_hash
                && canonical_hash(&bundle.encoder)? == bundle.encoder_hash,
            "model/encoder contract hash mismatch"
        );
        let config = bundle
            .model_config
            .as_object()
            .context("model config must be an object")?;
        let architecture_version = config
            .get("architecture_version")
            .and_then(Value::as_str)
            .context("architecture version missing")?;
        ensure!(
            match bundle.architecture_family.as_str() {
                "mask-resnet" => architecture_version == "mask-resnet-v2",
                "entity-transformer" => architecture_version == "entity-transformer-film-lora-v1",
                _ => false,
            },
            "model architecture version/family mismatch"
        );
        let encoder = bundle
            .encoder
            .as_object()
            .context("encoder contract must be an object")?;
        fields(
            &bundle.encoder,
            &[
                "rules_version",
                "catalog_version",
                "catalog_hash",
                "observation_policy_hash",
                "observation_version",
                "ir_version",
                "descriptor_version",
                "encoder_version",
                "feature_schema",
                "feature_schema_hash",
                "value_perspective",
            ],
        )?;
        ensure!(
            encoder["value_perspective"] == "observation.viewer"
                && ["accelerate-observation-v2", "synthetic-geometry-v1"]
                    .contains(&encoder["observation_version"].as_str().unwrap_or(""))
                && encoder["ir_version"] == "semantic-ir-v1"
                && encoder["descriptor_version"] == "move-program-v1"
                && encoder["encoder_version"] == "typed-input-v1"
                && hash(encoder["feature_schema_hash"].as_str().unwrap_or(""))
                && canonical_hash(&encoder["feature_schema"])? == encoder["feature_schema_hash"],
            "unsupported typed encoder semantics"
        );
        let (catalog, policy, projection) = frozen_baseline(
            encoder["rules_version"]
                .as_str()
                .context("rules version missing")?,
        )?;
        ensure!(
            encoder["rules_version"] == "augment-site-20260928-e5ed84fcf8e72a24"
                && encoder["catalog_version"] == catalog["catalogVersion"]
                && encoder["catalog_hash"].as_str() == Some(&canonical_hash(&catalog)?)
                && encoder["observation_policy_hash"].as_str() == Some(&canonical_hash(&policy)?)
                && policy["projectionVersion"] == projection,
            "frozen v7 rules/catalog compatibility mismatch"
        );
        let limits = &bundle.resource_limits;
        ensure!(
            (1..=64 * 1024 * 1024).contains(&limits.max_input_bytes)
                && (1..=256 * 1024 * 1024).contains(&limits.max_intermediate_bytes),
            "invalid typed resource byte limits"
        );
        ensure!(
            limits.axis_maxima.len() == 7
                && [
                    ("batch", 64),
                    ("actions", 4096),
                    ("records", 2048),
                    ("relations", 8192),
                    ("nodes", 64),
                    ("height", 32),
                    ("width", 32),
                ]
                .iter()
                .all(|(name, maximum)| limits
                    .axis_maxima
                    .get(*name)
                    .is_some_and(|value| (1..=*maximum).contains(value))),
            "invalid typed resource axis limits"
        );
        ensure!(bundle.onnx.opset == 18, "ONNX requires opset18");
        validate_typed_specs(&bundle.onnx, limits)?;
        validate_typed_schema(
            &encoder["feature_schema"],
            &bundle.architecture_family,
            &bundle.onnx,
            limits,
        )?;
        ensure!(
            bundle.numerical_tolerance == json!({"atol":1e-5,"rtol":1e-4}),
            "unsupported numerical tolerance"
        );
        match (&bundle.adapter, &bundle.adapter_hash) {
            (None, None) => {}
            (Some(adapter), Some(adapter_hash)) => {
                ensure!(
                    hash(adapter_hash)
                        && adapter["base_hash"] == bundle.base_hash
                        && adapter["config_hash"] == bundle.model_config_hash
                        && adapter["encoder_hash"] == bundle.encoder_hash
                        && adapter["generator"] == "static-lora"
                        && adapter["mergeable"] == true,
                    "typed adapter compatibility mismatch"
                );
            }
            _ => anyhow::bail!("typed adapter metadata/hash mismatch"),
        }
        let bytes = read(
            &path
                .parent()
                .context("manifest parent missing")?
                .join("model.onnx"),
            512 * 1024 * 1024,
        )?;
        ensure!(
            sha(&bytes) == bundle.model_sha256,
            "ONNX model file hash mismatch"
        );
        validate_typed_graph(&bytes, &bundle.onnx)?;
        Ok((bundle, bytes))
    }
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}

fn validate_typed_specs(contract: &TypedOnnxContract, limits: &TypedResourceLimits) -> Result<()> {
    ensure!(
        !contract.inputs.is_empty() && contract.inputs.len() <= 64,
        "invalid typed input count"
    );
    ensure!(
        contract.outputs.len() == 2
            && contract.outputs[0].name == "policy_logits"
            && contract.outputs[1].name == "value"
            && contract
                .outputs
                .iter()
                .all(|spec| spec.dtype == TensorDtype::Float32)
            && contract.outputs[0].shape
                == [
                    Axis::Dynamic("batch".into()),
                    Axis::Dynamic("actions".into())
                ]
            && contract.outputs[1].shape == [Axis::Dynamic("batch".into()), Axis::Fixed(1)],
        "typed output contract must be float32 [batch,actions] and [batch,1]"
    );
    let mut names = BTreeSet::new();
    for spec in contract.inputs.iter().chain(&contract.outputs) {
        ensure!(
            valid_name(&spec.name) && names.insert(&spec.name),
            "invalid or duplicate tensor name"
        );
        ensure!((1..=6).contains(&spec.shape.len()), "invalid tensor rank");
        ensure!(
            spec.shape.first() == Some(&Axis::Dynamic("batch".into())),
            "tensor batch axis mismatch"
        );
        for axis in &spec.shape {
            match axis {
                Axis::Fixed(value) => ensure!(
                    (1..=65_536).contains(value),
                    "invalid fixed tensor dimension"
                ),
                Axis::Dynamic(name) => {
                    ensure!(
                        valid_name(name) && limits.axis_maxima.contains_key(name),
                        "unbounded dynamic tensor axis"
                    );
                }
            }
        }
    }
    ensure!(
        contract
            .inputs
            .iter()
            .any(|spec| spec.shape.contains(&Axis::Dynamic("actions".into()))),
        "typed candidate axis missing"
    );
    Ok(())
}

fn validate_typed_schema(
    schema: &Value,
    family: &str,
    contract: &TypedOnnxContract,
    limits: &TypedResourceLimits,
) -> Result<()> {
    const COMMON: [(&str, &str, &str); 19] = [
        ("record_category", "int64", "batch,records,4"),
        ("record_numeric", "float32", "batch,records,8"),
        ("record_coord", "float32", "batch,records,2"),
        ("record_spatial_valid", "bool", "batch,records"),
        ("record_mask", "bool", "batch,records"),
        ("relation_index", "int64", "batch,relations,2"),
        ("relation_category", "int64", "batch,relations,2"),
        ("relation_numeric", "float32", "batch,relations,4"),
        ("relation_mask", "bool", "batch,relations"),
        ("candidate_category", "int64", "batch,actions,nodes,4"),
        ("candidate_numeric", "float32", "batch,actions,nodes,8"),
        ("candidate_coord", "float32", "batch,actions,nodes,2"),
        ("candidate_coord_valid", "bool", "batch,actions,nodes"),
        ("candidate_parent", "int64", "batch,actions,nodes"),
        ("candidate_order", "int64", "batch,actions,nodes"),
        ("candidate_target_index", "int64", "batch,actions,nodes"),
        ("candidate_node_mask", "bool", "batch,actions,nodes"),
        ("candidate_mask", "bool", "batch,actions"),
        ("condition", "float32", "batch,8"),
    ];
    const SPATIAL: [(&str, &str, &str); 2] = [
        ("spatial", "float32", "batch,6,height,width"),
        ("layout_mask", "bool", "batch,1,height,width"),
    ];
    let fields = schema
        .as_object()
        .context("typed feature schema must be an object")?;
    ensure!(
        fields.len() == 11
            && [
                "inputs",
                "input_order",
                "category_vocabulary",
                "category_slots",
                "numeric_slots",
                "spatial_channels",
                "coordinate_frame",
                "history_version",
                "limits",
                "candidate_tree",
                "padding"
            ]
            .iter()
            .all(|name| fields.contains_key(*name)),
        "unsupported typed feature schema fields"
    );
    let inputs = schema["inputs"]
        .as_object()
        .context("typed feature inputs must be an object")?;
    ensure!(
        inputs.len() == 21,
        "typed feature schema input count mismatch"
    );
    let shape = |description: &str| -> Vec<Value> {
        description
            .split(',')
            .map(|axis| match axis.parse::<usize>() {
                Ok(size) => json!(size),
                Err(_) => json!(axis),
            })
            .collect()
    };
    for (name, dtype, dimensions) in SPATIAL.iter().chain(COMMON.iter()) {
        ensure!(
            inputs.get(*name) == Some(&json!({"dtype": dtype, "shape": shape(dimensions)})),
            "typed feature schema tensor {name} mismatch"
        );
    }
    let mut a_order: Vec<&str> = SPATIAL.iter().map(|(name, _, _)| *name).collect();
    let b_order: Vec<&str> = COMMON.iter().map(|(name, _, _)| *name).collect();
    a_order.extend(&b_order);
    ensure!(
        schema["input_order"] == json!({"mask-resnet": a_order, "entity-transformer": b_order}),
        "typed feature schema input order mismatch"
    );
    let expected: Vec<&(&str, &str, &str)> = if family == "mask-resnet" {
        SPATIAL.iter().chain(COMMON.iter()).collect()
    } else {
        COMMON.iter().collect()
    };
    ensure!(
        contract.inputs.len() == expected.len(),
        "typed ONNX input count/family mismatch"
    );
    for (spec, &(name, dtype, dimensions)) in contract.inputs.iter().zip(expected) {
        let actual_dtype = match spec.dtype {
            TensorDtype::Float32 => "float32",
            TensorDtype::Int64 => "int64",
            TensorDtype::Bool => "bool",
        };
        let actual_shape: Vec<Value> = spec
            .shape
            .iter()
            .map(|axis| match axis {
                Axis::Fixed(size) => json!(size),
                Axis::Dynamic(name) => json!(name),
            })
            .collect();
        ensure!(
            spec.name == name && actual_dtype == dtype && actual_shape == shape(dimensions),
            "typed ONNX tensor {name} does not match feature schema"
        );
    }
    let vocabulary = schema["category_vocabulary"]
        .as_array()
        .context("typed category vocabulary missing")?;
    ensure!(
        !vocabulary.is_empty() && vocabulary.len() <= 65_536 && vocabulary[0] == "",
        "invalid typed category vocabulary"
    );
    let mut unique = BTreeSet::new();
    for item in vocabulary {
        let label = item.as_str().context("typed category must be a string")?;
        ensure!(
            label.len() <= 128 && unique.insert(label),
            "invalid or duplicate typed category"
        );
    }
    ensure!(
        schema["category_slots"]
            == json!({"record": ["kind", "field", "symbol", "owner"],
            "relation": ["kind", "field"], "candidate": ["kind", "field", "symbol", "owner"]})
            && schema["numeric_slots"]
                == json!({
                "record": ["scalar", "has_scalar", "span_length", "ordinal", "row_start", "col_start", "row_end", "col_end"],
                "relation": ["ordinal", "has_ordinal", "delta_row", "delta_col"],
                "candidate": ["scalar", "has_scalar", "child_count", "depth", "row_start", "col_start", "row_end", "col_end"],
                "condition": ["viewer_white", "turn_is_viewer", "actions_remaining_16", "move_count_512", "full_move_256",
                    "opponent_hand_count_16", "own_card_count_32", "history_event_count_512"]})
            && schema["spatial_channels"]
                == json!(["empty", "unknown", "hole", "occupied", "own", "opponent"])
            && schema["coordinate_frame"]
                == "absolute public Coord mapped to local row/column in current geometry; no viewer rotation"
            && schema["history_version"] == "public-history-summary-v2"
            && schema["candidate_tree"]
                == "root index zero; child parent index, array order, public record target index or -1"
            && schema["padding"]
                == "layout_mask false only for batch padding; hole remains within layout",
        "unsupported typed feature semantics"
    );
    let schema_limits = schema["limits"]
        .as_object()
        .context("typed feature limits missing")?;
    ensure!(
        schema_limits.len() == 7,
        "typed feature limits fields mismatch"
    );
    for (feature, axis) in [
        ("max_batch", "batch"),
        ("max_records", "records"),
        ("max_relations", "relations"),
        ("max_candidates", "actions"),
        ("max_candidate_nodes", "nodes"),
    ] {
        let actual = schema_limits
            .get(feature)
            .and_then(Value::as_u64)
            .context("invalid typed feature limit")?;
        ensure!(
            (1..=*limits
                .axis_maxima
                .get(axis)
                .context("missing typed axis limit")? as u64)
                .contains(&actual),
            "typed feature limit {feature} exceeds deployment limit"
        );
    }
    let board_axis = schema_limits
        .get("max_board_axis")
        .and_then(Value::as_u64)
        .context("invalid typed board limit")?;
    let input_bytes = schema_limits
        .get("max_input_bytes")
        .and_then(Value::as_u64)
        .context("invalid typed input byte limit")?;
    ensure!(
        board_axis >= 1
            && ["height", "width"]
                .iter()
                .all(|name| board_axis <= limits.axis_maxima[*name] as u64)
            && (1..=limits.max_input_bytes as u64).contains(&input_bytes),
        "typed feature geometry/input limits exceed deployment limits"
    );
    Ok(())
}

// The ONNX boundary reads official protobuf fields needed for the contract.
// Unknown ordinary metadata is retained by the backend; unsupported external
// data, functions and nested graphs are explicitly rejected here.
#[derive(Clone, PartialEq, Message)]
struct ModelProto {
    #[prost(message, optional, tag = "7")]
    graph: Option<GraphProto>,
    #[prost(message, repeated, tag = "8")]
    opset: Vec<OperatorSet>,
    #[prost(message, repeated, tag = "25")]
    functions: Vec<Empty>,
}
#[derive(Clone, PartialEq, Message)]
struct Empty {}
#[derive(Clone, PartialEq, Message)]
struct OperatorSet {
    #[prost(string, tag = "1")]
    domain: String,
    #[prost(int64, tag = "2")]
    version: i64,
}
#[derive(Clone, PartialEq, Message)]
struct GraphProto {
    #[prost(message, repeated, tag = "1")]
    nodes: Vec<NodeProto>,
    #[prost(message, repeated, tag = "5")]
    initializers: Vec<TensorProto>,
    #[prost(message, repeated, tag = "11")]
    inputs: Vec<ValueInfo>,
    #[prost(message, repeated, tag = "12")]
    outputs: Vec<ValueInfo>,
    #[prost(message, repeated, tag = "15")]
    sparse: Vec<Empty>,
}
#[derive(Clone, PartialEq, Message)]
struct NodeProto {
    #[prost(string, repeated, tag = "1")]
    inputs: Vec<String>,
    #[prost(string, repeated, tag = "2")]
    outputs: Vec<String>,
    #[prost(string, tag = "4")]
    op: String,
    #[prost(message, repeated, tag = "5")]
    attributes: Vec<AttributeProto>,
    #[prost(string, tag = "7")]
    domain: String,
}
#[derive(Clone, PartialEq, Message)]
struct AttributeProto {
    #[prost(float, tag = "2")]
    float: f32,
    #[prost(message, optional, tag = "5")]
    tensor: Option<TensorProto>,
    #[prost(message, optional, tag = "6")]
    graph: Option<GraphProto>,
    #[prost(float, repeated, tag = "7")]
    floats: Vec<f32>,
    #[prost(message, repeated, tag = "10")]
    tensors: Vec<TensorProto>,
    #[prost(message, repeated, tag = "11")]
    graphs: Vec<GraphProto>,
    #[prost(message, optional, tag = "22")]
    sparse_tensor: Option<Empty>,
    #[prost(message, repeated, tag = "23")]
    sparse_tensors: Vec<Empty>,
}
#[derive(Clone, PartialEq, Message)]
struct TensorProto {
    #[prost(int64, repeated, tag = "1")]
    dims: Vec<i64>,
    #[prost(int32, tag = "2")]
    dtype: i32,
    #[prost(float, repeated, tag = "4")]
    floats: Vec<f32>,
    #[prost(string, tag = "8")]
    name: String,
    #[prost(bytes = "vec", tag = "9")]
    raw: Vec<u8>,
    #[prost(message, repeated, tag = "13")]
    external: Vec<Empty>,
    #[prost(int32, tag = "14")]
    location: i32,
}
#[derive(Clone, PartialEq, Message)]
struct ValueInfo {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(message, optional, tag = "2")]
    kind: Option<TypeProto>,
}
#[derive(Clone, PartialEq, Message)]
struct TypeProto {
    #[prost(message, optional, tag = "1")]
    tensor: Option<TensorType>,
}
#[derive(Clone, PartialEq, Message)]
struct TensorType {
    #[prost(int32, tag = "1")]
    dtype: i32,
    #[prost(message, optional, tag = "2")]
    shape: Option<ShapeProto>,
}
#[derive(Clone, PartialEq, Message)]
struct ShapeProto {
    #[prost(message, repeated, tag = "1")]
    dims: Vec<Dimension>,
}
#[derive(Clone, PartialEq, Message)]
struct Dimension {
    #[prost(oneof = "dimension::Size", tags = "1,2")]
    size: Option<dimension::Size>,
}
mod dimension {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Size {
        #[prost(int64, tag = "1")]
        Fixed(i64),
        #[prost(string, tag = "2")]
        Symbol(String),
    }
}

fn tensor(t: &TensorProto) -> Result<()> {
    ensure!(
        t.external.is_empty() && t.location == 0,
        "external ONNX tensor data is forbidden"
    );
    ensure!(
        [1, 2, 6, 7, 9].contains(&t.dtype),
        "unsupported initializer dtype; floating weights must be float32"
    );
    if t.dtype == 1 {
        ensure!(
            t.floats.iter().all(|v| v.is_finite())
                && t.raw.len().is_multiple_of(4)
                && t.raw
                    .chunks_exact(4)
                    .all(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).is_finite()),
            "non-finite ONNX weights"
        );
    }
    Ok(())
}
fn validate_graph(bytes: &[u8], config: &ModelConfig) -> Result<()> {
    let model = ModelProto::decode(bytes)?;
    ensure!(
        model.opset.len() == 1
            && ["", "ai.onnx"].contains(&model.opset[0].domain.as_str())
            && model.opset[0].version == 18
            && model.functions.is_empty(),
        "ONNX requires opset18 without custom functions/domains"
    );
    let graph = model.graph.context("ONNX graph missing")?;
    ensure!(
        graph.sparse.is_empty() && graph.nodes.len() <= 100_000,
        "unsupported sparse graph or graph node limit"
    );
    ensure!(
        graph
            .inputs
            .iter()
            .map(|v| v.name.as_str())
            .eq(["board", "condition", "action_features"])
            && graph
                .outputs
                .iter()
                .map(|v| v.name.as_str())
                .eq(["policy_logits", "value"]),
        "ONNX input/output names mismatch"
    );
    let expected = [
        vec![None, Some(config.board_channels), Some(8), Some(8)],
        vec![None, Some(config.condition_dim)],
        vec![None, None, Some(config.action_dim)],
        vec![None, None],
        vec![None, Some(1)],
    ];
    let mut batch_symbol: Option<String> = None;
    let mut action_symbol: Option<String> = None;
    for (index, (value, dimensions)) in graph
        .inputs
        .iter()
        .chain(&graph.outputs)
        .zip(expected)
        .enumerate()
    {
        let tensor = value
            .kind
            .as_ref()
            .and_then(|v| v.tensor.as_ref())
            .context("ONNX tensor type missing")?;
        let shape = tensor.shape.as_ref().context("ONNX tensor shape missing")?;
        ensure!(
            tensor.dtype == 1 && shape.dims.len() == dimensions.len(),
            "ONNX float32/rank mismatch"
        );
        for (axis, (dim, required)) in shape.dims.iter().zip(dimensions).enumerate() {
            match (&dim.size, required) {
                (Some(dimension::Size::Fixed(actual)), Some(expected)) => ensure!(
                    *actual == expected as i64,
                    "ONNX static feature shape mismatch"
                ),
                (Some(dimension::Size::Symbol(symbol)), None) if !symbol.is_empty() => {
                    let shared = if axis == 0 {
                        &mut batch_symbol
                    } else {
                        ensure!(index == 2 || index == 3, "unexpected dynamic axis");
                        &mut action_symbol
                    };
                    if let Some(expected) = shared {
                        ensure!(expected == symbol, "ONNX shared dynamic symbols mismatch");
                    } else {
                        *shared = Some(symbol.clone());
                    }
                }
                _ => anyhow::bail!("ONNX batch/actions must remain dynamic and features static"),
            }
        }
    }
    ensure!(
        batch_symbol != action_symbol,
        "ONNX batch and action symbols must be independent"
    );
    let input_names: BTreeSet<&str> = graph
        .inputs
        .iter()
        .map(|input| input.name.as_str())
        .collect();
    let mut initializer_names = BTreeSet::new();
    for initializer in &graph.initializers {
        ensure!(
            !initializer.name.is_empty()
                && !input_names.contains(initializer.name.as_str())
                && initializer_names.insert(initializer.name.as_str()),
            "ONNX initializer name is empty, duplicate, or shadows an input"
        );
        tensor(initializer)?;
    }
    let mut producers = BTreeMap::new();
    for node in &graph.nodes {
        ensure!(
            ["", "ai.onnx"].contains(&node.domain.as_str()) && !node.op.is_empty(),
            "unsupported ONNX node domain"
        );
        for output in &node.outputs {
            if output.is_empty() {
                // ONNX permits an omitted optional output with an empty name.
                continue;
            }
            ensure!(
                !input_names.contains(output.as_str())
                    && !initializer_names.contains(output.as_str()),
                "ONNX node output shadows an input or initializer"
            );
            ensure!(
                producers.insert(output.as_str(), node).is_none(),
                "duplicate ONNX producer"
            );
        }
        for attr in &node.attributes {
            ensure!(
                attr.float.is_finite()
                    && attr.floats.iter().all(|value| value.is_finite())
                    && attr.graph.is_none()
                    && attr.graphs.is_empty()
                    && attr.sparse_tensor.is_none()
                    && attr.sparse_tensors.is_empty(),
                "nested/sparse graph or non-finite attribute unsupported"
            );
            if let Some(t) = &attr.tensor {
                tensor(t)?;
            }
            for t in &attr.tensors {
                tensor(t)?;
            }
        }
    }
    for output in ["policy_logits", "value"] {
        let mut pending = vec![output];
        let mut visited = BTreeSet::new();
        while let Some(name) = pending.pop() {
            if visited.insert(name)
                && let Some(node) = producers.get(name)
                && !["Shape", "Size"].contains(&node.op.as_str())
            {
                pending.extend(node.inputs.iter().map(String::as_str));
            }
        }
        ensure!(
            visited.contains("condition"),
            "FiLM condition is disconnected from an output"
        );
    }
    Ok(())
}

fn validate_typed_graph(bytes: &[u8], contract: &TypedOnnxContract) -> Result<()> {
    let model = ModelProto::decode(bytes)?;
    ensure!(
        model.opset.len() == 1
            && ["", "ai.onnx"].contains(&model.opset[0].domain.as_str())
            && model.opset[0].version == 18
            && model.functions.is_empty(),
        "ONNX requires opset18 without custom functions/domains"
    );
    let graph = model.graph.context("ONNX graph missing")?;
    ensure!(
        graph.sparse.is_empty() && graph.nodes.len() <= 100_000,
        "unsupported sparse graph or graph node limit"
    );
    ensure!(
        graph.inputs.len() == contract.inputs.len()
            && graph.outputs.len() == contract.outputs.len(),
        "ONNX input/output count mismatch"
    );
    for (actual, expected) in graph
        .inputs
        .iter()
        .zip(&contract.inputs)
        .chain(graph.outputs.iter().zip(&contract.outputs))
    {
        ensure!(
            actual.name == expected.name,
            "ONNX tensor name/order mismatch"
        );
        let tensor_type = actual
            .kind
            .as_ref()
            .and_then(|value| value.tensor.as_ref())
            .context("ONNX tensor type missing")?;
        let shape = tensor_type
            .shape
            .as_ref()
            .context("ONNX tensor shape missing")?;
        ensure!(
            tensor_type.dtype == expected.dtype.onnx_id()
                && shape.dims.len() == expected.shape.len(),
            "ONNX typed dtype/rank mismatch"
        );
        for (actual_dim, expected_dim) in shape.dims.iter().zip(&expected.shape) {
            match (&actual_dim.size, expected_dim) {
                (Some(dimension::Size::Fixed(value)), Axis::Fixed(required))
                    if *value == *required as i64 => {}
                (Some(dimension::Size::Symbol(value)), Axis::Dynamic(required))
                    if value == required => {}
                _ => anyhow::bail!("ONNX typed shape/dynamic symbol mismatch"),
            }
        }
    }
    let input_names: BTreeSet<&str> = graph.inputs.iter().map(|v| v.name.as_str()).collect();
    let mut initializer_names = BTreeSet::new();
    let mut parameter_elements: u128 = 0;
    for initializer in &graph.initializers {
        ensure!(
            !initializer.name.is_empty()
                && !input_names.contains(initializer.name.as_str())
                && initializer_names.insert(initializer.name.as_str()),
            "ONNX initializer name is empty, duplicate, or shadows an input"
        );
        tensor(initializer)?;
        parameter_elements = parameter_elements
            .checked_add(initializer.dims.iter().try_fold(1u128, |acc, dim| {
                ensure!(*dim >= 0, "negative ONNX initializer dimension");
                acc.checked_mul(*dim as u128)
                    .context("initializer shape overflow")
            })?)
            .context("initializer element overflow")?;
    }
    ensure!(
        parameter_elements <= 64_000_000,
        "model parameter budget exceeds 64 million"
    );
    let mut producers = BTreeMap::new();
    for node in &graph.nodes {
        ensure!(
            ["", "ai.onnx"].contains(&node.domain.as_str()) && !node.op.is_empty(),
            "unsupported ONNX node domain"
        );
        for output in &node.outputs {
            if output.is_empty() {
                continue;
            }
            ensure!(
                !input_names.contains(output.as_str())
                    && !initializer_names.contains(output.as_str()),
                "ONNX node output shadows an input or initializer"
            );
            ensure!(
                producers.insert(output.as_str(), node).is_none(),
                "duplicate ONNX producer"
            );
        }
        for attribute in &node.attributes {
            ensure!(
                attribute.float.is_finite()
                    && attribute.floats.iter().all(|value| value.is_finite())
                    && attribute.graph.is_none()
                    && attribute.graphs.is_empty()
                    && attribute.sparse_tensor.is_none()
                    && attribute.sparse_tensors.is_empty(),
                "nested/sparse graph or non-finite attribute unsupported"
            );
            if let Some(value) = &attribute.tensor {
                tensor(value)?;
            }
            for value in &attribute.tensors {
                tensor(value)?;
            }
        }
    }
    for output in ["policy_logits", "value"] {
        let mut pending = vec![output];
        let mut visited = BTreeSet::new();
        while let Some(name) = pending.pop() {
            if visited.insert(name)
                && let Some(node) = producers.get(name)
                && !["Shape", "Size"].contains(&node.op.as_str())
            {
                pending.extend(node.inputs.iter().map(String::as_str));
            }
        }
        ensure!(
            visited.contains("condition"),
            "FiLM condition is disconnected from an output"
        );
    }
    Ok(())
}
