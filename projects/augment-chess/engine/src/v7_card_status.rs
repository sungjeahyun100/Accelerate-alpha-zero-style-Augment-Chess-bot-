//! Frozen v7 ACTIVE cards whose direct effects create statuses or delayed work.
//!
//! This is the card effect boundary only. The caller owns card-use settlement,
//! turn changes and replay commits; the corresponding turn-entry/end-turn
//! callbacks remain in their own modules. Source: `main-OahWs0tU.js`, SHA-256
//! `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`.

use super::*;

pub(super) const IDS: &[&str] = &[
    "calling-card",
    "chameleon-mutation",
    "collapse",
    "dice",
    "gale",
    "hallucination",
    "icbm",
    "insight",
    "last-resistance",
    "lobster",
    "mistake-card",
    "otherworld",
    "panic",
    "prophecy",
    "reverse-pawns",
    "sacrifice",
    "spy",
    "submerge",
    "suspicious-potion",
    "ultimatum",
    "vanish",
    "wanted",
    "greek-gift",
];

fn owned(card: &CardSlot) -> bool {
    IDS.contains(&card.id.as_str())
}

fn unported(card: &CardSlot, phase: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!(
        "v7 status card {} {} requires frozen source parity",
        card.id, phase
    ))
}

fn effect_name(id: &str) -> Option<&'static str> {
    Some(match id {
        "calling-card" => "callingCard",
        "chameleon-mutation" => "chameleonMutation",
        "collapse" => "collapse",
        "dice" => "dice",
        "gale" => "gale",
        "hallucination" => "hallucination",
        "icbm" => "icbm",
        "insight" => "insight",
        "last-resistance" => "lastResistance",
        "lobster" => "lobster",
        "mistake-card" => "mistakeCard",
        "otherworld" => "otherworld",
        "panic" => "panic",
        "prophecy" => "prophecy",
        "reverse-pawns" => "reversePawns",
        "sacrifice" => "sacrifice",
        "spy" => "spy",
        "submerge" => "submerge",
        "suspicious-potion" => "suspiciousPotion",
        "ultimatum" => "ultimatum",
        "vanish" => "vanish",
        "wanted" => "wanted",
        "greek-gift" => "greekGift",
        _ => return None,
    })
}

fn validate_object(state: &GameState, card: &CardSlot) -> Result<()> {
    if !owned(card) || state.ruleset_id != RULES_VERSION_V7 {
        return Err(unported(card, "identity"));
    }
    let definition = crate::card_registry::definition_for(RULES_VERSION_V7, &card.id)?;
    let expected = effect_name(&card.id).ok_or_else(|| unported(card, "catalog"))?;
    if definition.effect != expected || card.effect != expected {
        return Err(EngineError::InvalidState(format!(
            "v7 status card {} catalog effect drift",
            card.id
        )));
    }
    validate_pinned_card(state, card)
}

// These direct branches have no public target choice. Otherworld alone draws
// a pawn and a queue identity from the source RNG. Deferred callbacks belong
// to turn entry, move settlement and board
// automata, rather than being silently applied during the card transaction.
const DIRECT_NO_TARGET: &[&str] = &[
    "calling-card",
    "collapse",
    "dice",
    "gale",
    "hallucination",
    "insight",
    "last-resistance",
    "mistake-card",
    "otherworld",
    "prophecy",
    "reverse-pawns",
    "ultimatum",
    "vanish",
    "wanted",
];

fn exact_square_target(state: &GameState, action: &Action) -> Result<Square> {
    let target = action.target.as_ref().ok_or(EngineError::IllegalAction)?;
    let object = target.as_object().ok_or(EngineError::IllegalAction)?;
    if object.len() != 2 {
        return Err(EngineError::IllegalAction);
    }
    let square: Square =
        serde_json::from_value(target.clone()).map_err(|_| EngineError::IllegalAction)?;
    if state.at(square).is_none()
        && state
            .board
            .get(square.row as usize)
            .and_then(|row| row.get(square.col as usize))
            .is_none()
    {
        return Err(EngineError::IllegalAction);
    }
    Ok(square)
}

fn source_squares(
    state: &GameState,
    mut predicate: impl FnMut(&Piece, Square) -> bool,
) -> Result<Vec<Square>> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            let Some(piece) = piece else { continue };
            let square = Square {
                row: u8::try_from(row).map_err(|_| {
                    EngineError::InvalidState("v7 status board row overflow".into())
                })?,
                col: u8::try_from(col).map_err(|_| {
                    EngineError::InvalidState("v7 status board col overflow".into())
                })?,
            };
            if !predicate(piece, square) {
                continue;
            }
            let key = if piece.id.is_empty() {
                format!("cell:{row}:{col}")
            } else {
                format!("piece:{}", piece.id)
            };
            if seen.insert(key) {
                result.push(square);
            }
        }
    }
    Ok(result)
}

fn bishop_targets(state: &GameState) -> Result<Vec<Square>> {
    source_squares(state, |piece, _| {
        piece.color == state.turn && piece.kind == "bishop" && !state.royal_identity(piece)
    })
}

fn submerge_targets(state: &GameState) -> Result<Vec<Square>> {
    source_squares(state, |piece, square| {
        if piece.color != state.turn
            || truthy(piece.extra.get("submerged"))
            || piece.is_large()
            || ["wall", "football", "blackHole", "coffin"].contains(&piece.kind.as_str())
        {
            return false;
        }
        (-1_i16..=1).all(|dr| {
            (-1_i16..=1).all(|dc| {
                if dr == 0 && dc == 0 {
                    return true;
                }
                let row = i16::from(square.row) + dr;
                let col = i16::from(square.col) + dc;
                let Ok(row) = u8::try_from(row) else {
                    return true;
                };
                let Ok(col) = u8::try_from(col) else {
                    return true;
                };
                state
                    .at(Square { row, col })
                    .is_none_or(|other| other.color != state.turn.opponent())
            })
        })
    })
}

fn potion_targets(state: &GameState) -> Result<Vec<Square>> {
    source_squares(state, |piece, _| super::potion_target(piece))
}

fn crown_ground(state: &GameState, square: Square) -> bool {
    let Some(rule) = state.extra.get("crownRule") else {
        return false;
    };
    let entries = rule
        .get("crowns")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty());
    let at = |entry: &Value| {
        if !truthy(Some(entry)) || truthy(entry.get("removed")) {
            return false;
        }
        if entry == &Value::Bool(true) {
            return square.row as usize == state.board.len().saturating_sub(2) / 2
                && square.col as usize
                    == state.board.first().map_or(0, Vec::len).saturating_sub(2) / 2;
        }
        entry
            .get("ground")
            .and_then(|ground| ground.get("row"))
            .and_then(Value::as_u64)
            == Some(u64::from(square.row))
            && entry
                .get("ground")
                .and_then(|ground| ground.get("col"))
                .and_then(Value::as_u64)
                == Some(u64::from(square.col))
    };
    entries.map_or_else(|| at(rule), |entries| entries.iter().any(at))
}

fn lobster_installation_open(state: &GameState, square: Square) -> Result<bool> {
    if state
        .board
        .get(square.row as usize)
        .and_then(|row| row.get(square.col as usize))
        .is_none()
        || crown_ground(state, square)
    {
        return Ok(false);
    }
    // main:68022-68027. An enemy concealed from the installer may occupy the
    // chosen cell. Remove just that cell in the availability probe so the
    // movement kernel still checks quantum aliases and reservations.
    let concealed = state.at(square).is_some_and(|piece| {
        piece.color == state.turn.opponent()
            && super::piece_hidden_from(state, piece, square) == json!(state.turn)
    });
    if state.at(square).is_some() && !concealed {
        return Ok(false);
    }
    let mut probe;
    let board = if concealed {
        probe = state.clone();
        probe.board[square.row as usize][square.col as usize] = None;
        &probe
    } else {
        state
    };
    crate::movement::open_placement(board, square, None)
}

fn lobster_targets(state: &GameState, destination_filter: bool) -> Result<Vec<Square>> {
    let mut result = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for col in 0..cells.len() {
            let square = Square {
                row: u8::try_from(row).map_err(|_| {
                    EngineError::InvalidState("v7 lobster board row overflow".into())
                })?,
                col: u8::try_from(col).map_err(|_| {
                    EngineError::InvalidState("v7 lobster board col overflow".into())
                })?,
            };
            if lobster_installation_open(state, square)?
                && (!destination_filter
                    || crate::movement::d4_destination_allowed(state, state.turn.into(), &[square]))
            {
                result.push(square);
            }
        }
    }
    Ok(result)
}

