//! Source-pinned card definitions and the boundary between definitions, hand
//! instances, and active RULE cards. The legacy GameState remains a source DTO.
use crate::{
    CardSlot, Color, EngineError, GameState, RULES_VERSION_V6, RULES_VERSION_V7, Result, Sides,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

const CATALOG_VERSION: &str = "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4";
const V6_MAIN: &str = "abfe01a035813875772d8eeaf8e300a1df0348888ff48778d4a1789b76ae492f";
const V7_MAIN: &str = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CardType {
    Opening,
    Middle,
    End,
    Piece,
    Rule,
}

impl CardType {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "OPENING" => Ok(Self::Opening),
            "MIDDLE" => Ok(Self::Middle),
            "END" => Ok(Self::End),
            "PIECE" => Ok(Self::Piece),
            "RULE" => Ok(Self::Rule),
            _ => Err(EngineError::InvalidState(format!(
                "unknown card type {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CardActType {
    Passive,
    Active,
    #[allow(
        dead_code,
        reason = "forced opening activation is staged for the v7 move boundary"
    )]
    ActiveForced,
}

impl CardActType {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "PASSIVE" => Ok(Self::Passive),
            "ACTIVE" => Ok(Self::Active),
            _ => Err(EngineError::InvalidState(format!(
                "unknown card activation {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CardTurnPolicy {
    PreserveTurn,
    EndTurn,
    NotApplicable,
}

impl CardTurnPolicy {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "PRESERVE_TURN" => Ok(Self::PreserveTurn),
            "END_TURN" => Ok(Self::EndTurn),
            "NOT_APPLICABLE" => Ok(Self::NotApplicable),
            _ => Err(EngineError::InvalidState(format!(
                "unknown card turn policy {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectionKind {
    None,
    Squares,
    Choice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SelectionContract {
    pub(crate) kind: SelectionKind,
    pub(crate) min_squares: usize,
    pub(crate) max_squares: usize,
    pub(crate) choice_required: bool,
}

#[derive(Debug)]
pub(crate) struct CardDefinition {
    pub(crate) id: String,
    pub(crate) effect: String,
    /// The one auxiliary definition has no public card type or activation.
    pub(crate) card_type: Option<CardType>,
    pub(crate) activation: Option<CardActType>,
    pub(crate) turn_policy: CardTurnPolicy,
    pub(crate) selection: SelectionContract,
    pub(crate) source_definition: Value,
}

#[derive(Debug)]
pub(crate) struct CardRegistry {
    pub(crate) rules_version: &'static str,
    pub(crate) cards: BTreeMap<String, CardDefinition>,
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| EngineError::InvalidState(format!("card catalog {field} missing")))
}

fn required_array<'a>(value: &'a Value, field: &str) -> Result<&'a [Value]> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| EngineError::InvalidState(format!("card catalog {field} missing")))
}

fn selection(value: &Value) -> Result<SelectionContract> {
    let kind = match required_str(value, "kind")? {
        "none" => SelectionKind::None,
        "squares" => SelectionKind::Squares,
        "choice" => SelectionKind::Choice,
        kind => {
            return Err(EngineError::InvalidState(format!(
                "unknown card selection {kind}"
            )));
        }
    };
    let number = |field| -> Result<usize> {
        value
            .get(field)
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| EngineError::InvalidState(format!("card selection {field} invalid")))
    };
    let min_squares = number("minSquares")?;
    let max_squares = number("maxSquares")?;
    let choice_required = value
        .get("choiceRequired")
        .and_then(Value::as_bool)
        .ok_or_else(|| EngineError::InvalidState("card selection choiceRequired invalid".into()))?;
    if min_squares > max_squares
        || max_squares > 64
        || kind != SelectionKind::Squares && (min_squares != 0 || max_squares != 0)
        || kind == SelectionKind::None && choice_required
    {
        return Err(EngineError::InvalidState(
            "card selection bounds inconsistent".into(),
        ));
    }
    Ok(SelectionContract {
        kind,
        min_squares,
        max_squares,
        choice_required,
    })
}

impl CardRegistry {
    fn load(
        rules_version: &'static str,
        expected_main: &'static str,
        site_text: &str,
        definition_text: &str,
        draft_text: &str,
        presentation_text: Option<&str>,
    ) -> Result<Self> {
        let site: Value = serde_json::from_str(site_text).map_err(EngineError::serialization)?;
        let definitions: Value =
            serde_json::from_str(definition_text).map_err(EngineError::serialization)?;
        let draft: Value = serde_json::from_str(draft_text).map_err(EngineError::serialization)?;
        let presentation: Option<Value> = presentation_text
            .map(serde_json::from_str)
            .transpose()
            .map_err(EngineError::serialization)?;
        for document in [&site, &definitions, &draft] {
            if required_str(document, "rulesVersion")? != rules_version {
                return Err(EngineError::InvalidState(
                    "card catalog version mismatch".into(),
                ));
            }
        }
        if required_str(&site, "catalogVersion")? != CATALOG_VERSION
            || required_str(&definitions, "catalogVersion")? != CATALOG_VERSION
        {
            return Err(EngineError::InvalidState(
                "card catalog version mismatch".into(),
            ));
        }
        if required_str(&definitions, "sourceMainSha256")? != expected_main
            || site["source"]["files"].as_array().is_none_or(|files| {
                !files.iter().any(|file| {
                    file["sha256"] == expected_main
                        && file["name"]
                            .as_str()
                            .is_some_and(|name| name.starts_with("main-"))
                })
            })
        {
            return Err(EngineError::InvalidState(
                "card source main hash mismatch".into(),
            ));
        }
        let public = required_array(&site, "cards")?;
        let raw_definitions = required_array(&definitions, "definitions")?;
        let weights = required_array(&draft, "weights")?;
        let presented = if let Some(presentation) = &presentation {
            if presentation["schemaVersion"].as_u64() != Some(1)
                || presentation["sourceDefinitionCount"].as_u64() != Some(257)
                || required_str(presentation, "rulesVersion")? != rules_version
                || required_str(presentation, "catalogVersion")? != CATALOG_VERSION
                || required_str(presentation, "sourceMainSha256")? != expected_main
                || required_str(presentation, "sourceExpression")? != "JSON.stringify(CARD_DEFS)"
                || definitions["presentationFieldsExcluded"]
                    != serde_json::json!(["name", "text", "art"])
            {
                return Err(EngineError::InvalidState(
                    "v7 card presentation source identity mismatch".into(),
                ));
            }
            Some(required_array(presentation, "definitions")?)
        } else {
            None
        };
        if public.len() != 256
            || raw_definitions.len() != 257
            || weights.len() != 257
            || definitions["publicCatalogCardCount"].as_u64() != Some(256)
            || presented.is_some_and(|cards| cards.len() != raw_definitions.len())
        {
            return Err(EngineError::InvalidState(
                "card catalog count mismatch".into(),
            ));
        }
        let mut cards = BTreeMap::new();
        for (index, raw) in raw_definitions.iter().enumerate() {
            let id = required_str(raw, "id")?.to_owned();
            let effect = required_str(raw, "effect")?.to_owned();
            let mut source_definition = raw.clone();
            if let Some(cards) = presented {
                let presentation = &cards[index];
                if presentation["id"] != raw["id"]
                    || presentation["effect"] != raw["effect"]
                    || presentation["phase"] != raw["phase"]
                    || presentation["stars"] != raw["stars"]
                {
                    return Err(EngineError::InvalidState(format!(
                        "v7 card presentation definition mismatch for {id}"
                    )));
                }
                let fields = source_definition.as_object_mut().ok_or_else(|| {
                    EngineError::InvalidState("card source definition is not an object".into())
                })?;
                for field in ["name", "text", "art"] {
                    fields.insert(
                        field.into(),
                        serde_json::json!(required_str(presentation, field)?),
                    );
                }
            }
            if cards
                .insert(
                    id.clone(),
                    CardDefinition {
                        id,
                        effect,
                        card_type: None,
                        activation: None,
                        turn_policy: CardTurnPolicy::NotApplicable,
                        selection: SelectionContract {
                            kind: SelectionKind::None,
                            min_squares: 0,
                            max_squares: 0,
                            choice_required: false,
                        },
                        source_definition,
                    },
                )
                .is_some()
            {
                return Err(EngineError::InvalidState(
                    "duplicate card definition id".into(),
                ));
            }
        }
        let mut seen = BTreeSet::new();
        for raw in public {
            let id = required_str(raw, "id")?;
            let definition = cards.get_mut(id).ok_or_else(|| {
                EngineError::InvalidState(format!("public card {id} has no definition"))
            })?;
            if definition.effect != required_str(raw, "effect")? || !seen.insert(id) {
                return Err(EngineError::InvalidState(format!(
                    "public card {id} mismatch"
                )));
            }
            definition.card_type = Some(CardType::parse(required_str(raw, "draftCategory")?)?);
            definition.activation = Some(CardActType::parse(required_str(raw, "activation")?)?);
            definition.turn_policy = CardTurnPolicy::parse(required_str(raw, "turnPolicy")?)?;
            definition.selection = selection(&raw["selection"])?;
            if raw["enabled"] != true {
                return Err(EngineError::InvalidState(format!(
                    "disabled public card {id}"
                )));
            }
        }
        let mut weight_ids = BTreeSet::new();
        for raw in weights {
            let id = required_str(raw, "id")?;
            if !cards.contains_key(id) || !weight_ids.insert(id) {
                return Err(EngineError::InvalidState(format!(
                    "draft weight {id} mismatch"
                )));
            }
        }
        if weight_ids.len() != cards.len()
            || cards
                .values()
                .filter(|card| card.card_type.is_none())
                .count()
                != 1
        {
            return Err(EngineError::InvalidState(
                "card auxiliary definition mismatch".into(),
            ));
        }
        Ok(Self {
            rules_version,
            cards,
        })
    }

    pub(crate) fn get(&self, id: &str) -> Result<&CardDefinition> {
        self.cards.get(id).ok_or_else(|| {
            EngineError::InvalidState(format!(
                "unknown {} card definition {id}",
                self.rules_version
            ))
        })
    }
}

pub(crate) fn registry_for(rules_version: &str) -> Result<&'static CardRegistry> {
    static V6: OnceLock<Result<CardRegistry>> = OnceLock::new();
    static V7: OnceLock<Result<CardRegistry>> = OnceLock::new();
    let registry = match rules_version {
        RULES_VERSION_V6 => V6.get_or_init(|| {
            CardRegistry::load(
                RULES_VERSION_V6,
                V6_MAIN,
                include_str!("../../bridge/catalog/site-20260927.json"),
                include_str!("../../bridge/catalog/card-definitions-20260927.json"),
                include_str!("../../bridge/catalog/draft-20260927.json"),
                None,
            )
        }),
        RULES_VERSION_V7 => V7.get_or_init(|| {
            CardRegistry::load(
                RULES_VERSION_V7,
                V7_MAIN,
                include_str!("../../bridge/catalog/site-20260928.json"),
                include_str!("../../bridge/catalog/card-definitions-20260928.json"),
                include_str!("../../bridge/catalog/draft-20260928.json"),
                Some(include_str!(
                    "../../bridge/catalog/card-presentation-20260928.json"
                )),
            )
        }),
        other => {
            return Err(EngineError::UnsupportedFeature(format!(
                "card rules version {other}"
            )));
        }
    };
    registry.as_ref().map_err(Clone::clone)
}

pub(crate) fn definition_for(
    rules_version: &str,
    card_id: &str,
) -> Result<&'static CardDefinition> {
    registry_for(rules_version)?.get(card_id)
}

/// Opening RULE effect support is deliberately narrower than the 26-card
/// selection pool. The returned source object includes the reviewed v7
/// presentation projection, including acceleration's OPENING source phase.
pub(crate) fn opening_rule_source_card(card_id: &str) -> Result<Value> {
    if !matches!(card_id, "saturation" | "acceleration") {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 opening RULE effect {card_id}"
        )));
    }
    let definition = definition_for(RULES_VERSION_V7, card_id)?;
    if definition.card_type != Some(CardType::Rule)
        || definition.activation != Some(CardActType::Active)
        || definition.selection.kind != SelectionKind::None
        || ["name", "text", "art"]
            .into_iter()
            .any(|field| required_str(&definition.source_definition, field).is_err())
    {
        return Err(EngineError::InvalidState(format!(
            "v7 opening RULE source definition drift for {card_id}"
        )));
    }
    Ok(definition.source_definition.clone())
}

