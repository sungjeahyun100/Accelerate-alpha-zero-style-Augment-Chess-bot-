//! Source-pinned v7 card objects that transform one piece or grant traits.
//!
//! The registry owns catalog identity; the host owns card cost and turn
//! settlement. This object owns target order, direct effect and RNG draws.
//! Frozen client SHA-256: e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c.

use super::*;

pub(super) const IDS: &[&str] = &[
    "grappler",
    "revolving-door",
    "don-quixote",
    "medium",
    "paladin",
    "octopus",
    "clockwork",
    "parrot",
    "thief",
    "amazon",
    "hook",
    "ordination",
    "nullification",
    "recurrence",
    "outpost",
    "loyalist",
    "parry",
    "trojan-horse",
    "suicide-bomber",
    "chimera",
    "basic-training",
    "holdout",
    "stealth",
    "frenzy",
    "vip",
    "stake",
    "empty-lunchbox",
    "witch-trial",
    "charge",
    "disarm",
    "severance",
    "inertia",
    "promotion-rush",
    "royal-shield",
    "poisoned-pawn",
    "reaper",
    "idol",
    "herald",
    "wizard",
    "constitutional-monarchy",
    "jester",
    "local-conscription",
    "assassin",
    "knightmaster",
    "standard-bearer",
    "log",
    "pegasus",
    "dragon",
    "grasshopper",
    "eastern-policy",
    "campfire",
    "princess",
    "hedgehog",
    "siege-ram",
    "magic-girl",
    "berserker",
    "slime",
    "siren",
    "trickster",
    "undead",
    "random-roulette",
];

// 공개 카드 256개와 Piece 객체 61개에는 보조 정의를 합산하지 않는다.
// shotgun-king은 source phase GUN/type 없음인 첫 이동 자동 카드다.
pub(super) const AUXILIARY_IDS: &[&str] = &["shotgun-king"];

pub(super) fn owns(id: &str) -> bool {
    IDS.contains(&id) || AUXILIARY_IDS.contains(&id)
}

macro_rules! card {
    ($id:literal, $effect:literal, $source:expr, $mutation:expr) => {
        CardRuleObject {
            id: $id,
            effect: $effect,
            plan: Plan {
                source: $source,
                mutation: $mutation,
            },
        }
    };
}

const OBJECTS: &[CardRuleObject] = &[
    card!(
        "grappler",
        "grappler",
        Source::NonRoyal("queen"),
        Mutation::Sacrificial("grappler")
    ),
    card!(
        "revolving-door",
        "revolvingDoor",
        Source::NonRoyal("rook"),
        Mutation::GenericTransform("revolvingDoor")
    ),
    card!(
        "don-quixote",
        "donQuixote",
        Source::NonRoyal("rook"),
        Mutation::GenericTransform("donQuixote")
    ),
    card!(
        "medium",
        "medium",
        Source::Minor,
        Mutation::GenericTransform("medium")
    ),
    card!(
        "paladin",
        "paladin",
        Source::NonRoyal("knight"),
        Mutation::InternalTransform("paladin")
    ),
    card!(
        "octopus",
        "octopus",
        Source::NonRoyal("rook"),
        Mutation::InternalTransform("octopus")
    ),
    card!(
        "clockwork",
        "clockwork",
        Source::MinorExcept("clockwork"),
        Mutation::InternalTransform("clockwork")
    ),
    card!(
        "parrot",
        "parrot",
        Source::NonRoyal("rook"),
        Mutation::InternalTransform("parrot")
    ),
    card!(
        "thief",
        "thief",
        Source::NonRoyal("queen"),
        Mutation::RandomThief
    ),
    card!(
        "amazon",
        "amazon",
        Source::QueenIdentity,
        Mutation::Sacrificial("amazon")
    ),
    card!(
        "hook",
        "hook",
        Source::QueenIdentity,
        Mutation::Sacrificial("hook")
    ),
    card!(
        "ordination",
        "ordination",
        Source::Exact("bishop"),
        Mutation::Sacrificial("cardinal")
    ),
    card!(
        "nullification",
        "nullification",
        Source::Exact(""),
        Mutation::Grant("nullification")
    ),
    card!(
        "recurrence",
        "recurrence",
        Source::Exact(""),
        Mutation::RandomRecurrence
    ),
    card!(
        "outpost",
        "outpost",
        Source::Exact(""),
        Mutation::Grant("outpostProtected")
    ),
    card!(
        "loyalist",
        "loyalist",
        Source::Exact(""),
        Mutation::Grant("loyalist")
    ),
    card!(
        "parry",
        "parry",
        Source::Exact(""),
        Mutation::Grant("parry")
    ),
    card!(
        "trojan-horse",
        "trojanHorse",
        Source::Exact("knight"),
        Mutation::Grant("trojanHorse")
    ),
    card!(
        "suicide-bomber",
        "suicideBomber",
        Source::Exact(""),
        Mutation::Grant("explosive")
    ),
    card!(
        "chimera",
        "chimera",
        Source::Exact(""),
        Mutation::Grant("chimera")
    ),
    card!(
        "basic-training",
        "basicTraining",
        Source::Exact(""),
        Mutation::Grant("basicTraining")
    ),
    card!(
        "holdout",
        "holdout",
        Source::Exact("pawn"),
        Mutation::Grant("holdoutPromotion")
    ),
    card!(
        "stealth",
        "stealth",
        Source::Exact(""),
        Mutation::Grant("hiddenFrom")
    ),
    card!(
        "frenzy",
        "frenzy",
        Source::Exact("pawn"),
        Mutation::Grant("frenzy")
    ),
    card!(
        "vip",
        "vip",
        Source::Exact("pawn"),
        Mutation::Grant("vipInvitation")
    ),
    card!(
        "stake",
        "stake",
        Source::Exact(""),
        Mutation::Grant("staked")
    ),
    card!(
        "empty-lunchbox",
        "emptyLunchbox",
        Source::Exact(""),
        Mutation::Grant("emptyLunchbox")
    ),
    card!(
        "witch-trial",
        "witchTrial",
        Source::Exact(""),
        Mutation::Grant("witchTrial")
    ),
    card!(
        "charge",
        "charge",
        Source::Exact("pawn"),
        Mutation::Grant("chargeRush")
    ),
    card!(
        "disarm",
        "disarm",
        Source::Exact(""),
        Mutation::Grant("disarmed")
    ),
    card!(
        "severance",
        "severance",
        Source::Exact(""),
        Mutation::Grant("severed")
    ),
    card!(
        "inertia",
        "inertia",
        Source::Exact(""),
        Mutation::Grant("inertia")
    ),
    card!(
        "promotion-rush",
        "promotionRush",
        Source::Exact(""),
        Mutation::Grant("promotionRushUntil")
    ),
    card!(
        "royal-shield",
        "royalShield",
        Source::Exact(""),
        Mutation::RandomShield
    ),
    card!(
        "poisoned-pawn",
        "poisonedPawn",
        Source::Exact("pawn"),
        Mutation::PoisonPawns
    ),
    card!(
        "reaper",
        "reaper",
        Source::QueenIdentity,
        Mutation::Transform("reaper")
    ),
    card!(
        "idol",
        "idol",
        Source::QueenIdentity,
        Mutation::Transform("idol")
    ),
    card!(
        "herald",
        "herald",
        Source::Exact("rook"),
        Mutation::Transform("herald")
    ),
    card!(
        "wizard",
        "wizard",
        Source::QueenIdentity,
        Mutation::Transform("wizard")
    ),
    card!(
        "constitutional-monarchy",
        "constitutionalMonarchy",
        Source::QueenIdentity,
        Mutation::Transform("primeMinister")
    ),
    card!(
        "jester",
        "jester",
        Source::QueenIdentity,
        Mutation::Transform("jester")
    ),
    card!(
        "local-conscription",
        "localConscription",
        Source::QueenIdentity,
        Mutation::Transform("recruiter")
    ),
    card!(
        "assassin",
        "assassin",
        Source::Exact("knight"),
        Mutation::Transform("assassin")
    ),
    card!(
        "knightmaster",
        "knightmaster",
        Source::Exact("knight"),
        Mutation::Transform("knightmaster")
    ),
    card!(
        "standard-bearer",
        "standardBearer",
        Source::Exact("pawn"),
        Mutation::Transform("standardBearer")
    ),
    card!(
        "log",
        "log",
        Source::Exact("pawn"),
        Mutation::Transform("log")
    ),
    card!(
        "pegasus",
        "pegasus",
        Source::Exact("rook"),
        Mutation::Transform("pegasus")
    ),
    card!(
        "dragon",
        "dragon",
        Source::Exact("rook"),
        Mutation::Transform("dragon")
    ),
    card!(
        "grasshopper",
        "grasshopper",
        Source::Minor,
        Mutation::Transform("grasshopper")
    ),
    card!(
        "eastern-policy",
        "easternPolicy",
        Source::Minor,
        Mutation::Transform("cannon")
    ),
    card!(
        "campfire",
        "campfire",
        Source::Minor,
        Mutation::GenericTransform("campfire")
    ),
    card!(
        "princess",
        "princess",
        Source::NonRoyal("rook"),
        Mutation::GenericTransform("princess")
    ),
    card!(
        "hedgehog",
        "hedgehog",
        Source::NonRoyal("queen"),
        Mutation::GenericTransform("hedgehog")
    ),
    card!(
        "siege-ram",
        "siegeRam",
        Source::NonRoyal("rook"),
        Mutation::GenericTransform("siegeRam")
    ),
    card!(
        "magic-girl",
        "magicGirl",
        Source::NonRoyal("rook"),
        Mutation::GenericTransform("magicGirl")
    ),
    card!(
        "berserker",
        "berserker",
        Source::NonRoyal("rook"),
        Mutation::GenericTransform("berserker")
    ),
    card!(
        "slime",
        "slime",
        Source::NonRoyal("rook"),
        Mutation::GenericTransform("slime")
    ),
    card!(
        "siren",
        "siren",
        Source::NonRoyal("queen"),
        Mutation::GenericTransform("siren")
    ),
    card!(
        "trickster",
        "trickster",
        Source::NonRoyal("rook"),
        Mutation::GenericTransform("trickster")
    ),
    card!(
        "undead",
        "undead",
        Source::NonRoyal("queen"),
        Mutation::GenericTransform("undead")
    ),
    card!(
        "random-roulette",
        "randomRoulette",
        Source::Exact(""),
        Mutation::RandomRoulette
    ),
];

pub(super) fn object(id: &str) -> Option<&'static CardRuleObject> {
    // 이 marker plan은 등록 identity용이며, 실제 효과는 아래의 전용
    // shotgunKing 분기가 소유한다. 일반 단일 기물 transform에 보내지 않는다.
    static SHOTGUN: CardRuleObject = card!(
        "shotgun-king",
        "shotgunKing",
        Source::Exact("king"),
        Mutation::Transform("shotgunKing")
    );
    if id == SHOTGUN.id {
        return Some(&SHOTGUN);
    }
    if !IDS.contains(&id) {
        return None;
    }
    OBJECTS.iter().find(|object| object.id == id)
}

fn checked_object(state: &GameState, card: &CardSlot) -> Result<&'static CardRuleObject> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 piece card {} outside v7 ruleset",
            card.id
        )));
    }
    let object = object(&card.id)
        .ok_or_else(|| EngineError::UnsupportedFeature(format!("v7 piece card {}", card.id)))?;
    let definition = crate::card_registry::validate_instance(state, card)?;
    if object.effect != card.effect || definition.effect != object.effect {
        return Err(EngineError::InvalidState(format!(
            "v7 piece card {} effect mismatch",
            card.id
        )));
    }
    super::validate_profile(state, object.plan)?;
    if !super::september26(state)
        && matches!(card.id.as_str(), "grappler" | "medium" | "poisoned-pawn")
    {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 piece card {} legacy target and effect profile",
            card.id
        )));
    }
    if card.id == "random-roulette"
        && super::catalog_hash(state).is_some_and(|hash| hash != super::CATALOG)
    {
        return Err(EngineError::UnsupportedFeature(
            "v7 random-roulette legacy outcome and initialization profile".into(),
        ));
    }
    Ok(object)
}

fn roulette_target(state: &GameState, piece: &Piece) -> bool {
    const MAJOR: &[&str] = &[
        "octopus",
        "grappler",
        "hedgehog",
        "princess",
        "bigBishop",
        "queen",
        "rook",
        "amazon",
        "man",
        "colossus",
        "bigRook",
        "herald",
        "jester",
        "hook",
        "primeMinister",
        "assassin",
        "windmill",
        "crown",
        "bear",
        "magicGirl",
        "berserker",
        "siren",
        "reaper",
        "undead",
    ];
    const PRE_REBALANCE: &[&str] = &[
        "clockwork",
        "octopus",
        "parrot",
        "recruiter",
        "wizard",
        "trickster",
    ];
    piece.color.owner().is_some()
        && !super::truthy(piece.extra.get("regencyHeir"))
        && !super::truthy(piece.extra.get("crownRoyal"))
        && if super::september26(state) {
            MAJOR.contains(&piece.kind.as_str())
        } else {
            PRE_REBALANCE.contains(&piece.kind.as_str())
                || MAJOR.contains(&piece.kind.as_str())
                    && !matches!(piece.kind.as_str(), "octopus" | "grappler")
        }
}