fn sacrifice_protection_targets(state: &GameState, source: Square) -> Result<Vec<Square>> {
    let piece = state.at(source).ok_or(EngineError::IllegalAction)?;
    if piece.color != state.turn
        || state.royal_identity(piece)
        || [
            "merchant",
            "timeTraveler",
            "vampireLord",
            "wall",
            "football",
            "blackHole",
            "coffin",
        ]
        .contains(&piece.kind.as_str())
    {
        return Ok(Vec::new());
    }
    let Some(value) = super::combat_value(state, piece).filter(|value| value.is_finite()) else {
        return Ok(Vec::new());
    };
    source_squares(state, |candidate, square| {
        square != source
            && candidate.color == state.turn
            && !state.royal_identity(candidate)
            && ![
                "merchant",
                "timeTraveler",
                "vampireLord",
                "wall",
                "football",
                "blackHole",
                "coffin",
            ]
            .contains(&candidate.kind.as_str())
            && super::combat_value(state, candidate)
                .is_some_and(|other| other.is_finite() && other < value)
    })
}

fn sacrifice_targets(state: &GameState) -> Result<Vec<Square>> {
    let all = source_squares(state, |piece, _| piece.color == state.turn)?;
    let mut result = Vec::new();
    for square in all {
        if !sacrifice_protection_targets(state, square)?.is_empty() {
            result.push(square);
        }
    }
    Ok(result)
}

const INSIGHT_PROTECTIONS: &[&str] = &[
    "shielded",
    "protected",
    "lastResistance",
    "sacrificeProtection",
    "coronationProtection",
    "queensGambitProtection",
];
const INSIGHT_ENEMY_EFFECTS: &[&str] = &[
    "metalized",
    "metalCooldown",
    "parry",
    "loyalist",
    "submerged",
    "ghost",
    "poisonedPawn",
    "explosive",
    "chimera",
    "chameleon",
    "basicTraining",
    "trojanHorse",
    "holdoutPromotion",
    "royalCommand",
    "outpostProtected",
    "nullification",
    "recurrence",
];
const INSIGHT_ALLY_EFFECTS: &[&str] = &[
    "wanted",
    "disarmed",
    "severed",
    "inertia",
    "potionManner",
    "potionSaturation",
    "iceSheet",
    "witchTrial",
    "frozen",
    "frozenByCard",
    "bloodCurse",
];

fn remove_truthy(piece: &mut Piece, fields: &[&str]) -> bool {
    let mut changed = false;
    for field in fields {
        if truthy(piece.extra.get(*field)) {
            piece.extra.shift_remove(*field);
            changed = true;
        }
    }
    changed
}

fn hostile_by(value: Option<&Value>, enemy: Color) -> bool {
    value.is_some_and(|value| {
        truthy(Some(value))
            && value
                .get("by")
                .is_none_or(|by| !truthy(Some(by)) || by.as_str() == Some(enemy.as_str()))
    })
}

fn clear_insight_piece(piece: &mut Piece, actor: Color, cool_guy: bool, saturation: bool) -> bool {
    let enemy = actor.opponent();
    if piece.color == enemy {
        let had_potion_training = truthy(piece.extra.get("potionBasicTraining"));
        remove_truthy(piece, INSIGHT_PROTECTIONS);
        if piece.extra.get("hiddenFrom").and_then(Value::as_str) == Some(actor.as_str()) {
            piece.extra.shift_remove("hiddenFrom");
        }
        let evasion_removed = truthy(piece.extra.get("evasion"));
        if evasion_removed {
            piece.extra.shift_remove("evasion");
        }
        remove_truthy(piece, INSIGHT_ENEMY_EFFECTS);
        if had_potion_training && !truthy(piece.extra.get("basicTraining")) {
            piece.extra.shift_remove("potionBasicTraining");
        }
        if ["twinBondId", "twinPartnerId", "twinSwapPending"]
            .iter()
            .any(|key| truthy(piece.extra.get(*key)))
        {
            for key in ["twinBondId", "twinPartnerId", "twinSwapPending"] {
                piece.extra.shift_remove(key);
            }
        }
        if truthy(piece.extra.get("feudalContractId")) {
            piece.extra.shift_remove("feudalContractId");
        }
        if [
            "quantum",
            "quantumNoCaptureUntil",
            "quantumFirstObservationFails",
        ]
        .iter()
        .any(|key| truthy(piece.extra.get(*key)))
        {
            for key in [
                "quantum",
                "quantumNoCaptureUntil",
                "quantumFirstObservationFails",
            ] {
                piece.extra.shift_remove(key);
            }
        }
        if piece
            .extra
            .get("imperialMoves")
            .and_then(Value::as_array)
            .is_some_and(|moves| !moves.is_empty())
        {
            piece.extra.shift_remove("imperialMoves");
        }
        if hostile_by(piece.extra.get("staked"), enemy) {
            piece.extra.shift_remove("staked");
        }
        if piece
            .extra
            .get("vipInvitation")
            .and_then(|entry| entry.get("by"))
            .and_then(Value::as_str)
            == Some(enemy.as_str())
        {
            piece.extra.shift_remove("vipInvitation");
        }
        return evasion_removed;
    }
    if piece.color != actor {
        return false;
    }
    let had_manner = truthy(piece.extra.get("potionManner"));
    let had_saturation = truthy(piece.extra.get("potionSaturation"));
    remove_truthy(piece, INSIGHT_ALLY_EFFECTS);
    if had_manner && !truthy(piece.extra.get("potionManner")) {
        piece
            .extra
            .insert("coolGuyCapturedLast".into(), json!(false));
    }
    if had_saturation && !truthy(piece.extra.get("potionSaturation")) {
        piece.extra.insert("capturesMade".into(), json!(0));
    }
    for key in ["callingCard", "emptyLunchbox"] {
        if hostile_by(piece.extra.get(key), enemy) {
            piece.extra.shift_remove(key);
        }
    }
    if piece
        .extra
        .get("vipInvitation")
        .and_then(|entry| entry.get("by"))
        .and_then(Value::as_str)
        == Some(enemy.as_str())
    {
        piece.extra.shift_remove("vipInvitation");
    }
    if piece.extra.get("spyOwner").and_then(Value::as_str) == Some(enemy.as_str()) {
        piece.extra.shift_remove("spyOwner");
    }
    if piece
        .extra
        .get("poisonStunTurns")
        .and_then(Value::as_f64)
        .is_some_and(|value| value > 0.0)
    {
        piece.extra.shift_remove("poisonStunTurns");
        piece.extra.shift_remove("poisonStunColor");
    }
    if cool_guy && truthy(piece.extra.get("coolGuyCapturedLast")) {
        piece
            .extra
            .insert("coolGuyCapturedLast".into(), json!(false));
    }
    if saturation
        && piece
            .extra
            .get("capturesMade")
            .and_then(Value::as_f64)
            .is_some_and(|value| value > 0.0)
    {
        piece.extra.insert("capturesMade".into(), json!(0));
    }
    false
}

fn clear_insight_side_value(
    state: &mut GameState,
    field: &str,
    color: Color,
    value: Value,
) -> Result<()> {
    let Some(map) = state.extra.get_mut(field).and_then(Value::as_object_mut) else {
        return Ok(());
    };
    if !map.contains_key(color.as_str()) {
        return Ok(());
    }
    map.insert(color.as_str().into(), value);
    Ok(())
}

