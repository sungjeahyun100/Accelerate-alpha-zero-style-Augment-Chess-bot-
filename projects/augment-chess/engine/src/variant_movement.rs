//! Variant base movement originally ported from frozen main-CqkYwJX4.js
//! (abfe01a035813875). v7-specific branches are checked against
//! main-OahWs0tU.js (e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c).
//!
//! This module emits source coordinates and execution flags. Turn ownership,
//! protection, forced moves, portal routing, footprint execution and capture
//! effects belong to the shared legality/transition layer. There is no worker
//! strategy pruning. Loops are bounded by the 8x8 board or source bounce limit.

use crate::movement::{
    DIAG, KING, KNIGHT, ORTHO, can_capture, cannon, checker, collapsed, encouraged,
    expansion_destination_allowed, frozen, grasshopper, landing, leaps, missionary, pawn_moves,
    rays, v7_jump_leaps, v7_stealth_transparent,
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
    if state.ruleset_id == RULES_VERSION_V7 && piece.kind == "trickster" {
        let kind = v7_trickster_move_type(piece);
        let moves = if matches!(
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
        ) {
            if kind == "clockwork" && !has_neighbor(state, piece, from) {
                Vec::new()
            } else {
                let memory = if matches!(kind, "parrot" | "medium") {
                    memory_for(state, piece, kind)
                } else {
                    Some(json!({"type":kind}))
                };
                match memory {
                    Some(memory) => v7_memory_base_moves(
                        state,
                        piece,
                        from,
                        &memory,
                        V7MemoryMode::Rules,
                        false,
                        piece.moved,
                    )?,
                    None => Vec::new(),
                }
            }
        } else {
            v7_imperial_moves_for_type(state, piece, from, kind, state)?
        };
        return Ok(Some(flagged(moves, "tricksterMove")));
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
        "hedgehog" | "undead" | "vip" | "crown" => v7_jump_leaps(state, piece, from, KING),
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
            let deltas = knight_deltas_for_move(state, piece, from)?;
            v7_jump_leaps(state, piece, from, &deltas)
        }
        "recruiter" => v7_jump_leaps(state, piece, from, KING),
        "bear" => rays(state, piece, from, QUEEN, 7),
        "revolvingDoor" => rays(state, piece, from, ORTHO, 7),
        "babyBear" => Vec::new(),
        "wizard" => quiet(leaps(state, piece, from, KING), state),
        "idol" => quiet(rays(state, piece, from, QUEEN, 7), state),
        "darkWizard" => v7_jump_leaps(
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
            v7_jump_leaps(state, piece, from, &[(dir, -1), (dir, 0), (dir, 1)])
        }
        "slime" => flagged(
            leaps(state, piece, from, &[(-3, 0), (3, 0), (0, -3), (0, 3)]),
            "slimeMove",
        ),
        "siren" => flagged(v7_jump_leaps(state, piece, from, KING), "sirenMove"),
        "magicGirl" => {
            if state.flag("magicGirlSurge", piece.color) {
                let mut moves = rays(state, piece, from, QUEEN, 7);
                moves.extend(v7_jump_leaps(state, piece, from, KNIGHT));
                unique(moves)
            } else {
                v7_jump_leaps(state, piece, from, KING)
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
            if state.ruleset_id == RULES_VERSION_V7 {
                crate::v7_special_piece_moves::raw_prime_minister_moves(state, piece, from)?
            } else {
                reject_portal(state, kind)?;
                prime_minister(state, piece, from)
            }
        }
        "hook" | "brutus" => {
            if state.ruleset_id == RULES_VERSION_V7 {
                v7_hook_moves(state, piece, from, V7HookPolicy::Physical)?
            } else {
                reject_portal(state, kind)?;
                hook(state, piece, from)
            }
        }
        "cardinal" => {
            if state.ruleset_id != RULES_VERSION_V7 {
                reject_portal(state, kind)?;
            }
            cardinal(state, piece, from)
        }
        "protestant" => {
            if state.ruleset_id != RULES_VERSION_V7 {
                reject_portal(state, kind)?;
            }
            protestant(state, piece, from)
        }
        "fanatic" => {
            if state.ruleset_id != RULES_VERSION_V7 {
                reject_portal(state, kind)?;
            }
            if state.ruleset_id == RULES_VERSION_V7 {
                crate::movement::v7_raw_fanatic_moves(state, piece, from)?
            } else {
                rays(state, piece, from, &[(owner(piece)?.pawn_dir(), 0)], 2)
            }
        }
        "herald" => herald(state, piece, from),
        "log" => log(state, from),
        "thief" => thief(state, piece, from)?,
        "siegeRam" => {
            if state.ruleset_id == RULES_VERSION_V7 {
                v7_siege_ram_moves(state, from)
            } else {
                reject_portal(state, kind)?;
                siege_ram(from)
            }
        }
        "shotgunKing" => shotgun(state, piece, from)?,
        "bat" | "vampireLord" => {
            let night = state.ruleset_id == RULES_VERSION_V7
                && crate::v7_campaign::uses_blood_moon_night_movement(state, owner(piece)?)?;
            if kind == "bat" && night {
                crate::v7_special_piece_moves::raw_prime_minister_moves(state, piece, from)?
            } else if kind == "bat" {
                rays(state, piece, from, ORTHO, 2)
            } else if night {
                let mut moves = rays(state, piece, from, QUEEN, 7);
                moves.extend(v7_jump_leaps(
                    state,
                    piece,
                    from,
                    &knight_deltas_for_move(state, piece, from)?,
                ));
                unique(moves)
            } else {
                v7_jump_leaps(state, piece, from, KING)
            }
        }
        "timeTraveler" => {
            if state.ruleset_id == RULES_VERSION_V7 {
                v7_time_traveler_moves(state, piece, from)?
            } else {
                reject_campaign(state, kind, "timeTraveler")?;
                quiet(leaps(state, piece, from, KING), state)
            }
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

/// The v7 source applies knightInjury to every long-axis knight jump
/// before destination checks, including Don Quixote, Royal Knight and Amazon.
/// The intermediate blocker is transparent only for a concealed enemy or an
/// allied Ghost (source passes "rook" to its ranged transparency helper).
/// v6 keeps its frozen unconditioned deltas.
pub(crate) fn knight_deltas_for_move(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<(i8, i8)>> {
    if state.ruleset_id != RULES_VERSION_V7 || !state.flag("knightInjury", piece.color) {
        return Ok(KNIGHT.to_vec());
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
            jump.and_then(|at| state.at(at).map(|blocker| (at, blocker)))
                .is_none_or(|(at, blocker)| {
                    v7_stealth_transparent(state, piece, blocker, at)
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
                v7_stealth_transparent(state, piece, target, to)
                    || state.ruleset_id != RULES_VERSION_V7
                        && piece.color.owner().is_some_and(|actor| {
                            target.color != piece.color
                                && target.extra.get("hiddenFrom").and_then(Value::as_str)
                                    == Some(actor.as_str())
                        })
                    || can_capture(state, piece, target)
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

/// Source freeMovePopulationBoard affects Berserker's tier while occupancy
/// and knight-injury routing continue to use the relaxed simulation board.
pub(crate) fn v7_berserker_with_population(
    state: &GameState,
    piece: &Piece,
    from: Square,
    population: &GameState,
) -> Result<Vec<MoveTarget>> {
    let count = population
        .board
        .iter()
        .enumerate()
        .flat_map(|(row, line)| {
            line.iter().enumerate().filter_map(move |(col, cell)| {
                cell.as_ref()
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
        let mut moves = rays(state, piece, from, QUEEN, 7);
        moves.extend(v7_jump_leaps(
            state,
            piece,
            from,
            &knight_deltas_for_move(state, piece, from)?,
        ));
        Ok(unique(moves))
    } else if count <= 9 {
        let mut moves = v7_jump_leaps(state, piece, from, QUEEN);
        moves.extend(rays(state, piece, from, ORTHO, 7));
        Ok(unique(moves))
    } else {
        Ok(v7_jump_leaps(state, piece, from, QUEEN))
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum V7HookPolicy {
    Physical,
    Remembered,
    Geometry,
    Betrayal,
}

#[derive(Clone)]
struct V7HookTarget {
    target: MoveTarget,
    paths: std::collections::BTreeMap<String, usize>,
}

fn v7_hook_occupant<'a>(
    state: &'a GameState,
    piece: &Piece,
    at: Square,
    policy: V7HookPolicy,
) -> (Option<&'a Piece>, bool) {
    if policy == V7HookPolicy::Physical {
        let reserved = state
            .extra
            .get("pendingScarecrows")
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry.get("reserved").is_some_and(nonempty)
                        && entry.get("row").and_then(Value::as_f64) == Some(f64::from(at.row))
                        && entry.get("col").and_then(Value::as_f64) == Some(f64::from(at.col))
                })
            });
        if reserved {
            return (None, true);
        }
        (
            state
                .at(at)
                .filter(|target| !v7_stealth_transparent(state, piece, target, at)),
            false,
        )
    } else {
        (state.at(at), false)
    }
}

fn v7_hook_transparent(
    state: &GameState,
    piece: &Piece,
    target: &Piece,
    at: Square,
    bent: bool,
    policy: V7HookPolicy,
) -> bool {
    matches!(policy, V7HookPolicy::Physical | V7HookPolicy::Betrayal)
        && (crate::movement::v7_ghost_transparent_for(state, piece, target, at, "hook")
            || policy == V7HookPolicy::Physical
                && !bent
                && crate::movement::v7_time_phase_transparent_blocker(state, piece, target))
}

struct V7HookContext<'a> {
    state: &'a GameState,
    piece: &'a Piece,
    policy: V7HookPolicy,
    portal: bool,
}
struct V7HookWalk {
    next: Option<Square>,
    direction: (i8, i8),
    bent: bool,
    transit: Option<(Square, Square)>,
    path_key: String,
}

fn v7_hook_walk(
    context: &V7HookContext<'_>,
    walk: V7HookWalk,
    moves: &mut Vec<V7HookTarget>,
) -> Result<()> {
    let V7HookContext {
        state,
        piece,
        policy,
        portal,
    } = *context;
    let V7HookWalk {
        next,
        direction,
        bent,
        transit: initial_transit,
        path_key,
    } = walk;
    let (dr, dc) = direction;
    let mut next = next;
    let mut transit = initial_transit;
    // 한 번만 꺾고 포털도 한 번만 사용하므로 경로마다 두 유한 구간이다.
    for step in 1..=16 {
        let Some(at) = next else {
            break;
        };
        let (target, reserved) = v7_hook_occupant(state, piece, at, policy);
        let transparent = target
            .is_some_and(|target| v7_hook_transparent(state, piece, target, at, bent, policy));
        let exit = if portal {
            crate::movement::v7_portal_exit_at(state, at)
        } else {
            None
        };
        if exit.is_some() && transit.is_some() {
            break;
        }
        let mut descriptor = MoveTarget::at(at);
        if bent {
            descriptor.flags.insert("bent".into(), json!(true));
        }
        if let Some((entry, exit)) = transit {
            descriptor.flags.insert("portalThrough".into(), json!(true));
            descriptor.flags.insert("portalEntry".into(), json!(entry));
            descriptor.flags.insert("portalExit".into(), json!(exit));
        }
        if reserved {
            break;
        }
        if let Some(target) = target.filter(|_| !transparent) {
            let allowed = if policy == V7HookPolicy::Betrayal {
                target.id != piece.id
                    && target.color == piece.color
                    && state.royal_identity(target)
                    && crate::movement::v7_can_capture_target(state, piece, target, true, false)?
            } else if policy == V7HookPolicy::Geometry {
                v7_memory_capture(state, piece, target, V7MemoryMode::Geometry)
            } else {
                crate::movement::v7_can_capture_target(state, piece, target, false, false)?
            };
            if allowed {
                moves.push(V7HookTarget {
                    target: descriptor,
                    paths: [(path_key.clone(), step)].into(),
                });
            }
            break;
        }
        if transparent && exit.is_some() {
            descriptor
                .flags
                .insert("portalTransparentEntry".into(), json!(true));
            moves.push(V7HookTarget {
                target: descriptor,
                paths: [(path_key.clone(), step)].into(),
            });
        } else if target.is_none() {
            moves.push(V7HookTarget {
                target: descriptor,
                paths: [(path_key.clone(), step)].into(),
            });
        }
        if let Some(exit) = exit {
            if policy != V7HookPolicy::Geometry && (collapsed(state, at) || collapsed(state, exit))
            {
                break;
            }
            let (exit_target, exit_reserved) = v7_hook_occupant(state, piece, exit, policy);
            if exit_reserved
                || exit_target.is_some_and(|target| {
                    !v7_hook_transparent(state, piece, target, exit, bent, policy)
                })
            {
                break;
            }
            transit = Some((at, exit));
            if !bent && exit_target.is_none() {
                for &(turn_r, turn_c) in ORTHO {
                    if turn_r * dr + turn_c * dc != 0 {
                        continue;
                    }
                    let bend_key = format!(
                        "{path_key}|bend:{},{}:{turn_r},{turn_c}",
                        exit.row, exit.col
                    );
                    v7_hook_walk(
                        context,
                        V7HookWalk {
                            next: exit.offset(turn_r, turn_c),
                            direction: (turn_r, turn_c),
                            bent: true,
                            transit,
                            path_key: bend_key,
                        },
                        moves,
                    )?;
                }
            }
            next = exit.offset(dr, dc);
            continue;
        }
        if !bent && target.is_none() {
            for &(turn_r, turn_c) in ORTHO {
                if turn_r * dr + turn_c * dc != 0 {
                    continue;
                }
                let bend_key = format!("{path_key}|bend:{},{}:{turn_r},{turn_c}", at.row, at.col);
                v7_hook_walk(
                    context,
                    V7HookWalk {
                        next: at.offset(turn_r, turn_c),
                        direction: (turn_r, turn_c),
                        bent: true,
                        transit,
                        path_key: bend_key,
                    },
                    moves,
                )?;
            }
        }
        next = at.offset(dr, dc);
    }
    Ok(())
}

fn v7_hook_targets(
    state: &GameState,
    piece: &Piece,
    from: Square,
    policy: V7HookPolicy,
) -> Result<Vec<V7HookTarget>> {
    v7_hook_targets_with_portal(state, piece, from, policy, true)
}

fn v7_hook_targets_with_portal(
    state: &GameState,
    piece: &Piece,
    from: Square,
    policy: V7HookPolicy,
    portal: bool,
) -> Result<Vec<V7HookTarget>> {
    let mut raw = Vec::new();
    let context = V7HookContext {
        state,
        piece,
        policy,
        portal,
    };
    for &(dr, dc) in ORTHO {
        v7_hook_walk(
            &context,
            V7HookWalk {
                next: from.offset(dr, dc),
                direction: (dr, dc),
                bent: false,
                transit: None,
                path_key: format!("straight:{dr},{dc}"),
            },
            &mut raw,
        )?;
    }
    let mut moves: Vec<V7HookTarget> = Vec::new();
    for candidate in raw {
        if let Some(existing) = moves
            .iter_mut()
            .find(|entry| entry.target.square() == candidate.target.square())
        {
            for (path, step) in candidate.paths {
                existing
                    .paths
                    .entry(path)
                    .and_modify(|old| *old = (*old).max(step))
                    .or_insert(step);
            }
        } else {
            moves.push(candidate);
        }
    }
    Ok(moves)
}

fn v7_hook_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    policy: V7HookPolicy,
) -> Result<Vec<MoveTarget>> {
    Ok(v7_hook_targets(state, piece, from, policy)?
        .into_iter()
        .map(|entry| entry.target)
        .collect())
}

fn v7_ice_movement(target: &MoveTarget) -> bool {
    !target.flag("colossusBody")
        && !target.flags.get("castle").is_some_and(nonempty)
        && ![
            "colossusAttack",
            "shotgunBlast",
            "shotgunSnipe",
            "setLogDirection",
        ]
        .iter()
        .any(|flag| target.flag(flag))
}

fn v7_ice_direction(from: Square, target: &MoveTarget) -> Option<((i16, i16), i16)> {
    let integer = |field: &str, fallback: u8| {
        target
            .flags
            .get(field)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && value.fract() == 0.0)
            .unwrap_or(f64::from(fallback)) as i16
    };
    let dr = integer("anchorRow", target.row) - i16::from(from.row);
    let dc = integer("anchorCol", target.col) - i16::from(from.col);
    if dr == 0 && dc == 0 {
        return None;
    }
    let (mut a, mut b) = (dr.abs(), dc.abs());
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    Some(((dr / a.max(1), dc / a.max(1)), dr.abs().max(dc.abs())))
}

/// Source Hook의 WeakMap 정보는 원래 descriptor에만 붙는다. spread로
/// 복제된 Trickster/Imperial/Portal/왕관 descriptor에는 경로를 소급하지 않는다.
pub(crate) fn v7_filter_ice_sheet_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    moves: Vec<MoveTarget>,
) -> Result<Vec<MoveTarget>> {
    if matches!(piece.ability_kind(), "hook" | "brutus") {
        let raw = v7_hook_targets(state, piece, from, V7HookPolicy::Physical)?;
        let paths_for = |target: &MoveTarget| {
            raw.iter()
                .find(|entry| entry.target == *target)
                .map(|entry| &entry.paths)
        };
        let mut max_step = std::collections::BTreeMap::<String, usize>::new();
        for target in &moves {
            if !v7_ice_movement(target) {
                continue;
            }
            if let Some(paths) = paths_for(target) {
                for (path, step) in paths {
                    max_step
                        .entry(path.clone())
                        .and_modify(|old| *old = (*old).max(*step))
                        .or_insert(*step);
                }
            }
        }
        if max_step.is_empty() {
            return Ok(moves);
        }
        return Ok(moves
            .into_iter()
            .filter(|target| {
                !v7_ice_movement(target)
                    || paths_for(target).is_none_or(|paths| {
                        paths
                            .iter()
                            .any(|(path, step)| max_step.get(path) == Some(step))
                    })
            })
            .collect());
    }
    if piece.kind == "cardinal" {
        let terminal = DIAG
            .iter()
            .filter_map(|&(dr, dc)| v7_cardinal_terminal_move(state, piece, from, dr, dc))
            .map(|target| target.square())
            .collect::<BTreeSet<_>>();
        if !terminal.is_empty() {
            return Ok(moves
                .into_iter()
                .filter(|target| !v7_ice_movement(target) || terminal.contains(&target.square()))
                .collect());
        }
    }
    let mut max_distance = std::collections::BTreeMap::<(i16, i16), i16>::new();
    for target in &moves {
        if v7_ice_movement(target)
            && let Some((direction, distance)) = v7_ice_direction(from, target)
        {
            max_distance
                .entry(direction)
                .and_modify(|old| *old = (*old).max(distance))
                .or_insert(distance);
        }
    }
    Ok(moves
        .into_iter()
        .filter(|target| {
            !v7_ice_movement(target)
                || v7_ice_direction(from, target).is_none_or(|(direction, distance)| {
                    max_distance
                        .get(&direction)
                        .is_none_or(|max| distance >= *max)
                })
        })
        .collect())
}

fn v7_cardinal_terminal_move(
    state: &GameState,
    piece: &Piece,
    from: Square,
    mut dr: i8,
    mut dc: i8,
) -> Option<MoveTarget> {
    let mut cursor = from;
    let mut transit: Option<(Square, Square)> = None;
    let mut terminal = None;
    let mut seen = BTreeSet::new();
    for _ in 0..256 {
        if !seen.insert((cursor, dr, dc, transit.is_some())) {
            break;
        }
        let row = i16::from(cursor.row) + i16::from(dr);
        let col = i16::from(cursor.col) + i16::from(dc);
        if !(0..8).contains(&row) {
            dr = -dr;
        }
        if !(0..8).contains(&col) {
            dc = -dc;
        }
        let Some(at) = cursor.offset(dr, dc) else {
            break;
        };
        let target = state.at(at);
        let exit = crate::movement::v7_portal_exit_at(state, at);
        if exit.is_some() && transit.is_some() {
            break;
        }
        if target.is_some_and(|target| target.kind == "wall") {
            dr = -dr;
            dc = -dc;
            continue;
        }
        let mut descriptor = MoveTarget::at(at);
        if let Some((entry, exit)) = transit {
            descriptor.flags.insert("portalThrough".into(), json!(true));
            descriptor.flags.insert("portalEntry".into(), json!(entry));
            descriptor.flags.insert("portalExit".into(), json!(exit));
        }
        if let Some(target) = target {
            if crate::movement::v7_ghost_transparent_for(state, piece, target, at, "cardinal") {
                if let Some(exit) = exit {
                    descriptor
                        .flags
                        .insert("portalTransparentEntry".into(), json!(true));
                    terminal = Some(descriptor);
                    if !collapsed(state, at)
                        && !collapsed(state, exit)
                        && state.at(exit).is_none_or(|target| {
                            crate::movement::v7_ghost_transparent_for(
                                state, piece, target, exit, "cardinal",
                            )
                        })
                    {
                        transit = Some((at, exit));
                        cursor = exit;
                        continue;
                    }
                    break;
                }
                cursor = at;
                continue;
            }
            if can_capture(state, piece, target) {
                terminal = Some(descriptor);
            }
            break;
        }
        terminal = Some(descriptor);
        if let Some(exit) = exit {
            if !collapsed(state, at)
                && !collapsed(state, exit)
                && state.at(exit).is_none_or(|target| {
                    crate::movement::v7_ghost_transparent_for(
                        state, piece, target, exit, "cardinal",
                    )
                })
            {
                transit = Some((at, exit));
                cursor = exit;
                continue;
            }
            break;
        }
        cursor = at;
    }
    terminal
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

/// Raw parrotBaseMoves(memory Brutus) used by the automatic betrayal. Ghost
/// transparency is explicitly supplied by that source caller; royal-friendly
/// capture permission remains in the shared source capture predicate.
pub(crate) fn v7_brutus_base_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    v7_hook_moves(state, piece, from, V7HookPolicy::Betrayal)
}

// cardinalMoves97673: source bounces at board edges and walls, capped at 256.
fn cardinal(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(start_dr, start_dc) in DIAG {
        let (mut dr, mut dc) = (start_dr, start_dc);
        let mut cursor = from;
        let mut seen = BTreeSet::new();
        let mut transit: Option<(Square, Square)> = None;
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
            if !seen.insert((to, dr, dc, transit.is_some())) {
                break;
            }
            let exit = (state.ruleset_id == RULES_VERSION_V7)
                .then(|| crate::movement::v7_portal_exit_at(state, to))
                .flatten();
            if exit.is_some() && transit.is_some() {
                break;
            }
            let mut descriptor = MoveTarget::at(to);
            if let Some((entry, exit)) = transit {
                descriptor.flags.insert("portalThrough".into(), json!(true));
                descriptor.flags.insert("portalEntry".into(), json!(entry));
                descriptor.flags.insert("portalExit".into(), json!(exit));
            }
            if let Some(victim) = state.at(to) {
                if victim.kind == "wall" {
                    dr = -dr;
                    dc = -dc;
                    continue;
                }
                if state.ruleset_id == RULES_VERSION_V7
                    && crate::movement::v7_ghost_transparent_for(
                        state, piece, victim, to, "cardinal",
                    )
                {
                    if let Some(exit) = exit {
                        descriptor
                            .flags
                            .insert("portalTransparentEntry".into(), json!(true));
                        moves.push(descriptor);
                        if !collapsed(state, to)
                            && !collapsed(state, exit)
                            && state.at(exit).is_none_or(|victim| {
                                crate::movement::v7_ghost_transparent_for(
                                    state, piece, victim, exit, "cardinal",
                                )
                            })
                        {
                            transit = Some((to, exit));
                            cursor = exit;
                            continue;
                        }
                        break;
                    }
                    cursor = to;
                    continue;
                }
                if can_capture(state, piece, victim) {
                    moves.push(descriptor);
                }
                break;
            }
            moves.push(descriptor);
            if let Some(exit) = exit {
                if !collapsed(state, to)
                    && !collapsed(state, exit)
                    && state.at(exit).is_none_or(|victim| {
                        crate::movement::v7_ghost_transparent_for(
                            state, piece, victim, exit, "cardinal",
                        )
                    })
                {
                    transit = Some((to, exit));
                    cursor = exit;
                    continue;
                }
                break;
            }
            cursor = to;
        }
    }
    unique(moves)
}

fn protestant(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    if state.ruleset_id == RULES_VERSION_V7 {
        return v7_protestant_path_moves(state, piece, from, true);
    }
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

fn v7_protestant_path_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    allow_portal: bool,
) -> Vec<MoveTarget> {
    v7_protestant_kernel(state, piece, from, allow_portal, V7MemoryMode::Rules)
}

fn v7_protestant_kernel(
    state: &GameState,
    piece: &Piece,
    from: Square,
    allow_portal: bool,
    mode: V7MemoryMode,
) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(dr, dc) in DIAG {
        let mut cursor = from;
        let mut transit: Option<(Square, Square)> = None;
        for _ in 0..3 {
            let Some(at) = cursor.offset(dr, dc) else {
                break;
            };
            cursor = at;
            let victim = state.at(at);
            if victim.is_none_or(|victim| v7_memory_capture(state, piece, victim, mode)) {
                let mut target = MoveTarget::at(at);
                if let Some((entry, exit)) = transit {
                    target.flags.insert("portalThrough".into(), json!(true));
                    target.flags.insert("portalEntry".into(), json!(entry));
                    target.flags.insert("portalExit".into(), json!(exit));
                }
                moves.push(target);
            }
            if transit.is_some() || !allow_portal {
                continue;
            }
            if let Some(exit) = crate::movement::v7_portal_exit_at(state, at) {
                if victim.is_some()
                    || state.at(exit).is_some()
                    || collapsed(state, at)
                    || collapsed(state, exit)
                {
                    break;
                }
                transit = Some((at, exit));
                cursor = exit;
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
                    if v7_stealth_transparent(state, piece, target, to)
                        || state.ruleset_id != RULES_VERSION_V7
                            && target.color != piece.color
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
    if state.ruleset_id == RULES_VERSION_V7 {
        let (quiet_jump, required_jump) = v7_thief_jump_modes(state);
        let offsets: Vec<_> = [-3_i8, -2, -1, 1, 2, 3]
            .into_iter()
            .flat_map(|distance| [(distance, 0), (0, distance)])
            .collect();
        let mut moves = Vec::new();
        for mut target in source_jump_moves(state, piece, from, &offsets) {
            let screens = v7_thief_screen_ids(state, from, target.square());
            if quiet_jump && state.at(target.square()).is_some() && !screens.is_empty()
                || !quiet_jump && required_jump && screens.len() != 1
            {
                continue;
            }
            if quiet_jump && !screens.is_empty() {
                target.flags.insert("thiefQuietJump".into(), json!(true));
            }
            moves.push(target);
        }
        return Ok(moves);
    }
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
#[cfg(test)]
pub(crate) struct SiegeRamPath {
    cells: Vec<Square>,
}

#[cfg(test)]
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

pub(crate) fn v7_can_resolve_siege_ram_move(piece: &Piece, target: &MoveTarget) -> bool {
    target.flag("siegeRamMove")
        && (piece.kind == "siegeRam"
            || target.flags.get("imperialStudy").and_then(Value::as_str) == Some("siegeRam")
                && piece
                    .extra
                    .get("imperialMoves")
                    .and_then(Value::as_array)
                    .is_some_and(|types| {
                        types.iter().any(|kind| kind.as_str() == Some("siegeRam"))
                    })
            || piece.kind == "trickster"
                && piece.extra.get("tricksterMoveType").and_then(Value::as_str) == Some("siegeRam")
                && target.flags.get("tricksterMove") == Some(&json!(true)))
}

pub(crate) fn v7_siege_ram_moves(state: &GameState, from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(dr, dc) in ORTHO {
        let mut cursor = from;
        let mut transit: Option<(Square, Square)> = None;
        let mut cells = Vec::new();
        for _ in 0..2 {
            let Some(at) = cursor.offset(dr, dc) else {
                break;
            };
            cursor = at;
            if !cells.contains(&at) {
                cells.push(at);
            }
            let exit = if transit.is_none() {
                crate::movement::v7_portal_exit_at(state, at)
            } else {
                None
            };
            let mut target = MoveTarget::at(at);
            target.flags.insert("siegeRamMove".into(), json!(true));
            if let Some(exit) = exit {
                if collapsed(state, at) || collapsed(state, exit) {
                    break;
                }
                if !cells.contains(&exit) {
                    cells.push(exit);
                }
                transit = Some((at, exit));
                target.flags.insert("portalLanding".into(), json!(true));
                target.flags.insert("portalEntry".into(), json!(at));
                target.flags.insert("portalExit".into(), json!(exit));
                target.flags.insert("highlightCells".into(), json!(cells));
                moves.push(target);
                cursor = exit;
                continue;
            }
            if let Some((entry, exit)) = transit {
                target.flags.insert("portalThrough".into(), json!(true));
                target.flags.insert("portalEntry".into(), json!(entry));
                target.flags.insert("portalExit".into(), json!(exit));
            }
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum V7MemoryMode {
    Rules,
    Geometry,
}

pub(crate) fn v7_trickster_move_type(piece: &Piece) -> &str {
    match piece.ability_kind() {
        "trickster" => "queen",
        kind => kind,
    }
}

fn v7_memory_capture(state: &GameState, piece: &Piece, target: &Piece, mode: V7MemoryMode) -> bool {
    target.color != piece.color
        && if mode == V7MemoryMode::Geometry {
            !matches!(target.kind.as_str(), "wall" | "football" | "blackHole")
        } else {
            can_capture(state, piece, target)
        }
}

struct V7MemoryContext<'a> {
    state: &'a GameState,
    piece: &'a Piece,
    from: Square,
    mode: V7MemoryMode,
}
impl V7MemoryContext<'_> {
    fn add(
        &self,
        moves: &mut Vec<MoveTarget>,
        at: Option<Square>,
        quiet: bool,
        capture_only: bool,
        flags: Fields,
    ) {
        let Some(at) = at else {
            return;
        };
        let target = self.state.at(at);
        if target.is_some_and(|target| {
            quiet || !v7_memory_capture(self.state, self.piece, target, self.mode)
        }) || target.is_none() && capture_only
        {
            return;
        }
        let mut descriptor = MoveTarget::at(at);
        if target.is_some() {
            descriptor.flags.insert("capture".into(), json!(true));
        }
        descriptor.flags.extend(flags);
        moves.push(descriptor);
    }
    fn leap(
        &self,
        moves: &mut Vec<MoveTarget>,
        offsets: &[(i8, i8)],
        quiet: bool,
        capture_only: bool,
    ) {
        for &(dr, dc) in offsets {
            self.add(
                moves,
                self.from.offset(dr, dc),
                quiet,
                capture_only,
                Fields::new(),
            );
        }
    }
    fn ray(&self, moves: &mut Vec<MoveTarget>, directions: &[(i8, i8)], limit: i8, quiet: bool) {
        for &(dr, dc) in directions {
            let mut cursor = self.from;
            for _ in 0..limit {
                let Some(at) = cursor.offset(dr, dc) else {
                    break;
                };
                self.add(moves, Some(at), quiet, false, Fields::new());
                if self.state.at(at).is_some() {
                    break;
                }
                cursor = at;
            }
        }
    }
}

fn v7_memory_feature(state: &GameState, feature: &str, legacy: &str) -> bool {
    if let Some(hash) = v7_movement_catalog_hash(state) {
        let common = matches!(
            hash.as_str(),
            Some(
                "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4"
                    | "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI"
                    | "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs"
                    | "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc"
                    | "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g"
                    | "HVzv4vxzNKDRi-ylshTw9HnLmIlyiQhMScQ4caG_aSI"
                    | "jkECTP8OBtMeXmZF7wmz1dmgw0mgTFn8jdD-d9LZhxU"
                    | "s0_j9SmUy1tSUcko_X3I32akB93Iz2bvV1Cn7Nw4Uoo"
            )
        );
        return common
            || feature == "thiefQuietJump"
                && matches!(
                    hash.as_str(),
                    Some(
                        "_Hw8otJVztzWQyIN69bg5khIqcJv-au6UwAcOQhtKcM"
                            | "Vj80kM6RlfZvbMo9kevioRks2VbcDGk8bASi5uz0yNA"
                            | "MM-bvPG6PYiUQ0-UrCM3GmEbFfBMTyPxjKk0vFXbXj0"
                    )
                );
    }
    state.extra.get(feature) == Some(&json!(true))
        || !state.extra.contains_key(feature) && !state.extra.contains_key(legacy)
}

fn v7_movement_catalog_hash(state: &GameState) -> Option<&Value> {
    crate::movement::v7_effective_catalog_hash(state)
}

fn v7_thief_jump_modes(state: &GameState) -> (bool, bool) {
    let quiet = v7_memory_feature(state, "thiefQuietJump", "thiefRequiredJump");
    let required = v7_movement_catalog_hash(state).map_or_else(
        || state.extra.get("thiefRequiredJump") == Some(&json!(true)),
        |hash| hash.as_str() == Some("abhSgcd4RVrr2b-bzPtaJo6gbqfUrYKk4IqsrW7oCO0"),
    );
    (quiet, required)
}

fn v7_thief_screen_ids(state: &GameState, from: Square, to: Square) -> BTreeSet<String> {
    let dr = (i16::from(to.row) - i16::from(from.row)).signum() as i8;
    let dc = (i16::from(to.col) - i16::from(from.col)).signum() as i8;
    let distance = from.row.abs_diff(to.row).max(from.col.abs_diff(to.col));
    (1..distance)
        .filter_map(|step| from.offset(dr * step as i8, dc * step as i8))
        .filter_map(|at| state.at(at))
        .map(
            |piece| match piece.extra.get("instanceId").filter(|v| nonempty(v)) {
                Some(value @ (Value::Bool(_) | Value::Number(_) | Value::String(_))) => {
                    format!("instance:{value}")
                }
                _ => format!("piece:{}", piece.id),
            },
        )
        .collect()
}

/// 원문 parrotBaseMoves의 literal occupancy와 source 순서를 공유한다.
/// Geometry는 Double Check의 순수 포획 술어이며 일반 포획 잠금을 읽지 않는다.
fn v7_memory_base_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    memory: &Value,
    mode: V7MemoryMode,
    portal: bool,
    moved: bool,
) -> Result<Vec<MoveTarget>> {
    let Some(raw) = memory
        .get("type")
        .and_then(Value::as_str)
        .filter(|kind| !kind.is_empty())
    else {
        return Ok(Vec::new());
    };
    let kind = canonical_memory_type(raw);
    if matches!(kind.as_str(), "hook" | "brutus") {
        let policy = if mode == V7MemoryMode::Geometry {
            V7HookPolicy::Geometry
        } else {
            V7HookPolicy::Remembered
        };
        return Ok(
            v7_hook_targets_with_portal(state, piece, from, policy, portal)?
                .into_iter()
                .map(|entry| entry.target)
                .collect(),
        );
    }
    if kind == "protestant" {
        return Ok(v7_protestant_kernel(state, piece, from, false, mode));
    }
    if kind == "cannon" {
        return Ok(crate::movement::v7_memory_cannon_moves(
            state,
            piece,
            from,
            mode == V7MemoryMode::Geometry,
            portal,
        ));
    }
    let ctx = V7MemoryContext {
        state,
        piece,
        from,
        mode,
    };
    let dir = owner(piece)?.pawn_dir();
    let mut moves = Vec::new();
    match kind.as_str() {
        "pawn" | "squire" | "standardBearer" => {
            if from.offset(dir, 0).is_none_or(|at| state.at(at).is_none()) {
                ctx.add(&mut moves, from.offset(dir, 0), false, false, Fields::new());
                if !moved
                    && from.row == if piece.color == Color::White { 6 } else { 1 }
                    && from
                        .offset(dir * 2, 0)
                        .is_some_and(|at| state.at(at).is_none())
                {
                    ctx.add(
                        &mut moves,
                        from.offset(dir * 2, 0),
                        false,
                        false,
                        Fields::new(),
                    );
                }
            }
            ctx.leap(&mut moves, &[(dir, -1), (dir, 1)], false, true);
        }
        "queen" | "bear" | "clockwork" | "grappler" => ctx.ray(&mut moves, MEMORY_KING, 32, false),
        "rook" | "bigRook" | "revolvingDoor" => ctx.ray(&mut moves, ORTHO, 32, false),
        "bishop" | "bigBishop" => ctx.ray(&mut moves, DIAG, 32, false),
        "knight" | "unicorn" | "donQuixote" => ctx.leap(&mut moves, KNIGHT, false, false),
        "dragon" => {
            ctx.leap(&mut moves, KNIGHT, false, false);
            for row in 0..8 {
                for col in 0..8 {
                    let at = Square { row, col };
                    if at != from
                        && state.at(at).is_some_and(|target| {
                            target.color == piece.color
                                && !matches!(
                                    target.kind.as_str(),
                                    "slime" | "wall" | "colossus" | "bigRook" | "bigBishop"
                                )
                        })
                    {
                        let mut target = MoveTarget::at(at);
                        target.flags.insert("dragonSwap".into(), json!(true));
                        moves.push(target);
                    }
                }
            }
        }
        "magicGirl" => {
            let source = state.extra.get("cardState").filter(|v| nonempty(v));
            let effects = source.map_or_else(|| state.extra.get("effects"), |v| v.get("effects"));
            let surge = state.flag("magicGirlSurge", piece.color)
                || effects
                    .and_then(|v| v.get("colors"))
                    .and_then(|v| v.get(piece.color.as_str()))
                    .and_then(Value::as_array)
                    .is_some_and(|effects| {
                        effects.iter().any(|effect| {
                            effect.get("kind").and_then(Value::as_str) == Some("magic-girl-surge")
                        })
                    });
            if surge {
                ctx.ray(&mut moves, MEMORY_KING, 32, false);
                ctx.leap(&mut moves, KNIGHT, false, false);
            } else {
                ctx.leap(&mut moves, MEMORY_KING, false, false);
            }
        }
        "berserker" => {
            let count = state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|p| p.color == piece.color)
                .map(|p| &p.id)
                .collect::<BTreeSet<_>>()
                .len();
            if count <= 5 {
                ctx.ray(&mut moves, MEMORY_KING, 32, false);
                ctx.leap(&mut moves, KNIGHT, false, false);
            } else {
                ctx.leap(&mut moves, MEMORY_KING, false, false);
                if count <= 9 {
                    ctx.ray(&mut moves, ORTHO, 32, false);
                }
            }
        }
        "assassin" => {
            ctx.leap(&mut moves, KNIGHT, false, false);
            for &(dr, dc) in MEMORY_KING {
                let mut cursor = from;
                for _ in 0..7 {
                    let Some(at) = cursor.offset(dr, dc) else {
                        break;
                    };
                    if let Some(target) = state.at(at) {
                        if target.color != piece.color && state.royal_identity(target) {
                            ctx.add(&mut moves, Some(at), false, true, Fields::new());
                        }
                        break;
                    }
                    cursor = at;
                }
            }
        }
        "paladin" => ctx.leap(&mut moves, KNIGHT, true, false),
        "royalKnight" => {
            ctx.leap(&mut moves, KNIGHT, false, false);
            if !v7_memory_feature(state, "parrotBasicMovement", "cannonGhostScreen") {
                ctx.leap(&mut moves, MEMORY_KING, false, false);
            }
        }
        "king" | "octopus" | "man" | "guard" | "reaper" | "recruiter" | "vip" | "crown"
        | "darkWizard" | "undead" | "hedgehog" | "vampireLord" | "siren" | "log" | "monster" => {
            ctx.leap(&mut moves, MEMORY_KING, false, false)
        }
        "ferz" | "knightmaster" | "princess" => ctx.leap(&mut moves, DIAG, false, false),
        "amazon" => {
            ctx.ray(&mut moves, MEMORY_KING, 32, false);
            ctx.leap(&mut moves, KNIGHT, false, false);
        }
        "cardinal" => {
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
                    let Some(at) = cursor.offset(dr, dc) else {
                        break;
                    };
                    if at == from || !seen.insert((at, dr, dc)) {
                        break;
                    }
                    ctx.add(&mut moves, Some(at), false, false, Fields::new());
                    if state.at(at).is_some() {
                        break;
                    }
                    cursor = at;
                }
            }
        }
        "camel" => ctx.leap(
            &mut moves,
            &KNIGHT
                .iter()
                .map(|&(r, c)| {
                    (
                        if r.abs() == 2 { r.signum() * 3 } else { r },
                        if c.abs() == 2 { c.signum() * 3 } else { c },
                    )
                })
                .collect::<Vec<_>>(),
            false,
            false,
        ),
        "alfil" => ctx.leap(
            &mut moves,
            &DIAG
                .iter()
                .map(|&(r, c)| (r * 2, c * 2))
                .collect::<Vec<_>>(),
            false,
            false,
        ),
        "eagle" | "alibaba" => {
            ctx.leap(
                &mut moves,
                &DIAG
                    .iter()
                    .map(|&(r, c)| (r * 2, c * 2))
                    .collect::<Vec<_>>(),
                false,
                false,
            );
            ctx.leap(
                &mut moves,
                &ORTHO
                    .iter()
                    .map(|&(r, c)| (r * 2, c * 2))
                    .collect::<Vec<_>>(),
                false,
                false,
            );
        }
        "thief" => {
            let (quiet_jump, required) = v7_thief_jump_modes(state);
            for n in 1..=3 {
                for &(dr, dc) in ORTHO {
                    let Some(at) = from.offset(dr * n, dc * n) else {
                        continue;
                    };
                    let screens = v7_thief_screen_ids(state, from, at);
                    if quiet_jump && state.at(at).is_some() && !screens.is_empty()
                        || !quiet_jump && required && screens.len() != 1
                    {
                        continue;
                    }
                    let mut flags = Fields::new();
                    if quiet_jump && !screens.is_empty() {
                        flags.insert("thiefQuietJump".into(), json!(true));
                    }
                    ctx.add(&mut moves, Some(at), false, false, flags);
                }
            }
        }
        "grasshopper" => {
            for &(dr, dc) in MEMORY_KING {
                let mut cursor = from;
                for _ in 0..7 {
                    let Some(at) = cursor.offset(dr, dc) else {
                        break;
                    };
                    if state.at(at).is_some() {
                        ctx.add(&mut moves, at.offset(dr, dc), false, false, Fields::new());
                        break;
                    }
                    cursor = at;
                }
            }
        }
        "jester" | "idol" => ctx.ray(&mut moves, MEMORY_KING, 32, true),
        "lobster" => ctx.leap(&mut moves, &[(dir, -1), (dir, 0), (dir, 1)], false, false),
        "fanatic" => {
            ctx.leap(&mut moves, &[(dir, -1), (dir, 1)], true, false);
            ctx.leap(&mut moves, &[(dir, 0)], false, true);
        }
        "bat" => ctx.ray(&mut moves, ORTHO, 2, false),
        "slime" => ctx.leap(
            &mut moves,
            &ORTHO
                .iter()
                .map(|&(r, c)| (r * 3, c * 3))
                .collect::<Vec<_>>(),
            false,
            false,
        ),
        "colossus" => ctx.leap(&mut moves, ORTHO, false, false),
        "siegeRam" => {
            for n in 1..=2 {
                ctx.leap(
                    &mut moves,
                    &ORTHO
                        .iter()
                        .map(|&(r, c)| (r * n, c * n))
                        .collect::<Vec<_>>(),
                    false,
                    false,
                );
            }
        }
        "wizard" | "shotgunKing" | "timeTraveler" => ctx.leap(&mut moves, MEMORY_KING, true, false),
        "campfire" => ctx.leap(&mut moves, ORTHO, true, false),
        "missionary" => ctx.leap(&mut moves, DIAG, true, false),
        "herald" => {
            for n in 1..=3 {
                ctx.leap(
                    &mut moves,
                    &ORTHO
                        .iter()
                        .map(|&(r, c)| (r * n, c * n))
                        .collect::<Vec<_>>(),
                    true,
                    false,
                );
            }
        }
        "pegasus" => {
            for row in 0..8 {
                for col in 0..8 {
                    let at = Square { row, col };
                    if at != from {
                        ctx.add(&mut moves, Some(at), true, false, Fields::new());
                    }
                }
            }
            ctx.leap(&mut moves, KNIGHT, false, false);
        }
        "primeMinister" => {
            for &(dr, dc) in MEMORY_KING {
                let Some(mid) = from.offset(dr, dc) else {
                    continue;
                };
                ctx.add(&mut moves, Some(mid), false, false, Fields::new());
                if state.at(mid).is_none() {
                    for &(r, c) in MEMORY_KING {
                        let at = mid.offset(r, c);
                        if at != Some(from) {
                            ctx.add(&mut moves, at, false, false, Fields::new());
                        }
                    }
                }
            }
        }
        "checker" | "checkerKing" => {
            let dirs = if kind == "checkerKing" {
                DIAG.to_vec()
            } else {
                vec![(dir, -1), (dir, 1)]
            };
            ctx.leap(&mut moves, &dirs, true, false);
            for &(dr, dc) in &dirs {
                let Some(mid) = from.offset(dr, dc) else {
                    continue;
                };
                if state
                    .at(mid)
                    .is_some_and(|p| v7_memory_capture(state, piece, p, mode))
                {
                    ctx.add(
                        &mut moves,
                        from.offset(dr * 2, dc * 2),
                        true,
                        false,
                        [("jumpCapture".into(), json!(mid))].into_iter().collect(),
                    );
                }
            }
        }
        "windmill" => ctx.ray(
            &mut moves,
            if memory.get("windmillMode").and_then(Value::as_str) == Some("rook") {
                ORTHO
            } else {
                DIAG
            },
            32,
            false,
        ),
        "babyBear" | "coffin" | "scarecrow" | "wall" | "merchant" | "medium" | "parrot"
        | "football" | "blackHole" | "bomb" | "platform" | "portal" | "trickster" => {}
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "v7 remembered base movement {kind}"
            )));
        }
    }
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