pub(crate) fn validate_instance(
    state: &GameState,
    card: &CardSlot,
) -> Result<&'static CardDefinition> {
    if card.vacant || card.id.is_empty() || card.instance_id.is_empty() {
        return Err(EngineError::InvalidState(
            "card instance identity missing".into(),
        ));
    }
    let definition = definition_for(&state.ruleset_id, &card.id)?;
    if definition.id != card.id || definition.effect != card.effect {
        return Err(EngineError::InvalidState(format!(
            "card instance {} definition mismatch",
            card.id
        )));
    }
    Ok(definition)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CardActionPolicy {
    pub(crate) actor: Color,
    pub(crate) slot: usize,
    pub(crate) activation: CardActType,
    pub(crate) use_cost: CardUseCost,
    /// Catalog metadata. `settle_policy` reproduces the live source branch,
    /// which also includes trolley/premove and conditional zugzwang.
    pub(crate) turn_policy: CardTurnPolicy,
}

/// The source `finishCard` marks and counts a normal hand card; a `devCard`
/// instance is explicitly exempt. The request cannot supply either cost.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CardUseCost {
    SpendInstance,
    Exempt,
}

impl CardUseCost {
    fn from_instance(card: &CardSlot) -> Self {
        if card.extra.get("devCard") == Some(&Value::Bool(true)) {
            Self::Exempt
        } else {
            Self::SpendInstance
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CardSettlePolicy {
    pub(crate) end_move: bool,
    pub(crate) clear_extra_actions: bool,
}

// The frozen main's usesTurnEndingZugzwang catalog allowlist. The current
// catalog is first; older accepted hashes remain for exact imported snapshots.
const ZUGZWANG_TURN_END_HASHES: &[&str] = &[
    CATALOG_VERSION,
    "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
    "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs",
    "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc",
    "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g",
    "HVzv4vxzNKDRi-ylshTw9HnLmIlyiQhMScQ4caG_aSI",
    "jkECTP8OBtMeXmZF7wmz1dmgw0mgTFn8jdD-d9LZhxU",
    "s0_j9SmUy1tSUcko_X3I32akB93Iz2bvV1Cn7Nw4Uoo",
    "_Hw8otJVztzWQyIN69bg5khIqcJv-au6UwAcOQhtKcM",
    "Vj80kM6RlfZvbMo9kevioRks2VbcDGk8bASi5uz0yNA",
    "MM-bvPG6PYiUQ0-UrCM3GmEbFfBMTyPxjKk0vFXbXj0",
    "abhSgcd4RVrr2b-bzPtaJo6gbqfUrYKk4IqsrW7oCO0",
    "jvr27l0Kk-YFld45zoyTV-MMOIAXuIXtubma6eKOPL4",
    "uIkywAndqkm8YawR1KMkyw_GROmXd4h1wpCXwH-0oYM",
    "RzvDge9_4q7il_D_pgjgO-vg2cu7Njx-VGcV_0ZeRLA",
    "Rq32ku1EGlC0GbWIZx5RTtxP43JwU2dz1dIrPWElXRM",
    "DxKZRNW24FynvEKBMzBDT7B1AH1e0BZFU-Ixpm1LtoM",
    "FMXnrO0g9t3Yd2TMDS2I9bbNBhbVEZaz7V93GzBAvOE",
    "2ltd5-M692bro1FgeSC0N8MnEEYR_QTEcxJwurJj2ME",
    "WaZMsX0HrVmtxwAakiwEjDS56uQtQqztvwLnJrv-sHE",
    "IFEPd1kgPLE5sPYp8yeRI_45n3sVsZZ4h3Y0MLemuyg",
    "OcuYVKEgBuAf8Pj5oy22E_YKE_1TeNMYdK3wMRWhnKU",
    "kf6NclPPEjBozgM7tKI4l0uSWrN2xvEmU7LphuWuTOU",
    "cK0OqbPFiHuGC_mnzI3hnbhkfFCNwI095p6BJ7ArWDA",
    "Fg_7NYite2mD8-JHvLIZ3hLWQ05d5S_DTriL5oUEgac",
    "iS-t2bAn7INQbziM4NlAUH3iC6Qj1LtD_IWROZOQQ94",
    "XdB_YEBKxUGZaEsR5xE8m5eMCvEbk13neAXnP_DWNEQ",
    "wPF6m7qHauYTW40uX7eo8A4oCj56Heqv5Gyp2p_WTss",
    "hJUd0w9MKj1xPXBdzToaAnQEKTsXEeihFhujhX-eWsk",
];

fn turn_ending_zugzwang(state: &GameState) -> bool {
    let card_state = state
        .extra
        .get("cardState")
        .filter(|value| crate::observation::truth(Some(value)));
    let profile = match card_state {
        Some(card_state) => card_state.get("profile"),
        None => state.extra.get("profile"),
    };
    let hash = profile
        .and_then(|value| value.get("catalogHash"))
        .filter(|value| crate::observation::truth(Some(value)));
    match hash {
        Some(Value::String(hash)) => ZUGZWANG_TURN_END_HASHES.contains(&hash.as_str()),
        Some(_) => false,
        None => state.extra.get("zugzwangConsumesTurn") != Some(&Value::Bool(false)),
    }
}

/// Frozen client finishCard branch, evaluated after the effect has run and at
/// the start of finishCard. `end_move` means call endMove even when acceleration
/// later retains the actor; only selected cards clear extra actions first.
pub(crate) fn settle_policy(state: &GameState, card: &CardSlot) -> Result<CardSettlePolicy> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 card settlement policy".into(),
        ));
    }
    validate_instance(state, card)?;
    let zugzwang = turn_ending_zugzwang(state)
        && (card.id == "zugzwang"
            || card.effect == "blackBox"
                && card.extra.get("boxRevealedCardId").and_then(Value::as_str) == Some("zugzwang"));
    let listed = matches!(
        card.id.as_str(),
        "shotgun-king"
            | "black-tower-legacy-magic"
            | "summon-colossus"
            | "trolley"
            | "premove"
            | "miracle"
            | "brainwash"
    );
    let end_move = state.mode == "play"
        && !crate::observation::truth(state.extra.get("simpleBoardEditorCardOverride"))
        && (listed || zugzwang);
    let clear_extra_actions = end_move
        && (matches!(
            card.id.as_str(),
            "trolley" | "premove" | "miracle" | "brainwash"
        ) || zugzwang);
    Ok(CardSettlePolicy {
        end_move,
        clear_extra_actions,
    })
}

