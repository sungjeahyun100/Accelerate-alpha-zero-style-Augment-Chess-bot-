//! Source-ordered v7 board automata at the completed-turn boundary.
//!
//! The frozen `main-OahWs0tU.js` (`e5ed84fc…`) calls these at different
//! points in `completeTurnAfterMove`. Keeping the stages separate prevents a
//! platform draw, belt movement, or collapse warning from crossing an Othello
//! or terminal boundary. Unsupported interacting effects fail before mutation;
//! the owning host transaction must roll back a later stage error.

use crate::v7_turn_flow::V7FlowControl;
use crate::{Color, EngineError, GameState, Piece, RULES_VERSION_V7, Result, Square};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

const PLATFORM_INTERVAL_TURNS: u64 = 5;
const PERIODIC_COLLAPSE_INTERVAL: u64 = 20;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct V7BoardAutomata;

impl V7BoardAutomata {
    /// `resolveConveyorAfterMove`, before `turnsTaken[black]++`.
    pub(crate) fn before_count(self, state: &mut GameState, actor: Color) -> Result<V7FlowControl> {
        actor_stage(state, actor)?;
        if actor != Color::Black {
            return Ok(V7FlowControl::Continue);
        }
        let factory = campaign_setup(state) == Some("conveyorFactory");
        if !factory && !truth(state.extra.get("conveyorRule")) {
            return Ok(V7FlowControl::Continue);
        }
        let rings = if factory {
            factory_rings()
        } else {
            vec![perimeter_ring()]
        };
        for (index, ring) in rings.iter().enumerate() {
            let belt_label = factory.then(|| format!("컨베이어 공장 {}번 벨트", index + 1));
            rotate_ring(state, actor, ring, belt_label.as_deref(), factory)?;
            // Every individual rotation settles environmental losses, bombs,
            // and campaign objectives before the next factory ring begins.
            if state.mode == "gameover" {
                return Ok(V7FlowControl::Terminal);
            }
        }
        Ok(V7FlowControl::Continue)
    }

    /// Platform tick and crown adjudication after the actor's turn count and
    /// empty-lunchbox settlement, before mistake-card/Othello settlement.
    pub(crate) fn after_empty_lunchboxes(
        self,
        state: &mut GameState,
        actor: Color,
    ) -> Result<V7FlowControl> {
        let ai_simulation = state.threat_probe_depth > 0;
        self.after_empty_lunchboxes_with_simulation(state, actor, ai_simulation)
    }

    pub(crate) fn after_empty_lunchboxes_with_simulation(
        self,
        state: &mut GameState,
        actor: Color,
        ai_simulation: bool,
    ) -> Result<V7FlowControl> {
        actor_stage(state, actor)?;
        // The source ticks the platform before crown reconciliation. Keep the
        // stage atomic so an invalid crown cannot commit the earlier RNG draw.
        let mut next = state.clone();
        tick_platform(&mut next)?;
        let terminal = resolve_crown_rule_after_move(&mut next, ai_simulation)?;
        *state = next;
        Ok(if terminal {
            V7FlowControl::Terminal
        } else {
            V7FlowControl::Continue
        })
    }

    /// Local revolving doors run after Greek Gift/Taboo and before sacrifice
    /// protection. Each door is found again by ID after earlier rotations.
    pub(crate) fn after_taboo(self, state: &mut GameState, actor: Color) -> Result<V7FlowControl> {
        actor_stage(state, actor)?;
        let mut ids = Vec::new();
        let mut seen = BTreeSet::new();
        for piece in state.board.iter().flatten().flatten() {
            if piece.color == actor
                && piece.kind == "revolvingDoor"
                && seen.insert(piece.id.clone())
            {
                ids.push(piece.id.clone());
            }
        }
        for id in ids {
            let Some(center) = find_piece(state, &id, actor, "revolvingDoor") else {
                continue;
            };
            if let Some(ring) = door_ring(center) {
                rotate_ring(state, actor, &ring, None, true)?;
            }
            if state.mode == "gameover" {
                return Ok(V7FlowControl::Terminal);
            }
        }
        Ok(V7FlowControl::Continue)
    }

    /// `resolvePeriodicCollapseAfterTurn`, after full-move cleanup/winter.
    pub(crate) fn after_full_move(
        self,
        state: &mut GameState,
        actor: Color,
    ) -> Result<V7FlowControl> {
        actor_stage(state, actor)?;
        let Some(periodic) = normalized_periodic(state)? else {
            state.extra.insert("periodicCollapse".into(), Value::Null);
            return Ok(V7FlowControl::Continue);
        };
        let current_turn = shared_turn(state);
        state.extra.insert("periodicCollapse".into(), periodic);
        let next_at = state.extra["periodicCollapse"]["nextAt"]
            .as_u64()
            .expect("normalized nextAt");
        if current_turn + 1 == next_at && actor == Color::Black {
            crate::replay::queue_visual(
                state,
                json!({"type":"collapse-warning","color":actor,"cells":edge_ring(collapse_depth(state))}),
            )?;
            return Ok(V7FlowControl::Continue);
        }
        while current_turn
            >= state.extra["periodicCollapse"]["nextAt"]
                .as_u64()
                .expect("normalized nextAt")
            && state.mode != "gameover"
        {
            let collapsed = collapse_edges(state, actor)?;
            let current_next = state.extra["periodicCollapse"]["nextAt"]
                .as_u64()
                .expect("normalized nextAt");
            let following = if collapsed {
                current_next.saturating_add(PERIODIC_COLLAPSE_INTERVAL)
            } else {
                next_interval(current_turn, PERIODIC_COLLAPSE_INTERVAL)
            };
            state.extra["periodicCollapse"]["nextAt"] = json!(following);
            if !collapsed {
                break;
            }
        }
        Ok(if state.mode == "gameover" {
            V7FlowControl::Terminal
        } else {
            V7FlowControl::Continue
        })
    }

    /// Pending collapse runs after the incoming actor switch and portal/ICBM
    /// reservations, before the incoming siren/herald windows.
    pub(crate) fn incoming_after_switch(
        self,
        state: &mut GameState,
        incoming: Color,
    ) -> Result<V7FlowControl> {
        if state.ruleset_id != RULES_VERSION_V7 || state.mode != "play" || state.turn != incoming {
            return Err(EngineError::WrongActor);
        }
        if state.extra.get("collapsePending").and_then(Value::as_str) != Some(incoming.as_str()) {
            return Ok(V7FlowControl::Continue);
        }
        collapse_edges(state, incoming)?;
        state.extra.insert("collapsePending".into(), Value::Null);
        Ok(if state.mode == "gameover" {
            V7FlowControl::Terminal
        } else {
            V7FlowControl::Continue
        })
    }
}

fn actor_stage(state: &GameState, _actor: Color) -> Result<()> {
    // source endMove/completeTurnAfterMove는 movingColor를 명시적으로 전달한다.
    // Spy 승급 등으로 기물 색이 바뀌면 outgoing actor와 state.turn이 달라도
    // 정상 종료 콜백이며, 공개 행동 actor 검사는 admission 경계가 소유한다.
    // incoming_after_switch의 새 state.turn 일치 조건은 별도로 유지한다.
    if state.ruleset_id != RULES_VERSION_V7 || state.mode != "play" {
        return Err(EngineError::WrongActor);
    }
    Ok(())
}

fn unsupported(callback: &str, field: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!("v7 {callback} requires unported {field} branch"))
}

fn truth(value: Option<&Value>) -> bool {
    crate::observation::truth(value)
}

fn campaign_setup(state: &GameState) -> Option<&str> {
    state.extra.get("campaign")?.get("setup")?.as_str()
}

fn square_name(square: Square) -> String {
    format!("{}{}", char::from(b'a' + square.col), 8 - square.row)
}

/// Frozen source `normalizeCrownRule`. Legacy ply counts are converted once;
/// the canonical persisted form always uses complete shared turns.
fn normalized_crown_entries(state: &GameState, value: &Value) -> Result<Vec<Value>> {
    if !truth(Some(value)) {
        return Ok(Vec::new());
    }
    let sources = value
        .get("crowns")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty())
        .cloned()
        .unwrap_or_else(|| vec![value.clone()]);
    let group_size = sources.len();
    let turn = shared_turn(state);
    sources.iter().enumerate().map(|(index, source)| {
        let full_turn = source.get("countUnit").and_then(Value::as_str) == Some("full-turn");
        let divisor = if full_turn { 1.0 } else { 2.0 };
        let counter = |value: Option<&Value>, field: &str| -> Result<u64> {
            let number = (crate::observation::number(value).unwrap_or(0.0) / divisor)
                .floor().max(0.0);
            if !number.is_finite() || number > (u64::MAX / 2) as f64 {
                return Err(EngineError::InvalidState(format!(
                    "v7 crown {field} cannot be represented as a safe counter: {number}",
                )));
            }
            Ok(number as u64)
        };
        let ground = if source == &Value::Bool(true) {
            Some(json!({"row":3,"col":3}))
        } else {
            source.get("ground").and_then(valid_square)
        };
        let pending = source.get("pendingTransfer");
        let attacker_id = pending.and_then(|pending| pending.get("attackerId"))
            .and_then(Value::as_str).unwrap_or("");
        let preferred = pending.and_then(|pending| pending.get("preferredCells"))
            .and_then(Value::as_array).map(|cells| cells.iter().filter_map(valid_square).collect::<Vec<_>>())
            .unwrap_or_default();
        let last_counted = if js_number(source.get("lastCountedMove")).is_some() {
            counter(source.get("lastCountedMove"),"lastCountedMove")?.min(turn)
        } else { turn };
        let holding = source.get("holdingColor").and_then(Value::as_str)
            .filter(|color| matches!(*color,"white"|"black")).unwrap_or("");
        Ok(json!({
            "id":source.get("id").and_then(Value::as_str).filter(|id| !id.is_empty())
                .map(str::to_owned).unwrap_or_else(|| format!("crown-{}",index+1)),
            "crownGroupSize":group_size,"enabled":true,
            "holderId":source.get("holderId").and_then(Value::as_str).unwrap_or(""),
            "ground":ground,"removed":truth(source.get("removed")),
            "pendingTransfer":if attacker_id.is_empty() { Value::Null } else {
                json!({"attackerId":attacker_id,"preferredCells":preferred})
            },
            "holdingColor":holding,"countUnit":"full-turn",
            "heldMoves":{
                "white":counter(source.get("heldMoves").and_then(|moves| moves.get("white")),"heldMoves.white")?,
                "black":counter(source.get("heldMoves").and_then(|moves| moves.get("black")),"heldMoves.black")?
            },
            "lastCountedMove":last_counted
        }))
    }).collect()
}

fn store_crown_entries(state: &mut GameState, entries: Vec<Value>) -> Result<()> {
    let entries = normalized_crown_entries(state, &json!({"crowns":entries}))?;
    let mut rule = entries[0].clone();
    if entries.len() > 1 {
        rule["crowns"] = json!(entries);
    }
    state.extra.insert("crownRule".into(), rule);
    Ok(())
}

fn crown_eligible(piece: &Piece) -> bool {
    piece.color.owner().is_some()
        && !matches!(
            piece.kind.as_str(),
            "wall" | "football" | "blackHole" | "monster"
        )
}

fn crown_holder(state: &GameState, rule: &Value) -> Option<Square> {
    let holder_id = rule["holderId"].as_str().unwrap_or("");
    let mut first = None;
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(piece) = state
                .at(square)
                .filter(|piece| crown_eligible(piece) && piece.kind == "crown")
            else {
                continue;
            };
            if (!holder_id.is_empty() && piece.id == holder_id)
                || piece
                    .extra
                    .get("crownTokenIds")
                    .and_then(Value::as_array)
                    .is_some_and(|ids| ids.contains(&rule["id"]))
            {
                return Some(square);
            }
            if first.is_none() && rule["crownGroupSize"].as_u64().unwrap_or(1) <= 1 {
                first = Some(square);
            }
        }
    }
    first
}

