//! Source-shaped v7 host actions and resumable public choices.
//!
//! This is the game project's ownership boundary between public intent and
//! state-changing rules. Scalar binding proves the selected source candidate
//! without exhausting a combinatorial family. The bound proof is checked again
//! against the current full Position and revision before an atomic transition.
//! Source move flags and trolley-window identities stay inside this boundary.
//! An incomplete rule owner fails explicitly instead of exposing a partial set.

use crate::v7_action_admission::{
    AdmissionError, AdmittedV7Action, V7_ACTION_PROTOCOL, VerifiedV7ActionSet, admit_v7_action,
};
use crate::{
    Action, ActionKind, Color, EngineError, GameResult, Piece, RULES_VERSION_V7, V7HostPosition,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, VecDeque};

const MAX_LEGAL_ACTIONS: usize = 4096;

#[derive(Debug)]
pub enum V7ActionHostError {
    Admission(AdmissionError),
    Engine(EngineError),
}

impl std::fmt::Display for V7ActionHostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admission(error) => write!(formatter, "{error}"),
            Self::Engine(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for V7ActionHostError {}

impl From<AdmissionError> for V7ActionHostError {
    fn from(error: AdmissionError) -> Self {
        Self::Admission(error)
    }
}

impl From<EngineError> for V7ActionHostError {
    fn from(error: EngineError) -> Self {
        Self::Engine(error)
    }
}

pub type V7ActionHostResult<T> = std::result::Result<T, V7ActionHostError>;

#[derive(Debug)]
pub struct AppliedV7Action {
    pub position: V7HostPosition,
    pub actor: Color,
    pub turn_changed: bool,
    pub captures: Vec<Piece>,
    pub result: Option<GameResult>,
    pub event: Value,
    pub action_id: String,
}

#[derive(Debug)]
pub struct V7PublicActionPage {
    pub intents: Vec<Value>,
    /// Counts source candidates, including rejected selections. Public move
    /// display aliases may add results without examining another candidate.
    pub examined: usize,
    pub exhausted: bool,
    pub stop_reason: &'static str,
}

/// An owned public projection of the frozen source action stream. The cursor
/// remains in the host; transport must expose only an opaque cursor token.
#[derive(Clone)]
pub struct V7PublicActionCursor {
    position_id: String,
    revision: u64,
    source: crate::v7_action_surface::SourceActionCursor,
    pending_public: VecDeque<Value>,
    // Only move aliases can collapse distinct private descriptors to the
    // same public click. Bound this set by the source movement-family limit
    // rather than retaining every card selection in a combinatorial stream.
    seen_movement: BTreeSet<Vec<u8>>,
}

impl std::fmt::Debug for V7PublicActionCursor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("V7PublicActionCursor")
            .field("revision", &self.revision)
            .field("pending_results", &self.pending_public.len())
            .field("exhausted", &self.source.is_exhausted())
            .finish_non_exhaustive()
    }
}

impl V7PublicActionCursor {
    pub fn new(position: &V7HostPosition, card_filter: Option<String>) -> V7ActionHostResult<Self> {
        require_v7(position)?;
        Ok(Self {
            position_id: position.position_id().to_owned(),
            revision: position.revision(),
            source: crate::v7_action_surface::SourceActionCursor::new(
                position.state(),
                card_filter,
            )?,
            pending_public: VecDeque::new(),
            seen_movement: BTreeSet::new(),
        })
    }

