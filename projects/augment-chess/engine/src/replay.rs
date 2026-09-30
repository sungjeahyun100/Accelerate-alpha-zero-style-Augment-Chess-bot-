//! Source replay, notation, and move rollback state. Replay deltas are rule
//! state: replayMove validates and restores them, so they cannot be discarded
//! as presentation metadata. All identifiers share the gameplay RNG stream.
use crate::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, OnceLock},
};

/// Source activeMoveReplayCapture: 시작 색과 before snapshot은 함께 복원한다.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MoveReplayCapture {
    pub actor: Color,
    pub before: Box<GameState>,
}

/// Source main85539/85637/85839는 state/depth를 복원하지만 전역
/// activeMoveReplayCapture는 복원하지 않는다. 임시 clone을 폐기하거나
/// 거절한 실행에서도 실제 begin/commit/cancel 명령의 마지막 결과를 보존한다.
/// 전역·thread-local 저장소를 쓰지 않고 한 probe의 자식 clone만 공유한다.
#[derive(Clone, Debug)]
pub(crate) struct ReplayCaptureScope(Arc<Mutex<ReplayCaptureJournal>>);

#[derive(Debug)]
struct ReplayCaptureJournal {
    pending: Option<MoveReplayCapture>,
    commands: u64,
}

impl PartialEq for ReplayCaptureScope {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl ReplayCaptureScope {
    pub(crate) fn for_probe(state: &GameState) -> Self {
        state.move_replay_scope.clone().unwrap_or_else(|| {
            Self(Arc::new(Mutex::new(ReplayCaptureJournal {
                pending: state.active_move_replay_before.clone(),
                commands: 0,
            })))
        })
    }

    pub(crate) fn attach(&self, state: &mut GameState) {
        state.move_replay_scope = Some(self.clone());
    }

    pub(crate) fn carry_into(&self, state: &mut GameState) -> Result<()> {
        state.active_move_replay_before = self.pending()?;
        Ok(())
    }

    fn pending(&self) -> Result<Option<MoveReplayCapture>> {
        self.0
            .lock()
            .map(|journal| journal.pending.clone())
            .map_err(|_| {
                EngineError::InvalidState("v7 replay capture scope mutex is poisoned".into())
            })
    }

    fn replace(&self, pending: Option<MoveReplayCapture>) -> Result<()> {
        let mut journal = self.0.lock().map_err(|_| {
            EngineError::InvalidState("v7 replay capture scope mutex is poisoned".into())
        })?;
        let commands = journal.commands.checked_add(1).ok_or_else(|| {
            EngineError::InvalidState("v7 replay capture scope command counter overflow".into())
        })?;
        journal.pending = pending;
        journal.commands = commands;
        Ok(())
    }
}

/// scope가 있는 clone의 로컬 mirror는 이전 명령의 값일 수 있으므로
/// 항상 journal의 실제 최신 capture를 읽는다.
pub(crate) fn active_move_capture(state: &GameState) -> Result<Option<MoveReplayCapture>> {
    match &state.move_replay_scope {
        Some(scope) => scope.pending(),
        None => Ok(state.active_move_replay_before.clone()),
    }
}

pub(crate) fn replace_active_move_capture(
    state: &mut GameState,
    mut capture: Option<MoveReplayCapture>,
) -> Result<()> {
    if let Some(capture) = capture.as_mut() {
        // source capture는 before snapshot을 보관한다. journal 자신을 snapshot에
        // 넣으면 순환 소유권과 다른 실행에 대한 공유 문맥이 생긴다.
        capture.before.active_move_replay_before = None;
        capture.before.move_replay_scope = None;
    }
    if let Some(scope) = &state.move_replay_scope {
        scope.replace(capture.clone())?;
    }
    state.active_move_replay_before = capture;
    Ok(())
}

fn metadata() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(SOURCE_METADATA).expect("frozen replay constants"))
}
fn value(state: &GameState, key: &str, fallback: Value) -> Value {
    state
        .extra
        .get(key)
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or(fallback)
}

/// main:88915-88966의 historical replayFrameAt 조회. 순수 frame을 반환하며
/// currentHistoryEntry의 visual 장식이나 live turn/mode는 합치지 않는다.
/// fresh source restore의 base/tail 캐시와 같은 가까운 방향으로 delta를 적용한다.
pub(crate) fn v7_history_frame(state: &GameState) -> Result<Option<Value>> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::InvalidState(
            "historical v7 frame requires v7 rules".into(),
        ));
    }
    let Some(index) = state
        .extra
        .get("historyViewIndex")
        .and_then(Value::as_f64)
        .filter(|index| index.is_finite() && index.fract() == 0.0 && *index >= 0.0)
    else {
        return Ok(None);
    };
    let Some(base) = state
        .extra
        .get("replayBaseFrame")
        .filter(|base| crate::observation::truth(Some(base)))
    else {
        return Ok(None);
    };
    let events = state
        .extra
        .get("replayEvents")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineError::InvalidState("historical replayEvents must be an array".into())
        })?;
    // isViewingHistory excludes the latest frame; fractional and string indices are not selections.
    if index >= events.len() as f64 {
        return Ok(None);
    }
    let target = index as usize;
    if target <= events.len() - target {
        let mut frame = base.clone();
        for event in &events[..target] {
            frame = apply_v7_replay_delta(frame, event.get("delta"), false)?;
        }
        return Ok(Some(frame));
    }
    let mut frame = if let Some(tail) = state
        .extra
        .get("replayTailFrame")
        .filter(|tail| crate::observation::truth(Some(tail)))
    {
        tail.clone()
    } else {
        let mut tail = base.clone();
        for event in events {
            tail = apply_v7_replay_delta(tail, event.get("delta"), false)?;
        }
        tail
    };
    for event in events[target..].iter().rev() {
        frame = apply_v7_replay_delta(frame, event.get("delta"), true)?;
    }
    Ok(Some(frame))
}