fn apply_insight(state: &mut GameState) -> Result<()> {
    // main:1922-2014 and 103132-103210. Piece identities are visited once
    // in board order; mutating all aliases preserves large-piece identity.
    let actor = state.turn;
    let enemy = actor.opponent();
    let cool_guy = truthy(state.extra.get("coolGuy"));
    let saturation = truthy(state.extra.get("saturationRule"));
    let pieces = source_squares(state, |_, _| true)?;
    for square in pieces {
        let mut piece = state
            .at(square)
            .cloned()
            .ok_or(EngineError::IllegalAction)?;
        let evasion_removed = clear_insight_piece(&mut piece, actor, cool_guy, saturation);
        write_piece(state, &piece);
        if evasion_removed && !piece.id.is_empty() {
            let moving = state.extra.entry("moving").or_insert_with(|| json!({"white":{"enabled":false,"pieceId":"","count":0},"black":{"enabled":false,"pieceId":"","count":0}}));
            let current = moving
                .get(piece.color.as_str())
                .cloned()
                .unwrap_or(Value::Null);
            let enabled = truthy(current.get("enabled"));
            let id = current.get("pieceId").and_then(Value::as_str).unwrap_or("");
            let count = current
                .get("count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(4);
            moving[piece.color.as_str()] = if enabled && id == piece.id {
                json!({"enabled":true,"pieceId":"","count":0})
            } else {
                json!({"enabled":enabled,"pieceId":id,"count":count})
            };
        }
    }
    if let Some(contracts) = state
        .extra
        .get_mut("feudalContracts")
        .and_then(Value::as_array_mut)
    {
        contracts
            .retain(|entry| entry.get("color").and_then(Value::as_str) != Some(enemy.as_str()));
    }
    if let Some(palaces) = state.extra.get_mut("palaces").and_then(Value::as_array_mut) {
        palaces.retain(|entry| entry.get("color").and_then(Value::as_str) != Some(actor.as_str()));
    }
    let bonds = super::normalize_chain_bonds(state.extra.get("chainBonds"))?;
    state.extra.insert(
        "chainBonds".into(),
        Value::Array(
            bonds
                .into_iter()
                .filter(|bond| bond.get("by").and_then(Value::as_str) != Some(enemy.as_str()))
                .collect(),
        ),
    );
    if state
        .extra
        .get("hallucination")
        .and_then(|map| map.get(actor.as_str()))
        .and_then(|entry| entry.get("color"))
        .and_then(Value::as_str)
        == Some(enemy.as_str())
    {
        clear_insight_side_value(state, "hallucination", actor, Value::Null)?;
    }
    for (field, nested) in [
        ("effects", Some("pawnReverse")),
        ("taunt", None),
        ("socialism", None),
    ] {
        let value = if let Some(nested) = nested {
            state
                .extra
                .get(field)
                .and_then(|entry| entry.get(nested))
                .and_then(|entry| entry.get(actor.as_str()))
        } else {
            state
                .extra
                .get(field)
                .and_then(|entry| entry.get(actor.as_str()))
        };
        if value
            .and_then(Value::as_f64)
            .is_some_and(|value| value > 0.0)
        {
            if let Some(nested) = nested {
                state
                    .extra
                    .get_mut(field)
                    .and_then(|entry| entry.get_mut(nested))
                    .ok_or_else(|| {
                        EngineError::InvalidState(format!("v7 insight {field}.{nested} missing"))
                    })?[actor.as_str()] = json!(0);
            } else {
                clear_insight_side_value(state, field, actor, json!(0))?;
            }
        }
    }
    for field in ["initiative", "diceLocks"] {
        if truthy(
            state
                .extra
                .get(field)
                .and_then(|entry| entry.get(actor.as_str())),
        ) {
            clear_insight_side_value(state, field, actor, Value::Null)?;
        }
    }
    for field in ["knightInjury", "mistakeCard", "zugzwang"] {
        if truthy(
            state
                .extra
                .get(field)
                .and_then(|entry| entry.get(actor.as_str())),
        ) {
            clear_insight_side_value(state, field, actor, json!(false))?;
        }
    }
    if truthy(
        state
            .extra
            .get("exhaustion")
            .and_then(|entry| entry.get(actor.as_str()))
            .and_then(|entry| entry.get("enabled")),
    ) {
        clear_insight_side_value(
            state,
            "exhaustion",
            actor,
            json!({"enabled":false,"pieceId":"","count":0}),
        )?;
    }
    if truthy(
        state
            .extra
            .get("royalCommand")
            .and_then(|entry| entry.get(enemy.as_str())),
    ) {
        clear_insight_side_value(state, "royalCommand", enemy, Value::Null)?;
    }
    let own_ids = source_squares(state, |piece, _| piece.color == actor)?
        .into_iter()
        .filter_map(|square| state.at(square).map(|piece| piece.id.clone()))
        .collect::<BTreeSet<_>>();
    if let Some(ids) = state
        .extra
        .get_mut("winterKingdom")
        .and_then(|kingdom| kingdom.get_mut("frozenIds"))
        .and_then(Value::as_array_mut)
    {
        ids.retain(|id| id.as_str().is_none_or(|id| !own_ids.contains(id)));
    }
    Ok(())
}

fn color_map<'a>(
    state: &'a mut GameState,
    key: &str,
) -> Result<&'a mut serde_json::Map<String, Value>> {
    if !truthy(state.extra.get(key)) {
        state
            .extra
            .insert(key.into(), json!({"white":null,"black":null}));
    }
    state
        .extra
        .get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState(format!("v7 {key} must be a color map")))
}

fn collapse_available(state: &GameState) -> Result<bool> {
    let rows = state.board.len();
    let cols = state.board.first().map_or(0, Vec::len);
    if rows == 0 || cols == 0 || state.board.iter().any(|row| row.len() != cols) {
        return Err(EngineError::InvalidState(
            "v7 collapse requires rectangular board geometry".into(),
        ));
    }
    let maximum = rows.min(cols).div_ceil(2);
    let legacy = usize::from(truthy(state.extra.get("collapsed")));
    let depth = state
        .extra
        .get("collapseDepth")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value != 0.0)
        .map_or(legacy, |value| value.max(0.0).floor() as usize);
    Ok(depth.min(maximum) < maximum)
}

fn active_prophecy(state: &GameState) -> bool {
    truthy(
        state
            .extra
            .get("prophecy")
            .and_then(|value| value.get(state.turn.as_str())),
    )
}

// main:99450-99469. The source de-duplicates a large piece by identity while
// walking board cells in row-major order. The adopted v7 catalog enables
// Revolving Door's guard ability, so these three abilities are immune to this
// indirect target selection even if the piece's displayed type differs.
fn calling_card_candidates(state: &GameState) -> Vec<Square> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, entry) in cells.iter().enumerate() {
            let Some(piece) = entry else { continue };
            if piece.color != state.turn.opponent()
                || piece.kind == "pawn"
                || state.royal_identity(piece)
                || ["wall", "football", "blackHole"].contains(&piece.kind.as_str())
                || ["guard", "revolvingDoor", "jester"].contains(&piece.ability_kind())
                || !seen.insert(if piece.id.is_empty() {
                    format!("square:{row}:{col}")
                } else {
                    piece.id.clone()
                })
            {
                continue;
            }
            if let (Ok(row), Ok(col)) = (u8::try_from(row), u8::try_from(col)) {
                result.push(Square { row, col });
            }
        }
    }
    result
}

fn reverse_pawns_available(state: &GameState) -> bool {
    !truthy(state.extra.get("machoChess"))
        && !state
            .deck_slots
            .white
            .iter()
            .chain(&state.deck_slots.black)
            .any(|card| card.id == "rule-ticket" || card.id == "macho-chess")
        && state
            .extra
            .get("appliedRuleCard")
            .and_then(|rule| rule.get("id"))
            != Some(&json!("macho-chess"))
}

// main:67973-67983 and 105944. Pending panic excludes the identity even if
// it occupies another cell later. The UI first-click list is board row-major
// and is independent of whether two candidates can ultimately be selected.
fn panic_targets(state: &GameState) -> Result<Vec<Square>> {
    let pending_ids = state
        .extra
        .get("pendingPanic")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("pieces").and_then(Value::as_array))
        .flatten()
        .filter_map(|piece| piece.get("id").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let mut result = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, entry) in cells.iter().enumerate() {
            let Some(piece) = entry else { continue };
            if piece.color != state.turn.opponent()
                || state.royal_identity(piece)
                || [
                    "merchant",
                    "wall",
                    "football",
                    "colossus",
                    "bigRook",
                    "bigBishop",
                ]
                .contains(&piece.kind.as_str())
                || (!piece.id.is_empty() && pending_ids.contains(piece.id.as_str()))
            {
                continue;
            }
            result.push(Square {
                row: u8::try_from(row).map_err(|_| {
                    EngineError::InvalidState("v7 panic board has too many rows".into())
                })?,
                col: u8::try_from(col).map_err(|_| {
                    EngineError::InvalidState("v7 panic board has too many columns".into())
                })?,
            });
        }
    }
    Ok(result)
}

// main:105853-105877. UI clicks may show several cells for one identity,
// while complete selection plans and panic() require two distinct pieces.
fn panic_selection_targets(state: &GameState) -> Result<Vec<Square>> {
    let mut seen = BTreeSet::new();
    Ok(panic_targets(state)?
        .into_iter()
        .filter(|square| {
            let key = state
                .at(*square)
                .filter(|piece| !piece.id.is_empty())
                .map_or_else(
                    || format!("square:{},{}", square.row, square.col),
                    |piece| format!("piece:{}", piece.id),
                );
            seen.insert(key)
        })
        .collect())
}

