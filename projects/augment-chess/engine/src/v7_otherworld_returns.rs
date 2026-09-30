//! 동결된 v7의 이세계 예약 귀환. 원본 main:99104는 반수 카운터를
//! 갱신한 뒤 만기가 된 ID를 큐에서 제거하고, 같은 배치를 끝까지 처리한다.
//! 호출자는 moveCount를 이미 증가시켰으며 이 모듈은 다른 턴 큐를 정산하지 않는다.

use crate::{Color, EngineError, GameState, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};

/// Source `resolvePendingOtherworldAfterMove`. Move counting remains with the
/// caller. A failed callback cannot expose a partially advanced reservation,
/// created wizard, capture, or RNG stream.
pub(crate) fn resolve_after_move(state: &mut GameState) -> Result<usize> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 resolvePendingOtherworldAfterMove on rules version {}",
            state.ruleset_id,
        )));
    }
    let Some(entries) = state
        .extra
        .get("pendingOtherworld")
        .and_then(Value::as_array)
    else {
        return Ok(0);
    };
    if entries.is_empty() {
        return Ok(0);
    }
    let mut next = state.clone();
    let returned = resolve_owned(&mut next, entries.clone())?;
    *state = next;
    Ok(returned)
}

fn resolve_owned(state: &mut GameState, entries: Vec<Value>) -> Result<usize> {
    let mut advanced = Vec::with_capacity(entries.len());
    let mut due = Vec::new();
    for mut entry in entries {
        if !entry.is_object() {
            return Err(EngineError::InvalidState(
                "v7 pendingOtherworld entries must be objects".into(),
            ));
        }
        // The source detects whether advanceScheduledHalfTurn returned a new
        // object. Only a nonnegative safe integer takes that branch; null,
        // numeric strings and single-element numeric arrays are JS Numbers.
        let remaining = scheduler_number(entry.get("remainingHalfTurns"));
        let is_due = if let Some(remaining) = remaining.filter(|number| {
            number.is_finite()
                && number.fract() == 0.0
                && (0.0..=9_007_199_254_740_991.0).contains(number)
        }) {
            let remaining = (remaining - 1.0).max(0.0) as u64;
            entry
                .as_object_mut()
                .expect("entry object checked")
                .insert("remainingHalfTurns".into(), json!(remaining));
            remaining == 0
        } else {
            // The legacy comparison is raw Number(...), not a floored turn
            // counter. NaN becomes zero through `|| 0`; Infinity remains due
            // only when the actual comparison permits it.
            f64::from(state.move_count)
                >= scheduler_number(entry.get("dueMoveCount")).unwrap_or(0.0)
        };
        if is_due {
            due.push(entry.clone());
        }
        advanced.push(entry);
    }
    if due.is_empty() {
        state
            .extra
            .insert("pendingOtherworld".into(), Value::Array(advanced));
        return Ok(0);
    }
    // Source Set equality removes every reservation sharing a due ID, even
    // when another reservation with that ID has not reached its own timer.
    // Snapshot JSON cannot express object-identity IDs safely; created source
    // reservations use strings. Reject that active shape with an exact cause.
    if due.iter().any(|entry| {
        entry
            .get("id")
            .is_some_and(|id| id.is_array() || id.is_object())
    }) {
        return Err(EngineError::UnsupportedFeature(
            "v7 pendingOtherworld due id uses JavaScript object identity".into(),
        ));
    }
    advanced.retain(|entry| {
        !due.iter()
            .any(|due| same_id(entry.get("id"), due.get("id")))
    });
    state
        .extra
        .insert("pendingOtherworld".into(), Value::Array(advanced));

    let mut returned = 0;
    for entry in &due {
        // No terminal early return: source forEach continues after a royal
        // crush or campaign objective ends the game during the same batch.
        if return_one(state, entry)? {
            returned += 1;
        }
    }
    crate::transition::refresh_submerged(state)?;
    let owner = entry_owner(&due[0])?;
    // Offline adapter context has online.enabled=false. playSound and
    // recordOnlineEvent therefore do not alter the exported rule state.
    crate::replay::queue_special_effect_notation(
        state,
        owner,
        "이세계",
        &format!("{} 이세계 귀환", crate::replay::label(owner)),
    )?;
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(returned)
}

