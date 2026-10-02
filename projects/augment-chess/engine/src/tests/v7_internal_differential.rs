//! Explicit test-only probe for source-pinned v7 cases.
//!
//! This descendant of `#[cfg(test)] mod tests` may construct a typed Position
//! directly and call internal rule functions while separately recording the
//! observed public gate. Bounded internal parity is not full v7 coverage or an
//! executable production capability. Inputs and detailed results live outside Git.

use crate::v7_action_admission::{AdmissionErrorKind, V7_ACTION_PROTOCOL, admit_v7_action};
use crate::{Action, Color, EngineError, Position, V7HostPosition};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf, sync::Arc};

const SOURCE_SHA256: &str = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
// faithful175 재생성 자료. 기존 21개 입력·행동·순서와 source 출력의 이행 감사가
// 완료된 전체 corpus만 사용한다. 원문 자료 생성 성공은 native 성공과 별도다.
const SOURCE_CASES_SHA256: &str =
    "5de198ccdb213481126c4f9af832b50d45f150be740ee175b6caf3d7653f52ab";
const PROFILE: &str = "accelerate-headless-semantic-v7-faithful-init-v1";
const EXPECTED_CASES: usize = 21;
const MAX_SOURCE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_DIFFERENCES_PER_STAGE: usize = 32;

fn required_external_path(name: &str) -> PathBuf {
    let path = PathBuf::from(env::var(name).unwrap_or_else(|_| panic!("{name} is required")));
    assert!(
        path.is_absolute(),
        "{name} must be an absolute external path"
    );
    path
}

fn first_difference(expected: &Value, actual: &Value, path: &str, depth: usize) -> Option<String> {
    if depth > 64 {
        return Some(format!("{path}: comparison depth exceeded"));
    }
    match (expected, actual) {
        (Value::Object(left), Value::Object(right)) => {
            let mut keys = left.keys().chain(right.keys()).collect::<Vec<_>>();
            // A Position ID is derived from the rest of the envelope. Report
            // the first underlying state/RNG/history difference before its
            // inevitable digest mismatch.
            keys.sort_by(|a, b| {
                (a.as_str() == "positionId")
                    .cmp(&(b.as_str() == "positionId"))
                    .then_with(|| a.cmp(b))
            });
            keys.dedup();
            for key in keys {
                let next = format!("{path}.{key}");
                match (left.get(key), right.get(key)) {
                    (Some(a), Some(b)) => {
                        if let Some(found) = first_difference(a, b, &next, depth + 1) {
                            return Some(found);
                        }
                    }
                    (None, Some(_)) => return Some(format!("{next}: extra")),
                    (Some(_), None) => return Some(format!("{next}: missing")),
                    (None, None) => unreachable!(),
                }
            }
            None
        }
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                return Some(format!("{path}: length {} != {}", left.len(), right.len()));
            }
            for (index, (a, b)) in left.iter().zip(right).enumerate() {
                if let Some(found) = first_difference(a, b, &format!("{path}[{index}]"), depth + 1)
                {
                    return Some(found);
                }
            }
            None
        }
        (Value::Number(a), Value::Number(b)) if numbers_equal(a, b) => None,
        _ if expected == actual => None,
        _ => {
            let scalar = |value: &Value| match value {
                Value::Null | Value::Bool(_) | Value::Number(_) => value.to_string(),
                Value::String(text) => {
                    let length = text.chars().count();
                    let preview = text.chars().take(160).collect::<String>();
                    let suffix = if length > 160 { "…" } else { "" };
                    format!("text({length} chars, {preview:?}{suffix})")
                }
                Value::Array(items) => format!("array({} items)", items.len()),
                Value::Object(fields) => format!("object({} fields)", fields.len()),
            };
            Some(format!(
                "{path}: expected {} actual {}",
                scalar(expected),
                scalar(actual)
            ))
        }
    }
}