fn roulette_targets(state: &GameState) -> Result<Vec<Square>> {
    let mut targets = Vec::new();
    for (row, cells) in state.board.iter().enumerate() {
        for (col, piece) in cells.iter().enumerate() {
            if piece
                .as_ref()
                .is_some_and(|piece| roulette_target(state, piece))
            {
                let row = u8::try_from(row).map_err(|_| {
                    EngineError::InvalidState("v7 roulette board row exceeds action range".into())
                })?;
                let col = u8::try_from(col).map_err(|_| {
                    EngineError::InvalidState(
                        "v7 roulette board column exceeds action range".into(),
                    )
                })?;
                let square = Square { row, col };
                targets.push(square);
            }
        }
    }
    Ok(targets)
}

// main:3066-3123. The order is observable because randomChoice consumes one
// draw and indexes this exact filtered pool.
const ROULETTE_TYPES: &[&str] = &[
    "pawn",
    "knight",
    "bishop",
    "rook",
    "queen",
    "colossus",
    "bigRook",
    "protestant",
    "herald",
    "cannon",
    "fanatic",
    "primeMinister",
    "eagle",
    "amazon",
    "cardinal",
    "pegasus",
    "jester",
    "camel",
    "log",
    "hook",
    "grasshopper",
    "dragon",
    "man",
    "assassin",
    "knightmaster",
    "standardBearer",
    "recruiter",
    "squire",
    "checker",
    "checkerKing",
    "wizard",
    "alfil",
    "bat",
    "guard",
    "reaper",
    "idol",
    "babyBear",
    "lobster",
    "missionary",
    "bear",
    "windmill",
    "siegeRam",
    "magicGirl",
    "berserker",
    "slime",
    "siren",
    "trickster",
    "undead",
    "campfire",
    "hedgehog",
    "princess",
    "thief",
    "brutus",
    "clockwork",
    "parrot",
    "paladin",
    "octopus",
    "grappler",
    "revolvingDoor",
    "donQuixote",
    "medium",
];

const ROULETTE_TRANSIENT_FIELDS: &[&str] = &[
    "anchorRow",
    "anchorCol",
    "hp",
    "maxHp",
    "mana",
    "maxMana",
    "ammo",
    "maxAmmo",
    "facing",
    "gold",
    "windmillMode",
    "logDir",
    "logRollAfterTurn",
    "heraldJumpUnlocked",
    "bribed",
    "bribedRemaining",
    "quantum",
    "quantumNoCaptureUntil",
    "quantumFirstObservationFails",
    "babyBearGrowAtTurn",
    "babyBearGrowAtMove",
    "babyBearMoveAfterTurn",
    "bearRetaliationsRemaining",
    "tricksterMoveType",
    "tricksterPreviousAbilityForTurn",
    "reaperCaptures",
];

fn roulette_large(kind: &str) -> bool {
    matches!(kind, "colossus" | "bigRook" | "bigBishop")
}

