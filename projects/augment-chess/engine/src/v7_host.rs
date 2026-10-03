//! Source-shaped v7 state admission and atomic host transactions.
//!
//! The source DTO remains lossless at the JSON boundary. `SpatialState` is a
//! checked, derived view of its board; RNG and events belong to the same
//! immutable position and are committed together with every state change.

use crate::spatial_state::SpatialState;
use crate::state::{
    Color, EngineError, GameState, PublicEvent, RULES_VERSION_V7, Result, RngState,
    validate_json_value,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

pub const V7_POSITION_PROTOCOL: &str = "accelerate-position-v1";
const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;
const ENVELOPE_FIELDS: [&str; 7] = [
    "protocolVersion",
    "rulesVersion",
    "catalogVersion",
    "state",
    "rng",
    "history",
    "positionId",
];

/// An immutable source v7 position. The original state shape is retained so
/// typed defaults and internally assigned piece IDs do not silently appear in
/// a source snapshot that did not contain them.
#[derive(Clone, Debug)]
pub struct V7HostPosition {
    state: Arc<GameState>,
    spatial: Arc<SpatialState>,
    original_state: Arc<Value>,
    baseline_state: Arc<Value>,
    position_id: String,
    revision: u64,
    colossus_reservation: Option<Arc<ColossusReservation>>,
}

#[derive(Debug)]
struct ColossusReservation {
    resolving_state: Weak<GameState>,
    actor: Color,
    issued: AtomicBool,
}

/// live host가 보유하는 원문 Colossus timer 권한. wire 입력으로 만들거나 복제할 수 없다.
/// source의 `state === resolvingState`를 Arc 인스턴스 동일성으로 검사한다.
#[derive(Debug)]
pub struct V7ColossusCompletion {
    scheduled_state: Weak<GameState>,
    actor: Color,
}

impl V7HostPosition {
    /// Import the exact public v7 position envelope. Identity covers state,
    /// RNG, history and version metadata, excluding only `positionId` itself.
    pub fn from_envelope(envelope: Value) -> Result<Self> {
        validate_json_value(&envelope, 0)?;
        let fields = envelope.as_object().ok_or_else(|| {
            EngineError::InvalidState("v7 position envelope must be an object".into())
        })?;
        if fields.len() != ENVELOPE_FIELDS.len()
            || ENVELOPE_FIELDS
                .iter()
                .any(|name| !fields.contains_key(*name))
        {
            return Err(EngineError::InvalidState(
                "v7 position envelope has missing or unexpected fields".into(),
            ));
        }
        if fields.get("protocolVersion").and_then(Value::as_str) != Some(V7_POSITION_PROTOCOL)
            || fields.get("rulesVersion").and_then(Value::as_str) != Some(RULES_VERSION_V7)
            || fields.get("catalogVersion").and_then(Value::as_str)
                != Some(v7_catalog_version()?.as_str())
        {
            return Err(EngineError::InvalidState(
                "v7 position protocol, rules or catalog version mismatch".into(),
            ));
        }
        let supplied_id = fields
            .get("positionId")
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::InvalidState("v7 positionId must be text".into()))?;
        let mut content = fields.clone();
        content.shift_remove("positionId");
        let actual_id = digest(&Value::Object(content))?;
        if supplied_id != actual_id {
            return Err(EngineError::InvalidState(
                "v7 positionId does not match the complete snapshot".into(),
            ));
        }
        let raw_state = fields
            .get("state")
            .ok_or_else(|| EngineError::InvalidState("v7 position state is missing".into()))?;
        let state_fields = raw_state.as_object().ok_or_else(|| {
            EngineError::InvalidState("v7 position state must be an object".into())
        })?;
        if ["board", "turn", "mode"]
            .iter()
            .any(|name| !state_fields.contains_key(*name))
        {
            return Err(EngineError::InvalidState(
                "v7 state requires board, turn and mode".into(),
            ));
        }
        if state_fields.contains_key("rng") || state_fields.contains_key("history") {
            return Err(EngineError::InvalidState(
                "v7 state must not duplicate outer RNG or history".into(),
            ));
        }
        if state_fields
            .get("rulesetId")
            .is_some_and(|version| version.as_str() != Some(RULES_VERSION_V7))
        {
            return Err(EngineError::InvalidState(
                "v7 state rulesetId conflicts with envelope rulesVersion".into(),
            ));
        }
        let rng_value = fields
            .get("rng")
            .ok_or_else(|| EngineError::InvalidState("v7 position RNG is missing".into()))?;
        let rng_fields = rng_value
            .as_object()
            .ok_or_else(|| EngineError::InvalidState("v7 position RNG must be an object".into()))?;
        if rng_fields.len() != 4
            || ["algorithm", "state", "tape", "cursor"]
                .iter()
                .any(|name| !rng_fields.contains_key(*name))
        {
            return Err(EngineError::InvalidState(
                "v7 RNG has missing or unexpected fields".into(),
            ));
        }
        let rng: RngState =
            serde_json::from_value(rng_value.clone()).map_err(EngineError::serialization)?;
        let history: Vec<Value> = serde_json::from_value(
            fields
                .get("history")
                .ok_or_else(|| EngineError::InvalidState("v7 history is missing".into()))?
                .clone(),
        )
        .map_err(EngineError::serialization)?;
        for (index, raw_event) in history.iter().enumerate() {
            let event: PublicEvent =
                serde_json::from_value(raw_event.clone()).map_err(|error| {
                    EngineError::InvalidState(format!("v7 history[{index}] is invalid: {error}"))
                })?;
            if event.protocol_version != "accelerate-game-event-v1"
                || event.actor != event.action.color
                || event.action.position_key.is_some()
                || [&event.public.white, &event.public.black]
                    .into_iter()
                    .any(|view| view.kind != "transition" || view.actor != event.actor)
            {
                return Err(EngineError::InvalidState(format!(
                    "v7 history[{index}] has inconsistent event identity"
                )));
            }
        }
        let mut state: GameState =
            serde_json::from_value(raw_state.clone()).map_err(EngineError::serialization)?;
        state.source_deck_slots_absent = !state_fields.contains_key("deckSlots");
        state.ruleset_id = RULES_VERSION_V7.into();
        state.rng = rng;
        state.history = history;
        state.validate_v7_snapshot_shape_and_identify()?;
        let spatial = SpatialState::from_v7_source(&state)?;
        let baseline_state = source_state_value(&state, state_fields.contains_key("rulesetId"))?;
        let position = Self {
            state: Arc::new(state),
            spatial: Arc::new(spatial),
            original_state: Arc::new(raw_state.clone()),
            baseline_state: Arc::new(baseline_state),
            position_id: actual_id,
            revision: 0,
            colossus_reservation: None,
        };
        if canonical_bytes(&position.export_envelope_unchecked()?)? != canonical_bytes(&envelope)? {
            return Err(EngineError::InvalidState(
                "v7 source snapshot cannot round-trip without loss".into(),
            ));
        }
        Ok(position)
    }

    /// Construct an envelope from separately owned source state and runtime
    /// metadata. This is the entry point for an explicit new-game result.
    pub fn from_parts(state: Value, rng: RngState, history: Vec<Value>) -> Result<Self> {
        if rng.has_source_trace() {
            return Err(EngineError::InvalidState(
                "v7 constructor has an unsettled source prior trace".into(),
            ));
        }
        validate_json_value(&state, 0)?;
        let content = json!({
            "protocolVersion": V7_POSITION_PROTOCOL,
            "rulesVersion": RULES_VERSION_V7,
            "catalogVersion": v7_catalog_version()?,
            "state": state,
            "rng": rng,
            "history": history,
        });
        let mut envelope = content
            .as_object()
            .expect("position content is object")
            .clone();
        envelope.insert("positionId".into(), Value::String(digest(&content)?));
        Self::from_envelope(Value::Object(envelope))
    }

    /// Explicitly normalize a typed state into a source v7 envelope. Callers
    /// importing a raw source snapshot should use `from_envelope` instead.
    pub fn from_state(state: GameState) -> Result<Self> {
        if state.ruleset_id != RULES_VERSION_V7 {
            return Err(EngineError::InvalidState(
                "typed v7 host state requires the v7 rules version".into(),
            ));
        }
        let mut raw = serde_json::to_value(&state).map_err(EngineError::serialization)?;
        let object = raw.as_object_mut().expect("GameState serializes to object");
        object.shift_remove("rulesetId");
        object.shift_remove("rng");
        object.shift_remove("history");
        Self::from_parts(raw, state.rng, state.history)
    }

    pub fn state(&self) -> &GameState {
        &self.state
    }

    pub fn spatial(&self) -> &SpatialState {
        &self.spatial
    }

    pub fn position_id(&self) -> &str {
        &self.position_id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// 실제 이동에서 예약한 callback만 얻는다. 저장 Position의 turnResolving만으로
    /// callback 권한을 복원하지 않으며 headless profile은 이를 자동 실행하지 않는다.
    pub fn pending_colossus_completion(&self) -> Option<V7ColossusCompletion> {
        let actor = self.state.pending_colossus_actor?;
        let reservation = self.colossus_reservation.as_ref()?;
        let resolving_state = reservation.resolving_state.upgrade()?;
        if reservation.actor != actor || !Arc::ptr_eq(&self.state, &resolving_state) {
            return None;
        }
        reservation
            .issued
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(V7ColossusCompletion {
            scheduled_state: reservation.resolving_state.clone(),
            actor,
        })
    }

    /// 소유 ticket을 소비한다. 새 Position/import/종료된 owner에 대한 callback은
    /// 원문처럼 실행하지 않는다. 성공한 callback의 상태·난수·이력을 원자적으로 반영한다.
    pub fn complete_colossus(&self, ticket: V7ColossusCompletion) -> Result<Option<Self>> {
        let Some(scheduled) = ticket.scheduled_state.upgrade() else {
            return Ok(None);
        };
        if !Arc::ptr_eq(&self.state, &scheduled)
            || self.state.pending_colossus_actor != Some(ticket.actor)
        {
            return Ok(None);
        }
        let (next, ()) = self.transact(&self.position_id, |working| {
            let outcome = crate::v7_move_execution::complete_colossus_turn(working, ticket.actor)?;
            if let crate::v7_move_execution::StationaryCompletion::EndMove { actor, .. } =
                outcome.completion
            {
                crate::transition::end_move_for_decision(working, actor, true, Some("move"))?;
            }
            crate::replay::settle(working)?;
            Ok(())
        })?;
        Ok(Some(next))
    }

    pub fn export_envelope(&self) -> Result<Value> {
        let envelope = self.export_envelope_unchecked()?;
        if envelope.get("positionId").and_then(Value::as_str) != Some(&self.position_id) {
            return Err(EngineError::InvalidState(
                "v7 host position identity changed without a transaction".into(),
            ));
        }
        Ok(envelope)
    }

    /// Execute all direct effects, automatic effects, settlement and event
    /// construction on one owned copy. Neither a failed callback nor a failed
    /// postcondition can change the original state, RNG or history.
    pub fn transact<T, F>(&self, expected_position_id: &str, operation: F) -> Result<(Self, T)>
    where
        F: FnOnce(&mut GameState) -> Result<T>,
    {
        if self.position_id != expected_position_id {
            return Err(EngineError::StaleAction);
        }
        let mut working = self.state.as_ref().clone();
        let result = operation(&mut working)?;
        let next = self.commit_working(expected_position_id, working)?;
        Ok((next, result))
    }

    /// Commit a caller-owned working copy. A registry host can begin its
    /// transaction with one clone and hand that exact state back here after
    /// all callbacks, budgets and cancellation checks have succeeded.
    pub fn commit_working(&self, expected_position_id: &str, working: GameState) -> Result<Self> {
        self.try_commit_working(expected_position_id, working)
            .map_err(|(error, _working)| error)
    }

    /// Keep ownership of the staged state with the caller if commit fails.
    /// The adapter runtime can then invoke its rollback path without cloning
    /// the transaction solely to satisfy its failure contract.
    pub fn try_commit_working(
        &self,
        expected_position_id: &str,
        mut working: GameState,
    ) -> std::result::Result<Self, (EngineError, Box<GameState>)> {
        if self.position_id != expected_position_id {
            return Err((EngineError::StaleAction, Box::new(working)));
        }
        let Some(revision) = self
            .revision
            .checked_add(1)
            .filter(|value| *value <= MAX_SAFE_REVISION)
        else {
            return Err((
                EngineError::InvalidState("v7 host revision overflow".into()),
                Box::new(working),
            ));
        };
        if working.gameover_replay_pending
            || working.semantic_chance_probability.is_some()
            || working.rng.has_source_trace()
            || working.move_replay_scope.is_some()
        {
            return Err((
                EngineError::InvalidState(
                    "v7 transaction has unsettled execution-only state".into(),
                ),
                Box::new(working),
            ));
        }
        if let Err(error) = working.validate_v7_snapshot_shape_and_identify() {
            return Err((error, Box::new(working)));
        }
        let spatial = match SpatialState::from_v7_source(&working)
            .and_then(|spatial| spatial.with_host_revision(revision))
        {
            Ok(spatial) => spatial,
            Err(error) => return Err((error, Box::new(working))),
        };
        let pending_actor = working.pending_colossus_actor;
        let state = Arc::new(working);
        let colossus_reservation = pending_actor.map(|actor| {
            self.colossus_reservation
                .as_ref()
                .filter(|reservation| reservation.actor == actor)
                .map(Arc::clone)
                .unwrap_or_else(|| {
                    Arc::new(ColossusReservation {
                        resolving_state: Arc::downgrade(&state),
                        actor,
                        issued: AtomicBool::new(false),
                    })
                })
        });
        let mut next = Self {
            state,
            spatial: Arc::new(spatial),
            original_state: Arc::clone(&self.original_state),
            baseline_state: Arc::clone(&self.baseline_state),
            position_id: String::new(),
            revision,
            colossus_reservation,
        };
        let finalization = (|| -> Result<String> {
            let envelope = next.export_envelope_unchecked()?;
            // A source-shape overlay is accepted only if a fresh import
            // recovers the exact typed state. This catches fields lost through
            // defaults or array replacements before commit becomes visible.
            let reimported = Self::from_envelope(envelope.clone())?;
            if canonical_bytes(
                &serde_json::to_value(&*reimported.state).map_err(EngineError::serialization)?,
            )? != canonical_bytes(
                &serde_json::to_value(&*next.state).map_err(EngineError::serialization)?,
            )? {
                return Err(EngineError::InvalidState(
                    "v7 transaction did not survive source snapshot round-trip".into(),
                ));
            }
            Ok(envelope["positionId"]
                .as_str()
                .expect("generated positionId is text")
                .to_owned())
        })();
        match finalization {
            Ok(position_id) => {
                next.position_id = position_id;
                Ok(next)
            }
            Err(error) => Err((
                error,
                Box::new(Arc::into_inner(next.state).expect("new transaction state has one owner")),
            )),
        }
    }

    fn export_envelope_unchecked(&self) -> Result<Value> {
        let typed = source_state_value(
            &self.state,
            self.original_state
                .as_object()
                .expect("validated source state")
                .contains_key("rulesetId"),
        )?;
        let mut state = restore_source_shape(&self.original_state, &self.baseline_state, &typed);
        restore_large_piece_aliases(&mut state, &self.baseline_state, &self.state);
        validate_json_value(&state, 0)?;
        let content = json!({
            "protocolVersion": V7_POSITION_PROTOCOL,
            "rulesVersion": RULES_VERSION_V7,
            "catalogVersion": v7_catalog_version()?,
            "state": state,
            "rng": self.state.rng,
            "history": self.state.history,
        });
        let mut envelope = content
            .as_object()
            .expect("position content is object")
            .clone();
        envelope.insert("positionId".into(), Value::String(digest(&content)?));
        Ok(Value::Object(envelope))
    }
}

fn source_state_value(state: &GameState, retain_ruleset_id: bool) -> Result<Value> {
    let mut value = serde_json::to_value(state).map_err(EngineError::serialization)?;
    let object = value
        .as_object_mut()
        .expect("GameState serializes to object");
    object.shift_remove("rng");
    object.shift_remove("history");
    if !retain_ruleset_id {
        object.shift_remove("rulesetId");
    }
    Ok(value)
}

/// Preserve an imported field exactly when its typed value has not changed.
/// Array slots containing a different identity are replacements, so fields
/// from the old piece/card cannot leak into the new one. An updated piece
/// anchor also replaces the slot: an overlapping translated footprint must
/// not mix the old slot's source ordering with its newly occupied slots.
fn restore_source_shape(original: &Value, baseline: &Value, current: &Value) -> Value {
    if baseline == current {
        return original.clone();
    }
    match (original, baseline, current) {
        (Value::Object(old), Value::Object(base), Value::Object(now)) => {
            let identity_changed = ["id", "instanceId"].into_iter().any(|name| {
                base.get(name).is_some()
                    && now.get(name).is_some()
                    && base.get(name) != now.get(name)
            });
            let anchor_changed = ["type", "color", "id"]
                .into_iter()
                .all(|name| base.contains_key(name) && now.contains_key(name))
                && ["anchorRow", "anchorCol"]
                    .into_iter()
                    .any(|name| base.get(name) != now.get(name));
            if identity_changed || anchor_changed {
                return current.clone();
            }
            let mut output: Map<String, Value> = old.clone();
            for name in base.keys() {
                if !now.contains_key(name) {
                    output.shift_remove(name);
                }
            }
            for (name, value) in now {
                if let Some(previous) = base.get(name) {
                    if previous != value {
                        output.insert(
                            name.clone(),
                            old.get(name)
                                .map(|raw| restore_source_shape(raw, previous, value))
                                .unwrap_or_else(|| value.clone()),
                        );
                    }
                } else {
                    output.insert(name.clone(), value.clone());
                }
            }
            Value::Object(output)
        }
        (Value::Array(old), Value::Array(base), Value::Array(now))
            if old.len() == base.len() && base.len() == now.len() =>
        {
            Value::Array(
                old.iter()
                    .zip(base)
                    .zip(now)
                    .map(|((raw, previous), value)| restore_source_shape(raw, previous, value))
                    .collect(),
            )
        }
        _ => current.clone(),
    }
}

/// A large piece is one source object repeated across its board cells. The
/// per-cell source overlay may otherwise retain omitted fields on overlapping
/// old cells while giving newly occupied cells the full typed representation.
fn restore_large_piece_aliases(state: &mut Value, baseline: &Value, current: &GameState) {
    let Some(board) = state.get_mut("board").and_then(Value::as_array_mut) else {
        return;
    };
    let mut aliases = std::collections::BTreeMap::<String, (bool, Value)>::new();
    for (row, cells) in current.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            let Some(piece) = piece.as_ref().filter(|piece| piece.is_large()) else {
                continue;
            };
            let Some(alias) = board.get(row).and_then(|line| line.get(col)) else {
                continue;
            };
            let retained = baseline["board"][row][col]["id"] == piece.id;
            let entry = aliases
                .entry(piece.id.clone())
                .or_insert_with(|| (retained, alias.clone()));
            if retained && !entry.0 {
                *entry = (true, alias.clone());
            }
        }
    }
    for (row, cells) in current.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            let Some(piece) = piece.as_ref().filter(|piece| piece.is_large()) else {
                continue;
            };
            if let Some((_, alias)) = aliases.get(&piece.id) {
                let mut alias = alias.clone();
                if alias.get("id").is_none()
                    && let (Some(anchor_row), Some(anchor_col)) = (
                        piece.extra.get("anchorRow").and_then(Value::as_u64),
                        piece.extra.get("anchorCol").and_then(Value::as_u64),
                    )
                    && piece.id != format!("{}-{anchor_row}-{anchor_col}", piece.color.as_str())
                {
                    alias
                        .as_object_mut()
                        .expect("source piece is an object")
                        .insert("id".into(), Value::String(piece.id.clone()));
                }
                board[row][col] = alias;
            }
        }
    }
}

