//! Source-ordered v7 candidates, bounded eager sets and resumable enumeration.
//! The immutable game host owns Position identity and public projection.

use crate::{Action, Color, EngineError, GameState, RULES_VERSION_V7, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::Arc};

struct SourceOpening {
    state_digest: &'static str,
    rng_cursor: usize,
    rng_state: u32,
    card_counts: &'static [(&'static str, usize)],
    legal_count: usize,
    legal_digest: &'static str,
}

/// The eager host response is limited independently of the frozen oracle's
/// default maxCandidates (100000). Large complete sets use the page cursor;
/// exceeding this host limit never means that source candidates disappeared.
const MAX_EAGER_ACTIONS: usize = 4096;
const MAX_SOURCE_NONLAZY_CANDIDATES: usize = 100000;
pub(crate) const MAX_PAGE_ACTIONS: usize = 4096;
pub(crate) const MAX_PAGE_EXAMINED: usize = 65536;

/// Native completion of a source UI window is distinct from the frozen
/// adapter's atomic-snapshot import. The source starts these windows before
/// applying the card: main:73337-73365,73430-73492,73632-73688. Keep the exact
/// window and its progress in the host until finishCard commits one effect.
#[derive(Clone)]
struct UiCardCompletion {
    card_id: String,
    card_instance_id: String,
    constraints: Vec<UiTargetConstraint>,
}

#[derive(Clone)]
enum UiTargetConstraint {
    Field(&'static str, Value),
    RootSquare(Value),
    Selections(Vec<Value>),
    NextFreeMoveOrigin { origin: Value, prefix_length: usize },
}

impl UiCardCompletion {
    fn matches_card(&self, card: &crate::CardSlot) -> bool {
        card.id == self.card_id && card.instance_id == self.card_instance_id
    }

