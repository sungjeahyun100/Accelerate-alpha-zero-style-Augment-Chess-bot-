//! Frozen v7 campaign objectives, Highlander, flag occupation, and Vigilance callbacks.
//!
//! Source: main-OahWs0tU.js, SHA-256
//! e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c.
//! `endMove` checks `updateCaptureFlags(actor, false)` after submerged pieces
//! refresh and before Herald threats. `completeTurnAfterMove` calls
//! `updateCaptureFlags(actor, true)` after resetting active-card use and before
//! `tickVigilanceProtection`, then changes the actor. These are separate source
//! boundaries: the latter observes the already incremented `turnsTaken`.

use crate::v7_turn_flow::V7FlowControl;
use crate::{Color, EngineError, GameState, Piece, RULES_VERSION_V7, Result};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const SEPTEMBER22_HASHES: &[&str] = &[
    "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
    "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
    "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs",
    "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc",
    "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g",
];
const SEPTEMBER18_HASHES: &[&str] = &[
    "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
    "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
    "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs",
    "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc",
    "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g",
    "HVzv4vxzNKDRi-ylshTw9HnLmIlyiQhMScQ4caG_aSI",
];

fn checked_boundary(state: &GameState, _actor: Color) -> Result<Option<V7FlowControl>> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 capture objectives on rules version {}",
            state.ruleset_id
        )));
    }
    if state.mode == "gameover" {
        return Ok(Some(V7FlowControl::Terminal));
    }
    // Source endMove/completeTurnAfterMove receives movingColor. A Spy can
    // change that color before settlement without changing state.turn.
    // Public actor admission and incoming callbacks enforce their own actor.
    if state.mode != "play" {
        return Err(EngineError::InvalidState(format!(
            "v7 capture objectives require play mode, received {}",
            state.mode
        )));
    }
    let Some(width) = state.board.first().map(Vec::len) else {
        return Err(EngineError::InvalidState(
            "v7 capture objectives require a nonempty board".into(),
        ));
    };
    if width == 0 || state.board.iter().any(|row| row.len() != width) {
        return Err(EngineError::InvalidState(
            "v7 capture objectives require a rectangular board".into(),
        ));
    }
    Ok(None)
}

/// The `endMove` call of `updateCaptureFlags(actor, false)`. Its return must
/// short-circuit the rest of `endMove` on a Highlander or flag result.
pub(crate) fn after_end_move_reactions(
    state: &mut GameState,
    actor: Color,
) -> Result<V7FlowControl> {
    if let Some(flow) = checked_boundary(state, actor)? {
        return Ok(flow);
    }
    if !has_active_objective(state) {
        return Ok(V7FlowControl::Continue);
    }
    let mut working = state.clone();
    let flow = update_capture_flags(&mut working, actor, false)?;
    *state = working;
    Ok(flow)
}

/// The post-count call of `updateCaptureFlags(actor, true)`, followed by
/// `tickVigilanceProtection`. The incoming actor has not been installed yet.
pub(crate) fn after_completed_turn_count(
    state: &mut GameState,
    actor: Color,
) -> Result<V7FlowControl> {
    if let Some(flow) = checked_boundary(state, actor)? {
        return Ok(flow);
    }
    if !has_active_objective(state) && !has_actor_vigilance(state, actor) {
        return Ok(V7FlowControl::Continue);
    }
    let mut working = state.clone();
    let flow = update_capture_flags(&mut working, actor, true)?;
    if flow != V7FlowControl::Terminal {
        tick_vigilance_protection(&mut working, actor)?;
    }
    *state = working;
    Ok(flow)
}

fn has_active_objective(state: &GameState) -> bool {
    [Color::White, Color::Black].into_iter().any(|color| {
        state
            .extra
            .get("highlander")
            .and_then(|value| value.get(color.as_str()))
            .is_some_and(js_truth)
    }) || state.extra.get("captureTheFlag").is_some_and(js_truth)
}

fn has_actor_vigilance(state: &GameState, actor: Color) -> bool {
    state.board.iter().flatten().flatten().any(|piece| {
        piece
            .extra
            .get("vigilanceProtection")
            .and_then(|value| value.get("countBy"))
            .and_then(Value::as_str)
            == Some(actor.as_str())
    })
}

