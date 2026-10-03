use super::*;
use adapter_runtime::{AdapterDescriptor, AdapterSelection, CallLimits};
use augment_chess_engine::Color;

#[test]
fn rng_cursor_machine_limit_fails_before_mutating_state() {
    let mut rng = augment_chess_engine::RngState::seeded(19);
    rng.cursor = usize::MAX;
    let before = rng.clone();
    let error = rng.sample().unwrap_err();
    assert_eq!(error.to_string(), "invalid state: RNG cursor overflow");
    assert_eq!(rng.state, before.state);
    assert_eq!(rng.cursor, before.cursor);
    assert_eq!(rng.tape, before.tape);
}

fn request(
    browser: &BrowserGameSession,
    capability_id: &str,
    payload: GameAdapterPayload,
) -> AdapterRequest<GameAdapterPayload> {
    let metadata: Value = serde_json::from_str(&browser.metadata()).unwrap();
    let descriptors: Vec<AdapterDescriptor> =
        serde_json::from_value(metadata["descriptors"].clone()).unwrap();
    let descriptor = descriptors
        .iter()
        .find(|descriptor| {
            descriptor
                .capabilities
                .iter()
                .any(|capability| capability.id == capability_id)
        })
        .unwrap();
    let capability = descriptor
        .capabilities
        .iter()
        .find(|capability| capability.id == capability_id)
        .unwrap();
    AdapterRequest {
        request_id: "browser-test".into(),
        selection: AdapterSelection {
            project_id: descriptor.project_id.clone(),
            adapter_id: descriptor.adapter_id.clone(),
            contract_version: descriptor.contract_version,
            implementation_version: descriptor.implementation_version.clone(),
            capability_id: capability.id.clone(),
            request_schema: capability.request_schema.clone(),
            response_schema: capability.response_schema.clone(),
        },
        snapshot_revision: browser.revision(),
        call_limits: descriptor.call_limits,
        payload,
    }
}