fn v7_catalog_version() -> Result<String> {
    crate::v7_execution_profile::catalog_version()
}

fn canonical_bytes(value: &Value) -> Result<Vec<u8>> {
    serde_jcs::to_vec(value).map_err(|error| EngineError::Serialization(error.to_string()))
}

fn digest(value: &Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(canonical_bytes(value)?)))
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    #[test]
    fn translated_large_aliases_survive_canonical_source_shape_round_trip() {
        for (source_large, destination_row) in [(true, 2), (true, 1), (false, 3)] {
            for explicit_id in [false, true] {
                let mut piece = json!({
                    "type":if source_large { "bigRook" } else { "rook" },
                    "color":"white", "moved":false, "unlistedPieceProperty":{"active":true},
                });
                if source_large {
                    piece["anchorRow"] = json!(3);
                    piece["anchorCol"] = json!(1);
                }
                if explicit_id {
                    piece["id"] = json!("test-large-rook");
                }
                let mut board = vec![vec![Value::Null; 8]; 8];
                for row in &mut board[3..if source_large { 5 } else { 4 }] {
                    for cell in &mut row[1..if source_large { 3 } else { 2 }] {
                        *cell = piece.clone();
                    }
                }
                let original = V7HostPosition::from_parts(
                    json!({"board":board, "turn":"white", "mode":"play"}),
                    RngState::seeded(19),
                    Vec::new(),
                )
                .unwrap();
                let before = original.export_envelope().unwrap();
                let (next, ()) = original
                    .transact(original.position_id(), |working| {
                        let mut moving = working.board[3][1].as_ref().unwrap().clone();
                        crate::transition::clear_piece(working, &moving.id);
                        moving.kind = "bigRook".into();
                        moving.moved = true;
                        moving
                            .extra
                            .insert("anchorRow".into(), json!(destination_row));
                        moving.extra.insert("anchorCol".into(), json!(1));
                        moving
                            .extra
                            .insert("coolGuyCapturedLast".into(), json!(false));
                        for row in destination_row..destination_row + 2 {
                            for col in 1..3 {
                                working.board[row][col] = Some(moving.clone());
                            }
                        }
                        // The public action host applies this same JCS boundary
                        // before commit. This isolates overlay/reimport from
                        // movement rules while retaining its exact key ordering.
                        crate::replay::canonicalize_position_frames(working)?;
                        working.validate_v7_snapshot_shape_and_identify()?;
                        Ok(())
                    })
                    .unwrap();
                assert_eq!(original.export_envelope().unwrap(), before);
                assert_eq!(next.revision(), 1);
                assert_eq!(next.spatial().pieces().len(), 1);
                let envelope = next.export_envelope().unwrap();
                let reimported = V7HostPosition::from_envelope(envelope).unwrap();
                let expected = next.state().board[destination_row][1].as_ref().unwrap();
                assert_eq!(expected.extra["unlistedPieceProperty"]["active"], true);
                assert_eq!(expected.extra["anchorRow"], json!(destination_row));
                assert!(expected.moved);
                for row in 0..8 {
                    for col in 0..8 {
                        if (destination_row..destination_row + 2).contains(&row)
                            && (1..3).contains(&col)
                        {
                            assert_eq!(reimported.state().board[row][col].as_ref(), Some(expected));
                        } else {
                            assert!(reimported.state().board[row][col].is_none());
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn colossus_callback_ticket_is_issued_once_across_host_clones_and_cannot_be_restored() {
        let source = V7HostPosition::from_parts(
            json!({
                "board": vec![vec![Value::Null; 8]; 8], "turn":"white", "mode":"play",
            }),
            RngState::seeded(19),
            Vec::new(),
        )
        .unwrap();
        let (pending, ()) = source
            .transact(source.position_id(), |state| {
                state.pending_colossus_actor = Some(Color::White);
                state.extra.insert("turnResolving".into(), json!(true));
                Ok(())
            })
            .unwrap();
        let envelope = pending.export_envelope().unwrap();
        let (replaced, ()) = pending
            .transact(pending.position_id(), |state| {
                state
                    .extra
                    .insert("unrelatedHostChange".into(), json!(true));
                Ok(())
            })
            .unwrap();
        assert!(
            replaced.pending_colossus_completion().is_none(),
            "a state replacement must not reschedule the old reservation"
        );
        let clone = pending.clone();
        let ticket = pending
            .pending_colossus_completion()
            .expect("one source reservation");
        assert!(pending.pending_colossus_completion().is_none());
        assert!(clone.pending_colossus_completion().is_none());
        assert_eq!(pending.export_envelope().unwrap(), envelope);
        let reimported = V7HostPosition::from_envelope(envelope).unwrap();
        assert!(reimported.pending_colossus_completion().is_none());
        assert!(reimported.complete_colossus(ticket).unwrap().is_none());
    }
}