fn numbers_equal(expected: &serde_json::Number, actual: &serde_json::Number) -> bool {
    // Keep exact integer comparisons before normalizing 1 and 1.0. Converting
    // every integer to f64 can hide a one-unit error above the safe range.
    if let (Some(left), Some(right)) = (expected.as_i64(), actual.as_i64()) {
        return left == right;
    }
    if let (Some(left), Some(right)) = (expected.as_u64(), actual.as_u64()) {
        return left == right;
    }
    let safe_float_conversion = |number: &serde_json::Number| {
        const MAX_EXACT_INTEGER: i64 = 9_007_199_254_740_991;
        number.is_f64()
            || number
                .as_i64()
                .is_some_and(|value| (-MAX_EXACT_INTEGER..=MAX_EXACT_INTEGER).contains(&value))
            || number
                .as_u64()
                .is_some_and(|value| value <= MAX_EXACT_INTEGER as u64)
    };
    safe_float_conversion(expected)
        && safe_float_conversion(actual)
        && expected.as_f64() == actual.as_f64()
}

fn difference_details(expected: &Value, actual: &Value, path: &str, depth: usize) -> Vec<String> {
    fn visit(
        expected: &Value,
        actual: &Value,
        path: &str,
        depth: usize,
        differences: &mut Vec<String>,
    ) {
        if differences.len() >= MAX_DIFFERENCES_PER_STAGE {
            return;
        }
        if depth > 64 {
            differences.push(format!("{path}: comparison depth exceeded"));
            return;
        }
        match (expected, actual) {
            (Value::Object(left), Value::Object(right)) => {
                let mut keys = left.keys().chain(right.keys()).collect::<Vec<_>>();
                keys.sort_by(|a, b| {
                    (a.as_str() == "positionId")
                        .cmp(&(b.as_str() == "positionId"))
                        .then_with(|| a.cmp(b))
                });
                keys.dedup();
                for key in keys {
                    if differences.len() >= MAX_DIFFERENCES_PER_STAGE {
                        break;
                    }
                    let next = format!("{path}.{key}");
                    match (left.get(key), right.get(key)) {
                        (Some(a), Some(b)) => visit(a, b, &next, depth + 1, differences),
                        (None, Some(_)) => differences.push(format!("{next}: extra")),
                        (Some(_), None) => differences.push(format!("{next}: missing")),
                        (None, None) => unreachable!(),
                    }
                }
            }
            (Value::Array(left), Value::Array(right)) => {
                if left.len() != right.len() {
                    differences.push(format!("{path}: length {} != {}", left.len(), right.len()));
                }
                for (index, (a, b)) in left.iter().zip(right).enumerate() {
                    if differences.len() >= MAX_DIFFERENCES_PER_STAGE {
                        break;
                    }
                    visit(a, b, &format!("{path}[{index}]"), depth + 1, differences);
                }
            }
            _ => {
                if let Some(detail) = first_difference(expected, actual, path, depth) {
                    differences.push(detail);
                }
            }
        }
    }
    let mut differences = Vec::new();
    visit(expected, actual, path, depth, &mut differences);
    differences
}

fn record_mismatch(
    issues: &mut Vec<Value>,
    stage: &str,
    status: &str,
    expected: &Value,
    actual: &Value,
    path: &str,
) -> bool {
    let differences = difference_details(expected, actual, path, 0);
    if differences.is_empty() {
        return false;
    }
    issues.push(json!({
        "stage": stage,
        "status": status,
        "detail": differences[0],
        "diagnosticLimitReached": differences.len() == MAX_DIFFERENCES_PER_STAGE,
        "differences": differences,
    }));
    true
}

