//! Variant base movement from frozen main-CqkYwJX4.js (abfe01a035813875).
//!
//! This module emits source coordinates and execution flags. Turn ownership,
//! protection, forced moves, portal routing, footprint execution and capture
//! effects belong to the shared legality/transition layer. There is no worker
//! strategy pruning. Loops are bounded by the 8x8 board or source bounce limit.

use crate::movement::{
    DIAG, KING, KNIGHT, ORTHO, can_capture, cannon, checker, collapsed, encouraged,
    expansion_destination_allowed, frozen, grasshopper, landing, leaps, missionary, pawn_moves,
    rays,
};
use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

// Source queenDirections and parrotBaseMoves' local KING have distinct order.
// Shared KING is kingDeltas' row-major order; do not conflate these kernels.
const QUEEN: &[(i8, i8)] = &[
    (-1, -1),
    (-1, 1),
    (1, -1),
    (1, 1),
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
];
const MEMORY_KING: &[(i8, i8)] = &[
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (-1, 1),
    (1, -1),
    (1, 1),
];

/// None means this module does not own that canonical piece kind. A recognized
/// source-dependent path that is not implemented returns an explicit error.
pub(crate) fn base_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Option<Vec<MoveTarget>>> {
    if from.row >= 8 || from.col >= 8 {
        return Err(EngineError::InvalidState(
            "variant origin outside 8x8".into(),
        ));
    }
    // The source movement fallback is queen, while capture uses the physical
    // trickster ability when its stored roll is absent/invalid (1753, 95458).
    let kind = match piece.ability_kind() {
        "trickster" => "queen",
        kind => kind,
    };
    if piece.kind == "trickster"
        && matches!(
            kind,
            "thief"
                | "brutus"
                | "clockwork"
                | "parrot"
                | "paladin"
                | "octopus"
                | "grappler"
                | "revolvingDoor"
                | "donQuixote"
                | "medium"
        )
    {
        if kind == "clockwork" && !has_neighbor(state, piece, from) {
            return Ok(Some(Vec::new()));
        }
        let memory = if matches!(kind, "parrot" | "medium") {
            memory_for(state, piece, kind)
        } else {
            Some(json!({"type":kind}))
        };
        return Ok(Some(flagged(
            memory_moves(state, piece, from, memory.as_ref())?,
            "tricksterMove",
        )));
    }
    let mut moves = match kind {
        "reaper" => source_jump_moves(state, piece, from, KING),
        "hedgehog" | "undead" | "vip" | "crown" => leaps(state, piece, from, KING),
        "octopus" => source_jump_moves(state, piece, from, QUEEN),
        "paladin" => source_jump_moves(
            state,
            piece,
            from,
            &[
                (2, 1),
                (2, -1),
                (-2, 1),
                (-2, -1),
                (1, 2),
                (1, -2),
                (-1, 2),
                (-1, -2),
            ],
        ),
        "donQuixote" => {
            let deltas = don_quixote_deltas(state, piece, from)?;
            leaps(state, piece, from, &deltas)
        }
        "recruiter" => leaps(state, piece, from, KING),
        "bear" => rays(state, piece, from, QUEEN, 7),
        "revolvingDoor" => rays(state, piece, from, ORTHO, 7),
        "babyBear" => Vec::new(),
        "wizard" => quiet(leaps(state, piece, from, KING), state),
        "idol" => quiet(rays(state, piece, from, QUEEN, 7), state),
        "darkWizard" => leaps(
            state,
            piece,
            from,
            if piece.extra.get("darkMagicCircle").is_some_and(nonempty) {
                ORTHO
            } else {
                KING
            },
        ),
        "windmill" => rays(
            state,
            piece,
            from,
            if piece.extra.get("windmillMode").and_then(Value::as_str) == Some("rook") {
                ORTHO
            } else {
                DIAG
            },
            7,
        ),
        "lobster" => {
            let dir = owner(piece)?.pawn_dir();
            leaps(state, piece, from, &[(dir, -1), (dir, 0), (dir, 1)])
        }
        "slime" => flagged(
            leaps(state, piece, from, &[(-3, 0), (3, 0), (0, -3), (0, 3)]),
            "slimeMove",
        ),
        "siren" => flagged(leaps(state, piece, from, KING), "sirenMove"),
        "magicGirl" => {
            if state.flag("magicGirlSurge", piece.color) {
                let mut moves = rays(state, piece, from, QUEEN, 7);
                moves.extend(leaps(state, piece, from, KNIGHT));
                unique(moves)
            } else {
                leaps(state, piece, from, KING)
            }
        }
        "berserker" => berserker(state, piece, from, QUEEN),
        "pegasus" => pegasus(state, piece, from),
        "dragon" => dragon(state, piece, from),
        "assassin" => assassin(state, piece, from, QUEEN),
        "jester" => rays(state, piece, from, QUEEN, 7)
            .into_iter()
            .filter(|target| {
                state.at(target.square()).is_none_or(|victim| {
                    victim.color != piece.color
                        && victim.kind != "jester"
                        && (state.royal_identity(victim) || victim.kind == "merchant")
                })
            })
            .collect(),
        "primeMinister" => {
            reject_portal(state, kind)?;
            prime_minister(state, piece, from)
        }
        "hook" | "brutus" => {
            reject_portal(state, kind)?;
            hook(state, piece, from)
        }
        "cardinal" => {
            reject_portal(state, kind)?;
            cardinal(state, piece, from)
        }
        "protestant" => {
            reject_portal(state, kind)?;
            protestant(state, piece, from)
        }
        "fanatic" => {
            reject_portal(state, kind)?;
            rays(state, piece, from, &[(owner(piece)?.pawn_dir(), 0)], 2)
        }
        "herald" => herald(state, piece, from),
        "log" => log(state, from),
        "thief" => thief(state, piece, from)?,
        "siegeRam" => {
            reject_portal(state, kind)?;
            siege_ram(from)
        }
        "shotgunKing" => shotgun(state, piece, from)?,
        "bat" | "vampireLord" => {
            reject_campaign(state, kind, "bloodMoon")?;
            if kind == "bat" {
                rays(state, piece, from, ORTHO, 2)
            } else {
                leaps(state, piece, from, KING)
            }
        }
        "timeTraveler" => {
            reject_campaign(state, kind, "timeTraveler")?;
            quiet(leaps(state, piece, from, KING), state)
        }
        "parrot" | "medium" => {
            memory_moves(state, piece, from, memory_for(state, piece, kind).as_ref())?
        }
        "merchant" => merchant(state, piece)?,
        "grappler" => grappler(state, piece, from)?,
        "colossus" => colossus(state, piece, from)?,
        "football" => football(state, piece, from)?,
        // These source types have no base switch in getLegalMoves95533. Global
        // modifiers and automatic effects remain the shared layer's concern.
        "monster" | "blackHole" | "bomb" | "platform" | "portal" => Vec::new(),
        _ if piece.kind == "trickster" => copied_classic(state, piece, from, kind)?,
        _ => return Ok(None),
    };
    if piece.kind == "trickster" {
        moves = flagged(moves, "tricksterMove");
    }
    Ok(Some(moves))
}

fn owner(piece: &Piece) -> Result<Color> {
    piece.color.owner().ok_or_else(|| {
        EngineError::InvalidState(format!(
            "moving variant {} has neutral allegiance",
            piece.kind
        ))
    })
}

/// The v7 source applies knightInjury to Don Quixote's long axis before
/// checking its destination. The intermediate blocker is transparent only for
/// a concealed enemy or an allied Ghost (the source passes "rook" to its
/// ranged transparency helper). v6 keeps its frozen unconditioned deltas.
fn don_quixote_deltas(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<(i8, i8)>> {
    if state.ruleset_id != RULES_VERSION_V7 || !state.flag("knightInjury", piece.color) {
        return Ok(KNIGHT.to_vec());
    }
    if state.extra.get("camouflageRule").is_some_and(nonempty) {
        return Err(EngineError::UnsupportedFeature(
            "v7 injured Don Quixote camouflage transparency".into(),
        ));
    }
    Ok(KNIGHT
        .iter()
        .copied()
        .filter(|&(dr, dc)| {
            let jump = if dr.abs() > dc.abs() {
                from.offset(dr.signum(), 0)
            } else {
                from.offset(0, dc.signum())
            };
            jump.and_then(|at| state.at(at)).is_none_or(|blocker| {
                blocker.color != piece.color
                    && blocker.extra.get("hiddenFrom").and_then(Value::as_str)
                        == Some(piece.color.as_str())
                    || blocker.color == piece.color
                        && blocker.extra.get("ghost").is_some_and(nonempty)
            })
        })
        .collect())
}

/// Source jumpMoves97585 accepts a directly concealed enemy square before
/// canCaptureTarget and does not inspect collapsed squares. Later move
/// restrictions discard collapsed landings, and Paladin's threeMoveAllowed
/// may reject other candidates. This function represents only the raw kernel.
fn source_jump_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    deltas: &[(i8, i8)],
) -> Vec<MoveTarget> {
    deltas
        .iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .filter(|&to| {
            state.at(to).is_none_or(|target| {
                piece.color.owner().is_some_and(|actor| {
                    target.color != piece.color
                        && target.extra.get("hiddenFrom").and_then(Value::as_str)
                            == Some(actor.as_str())
                }) || can_capture(state, piece, target)
            })
        })
        .map(MoveTarget::at)
        .collect()
}

