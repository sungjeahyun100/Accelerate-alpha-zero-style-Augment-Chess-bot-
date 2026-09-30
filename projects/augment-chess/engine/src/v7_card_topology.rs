//! Source-pinned topology-changing ACTIVE cards.
//!
//! This object owns only the IDs below. The caller owns card accounting,
//! common reconciliation, turn settlement, and replay commit. A source branch
//! whose consequences have not been ported is rejected before mutation.
//! Frozen client: main-OahWs0tU.js SHA-256
//! e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c.

use super::*;

pub(super) const IDS: &[&str] = &[
    "baby-bear",
    "canceling",
    "castling",
    "conscription",
    "death-squad",
    "eagle",
    "fanatical-ritual",
    "homecoming",
    "knightmate",
    "martyrdom",
    "merchant-guild",
    "palace",
    "pawn-storm",
    "queen-cavalry",
    "qxe1",
    "reformation",
    "schrodinger-pawns",
    "summon-colossus",
    "traitor",
    "vortex",
    "black-magic",
    "fleeting-dream",
    "extinction",
    "brainwash",
];

const EFFECTS: &[(&str, &str)] = &[
    ("baby-bear", "babyBear"),
    ("canceling", "canceling"),
    ("castling", "freeCastling"),
    ("conscription", "conscription"),
    ("death-squad", "deathSquad"),
    ("eagle", "eagle"),
    ("fanatical-ritual", "fanaticalRitual"),
    ("homecoming", "homecoming"),
    ("knightmate", "knightmate"),
    ("martyrdom", "martyrdom"),
    ("merchant-guild", "merchantGuild"),
    ("palace", "palace"),
    ("pawn-storm", "pawnStorm"),
    ("queen-cavalry", "queenCavalry"),
    ("qxe1", "qxe1"),
    ("reformation", "reformation"),
    ("schrodinger-pawns", "schrodingerPawns"),
    ("summon-colossus", "summonColossus"),
    ("traitor", "traitor"),
    ("vortex", "vortex"),
    ("black-magic", "blackMagic"),
    ("fleeting-dream", "fleetingDream"),
    ("extinction", "extinction"),
    ("brainwash", "brainwash"),
];

fn owned(card: &CardSlot) -> bool {
    IDS.contains(&card.id.as_str())
}

fn source_card(state: &GameState, card: &CardSlot) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "topology card {} on rules version {}",
            card.id, state.ruleset_id
        )));
    }
    let expected = EFFECTS
        .iter()
        .find(|entry| entry.0 == card.id)
        .ok_or_else(|| EngineError::UnsupportedFeature(format!("topology card {}", card.id)))?
        .1;
    let definition = crate::card_registry::definition_for(RULES_VERSION_V7, &card.id)?;
    if card.effect != expected || definition.effect != expected {
        return Err(EngineError::InvalidState(format!(
            "topology card catalog effect drift for {}",
            card.id
        )));
    }
    Ok(())
}

fn unsupported(card: &CardSlot, branch: &str) -> EngineError {
    EngineError::UnsupportedFeature(format!("v7 topology card {}: {branch}", card.id))
}

fn square_name(square: Square) -> String {
    format!("{}{}", char::from(b'a' + square.col), 8 - square.row)
}

fn original_square(piece: &Piece) -> Option<Square> {
    let text = piece.extra.get("origin")?.as_str()?.as_bytes();
    if text.len() != 2 || !(b'a'..=b'h').contains(&text[0]) || !(b'1'..=b'8').contains(&text[1]) {
        return None;
    }
    Some(Square {
        row: 8 - (text[1] - b'0'),
        col: text[0] - b'a',
    })
}

fn unique_squares(
    state: &GameState,
    predicate: impl Fn(Square, &Piece) -> Result<bool>,
) -> Result<Vec<Square>> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(piece) = state.at(square) else {
                continue;
            };
            let key = if piece.id.is_empty() {
                format!("square:{row}:{col}")
            } else {
                format!("piece:{}", piece.id)
            };
            if seen.insert(key) && predicate(square, piece)? {
                result.push(square);
            }
        }
    }
    Ok(result)
}

fn baby_bear_candidate(state: &GameState, piece: &Piece) -> bool {
    source_matches(state, piece, Source::NonRoyal("queen"))
}

fn homecoming_candidate(state: &GameState, square: Square, piece: &Piece) -> Result<bool> {
    if piece.color != state.turn
        || state.royal_identity(piece)
        || [
            "wall",
            "football",
            "blackHole",
            "coffin",
            "colossus",
            "bigRook",
            "bigBishop",
        ]
        .contains(&piece.kind.as_str())
    {
        return Ok(false);
    }
    let Some(origin) = original_square(piece) else {
        return Ok(false);
    };
    Ok(origin != square
        && !crate::movement::collapsed(state, origin)
        && crate::movement::open_relocation(state, origin)?)
}

fn minor_extinction(state: &GameState, piece: &Piece) -> bool {
    !state.royal_identity(piece) && minor(state, piece)
}

fn pawn_storm_candidate(state: &GameState, square: Square, piece: &Piece) -> Result<bool> {
    if piece.color != state.turn || piece.kind != "pawn" {
        return Ok(false);
    }
    if crate::movement::frozen(piece) || staked(piece) {
        return Ok(false);
    }
    let direction = pawn_storm_direction(state);
    let Some(next) = square.offset(direction, 0) else {
        return Ok(false);
    };
    crate::movement::open_relocation(state, next)
}

fn pawn_storm_direction(state: &GameState) -> i8 {
    // Source pawnStormDirection (67988-67990) switches to pawnDir only for
    // the September 18 profile. Its reverse counter is not a plain flag.
    let reversed = state
        .extra
        .get("effects")
        .and_then(|effects| effects.get("pawnReverse"))
        .and_then(|reverse| reverse.get(state.turn.as_str()))
        .and_then(Value::as_f64)
        .is_some_and(|remaining| remaining > 0.0);
    state.turn.pawn_dir()
        * if september18_balance(state) && reversed {
            -1
        } else {
            1
        }
}

fn september18_balance(state: &GameState) -> bool {
    let profile = match state
        .extra
        .get("cardState")
        .filter(|value| truthy(Some(value)))
    {
        Some(card_state) => card_state.get("profile"),
        None => state.extra.get("profile"),
    };
    let hash = profile.and_then(|profile| profile.get("catalogHash"));
    if truthy(hash) {
        return hash.and_then(Value::as_str).is_some_and(|hash| {
            [
                "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
                "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
                "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs",
                "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc",
                "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g",
                "HVzv4vxzNKDRi-ylshTw9HnLmIlyiQhMScQ4caG_aSI",
            ]
            .contains(&hash)
        });
    }
    state.extra.get("september18Balance") != Some(&Value::Bool(false))
}

fn quantum_shadow_at(state: &GameState, square: Square, except_id: &str) -> bool {
    let mut seen = BTreeSet::new();
    state.board.iter().flatten().flatten().any(|piece| {
        if !seen.insert(piece.id.as_str()) || piece.id == except_id {
            return false;
        }
        let Some(quantum) = piece.extra.get("quantum") else {
            return false;
        };
        let (Some(row), Some(col)) = (
            quantum.get("row").and_then(Value::as_u64),
            quantum.get("col").and_then(Value::as_u64),
        ) else {
            return false;
        };
        let width = u64::from(piece.is_large());
        u64::from(square.row) >= row
            && u64::from(square.row) <= row + width
            && u64::from(square.col) >= col
            && u64::from(square.col) <= col + width
    })
}

fn quantum_destination_open(state: &GameState, square: Square, piece: &Piece) -> bool {
    if state.at(square).is_some() || quantum_shadow_at(state, square, &piece.id) {
        return false;
    }
    !["pendingScarecrows", "pendingLobsters"].iter().any(|name| {
        state
            .extra
            .get(*name)
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry.get("row").and_then(Value::as_u64) == Some(u64::from(square.row))
                        && entry.get("col").and_then(Value::as_u64) == Some(u64::from(square.col))
                        && (*name == "pendingLobsters" || !truthy(entry.get("pieceId")))
                })
            })
    })
}

fn schrodinger_candidates(state: &GameState) -> Vec<(Square, Square)> {
    let reversed = state
        .extra
        .get("effects")
        .and_then(|effects| effects.get("pawnReverse"))
        .and_then(|reverse| reverse.get(state.turn.as_str()))
        .and_then(Value::as_f64)
        .is_some_and(|remaining| remaining > 0.0);
    let back = -state.turn.pawn_dir() * if reversed { -1 } else { 1 };
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let source = Square { row, col };
            let Some(piece) = state.at(source) else {
                continue;
            };
            if !seen.insert(piece.id.as_str())
                || piece.color != state.turn
                || piece.kind != "pawn"
                || truthy(piece.extra.get("quantum"))
            {
                continue;
            }
            let Some(destination) = source.offset(back, 0) else {
                continue;
            };
            if quantum_destination_open(state, destination, piece) {
                candidates.push((source, destination));
            }
        }
    }
    candidates
}

fn black_magic_candidates(state: &GameState) -> Result<Vec<Square>> {
    let royals = unique_squares(state, |_, piece| {
        Ok(piece.color == state.turn && state.royal_identity(piece))
    })?;
    let mut cells = BTreeSet::new();
    for royal in royals {
        for dr in -1..=1 {
            for dc in -1..=1 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let Some(square) = royal.offset(dr, dc) else {
                    continue;
                };
                if crate::movement::open_placement(state, square, Some(state.turn))?
                    && !crown_ground(state, square)
                    && !crate::movement::collapsed(state, square)
                    && !black_hole_cell(state, square)
                {
                    cells.insert(square);
                }
            }
        }
    }
    Ok(cells.into_iter().collect())
}