fn apply_v7_replay_delta(mut frame: Value, delta: Option<&Value>, reverse: bool) -> Result<Value> {
    let output = frame.as_object_mut().ok_or_else(|| {
        EngineError::InvalidState("historical replay frame must be an object".into())
    })?;
    let side = if reverse { "before" } else { "after" };
    let exists_side = if reverse {
        "beforeExists"
    } else {
        "afterExists"
    };
    let board_delta = delta.and_then(|delta| delta.get("board"));
    let current = output.get("board").and_then(Value::as_array);
    let dimension = |key: &str, fallback: usize| -> Result<usize> {
        let Some(shape) = board_delta.and_then(|board| board.get(side)) else {
            return Ok(fallback);
        };
        let number = crate::card_effects::js_number(shape.get(key), 0)
            .unwrap_or(0.0)
            .max(0.0);
        if !number.is_finite() || number > 8.0 {
            return Err(EngineError::InvalidState(format!(
                "historical v7 replay {key} is outside the 8x8 profile"
            )));
        }
        Ok(number.floor() as usize)
    };
    let rows = dimension("rows", current.map_or(0, Vec::len))?;
    let cols = dimension(
        "cols",
        current
            .and_then(|rows| rows.first())
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
    )?;
    let mut board = (0..rows)
        .map(|row| {
            (0..cols)
                .map(|col| {
                    current
                        .and_then(|rows| rows.get(row))
                        .and_then(Value::as_array)
                        .and_then(|cells| cells.get(col))
                        .cloned()
                        .unwrap_or(Value::Null)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    if let Some(cells) = board_delta
        .and_then(|board| board.get("cells"))
        .and_then(Value::as_array)
    {
        for cell in cells {
            let row = crate::card_effects::js_number(cell.get("row"), 0);
            let col = crate::card_effects::js_number(cell.get("col"), 0);
            if let (Some(row), Some(col)) = (row, col)
                && row.is_finite()
                && col.is_finite()
                && row.fract() == 0.0
                && col.fract() == 0.0
                && row >= 0.0
                && col >= 0.0
                && row < rows as f64
                && col < cols as f64
            {
                board[row as usize][col as usize] = cell.get(side).cloned().unwrap_or(Value::Null);
            }
        }
    }
    output.insert("board".into(), json!(board));
    if let Some(fields) = delta
        .and_then(|delta| delta.get("fields"))
        .and_then(Value::as_array)
    {
        for field in fields {
            let Some(key) = field
                .get("key")
                .and_then(Value::as_str)
                .filter(|key| !key.is_empty() && *key != "board")
            else {
                continue;
            };
            if field.get(exists_side) == Some(&Value::Bool(false)) {
                output.shift_remove(key);
            } else if let Some(value) = field.get(side) {
                output.insert(key.into(), value.clone());
            } else {
                output.shift_remove(key);
            }
        }
    }
    Ok(frame)
}

/// applyWinterFreezeCycle runs during endMove, including when Winter Kingdom
/// is disabled. It reconstructs this object; replay uses JSON.stringify, so
/// property order matters even when all values are unchanged.
pub(crate) fn normalize_winter_after_turn(state: &mut GameState) -> Result<()> {
    if state.ruleset_id == RULES_VERSION_V7 {
        crate::card_effects::v7_rule_timed::apply_winter_freeze_cycle(state, false)?;
        return Ok(());
    }
    let empty = serde_json::Map::new();
    let original = state
        .extra
        .get("winterKingdom")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    if crate::observation::truth(original.get("enabled")) {
        return Err(EngineError::UnsupportedFeature(
            "winter freeze cycle".into(),
        ));
    }
    let mut winter = serde_json::Map::new();
    if let Some(cycle) = original
        .get("previewCycle")
        .filter(|v| v.as_i64().is_some() || v.as_u64().is_some())
    {
        winter.insert("previewCycle".into(), cycle.clone());
        let ids = original.get("previewIds").and_then(Value::as_array);
        let mut seen = BTreeSet::new();
        let ids = ids
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|id| seen.insert((*id).to_owned()))
            .take(6)
            .collect::<Vec<_>>();
        winter.insert("previewIds".into(), json!(ids));
    }
    winter.insert(
        "enabled".into(),
        json!(crate::observation::truth(original.get("enabled"))),
    );
    let cycle = crate::observation::number(original.get("lastCycle"))
        .filter(|n| *n != 0.0)
        .unwrap_or(0.0);
    winter.insert("lastCycle".into(), json!(cycle));
    let ids = original.get("frozenIds").and_then(Value::as_array);
    let ids = ids
        .into_iter()
        .flatten()
        .filter(|id| crate::observation::truth(Some(id)))
        .take(12)
        .map(|id| {
            id.as_str().map(str::to_owned).ok_or_else(|| {
                EngineError::UnsupportedFeature(
                    "winter frozenIds non-string source coercion".into(),
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    winter.insert("frozenIds".into(), json!(ids));
    winter.insert(
        "disabledByLastWarmth".into(),
        json!(crate::observation::truth(
            original.get("disabledByLastWarmth")
        )),
    );
    state
        .extra
        .insert("winterKingdom".into(), Value::Object(winter));
    Ok(())
}
fn canonical_equal(first: &Value, second: &Value) -> Result<bool> {
    // Source valuesEqual uses JSON.stringify, including object insertion order.
    // Numeric spelling still follows JavaScript, where 0.0 and 0 stringify alike.
    fn stringify(value: &Value) -> Result<String> {
        match value {
            Value::Object(map) => {
                let mut fields = Vec::new();
                for (key, value) in map {
                    fields.push(format!(
                        "{}:{}",
                        serde_json::to_string(key).map_err(EngineError::serialization)?,
                        stringify(value)?
                    ));
                }
                Ok(format!("{{{}}}", fields.join(",")))
            }
            Value::Array(list) => Ok(format!(
                "[{}]",
                list.iter()
                    .map(stringify)
                    .collect::<Result<Vec<_>>>()?
                    .join(",")
            )),
            _ => String::from_utf8(serde_jcs::to_vec(value).map_err(EngineError::serialization)?)
                .map_err(|error| EngineError::InvalidState(error.to_string())),
        }
    }
    Ok(stringify(first)? == stringify(second)?)
}
fn clone_replay(value: &Value) -> Value {
    match value {
        Value::Object(map) if map.get("__simType").and_then(Value::as_str) == Some("Set") => {
            clone_replay(&map["values"])
        }
        Value::Object(map) if map.get("__simType").and_then(Value::as_str) == Some("Map") => {
            clone_replay(&map["entries"])
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), clone_replay(v)))
                .collect(),
        ),
        Value::Array(list) => Value::Array(list.iter().map(clone_replay).collect()),
        _ => value.clone(),
    }
}

/// Each immutable source Position passes through contract.position, whose JCS
/// copy sorts nested object keys. The next action restores that copy before
/// createReplayDelta compares frames with JSON.stringify (key-order sensitive).
/// The live state and frames share this boundary. Sorting only replay frames
/// would invent order-only democracy/hillKing deltas on the next move. Replay
/// delta arrays and the JSON.stringify comparison inside a turn stay intact.
pub(crate) fn canonicalize_position_frames(state: &mut GameState) -> Result<()> {
    if state.ruleset_id == RULES_VERSION_V7 {
        let bytes = serde_jcs::to_vec(state).map_err(EngineError::serialization)?;
        let normalized: GameState =
            serde_json::from_slice(&bytes).map_err(EngineError::serialization)?;
        // Copy serialized containers only: execution-only captures, scopes,
        // cancellation state and chance likelihood remain owned by the caller.
        state.board = normalized.board;
        state.turns_taken = normalized.turns_taken;
        state.cards_used_this_turn = normalized.cards_used_this_turn;
        state.deck_slots = normalized.deck_slots;
        state.captures = normalized.captures;
        state.history = normalized.history;
        state.extra = normalized.extra;
        return Ok(());
    }
    for key in ["replayBaseFrame", "replayTailFrame"] {
        if let Some(frame) = state.extra.get_mut(key) {
            let bytes = serde_jcs::to_vec(frame).map_err(EngineError::serialization)?;
            *frame = serde_json::from_slice(&bytes).map_err(EngineError::serialization)?;
        }
    }
    Ok(())
}
pub(crate) fn normalize_color_booleans(state: &mut GameState, key: &str) {
    let current = state.extra.get(key);
    let truthy = |color: &str| {
        current.and_then(|v| v.get(color)).is_some_and(|v| match v {
            Value::Null => false,
            Value::Bool(v) => *v,
            Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
            Value::String(v) => !v.is_empty(),
            _ => true,
        })
    };
    let map = json!({"white":truthy("white"),"black":truthy("black")});
    state.extra.insert(key.into(), map);
}
pub(crate) fn track_moving(state: &mut GameState, piece: &Piece) -> Result<()> {
    track_moving_with_exhaustion(state, piece, true)
}

pub(crate) fn track_moving_with_exhaustion(
    state: &mut GameState,
    piece: &Piece,
    count_exhaustion: bool,
) -> Result<()> {
    let Some(actor) = piece.color.owner() else {
        return Ok(());
    };
    if count_exhaustion {
        // main846/100695: another allied move releases every alias of the bond.
        // The moving object's own binding is preserved by the source helper.
        for candidate in state.board.iter_mut().flatten().flatten() {
            if candidate.color == piece.color
                && candidate.id != piece.id
                && crate::observation::truth(candidate.extra.get("grapplerBound"))
            {
                candidate.extra.shift_remove("grapplerBound");
            }
        }
        let current = value(state, "exhaustion", json!({}));
        let entry = |color: Color| json!({"enabled":current[color.as_str()]["enabled"].as_bool().unwrap_or(false),"pieceId":current[color.as_str()]["pieceId"].as_str().unwrap_or(""),"count":current[color.as_str()]["count"].as_u64().unwrap_or(0)});
        let mut exhaustion = json!({"white":entry(Color::White),"black":entry(Color::Black)});
        if exhaustion[actor.as_str()]["enabled"] == json!(true) {
            let slot = &mut exhaustion[actor.as_str()];
            let count = if slot["pieceId"] == json!(piece.id) {
                slot["count"].as_u64().unwrap_or(0) + 1
            } else {
                1
            };
            slot["pieceId"] = json!(piece.id);
            slot["count"] = json!(count);
        }
        state.extra.insert("exhaustion".into(), exhaustion);
    }
    if matches!(piece.kind.as_str(), "wall" | "football" | "blackHole") {
        return Ok(());
    }
    let moving=state.extra.entry("moving").or_insert_with(||json!({"white":{"enabled":false,"pieceId":"","count":0},"black":{"enabled":false,"pieceId":"","count":0}}));
    let old = &moving[actor.as_str()];
    let enabled = old["enabled"].as_bool().unwrap_or(false);
    let count = if enabled {
        if old["pieceId"] == json!(piece.id) {
            old["count"].as_u64().unwrap_or(0) + 1
        } else {
            1
        }
    } else {
        old["count"].as_u64().unwrap_or(0)
    };
    let id = if enabled {
        piece.id.as_str()
    } else {
        old["pieceId"].as_str().unwrap_or("")
    };
    moving[actor.as_str()] = json!({"enabled":enabled,"pieceId":if count>=5&&enabled{""}else{id},"count":if count>=5&&enabled{0}else{count}});
    if count >= 5 && enabled {
        let gained_evasion = !crate::observation::truth(piece.extra.get("evasion"));
        for candidate in state
            .board
            .iter_mut()
            .flatten()
            .flatten()
            .filter(|p| p.id == piece.id)
        {
            candidate.extra.insert("evasion".into(), json!(true));
        }
        if gained_evasion {
            let name = if state.ruleset_id == RULES_VERSION_V7 {
                source_piece_label(&piece.kind)
                    .filter(|name| !name.is_empty())
                    .unwrap_or(&piece.kind)
            } else {
                piece_label(&piece.kind)
            };
            add_piece_action_log(
                state,
                piece,
                None,
                None,
                format!("무빙: {name}이 5번 연속 이동해 회피를 얻었습니다."),
            )?;
        }
    }
    Ok(())
}
pub(crate) fn queue_visual(state: &mut GameState, visual: Value) -> Result<()> {
    // main88961: threat depth는 이 queue의 억제 조건이 아니다.
    if state.ruleset_id == RULES_VERSION_V7 && state.ai_simulation_depth > 0 {
        return Ok(());
    }
    let visual = if state.ruleset_id == RULES_VERSION_V7 {
        let Some(visual) = normalize_visual_v7(&visual)? else {
            return Ok(());
        };
        visual
    } else {
        visual
    };
    let list = state
        .extra
        .entry("pendingReplayVisuals")
        .or_insert_with(|| json!([]));
    list.as_array_mut()
        .ok_or_else(|| EngineError::InvalidState("pending replay visuals must be an array".into()))?
        .push(visual);
    Ok(())
}

/// main45780 / main88965: 직접 생성한 효과도 원문의 replay 정규화 경계를 지난다.
fn normalize_visual_v7(value: &Value) -> Result<Option<Value>> {
    if !value.is_object() && !value.is_array() {
        return Ok(None);
    }
    let mut visual = clone_replay(value);
    let Some(object) = visual.as_object_mut() else {
        return Ok(Some(visual));
    };
    const TYPES: [&str; 20] = [
        "move",
        "card",
        "trolley",
        "random-roulette",
        "time-is-mine",
        "mistake",
        "parry",
        "evasion",
        "shield-break",
        "merchant-buy",
        "grappler-pull",
        "don-quixote-route",
        "collapse",
        "collapse-warning",
        "monster-move",
        "board-change",
        "rule-bomb-sweep",
        "reaper-execution",
        "magic",
        "effect",
    ];
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .filter(|kind| TYPES.contains(kind))
        .unwrap_or("effect");
    object.insert("type".into(), json!(kind));
    if !matches!(
        object.get("color").and_then(Value::as_str),
        Some("white" | "black")
    ) {
        object.shift_remove("color");
    }
    if kind == "card" {
        if let Some(animation) = object.get_mut("animation").and_then(Value::as_object_mut) {
            animation.shift_remove("html");
        }
    }
    if let Some(cells) = object.get("cells").and_then(Value::as_array) {
        let cells = cells
            .iter()
            .filter_map(|cell| {
                let row = crate::card_effects::js_number(cell.get("row"), 0)?;
                let col = crate::card_effects::js_number(cell.get("col"), 0)?;
                (row.is_finite()
                    && col.is_finite()
                    && row.fract() == 0.0
                    && col.fract() == 0.0
                    && row >= 0.0
                    && col >= 0.0)
                    .then(|| json!({"row":row,"col":col}))
            })
            .collect::<Vec<_>>();
        object.insert("cells".into(), json!(cells));
    }
    Ok(Some(visual))
}
pub(crate) fn add_log(state: &mut GameState, text: String) -> Result<()> {
    if !state.extra.contains_key("logs") {
        return Ok(());
    }
    let list = state
        .extra
        .get_mut("logs")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState("logs must be an array".into()))?;
    list.insert(0, json!(text));
    Ok(())
}

/// The source action log conceals an event from the opponent if its origin
/// was hidden or its resulting square is hidden. Missing identities are also
/// concealed; absence does not prove that an event was public.
pub(crate) fn add_piece_action_log(
    state: &mut GameState,
    piece: &Piece,
    square: Option<Square>,
    privacy: Option<&Value>,
    message: String,
) -> Result<()> {
    let concealed = if let Some(owner) = piece.color.owner() {
        let viewer = owner.opponent();
        let square = square.or_else(|| {
            (0..8)
                .flat_map(|row| (0..8).map(move |col| Square { row, col }))
                .find(|&square| state.at(square).is_some_and(|item| item.id == piece.id))
        });
        if let Some(square) = square {
            let origin_hidden = privacy
                .and_then(|v| v.get(viewer.as_str()))
                .and_then(|v| v.get("originVisible"))
                == Some(&json!(false));
            let current_visible = if state.ruleset_id == RULES_VERSION_V7 {
                crate::observation::piece_visible_to_color_at_v7(state, piece, square, viewer)?
            } else {
                state.piece_visible(piece, square, viewer)
            };
            origin_hidden || !current_visible
        } else {
            true
        }
    } else {
        false
    };
    add_log(
        state,
        if concealed {
            "기물이 행동했습니다.".into()
        } else {
            message
        },
    )
}
pub(crate) fn label(color: Color) -> &'static str {
    match color {
        Color::White => "백",
        Color::Black => "흑",
    }
}
fn trim(text: &str, limit: usize) -> String {
    text.trim().chars().take(limit).collect()
}
pub(crate) fn queue_notation(
    state: &mut GameState,
    kind: &str,
    color: Color,
    text: String,
    description: String,
    move_number: u64,
) -> Result<Value> {
    // main88225: queueHistoryNotation은 AI simulation만 억제한다.
    if state.ruleset_id == RULES_VERSION_V7 && state.ai_simulation_depth > 0 {
        return Ok(Value::Null);
    }
    let suffix = crate::draft::random_suffix(state.rng.sample_opaque("notation identity")?)?;
    let id = format!(
        "{kind}-{}-{}",
        crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?,
        suffix.chars().take(7).collect::<String>()
    );
    let text = trim(&text, 96);
    if text.is_empty() {
        return Ok(Value::Null);
    }
    let event = json!({"id":id,"kind":kind,"color":color,"text":text,"description":trim(&description,180),"moveNumber":move_number});
    let list = state
        .extra
        .entry("pendingNotations")
        .or_insert_with(|| json!([]));
    let list = list
        .as_array_mut()
        .ok_or_else(|| EngineError::InvalidState("pending notations must be an array".into()))?;
    if !list.iter().any(|v| v["id"] == event["id"]) {
        list.push(event.clone());
    }
    state.extra.insert("pendingNotation".into(), event.clone());
    Ok(event)
}

/// 원문의 환경 사신 처형은 sibling 이동 후보를 검사하지 않고 RPx…#를 기록한다.
pub(crate) fn queue_reaper_execution_notation(
    state: &mut GameState,
    reaper: &Piece,
    from: Square,
    to: Square,
) -> Result<()> {
    let color = reaper.color.owner().ok_or(EngineError::WrongActor)?;
    let move_number = state
        .extra
        .get("activeHistoryMoveNumber")
        .and_then(Value::as_u64)
        .filter(|number| *number != 0)
        .unwrap_or(u64::from(state.full_move));
    queue_notation(
        state,
        "move",
        color,
        format!("RPx{}#", square_name(to)),
        format!(
            "{} 사신 {}에서 {} 포획",
            label(color),
            square_name(from),
            square_name(to)
        ),
        move_number,
    )?;
    Ok(())
}

pub(crate) fn amend_pending_promotion_notation(state: &mut GameState, kind: &str) -> Result<()> {
    amend_pending_piece_change_notation(state, kind, "프로모션")
}

pub(crate) fn amend_pending_piece_change_notation(
    state: &mut GameState,
    kind: &str,
    change_label: &str,
) -> Result<()> {
    let pending = state
        .extra
        .get("pendingNotations")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .rev()
                .find(|entry| entry.get("kind").and_then(Value::as_str) == Some("move"))
        })
        .or_else(|| {
            state
                .extra
                .get("pendingNotation")
                .filter(|entry| !entry.is_null())
        })
        .cloned();
    let Some(mut amended) = pending else {
        return Ok(());
    };
    if amended.get("kind").and_then(Value::as_str) != Some("move") {
        return Ok(());
    }
    let text = amended.get("text").and_then(Value::as_str).ok_or_else(|| {
        EngineError::InvalidState("promotion notation text is not a string".into())
    })?;
    let text = trim(text, 96);
    let (base, ending) = text
        .strip_suffix('#')
        .map(|text| (text, "#"))
        .unwrap_or((&text, ""));
    let code = piece_code(&state.ruleset_id, kind);
    let description = amended
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or(&text);
    let description = format!("{} {code} {change_label}", trim(description, 180));
    amended["text"] = json!(format!("{base}={code}{ending}"));
    amended["description"] = json!(description.trim());
    let id = amended.get("id").cloned();
    if id.is_some()
        && state
            .extra
            .get("pendingNotation")
            .and_then(|entry| entry.get("id"))
            == id.as_ref()
    {
        state
            .extra
            .insert("pendingNotation".into(), amended.clone());
    }
    if let Some(stored) = state
        .extra
        .get_mut("pendingNotations")
        .and_then(Value::as_array_mut)
        .and_then(|entries| {
            entries.iter_mut().find(|entry| {
                entry
                    .get("id")
                    .filter(|id| crate::observation::truth(Some(id)))
                    == id.as_ref()
            })
        })
    {
        *stored = amended;
    }
    Ok(())
}

/// Source `queueSpecialHistoryNotation(color, "effect", "effect", {name},
/// description)` formats `!name`, then creates one notation ID from the
/// gameplay RNG. The active history move number wins over `fullMove`.
pub(crate) fn queue_special_effect_notation(
    state: &mut GameState,
    color: Color,
    name: &str,
    description: &str,
) -> Result<Value> {
    queue_special_history_notation(state, color, "effect", name, description)
}

pub(crate) fn queue_special_history_notation(
    state: &mut GameState,
    color: Color,
    kind: &str,
    name: &str,
    description: &str,
) -> Result<Value> {
    if !matches!(kind, "special" | "effect" | "move") {
        return Err(EngineError::InvalidState(
            "unknown source special notation kind".into(),
        ));
    }
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 special effect notation outside the pinned ruleset".into(),
        ));
    }
    let move_number = crate::observation::number(state.extra.get("activeHistoryMoveNumber"))
        .or_else(|| {
            let full_move = f64::from(state.full_move);
            (full_move != 0.0).then_some(full_move)
        })
        .unwrap_or(1.0)
        .max(0.0)
        .floor();
    if move_number > u64::MAX as f64 {
        return Err(EngineError::InvalidState(
            "v7 special effect move number exceeds supported range".into(),
        ));
    }
    queue_notation(
        state,
        kind,
        color,
        format!("!{}", trim(name, 60)),
        description.to_owned(),
        move_number as u64,
    )
}

