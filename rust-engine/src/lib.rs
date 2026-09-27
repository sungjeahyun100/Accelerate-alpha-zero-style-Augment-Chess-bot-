//! Independent rules and immutable snapshots. No language or inference runtime is used here.
mod conditioning;
mod draft;
mod flow;
mod movement;
mod state;
#[cfg(test)]
mod tests;
mod transition;

pub use movement::implemented_piece_types;
pub use state::*;

use serde_json::{Map, Value};
use std::sync::Arc;

#[derive(Clone, Debug)]
struct SnapshotShape {
    original: Map<String, Value>,
    baseline: Map<String, Value>,
}

#[derive(Clone, Debug)]
pub struct Position(Arc<GameState>, Option<Arc<SnapshotShape>>);

impl Position {
    pub(crate) fn with_state(&self, mut state: GameState) -> Result<Self> {
        state.validate_and_identify()?;
        Ok(Self(Arc::new(state), self.1.clone()))
    }
    pub fn sample_initial_public(
        config: GameConfig,
        observation: Value,
        seed: u32,
    ) -> Result<Self> {
        conditioning::sample_initial(config, observation, seed)
    }
    pub fn condition_public_identities(&self, observation: Value) -> Result<Self> {
        conditioning::condition_identities(self, observation)
    }
    pub fn public_intent(&self, action: &Action) -> Result<Value> {
        self.validate_action(action)?;
        let mut semantic = action.clone();
        semantic.position_key = None;
        if semantic.kind == ActionKind::Move {
            movement::public_move_intent(self.state(), &semantic)
        } else {
            serde_json::to_value(semantic).map_err(EngineError::serialization)
        }
    }
    pub fn bind_public_intent(&self, intent: Value) -> Result<Action> {
        state::validate_json_value(&intent, 0)?;
        if intent.get("type").and_then(Value::as_str) != Some("move") {
            return self.bind_payload(intent);
        }
        let mut action = movement::resolve_move_intent(self.state(), &intent)?;
        action.position_key = Some(format!("{:016x}", self.key()));
        Ok(action)
    }
    pub fn new_game(config: GameConfig, seed: u64) -> Result<Self> {
        let state = GameState::new(config, seed)?;
        let rng = state.rng.clone();
        let history = state.history.clone();
        let mut raw = serde_json::to_value(&state).map_err(EngineError::serialization)?;
        for name in ["rng", "history", "rulesetId"] {
            raw.as_object_mut().expect("state object").remove(name);
        }
        Self::from_snapshot_value(raw)?.with_metadata(rng, history)
    }
    pub fn from_state(mut state: GameState) -> Result<Self> {
        state.validate_and_identify()?;
        Ok(Self(Arc::new(state), None))
    }
    /// Import a source snapshot without inventing fields at its serialization
    /// boundary. Typed defaults remain internal until a rule changes them.
    pub fn from_snapshot_value(value: Value) -> Result<Self> {
        state::validate_json_value(&value, 0)?;
        let original = value
            .as_object()
            .ok_or_else(|| EngineError::InvalidState("snapshot state must be an object".into()))?
            .clone();
        let mut state: GameState =
            serde_json::from_value(value).map_err(EngineError::serialization)?;
        state.validate_and_identify()?;
        let baseline = serde_json::to_value(&state)
            .map_err(EngineError::serialization)?
            .as_object()
            .expect("state object")
            .clone();
        Ok(Self(
            Arc::new(state),
            Some(Arc::new(SnapshotShape { original, baseline })),
        ))
    }
    /// RNG and public history are position metadata in the v1 transport.
    pub fn with_metadata(&self, rng: RngState, history: Vec<Value>) -> Result<Self> {
        let mut state = self.state().clone();
        state.rng = rng;
        state.history = history;
        state.validate_and_identify()?;
        Ok(Self(Arc::new(state), self.1.clone()))
    }
    pub fn export_state(&self) -> Result<Value> {
        let current = serde_json::to_value(self.state()).map_err(EngineError::serialization)?;
        let Some(shape) = &self.1 else {
            return Ok(current);
        };
        let mut output = shape.original.clone();
        for name in shape.baseline.keys() {
            if !current
                .as_object()
                .expect("state object")
                .contains_key(name)
            {
                output.remove(name);
            }
        }
        for (name, value) in current.as_object().expect("state object") {
            if matches!(name.as_str(), "rulesetId" | "rng" | "history")
                && !shape.original.contains_key(name)
            {
                continue;
            }
            if shape.baseline.get(name) != Some(value) {
                output.insert(name.clone(), value.clone());
            }
        }
        Ok(Value::Object(output))
    }
    pub fn from_json(json: &str) -> Result<Self> {
        Self::from_snapshot_value(serde_json::from_str(json).map_err(EngineError::serialization)?)
    }
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(&self.export_state()?).map_err(EngineError::serialization)
    }
    pub fn state(&self) -> &GameState {
        &self.0
    }
    /// Player making the current decision. Board turn and decision actor differ in
    /// draft, promotion and reaction windows; search must use this value.
    pub fn actor(&self) -> Color {
        self.state().decision_actor()
    }
    pub fn decision_actor(&self) -> Color {
        self.actor()
    }
    pub fn key(&self) -> u64 {
        stable_hash(
            serde_json::to_string(self.state())
                .expect("validated state serializes")
                .as_bytes(),
        )
    }
    pub fn legal_actions(&self) -> Result<Vec<Action>> {
        let mut actions = movement::legal_actions(self.state())?;
        let key = format!("{:016x}", self.key());
        for action in &mut actions {
            action.position_key = Some(key.clone());
        }
        Ok(actions)
    }
    pub fn action_stream(&self) -> Result<ActionStream> {
        Ok(ActionStream {
            position: self.clone(),
            cursor: movement::ActionCursor::new(self.state())?,
        })
    }
    pub fn bind_payload(&self, payload: Value) -> Result<Action> {
        state::validate_json_value(&payload, 0)?;
        let mut action: Action =
            serde_json::from_value(payload).map_err(EngineError::serialization)?;
        if action.position_key.is_some() {
            return Err(EngineError::IllegalAction);
        }
        self.validate_action(&action)?;
        action.position_key = Some(format!("{:016x}", self.key()));
        Ok(action)
    }
    pub fn validate_action(&self, action: &Action) -> Result<()> {
        if action
            .position_key
            .as_ref()
            .is_some_and(|key| key != &format!("{:016x}", self.key()))
        {
            return Err(EngineError::StaleAction);
        }
        let mut semantic = action.clone();
        semantic.position_key = None;
        state::validate_json_value(
            &serde_json::to_value(&semantic).map_err(EngineError::serialization)?,
            0,
        )?;
        movement::validate_action(self.state(), &semantic)
    }
    pub fn apply(&self, action: &Action) -> Result<StepResult> {
        if let Some(key) = &action.position_key
            && key != &format!("{:016x}", self.key())
        {
            return Err(EngineError::StaleAction);
        }
        if action.color != self.actor() {
            return Err(EngineError::WrongActor);
        }
        let mut comparable = action.clone();
        comparable.position_key = None;
        self.validate_action(action)?;
        let actor = self.actor();
        let mut state = self.state().clone();
        let captures = transition::apply(&mut state, &comparable)?;
        state.validate_and_identify()?;
        let turn_changed = self.state().turn != state.turn;
        let result = state.result();
        Ok(StepResult {
            position: Self(Arc::new(state), self.1.clone()),
            actor,
            turn_changed,
            captures,
            result,
        })
    }
    pub fn result(&self) -> Option<GameResult> {
        self.state().result()
    }
    pub fn observe(&self, viewer: Color) -> Observation {
        self.state().observe(viewer)
    }
    pub fn try_observe(&self, viewer: Color) -> Result<Observation> {
        self.state().try_observe(viewer)
    }
}

pub struct ActionStream {
    position: Position,
    cursor: movement::ActionCursor,
}
#[derive(Clone, Debug)]
pub struct ActionPage {
    pub actions: Vec<Action>,
    pub exhausted: bool,
}
impl ActionStream {
    pub fn next_page(&mut self, limit: usize) -> Result<ActionPage> {
        if !(1..=4096).contains(&limit) {
            return Err(EngineError::InvalidConfig(
                "action page size must be in 1..=4096".into(),
            ));
        }
        let mut actions = Vec::with_capacity(limit);
        let key = format!("{:016x}", self.position.key());
        while actions.len() < limit && self.cursor.fill(self.position.state())? {
            let mut action = self.cursor.pop().expect("filled cursor");
            action.position_key = Some(key.clone());
            actions.push(action);
        }
        let exhausted = !self.cursor.fill(self.position.state())?;
        Ok(ActionPage { actions, exhausted })
    }
}

#[derive(Clone, Debug)]
pub struct StepResult {
    pub position: Position,
    pub actor: Color,
    pub turn_changed: bool,
    pub captures: Vec<Piece>,
    pub result: Option<GameResult>,
}

pub(crate) fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