fn black_hole_cell(state: &GameState, square: Square) -> bool {
    state
        .extra
        .get("blackHole")
        .and_then(Value::as_array)
        .is_some_and(|cells| {
            cells.iter().any(|cell| {
                cell.get("row").and_then(Value::as_u64) == Some(u64::from(square.row))
                    && cell.get("col").and_then(Value::as_u64) == Some(u64::from(square.col))
            })
        })
}

#[derive(Clone, Copy)]
struct CardCastle {
    king_from: Square,
    king_to: Square,
    rook_from: Square,
    rook_to: Square,
}

// Source freeCastlingMoves / freeCastlingTargetEntries (96675-96704). This
// card does not use ordinary chess castling rights, clear paths, or attack
// safety: it scans all eight rays from the first king augment recipient and
// may crush occupants at its two landing cells when played.
fn card_castles(state: &GameState) -> Vec<CardCastle> {
    if state.flag("castlingCanceled", state.turn) {
        return Vec::new();
    }
    let Some(king_from) = king_augment_square(state) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    const DIRECTIONS: [(i8, i8); 8] = [
        (-1, -1),
        (-1, 1),
        (1, -1),
        (1, 1),
        (-1, 0),
        (1, 0),
        (0, -1),
        (0, 1),
    ];
    for (dr, dc) in DIRECTIONS {
        let (Some(king_to), Some(rook_to)) =
            (king_from.offset(2 * dr, 2 * dc), king_from.offset(dr, dc))
        else {
            continue;
        };
        for distance in 3..8_i8 {
            let Some(rook_from) = king_from.offset(distance * dr, distance * dc) else {
                break;
            };
            if state
                .at(rook_from)
                .is_some_and(|piece| piece.color == state.turn && piece.kind == "rook")
            {
                result.push(CardCastle {
                    king_from,
                    king_to,
                    rook_from,
                    rook_to,
                });
            }
        }
    }
    result
}

fn targets(state: &GameState, card: &CardSlot) -> Result<Vec<Square>> {
    match card.id.as_str() {
        "baby-bear" => unique_squares(state, |_, piece| Ok(baby_bear_candidate(state, piece))),
        "brainwash" => unique_squares(state, |_, piece| Ok(brainwash_source(state, piece))),
        "extinction" => unique_squares(state, |_, piece| {
            Ok(piece.color == state.turn && minor_extinction(state, piece))
        }),
        "homecoming" => unique_squares(state, |square, piece| {
            homecoming_candidate(state, square, piece)
        }),
        "pawn-storm" => unique_squares(state, |square, piece| {
            pawn_storm_candidate(state, square, piece)
        }),
        "death-squad" => {
            let files = (0..8)
                .filter(|&col| {
                    (0..8).any(|row| {
                        state
                            .at(Square { row, col })
                            .is_some_and(|piece| piece.color == state.turn && piece.kind == "pawn")
                    })
                })
                .collect::<Vec<_>>();
            Ok((0..8)
                .flat_map(|row| files.iter().map(move |&col| Square { row, col }))
                .collect())
        }
        "castling" => Ok(card_castles(state)
            .into_iter()
            .map(|entry| entry.rook_from)
            .collect()),
        _ => Ok(Vec::new()),
    }
}

pub(super) fn ui_targets(state: &GameState, card: &CardSlot) -> Result<Option<Vec<Square>>> {
    if !owned(card) {
        return Ok(None);
    }
    source_card(state, card)?;
    Ok(Some(targets(state, card)?))
}

fn untargeted_ready(state: &GameState, card: &CardSlot) -> Result<bool> {
    let own = state.turn;
    let enemy = own.opponent();
    Ok(match card.id.as_str() {
        "reformation" => {
            !unique_squares(state, |_, p| Ok(p.color == own && p.kind == "bishop"))?.is_empty()
        }
        "knightmate" => king_augment_square(state).is_some(),
        "palace" => !unique_squares(
            state,
            |_, p| Ok(p.color == enemy && state.royal_identity(p)),
        )?
        .is_empty(),
        // These source effects succeed even when they change zero pieces.
        "fleeting-dream" | "conscription" => true,
        "queen-cavalry" => queen_cavalry_pawn(state).is_some(),
        "fanatical-ritual" => !ritual_candidates(state)?.is_empty(),
        "vortex" => vortex_candidates(state)?.len() >= 2,
        "qxe1" => qxe1_plan(state).is_some(),
        "black-magic" => !black_magic_candidates(state)?.is_empty(),
        "schrodinger-pawns" => !schrodinger_candidates(state).is_empty(),
        "martyrdom" => {
            !unique_squares(state, |_, piece| {
                Ok(piece.color == own && piece.kind == "bishop")
            })?
            .is_empty()
                && !unique_squares(state, |_, piece| {
                    Ok(piece.color == own && piece.kind == "pawn")
                })?
                .is_empty()
        }
        "merchant-guild" => merchant_guild_plan(state).is_some(),
        "traitor" => !traitor_candidates(state)?.is_empty(),
        "eagle" => !unique_squares(state, |_, piece| {
            Ok(piece.color == own && piece.kind == "knight")
        })?
        .is_empty(),
        "canceling" => canceling_plan(state).is_some(),
        "summon-colossus" => colossus_sacrifices(state)?.len() == 6,
        _ => false,
    })
}

pub(super) fn actions(state: &GameState, card: &CardSlot) -> Result<Option<Vec<Action>>> {
    if !owned(card) {
        return Ok(None);
    }
    source_card(state, card)?;
    if card.id == "pawn-storm" {
        // completeCardTargets in the source oracle emits each prefix before
        // its increasing-index descendants, not every permutation. Pawn Storm
        // is an eager source family with at most eight selected pieces.
        fn combinations(
            state: &GameState,
            card: &CardSlot,
            squares: &[Square],
            start: usize,
            selected: &mut Vec<Square>,
            actions: &mut Vec<Action>,
        ) -> Result<()> {
            if !selected.is_empty() {
                if actions.len() == 100_000 {
                    return Err(unsupported(
                        card,
                        "source eager target capacity exceeded (100000)",
                    ));
                }
                actions.push(Action::card(
                    state.turn,
                    card,
                    Some(json!({"selections":selected})),
                ));
            }
            if selected.len() == 8 {
                return Ok(());
            }
            for index in start..squares.len() {
                selected.push(squares[index]);
                combinations(state, card, squares, index + 1, selected, actions)?;
                selected.pop();
            }
            Ok(())
        }
        let mut actions = Vec::new();
        combinations(
            state,
            card,
            &targets(state, card)?,
            0,
            &mut Vec::new(),
            &mut actions,
        )?;
        return Ok(Some(actions));
    }
    if card.id == "brainwash" {
        let mut actions = Vec::new();
        for source in targets(state, card)? {
            let offered = state.at(source).ok_or(EngineError::IllegalAction)?;
            for victim in brainwash_victims(state, offered) {
                actions.push(Action::card(
                    state.turn,
                    card,
                    Some(json!({"selections":[source,victim]})),
                ));
            }
        }
        return Ok(Some(actions));
    }
    if truthy(card.extra.get("target")) {
        return Ok(Some(
            targets(state, card)?
                .into_iter()
                .map(|square| Action::card(state.turn, card, Some(json!(square))))
                .collect(),
        ));
    }
    Ok(Some(if untargeted_ready(state, card)? {
        vec![Action::card(state.turn, card, None)]
    } else {
        Vec::new()
    }))
}

fn validate_envelope(state: &GameState, card: &CardSlot, action: &Action) -> Result<()> {
    source_card(state, card)?;
    if action.kind != ActionKind::Card
        || action.color != state.turn
        || action.card_id.as_deref() != Some(card.id.as_str())
        || action.card_instance_id.as_deref() != Some(card.instance_id.as_str())
        || action.from.is_some()
        || action.destination.is_some()
        || action.position_key.is_some()
        || !action.extra.is_empty()
    {
        return Err(EngineError::IllegalAction);
    }
    Ok(())
}

pub(super) fn apply(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    validate_envelope(state, card, action)?;
    // canResolve/royal probes own a disposable source state. A failed effect
    // keeps its preceding source mutations and random draws in that state.
    // Public actions still commit only after the complete effect succeeds.
    if state.is_ai_simulation() {
        return apply_direct(state, card, action);
    }
    let mut working = state.clone();
    let captures = apply_direct(&mut working, card, action)?;
    *state = working;
    Ok(captures)
}

fn apply_direct(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    match card.id.as_str() {
        "brainwash" => apply_brainwash(state, action),
        "baby-bear" => apply_baby_bear(state, card, action),
        "death-squad" => apply_death_squad(state, card, action),
        "extinction" => apply_extinction(state, card, action),
        "homecoming" => apply_homecoming(state, card, action),
        "pawn-storm" => apply_pawn_storm(state, card, action),
        "reformation" => apply_reformation(state, card, action),
        "knightmate" => apply_knightmate(state, card, action),
        "palace" => apply_palace(state, card, action),
        "fleeting-dream" => apply_fleeting_dream(state, card, action),
        "queen-cavalry" => apply_queen_cavalry(state, card, action),
        "conscription" => apply_conscription(state, card, action),
        "fanatical-ritual" => apply_fanatical_ritual(state, card, action),
        "vortex" => apply_vortex(state, card, action),
        "qxe1" => apply_qxe1(state, card, action),
        "black-magic" => apply_black_magic(state, card, action),
        "schrodinger-pawns" => apply_schrodinger_pawns(state, card, action),
        "martyrdom" => apply_martyrdom(state, card, action),
        "merchant-guild" => apply_merchant_guild(state, card, action),
        "traitor" => apply_traitor(state, card, action),
        "castling" => apply_card_castling(state, card, action),
        "eagle" => apply_eagle(state, card, action),
        "canceling" => apply_canceling(state, card, action),
        "summon-colossus" => apply_summon_colossus(state, card, action),
        _ => Err(unsupported(card, "source direct effect")),
    }
}

