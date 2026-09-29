//! Independent rules and immutable snapshots. No language or inference runtime is used here.
mod card_constraints;
mod card_effects;
mod card_registry;
mod card_target_hints;
mod conditioning;
mod draft;
mod eligibility;
mod flow;
pub mod geometry;
pub mod move_program;
mod movement;
mod observation;
mod opening;
mod replay;
mod spatial_state;
mod state;
#[cfg(test)]
mod tests;
mod threat;
mod transition;
mod turn_effects_v7;
mod v7_action_surface;
mod variant_movement;

pub use geometry::{BoardGeometry, Coord, Offset};
pub use movement::implemented_piece_types;
pub use spatial_state::*;
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

/// Importance proposal for a previously hidden source OPENING offer.
/// Probabilities refer to the ordered weighted-choice chance kernel; opaque
/// identity draws are unchanged ancillary draws in both distributions.
#[derive(Clone, Debug)]
pub struct HiddenDraftProposal {
    pub position: Position,
    pub importance_weight: f64,
    pub source_probability: f64,
    pub proposal_probability: f64,
}

/// An importance proposal for a realized semantic chance trace. This metadata
/// is separate from the executed game's result and from opaque identity draws.
#[derive(Clone, Debug)]
pub struct ConditionedStepProposal {
    pub step: StepResult,
    pub importance_weight: f64,
    pub source_probability: f64,
    pub proposal_probability: f64,
}

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
    /// Inspect source draw predicates against an owned clone. Random predicate
    /// probes cannot advance this immutable position's future random stream.
    pub fn draft_availability(&self, color: Color) -> Result<Vec<(String, bool)>> {
        let mut state = self.state().clone();
        crate::draft::definitions_for_ruleset(&state.ruleset_id)?
            .definitions
            .iter()
            .map(|card| {
                Ok((
                    card["id"].as_str().expect("adopted card id").to_owned(),
                    crate::eligibility::draft_drawable(&mut state, card, color)?,
                ))
            })
            .collect()
    }
    pub fn condition_public_identities(&self, observation: Value) -> Result<Self> {
        conditioning::condition_identities(self, observation)
    }
    pub fn apply_conditioned_public(
        &self,
        action: &Action,
        expected_public_observation: Value,
        independent_seed: u32,
    ) -> Result<StepResult> {
        conditioning::apply_conditioned(self, action, expected_public_observation, independent_seed)
    }
    pub fn apply_weighted_conditioned_public(
        &self,
        action: &Action,
        expected_public_observation: Value,
        independent_seed: u32,
    ) -> Result<ConditionedStepProposal> {
        conditioning::apply_weighted_conditioned(
            self,
            action,
            expected_public_observation,
            independent_seed,
        )
    }
    pub fn public_transition_compatible(&self, action: &Action, expected: Value) -> Result<bool> {
        conditioning::transition_compatible(self, action, expected)
    }
    pub fn condition_hidden_opening_draft(
        &self,
        expected_next_public: Value,
        independent_seed: u32,
    ) -> Result<HiddenDraftProposal> {
        conditioning::hidden_opening(self, expected_next_public, independent_seed)
    }
    pub fn public_intent(&self, action: &Action) -> Result<Value> {
        self.validate_action(action)?;
        self.validate_public_selection(action)?;
        let mut semantic = action.clone();
        semantic.position_key = None;
        if semantic.kind == ActionKind::TrolleyChoice {
            return Ok(
                serde_json::json!({"type":"trolleyChoice","color":semantic.color,"doomedIndex":semantic.extra.get("doomedIndex")}),
            );
        }
        if semantic.kind == ActionKind::Move {
            movement::public_move_intent(self.state(), &semantic)
        } else {
            serde_json::to_value(semantic).map_err(EngineError::serialization)
        }
    }
    pub fn bind_public_intent(&self, intent: Value) -> Result<Action> {
        state::validate_json_value(&intent, 0)?;
        if intent.get("type").and_then(Value::as_str) == Some("trolleyChoice") {
            let fields = intent.as_object().ok_or(EngineError::IllegalAction)?;
            if fields.len() != 3
                || fields
                    .keys()
                    .any(|key| !matches!(key.as_str(), "type" | "color" | "doomedIndex"))
            {
                return Err(EngineError::IllegalAction);
            }
            let mut payload = intent;
            let window = self
                .state()
                .extra
                .get("activeTrolley")
                .and_then(|window| window.get("id"))
                .filter(|id| !id.is_null())
                .cloned()
                .ok_or(EngineError::IllegalAction)?;
            payload["windowId"] = window;
            return self.bind_payload(payload);
        }
        if intent.get("type").and_then(Value::as_str) != Some("move") {
            let action = self.bind_payload(intent)?;
            self.validate_public_selection(&action)?;
            return Ok(action);
        }
        let mut action = movement::resolve_move_intent(self.state(), &intent)?;
        action.position_key = Some(format!("{:016x}", self.key()));
        Ok(action)
    }
    fn validate_public_selection(&self, action: &Action) -> Result<()> {
        if action.kind == ActionKind::Card {
            let card = self
                .state()
                .deck_slots
                .get(action.color)
                .iter()
                .find(|card| Some(&card.instance_id) == action.card_instance_id.as_ref())
                .ok_or(EngineError::IllegalAction)?;
            let mut semantic = action.clone();
            semantic.position_key = None;
            let selected =
                if let Some(selected) = card_effects::ui_validate(self.state(), card, &semantic)? {
                    selected
                } else {
                    transition::card_ui_actions(self.state(), card)?.contains(&semantic)
                };
            if !selected {
                return Err(EngineError::IllegalAction);
            }
        }
        Ok(())
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
        Self::from_snapshot_value_with_rules_version(value, RULES_VERSION_V6)
    }
    /// Select the source rules version from the outer Position envelope, not
    /// from card metadata or an absent state.rulesetId. v7 is recognized but
    /// cannot create an executable Position until its rules are ported.
    pub fn from_snapshot_value_with_rules_version(
        value: Value,
        rules_version: &str,
    ) -> Result<Self> {
        if !matches!(rules_version, RULES_VERSION_V6 | RULES_VERSION_V7) {
            return Err(EngineError::InvalidConfig("unknown rules version".into()));
        }
        state::validate_json_value(&value, 0)?;
        let original = value
            .as_object()
            .ok_or_else(|| EngineError::InvalidState("snapshot state must be an object".into()))?
            .clone();
        if original
            .get("rulesetId")
            .is_some_and(|version| version.as_str() != Some(rules_version))
        {
            return Err(EngineError::InvalidState(
                "state rulesetId does not match Position rulesVersion".into(),
            ));
        }
        if rules_version == RULES_VERSION_V7 {
            return Err(EngineError::UnsupportedFeature(
                "v7 rules profile is not executable".into(),
            ));
        }
        let mut state: GameState =
            serde_json::from_value(value).map_err(EngineError::serialization)?;
        state.ruleset_id = rules_version.into();
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
                output.shift_remove(name);
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
    pub fn rules_version(&self) -> &str {
        &self.state().ruleset_id
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
            serde_jcs::to_vec(self.state())
                .expect("validated state serializes")
                .as_slice(),
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
        replay::canonicalize_position_frames(&mut state)?;
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
    /// Raw candidates and scanned slots consumed in this page. Rejected
    /// ordered selections count even when `actions` is empty.
    pub examined: usize,
}
impl ActionStream {
    pub fn next_page(&mut self, limit: usize) -> Result<ActionPage> {
        if !(1..=4096).contains(&limit) {
            return Err(EngineError::InvalidConfig(
                "action page size must be in 1..=4096".into(),
            ));
        }
        let mut actions = Vec::with_capacity(limit);
        let mut examined = 0;
        let mut cursor = self.cursor.clone();
        let key = format!("{:016x}", self.position.key());
        while examined < limit && !cursor.is_exhausted(self.position.state()) {
            let work = cursor.examine(self.position.state(), limit - examined)?;
            if let Some(page) = work.staged {
                examined += page.examined;
                actions.extend(page.actions.into_iter().map(|mut action| {
                    action.position_key = Some(key.clone());
                    action
                }));
            } else {
                examined += 1;
                if let Some(mut action) = work.action {
                    action.position_key = Some(key.clone());
                    actions.push(action);
                }
            }
        }
        let exhausted = cursor.is_exhausted(self.position.state());
        if actions.len() > examined || examined > limit || !exhausted && examined == 0 {
            return Err(EngineError::InvalidState(
                "action page examination budget invariant".into(),
            ));
        }
        self.cursor = cursor;
        Ok(ActionPage {
            actions,
            exhausted,
            examined,
        })
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