pub(crate) fn attach_notation_redactions(
    state: &mut GameState,
    mut event: Value,
    redactions: serde_json::Map<String, Value>,
) -> Result<Value> {
    if event.is_null() || redactions.is_empty() {
        return Ok(event);
    }
    event["redactions"] = Value::Object(redactions);
    if let Some(list) = state
        .extra
        .get_mut("pendingNotations")
        .and_then(Value::as_array_mut)
        && let Some(stored) = list.iter_mut().find(|entry| entry["id"] == event["id"])
    {
        *stored = event.clone();
    }
    state.extra.insert("pendingNotation".into(), event.clone());
    Ok(event)
}

/// 원문 moveNotationRedactions의 현재 목적지·이전 typeKnown·목격 포획 셀.
pub(crate) fn queue_special_move_notation(
    state: &mut GameState,
    piece: &Piece,
    at: Square,
    text: String,
    description: String,
    privacy: &Value,
    capture: bool,
    capture_known_squares: &Value,
) -> Result<()> {
    let color = piece.color.owner().ok_or(EngineError::WrongActor)?;
    queue_source_move_notation(
        state,
        piece,
        at,
        color,
        &piece.kind,
        text,
        description,
        privacy,
        capture,
        capture_known_squares,
    )
}

fn queue_source_move_notation(
    state: &mut GameState,
    piece: &Piece,
    at: Square,
    color: Color,
    piece_type: &str,
    text: String,
    description: String,
    privacy: &Value,
    capture: bool,
    capture_known_squares: &Value,
) -> Result<()> {
    queue_source_move_notation_kind(
        state,
        piece,
        at,
        color,
        piece_type,
        "special",
        text,
        description,
        privacy,
        capture,
        capture_known_squares,
    )
}

fn source_notation_move_number(state: &GameState) -> Result<u64> {
    let value = crate::observation::number(state.extra.get("activeHistoryMoveNumber"))
        .unwrap_or(f64::from(state.full_move).max(1.0))
        .max(0.0)
        .floor();
    if value > u64::MAX as f64 {
        return Err(EngineError::InvalidState(
            "v7 notation move number exceeds supported range".into(),
        ));
    }
    Ok(value as u64)
}

fn queue_source_move_notation_kind(
    state: &mut GameState,
    piece: &Piece,
    at: Square,
    color: Color,
    piece_type: &str,
    kind: &str,
    text: String,
    description: String,
    privacy: &Value,
    capture: bool,
    capture_known_squares: &Value,
) -> Result<()> {
    let move_number = source_notation_move_number(state)?;
    let event = queue_notation(state, kind, color, text.clone(), description, move_number)?;
    if event.is_null() {
        return Ok(());
    }
    let redactions = source_move_redactions(
        state,
        piece,
        at,
        piece_type,
        &text,
        privacy,
        capture,
        capture_known_squares,
    )?;
    attach_notation_redactions(state, event, redactions)?;
    Ok(())
}

fn source_move_redactions(
    state: &GameState,
    piece: &Piece,
    at: Square,
    piece_type: &str,
    text: &str,
    privacy: &Value,
    capture: bool,
    capture_known_squares: &Value,
) -> Result<serde_json::Map<String, Value>> {
    let mut redactions = serde_json::Map::new();
    let code = piece_code(&state.ruleset_id, piece_type);
    let code = if code.is_empty() { "P" } else { &code };
    for viewer in [Color::White, Color::Black] {
        if piece.color == viewer {
            continue;
        }
        let visible = crate::observation::piece_visible_to_color_at_v7(state, piece, at, viewer)?;
        if visible {
            continue;
        }
        let known = crate::observation::truth(
            privacy
                .get(viewer.as_str())
                .and_then(|entry| entry.get("typeKnown")),
        );
        let capture_square = capture_known_squares
            .get(viewer.as_str())
            .and_then(|value| serde_json::from_value::<Square>(value.clone()).ok())
            .filter(|square| square.row < 8 && square.col < 8);
        let masked = if let Some(square) = capture_square {
            format!(
                "{}{}{}",
                if known { code } else { "?" },
                if capture { "x" } else { "" },
                square_name(square)
            )
        } else if known {
            format!("{code}??")
        } else {
            "???".into()
        };
        if masked != text {
            redactions.insert(
                viewer.as_str().into(),
                json!({"text":masked,"description":format!("상대의 숨겨진 이동 ({masked})")}),
            );
        }
    }
    Ok(redactions)
}

/// main63859. 브라우저 global madAiOption은 동결 local profile에 포함되지 않는다.
pub(crate) fn fog_log_redaction_active_v7(state: &GameState) -> bool {
    matches!(
        state
            .extra
            .get("campaign")
            .and_then(|value| value.get("setup"))
            .and_then(Value::as_str),
        Some("fogWar" | "fog")
    ) || crate::observation::truth(state.extra.get("fogWar"))
        || crate::observation::truth(state.extra.get("fogOfWar"))
        || crate::observation::truth(
            state
                .extra
                .get("fog")
                .and_then(|value| value.get("enabled")),
        )
}

pub(crate) fn queue_automatic_move_notation(
    state: &mut GameState,
    piece: &Piece,
    from: Square,
    to: Square,
    privacy: Option<&Value>,
    capture: bool,
) -> Result<()> {
    let color = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let name = if state.ruleset_id == RULES_VERSION_V7 {
        source_piece_label(&piece.kind)
            .filter(|name| !name.is_empty())
            .unwrap_or(&piece.kind)
    } else {
        piece_label(&piece.kind)
    };
    let description = format!(
        "{} {} {}에서 {}{}",
        label(color),
        name,
        square_name(from),
        square_name(to),
        if capture { " 포획" } else { " 자동 이동" }
    );
    queue_automatic_move_notation_with_options(
        state,
        piece,
        from,
        to,
        privacy,
        color,
        &piece.kind,
        capture,
        state.mode == "gameover",
        &description,
    )
}

/// Source 88342. The notation color may differ from a neutral mover's color;
/// privacy still follows the actual mover. Omitted moveNumber keeps the common
/// activeHistoryMoveNumber fallback used by queueHistoryNotation.
pub(crate) fn queue_automatic_move_notation_with_options(
    state: &mut GameState,
    piece: &Piece,
    from: Square,
    to: Square,
    privacy: Option<&Value>,
    notation_color: Color,
    piece_type: &str,
    capture: bool,
    game_end: bool,
    description: &str,
) -> Result<()> {
    let code = piece_code(&state.ruleset_id, piece_type);
    let prefix = if piece_type == "pawn" && capture {
        char::from(b'a' + from.col).to_string()
    } else {
        String::new()
    };
    let text = format!(
        "{code}{prefix}{}{}{}",
        if capture { "x" } else { "" },
        square_name(to),
        if game_end { "#" } else { "" }
    );
    queue_source_move_notation(
        state,
        piece,
        to,
        notation_color,
        piece_type,
        text,
        description.to_owned(),
        privacy.unwrap_or(&Value::Null),
        capture,
        &Value::Null,
    )
}

