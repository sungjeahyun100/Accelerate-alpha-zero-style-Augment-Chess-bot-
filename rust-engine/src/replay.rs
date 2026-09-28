//! Source replay, notation, and move rollback state. Replay deltas are rule
//! state: replayMove validates and restores them, so they cannot be discarded
//! as presentation metadata. All identifiers share the gameplay RNG stream.
use crate::*;
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::OnceLock};

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

/// applyWinterFreezeCycle runs during endMove, including when Winter Kingdom
/// is disabled. It reconstructs this object; replay uses JSON.stringify, so
/// property order matters even when all values are unchanged.
pub(crate) fn normalize_winter_after_turn(state: &mut GameState) -> Result<()> {
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
/// Keep this frame-order boundary after an action without changing rule values.
pub(crate) fn canonicalize_position_frames(state: &mut GameState) -> Result<()> {
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
    let Some(actor) = piece.color.owner() else {
        return Ok(());
    };
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
        for candidate in state
            .board
            .iter_mut()
            .flatten()
            .flatten()
            .filter(|p| p.id == piece.id)
        {
            candidate.extra.insert("evasion".into(), json!(true));
        }
    }
    Ok(())
}
pub(crate) fn queue_visual(state: &mut GameState, visual: Value) -> Result<()> {
    let list = state
        .extra
        .entry("pendingReplayVisuals")
        .or_insert_with(|| json!([]));
    list.as_array_mut()
        .ok_or_else(|| EngineError::InvalidState("pending replay visuals must be an array".into()))?
        .push(visual);
    Ok(())
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
    let concealed = piece.color.owner().is_some_and(|owner| {
        let viewer = owner.opponent();
        let square = square.or_else(|| {
            (0..8)
                .flat_map(|row| (0..8).map(move |col| Square { row, col }))
                .find(|&square| state.at(square).is_some_and(|item| item.id == piece.id))
        });
        square.is_none_or(|square| {
            privacy
                .and_then(|v| v.get(viewer.as_str()))
                .and_then(|v| v.get("originVisible"))
                == Some(&json!(false))
                || !state.piece_visible(piece, square, viewer)
        })
    });
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
fn queue_notation(
    state: &mut GameState,
    kind: &str,
    color: Color,
    text: String,
    description: String,
    move_number: u64,
) -> Result<Value> {
    let suffix = crate::draft::random_suffix(state.rng.sample()?)?;
    let id = format!(
        "{kind}-{}-{}",
        crate::draft::frozen_timestamp()?,
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
fn piece_code(kind: &str) -> String {
    metadata()["codes"][kind]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| kind.chars().take(2).collect::<String>().to_uppercase())
}
pub(crate) fn piece_label(kind: &str) -> &str {
    metadata()["labels"][kind].as_str().unwrap_or(kind)
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
                    || piece_code(&other.kind) != piece_code(&piece.kind)
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
            piece_code(&piece.kind),
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
                    piece_code(&piece.kind)
                } else {
                    "?".into()
                }
            )
        } else if known {
            format!("{}??", piece_code(&piece.kind))
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
    let before = state.clone();
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
    Ok(before)
}
fn board_delta(before: &Value, after: &Value) -> Result<Vec<Value>> {
    let mut cells = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
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
    for key in metadata()["frameKeys"]
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
    Ok(clone_replay(&Value::Object(frame)))
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
    Ok(
        json!({"board":{"before":{"rows":8,"cols":8},"after":{"rows":8,"cols":8},"cells":board_delta(&before["board"],&after["board"])?},"fields":fields}),
    )
}
fn notation_key(event: &Value) -> String {
    event["id"].as_str().unwrap_or("").to_owned()
}
pub(crate) fn record(state: &mut GameState, label: &str) -> Result<()> {
    if !state.extra.contains_key("boardHistory") {
        return Ok(());
    }
    if state
        .extra
        .get("chainBonds")
        .and_then(Value::as_array)
        .is_some_and(|v| !v.is_empty())
    {
        return Err(EngineError::UnsupportedFeature(
            "record-time chain bond breakage".into(),
        ));
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