// main:68573-68575, 105195-105213. The source allows one to three own
// non-royal identities in board order; a previously mutated piece remains a
// valid choice. The excluded large types are source names, not all large
// piece flags, so do not broaden the filter here.
fn chameleon_targets(state: &GameState) -> Result<Vec<Square>> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, entry) in cells.iter().enumerate() {
            let Some(piece) = entry else { continue };
            if piece.color != state.turn
                || state.royal_identity(piece)
                || ["merchant", "wall", "colossus", "bigRook", "bigBishop"]
                    .contains(&piece.kind.as_str())
                || !seen.insert(if piece.id.is_empty() {
                    format!("square:{row}:{col}")
                } else {
                    format!("piece:{}", piece.id)
                })
            {
                continue;
            }
            result.push(Square {
                row: u8::try_from(row).map_err(|_| {
                    EngineError::InvalidState("v7 chameleon board has too many rows".into())
                })?,
                col: u8::try_from(col).map_err(|_| {
                    EngineError::InvalidState("v7 chameleon board has too many columns".into())
                })?,
            });
        }
    }
    Ok(result)
}

fn spy_targets(state: &GameState) -> Result<Vec<Square>> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, entry) in cells.iter().enumerate() {
            let Some(piece) = entry else { continue };
            if piece.color != state.turn.opponent()
                || piece.kind != "pawn"
                || !seen.insert(if piece.id.is_empty() {
                    format!("square:{row}:{col}")
                } else {
                    format!("piece:{}", piece.id)
                })
            {
                continue;
            }
            result.push(Square {
                row: u8::try_from(row).map_err(|_| {
                    EngineError::InvalidState("v7 spy board has too many rows".into())
                })?,
                col: u8::try_from(col).map_err(|_| {
                    EngineError::InvalidState("v7 spy board has too many columns".into())
                })?,
            });
        }
    }
    Ok(result)
}

fn icbm_queens(state: &GameState, color: Color) -> Result<Vec<Square>> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, entry) in cells.iter().enumerate() {
            let Some(piece) = entry else { continue };
            if piece.color != color
                || piece.kind != "queen"
                || piece.flag("regencyHeir")
                || !seen.insert(if piece.id.is_empty() {
                    format!("square:{row}:{col}")
                } else {
                    format!("piece:{}", piece.id)
                })
            {
                continue;
            }
            result.push(Square {
                row: u8::try_from(row).map_err(|_| {
                    EngineError::InvalidState("v7 ICBM board has too many rows".into())
                })?,
                col: u8::try_from(col).map_err(|_| {
                    EngineError::InvalidState("v7 ICBM board has too many columns".into())
                })?,
            });
        }
    }
    Ok(result)
}

fn queue_id(state: &mut GameState, prefix: &str, suffix_length: usize) -> Result<String> {
    let fraction = state
        .rng
        .sample_opaque("source card status queue identity")?;
    let suffix = crate::draft::random_suffix(fraction)?
        .chars()
        .take(suffix_length)
        .collect::<String>();
    Ok(format!(
        "{prefix}-{}-{suffix}",
        crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?
    ))
}

fn queue_array<'a>(state: &'a mut GameState, field: &str) -> Result<&'a mut Vec<Value>> {
    if !state.extra.get(field).is_some_and(Value::is_array) {
        state.extra.insert(field.into(), json!([]));
    }
    state
        .extra
        .get_mut(field)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState(format!("v7 {field} must be an array")))
}

fn selections(action: &Action, minimum: usize, maximum: usize) -> Result<Vec<Square>> {
    let target = action.target.as_ref().ok_or(EngineError::IllegalAction)?;
    let object = target.as_object().ok_or(EngineError::IllegalAction)?;
    if object.len() != 1 {
        return Err(EngineError::IllegalAction);
    }
    let cells = object
        .get("selections")
        .and_then(Value::as_array)
        .ok_or(EngineError::IllegalAction)?;
    if !(minimum..=maximum).contains(&cells.len()) {
        return Err(EngineError::IllegalAction);
    }
    cells
        .iter()
        .map(|cell| {
            let object = cell.as_object().ok_or(EngineError::IllegalAction)?;
            if object.len() != 2 {
                return Err(EngineError::IllegalAction);
            }
            let row = object
                .get("row")
                .and_then(Value::as_u64)
                .and_then(|value| u8::try_from(value).ok())
                .ok_or(EngineError::IllegalAction)?;
            let col = object
                .get("col")
                .and_then(Value::as_u64)
                .and_then(|value| u8::try_from(value).ok())
                .ok_or(EngineError::IllegalAction)?;
            Ok(Square { row, col })
        })
        .collect()
}

fn combinations(
    state: &GameState,
    card: &CardSlot,
    squares: &[Square],
    maximum: usize,
) -> Vec<Action> {
    fn visit(
        state: &GameState,
        card: &CardSlot,
        squares: &[Square],
        maximum: usize,
        start: usize,
        chosen: &mut Vec<Square>,
        result: &mut Vec<Action>,
    ) {
        if !chosen.is_empty() {
            result.push(Action::card(
                state.turn,
                card,
                Some(json!({"selections":chosen})),
            ));
        }
        if chosen.len() == maximum {
            return;
        }
        for index in start..squares.len() {
            chosen.push(squares[index]);
            visit(state, card, squares, maximum, index + 1, chosen, result);
            chosen.pop();
        }
    }
    let mut result = Vec::new();
    visit(
        state,
        card,
        squares,
        maximum,
        0,
        &mut Vec::new(),
        &mut result,
    );
    result
}

fn direct_available(state: &GameState, card: &CardSlot) -> Result<bool> {
    Ok(match card.id.as_str() {
        "calling-card" => !calling_card_candidates(state).is_empty(),
        "collapse" => collapse_available(state)?,
        "dice" | "gale" => true,
        "hallucination" | "ultimatum" | "vanish" => true,
        // Source hasInsightTarget is a UI hint, while canPlayCard returns true.
        "insight" => true,
        "last-resistance" => king_augment_square(state).is_some(),
        "lobster" => !lobster_targets(state, false)?.is_empty(),
        "mistake-card" => !truthy(
            state
                .extra
                .get("mistakeCard")
                .and_then(|value| value.get(state.turn.opponent().as_str())),
        ),
        "otherworld" => state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.color == state.turn && piece.kind == "pawn"),
        "prophecy" => king_augment_square(state).is_some() && !active_prophecy(state),
        "reverse-pawns" => reverse_pawns_available(state),
        "wanted" => !super::wanted_candidates(state).is_empty(),
        _ => return Err(unported(card, "legal actions")),
    })
}

pub(super) fn ui_targets(state: &GameState, card: &CardSlot) -> Result<Option<Vec<Square>>> {
    if !owned(card) {
        return Ok(None);
    }
    validate_object(state, card)?;
    match card.id.as_str() {
        "panic" => return panic_targets(state).map(Some),
        "spy" => return spy_targets(state).map(Some),
        "chameleon-mutation" => return chameleon_targets(state).map(Some),
        "greek-gift" => return bishop_targets(state).map(Some),
        "lobster" => return lobster_targets(state, true).map(Some),
        "sacrifice" => return sacrifice_targets(state).map(Some),
        "submerge" => return submerge_targets(state).map(Some),
        "suspicious-potion" => return potion_targets(state).map(Some),
        "icbm" => {
            if icbm_queens(state, state.turn.opponent())?.is_empty() {
                return Ok(Some(Vec::new()));
            }
            return icbm_queens(state, state.turn).map(Some);
        }
        _ => {}
    }
    if truthy(card.extra.get("target")) {
        return Err(unported(card, "UI targets"));
    }
    Ok(Some(Vec::new()))
}

