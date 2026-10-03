//! Immediate effects of the source-pinned v7 timed RULE cards.
//!
//! Card selection, notation, cost and replay events belong to their respective
//! owners. Periodic collapse and crown progression use the shared board
//! automata; this module also owns Winter's forecast and freeze cycle.
//! Source: main-OahWs0tU.js (SHA-256 e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c),
//! enablePeriodicCollapse, revelation, enableCrownRule and applyWinterFreezeCycle.

use crate::{Color, EngineError, GameState, RULES_VERSION_V7, Result, Square};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

const PERIODIC_COLLAPSE_INTERVAL: u64 = 20;
const REVELATION_INTERVAL_HALF_TURNS: u64 = 10;
const LAST_WARMTH_PIECE_LIMIT: usize = 4;

/// Source main74235. Even a disabled rule is normalized on every callback.
/// Forecast draws occur after White's third turn, before Black completes the
/// pair. A matching forecast is later filtered in board order without draws.
/// This callback is transactional so an exhausted tape cannot partially thaw.
pub(crate) fn apply_winter_freeze_cycle(state: &mut GameState, force: bool) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "Winter freeze cycle requires frozen v7".into(),
        ));
    }
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::InvalidState("Winter board must be 8x8".into()));
    }
    let mut next = state.clone();
    let frozen = winter_freeze_cycle_inner(&mut next, force)?;
    *state = next;
    Ok(frozen)
}