fn legacy_crown_holder(state: &GameState, rule: &Value) -> Option<Square> {
    let id = rule["holderId"].as_str().unwrap_or("");
    let mut flagged = None;
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(piece) = state
                .at(square)
                .filter(|piece| crown_eligible(piece) && piece.kind != "crown")
            else {
                continue;
            };
            if !id.is_empty() && piece.id == id {
                return Some(square);
            }
            if flagged.is_none() && truth(piece.extra.get("crownBearer")) {
                flagged = Some(square);
            }
        }
    }
    flagged
}

fn remove_piece_field(piece: &mut Piece, field: &str) {
    piece.extra.shift_remove(field);
    piece.source_order.retain(|name| name != field);
}

fn sync_crown_bearer_flags(state: &mut GameState) -> Result<()> {
    let Some(rule) = state.extra.get("crownRule") else {
        return Ok(());
    };
    let entries = normalized_crown_entries(state, rule)?;
    for piece in state.board.iter_mut().flatten().flatten() {
        let ids = entries
            .iter()
            .filter(|entry| {
                !truth(entry.get("removed"))
                    && entry["holderId"]
                        .as_str()
                        .is_some_and(|id| !id.is_empty() && id == piece.id)
            })
            .map(|entry| entry["id"].clone())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            remove_piece_field(piece, "crownBearer");
            remove_piece_field(piece, "crownTokenIds");
        } else {
            piece.extra.insert("crownBearer".into(), json!(true));
            piece.extra.insert("crownTokenIds".into(), json!(ids));
        }
    }
    Ok(())
}

fn reset_removed_crown(rule: &mut Value, current_turn: u64) {
    rule["holderId"] = json!("");
    rule["ground"] = Value::Null;
    rule["pendingTransfer"] = Value::Null;
    rule["removed"] = json!(true);
    rule["holdingColor"] = json!("");
    rule["heldMoves"] = json!({"white":0,"black":0});
    rule["lastCountedMove"] = json!(current_turn);
}

fn set_crown_holder(
    state: &mut GameState,
    origin: Square,
    rule_id: &str,
    preferred: &[Square],
    announce: bool,
    ai_simulation: bool,
) -> Result<bool> {
    let Some(original) = state
        .at(origin)
        .filter(|piece| crown_eligible(piece))
        .cloned()
    else {
        return Ok(false);
    };
    let entries =
        normalized_crown_entries(state, state.extra.get("crownRule").unwrap_or(&Value::Null))?;
    if entries.is_empty() {
        return Ok(false);
    }
    let rule_index = entries
        .iter()
        .position(|entry| entry["id"].as_str() == Some(rule_id))
        .or_else(|| {
            entries.iter().position(|entry| {
                entry
                    .get("ground")
                    .and_then(valid_square)
                    .is_some_and(|cell| {
                        let square = Square {
                            row: cell["row"].as_u64().expect("row") as u8,
                            col: cell["col"].as_u64().expect("col") as u8,
                        };
                        preferred.contains(&square)
                    })
            })
        })
        .or_else(|| {
            entries.iter().position(|entry| {
                !truth(entry.get("removed")) && entry["holderId"].as_str().is_none_or(str::is_empty)
            })
        })
        .unwrap_or(0);
    let mut entries = entries;
    let mut occupied = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if state.at(square).is_some_and(|piece| {
                if original.id.is_empty() {
                    square == origin
                } else {
                    piece.id == original.id
                }
            }) {
                occupied.push(square);
            }
        }
    }
    if occupied.is_empty() {
        return Ok(false);
    }
    let matches = preferred
        .iter()
        .copied()
        .filter(|cell| occupied.contains(cell))
        .collect::<Vec<_>>();
    let pool = if matches.is_empty() {
        occupied
    } else {
        matches
    };
    // The UI source consumes one draw even when the chosen footprint has one
    // cell. Retaining the old ID below also consumes the new piece's ID draw.
    let index = if ai_simulation {
        // `aiSimulationDepth` is an invocation context, not snapshot data.
        (u64::from(state.move_count) % pool.len() as u64) as usize
    } else {
        let index =
            ((state.rng.sample()? * pool.len() as f64).floor() as usize).min(pool.len() - 1);
        state.rng.record_last_probability(
            1.0 / pool.len() as f64,
            "source Crown holder footprint cell",
        )?;
        index
    };
    let chosen = pool[index];
    let color = original.color.owner().expect("eligible crown color");
    let rule = &mut entries[rule_index];
    if rule["holdingColor"] != color.as_str() {
        rule["holdingColor"] = json!(color);
        rule["heldMoves"] = json!({"white":0,"black":0});
        rule["lastCountedMove"] = json!(shared_turn(state));
    }
    for (row, line) in state.board.iter_mut().enumerate() {
        for (col, cell) in line.iter_mut().enumerate() {
            if cell.as_ref().is_some_and(|piece| {
                if original.id.is_empty() {
                    row == origin.row as usize && col == origin.col as usize
                } else {
                    piece.id == original.id
                }
            }) {
                *cell = None;
            }
        }
    }
    let mut piece = original.clone();
    if original.kind != "crown" {
        let suffix = crate::draft::random_suffix(
            state
                .rng
                .sample_opaque("source crown replacement piece identity")?,
        )?;
        let generated_id = format!("{}-crown-{suffix}", color.as_str());
        piece = serde_json::from_value(
            json!({"color":color,"type":"crown","moved":true,"shielded":false,
            "id":if original.id.is_empty() { generated_id } else { original.id.clone() }}),
        )
        .map_err(EngineError::serialization)?;
        piece.extra.insert("crownBearer".into(), json!(true));
        piece
            .extra
            .insert("origin".into(), json!(square_name(chosen)));
        if source_royal_king(state, &original)
            || (truth(original.extra.get("regencyHeir"))
                && state.flag("kingDead", color)
                && state.flag("regency", color))
        {
            piece.extra.insert("crownRoyal".into(), json!(true));
        }
        if truth(original.extra.get("regencyHeir"))
            && state.flag("kingDead", color)
            && state.flag("regency", color)
        {
            piece.extra.insert("regencyHeir".into(), json!(true));
            for field in [
                "undergroundBunker",
                "hp",
                "maxHp",
                "lastResistance",
                "protected",
            ] {
                if let Some(value) = original.extra.get(field) {
                    piece.extra.insert(field.into(), value.clone());
                }
            }
            if let Some(value) = original
                .extra
                .get("imperialMoves")
                .filter(|value| value.is_array())
            {
                piece.extra.insert("imperialMoves".into(), value.clone());
            }
        }
    }
    piece.extra.insert("crownBearer".into(), json!(true));
    let mut token_ids = original
        .extra
        .get("crownTokenIds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !token_ids.contains(&rule["id"]) {
        token_ids.push(rule["id"].clone());
    }
    piece.extra.insert("crownTokenIds".into(), json!(token_ids));
    if truth(state.extra.get("monochromeChess")) {
        piece.extra.insert(
            "monoShade".into(),
            json!(if (chosen.row + chosen.col).is_multiple_of(2) {
                "light"
            } else {
                "dark"
            }),
        );
    }
    state.board[chosen.row as usize][chosen.col as usize] = Some(piece.clone());
    rule["holderId"] = json!(piece.id);
    rule["ground"] = Value::Null;
    rule["pendingTransfer"] = Value::Null;
    rule["removed"] = json!(false);
    store_crown_entries(state, entries)?;
    sync_crown_bearer_flags(state)?;
    let animated = state.at(chosen).expect("installed crown").clone();
    crate::card_effects::mark_animation(state, &animated)?;
    if announce {
        crate::replay::add_log(
            state,
            format!(
                "왕관: {}의 {} 기물이 왕관으로 변했습니다.",
                square_name(chosen),
                if color == Color::White { "백" } else { "흑" }
            ),
        )?;
    }
    Ok(true)
}

/// Source `reconcileCrownRule`, used by captures and environmental callbacks
/// as well as completed turns. Reconciliation does not advance held counters.
pub(crate) fn reconcile_crown_rule(
    state: &mut GameState,
    announce: bool,
) -> Result<Option<Square>> {
    // The frozen threat probes increase kingThreatProbeDepth together with
    // aiSimulationDepth. Public host actions enter with both depths at zero.
    let ai_simulation = state.threat_probe_depth > 0;
    reconcile_crown_rule_with_simulation(state, announce, ai_simulation)
}

pub(crate) fn reconcile_crown_rule_with_simulation(
    state: &mut GameState,
    announce: bool,
    ai_simulation: bool,
) -> Result<Option<Square>> {
    let mut next = state.clone();
    let holder = reconcile_crown_rule_inner(&mut next, announce, ai_simulation)?;
    *state = next;
    Ok(holder)
}

/// Source `transferCrownAfterCapture`. Transfer is queued until the source
/// landing/reconciliation stage; an unavailable attacker removes the token.
pub(crate) fn transfer_crown_after_capture(
    state: &mut GameState,
    captured: &mut Piece,
    attacker: Option<&Piece>,
    attacker_landing_cells: &[Square],
) -> Result<bool> {
    let Some(rule) = state
        .extra
        .get("crownRule")
        .filter(|value| truth(Some(value)))
    else {
        return Ok(false);
    };
    let mut entries = normalized_crown_entries(state, rule)?;
    let token_ids = captured
        .extra
        .get("crownTokenIds")
        .and_then(Value::as_array);
    let mut selected = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            ((!captured.id.is_empty() && entry["holderId"].as_str() == Some(captured.id.as_str()))
                || token_ids.is_some_and(|ids| ids.contains(&entry["id"])))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    if selected.is_empty()
        && !(crown_eligible(captured) && captured.kind == "crown")
        && !truth(captured.extra.get("crownBearer"))
    {
        return Ok(false);
    }
    if selected.is_empty() {
        selected.push(0);
    }
    let current = shared_turn(state);
    for index in selected {
        let entry = &mut entries[index];
        entry["holderId"] = json!("");
        entry["ground"] = Value::Null;
        entry["pendingTransfer"] = Value::Null;
        entry["removed"] = json!(false);
        if let Some(attacker) =
            attacker.filter(|piece| crown_eligible(piece) && !piece.id.is_empty())
        {
            entry["pendingTransfer"] =
                json!({"attackerId":attacker.id,"preferredCells":attacker_landing_cells});
        } else {
            reset_removed_crown(entry, current);
        }
    }
    store_crown_entries(state, entries)?;
    sync_crown_bearer_flags(state)?;
    remove_piece_field(captured, "crownBearer");
    remove_piece_field(captured, "crownTokenIds");
    Ok(true)
}

/// Source bear/parry restoration assigns each prior token to the restored
/// object before further retaliation logging and movement callbacks.
pub(crate) fn restore_crown_holder_after_retaliation(
    state: &mut GameState,
    restored: &mut Piece,
    square: Square,
) -> Result<()> {
    let ai_simulation = state.threat_probe_depth > 0;
    restore_crown_holder_after_retaliation_with_simulation(state, restored, square, ai_simulation)
}

pub(crate) fn restore_crown_holder_after_retaliation_with_simulation(
    state: &mut GameState,
    restored: &mut Piece,
    square: Square,
    ai_simulation: bool,
) -> Result<()> {
    if !truth(restored.extra.get("crownBearer")) {
        return Ok(());
    }
    let mut next = state.clone();
    let token_ids = restored
        .extra
        .get("crownTokenIds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if token_ids.is_empty() {
        set_crown_holder(&mut next, square, "", &[], true, ai_simulation)?;
    } else {
        for token in token_ids {
            let origin = if restored.id.is_empty() {
                Some(square)
            } else {
                piece_square_by_id(&next, &restored.id)
            };
            if let Some(origin) = origin {
                set_crown_holder(
                    &mut next,
                    origin,
                    token.as_str().unwrap_or(""),
                    &[],
                    true,
                    ai_simulation,
                )?;
            }
        }
    }
    let at = if restored.id.is_empty() {
        Some(square)
    } else {
        piece_square_by_id(&next, &restored.id)
    };
    if let Some(piece) = at.and_then(|square| next.at(square)).cloned() {
        *restored = piece;
    }
    *state = next;
    Ok(())
}

