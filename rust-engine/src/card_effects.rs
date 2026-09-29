//! Targeted piece transformations and trait grants from the frozen site client.
//!
//! The caller owns card availability, card-use accounting, notation, global
//! reconciliation, hazards and turn settlement. `None` means another effect
//! family owns the card; an owned but incomplete effect returns Unsupported.
use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const CATALOG: &str = "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4";
const SEPTEMBER26_HASHES: &[&str] = &[
    CATALOG,
    "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
    "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs",
];
const MINOR_BASE: &[&str] = &["knight", "bishop", "camel"];
const MINOR_CURRENT: &[&str] = &[
    "knight",
    "bishop",
    "camel",
    "clockwork",
    "parrot",
    "wizard",
    "recruiter",
    "trickster",
];
// main:3655-3687. Filtering preserves this order for the one-draw choice.
const POTION_EFFECTS: &[&str] = &[
    "sacrificeProtection",
    "lastResistance",
    "coronationProtection",
    "shield",
    "evasion",
    "parry",
    "basicTraining",
    "stealth",
    "loyalist",
    "ghost",
    "chameleon",
    "submerge",
    "freeze",
    "chimera",
    "witchTrial",
    "stake",
    "callingCard",
    "emptyLunchbox",
    "poisonStun",
    "disarm",
    "mannerNoCapture",
    "saturationNoCapture",
    "explosive",
    "poisonedPawn",
    "queensGambitProtection",
    "trojanHorse",
    "severance",
    "inertia",
    "outpostProtection",
    "nullification",
    "recurrence",
    "grapplerBound",
];
// main-CqkYwJX4.js:1747-1761. Pool order is part of the random-choice contract.
const TRICKSTER_TYPES: &[&str] = &[
    "queen",
    "rook",
    "bishop",
    "missionary",
    "knight",
    "pawn",
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
    "hook",
    "grasshopper",
    "dragon",
    "man",
    "assassin",
    "reaper",
    "knightmaster",
    "standardBearer",
    "guard",
    "recruiter",
    "squire",
    "checker",
    "checkerKing",
    "wizard",
    "alfil",
    "windmill",
    "idol",
    "lobster",
    "bear",
    "siegeRam",
    "magicGirl",
    "berserker",
    "slime",
    "siren",
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

#[derive(Clone, Copy)]
enum Source {
    Exact(&'static str),
    NonRoyal(&'static str),
    QueenIdentity,
    Minor,
    MinorExcept(&'static str),
}
#[derive(Clone, Copy)]
enum Mutation {
    /// Local, older transform functions preserve previous minor ability state.
    Transform(&'static str),
    /// transformCardPiece clears former wizard/trickster state and animates.
    GenericTransform(&'static str),
    /// Internal three/five helpers set capture lock but do not change origin.
    InternalTransform(&'static str),
    Grant(&'static str),
    RandomThief,
    RandomBrutus,
    RandomRecurrence,
    PoisonPawns,
    GhostPawns,
    Sacrificial(&'static str),
    SideFlag(&'static str, bool),
    RandomShield,
    Selection(&'static str),
    Missionary,
    BabyBear,
    Submerge,
    Necromancy,
    Exile,
    Judgment,
    Evacuation,
    Desperado,
    Bribe,
    Windmill,
    QueensGambit,
    Chain,
    FeudalContract,
    Guard,
    Freeze,
    Alekhine,
    Evasion,
    Scarecrow,
    Potion,
    Metal,
    Twins,
    LastResistance,
    Coronation,
    Spy,
    Wanted,
    Brainwash,
    Othello,
    Reposition,
    Taunt,
    Taboo,
    Cleanup,
    Hypocrisy,
    PortalGun,
}
#[derive(Clone, Copy)]
struct Plan {
    source: Source,
    mutation: Mutation,
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Number(value)) => value.as_f64().is_some_and(|value| value != 0.0),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}
fn catalog_hash(state: &GameState) -> Option<&str> {
    let source = state
        .extra
        .get("cardState")
        .filter(|value| truthy(Some(value)));
    let profile = match source {
        Some(source) => source.get("profile"),
        None => state.extra.get("profile"),
    };
    profile
        .and_then(|profile| profile.get("catalogHash"))
        .and_then(Value::as_str)
        .filter(|hash| !hash.is_empty())
}
fn september26(state: &GameState) -> bool {
    catalog_hash(state).is_none_or(|hash| SEPTEMBER26_HASHES.contains(&hash))
}
fn september18(state: &GameState) -> bool {
    catalog_hash(state).map_or_else(
        || state.extra.get("september18Balance") != Some(&Value::Bool(false)),
        |hash| {
            SEPTEMBER26_HASHES.contains(&hash)
                || [
                    "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc",
                    "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g",
                    "HVzv4vxzNKDRi-ylshTw9HnLmIlyiQhMScQ4caG_aSI",
                ]
                .contains(&hash)
        },
    )
}
fn current_random_pool(state: &GameState) -> Result<()> {
    if catalog_hash(state).is_some_and(|hash| hash != CATALOG) {
        return Err(EngineError::UnsupportedFeature(
            "legacy random transform pool".into(),
        ));
    }
    Ok(())
}
fn validate_profile(state: &GameState, plan: Plan) -> Result<()> {
    if matches!(plan.mutation, Mutation::Othello) && !september18(state) {
        return Err(EngineError::UnsupportedFeature(
            "legacy targeted Othello profile".into(),
        ));
    }
    if matches!(plan.mutation, Mutation::Scarecrow) && !september18(state) {
        return Err(EngineError::UnsupportedFeature(
            "legacy scarecrow installation reservations".into(),
        ));
    }
    if matches!(plan.mutation, Mutation::Grant("promotionRushUntil"))
        && (catalog_hash(state).is_some_and(|hash| hash != CATALOG)
            || (catalog_hash(state).is_none()
                && state.extra.get("internalSixFixes") == Some(&Value::Bool(false))))
    {
        return Err(EngineError::UnsupportedFeature(
            "legacy promotion-rush capture lock".into(),
        ));
    }
    if matches!(plan.mutation, Mutation::InternalTransform("parrot"))
        && (catalog_hash(state).is_some_and(|hash| hash != CATALOG)
            || (catalog_hash(state).is_none()
                && state.extra.get("parrotRookTarget") == Some(&Value::Bool(false))))
    {
        return Err(EngineError::UnsupportedFeature(
            "legacy parrot target catalog".into(),
        ));
    }
    if matches!(plan.mutation, Mutation::RandomThief)
        && catalog_hash(state).is_none()
        && state.extra.get("thiefRemake") == Some(&Value::Bool(false))
    {
        return Err(EngineError::UnsupportedFeature(
            "legacy thief transform".into(),
        ));
    }
    Ok(())
}
fn minor(state: &GameState, piece: &Piece) -> bool {
    if september26(state) {
        MINOR_CURRENT
    } else {
        MINOR_BASE
    }
    .contains(&piece.kind.as_str())
}
fn source_matches(state: &GameState, piece: &Piece, source: Source) -> bool {
    piece.color == state.turn
        && match source {
            Source::Exact(kind) => piece.kind == kind,
            Source::NonRoyal(kind) => {
                piece.kind == kind
                    && !state.royal_identity(piece)
                    && (kind != "queen"
                        || !matches!(piece.extra.get("regencyHeir"), Some(Value::Bool(true))))
            }
            Source::QueenIdentity => {
                piece.kind == "queen"
                    && !matches!(piece.extra.get("regencyHeir"), Some(Value::Bool(true)))
            }
            Source::Minor => minor(state, piece) && !state.royal_identity(piece),
            Source::MinorExcept(kind) => {
                minor(state, piece) && piece.kind != kind && !state.royal_identity(piece)
            }
        }
}
fn plan(state: &GameState, card: &CardSlot) -> Option<Plan> {
    use Mutation::*;
    use Source::*;
    let (source, mutation) = match card.id.as_str() {
        "grappler" => (
            NonRoyal("queen"),
            if september26(state) {
                Sacrificial("grappler")
            } else {
                GenericTransform("grappler")
            },
        ),
        "revolving-door" => (NonRoyal("rook"), GenericTransform("revolvingDoor")),
        "don-quixote" => (NonRoyal("rook"), GenericTransform("donQuixote")),
        "medium" => (
            if september26(state) {
                Minor
            } else {
                NonRoyal("rook")
            },
            GenericTransform("medium"),
        ),
        "paladin" => (NonRoyal("knight"), InternalTransform("paladin")),
        "octopus" => (NonRoyal("rook"), InternalTransform("octopus")),
        "clockwork" => (MinorExcept("clockwork"), InternalTransform("clockwork")),
        // The adopted catalog's parrot helper uses rook targets.
        "parrot" => (NonRoyal("rook"), InternalTransform("parrot")),
        "brutus" => (NonRoyal("rook"), RandomBrutus),
        "metal" => (Exact(""), Metal),
        "wanted" => (Exact(""), Wanted),
        "brainwash" => (Exact(""), Brainwash),
        "taboo" => (Exact(""), Taboo),
        _ => match card.effect.as_str() {
            "othello" if state.ruleset_id == RULES_VERSION_V7 => (Exact(""), Othello),
            "relay" if state.ruleset_id == RULES_VERSION_V7 => {
                (Exact(""), SideFlag("relay", false))
            }
            "reposition" if state.ruleset_id == RULES_VERSION_V7 => (Exact(""), Reposition),
            "taunt" if state.ruleset_id == RULES_VERSION_V7 => (Exact(""), Taunt),
            "cleanupPieces" => (Exact(""), Cleanup),
            "hypocrisy" => (Exact(""), Hypocrisy),
            "portalGun" => (Exact(""), PortalGun),
            "spy" => (Exact("pawn"), Spy),
            "reaper" => (QueenIdentity, Transform("reaper")),
            "idol" => (QueenIdentity, Transform("idol")),
            "herald" => (Exact("rook"), Transform("herald")),
            "missionary" => (Exact("bishop"), Missionary),
            "babyBear" => (NonRoyal("queen"), BabyBear),
            "charge" => (Exact("pawn"), Grant("chargeRush")),
            "submerge" => (Exact(""), Submerge),
            "disarm" => (Exact(""), Grant("disarmed")),
            "severance" => (Exact(""), Grant("severed")),
            "inertia" => (Exact(""), Grant("inertia")),
            "injury" => (Exact(""), SideFlag("knightInjury", true)),
            "finalWeapon" => (Exact(""), SideFlag("finalWeapon", false)),
            "royalShield" => (Exact(""), RandomShield),
            "promotionRush" => (Exact(""), Grant("promotionRushUntil")),
            "chameleonMutation" => (Exact(""), Selection("chameleon")),
            "panic" => (Exact(""), Selection("panic")),
            "desperado" => (Exact(""), Desperado),
            "judgment" => (Exact(""), Judgment),
            "exile" => (Exact(""), Exile),
            "emergencyEvacuation" => (Exact(""), Evacuation),
            "necromancy" => (Exact("pawn"), Necromancy),
            "bribe" => (Exact("knight"), Bribe),
            "windmill" => (Exact("bishop"), Windmill),
            "queensGambit" => (NonRoyal("queen"), QueensGambit),
            "chain" => (Exact(""), Chain),
            "feudalContract" => (Exact(""), FeudalContract),
            "guard" => (Exact(""), Guard),
            "freeze" => (Exact(""), Freeze),
            "alekhineMachineGun" => (Exact(""), Alekhine),
            "evasion" => (Exact(""), Evasion),
            "scarecrow" => (Exact(""), Scarecrow),
            "suspiciousPotion" => (Exact(""), Potion),
            "twins" => (Exact(""), Twins),
            "lastResistance" => (Exact(""), LastResistance),
            "coronation" => (Exact(""), Coronation),
            "wizard" => (QueenIdentity, Transform("wizard")),
            "constitutionalMonarchy" => (QueenIdentity, Transform("primeMinister")),
            "jester" => (QueenIdentity, Transform("jester")),
            "localConscription" => (QueenIdentity, Transform("recruiter")),
            "assassin" => (Exact("knight"), Transform("assassin")),
            "knightmaster" => (Exact("knight"), Transform("knightmaster")),
            "standardBearer" => (Exact("pawn"), Transform("standardBearer")),
            "log" => (Exact("pawn"), Transform("log")),
            "pegasus" => (Exact("rook"), Transform("pegasus")),
            "dragon" => (Exact("rook"), Transform("dragon")),
            "grasshopper" => (Minor, Transform("grasshopper")),
            "easternPolicy" => (Minor, Transform("cannon")),
            "campfire" => (
                if september18(state) {
                    Minor
                } else {
                    NonRoyal("rook")
                },
                GenericTransform("campfire"),
            ),
            "princess" => (NonRoyal("rook"), GenericTransform("princess")),
            "hedgehog" => (NonRoyal("queen"), GenericTransform("hedgehog")),
            "siegeRam" => (NonRoyal("rook"), GenericTransform("siegeRam")),
            "magicGirl" => (NonRoyal("rook"), GenericTransform("magicGirl")),
            "berserker" => (NonRoyal("rook"), GenericTransform("berserker")),
            "slime" => (NonRoyal("rook"), GenericTransform("slime")),
            "siren" => (NonRoyal("queen"), GenericTransform("siren")),
            "trickster" => (NonRoyal("rook"), GenericTransform("trickster")),
            "undead" => (NonRoyal("queen"), GenericTransform("undead")),
            "thief" => (NonRoyal("queen"), RandomThief),
            "amazon" => (QueenIdentity, Sacrificial("amazon")),
            "hook" => (QueenIdentity, Sacrificial("hook")),
            "ordination" => (Exact("bishop"), Sacrificial("cardinal")),
            "nullification" => (Exact(""), Grant("nullification")),
            "recurrence" => (Exact(""), RandomRecurrence),
            "outpost" => (Exact(""), Grant("outpostProtected")),
            "loyalist" => (Exact(""), Grant("loyalist")),
            "parry" => (Exact(""), Grant("parry")),
            "trojanHorse" => (Exact("knight"), Grant("trojanHorse")),
            "suicideBomber" => (Exact(""), Grant("explosive")),
            "chimera" => (Exact(""), Grant("chimera")),
            "basicTraining" => (Exact(""), Grant("basicTraining")),
            "holdout" => (Exact("pawn"), Grant("holdoutPromotion")),
            "stealth" => (Exact(""), Grant("hiddenFrom")),
            "frenzy" => (Exact("pawn"), Grant("frenzy")),
            "vip" => (Exact("pawn"), Grant("vipInvitation")),
            "stake" => (Exact(""), Grant("staked")),
            "emptyLunchbox" => (Exact(""), Grant("emptyLunchbox")),
            "witchTrial" => (Exact(""), Grant("witchTrial")),
            "ghost" => (Exact("pawn"), GhostPawns),
            "poisonedPawn" => (Exact("pawn"), PoisonPawns),
            _ => return None,
        },
    };
    Some(Plan { source, mutation })
}

fn validate_pinned_card(state: &GameState, card: &CardSlot) -> Result<()> {
    if state.ruleset_id == RULES_VERSION_V7 {
        crate::card_registry::validate_instance(state, card)?;
    }
    Ok(())
}

fn grant_matches(state: &GameState, piece: &Piece, square: Square, field: &str) -> bool {
    let own = piece.color == state.turn;
    let enemy = piece.color == state.turn.opponent();
    let excluded = |types: &[&str]| types.contains(&piece.kind.as_str());
    let present = |name| truthy(piece.extra.get(name));
    match field {
        "disarmed" | "exile" => {
            enemy && !state.royal_identity(piece) && !excluded(&["wall", "colossus"])
        }
        "severed" | "inertia" => {
            enemy && !state.royal_identity(piece) && ranged_piece(state, piece)
        }
        "shielded" => own && !excluded(&["wall", "scarecrow"]) && !present(field),
        "promotionRushUntil" => {
            own && !piece.is_large()
                && !excluded(&["pawn", "wall", "football", "monster", "blackHole"])
        }
        "chameleon" => {
            own && !state.royal_identity(piece)
                && !excluded(&["merchant", "wall", "colossus", "bigRook", "bigBishop"])
        }
        "panic" => {
            enemy
                && !state.royal_identity(piece)
                && !excluded(&[
                    "merchant",
                    "wall",
                    "football",
                    "colossus",
                    "bigRook",
                    "bigBishop",
                ])
                && !state
                    .extra
                    .get("pendingPanic")
                    .and_then(Value::as_array)
                    .is_some_and(|entries| {
                        entries.iter().any(|entry| {
                            entry
                                .get("pieces")
                                .and_then(Value::as_array)
                                .is_some_and(|pieces| {
                                    pieces.iter().any(|pending| {
                                        pending.get("id").and_then(Value::as_str) == Some(&piece.id)
                                    })
                                })
                        })
                    })
        }
        "nullification" => {
            own && !present(field)
                && !excluded(&["wall", "football", "blackHole", "monster", "coffin"])
        }
        "recurrence" => {
            own && !present(field)
                && !excluded(&["pawn", "wall", "football", "blackHole", "monster", "coffin"])
        }
        "outpostProtected" => {
            let across = match piece.color {
                PieceColor::White => square.row < 4,
                PieceColor::Black => square.row + u8::from(piece.is_large()) >= 4,
                PieceColor::Neutral => false,
            };
            own && across
                && !state.royal_identity(piece)
                && !present(field)
                && !excluded(&["wall", "football", "blackHole", "monster", "coffin"])
        }
        "loyalist" => {
            own && !present(field)
                && piece.ability_kind() != "slime"
                && !state.royal_identity(piece)
                && !piece.is_large()
                && !excluded(&[
                    "merchant",
                    "wall",
                    "football",
                    "blackHole",
                    "monster",
                    "coffin",
                ])
        }
        "parry" => {
            own && !present(field)
                && !excluded(&["wall", "football", "blackHole", "monster", "scarecrow"])
        }
        "trojanHorse" => own && piece.kind == "knight" && !present(field),
        "explosive" => own && excluded(&["pawn", "fanatic"]) && !present("feudalContractId"),
        "chimera" => own && excluded(&["knight", "bishop"]) && !present(field),
        "basicTraining" => {
            own && !present(field)
                && !piece.is_large()
                && !excluded(&[
                    "pawn",
                    "king",
                    "queen",
                    "primeMinister",
                    "jester",
                    "guard",
                    "amazon",
                    "man",
                    "idol",
                    "babyBear",
                    "bear",
                    "wall",
                    "football",
                    "blackHole",
                ])
        }
        "holdoutPromotion" => own && piece.kind == "pawn" && !present(field),
        "hiddenFrom" => own && excluded(&["bishop", "protestant"]),
        "frenzy" => own && piece.kind == "pawn",
        "vipInvitation" => piece.color.owner().is_some() && piece.kind == "pawn",
        "staked" => own && !present(field) && !excluded(&["wall", "football", "colossus"]),
        "emptyLunchbox" => {
            enemy
                && !state.royal_identity(piece)
                && !present(field)
                && !excluded(&["merchant", "wall", "football", "blackHole", "monster"])
        }
        "witchTrial" => {
            enemy
                && !state.royal_identity(piece)
                && !excluded(&[
                    "vip",
                    "merchant",
                    "wall",
                    "colossus",
                    "bigRook",
                    "bigBishop",
                ])
        }
        _ => false,
    }
}
// main:1803-1815,68455-68463. This predicate differs from movement dispatch:
// an invalid trickster copy does not fall back to queen movement here.
pub(crate) fn ranged_piece(state: &GameState, piece: &Piece) -> bool {
    let kind = if piece.kind == "trickster" {
        let Some(kind) = piece
            .extra
            .get("tricksterMoveType")
            .and_then(Value::as_str)
            .filter(|kind| TRICKSTER_TYPES.contains(kind))
        else {
            return false;
        };
        kind
    } else {
        piece.kind.as_str()
    };
    match kind {
        "brutus" | "bigBishop" | "rook" | "bishop" | "queen" | "bear" | "amazon" | "cardinal"
        | "cannon" | "herald" | "hook" | "protestant" | "windmill" | "windmillBishop"
        | "windmillRook" | "bigRook" | "jester" | "idol" => true,
        "princess" => {
            piece.ability_kind() == "princess"
                && !state.board.iter().flatten().flatten().any(|other| {
                    other.color == piece.color
                        && other.kind == "queen"
                        && !other.flag("regencyHeir")
                })
        }
        "magicGirl" => truthy(
            state
                .extra
                .get("magicGirlSurge")
                .and_then(|flags| flags.get(piece.color.as_str())),
        ),
        "berserker" => {
            let mut seen = BTreeSet::new();
            for row in 0..8 {
                for col in 0..8 {
                    if let Some(other) = state
                        .at(Square { row, col })
                        .filter(|other| other.color == piece.color)
                    {
                        seen.insert(if other.id.is_empty() {
                            format!("{row}:{col}")
                        } else {
                            other.id.clone()
                        });
                    }
                }
            }
            seen.len() <= 9
        }
        _ => false,
    }
}

// main:1435-1505. The client compares combat values after the historical
// type aliases and balance flags; unknown values are not eligible sacrifices.
pub(crate) fn combat_value(state: &GameState, piece: &Piece) -> Option<f64> {
    let kind = match piece.kind.as_str() {
        "alibaba" => "eagle",
        "unicorn" => "pegasus",
        "logRolling" => "log",
        "windmillBishop" | "windmillRook" => "windmill",
        "big-rook" => "bigRook",
        "big-bishop" => "bigBishop",
        other => other,
    };
    if !september18(state) {
        match kind {
            "grasshopper" | "campfire" => return Some(5.0),
            "checker" => return Some(2.0),
            "checkerKing" => return Some(4.0),
            _ => {}
        }
    }
    if kind == "checkerKing" && !september26(state) {
        return Some(3.0);
    }
    Some(match kind {
        "queen" | "primeMinister" | "jester" | "reaper" | "recruiter" | "wizard" | "idol"
        | "siren" | "thief" | "hedgehog" => 9.0,
        "rook" | "herald" | "pegasus" | "dragon" | "siegeRam" | "slime" | "trickster"
        | "paladin" | "octopus" | "clockwork" | "parrot" | "revolvingDoor" | "donQuixote" => 5.0,
        "bishop" | "knight" | "protestant" | "knightmaster" | "medium" => 3.0,
        "missionary" | "camel" | "log" | "standardBearer" | "guard" | "lobster" | "checkerKing"
        | "eagle" => 2.0,
        "pawn" | "fanatic" | "squire" | "checker" | "alfil" => 1.0,
        "cannon" | "grasshopper" | "man" | "assassin" | "babyBear" | "undead" | "windmill"
        | "campfire" => 4.0,
        "amazon" | "grappler" => 13.0,
        "cardinal" | "berserker" => 7.0,
        "hook" => 15.0,
        "bear" => 17.0,
        "magicGirl" | "princess" => 6.0,
        "brutus" => 10.0,
        "colossus" => 12.0,
        "bigRook" | "bigBishop" => 8.0,
        _ => return None,
    })
}
fn staked(piece: &Piece) -> bool {
    truthy(piece.extra.get("staked"))
        && js_number(
            piece
                .extra
                .get("staked")
                .and_then(|value| value.get("remaining")),
            0,
        )
        .is_some_and(|value| value > 0.0)
}
fn desperado_candidate(state: &GameState, piece: &Piece) -> bool {
    piece.color == state.turn
        && !state.royal_identity(piece)
        && ![
            "merchant",
            "wall",
            "football",
            "colossus",
            "bigRook",
            "bigBishop",
        ]
        .contains(&piece.kind.as_str())
        && !crate::movement::frozen(piece)
        && !staked(piece)
}
fn judgment_count(piece: &Piece) -> f64 {
    js_number(
        piece
            .extra
            .get("totalCaptures")
            .filter(|value| !value.is_null())
            .or_else(|| piece.extra.get("capturesMade")),
        0,
    )
    .unwrap_or(0.0)
    .max(0.0)
}
fn judgment_candidate(state: &GameState, piece: &Piece) -> bool {
    piece.color.owner().is_some()
        && !state.royal_identity(piece)
        && !piece.is_large()
        && !["wall", "football", "blackHole"].contains(&piece.kind.as_str())
}
fn judgment_matches(state: &GameState, piece: &Piece) -> bool {
    if !judgment_candidate(state, piece) || judgment_count(piece) < 2.0 {
        return false;
    }
    let maximum = state
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|other| judgment_candidate(state, other))
        .map(judgment_count)
        .fold(0.0_f64, f64::max);
    judgment_count(piece) == maximum
}
fn evacuation_candidate(state: &GameState, piece: &Piece, square: Square) -> Result<bool> {
    if piece.color != state.turn
        || piece.is_large()
        || ["wall", "football"].contains(&piece.kind.as_str())
        || (state.royal_identity(piece)
            && truthy(piece.extra.get("undergroundBunker"))
            && number_is_finite(piece.extra.get("hp")))
    {
        return Ok(false);
    }
    let Some(destination) = square.offset(-state.turn.pawn_dir(), 0) else {
        return Ok(false);
    };
    if !crate::movement::open_relocation(state, destination)? {
        return Ok(false);
    }
    if truthy(
        state
            .extra
            .get("majesty")
            .and_then(|sides| sides.get(state.turn.opponent().as_str())),
    ) {
        return Err(EngineError::UnsupportedFeature(
            "evacuation majesty movement restriction".into(),
        ));
    }
    let blocked = |item: &Piece, origin: Square| {
        !crate::movement::fianchetto_destination_allowed(state, item, origin, &[destination])
    };
    if blocked(piece, square) {
        return Ok(false);
    }
    if truthy(piece.extra.get("twinBondId"))
        && let Some(partner_id) = piece
            .extra
            .get("twinPartnerId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        && let Some((partner, origin)) = (0..8)
            .flat_map(|row| (0..8).map(move |col| Square { row, col }))
            .find_map(|origin| {
                state
                    .at(origin)
                    .filter(|other| other.id == partner_id && other.color == piece.color)
                    .map(|other| (other, origin))
            })
        && blocked(partner, origin)
    {
        return Ok(false);
    }
    Ok(true)
}
fn charge_matches(state: &GameState, piece: &Piece, square: Square) -> Result<bool> {
    if piece.color != state.turn
        || piece.kind != "pawn"
        || crate::movement::frozen(piece)
        || staked(piece)
    {
        return Ok(false);
    }
    for step in 1..=2 {
        let Some(next) = square.offset(state.turn.pawn_dir() * step, 0) else {
            return Ok(false);
        };
        if !crate::movement::open_relocation(state, next)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn submerge_matches(state: &GameState, piece: &Piece, square: Square) -> bool {
    piece.color == state.turn
        && !truthy(piece.extra.get("submerged"))
        && !piece.is_large()
        && !["wall", "football", "blackHole", "coffin"].contains(&piece.kind.as_str())
        && !crate::movement::KING
            .iter()
            .filter_map(|&(dr, dc)| square.offset(dr, dc))
            .any(|other| {
                state
                    .at(other)
                    .is_some_and(|neighbor| neighbor.color == state.turn.opponent())
            })
}
fn necromancy_types(state: &GameState) -> Vec<String> {
    state
        .captures
        .get(state.turn.opponent())
        .iter()
        .filter(|piece| piece.color == state.turn)
        .filter_map(|piece| {
            let kind = if truthy(state.extra.get("monochromeChess")) && piece.kind == "knight" {
                "camel"
            } else {
                piece.kind.as_str()
            };
            (!kind.is_empty()
                && ![
                    "bigBishop",
                    "king",
                    "royalKnight",
                    "shotgunKing",
                    "darkWizard",
                    "merchant",
                    "timeTraveler",
                    "vampireLord",
                    "colossus",
                    "bigRook",
                    "wall",
                    "scarecrow",
                    "football",
                    "monster",
                    "blackHole",
                    "coffin",
                    "crown",
                    "siegeRam",
                    "magicGirl",
                    "berserker",
                    "pawn",
                    "squire",
                ]
                .contains(&kind))
            .then(|| kind.to_owned())
        })
        .collect()
}
// main:485-499,100618-100638. The first nonempty distance class wins;
// diagonal, orthogonal and then perimeter row/column order determines RNG.
fn missionary_candidates(state: &GameState, origin: Square) -> Result<Vec<Square>> {
    for offsets in [
        &[(-1, -1), (-1, 1), (1, -1), (1, 1)][..],
        &[(-1, 0), (1, 0), (0, -1), (0, 1)][..],
    ] {
        let mut cells = Vec::with_capacity(4);
        for &(dr, dc) in offsets {
            if let Some(square) = origin.offset(dr, dc)
                && crate::movement::open_alibaba_placement(state, square, state.turn)?
            {
                cells.push(square);
            }
        }
        if !cells.is_empty() {
            return Ok(cells);
        }
    }
    for distance in 2_i8..8 {
        let mut cells = Vec::with_capacity((distance as usize) * 8);
        for row in -distance..=distance {
            for col in -distance..=distance {
                if row.abs().max(col.abs()) == distance
                    && let Some(square) = origin.offset(row, col)
                    && crate::movement::open_alibaba_placement(state, square, state.turn)?
                {
                    cells.push(square);
                }
            }
        }
        if !cells.is_empty() {
            return Ok(cells);
        }
    }
    Ok(Vec::new())
}
fn matches_plan(state: &GameState, piece: &Piece, square: Square, plan: Plan) -> Result<bool> {
    Ok(match plan.mutation {
        Mutation::Grant("chargeRush") => return charge_matches(state, piece, square),
        Mutation::Grant(field) | Mutation::Selection(field) => {
            grant_matches(state, piece, square, field)
        }
        Mutation::RandomShield => grant_matches(state, piece, square, "shielded"),
        Mutation::Submerge => submerge_matches(state, piece, square),
        Mutation::Exile => grant_matches(state, piece, square, "exile"),
        Mutation::Judgment => judgment_matches(state, piece),
        Mutation::Desperado => desperado_candidate(state, piece),
        Mutation::QueensGambit => {
            source_matches(state, piece, plan.source) && !state.flag("regency", state.turn)
        }
        Mutation::Chain => chain_target(state, piece),
        Mutation::FeudalContract => {
            piece.color == state.turn
                && ["pawn", "fanatic"].contains(&piece.kind.as_str())
                && !truthy(piece.extra.get("explosive"))
                && !truthy(piece.extra.get("feudalContractId"))
        }
        Mutation::Scarecrow => {
            piece.color == state.turn
                && !["wall", "football", "blackHole", "coffin"].contains(&piece.kind.as_str())
                && crate::movement::d4_destination_allowed(state, piece.color, &[square])
        }
        Mutation::Potion => potion_target(piece),
        Mutation::Metal => {
            piece.color == state.turn
                && !state.royal_identity(piece)
                && !truthy(piece.extra.get("metalized"))
                && ranged_piece(state, piece)
        }
        Mutation::Twins => twin_target(state, piece),
        Mutation::Spy => piece.color == state.turn.opponent() && piece.kind == "pawn",
        Mutation::Brainwash => brainwash_source(state, piece),
        Mutation::Taboo => {
            piece.color == state.turn
                && piece.kind == "queen"
                && !piece.id.is_empty()
                && !state.royal_identity(piece)
        }
        Mutation::Cleanup => cleanup_target(state, piece),
        Mutation::Evacuation => {
            return evacuation_candidate(state, piece, square);
        }
        Mutation::Necromancy => {
            source_matches(state, piece, plan.source) && !necromancy_types(state).is_empty()
        }
        Mutation::RandomRecurrence => grant_matches(state, piece, square, "recurrence"),
        Mutation::GhostPawns => {
            piece.color == state.turn && piece.kind == "pawn" && !truthy(piece.extra.get("ghost"))
        }
        Mutation::PoisonPawns => {
            piece.color == state.turn
                && piece.kind == "pawn"
                && !truthy(piece.extra.get("poisonedPawn"))
        }
        _ => source_matches(state, piece, plan.source),
    })
}
fn all_targets(state: &GameState, plan: Plan, unique: bool) -> Result<Vec<Square>> {
    let mut targets = Vec::with_capacity(64);
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if let Some(piece) = state.at(square)
                && matches_plan(state, piece, square, plan)?
                && (!matches!(plan.mutation, Mutation::Grant("outpostProtected"))
                    || grant_matches(
                        state,
                        piece,
                        normalize_square(state, square),
                        "outpostProtected",
                    ))
                && (!unique
                    || seen.insert(if piece.id.is_empty() {
                        format!("square:{row}:{col}")
                    } else {
                        format!("piece:{}", piece.id)
                    }))
            {
                targets.push(square);
            }
        }
    }
    Ok(targets)
}

fn ui_targets(state: &GameState, plan: Plan, unique: bool) -> Result<Vec<Square>> {
    let targets = all_targets(state, plan, unique)?
        .into_iter()
        .filter(|square| {
            // The generic own-queen UI branch precedes siren/undead submission.
            !matches!(
                plan.mutation,
                Mutation::GenericTransform("siren" | "undead")
            ) || state
                .at(*square)
                .is_some_and(|piece| !piece.flag("regencyHeir"))
        })
        .collect::<Vec<_>>();
    if !matches!(plan.mutation, Mutation::Exile) {
        if matches!(plan.mutation, Mutation::Chain) {
            let pairs = chain_pairs(state, &all_targets(state, plan, true)?)?;
            return Ok(targets
                .into_iter()
                .filter(|square| {
                    state.at(*square).is_some_and(|piece| {
                        pairs.iter().flatten().any(|candidate| {
                            state.at(*candidate).is_some_and(|other| {
                                if piece.id.is_empty() {
                                    square == candidate
                                } else {
                                    piece.id == other.id
                                }
                            })
                        })
                    })
                })
                .collect());
        }
        return Ok(targets);
    }
    let mut legal = Vec::with_capacity(targets.len());
    for square in targets {
        let piece = state.at(square).ok_or(EngineError::IllegalAction)?;
        if let Ok(origin) = exile_origin(piece, square)
            && crate::movement::open_relocation(state, origin)?
            && !crate::movement::collapsed(state, origin)
        {
            legal.push(square);
        }
    }
    Ok(legal)
}
/// First-click getTargetSquares surface, before the source effect is applied.
/// Multi-selection and sacrifice stages retain only their initial candidates.
pub(crate) fn target_squares(state: &GameState, card: &CardSlot) -> Result<Option<Vec<Square>>> {
    let Some(plan) = plan(state, card) else {
        return Ok(None);
    };
    validate_pinned_card(state, card)?;
    if !truthy(card.extra.get("target")) {
        // getTargetSquares itself does not check card.target. Most untargeted
        // cards have no isValidTarget branch; ghost and recurrence do.
        return Ok(Some(match plan.mutation {
            Mutation::RandomRecurrence => ui_targets(state, plan, false)?,
            Mutation::GhostPawns => {
                let mut squares = Vec::with_capacity(64);
                for row in 0..8 {
                    for col in 0..8 {
                        let square = Square { row, col };
                        if state.at(square).is_some_and(|piece| {
                            piece.color == state.turn
                                && !truthy(piece.extra.get("ghost"))
                                && !["wall", "football", "blackHole"].contains(&piece.kind.as_str())
                        }) {
                            squares.push(square);
                        }
                    }
                }
                squares
            }
            _ => Vec::new(),
        }));
    }
    validate_profile(state, plan)?;
    if matches!(plan.mutation, Mutation::Hypocrisy | Mutation::PortalGun) {
        return Ok(Some(if matches!(plan.mutation, Mutation::PortalGun) {
            portal_squares(state)?
        } else {
            hypocrisy_squares(state)?
        }));
    }
    Ok(Some(ui_targets(state, plan, false)?))
}

/// Enumerate source UI click sequences without allocating their Cartesian
/// action space. A candidate is only a distinct ordered selection; the shared
/// action stream must validate it and count it against its examination budget.
#[derive(Clone)]
pub(crate) struct OrderedSelectionCursor {
    color: Color,
    card: CardSlot,
    squares: Vec<Square>,
    indices: [usize; 4],
    length: usize,
    maximum: usize,
    exhausted: bool,
}

/// One bounded portion of a card's complete ordered UI selection space.
/// `examined` includes rejected candidates, so an empty non-exhausted page
/// still proves progress without claiming the family has no legal actions.
pub(crate) struct StagedActionPage {
    pub(crate) actions: Vec<Action>,
    pub(crate) examined: usize,
    pub(crate) exhausted: bool,
}

fn active_staged_card(state: &GameState, slot_index: usize) -> Result<&CardSlot> {
    if state.result().is_some() {
        return Err(EngineError::Terminal);
    }
    if state.mode != "play"
        || state
            .extra
            .get("pendingPromotion")
            .is_some_and(|pending| !pending.is_null())
    {
        return Err(EngineError::IllegalAction);
    }
    let card = state
        .deck_slots
        .get(state.turn)
        .get(slot_index)
        .ok_or(EngineError::IllegalAction)?;
    if card.vacant
        || card.used
        || card.recovering
        || card
            .extra
            .get("nextTurnPending")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err(EngineError::IllegalAction);
    }
    Ok(card)
}

impl OrderedSelectionCursor {
    fn new(
        color: Color,
        card: &CardSlot,
        squares: Vec<Square>,
        minimum: usize,
        maximum: usize,
    ) -> Self {
        let mut indices = [0; 4];
        for (index, slot) in indices.iter_mut().take(minimum).enumerate() {
            *slot = index;
        }
        let exhausted = squares.len() < minimum;
        Self {
            color,
            card: card.clone(),
            squares,
            indices,
            length: minimum,
            maximum,
            exhausted,
        }
    }

    pub(crate) fn next_candidate(&mut self) -> Option<Action> {
        if self.exhausted {
            return None;
        }
        let selections = self.indices[..self.length]
            .iter()
            .map(|index| self.squares[*index])
            .collect::<Vec<_>>();
        self.advance();
        Some(Action::card(
            self.color,
            &self.card,
            Some(json!({"selections":selections})),
        ))
    }

    /// The shared page cursor can report exhaustion without consuming an
    /// extra candidate outside its per-page examination budget.
    pub(crate) fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    /// Examine at most `max_examined` source UI selections from one owned
    /// deck slot. The shared ActionCursor owns board/card ordering, global
    /// checker/forced-turn gates and its page stop reason.
    pub(crate) fn next_public_page(
        &mut self,
        state: &GameState,
        slot_index: usize,
        limit: usize,
        max_examined: usize,
    ) -> Result<StagedActionPage> {
        if !(1..=4096).contains(&limit) || !(1..=4096).contains(&max_examined) {
            return Err(EngineError::InvalidConfig(
                "staged action page size and examination budget must be in 1..=4096".into(),
            ));
        }
        let card = active_staged_card(state, slot_index)?;
        if self.color != state.turn || card != &self.card {
            return Err(EngineError::IllegalAction);
        }
        let mut cursor = self.clone();
        let mut actions = Vec::with_capacity(limit.min(max_examined));
        let mut examined = 0;
        while actions.len() < limit && examined < max_examined {
            let Some(candidate) = cursor.next_candidate() else {
                break;
            };
            examined += 1;
            match ui_validate(state, card, &candidate)? {
                Some(true) => actions.push(candidate),
                Some(false) => {}
                None => {
                    return Err(EngineError::InvalidState(
                        "staged card family changed during enumeration".into(),
                    ));
                }
            }
        }
        *self = cursor;
        Ok(StagedActionPage {
            actions,
            examined,
            exhausted: self.exhausted,
        })
    }

    fn advance(&mut self) {
        let count = self.squares.len();
        for pivot in (0..self.length).rev() {
            for replacement in self.indices[pivot] + 1..count {
                if self.indices[..pivot].contains(&replacement) {
                    continue;
                }
                self.indices[pivot] = replacement;
                for index in pivot + 1..self.length {
                    let next = (0..count)
                        .find(|candidate| !self.indices[..index].contains(candidate))
                        .expect("enough distinct squares for selection");
                    self.indices[index] = next;
                }
                return;
            }
        }
        if self.length < self.maximum && count > self.length {
            self.length += 1;
            for (index, slot) in self.indices.iter_mut().take(self.length).enumerate() {
                *slot = index;
            }
        } else {
            self.exhausted = true;
        }
    }
}

/// `None` means another card family owns enumeration. The exact first-click
/// surface supplies row-major candidates; source UI toggles repeated clicks,
/// so ordered tuples never repeat a square. Raw effect legality is checked
/// separately by `ui_validate` before a candidate becomes a public action.
pub(crate) fn staged_cursor(
    state: &GameState,
    card: &CardSlot,
) -> Result<Option<OrderedSelectionCursor>> {
    let Some(plan) = plan(state, card) else {
        return Ok(None);
    };
    let (minimum, maximum) = match plan.mutation {
        Mutation::Cleanup => (1, 3),
        Mutation::PortalGun => (2, 2),
        Mutation::Hypocrisy => (4, 4),
        _ => return Ok(None),
    };
    let squares = target_squares(state, card)?.ok_or(EngineError::IllegalAction)?;
    Ok(Some(OrderedSelectionCursor::new(
        state.turn, card, squares, minimum, maximum,
    )))
}

/// Bind the staged family to an actual deck position. Callers must traverse
/// deck slots in their source order rather than sorting by card identity.
pub(crate) fn staged_cursor_for_slot(
    state: &GameState,
    slot_index: usize,
) -> Result<Option<OrderedSelectionCursor>> {
    let card = state
        .deck_slots
        .get(state.turn)
        .get(slot_index)
        .ok_or(EngineError::IllegalAction)?;
    if !plan(state, card).is_some_and(|plan| {
        matches!(
            plan.mutation,
            Mutation::Cleanup | Mutation::PortalGun | Mutation::Hypocrisy
        )
    }) {
        return Ok(None);
    }
    let card = active_staged_card(state, slot_index)?;
    staged_cursor(state, card)
}

pub(crate) fn actions(state: &GameState, card: &CardSlot) -> Result<Option<Vec<Action>>> {
    let Some(plan) = plan(state, card) else {
        return Ok(None);
    };
    validate_pinned_card(state, card)?;
    validate_profile(state, plan)?;
    if matches!(plan.mutation, Mutation::Cleanup) {
        let targets = ui_targets(state, plan, true)?;
        if targets.len() > 1 {
            // Both [a,b] and [b,a] are selectable and can produce distinct
            // ordered source results. The common ActionCursor needs a staged
            // family before it can represent this full legal space lazily.
            return Err(EngineError::UnsupportedFeature(
                "cleanup ordered public action stream".into(),
            ));
        }
        return Ok(Some(selection_actions(state, card, &targets, false)));
    }
    if matches!(plan.mutation, Mutation::Hypocrisy) {
        // Four ordered choices can exceed fifteen million Actions on an
        // empty board. Direct staged validation does not enumerate that list.
        return Err(EngineError::UnsupportedFeature(
            "hypocrisy staged four-square public actions".into(),
        ));
    }
    if matches!(plan.mutation, Mutation::PortalGun) {
        // Direct public binding validates a chosen ordered pair. Full public
        // enumeration can produce 3,782 Actions with only two kings on the
        // board, so it remains unavailable until ActionCursor consumes the
        // staged family with a per-page examination budget.
        let cursor = staged_cursor(state, card)?.ok_or(EngineError::IllegalAction)?;
        if !cursor.is_exhausted() {
            return Err(EngineError::UnsupportedFeature(
                "portal ordered public action stream".into(),
            ));
        }
        return Ok(Some(Vec::new()));
    }
    if matches!(
        plan.mutation,
        Mutation::SideFlag(..) | Mutation::Coronation | Mutation::Othello
    ) {
        return Ok(Some(vec![Action::card(state.turn, card, None)]));
    }
    if matches!(
        plan.mutation,
        Mutation::Guard
            | Mutation::Freeze
            | Mutation::Alekhine
            | Mutation::Evasion
            | Mutation::LastResistance
            | Mutation::Reposition
            | Mutation::Taunt
            | Mutation::Wanted
    ) {
        let available = match plan.mutation {
            Mutation::Guard => guard_pawn_square(state).is_some(),
            Mutation::Freeze => !freeze_candidates(state).is_empty(),
            Mutation::Alekhine => alekhine_formation(state).is_some(),
            Mutation::Evasion => !evasion_candidates(state).is_empty(),
            Mutation::LastResistance => king_augment_square(state).is_some(),
            Mutation::Reposition => reposition_available(state),
            Mutation::Taunt => taunt_available(state),
            Mutation::Wanted => !wanted_candidates(state).is_empty(),
            _ => unreachable!(),
        };
        return Ok(Some(if available {
            vec![Action::card(state.turn, card, None)]
        } else {
            Vec::new()
        }));
    }
    let targets = ui_targets(
        state,
        plan,
        matches!(
            plan.mutation,
            Mutation::Selection(_)
                | Mutation::Judgment
                | Mutation::Evacuation
                | Mutation::Chain
                | Mutation::Twins
                | Mutation::Spy
                | Mutation::Brainwash
                | Mutation::Taboo
        ),
    )?;
    let actions = match plan.mutation {
        Mutation::Spy => {
            let required = spy_candidates(state).len().min(2);
            if required == 1 {
                targets
                    .iter()
                    .map(|square| {
                        Action::card(state.turn, card, Some(json!({"selections":[square]})))
                    })
                    .collect()
            } else if required == 2 {
                selection_actions(state, card, &targets, true)
            } else {
                Vec::new()
            }
        }
        Mutation::Brainwash => {
            let mut result = Vec::new();
            for source in targets {
                let offered = state.at(source).ok_or(EngineError::IllegalAction)?;
                for victim in brainwash_victims(state, offered) {
                    result.push(Action::card(
                        state.turn,
                        card,
                        Some(json!({"selections":[source,victim]})),
                    ));
                }
            }
            result
        }
        Mutation::Taboo => {
            let destinations = taboo_destinations(state)?;
            let mut result = Vec::new();
            for source in targets {
                for destination in &destinations {
                    result.push(Action::card(
                        state.turn,
                        card,
                        Some(json!({"selections":[source,destination]})),
                    ));
                }
            }
            result
        }
        Mutation::Selection(field) => selection_actions(state, card, &targets, field == "panic"),
        Mutation::Evacuation => selection_actions(state, card, &targets, false),
        Mutation::Twins => selection_actions(state, card, &targets, true),
        Mutation::Chain => chain_pairs(state, &targets)?
            .into_iter()
            .map(|pair| Action::card(state.turn, card, Some(json!({"selections":pair}))))
            .collect(),
        Mutation::Windmill => {
            let rooks = all_targets(
                state,
                Plan {
                    source: Source::Exact("rook"),
                    mutation: Mutation::Transform(""),
                },
                false,
            )?;
            targets
                .into_iter()
                .flat_map(|bishop| {
                    rooks.iter().map(move |rook| {
                        Action::card(
                            state.turn,
                            card,
                            Some(json!({"row":rook.row,"col":rook.col,"bishop":bishop})),
                        )
                    })
                })
                .collect()
        }
        Mutation::FeudalContract => {
            let guardians = feudal_guardians(state);
            targets
                .into_iter()
                .flat_map(|pawn| {
                    guardians.iter().map(move |guardian| {
                        Action::card(
                            state.turn,
                            card,
                            Some(json!({"row":guardian.row,"col":guardian.col,"pawn":pawn})),
                        )
                    })
                })
                .collect()
        }
        Mutation::RandomThief
        | Mutation::RandomBrutus
        | Mutation::RandomRecurrence
        | Mutation::RandomShield
        | Mutation::GhostPawns
        | Mutation::PoisonPawns => {
            if matches!(
                plan.mutation,
                Mutation::RandomThief | Mutation::RandomBrutus | Mutation::PoisonPawns
            ) {
                current_random_pool(state)?;
            }
            if targets.is_empty() {
                Vec::new()
            } else {
                vec![Action::card(state.turn, card, None)]
            }
        }
        Mutation::Sacrificial(kind) => {
            let (secondary_source, field) = match kind {
                "grappler" => (Source::Minor, "minor"),
                "amazon" => (Source::Exact("knight"), "knight"),
                "hook" => (Source::Exact("rook"), "rook"),
                "cardinal" => (Source::Exact("bishop"), ""),
                _ => {
                    return Err(EngineError::UnsupportedFeature(format!(
                        "sacrificial transform {}",
                        card.id
                    )));
                }
            };
            let secondary = all_targets(
                state,
                Plan {
                    source: secondary_source,
                    mutation: Mutation::Transform(""),
                },
                true,
            )?;
            let mut actions = Vec::new();
            for primary in targets {
                let primary_piece = state.at(primary).ok_or(EngineError::IllegalAction)?;
                for second in &secondary {
                    if state
                        .at(*second)
                        .is_none_or(|piece| piece.id == primary_piece.id)
                    {
                        continue;
                    }
                    let mut target = json!(primary);
                    if field.is_empty() {
                        actions.push(Action::card(state.turn, card, Some(target)));
                        break;
                    }
                    target[field] = json!(second);
                    actions.push(Action::card(state.turn, card, Some(target)));
                }
            }
            actions
        }
        _ => targets
            .into_iter()
            .map(|square| Action::card(state.turn, card, Some(json!(square))))
            .collect(),
    };
    Ok(Some(actions))
}

// Complete UI combinations are depth-first and have maximum depth three.
// The 8x8 board bounds this collection by C(64,1)+C(64,2)+C(64,3)=43,744.
fn selection_actions(
    state: &GameState,
    card: &CardSlot,
    targets: &[Square],
    pair_only: bool,
) -> Vec<Action> {
    let mut result = Vec::new();
    for (index, first) in targets.iter().enumerate() {
        if !pair_only {
            result.push(Action::card(
                state.turn,
                card,
                Some(json!({"selections":[first]})),
            ));
        }
        for (second_index, second) in targets.iter().enumerate().skip(index + 1) {
            result.push(Action::card(
                state.turn,
                card,
                Some(json!({"selections":[first,second]})),
            ));
            if !pair_only {
                for third in targets.iter().skip(second_index + 1) {
                    result.push(Action::card(
                        state.turn,
                        card,
                        Some(json!({"selections":[first,second,third]})),
                    ));
                }
            }
        }
    }
    result
}

// main:96796-96850 and shared source normalizeChainBonds. The UI is
// exhaustive; collectAiCardTargets' value-sort/cap12 is only AI sampling.
fn chain_target(state: &GameState, piece: &Piece) -> bool {
    piece.color == state.turn.opponent()
        && !piece.is_large()
        && !["wall", "football", "blackHole"].contains(&piece.kind.as_str())
}
fn feudal_guardian(state: &GameState, piece: &Piece) -> bool {
    piece.color == state.turn
        && ![
            "pawn",
            "fanatic",
            "wall",
            "colossus",
            "bigRook",
            "bigBishop",
        ]
        .contains(&piece.kind.as_str())
}
fn feudal_guardians(state: &GameState) -> Vec<Square> {
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter(|square| {
            state
                .at(*square)
                .is_some_and(|piece| feudal_guardian(state, piece))
        })
        .collect()
}
// main:102969-102982. Existing pawn contracts are excluded by UI selection,
// but the raw handler intentionally replaces matching ledger entries.
fn apply_feudal_contract(state: &mut GameState, action: &Action) -> Result<()> {
    let target = action.target.as_ref().ok_or(EngineError::IllegalAction)?;
    let pawn_square = square_value(target.get("pawn").ok_or(EngineError::IllegalAction)?)?;
    let guardian_square = square_value(&json!({"row":target.get("row"),"col":target.get("col")}))?;
    let mut pawn = state
        .at(pawn_square)
        .filter(|piece| {
            piece.color == state.turn
                && ["pawn", "fanatic"].contains(&piece.kind.as_str())
                && !truthy(piece.extra.get("explosive"))
        })
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    let guardian = state
        .at(guardian_square)
        .filter(|piece| feudal_guardian(state, piece))
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    let suffix = crate::draft::random_suffix(state.rng.sample()?)?;
    let id = format!("feudal-{suffix}");
    pawn.extra.insert("feudalContractId".into(), json!(id));
    write_piece(state, &pawn);
    let entries = state
        .extra
        .get_mut("feudalContracts")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState("feudalContracts must be an array".into()))?;
    entries.retain(|entry| {
        entry.get("pawnId").and_then(Value::as_str) != Some(pawn.id.as_str())
            && entry.get("guardianId").and_then(Value::as_str) != Some(guardian.id.as_str())
    });
    entries.push(json!({"id":id,"color":state.turn,"pawnId":pawn.id,"guardianId":guardian.id}));
    Ok(())
}
// main:103967-103991 and findGuardPawnByOrigin:2161. Current coordinates
// select the first matching pawn's recorded origin, even after it moved.
fn guard_pawn_square(state: &GameState) -> Option<Square> {
    let default_king = Square {
        row: if state.turn == Color::White { 7 } else { 0 },
        col: 4,
    };
    let king = state
        .board
        .iter()
        .flatten()
        .flatten()
        .find(|piece| piece.color == state.turn && state.royal_identity(piece))?;
    let king_origin = exile_origin(king, default_king).unwrap_or(default_king);
    let reversed = js_number(
        state
            .extra
            .get("effects")
            .and_then(|effects| effects.get("pawnReverse"))
            .and_then(|sides| sides.get(state.turn.as_str())),
        0,
    )
    .is_some_and(|value| value > 0.0);
    let direction = state.turn.pawn_dir() * if reversed { -1 } else { 1 };
    let expected = king_origin.offset(direction, 0)?;
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .find(|square| {
            state.at(*square).is_some_and(|piece| {
                piece.color == state.turn
                    && ["pawn", "fanatic"].contains(&piece.kind.as_str())
                    && exile_origin(piece, *square).is_ok_and(|origin| origin == expected)
            })
        })
}
fn apply_guard(state: &mut GameState) -> Result<()> {
    let square = guard_pawn_square(state).ok_or(EngineError::IllegalAction)?;
    let mut piece = state
        .at(square)
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    clear_promotion_inherited_traits(state, &mut piece)?;
    piece.kind = "guard".into();
    piece.moved = true;
    mark_transformed_origin(state, &mut piece, square)?;
    mark_animation(state, &piece)?;
    write_piece(state, &piece);
    Ok(())
}
// freezeCardAvailability:74177 counts every eligible enemy before excluding
// already frozen pieces. The source's Last Warmth limit is unconditional.
fn freeze_candidates(state: &GameState) -> Vec<Square> {
    let mut seen = BTreeSet::new();
    let eligible = (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter(|square| {
            state.at(*square).is_some_and(|piece| {
                !piece.id.is_empty()
                    && piece.color == state.turn.opponent()
                    && !state.royal_identity(piece)
                    && !["wall", "football", "blackHole", "scarecrow"]
                        .contains(&piece.kind.as_str())
                    && seen.insert(piece.id.clone())
            })
        })
        .collect::<Vec<_>>();
    if eligible.len() <= 4 {
        return Vec::new();
    }
    eligible
        .into_iter()
        .filter(|square| {
            state
                .at(*square)
                .is_some_and(|piece| !crate::movement::frozen(piece))
        })
        .collect()
}
fn apply_freeze(state: &mut GameState) -> Result<()> {
    let mut candidates = freeze_candidates(state);
    if candidates.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    for index in (1..candidates.len()).rev() {
        let chosen = (state.rng.sample()? * (index + 1) as f64).floor() as usize;
        candidates.swap(index, chosen);
    }
    for square in candidates.into_iter().take(3) {
        let mut piece = state
            .at(square)
            .cloned()
            .ok_or(EngineError::IllegalAction)?;
        piece.extra.insert("frozen".into(), json!(true));
        let mut frozen = json!({"remaining":3,"source":state.turn});
        if september18(state) {
            frozen["countBy"] = json!(state.turn);
        }
        piece.extra.insert("frozenByCard".into(), frozen);
        mark_animation(state, &piece)?;
        write_piece(state, &piece);
    }
    Ok(())
}
fn evasion_candidates(state: &GameState) -> Vec<Square> {
    let mut seen = BTreeSet::new();
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter_map(|square| {
            let piece = state.at(square)?;
            if piece.color != state.turn
                || !seen.insert(piece.id.clone())
                || ["wall", "football", "blackHole", "scarecrow"].contains(&piece.kind.as_str())
                || truthy(piece.extra.get("evasion"))
            {
                return None;
            }
            Some(if piece.is_large() {
                normalize_square(state, square)
            } else {
                square
            })
        })
        .collect()
}
fn apply_evasion(state: &mut GameState) -> Result<()> {
    let candidates = evasion_candidates(state);
    if candidates.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let square = candidates[(state.rng.sample()? * candidates.len() as f64).floor() as usize];
    let mut piece = state
        .at(square)
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    piece.extra.insert("evasion".into(), json!(true));
    write_piece(state, &piece);
    Ok(())
}
// main:105000-105013 chooses the first queen and first two row-major rooks,
// with no adjacency or unobstructed-ray requirement.
fn alekhine_formation(state: &GameState) -> Option<[Square; 3]> {
    let queens = (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter(|square| {
            state
                .at(*square)
                .is_some_and(|piece| source_matches(state, piece, Source::QueenIdentity))
        })
        .collect::<Vec<_>>();
    let rooks = (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter(|square| {
            state
                .at(*square)
                .is_some_and(|piece| piece.color == state.turn && piece.kind == "rook")
        })
        .collect::<Vec<_>>();
    for queen in queens {
        let same = rooks
            .iter()
            .filter(|rook| rook.col == queen.col)
            .copied()
            .collect::<Vec<_>>();
        if same.len() < 2
            || same.iter().any(|rook| {
                if state.turn == Color::White {
                    rook.row > queen.row
                } else {
                    rook.row < queen.row
                }
            })
        {
            continue;
        }
        return Some([queen, same[0], same[1]]);
    }
    None
}
fn apply_alekhine(state: &mut GameState) -> Result<()> {
    let formation = alekhine_formation(state).ok_or(EngineError::IllegalAction)?;
    for square in formation {
        let mut piece = state
            .at(square)
            .cloned()
            .ok_or(EngineError::IllegalAction)?;
        piece.extra.insert("shielded".into(), json!(true));
        mark_animation(state, &piece)?;
        write_piece(state, &piece);
    }
    Ok(())
}
fn scarecrow_piece_reservation(state: &GameState) -> bool {
    catalog_hash(state).map_or_else(
        || state.extra.get("scarecrowPieceReservation") != Some(&Value::Bool(false)),
        |hash| {
            SEPTEMBER26_HASHES.contains(&hash)
                || [
                    "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc",
                    "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g",
                ]
                .contains(&hash)
        },
    )
}
// main:104532-104556. Royal resolution follows reservation/spawn; this is
// direct environmental removal and intentionally does not use sacrifice.
fn apply_scarecrow(state: &mut GameState, action: &Action, plan: Plan) -> Result<Vec<Piece>> {
    let square = target_square(action)?;
    let piece = state.at(square).ok_or(EngineError::IllegalAction)?;
    if !matches_plan(state, piece, square, plan)? {
        return Err(EngineError::IllegalAction);
    }
    if state
        .extra
        .get("campaign")
        .is_some_and(|campaign| !campaign.is_null())
    {
        return Err(EngineError::UnsupportedFeature(
            "scarecrow campaign sacrifice objectives".into(),
        ));
    }
    let immediate = scarecrow_piece_reservation(state);
    if !truthy(state.extra.get("pendingScarecrows")) {
        state.extra.insert("pendingScarecrows".into(), json!([]));
    }
    let actor = state.turn;
    let removed = crate::transition::scarecrow_remove(state, square, actor.opponent())?
        .ok_or(EngineError::IllegalAction)?;
    let entry = json!({"id":format!("scarecrow-pending-{}",crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?),"reserved":true,"color":actor,"by":actor,"row":square.row,"col":square.col,"remainingOwnTurns":3});
    state
        .extra
        .get_mut("pendingScarecrows")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState("pendingScarecrows must be an array".into()))?
        .push(entry);
    if immediate {
        let mut reserved = crate::opening::spawn(state, actor, "scarecrow")?;
        reserved.moved = true;
        reserved
            .extra
            .insert("scarecrowReserved".into(), json!(true));
        let id = reserved.id.clone();
        state.board[square.row as usize][square.col as usize] = Some(reserved);
        let entry = state
            .extra
            .get_mut("pendingScarecrows")
            .and_then(Value::as_array_mut)
            .and_then(|entries| entries.last_mut())
            .and_then(Value::as_object_mut)
            .ok_or_else(|| EngineError::InvalidState("scarecrow reservation disappeared".into()))?;
        entry.insert("pieceId".into(), json!(id));
        entry.insert("reserved".into(), json!(false));
    }
    crate::transition::resolve_royal_capture(state, &removed, actor.opponent())?;
    Ok(vec![removed])
}
// main:99092-99145. Twin pairing includes royal pieces; slime's copied
// ability blocks selection even when the visible type is a trickster.
fn twin_target(state: &GameState, piece: &Piece) -> bool {
    piece.color == state.turn
        && piece.ability_kind() != "slime"
        && !truthy(piece.extra.get("twinBondId"))
        && !piece.is_large()
        && !["wall", "football", "blackHole", "coffin"].contains(&piece.kind.as_str())
}
fn apply_twins(state: &mut GameState, action: &Action) -> Result<()> {
    let requested = action
        .target
        .as_ref()
        .and_then(|target| target.get("selections"))
        .and_then(Value::as_array)
        .ok_or(EngineError::IllegalAction)?;
    let mut selected = Vec::with_capacity(2);
    let mut seen = BTreeSet::new();
    for cell in requested.iter().take(2) {
        let Some(square) = loose_square(cell).map(|square| normalize_square(state, square)) else {
            continue;
        };
        if let Some(piece) = state.at(square).filter(|piece| twin_target(state, piece))
            && seen.insert(piece.id.clone())
        {
            selected.push(piece.clone());
        }
    }
    if selected.len() != 2 {
        return Err(EngineError::IllegalAction);
    }
    let suffix = crate::draft::random_suffix(state.rng.sample()?)?
        .chars()
        .take(6)
        .collect::<String>();
    let bond = format!(
        "twins-{}-{suffix}",
        crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?
    );
    let partner_ids = [selected[1].id.clone(), selected[0].id.clone()];
    for (piece, partner_id) in selected.iter_mut().zip(partner_ids) {
        piece.extra.insert("twinBondId".into(), json!(bond));
        piece
            .extra
            .insert("twinPartnerId".into(), json!(partner_id));
        mark_animation(state, piece)?;
        write_piece(state, piece);
    }
    Ok(())
}

// main:639-655,67935,104702-104723. Counts use source IDs while UI cells
// remain in board order; the raw handler consumes only its first two choices.
fn spy_candidates(state: &GameState) -> Vec<Square> {
    let mut seen = BTreeSet::new();
    let mut pawns = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if let Some(piece) = state
                .at(square)
                .filter(|piece| piece.color == state.turn.opponent() && piece.kind == "pawn")
                && seen.insert(piece.id.clone())
            {
                pawns.push(square);
            }
        }
    }
    pawns
}

fn apply_spy(state: &mut GameState, action: &Action) -> Result<()> {
    let target = action.target.as_ref().ok_or(EngineError::IllegalAction)?;
    let cells = if let Some(selections) = target.get("selections").and_then(Value::as_array) {
        selections.as_slice()
    } else {
        std::slice::from_ref(target)
    };
    let required = spy_candidates(state).len().min(2);
    if required == 0 {
        return Err(EngineError::IllegalAction);
    }
    let mut selected = Vec::with_capacity(2);
    let mut seen = BTreeSet::new();
    for cell in cells {
        let integer = |field: &str| {
            cell.get(field)
                .and_then(Value::as_f64)
                .filter(|number| number.is_finite() && number.fract() == 0.0)
        };
        let (Some(row), Some(col)) = (integer("row"), integer("col")) else {
            continue;
        };
        let square = loose_square(cell);
        let key = square
            .and_then(|square| state.at(square))
            .filter(|piece| !piece.id.is_empty())
            .map_or_else(
                || format!("square:{row}:{col}"),
                |piece| format!("piece:{}", piece.id),
            );
        if seen.insert(key) {
            selected.push(square);
            if selected.len() == 2 {
                break;
            }
        }
    }
    if selected.len() != required {
        return Err(EngineError::IllegalAction);
    }
    let mut pieces = Vec::with_capacity(required);
    for square in selected {
        let piece = square
            .and_then(|square| state.at(square))
            .filter(|piece| piece.color == state.turn.opponent() && piece.kind == "pawn")
            .cloned()
            .ok_or(EngineError::IllegalAction)?;
        pieces.push(piece);
    }
    for mut piece in pieces {
        piece.extra.insert("spyOwner".into(), json!(state.turn));
        write_piece(state, &piece);
    }
    Ok(())
}

// main:724-736 and100066. Source picks one distinct enemy ranged identity.
fn wanted_candidates(state: &GameState) -> Vec<Square> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if let Some(piece) = state.at(square)
                && piece.color == state.turn.opponent()
                && !truthy(piece.extra.get("wanted"))
                && ranged_piece(state, piece)
                && seen.insert(if piece.id.is_empty() {
                    format!("square:{row}:{col}")
                } else {
                    format!("piece:{}", piece.id)
                })
            {
                result.push(square);
            }
        }
    }
    result
}

fn apply_wanted(state: &mut GameState) -> Result<()> {
    let candidates = wanted_candidates(state);
    if candidates.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let index = crate::transition::sample_choice(state, candidates.len())?;
    let mut piece = state
        .at(candidates[index])
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    piece.extra.insert("submerged".into(), json!(true));
    piece
        .extra
        .insert("wanted".into(), json!({"by":state.turn}));
    write_piece(state, &piece);
    Ok(())
}

// main:748-760. The source requires a finite priced sacrifice and a strictly
// lower-priced enemy; expansionBoardEntries makes each ID one candidate.
fn brainwash_victims(state: &GameState, offered: &Piece) -> Vec<Square> {
    let Some(value) = combat_value(state, offered) else {
        return Vec::new();
    };
    let mut victims = Vec::new();
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if let Some(piece) = state.at(square)
                && piece.color == state.turn.opponent()
                && !state.royal_identity(piece)
                && combat_value(state, piece).is_some_and(|worth| worth < value)
                && seen.insert(if piece.id.is_empty() {
                    format!("square:{row}:{col}")
                } else {
                    format!("piece:{}", piece.id)
                })
            {
                victims.push(square);
            }
        }
    }
    victims
}

fn brainwash_source(state: &GameState, piece: &Piece) -> bool {
    piece.color == state.turn
        && !state.royal_identity(piece)
        && !brainwash_victims(state, piece).is_empty()
}

fn selected_pair(action: &Action) -> Result<(Square, Square)> {
    let selections = action
        .target
        .as_ref()
        .and_then(|target| target.get("selections"))
        .and_then(Value::as_array)
        .filter(|cells| cells.len() == 2)
        .ok_or(EngineError::IllegalAction)?;
    Ok((
        loose_square(&selections[0]).ok_or(EngineError::IllegalAction)?,
        loose_square(&selections[1]).ok_or(EngineError::IllegalAction)?,
    ))
}

fn apply_brainwash(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    let (source, target) = selected_pair(action)?;
    let offered = state
        .at(source)
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    let victim = state
        .at(target)
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    if !brainwash_source(state, &offered)
        || !brainwash_victims(state, &offered)
            .into_iter()
            .any(|square| state.at(square).is_some_and(|piece| piece.id == victim.id))
    {
        return Err(EngineError::IllegalAction);
    }
    let sacrificed = crate::transition::expansion_sacrifice(state, source, state.turn.opponent())?
        .ok_or(EngineError::IllegalAction)?;
    if state.mode != "gameover"
        && let Some(mut converted) = state
            .at(target)
            .cloned()
            .filter(|piece| piece.id == victim.id)
    {
        converted.color = state.turn.into();
        converted.moved = true;
        converted.extra.insert("defected".into(), json!(true));
        let health = match converted.kind.as_str() {
            "colossus" => Some(3),
            "bigRook" | "bigBishop" | "big-rook" | "big-bishop" => Some(2),
            _ => None,
        };
        if let Some(health) = health {
            converted.extra.insert("hp".into(), json!(health));
            converted.extra.insert("maxHp".into(), json!(health));
        }
        mark_transformed_origin_with_options(state, &mut converted, target, true)?;
        converted.extra.shift_remove("freshNoCaptureUntil");
        converted
            .extra
            .insert("coolGuyCapturedLast".into(), json!(false));
        mark_animation(state, &converted)?;
        write_piece(state, &converted);
    }
    Ok(vec![sacrificed])
}

fn taboo_destinations(state: &GameState) -> Result<Vec<Square>> {
    let mut squares = Vec::with_capacity(64);
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if crate::movement::open_placement(state, square, None)? {
                squares.push(square);
            }
        }
    }
    Ok(squares)
}

// main:761-769. The ordered reservation is consumed at a later turn boundary.
fn apply_taboo(state: &mut GameState, action: &Action) -> Result<()> {
    let (from, to) = selected_pair(action)?;
    let piece = state.at(from).ok_or(EngineError::IllegalAction)?;
    if piece.color != state.turn
        || piece.id.is_empty()
        || piece.kind != "queen"
        || state.royal_identity(piece)
        || !crate::movement::open_placement(state, to, None)?
    {
        return Err(EngineError::IllegalAction);
    }
    let entry = json!({"color":state.turn,"pieceId":piece.id,"square":to});
    if !truthy(state.extra.get("tabooPending")) {
        state.extra.insert("tabooPending".into(), json!([]));
    }
    let pending = state
        .extra
        .get_mut("tabooPending")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::UnsupportedFeature("non-array taboo pending".into()))?;
    if pending.len() >= 4096 {
        return Err(EngineError::UnsupportedFeature(
            "taboo pending capacity".into(),
        ));
    }
    pending.push(entry);
    Ok(())
}

