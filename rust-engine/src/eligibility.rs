//! Source card draw predicates. Availability is not always pure in the client:
//! trolley and black-box evaluate a shuffled subset family while scanning a
//! weighted pool. Those draws are intentionally consumed in source order.
use crate::*;
use serde_json::Value;
use std::{collections::BTreeSet, sync::OnceLock};

fn truth(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_f64().is_some_and(|v| v != 0.0),
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}
fn side(state: &GameState, name: &str, color: Color) -> bool {
    truth(
        state
            .extra
            .get(name)
            .and_then(|value| value.get(color.as_str())),
    )
}
fn has(piece: &Piece, name: &str) -> bool {
    truth(piece.extra.get(name))
}
fn entries(state: &GameState) -> Vec<(Square, &Piece)> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            if let Some(piece) = state.at(square)
                && seen.insert(&piece.id)
            {
                result.push((square, piece));
            }
        }
    }
    result
}
fn any(state: &GameState, color: Color, test: impl Fn(&Piece, Square) -> bool) -> bool {
    entries(state)
        .into_iter()
        .any(|(square, piece)| piece.color == color && test(piece, square))
}
fn count(state: &GameState, color: Color, kind: &str) -> usize {
    entries(state)
        .into_iter()
        .filter(|(_, p)| p.color == color && p.kind == kind)
        .count()
}
fn royal(state: &GameState, piece: &Piece) -> bool {
    state.royal_identity(piece)
}
fn queen(piece: &Piece) -> bool {
    piece.kind == "queen" && !has(piece, "regencyHeir")
}
fn minor(kind: &str) -> bool {
    matches!(
        kind,
        "knight"
            | "bishop"
            | "camel"
            | "clockwork"
            | "parrot"
            | "wizard"
            | "recruiter"
            | "trickster"
    )
}
fn major(kind: &str) -> bool {
    matches!(
        kind,
        "octopus"
            | "grappler"
            | "hedgehog"
            | "princess"
            | "bigBishop"
            | "queen"
            | "rook"
            | "amazon"
            | "man"
            | "colossus"
            | "bigRook"
            | "herald"
            | "jester"
            | "hook"
            | "primeMinister"
            | "assassin"
            | "windmill"
            | "crown"
            | "bear"
            | "magicGirl"
            | "berserker"
            | "siren"
            | "reaper"
            | "undead"
    )
}
fn ability(piece: &Piece) -> &str {
    if piece.kind == "trickster" {
        piece
            .extra
            .get("tricksterMoveType")
            .and_then(Value::as_str)
            .unwrap_or(&piece.kind)
    } else {
        &piece.kind
    }
}
fn ranged(state: &GameState, piece: &Piece) -> bool {
    let color = piece.color.owner();
    match ability(piece) {
        "brutus" | "bigBishop" | "rook" | "bishop" | "queen" | "bear" | "amazon" | "cardinal"
        | "cannon" | "herald" | "hook" | "protestant" | "windmill" | "windmillBishop"
        | "windmillRook" | "bigRook" | "jester" | "idol" => true,
        "princess" => color.is_some_and(|color| count(state, color, "queen") == 0),
        "magicGirl" => color.is_some_and(|color| side(state, "magicGirlSurge", color)),
        "berserker" => color.is_some_and(|color| {
            entries(state)
                .iter()
                .filter(|(_, p)| p.color == color)
                .count()
                <= 8
        }),
        _ => false,
    }
}
// Actual ENCYCLOPEDIA_PIECE_VALUES, including the September26 overrides. Kings
// and catalog types absent from that map have no finite combat value.
fn worth(kind: &str) -> Option<u32> {
    Some(match kind {
        "pawn" | "fanatic" | "squire" | "checker" | "alfil" => 1,
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
    })
}
fn parsed_square(value: &str) -> Option<Square> {
    let bytes = value.as_bytes();
    if bytes.len() == 2 && (b'a'..=b'h').contains(&bytes[0]) && (b'1'..=b'8').contains(&bytes[1]) {
        Some(Square {
            row: b'8' - bytes[1],
            col: bytes[0] - b'a',
        })
    } else {
        None
    }
}
fn origin(piece: &Piece, square: Square) -> Square {
    piece
        .extra
        .get("origin")
        .and_then(Value::as_str)
        .and_then(parsed_square)
        .unwrap_or(square)
}
fn direction(state: &GameState, color: Color) -> i8 {
    let reversed = state
        .extra
        .get("effects")
        .and_then(|v| v.get("pawnReverse"))
        .and_then(|v| v.get(color.as_str()))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        > 0;
    color.pawn_dir() * if reversed { -1 } else { 1 }
}
fn occupied_quantum(state: &GameState, square: Square) -> bool {
    entries(state).iter().any(|(_, p)| {
        p.extra.get("quantum").is_some_and(|q| {
            let row = q.get("row").and_then(Value::as_u64);
            let col = q.get("col").and_then(Value::as_u64);
            match (row, col) {
                (Some(row), Some(col)) if row < 8 && col < 8 => {
                    square.row as u64 >= row
                        && square.col as u64 >= col
                        && square.row as u64 <= row + u64::from(p.is_large())
                        && square.col as u64 <= col + u64::from(p.is_large())
                }
                _ => false,
            }
        })
    })
}
fn reserved(state: &GameState, square: Square, installation: bool) -> bool {
    ["pendingScarecrows", "pendingLobsters"].iter().any(|name| {
        state
            .extra
            .get(*name)
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry["row"].as_u64() == Some(u64::from(square.row))
                        && entry["col"].as_u64() == Some(u64::from(square.col))
                        && (*name == "pendingLobsters" || !truth(entry.get("pieceId")))
                })
            })
    }) || state
        .extra
        .get("pendingPortals")
        .and_then(Value::as_array)
        .is_some_and(|plans| {
            plans.iter().any(|plan| {
                (installation || plan.get("blocksMovement").and_then(Value::as_bool) == Some(true))
                    && plan
                        .get("cells")
                        .and_then(Value::as_array)
                        .is_some_and(|cells| {
                            cells.iter().any(|cell| {
                                cell["row"].as_u64() == Some(u64::from(square.row))
                                    && cell["col"].as_u64() == Some(u64::from(square.col))
                            })
                        })
            })
        })
}
fn open(state: &GameState, square: Square) -> bool {
    state.at(square).is_none()
        && !occupied_quantum(state, square)
        && !reserved(state, square, false)
}
fn placements(state: &GameState) -> Vec<Square> {
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .filter(|square| {
            state.at(*square).is_none()
                && !occupied_quantum(state, *square)
                && !reserved(state, *square, true)
        })
        .collect()
}
fn pawn_ahead(state: &GameState, color: Color, steps: i8) -> bool {
    any(state, color, |piece, square| {
        piece.kind == "pawn"
            && !crate::movement::frozen(piece)
            && piece
                .extra
                .get("staked")
                .and_then(|v| v.get("remaining"))
                .and_then(Value::as_i64)
                .unwrap_or(0)
                <= 0
            && (1..=steps).all(|step| {
                square
                    .offset(direction(state, color) * step, 0)
                    .is_some_and(|to| open(state, to))
            })
    })
}
fn active_ids(state: &GameState, color: Option<Color>) -> BTreeSet<String> {
    [Color::White, Color::Black]
        .into_iter()
        .filter(|side| color.is_none_or(|wanted| wanted == *side))
        .flat_map(|side| state.deck_slots.get(side).iter())
        .filter(|card| !card.vacant)
        .map(|card| card.id.clone())
        .collect()
}
fn passive(card: &Value) -> bool {
    static IDS: OnceLock<BTreeSet<String>> = OnceLock::new();
    IDS.get_or_init(|| {
        let catalog: Value =
            serde_json::from_str(include_str!("../../bridge/catalog/site-20260927.json"))
                .expect("catalog");
        catalog["cards"]
            .as_array()
            .expect("cards")
            .iter()
            .filter(|meta| meta["activation"] == "PASSIVE")
            .map(|meta| meta["id"].as_str().expect("id").into())
            .collect()
    })
    .contains(card["id"].as_str().unwrap_or(""))
}
fn trolley(state: &mut GameState, color: Color) -> Result<bool> {
    let mut values = entries(state)
        .into_iter()
        .filter(|(_, p)| {
            p.color == color
                && !royal(state, p)
                && !matches!(p.kind.as_str(), "wall" | "football" | "blackHole")
        })
        .filter_map(|(_, p)| worth(&p.kind))
        .collect::<Vec<_>>();
    // The source shuffles BEFORE removing >10 values or truncating to 32.
    for index in (1..values.len()).rev() {
        let other = (state.rng.sample()? * (index + 1) as f64).floor() as usize;
        values.swap(index, other);
    }
    values.retain(|value| *value > 0 && *value <= 10);
    values.truncate(32);
    let mut buckets: [Vec<u32>; 11] = std::array::from_fn(|_| Vec::new());
    // Bounded source walk: at most 4 distinct pieces and combat sum 10. The
    // per-score cap of 240 is part of source availability, not arbitrary pruning.
    fn walk(
        values: &[u32],
        start: usize,
        mask: u32,
        count: u8,
        score: u32,
        buckets: &mut [Vec<u32>; 11],
    ) {
        if score >= 2 && buckets[score as usize].len() < 240 {
            buckets[score as usize].push(mask);
        }
        if count >= 4 {
            return;
        }
        for index in start..values.len() {
            let sum = score + values[index];
            if sum <= 10 {
                walk(
                    values,
                    index + 1,
                    mask | (1_u32 << index),
                    count + 1,
                    sum,
                    buckets,
                );
            }
        }
    }
    walk(&values, 0, 0, 0, 0, &mut buckets);
    for low in 2..=8 {
        for high in low..=10.min(low + 2) {
            if buckets[low]
                .iter()
                .any(|left| buckets[high].iter().any(|right| left & right == 0))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn random_box(state: &mut GameState, color: Color, want_passive: bool, depth: u8) -> Result<bool> {
    let own = active_ids(state, Some(color));
    let acquired = active_ids(state, None);
    let mut available = false;
    for candidate in &crate::draft::definitions_for_ruleset(&state.ruleset_id)?.definitions {
        let id = candidate["id"].as_str().expect("definition id");
        let is_passive = passive(candidate);
        if is_passive != want_passive
            || matches!(id, "white-box" | "black-box" | "shotgun-king")
            || acquired.contains(id)
            || crate::draft::conflicts_with_ruleset(&state.ruleset_id, id, &own)
            || crate::draft::category_with_ruleset(&state.ruleset_id, candidate) == "RULE"
            || (!want_passive
                && (crate::draft::category_with_ruleset(&state.ruleset_id, candidate) == "GUN"
                    || matches!(id, "horde" | "london-system" | "big-rook" | "initiative")))
            || (want_passive && matches!(id, "big-rook" | "big-bishop"))
        {
            continue;
        }
        if want_passive
            && matches!(id, "democracy" | "queens-gambit")
            && own.contains(if id == "democracy" {
                "queens-gambit"
            } else {
                "democracy"
            })
        {
            continue;
        }
        // Source filter evaluates every candidate, including after the first
        // success. Short-circuiting here would omit trolley's RNG consumption.
        available |= drawable(state, candidate, color, false, depth + 1)?;
    }
    Ok(available)
}

pub(crate) fn draft_drawable(state: &mut GameState, card: &Value, color: Color) -> Result<bool> {
    let id = card["id"]
        .as_str()
        .ok_or_else(|| EngineError::InvalidState("card definition id missing".into()))?;
    if state
        .extra
        .get("cardBanIds")
        .and_then(Value::as_array)
        .is_some_and(|ids| ids.contains(&serde_json::json!(id)))
    {
        return Ok(false);
    }
    if matches!(
        id,
        "insight"
            | "joker"
            | "zugzwang"
            | "othello"
            | "homecoming"
            | "judgment"
            | "necromancy"
            | "canceling"
    ) {
        return Ok(true);
    }
    drawable(state, card, color, true, 0)
}
fn drawable(
    state: &mut GameState,
    card: &Value,
    color: Color,
    for_draft: bool,
    depth: u8,
) -> Result<bool> {
    if depth > 4 {
        return Err(EngineError::InvalidState(
            "cyclic card availability dependency".into(),
        ));
    }
    let id = card["id"].as_str().expect("definition id");
    let own = active_ids(state, Some(color));
    if crate::draft::conflicts_with_ruleset(&state.ruleset_id, id, &own) {
        return Ok(false);
    }
    let enemy = color.opponent();
    let own_type = |kind| count(state, color, kind) > 0;
    let own_royal = || any(state, color, |p, _| royal(state, p));
    let own_queen = || any(state, color, |p, _| queen(p));
    let nonroyal = |p: &Piece| !royal(state, p);
    let answer = match id {
        "frenzy" | "last-stand" | "early-promotion" | "fast-growth" | "otherworld" | "sprint"
        | "leap" | "retreat" | "underpromotion" | "final-weapon" | "en-passant-bang" | "ghost"
        | "breakthrough-order" | "death-squad" => Some(own_type("pawn")),
        "vip" => Some(own_type("pawn") || count(state, enemy, "pawn") > 0),
        "big-bishop" | "big-rook" => Some(own_type("king")),
        "constitutional-monarchy"
        | "jester"
        | "wizard"
        | "reaper"
        | "idol"
        | "queen-afterimage"
        | "local-conscription" => Some(own_queen()),
        "icbm" => Some(own_queen() && any(state, enemy, |p, _| queen(p))),
        "reformation" | "snipe" | "bishopSnipe" => Some(own_type("bishop")),
        "fianchetto" => Some(!side(state, "fianchetto", color) && own_type("bishop")),
        "dutch" => Some(own_type("rook") || own_type("bishop") || own_type("knight")),
        "pawn-conversion" => Some(!side(state, "pawnConversion", color) && own_type("pawn")),
        "queens-gambit" | "baby-bear" => Some(any(state, color, |p, _| queen(p) && nonroyal(p))),
        "charge" => Some(pawn_ahead(state, color, 2)),
        "pawn-storm" => Some(pawn_ahead(state, color, 1)),
        "king-of-the-hill" | "shotgun-king" | "horse-riding" | "knightmate" | "horde"
        | "encouragement" | "castling" | "racing-king" | "iron-monarch" | "imperial-studies"
        | "last-resistance" => Some(own_royal()),
        "underground-bunker" => Some(any(state, color, |p, _| {
            royal(state, p) && !has(p, "undergroundBunker")
        })),
        "reverse-pawns" | "spy" => Some(count(state, enemy, "pawn") > 0),
        "random-roulette" => Some(entries(state).iter().any(|(_, p)| {
            p.color.owner().is_some()
                && !has(p, "regencyHeir")
                && !has(p, "crownRoyal")
                && major(&p.kind)
        })),
        "blue-jeans" => {
            Some(entries(state).iter().any(|(_, p)| {
                p.color.owner().is_some() && !has(p, "regencyHeir") && major(&p.kind)
            }))
        }
        "queen-cavalry" => Some(any(state, color, |p, s| p.kind == "pawn" && s.col == 3)),
        "chameleon-mutation" | "mongolian-gambit" => Some(any(state, color, |p, _| {
            nonroyal(p)
                && !matches!(
                    p.kind.as_str(),
                    "merchant" | "wall" | "colossus" | "bigRook" | "bigBishop"
                )
        })),
        "suicide-bomber" => Some(any(state, color, |p, _| {
            matches!(p.kind.as_str(), "pawn" | "fanatic") && !has(p, "feudalContractId")
        })),
        "switcheroo" => Some(
            own_type("pawn")
                && any(state, color, |p, _| {
                    royal(state, p) && !has(p, "undergroundBunker")
                }),
        ),
        "radical-charge" | "backward-knight" | "file-surge" | "eagle" | "conversion" => {
            Some(own_type("knight"))
        }
        "rook-lift" => Some(!side(state, "rookLift", color) && own_type("rook")),
        "corner-kick" => Some(!side(state, "cornerKick", color) && own_type("knight")),
        "amazon" => Some(own_queen() && own_type("knight")),
        "palace" => Some(any(state, enemy, |p, _| royal(state, p))),
        "religious-victory" => {
            Some(count(state, color, "bishop") >= 2 && !side(state, "religiousVictory", color))
        }
        "bina-mate" => Some(!side(state, "binaMate", color)),
        "overwhelm" => Some(!side(state, "overwhelm", color)),
        "prophecy" => Some(own_royal() && !side(state, "prophecy", color)),
        "royal-shield" => Some(any(state, color, |p, _| {
            !matches!(p.kind.as_str(), "wall" | "scarecrow") && !has(p, "shielded")
        })),
        "witch-trial" => Some(any(state, enemy, |p, _| {
            nonroyal(p) && !matches!(p.kind.as_str(), "wall" | "colossus" | "merchant" | "vip")
        })),
        "disarm" | "fanatical-ritual" => Some(any(state, enemy, |p, _| {
            nonroyal(p)
                && !matches!(p.kind.as_str(), "wall" | "colossus")
                && (id != "fanatical-ritual" || p.kind != "merchant")
        })),
        "royal-command" => Some(any(state, color, |p, _| {
            nonroyal(p) && !matches!(p.kind.as_str(), "wall" | "football" | "colossus")
        })),
        "reposition" => Some(any(state, color, |p, _| {
            !matches!(p.kind.as_str(), "wall" | "football")
        })),
        "severance" => Some(any(state, enemy, |p, _| nonroyal(p) && ranged(state, p))),
        "ice-sheet" => Some(any(state, enemy, |p, _| {
            matches!(p.kind.as_str(), "magicGirl" | "berserker" | "trickster") || ranged(state, p)
        })),
        "injury" => Some(any(state, enemy, |p, _| {
            matches!(
                p.kind.as_str(),
                "knight" | "royalKnight" | "assassin" | "dragon" | "pegasus" | "unicorn" | "amazon"
            ) || (royal(state, p) && side(state, "kingKnight", enemy))
        })),
        "scarecrow" => Some(any(state, color, |p, _| {
            !matches!(
                p.kind.as_str(),
                "wall" | "football" | "blackHole" | "coffin"
            )
        })),
        "rule-ticket" => {
            let definitions = crate::draft::definitions_for_ruleset(&state.ruleset_id)?;
            Some(definitions.definitions.iter().any(|c| {
                crate::draft::category_with_ruleset(&state.ruleset_id, c) == "RULE"
                    && !active_ids(state, None).contains(c["id"].as_str().expect("id"))
            }))
        }
        "calling-card" => Some(any(state, enemy, |piece, _| {
            nonroyal(piece)
                && !matches!(
                    piece.kind.as_str(),
                    "pawn" | "wall" | "football" | "blackHole"
                )
                && !matches!(ability(piece), "guard" | "revolvingDoor" | "jester")
        })),
        "insight" => Some(true),
        "taunt" => Some(any(state, enemy, |p, _| {
            !matches!(p.kind.as_str(), "wall" | "football" | "blackHole")
        })),
        "basic-training" => Some(any(state, color, |p, _| {
            !has(p, "basicTraining")
                && !p.is_large()
                && !matches!(
                    p.kind.as_str(),
                    "pawn"
                        | "king"
                        | "queen"
                        | "primeMinister"
                        | "jester"
                        | "guard"
                        | "amazon"
                        | "man"
                        | "idol"
                        | "babyBear"
                        | "bear"
                        | "wall"
                        | "football"
                        | "blackHole"
                )
        })),
        "martyrdom" => Some(own_type("bishop") && own_type("pawn")),
        "emergency-evacuation" => Some(any(state, color, |p, s| {
            !matches!(p.kind.as_str(), "wall" | "football" | "colossus")
                && !has(p, "undergroundBunker")
                && s.offset(-color.pawn_dir(), 0)
                    .is_some_and(|to| open(state, to))
        })),
        "traitor" => Some(any(state, enemy, |p, _| {
            p.moved
                && !matches!(
                    p.kind.as_str(),
                    "merchant" | "wall" | "football" | "colossus"
                )
        })),
        "panic" => Some(
            entries(state)
                .iter()
                .filter(|(_, piece)| {
                    piece.color == enemy
                        && nonroyal(piece)
                        && !matches!(
                            piece.kind.as_str(),
                            "merchant" | "wall" | "football" | "colossus" | "bigRook" | "bigBishop"
                        )
                        && !state
                            .extra
                            .get("pendingPanic")
                            .and_then(Value::as_array)
                            .is_some_and(|pending| {
                                pending.iter().any(|entry| {
                                    entry.get("pieces").and_then(Value::as_array).is_some_and(
                                        |pieces| {
                                            pieces.iter().any(|reference| {
                                                reference.get("id").and_then(Value::as_str)
                                                    == Some(&piece.id)
                                            })
                                        },
                                    )
                                })
                            })
                })
                .count()
                >= 2,
        ),
        "vortex" => Some(
            entries(state)
                .iter()
                .filter(|(_, piece)| {
                    piece.color == enemy
                        && nonroyal(piece)
                        && !piece.is_large()
                        && !matches!(piece.kind.as_str(), "pawn" | "wall")
                })
                .count()
                >= 2,
        ),
        "alekhine-machine-gun" => {
            Some(for_draft || (own_queen() && count(state, color, "rook") >= 2))
        }
        "apprentice-knights" => Some(any(state, color, |p, s| {
            p.kind == "pawn" && matches!(origin(p, s).col, 1 | 6)
        })),
        "checker" => Some([0, 1, 6, 7].into_iter().all(|col| {
            open(
                state,
                Square {
                    row: if color == Color::White { 5 } else { 2 },
                    col,
                },
            )
        })),
        "holdout" => Some(any(state, color, |p, _| {
            p.kind == "pawn" && !has(p, "holdoutPromotion")
        })),
        "conscription" => Some((2..=5).any(|col| {
            open(
                state,
                Square {
                    row: if color == Color::White { 6 } else { 1 },
                    col,
                },
            )
        })),
        "chimera" => Some(any(state, color, |p, _| {
            !has(p, "chimera") && matches!(p.kind.as_str(), "knight" | "bishop")
        })),
        "desperado" => Some(any(state, color, |p, _| {
            nonroyal(p)
                && !matches!(
                    p.kind.as_str(),
                    "merchant" | "wall" | "football" | "colossus" | "bigRook" | "bigBishop"
                )
                && !crate::movement::frozen(p)
                && p.extra
                    .get("staked")
                    .and_then(|v| v.get("remaining"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    <= 0
        })),
        "schrodinger-pawns" => Some(any(state, color, |p, s| {
            p.kind == "pawn"
                && !has(p, "quantum")
                && s.offset(-direction(state, color), 0).is_some_and(|to| {
                    state.at(to).is_none()
                        && !occupied_quantum(state, to)
                        && !reserved(state, to, false)
                })
        })),
        "stake" => Some(any(state, color, |p, _| {
            !has(p, "staked") && !matches!(p.kind.as_str(), "wall" | "football" | "colossus")
        })),
        "summon-colossus" => Some(count(state, color, "pawn") >= 6),
        "collapse" => Some(
            state
                .extra
                .get("collapseDepth")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                < 4,
        ),
        "exile" => Some(any(state, enemy, |p, s| {
            nonroyal(p)
                && !matches!(p.kind.as_str(), "wall" | "colossus")
                && open(state, origin(p, s))
        })),
        "freeze" => {
            let candidates = entries(state)
                .into_iter()
                .filter(|(_, p)| {
                    p.color == enemy
                        && nonroyal(p)
                        && !matches!(
                            p.kind.as_str(),
                            "wall" | "football" | "blackHole" | "scarecrow"
                        )
                })
                .collect::<Vec<_>>();
            Some(
                candidates.len() > 4 && candidates.iter().any(|(_, p)| !crate::movement::frozen(p)),
            )
        }
        "trolley" => return trolley(state, enemy),
        "white-box" => return random_box(state, color, true, depth),
        "black-box" => return random_box(state, color, false, depth),
        "joker" => Some(state.deck_slots.get(color).iter().any(|c| {
            !c.vacant
                && c.effect != "joker"
                && c.effect != "bloodCard"
                && c.extra.get("phase").and_then(Value::as_str) != Some("RULE")
                && !truth(c.extra.get("passiveApplied"))
                && !truth(c.extra.get("devCard"))
                && !passive(&serde_json::to_value(c).expect("card"))
        })),
        "guard" => {
            let kings = entries(state);
            let king = kings
                .iter()
                .find(|(_, p)| p.color == color && royal(state, p));
            Some(king.is_some_and(|(square, p)| {
                let throne = origin(p, *square);
                any(state, color, |p, s| {
                    matches!(p.kind.as_str(), "pawn" | "fanatic")
                        && throne.offset(direction(state, color), 0) == Some(origin(p, s))
                })
            }))
        }
        "merchant-guild" => Some(
            entries(state)
                .iter()
                .find(|(_, p)| p.color == color && p.kind == "king")
                .is_some_and(|(s, _)| {
                    s.offset(direction(state, color) * 2, 0).is_some_and(|to| {
                        state
                            .at(to)
                            .is_none_or(|p| !p.is_large() && p.color == color)
                    })
                }),
        ),
        "qxe1" => Some(
            entries(state)
                .iter()
                .filter(|(_, p)| p.color == color && p.is_royal())
                .any(|(king, _)| {
                    entries(state)
                        .iter()
                        .filter(|(_, p)| p.color == color && p.kind == "queen")
                        .any(|(queen, _)| {
                            let dr = king.row as i8 - queen.row as i8;
                            let dc = king.col as i8 - queen.col as i8;
                            if dr != 0 && dc != 0 && dr.abs() != dc.abs() {
                                return false;
                            }
                            let mut next = queen.offset(dr.signum(), dc.signum());
                            while let Some(square) = next {
                                if square == *king {
                                    return true;
                                }
                                if state.at(square).is_some() {
                                    return false;
                                }
                                next = square.offset(dr.signum(), dc.signum());
                            }
                            false
                        })
                }),
        ),
        "premove" => {
            for (square, piece) in entries(state) {
                if piece.color == color
                    && !matches!(
                        piece.kind.as_str(),
                        "wall" | "football" | "blackHole" | "coffin"
                    )
                    && !crate::movement::piece_moves(state, piece, square)?.is_empty()
                {
                    return Ok(true);
                }
            }
            Some(false)
        }
        "sacrifice" => Some(entries(state).iter().any(|(_, p)| {
            p.color == color
                && nonroyal(p)
                && !matches!(
                    p.kind.as_str(),
                    "timeTraveler" | "vampireLord" | "wall" | "football" | "blackHole" | "coffin"
                )
                && worth(&p.kind).is_some_and(|value| {
                    any(state, color, |other, _| {
                        other.id != p.id
                            && nonroyal(other)
                            && !matches!(
                                other.kind.as_str(),
                                "timeTraveler"
                                    | "vampireLord"
                                    | "wall"
                                    | "football"
                                    | "blackHole"
                                    | "coffin"
                            )
                            && worth(&other.kind).is_some_and(|worth| worth < value)
                    })
                })
        })),
        _ => None,
    };
    if let Some(answer) = answer {
        return Ok(answer);
    }
    if truth(card.get("target")) && !target_drawable(state, card, color)? {
        return Ok(false);
    }
    if card["phase"] != "PIECE" && id != "bribe" {
        return Ok(true);
    }
    Ok(match id {
        "assassin" | "knightmaster" | "bribe" => own_type("knight"),
        "grasshopper" | "eastern-policy" => any(state, color, |p, _| minor(&p.kind)),
        "pegasus" | "dragon" | "herald" => own_type("rook"),
        "hook" => own_queen() && own_type("rook"),
        "log" | "standard-bearer" => own_type("pawn"),
        "windmill" => own_type("bishop") && own_type("rook"),
        "ordination" => count(state, color, "bishop") >= 2,
        // This is the client's documented fallthrough for other PIECE cards;
        // acquiring a card still requires its separate actual effect handler.
        _ => true,
    })
}

fn target_drawable(state: &GameState, card: &Value, color: Color) -> Result<bool> {
    let id = card["id"].as_str().expect("id");
    let effect = card["effect"].as_str().unwrap_or("");
    let enemy = color.opponent();
    let nonroyal = |p: &Piece| !royal(state, p);
    let unobstructed = placements(state);
    let own_queen = || any(state, color, |p, _| queen(p));
    let result = match id {
        "brainwash" => Some(any(state, color, |p, _| {
            nonroyal(p)
                && worth(&p.kind).is_some_and(|value| {
                    any(state, enemy, |victim, _| {
                        nonroyal(victim) && worth(&victim.kind).is_some_and(|v| v < value)
                    })
                })
        })),
        "taboo" => {
            Some(any(state, color, |p, _| queen(p) && nonroyal(p)) && !unobstructed.is_empty())
        }
        "wanted" => Some(any(state, enemy, |p, _| {
            ranged(state, p) && !has(p, "wanted")
        })),
        "grappler" => Some(
            any(state, color, |p, _| queen(p) && nonroyal(p))
                && any(state, color, |p, _| minor(&p.kind) && nonroyal(p)),
        ),
        "medium" => Some(any(state, color, |p, _| minor(&p.kind) && nonroyal(p))),
        "revolving-door" | "don-quixote" | "brutus" => {
            Some(any(state, color, |p, _| p.kind == "rook" && nonroyal(p)))
        }
        "greek-gift" => Some(any(state, color, |p, _| p.kind == "bishop" && nonroyal(p))),
        "d4"
        | "e4"
        | "solidarity"
        | "bishop-infiltration"
        | "synchronization"
        | "assembly"
        | "vigilance"
        | "roller" => Some(!side(state, effect, color)),
        "metal" => Some(any(state, color, |p, _| {
            nonroyal(p) && !has(p, "metalized") && ranged(state, p)
        })),
        "paladin" => Some(any(state, color, |p, _| p.kind == "knight" && nonroyal(p))),
        "octopus" => Some(any(state, color, |p, _| p.kind == "rook" && nonroyal(p))),
        "symmetry" | "mutation" => Some(!side(state, id, color)),
        "thief" => Some(any(state, color, |p, _| queen(p) && nonroyal(p))),
        "clockwork" | "parrot" => Some(any(state, color, |p, _| {
            minor(&p.kind) && nonroyal(p) && (id == "parrot" || p.kind != "clockwork")
        })),
        _ => None,
    };
    if let Some(result) = result {
        return Ok(result);
    }
    let result = match effect {
        "recurrence" => Some(any(state, color, |p, _| {
            !has(p, "recurrence")
                && !matches!(
                    p.kind.as_str(),
                    "pawn" | "wall" | "football" | "blackHole" | "monster" | "coffin"
                )
        })),
        "nullification" => Some(any(state, color, |p, _| {
            !has(p, "nullification")
                && !matches!(
                    p.kind.as_str(),
                    "wall" | "football" | "blackHole" | "monster" | "coffin"
                )
        })),
        "siegeRam" | "magicGirl" | "berserker" | "slime" | "trickster" => {
            Some(count(state, color, "rook") > 0)
        }
        "siren" | "undead" => Some(own_queen()),
        "suspiciousPotion" => Some(entries(state).iter().any(|(_, p)| {
            p.color.owner().is_some()
                && !p.is_large()
                && !matches!(
                    p.kind.as_str(),
                    "wall" | "football" | "blackHole" | "monster" | "coffin"
                )
        })),
        "replayMove" => Some(
            state
                .extra
                .get("moveReplay")
                .and_then(|s| s.get(color.as_str()))
                .is_some_and(|r| r.get("available").and_then(Value::as_bool) == Some(true)),
        ),
        "hypocrisy" => Some(unobstructed.len() >= 4),
        "cleanupPieces" => Some(any(state, color, |p, _| {
            nonroyal(p)
                && !p.is_large()
                && !matches!(
                    p.kind.as_str(),
                    "wall" | "football" | "blackHole" | "coffin"
                )
        })),
        "frontlineResponse" | "relay" => Some(!side(state, effect, color)),
        "fieldPromotion" => {
            Some(!side(state, "fieldPromotion", color) && count(state, color, "pawn") > 0)
        }
        "kingOfTheHill" => Some(!side(state, "hillKing", color)),
        "genevaConvention" => Some(!side(state, effect, color)),
        "loyalist" => Some(any(state, color, |p, _| {
            !has(p, "loyalist")
                && ability(p) != "slime"
                && nonroyal(p)
                && !p.is_large()
                && !matches!(
                    p.kind.as_str(),
                    "wall" | "football" | "blackHole" | "monster" | "coffin"
                )
        })),
        "parry" => Some(any(state, color, |p, _| {
            !has(p, "parry")
                && !matches!(
                    p.kind.as_str(),
                    "wall" | "football" | "blackHole" | "monster" | "scarecrow"
                )
        })),
        "blackMagic" => Some(any(state, color, |p, _| royal(state, p))),
        "fleetingDream" => Some(any(state, enemy, |p, _| has(p, "promotedFromPawn"))),
        "emptyLunchbox" => Some(any(state, enemy, |p, _| {
            nonroyal(p)
                && !has(p, "emptyLunchbox")
                && !matches!(
                    p.kind.as_str(),
                    "merchant" | "wall" | "football" | "blackHole" | "monster"
                )
        })),
        "armistice" => Some(true),
        "poisonedPawn" => Some(any(state, color, |p, _| {
            p.kind == "pawn" && !has(p, "poisonedPawn")
        })),
        "portalGun" => Some(unobstructed.len() >= 2),
        "exhaustion" => Some(
            state
                .extra
                .get("exhaustion")
                .and_then(|s| s.get(enemy.as_str()))
                .is_none_or(|s| s.get("enabled").and_then(Value::as_bool) != Some(true)),
        ),
        // secondMissionaryCandidates searches diagonal neighbours, orthogonal
        // neighbours, then every Chebyshev ring through the entire 8x8 board.
        "missionary" => Some(count(state, color, "bishop") > 0 && !unobstructed.is_empty()),
        "democracy" => Some(
            !side(state, "democracy", color)
                && (count(state, color, "pawn") > 0
                    || state
                        .extra
                        .get("pendingRecurrences")
                        .and_then(Value::as_array)
                        .is_some_and(|plans| {
                            plans.iter().any(|plan| {
                                plan["piece"]["color"] == color.as_str()
                                    && plan["piece"]["type"] == "pawn"
                            })
                        })),
        ),
        "feudalContract" => Some(
            any(state, color, |p, _| {
                matches!(p.kind.as_str(), "pawn" | "fanatic")
                    && !has(p, "explosive")
                    && !has(p, "feudalContractId")
            }) && any(state, color, |p, _| {
                !matches!(
                    p.kind.as_str(),
                    "pawn" | "fanatic" | "wall" | "colossus" | "bigRook" | "bigBishop"
                )
            }),
        ),
        "windmill" => Some(count(state, color, "bishop") > 0 && count(state, color, "rook") > 0),
        "hook" => Some(own_queen() && count(state, color, "rook") > 0),
        "chain" => {
            let targets = entries(state)
                .into_iter()
                .filter(|(_, p)| {
                    p.color == enemy
                        && !p.is_large()
                        && !matches!(p.kind.as_str(), "wall" | "football" | "blackHole")
                })
                .collect::<Vec<_>>();
            Some(targets.iter().enumerate().any(|(index, (a, _))| {
                targets[index + 1..]
                    .iter()
                    .any(|(b, _)| a.row.abs_diff(b.row).max(a.col.abs_diff(b.col)) <= 2)
            }))
        }
        "twins" => Some(
            entries(state)
                .iter()
                .filter(|(_, p)| {
                    p.color == color
                        && ability(p) != "slime"
                        && !has(p, "twinBondId")
                        && !p.is_large()
                        && !matches!(
                            p.kind.as_str(),
                            "wall" | "football" | "blackHole" | "coffin"
                        )
                })
                .count()
                >= 2,
        ),
        "submerge" => Some(any(state, color, |p, s| {
            !has(p, "submerged")
                && !p.is_large()
                && !matches!(
                    p.kind.as_str(),
                    "wall" | "football" | "blackHole" | "coffin"
                )
                && !crate::movement::KING
                    .iter()
                    .filter_map(|&(dr, dc)| s.offset(dr, dc))
                    .any(|to| state.at(to).is_some_and(|p| p.color == enemy))
        })),
        "homecoming" => Some(any(state, color, |p, s| {
            nonroyal(p)
                && !p.is_large()
                && has(p, "origin")
                && !matches!(
                    p.kind.as_str(),
                    "wall" | "football" | "blackHole" | "coffin"
                )
                && origin(p, s) != s
                && open(state, origin(p, s))
        })),
        "judgment" => Some(entries(state).iter().any(|(_, p)| {
            p.color.owner().is_some()
                && nonroyal(p)
                && !p.is_large()
                && !matches!(p.kind.as_str(), "wall" | "football" | "blackHole")
                && p.number("captureCount") >= 2
        })),
        "lobster" => Some(!unobstructed.is_empty()),
        "callingCard" => Some(any(state, enemy, |p, _| {
            nonroyal(p)
                && !matches!(p.kind.as_str(), "pawn" | "wall" | "football" | "blackHole")
                && !matches!(ability(p), "guard" | "revolvingDoor" | "jester")
        })),
        "ghost" => Some(any(state, color, |p, _| {
            !has(p, "ghost") && !matches!(p.kind.as_str(), "wall" | "football" | "blackHole")
        })),
        "freeCastling" => Some(
            !side(state, "castlingCanceled", color)
                && entries(state)
                    .iter()
                    .filter(|(_, p)| p.color == color && royal(state, p))
                    .any(|(s, _)| {
                        any(state, color, |p, to| {
                            let dr = to.row as i16 - s.row as i16;
                            let dc = to.col as i16 - s.col as i16;
                            p.kind == "rook"
                                && dr.abs().max(dc.abs()) >= 3
                                && (dr == 0 || dc == 0 || dr.abs() == dc.abs())
                        })
                    }),
        ),
        _ => None,
    };
    if let Some(result) = result {
        return Ok(result);
    }
    Ok(match card["target"].as_str().unwrap_or("") {
        "enemy-ranged" => any(state, enemy, |piece, _| {
            nonroyal(piece) && ranged(state, piece)
        }),
        "enemy-slider" => any(state, enemy, |piece, _| {
            nonroyal(piece) && matches!(piece.kind.as_str(), "queen" | "bishop" | "rook")
        }),
        "enemy-piece" => any(state, enemy, |piece, _| {
            nonroyal(piece) && !matches!(piece.kind.as_str(), "wall" | "football" | "colossus")
        }),
        "empty" => unobstructed.iter().any(|s| {
            s.offset(0, 1).is_some_and(|to| unobstructed.contains(&to))
                || s.offset(1, 0).is_some_and(|to| unobstructed.contains(&to))
        }),
        "own-outpost-piece" => any(state, color, |p, s| {
            nonroyal(p)
                && !has(p, "outpostProtected")
                && !matches!(
                    p.kind.as_str(),
                    "wall" | "football" | "blackHole" | "monster" | "coffin"
                )
                && (if color == Color::White {
                    s.row < 4
                } else {
                    s.row >= 4
                })
        }),
        "own-extinction" => any(state, color, |p, _| nonroyal(p) && minor(&p.kind)),
        "own-piece" => any(state, color, |p, _| p.kind != "wall"),
        "own-promotion-rush-piece" => any(state, color, |p, _| {
            p.kind != "pawn"
                && !p.is_large()
                && !matches!(
                    p.kind.as_str(),
                    "wall" | "football" | "monster" | "blackHole"
                )
        }),
        "own-bishop" => any(state, color, |p, _| {
            matches!(p.kind.as_str(), "bishop" | "protestant")
        }),
        "own-plain-bishop" => count(state, color, "bishop") > 0,
        "own-pawn" | "own-frenzy-pawn" => count(state, color, "pawn") > 0,
        "own-queen" => own_queen(),
        "own-rook" => count(state, color, "rook") > 0,
        "own-minor" => any(state, color, |p, _| minor(&p.kind)),
        "own-knight" => any(state, color, |p, _| {
            p.kind == "knight" && (effect != "trojanHorse" || !has(p, "trojanHorse"))
        }),
        // The actual target switch falls through to true for effect-specific
        // targets already handled by their availability case above.
        _ => true,
    })
}