pub(crate) fn queue_hp_attack_notation(
    state: &mut GameState,
    piece: &Piece,
    from: Square,
    target: &Piece,
    at: Square,
    privacy: &Value,
) -> Result<()> {
    let color = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let target_square = square_name(at);
    let ending = if state.mode == "gameover" { "#" } else { "" };
    let text = format!(
        "{}⇢{target_square}{ending}",
        piece_code(&state.ruleset_id, &piece.kind)
    );
    let hp = crate::observation::number(target.extra.get("hp"))
        .unwrap_or(0.0)
        .max(0.0);
    let max_hp = crate::observation::number(target.extra.get("maxHp"))
        .unwrap_or(hp)
        .max(hp);
    let moving_label = if state.ruleset_id == RULES_VERSION_V7 {
        source_piece_label(&piece.kind).unwrap_or(&piece.kind)
    } else {
        piece_label(&piece.kind)
    };
    let target_label = if state.ruleset_id == RULES_VERSION_V7 {
        source_piece_label(&target.kind).unwrap_or(&target.kind)
    } else {
        piece_label(&target.kind)
    };
    let description = format!(
        "{} {}이 {target_square}의 {}을 공격 (HP {hp}/{max_hp})",
        label(color),
        moving_label,
        target_label
    );
    let number = state
        .extra
        .get("activeHistoryMoveNumber")
        .and_then(Value::as_u64)
        .unwrap_or(u64::from(state.full_move).max(1));
    let event = queue_notation(state, "special", color, text, description, number)?;
    if event.is_null() {
        return Ok(());
    }
    let mut redactions = serde_json::Map::new();
    for viewer in [Color::White, Color::Black] {
        if piece.color == viewer {
            continue;
        }
        if crate::observation::truth(
            privacy
                .get(viewer.as_str())
                .and_then(|entry| entry.get("typeKnown")),
        ) || crate::observation::piece_visible_to_color_at_v7(state, piece, from, viewer)?
        {
            continue;
        }
        let masked = format!("?⇢{target_square}{ending}");
        redactions.insert(
            viewer.as_str().into(),
            json!({"text":masked,"description":format!("상대의 숨겨진 공격 ({masked})")}),
        );
    }
    attach_notation_redactions(state, event, redactions)?;
    Ok(())
}

/// Queue the source collapse replay visual before its `!붕괴` notation. The
/// caller has already settled the board, captures, and bomb removals. A
/// removed item is the complete source-shaped piece at its first ring cell.
fn collapse_ring(depth: u64) -> Vec<(u8, u8)> {
    let min = (depth - 1) as u8;
    let max = (8 - depth) as u8;
    let mut cells = Vec::new();
    for row in min..=max {
        for col in min..=max {
            if row == min || row == max || col == min || col == max {
                cells.push((row, col));
            }
        }
    }
    cells
}

pub(crate) fn queue_collapse_effect(
    state: &mut GameState,
    color: Color,
    depth: u64,
    cells: &[(u8, u8)],
    removed: &[(Value, u8, u8)],
) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 || !(1..=4).contains(&depth) || cells.is_empty() {
        return Err(EngineError::InvalidState(
            "v7 collapse replay needs a ring and depth 1 through 4".into(),
        ));
    }
    if cells != collapse_ring(depth).as_slice() {
        return Err(EngineError::InvalidState(
            "v7 collapse replay cells must be the complete row-major ring".into(),
        ));
    }
    for (item, row, col) in removed {
        if !item.is_object() || cells.binary_search(&(*row, *col)).is_err() {
            return Err(EngineError::InvalidState(
                "v7 collapse replay removed item must belong to the ring".into(),
            ));
        }
    }
    if state
        .extra
        .get("pendingReplayVisuals")
        .is_some_and(|value| !value.is_array())
        || state
            .extra
            .get("pendingNotations")
            .is_some_and(|value| !value.is_array())
    {
        return Err(EngineError::InvalidState(
            "v7 collapse replay queues must be arrays".into(),
        ));
    }
    queue_visual(
        state,
        json!({
            "type":"collapse",
            "color":color,
            "cells":cells.iter().map(|&(row,col)|json!({"row":row,"col":col})).collect::<Vec<_>>(),
            "removed":removed.iter().map(|(item,row,col)|json!({"item":item,"row":row,"col":col})).collect::<Vec<_>>()
        }),
    )?;
    queue_special_effect_notation(
        state,
        color,
        "붕괴",
        &format!("{depth}번째 보드 외곽 붕괴로 기물 {}개 제거", removed.len()),
    )?;
    Ok(())
}

pub(crate) fn queue_gain(
    state: &mut GameState,
    color: Color,
    card: &Value,
    phase: &str,
) -> Result<Value> {
    let name = card["name"]
        .as_str()
        .filter(|v| !v.is_empty())
        .or_else(|| card["id"].as_str())
        .ok_or(EngineError::IllegalAction)?;
    let number = match phase {
        "OPENING" => 1,
        "MIDDLE" => 10,
        "END" => 20,
        _ => state.full_move,
    };
    queue_notation(
        state,
        "gain",
        color,
        format!("+{}", trim(name, 60)),
        format!("{} 카드 획득: {name}", label(color)),
        u64::from(number),
    )
}
pub(crate) fn queue_card(state: &mut GameState, color: Color, card: &CardSlot) -> Result<()> {
    let name = card.extra.get("name").and_then(Value::as_str);
    let move_number = state
        .extra
        .get("activeHistoryMoveNumber")
        .and_then(Value::as_u64)
        .unwrap_or(u64::from(state.full_move.max(1)));
    let event = queue_notation(
        state,
        "card",
        color,
        format!("@{}", trim(name.unwrap_or(""), 60)),
        format!("{} 카드 {}", label(color), name.unwrap_or("undefined")),
        move_number,
    )?;
    let list = state
        .extra
        .get_mut("pendingNotations")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState("pending notations must be an array".into()))?;
    if list.len() > 1 {
        list.retain(|notation| notation["id"] != event["id"]);
        list.insert(0, event);
    }
    Ok(())
}

/// The source's forced first-move card goes through
/// recordForcedOpeningCardUse, which appends a special notation after the move
/// notation. Ordinary finishCard reorders its card notation to the front.
pub(crate) fn queue_forced_opening_card(
    state: &mut GameState,
    color: Color,
    card: &CardSlot,
) -> Result<()> {
    let name = card.extra.get("name").and_then(Value::as_str).unwrap_or("");
    let move_number = state
        .extra
        .get("activeHistoryMoveNumber")
        .and_then(Value::as_u64)
        .unwrap_or(u64::from(state.full_move.max(1)));
    queue_notation(
        state,
        "card",
        color,
        format!("@{}", trim(name, 60)),
        format!(
            "{} 카드 {}",
            label(color),
            card.extra
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("undefined")
        ),
        move_number,
    )?;
    Ok(())
}
fn square_name(square: Square) -> String {
    format!("{}{}", char::from(b'a' + square.col), 8 - square.row)
}

pub(crate) fn piece_code(rules_version: &str, kind: &str) -> String {
    let constants = if rules_version == RULES_VERSION_V7 {
        crate::v7_execution_profile::replay_metadata()
    } else {
        metadata()
    };
    constants["codes"][kind]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| kind.chars().take(2).collect::<String>().to_uppercase())
}
pub(crate) fn piece_label(kind: &str) -> &str {
    metadata()["labels"][kind].as_str().unwrap_or(kind)
}

/// Reviewed source initializer의 직접 TYPE_LABELS 조회. 원문에서 fallback을
/// 쓰지 않는 로그는 누락된 key를 JavaScript의 "undefined"로 표시한다.
pub(crate) fn source_piece_label(kind: &str) -> Option<&'static str> {
    crate::v7_execution_profile::replay_metadata()["labels"][kind].as_str()
}

/// main88255: 현재 보드의 legal 후보와 실제 portal 목적지에서 구분 문자를
/// 계산한다. 호출자는 포획 전의 source 시점에 이 결과를 보관해야 한다.
pub(crate) fn move_notation_disambiguation_v7(
    state: &GameState,
    piece: &Piece,
    from: Square,
    to: Square,
) -> Result<String> {
    if matches!(
        piece.kind.as_str(),
        "pawn" | "colossus" | "bigRook" | "bigBishop"
    ) {
        return Ok(String::new());
    }
    let code = piece_code(&state.ruleset_id, &piece.kind);
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            let Some(other) = state.at(at) else {
                continue;
            };
            if other.id == piece.id
                || other.color != piece.color
                || seen.contains(&other.id)
                || piece_code(&state.ruleset_id, &other.kind) != code
            {
                continue;
            }
            seen.insert(other.id.clone());
            if crate::movement::v7_legal_move_targets(
                state,
                other,
                at,
                crate::movement::V7MoveOptions::default(),
            )?
            .iter()
            .any(|m| {
                let destination = if m.flag("portalLanding") {
                    m.flags
                        .get("portalExit")
                        .and_then(|v| serde_json::from_value::<Square>(v.clone()).ok())
                        .unwrap_or_else(|| m.square())
                } else {
                    m.square()
                };
                destination == to
            }) {
                candidates.push(at);
            }
        }
    }
    if candidates.is_empty() {
        return Ok(String::new());
    }
    Ok(if !candidates.iter().any(|at| at.col == from.col) {
        char::from(b'a' + from.col).to_string()
    } else if !candidates.iter().any(|at| at.row == from.row) {
        (8 - from.row).to_string()
    } else {
        square_name(from)
    })
}

pub(crate) struct V7MoveNotationOptions<'a> {
    pub piece_type: &'a str,
    pub disambiguation: &'a str,
    pub promotion: Option<&'a str>,
    pub privacy: &'a Value,
    pub capture: bool,
    pub capture_known_squares: &'a Value,
    pub game_end: bool,
}

fn format_v7_move_notation(
    state: &GameState,
    from: Square,
    to: Square,
    target: &MoveTarget,
    options: &V7MoveNotationOptions<'_>,
) -> String {
    let ending = if options.game_end { "#" } else { "" };
    if target.flag("castle") {
        return format!(
            "{}{ending}",
            if to.col > from.col { "O-O" } else { "O-O-O" }
        );
    }
    let prefix = if options.piece_type == "pawn" {
        if options.capture {
            char::from(b'a' + from.col).to_string()
        } else {
            String::new()
        }
    } else {
        options.disambiguation.to_owned()
    };
    let promotion = options
        .promotion
        .filter(|value| !value.is_empty())
        .map(|kind| format!("={}", piece_code(&state.ruleset_id, kind)))
        .unwrap_or_default();
    format!(
        "{}{}{}{}{}{}{ending}",
        piece_code(&state.ruleset_id, options.piece_type),
        prefix,
        if options.capture { "x" } else { "" },
        square_name(to),
        promotion,
        if target.flag("enPassant") {
            " e.p."
        } else {
            ""
        }
    )
}

/// main88279: 실제 mover의 visibility와 이동 전 기물 type을 각각 보존한다.
pub(crate) fn queue_v7_move_notation_with_options(
    state: &mut GameState,
    piece: &Piece,
    from: Square,
    to: Square,
    target: &MoveTarget,
    options: &V7MoveNotationOptions<'_>,
) -> Result<()> {
    if state.free_move_don_quixote {
        return queue_don_quixote_notation_with_type(
            state,
            piece,
            from,
            to,
            options.privacy,
            options.capture,
            false,
            options.piece_type,
            options.capture_known_squares,
        );
    }
    let color = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let text = format_v7_move_notation(state, from, to, target, options);
    let description = format!(
        "{} {} {}에서 {}{}",
        label(color),
        source_piece_label(options.piece_type).unwrap_or(options.piece_type),
        square_name(from),
        square_name(to),
        if options.capture {
            " 포획"
        } else {
            " 이동"
        }
    );
    queue_source_move_notation_kind(
        state,
        piece,
        to,
        color,
        options.piece_type,
        "move",
        text,
        description,
        options.privacy,
        options.capture,
        options.capture_known_squares,
    )
}

/// main88342: Don Quixote 자동 이동과 폭주의 기보·양측 숨김을 공유한다.
pub(crate) fn queue_don_quixote_move_notation(
    state: &mut GameState,
    piece: &Piece,
    from: Square,
    to: Square,
    privacy: &Value,
    capture: bool,
    rampage: bool,
) -> Result<()> {
    queue_don_quixote_notation_with_type(
        state,
        piece,
        from,
        to,
        privacy,
        capture,
        rampage,
        &piece.kind,
        &Value::Null,
    )
}