fn require_none(action: &Action) -> Result<()> {
    if !has_card_selection(action) {
        Ok(())
    } else {
        Err(EngineError::IllegalAction)
    }
}

fn require_target(state: &GameState, card: &CardSlot, action: &Action) -> Result<Square> {
    let value = action.target.as_ref().ok_or(EngineError::IllegalAction)?;
    let square = square_value(value)?;
    if !targets(state, card)?.contains(&square) {
        return Err(EngineError::IllegalAction);
    }
    Ok(square)
}

fn piece_at(state: &GameState, square: Square) -> Result<Piece> {
    state.at(square).cloned().ok_or(EngineError::IllegalAction)
}

fn card_castle_racing_kings(state: &mut GameState, _card: &CardSlot) -> Result<()> {
    if state.mode == "gameover" {
        return Ok(());
    }
    let racing =
        |color: Color| state.flag("racingKing", color) || truthy(state.extra.get("machoChess"));
    if !racing(Color::White) && !racing(Color::Black) {
        return Ok(());
    }
    let depth = if truthy(state.extra.get("collapsed")) {
        let raw = js_number(state.extra.get("collapseDepth"), 1).unwrap_or(1.0);
        (if raw == 0.0 { 1.0 } else { raw }).floor().clamp(0.0, 4.0) as u8
    } else {
        0
    };
    let winners = [Color::White, Color::Black]
        .into_iter()
        .filter(|&color| {
            racing(color)
                && color_king_augment_square(state, color).is_some_and(|square| {
                    square.row
                        == if color == Color::White {
                            depth
                        } else {
                            7 - depth
                        }
                })
        })
        .collect::<Vec<_>>();
    match winners.as_slice() {
        [] => Ok(()),
        [winner] => crate::flow::end_game(
            state,
            Some(*winner),
            "레이싱 킹이 목표 랭크에 도달했습니다.",
        ),
        _ => crate::flow::end_game(
            state,
            None,
            "양쪽 킹이 동시에 목표 랭크에 도달하여 무승부입니다.",
        ),
    }
}