    fn selection_prefix(&self) -> &[Value] {
        self.constraints
            .iter()
            .find_map(|constraint| match constraint {
                UiTargetConstraint::Selections(prefix) => Some(prefix.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
    }

    fn permits(&self, action: &Action) -> bool {
        action.kind == crate::ActionKind::Card
            && action.card_id.as_ref() == Some(&self.card_id)
            && action.card_instance_id.as_ref() == Some(&self.card_instance_id)
            && self.constraints.iter().all(|constraint| {
                let Some(target) = action.target.as_ref() else {
                    return false;
                };
                match constraint {
                    UiTargetConstraint::Field(field, value) => target.get(*field) == Some(value),
                    UiTargetConstraint::RootSquare(square) => {
                        target.get("row") == square.get("row")
                            && target.get("col") == square.get("col")
                    }
                    UiTargetConstraint::Selections(prefix) => target
                        .get("selections")
                        .and_then(Value::as_array)
                        .is_some_and(|selected| selected.starts_with(prefix)),
                    UiTargetConstraint::NextFreeMoveOrigin {
                        origin,
                        prefix_length,
                    } => target
                        .get("selections")
                        .and_then(Value::as_array)
                        .and_then(|selected| selected.get(*prefix_length))
                        .is_some_and(|selection| selection.get("from") == Some(origin)),
                }
            })
    }

    /// A bounded pair collector retains one order per pair, while the UI
    /// permits either member to be clicked first. Ordered multi-selections
    /// use the UI cursor instead of changing a source combination payload.
    fn complete_candidates(&self, candidates: Vec<Action>) -> Result<Vec<Action>> {
        require_source_family_budget(candidates.len(), "UI completion")?;
        let mut complete = Vec::new();
        let mut seen = BTreeSet::new();
        for action in candidates {
            let mut ordered = vec![action];
            if matches!(self.card_id.as_str(), "twins" | "chain") {
                let mut reverse = ordered[0].clone();
                if let Some(selected) = reverse
                    .target
                    .as_mut()
                    .and_then(|target| target.get_mut("selections"))
                    .and_then(Value::as_array_mut)
                    .filter(|selected| selected.len() == 2)
                {
                    selected.reverse();
                    ordered.push(reverse);
                }
            }
            for action in ordered {
                if self.permits(&action) {
                    let identity =
                        serde_jcs::to_vec(&action).map_err(EngineError::serialization)?;
                    if seen.insert(identity) {
                        complete.push(action);
                    }
                }
            }
        }
        Ok(complete)
    }
}

fn ui_square(value: &Value, field: &str) -> Result<Value> {
    let fields = value.as_object().ok_or_else(|| {
        EngineError::InvalidState(format!("v7 UI progress {field} must be a coordinate"))
    })?;
    if fields.len() != 2 || !fields.contains_key("row") || !fields.contains_key("col") {
        return Err(EngineError::InvalidState(format!(
            "v7 UI progress {field} has non-coordinate fields"
        )));
    }
    let square: crate::Square =
        serde_json::from_value(value.clone()).map_err(EngineError::serialization)?;
    if square.row >= 8 || square.col >= 8 {
        return Err(EngineError::InvalidState(format!(
            "v7 UI progress {field} is outside 8x8"
        )));
    }
    Ok(value.clone())
}

fn ui_card_completion(state: &GameState) -> Result<Option<UiCardCompletion>> {
    let windows: Vec<_> = [
        "ruleTicketChoice",
        "jokerChoice",
        "barricadeDirectionChoice",
        "targeting",
    ]
    .into_iter()
    .filter_map(|field| {
        state
            .extra
            .get(field)
            .filter(|window| !window.is_null())
            .map(|window| (field, window))
    })
    .collect();
    if windows.is_empty() {
        return Ok(None);
    }
    if windows.len() != 1 {
        return Err(EngineError::InvalidState(
            "v7 UI completion requires exactly one active card window".into(),
        ));
    }
    let (field, window) = windows[0];
    let object = window.as_object().ok_or_else(|| {
        EngineError::InvalidState(format!("v7 UI window {field} must be an object"))
    })?;
    if crate::observation::truth(object.get("serverAuthoritative"))
        || crate::observation::truth(object.get("submitting"))
    {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 UI window {field} has an external or already-submitting card command"
        )));
    }
    const PRESENTATION_FIELDS: &[&str] = &[
        "card",
        "launch",
        "color",
        "serverAuthoritative",
        "submitting",
    ];
    const TARGET_FIELDS: &[&str] = &[
        "barricadeDirection",
        "hypocrisy",
        "cleanupPieces",
        "portalGun",
        "amazon",
        "queen",
        "sacrifice",
        "pawn",
        "bishop",
        "evacuation",
        "panic",
        "spy",
        "pawnStorm",
        "twins",
        "chain",
        "chameleonMutation",
        "selections",
        "freeMoves",
        "freeMoveOrigin",
        "freeMoveCandidates",
    ];
    if let Some(key) = object.keys().find(|key| {
        !PRESENTATION_FIELDS.contains(&key.as_str())
            && (field != "targeting" || !TARGET_FIELDS.contains(&key.as_str()))
    }) {
        return Err(EngineError::InvalidState(format!(
            "v7 UI window {field} has unknown progress field {key}"
        )));
    }
    if let Some(color) = object.get("color") {
        let actor: Color =
            serde_json::from_value(color.clone()).map_err(EngineError::serialization)?;
        if actor != state.turn {
            return Err(EngineError::WrongActor);
        }
    }
    let presented = object
        .get("card")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            EngineError::InvalidState(format!("v7 UI window {field} is missing its exact card"))
        })?;
    let id = presented
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            EngineError::InvalidState(format!("v7 UI window {field} card id is missing"))
        })?;
    let instance = presented
        .get("instanceId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            EngineError::InvalidState(format!("v7 UI window {field} card instanceId is missing"))
        })?;
    let mut matching = state
        .deck_slots
        .get(state.turn)
        .iter()
        .filter(|card| card.id == id && card.instance_id == instance && !card.vacant);
    let card = matching.next().ok_or_else(|| {
        EngineError::InvalidState(format!(
            "v7 UI window {field} has no exact owned card instance"
        ))
    })?;
    if matching.next().is_some() {
        return Err(EngineError::InvalidState(
            "v7 UI window card identity is ambiguous".into(),
        ));
    }
    if card.used
        || card.recovering
        || crate::observation::truth(presented.get("used"))
        || crate::observation::truth(presented.get("recovering"))
    {
        return Err(EngineError::IllegalAction);
    }
    if crate::observation::truth(card.extra.get("serverAuthoritativeCard"))
        || crate::observation::truth(presented.get("serverAuthoritativeCard"))
    {
        return Err(EngineError::UnsupportedFeature(
            "v7 external authoritative card UI completion".into(),
        ));
    }
    if presented
        .get("effect")
        .is_some_and(|effect| effect.as_str() != Some(card.effect.as_str()))
    {
        return Err(EngineError::InvalidState(
            "v7 UI window card effect differs from its owned instance".into(),
        ));
    }
    if !crate::card_registry::source_candidate_available(state, card)? {
        return Err(EngineError::IllegalAction);
    }
    let expected = match field {
        "ruleTicketChoice" => Some("ruleTicket"),
        "jokerChoice" => Some("joker"),
        "barricadeDirectionChoice" => Some("barricade"),
        _ => None,
    };
    if expected.is_some_and(|effect| card.effect != effect) {
        return Err(EngineError::InvalidState(format!(
            "v7 UI window {field} has the wrong card effect"
        )));
    }
    let mut completion = UiCardCompletion {
        card_id: id.to_owned(),
        card_instance_id: instance.to_owned(),
        constraints: Vec::new(),
    };
    if field != "targeting" {
        return Ok(Some(completion));
    }
    if !crate::observation::truth(card.extra.get("target")) {
        return Err(EngineError::InvalidState(
            "v7 targeting window card has no source target selection".into(),
        ));
    }
    if let Some(direction) = object.get("barricadeDirection") {
        if card.effect != "barricade"
            || !matches!(direction.as_str(), Some("horizontal" | "vertical"))
        {
            return Err(EngineError::InvalidState(
                "v7 targeting barricade direction is invalid".into(),
            ));
        }
        completion
            .constraints
            .push(UiTargetConstraint::Field("direction", direction.clone()));
    }
    for (key, effect, maximum) in [
        ("hypocrisy", "hypocrisy", 4),
        ("portalGun", "portalGun", 2),
        ("cleanupPieces", "cleanupPieces", 3),
        ("evacuation", "emergencyEvacuation", 3),
        ("panic", "panic", 2),
        ("spy", "spy", 2),
        ("pawnStorm", "pawnStorm", 64),
        ("twins", "twins", 2),
        ("chain", "chain", 2),
        ("chameleonMutation", "chameleonMutation", 3),
    ] {
        let Some(value) = object.get(key) else {
            continue;
        };
        if card.effect != effect {
            return Err(EngineError::InvalidState(format!(
                "v7 targeting progress {key} belongs to another card"
            )));
        }
        let selected = value
            .as_array()
            .filter(|selected| selected.len() <= maximum)
            .ok_or_else(|| {
                EngineError::InvalidState(format!(
                    "v7 targeting progress {key} exceeds its selection limit"
                ))
            })?;
        let prefix: Vec<_> = selected
            .iter()
            .map(|cell| ui_square(cell, key))
            .collect::<Result<_>>()?;
        if prefix
            .iter()
            .enumerate()
            .any(|(index, cell)| prefix[..index].contains(cell))
        {
            return Err(EngineError::InvalidState(format!(
                "v7 targeting progress {key} repeats a square"
            )));
        }
        completion
            .constraints
            .push(UiTargetConstraint::Selections(prefix));
    }
    if let Some(value) = object.get("selections") {
        if !matches!(card.id.as_str(), "brainwash" | "taboo") {
            return Err(EngineError::InvalidState(
                "v7 targeting staged selections have the wrong card".into(),
            ));
        }
        let selected = value
            .as_array()
            .filter(|selected| selected.len() <= 2)
            .ok_or_else(|| {
                EngineError::InvalidState("v7 targeting selections exceed two stages".into())
            })?;
        let prefix = selected
            .iter()
            .map(|cell| ui_square(cell, "selections"))
            .collect::<Result<_>>()?;
        completion
            .constraints
            .push(UiTargetConstraint::Selections(prefix));
    }
    for key in ["queen", "bishop", "pawn", "sacrifice"] {
        let Some(value) = object.get(key).filter(|value| !value.is_null()) else {
            continue;
        };
        let allowed = match key {
            "queen" => matches!(card.effect.as_str(), "amazon" | "hook") || card.id == "grappler",
            "bishop" => card.effect == "windmill",
            "pawn" => card.effect == "feudalContract",
            "sacrifice" => card.effect == "sacrifice",
            _ => false,
        };
        if !allowed {
            return Err(EngineError::InvalidState(format!(
                "v7 targeting progress {key} has the wrong card"
            )));
        }
        let square = ui_square(value, key)?;
        completion.constraints.push(if key == "queen" {
            UiTargetConstraint::RootSquare(square)
        } else {
            UiTargetConstraint::Field(key, square)
        });
    }
    if object.contains_key("amazon") {
        return Err(EngineError::UnsupportedFeature(
            "v7 authoritative Amazon UI selection completion".into(),
        ));
    }
    let has_free_move = ["freeMoves", "freeMoveOrigin", "freeMoveCandidates"]
        .iter()
        .any(|key| object.contains_key(*key));
    if has_free_move {
        if card.effect != "freeMove" {
            return Err(EngineError::InvalidState(
                "v7 targeting free-move progress has the wrong card".into(),
            ));
        }
        if object
            .get("freeMoveCandidates")
            .is_some_and(|value| !value.is_array() && !value.is_null())
        {
            return Err(EngineError::InvalidState(
                "v7 free-move presentation candidates must be an array".into(),
            ));
        }
        let selected = object
            .get("freeMoves")
            .map(|value| {
                value.as_array().ok_or_else(|| {
                    EngineError::InvalidState("v7 targeting freeMoves must be an array".into())
                })
            })
            .transpose()?
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if selected.len() > 3 {
            return Err(EngineError::InvalidState(
                "v7 free-move progress exceeds three plans".into(),
            ));
        }
        let mut prefix = Vec::new();
        let mut origins = BTreeSet::new();
        for selection in selected {
            if selection.as_object().is_none_or(|selection| {
                selection.keys().any(|key| {
                    !matches!(
                        key.as_str(),
                        "from" | "to" | "pieceId" | "pieceType" | "fromCells" | "toCells"
                    )
                })
            }) {
                return Err(EngineError::InvalidState(
                    "v7 free-move progress has unknown plan fields".into(),
                ));
            }
            let from = ui_square(
                selection.get("from").ok_or_else(|| {
                    EngineError::InvalidState("v7 free-move progress lacks from".into())
                })?,
                "freeMoves.from",
            )?;
            let to = ui_square(
                selection.get("to").ok_or_else(|| {
                    EngineError::InvalidState("v7 free-move progress lacks to".into())
                })?,
                "freeMoves.to",
            )?;
            let square: crate::Square =
                serde_json::from_value(from.clone()).map_err(EngineError::serialization)?;
            let piece = state.at(square).ok_or_else(|| {
                EngineError::InvalidState("v7 free-move progress source is missing".into())
            })?;
            if piece.color != state.turn
                || !origins.insert(piece.id.clone())
                || selection
                    .get("pieceId")
                    .is_some_and(|id| id.as_str() != Some(piece.id.as_str()))
                || selection
                    .get("pieceType")
                    .is_some_and(|kind| kind.as_str() != Some(piece.kind.as_str()))
            {
                return Err(EngineError::InvalidState(
                    "v7 free-move progress has a stale or repeated piece".into(),
                ));
            }
            prefix.push(serde_json::json!({"from":from,"to":to}));
        }
        let prefix_length = prefix.len();
        completion
            .constraints
            .push(UiTargetConstraint::Selections(prefix));
        if let Some(origin) = object
            .get("freeMoveOrigin")
            .filter(|value| !value.is_null())
        {
            if origin.as_object().is_none_or(|origin| {
                origin.len() != 3
                    || origin
                        .keys()
                        .any(|key| !matches!(key.as_str(), "row" | "col" | "pieceId"))
            }) {
                return Err(EngineError::InvalidState(
                    "v7 active free-move origin has unknown fields".into(),
                ));
            }
            let square = ui_square(
                &serde_json::json!({"row":origin.get("row"),"col":origin.get("col")}),
                "freeMoveOrigin",
            )?;
            let at: crate::Square =
                serde_json::from_value(square.clone()).map_err(EngineError::serialization)?;
            let piece = state.at(at).ok_or_else(|| {
                EngineError::InvalidState("v7 active free-move origin is missing".into())
            })?;
            if prefix_length >= 3
                || piece.color != state.turn
                || origins.contains(&piece.id)
                || origin.get("pieceId").and_then(Value::as_str) != Some(piece.id.as_str())
            {
                return Err(EngineError::InvalidState(
                    "v7 active free-move origin is stale or already selected".into(),
                ));
            }
            completion
                .constraints
                .push(UiTargetConstraint::NextFreeMoveOrigin {
                    origin: square,
                    prefix_length,
                });
        }
    }
    Ok(Some(completion))
}