fn same_id(first: Option<&Value>, second: Option<&Value>) -> bool {
    match (first, second) {
        (None, None) => true,
        (Some(Value::Number(first)), Some(Value::Number(second))) => {
            first.as_f64() == second.as_f64()
        }
        (Some(Value::Array(_) | Value::Object(_)), _)
        | (_, Some(Value::Array(_) | Value::Object(_))) => false,
        _ => first == second,
    }
}

fn return_one(state: &mut GameState, entry: &Value) -> Result<bool> {
    let Some(row) = scheduler_number(entry.get("row")) else {
        return Ok(false);
    };
    let Some(col) = scheduler_number(entry.get("col")) else {
        return Ok(false);
    };
    // Source inBounds skips absent/NaN/outside coordinates before spawning.
    if !(0.0..8.0).contains(&row) || !(0.0..8.0).contains(&col) {
        return Ok(false);
    }
    // expansionDestinationAllowed additionally rejects fractional cells.
    // Its source denial log still uses the original numeric square spelling.
    if row.fract() != 0.0 || col.fract() != 0.0 {
        let file = if col.fract() == 0.0 {
            char::from(b'a' + col as u8).to_string()
        } else {
            "?".into()
        };
        let rank =
            String::from_utf8(serde_jcs::to_vec(&(8.0 - row)).map_err(EngineError::serialization)?)
                .map_err(|error| {
                    EngineError::InvalidState(format!(
                        "v7 Otherworld fractional square rank is not UTF-8: {error}",
                    ))
                })?;
        crate::replay::add_log(
            state,
            format!("이세계: {file}{rank}에 귀환할 수 없어 기물이 소멸했습니다."),
        )?;
        return Ok(false);
    }
    let square = Square {
        row: row as u8,
        col: col as u8,
    };
    let owner = entry_owner(entry)?;
    // This source caller deliberately omits synchronization. Reuse the
    // installation predicate that accepts d4 alone.
    if !crate::movement::d4_destination_allowed(state, owner.into(), &[square]) {
        crate::replay::add_log(
            state,
            format!(
                "이세계: {}에 귀환할 수 없어 기물이 소멸했습니다.",
                name(square),
            ),
        )?;
        return Ok(false);
    }
    let mut wizard = crate::opening::spawn(state, owner, "wizard")?;
    if let Some(id) = entry
        .get("pieceId")
        .filter(|value| crate::observation::truth(Some(value)))
    {
        wizard.id = id
            .as_str()
            .ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 pendingOtherworld.pieceId must be a string when truthy".into(),
                )
            })?
            .into();
    }
    let origin = entry
        .get("origin")
        .filter(|value| crate::observation::truth(Some(value)))
        .cloned()
        .unwrap_or_else(|| json!(name(square)));
    wizard.extra.insert("origin".into(), origin);
    wizard.extra.insert("mana".into(), json!(0));
    wizard.extra.insert("maxMana".into(), json!(5));
    wizard.moved = true;
    if state.at(square).is_some() {
        let crushed = crate::transition::force_remove_piece_at(state, square, owner)?;
        if let Some(crushed) = crushed {
            let source = json!({
                "attacker":wizard,
                "origin":square,
                "effect":"otherworld-return",
                "spawn":true,
            });
            let threat_probe = state.threat_probe_depth > 0;
            crate::v7_threat::mark_king_threat_removal_cause(
                state,
                &crushed,
                square,
                &source,
                threat_probe,
            )?;
        }
        // piece() starts with no totalCaptures. Increment after the blocker
        // branch regardless of the force-removal return value, like source.
        wizard.extra.insert("totalCaptures".into(), json!(1));
    }
    if crate::movement::collapsed(state, square) {
        // Source directly appends this wizard to the opponent's capture list.
        // It does not dispatch general capture/royal/environment callbacks.
        state.captures.get_mut(owner.opponent()).push(wizard);
        crate::replay::add_log(
            state,
            format!(
                "이세계: {}로 귀환한 마법사가 붕괴된 칸에서 즉시 사망했습니다.",
                name(square),
            ),
        )?;
        return Ok(false);
    }
    state.board[square.row as usize][square.col as usize] = Some(wizard.clone());
    crate::card_effects::mark_animation(state, &wizard)?;
    crate::replay::add_piece_action_log(
        state,
        &wizard,
        Some(square),
        None,
        format!("이세계: {}에 마법사로 귀환했습니다.", name(square),),
    )?;
    Ok(true)
}