/// Source `checkCampaignObjectives()` is called at several capture, removal,
/// transformation, and board-effect boundaries. It is independent of the
/// completed-turn flag callback: the caller must invoke it at the source site,
/// after its own effect has updated the board. A true result means this call
/// ended the game; an already terminal game returns false, as in the source.
///
/// The frozen source has a second group of checks below the setup-specific
/// branches. Every setup in that group returns from its first branch, so the
/// second group is unreachable for this source revision.
pub(crate) fn check_campaign_objectives(state: &mut GameState) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 campaign objectives on rules version {}",
            state.ruleset_id
        )));
    }
    let Some(campaign) = state.extra.get("campaign").filter(|value| js_truth(value)) else {
        return Ok(false);
    };
    if state.mode == "gameover" {
        return Ok(false);
    }
    let setup = campaign.get("setup").and_then(Value::as_str);
    let outcome = match setup {
        Some("machineRebellion") if !campaign_has_pieces(state, Color::Black, &["colossus"]) => {
            Some((Color::White, "거신병 3개를 모두 파괴했습니다."))
        }
        Some("maharajaSepoy") if !campaign_has_pieces(state, Color::Black, &["amazon"]) => {
            Some((Color::White, "아마존을 쓰러뜨렸습니다."))
        }
        Some("magicParty") if !campaign_has_pieces(state, Color::White, &["wizard"]) => {
            Some((Color::Black, "모든 마법사가 쓰러졌습니다."))
        }
        Some("bloodMoon") if !campaign_has_pieces(state, Color::White, &["vampireLord"]) => {
            Some((Color::Black, "뱀파이어 군주가 쓰러졌습니다."))
        }
        Some("timeTraveler") if !campaign_has_pieces(state, Color::White, &["timeTraveler"]) => {
            Some((Color::Black, "시간 여행자가 쓰러졌습니다."))
        }
        Some("majorPiece") if !campaign_has_pieces(state, Color::Black, &["king"]) => {
            Some((Color::White, "상대 킹을 잡았습니다."))
        }
        Some("knightJourney" | "knightGame") => {
            let knight_color = if campaign.get("playerColor").and_then(Value::as_str)
                == Some(Color::Black.as_str())
            {
                Color::Black
            } else {
                Color::White
            };
            (!campaign_has_pieces(state, knight_color, &["knight"]))
                .then_some((knight_color.opponent(), "나이트가 사라졌습니다."))
        }
        _ => None,
    };
    let Some((winner, reason)) = outcome else {
        return Ok(false);
    };
    let mut working = state.clone();
    crate::flow::end_game(&mut working, Some(winner), reason)?;
    *state = working;
    Ok(true)
}

/// Source `campaignHasPieces` visits cells row-major and counts the first
/// occurrence of each piece ID for the requested color. This matters for a
/// large piece occupying several cells and for malformed duplicate IDs.
fn campaign_has_pieces(state: &GameState, color: Color, kinds: &[&str]) -> bool {
    let mut seen = BTreeSet::new();
    for piece in state.board.iter().flatten().flatten() {
        if piece.color != color || !seen.insert(piece.id.as_str()) {
            continue;
        }
        if kinds.contains(&piece.kind.as_str()) {
            return true;
        }
    }
    false
}

pub(crate) fn update_capture_flags(
    state: &mut GameState,
    actor: Color,
    completed: bool,
) -> Result<V7FlowControl> {
    if check_internal_highlander(state)? {
        return Ok(V7FlowControl::Terminal);
    }
    if state.mode == "gameover" {
        return Ok(V7FlowControl::Continue);
    }
    let Some(rule) = state.extra.get("captureTheFlag").cloned() else {
        return Ok(V7FlowControl::Continue);
    };
    if rule.is_null() || rule == Value::Bool(false) {
        return Ok(V7FlowControl::Continue);
    }
    let flags = rule
        .as_object()
        .ok_or_else(|| EngineError::InvalidState("v7 captureTheFlag must be an object".into()))?
        .get("flags")
        .and_then(Value::as_object);
    let Some(flags) = flags else {
        // `septemberAdvanceFlags` returns the same rule when `flags` is absent.
        return Ok(V7FlowControl::Continue);
    };
    let mut next = rule.clone();
    if next.get("occupations").is_none_or(Value::is_null) {
        next["occupations"] = json!({"white":null,"black":null});
    }
    if !next["occupations"].is_object() {
        return Err(EngineError::InvalidState(
            "v7 captureTheFlag.occupations must be an object".into(),
        ));
    }
    let mut defeated = Vec::new();
    for owner in [Color::White, Color::Black] {
        let occupant = flag_occupant(state, flags.get(owner.as_str()))?;
        let Some(occupant) = occupant.filter(|piece| {
            !piece.id.is_empty() && piece.color.owner().is_some_and(|color| color != owner)
        }) else {
            next["occupations"][owner.as_str()] = Value::Null;
            continue;
        };
        let previous = rule
            .get("occupations")
            .and_then(|value| value.get(owner.as_str()));
        if previous
            .and_then(|value| value.get("pieceId"))
            .and_then(Value::as_str)
            == Some(occupant.id.as_str())
        {
            let threshold = previous
                .and_then(|value| value.get("afterOwnerTurn"))
                .map(js_comparable_number)
                .transpose()?
                .flatten();
            if threshold
                .is_some_and(|threshold| f64::from(*state.turns_taken.get(owner)) > threshold)
            {
                defeated.push(owner);
            }
        } else {
            let after = u64::from(*state.turns_taken.get(owner));
            let before = if completed && owner == actor {
                after.saturating_sub(1)
            } else {
                after
            };
            next["occupations"][owner.as_str()] = json!({
                "pieceId": occupant.id,
                "afterOwnerTurn": before + u64::from(owner == actor),
            });
        }
    }
    state.extra.insert("captureTheFlag".into(), next);
    if defeated.is_empty() {
        return Ok(V7FlowControl::Continue);
    }
    let winner = if defeated.len() == 2 {
        None
    } else {
        Some(defeated[0].opponent())
    };
    queue_flag_victory_notation(state, winner)?;
    crate::flow::end_game(
        state,
        winner,
        if winner.is_some() {
            "깃발 승리"
        } else {
            "깃발 무승부"
        },
    )?;
    Ok(V7FlowControl::Terminal)
}

