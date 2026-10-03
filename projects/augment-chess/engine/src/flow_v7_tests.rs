use super::*;

#[test]
fn source_v7_normal_and_chaos_milestones_match_full_state_and_rng() {
    use sha2::{Digest, Sha256};
    let digest = |state: &GameState| {
        let mut value = serde_json::to_value(state).unwrap();
        for key in ["rulesetId", "rng", "history"] {
            value.as_object_mut().unwrap().remove(key);
        }
        format!("{:x}", Sha256::digest(serde_jcs::to_vec(&value).unwrap()))
    };
    // SHA-pinned main-OahWs0tU.js with reviewed top-level initializers:
    // seed 37 first-play states with a synthetic completed-turn count.
    // These are direct maybeStartMilestoneDraft probes,
    // not evidence that the intervening 10/20 natural turns are supported.
    for (style, phase, turns, middle_done, expected_digest, cursor, rng_state) in [
        (
            "normal",
            "MIDDLE",
            10,
            false,
            "4b4c63a0272cfbebb94347b17870cde5158bf333c43dab7203fd64bd3c39f6b1",
            278,
            4_230_071_635,
        ),
        (
            "normal",
            "END",
            20,
            true,
            "0c715fe0b6a28ece095c7e1da71e0bba0f99b8852e37513aa9b5d1ed4debd6ce",
            306,
            3_997_385_423,
        ),
        (
            "chaos",
            "MIDDLE",
            10,
            false,
            "51258db3f7940e2ef7d37693ae6ee2315ffa8878858d4c44c4a98dbac9f9516f",
            526,
            3_751_996_747,
        ),
        (
            "chaos",
            "END",
            20,
            true,
            "1267d6e7ff9c2610e7440551bee3e69b2df8638c6415688a9e66137b25b0f77e",
            582,
            768_950_659,
        ),
    ] {
        let mut opening = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            37,
            RULES_VERSION_V7,
        )
        .unwrap();
        for _ in 0..2 {
            let action = crate::draft::legal_actions(&opening).unwrap().remove(0);
            crate::transition::apply(&mut opening, &action).unwrap();
            crate::replay::canonicalize_position_frames(&mut opening).unwrap();
        }
        assert_eq!(opening.mode, "play");
        let mut state = opening;
        state.turns_taken = Sides::new(turns, turns);
        state.move_count = turns * 2;
        state.full_move = turns + 1;
        state
            .extra
            .insert("middleDraftDone".into(), json!(middle_done));
        state.extra.insert("endDraftDone".into(), json!(false));
        if style == "normal" && phase == "MIDDLE" {
            // Direct checkRepetitionOrStarLimit on this same seed-37 opening
            // state, with only the completed-turn count and limit settings
            // injected, matches the frozen v7 client over the whole state.
            // Regenerated with faithful-init-v1: startDeathmatch stores the
            // normalized 1.6 -> 2 limit before deriving its four-half-turn window.
            let mut overtime = state.clone();
            overtime.extra.insert("starWinLimit".into(), json!(10));
            overtime
                .extra
                .insert("deathmatchEnabled".into(), json!(true));
            overtime
                .extra
                .insert("deathmatchLimitTurns".into(), json!(1.6));
            assert!(!check_termination(&mut overtime).unwrap());
            assert_eq!(
                digest(&overtime),
                "4dca89eea814762a618b5170917a38daf58196ee268379b47aaabc717ed3c9c9",
                "normal MIDDLE overtime entry full state"
            );
            assert_eq!(overtime.rng.cursor, 216);
            assert_eq!(overtime.rng.state, 1_469_516_477);
        }
        assert!(
            maybe_start_milestone_draft(&mut state).unwrap(),
            "{style} {phase}"
        );
        assert_eq!(state.mode, "draft");
        assert_eq!(state.turn, Color::White);
        assert_eq!(state.extra["draft"]["phase"], phase);
        assert_eq!(
            digest(&state),
            expected_digest,
            "{style} {phase} full state"
        );
        assert_eq!(state.rng.cursor, cursor, "{style} {phase} RNG cursor");
        assert_eq!(state.rng.state, rng_state, "{style} {phase} RNG state");
    }
}