fn invoke(browser: &mut BrowserGameSession, request: &AdapterRequest<GameAdapterPayload>) -> Value {
    serde_json::from_str(
        &browser
            .invoke_json(&serde_json::to_string(request).unwrap(), 30_000.0)
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn native_and_browser_transport_match_for_three_styles_and_committed_draft() {
    for style in ["normal", "chaos", "grand"] {
        let config = json!({"gameStyle": style});
        let mut browser = BrowserGameSession::new_game(&config.to_string(), 19).unwrap();
        let mut native =
            GameAdapterSession::new_game(serde_json::from_value(config).unwrap(), 19).unwrap();
        let cancellation = NeverCancelled;
        let control = InvocationControl::unlimited_time(&cancellation);
        assert_eq!(browser.revision(), native.position().position_id());
        for viewer in [Color::White, Color::Black] {
            let request = request(&browser, "observe", GameAdapterPayload::Observe { viewer });
            let expected = outcome_json(native.invoke(&request, &control)).unwrap();
            assert_eq!(
                browser
                    .invoke_json(&serde_json::to_string(&request).unwrap(), 30_000.0)
                    .unwrap(),
                expected
            );
        }
        let page_request = request(
            &browser,
            "legal-actions-page",
            GameAdapterPayload::LegalActionsPage {
                limit: 1,
                max_examined: 64,
                cursor: None,
            },
        );
        let page = invoke(&mut browser, &page_request);
        assert_eq!(page["ok"], true);
        let intent = page["response"]["result"]["intents"][0].clone();
        assert!(intent.is_object());
        let apply = request(
            &browser,
            "apply-public-intent",
            GameAdapterPayload::ApplyPublicIntent { intent },
        );
        let expected = outcome_json(native.invoke(&apply, &control)).unwrap();
        assert_eq!(
            browser
                .invoke_json(&serde_json::to_string(&apply).unwrap(), 30_000.0)
                .unwrap(),
            expected
        );
        assert_eq!(browser.revision(), native.position().position_id());
        for viewer in [Color::White, Color::Black] {
            let request = request(&browser, "observe", GameAdapterPayload::Observe { viewer });
            assert_eq!(
                browser
                    .invoke_json(&serde_json::to_string(&request).unwrap(), 30_000.0)
                    .unwrap(),
                outcome_json(native.invoke(&request, &control)).unwrap()
            );
        }
    }
}

#[test]
fn browser_metadata_is_public_and_matches_observed_policy() {
    let mut browser = BrowserGameSession::new_game("{}", 7).unwrap();
    let metadata: Value = serde_json::from_str(&browser.metadata()).unwrap();
    let observe = request(
        &browser,
        "observe",
        GameAdapterPayload::Observe {
            viewer: Color::White,
        },
    );
    let outcome = invoke(&mut browser, &observe);
    let observation = &outcome["response"]["result"]["observation"];
    assert_eq!(metadata["protocolVersion"], observation["protocolVersion"]);
    for key in [
        "rulesVersion",
        "catalogVersion",
        "projectionVersion",
        "observationPolicyHash",
    ] {
        assert_eq!(metadata[key], observation["publicState"][key]);
        assert!(metadata[key].is_string());
    }
    assert_eq!(metadata["descriptors"].as_array().unwrap().len(), 2);
    for private in ["state", "rng", "history", "seed", "position"] {
        assert!(metadata.get(private).is_none());
    }
    assert_eq!(browser.result(), None);
}

#[test]
fn stale_and_budget_failed_writes_preserve_revision_and_public_state() {
    let mut browser = BrowserGameSession::new_game("{}", 19).unwrap();
    let observe = request(
        &browser,
        "observe",
        GameAdapterPayload::Observe {
            viewer: Color::White,
        },
    );
    let before = invoke(&mut browser, &observe);
    let page_request = request(
        &browser,
        "legal-actions-page",
        GameAdapterPayload::LegalActionsPage {
            limit: 1,
            max_examined: 64,
            cursor: None,
        },
    );
    let page = invoke(&mut browser, &page_request);
    let intent = page["response"]["result"]["intents"][0].clone();
    let mut apply = request(
        &browser,
        "apply-public-intent",
        GameAdapterPayload::ApplyPublicIntent { intent },
    );
    let revision = browser.revision();
    apply.call_limits = CallLimits {
        max_work: 1,
        max_results: 0,
    };
    let failed = invoke(&mut browser, &apply);
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["error"]["code"], "result_limit_exceeded");
    assert_eq!(browser.revision(), revision);
    assert_eq!(invoke(&mut browser, &observe), before);
    apply.snapshot_revision = "stale".into();
    let stale = invoke(&mut browser, &apply);
    assert_eq!(stale["error"]["code"], "stale_revision");
    assert_eq!(browser.revision(), revision);
}

#[test]
fn browser_read_preserves_opaque_page_cursor_between_invocations() {
    let mut browser = BrowserGameSession::new_game("{}", 19).unwrap();
    let first_request = request(
        &browser,
        "legal-actions-page",
        GameAdapterPayload::LegalActionsPage {
            limit: 1,
            max_examined: 64,
            cursor: None,
        },
    );
    let first = invoke(&mut browser, &first_request);
    let cursor = first["response"]["result"]["cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    let next_request = request(
        &browser,
        "legal-actions-page",
        GameAdapterPayload::LegalActionsPage {
            limit: 1,
            max_examined: 64,
            cursor: Some(cursor),
        },
    );
    let next = invoke(&mut browser, &next_request);
    assert_eq!(next["ok"], true);
    assert_ne!(
        first["response"]["result"]["intents"],
        next["response"]["result"]["intents"]
    );
    assert_eq!(invoke(&mut browser, &next_request), next);
}

#[test]
fn malformed_json_timeout_and_unknown_schema_return_exact_failures() {
    let mut browser = BrowserGameSession::new_game("{}", 19).unwrap();
    let revision = browser.revision();
    let parse_error: Value =
        serde_json::from_str(&browser.invoke_json("{", 30_000.0).unwrap()).unwrap();
    assert_eq!(parse_error["error"]["code"], "invalid_adapter_request");
    assert!(
        parse_error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("line 1")
    );
    for timeout in [0.0, -1.0, 0.5, f64::NAN, f64::INFINITY, 30_001.0] {
        let error: Value =
            serde_json::from_str(&browser.invoke_json("{}", timeout).unwrap()).unwrap();
        assert_eq!(error["error"]["code"], "invalid_browser_timeout");
    }
    let mut observe = request(
        &browser,
        "observe",
        GameAdapterPayload::Observe {
            viewer: Color::White,
        },
    );
    observe.selection.request_schema.sha256 = "0".repeat(64);
    let error = invoke(&mut browser, &observe);
    assert_eq!(error["error"]["code"], "schema_mismatch");
    assert_eq!(error["error"]["adapterId"], "public-observation");
    assert_eq!(browser.revision(), revision);
    let large_request = " ".repeat(MAX_REQUEST_BYTES + 1);
    let error: Value =
        serde_json::from_str(&browser.invoke_json(&large_request, 30_000.0).unwrap()).unwrap();
    assert_eq!(error["error"]["code"], "browser_request_size_exceeded");
    assert_eq!(browser.revision(), revision);
}

#[test]
fn constructor_refuses_private_envelope_and_reports_original_validation_error() {
    let error = BrowserGameSession::new_game(r#"{"state":{},"rng":{}}"#, 19)
        .err()
        .unwrap();
    let error: Value = serde_json::from_str(&error).unwrap();
    assert_eq!(error["code"], "invalid_game_config");
    assert!(error["message"].as_str().unwrap().contains("unknown field"));
}