#[derive(Clone)]
enum CandidateCursor {
    Source(crate::card_effects::SourceCardCandidateCursor),
    Ui(Box<crate::card_effects::OrderedSelectionCursor>),
}

impl CandidateCursor {
    fn next_candidate(&mut self) -> Option<Action> {
        match self {
            Self::Source(cursor) => cursor.next_candidate(),
            Self::Ui(cursor) => cursor.next_candidate(),
        }
    }

    fn is_exhausted(&self) -> bool {
        match self {
            Self::Source(cursor) => cursor.is_exhausted(),
            Self::Ui(cursor) => cursor.is_exhausted(),
        }
    }

    fn contains_candidate(&self, action: &Action) -> Result<bool> {
        match self {
            Self::Source(cursor) => cursor.contains_candidate(action),
            Self::Ui(cursor) => cursor.contains_candidate(action),
        }
    }
}

fn card_cursor(
    state: &GameState,
    card: &crate::CardSlot,
    completion: Option<&UiCardCompletion>,
) -> Result<Option<CandidateCursor>> {
    if let Some(completion) = completion {
        if let Some(mut cursor) = crate::card_effects::staged_cursor(state, card)? {
            cursor.resume_prefix(completion.selection_prefix())?;
            return Ok(Some(CandidateCursor::Ui(Box::new(cursor))));
        }
        if let Some(mut cursor) = crate::card_effects::source_card_candidate_cursor(state, card)? {
            cursor.resume_prefix(completion.selection_prefix())?;
            return Ok(Some(CandidateCursor::Source(cursor)));
        }
        return Ok(None);
    }
    Ok(
        crate::card_effects::source_card_candidate_cursor(state, card)?
            .map(CandidateCursor::Source),
    )
}

fn nonlazy_card_candidates(
    state: &GameState,
    card: &crate::CardSlot,
    completion: Option<&UiCardCompletion>,
) -> Result<Vec<Action>> {
    if let Some(completion) = completion {
        let candidates = crate::card_effects::actions(state, card)?.ok_or_else(|| {
            EngineError::UnsupportedFeature(format!(
                "v7 completed UI target enumeration for {}",
                card.id
            ))
        })?;
        completion.complete_candidates(candidates)
    } else {
        crate::card_effects::source_card_nonlazy_candidates(state, card)
    }
}

#[derive(Clone, Copy)]
enum SourceCursorStage {
    Movement,
    Cards,
    FriendlyFallback,
    Done,
}

/// Retain only the current non-lazy family and the depth-bounded selection
/// cursor. In particular, a four-square Hypocrisy space is never allocated as
/// millions of Actions. A cursor can be cloned cheaply enough to stage a page;
/// callers commit that staged cursor only after every probe succeeds.
#[derive(Clone)]
pub(crate) struct SourceActionCursor {
    state: Arc<GameState>,
    stage: SourceCursorStage,
    pending: Arc<Vec<Action>>,
    pending_index: usize,
    deck_slot: usize,
    lazy: Option<CandidateCursor>,
    card_window: Option<bool>,
    card_filter: Option<String>,
    ui_completion: Option<UiCardCompletion>,
    special: bool,
    emitted: bool,
    exhausted: bool,
}