#[test]
fn v7_milestone_skips_source_exclusions_and_returns_due_judgment_exile() {
    let mut state = play_state(RULES_VERSION_V7);
    state.turns_taken = Sides::new(10, 10);
    state.extra.insert("draftDelete".into(), json!(false));
    state.extra.insert("middleDraftDone".into(), json!(false));
    state.extra.insert("endDraftDone".into(), json!(false));
    state.extra.insert("gameStyle".into(), json!("normal"));
    let mut below_threshold = state.clone();
    below_threshold.turns_taken.black = 9;
    let before = below_threshold.clone();
    assert!(!maybe_start_milestone_draft(&mut below_threshold).unwrap());
    assert_eq!(below_threshold, before);

    let mut grand = state.clone();
    grand.extra.insert("gameStyle".into(), json!("grand"));
    let before = grand.clone();
    assert!(!maybe_start_milestone_draft(&mut grand).unwrap());
    assert_eq!(grand, before);

    let mut forced = state.clone();
    place_king(&mut forced, Color::White, 7, 4);
    forced.board[7][4]
        .as_mut()
        .unwrap()
        .extra
        .insert("thiefSecondMove".into(), json!(true));
    let before = forced.clone();
    assert!(!maybe_start_milestone_draft(&mut forced).unwrap());
    assert_eq!(forced, before);

    // main67022는 Judgment 복귀를 clock pause보다 먼저 수행한다. 이 helper는
    // fresh faithful 반환 10개와 MIDDLE/END 기본 milestone 4개로 검증됐다.
    // v6 state를 v7으로 표시한 축약 fixture 대신 실제 v7 초기 상태를 사용한다.
    let mut returning = crate::draft::initialize_for_ruleset(
        GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        },
        17,
        RULES_VERSION_V7,
    )
    .unwrap();
    returning.turns_taken = Sides::new(10, 10);
    returning.move_count = 20;
    returning.full_move = 11;
    returning.extra.insert("draftDelete".into(), json!(false));
    returning
        .extra
        .insert("middleDraftDone".into(), json!(false));
    returning.extra.insert("endDraftDone".into(), json!(false));
    returning.extra.insert("repetitionSalt".into(), json!(11));
    returning.extra.insert(
        "positionCounts".into(),
        json!({"__simType":"Map","entries":[["milestone-before",3]]}),
    );
    let exile = Piece::new("pawn", Color::White, "milestone-return-pawn");
    returning.extra.insert(
        "judgmentExiles".into(),
        json!([{"id":"milestone-judgment-MIDDLE","returnPhase":"MIDDLE","piece":exile,
            "exiledBy":"black","from":{"row":3,"col":3}}]),
    );
    let before = returning.clone();
    assert!(maybe_start_milestone_draft(&mut returning).unwrap());
    assert_eq!(returning.mode, "draft");
    assert_eq!(returning.extra["draft"]["phase"], json!("MIDDLE"));
    assert_eq!(returning.turn, Color::White);
    assert_eq!(returning.extra["draftResumeTurn"], json!(before.turn));
    assert_eq!(returning.extra["judgmentExiles"], json!([]));
    let returned = returning
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|piece| piece.id == exile.id)
        .collect::<Vec<_>>();
    assert_eq!(
        returned.len(),
        1,
        "the due exile identity must return exactly once"
    );
    assert_eq!(
        serde_jcs::to_vec(returned[0]).unwrap(),
        serde_jcs::to_vec(&exile).unwrap(),
        "Judgment return must preserve every piece field"
    );
    assert_eq!(returning.extra["repetitionSalt"], json!(12));
    assert_eq!(
        returning.extra["positionCounts"],
        json!({"__simType":"Map","entries":[]})
    );
    assert_eq!(returning.history, before.history);
    assert_eq!(returning.captures, before.captures);
    assert_eq!(returning.turns_taken, before.turns_taken);
    assert_eq!(returning.move_count, before.move_count);
    assert_eq!(returning.full_move, before.full_move);
    assert!(
        returning.rng.cursor > before.rng.cursor,
        "milestone draft drawing must consume RNG"
    );
}