fn queue_don_quixote_notation_with_type(
    state: &mut GameState,
    piece: &Piece,
    from: Square,
    to: Square,
    privacy: &Value,
    capture: bool,
    rampage: bool,
    piece_type: &str,
    capture_known_squares: &Value,
) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::InvalidState(
            "Don Quixote notation requires v7 rules".into(),
        ));
    }
    let color = piece.color.owner().ok_or(EngineError::WrongActor)?;
    if from.row >= 8 || from.col >= 8 || to.row >= 8 || to.col >= 8 {
        return Err(EngineError::IllegalAction);
    }
    let text = format!(
        "DQ{}{}{}",
        square_name(from),
        if capture { "x" } else { "-" },
        square_name(to)
    );
    let description = format!(
        "돈 키호테 {}: {} → {}{}",
        if rampage { "폭주" } else { "자동 이동" },
        square_name(from),
        square_name(to),
        if capture { " 포획" } else { "" }
    );
    let event = queue_notation(
        state,
        "special",
        color,
        text.clone(),
        description,
        u64::from(state.full_move),
    )?;
    if event.is_null() {
        return Ok(());
    }
    let redactions = source_move_redactions(
        state,
        piece,
        to,
        piece_type,
        &text,
        privacy,
        capture,
        capture_known_squares,
    )?;
    attach_notation_redactions(state, event, redactions).map(|_| ())
}

/// main88313: 실제 포획 전 실패 시도 기보와 별도 숨김 문구를 기록한다.
pub(crate) fn queue_mistake_attempt_notation_v7(
    state: &mut GameState,
    piece: &Piece,
    from: Square,
    to: Square,
    target: &MoveTarget,
    options: &V7MoveNotationOptions<'_>,
) -> Result<()> {
    let actor = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let attempted = format_v7_move_notation(state, from, to, target, options);
    let text = format!(
        "{} !실수",
        if attempted.is_empty() {
            "Move"
        } else {
            &attempted
        }
    );
    let description = format!(
        "{} {}의 {}→{} 포획 시도가 반격되었습니다.",
        label(actor),
        source_piece_label(options.piece_type).unwrap_or(options.piece_type),
        square_name(from),
        square_name(to)
    );
    let move_number = source_notation_move_number(state)?;
    let event = queue_notation(state, "special", actor, text, description, move_number)?;
    if event.is_null() {
        return Ok(());
    }
    let mut redactions = source_move_redactions(
        state,
        piece,
        to,
        options.piece_type,
        &attempted,
        options.privacy,
        true,
        options.capture_known_squares,
    )?;
    for value in redactions.values_mut() {
        let masked = value.get("text").and_then(Value::as_str).ok_or_else(|| {
            EngineError::InvalidState("Mistake redaction lost notation text".into())
        })?;
        let masked = format!("{masked} !실수");
        *value = json!({"text":masked,"description":format!("상대의 숨겨진 포획 시도가 반격되었습니다. ({masked})")});
    }
    attach_notation_redactions(state, event, redactions)?;
    Ok(())
}

/// main88416: 도약 포획은 실제 이동 기보와 별개인 effect event다.
pub(crate) fn queue_jump_capture_notation_v7(
    state: &mut GameState,
    piece: &Piece,
    target: &MoveTarget,
    captured: Option<&Piece>,
    privacy: &Value,
) -> Result<()> {
    let Some(value) = target.flags.get("jumpCapture") else {
        return Ok(());
    };
    let Some(captured) = captured else {
        return Ok(());
    };
    let at: Square = serde_json::from_value(value.clone()).map_err(|error| {
        EngineError::InvalidState(format!("jump capture notation square: {error}"))
    })?;
    if at.row >= 8 || at.col >= 8 {
        return Err(EngineError::InvalidState(
            "jump capture notation square is out of bounds".into(),
        ));
    }
    let actor = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let name = if target.flag("checkerCapture") {
        "체커"
    } else {
        "난폭한 돌진"
    };
    let text = format!("!{name}:{}", square_name(at));
    let description = format!(
        "{} {name}이 {}의 {}을 포획",
        label(actor),
        square_name(at),
        source_piece_label(&captured.kind).unwrap_or(&captured.kind)
    );
    let move_number = source_notation_move_number(state)?;
    let event = queue_notation(
        state,
        "effect",
        actor,
        text.clone(),
        description,
        move_number,
    )?;
    if event.is_null() {
        return Ok(());
    }
    let destination = (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .find(|cell| state.at(*cell).is_some_and(|other| other.id == piece.id))
        .unwrap_or(at);
    let known = if let Some(color) = captured.color.owner() {
        json!({color.as_str():at})
    } else {
        json!({})
    };
    let mut redactions = source_move_redactions(
        state,
        piece,
        destination,
        &piece.kind,
        &text,
        privacy,
        true,
        &known,
    )?;
    for value in redactions.values_mut() {
        let masked = value.get("text").and_then(Value::as_str).ok_or_else(|| {
            EngineError::InvalidState("jump capture redaction lost notation text".into())
        })?;
        *value = json!({"text":masked,"description":format!("상대의 숨겨진 포획 ({masked})")});
    }
    attach_notation_redactions(state, event, redactions)?;
    Ok(())
}

pub(crate) fn queue_v7_move_notation(
    state: &mut GameState,
    piece: &Piece,
    from: Square,
    to: Square,
    target: &MoveTarget,
    privacy: &Value,
    capture: bool,
    capture_known_squares: &Value,
    game_end: bool,
) -> Result<()> {
    let disambiguation = move_notation_disambiguation_v7(state, piece, from, to)?;
    queue_v7_move_notation_with_options(
        state,
        piece,
        from,
        to,
        target,
        &V7MoveNotationOptions {
            piece_type: &piece.kind,
            disambiguation: &disambiguation,
            promotion: None,
            privacy,
            capture,
            capture_known_squares,
            game_end,
        },
    )
}
pub(crate) fn queue_move(
    state: &mut GameState,
    before: &GameState,
    piece: &Piece,
    from: Square,
    to: Square,
    target: &MoveTarget,
    capture: bool,
) -> Result<()> {
    let actor = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let mut disambiguation = String::new();
    if !matches!(
        piece.kind.as_str(),
        "pawn" | "colossus" | "bigRook" | "bigBishop"
    ) {
        let mut candidates = Vec::new();
        let mut seen = BTreeSet::new();
        for row in 0..8 {
            for col in 0..8 {
                let square = Square { row, col };
                let Some(other) = before.at(square) else {
                    continue;
                };
                if other.id == piece.id
                    || other.color != piece.color
                    || piece_code(&state.ruleset_id, &other.kind)
                        != piece_code(&state.ruleset_id, &piece.kind)
                    || !seen.insert(other.id.clone())
                {
                    continue;
                }
                if crate::movement::piece_moves(before, other, square)?
                    .iter()
                    .any(|m| m.square() == to)
                {
                    candidates.push(square);
                }
            }
        }
        if !candidates.is_empty() {
            disambiguation = if !candidates.iter().any(|s| s.col == from.col) {
                char::from(b'a' + from.col).to_string()
            } else if !candidates.iter().any(|s| s.row == from.row) {
                (8 - from.row).to_string()
            } else {
                square_name(from)
            };
        }
    }
    let from_name = square_name(from);
    let to_name = square_name(to);
    let text = if target.flag("castle") {
        if to.col > from.col {
            "O-O".into()
        } else {
            "O-O-O".into()
        }
    } else {
        format!(
            "{}{}{}{to_name}{}{}",
            piece_code(&state.ruleset_id, &piece.kind),
            if piece.kind == "pawn" {
                if capture {
                    char::from(b'a' + from.col).to_string()
                } else {
                    String::new()
                }
            } else {
                disambiguation
            },
            if capture { "x" } else { "" },
            if target.flag("enPassant") {
                " e.p."
            } else {
                ""
            },
            if state.mode == "gameover" { "#" } else { "" }
        )
    };
    let name = metadata()["labels"][&piece.kind]
        .as_str()
        .unwrap_or(&piece.kind);
    let mut event = queue_notation(
        state,
        "move",
        actor,
        text.clone(),
        format!(
            "{} {name} {from_name}에서 {to_name} {}",
            label(actor),
            if capture { "포획" } else { "이동" }
        ),
        u64::from(state.full_move),
    )?;
    let viewer = actor.opponent();
    if !state.piece_visible(piece, to, viewer) {
        let known = before.piece_visible(piece, from, viewer)
            || piece.extra.get("hiddenFrom").and_then(Value::as_str) == Some(viewer.as_str());
        let capture_square = before
            .at(to)
            .filter(|victim| capture && victim.color == viewer)
            .map(|_| to_name.clone());
        let masked = if let Some(square) = capture_square {
            format!(
                "{}x{square}",
                if known {
                    piece_code(&state.ruleset_id, &piece.kind)
                } else {
                    "?".into()
                }
            )
        } else if known {
            format!("{}??", piece_code(&state.ruleset_id, &piece.kind))
        } else {
            "???".into()
        };
        if masked != text {
            event["redactions"] = json!({viewer.as_str():{"text":masked,"description":format!("상대의 숨겨진 이동 ({masked})")}});
            if let Some(list) = state
                .extra
                .get_mut("pendingNotations")
                .and_then(Value::as_array_mut)
                && let Some(stored) = list.iter_mut().find(|v| v["id"] == event["id"])
            {
                *stored = event.clone();
            }
            state.extra.insert("pendingNotation".into(), event);
        }
    }
    Ok(())
}

/// Begin/commit is kept local to an atomic transition, matching the source's
/// activeMoveReplayCapture control variable without leaking it into snapshots.
pub(crate) fn begin_move(state: &mut GameState, actor: Color) -> Result<GameState> {
    let mut before = state.clone();
    before.active_move_replay_before = None;
    let old = state
        .extra
        .get("moveReplay")
        .cloned()
        .unwrap_or(Value::Null);
    if !old.is_null() && !old.is_object() {
        return Err(EngineError::InvalidState(
            "moveReplay must be a player map".into(),
        ));
    }
    // normalizeColorValues constructs a new white-then-black object. Its
    // insertion order enters the source's JSON.stringify replay delta even
    // when both color values stay null after a threat probe interrupts capture.
    let mut replay = serde_json::Map::new();
    for color in [Color::White, Color::Black] {
        let value = if color == actor {
            Value::Null
        } else {
            old[color.as_str()].clone()
        };
        replay.insert(color.as_str().into(), value);
    }
    state
        .extra
        .insert("moveReplay".into(), Value::Object(replay));
    if state.ruleset_id == RULES_VERSION_V7 {
        let capture = if state.free_move_resolution == Some(actor) {
            None
        } else {
            Some(MoveReplayCapture {
                actor,
                before: Box::new(before.clone()),
            })
        };
        replace_active_move_capture(state, capture)?;
    }
    Ok(before)
}
pub(crate) fn commit_active_move(state: &mut GameState, actor: Color) -> Result<()> {
    let capture = active_move_capture(state)?;
    replace_active_move_capture(state, None)?;
    if let Some(capture) = capture {
        if capture.actor == actor {
            commit_move(state, &capture.before, actor)?;
        }
    }
    Ok(())
}
fn board_dimensions(board: &Value) -> Result<(usize, usize)> {
    let rows = board
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("replay board must be an array".into()))?;
    let cols = rows
        .first()
        .map(|row| {
            row.as_array()
                .map(Vec::len)
                .ok_or_else(|| EngineError::InvalidState("replay board rows must be arrays".into()))
        })
        .transpose()?
        .unwrap_or(0);
    if rows.is_empty() || cols == 0 {
        return Err(EngineError::InvalidState(
            "replay board dimensions must be nonzero".into(),
        ));
    }
    if rows
        .iter()
        .any(|row| row.as_array().is_none_or(|cells| cells.len() != cols))
    {
        return Err(EngineError::InvalidState(
            "replay board must be rectangular".into(),
        ));
    }
    Ok((rows.len(), cols))
}