/// imperialStudyMovesForType96077는 실제 공격자의 속성으로 각 raw 행마를
/// 질의한다. 트릭스터도 동일한 switch를 사용하며 타입별 복제본을 만들지 않는다.
fn v7_imperial_moves_for_type(
    state: &GameState,
    piece: &Piece,
    from: Square,
    raw: &str,
    population: &GameState,
) -> Result<Vec<MoveTarget>> {
    let kind = if crate::movement::v7_uses_revolving_door_guard(state) {
        match raw {
            "grappler" => "queen",
            "revolvingDoor" => "rook",
            "donQuixote" => "knight",
            kind => kind,
        }
    } else {
        raw
    };
    Ok(match kind {
        "paladin" | "brutus" | "clockwork" | "octopus" | "thief" => v7_memory_base_moves(
            state,
            piece,
            from,
            &json!({"type":kind}),
            V7MemoryMode::Rules,
            false,
            true,
        )?,
        "pawn" | "squire" | "standardBearer" => {
            let mut virtual_piece = piece.clone();
            virtual_piece.kind = kind.into();
            pawn_moves(state, &virtual_piece, from)
        }
        "checker" | "checkerKing" => {
            crate::movement::v7_checker_moves_for_type(state, piece, from, kind)
        }
        "fanatic" => crate::movement::v7_raw_fanatic_moves(state, piece, from)?,
        "knight" => crate::movement::v7_raw_knight_moves(state, piece, from, false)?,
        "knightmaster" | "ferz" => v7_jump_leaps(state, piece, from, DIAG),
        "assassin" => assassin(state, piece, from, QUEEN),
        "man" | "guard" | "reaper" | "recruiter" | "king" | "undead" | "vip" | "crown"
        | "darkWizard" | "hedgehog" => v7_jump_leaps(state, piece, from, KING),
        "camel" => v7_jump_leaps(
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
        "alfil" => v7_jump_leaps(state, piece, from, &[(-2, -2), (-2, 2), (2, -2), (2, 2)]),
        "eagle" | "alibaba" => v7_jump_leaps(
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
        "bat" | "vampireLord" => {
            let night = crate::v7_campaign::uses_blood_moon_night_movement(state, owner(piece)?)?;
            if kind == "bat" && night {
                crate::v7_special_piece_moves::raw_prime_minister_moves(state, piece, from)?
            } else if kind == "bat" {
                rays(state, piece, from, ORTHO, 2)
            } else if night {
                let mut moves = rays(state, piece, from, QUEEN, 7);
                moves.extend(v7_jump_leaps(
                    state,
                    piece,
                    from,
                    &knight_deltas_for_move(state, piece, from)?,
                ));
                unique(moves)
            } else {
                v7_jump_leaps(state, piece, from, KING)
            }
        }
        "grasshopper" => source_ray_order(grasshopper(state, piece, from), from, QUEEN),
        "bishop" => rays(state, piece, from, DIAG, 7),
        "rook" => rays(state, piece, from, ORTHO, 7),
        "queen" | "bear" => rays(state, piece, from, QUEEN, 7),
        "missionary" => missionary(state, piece, from),
        "cardinal" => cardinal(state, piece, from),
        "protestant" => v7_protestant_path_moves(state, piece, from, true),
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
        "hook" => v7_hook_moves(state, piece, from, V7HookPolicy::Physical)?,
        "herald" => herald(state, piece, from),
        "cannon" => crate::movement::v7_raw_cannon_moves(state, piece, from)?,
        "magicGirl" => {
            if state.flag("magicGirlSurge", piece.color) {
                let mut moves = rays(state, piece, from, QUEEN, 7);
                moves.extend(v7_jump_leaps(
                    state,
                    piece,
                    from,
                    &knight_deltas_for_move(state, piece, from)?,
                ));
                unique(moves)
            } else {
                v7_jump_leaps(state, piece, from, KING)
            }
        }
        "royalKnight" => {
            let mut moves = v7_jump_leaps(
                state,
                piece,
                from,
                &knight_deltas_for_move(state, piece, from)?,
            );
            moves.extend(v7_jump_leaps(state, piece, from, KING));
            unique(moves)
        }
        "siegeRam" => v7_siege_ram_moves(state, from),
        "berserker" => v7_berserker_with_population(state, piece, from, population)?,
        "slime" => flagged(
            v7_jump_leaps(state, piece, from, &[(-3, 0), (3, 0), (0, -3), (0, 3)]),
            "slimeMove",
        ),
        "siren" => flagged(v7_jump_leaps(state, piece, from, KING), "sirenMove"),
        "amazon" => {
            let mut moves = rays(state, piece, from, QUEEN, 7);
            moves.extend(v7_jump_leaps(
                state,
                piece,
                from,
                &knight_deltas_for_move(state, piece, from)?,
            ));
            unique(moves)
        }
        "pegasus" => pegasus(state, piece, from),
        "dragon" => dragon(state, piece, from),
        "jester" => rays(state, piece, from, QUEEN, 7)
            .into_iter()
            .filter(|m| {
                state.at(m.square()).is_none_or(|p| {
                    p.color != piece.color
                        && p.kind != "jester"
                        && (state.royal_identity(p) || p.kind == "merchant")
                })
            })
            .collect(),
        "primeMinister" => {
            crate::v7_special_piece_moves::raw_prime_minister_moves(state, piece, from)?
        }
        "wizard" => quiet(v7_jump_leaps(state, piece, from, KING), state),
        "idol" => quiet(rays(state, piece, from, QUEEN, 7), state),
        "lobster" => {
            let dir = owner(piece)?.pawn_dir();
            v7_jump_leaps(state, piece, from, &[(dir, -1), (dir, 0), (dir, 1)])
        }
        "princess" => {
            if population
                .board
                .iter()
                .flatten()
                .flatten()
                .any(|p| p.color == piece.color && p.kind == "queen")
            {
                v7_jump_leaps(state, piece, from, DIAG)
            } else {
                rays(state, piece, from, QUEEN, 7)
            }
        }
        "campfire" => quiet(v7_jump_leaps(state, piece, from, KING), state)
            .into_iter()
            .filter(|m| m.row == from.row || m.col == from.col)
            .collect(),
        "shotgunKing" => {
            let mut virtual_piece = piece.clone();
            virtual_piece.kind = "shotgunKing".into();
            virtual_piece.extra.insert("ammo".into(), json!(1));
            shotgun(state, &virtual_piece, from)?
        }
        "timeTraveler" => {
            let mut virtual_piece = piece.clone();
            virtual_piece.kind = "timeTraveler".into();
            v7_time_traveler_moves(state, &virtual_piece, from)?
        }
        "merchant" => merchant(state, piece)?,
        "log" => log(state, from),
        // 원문 switch의 default는 제왕학에 없는 알려진 stationary 타입에 한한다.
        "babyBear" | "coffin" | "scarecrow" | "wall" | "football" | "blackHole" | "monster"
        | "bomb" | "platform" | "portal" | "medium" | "parrot" | "unicorn" | "trickster"
        | "grappler" | "revolvingDoor" | "donQuixote" => Vec::new(),
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "v7 imperial movement type {kind}"
            )));
        }
    })
}