/// Judgment 반환 helper만의 raw receipt와 별개로 실제 milestone -> startDraft
/// 접합을 대조한다. 합성 완료-turn 입력이며 자연적인 10/20턴 전이를 증명하지 않는다.
#[test]
#[ignore = "메인이 생성한 faithful milestone+Judgment 전체 Position 영수증을 요구한다"]
fn frozen_milestone_judgment_returns_match_full_positions() {
    use crate::tests::source_callback_fixture::compare_value;
    use std::collections::BTreeSet;
    let path = std::env::var_os("ACCELERATE_V7_MILESTONE_RETURN_CASES")
        .expect("main must provide fresh source milestone+Judgment receipts");
    let path = std::path::PathBuf::from(path);
    assert!(
        std::fs::metadata(&path).unwrap().len() <= 16 * 1024 * 1024,
        "milestone return receipt exceeds the comparison budget"
    );
    let receipt: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(receipt["schemaVersion"], json!(1));
    assert_eq!(receipt["status"], "source-generated");
    assert_eq!(receipt["nativeCompared"], json!(false));
    assert_eq!(
        receipt["sourceSha256"],
        "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
    );
    assert_eq!(
        receipt["executionProfile"]["profileVersion"],
        "accelerate-headless-semantic-v7-faithful-init-v1"
    );
    assert_eq!(
        receipt["executionProfile"]["selectedInitializerCount"],
        json!(175)
    );
    assert_eq!(
        receipt["executionProfile"]["excludedInitializerCount"],
        json!(168)
    );
    assert_eq!(receipt["rulesVersion"], RULES_VERSION_V7);
    assert_eq!(
        receipt["catalogVersion"],
        crate::v7_execution_profile::catalog_version().unwrap()
    );
    assert_eq!(
        receipt["sourceBoundary"],
        "restored-synthetic-before-position-milestone"
    );
    assert_eq!(receipt["semanticFieldsExcluded"], json!([]));
    let rows = receipt["rows"].as_array().unwrap();
    let names = BTreeSet::from([
        "normal-middle-judgment-return",
        "normal-end-judgment-carryover",
    ]);
    assert_eq!(
        rows.len(),
        names.len(),
        "both MIDDLE and END Judgment milestone receipts are required"
    );
    assert_eq!(
        rows.iter()
            .map(|row| row["name"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        names
    );
    let mut mismatches = Vec::new();
    for row in rows {
        let name = row["name"].as_str().unwrap();
        assert_eq!(row["callback"], "maybeStartMilestoneDraft");
        assert_eq!(
            row["callbackReturned"], "undefined",
            "{name}: source callback returns no boolean"
        );
        assert_eq!(
            row["checkpoint"],
            "restored-synthetic-before-position-milestone-after-snapshot-microtask-settlement"
        );
        assert_eq!(row["seed"], json!(37));
        assert_eq!(row["openingPicks"], json!(2));
        assert_eq!(
            serde_jcs::to_vec(&row["restoredBefore"]).unwrap(),
            serde_jcs::to_vec(&row["before"]).unwrap(),
            "{name}: actual source before restore must preserve the complete Position"
        );
        assert_eq!(
            row["referenceAfterPositionId"], row["after"]["positionId"],
            "{name}: independent source identity"
        );
        let host = crate::v7_host::V7HostPosition::from_envelope(row["before"].clone())
            .unwrap_or_else(|error| panic!("{name}: full source Position import failed: {error}"));
        let mut state = host.state().clone();
        let entered = maybe_start_milestone_draft(&mut state)
            .unwrap_or_else(|error| panic!("{name}: milestone+Judgment callback failed: {error}"));
        assert_eq!(
            json!(entered),
            row["sourceEntered"],
            "{name}: native milestone entry ownership flag"
        );
        crate::replay::settle(&mut state).unwrap_or_else(|error| {
            panic!("{name}: snapshot microtask settlement failed: {error}")
        });
        let actual = crate::v7_host::V7HostPosition::from_state(state)
            .and_then(|host| host.export_envelope())
            .unwrap_or_else(|error| panic!("{name}: full native Position export failed: {error}"));
        compare_value(&row["after"], &actual, name, &mut mismatches).unwrap_or_else(|error| {
            panic!("{name}: full milestone+Judgment comparison failed: {error}")
        });
    }
    assert!(
        mismatches.is_empty(),
        "source milestone+Judgment full Position differences:\n{}",
        mismatches.join("\n")
    );
}

fn play_state(rules_version: &str) -> GameState {
    let mut state = GameState::new(
        GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        },
        17,
    )
    .expect("local deterministic game");
    // The v7 Position execution guard remains closed. These focused tests
    // exercise only the source-identical flow kernel on an owned rule state.
    state.ruleset_id = rules_version.into();
    state.board = vec![vec![None; 8]; 8];
    state.extra.insert("logs".into(), json!([]));
    state.extra.insert(
        "positionCounts".into(),
        json!({"__simType":"Map","entries":[]}),
    );
    state
}

fn place_king(state: &mut GameState, color: Color, row: usize, col: usize) {
    state.board[row][col] = Some(Piece::new(
        "king",
        color,
        format!("{}-king", color.as_str()),
    ));
}

#[test]
fn relay_first_play_no_action_probe_uses_source_witness_after_card_trial() {
    // Source seed 19 normal: second white offer is Relay and the first black
    // offer enters play. Relay adds swap moves, but a2-a3 alone proves that
    // checkNoActionLoss must not declare a loss after the used card probe.
    let mut state =
        crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7).unwrap();
    let white_pick = crate::draft::legal_actions(&state).unwrap().remove(1);
    crate::draft::apply_pick(&mut state, &white_pick).unwrap();
    crate::replay::canonicalize_position_frames(&mut state).unwrap();
    let black_pick = crate::draft::legal_actions(&state).unwrap().remove(0);
    crate::transition::apply(&mut state, &black_pick).unwrap();
    assert_eq!(state.mode, "play");
    assert_eq!(state.turn, Color::White);
    let card = state.deck_slots.white[0].clone();
    assert_eq!(card.id, "relay");
    let action = crate::Action::card(Color::White, &card, None);
    // Position's v7 public-action gate remains closed. Construct only the
    // source-probed post-card state needed by this no-action flow check.
    crate::card_effects::apply(&mut state, &card, &action).unwrap();
    state.deck_slots.white[0].used = true;
    state.deck_slots.white[0].extra.insert(
        "usedAt".into(),
        json!(crate::draft::frozen_timestamp_for_ruleset(RULES_VERSION_V7).unwrap()),
    );
    state.cards_used_this_turn.white = 1;
    note_card_event(&mut state).unwrap();
    crate::replay::queue_card(&mut state, Color::White, &card).unwrap();
    crate::replay::record(&mut state, "card").unwrap();
    assert_eq!(state.rng.cursor, 293);
    assert_eq!(state.rng.state, 4_255_249_034);
    assert_eq!(state.extra["replayEventNonce"], 3);
    assert_eq!(state.extra["relay"], json!({"white":true,"black":false}));
    let before = state.clone();
    assert_eq!(
        crate::movement::v7_first_play_has_legal_move_witness(&state, Color::White).unwrap(),
        Some(true)
    );
    assert!(!check_no_action_loss(&mut state).unwrap());
    assert_eq!(state, before);
}