fn normalize_winter(value: Option<&Value>) -> Result<Map<String, Value>> {
    let field = |name: &str| value.and_then(|value| value.get(name));
    let mut winter = Map::new();
    if let Some(cycle) = field("previewCycle").filter(|value| {
        value
            .as_f64()
            .is_some_and(|number| number.is_finite() && number.fract() == 0.0)
    }) {
        winter.insert("previewCycle".into(), cycle.clone());
        let mut seen = BTreeSet::new();
        let ids = field("previewIds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|id| seen.insert((*id).to_owned()))
            .take(6)
            .map(|id| json!(id))
            .collect::<Vec<_>>();
        winter.insert("previewIds".into(), Value::Array(ids));
    }
    winter.insert(
        "enabled".into(),
        json!(crate::observation::truth(field("enabled"))),
    );
    winter.insert(
        "lastCycle".into(),
        numeric_value(number_or_zero(
            field("lastCycle"),
            "winterKingdom.lastCycle",
        )?),
    );
    let mut frozen_ids = Vec::new();
    for id in field("frozenIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|id| source_truthy(id))
    {
        // The source maps every truthy entry before slicing. Conversion errors
        // after the twelfth entry must therefore still reach the caller.
        let id = source_string(id, 0)?;
        if frozen_ids.len() < 12 {
            frozen_ids.push(json!(id));
        }
    }
    winter.insert("frozenIds".into(), Value::Array(frozen_ids));
    winter.insert(
        "disabledByLastWarmth".into(),
        json!(crate::observation::truth(field("disabledByLastWarmth"))),
    );
    Ok(winter)
}

fn source_string(value: &Value, depth: usize) -> Result<String> {
    if depth > 64 {
        return Err(EngineError::InvalidState(
            "Winter frozen ID String conversion exceeds depth 64".into(),
        ));
    }
    match value {
        Value::Null => Ok("null".into()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(_) => String::from_utf8(serde_jcs::to_vec(value).map_err(EngineError::serialization)?)
            .map_err(|error| EngineError::InvalidState(error.to_string())),
        Value::String(value) => Ok(value.clone()),
        Value::Array(values) => values.iter().map(|value| {
            if value.is_null() { Ok(String::new()) } else { source_string(value, depth + 1) }
        }).collect::<Result<Vec<_>>>().map(|parts| parts.join(",")),
        Value::Object(value) if value.contains_key("toString") => Err(EngineError::InvalidState(
            "Winter frozen ID String conversion: TypeError: Cannot convert object to primitive value".into(),
        )),
        Value::Object(_) => Ok("[object Object]".into()),
    }
}

fn source_numeric(value: Option<&Value>, depth: usize) -> Result<Option<f64>> {
    if depth > 64 {
        return Err(EngineError::InvalidState(
            "RULE Number conversion exceeds depth 64".into(),
        ));
    }
    if let Some(number) = crate::card_effects::js_number(value, depth) {
        return Ok(Some(number));
    }
    match value {
        Some(Value::String(value)) => {
            let text = value.trim();
            let infinity = matches!(text, "Infinity" | "+Infinity" | "-Infinity")
                || (!text.is_empty()
                    && text.bytes().all(|character| {
                        character.is_ascii_digit() || b".eE+-".contains(&character)
                    }));
            Ok(text
                .parse::<f64>()
                .ok()
                .filter(|number| infinity && number.is_infinite()))
        }
        Some(value @ Value::Array(_)) => source_numeric(
            Some(&Value::String(source_string(value, depth + 1)?)),
            depth + 1,
        ),
        Some(Value::Object(value)) if value.contains_key("toString") => {
            Err(EngineError::InvalidState(
                "RULE Number conversion: TypeError: Cannot convert object to primitive value"
                    .into(),
            ))
        }
        _ => Ok(None),
    }
}

fn number_or_zero(value: Option<&Value>, field: &str) -> Result<f64> {
    let number = source_numeric(value, 0)?.unwrap_or(0.0);
    if !number.is_finite() {
        return Err(EngineError::InvalidState(format!(
            "{field} source Number is not finite"
        )));
    }
    Ok(if number == 0.0 { 0.0 } else { number })
}

fn numeric_value(number: f64) -> Value {
    if number.fract() == 0.0 && number.abs() <= 9_007_199_254_740_991.0 {
        if number >= 0.0 {
            json!(number as u64)
        } else {
            json!(number as i64)
        }
    } else {
        json!(number)
    }
}

fn clear_winter_frozen_pieces(state: &mut GameState) -> Result<()> {
    for piece in state.board.iter_mut().flatten().flatten() {
        if piece.extra.get("frozen").is_some_and(source_truthy)
            && !source_numeric(
                piece
                    .extra
                    .get("frozenByCard")
                    .and_then(|value| value.get("remaining")),
                0,
            )?
            .is_some_and(|number| number > 0.0)
        {
            piece.extra.shift_remove("frozen");
        }
    }
    Ok(())
}

fn winter_eligible_pieces(state: &GameState, color: Color) -> Vec<(String, Square)> {
    let mut seen = BTreeSet::new();
    let mut eligible = Vec::new();
    for (row, line) in state.board.iter().enumerate() {
        for (col, piece) in line.iter().enumerate() {
            let Some(piece) = piece else {
                continue;
            };
            if piece.id.is_empty()
                || seen.contains(&piece.id)
                || piece.color != color
                || crate::v7_threat::is_royal_identity_v7(state, piece)
                || matches!(
                    piece.kind.as_str(),
                    "wall" | "football" | "blackHole" | "scarecrow"
                )
            {
                continue;
            }
            seen.insert(piece.id.clone());
            eligible.push((
                piece.id.clone(),
                Square {
                    row: row as u8,
                    col: col as u8,
                },
            ));
        }
    }
    eligible
}

fn winter_shuffle(state: &mut GameState, entries: &mut [(String, Square)]) -> Result<()> {
    for index in (1..entries.len()).rev() {
        let draw = state.rng.sample()?;
        if !draw.is_finite() || !(0.0..1.0).contains(&draw) {
            return Err(EngineError::InvalidState(
                "Winter random draw outside [0,1)".into(),
            ));
        }
        let chosen = (draw * (index + 1) as f64).floor() as usize;
        state
            .rng
            .record_last_probability(1.0 / (index + 1) as f64, "source Winter candidate shuffle")?;
        entries.swap(index, chosen);
    }
    Ok(())
}

fn winter_freeze_cycle_inner(state: &mut GameState, force: bool) -> Result<bool> {
    let mut winter = normalize_winter(state.extra.get("winterKingdom"))?;
    let mut frozen = false;
    if winter["enabled"] == true && state.mode != "gameover" {
        let eligible = [
            winter_eligible_pieces(state, Color::White),
            winter_eligible_pieces(state, Color::Black),
        ];
        let last_warmth = winter["disabledByLastWarmth"] == true
            || eligible
                .iter()
                .any(|entries| entries.len() <= LAST_WARMTH_PIECE_LIMIT);
        if last_warmth {
            clear_winter_frozen_pieces(state)?;
            winter.insert("frozenIds".into(), json!([]));
            winter.insert("disabledByLastWarmth".into(), json!(true));
            winter.shift_remove("previewCycle");
            winter.shift_remove("previewIds");
        } else {
            let white = u64::from(state.turns_taken.white);
            let black = u64::from(state.turns_taken.black);
            let copy_pools = super::uses_current_copy_pool(state);
            if copy_pools
                && white == black + 1
                && white.is_multiple_of(3)
                && winter.get("previewCycle").and_then(Value::as_f64) != Some(white as f64)
            {
                winter.insert("previewCycle".into(), json!(white));
                let mut preview_ids = Vec::with_capacity(6);
                for entries in &eligible {
                    let mut entries = entries.clone();
                    winter_shuffle(state, &mut entries)?;
                    preview_ids.extend(entries.into_iter().take(3).map(|(id, _)| json!(id)));
                }
                winter.insert("previewIds".into(), Value::Array(preview_ids));
            }
            let cycle = white.min(black);
            if cycle > 0
                && cycle.is_multiple_of(3)
                && (force || winter["lastCycle"].as_f64() != Some(cycle as f64))
            {
                clear_winter_frozen_pieces(state)?;
                let preview =
                    winter.get("previewCycle").and_then(Value::as_f64) == Some(cycle as f64);
                let preview_ids = winter
                    .get("previewIds")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let mut frozen_ids = Vec::with_capacity(6);
                for mut entries in eligible {
                    if preview {
                        entries.retain(|(id, _)| preview_ids.contains(&json!(id)));
                    } else {
                        winter_shuffle(state, &mut entries)?;
                        entries.truncate(3);
                    }
                    for (id, _) in entries {
                        for piece in state
                            .board
                            .iter_mut()
                            .flatten()
                            .flatten()
                            .filter(|piece| piece.id == id)
                        {
                            piece.extra.insert("frozen".into(), json!(true));
                        }
                        frozen_ids.push(json!(id));
                    }
                }
                winter.shift_remove("previewCycle");
                winter.shift_remove("previewIds");
                winter.insert("lastCycle".into(), json!(cycle));
                frozen = !frozen_ids.is_empty();
                winter.insert("frozenIds".into(), Value::Array(frozen_ids));
                if frozen {
                    crate::replay::add_log(
                        state,
                        "겨울 왕국: 흑과 백의 기물이 얼어붙었습니다.".into(),
                    )?;
                }
            }
        }
    }
    state
        .extra
        .insert("winterKingdom".into(), Value::Object(winter));
    Ok(frozen)
}

/// Apply one immediate v7 RULE effect. A failed effect leaves both the state
/// and the random tape cursor unchanged, including failures after a draw.
pub(crate) fn apply(state: &mut GameState, card_id: &str) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "timed RULE {card_id} on rules version {}",
            state.ruleset_id
        )));
    }
    let mut next = state.clone();
    match card_id {
        "periodic-collapse" => periodic_collapse(&mut next)?,
        "revelation" => revelation(&mut next)?,
        "crown" => crown(&mut next)?,
        other => {
            return Err(EngineError::UnsupportedFeature(format!(
                "v7 timed RULE {other}"
            )));
        }
    }
    *state = next;
    Ok(())
}