fn flag_occupant<'a>(state: &'a GameState, cell: Option<&Value>) -> Result<Option<&'a Piece>> {
    let Some(cell) = cell.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let cell = cell.as_object().ok_or_else(|| {
        EngineError::InvalidState("v7 captureTheFlag.flags cell must be an object".into())
    })?;
    let (Some(row), Some(col)) = (
        cell.get("row")
            .map(|value| js_array_index(value, state.board.len()))
            .transpose()?
            .flatten(),
        cell.get("col")
            .map(|value| js_array_index(value, state.board[0].len()))
            .transpose()?
            .flatten(),
    ) else {
        return Ok(None);
    };
    Ok(state
        .board
        .get(row)
        .and_then(|line| line.get(col))
        .and_then(Option::as_ref))
}

fn js_array_index(value: &Value, upper_bound: usize) -> Result<Option<usize>> {
    match value {
        Value::Number(value) => {
            let number = value.as_f64().ok_or_else(|| {
                EngineError::InvalidState("v7 flag coordinate is not finite".into())
            })?;
            if (0.0..upper_bound as f64).contains(&number) && number.fract() == 0.0 {
                Ok(Some(number as usize))
            } else {
                Ok(None)
            }
        }
        Value::String(value) => Ok((0usize..upper_bound).find(|index| value == &index.to_string())),
        Value::Null | Value::Bool(_) => Ok(None),
        Value::Array(_) | Value::Object(_) => Err(EngineError::UnsupportedFeature(
            "v7 flag coordinate requires unported JavaScript property coercion".into(),
        )),
    }
}

fn js_comparable_number(value: &Value) -> Result<Option<f64>> {
    match value {
        Value::Null | Value::Bool(false) => Ok(Some(0.0)),
        Value::Bool(true) => Ok(Some(1.0)),
        Value::Number(number) => Ok(number.as_f64().filter(|number| number.is_finite())),
        Value::String(value) if value.trim().is_empty() => Ok(Some(0.0)),
        Value::String(value) => Ok(value.trim().parse::<f64>().ok()),
        Value::Array(_) | Value::Object(_) => Err(EngineError::UnsupportedFeature(
            "v7 flag occupation deadline requires unported JavaScript number coercion".into(),
        )),
    }
}