fn nonempty(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn reject_portal(state: &GameState, kind: &str) -> Result<()> {
    if state.extra.get("portalRule").is_some_and(nonempty) {
        return Err(EngineError::UnsupportedFeature(format!(
            "{kind} base portal path requires source transit helper"
        )));
    }
    Ok(())
}

fn reject_campaign(state: &GameState, kind: &str, setup: &str) -> Result<()> {
    if state
        .extra
        .get("campaign")
        .and_then(|value| value.get("setup"))
        .and_then(Value::as_str)
        == Some(setup)
    {
        return Err(EngineError::UnsupportedFeature(format!(
            "{kind} campaign movement {setup}"
        )));
    }
    Ok(())
}

fn quiet(moves: Vec<MoveTarget>, state: &GameState) -> Vec<MoveTarget> {
    moves
        .into_iter()
        .filter(|target| state.at(target.square()).is_none())
        .collect()
}

fn flagged(mut moves: Vec<MoveTarget>, name: &str) -> Vec<MoveTarget> {
    for target in &mut moves {
        target.flags.insert(name.into(), json!(true));
    }
    moves
}

/// The frozen source's uniqueMoves retains the first source path per coordinate.
/// This is piece-kernel routing, not an alias between different player choices.
fn unique(moves: Vec<MoveTarget>) -> Vec<MoveTarget> {
    let mut seen = BTreeSet::new();
    moves
        .into_iter()
        .filter(|target| seen.insert(target.square()))
        .collect()
}

fn source_ray_order(
    mut moves: Vec<MoveTarget>,
    from: Square,
    directions: &[(i8, i8)],
) -> Vec<MoveTarget> {
    moves.sort_by_key(|target| {
        let dr = (i16::from(target.row) - i16::from(from.row)).signum() as i8;
        let dc = (i16::from(target.col) - i16::from(from.col)).signum() as i8;
        directions
            .iter()
            .position(|&direction| direction == (dr, dc))
            .unwrap_or(directions.len())
    });
    moves
}

// getLegalMoves95600~95850 and berserkerMoves95447.
fn berserker(
    state: &GameState,
    piece: &Piece,
    from: Square,
    directions: &[(i8, i8)],
) -> Vec<MoveTarget> {
    let count = state
        .board
        .iter()
        .enumerate()
        .flat_map(|(row, line)| {
            line.iter().enumerate().filter_map(move |(col, occupant)| {
                occupant
                    .as_ref()
                    .filter(|ally| ally.color == piece.color)
                    .map(|ally| {
                        if ally.id.is_empty() {
                            format!("{row}:{col}")
                        } else {
                            ally.id.clone()
                        }
                    })
            })
        })
        .collect::<BTreeSet<_>>()
        .len();
    if count <= 5 {
        let mut moves = rays(state, piece, from, directions, 7);
        moves.extend(leaps(state, piece, from, KNIGHT));
        unique(moves)
    } else if count <= 9 {
        let mut moves = leaps(state, piece, from, directions);
        moves.extend(rays(state, piece, from, ORTHO, 7));
        unique(moves)
    } else {
        leaps(state, piece, from, directions)
    }
}

fn pegasus(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let knight = KNIGHT
        .iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .collect::<BTreeSet<_>>();
    let mut moves = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let to = Square { row, col };
            if to != from
                && !collapsed(state, to)
                && state
                    .at(to)
                    .is_none_or(|victim| knight.contains(&to) && can_capture(state, piece, victim))
            {
                moves.push(MoveTarget::at(to));
            }
        }
    }
    moves
}

fn dragon(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let mut moves = leaps(state, piece, from, KNIGHT);
    for row in 0..8 {
        for col in 0..8 {
            let to = Square { row, col };
            if to == from {
                continue;
            }
            if state.at(to).is_some_and(|ally| {
                ally.color == piece.color
                    && ally.kind != "wall"
                    && !ally.is_large()
                    && ally.ability_kind() != "slime"
            }) {
                let mut target = MoveTarget::at(to);
                target.flags.insert("dragonSwap".into(), json!(true));
                moves.push(target);
            }
        }
    }
    unique(moves)
}

fn assassin(
    state: &GameState,
    piece: &Piece,
    from: Square,
    directions: &[(i8, i8)],
) -> Vec<MoveTarget> {
    let mut moves = leaps(state, piece, from, KNIGHT);
    for &(dr, dc) in directions {
        let mut cursor = from;
        for _ in 0..7 {
            let Some(to) = cursor.offset(dr, dc) else {
                break;
            };
            cursor = to;
            if let Some(victim) = state.at(to) {
                if victim.color != piece.color
                    && state.royal_identity(victim)
                    && can_capture(state, piece, victim)
                {
                    moves.push(MoveTarget::at(to));
                }
                break;
            }
        }
    }
    unique(moves)
}

// primeMinisterMoves97176: a second step requires an empty intermediate cell.
fn prime_minister(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    prime_minister_order(state, piece, from, KING)
}

fn prime_minister_order(
    state: &GameState,
    piece: &Piece,
    from: Square,
    directions: &[(i8, i8)],
) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(dr, dc) in directions {
        let Some(mid) = from.offset(dr, dc).filter(|&to| !collapsed(state, to)) else {
            continue;
        };
        if landing(state, piece, mid) {
            moves.extend(flagged(vec![MoveTarget::at(mid)], "primeMinisterMove"));
        }
        if state.at(mid).is_some() {
            continue;
        }
        for &(nr, nc) in directions {
            if let Some(to) = mid
                .offset(nr, nc)
                .filter(|&to| to != from && landing(state, piece, to))
            {
                moves.extend(flagged(vec![MoveTarget::at(to)], "primeMinisterMove"));
            }
        }
    }
    unique(moves)
}

// hookPathMoves2836: one perpendicular bend at an empty straight-path cell.
fn hook(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(dr, dc) in ORTHO {
        let mut cursor = from;
        for _ in 0..7 {
            let Some(to) = cursor.offset(dr, dc) else {
                break;
            };
            cursor = to;
            if let Some(victim) = state.at(to) {
                if can_capture(state, piece, victim) {
                    moves.push(MoveTarget::at(to));
                }
                break;
            }
            moves.push(MoveTarget::at(to));
            for &(nr, nc) in ORTHO {
                if nr * dr + nc * dc != 0 {
                    continue;
                }
                let mut bend = to;
                for _ in 0..7 {
                    let Some(landing) = bend.offset(nr, nc) else {
                        break;
                    };
                    bend = landing;
                    if let Some(victim) = state.at(landing) {
                        if can_capture(state, piece, victim) {
                            let mut target = MoveTarget::at(landing);
                            target.flags.insert("bent".into(), json!(true));
                            moves.push(target);
                        }
                        break;
                    }
                    let mut target = MoveTarget::at(landing);
                    target.flags.insert("bent".into(), json!(true));
                    moves.push(target);
                }
            }
        }
    }
    unique(moves)
}

// cardinalMoves97673: source bounces at board edges and walls, capped at 256.
fn cardinal(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(start_dr, start_dc) in DIAG {
        let (mut dr, mut dc) = (start_dr, start_dc);
        let mut cursor = from;
        let mut seen = BTreeSet::new();
        for _ in 0..256 {
            let mut row = i16::from(cursor.row) + i16::from(dr);
            let mut col = i16::from(cursor.col) + i16::from(dc);
            if !(0..8).contains(&row) || !(0..8).contains(&col) {
                if !(0..8).contains(&row) {
                    dr = -dr;
                }
                if !(0..8).contains(&col) {
                    dc = -dc;
                }
                row = i16::from(cursor.row) + i16::from(dr);
                col = i16::from(cursor.col) + i16::from(dc);
            }
            if !(0..8).contains(&row) || !(0..8).contains(&col) {
                break;
            }
            let to = Square {
                row: row as u8,
                col: col as u8,
            };
            if !seen.insert((to, dr, dc)) {
                break;
            }
            if let Some(victim) = state.at(to) {
                if victim.kind == "wall" {
                    dr = -dr;
                    dc = -dc;
                    continue;
                }
                if can_capture(state, piece, victim) {
                    moves.push(MoveTarget::at(to));
                }
                break;
            }
            moves.push(MoveTarget::at(to));
            cursor = to;
        }
    }
    unique(moves)
}

