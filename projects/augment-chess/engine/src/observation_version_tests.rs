use super::{card_revelation_for_ruleset, public_hints_v7, validate_projection_for_ruleset};
use crate::state::{
    Color, EngineError, GameConfig, GameState, ObservationPolicy, RULES_VERSION_V6,
    RULES_VERSION_V7, observation_policy_for_ruleset, observation_policy_hash_for_ruleset,
    observation_projection_for_ruleset,
};
use serde_json::json;
use sha2::{Digest, Sha256};

// Source cases are generated outside Git from the fully initialized frozen
// client. This probe isolates the projection body when a separate UI hint
// profile is still closed, so a hint error cannot conceal a visibility leak.
#[test]
#[ignore = "requires source-pinned external JSONL and report"]
fn source_pinned_v7_observation_bodies_match_all_bounded_cases() {
    compare_source_v7_observations(false);
}

// The complete public projection also includes source UI legal hints and its
// information-state key. Keep this separate from body parity so an unsupported
// movement/card hint cannot hide a visibility regression in the body.
#[test]
#[ignore = "requires source-pinned external JSONL and report"]
fn source_pinned_v7_full_observations_match_all_bounded_cases() {
    compare_source_v7_observations(true);
}

// Card first-click targets are a distinct source surface from executable
// card actions. Compare them independently so an unimplemented move profile
// does not conceal a card target discrepancy.
#[test]
#[ignore = "requires source-pinned external JSONL and report"]
fn source_pinned_v7_card_target_hints_match_all_bounded_cases() {
    use crate::v7_host::V7HostPosition;

    let cases = source_observation_cases();
    let mut compared = 0;
    let mut attempted = 0;
    let mut failures = Vec::<String>::new();
    for case in &cases {
        let name = case["name"].as_str().expect("source case name");
        let mut entries = vec![(
            "position".to_owned(),
            &case["position"],
            &case["observations"],
        )];
        for (index, sample) in case["samples"].as_array().unwrap().iter().enumerate() {
            entries.push((
                format!("sample[{index}]"),
                &sample["position"],
                &sample["observations"],
            ));
        }
        for (stage, envelope, observations) in entries {
            let host = V7HostPosition::from_envelope(envelope.clone())
                .unwrap_or_else(|error| panic!("{name} {stage}: source import: {error}"));
            for viewer in [Color::White, Color::Black] {
                attempted += 1;
                let expected =
                    &observations[viewer.as_str()]["publicState"]["legalHints"]["cardTargets"];
                let actual = match crate::card_target_hints::public_card_target_hints_v7(
                    host.state(),
                    viewer,
                ) {
                    Ok(hints) => json!(hints),
                    Err(error) => {
                        failures.push(format!(
                            "{name} {stage} {}: card target projection: {error}",
                            viewer.as_str()
                        ));
                        continue;
                    }
                };
                if serde_jcs::to_vec(&actual).unwrap() != serde_jcs::to_vec(expected).unwrap() {
                    failures.push(format!(
                        "{name} {stage} {}: card targets differ at {}",
                        viewer.as_str(),
                        first_observation_difference(expected, &actual, "$.cardTargets")
                            .unwrap_or_else(|| "canonical representation".into())
                    ));
                    continue;
                }
                compared += 1;
            }
        }
    }
    assert_eq!(attempted, 126, "bounded source observation corpus changed");
    assert!(
        failures.is_empty(),
        "{compared}/{attempted} source card target hints matched; {} gaps:\n{}",
        failures.len(),
        failures
            .iter()
            .take(126)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(compared, 126);
}

#[test]
#[ignore = "requires source-pinned external visibility JSON and its SHA256"]
fn source_pinned_v7_visibility_and_fog_surfaces_match_bounded_cases() {
    use crate::v7_host::V7HostPosition;
    use serde_json::Value;
    use std::fs;

    let path = std::env::var("ACCELERATE_V7_VISIBILITY_SOURCE_CASES")
        .expect("set ACCELERATE_V7_VISIBILITY_SOURCE_CASES to the source receipt");
    let expected_digest = std::env::var("ACCELERATE_V7_VISIBILITY_SOURCE_CASES_SHA256")
        .expect("set ACCELERATE_V7_VISIBILITY_SOURCE_CASES_SHA256 to its verified digest");
    let bytes = fs::read(path).unwrap();
    assert!(
        bytes.len() <= 2 * 1024 * 1024,
        "visibility receipt exceeds 2 MiB"
    );
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected_digest);
    let receipt: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(receipt["schemaVersion"], 1);
    assert_eq!(
        receipt["sourceSha256"],
        "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
    );
    assert_eq!(
        receipt["profile"],
        "accelerate-headless-semantic-v7-faithful-init-v1"
    );
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 10, "bounded visibility corpus changed");
    let mut failures = Vec::new();
    let mut compared = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let host = V7HostPosition::from_envelope(case["position"].clone())
            .unwrap_or_else(|error| panic!("{name}: source position import: {error}"));
        for viewer in [Color::White, Color::Black] {
            let expected = &case["projections"][viewer.as_str()];
            let fog = match super::fog_visible_squares_v7(host.state(), viewer) {
                Ok(fog) => fog,
                Err(error) => {
                    failures.push(format!("{name} {}: fog probe: {error}", viewer.as_str()));
                    continue;
                }
            };
            let actual_fog = json!(
                fog.as_ref()
                    .map(|squares| squares.iter().copied().collect::<Vec<_>>())
            );
            if actual_fog != expected["fog"] {
                failures.push(format!(
                    "{name} {}: fog cells differ at {}",
                    viewer.as_str(),
                    first_observation_difference(&expected["fog"], &actual_fog, "$.fog").unwrap()
                ));
                continue;
            }
            let actual_visible = expected["visible"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| {
                    let at: crate::Square = serde_json::from_value(entry["at"].clone())
                        .map_err(EngineError::serialization)?;
                    let visible = host
                        .state()
                        .at(at)
                        .map(|piece| {
                            super::piece_visible_to_color_at_v7_with_fog(
                                host.state(),
                                piece,
                                at,
                                viewer,
                                fog.as_ref(),
                            )
                        })
                        .transpose()?
                        .unwrap_or(false);
                    Ok(json!({"at":at,"visible":visible}))
                })
                .collect::<crate::Result<Vec<Value>>>();
            let actual_visible = match actual_visible {
                Ok(visible) => json!(visible),
                Err(error) => {
                    failures.push(format!(
                        "{name} {}: piece visibility: {error}",
                        viewer.as_str()
                    ));
                    continue;
                }
            };
            if actual_visible != expected["visible"] {
                failures.push(format!(
                    "{name} {}: piece visibility differs at {}",
                    viewer.as_str(),
                    first_observation_difference(
                        &expected["visible"],
                        &actual_visible,
                        "$.visible"
                    )
                    .unwrap()
                ));
                continue;
            }
            let surface = match super::board_surface_v7_with_fog(host.state(), viewer, fog.as_ref())
            {
                Ok(surface) => surface,
                Err(error) => {
                    failures.push(format!(
                        "{name} {}: renderer surface: {error}",
                        viewer.as_str()
                    ));
                    continue;
                }
            };
            if serde_jcs::to_vec(&surface).unwrap()
                != serde_jcs::to_vec(&expected["surface"]).unwrap()
            {
                failures.push(format!(
                    "{name} {}: surface differs at {}",
                    viewer.as_str(),
                    first_observation_difference(&expected["surface"], &surface, "$.surface")
                        .unwrap()
                ));
                continue;
            }
            compared += 1;
        }
    }
    assert!(
        failures.is_empty(),
        "{compared}/20 pinned visibility projections matched; {} gaps:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(compared, 20);
}