#[test]
fn end_game_uses_the_selected_frozen_client_time_and_keeps_v6_distinct() {
    let mut v6 = play_state(RULES_VERSION_V6);
    let mut v7 = play_state(RULES_VERSION_V7);
    end_game(&mut v6, Some(Color::White), "백 킹이 잡혔습니다.").unwrap();
    end_game(&mut v7, Some(Color::White), "백 킹이 잡혔습니다.").unwrap();
    assert_eq!(v6.extra["replayEndedAt"], "2026-09-27T14:37:10.842Z");
    assert_eq!(v7.extra["replayEndedAt"], "2026-09-28T07:41:32.828Z");
    assert_eq!(v7.extra["replayEndReason"], "백 킹이 잡혔습니다.");
    assert_eq!(v7.extra["logs"][0], "백 승리: 백 킹이 잡혔습니다.");
    assert_eq!(v7.mode, "gameover");
    assert_eq!(v7.winner.as_deref(), Some("white"));
    assert_eq!(v7.extra["legalMoves"], json!([]));
}

#[test]
fn active_clock_uses_its_own_ruleset_freeze_time() {
    let mut v6 = play_state(RULES_VERSION_V6);
    let mut v7 = play_state(RULES_VERSION_V7);
    for state in [&mut v6, &mut v7] {
        state.extra.insert(
            "clock".into(),
            json!({"enabled":true,"whiteMs":60000,"blackMs":60000,"runningColor":null,"lastStartedAt":null,"incrementMs":10000}),
        );
        start_clock(state).unwrap();
        assert_eq!(state.extra["clock"]["runningColor"], "white");
    }
    assert_eq!(
        v6.extra["clock"]["lastStartedAt"],
        json!(crate::draft::frozen_timestamp_for_ruleset(RULES_VERSION_V6).unwrap())
    );
    assert_eq!(
        v7.extra["clock"]["lastStartedAt"],
        json!(crate::draft::frozen_timestamp_for_ruleset(RULES_VERSION_V7).unwrap())
    );
    assert_ne!(
        v6.extra["clock"]["lastStartedAt"],
        v7.extra["clock"]["lastStartedAt"]
    );
}