fn reconcile_crown_rule_inner(
    state: &mut GameState,
    announce: bool,
    ai_simulation: bool,
) -> Result<Option<Square>> {
    let Some(value) = state
        .extra
        .get("crownRule")
        .cloned()
        .filter(|value| truth(Some(value)))
    else {
        return Ok(None);
    };
    let entries = normalized_crown_entries(state, &value)?;
    store_crown_entries(state, entries.clone())?;
    let rule_ids = entries
        .iter()
        .map(|entry| {
            entry["id"]
                .as_str()
                .expect("normalized crown ID")
                .to_owned()
        })
        .collect::<Vec<_>>();
    let mut first_holder = None;
    for rule_id in &rule_ids {
        let mut entries = normalized_crown_entries(state, &state.extra["crownRule"])?;
        let Some(index) = entries
            .iter()
            .position(|entry| entry["id"].as_str() == Some(rule_id.as_str()))
        else {
            continue;
        };
        if truth(entries[index].get("removed")) {
            reset_removed_crown(&mut entries[index], shared_turn(state));
            store_crown_entries(state, entries)?;
            continue;
        }
        if let Some(holder) = crown_holder(state, &entries[index]) {
            let piece = state.at(holder).expect("crown holder");
            let color = piece.color.owner().expect("eligible crown holder");
            entries[index]["holderId"] = json!(piece.id);
            entries[index]["ground"] = Value::Null;
            entries[index]["pendingTransfer"] = Value::Null;
            if entries[index]["holdingColor"] != color.as_str() {
                entries[index]["holdingColor"] = json!(color);
                entries[index]["heldMoves"] = json!({"white":0,"black":0});
                entries[index]["lastCountedMove"] = json!(shared_turn(state));
            }
            store_crown_entries(state, entries)?;
            first_holder = first_holder.or(Some(holder));
            continue;
        }
        let pending = entries[index]["pendingTransfer"].clone();
        if !pending.is_null() {
            let attacker = pending["attackerId"]
                .as_str()
                .and_then(|id| piece_square_by_id(state, id));
            let preferred = pending["preferredCells"]
                .as_array()
                .expect("normalized preferred cells")
                .iter()
                .map(|cell| Square {
                    row: cell["row"].as_u64().expect("row") as u8,
                    col: cell["col"].as_u64().expect("col") as u8,
                })
                .collect::<Vec<_>>();
            if let Some(attacker) = attacker
                && set_crown_holder(
                    state,
                    attacker,
                    rule_id,
                    &preferred,
                    announce,
                    ai_simulation,
                )?
            {
                let updated = normalized_crown_entries(state, &state.extra["crownRule"])?;
                first_holder = first_holder.or_else(|| {
                    updated
                        .iter()
                        .find(|entry| entry["id"].as_str() == Some(rule_id.as_str()))
                        .and_then(|entry| crown_holder(state, entry))
                });
            } else {
                reset_removed_crown(&mut entries[index], shared_turn(state));
                store_crown_entries(state, entries)?;
            }
            continue;
        }
        if rule_ids.len() == 1
            && let Some(legacy) = legacy_crown_holder(state, &entries[index])
            && set_crown_holder(state, legacy, rule_id, &[legacy], announce, ai_simulation)?
        {
            let updated = normalized_crown_entries(state, &state.extra["crownRule"])?;
            first_holder = first_holder.or_else(|| crown_holder(state, &updated[0]));
            continue;
        }
        if let Some(ground) = entries[index].get("ground").and_then(valid_square) {
            let at = Square {
                row: ground["row"].as_u64().expect("row") as u8,
                col: ground["col"].as_u64().expect("col") as u8,
            };
            if set_crown_holder(state, at, rule_id, &[at], announce, ai_simulation)? {
                let updated = normalized_crown_entries(state, &state.extra["crownRule"])?;
                first_holder = first_holder.or_else(|| {
                    updated
                        .iter()
                        .find(|entry| entry["id"].as_str() == Some(rule_id.as_str()))
                        .and_then(|entry| crown_holder(state, entry))
                });
            }
            continue;
        }
        reset_removed_crown(&mut entries[index], shared_turn(state));
        store_crown_entries(state, entries)?;
    }
    sync_crown_bearer_flags(state)?;
    Ok(first_holder)
}

fn resolve_crown_rule_after_move(state: &mut GameState, ai_simulation: bool) -> Result<bool> {
    reconcile_crown_rule_with_simulation(state, true, ai_simulation)?;
    let Some(value) = state
        .extra
        .get("crownRule")
        .filter(|value| truth(Some(value)))
    else {
        return Ok(false);
    };
    let mut entries = normalized_crown_entries(state, value)?;
    let turn = shared_turn(state);
    for entry in &mut entries {
        let Some(holder) = crown_holder(state, entry) else {
            continue;
        };
        let color = state
            .at(holder)
            .expect("crown holder")
            .color
            .owner()
            .expect("eligible holder");
        if entry["holdingColor"] != color.as_str() {
            entry["holdingColor"] = json!(color);
            entry["heldMoves"] = json!({"white":0,"black":0});
            entry["lastCountedMove"] = json!(turn);
        }
        let prior = entry["lastCountedMove"]
            .as_u64()
            .expect("normalized counted move");
        let elapsed = turn.saturating_sub(prior);
        entry["lastCountedMove"] = json!(turn);
        if elapsed > 0 {
            entry["heldMoves"][color.opponent().as_str()] = json!(0);
            let held = entry["heldMoves"][color.as_str()]
                .as_u64()
                .expect("normalized held moves");
            entry["heldMoves"][color.as_str()] = json!(held.saturating_add(elapsed).min(10));
        }
    }
    let winner = [Color::White, Color::Black].into_iter().find(|color| {
        entries.iter().any(|entry| {
            entry["holdingColor"] == color.as_str()
                && entry["heldMoves"][color.as_str()]
                    .as_u64()
                    .is_some_and(|held| held >= 10)
        })
    });
    store_crown_entries(state, entries)?;
    if let Some(color) = winner {
        let label = if color == Color::White { "백" } else { "흑" };
        crate::replay::queue_special_effect_notation(
            state,
            color,
            "왕관",
            &format!("{label} 왕관 10수 보유"),
        )?;
        crate::flow::end_game(
            state,
            Some(color),
            &format!("{label}이 왕관을 10수 동안 지켰습니다."),
        )?;
        return Ok(true);
    }
    Ok(false)
}

fn perimeter_ring() -> Vec<Square> {
    let mut ring = Vec::with_capacity(28);
    for col in 0..8 {
        ring.push(Square { row: 0, col });
    }
    for row in 1..8 {
        ring.push(Square { row, col: 7 });
    }
    for col in (0..7).rev() {
        ring.push(Square { row: 7, col });
    }
    for row in (1..7).rev() {
        ring.push(Square { row, col: 0 });
    }
    ring
}

fn factory_rings() -> Vec<Vec<Square>> {
    (0..4)
        .map(|inset| {
            let min = inset as u8;
            let max = 7 - min;
            let mut ring = Vec::new();
            for col in min..=max {
                ring.push(Square { row: min, col });
            }
            for row in min + 1..=max {
                ring.push(Square { row, col: max });
            }
            for col in (min..max).rev() {
                ring.push(Square { row: max, col });
            }
            for row in (min + 1..max).rev() {
                ring.push(Square { row, col: min });
            }
            if inset % 2 == 1 {
                ring.reverse();
            }
            ring
        })
        .collect()
}

fn door_ring(center: Square) -> Option<Vec<Square>> {
    let offsets = [
        (-1, -1),
        (-1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
        (1, 0),
        (1, -1),
        (0, -1),
    ];
    offsets
        .iter()
        .map(|&(dr, dc)| center.offset(dr, dc))
        .collect()
}

fn find_piece(state: &GameState, id: &str, owner: Color, kind: &str) -> Option<Square> {
    if id.is_empty() {
        return None;
    }
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if state
                .at(square)
                .is_some_and(|piece| piece.id == id && piece.color == owner && piece.kind == kind)
            {
                return Some(square);
            }
        }
    }
    None
}

fn piece_square_by_id(state: &GameState, id: &str) -> Option<Square> {
    if id.is_empty() {
        return None;
    }
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if state.at(square).is_some_and(|piece| piece.id == id) {
                return Some(square);
            }
        }
    }
    None
}

fn conveyor_destination(square: Square) -> Option<Square> {
    let Square { row, col } = square;
    if row == 0 && col < 7 {
        Some(Square { row, col: col + 1 })
    } else if col == 7 && row < 7 {
        Some(Square { row: row + 1, col })
    } else if row == 7 && col > 0 {
        Some(Square { row, col: col - 1 })
    } else if col == 0 && row > 0 {
        Some(Square { row: row - 1, col })
    } else {
        None
    }
}

fn advance_reservations(
    state: &GameState,
    name: &str,
    ring: &[Square],
    ring_override: bool,
) -> Result<Value> {
    let Some(entries) = state.extra.get(name) else {
        return Ok(json!([]));
    };
    let Some(entries) = entries.as_array() else {
        if entries.is_null() {
            return Ok(json!([]));
        }
        return Err(EngineError::InvalidState(format!(
            "v7 {name} must be an array"
        )));
    };
    let mut advanced = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(fields) = entry.as_object() else {
            if entry.is_null() && !ring_override {
                advanced.push(Value::Null);
                continue;
            }
            return Err(EngineError::InvalidState(format!(
                "v7 {name} entry must be an object"
            )));
        };
        let mut advanced_entry = entry.clone();
        let square = fields
            .get("row")
            .and_then(Value::as_u64)
            .zip(fields.get("col").and_then(Value::as_u64))
            .and_then(|(row, col)| {
                (row < 8 && col < 8).then_some(Square {
                    row: row as u8,
                    col: col as u8,
                })
            });
        let destination = square.and_then(|square| {
            if ring_override {
                ring.iter()
                    .position(|&cell| cell == square)
                    .map(|index| ring[(index + 1) % ring.len()])
            } else {
                conveyor_destination(square)
            }
        });
        if let Some(destination) = destination {
            advanced_entry["row"] = json!(destination.row);
            advanced_entry["col"] = json!(destination.col);
        }
        advanced.push(advanced_entry);
    }
    Ok(Value::Array(advanced))
}

fn rotate_ring(
    state: &mut GameState,
    actor: Color,
    ring: &[Square],
    belt_label: Option<&str>,
    ring_override: bool,
) -> Result<usize> {
    // Direct callback users receive the host transaction's all-or-nothing
    // behavior when a later promotion or hazard callback rejects input.
    let mut next = state.clone();
    let moved = rotate_ring_inner(&mut next, actor, ring, belt_label, ring_override)?;
    *state = next;
    Ok(moved)
}

