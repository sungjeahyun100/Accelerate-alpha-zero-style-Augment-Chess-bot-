//! Turn and move-window ACTIVE card objects from the pinned v7 client.
//!
//! Source: `main-OahWs0tU.js`, SHA-256
//! `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`.
//! The shared caller owns card use, settlement, replay and turn completion.
//! Each unsupported path is explicit until its source transition is verified.

use super::*;
use serde_json::{Value, json};

pub(super) const IDS: &[&str] = &[
    "armistice",
    "breakthrough-order",
    "desperado",
    "en-passant-bang",
    "premove",
    "royal-command",
    "snipe",
    "socialism",
    "substitution",
    "switcheroo",
    "zugzwang",
    "replay",
    "reversal",
    "overtake",
    "disassembly",
    "e4",
    "bishop-infiltration",
    "solidarity",
    "roller",
];

fn admit(state: &GameState, card: &CardSlot) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 || !IDS.contains(&card.id.as_str()) {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 turn card {} on rules version {}",
            card.id, state.ruleset_id
        )));
    }
    let definition = crate::card_registry::validate_instance(state, card)?;
    if definition.effect != card.effect {
        return Err(EngineError::InvalidState(format!(
            "v7 turn card {} effect identity drift",
            card.id
        )));
    }
    if catalog_hash(state).is_some_and(|hash| hash != CATALOG) {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 turn card {} unverified historical catalog",
            card.id
        )));
    }
    Ok(())
}

fn board_squares(state: &GameState) -> Result<Vec<Square>> {
    let mut squares = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for col in 0..cells.len() {
            squares.push(Square {
                row: u8::try_from(row).map_err(|_| {
                    EngineError::InvalidState("v7 turn card board has too many rows".into())
                })?,
                col: u8::try_from(col).map_err(|_| {
                    EngineError::InvalidState("v7 turn card board has too many columns".into())
                })?,
            });
        }
    }
    Ok(squares)
}

fn has_own_type(state: &GameState, kind: &str) -> bool {
    state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|piece| piece.color == state.turn && piece.kind == kind)
}

/// Source `hasSubstitutionCandidate` scans board identities and asks whether
/// at least one allied piece can swap with an enemy piece of the same physical
/// type. Slime ability locks both ends, even when the physical type differs.
fn has_substitution_candidate(state: &GameState) -> bool {
    state.board.iter().flatten().flatten().any(|moving| {
        moving.color == state.turn
            && !matches!(
                moving.kind.as_str(),
                "wall" | "football" | "monster" | "blackHole"
            )
            && moving.ability_kind() != "slime"
            && state.board.iter().flatten().flatten().any(|target| {
                target.color == state.turn.opponent()
                    && target.kind == moving.kind
                    && target.ability_kind() != "slime"
            })
    })
}

fn desperado_target(state: &GameState, piece: &Piece) -> bool {
    desperado_candidate(state, piece)
}

fn reversal_target(state: &GameState, piece: &Piece) -> bool {
    piece.color == state.turn && minor(state, piece) && !state.royal_identity(piece)
}

fn candidate_squares(state: &GameState, card: &CardSlot) -> Result<Vec<Square>> {
    let mut result = Vec::new();
    for square in board_squares(state)? {
        if let Some(piece) = state.at(square)
            && match card.id.as_str() {
                "desperado" => desperado_target(state, piece),
                "reversal" => reversal_target(state, piece),
                _ => false,
            }
        {
            result.push(square);
        }
    }
    Ok(result)
}

/// The first free-move click uses `isValidTarget`, which normalizes each board
/// cell to its piece origin and checks physical type and owner. It does not
/// check whether that piece has a declaration move until the next click.
fn premove_origin_squares(state: &GameState) -> Result<Vec<Square>> {
    let mut result = Vec::new();
    for square in board_squares(state)? {
        if let Some(piece) = state.at(square)
            && piece.color == state.turn
            && !["wall", "football", "blackHole", "coffin"].contains(&piece.kind.as_str())
        {
            result.push(square);
        }
    }
    Ok(result)
}

/// Source `freeMove` stamps one pending entry after at least one declaration
/// survived validation. The frozen oracle pins `Date.now()` to the adopted
/// baseline and takes the suffix from the same Position RNG as other effects.
fn premove_plan_id(state: &mut GameState) -> Result<String> {
    let timestamp = crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?;
    let suffix =
        crate::draft::random_suffix(state.rng.sample_opaque("source premove plan identity")?)?;
    Ok(format!(
        "premove-{timestamp}-{}",
        suffix.chars().take(6).collect::<String>()
    ))
}

fn premove_declaration_targets(
    state: &GameState,
    piece: &Piece,
    origin: Square,
) -> Result<Vec<MoveTarget>> {
    crate::movement::v7_free_move_declaration_targets(state, piece, origin)
}