pub(super) fn actions(state: &GameState, card: &CardSlot) -> Result<Option<Vec<Action>>> {
    if !owned(card) {
        return Ok(None);
    }
    validate_object(state, card)?;
    match card.id.as_str() {
        "greek-gift" | "lobster" | "submerge" | "suspicious-potion" => {
            let targets = ui_targets(state, card)?.ok_or(EngineError::IllegalAction)?;
            return Ok(Some(
                targets
                    .into_iter()
                    .map(|square| Action::card(state.turn, card, Some(json!(square))))
                    .collect(),
            ));
        }
        "sacrifice" => {
            let mut actions = Vec::new();
            for source in sacrifice_targets(state)? {
                for protected in sacrifice_protection_targets(state, source)? {
                    actions.push(Action::card(
                        state.turn,
                        card,
                        Some(json!({"row":protected.row,"col":protected.col,"sacrifice":source})),
                    ));
                }
            }
            return Ok(Some(actions));
        }
        "chameleon-mutation" => {
            return Ok(Some(combinations(
                state,
                card,
                &chameleon_targets(state)?,
                3,
            )));
        }
        "spy" => {
            let candidates = spy_targets(state)?;
            return Ok(Some(
                combinations(state, card, &candidates, candidates.len().min(2))
                    .into_iter()
                    .filter(|action| {
                        action
                            .target
                            .as_ref()
                            .and_then(|target| target.get("selections"))
                            .and_then(Value::as_array)
                            .is_some_and(|cells| cells.len() == candidates.len().min(2))
                    })
                    .collect(),
            ));
        }
        "panic" => {
            let candidates = panic_selection_targets(state)?;
            let mut actions = Vec::new();
            for (first_index, first) in candidates.iter().enumerate() {
                for (second_index, second) in candidates.iter().enumerate() {
                    if first_index != second_index {
                        actions.push(Action::card(
                            state.turn,
                            card,
                            Some(json!({"selections":[first,second]})),
                        ));
                    }
                }
            }
            return Ok(Some(actions));
        }
        "icbm" => {
            return Ok(Some(
                if icbm_queens(state, state.turn.opponent())?.is_empty() {
                    Vec::new()
                } else {
                    icbm_queens(state, state.turn)?
                        .into_iter()
                        .map(|square| Action::card(state.turn, card, Some(json!(square))))
                        .collect()
                },
            ));
        }
        _ => {}
    }
    if !DIRECT_NO_TARGET.contains(&card.id.as_str()) {
        return Err(unported(card, "legal actions"));
    }
    Ok(Some(if direct_available(state, card)? {
        vec![Action::card(state.turn, card, None)]
    } else {
        Vec::new()
    }))
}

fn validate_action(state: &GameState, card: &CardSlot, action: &Action) -> Result<()> {
    validate_object(state, card)?;
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
    Ok(())
}

pub(super) fn apply(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    validate_action(state, card, action)?;
    // Source probes own a disposable clone. A rejected direct effect retains
    // its earlier mutations and RNG there; public actions still commit once.
    if state.is_ai_simulation() {
        if DIRECT_NO_TARGET.contains(&card.id.as_str()) {
            if super::has_card_selection(action) {
                return Err(EngineError::IllegalAction);
            }
            apply_direct(state, card)?;
            return Ok(Vec::new());
        }
        return apply_targeted(state, card, action);
    }
    let mut staged = state.clone();
    let captures = if DIRECT_NO_TARGET.contains(&card.id.as_str()) {
        if super::has_card_selection(action) || !direct_available(state, card)? {
            return Err(EngineError::IllegalAction);
        }
        apply_direct(&mut staged, card)?;
        Vec::new()
    } else {
        apply_targeted(&mut staged, card, action)?
    };
    *state = staged;
    Ok(captures)
}

/// Nested boxes use their own card object. Only the potion effect writes
/// result metadata here; no temporary hand slot may receive those writes.
pub(super) fn apply_virtual_effect(
    state: &mut GameState,
    card: &mut CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    if card.id != "suspicious-potion" {
        return apply(state, card, action);
    }
    validate_action(state, card, action)?;
    let square = exact_square_target(state, action)?;
    if !potion_targets(state)?.contains(&square) {
        return Err(EngineError::IllegalAction);
    }
    if state.is_ai_simulation() {
        super::apply_potion_virtual_effect(state, card, action)?;
        return Ok(Vec::new());
    }
    let mut staged = state.clone();
    let mut staged_card = card.clone();
    super::apply_potion_virtual_effect(&mut staged, &mut staged_card, action)?;
    *state = staged;
    *card = staged_card;
    Ok(Vec::new())
}

fn canonical_selection_indices(
    candidates: &[Square],
    selected: &[Square],
    ordered: bool,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut previous = None;
    for square in selected {
        let index = candidates
            .iter()
            .position(|candidate| candidate == square)
            .ok_or(EngineError::IllegalAction)?;
        if !seen.insert(index) || (ordered && previous.is_some_and(|earlier| index <= earlier)) {
            return Err(EngineError::IllegalAction);
        }
        previous = Some(index);
    }
    Ok(())
}