fn rotate_ring_inner(
    state: &mut GameState,
    actor: Color,
    ring: &[Square],
    belt_label: Option<&str>,
    ring_override: bool,
) -> Result<usize> {
    if ring.is_empty() {
        return Ok(0);
    }
    let scarecrows = advance_reservations(state, "pendingScarecrows", ring, ring_override)?;
    let lobsters = advance_reservations(state, "pendingLobsters", ring, ring_override)?;
    let en_passant_pawn = state.en_passant.as_ref().and_then(|right| {
        state
            .at(Square {
                row: right.captured_row,
                col: right.captured_col,
            })
            .cloned()
    });
    if state
        .en_passant
        .as_ref()
        .zip(en_passant_pawn.as_ref())
        .is_some_and(|(right, pawn)| {
            pawn.kind == "pawn" && pawn.color == right.color && pawn.id.is_empty()
        })
    {
        return Err(unsupported(
            "conveyorEnPassantAfterMove",
            "pawn without stable identity",
        ));
    }
    let occupants: Vec<Option<Piece>> = ring
        .iter()
        .map(|&square| state.at(square).cloned())
        .collect();
    let fixed: Vec<bool> = occupants
        .iter()
        .enumerate()
        .map(|(index, piece)| {
            piece.as_ref().is_some_and(|piece| {
                piece.is_large()
                    || !crate::movement::expansion_destination_allowed(
                        state,
                        piece.color,
                        &[ring[(index + 1) % ring.len()]],
                    )
            })
        })
        .collect();
    let mut can_move = vec![false; ring.len()];
    if !fixed.iter().any(|fixed| *fixed) {
        for (index, piece) in occupants.iter().enumerate() {
            can_move[index] = piece.is_some();
        }
    } else {
        for (index, piece) in occupants.iter().enumerate() {
            let next = (index + 1) % ring.len();
            if piece.is_some() && !fixed[index] && !fixed[next] && occupants[next].is_none() {
                can_move[index] = true;
            }
        }
        loop {
            let mut changed = false;
            for (index, piece) in occupants.iter().enumerate() {
                let next = (index + 1) % ring.len();
                if piece.is_some()
                    && !fixed[index]
                    && !can_move[index]
                    && !fixed[next]
                    && occupants[next].is_some()
                    && can_move[next]
                {
                    can_move[index] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
    // The source clears every moving origin before writing any destination.
    for (index, &square) in ring.iter().enumerate() {
        if can_move[index] {
            state.board[square.row as usize][square.col as usize] = None;
        }
    }
    let mut transitions = Vec::new();
    let mut removed = Vec::new();
    for (index, &source) in ring.iter().enumerate() {
        if !can_move[index] {
            continue;
        }
        let destination = ring[(index + 1) % ring.len()];
        let mut piece = occupants[index].as_ref().expect("moving occupant").clone();
        piece.moved = true;
        crate::card_effects::note_ultimatum_movement(state, &mut piece)?;
        let lost = crate::movement::collapsed(state, destination);
        if lost {
            let capture_owner = piece.color.owner().map_or(actor, Color::opponent);
            if piece.color.owner().is_some() {
                state.captures.get_mut(capture_owner).push(piece.clone());
            }
            removed.push(crate::v7_board_hazards::EnvironmentalRemoval {
                piece: piece.clone(),
                square: destination,
                capture_owner,
            });
        } else {
            state.board[destination.row as usize][destination.col as usize] = Some(piece.clone());
            if matches!(piece.kind.as_str(), "pawn" | "squire" | "standardBearer")
                && crate::v7_promotion::should_promote_v7(state, &piece, destination)?
            {
                crate::v7_promotion::auto_promote_forced_pawn_v7(state, destination)?;
                piece = state.at(destination).cloned().ok_or_else(|| {
                    EngineError::InvalidState("v7 forced promotion lost its landing piece".into())
                })?;
            }
            crate::card_effects::mark_animation(state, &piece)?;
        }
        transitions.push(json!({"id":piece.id,"color":piece.color,"type":piece.kind,"from":source,"to":destination,"removed":lost}));
    }
    state.extra.insert("pendingScarecrows".into(), scarecrows);
    state.extra.insert("pendingLobsters".into(), lobsters);
    if let Some(mut right) = state.en_passant.clone() {
        let moved_pawn_square = en_passant_pawn.as_ref().and_then(|pawn| {
            (pawn.kind == "pawn" && pawn.color == right.color && !pawn.id.is_empty())
                .then(|| piece_square_by_id(state, &pawn.id))
                .flatten()
        });
        state.en_passant = moved_pawn_square.and_then(|pawn_square| {
            let capture_square = pawn_square.offset(right.color.opponent().pawn_dir(), 0)?;
            if state.at(capture_square).is_some() {
                return None;
            }
            right.row = capture_square.row;
            right.col = capture_square.col;
            right.captured_row = pawn_square.row;
            right.captured_col = pawn_square.col;
            Some(right)
        });
    }
    if transitions.is_empty() {
        return Ok(0);
    }
    let broken_bonds = partition_chain_bonds_by_range(state)?;
    let moved = transitions.len();
    crate::replay::queue_visual(
        state,
        // `queueReplayVisual` normalizes unlisted source type
        // `conveyor-move` to the persisted `effect` type.
        json!({"type":"effect","color":actor,"transitions":transitions}),
    )?;
    let noun = belt_label.unwrap_or(if ring.len() == 8 {
        "회전문"
    } else {
        "컨베이어"
    });
    let direction = if belt_label.is_some() {
        "벨트 방향으로"
    } else {
        "시계방향으로"
    };
    crate::replay::add_log(
        state,
        format!("{noun}: 기물 {moved}개가 {direction} 이동했습니다."),
    )?;
    if broken_bonds > 0 {
        crate::replay::add_log(
            state,
            format!("사슬: 컨베이어 이동으로 {broken_bonds}개의 사슬이 끊어졌습니다."),
        )?;
    }
    if !removed.is_empty() {
        crate::v7_rule_bombs::mark_deathmatch_progress(state)?;
        crate::v7_board_hazards::resolve_environmental_defeats(state, &removed, "컨베이어", false)?;
    }
    if state.mode != "gameover" {
        crate::v7_rule_bombs::resolve_under_pieces(state, actor, true)?;
        crate::v7_capture_objectives::check_campaign_objectives(state)?;
    }
    Ok(moved)
}

/// Source `partitionChainBondsByRange`: normalize the bounded bond list, then
/// retain pairs whose first board positions are within Chebyshev distance two.
/// Conveyor rotation records its own message later, so this stage emits no log.
fn partition_chain_bonds_by_range(state: &mut GameState) -> Result<usize> {
    let bonds = crate::card_effects::normalize_chain_bonds(state.extra.get("chainBonds"))?;
    let mut active = Vec::with_capacity(bonds.len());
    let mut broken = 0;
    for bond in bonds {
        let first = bond["aId"]
            .as_str()
            .and_then(|id| piece_square_by_id(state, id));
        let second = bond["bId"]
            .as_str()
            .and_then(|id| piece_square_by_id(state, id));
        if first.zip(second).is_some_and(|(first, second)| {
            first
                .row
                .abs_diff(second.row)
                .max(first.col.abs_diff(second.col))
                <= 2
        }) {
            active.push(bond);
        } else {
            broken += 1;
        }
    }
    state.extra.insert("chainBonds".into(), json!(active));
    Ok(broken)
}

/// Source `breakOutOfRangeChainBonds`, shared by turn availability and delayed
/// bear/parry restoration. Publish the canonical bonds and log together.
pub(crate) fn break_out_of_range_chain_bonds(state: &mut GameState) -> Result<usize> {
    let mut next = state.clone();
    let broken = partition_chain_bonds_by_range(&mut next)?;
    if broken > 0 {
        crate::replay::add_log(
            &mut next,
            format!(
                "사슬: 연결된 기물이 사라지거나 사이가 3칸 이상 벌어져 {broken}개의 사슬이 끊어졌습니다."
            ),
        )?;
    }
    *state = next;
    Ok(broken)
}

fn js_number(value: Option<&Value>) -> Option<f64> {
    crate::observation::number(value).filter(|number| number.is_finite())
}

fn nonzero_floor(value: Option<&Value>, fallback: u64, min: u64) -> u64 {
    let number = js_number(value)
        .filter(|number| *number != 0.0)
        .unwrap_or(fallback as f64);
    number.floor().max(min as f64).min((u64::MAX / 2) as f64) as u64
}

fn strict_integer(value: Option<&Value>) -> Option<i64> {
    let number = value?.as_f64()?;
    (number.is_finite()
        && number.fract() == 0.0
        && number >= i64::MIN as f64
        && number <= i64::MAX as f64)
        .then_some(number as i64)
}

fn valid_square(value: &Value) -> Option<Value> {
    let row = strict_integer(value.get("row"))?;
    let col = strict_integer(value.get("col"))?;
    ((0..8).contains(&row) && (0..8).contains(&col)).then(|| json!({"row":row,"col":col}))
}

fn normalized_platform(state: &GameState, input: &Value) -> Result<Value> {
    let value = input
        .as_object()
        .ok_or_else(|| EngineError::InvalidState("platformRule must be an object".into()))?;
    let ply = value.get("countUnit").and_then(Value::as_str) == Some("ply");
    let cadence = value.get("cadence").and_then(Value::as_str) == Some("full-turn");
    let current = platform_turn(state, ply);
    let mut out = Map::new();
    out.insert("enabled".into(), json!(true));
    if ply {
        out.insert("countUnit".into(), json!("ply"));
    }
    if cadence {
        out.insert("cadence".into(), json!("full-turn"));
    }
    if let Some(preview_at) = strict_integer(value.get("previewAt")) {
        out.insert("previewAt".into(), json!(preview_at));
        out.insert(
            "previewCell".into(),
            value
                .get("previewCell")
                .and_then(valid_square)
                .unwrap_or(Value::Null),
        );
    }
    let fallback = next_interval(current, PLATFORM_INTERVAL_TURNS);
    out.insert(
        "nextAt".into(),
        json!(nonzero_floor(value.get("nextAt"), fallback, 1)),
    );
    let mut cells = Vec::new();
    let mut seen = BTreeSet::new();
    for candidate in std::iter::once(value.get("cell")).chain(
        value
            .get("cells")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(Some),
    ) {
        if let Some(square) = candidate.and_then(valid_square) {
            let row = square["row"].as_u64().expect("valid row");
            let col = square["col"].as_u64().expect("valid col");
            if seen.insert((row, col)) {
                cells.push(square);
            }
        }
    }
    out.insert("cell".into(), cells.first().cloned().unwrap_or(Value::Null));
    out.insert("cells".into(), Value::Array(cells));
    out.insert("fixed".into(), json!(truth(value.get("fixed"))));
    out.insert(
        "spawnedAt".into(),
        json!(nonzero_floor(value.get("spawnedAt"), 0, 0)),
    );
    out.insert(
        "nonce".into(),
        json!(nonzero_floor(value.get("nonce"), 0, 0)),
    );
    let ids = value
        .get("triggeredIds")
        .and_then(Value::as_array)
        .map(|items| {
            let mut seen = BTreeSet::new();
            let strings = items.iter().filter_map(Value::as_str).collect::<Vec<_>>();
            strings[strings.len().saturating_sub(128)..]
                .iter()
                .copied()
                .filter(|id| seen.insert((*id).to_owned()))
                .map(|id| json!(id))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    out.insert("triggeredIds".into(), Value::Array(ids));
    Ok(Value::Object(out))
}

fn platform_turn(state: &GameState, ply: bool) -> u64 {
    let white = u64::from(state.turns_taken.white);
    let black = u64::from(state.turns_taken.black);
    if ply { white + black } else { white.min(black) }
}

fn shared_turn(state: &GameState) -> u64 {
    u64::from(state.turns_taken.white.min(state.turns_taken.black))
}

fn next_interval(current: u64, step: u64) -> u64 {
    (current / step + 1) * step
}

fn platform_candidates(state: &GameState) -> Result<Vec<Square>> {
    let mut cells = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if state.at(square).is_none()
                && !crate::movement::quantum_occupied(state, square)?
                && !crate::movement::collapsed(state, square)
                && !crown_ground(state, square)
                && !black_hole_cell(state, square)
                && !installation_reserved(state, square)
            {
                cells.push(square);
            }
        }
    }
    Ok(cells)
}

fn strict_square_match(value: &Value, square: Square) -> bool {
    strict_integer(value.get("row")) == Some(i64::from(square.row))
        && strict_integer(value.get("col")) == Some(i64::from(square.col))
}

fn crown_ground(state: &GameState, square: Square) -> bool {
    let Some(rule) = state.extra.get("crownRule") else {
        return false;
    };
    let entries = rule
        .get("crowns")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty());
    let entries = entries
        .map(Vec::as_slice)
        .unwrap_or_else(|| std::slice::from_ref(rule));
    entries.iter().any(|entry| {
        truth(Some(entry))
            && !truth(entry.get("removed"))
            && (entry == &Value::Bool(true) && square == (Square { row: 3, col: 3 })
                || entry
                    .get("ground")
                    .is_some_and(|ground| strict_square_match(ground, square)))
    })
}

fn black_hole_cell(state: &GameState, square: Square) -> bool {
    state
        .extra
        .get("blackHole")
        .and_then(Value::as_array)
        .is_some_and(|cells| {
            cells.iter().any(|cell| {
                js_number(cell.get("row")) == Some(f64::from(square.row))
                    && js_number(cell.get("col")) == Some(f64::from(square.col))
            })
        })
}

fn installation_reserved(state: &GameState, square: Square) -> bool {
    let scarecrow = state
        .extra
        .get("pendingScarecrows")
        .and_then(Value::as_array)
        .is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| !truth(entry.get("pieceId")) && strict_square_match(entry, square))
        });
    let lobster = state
        .extra
        .get("pendingLobsters")
        .and_then(Value::as_array)
        .is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| strict_square_match(entry, square))
        });
    let portal = state
        .extra
        .get("pendingPortals")
        .and_then(Value::as_array)
        .is_some_and(|entries| {
            entries.iter().any(|entry| {
                entry
                    .get("cells")
                    .and_then(Value::as_array)
                    .is_some_and(|cells| cells.iter().any(|cell| strict_square_match(cell, square)))
            })
        });
    scarecrow || lobster || portal
}