/// Ordered plan groups for the shared lazy candidate cursor. A group belongs
/// to one board identity and retains the declaration descriptor order; the
/// caller generates source DFS prefixes with at most three distinct groups.
pub(super) fn free_move_target_groups(
    state: &GameState,
    card: &CardSlot,
) -> Result<Vec<Vec<Value>>> {
    admit(state, card)?;
    if card.id != "premove" {
        return Err(EngineError::InvalidState(
            "free-move target groups require the premove card".into(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut groups = Vec::new();
    for square in board_squares(state)? {
        let origin = normalize_square(state, square);
        let Some(piece) = state.at(origin).filter(|piece| {
            piece.color == state.turn
                && !["wall", "football", "blackHole", "coffin"].contains(&piece.kind.as_str())
        }) else {
            continue;
        };
        if !seen.insert(piece.id.clone()) {
            continue;
        }
        let declarations = premove_declaration_targets(state, piece, origin)?;
        if declarations.is_empty() {
            continue;
        }
        groups.push(
            declarations
                .into_iter()
                .map(|declaration| json!({"from":origin,"to":declaration.square()}))
                .collect(),
        );
    }
    Ok(groups)
}

fn premove_square(row: Option<&Value>, col: Option<&Value>) -> Option<Square> {
    let row = js_number(row, 0)?;
    let col = js_number(col, 0)?;
    if !row.is_finite()
        || !col.is_finite()
        || row.fract() != 0.0
        || col.fract() != 0.0
        || !(0.0..8.0).contains(&row)
        || !(0.0..8.0).contains(&col)
    {
        return None;
    }
    Some(Square {
        row: row as u8,
        col: col as u8,
    })
}

fn premove_contains_square(target: &MoveTarget, square: Square) -> bool {
    // Source moveContainsSquare prefers the first present cell-list property,
    // including an empty list; falling through would admit a different click.
    for key in ["bodyCells", "displayCells", "highlightCells", "sectorCells"] {
        if let Some(cells) = target.flags.get(key).and_then(Value::as_array) {
            return cells.iter().any(|cell| {
                cell.get("row").and_then(Value::as_f64) == Some(f64::from(square.row))
                    && cell.get("col").and_then(Value::as_f64) == Some(f64::from(square.col))
            });
        }
    }
    target.square() == square
}

fn premove_trace_cells(target: &MoveTarget) -> Result<Vec<Square>> {
    if let Some(cells) = target.flags.get("highlightCells").and_then(Value::as_array)
        && !cells.is_empty()
    {
        let mut seen = std::collections::BTreeSet::new();
        let mut trace = Vec::new();
        for cell in cells {
            let square = premove_square(cell.get("row"), cell.get("col")).ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 premove movement trace contains an invalid square".into(),
                )
            })?;
            if seen.insert(square) {
                trace.push(square);
                if trace.len() == 4 {
                    break;
                }
            }
        }
        return Ok(trace);
    }
    Ok(vec![target.square()])
}

fn apply_premove(state: &mut GameState, target: Option<&Value>) -> Result<()> {
    let selections = target
        .and_then(|target| target.get("selections"))
        .and_then(Value::as_array)
        .filter(|selections| !selections.is_empty())
        .ok_or(EngineError::IllegalAction)?;
    let actor = state.turn;
    let mut seen = std::collections::BTreeSet::new();
    let mut plans = Vec::new();
    // Source slices requested selections before validation. Invalid entries
    // consume one of these three positions; a fourth entry cannot replace one.
    for selection in selections.iter().take(3) {
        let from_field = |name| {
            selection
                .get("from")
                .and_then(|from| from.get(name))
                .filter(|value| !value.is_null())
                .or_else(|| selection.get(name))
        };
        let Some(from) = premove_square(from_field("row"), from_field("col")) else {
            continue;
        };
        let Some(to) = premove_square(
            selection.get("to").and_then(|to| to.get("row")),
            selection.get("to").and_then(|to| to.get("col")),
        ) else {
            continue;
        };
        let origin = normalize_square(state, from);
        let Some(piece) = state.at(origin).filter(|piece| {
            piece.color == actor
                && !piece.id.is_empty()
                && !seen.contains(&piece.id)
                && !["wall", "football", "blackHole", "coffin"].contains(&piece.kind.as_str())
        }) else {
            continue;
        };
        let declarations = premove_declaration_targets(state, piece, origin)?;
        let Some(declaration) = declarations
            .iter()
            .find(|declaration| premove_contains_square(declaration, to))
        else {
            continue;
        };
        seen.insert(piece.id.clone());
        let from_cells = if piece.is_large() {
            [(0, 0), (0, 1), (1, 0), (1, 1)]
                .into_iter()
                .filter_map(|(dr, dc)| origin.offset(dr, dc))
                .collect::<Vec<_>>()
        } else {
            vec![origin]
        };
        plans.push(json!({
            "pieceId":piece.id,
            "pieceType":piece.kind,
            "from":origin,
            "to":declaration.square(),
            "fromCells":from_cells,
            "toCells":premove_trace_cells(declaration)?,
        }));
    }
    if plans.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    if !state
        .extra
        .get("pendingFreeMoves")
        .is_some_and(Value::is_array)
    {
        state.extra.insert("pendingFreeMoves".into(), json!([]));
    }
    let trigger_color = actor.opponent();
    let id = premove_plan_id(state)?;
    let entry = json!({
        "id":id,"color":actor,"triggerColor":trigger_color,
        "triggerTurn":u64::from(*state.turns_taken.get(trigger_color)) + 1,
        "moves":plans,
    });
    state
        .extra
        .get_mut("pendingFreeMoves")
        .and_then(Value::as_array_mut)
        .expect("premove pending array initialized")
        .push(entry);
    Ok(())
}

/// Frozen getTargetSquares, before the later legality and effect probe.
pub(super) fn ui_targets(state: &GameState, card: &CardSlot) -> Result<Option<Vec<Square>>> {
    if !IDS.contains(&card.id.as_str()) {
        return Ok(None);
    }
    admit(state, card)?;
    match card.id.as_str() {
        "desperado" | "reversal" => candidate_squares(state, card).map(Some),
        "premove" => premove_origin_squares(state).map(Some),
        _ => Ok(Some(Vec::new())),
    }
}

pub(super) fn actions(state: &GameState, card: &CardSlot) -> Result<Option<Vec<Action>>> {
    if !IDS.contains(&card.id.as_str()) {
        return Ok(None);
    }
    admit(state, card)?;
    if card.id == "premove" {
        let mut cursor = source_card_candidate_cursor(state, card)?.ok_or_else(|| {
            EngineError::InvalidState("v7 premove source cursor is missing".into())
        })?;
        let mut offered = Vec::new();
        while let Some(candidate) = cursor.next_candidate() {
            if offered.len() == 4096 {
                return Err(EngineError::UnsupportedFeature(
                    "v7 premove actions exceed the eager limit 4096; use the paged source cursor"
                        .into(),
                ));
            }
            offered.push(candidate);
        }
        return Ok(Some(offered));
    }
    let candidates = if matches!(card.id.as_str(), "desperado" | "reversal") {
        candidate_squares(state, card)?
            .into_iter()
            .map(|square| Action::card(state.turn, card, Some(json!(square))))
            .collect::<Vec<_>>()
    } else {
        vec![Action::card(state.turn, card, None)]
    };
    let mut ready = Vec::new();
    for candidate in candidates {
        let mut staged = state.clone();
        match apply(&mut staged, card, &candidate) {
            Ok(_) => ready.push(candidate),
            Err(EngineError::IllegalAction) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(Some(ready))
}

pub(super) fn apply(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    admit(state, card)?;
    if action.kind != ActionKind::Card
        || action.color != state.turn
        || action.card_id.as_deref() != Some(card.id.as_str())
        || action.card_instance_id.as_deref() != Some(card.instance_id.as_str())
        || action.from.is_some()
        || action.destination.is_some()
        || !action.extra.is_empty()
    {
        return Err(EngineError::IllegalAction);
    }
    // Source effect probes operate on their already isolated simulation state.
    // A virtual retry observes mutations and draws made before {ok:false}.
    if state.is_ai_simulation() {
        return apply_direct(state, card, action);
    }
    let mut staged = state.clone();
    let captured = apply_direct(&mut staged, card, action)?;
    *state = staged;
    Ok(captured)
}

fn set_color_value(state: &mut GameState, key: &str, color: Color, value: Value) -> Result<()> {
    let entry = state
        .extra
        .entry(key.to_owned())
        .or_insert_with(|| json!({"white":false,"black":false}));
    let sides = entry
        .as_object_mut()
        .ok_or_else(|| EngineError::InvalidState(format!("v7 {key} must be a color map")))?;
    sides.insert(color.as_str().into(), value);
    Ok(())
}

// Source20824 and source638 spread the existing value, then add only the
// acting color. An absent side stays absent in these sparse state maps.
fn set_sparse_color_value(
    state: &mut GameState,
    key: &str,
    color: Color,
    value: Value,
) -> Result<()> {
    let mut sides = match state.extra.get(key) {
        Some(Value::Object(sides)) => sides.clone(),
        Some(Value::Array(entries)) => entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (index.to_string(), entry.clone()))
            .collect(),
        Some(Value::String(text)) => {
            let mut fields = serde_json::Map::new();
            for (index, unit) in text.encode_utf16().enumerate() {
                let character = char::from_u32(u32::from(unit)).ok_or_else(|| {
                    EngineError::UnsupportedFeature(format!(
                        "v7 {key} object spread contains an unpaired UTF-16 code unit"
                    ))
                })?;
                fields.insert(index.to_string(), Value::String(character.to_string()));
            }
            fields
        }
        _ => serde_json::Map::new(),
    };
    sides.insert(color.as_str().into(), value);
    state.extra.insert(key.into(), Value::Object(sides));
    Ok(())
}

fn selected_target(state: &GameState, card: &CardSlot, action: &Action) -> Result<Square> {
    let square: Square =
        serde_json::from_value(action.target.clone().ok_or(EngineError::IllegalAction)?)
            .map_err(EngineError::serialization)?;
    let piece = state.at(square).ok_or(EngineError::IllegalAction)?;
    let eligible = match card.id.as_str() {
        "desperado" => desperado_target(state, piece),
        "reversal" => reversal_target(state, piece),
        _ => false,
    };
    if !eligible {
        return Err(EngineError::IllegalAction);
    }
    Ok(square)
}

fn no_target(action: &Action) -> Result<()> {
    if action.target.as_ref().is_some_and(|value| !value.is_null()) {
        Err(EngineError::IllegalAction)
    } else {
        Ok(())
    }
}

/// Source `zugzwangKingMoves` probes every enemy royal without replacing
/// state.turn. The option suppresses only the forced-turn constraint and the
/// descriptor filter excludes attacks or actions which do not move the king.
fn zugzwang_available(state: &GameState, color: Color) -> Result<bool> {
    for square in board_squares(state)? {
        let Some(piece) = state
            .at(square)
            .filter(|piece| piece.color == color && state.royal_identity(piece))
        else {
            continue;
        };
        let targets = crate::movement::v7_legal_move_targets(
            state,
            piece,
            square,
            crate::movement::V7MoveOptions {
                ignore_forced_turn_move: true,
                ..Default::default()
            },
        )?;
        if targets.iter().any(|target| {
            ![
                "colossusAttack",
                "shotgunBlast",
                "shotgunSnipe",
                "merchantBuy",
                "setLogDirection",
            ]
            .iter()
            .any(|field| target.flag(field))
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

// The frozen client uses JSON.stringify for the replay board and color-state
// comparisons. Preserve object insertion order while formatting number leaves
// as JavaScript does, including 0.0 -> 0.
fn replay_stringify(value: &Value) -> Result<String> {
    match value {
        Value::Object(map) => Ok(format!(
            "{{{}}}",
            map.iter()
                .map(|(key, field)| {
                    Ok(format!(
                        "{}:{}",
                        serde_json::to_string(key).map_err(EngineError::serialization)?,
                        replay_stringify(field)?
                    ))
                })
                .collect::<Result<Vec<_>>>()?
                .join(",")
        )),
        Value::Array(list) => Ok(format!(
            "[{}]",
            list.iter()
                .map(replay_stringify)
                .collect::<Result<Vec<_>>>()?
                .join(",")
        )),
        _ => String::from_utf8(serde_jcs::to_vec(value).map_err(EngineError::serialization)?)
            .map_err(|error| EngineError::InvalidState(error.to_string())),
    }
}

fn replay_value_equal(left: &Value, right: &Value) -> Result<bool> {
    Ok(replay_stringify(left)? == replay_stringify(right)?)
}

fn replay_cell_key(value: &Value) -> String {
    if value.is_null() {
        return String::new();
    }
    value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            format!(
                "{}:{}",
                value.get("color").and_then(Value::as_str).unwrap_or(""),
                value.get("type").and_then(Value::as_str).unwrap_or("")
            )
        })
}

// Source replay converts Ultimatum IDs with String(), including IDs from an
// imported snapshot. This conversion does not mutate piece identities.
fn replay_id_text(value: &Value) -> Result<String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Null => Ok("null".into()),
        Value::Object(_) => Ok("[object Object]".into()),
        Value::Array(values) => Ok(values
            .iter()
            .map(|value| {
                if value.is_null() {
                    Ok(String::new())
                } else {
                    replay_id_text(value)
                }
            })
            .collect::<Result<Vec<_>>>()?
            .join(",")),
        _ => replay_stringify(value),
    }
}

fn replay_delta_coordinate(value: Option<&Value>) -> Option<usize> {
    // Source board-delta coordinates are JavaScript Numbers. JSON 3 and 3.0
    // address the same array cell; strings are not part of that numeric DTO.
    let number = value?.as_f64()?;
    if !number.is_finite()
        || number.fract() != 0.0
        || number < 0.0
        || number > 9_007_199_254_740_991.0
        || number > usize::MAX as f64
    {
        return None;
    }
    Some(number as usize)
}

fn replay_available(state: &GameState, color: Color) -> Result<bool> {
    let replay = state
        .extra
        .get("moveReplay")
        .and_then(|value| value.get(color.as_str()))
        .unwrap_or(&Value::Null);
    let Some(delta) = replay.get("delta").and_then(Value::as_array) else {
        return Ok(false);
    };
    if delta.is_empty() {
        return Ok(false);
    }
    for entry in delta {
        let (Some(row), Some(col)) = (
            replay_delta_coordinate(entry.get("row")),
            replay_delta_coordinate(entry.get("col")),
        ) else {
            return Ok(false);
        };
        let current = state
            .board
            .get(row)
            .and_then(|cells| cells.get(col))
            .and_then(Option::as_ref)
            .map(serde_json::to_value)
            .transpose()
            .map_err(EngineError::serialization)?
            .unwrap_or(Value::Null);
        let after = &entry["after"];
        if replay_cell_key(&current) != replay_cell_key(after)
            || !replay_value_equal(&current, after)?
        {
            return Ok(false);
        }
    }
    let expected = match replay
        .get("capturesAfter")
        .filter(|value| truthy(Some(value)))
    {
        Some(Value::Array(values)) => values.as_slice(),
        None => &[],
        Some(_) => return Ok(false),
    };
    let current = state.captures.get(color);
    if current.len() < expected.len() {
        return Ok(false);
    }
    for (piece, expected) in current.iter().zip(expected) {
        let piece = serde_json::to_value(piece).map_err(EngineError::serialization)?;
        if replay_cell_key(&piece) != replay_cell_key(expected) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn restore_replay_color(
    state: &mut GameState,
    replay: &Value,
    color: Color,
    state_key: &str,
    before_key: &str,
    after_key: &str,
) -> Result<()> {
    let (Some(before), Some(after)) = (replay.get(before_key), replay.get(after_key)) else {
        return Ok(());
    };
    if replay_value_equal(&before[color.as_str()], &after[color.as_str()])? {
        return Ok(());
    }
    let state_value = state
        .extra
        .entry(state_key.to_owned())
        .or_insert_with(|| json!({}));
    if !state_value.is_object() {
        *state_value = json!({});
    }
    let sides = state_value.as_object_mut().expect("replay color map");
    if let Some(value) = before.get(color.as_str()) {
        sides.insert(color.as_str().into(), value.clone());
    } else {
        // clonePlain(undefined) creates an omitted property in source JSON.
        sides.remove(color.as_str());
    }
    Ok(())
}

fn replay_move(state: &mut GameState, color: Color) -> Result<()> {
    if !replay_available(state, color)? {
        return Err(EngineError::IllegalAction);
    }
    let replay = state.extra["moveReplay"][color.as_str()].clone();
    let delta = replay["delta"]
        .as_array()
        .ok_or_else(|| EngineError::InvalidState("v7 replay delta must be an array".into()))?;
    let mut restored_by_id = std::collections::BTreeMap::<String, Piece>::new();
    for entry in delta {
        let (row, col) = (
            replay_delta_coordinate(entry.get("row")).ok_or(EngineError::IllegalAction)?,
            replay_delta_coordinate(entry.get("col")).ok_or(EngineError::IllegalAction)?,
        );
        if let Some(cell) = state
            .board
            .get_mut(row)
            .and_then(|cells| cells.get_mut(col))
        {
            *cell = None;
        }
    }
    for entry in delta {
        if entry["before"].is_null() {
            continue;
        }
        let (row, col) = (
            replay_delta_coordinate(entry.get("row")).ok_or(EngineError::IllegalAction)?,
            replay_delta_coordinate(entry.get("col")).ok_or(EngineError::IllegalAction)?,
        );
        let mut restored: Piece =
            serde_json::from_value(entry["before"].clone()).map_err(EngineError::serialization)?;
        if !restored.id.is_empty() {
            restored = restored_by_id
                .entry(restored.id.clone())
                .or_insert(restored)
                .clone();
        }
        if let Some(cell) = state
            .board
            .get_mut(row)
            .and_then(|cells| cells.get_mut(col))
        {
            *cell = Some(restored);
        }
    }
    // restoreReplayBoardDelta finishes with relinkBoardPieceReferences: cells
    // left outside the delta still share the first row-major identity snapshot.
    let mut board_by_id = std::collections::BTreeMap::<String, Piece>::new();
    for piece in state.board.iter_mut().flatten().flatten() {
        if !piece.id.is_empty() {
            *piece = board_by_id
                .entry(piece.id.clone())
                .or_insert_with(|| piece.clone())
                .clone();
        }
    }
    let before_captures: Vec<Piece> = serde_json::from_value(
        replay
            .get("capturesBefore")
            .filter(|value| truthy(Some(value)))
            .cloned()
            .unwrap_or_else(|| json!([])),
    )
    .map_err(EngineError::serialization)?;
    let after_count = replay["capturesAfter"].as_array().map_or(0, Vec::len);
    let retained = state.captures.get(color)[after_count..].to_vec();
    *state.captures.get_mut(color) = before_captures.into_iter().chain(retained).collect();

    let current_last_move = state.extra.get("lastMove").cloned().unwrap_or(Value::Null);
    if replay.get("lastMoveAfter").is_some()
        && replay_value_equal(&current_last_move, &replay["lastMoveAfter"])?
    {
        state.en_passant = serde_json::from_value(replay["enPassantBefore"].clone())
            .map_err(EngineError::serialization)?;
        state
            .extra
            .insert("lastMove".into(), replay["previousLastMove"].clone());
        state.extra.insert(
            "accelerationTrail".into(),
            replay["accelerationTrailBefore"].clone(),
        );
    }
    for (state_key, before_key, after_key) in [
        ("castled", "castledBefore", "castledAfter"),
        ("zugzwang", "zugzwangBefore", "zugzwangAfter"),
        (
            "quantumPending",
            "quantumPendingBefore",
            "quantumPendingAfter",
        ),
        ("switcheroo", "switcherooBefore", "switcherooAfter"),
        ("moving", "movingBefore", "movingAfter"),
        ("exhaustion", "exhaustionBefore", "exhaustionAfter"),
    ] {
        restore_replay_color(state, &replay, color, state_key, before_key, after_key)?;
    }
    if replay.get("crownRuleBefore").is_some()
        && replay.get("crownRuleAfter").is_some()
        && !replay_value_equal(&replay["crownRuleBefore"], &replay["crownRuleAfter"])?
    {
        state
            .extra
            .insert("crownRule".into(), replay["crownRuleBefore"].clone());
    }
    if state.extra.get("ultimatum").is_some_and(Value::is_object)
        && replay["ultimatumBefore"].is_object()
        && replay["ultimatumAfter"].is_object()
    {
        let before = replay["ultimatumBefore"]["movedIds"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let after = replay["ultimatumAfter"]["movedIds"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let previous = before
            .iter()
            .map(replay_id_text)
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        let added = after
            .iter()
            .map(replay_id_text)
            .collect::<Result<std::collections::BTreeSet<_>>>()?
            .difference(&previous)
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        if !added.is_empty() {
            let current = state.extra["ultimatum"]["movedIds"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let retained = current
                .into_iter()
                .map(|value| replay_id_text(&value))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .filter(|id| !added.contains(id))
                .collect::<Vec<_>>();
            state.extra.get_mut("ultimatum").expect("checked object")["movedIds"] = json!(retained);
        }
    }
    state
        .extra
        .get_mut("moveReplay")
        .ok_or(EngineError::IllegalAction)?[color.as_str()] = Value::Null;
    crate::replay::queue_special_effect_notation(
        state,
        color,
        "리플레이",
        &format!("{}의 마지막 기물 이동 취소", crate::replay::label(color)),
    )?;
    Ok(())
}

fn apply_direct(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let actor = state.turn;
    match card.id.as_str() {
        "armistice" => {
            no_target(action)?;
            state.extra.insert(
                "armistice".into(),
                json!({"remaining":2,"by":actor,"actedColors":[]}),
            );
        }
        "breakthrough-order" => {
            no_target(action)?;
            if !has_own_type(state, "pawn") {
                return Err(EngineError::IllegalAction);
            }
            set_color_value(state, "breakthroughPawns", actor, json!(true))?;
        }
        "desperado" => {
            let square = selected_target(state, card, action)?;
            let mut piece = state.at(square).ok_or(EngineError::IllegalAction)?.clone();
            piece
                .extra
                .insert("desperado".into(), json!({"remaining":2}));
            write_piece(state, &piece);
            mark_animation(state, &piece)?;
            if crate::movement::v7_legal_move_targets(
                state,
                &piece,
                square,
                crate::movement::V7MoveOptions::default(),
            )?
            .is_empty()
            {
                return Err(EngineError::IllegalAction);
            }
        }
        "en-passant-bang" => {
            no_target(action)?;
            if !has_own_type(state, "pawn") {
                return Err(EngineError::IllegalAction);
            }
            set_color_value(state, "enPassantFrenzy", actor, json!(true))?;
        }
        "royal-command" => {
            no_target(action)?;
            // The effect itself is unconditional, but source public admission
            // checks `hasTargetableNonKingPiece` before dispatching it.
            let own_nonroyal = state.board.iter().flatten().flatten().any(|piece| {
                piece.color == actor
                    && !state.royal_identity(piece)
                    && !["wall", "football", "colossus"].contains(&piece.kind.as_str())
            });
            if !own_nonroyal {
                return Err(EngineError::IllegalAction);
            }
            state
                .extra
                .entry("royalCommand")
                .or_insert_with(|| json!({"white":null,"black":null}));
            let active = u64::from(*state.turns_taken.get(actor)) + 1;
            set_color_value(
                state,
                "royalCommand",
                actor,
                json!({"activeTurn":active,"expiresTurn":active+2}),
            )?;
        }
        "snipe" => {
            no_target(action)?;
            if !has_own_type(state, "bishop") {
                return Err(EngineError::IllegalAction);
            }
            set_color_value(state, "bishopSnipe", actor, json!(true))?;
        }
        "socialism" => {
            no_target(action)?;
            let enemy = actor.opponent();
            let current = js_number(
                state
                    .extra
                    .get("socialism")
                    .and_then(|sides| sides.get(enemy.as_str())),
                0,
            )
            .ok_or_else(|| EngineError::InvalidState("v7 socialism enemy count missing".into()))?;
            if !current.is_finite() {
                return Err(EngineError::InvalidState(
                    "v7 socialism enemy count is not finite".into(),
                ));
            }
            set_color_value(state, "socialism", enemy, json!(current.max(1.0)))?;
        }
        "zugzwang" => {
            no_target(action)?;
            let enemy = actor.opponent();
            if !zugzwang_available(state, enemy)? {
                return Err(EngineError::IllegalAction);
            }
            set_color_value(state, "zugzwang", enemy, json!(true))?;
        }
        "substitution" => {
            no_target(action)?;
            if !has_substitution_candidate(state) {
                return Err(EngineError::IllegalAction);
            }
            set_color_value(state, "substitution", actor, json!(true))?;
        }
        "replay" => {
            no_target(action)?;
            replay_move(state, actor)?;
        }
        "switcheroo" => {
            no_target(action)?;
            // Source `findMobileKing` tests only the first royal recipient in
            // board order; a later king cannot bypass an earlier bunker king.
            let mobile_king = state
                .board
                .iter()
                .flatten()
                .flatten()
                .find(|piece| piece.color == actor && state.royal_identity(piece))
                .is_some_and(|piece| {
                    !(piece.flag("undergroundBunker") && number_is_finite(piece.extra.get("hp")))
                });
            if !mobile_king || !has_own_type(state, "pawn") {
                return Err(EngineError::IllegalAction);
            }
            set_color_value(state, "switcheroo", actor, json!(true))?;
        }
        "reversal" => {
            if state.flag("reversal", actor) {
                return Err(EngineError::IllegalAction);
            }
            let square = selected_target(state, card, action)?;
            let removed = crate::transition::sacrifice(state, square, actor.opponent())?
                .ok_or(EngineError::IllegalAction)?;
            set_color_value(state, "reversal", actor, json!(true))?;
            return Ok(vec![removed]);
        }
        "overtake" => {
            no_target(action)?;
            if state.flag(&card.effect, actor) {
                return Err(EngineError::IllegalAction);
            }
            set_color_value(state, &card.effect, actor, json!(true))?;
        }
        "disassembly" | "e4" | "solidarity" | "roller" => {
            no_target(action)?;
            if crate::observation::truth(
                state
                    .extra
                    .get(&card.effect)
                    .and_then(|sides| sides.get(actor.as_str())),
            ) {
                return Err(EngineError::IllegalAction);
            }
            set_sparse_color_value(state, &card.effect, actor, json!(true))?;
        }
        "bishop-infiltration" => {
            no_target(action)?;
            if crate::observation::truth(
                state
                    .extra
                    .get("bishopInfiltration")
                    .and_then(|sides| sides.get(actor.as_str())),
            ) {
                return Err(EngineError::IllegalAction);
            }
            set_sparse_color_value(state, "bishopInfiltration", actor, json!(3))?;
        }
        "premove" => {
            apply_premove(state, action.target.as_ref())?;
        }
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "v7 turn card {}",
                card.id
            )));
        }
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn source_state_value(state: &GameState) -> Value {
        let mut value = serde_json::to_value(state).expect("source state JSON");
        let fields = value.as_object_mut().expect("state object");
        for field in ["rulesetId", "rng", "history"] {
            fields.remove(field);
        }
        value
    }

    fn source_state_digest(state: &GameState) -> String {
        format!(
            "{:x}",
            Sha256::digest(serde_jcs::to_vec(&source_state_value(state)).expect("JCS"))
        )
    }

    fn source_card(id: &str) -> CardSlot {
        let mut card: CardSlot = serde_json::from_value(
            crate::card_registry::definition_for(RULES_VERSION_V7, id)
                .expect("source card definition")
                .source_definition
                .clone(),
        )
        .expect("source card instance");
        card.instance_id = format!("turn-card-{id}");
        card
    }

    #[test]
    fn direct_turn_flags_match_frozen_source_full_state() {
        // Frozen SHA e5ed84fc / normal draftDelete seed 37. Each receipt is
        // `applyCardEffect(CARD_BY_ID.get(id), null)` on a fresh source state.
        // Reobserved together against source pre-state JCS d4c7e707a95a4faecca5b4187993fcb20d03b9d93ff2956b3b559eff65f05b55;
        // every source effect succeeds and retains RNG cursor 32/state 3271378757.
        // The hash covers the complete state (excluding outer Position fields),
        // so field shape and untouched neighbors remain part of the contract.
        for (id, expected) in [
            (
                "armistice",
                "3d34dba9eae63301582038a727f1ee3952319f56ee5ab0c4466a530a21bc3ba4",
            ),
            (
                "breakthrough-order",
                "d4b0a6df09c9d016aa9af5700a37d524e16aa511130236c5ccbf1eedc220ceb9",
            ),
            (
                "en-passant-bang",
                "20cee5ad380b67e1175409b75325680dc17a6566e48edf48cfd8270eb26fa757",
            ),
            (
                "royal-command",
                "f7fa140273910fe314c2c9bf144af2f1be5b599328d5b28345f92be358b7477e",
            ),
            (
                "snipe",
                "d9b610c1b940a8ca7e413e52d37ecc24c0ec62d3c4e222f7260e006695b7a2b6",
            ),
            (
                "socialism",
                "c8629b605beb824d3ec2d966cc1f850e262057eea7282666adfa904f51d02e04",
            ),
            (
                "substitution",
                "5b7d0f3c20906e628d497f5a96266734a8cdc188a13eff1dff3d9b406e084201",
            ),
            (
                "switcheroo",
                "78a0d53b9e80d57c204dfb3475ecee248cd200546ba94a7d1f9feac528434af5",
            ),
            (
                "overtake",
                "4b4e4295648a47a1ff29e84f64aae9c01e21c3a173cf46dcdd884143403014bc",
            ),
            (
                "disassembly",
                "156be7fbe3772eddf9c5e532e84482d37bddb4e4c526eebf41b08b14d3df7eeb",
            ),
            (
                "e4",
                "f4d00635a88433059cf2903b8893320d2defb2f518a5ea090e2a6bc51bdd8a30",
            ),
            (
                "bishop-infiltration",
                "b44a7f8a1b7af458cba23f6b31294c62db6b1d62d000f81cb2e8af2061c19ab2",
            ),
            (
                "solidarity",
                "8ad8f4f6f93f334d253dc56c3569780a82ef44669e5584e12c99c9e09c32ace1",
            ),
            (
                "roller",
                "a32e2c78e592d8e95eb43e1b6a7ca6277a8036f5abd8198971033c948084bddd",
            ),
        ] {
            let mut state = crate::v7_new_game::new_game(
                GameConfig {
                    draft_delete: true,
                    ..GameConfig::default()
                },
                37,
            )
            .expect("source v7 opening");
            let card = source_card(id);
            let action = Action::card(state.turn, &card, None);
            let mut explicit_null_state = (id == "armistice").then(|| state.clone());
            assert_eq!(
                actions(&state, &card).unwrap(),
                Some(vec![action.clone()]),
                "{id} availability"
            );
            assert!(apply(&mut state, &card, &action).unwrap().is_empty());
            if let Some(null_state) = explicit_null_state.as_mut() {
                let null_action = Action::card(null_state.turn, &card, Some(Value::Null));
                apply(null_state, &card, &null_action).expect("source no-target null");
                assert_eq!(*null_state, state);
            }
            assert_eq!(
                source_state_digest(&state),
                expected,
                "{id} full source state"
            );
            assert_eq!(
                (state.rng.cursor, state.rng.state),
                (32, 3_271_378_757),
                "{id} RNG"
            );
        }
    }

    #[test]
    #[ignore = "requires the frozen source full-position turn-card receipt"]
    fn frozen_direct_turn_flags_when_receipt_is_supplied() {
        use crate::tests::source_callback_fixture::{
            collect_case_diagnostics, compare_callback_envelope, compare_value,
            source_callback_state,
        };
        let path = std::env::var_os("ACCELERATE_V7_CARD_TURN_CASES")
            .expect("set ACCELERATE_V7_CARD_TURN_CASES to the source receipt JSON");
        let report: Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("source turn-card receipt"))
                .expect("source turn-card receipt JSON");
        assert_eq!(
            report["schemaVersion"], 1,
            "source turn-card receipt schema"
        );
        assert_eq!(
            report["sourceSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        let cases = report["cases"].as_array().expect("source turn-card cases");
        let mut expected_names = [
            "desperado-normal37",
            "reversal-normal37",
            "premove-one-normal19",
            "premove-duplicate-normal19",
            "premove-probe-normal19",
            "premove-first3-invalid-normal19",
            "zugzwang-blocked-normal19",
            "zugzwang-e7-open-normal19",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
        for id in [
            "armistice",
            "breakthrough-order",
            "en-passant-bang",
            "royal-command",
            "snipe",
            "socialism",
            "substitution",
            "switcheroo",
            "overtake",
            "disassembly",
            "e4",
            "bishop-infiltration",
            "solidarity",
            "roller",
        ] {
            expected_names.insert(format!("{id}-direct-flag-normal37"));
        }
        for id in [
            "disassembly",
            "e4",
            "bishop-infiltration",
            "solidarity",
            "roller",
        ] {
            for shape in ["null", "preserve-other-side"] {
                expected_names.insert(format!("{id}-sparse-map-{shape}-normal37"));
            }
        }
        for shape in [
            "false",
            "array-spread",
            "string-spread",
            "truthy-actor-reject",
        ] {
            expected_names.insert(format!("disassembly-sparse-map-{shape}-normal37"));
        }
        let actual_names = cases
            .iter()
            .map(|receipt| {
                receipt["case"]
                    .as_str()
                    .expect("source case name")
                    .to_owned()
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(cases.len(), 36, "source turn-card input count");
        assert_eq!(
            actual_names, expected_names,
            "complete source turn-card input set"
        );

        let mut mismatches = Vec::new();
        for receipt in cases {
            let name = receipt["case"].as_str().expect("source case name");
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                // Preserve the source's complete internal callback envelope,
                // including outer RNG/history and the validated Position ID.
                let mut state = source_callback_state(&receipt["before"])?;
                let card = source_card(receipt["card"].as_str().expect("source card ID"));
                let action = Action::card(state.turn, &card, receipt.get("target").cloned());
                let expected_ok = receipt["result"]["ok"]
                    .as_bool()
                    .expect("source effect result.ok must be boolean");
                let result = apply(&mut state, &card, &action);
                let matches_result = match &result {
                    Ok(_) => expected_ok,
                    Err(EngineError::IllegalAction) => !expected_ok,
                    Err(_) => false,
                };
                if !matches_result {
                    mismatches.push(format!(
                        "{name}.result: source ok={expected_ok}, native={result:?}"
                    ));
                }
                if let Ok(captures) = &result {
                    // Reversal sacrifices the selected ally into the enemy's
                    // source capture ledger. The native effect returns that
                    // removed piece; these callbacks never remove old ledger
                    // entries or append to both owners in a single effect.
                    let mut source_captures = Vec::new();
                    for color in ["white", "black"] {
                        let before = receipt["before"]["state"]["captures"][color]
                            .as_array()
                            .expect("source capture ledger before callback");
                        let after = receipt["after"]["state"]["captures"][color]
                            .as_array()
                            .expect("source capture ledger after callback");
                        assert!(
                            after.len() >= before.len(),
                            "source turn capture ledger must append"
                        );
                        compare_value(
                            &Value::Array(before.clone()),
                            &Value::Array(after[..before.len()].to_vec()),
                            &format!("{name}.sourceCapturePrefix.{color}"),
                            mismatches,
                        )?;
                        source_captures.extend_from_slice(&after[before.len()..]);
                    }
                    compare_value(
                        &Value::Array(source_captures),
                        &serde_json::to_value(captures).map_err(EngineError::serialization)?,
                        &format!("{name}.returnedCaptures"),
                        mismatches,
                    )?;
                }
                // OracleRuntime.snapshot settles queued terminal microtasks
                // after the direct effect, without finishCard or endMove.
                crate::replay::settle(&mut state)?;
                compare_callback_envelope(&state, &receipt["after"], name, mismatches)
            });
        }
        assert!(
            mismatches.is_empty(),
            "{} source turn-card differences across {} cases:\n{}",
            mismatches.len(),
            cases.len(),
            mismatches.join("\n")
        );
    }

    #[test]
    fn premove_first_click_matches_frozen_normal_seed_19_source_order() {
        let state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .expect("v7 source opening");
        let card = source_card("premove");
        let expected = (6..=7)
            .flat_map(|row| (0..8).map(move |col| Square { row, col }))
            .collect::<Vec<_>>();
        assert_eq!(ui_targets(&state, &card).unwrap(), Some(expected));
    }

    #[test]
    fn premove_plan_identity_matches_frozen_normal_seed_19_source_draw() {
        // Source freeMove({selections:[{from:a2,to:a3}]}) on the adopted
        // baseline, before finishCard: one generated ID and one RNG draw.
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .expect("source v7 state");
        assert_eq!(state.rng.cursor, 32);
        assert_eq!(
            premove_plan_id(&mut state).unwrap(),
            "premove-1790581292828-o97mle"
        );
        assert_eq!(state.rng.cursor, 33);
        assert_eq!(state.rng.state, 2_893_839_862);
    }

    #[test]
    fn premove_slices_the_first_three_requests_before_skipping_invalid_entries() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .expect("source v7 state");
        let card = source_card("premove");
        let action = Action::card(
            state.turn,
            &card,
            Some(json!({"selections":[
                {"from":{"row":9,"col":0},"to":{"row":5,"col":0}},
                {"from":{"row":1,"col":0},"to":{"row":2,"col":0}},
                {"from":{"row":6,"col":0},"to":{"row":12,"col":0}},
                {"from":{"row":6,"col":0},"to":{"row":5,"col":0}}
            ]})),
        );
        let before = state.clone();
        assert!(matches!(
            apply(&mut state, &card, &action),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(state, before, "no plan, no RNG draw, no queue mutation");
    }

    #[test]
    fn premove_registers_source_identity_and_empty_diagonal_probe_without_moving_a_piece() {
        // Source normal/draftDelete seed19: an ordinary a2-a3 declaration and
        // an empty b3 pawn-capture probe each store one plan and the same first
        // pending ID. Neither declaration executes a piece move at card time.
        for to in [Square { row: 5, col: 0 }, Square { row: 5, col: 1 }] {
            let mut state = crate::v7_new_game::new_game(
                GameConfig {
                    draft_delete: true,
                    ..GameConfig::default()
                },
                19,
            )
            .expect("source v7 state");
            let from = Square { row: 6, col: 0 };
            let piece_id = state.at(from).expect("source pawn").id.clone();
            let card = source_card("premove");
            let action = Action::card(
                state.turn,
                &card,
                Some(json!({"selections":[{"from":from,"to":to}]})),
            );
            let mut expected = state.clone();
            expected.extra.insert(
                "pendingFreeMoves".into(),
                json!([{
                    "id":"premove-1790581292828-o97mle","color":"white",
                    "triggerColor":"black","triggerTurn":1,"moves":[{
                        "pieceId":piece_id,"pieceType":"pawn",
                        "from":from,"to":to,"fromCells":[from],"toCells":[to]
                    }]
                }]),
            );
            expected.rng.cursor = 33;
            expected.rng.state = 2_893_839_862;
            assert!(apply(&mut state, &card, &action).unwrap().is_empty());
            assert_eq!(state, expected, "source declaration at {to:?}");
        }
    }

    #[test]
    fn premove_preserves_first_valid_plan_per_identity_and_draws_only_one_pending_id() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .expect("source v7 state");
        let from_a = Square { row: 6, col: 0 };
        let from_b = Square { row: 6, col: 1 };
        let pawn_a = state.at(from_a).expect("a pawn").id.clone();
        let pawn_b = state.at(from_b).expect("b pawn").id.clone();
        let to_a = Square { row: 5, col: 0 };
        let to_b = Square { row: 5, col: 1 };
        let card = source_card("premove");
        let action = Action::card(
            state.turn,
            &card,
            Some(json!({"selections":[
                {"from":from_a,"to":to_a},
                {"from":from_a,"to":{"row":4,"col":0}},
                {"from":from_b,"to":to_b}
            ]})),
        );
        let before_board = state.board.clone();
        apply(&mut state, &card, &action).expect("source duplicate plans");
        assert_eq!(
            state.extra["pendingFreeMoves"],
            json!([{
                "id":"premove-1790581292828-o97mle","color":"white",
                "triggerColor":"black","triggerTurn":1,"moves":[
                    {"pieceId":pawn_a,"pieceType":"pawn","from":from_a,"to":to_a,"fromCells":[from_a],"toCells":[to_a]},
                    {"pieceId":pawn_b,"pieceType":"pawn","from":from_b,"to":to_b,"fromCells":[from_b],"toCells":[to_b]}
                ]
            }])
        );
        assert_eq!(state.board, before_board);
        assert_eq!((state.rng.cursor, state.rng.state), (33, 2_893_839_862));
    }

    #[test]
    fn switcheroo_rejects_a_bunker_first_king_even_with_a_later_mobile_royal() {
        // Frozen normal/draftDelete seed 19: after setting e1 to a bunker king
        // and marking f1 royal, source `findMobileKing` still returns null.
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .expect("v7 source opening");
        let first = Square { row: 7, col: 4 };
        state
            .at_mut(first)
            .expect("king")
            .extra
            .insert("undergroundBunker".into(), json!(true));
        state
            .at_mut(first)
            .expect("king")
            .extra
            .insert("hp".into(), json!(2));
        state
            .at_mut(Square { row: 7, col: 5 })
            .expect("bishop")
            .extra
            .insert("crownRoyal".into(), json!(true));
        let card = source_card("switcheroo");
        let action = Action::card(state.turn, &card, None);
        for hp in [json!(2), json!("2"), Value::Null] {
            // Source Number(hp) treats both a numeric string and null as
            // finite health, so neither makes the first bunker king mobile.
            state
                .at_mut(first)
                .expect("king")
                .extra
                .insert("hp".into(), hp);
            let before = state.clone();
            assert_eq!(actions(&state, &card).unwrap(), Some(Vec::new()));
            assert!(matches!(
                apply(&mut state, &card, &action),
                Err(EngineError::IllegalAction)
            ));
            assert_eq!(state, before);
        }
    }

    #[test]
    fn replay_restores_captured_square_but_keeps_later_capture_and_one_notation_draw() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .expect("source v7 state");
        let from = Square { row: 6, col: 0 };
        let to = Square { row: 5, col: 0 };
        let original = state.at(from).expect("own pawn").clone();
        let captured = Piece::new("pawn", Color::Black, "replay-captured");
        let later = Piece::new("knight", Color::Black, "later-captured");
        let mut moved = original.clone();
        moved.moved = true;
        state.board[from.row as usize][from.col as usize] = None;
        state.board[to.row as usize][to.col as usize] = Some(moved.clone());
        state.captures.white = vec![captured.clone(), later.clone()];
        state.extra.insert(
            "moveReplay".into(),
            json!({"white":{"delta":[
                {"row":from.row,"col":from.col,"before":original,"after":null},
                {"row":to.row,"col":to.col,"before":captured,"after":moved}
            ],"capturesBefore":[],"capturesAfter":[captured],
            "lastMoveAfter":null,"previousLastMove":null,
            "zugzwangBefore":{"white":false,"black":false},
            "zugzwangAfter":{"white":true,"black":false}},"black":null}),
        );
        state
            .extra
            .insert("zugzwang".into(), json!({"white":true,"black":false}));
        let mut expected_rng = state.rng.clone();
        expected_rng.sample().expect("notation draw");

        let mut numeric_equivalent = state.clone();
        for entry in numeric_equivalent.extra.get_mut("moveReplay").unwrap()["white"]["delta"]
            .as_array_mut()
            .unwrap()
        {
            for key in ["row", "col"] {
                entry[key] = json!(entry[key].as_f64().expect("numeric replay coordinate"));
            }
        }

        replay_move(&mut state, Color::White).expect("source replay");
        replay_move(&mut numeric_equivalent, Color::White)
            .expect("equivalent JavaScript Number replay");
        assert_eq!(
            source_state_digest(&numeric_equivalent),
            source_state_digest(&state),
            "equivalent numeric coordinates must preserve the complete replay state"
        );
        assert_eq!(
            numeric_equivalent.rng, state.rng,
            "equivalent numeric replay RNG"
        );
        assert_eq!(
            numeric_equivalent.history, state.history,
            "equivalent numeric replay outer history"
        );
        assert_eq!(state.at(from).expect("restored pawn").id, original.id);
        assert_eq!(state.at(to).expect("restored target").id, "replay-captured");
        assert_eq!(state.captures.white, vec![later]);
        assert_eq!(state.extra["zugzwang"]["white"], false);
        assert!(state.extra["moveReplay"]["white"].is_null());
        assert_eq!(state.extra["pendingNotation"]["text"], "!리플레이");
        assert_eq!(state.rng, expected_rng);
    }

    #[test]
    fn replay_rejects_a_changed_destination_without_mutating_state_or_rng() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .expect("source v7 state");
        let pawn = state.at(Square { row: 6, col: 0 }).expect("pawn").clone();
        state.extra.insert(
            "moveReplay".into(),
            json!({"white":{"delta":[{"row":5,"col":0,"before":null,"after":pawn}],
                "capturesBefore":[],"capturesAfter":[]},"black":null}),
        );
        let before = state.clone();
        assert!(matches!(
            replay_move(&mut state, Color::White),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn replay_uses_source_empty_capture_defaults_and_removes_omitted_color_state() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .expect("source v7 state");
        state.extra.insert(
            "moveReplay".into(),
            json!({"white":{"delta":[{"row":5,"col":0,"before":null,"after":null}],
                "movingBefore":{},"movingAfter":{"white":true}},"black":null}),
        );
        state
            .extra
            .insert("moving".into(), json!({"white":true,"black":true}));
        state
            .captures
            .white
            .push(Piece::new("pawn", Color::Black, "later-capture"));
        replay_move(&mut state, Color::White).expect("source optional capture fields");
        assert_eq!(state.captures.white[0].id, "later-capture");
        assert_eq!(state.extra["moving"], json!({"black":true}));
        assert!(state.extra["moveReplay"]["white"].is_null());
    }

    #[test]
    fn replay_relinks_large_piece_cells_and_applies_source_ultimatum_id_coercion() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .expect("source v7 state");
        let mut before = Piece::new("bigRook", Color::White, "large-replay");
        before.extra.insert("anchorRow".into(), json!(4));
        before.extra.insert("anchorCol".into(), json!(0));
        before.extra.insert("hp".into(), json!(2));
        // Imported source pieces retain every serialized field in source_order.
        // Native construction appends extra fields only during serialization.
        let before: Piece = serde_json::from_value(serde_json::to_value(before).unwrap()).unwrap();
        let mut after = before.clone();
        after.extra.insert("hp".into(), json!(1));
        state.board[4][0] = Some(after.clone());
        state.board[4][1] = Some(after.clone());
        state.extra.insert(
            "moveReplay".into(),
            json!({"white":{"delta":[
                {"row":4,"col":0,"before":before,"after":after},
                {"row":4,"col":1,"before":before,"after":after}
            ],"ultimatumBefore":{"movedIds":[1]},
                "ultimatumAfter":{"movedIds":["1",2]}},"black":null}),
        );
        state
            .extra
            .insert("ultimatum".into(), json!({"movedIds":[1,"2","later"]}));
        replay_move(&mut state, Color::White).expect("source identity and ID conversion");
        assert_eq!(state.board[4][0], Some(before.clone()));
        assert_eq!(state.board[4][1], Some(before));
        assert_eq!(state.extra["ultimatum"]["movedIds"], json!(["1", "later"]));
    }

    #[test]
    fn zugzwang_requires_an_enemy_royal_movement_and_rejects_atomically() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .expect("source v7 opening");
        let card = source_card("zugzwang");
        let mut action = Action::card(state.turn, &card, Some(Value::Null));
        let before = state.clone();
        assert_eq!(actions(&state, &card).unwrap(), Some(Vec::new()));
        assert!(matches!(
            apply(&mut state, &card, &action),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(state, before);

        // Opening e7 pawn removal gives the enemy king an actual e7 move.
        state.board[1][4] = None;
        action.target = None;
        let before_rng = state.rng.clone();
        apply(&mut state, &card, &action).expect("source enemy royal can move");
        assert_eq!(state.extra["zugzwang"], json!({"white":false,"black":true}));
        assert_eq!(state.rng, before_rng);
    }
}