fn compare_result(host: &V7HostPosition, expected: &Value, stage: &str, issues: &mut Vec<Value>) {
    let state = host.state();
    let terminal = state.mode == "gameover";
    let winner = state.winner.as_deref();
    let actual = json!({
        "protocolVersion": "accelerate-result-v1",
        "status": if terminal { "terminal" } else { "ongoing" },
        "winner": if terminal && matches!(winner, Some("white" | "black")) {
            json!(winner)
        } else { Value::Null },
        "outcome": if terminal { json!(winner.unwrap_or("draw")) } else { Value::Null },
        "reason": if terminal {
            state.extra.get("replayEndReason").cloned().unwrap_or(json!(""))
        } else { json!("") },
    });
    record_mismatch(
        issues,
        stage,
        "result-envelope-mismatch",
        expected,
        &actual,
        "$",
    );
    match serde_json::to_value(state.result()) {
        Ok(outcome) => {
            record_mismatch(
                issues,
                stage,
                "result-outcome-mismatch",
                &expected["outcome"],
                &outcome,
                "$.outcome",
            );
        }
        Err(error) => issue(issues, stage, "serialization-error", error.to_string()),
    }
}

fn issue(issues: &mut Vec<Value>, stage: &str, status: &str, detail: impl Into<String>) {
    issues.push(json!({"stage": stage, "status": status, "detail": detail.into()}));
}

fn engine_error_status(error: &EngineError) -> &'static str {
    if matches!(error, EngineError::UnsupportedFeature(_)) {
        "unsupported"
    } else {
        "error"
    }
}

fn serialized_actions(actions: &[Action]) -> Result<Value, String> {
    let mut payloads = Vec::with_capacity(actions.len());
    for action in actions {
        let mut unbound = action.clone();
        unbound.position_key = None;
        payloads.push(serde_json::to_value(unbound).map_err(|error| error.to_string())?);
    }
    Ok(Value::Array(payloads))
}

fn serialized_action_envelopes(actions: &[Action], position_id: &str) -> Result<Value, String> {
    let mut envelopes = Vec::with_capacity(actions.len());
    let payloads = serialized_actions(actions)?;
    for payload in payloads
        .as_array()
        .expect("serialized actions are an array")
    {
        let action_id = format!(
            "{:x}",
            Sha256::digest(serde_jcs::to_vec(payload).map_err(|error| error.to_string())?)
        );
        envelopes.push(json!({
            "protocolVersion": V7_ACTION_PROTOCOL,
            "positionId": position_id,
            "actionId": action_id,
            "payload": payload,
        }));
    }
    Ok(Value::Array(envelopes))
}

fn compare_observations(
    host: &V7HostPosition,
    expected: &Value,
    stage: &str,
    issues: &mut Vec<Value>,
) {
    for (name, color) in [("white", Color::White), ("black", Color::Black)] {
        match host.state().try_observe(color) {
            Ok(observation) => match serde_json::to_value(observation) {
                Ok(actual) => {
                    record_mismatch(
                        issues,
                        &format!("{stage}-{name}"),
                        "mismatch",
                        &expected[name],
                        &actual,
                        "$",
                    );
                }
                Err(error) => issue(issues, stage, "serialization-error", error.to_string()),
            },
            Err(error) => issue(
                issues,
                stage,
                engine_error_status(&error),
                format!("{name}: {error}"),
            ),
        }
    }
}

