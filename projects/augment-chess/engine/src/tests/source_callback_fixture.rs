//! 동결 원문 callback 영수증의 테스트 전용 입력·비교 경계.
//!
//! 합성 중간 상태는 공개 host 입력이 아니다. topology 정리 전의 상태를
//! lossless GameState로 읽되 source SHA, envelope identity, 버전, RNG와 history는
//! 그대로 검사한다. production V7HostPosition의 admission을 변경하지 않는다.

use crate::state::{RngState, validate_json_value};
use crate::{EngineError, GameState, RULES_VERSION_V7, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

const SOURCE_MAIN_SHA256: &str = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";

pub(crate) fn validate_source_receipt(receipt: &Value) -> Result<&[Value]> {
    if receipt["schemaVersion"] != 1 || receipt["sourceMainSha256"] != SOURCE_MAIN_SHA256 {
        return Err(invalid("receipt schema or exact frozen source SHA differs"));
    }
    let cases = receipt["cases"]
        .as_array()
        .filter(|cases| !cases.is_empty())
        .ok_or_else(|| invalid("receipt cases must be a nonempty array"))?;
    Ok(cases)
}

/// 공개 spatial admission을 호출하지 않는 내부 callback fixture 전용 importer.
/// 8x8 DTO, 원문 envelope identity와 round-trip은 여전히 요구한다.
pub(crate) fn source_callback_state(envelope: &Value) -> Result<GameState> {
    validate_json_value(envelope, 0)?;
    let fields = envelope
        .as_object()
        .ok_or_else(|| invalid("envelope must be an object"))?;
    const KEYS: [&str; 7] = [
        "protocolVersion",
        "rulesVersion",
        "catalogVersion",
        "state",
        "rng",
        "history",
        "positionId",
    ];
    if fields.len() != KEYS.len() || KEYS.iter().any(|key| !fields.contains_key(*key)) {
        return Err(invalid("envelope has missing or unexpected fields"));
    }
    let catalog: Value = serde_json::from_str(include_str!(
        "../../../contracts/catalog/site-20260928.json"
    ))
    .map_err(EngineError::serialization)?;
    if envelope["protocolVersion"] != crate::v7_host::V7_POSITION_PROTOCOL
        || envelope["rulesVersion"] != RULES_VERSION_V7
        || envelope["catalogVersion"] != catalog["catalogVersion"]
    {
        return Err(invalid(
            "envelope protocol, rules or catalog version differs",
        ));
    }
    let mut content = fields.clone();
    content.shift_remove("positionId");
    let actual_id = digest(&Value::Object(content))?;
    if envelope["positionId"].as_str() != Some(actual_id.as_str()) {
        return Err(invalid(&format!(
            "complete envelope positionId differs; computed {actual_id}"
        )));
    }

    let source = envelope["state"]
        .as_object()
        .ok_or_else(|| invalid("state must be an object"))?;
    if ["board", "turn", "mode"]
        .iter()
        .any(|key| !source.contains_key(*key))
    {
        return Err(invalid("state requires board, turn and mode"));
    }
    if source.contains_key("rng") || source.contains_key("history") {
        return Err(invalid("state must not duplicate outer RNG or history"));
    }
    if source
        .get("rulesetId")
        .is_some_and(|value| value.as_str() != Some(RULES_VERSION_V7))
    {
        return Err(invalid(
            "state rulesetId conflicts with envelope rulesVersion",
        ));
    }
    let rng_fields = envelope["rng"]
        .as_object()
        .ok_or_else(|| invalid("RNG must be an object"))?;
    if rng_fields.len() != 4
        || ["algorithm", "state", "tape", "cursor"]
            .iter()
            .any(|key| !rng_fields.contains_key(*key))
    {
        return Err(invalid("RNG has missing or unexpected fields"));
    }
    let rng: RngState =
        serde_json::from_value(envelope["rng"].clone()).map_err(EngineError::serialization)?;
    if rng.algorithm != "lcg32-v1" || rng.tape.iter().any(|sample| !(0.0..1.0).contains(sample)) {
        return Err(invalid("RNG requires lcg32-v1 and tape samples in [0,1)"));
    }
    let history: Vec<Value> =
        serde_json::from_value(envelope["history"].clone()).map_err(EngineError::serialization)?;
    let mut state: GameState =
        serde_json::from_value(envelope["state"].clone()).map_err(EngineError::serialization)?;
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(invalid("internal callback fixture board must be 8x8"));
    }
    state.ruleset_id = RULES_VERSION_V7.into();
    state.rng = rng;
    state.history = history;

    let round_trip = source_state_value(&state, source.contains_key("rulesetId"))?;
    require_equal(&envelope["state"], &round_trip, "input.state")?;
    require_equal(
        &envelope["rng"],
        &serde_json::to_value(&state.rng).map_err(EngineError::serialization)?,
        "input.rng",
    )?;
    require_equal(
        &envelope["history"],
        &serde_json::to_value(&state.history).map_err(EngineError::serialization)?,
        "input.history",
    )?;
    Ok(state)
}

