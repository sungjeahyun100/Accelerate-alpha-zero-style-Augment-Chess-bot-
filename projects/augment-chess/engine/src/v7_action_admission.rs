//! Source-versioned v7 action-envelope admission.
//!
//! This module checks the internal source identity and shape before a rule
//! object inspects an action. Legality probes use an owned state copy; admission
//! never changes the host Position or records a public transition event.
//! The source payload is retained verbatim: absent and explicit-null target
//! fields have different action IDs even if they decode to the same `Action`.

use crate::{Action, EngineError, GameState, RULES_VERSION_V7, V7HostPosition};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

pub const V7_ACTION_PROTOCOL: &str = "accelerate-action-v1";
const MAX_ACTION_BYTES: usize = 64 * 1024;
const COMMON: &[&str] = &["type", "color"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionErrorKind {
    InvalidEnvelope,
    InvalidPayload,
    StalePosition,
    WrongActor,
    Terminal,
    Unsupported,
    IllegalAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmissionError {
    pub kind: AdmissionErrorKind,
    pub code: &'static str,
    pub detail: String,
}

impl AdmissionError {
    fn new(kind: AdmissionErrorKind, code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            kind,
            code,
            detail: detail.into(),
        }
    }

    fn envelope(detail: impl Into<String>) -> Self {
        Self::new(
            AdmissionErrorKind::InvalidEnvelope,
            "invalid_action_envelope",
            detail,
        )
    }

    fn payload(detail: impl Into<String>) -> Self {
        Self::new(
            AdmissionErrorKind::InvalidPayload,
            "invalid_action_payload",
            detail,
        )
    }

    fn semantic(error: EngineError, kind: &str) -> Self {
        match error {
            EngineError::UnsupportedFeature(detail) => Self::new(
                AdmissionErrorKind::Unsupported,
                "action_rule_unsupported",
                detail,
            ),
            EngineError::IllegalAction => Self::new(
                AdmissionErrorKind::IllegalAction,
                "illegal_action",
                format!("{kind} is not a legal selection for this Position"),
            ),
            EngineError::WrongActor => Self::new(
                AdmissionErrorKind::WrongActor,
                "wrong_action_actor",
                "action color is not the current decision actor",
            ),
            EngineError::StaleAction => Self::new(
                AdmissionErrorKind::StalePosition,
                "stale_action_position",
                "action belongs to another Position",
            ),
            EngineError::Terminal => Self::new(
                AdmissionErrorKind::Terminal,
                "terminal_action_position",
                "game is terminal",
            ),
            other => Self::new(
                AdmissionErrorKind::InvalidPayload,
                "invalid_action_rule_input",
                other.to_string(),
            ),
        }
    }
}

impl std::fmt::Display for AdmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for AdmissionError {}

#[derive(Debug)]
struct VerifiedSourceAction {
    action: Action,
    payload: Value,
    action_id: String,
}

/// Source-verified selections for one immutable host Position. `complete`
/// proves an eager ordered set; scalar admission proves the selected member
/// without allocating its potentially enormous family. Its digest index is
/// an accelerator: membership still compares the exact payload and decoded
/// Action, so a hash collision cannot authorize another selection.
#[derive(Debug)]
pub(crate) struct VerifiedV7ActionSet {
    position_id: String,
    revision: u64,
    entries: Vec<VerifiedSourceAction>,
    by_id: BTreeMap<String, Vec<usize>>,
}

impl VerifiedV7ActionSet {
    pub(crate) fn complete(position: &V7HostPosition) -> Result<Arc<Self>, AdmissionError> {
        let actions = crate::v7_action_surface::legal_source_actions(position.state())
            .map_err(|error| AdmissionError::semantic(error, "source action set"))?;
        Self::from_verified_actions(position, actions)
    }

    fn from_verified_actions(
        position: &V7HostPosition,
        actions: Vec<Action>,
    ) -> Result<Arc<Self>, AdmissionError> {
        let mut entries = Vec::with_capacity(actions.len());
        let mut by_id: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for action in actions {
            let payload = serde_json::to_value(&action).map_err(|error| {
                AdmissionError::payload(format!("source candidate cannot serialize: {error}"))
            })?;
            let action_id = source_action_id(&payload)?;
            let decoded = decode_v7_action(
                position,
                serde_json::json!({
                    "protocolVersion": V7_ACTION_PROTOCOL,
                    "positionId": position.position_id(),
                    "actionId": action_id,
                    "payload": payload,
                }),
            )?;
            if decoded.action != action {
                return Err(AdmissionError::payload(
                    "source candidate changed while checking its exact envelope",
                ));
            }
            by_id
                .entry(action_id.clone())
                .or_default()
                .push(entries.len());
            entries.push(VerifiedSourceAction {
                action,
                payload,
                action_id,
            });
        }
        Ok(Arc::new(Self {
            position_id: position.position_id().to_owned(),
            revision: position.revision(),
            entries,
            by_id,
        }))
    }

    pub(crate) fn source_entries(&self) -> impl Iterator<Item = (&Action, &Value, &str)> {
        self.entries
            .iter()
            .map(|entry| (&entry.action, &entry.payload, entry.action_id.as_str()))
    }

    fn admit_decoded(
        self: Arc<Self>,
        position: &V7HostPosition,
        decoded: DecodedV7Action,
    ) -> Result<AdmittedV7Action, AdmissionError> {
        if self.position_id != position.position_id() || self.revision != position.revision() {
            return Err(AdmissionError::new(
                AdmissionErrorKind::StalePosition,
                "stale_action_position",
                "verified action set belongs to another Position or revision",
            ));
        }
        if !self.by_id.get(&decoded.action_id).is_some_and(|indices| {
            indices.iter().any(|index| {
                let entry = &self.entries[*index];
                entry.payload == decoded.source_payload && entry.action == decoded.action
            })
        }) {
            return Err(AdmissionError::semantic(
                EngineError::IllegalAction,
                "action",
            ));
        }
        Ok(AdmittedV7Action {
            action: decoded.action,
            source_payload: decoded.source_payload,
            action_id: decoded.action_id,
            position_id: decoded.position_id,
            verified_set: self,
        })
    }
}

#[derive(Debug)]
struct DecodedV7Action {
    action: Action,
    source_payload: Value,
    action_id: String,
    position_id: String,
}

/// A host-bound action keeps the exact source payload for replay and action-ID
/// checks. Its internal `Action` stays unbound: legacy `Position::apply` uses a
/// different, 16-digit key and must never mistake a v7 host ID for that key.
/// Consumers must use `revalidate` at the point of applying it.
#[derive(Clone, Debug)]
pub struct AdmittedV7Action {
    action: Action,
    source_payload: Value,
    action_id: String,
    position_id: String,
    verified_set: Arc<VerifiedV7ActionSet>,
}

impl AdmittedV7Action {
    pub fn action(&self) -> &Action {
        &self.action
    }

    pub fn source_payload(&self) -> &Value {
        &self.source_payload
    }

    pub fn action_id(&self) -> &str {
        &self.action_id
    }

    pub fn revalidate(&self, position: &V7HostPosition) -> Result<(), AdmissionError> {
        let envelope = serde_json::json!({
            "protocolVersion": V7_ACTION_PROTOCOL,
            "positionId": self.position_id,
            "actionId": self.action_id,
            "payload": self.source_payload,
        });
        let checked = admit_v7_action_from_set(position, envelope, self.verified_set.clone())?;
        if checked.action != self.action {
            return Err(AdmissionError::payload(
                "bound action no longer matches its exact source payload",
            ));
        }
        Ok(())
    }
}

/// Each action kind has one static owner for its public field shape and source
/// meaning handoff. An absent meaning implementation is never an allow-all.
struct ActionKindAdmission {
    name: &'static str,
    required: &'static [&'static str],
    optional: &'static [&'static str],
    exact_fields: bool,
}

