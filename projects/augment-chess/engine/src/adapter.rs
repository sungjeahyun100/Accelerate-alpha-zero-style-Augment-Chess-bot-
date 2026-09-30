//! Source-versioned, game-owned consumers of the project-independent adapter runtime.
//!
//! The registry is sealed per session. A v7 host owns the position, RNG and
//! history; the shared runtime never sees an untyped game state or chooses a
//! fallback implementation. Capabilities are admitted only as their source
//! semantics become independently verified.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Mutex, OnceLock},
};

use adapter_runtime::{
    AdapterDescriptor, AdapterError, AdapterErrorKind, AdapterHost, AdapterObject, AdapterOutput,
    AdapterRegistry, AdapterRequest, AdapterResponse, CallLimits, CallMeter, CapabilityAccess,
    CapabilityDescriptor, CommittedRevision, ContractVersion, InvocationControl, PageInfo,
    SchemaRef,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::v7_action_admission::AdmissionErrorKind;
use crate::v7_adapter_actions::V7ActionHostError;
use crate::{Color, EngineError, GameConfig, GameResult, Observation, V7HostPosition};

pub const PROJECT_ID: &str = "augment-chess";
pub const OBSERVATION_ADAPTER_ID: &str = "public-observation";
pub const OBSERVATION_CAPABILITY_ID: &str = "observe";
pub const ACTION_ADAPTER_ID: &str = "public-actions";
pub const LEGAL_ACTIONS_CAPABILITY_ID: &str = "legal-actions";
pub const LEGAL_ACTIONS_PAGE_CAPABILITY_ID: &str = "legal-actions-page";
pub const BIND_INTENT_CAPABILITY_ID: &str = "bind-public-intent";
pub const APPLY_INTENT_CAPABILITY_ID: &str = "apply-public-intent";
pub const IMPLEMENTATION_VERSION: &str = "v7-e5ed84fcf8e72a24-0.1.0";

const REQUEST_SCHEMA_ID: &str = "urn:augment-chess:adapter:observe-request:v1";
const REQUEST_SCHEMA_SHA: &str = "ae667bff6b5000cd8ad8fd28a8e7b012adeee3f9560b8327e69e7e56fb4301ba";
const REQUEST_SCHEMA: &str =
    include_str!("../../contracts/schemas/adapter-observe-request-v1.schema.json");
const RESPONSE_SCHEMA_ID: &str = "urn:augment-chess:adapter:observe-response:v1";
const RESPONSE_SCHEMA_SHA: &str =
    "d26d40915cd92caa3be92f784db2faf8f9bb584c1ad410df4502f037239f581e";
const RESPONSE_SCHEMA: &str =
    include_str!("../../contracts/schemas/adapter-observe-response-v1.schema.json");
const ACTION_REQUEST_SCHEMA_ID: &str = "urn:augment-chess:adapter:actions-request:v1";
const ACTION_REQUEST_SCHEMA_SHA: &str =
    "5e2453681830f42bca4045bebee8864d452510cdf1a7ad49fea113729f80456c";
const ACTION_REQUEST_SCHEMA: &str =
    include_str!("../../contracts/schemas/adapter-actions-request-v1.schema.json");
const ACTION_RESPONSE_SCHEMA_ID: &str = "urn:augment-chess:adapter:actions-response:v1";
const ACTION_RESPONSE_SCHEMA_SHA: &str =
    "369d047d7fb9236a5b023cdefbd730ea9672ddb5b8f0c31c63cef196739075b6";
const ACTION_RESPONSE_SCHEMA: &str =
    include_str!("../../contracts/schemas/adapter-actions-response-v1.schema.json");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GameAdapterPayload {
    Observe {
        viewer: Color,
    },
    LegalActions,
    LegalActionsPage {
        limit: usize,
        max_examined: usize,
        cursor: Option<String>,
    },
    BindPublicIntent {
        intent: Value,
    },
    ApplyPublicIntent {
        intent: Value,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GameAdapterValue {
    Observation {
        observation: Observation,
    },
    LegalActions {
        intents: Vec<Value>,
    },
    LegalActionsPage {
        intents: Vec<Value>,
        examined: usize,
        exhausted: bool,
        stop_reason: String,
        cursor: Option<String>,
    },
    BoundPublicIntent {
        intent: Value,
    },
    AppliedPublicIntent {
        actor: Color,
        turn_changed: bool,
        result: Option<GameResult>,
    },
}

/// A typed game facade. Callers supply the exact descriptor selection and
/// schema references in each request, rather than relying on implicit latest.
pub struct GameAdapterSession {
    host: GameAdapterHost,
    registry: AdapterRegistry<V7HostPosition, GameAdapterPayload, GameAdapterValue>,
}

impl GameAdapterSession {
    /// Construct only an opening profile that the frozen v7 client has been
    /// compared against. The game constructor bounds accepted RULE combinations.
    pub fn new_game(config: GameConfig, seed: u64) -> Result<Self, AdapterError> {
        let state = crate::v7_new_game::new_game(config, seed).map_err(map_engine_error)?;
        let position = V7HostPosition::from_state(state).map_err(map_engine_error)?;
        Self::new(position)
    }

    /// 동결 로컬 8×8 장기의 cold preview, 같은 RNG의 reset(false), 실제
    /// 캠페인 초기화를 거쳐 세션을 만든다. online 캠페인 상태는 별도 권한이다.
    pub fn new_local_janggi(
        config: GameConfig,
        seed: u64,
        player: Color,
        local_mode: &str,
    ) -> Result<Self, AdapterError> {
        let state = crate::v7_new_game::new_local_janggi(config, seed, player, local_mode)
            .map_err(map_engine_error)?;
        let position = V7HostPosition::from_state(state).map_err(map_engine_error)?;
        Self::new(position)
    }

    pub fn from_envelope(envelope: Value) -> Result<Self, AdapterError> {
        let position = V7HostPosition::from_envelope(envelope).map_err(map_engine_error)?;
        Self::new(position)
    }

    pub fn new(position: V7HostPosition) -> Result<Self, AdapterError> {
        verify_schema(REQUEST_SCHEMA, REQUEST_SCHEMA_ID, REQUEST_SCHEMA_SHA)?;
        verify_schema(RESPONSE_SCHEMA, RESPONSE_SCHEMA_ID, RESPONSE_SCHEMA_SHA)?;
        verify_schema(
            ACTION_REQUEST_SCHEMA,
            ACTION_REQUEST_SCHEMA_ID,
            ACTION_REQUEST_SCHEMA_SHA,
        )?;
        verify_schema(
            ACTION_RESPONSE_SCHEMA,
            ACTION_RESPONSE_SCHEMA_ID,
            ACTION_RESPONSE_SCHEMA_SHA,
        )?;
        if !position.state().history.is_empty() {
            crate::v7_replay::validate_history(position.state()).map_err(map_engine_error)?;
        }
        let mut registry = AdapterRegistry::new();
        registry.register(PublicObservationObject)?;
        registry.register(PublicActionObject::default())?;
        registry.seal();
        Ok(Self {
            host: GameAdapterHost { position },
            registry,
        })
    }

    pub fn position(&self) -> &V7HostPosition {
        &self.host.position
    }

    pub fn descriptors(&self) -> Vec<&AdapterDescriptor> {
        self.registry.descriptors()
    }

    pub fn invoke(
        &mut self,
        request: &AdapterRequest<GameAdapterPayload>,
        control: &InvocationControl<'_>,
    ) -> Result<AdapterResponse<GameAdapterValue>, AdapterError> {
        self.registry.invoke(&mut self.host, request, control)
    }
}

struct GameAdapterHost {
    position: V7HostPosition,
}

impl AdapterHost<V7HostPosition> for GameAdapterHost {
    type Transaction = Box<V7HostPosition>;

    fn revision(&self) -> &str {
        self.position.position_id()
    }

    fn read_state(&self) -> &V7HostPosition {
        &self.position
    }

    fn begin_transaction(&mut self) -> Result<Self::Transaction, AdapterError> {
        Ok(Box::new(self.position.clone()))
    }

    fn commit_transaction(
        &mut self,
        transaction: Self::Transaction,
        expected_revision: &str,
    ) -> Result<CommittedRevision, (AdapterError, Self::Transaction)> {
        if self.position.position_id() != expected_revision {
            return Err((map_engine_error(EngineError::StaleAction), transaction));
        }
        let next = *transaction;
        if self.position.revision().checked_add(1) != Some(next.revision()) {
            return Err((
                AdapterError::execution_failed(
                    "uncommitted_game_position",
                    "transaction must contain exactly one committed v7 host revision",
                ),
                Box::new(next),
            ));
        }
        let revision = match CommittedRevision::new(next.position_id().to_owned()) {
            Ok(revision) => revision,
            Err(error) => return Err((error, Box::new(next))),
        };
        self.position = next;
        Ok(revision)
    }

    fn rollback_transaction(&mut self, transaction: Self::Transaction) {
        drop(transaction);
    }
}

struct PublicObservationObject;

impl AdapterObject<V7HostPosition, GameAdapterPayload, GameAdapterValue>
    for PublicObservationObject
{
    fn descriptor(&self) -> &AdapterDescriptor {
        static DESCRIPTOR: OnceLock<AdapterDescriptor> = OnceLock::new();
        DESCRIPTOR.get_or_init(|| AdapterDescriptor {
            project_id: PROJECT_ID.into(),
            adapter_id: OBSERVATION_ADAPTER_ID.into(),
            contract_version: ContractVersion { major: 1, minor: 0 },
            implementation_version: IMPLEMENTATION_VERSION.into(),
            capabilities: vec![CapabilityDescriptor {
                id: OBSERVATION_CAPABILITY_ID.into(),
                request_schema: SchemaRef {
                    id: REQUEST_SCHEMA_ID.into(),
                    sha256: REQUEST_SCHEMA_SHA.into(),
                },
                response_schema: SchemaRef {
                    id: RESPONSE_SCHEMA_ID.into(),
                    sha256: RESPONSE_SCHEMA_SHA.into(),
                },
                access: CapabilityAccess::ReadOnly,
            }],
            deterministic: true,
            call_limits: CallLimits {
                max_work: 100_000,
                max_results: 1,
            },
        })
    }

    fn invoke_read(
        &self,
        position: &V7HostPosition,
        request: &AdapterRequest<GameAdapterPayload>,
        meter: &mut CallMeter<'_>,
    ) -> Result<AdapterOutput<GameAdapterValue>, AdapterError> {
        let GameAdapterPayload::Observe { viewer } = &request.payload else {
            return Err(AdapterError::invalid_input(
                "observation_payload_mismatch",
                "observe capability requires an observe payload",
            ));
        };
        let state = position.state();
        // Work units are source cells/cards/events visited during projection.
        let cells = state
            .board
            .iter()
            .try_fold(0usize, |count, row| count.checked_add(row.len()));
        let work = cells
            .and_then(|count| count.checked_add(state.deck_slots.white.len()))
            .and_then(|count| count.checked_add(state.deck_slots.black.len()))
            .and_then(|count| count.checked_add(state.history.len()))
            .and_then(|count| count.checked_add(1))
            .ok_or_else(|| {
                AdapterError::new(
                    AdapterErrorKind::LimitExceeded,
                    "observation_work_overflow",
                    "observation projection work count overflowed",
                )
            })?;
        let work = u64::try_from(work).map_err(|_| {
            AdapterError::new(
                AdapterErrorKind::LimitExceeded,
                "observation_work_overflow",
                "observation projection work count exceeds u64",
            )
        })?;
        meter.consume_work(work)?;
        let observation = state.try_observe(*viewer).map_err(map_engine_error)?;
        meter.consume_results(1)?;
        Ok(AdapterOutput::value(GameAdapterValue::Observation {
            observation,
        }))
    }

    fn validate_output(
        &self,
        capability_id: &str,
        result: &GameAdapterValue,
    ) -> Result<u64, AdapterError> {
        match (capability_id, result) {
            (OBSERVATION_CAPABILITY_ID, GameAdapterValue::Observation { observation })
                if !observation.protocol_version.is_empty()
                    && !observation.information_state_key.is_empty() =>
            {
                Ok(1)
            }
            _ => Err(AdapterError::execution_failed(
                "invalid_observation_result",
                "observation adapter emitted a result incompatible with its capability",
            )),
        }
    }
}

/// The game adapter owns the exact public-intent boundary. Its read path
/// exposes source-ordered payloads, never host action IDs or hidden-state
/// envelopes. A write is rebound against the current host Position inside a
/// registry transaction, so failure or cancellation leaves the host intact.
/// Opaque cursors are session-local, bounded host resources. They never enter
/// game snapshots or expose the source cursor's private candidates. A retained
/// cursor denotes an immutable stream position, so retries produce the same
/// result and token until the documented 64-position retention bound is hit.
#[derive(Default)]
struct PublicActionObject {
    cursors: Mutex<PublicCursorStore>,
}

#[derive(Default)]
struct PublicCursorStore {
    entries: BTreeMap<String, crate::v7_adapter_actions::V7PublicActionCursor>,
    insertion_order: VecDeque<String>,
}

impl PublicActionObject {
    fn page(
        &self,
        position: &V7HostPosition,
        request: &AdapterRequest<GameAdapterPayload>,
        limit: usize,
        max_examined: usize,
        cursor: Option<&str>,
        meter: &mut CallMeter<'_>,
    ) -> Result<AdapterOutput<GameAdapterValue>, AdapterError> {
        if !(1..=4096).contains(&limit)
            || !(1..=65536).contains(&max_examined)
            || cursor.is_some_and(|token| {
                token.len() != 68
                    || !token.starts_with("v7p-")
                    || !token[4..].bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            return Err(AdapterError::invalid_input(
                "invalid_action_page",
                "page limit must be 1..4096, max_examined 1..65536, and cursor an issued opaque token or null",
            ));
        }
        // Admit a bounded amount of work before constructing/probing candidates.
        if 1 + max_examined as u64 > request.call_limits.max_work
            || limit as u64 > request.call_limits.max_results
        {
            return Err(AdapterError::new(
                AdapterErrorKind::LimitExceeded,
                "action_page_budget_exceeded",
                "page bounds exceed this invocation's work or result budget",
            ));
        }
        meter.checkpoint()?;
        let mut store = self.cursors.lock().map_err(|_| {
            AdapterError::execution_failed(
                "action_cursor_store_poisoned",
                "public action cursor store mutex was poisoned",
            )
        })?;
        let mut staged = if let Some(token) = cursor {
            store.entries.get(token).cloned().ok_or_else(|| {
                AdapterError::invalid_input(
                    "action_cursor_unavailable",
                    "cursor belongs to another session or exceeded the 64-position retention bound",
                )
            })?
        } else {
            crate::v7_adapter_actions::V7PublicActionCursor::new(position, None)
                .map_err(map_v7_action_error)?
        };
        let page = staged
            .next_page(position, limit, max_examined)
            .map_err(map_v7_action_error)?;
        meter.consume_examined(page.examined as u64)?;
        meter.consume_results(page.intents.len() as u64)?;
        let next_cursor = if page.exhausted {
            None
        } else {
            let mut hash = Sha256::new();
            hash.update(b"augment-chess-public-action-page-v1\0");
            hash.update(position.position_id().as_bytes());
            hash.update(position.revision().to_be_bytes());
            hash.update(cursor.unwrap_or("").as_bytes());
            hash.update((limit as u64).to_be_bytes());
            hash.update((max_examined as u64).to_be_bytes());
            Some(format!("v7p-{:x}", hash.finalize()))
        };
        // Budget/cancellation failures preserve every retained stream position.
        meter.checkpoint()?;
        if let Some(token) = &next_cursor {
            if !store.entries.contains_key(token) {
                while store.entries.len() >= 64 {
                    let oldest = store.insertion_order.pop_front().ok_or_else(|| {
                        AdapterError::execution_failed(
                            "invalid_action_cursor_store",
                            "cursor retention order is inconsistent",
                        )
                    })?;
                    store.entries.remove(&oldest);
                }
                store.insertion_order.push_back(token.clone());
                store.entries.insert(token.clone(), staged);
            }
        }
        let mut output = AdapterOutput::value(GameAdapterValue::LegalActionsPage {
            intents: page.intents,
            examined: page.examined,
            exhausted: page.exhausted,
            stop_reason: page.stop_reason.into(),
            cursor: next_cursor.clone(),
        });
        output.page = Some(PageInfo {
            examined: page.examined as u64,
            cursor: next_cursor,
            exhausted: page.exhausted,
        });
        Ok(output)
    }
}

impl AdapterObject<V7HostPosition, GameAdapterPayload, GameAdapterValue> for PublicActionObject {
    fn descriptor(&self) -> &AdapterDescriptor {
        static DESCRIPTOR: OnceLock<AdapterDescriptor> = OnceLock::new();
        DESCRIPTOR.get_or_init(|| {
            let capability = |id: &str, access| CapabilityDescriptor {
                id: id.into(),
                request_schema: SchemaRef {
                    id: ACTION_REQUEST_SCHEMA_ID.into(),
                    sha256: ACTION_REQUEST_SCHEMA_SHA.into(),
                },
                response_schema: SchemaRef {
                    id: ACTION_RESPONSE_SCHEMA_ID.into(),
                    sha256: ACTION_RESPONSE_SCHEMA_SHA.into(),
                },
                access,
            };
            AdapterDescriptor {
                project_id: PROJECT_ID.into(),
                adapter_id: ACTION_ADAPTER_ID.into(),
                contract_version: ContractVersion { major: 1, minor: 0 },
                implementation_version: IMPLEMENTATION_VERSION.into(),
                capabilities: vec![
                    capability(LEGAL_ACTIONS_CAPABILITY_ID, CapabilityAccess::ReadOnly),
                    capability(LEGAL_ACTIONS_PAGE_CAPABILITY_ID, CapabilityAccess::ReadOnly),
                    capability(BIND_INTENT_CAPABILITY_ID, CapabilityAccess::ReadOnly),
                    capability(APPLY_INTENT_CAPABILITY_ID, CapabilityAccess::Transactional),
                ],
                deterministic: true,
                call_limits: CallLimits {
                    max_work: 100_000,
                    max_results: 4096,
                },
            }
        })
    }

    fn invoke_read(
        &self,
        position: &V7HostPosition,
        request: &AdapterRequest<GameAdapterPayload>,
        meter: &mut CallMeter<'_>,
    ) -> Result<AdapterOutput<GameAdapterValue>, AdapterError> {
        meter.consume_work(1)?;
        match (&request.selection.capability_id[..], &request.payload) {
            (
                LEGAL_ACTIONS_PAGE_CAPABILITY_ID,
                GameAdapterPayload::LegalActionsPage {
                    limit,
                    max_examined,
                    cursor,
                },
            ) => self.page(
                position,
                request,
                *limit,
                *max_examined,
                cursor.as_deref(),
                meter,
            ),
            (LEGAL_ACTIONS_CAPABILITY_ID, GameAdapterPayload::LegalActions) => {
                let intents = crate::v7_adapter_actions::legal_public_intents(position)
                    .map_err(map_v7_action_error)?;
                meter.consume_work(intents.len() as u64)?;
                meter.consume_results(intents.len() as u64)?;
                Ok(AdapterOutput::value(GameAdapterValue::LegalActions {
                    intents,
                }))
            }
            (BIND_INTENT_CAPABILITY_ID, GameAdapterPayload::BindPublicIntent { intent }) => {
                let _bound =
                    crate::v7_adapter_actions::bind_public_intent(position, intent.clone())
                        .map_err(map_v7_action_error)?;
                meter.consume_results(1)?;
                Ok(AdapterOutput::value(GameAdapterValue::BoundPublicIntent {
                    intent: intent.clone(),
                }))
            }
            _ => Err(AdapterError::invalid_input(
                "action_payload_mismatch",
                "action capability requires its matching public payload",
            )),
        }
    }

    fn invoke_write(
        &self,
        working: &mut V7HostPosition,
        request: &AdapterRequest<GameAdapterPayload>,
        meter: &mut CallMeter<'_>,
    ) -> Result<AdapterOutput<GameAdapterValue>, AdapterError> {
        let (APPLY_INTENT_CAPABILITY_ID, GameAdapterPayload::ApplyPublicIntent { intent }) =
            (&request.selection.capability_id[..], &request.payload)
        else {
            return Err(AdapterError::invalid_input(
                "action_payload_mismatch",
                "apply-public-intent capability requires an exact public intent",
            ));
        };
        meter.consume_work(1)?;
        let admitted = crate::v7_adapter_actions::bind_public_intent(working, intent.clone())
            .map_err(map_v7_action_error)?;
        let applied = crate::v7_adapter_actions::apply_admitted(working, &admitted)
            .map_err(map_v7_action_error)?;
        meter.consume_results(1)?;
        let result = GameAdapterValue::AppliedPublicIntent {
            actor: applied.actor,
            turn_changed: applied.turn_changed,
            result: applied.result,
        };
        *working = applied.position;
        Ok(AdapterOutput::value(result))
    }

    fn validate_output(
        &self,
        capability_id: &str,
        result: &GameAdapterValue,
    ) -> Result<u64, AdapterError> {
        match (capability_id, result) {
            (
                LEGAL_ACTIONS_PAGE_CAPABILITY_ID,
                GameAdapterValue::LegalActionsPage {
                    intents,
                    examined,
                    exhausted,
                    stop_reason,
                    cursor,
                },
            ) if intents.len() <= 4096
                && *examined <= 65536
                && intents.iter().all(Value::is_object)
                && *exhausted == cursor.is_none()
                && *exhausted == (stop_reason == "exhausted")
                && matches!(
                    stop_reason.as_str(),
                    "exhausted" | "page-limit" | "examined-budget"
                ) =>
            {
                Ok(intents.len() as u64)
            }
            (LEGAL_ACTIONS_CAPABILITY_ID, GameAdapterValue::LegalActions { intents })
                if intents.len() <= 4096 && intents.iter().all(Value::is_object) =>
            {
                Ok(intents.len() as u64)
            }
            (BIND_INTENT_CAPABILITY_ID, GameAdapterValue::BoundPublicIntent { intent })
                if intent.is_object() =>
            {
                Ok(1)
            }
            (APPLY_INTENT_CAPABILITY_ID, GameAdapterValue::AppliedPublicIntent { .. }) => Ok(1),
            _ => Err(AdapterError::execution_failed(
                "invalid_action_result",
                "public action adapter emitted a result incompatible with its capability",
            )),
        }
    }
}

fn verify_schema(source: &str, id: &str, expected_sha: &str) -> Result<(), AdapterError> {
    let schema: Value = serde_json::from_str(source).map_err(|error| {
        AdapterError::execution_failed("invalid_domain_schema", error.to_string())
    })?;
    if schema.get("$id").and_then(Value::as_str) != Some(id) {
        return Err(AdapterError::execution_failed(
            "domain_schema_id_mismatch",
            format!("domain schema ID differs from {id}"),
        ));
    }
    let bytes = serde_jcs::to_vec(&schema).map_err(|error| {
        AdapterError::execution_failed("invalid_domain_schema", error.to_string())
    })?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected_sha {
        return Err(AdapterError::execution_failed(
            "domain_schema_hash_mismatch",
            format!("domain schema {id} hash {actual} differs from pinned {expected_sha}"),
        ));
    }
    Ok(())
}

fn map_engine_error(error: EngineError) -> AdapterError {
    let (kind, code) = match &error {
        EngineError::UnsupportedFeature(_) => {
            (AdapterErrorKind::Unsupported, "game_rule_unsupported")
        }
        EngineError::StaleAction => (AdapterErrorKind::StaleRevision, "stale_game_position"),
        EngineError::InvalidState(_) => (AdapterErrorKind::InvalidInput, "invalid_game_state"),
        EngineError::InvalidConfig(_) => (AdapterErrorKind::InvalidInput, "invalid_game_config"),
        EngineError::Serialization(_) => {
            (AdapterErrorKind::InvalidInput, "invalid_game_serialization")
        }
        EngineError::ConditioningMismatch(_) => {
            (AdapterErrorKind::InvalidInput, "game_conditioning_mismatch")
        }
        EngineError::IllegalAction => (AdapterErrorKind::InvalidInput, "illegal_game_action"),
        EngineError::WrongActor => (AdapterErrorKind::InvalidInput, "wrong_game_actor"),
        EngineError::Terminal => (AdapterErrorKind::InvalidInput, "terminal_game"),
    };
    AdapterError::new(kind, code, error.to_string())
}

fn map_v7_action_error(error: V7ActionHostError) -> AdapterError {
    match error {
        V7ActionHostError::Engine(error) => map_engine_error(error),
        V7ActionHostError::Admission(error) => {
            let kind = match error.kind {
                AdmissionErrorKind::Unsupported => AdapterErrorKind::Unsupported,
                AdmissionErrorKind::StalePosition => AdapterErrorKind::StaleRevision,
                AdmissionErrorKind::InvalidEnvelope
                | AdmissionErrorKind::InvalidPayload
                | AdmissionErrorKind::WrongActor
                | AdmissionErrorKind::Terminal
                | AdmissionErrorKind::IllegalAction => AdapterErrorKind::InvalidInput,
            };
            AdapterError::new(kind, error.code, error.detail)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RngState;
    use adapter_runtime::{AdapterSelection, NeverCancelled};
    use serde_json::json;

    fn action_request(
        session: &GameAdapterSession,
        id: &str,
        payload: GameAdapterPayload,
    ) -> AdapterRequest<GameAdapterPayload> {
        let descriptor = session
            .descriptors()
            .into_iter()
            .find(|item| item.adapter_id == ACTION_ADAPTER_ID)
            .unwrap();
        let capability = descriptor
            .capabilities
            .iter()
            .find(|item| item.id == id)
            .unwrap();
        AdapterRequest {
            request_id: format!("page-test-{id}"),
            selection: AdapterSelection {
                project_id: descriptor.project_id.clone(),
                adapter_id: descriptor.adapter_id.clone(),
                contract_version: descriptor.contract_version,
                implementation_version: descriptor.implementation_version.clone(),
                capability_id: id.into(),
                request_schema: capability.request_schema.clone(),
                response_schema: capability.response_schema.clone(),
            },
            snapshot_revision: session.position().position_id().into(),
            call_limits: CallLimits {
                max_work: 100_000,
                max_results: 4096,
            },
            payload,
        }
    }

    #[test]
    fn action_pages_match_eager_order_and_retry_without_consuming_the_stream() {
        let mut session = GameAdapterSession::new_game(GameConfig::default(), 19).unwrap();
        let before = session.position().export_envelope().unwrap();
        let control = InvocationControl::unlimited_time(&NeverCancelled);
        let eager_request = action_request(
            &session,
            LEGAL_ACTIONS_CAPABILITY_ID,
            GameAdapterPayload::LegalActions,
        );
        let eager = session.invoke(&eager_request, &control).unwrap();
        let GameAdapterValue::LegalActions { intents: expected } = eager.result else {
            panic!("wrong eager result")
        };
        assert!(expected.len() > 1);
        let mut combined = Vec::new();
        let mut cursor = None;
        for _ in 0..20 {
            let request = action_request(
                &session,
                LEGAL_ACTIONS_PAGE_CAPABILITY_ID,
                GameAdapterPayload::LegalActionsPage {
                    limit: 1,
                    max_examined: 4,
                    cursor: cursor.clone(),
                },
            );
            let response = session.invoke(&request, &control).unwrap();
            let retry = session.invoke(&request, &control).unwrap();
            assert_eq!(response.result, retry.result);
            assert_eq!(response.page, retry.page);
            let GameAdapterValue::LegalActionsPage {
                intents,
                examined,
                exhausted,
                cursor: next,
                ..
            } = response.result
            else {
                panic!("wrong page result")
            };
            assert!(intents.len() <= 1 && examined <= 4);
            let info = response.page.unwrap();
            assert_eq!(
                (info.examined, info.exhausted, info.cursor),
                (examined as u64, exhausted, next.clone())
            );
            combined.extend(intents);
            cursor = next;
            if exhausted {
                assert!(cursor.is_none());
                break;
            }
        }
        assert_eq!(combined, expected);
        assert_eq!(session.position().export_envelope().unwrap(), before);
    }

    #[test]
    fn failed_page_budgets_and_cancellation_preserve_the_issued_cursor() {
        struct Cancelled;
        impl adapter_runtime::Cancellation for Cancelled {
            fn is_cancelled(&self) -> bool {
                true
            }
        }
        let mut session = GameAdapterSession::new_game(GameConfig::default(), 19).unwrap();
        let before = session.position().export_envelope().unwrap();
        let control = InvocationControl::unlimited_time(&NeverCancelled);
        let request = action_request(
            &session,
            LEGAL_ACTIONS_PAGE_CAPABILITY_ID,
            GameAdapterPayload::LegalActionsPage {
                limit: 1,
                max_examined: 4,
                cursor: None,
            },
        );
        let first = session.invoke(&request, &control).unwrap();
        let GameAdapterValue::LegalActionsPage {
            cursor: Some(cursor),
            ..
        } = first.result
        else {
            panic!("draft page must have continuation")
        };
        let next = action_request(
            &session,
            LEGAL_ACTIONS_PAGE_CAPABILITY_ID,
            GameAdapterPayload::LegalActionsPage {
                limit: 1,
                max_examined: 4,
                cursor: Some(cursor),
            },
        );
        let mut too_small = next.clone();
        too_small.call_limits.max_work = 4;
        assert_eq!(
            session.invoke(&too_small, &control).unwrap_err().kind,
            AdapterErrorKind::LimitExceeded
        );
        assert_eq!(
            session
                .invoke(&next, &InvocationControl::unlimited_time(&Cancelled))
                .unwrap_err()
                .kind,
            AdapterErrorKind::Cancelled
        );
        let actual = session.invoke(&next, &control).unwrap();
        assert_eq!(
            actual.result,
            session.invoke(&next, &control).unwrap().result
        );
        assert_eq!(session.position().export_envelope().unwrap(), before);
    }

    #[test]
    fn cursor_retention_is_bounded_and_old_snapshot_tokens_are_rejected() {
        let mut session = GameAdapterSession::new_game(GameConfig::default(), 19).unwrap();
        let control = InvocationControl::unlimited_time(&NeverCancelled);
        let mut earliest = None;
        let mut latest = None;
        let mut intent = None;
        for max_examined in 1..=65 {
            let request = action_request(
                &session,
                LEGAL_ACTIONS_PAGE_CAPABILITY_ID,
                GameAdapterPayload::LegalActionsPage {
                    limit: 1,
                    max_examined,
                    cursor: None,
                },
            );
            let response = session.invoke(&request, &control).unwrap();
            let GameAdapterValue::LegalActionsPage {
                intents,
                cursor: Some(token),
                ..
            } = response.result
            else {
                panic!("draft page must have continuation")
            };
            if max_examined == 1 {
                earliest = Some(token.clone());
                intent = intents.into_iter().next();
            }
            latest = Some(token);
        }
        let evicted = action_request(
            &session,
            LEGAL_ACTIONS_PAGE_CAPABILITY_ID,
            GameAdapterPayload::LegalActionsPage {
                limit: 1,
                max_examined: 4,
                cursor: earliest,
            },
        );
        assert_eq!(
            session.invoke(&evicted, &control).unwrap_err().code,
            "action_cursor_unavailable"
        );
        let apply = action_request(
            &session,
            APPLY_INTENT_CAPABILITY_ID,
            GameAdapterPayload::ApplyPublicIntent {
                intent: intent.unwrap(),
            },
        );
        session.invoke(&apply, &control).unwrap();
        let stale = action_request(
            &session,
            LEGAL_ACTIONS_PAGE_CAPABILITY_ID,
            GameAdapterPayload::LegalActionsPage {
                limit: 1,
                max_examined: 4,
                cursor: latest,
            },
        );
        assert_eq!(
            session.invoke(&stale, &control).unwrap_err().kind,
            AdapterErrorKind::StaleRevision
        );
    }

    #[test]
    fn source_verified_new_game_uses_the_host() {
        let session = GameAdapterSession::new_game(GameConfig::default(), 19).unwrap();
        let position = session.position();
        assert_eq!(position.state().ruleset_id, crate::RULES_VERSION_V7);
        assert_eq!(position.state().history.len(), 0);
        assert_eq!(
            position.export_envelope().unwrap()["positionId"],
            position.position_id()
        );
    }

    #[test]
    fn game_session_pins_descriptor_and_rejects_stale_or_mismatched_calls() {
        let board = vec![vec![Value::Null; 8]; 8];
        let position = V7HostPosition::from_parts(
            json!({"board":board,"turn":"white","mode":"play"}),
            RngState::seeded(19),
            Vec::new(),
        )
        .unwrap();
        let mut session = GameAdapterSession::new(position).unwrap();
        let before = session.position().export_envelope().unwrap();
        let descriptor = session
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.adapter_id == OBSERVATION_ADAPTER_ID)
            .unwrap()
            .clone();
        assert_eq!(descriptor.project_id, PROJECT_ID);
        assert_eq!(descriptor.capabilities.len(), 1);
        let capability = &descriptor.capabilities[0];
        assert_eq!(capability.access, CapabilityAccess::ReadOnly);

        let mut request = AdapterRequest {
            request_id: "observation-1".into(),
            selection: AdapterSelection {
                project_id: descriptor.project_id,
                adapter_id: descriptor.adapter_id,
                contract_version: descriptor.contract_version,
                implementation_version: descriptor.implementation_version,
                capability_id: capability.id.clone(),
                request_schema: capability.request_schema.clone(),
                response_schema: capability.response_schema.clone(),
            },
            snapshot_revision: "stale-position".into(),
            call_limits: CallLimits {
                max_work: 1000,
                max_results: 1,
            },
            payload: GameAdapterPayload::Observe {
                viewer: Color::White,
            },
        };
        let cancellation = NeverCancelled;
        let control = InvocationControl::unlimited_time(&cancellation);
        let error = session.invoke(&request, &control).unwrap_err();
        assert_eq!(error.kind, AdapterErrorKind::StaleRevision);
        request.snapshot_revision = session.position().position_id().into();
        request.selection.response_schema.sha256 = "0".repeat(64);
        let error = session.invoke(&request, &control).unwrap_err();
        assert_eq!(error.kind, AdapterErrorKind::Unsupported);
        assert_eq!(session.position().export_envelope().unwrap(), before);
    }

    #[test]
    fn draft_public_intents_bind_and_commit_without_exposing_host_action_ids() {
        let state = crate::draft::initialize_for_ruleset(
            GameConfig::default(),
            19,
            crate::RULES_VERSION_V7,
        )
        .unwrap();
        let mut session =
            GameAdapterSession::new(V7HostPosition::from_state(state).unwrap()).unwrap();
        let descriptor = session
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.adapter_id == ACTION_ADAPTER_ID)
            .unwrap()
            .clone();
        let selection = |capability_id: &str| {
            let capability = descriptor
                .capabilities
                .iter()
                .find(|capability| capability.id == capability_id)
                .unwrap();
            AdapterSelection {
                project_id: descriptor.project_id.clone(),
                adapter_id: descriptor.adapter_id.clone(),
                contract_version: descriptor.contract_version,
                implementation_version: descriptor.implementation_version.clone(),
                capability_id: capability.id.clone(),
                request_schema: capability.request_schema.clone(),
                response_schema: capability.response_schema.clone(),
            }
        };
        let request = |capability_id: &str, revision: String, payload| AdapterRequest {
            request_id: format!("test-{capability_id}"),
            selection: selection(capability_id),
            snapshot_revision: revision,
            call_limits: CallLimits {
                max_work: 100_000,
                max_results: 4096,
            },
            payload,
        };
        let control = InvocationControl::unlimited_time(&NeverCancelled);
        let first_revision = session.position().position_id().to_owned();
        let legal = session
            .invoke(
                &request(
                    LEGAL_ACTIONS_CAPABILITY_ID,
                    first_revision.clone(),
                    GameAdapterPayload::LegalActions,
                ),
                &control,
            )
            .unwrap();
        let GameAdapterValue::LegalActions { intents } = legal.result else {
            panic!("legal-actions emitted another result kind")
        };
        assert_eq!(intents.len(), 3);
        assert!(intents.iter().all(|intent| {
            intent.get("positionId").is_none() && intent.get("actionId").is_none()
        }));
        let intent = intents[0].clone();
        let bound = session
            .invoke(
                &request(
                    BIND_INTENT_CAPABILITY_ID,
                    first_revision.clone(),
                    GameAdapterPayload::BindPublicIntent {
                        intent: intent.clone(),
                    },
                ),
                &control,
            )
            .unwrap();
        assert_eq!(
            bound.result,
            GameAdapterValue::BoundPublicIntent {
                intent: intent.clone()
            }
        );
        assert_eq!(session.position().position_id(), first_revision);

        let mut extra = intent.clone();
        extra["clientNote"] = json!("not source intent");
        let invalid = session
            .invoke(
                &request(
                    APPLY_INTENT_CAPABILITY_ID,
                    first_revision.clone(),
                    GameAdapterPayload::ApplyPublicIntent { intent: extra },
                ),
                &control,
            )
            .unwrap_err();
        assert_eq!(invalid.kind, AdapterErrorKind::InvalidInput);
        assert_eq!(session.position().position_id(), first_revision);

        let applied = session
            .invoke(
                &request(
                    APPLY_INTENT_CAPABILITY_ID,
                    first_revision.clone(),
                    GameAdapterPayload::ApplyPublicIntent {
                        intent: intent.clone(),
                    },
                ),
                &control,
            )
            .unwrap();
        assert_ne!(session.position().position_id(), first_revision);
        assert_eq!(session.position().revision(), 1);
        assert_eq!(session.position().state().history.len(), 1);
        assert!(matches!(
            applied.result,
            GameAdapterValue::AppliedPublicIntent { .. }
        ));
        let committed = session.position().position_id().to_owned();
        let stale = session
            .invoke(
                &request(
                    APPLY_INTENT_CAPABILITY_ID,
                    first_revision,
                    GameAdapterPayload::ApplyPublicIntent { intent },
                ),
                &control,
            )
            .unwrap_err();
        assert_eq!(stale.kind, AdapterErrorKind::StaleRevision);
        assert_eq!(session.position().position_id(), committed);
    }
}