/// 오류나 panic이 한 사례 뒤의 다른 독립 callback 진단을 가리지 않게 한다.
/// 잡은 오류는 최종 검사를 실패시키는 정확한 사례별 진단으로 남긴다.
pub(crate) fn collect_case_diagnostics(
    label: &str,
    mismatches: &mut Vec<String>,
    compare: impl FnOnce(&mut Vec<String>) -> Result<()>,
) {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compare(mismatches))) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => mismatches.push(format!("{label}: {error}")),
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("non-text panic payload");
            mismatches.push(format!(
                "{label}: native callback comparison panic: {message}"
            ));
        }
    }
}

pub(crate) fn compare_callback_envelope(
    state: &GameState,
    expected: &Value,
    label: &str,
    mismatches: &mut Vec<String>,
) -> Result<()> {
    // 결과 envelope도 complete identity와 lossless state/RNG/history를 확인한다.
    source_callback_state(expected).map_err(|error| {
        invalid(&format!(
            "{label}: reference envelope validation failed: {error}"
        ))
    })?;
    let expected_state = &expected["state"];
    let actual_state = source_state_value(state, expected_state.get("rulesetId").is_some())?;
    compare_value(
        expected_state,
        &actual_state,
        &format!("{label}.state"),
        mismatches,
    )?;
    compare_value(
        &expected["rng"],
        &serde_json::to_value(&state.rng).map_err(EngineError::serialization)?,
        &format!("{label}.rng"),
        mismatches,
    )?;
    compare_value(
        &expected["history"],
        &serde_json::to_value(&state.history).map_err(EngineError::serialization)?,
        &format!("{label}.history"),
        mismatches,
    )?;
    let content = serde_json::json!({
        "protocolVersion": expected["protocolVersion"],
        "rulesVersion": expected["rulesVersion"],
        "catalogVersion": expected["catalogVersion"],
        "state": actual_state,
        "rng": state.rng,
        "history": state.history,
    });
    let mut actual = content
        .as_object()
        .expect("callback envelope content is an object")
        .clone();
    actual.insert("positionId".into(), Value::String(digest(&content)?));
    compare_value(
        expected,
        &Value::Object(actual),
        &format!("{label}.envelope"),
        mismatches,
    )?;
    Ok(())
}

fn source_state_value(state: &GameState, include_ruleset: bool) -> Result<Value> {
    let mut value = serde_json::to_value(state).map_err(EngineError::serialization)?;
    let fields = value
        .as_object_mut()
        .ok_or_else(|| invalid("typed GameState is not an object"))?;
    fields.shift_remove("rng");
    fields.shift_remove("history");
    if !include_ruleset {
        fields.shift_remove("rulesetId");
    }
    Ok(value)
}

fn require_equal(expected: &Value, actual: &Value, label: &str) -> Result<()> {
    if canonical(expected)? != canonical(actual)? {
        let detail = first_difference(expected, actual, label)
            .unwrap_or_else(|| format!("{label}: canonical bytes differ"));
        return Err(invalid(&format!(
            "fixture cannot round-trip without loss: {detail}"
        )));
    }
    Ok(())
}