impl SourceActionCursor {
    pub(crate) fn new(state: &GameState, card_filter: Option<String>) -> Result<Self> {
        require_v7(state)?;
        if let Some(id) = &card_filter {
            crate::card_registry::definition_for(RULES_VERSION_V7, id)?;
        }
        let special = state.result().is_some()
            || state.mode != "play"
            || state
                .extra
                .get("pendingPromotion")
                .is_some_and(|v| !v.is_null())
            || state
                .extra
                .get("activeTrolley")
                .is_some_and(|v| !v.is_null());
        let ui_completion = if special {
            None
        } else {
            ui_card_completion(state)?
        };
        let (stage, mut pending) = if special {
            (SourceCursorStage::Done, special_candidates(state)?)
        } else {
            (
                if card_filter.is_none() && ui_completion.is_none() {
                    SourceCursorStage::Movement
                } else {
                    SourceCursorStage::Cards
                },
                Vec::new(),
            )
        };
        if let Some(id) = &card_filter {
            pending.retain(|action| action.card_id.as_ref() == Some(id));
        }
        Ok(Self {
            state: Arc::new(state.clone()),
            stage,
            pending: Arc::new(pending),
            pending_index: 0,
            deck_slot: 0,
            lazy: None,
            card_window: None,
            card_filter,
            ui_completion,
            special,
            emitted: false,
            exhausted: false,
        })
    }

    pub(crate) fn state(&self) -> &GameState {
        &self.state
    }

    pub(crate) fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    pub(crate) fn next_candidate(&mut self) -> Result<Option<Action>> {
        if self.exhausted {
            return Ok(None);
        }
        loop {
            if let Some(cursor) = &mut self.lazy {
                if !cursor.is_exhausted()
                    && let Some(action) = cursor.next_candidate()
                {
                    self.emitted = true;
                    return Ok(Some(action));
                }
                self.lazy = None;
            }
            if let Some(action) = self.pending.get(self.pending_index) {
                let action = action.clone();
                self.pending_index += 1;
                self.emitted = true;
                return Ok(Some(action));
            }
            match self.stage {
                SourceCursorStage::Movement => {
                    let actions =
                        crate::movement::v7_source_ordered_move_candidates(&self.state, false)?;
                    require_source_family_budget(actions.len(), "movement")?;
                    self.pending = Arc::new(actions);
                    self.pending_index = 0;
                    self.stage = SourceCursorStage::Cards;
                }
                SourceCursorStage::Cards => {
                    let card_window = match self.card_window {
                        Some(open) => open,
                        None => {
                            let open = crate::movement::v7_card_action_window_open(&self.state)?;
                            self.card_window = Some(open);
                            open
                        }
                    };
                    if !card_window
                        || self.deck_slot >= self.state.deck_slots.get(self.state.turn).len()
                    {
                        self.stage = SourceCursorStage::FriendlyFallback;
                        continue;
                    }
                    let card = &self.state.deck_slots.get(self.state.turn)[self.deck_slot];
                    self.deck_slot += 1;
                    if card.vacant
                        || self.card_filter.as_ref().is_some_and(|id| &card.id != id)
                        || self
                            .ui_completion
                            .as_ref()
                            .is_some_and(|completion| !completion.matches_card(card))
                    {
                        continue;
                    }
                    if !crate::card_registry::source_candidate_available(&self.state, card)? {
                        continue;
                    }
                    if let Some(cursor) =
                        card_cursor(&self.state, card, self.ui_completion.as_ref())?
                    {
                        self.lazy = Some(cursor);
                    } else {
                        let actions = nonlazy_card_candidates(
                            &self.state,
                            card,
                            self.ui_completion.as_ref(),
                        )?;
                        require_source_family_budget(actions.len(), "card")?;
                        self.pending = Arc::new(actions);
                        self.pending_index = 0;
                    }
                }
                SourceCursorStage::FriendlyFallback => {
                    self.stage = SourceCursorStage::Done;
                    // Raw emissions, including later rejected selections,
                    // suppress fallback just as iterateCandidatePayloads does.
                    if !self.emitted && self.card_filter.is_none() && self.ui_completion.is_none() {
                        let actions =
                            crate::movement::v7_source_ordered_move_candidates(&self.state, true)?;
                        require_source_family_budget(actions.len(), "friendly-crush fallback")?;
                        self.pending = Arc::new(actions);
                        self.pending_index = 0;
                    }
                }
                SourceCursorStage::Done => {
                    self.exhausted = true;
                    return Ok(None);
                }
            }
        }
    }

    pub(crate) fn accepts(&self, action: &Action) -> Result<bool> {
        if action.color != self.state.decision_actor() || action.position_key.is_some() {
            return Err(EngineError::InvalidState(
                "v7 source candidate has the wrong actor or a private Position key".into(),
            ));
        }
        if self.special {
            return Ok(true);
        }
        if self
            .ui_completion
            .as_ref()
            .is_some_and(|completion| !completion.permits(action))
        {
            return Ok(false);
        }
        let mut owned = self.state.as_ref().clone();
        match crate::transition::apply_without_public_event(&mut owned, action) {
            Ok(_) => {
                crate::replay::canonicalize_position_frames(&mut owned)?;
                Ok(true)
            }
            Err(EngineError::IllegalAction) => Ok(false),
            Err(error) => Err(error),
        }
    }
}

fn require_v7(state: &GameState) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 action surface requires the pinned v7 rules version".into(),
        ));
    }
    Ok(())
}