fn apply_card_castling(
    state: &mut GameState,
    card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    let selected = require_target(state, card, action)?;
    let castle = card_castles(state)
        .into_iter()
        .find(|entry| entry.rook_from == selected)
        .ok_or(EngineError::IllegalAction)?;
    let mut king = piece_at(state, castle.king_from)?;
    let mut rook = piece_at(state, castle.rook_from)?;
    let privacy = relocation_privacy(state, &king, castle.king_from, card)?;
    state.board[castle.king_from.row as usize][castle.king_from.col as usize] = None;
    state.board[castle.rook_from.row as usize][castle.rook_from.col as usize] = None;
    let mut captures = Vec::new();
    for (destination, by_rook) in [(castle.king_to, false), (castle.rook_to, true)] {
        if state.at(destination).is_none() {
            continue;
        }
        let options = crate::v7_capture_reactions::CaptureOptions {
            force_capture: true,
            allow_jester: true,
            attacker_landing: Some(destination),
            ..Default::default()
        };
        let captured = crate::v7_capture_reactions::capture_at(
            state,
            if by_rook { &mut rook } else { &mut king },
            destination,
            &options,
        )?;
        if let Some(captured) = captured {
            if by_rook {
                crate::v7_capture_reactions::learn_imperial_study(state, &mut king, &captured)?;
            }
            captures.push(captured);
        }
    }
    state.board[castle.king_to.row as usize][castle.king_to.col as usize] = Some(king.clone());
    state.board[castle.rook_to.row as usize][castle.rook_to.col as usize] = Some(rook.clone());
    let castle_hidden = relocation_hidden_from(state, &king, castle.king_to, &privacy)?;
    set_last_move(
        state,
        castle.king_from,
        castle.king_to,
        "castle",
        state.turn,
        &castle_hidden,
        Some(&king),
    )?;
    let hidden = state.extra["lastMove"]["hiddenFrom"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    track_acceleration_trail(
        state,
        state.turn,
        &[
            castle.king_from,
            castle.king_to,
            castle.rook_to,
            castle.rook_from,
            castle.rook_to,
        ],
        false,
        &hidden,
    )?;
    mark_animation(state, &king)?;
    mark_animation(state, &rook)?;
    crate::v7_threat::play_move_sound_v7(
        state,
        if captures.is_empty() {
            "castle"
        } else {
            "capture"
        },
        state.turn,
    )?;
    king.moved = true;
    rook.moved = true;
    state.board[castle.king_to.row as usize][castle.king_to.col as usize] = Some(king.clone());
    state.board[castle.rook_to.row as usize][castle.rook_to.col as usize] = Some(rook.clone());
    crate::replay::track_moving(state, &king)?;
    state.set_flag("castled", state.turn, true);
    state.set_flag("freeCastling", state.turn, false);
    state.set_flag("zugzwang", state.turn, false);
    king = piece_at(state, castle.king_to)?;
    rook = piece_at(state, castle.rook_to)?;
    note_ultimatum_movement(state, &mut king)?;
    note_ultimatum_movement(state, &mut rook)?;
    state.board[castle.king_to.row as usize][castle.king_to.col as usize] = Some(king.clone());
    state.board[castle.rook_to.row as usize][castle.rook_to.col as usize] = Some(rook);
    state.en_passant = None;
    let mut move_target = MoveTarget::at(castle.king_to);
    move_target
        .flags
        .insert("castle".into(), json!("카드 캐슬링"));
    crate::replay::queue_v7_move_notation_with_options(
        state,
        &king,
        castle.king_from,
        castle.king_to,
        &move_target,
        &crate::replay::V7MoveNotationOptions {
            piece_type: &king.kind,
            disambiguation: "",
            promotion: None,
            privacy: &privacy,
            capture: false,
            capture_known_squares: &json!({}),
            game_end: false,
        },
    )?;
    let message = if crate::replay::fog_log_redaction_active_v7(state) {
        format!("{} 기물이 이동했습니다.", crate::replay::label(state.turn))
    } else {
        format!("{} 캐슬링: 카드 캐슬링", crate::replay::label(state.turn))
    };
    crate::replay::add_log(state, message)?;
    crate::transition::resolve_herald_threats(state, state.turn)?;
    card_castle_racing_kings(state, card)?;
    Ok(captures)
}

// Source nearestAlibabaPlacementCandidates (3422-3448) stops at the first
// Chebyshev ring containing an open cell. If every cell in that ring is
// attacked, it falls back to that same ring rather than searching farther.
fn alibaba_candidates(state: &GameState, origin: Square, color: Color) -> Result<Vec<Square>> {
    for radius in 1..=8_u8 {
        let mut candidates = Vec::new();
        for row in origin.row.saturating_sub(radius)..=(origin.row + radius).min(7) {
            for col in origin.col.saturating_sub(radius)..=(origin.col + radius).min(7) {
                let target = Square { row, col };
                if origin.row.abs_diff(row).max(origin.col.abs_diff(col)) != radius {
                    continue;
                }
                if crate::movement::open_placement(state, target, Some(color))?
                    && !crown_ground(state, target)
                    && !crate::movement::collapsed(state, target)
                    && !black_hole_cell(state, target)
                {
                    candidates.push(target);
                }
            }
        }
        if candidates.is_empty() {
            continue;
        }
        // Reuse one owned probe while restoring its single substituted cell.
        // Candidate safety does not copy the history once per destination.
        let temporary: Piece = serde_json::from_value(json!({
            "color":color,"type":"eagle","moved":true,"shielded":false
        }))
        .map_err(EngineError::serialization)?;
        let mut probe = state.clone();
        let mut safe = Vec::new();
        for &target in &candidates {
            // isAlibabaPlacementSafe replaces the destination with this
            // temporary piece before calling isSquareAttacked. No identity
            // or random piece creation belongs to the safety probe.
            let previous =
                probe.board[target.row as usize][target.col as usize].replace(temporary.clone());
            let attacked = crate::v7_threat::is_square_attacked_v7(
                &probe,
                target,
                color.opponent(),
                Some(&temporary),
                None,
            );
            probe.board[target.row as usize][target.col as usize] = previous;
            if !attacked? {
                safe.push(target);
            }
        }
        return Ok(if safe.is_empty() { candidates } else { safe });
    }
    Ok(Vec::new())
}

fn apply_eagle(state: &mut GameState, _card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    require_none(action)?;
    let color = state.turn;
    let knights = unique_squares(state, |_, piece| {
        Ok(piece.color == color && piece.kind == "knight")
    })?;
    if knights.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    for &origin in &knights {
        transform_at(state, origin, "eagle", true)?;
    }
    for origin in knights {
        let candidates = alibaba_candidates(state, origin, color)?;
        if candidates.is_empty() {
            break;
        }
        // Source randomChoice consumes a draw even for a single candidate.
        let index = (state.rng.sample()? * candidates.len() as f64).floor() as usize;
        state
            .rng
            .record_last_probability(1.0 / candidates.len() as f64, "source Eagle placement")?;
        let target = candidates[index.min(candidates.len() - 1)];
        let mut summoned = crate::opening::spawn(state, color, "eagle")?;
        summoned.moved = true;
        mark_transformed_origin(state, &mut summoned, target)?;
        state.board[target.row as usize][target.col as usize] = Some(summoned.clone());
        mark_animation(state, &summoned)?;
    }
    // The source invokes playSound, not playMoveSound. It neither updates
    // lastMove nor executes the royal-capture threat simulation here.
    Ok(Vec::new())
}

fn color_king_augment_square(state: &GameState, color: Color) -> Option<Square> {
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .find(|&square| {
            state
                .at(square)
                .is_some_and(|piece| piece.color == color && state.royal_identity(piece))
        })
}

fn canceling_plan(state: &GameState) -> Option<(Square, Square)> {
    let enemy = state.turn.opponent();
    if !state.flag("castled", enemy) {
        return None;
    }
    let from = color_king_augment_square(state, enemy)?;
    let origin = original_square(state.at(from)?).unwrap_or(Square {
        row: if enemy == Color::White { 7 } else { 0 },
        col: 4,
    });
    (from != origin).then_some((from, origin))
}

fn relocation_privacy(
    state: &GameState,
    piece: &Piece,
    from: Square,
    _card: &CardSlot,
) -> Result<Value> {
    let mut privacy = Map::new();
    for viewer in [Color::White, Color::Black] {
        let visible = crate::observation::piece_visible_to_color_at_v7(state, piece, from, viewer)?;
        privacy.insert(
            viewer.as_str().into(),
            json!({
                "originVisible":visible,
                "typeKnown":visible || piece.color == viewer
                    || piece.extra.get("hiddenFrom").and_then(Value::as_str)==Some(viewer.as_str())
            }),
        );
    }
    Ok(Value::Object(privacy))
}

fn relocation_hidden_from(
    state: &GameState,
    piece: &Piece,
    destination: Square,
    privacy: &Value,
) -> Result<String> {
    let Some(owner) = piece.color.owner() else {
        return Ok(String::new());
    };
    let viewer = owner.opponent();
    if privacy[viewer.as_str()]["originVisible"] == json!(false)
        || !crate::observation::piece_visible_to_color_at_v7(state, piece, destination, viewer)?
    {
        Ok(viewer.as_str().into())
    } else {
        Ok(String::new())
    }
}

fn apply_canceling(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    require_none(action)?;
    let (from, origin) = canceling_plan(state).ok_or(EngineError::IllegalAction)?;
    let mut king = piece_at(state, from)?;
    let enemy = state.turn.opponent();
    let privacy = relocation_privacy(state, &king, from, card)?;
    let collapsed = crate::movement::collapsed(state, origin);
    let mut captures = Vec::new();
    if state
        .at(origin)
        .is_some_and(|occupant| occupant.id != king.id)
    {
        let threat_source = json!({"label":"캔슬링"});
        if let Some(victim) = crate::transition::force_remove_piece_at_with_options(
            state,
            origin,
            state.turn,
            &crate::transition::ForceRemovalOptions {
                threat_source: Some(&threat_source),
                ..Default::default()
            },
        )? {
            captures.push(victim);
        }
        // Force removal can grant Vigilance to the relocating king.
        king = piece_at(state, from)?;
    }
    state.board[from.row as usize][from.col as usize] = None;
    state.board[origin.row as usize][origin.col as usize] = Some(king.clone());
    let hidden = relocation_hidden_from(state, &king, origin, &privacy)?;
    set_last_move(state, from, origin, "move", enemy, &hidden, None)?;
    king.moved = true;
    if truthy(state.extra.get("monochromeChess")) {
        king.extra.insert(
            "monoShade".into(),
            json!(if (origin.row + origin.col).is_multiple_of(2) {
                "light"
            } else {
                "dark"
            }),
        );
    }
    state.board[origin.row as usize][origin.col as usize] = Some(king.clone());
    state.set_flag("castled", enemy, false);
    state.en_passant = None;
    mark_animation(state, &king)?;
    if collapsed {
        let threat_source = json!({"label":"캔슬링 · 붕괴"});
        if let Some(victim) = crate::transition::force_remove_piece_at_with_options(
            state,
            origin,
            state.turn,
            &crate::transition::ForceRemovalOptions {
                threat_source: Some(&threat_source),
                ..Default::default()
            },
        )? {
            captures.push(victim);
        }
    }
    Ok(captures)
}

fn colossus_anchor(state: &GameState) -> Square {
    let diagonal = truthy(state.extra.get("diagonalChess"));
    Square {
        row: if state.turn == Color::White {
            if diagonal { 4 } else { 5 }
        } else if diagonal {
            2
        } else {
            1
        },
        col: 3,
    }
}

fn colossus_sacrifices(state: &GameState) -> Result<Vec<Square>> {
    let pawns = unique_squares(state, |_, piece| {
        Ok(piece.color == state.turn && piece.kind == "pawn")
    })?;
    if pawns.len() > 8 {
        let anchor = colossus_anchor(state);
        let home_row = if state.turn == Color::White {
            anchor.row + 1
        } else {
            anchor.row
        };
        return Ok([
            Square {
                row: home_row,
                col: anchor.col - 1,
            },
            anchor,
            Square {
                row: anchor.row + 1,
                col: anchor.col,
            },
            Square {
                row: anchor.row,
                col: anchor.col + 1,
            },
            Square {
                row: anchor.row + 1,
                col: anchor.col + 1,
            },
            Square {
                row: home_row,
                col: anchor.col + 2,
            },
        ]
        .into_iter()
        .filter(|&square| {
            state
                .at(square)
                .is_some_and(|piece| piece.color == state.turn && piece.kind == "pawn")
        })
        .collect());
    }
    let rooks = unique_squares(state, |_, piece| {
        Ok(piece.color == state.turn && piece.kind == "rook")
    })?;
    let reversed = state
        .extra
        .get("effects")
        .and_then(|effects| effects.get("pawnReverse"))
        .and_then(|reverse| reverse.get(state.turn.as_str()))
        .and_then(Value::as_f64)
        .is_some_and(|remaining| remaining > 0.0);
    let direction = state.turn.pawn_dir() * if reversed { -1 } else { 1 };
    let diagonal = truthy(state.extra.get("diagonalChess"));
    let preserved = |square: Square| {
        if diagonal && !rooks.is_empty() {
            rooks
                .iter()
                .any(|&rook| rook.offset(direction, 0) == Some(square))
        } else if rooks.is_empty() {
            square.col == 0 || square.col == 7
        } else {
            rooks.iter().any(|rook| rook.col == square.col)
        }
    };
    Ok(pawns
        .iter()
        .copied()
        .filter(|&square| !preserved(square))
        .chain(pawns.iter().copied().filter(|&square| preserved(square)))
        .take(6)
        .collect())
}

fn apply_summon_colossus(
    state: &mut GameState,
    _card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    require_none(action)?;
    let sacrifices = colossus_sacrifices(state)?;
    if sacrifices.len() != 6 {
        return Err(EngineError::IllegalAction);
    }
    let summoner = state.turn;
    let anchor = colossus_anchor(state);
    let cells = [
        anchor,
        Square {
            row: anchor.row,
            col: anchor.col + 1,
        },
        Square {
            row: anchor.row + 1,
            col: anchor.col,
        },
        Square {
            row: anchor.row + 1,
            col: anchor.col + 1,
        },
    ];
    let mut victims: Vec<(Square, String, bool)> = Vec::new();
    for (square, sacrificed) in sacrifices
        .into_iter()
        .map(|square| (square, true))
        .chain(cells.into_iter().map(|square| (square, false)))
    {
        let Some(piece) = state.at(square) else {
            continue;
        };
        if let Some(existing) = victims.iter_mut().find(|(_, id, _)| *id == piece.id) {
            existing.2 |= sacrificed;
        } else {
            victims.push((square, piece.id.clone(), sacrificed));
        }
    }
    let defeated = victims
        .iter()
        .filter_map(|(square, _, _)| {
            let piece = state.at(*square)?;
            state
                .royal_identity(piece)
                .then(|| piece.color.owner())
                .flatten()
        })
        .collect::<BTreeSet<_>>();
    let mut captures = Vec::new();
    let mut removed = Vec::new();
    for (square, id, sacrificed) in victims {
        let victim = state
            .board
            .iter()
            .flatten()
            .flatten()
            .find(|piece| piece.id == id)
            .cloned()
            .ok_or_else(|| {
                EngineError::InvalidState("colossus summon victim disappeared".into())
            })?;
        if id.is_empty() {
            state.board[square.row as usize][square.col as usize] = None;
        } else {
            crate::transition::clear_piece(state, &id);
        }
        crate::transition::grant_vigilance_protection(state, &victim)?;
        if let Some(prophecy) = state.extra.get_mut("prophecy") {
            let sides = prophecy.as_object_mut().ok_or_else(|| {
                EngineError::InvalidState("colossus summon prophecy must be a player map".into())
            })?;
            for color in [Color::White, Color::Black] {
                if sides
                    .get(color.as_str())
                    .is_some_and(|entry| !entry.is_null())
                {
                    sides.insert(color.as_str().into(), Value::Null);
                }
            }
        }
        let capture_owner = if sacrificed || victim.color == summoner {
            summoner.opponent()
        } else {
            summoner
        };
        state.captures.get_mut(capture_owner).push(victim.clone());
        removed.push(crate::v7_board_hazards::EnvironmentalRemoval {
            piece: victim.clone(),
            square,
            capture_owner,
        });
        captures.push(victim);
    }
    crate::v7_board_hazards::resolve_reaper_nearby_deaths(state, &removed)?;
    let mut colossus = crate::opening::spawn(state, summoner, "colossus")?;
    capture_lock(state, &mut colossus)?;
    colossus.moved = true;
    colossus.extra.insert("anchorRow".into(), json!(anchor.row));
    colossus.extra.insert("anchorCol".into(), json!(anchor.col));
    if truthy(state.extra.get("monochromeChess")) && !truthy(colossus.extra.get("monoShade")) {
        colossus.extra.insert(
            "monoShade".into(),
            json!(if (anchor.row + anchor.col).is_multiple_of(2) {
                "light"
            } else {
                "dark"
            }),
        );
    }
    for square in cells {
        state.board[square.row as usize][square.col as usize] = Some(colossus.clone());
    }
    let threat_source = json!({"attacker":colossus,"spawn":true,"label":"거신병 소환"});
    let threat_probe = state.threat_probe_depth > 0;
    for victim in &removed {
        crate::v7_threat::mark_king_threat_removal_cause(
            state,
            &victim.piece,
            victim.square,
            &threat_source,
            threat_probe,
        )?;
    }
    if defeated.len() > 1 {
        crate::flow::end_game(state, None, "양쪽 킹이 거신병 소환 위치에 깔렸습니다.")?;
    } else if let Some(&color) = defeated.first() {
        crate::flow::end_game(
            state,
            Some(color.opponent()),
            &format!(
                "{} 킹이 거신병 소환 위치에 깔렸습니다.",
                crate::replay::label(color)
            ),
        )?;
    }
    Ok(captures)
}

fn apply_pawn_storm(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let values = action
        .target
        .as_ref()
        .and_then(Value::as_object)
        .filter(|target| target.len() == 1 && target.contains_key("selections"))
        .and_then(|target| target.get("selections"))
        .and_then(Value::as_array)
        .ok_or(EngineError::IllegalAction)?;
    if values.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    for value in values {
        let square = square_value(value)?;
        let Some(piece) = state.at(square) else {
            continue;
        };
        if seen.insert(piece.id.clone()) && pawn_storm_candidate(state, square, piece)? {
            selected.push((square, piece.id.clone()));
        }
    }
    if selected.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let direction = pawn_storm_direction(state);
    selected.sort_by_key(|(square, _)| {
        (
            if direction < 0 {
                square.row
            } else {
                7 - square.row
            },
            square.col,
        )
    });
    let color = state.turn;
    let mut moved = Vec::new();
    for (from, id) in selected {
        let Some(mut piece) = state.at(from).cloned().filter(|piece| piece.id == id) else {
            continue;
        };
        if !pawn_storm_candidate(state, from, &piece)? {
            continue;
        }
        let to = from
            .offset(direction, 0)
            .ok_or(EngineError::IllegalAction)?;
        let privacy = relocation_privacy(state, &piece, from, card)?;
        state.board[from.row as usize][from.col as usize] = None;
        piece.moved = true;
        crate::transition::mark_card_no_capture(state, &mut piece)?;
        note_ultimatum_movement(state, &mut piece)?;
        state.board[to.row as usize][to.col as usize] = Some(piece.clone());
        mark_animation(state, &piece)?;
        if crate::v7_promotion::should_promote_v7(state, &piece, to)? {
            crate::v7_promotion::auto_promote_forced_pawn_v7(state, to)?;
            piece = piece_at(state, to)?;
        }
        moved.push((
            from,
            to,
            relocation_hidden_from(state, &piece, to, &privacy)?,
        ));
    }
    if moved.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    crate::flow::mark_progress(state);
    let hidden = moved
        .iter()
        .find(|entry| !entry.2.is_empty())
        .map_or("", |entry| entry.2.as_str());
    let sound = if color == Color::White {
        "moveSelf"
    } else {
        "moveOpponent"
    };
    set_last_move(state, moved[0].0, moved[0].1, sound, color, hidden, None)?;
    state
        .extra
        .get_mut("lastMove")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState("pawn storm lastMove must be an object".into()))?
        .insert(
            "pawnStormMoves".into(),
            Value::Array(
                moved
                    .iter()
                    .map(|(from, to, _)| json!({"from":from,"to":to}))
                    .collect(),
            ),
        );
    crate::v7_threat::play_move_sound_v7(state, sound, color)?;
    crate::replay::add_log(
        state,
        format!(
            "폰 스톰: {} 폰 {}개가 즉시 한 칸씩 전진했습니다.",
            crate::replay::label(color),
            moved.len()
        ),
    )?;
    crate::replay::queue_special_effect_notation(
        state,
        color,
        &format!("폰스톰×{}", moved.len()),
        &format!(
            "{} 폰 스톰으로 폰 {}개 이동",
            crate::replay::label(color),
            moved.len()
        ),
    )?;
    Ok(Vec::new())
}