pub(crate) fn compare_value(
    expected: &Value,
    actual: &Value,
    label: &str,
    mismatches: &mut Vec<String>,
) -> Result<()> {
    // 원문 importer와 같은 depth 64 상한을 native 진단에도 적용해 아래의
    // first_difference 재귀를 유한하게 유지한다.
    validate_json_value(actual, 0)?;
    if canonical(expected)? != canonical(actual)? {
        let detail = first_difference(expected, actual, label)
            .unwrap_or_else(|| format!("{label}: canonical bytes differ"));
        mismatches.push(format!(
            "{detail}; source SHA256={}, native SHA256={}",
            digest(expected)?,
            digest(actual)?
        ));
    }
    Ok(())
}

fn first_difference(expected: &Value, actual: &Value, path: &str) -> Option<String> {
    match (expected, actual) {
        (Value::Number(_), Value::Number(_)) => {
            // serde_json은 정수/실수 표현을 구분하므로 진단도 JCS와 같은
            // JavaScript Number 의미를 사용한다. 실제 값 차이는 유지한다.
            match (canonical(expected), canonical(actual)) {
                (Ok(expected_bytes), Ok(actual_bytes)) if expected_bytes == actual_bytes => None,
                (Ok(_), Ok(_)) => Some(format!("{path}: source={expected}, native={actual}")),
                (Err(error), _) | (_, Err(error)) => Some(format!(
                    "{path}: canonical Number comparison failed: {error}"
                )),
            }
        }
        (Value::Object(expected), Value::Object(actual)) => {
            for (key, value) in expected {
                let path = format!("{path}.{key}");
                let Some(actual) = actual.get(key) else {
                    return Some(format!("{path}: source={value}, native=<missing>"));
                };
                if let Some(difference) = first_difference(value, actual, &path) {
                    return Some(difference);
                }
            }
            for (key, value) in actual {
                if !expected.contains_key(key) {
                    return Some(format!("{path}.{key}: source=<missing>, native={value}"));
                }
            }
            None
        }
        (Value::Array(expected), Value::Array(actual)) => {
            for index in 0..expected.len().max(actual.len()) {
                let path = format!("{path}[{index}]");
                match (expected.get(index), actual.get(index)) {
                    (Some(expected), Some(actual)) => {
                        if let Some(difference) = first_difference(expected, actual, &path) {
                            return Some(difference);
                        }
                    }
                    (expected, actual) => {
                        return Some(format!("{path}: source={expected:?}, native={actual:?}"));
                    }
                }
            }
            None
        }
        _ if expected != actual => Some(format!("{path}: source={expected}, native={actual}")),
        _ => None,
    }
}

fn canonical(value: &Value) -> Result<Vec<u8>> {
    serde_jcs::to_vec(value).map_err(|error| EngineError::Serialization(error.to_string()))
}

fn digest(value: &Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(canonical(value)?)))
}

fn invalid(message: &str) -> EngineError {
    EngineError::InvalidState(format!("raw source callback fixture: {message}"))
}

#[test]
fn canonical_number_diagnostics_preserve_semantic_differences() {
    use serde_json::json;
    assert_eq!(
        canonical(&json!(4)).unwrap(),
        canonical(&json!(4.0)).unwrap()
    );
    assert!(first_difference(&json!(4), &json!(4.0), "state.stars").is_none());
    let expected = json!({"deckSlots":{"stars":4},"replayEvents":[{"type":"king"}]});
    let actual = json!({"deckSlots":{"stars":4.0},"replayEvents":[{}]});
    assert_eq!(
        first_difference(&expected, &actual, "state").unwrap(),
        "state.replayEvents[0].type: source=\"king\", native=<missing>"
    );
    assert!(
        first_difference(&json!(4), &json!(4.5), "state.stars")
            .unwrap()
            .contains("native=4.5")
    );
}