fn shared_turn_count(state: &GameState) -> u64 {
    u64::from(state.turns_taken.white.min(state.turns_taken.black))
}

fn periodic_collapse(state: &mut GameState) -> Result<()> {
    let next_at = shared_turn_count(state)
        .checked_div(PERIODIC_COLLAPSE_INTERVAL)
        .and_then(|cycle| cycle.checked_add(1))
        .and_then(|cycle| cycle.checked_mul(PERIODIC_COLLAPSE_INTERVAL))
        .ok_or_else(|| EngineError::InvalidState("periodic collapse turn overflow".into()))?;
    state.extra.insert(
        "periodicCollapse".into(),
        json!({"enabled":true,"interval":PERIODIC_COLLAPSE_INTERVAL,"nextAt":next_at}),
    );
    Ok(())
}

fn revelation(state: &mut GameState) -> Result<()> {
    if state.extra.get("deathmatchEnabled") != Some(&Value::Bool(true)) {
        return Err(EngineError::IllegalAction);
    }
    state.extra.insert("deathmatchLimitTurns".into(), json!(5));
    let Some(deathmatch) = state.extra.get_mut("deathmatch") else {
        return Ok(());
    };
    if deathmatch.is_null() {
        return Ok(());
    }
    if !deathmatch.get("active").is_some_and(source_truthy) {
        return Ok(());
    }
    let fields = deathmatch
        .as_object_mut()
        .ok_or_else(|| EngineError::InvalidState("deathmatch must be an object".into()))?;
    // ensureDeathmatchState first normalizes to its previous interval and
    // clamps progress. revelation then sets the interval to five full turns
    // and clamps the progress again, without resetting other deathmatch data.
    let previous_interval = source_numeric(fields.get("intervalHalfTurns"), 0)?.unwrap_or(0.0);
    let previous_interval = if previous_interval == 0.0 {
        REVELATION_INTERVAL_HALF_TURNS as f64
    } else {
        previous_interval.max(1.0)
    };
    let progress = source_numeric(fields.get("halfTurnsSinceProgress"), 0)?
        .unwrap_or(0.0)
        .min(previous_interval)
        .max(0.0)
        .min(REVELATION_INTERVAL_HALF_TURNS as f64);
    fields.insert(
        "intervalHalfTurns".into(),
        json!(REVELATION_INTERVAL_HALF_TURNS),
    );
    fields.insert("halfTurnsSinceProgress".into(), numeric_value(progress));
    Ok(())
}

