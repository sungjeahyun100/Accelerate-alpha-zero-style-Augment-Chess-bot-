//! Bounded in-process timings through the same public adapter as consumers.
mod adapter_probe;
#[cfg(feature = "allocation-probe")]
mod allocation_probe;

use adapter_runtime::{
    AdapterError, AdapterErrorKind, AdapterRequest, AdapterSelection, InvocationControl,
    NeverCancelled,
};
use augment_chess_engine::adapter::{
    ACTION_ADAPTER_ID, APPLY_INTENT_CAPABILITY_ID, BIND_INTENT_CAPABILITY_ID, GameAdapterPayload,
    GameAdapterSession, GameAdapterValue, LEGAL_ACTIONS_CAPABILITY_ID, OBSERVATION_ADAPTER_ID,
    OBSERVATION_CAPABILITY_ID,
};
use augment_chess_engine::{Color, GameConfig, V7HostPosition};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::hint::black_box;
use std::io::{self, Read};
use std::time::Instant;

const MAX_INPUT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOTAL_OPERATIONS: u64 = 50_000;
const BUILD_PROVENANCE: &str = include_str!(concat!(env!("OUT_DIR"), "/build-provenance.json"));

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    value.get(key).ok_or_else(|| format!("missing {key}"))
}

fn bounded_integer(value: &Value, key: &str, minimum: u64, maximum: u64) -> Result<u64, String> {
    let value = required(value, key)?
        .as_u64()
        .ok_or_else(|| format!("{key} must be an integer"))?;
    if !(minimum..=maximum).contains(&value) {
        return Err(format!("{key} must be {minimum}..{maximum}"));
    }
    Ok(value)
}

fn percentile(sorted: &[f64], numerator: usize, denominator: usize) -> f64 {
    sorted[(sorted.len() * numerator)
        .div_ceil(denominator)
        .saturating_sub(1)]
}

fn timing_report(durations: Vec<f64>, samples: u64, iterations: u64) -> Value {
    let mean = durations.iter().sum::<f64>() / samples as f64;
    let mut ordered = durations.clone();
    ordered.sort_by(f64::total_cmp);
    json!({
        "samples": samples, "iterationsPerSample": iterations,
        "p50Ns": percentile(&ordered, 1, 2), "p95Ns": percentile(&ordered, 95, 100),
        "meanNs": mean, "minNs": ordered[0], "maxNs": ordered[ordered.len() - 1],
        "durationsNs": durations,
    })
}

fn measure<T>(
    samples: u64,
    warmups: u64,
    iterations: u64,
    mut operation: impl FnMut() -> Result<T, String>,
) -> Result<Value, String> {
    for _ in 0..warmups * iterations {
        black_box(operation()?);
    }
    let mut durations = Vec::with_capacity(samples as usize);
    for _ in 0..samples {
        let started = Instant::now();
        for _ in 0..iterations {
            black_box(operation()?);
        }
        durations.push(started.elapsed().as_nanos() as f64 / iterations as f64);
    }
    Ok(timing_report(durations, samples, iterations))
}

/// Mutable sessions are rebuilt before each apply; setup is outside its timer.
/// The call still includes selection/admission/transaction and response drop.
fn measure_prepared<S, T>(
    samples: u64,
    warmups: u64,
    iterations: u64,
    mut setup: impl FnMut() -> Result<S, String>,
    mut operation: impl FnMut(&mut S) -> Result<T, String>,
) -> Result<Value, String> {
    for _ in 0..warmups * iterations {
        black_box(operation(&mut setup()?)?);
    }
    let mut durations = Vec::with_capacity(samples as usize);
    for _ in 0..samples {
        let mut total_ns = 0;
        for _ in 0..iterations {
            let mut prepared = setup()?;
            let started = Instant::now();
            black_box(operation(&mut prepared)?);
            total_ns += started.elapsed().as_nanos();
        }
        durations.push(total_ns as f64 / iterations as f64);
    }
    Ok(timing_report(durations, samples, iterations))
}

enum CapabilityError {
    Unsupported(AdapterError),
    Failed(String),
}