fn protestant(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(dr, dc) in DIAG {
        for distance in 1..=3 {
            let Some(to) = from.offset(dr * distance, dc * distance) else {
                break;
            };
            // This source kernel jumps past occupants, including allied ones.
            if landing(state, piece, to) {
                moves.push(MoveTarget::at(to));
            }
        }
    }
    moves
}

fn herald(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let lock = piece
        .extra
        .get("heraldJumpLockTurn")
        .and_then(herald_lock_number);
    let locked = if let Some(turn) = lock {
        piece.color == state.turn && turn == *state.turns_taken.get(state.turn) as f64
    } else {
        piece.extra.get("heraldJumpUnlocked") == Some(&Value::Bool(false))
    };
    let mut moves = Vec::new();
    for &(dr, dc) in ORTHO {
        for distance in 1..=3 {
            let Some(to) = from.offset(dr * distance, dc * distance) else {
                break;
            };
            match state.at(to) {
                None => moves.push(MoveTarget::at(to)),
                Some(target)
                    if target.color != piece.color
                        && target.extra.get("hiddenFrom").and_then(Value::as_str)
                            == Some(piece.color.as_str()) =>
                {
                    // movementOccupant treats directly concealed enemies as empty.
                    moves.push(MoveTarget::at(to));
                }
                Some(target)
                    if target.color == piece.color
                        && target.extra.get("ghost").is_some_and(nonempty) =>
                {
                    // Herald is ranged, so allied Ghost is transparent even
                    // when the jump lock would stop at a normal occupant.
                }
                Some(_) if locked => break,
                Some(_) => {}
            }
        }
    }
    moves
}

// Number.isFinite(Number(heraldJumpLockTurn)) in isHeraldJumpLocked72082.
// JSON arrays stringify to their one element (or an empty string), while
// Boolean/object members stringify to nonnumeric words. A bounded walk avoids
// recursion on arbitrary input without widening the movement contract.
fn herald_lock_number(mut value: &Value) -> Option<f64> {
    for _ in 0..64 {
        match value {
            Value::Array(items) => match items.as_slice() {
                [] | [Value::Null] => return Some(0.0),
                [Value::Bool(_) | Value::Object(_)] => return None,
                [single] => value = single,
                _ => return None,
            },
            Value::String(text) => return herald_lock_string_number(text),
            _ => return crate::observation::number(Some(value)),
        }
    }
    None
}

fn herald_lock_string_number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return Some(0.0);
    }
    let radix = [
        ("0x", 16),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ]
    .into_iter()
    .find_map(|(prefix, radix)| text.strip_prefix(prefix).map(|digits| (digits, radix)));
    if let Some((digits, radix)) = radix {
        if digits.is_empty() {
            return None;
        }
        let value = digits.chars().try_fold(0.0, |value, digit| {
            Some(value * f64::from(radix) + f64::from(digit.to_digit(radix)?))
        })?;
        return value.is_finite().then_some(value);
    }
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn log(state: &GameState, from: Square) -> Vec<MoveTarget> {
    QUEEN
        .iter()
        .filter(|&&(dr, dc)| {
            state.extra.get("monochromeChess") != Some(&Value::Bool(true)) || dr != 0 && dc != 0
        })
        .filter_map(|&(dr, dc)| {
            from.offset(dr, dc).map(|to| {
                let mut target = MoveTarget::at(to);
                target
                    .flags
                    .insert("setLogDirection".into(), json!({"dr":dr,"dc":dc}));
                target
            })
        })
        .collect()
}

fn thief(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    if state.extra.get("thiefQuietJump") == Some(&Value::Bool(false))
        || state.extra.get("thiefRequiredJump") == Some(&Value::Bool(true))
    {
        return Err(EngineError::UnsupportedFeature(
            "legacy thief movement profile".into(),
        ));
    }
    let mut moves = Vec::new();
    for distance in [-3_i8, -2, -1, 1, 2, 3] {
        for (dr, dc) in [(distance, 0), (0, distance)] {
            let Some(to) = from.offset(dr, dc).filter(|&to| landing(state, piece, to)) else {
                continue;
            };
            let jumped = (1..distance.abs()).any(|step| {
                from.offset(dr.signum() * step, dc.signum() * step)
                    .and_then(|square| state.at(square))
                    .is_some()
            });
            if jumped && state.at(to).is_some() {
                continue;
            }
            let mut target = MoveTarget::at(to);
            if jumped {
                target.flags.insert("thiefQuietJump".into(), json!(true));
            }
            moves.push(target);
        }
    }
    Ok(moves)
}

/// The source executes a Siege Ram's occupied path cells in this order before
/// handling a direct or jump capture. `highlightCells` is therefore an
/// execution descriptor, not merely a display hint. Portal paths require a
/// separate descriptor and are deliberately rejected here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiegeRamPath {
    cells: Vec<Square>,
}

#[allow(dead_code, reason = "v7 Siege Ram transition is staged")]
impl SiegeRamPath {
    pub(crate) fn from_target(from: Square, target: &MoveTarget) -> Result<Option<Self>> {
        if !target.flag("siegeRamMove") {
            return Ok(None);
        }
        if target.flags.get("siegeRamMove") != Some(&json!(true)) {
            return Err(EngineError::IllegalAction);
        }
        if target.flag("portalLanding") || target.flag("portalThrough") {
            return Err(EngineError::UnsupportedFeature(
                "Siege Ram portal path descriptor".into(),
            ));
        }
        let to = target.square();
        let row_distance = from.row.abs_diff(to.row);
        let col_distance = from.col.abs_diff(to.col);
        let (dr, dc, distance) = match (row_distance, col_distance) {
            (1..=2, 0) => (
                (i16::from(to.row) - i16::from(from.row)).signum() as i8,
                0,
                row_distance,
            ),
            (0, 1..=2) => (
                0,
                (i16::from(to.col) - i16::from(from.col)).signum() as i8,
                col_distance,
            ),
            _ => return Err(EngineError::IllegalAction),
        };
        let mut cells = Vec::with_capacity(usize::from(distance));
        for step in 1..=distance {
            cells.push(
                from.offset(dr * step as i8, dc * step as i8)
                    .ok_or(EngineError::IllegalAction)?,
            );
        }
        if target.flags.get("highlightCells") != Some(&json!(cells)) {
            return Err(EngineError::IllegalAction);
        }
        Ok(Some(Self { cells }))
    }

    pub(crate) fn cells(&self) -> &[Square] {
        &self.cells
    }

    /// The source's saturation gate counts distinct occupied piece IDs over
    /// the whole path, including allied and neutral pieces. It does not call
    /// ordinary `canCaptureTarget` on each path occupant.
    pub(crate) fn potential_capture_count(&self, state: &GameState) -> usize {
        self.cells
            .iter()
            .filter_map(|&square| state.at(square))
            .map(|piece| piece.id.as_str())
            .collect::<BTreeSet<_>>()
            .len()
    }

    /// `captured` must contain actual successful path captures, keyed by the
    /// cell where each capture occurred. Forced removals and failed captures
    /// must not be included. The caller handles direct/jump fallback after
    /// this source-priority path victim.
    pub(crate) fn first_chameleon_victim<'a>(
        &self,
        state: &GameState,
        captured: &'a [(Square, Piece)],
    ) -> Result<Option<&'a Piece>> {
        if captured.len() > self.cells.len()
            || captured
                .iter()
                .any(|(square, _)| !self.cells.contains(square))
            || captured.iter().enumerate().any(|(index, (square, _))| {
                captured[..index].iter().any(|(other, _)| other == square)
            })
        {
            return Err(EngineError::InvalidState(
                "Siege Ram capture trace outside path".into(),
            ));
        }
        Ok(self.cells.iter().find_map(|cell| {
            captured.iter().find_map(|(at, victim)| {
                (at == cell
                    && !state.royal_identity(victim)
                    && !matches!(
                        victim.kind.as_str(),
                        "wall" | "colossus" | "bigRook" | "bigBishop"
                    ))
                .then_some(victim)
            })
        }))
    }
}

fn siege_ram(from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(dr, dc) in ORTHO {
        let mut cells = Vec::new();
        for distance in 1..=2 {
            let Some(to) = from.offset(dr * distance, dc * distance) else {
                break;
            };
            cells.push(to);
            let mut target = MoveTarget::at(to);
            target.flags.insert("siegeRamMove".into(), json!(true));
            target.flags.insert("highlightCells".into(), json!(cells));
            moves.push(target);
        }
    }
    moves
}