#[test]
fn repetition_key_uses_derived_board_extent_and_one_piece_identity() {
    let mut state = play_state(RULES_VERSION_V7);
    state.board = vec![vec![None; 5]; 3];
    let large = Piece::new("colossus", Color::White, "large-1");
    state.board[2][3] = Some(large.clone());
    state.board[2][4] = Some(large);
    assert_eq!(record_position(&mut state).unwrap(), 1);
    assert_eq!(record_position(&mut state).unwrap(), 2);
    let entries = state.extra["positionCounts"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0][0], "white|0|2,3:white:colossus:::0");
}

#[test]
fn repetition_and_star_limit_write_the_exact_terminal_reason() {
    let mut repeated = play_state(RULES_VERSION_V7);
    place_king(&mut repeated, Color::White, 7, 7);
    place_king(&mut repeated, Color::Black, 0, 0);
    assert!(!check_termination(&mut repeated).unwrap());
    assert!(!check_termination(&mut repeated).unwrap());
    assert!(check_termination(&mut repeated).unwrap());
    assert_eq!(repeated.result(), Some(GameResult::Draw));
    assert_eq!(
        repeated.extra["replayEndReason"],
        "3회 동형반복: 별 합계가 같아 무승부입니다. (0 : 0)"
    );

    let mut limit = play_state(RULES_VERSION_V7);
    limit.turns_taken.white = 2;
    limit.turns_taken.black = 2;
    limit.extra.insert("starWinLimit".into(), json!(2));
    limit.extra.insert("deathmatchEnabled".into(), json!(false));
    assert!(check_star_limit(&mut limit).unwrap());
    assert_eq!(
        limit.extra["replayEndReason"],
        "2수: 별 합계가 같아 무승부입니다. (0 : 0)"
    );
}

#[test]
fn v7_deathmatch_entry_normalizes_the_stored_turn_limit() {
    // Frozen main-OahWs0tU.js startDeathmatch calls
    // normalizeDeathmatchLimitTurns and writes its result to the state before
    // deriving intervalHalfTurns. These synthetic limit-entry probes do not
    // claim that the intervening two natural turns are executable in Rust.
    for (input, normalized, interval) in [
        (json!(1.6), 2, 4),
        (json!("4"), 4, 8),
        (Value::Null, 10, 20),
    ] {
        let mut state = play_state(RULES_VERSION_V7);
        state.turns_taken = Sides::new(2, 2);
        state.extra.insert("starWinLimit".into(), json!(2));
        state.extra.insert("deathmatchEnabled".into(), json!(true));
        state.extra.insert("deathmatchLimitTurns".into(), input);
        let rng = state.rng.clone();
        assert!(!check_star_limit(&mut state).unwrap());
        assert_eq!(state.mode, "play");
        assert_eq!(state.extra["deathmatchLimitTurns"], normalized);
        assert_eq!(state.extra["deathmatch"]["intervalHalfTurns"], interval);
        assert_eq!(state.extra["deathmatch"]["startedAtTurn"], 2);
        assert_eq!(state.extra["endPhaseStartMove"], 2);
        assert_eq!(state.rng, rng);
        assert_eq!(
            state.extra["logs"].as_array().unwrap().last().unwrap(),
            &json!(format!(
                "연장전 시작: 2수 이후 {normalized}수 동안 폰 이동, 포획, 액티브 카드 사용이 없으면 별이 더 적은 쪽이 승리합니다."
            ))
        );
    }

    let mut v6 = play_state(RULES_VERSION_V6);
    v6.turns_taken = Sides::new(2, 2);
    v6.extra.insert("starWinLimit".into(), json!(2));
    v6.extra.insert("deathmatchEnabled".into(), json!(true));
    v6.extra.insert("deathmatchLimitTurns".into(), json!(1.6));
    assert!(!check_star_limit(&mut v6).unwrap());
    assert_eq!(v6.extra["deathmatchLimitTurns"], 1.6);
    assert_eq!(v6.extra["deathmatch"]["intervalHalfTurns"], 20);

    let mut out_of_range = play_state(RULES_VERSION_V7);
    out_of_range.turns_taken = Sides::new(2, 2);
    out_of_range.extra.insert("starWinLimit".into(), json!(2));
    out_of_range
        .extra
        .insert("deathmatchEnabled".into(), json!(true));
    out_of_range
        .extra
        .insert("deathmatchLimitTurns".into(), json!(1_u64 << 52));
    let before = out_of_range.clone();
    assert!(matches!(
        check_star_limit(&mut out_of_range),
        Err(EngineError::InvalidState(_))
    ));
    assert_eq!(out_of_range, before);
}