fn transform_at(state: &mut GameState, square: Square, kind: &str, animate: bool) -> Result<()> {
    let mut piece = piece_at(state, square)?;
    piece.kind = kind.into();
    mark_transformed_origin(state, &mut piece, square)?;
    piece.moved = true;
    write_piece(state, &piece);
    if animate {
        mark_animation(state, &piece)?;
    }
    Ok(())
}

fn apply_baby_bear(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let selected = require_target(state, card, action)?;
    let queen = piece_at(state, selected)?;
    mark_vanish_animation(state, &queen, selected)?;
    let sacrificed = crate::transition::sacrifice(state, selected, state.turn.opponent())?
        .ok_or(EngineError::IllegalAction)?;
    let mut bear = crate::opening::spawn(state, state.turn, "babyBear")?;
    bear.extra
        .insert("origin".into(), json!(square_name(selected)));
    bear.moved = true;
    let turn = (*state.turns_taken.get(Color::White)).min(*state.turns_taken.get(Color::Black));
    bear.extra.insert(
        "babyBearGrowAtTurn".into(),
        json!(
            turn.checked_add(7)
                .ok_or_else(|| EngineError::InvalidState(
                    "baby bear growth deadline overflow".into()
                ))?
        ),
    );
    state.board[selected.row as usize][selected.col as usize] = Some(bear.clone());
    mark_animation(state, &bear)?;
    crate::flow::mark_progress(state);
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(vec![sacrificed])
}

fn apply_death_squad(
    state: &mut GameState,
    card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    let selected = require_target(state, card, action)?;
    let squares = (0..8)
        .map(|row| Square {
            row,
            col: selected.col,
        })
        .filter(|&square| {
            state
                .at(square)
                .is_some_and(|piece| piece.color == state.turn && piece.kind == "pawn")
        })
        .collect::<Vec<_>>();
    if squares.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    for square in squares {
        transform_at(state, square, "fanatic", true)?;
    }
    Ok(Vec::new())
}

fn apply_extinction(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let selected = require_target(state, card, action)?;
    let kind = piece_at(state, selected)?.kind;
    let victims = unique_squares(state, |_, piece| Ok(piece.kind == kind))?;
    let defeated = victims
        .iter()
        .filter_map(|&square| {
            let victim = state.at(square)?;
            state
                .royal_identity(victim)
                .then(|| victim.color.owner())
                .flatten()
        })
        .collect::<BTreeSet<_>>();
    for square in victims {
        let victim = piece_at(state, square)?;
        for row in &mut state.board {
            for cell in row {
                if cell.as_ref().is_some_and(|piece| piece.id == victim.id) {
                    *cell = None;
                }
            }
        }
    }
    if defeated.len() > 1 {
        // The frozen local effect uses the literal "draw" winner, including
        // its distinct source log title, rather than the ordinary null draw.
        crate::flow::end_game_with_draw_literal(state, "멸종으로 왕족이 제거되었습니다.")?;
    } else if let Some(&color) = defeated.first() {
        crate::flow::end_game(
            state,
            Some(color.opponent()),
            "멸종으로 왕족이 제거되었습니다.",
        )?;
    }
    Ok(Vec::new())
}

fn apply_homecoming(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let selected = require_target(state, card, action)?;
    let mut piece = piece_at(state, selected)?;
    let origin = original_square(&piece).ok_or(EngineError::IllegalAction)?;
    for row in &mut state.board {
        for cell in row {
            if cell
                .as_ref()
                .is_some_and(|candidate| candidate.id == piece.id)
            {
                *cell = None;
            }
        }
    }
    for key in [
        "quantum",
        "freshNoCaptureUntil",
        "cardNoCaptureUntil",
        "quantumNoCaptureUntil",
    ] {
        piece.extra.shift_remove(key);
    }
    piece
        .extra
        .insert("coolGuyCapturedLast".into(), json!(false));
    piece.moved = true;
    state.board[origin.row as usize][origin.col as usize] = Some(piece.clone());
    mark_animation(state, &piece)?;
    crate::replay::add_piece_action_log(
        state,
        &piece,
        Some(origin),
        None,
        format!(
            "귀환: {}에서 {}로 돌아왔습니다.",
            square_name(selected),
            square_name(origin)
        ),
    )?;
    Ok(Vec::new())
}

fn apply_reformation(
    state: &mut GameState,
    _card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    require_none(action)?;
    let bishops = unique_squares(state, |_, piece| {
        Ok(piece.color == state.turn && piece.kind == "bishop")
    })?;
    if bishops.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    for square in bishops {
        transform_at(state, square, "protestant", false)?;
    }
    Ok(Vec::new())
}

fn apply_knightmate(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    require_none(action)?;
    let king = king_augment_square(state).ok_or(EngineError::IllegalAction)?;
    transform_at(state, king, "royalKnight", false)?;
    let knights = unique_squares(state, |_, piece| {
        Ok(piece.color == state.turn && piece.kind == "knight")
    })?;
    for square in knights {
        transform_at(state, square, "man", false)?;
    }
    for flag in ["knightmate", "kingKnight", "royalKnightKing"] {
        if flag == "knightmate" || flag == "royalKnightKing" && state.flag("kingKnight", state.turn)
        {
            let entry = state
                .extra
                .get_mut(flag)
                .and_then(Value::as_object_mut)
                .ok_or_else(|| unsupported(card, "knightmate state flag"))?;
            entry.insert(state.turn.as_str().into(), json!(true));
        }
    }
    Ok(Vec::new())
}

fn apply_palace(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    require_none(action)?;
    let enemy = state.turn.opponent();
    let king = unique_squares(state, |_, piece| {
        Ok(piece.color == enemy && state.royal_identity(piece))
    })?
    .into_iter()
    .next()
    .ok_or(EngineError::IllegalAction)?;
    let mut cells = Vec::new();
    for row in king.row.saturating_sub(1)..=(king.row + 1).min(7) {
        for col in king.col.saturating_sub(1)..=(king.col + 1).min(7) {
            cells.push(Square { row, col });
        }
    }
    let palaces = state
        .extra
        .get_mut("palaces")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| unsupported(card, "palaces array"))?;
    palaces.retain(|entry| entry.get("color").and_then(Value::as_str) != Some(enemy.as_str()));
    palaces.push(json!({"color":enemy,"center":king,"cells":cells}));
    Ok(Vec::new())
}