fn shotgun(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    match state.extra.get("shotgunAction").and_then(Value::as_str) {
        Some("shotgun") => {
            let mut moves = Vec::new();
            for row in 0..8 {
                for col in 0..8 {
                    let to = Square { row, col };
                    if to == from {
                        continue;
                    }
                    let mut target = MoveTarget::at(to);
                    target.flags.insert("shotgunBlast".into(), json!(true));
                    target.flags.insert(
                        "shotgunDirection".into(),
                        json!([
                            (i16::from(row) - i16::from(from.row)).signum(),
                            (i16::from(col) - i16::from(from.col)).signum()
                        ]),
                    );
                    moves.push(target);
                }
            }
            Ok(moves)
        }
        Some("snipe") => {
            if piece.number("ammo") < 3 {
                return Ok(Vec::new());
            }
            let mut moves = Vec::new();
            for &(dr, dc) in QUEEN {
                let mut cursor = from;
                for _ in 0..7 {
                    let Some(to) = cursor.offset(dr, dc) else {
                        break;
                    };
                    cursor = to;
                    if let Some(victim) = state.at(to) {
                        if can_capture(state, piece, victim) && victim.ability_kind() != "jester" {
                            moves.extend(flagged(vec![MoveTarget::at(to)], "shotgunSnipe"));
                        }
                        break;
                    }
                }
            }
            Ok(moves)
        }
        _ => Ok(quiet(leaps(state, piece, from, KING), state)),
    }
}

fn has_neighbor(state: &GameState, piece: &Piece, from: Square) -> bool {
    KING.iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .any(|to| state.at(to).is_some_and(|ally| ally.color == piece.color))
}

fn memory_for(state: &GameState, piece: &Piece, kind: &str) -> Option<Value> {
    if kind == "medium" {
        state.extra.get("mediumMovement").cloned()
    } else {
        state
            .extra
            .get("parrotMovement")?
            .get(owner(piece).ok()?.as_str())
            .cloned()
    }
}

fn canonical_memory_type(raw: &str) -> String {
    let mut result = String::new();
    let mut capitalize = false;
    for ch in raw.chars() {
        if ch == '-' {
            capitalize = true;
        } else if capitalize {
            result.extend(ch.to_uppercase());
            capitalize = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// parrotBaseMoves15923 is deliberately different from native piece kernels:
/// it marks captures and copies only remembered base movement, not abilities.
fn memory_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    memory: Option<&Value>,
) -> Result<Vec<MoveTarget>> {
    let Some(memory) = memory.filter(|value| {
        value
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| !kind.is_empty())
    }) else {
        return Ok(Vec::new());
    };
    let kind = canonical_memory_type(memory["type"].as_str().expect("checked memory type"));
    reject_portal(state, &kind)?;
    let mut moves = match kind.as_str() {
        "hook" | "brutus" => return Ok(hook(state, piece, from)),
        "protestant" => return Ok(protestant(state, piece, from)),
        "cannon" => return Ok(cannon(state, piece, from)),
        "pawn" | "squire" | "standardBearer" => {
            let dir = owner(piece)?.pawn_dir();
            let mut moves = Vec::new();
            if let Some(to) = from.offset(dir, 0).filter(|&to| state.at(to).is_none()) {
                moves.push(MoveTarget::at(to));
                if !piece.moved
                    && from.row == if piece.color == Color::White { 6 } else { 1 }
                    && let Some(to) = from.offset(dir * 2, 0).filter(|&to| state.at(to).is_none())
                {
                    moves.push(MoveTarget::at(to));
                }
            }
            moves.extend(
                leaps(state, piece, from, &[(dir, -1), (dir, 1)])
                    .into_iter()
                    .filter(|to| state.at(to.square()).is_some()),
            );
            moves
        }
        "queen" | "bear" | "clockwork" | "grappler" => rays(state, piece, from, MEMORY_KING, 7),
        "rook" | "bigRook" | "revolvingDoor" => rays(state, piece, from, ORTHO, 7),
        "bishop" | "bigBishop" => rays(state, piece, from, DIAG, 7),
        "knight" | "unicorn" | "donQuixote" => leaps(state, piece, from, KNIGHT),
        "dragon" => dragon(state, piece, from),
        "magicGirl" => {
            if state.flag("magicGirlSurge", piece.color) {
                let mut moves = rays(state, piece, from, MEMORY_KING, 7);
                moves.extend(leaps(state, piece, from, KNIGHT));
                moves
            } else {
                leaps(state, piece, from, MEMORY_KING)
            }
        }
        "berserker" => berserker(state, piece, from, MEMORY_KING),
        "assassin" => assassin(state, piece, from, MEMORY_KING),
        "paladin" => quiet(leaps(state, piece, from, KNIGHT), state),
        "royalKnight" => leaps(state, piece, from, KNIGHT),
        "king" | "octopus" | "man" | "guard" | "reaper" | "recruiter" | "vip" | "crown"
        | "darkWizard" | "undead" | "hedgehog" | "vampireLord" | "siren" | "log" | "monster" => {
            leaps(state, piece, from, MEMORY_KING)
        }
        "ferz" | "knightmaster" | "princess" => leaps(state, piece, from, DIAG),
        "amazon" => {
            let mut moves = rays(state, piece, from, MEMORY_KING, 7);
            moves.extend(leaps(state, piece, from, KNIGHT));
            moves
        }
        "cardinal" => memory_cardinal(state, piece, from),
        "camel" => leaps(
            state,
            piece,
            from,
            &[
                (-3, -1),
                (-3, 1),
                (-1, -3),
                (-1, 3),
                (1, -3),
                (1, 3),
                (3, -1),
                (3, 1),
            ],
        ),
        "alfil" => leaps(state, piece, from, &[(-2, -2), (-2, 2), (2, -2), (2, 2)]),
        "eagle" | "alibaba" => leaps(
            state,
            piece,
            from,
            &[
                (-2, -2),
                (-2, 2),
                (2, -2),
                (2, 2),
                (-2, 0),
                (2, 0),
                (0, -2),
                (0, 2),
            ],
        ),
        "thief" => {
            let raw = thief(state, piece, from)?;
            let mut moves = Vec::new();
            // Memory helper visits distance 1..3 then ORTH; native thief's
            // signed-offset order is different, and source preserves ordering.
            for distance in 1..=3 {
                for &(dr, dc) in ORTHO {
                    if let Some(to) = from.offset(dr * distance, dc * distance)
                        && let Some(target) = raw.iter().find(|target| target.square() == to)
                    {
                        moves.push(target.clone());
                    }
                }
            }
            moves
        }
        "grasshopper" => source_ray_order(grasshopper(state, piece, from), from, MEMORY_KING),
        "jester" | "idol" => quiet(rays(state, piece, from, MEMORY_KING, 7), state),
        "lobster" => {
            let dir = owner(piece)?.pawn_dir();
            leaps(state, piece, from, &[(dir, -1), (dir, 0), (dir, 1)])
        }
        "fanatic" => {
            let dir = owner(piece)?.pawn_dir();
            let mut moves = quiet(leaps(state, piece, from, &[(dir, -1), (dir, 1)]), state);
            moves.extend(
                leaps(state, piece, from, &[(dir, 0)])
                    .into_iter()
                    .filter(|to| state.at(to.square()).is_some()),
            );
            moves
        }
        "bat" => rays(state, piece, from, ORTHO, 2),
        "slime" => leaps(state, piece, from, &[(-3, 0), (3, 0), (0, -3), (0, 3)]),
        "colossus" => leaps(state, piece, from, ORTHO),
        "siegeRam" => {
            let mut moves = Vec::new();
            for distance in 1..=2 {
                moves.extend(leaps(
                    state,
                    piece,
                    from,
                    &[(-distance, 0), (distance, 0), (0, -distance), (0, distance)],
                ));
            }
            moves
        }
        "wizard" | "shotgunKing" | "timeTraveler" => {
            quiet(leaps(state, piece, from, MEMORY_KING), state)
        }
        "campfire" => quiet(leaps(state, piece, from, ORTHO), state),
        "missionary" => quiet(leaps(state, piece, from, DIAG), state),
        "herald" => {
            let mut moves = Vec::new();
            for distance in 1..=3 {
                moves.extend(quiet(
                    leaps(
                        state,
                        piece,
                        from,
                        &[(-distance, 0), (distance, 0), (0, -distance), (0, distance)],
                    ),
                    state,
                ));
            }
            moves
        }
        "pegasus" => {
            let mut moves = Vec::new();
            for row in 0..8 {
                for col in 0..8 {
                    let to = Square { row, col };
                    if to != from && state.at(to).is_none() {
                        moves.push(MoveTarget::at(to));
                    }
                }
            }
            moves.extend(leaps(state, piece, from, KNIGHT));
            moves
        }
        "primeMinister" => prime_minister_order(state, piece, from, MEMORY_KING)
            .into_iter()
            .map(|mut target| {
                target.flags.remove("primeMinisterMove");
                target
            })
            .collect(),
        "checker" | "checkerKing" => {
            let dir = owner(piece)?.pawn_dir();
            let directions: &[(i8, i8)] = if kind == "checkerKing" {
                DIAG
            } else {
                &[(dir, -1), (dir, 1)]
            };
            let mut moves = quiet(leaps(state, piece, from, directions), state);
            for &(dr, dc) in directions {
                if let Some(mid) = from.offset(dr, dc)
                    && state
                        .at(mid)
                        .is_some_and(|target| can_capture(state, piece, target))
                    && let Some(to) = from
                        .offset(dr * 2, dc * 2)
                        .filter(|&to| state.at(to).is_none())
                {
                    let mut target = MoveTarget::at(to);
                    target.flags.insert("jumpCapture".into(), json!(mid));
                    moves.push(target);
                }
            }
            moves
        }
        "windmill" => rays(
            state,
            piece,
            from,
            if memory.get("windmillMode").and_then(Value::as_str) == Some("rook") {
                ORTHO
            } else {
                DIAG
            },
            7,
        ),
        // Source parrotBaseMoves has no movement branch for these remembered
        // stationary types. This is not an unknown-type fallback.
        "babyBear" | "coffin" | "scarecrow" | "wall" | "merchant" | "medium" | "parrot"
        | "football" | "blackHole" | "bomb" | "platform" | "portal" => Vec::new(),
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "remembered base movement {kind}"
            )));
        }
    };
    for target in &mut moves {
        if state.at(target.square()).is_some() && !target.flag("dragonSwap") {
            target.flags.insert("capture".into(), json!(true));
        }
    }
    // JS Map keeps the first key position, replacing its value on duplicates.
    let mut result: Vec<MoveTarget> = Vec::new();
    for target in moves {
        if let Some(existing) = result
            .iter_mut()
            .find(|old| old.square() == target.square())
        {
            *existing = target;
        } else {
            result.push(target);
        }
    }
    Ok(result)
}