fn compare_case(case: &Value) -> Value {
    let name = case["name"]
        .as_str()
        .expect("source case name must be text");
    let mode = case["position"]["state"]["mode"]
        .as_str()
        .expect("source case mode must be text");
    let mut issues = Vec::new();
    let mut evidence = Vec::new();
    let host = match V7HostPosition::from_envelope(case["position"].clone()) {
        Ok(host) => host,
        Err(error) => {
            issue(
                &mut issues,
                "v7-host-import",
                engine_error_status(&error),
                error.to_string(),
            );
            return json!({"name":name,"mode":mode,"issues":issues,"evidence":evidence});
        }
    };
    match host.export_envelope() {
        Ok(actual) => {
            if !record_mismatch(
                &mut issues,
                "v7-host-round-trip",
                "mismatch",
                &case["position"],
                &actual,
                "$",
            ) {
                evidence.push("source-position-rng-history-round-trip");
            }
        }
        Err(error) => issue(
            &mut issues,
            "v7-host-round-trip",
            "error",
            error.to_string(),
        ),
    }
    compare_observations(
        &host,
        &case["observations"],
        "baseline-observation",
        &mut issues,
    );
    compare_result(&host, &case["result"], "baseline-result", &mut issues);

    // The explicit internal probe stays useful while the public v7 gate is
    // closed and after it opens. Report which boundary was actually observed.
    match Position::from_state(host.state().clone()) {
        Err(EngineError::UnsupportedFeature(message))
            if message.contains("v7 rules profile is not executable") =>
        {
            evidence.push("public-v7-position-gate-closed");
        }
        Err(error) => issue(
            &mut issues,
            "public-v7-gate",
            "unexpected-error",
            error.to_string(),
        ),
        Ok(_) => evidence.push("public-v7-position-gate-open"),
    }
    let typed = Position(Arc::new(host.state().clone()), None);
    match typed.legal_actions() {
        Ok(actions) => match serialized_action_envelopes(&actions, host.position_id()) {
            Ok(actual) => {
                if !record_mismatch(
                    &mut issues,
                    "complete-legal",
                    "mismatch",
                    &case["actions"],
                    &actual,
                    "$",
                ) {
                    evidence.push("complete-ordered-legal-envelopes");
                }
            }
            Err(error) => issue(&mut issues, "complete-legal", "serialization-error", error),
        },
        Err(error) => issue(
            &mut issues,
            "complete-legal",
            engine_error_status(&error),
            error.to_string(),
        ),
    }
    // D-017의 legal/reject/apply 계약은 운영 host의 전체 source 표면을 검증한다.
    // 초기 orthodox first-play whitelist 대신 draft/play/terminal 모두에서
    // VerifiedV7ActionSet의 원문 순서·Position/action identity·payload를 비교한다.
    // 위 typed Position 검사도 유지하므로 두 실제 호출 경계를 따로 확인한다.
    match crate::v7_adapter_actions::legal_action_envelopes(&host) {
        Ok(actual) => {
            if !record_mismatch(
                &mut issues,
                "source-host-complete-legal",
                "mismatch",
                &case["actions"],
                &Value::Array(actual),
                "$",
            ) {
                evidence.push("source-host-complete-ordered-legal-envelopes");
            }
        }
        Err(error) => {
            let status = match &error {
                crate::v7_adapter_actions::V7ActionHostError::Engine(error) => {
                    engine_error_status(error)
                }
                crate::v7_adapter_actions::V7ActionHostError::Admission(error)
                    if error.kind == AdmissionErrorKind::Unsupported =>
                {
                    "unsupported"
                }
                crate::v7_adapter_actions::V7ActionHostError::Admission(_) => "admission-error",
            };
            issue(
                &mut issues,
                "source-host-complete-legal",
                status,
                error.to_string(),
            );
        }
    }

    if !case["rejectPayload"].is_null() {
        let payload = case["rejectPayload"].clone();
        let action_id = format!(
            "{:x}",
            Sha256::digest(
                serde_jcs::to_vec(&payload).expect("source rejection is canonical JSON")
            )
        );
        let envelope = json!({
            "protocolVersion": V7_ACTION_PROTOCOL,
            "positionId": host.position_id(),
            "actionId": action_id,
            "payload": payload,
        });
        match admit_v7_action(&host, envelope) {
            Err(error) if error.kind == AdmissionErrorKind::WrongActor => {
                evidence.push("wrong-actor-rejected-at-host-admission");
            }
            Err(error) => issue(
                &mut issues,
                "wrong-actor-rejection",
                "unexpected-rejection",
                format!("{}: {}", error.code, error.detail),
            ),
            Ok(_) => issue(
                &mut issues,
                "wrong-actor-rejection",
                "accepted",
                "source rejects this wrong-actor payload without changing the position",
            ),
        }
    }

    let mut sample_diagnostics = Vec::new();
    for (index, sample) in case["samples"]
        .as_array()
        .expect("source sample list must be an array")
        .iter()
        .enumerate()
    {
        let stage = format!("sample[{index}]");
        let mut diagnostic = json!({
            "stage": stage,
            "sourceAction": sample["action"],
            "expectedPositionId": sample["position"]["positionId"],
            "expectedResult": sample["result"],
        });
        let action: Action = match serde_json::from_value(sample["action"]["payload"].clone()) {
            Ok(action) => action,
            Err(error) => {
                issue(
                    &mut issues,
                    &stage,
                    "action-decode-error",
                    error.to_string(),
                );
                diagnostic["status"] = json!("action-decode-error");
                sample_diagnostics.push(diagnostic);
                continue;
            }
        };
        match host.transact(host.position_id(), |working| {
            crate::transition::apply(working, &action)
        }) {
            Ok((next, _captures)) => {
                diagnostic["status"] = json!("applied");
                diagnostic["actualPositionId"] = json!(next.position_id());
                match next.export_envelope() {
                    Ok(actual) => {
                        if !record_mismatch(
                            &mut issues,
                            &stage,
                            "full-position-mismatch",
                            &sample["position"],
                            &actual,
                            "$",
                        ) {
                            evidence.push("sample-full-state-rng-history");
                        }
                    }
                    Err(error) => issue(&mut issues, &stage, "export-error", error.to_string()),
                }
                compare_observations(
                    &next,
                    &sample["observations"],
                    &format!("{stage}-observation"),
                    &mut issues,
                );
                compare_result(
                    &next,
                    &sample["result"],
                    &format!("{stage}-result"),
                    &mut issues,
                );
            }
            Err(error) => {
                diagnostic["status"] = json!(engine_error_status(&error));
                diagnostic["error"] = json!(error.to_string());
                issue(
                    &mut issues,
                    &stage,
                    engine_error_status(&error),
                    error.to_string(),
                );
            }
        }
        sample_diagnostics.push(diagnostic);
        // A test-only transition must never mutate its source position.
        match host.export_envelope() {
            Ok(actual) => {
                record_mismatch(
                    &mut issues,
                    &stage,
                    "source-mutated",
                    &case["position"],
                    &actual,
                    "$",
                );
            }
            Err(error) => issue(
                &mut issues,
                &stage,
                "source-export-error",
                error.to_string(),
            ),
        }
    }
    json!({
        "name":name,
        "mode":mode,
        "sourcePositionId":host.position_id(),
        "issues":issues,
        "evidence":evidence,
        "samples":sample_diagnostics,
    })
}