fn crown(state: &mut GameState) -> Result<()> {
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::InvalidState(
            "v7 crown board must be 8x8".into(),
        ));
    }
    let center = [
        Square { row: 3, col: 3 },
        Square { row: 3, col: 4 },
        Square { row: 4, col: 3 },
        Square { row: 4, col: 4 },
    ];
    let mut open = Vec::with_capacity(4);
    for square in center {
        if crate::movement::open_installation(state, square, state.turn)? {
            open.push(square);
        }
    }
    if open.is_empty() {
        open.extend(center);
    }
    for index in (1..open.len()).rev() {
        let draw = state.rng.sample()?;
        if !draw.is_finite() || !(0.0..1.0).contains(&draw) {
            return Err(EngineError::InvalidState(
                "crown random draw outside [0,1)".into(),
            ));
        }
        state
            .rng
            .record_last_probability(1.0 / (index + 1) as f64, "source Crown ground shuffle")?;
        open.swap(index, (draw * (index + 1) as f64).floor() as usize);
    }
    let chosen = open[0];
    super::v7_rule_board::crush_concealed_installation_occupant(state, chosen, state.turn)?;
    // The source replaces the rule on reactivation, then its common
    // reconciliation can retain a pre-existing crown bearer elsewhere.
    state.extra.insert(
        "crownRule".into(),
        json!({
            "id":"crown-1","crownGroupSize":1,"enabled":true,"holderId":"",
            "ground":{"row":chosen.row,"col":chosen.col},"removed":false,
            "pendingTransfer":null,"holdingColor":"","countUnit":"full-turn",
            "heldMoves":{"white":0,"black":0},"lastCountedMove":shared_turn_count(state)
        }),
    );
    crate::v7_board_automata::reconcile_crown_rule(state, true)?;
    Ok(())
}

