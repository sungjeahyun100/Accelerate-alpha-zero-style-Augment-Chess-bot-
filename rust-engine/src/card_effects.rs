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
    Pending(&'static str),
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
            Source::NonRoyal(kind) => piece.kind == kind && !state.royal_identity(piece),
            Source::QueenIdentity => piece.kind == "queen" && !piece.flag("regencyHeir"),
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
            "desperado" => (Exact(""), Pending("desperado")),
            "judgment" => (Exact(""), Judgment),
            "exile" => (Exact(""), Exile),
            "emergencyEvacuation" => (Exact(""), Pending("emergency evacuation")),
            "necromancy" => (Exact("pawn"), Necromancy),
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
            .get("fianchetto")
            .and_then(|sides| sides.get(state.turn.opponent().as_str())),
    ) || truthy(
        state
            .extra
            .get("majesty")
            .and_then(|sides| sides.get(state.turn.opponent().as_str())),
    ) {
        return Err(EngineError::UnsupportedFeature(
            "evacuation fianchetto/majesty movement restriction".into(),
        ));
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
        Mutation::Pending("desperado") => desperado_candidate(state, piece),
        Mutation::Pending("emergency evacuation") => {
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
                && (!unique || seen.insert(piece.id.clone()))
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
            Mutation::Selection(_) | Mutation::Judgment | Mutation::Pending("emergency evacuation")
        ),
    )?;
    let actions = match plan.mutation {
        Mutation::Selection(field) => selection_actions(state, card, &targets, field == "panic"),
        Mutation::Pending("emergency evacuation") => {
            selection_actions(state, card, &targets, false)
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
pub(crate) fn mark_animation(state: &mut GameState, piece: &Piece) -> Result<()> {
    if piece.id.is_empty() {
        return Ok(());
    }
    if !truthy(state.extra.get("forceAnimatedPieceIds")) {
        state.extra.insert(
            "forceAnimatedPieceIds".into(),
            json!({"__simType":"Set","values":[]}),
        );
    }
    let set = state
        .extra
        .entry("forceAnimatedPieceIds")
        .or_insert_with(|| json!({"__simType":"Set","values":[]}));
    if set.get("__simType").and_then(Value::as_str) != Some("Set") {
        return Err(EngineError::InvalidState(
            "forceAnimatedPieceIds must encode a Set".into(),
        ));
    }
    let values = set
        .get_mut("values")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState("animation Set values missing".into()))?;
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
    if let Mutation::Pending(effect) = plan.mutation {
        return Err(EngineError::UnsupportedFeature(format!(
            "targeted {effect} settlement"
        )));
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
                return Err(EngineError::UnsupportedFeature(
                    "permanent judgment vigilance and environmental removal".into(),
                ));
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
            put(&mut state, from, Color::White, queen);
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
        }
    }
}