pub(crate) fn v7_imperial_study_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
    population: &GameState,
) -> Result<Vec<MoveTarget>> {
    let Some(types) = piece.extra.get("imperialMoves").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut seen = BTreeSet::new();
    let mut moves = Vec::new();
    for value in types {
        let kind = match value {
            Value::String(s) => s.clone(),
            Value::Null => "null".into(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            _ => {
                return Err(EngineError::InvalidState(
                    "v7 imperialMoves entries must be source movement names".into(),
                ));
            }
        };
        if matches!(
            kind.as_str(),
            "bigBishop"
                | ""
                | "king"
                | "royalKnight"
                | "shotgunKing"
                | "merchant"
                | "recruiter"
                | "wall"
                | "scarecrow"
                | "football"
                | "blackHole"
                | "colossus"
                | "bigRook"
                | "coffin"
                | "log"
                | "timeTraveler"
                | "wizard"
        ) || !seen.insert(kind.clone())
        {
            continue;
        }
        for mut target in v7_imperial_moves_for_type(state, piece, from, &kind, population)? {
            target.flags.insert("imperialStudy".into(), json!(kind));
            moves.push(target);
        }
    }
    Ok(unique(moves))
}

fn v7_time_traveler_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    let enabled = crate::v7_campaign::time_traveler_attack_enabled_for(state, owner(piece)?)?;
    Ok(KING
        .iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .filter(|&at| {
            state.at(at).is_none_or(|p| {
                !crate::movement::v7_time_phase_transparent_blocker(state, piece, p)
                    && enabled
                    && can_capture(state, piece, p)
            })
        })
        .map(MoveTarget::at)
        .collect())
}

