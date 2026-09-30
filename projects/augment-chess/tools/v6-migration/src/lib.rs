//! Read-only transport admission for frozen v6 positions and one source-proven
//! v7 data conversion. This crate does not link or execute either rules engine.

use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Number, Value, json};
use sha2::{Digest, Sha256};
use std::fmt;

pub const V6_RULES_VERSION: &str = "augment-site-20260927-abfe01a035813875";
pub const V7_RULES_VERSION: &str = "augment-site-20260928-e5ed84fcf8e72a24";
pub const CATALOG_VERSION: &str = "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4";
pub const V7_CATALOG_VERSION: &str =
    "f80ebcd21759df179bccfb301415e672194de67691a6383beafc549538ffae7c";
const V7_EXECUTION_PROFILE: &str = "accelerate-headless-semantic-v7-faithful-init-v1";
const V7_EXECUTION_PROFILE_SHA256: &str =
    "d811f0232ac38af4e45e0e4f93e89c49712142dfd0f2b5fe57d36f63cd05a29f";
pub const POSITION_PROTOCOL_VERSION: &str = "accelerate-position-v1";
pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_NODES: usize = 100_000;
const MAX_DEPTH: usize = 64;
const SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
const FROZEN_INITIAL_STATE_SHA256: &str =
    "7007e25d9e95b7ff2753ef5e9d75f52ebaacecdda533a741a723849525a43a60";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationError {
    InvalidInput(String),
    LimitExceeded(String),
    IdentityMismatch { expected: String, actual: String },
    SourceMismatch(String),
    Unsupported { path: String, reason: String },
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(f, "invalid v6 input: {message}"),
            Self::LimitExceeded(message) => write!(f, "v6 input limit exceeded: {message}"),
            Self::IdentityMismatch { expected, actual } => {
                write!(
                    f,
                    "v6 positionId mismatch: expected {expected}, got {actual}"
                )
            }
            Self::SourceMismatch(message) => write!(f, "frozen source mismatch: {message}"),
            Self::Unsupported { path, reason } => {
                write!(f, "v6 to v7 conversion unsupported at {path}: {reason}")
            }
        }
    }
}

impl std::error::Error for MigrationError {}

pub type Result<T> = std::result::Result<T, MigrationError>;

/// A transport-validated, immutable v6 envelope. Unknown source `state`
/// fields and historical events are retained; their rule semantics are not
/// interpreted or certified by this reader.
#[derive(Debug, Clone)]
pub struct V6Position {
    envelope: Value,
}

impl V6Position {
    pub fn envelope(&self) -> &Value {
        &self.envelope
    }