fn special_candidates(state: &GameState) -> Result<Vec<Action>> {
    if state.result().is_some() || state.mode == "gameover" {
        return Ok(Vec::new());
    }
    if state.mode == "draft" {
        let actions = crate::draft::legal_actions(state)?;
        require_budget(actions.len(), "draft choices")?;
        return Ok(actions);
    }
    if state.mode != "play" {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 source action surface for {} mode",
            state.mode
        )));
    }
    if let Some(window) = state
        .extra
        .get("pendingPromotion")
        .filter(|value| !value.is_null())
    {
        let color = window
            .get("color")
            .cloned()
            .ok_or_else(|| EngineError::InvalidState("pendingPromotion.color is missing".into()))?;
        let choices = if let Some(value) = window
            .get("choices")
            .filter(|value| crate::observation::truth(Some(value)))
        {
            value.as_array().cloned().ok_or_else(|| {
                EngineError::InvalidState(
                    "pendingPromotion.choices must be a source choice array".into(),
                )
            })?
        } else {
            let square: crate::Square = serde_json::from_value(serde_json::json!({
                "row":window.get("row"),"col":window.get("col"),
            }))
            .map_err(EngineError::serialization)?;
            let piece = state.at(square).ok_or_else(|| {
                EngineError::InvalidState(
                    "pendingPromotion fallback source piece is missing".into(),
                )
            })?;
            crate::v7_promotion::promotion_choices_for_v7(state, piece, square)?
                .into_iter()
                .map(Value::String)
                .collect()
        };
        require_budget(choices.len(), "promotion choices")?;
        return choices
            .iter()
            .map(|choice| {
                let kind = choice
                    .as_str()
                    .or_else(|| choice.get("type").and_then(Value::as_str))
                    .filter(|kind| !kind.is_empty())
                    .ok_or_else(|| {
                        EngineError::InvalidState(
                            "pendingPromotion choice needs a source piece type".into(),
                        )
                    })?;
                serde_json::from_value(serde_json::json!({
                    "type":"promotionChoice", "color":color, "promotionType":kind,
                }))
                .map_err(EngineError::serialization)
            })
            .collect();
    }
    if let Some(window) = state
        .extra
        .get("activeTrolley")
        .filter(|value| !value.is_null())
    {
        return trolley_choices(window, state.decision_actor());
    }
    Err(EngineError::InvalidState(
        "v7 special action cursor has no selection window".into(),
    ))
}

// %APPDATA%/Accelerate/reports/v7-opening-source-probe/seed19-active-only/
// manifest.json, generated from the frozen main-OahWs0tU.js (SHA-256
// e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c).
// The state digest is JCS SHA-256 of state without the outer Position's
// rulesetId, rng and history metadata. The action digest covers the complete
// ordered payload array, excluding actionId/positionId envelopes.
fn opening_for(style: &str) -> Result<SourceOpening> {
    let opening = match style {
        "normal" => SourceOpening {
            state_digest: "8ea26cf766f9fee2e04804f0cea78c07828d6749e14873578be3ab3848b868f8",
            rng_cursor: 292,
            rng_state: 596_976_663,
            card_counts: &[("relay", 1)],
            legal_count: 21,
            legal_digest: "a8439b7876ea8387abe35d36f7036f705cd8ac042ed1db534dc30c1acf70a831",
        },
        "chaos" => SourceOpening {
            state_digest: "3b8fd9fae0404a6032c8f5daa7bbdecc03cac93730355f13ebb6d97a2f290793",
            rng_cursor: 400,
            rng_state: 185_085_603,
            card_counts: &[("queens-gambit", 1), ("quantum-mechanics", 1)],
            legal_count: 22,
            legal_digest: "f93efb00f5d6b25078b766ef36c1c3efd090e45543c1a2ceb2e1859e7d9ff3cd",
        },
        "grand" => SourceOpening {
            state_digest: "17f03e0add054f3b231f4f3943c78dcd358ed18f815ab368b9d1879869072075",
            rng_cursor: 124,
            rng_state: 1_313_359_343,
            card_counts: &[
                ("inertia", 5),
                ("miracle", 0),
                ("taunt", 1),
                ("trojan-horse", 2),
                ("grasshopper", 4),
                ("grappler", 4),
            ],
            legal_count: 36,
            legal_digest: "e3d592dd54d9a53bfee69101f58ed933ca15acf0042d6fc02f3b7af6d875a841",
        },
        _ => {
            return Err(EngineError::UnsupportedFeature(
                "v7 action candidates outside source-verified seed-19 styles".into(),
            ));
        }
    };
    Ok(opening)
}

fn unsupported(reason: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!(
        "v7 action candidates outside source-verified seed-19 first play: {reason}"
    ))
}

fn digest<T: serde::Serialize>(value: &T) -> Result<String> {
    let canonical = serde_jcs::to_vec(value).map_err(EngineError::serialization)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

fn require_source_opening(state: &GameState) -> Result<SourceOpening> {
    if state.ruleset_id != RULES_VERSION_V7 || !state.history.is_empty() {
        return Err(unsupported("rules version or history"));
    }
    let style = state
        .extra
        .get("gameStyle")
        .and_then(Value::as_str)
        .ok_or_else(|| unsupported("game style"))?;
    let source = opening_for(style)?;
    if state.rng.algorithm != "lcg32-v1"
        || !state.rng.tape.is_empty()
        || state.rng.cursor != source.rng_cursor
        || state.rng.state != source.rng_state
    {
        return Err(unsupported("RNG"));
    }
    let mut checked = state.clone();
    checked.validate_v7_snapshot_shape_and_identify()?;
    if &checked != state {
        return Err(unsupported("noncanonical state"));
    }
    let mut raw = serde_json::to_value(state).map_err(EngineError::serialization)?;
    let fields = raw
        .as_object_mut()
        .ok_or_else(|| unsupported("state object"))?;
    for name in ["rulesetId", "rng", "history"] {
        if fields.remove(name).is_none() {
            return Err(unsupported("source state envelope"));
        }
    }
    if digest(&raw)? != source.state_digest {
        return Err(unsupported("state digest"));
    }
    Ok(source)
}

/// A card target enumerator may use this only to prove the frozen grand
/// seed-19 first-play zero-target case. The predicate checks full source
/// state, RNG and history; it does not authorize any general Miracle rule.
/// Return the complete ordered source candidate payloads for the three exact
/// seed-19 openings. The card kernel still owns each supported card's target
/// enumeration. Miracle has zero candidates in only this pinned grand state;
/// its general target predicate remains unsupported.
#[allow(dead_code, reason = "v7 Position execution remains closed")]
pub(crate) fn seed19_first_play_legal_actions(state: &GameState) -> Result<Vec<Action>> {
    let source = require_source_opening(state)?;
    let (_, mut actions) = crate::movement::v7_verified_first_play_movement_actions(state)?;
    if actions.len() != 20 {
        return Err(unsupported("opening movement count"));
    }
    let cards = state
        .deck_slots
        .get(Color::White)
        .iter()
        .filter(|card| !card.vacant)
        .collect::<Vec<_>>();
    if cards.len() != source.card_counts.len()
        || cards
            .iter()
            .zip(source.card_counts)
            .any(|(card, (id, _))| card.id != *id || card.used || card.recovering)
    {
        return Err(unsupported("selected card order"));
    }
    for (card, &(_, expected_count)) in cards.into_iter().zip(source.card_counts) {
        if card.id == "miracle" {
            // Its source first-play capture target set is empty. Keep the
            // unsupported general Miracle rule closed under all other states.
            continue;
        }
        let offered = crate::card_effects::actions(state, card)?
            .ok_or_else(|| unsupported("unimplemented selected card"))?;
        if offered.len() != expected_count {
            return Err(unsupported("card action count"));
        }
        for action in &offered {
            if crate::card_effects::validate(state, card, action)? != Some(true) {
                return Err(unsupported("card action validation"));
            }
        }
        actions.extend(offered);
    }
    if actions.len() != source.legal_count || digest(&actions)? != source.legal_digest {
        return Err(unsupported("ordered legal payload"));
    }
    Ok(actions)
}

fn require_budget(count: usize, stage: &str) -> Result<()> {
    if count > MAX_EAGER_ACTIONS {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 {stage} contains {count} actions; eager limit is {MAX_EAGER_ACTIONS}, use a paged action cursor"
        )));
    }
    Ok(())
}