fn apply_fleeting_dream(
    state: &mut GameState,
    _card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    require_none(action)?;
    let enemy = state.turn.opponent();
    let promoted = unique_squares(state, |_, piece| {
        Ok(piece.color == enemy && truthy(piece.extra.get("promotedFromPawn")))
    })?;
    for square in promoted {
        let mut piece = piece_at(state, square)?;
        piece.kind = "pawn".into();
        piece.moved = true;
        piece.extra.shift_remove("promotedFromPawn");
        piece.extra.shift_remove("noPromotion");
        clear_promotion_inherited_traits(state, &mut piece)?;
        write_piece(state, &piece);
        mark_animation(state, &piece)?;
    }
    Ok(Vec::new())
}

fn queen_cavalry_pawn(state: &GameState) -> Option<Square> {
    let col = if truthy(state.extra.get("diagonalChess")) {
        if state.turn == Color::Black { 7 } else { 0 }
    } else {
        3
    };
    let mut pawns = (0..8)
        .filter_map(|row| {
            let square = Square { row, col };
            state
                .at(square)
                .filter(|piece| piece.color == state.turn && piece.kind == "pawn")
                .map(|_| square)
        })
        .collect::<Vec<_>>();
    if state.turn == Color::White {
        pawns.reverse();
    }
    pawns.into_iter().next()
}

fn apply_queen_cavalry(
    state: &mut GameState,
    _card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    require_none(action)?;
    let square = queen_cavalry_pawn(state).ok_or(EngineError::IllegalAction)?;
    let kind = if truthy(state.extra.get("monochromeChess")) {
        "camel"
    } else {
        "knight"
    };
    transform_at(state, square, kind, true)?;
    let mut piece = piece_at(state, square)?;
    piece.extra.insert("shielded".into(), json!(false));
    write_piece(state, &piece);
    Ok(Vec::new())
}

fn conscription_cells(state: &GameState) -> Result<Vec<Square>> {
    let row = if state.turn == Color::White { 6 } else { 1 };
    let mut cells = Vec::new();
    for col in 2..=5 {
        let square = Square { row, col };
        if crate::movement::open_placement(state, square, Some(state.turn))?
            && !crown_ground(state, square)
        {
            cells.push(square);
        }
    }
    Ok(cells)
}

fn crown_ground(state: &GameState, square: Square) -> bool {
    let Some(rule) = state.extra.get("crownRule") else {
        return false;
    };
    let crowns = rule
        .get("crowns")
        .and_then(Value::as_array)
        .filter(|crowns| !crowns.is_empty());
    let matches = |entry: &Value| {
        if !truthy(Some(entry)) || truthy(entry.get("removed")) {
            return false;
        }
        if entry == &Value::Bool(true) {
            return square == Square { row: 3, col: 3 };
        }
        entry.get("ground").is_some_and(|ground| {
            ground["row"].as_u64() == Some(u64::from(square.row))
                && ground["col"].as_u64() == Some(u64::from(square.col))
        })
    };
    crowns.map_or_else(|| matches(rule), |entries| entries.iter().any(matches))
}

fn apply_conscription(
    state: &mut GameState,
    _card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    require_none(action)?;
    let cells = conscription_cells(state)?;
    for square in cells {
        let mut pawn = crate::opening::spawn(state, state.turn, "pawn")?;
        capture_lock(state, &mut pawn)?;
        pawn.extra
            .insert("origin".into(), json!(square_name(square)));
        if truthy(state.extra.get("monochromeChess")) {
            pawn.extra.insert(
                "monoShade".into(),
                json!(if (square.row + square.col).is_multiple_of(2) {
                    "light"
                } else {
                    "dark"
                }),
            );
        }
        state.board[square.row as usize][square.col as usize] = Some(pawn);
    }
    Ok(Vec::new())
}

fn ritual_candidates(state: &GameState) -> Result<Vec<Square>> {
    unique_squares(state, |_, piece| {
        Ok(piece.color == state.turn.opponent()
            && !state.royal_identity(piece)
            && !["merchant", "wall", "colossus"].contains(&piece.kind.as_str()))
    })
}

fn shuffle_squares(state: &mut GameState, squares: &mut [Square]) -> Result<()> {
    for i in (1..squares.len()).rev() {
        let j = (state.rng.sample()? * (i + 1) as f64).floor() as usize;
        state
            .rng
            .record_last_probability(1.0 / (i + 1) as f64, "source Fanatical Ritual shuffle")?;
        squares.swap(i, j.min(i));
    }
    Ok(())
}

fn apply_fanatical_ritual(
    state: &mut GameState,
    _card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    require_none(action)?;
    let mut candidates = ritual_candidates(state)?;
    if candidates.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    shuffle_squares(state, &mut candidates)?;
    transform_at(state, candidates[0], "fanatic", true)?;
    Ok(Vec::new())
}

fn vortex_candidates(state: &GameState) -> Result<Vec<Square>> {
    unique_squares(state, |_, piece| {
        Ok(piece.color == state.turn.opponent()
            && !["pawn", "wall", "colossus", "bigRook", "bigBishop"].contains(&piece.kind.as_str())
            && !state.royal_identity(piece))
    })
}

fn apply_vortex(state: &mut GameState, _card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    require_none(action)?;
    let cells = vortex_candidates(state)?;
    if cells.len() < 2 {
        return Err(EngineError::IllegalAction);
    }
    let mut pieces = cells
        .iter()
        .map(|&square| piece_at(state, square))
        .collect::<Result<Vec<_>>>()?;
    for i in (1..pieces.len()).rev() {
        let j = (state.rng.sample()? * (i + 1) as f64).floor() as usize;
        state
            .rng
            .record_last_probability(1.0 / (i + 1) as f64, "source Vortex permutation")?;
        pieces.swap(i, j.min(i));
    }
    for (square, piece) in cells.into_iter().zip(pieces) {
        state.board[square.row as usize][square.col as usize] = Some(piece.clone());
        mark_animation(state, &piece)?;
    }
    Ok(Vec::new())
}

fn apply_black_magic(
    state: &mut GameState,
    _card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    require_none(action)?;
    let square = black_magic_candidates(state)?
        .into_iter()
        .next()
        .ok_or(EngineError::IllegalAction)?;
    let mut monster = crate::opening::spawn(state, state.turn, "monster")?;
    monster.moved = true;
    monster.extra.insert("alliedMonster".into(), json!(true));
    monster
        .extra
        .insert("blackMagicMonster".into(), json!(true));
    monster
        .extra
        .insert("blackMagicOwner".into(), json!(state.turn));
    monster
        .extra
        .insert("origin".into(), json!(square_name(square)));
    state.board[square.row as usize][square.col as usize] = Some(monster.clone());
    mark_animation(state, &monster)?;
    Ok(Vec::new())
}

fn apply_schrodinger_pawns(
    state: &mut GameState,
    _card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    require_none(action)?;
    let mut candidates = schrodinger_candidates(state);
    if candidates.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    for i in (1..candidates.len()).rev() {
        let j = (state.rng.sample()? * (i + 1) as f64).floor() as usize;
        state
            .rng
            .record_last_probability(1.0 / (i + 1) as f64, "source Schrodinger pawn shuffle")?;
        candidates.swap(i, j.min(i));
    }
    for (source, destination) in candidates.into_iter().take(4) {
        let mut pawn = piece_at(state, source)?;
        pawn.extra.insert("quantum".into(), json!(destination));
        let turns = *state.turns_taken.get(state.turn);
        pawn.extra.insert(
            "quantumNoCaptureUntil".into(),
            json!(turns.checked_add(1).ok_or_else(|| {
                EngineError::InvalidState("Schrodinger pawn deadline overflow".into())
            })?),
        );
        pawn.extra
            .insert("quantumFirstObservationFails".into(), json!(true));
        write_piece(state, &pawn);
        mark_animation(state, &pawn)?;
    }
    Ok(Vec::new())
}

// The client keeps direct references to these objects across grouped Reaper
// callbacks. A reference can now live in captures, or survive only locally.
fn retained_piece(state: &GameState, original: &Piece) -> Piece {
    if original.id.is_empty() {
        return original.clone();
    }
    state
        .board
        .iter()
        .flatten()
        .flatten()
        .chain(state.captures.white.iter())
        .chain(state.captures.black.iter())
        .find(|piece| piece.id == original.id)
        .cloned()
        .unwrap_or_else(|| original.clone())
}

fn apply_martyrdom(state: &mut GameState, _card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    require_none(action)?;
    let bishops = unique_squares(state, |_, piece| {
        Ok(piece.color == state.turn && piece.kind == "bishop")
    })?;
    let pawns = unique_squares(state, |_, piece| {
        Ok(piece.color == state.turn && piece.kind == "pawn")
    })?;
    if bishops.is_empty() || pawns.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let pawns = pawns
        .into_iter()
        .map(|square| piece_at(state, square))
        .collect::<Result<Vec<_>>>()?;
    let capture_owner = state.turn.opponent();
    let mut removed = Vec::with_capacity(bishops.len());
    for square in bishops {
        let captured = piece_at(state, square)?;
        state.board[square.row as usize][square.col as usize] = None;
        crate::transition::grant_vigilance_protection(state, &captured)?;
        crate::transition::cancel_prophecies_by_capture(state)?;
        state.captures.get_mut(capture_owner).push(captured.clone());
        removed.push(crate::v7_board_hazards::EnvironmentalRemoval {
            piece: captured,
            square,
            capture_owner,
        });
    }
    crate::v7_board_hazards::resolve_reaper_nearby_deaths(state, &removed)?;
    for original in pawns {
        let mut pawn = retained_piece(state, &original);
        pawn.extra.insert("shielded".into(), json!(true));
        crate::v7_board_hazards::replace_object_aliases(state, &pawn);
        mark_animation(state, &pawn)?;
    }
    Ok(removed
        .into_iter()
        .map(|entry| retained_piece(state, &entry.piece))
        .collect())
}