#[test]
fn differential_numeric_normalization_preserves_exact_integer_errors() {
    assert!(first_difference(&json!(1), &json!(1.0), "$", 0).is_none());
    assert!(first_difference(&json!(1), &json!(1.5), "$", 0).is_some());
    let left = json!(9_007_199_254_740_992_u64);
    let right = json!(9_007_199_254_740_993_u64);
    assert!(first_difference(&left, &right, "$", 0).is_some());
    // A lossy floating representation cannot prove equality to an integer
    // outside the source's safe-integer range.
    assert!(first_difference(&right, &json!(9_007_199_254_740_992_f64), "$", 0).is_some());
}

#[test]
fn differential_diagnostics_report_state_before_derived_identity() {
    let expected = json!({"positionId":"source", "state":{"missing":1,"value":1}});
    let actual = json!({"positionId":"rust", "state":{"extra":2,"value":2}});
    let differences = difference_details(&expected, &actual, "$", 0);
    assert_eq!(differences.len(), 4);
    assert_eq!(differences[0], "$.state.extra: extra");
    assert_eq!(differences[1], "$.state.missing: missing");
    assert!(differences[2].starts_with("$.state.value: expected 1 actual 2"));
    assert!(differences[3].starts_with("$.positionId: expected"));
}

#[test]
fn differential_diagnostics_bound_output_without_accepting_a_prefix() {
    let expected = json!(vec![0; MAX_DIFFERENCES_PER_STAGE + 1]);
    let actual = json!(vec![1; MAX_DIFFERENCES_PER_STAGE + 1]);
    let mut issues = Vec::new();
    assert!(record_mismatch(
        &mut issues,
        "sample",
        "mismatch",
        &expected,
        &actual,
        "$"
    ));
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0]["diagnosticLimitReached"], true);
    assert_eq!(
        issues[0]["differences"].as_array().unwrap().len(),
        MAX_DIFFERENCES_PER_STAGE
    );
}