const OBJECTS: &[ActionKindAdmission] = &[
    ActionKindAdmission {
        name: "move",
        required: &["from", "move"],
        optional: &["forcedFriendlyCrush"],
        exact_fields: true,
    },
    ActionKindAdmission {
        name: "card",
        required: &["cardId", "cardInstanceId"],
        optional: &["target"],
        exact_fields: true,
    },
    ActionKindAdmission {
        name: "promotion",
        required: &["from"],
        optional: &[],
        exact_fields: true,
    },
    ActionKindAdmission {
        name: "promotionChoice",
        required: &["promotionType"],
        optional: &[],
        exact_fields: true,
    },
    ActionKindAdmission {
        name: "shotgunReload",
        required: &["from"],
        optional: &[],
        exact_fields: true,
    },
    ActionKindAdmission {
        name: "wizardSpell",
        required: &["from", "spellId", "target"],
        optional: &[],
        exact_fields: true,
    },
    ActionKindAdmission {
        name: "fileSurgeSkip",
        required: &["from"],
        optional: &[],
        exact_fields: true,
    },
    ActionKindAdmission {
        name: "draftPick",
        required: &["cardInstanceId"],
        optional: &[],
        exact_fields: true,
    },
    ActionKindAdmission {
        name: "draftBundlePick",
        required: &["bundleIndex", "cardInstanceIds"],
        optional: &[],
        exact_fields: true,
    },
    ActionKindAdmission {
        name: "trolleyChoice",
        required: &["windowId", "doomedIndex"],
        optional: &[],
        exact_fields: true,
    },
];