fn apply_targeted(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let mut captures = Vec::new();
    match card.id.as_str() {
        // main:99400-99416. Reservation uses one complete base-36 RNG suffix
        // and matures after two half turns, even if a later move blocks it.
        "lobster" => {
            let square = exact_square_target(state, action)?;
            if !lobster_targets(state, true)?.contains(&square) {
                return Err(EngineError::IllegalAction);
            }
            let due = state
                .move_count
                .checked_add(2)
                .ok_or_else(|| EngineError::InvalidState("v7 lobster due move overflow".into()))?;
            let actor = state.turn;
            let entry = json!({"id":queue_id(state,"lobster-pending",usize::MAX)?,"color":actor,"by":actor,
                "row":square.row,"col":square.col,"dueMoveCount":due,"remainingHalfTurns":2});
            queue_array(state, "pendingLobsters")?.push(entry);
        }
        // main:713-719. The source removes the bishop before setting the
        // pending end-turn flag; the pawn blast belongs to the queued callback.
        "greek-gift" => {
            let square = exact_square_target(state, action)?;
            if !bishop_targets(state)?.contains(&square) {
                return Err(EngineError::IllegalAction);
            }
            let removed =
                crate::transition::expansion_sacrifice(state, square, state.turn.opponent())?
                    .ok_or(EngineError::IllegalAction)?;
            captures.push(removed);
            let actor = state.turn;
            let pending = state
                .extra
                .entry("greekGiftPending")
                .or_insert_with(|| json!({}));
            if !pending.is_object() {
                return Err(EngineError::InvalidState(
                    "v7 greekGiftPending must be an object".into(),
                ));
            }
            pending[actor.as_str()] = json!(true);
        }
        // main:98809-98825. Refresh runs again at the parent card boundary,
        // but immediate adjacency can reveal this just-applied status.
        "submerge" => {
            let square = exact_square_target(state, action)?;
            if !submerge_targets(state)?.contains(&square) {
                return Err(EngineError::IllegalAction);
            }
            let mut piece = state
                .at(square)
                .cloned()
                .ok_or(EngineError::IllegalAction)?;
            piece.extra.insert("submerged".into(), json!(true));
            write_piece(state, &piece);
            crate::transition::refresh_submerged(state)?;
        }
        // main:99748-99876. The established potion kernel carries the
        // source-ordered pool, status provenance and mutable card result.
        "suspicious-potion" => {
            let square = exact_square_target(state, action)?;
            if !potion_targets(state)?.contains(&square) {
                return Err(EngineError::IllegalAction);
            }
            super::apply_potion(state, card, action)?;
        }
        // main:104111-104148. An explicit source+recipient target consumes no
        // random draw; the unselected AI branch is outside public card intents.
        "sacrifice" => {
            let target = action.target.as_ref().ok_or(EngineError::IllegalAction)?;
            let object = target.as_object().ok_or(EngineError::IllegalAction)?;
            if object.len() != 3 {
                return Err(EngineError::IllegalAction);
            }
            let source: Square = serde_json::from_value(
                object
                    .get("sacrifice")
                    .cloned()
                    .ok_or(EngineError::IllegalAction)?,
            )
            .map_err(|_| EngineError::IllegalAction)?;
            if object
                .get("sacrifice")
                .and_then(Value::as_object)
                .is_none_or(|source| source.len() != 2)
            {
                return Err(EngineError::IllegalAction);
            }
            let protected: Square =
                serde_json::from_value(json!({"row":object.get("row"),"col":object.get("col")}))
                    .map_err(|_| EngineError::IllegalAction)?;
            if !sacrifice_targets(state)?.contains(&source)
                || !sacrifice_protection_targets(state, source)?.contains(&protected)
            {
                return Err(EngineError::IllegalAction);
            }
            let actor = state.turn;
            let recipient_id = state
                .at(protected)
                .ok_or(EngineError::IllegalAction)?
                .id
                .clone();
            // The direct card calls removeSacrificedPiece and only plays a
            // local vanish effect. Unlike expansion sacrifice, it does not
            // enqueue a board-change replay visual.
            let removed = crate::transition::sacrifice(state, source, actor.opponent())?
                .ok_or(EngineError::IllegalAction)?;
            crate::card_effects::mark_vanish_animation(state, &removed, source)?;
            captures.push(removed);
            // Removal may update this identity through reaper progress or
            // Recycling's holdout promotion. The source retains the live
            // recipient reference rather than a pre-removal piece copy.
            let mut recipient = if recipient_id.is_empty() {
                state.at(protected)
            } else {
                state
                    .board
                    .iter()
                    .flatten()
                    .flatten()
                    .find(|piece| piece.id == recipient_id)
            }
            .cloned()
            .ok_or_else(|| {
                EngineError::InvalidState(format!(
                    "v7 sacrifice protection recipient {recipient_id} disappeared during removal"
                ))
            })?;
            let previous_protected = truthy(recipient.extra.get("protected"));
            recipient.extra.insert(
                "sacrificeProtection".into(),
                json!({"by":actor,"remaining":3,"previousProtected":previous_protected}),
            );
            recipient.extra.insert("protected".into(), json!(true));
            write_piece(state, &recipient);
            mark_animation(state, &recipient)?;
            crate::flow::mark_progress(state);
            crate::v7_capture_objectives::check_campaign_objectives(state)?;
        }
        // main:105195-105213. Mutating a piece does not mark it for animation.
        "chameleon-mutation" => {
            let selected = selections(action, 1, 3)?;
            canonical_selection_indices(&chameleon_targets(state)?, &selected, false)?;
            for square in selected {
                let mut piece = state
                    .at(square)
                    .cloned()
                    .ok_or(EngineError::IllegalAction)?;
                piece.extra.insert("chameleon".into(), json!(true));
                write_piece(state, &piece);
            }
        }
        // main:104778-104797. Exactly min(2, distinct enemy pawns) are
        // selected; the UI enumerator emits combinations in row-major order.
        "spy" => {
            let candidates = spy_targets(state)?;
            let required = candidates.len().min(2);
            if required == 0 {
                return Err(EngineError::IllegalAction);
            }
            let selected = selections(action, required, required)?;
            canonical_selection_indices(&candidates, &selected, false)?;
            for square in selected {
                let mut piece = state
                    .at(square)
                    .cloned()
                    .ok_or(EngineError::IllegalAction)?;
                piece.extra.insert("spyOwner".into(), json!(state.turn));
                write_piece(state, &piece);
            }
        }
        // main:104586-104606. Pair order is observable in pendingPanic and
        // all source UI permutations are legal, unlike combination cards.
        "panic" => {
            let candidates = panic_selection_targets(state)?;
            if candidates.len() < 2 {
                return Err(EngineError::IllegalAction);
            }
            let selected = selections(action, 2, 2)?;
            canonical_selection_indices(&candidates, &selected, false)?;
            let pieces = selected
                .iter()
                .map(|square| {
                    let piece = state.at(*square).ok_or(EngineError::IllegalAction)?;
                    Ok(json!({"id":piece.id,"row":square.row,"col":square.col}))
                })
                .collect::<Result<Vec<_>>>()?;
            let pending = state
                .extra
                .entry("pendingPanic")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or_else(|| {
                    EngineError::InvalidState("v7 pendingPanic must be an array".into())
                })?;
            pending.push(json!({"color":state.turn.opponent(),"by":state.turn,"pieces":pieces}));
        }
        // main:104181-104198. The source first selects the opposing queen
        // identity in board order, then consumes one RNG draw for the queue ID.
        "icbm" => {
            let target = action.target.as_ref().ok_or(EngineError::IllegalAction)?;
            let selected: Square =
                serde_json::from_value(target.clone()).map_err(|_| EngineError::IllegalAction)?;
            if target.as_object().is_none_or(|object| object.len() != 2)
                || !icbm_queens(state, state.turn)?.contains(&selected)
            {
                return Err(EngineError::IllegalAction);
            }
            let enemy = icbm_queens(state, state.turn.opponent())?
                .into_iter()
                .next()
                .ok_or(EngineError::IllegalAction)?;
            let own_id = state
                .at(selected)
                .ok_or(EngineError::IllegalAction)?
                .id
                .clone();
            let enemy_id = state
                .at(enemy)
                .ok_or(EngineError::IllegalAction)?
                .id
                .clone();
            let trigger = state
                .turns_taken
                .get(state.turn)
                .checked_add(1)
                .ok_or_else(|| EngineError::InvalidState("v7 ICBM trigger turn overflow".into()))?;
            let entry = json!({"id":queue_id(state,"icbm",6)?,"color":state.turn,
                "triggerTurn":trigger,"sourceQueenId":own_id,"targetQueenId":enemy_id});
            queue_array(state, "pendingIcbm")?.push(entry);
        }
        _ => return Err(unported(card, "effect")),
    }
    Ok(captures)
}