fn queue_flag_victory_notation(state: &mut GameState, winner: Option<Color>) -> Result<()> {
    // `createFlagVictoryNotation` constructs the event; queueHistoryNotation
    // adds one random ID only after a flag result has actually been reached.
    let suffix = crate::draft::random_suffix(
        state
            .rng
            .sample_opaque("source flag victory notation identity")?,
    )?;
    let id = format!(
        "special-{}-{}",
        crate::draft::frozen_timestamp_for_ruleset(RULES_VERSION_V7)?,
        suffix.chars().take(7).collect::<String>()
    );
    let move_number = match state.extra.get("activeHistoryMoveNumber") {
        None => u64::from(state.full_move.max(1)),
        Some(Value::Null | Value::Bool(false)) => 0,
        Some(Value::Bool(true)) => 1,
        Some(Value::Number(number)) => number
            .as_f64()
            .filter(|number| number.is_finite())
            .map(|number| number.max(0.0).floor() as u64)
            .ok_or_else(|| {
                EngineError::InvalidState("v7 activeHistoryMoveNumber must be finite".into())
            })?,
        Some(_) => {
            return Err(EngineError::UnsupportedFeature(
                "v7 flag victory with nonnumeric activeHistoryMoveNumber".into(),
            ));
        }
    };
    let (color, text, description) = match winner {
        Some(color) => (
            color,
            "깃발 승리",
            if color == Color::White {
                "백 깃발 점령 승리"
            } else {
                "흑 깃발 점령 승리"
            },
        ),
        None => (Color::White, "깃발 무승부", "양측 깃발 점령으로 무승부"),
    };
    let notation = json!({
        "id": id,
        "kind": "special",
        "color": color,
        "text": text,
        "description": description,
        "moveNumber": move_number,
    });
    let pending = state
        .extra
        .entry("pendingNotations")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| EngineError::InvalidState("v7 pendingNotations must be an array".into()))?;
    if !pending
        .iter()
        .any(|event| event.get("id") == notation.get("id"))
    {
        pending.push(notation.clone());
    }
    state.extra.insert("pendingNotation".into(), notation);
    Ok(())
}

pub(crate) fn check_internal_highlander(state: &mut GameState) -> Result<bool> {
    if state.mode == "gameover" {
        return Ok(false);
    }
    let enabled = |color: Color| {
        state
            .extra
            .get("highlander")
            .and_then(|value| value.get(color.as_str()))
            .is_some_and(js_truth)
    };
    if !enabled(Color::White) && !enabled(Color::Black) {
        return Ok(false);
    }
    let mut seen_ids = BTreeSet::new();
    let mut white = Vec::new();
    let mut black = Vec::new();
    let september18 = uses_legacy_hash_or_flag(state, SEPTEMBER18_HASHES, "september18Balance");
    for piece in state.board.iter().flatten().flatten() {
        if !piece.id.is_empty() && !seen_ids.insert(piece.id.as_str()) {
            continue;
        }
        let kind = if is_king_augment_recipient(state, piece, september18) {
            "king"
        } else {
            piece.kind.as_str()
        };
        match piece.color.owner() {
            Some(Color::White) => white.push(kind),
            Some(Color::Black) => black.push(kind),
            None => {}
        }
    }
    let eligible = |kinds: &[&str]| {
        !kinds.is_empty() && kinds.iter().copied().collect::<BTreeSet<_>>().len() == kinds.len()
    };
    let white_wins = enabled(Color::White) && eligible(&white);
    let black_wins = enabled(Color::Black) && eligible(&black);
    let september22 =
        uses_legacy_hash_or_flag(state, SEPTEMBER22_HASHES, "scarecrowPieceReservation");
    let outcome = if september22 && white_wins && black_wins {
        Some((None, "양측이 동시에 하이랜더를 달성하여 무승부입니다."))
    } else if white_wins {
        Some((
            Some(Color::White),
            "하이랜더: 아군의 기물 종류가 모두 다릅니다.",
        ))
    } else if black_wins {
        Some((
            Some(Color::Black),
            "하이랜더: 아군의 기물 종류가 모두 다릅니다.",
        ))
    } else {
        None
    };
    if let Some((winner, reason)) = outcome {
        crate::flow::end_game(state, winner, reason)?;
        return Ok(true);
    }
    Ok(false)
}

fn is_king_augment_recipient(state: &GameState, piece: &Piece, september18: bool) -> bool {
    matches!(
        piece.kind.as_str(),
        "king" | "royalKnight" | "shotgunKing" | "darkWizard"
    ) || piece.kind == "merchant" && september18
        || piece.flag("crownRoyal")
        || piece.flag("editorRoyal")
        || piece.flag("regencyHeir")
            && state.flag("kingDead", piece.color)
            && state.flag("regency", piece.color)
}

fn uses_legacy_hash_or_flag(state: &GameState, hashes: &[&str], fallback: &str) -> bool {
    let hash =
        if let Some(card_state) = state.extra.get("cardState").filter(|value| js_truth(value)) {
            card_state
                .get("profile")
                .and_then(|value| value.get("catalogHash"))
                .and_then(Value::as_str)
        } else {
            state
                .extra
                .get("profile")
                .and_then(|value| value.get("catalogHash"))
                .and_then(Value::as_str)
        };
    if let Some(hash) = hash.filter(|hash| !hash.is_empty()) {
        hashes.contains(&hash)
    } else {
        state.extra.get(fallback) != Some(&Value::Bool(false))
    }
}