/// Admit a source v7 Action envelope against one immutable Position. This is
/// deliberately private to the game project and does not itself advertise a
/// public adapter capability or imply that an admitted action can be applied.
pub fn admit_v7_action(
    position: &V7HostPosition,
    envelope: Value,
) -> Result<AdmittedV7Action, AdmissionError> {
    // Malformed, stale, wrong-actor and terminal input fails before rule work.
    // The source owner proves exact candidate membership and selected effect
    // acceptance. Large lazy target families need no complete allocation.
    let decoded = decode_v7_action(position, envelope)?;
    crate::v7_action_surface::validate_source_selection(position.state(), &decoded.action)
        .map_err(|error| AdmissionError::semantic(error, "action"))?;
    VerifiedV7ActionSet::from_verified_actions(position, vec![decoded.action.clone()])?
        .admit_decoded(position, decoded)
}

pub(crate) fn admit_v7_action_from_set(
    position: &V7HostPosition,
    envelope: Value,
    verified_set: Arc<VerifiedV7ActionSet>,
) -> Result<AdmittedV7Action, AdmissionError> {
    let decoded = decode_v7_action(position, envelope)?;
    verified_set.admit_decoded(position, decoded)
}

fn decode_v7_action(
    position: &V7HostPosition,
    envelope: Value,
) -> Result<DecodedV7Action, AdmissionError> {
    if position.state().ruleset_id != RULES_VERSION_V7 {
        return Err(AdmissionError::new(
            AdmissionErrorKind::Unsupported,
            "action_rules_version_unsupported",
            "action admission requires the pinned v7 rules version",
        ));
    }
    let wire = serde_json::to_vec(&envelope).map_err(|error| {
        AdmissionError::envelope(format!("action JSON cannot serialize: {error}"))
    })?;
    if wire.len() > MAX_ACTION_BYTES {
        return Err(AdmissionError::envelope(format!(
            "action envelope exceeds {MAX_ACTION_BYTES} bytes"
        )));
    }
    crate::state::validate_json_value(&envelope, 0)
        .map_err(|error| AdmissionError::envelope(error.to_string()))?;
    let fields = envelope
        .as_object()
        .ok_or_else(|| AdmissionError::envelope("action envelope must be an object"))?;
    require_exact_fields(
        fields,
        &["protocolVersion", "positionId", "actionId", "payload"],
        &[],
        "action envelope",
        AdmissionError::envelope,
    )?;
    if fields.get("protocolVersion").and_then(Value::as_str) != Some(V7_ACTION_PROTOCOL) {
        return Err(AdmissionError::envelope(format!(
            "action protocolVersion must be {V7_ACTION_PROTOCOL}"
        )));
    }
    let position_id = fields["positionId"]
        .as_str()
        .ok_or_else(|| AdmissionError::envelope("action positionId must be text"))?;
    if position_id != position.position_id() {
        return Err(AdmissionError::new(
            AdmissionErrorKind::StalePosition,
            "stale_action_position",
            "action positionId does not match the complete Position",
        ));
    }
    let action_id = fields["actionId"]
        .as_str()
        .ok_or_else(|| AdmissionError::envelope("action actionId must be text"))?;
    if action_id.len() != 64 || !action_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AdmissionError::envelope(
            "actionId must be a SHA-256 hex digest",
        ));
    }
    let payload = fields["payload"]
        .as_object()
        .ok_or_else(|| AdmissionError::payload("action payload must be an object"))?;
    let type_name = payload
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| AdmissionError::payload("action payload type must be text"))?;
    let rule = OBJECTS
        .iter()
        .find(|rule| rule.name == type_name)
        .ok_or_else(|| {
            AdmissionError::payload(format!("unknown action payload type {type_name}"))
        })?;
    require_exact_fields(
        payload,
        COMMON,
        rule.required,
        type_name,
        AdmissionError::payload,
    )?;
    if let Some(field) = payload.keys().find(|field| {
        matches!(
            field.as_str(),
            "positionKey" | "position_key" | "positionId" | "actionId"
        ) || rule.exact_fields
            && !COMMON.contains(&field.as_str())
            && !rule.required.contains(&field.as_str())
            && !rule.optional.contains(&field.as_str())
    }) {
        return Err(AdmissionError::payload(format!(
            "unexpected {type_name} payload field {field}"
        )));
    }
    validate_kind_shape(position.state(), type_name, payload)?;
    let calculated = source_action_id(&fields["payload"])?;
    if calculated != action_id {
        return Err(AdmissionError::envelope(
            "actionId does not match the exact source payload",
        ));
    }
    let mut action: Action =
        serde_json::from_value(fields["payload"].clone()).map_err(|error| {
            AdmissionError::payload(format!("action payload cannot decode: {error}"))
        })?;
    // serde's `Option<Value>` maps explicit JSON null to None. The pinned
    // action protocol distinguishes an absent target from `"target":null`
    // in both action identity and candidate membership.
    if type_name == "card" && payload.get("target") == Some(&Value::Null) {
        action.target = Some(Value::Null);
    }
    if action.position_key.is_some() {
        return Err(AdmissionError::payload(
            "client must not provide an internal position_key",
        ));
    }
    if position.state().result().is_some() || position.state().mode == "gameover" {
        return Err(AdmissionError::new(
            AdmissionErrorKind::Terminal,
            "terminal_action_position",
            "game is terminal",
        ));
    }
    if action.color != position.state().decision_actor() {
        return Err(AdmissionError::new(
            AdmissionErrorKind::WrongActor,
            "wrong_action_actor",
            "action color is not the current decision actor",
        ));
    }
    Ok(DecodedV7Action {
        action,
        source_payload: fields["payload"].clone(),
        action_id: action_id.to_owned(),
        position_id: position_id.to_owned(),
    })
}