// main:73807-73813,99988-100002,105846. Cleanup removes each selected
// identity directly; source does not treat this as a capture or sacrifice.
fn cleanup_target(state: &GameState, piece: &Piece) -> bool {
    piece.color == state.turn
        && !state.royal_identity(piece)
        && !piece.is_large()
        && !["wall", "football", "blackHole", "monster", "coffin"].contains(&piece.kind.as_str())
}

fn apply_cleanup(state: &mut GameState, action: &Action) -> Result<()> {
    let cells = action
        .target
        .as_ref()
        .and_then(|target| target.get("selections"))
        .and_then(Value::as_array)
        .ok_or(EngineError::IllegalAction)?;
    let mut selected = Vec::with_capacity(3);
    let mut seen = BTreeSet::new();
    for cell in cells {
        let integer = |field: &str| {
            cell.get(field)
                .and_then(Value::as_f64)
                .filter(|number| number.is_finite() && number.fract() == 0.0)
        };
        let (Some(row), Some(col)) = (integer("row"), integer("col")) else {
            continue;
        };
        // uniqueSelectionCellsByPiece discards non-integers, but retains an
        // integer outside the board and then rejects it in the target check.
        if !(0.0..8.0).contains(&row) || !(0.0..8.0).contains(&col) {
            return Err(EngineError::IllegalAction);
        }
        let square = Square {
            row: row as u8,
            col: col as u8,
        };
        let key = state
            .at(square)
            .filter(|piece| !piece.id.is_empty())
            .map_or_else(
                || format!("square:{}:{}", square.row, square.col),
                |piece| format!("piece:{}", piece.id),
            );
        if seen.insert(key) {
            selected.push(square);
            if selected.len() > 3 {
                return Err(EngineError::IllegalAction);
            }
        }
    }
    if selected.is_empty()
        || selected.iter().any(|square| {
            !state
                .at(*square)
                .is_some_and(|piece| cleanup_target(state, piece))
        })
    {
        return Err(EngineError::IllegalAction);
    }
    for square in selected {
        let piece = state
            .at(square)
            .cloned()
            .ok_or(EngineError::IllegalAction)?;
        if piece.id.is_empty() {
            state.board[square.row as usize][square.col as usize] = None;
        } else {
            crate::transition::clear_piece(state, &piece.id);
        }
        crate::transition::grant_vigilance_protection(state, &piece)?;
    }
    Ok(())
}