fn merchant_guild_plan(state: &GameState) -> Option<(Square, Square)> {
    // Source findPiece(turn, "king") returns the first royal-king identity in
    // board traversal order. The September 18 balance also admits merchants.
    let king = (0..8).find_map(|row| {
        (0..8).find_map(|col| {
            let square = Square { row, col };
            let piece = state.at(square)?;
            (piece.color == state.turn && royal_king_identity(state, piece)).then_some(square)
        })
    })?;
    let king_piece = state.at(king)?;
    if truthy(king_piece.extra.get("undergroundBunker"))
        && king_piece
            .extra
            .get("hp")
            .and_then(Value::as_f64)
            .is_some_and(f64::is_finite)
    {
        return None;
    }
    let reversed = state
        .extra
        .get("effects")
        .and_then(|effects| effects.get("pawnReverse"))
        .and_then(|reverse| reverse.get(state.turn.as_str()))
        .and_then(Value::as_f64)
        .is_some_and(|remaining| remaining > 0.0);
    let direction = state.turn.pawn_dir() * if reversed { -1 } else { 1 };
    let target = king.offset(direction * 2, 0)?;
    if state
        .at(target)
        .is_some_and(|occupant| occupant.is_large() || occupant.color != state.turn)
    {
        return None;
    }
    Some((king, target))
}

fn apply_merchant_guild(
    state: &mut GameState,
    _card: &CardSlot,
    action: &Action,
) -> Result<Vec<Piece>> {
    require_none(action)?;
    let (source, target) = merchant_guild_plan(state).ok_or(EngineError::IllegalAction)?;
    let original = piece_at(state, source)?;
    let mut captures = Vec::new();
    if let Some(victim) = state
        .at(target)
        .cloned()
        .filter(|piece| piece.id != original.id)
    {
        let capture_owner = state.turn.opponent();
        // Merchant Guild's source sacrifice intentionally has no Vigilance
        // callback and resolves recycling before clearing the landing cell.
        crate::transition::cancel_prophecies_by_capture(state)?;
        state.captures.get_mut(capture_owner).push(victim.clone());
        crate::v7_promotion::resolve_recycling_holdout_after_queen_loss_v7(state, &victim)?;
        state.board[target.row as usize][target.col as usize] = None;
        crate::v7_board_hazards::resolve_reaper_nearby_deaths(
            state,
            &[crate::v7_board_hazards::EnvironmentalRemoval {
                piece: victim.clone(),
                square: target,
                capture_owner,
            }],
        )?;
        captures.push(victim);
    }
    let mut merchant = retained_piece(state, &original);
    state.board[source.row as usize][source.col as usize] = None;
    merchant.kind = "merchant".into();
    merchant.extra.insert("gold".into(), json!(0));
    merchant.moved = true;
    for key in ["freshNoCaptureUntil", "undergroundBunker", "hp", "maxHp"] {
        merchant.extra.shift_remove(key);
    }
    crate::v7_board_hazards::replace_object_aliases(state, &merchant);
    state.board[target.row as usize][target.col as usize] = Some(merchant);
    Ok(captures)
}

fn traitor_candidates(state: &GameState) -> Result<Vec<Square>> {
    // This validates the catalog profile before applying the frozen client's
    // moved-only predicate. Profiles not covered by the shared combat-value
    // table fail closed; its pinned hash also uses this predicate.
    let _ = crate::eligibility::v7_piece_combat_value(state, "pawn")?;
    let moved_only = september18_balance(state);
    let enemy = state.turn.opponent();
    let mut pieces = Vec::new();
    let mut pawns = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(piece) = state.at(square) else {
                continue;
            };
            if piece.color != enemy
                || moved_only && !piece.moved
                || state.royal_identity(piece)
                || ["merchant", "wall", "football", "colossus"].contains(&piece.kind.as_str())
            {
                continue;
            }
            pieces.push(square);
            if piece.kind == "pawn" {
                pawns.push(square);
            }
        }
    }
    if !pawns.is_empty() {
        return Ok(pawns);
    }
    if pieces.is_empty() {
        // Source first tries findPiece(enemy, "king") and, if that first
        // recipient is ineligible, scans all royal identities again. A
        // regency heir participates only in the second scan.
        let first_king = (0..8)
            .flat_map(|row| (0..8).map(move |col| Square { row, col }))
            .find(|&square| {
                state
                    .at(square)
                    .is_some_and(|piece| piece.color == enemy && royal_king_identity(state, piece))
            })
            .filter(|&square| {
                state
                    .at(square)
                    .is_some_and(|piece| !moved_only || piece.moved)
            });
        let royal = first_king.or_else(|| {
            (0..8)
                .flat_map(|row| (0..8).map(move |col| Square { row, col }))
                .find(|&square| {
                    state.at(square).is_some_and(|piece| {
                        piece.color == enemy
                            && state.royal_identity(piece)
                            && (!moved_only || piece.moved)
                    })
                })
        });
        return Ok(royal.into_iter().collect());
    }
    let mut valued = Vec::new();
    for square in pieces {
        let piece = state.at(square).ok_or(EngineError::InvalidState(
            "traitor candidate disappeared".into(),
        ))?;
        if let Some(value) = crate::eligibility::v7_piece_combat_value(state, &piece.kind)? {
            valued.push((square, value));
        }
    }
    let Some(minimum) = valued.iter().map(|(_, value)| *value).min() else {
        return Ok(Vec::new());
    };
    Ok(valued
        .into_iter()
        .filter_map(|(square, value)| (value == minimum).then_some(square))
        .collect())
}

fn royal_king_identity(state: &GameState, piece: &Piece) -> bool {
    piece.kind == "merchant" && september18_balance(state)
        || truthy(piece.extra.get("crownRoyal"))
        || truthy(piece.extra.get("editorRoyal"))
        || ["king", "royalKnight", "shotgunKing", "darkWizard"].contains(&piece.kind.as_str())
}

fn apply_traitor(state: &mut GameState, _card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    require_none(action)?;
    let candidates = traitor_candidates(state)?;
    if candidates.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let royal = state
        .at(candidates[0])
        .is_some_and(|piece| state.royal_identity(piece));
    // The royal fallback is deterministic and consumes no randomChoice.
    let choice = if royal {
        0
    } else {
        let index = (state.rng.sample()? * candidates.len() as f64).floor() as usize;
        state
            .rng
            .record_last_probability(1.0 / candidates.len() as f64, "source Traitor recipient")?;
        index
    };
    let square = candidates[choice.min(candidates.len() - 1)];
    let mut piece = piece_at(state, square)?;
    let converted = piece.clone();
    piece.color = state.turn.into();
    mark_transformed_origin_with_options(state, &mut piece, square, true)?;
    piece.extra.shift_remove("freshNoCaptureUntil");
    piece
        .extra
        .insert("coolGuyCapturedLast".into(), json!(false));
    write_piece(state, &piece);
    mark_animation(state, &piece)?;
    if royal {
        let threat_probe = state.threat_probe_depth > 0;
        crate::v7_threat::mark_king_threat_removal_cause(
            state,
            &converted,
            square,
            &json!({"label":"변절자"}),
            threat_probe,
        )?;
        crate::transition::resolve_royal_capture(state, &converted, state.turn)?;
    }
    Ok(Vec::new())
}

fn qxe1_royal_king(piece: &Piece) -> bool {
    piece.kind == "merchant"
        || truthy(piece.extra.get("crownRoyal"))
        || truthy(piece.extra.get("editorRoyal"))
        || ["king", "royalKnight", "shotgunKing", "darkWizard"].contains(&piece.kind.as_str())
}

fn qxe1_plan(state: &GameState) -> Option<(Square, Square)> {
    let mut plans = Vec::new();
    for king_row in 0..8 {
        for king_col in 0..8 {
            let throne = Square {
                row: king_row,
                col: king_col,
            };
            if !state
                .at(throne)
                .is_some_and(|piece| piece.color == state.turn && qxe1_royal_king(piece))
            {
                continue;
            }
            let mut seen = BTreeSet::new();
            for queen_row in 0..8 {
                for queen_col in 0..8 {
                    let source = Square {
                        row: queen_row,
                        col: queen_col,
                    };
                    let Some(queen) = state
                        .at(source)
                        .filter(|piece| piece.color == state.turn && piece.kind == "queen")
                    else {
                        continue;
                    };
                    if !seen.insert(queen.id.clone()) {
                        continue;
                    }
                    let dr = i16::from(throne.row) - i16::from(source.row);
                    let dc = i16::from(throne.col) - i16::from(source.col);
                    if dr != 0 && dc != 0 && dr.abs() != dc.abs() {
                        continue;
                    }
                    let row_step = dr.signum();
                    let col_step = dc.signum();
                    let mut row = i16::from(source.row) + row_step;
                    let mut col = i16::from(source.col) + col_step;
                    let mut blocked = false;
                    while row != i16::from(throne.row) || col != i16::from(throne.col) {
                        if state
                            .at(Square {
                                row: row as u8,
                                col: col as u8,
                            })
                            .is_some()
                        {
                            blocked = true;
                            break;
                        }
                        row += row_step;
                        col += col_step;
                    }
                    if !blocked {
                        plans.push((source, throne));
                    }
                }
            }
        }
    }
    plans.into_iter().min_by_key(|&(source, throne)| {
        (
            source
                .row
                .abs_diff(throne.row)
                .max(source.col.abs_diff(throne.col)),
            throne.row,
            throne.col,
            source.row,
            source.col,
        )
    })
}