fn entry_owner(entry: &Value) -> Result<Color> {
    match entry.get("color").and_then(Value::as_str) {
        Some("white") => Ok(Color::White),
        Some("black") => Ok(Color::Black),
        _ => Err(EngineError::InvalidState(
            "v7 pendingOtherworld.color must name a player".into(),
        )),
    }
}

fn name(square: Square) -> String {
    format!("{}{}", char::from(b'a' + square.col), 8 - square.row)
}

/// The common source parser returns finite Numbers. Scheduler comparisons
/// also distinguish valid Infinity text from NaN: Infinity cannot be replaced
/// by `|| 0`. Recover only nonfinite syntax after the shared finite coercion.
fn scheduler_number(value: Option<&Value>) -> Option<f64> {
    if let Some(number) = crate::card_effects::js_number(value, 0) {
        return Some(number);
    }
    let mut leaf = value?;
    let mut depth = 0;
    while let Value::Array(values) = leaf {
        if values.len() != 1 || depth >= 64 {
            return None;
        }
        leaf = &values[0];
        depth += 1;
    }
    let Value::String(text) = leaf else {
        return None;
    };
    let text =
        text.trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}');
    match text {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    if text
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.' | b'e' | b'E'))
        && let Ok(number) = text.parse::<f64>()
        && number.is_infinite()
    {
        return Some(number);
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if text.strip_prefix(prefix).is_some_and(|digits| {
            !digits.is_empty() && digits.chars().all(|digit| digit.to_digit(radix).is_some())
        }) {
            return Some(f64::INFINITY);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Piece;

    fn state() -> GameState {
        let mut state = crate::v7_new_game::new_game(crate::GameConfig::default(), 37).unwrap();
        state.mode = "play".into();
        state
    }

    fn plan(id: &str, square: Square, remaining: Value) -> Value {
        json!({"id":id,"pieceId":format!("returned-{id}"),"color":"white",
            "row":square.row,"col":square.col,"origin":"a2",
            "dueMoveCount":900,"remainingHalfTurns":remaining})
    }

    #[test]
    fn scheduler_preserves_legacy_numbers_and_due_id_set_semantics() {
        let mut state = state();
        state.move_count = 20;
        let mut overdue = plan("duplicate", Square { row: 8, col: 0 }, json!(1));
        overdue["remainingHalfTurns"] = json!(null);
        let mut legacy = plan("legacy", Square { row: 8, col: 0 }, json!(-2));
        legacy["dueMoveCount"] = json!("20.5");
        let mut infinity = plan("infinity", Square { row: 8, col: 0 }, json!("Infinity"));
        infinity["dueMoveCount"] = json!("Infinity");
        state.extra.insert(
            "pendingOtherworld".into(),
            json!([
                overdue,
                plan("duplicate", Square { row: 5, col: 0 }, json!(5)),
                plan("future", Square { row: 5, col: 1 }, json!(["0x3"])),
                legacy,
                infinity,
            ]),
        );
        let cursor = state.rng.cursor;
        assert_eq!(resolve_after_move(&mut state).unwrap(), 0);
        let queue = state.extra["pendingOtherworld"].as_array().unwrap();
        assert_eq!(queue.len(), 3);
        assert_eq!(queue[0]["id"], "future");
        assert_eq!(queue[0]["remainingHalfTurns"], 2);
        assert_eq!(queue[1]["remainingHalfTurns"], -2);
        assert_eq!(queue[2]["remainingHalfTurns"], "Infinity");
        assert_eq!(
            state.rng.cursor,
            cursor + 1,
            "invalid coordinate still queues batch notation"
        );
        assert_eq!(state.move_count, 20, "caller owns counting");
    }

    #[test]
    fn return_uses_d4_without_synchronization_and_keeps_source_identity() {
        let mut state = state();
        state
            .extra
            .insert("d4".into(), json!({"white":false,"black":true}));
        state.extra.insert(
            "synchronization".into(),
            json!({"white":false,"black":true}),
        );
        // A uniform black parity would forbid a3 under synchronization. This
        // installation source omits that argument and still returns there.
        state.board = vec![vec![None; 8]; 8];
        state.board[0][0] = Some(Piece::new("king", Color::Black, "black-royal"));
        state.board[7][7] = Some(Piece::new("king", Color::White, "white-royal"));
        state.extra.insert(
            "pendingOtherworld".into(),
            json!([
                plan("d4-denied", Square { row: 3, col: 3 }, json!(1)),
                plan("returned", Square { row: 5, col: 0 }, json!(1)),
            ]),
        );
        let cursor = state.rng.cursor;
        assert_eq!(resolve_after_move(&mut state).unwrap(), 1);
        let wizard = state.at(Square { row: 5, col: 0 }).unwrap();
        assert_eq!(wizard.kind, "wizard");
        assert_eq!(wizard.id, "returned-returned");
        assert!(wizard.moved);
        assert_eq!(wizard.extra["origin"], "a2");
        assert_eq!(wizard.extra["mana"], 0);
        assert_eq!(wizard.extra["maxMana"], 5);
        assert_eq!(
            state.rng.cursor,
            cursor + 2,
            "wizard creation draw remains when ID is overridden"
        );
    }

    #[test]
    fn collapsed_return_crushes_occupant_before_direct_wizard_capture() {
        let mut state = state();
        let square = Square { row: 6, col: 0 };
        let victim = state.at(square).unwrap().clone();
        state.extra.insert("collapsed".into(), json!(true));
        state.extra.insert("collapseDepth".into(), json!(1));
        state.extra.insert(
            "pendingOtherworld".into(),
            json!([plan("crush", square, json!(1))]),
        );
        assert_eq!(resolve_after_move(&mut state).unwrap(), 0);
        assert!(state.at(square).is_none());
        assert!(
            state
                .captures
                .white
                .iter()
                .any(|piece| piece.id == victim.id)
        );
        let wizard = state
            .captures
            .black
            .iter()
            .find(|piece| piece.id == "returned-crush")
            .unwrap();
        assert_eq!(wizard.kind, "wizard");
        assert_eq!(wizard.extra["totalCaptures"], 1);
    }

    #[test]
    fn invalid_return_identity_rolls_back_queue_rng_and_board() {
        let mut state = state();
        let mut reservation = plan("bad-id", Square { row: 5, col: 0 }, json!(1));
        reservation["pieceId"] = json!(true);
        state
            .extra
            .insert("pendingOtherworld".into(), json!([reservation]));
        let before = state.clone();
        let error = resolve_after_move(&mut state).unwrap_err();
        assert!(error.to_string().contains("pendingOtherworld.pieceId"));
        assert_eq!(state, before);
    }

    #[test]
    #[ignore = "external frozen-source receipt is generated outside Git"]
    fn temporary_source_otherworld_matrix() {
        use sha2::{Digest, Sha256};

        let path = std::env::var("ACCELERATE_OTHERWORLD_SOURCE_RECEIPT").unwrap();
        let receipt: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let cases =
            crate::tests::source_callback_fixture::validate_source_receipt(&receipt).unwrap();
        let profile: Value = serde_json::from_str(include_str!(
            "../../contracts/catalog/execution-profile-20260928.json"
        ))
        .unwrap();
        let profile_sha = format!("{:x}", Sha256::digest(serde_jcs::to_vec(&profile).unwrap()));
        assert_eq!(receipt["status"], "source-generated");
        assert_eq!(receipt["nativeCompared"], json!(false));
        assert_eq!(receipt["rulesVersion"], RULES_VERSION_V7);
        assert_eq!(
            receipt["catalogVersion"],
            crate::v7_execution_profile::catalog_version().unwrap()
        );
        assert_eq!(
            receipt["executionProfile"]["profileVersion"],
            profile["profileVersion"]
        );
        assert_eq!(receipt["executionProfile"]["profileSha256"], profile_sha);
        assert_eq!(
            receipt["executionProfile"]["selectedInitializerCount"],
            json!(175)
        );
        assert_eq!(
            receipt["executionProfile"]["excludedInitializerCount"],
            json!(168)
        );
        assert_eq!(receipt["executionBoundary"], "restore-exported-before");
        assert_eq!(
            receipt["contextBoundary"],
            "source-ai-and-royal-threat-depths-explicit-through-callback-and-settlement"
        );
        assert_eq!(receipt["semanticFieldsExcluded"], json!([]));
        assert_eq!(
            cases.len(),
            26,
            "all preserved Otherworld source cases are required"
        );
        assert_eq!(
            receipt["scope"],
            "direct resolvePendingOtherworldAfterMove + queued terminal replay; not full move parity"
        );
        eprintln!(
            "Otherworld comparison scope: rawGameState callbacks, {} cases, boundary={}; public host admission and full moves are not exercised",
            cases.len(),
            receipt["executionBoundary"]
                .as_str()
                .unwrap_or("live-source-callback")
        );
        let mut mismatches = Vec::new();
        for (index, case) in cases.iter().enumerate() {
            let name = case["name"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("case[{index}]"));
            crate::tests::source_callback_fixture::collect_case_diagnostics(
                &name,
                &mut mismatches,
                |mismatches| compare_source_otherworld_case(case, &name, mismatches),
            );
        }
        for mismatch in &mismatches {
            eprintln!("{mismatch}");
        }
        assert!(
            mismatches.is_empty(),
            "{} Otherworld source callback mismatches",
            mismatches.len()
        );
    }

    fn compare_source_otherworld_case(
        case: &Value,
        name: &str,
        mismatches: &mut Vec<String>,
    ) -> Result<()> {
        use crate::tests::source_callback_fixture::{
            compare_callback_envelope, source_callback_state,
        };
        if let Some(error) = case.get("error") {
            return Err(EngineError::InvalidState(format!(
                "source callback error {error}"
            )));
        }
        let mut state = source_callback_state(&case["before"])?;
        // main85645/85646의 왕족 probe는 두 depth를 함께 올리지만
        // main88225/109741의 기보·종료 예약은 AI depth만 검사한다.
        // 두 값은 snapshot에 없으므로 실제 producer context를 각각 복원한다.
        let source_ai_simulation = case["sourceAiSimulation"].as_bool().ok_or_else(|| {
            EngineError::InvalidState("Otherworld sourceAiSimulation must be a boolean".into())
        })?;
        let source_threat_probe = case["sourceThreatProbe"].as_bool().ok_or_else(|| {
            EngineError::InvalidState("Otherworld sourceThreatProbe must be a boolean".into())
        })?;
        for (field, enabled) in [
            ("aiSimulationDepth", source_ai_simulation),
            ("kingThreatProbeDepth", source_threat_probe),
        ] {
            if case["executionContext"][field].as_u64() != Some(u64::from(enabled)) {
                return Err(EngineError::InvalidState(format!(
                    "Otherworld source context {field} does not match its recorded control"
                )));
            }
        }
        state.ai_simulation_depth = u32::from(source_ai_simulation);
        state.threat_probe_depth = u32::from(source_threat_probe);
        let count = resolve_after_move(&mut state)?;
        let expected_count = case["returned"].as_u64().ok_or_else(|| {
            EngineError::InvalidState("receipt returned count must be a nonnegative integer".into())
        })?;
        if expected_count != count as u64 {
            mismatches.push(format!(
                "{name}: return count source={expected_count}, native={count}"
            ));
        }
        compare_callback_envelope(
            &state,
            &case["rawAfter"],
            &format!("{name}/callback"),
            mismatches,
        )?;
        crate::replay::settle(&mut state)?;
        compare_callback_envelope(
            &state,
            &case["after"],
            &format!("{name}/settled"),
            mismatches,
        )?;
        Ok(())
    }
}