// main:73815-73822 and99969-99986. Only the raw four-cell mutation and
// first-click surface live here until public staged actions can be lazy.
fn hypocrisy_squares(state: &GameState) -> Result<Vec<Square>> {
    let mut cells = Vec::with_capacity(64);
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if crate::movement::open_alibaba_placement(state, square, state.turn.opponent())? {
                cells.push(square);
            }
        }
    }
    Ok(cells)
}

fn apply_hypocrisy(state: &mut GameState, action: &Action) -> Result<()> {
    let cells = action
        .target
        .as_ref()
        .and_then(|target| target.get("selections"))
        .and_then(Value::as_array)
        .ok_or(EngineError::IllegalAction)?;
    let mut selected = Vec::with_capacity(4);
    let mut seen = BTreeSet::new();
    for cell in cells {
        let square = loose_square(cell).ok_or(EngineError::IllegalAction)?;
        if seen.insert(square) {
            selected.push(square);
            if selected.len() > 4 {
                return Err(EngineError::IllegalAction);
            }
        }
    }
    if selected.len() != 4 {
        return Err(EngineError::IllegalAction);
    }
    for square in &selected {
        if !crate::movement::open_alibaba_placement(state, *square, state.turn.opponent())? {
            return Err(EngineError::IllegalAction);
        }
    }
    let enemy = state.turn.opponent();
    for (index, square) in selected.into_iter().enumerate() {
        let suffix = crate::draft::random_suffix(state.rng.sample()?)?
            .chars()
            .take(5)
            .collect::<String>();
        let piece: Piece = serde_json::from_value(json!({
            "id":format!("hypocrisy-{}-{index}-{suffix}", crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?),
            "type":"pawn",
            "color":enemy,
            "moved":true,
            "origin":format!("{}{}",char::from(b'a'+square.col),8-square.row),
        }))
        .map_err(EngineError::serialization)?;
        state.board[square.row as usize][square.col as usize] = Some(piece.clone());
        mark_animation(state, &piece)?;
    }
    Ok(())
}