impl CapabilityError {
    fn message(self) -> String {
        match self {
            Self::Unsupported(error) => error.to_string(),
            Self::Failed(reason) => reason,
        }
    }
    fn into_stage(self) -> Result<Value, String> {
        match self {
            Self::Unsupported(error) => {
                Ok(json!({"status": "unsupported", "reason": error.to_string(),
                "errorKind": "Unsupported", "errorCode": error.code,
                "adapterId": error.adapter_id, "capabilityId": error.capability_id}))
            }
            Self::Failed(reason) => Err(reason),
        }
    }
}

fn invoke(
    session: &mut GameAdapterSession,
    adapter_id: &str,
    capability_id: &str,
    payload: GameAdapterPayload,
) -> Result<GameAdapterValue, CapabilityError> {
    let descriptor = session
        .descriptors()
        .into_iter()
        .find(|entry| entry.adapter_id == adapter_id)
        .cloned()
        .ok_or_else(|| {
            CapabilityError::Failed(format!("adapter {adapter_id} is not registered"))
        })?;
    let capability = descriptor
        .capabilities
        .iter()
        .find(|entry| entry.id == capability_id)
        .cloned()
        .ok_or_else(|| {
            CapabilityError::Failed(format!(
                "capability {adapter_id}/{capability_id} is not registered"
            ))
        })?;
    let request = AdapterRequest {
        request_id: "performance-probe".into(),
        selection: AdapterSelection {
            project_id: descriptor.project_id,
            adapter_id: descriptor.adapter_id,
            contract_version: descriptor.contract_version,
            implementation_version: descriptor.implementation_version,
            capability_id: capability.id,
            request_schema: capability.request_schema,
            response_schema: capability.response_schema,
        },
        snapshot_revision: session.position().position_id().into(),
        call_limits: descriptor.call_limits,
        payload,
    };
    let cancellation = NeverCancelled;
    let control = InvocationControl::unlimited_time(&cancellation);
    session
        .invoke(&request, &control)
        .map(|response| response.result)
        .map_err(|error| {
            if error.kind == AdapterErrorKind::Unsupported {
                CapabilityError::Unsupported(error)
            } else {
                CapabilityError::Failed(format!("{adapter_id}/{capability_id}: {error}"))
            }
        })
}

fn observe(session: &mut GameAdapterSession, viewer: Color) -> Result<Value, CapabilityError> {
    match invoke(
        session,
        OBSERVATION_ADAPTER_ID,
        OBSERVATION_CAPABILITY_ID,
        GameAdapterPayload::Observe { viewer },
    )? {
        GameAdapterValue::Observation { observation } => {
            serde_json::to_value(observation).map_err(|error| {
                CapabilityError::Failed(format!("observation serialization: {error}"))
            })
        }
        _ => Err(CapabilityError::Failed(
            "observation returned a different result variant".into(),
        )),
    }
}

fn legal(session: &mut GameAdapterSession) -> Result<Vec<Value>, CapabilityError> {
    match invoke(
        session,
        ACTION_ADAPTER_ID,
        LEGAL_ACTIONS_CAPABILITY_ID,
        GameAdapterPayload::LegalActions,
    )? {
        GameAdapterValue::LegalActions { intents } => Ok(intents),
        _ => Err(CapabilityError::Failed(
            "legal-actions returned a different result variant".into(),
        )),
    }
}

fn apply(
    session: &mut GameAdapterSession,
    intent: &Value,
) -> Result<GameAdapterValue, CapabilityError> {
    let result = invoke(
        session,
        ACTION_ADAPTER_ID,
        APPLY_INTENT_CAPABILITY_ID,
        GameAdapterPayload::ApplyPublicIntent {
            intent: intent.clone(),
        },
    )?;
    if !matches!(&result, GameAdapterValue::AppliedPublicIntent { .. }) {
        return Err(CapabilityError::Failed(
            "apply-public-intent returned a different result variant".into(),
        ));
    }
    Ok(result)
}