/// Source doubleCheckGeometry deliberately precedes capture locks and the
/// ordinary legal-movement pipeline. Its capture predicate depends only on
/// allegiance and physical blockers; Frozen, Disarm and Saturation cannot
/// erase a geometry witness. No production capture predicate is reused here.
pub(crate) fn v7_double_check_geometry(
    state: &GameState,
    piece: &Piece,
    from: Square,
    royal: Square,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7
        || from.row >= 8
        || from.col >= 8
        || royal.row >= 8
        || royal.col >= 8
        || state.board.len() != 8
        || state.board.iter().any(|row| row.len() != 8)
    {
        return Err(EngineError::InvalidState(
            "v7 Double Check geometry requires an 8x8 Position".into(),
        ));
    }
    if from == royal {
        return Ok(false);
    }
    if piece.ability_kind() == "grappler"
        && from
            .row
            .abs_diff(royal.row)
            .max(from.col.abs_diff(royal.col))
            > 1
    {
        return Ok(false);
    }
    let source = state.extra.get("cardState").filter(|v| nonempty(v));
    let effects = source.map_or_else(|| state.extra.get("effects"), |v| v.get("effects"));
    let effects = effects
        .and_then(|v| v.get("colors"))
        .and_then(|v| v.get(piece.color.as_str()))
        .and_then(Value::as_array);
    let mut memory = if piece.ability_kind() == "parrot" {
        state
            .extra
            .get("parrotMovement")
            .and_then(|v| v.get(piece.color.as_str()))
            .filter(|v| nonempty(v))
            .cloned()
            .or_else(|| {
                effects
                    .and_then(|effects| {
                        effects.iter().find(|e| {
                            e.get("kind").and_then(Value::as_str) == Some("internal-last-movement")
                        })
                    })
                    .and_then(|e| e.get("attributes"))
                    .and_then(|a| a.get("movement"))
                    .cloned()
            })
    } else {
        crate::card_effects::current_base_movement(state, piece)
    };
    let Some(mut memory) = memory.take() else {
        return Ok(false);
    };
    let Some(kind) = memory
        .get("type")
        .and_then(Value::as_str)
        .map(canonical_memory_type)
    else {
        return Ok(false);
    };
    if kind == "revolvingDoor" && crate::movement::v7_uses_revolving_door_guard(state) {
        return Ok(false);
    }
    if matches!(
        kind.as_str(),
        "guard"
            | "recruiter"
            | "paladin"
            | "idol"
            | "jester"
            | "campfire"
            | "herald"
            | "merchant"
            | "log"
            | "babyBear"
            | "coffin"
            | "wall"
            | "football"
            | "blackHole"
    ) {
        return Ok(false);
    }
    if kind == "missionary" {
        return Ok(from.row.abs_diff(royal.row) == 1 && from.col.abs_diff(royal.col) == 1);
    }
    let reversal = state.flag("reversal", piece.color)
        || effects.is_some_and(|effects| {
            effects
                .iter()
                .any(|e| e.get("kind").and_then(Value::as_str) == Some("reversal"))
        });
    if reversal && matches!(kind.as_str(), "rook" | "bishop") {
        memory["type"] = json!(if kind == "rook" { "bishop" } else { "rook" });
    }
    let moves = v7_memory_base_moves(
        state,
        piece,
        from,
        &memory,
        V7MemoryMode::Geometry,
        true,
        piece.moved,
    )?;
    Ok(moves.iter().any(|m| {
        !m.flag("dragonSwap")
            && match m.flags.get("jumpCapture") {
                Some(value) if nonempty(value) => {
                    serde_json::from_value::<Square>(value.clone()).ok() == Some(royal)
                }
                _ => m.square() == royal,
            }
    }))
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
    if state.ruleset_id == RULES_VERSION_V7 {
        return v7_memory_base_moves(
            state,
            piece,
            from,
            memory,
            V7MemoryMode::Rules,
            true,
            piece.moved,
        );
    }
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

#[derive(Clone, Debug)]
pub(crate) struct V7GrapplerPullPlan {
    pub target: Piece,
    pub target_from: Square,
    pub source_cells: Vec<Square>,
    pub cells: Vec<Square>,
    pub to: Square,
    pub until_color: Color,
}

/// localGrapplerPullPlan의 원래 클릭 좌표와 실제 기물의 원점을 구분한다.
/// 다중 셀 대상의 이동량은 클릭한 셀에서 계산하고 모든 셀에 적용한다.
pub(crate) fn v7_grappler_pull_plan(
    state: &GameState,
    from: Square,
    clicked: Square,
) -> Result<Option<V7GrapplerPullPlan>> {
    if [from, clicked].iter().any(|at| at.row >= 8 || at.col >= 8) {
        return Err(EngineError::InvalidState(
            "v7 Grappler origin or target outside 8x8".into(),
        ));
    }
    let Some(attacker) = state.at(from) else {
        return Ok(None);
    };
    let Some(target) = state.at(clicked) else {
        return Ok(None);
    };
    let dr = i16::from(clicked.row) - i16::from(from.row);
    let dc = i16::from(clicked.col) - i16::from(from.col);
    if crate::observation::truth(attacker.extra.get("grapplerBound"))
        || attacker.color == target.color
        || dr.abs().max(dc.abs()) < 2
        || dr != 0 && dc != 0 && dr.abs() != dc.abs()
        || !crate::movement::v7_can_capture_target(state, attacker, target, false, false)?
    {
        return Ok(None);
    }
    let (step_r, step_c) = (dr.signum() as i8, dc.signum() as i8);
    let Some(destination) = from.offset(step_r, step_c) else {
        return Ok(None);
    };
    let mut cursor = destination;
    for _ in 0..7 {
        if cursor == clicked {
            break;
        }
        if state.at(cursor).is_some() {
            return Ok(None);
        }
        let Some(next) = cursor.offset(step_r, step_c) else {
            return Ok(None);
        };
        cursor = next;
    }
    if cursor != clicked {
        return Ok(None);
    }
    let mut source_cells = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            if state.at(at).is_some_and(|occupant| {
                at == clicked || !target.id.is_empty() && occupant.id == target.id
            }) {
                source_cells.push(at);
            }
        }
    }
    let Some(target_from) = source_cells.first().copied() else {
        return Ok(None);
    };
    let shift_r = i16::from(destination.row) - i16::from(clicked.row);
    let shift_c = i16::from(destination.col) - i16::from(clicked.col);
    let mut cells = Vec::with_capacity(source_cells.len());
    for at in &source_cells {
        let row = i16::from(at.row) + shift_r;
        let col = i16::from(at.col) + shift_c;
        if !(0..8).contains(&row) || !(0..8).contains(&col) {
            return Ok(None);
        }
        let cell = Square {
            row: row as u8,
            col: col as u8,
        };
        if state
            .at(cell)
            .is_some_and(|occupant| target.id.is_empty() || occupant.id != target.id)
            || collapsed(state, cell)
            || crate::movement::v7_scarecrow_reserved_square(state, cell)
            || !expansion_destination_allowed(state, target.color, &[cell])
        {
            return Ok(None);
        }
        cells.push(cell);
    }
    let to = Square {
        row: (i16::from(target_from.row) + shift_r) as u8,
        col: (i16::from(target_from.col) + shift_c) as u8,
    };
    Ok(Some(V7GrapplerPullPlan {
        target: target.clone(),
        target_from,
        source_cells,
        cells,
        to,
        until_color: owner(attacker)?.opponent(),
    }))
}