fn apply_direct(state: &mut GameState, card: &CardSlot) -> Result<()> {
    match card.id.as_str() {
        // main:99450-99469. A single source randomChoice draw chooses one
        // row-major unique opponent identity. The action log observes the
        // selected piece's visibility before it is committed.
        "calling-card" => {
            let candidates = calling_card_candidates(state);
            let index = crate::transition::sample_choice(state, candidates.len())?;
            let square = candidates[index];
            let mut piece = state
                .at(square)
                .cloned()
                .ok_or(EngineError::IllegalAction)?;
            piece
                .extra
                .insert("callingCard".into(), json!({"by":state.turn}));
            write_piece(state, &piece);
            mark_animation(state, &piece)?;
            let file = char::from(b'a' + square.col);
            let rank = state.board.len() - usize::from(square.row);
            crate::replay::add_piece_action_log(
                state,
                &piece,
                Some(square),
                None,
                format!("예고장: {file}{rank}의 기물이 표적이 되었습니다."),
            )?;
        }
        // main:105787-105791. The actual collapse waits until the opponent's
        // turn ends; queueing this card consumes no random value.
        "collapse" => {
            state
                .extra
                .insert("collapsePending".into(), json!(state.turn));
        }
        // main:101301-101309. One draw chooses one of the six source types;
        // the opponent's previous lock is replaced, not stacked.
        "dice" => {
            const TYPES: &[(&str, &str)] = &[
                ("pawn", "폰"),
                ("knight", "나이트"),
                ("bishop", "비숍"),
                ("rook", "룩"),
                ("queen", "퀸"),
                ("king", "킹"),
            ];
            let roll = crate::transition::sample_choice(state, TYPES.len())?;
            let locks = state
                .extra
                .get_mut("diceLocks")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| {
                    EngineError::InvalidState("v7 diceLocks must be a color map".into())
                })?;
            locks.insert(
                state.turn.opponent().as_str().into(),
                json!({"type":TYPES[roll].0,"remaining":2}),
            );
            crate::replay::add_log(state, format!("주사위: {} ({})", roll + 1, TYPES[roll].1))?;
        }
        // main:98874-98883. The adopted source profile counts three opponent
        // turns and also retains triggerTurn for replay/import compatibility.
        "gale" => {
            let september18 = crate::v7_queued_effects::uses_september18_balance(state)?;
            let id = queue_id(state, "gale", 6)?;
            let trigger = state
                .turns_taken
                .get(state.turn)
                .checked_add(3)
                .ok_or_else(|| EngineError::InvalidState("v7 gale trigger turn overflow".into()))?;
            let mut entry = json!({"id":id,"color":state.turn,"triggerTurn":trigger});
            if september18 {
                entry["remainingOwnTurns"] = json!(3);
            }
            queue_array(state, "pendingGales")?.push(entry);
        }
        // main:104557-104562. The color key denotes the viewer, not owner.
        "hallucination" => {
            let actor = state.turn;
            color_map(state, "hallucination")?.insert(
                actor.opponent().as_str().into(),
                json!({"color":actor,"remaining":5}),
            );
        }
        "insight" => apply_insight(state)?,
        // main:103363-103374. Replaying the card preserves the first
        // protection baseline even if another effect currently protects it.
        "last-resistance" => {
            let square = king_augment_square(state).ok_or(EngineError::IllegalAction)?;
            let mut king = state
                .at(square)
                .cloned()
                .ok_or(EngineError::IllegalAction)?;
            let baseline = king
                .extra
                .get("lastResistance")
                .filter(|value| truthy(Some(value)))
                .map(|value| truthy(value.get("previousProtected")))
                .unwrap_or_else(|| truthy(king.extra.get("protected")));
            king.extra.insert(
                "lastResistance".into(),
                json!({"by":state.turn,"remaining":3,"previousProtected":baseline}),
            );
            king.extra.insert("protected".into(), json!(true));
            write_piece(state, &king);
        }
        // main:100703-100709. The source normalizes both color booleans before
        // checking the opponent flag.
        "mistake-card" => {
            crate::replay::normalize_color_booleans(state, "mistakeCard");
            let enemy = state.turn.opponent();
            if truthy(
                state
                    .extra
                    .get("mistakeCard")
                    .and_then(|value| value.get(enemy.as_str())),
            ) {
                return Err(EngineError::IllegalAction);
            }
            color_map(state, "mistakeCard")?.insert(enemy.as_str().into(), json!(true));
        }
        // main:99084-99103. Reuse the v7-aware scheduler writer so first-move
        // automatic play and manual play create identical queue and visual.
        "otherworld" => crate::transition::apply_otherworld(state)?,
        // main:105802-105813. The timer and forced animation are part of the
        // direct card effect; the capture-free victory check runs later.
        "prophecy" => {
            let square = king_augment_square(state).ok_or(EngineError::IllegalAction)?;
            let royal = state
                .at(square)
                .cloned()
                .ok_or(EngineError::IllegalAction)?;
            if active_prophecy(state) {
                return Err(EngineError::IllegalAction);
            }
            let actor = state.turn;
            color_map(state, "prophecy")?.insert(
                actor.as_str().into(),
                json!({"by":actor,"remainingHalfTurns":6}),
            );
            mark_animation(state, &royal)?;
        }
        // main:104234-104238. The source stores a counter on the target
        // color and extends an existing longer reversal instead of shortening it.
        "reverse-pawns" => {
            let effects = state
                .extra
                .get_mut("effects")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| EngineError::InvalidState("v7 effects must be an object".into()))?;
            let reverse = effects
                .get_mut("pawnReverse")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| {
                    EngineError::InvalidState("v7 effects.pawnReverse must be a color map".into())
                })?;
            let slot = reverse
                .get_mut(state.turn.opponent().as_str())
                .ok_or_else(|| {
                    EngineError::InvalidState("v7 opponent pawnReverse counter missing".into())
                })?;
            let current = slot.as_u64().ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 pawnReverse counter must be a nonnegative integer".into(),
                )
            })?;
            *slot = json!(current.max(3));
        }
        // main:105792-105801. The caller's replay step records the new timer;
        // no piece has moved yet, so movedIds starts empty.
        "ultimatum" => {
            state.extra.insert(
                "ultimatum".into(),
                json!({"by":state.turn,"remaining":4,"remainingHalfTurns":8,"expiresFullMove":null,"movedIds":[]}),
            );
        }
        // main:104176-104180. Repeated use keeps both established flags.
        "vanish" => {
            crate::replay::normalize_color_booleans(state, "vanishing");
            let actor = state.turn;
            color_map(state, "vanishing")?.insert(actor.as_str().into(), json!(true));
        }
        // main:732-745, 68627 and 100128. Candidate identities remain in
        // expansionBoardEntries order and the source consumes one RNG draw.
        // Arrest resolution after submerged is revealed belongs to the turn
        // callback, not this direct card effect.
        "wanted" => super::apply_wanted(state)?,
        _ => return Err(unported(card, "effect")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn source_state_digest(state: &GameState) -> String {
        let mut state = serde_json::to_value(state).unwrap();
        let fields = state.as_object_mut().unwrap();
        for outer in ["rulesetId", "rng", "history"] {
            fields.remove(outer);
        }
        format!("{:x}", Sha256::digest(serde_jcs::to_vec(&state).unwrap()))
    }

    fn source_card(id: &str) -> CardSlot {
        let mut card: CardSlot = serde_json::from_value(
            crate::card_registry::definition_for(RULES_VERSION_V7, id)
                .unwrap()
                .source_definition
                .clone(),
        )
        .unwrap();
        card.instance_id = format!("{id}-status-test");
        card
    }

    #[test]
    fn rejected_simulation_keeps_source_normalization_and_public_state_is_atomic() {
        let mut public = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            37,
        )
        .unwrap();
        // main:100703-100709 normalizes both colors before returning false
        // for an already-active opponent. This mutation belongs to the probe.
        public
            .extra
            .insert("mistakeCard".into(), json!({"white":0,"black":true}));
        let card = source_card("mistake-card");
        let action = Action::card(public.turn, &card, None);
        let before = public.clone();
        assert_eq!(
            apply(&mut public, &card, &action),
            Err(EngineError::IllegalAction)
        );
        assert_eq!(public, before);

        let mut probe = before.clone();
        probe.ai_simulation_depth = 1;
        assert_eq!(
            apply(&mut probe, &card, &action),
            Err(EngineError::IllegalAction)
        );
        assert_eq!(
            probe.extra["mistakeCard"],
            json!({"white":false,"black":true})
        );
        assert_eq!(probe.rng, before.rng);
        assert_eq!(probe.history, before.history);
        assert_eq!(probe.board, before.board);
    }

    #[test]
    fn remaining_status_cards_match_frozen_source_direct_full_state() {
        // FrozenClientSource SHA e5ed84fc / normal draftDelete seed 37,
        // `applyCardEffect(CARD_BY_ID.get(id), target)` and full-state JCS.
        // No source snapshot or generated report is committed to this test.
        for (id, target, digest, cursor, rng_state) in [
            (
                "gale",
                None,
                "d3ac1407d42bf2491c6c14075cd39eca352ad8a1a6a72c53a1d0f7eddcec4252",
                33,
                57_544_672_u32,
            ),
            (
                "greek-gift",
                Some(json!({"row":7,"col":2})),
                "e000d1a02678d5452569323c51c002bc67a617f874aa91f8872f44d558bafa24",
                32,
                3_271_378_757_u32,
            ),
            (
                "submerge",
                Some(json!({"row":6,"col":0})),
                "5d4fd46e25bf190cb4e5da933028ca2704dc18cb612b944e78ef4c26ff148942",
                32,
                3_271_378_757,
            ),
            (
                "lobster",
                Some(json!({"row":4,"col":4})),
                "aaa35e28ad4c16efa591f14942c90314fc177a3966a946bb95dccbe14dfb3dc9",
                33,
                57_544_672,
            ),
            (
                "sacrifice",
                Some(json!({"row":6,"col":0,"sacrifice":{"row":7,"col":3}})),
                "2db638c4860cf3bff5b6c6e041b1cd0f6e5eae35f2e39f9c2f9bd4fed37717e9",
                32,
                3_271_378_757,
            ),
            (
                "suspicious-potion",
                Some(json!({"row":6,"col":0})),
                "468aaad28e0309d449577bf429aeacf79a685a570d2ce6e9886a13209ce21480",
                33,
                57_544_672,
            ),
            (
                "insight",
                None,
                "d4c7e707a95a4faecca5b4187993fcb20d03b9d93ff2956b3b559eff65f05b55",
                32,
                3_271_378_757,
            ),
        ] {
            let mut state = crate::v7_new_game::new_game(
                GameConfig {
                    draft_delete: true,
                    ..GameConfig::default()
                },
                37,
            )
            .unwrap();
            let card = source_card(id);
            let action = Action::card(state.turn, &card, target);
            let candidates = actions(&state, &card).unwrap().unwrap();
            assert!(candidates.contains(&action), "{id} direct source candidate");
            apply(&mut state, &card, &action).unwrap();
            assert_eq!(
                source_state_digest(&state),
                digest,
                "{id} full source state"
            );
            assert_eq!(
                (state.rng.cursor, state.rng.state),
                (cursor, rng_state),
                "{id} RNG"
            );
        }
    }

    #[test]
    fn otherworld_uses_source_seed37_pawn_and_two_random_draws() {
        // Direct frozen-client call `applyCardEffect(CARD_BY_ID.get('otherworld'),
        // null)` after normal/draftDelete newGame(seed=37) selected a2 and
        // produced this queue identity and RNG cursor/state.
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            37,
        )
        .unwrap();
        assert_eq!(state.mode, "play");
        let card = source_card("otherworld");
        let action = Action::card(state.turn, &card, None);
        assert_eq!(actions(&state, &card).unwrap(), Some(vec![action.clone()]));
        let captured = apply(&mut state, &card, &action).unwrap();
        assert!(captured.is_empty());
        let expected = json!({
            "id":"otherworld-1790581292828-ta4l0t",
            "color":"white",
            "pieceId":"white-pawn-vgte1dk3jh",
            "row":6,"col":0,"origin":"a2",
            "dueMoveCount":28,"remainingHalfTurns":28
        });
        assert_eq!(state.extra["pendingOtherworld"], json!([expected]));
        assert!(state.board[6][0].is_none());
        assert_eq!(state.rng.cursor, 34);
        assert_eq!(state.rng.state, 3_493_396_927);
    }

    #[test]
    fn status_identity_and_rejected_target_leave_state_unchanged() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            37,
        )
        .unwrap();
        let card = source_card("ultimatum");
        let mut forged = Action::card(state.turn, &card, Some(json!({"row":0,"col":0})));
        let before = state.clone();
        assert!(matches!(
            apply(&mut state, &card, &forged),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(state, before);
        forged.target = None;
        forged.card_instance_id = Some("another-instance".into());
        assert!(matches!(
            apply(&mut state, &card, &forged),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn grand_seed37_panic_first_click_matches_frozen_source_rows() {
        let state = crate::v7_new_game::new_game(
            GameConfig {
                game_style: "grand".into(),
                draft_delete: true,
                ..GameConfig::default()
            },
            37,
        )
        .unwrap();
        let card = source_card("panic");
        let expected = [
            (0, 0),
            (0, 1),
            (0, 2),
            (0, 3),
            (0, 5),
            (0, 6),
            (0, 7),
            (1, 0),
            (1, 1),
            (1, 2),
            (1, 3),
            (1, 4),
            (1, 5),
            (1, 6),
            (1, 7),
        ]
        .into_iter()
        .map(|(row, col)| Square { row, col })
        .collect::<Vec<_>>();
        assert_eq!(ui_targets(&state, &card).unwrap(), Some(expected));
    }

    #[test]
    fn panic_selection_plans_use_distinct_piece_identities() {
        // The source's first-click surface retains cells, but its complete
        // plans de-duplicate identities at main:105853 and panic() rejects
        // two submitted cells referring to the same piece at main:104587.
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            37,
        )
        .unwrap();
        let card = source_card("panic");
        let original = Square { row: 1, col: 0 };
        let alias = Square { row: 2, col: 0 };
        state.board[usize::from(alias.row)][usize::from(alias.col)] = state.at(original).cloned();
        let raw = ui_targets(&state, &card).unwrap().unwrap();
        assert!(raw.contains(&original) && raw.contains(&alias));
        let plans = actions(&state, &card).unwrap().unwrap();
        let distinct = raw.len() - 1;
        assert_eq!(plans.len(), distinct * (distinct - 1));
        for action in &plans {
            let selected = selections(action, 2, 2).unwrap();
            assert_ne!(
                state.at(selected[0]).unwrap().id,
                state.at(selected[1]).unwrap().id
            );
        }
        let invalid = Action::card(
            state.turn,
            &card,
            Some(json!({"selections":[original,alias]})),
        );
        let before = state.clone();
        assert!(matches!(
            apply(&mut state, &card, &invalid),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn virtual_potion_result_belongs_to_the_supplied_card_object() {
        // The nested Box effect receives a separate card object. Source
        // suspiciousPotion writes the chosen result to that object, without
        // inserting a hand slot or spending another random value.
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            37,
        )
        .unwrap();
        let mut card = source_card("suspicious-potion");
        let hand = state.deck_slots.clone();
        let cursor = state.rng.cursor;
        let action = Action::card(state.turn, &card, Some(json!({"row":6,"col":0})));
        apply_virtual_effect(&mut state, &mut card, &action).unwrap();
        assert_eq!(
            card.extra["suspiciousPotionResultId"],
            "sacrificeProtection"
        );
        assert_eq!(state.deck_slots, hand);
        assert_eq!(state.rng.cursor, cursor + 1);

        let invalid = Action::card(state.turn, &card, Some(json!({"row":4,"col":4})));
        let before = (state.clone(), card.clone());
        assert!(matches!(
            apply_virtual_effect(&mut state, &mut card, &invalid),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!((state, card), before);
    }

    #[test]
    fn selected_status_cards_keep_source_candidates_and_accept_ui_click_order() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            37,
        )
        .unwrap();
        let rng_before = state.rng.clone();

        let mutation = source_card("chameleon-mutation");
        let mutation_targets = chameleon_targets(&state).unwrap();
        assert!(mutation_targets.len() >= 3);
        let mutation_actions = actions(&state, &mutation).unwrap().unwrap();
        let n = mutation_targets.len();
        assert_eq!(
            mutation_actions.len(),
            n + n * (n - 1) / 2 + n * (n - 1) * (n - 2) / 6
        );
        let first = mutation_targets[0];
        let second = mutation_targets[1];
        let out_of_order = Action::card(
            state.turn,
            &mutation,
            Some(json!({"selections":[second,first]})),
        );
        // The source collector emits index-ordered combinations, while direct
        // UI selections retain click order. These two effects accept either.
        let mut canonical_state = state.clone();
        apply(&mut state, &mutation, &out_of_order).unwrap();
        let selected = Action::card(
            state.turn,
            &mutation,
            Some(json!({"selections":[first,second]})),
        );
        apply(&mut canonical_state, &mutation, &selected).unwrap();
        assert_eq!(state, canonical_state);
        assert_eq!(state.at(first).unwrap().extra["chameleon"], true);
        assert_eq!(state.at(second).unwrap().extra["chameleon"], true);
        assert_eq!(state.rng, rng_before);

        let spy = source_card("spy");
        let spy_targets = spy_targets(&state).unwrap();
        assert!(spy_targets.len() >= 2);
        let spy_actions = actions(&state, &spy).unwrap().unwrap();
        assert_eq!(
            spy_actions.len(),
            spy_targets.len() * (spy_targets.len() - 1) / 2
        );
        let spies = [spy_targets[0], spy_targets[1]];
        let mut canonical_state = state.clone();
        let selected = Action::card(state.turn, &spy, Some(json!({"selections":spies})));
        let reversed = Action::card(
            state.turn,
            &spy,
            Some(json!({"selections":[spies[1],spies[0]]})),
        );
        apply(&mut state, &spy, &reversed).unwrap();
        apply(&mut canonical_state, &spy, &selected).unwrap();
        assert_eq!(state, canonical_state);
        assert_eq!(
            state.at(spies[0]).unwrap().extra["spyOwner"],
            json!(state.turn)
        );
        assert_eq!(
            state.at(spies[1]).unwrap().extra["spyOwner"],
            json!(state.turn)
        );
        assert_eq!(state.rng, rng_before);

        let panic = source_card("panic");
        let panic_targets = panic_targets(&state).unwrap();
        assert!(panic_targets.len() >= 2);
        assert_eq!(
            actions(&state, &panic).unwrap().unwrap().len(),
            panic_targets.len() * (panic_targets.len() - 1)
        );
        let [a, b] = [panic_targets[0], panic_targets[1]];
        let reversed = Action::card(state.turn, &panic, Some(json!({"selections":[b,a]})));
        apply(&mut state, &panic, &reversed).unwrap();
        assert_eq!(state.extra["pendingPanic"][0]["pieces"][0]["row"], b.row);
        assert_eq!(state.extra["pendingPanic"][0]["pieces"][0]["col"], b.col);
        assert_eq!(state.extra["pendingPanic"][0]["pieces"][1]["row"], a.row);
        assert_eq!(state.extra["pendingPanic"][0]["pieces"][1]["col"], a.col);
        assert_eq!(state.rng, rng_before);
    }

    #[test]
    fn wanted_consumes_one_draw_for_an_enemy_ranged_identity() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            37,
        )
        .unwrap();
        let card = source_card("wanted");
        let candidates = super::super::wanted_candidates(&state);
        assert!(!candidates.is_empty());
        let cursor = state.rng.cursor;
        let action = Action::card(state.turn, &card, None);
        apply(&mut state, &card, &action).unwrap();
        let marked = candidates
            .into_iter()
            .filter_map(|square| state.at(square))
            .filter(|piece| piece.extra.get("wanted") == Some(&json!({"by":"white"})))
            .collect::<Vec<_>>();
        assert_eq!(marked.len(), 1);
        assert_eq!(marked[0].extra["submerged"], true);
        assert_eq!(state.rng.cursor, cursor + 1);
    }
}