fn observation_report(
    session: &mut GameAdapterSession,
    position: &V7HostPosition,
    samples: u64,
    warmups: u64,
    iterations: u64,
) -> Result<Value, String> {
    let white = match observe(session, Color::White) {
        Ok(value) => value,
        Err(error) => return error.into_stage(),
    };
    let black = match observe(session, Color::Black) {
        Ok(value) => value,
        Err(error) => return error.into_stage(),
    };
    let fresh = measure(samples, 0, 1, || {
        let mut fresh = GameAdapterSession::new(position.clone())
            .map_err(|error| format!("fresh observation session: {error}"))?;
        observe(&mut fresh, Color::White).map_err(CapabilityError::message)
    })?;
    let warm = measure(samples, warmups, iterations, || {
        observe(session, Color::White).map_err(CapabilityError::message)
    })?;
    Ok(
        json!({"status": "supported", "preflight": {"observationWhite": white, "observationBlack": black},
        "timings": {"observeWhiteFreshSession": fresh, "observeWhiteWarmSession": warm}}),
    )
}

fn transition_report(
    case: &Value,
    session: &mut GameAdapterSession,
    position: &V7HostPosition,
    samples: u64,
    warmups: u64,
    iterations: u64,
) -> Result<Value, String> {
    let legal_payloads = match legal(session) {
        Ok(value) => value,
        Err(error) => return error.into_stage(),
    };
    let intent = required(case, "actionPayload")?;
    let bound = match invoke(
        session,
        ACTION_ADAPTER_ID,
        BIND_INTENT_CAPABILITY_ID,
        GameAdapterPayload::BindPublicIntent {
            intent: intent.clone(),
        },
    ) {
        Ok(value) => value,
        Err(error) => return error.into_stage(),
    };
    if !matches!(bound, GameAdapterValue::BoundPublicIntent { intent: ref bound } if bound == intent)
    {
        return Err("bind-public-intent changed the selected source payload".into());
    }
    let mut preflight_session = GameAdapterSession::new(position.clone())
        .map_err(|error| format!("apply preflight setup: {error}"))?;
    if let Err(error) = apply(&mut preflight_session, intent) {
        return error.into_stage();
    }
    let next = preflight_session
        .position()
        .export_envelope()
        .map_err(|error| format!("apply preflight export: {error}"))?;
    let preflight = json!({"legalPayloads": legal_payloads, "nextPosition": next,
        "nextState": required(&next, "state")?, "nextRng": required(&next, "rng")?, "nextHistory": required(&next, "history")?});
    let legal_timing = measure(samples, warmups, iterations, || {
        legal(session).map_err(CapabilityError::message)
    })?;
    let apply_timing = measure_prepared(
        samples,
        warmups,
        iterations,
        || {
            GameAdapterSession::new(position.clone())
                .map_err(|error| format!("apply benchmark setup: {error}"))
        },
        |fresh| apply(fresh, intent).map_err(CapabilityError::message),
    )?;
    Ok(json!({"status": "supported", "preflight": preflight,
        "timings": {"legalActions": legal_timing, "applyWithHistory": apply_timing},
        "applySetup": "fresh session around the same immutable source position before every operation; setup and session teardown excluded"}))
}