fn grappler(state: &GameState, piece: &Piece, from: Square) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id != RULES_VERSION_V7 {
        reject_portal(state, "grappler")?;
    }
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
        if state.ruleset_id == RULES_VERSION_V7 {
            if v7_grappler_pull_plan(state, from, to)?.is_some() {
                target.flags.insert("capture".into(), json!(false));
                target.flags.insert("grapplePull".into(), json!(true));
                moves.push(target);
            }
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
    if state.ruleset_id != RULES_VERSION_V7 {
        reject_pending_capture_policy(state, "colossus")?;
    }
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
        if state.ruleset_id == RULES_VERSION_V7 {
            let Some(source_captures) = crate::movement::v7_large_landing_captures(
                state,
                piece,
                &cells,
                usize::MAX,
                false,
            )?
            else {
                continue;
            };
            captures = source_captures;
        } else if !allowed {
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
        let mut source_enemy = false;
        if state.ruleset_id == RULES_VERSION_V7 {
            for &cell in &cells {
                let quantum = if state.at(cell).is_none() {
                    crate::v7_quantum_state::find_quantum_at(state, cell)?
                } else {
                    None
                };
                if let Some(victim) = state
                    .at(cell)
                    .or_else(|| quantum.as_ref().map(|q| &q.piece))
                    && colossus_sector_target_allowed_v7(state, piece, victim, cell)?
                {
                    source_enemy = true;
                    break;
                }
            }
        }
        if !(if state.ruleset_id == RULES_VERSION_V7 {
            source_enemy
        } else {
            cells.iter().any(|&cell| {
                state.at(cell).is_some_and(|victim| {
                    victim.color == owner.opponent()
                        && !victim.flag("shielded")
                        && !matches!(victim.ability_kind(), "guard" | "jester")
                        && victim.kind != "monster"
                        && !encouraged(state, victim)
                        && can_capture(state, piece, victim)
                })
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

pub(crate) fn merchant_cost_v7(piece: &Piece) -> Option<f64> {
    merchant_price(piece).map(|cost| cost as f64)
}

pub(crate) fn colossus_sector_target_allowed_v7(
    state: &GameState,
    attacker: &Piece,
    victim: &Piece,
    at: Square,
) -> Result<bool> {
    let enemy = owner(attacker)?.opponent();
    if victim.color != enemy
        || victim.flag("submerged")
        || frozen(victim)
        || matches!(victim.kind.as_str(), "wall" | "football" | "monster")
        || matches!(victim.ability_kind(), "guard" | "jester")
        || victim.ability_kind() == "revolvingDoor"
            && crate::movement::v7_uses_revolving_door_guard(state)
        || crate::v7_capture_reactions::is_protected_piece(state, victim, attacker)
        || crate::movement::v7_encouraged_at(state, victim, at)
        || !crate::movement::v7_can_capture_target(state, attacker, victim, false, false)?
    {
        return Ok(false);
    }
    let from = crate::movement::find_square(state, &attacker.id).ok_or_else(|| {
        EngineError::InvalidState("v7 Colossus sector attacker has no board origin".into())
    })?;
    let from = crate::movement::v7_normalize_origin(attacker, from)?;
    crate::v7_rule_geometry::v7_high_ground_capture_allowed(
        state,
        attacker,
        from,
        &MoveTarget::at(at),
    )
}

fn merchant(state: &GameState, piece: &Piece) -> Result<Vec<MoveTarget>> {
    if state.ruleset_id != RULES_VERSION_V7 && state.extra.get("armistice").is_some_and(nonempty) {
        return Err(EngineError::UnsupportedFeature(
            "merchant source armistice predicate".into(),
        ));
    }
    let enemy = owner(piece)?.opponent();
    let gold = if state.ruleset_id == RULES_VERSION_V7 {
        match piece.extra.get("gold") {
            None | Some(Value::Null) => 0.0,
            value => crate::card_effects::js_number(value, 0).unwrap_or(f64::NAN),
        }
    } else {
        piece.number("gold") as f64
    };
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
                || state.ruleset_id == RULES_VERSION_V7
                    && crate::movement::v7_armistice_active(state)
            {
                continue;
            }
            if let Some(price) = merchant_price(victim).filter(|price| *price as f64 <= gold) {
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
    if state.ruleset_id != RULES_VERSION_V7 {
        reject_pending_capture_policy(state, "football")?;
    }
    if state.ruleset_id != RULES_VERSION_V7
        && (state.extra.get("crownRule").is_some_and(nonempty)
            || state.extra.get("diceLocks").is_some_and(|value| {
                value
                    .as_object()
                    .is_some_and(|map| map.values().any(nonempty))
            }))
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
            if let Some(to) = from.offset(dr, dc).filter(|&to| {
                state.at(to).is_none()
                    && (state.ruleset_id != RULES_VERSION_V7
                        || !crate::v7_queued_effects::crown_ground_at(state, to))
            }) {
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
            || state.ruleset_id == RULES_VERSION_V7
                && (crate::movement::v7_dice_locked(state, kicker)
                    || crate::observation::piece_hidden_from_v7(state, kicker, kicker_square)
                        == Some(state.turn))
        {
            continue;
        }
        if state.ruleset_id != RULES_VERSION_V7
            && (kicker.extra.get("hiddenFrom").is_some_and(nonempty)
                || kicker.flag("camouflage")
                || kicker.flag("hallucination"))
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
            if state.ruleset_id == RULES_VERSION_V7
                && crate::v7_queued_effects::crown_ground_at(state, to)
            {
                break;
            }
            let victim = state.at(to);
            if victim.is_none_or(|victim| {
                !(if state.ruleset_id == RULES_VERSION_V7 {
                    crate::movement::v7_encouraged_at(state, victim, to)
                } else {
                    encouraged(state, victim)
                }) && can_capture(state, &attacker, victim)
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
    fn v7_imperial_family_keeps_source_type_order_and_exclusions() {
        let (mut state, mut piece, from) = empty("king");
        state.ruleset_id = RULES_VERSION_V7.into();
        piece.extra.insert(
            "imperialMoves".into(),
            json!(["rook", "bishop", "rook", "king", "wizard"]),
        );
        state.board[from.row as usize][from.col as usize] = Some(piece.clone());
        let before = serde_json::to_value(&state).unwrap();
        let moves = v7_imperial_study_moves(&state, &piece, from, &state).unwrap();
        assert_eq!(
            moves.first().map(MoveTarget::square),
            Some(Square { row: 3, col: 3 })
        );
        let first_bishop = moves
            .iter()
            .position(|m| m.flags.get("imperialStudy").and_then(Value::as_str) == Some("bishop"))
            .unwrap();
        assert_eq!(first_bishop, 14);
        assert_eq!(moves[first_bishop].square(), Square { row: 3, col: 2 });
        assert!(moves.iter().all(|m| matches!(
            m.flags.get("imperialStudy").and_then(Value::as_str),
            Some("rook" | "bishop")
        )));
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    fn v7_memory_ray_uses_literal_occupancy_and_geometry_ignores_capture_locks() {
        let (mut state, mut piece, from) = empty("parrot");
        state.ruleset_id = RULES_VERSION_V7.into();
        let mut ghost = Piece::new("pawn", Color::White, "memory-screen");
        ghost.extra.insert("ghost".into(), json!(true));
        state.board[4][4] = Some(ghost);
        state.board[4][5] = Some(Piece::new("king", Color::Black, "memory-royal"));
        let memory = json!({"type":"rook"});
        let raw = v7_memory_base_moves(
            &state,
            &piece,
            from,
            &memory,
            V7MemoryMode::Rules,
            true,
            true,
        )
        .unwrap();
        assert!(!raw.iter().any(|m| m.square() == Square { row: 4, col: 5 }));
        state.board[4][4] = None;
        piece.extra.insert("cardNoCaptureUntil".into(), json!(1));
        state.board[from.row as usize][from.col as usize] = Some(piece.clone());
        assert!(
            !v7_memory_base_moves(
                &state,
                &piece,
                from,
                &memory,
                V7MemoryMode::Rules,
                true,
                true
            )
            .unwrap()
            .iter()
            .any(|m| m.square() == Square { row: 4, col: 5 })
        );
        let geometry = v7_memory_base_moves(
            &state,
            &piece,
            from,
            &memory,
            V7MemoryMode::Geometry,
            true,
            true,
        )
        .unwrap();
        assert!(
            geometry
                .iter()
                .any(|m| m.square() == Square { row: 4, col: 5 } && m.flag("capture"))
        );
    }

    #[test]
    fn v7_thief_catalog_overrides_legacy_flags_and_required_jump_counts_identity() {
        let (mut state, piece, from) = empty("thief");
        state.ruleset_id = RULES_VERSION_V7.into();
        state.extra.remove("cardState");
        state.extra.insert(
            "profile".into(),
            json!({"catalogHash":"yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4"}),
        );
        state.extra.insert("thiefQuietJump".into(), json!(false));
        state.extra.insert("thiefRequiredJump".into(), json!(true));
        state.board[4][4] = Some(Piece::new("pawn", Color::White, "thief-screen"));
        state.board[4][5] = Some(Piece::new("pawn", Color::Black, "thief-target"));
        let moves = thief(&state, &piece, from).unwrap();
        assert!(
            !moves
                .iter()
                .any(|target| target.square() == Square { row: 4, col: 5 })
        );
        assert!(
            moves
                .iter()
                .any(|target| target.square() == Square { row: 4, col: 6 }
                    && target.flag("thiefQuietJump"))
        );
        state.extra.remove("profile");
        let moves = thief(&state, &piece, from).unwrap();
        assert!(
            moves
                .iter()
                .any(|target| target.square() == Square { row: 4, col: 5 }
                    && !target.flag("thiefQuietJump"))
        );
        assert!(
            !moves
                .iter()
                .any(|target| target.square() == Square { row: 4, col: 2 })
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