#[test]
fn overtime_finishes_only_at_black_boundary_and_keeps_unprefixed_reason() {
    let mut state = play_state(RULES_VERSION_V7);
    state.extra.insert(
        "deathmatch".into(),
        json!({"active":true,"startedAtTurn":1,"halfTurnsSinceProgress":0,"intervalHalfTurns":2,"progressThisTurn":false,"warningKey":""}),
    );
    assert!(!tick_deathmatch(&mut state, Color::White).unwrap());
    assert_eq!(state.mode, "play");
    assert!(tick_deathmatch(&mut state, Color::Black).unwrap());
    assert_eq!(
        state.extra["replayEndReason"],
        "별 합계가 같아 무승부입니다. (0 : 0)"
    );
}

#[test]
fn no_action_probe_recognizes_v7_football_moves_without_changing_v6() {
    let mut v7 = play_state(RULES_VERSION_V7);
    v7.board[0][0] = Some(Piece::new("football", PieceColor::Neutral, "ball"));
    let before = v7.clone();
    // Source hasAnyLegalMove includes the neutral football even though the
    // public physical-piece hints omit it. A corner ball has three legal
    // corner moves without a kicker; a ball alone at e4 cannot be kicked.
    assert!(!check_no_action_loss(&mut v7).unwrap());
    assert_eq!(v7.mode, before.mode);
    assert_eq!(v7.winner, before.winner);

    let mut v6 = play_state(RULES_VERSION_V6);
    v6.board[0][0] = Some(Piece::new("football", PieceColor::Neutral, "ball"));
    assert!(matches!(
        check_no_action_loss(&mut v6),
        Err(EngineError::UnsupportedFeature(_))
    ));
    assert_eq!(v6.mode, "play");
}

#[test]
fn v7_terminal_replay_settles_once_after_end_game() {
    let mut state = GameState::new(
        GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        },
        17,
    )
    .unwrap();
    state.ruleset_id = RULES_VERSION_V7.into();
    state.mode = "play".into();
    let rng = state.rng.clone();
    let events_before = state.extra["replayEvents"].as_array().unwrap().len();
    let history_before = state.extra["boardHistory"].as_array().unwrap().len();
    let nonce_before = state.extra["replayEventNonce"].as_u64().unwrap();

    end_game(&mut state, Some(Color::White), "검증용 종료").unwrap();
    assert!(state.gameover_replay_pending);
    assert_eq!(
        state.extra["replayEvents"].as_array().unwrap().len(),
        events_before
    );
    crate::replay::settle(&mut state).unwrap();
    assert!(!state.gameover_replay_pending);
    assert_eq!(
        state.extra["replayEvents"].as_array().unwrap().len(),
        events_before + 1
    );
    assert_eq!(
        state.extra["boardHistory"].as_array().unwrap().len(),
        history_before + 1
    );
    assert_eq!(state.extra["replayEventNonce"], json!(nonce_before + 1));
    assert_eq!(
        state.extra["replayEvents"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["label"],
        "gameover"
    );
    assert_eq!(state.rng, rng);
    crate::replay::settle(&mut state).unwrap();
    assert_eq!(
        state.extra["replayEvents"].as_array().unwrap().len(),
        events_before + 1
    );
}