#[test]
#[ignore = "requires source-pinned external JSONL; explicit test-only v7 probe"]
fn source_pinned_v7_internal_differential_21_cases() {
    let cases_path = required_external_path("ACCELERATE_V7_INTERNAL_CASES");
    let source_report_path = required_external_path("ACCELERATE_V7_INTERNAL_SOURCE_REPORT");
    let result_path = required_external_path("ACCELERATE_V7_INTERNAL_REPORT");
    let source_report: Value =
        serde_json::from_slice(&fs::read(source_report_path).expect("read source export report"))
            .expect("parse source export report");
    assert_eq!(source_report["source"]["sha256"], SOURCE_SHA256);
    assert_eq!(source_report["source"]["profile"], PROFILE);
    assert_eq!(source_report["sourceExport"]["file"], "source-cases.jsonl");
    assert_eq!(source_report["sourceExport"]["cases"], EXPECTED_CASES);
    assert_eq!(
        source_report["sourceCases"].as_array().map(Vec::len),
        Some(EXPECTED_CASES)
    );
    assert!(
        fs::metadata(&cases_path)
            .expect("source case metadata")
            .len()
            <= MAX_SOURCE_BYTES,
        "source case JSONL exceeds the test-only input budget"
    );
    let bytes = fs::read(cases_path).expect("read source case JSONL");
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(source_report["sourceExport"]["sha256"], sha256);
    assert_eq!(sha256, SOURCE_CASES_SHA256);
    let text = std::str::from_utf8(&bytes).expect("source case JSONL is UTF-8");
    let cases = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("parse source case"))
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), EXPECTED_CASES);
    let results = cases.iter().map(compare_case).collect::<Vec<_>>();
    let failed = results
        .iter()
        .filter(|result| {
            result["issues"]
                .as_array()
                .is_some_and(|issues| !issues.is_empty())
        })
        .count();
    let has_gate_evidence = |result: &Value, gate: &str| {
        result["evidence"]
            .as_array()
            .is_some_and(|evidence| evidence.iter().any(|item| item.as_str() == Some(gate)))
    };
    let public_position_gate = if results
        .iter()
        .all(|result| has_gate_evidence(result, "public-v7-position-gate-open"))
    {
        "open"
    } else if results
        .iter()
        .all(|result| has_gate_evidence(result, "public-v7-position-gate-closed"))
    {
        "closed"
    } else {
        "mixed-or-error"
    };
    let report = json!({
        "schemaVersion":2,
        "gate":"v7-test-only-internal-differential",
        "sourceSha256":SOURCE_SHA256,
        "sourceCasesSha256":sha256,
        "cases":EXPECTED_CASES,
        "failedCases":failed,
        "status":if failed == 0 {"bounded-internal-pass"} else {"fail"},
        "decision":if failed == 0 {"internal-evidence-only"} else {"NO-GO"},
        "publicPositionGate":public_position_gate,
        "projectGo":false,
        "completeRuleCoverage":false,
        "diagnosticDifferencesPerStage":MAX_DIFFERENCES_PER_STAGE,
        "results":results,
    });
    fs::create_dir_all(result_path.parent().expect("external report parent"))
        .expect("create external report directory");
    fs::write(
        result_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&report).expect("serialize report")
        ),
    )
    .expect("write external internal differential report");
    for case in report["results"]
        .as_array()
        .expect("report has case results")
    {
        for observed in case["issues"].as_array().expect("case has issue list") {
            eprintln!(
                "{} {} {}: {}",
                case["name"].as_str().expect("case has name"),
                observed["stage"].as_str().expect("issue has stage"),
                observed["status"].as_str().expect("issue has status"),
                observed["detail"].as_str().expect("issue has detail"),
            );
        }
    }
    assert_eq!(
        failed, 0,
        "{failed} of {EXPECTED_CASES} internal v7 cases have explicit gaps"
    );
}