fn source_observation_cases() -> Vec<serde_json::Value> {
    use serde_json::Value;
    use std::fs;

    let cases_path = std::env::var("ACCELERATE_V7_OBSERVATION_CASES")
        .expect("set ACCELERATE_V7_OBSERVATION_CASES to the source JSONL path");
    let report_path = std::env::var("ACCELERATE_V7_OBSERVATION_SOURCE_REPORT")
        .expect("set ACCELERATE_V7_OBSERVATION_SOURCE_REPORT to its source report path");
    let report: Value = serde_json::from_slice(&fs::read(report_path).unwrap()).unwrap();
    assert_eq!(
        report["source"]["sha256"],
        "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
    );
    assert_eq!(
        report["source"]["profile"],
        "accelerate-headless-semantic-v7-faithful-init-v1"
    );
    assert_eq!(report["sourceExport"]["cases"], 21);
    let bytes = fs::read(cases_path).unwrap();
    assert!(bytes.len() <= 8 * 1024 * 1024, "source cases exceed 8 MiB");
    assert_eq!(
        report["sourceExport"]["sha256"],
        format!("{:x}", Sha256::digest(&bytes))
    );
    let text = std::str::from_utf8(&bytes).unwrap();
    let cases = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 21, "bounded source corpus changed");
    cases
}