fn board_delta(before: &Value, after: &Value) -> Result<Vec<Value>> {
    let (rows, cols) = board_dimensions(before)?;
    if board_dimensions(after)? != (rows, cols) {
        // Source behavior for a rule that resizes the board needs its own
        // captured replay proof. Never omit cells outside the old 8x8 scan.
        return Err(EngineError::UnsupportedFeature(
            "replay board resize".into(),
        ));
    }
    let mut cells = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            if !canonical_equal(&before[row][col], &after[row][col])? {
                cells.push(
                    json!({"row":row,"col":col,"before":before[row][col],"after":after[row][col]}),
                );
            }
        }
    }
    Ok(cells)
}
pub(crate) fn commit_move(state: &mut GameState, before: &GameState, actor: Color) -> Result<()> {
    if state.ruleset_id == RULES_VERSION_V7 {
        replace_active_move_capture(state, None)?;
    }
    let delta = board_delta(&json!(before.board), &json!(state.board))?;
    if delta.is_empty() {
        return Ok(());
    }
    let mut replay = json!({"color":actor,"delta":delta,"capturesBefore":before.captures.get(actor),"capturesAfter":state.captures.get(actor),"enPassantBefore":before.en_passant,"enPassantAfter":state.en_passant,"previousLastMove":value(before,"lastMove",Value::Null),"lastMoveAfter":value(state,"lastMove",Value::Null),"recordedMoveCount":state.move_count});
    for key in [
        "accelerationTrail",
        "castled",
        "zugzwang",
        "quantumPending",
        "switcheroo",
        "crownRule",
        "moving",
        "exhaustion",
        "ultimatum",
    ] {
        let fallback = match key {
            "castled" | "zugzwang" | "quantumPending" | "switcheroo" => {
                json!({"white":false,"black":false})
            }
            "moving" => {
                json!({"white":{"enabled":false,"pieceId":"","count":0},"black":{"enabled":false,"pieceId":"","count":0}})
            }
            "exhaustion" => {
                json!({"white":{"pieceId":"","count":0},"black":{"pieceId":"","count":0}})
            }
            _ => Value::Null,
        };
        replay[format!("{key}Before")] = value(before, key, fallback.clone());
        replay[format!("{key}After")] = value(state, key, fallback);
    }
    state
        .extra
        .get_mut("moveReplay")
        .ok_or(EngineError::IllegalAction)?[actor.as_str()] = replay;
    Ok(())
}

fn card_snapshot(card: &Value, color: Color, slot: usize) -> Value {
    if card.is_null() {
        return Value::Null;
    }
    let number = |key: &str| {
        card.get(key)
            .and_then(Value::as_f64)
            .or_else(|| card.get(key).filter(|v| v.is_null()).map(|_| 0.0))
    };
    let flag = |key: &str| card[key].as_bool().unwrap_or(false);
    let mut snapshot = json!({"id":card["id"],"instanceId":card["instanceId"],"color":color,"slot":slot,"acquiredOrder":number("acquiredOrder").filter(|n|*n!=0.0).unwrap_or(slot as f64+1.0).max(0.0),"used":flag("used"),"passiveApplied":flag("passiveApplied"),"recovering":flag("recovering"),"nextTurnPending":flag("nextTurnPending"),"nextTurnPendingSinceTurn":number("nextTurnPendingSinceTurn").map(|v|json!(v.max(0.0))).unwrap_or(Value::Null),"deckCard":card["deckCard"]!=json!(false),"campaignCard":flag("campaignCard"),"firstTurnCard":flag("firstTurnCard"),"devCard":flag("devCard"),"disabled":flag("disabled")});
    if let Some(remaining) = number("usesRemaining") {
        snapshot["usesRemaining"] = json!(remaining.max(0.0));
    }
    for key in [
        "bloodEffectId",
        "bloodRevealed",
        "boxRevealedCardId",
        "randomRouletteResultType",
        "suspiciousPotionResultId",
    ] {
        if let Some(v) = card.get(key)
            && !v.is_null()
            && v != &json!(false)
            && v != &json!("")
        {
            snapshot[key] = v.clone();
        }
    }
    snapshot
}
fn capture_frame(state: &GameState) -> Result<Value> {
    let raw = serde_json::to_value(state).map_err(EngineError::serialization)?;
    let mut frame = serde_json::Map::new();
    let constants = if state.ruleset_id == RULES_VERSION_V7 {
        crate::v7_execution_profile::replay_metadata()
    } else {
        metadata()
    };
    for key in constants["frameKeys"]
        .as_array()
        .expect("frame keys")
        .iter()
        .filter_map(Value::as_str)
    {
        if let Some(v) = raw.get(key) {
            frame.insert(key.into(), v.clone());
        }
    }
    let mut cards = serde_json::Map::new();
    for color in [Color::White, Color::Black] {
        let snapshots = raw["deckSlots"][color.as_str()]
            .as_array()
            .expect("serialized deck slots")
            .iter()
            .enumerate()
            .map(|(i, c)| card_snapshot(c, color, i))
            .collect::<Vec<_>>();
        cards.insert(color.as_str().into(), json!(snapshots));
    }
    frame.insert("cardState".into(), Value::Object(cards));
    let mut frame = clone_replay(&Value::Object(frame));
    if state.ruleset_id == RULES_VERSION_V7 {
        if let Some(journey) = frame
            .get_mut("campaign")
            .and_then(|campaign| campaign.get_mut("knightJourney"))
            .and_then(Value::as_object_mut)
        {
            journey.shift_remove("undo");
        }
        if let Some(auction) = frame
            .get_mut("campaign")
            .and_then(|campaign| campaign.get_mut("auction"))
            .and_then(Value::as_object_mut)
        {
            for key in [
                "uiSignature",
                "selectedPlacement",
                "pendingPlacementCell",
                "aiBidAt",
                "resolving",
            ] {
                auction.shift_remove(key);
            }
        }
    }
    Ok(frame)
}
fn replay_delta(before: &Value, after: &Value) -> Result<Value> {
    let mut fields = Vec::new();
    let before_fields = before
        .as_object()
        .ok_or_else(|| EngineError::InvalidState("replay before frame must be an object".into()))?;
    let after_fields = after
        .as_object()
        .ok_or_else(|| EngineError::InvalidState("replay after frame must be an object".into()))?;
    let mut keys = before_fields.keys().map(String::as_str).collect::<Vec<_>>();
    for key in after_fields.keys().map(String::as_str) {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    for key in keys.into_iter().filter(|k| *k != "board") {
        if before.get(key).is_some() == after.get(key).is_some()
            && canonical_equal(&before[key], &after[key])?
        {
            continue;
        }
        fields.push(json!({"key":key,"beforeExists":before.get(key).is_some(),"afterExists":after.get(key).is_some(),"before":before[key],"after":after[key]}));
    }
    let (before_rows, before_cols) = board_dimensions(&before["board"])?;
    let (after_rows, after_cols) = board_dimensions(&after["board"])?;
    let cells = board_delta(&before["board"], &after["board"])?;
    Ok(
        json!({"board":{"before":{"rows":before_rows,"cols":before_cols},"after":{"rows":after_rows,"cols":after_cols},"cells":cells},"fields":fields}),
    )
}
fn notation_key(event: &Value) -> String {
    event["id"].as_str().unwrap_or("").to_owned()
}
pub(crate) fn record(state: &mut GameState, label: &str) -> Result<()> {
    record_with_effects(state, label, &[])
}

pub(crate) fn record_with_trolley_effects(
    state: &mut GameState,
    targets: &[Value],
    by: Color,
) -> Result<()> {
    if targets.is_empty() {
        return record_with_effects(state, "trolley", &[]);
    }
    record_with_effects(
        state,
        "trolley",
        &[json!({"effect":"trolley","color":by,"targets":targets})],
    )
}

fn record_with_effects(state: &mut GameState, label: &str, effects: &[Value]) -> Result<()> {
    if state.ruleset_id == RULES_VERSION_V7 {
        crate::v7_move_transition::sync_active_metal(state)?;
    }
    if !state.extra.contains_key("boardHistory") {
        return Ok(());
    }
    if state.ruleset_id == RULES_VERSION_V7 && state.threat_probe_depth > 0 {
        // main89241: threat probe는 frame/history 대신 실행 횟수를 담은
        // compact event를 남긴다. AI depth만 있는 기록은 일반 경로를 사용한다.
        let nonce = crate::observation::number(state.extra.get("replayEventNonce"))
            .filter(|number| number.is_finite())
            .unwrap_or(0.0)
            + 1.0;
        if !nonce.is_finite() || nonce.abs() > 9_007_199_254_740_991.0 {
            return Err(EngineError::InvalidState(
                "v7 threat replay event nonce exceeds finite safe Number".into(),
            ));
        }
        if !state.extra.get("replayEvents").is_some_and(Value::is_array) {
            state.extra.insert("replayEvents".into(), json!([]));
        }
        let nonce = serde_json::from_str::<Value>(
            &serde_jcs::to_string(&json!(nonce)).map_err(EngineError::serialization)?,
        )
        .map_err(EngineError::serialization)?;
        let id = format!(
            "threat-probe-{}",
            serde_jcs::to_string(&nonce).map_err(EngineError::serialization)?
        );
        state.extra.insert("replayEventNonce".into(), nonce);
        state
            .extra
            .get_mut("replayEvents")
            .and_then(Value::as_array_mut)
            .expect("replayEvents was normalized above")
            .push(json!({
                "id":id,"label":if label.is_empty(){"move"}else{label},"simulation":true
            }));
        state.extra.insert("pendingNotation".into(), Value::Null);
        state.extra.insert("pendingNotations".into(), json!([]));
        state.extra.insert("pendingReplayVisuals".into(), json!([]));
        return Ok(());
    }
    if state
        .extra
        .get("chainBonds")
        .and_then(Value::as_array)
        .is_some_and(|v| !v.is_empty())
    {
        if state.ruleset_id == RULES_VERSION_V7 {
            crate::v7_board_automata::break_out_of_range_chain_bonds(state)?;
        } else {
            return Err(EngineError::UnsupportedFeature(
                "record-time chain bond breakage".into(),
            ));
        }
    }
    if state
        .extra
        .get("replayTimelineReady")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(EngineError::UnsupportedFeature(
            "legacy replay timeline migration".into(),
        ));
    }
    let mut notations = state
        .extra
        .get("pendingNotations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(notation) = state.extra.get("pendingNotation").filter(|v| !v.is_null()) {
        notations.push(notation.clone());
    }
    let mut seen = BTreeSet::new();
    notations.retain(|v| {
        v["text"].as_str().is_some_and(|s| !s.is_empty()) && seen.insert(notation_key(v))
    });
    if label != "sync" && state.mode == "play" {
        let selected = notations
            .iter()
            .rposition(|event| event["kind"] == "move")
            .or_else(|| {
                notations
                    .iter()
                    .rposition(|event| event["kind"] == "special")
            });
        if let Some(index) = selected {
            let color = notations[index]["color"]
                .as_str()
                .and_then(|color| match color {
                    "white" => Some(Color::White),
                    "black" => Some(Color::Black),
                    _ => None,
                });
            if let Some(color) = color
                && crate::threat::evaluate_royal_capture(state, color.opponent())?.0
            {
                let event = &mut notations[index];
                let text = event["text"].as_str().unwrap_or("");
                if !text.ends_with(['+', '#']) {
                    event["text"] =
                        json!(format!("{}+", text.chars().take(95).collect::<String>()));
                    if let Some(redactions) =
                        event.get_mut("redactions").and_then(Value::as_object_mut)
                    {
                        for redaction in redactions.values_mut() {
                            let text = redaction["text"].as_str().unwrap_or("");
                            redaction["text"] =
                                json!(format!("{}+", text.chars().take(95).collect::<String>()));
                        }
                    }
                }
            }
        }
    }
    let mut entry = json!({"board":state.board,"lastMove":value(state,"lastMove",Value::Null),"blackHole":value(state,"blackHole",json!([])),"winterKingdom":value(state,"winterKingdom",Value::Null),"camouflageRule":state.extra.get("camouflageRule").and_then(Value::as_bool).unwrap_or(false),"cardAnimation":null,"effects":[],"notation":notations.first().cloned().unwrap_or(Value::Null),"label":label,"turn":state.turn,"moveCount":state.move_count,"fullMove":state.full_move});
    entry["effects"] = json!(effects);
    if !notations.is_empty() {
        state.extra.insert(
            "notationEvent".into(),
            notations.last().expect("notations").clone(),
        );
        let merged = state
            .extra
            .entry("notationEvents")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| EngineError::InvalidState("notationEvents must be an array".into()))?;
        for notation in &notations {
            if !merged
                .iter()
                .any(|v| notation_key(v) == notation_key(notation))
            {
                merged.push(notation.clone());
            }
        }
        if merged.len() > 20 {
            merged.drain(..merged.len() - 20);
        }
    }
    if state
        .extra
        .get("replayBaseFrame")
        .is_none_or(Value::is_null)
    {
        let base = capture_frame(state)?;
        state.extra.insert("replayBaseFrame".into(), base.clone());
        state.extra.insert("replayTailFrame".into(), base);
        state.extra.insert("replayEvents".into(), json!([]));
        state.extra.insert("notationTimeline".into(), json!([]));
        state.extra.insert("replayEventNonce".into(), json!(0));
    }
    let before = value(
        state,
        "replayTailFrame",
        value(state, "replayBaseFrame", json!({})),
    );
    let after = capture_frame(state)?;
    let delta = replay_delta(&before, &after)?;
    let mut visuals = state
        .extra
        .get("pendingReplayVisuals")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for effect in effects {
        if effect["effect"] == "trolley" {
            visuals.push(
                json!({"type":"trolley","color":effect["color"],"targets":effect["targets"]}),
            );
        }
    }
    if !entry["lastMove"].is_null() && !canonical_equal(&entry["lastMove"], &before["lastMove"])? {
        visuals.push(json!({"type":"move","move":entry["lastMove"]}));
    }
    let timeline = state
        .extra
        .get("notationTimeline")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let unique = notations
        .iter()
        .filter(|n| {
            !timeline
                .iter()
                .any(|e| notation_key(&e["notation"]) == notation_key(n))
        })
        .cloned()
        .collect::<Vec<_>>();
    state.extra.insert("pendingReplayVisuals".into(), json!([]));
    let changed = !delta["board"]["cells"]
        .as_array()
        .expect("cells")
        .is_empty()
        || !delta["fields"].as_array().expect("fields").is_empty()
        || !visuals.is_empty()
        || !unique.is_empty();
    let events = state
        .extra
        .get("replayEvents")
        .and_then(Value::as_array)
        .ok_or_else(|| EngineError::InvalidState("replayEvents must be an array".into()))?;
    let index = events.len() + usize::from(changed);
    if changed {
        let nonce = state
            .extra
            .get("replayEventNonce")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            + 1;
        state.extra.insert("replayEventNonce".into(), json!(nonce));
        let first = unique.first();
        let event = json!({"id":first.map(|n|n["id"].clone()).unwrap_or_else(||json!(format!("replay-{nonce}"))),"label":label,"moveNumber":first.and_then(|n|n["moveNumber"].as_u64()).unwrap_or(u64::from(state.full_move)),"color":first.map(|n|n["color"].clone()).unwrap_or_else(||json!(state.turn)),"notations":unique,"visuals":visuals,"delta":delta});
        state
            .extra
            .get_mut("replayEvents")
            .and_then(Value::as_array_mut)
            .expect("validated events")
            .push(event);
        let timeline = state
            .extra
            .entry("notationTimeline")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                EngineError::InvalidState("notation timeline must be an array".into())
            })?;
        for notation in unique {
            timeline.push(json!({"moveNumber":notation["moveNumber"].as_u64().unwrap_or(u64::from(state.full_move)),"color":notation["color"],"notation":notation,"replayIndex":index}));
        }
    }
    state.extra.insert("replayTailFrame".into(), after);
    entry["replayIndex"] = json!(index);
    state.extra.insert("pendingNotation".into(), Value::Null);
    state.extra.insert("pendingNotations".into(), json!([]));
    let history = state
        .extra
        .get_mut("boardHistory")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState("board history must be an array".into()))?;
    history.push(entry);
    if history.len() > 12 {
        history.drain(..history.len() - 12);
    }
    let preserved = state
        .extra
        .get("historyViewIndex")
        .and_then(Value::as_u64)
        .filter(|n| *n < index as u64)
        .map(|n| json!(n.min(index.saturating_sub(1) as u64)))
        .unwrap_or(Value::Null);
    state.extra.insert("historyViewIndex".into(), preserved);
    Ok(())
}