    pub fn position_id(&self) -> &str {
        self.envelope["positionId"]
            .as_str()
            .expect("read_v6_position checked positionId")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionEvidence {
    pub source_position_id: String,
    pub source_rules_version: &'static str,
    pub target_rules_version: &'static str,
    pub source_state_sha256: String,
    pub verified_scope: &'static str,
}

/// This is a versioned data envelope, not an executable v7 Position. The v7
/// host must independently admit the result before any rule call.
#[derive(Debug, Clone)]
pub struct V7Conversion {
    envelope: Value,
    evidence: ConversionEvidence,
}

impl V7Conversion {
    pub fn envelope(&self) -> &Value {
        &self.envelope
    }

    pub fn evidence(&self) -> &ConversionEvidence {
        &self.evidence
    }
}

// serde_json::Value silently keeps the last occurrence of a duplicate key.
// Read duplicates explicitly so neither identity nor conversion can depend
// on ambiguous input text.
struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueValue;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON value without duplicate object keys")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                value: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(value)))
            }
            fn visit_i64<E: serde::de::Error>(
                self,
                value: i64,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::Number(value.into())))
            }
            fn visit_u64<E: serde::de::Error>(
                self,
                value: u64,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::Number(value.into())))
            }
            fn visit_f64<E: serde::de::Error>(
                self,
                value: f64,
            ) -> std::result::Result<Self::Value, E> {
                Number::from_f64(value)
                    .map(Value::Number)
                    .map(UniqueValue)
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                value: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(value.to_owned())))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                value: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(value)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element::<UniqueValue>()? {
                    values.push(value.0);
                }
                Ok(UniqueValue(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some((key, value)) = map.next_entry::<String, UniqueValue>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON key {key}"
                        )));
                    }
                    values.insert(key, value.0);
                }
                Ok(UniqueValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

fn checked_json(bytes: &[u8]) -> Result<Value> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(MigrationError::LimitExceeded(format!(
            "{} bytes exceeds 8 MiB",
            bytes.len()
        )));
    }
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    let value = UniqueValue::deserialize(&mut parser)
        .map_err(|error| MigrationError::InvalidInput(format!("JSON: {error}")))?
        .0;
    parser
        .end()
        .map_err(|error| MigrationError::InvalidInput(format!("JSON trailing data: {error}")))?;
    let mut pending = vec![(&value, 0usize)];
    let mut nodes = 0usize;
    while let Some((value, depth)) = pending.pop() {
        nodes += 1;
        if nodes > MAX_NODES {
            return Err(MigrationError::LimitExceeded(
                "JSON node count exceeds 100000".into(),
            ));
        }
        if depth > MAX_DEPTH {
            return Err(MigrationError::LimitExceeded(
                "JSON depth exceeds 64".into(),
            ));
        }
        match value {
            Value::Number(number) => {
                let numeric = number.as_f64().ok_or_else(|| {
                    MigrationError::InvalidInput("JSON number outside execution range".into())
                })?;
                if !numeric.is_finite() || (numeric.fract() == 0.0 && numeric.abs() > SAFE_INTEGER)
                {
                    return Err(MigrationError::InvalidInput(
                        "JSON numbers must be finite and integral values JavaScript-safe".into(),
                    ));
                }
            }
            Value::Array(items) => pending.extend(items.iter().map(|item| (item, depth + 1))),
            Value::Object(items) => {
                nodes = nodes.saturating_add(items.len());
                pending.extend(items.values().map(|item| (item, depth + 1)));
            }
            _ => {}
        }
    }
    Ok(value)
}

fn object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| MigrationError::InvalidInput(format!("{path} must be an object")))
}

fn exact_fields(object: &Map<String, Value>, required: &[&str], path: &str) -> Result<()> {
    if object.len() != required.len() || required.iter().any(|name| !object.contains_key(*name)) {
        let unexpected: Vec<_> = object
            .keys()
            .filter(|name| !required.contains(&name.as_str()))
            .collect();
        let missing: Vec<_> = required
            .iter()
            .filter(|name| !object.contains_key(**name))
            .collect();
        return Err(MigrationError::InvalidInput(format!(
            "{path} fields differ: missing {missing:?}, unexpected {unexpected:?}"
        )));
    }
    Ok(())
}