fn require_source_family_budget(count: usize, stage: &str) -> Result<()> {
    if count > MAX_SOURCE_NONLAZY_CANDIDATES {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 non-lazy {stage} family contains {count} candidates; frozen oracle default is {MAX_SOURCE_NONLAZY_CANDIDATES}"
        )));
    }
    Ok(())
}

fn trolley_choices(window: &Value, actor: Color) -> Result<Vec<Action>> {
    let fields = window.as_object().ok_or_else(|| {
        EngineError::InvalidState("activeTrolley must be a source choice object".into())
    })?;
    let id = fields
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| EngineError::InvalidState("activeTrolley.id must be text".into()))?;
    let choices = fields
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineError::InvalidState("activeTrolley.choices must be an array".into())
        })?;
    require_budget(choices.len(), "trolley choice window")?;
    // Source apply accepts only doomedIndex 0 or 1. A malformed window with
    // more entries must not advertise a choice that public admission rejects.
    if choices.len() > 2 {
        return Err(EngineError::UnsupportedFeature(
            "v7 trolley window has more than two source-applicable choices".into(),
        ));
    }
    let color: Color = fields
        .get("color")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(EngineError::serialization)?
        .ok_or_else(|| EngineError::InvalidState("activeTrolley.color is missing".into()))?;
    if color != actor {
        return Err(EngineError::InvalidState(
            "activeTrolley.color differs from the current decision actor".into(),
        ));
    }
    let mut actions = Vec::with_capacity(choices.len());
    for index in 0..choices.len() {
        let payload = serde_json::json!({
            "type":"trolleyChoice", "color":color,
            "windowId":id, "doomedIndex":index,
        });
        let action = serde_json::from_value(payload).map_err(EngineError::serialization)?;
        actions.push(action);
    }
    Ok(actions)
}

fn transition_accepted_actions(state: &GameState, candidates: Vec<Action>) -> Result<Vec<Action>> {
    require_budget(candidates.len(), "candidate set")?;
    let actor = state.decision_actor();
    let ui_completion = ui_card_completion(state)?;
    let mut accepted = Vec::with_capacity(candidates.len());
    for action in candidates {
        if action.color != actor || action.position_key.is_some() {
            return Err(EngineError::InvalidState(
                "v7 source candidate has the wrong actor or a private Position key".into(),
            ));
        }
        if ui_completion
            .as_ref()
            .is_some_and(|completion| !completion.permits(&action))
        {
            continue;
        }
        // Source `actionStream(legal:true)` tests each candidate on an owned
        // restored Position. A rejected selection cannot mutate this state.
        let mut owned = state.clone();
        match crate::transition::apply_without_public_event(&mut owned, &action) {
            Ok(_) => {
                crate::replay::canonicalize_position_frames(&mut owned)?;
                accepted.push(action);
            }
            Err(EngineError::IllegalAction) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(accepted)
}

fn ordered_play_candidates(state: &GameState) -> Result<Vec<Action>> {
    // Frozen iterateCandidatePayloads yields ordinary movement first, then
    // one card instance at a time in deck order. Its friendly-crush fallback
    // runs only if no ordinary movement or card candidate was emitted, even
    // if every emitted candidate is later rejected by apply.
    let mut cursor = SourceActionCursor::new(state, None)?;
    let mut candidates = Vec::new();
    while let Some(action) = cursor.next_candidate()? {
        candidates.push(action);
        require_budget(candidates.len(), "complete eager candidate set")?;
    }
    Ok(candidates)
}

/// Return the source-ordered complete legal set. The exact seed-19 first-play
/// snapshots remain a separately verified profile while the general movement
/// and card owners complete source parity. Every other branch either proves
/// its full set or fails with its precise unsupported rule.
pub(crate) fn legal_source_actions(state: &GameState) -> Result<Vec<Action>> {
    require_v7(state)?;
    if state.result().is_some() || state.mode == "gameover" {
        return Ok(Vec::new());
    }
    if state.mode == "draft" {
        return special_candidates(state);
    }
    if state.mode != "play" {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 source action surface for {} mode",
            state.mode
        )));
    }
    if state
        .extra
        .get("pendingPromotion")
        .is_some_and(|value| !value.is_null())
        || state
            .extra
            .get("activeTrolley")
            .is_some_and(|value| !value.is_null())
    {
        return special_candidates(state);
    }
    // The frozen client has complete ordered results for these three exact
    // positions. A general candidate probe may not override their digest.
    if require_source_opening(state).is_ok() {
        return seed19_first_play_legal_actions(state);
    }
    let candidates = ordered_play_candidates(state)?;
    transition_accepted_actions(state, candidates)
}