fn memory_cardinal(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(start_dr, start_dc) in DIAG {
        let (mut dr, mut dc) = (start_dr, start_dc);
        let mut cursor = from;
        let mut seen = BTreeSet::new();
        for _ in 0..256 {
            if cursor.offset(dr, 0).is_none() {
                dr = -dr;
            }
            if cursor.offset(0, dc).is_none() {
                dc = -dc;
            }
            let Some(to) = cursor.offset(dr, dc) else {
                break;
            };
            if to == from || !seen.insert((to, dr, dc)) {
                break;
            }
            if landing(state, piece, to) {
                moves.push(MoveTarget::at(to));
            }
            if state.at(to).is_some() {
                break;
            }
            cursor = to;
        }
    }
    moves
}

fn copied_classic(
    state: &GameState,
    piece: &Piece,
    from: Square,
    kind: &str,
) -> Result<Vec<MoveTarget>> {
    Ok(match kind {
        "pawn" | "squire" | "standardBearer" => {
            let mut override_piece = piece.clone();
            override_piece.kind = kind.into();
            pawn_moves(state, &override_piece, from)
        }
        "checker" | "checkerKing" => {
            let mut override_piece = piece.clone();
            override_piece.kind = kind.into();
            checker(state, &override_piece, from)
        }
        "rook" => rays(state, piece, from, ORTHO, 7),
        "bishop" => rays(state, piece, from, DIAG, 7),
        "queen" | "clockwork" => rays(state, piece, from, QUEEN, 7),
        "knight" => leaps(state, piece, from, KNIGHT),
        "man" | "guard" => leaps(state, piece, from, KING),
        "ferz" | "knightmaster" => leaps(state, piece, from, DIAG),
        "camel" => leaps(
            state,
            piece,
            from,
            &[
                (-3, -1),
                (-3, 1),
                (-1, -3),
                (-1, 3),
                (1, -3),
                (1, 3),
                (3, -1),
                (3, 1),
            ],
        ),
        "alfil" => leaps(state, piece, from, &[(-2, -2), (-2, 2), (2, -2), (2, 2)]),
        "eagle" => leaps(
            state,
            piece,
            from,
            &[
                (-2, -2),
                (-2, 0),
                (-2, 2),
                (0, -2),
                (0, 2),
                (2, -2),
                (2, 0),
                (2, 2),
            ],
        ),
        "amazon" => {
            let mut moves = rays(state, piece, from, QUEEN, 7);
            moves.extend(leaps(state, piece, from, KNIGHT));
            unique(moves)
        }
        "cannon" => cannon(state, piece, from),
        "grasshopper" => source_ray_order(grasshopper(state, piece, from), from, QUEEN),
        "campfire" => quiet(
            leaps(state, piece, from, &[(-1, 0), (0, -1), (0, 1), (1, 0)]),
            state,
        ),
        "princess" => {
            if state
                .board
                .iter()
                .flatten()
                .flatten()
                .any(|ally| ally.color == piece.color && ally.kind == "queen")
            {
                leaps(state, piece, from, DIAG)
            } else {
                rays(state, piece, from, QUEEN, 7)
            }
        }
        "missionary" => missionary(state, piece, from),
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "trickster copied base movement {kind}"
            )));
        }
    })
}

fn reserved(state: &GameState, to: Square) -> bool {
    state
        .extra
        .get("pendingScarecrows")
        .and_then(Value::as_array)
        .is_some_and(|entries| {
            entries.iter().any(|entry| {
                entry.get("reserved") == Some(&Value::Bool(true))
                    && entry.get("row").and_then(Value::as_u64) == Some(u64::from(to.row))
                    && entry.get("col").and_then(Value::as_u64) == Some(u64::from(to.col))
            })
        })
}

fn grappler(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    reject_portal(state, "grappler")?;
    let mut moves = Vec::new();
    for mut target in rays(state, piece, from, QUEEN, 7) {
        let to = target.square();
        let Some(victim) = state.at(to) else {
            moves.push(target);
            continue;
        };
        let dr = i16::from(to.row) - i16::from(from.row);
        let dc = i16::from(to.col) - i16::from(from.col);
        if dr.abs().max(dc.abs()) < 2 {
            moves.push(target);
            continue;
        }
        if piece.flag("grapplerBound") {
            continue;
        }
        let shift_r = i16::from(from.row) + dr.signum() - i16::from(to.row);
        let shift_c = i16::from(from.col) + dc.signum() - i16::from(to.col);
        let mut cells = Vec::new();
        let mut valid = true;
        for row in 0..8 {
            for col in 0..8 {
                let source = Square { row, col };
                let Some(occupant) = state.at(source) else {
                    continue;
                };
                if source != to && (victim.id.is_empty() || occupant.id != victim.id) {
                    continue;
                }
                let row = i16::from(row) + shift_r;
                let col = i16::from(col) + shift_c;
                if !(0..8).contains(&row) || !(0..8).contains(&col) {
                    valid = false;
                    continue;
                }
                let cell = Square {
                    row: row as u8,
                    col: col as u8,
                };
                if collapsed(state, cell)
                    || reserved(state, cell)
                    || state
                        .at(cell)
                        .is_some_and(|other| victim.id.is_empty() || other.id != victim.id)
                {
                    valid = false;
                }
                cells.push(cell);
            }
        }
        if valid && expansion_destination_allowed(state, victim.color, &cells) {
            target.flags.insert("capture".into(), json!(false));
            target.flags.insert("grapplePull".into(), json!(true));
            moves.push(target);
        }
    }
    Ok(moves)
}

fn reject_pending_capture_policy(state: &GameState, kind: &str) -> Result<()> {
    if state
        .extra
        .get("highGround")
        .and_then(Value::as_array)
        .is_some_and(|cells| !cells.is_empty())
    {
        return Err(EngineError::UnsupportedFeature(format!(
            "{kind} pending source high-ground policy"
        )));
    }
    Ok(())
}

fn cells_2x2(from: Square) -> Vec<Square> {
    if from.row >= 7 || from.col >= 7 {
        return Vec::new();
    }
    vec![
        from,
        Square {
            row: from.row,
            col: from.col + 1,
        },
        Square {
            row: from.row + 1,
            col: from.col,
        },
        Square {
            row: from.row + 1,
            col: from.col + 1,
        },
    ]
}