fn random_platform_cell(state: &mut GameState, candidates: &[Square]) -> Result<Value> {
    // The source evaluates Math.random even for an empty array.
    let sample = if candidates.is_empty() {
        state
            .rng
            .sample_invariant("source empty Platform candidate")?
    } else {
        let sample = state.rng.sample()?;
        state.rng.record_last_probability(
            1.0 / candidates.len() as f64,
            "source Platform preview or spawn candidate",
        )?;
        sample
    };
    let index = (sample * candidates.len() as f64).floor();
    if index < 0.0 {
        return Ok(Value::Null);
    }
    Ok(candidates
        .get(index as usize)
        .map(|square| json!(square))
        .unwrap_or(Value::Null))
}

fn tick_platform(state: &mut GameState) -> Result<()> {
    let Some(value) = state.extra.get("platformRule") else {
        return Ok(());
    };
    if !truth(value.get("enabled")) {
        return Ok(());
    }
    let mut rule = normalized_platform(state, value)?;
    let current = platform_turn(state, rule["countUnit"] == "ply");
    let next_at = rule["nextAt"].as_u64().expect("normalized nextAt");
    let fixed = rule["fixed"].as_bool().expect("normalized fixed");
    let card_state = state
        .extra
        .get("cardState")
        .filter(|value| truth(Some(value)));
    let profile = match card_state {
        Some(card_state) => card_state.get("profile"),
        None => state.extra.get("profile"),
    };
    let hash = profile
        .and_then(|profile| profile.get("catalogHash"))
        .and_then(Value::as_str)
        .filter(|hash| !hash.is_empty());
    let use_forecast = hash.map_or_else(
        || {
            !truth(state.extra.get("campaign"))
                || state.extra.get("september27CopyPools") == Some(&json!(true))
        },
        |hash| hash == "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
    );
    if use_forecast
        && !fixed
        && u64::from(state.turns_taken.white) == u64::from(state.turns_taken.black) + 1
        && current + 1 == next_at
        && rule["previewAt"].as_u64() != Some(next_at)
    {
        let candidates = platform_candidates(state)?;
        let cell = random_platform_cell(state, &candidates)?;
        rule["previewAt"] = json!(next_at);
        rule["previewCell"] = cell;
    }
    if !fixed && current >= next_at {
        let candidates = platform_candidates(state)?;
        let cell = if rule["previewAt"].as_u64() == Some(current) {
            rule["previewCell"].clone()
        } else {
            random_platform_cell(state, &candidates)?
        };
        let had_cell = !cell.is_null();
        rule.as_object_mut()
            .expect("normalized object")
            .remove("previewAt");
        rule.as_object_mut()
            .expect("normalized object")
            .remove("previewCell");
        rule["cell"] = cell.clone();
        rule["cells"] = if had_cell {
            json!([cell.clone()])
        } else {
            json!([])
        };
        rule["fixed"] = json!(false);
        rule["spawnedAt"] = json!(current);
        rule["nextAt"] = json!(
            current
                + if rule["countUnit"] == "ply" && rule["cadence"] == "full-turn" {
                    2 * PLATFORM_INTERVAL_TURNS
                } else {
                    PLATFORM_INTERVAL_TURNS
                }
        );
        rule["triggeredIds"] = json!([]);
        rule["nonce"] = json!(rule["nonce"].as_u64().unwrap_or(0) + 1);
        let message = if had_cell {
            let row = cell["row"].as_u64().expect("candidate row");
            let col = cell["col"].as_u64().expect("candidate col");
            format!(
                "발판: {}{}에 새 발판이 생성되었습니다.",
                char::from(b'a' + col as u8),
                8 - row
            )
        } else {
            "발판: 설치할 빈칸이 없어 이번 발판이 생성되지 않았습니다.".into()
        };
        crate::replay::add_log(state, message)?;
    }
    state.extra.insert("platformRule".into(), rule);
    Ok(())
}

#[derive(Clone)]
struct CollapseVictim {
    item: Piece,
    square: Square,
    capture_owner: Color,
}

/// Source `collapseEdges`: the ring is captured before depth advances, and
/// everything after that point is one host-owned transaction. The caller
/// rolls back errors from the replay or terminal stage.
fn collapse_edges(state: &mut GameState, owner: Color) -> Result<bool> {
    // A collapse can reach replay, inheritance, and terminal callbacks. Keep
    // the whole source callback atomic even when this helper is called outside
    // a host transaction (for example by the periodic-collapse catch-up).
    let mut next = state.clone();
    let collapsed = collapse_edges_inner(&mut next, owner)?;
    *state = next;
    Ok(collapsed)
}