fn reserved_roulette_cell(state: &GameState, square: Square) -> Result<bool> {
    for name in ["pendingScarecrows", "pendingLobsters"] {
        let Some(value) = state.extra.get(name) else {
            continue;
        };
        let entries = value
            .as_array()
            .ok_or_else(|| EngineError::InvalidState(format!("{name} must be an array")))?;
        for entry in entries {
            let matches = entry.get("row").and_then(Value::as_u64) == Some(u64::from(square.row))
                && entry.get("col").and_then(Value::as_u64) == Some(u64::from(square.col));
            if matches && (name == "pendingLobsters" || !super::truthy(entry.get("pieceId"))) {
                return Ok(true);
            }
        }
    }
    if let Some(value) = state.extra.get("blackHole") {
        let entries = value
            .as_array()
            .ok_or_else(|| EngineError::InvalidState("blackHole must be an array".into()))?;
        if entries.iter().any(|entry| {
            entry.get("row").and_then(Value::as_u64) == Some(u64::from(square.row))
                && entry.get("col").and_then(Value::as_u64) == Some(u64::from(square.col))
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn roulette_offset(state: &GameState, base: Square, dr: i16, dc: i16) -> Option<Square> {
    let row = usize::try_from(i16::from(base.row) + dr).ok()?;
    let col = usize::try_from(i16::from(base.col) + dc).ok()?;
    state.board.get(row)?.get(col)?;
    Some(Square {
        row: u8::try_from(row).ok()?,
        col: u8::try_from(col).ok()?,
    })
}

fn roulette_normalize_square(state: &GameState, selected: Square) -> Square {
    let Some(piece) = state.at(selected).filter(|piece| piece.is_large()) else {
        return selected;
    };
    let anchor = piece
        .extra
        .get("anchorRow")
        .and_then(Value::as_u64)
        .and_then(|row| u8::try_from(row).ok())
        .zip(
            piece
                .extra
                .get("anchorCol")
                .and_then(Value::as_u64)
                .and_then(|col| u8::try_from(col).ok()),
        )
        .map(|(row, col)| Square { row, col });
    anchor
        .filter(|anchor| {
            state
                .at(*anchor)
                .is_some_and(|candidate| candidate.is_large() && candidate.id == piece.id)
        })
        .unwrap_or(selected)
}

fn roulette_large_anchors(state: &GameState, origin: Square, piece: &Piece) -> Result<Vec<Square>> {
    let candidates = if piece.is_large() {
        vec![origin]
    } else {
        [(0, 0), (-1, 0), (0, -1), (-1, -1)]
            .into_iter()
            .filter_map(|(dr, dc)| roulette_offset(state, origin, dr, dc))
            .collect()
    };
    let mut anchors = Vec::new();
    for candidate in candidates {
        let Some(bottom_right) = roulette_offset(state, candidate, 1, 1) else {
            continue;
        };
        let cells = [
            candidate,
            Square {
                row: candidate.row,
                col: bottom_right.col,
            },
            Square {
                row: bottom_right.row,
                col: candidate.col,
            },
            bottom_right,
        ];
        let mut available = true;
        for cell in cells {
            if reserved_roulette_cell(state, cell)?
                || state.at(cell).is_some_and(|other| other.id != piece.id)
            {
                available = false;
                break;
            }
        }
        if available && !anchors.contains(&candidate) {
            anchors.push(candidate);
        }
    }
    Ok(anchors)
}

fn roulette_outcomes(
    state: &GameState,
    current: &str,
    allow_large: bool,
) -> Result<Vec<&'static str>> {
    let hash = super::catalog_hash(state);
    let newest_pool = hash.map_or_else(
        || {
            if super::truthy(state.extra.get("campaign")) {
                state.extra.get("revolvingDoorGuard") == Some(&Value::Bool(true))
            } else {
                state.extra.get("revolvingDoorGuard") != Some(&Value::Bool(false))
            }
        },
        |hash| hash == super::CATALOG || hash == "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
    );
    let mut outcomes = Vec::with_capacity(ROULETTE_TYPES.len());
    for &kind in ROULETTE_TYPES {
        if (!newest_pool && matches!(kind, "grappler" | "revolvingDoor" | "donQuixote" | "medium"))
            || kind == current
            || !allow_large && roulette_large(kind)
        {
            continue;
        }
        outcomes.push(kind);
    }
    if outcomes.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    Ok(outcomes)
}

fn initialize_roulette_piece(state: &mut GameState, piece: &mut Piece, kind: &str) -> Result<()> {
    for field in ROULETTE_TRANSIENT_FIELDS {
        piece.extra.shift_remove(*field);
    }
    piece.kind = kind.into();
    piece.moved = true;
    // main:3203의 초기화 정의를 Trickster 능력 초기화와 공동 사용한다.
    // Roulette는 기존 form 필드를 지운 뒤 결과 필드를 반드시 덮어쓴다.
    for (field, value) in super::random_roulette_initial_piece_state(state, kind)? {
        piece.extra.insert(field, value);
    }
    if kind == "trickster" {
        super::trickster_defaults(state, piece)?;
    }
    Ok(())
}

/// Raw first-click UI targets, preserving row-major board-cell order.
pub(super) fn ui_targets(state: &GameState, card: &CardSlot) -> Result<Vec<Square>> {
    let object = checked_object(state, card)?;
    if card.id == "shotgun-king" {
        return Ok(Vec::new());
    }
    if card.id == "random-roulette" {
        return roulette_targets(state);
    }
    if card.id == "loyalist" {
        // Source getTargetSquares is a raw UI highlight. The later effect can
        // reject a Slime loyalist; legal AI actions below use the narrower
        // execution predicate instead.
        let mut raw = Vec::new();
        for (row, cells) in state.board.iter().enumerate() {
            for (col, piece) in cells.iter().enumerate() {
                let Some(piece) = piece.as_ref() else {
                    continue;
                };
                let eligible = piece.color == state.turn
                    && !super::truthy(piece.extra.get("loyalist"))
                    && !state.royal_identity(piece)
                    && !piece.is_large()
                    && ![
                        "merchant",
                        "wall",
                        "football",
                        "blackHole",
                        "monster",
                        "coffin",
                    ]
                    .contains(&piece.kind.as_str());
                if eligible {
                    raw.push(Square {
                        row: u8::try_from(row).map_err(|_| {
                            EngineError::InvalidState(
                                "v7 loyalist board row exceeds action range".into(),
                            )
                        })?,
                        col: u8::try_from(col).map_err(|_| {
                            EngineError::InvalidState(
                                "v7 loyalist board column exceeds action range".into(),
                            )
                        })?,
                    });
                }
            }
        }
        return Ok(raw);
    }
    if matches!(card.id.as_str(), "thief" | "royal-shield" | "poisoned-pawn") {
        return Ok(Vec::new());
    }
    if card.id == "ordination" && super::all_targets(state, object.plan, true)?.len() < 2 {
        return Ok(Vec::new());
    }
    super::ui_targets(state, object.plan, false)
}

fn execution_targets(state: &GameState, object: &CardRuleObject) -> Result<Vec<Square>> {
    if object.id == "random-roulette" {
        return roulette_targets(state);
    }
    super::all_targets(
        state,
        object.plan,
        !matches!(
            object.plan.mutation,
            Mutation::PoisonPawns | Mutation::RandomShield
        ),
    )
}

/// Ordered public actions for this finite, bounded card family.
pub(super) fn actions(state: &GameState, card: &CardSlot) -> Result<Vec<Action>> {
    let object = checked_object(state, card)?;
    if card.id == "shotgun-king" {
        return Ok(vec![Action::card(state.turn, card, None)]);
    }
    let plan = object.plan;
    if matches!(
        plan.mutation,
        Mutation::RandomThief
            | Mutation::RandomRecurrence
            | Mutation::RandomShield
            | Mutation::PoisonPawns
    ) {
        if matches!(plan.mutation, Mutation::RandomThief | Mutation::PoisonPawns) {
            super::current_random_pool(state)?;
        }
        return Ok(if execution_targets(state, object)?.is_empty() {
            Vec::new()
        } else {
            vec![Action::card(state.turn, card, None)]
        });
    }
    if let Mutation::Sacrificial(kind) = plan.mutation {
        let (secondary_source, field) = match kind {
            "grappler" => (Source::Minor, "minor"),
            "amazon" => (Source::Exact("knight"), "knight"),
            "hook" => (Source::Exact("rook"), "rook"),
            "cardinal" => (Source::Exact("bishop"), ""),
            _ => {
                return Err(EngineError::UnsupportedFeature(format!(
                    "sacrificial card {}",
                    card.id
                )));
            }
        };
        let primary = super::ui_targets(state, plan, false)?;
        let secondary = super::all_targets(
            state,
            Plan {
                source: secondary_source,
                mutation: Mutation::Transform(""),
            },
            true,
        )?;
        let mut out = Vec::new();
        for first in primary {
            let first_piece = state.at(first).ok_or(EngineError::IllegalAction)?;
            for second in &secondary {
                if state
                    .at(*second)
                    .is_none_or(|piece| piece.id == first_piece.id)
                {
                    continue;
                }
                let mut target = json!(first);
                if field.is_empty() {
                    out.push(Action::card(state.turn, card, Some(target)));
                    break;
                }
                target[field] = json!(second);
                out.push(Action::card(state.turn, card, Some(target)));
            }
        }
        return Ok(out);
    }
    let legal_targets = if card.id == "loyalist" {
        execution_targets(state, object)?
    } else {
        ui_targets(state, card)?
    };
    Ok(legal_targets
        .into_iter()
        .map(|square| Action::card(state.turn, card, Some(json!(square))))
        .collect())
}

fn random_index(state: &mut GameState, len: usize) -> Result<usize> {
    // main:67540 randomChoice consumes Math.random even for an empty pool.
    // A Black Box decline keeps that draw for the next shuffled candidate.
    let value = if len == 0 {
        state
            .rng
            .sample_invariant("source empty piece card candidate")?
    } else {
        state.rng.sample()?
    };
    if !value.is_finite() || !(0.0..1.0).contains(&value) {
        return Err(EngineError::InvalidState(
            "v7 piece card random draw outside [0,1)".into(),
        ));
    }
    if len == 0 {
        return Err(EngineError::IllegalAction);
    }
    state
        .rng
        .record_last_probability(1.0 / len as f64, "source piece card candidate index")?;
    Ok((value * len as f64).floor() as usize)
}

fn shuffle(state: &mut GameState, cells: &mut [Square]) -> Result<()> {
    for index in (1..cells.len()).rev() {
        let destination = random_index(state, index + 1)?;
        cells.swap(index, destination);
    }
    Ok(())
}

/// Execute the direct effect against an isolated state. Host card-cost and
/// turn settlement are deliberately outside this object.
pub(super) fn apply(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let object = admit_effect_action(state, card, action)?;
    if state.is_ai_simulation() {
        // canResolveUntargetedCard의 호출자가 소유한 disposable clone에서
        // 원문 false 이전 draw/부분 효과를 보존한다. devCard를 바꾸지 않는다.
        return apply_inner(state, card, action, object);
    }
    let mut working = state.clone();
    let captured = apply_inner(&mut working, card, action, object)?;
    *state = working;
    Ok(captured)
}

/// Box가 보유한 devCard의 효과를 원문 그대로 가변 상태에 적용한다.
/// main:104736-104785는 실패 후보의 RNG와 부분 효과를 다음 후보에 넘기므로
/// 이 진입점은 state/card를 복제해 rollback하지 않는다. 공개 요청의 atomic
/// transaction과 applyCard의 비용·턴 정산은 호출자가 소유한다.
///
/// action/card identity·actor·devCard 계약 위반은 InvalidState다. target의
/// 효과 도메인 거절은 IllegalAction으로 남지만, 원문 false 재시도 증명은
/// 검증된 자동 타깃을 사용하는 Box caller에만 적용한다. 임의의 외부 target
/// 또는 RNG/상태 불변식 오류를 이 증명으로 정상 실패로 바꾸면 안 된다.
pub(super) fn apply_virtual_effect(
    state: &mut GameState,
    card: &mut CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    let object = admit_effect_action(state, card, action).map_err(|error| match error {
        EngineError::IllegalAction => EngineError::InvalidState(format!(
            "v7 virtual piece card {} action identity/admission mismatch",
            card.id
        )),
        error => error,
    })?;
    if card.extra.get("devCard") != Some(&Value::Bool(true)) {
        return Err(EngineError::InvalidState(format!(
            "v7 virtual piece card {} requires devCard=true",
            card.id
        )));
    }
    if matches!(object.plan.mutation, Mutation::RandomRoulette) {
        // Only immutable identity is copied to satisfy the separate mutable
        // result-card borrow; neither state nor result metadata is staged.
        let id = card.id.clone();
        let instance_id = card.instance_id.clone();
        apply_roulette(state, &id, &instance_id, action, Some(card))
    } else {
        apply_inner(state, card, action, object)
    }
}

/// 검증된 Box 자동 타깃의 IllegalAction이 원문 `{ok:false}`인 exact 경로.
/// catalog 등록 여부만으로 새 카드의 실패를 허용하지 않는다. envelope 검사는
/// raw entry 전에 끝나고, downstream 불변식 오류는 아래에서 InvalidState로
/// 구분한다. 이 증명은 임의의 malformed target을 재시도하라는 허가가 아니다.
pub(super) fn source_decline_retry_safe(card: &CardSlot) -> bool {
    let Some(object) = object(&card.id) else {
        return false;
    };
    if card.effect != object.effect || card.extra.get("devCard") != Some(&Value::Bool(true)) {
        return false;
    }
    match card.id.as_str() {
        // transformCardPiece:99699-99712 and exact applyCardEffect dispatch:
        // 100134-100146,100159-100173,100211-100228.
        "grappler" | "revolving-door" | "don-quixote" | "medium" | "campfire" | "princess"
        | "hedgehog" | "siege-ram" | "magic-girl" | "berserker" | "slime" | "siren"
        | "trickster" | "undead" => true,
        // applyThreeLocalCard:15652-15666; applyFiveLocalCard:15942-15961.
        "paladin" | "octopus" | "clockwork" | "parrot" => true,
        // Every guard precedes mutation in these exact effect functions:
        // idol:98949, knightmaster:103291, standardBearer:103301,
        // constitutionalMonarchy:103946, jester:104035, reaper:104069,
        // localConscription:105369, log:105493, pegasus:105544,
        // herald:105591, grasshopper:105727, dragon:105737,
        // assassin:105814, wizard:105824, easternPolicy:105379.
        "reaper"
        | "idol"
        | "herald"
        | "wizard"
        | "constitutional-monarchy"
        | "jester"
        | "local-conscription"
        | "assassin"
        | "knightmaster"
        | "standard-bearer"
        | "log"
        | "pegasus"
        | "dragon"
        | "grasshopper"
        | "eastern-policy" => true,
        // The held primary object and post-sacrifice failure order are
        // explicitly preserved below: amazon:105509, ordination:105525,
        // hook:105565. These are not blanket capture-error attestations.
        "amazon" | "hook" | "ordination" => true,
        // RNG-before-empty-false: thief:100151-100161, recurrence:99732,
        // royalShield:105706. PoisonedPawn:100639 rejects before shuffle.
        "thief" | "recurrence" | "royal-shield" | "poisoned-pawn" => true,
        // Direct grants reject only before writing the trait:
        // frenzy:99471, vip:99493, loyalist:99595, parry:99645,
        // emptyLunchbox:99677, outpost/nullification:99718-99746,
        // charge:103239, witchTrial:103283, disarm:103355,
        // severance/inertia:103385-103402, trojanHorse:104080,
        // basicTraining:104548, chimera/holdout:105224-105246,
        // stake:105357, suicideBomber:105390, stealth:105604,
        // promotionRush:105623. Holdout's UI predicate is stricter than
        // its effect and therefore is not used as the effect guard below.
        "nullification" | "outpost" | "loyalist" | "parry" | "trojan-horse" | "suicide-bomber"
        | "chimera" | "basic-training" | "holdout" | "stealth" | "frenzy" | "vip" | "stake"
        | "empty-lunchbox" | "witch-trial" | "charge" | "disarm" | "severance" | "inertia"
        | "promotion-rush" => true,
        // main:104274-104342. Guard declines happen before mutation. A
        // failed engine invariant is kept explicit, not converted to the
        // source's caught DOM/runtime exception branch.
        "random-roulette" => true,
        _ => false,
    }
}

fn effect_invariant(error: EngineError, card_id: &str, stage: &str) -> EngineError {
    match error {
        EngineError::IllegalAction => EngineError::InvalidState(format!(
            "v7 piece card {card_id} unexpected downstream IllegalAction at {stage}"
        )),
        error => error,
    }
}

fn admit_effect_action(
    state: &GameState,
    card: &CardSlot,
    action: &Action,
) -> Result<&'static CardRuleObject> {
    let object = checked_object(state, card)?;
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
    Ok(object)
}

fn active_regency_heir(state: &GameState, piece: &Piece) -> bool {
    super::truthy(piece.extra.get("regencyHeir"))
        && super::truthy(
            state
                .extra
                .get("kingDead")
                .and_then(|sides| sides.get(piece.color.as_str())),
        )
        && super::truthy(
            state
                .extra
                .get("regency")
                .and_then(|sides| sides.get(piece.color.as_str())),
        )
}

fn piece_effect_square_name(state: &GameState, square: Square) -> String {
    // main:49259,109914,110028: a..l 이후에는 x13,x14,...를 사용한다.
    let file = if square.col < 12 {
        char::from(b'a' + square.col).to_string()
    } else {
        format!("x{}", usize::from(square.col) + 1)
    };
    format!("{file}{}", state.board.len() - usize::from(square.row))
}

/// main:103954-103990. 획득 처리 자체는 PASSIVE가 아니며, 호출자는 첫 이동
/// 자동 실행·undo 복원·카드 사용 정산을 별도로 수행한다. 제거는 포획 callback이
/// 아니므로 captures/왕실 패배/예언/경계·사신 반응을 호출하지 않는다.
fn apply_shotgun_king(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    if super::has_card_selection(action) {
        // shotgunKing은 source-false를 반환하지 않는다. private 계약 위반을
        // IllegalAction으로 섞으면 Black Box가 정상 false로 오인할 수 있다.
        return Err(EngineError::InvalidState(
            "v7 shotgunKing expects an absent or null target".into(),
        ));
    }
    let rows = state.board.len();
    let cols = state.board.first().map_or(0, Vec::len);
    if rows == 0
        || cols == 0
        || rows > 256
        || cols > 256
        || state.board.iter().any(|row| row.len() != cols)
    {
        return Err(EngineError::InvalidState(
            "v7 shotgunKing requires a nonempty rectangular board within action coordinates".into(),
        ));
    }
    let actor = state.turn;
    let mut recipient = None;
    for (row, cells) in state.board.iter().enumerate() {
        for (col, item) in cells.iter().enumerate() {
            let Some(item) = item.as_ref().filter(|item| item.color == actor) else {
                continue;
            };
            if item.id.is_empty() {
                return Err(EngineError::InvalidState(
                    "v7 shotgunKing friendly piece identity missing".into(),
                ));
            }
            if recipient.is_none()
                && (active_regency_heir(state, item)
                    || crate::v7_board_hazards::source_royal_king(state, item)?)
            {
                recipient = Some((
                    Square {
                        row: row as u8,
                        col: col as u8,
                    },
                    item.clone(),
                ));
            }
        }
    }
    let landing = recipient.as_ref().map_or(
        Square {
            row: if actor == Color::White {
                (rows - 1) as u8
            } else {
                0
            },
            col: (cols / 2).min(cols - 1) as u8,
        },
        |(square, _)| *square,
    );
    let mut retained = recipient
        .filter(|(_, piece)| active_regency_heir(state, piece))
        .map(|(_, piece)| piece);
    let retained_id = retained.as_ref().map(|piece| piece.id.as_str());
    for cell in state.board.iter_mut().flatten() {
        if cell
            .as_ref()
            .is_some_and(|piece| piece.color == actor && Some(piece.id.as_str()) != retained_id)
        {
            *cell = None;
        }
    }
    let deadline = u64::from(*state.turns_taken.get(actor)) + 1;
    if let Some(ref mut heir) = retained {
        // 원문은 객체를 새로 만들지 않는다. 큰 기물의 alias도 남고 기존 trait,
        // origin·shielded 등은 유지하며 아래 Shotgun 필드만 덮어쓴다.
        heir.kind = "shotgunKing".into();
        for (field, value) in [
            ("hp", json!(4)),
            ("maxHp", json!(4)),
            (
                "facing",
                json!(if actor == Color::White { "up" } else { "down" }),
            ),
            ("ammo", json!(3)),
            ("maxAmmo", json!(3)),
            ("freshNoCaptureUntil", json!(deadline)),
        ] {
            heir.extra.insert(field.into(), value);
        }
        heir.moved = true;
        crate::v7_board_hazards::replace_object_aliases(state, heir);
    } else {
        // piece(color,"shotgunKing")는 새 identity에 정확히 RNG 1회를 쓴다.
        // 왕이 없으면 home fallback의 적 기물도 callback 없이 덮어쓴다.
        let mut royal = crate::opening::spawn(state, actor, "shotgunKing")?;
        royal
            .extra
            .insert("freshNoCaptureUntil".into(), json!(deadline));
        royal.extra.insert(
            "origin".into(),
            json!(piece_effect_square_name(state, landing)),
        );
        royal.moved = true;
        state.board[usize::from(landing.row)][usize::from(landing.col)] = Some(royal);
    }
    // playSound("shotgunReload")는 frozen oracle의 presentation no-op이다.
    // move sound/threat probe는 여기서 추가하지 않는다.
    Ok(Vec::new())
}

/// main:102334-102359의 포획 후 초월 변환. 공통 전이는 HP 피해·포획·Reaper
/// 순서를 먼저 처리하고, 그 시점의 살아 있는 객체와 포획 전 source_type을
/// 전달한다. 실패한 바깥 transaction의 rollback은 호출자가 소유한다.
pub(super) fn apply_transcendence_capture_upgrade(
    state: &mut GameState,
    piece: &mut Piece,
    landing: Square,
    source_type: &str,
) -> Result<Option<String>> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 Transcendence capture upgrade outside v7 ruleset".into(),
        ));
    }
    if !super::truthy(state.extra.get("transcendenceRule"))
        || state.at(landing).is_none_or(|live| live.id != piece.id)
    {
        return Ok(None);
    }
    if piece.id.is_empty() {
        return Err(EngineError::InvalidState(
            "v7 Transcendence active object identity missing".into(),
        ));
    }
    let roll = if state.ai_simulation_depth > 0 {
        // source aiSimulationDepth > 0의 후보 조사에는 RNG를 쓰지 않는다.
        // kingThreatProbeDepth만 있는 실행은 upgrade와 notation draw를 유지한다.
        ((u64::from(state.move_count) + u64::from(landing.row) * 7 + u64::from(landing.col) * 11)
            % 2) as f64
            * 0.5
    } else {
        // main:102334에서 pawn만 roll로 분기하며 나머지 타입은 값이 쓰이지 않는다.
        let roll = if source_type == "pawn" {
            state.rng.sample()?
        } else {
            state
                .rng
                .sample_invariant("source deterministic Transcendence upgrade")?
        };
        if !roll.is_finite() || !(0.0..1.0).contains(&roll) {
            return Err(EngineError::InvalidState(
                "v7 Transcendence random draw outside [0,1)".into(),
            ));
        }
        if source_type == "pawn" {
            state
                .rng
                .record_last_probability(0.5, "source Transcendence pawn upgrade")?;
        }
        roll
    };
    // 원문은 지원하는 upgrade가 없는 queen/변형 기물도 먼저 1회 draw한다.
    let next = match source_type {
        "pawn" if roll < 0.5 => "knight",
        "pawn" => "bishop",
        "knight" | "bishop" => "rook",
        "rook" => "queen",
        _ => return Ok(None),
    };
    let next = if next == "knight" && super::truthy(state.extra.get("monochromeChess")) {
        "camel"
    } else {
        next
    };
    for field in [
        "windmillMode",
        "logDir",
        "logRollAfterTurn",
        "mana",
        "maxMana",
        "ammo",
        "maxAmmo",
        "facing",
    ] {
        piece.extra.shift_remove(field);
    }
    piece.kind = next.into();
    piece.moved = true;
    // markTransformedOrigin의 exact cleanup이며 hp/anchor/shielded/traits는
    // clearChimeraTypeState에서 삭제하는 필드가 아니므로 그대로 유지한다.
    piece.extra.shift_remove("vipInvitation");
    piece.extra.shift_remove("holdoutPromotion");
    piece.extra.insert(
        "origin".into(),
        json!(piece_effect_square_name(state, landing)),
    );
    if super::truthy(state.extra.get("monochromeChess"))
        && !super::truthy(piece.extra.get("monoShade"))
    {
        piece.extra.insert(
            "monoShade".into(),
            json!(
                if (u16::from(landing.row) + u16::from(landing.col)).is_multiple_of(2) {
                    "light"
                } else {
                    "dark"
                }
            ),
        );
    }
    if let Some(owner) = piece.color.owner() {
        piece.extra.insert(
            "freshNoCaptureUntil".into(),
            json!(u64::from(*state.turns_taken.get(owner)) + 1),
        );
    }
    crate::v7_board_hazards::replace_object_aliases(state, piece);
    super::mark_animation(state, piece)?;
    crate::replay::amend_pending_piece_change_notation(state, next, "초월")?;
    let mut redactions = serde_json::Map::new();
    for viewer in [Color::White, Color::Black] {
        if piece.color != viewer
            && !crate::observation::piece_visible_to_color_at_v7(state, piece, landing, viewer)?
        {
            redactions.insert(
                viewer.as_str().into(),
                json!({"text":"???","description":"기물이 행동했습니다."}),
            );
        }
    }
    let owner = piece.color.owner().ok_or_else(|| {
        EngineError::UnsupportedFeature(
            "v7 Transcendence notation for a neutral active object".into(),
        )
    })?;
    let previous_label = crate::replay::source_piece_label(source_type)
        .filter(|label| !label.is_empty())
        .unwrap_or(source_type);
    let next_label = crate::replay::source_piece_label(next)
        .filter(|label| !label.is_empty())
        .unwrap_or(next);
    let event = crate::replay::queue_special_effect_notation(
        state,
        owner,
        "초월",
        &format!(
            "{} {previous_label}이 포획 후 {next_label}(으)로 초월",
            crate::replay::label(owner),
        ),
    )?;
    crate::replay::attach_notation_redactions(state, event, redactions)?;
    // playSound("promote")는 oracle presentation hook이라 probe/RNG 없음.
    crate::replay::add_piece_action_log(
        state,
        piece,
        Some(landing),
        None,
        format!(
            "초월: {}의 {previous_label}이 {next_label}(으)로 변했습니다.",
            piece_effect_square_name(state, landing),
        ),
    )?;
    Ok(Some(next.into()))
}