fn colossus(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    reject_pending_capture_policy(state, "colossus")?;
    let mut body = MoveTarget::at(from);
    body.flags
        .insert("bodyCells".into(), json!(cells_2x2(from)));
    body.flags.insert("colossusBody".into(), json!(true));
    let mut moves = vec![body];
    for &(dr, dc) in ORTHO {
        let Some(anchor) = from.offset(dr, dc) else {
            continue;
        };
        let cells = cells_2x2(anchor);
        if cells.len() != 4 {
            continue;
        }
        let mut captures = Vec::new();
        let mut seen = BTreeSet::new();
        let mut allowed = true;
        for &cell in &cells {
            let Some(victim) = state.at(cell) else {
                continue;
            };
            if !piece.id.is_empty() && victim.id == piece.id {
                continue;
            }
            if encouraged(state, victim) || !can_capture(state, piece, victim) {
                allowed = false;
                break;
            }
            let id = if victim.id.is_empty() {
                format!("{}:{}", cell.row, cell.col)
            } else {
                victim.id.clone()
            };
            if seen.insert(id) {
                captures.push(cell);
            }
        }
        if !allowed {
            continue;
        }
        let display = if dr != 0 {
            let row = if dr < 0 { from.row - 1 } else { from.row + 2 };
            vec![
                Square { row, col: from.col },
                Square {
                    row,
                    col: from.col + 1,
                },
            ]
        } else {
            let col = if dc < 0 { from.col - 1 } else { from.col + 2 };
            vec![
                Square { row: from.row, col },
                Square {
                    row: from.row + 1,
                    col,
                },
            ]
        };
        let mut target = MoveTarget::at(anchor);
        for (name, value) in [
            ("anchorRow", json!(anchor.row)),
            ("anchorCol", json!(anchor.col)),
            ("highlightCells", json!(cells)),
            ("displayCells", json!(display)),
            ("colossusLandingCaptures", json!(captures)),
            ("colossusMove", json!(true)),
        ] {
            target.flags.insert(name.into(), value);
        }
        moves.push(target);
    }
    let owner = owner(piece)?;
    let row_start =
        (i16::from(from.row) + if owner == Color::White { -3 } else { 3 }).clamp(0, 6) as u8;
    let rows = if owner == Color::White {
        [row_start + 1, row_start]
    } else {
        [row_start, row_start + 1]
    };
    for col_start in [
        (i16::from(from.col) + 3).clamp(0, 6) as u8,
        (i16::from(from.col) - 3).clamp(0, 6) as u8,
    ] {
        let cells = vec![
            Square {
                row: rows[0],
                col: col_start,
            },
            Square {
                row: rows[0],
                col: col_start + 1,
            },
            Square {
                row: rows[1],
                col: col_start,
            },
            Square {
                row: rows[1],
                col: col_start + 1,
            },
        ];
        if !cells.iter().any(|&cell| {
            state.at(cell).is_some_and(|victim| {
                victim.color == owner.opponent()
                    && !victim.flag("shielded")
                    && !matches!(victim.ability_kind(), "guard" | "jester")
                    && victim.kind != "monster"
                    && !encouraged(state, victim)
                    && can_capture(state, piece, victim)
            })
        }) {
            continue;
        }
        for &cell in &cells {
            let mut target = MoveTarget::at(cell);
            target.flags.insert("sectorCells".into(), json!(cells));
            target.flags.insert("colossusAttack".into(), json!(true));
            moves.push(target);
        }
    }
    Ok(unique(moves))
}

// merchantPurchasePrice3795 uses the encyclopedia values, not display scores
// or capture material values. Unsupported campaign/editor kinds stay unpriced.
fn merchant_price(piece: &Piece) -> Option<i64> {
    if matches!(
        piece.kind.as_str(),
        "king"
            | "royalKnight"
            | "darkWizard"
            | "shotgunKing"
            | "vip"
            | "merchant"
            | "timeTraveler"
            | "vampireLord"
    ) || ["crownRoyal", "editorRoyal", "regencyHeir"]
        .iter()
        .any(|&key| {
            piece.extra.get(key) == Some(&json!(true))
                || piece
                    .extra
                    .get("attributes")
                    .and_then(|value| value.get(key))
                    == Some(&json!(true))
        })
    {
        return Some(20);
    }
    let price = match piece.kind.as_str() {
        "coffin" => 1,
        "pawn" | "squire" | "fanatic" | "alfil" | "checker" => 2,
        "missionary" | "eagle" | "camel" | "log" | "standardBearer" | "guard" | "checkerKing"
        | "lobster" => 2,
        "bishop" | "knight" | "protestant" | "knightmaster" | "medium" => 3,
        "cannon" | "grasshopper" | "man" | "assassin" | "windmill" | "babyBear" | "undead"
        | "paladin" | "campfire" => 4,
        "rook" | "herald" | "pegasus" | "dragon" | "siegeRam" | "slime" | "trickster"
        | "octopus" | "clockwork" | "parrot" | "revolvingDoor" => 5,
        "magicGirl" | "princess" => 6,
        "cardinal" | "berserker" | "donQuixote" => 7,
        "bigRook" | "bigBishop" => 8,
        "queen" | "primeMinister" | "jester" | "reaper" | "recruiter" | "wizard" | "idol"
        | "siren" | "thief" | "hedgehog" => 9,
        "brutus" => 10,
        "colossus" => 12,
        "amazon" | "grappler" => 13,
        "hook" => 15,
        "bear" => 17,
        _ => return None,
    };
    Some(price)
}

fn merchant(state: &GameState, piece: &Piece) -> Result<Vec<MoveTarget>> {
    if state.extra.get("armistice").is_some_and(nonempty) {
        return Err(EngineError::UnsupportedFeature(
            "merchant source armistice predicate".into(),
        ));
    }
    let enemy = owner(piece)?.opponent();
    let gold = piece.number("gold");
    let mut moves = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let to = Square { row, col };
            let Some(victim) = state.at(to) else {
                continue;
            };
            if victim.color != enemy
                || matches!(
                    victim.kind.as_str(),
                    "merchant" | "wall" | "football" | "monster" | "blackHole"
                )
                || frozen(victim)
                || (victim.kind != "scarecrow" && victim.flag("shielded"))
            {
                continue;
            }
            if let Some(price) = merchant_price(victim).filter(|price| *price <= gold) {
                let mut target = MoveTarget::at(to);
                target.flags.insert("merchantBuy".into(), json!(true));
                target.flags.insert("cost".into(), json!(price));
                moves.push(target);
            }
        }
    }
    Ok(moves)
}