// main:67957-67977,68010-68020,100597-100611. The two clicks are ordered:
// source keeps the first insertion order in pendingPortals.cells.
fn portal_squares(state: &GameState) -> Result<Vec<Square>> {
    let mut cells = Vec::with_capacity(64);
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if crate::movement::open_portal_reservation(state, square, state.turn)? {
                cells.push(square);
            }
        }
    }
    Ok(cells)
}

fn apply_portal_gun(state: &mut GameState, action: &Action) -> Result<()> {
    let cells = action
        .target
        .as_ref()
        .and_then(|target| target.get("selections"))
        .and_then(Value::as_array)
        .ok_or(EngineError::IllegalAction)?;
    let mut selected = Vec::with_capacity(2);
    let mut seen = BTreeSet::new();
    for cell in cells {
        let square = loose_square(cell).ok_or(EngineError::IllegalAction)?;
        if seen.insert(square) {
            selected.push(square);
            if selected.len() > 2 {
                return Err(EngineError::IllegalAction);
            }
        }
    }
    if selected.len() != 2 {
        return Err(EngineError::IllegalAction);
    }
    for square in &selected {
        if !crate::movement::open_portal_reservation(state, *square, state.turn)? {
            return Err(EngineError::IllegalAction);
        }
    }
    let trigger_turn = state
        .turns_taken
        .get(state.turn)
        .checked_add(1)
        .ok_or_else(|| EngineError::UnsupportedFeature("portal trigger turn overflow".into()))?;
    let timestamp = crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?;
    let suffix = crate::draft::random_suffix(state.rng.sample()?)?
        .chars()
        .take(5)
        .collect::<String>();
    let entry = json!({
        "id":format!("portal-gun-{timestamp}-{suffix}"),
        "color":state.turn,
        "cells":selected,
        "triggerTurn":trigger_turn,
    });
    if !state
        .extra
        .get("pendingPortals")
        .is_some_and(Value::is_array)
    {
        state.extra.insert("pendingPortals".into(), json!([]));
    }
    let pending = state
        .extra
        .get_mut("pendingPortals")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState("pendingPortals must be an array".into()))?;
    if pending.len() >= 4096 {
        return Err(EngineError::UnsupportedFeature(
            "portal pending capacity".into(),
        ));
    }
    pending.push(entry);
    Ok(())
}

// main:2502-2517. The source normalizes the complete ledger at turn entry,
// including entries that are not yet due. Keep insertion order because the
// last due entry wins even when several reservations share a trigger turn.
fn normalize_pending_portals(state: &GameState) -> Result<Vec<Value>> {
    let Some(entries) = state.extra.get("pendingPortals").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    if entries.len() > 4096 {
        return Err(EngineError::UnsupportedFeature(
            "portal pending ledger capacity".into(),
        ));
    }
    let mut normalized = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let Some(color @ ("white" | "black")) = entry.get("color").and_then(Value::as_str) else {
            continue;
        };
        let mut cells = Vec::with_capacity(2);
        if let Some(source_cells) = entry.get("cells").and_then(Value::as_array) {
            if source_cells.len() > 4096 {
                return Err(EngineError::UnsupportedFeature(
                    "portal pending cells capacity".into(),
                ));
            }
            for cell in source_cells {
                let Some(row) = js_number(cell.get("row"), 0) else {
                    continue;
                };
                let Some(col) = js_number(cell.get("col"), 0) else {
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
                if !cells.contains(&square) {
                    cells.push(square);
                }
            }
        }
        if cells.len() != 2 {
            continue;
        }
        let id = match entry.get("id").filter(|value| truthy(Some(value))) {
            None => format!("pending-portal-{index}"),
            Some(Value::String(value)) => value.clone(),
            Some(Value::Bool(value)) => value.to_string(),
            Some(Value::Number(value)) => {
                let number = value.as_f64().ok_or_else(|| {
                    EngineError::UnsupportedFeature("portal pending id number".into())
                })?;
                if number.fract() == 0.0 && number.abs() < (1u64 << 53) as f64 {
                    format!("{number:.0}")
                } else {
                    return Err(EngineError::UnsupportedFeature(
                        "portal pending noninteger id number".into(),
                    ));
                }
            }
            Some(_) => {
                return Err(EngineError::UnsupportedFeature(
                    "portal pending compound id".into(),
                ));
            }
        };
        let trigger = js_number(entry.get("triggerTurn"), 0)
            .filter(|number| number.is_finite())
            .unwrap_or(0.0)
            .floor()
            .max(0.0);
        if trigger >= (1u64 << 53) as f64 {
            return Err(EngineError::UnsupportedFeature(
                "portal trigger turn exceeds safe integer".into(),
            ));
        }
        let mut value = serde_json::Map::new();
        value.insert("id".into(), json!(id));
        if entry.get("blocksMovement") == Some(&Value::Bool(true)) {
            value.insert("blocksMovement".into(), Value::Bool(true));
        }
        value.insert("color".into(), json!(color));
        value.insert("cells".into(), json!(cells));
        value.insert("triggerTurn".into(), json!(trigger as u64));
        normalized.push(Value::Object(value));
    }
    Ok(normalized)
}

/// Resolve portal reservations after `endMove` switches the actor (main:
/// 93664-93666,74148-74163). This kernel intentionally does not advance
/// turnsTaken, settle replay, or execute later turn-start callbacks.
pub(crate) fn resolve_pending_portals_for_turn(
    state: &mut GameState,
    color: Color,
) -> Result<bool> {
    let normalized = normalize_pending_portals(state)?;
    let current_turn = u64::from(*state.turns_taken.get(color));
    let mut selected = None;
    let mut due_ids = BTreeSet::new();
    for entry in &normalized {
        if entry.get("color").and_then(Value::as_str) == Some(color.as_str())
            && entry
                .get("triggerTurn")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                <= current_turn
        {
            if let Some(id) = entry.get("id").and_then(Value::as_str) {
                due_ids.insert(id.to_owned());
            }
            selected = Some(entry.clone());
        }
    }
    if let Some(selected) = &selected
        && selected.get("blocksMovement") == Some(&Value::Bool(true))
    {
        // The legacy movement-blocking path may crush a concealed occupant,
        // trigger royal capture, and add a vanish replay visual. The ordinary
        // Portal Gun never writes this flag.
        return Err(EngineError::UnsupportedFeature(
            "movement-blocking portal installation".into(),
        ));
    }
    let remaining = normalized
        .into_iter()
        .filter(|entry| {
            !entry
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| due_ids.contains(id))
        })
        .collect::<Vec<_>>();
    state
        .extra
        .insert("pendingPortals".into(), json!(remaining));
    let Some(selected) = selected else {
        return Ok(false);
    };
    let cells = selected
        .get("cells")
        .and_then(Value::as_array)
        .ok_or_else(|| EngineError::InvalidState("normalized portal cells missing".into()))?;
    let cells = cells
        .iter()
        .map(|cell| {
            serde_json::from_value::<Square>(cell.clone()).map_err(EngineError::serialization)
        })
        .collect::<Result<Vec<_>>>()?;
    if cells
        .iter()
        .any(|square| crate::movement::portal_installation_hazard(state, *square))
    {
        crate::replay::add_log(
            state,
            "포탈 건: 설치 예정 칸이 막혀 포탈 설치가 취소되었습니다.".into(),
        )?;
        return Ok(false);
    }
    state
        .extra
        .insert("portalRule".into(), json!({"enabled":true,"cells":cells}));
    let names = cells
        .iter()
        .map(|square| format!("{}{}", char::from(b'a' + square.col), 8 - square.row))
        .collect::<Vec<_>>()
        .join("·");
    crate::replay::add_log(state, format!("포탈 건: {names}에 포탈이 설치되었습니다."))?;
    Ok(true)
}
// main:67810 and103287. This is the first actual royal identity in board
// order, including the source's active regency heir.
fn king_augment_square(state: &GameState) -> Option<Square> {
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .find(|square| {
            state
                .at(*square)
                .is_some_and(|piece| piece.color == state.turn && state.royal_identity(piece))
        })
}
fn temporary_protection(piece: &mut Piece, field: &str, owner: Color) {
    let previous = if field == "lastResistance" && truthy(piece.extra.get(field)) {
        truthy(
            piece
                .extra
                .get(field)
                .and_then(|entry| entry.get("previousProtected")),
        )
    } else {
        truthy(piece.extra.get("protected"))
    };
    piece.extra.insert(
        field.into(),
        json!({"by":owner,"remaining":3,"previousProtected":previous}),
    );
    piece.extra.insert("protected".into(), json!(true));
}
fn apply_last_resistance(state: &mut GameState) -> Result<()> {
    let square = king_augment_square(state).ok_or(EngineError::IllegalAction)?;
    let mut piece = state
        .at(square)
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    temporary_protection(&mut piece, "lastResistance", state.turn);
    write_piece(state, &piece);
    Ok(())
}
// v7 main-OahWs0tU.js:95363-95366,104799-104807. The untargeted
// reposition effect clears the owner's older marks, then marks eligible pieces
// and queues their animation in board iteration order without sampling RNG.
fn reposition_available(state: &GameState) -> bool {
    state.board.iter().flatten().flatten().any(|piece| {
        piece.color == state.turn && !["wall", "football"].contains(&piece.kind.as_str())
    })
}
fn apply_reposition(state: &mut GameState, action: &Action) -> Result<()> {
    if action.target.is_some() || !reposition_available(state) {
        return Err(EngineError::IllegalAction);
    }
    let color = state.turn;
    for piece in state.board.iter_mut().flatten().flatten() {
        if piece.color == color {
            piece.extra.shift_remove("repositionSecondMove");
        }
    }
    for row in 0..state.board.len() {
        for col in 0..state.board[row].len() {
            let marked = state.board[row][col].as_mut().and_then(|piece| {
                (piece.color == color && !["wall", "football"].contains(&piece.kind.as_str())).then(
                    || {
                        piece
                            .extra
                            .insert("repositionSecondMove".into(), json!({"used":false}));
                        piece.clone()
                    },
                )
            });
            if let Some(piece) = marked {
                mark_animation(state, &piece)?;
            }
        }
    }
    Ok(())
}
// v7 main-OahWs0tU.js:68479-68481,104541-104547. Taunt affects the
// opposing side's next turn and is available even when its only target is a
// royal piece; only wall, football and blackHole are excluded.
fn taunt_available(state: &GameState) -> bool {
    state.board.iter().flatten().flatten().any(|piece| {
        piece.color == state.turn.opponent()
            && !["wall", "football", "blackHole"].contains(&piece.kind.as_str())
    })
}
fn apply_taunt(state: &mut GameState, action: &Action) -> Result<()> {
    if action.target.is_some() || !taunt_available(state) {
        return Err(EngineError::IllegalAction);
    }
    let target = state.turn.opponent().as_str();
    if !truthy(state.extra.get("taunt")) {
        state
            .extra
            .insert("taunt".into(), json!({"white":0,"black":0}));
    }
    let counters = state
        .extra
        .get_mut("taunt")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState("taunt color map missing".into()))?;
    let previous = match counters.get(target) {
        None | Some(Value::Null) => 0,
        Some(value) => value
            .as_u64()
            .ok_or_else(|| EngineError::InvalidState("taunt counter must be unsigned".into()))?,
    };
    counters.insert(target.into(), json!(previous.max(1)));
    Ok(())
}
fn apply_side_flag(state: &mut GameState, field: &str, color: Color) -> Result<()> {
    if !truthy(state.extra.get(field)) {
        state
            .extra
            .insert(field.into(), json!({"white":false,"black":false}));
    }
    state
        .extra
        .get_mut(field)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState(format!("{field} color map missing")))?
        .insert(color.as_str().into(), json!(true));
    Ok(())
}
fn potion_target(piece: &Piece) -> bool {
    piece.color.owner().is_some()
        && !piece.is_large()
        && !["wall", "football", "blackHole", "monster", "coffin"].contains(&piece.kind.as_str())
}
// main:3689-3707 and suspiciousPotion:99691. Royal status and ranged
// movement are source option predicates, rather than physical king type.
pub(crate) fn potion_pool(state: &GameState, piece: &Piece) -> Vec<&'static str> {
    let royal = state.royal_identity(piece);
    let ranged = ranged_piece(state, piece);
    let current_copy_pool = catalog_hash(state).map_or_else(
        || {
            !truthy(state.extra.get("campaign"))
                || state.extra.get("september27CopyPools") == Some(&Value::Bool(true))
        },
        |hash| hash == CATALOG,
    );
    POTION_EFFECTS
        .iter()
        .copied()
        .filter(|effect| match *effect {
            "grapplerBound" => current_copy_pool,
            "lastResistance" => royal,
            "sacrificeProtection" => !royal,
            "basicTraining" => !royal && piece.kind != "pawn",
            "loyalist" | "chameleon" | "chimera" | "witchTrial" | "callingCard"
            | "emptyLunchbox" | "recurrence" => !royal,
            "explosive" | "poisonedPawn" | "queensGambitProtection" => piece.kind == "pawn",
            "trojanHorse" => piece.kind == "knight",
            "severance" | "inertia" => ranged,
            _ => true,
        })
        .collect()
}
fn potion_effect_active(piece: &Piece, effect: &str) -> bool {
    if !POTION_EFFECTS.contains(&effect) {
        return false;
    }
    match effect {
        "mannerNoCapture" => {
            truthy(piece.extra.get("potionManner"))
                && truthy(piece.extra.get("coolGuyCapturedLast"))
        }
        "saturationNoCapture" => {
            truthy(piece.extra.get("potionSaturation"))
                && js_number(piece.extra.get("capturesMade"), 0).is_some_and(|value| value >= 3.0)
        }
        "poisonStun" => {
            js_number(piece.extra.get("poisonStunTurns"), 0).is_some_and(|value| value > 0.0)
        }
        "queensGambitProtection" => truthy(piece.extra.get("protected")),
        effect => truthy(piece.extra.get(match effect {
            "shield" => "shielded",
            "stealth" => "hiddenFrom",
            "submerge" => "submerged",
            "freeze" => "frozen",
            "stake" => "staked",
            "disarm" => "disarmed",
            "severance" => "severed",
            "outpostProtection" => "outpostProtected",
            _ => effect,
        })),
    }
}
// main:3872-3882. Rendering and endMove both call this state mutation.
// Non-array provenance is ignored by pruning, unlike the later note spread.
pub(crate) fn prune_potion_effects(piece: &mut Piece) -> Result<()> {
    let nested = piece
        .extra
        .get("attributes")
        .filter(|value| truthy(Some(value)));
    let provenance = nested.map_or_else(
        || piece.extra.get("potionEffects"),
        |attributes| attributes.get("potionEffects"),
    );
    let Some(Value::Array(entries)) = provenance else {
        return Ok(());
    };
    if nested.is_some() {
        return Err(EngineError::UnsupportedFeature(
            "raw potion piece with nested canonical attributes".into(),
        ));
    }
    let mut ids = Vec::with_capacity(POTION_EFFECTS.len());
    let mut seen = BTreeSet::new();
    for entry in entries {
        if let Some(id) = entry.as_str()
            && potion_effect_active(piece, id)
            && seen.insert(id)
        {
            ids.push(id.to_owned());
        }
    }
    if ids.is_empty() {
        piece.extra.shift_remove("potionEffects");
    } else {
        piece.extra.insert("potionEffects".into(), json!(ids));
    }
    Ok(())
}
// main:3883-3888. Append the selected effect after pruning, even when its
// active predicate differs. Deleting an empty list preserves JS key order.
pub(crate) fn note_potion_effect(piece: &mut Piece, effect: &str) -> Result<()> {
    prune_potion_effects(piece)?;
    if !POTION_EFFECTS.contains(&effect) {
        return Ok(());
    }
    if truthy(piece.extra.get("attributes")) {
        return Err(EngineError::UnsupportedFeature(
            "raw potion piece with nested canonical attributes".into(),
        ));
    }
    if piece
        .extra
        .get("potionEffects")
        .is_some_and(|value| !value.is_array() && truthy(Some(value)))
    {
        return Err(EngineError::UnsupportedFeature(
            "non-array potion effect provenance spread".into(),
        ));
    }
    let mut ids: Vec<String> = piece
        .extra
        .get("potionEffects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if !ids.iter().any(|id| id == effect) {
        ids.push(effect.into());
    }
    piece.extra.insert("potionEffects".into(), json!(ids));
    Ok(())
}
fn apply_potion(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<()> {
    let square = target_square(action)?;
    let mut piece = state
        .at(square)
        .filter(|piece| potion_target(piece))
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    let pool = potion_pool(state, &piece);
    if pool.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let effect = pool[crate::transition::sample_choice(state, pool.len())?];
    let owner = piece.color.owner().ok_or(EngineError::IllegalAction)?;
    let hostile = owner.opponent();
    match effect {
        "grapplerBound" => {
            piece
                .extra
                .insert("grapplerBound".into(), json!({"untilColor":owner}));
        }
        "sacrificeProtection" | "lastResistance" => {
            temporary_protection(&mut piece, effect, owner);
        }
        "coronationProtection" => {
            piece.extra.insert("coronationProtection".into(),json!({"color":owner,"startTurn":state.turns_taken.get(owner),"previousProtected":truthy(piece.extra.get("protected"))}));
            piece.extra.insert("protected".into(), json!(true));
        }
        "shield" | "evasion" | "ghost" | "chameleon" | "loyalist" | "explosive"
        | "outpostProtection" | "nullification" | "recurrence" | "poisonedPawn" | "trojanHorse"
        | "inertia" => {
            piece.extra.insert(
                match effect {
                    "shield" => "shielded",
                    "outpostProtection" => "outpostProtected",
                    _ => effect,
                }
                .into(),
                json!(true),
            );
        }
        "basicTraining" => {
            piece.extra.insert("basicTraining".into(), json!(true));
            piece
                .extra
                .insert("potionBasicTraining".into(), json!(true));
        }
        "stealth" => {
            piece.extra.insert("hiddenFrom".into(), json!(hostile));
        }
        "parry" => {
            piece.extra.insert("parry".into(), json!({"chance":0.4}));
        }
        "submerge" => {
            piece.extra.insert("submerged".into(), json!(true));
        }
        "chimera" => {
            piece.extra.insert("chimera".into(), json!(true));
            if state.royal_identity(&piece) {
                piece.extra.insert("crownRoyal".into(), json!(true));
            }
        }
        "witchTrial" => {
            piece.extra.insert(
                "witchTrial".into(),
                json!({"by":hostile,"remaining":if september26(state){2}else{3}}),
            );
        }
        "stake" => {
            piece
                .extra
                .insert("staked".into(), json!({"by":owner,"remaining":4}));
        }
        "callingCard" => {
            piece
                .extra
                .insert("callingCard".into(), json!({"by":hostile}));
        }
        "emptyLunchbox" => {
            piece.extra.insert(
                "emptyLunchbox".into(),
                json!({"by":hostile,"deadlineTurn":u64::from(*state.turns_taken.get(owner))+3}),
            );
        }
        "poisonStun" => {
            let remaining = js_number(piece.extra.get("poisonStunTurns"), 0)
                .unwrap_or(0.0)
                .max(3.0);
            piece
                .extra
                .insert("poisonStunTurns".into(), json!(remaining));
            piece.extra.insert("poisonStunColor".into(), json!(owner));
        }
        "freeze" => {
            piece.extra.insert("frozen".into(), json!(true));
            piece.extra.insert(
                "frozenByCard".into(),
                json!({"remaining":3,"source":hostile}),
            );
        }
        "disarm" => {
            piece
                .extra
                .insert("disarmed".into(), json!({"by":hostile,"remaining":1}));
        }
        "mannerNoCapture" => {
            piece.extra.insert("potionManner".into(), json!(true));
            piece
                .extra
                .insert("coolGuyCapturedLast".into(), json!(true));
        }
        "saturationNoCapture" => {
            piece.extra.insert("potionSaturation".into(), json!(true));
            piece.extra.insert("capturesMade".into(), json!(3));
        }
        "queensGambitProtection" => {
            piece.extra.insert("protected".into(), json!(true));
            piece
                .extra
                .insert("queensGambitProtection".into(), json!(true));
        }
        "severance" => {
            piece
                .extra
                .insert("severed".into(), json!({"by":hostile,"remaining":2}));
        }
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "potion result {effect}"
            )));
        }
    }
    note_potion_effect(&mut piece, effect)?;
    if let Some(actual) = state
        .deck_slots
        .get_mut(state.turn)
        .iter_mut()
        .find(|actual| actual.instance_id == card.instance_id && actual.id == card.id)
    {
        actual
            .extra
            .insert("suspiciousPotionResultId".into(), json!(effect));
    }
    mark_animation(state, &piece)?;
    write_piece(state, &piece);
    Ok(())
}
fn chain_range(first: Square, second: Square) -> bool {
    first
        .row
        .abs_diff(second.row)
        .max(first.col.abs_diff(second.col))
        <= 2
}
fn chain_key(first: &str, second: &str) -> String {
    if first.encode_utf16().cmp(second.encode_utf16()).is_gt() {
        format!("{second}\0{first}")
    } else {
        format!("{first}\0{second}")
    }
}
fn chain_text(value: &str) -> Result<String> {
    let units = value.encode_utf16().take(160).collect::<Vec<_>>();
    String::from_utf16(&units).map_err(|_| {
        EngineError::UnsupportedFeature("chain identifier truncated inside UTF-16 surrogate".into())
    })
}
pub(crate) fn normalize_chain_bonds(value: Option<&Value>) -> Result<Vec<Value>> {
    let Some(entries) = value.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut result = Vec::with_capacity(entries.len().min(64));
    let mut seen = BTreeSet::new();
    for (index, entry) in entries.iter().take(64).enumerate() {
        let first = chain_text(entry.get("aId").and_then(Value::as_str).unwrap_or(""))?;
        let second = chain_text(entry.get("bId").and_then(Value::as_str).unwrap_or(""))?;
        if first.is_empty()
            || second.is_empty()
            || first == second
            || !seen.insert(chain_key(&first, &second))
        {
            continue;
        }
        let id = match entry
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        {
            Some(id) => chain_text(id)?,
            None => chain_text(&format!("chain-{index}-{first}-{second}"))?,
        };
        result.push(json!({"id":id,"aId":first,"bId":second,"by":if entry.get("by").and_then(Value::as_str)==Some("black"){Color::Black}else{Color::White}}));
    }
    Ok(result)
}
fn chain_pairs(state: &GameState, targets: &[Square]) -> Result<Vec<[Square; 2]>> {
    let bound = normalize_chain_bonds(state.extra.get("chainBonds"))?
        .into_iter()
        .map(|bond| {
            chain_key(
                bond["aId"].as_str().unwrap_or(""),
                bond["bId"].as_str().unwrap_or(""),
            )
        })
        .collect::<BTreeSet<_>>();
    let mut pairs = Vec::new();
    for (index, first) in targets.iter().enumerate() {
        for second in targets.iter().skip(index + 1) {
            let first_piece = state.at(*first).ok_or(EngineError::IllegalAction)?;
            let second_piece = state.at(*second).ok_or(EngineError::IllegalAction)?;
            if chain_range(*first, *second)
                && (first_piece.id.is_empty() || first_piece.id != second_piece.id)
                && (first_piece.id.is_empty()
                    || second_piece.id.is_empty()
                    || !bound.contains(&chain_key(&first_piece.id, &second_piece.id)))
            {
                pairs.push([*first, *second]);
            }
        }
    }
    Ok(pairs)
}