fn js_truth(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Bool(true) | Value::Array(_) | Value::Object(_) => true,
    }
}

fn tick_vigilance_protection(state: &mut GameState, actor: Color) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut changes = Vec::new();
    for row in 0..state.board.len() {
        for col in 0..state.board[row].len() {
            let Some(piece) = state.board[row][col].as_ref() else {
                continue;
            };
            if !piece.id.is_empty() && !seen.insert(piece.id.clone()) {
                continue;
            }
            let Some(protection) = piece.extra.get("vigilanceProtection") else {
                continue;
            };
            if protection.get("countBy").and_then(Value::as_str) != Some(actor.as_str()) {
                continue;
            }
            // main:887-891 uses the JavaScript numeric decrement operator.
            // grantVigilanceProtection also writes finite Number values, so
            // an integral f64 is a valid counter rather than an unsupported
            // state. Keep the Number coercion shared with the other v7 rules.
            let remaining = crate::card_effects::js_number(protection.get("remaining"), 0)
                .filter(|number| number.is_finite())
                .ok_or_else(|| {
                    EngineError::UnsupportedFeature(
                        "v7 tickVigilanceProtection remaining is not a finite Number".into(),
                    )
                })?;
            let after = remaining - 1.0;
            changes.push((piece.id.clone(), row, col, (after > 0.0).then_some(after)));
        }
    }
    for (id, row, col, after) in changes {
        let prior = state.board[row][col]
            .as_ref()
            .expect("collected piece")
            .clone();
        let mut cells = Vec::new();
        if id.is_empty() {
            cells.push((row, col));
        } else {
            for (r, line) in state.board.iter().enumerate() {
                for (c, item) in line.iter().enumerate() {
                    if item.as_ref().is_some_and(|item| item.id == id) {
                        cells.push((r, c));
                    }
                }
            }
        }
        for &(r, c) in &cells {
            if state.board[r][c].as_ref() != Some(&prior) {
                return Err(EngineError::InvalidState(format!(
                    "v7 aliased vigilance piece {id} has inconsistent cells"
                )));
            }
        }
        for (r, c) in cells {
            let piece = state.board[r][c].as_mut().expect("collected piece");
            if let Some(after) = after {
                piece.extra["vigilanceProtection"]["remaining"] =
                    if after.fract() == 0.0 && after <= 9_007_199_254_740_991.0 {
                        json!(after as i64)
                    } else {
                        json!(after)
                    };
            } else {
                piece.extra.shift_remove("vigilanceProtection");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameConfig, Piece, Square};
    use sha2::{Digest, Sha256};

    fn source_state_digest(state: &GameState) -> String {
        let mut raw = serde_json::to_value(state).unwrap();
        let fields = raw.as_object_mut().unwrap();
        for outer in ["rulesetId", "rng", "history"] {
            fields.remove(outer);
        }
        format!("{:x}", Sha256::digest(serde_jcs::to_vec(&raw).unwrap()))
    }

    fn initial() -> GameState {
        crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..Default::default()
            },
            19,
        )
        .expect("pinned v7 initial state")
    }

    fn flag_state() -> GameState {
        let mut state = initial();
        state.extra.insert(
            "captureTheFlag".into(),
            json!({"flags":{"white":{"row":7,"col":0},"black":{"row":0,"col":0}},"occupations":{"white":null,"black":null}}),
        );
        let mut invader = Piece::new("pawn", Color::Black, "b-invader");
        invader.moved = true;
        state.board[7][0] = Some(invader);
        state
    }

    #[test]
    fn rectangular_board_flag_and_vigilance_callbacks_use_actual_dimensions() {
        // The frozen callbacks use `state.board[row]?.[col]` and iterate the
        // board, so cells past the standard eighth row or file remain live.
        let mut state = initial();
        for row in &mut state.board {
            row.push(None);
        }
        state.board.push(vec![None; 9]);
        state.board.push(vec![None; 9]);
        state.extra.insert(
            "captureTheFlag".into(),
            json!({"flags":{"white":{"row":9,"col":8},"black":{"row":0,"col":0}},"occupations":{"white":null,"black":null}}),
        );
        let mut invader = Piece::new("pawn", Color::Black, "rectangular-invader");
        invader.extra.insert(
            "vigilanceProtection".into(),
            json!({"countBy":"white","remaining":2}),
        );
        state.board[9][8] = Some(invader);
        state.turn = Color::Black;
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert_eq!(
            after_end_move_reactions(&mut state, Color::Black).unwrap(),
            V7FlowControl::Continue,
        );
        assert_eq!(
            state.extra["captureTheFlag"]["occupations"]["white"],
            json!({"pieceId":"rectangular-invader","afterOwnerTurn":0}),
        );
        state.turn = Color::White;
        assert_eq!(
            after_completed_turn_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue,
        );
        assert_eq!(
            state.board[9][8].as_ref().unwrap().extra["vigilanceProtection"]["remaining"],
            1,
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn campaign_objectives_match_frozen_full_state_rng_and_history() {
        // Each digest was produced by the SHA-pinned client after restoring
        // its draft-deleted seed-19 opening, installing the same campaign and
        // board edits, then calling `checkCampaignObjectives()` directly.
        let cases: [(
            &str,
            &str,
            &[(usize, usize)],
            &str,
            &str,
            Option<&str>,
            &str,
        ); 10] = [
            (
                "machineRebellion",
                "white",
                &[],
                "af176436f65aa674851ddb839e6466a0256a4fd7a33a798a8a5b1a0b063eb514",
                "1354fdea71ffeaa3ccd6320fb343333501d109863facf71c31086e8bf9512fef",
                Some("white"),
                "거신병 3개를 모두 파괴했습니다.",
            ),
            (
                "maharajaSepoy",
                "white",
                &[],
                "22ce4ecdf3f8c86c033004ce0f60b045bfa4b3ef5a426e696d3a3f67e72b7d2f",
                "8dce9102d0e735368f96af1dcd1d977fbd33f26ef4d35f7b292f66f656ec13ec",
                Some("white"),
                "아마존을 쓰러뜨렸습니다.",
            ),
            (
                "magicParty",
                "white",
                &[],
                "0136b401abcc93ef9c71c6ca90fd8516554979d768c4b024d15d9bf899a558e9",
                "7801162742b4f84217a423dd4af86f987fab58e7d05a01ba25e7666c55a1b8ab",
                Some("black"),
                "모든 마법사가 쓰러졌습니다.",
            ),
            (
                "bloodMoon",
                "white",
                &[],
                "b07e3f3e2c90371676b128526c286124690be0fa2ce065efeec7cf1e4b77027f",
                "c17828cc43c6e81a707a029d088c27eee25f19b78eb9038cb73f36b7c239a2e2",
                Some("black"),
                "뱀파이어 군주가 쓰러졌습니다.",
            ),
            (
                "timeTraveler",
                "white",
                &[],
                "47cda816dd4fa2ea9f94909be73dc70af1d8e8413f425ddef125a2a4774ab5fa",
                "075abbd875deec9cf85fcad164cde11a620c139f548cdb730b0539d553799afa",
                Some("black"),
                "시간 여행자가 쓰러졌습니다.",
            ),
            (
                "majorPiece",
                "white",
                &[(0, 4)],
                "571aed53d9258f5fe89212292e20b8c7466371da3eb78b0371336df41f3143a9",
                "f19bad3bf6a865c3da7e5b3aa79c095a39b21a6facc05cae31883c8135ba4d1d",
                Some("white"),
                "상대 킹을 잡았습니다.",
            ),
            (
                "knightJourney",
                "white",
                &[(7, 1), (7, 6)],
                "f39ba3f5d5f4c4b3eef3af55905babe84b9377129967408488a4790629e9af17",
                "f266af61a1c0160cd04b133f64669d740ae4527436e1dcb0579c4b09e138586f",
                Some("black"),
                "나이트가 사라졌습니다.",
            ),
            (
                "knightGame",
                "black",
                &[(0, 1), (0, 6)],
                "19cd2d9559ed6fc7c1df7ad9694236ba665dc9daad915f87aa42bdf739102650",
                "e17e7e49535dd95f659903255c3d77618a0280a8ab23a56532b5cca450706746",
                Some("white"),
                "나이트가 사라졌습니다.",
            ),
            (
                "conveyorFactory",
                "white",
                &[],
                "3a24ea3366d2053a1fda52bc81d68989c21bece4c57be406ffd276f7644154b8",
                "3a24ea3366d2053a1fda52bc81d68989c21bece4c57be406ffd276f7644154b8",
                None,
                "",
            ),
            (
                "unrecognized",
                "white",
                &[],
                "0ed1ff6d9d9fe9508d6f5f387692a27545d4795500130c500ac16ff59bd4fb02",
                "0ed1ff6d9d9fe9508d6f5f387692a27545d4795500130c500ac16ff59bd4fb02",
                None,
                "",
            ),
        ];
        for (setup, player_color, removed, before_digest, after_digest, winner, reason) in cases {
            let mut state = initial();
            state.extra.insert(
                "campaign".into(),
                json!({"setup":setup,"playerColor":player_color}),
            );
            for &(row, col) in removed {
                state.board[row][col] = None;
            }
            assert_eq!(source_state_digest(&state), before_digest, "{setup} before");
            let rng = state.rng.clone();
            let history = state.history.clone();
            assert_eq!(
                check_campaign_objectives(&mut state).unwrap(),
                winner.is_some(),
                "{setup} result"
            );
            assert_eq!(source_state_digest(&state), after_digest, "{setup} after");
            assert_eq!(
                state.mode,
                if winner.is_some() { "gameover" } else { "play" },
                "{setup} mode"
            );
            assert_eq!(state.winner.as_deref(), winner, "{setup} winner");
            assert_eq!(state.extra["replayEndReason"], reason, "{setup} reason");
            assert_eq!(state.rng, rng, "{setup} RNG");
            assert_eq!(state.history, history, "{setup} history");
        }
    }

    #[test]
    fn end_move_records_occupation_without_advancing_rng_or_history() {
        let mut state = flag_state();
        state.turn = Color::Black;
        assert_eq!(
            source_state_digest(&state),
            "d056eaa58deaf3ac1d2f5eed8a88bf89d7976b948045b84d9402c467abe362bf"
        );
        let mut expected = state.clone();
        expected.extra["captureTheFlag"]["occupations"]["white"] =
            json!({"pieceId":"b-invader","afterOwnerTurn":0});
        assert_eq!(
            after_end_move_reactions(&mut state, Color::Black).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(state, expected);
        assert_eq!(
            source_state_digest(&state),
            "4a6c8ca88c4443eeb79a876475efbb0650d0d51f95ca0b782e59e8549816a7b3"
        );
    }

    #[test]
    fn completed_owner_turn_reaches_flag_deadline_and_queues_notation() {
        let mut state = flag_state();
        state.extra["captureTheFlag"]["occupations"]["white"] =
            json!({"pieceId":"b-invader","afterOwnerTurn":0});
        state.turns_taken.white = 1;
        assert_eq!(
            source_state_digest(&state),
            "3dcc5413d560e162e682ab0d9d0ccdbb44b72eb7b8ed8f74079dcae442778eef"
        );
        let old_rng = state.rng.clone();
        let old_history = state.history.clone();
        assert_eq!(
            after_completed_turn_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal
        );
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner.as_deref(), Some("black"));
        assert_eq!(state.extra["replayEndReason"], "깃발 승리");
        assert_eq!(
            state.extra["pendingNotation"]["description"],
            "흑 깃발 점령 승리"
        );
        assert_eq!(state.extra["pendingNotation"]["moveNumber"], 1);
        assert_eq!(
            state.extra["pendingNotation"]["id"],
            "special-1790581292828-o97mlep"
        );
        assert_eq!(state.rng.cursor, old_rng.cursor + 1);
        assert_eq!(state.rng.state, 2_893_839_862);
        assert_eq!(state.history, old_history);
        assert_eq!(
            source_state_digest(&state),
            "1fc4938cd0d214f74cb063b552fc219e0376caf01c5e2494f8baaf5cd2e8850c"
        );
    }

    #[test]
    fn simultaneous_flag_deadlines_match_frozen_draw_state() {
        let mut state = flag_state();
        let mut white_invader = Piece::new("pawn", Color::White, "w-invader");
        white_invader.moved = true;
        state.board[0][0] = Some(white_invader);
        state.extra["captureTheFlag"]["occupations"]["white"] =
            json!({"pieceId":"b-invader","afterOwnerTurn":0});
        state.extra["captureTheFlag"]["occupations"]["black"] =
            json!({"pieceId":"w-invader","afterOwnerTurn":0});
        state.turns_taken.white = 1;
        state.turns_taken.black = 1;
        assert_eq!(
            source_state_digest(&state),
            "00d329a5dfed35dbe25c3f92ed2c6182175db25efbb4e9a7aab99f4837ea52b8"
        );
        let history = state.history.clone();
        assert_eq!(
            after_completed_turn_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal
        );
        assert_eq!(
            source_state_digest(&state),
            "ed80d01d6e6f29f02a4dd5ebce897fb99b47822f69b288023747de598c8dd233"
        );
        assert_eq!(state.winner, None);
        assert_eq!(state.extra["pendingNotation"]["color"], "white");
        assert_eq!(state.extra["pendingNotation"]["text"], "깃발 무승부");
        assert_eq!(state.rng.cursor, 33);
        assert_eq!(state.rng.state, 2_893_839_862);
        assert_eq!(state.history, history);
    }

    #[test]
    fn simultaneous_highlander_ends_in_draw_before_flag_or_vigilance() {
        let mut state = flag_state();
        state.board = state
            .board
            .into_iter()
            .enumerate()
            .map(|(row, line)| {
                line.into_iter()
                    .enumerate()
                    .map(|(col, piece)| {
                        if (row == 0 || row == 7) && col == 4 {
                            piece
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .collect();
        state
            .extra
            .insert("highlander".into(), json!({"white":true,"black":true}));
        state.board[7][4].as_mut().unwrap().extra.insert(
            "vigilanceProtection".into(),
            json!({"countBy":"white","remaining":2}),
        );
        let old_rng = state.rng.clone();
        assert_eq!(
            after_completed_turn_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal
        );
        assert_eq!(state.winner, None);
        assert_eq!(
            state.extra["replayEndReason"],
            "양측이 동시에 하이랜더를 달성하여 무승부입니다."
        );
        assert_eq!(state.rng, old_rng);
        assert_eq!(
            state.board[7][4].as_ref().unwrap().extra["vigilanceProtection"]["remaining"],
            2
        );
    }

    #[test]
    fn highlander_terminal_matches_frozen_full_state_and_rng() {
        let mut state = initial();
        state
            .extra
            .insert("highlander".into(), json!({"white":true,"black":true}));
        state.board = state
            .board
            .into_iter()
            .enumerate()
            .map(|(row, line)| {
                line.into_iter()
                    .enumerate()
                    .map(|(col, piece)| {
                        if (row == 0 || row == 7) && col == 4 {
                            piece
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .collect();
        assert_eq!(
            source_state_digest(&state),
            "40a9700e82a13974b96e60adb3fb5be7aa6bb0518e5c8af52730895ec6f7833f"
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert_eq!(
            after_completed_turn_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Terminal
        );
        assert_eq!(
            source_state_digest(&state),
            "10e904c6b34c9d6abe0f87f0dffb5f78129b2da22c2514cb08042ed9158d9b08"
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn vigilance_matches_frozen_full_state_without_rng_or_history_change() {
        let mut state = initial();
        state.board[6][0].as_mut().unwrap().extra.insert(
            "vigilanceProtection".into(),
            json!({"countBy":"white","remaining":2}),
        );
        assert_eq!(
            source_state_digest(&state),
            "a2a8ba072454043409c6a2b9c105dc335de0e599a676fe99160f9e82ce992590"
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert_eq!(
            after_completed_turn_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        assert_eq!(
            source_state_digest(&state),
            "d966e67214e3e9a4b7c20e12bc2f9514fe852a1e221cefbd6adaba519980526c"
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn vigilance_ticks_once_per_piece_identity_and_expires_on_owner_turn() {
        let mut state = initial();
        let mut large = Piece::new("colossus", Color::White, "large-1");
        large.extra.insert(
            "vigilanceProtection".into(),
            json!({"countBy":"white","remaining":2.0}),
        );
        state.board[3][3] = Some(large.clone());
        state.board[3][4] = Some(large);
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert_eq!(
            after_completed_turn_count(&mut state, Color::White).unwrap(),
            V7FlowControl::Continue
        );
        for square in [Square { row: 3, col: 3 }, Square { row: 3, col: 4 }] {
            assert_eq!(
                state.at(square).unwrap().extra["vigilanceProtection"]["remaining"],
                1
            );
        }
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
        state.turn = Color::Black;
        after_completed_turn_count(&mut state, Color::Black).unwrap();
        assert_eq!(
            state.at(Square { row: 3, col: 3 }).unwrap().extra["vigilanceProtection"]["remaining"],
            1
        );
        state.turn = Color::White;
        after_completed_turn_count(&mut state, Color::White).unwrap();
        for square in [Square { row: 3, col: 3 }, Square { row: 3, col: 4 }] {
            assert!(
                !state
                    .at(square)
                    .unwrap()
                    .extra
                    .contains_key("vigilanceProtection")
            );
        }
    }

    #[test]
    fn malformed_occupied_protection_fails_without_mutating_input() {
        let mut state = initial();
        state.board[6][0].as_mut().unwrap().extra.insert(
            "vigilanceProtection".into(),
            json!({"countBy":"white","remaining":"unexpected"}),
        );
        let before = state.clone();
        assert!(matches!(
            after_completed_turn_count(&mut state, Color::White),
            Err(EngineError::UnsupportedFeature(message)) if message.contains("tickVigilanceProtection")
        ));
        assert_eq!(state, before);
    }
}
