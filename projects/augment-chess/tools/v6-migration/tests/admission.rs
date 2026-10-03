use augment_chess_v6_migration::{
    CATALOG_VERSION, MAX_INPUT_BYTES, MigrationError, POSITION_PROTOCOL_VERSION, V6_RULES_VERSION,
    V7_CATALOG_VERSION, V7_RULES_VERSION, convert_verified_initial_template, read_v6_position,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(serde_jcs::to_vec(value).unwrap()))
}

fn sample() -> Value {
    let template: Value = serde_json::from_str(include_str!(
        "../../../contracts/catalog/initial-state-20260927.json"
    ))
    .unwrap();
    let mut position = json!({
        "protocolVersion": POSITION_PROTOCOL_VERSION,
        "rulesVersion": V6_RULES_VERSION,
        "catalogVersion": CATALOG_VERSION,
        "state": template["state"],
        "rng": {"algorithm":"lcg32-v1","state":19,"cursor":0,"tape":[]},
        "history": []
    });
    position["positionId"] = json!(digest(&position));
    position
}

fn parse(value: &Value) -> Result<augment_chess_v6_migration::V6Position, MigrationError> {
    read_v6_position(&serde_json::to_vec(value).unwrap())
}

fn resign(value: &mut Value) {
    value.as_object_mut().unwrap().remove("positionId");
    value["positionId"] = json!(digest(value));
}

#[test]
fn exact_initial_template_converts_to_a_v7_data_envelope() {
    let source = parse(&sample()).unwrap();
    let converted = convert_verified_initial_template(&source).unwrap();
    let output = converted.envelope();
    assert_eq!(output["rulesVersion"], V7_RULES_VERSION);
    assert_eq!(output["catalogVersion"], V7_CATALOG_VERSION);
    assert_eq!(source.envelope()["catalogVersion"], CATALOG_VERSION);
    assert_eq!(output["state"], source.envelope()["state"]);
    assert_eq!(output["rng"], source.envelope()["rng"]);
    assert_ne!(output["positionId"], source.position_id());
    let mut unsigned = output.clone();
    unsigned.as_object_mut().unwrap().remove("positionId");
    assert_eq!(output["positionId"], digest(&unsigned));
    assert_eq!(
        converted.evidence().source_position_id,
        source.position_id()
    );
    assert_eq!(
        converted.evidence().verified_scope,
        "frozen-initial-template-no-history-pre-draw"
    );
}

#[test]
fn reader_preserves_unknown_state_and_history_but_converter_names_unsupported_scope() {
    let mut source = sample();
    source["state"]["opaqueRuleState"] = json!({"sourceOnly": [3, 1, 4]});
    source["history"] = json!([{"protocolVersion":"accelerate-game-event-v1","opaque":true}]);
    resign(&mut source);
    let imported = parse(&source).unwrap();
    assert_eq!(imported.envelope(), &source);
    assert!(matches!(
        convert_verified_initial_template(&imported),
        Err(MigrationError::Unsupported { path, .. }) if path == "state"
    ));
    source["state"]
        .as_object_mut()
        .unwrap()
        .remove("opaqueRuleState");
    resign(&mut source);
    let imported = parse(&source).unwrap();
    assert!(matches!(
        convert_verified_initial_template(&imported),
        Err(MigrationError::Unsupported { path, .. }) if path == "history"
    ));
}

#[test]
fn converter_rejects_consumed_rng_even_when_state_is_identical() {
    let mut source = sample();
    source["rng"]["cursor"] = json!(1);
    resign(&mut source);
    let imported = parse(&source).unwrap();
    assert!(matches!(
        convert_verified_initial_template(&imported),
        Err(MigrationError::Unsupported { path, .. }) if path == "rng"
    ));
}

#[test]
fn reader_checks_identity_and_fixed_source_versions() {
    let mut source = sample();
    source["positionId"] = json!("0".repeat(64));
    assert!(matches!(
        parse(&source),
        Err(MigrationError::IdentityMismatch { .. })
    ));
    resign(&mut source);
    source["rulesVersion"] = json!(V7_RULES_VERSION);
    resign(&mut source);
    assert!(
        matches!(parse(&source), Err(MigrationError::InvalidInput(message)) if message.contains("rulesVersion"))
    );
}

#[test]
fn reader_rejects_ambiguous_and_malformed_transport_without_rule_execution() {
    let mut source = sample();
    source["rng"]["cursor"] = json!(-1);
    resign(&mut source);
    assert!(
        matches!(parse(&source), Err(MigrationError::InvalidInput(message)) if message.contains("rng.cursor"))
    );

    let mut source = sample();
    source["state"]["board"][0] = json!([]);
    resign(&mut source);
    assert!(
        matches!(parse(&source), Err(MigrationError::InvalidInput(message)) if message.contains("board[0]"))
    );

    let bytes = serde_json::to_string(&sample()).unwrap();
    let duplicate = bytes.replacen(
        "\"protocolVersion\":",
        "\"protocolVersion\":\"accelerate-position-v1\",\"protocolVersion\":",
        1,
    );
    assert!(matches!(
        read_v6_position(duplicate.as_bytes()),
        Err(MigrationError::InvalidInput(message)) if message.contains("duplicate JSON key protocolVersion")
    ));
}

#[test]
fn reader_reports_history_and_size_failures_explicitly() {
    let mut source = sample();
    source["history"] = json!([{"protocolVersion":"unexpected"}]);
    resign(&mut source);
    assert!(matches!(
        parse(&source),
        Err(MigrationError::InvalidInput(message)) if message.contains("history[0]")
    ));

    let oversized = vec![b' '; MAX_INPUT_BYTES + 1];
    assert!(matches!(
        read_v6_position(&oversized),
        Err(MigrationError::LimitExceeded(message)) if message.contains("8 MiB")
    ));
}