    pub fn next_page(
        &mut self,
        position: &V7HostPosition,
        limit: usize,
        max_examined: usize,
    ) -> V7ActionHostResult<V7PublicActionPage> {
        require_v7(position)?;
        if self.position_id != position.position_id() || self.revision != position.revision() {
            return Err(EngineError::StaleAction.into());
        }
        if !(1..=crate::v7_action_surface::MAX_PAGE_ACTIONS).contains(&limit)
            || !(1..=crate::v7_action_surface::MAX_PAGE_EXAMINED).contains(&max_examined)
        {
            return Err(EngineError::InvalidConfig(
                "v7 action page size must be 1..4096 and maxExamined must be 1..65536".into(),
            )
            .into());
        }
        let mut staged = self.clone();
        let mut intents = Vec::with_capacity(limit);
        let mut examined = 0;
        loop {
            while intents.len() < limit {
                let Some(intent) = staged.pending_public.pop_front() else {
                    break;
                };
                intents.push(intent);
            }
            if intents.len() == limit || examined == max_examined {
                break;
            }
            let Some(action) = staged.source.next_candidate()? else {
                break;
            };
            examined += 1;
            if !staged.source.accepts(&action)? {
                continue;
            }
            for intent in public_intents_for_source_action(staged.source.state(), &action)? {
                if action.kind == ActionKind::Move {
                    let canonical =
                        serde_jcs::to_vec(&intent).map_err(EngineError::serialization)?;
                    if !staged.seen_movement.insert(canonical) {
                        continue;
                    }
                }
                staged.pending_public.push_back(intent);
            }
        }
        let exhausted = staged.source.is_exhausted() && staged.pending_public.is_empty();
        let stop_reason = if exhausted {
            "exhausted"
        } else if intents.len() == limit {
            "page-limit"
        } else {
            "examined-budget"
        };
        *self = staged;
        Ok(V7PublicActionPage {
            intents,
            examined,
            exhausted,
            stop_reason,
        })
    }
}

fn require_v7(position: &V7HostPosition) -> crate::Result<()> {
    if position.state().ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 action host requires the pinned v7 rules version".into(),
        ));
    }
    Ok(())
}

fn require_decision(position: &V7HostPosition) -> crate::Result<()> {
    require_v7(position)?;
    if position.state().result().is_some() || position.state().mode == "gameover" {
        return Err(EngineError::Terminal);
    }
    if !matches!(position.state().mode.as_str(), "draft" | "play") {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 public action host for {} mode",
            position.state().mode
        )));
    }
    Ok(())
}