/// Prove one source selection without exhausting an unrelated card family.
/// Membership and execution acceptance remain separate checks. The lazy
/// owner's finite domain predicate covers every offset in its family.
pub(crate) fn validate_source_selection(state: &GameState, action: &Action) -> Result<()> {
    require_v7(state)?;
    if state.result().is_some() || state.mode == "gameover" {
        return Err(EngineError::Terminal);
    }
    if action.color != state.decision_actor() {
        return Err(EngineError::WrongActor);
    }
    if action.position_key.is_some() {
        return Err(EngineError::IllegalAction);
    }
    if require_source_opening(state).is_ok() {
        return if seed19_first_play_legal_actions(state)?.contains(action) {
            Ok(())
        } else {
            Err(EngineError::IllegalAction)
        };
    }
    let special = state.mode != "play"
        || state
            .extra
            .get("pendingPromotion")
            .is_some_and(|value| !value.is_null())
        || state
            .extra
            .get("activeTrolley")
            .is_some_and(|value| !value.is_null());
    if special {
        return if special_candidates(state)?.contains(action) {
            Ok(())
        } else {
            Err(EngineError::IllegalAction)
        };
    }
    let ui_completion = ui_card_completion(state)?;
    if ui_completion
        .as_ref()
        .is_some_and(|completion| !completion.permits(action))
    {
        return Err(EngineError::IllegalAction);
    }
    // UI moveQuantumPiece is a separate source completion. The frozen AI
    // collector visits physical board cells and never enumerates these moves.
    if action.kind == crate::ActionKind::Move
        && action
            .destination
            .as_ref()
            .is_some_and(|target| target.flags.contains_key("quantumFrom"))
    {
        let ghost: crate::Square = serde_json::from_value(
            action.destination.as_ref().unwrap().flags["quantumFrom"].clone(),
        )
        .map_err(EngineError::serialization)?;
        if !quantum_ui_actions(state, ghost)?.contains(action) {
            return Err(EngineError::IllegalAction);
        }
        let mut owned = state.clone();
        crate::transition::apply_without_public_event(&mut owned, action)?;
        return Ok(());
    }
    let member = if action.kind == crate::ActionKind::Card {
        if !crate::movement::v7_card_action_window_open(state)? {
            return Err(EngineError::IllegalAction);
        }
        let card = state
            .deck_slots
            .get(state.turn)
            .iter()
            .find(|card| {
                !card.vacant
                    && Some(&card.id) == action.card_id.as_ref()
                    && Some(&card.instance_id) == action.card_instance_id.as_ref()
            })
            .ok_or(EngineError::IllegalAction)?;
        if !crate::card_registry::source_candidate_available(state, card)? {
            return Err(EngineError::IllegalAction);
        }
        if let Some(cursor) = card_cursor(state, card, ui_completion.as_ref())? {
            cursor.contains_candidate(action)?
        } else {
            let candidates = nonlazy_card_candidates(state, card, ui_completion.as_ref())?;
            require_source_family_budget(candidates.len(), "selected card")?;
            candidates.contains(action)
        }
    } else {
        let ordinary = crate::movement::v7_source_ordered_move_candidates(state, false)?;
        require_source_family_budget(ordinary.len(), "selected movement")?;
        if ordinary.contains(action) {
            true
        } else {
            friendly_fallback_candidates(state)?.contains(action)
        }
    };
    if !member {
        return Err(EngineError::IllegalAction);
    }
    let mut owned = state.clone();
    crate::transition::apply_without_public_event(&mut owned, action)?;
    Ok(())
}

fn friendly_fallback_candidates(state: &GameState) -> Result<Vec<Action>> {
    let mut source = SourceActionCursor::new(state, None)?;
    // The cursor enters fallback only after every ordinary raw family proved
    // empty. An emitted but rejected card must therefore still block it.
    if source.next_candidate()?.is_some_and(|action| {
        action.extra.get("forcedFriendlyCrush") == Some(&serde_json::json!(true))
    }) {
        let candidates = crate::movement::v7_source_ordered_move_candidates(state, true)?;
        require_source_family_budget(candidates.len(), "selected friendly-crush fallback")?;
        Ok(candidates)
    } else {
        Ok(Vec::new())
    }
}

pub(crate) fn resolve_public_move(state: &GameState, intent: &Value) -> Result<Action> {
    if ui_card_completion(state)?.is_some() {
        return Err(EngineError::IllegalAction);
    }
    let origin = intent
        .get("from")
        .and_then(Value::as_object)
        .ok_or(EngineError::IllegalAction)?;
    if origin.len() != 2 || !origin.contains_key("row") || !origin.contains_key("col") {
        return Err(EngineError::IllegalAction);
    }
    let from: crate::Square = serde_json::from_value(
        intent
            .get("from")
            .cloned()
            .ok_or(EngineError::IllegalAction)?,
    )
    .map_err(EngineError::serialization)?;
    if let Some(selection) = crate::v7_quantum_state::selection_from_ghost_click(state, from)? {
        let candidates = quantum_ui_actions(state, from)?;
        let mut physical_intent = intent.clone();
        physical_intent["from"] =
            serde_json::to_value(selection.origin).map_err(EngineError::serialization)?;
        return crate::movement::v7_resolve_public_move_from_candidates(
            state,
            &physical_intent,
            &candidates,
        );
    }
    let candidates = crate::movement::v7_source_ordered_move_candidates(state, false)?;
    require_source_family_budget(candidates.len(), "public movement")?;
    match crate::movement::v7_resolve_public_move_from_candidates(state, intent, &candidates) {
        Ok(action) => Ok(action),
        Err(EngineError::IllegalAction) => crate::movement::v7_resolve_public_move_from_candidates(
            state,
            intent,
            &friendly_fallback_candidates(state)?,
        ),
        Err(error) => Err(error),
    }
}

fn quantum_ui_actions(state: &GameState, clicked_origin: crate::Square) -> Result<Vec<Action>> {
    require_v7(state)?;
    if state.result().is_some() || state.mode == "gameover" {
        return Err(EngineError::Terminal);
    }
    if state.mode != "play"
        || crate::observation::truth(state.extra.get("turnResolving"))
        || crate::observation::truth(state.extra.get("pendingPromotion"))
        || crate::observation::truth(state.extra.get("activeTrolley"))
        || ui_card_completion(state)?.is_some()
    {
        return Err(EngineError::IllegalAction);
    }
    let selection = crate::v7_quantum_state::selection_from_ghost_click(state, clicked_origin)?
        .ok_or(EngineError::IllegalAction)?;
    if selection.piece.color != state.turn {
        return Err(EngineError::WrongActor);
    }
    if !crate::observation::piece_visible_to_color_at_v7(
        state,
        &selection.piece,
        clicked_origin,
        state.turn,
    )? {
        return Err(EngineError::IllegalAction);
    }
    let targets = crate::v7_quantum_state::legal_moves_for_quantum_selection(
        state,
        selection.origin,
        selection.ghost,
    )?;
    require_source_family_budget(targets.len(), "quantum UI movement")?;
    Ok(targets
        .into_iter()
        .map(|target| Action::movement(state.turn, selection.origin, target))
        .collect())
}