/// Settle the source's queued terminal record at the atomic snapshot boundary,
/// after card/move bookkeeping has finished. This control flag is not rule
/// snapshot data and is never serialized into a saved source state.
pub(crate) fn settle(state: &mut GameState) -> Result<()> {
    if !std::mem::take(&mut state.gameover_replay_pending)
        || state.mode != "gameover"
        || !state.extra.contains_key("boardHistory")
    {
        return Ok(());
    }
    let pending = state
        .extra
        .get("pendingNotation")
        .is_some_and(|v| !v.is_null())
        || state
            .extra
            .get("pendingNotations")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
        || state
            .extra
            .get("pendingReplayVisuals")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty());
    let before = state
        .extra
        .get("replayTailFrame")
        .filter(|v| !v.is_null())
        .or_else(|| state.extra.get("replayBaseFrame").filter(|v| !v.is_null()));
    let changed = if let Some(before) = before {
        let delta = replay_delta(before, &capture_frame(state)?)?;
        !delta["board"]["cells"]
            .as_array()
            .expect("delta cells")
            .is_empty()
            || !delta["fields"].as_array().expect("delta fields").is_empty()
    } else {
        true
    };
    if pending || changed {
        record(state, "gameover")?;
    }
    Ok(())
}

// Literal metadata extracted from the adopted frozen source, not outcome data.
const SOURCE_METADATA: &str = r##"{
  "frameKeys": [
    "d4",
    "e4",
    "solidarity",
    "bishopInfiltration",
    "synchronization",
    "assembly",
    "vigilance",
    "roller",
    "greekGiftPending",
    "tabooPending",
    "mediumMovement",
    "highwayCells",
    "mode",
    "onlineGameStarted",
    "gameStyle",
    "completeRandom",
    "draftDelete",
    "campaign",
    "turn",
    "board",
    "captures",
    "winner",
    "replayStartedAt",
    "replayEndedAt",
    "replayEndReason",
    "fullMove",
    "moveCount",
    "appliedRuleCard",
    "additionalRuleCards",
    "pendingRuleTickets",
    "effects",
    "palaces",
    "regency",
    "retreat",
    "kingDead",
    "skipTurn",
    "enPassant",
    "enPassantFrenzy",
    "zugzwang",
    "zugzwangConsumesTurn",
    "revolvingDoorGuard",
    "september27CopyPools",
    "cannonScarecrowScreen",
    "internalSixFixes",
    "overtakeTurnOnly",
    "vanguardDiagonalOnly",
    "parrotRookTarget",
    "extinctionMinorTargets",
    "cannonGhostScreen",
    "unifiedJumpObstacles",
    "parrotBasicMovement",
    "september18Balance",
    "othelloPending",
    "miracleSelectsBishop",
    "thiefQuietJump",
    "thiefRequiredJump",
    "thiefRemake",
    "taunt",
    "hallucination",
    "pendingPawnStorm",
    "pendingPanic",
    "pendingFreeMoves",
    "freeMoveCaptureLock",
    "chaosNoCaptureUntilHalfTurn",
    "pendingIcbm",
    "pendingGales",
    "pendingOtherworld",
    "pendingTrojanHorse",
    "pendingBearRetaliations",
    "pendingTrolley",
    "pendingScarecrows",
    "pendingLobsters",
    "armistice",
    "pendingPortals",
    "exhaustion",
    "mistakeCard",
    "democracy",
    "hillKing",
    "genevaConvention",
    "platformRule",
    "activeTrolley",
    "lastMove",
    "idolEncoreUsedByPiece",
    "drawOffer",
    "drawRejection",
    "turnsTaken",
    "cardsUsedThisTurn",
    "royalCommand",
    "actionsRemaining",
    "pendingPromotion",
    "coronation",
    "earlyPromotion",
    "fastGrowth",
    "pawnSprint",
    "pawnLeap",
    "pawnConversion",
    "racingKing",
    "radicalCharge",
    "conscription",
    "conscriptionUsed",
    "encouragement",
    "ironMonarch",
    "imperialStudies",
    "religiousVictory",
    "fianchetto",
    "majesty",
    "infiltration",
    "killerKing",
    "resolve",
    "vanguard",
    "overtake",
    "highlander",
    "falseStart",
    "proficiency",
    "locustSwarm",
    "longEnPassant",
    "disassembly",
    "symmetry",
    "mutation",
    "parrotMovement",
    "freeCastling",
    "switcheroo",
    "substitution",
    "cornerKick",
    "chainBonds",
    "moving",
    "castlingCanceled",
    "castled",
    "bishopSnipe",
    "backwardKnight",
    "trojanHorse",
    "madHorse",
    "clonePassive",
    "clonedPassiveCards",
    "vanishing",
    "knightInjury",
    "fileSurge",
    "rookLift",
    "underpromotion",
    "finalWeapon",
    "afterimageQueen",
    "recycling",
    "breakthroughPawns",
    "quantumPending",
    "highGround",
    "highway",
    "blackHole",
    "winterKingdom",
    "initiative",
    "machoChess",
    "feudalContracts",
    "pendingFeudalStrike",
    "coolGuy",
    "capturedTypes",
    "turnCaptures",
    "lastTurnCaptures",
    "kingKnight",
    "royalKnightKing",
    "knightmate",
    "monochromeChess",
    "acceleration",
    "accelerationPendingFor",
    "accelerationPendingTurns",
    "accelerationStartsAfterBlackTurns",
    "socialism",
    "ultimatum",
    "prophecy",
    "collapsePending",
    "collapsed",
    "collapseDepth",
    "collapsedCells",
    "periodicCollapse",
    "ruleBombs",
    "saturationRule",
    "portalRule",
    "mistakeRule",
    "transcendenceRule",
    "binaMate",
    "overwhelm",
    "camouflageRule",
    "crownRule",
    "conveyorRule",
    "delayedHazards",
    "ruleOpeningEnabled",
    "ruleSelectionEnabled",
    "selectedRuleCardIds",
    "selectedRuleCardId",
    "queensGambitFiles",
    "shotgunDlc",
    "shotgunOpeningColor",
    "firstMoveCardsForced",
    "diceLocks",
    "starWinLimit",
    "deathmatchEnabled",
    "deathmatchLimitTurns",
    "deathmatch",
    "middleDraftDone",
    "endDraftDone",
    "endPhaseStartMove",
    "temporaryQueens",
    "necromancy",
    "accelerationTrail",
    "judgmentExiles",
    "frontlineResponse",
    "relay",
    "fieldPromotion",
    "magicGirlSurge",
    "magicGirlSurgeRefreshPending",
    "moveReplay",
    "gomoku",
    "gomokuVictoryCells",
    "sirenExposure",
    "resolveReady",
    "resolveSpentTurn",
    "resolveMoveCredit",
    "resolveCreditPieceId",
    "reversal",
    "captureTheFlag",
    "pendingRecurrences",
    "undeadResurrections",
    "simpleBoardEditor",
    "captureDisplay"
  ],
  "codes": {
    "king": "K",
    "queen": "Q",
    "rook": "R",
    "bishop": "B",
    "knight": "N",
    "pawn": "",
    "alfil": "A",
    "camel": "C",
    "dragon": "D",
    "fanatic": "F",
    "grasshopper": "G",
    "herald": "H",
    "jester": "J",
    "log": "L",
    "logRolling": "L",
    "merchant": "M",
    "missionary": "MI",
    "protestant": "MS",
    "scarecrow": "S",
    "timeTraveler": "T",
    "pegasus": "U",
    "vampireLord": "V",
    "windmill": "W",
    "windmillBishop": "W",
    "windmillRook": "W",
    "colossus": "X",
    "amazon": "Z",
    "eagle": "AB",
    "assassin": "AS",
    "bigRook": "BR",
    "bigBishop": "BS",
    "bat": "BT",
    "cardinal": "CA",
    "coffin": "CF",
    "checker": "CH",
    "checkerKing": "CH",
    "cannon": "CN",
    "ferz": "FZ",
    "guard": "GD",
    "hook": "HK",
    "knightmaster": "KM",
    "man": "MN",
    "primeMinister": "PM",
    "recruiter": "RC",
    "reaper": "RP",
    "royalKnight": "RN",
    "standardBearer": "SB",
    "shotgunKing": "SK",
    "squire": "SQ",
    "wizard": "WZ",
    "darkWizard": "DW",
    "idol": "ID",
    "lobster": "LB",
    "babyBear": "BB",
    "bear": "BE",
    "vip": "VP",
    "crown": "CR",
    "football": "FB",
    "siegeRam": "SR",
    "magicGirl": "MG",
    "berserker": "BK",
    "slime": "SL",
    "siren": "SI",
    "trickster": "TR",
    "undead": "UD",
    "campfire": "CP",
    "princess": "PR",
    "hedgehog": "HG",
    "monster": "MO",
    "paladin": "PD",
    "octopus": "OC",
    "grappler": "GP",
    "revolvingDoor": "RD",
    "donQuixote": "DQ",
    "medium": "MD",
    "brutus": "BU",
    "clockwork": "CW",
    "parrot": "PA",
    "thief": "TH"
  },
  "labels": {
    "king": "킹",
    "queen": "퀸",
    "rook": "룩",
    "bishop": "비숍",
    "knight": "나이트",
    "pawn": "폰",
    "colossus": "거신병",
    "protestant": "몬시뇰",
    "herald": "전령",
    "cannon": "포",
    "fanatic": "광신도",
    "primeMinister": "국무총리",
    "eagle": "알리바바",
    "amazon": "아마존",
    "cardinal": "추기경",
    "pegasus": "유니콘",
    "jester": "광대",
    "camel": "낙타",
    "log": "통나무",
    "merchant": "상인",
    "hook": "구행",
    "grasshopper": "그래스호퍼",
    "royalKnight": "로얄 나이트",
    "man": "만",
    "assassin": "암살자",
    "guard": "근위병",
    "reaper": "사신",
    "wizard": "마법사",
    "dragon": "드래곤",
    "shotgunKing": "샷건 킹"
  }
}"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_replay_capture_survives_discarded_probe_clones_and_isolates_sessions() {
        let mut live = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..Default::default()
            },
            19,
        )
        .unwrap();
        begin_move(&mut live, Color::White).unwrap();
        let independent = live.clone();
        let wire_before = serde_jcs::to_vec(&live).unwrap();
        let scope = ReplayCaptureScope::for_probe(&live);
        let mut temporary = live.clone();
        scope.attach(&mut temporary);
        temporary.board[0][0]
            .as_mut()
            .unwrap()
            .extra
            .insert("probeBefore".into(), json!(true));
        begin_move(&mut temporary, Color::Black).unwrap();
        drop(temporary);
        // source finally restores the board but does not restore its global
        // pending capture. Dropping the child cannot erase an actual begin.
        scope.carry_into(&mut live).unwrap();
        let capture = active_move_capture(&live).unwrap().unwrap();
        assert_eq!(capture.actor, Color::Black);
        assert_eq!(
            capture.before.board[0][0].as_ref().unwrap().extra["probeBefore"],
            true
        );
        assert!(capture.before.move_replay_scope.is_none());
        assert!(capture.before.active_move_replay_before.is_none());
        assert_eq!(serde_jcs::to_vec(&live).unwrap(), wire_before);
        assert_eq!(
            active_move_capture(&independent).unwrap().unwrap().actor,
            Color::White
        );
        assert!(live.move_replay_scope.is_none());

        // A mismatching actor still consumes the source global pending. A
        // stale clone must read the shared latest value rather than its mirror.
        let mut discarded = live.clone();
        scope.attach(&mut discarded);
        let mut stale = discarded.clone();
        commit_active_move(&mut discarded, Color::White).unwrap();
        assert!(active_move_capture(&stale).unwrap().is_none());
        commit_active_move(&mut stale, Color::Black).unwrap();
        drop(discarded);
        drop(stale);
        scope.carry_into(&mut live).unwrap();
        assert!(active_move_capture(&live).unwrap().is_none());
        assert_eq!(serde_jcs::to_vec(&live).unwrap(), wire_before);
    }

    #[test]
    fn v7_ai_and_threat_replay_use_distinct_source_boundaries() {
        let base = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..Default::default()
            },
            37,
        )
        .unwrap();
        let mut probe = base.clone();
        probe.threat_probe_depth = 1;
        let clock = probe.extra["clock"].clone();
        let mut expected_rng = probe.rng.clone();
        expected_rng.sample().unwrap();
        assert!(
            !queue_notation(
                &mut probe,
                "special",
                Color::White,
                "probe".into(),
                "probe".into(),
                1
            )
            .unwrap()
            .is_null()
        );
        queue_visual(&mut probe, json!({"type":"effect","color":"white"})).unwrap();
        assert_eq!(probe.rng, expected_rng);
        let frames = probe.extra["boardHistory"].as_array().unwrap().len();
        let nonce = crate::observation::number(probe.extra.get("replayEventNonce")).unwrap();
        crate::flow::end_game(&mut probe, Some(Color::White), "probe terminal").unwrap();
        assert!(probe.gameover_replay_pending);
        assert_eq!(probe.extra["clock"], clock);
        settle(&mut probe).unwrap();
        assert_eq!(
            probe.extra["boardHistory"].as_array().unwrap().len(),
            frames
        );
        let last = probe.extra["replayEvents"]
            .as_array()
            .unwrap()
            .last()
            .unwrap();
        assert_eq!(
            last,
            &json!({"id":format!("threat-probe-{}",nonce as u64+1),"label":"gameover","simulation":true})
        );
        assert!(probe.extra["pendingNotation"].is_null());
        assert_eq!(probe.extra["pendingNotations"], json!([]));
        assert_eq!(probe.extra["pendingReplayVisuals"], json!([]));

        let mut ai = base;
        ai.ai_simulation_depth = 1;
        let clock = ai.extra["clock"].clone();
        let rng = ai.rng.clone();
        assert!(
            queue_notation(
                &mut ai,
                "special",
                Color::White,
                "ai".into(),
                "ai".into(),
                1
            )
            .unwrap()
            .is_null()
        );
        queue_visual(&mut ai, json!({"type":"effect"})).unwrap();
        assert_eq!(ai.rng, rng);
        assert_eq!(ai.extra["pendingReplayVisuals"], json!([]));
        ai.board[0][0] = None;
        crate::flow::end_game(&mut ai, Some(Color::White), "ai terminal").unwrap();
        assert!(!ai.gameover_replay_pending);
        assert_eq!(ai.extra["clock"], clock);
        let frames = ai.extra["boardHistory"].as_array().unwrap().len();
        record(&mut ai, "gameover").unwrap();
        assert_eq!(
            ai.extra["boardHistory"].as_array().unwrap().len(),
            frames + 1
        );
        assert!(
            ai.extra["replayEvents"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .get("simulation")
                .is_none()
        );
    }

    #[test]
    fn collapse_visual_and_effect_notation_follow_source_order_and_rng() {
        let mut state = GameState::new(GameConfig::default(), 19).expect("state");
        state.ruleset_id = RULES_VERSION_V7.into();
        state
            .extra
            .insert("activeHistoryMoveNumber".into(), json!(7));
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().expect("one notation ID draw");
        let cells = collapse_ring(1);
        let removed = [(
            json!({"type":"rook","color":"white","moved":false,"id":"rook-1"}),
            0,
            0,
        )];

        queue_collapse_effect(&mut state, Color::Black, 1, &cells, &removed).expect("collapse");
        let visual = &state.extra["pendingReplayVisuals"][0];
        assert_eq!(visual["type"], "collapse");
        assert_eq!(visual["color"], "black");
        assert_eq!(visual["cells"].as_array().expect("ring").len(), 28);
        // JS Number는 정수와 f64 JSON 표현을 구분하지 않는다. normalize의
        // Number(row/col) 결과를 같은 원문 JSON 의미로 비교한다.
        assert!(canonical_equal(&visual["cells"][0], &json!({"row":0,"col":0})).unwrap());
        assert!(canonical_equal(&visual["cells"][27], &json!({"row":7,"col":7})).unwrap());
        assert_eq!(
            visual["removed"],
            json!([{"item":removed[0].0,"row":0,"col":0}])
        );
        let notation = &state.extra["pendingNotations"][0];
        assert_eq!(notation["kind"], "effect");
        assert_eq!(notation["color"], "black");
        assert_eq!(notation["text"], "!붕괴");
        assert_eq!(
            notation["description"],
            "1번째 보드 외곽 붕괴로 기물 1개 제거"
        );
        assert_eq!(notation["moveNumber"], 7);
        assert!(
            notation["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("effect-1790581292828-"))
        );
        assert_eq!(state.extra["pendingNotation"], *notation);
        assert_eq!(state.rng, expected_rng);
    }

    #[test]
    fn collapse_queue_rejects_invalid_ring_without_state_or_rng_change() {
        let mut state = GameState::new(GameConfig::default(), 19).expect("state");
        state.ruleset_id = RULES_VERSION_V7.into();
        let before = state.clone();
        let piece = json!({"type":"rook","color":"white","id":"rook-1"});
        assert!(matches!(
            queue_collapse_effect(
                &mut state,
                Color::White,
                1,
                &[(0, 1), (0, 0)],
                &[(piece.clone(), 0, 0)]
            ),
            Err(EngineError::InvalidState(_))
        ));
        assert_eq!(state, before);
        assert!(matches!(
            queue_collapse_effect(
                &mut state,
                Color::White,
                1,
                &collapse_ring(1),
                &[(piece, 1, 1)]
            ),
            Err(EngineError::InvalidState(_))
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn source_effect_notation_preserves_null_active_move_number() {
        let mut state = GameState::new(GameConfig::default(), 19).expect("state");
        state.ruleset_id = RULES_VERSION_V7.into();
        state
            .extra
            .insert("activeHistoryMoveNumber".into(), Value::Null);
        let notation = queue_special_effect_notation(&mut state, Color::White, "강풍", "백 강풍")
            .expect("effect notation");
        assert_eq!(notation["text"], "!강풍");
        assert_eq!(notation["moveNumber"], 0);
    }

    #[test]
    fn replay_delta_uses_actual_board_shape_and_rejects_unproved_resize() {
        let before = json!({"board":[[null,null,null],[null,null,null]]});
        let after = json!({"board":[[null,null,null],[null,null,{"id":"piece"}]]});
        let delta = replay_delta(&before, &after).unwrap();
        assert_eq!(delta["board"]["before"], json!({"rows":2,"cols":3}));
        assert_eq!(delta["board"]["after"], json!({"rows":2,"cols":3}));
        assert_eq!(
            delta["board"]["cells"],
            json!([{"row":1,"col":2,"before":null,"after":{"id":"piece"}}])
        );
        let larger_before = vec![vec![Value::Null; 9]; 9];
        let mut larger_after = larger_before.clone();
        larger_after[8][8] = json!({"id":"outside-old-scan"});
        let larger = replay_delta(
            &json!({"board":larger_before}),
            &json!({"board":larger_after}),
        )
        .unwrap();
        assert_eq!(larger["board"]["before"], json!({"rows":9,"cols":9}));
        assert_eq!(
            larger["board"]["cells"],
            json!([{"row":8,"col":8,"before":null,"after":{"id":"outside-old-scan"}}])
        );
        assert!(matches!(
            replay_delta(&before, &json!({"board":[[null,null],[null,null]]})),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert!(matches!(
            replay_delta(&before, &json!({"board":[[null],[null,null]]})),
            Err(EngineError::InvalidState(_))
        ));
        for empty_board in [json!([]), json!([[]])] {
            assert!(matches!(
                replay_delta(&before, &json!({"board":empty_board})),
                Err(EngineError::InvalidState(_))
            ));
        }
    }
}