/// A manual request is accepted only for an active instance in the actor's
/// hand. The returned turn policy belongs to the engine, never the request.
pub(crate) fn action_policy(state: &GameState, card: &CardSlot) -> Result<CardActionPolicy> {
    let definition = validate_instance(state, card)?;
    let (slot, owned) = state
        .deck_slots
        .get(state.turn)
        .iter()
        .enumerate()
        .find(|(_, owned)| {
            !owned.vacant && owned.id == card.id && owned.instance_id == card.instance_id
        })
        .ok_or(EngineError::IllegalAction)?;
    if owned != card
        || card.used
        || card.recovering
        || card.extra.get("nextTurnPending").and_then(Value::as_bool) == Some(true)
        || definition.activation != Some(CardActType::Active)
        || definition.card_type == Some(CardType::Rule)
    {
        return Err(EngineError::IllegalAction);
    }
    Ok(CardActionPolicy {
        actor: state.turn,
        slot,
        activation: CardActType::Active,
        use_cost: CardUseCost::from_instance(owned),
        turn_policy: definition.turn_policy,
    })
}

/// Source first-move resolution is a separate event from a manual card action.
#[allow(dead_code, reason = "v7 forced opening action integration is staged")]
pub(crate) fn forced_first_move_policy(
    state: &GameState,
    card: &CardSlot,
    actor: Color,
) -> Result<CardActionPolicy> {
    let definition = validate_instance(state, card)?;
    let (slot, owned) = state
        .deck_slots
        .get(actor)
        .iter()
        .enumerate()
        .find(|(_, owned)| {
            !owned.vacant && owned.id == card.id && owned.instance_id == card.instance_id
        })
        .ok_or(EngineError::IllegalAction)?;
    if definition.activation != Some(CardActType::Active)
        || definition.card_type != Some(CardType::Opening)
        || owned != card
        || card.used
        || card.recovering
        || card.extra.get("firstTurnCard").and_then(Value::as_bool) != Some(true)
        || *state.turns_taken.get(actor) != 0
        || state.flag("firstMoveCardsForced", actor)
    {
        return Err(EngineError::IllegalAction);
    }
    Ok(CardActionPolicy {
        actor,
        slot,
        activation: CardActType::ActiveForced,
        use_cost: CardUseCost::from_instance(owned),
        turn_policy: CardTurnPolicy::NotApplicable,
    })
}