fn apply_windmill(state: &mut GameState, action: &Action) -> Result<()> {
    let target = action.target.as_ref().ok_or(EngineError::IllegalAction)?;
    let bishop_square = square_value(target.get("bishop").ok_or(EngineError::IllegalAction)?)?;
    let rook_square = square_value(&json!({"row":target.get("row"),"col":target.get("col")}))?;
    let mut bishop = state
        .at(bishop_square)
        .filter(|piece| piece.color == state.turn && piece.kind == "bishop")
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    let mut rook = state
        .at(rook_square)
        .filter(|piece| piece.color == state.turn && piece.kind == "rook")
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    for piece in [&mut bishop, &mut rook] {
        piece.kind = "windmill".into();
        piece.extra.insert("windmillMode".into(), json!("bishop"));
        piece.moved = true;
        mark_animation(state, piece)?;
    }
    mark_transformed_origin(state, &mut bishop, bishop_square)?;
    mark_transformed_origin(state, &mut rook, rook_square)?;
    write_piece(state, &bishop);
    write_piece(state, &rook);
    Ok(())
}

fn apply_queens_gambit(state: &mut GameState, action: &Action) -> Result<Vec<Piece>> {
    let square = normalize_square(state, target_square(action)?);
    let queen = state
        .at(square)
        .filter(|piece| {
            source_matches(state, piece, Source::NonRoyal("queen"))
                && !state.flag("regency", state.turn)
        })
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    if state
        .extra
        .get("campaign")
        .is_some_and(|campaign| !campaign.is_null())
    {
        return Err(EngineError::UnsupportedFeature(
            "Queen's Gambit campaign sacrifice objectives".into(),
        ));
    }
    let pawns = (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter(|cell| {
            state
                .at(*cell)
                .is_some_and(|piece| piece.color == state.turn && piece.kind == "pawn")
        })
        .collect::<Vec<_>>();
    let mut files = (0_u8..8)
        .filter(|col| col.abs_diff(square.col) > 1)
        .collect::<Vec<_>>();
    let occupied = files
        .iter()
        .copied()
        .filter(|col| pawns.iter().any(|pawn| pawn.col == *col))
        .collect::<Vec<_>>();
    if !occupied.is_empty() {
        files = occupied;
    }
    let random_col = files[(state.rng.sample()? * files.len() as f64).floor() as usize];
    mark_vanish_animation(state, &queen, square)?;
    let removed = crate::transition::sacrifice(state, square, state.turn.opponent())?
        .ok_or(EngineError::IllegalAction)?;
    if !truthy(state.extra.get("queensGambitFiles")) {
        state.extra.insert(
            "queensGambitFiles".into(),
            json!({"white":null,"black":null}),
        );
    }
    state
        .extra
        .get_mut("queensGambitFiles")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            EngineError::InvalidState("Queen's Gambit file state must be an object".into())
        })?
        .insert(
            state.turn.as_str().into(),
            json!({"queenCol":square.col,"randomCol":random_col}),
        );
    for pawn_square in pawns
        .into_iter()
        .filter(|pawn| pawn.col == square.col || pawn.col == random_col)
    {
        let mut pawn = state
            .at(pawn_square)
            .cloned()
            .ok_or(EngineError::IllegalAction)?;
        pawn.extra.insert(
            "queensGambitPreviousProtected".into(),
            json!(truthy(pawn.extra.get("protected"))),
        );
        pawn.extra.insert("protected".into(), json!(true));
        pawn.extra
            .insert("queensGambitProtection".into(), json!(true));
        mark_animation(state, &pawn)?;
        write_piece(state, &pawn);
    }
    crate::flow::mark_progress(state);
    Ok(vec![removed])
}

fn apply_chain(state: &mut GameState, action: &Action) -> Result<()> {
    let selections = action
        .target
        .as_ref()
        .and_then(|target| target.get("selections"))
        .and_then(Value::as_array)
        .ok_or(EngineError::IllegalAction)?;
    let mut entries = Vec::with_capacity(2);
    let mut seen = BTreeSet::new();
    for cell in selections {
        let Some(square) = loose_square(cell) else {
            if ["row", "col"].into_iter().all(|key| {
                cell.get(key)
                    .and_then(Value::as_f64)
                    .is_some_and(|value| value.is_finite() && value.fract() == 0.0)
            }) {
                return Err(EngineError::IllegalAction);
            }
            continue;
        };
        let square = normalize_square(state, square);
        let Some(piece) = state.at(square) else {
            return Err(EngineError::IllegalAction);
        };
        let key = if piece.id.is_empty() {
            format!("square:{}:{}", square.row, square.col)
        } else {
            format!("piece:{}", piece.id)
        };
        if seen.insert(key) {
            if entries.len() == 2 {
                return Err(EngineError::IllegalAction);
            }
            entries.push((square, piece.clone()));
        }
    }
    if entries.len() != 2
        || !chain_range(entries[0].0, entries[1].0)
        || entries.iter().any(|(_, piece)| !chain_target(state, piece))
    {
        return Err(EngineError::IllegalAction);
    }
    for (square, piece) in &mut entries {
        if piece.id.is_empty() {
            piece.id = format!(
                "{}-{}-chain-{}",
                piece.color.as_str(),
                piece.kind,
                crate::draft::random_suffix(state.rng.sample()?)?
            );
            state.board[square.row as usize][square.col as usize] = Some(piece.clone());
        }
    }
    let expected = chain_key(&entries[0].1.id, &entries[1].1.id);
    if normalize_chain_bonds(state.extra.get("chainBonds"))?
        .iter()
        .any(|bond| {
            chain_key(
                bond["aId"].as_str().unwrap_or(""),
                bond["bId"].as_str().unwrap_or(""),
            ) == expected
        })
    {
        return Err(EngineError::IllegalAction);
    }
    let suffix = crate::draft::random_suffix(state.rng.sample()?)?;
    let id = format!(
        "chain-{}-{suffix}",
        crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?
    );
    let new_bond = json!({"id":id,"aId":entries[0].1.id,"bId":entries[1].1.id,"by":state.turn});
    let mut bonds = match state.extra.get("chainBonds") {
        Some(Value::Array(entries)) => entries.iter().take(64).cloned().collect::<Vec<_>>(),
        value if !truthy(value) => Vec::new(),
        _ => {
            return Err(EngineError::UnsupportedFeature(
                "non-array chainBonds source spread".into(),
            ));
        }
    };
    if bonds.len() < 64 {
        bonds.push(new_bond);
    }
    state.extra.insert(
        "chainBonds".into(),
        json!(normalize_chain_bonds(Some(&json!(bonds)))?),
    );
    Ok(())
}
fn loose_square(value: &Value) -> Option<Square> {
    let coordinate = |key| {
        value
            .get(key)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && value.fract() == 0.0 && (0.0..8.0).contains(value))
            .map(|value| value as u8)
    };
    Some(Square {
        row: coordinate("row")?,
        col: coordinate("col")?,
    })
}
fn apply_selection(state: &mut GameState, action: &Action, field: &str) -> Result<()> {
    let target = action.target.as_ref().ok_or(EngineError::IllegalAction)?;
    let cells = target.get("selections").and_then(Value::as_array);
    let mut selected = Vec::with_capacity(3);
    let mut seen = BTreeSet::new();
    if field == "chameleon" {
        for cell in cells
            .map_or_else(|| std::slice::from_ref(target), Vec::as_slice)
            .iter()
            .take(3)
        {
            let Some(square) = loose_square(cell).map(|square| normalize_square(state, square))
            else {
                continue;
            };
            let Some(piece) = state
                .at(square)
                .filter(|piece| grant_matches(state, piece, square, field))
            else {
                continue;
            };
            if seen.insert(piece.id.clone()) {
                selected.push(square);
            }
        }
        if selected.is_empty() {
            return Err(EngineError::IllegalAction);
        }
        for square in selected {
            let mut piece = state
                .at(square)
                .cloned()
                .ok_or(EngineError::IllegalAction)?;
            piece.extra.insert("chameleon".into(), json!(true));
            write_piece(state, &piece);
        }
    } else {
        for cell in cells.ok_or(EngineError::IllegalAction)? {
            let Some(square) = loose_square(cell) else {
                continue;
            };
            let key = state
                .at(square)
                .filter(|piece| !piece.id.is_empty())
                .map_or_else(
                    || format!("square:{}:{}", square.row, square.col),
                    |piece| format!("piece:{}", piece.id),
                );
            if seen.insert(key) {
                selected.push(square);
            }
            if selected.len() == 2 {
                break;
            }
        }
        if selected.len() != 2
            || selected.iter().any(|square| {
                state
                    .at(*square)
                    .is_none_or(|piece| !grant_matches(state, piece, *square, field))
            })
        {
            return Err(EngineError::IllegalAction);
        }
        let pieces = selected
            .into_iter()
            .map(|square| {
                let piece = state.at(square).ok_or(EngineError::IllegalAction)?;
                Ok(json!({"id":piece.id,"row":square.row,"col":square.col}))
            })
            .collect::<Result<Vec<_>>>()?;
        if !state.extra.get("pendingPanic").is_some_and(Value::is_array) {
            state.extra.insert("pendingPanic".into(), json!([]));
        }
        let entry = json!({"color":state.turn.opponent(),"by":state.turn,"pieces":pieces});
        state
            .extra
            .get_mut("pendingPanic")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| EngineError::InvalidState("pending panic array missing".into()))?
            .push(entry);
    }
    Ok(())
}

fn square_value(target: &Value) -> Result<Square> {
    let fields = target.as_object().ok_or(EngineError::IllegalAction)?;
    if fields.len() != 2 {
        return Err(EngineError::IllegalAction);
    }
    let row = target
        .get("row")
        .and_then(Value::as_u64)
        .filter(|row| *row < 8);
    let col = target
        .get("col")
        .and_then(Value::as_u64)
        .filter(|col| *col < 8);
    match (row, col) {
        (Some(row), Some(col)) => Ok(Square {
            row: row as u8,
            col: col as u8,
        }),
        _ => Err(EngineError::IllegalAction),
    }
}
fn target_square(action: &Action) -> Result<Square> {
    square_value(action.target.as_ref().ok_or(EngineError::IllegalAction)?)
}
fn exile_origin(piece: &Piece, selected: Square) -> Result<Square> {
    let Some(origin) = piece
        .extra
        .get("origin")
        .filter(|value| truthy(Some(value)))
    else {
        return Ok(selected);
    };
    let text = origin.as_str().ok_or(EngineError::IllegalAction)?;
    let bytes = text.as_bytes();
    let Some(file) = bytes
        .first()
        .map(u8::to_ascii_lowercase)
        .filter(|file| (b'a'..=b'h').contains(file))
    else {
        return Err(EngineError::IllegalAction);
    };
    let rank = std::str::from_utf8(bytes.get(1..).ok_or(EngineError::IllegalAction)?)
        .ok()
        .filter(|rank| !rank.is_empty() && rank.bytes().all(|digit| digit.is_ascii_digit()))
        .and_then(|rank| rank.parse::<u8>().ok())
        .filter(|rank| (1..=8).contains(rank))
        .ok_or(EngineError::IllegalAction)?;
    Ok(Square {
        row: 8 - rank,
        col: file - b'a',
    })
}
fn sacrificial_target(
    state: &GameState,
    action: &Action,
    plan: Plan,
    kind: &str,
) -> Result<(Square, Square)> {
    let target = action
        .target
        .as_ref()
        .and_then(Value::as_object)
        .ok_or(EngineError::IllegalAction)?;
    let field = match kind {
        "grappler" => "minor",
        "amazon" => "knight",
        "hook" => "rook",
        "cardinal" => "",
        _ => return Err(EngineError::IllegalAction),
    };
    if target.len() != if field.is_empty() { 2 } else { 3 } {
        return Err(EngineError::IllegalAction);
    }
    let primary = square_value(&json!({"row":target.get("row"),"col":target.get("col")}))?;
    let primary = normalize_square(state, primary);
    let piece = state.at(primary).ok_or(EngineError::IllegalAction)?;
    if !source_matches(state, piece, plan.source) {
        return Err(EngineError::IllegalAction);
    }
    let secondary_source = match kind {
        "grappler" => Source::Minor,
        "amazon" => Source::Exact("knight"),
        "hook" => Source::Exact("rook"),
        "cardinal" => Source::Exact("bishop"),
        _ => return Err(EngineError::IllegalAction),
    };
    let secondary = if field.is_empty() {
        all_targets(
            state,
            Plan {
                source: secondary_source,
                mutation: Mutation::Transform(""),
            },
            true,
        )?
        .into_iter()
        .find(|square| {
            state
                .at(*square)
                .is_some_and(|second| second.id != piece.id)
        })
        .ok_or(EngineError::IllegalAction)?
    } else {
        normalize_square(
            state,
            square_value(target.get(field).ok_or(EngineError::IllegalAction)?)?,
        )
    };
    let second = state.at(secondary).ok_or(EngineError::IllegalAction)?;
    if second.id == piece.id || !source_matches(state, second, secondary_source) {
        return Err(EngineError::IllegalAction);
    }
    Ok((primary, secondary))
}
fn normalize_square(state: &GameState, square: Square) -> Square {
    let Some(piece) = state.at(square).filter(|piece| piece.is_large()) else {
        return square;
    };
    let row = piece
        .extra
        .get("anchorRow")
        .and_then(Value::as_u64)
        .filter(|row| *row < 8);
    let col = piece
        .extra
        .get("anchorCol")
        .and_then(Value::as_u64)
        .filter(|col| *col < 8);
    if let (Some(row), Some(col)) = (row, col) {
        let anchor = Square {
            row: row as u8,
            col: col as u8,
        };
        if state
            .at(anchor)
            .is_some_and(|p| p.is_large() && p.id == piece.id)
        {
            return anchor;
        }
    }
    square
}
fn write_piece(state: &mut GameState, piece: &Piece) {
    for cell in state.board.iter_mut().flatten().flatten() {
        if cell.id == piece.id {
            *cell = piece.clone();
        }
    }
}
// main:15872-15899. Remember the base movement, including the site's legacy
// state and canonical-effect fallback. This is also the general move kernel.
pub(crate) fn current_base_movement(state: &GameState, piece: &Piece) -> Option<Value> {
    fn movement_name(kind: &str) -> String {
        let mut name = String::with_capacity(kind.len());
        for character in kind.chars() {
            if character.is_ascii_uppercase() {
                name.push('-');
                name.push(character.to_ascii_lowercase());
            } else {
                name.push(character);
            }
        }
        name
    }
    let kind = movement_name(piece.ability_kind());
    let canonical = state
        .extra
        .get("cardState")
        .filter(|value| truthy(Some(value)));
    let effects =
        canonical.map_or_else(|| state.extra.get("effects"), |value| value.get("effects"));
    let find = |entries: Option<&Value>, kind: &str| {
        entries
            .and_then(Value::as_array)
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|effect| effect.get("kind").and_then(Value::as_str) == Some(kind))
            })
            .and_then(|effect| effect.get("attributes"))
            .and_then(|attributes| attributes.get("movement"))
            .filter(|movement| truthy(Some(movement)))
            .cloned()
    };
    if kind == "medium" {
        return state
            .extra
            .get("mediumMovement")
            .filter(|value| truthy(Some(value)))
            .cloned()
            .or_else(|| {
                find(
                    effects.and_then(|effects| effects.get("global")),
                    "medium-last-capture",
                )
            });
    }
    if kind == "parrot" {
        return state
            .extra
            .get("parrotMovement")
            .and_then(|sides| sides.get(piece.color.as_str()))
            .filter(|value| truthy(Some(value)))
            .cloned()
            .or_else(|| {
                find(
                    effects
                        .and_then(|effects| effects.get("colors"))
                        .and_then(|sides| sides.get(piece.color.as_str())),
                    "internal-last-movement",
                )
            });
    }
    let mut memory = json!({"type":kind});
    for field in ["logDirection", "windmillMode"] {
        if let Some(value) = piece.extra.get(field).filter(|value| truthy(Some(value))) {
            memory[field] = value.clone();
        }
    }
    if kind == "trickster"
        && let Some(copy) = piece
            .extra
            .get("tricksterMoveType")
            .filter(|value| truthy(Some(value)))
    {
        let copied = copy
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| copy.to_string());
        memory["type"] = json!(movement_name(&copied));
    }
    Some(memory)
}
pub(crate) fn remember_local_movement(state: &mut GameState, piece: &Piece) -> Result<()> {
    if piece.color.owner().is_none() || !truthy(state.extra.get("parrotMovement")) {
        return Ok(());
    }
    let memory = current_base_movement(state, piece).unwrap_or(Value::Null);
    state
        .extra
        .get_mut("parrotMovement")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState("parrot movement color map missing".into()))?
        .insert(piece.color.as_str().into(), memory);
    Ok(())
}
// main:94557-94572. Movement changes the twin counter before recording the
// ultimatum identity. The source intentionally retains at most 96 identities.
pub(crate) fn note_ultimatum_movement(state: &mut GameState, piece: &mut Piece) -> Result<()> {
    if truthy(piece.extra.get("twinBondId")) {
        let pending = js_number(piece.extra.get("twinSwapPending"), 0)
            .unwrap_or(0.0)
            .max(0.0)
            + 1.0;
        piece.extra.insert("twinSwapPending".into(), json!(pending));
    }
    if !truthy(state.extra.get("ultimatum"))
        || piece.id.is_empty()
        || piece.color.owner().is_none()
        || state.royal_identity(piece)
        || ["merchant", "wall", "football"].contains(&piece.kind.as_str())
    {
        return Ok(());
    }
    let ultimatum = state
        .extra
        .get_mut("ultimatum")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState("ultimatum state must be an object".into()))?;
    if !ultimatum.get("movedIds").is_some_and(Value::is_array) {
        ultimatum.insert("movedIds".into(), json!([]));
    }
    let identities = ultimatum
        .get_mut("movedIds")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState("ultimatum identity array missing".into()))?;
    if !identities
        .iter()
        .any(|value| value.as_str() == Some(&piece.id))
    {
        identities.push(json!(piece.id));
    }
    if identities.len() > 96 {
        identities.drain(..identities.len() - 96);
    }
    Ok(())
}
fn piece_hidden_from(state: &GameState, piece: &Piece, square: Square) -> Value {
    if let Some(hidden) = piece
        .extra
        .get("hiddenFrom")
        .filter(|value| truthy(Some(value)))
    {
        return hidden.clone();
    }
    if !truthy(state.extra.get("camouflageRule")) || state.royal_identity(piece) {
        return json!("");
    }
    let Some(owner) = piece.color.owner() else {
        return json!("");
    };
    let row = piece
        .extra
        .get("anchorRow")
        .and_then(Value::as_u64)
        .unwrap_or(u64::from(square.row));
    let col = piece
        .extra
        .get("anchorCol")
        .and_then(Value::as_u64)
        .unwrap_or(u64::from(square.col));
    let matching = (row.wrapping_add(col)).is_multiple_of(2) == (owner == Color::White);
    json!(if matching {
        owner.opponent().as_str()
    } else {
        ""
    })
}
// main:72314-72342. An earlier moved piece's origin may now contain a later
// piece; the source chooses that origin identity but remembers the destination.
pub(crate) fn set_last_move(
    state: &mut GameState,
    from: Square,
    to: Square,
    sound_name: &str,
    sound_color: Color,
    hidden_from: &str,
    moved_override: Option<&Piece>,
) -> Result<()> {
    let moved = moved_override
        .or_else(|| state.at(from))
        .or_else(|| state.at(to))
        .cloned();
    if from != to
        && let Some(remembered) = moved_override
            .or_else(|| state.at(to))
            .or(moved.as_ref())
            .cloned()
    {
        if state
            .extra
            .get("activeMetalMove")
            .is_some_and(|value| !value.is_null())
        {
            return Err(EngineError::UnsupportedFeature(
                "active metal movement memory".into(),
            ));
        }
        remember_local_movement(state, &remembered)?;
    }
    let old = state.extra.get("lastMove");
    let remembered = old
        .filter(|value| {
            truthy(value.get("idolEncoreEligible"))
                && !truthy(value.get("idolEncoreConsumed"))
                && moved.as_ref().is_some_and(|piece| {
                    value.get("soundColor").and_then(Value::as_str) == Some(piece.color.as_str())
                })
        })
        .and_then(|value| value.get("idolEncoreId"))
        .filter(|value| truthy(Some(value)))
        .cloned()
        .unwrap_or(json!(""));
    if !truthy(Some(&remembered))
        && from != to
        && let Some(piece) = moved.as_ref()
        && piece.ability_kind() != "idol"
        && piece.color.owner().is_some()
        && crate::movement::KING
            .iter()
            .filter_map(|&(dr, dc)| from.offset(dr, dc))
            .any(|square| {
                state.at(square).is_some_and(|other| {
                    other.color == piece.color
                        && other.id != piece.id
                        && other.ability_kind() == "idol"
                })
            })
    {
        return Err(EngineError::UnsupportedFeature(
            "friendly idol aura movement callback".into(),
        ));
    }
    let encore_piece = if truthy(Some(&remembered)) {
        old.and_then(|value| value.get("idolEncorePieceId"))
            .filter(|value| truthy(Some(value)))
            .or_else(|| {
                old.and_then(|value| value.get("pieceId"))
                    .filter(|value| truthy(Some(value)))
            })
            .cloned()
            .unwrap_or(json!(""))
    } else {
        json!("")
    };
    let hidden = if hidden_from.is_empty() {
        moved
            .as_ref()
            .map_or_else(|| json!(""), |piece| piece_hidden_from(state, piece, to))
    } else {
        json!(hidden_from)
    };
    if truthy(Some(&hidden)) {
        state.extra.insert("accelerationTrail".into(), Value::Null);
    }
    state.extra.insert(
        "lastMove".into(),
        json!({
            "from":from,"to":to,"pieceId":moved.as_ref().map_or("",|piece| piece.id.as_str()),
            "pieceType":moved.as_ref().map_or("",|piece| piece.kind.as_str()),
            "soundName":sound_name,"soundColor":sound_color,"hiddenFrom":hidden,
            "idolEncoreEligible":truthy(Some(&remembered)),"idolEncoreId":remembered,
            "idolEncorePieceId":encore_piece,"idolEncoreConsumed":false
        }),
    );
    Ok(())
}
pub(crate) fn track_acceleration_trail(
    state: &mut GameState,
    color: Color,
    cells: &[Square],
    force: bool,
    hidden_from: &str,
) -> Result<()> {
    if !truthy(state.extra.get("acceleration")) && !force {
        return Ok(());
    }
    let hidden = if ["white", "black"].contains(&hidden_from) {
        hidden_from
    } else {
        ""
    };
    let current = state.extra.get("accelerationTrail");
    let append = current.is_some_and(|value| {
        value.get("color").and_then(Value::as_str) == Some(color.as_str())
            && !truthy(value.get("clearOnTurnStart"))
            && value
                .get("hiddenFrom")
                .filter(|value| truthy(Some(value)))
                .and_then(Value::as_str)
                .unwrap_or("")
                == hidden
    });
    let mut trail = if append {
        current.cloned().unwrap_or(Value::Null)
    } else {
        json!({"color":color,"cells":[],"hiddenFrom":hidden})
    };
    let mut seen = BTreeSet::new();
    let mut combined = Vec::with_capacity(64);
    for cell in trail
        .get("cells")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .cloned()
        .chain(cells.iter().map(|square| json!(square)))
    {
        if let Some(square) = loose_square(&cell)
            && seen.insert(square)
        {
            combined.push(cell);
        }
    }
    trail["cells"] = json!(combined);
    state.extra.insert(
        "accelerationTrail".into(),
        if seen.is_empty() { Value::Null } else { trail },
    );
    Ok(())
}
// main:104763-104801. Selection deduplication and truncation happen before
// eligibility, and each relocation sees changes made by earlier selections.
fn apply_evacuation(state: &mut GameState, action: &Action) -> Result<()> {
    if state
        .extra
        .get("campaign")
        .and_then(|value| value.get("setup"))
        .and_then(Value::as_str)
        == Some("fogWar")
    {
        return Err(EngineError::UnsupportedFeature(
            "campaign fog relocation privacy".into(),
        ));
    }
    let selections = action
        .target
        .as_ref()
        .and_then(|target| target.get("selections"))
        .and_then(Value::as_array)
        .ok_or(EngineError::IllegalAction)?;
    let mut seen = BTreeSet::new();
    let mut selected = Vec::with_capacity(3);
    for cell in selections {
        let Some((row, col)) = cell
            .get("row")
            .and_then(Value::as_f64)
            .zip(cell.get("col").and_then(Value::as_f64))
            .filter(|(row, col)| {
                row.is_finite() && col.is_finite() && row.fract() == 0.0 && col.fract() == 0.0
            })
        else {
            continue;
        };
        let square = loose_square(cell);
        let key = square
            .and_then(|square| state.at(square))
            .filter(|piece| !piece.id.is_empty())
            .map_or_else(
                || format!("square:{row}:{col}"),
                |piece| format!("piece:{}", piece.id),
            );
        if seen.insert(key) {
            selected.push(square);
        }
        if selected.len() == 3 {
            break;
        }
    }
    let mut moved = Vec::with_capacity(3);
    for origin in selected.into_iter().flatten() {
        let Some(mut piece) = state.at(origin).cloned() else {
            continue;
        };
        if !evacuation_candidate(state, &piece, origin)? {
            continue;
        }
        let destination = origin
            .offset(-state.turn.pawn_dir(), 0)
            .ok_or(EngineError::IllegalAction)?;
        let viewer = state.turn.opponent();
        let visible_origin = state.piece_visible(&piece, origin, viewer);
        state.board[destination.row as usize][destination.col as usize] = Some(piece.clone());
        state.board[origin.row as usize][origin.col as usize] = None;
        piece.moved = true;
        crate::transition::mark_card_no_capture(state, &mut piece)?;
        note_ultimatum_movement(state, &mut piece)?;
        mark_animation(state, &piece)?;
        write_piece(state, &piece);
        let hidden = if !visible_origin || !state.piece_visible(&piece, destination, viewer) {
            viewer.as_str()
        } else {
            ""
        };
        moved.push((origin, destination, hidden));
    }
    let Some(&(first, destination, _)) = moved.first() else {
        return Err(EngineError::IllegalAction);
    };
    let hidden = moved
        .iter()
        .find_map(|(_, _, hidden)| (!hidden.is_empty()).then_some(*hidden))
        .unwrap_or("");
    let sound = if state.turn == Color::White {
        "moveSelf"
    } else {
        "moveOpponent"
    };
    set_last_move(state, first, destination, sound, state.turn, hidden, None)?;
    let trail: Vec<_> = moved
        .iter()
        .flat_map(|(from, to, _)| [*from, *to])
        .collect();
    let effective_hidden = state
        .extra
        .get("lastMove")
        .and_then(|value| value.get("hiddenFrom"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    track_acceleration_trail(state, state.turn, &trail, true, &effective_hidden)?;
    if let Some(trail) = state
        .extra
        .get_mut("accelerationTrail")
        .and_then(Value::as_object_mut)
        && trail.get("color").and_then(Value::as_str) == Some(state.turn.as_str())
    {
        trail.insert("clearOnTurnStart".into(), json!(state.turn));
    }
    crate::threat::play_move_sound(state, sound, state.turn)?;
    Ok(())
}
pub(crate) fn mark_animation(state: &mut GameState, piece: &Piece) -> Result<()> {
    if piece.id.is_empty() {
        return Ok(());
    }
    let values = animation_set(state, "forceAnimatedPieceIds", true)?;
    if !values.iter().any(|value| value.as_str() == Some(&piece.id)) {
        values.push(json!(piece.id));
    }
    Ok(())
}

fn animation_set<'a>(
    state: &'a mut GameState,
    field: &str,
    initialize: bool,
) -> Result<&'a mut Vec<Value>> {
    if initialize && !truthy(state.extra.get(field)) {
        state
            .extra
            .insert(field.into(), json!({"__simType":"Set","values":[]}));
    }
    let set = state
        .extra
        .get_mut(field)
        .ok_or_else(|| EngineError::InvalidState(format!("{field} Set missing")))?;
    if set.get("__simType").and_then(Value::as_str) != Some("Set") {
        return Err(EngineError::InvalidState(format!(
            "{field} must encode a Set"
        )));
    }
    set.get_mut("values")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState(format!("{field} Set values missing")))
}