fn hash(value: &Value) -> Result<String> {
    let canonical = serde_jcs::to_vec(value).map_err(|error| {
        MigrationError::InvalidInput(format!("JCS canonicalization failed: {error}"))
    })?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

fn validate_board(state: &Map<String, Value>) -> Result<()> {
    let board = state
        .get("board")
        .and_then(Value::as_array)
        .ok_or_else(|| MigrationError::InvalidInput("state.board must be an 8x8 array".into()))?;
    if board.len() != 8 {
        return Err(MigrationError::InvalidInput(
            "state.board must have eight rows".into(),
        ));
    }
    for (row_index, row) in board.iter().enumerate() {
        let cells = row.as_array().ok_or_else(|| {
            MigrationError::InvalidInput(format!("state.board[{row_index}] must be an array"))
        })?;
        if cells.len() != 8 {
            return Err(MigrationError::InvalidInput(format!(
                "state.board[{row_index}] must have eight cells"
            )));
        }
        for (col_index, cell) in cells.iter().enumerate() {
            if cell.is_null() {
                continue;
            }
            let path = format!("state.board[{row_index}][{col_index}]");
            let piece = object(cell, &path)?;
            if piece
                .get("type")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
                || !matches!(
                    piece.get("color").and_then(Value::as_str),
                    Some("white" | "black" | "neutral")
                )
                || piece.get("moved").is_some_and(|value| !value.is_boolean())
                || piece.get("id").is_some_and(|value| !value.is_string())
            {
                return Err(MigrationError::InvalidInput(format!(
                    "{path} has invalid piece identity, color or movement field"
                )));
            }
        }
    }
    Ok(())
}

fn validate_rng(value: &Value) -> Result<()> {
    let rng = object(value, "rng")?;
    exact_fields(rng, &["algorithm", "state", "cursor", "tape"], "rng")?;
    if rng.get("algorithm").and_then(Value::as_str) != Some("lcg32-v1") {
        return Err(MigrationError::InvalidInput(
            "rng.algorithm must be lcg32-v1".into(),
        ));
    }
    if rng
        .get("state")
        .and_then(Value::as_u64)
        .is_none_or(|value| value > u32::MAX as u64)
    {
        return Err(MigrationError::InvalidInput(
            "rng.state must be uint32".into(),
        ));
    }
    if rng
        .get("cursor")
        .and_then(Value::as_u64)
        .is_none_or(|value| value > SAFE_INTEGER as u64)
    {
        return Err(MigrationError::InvalidInput(
            "rng.cursor must be a JavaScript-safe nonnegative integer".into(),
        ));
    }
    if rng
        .get("tape")
        .and_then(Value::as_array)
        .is_none_or(|tape| {
            tape.iter().any(|value| {
                value
                    .as_f64()
                    .is_none_or(|number| !(0.0..1.0).contains(&number))
            })
        })
    {
        return Err(MigrationError::InvalidInput(
            "rng.tape values must be in [0, 1)".into(),
        ));
    }
    Ok(())
}

/// Validate the fixed v6 transport and JCS identity without executing or
/// interpreting card, movement, replay, or arbitrary state-extra semantics.
pub fn read_v6_position(bytes: &[u8]) -> Result<V6Position> {
    let value = checked_json(bytes)?;
    let envelope = object(&value, "position")?;
    exact_fields(
        envelope,
        &[
            "protocolVersion",
            "rulesVersion",
            "catalogVersion",
            "state",
            "rng",
            "history",
            "positionId",
        ],
        "position",
    )?;
    for (name, expected) in [
        ("protocolVersion", POSITION_PROTOCOL_VERSION),
        ("rulesVersion", V6_RULES_VERSION),
        ("catalogVersion", CATALOG_VERSION),
    ] {
        if envelope.get(name).and_then(Value::as_str) != Some(expected) {
            return Err(MigrationError::InvalidInput(format!(
                "position.{name} must equal {expected}"
            )));
        }
    }
    let state = object(&value["state"], "state")?;
    if state.contains_key("rng") || state.contains_key("history") {
        return Err(MigrationError::InvalidInput(
            "state must not duplicate outer rng/history".into(),
        ));
    }
    if state
        .get("rulesetId")
        .is_some_and(|version| version.as_str() != Some(V6_RULES_VERSION))
    {
        return Err(MigrationError::InvalidInput(
            "state.rulesetId disagrees with v6 envelope".into(),
        ));
    }
    if !matches!(
        state.get("turn").and_then(Value::as_str),
        Some("white" | "black")
    ) {
        return Err(MigrationError::InvalidInput(
            "state.turn must be white or black".into(),
        ));
    }
    if state
        .get("mode")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(MigrationError::InvalidInput(
            "state.mode must be nonempty text".into(),
        ));
    }
    if state
        .get("actionsRemaining")
        .is_some_and(|value| value.as_u64().is_none_or(|n| n > 32))
    {
        return Err(MigrationError::InvalidInput(
            "state.actionsRemaining must be in 0..=32".into(),
        ));
    }
    validate_board(state)?;
    validate_rng(&value["rng"])?;
    let history = value["history"]
        .as_array()
        .ok_or_else(|| MigrationError::InvalidInput("history must be an array".into()))?;
    for (index, event) in history.iter().enumerate() {
        if event.as_object().is_none() || event["protocolVersion"] != "accelerate-game-event-v1" {
            return Err(MigrationError::InvalidInput(format!(
                "history[{index}] must be a v1 game event object"
            )));
        }
    }
    let actual = value["positionId"].as_str().ok_or_else(|| {
        MigrationError::InvalidInput("positionId must be lowercase SHA-256 hex".into())
    })?;
    if actual.len() != 64
        || !actual
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(MigrationError::InvalidInput(
            "positionId must be lowercase SHA-256 hex".into(),
        ));
    }
    let mut content = value.clone();
    content
        .as_object_mut()
        .expect("checked envelope")
        .remove("positionId");
    let expected = hash(&content)?;
    if actual != expected {
        return Err(MigrationError::IdentityMismatch {
            expected,
            actual: actual.into(),
        });
    }
    Ok(V6Position { envelope: value })
}

