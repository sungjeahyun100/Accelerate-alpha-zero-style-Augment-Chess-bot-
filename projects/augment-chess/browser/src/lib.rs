//! Browser transport only: rules, public projection and transactions remain in
//! the engine-owned adapter. No private state serialization enters this API.
#![forbid(unsafe_code)]

use std::time::Duration;

use adapter_runtime::{
    AdapterError, AdapterErrorKind, AdapterOutcome, AdapterRequest, AdapterResponse, Instant,
    InvocationControl, NeverCancelled,
};
use augment_chess_engine::{
    GameConfig, GameResult, RULES_VERSION_V7,
    adapter::{GameAdapterPayload, GameAdapterSession, GameAdapterValue, IMPLEMENTATION_VERSION},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen::prelude::*;

pub const MAX_REQUEST_BYTES: usize = 1_048_576;
pub const MAX_RESPONSE_BYTES: usize = 8_388_608;
pub const MAX_TIMEOUT_MS: u32 = 30_000;

#[cfg(any(test, feature = "browser-test-fixtures"))]
pub mod fixtures;

/// Fixed browser parity scenarios are available only in a separate test build.
/// This export never accepts a state/envelope and is absent from default builds.
#[cfg(feature = "browser-test-fixtures")]
#[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen)]
pub fn browser_test_case(case_id: &str) -> Result<BrowserGameSession, String> {
    let inner = fixtures::session(case_id).map_err(error_json)?;
    let metadata_json = metadata_json(&inner).map_err(error_json)?;
    Ok(BrowserGameSession {
        inner,
        metadata_json,
    })
}

/// Owned by the game Worker. Its only state-bearing methods are public adapter
/// invocations; no envelope import/export, raw state or RNG accessor exists.
#[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen)]
pub struct BrowserGameSession {
    inner: GameAdapterSession,
    metadata_json: String,
}

#[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen)]
impl BrowserGameSession {
    /// Errors are thrown as an AdapterError JSON string, preserving its code
    /// and original engine message. The Worker parses and annotates the stage.
    pub fn new_game(config_json: &str, seed: u32) -> Result<Self, String> {
        Self::create(config_json, seed).map_err(error_json)
    }

    pub fn metadata(&self) -> String {
        self.metadata_json.clone()
    }

    pub fn revision(&self) -> String {
        self.inner.position().position_id().to_owned()
    }

    pub fn decision_actor(&self) -> String {
        self.inner
            .position()
            .state()
            .decision_actor()
            .as_str()
            .to_owned()
    }

    pub fn result(&self) -> Option<String> {
        self.inner.position().state().result().map(|result| {
            match result {
                GameResult::White => "white",
                GameResult::Black => "black",
                GameResult::Draw => "draw",
            }
            .to_owned()
        })
    }

    /// Complete exact AdapterRequest -> AdapterOutcome JSON transport. The
    /// caller must bind descriptor, schema hashes and snapshot revision.
    /// A Worker message cannot interrupt a synchronous WASM call; deadlines
    /// are checked cooperatively. Hard cancellation must discard that Worker.
    pub fn invoke_json(&mut self, request_json: &str, timeout_ms: f64) -> Result<String, String> {
        let response = self.invoke_bounded(request_json, timeout_ms);
        match response {
            Ok(json) => Ok(json),
            Err(error) => outcome_json(Err(error)).map_err(error_json),
        }
    }
}

impl BrowserGameSession {
    fn create(config_json: &str, seed: u32) -> Result<Self, AdapterError> {
        check_request_size(config_json)?;
        let config: GameConfig = serde_json::from_str(config_json).map_err(|error| {
            AdapterError::invalid_input(
                "invalid_game_config",
                format!("invalid v7 game config: {error}"),
            )
        })?;
        let inner = GameAdapterSession::new_game(config, u64::from(seed))?;
        let metadata_json = metadata_json(&inner)?;
        Ok(Self {
            inner,
            metadata_json,
        })
    }

    fn invoke_bounded(
        &mut self,
        request_json: &str,
        timeout_ms: f64,
    ) -> Result<String, AdapterError> {
        check_request_size(request_json)?;
        if !timeout_ms.is_finite()
            || timeout_ms.fract() != 0.0
            || !(1.0..=f64::from(MAX_TIMEOUT_MS)).contains(&timeout_ms)
        {
            return Err(AdapterError::invalid_input(
                "invalid_browser_timeout",
                format!("timeout_ms must be a finite integer within 1..={MAX_TIMEOUT_MS}"),
            ));
        }
        let request: AdapterRequest<GameAdapterPayload> = serde_json::from_str(request_json)
            .map_err(|error| {
                AdapterError::invalid_input(
                    "invalid_adapter_request",
                    format!("invalid adapter request: {error}"),
                )
            })?;
        let cancellation = NeverCancelled;
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(timeout_ms as u64))
            .ok_or_else(|| {
                AdapterError::invalid_input("deadline_overflow", "browser deadline overflow")
            })?;
        let control = InvocationControl {
            cancellation: &cancellation,
            deadline: Some(deadline),
        };

        // The native registry commits only after all semantic/meter checks.
        // For a browser write, final transport serialization and byte limits
        // are also checked before publishing the new private session. Rebuild
        // only writes: read calls keep the registry's opaque page cursor cache.
        if matches!(
            &request.payload,
            GameAdapterPayload::ApplyPublicIntent { .. }
        ) {
            let mut working = GameAdapterSession::new(self.inner.position().clone())?;
            let response = working.invoke(&request, &control);
            let committed = response.is_ok();
            let json = outcome_json(response)?;
            if committed && Instant::now() >= deadline {
                return Err(AdapterError::new(
                    AdapterErrorKind::LimitExceeded,
                    "deadline_exceeded",
                    "adapter invocation deadline was exceeded",
                ));
            }
            if committed {
                self.inner = working;
            }
            Ok(json)
        } else {
            outcome_json(self.inner.invoke(&request, &control))
        }
    }
}

