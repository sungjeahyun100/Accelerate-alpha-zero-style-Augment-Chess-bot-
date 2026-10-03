use super::*;

fn limit_entry_state(ruleset: &str) -> GameState {
    let mut state = GameState::new(
        GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        },
        17,
    )
    .unwrap();
    state.ruleset_id = ruleset.into();
    state.mode = "play".into();
    state.turns_taken = Sides::new(2, 2);
    state.extra.insert("logs".into(), json!([]));
    state.extra.insert("starWinLimit".into(), json!(2));
    state.extra.insert("deathmatchEnabled".into(), json!(true));
    state
}

#[test]
fn v7_deathmatch_entry_stores_the_normalized_limit_without_using_rng() {
    // These are direct limit-entry probes; they do not imply that the
    // intervening natural turns can yet execute through the public Position.
    for (input, normalized, interval) in [
        (json!(1.6), 2, 4),
        (json!("4"), 4, 8),
        (Value::Null, 10, 20),
    ] {
        let mut state = limit_entry_state(RULES_VERSION_V7);
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
}

#[test]
fn v7_limit_rejects_inexact_interval_atomically_and_v6_keeps_its_rule() {
    let mut v7 = limit_entry_state(RULES_VERSION_V7);
    v7.extra
        .insert("deathmatchLimitTurns".into(), json!(1_u64 << 52));
    let before = v7.clone();
    assert!(matches!(
        check_star_limit(&mut v7),
        Err(EngineError::InvalidState(reason)) if reason.contains("exact integer bound")
    ));
    assert_eq!(v7, before);

    let mut v6 = limit_entry_state(RULES_VERSION_V6);
    v6.extra.insert("deathmatchLimitTurns".into(), json!(1.6));
    assert!(!check_star_limit(&mut v6).unwrap());
    assert_eq!(v6.extra["deathmatchLimitTurns"], 1.6);
    assert_eq!(v6.extra["deathmatch"]["intervalHalfTurns"], 20);
}