fn collapse_edges_inner(state: &mut GameState, owner: Color) -> Result<bool> {
    let previous_depth = collapse_depth(state);
    let cells = edge_ring(previous_depth);
    if cells.is_empty() {
        return Ok(false);
    }
    let mut removed = collapse_victims(state, owner, &cells)?;
    let surviving_bombs = normalized_rule_bombs(state, &cells)?;
    state
        .extra
        .insert("collapseDepth".into(), json!(previous_depth + 1));
    state.extra.insert("collapsed".into(), json!(true));
    for victim in &mut removed {
        // Source victims retain object identity. A preceding Vigilance loss
        // can protect a later royal victim before it enters the captures list.
        victim.item = state
            .at(victim.square)
            .filter(|piece| piece.id == victim.item.id)
            .cloned()
            .ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 collapse victim identity disappeared before removal".into(),
                )
            })?;
        remove_collapse_piece(state, victim);
        crate::transition::grant_vigilance_protection(state, &victim.item)?;
        cancel_collapse_prophecies(state)?;
        if victim.item.color.owner().is_some() {
            state
                .captures
                .get_mut(victim.capture_owner)
                .push(victim.item.clone());
        }
    }
    state
        .extra
        .insert("ruleBombs".into(), Value::Array(surviving_bombs));
    let visual_removed = removed
        .iter()
        .map(|victim| {
            Ok((
                serde_json::to_value(&victim.item).map_err(EngineError::serialization)?,
                victim.square.row,
                victim.square.col,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let visual_cells = cells
        .iter()
        .map(|square| (square.row, square.col))
        .collect::<Vec<_>>();
    crate::replay::queue_collapse_effect(
        state,
        owner,
        previous_depth + 1,
        &visual_cells,
        &visual_removed,
    )?;
    crate::replay::add_log(
        state,
        format!(
            "보드의 {}번째 외곽이 붕괴해 {}개의 기물이 사라졌습니다.",
            previous_depth + 1,
            removed.len()
        ),
    )?;
    if resolve_collapse_defeats(state, &removed)? {
        return Ok(true);
    }
    crate::v7_threat::check_racing_kings_v7(state)?;
    crate::v7_threat::check_racing_kings_v7(state)?;
    Ok(true)
}

fn collapse_victims(
    state: &GameState,
    owner: Color,
    cells: &[Square],
) -> Result<Vec<CollapseVictim>> {
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(unsupported("collapseEdges", "8x8 board geometry"));
    }
    let mut seen = std::collections::BTreeMap::<String, Piece>::new();
    let mut removed = Vec::new();
    for &square in cells {
        let Some(item) = state.at(square) else {
            continue;
        };
        if item.is_large() && item.id.is_empty() {
            return Err(unsupported("collapseEdges", "large piece without identity"));
        }
        if !item.id.is_empty() {
            if let Some(prior) = seen.get(&item.id) {
                if prior != item {
                    return Err(unsupported(
                        "collapseEdges",
                        "inconsistent duplicate piece identity",
                    ));
                }
                continue;
            }
            seen.insert(item.id.clone(), item.clone());
        }
        removed.push(CollapseVictim {
            item: item.clone(),
            square,
            capture_owner: if item.color == owner {
                owner.opponent()
            } else {
                owner
            },
        });
    }
    Ok(removed)
}

fn remove_collapse_piece(state: &mut GameState, victim: &CollapseVictim) {
    if victim.item.id.is_empty() {
        state.board[victim.square.row as usize][victim.square.col as usize] = None;
    } else {
        for cell in state.board.iter_mut().flatten() {
            if cell
                .as_ref()
                .is_some_and(|piece| piece.id == victim.item.id)
            {
                *cell = None;
            }
        }
    }
}

fn cancel_collapse_prophecies(state: &mut GameState) -> Result<()> {
    let Some(value) = state.extra.get_mut("prophecy") else {
        return Ok(());
    };
    if value.is_null() {
        return Ok(());
    }
    let prophecy = value
        .as_object_mut()
        .ok_or_else(|| EngineError::InvalidState("v7 prophecy must be an object".into()))?;
    for color in [Color::White, Color::Black] {
        if prophecy
            .get(color.as_str())
            .is_some_and(|value| truth(Some(value)))
        {
            prophecy.insert(color.as_str().into(), Value::Null);
        }
    }
    Ok(())
}

fn normalized_rule_bombs(state: &GameState, ring: &[Square]) -> Result<Vec<Value>> {
    let Some(input) = state.extra.get("ruleBombs").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let max_bombs = input.len().clamp(3, 64);
    let mut seen = BTreeSet::new();
    let mut bombs = Vec::new();
    let mut normalized_count = 0;
    for (index, entry) in input.iter().enumerate() {
        if normalized_count == max_bombs {
            break;
        }
        let Some(row) = js_number(entry.get("row")) else {
            continue;
        };
        let Some(col) = js_number(entry.get("col")) else {
            continue;
        };
        if row.fract() != 0.0
            || col.fract() != 0.0
            || !(0.0..8.0).contains(&row)
            || !(0.0..8.0).contains(&col)
        {
            continue;
        }
        let square = Square {
            row: row as u8,
            col: col as u8,
        };
        if !seen.insert(square) {
            continue;
        }
        normalized_count += 1;
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(source_bomb_text)
            .transpose()?
            .unwrap_or_else(|| format!("rule-bomb-{index}-{}-{}", square.row, square.col));
        let mut bomb = json!({"id":id,"row":square.row,"col":square.col});
        if let Some(ignore_id) = entry
            .get("ignorePieceId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        {
            bomb["ignorePieceId"] = json!(source_bomb_text(ignore_id)?);
        }
        if !ring.contains(&square) {
            bombs.push(bomb);
        }
    }
    Ok(bombs)
}

fn source_bomb_text(text: &str) -> Result<String> {
    String::from_utf16(&text.encode_utf16().take(160).collect::<Vec<_>>())
        .map_err(|_| unsupported("normalizeRuleBombs", "lone surrogate truncation"))
}

fn source_royal_king(state: &GameState, piece: &Piece) -> bool {
    piece.flag("crownRoyal")
        || piece.flag("editorRoyal")
        || matches!(
            piece.kind.as_str(),
            "king" | "royalKnight" | "shotgunKing" | "darkWizard"
        )
        || (piece.kind == "merchant"
            && state.extra.get("september18Balance") != Some(&json!(false)))
}

fn resolve_collapse_defeats(state: &mut GameState, removed: &[CollapseVictim]) -> Result<bool> {
    let removals = removed
        .iter()
        .map(|victim| crate::v7_board_hazards::EnvironmentalRemoval {
            piece: victim.item.clone(),
            square: victim.square,
            capture_owner: victim.capture_owner,
        })
        .collect::<Vec<_>>();
    crate::v7_board_hazards::resolve_environmental_defeats(state, &removals, "붕괴", false)
}

fn collapse_depth(state: &GameState) -> u64 {
    let fallback = if truth(state.extra.get("collapsed")) {
        1
    } else {
        0
    };
    nonzero_floor(state.extra.get("collapseDepth"), fallback, 0).min(4)
}

fn edge_ring(depth: u64) -> Vec<Square> {
    if depth >= 4 {
        return Vec::new();
    }
    let min = depth as u8;
    let max = 7 - min;
    let mut cells = Vec::new();
    for row in min..=max {
        for col in min..=max {
            if row == min || row == max || col == min || col == max {
                cells.push(Square { row, col });
            }
        }
    }
    cells
}

fn normalized_periodic(state: &GameState) -> Result<Option<Value>> {
    let Some(value) = state.extra.get("periodicCollapse") else {
        return Ok(None);
    };
    if !truth(value.get("enabled")) {
        return Ok(None);
    }
    if !value.is_object() {
        return Err(EngineError::InvalidState(
            "periodicCollapse must be an object".into(),
        ));
    }
    let fallback = next_interval(shared_turn(state), PERIODIC_COLLAPSE_INTERVAL);
    Ok(Some(
        json!({"enabled":true,"interval":PERIODIC_COLLAPSE_INTERVAL,"nextAt":nonzero_floor(value.get("nextAt"), fallback, 1)}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EnPassant, GameConfig, Piece};
    use sha2::{Digest, Sha256};

    fn state(actor: Color) -> GameState {
        let mut state = GameState::new(GameConfig::default(), 19).expect("state");
        state.ruleset_id = RULES_VERSION_V7.into();
        state.mode = "play".into();
        state.turn = actor;
        state
    }

    fn source_state_hash(state: &GameState) -> String {
        let mut value = serde_json::to_value(state).expect("state JSON");
        let fields = value.as_object_mut().expect("state object");
        for key in ["rulesetId", "rng", "history"] {
            fields.remove(key);
        }
        format!(
            "{:x}",
            Sha256::digest(serde_jcs::to_vec(&value).expect("JCS"))
        )
    }

    /// The receipts remain outside Git; an explicit test invocation requires
    /// the source-pinned snapshots instead of counting a missing oracle pass.
    #[test]
    #[ignore = "requires ACCELERATE_V7_BOARD_SOURCE_RECEIPTS with frozen source receipts"]
    fn frozen_board_callbacks_when_source_receipts_supplied() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_BOARD_SOURCE_RECEIPTS")
                .expect("source receipt directory"),
        );
        for name in [
            "conveyor-twin-ultimatum",
            "conveyor-synchronization",
            "revolving-d4",
            "conveyor-bomb",
            "conveyor-promotion",
            "conveyor-collapsed",
            "crown-ground",
            "crown-multiple",
            "collapse-crown-holder",
            "collapse-recurrence",
        ] {
            let receipt: Value = serde_json::from_slice(
                &std::fs::read(root.join(format!("{name}.json"))).expect("frozen source receipt"),
            )
            .expect("source JSON");
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            let mut state: GameState =
                serde_json::from_value(receipt["before"].clone()).expect("source state");
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(receipt["rngBefore"].clone()).expect("source RNG");
            if name.starts_with("conveyor-") {
                V7BoardAutomata
                    .before_count(&mut state, Color::Black)
                    .expect(name);
            } else if name == "revolving-d4" {
                V7BoardAutomata
                    .after_taboo(&mut state, Color::Black)
                    .expect(name);
            } else if name.starts_with("crown-") {
                V7BoardAutomata
                    .after_empty_lunchboxes(&mut state, Color::Black)
                    .expect(name);
            } else {
                collapse_edges(&mut state, Color::Black).expect(name);
            }
            assert_eq!(
                source_state_hash(&state),
                receipt["afterJcsSha256"].as_str().expect("source digest"),
                "{name}"
            );
            let source_rng: crate::state::RngState =
                serde_json::from_value(receipt["rngAfter"].clone()).expect("after RNG");
            assert_eq!(state.rng, source_rng, "{name} RNG");
            assert!(state.history.is_empty(), "{name} direct callback history");
        }
    }

    #[test]
    #[ignore = "requires ACCELERATE_V7_BOARD_SOURCE_RECEIPTS with frozen crown receipts"]
    fn frozen_crown_capture_callbacks_when_source_receipts_supplied() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_BOARD_SOURCE_RECEIPTS")
                .expect("source receipt directory"),
        );
        for name in [
            "crown-transfer",
            "crown-large-ground",
            "crown-regency-legacy",
            "crown-legacy-boolean",
        ] {
            let receipt: Value = serde_json::from_slice(
                &std::fs::read(root.join(format!("{name}.json"))).expect("frozen crown receipt"),
            )
            .expect("source JSON");
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            let mut state: GameState =
                serde_json::from_value(receipt["before"].clone()).expect("source state");
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(receipt["rngBefore"].clone()).expect("source RNG");
            if name == "crown-transfer" {
                let at = Square { row: 2, col: 2 };
                let attacker = state.at(at).cloned().expect("attacker");
                let mut captured = state.captures.black[0].clone();
                assert!(
                    transfer_crown_after_capture(&mut state, &mut captured, Some(&attacker), &[at])
                        .expect("transfer")
                );
                state.captures.black[0] = captured;
            }
            reconcile_crown_rule(&mut state, true).expect(name);
            assert_eq!(
                source_state_hash(&state),
                receipt["afterJcsSha256"].as_str().expect("source digest"),
                "{name}"
            );
            let source_rng: crate::state::RngState =
                serde_json::from_value(receipt["rngAfter"].clone()).expect("after RNG");
            assert_eq!(state.rng, source_rng, "{name} RNG");
            assert!(state.history.is_empty(), "{name} direct callback history");
        }
    }

    #[test]
    #[ignore = "requires ACCELERATE_V7_BOARD_SOURCE_RECEIPTS with frozen AI crown receipt"]
    fn frozen_crown_ai_context_when_source_receipt_supplied() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_BOARD_SOURCE_RECEIPTS")
                .expect("source receipt directory"),
        );
        let receipt: Value = serde_json::from_slice(
            &std::fs::read(root.join("crown-ai-large-transfer.json"))
                .expect("frozen AI crown receipt"),
        )
        .expect("source JSON");
        assert_eq!(
            receipt["sourceSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        let mut state: GameState =
            serde_json::from_value(receipt["before"].clone()).expect("source state");
        state.ruleset_id = RULES_VERSION_V7.into();
        state.rng = serde_json::from_value(receipt["rngBefore"].clone()).expect("source RNG");
        reconcile_crown_rule_with_simulation(&mut state, true, true)
            .expect("AI crown reconciliation");
        assert_eq!(
            source_state_hash(&state),
            receipt["afterJcsSha256"].as_str().expect("source digest")
        );
        let source_rng: crate::state::RngState =
            serde_json::from_value(receipt["rngAfter"].clone()).expect("after RNG");
        assert_eq!(state.rng, source_rng);
        assert!(state.history.is_empty());
    }

    #[test]
    fn collapse_matches_frozen_source_full_state_rng_and_history() {
        // FrozenClientSource e5ed84fc… / source newGame({draftDelete:true},19)
        // with the same current board, turn, and rule fields, followed by
        // collapseEdges("black"). The digest covers the entire source state,
        // including captures, rule bombs, visual queue, notation and logs.
        for (royal, expected_hash) in [
            (
                false,
                "7bf8d71efef08742bbb91ea3f9c145b255a18fa0e0f7b3ff0fbaf1103e67f3bb",
            ),
            (
                true,
                "efea8cf3f019c582d1e74b33a0f3cf533b5f12d1f9eac48ee90c65937b222dff",
            ),
        ] {
            let mut state = crate::v7_new_game::new_game(
                GameConfig {
                    draft_delete: true,
                    ..GameConfig::default()
                },
                19,
            )
            .expect("source initial state");
            state.mode = "play".into();
            state.turn = Color::Black;
            state.board = vec![vec![None; 8]; 8];
            state.board[3][3] = Some(Piece::new("king", Color::White, "wking"));
            state.board[4][4] = Some(Piece::new("king", Color::Black, "bking"));
            state
                .extra
                .insert("replayEndedAt".into(), json!("2026-09-28T07:41:32.828Z"));
            if royal {
                state.board[4][4] = None;
                state.board[0][4] = Some(Piece::new("king", Color::Black, "bking"));
            } else {
                state.board[0][0] = Some(Piece::new("rook", Color::White, "wrook"));
                state.extra.insert(
                    "ruleBombs".into(),
                    json!([
                        {"id":"edge","row":0,"col":1},
                        {"id":"inner","row":3,"col":3}
                    ]),
                );
                state
                    .extra
                    .insert("vigilance".into(), json!({"white":true,"black":false}));
                state.extra.insert(
                    "prophecy".into(),
                    json!({"white":{"pieceId":"wrook","remaining":1},"black":null}),
                );
            }
            assert!(collapse_edges(&mut state, Color::Black).expect("collapse"));
            assert_eq!(source_state_hash(&state), expected_hash, "royal={royal}");
            assert_eq!((state.rng.cursor, state.rng.state), (33, 2_893_839_862));
            assert!(state.history.is_empty());
        }
    }

    #[test]
    fn collapse_environmental_branches_match_frozen_source_full_state() {
        // Frozen e5ed84fc source, newGame({draftDelete:true}, 19), then the
        // same board/rule edits and collapseEdges("black"). The digest covers
        // every state field except the host-only rulesetId/rng/history.
        for (case, expected_hash) in [
            (
                "crown",
                "01aefeb843503c44c66b63f2a0c357fea62f9f7ad244d04501fe676330869b1c",
            ),
            (
                "resolve",
                "a00d17223d5cfd39eb86f8af4bc843e3c63447fe0ab548bb0227043f8d6e07d8",
            ),
            (
                "democracy",
                "cc52ee0089c0acf4ae9cb75c44b3935315a782a419387746091bc178fe4151cf",
            ),
            (
                "neutral-royal",
                "ecee22f5c86d219e7e6ab8f2bb74cb1007236284dab1786526fe43234b8632c8",
            ),
            (
                "regency",
                "0a854ce16c4f5efea9750e0febeb29bca6d3b54cd69aaf5f777d84f3dac71bf6",
            ),
        ] {
            let mut state = crate::v7_new_game::new_game(
                GameConfig {
                    draft_delete: true,
                    ..GameConfig::default()
                },
                19,
            )
            .expect("source initial state");
            state.mode = "play".into();
            state.turn = Color::Black;
            state.board = vec![vec![None; 8]; 8];
            state.board[3][3] = Some(Piece::new("king", Color::White, "wking"));
            state.board[4][4] = Some(Piece::new("king", Color::Black, "bking"));
            match case {
                "crown" => {
                    state.extra.insert(
                        "crownRule".into(),
                        json!({"id":"crown-1","crownGroupSize":1,"enabled":true,
                            "holderId":"","ground":{"row":0,"col":0},"removed":false,
                            "pendingTransfer":null,"holdingColor":"","countUnit":"full-turn",
                            "heldMoves":{"white":0,"black":0},"lastCountedMove":0}),
                    );
                }
                "resolve" => {
                    state.board[0][0] = Some(Piece::new("pawn", Color::White, "pawn"));
                    state
                        .extra
                        .insert("resolve".into(), json!({"white":true,"black":false}));
                    state
                        .extra
                        .insert("resolveReady".into(), json!({"white":false,"black":false}));
                    state
                        .extra
                        .insert("resolveSpentTurn".into(), json!({"white":-1,"black":-1}));
                }
                "democracy" => {
                    state.board[0][0] = Some(Piece::new("pawn", Color::White, "pawn"));
                    state
                        .extra
                        .insert("democracy".into(), json!({"white":true,"black":false}));
                }
                "neutral-royal" => {
                    state.board[0][0] = Some(Piece::new(
                        "king",
                        crate::PieceColor::Neutral,
                        "neutral-king",
                    ));
                }
                "regency" => {
                    state.board[0][0] = Some(Piece::new("king", Color::White, "wking-outer"));
                    state.board[2][2] = Some(Piece::new("queen", Color::White, "wqueen"));
                    state
                        .extra
                        .insert("regency".into(), json!({"white":true,"black":false}));
                }
                _ => unreachable!(),
            }
            assert!(collapse_edges(&mut state, Color::Black).expect(case));
            assert_eq!(source_state_hash(&state), expected_hash, "{case}");
            assert_eq!((state.rng.cursor, state.rng.state), (33, 2_893_839_862));
            assert!(state.history.is_empty());
        }
    }

    #[test]
    fn periodic_multi_ring_matches_source_including_terminal_schedule() {
        // Frozen `resolvePeriodicCollapseAfterTurn("black")` with turn 40
        // catches up two rings. The source advances nextAt after the second
        // collapse even when the second ring ends the game.
        for (royal, expected_hash) in [
            (
                false,
                "971c59d11f502f540bff162a49277af233aed59303fdc9d906d25f0ce7b5fcac",
            ),
            (
                true,
                "a0ead4a986bf52f33f819fabda3b17550606d4ee65f81d182c8e5a330fe67534",
            ),
        ] {
            let mut state = crate::v7_new_game::new_game(
                GameConfig {
                    draft_delete: true,
                    ..GameConfig::default()
                },
                19,
            )
            .expect("source initial state");
            state.mode = "play".into();
            state.turn = Color::Black;
            state.board = vec![vec![None; 8]; 8];
            state.board[3][3] = Some(Piece::new("king", Color::White, "wking"));
            state.board[4][4] = Some(Piece::new("king", Color::Black, "bking"));
            state.board[0][0] = Some(Piece::new("rook", Color::White, "outer"));
            state.board[1][1] = Some(Piece::new(
                if royal { "king" } else { "knight" },
                if royal { Color::Black } else { Color::White },
                "inner",
            ));
            if royal {
                state.board[4][4] = None;
            }
            state.turns_taken.white = 40;
            state.turns_taken.black = 40;
            state.extra.insert(
                "periodicCollapse".into(),
                json!({"enabled":true,"nextAt":20}),
            );
            state
                .extra
                .insert("replayEndedAt".into(), json!("2026-09-28T07:41:32.828Z"));
            let flow = V7BoardAutomata
                .after_full_move(&mut state, Color::Black)
                .expect("periodic collapse");
            assert_eq!(
                flow,
                if royal {
                    V7FlowControl::Terminal
                } else {
                    V7FlowControl::Continue
                }
            );
            assert_eq!(source_state_hash(&state), expected_hash, "royal={royal}");
            assert_eq!((state.rng.cursor, state.rng.state), (34, 3_858_193_629));
            assert!(state.history.is_empty());
        }
    }

    #[test]
    fn conveyor_keeps_source_ring_order_and_does_not_draw_rng() {
        let mut state = state(Color::Black);
        state.board = vec![vec![None; 8]; 8];
        state.board[0][0] = Some(Piece::new("rook", Color::Black, "r"));
        state.board[0][1] = Some(Piece::new("knight", Color::Black, "n"));
        state.extra.insert("conveyorRule".into(), json!(true));
        state.extra.insert(
            "pendingScarecrows".into(),
            json!([{"row":0,"col":0,"id":"a"},{"row":3,"col":3,"id":"b"}]),
        );
        state.extra.insert(
            "pendingLobsters".into(),
            json!([{"row":0,"col":7,"id":"c"}]),
        );
        state.en_passant = None;
        let before = state.rng.clone();
        V7BoardAutomata
            .before_count(&mut state, Color::Black)
            .expect("conveyor");
        assert!(state.at(Square { row: 0, col: 0 }).is_none());
        assert_eq!(state.at(Square { row: 0, col: 1 }).expect("rook").id, "r");
        assert_eq!(state.at(Square { row: 0, col: 2 }).expect("knight").id, "n");
        assert!(state.at(Square { row: 0, col: 1 }).expect("rook").moved);
        assert!(state.at(Square { row: 0, col: 2 }).expect("knight").moved);
        assert_eq!(state.rng, before);
        let visual = state.extra["pendingReplayVisuals"]
            .as_array()
            .expect("visual")
            .last()
            .expect("visual");
        assert_eq!(visual["type"], "effect");
        assert_eq!(
            visual["transitions"].as_array().expect("transitions").len(),
            2
        );
        assert_eq!(
            state.extra["logs"][0],
            "컨베이어: 기물 2개가 시계방향으로 이동했습니다."
        );
        assert_eq!(
            state.extra["forceAnimatedPieceIds"]["values"],
            json!(["r", "n"])
        );
        assert_eq!(
            state.extra["pendingScarecrows"],
            json!([{"row":0,"col":1,"id":"a"},{"row":3,"col":3,"id":"b"}])
        );
        assert_eq!(
            state.extra["pendingLobsters"],
            json!([{"row":1,"col":7,"id":"c"}])
        );
    }

    #[test]
    fn conveyor_checks_campaign_objective_only_after_a_belt_transition() {
        let mut moving = state(Color::Black);
        moving.board = vec![vec![None; 8]; 8];
        moving.board[0][0] = Some(Piece::new("rook", Color::Black, "belt-rook"));
        moving.extra.insert("conveyorRule".into(), json!(true));
        moving.extra.insert(
            "campaign".into(),
            json!({"setup":"knightJourney","playerColor":"white"}),
        );
        let result = V7BoardAutomata
            .before_count(&mut moving, Color::Black)
            .expect("campaign belt");
        assert_eq!(result, V7FlowControl::Terminal);
        assert_eq!(moving.mode, "gameover");
        assert_eq!(moving.winner.as_deref(), Some("black"));
        assert_eq!(
            moving.at(Square { row: 0, col: 1 }).expect("moved rook").id,
            "belt-rook"
        );
        assert_eq!(moving.extra["logs"][0], "흑 승리: 나이트가 사라졌습니다.");
        assert_eq!(
            moving.extra["logs"][1],
            "컨베이어: 기물 1개가 시계방향으로 이동했습니다."
        );

        let mut stationary = state(Color::Black);
        stationary.board = vec![vec![None; 8]; 8];
        stationary.extra.insert("conveyorRule".into(), json!(true));
        stationary.extra.insert(
            "campaign".into(),
            json!({"setup":"knightJourney","playerColor":"white"}),
        );
        let result = V7BoardAutomata
            .before_count(&mut stationary, Color::Black)
            .expect("empty belt");
        assert_eq!(result, V7FlowControl::Continue);
        assert_eq!(stationary.mode, "play");
    }

    #[test]
    fn pending_collapse_removes_ring_once_and_clears_reservation() {
        let mut state = state(Color::White);
        state.board = vec![vec![None; 8]; 8];
        state.board[0][0] = Some(Piece::new("rook", Color::Black, "outer"));
        state.board[3][3] = Some(Piece::new("king", Color::White, "white-king"));
        state.board[4][4] = Some(Piece::new("king", Color::Black, "black-king"));
        state.extra.insert("collapsePending".into(), json!("white"));
        let cursor = state.rng.cursor;
        let result = V7BoardAutomata
            .incoming_after_switch(&mut state, Color::White)
            .expect("collapse");
        assert_eq!(result, V7FlowControl::Continue);
        assert_eq!(state.extra["collapsePending"], Value::Null);
        assert_eq!(state.extra["collapseDepth"], 1);
        assert_eq!(state.extra["collapsed"], true);
        assert!(state.board[0][0].is_none());
        assert_eq!(state.captures.white.len(), 1);
        assert_eq!(state.captures.white[0].id, "outer");
        assert_eq!(state.rng.cursor, cursor + 1);
        assert_eq!(state.extra["pendingReplayVisuals"][0]["type"], "collapse");
        assert_eq!(
            state.extra["pendingReplayVisuals"][0]["cells"]
                .as_array()
                .unwrap()
                .len(),
            28
        );
        assert_eq!(state.extra["pendingNotations"][0]["text"], "!붕괴");
    }

    #[test]
    fn platform_preview_and_spawn_reuse_one_source_draw() {
        let mut state = state(Color::White);
        state.board = vec![vec![None; 8]; 8];
        state.board[0][0] = Some(Piece::new("rook", Color::Black, "r"));
        state.turns_taken.white = 5;
        state.turns_taken.black = 4;
        state.rng.cursor = 0;
        state.rng.tape = vec![0.56];
        state
            .extra
            .insert("platformRule".into(), json!({"enabled":true,"nextAt":5}));
        let cursor = state.rng.cursor;
        V7BoardAutomata
            .after_empty_lunchboxes(&mut state, Color::White)
            .expect("preview");
        assert_eq!(state.rng.cursor, cursor + 1);
        assert_eq!(state.extra["platformRule"]["previewAt"], 5);
        assert_eq!(
            state.extra["platformRule"]["previewCell"],
            json!({"row":4,"col":4})
        );
        state.turn = Color::Black;
        state.turns_taken.black = 5;
        V7BoardAutomata
            .after_empty_lunchboxes(&mut state, Color::Black)
            .expect("spawn");
        assert_eq!(state.rng.cursor, cursor + 1);
        assert_eq!(
            state.extra["platformRule"]["cell"],
            json!({"row":4,"col":4})
        );
        assert_eq!(state.extra["platformRule"]["nextAt"], 10);
        assert_eq!(state.extra["platformRule"]["nonce"], 1);
        assert_eq!(
            state.extra["logs"][0],
            "발판: e4에 새 발판이 생성되었습니다."
        );
    }

    #[test]
    fn canonical_empty_crown_ground_is_inert_until_occupied() {
        let mut state = state(Color::White);
        state.board[4][4] = None;
        state.extra.insert(
            "crownRule".into(),
            json!({
                "id":"crown-1","crownGroupSize":1,"enabled":true,
                "holderId":"","ground":{"row":4,"col":4},"removed":false,
                "pendingTransfer":null,"holdingColor":"","countUnit":"full-turn",
                "heldMoves":{"white":0,"black":0},"lastCountedMove":0
            }),
        );
        let before = state.clone();
        V7BoardAutomata
            .after_empty_lunchboxes(&mut state, Color::White)
            .expect("empty crown ground");
        assert_eq!(state, before);
        state.board[4][4] = Some(Piece::new("rook", Color::White, "occupant"));
        let cursor = state.rng.cursor;
        V7BoardAutomata
            .after_empty_lunchboxes(&mut state, Color::White)
            .expect("ground occupant takes crown");
        let holder = state.at(Square { row: 4, col: 4 }).expect("holder");
        assert_eq!(holder.kind, "crown");
        assert_eq!(holder.id, "occupant");
        assert_eq!(holder.extra["origin"], "e4");
        assert_eq!(state.extra["crownRule"]["holderId"], "occupant");
        assert_eq!(state.extra["crownRule"]["ground"], Value::Null);
        assert_eq!(state.rng.cursor, cursor + 2);
    }

    #[test]
    fn single_crown_holder_matches_frozen_source_progress_and_win() {
        // Frozen e5ed84fc source newGame({draftDelete:true}, 19), followed by
        // the same board, turn, counter and crown edits, then source
        // resolveCrownRuleAfterMove(). The win path includes notation RNG and
        // replay-end state, not just the heldMoves projection.
        for (held, expected_hash, expected_cursor, expected_rng_state, terminal) in [
            (
                8,
                "8fe5b05d1c9d6412357986d28bc40054ec6b242e49b92e7ea664cbee8d1c94f2",
                32,
                4_163_866_163,
                false,
            ),
            (
                9,
                "780f496471ef870098e19ed3cf320236442b98e3bb0a1efad228ad41b750ee67",
                33,
                2_893_839_862,
                true,
            ),
        ] {
            let mut state = crate::v7_new_game::new_game(
                GameConfig {
                    draft_delete: true,
                    ..GameConfig::default()
                },
                19,
            )
            .expect("source initial state");
            state.mode = "play".into();
            state.turn = Color::Black;
            state.board = vec![vec![None; 8]; 8];
            let mut crown = Piece::new("crown", Color::White, "holder");
            crown.moved = true;
            crown.extra.insert("crownBearer".into(), json!(true));
            crown
                .extra
                .insert("crownTokenIds".into(), json!(["crown-1"]));
            state.board[3][3] = Some(crown);
            state.board[4][4] = Some(Piece::new("king", Color::Black, "bking"));
            state.turns_taken.white = 1;
            state.turns_taken.black = 1;
            state.extra.insert(
                "crownRule".into(),
                json!({"id":"crown-1","crownGroupSize":1,"enabled":true,
                    "holderId":"holder","ground":null,"removed":false,
                    "pendingTransfer":null,"holdingColor":"white","countUnit":"full-turn",
                    "heldMoves":{"white":held,"black":0},"lastCountedMove":0}),
            );
            let outcome = V7BoardAutomata
                .after_empty_lunchboxes(&mut state, Color::Black)
                .expect("crown turn");
            assert_eq!(outcome == V7FlowControl::Terminal, terminal);
            assert_eq!(source_state_hash(&state), expected_hash, "held={held}");
            assert_eq!(
                (state.rng.cursor, state.rng.state),
                (expected_cursor, expected_rng_state)
            );
            assert!(state.history.is_empty());
        }
    }

    #[test]
    fn collapse_with_surviving_crown_holder_matches_frozen_source() {
        // Frozen e5ed84fc source collapseEdges("black") with an interior
        // holder and one outer rook. Environmental defeat reconciliation does
        // not advance the crown turn counter at this callback.
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .expect("source initial state");
        state.mode = "play".into();
        state.turn = Color::Black;
        state.board = vec![vec![None; 8]; 8];
        let mut crown = Piece::new("crown", Color::White, "holder");
        crown.moved = true;
        crown.extra.insert("crownBearer".into(), json!(true));
        crown
            .extra
            .insert("crownTokenIds".into(), json!(["crown-1"]));
        state.board[3][3] = Some(crown);
        state.board[4][4] = Some(Piece::new("king", Color::Black, "bking"));
        state.board[0][0] = Some(Piece::new("rook", Color::White, "outer"));
        state.turns_taken.white = 1;
        state.turns_taken.black = 1;
        state.extra.insert(
            "crownRule".into(),
            json!({"id":"crown-1","crownGroupSize":1,"enabled":true,
                "holderId":"holder","ground":null,"removed":false,
                "pendingTransfer":null,"holdingColor":"white","countUnit":"full-turn",
                "heldMoves":{"white":8,"black":0},"lastCountedMove":0}),
        );
        assert!(collapse_edges(&mut state, Color::Black).expect("crown collapse"));
        assert_eq!(
            source_state_hash(&state),
            "14baf63a3ff20411741c8e49fbbd320939e9aafaff272bc2bd94262ba5a5bc7d"
        );
        assert_eq!((state.rng.cursor, state.rng.state), (33, 2_893_839_862));
        assert_eq!(state.extra["crownRule"]["heldMoves"]["white"], 8);
        assert!(state.history.is_empty());
    }

    #[test]
    fn platform_candidates_exclude_source_ground_hazard_and_reservations() {
        let mut state = state(Color::White);
        state.board = vec![vec![None; 8]; 8];
        state
            .extra
            .insert("blackHole".into(), json!([{"row":4,"col":4}]));
        state.extra.insert(
            "crownRule".into(),
            json!({
                "id":"crown-1","crownGroupSize":1,"enabled":true,
                "holderId":"","ground":{"row":4,"col":5},"removed":false,
                "pendingTransfer":null,"holdingColor":"","countUnit":"full-turn",
                "heldMoves":{"white":0,"black":0},"lastCountedMove":0
            }),
        );
        state.extra.insert(
            "pendingScarecrows".into(),
            json!([{"row":4,"col":6,"id":"s"}]),
        );
        state.extra.insert(
            "pendingLobsters".into(),
            json!([{"row":4,"col":7,"id":"l"}]),
        );
        state.extra.insert(
            "pendingPortals".into(),
            json!([{"cells":[{"row":5,"col":0}]}]),
        );
        let candidates = platform_candidates(&state).expect("source candidates");
        assert_eq!(candidates.len(), 59);
        for square in [
            Square { row: 4, col: 4 },
            Square { row: 4, col: 5 },
            Square { row: 4, col: 6 },
            Square { row: 4, col: 7 },
            Square { row: 5, col: 0 },
        ] {
            assert!(!candidates.contains(&square));
        }
    }

    #[test]
    fn revolving_door_rotates_adjacent_ring_without_moving_its_center() {
        let mut state = state(Color::Black);
        state.board = vec![vec![None; 8]; 8];
        state.board[3][3] = Some(Piece::new("revolvingDoor", Color::Black, "door"));
        state.board[2][2] = Some(Piece::new("rook", Color::Black, "r"));
        state.board[2][3] = Some(Piece::new("knight", Color::Black, "n"));
        let rng = state.rng.clone();
        V7BoardAutomata
            .after_taboo(&mut state, Color::Black)
            .expect("door");
        assert_eq!(
            state.at(Square { row: 3, col: 3 }).expect("center").id,
            "door"
        );
        assert_eq!(state.at(Square { row: 2, col: 3 }).expect("rook").id, "r");
        assert_eq!(state.at(Square { row: 2, col: 4 }).expect("knight").id, "n");
        assert_eq!(state.rng, rng);
        assert_eq!(
            state.extra["logs"][0],
            "회전문: 기물 2개가 시계방향으로 이동했습니다."
        );
    }

    #[test]
    fn conveyor_breaks_out_of_range_chain_after_movement() {
        let mut state = state(Color::Black);
        state.board = vec![vec![None; 8]; 8];
        state.board[0][0] = Some(Piece::new("rook", Color::Black, "r"));
        state.board[3][3] = Some(Piece::new("knight", Color::Black, "n"));
        state.extra.insert("conveyorRule".into(), json!(true));
        state.extra.insert(
            "chainBonds".into(),
            json!([{"id":"bond","aId":"r","bId":"n","by":"black"}]),
        );
        V7BoardAutomata
            .before_count(&mut state, Color::Black)
            .expect("conveyor");
        assert_eq!(state.extra["chainBonds"], json!([]));
        assert_eq!(
            state.extra["logs"][0],
            "사슬: 컨베이어 이동으로 1개의 사슬이 끊어졌습니다."
        );
        assert_eq!(
            state.extra["logs"][1],
            "컨베이어: 기물 1개가 시계방향으로 이동했습니다."
        );
    }

    #[test]
    fn factory_belts_alternate_direction_even_without_conveyor_rule() {
        let mut state = state(Color::Black);
        state.board = vec![vec![None; 8]; 8];
        state.board[0][0] = Some(Piece::new("rook", Color::Black, "outer"));
        state.board[1][1] = Some(Piece::new("rook", Color::Black, "inner"));
        state
            .extra
            .insert("campaign".into(), json!({"setup":"conveyorFactory"}));
        state.extra.insert("conveyorRule".into(), json!(false));
        let rng = state.rng.clone();
        V7BoardAutomata
            .before_count(&mut state, Color::Black)
            .expect("factory belts");
        assert_eq!(
            state.at(Square { row: 0, col: 1 }).expect("outer").id,
            "outer"
        );
        assert_eq!(
            state.at(Square { row: 2, col: 1 }).expect("inner").id,
            "inner"
        );
        assert_eq!(state.rng, rng);
        assert_eq!(
            state.extra["logs"][0],
            "컨베이어 공장 2번 벨트: 기물 1개가 벨트 방향으로 이동했습니다."
        );
        assert_eq!(
            state.extra["logs"][1],
            "컨베이어 공장 1번 벨트: 기물 1개가 벨트 방향으로 이동했습니다."
        );
    }

    #[test]
    fn conveyor_relocates_en_passant_right_with_its_pawn() {
        let mut state = state(Color::Black);
        state.board = vec![vec![None; 8]; 8];
        state.board[3][0] = Some(Piece::new("pawn", Color::Black, "p"));
        state.extra.insert("conveyorRule".into(), json!(true));
        state.en_passant = Some(EnPassant {
            row: 4,
            col: 0,
            captured_row: 3,
            captured_col: 0,
            color: Color::Black,
            extra: Default::default(),
        });
        V7BoardAutomata
            .before_count(&mut state, Color::Black)
            .expect("pawn belt");
        assert_eq!(state.at(Square { row: 2, col: 0 }).expect("pawn").id, "p");
        assert_eq!(state.en_passant.as_ref().expect("right").row, 1);
        assert_eq!(state.en_passant.as_ref().expect("right").captured_row, 2);
        assert_eq!(state.en_passant.as_ref().expect("right").col, 0);
    }

    #[test]
    fn periodic_warning_is_before_due_collapse_without_rng_draw() {
        let mut state = state(Color::Black);
        state.turns_taken.white = 19;
        state.turns_taken.black = 19;
        state.extra.insert(
            "periodicCollapse".into(),
            json!({"enabled":true,"nextAt":20}),
        );
        let rng = state.rng.clone();
        V7BoardAutomata
            .after_full_move(&mut state, Color::Black)
            .expect("warning");
        assert_eq!(state.rng, rng);
        let visual = state.extra["pendingReplayVisuals"]
            .as_array()
            .expect("visuals")
            .last()
            .expect("warning");
        assert_eq!(visual["type"], "collapse-warning");
        assert_eq!(visual["cells"].as_array().expect("cells").len(), 28);
    }

    #[test]
    fn due_periodic_collapse_advances_depth_and_schedule() {
        let mut state = state(Color::Black);
        state.board = vec![vec![None; 8]; 8];
        state.board[0][0] = Some(Piece::new("rook", Color::White, "outer"));
        state.board[3][3] = Some(Piece::new("king", Color::White, "white-king"));
        state.board[4][4] = Some(Piece::new("king", Color::Black, "black-king"));
        state.turns_taken.white = 20;
        state.turns_taken.black = 20;
        state.extra.insert(
            "periodicCollapse".into(),
            json!({"enabled":true,"nextAt":20}),
        );
        let cursor = state.rng.cursor;
        let result = V7BoardAutomata
            .after_full_move(&mut state, Color::Black)
            .expect("periodic collapse");
        assert_eq!(result, V7FlowControl::Continue);
        assert_eq!(state.extra["collapseDepth"], 1);
        assert_eq!(state.extra["periodicCollapse"]["nextAt"], 40);
        assert_eq!(state.rng.cursor, cursor + 1);
    }

    #[test]
    fn malformed_prophecy_rolls_back_collapse_before_any_commit() {
        let mut state = state(Color::Black);
        state.extra.insert("collapsePending".into(), json!("black"));
        state
            .extra
            .insert("prophecy".into(), json!("invalid-color-map"));
        let before = state.clone();
        let error = V7BoardAutomata
            .incoming_after_switch(&mut state, Color::Black)
            .unwrap_err();
        assert!(
            matches!(error, EngineError::InvalidState(message) if message.contains("prophecy"))
        );
        assert_eq!(state, before);
    }

    #[test]
    fn periodic_collapse_after_final_ring_only_advances_schedule() {
        let mut state = state(Color::Black);
        state.turns_taken.white = 80;
        state.turns_taken.black = 80;
        state.extra.insert("collapsed".into(), json!(true));
        state.extra.insert("collapseDepth".into(), json!(4));
        state.extra.insert(
            "periodicCollapse".into(),
            json!({"enabled":true,"nextAt":80}),
        );
        let rng = state.rng.clone();
        V7BoardAutomata
            .after_full_move(&mut state, Color::Black)
            .expect("no fifth ring");
        assert_eq!(
            state.extra["periodicCollapse"],
            json!({"enabled":true,"interval":20,"nextAt":100})
        );
        assert_eq!(state.extra["collapseDepth"], 4);
        assert_eq!(state.rng, rng);
    }
}