fn color_flag(state: &mut GameState, field: &str, color: Color, value: bool) -> Result<()> {
    let entries = state
        .extra
        .get_mut(field)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState(format!("qxe1 missing {field} side flags")))?;
    entries.insert(color.as_str().into(), json!(value));
    Ok(())
}

fn apply_qxe1(state: &mut GameState, _card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    require_none(action)?;
    let (source, throne) = qxe1_plan(state).ok_or(EngineError::IllegalAction)?;
    let mut queen = piece_at(state, source)?;
    let king = piece_at(state, throne)?;
    let color = state.turn;
    set_last_move(state, source, throne, "capture", color, "", None)?;
    for id in [&queen.id, &king.id] {
        for row in &mut state.board {
            for cell in row {
                if cell.as_ref().is_some_and(|piece| piece.id == *id) {
                    *cell = None;
                }
            }
        }
    }
    for row in &mut state.board {
        for cell in row {
            if let Some(piece) = cell.as_mut()
                && piece.color == color
            {
                piece.extra.shift_remove("regencyHeir");
            }
        }
    }
    if truthy(king.extra.get("undergroundBunker")) {
        queen.extra.insert("undergroundBunker".into(), json!(true));
        let hp = king
            .extra
            .get("hp")
            .and_then(Value::as_f64)
            .unwrap_or(5.0)
            .max(1.0);
        let max_hp = king
            .extra
            .get("maxHp")
            .and_then(Value::as_f64)
            .unwrap_or(5.0)
            .max(hp);
        queen.extra.insert("hp".into(), json!(hp));
        queen.extra.insert("maxHp".into(), json!(max_hp));
    }
    if let Some(last_resistance) = king.extra.get("lastResistance") {
        queen
            .extra
            .insert("lastResistance".into(), last_resistance.clone());
        queen.extra.insert(
            "protected".into(),
            json!(truthy(king.extra.get("protected"))),
        );
    }
    if let Some(imperial_moves) = king.extra.get("imperialMoves").and_then(Value::as_array) {
        let mut moves = Vec::new();
        for movement in imperial_moves {
            let text = movement
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| movement.to_string());
            if !moves.contains(&text) {
                moves.push(text);
            }
        }
        queen.extra.insert("imperialMoves".into(), json!(moves));
    }
    queen.extra.insert("regencyHeir".into(), json!(true));
    queen.moved = true;
    state.board[throne.row as usize][throne.col as usize] = Some(queen.clone());
    color_flag(state, "regency", color, true)?;
    color_flag(state, "kingDead", color, true)?;
    mark_animation(state, &queen)?;
    crate::v7_threat::play_move_sound_v7(state, "capture", color)?;
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn diagnostic_value(value: &Value) -> String {
        let serialized = value.to_string();
        if serialized.chars().count() <= 240 {
            serialized
        } else {
            let prefix = serialized.chars().take(240).collect::<String>();
            format!("{prefix}... ({} UTF-8 bytes)", serialized.len())
        }
    }

    fn differences(actual: &Value, expected: &Value, path: &str, output: &mut Vec<String>) {
        if output.len() >= 12 || actual == expected {
            return;
        }
        match (actual, expected) {
            (Value::Object(a), Value::Object(b)) => {
                let keys = a.keys().chain(b.keys()).collect::<BTreeSet<_>>();
                for key in keys {
                    if output.len() >= 12 {
                        break;
                    }
                    let path = format!("{path}.{key}");
                    match (a.get(key), b.get(key)) {
                        (Some(actual), Some(expected)) => {
                            differences(actual, expected, &path, output);
                        }
                        (None, Some(expected)) => output.push(format!(
                            "{path}: missing actual field; expected {}",
                            diagnostic_value(expected)
                        )),
                        (Some(actual), None) => output.push(format!(
                            "{path}: unexpected actual field {}",
                            diagnostic_value(actual)
                        )),
                        (None, None) => unreachable!("union key exists on at least one side"),
                    }
                }
            }
            (Value::Array(a), Value::Array(b)) => {
                if a.len() != b.len() {
                    output.push(format!("{path}.length {} != {}", a.len(), b.len()));
                }
                for (index, (actual, expected)) in a.iter().zip(b).enumerate() {
                    differences(actual, expected, &format!("{path}[{index}]"), output);
                }
            }
            _ => output.push(format!(
                "{path}: {} != {}",
                diagnostic_value(actual),
                diagnostic_value(expected)
            )),
        }
    }

    fn assert_jcs_equal(actual: &Value, expected: &Value, path: &str, label: &str) {
        let actual = serde_jcs::to_vec(actual).expect("native comparison value is JCS data");
        let expected = serde_jcs::to_vec(expected).expect("source comparison value is JCS data");
        if actual != expected {
            // Compare the complete JCS bytes. The bounded field list is only
            // a diagnostic, so missing fields and explicit null stay distinct.
            let actual: Value = serde_json::from_slice(&actual).unwrap();
            let expected: Value = serde_json::from_slice(&expected).unwrap();
            let mut diff = Vec::new();
            differences(&actual, &expected, path, &mut diff);
            panic!("{label}: {path} diverged (first 12 field differences): {diff:?}");
        }
    }

    fn position_identity(position: &Value) -> String {
        let mut content = position
            .as_object()
            .expect("receipt position is an object")
            .clone();
        content.remove("positionId");
        format!(
            "{:x}",
            Sha256::digest(serde_jcs::to_vec(&Value::Object(content)).unwrap())
        )
    }

    fn assert_receipt_position(position: &Value, label: &str) {
        assert_eq!(
            position["protocolVersion"], "accelerate-position-v1",
            "{label}: protocol"
        );
        assert_eq!(
            position["rulesVersion"], RULES_VERSION_V7,
            "{label}: rules version"
        );
        assert_eq!(
            position["catalogVersion"],
            crate::v7_execution_profile::catalog_version().unwrap(),
            "{label}: catalog version"
        );
        assert_eq!(
            position["positionId"],
            position_identity(position),
            "{label}: Position ID"
        );
    }

    #[test]
    fn topology_ids_are_unique_and_pinned() {
        let mut seen = BTreeSet::new();
        assert_eq!(IDS.len(), 24);
        for id in IDS {
            assert!(seen.insert(*id), "duplicate topology card {id}");
            let effect = EFFECTS.iter().find(|entry| entry.0 == *id).unwrap().1;
            let definition = crate::card_registry::definition_for(RULES_VERSION_V7, id).unwrap();
            assert_eq!(definition.effect, effect, "source effect for {id}");
        }
    }

    /// Generated outside Git from the exact frozen client. The direct-effect
    /// boundary excludes the caller's finishCard and common reconciliation.
    #[test]
    fn frozen_direct_effects_when_receipt_is_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_CARD_TOPOLOGY_CASES") else {
            return;
        };
        let data = std::fs::read_to_string(path).unwrap();
        let mut checked = 0;
        for (index, line) in data.lines().enumerate() {
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            let id = receipt["id"].as_str().unwrap();
            assert!(IDS.contains(&id), "receipt {index}: unexpected card {id}");
            assert_eq!(receipt["sourceDirectResult"]["ok"], true);
            let case_key = receipt.get("caseKey").and_then(Value::as_str).unwrap_or(id);
            let label = format!("receipt {index} card {id}/{case_key}");
            assert_receipt_position(&receipt["sourcePosition"], &format!("{label} input"));
            assert_receipt_position(
                &receipt["sourceDirectPosition"],
                &format!("{label} expected"),
            );
            let mut state: GameState =
                serde_json::from_value(receipt["sourcePosition"]["state"].clone()).unwrap();
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(receipt["sourcePosition"]["rng"].clone()).unwrap();
            state.threat_probe_depth = receipt
                .get("sourceThreatProbe")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .try_into()
                .unwrap();
            state.history = receipt["sourcePosition"]["history"]
                .as_array()
                .expect("source receipt has history")
                .clone();
            let card = state
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
            apply(&mut state, &card, &action)
                .unwrap_or_else(|error| panic!("{label}: native rejected source action: {error}"));
            // OracleRuntime.snapshot drains the queued source terminal replay
            // after this direct callback. Keep settlement at the comparison
            // boundary so ordinary card callers can finish their bookkeeping.
            crate::replay::settle(&mut state).unwrap_or_else(|error| {
                panic!("{label}: native snapshot replay settlement failed: {error}")
            });
            let mut actual = serde_json::to_value(&state).unwrap();
            for envelope in ["rulesetId", "rng", "history"] {
                actual.as_object_mut().unwrap().remove(envelope);
            }
            // The source writes integral `stars` as JSON 4 while CardSlot
            // stores it as f64. JCS normalizes both to the same wire number.
            assert_jcs_equal(
                &actual,
                &receipt["sourceDirectPosition"]["state"],
                "state",
                &label,
            );
            let rng = serde_json::to_value(&state.rng).unwrap();
            let history = serde_json::to_value(&state.history).unwrap();
            assert_jcs_equal(&rng, &receipt["sourceDirectPosition"]["rng"], "rng", &label);
            assert_jcs_equal(
                &history,
                &receipt["sourceDirectPosition"]["history"],
                "history",
                &label,
            );
            let mut position = json!({
                "protocolVersion":"accelerate-position-v1",
                "rulesVersion":state.ruleset_id,
                "catalogVersion":crate::v7_execution_profile::catalog_version().unwrap(),
                "state":actual,
                "rng":rng,
                "history":history,
            });
            position["positionId"] = json!(position_identity(&position));
            assert_jcs_equal(
                &position,
                &receipt["sourceDirectPosition"],
                "position",
                &label,
            );
            checked += 1;
        }
        assert!(checked > 0, "topology source receipt is empty");
    }
}