fn apply_inner(
    state: &mut GameState,
    card: &CardSlot,
    action: &Action,
    object: &CardRuleObject,
) -> Result<Vec<Piece>> {
    if card.id == "shotgun-king" {
        return apply_shotgun_king(state, action);
    }
    let plan = object.plan;
    if matches!(plan.mutation, Mutation::RandomRoulette) {
        return apply_roulette(state, &card.id, &card.instance_id, action, None);
    }
    let mut captured = Vec::new();
    let mut retained_primary = None;
    let mut retained_capture_starts = None;
    let selected = match plan.mutation {
        Mutation::RandomThief
        | Mutation::RandomRecurrence
        | Mutation::RandomShield
        | Mutation::PoisonPawns => {
            if super::has_card_selection(action) {
                return Err(EngineError::IllegalAction);
            }
            if matches!(plan.mutation, Mutation::RandomThief | Mutation::PoisonPawns) {
                super::current_random_pool(state)?;
            }
            let mut targets = execution_targets(state, object)?;
            if targets.is_empty() && matches!(plan.mutation, Mutation::PoisonPawns) {
                return Err(EngineError::IllegalAction);
            }
            if matches!(plan.mutation, Mutation::PoisonPawns) {
                shuffle(state, &mut targets)?;
                for square in targets.into_iter().take(4) {
                    let mut piece = state.at(square).cloned().ok_or_else(|| {
                        EngineError::InvalidState(
                            "v7 poisoned pawn target disappeared after shuffle".into(),
                        )
                    })?;
                    if piece.id.is_empty() {
                        return Err(EngineError::InvalidState(
                            "poisoned pawn target identity missing".into(),
                        ));
                    }
                    piece.extra.insert("poisonedPawn".into(), json!(true));
                    super::write_piece(state, &piece);
                }
                return Ok(Vec::new());
            }
            // randomChoice(empty) still consumes a draw before false;
            // poisonedPawn's empty guard above consumes none.
            let index = random_index(state, targets.len())?;
            targets[index]
        }
        Mutation::Sacrificial(kind) => {
            let (primary, secondary) = super::sacrificial_target(state, action, plan, kind)?;
            // main:105509,105525,105565 hold the primary JS object across
            // sacrifice callbacks. Grappler:100134 instead re-reads its
            // board target through transformCardPiece after the sacrifice.
            let held = state.at(primary).cloned().ok_or_else(|| {
                EngineError::InvalidState(format!(
                    "v7 piece card {} primary disappeared after admission",
                    card.id
                ))
            })?;
            let capture_starts = [state.captures.white.len(), state.captures.black.len()];
            let removed = if kind == "grappler" {
                crate::transition::expansion_sacrifice(state, secondary, state.turn)
            } else {
                crate::transition::sacrifice(state, secondary, state.turn.opponent())
            }
            .map_err(|error| effect_invariant(error, &card.id, "sacrifice callback"))?
            .ok_or(EngineError::IllegalAction)?;
            captured.push(removed);
            if kind == "grappler" {
                let piece = state.at(primary).ok_or(EngineError::IllegalAction)?;
                if !super::source_matches(state, piece, Source::NonRoyal("queen")) {
                    return Err(EngineError::IllegalAction);
                }
            } else {
                // Keep callback changes to the same object, including a
                // newly captured primary. A replacement at the same cell
                // is a different object and must not be transformed.
                retained_primary = Some(
                    state
                        .board
                        .iter()
                        .flatten()
                        .flatten()
                        .find(|piece| piece.id == held.id)
                        .or_else(|| {
                            state
                                .captures
                                .white
                                .iter()
                                .skip(capture_starts[0])
                                .find(|piece| piece.id == held.id)
                        })
                        .or_else(|| {
                            state
                                .captures
                                .black
                                .iter()
                                .skip(capture_starts[1])
                                .find(|piece| piece.id == held.id)
                        })
                        .cloned()
                        .unwrap_or(held),
                );
                retained_capture_starts = Some(capture_starts);
            }
            primary
        }
        _ => {
            let square = super::target_square(action)?;
            let square = if matches!(
                plan.mutation,
                Mutation::Grant("nullification" | "outpostProtected")
            ) {
                super::normalize_square(state, square)
            } else {
                square
            };
            let piece = state.at(square).ok_or(EngineError::IllegalAction)?;
            // main:105974 excludes existing holdout from UI choices;
            // holdout:105233 itself permits refreshing an own pawn.
            let matches = if card.id == "holdout" {
                piece.color == state.turn && piece.kind == "pawn"
            } else {
                super::matches_plan(state, piece, square, plan)
                    .map_err(|error| effect_invariant(error, &card.id, "target predicate"))?
            };
            if !matches {
                return Err(EngineError::IllegalAction);
            }
            square
        }
    };
    let mut piece = match retained_primary {
        Some(piece) => piece,
        None => state.at(selected).cloned().ok_or_else(|| {
            EngineError::InvalidState(format!(
                "v7 piece card {} selected target disappeared",
                card.id
            ))
        })?,
    };
    if piece.id.is_empty() {
        return Err(EngineError::InvalidState(format!(
            "v7 piece card {} target identity missing",
            card.id
        )));
    }
    match plan.mutation {
        Mutation::Transform(_) | Mutation::GenericTransform(_) | Mutation::InternalTransform(_) => {
            super::transform(state, &mut piece, selected, plan.mutation)
                .map_err(|error| effect_invariant(error, &card.id, "transform"))?;
        }
        Mutation::Sacrificial(kind) => {
            let mutation = if kind == "grappler" {
                Mutation::GenericTransform(kind)
            } else {
                Mutation::Transform(kind)
            };
            super::transform(state, &mut piece, selected, mutation)
                .map_err(|error| effect_invariant(error, &card.id, "sacrificial transform"))?;
        }
        Mutation::Grant(field) => {
            if field == "chargeRush" {
                for other in state
                    .board
                    .iter_mut()
                    .flatten()
                    .flatten()
                    .filter(|other| other.color == state.turn)
                {
                    other.extra.shift_remove("chargeRush");
                }
            }
            super::grant(state, &mut piece, field)
                .map_err(|error| effect_invariant(error, &card.id, "grant"))?;
        }
        Mutation::RandomThief => {
            super::transform(
                state,
                &mut piece,
                selected,
                Mutation::GenericTransform("thief"),
            )
            .map_err(|error| effect_invariant(error, &card.id, "thief transform"))?;
            piece.extra.insert("submerged".into(), json!(true));
            piece
                .extra
                .insert("wanted".into(), json!({"by":state.turn}));
        }
        Mutation::RandomRecurrence => {
            piece.extra.insert("recurrence".into(), json!(true));
            super::mark_animation(state, &piece)
                .map_err(|error| effect_invariant(error, &card.id, "recurrence animation"))?;
        }
        Mutation::RandomShield => super::grant(state, &mut piece, "shielded")
            .map_err(|error| effect_invariant(error, &card.id, "random shield"))?,
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "v7 piece card {} direct effect",
                card.id
            )));
        }
    }
    super::write_piece(state, &piece);
    if let Some(starts) = retained_capture_starts {
        // Source capture pools retain object references. Limit this write
        // to entries added by this sacrifice; earlier capture snapshots
        // with a resurrected identity are not the held primary reference.
        for (color, start) in [(Color::White, starts[0]), (Color::Black, starts[1])] {
            for captured in state.captures.get_mut(color).iter_mut().skip(start) {
                if captured.id == piece.id {
                    *captured = piece.clone();
                }
            }
        }
    }
    if matches!(plan.mutation, Mutation::Transform("herald")) {
        crate::transition::resolve_herald_for_color(state, state.turn)
            .map_err(|error| effect_invariant(error, &card.id, "herald settlement"))?;
    }
    Ok(captured)
}