fn source_truthy(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(string) => !string.is_empty(),
        Value::Array(_) | Value::Object(_) | Value::Bool(true) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, GameConfig, RngState};

    fn state() -> GameState {
        let initial: Value = serde_json::from_str(include_str!(
            "../../contracts/catalog/initial-state-20260928.json"
        ))
        .unwrap();
        let mut state: GameState = serde_json::from_value(initial["state"].clone()).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.rng = RngState::seeded(19);
        state
    }

    #[test]
    fn periodic_collapse_uses_next_strict_shared_turn_multiple() {
        let mut state = state();
        state.turns_taken.white = 40;
        state.turns_taken.black = 20;
        let before = state.rng.clone();
        apply(&mut state, "periodic-collapse").unwrap();
        assert_eq!(
            state.extra["periodicCollapse"],
            json!({
                "enabled":true,"interval":20,"nextAt":40
            })
        );
        assert_eq!(state.rng, before);
    }

    #[test]
    fn revelation_reduces_active_counter_without_resetting_progress() {
        let mut state = state();
        state.extra.insert("deathmatchEnabled".into(), json!(true));
        state.extra.insert("deathmatchLimitTurns".into(), json!(10));
        state.extra.insert(
            "deathmatch".into(),
            json!({
                "active":true,"intervalHalfTurns":20,"halfTurnsSinceProgress":18,
                "progressThisTurn":true
            }),
        );
        apply(&mut state, "revelation").unwrap();
        assert_eq!(state.extra["deathmatchLimitTurns"], json!(5));
        assert_eq!(state.extra["deathmatch"]["intervalHalfTurns"], json!(10));
        assert_eq!(
            state.extra["deathmatch"]["halfTurnsSinceProgress"],
            json!(10)
        );
        assert_eq!(state.extra["deathmatch"]["progressThisTurn"], json!(true));
    }

    #[test]
    fn disabled_revelation_and_unknown_rule_are_atomic() {
        let mut state = state();
        state.extra.insert("deathmatchEnabled".into(), json!(false));
        let before = state.clone();
        assert_eq!(
            apply(&mut state, "revelation"),
            Err(EngineError::IllegalAction)
        );
        assert_eq!(state, before);
        assert!(matches!(
            apply(&mut state, "other"),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn crown_shuffles_open_center_and_preserves_its_rng_cursor() {
        let mut state = state();
        state.turn = Color::White;
        let cursor = state.rng.cursor;
        apply(&mut state, "crown").unwrap();
        assert_eq!(state.rng.cursor, cursor + 3);
        let rule = &state.extra["crownRule"];
        assert_eq!(rule["id"], "crown-1");
        assert_eq!(rule["countUnit"], "full-turn");
        assert_eq!(rule["lastCountedMove"], 0);
        assert!(matches!(
            (
                rule["ground"]["row"].as_u64(),
                rule["ground"]["col"].as_u64()
            ),
            (Some(3 | 4), Some(3 | 4))
        ));
    }

    #[test]
    fn opening_rule_effects_match_frozen_source_seed_19() {
        // GameAdapter.newGame({draftDelete:true,ruleCardIds:[id]},19) from
        // verified main-OahWs0tU.js. This checks the full reset RNG offset,
        // RULE dispatch, direct effect and source-normalized crown object.
        for (id, cursor, rng_state) in [
            ("periodic-collapse", 35, 2_324_936_856),
            ("revelation", 35, 2_324_936_856),
            ("crown", 38, 3_394_590_561),
        ] {
            let state = crate::draft::initialize_for_ruleset(
                GameConfig {
                    draft_delete: true,
                    rule_card_ids: vec![id.into()],
                    ..GameConfig::default()
                },
                19,
                RULES_VERSION_V7,
            )
            .unwrap();
            assert_eq!(state.rng.cursor, cursor, "{id} RNG cursor");
            assert_eq!(state.rng.state, rng_state, "{id} RNG state");
            match id {
                "periodic-collapse" => assert_eq!(
                    state.extra["periodicCollapse"],
                    json!({"enabled":true,"interval":20,"nextAt":20})
                ),
                "revelation" => assert_eq!(state.extra["deathmatchLimitTurns"], 5),
                "crown" => assert_eq!(
                    state.extra["crownRule"],
                    json!({
                        "countUnit":"full-turn",
                        "crownGroupSize":1,
                        "enabled":true,
                        "ground":{"col":4,"row":4},
                        "heldMoves":{"black":0,"white":0},
                        "holderId":"",
                        "holdingColor":"",
                        "id":"crown-1",
                        "lastCountedMove":0,
                        "pendingTransfer":null,
                        "removed":false
                    })
                ),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn occupied_crown_center_converts_bearer_with_source_rng_order() {
        // In the pinned OracleRuntime, four white pawns moved from a2-d2
        // onto d5/e5/d4/e4 before enableCrownRule(). Seed 19 chooses d5,
        // consumes three shuffle draws, then one replacement-square and one
        // piece-ID draw. The latter is consumed despite retaining the pawn ID.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        for (to_row, to_col, from_col) in [(3, 3, 0), (3, 4, 1), (4, 3, 2), (4, 4, 3)] {
            state.board[to_row][to_col] = state.board[6][from_col].take();
        }
        apply(&mut state, "crown").unwrap();
        assert_eq!(state.rng.cursor, 37);
        assert_eq!(state.rng.state, 3_798_705_546);
        assert_eq!(state.extra["crownRule"]["ground"], Value::Null);
        assert_eq!(
            state.extra["crownRule"]["holderId"],
            "white-pawn-lm81oj7t4fq"
        );
        let bearer = state.board[3][3].as_ref().unwrap();
        assert_eq!(bearer.kind, "crown");
        assert_eq!(bearer.id, "white-pawn-lm81oj7t4fq");
        assert_eq!(bearer.extra["origin"], "d5");
        assert_eq!(bearer.extra["shielded"], false);
        assert_eq!(bearer.extra["crownTokenIds"], json!(["crown-1"]));
        assert_eq!(
            state.extra["logs"].as_array().unwrap().first(),
            Some(&json!("왕관: d5의 백 기물이 왕관으로 변했습니다."))
        );
    }

    #[test]
    fn crown_filters_piece_reservation_and_quantum_shadow_like_source() {
        // Pinned OracleRuntime seed 19: d5 holds a visible white pawn, e5
        // has a scarecrow reservation, and d4 is a pawn's quantum shadow.
        // Only e4 is open. shuffle([e4]) consumes no draw.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        state.board[3][3] = state.board[6][0].take();
        state
            .extra
            .insert("pendingScarecrows".into(), json!([{"row":3,"col":4}]));
        state.board[6][1]
            .as_mut()
            .unwrap()
            .extra
            .insert("quantum".into(), json!({"row":4,"col":3}));
        apply(&mut state, "crown").unwrap();
        assert_eq!(state.extra["crownRule"]["ground"], json!({"row":4,"col":4}));
        assert_eq!(state.rng.cursor, 32);
        assert_eq!(state.rng.state, 4_163_866_163);
    }

    #[test]
    fn concealed_crown_installation_removes_without_capture_side_effects() {
        let mut state = state();
        state.board[3][3] = state.board[1][0].take();
        state.board[3][3]
            .as_mut()
            .unwrap()
            .extra
            .insert("hiddenFrom".into(), json!("white"));
        state.extra.insert(
            "pendingScarecrows".into(),
            json!([
                {"row":3,"col":4},
                {"row":4,"col":3},
                {"row":4,"col":4}
            ]),
        );
        let id = state.board[3][3].as_ref().unwrap().id.clone();
        let captures = state.captures.clone();
        let cursor = state.rng.cursor;
        apply(&mut state, "crown").unwrap();
        assert!(
            state
                .board
                .iter()
                .flatten()
                .flatten()
                .all(|piece| piece.id != id)
        );
        assert_eq!(state.captures, captures);
        assert_eq!(state.rng.cursor, cursor);
        assert_eq!(state.extra["crownRule"]["ground"], json!({"row":3,"col":3}));
    }

    #[test]
    fn winter_normalization_keeps_source_coercion_and_field_order() {
        let normalized = normalize_winter(Some(&json!({
            "previewCycle":3.0,"previewIds":["a",5,"a","b","c","d","e","f","g"],
            "enabled":0,"lastCycle":"3.5","frozenIds":[0,false,null,"",true,3,["x",null],{},"z"],
            "disabledByLastWarmth":[]
        })))
        .unwrap();
        assert_eq!(
            normalized.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "previewCycle",
                "previewIds",
                "enabled",
                "lastCycle",
                "frozenIds",
                "disabledByLastWarmth"
            ]
        );
        assert_eq!(
            normalized["previewIds"],
            json!(["a", "b", "c", "d", "e", "f"])
        );
        assert_eq!(
            normalized["frozenIds"],
            json!(["true", "3", "x,", "[object Object]", "z"])
        );
        assert_eq!(normalized["lastCycle"], json!(3.5));
        assert_eq!(normalized["disabledByLastWarmth"], true);
    }

    #[test]
    fn winter_forecast_is_reused_without_rng_and_freezes_in_board_order() {
        let mut state = state();
        state
            .extra
            .insert("winterKingdom".into(), json!({"enabled":true}));
        state.turns_taken.white = 3;
        state.turns_taken.black = 2;
        let cursor = state.rng.cursor;
        assert!(!apply_winter_freeze_cycle(&mut state, false).unwrap());
        let preview = state.extra["winterKingdom"]["previewIds"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(preview.len(), 6);
        let white_count = winter_eligible_pieces(&state, Color::White).len();
        let black_count = winter_eligible_pieces(&state, Color::Black).len();
        assert_eq!(state.rng.cursor - cursor, white_count + black_count - 2);
        let expected = [Color::White, Color::Black]
            .into_iter()
            .flat_map(|color| winter_eligible_pieces(&state, color))
            .map(|(id, _)| json!(id))
            .filter(|id| preview.contains(id))
            .collect::<Vec<_>>();
        let cursor = state.rng.cursor;
        state.turns_taken.black = 3;
        assert!(apply_winter_freeze_cycle(&mut state, false).unwrap());
        assert_eq!(state.rng.cursor, cursor);
        assert_eq!(state.extra["winterKingdom"]["frozenIds"], json!(expected));
        assert!(state.extra["winterKingdom"].get("previewCycle").is_none());
        assert!(!apply_winter_freeze_cycle(&mut state, false).unwrap());
        assert_eq!(state.rng.cursor, cursor);
        assert!(apply_winter_freeze_cycle(&mut state, true).unwrap());
        assert_eq!(state.rng.cursor - cursor, white_count + black_count - 2);
    }

    #[test]
    fn winter_last_warmth_preserves_card_freeze_and_never_restarts() {
        let mut state = state();
        for (col, piece) in state.board[6].iter_mut().flatten().enumerate() {
            piece.extra.insert("frozen".into(), json!(true));
            if col == 0 {
                piece
                    .extra
                    .insert("frozenByCard".into(), json!({"remaining":"Infinity"}));
            }
        }
        for row in 0..8 {
            for col in 0..8 {
                if state.board[row][col]
                    .as_ref()
                    .is_some_and(|piece| piece.color == Color::Black)
                {
                    state.board[row][col] = None;
                }
            }
        }
        state.extra.insert(
            "winterKingdom".into(),
            json!({"enabled":true,"previewCycle":3,"previewIds":["missing"]}),
        );
        state.turns_taken.white = 3;
        state.turns_taken.black = 3;
        let cursor = state.rng.cursor;
        assert!(!apply_winter_freeze_cycle(&mut state, false).unwrap());
        assert_eq!(state.extra["winterKingdom"]["disabledByLastWarmth"], true);
        assert!(state.extra["winterKingdom"].get("previewIds").is_none());
        assert!(
            state.board[6][0]
                .as_ref()
                .unwrap()
                .extra
                .get("frozen")
                .is_some()
        );
        assert!(
            state.board[6][1]
                .as_ref()
                .unwrap()
                .extra
                .get("frozen")
                .is_none()
        );
        assert_eq!(state.rng.cursor, cursor);
        for col in 0..5 {
            state.board[1][col] = Some(crate::Piece::new(
                "pawn",
                Color::Black,
                format!("restored-{col}"),
            ));
        }
        assert!(!apply_winter_freeze_cycle(&mut state, true).unwrap());
        assert_eq!(state.rng.cursor, cursor);
    }

    #[test]
    fn winter_invalid_draw_is_atomic_after_partial_forecast() {
        let mut state = state();
        state
            .extra
            .insert("winterKingdom".into(), json!({"enabled":true}));
        state.turns_taken.white = 3;
        state.turns_taken.black = 2;
        state.rng.tape = vec![0.5, 1.0];
        state.rng.cursor = 0;
        let before = state.clone();
        assert!(matches!(
            apply_winter_freeze_cycle(&mut state, false),
            Err(EngineError::InvalidState(_))
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn revelation_uses_truthy_active_and_number_coercion() {
        let mut state = state();
        state.extra.insert("deathmatchEnabled".into(), json!(true));
        state.extra.insert(
            "deathmatch".into(),
            json!({"active":"active","intervalHalfTurns":["2.5"],
            "halfTurnsSinceProgress":"9","progressThisTurn":false}),
        );
        apply(&mut state, "revelation").unwrap();
        assert_eq!(state.extra["deathmatch"]["intervalHalfTurns"], 10);
        assert_eq!(
            state.extra["deathmatch"]["halfTurnsSinceProgress"],
            json!(2.5)
        );
        assert_eq!(state.extra["deathmatch"]["active"], "active");
    }

    #[test]
    #[ignore = "requires the frozen source full-position RULE environment receipt"]
    fn frozen_rule_environment_when_receipt_is_supplied() {
        use crate::tests::source_callback_fixture::{
            collect_case_diagnostics, compare_callback_envelope, compare_value,
            source_callback_state,
        };
        let path = std::env::var_os("ACCELERATE_V7_RULE_ENVIRONMENT_CASES")
            .expect("set ACCELERATE_V7_RULE_ENVIRONMENT_CASES to the source receipt JSONL");
        let text = std::fs::read_to_string(path).expect("source RULE environment receipt");
        let cases = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<Value>(line).expect("source RULE environment JSON"))
            .collect::<Vec<_>>();
        let expected_names = [
            "winter-disabled-coercion",
            "winter-gameover-normalizes-without-thaw",
            "winter-preview-white-third",
            "winter-first-complete-cycle",
            "winter-idempotent-complete-cycle",
            "winter-forced-complete-cycle",
            "winter-preview-reuse-no-draw",
            "winter-preview-missing-id-no-refill",
            "winter-last-warmth-threshold-four",
            "winter-disabled-remains-disabled-with-population",
            "winter-card-freeze-string-coercion",
            "winter-legacy-catalog-no-forecast",
            "winter-stale-forecast-replaced",
            "winter-large-identity-once",
            "revelation-truthy-active-fraction",
            "revelation-negative-interval",
            "revelation-nan-progress",
            "revelation-infinite-intermediates",
            "revelation-inactive-preserved",
            "revelation-disabled",
            "periodic-collapse-reactivation",
            "football-concealed-enemy-removal",
            "football-camouflage-occupant",
            "monster-concealed-enemy-removal",
            "monster-camouflage-occupant",
            "crown-concealed-enemy-removal",
            "crown-camouflage-occupant",
            "football-concealed-royal",
            "monster-concealed-removal-vigilance-deathmatch",
            "crown-reactivation-retains-existing-bearer",
            "crown-existing-ground-excluded",
            "capture-the-flag-prior-false",
            "capture-the-flag-prior-0",
            "capture-the-flag-prior-\"\"",
            "capture-the-flag-prior-{}",
            "platform-reactivation-replaces-forecast",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        let actual_names = cases
            .iter()
            .map(|receipt| {
                receipt["case"]
                    .as_str()
                    .expect("source RULE environment case name")
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(cases.len(), 36, "source RULE environment input count");
        assert_eq!(
            actual_names, expected_names,
            "complete source RULE environment input set"
        );

        let mut mismatches = Vec::new();
        for receipt in &cases {
            let name = receipt["case"]
                .as_str()
                .expect("source RULE environment case name");
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                assert_eq!(
                    receipt["schemaVersion"], 1,
                    "source RULE environment receipt schema"
                );
                assert_eq!(
                    receipt["sourceSha256"],
                    "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
                );
                let mut state = source_callback_state(&receipt["before"])?;
                let result = match receipt["operation"].as_str().expect("source operation") {
                    "winter" => apply_winter_freeze_cycle(
                        &mut state,
                        receipt["force"]
                            .as_bool()
                            .expect("source winter force boolean"),
                    )
                    .map(|frozen| json!(frozen)),
                    "timed" => apply(
                        &mut state,
                        receipt["cardId"].as_str().expect("source timed card ID"),
                    )
                    .map(|_| json!(true)),
                    "board" => super::super::v7_rule_board::apply(
                        &mut state,
                        receipt["cardId"].as_str().expect("source board card ID"),
                    )
                    .map(|_| json!(true)),
                    "spatial" => super::super::v7_rule_spatial::apply(
                        &mut state,
                        receipt["cardId"].as_str().expect("source spatial card ID"),
                    )
                    .map(|_| json!(true)),
                    other => {
                        return Err(EngineError::InvalidState(format!(
                            "unknown source RULE environment operation {other}"
                        )));
                    }
                };
                let actual_result = match result {
                    Ok(value) => value,
                    Err(EngineError::IllegalAction) => json!(false),
                    Err(error) => {
                        mismatches.push(format!("{name}.result: native callback error: {error}"));
                        Value::Null
                    }
                };
                compare_value(
                    &receipt["result"],
                    &actual_result,
                    &format!("{name}.result"),
                    mismatches,
                )?;
                // The source snapshot drains the endGame microtask after all
                // installation effects. Recording before this point would
                // omit the installed piece from the terminal replay frame.
                crate::replay::settle(&mut state)?;
                compare_callback_envelope(&state, &receipt["after"], name, mismatches)
            });
        }
        assert!(
            mismatches.is_empty(),
            "{} source RULE environment differences across {} cases:\n{}",
            mismatches.len(),
            cases.len(),
            mismatches.join("\n")
        );
    }
}