#[cfg(test)]
mod tests;

fn check_request_size(value: &str) -> Result<(), AdapterError> {
    if value.len() > MAX_REQUEST_BYTES {
        Err(AdapterError::new(
            AdapterErrorKind::LimitExceeded,
            "browser_request_size_exceeded",
            format!(
                "request bytes {} exceeds limit {MAX_REQUEST_BYTES}",
                value.len()
            ),
        ))
    } else {
        Ok(())
    }
}

fn outcome_json(
    response: Result<AdapterResponse<GameAdapterValue>, AdapterError>,
) -> Result<String, AdapterError> {
    let json = serde_json::to_string(&AdapterOutcome::from(response)).map_err(|error| {
        AdapterError::execution_failed("browser_response_serialization_failed", error.to_string())
    })?;
    if json.len() > MAX_RESPONSE_BYTES {
        return Err(AdapterError::new(
            AdapterErrorKind::LimitExceeded,
            "browser_response_size_exceeded",
            format!(
                "response bytes {} exceeds limit {MAX_RESPONSE_BYTES}",
                json.len()
            ),
        ));
    }
    Ok(json)
}

fn error_json(error: AdapterError) -> String {
    // AdapterError contains only strings and enum values; JSON serialization
    // cannot fail for its owned fields. Preserve the original message if the
    // serializer ever changes to a fallible representation.
    serde_json::to_string(&error).unwrap_or_else(|serialization_error| {
        format!("{error}; AdapterError serialization failed: {serialization_error}")
    })
}

fn metadata_json(session: &GameAdapterSession) -> Result<String, AdapterError> {
    let parse = |source: &str| -> Result<Value, AdapterError> {
        serde_json::from_str(source).map_err(|error| {
            AdapterError::execution_failed("browser_metadata_invalid", error.to_string())
        })
    };
    let catalog = parse(include_str!("../../contracts/catalog/site-20260928.json"))?;
    let policy = parse(include_str!(
        "../../contracts/catalog/observation-20260928.json"
    ))?;
    let policy_bytes = serde_jcs::to_vec(&policy).map_err(|error| {
        AdapterError::execution_failed("browser_metadata_invalid", error.to_string())
    })?;
    let metadata = json!({
        "rulesVersion": RULES_VERSION_V7,
        "catalogVersion": catalog["catalogVersion"],
        "protocolVersion": policy["protocolVersion"],
        "projectionVersion": policy["projectionVersion"],
        "observationPolicyHash": format!("{:x}", Sha256::digest(policy_bytes)),
        "implementationVersion": IMPLEMENTATION_VERSION,
        "executionProfileVersion": catalog["executionProfile"]["version"],
        "executionProfileSha256": catalog["executionProfile"]["sha256"],
        "descriptors": session.descriptors(),
    });
    serde_json::to_string(&metadata).map_err(|error| {
        AdapterError::execution_failed("browser_metadata_serialization_failed", error.to_string())
    })
}