fn apply_roulette(
    state: &mut GameState,
    card_id: &str,
    instance_id: &str,
    action: &Action,
    virtual_card: Option<&mut CardSlot>,
) -> Result<Vec<Piece>> {
    let selected = super::target_square(action)?;
    let origin = roulette_normalize_square(state, selected);
    let mut piece = state
        .at(origin)
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    if !roulette_target(state, &piece) {
        return Err(EngineError::IllegalAction);
    }
    if piece.id.is_empty() {
        return Err(EngineError::InvalidState(
            "v7 roulette target identity missing".into(),
        ));
    }
    let anchors = roulette_large_anchors(state, origin, &piece)?;
    let outcomes = roulette_outcomes(state, &piece.kind, !anchors.is_empty())?;
    let chosen = outcomes[random_index(state, outcomes.len())?];
    let result_kind = if super::truthy(state.extra.get("monochromeChess")) && chosen == "knight" {
        "camel"
    } else {
        chosen
    };
    let destination = if roulette_large(result_kind) {
        *anchors
            .get(random_index(state, anchors.len())?)
            .ok_or_else(|| EngineError::InvalidState("v7 roulette sampled anchor missing".into()))?
    } else {
        origin
    };
    let previous_kind = piece.kind.clone();
    let target_color = piece.color;
    initialize_roulette_piece(state, &mut piece, result_kind)
        .map_err(|error| effect_invariant(error, card_id, "roulette initialization"))?;
    for cell in state.board.iter_mut().flatten() {
        if cell
            .as_ref()
            .is_some_and(|candidate| candidate.id == piece.id)
        {
            *cell = None;
        }
    }
    if roulette_large(result_kind) {
        piece
            .extra
            .insert("anchorRow".into(), json!(destination.row));
        piece
            .extra
            .insert("anchorCol".into(), json!(destination.col));
        if super::truthy(state.extra.get("monochromeChess"))
            && !super::truthy(piece.extra.get("monoShade"))
        {
            piece.extra.insert(
                "monoShade".into(),
                json!(if (destination.row + destination.col).is_multiple_of(2) {
                    "light"
                } else {
                    "dark"
                }),
            );
        }
        let bottom_right = roulette_offset(state, destination, 1, 1).ok_or_else(|| {
            EngineError::InvalidState("v7 roulette admitted large anchor outside board".into())
        })?;
        for row in destination.row..=bottom_right.row {
            for col in destination.col..=bottom_right.col {
                state.board[row as usize][col as usize] = Some(piece.clone());
            }
        }
    } else {
        state.board[destination.row as usize][destination.col as usize] = Some(piece.clone());
    }
    super::mark_transformed_origin(state, &mut piece, destination)
        .map_err(|error| effect_invariant(error, card_id, "roulette origin"))?;
    super::mark_animation(state, &piece)
        .map_err(|error| effect_invariant(error, card_id, "roulette animation"))?;
    super::write_piece(state, &piece);
    if let Some(temporary) = state.extra.get_mut("temporaryQueens")
        && let Some(temporary) = temporary.as_array_mut()
    {
        temporary.retain(|entry| entry.get("id").and_then(Value::as_str) != Some(&piece.id));
    }
    // The source resolves campaign objectives after the board and temporary
    // queen cleanup, before it emits the visual event or publishes the card
    // result. A roulette transformation can therefore end a campaign.
    crate::v7_capture_objectives::check_campaign_objectives(state)
        .map_err(|error| effect_invariant(error, card_id, "roulette campaign"))?;
    crate::replay::queue_visual(
        state,
        json!({"type":"random-roulette","color":state.turn,"targetColor":target_color,
            "previousType":previous_kind,"resultType":result_kind}),
    )
    .map_err(|error| effect_invariant(error, card_id, "roulette replay"))?;
    let result = json!({"color":state.turn,"targetColor":target_color,"previousType":previous_kind,
        "resultType":result_kind,"row":destination.row,"col":destination.col});
    if let Some(virtual_card) = virtual_card {
        virtual_card
            .extra
            .insert("randomRouletteResultType".into(), json!(result_kind));
        virtual_card
            .extra
            .insert("randomRouletteResult".into(), result);
        // Source findDeckCard uses the instance identity, and only mirrors
        // the type if a distinct live card exists. A virtual draw normally
        // has no live slot, so its absence must not reject the effect.
        if let Some(live) = state
            .deck_slots
            .white
            .iter_mut()
            .chain(state.deck_slots.black.iter_mut())
            .find(|slot| slot.instance_id == instance_id)
        {
            live.extra
                .insert("randomRouletteResultType".into(), json!(result_kind));
        }
    } else {
        let live = state
            .deck_slots
            .get_mut(state.turn)
            .iter_mut()
            .find(|slot| slot.instance_id == instance_id && slot.id == card_id)
            .ok_or(EngineError::IllegalAction)?;
        live.extra
            .insert("randomRouletteResultType".into(), json!(result_kind));
        live.extra.insert("randomRouletteResult".into(), result);
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn bare_v7() -> GameState {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.mode = "play".into();
        state.turn = Color::White;
        state.board = vec![vec![None; 8]; 8];
        state
    }

    fn card(id: &str, effect: &str) -> CardSlot {
        serde_json::from_value(json!({"id":id,"effect":effect,"instanceId":format!("test-{id}")}))
            .unwrap()
    }

    fn receipt_state(position: &Value) -> GameState {
        let mut state: GameState = serde_json::from_value(position["state"].clone()).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.rng = serde_json::from_value(position["rng"].clone()).unwrap();
        state.history = position["history"].as_array().unwrap().clone();
        state
    }

    fn assert_receipt_position(actual: &GameState, expected: &Value, label: &str) {
        use crate::tests::source_callback_fixture::{compare_value, source_callback_state};
        source_callback_state(expected)
            .unwrap_or_else(|error| panic!("{label}: invalid source envelope: {error}"));
        assert_eq!(
            actual.ruleset_id, RULES_VERSION_V7,
            "{label}: native rules version differs"
        );
        let mut value = serde_json::to_value(actual).unwrap();
        for field in ["rulesetId", "rng", "history"] {
            value.as_object_mut().unwrap().remove(field);
        }
        // 테스트용 raw callback은 public spatial admission을 거치지 않는다.
        // state/RNG/history와 버전을 모두 포함한 실제 envelope identity는
        // source 계약과 같은 JCS bytes로 계산하고 전체 envelope를 비교한다.
        let content = json!({
            "protocolVersion":crate::v7_host::V7_POSITION_PROTOCOL,
            "rulesVersion":actual.ruleset_id,
            "catalogVersion":crate::v7_execution_profile::catalog_version().unwrap(),
            "state":value,
            "rng":&actual.rng,
            "history":&actual.history,
        });
        let position_id = format!("{:x}", Sha256::digest(serde_jcs::to_vec(&content).unwrap()));
        let mut envelope = content.as_object().unwrap().clone();
        envelope.insert("positionId".into(), json!(position_id));
        let mut differences = Vec::new();
        compare_value(expected, &Value::Object(envelope), label, &mut differences)
            .unwrap_or_else(|error| panic!("{label}: source envelope comparison failed: {error}"));
        assert!(
            differences.is_empty(),
            "{label}: exact source position diverged: {differences:?}"
        );
    }

    /// Git 밖 recipe의 exact 81개 raw boundary를 재검증한다. receipt가 없으면
    /// 성공으로 가장하지 않고, 메인이 명시적으로 --ignored로 실행한다.
    #[test]
    #[ignore = "ACCELERATE_V7_CARD_PIECE_CASES의 frozen source receipt 필요"]
    fn frozen_piece_aux_and_declines() {
        let path = std::env::var_os("ACCELERATE_V7_CARD_PIECE_CASES").expect(
            "ACCELERATE_V7_CARD_PIECE_CASES must name the generated external JSONL receipt",
        );
        let data = std::fs::read_to_string(path).unwrap();
        assert!(
            data.len() <= 32 * 1024 * 1024,
            "source receipt exceeded the recipe bound"
        );
        let profile: Value = serde_json::from_str(include_str!(
            "../../contracts/catalog/execution-profile-20260928.json"
        ))
        .unwrap();
        let profile_sha = format!("{:x}", Sha256::digest(serde_jcs::to_vec(&profile).unwrap()));
        let mut family_counts = [0; 3];
        for (index, line) in data.lines().enumerate() {
            assert!(index < 81, "source receipt exceeded 81 cases");
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            assert_eq!(
                receipt["schemaVersion"], 2,
                "exact AI/probe context receipt required"
            );
            assert_eq!(
                receipt["executionProfile"]["profileVersion"],
                "accelerate-headless-semantic-v7-faithful-init-v1"
            );
            assert_eq!(
                receipt["executionProfile"]["executionProfileSha256"],
                profile_sha
            );
            let id = receipt["id"].as_str().unwrap();
            let label = format!("receipt {index} {}", receipt["caseKey"].as_str().unwrap());
            let mut state = receipt_state(&receipt["sourcePosition"]);
            state.ai_simulation_depth = receipt["sourceAiSimulation"]
                .as_u64()
                .expect("sourceAiSimulation must preserve the actual source global")
                .try_into()
                .unwrap();
            state.threat_probe_depth = receipt["sourceThreatProbe"]
                .as_u64()
                .unwrap()
                .try_into()
                .unwrap();
            match receipt["family"].as_str().unwrap() {
                "decline" | "shotgun" => {
                    let mut card = state
                        .deck_slots
                        .get(state.turn)
                        .iter()
                        .find(|card| card.id == id)
                        .unwrap()
                        .clone();
                    let action = Action::card(
                        state.turn,
                        &card,
                        receipt["sourceAction"]["payload"].get("target").cloned(),
                    );
                    let result = apply_virtual_effect(&mut state, &mut card, &action);
                    if receipt["family"] == "decline" {
                        assert!(IDS.contains(&id), "{label}: unknown public Piece family");
                        assert_eq!(receipt["sourceDirectResult"]["ok"], false, "{label}");
                        assert!(
                            source_decline_retry_safe(&card),
                            "{label}: source-false attestation missing"
                        );
                        assert!(
                            matches!(result, Err(EngineError::IllegalAction)),
                            "{label}: source false must remain an effect-domain decline: {result:?}"
                        );
                        family_counts[0] += 1;
                    } else {
                        assert_eq!(id, "shotgun-king");
                        assert_eq!(receipt["sourceDirectResult"]["ok"], true, "{label}");
                        assert!(
                            !source_decline_retry_safe(&card),
                            "{label}: always-success aux must not attest false"
                        );
                        result.unwrap_or_else(|error| panic!("{label}: {error}"));
                        family_counts[1] += 1;
                    }
                }
                "transcendence" => {
                    let landing: Square =
                        serde_json::from_value(receipt["sourceAction"]["landing"].clone()).unwrap();
                    let mut piece = state.at(landing).unwrap().clone();
                    let source_type = receipt["sourceAction"]["sourceType"].as_str().unwrap();
                    let result = apply_transcendence_capture_upgrade(
                        &mut state,
                        &mut piece,
                        landing,
                        source_type,
                    )
                    .unwrap_or_else(|error| panic!("{label}: {error}"));
                    assert_eq!(
                        result.as_deref().unwrap_or(""),
                        receipt["sourceDirectResult"]["type"].as_str().unwrap(),
                        "{label}: source type"
                    );
                    family_counts[2] += 1;
                }
                family => panic!("{label}: unexpected receipt family {family}"),
            }
            assert_receipt_position(&state, &receipt["sourceDirectPosition"], &label);
        }
        assert_eq!(
            family_counts,
            [61, 8, 12],
            "source recipe coverage must be exact"
        );
    }

    #[test]
    #[ignore = "ACCELERATE_V7_TRANSCENDENCE_CONTEXT_CASES의 faithful source 4-context receipt 필요"]
    fn frozen_transcendence_ai_and_probe_contexts_match_full_positions() {
        use crate::tests::source_callback_fixture::{
            collect_case_diagnostics, compare_callback_envelope, compare_value,
            source_callback_state, validate_source_receipt,
        };
        let path = std::env::var_os("ACCELERATE_V7_TRANSCENDENCE_CONTEXT_CASES")
            .expect("ACCELERATE_V7_TRANSCENDENCE_CONTEXT_CASES must name the four-context receipt");
        let data = std::fs::read_to_string(path).unwrap();
        assert!(
            data.len() <= 8 * 1024 * 1024,
            "source receipt exceeds the recipe bound"
        );
        let receipt: Value = serde_json::from_str(&data).unwrap();
        let cases = validate_source_receipt(&receipt).unwrap();
        assert_eq!(cases.len(), 4, "source context count must remain exact");
        assert_eq!(
            receipt["fixtureKind"],
            "synthetic-contract-frozen-source-callback"
        );
        let profile: Value = serde_json::from_str(include_str!(
            "../../contracts/catalog/execution-profile-20260928.json"
        ))
        .unwrap();
        assert_eq!(
            receipt["executionProfile"]["profileVersion"],
            profile["profileVersion"]
        );
        assert_eq!(
            receipt["executionProfile"]["executionProfileSha256"],
            format!("{:x}", Sha256::digest(serde_jcs::to_vec(&profile).unwrap()))
        );
        assert_eq!(
            receipt["executionProfile"]["initializersSha256"],
            profile["initializersSha256"]
        );
        assert_eq!(
            receipt["executionProfile"]["replayMetadataSha256"],
            profile["replayMetadataSha256"]
        );
        assert_eq!(receipt["executionProfile"]["initializerCount"], 175);
        assert_eq!(receipt["executionProfile"]["frameKeyCount"], 222);
        assert_eq!(receipt["executionProfile"]["labelCount"], 79);
        assert_eq!(receipt["executionProfile"]["notationCodeCount"], 79);
        let contexts = [
            ("live", 0, 0),
            ("ai-only", 1, 0),
            ("probe-only", 0, 1),
            ("ai-and-probe", 1, 1),
        ];
        let mut mismatches = Vec::new();
        for (case, (name, ai_depth, probe_depth)) in cases.iter().zip(contexts) {
            assert_eq!(case["name"], name, "source context identity/order differs");
            assert_eq!(case["sourceAiSimulation"], ai_depth);
            assert_eq!(case["sourceThreatProbe"], probe_depth);
            let label = format!("Transcendence {name}");
            collect_case_diagnostics(&label, &mut mismatches, |mismatches| {
                let mut state = source_callback_state(&case["before"])?;
                assert_receipt_position(&state, &case["before"], &format!("{label} before"));
                assert_eq!(
                    state.rng.cursor, 122,
                    "{label}: native unit RNG input cursor"
                );
                assert_eq!(
                    state.rng.state, 1_316_640_757,
                    "{label}: native unit RNG input state"
                );
                assert!(
                    state.rng.tape.is_empty(),
                    "{label}: native unit has no random tape"
                );
                assert_eq!(state.move_count, 0, "{label}: deterministic upgrade input");
                state.ai_simulation_depth = ai_depth;
                state.threat_probe_depth = probe_depth;
                let landing = Square { row: 4, col: 4 };
                let mut piece = state.at(landing).unwrap().clone();
                assert_eq!(piece.kind, "pawn");
                assert_eq!(piece.id, "source-pawn");
                assert_eq!(piece.color, Color::White);
                assert_eq!(piece.extra["ammo"], 99);
                assert_eq!(piece.extra["logRollAfterPly"], 88);
                assert_eq!(piece.extra["hp"], 7);
                assert_eq!(piece.extra["shielded"], true);
                assert_eq!(piece.extra["holdoutPromotion"], "queen");
                let cursor_before = state.rng.cursor;
                let result =
                    apply_transcendence_capture_upgrade(&mut state, &mut piece, landing, "pawn")?;
                compare_value(
                    &case["returned"],
                    &json!(result.as_deref().unwrap_or("")),
                    &format!("{label}.returned"),
                    mismatches,
                )?;
                assert_eq!(
                    case["rngDraws"],
                    state.rng.cursor - cursor_before,
                    "{label}: exact source RNG consumption"
                );
                assert_eq!(
                    state.ai_simulation_depth, ai_depth,
                    "{label}: AI context changed"
                );
                assert_eq!(
                    state.threat_probe_depth, probe_depth,
                    "{label}: probe context changed"
                );
                compare_callback_envelope(&state, &case["after"], &label, mismatches)
            });
        }
        assert!(
            mismatches.is_empty(),
            "exact Transcendence contexts differ: {mismatches:?}"
        );
    }

    #[test]
    #[ignore = "ACCELERATE_V7_PIECE_GRAND_RECEIPT의 source full-transition receipt 필요"]
    fn frozen_piece_grand_full_transition_differences() {
        let path = std::env::var_os("ACCELERATE_V7_PIECE_GRAND_RECEIPT").expect(
            "ACCELERATE_V7_PIECE_GRAND_RECEIPT must name the generated external JSON receipt",
        );
        let receipt: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(
            receipt["schemaVersion"], 2,
            "Grand source receipt must record its faithful execution profile"
        );
        assert_eq!(
            receipt["sourceSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        let profile: Value = serde_json::from_str(include_str!(
            "../../contracts/catalog/execution-profile-20260928.json"
        ))
        .unwrap();
        let profile_sha = format!("{:x}", Sha256::digest(serde_jcs::to_vec(&profile).unwrap()));
        assert_eq!(
            receipt["executionProfile"]["profileVersion"],
            profile["profileVersion"]
        );
        assert_eq!(
            receipt["executionProfile"]["executionProfileSha256"],
            profile_sha
        );
        assert_eq!(
            receipt["executionProfile"]["initializersSha256"],
            profile["initializersSha256"]
        );
        assert_eq!(
            receipt["executionProfile"]["replayMetadataSha256"],
            profile["replayMetadataSha256"]
        );
        assert_eq!(receipt["executionProfile"]["initializerCount"], 175);
        assert_eq!(receipt["executionProfile"]["frameKeyCount"], 222);
        assert_eq!(receipt["executionProfile"]["labelCount"], 79);
        assert_eq!(receipt["executionProfile"]["notationCodeCount"], 79);
        assert_eq!(receipt["steps"], 14);
        let catalog_version = crate::v7_execution_profile::catalog_version().unwrap();
        for stage in ["afterDraft", "beforeRoulette", "afterRoulette"] {
            assert_eq!(
                receipt[stage]["catalogVersion"], catalog_version,
                "{stage}: stale source execution identity"
            );
        }
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: "grand".into(),
                ..GameConfig::default()
            },
            37,
            RULES_VERSION_V7,
        )
        .unwrap();
        for (index, payload) in receipt["draftActions"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            assert!(index < 12, "grand draft exceeded source bound");
            let action: Action = serde_json::from_value(payload.clone()).unwrap();
            crate::draft::apply_pick(&mut state, &action).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        assert_eq!(receipt["draftActions"].as_array().unwrap().len(), 12);
        assert_receipt_position(&state, &receipt["afterDraft"], "grand after draft");
        let first_move: Action = serde_json::from_value(receipt["moveAction"].clone()).unwrap();
        crate::transition::apply(&mut state, &first_move).unwrap();
        // Source apply returns a JCS Position that the next action restores.
        crate::replay::canonicalize_position_frames(&mut state).unwrap();
        assert_receipt_position(
            &state,
            &receipt["beforeRoulette"],
            "grand after source a2-a3",
        );
        let card_action: Action = serde_json::from_value(receipt["cardAction"].clone()).unwrap();
        crate::transition::apply(&mut state, &card_action).unwrap();
        crate::replay::canonicalize_position_frames(&mut state).unwrap();
        assert_receipt_position(
            &state,
            &receipt["afterRoulette"],
            "grand after source Roulette",
        );
    }

    #[test]
    fn shotgun_aux_replaces_native_royal_without_capture_callbacks() {
        let mut state = bare_v7();
        state.rng = RngState::seeded(19);
        state.turns_taken.white = u32::MAX;
        let mut king = Piece::new("royalKnight", Color::White, "old-royal");
        king.extra.insert("shielded".into(), json!(true));
        king.extra.insert("vipInvitation".into(), json!(true));
        state.board[6][3] = Some(king);
        state.board[4][3] = Some(Piece::new("pawn", Color::White, "doomed-pawn"));
        let mut enemy_reaper = Piece::new("reaper", Color::Black, "enemy-reaper");
        enemy_reaper.extra.insert("reaperCaptures".into(), json!(3));
        state.board[3][3] = Some(enemy_reaper.clone());
        let before = state.clone();
        let shotgun = card("shotgun-king", "shotgunKing");
        assert_eq!(ui_targets(&state, &shotgun).unwrap(), Vec::new());
        assert_eq!(
            actions(&state, &shotgun).unwrap(),
            vec![Action::card(Color::White, &shotgun, None)]
        );
        apply(
            &mut state,
            &shotgun,
            &Action::card(Color::White, &shotgun, None),
        )
        .unwrap();
        let royal = state.board[6][3].as_ref().unwrap();
        assert_eq!(royal.kind, "shotgunKing");
        assert_ne!(royal.id, "old-royal");
        assert_eq!(royal.extra["shielded"], json!(false));
        assert!(!royal.extra.contains_key("vipInvitation"));
        assert_eq!(royal.extra["origin"], json!("d2"));
        assert_eq!(
            royal.extra["freshNoCaptureUntil"],
            json!(u64::from(u32::MAX) + 1)
        );
        assert_eq!(royal.extra["facing"], json!("up"));
        assert_eq!(royal.extra["hp"], json!(4));
        assert!(royal.moved);
        assert!(state.board[4][3].is_none());
        assert_eq!(state.board[3][3].as_ref(), Some(&enemy_reaper));
        assert_eq!((state.rng.cursor, state.rng.state), (1, 1_045_530_198));
        assert_eq!(state.captures, before.captures);
        assert_eq!(state.deck_slots, before.deck_slots);
        assert_eq!(state.extra, before.extra);
        assert_eq!(state.mode, "play");
    }

    #[test]
    fn shotgun_aux_retains_active_regency_object_and_large_aliases_without_rng() {
        let mut state = bare_v7();
        state.turn = Color::Black;
        state
            .extra
            .insert("regency".into(), json!({"white":false,"black":true}));
        state
            .extra
            .insert("kingDead".into(), json!({"white":false,"black":true}));
        let mut heir = Piece::new("colossus", Color::Black, "active-heir");
        heir.extra.insert("regencyHeir".into(), json!(true));
        heir.extra.insert("shielded".into(), json!(true));
        heir.extra.insert("origin".into(), json!("retained-origin"));
        heir.extra.insert("vipInvitation".into(), json!(true));
        heir.extra.insert("anchorRow".into(), json!(2));
        heir.extra.insert("anchorCol".into(), json!(2));
        for row in 2..=3 {
            for col in 2..=3 {
                state.board[row][col] = Some(heir.clone());
            }
        }
        state.board[0][0] = Some(Piece::new("rook", Color::Black, "other-friend"));
        let before = state.clone();
        let mut shotgun = card("shotgun-king", "shotgunKing");
        shotgun.extra.insert("devCard".into(), json!(true));
        assert!(!source_decline_retry_safe(&shotgun));
        let action = Action::card(Color::Black, &shotgun, Some(Value::Null));
        apply_virtual_effect(&mut state, &mut shotgun, &action).unwrap();
        assert!(state.board[0][0].is_none());
        let result = state.board[2][2].as_ref().unwrap();
        for row in 2..=3 {
            for col in 2..=3 {
                assert_eq!(state.board[row][col].as_ref(), Some(result));
            }
        }
        assert_eq!(result.id, heir.id);
        assert_eq!(result.extra["origin"], heir.extra["origin"]);
        assert_eq!(result.extra["vipInvitation"], json!(true));
        assert_eq!(result.extra["shielded"], json!(true));
        assert_eq!(result.extra["facing"], json!("down"));
        assert_eq!(result.extra["hp"], json!(4));
        assert_eq!(state.rng, before.rng);
        assert_eq!(state.captures, before.captures);
        assert_eq!(state.extra, before.extra);
        assert!(!shotgun.used);
    }

    #[test]
    fn shotgun_aux_fallback_overwrites_enemy_home_without_capture() {
        let mut state = bare_v7();
        state.board[7][4] = Some(Piece::new("vip", Color::Black, "enemy-fallback"));
        state.board[5][0] = Some(Piece::new("pawn", Color::White, "friendly-pawn"));
        let before = state.clone();
        let mut shotgun = card("shotgun-king", "shotgunKing");
        shotgun.extra.insert("devCard".into(), json!(true));
        let action = Action::card(Color::White, &shotgun, None);
        apply_virtual_effect(&mut state, &mut shotgun, &action).unwrap();
        assert_eq!(state.board[7][4].as_ref().unwrap().kind, "shotgunKing");
        assert!(state.board[5][0].is_none());
        assert_eq!(state.rng.cursor, before.rng.cursor + 1);
        assert_eq!(state.captures, before.captures);
        assert_eq!(state.winner, before.winner);
        assert_eq!(state.mode, before.mode);
        let bad_action = Action::card(Color::White, &shotgun, Some(json!({"row":7,"col":4})));
        let before = (state.clone(), shotgun.clone());
        assert!(matches!(
            apply_virtual_effect(&mut state, &mut shotgun, &bad_action),
            Err(EngineError::InvalidState(message)) if message.contains("absent or null target")
        ));
        assert_eq!((state, shotgun), before);
    }

    #[test]
    fn transcendence_live_guards_and_non_upgrade_types_preserve_source_rng_order() {
        let mut state = bare_v7();
        state.rng = RngState::seeded(19);
        let at = Square { row: 4, col: 4 };
        let mut queen = Piece::new("queen", Color::White, "live-queen");
        state.board[4][4] = Some(queen.clone());
        let before = state.clone();
        assert_eq!(
            apply_transcendence_capture_upgrade(&mut state, &mut queen, at, "queen").unwrap(),
            None
        );
        assert_eq!(state, before);
        state.extra.insert("transcendenceRule".into(), json!(true));
        let before = state.clone();
        assert_eq!(
            apply_transcendence_capture_upgrade(&mut state, &mut queen, at, "queen").unwrap(),
            None
        );
        assert_eq!((state.rng.cursor, state.rng.state), (1, 1_045_530_198));
        let mut expected = before;
        expected.rng = state.rng.clone();
        assert_eq!(state, expected);
        let before = state.clone();
        let mut stale = Piece::new("pawn", Color::White, "removed-object");
        assert_eq!(
            apply_transcendence_capture_upgrade(&mut state, &mut stale, at, "pawn").unwrap(),
            None
        );
        assert_eq!(state, before);
    }

    #[test]
    fn transcendence_ai_simulation_changes_type_and_metadata_without_random_draw() {
        let mut state = bare_v7();
        state.extra.insert("transcendenceRule".into(), json!(true));
        state.extra.insert("monochromeChess".into(), json!(true));
        // Frozen main102334 and main88225 both gate on aiSimulationDepth.
        // A king threat simulation also sets this depth; probe alone is separate.
        state.ai_simulation_depth = 1;
        state.threat_probe_depth = 1;
        let at = Square { row: 4, col: 4 };
        let mut pawn = Piece::new("pawn", Color::White, "source-pawn");
        pawn.extra.insert("ammo".into(), json!(99));
        pawn.extra.insert("logRollAfterPly".into(), json!(88));
        pawn.extra.insert("hp".into(), json!(7));
        pawn.extra.insert("shielded".into(), json!(true));
        pawn.extra.insert("holdoutPromotion".into(), json!("queen"));
        state.board[4][4] = Some(pawn.clone());
        let rng_before = state.rng.clone();
        assert_eq!(
            apply_transcendence_capture_upgrade(&mut state, &mut pawn, at, "pawn").unwrap(),
            Some("camel".into())
        );
        assert_eq!(state.board[4][4].as_ref(), Some(&pawn));
        assert_eq!(pawn.extra["hp"], json!(7));
        assert_eq!(pawn.extra["shielded"], json!(true));
        assert_eq!(pawn.extra["logRollAfterPly"], json!(88));
        assert_eq!(pawn.extra["origin"], json!("e4"));
        assert_eq!(pawn.extra["monoShade"], json!("light"));
        assert!(!pawn.extra.contains_key("ammo"));
        assert!(!pawn.extra.contains_key("holdoutPromotion"));
        assert_eq!(state.rng, rng_before);
    }

    #[test]
    fn loyalist_keeps_source_raw_highlights_separate_from_legal_actions() {
        // Source getTargetSquares: 105834-105841, 105901;
        // source loyalist/transformCardPiece rejection: 99595-99601, 99699-99712.
        let mut state = bare_v7();
        state.board[2][4] = Some(Piece::new("slime", Color::White, "slime"));
        let mut royal_rook = Piece::new("rook", Color::White, "royal-rook");
        royal_rook.extra.insert("crownRoyal".into(), json!(true));
        state.board[1][6] = Some(royal_rook);
        state.board[7][0] = Some(Piece::new("rook", Color::White, "rook"));
        let loyalist = card("loyalist", "loyalist");
        assert_eq!(
            ui_targets(&state, &loyalist).unwrap(),
            vec![Square { row: 2, col: 4 }, Square { row: 7, col: 0 }]
        );
        assert_eq!(
            actions(&state, &loyalist).unwrap(),
            vec![Action::card(
                Color::White,
                &loyalist,
                Some(json!({"row":7,"col":0}))
            )]
        );
        let princess = card("princess", "princess");
        // The princess effect branch at 105960 precedes the own-rook target
        // fallback at 106006, so even its raw UI excludes a royal rook.
        assert_eq!(
            ui_targets(&state, &princess).unwrap(),
            vec![Square { row: 7, col: 0 }]
        );
        assert_eq!(
            actions(&state, &princess).unwrap(),
            vec![Action::card(
                Color::White,
                &princess,
                Some(json!({"row":7,"col":0}))
            )]
        );
    }

    #[test]
    fn ordination_highlights_only_when_a_distinct_second_bishop_exists() {
        // Source isValidTarget: 105904-105906, countPiecesOf >= 2.
        let mut state = bare_v7();
        state.board[7][2] = Some(Piece::new("bishop", Color::White, "bishop-a"));
        let card = card("ordination", "ordination");
        assert!(ui_targets(&state, &card).unwrap().is_empty());
        assert!(actions(&state, &card).unwrap().is_empty());
        state.board[7][5] = Some(Piece::new("bishop", Color::White, "bishop-b"));
        assert_eq!(
            ui_targets(&state, &card).unwrap(),
            vec![Square { row: 7, col: 2 }, Square { row: 7, col: 5 }]
        );
    }

    #[test]
    fn parry_grant_matches_frozen_full_state_without_rng_draw() {
        // Frozen e5ed84fc normal seed 19: source parry({row:6,col:0})
        // changes exactly one pawn trait and its animation projection.
        let mut state =
            crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7)
                .unwrap();
        state.mode = "play".into();
        state.turn = Color::White;
        assert_eq!(
            source_state_digest(&state),
            "e314f49c9a2a2bc22370ee935c88da5ac3836cea224e2b30f70dc4f425adaeaa"
        );
        assert_eq!((state.rng.cursor, state.rng.state), (122, 1_316_640_757));
        let card = card("parry", "parry");
        let action = Action::card(Color::White, &card, Some(json!({"row":6,"col":0})));
        apply(&mut state, &card, &action).unwrap();
        assert_eq!(
            source_state_digest(&state),
            "4a3ee0caeaaf4a26a19798688a80f8ecc68be10d4c3ccf098eb4c706322a55d6"
        );
        assert_eq!((state.rng.cursor, state.rng.state), (122, 1_316_640_757));
    }

    #[test]
    fn poisoned_pawns_match_frozen_source_shuffle_and_rng_cursor() {
        // Frozen client e5ed84fc, normal/draftDelete new game seed 19,
        // direct poisonedPawn(): eight white pawns at row 6, source RNG
        // {state:4163866163,cursor:32} -> {state:202246476,cursor:39}.
        // The source's Fisher-Yates shuffle selects files b,c,e,h.
        let mut state = bare_v7();
        for col in 0..8 {
            state.board[6][col] = Some(Piece::new(
                "pawn",
                Color::White,
                format!("white-pawn-{col}"),
            ));
        }
        state.rng.state = 4_163_866_163;
        state.rng.cursor = 32;
        state.rng.tape.clear();
        let card = card("poisoned-pawn", "poisonedPawn");
        let action = Action::card(Color::White, &card, None);
        assert_eq!(actions(&state, &card).unwrap(), vec![action.clone()]);
        assert!(apply(&mut state, &card, &action).unwrap().is_empty());
        assert_eq!((state.rng.state, state.rng.cursor), (202_246_476, 39));
        let selected: Vec<_> = (0..8)
            .filter(|&col| {
                state.board[6][col]
                    .as_ref()
                    .is_some_and(|piece| piece.extra.get("poisonedPawn") == Some(&json!(true)))
            })
            .collect();
        assert_eq!(selected, vec![1, 2, 4, 7]);
    }

    fn source_state_digest(state: &GameState) -> String {
        let mut value = serde_json::to_value(state).unwrap();
        for field in ["rulesetId", "rng", "history"] {
            value.as_object_mut().unwrap().remove(field);
        }
        format!("{:x}", Sha256::digest(serde_jcs::to_vec(&value).unwrap()))
    }

    #[test]
    fn grand_seed37_reachable_roulette_direct_effect_matches_frozen_source() {
        // The first source-legal roulette action after twelve first-offer
        // grand draft picks targets black's a8 rook. Compare the direct card
        // effect before finishCard and the later turn/no-action settlement.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: "grand".into(),
                ..GameConfig::default()
            },
            37,
            RULES_VERSION_V7,
        )
        .unwrap();
        for _ in 0..12 {
            let choice = crate::draft::legal_actions(&state).unwrap().remove(0);
            crate::draft::apply_pick(&mut state, &choice).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        assert_eq!(state.mode, "play");
        assert_eq!(
            source_state_digest(&state),
            "94cd6723db21c6e1911e94dbe4587f152d168e44d4f948185c5d85a78b35b389"
        );
        let card = state
            .deck_slots
            .white
            .iter()
            .find(|slot| slot.id == "random-roulette")
            .unwrap()
            .clone();
        let action = Action::card(Color::White, &card, Some(json!({"row":0,"col":0})));
        assert_eq!(actions(&state, &card).unwrap().first(), Some(&action));
        apply(&mut state, &card, &action).unwrap();
        assert_eq!((state.rng.cursor, state.rng.state), (125, 2_854_222_796));
        assert_eq!(
            source_state_digest(&state),
            "8d8d92a4d8b846fe3fec9fb9c27498d1886725b8cc6876810ac25acd4cf66194"
        );
    }

    #[test]
    fn grand_seed37_reachable_roulette_consumes_transient_in_full_transition() {
        // Frozen client e5ed84fc: grand seed 37, white chooses the second
        // offer and black the first across twelve picks, then a2-a3. Black's
        // real fourth card is random-roulette-cdiswtbmz2. Its d8 target
        // becomes a recruiter. The source's finishCard removes the online
        // event payload but keeps the result type in the used card and replay.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: "grand".into(),
                ..GameConfig::default()
            },
            37,
            RULES_VERSION_V7,
        )
        .unwrap();
        for expected in [
            "king-of-the-hill",
            "d4",
            "democracy",
            "freeze",
            "guard",
            "alekhine-machine-gun",
            "random-roulette",
            "retreat",
            "feudal-contract",
            "nullification",
            "panic",
            "submerge",
        ] {
            let offer_index = usize::from(state.extra["draft"]["color"] == "white");
            let action = crate::draft::legal_actions(&state)
                .unwrap()
                .remove(offer_index);
            let selected = state.extra["draft"]["choices"]
                .as_array()
                .unwrap()
                .iter()
                .find(|card| card["instanceId"].as_str() == action.card_instance_id.as_deref())
                .unwrap();
            assert_eq!(selected["id"], expected);
            crate::draft::apply_pick(&mut state, &action).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        assert_eq!(state.mode, "play");
        let first_move = Action::movement(
            Color::White,
            Square { row: 6, col: 0 },
            MoveTarget::at(Square { row: 5, col: 0 }),
        );
        crate::transition::apply(&mut state, &first_move).unwrap();
        // Match the production host's source Position copy before Roulette.
        crate::replay::canonicalize_position_frames(&mut state).unwrap();
        assert_eq!(state.turn, Color::Black);
        assert_eq!(state.rng.cursor, 139);
        assert_eq!(state.rng.state, 265_745_114);
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.extra["replayEvents"].as_array().unwrap().len(), 13);
        assert_eq!(
            source_state_digest(&state),
            "d8c4f2db159884c21b732726ad924cc3ee3242d313d409e0c924753e3e1ac8d7"
        );
        let card = state.deck_slots.black[3].clone();
        assert_eq!(card.id, "random-roulette");
        assert_eq!(card.instance_id, "random-roulette-cdiswtbmz2");
        let action = Action::card(Color::Black, &card, Some(json!({"row":0,"col":3})));
        crate::transition::apply(&mut state, &action).unwrap();
        crate::replay::canonicalize_position_frames(&mut state).unwrap();
        let live = &state.deck_slots.black[3];
        assert_eq!(
            live.extra.get("randomRouletteResultType"),
            Some(&json!("recruiter"))
        );
        assert!(!live.extra.contains_key("randomRouletteResult"));
        assert!(live.used);
        assert_eq!(state.board[0][3].as_ref().unwrap().kind, "recruiter");
        assert_eq!((state.rng.cursor, state.rng.state), (144, 136_244_789));
        assert_eq!(state.history.len(), 2);
        assert_eq!(state.extra["replayEvents"].as_array().unwrap().len(), 14);
        assert_eq!(
            source_state_digest(&state),
            "5467dc49940c04c130b51783c52c151daa38f22434deafec9135be4e5c75838f"
        );
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(serde_jcs::to_vec(&state.history[1]).unwrap())
            ),
            "b9e3e4513118291516d7b0a149db1e72441e4588293790c2a1db551ffa6f37f7"
        );
    }

    #[test]
    fn roulette_consumes_one_draw_for_a_small_result_and_reveals_the_exact_card_instance() {
        let mut state = bare_v7();
        state.board[3][3] = Some(Piece::new("queen", Color::Black, "enemy-queen"));
        let card = card("random-roulette", "randomRoulette");
        state.deck_slots.white.push(card.clone());
        let outcome = roulette_outcomes(&state, "queen", true).unwrap();
        let index = outcome.iter().position(|kind| *kind == "wizard").unwrap();
        assert_eq!((outcome.len(), index), (60, 29));
        state.rng = RngState::seeded(19);
        state.rng.tape = vec![(index as f64 + 0.5) / outcome.len() as f64];
        let action = Action::card(Color::White, &card, Some(json!({"row":3,"col":3})));
        assert_eq!(actions(&state, &card).unwrap(), vec![action.clone()]);
        let before_cursor = state.rng.cursor;
        assert_eq!(
            apply(&mut state, &card, &action).unwrap(),
            Vec::<Piece>::new()
        );
        assert_eq!(state.rng.cursor, before_cursor + 1);
        assert_eq!(state.rng.state, 1_045_530_198);
        let piece = state.board[3][3].as_ref().unwrap();
        assert_eq!(piece.kind, "wizard");
        assert_eq!(piece.color, PieceColor::Black);
        assert_eq!(piece.id, "enemy-queen");
        assert!(piece.moved);
        assert_eq!(piece.extra["mana"], json!(0));
        assert_eq!(piece.extra["maxMana"], json!(5));
        assert_eq!(piece.extra["origin"], json!("d5"));
        let live = state.deck_slots.white.last().unwrap();
        assert_eq!(live.extra["randomRouletteResultType"], json!("wizard"));
        assert_eq!(
            live.extra["randomRouletteResult"]["targetColor"],
            json!("black")
        );
        assert_eq!(
            state.extra["pendingReplayVisuals"][0]["type"],
            json!("random-roulette")
        );
    }

    #[test]
    fn virtual_roulette_publishes_result_without_inserting_or_spending_a_hand_card() {
        // Frozen main:104295-104338 mutates the provided card and optionally
        // mirrors its type to a live card. applyRandomBoxCard:104742-104765
        // creates a separate devCard and later copies this result into Box.
        let mut state = bare_v7();
        state.board[3][3] = Some(Piece::new("queen", Color::Black, "enemy-queen"));
        let mut live = card("random-roulette", "randomRoulette");
        live.instance_id = "physical-roulette".into();
        state.deck_slots.white.push(live);
        let hand_before = state.deck_slots.clone();
        let mut virtual_card = card("random-roulette", "randomRoulette");
        virtual_card.instance_id = "virtual-roulette".into();
        virtual_card.extra.insert("devCard".into(), json!(true));
        let outcomes = roulette_outcomes(&state, "queen", true).unwrap();
        let index = outcomes.iter().position(|kind| *kind == "wizard").unwrap();
        state.rng = RngState::seeded(19);
        state.rng.tape = vec![(index as f64 + 0.5) / outcomes.len() as f64];
        let action = Action::card(Color::White, &virtual_card, Some(json!({"row":3,"col":3})));
        assert!(
            apply_virtual_effect(&mut state, &mut virtual_card, &action)
                .unwrap()
                .is_empty()
        );
        assert_eq!(state.deck_slots, hand_before);
        assert_eq!(state.board[3][3].as_ref().unwrap().kind, "wizard");
        assert_eq!((state.rng.cursor, state.rng.state), (1, 1_045_530_198));
        assert_eq!(state.turn, Color::White);
        assert!(state.history.is_empty());
        assert_eq!(
            virtual_card.extra["randomRouletteResultType"],
            json!("wizard")
        );
        assert_eq!(
            virtual_card.extra["randomRouletteResult"],
            json!({"color":"white","targetColor":"black","previousType":"queen",
                "resultType":"wizard","row":3,"col":3})
        );
        assert!(!virtual_card.used);
    }

    #[test]
    fn virtual_piece_card_admission_errors_preserve_state_and_result_card() {
        let mut state = bare_v7();
        state.board[3][3] = Some(Piece::new("queen", Color::Black, "enemy-queen"));
        let mut virtual_card = card("random-roulette", "randomRoulette");
        let action = Action::card(Color::White, &virtual_card, Some(json!({"row":3,"col":3})));
        let before = (state.clone(), virtual_card.clone());
        assert!(matches!(
            apply_virtual_effect(&mut state, &mut virtual_card, &action),
            Err(EngineError::InvalidState(message)) if message.contains("devCard=true")
        ));
        assert_eq!((state.clone(), virtual_card.clone()), before);

        virtual_card.extra.insert("devCard".into(), json!(true));
        let mut wrong_instance = action.clone();
        wrong_instance.card_instance_id = Some("unbound-virtual-instance".into());
        let before = (state.clone(), virtual_card.clone());
        assert!(matches!(
            apply_virtual_effect(&mut state, &mut virtual_card, &wrong_instance),
            Err(EngineError::InvalidState(message)) if message.contains("identity/admission")
        ));
        assert_eq!((state, virtual_card), before);
    }

    #[test]
    fn raw_virtual_failure_keeps_consumed_rng_but_ordinary_effect_is_atomic() {
        let mut state = bare_v7();
        // newGame의 RNG 소비량과 무관한 raw 실패 경계를 검증한다.
        state.rng = RngState::seeded(19);
        state.board[3][3] = Some(Piece::new("queen", Color::Black, "enemy-queen"));
        let mut virtual_card = card("random-roulette", "randomRoulette");
        virtual_card.extra.insert("devCard".into(), json!(true));
        // The first draw succeeds; a large outcome then needs a second draw.
        // Invalid RNG is an explicit engine error, never a retryable source
        // false. The raw caller retains both draws, while a public/ordinary
        // transaction discards the entire failed operation.
        let outcomes = roulette_outcomes(&state, "queen", true).unwrap();
        let index = outcomes.iter().position(|kind| *kind == "bigRook").unwrap();
        state.rng.tape = vec![(index as f64 + 0.5) / outcomes.len() as f64, 1.0];
        let action = Action::card(Color::White, &virtual_card, Some(json!({"row":3,"col":3})));
        let before = (state.clone(), virtual_card.clone());
        let mut ordinary_state = state.clone();
        assert!(matches!(
            apply(&mut ordinary_state, &virtual_card, &action),
            Err(EngineError::InvalidState(message)) if message.contains("random draw outside [0,1)")
        ));
        assert_eq!(ordinary_state, before.0);
        assert!(matches!(
            apply_virtual_effect(&mut state, &mut virtual_card, &action),
            Err(EngineError::InvalidState(message)) if message.contains("random draw outside [0,1)")
        ));
        assert_eq!((state.rng.cursor, state.rng.state), (2, 8_325_565));
        let mut expected = before.0;
        expected.rng = state.rng.clone();
        assert_eq!(state, expected);
        assert_eq!(virtual_card, before.1);
    }

    #[test]
    fn raw_virtual_empty_random_choices_keep_source_draw_before_retry() {
        // main:67540 draws even for randomChoice([]). The three effects
        // below call it before checking for a selected piece; poison checks
        // candidates.length before shuffle at main:100646 instead.
        for (id, effect) in [
            ("thief", "thief"),
            ("recurrence", "recurrence"),
            ("royal-shield", "royalShield"),
            ("poisoned-pawn", "poisonedPawn"),
        ] {
            let mut state = bare_v7();
            state.rng = RngState::seeded(19);
            let mut virtual_card = card(id, effect);
            virtual_card.extra.insert("devCard".into(), json!(true));
            let before = (state.clone(), virtual_card.clone());
            let action = Action::card(Color::White, &virtual_card, Some(Value::Null));
            assert!(source_decline_retry_safe(&virtual_card));
            state.rng.begin_source_trace().unwrap();
            assert!(
                matches!(
                    apply_virtual_effect(&mut state, &mut virtual_card, &action),
                    Err(EngineError::IllegalAction)
                ),
                "{id}"
            );
            assert_eq!(state.rng.finish_source_trace().unwrap(), 1.0, "{id}");
            let draws = usize::from(id != "poisoned-pawn");
            let expected_state = if draws == 0 {
                before.0.rng.state
            } else {
                1_045_530_198
            };
            assert_eq!(
                (state.rng.cursor, state.rng.state),
                (draws, expected_state),
                "{id}"
            );
            let mut expected = before.0;
            expected.rng = state.rng.clone();
            assert_eq!(state, expected, "{id}");
            assert_eq!(virtual_card, before.1, "{id}");
        }
        let mut unknown = card("parry", "parry");
        assert!(!source_decline_retry_safe(&unknown));
        unknown.extra.insert("devCard".into(), json!(true));
        unknown.effect = "unreviewed-effect".into();
        assert!(!source_decline_retry_safe(&unknown));
        unknown.id = "unreviewed-card".into();
        assert!(!source_decline_retry_safe(&unknown));
    }

    #[test]
    fn source_chance_transcendence_records_only_the_pawn_branch_and_keeps_execution() {
        // main:102334의 pawn 분기만 .5/.5다. 나머지 타입의 unused roll과
        // 후속 기보 ID는 source RNG를 소비하지만 의미적 질량을 곱하지 않는다.
        for (kind, ai_depth, mass) in [
            ("pawn", 0, 0.5),
            ("rook", 0, 1.0),
            ("queen", 0, 1.0),
            ("pawn", 1, 1.0),
        ] {
            let at = Square { row: 4, col: 4 };
            let mut plain = bare_v7();
            plain.rng = RngState::seeded(19);
            plain.extra.insert("transcendenceRule".into(), json!(true));
            plain.ai_simulation_depth = ai_depth;
            let mut plain_piece = Piece::new(kind, Color::White, "transcendence-audit");
            plain.board[4][4] = Some(plain_piece.clone());
            let mut traced = plain.clone();
            let mut traced_piece = plain_piece.clone();
            traced.rng.begin_source_trace().unwrap();
            let expected =
                apply_transcendence_capture_upgrade(&mut plain, &mut plain_piece, at, kind)
                    .unwrap();
            let actual =
                apply_transcendence_capture_upgrade(&mut traced, &mut traced_piece, at, kind)
                    .unwrap();
            assert_eq!(
                traced.rng.finish_source_trace().unwrap(),
                mass,
                "{kind}/{ai_depth}"
            );
            assert_eq!(actual, expected);
            assert_eq!(traced_piece, plain_piece);
            assert_eq!(traced, plain, "{kind}/{ai_depth}");
        }
    }

    #[test]
    fn raw_virtual_no_target_accepts_json_null_and_absent_equally() {
        for (id, effect, target_kind) in [
            ("thief", "thief", "queen"),
            ("recurrence", "recurrence", "rook"),
            ("royal-shield", "royalShield", "rook"),
            ("poisoned-pawn", "poisonedPawn", "pawn"),
        ] {
            let mut state = bare_v7();
            state.board[6][0] = Some(Piece::new(target_kind, Color::White, "selected"));
            let mut absent_card = card(id, effect);
            absent_card.extra.insert("devCard".into(), json!(true));
            let mut null_card = absent_card.clone();
            let mut null_state = state.clone();
            let absent = Action::card(Color::White, &absent_card, None);
            let null = Action::card(Color::White, &null_card, Some(Value::Null));
            let absent_result =
                apply_virtual_effect(&mut state, &mut absent_card, &absent).unwrap();
            let null_result = apply_virtual_effect(&mut null_state, &mut null_card, &null).unwrap();
            assert_eq!(
                (state, absent_card, absent_result),
                (null_state, null_card, null_result),
                "{id}"
            );
        }
    }

    #[test]
    fn raw_virtual_holdout_refreshes_existing_trait_without_changing_ui_choices() {
        let mut state = bare_v7();
        let mut pawn = Piece::new("pawn", Color::White, "holdout-pawn");
        pawn.extra.insert(
            "holdoutPromotion".into(),
            json!({"by":"white","readyTurn":99}),
        );
        state.board[6][0] = Some(pawn);
        let mut virtual_card = card("holdout", "holdout");
        virtual_card.extra.insert("devCard".into(), json!(true));
        assert!(ui_targets(&state, &virtual_card).unwrap().is_empty());
        assert!(actions(&state, &virtual_card).unwrap().is_empty());
        let action = Action::card(Color::White, &virtual_card, Some(json!({"row":6,"col":0})));
        apply_virtual_effect(&mut state, &mut virtual_card, &action).unwrap();
        assert_eq!(
            state.board[6][0].as_ref().unwrap().extra["holdoutPromotion"],
            json!({"by":"white","readyTurn":14})
        );
    }

    #[test]
    fn sacrificial_transform_retains_the_primary_object_after_reaper_execution() {
        // Source amazon:105509 holds queenSquare.item across sacrifice.
        // removeSacrificedPiece:103475 and forceRemovePieceAt retain captured
        // object references; transforming the held queen changes its new
        // capture-pool entry, and never the reaper now occupying that cell.
        let mut state =
            crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7)
                .unwrap();
        state.mode = "play".into();
        state.turn = Color::White;
        state.board = vec![vec![None; 8]; 8];
        let mut queen = Piece::new("queen", Color::White, "primary-queen");
        queen.extra.insert("crownRoyal".into(), json!(true));
        state.board[3][3] = Some(queen);
        state.board[3][4] = Some(Piece::new("knight", Color::White, "sacrifice-knight"));
        let mut reaper = Piece::new("reaper", Color::Black, "witness-reaper");
        reaper.extra.insert("reaperCaptures".into(), json!(3));
        state.board[2][4] = Some(reaper);
        let mut virtual_card = card("amazon", "amazon");
        virtual_card.extra.insert("devCard".into(), json!(true));
        let action = Action::card(
            Color::White,
            &virtual_card,
            Some(json!({"row":3,"col":3,"knight":{"row":3,"col":4}})),
        );
        let captured = apply_virtual_effect(&mut state, &mut virtual_card, &action).unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].id, "sacrifice-knight");
        assert_eq!(state.board[3][3].as_ref().unwrap().id, "witness-reaper");
        assert_eq!(state.board[3][3].as_ref().unwrap().kind, "reaper");
        let primary = state
            .captures
            .black
            .iter()
            .find(|piece| piece.id == "primary-queen")
            .unwrap();
        assert_eq!(primary.kind, "amazon");
        assert!(primary.moved);
        assert_eq!(primary.extra["origin"], json!("d5"));
    }

    #[test]
    fn roulette_direct_effect_matches_frozen_full_state() {
        // Frozen e5ed84fc normal seed 19: the source restores its opening,
        // replaces one board piece/card, then calls randomRoulette directly.
        // These digests cover every non-RNG, non-history state field rather
        // than just the transformed piece and visible card metadata.
        let mut state =
            crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7)
                .unwrap();
        state.mode = "play".into();
        state.turn = Color::White;
        state.board = vec![vec![None; 8]; 8];
        let mut queen = Piece::new("queen", Color::Black, "enemy-queen");
        queen.extra.insert("hp".into(), json!(99));
        state.board[3][3] = Some(queen);
        let card = card("random-roulette", "randomRoulette");
        state.deck_slots.white[0] = card.clone();
        let outcomes = roulette_outcomes(&state, "queen", true).unwrap();
        let index = outcomes.iter().position(|kind| *kind == "wizard").unwrap();
        assert_eq!((outcomes.len(), index), (60, 29));
        state.rng = RngState::seeded(19);
        state.rng.tape = vec![(index as f64 + 0.5) / outcomes.len() as f64];
        assert_eq!(
            source_state_digest(&state),
            "a1bf292973f0f94becbd6c392480c632b42e7e9568a54cf61566e770070705b9"
        );
        let action = Action::card(Color::White, &card, Some(json!({"row":3,"col":3})));
        apply(&mut state, &card, &action).unwrap();
        assert_eq!((state.rng.cursor, state.rng.state), (1, 1_045_530_198));
        assert_eq!(
            source_state_digest(&state),
            "2b8e4e9f32b380929cefa335a175fb9c3894870fdc8fa8eaa99316903493f78f"
        );
    }

    #[test]
    fn roulette_large_result_uses_a_second_draw_and_four_alias_cells() {
        let mut state = bare_v7();
        let mut queen = Piece::new("queen", Color::White, "white-queen");
        queen.extra.insert("hp".into(), json!(99));
        state.board[3][3] = Some(queen);
        let card = card("random-roulette", "randomRoulette");
        state.deck_slots.white.push(card.clone());
        let outcome = roulette_outcomes(&state, "queen", true).unwrap();
        let index = outcome.iter().position(|kind| *kind == "bigRook").unwrap();
        assert_eq!((outcome.len(), index), (60, 5));
        state.rng = RngState::seeded(19);
        state.rng.tape = vec![(index as f64 + 0.5) / outcome.len() as f64, 0.875];
        let action = Action::card(Color::White, &card, Some(json!({"row":3,"col":3})));
        apply(&mut state, &card, &action).unwrap();
        assert_eq!(state.rng.cursor, 2);
        assert_eq!(state.rng.state, 8_325_565);
        for row in 2..=3 {
            for col in 2..=3 {
                let piece = state.board[row][col].as_ref().unwrap();
                assert_eq!(piece.kind, "bigRook");
                assert_eq!(piece.id, "white-queen");
                assert_eq!(piece.extra["anchorRow"], json!(2));
                assert_eq!(piece.extra["anchorCol"], json!(2));
                assert_eq!(piece.extra["hp"], json!(2));
                assert_eq!(piece.extra["maxHp"], json!(2));
            }
        }
    }

    #[test]
    fn roulette_resolves_campaign_loss_after_transforming_last_colossus() {
        // Frozen client randomRoulette calls checkCampaignObjectives after
        // replacing the piece and before emitting the roulette visual. A
        // campaign may therefore end even though the card itself preserves
        // the current turn.
        let mut state =
            crate::draft::initialize_for_ruleset(GameConfig::default(), 19, RULES_VERSION_V7)
                .unwrap();
        state.mode = "play".into();
        state.turn = Color::White;
        state.board = vec![vec![None; 8]; 8];
        state.extra.insert(
            "campaign".into(),
            json!({"setup":"machineRebellion","playerColor":"white"}),
        );
        let mut colossus = Piece::new("colossus", Color::Black, "last-colossus");
        colossus.extra.insert("anchorRow".into(), json!(2));
        colossus.extra.insert("anchorCol".into(), json!(2));
        for row in 2..=3 {
            for col in 2..=3 {
                state.board[row][col] = Some(colossus.clone());
            }
        }
        let card = card("random-roulette", "randomRoulette");
        state.deck_slots.white[0] = card.clone();
        state.rng = RngState::seeded(19);
        state.rng.tape = vec![0.0]; // pawn is the first permitted outcome
        assert_eq!(
            source_state_digest(&state),
            "e8242d8b638064b9e7f443dfd254857c22009b469ba04e41e3e5c7493e301d08"
        );
        let action = Action::card(Color::White, &card, Some(json!({"row":2,"col":2})));
        apply(&mut state, &card, &action).unwrap();
        assert_eq!((state.rng.cursor, state.rng.state), (1, 1_045_530_198));
        assert_eq!(
            source_state_digest(&state),
            "351e0c4f86f412ac93acc12ef52eede2ccea6bdea48dd35419d7cbadae635865"
        );
        assert_eq!(state.board[2][2].as_ref().unwrap().kind, "pawn");
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner.as_deref(), Some("white"));
        assert_eq!(
            state.extra["replayEndReason"],
            json!("거신병 3개를 모두 파괴했습니다.")
        );
        assert_eq!(
            state.extra["pendingReplayVisuals"][0]["type"],
            "random-roulette"
        );
    }
}