#[allow(
    dead_code,
    reason = "typed hand state is staged for canonical Position admission"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CardInstance {
    pub(crate) definition_id: String,
    pub(crate) instance_id: String,
    pub(crate) owner: Color,
    pub(crate) slot: usize,
    pub(crate) used: bool,
    pub(crate) recovering: bool,
    pub(crate) next_turn_pending: bool,
}

#[allow(
    dead_code,
    reason = "active RULE projection is staged for canonical Position admission"
)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActiveRule {
    pub(crate) definition_id: String,
    pub(crate) order: usize,
    pub(crate) value: Value,
}

#[allow(
    dead_code,
    reason = "typed card state is staged for canonical Position admission"
)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CardState {
    pub(crate) hands: Sides<Vec<CardInstance>>,
    pub(crate) active_rules: Vec<ActiveRule>,
}

impl CardState {
    /// Read the source DTO into separate hand and active-rule collections.
    /// No rules are synthesized from card IDs in hand slots.
    #[allow(
        dead_code,
        reason = "typed card projection awaits v7 Position integration"
    )]
    pub(crate) fn from_legacy(state: &GameState) -> Result<Self> {
        let hand = |owner: Color| -> Result<Vec<CardInstance>> {
            state
                .deck_slots
                .get(owner)
                .iter()
                .enumerate()
                .filter(|(_, card)| !card.vacant)
                .map(|(slot, card)| {
                    validate_instance(state, card)?;
                    if card
                        .extra
                        .get("slot")
                        .and_then(Value::as_u64)
                        .is_some_and(|stored| stored != slot as u64)
                    {
                        return Err(EngineError::InvalidState("card slot index mismatch".into()));
                    }
                    Ok(CardInstance {
                        definition_id: card.id.clone(),
                        instance_id: card.instance_id.clone(),
                        owner,
                        slot,
                        used: card.used,
                        recovering: card.recovering,
                        next_turn_pending: card
                            .extra
                            .get("nextTurnPending")
                            .and_then(Value::as_bool)
                            == Some(true),
                    })
                })
                .collect()
        };
        let mut active_rules = Vec::new();
        let mut append = |value: &Value| -> Result<()> {
            let id = required_str(value, "id")?;
            let definition = definition_for(&state.ruleset_id, id)?;
            if definition.card_type != Some(CardType::Rule)
                || value
                    .get("effect")
                    .and_then(Value::as_str)
                    .is_some_and(|effect| effect != definition.effect)
            {
                return Err(EngineError::InvalidState(format!(
                    "active RULE {id} definition mismatch"
                )));
            }
            active_rules.push(ActiveRule {
                definition_id: id.into(),
                order: active_rules.len(),
                value: value.clone(),
            });
            Ok(())
        };
        if let Some(primary) = state
            .extra
            .get("appliedRuleCard")
            .filter(|value| !value.is_null())
        {
            append(primary)?;
        }
        if let Some(additional) = state.extra.get("additionalRuleCards") {
            for value in additional.as_array().ok_or_else(|| {
                EngineError::InvalidState("additionalRuleCards must be an array".into())
            })? {
                append(value)?;
            }
        }
        Ok(Self {
            hands: Sides::new(hand(Color::White)?, hand(Color::Black)?),
            active_rules,
        })
    }
}

#[cfg(test)]
#[path = "card_registry_tests.rs"]
mod tests;
