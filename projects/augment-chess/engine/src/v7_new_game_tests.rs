use super::new_game;
use crate::{EngineError, GameConfig, GameState};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

fn source_state_digest(state: &GameState) -> String {
    let mut state = serde_json::to_value(state).unwrap();
    let fields = state.as_object_mut().unwrap();
    for outer in ["rulesetId", "rng", "history"] {
        fields.remove(outer);
    }
    format!("{:x}", Sha256::digest(serde_jcs::to_vec(&state).unwrap()))
}

#[test]
fn v7_initial_draft_matches_frozen_source_across_styles_and_seeds() {
    // Direct FrozenClientSource at SHA-256
    // e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c /
    // newGame(...), with the reviewed top-level initializers and the
    // site-20260928 runtime contract. These
    // full-state JCS hashes cover the draft offer, replay frames, clock and
    // all private setup fields without committing the full source snapshots.
    let cases = [
        (
            "normal",
            0,
            "f5bc9e29e9ae00126d892edf39cf4d2fbb20c5ffa3f6824800bb1c0fd5975308",
            122,
            3565086410_u32,
        ),
        (
            "normal",
            37,
            "ec494d043ba3c535229308b391de4660beff1961c3eabb0fe0bd06881f05f8ff",
            122,
            2125196183,
        ),
        (
            "normal",
            19,
            "3b753013dca2945893beee75945e792c9dc2d35897533ede99f3e8e30266be72",
            122,
            1316640757,
        ),
        (
            "normal",
            12345,
            "58182467c55a79aee0808850a892bf14550f08fbb30ee8e52bfb7b0310cec0fb",
            122,
            469428811,
        ),
        (
            "normal",
            u64::from(u32::MAX),
            "ee4a3b61af4f5571479ec6d58e57e227af78fe5b731f795bdc39f0624360c633",
            122,
            2327120193,
        ),
        (
            "chaos",
            0,
            "596009e7568dfe2a0c41ee36a486afbdde47a727d52bf819aa0ea5004bddfbc2",
            212,
            2312014916,
        ),
        (
            "chaos",
            37,
            "a8b4d9a7bb518eb235868276f67c9cf27e45ea325ff8b1cbafc3674f00a82aae",
            212,
            3989898361,
        ),
        (
            "chaos",
            19,
            "a34f1d1577030aeee3d467d9626cf1a4e8fe402d6b86d5cc5ab5dee770a9e1cf",
            212,
            39465415,
        ),
        (
            "chaos",
            12345,
            "e427e19d0c4d913cd5e56767f802c361c06025b55cb8e631baf3523d8831dfb7",
            212,
            3673730253,
        ),
        (
            "chaos",
            u64::from(u32::MAX),
            "2ccccfc7d5927976b4de4105021f9b632c7baaca7ded555bf8822c3f35b28405",
            212,
            4240030067,
        ),
        // Frozen source replacement draws make these initial cursors
        // seed dependent, while the complete setup remains deterministic.
        (
            "chaos",
            15,
            "9d6b3874001c53fe561f2d2beb588306fdd64b1e9980712d6a4f4a229ec21439",
            156,
            2140418635,
        ),
        (
            "chaos",
            54,
            "9b801d531d28a0e1e012bd5362cafe902ba4f6ae2ed5064a8fbd3a73c28bee75",
            242,
            113116040,
        ),
        (
            "grand",
            0,
            "d7c34c6ae162acdb5c64eea1522ed5e5aabe8da175426fe537ba5a7e1c185e10",
            112,
            3925529136,
        ),
        (
            "grand",
            37,
            "7243871ce98ae31a6666c0c6e9748c3d43184d527cf626b5f2e992472ba0b300",
            112,
            3695423253,
        ),
        (
            "grand",
            19,
            "3b80a064d65eac0c6411766b0bd1acdd12d5c59539bcf31bf0e958ac16e92e57",
            112,
            1021441923,
        ),
        (
            "grand",
            12345,
            "d653c1c1ef9a1421a61ce0160f7f6e0475ff2235ce3fbdcb829cec063f86a7f0",
            112,
            3880022569,
        ),
        (
            "grand",
            u64::from(u32::MAX),
            "d8bec552aecdd8630c5806f8c279d2dffe397d67b4a2064cdc3ebb47eca6fc20",
            112,
            913663087,
        ),
    ];
    let mut differences = Vec::new();
    for (style, seed, digest, cursor, rng_state) in cases {
        let state = new_game(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            seed,
        )
        .unwrap();
        if state.mode != "draft"
            || !state.history.is_empty()
            || state.rng.cursor != cursor
            || state.rng.state != rng_state
            || source_state_digest(&state) != digest
        {
            differences.push(format!(
                "{style} seed {seed}: mode={}, history={}, RNG={}/{}, state={}",
                state.mode,
                state.history.len(),
                state.rng.cursor,
                state.rng.state,
                source_state_digest(&state),
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "source/new game drift:\n{}",
        differences.join("\n")
    );
}

#[test]
fn v7_initial_draft_keeps_empty_public_history() {
    for style in ["normal", "chaos", "grand"] {
        let state = new_game(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            19,
        )
        .unwrap();
        assert!(state.history.is_empty(), "{style} initial public history");
        assert_eq!(state.extra["replayEvents"], Value::Array(Vec::new()));
    }
}

#[test]
fn v7_new_game_rejects_invalid_setup_before_exposing_a_position() {
    for config in [
        GameConfig {
            rule_card_ids: vec!["saturation".into(), "saturation".into()],
            ..GameConfig::default()
        },
        GameConfig {
            rule_card_ids: vec!["not-a-rule".into()],
            ..GameConfig::default()
        },
        GameConfig {
            rule_card_ids: vec!["revelation".into(), "saturation".into()],
            deathmatch_enabled: false,
            ..GameConfig::default()
        },
    ] {
        assert!(matches!(
            new_game(config, 37),
            Err(EngineError::InvalidConfig(_))
        ));
    }
    assert!(matches!(
        new_game(GameConfig::default(), u64::from(u32::MAX) + 1),
        Err(EngineError::InvalidConfig(_))
    ));
    for config in [
        GameConfig {
            star_win_limit: 0,
            ..GameConfig::default()
        },
        GameConfig {
            deathmatch_limit_turns: 0,
            ..GameConfig::default()
        },
        GameConfig {
            game_style: "unknown".into(),
            ..GameConfig::default()
        },
    ] {
        assert!(matches!(
            new_game(config, 37),
            Err(EngineError::InvalidConfig(_))
        ));
    }
}

#[test]
fn v7_revelation_opening_keeps_pre_rule_replay_limit() {
    // The source records both initial replay frames at the configured limit,
    // then the RULE effect changes the live value. This case is independently
    // pinned by the 324-case source matrix at seed zero.
    let state = new_game(
        GameConfig {
            rule_card_ids: vec!["revelation".into()],
            ..GameConfig::default()
        },
        0,
    )
    .unwrap();
    assert_eq!(state.extra["deathmatchLimitTurns"], 5);
    assert_eq!(state.extra["replayBaseFrame"]["deathmatchLimitTurns"], 10);
    assert_eq!(state.extra["replayTailFrame"]["deathmatchLimitTurns"], 10);
    assert_eq!(state.rng.cursor, 125);
    assert_eq!(state.rng.state, 1_514_614_395);
    assert_eq!(
        source_state_digest(&state),
        "5795b10c83e5b0fd67400aa1c6d7d1e1ae87c41bb2edab364dfbb43fda116e1b"
    );
}

#[test]
fn v7_draft_delete_and_limit_settings_match_source_setup() {
    // Source newGame replays reset-time configuration before the initial
    // frame. These cases cover both the no-draft path and nondefault limits.
    let draft_delete = [
        (
            "normal",
            0,
            "be475b46dc81a4bdb00b5b227a78ec58bf0491d93792650459180129cc81a9dc",
        ),
        (
            "normal",
            37,
            "d4c7e707a95a4faecca5b4187993fcb20d03b9d93ff2956b3b559eff65f05b55",
        ),
        (
            "chaos",
            0,
            "f835b73873f3af6570aba5bcb6eec940962ffd5e41cb3a85f3f572ea86f99bdb",
        ),
        (
            "chaos",
            37,
            "aacb756b91ae3ef6d751440b6d30ce002be0b25d3131cd6b2a530427f3141423",
        ),
        (
            "grand",
            0,
            "b7401d82b618b37cb9f92e8707378b391341ae3a54b57fb02accc21466356b34",
        ),
        (
            "grand",
            37,
            "3122701a564e588f1714e2688bb93381c68a50456c393a3c9bbaa9b0546e2caf",
        ),
    ];
    let mut differences = Vec::new();
    for (style, seed, digest) in draft_delete {
        let state = new_game(
            GameConfig {
                game_style: style.into(),
                draft_delete: true,
                ..GameConfig::default()
            },
            seed,
        )
        .unwrap();
        if state.mode != "play"
            || !state.history.is_empty()
            || state.rng.cursor != 32
            || source_state_digest(&state) != digest
        {
            differences.push(format!(
                "{style} draftDelete seed {seed}: RNG={}/{}, state={}",
                state.rng.cursor,
                state.rng.state,
                source_state_digest(&state)
            ));
        }
    }
    for (seed, digest) in [
        (
            0,
            "1b9bdd6cc6901e4ed9ea81846d3789ea4329543cb56dac8c04d461938a817c28",
        ),
        (
            37,
            "b2adb76e1f451b7d651528deb9272dbc9e309632f2c3d78b115c916b3046c88f",
        ),
    ] {
        let state = new_game(
            GameConfig {
                star_win_limit: 7,
                deathmatch_limit_turns: 3,
                deathmatch_enabled: false,
                ..GameConfig::default()
            },
            seed,
        )
        .unwrap();
        if state.mode != "draft"
            || !state.history.is_empty()
            || state.rng.cursor != 122
            || source_state_digest(&state) != digest
        {
            differences.push(format!(
                "normal custom limits seed {seed}: RNG={}/{}, state={}",
                state.rng.cursor,
                state.rng.state,
                source_state_digest(&state)
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "source/setup drift:\n{}",
        differences.join("\n")
    );
}

#[test]
fn v7_combined_settings_and_opening_rule_selection_match_source() {
    let custom = |style: &str, draft_delete| GameConfig {
        game_style: style.into(),
        draft_delete,
        star_win_limit: 7,
        deathmatch_enabled: false,
        deathmatch_limit_turns: 3,
        ..GameConfig::default()
    };
    let rule = |style: &str, ids: &[&str]| GameConfig {
        game_style: style.into(),
        rule_card_ids: ids.iter().map(|id| (*id).into()).collect(),
        ..GameConfig::default()
    };
    let cases = [
        (
            custom("normal", true),
            0,
            "eadb09de63ace7af7c971acdbee15059b7c11f7a75965d6b40a1f704eca7a34e",
            32,
            95141024_u32,
            "play",
            None,
        ),
        (
            custom("chaos", true),
            37,
            "446f0403b3b621a006faa92bb166d20963a03525b0b88867e7153c37102321d0",
            32,
            3271378757,
            "play",
            None,
        ),
        (
            custom("grand", true),
            19,
            "d0f113026fb37286e72e0c15a8836d0a6be8b80cb4211ea01a290e1909139205",
            32,
            4163866163,
            "play",
            None,
        ),
        (
            custom("chaos", false),
            19,
            "a2c516b2265dd3e1126ef035a87aa90a9286fc17c1ee755148973df0bc27c384",
            212,
            39465415,
            "draft",
            None,
        ),
        (
            custom("grand", false),
            37,
            "f1113c8880ace4b9754bf8f4e84d0dfd18b4c923e6ac83fc4efe5085c6ef9e9c",
            112,
            3695423253,
            "draft",
            None,
        ),
        (
            rule("normal", &["saturation"]),
            19,
            "996246cd3dea5d4c7f54f0d6c1de270f7dd0907b85289e8e97415d2c92028bdd",
            125,
            3595483778,
            "draft",
            Some("saturation"),
        ),
        (
            rule("chaos", &["saturation"]),
            37,
            "5880ccc499f050fee78db22da0ae832d64a670a15c1bc4436c94b3cf78a796f7",
            215,
            1679969110,
            "draft",
            Some("saturation"),
        ),
        (
            rule("grand", &["saturation"]),
            0,
            "48f1333fd979945502331280caf6a8e08cd520666c6dd5104db8d86d07d4665e",
            115,
            2360290521,
            "draft",
            Some("saturation"),
        ),
        (
            rule("normal", &["saturation", "acceleration"]),
            37,
            "2c1385cec38d3c5f156a4ac169c939fd257fd1f2e3a66f9cdf0b4ad1a0dca1a2",
            125,
            2854222796,
            "draft",
            Some("acceleration"),
        ),
    ];
    let mut differences = Vec::new();
    for (config, seed, digest, cursor, rng_state, mode, applied_rule) in cases {
        let style = config.game_style.clone();
        let state = new_game(config, seed).unwrap();
        if state.mode != mode
            || !state.history.is_empty()
            || state.rng.cursor != cursor
            || state.rng.state != rng_state
            || state
                .extra
                .get("appliedRuleCard")
                .and_then(|card| card.get("id"))
                .and_then(Value::as_str)
                != applied_rule
            || source_state_digest(&state) != digest
        {
            differences.push(format!(
                "{style} seed {seed}: mode={}, RNG={}/{}, state={}",
                state.mode,
                state.rng.cursor,
                state.rng.state,
                source_state_digest(&state)
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "source/combined setup drift:\n{}",
        differences.join("\n")
    );
}

#[test]
fn v7_rule_pools_and_custom_settings_match_frozen_source_positions() {
    // Direct GameAdapter.newGame against the SHA-pinned v7 client. The pool
    // order and seed select one installed RULE; these exact state, position,
    // RNG and history checks cover non-matrix settings without storing source
    // snapshots in Git.
    let rule = |style: &str, ids: &[&str]| GameConfig {
        game_style: style.into(),
        rule_card_ids: ids.iter().map(|id| (*id).into()).collect(),
        ..GameConfig::default()
    };
    let cases = [
        (
            GameConfig {
                draft_delete: true,
                ..rule("chaos", &["saturation", "acceleration"])
            },
            19,
            "eb57522bc29085ed29cb5c2e7b81164b38795caaf8bfe838f0c9854224f44dec",
            "874b1a725f6ff04a0806a5a0ec0c07410478a27896d8bc1bf3075d98bbec0835",
            35,
            2_324_936_856_u32,
            "acceleration",
            "play",
        ),
        (
            GameConfig {
                star_win_limit: 7,
                deathmatch_limit_turns: 3,
                ..rule("grand", &["revelation", "saturation", "acceleration"])
            },
            12_345,
            "a5956a89bb3621dbd765889ebff7dc8a3c9f1eff4adb38045783a08ece064966",
            "b25d00d91dfeb061c7f0e0683708819de6b100d443906b6a1e2db68446484416",
            115,
            2_915_697_350,
            "revelation",
            "draft",
        ),
        (
            GameConfig {
                draft_delete: true,
                star_win_limit: 7,
                deathmatch_enabled: false,
                deathmatch_limit_turns: 3,
                ..rule("normal", &["saturation"])
            },
            u64::from(u32::MAX),
            "89fb7c5988608b37f1729d526d44b42c1774dbf7610670fad457503b756936c7",
            "b830be671b10a788b394707e1e07e11526c2a4a8891884966770b720714f6769",
            35,
            2_550_510_324,
            "saturation",
            "play",
        ),
        (
            rule("normal", &["black-hole", "portal", "camouflage-color"]),
            0,
            "5ee4f6ee0741aa2307541ce7fe74ba3c6eec5265147b692a7ea37cd806b6852d",
            "e9a6b2ef720ef7e261f6aee499a7b96f948f8b88075eb87070a42001dbdbaad7",
            125,
            1_514_614_395,
            "portal",
            "draft",
        ),
        (
            GameConfig {
                deathmatch_enabled: false,
                ..rule("chaos", &["black-hole", "portal", "camouflage-color"])
            },
            37,
            "3487ade8e64a5c1959dace202229ddc2ca0058df3d38180ff5dedecabbb8d862",
            "3e189982166a93b1de9c75a74c9e660a2de4830ad1d5fbbf38536cd3440d05b6",
            215,
            1_679_969_110,
            "camouflage-color",
            "draft",
        ),
        (
            GameConfig {
                draft_delete: true,
                ..rule("grand", &["black-hole", "portal", "camouflage-color"])
            },
            19,
            "a4b16c3e1e4f9d5c6ad4c5b099398fa3434e325432342ead3114878721ec2c0c",
            "9b51f2f0d76c591f280f0197ada4bedfde6c18803e25bd8d90fe6e9a3994b1e7",
            35,
            2_324_936_856,
            "camouflage-color",
            "play",
        ),
        (
            GameConfig {
                star_win_limit: 1,
                deathmatch_limit_turns: 1,
                ..rule("normal", &["rule-bombs", "recycling", "platform"])
            },
            u64::from(u32::MAX),
            "1865345e2e9086036954e9e48fa5554743c39c5c781fb9141fbc7d97efc272ef",
            "20fc9d827a677e334ac9f85c0a001e827d865145be9cf795df99636d649ddb2b",
            126,
            1_066_809_349,
            "platform",
            "draft",
        ),
    ];
    for (config, seed, digest, position_id, cursor, rng_state, selected, mode) in cases {
        let state = new_game(config, seed).expect("source-supported opening configuration");
        assert_eq!(state.mode, mode, "selected {selected}");
        assert_eq!(state.history.len(), 0, "selected {selected}");
        assert_eq!(state.rng.cursor, cursor, "selected {selected}");
        assert_eq!(state.rng.state, rng_state, "selected {selected}");
        assert_eq!(state.extra["appliedRuleCard"]["id"], selected);
        assert_eq!(source_state_digest(&state), digest, "selected {selected}");
        let host = crate::V7HostPosition::from_state(state).unwrap();
        assert_eq!(host.position_id(), position_id, "selected {selected}");
    }
}

#[test]
#[ignore = "requires the faithful175 source324 manifest, its observed SHA and complete Position files"]
fn v7_all_single_rule_openings_match_pinned_source_matrix_when_supplied() {
    // 원문 초기화와 native 결과를 분리한다. 새 원문 manifest의 실제 바이트 SHA를
    // 메인이 함께 지정해야 하며, 과거 324개 요약은 faithful175 증거가 아니다.
    const LEGACY_SHA: &str = "41dbb57b3523a3f27d5ce1f757b8e0d33eb5a1f1b11b8168b3eeffb2a36a7f09";
    const INPUTS_SHA: &str = "8c9a8d4de4f976274d5bc9eb7c511e2de0773685b8304731f7945ab60a081377";
    let digest = |value: &Value| {
        format!(
            "{:x}",
            Sha256::digest(serde_jcs::to_vec(value).expect("source JCS data"))
        )
    };
    let path = std::path::PathBuf::from(
        std::env::var_os("ACCELERATE_V7_RULE_OPENING_SOURCE_MANIFEST").expect(
            "set ACCELERATE_V7_RULE_OPENING_SOURCE_MANIFEST to the faithful175 source matrix",
        ),
    );
    let bytes = std::fs::read(&path).expect("read faithful175 source RULE matrix");
    assert!(
        bytes.len() <= 8 * 1024 * 1024,
        "source manifest exceeds the JSON byte boundary"
    );
    let actual_sha = format!("{:x}", Sha256::digest(&bytes));
    assert_ne!(
        actual_sha, LEGACY_SHA,
        "the historical declaration-profile matrix cannot validate faithful175; generate the new source324 receipt"
    );
    let expected_sha = std::env::var("ACCELERATE_V7_RULE_OPENING_SOURCE_MANIFEST_SHA256")
        .expect("set the manifest SHA256 from the new source generator output; do not derive it from native results");
    assert!(
        expected_sha.len() == 64
            && expected_sha
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "source manifest SHA must be lowercase hex"
    );
    assert_eq!(
        actual_sha, expected_sha,
        "source RULE matrix bytes differ from the observed generator SHA"
    );
    let matrix: Value = serde_json::from_slice(&bytes).expect("parse faithful175 source matrix");
    let profile: Value = serde_json::from_str(include_str!(
        "../../contracts/catalog/execution-profile-20260928.json"
    ))
    .expect("compiled execution profile");
    let catalog: Value =
        serde_json::from_str(include_str!("../../contracts/catalog/site-20260928.json"))
            .expect("compiled execution catalog");
    assert_eq!(
        matrix["schemaVersion"], 2,
        "full Position receipts require schema2"
    );
    assert_eq!(matrix["kind"], "source-rule-opening-matrix");
    assert_eq!(matrix["contractBaseline"], "site-20260928");
    assert_eq!(matrix["sourceClientSha256"], profile["sourceMainSha256"]);
    assert_eq!(matrix["rulesVersion"], profile["rulesVersion"]);
    assert_eq!(
        matrix["sourcePublicCatalogHash"],
        profile["sourcePublicCatalogHash"]
    );
    assert_eq!(
        matrix["catalogVersion"],
        crate::v7_execution_profile::catalog_version().unwrap()
    );
    assert_eq!(
        matrix["legacyMatrixSha256"], LEGACY_SHA,
        "keep the original source324 input authority"
    );
    assert_eq!(matrix["inputsSha256"], INPUTS_SHA);
    assert_eq!(
        matrix["executionProfile"]["version"],
        profile["profileVersion"]
    );
    assert_eq!(
        matrix["executionProfile"]["version"],
        "accelerate-headless-semantic-v7-faithful-init-v1"
    );
    assert_eq!(matrix["executionProfile"]["sha256"], digest(&profile));
    assert_eq!(
        matrix["executionProfile"]["parserSha256"],
        profile["parserSha256"]
    );
    assert_eq!(matrix["executionProfile"]["initializersCount"], 175);
    assert_eq!(matrix["executionProfile"]["excludedInitializersCount"], 168);
    assert_eq!(profile["initializers"].as_array().unwrap().len(), 175);
    assert_eq!(
        profile["excludedInitializers"].as_array().unwrap().len(),
        168
    );
    for key in [
        "initializersSha256",
        "excludedInitializersSha256",
        "replayMetadataSha256",
    ] {
        assert_eq!(
            matrix["executionProfile"][key], profile[key],
            "{key} source authority"
        );
    }
    assert_eq!(
        matrix["executionProfile"]["bootstrapSha256"],
        profile["bootstrap"]["scriptSha256"]
    );
    assert_eq!(matrix["producer"]["kind"], "source-newGame-full-position");
    assert_eq!(
        matrix["producer"]["recipe"],
        "collect-rule-opening-matrix-faithful175.cjs"
    );
    let recipe_sha = matrix["producer"]["sha256"]
        .as_str()
        .expect("source recipe SHA");
    assert!(
        recipe_sha.len() == 64
            && recipe_sha
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "source recipe SHA must be lowercase hex"
    );
    assert_eq!(
        matrix["parameters"]["styles"],
        serde_json::json!(["normal", "chaos", "grand"])
    );
    assert_eq!(matrix["parameters"]["seeds"], serde_json::json!([0, 37]));
    assert_eq!(
        matrix["parameters"]["draftDelete"],
        serde_json::json!([false, true])
    );
    assert_eq!(matrix["parameters"]["ruleCount"], 27);
    assert_eq!(matrix["parameters"]["planned"], 324);
    assert!(
        matrix["parameters"]["maxElapsedMs"]
            .as_u64()
            .is_some_and(|limit| (1..=600000).contains(&limit))
    );
    assert_eq!(matrix["counts"]["completed"], 324);
    assert_eq!(matrix["counts"]["success"], 324);
    assert_eq!(matrix["counts"]["error"], 0);
    assert_eq!(matrix["counts"]["unrun"], 0);
    assert_eq!(matrix["stoppedByDeadline"], false);
    let cases = matrix["cases"].as_array().expect("source RULE cases");
    assert_eq!(cases.len(), 324);
    let inputs = Value::Array(
        cases
            .iter()
            .map(|case| {
                serde_json::json!({
                    "ruleId": case["ruleId"], "gameStyle": case["gameStyle"],
                    "seed": case["seed"], "draftDelete": case["draftDelete"],
                })
            })
            .collect(),
    );
    assert_eq!(
        digest(&inputs),
        INPUTS_SHA,
        "preserve every legacy source input and its order"
    );
    let expected_rules: BTreeSet<&str> = catalog["cards"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|card| card["draftCategory"] == "RULE")
        .map(|card| card["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        expected_rules.len(),
        27,
        "compiled source RULE category changed"
    );
    let declared_rules: BTreeSet<&str> = matrix["parameters"]["ruleIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rule| rule.as_str().unwrap())
        .collect();
    assert_eq!(
        declared_rules, expected_rules,
        "manifest RULE set differs from source catalog"
    );
    assert_eq!(
        matrix["parameters"]["ruleIds"].as_array().unwrap().len(),
        27
    );
    let report_root = path
        .parent()
        .expect("source matrix parent")
        .canonicalize()
        .expect("canonical source report root");
    let mut seen = BTreeSet::new();
    let mut rules = BTreeSet::new();
    let mut differences = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let rule = case["ruleId"].as_str().expect("RULE ID");
        let style = case["gameStyle"].as_str().expect("game style");
        let seed = case["seed"].as_u64().expect("uint32 seed");
        let draft_delete = case["draftDelete"].as_bool().expect("draft setting");
        let identity = format!("{rule}/{style}/{seed}/{draft_delete}");
        assert!(expected_rules.contains(rule), "unknown RULE in {identity}");
        assert!(matches!(style, "normal" | "chaos" | "grand"));
        assert!(matches!(seed, 0 | 37));
        assert_eq!(
            case["status"], "success",
            "{identity} source error: {:?}",
            case["error"]
        );
        assert!(
            seen.insert((rule, style, seed, draft_delete)),
            "duplicate RULE input"
        );
        rules.insert(rule);
        assert_eq!(
            case["config"],
            serde_json::json!({
                "gameStyle": style, "draftDelete": draft_delete, "ruleCardIds": [rule],
            }),
            "{identity} source configuration drift"
        );
        let position_file =
            format!("positions/{index:03}-{rule}-{style}-{seed}-{draft_delete}.json");
        assert_eq!(
            case["positionFile"], position_file,
            "{identity} source Position path"
        );
        let position_path = report_root
            .join(position_file)
            .canonicalize()
            .expect("canonical source Position path");
        assert!(
            position_path.starts_with(&report_root),
            "{identity} source Position escapes report root"
        );
        let position_bytes = std::fs::read(position_path).expect("read complete source Position");
        assert!(
            position_bytes.len() <= 8 * 1024 * 1024,
            "{identity} source Position exceeds JSON byte boundary"
        );
        assert_eq!(
            case["positionFileSha256"],
            format!("{:x}", Sha256::digest(&position_bytes)),
            "{identity} source Position bytes changed"
        );
        let source_position: Value =
            serde_json::from_slice(&position_bytes).expect("source Position JSON");
        crate::V7HostPosition::from_envelope(source_position.clone())
            .unwrap_or_else(|error| panic!("{identity} source Position contract: {error}"));
        assert_eq!(
            case["positionId"], source_position["positionId"],
            "{identity} source positionId metadata"
        );
        for (section, hash_key) in [
            ("state", "stateJcsSha256"),
            ("rng", "rngJcsSha256"),
            ("history", "historyJcsSha256"),
        ] {
            assert_eq!(
                case[hash_key],
                digest(&source_position[section]),
                "{identity} source {section} metadata"
            );
        }
        assert_eq!(
            case["positionJcsSha256"],
            digest(&source_position),
            "{identity} source envelope metadata"
        );
        assert_eq!(case["rngCursor"], source_position["rng"]["cursor"]);
        assert_eq!(case["rngState"], source_position["rng"]["state"]);
        assert_eq!(
            case["historyCount"].as_u64(),
            Some(source_position["history"].as_array().unwrap().len() as u64)
        );
        assert_eq!(case["mode"], source_position["state"]["mode"]);
        assert_eq!(
            case["appliedRuleId"],
            source_position["state"]["appliedRuleCard"]["id"]
        );
        let state = match new_game(
            GameConfig {
                game_style: style.into(),
                draft_delete,
                rule_card_ids: vec![rule.into()],
                ..GameConfig::default()
            },
            seed,
        ) {
            Ok(state) => state,
            Err(error) => {
                differences.push(format!("{identity}: native newGame: {error}"));
                continue;
            }
        };
        let actual_position = match crate::V7HostPosition::from_state(state)
            .and_then(|position| position.export_envelope())
        {
            Ok(position) => position,
            Err(error) => {
                differences.push(format!("{identity}: native Position: {error}"));
                continue;
            }
        };
        // JCS compares every state, RNG/tape, history entry and version field.
        // Separate section digests make the exact divergence reviewable.
        if digest(&actual_position) != digest(&source_position) {
            let sections: Vec<_> = [
                "protocolVersion",
                "rulesVersion",
                "catalogVersion",
                "state",
                "rng",
                "history",
                "positionId",
            ]
            .into_iter()
            .filter(|section| {
                digest(&actual_position[*section]) != digest(&source_position[*section])
            })
            .collect();
            let mut state_keys = BTreeSet::new();
            state_keys.extend(actual_position["state"].as_object().unwrap().keys());
            state_keys.extend(source_position["state"].as_object().unwrap().keys());
            let changed_state: Vec<_> = state_keys
                .into_iter()
                .filter(|key| {
                    let actual = actual_position["state"].get(key.as_str());
                    let expected = source_position["state"].get(key.as_str());
                    match (actual, expected) {
                        (Some(actual), Some(expected)) => digest(actual) != digest(expected),
                        _ => actual.is_some() != expected.is_some(),
                    }
                })
                .collect();
            differences.push(format!(
                "{identity}: sections={sections:?}, stateFields={changed_state:?}, state={}/{}, RNG={}/{}, history={}/{}, position={}/{}",
                digest(&actual_position["state"]), case["stateJcsSha256"],
                digest(&actual_position["rng"]), case["rngJcsSha256"],
                digest(&actual_position["history"]), case["historyJcsSha256"],
                actual_position["positionId"], case["positionId"],
            ));
        }
    }
    assert_eq!(
        rules, expected_rules,
        "source matrix must cover every pinned RULE"
    );
    assert_eq!(
        seen.len(),
        324,
        "source matrix must cover every combination"
    );
    assert!(
        differences.is_empty(),
        "faithful175 source/RULE drift ({} cases):\n{}",
        differences.len(),
        differences.join("\n")
    );
}