fn frozen_initial(version: &str) -> Result<Value> {
    let source = match version {
        V6_RULES_VERSION => include_str!("../../../contracts/catalog/initial-state-20260927.json"),
        V7_RULES_VERSION => include_str!("../../../contracts/catalog/initial-state-20260928.json"),
        _ => {
            return Err(MigrationError::SourceMismatch(
                "unknown frozen initial-state version".into(),
            ));
        }
    };
    let value: Value = serde_json::from_str(source)
        .map_err(|error| MigrationError::SourceMismatch(format!("initial-state JSON: {error}")))?;
    let expected_catalog = if version == V7_RULES_VERSION {
        V7_CATALOG_VERSION
    } else {
        CATALOG_VERSION
    };
    if value["schemaVersion"] != 1
        || value["rulesVersion"] != version
        || value["catalogVersion"] != expected_catalog
        || !value["state"].is_object()
    {
        return Err(MigrationError::SourceMismatch(format!(
            "{version} initial-state metadata invalid"
        )));
    }
    if version == V7_RULES_VERSION {
        let manifest: Value = serde_json::from_str(include_str!(
            "../../../contracts/catalog/execution-profile-20260928.json"
        ))
        .map_err(|error| {
            MigrationError::SourceMismatch(format!("v7 execution profile JSON: {error}"))
        })?;
        if value["sourcePublicCatalogHash"] != CATALOG_VERSION
            || value["executionProfile"]["version"] != V7_EXECUTION_PROFILE
            || value["executionProfile"]["sha256"] != V7_EXECUTION_PROFILE_SHA256
            || hash(&manifest)? != V7_EXECUTION_PROFILE_SHA256
        {
            return Err(MigrationError::SourceMismatch(
                "v7 initial-state execution profile does not match the frozen faithful manifest"
                    .into(),
            ));
        }
    }
    Ok(value)
}

/// Convert only an untouched, history-free v6 initial-state template whose
/// complete state matches *both* frozen source templates. The unchanged RNG
/// must be at its pre-draw boundary. All other v6 states fail with a named
/// unsupported path; no v6 rule or replay is run.
pub fn convert_verified_initial_template(position: &V6Position) -> Result<V7Conversion> {
    let v6 = frozen_initial(V6_RULES_VERSION)?;
    let v7 = frozen_initial(V7_RULES_VERSION)?;
    if v6["state"] != v7["state"] {
        return Err(MigrationError::SourceMismatch(
            "v6 and v7 frozen initial-state templates are not identical".into(),
        ));
    }
    let frozen_hash = hash(&v6["state"])?;
    if frozen_hash != FROZEN_INITIAL_STATE_SHA256 {
        return Err(MigrationError::SourceMismatch(format!(
            "initial-state canonical SHA-256 changed: expected {FROZEN_INITIAL_STATE_SHA256}, got {frozen_hash}"
        )));
    }
    let source = position.envelope();
    if source["state"] != v6["state"] {
        return Err(MigrationError::Unsupported {
            path: "state".into(),
            reason: "only the exact frozen initial-state template is verified".into(),
        });
    }
    if source["history"]
        .as_array()
        .is_none_or(|events| !events.is_empty())
    {
        return Err(MigrationError::Unsupported {
            path: "history".into(),
            reason: "v6 replay events have no validated v7 conversion".into(),
        });
    }
    if source["rng"]["cursor"] != 0 || source["rng"]["tape"] != json!([]) {
        return Err(MigrationError::Unsupported {
            path: "rng".into(),
            reason: "only a pre-draw RNG cursor with an empty tape is verified".into(),
        });
    }
    let mut target = source.clone();
    target["rulesVersion"] = json!(V7_RULES_VERSION);
    target["catalogVersion"] = json!(V7_CATALOG_VERSION);
    // Use the target template's insertion order as well as its field values.
    // Some source replay paths use JSON.stringify on state-derived objects.
    target["state"] = v7["state"].clone();
    target
        .as_object_mut()
        .expect("checked envelope")
        .remove("positionId");
    let target_id = hash(&target)?;
    target["positionId"] = json!(target_id);
    Ok(V7Conversion {
        evidence: ConversionEvidence {
            source_position_id: position.position_id().into(),
            source_rules_version: V6_RULES_VERSION,
            target_rules_version: V7_RULES_VERSION,
            source_state_sha256: frozen_hash,
            verified_scope: "frozen-initial-template-no-history-pre-draw",
        },
        envelope: target,
    })
}