fn compare_source_v7_observations(include_hints: bool) {
    use crate::v7_host::V7HostPosition;

    let cases = source_observation_cases();
    let mut compared = 0;
    let mut attempted = 0;
    let mut failures = Vec::<String>::new();
    for case in &cases {
        let name = case["name"].as_str().expect("source case name");
        let mut entries = vec![(
            "position".to_owned(),
            &case["position"],
            &case["observations"],
        )];
        for (index, sample) in case["samples"].as_array().unwrap().iter().enumerate() {
            entries.push((
                format!("sample[{index}]"),
                &sample["position"],
                &sample["observations"],
            ));
        }
        for (stage, envelope, observations) in entries {
            let host = V7HostPosition::from_envelope(envelope.clone())
                .unwrap_or_else(|error| panic!("{name} {stage}: source import: {error}"));
            for viewer in [Color::White, Color::Black] {
                attempted += 1;
                let mut expected = observations[viewer.as_str()].clone();
                if !include_hints {
                    expected
                        .as_object_mut()
                        .unwrap()
                        .remove("informationStateKey");
                    expected["publicState"]
                        .as_object_mut()
                        .unwrap()
                        .remove("legalHints");
                }
                let actual = match if include_hints {
                    host.state().try_observe(viewer)
                } else {
                    host.state().observe_checked(viewer)
                } {
                    Ok(observation) => observation,
                    Err(error) if include_hints => {
                        failures.push(format!(
                            "{name} {stage} {}: projection: {error}",
                            viewer.as_str()
                        ));
                        continue;
                    }
                    Err(error) => {
                        panic!("{name} {stage} {}: projection: {error}", viewer.as_str())
                    }
                };
                let mut actual = serde_json::to_value(actual).unwrap();
                if !include_hints {
                    actual
                        .as_object_mut()
                        .unwrap()
                        .remove("informationStateKey");
                }
                let expected_jcs = serde_jcs::to_vec(&expected).unwrap();
                let actual_jcs = serde_jcs::to_vec(&actual).unwrap();
                if actual_jcs != expected_jcs {
                    let detail = format!(
                        "{name} {stage} {}: {} observation differs at {}",
                        viewer.as_str(),
                        if include_hints { "full" } else { "body" },
                        first_observation_difference(&expected, &actual, "$")
                            .unwrap_or_else(|| "canonical representation".into())
                    );
                    if include_hints {
                        failures.push(detail);
                        continue;
                    }
                    panic!("{detail}");
                }
                compared += 1;
            }
        }
    }
    assert_eq!(attempted, 126, "bounded source observation corpus changed");
    assert!(
        failures.is_empty(),
        "{compared}/{attempted} full source observations matched; {} gaps:\n{}",
        failures.len(),
        failures
            .iter()
            .take(126)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(compared, 126);
}

fn first_observation_difference(
    expected: &serde_json::Value,
    actual: &serde_json::Value,
    path: &str,
) -> Option<String> {
    use serde_json::Value;
    if serde_jcs::to_vec(expected).ok()? == serde_jcs::to_vec(actual).ok()? {
        return None;
    }
    match (expected, actual) {
        (Value::Object(left), Value::Object(right)) => {
            for key in left.keys().chain(right.keys()) {
                let field_path = format!("{path}.{key}");
                match (left.get(key), right.get(key)) {
                    (Some(a), Some(b)) => {
                        if let Some(difference) = first_observation_difference(a, b, &field_path) {
                            return Some(difference);
                        }
                    }
                    _ => return Some(field_path),
                }
            }
            Some(path.into())
        }
        (Value::Array(left), Value::Array(right)) => {
            for (index, (a, b)) in left.iter().zip(right).enumerate() {
                if let Some(difference) =
                    first_observation_difference(a, b, &format!("{path}[{index}]"))
                {
                    return Some(difference);
                }
            }
            Some(format!("{path}[{}]", left.len().min(right.len())))
        }
        _ => Some(path.into()),
    }
}

#[test]
fn pinned_observation_metadata_matches_both_js_contract_digests() {
    let versions: [(&str, &str, &str); 2] = [
        (
            RULES_VERSION_V6,
            "source-visible-20260927-v3",
            "5e17b5622f1e761d6e0719187006aaaac336150c4ae2372f76ef0080da0ab037",
        ),
        (
            RULES_VERSION_V7,
            "source-visible-20260928-v1",
            "baa57576dd60387813e87fc0dbfc095ce68c951fc51797031b22be62a419c270",
        ),
    ];
    for (ruleset, projection, hash) in versions {
        let _: &ObservationPolicy = observation_policy_for_ruleset(ruleset).unwrap();
        assert_eq!(
            observation_projection_for_ruleset(ruleset).unwrap(),
            projection
        );
        assert_eq!(observation_policy_hash_for_ruleset(ruleset).unwrap(), hash);
    }
    assert!(matches!(
        observation_policy_for_ruleset("unknown-ruleset"),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert!(matches!(
        observation_policy_hash_for_ruleset("unknown-ruleset"),
        Err(EngineError::UnsupportedFeature(_))
    ));
}

#[test]
fn box_revelation_uses_the_selected_catalog_and_card_lifecycle() {
    let card = serde_json::from_value(json!({
        "id": "black-box",
        "effect": "blackBox",
        "used": true,
        "boxRevealedCardId": "random-roulette"
    }))
    .unwrap();
    for version in [RULES_VERSION_V6, RULES_VERSION_V7] {
        assert_eq!(
            card_revelation_for_ruleset(&card, version).unwrap(),
            Some(json!({"boxCardId": "random-roulette"}))
        );
    }
    let mut unresolved = card.clone();
    unresolved.used = false;
    assert_eq!(
        card_revelation_for_ruleset(&unresolved, RULES_VERSION_V7).unwrap(),
        None
    );
    unresolved.used = true;
    unresolved
        .extra
        .insert("boxRevealedCardId".into(), json!("unknown-card"));
    assert_eq!(
        card_revelation_for_ruleset(&unresolved, RULES_VERSION_V7).unwrap(),
        None
    );
    assert!(matches!(
        card_revelation_for_ruleset(&card, "unknown-ruleset"),
        Err(EngineError::UnsupportedFeature(_))
    ));
}

#[test]
fn v7_empty_hint_windows_use_their_own_policy_and_supported_play_opens() {
    let mut state = GameState::new(GameConfig::default(), 41).unwrap();
    let v6 = state.try_observe(Color::White).unwrap();
    assert_eq!(
        v6.public_state["observationPolicyHash"],
        observation_policy_hash_for_ruleset(RULES_VERSION_V6).unwrap()
    );
    validate_projection_for_ruleset(&v6, RULES_VERSION_V6).unwrap();
    assert!(matches!(
        validate_projection_for_ruleset(&v6, RULES_VERSION_V7),
        Err(EngineError::InvalidState(_))
    ));
    state.ruleset_id = RULES_VERSION_V7.into();
    state
        .extra
        .insert("othelloPending".into(), json!({"white":true,"black":false}));
    let draft = state.try_observe(Color::White).unwrap();
    assert_eq!(
        draft.public_state["observationPolicyHash"],
        observation_policy_hash_for_ruleset(RULES_VERSION_V7).unwrap()
    );
    assert!(!draft.public_state.contains_key("othelloPending"));
    validate_projection_for_ruleset(&draft, RULES_VERSION_V7).unwrap();
    assert!(matches!(
        validate_projection_for_ruleset(&draft, RULES_VERSION_V6),
        Err(EngineError::InvalidState(_))
    ));
    assert!(std::panic::catch_unwind(|| state.observe(Color::White)).is_err());
    state.mode = "play".into();
    let play = state.try_observe(Color::White).unwrap();
    assert_eq!(
        play.public_state["observationPolicyHash"],
        observation_policy_hash_for_ruleset(RULES_VERSION_V7).unwrap()
    );
    assert!(!play.public_state.contains_key("othelloPending"));
    // 정상 초기 폰의 공개 목적지는 원문 getLegalMoves의 순서를 유지한다.
    // 내부 Othello 대기 정보는 이 목록이나 공개 상태에 섞이지 않는다.
    let pawn = play.public_state["legalHints"]["moves"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hint| hint["from"] == json!({"row":6,"col":0}))
        .unwrap();
    assert_eq!(
        pawn["destinations"],
        json!([{"row":5,"col":0},{"row":4,"col":0}])
    );
    validate_projection_for_ruleset(&play, RULES_VERSION_V7).unwrap();
    state.mode = "gameover".into();
    let terminal = state.try_observe(Color::White).unwrap();
    assert_eq!(
        terminal.public_state["legalHints"],
        json!({"moves":[],"cardTargets":[]})
    );
    validate_projection_for_ruleset(&terminal, RULES_VERSION_V7).unwrap();
    state.ruleset_id = "unknown-ruleset".into();
    assert!(matches!(
        state.try_observe(Color::White),
        Err(EngineError::UnsupportedFeature(_))
    ));
}

#[test]
fn v7_hints_follow_active_windows_and_reject_unknown_rule_inputs() {
    let mut state = GameState::new(GameConfig::default(), 41).unwrap();
    state.ruleset_id = RULES_VERSION_V7.into();
    let empty = json!({"moves":[],"cardTargets":[]});
    assert_eq!(state.mode, "draft");
    assert_eq!(public_hints_v7(&state, Color::White).unwrap(), empty);

    state.mode = "gameover".into();
    assert_eq!(public_hints_v7(&state, Color::White).unwrap(), empty);
    state.mode = "play".into();
    state.turn = Color::White;
    assert_eq!(public_hints_v7(&state, Color::Black).unwrap(), empty);

    state
        .extra
        .insert("pendingPromotion".into(), json!({"color":"white"}));
    assert_eq!(public_hints_v7(&state, Color::White).unwrap(), empty);
    state.extra.remove("pendingPromotion");
    state
        .extra
        .insert("activeTrolley".into(), json!({"color":"white"}));
    assert_eq!(public_hints_v7(&state, Color::White).unwrap(), empty);
    state.extra.remove("activeTrolley");
    let before = state.clone();
    let active = public_hints_v7(&state, Color::White).unwrap();
    let moves = active["moves"].as_array().unwrap();
    assert_eq!(moves.len(), 10);
    assert_eq!(
        moves
            .iter()
            .map(|hint| hint["destinations"].as_array().unwrap().len())
            .sum::<usize>(),
        20
    );
    assert_eq!(active["cardTargets"], json!([]));
    assert_eq!(
        state, before,
        "공개 힌트 조회가 보드/RNG/내부 대기 정보를 바꿨습니다"
    );

    // 일반 play 창이 열려도 알 수 없는 기하를 빈 힌트로 숨기면 안 된다.
    let unknown_kind = "unknown-v7-observation-piece";
    state.board[6][0].as_mut().unwrap().kind = unknown_kind.into();
    let error = public_hints_v7(&state, Color::White).unwrap_err();
    assert!(matches!(&error, EngineError::UnsupportedFeature(_)));
    assert!(
        error.to_string().contains(unknown_kind),
        "알 수 없는 기물의 정확한 오류가 사라졌습니다: {error}"
    );
    state.ruleset_id = RULES_VERSION_V6.into();
    assert!(matches!(
        public_hints_v7(&state, Color::White),
        Err(EngineError::UnsupportedFeature(_))
    ));
}

#[test]
fn v7_seed19_normal_first_play_hints_match_source_ordered_destinations() {
    // The frozen v7 client shows 10 origins/20 destinations for the acting
    // white viewer after Relay and Berserker are selected at seed 19. Relay
    // has no target hint; the non-acting viewer has no hints at all.
    let mut state =
        crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7).unwrap();
    let white_pick = crate::draft::legal_actions(&state).unwrap().remove(1);
    crate::draft::apply_pick(&mut state, &white_pick).unwrap();
    crate::replay::canonicalize_position_frames(&mut state).unwrap();
    let black_pick = crate::draft::legal_actions(&state).unwrap().remove(0);
    crate::transition::apply(&mut state, &black_pick).unwrap();
    assert_eq!(state.mode, "play");
    assert_eq!(state.deck_slots.white[0].id, "relay");
    assert_eq!(state.deck_slots.black[0].id, "berserker");

    let observed = state.try_observe(Color::White).unwrap();
    let hints = &observed.public_state["legalHints"];
    assert_eq!(hints["cardTargets"], json!([]));
    let moves = hints["moves"].as_array().unwrap();
    assert_eq!(moves.len(), 10);
    let mut pairs = Vec::new();
    for move_hint in moves {
        let from = &move_hint["from"];
        for destination in move_hint["destinations"].as_array().unwrap() {
            pairs.push(format!(
                "{},{}->{},{}",
                from["row"], from["col"], destination["row"], destination["col"]
            ));
        }
    }
    assert_eq!(
        pairs,
        [
            "6,0->5,0", "6,0->4,0", "6,1->5,1", "6,1->4,1", "6,2->5,2", "6,2->4,2", "6,3->5,3",
            "6,3->4,3", "6,4->5,4", "6,4->4,4", "6,5->5,5", "6,5->4,5", "6,6->5,6", "6,6->4,6",
            "6,7->5,7", "6,7->4,7", "7,1->5,0", "7,1->5,2", "7,6->5,5", "7,6->5,7",
        ]
    );
    assert_eq!(
        state.try_observe(Color::Black).unwrap().public_state["legalHints"],
        json!({"moves":[], "cardTargets":[]})
    );
}

#[test]
fn v7_seed19_chaos_target_hints_match_source_ui_selection() {
    // Frozen v7 first-play source: one primary Queen's Gambit target, while
    // no-target Reposition has no hint entry. These are UI hints, not the
    // complete legal card action stream.
    let mut state = crate::draft::initialize_for_ruleset(
        GameConfig {
            game_style: "chaos".into(),
            ..GameConfig::default()
        },
        19,
        RULES_VERSION_V7,
    )
    .unwrap();
    let white_pick = crate::draft::legal_actions(&state).unwrap().remove(1);
    crate::draft::apply_pick(&mut state, &white_pick).unwrap();
    crate::replay::canonicalize_position_frames(&mut state).unwrap();
    let black_pick = crate::draft::legal_actions(&state).unwrap().remove(2);
    crate::draft::apply_pick(&mut state, &black_pick).unwrap();
    crate::replay::canonicalize_position_frames(&mut state).unwrap();
    assert_eq!(state.mode, "play");
    assert_eq!(state.deck_slots.white[0].id, "queens-gambit");
    let hints = &state.try_observe(Color::White).unwrap().public_state["legalHints"];
    assert_eq!(hints["moves"].as_array().unwrap().len(), 10);
    assert_eq!(
        hints["cardTargets"],
        json!([{"cardInstanceId":"queens-gambit-5iuubljvcig", "targets":[{"row":7,"col":3}]}])
    );
    assert_eq!(
        state.try_observe(Color::Black).unwrap().public_state["legalHints"],
        json!({"moves":[], "cardTargets":[]})
    );
}

#[test]
#[ignore = "requires freshly regenerated faithful seed19 full observations and verified SHA256"]
fn source_pinned_v7_seed19_full_observations_match_regenerated_first_play() {
    use crate::V7HostPosition;
    use crate::tests::source_callback_fixture::{collect_case_diagnostics, compare_value};
    use serde_json::Value;

    let path = std::env::var("ACCELERATE_V7_SEED19_FULL_OBSERVATION_CASES").expect(
        "set ACCELERATE_V7_SEED19_FULL_OBSERVATION_CASES to the regenerated source receipt",
    );
    let expected_sha = std::env::var("ACCELERATE_V7_SEED19_FULL_OBSERVATION_CASES_SHA256")
        .expect("set ACCELERATE_V7_SEED19_FULL_OBSERVATION_CASES_SHA256 to its verified digest");
    let bytes = std::fs::read(path).unwrap();
    assert!(
        bytes.len() <= 8 * 1024 * 1024,
        "seed19 full-observation receipt exceeds 8 MiB"
    );
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected_sha);
    let receipt: Value = serde_json::from_slice(&bytes).unwrap();
    crate::state::validate_json_value(&receipt, 0).unwrap();
    let catalog: Value =
        serde_json::from_str(include_str!("../../contracts/catalog/site-20260928.json")).unwrap();
    let profile: Value = serde_json::from_str(include_str!(
        "../../contracts/catalog/execution-profile-20260928.json"
    ))
    .unwrap();
    assert_eq!(receipt["schemaVersion"], 1);
    assert_eq!(
        receipt["sourceSha256"],
        "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
    );
    assert_eq!(
        receipt["profile"],
        "accelerate-headless-semantic-v7-faithful-init-v1"
    );
    assert_eq!(
        receipt["executionProfileSha256"],
        json!(format!(
            "{:x}",
            Sha256::digest(serde_jcs::to_vec(&profile).unwrap())
        ))
    );
    assert_eq!(receipt["initializerCount"], 175);
    assert_eq!(
        receipt["sourcePublicCatalogHash"],
        catalog["sourcePublicCatalogHash"]
    );
    assert_eq!(receipt["catalogVersion"], catalog["catalogVersion"]);
    assert_eq!(
        receipt["observationPolicyHash"],
        observation_policy_hash_for_ruleset(RULES_VERSION_V7).unwrap()
    );
    assert_eq!(
        receipt["scope"],
        "seed19 fixed golden offer sequence to first play; normal/chaos/grand and both full public viewers; all observation fields included"
    );
    let cases = receipt["cases"].as_array().unwrap();
    let openings: [(&str, &[usize]); 3] = [
        ("normal", &[1, 0]),
        ("chaos", &[1, 2]),
        ("grand", &[1, 3, 3, 3, 3, 3, 3, 3, 5, 5, 5, 5]),
    ];
    assert_eq!(
        cases.len(),
        openings.len(),
        "all three fixed golden opening constructions are required"
    );
    let mut mismatches = Vec::new();
    let mut attempted = 0;
    let mut compared = 0;
    for (case, (style, indexes)) in cases.iter().zip(openings) {
        assert_eq!(
            case["name"], style,
            "first-play receipt cases are missing, repeated or reordered"
        );
        collect_case_diagnostics(style, &mut mismatches, |mismatches| {
            let construction_before = mismatches.len();
            compare_value(
                &json!({"gameStyle":style}),
                &case["config"],
                &format!("{style}.config"),
                mismatches,
            )?;
            compare_value(
                &json!(19),
                &case["seed"],
                &format!("{style}.seed"),
                mismatches,
            )?;
            compare_value(
                &json!(indexes),
                &case["offerIndexes"],
                &format!("{style}.offerIndexes"),
                mismatches,
            )?;
            let initial = V7HostPosition::from_envelope(case["initial"].clone())?;
            let source_play = V7HostPosition::from_envelope(case["position"].clone())?;
            if initial.state().mode != "draft" || source_play.state().mode != "play" {
                return Err(EngineError::InvalidState(
                    "source golden opening did not traverse draft to first play".into(),
                ));
            }
            let draft_actions = case["draftActions"]
                .as_array()
                .filter(|actions| actions.len() == indexes.len())
                .ok_or_else(|| {
                    EngineError::InvalidState(
                        "source fixed offer sequence requires every draft payload".into(),
                    )
                })?;
            // source Position을 native에 바로 주입하지 않는다. 기존 golden과
            // 같은 native 초기화/선택/프레임 정산 경로를 다시 구성해 전체를 비교한다.
            let mut state = crate::draft::initialize_for_ruleset(
                GameConfig {
                    game_style: style.into(),
                    ..GameConfig::default()
                },
                19,
                RULES_VERSION_V7,
            )?;
            for (step, &index) in indexes.iter().enumerate() {
                let pick = crate::draft::legal_actions(&state)?
                    .get(index)
                    .cloned()
                    .ok_or_else(|| {
                        EngineError::InvalidState(format!(
                            "{style} draft step {step}: native offer {index} missing"
                        ))
                    })?;
                compare_value(
                    &draft_actions[step],
                    &serde_json::to_value(&pick).map_err(EngineError::serialization)?,
                    &format!("{style}.draft[{step}].payload"),
                    mismatches,
                )?;
                crate::draft::apply_pick(&mut state, &pick)?;
                crate::replay::canonicalize_position_frames(&mut state)?;
            }
            if state.mode != "play" {
                return Err(EngineError::InvalidState(format!(
                    "{style}: native fixed golden draft did not reach first play"
                )));
            }
            let construction_matches = mismatches.len() == construction_before;
            for viewer in [Color::White, Color::Black] {
                attempted += 1;
                let label = format!("{style}.{}", viewer.as_str());
                collect_case_diagnostics(&label, mismatches, |mismatches| {
                    let count_before = mismatches.len();
                    let expected = case["observations"]
                        .get(viewer.as_str())
                        .filter(|value| value.is_object())
                        .ok_or_else(|| {
                            EngineError::InvalidState(format!(
                                "{label}: source full observation missing"
                            ))
                        })?;
                    let source_observation: crate::state::Observation =
                        serde_json::from_value(expected.clone())
                            .map_err(EngineError::serialization)?;
                    validate_projection_for_ruleset(&source_observation, RULES_VERSION_V7)?;
                    compare_value(
                        &catalog["catalogVersion"],
                        &expected["publicState"]["catalogVersion"],
                        &format!("{label}.source.catalogVersion"),
                        mismatches,
                    )?;
                    let mut content = expected.clone();
                    content
                        .as_object_mut()
                        .unwrap()
                        .remove("informationStateKey");
                    let source_key = format!(
                        "{:x}",
                        Sha256::digest(
                            serde_jcs::to_vec(&content).map_err(EngineError::serialization)?
                        )
                    );
                    compare_value(
                        &json!(source_key),
                        &expected["informationStateKey"],
                        &format!("{label}.source.informationStateKey"),
                        mismatches,
                    )?;
                    let source_digest = format!(
                        "{:x}",
                        Sha256::digest(
                            serde_jcs::to_vec(expected).map_err(EngineError::serialization)?
                        )
                    );
                    compare_value(
                        &json!(source_digest),
                        &case["digests"][viewer.as_str()]["observationDigest"],
                        &format!("{label}.source.observationDigest"),
                        mismatches,
                    )?;
                    compare_value(
                        &expected["informationStateKey"],
                        &case["digests"][viewer.as_str()]["informationStateKey"],
                        &format!("{label}.source.digestInformationStateKey"),
                        mismatches,
                    )?;
                    let actual = serde_json::to_value(state.try_observe(viewer)?)
                        .map_err(EngineError::serialization)?;
                    // informationStateKey, legalHints와 모든 derived/public 필드를 포함한다.
                    compare_value(
                        expected,
                        &actual,
                        &format!("{label}.fullObservation"),
                        mismatches,
                    )?;
                    if construction_matches && mismatches.len() == count_before {
                        compared += 1;
                        eprintln!(
                            "{label}: full source/native JCS matched; observationDigest={source_digest}, informationStateKey={source_key}"
                        );
                    }
                    Ok(())
                });
            }
            Ok(())
        });
    }
    assert!(
        mismatches.is_empty(),
        "{compared}/{attempted} regenerated full source observations matched (expected 6); {} gaps:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    assert_eq!(
        attempted, 6,
        "every source opening must exercise both complete public viewers"
    );
    assert_eq!(
        compared, 6,
        "golden digests may be updated only after all six complete observations match"
    );
}

#[test]
fn v7_seed19_full_observations_match_frozen_source_for_both_viewers() {
    // These SHA-256/JCS digests and informationStateKeys come from
    // FrozenClientSource main-OahWs0tU.js with reviewed top-level initializers
    // at SHA-256
    // e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c.
    // Full source/native JCS comparison evidence stays outside Git at
    // %APPDATA%/Accelerate/reports/v7-public-hints/full-observation.
    type ViewerDigests = (&'static str, &'static str);
    type OpeningCase = (&'static str, &'static [usize], [ViewerDigests; 2]);
    let cases: [OpeningCase; 3] = [
        (
            "normal",
            &[1, 0],
            [
                (
                    "86d53ee66f05e6fe2255af096b17dacc147fb330f0da4503a7cfa7347a5e1044",
                    "f4e94c2e164d00021edcbae7e02ece8ac20708afca2cdc20be4dc9284faaa650",
                ),
                (
                    "8022d52672f8f45f8400f5c5e99caff33b4f6f568959a34f7925f623a9ec39f3",
                    "2af0ad0b24b21839b1832d815674a355d60d0538494b4c2f94a96a504cdc326a",
                ),
            ],
        ),
        (
            "chaos",
            &[1, 2],
            [
                (
                    "b51066d043bdc337d536b6772b2b7236cd8e72b15e1b9ae0b6c2a85523f3cae7",
                    "1d222c63d9e6930ed51474929b9a9ae4ea55c4e098023b65eb64d0517a039b8e",
                ),
                (
                    "8a49a9fef52f6c058ae02a91dc6751d609ceb2663bd2e3444a27c67dbfa2e442",
                    "56cde3d9ac7b3eaf37e697e1225e893c12901115f19f629cc311ff143058191a",
                ),
            ],
        ),
        (
            "grand",
            &[1, 3, 3, 3, 3, 3, 3, 3, 5, 5, 5, 5],
            [
                (
                    "9aeb7af41047a5739ca045cd7ff2e294e2a3badc11b58453abd5984fc005ff6f",
                    "1ad50399aa2b3d10e950a8fc9daeab18e23169c6eec59273af5e474c4d3afc59",
                ),
                (
                    "2e19348734676f5ed14d080e63c8b9fb36ee6d44b79cd0444a7f1b0c0ff9749e",
                    "bc2ef5547bc0329b946d4f1a907cbd1830fd170c969fa8358732345240127bc9",
                ),
            ],
        ),
    ];
    for (style, offers, expected) in cases {
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        for &index in offers {
            let pick = crate::draft::legal_actions(&state).unwrap().remove(index);
            crate::draft::apply_pick(&mut state, &pick).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        for (viewer, (full_digest, information_key)) in
            [Color::White, Color::Black].into_iter().zip(expected)
        {
            let observation = state.try_observe(viewer).unwrap();
            let canonical = serde_jcs::to_vec(&observation).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(canonical)),
                full_digest,
                "{style} {} full Observation v2",
                viewer.as_str()
            );
            assert_eq!(
                observation.information_state_key,
                information_key,
                "{style} {} informationStateKey",
                viewer.as_str()
            );
        }
    }
}