/// Source playPieceVanishLocalEffect (94098), createPieceElement (75203),
/// and shouldAnimatePieceElement (75806), with the oracle's cold renderer
/// context at admission. Repeated ghosts only change DOM classes after the
/// first entry; the serialized Sets have already received the same mutation.
pub(crate) fn mark_vanish_animation(
    state: &mut GameState,
    piece: &Piece,
    square: Square,
) -> Result<()> {
    if piece.id.is_empty() || piece.kind == "wall" {
        return Ok(());
    }
    if state.mode != "gameover" && piece.color != state.turn {
        let reference = state
            .board
            .iter()
            .enumerate()
            .find_map(|(row, cells)| {
                cells.iter().enumerate().find_map(|(col, cell)| {
                    cell.as_ref()
                        .is_some_and(|occupant| occupant.id == piece.id)
                        .then_some(Square {
                            row: row as u8,
                            col: col as u8,
                        })
                })
            })
            .unwrap_or(square);
        if piece_hidden_from(state, piece, reference).as_str() == Some(state.turn.as_str()) {
            return Ok(());
        }
        if state
            .extra
            .get("campaign")
            .and_then(|campaign| campaign.get("setup"))
            .and_then(Value::as_str)
            == Some("fogWar")
        {
            return Err(EngineError::UnsupportedFeature(
                "vanish ghost campaign fog visibility".into(),
            ));
        }
    }
    if state
        .extra
        .get("forceAnimatedPieceIds")
        .is_some_and(|value| !value.is_null())
    {
        let forced = animation_set(state, "forceAnimatedPieceIds", false)?;
        if let Some(index) = forced
            .iter()
            .position(|value| value.as_str() == Some(&piece.id))
        {
            forced.remove(index);
        }
    }
    let values = animation_set(state, "animatedPieceIds", true)?;
    if !values.iter().any(|value| value.as_str() == Some(&piece.id)) {
        values.push(json!(piece.id));
    }
    Ok(())
}
fn capture_lock(state: &GameState, piece: &mut Piece) -> Result<()> {
    let Some(color) = piece.color.owner() else {
        return Ok(());
    };
    if piece.kind == "wall" {
        return Ok(());
    }
    let turn = *state.turns_taken.get(color);
    piece.extra.insert(
        "freshNoCaptureUntil".into(),
        json!(
            turn.checked_add(1)
                .ok_or_else(|| EngineError::InvalidState("capture lock overflow".into()))?
        ),
    );
    if piece.ability_kind() == "herald" {
        piece
            .extra
            .insert("heraldJumpUnlocked".into(), json!(false));
        piece.extra.insert("heraldJumpLockTurn".into(), json!(turn));
    }
    Ok(())
}
pub(crate) fn mark_transformed_origin(
    state: &GameState,
    piece: &mut Piece,
    square: Square,
) -> Result<()> {
    mark_transformed_origin_with_options(state, piece, square, false)
}
pub(crate) fn mark_transformed_origin_with_options(
    state: &GameState,
    piece: &mut Piece,
    square: Square,
    skip_fresh_no_capture: bool,
) -> Result<()> {
    if piece.kind != "pawn" {
        piece.extra.shift_remove("vipInvitation");
        piece.extra.shift_remove("holdoutPromotion");
    }
    if square.row >= 8 || square.col >= 8 {
        return Ok(());
    }
    piece.extra.insert(
        "origin".into(),
        json!(format!(
            "{}{}",
            char::from(b'a' + square.col),
            8 - square.row
        )),
    );
    if truthy(state.extra.get("monochromeChess")) && !truthy(piece.extra.get("monoShade")) {
        piece.extra.insert(
            "monoShade".into(),
            json!(if (square.row + square.col).is_multiple_of(2) {
                "light"
            } else {
                "dark"
            }),
        );
    }
    if !skip_fresh_no_capture {
        capture_lock(state, piece)?;
    }
    Ok(())
}
pub(crate) fn clear_promotion_inherited_traits(
    state: &mut GameState,
    piece: &mut Piece,
) -> Result<()> {
    if let Some(contract_id) = piece
        .extra
        .get("feudalContractId")
        .filter(|value| truthy(Some(value)))
        && let Some(contracts) = state
            .extra
            .get_mut("feudalContracts")
            .and_then(Value::as_array_mut)
    {
        contracts.retain(|entry| {
            entry.get("id") != Some(contract_id)
                && entry.get("pawnId").and_then(Value::as_str) != Some(&piece.id)
        });
    }
    if truthy(piece.extra.get("potionBasicTraining")) {
        piece.extra.shift_remove("basicTraining");
    }
    if truthy(piece.extra.get("potionManner")) {
        piece
            .extra
            .insert("coolGuyCapturedLast".into(), json!(false));
    }
    if truthy(piece.extra.get("potionSaturation")) {
        piece.extra.insert("capturesMade".into(), json!(0));
    }
    for field in [
        "feudalContractId",
        "shielded",
        "explosive",
        "chimera",
        "witchTrial",
        "severed",
        "potionBasicTraining",
        "potionManner",
        "potionSaturation",
        "chargeRush",
        "lastSprintPending",
        "trojanHorse",
        "vipInvitation",
    ] {
        piece.extra.shift_remove(field);
    }
    Ok(())
}
fn clear_former_minor(piece: &mut Piece, next: &str) {
    if piece.kind == next {
        return;
    }
    if matches!(piece.kind.as_str(), "wizard" | "trickster") {
        piece.extra.shift_remove("mana");
        piece.extra.shift_remove("maxMana");
    }
    if piece.kind == "trickster" {
        for field in [
            "ammo",
            "maxAmmo",
            "facing",
            "gold",
            "windmillMode",
            "logDir",
            "logRollAfterTurn",
            "logRollAfterPly",
            "heraldJumpUnlocked",
            "heraldJumpLockPly",
            "babyBearGrowAtTurn",
            "babyBearGrowthDueTurn",
            "bearRetaliationsRemaining",
            "reaperCaptures",
            "tricksterMoveType",
            "tricksterPreviousAbilityForTurn",
        ] {
            piece.extra.shift_remove(field);
        }
    }
}
fn default_if_missing(piece: &mut Piece, field: &str, value: Value) {
    if piece.extra.get(field).is_none_or(Value::is_null) {
        piece.extra.insert(field.into(), value);
    }
}
fn numeric_text_is_finite(text: &str) -> bool {
    numeric_text_number(text).is_some_and(f64::is_finite)
}
fn numeric_text_number(text: &str) -> Option<f64> {
    let text =
        text.trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}');
    if text.is_empty() {
        return Some(0.0);
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = text.strip_prefix(prefix) {
            return (!digits.is_empty())
                .then(|| {
                    digits.chars().try_fold(0.0_f64, |total, digit| {
                        let next = total * f64::from(radix) + f64::from(digit.to_digit(radix)?);
                        next.is_finite().then_some(next)
                    })
                })
                .flatten();
        }
    }
    text.parse::<f64>().ok().filter(|number| number.is_finite())
}
fn js_number(value: Option<&Value>, depth: usize) -> Option<f64> {
    if depth > 64 {
        return None;
    }
    match value? {
        Value::Null => Some(0.0),
        Value::Bool(value) => Some(f64::from(*value)),
        Value::Number(value) => value.as_f64(),
        Value::String(value) => numeric_text_number(value),
        Value::Array(values) => match values.as_slice() {
            [] => Some(0.0),
            [Value::Null] => Some(0.0),
            [value @ (Value::Array(_) | Value::Number(_) | Value::String(_))] => {
                js_number(Some(value), depth + 1)
            }
            _ => None,
        },
        _ => None,
    }
}
fn array_number_is_finite(value: &Value, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    match value {
        Value::Null => true,
        Value::String(value) => numeric_text_is_finite(value),
        Value::Number(value) => value.as_f64().is_some_and(f64::is_finite),
        Value::Bool(_) | Value::Object(_) => false,
        Value::Array(values) => match values.as_slice() {
            [] => true,
            [value] => array_number_is_finite(value, depth + 1),
            _ => false,
        },
    }
}
fn number_is_finite(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Object(_)) => false,
        Some(Value::Null | Value::Bool(_)) => true,
        Some(Value::Number(value)) => value.as_f64().is_some_and(f64::is_finite),
        Some(Value::String(value)) => numeric_text_is_finite(value),
        Some(value @ Value::Array(_)) => array_number_is_finite(value, 0),
    }
}
fn trickster_defaults(state: &mut GameState, piece: &mut Piece) -> Result<()> {
    current_random_pool(state)?;
    let index = (state.rng.sample()? * TRICKSTER_TYPES.len() as f64).floor() as usize;
    let ability = TRICKSTER_TYPES[index];
    piece
        .extra
        .insert("tricksterMoveType".into(), json!(ability));
    match ability {
        "thief" => {
            default_if_missing(piece, "submerged", json!(true));
            // The September 26 source also makes a thief-form Trickster
            // wanted. The legacy source left this field untouched.
            if state.ruleset_id == RULES_VERSION_V7 {
                default_if_missing(piece, "wanted", json!({"by":piece.color}));
            }
        }
        "wizard" => {
            default_if_missing(piece, "mana", json!(0));
            default_if_missing(piece, "maxMana", json!(5));
        }
        "windmill" => default_if_missing(piece, "windmillMode", json!("bishop")),
        "bear" | "hedgehog" => default_if_missing(piece, "bearRetaliationsRemaining", json!(2)),
        _ => {}
    }
    if ability == "reaper" && !number_is_finite(piece.extra.get("reaperCaptures")) {
        piece.extra.insert("reaperCaptures".into(), json!(0));
    }
    Ok(())
}

fn transform(
    state: &mut GameState,
    piece: &mut Piece,
    square: Square,
    mutation: Mutation,
) -> Result<()> {
    let (kind, generic, internal) = match mutation {
        Mutation::Transform(kind) => (kind, false, false),
        Mutation::GenericTransform(kind) => (kind, true, false),
        Mutation::InternalTransform(kind) => (kind, false, true),
        _ => return Err(EngineError::IllegalAction),
    };
    if generic || internal {
        clear_former_minor(piece, kind);
    }
    if matches!(kind, "reaper" | "idol") {
        clear_promotion_inherited_traits(state, piece)?;
    }
    piece.kind = kind.into();
    piece.moved = true;
    if kind == "trickster" {
        trickster_defaults(state, piece)?;
    }
    if kind == "hedgehog" {
        piece
            .extra
            .insert("bearRetaliationsRemaining".into(), json!(2));
    }
    if internal {
        capture_lock(state, piece)?;
    } else {
        mark_transformed_origin(state, piece, square)?;
    }
    match kind {
        "wizard" => {
            piece.extra.insert("mana".into(), json!(0));
            piece.extra.insert("maxMana".into(), json!(5));
        }
        "reaper" => {
            piece.extra.insert("reaperCaptures".into(), json!(0));
        }
        "log" => {
            piece.extra.insert("logDir".into(), Value::Null);
        }
        _ => {}
    }
    if generic || matches!(kind, "reaper" | "idol") {
        mark_animation(state, piece)?;
    }
    Ok(())
}
fn grant(state: &mut GameState, piece: &mut Piece, field: &str) -> Result<()> {
    let color = state.turn;
    let value = match field {
        "disarmed" => json!({"by":color,"remaining":1}),
        "severed" => json!({"by":color,"remaining":2}),
        "promotionRushUntil" => {
            json!(state.turns_taken.get(color).checked_add(1).ok_or_else(|| {
                EngineError::InvalidState("promotion rush deadline overflow".into())
            })?)
        }
        "parry" => json!({"chance":0.4}),
        "hiddenFrom" => json!(color.opponent()),
        "frenzy" => json!({"by":color}),
        "staked" => json!({"by":color,"remaining":4}),
        "emptyLunchbox" => {
            json!({"by":color,"deadlineTurn":state.turns_taken.get(color.opponent()).checked_add(3).ok_or_else(|| EngineError::InvalidState("lunchbox deadline overflow".into()))?})
        }
        "witchTrial" => {
            let mut value = json!({"by":color,"remaining":if september26(state){2}else{3}});
            if september18(state) {
                value["countBy"] = json!(color);
            }
            value
        }
        "vipInvitation" => {
            json!({"by":color,"triggerTurn":state.turns_taken.get(piece.color.owner().ok_or(EngineError::IllegalAction)?).checked_add(3).ok_or_else(|| EngineError::InvalidState("VIP deadline overflow".into()))?})
        }
        "holdoutPromotion" => {
            json!({"by":color,"readyTurn":(*state.turns_taken.get(Color::White)).min(*state.turns_taken.get(Color::Black)).checked_add(14).ok_or_else(|| EngineError::InvalidState("holdout deadline overflow".into()))?})
        }
        "nullification" | "outpostProtected" | "loyalist" | "trojanHorse" | "explosive"
        | "chimera" | "basicTraining" | "inertia" | "shielded" | "chargeRush" => json!(true),
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "piece trait {field}"
            )));
        }
    };
    piece.extra.insert(field.into(), value);
    if matches!(
        field,
        "outpostProtected"
            | "inertia"
            | "shielded"
            | "chargeRush"
            | "promotionRushUntil"
            | "nullification"
            | "parry"
            | "chimera"
            | "basicTraining"
            | "holdoutPromotion"
            | "frenzy"
            | "vipInvitation"
            | "staked"
    ) {
        mark_animation(state, piece)?;
    }
    Ok(())
}

/// Validate a completed UI selection without materializing its ordered action
/// space. Source click handlers (main:87542-87584) toggle an existing choice;
/// duplicates cannot appear in a submitted selection even though some raw
/// effect helpers silently deduplicate them. `None` preserves the existing
/// single-selection public binder.
pub(crate) fn ui_validate(
    state: &GameState,
    card: &CardSlot,
    action: &Action,
) -> Result<Option<bool>> {
    let Some(plan) = plan(state, card) else {
        return Ok(None);
    };
    validate_pinned_card(state, card)?;
    let (minimum, maximum) = match plan.mutation {
        Mutation::Cleanup => (1, 3),
        Mutation::Hypocrisy => (4, 4),
        Mutation::PortalGun => (2, 2),
        _ => return Ok(None),
    };
    let Some(target) = action.target.as_ref().and_then(Value::as_object) else {
        return Ok(Some(false));
    };
    if target.len() != 1 {
        return Ok(Some(false));
    }
    let Some(cells) = target.get("selections").and_then(Value::as_array) else {
        return Ok(Some(false));
    };
    if !(minimum..=maximum).contains(&cells.len()) {
        return Ok(Some(false));
    }
    let candidates = target_squares(state, card)?.ok_or(EngineError::IllegalAction)?;
    let mut seen_squares = BTreeSet::new();
    let mut seen_piece_ids = BTreeSet::new();
    for cell in cells {
        let Some(square) = loose_square(cell) else {
            return Ok(Some(false));
        };
        if *cell != json!(square) || !candidates.contains(&square) || !seen_squares.insert(square) {
            return Ok(Some(false));
        }
        if matches!(plan.mutation, Mutation::Cleanup) {
            let piece = state.at(square).ok_or(EngineError::IllegalAction)?;
            if !piece.id.is_empty() && !seen_piece_ids.insert(piece.id.as_str()) {
                return Ok(Some(false));
            }
        }
    }
    validate(state, card, action)
}

/// Source apply acceptance is broader than some UI selection surfaces. Probe
/// the canonical effect on an owned state so validation consumes no live RNG
/// and cannot change the caller's board, collections or pending replay state.
pub(crate) fn validate(
    state: &GameState,
    card: &CardSlot,
    action: &Action,
) -> Result<Option<bool>> {
    if plan(state, card).is_none() {
        return Ok(None);
    }
    validate_pinned_card(state, card)?;
    let mut candidate = state.clone();
    match apply(&mut candidate, card, action) {
        Ok(Some(_)) => Ok(Some(true)),
        Ok(None) => Ok(None),
        Err(EngineError::IllegalAction) => Ok(Some(false)),
        Err(error) => Err(error),
    }
}

