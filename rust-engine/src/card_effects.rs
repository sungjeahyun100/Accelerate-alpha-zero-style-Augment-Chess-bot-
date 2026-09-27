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
        _ => match card.effect.as_str() {
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
fn ranged_piece(state: &GameState, piece: &Piece) -> bool {
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
    Ok(Some(ui_targets(state, plan, false)?))
}

pub(crate) fn actions(state: &GameState, card: &CardSlot) -> Result<Option<Vec<Action>>> {
    let Some(plan) = plan(state, card) else {
        return Ok(None);
    };
    validate_profile(state, plan)?;
    if matches!(plan.mutation, Mutation::SideFlag(..)) {
        return Ok(Some(vec![Action::card(state.turn, card, None)]));
    }
    let targets = ui_targets(
        state,
        plan,
        matches!(
            plan.mutation,
            Mutation::Selection(_) | Mutation::Judgment | Mutation::Evacuation | Mutation::Chain
        ),
    )?;
    let actions = match plan.mutation {
        Mutation::Selection(field) => selection_actions(state, card, &targets, field == "panic"),
        Mutation::Evacuation => selection_actions(state, card, &targets, false),
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
    let id = format!("chain-{}-{suffix}", crate::draft::frozen_timestamp()?);
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
        "thief" => default_if_missing(piece, "submerged", json!(true)),
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
    let mut candidate = state.clone();
    match apply(&mut candidate, card, action) {
        Ok(Some(_)) => Ok(Some(true)),
        Ok(None) => Ok(None),
        Err(EngineError::IllegalAction) => Ok(Some(false)),
        Err(error) => Err(error),
    }
}

pub(crate) fn apply(
    state: &mut GameState,
    card: &CardSlot,
    action: &Action,
) -> Result<Option<Vec<Piece>>> {
    let Some(plan) = plan(state, card) else {
        return Ok(None);
    };
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
            if !truthy(state.extra.get(field)) {
                state
                    .extra
                    .insert(field.into(), json!({"white":false,"black":false}));
            }
            let color = if enemy {
                state.turn.opponent()
            } else {
                state.turn
            };
            state
                .extra
                .get_mut(field)
                .and_then(Value::as_object_mut)
                .ok_or_else(|| EngineError::InvalidState(format!("{field} color map missing")))?
                .insert(color.as_str().into(), json!(true));
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
                crate::draft::frozen_timestamp()?,
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