fn require_exact_fields(
    fields: &Map<String, Value>,
    common: &[&str],
    required: &[&str],
    label: &str,
    error: fn(String) -> AdmissionError,
) -> Result<(), AdmissionError> {
    if let Some(name) = common
        .iter()
        .chain(required)
        .find(|name| !fields.contains_key(**name))
    {
        return Err(error(format!("{label} is missing required field {name}")));
    }
    // The envelope has no optional keys. Payload optional keys are checked by
    // its owning action object after the required-fields check.
    if label == "action envelope"
        && let Some(name) = fields.keys().find(|name| !common.contains(&name.as_str()))
    {
        return Err(error(format!("unexpected action envelope field {name}")));
    }
    Ok(())
}

fn source_action_id(payload: &Value) -> Result<String, AdmissionError> {
    let bytes = serde_jcs::to_vec(payload)
        .map_err(|error| AdmissionError::payload(format!("invalid canonical action: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn square(state: &GameState, value: Option<&Value>, label: &str) -> Result<(), AdmissionError> {
    let fields = value.and_then(Value::as_object).ok_or_else(|| {
        AdmissionError::payload(format!("{label} must be a board coordinate object"))
    })?;
    if fields.len() != 2
        || !fields.contains_key("row")
        || !fields.contains_key("col")
        || !fields
            .get("row")
            .and_then(Value::as_u64)
            .zip(fields.get("col").and_then(Value::as_u64))
            .is_some_and(|(row, col)| {
                row <= u8::MAX as u64
                    && col <= u8::MAX as u64
                    && state
                        .board
                        .get(row as usize)
                        .is_some_and(|cells| (col as usize) < cells.len())
            })
    {
        return Err(AdmissionError::payload(format!(
            "{label} requires only integer row and col within the current board"
        )));
    }
    Ok(())
}

fn nonempty_text(value: Option<&Value>, label: &str) -> Result<(), AdmissionError> {
    if value.and_then(Value::as_str).is_none_or(str::is_empty) {
        return Err(AdmissionError::payload(format!(
            "{label} must be nonempty text"
        )));
    }
    Ok(())
}

fn validate_kind_shape(
    state: &GameState,
    name: &str,
    payload: &Map<String, Value>,
) -> Result<(), AdmissionError> {
    if !matches!(
        payload.get("color").and_then(Value::as_str),
        Some("white" | "black")
    ) {
        return Err(AdmissionError::payload(
            "action color must be white or black",
        ));
    }
    if matches!(
        name,
        "move" | "promotion" | "shotgunReload" | "wizardSpell" | "fileSurgeSkip"
    ) {
        square(state, payload.get("from"), "from")?;
    }
    match name {
        "move" => {
            if payload
                .get("forcedFriendlyCrush")
                .is_some_and(|value| !value.is_boolean())
            {
                return Err(AdmissionError::payload(
                    "forcedFriendlyCrush must be boolean",
                ));
            }
            let target = payload
                .get("move")
                .and_then(Value::as_object)
                .ok_or_else(|| AdmissionError::payload("move must be a destination object"))?;
            // Source move flags are part of action identity. Their meaning is
            // delegated to the exact candidate validator below.
            if target.get("row").and_then(Value::as_u64).is_none()
                || target.get("col").and_then(Value::as_u64).is_none()
                || target
                    .get("row")
                    .and_then(Value::as_u64)
                    .zip(target.get("col").and_then(Value::as_u64))
                    .is_none_or(|(row, col)| {
                        row > u8::MAX as u64
                            || col > u8::MAX as u64
                            || state
                                .board
                                .get(row as usize)
                                .is_none_or(|cells| col as usize >= cells.len())
                    })
            {
                return Err(AdmissionError::payload(
                    "move requires integer row and col within the current board",
                ));
            }
        }
        "card" => {
            nonempty_text(payload.get("cardId"), "cardId")?;
            nonempty_text(payload.get("cardInstanceId"), "cardInstanceId")?;
            if payload
                .get("target")
                .is_some_and(|value| !value.is_null() && !value.is_object())
            {
                return Err(AdmissionError::payload(
                    "card target must be an object or null",
                ));
            }
        }
        "promotionChoice" => nonempty_text(payload.get("promotionType"), "promotionType")?,
        "wizardSpell" => {
            if !matches!(
                payload.get("spellId").and_then(Value::as_str),
                Some("meteor" | "lightning" | "shield" | "timeStop")
            ) {
                return Err(AdmissionError::payload("unknown wizard spellId"));
            }
            square(state, payload.get("target"), "wizard target")?;
        }
        "draftPick" => nonempty_text(payload.get("cardInstanceId"), "cardInstanceId")?,
        "draftBundlePick" => {
            if payload
                .get("bundleIndex")
                .and_then(Value::as_u64)
                .is_none_or(|index| index > 2)
            {
                return Err(AdmissionError::payload("bundleIndex must be 0, 1 or 2"));
            }
            let ids = payload
                .get("cardInstanceIds")
                .and_then(Value::as_array)
                .ok_or_else(|| AdmissionError::payload("cardInstanceIds must be an array"))?;
            if ids.len() != 2 || ids.iter().any(|id| id.as_str().is_none_or(str::is_empty)) {
                return Err(AdmissionError::payload(
                    "cardInstanceIds must contain exactly two nonempty strings",
                ));
            }
        }
        "trolleyChoice" => {
            nonempty_text(payload.get("windowId"), "windowId")?;
            if !matches!(
                payload.get("doomedIndex").and_then(Value::as_u64),
                Some(0 | 1)
            ) {
                return Err(AdmissionError::payload("doomedIndex must be 0 or 1"));
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameConfig, RngState};
    use serde_json::json;

    fn host(seed: u64) -> V7HostPosition {
        V7HostPosition::from_parts(
            json!({"board":vec![vec![Value::Null;8];8],"turn":"white","mode":"play"}),
            RngState::seeded(seed),
            Vec::new(),
        )
        .unwrap()
    }

    fn envelope(position: &V7HostPosition, payload: Value) -> Value {
        json!({
            "protocolVersion":V7_ACTION_PROTOCOL,
            "positionId":position.position_id(),
            "actionId":source_action_id(&payload).unwrap(),
            "payload":payload,
        })
    }

    fn draft_host(style: &str) -> V7HostPosition {
        let state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        V7HostPosition::from_state(state).unwrap()
    }

    #[test]
    fn source_offer_binds_once_and_preserves_exact_payload_and_position() {
        // Direct pinned-client `GameAdapter.actions(newGame({gameStyle}, 19))[0]`
        // action IDs. This independently checks Rust JCS hashing and the
        // corrected source draft identities across all three styles.
        for (style, expected_action_id) in [
            (
                "normal",
                "dc30f0af0259dc2c3cf7cdb5f97c992a55d7562b79c087e5eab0896e771f79ad",
            ),
            (
                "chaos",
                "feed8e68ca6e44c0d0560be2d9ae575b6f398bd40aa22ff82e835d09c6765c6c",
            ),
            (
                "grand",
                "f4203369d835173d1d97bf81b0ace3be6c81951bd10913d12751455cb2ead8e6",
            ),
        ] {
            let position = draft_host(style);
            let before = position.export_envelope().unwrap();
            let selected = crate::draft::legal_actions(position.state())
                .unwrap()
                .remove(0);
            assert_eq!(selected.color, position.state().decision_actor());
            let payload = serde_json::to_value(&selected).unwrap();
            let wire = envelope(&position, payload.clone());
            let action_id = wire["actionId"].as_str().unwrap().to_owned();
            assert_eq!(action_id, expected_action_id, "{style}");
            let bound = admit_v7_action(&position, wire).unwrap();
            assert_eq!(bound.source_payload(), &payload);
            assert_eq!(bound.action_id(), action_id);
            assert!(bound.action().position_key.is_none());
            bound.revalidate(&position).unwrap();
            assert_eq!(position.export_envelope().unwrap(), before);
        }
    }

    #[test]
    fn stale_actor_identity_and_arbitrary_fields_fail_without_state_rng_or_history_change() {
        let position = draft_host("normal");
        let other = draft_host("chaos");
        let before = position.export_envelope().unwrap();
        let selected = crate::draft::legal_actions(position.state())
            .unwrap()
            .remove(0);
        let payload = serde_json::to_value(&selected).unwrap();
        let bound = admit_v7_action(&position, envelope(&position, payload.clone())).unwrap();

        let wrong_actor =
            json!({"type":"draftPick","color":"black","cardInstanceId":selected.card_instance_id});
        assert_eq!(
            admit_v7_action(&position, envelope(&position, wrong_actor))
                .unwrap_err()
                .kind,
            AdmissionErrorKind::WrongActor
        );
        let not_offered =
            json!({"type":"draftPick","color":"white","cardInstanceId":"not-offered"});
        assert_eq!(
            admit_v7_action(&position, envelope(&position, not_offered))
                .unwrap_err()
                .kind,
            AdmissionErrorKind::IllegalAction
        );
        let extra = json!({"type":"draftPick","color":"white","cardInstanceId":selected.card_instance_id,"unexpected":true});
        assert_eq!(
            admit_v7_action(&position, envelope(&position, extra))
                .unwrap_err()
                .kind,
            AdmissionErrorKind::InvalidPayload
        );
        let mut corrupted = envelope(&position, payload.clone());
        corrupted["actionId"] = json!("0".repeat(64));
        assert_eq!(
            admit_v7_action(&position, corrupted).unwrap_err().kind,
            AdmissionErrorKind::InvalidEnvelope
        );
        let mut injected = envelope(&position, payload);
        injected["extra"] = json!(true);
        assert_eq!(
            admit_v7_action(&position, injected).unwrap_err().kind,
            AdmissionErrorKind::InvalidEnvelope
        );
        assert_eq!(
            bound.revalidate(&other).unwrap_err().kind,
            AdmissionErrorKind::StalePosition
        );
        assert_eq!(position.export_envelope().unwrap(), before);
    }

    #[test]
    fn each_object_rejects_forbidden_or_missing_fields_before_rule_dispatch() {
        let position = host(19);
        let malformed = [
            json!({"type":"move","color":"white","from":{"row":6,"col":0},"move":{"row":5,"col":0},"cardId":"injected"}),
            json!({"type":"move","color":"white","from":{"row":8,"col":0},"move":{"row":5,"col":0}}),
            json!({"type":"move","color":"white","from":{"row":6,"col":0},"move":{"row":5,"col":0},"positionKey":"external"}),
            json!({"type":"card","color":"white","cardId":"relay"}),
            json!({"type":"card","color":"white","cardId":"relay","cardInstanceId":"x","target":[]}),
            json!({"type":"draftPick","color":"white","cardInstanceId":"x","from":{"row":6,"col":0}}),
            json!({"type":"draftBundlePick","color":"white","bundleIndex":3,"cardInstanceIds":["a","b"]}),
            json!({"type":"wizardSpell","color":"white","from":{"row":6,"col":0},"spellId":"unknown","target":{"row":5,"col":0}}),
            json!({"type":"trolleyChoice","color":"white","windowId":"x","doomedIndex":2}),
        ];
        for payload in malformed {
            let error = admit_v7_action(&position, envelope(&position, payload)).unwrap_err();
            assert_eq!(
                error.kind,
                AdmissionErrorKind::InvalidPayload,
                "{}",
                error.detail
            );
        }
    }

    #[test]
    fn coordinate_shape_uses_the_imported_board_extent() {
        let position = host(19);
        let inside = json!({"type":"move","color":"white","from":{"row":7,"col":7},"move":{"row":6,"col":7}});
        let outside = json!({"type":"move","color":"white","from":{"row":8,"col":7},"move":{"row":6,"col":7}});
        // The inside coordinate reaches rule admission, which may be an
        // unsupported profile or an ordinary illegal move on an empty board.
        // It is not a malformed coordinate shape.
        assert!(matches!(
            admit_v7_action(&position, envelope(&position, inside))
                .unwrap_err()
                .kind,
            AdmissionErrorKind::Unsupported | AdmissionErrorKind::IllegalAction
        ));
        assert_eq!(
            admit_v7_action(&position, envelope(&position, outside))
                .unwrap_err()
                .kind,
            AdmissionErrorKind::InvalidPayload,
        );
    }

    #[test]
    fn source_reject_examples_keep_exact_error_categories_and_original_position() {
        // Pinned source seed-19 draft rejects wrong actor as "Wrong acting
        // player." and an unoffered instance as "Draft choice is not in the
        // current offer.". These assertions keep the corresponding Rust
        // categories distinct while preserving the caller's immutable state.
        let draft = draft_host("normal");
        let before_draft = draft.export_envelope().unwrap();
        let wrong_actor = envelope(
            &draft,
            json!({"type":"draftPick","color":"black","cardInstanceId":"unavailable"}),
        );
        let not_offered = envelope(
            &draft,
            json!({"type":"draftPick","color":"white","cardInstanceId":"unavailable"}),
        );
        assert_eq!(
            admit_v7_action(&draft, wrong_actor).unwrap_err().code,
            "wrong_action_actor"
        );
        assert_eq!(
            admit_v7_action(&draft, not_offered).unwrap_err().code,
            "illegal_action"
        );
        assert_eq!(draft.export_envelope().unwrap(), before_draft);
    }

    #[test]
    fn pinned_first_play_move_binds_but_source_rejected_coordinate_does_not() {
        let mut state =
            crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7)
                .unwrap();
        for index in [1, 0] {
            let pick = crate::draft::legal_actions(&state).unwrap().remove(index);
            crate::draft::apply_pick(&mut state, &pick).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        let position = V7HostPosition::from_state(state).unwrap();
        let before = position.export_envelope().unwrap();
        let legal = json!({"type":"move","color":"white","from":{"row":6,"col":0},"move":{"row":5,"col":0}});
        let expected_action_id = "d7b376e979d3b7a5d7a37e7f53c48603b53bc6a31454d8430a4c90aefbec984a";
        assert_eq!(source_action_id(&legal).unwrap(), expected_action_id);
        let bound = admit_v7_action(&position, envelope(&position, legal)).unwrap();
        assert_eq!(bound.action_id(), expected_action_id);
        bound.revalidate(&position).unwrap();

        // Relay is the sole card in this opening. A client cannot turn its
        // absent target into an explicit null while keeping the same meaning.
        let relay = crate::v7_action_surface::legal_source_actions(position.state())
            .unwrap()
            .into_iter()
            .find(|action| action.kind == crate::ActionKind::Card)
            .unwrap();
        let relay_payload = serde_json::to_value(&relay).unwrap();
        assert!(relay_payload.get("target").is_none());
        admit_v7_action(&position, envelope(&position, relay_payload.clone())).unwrap();
        let mut null_target = relay_payload;
        null_target["target"] = Value::Null;
        assert_eq!(
            admit_v7_action(&position, envelope(&position, null_target))
                .unwrap_err()
                .kind,
            AdmissionErrorKind::IllegalAction
        );

        // Source `applyAiAction` rejects a2->b4 with "AI move is no longer
        // legal" on this exact seed-19 first-play snapshot.
        let illegal = json!({"type":"move","color":"white","from":{"row":6,"col":0},"move":{"row":4,"col":1}});
        let error = admit_v7_action(&position, envelope(&position, illegal)).unwrap_err();
        assert_eq!(error.kind, AdmissionErrorKind::IllegalAction);
        assert_eq!(position.export_envelope().unwrap(), before);
    }

    #[test]
    fn absent_exact_card_and_unknown_fields_fail_before_effect_dispatch() {
        let position = host(19);
        let before = position.export_envelope().unwrap();
        let absent = json!({"type":"card","color":"white","cardId":"relay","cardInstanceId":"white-relay-1"});
        let explicit_null = json!({"type":"card","color":"white","cardId":"relay","cardInstanceId":"white-relay-1","target":null});
        assert_ne!(
            source_action_id(&absent).unwrap(),
            source_action_id(&explicit_null).unwrap()
        );
        for payload in [absent, explicit_null] {
            let error = admit_v7_action(&position, envelope(&position, payload)).unwrap_err();
            assert_eq!(error.kind, AdmissionErrorKind::IllegalAction);
        }
        let unverified_extra = envelope(
            &position,
            json!({"type":"card","color":"white","cardId":"relay","cardInstanceId":"white-relay-1","sourceFutureField":true}),
        );
        assert_eq!(
            admit_v7_action(&position, unverified_extra)
                .unwrap_err()
                .kind,
            AdmissionErrorKind::InvalidPayload,
        );
        let internal_key = envelope(
            &position,
            json!({"type":"card","color":"white","cardId":"relay","cardInstanceId":"white-relay-1","positionKey":"injected"}),
        );
        assert_eq!(
            admit_v7_action(&position, internal_key).unwrap_err().kind,
            AdmissionErrorKind::InvalidPayload,
        );
        assert_eq!(position.export_envelope().unwrap(), before);
    }

    #[test]
    fn verified_set_reuses_exact_membership_and_rejects_another_revision() {
        let position = draft_host("normal");
        let verified = VerifiedV7ActionSet::complete(&position).unwrap();
        for (_, payload, _) in verified.source_entries() {
            let admitted = admit_v7_action_from_set(
                &position,
                envelope(&position, payload.clone()),
                verified.clone(),
            )
            .unwrap();
            admitted.revalidate(&position).unwrap();
        }
        let payload = verified.source_entries().next().unwrap().1.clone();
        let admitted =
            admit_v7_action_from_set(&position, envelope(&position, payload), verified).unwrap();
        let (later_revision, ()) = position
            .transact(position.position_id(), |_| Ok(()))
            .unwrap();
        assert_eq!(later_revision.position_id(), position.position_id());
        assert_ne!(later_revision.revision(), position.revision());
        assert_eq!(
            admitted.revalidate(&later_revision).unwrap_err().kind,
            AdmissionErrorKind::StalePosition
        );
    }
}