fn boundary_report(
    raw: &Value,
    position: &V7HostPosition,
    samples: u64,
    warmups: u64,
    iterations: u64,
) -> Result<Value, String> {
    let encoded =
        serde_json::to_vec(raw).map_err(|error| format!("boundary JSON preflight: {error}"))?;
    let canonical =
        serde_jcs::to_vec(raw).map_err(|error| format!("boundary JCS preflight: {error}"))?;
    let canonical_sha = format!("{:x}", Sha256::digest(&canonical));
    let timings = json!({
        "hostSnapshotHandleClone": measure(samples, warmups, iterations, || Ok(position.clone()))?,
        "transactionStateClone": measure(samples, warmups, iterations, || Ok(position.state().clone()))?,
        "sourceEnvelopeClone": measure(samples, warmups, iterations, || Ok(raw.clone()))?,
        "hostExportEnvelope": measure(samples, warmups, iterations, || position.export_envelope().map_err(|error| error.to_string()))?,
        "hostImportEnvelope": measure(samples, warmups, iterations, || V7HostPosition::from_envelope(raw.clone()).map_err(|error| error.to_string()))?,
        "newSessionFromSnapshot": measure(samples, warmups, iterations, || GameAdapterSession::new(position.clone()).map_err(|error| error.to_string()))?,
        "canonicalEnvelope": measure(samples, warmups, iterations, || serde_jcs::to_vec(raw).map_err(|error| error.to_string()))?,
        "sha256PreparedCanonicalBytes": measure(samples, warmups, iterations, || Ok(Sha256::digest(&canonical)))?,
        "jsonEncodeEnvelope": measure(samples, warmups, iterations, || serde_json::to_vec(raw).map_err(|error| error.to_string()))?,
        "jsonDecodeEnvelope": measure(samples, warmups, iterations, || serde_json::from_slice::<Value>(&encoded).map_err(|error| error.to_string()))?,
    });
    #[cfg(feature = "allocation-probe")]
    let allocations = json!({
        "hostSnapshotHandleClone": allocation_probe::measure(samples, || Ok(position.clone()))?,
        "transactionStateClone": allocation_probe::measure(samples, || Ok(position.state().clone()))?,
        "sourceEnvelopeClone": allocation_probe::measure(samples, || Ok(raw.clone()))?,
        "hostExportEnvelope": allocation_probe::measure(samples, || position.export_envelope().map_err(|error| error.to_string()))?,
        "hostImportEnvelope": allocation_probe::measure(samples, || V7HostPosition::from_envelope(raw.clone()).map_err(|error| error.to_string()))?,
        "newSessionFromSnapshot": allocation_probe::measure(samples, || GameAdapterSession::new(position.clone()).map_err(|error| error.to_string()))?,
        "canonicalEnvelope": allocation_probe::measure(samples, || serde_jcs::to_vec(raw).map_err(|error| error.to_string()))?,
        "jsonEncodeEnvelope": allocation_probe::measure(samples, || serde_json::to_vec(raw).map_err(|error| error.to_string()))?,
        "jsonDecodeEnvelope": allocation_probe::measure(samples, || serde_json::from_slice::<Value>(&encoded).map_err(|error| error.to_string()))?,
    });
    #[cfg(not(feature = "allocation-probe"))]
    let allocations = Value::Null;
    Ok(
        json!({"scope": "copy, admission, registry setup, JCS, digest and JSON costs on a prepared source position; no wire transport",
        "preflight": {"canonicalEnvelopeSha256": canonical_sha, "canonicalBytes": canonical.len(), "jsonBytes": encoded.len()},
        "timings": timings, "allocationCounts": allocations,
        "interpretation": "Snapshot-handle clone shares Arc state; transaction-state clone copies GameState. These phases overlap real calls and must not be summed into a predicted call time. Allocation instrumentation is opt-in and changes timing."}),
    )
}

fn case_report(case: &Value, samples: u64, warmups: u64, iterations: u64) -> Result<Value, String> {
    let style = required(case, "style")?
        .as_str()
        .ok_or("style must be a string")?;
    if !matches!(style, "normal" | "chaos" | "grand") {
        return Err(format!("unsupported style {style}"));
    }
    let config: GameConfig = serde_json::from_value(required(case, "config")?.clone())
        .map_err(|error| format!("{style} config: {error}"))?;
    if config.game_style != style || !config.draft_delete || !config.rule_card_ids.is_empty() {
        return Err(format!("{style}: unsupported benchmark configuration"));
    }
    let seed = required(case, "seed")?
        .as_u64()
        .ok_or("seed must be an integer")?;
    if seed != 37 {
        return Err("benchmark seed must be 37".into());
    }
    let created = GameAdapterSession::new_game(config.clone(), seed)
        .map_err(|error| format!("{style} new game: {error}"))?;
    let new_game_position = created
        .position()
        .export_envelope()
        .map_err(|error| format!("{style} new-game export: {error}"))?;
    let raw = required(case, "position")?;
    let position = V7HostPosition::from_envelope(raw.clone())
        .map_err(|error| format!("{style} source-host import: {error}"))?;
    let mut session = GameAdapterSession::new(position.clone())
        .map_err(|error| format!("{style} adapter session: {error}"))?;
    let new_game_fresh = measure(samples, 0, 1, || {
        GameAdapterSession::new_game(config.clone(), seed)
            .map_err(|error| format!("{style} fresh new-game benchmark: {error}"))
    })?;
    let observation = observation_report(&mut session, &position, samples, warmups, iterations)
        .map_err(|error| format!("{style} observation: {error}"))?;
    let transition = transition_report(case, &mut session, &position, samples, warmups, iterations)
        .map_err(|error| format!("{style} transition: {error}"))?;
    if session.position().position_id() != position.position_id() {
        return Err(format!("{style}: read-only probes mutated their snapshot"));
    }
    let boundaries = boundary_report(raw, &position, samples, warmups, iterations)
        .map_err(|error| format!("{style} boundary diagnostic: {error}"))?;
    Ok(
        json!({"style": style, "newGame": {"status": "supported", "preflight": new_game_position,
        "timings": {"newGameFreshSession": new_game_fresh}}, "observation": observation, "transition": transition, "boundaryDiagnostics": boundaries}),
    )
}