pub(crate) fn quantum_ui_public_intents(
    state: &GameState,
    action: &Action,
) -> Result<Option<Vec<Value>>> {
    let Some(target) = action.destination.as_ref() else {
        return Ok(None);
    };
    let Some(ghost) = target.flags.get("quantumFrom") else {
        return Ok(None);
    };
    let ghost: crate::Square =
        serde_json::from_value(ghost.clone()).map_err(EngineError::serialization)?;
    let physical = action.from.ok_or(EngineError::IllegalAction)?;
    let piece = state.at(physical).ok_or(EngineError::IllegalAction)?;
    let ordinary = crate::movement::v7_public_move_intents(state, action)?;
    let mut intents = Vec::new();
    for origin in crate::v7_quantum_state::quantum_cells_for_item_at(piece, ghost) {
        if !crate::v7_quantum_state::selection_from_ghost_click(state, origin)?
            .is_some_and(|selection| selection.origin == physical && selection.piece.id == piece.id)
        {
            continue;
        }
        for mut intent in ordinary.clone() {
            intent["from"] = serde_json::to_value(origin).map_err(EngineError::serialization)?;
            intents.push(intent);
        }
    }
    Ok(Some(intents))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;
    use serde_json::json;

    fn first_play(style: &str) -> GameState {
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: style.into(),
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        let offer_indices: &[usize] = match style {
            "normal" => &[1, 0],
            "chaos" => &[1, 2],
            "grand" => &[1, 3, 3, 3, 3, 3, 3, 3, 5, 5, 5, 5],
            _ => unreachable!(),
        };
        for &index in offer_indices {
            let pick = crate::draft::legal_actions(&state).unwrap().remove(index);
            crate::draft::apply_pick(&mut state, &pick).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        assert_eq!(state.mode, "play");
        state
    }

    fn ui_state(id: &str, field: &str) -> (GameState, crate::CardSlot) {
        let mut state = first_play("normal");
        let mut definition = crate::card_registry::definition_for(RULES_VERSION_V7, id)
            .unwrap()
            .source_definition
            .clone();
        definition["instanceId"] = json!("ui-owned-card");
        let card: crate::CardSlot = serde_json::from_value(definition).unwrap();
        state.deck_slots.white = vec![card.clone()];
        state.extra.insert(
            field.into(),
            json!({"card":serde_json::to_value(&card).unwrap(),"launch":null}),
        );
        (state, card)
    }

    #[test]
    fn native_ui_completion_retains_exact_card_and_direction_without_mutation() {
        let (mut state, card) = ui_state("barricade", "targeting");
        state.extra.get_mut("targeting").unwrap()["barricadeDirection"] = json!("vertical");
        let before = state.clone();
        let completion = ui_card_completion(&state).unwrap().unwrap();
        assert!(completion.matches_card(&card));
        assert!(completion.permits(&Action::card(
            Color::White,
            &card,
            Some(json!({"row":3,"col":3,"direction":"vertical"}))
        )));
        assert!(!completion.permits(&Action::card(
            Color::White,
            &card,
            Some(json!({"row":3,"col":3,"direction":"horizontal"}))
        )));
        let mut another = card.clone();
        another.instance_id = "another-owned-instance".into();
        assert!(!completion.permits(&Action::card(
            Color::White,
            &another,
            Some(json!({"row":3,"col":3,"direction":"vertical"}))
        )));
        assert_eq!(state, before);
    }

    #[test]
    fn ui_completion_rejects_conflicting_submitted_and_consumed_windows() {
        let (state, _) = ui_state("barricade", "barricadeDirectionChoice");
        let mut conflicting = state.clone();
        conflicting
            .extra
            .insert("jokerChoice".into(), json!({"card":{}}));
        assert!(matches!(
            ui_card_completion(&conflicting),
            Err(EngineError::InvalidState(_))
        ));
        let mut submitted = state.clone();
        submitted.extra.get_mut("barricadeDirectionChoice").unwrap()["submitting"] = json!(true);
        let before = submitted.clone();
        assert!(matches!(
            ui_card_completion(&submitted),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert_eq!(submitted, before);
        let mut consumed = state;
        consumed.deck_slots.white[0].used = true;
        let before = consumed.clone();
        assert!(matches!(
            ui_card_completion(&consumed),
            Err(EngineError::IllegalAction)
        ));
        assert_eq!(consumed, before);
    }

    #[test]
    fn ui_resume_starts_inside_retained_large_family_prefix() {
        let (mut state, _) = ui_state("hypocrisy", "targeting");
        state.board = vec![vec![None; 8]; 8];
        let prefix = json!({"row":7,"col":7});
        state.extra.get_mut("targeting").unwrap()["hypocrisy"] = json!([prefix]);
        let before = state.clone();
        let mut cursor = SourceActionCursor::new(&state, None).unwrap();
        let first = cursor.next_candidate().unwrap().unwrap();
        assert_eq!(first.card_id.as_deref(), Some("hypocrisy"));
        assert_eq!(first.target.as_ref().unwrap()["selections"][0], prefix);
        assert_eq!(
            first.target.as_ref().unwrap()["selections"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        assert_eq!(state, before);
    }

    #[test]
    fn promotion_fallback_distinguishes_falsy_choice_data_from_an_empty_array() {
        // Source adapter uses choices || promotionChoicesFor; an empty JS
        // array is truthy and must remain an empty declared decision set.
        for choices in [None, Some(Value::Null), Some(json!(false)), Some(json!([]))] {
            let mut state = first_play("normal");
            state.board[0][0] = state.board[6][0].take();
            let mut window = json!({"color":"white","row":0,"col":0});
            if let Some(value) = &choices {
                window["choices"] = value.clone();
            }
            state.extra.insert("pendingPromotion".into(), window);
            let before = state.clone();
            let offered: Vec<_> = special_candidates(&state)
                .unwrap()
                .into_iter()
                .map(|action| action.extra["promotionType"].as_str().unwrap().to_owned())
                .collect();
            if choices == Some(json!([])) {
                assert!(offered.is_empty());
            } else {
                assert_eq!(offered, ["queen", "rook", "bishop", "knight"]);
            }
            assert_eq!(state, before);
        }
    }

    #[test]
    fn exact_source_openings_have_complete_ordered_candidates() {
        for style in ["normal", "chaos", "grand"] {
            let state = first_play(style);
            let source = opening_for(style).unwrap();
            let actions = seed19_first_play_legal_actions(&state).unwrap();
            assert_eq!(actions.len(), source.legal_count, "{style}");
            assert_eq!(digest(&actions).unwrap(), source.legal_digest, "{style}");
            assert!(actions.iter().all(|action| action.position_key.is_none()));
        }
    }

    #[test]
    fn source_ordered_generic_candidate_path_matches_normal_opening() {
        let state = first_play("normal");
        let pinned = seed19_first_play_legal_actions(&state).unwrap();
        let candidates = ordered_play_candidates(&state).unwrap();
        assert_eq!(candidates, pinned);
        assert_eq!(
            digest(&candidates).unwrap(),
            opening_for("normal").unwrap().legal_digest
        );
    }

    #[test]
    fn changed_state_rng_history_and_rules_version_fail_closed() {
        let state = first_play("grand");
        let mut cases = Vec::new();
        let mut changed = state.clone();
        changed.rng.state ^= 1;
        cases.push(changed);
        let mut changed = state.clone();
        changed.history.push(json!({"public":{}}));
        cases.push(changed);
        let mut changed = state.clone();
        changed.ruleset_id = crate::RULES_VERSION_V6.into();
        cases.push(changed);
        let mut changed = state.clone();
        changed
            .extra
            .insert("cornerKick".into(), json!({"white":true,"black":false}));
        cases.push(changed);
        let mut changed = state.clone();
        changed.deck_slots.white[0].id = "unverified".into();
        cases.push(changed);
        for changed in cases {
            assert!(matches!(
                seed19_first_play_legal_actions(&changed),
                Err(EngineError::UnsupportedFeature(_))
            ));
        }
    }
}