/// Opening RULEs use a separate source activation path from hand-card use.
/// Only effects with verified immediate initial-state semantics are admitted.
pub(crate) fn apply_opening_rule_effect(state: &mut GameState, card_id: &str) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 || state.turn != Color::White {
        return Err(EngineError::UnsupportedFeature(
            "opening RULE activation outside v7 white reset state".into(),
        ));
    }
    let card = crate::card_registry::opening_rule_source_card(card_id)?;
    match card["effect"].as_str() {
        Some("saturation") => {
            state.extra.insert("saturationRule".into(), json!(true));
        }
        Some("acceleration") => {
            if state.extra.get("acceleration") == Some(&Value::Bool(true))
                || state
                    .extra
                    .get("accelerationPendingFor")
                    .is_some_and(|value| !value.is_null() && value != "")
            {
                return Ok(());
            }
            let completed = u64::from(state.turns_taken.black);
            let starts_after = completed.checked_add(2).ok_or_else(|| {
                EngineError::InvalidState("acceleration black-turn counter overflow".into())
            })?;
            state.extra.insert(
                "accelerationStartsAfterBlackTurns".into(),
                json!(starts_after),
            );
            state
                .extra
                .insert("accelerationPendingFor".into(), json!("black"));
            state.extra.insert(
                "accelerationPendingTurns".into(),
                json!(starts_after - completed),
            );
        }
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "v7 opening RULE effect {card_id}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn apply(
    state: &mut GameState,
    card: &CardSlot,
    action: &Action,
) -> Result<Option<Vec<Piece>>> {
    let Some(plan) = plan(state, card) else {
        return Ok(None);
    };
    validate_pinned_card(state, card)?;
    validate_profile(state, plan)?;
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
    match plan.mutation {
        Mutation::SideFlag(field, enemy) => {
            if action.target.is_some() {
                return Err(EngineError::IllegalAction);
            }
            let color = if enemy {
                state.turn.opponent()
            } else {
                state.turn
            };
            apply_side_flag(state, field, color)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Coronation => {
            apply_side_flag(state, "coronation", state.turn)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::LastResistance => {
            apply_last_resistance(state)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Twins => {
            apply_twins(state, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Spy => {
            apply_spy(state, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Wanted => {
            apply_wanted(state)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Brainwash => {
            return Ok(Some(apply_brainwash(state, action)?));
        }
        Mutation::Othello => {
            if action.target.is_some() {
                return Err(EngineError::IllegalAction);
            }
            let actor = state.turn;
            crate::turn_effects_v7::activate_othello(state, actor)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Reposition => {
            apply_reposition(state, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Taunt => {
            apply_taunt(state, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Taboo => {
            apply_taboo(state, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Cleanup => {
            apply_cleanup(state, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Hypocrisy => {
            apply_hypocrisy(state, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::PortalGun => {
            apply_portal_gun(state, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Metal => {
            let target = action.target.as_ref().ok_or(EngineError::IllegalAction)?;
            let square = loose_square(target).ok_or(EngineError::IllegalAction)?;
            let mut piece = state
                .at(square)
                .cloned()
                .ok_or(EngineError::IllegalAction)?;
            if !matches_plan(state, &piece, square, plan)? {
                return Err(EngineError::IllegalAction);
            }
            piece.extra.insert("metalized".into(), json!(true));
            piece.extra.insert("metalCooldown".into(), json!(0));
            write_piece(state, &piece);
            return Ok(Some(Vec::new()));
        }
        Mutation::Selection(field) => {
            apply_selection(state, action, field)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Evacuation => {
            apply_evacuation(state, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Windmill => {
            apply_windmill(state, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Chain => {
            apply_chain(state, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::FeudalContract => {
            apply_feudal_contract(state, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Scarecrow => {
            return apply_scarecrow(state, action, plan).map(Some);
        }
        Mutation::Potion => {
            apply_potion(state, card, action)?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Guard | Mutation::Freeze | Mutation::Alekhine | Mutation::Evasion => {
            match plan.mutation {
                Mutation::Guard => apply_guard(state)?,
                Mutation::Freeze => apply_freeze(state)?,
                Mutation::Alekhine => apply_alekhine(state)?,
                Mutation::Evasion => apply_evasion(state)?,
                _ => unreachable!(),
            }
            return Ok(Some(Vec::new()));
        }
        Mutation::QueensGambit => return apply_queens_gambit(state, action).map(Some),
        _ => {}
    }
    let mut captures = Vec::new();
    let selected = match plan.mutation {
        Mutation::RandomThief
        | Mutation::RandomBrutus
        | Mutation::RandomRecurrence
        | Mutation::RandomShield
        | Mutation::PoisonPawns
        | Mutation::GhostPawns => {
            if matches!(
                plan.mutation,
                Mutation::RandomThief | Mutation::RandomBrutus | Mutation::PoisonPawns
            ) {
                current_random_pool(state)?;
            }
            if action.target.is_some() {
                return Err(EngineError::IllegalAction);
            }
            let unique = !matches!(
                plan.mutation,
                Mutation::RandomBrutus
                    | Mutation::RandomShield
                    | Mutation::PoisonPawns
                    | Mutation::GhostPawns
            );
            let mut targets = all_targets(state, plan, unique)?;
            if targets.is_empty() {
                return Err(EngineError::IllegalAction);
            }
            match plan.mutation {
                Mutation::GhostPawns => {
                    for square in targets {
                        let mut piece = state
                            .at(square)
                            .cloned()
                            .ok_or(EngineError::IllegalAction)?;
                        piece.extra.insert("ghost".into(), json!(true));
                        mark_animation(state, &piece)?;
                        write_piece(state, &piece);
                    }
                    return Ok(Some(Vec::new()));
                }
                Mutation::PoisonPawns => {
                    for index in (1..targets.len()).rev() {
                        let swap = (state.rng.sample()? * (index + 1) as f64).floor() as usize;
                        targets.swap(index, swap);
                    }
                    for square in targets.into_iter().take(4) {
                        let mut piece = state
                            .at(square)
                            .cloned()
                            .ok_or(EngineError::IllegalAction)?;
                        piece.extra.insert("poisonedPawn".into(), json!(true));
                        write_piece(state, &piece);
                    }
                    return Ok(Some(Vec::new()));
                }
                _ => {
                    let index = (state.rng.sample()? * targets.len() as f64).floor() as usize;
                    targets[index]
                }
            }
        }
        Mutation::Sacrificial(kind) => {
            let (primary, secondary) = sacrificial_target(state, action, plan, kind)?;
            let captured = if kind == "grappler" {
                crate::transition::expansion_sacrifice(state, secondary, state.turn)?
            } else {
                crate::transition::sacrifice(state, secondary, state.turn.opponent())?
            }
            .ok_or(EngineError::IllegalAction)?;
            captures.push(captured);
            primary
        }
        _ => {
            let square = target_square(action)?;
            let square = if matches!(
                plan.mutation,
                Mutation::Grant("nullification" | "outpostProtected")
            ) {
                normalize_square(state, square)
            } else {
                square
            };
            let piece = state.at(square).ok_or(EngineError::IllegalAction)?;
            if !matches_plan(state, piece, square, plan)? {
                return Err(EngineError::IllegalAction);
            }
            square
        }
    };
    let mut piece = state
        .at(selected)
        .cloned()
        .ok_or(EngineError::IllegalAction)?;
    match plan.mutation {
        Mutation::Bribe => {
            piece.kind = "amazon".into();
            mark_transformed_origin(state, &mut piece, selected)?;
            piece.extra.insert("bribed".into(), json!(true));
            piece.extra.insert("bribedRemaining".into(), json!(3));
            let created = state.move_count;
            state
                .extra
                .get_mut("temporaryQueens")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| {
                    EngineError::InvalidState("temporaryQueens must be an array".into())
                })?
                .push(json!({"id":piece.id,"color":piece.color,"remaining":3,"createdAt":created}));
        }
        Mutation::Desperado => {
            piece
                .extra
                .insert("desperado".into(), json!({"remaining":2}));
            write_piece(state, &piece);
            mark_animation(state, &piece)?;
            if !crate::movement::desperado_has_legal_move(state, &piece, selected)? {
                piece.extra.shift_remove("desperado");
                write_piece(state, &piece);
                return Err(EngineError::IllegalAction);
            }
        }
        Mutation::Judgment => {
            let return_phase = if truthy(state.extra.get("draftDelete"))
                || state.extra.get("gameStyle").and_then(Value::as_str) == Some("grand")
            {
                None
            } else if !truthy(state.extra.get("middleDraftDone")) {
                Some("MIDDLE")
            } else if !truthy(state.extra.get("endDraftDone")) {
                Some("END")
            } else {
                None
            };
            let Some(return_phase) = return_phase else {
                let removed = crate::transition::judgment_remove(state, selected, state.turn)?
                    .ok_or(EngineError::IllegalAction)?;
                return Ok(Some(vec![removed]));
            };
            if !state
                .extra
                .get("judgmentExiles")
                .is_some_and(Value::is_array)
            {
                state.extra.insert("judgmentExiles".into(), json!([]));
            }
            crate::transition::clear_piece(state, &piece.id);
            let suffix = crate::draft::random_suffix(state.rng.sample()?)?;
            let id = format!(
                "judgment-exile-{}-{}",
                crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?,
                suffix.chars().take(6).collect::<String>()
            );
            let entry = json!({"id":id,"piece":piece,"returnPhase":return_phase,"exiledBy":state.turn,"from":selected});
            state
                .extra
                .get_mut("judgmentExiles")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| EngineError::InvalidState("judgment exile array missing".into()))?
                .push(entry);
            mark_vanish_animation(state, &piece, selected)?;
            crate::flow::mark_progress(state);
            crate::replay::add_log(
                state,
                format!(
                    "레드카드: {}{}의 {}이 다음 {return_phase} 드래프트까지 추방되었습니다.",
                    char::from(b'a' + selected.col),
                    8 - selected.row,
                    crate::replay::piece_label(&piece.kind)
                ),
            )?;
            return Ok(Some(Vec::new()));
        }
        Mutation::Missionary => {
            let candidates = missionary_candidates(state, selected)?;
            if candidates.is_empty() {
                return Err(EngineError::IllegalAction);
            }
            let chosen =
                candidates[(state.rng.sample()? * candidates.len() as f64).floor() as usize];
            let mut summoned = crate::opening::spawn(state, state.turn, "missionary")?;
            summoned.moved = true;
            summoned.extra.insert(
                "origin".into(),
                json!(format!(
                    "{}{}",
                    char::from(b'a' + chosen.col),
                    8 - chosen.row
                )),
            );
            mark_transformed_origin(state, &mut summoned, chosen)?;
            state.board[chosen.row as usize][chosen.col as usize] = Some(summoned.clone());
            mark_animation(state, &summoned)?;
            piece.kind = "missionary".into();
            mark_transformed_origin(state, &mut piece, selected)?;
            piece.moved = true;
        }
        Mutation::BabyBear => {
            if state
                .extra
                .get("campaign")
                .is_some_and(|campaign| !campaign.is_null())
            {
                return Err(EngineError::UnsupportedFeature(
                    "campaign sacrifice objectives".into(),
                ));
            }
            mark_vanish_animation(state, &piece, selected)?;
            captures.push(
                crate::transition::sacrifice(state, selected, state.turn.opponent())?
                    .ok_or(EngineError::IllegalAction)?,
            );
            piece = crate::opening::spawn(state, state.turn, "babyBear")?;
            piece.extra.insert(
                "origin".into(),
                json!(format!(
                    "{}{}",
                    char::from(b'a' + selected.col),
                    8 - selected.row
                )),
            );
            piece.moved = true;
            let turn =
                (*state.turns_taken.get(Color::White)).min(*state.turns_taken.get(Color::Black));
            piece.extra.insert(
                "babyBearGrowAtTurn".into(),
                json!(
                    turn.checked_add(7)
                        .ok_or_else(|| EngineError::InvalidState(
                            "baby bear growth deadline overflow".into()
                        ))?
                ),
            );
            state.board[selected.row as usize][selected.col as usize] = Some(piece.clone());
            mark_animation(state, &piece)?;
            crate::flow::mark_progress(state);
        }
        Mutation::Necromancy => {
            let types = necromancy_types(state);
            if types.is_empty() {
                return Err(EngineError::IllegalAction);
            }
            let revived =
                types[(state.rng.sample()? * types.len() as f64).floor() as usize].clone();
            piece.kind = revived.clone();
            piece.extra.shift_remove("vipInvitation");
            piece.extra.shift_remove("holdoutPromotion");
            piece.moved = true;
            piece.extra.insert("necromancy".into(),json!({"originalType":"pawn","revivedType":revived,"remaining":4,"createdAt":state.move_count}));
            piece.extra.insert("necromancyRemaining".into(), json!(4));
            if !state.extra.get("necromancy").is_some_and(Value::is_array) {
                state.extra.insert("necromancy".into(), json!([]));
            }
            let created = state.move_count;
            let entries = state
                .extra
                .get_mut("necromancy")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| EngineError::InvalidState("necromancy array missing".into()))?;
            entries.retain(|entry| entry.get("id").and_then(Value::as_str) != Some(&piece.id));
            entries.push(json!({"id":piece.id,"color":piece.color,"revivedType":revived,"remaining":4,"createdAt":created}));
            capture_lock(state, &mut piece)?;
            crate::transition::mark_card_no_capture(state, &mut piece)?;
            mark_animation(state, &piece)?;
            write_piece(state, &piece);
            crate::replay::add_piece_action_log(
                state,
                &piece,
                Some(selected),
                None,
                format!(
                    "빙의: {}{}의 폰이 4수 동안 {}(으)로 되살아났습니다.",
                    char::from(b'a' + selected.col),
                    8 - selected.row,
                    crate::replay::piece_label(&revived)
                ),
            )?;
        }
        Mutation::Exile => {
            let origin = exile_origin(&piece, selected)?;
            if state.at(origin).is_some() {
                return Err(EngineError::IllegalAction);
            }
            if crate::movement::collapsed(state, origin) {
                return Err(EngineError::UnsupportedFeature(
                    "exile collapse removal and undead resurrection".into(),
                ));
            }
            state.board[selected.row as usize][selected.col as usize] = None;
            piece.moved = true;
            state.board[origin.row as usize][origin.col as usize] = Some(piece.clone());
        }
        Mutation::Transform(_) | Mutation::GenericTransform(_) | Mutation::InternalTransform(_) => {
            transform(state, &mut piece, selected, plan.mutation)?
        }
        Mutation::Grant(field) => {
            if field == "chargeRush" {
                for other in state
                    .board
                    .iter_mut()
                    .flatten()
                    .flatten()
                    .filter(|piece| piece.color == state.turn)
                {
                    other.extra.shift_remove("chargeRush");
                }
            }
            grant(state, &mut piece, field)?;
        }
        Mutation::RandomShield => grant(state, &mut piece, "shielded")?,
        Mutation::Submerge => {
            piece.extra.insert("submerged".into(), json!(true));
        }
        Mutation::RandomThief => {
            transform(
                state,
                &mut piece,
                selected,
                Mutation::GenericTransform("thief"),
            )?;
            piece.extra.insert("submerged".into(), json!(true));
            piece
                .extra
                .insert("wanted".into(), json!({"by":state.turn}));
        }
        Mutation::RandomBrutus => transform(
            state,
            &mut piece,
            selected,
            Mutation::InternalTransform("brutus"),
        )?,
        Mutation::RandomRecurrence => {
            piece.extra.insert("recurrence".into(), json!(true));
            mark_animation(state, &piece)?;
        }
        Mutation::Sacrificial(kind) => transform(
            state,
            &mut piece,
            selected,
            if kind == "grappler" {
                Mutation::GenericTransform(kind)
            } else {
                Mutation::Transform(kind)
            },
        )?,
        _ => return Err(EngineError::IllegalAction),
    }
    write_piece(state, &piece);
    if matches!(plan.mutation, Mutation::Submerge) {
        crate::transition::refresh_submerged(state)?;
    }
    if matches!(plan.mutation, Mutation::Transform("herald")) {
        crate::transition::resolve_herald_for_color(state, state.turn)?;
    }
    Ok(Some(captures))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> GameState {
        let mut state = GameState::new(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            11,
        )
        .unwrap();
        state.board = vec![vec![None; 8]; 8];
        state
    }
    fn put(state: &mut GameState, kind: &str, color: Color, square: Square) -> String {
        let id = format!("{}-{kind}-{}-{}", color.as_str(), square.row, square.col);
        state.board[square.row as usize][square.col as usize] =
            Some(Piece::new(kind, color, id.clone()));
        id
    }
    fn card(id: &str) -> CardSlot {
        let definition = crate::draft::definitions()
            .definitions
            .iter()
            .find(|card| card["id"] == id)
            .unwrap()
            .clone();
        let mut card: CardSlot = serde_json::from_value(definition).unwrap();
        card.instance_id = format!("test-{id}");
        card
    }
    fn action(card: &CardSlot, square: Square) -> Action {
        Action::card(Color::White, card, Some(json!(square)))
    }

    #[test]
    fn staged_cursor_preserves_ordered_click_sequences() {
        let squares = [
            Square { row: 1, col: 1 },
            Square { row: 2, col: 2 },
            Square { row: 3, col: 3 },
            Square { row: 4, col: 4 },
        ];
        let sequences = |mut cursor: OrderedSelectionCursor| {
            let mut result = Vec::new();
            while let Some(action) = cursor.next_candidate() {
                result.push(
                    serde_json::from_value::<Vec<Square>>(
                        action.target.unwrap()["selections"].clone(),
                    )
                    .unwrap(),
                );
            }
            result
        };
        let cleanup = sequences(OrderedSelectionCursor::new(
            Color::White,
            &card("cleanup"),
            squares[..3].to_vec(),
            1,
            3,
        ));
        assert_eq!(cleanup.len(), 15); // P(3,1) + P(3,2) + P(3,3)
        assert_eq!(cleanup.first().unwrap(), &vec![squares[0]]);
        assert!(cleanup.contains(&vec![squares[1], squares[0]]));
        assert_eq!(
            cleanup.last().unwrap(),
            &vec![squares[2], squares[1], squares[0]]
        );

        let portal = sequences(OrderedSelectionCursor::new(
            Color::White,
            &card("portal-gun"),
            squares.to_vec(),
            2,
            2,
        ));
        assert_eq!(portal.len(), 12); // P(4,2)
        assert_eq!(portal.first().unwrap(), &vec![squares[0], squares[1]]);
        assert_eq!(portal.last().unwrap(), &vec![squares[3], squares[2]]);

        let hypocrisy = sequences(OrderedSelectionCursor::new(
            Color::White,
            &card("hypocrisy"),
            squares.to_vec(),
            4,
            4,
        ));
        assert_eq!(hypocrisy.len(), 24); // P(4,4)
        assert_eq!(hypocrisy.first().unwrap(), &squares.to_vec());
        assert_eq!(
            hypocrisy.last().unwrap(),
            &squares.iter().rev().copied().collect::<Vec<_>>()
        );
        assert_eq!(hypocrisy.iter().collect::<BTreeSet<_>>().len(), 24);

        // A full empty-board Hypocrisy family has P(64,4) = 15,249,024
        // ordered candidates. Advancing one bounded page must not construct
        // or consume the entire family.
        let board = (0..8)
            .flat_map(|row| (0..8).map(move |col| Square { row, col }))
            .collect::<Vec<_>>();
        let mut large = OrderedSelectionCursor::new(Color::White, &card("hypocrisy"), board, 4, 4);
        for _ in 0..4096 {
            assert!(large.next_candidate().is_some());
        }
        assert!(!large.is_exhausted());
    }

    #[test]
    fn portal_staged_page_keeps_slot_identity_order_and_examination_budget() {
        let mut state = empty();
        state.mode = "play".into();
        put(&mut state, "king", Color::White, Square { row: 7, col: 7 });
        put(&mut state, "king", Color::Black, Square { row: 0, col: 7 });
        let mut first = card("portal-gun");
        first.instance_id = "portal-staged-0".into();
        let mut second = first.clone();
        second.instance_id = "portal-staged-1".into();
        state.deck_slots.white = vec![first.clone(), second.clone()];
        let before = state.clone();

        assert!(matches!(
            actions(&state, &first),
            Err(EngineError::UnsupportedFeature(_))
        ));
        let mut cursor = staged_cursor_for_slot(&state, 0).unwrap().unwrap();
        assert_eq!(cursor.squares.len(), 62);
        let one = cursor.next_public_page(&state, 0, 2, 1).unwrap();
        assert_eq!(one.examined, 1);
        assert!(!one.exhausted);
        assert_eq!(one.actions.len(), 1);
        assert_eq!(
            one.actions[0].target,
            Some(json!({"selections":[{"row":0,"col":0},{"row":0,"col":1}]}))
        );
        assert_eq!(
            one.actions[0].card_instance_id.as_deref(),
            Some(first.instance_id.as_str())
        );

        let next = cursor.next_public_page(&state, 0, 2, 2).unwrap();
        assert_eq!(next.examined, 2);
        assert_eq!(next.actions.len(), 2);
        assert!(!next.exhausted);
        assert_eq!(
            next.actions
                .iter()
                .map(|action| action.target.as_ref().unwrap()["selections"].clone())
                .collect::<Vec<_>>(),
            vec![
                json!([{"row":0,"col":0},{"row":0,"col":2}]),
                json!([{"row":0,"col":0},{"row":0,"col":3}]),
            ]
        );
        assert!(matches!(
            cursor.next_public_page(&state, 1, 1, 1),
            Err(EngineError::IllegalAction)
        ));
        let resumed = cursor.next_public_page(&state, 0, 1, 1).unwrap();
        assert_eq!(
            resumed.actions[0].target,
            Some(json!({"selections":[{"row":0,"col":0},{"row":0,"col":4}]}))
        );

        let mut second_cursor = staged_cursor_for_slot(&state, 1).unwrap().unwrap();
        let second_page = second_cursor.next_public_page(&state, 1, 1, 1).unwrap();
        assert_eq!(second_page.actions[0].target, one.actions[0].target);
        assert_eq!(
            second_page.actions[0].card_instance_id.as_deref(),
            Some(second.instance_id.as_str())
        );
        assert_eq!(state, before);

        // A candidate rejected by the public click predicate still consumes
        // one examination and leaves the family available for the next page.
        let mut rejected = OrderedSelectionCursor::new(
            Color::White,
            &first,
            vec![
                Square { row: 0, col: 7 },
                Square { row: 0, col: 0 },
                Square { row: 0, col: 1 },
            ],
            2,
            2,
        );
        let empty_page = rejected.next_public_page(&state, 0, 1, 1).unwrap();
        assert!(empty_page.actions.is_empty());
        assert_eq!(empty_page.examined, 1);
        assert!(!empty_page.exhausted);
        let next_rejected = rejected.next_public_page(&state, 0, 1, 1).unwrap();
        assert!(next_rejected.actions.is_empty());
        assert_eq!(next_rejected.examined, 1);
        assert!(!next_rejected.exhausted);

        state.deck_slots.white[0].used = true;
        assert!(matches!(
            staged_cursor_for_slot(&state, 0),
            Err(EngineError::IllegalAction)
        ));
        assert!(matches!(
            cursor.next_public_page(&state, 0, 1, 1),
            Err(EngineError::IllegalAction)
        ));
        state.deck_slots.white[0].used = false;
        state.deck_slots.white[0].recovering = true;
        assert!(matches!(
            staged_cursor_for_slot(&state, 0),
            Err(EngineError::IllegalAction)
        ));
        state.deck_slots.white[0].recovering = false;
        state.deck_slots.white[0]
            .extra
            .insert("nextTurnPending".into(), Value::Bool(true));
        assert!(matches!(
            staged_cursor_for_slot(&state, 0),
            Err(EngineError::IllegalAction)
        ));
    }

    #[test]
    fn pending_portals_choose_last_due_and_cancel_only_on_installation_hazard() {
        let mut state = empty();
        state.turns_taken.white = 1;
        let occupied = Square { row: 5, col: 1 };
        put(&mut state, "rook", Color::White, occupied);
        let rng_before = state.rng.clone();
        let history_before = state.history.clone();
        state.extra.insert("logs".into(), json!([]));
        state.extra.insert(
            "pendingPortals".into(),
            json!([
                {"id":"discard","color":"white","cells":[{"row":8,"col":1}],"triggerTurn":0},
                {"id":"first","color":"white","cells":[{"row":1,"col":1},{"row":2,"col":2}],"triggerTurn":1},
                {"id":"future","color":"black","cells":[{"row":3,"col":3},{"row":4,"col":4}],"triggerTurn":1},
                {"id":"last","color":"white","cells":[{"row":5,"col":1},{"row":5,"col":2}],"triggerTurn":1}
            ]),
        );
        assert!(resolve_pending_portals_for_turn(&mut state, Color::White).unwrap());
        assert_eq!(
            state.extra.get("pendingPortals"),
            Some(
                &json!([{"id":"future","color":"black","cells":[{"row":3,"col":3},{"row":4,"col":4}],"triggerTurn":1}])
            )
        );
        assert_eq!(
            state.extra.get("portalRule"),
            Some(&json!({"enabled":true,"cells":[{"row":5,"col":1},{"row":5,"col":2}]}))
        );
        assert_eq!(state.at(occupied).unwrap().kind, "rook");
        assert_eq!(
            state.extra.get("logs").and_then(Value::as_array).unwrap()[0],
            json!("포탈 건: b3·c3에 포탈이 설치되었습니다.")
        );
        state
            .extra
            .insert("collapsedCells".into(), json!([{"row":3,"col":3}]));
        state.turns_taken.black = 1;
        assert!(!resolve_pending_portals_for_turn(&mut state, Color::Black).unwrap());
        assert_eq!(state.extra.get("pendingPortals"), Some(&json!([])));
        assert_eq!(
            state.extra.get("portalRule"),
            Some(&json!({"enabled":true,"cells":[{"row":5,"col":1},{"row":5,"col":2}]}))
        );
        assert_eq!(
            state.extra.get("logs").and_then(Value::as_array).unwrap()[0],
            json!("포탈 건: 설치 예정 칸이 막혀 포탈 설치가 취소되었습니다.")
        );
        assert_eq!(state.rng, rng_before);
        assert_eq!(state.history, history_before);
    }

    #[test]
    fn ordered_public_selection_is_direct_and_does_not_change_raw_acceptance() {
        let a = Square { row: 3, col: 2 };
        let b = Square { row: 4, col: 5 };
        let mut state = empty();
        put(&mut state, "pawn", Color::White, a);
        put(&mut state, "pawn", Color::White, b);
        let cleanup = card("cleanup");
        let reversed = Action::card(Color::White, &cleanup, Some(json!({"selections":[b,a]})));
        let duplicate = Action::card(Color::White, &cleanup, Some(json!({"selections":[a,a]})));
        let before = state.clone();
        assert!(matches!(
            actions(&state, &cleanup),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert_eq!(
            ui_validate(&state, &cleanup, &reversed).unwrap(),
            Some(true)
        );
        assert_eq!(validate(&state, &cleanup, &duplicate).unwrap(), Some(true));
        assert_eq!(
            ui_validate(&state, &cleanup, &duplicate).unwrap(),
            Some(false)
        );
        assert_eq!(state, before);

        let mut cursor = staged_cursor(&state, &cleanup).unwrap().unwrap();
        let mut emitted = Vec::new();
        while let Some(candidate) = cursor.next_candidate() {
            assert_eq!(
                ui_validate(&state, &cleanup, &candidate).unwrap(),
                Some(true)
            );
            emitted.push(candidate.target.unwrap()["selections"].clone());
        }
        assert_eq!(
            emitted,
            vec![json!([a]), json!([b]), json!([a, b]), json!([b, a])]
        );

        // The frozen source accepts all 15 ordered applications for these
        // three coordinates; the first-click surface is row-major.
        let ordered = [
            Square { row: 2, col: 2 },
            Square { row: 3, col: 4 },
            Square { row: 4, col: 3 },
        ];
        let mut three = empty();
        for (square, kind) in ordered.into_iter().zip(["rook", "bishop", "knight"]) {
            put(&mut three, kind, Color::White, square);
        }
        let mut cursor = staged_cursor(&three, &cleanup).unwrap().unwrap();
        let mut selected = Vec::new();
        while let Some(candidate) = cursor.next_candidate() {
            assert_eq!(
                ui_validate(&three, &cleanup, &candidate).unwrap(),
                Some(true)
            );
            selected.push(candidate.target.unwrap()["selections"].clone());
        }
        assert_eq!(selected.len(), 15);
        assert_eq!(selected.first().unwrap(), &json!([ordered[0]]));
        assert_eq!(
            selected.last().unwrap(),
            &json!([ordered[2], ordered[1], ordered[0]])
        );

        let portal = card("portal-gun");
        let portal_action = Action::card(Color::White, &portal, Some(json!({"selections":[b,a]})));
        // The occupied squares above are not portal candidates, regardless of
        // the raw target's syntactic shape.
        assert_eq!(
            ui_validate(&state, &portal, &portal_action).unwrap(),
            Some(false)
        );

        let vacant = empty();
        assert!(matches!(
            actions(&vacant, &portal),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert_eq!(
            ui_validate(&vacant, &portal, &portal_action).unwrap(),
            Some(true)
        );
        let repeated = Action::card(Color::White, &portal, Some(json!({"selections":[a,a]})));
        assert_eq!(
            ui_validate(&vacant, &portal, &repeated).unwrap(),
            Some(false)
        );
        let hypocrisy = card("hypocrisy");
        let four = Action::card(
            Color::White,
            &hypocrisy,
            Some(json!({"selections":[
                a,b,Square {row:1,col:2},Square {row:6,col:5}
            ]})),
        );
        assert_eq!(ui_validate(&vacant, &hypocrisy, &four).unwrap(), Some(true));
        assert_eq!(vacant, empty());
    }

    #[test]
    fn transforms_preserve_identity_and_obey_the_source_cleanup_boundaries() {
        let square = Square { row: 4, col: 2 };
        for (id, from, to) in [
            ("assassin", "knight", "assassin"),
            ("wizard", "queen", "wizard"),
            ("knightmaster", "knight", "knightmaster"),
            ("standard-bearer", "pawn", "standardBearer"),
            ("pegasus", "rook", "pegasus"),
            ("grasshopper", "wizard", "grasshopper"),
            ("dragon", "rook", "dragon"),
            ("princess", "rook", "princess"),
            ("campfire", "wizard", "campfire"),
            ("hedgehog", "queen", "hedgehog"),
            ("revolving-door", "rook", "revolvingDoor"),
            ("don-quixote", "rook", "donQuixote"),
            ("medium", "wizard", "medium"),
            ("clockwork", "wizard", "clockwork"),
            ("parrot", "rook", "parrot"),
            ("paladin", "knight", "paladin"),
            ("octopus", "rook", "octopus"),
        ] {
            let mut state = empty();
            state.turns_taken.white = 3;
            let id_before = put(&mut state, from, Color::White, square);
            let piece = state.at_mut(square).unwrap();
            piece.extra.insert("origin".into(), json!("a8"));
            piece.extra.insert("mana".into(), json!(4));
            piece.extra.insert("maxMana".into(), json!(5));
            piece.extra.insert("explosive".into(), json!(true));
            let card = card(id);
            let candidate = action(&card, square);
            assert!(
                actions(&state, &card)
                    .unwrap()
                    .unwrap()
                    .contains(&candidate)
            );
            let before_rng = state.rng.clone();
            assert_eq!(
                apply(&mut state, &card, &candidate).unwrap(),
                Some(Vec::new())
            );
            let piece = state.at(square).unwrap();
            assert_eq!(piece.id, id_before);
            assert_eq!(piece.kind, to);
            assert!(piece.moved);
            assert_eq!(piece.extra["freshNoCaptureUntil"], 4);
            assert_eq!(piece.extra["explosive"], true);
            let internal = ["clockwork", "parrot", "paladin", "octopus"].contains(&id);
            assert_eq!(piece.extra["origin"], if internal { "a8" } else { "c4" });
            if id == "grasshopper" {
                assert_eq!(piece.extra["mana"], 4);
            }
            if ["campfire", "medium", "clockwork"].contains(&id) {
                assert!(!piece.extra.contains_key("mana"));
            }
            if id == "wizard" {
                assert_eq!(piece.extra["mana"], 0);
                assert_eq!(piece.extra["maxMana"], 5);
            }
            if id == "hedgehog" {
                assert_eq!(piece.extra["bearRetaliationsRemaining"], 2);
            }
            assert_eq!(state.rng, before_rng);
            let mut malformed = candidate.clone();
            malformed.target.as_mut().unwrap()["extra"] = json!(true);
            assert_eq!(
                apply(&mut state, &card, &malformed).unwrap_err(),
                EngineError::IllegalAction
            );
        }
        let mut state = empty();
        let id = put(&mut state, "queen", Color::White, square);
        state.extra.insert(
            "feudalContracts".into(),
            json!([{"id":"c1","pawnId":id},{"id":"keep","pawnId":"other"}]),
        );
        let piece = state.at_mut(square).unwrap();
        for field in [
            "explosive",
            "chimera",
            "potionBasicTraining",
            "basicTraining",
            "trojanHorse",
            "vipInvitation",
        ] {
            piece.extra.insert(field.into(), json!(true));
        }
        piece.extra.insert("feudalContractId".into(), json!("c1"));
        let card = card("reaper");
        apply(&mut state, &card, &action(&card, square)).unwrap();
        let piece = state.at(square).unwrap();
        assert_eq!(piece.extra["reaperCaptures"], 0);
        for field in [
            "explosive",
            "chimera",
            "basicTraining",
            "trojanHorse",
            "vipInvitation",
            "feudalContractId",
        ] {
            assert!(!piece.extra.contains_key(field));
        }
        assert_eq!(state.extra["feudalContracts"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn random_transforms_use_one_source_draw_and_preserve_defaults() {
        let a = Square { row: 3, col: 0 };
        let b = Square { row: 3, col: 7 };
        let mut state = empty();
        put(&mut state, "queen", Color::White, a);
        put(&mut state, "queen", Color::White, b);
        state.rng = RngState {
            tape: vec![0.75],
            ..RngState::seeded(11)
        };
        let thief_card = card("thief");
        let candidate = Action::card(Color::White, &thief_card, None);
        let before_probe = state.clone();
        assert_eq!(
            validate(&state, &thief_card, &candidate).unwrap(),
            Some(true)
        );
        assert_eq!(state, before_probe);
        let mut malformed = candidate.clone();
        malformed.target = Some(json!(a));
        assert_eq!(
            validate(&state, &thief_card, &malformed).unwrap(),
            Some(false)
        );
        assert_eq!(state, before_probe);
        apply(&mut state, &thief_card, &candidate).unwrap();
        assert_eq!(state.at(a).unwrap().kind, "queen");
        let thief = state.at(b).unwrap();
        assert_eq!(thief.kind, "thief");
        assert_eq!(thief.extra["submerged"], true);
        assert_eq!(thief.extra["wanted"], json!({"by":"white"}));
        assert_eq!(state.rng.cursor, 1);
        for (ability, field, value) in [
            ("wizard", "mana", json!(0)),
            ("windmill", "windmillMode", json!("bishop")),
            ("hedgehog", "bearRetaliationsRemaining", json!(2)),
            ("reaper", "reaperCaptures", json!(0)),
        ] {
            let mut state = empty();
            put(&mut state, "rook", Color::White, a);
            let index = TRICKSTER_TYPES
                .iter()
                .position(|kind| *kind == ability)
                .unwrap();
            state.rng = RngState {
                tape: vec![(index as f64 + 0.25) / TRICKSTER_TYPES.len() as f64],
                ..RngState::seeded(0)
            };
            let card = card("trickster");
            apply(&mut state, &card, &action(&card, a)).unwrap();
            let piece = state.at(a).unwrap();
            assert_eq!(piece.extra["tricksterMoveType"], ability);
            assert_eq!(piece.extra[field], value);
            assert_eq!(state.rng.cursor, 1);
        }
        assert!(number_is_finite(Some(&json!(""))));
        assert!(number_is_finite(Some(&json!([null]))));
        assert!(number_is_finite(Some(&json!("0x20"))));
        assert!(!number_is_finite(Some(&json!([1, 2]))));
        assert!(!number_is_finite(None));
    }

    #[test]
    fn trickster_thief_defaults_follow_the_source_ruleset() {
        // Direct v6/v7 applyTricksterAbilityDefaults probes in the pinned
        // clients differ only in wanted: v6 leaves it absent, v7 writes the
        // Trickster's own color while both mark it submerged.
        let thief_index = TRICKSTER_TYPES
            .iter()
            .position(|kind| *kind == "thief")
            .unwrap();
        for (rules_version, expected_wanted) in [
            (RULES_VERSION_V6, Value::Null),
            (RULES_VERSION_V7, json!({"by":"white"})),
        ] {
            let mut state = empty();
            state.ruleset_id = rules_version.into();
            state.rng = RngState {
                tape: vec![(thief_index as f64 + 0.25) / TRICKSTER_TYPES.len() as f64],
                ..RngState::seeded(0)
            };
            let mut piece = Piece::new("trickster", Color::White, "test-trickster");
            trickster_defaults(&mut state, &mut piece).unwrap();
            assert_eq!(piece.extra["tricksterMoveType"], "thief");
            assert_eq!(piece.extra["submerged"], true);
            assert_eq!(
                piece.extra.get("wanted").unwrap_or(&Value::Null),
                &expected_wanted
            );
            assert_eq!(state.rng.cursor, 1);

            if rules_version == RULES_VERSION_V7 {
                piece.extra.insert("wanted".into(), json!({"by":"black"}));
                trickster_defaults(&mut state, &mut piece).unwrap();
                assert_eq!(piece.extra["wanted"], json!({"by":"black"}));
            }
        }
    }

    #[test]
    fn selected_status_effects_preserve_raw_order_and_probe_owned_state() {
        let a = Square { row: 3, col: 2 };
        let b = Square { row: 4, col: 5 };
        let mut state = empty();
        let a_id = put(&mut state, "bishop", Color::Black, a);
        let b_id = put(&mut state, "rook", Color::Black, b);
        let panic = card("panic");
        let candidate = Action::card(Color::White, &panic, Some(json!({"selections":[b,a,b]})));
        let before = state.clone();
        assert_eq!(validate(&state, &panic, &candidate).unwrap(), Some(true));
        assert_eq!(state, before);
        apply(&mut state, &panic, &candidate).unwrap();
        assert_eq!(
            state.extra["pendingPanic"][0]["pieces"],
            json!([
                {"id":b_id,"row":b.row,"col":b.col},{"id":a_id,"row":a.row,"col":a.col}
            ])
        );
        assert!(actions(&state, &panic).unwrap().unwrap().is_empty());
        let before = state.clone();
        assert_eq!(validate(&state, &panic, &candidate).unwrap(), Some(false));
        assert_eq!(state, before);

        let mut state = empty();
        put(&mut state, "pawn", Color::White, a);
        put(&mut state, "pawn", Color::White, b);
        state
            .at_mut(a)
            .unwrap()
            .extra
            .insert("chargeRush".into(), json!(true));
        state
            .at_mut(b)
            .unwrap()
            .extra
            .insert("staked".into(), json!({"remaining":"0"}));
        let charge = card("charge");
        apply(&mut state, &charge, &action(&charge, b)).unwrap();
        assert!(!state.at(a).unwrap().extra.contains_key("chargeRush"));
        assert_eq!(state.at(b).unwrap().extra["chargeRush"], true);
        state.at_mut(b).unwrap().extra["staked"]["remaining"] = json!("0x2");
        let before = state.clone();
        assert_eq!(
            validate(&state, &charge, &action(&charge, b)).unwrap(),
            Some(false)
        );
        assert_eq!(state, before);

        // Sequential retreat can put the second piece in the first origin.
        // Source lastMove uses that identity; parrot memory uses the first to.
        let mut state = empty();
        let from = Square { row: 4, col: 3 };
        let following = Square { row: 3, col: 3 };
        put(&mut state, "bishop", Color::White, from);
        let following_id = put(&mut state, "knight", Color::White, following);
        state
            .extra
            .insert("parrotMovement".into(), json!({"white":null,"black":null}));
        let evacuation = card("emergency-evacuation");
        let action = Action::card(
            Color::White,
            &evacuation,
            Some(json!({"selections":[from,following]})),
        );
        let before = state.clone();
        assert_eq!(validate(&state, &evacuation, &action).unwrap(), Some(true));
        assert_eq!(state, before);
        apply(&mut state, &evacuation, &action).unwrap();
        assert_eq!(state.extra["lastMove"]["pieceId"], following_id);
        assert_eq!(state.extra["parrotMovement"]["white"]["type"], "bishop");
    }

    #[test]
    fn grants_use_piece_owner_and_shared_turn_clocks_and_bound_random_work() {
        let square = Square { row: 2, col: 1 };
        let mut state = empty();
        state.turns_taken.white = 8;
        state.turns_taken.black = 5;
        put(&mut state, "pawn", Color::Black, square);
        let vip = card("vip");
        apply(&mut state, &vip, &action(&vip, square)).unwrap();
        assert_eq!(
            state.at(square).unwrap().extra["vipInvitation"],
            json!({"by":"white","triggerTurn":8})
        );
        put(&mut state, "pawn", Color::White, square);
        let holdout = card("holdout");
        apply(&mut state, &holdout, &action(&holdout, square)).unwrap();
        assert_eq!(
            state.at(square).unwrap().extra["holdoutPromotion"],
            json!({"by":"white","readyTurn":19})
        );
        assert!(actions(&state, &holdout).unwrap().unwrap().is_empty());
        let mut state = empty();
        for col in 0..8 {
            put(&mut state, "pawn", Color::White, Square { row: 6, col });
        }
        state.rng = RngState {
            tape: vec![0.5; 7],
            ..RngState::seeded(0)
        };
        let poison = card("poisoned-pawn");
        apply(
            &mut state,
            &poison,
            &Action::card(Color::White, &poison, None),
        )
        .unwrap();
        assert_eq!(
            state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|p| p.flag("poisonedPawn"))
                .count(),
            4
        );
        assert_eq!(state.rng.cursor, 7);
        // The UI checks the clicked cell, while outpost's effect normalizes
        // first. A large piece retains one logical set of properties in Rust.
        let mut large_state = empty();
        let anchor = Square { row: 3, col: 3 };
        put(&mut large_state, "bigRook", Color::White, anchor);
        let piece = large_state.at_mut(anchor).unwrap();
        piece.extra.insert("anchorRow".into(), json!(3));
        piece.extra.insert("anchorCol".into(), json!(3));
        let piece = piece.clone();
        for row in 3..=4 {
            for col in 3..=4 {
                large_state.board[row][col] = Some(piece.clone());
            }
        }
        let outpost_card = card("outpost");
        let targets = actions(&large_state, &outpost_card).unwrap().unwrap();
        assert_eq!(targets.len(), 2);
        let clicked = action(&outpost_card, Square { row: 4, col: 4 });
        assert!(!targets.contains(&clicked));
        let before_probe = large_state.clone();
        assert_eq!(
            validate(&large_state, &outpost_card, &clicked).unwrap(),
            Some(true)
        );
        assert_eq!(large_state, before_probe);
        apply(&mut large_state, &outpost_card, &clicked).unwrap();
        assert_eq!(
            large_state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|p| p.flag("outpostProtected"))
                .count(),
            4
        );
        let ghost = card("ghost");
        apply(
            &mut state,
            &ghost,
            &Action::card(Color::White, &ghost, None),
        )
        .unwrap();
        assert_eq!(
            state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|p| p.flag("ghost"))
                .count(),
            8
        );
        assert_eq!(state.rng.cursor, 7);
    }

    #[test]
    fn sacrificial_transforms_choose_the_source_pair_and_bypass_capture_protection() {
        let queen = Square { row: 4, col: 3 };
        let knight = Square { row: 5, col: 1 };
        for (id, from, secondary, kind, field) in [
            ("grappler", "queen", "bishop", "grappler", "minor"),
            ("amazon", "queen", "knight", "amazon", "knight"),
            ("hook", "queen", "rook", "hook", "rook"),
            ("ordination", "bishop", "bishop", "cardinal", ""),
        ] {
            let mut state = empty();
            let transformed_id = put(&mut state, from, Color::White, queen);
            let sacrificed_id = put(&mut state, secondary, Color::White, knight);
            state
                .at_mut(knight)
                .unwrap()
                .extra
                .insert("protected".into(), json!(true));
            state
                .at_mut(knight)
                .unwrap()
                .extra
                .insert("shielded".into(), json!(true));
            if id == "grappler" {
                state.extra.insert(
                    "forceAnimatedPieceIds".into(),
                    json!({"__simType":"Set","values":["keep",sacrificed_id]}),
                );
            }
            let card = card(id);
            let mut target = json!(queen);
            if !field.is_empty() {
                target[field] = json!(knight);
            }
            let candidate = Action::card(Color::White, &card, Some(target));
            assert!(
                actions(&state, &card)
                    .unwrap()
                    .unwrap()
                    .contains(&candidate)
            );
            let captures = apply(&mut state, &card, &candidate).unwrap().unwrap();
            assert_eq!(captures.len(), 1);
            assert_eq!(captures[0].id, sacrificed_id);
            assert!(state.at(knight).is_none());
            assert_eq!(state.at(queen).unwrap().kind, kind);
            let owner = if id == "grappler" {
                Color::White
            } else {
                Color::Black
            };
            assert_eq!(state.captures.get(owner).last().unwrap().id, sacrificed_id);
            if id == "grappler" {
                assert_eq!(
                    state.extra["forceAnimatedPieceIds"]["values"],
                    json!(["keep", transformed_id])
                );
                assert_eq!(
                    state.extra["animatedPieceIds"]["values"],
                    json!([sacrificed_id])
                );
                let removed = captures[0].clone();
                mark_vanish_animation(&mut state, &removed, knight).unwrap();
                assert_eq!(
                    state.extra["animatedPieceIds"]["values"],
                    json!([sacrificed_id])
                );
            }
        }
        let mut state = empty();
        let sacrificed_id = put(&mut state, "queen", Color::White, queen);
        let original_pawn = Square {
            row: 6,
            col: queen.col,
        };
        let other_pawn = Square { row: 6, col: 5 };
        let protected_pawn = Square { row: 6, col: 7 };
        for square in [original_pawn, other_pawn, protected_pawn] {
            put(&mut state, "pawn", Color::White, square);
        }
        state.rng = RngState {
            tape: vec![0.75],
            ..RngState::seeded(0)
        };
        let gambit = card("queens-gambit");
        state
            .at_mut(queen)
            .unwrap()
            .extra
            .insert("regencyHeir".into(), json!(true));
        let before = state.clone();
        assert_eq!(
            validate(&state, &gambit, &action(&gambit, queen)).unwrap(),
            Some(false)
        );
        assert_eq!(state, before);
        state
            .at_mut(queen)
            .unwrap()
            .extra
            .shift_remove("regencyHeir");
        let captures = apply(&mut state, &gambit, &action(&gambit, queen))
            .unwrap()
            .unwrap();
        assert_eq!(captures[0].id, sacrificed_id);
        assert_eq!(
            state.extra["queensGambitFiles"]["white"],
            json!({"queenCol":3,"randomCol":7})
        );
        assert_eq!(state.rng.cursor, 1);
        assert!(state.at(queen).is_none());
        assert!(
            state
                .at(original_pawn)
                .unwrap()
                .flag("queensGambitProtection")
        );
        assert!(
            state
                .at(protected_pawn)
                .unwrap()
                .flag("queensGambitProtection")
        );
        assert!(!state.at(other_pawn).unwrap().flag("queensGambitProtection"));
    }
}