fn build_provenance() -> Result<Value, String> {
    serde_json::from_str(BUILD_PROVENANCE)
        .map_err(|error| format!("embedded build provenance: {error}"))
}

fn run() -> Result<Value, String> {
    let mut input = Vec::new();
    io::stdin()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut input)
        .map_err(|error| format!("reading input: {error}"))?;
    if input.len() as u64 > MAX_INPUT_BYTES {
        return Err(format!("input exceeds {MAX_INPUT_BYTES} bytes"));
    }
    let request: Value = serde_json::from_slice(&input)
        .map_err(|error| format!("invalid benchmark request: {error}"))?;
    let samples = bounded_integer(&request, "samples", 1, 100)?;
    let warmups = bounded_integer(&request, "warmups", 0, 20)?;
    let iterations = bounded_integer(&request, "iterations", 1, 100)?;
    let cases = required(&request, "cases")?
        .as_array()
        .ok_or("cases must be an array")?;
    if cases.is_empty() || cases.len() > 3 {
        return Err("cases must contain 1..3 game styles".into());
    }
    let mut styles = Vec::new();
    for case in cases {
        let style = required(case, "style")?
            .as_str()
            .ok_or("style must be a string")?;
        if styles.contains(&style) {
            return Err(format!("duplicate benchmark style {style}"));
        }
        styles.push(style);
    }
    // Reserve cold calls, main stages, all diagnostics and allocation samples.
    // This bounds outer operations, not inner engine/VM work.
    let per_case = 2 * samples + 14 * (samples + warmups) * iterations + 40 + 11 * samples;
    let total = cases.len() as u64 * per_case + 200 * (samples + warmups);
    if total > MAX_TOTAL_OPERATIONS {
        return Err(format!(
            "benchmark exceeds {MAX_TOTAL_OPERATIONS} bounded operations"
        ));
    }
    let dispatch = adapter_probe::measure_dispatch(samples, warmups)?;
    let results = cases
        .iter()
        .map(|case| case_report(case, samples, warmups, iterations))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(
        json!({"schemaVersion": 2, "scope": "rust-public-game-adapter-in-process", "binaryVersion": env!("CARGO_PKG_VERSION"),
        "buildProvenance": build_provenance()?, "syntheticAdapterDispatch": dispatch, "cases": results}),
    )
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = if arguments.as_slice() == ["--synthetic-only"] {
        adapter_probe::measure_dispatch(7, 2).and_then(|report| {
            Ok(json!({"schemaVersion": 2,
            "buildProvenance": build_provenance()?, "syntheticAdapterDispatch": report}))
        })
    } else if arguments.as_slice() == ["--build-info"] {
        build_provenance()
            .map(|provenance| json!({"schemaVersion": 2, "buildProvenance": provenance}))
    } else if arguments.is_empty() {
        run()
    } else {
        Err("only --synthetic-only, --build-info or a JSON request on stdin is supported".into())
    };
    match result {
        Ok(report) => println!("{report}"),
        Err(error) => {
            eprintln!("augment-chess-perf: {error}");
            std::process::exit(1);
        }
    }
}