fn football(state: &GameState, _piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    reject_pending_capture_policy(state, "football")?;
    if state.extra.get("crownRule").is_some_and(nonempty)
        || state.extra.get("diceLocks").is_some_and(|value| {
            value
                .as_object()
                .is_some_and(|map| map.values().any(nonempty))
        })
    {
        return Err(EngineError::UnsupportedFeature(
            "football pending source crown/dice policy".into(),
        ));
    }
    let mut moves = Vec::new();
    if [0, 7].contains(&from.row) && [0, 7].contains(&from.col) {
        let dr = if from.row == 0 { 1 } else { -1 };
        let dc = if from.col == 0 { 1 } else { -1 };
        for (dr, dc) in [(dr, 0), (0, dc), (dr, dc)] {
            if let Some(to) = from.offset(dr, dc).filter(|&to| state.at(to).is_none()) {
                moves.extend(flagged(vec![MoveTarget::at(to)], "footballCornerMove"));
            }
        }
    }
    for &(dr, dc) in QUEEN {
        let Some(kicker_square) = from.offset(-dr, -dc) else {
            continue;
        };
        let Some(kicker) = state.at(kicker_square) else {
            continue;
        };
        if kicker.color != state.turn
            || matches!(kicker.kind.as_str(), "football" | "wall")
            || frozen(kicker)
        {
            continue;
        }
        if kicker.extra.get("hiddenFrom").is_some_and(nonempty)
            || kicker.flag("camouflage")
            || kicker.flag("hallucination")
        {
            return Err(EngineError::UnsupportedFeature(
                "football kicker visibility source policy".into(),
            ));
        }
        let attacker = Piece::new("football", state.turn, "football-capture-policy");
        let mut cursor = from;
        for _ in 0..7 {
            let Some(to) = cursor.offset(dr, dc) else {
                break;
            };
            cursor = to;
            let victim = state.at(to);
            if victim.is_none_or(|victim| {
                !encouraged(state, victim) && can_capture(state, &attacker, victim)
            }) {
                let mut target = MoveTarget::at(to);
                target.flags.insert("footballKick".into(), json!(true));
                target.flags.insert("kicker".into(), json!(kicker_square));
                if victim.is_some() {
                    target.flags.insert("footballCapture".into(), json!(true));
                }
                moves.push(target);
            }
            if victim.is_some() {
                break;
            }
        }
    }
    Ok(unique(moves))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty(kind: &str) -> (GameState, Piece, Square) {
        let mut state = GameState::new(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            11,
        )
        .unwrap();
        state.board = vec![vec![None; 8]; 8];
        let from = Square { row: 4, col: 3 };
        let piece = Piece::new(kind, Color::White, "variant");
        state.board[4][3] = Some(piece.clone());
        (state, piece, from)
    }

    fn put(state: &mut GameState, square: Square, kind: &str, color: Color) {
        state.board[square.row as usize][square.col as usize] = Some(Piece::new(
            kind,
            color,
            format!("target-{}-{}", square.row, square.col),
        ));
    }

    fn moves(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
        base_moves(state, piece, from).unwrap().unwrap()
    }

    #[test]
    fn v7_source_reachable_grappler_keeps_four_ordered_plain_rays() {
        // Frozen v7 seed 19 grand: twelve active-only draft picks, White e2-e4,
        // Black a7-a6, then White's Grappler card converts the queen on d1
        // while sacrificing the knight on b1. Source raw and complete legal
        // streams both emit e2, f3, g4, h5 with no execution flags.
        let mut state = crate::draft::initialize_for_ruleset(
            GameConfig {
                game_style: "grand".into(),
                ..GameConfig::default()
            },
            19,
            RULES_VERSION_V7,
        )
        .unwrap();
        for (offer, id) in [
            (1, "princess"),
            (3, "inertia"),
            (3, "suicide-bomber"),
            (3, "miracle"),
            (3, "suspicious-potion"),
            (3, "taunt"),
            (3, "symmetry"),
            (3, "trojan-horse"),
            (5, "chimera"),
            (5, "grasshopper"),
            (5, "missionary"),
            (5, "grappler"),
        ] {
            let action = crate::draft::legal_actions(&state).unwrap().remove(offer);
            let chosen = state.extra["draft"]["choices"]
                .as_array()
                .unwrap()
                .iter()
                .find(|card| card["instanceId"].as_str() == action.card_instance_id.as_deref())
                .unwrap();
            assert_eq!(chosen["id"], id);
            crate::draft::apply_pick(&mut state, &action).unwrap();
            crate::replay::canonicalize_position_frames(&mut state).unwrap();
        }
        assert_eq!(state.mode, "play");
        for (from, to) in [((6, 4), (4, 4)), ((1, 0), (2, 0))] {
            let mut pawn = state.board[from.0][from.1].take().unwrap();
            assert_eq!(pawn.kind, "pawn");
            pawn.moved = true;
            assert!(state.board[to.0][to.1].is_none());
            state.board[to.0][to.1] = Some(pawn);
        }
        state.turns_taken.white = 1;
        state.turns_taken.black = 1;
        state.move_count = 2;
        state.full_move = 2;
        let card = state
            .deck_slots
            .white
            .iter()
            .find(|card| card.id == "grappler")
            .unwrap()
            .clone();
        let action = Action::card(
            Color::White,
            &card,
            Some(json!({"row":7,"col":3,"minor":{"row":7,"col":1}})),
        );
        let captures = crate::card_effects::apply(&mut state, &card, &action)
            .unwrap()
            .unwrap();
        assert_eq!(captures.len(), 1);
        assert!(state.board[7][1].is_none());
        let from = Square { row: 7, col: 3 };
        let grappler = state.at(from).unwrap();
        assert_eq!(grappler.id, "white-queen-1ou4c52pl2n");
        assert_eq!(grappler.kind, "grappler");
        assert_eq!(
            moves(&state, grappler, from),
            [
                Square { row: 6, col: 4 },
                Square { row: 5, col: 5 },
                Square { row: 4, col: 6 },
                Square { row: 3, col: 7 },
            ]
            .map(MoveTarget::at)
        );
    }

    #[test]
    fn v7_injured_don_quixote_uses_the_long_axis_jump_blocker() {
        let mut state = GameState::new(GameConfig::default(), 37).unwrap();
        let from = Square { row: 4, col: 4 };
        let don = Piece::new("donQuixote", Color::White, "don");
        state.board[4][4] = Some(don.clone());
        state.board[3][4] = Some(Piece::new("wall", PieceColor::Neutral, "wall"));
        state
            .extra
            .insert("knightInjury".into(), json!({"white":true,"black":false}));
        let destinations = |state: &GameState| {
            moves(state, &don, from)
                .into_iter()
                .map(|target| target.square())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            destinations(&state),
            vec![
                Square { row: 2, col: 3 },
                Square { row: 2, col: 5 },
                Square { row: 3, col: 2 },
                Square { row: 3, col: 6 },
                Square { row: 5, col: 2 },
                Square { row: 5, col: 6 },
            ]
        );
        state.ruleset_id = RULES_VERSION_V7.into();
        assert_eq!(
            destinations(&state),
            vec![
                Square { row: 3, col: 2 },
                Square { row: 3, col: 6 },
                Square { row: 5, col: 2 },
                Square { row: 5, col: 6 },
            ]
        );
    }

    #[test]
    fn source_quiet_and_jump_paths_preserve_blockers_and_identity() {
        for (kind, delta) in [
            ("reaper", (-1, -1)),
            ("octopus", (-1, -1)),
            ("paladin", (-2, 1)),
        ] {
            let (mut state, piece, from) = empty(kind);
            let target = from.offset(delta.0, delta.1).unwrap();
            state.extra.insert(
                "collapsedCells".into(),
                json!([{"row":target.row,"col":target.col}]),
            );
            assert!(
                moves(&state, &piece, from)
                    .iter()
                    .any(|to| to.square() == target)
            );
            state.extra.remove("collapsedCells");
            let mut hidden = Piece::new("rook", Color::Black, "hidden");
            hidden.extra.insert("submerged".into(), json!(true));
            hidden.extra.insert("hiddenFrom".into(), json!("white"));
            state.board[target.row as usize][target.col as usize] = Some(hidden);
            assert!(
                moves(&state, &piece, from)
                    .iter()
                    .any(|to| to.square() == target)
            );
            state.board[target.row as usize][target.col as usize]
                .as_mut()
                .unwrap()
                .extra
                .insert("hiddenFrom".into(), json!({"viewer":"white"}));
            assert!(
                !moves(&state, &piece, from)
                    .iter()
                    .any(|to| to.square() == target)
            );
        }
        let (mut state, mut herald, from) = empty("herald");
        herald.extra.insert("heraldJumpLockTurn".into(), json!("0"));
        state.board[from.row as usize][from.col as usize] = Some(herald.clone());
        let target = Square { row: 3, col: 3 };
        let beyond = Square { row: 2, col: 3 };
        put(&mut state, target, "rook", Color::Black);
        state.board[3][3]
            .as_mut()
            .unwrap()
            .extra
            .insert("hiddenFrom".into(), json!("white"));
        assert!(
            moves(&state, &herald, from)
                .iter()
                .any(|to| to.square() == target)
        );
        put(&mut state, target, "rook", Color::White);
        state.board[3][3]
            .as_mut()
            .unwrap()
            .extra
            .insert("ghost".into(), json!(true));
        let herald_moves = moves(&state, &herald, from);
        assert!(!herald_moves.iter().any(|to| to.square() == target));
        assert!(herald_moves.iter().any(|to| to.square() == beyond));
        let (mut state, wizard, from) = empty("wizard");
        let target = Square { row: 3, col: 3 };
        put(&mut state, target, "pawn", Color::Black);
        assert!(
            !moves(&state, &wizard, from)
                .iter()
                .any(|to| to.square() == target)
        );
        let pegasus = Piece::new("pegasus", Color::White, "variant");
        assert!(
            !moves(&state, &pegasus, from)
                .iter()
                .any(|to| to.square() == target)
        );
        let far = Square { row: 0, col: 0 };
        assert!(
            moves(&state, &pegasus, from)
                .iter()
                .any(|to| to.square() == far)
        );
        let (mut state, protestant, from) = empty("protestant");
        put(&mut state, Square { row: 3, col: 2 }, "wall", Color::White);
        assert!(
            moves(&state, &protestant, from)
                .iter()
                .any(|to| to.square() == Square { row: 2, col: 1 })
        );
        let (mut state, medium, from) = empty("medium");
        state
            .extra
            .insert("mediumMovement".into(), json!({"type":"rook"}));
        put(&mut state, Square { row: 4, col: 6 }, "pawn", Color::Black);
        let copied = moves(&state, &medium, from)
            .into_iter()
            .find(|to| to.square() == Square { row: 4, col: 6 })
            .unwrap();
        assert_eq!(copied.flags.get("capture"), Some(&json!(true)));
        state
            .extra
            .insert("mediumMovement".into(), json!({"type":"slime"}));
        assert!(
            moves(&state, &medium, from)
                .iter()
                .all(|to| !to.flag("slimeMove"))
        );
        let mut trickster = Piece::new("trickster", Color::White, "variant");
        trickster
            .extra
            .insert("tricksterMoveType".into(), json!("grappler"));
        let copied = moves(&state, &trickster, from)
            .into_iter()
            .find(|to| to.square() == Square { row: 4, col: 6 })
            .unwrap();
        assert!(
            copied.flag("tricksterMove") && copied.flag("capture") && !copied.flag("grapplePull")
        );
    }

    #[test]
    fn hook_bend_prime_minister_and_cardinal_are_finite_source_paths() {
        let (state, hook, from) = empty("hook");
        let bend = moves(&state, &hook, from)
            .into_iter()
            .find(|to| to.square() == Square { row: 2, col: 1 })
            .unwrap();
        assert_eq!(bend.flags.get("bent"), Some(&json!(true)));
        let (mut state, prime, from) = empty("primeMinister");
        for &(dr, dc) in KING {
            put(
                &mut state,
                from.offset(dr, dc).unwrap(),
                "wall",
                Color::White,
            );
        }
        assert!(moves(&state, &prime, from).is_empty());
        let (mut state, cardinal, from) = empty("cardinal");
        put(&mut state, Square { row: 3, col: 2 }, "wall", Color::White);
        let reflected = moves(&state, &cardinal, from);
        assert!(
            reflected.len() <= 64
                && reflected
                    .iter()
                    .all(|to| to.square() != Square { row: 3, col: 2 })
        );
    }

    #[test]
    fn thief_siege_ram_log_and_shotgun_keep_execution_flags() {
        let (mut state, thief, from) = empty("thief");
        put(&mut state, Square { row: 4, col: 4 }, "pawn", Color::Black);
        let jump = moves(&state, &thief, from)
            .into_iter()
            .find(|to| to.square() == Square { row: 4, col: 5 })
            .unwrap();
        assert_eq!(jump.flags.get("thiefQuietJump"), Some(&json!(true)));
        put(&mut state, Square { row: 4, col: 5 }, "pawn", Color::Black);
        assert!(
            !moves(&state, &thief, from)
                .iter()
                .any(|to| to.square() == Square { row: 4, col: 5 })
        );
        let siege = Piece::new("siegeRam", Color::White, "variant");
        let path = moves(&state, &siege, from)
            .into_iter()
            .find(|to| to.square() == Square { row: 4, col: 5 })
            .unwrap();
        assert_eq!(
            path.flags["highlightCells"],
            json!([{"row":4,"col":4},{"row":4,"col":5}])
        );
        let log = Piece::new("log", Color::White, "variant");
        assert_eq!(moves(&state, &log, from).len(), 8);
        let king = Piece::new("shotgunKing", Color::White, "variant");
        state.extra.insert("shotgunAction".into(), json!("shotgun"));
        let blasts = moves(&state, &king, from);
        assert_eq!(blasts.len(), 63);
        assert_eq!(blasts[0].flags["shotgunDirection"], json!([-1, -1]));
        let (mut state, grappler, from) = empty("grappler");
        let victim = Piece::new("colossus", Color::Black, "large-victim");
        for (row, col) in [(4, 6), (4, 7), (5, 6), (5, 7)] {
            state.board[row][col] = Some(victim.clone());
        }
        let pull = moves(&state, &grappler, from)
            .into_iter()
            .find(|to| to.square() == Square { row: 4, col: 6 })
            .unwrap();
        assert!(pull.flag("grapplePull"));
        assert_eq!(pull.flags.get("capture"), Some(&json!(false)));
        put(&mut state, Square { row: 5, col: 4 }, "pawn", Color::White);
        assert!(
            !moves(&state, &grappler, from)
                .iter()
                .any(|to| to.square() == Square { row: 4, col: 6 })
        );
        let (mut state, colossus, from) = empty("colossus");
        for (row, col) in [(4, 4), (5, 3), (5, 4)] {
            state.board[row][col] = Some(colossus.clone());
        }
        put(&mut state, Square { row: 3, col: 3 }, "pawn", Color::Black);
        put(&mut state, Square { row: 3, col: 4 }, "pawn", Color::Black);
        let footprint = moves(&state, &colossus, from)
            .into_iter()
            .find(|to| to.flag("colossusMove") && to.square() == Square { row: 3, col: 3 })
            .unwrap();
        assert_eq!(
            footprint.flags["colossusLandingCaptures"],
            json!([{"row":3,"col":3},{"row":3,"col":4}])
        );
        assert_eq!(
            footprint.flags["displayCells"],
            json!([{"row":3,"col":3},{"row":3,"col":4}])
        );
        let (mut state, mut merchant, from) = empty("merchant");
        merchant.extra.insert("gold".into(), json!(20));
        put(&mut state, Square { row: 0, col: 0 }, "king", Color::Black);
        state.board[0][0]
            .as_mut()
            .unwrap()
            .extra
            .insert("protected".into(), json!(true));
        assert_eq!(moves(&state, &merchant, from)[0].flags["cost"], json!(20));
        let (mut state, ball, from) = empty("football");
        put(&mut state, Square { row: 4, col: 2 }, "rook", Color::White);
        let kick = moves(&state, &ball, from)
            .into_iter()
            .find(|to| to.square() == Square { row: 4, col: 7 })
            .unwrap();
        assert_eq!(kick.flags["kicker"], json!({"row":4,"col":2}));
    }

    #[test]
    fn v7_siege_chameleon_path_preserves_source_candidate_and_first_victim() {
        // Frozen v7 P8 synthetic board: Ram (4,4), knight (4,5), bishop
        // (4,6). The source accepts the two-cell move and transforms the Ram
        // into the first successful path victim after both forced captures.
        let (mut state, mut ram, _) = empty("siegeRam");
        state.ruleset_id = RULES_VERSION_V7.into();
        state.board[4][3] = None;
        let from = Square { row: 4, col: 4 };
        ram.extra.insert("chameleon".into(), json!(true));
        state.board[4][4] = Some(ram.clone());
        let first = Square { row: 4, col: 5 };
        let landing = Square { row: 4, col: 6 };
        put(&mut state, first, "knight", Color::Black);
        put(&mut state, landing, "bishop", Color::Black);
        let target = base_moves(&state, &ram, from)
            .unwrap()
            .unwrap()
            .into_iter()
            .find(|target| target.square() == landing)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&target).unwrap(),
            json!({
                "row":4,"col":6,"siegeRamMove":true,
                "highlightCells":[{"row":4,"col":5},{"row":4,"col":6}]
            })
        );
        let path = SiegeRamPath::from_target(from, &target).unwrap().unwrap();
        assert_eq!(path.cells(), &[first, landing]);
        assert_eq!(path.potential_capture_count(&state), 2);
        assert!(
            crate::movement::piece_moves(&state, &ram, from)
                .unwrap()
                .contains(&target)
        );
        let captures = vec![
            (landing, state.at(landing).unwrap().clone()),
            (first, state.at(first).unwrap().clone()),
        ];
        assert_eq!(
            path.first_chameleon_victim(&state, &captures)
                .unwrap()
                .unwrap()
                .kind,
            "knight"
        );
        let mut royal_first = captures.clone();
        royal_first[1].1.kind = "king".into();
        assert_eq!(
            path.first_chameleon_victim(&state, &royal_first)
                .unwrap()
                .unwrap()
                .kind,
            "bishop"
        );
        let mut invalid = target.clone();
        invalid
            .flags
            .insert("highlightCells".into(), json!([landing]));
        assert!(matches!(
            SiegeRamPath::from_target(from, &invalid),
            Err(EngineError::IllegalAction)
        ));
        let first_id = state.at(first).unwrap().id.clone();
        state.board[4][6].as_mut().unwrap().id = first_id;
        assert_eq!(path.potential_capture_count(&state), 1);
    }

    #[test]
    fn owned_unimplemented_paths_fail_and_unowned_types_are_none() {
        let (mut state, piece, from) = empty("medium");
        assert!(moves(&state, &piece, from).is_empty());
        state.extra.insert(
            "mediumMovement".into(),
            json!({"type":"unknown-memory-kind"}),
        );
        assert!(matches!(
            base_moves(&state, &piece, from),
            Err(EngineError::UnsupportedFeature(_))
        ));
        let ordinary = Piece::new("rook", Color::White, "variant");
        assert!(base_moves(&state, &ordinary, from).unwrap().is_none());
        let hook = Piece::new("hook", Color::White, "variant");
        state.extra.insert(
            "portalRule".into(),
            json!({"cells":[{"row":1,"col":1},{"row":6,"col":6}]}),
        );
        assert!(matches!(
            base_moves(&state, &hook, from),
            Err(EngineError::UnsupportedFeature(_))
        ));
        state.extra.remove("portalRule");
        state
            .extra
            .insert("highGround".into(), json!([{"row":3,"col":3}]));
        let large = Piece::new("colossus", Color::White, "variant");
        assert!(matches!(
            base_moves(&state, &large, from),
            Err(EngineError::UnsupportedFeature(_))
        ));
    }
}