fn source_action_id(payload: &Value) -> crate::Result<String> {
    let canonical = serde_jcs::to_vec(payload).map_err(EngineError::serialization)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

fn source_envelope(position: &V7HostPosition, payload: Value) -> crate::Result<Value> {
    Ok(json!({
        "protocolVersion": V7_ACTION_PROTOCOL,
        "positionId": position.position_id(),
        "actionId": source_action_id(&payload)?,
        "payload": payload,
    }))
}

/// Exhaust the complete ordered legal action set for one v7 Position.
/// Source envelopes contain a full-state-derived Position ID and remain inside
/// the game host. A first-play movement subset is never returned as complete
/// legal actions.
pub(crate) fn legal_action_envelopes(position: &V7HostPosition) -> V7ActionHostResult<Vec<Value>> {
    require_v7(position)?;
    if position.state().result().is_some() || position.state().mode == "gameover" {
        return Ok(Vec::new());
    }
    require_decision(position)?;
    let verified = VerifiedV7ActionSet::complete(position)?;
    verified
        .source_entries()
        .map(|(_, payload, _)| source_envelope(position, payload.clone()).map_err(Into::into))
        .collect()
}

fn public_intents_for_source_action(
    state: &crate::GameState,
    action: &Action,
) -> crate::Result<Vec<Value>> {
    if action.position_key.is_some() || action.color != state.decision_actor() {
        return Err(EngineError::InvalidState(
            "v7 public candidate has a private binding or wrong decision actor".into(),
        ));
    }
    match action.kind {
        ActionKind::Move => {
            match crate::v7_action_surface::quantum_ui_public_intents(state, action)? {
                Some(intents) => Ok(intents),
                None => crate::movement::v7_public_move_intents(state, action),
            }
        }
        ActionKind::TrolleyChoice => Ok(vec![json!({
            "type": "trolleyChoice", "color": action.color,
            "doomedIndex": action.extra.get("doomedIndex"),
        })]),
        _ => Ok(vec![
            serde_json::to_value(action).map_err(EngineError::serialization)?,
        ]),
    }
}

/// The exact ordered public choices that may be shown to the decision maker.
/// Source action IDs and full-state-derived Position IDs stay in the host.
pub fn legal_public_intents(position: &V7HostPosition) -> V7ActionHostResult<Vec<Value>> {
    require_v7(position)?;
    if position.state().result().is_some() || position.state().mode == "gameover" {
        return Ok(Vec::new());
    }
    require_decision(position)?;
    let verified = VerifiedV7ActionSet::complete(position)?;
    let mut intents = Vec::new();
    let mut seen = BTreeSet::new();
    for (action, _, _) in verified.source_entries() {
        for intent in public_intents_for_source_action(position.state(), action)? {
            let canonical = serde_jcs::to_vec(&intent).map_err(EngineError::serialization)?;
            if seen.insert(canonical) {
                if intents.len() >= MAX_LEGAL_ACTIONS {
                    return Err(EngineError::UnsupportedFeature(format!(
                        "v7 public choices exceed the eager limit of {MAX_LEGAL_ACTIONS}; use the action cursor"
                    )).into());
                }
                intents.push(intent);
            }
        }
    }
    Ok(intents)
}

/// Bind a public intent. The caller supplies only public coordinates and
/// choices, without Position/action IDs; the host constructs those identities
/// and compares the selected payload with its source-owned public projection.
/// Array order, absent fields and extra fields are therefore significant.
pub fn bind_public_intent(
    position: &V7HostPosition,
    intent: Value,
) -> V7ActionHostResult<AdmittedV7Action> {
    require_decision(position)?;
    crate::state::validate_json_value(&intent, 0)?;
    let color: Color = serde_json::from_value(
        intent
            .get("color")
            .cloned()
            .ok_or(EngineError::IllegalAction)?,
    )
    .map_err(EngineError::serialization)?;
    if color != position.state().decision_actor() {
        return Err(EngineError::WrongActor.into());
    }
    let payload = match intent.get("type").and_then(Value::as_str) {
        Some("move") => {
            let action = crate::v7_action_surface::resolve_public_move(position.state(), &intent)?;
            serde_json::to_value(&action).map_err(EngineError::serialization)?
        }
        Some("trolleyChoice") => {
            let fields = intent.as_object().ok_or(EngineError::IllegalAction)?;
            if fields.len() != 3
                || fields
                    .keys()
                    .any(|key| !matches!(key.as_str(), "type" | "color" | "doomedIndex"))
            {
                return Err(EngineError::IllegalAction.into());
            }
            let mut payload = intent.clone();
            let window_id = position
                .state()
                .extra
                .get("activeTrolley")
                .and_then(|window| window.get("id"))
                .filter(|id| !id.is_null())
                .cloned()
                .ok_or(EngineError::IllegalAction)?;
            payload["windowId"] = window_id;
            payload
        }
        _ => intent.clone(),
    };
    let admitted = admit_v7_action(position, source_envelope(position, payload)?)?;
    if !public_intents_for_source_action(position.state(), admitted.action())?.contains(&intent) {
        return Err(EngineError::IllegalAction.into());
    }
    Ok(admitted)
}

/// Revalidate the complete current Position and apply one supported action on
/// an owned working copy. A failure never changes the caller's state, RNG or
/// history. The committed host Position contains the full next source-shaped
/// state and exactly one new public event.
pub fn apply_admitted(
    position: &V7HostPosition,
    admitted: &AdmittedV7Action,
) -> V7ActionHostResult<AppliedV7Action> {
    require_decision(position)?;
    admitted.revalidate(position)?;
    let projected = serde_json::to_value(admitted.action()).map_err(EngineError::serialization)?;
    if admitted.source_payload() != &projected {
        return Err(EngineError::IllegalAction.into());
    }
    let actor = position.state().decision_actor();
    let old_turn = position.state().turn;
    let old_history_len = position.state().history.len();
    let action: Action = admitted.action().clone();
    let (next, captures) = position.transact(position.position_id(), |working| {
        execute_on_working(working, &action, actor, old_history_len)
    })?;
    let event = next.state().history.last().cloned().ok_or_else(|| {
        EngineError::InvalidState("v7 host transition omitted its public event".into())
    })?;
    if event.get("action") != Some(admitted.source_payload())
        || event.get("actor") != Some(&json!(actor))
    {
        return Err(EngineError::InvalidState(
            "v7 host event differs from the admitted source action".into(),
        )
        .into());
    }
    let result = next.state().result();
    Ok(AppliedV7Action {
        turn_changed: old_turn != next.state().turn,
        position: next,
        actor,
        captures,
        result,
        event,
        action_id: admitted.action_id().to_owned(),
    })
}

/// raw 적용과 공개 조건부 제안이 같은 전이·정산·history 계약을 사용한다.
pub(crate) fn execute_on_working(
    working: &mut crate::GameState,
    action: &Action,
    actor: Color,
    old_history_len: usize,
) -> crate::Result<Vec<crate::Piece>> {
    if !matches!(working.mode.as_str(), "draft" | "play") || working.decision_actor() != actor {
        return Err(EngineError::StaleAction);
    }
    let captures = crate::transition::apply(working, action)?;
    crate::replay::canonicalize_position_frames(working)?;
    if working.history.len() != old_history_len + 1 {
        return Err(EngineError::InvalidState(
            "v7 host transition must append exactly one event".into(),
        ));
    }
    Ok(captures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;
    use serde_json::json;

    fn draft_host(style: &str, seed: u64) -> V7HostPosition {
        let state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            seed,
            RULES_VERSION_V7,
        )
        .unwrap();
        V7HostPosition::from_state(state).unwrap()
    }

    #[test]
    fn full_draft_legal_actions_have_ordered_source_payloads_and_identity() {
        // Generated with the frozen client SHA e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c:
        // newGame({gameStyle}, seed), then the complete adapter.actions(Position).
        for (style, seed, expected_count, expected_ordered_digest, expected_first_id) in [
            (
                "normal",
                19,
                3,
                "2e9477cc1f50bc16ddbbb6de10fc6a1f7a838cac1de300be27a1ee851aca1d69",
                "dc30f0af0259dc2c3cf7cdb5f97c992a55d7562b79c087e5eab0896e771f79ad",
            ),
            (
                "chaos",
                19,
                3,
                "973b4d47e1dac28d46364a65df92ba7c6b2ab784b70eab329d2b75b9c8118fdf",
                "feed8e68ca6e44c0d0560be2d9ae575b6f398bd40aa22ff82e835d09c6765c6c",
            ),
            (
                "grand",
                19,
                28,
                "8c084b5d22965a5cafcdcf77866d2abda35488b0c308f80735a0cc925282da27",
                "f4203369d835173d1d97bf81b0ace3be6c81951bd10913d12751455cb2ead8e6",
            ),
            (
                "normal",
                37,
                3,
                "ba5484f8cb72a238b8297d51e6ae2821a69903b863f7cb2bf9b3c0cf4c28692e",
                "e17f8bb215a4822b21a63e3e436429399febddbe74093ac50bd2deee8225d3d8",
            ),
            (
                "chaos",
                37,
                3,
                "ddf03a0e97f4bdcb1b923baa80f0ddeb864eaf0d4695aaa1cf1497e1cfa59593",
                "26e373bcf78948a027db09f61479ea1ccbcdc28f61b25ecc8a87c7f3d5dbfc3b",
            ),
            (
                "grand",
                37,
                28,
                "242631d70cc7665155c09c2435b908bcc5cf8f0dd6d67529917fda9c45d2f319",
                "fd69985ac3198ae0a86fed6b1af448a5072d2dcd565e29f07c74f75fc4987220",
            ),
        ] {
            let host = draft_host(style, seed);
            let source = crate::draft::legal_actions(host.state()).unwrap();
            let legal = legal_action_envelopes(&host).unwrap();
            assert_eq!(legal.len(), source.len(), "{style}");
            assert_eq!(legal.len(), expected_count, "{style}");
            let ordered_payloads = Value::Array(
                legal
                    .iter()
                    .map(|candidate| candidate["payload"].clone())
                    .collect(),
            );
            assert_eq!(
                legal_public_intents(&host).unwrap(),
                ordered_payloads.as_array().unwrap().clone(),
                "{style} public intents must omit private action/Position IDs"
            );
            assert_eq!(
                source_action_id(&ordered_payloads).unwrap(),
                expected_ordered_digest,
                "{style} complete source draft order"
            );
            assert_eq!(legal[0]["actionId"], expected_first_id, "{style}");
            for (candidate, action) in legal.iter().zip(source.iter()) {
                assert_eq!(candidate["positionId"], host.position_id());
                assert_eq!(candidate["payload"], serde_json::to_value(action).unwrap());
                assert_eq!(
                    candidate["actionId"],
                    source_action_id(&candidate["payload"]).unwrap()
                );
            }
        }
    }

    #[test]
    fn first_draft_apply_matches_frozen_full_state_rng_history_and_position_identity() {
        // Pinned source: adapter.apply(newGame({gameStyle}, seed), actions()[0],
        // {recordHistory:true}). The four digests cover the whole source
        // Position, state, RNG and complete public history, respectively.
        for (style, seed, expected_position_id, expected_state, expected_rng, expected_history) in [
            (
                "normal",
                19,
                "c2d6363f0316263ab9d4119a859b61c798a4c857aafb7328e97edde48b4bd0ee",
                "5104a2ffe1b932cbbaa58d38f1ee138f56f7f5c4f5058e16d7a9ecf9e3ed0219",
                "a52d7ca8e7f87ca33d7e97a5b0187574af29ed5460a45745d536548a8d9be178",
                "56f8c54c6d1262d77f1e8c2734e13a595eacdb5722089e98d8d37fe4f0c02878",
            ),
            (
                "chaos",
                19,
                "d76d0d23c4d5f23c7af43740a59c193126e9d43db2977d61dd48877aecd2dee7",
                "81ab2090c55561792f0216029eb4b8e402ab64e8d21e8b0691611c5d36e9ae72",
                "b749069e00bdbc934b2ef111c356b9a3c321a7368be62a0d8d7b2024c0273f6b",
                "d677f1bd4bb5e6413f9d75572657179ab348d1e63aed9ffef541dd39541ba949",
            ),
            (
                "grand",
                19,
                "52e7f3d4509171146fd3d74db2d85da1fdab9102caeb6cca6bb10e8da941284d",
                "a49faf1f1959ceb332a643247d89f0832b853a712b8d8ffa1fb42e802fd5b87b",
                "89bc3aefa362bc34b50e995e4d9178ebf41685490276b34e31e4b05ae06faf2c",
                "6440a55d6a7a8d134b91faba892a8cc00cc4c7b177ed73982a46c592c3b2b736",
            ),
            (
                "normal",
                37,
                "a0e90e94029fc6ab450f1adab1cfe0aef1ea059bc8be25c564c3b2fb6fc6e025",
                "8d87f10b832cfed5d24d402da8a0b9c48a3b591603f93c8fa3ab1699e0a9fe58",
                "b3620c7419bdbd2168046500c718b1351c1c9c190760fe8a901f1778d739fe0b",
                "980b94d3fac46872e812d3c24a5a01f0d6e577f04f006fdb11aa483817e018ac",
            ),
            (
                "chaos",
                37,
                "8b3d424953fffd43e4d102cd1a11a7be2d540f1e0f9da12df2f9a79e60904ca0",
                "9f836eb107c5676c4d1252f22afa5d14833220c4617ea6331eb7cfcf6857e966",
                "3291647c8595da0d1d2e6b385bb579bb55cb0ceeb9caf87d20ca0f5ab7485b62",
                "ace3b35f337767aa5d1f9b9d4a6a5784419e4dec3bb734811a6a78d33a8e7e4c",
            ),
            (
                "grand",
                37,
                "f69d72b18b465229280afe6c192286339537862c43d7e3bcb3335f13a6ebc705",
                "819614db44b2c5d0fe67ccdaa9567630299665c987f0f01c31ad98d98cb4a39c",
                "66ee87db3f5dca3c0fbc5fa7decee456316a20b1e10eb5083d99bf436e35e660",
                "66b6f3f0ee8de1b163c9c5e57eee209fdc8ea9d7e271593298b0cb29b41d4d8c",
            ),
        ] {
            let host = draft_host(style, seed);
            let intent = legal_action_envelopes(&host).unwrap()[0]["payload"].clone();
            let admitted = bind_public_intent(&host, intent).unwrap();
            let applied = apply_admitted(&host, &admitted).unwrap();
            let next = applied.position.export_envelope().unwrap();
            assert_eq!(
                applied.position.position_id(),
                expected_position_id,
                "{style}"
            );
            assert_eq!(
                source_action_id(&next["state"]).unwrap(),
                expected_state,
                "{style}"
            );
            assert_eq!(
                source_action_id(&next["rng"]).unwrap(),
                expected_rng,
                "{style}"
            );
            assert_eq!(
                source_action_id(&next["history"]).unwrap(),
                expected_history,
                "{style}"
            );
            assert_eq!(applied.event, next["history"][0], "{style}");
        }
    }

    #[test]
    fn exact_public_intent_and_stale_bound_action_are_checked_before_commit() {
        let host = draft_host("chaos", 19);
        let before = host.export_envelope().unwrap();
        let payload = legal_action_envelopes(&host).unwrap()[0]["payload"].clone();
        let admitted = bind_public_intent(&host, payload.clone()).unwrap();
        let mut reordered = payload.clone();
        reordered["cardInstanceIds"]
            .as_array_mut()
            .unwrap()
            .reverse();
        assert!(bind_public_intent(&host, reordered).is_err());
        let mut extra = payload;
        extra["unexpected"] = json!(true);
        assert!(bind_public_intent(&host, extra).is_err());
        assert_eq!(host.export_envelope().unwrap(), before);

        let applied = apply_admitted(&host, &admitted).unwrap();
        assert_eq!(host.export_envelope().unwrap(), before);
        assert_eq!(applied.position.revision(), host.revision() + 1);
        assert_eq!(applied.position.state().history.len(), 1);
        assert_eq!(&applied.event["action"], admitted.source_payload());
        assert!(!applied.action_id.is_empty());
        assert!(matches!(
            apply_admitted(&applied.position, &admitted),
            Err(V7ActionHostError::Admission(error))
                if error.kind == crate::v7_action_admission::AdmissionErrorKind::StalePosition
        ));
    }

    #[test]
    fn verified_play_public_click_keeps_private_source_descriptor_in_host() {
        let mut state =
            crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7)
                .unwrap();
        for index in [1, 0] {
            let action = crate::draft::legal_actions(&state).unwrap().remove(index);
            crate::draft::apply_pick(&mut state, &action).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        let host = V7HostPosition::from_state(state).unwrap();
        let before = host.export_envelope().unwrap();
        let source = legal_action_envelopes(&host).unwrap();
        let public = legal_public_intents(&host).unwrap();
        assert_eq!(source.len(), 21);
        assert_eq!(public.len(), 21);
        let intent = json!({"type":"move","color":"white","from":{"row":6,"col":0},"destination":{"row":5,"col":0}});
        assert_eq!(public[0], intent);
        assert!(public.iter().all(|intent| {
            intent.get("positionId").is_none()
                && intent.get("actionId").is_none()
                && intent.get("windowId").is_none()
                && (intent["type"] != "move" || intent.get("move").is_none())
        }));
        let bound = bind_public_intent(&host, intent.clone()).unwrap();
        assert_eq!(bound.source_payload(), &source[0]["payload"]);
        assert!(bound.source_payload().get("move").is_some());
        bound.revalidate(&host).unwrap();
        assert!(bind_public_intent(&host, source[0]["payload"].clone()).is_err());
        let mut injected = intent;
        injected["positionId"] = json!(host.position_id());
        assert!(bind_public_intent(&host, injected).is_err());
        assert_eq!(host.export_envelope().unwrap(), before);
    }

    #[test]
    fn trolley_private_window_and_source_page_stop_are_preserved() {
        let host = V7HostPosition::from_parts(
            json!({
                "board":vec![vec![Value::Null;8];8],"turn":"black","mode":"play",
                "activeTrolley":{"id":"private-window-1","color":"white","choices":[{},{}]},
            }),
            crate::RngState::seeded(19),
            Vec::new(),
        )
        .unwrap();
        let before = host.export_envelope().unwrap();
        let public = legal_public_intents(&host).unwrap();
        assert_eq!(
            public,
            vec![
                json!({"type":"trolleyChoice","color":"white","doomedIndex":0}),
                json!({"type":"trolleyChoice","color":"white","doomedIndex":1}),
            ]
        );
        let bound = bind_public_intent(&host, public[1].clone()).unwrap();
        assert_eq!(bound.source_payload()["windowId"], "private-window-1");
        let mut injected = public[0].clone();
        injected["windowId"] = json!("private-window-1");
        assert!(bind_public_intent(&host, injected).is_err());

        let mut cursor = V7PublicActionCursor::new(&host, None).unwrap();
        for expected in &public {
            let page = cursor.next_page(&host, 1, 65536).unwrap();
            assert_eq!(page.intents, vec![expected.clone()]);
            assert_eq!(page.examined, 1);
            assert!(!page.exhausted);
            assert_eq!(page.stop_reason, "page-limit");
        }
        let final_page = cursor.next_page(&host, 1, 65536).unwrap();
        assert!(final_page.intents.is_empty());
        assert_eq!(final_page.examined, 0);
        assert!(final_page.exhausted);
        assert_eq!(final_page.stop_reason, "exhausted");
        assert_eq!(host.export_envelope().unwrap(), before);
    }

    #[test]
    fn promotion_choice_objects_normalize_without_exposing_presentation_metadata() {
        let host = V7HostPosition::from_parts(
            json!({
                "board":vec![vec![Value::Null;8];8],"turn":"white","mode":"play",
                "pendingPromotion":{"color":"black","row":7,"col":0,
                    "choices":["queen",{"type":"rook","name":"display-only"}]},
                "activeTrolley":{"id":"later-window","color":"white","choices":[{},{}]},
            }),
            crate::RngState::seeded(19),
            Vec::new(),
        )
        .unwrap();
        let public = legal_public_intents(&host).unwrap();
        assert_eq!(
            public,
            vec![
                json!({"type":"promotionChoice","color":"black","promotionType":"queen"}),
                json!({"type":"promotionChoice","color":"black","promotionType":"rook"}),
            ]
        );
        bind_public_intent(&host, public[1].clone()).unwrap();
        let mut wrong = public[0].clone();
        wrong["color"] = json!("white");
        assert!(matches!(
            bind_public_intent(&host, wrong),
            Err(V7ActionHostError::Engine(EngineError::WrongActor))
        ));
    }

    #[test]
    fn completed_ui_pawn_storm_keeps_ten_selections_through_public_bind_and_commit() {
        // Native UI completion preserves the selected prefix. The frozen AI
        // collector's depth-eight bound is not the browser UI's pawn limit.
        let mut state = draft_host("normal", 19).state().clone();
        for index in [1, 0] {
            let pick = crate::draft::legal_actions(&state).unwrap().remove(index);
            crate::draft::apply_pick(&mut state, &pick).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        let pawn = state.board[6][0].as_ref().unwrap().clone();
        let white_king = state.board[7][4].clone();
        let black_king = state.board[0][4].clone();
        state.board = vec![vec![None; 8]; 8];
        state.board[7][4] = white_king;
        state.board[0][4] = black_king;
        let selected: Vec<_> = (0..8)
            .map(|col| crate::Square { row: 2, col })
            .chain((0..2).map(|col| crate::Square { row: 4, col }))
            .collect();
        for (index, square) in selected.iter().enumerate() {
            let mut piece = pawn.clone();
            piece.id = format!("ui-pawn-{index}");
            state.board[square.row as usize][square.col as usize] = Some(piece);
        }
        let mut definition = crate::card_registry::definition_for(RULES_VERSION_V7, "pawn-storm")
            .unwrap()
            .source_definition
            .clone();
        definition["instanceId"] = json!("ui-pawn-storm");
        let card: crate::CardSlot = serde_json::from_value(definition).unwrap();
        state.deck_slots.white = vec![card.clone()];
        state.deck_slots.black.clear();
        let selections: Vec<_> = selected.iter().rev().map(|square| json!(square)).collect();
        state.extra.insert(
            "targeting".into(),
            json!({
                "card":serde_json::to_value(&card).unwrap(), "launch":null, "pawnStorm":selections,
            }),
        );
        let host = V7HostPosition::from_state(state).unwrap();
        let before = host.export_envelope().unwrap();
        let intent = json!({
            "type":"card", "color":"white", "cardId":"pawn-storm", "cardInstanceId":"ui-pawn-storm",
            "target":{"selections":selections},
        });
        let admitted = bind_public_intent(&host, intent.clone()).unwrap();
        assert_eq!(admitted.source_payload(), &intent);
        assert_eq!(
            admitted.action().target.as_ref().unwrap()["selections"]
                .as_array()
                .unwrap()
                .len(),
            10
        );
        let mut cursor = V7PublicActionCursor::new(&host, None).unwrap();
        let page = cursor.next_page(&host, 4096, 65536).unwrap();
        assert_eq!(page.intents, vec![intent.clone()]);
        assert_eq!(page.examined, 1);
        assert!(page.exhausted);

        let mut reordered = intent;
        reordered["target"]["selections"]
            .as_array_mut()
            .unwrap()
            .reverse();
        assert!(bind_public_intent(&host, reordered).is_err());
        assert_eq!(host.export_envelope().unwrap(), before);
        let applied = apply_admitted(&host, &admitted).unwrap();
        assert_eq!(host.export_envelope().unwrap(), before);
        assert!(applied.position.state().extra["targeting"].is_null());
        assert!(applied.position.state().deck_slots.white[0].used);
        assert_eq!(
            applied.position.state().cards_used_this_turn.white,
            host.state().cards_used_this_turn.white + 1
        );
        assert_eq!(
            applied.position.state().history.len(),
            host.state().history.len() + 1
        );
        for (index, from) in selected.iter().enumerate() {
            let to = crate::Square {
                row: from.row - 1,
                col: from.col,
            };
            assert_eq!(
                applied.position.state().at(to).unwrap().id,
                format!("ui-pawn-{index}")
            );
            assert!(applied.position.state().at(*from).is_none());
        }
    }
}
