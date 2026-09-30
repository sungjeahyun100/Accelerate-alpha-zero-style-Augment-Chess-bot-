//! Board-changing ACTIVE cards from the pinned September 28 client.
//!
//! This module owns the first-click surface, legal action ordering and direct
//! effects of its exact card IDs. The caller retains card identity, card-use,
//! turn settlement, common reconciliation, notation and event ownership.
//! Source: `main-OahWs0tU.js`, SHA-256
//! `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`.

use super::*;

pub(super) const IDS: &[&str] = &[
    "alekhine-machine-gun",
    "bribe",
    "chain",
    "emergency-evacuation",
    "evasion",
    "exile",
    "feudal-contract",
    "freeze",
    "guard",
    "judgment",
    "metal",
    "missionary",
    "necromancy",
    "queens-gambit",
    "scarecrow",
    "twins",
    "windmill",
];

fn owned(card: &CardSlot) -> bool {
    IDS.contains(&card.id.as_str())
}

fn board_match(state: &GameState, card: &CardSlot, piece: &Piece, square: Square) -> Result<bool> {
    Ok(match card.id.as_str() {
        "bribe" => source_matches(state, piece, Source::Exact("knight")),
        "judgment" => judgment_matches(state, piece),
        "exile" => grant_matches(state, piece, square, "exile"),
        "emergency-evacuation" => evacuation_candidate(state, piece, square)?,
        "necromancy" => {
            source_matches(state, piece, Source::Exact("pawn"))
                && !necromancy_types(state).is_empty()
        }
        "missionary" => source_matches(state, piece, Source::Exact("bishop")),
        "queens-gambit" => {
            source_matches(state, piece, Source::NonRoyal("queen"))
                && !state.flag("regency", state.turn)
        }
        "chain" => chain_target(state, piece),
        "feudal-contract" => {
            piece.color == state.turn
                && ["pawn", "fanatic"].contains(&piece.kind.as_str())
                && !truthy(piece.extra.get("explosive"))
                && !truthy(piece.extra.get("feudalContractId"))
        }
        "scarecrow" => {
            piece.color == state.turn
                && !["wall", "football", "blackHole", "coffin"].contains(&piece.kind.as_str())
                && crate::movement::d4_destination_allowed(state, piece.color, &[square])
        }
        "metal" => {
            piece.color == state.turn
                && !state.royal_identity(piece)
                && !truthy(piece.extra.get("metalized"))
                && ranged_piece(state, piece)
        }
        "twins" => twin_target(state, piece),
        "windmill" => source_matches(state, piece, Source::Exact("bishop")),
        _ => {
            return Err(EngineError::UnsupportedFeature(format!(
                "card board target {}",
                card.id
            )));
        }
    })
}

fn board_targets(state: &GameState, card: &CardSlot, unique: bool) -> Result<Vec<Square>> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(piece) = state.at(square) else {
                continue;
            };
            if board_match(state, card, piece, square)?
                && (!unique
                    || seen.insert(if piece.id.is_empty() {
                        format!("square:{row}:{col}")
                    } else {
                        format!("piece:{}", piece.id)
                    }))
            {
                result.push(square);
            }
        }
    }
    Ok(result)
}

/// The site's `getTargetSquares` first click is independent of later legal
/// selections and of a card's eventual direct-effect success.
pub(super) fn ui_targets(state: &GameState, card: &CardSlot) -> Result<Option<Vec<Square>>> {
    if !owned(card) {
        return Ok(None);
    }
    if !truthy(card.extra.get("target")) {
        return Ok(Some(Vec::new()));
    }
    let mut targets = board_targets(state, card, false)?;
    if card.id == "chain" {
        let pairs = chain_pairs(state, &board_targets(state, card, true)?)?;
        targets.retain(|square| {
            pairs.iter().flatten().any(|member| {
                let Some(piece) = state.at(*square) else {
                    return false;
                };
                let Some(other) = state.at(*member) else {
                    return false;
                };
                if piece.id.is_empty() {
                    square == member
                } else {
                    piece.id == other.id
                }
            })
        });
    }
    if card.id == "exile" {
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
        targets = legal;
    }
    Ok(Some(targets))
}

/// Source UI order: row-major cells, then second/third selections in ascending
/// index order. This is distinct from target-square projection above.
pub(super) fn actions(state: &GameState, card: &CardSlot) -> Result<Option<Vec<Action>>> {
    if !owned(card) {
        return Ok(None);
    }
    let ready = match card.id.as_str() {
        "guard" => guard_pawn_square(state).is_some(),
        "freeze" => !freeze_candidates(state).is_empty(),
        "alekhine-machine-gun" => alekhine_formation(state).is_some(),
        "evasion" => !evasion_candidates(state).is_empty(),
        _ => false,
    };
    if ["guard", "freeze", "alekhine-machine-gun", "evasion"].contains(&card.id.as_str()) {
        return Ok(Some(if ready {
            vec![Action::card(state.turn, card, None)]
        } else {
            Vec::new()
        }));
    }
    let unique = ["judgment", "emergency-evacuation", "chain", "twins"].contains(&card.id.as_str());
    let mut targets = board_targets(state, card, unique)?;
    if card.id == "exile" {
        targets = ui_targets(state, card)?.ok_or(EngineError::IllegalAction)?;
    }
    let result = match card.id.as_str() {
        "emergency-evacuation" => selection_actions(state, card, &targets, false),
        "twins" => selection_actions(state, card, &targets, true),
        "chain" => chain_pairs(state, &targets)?
            .into_iter()
            .map(|pair| Action::card(state.turn, card, Some(json!({"selections":pair}))))
            .collect(),
        "windmill" => {
            let rooks = (0..8)
                .flat_map(|row| (0..8).map(move |col| Square { row, col }))
                .filter(|square| {
                    state
                        .at(*square)
                        .is_some_and(|piece| source_matches(state, piece, Source::Exact("rook")))
                })
                .collect::<Vec<_>>();
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
        "feudal-contract" => {
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
        _ => targets
            .into_iter()
            .map(|square| Action::card(state.turn, card, Some(json!(square))))
            .collect(),
    };
    Ok(Some(result))
}

/// 공개 실행은 거절된 포획 정산·RNG 선택도 사본 안에서 원자적으로 처리한다.
/// 내부 AI simulation의 disposable state는 원문 실패 전 부분 효과를 보존한다.
pub(super) fn apply(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    if !owned(card) {
        return Err(EngineError::UnsupportedFeature(format!(
            "card board effect {}",
            card.id
        )));
    }
    validate_pinned_card(state, card)?;
    let plan = plan(state, card)
        .ok_or_else(|| EngineError::UnsupportedFeature(format!("card board plan {}", card.id)))?;
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
    if state.is_ai_simulation() {
        // admission은 동일하게 유지하고 source raw kernel로 바로 진입한다.
        // 시뮬레이션 caller가 state 수명을 소유하며 실제 카드 권한은 바꾸지 않는다.
        return apply_direct(state, card, action);
    }
    let mut staged = state.clone();
    let removed = apply_direct(&mut staged, card, action)?;
    *state = staged;
    Ok(removed)
}

fn selected_piece(state: &GameState, card: &CardSlot, action: &Action) -> Result<(Square, Piece)> {
    let square = target_square(action)?;
    let piece = state.at(square).ok_or(EngineError::IllegalAction)?;
    if !board_match(state, card, piece, square)? {
        return Err(EngineError::IllegalAction);
    }
    Ok((square, piece.clone()))
}

fn apply_direct(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    match card.id.as_str() {
        "guard" | "freeze" | "alekhine-machine-gun" | "evasion" => {
            if has_card_selection(action) {
                return Err(EngineError::IllegalAction);
            }
            match card.id.as_str() {
                "guard" => apply_guard(state)?,
                "freeze" => apply_freeze(state)?,
                "alekhine-machine-gun" => apply_alekhine(state)?,
                "evasion" => apply_evasion(state)?,
                _ => unreachable!(),
            }
            Ok(Vec::new())
        }
        "emergency-evacuation" => {
            apply_evacuation(state, action)?;
            Ok(Vec::new())
        }
        "twins" => {
            apply_twins(state, action)?;
            Ok(Vec::new())
        }
        "chain" => {
            apply_chain(state, action)?;
            Ok(Vec::new())
        }
        "windmill" => {
            apply_windmill(state, action)?;
            Ok(Vec::new())
        }
        "feudal-contract" => {
            apply_feudal_contract(state, action)?;
            Ok(Vec::new())
        }
        "scarecrow" => apply_scarecrow(
            state,
            action,
            plan(state, card).ok_or(EngineError::IllegalAction)?,
        ),
        "queens-gambit" => apply_queens_gambit(state, action),
        "metal" => {
            let (_, mut piece) = selected_piece(state, card, action)?;
            piece.extra.insert("metalized".into(), json!(true));
            piece.extra.insert("metalCooldown".into(), json!(0));
            write_piece(state, &piece);
            Ok(Vec::new())
        }
        "bribe" => apply_bribe(state, card, action),
        "judgment" => apply_judgment(state, card, action),
        "missionary" => apply_missionary(state, card, action),
        "necromancy" => apply_necromancy(state, card, action),
        "exile" => apply_exile(state, card, action),
        _ => Err(EngineError::UnsupportedFeature(format!(
            "card board effect {}",
            card.id
        ))),
    }
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
    let suffix = crate::draft::random_suffix(
        state
            .rng
            .sample_opaque("source feudal installation identity")?,
    )?;
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
        state
            .rng
            .record_last_probability(1.0 / (index + 1) as f64, "source Freeze shuffle")?;
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
            let identity = if piece.id.is_empty() {
                format!("square:{}:{}", square.row, square.col)
            } else {
                format!("piece:{}", piece.id)
            };
            if piece.color != state.turn
                || !seen.insert(identity)
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
    state.rng.record_last_probability(
        1.0 / candidates.len() as f64,
        "source Evasion card recipient",
    )?;
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
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
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
    let suffix =
        crate::draft::random_suffix(state.rng.sample_opaque("source twins bond identity")?)?
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
    state
        .rng
        .record_last_probability(1.0 / files.len() as f64, "source Queen's Gambit file")?;
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
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
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
                crate::draft::random_suffix(
                    state
                        .rng
                        .sample_opaque("source missing chain piece identity")?
                )?
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
    let suffix =
        crate::draft::random_suffix(state.rng.sample_opaque("source chain bond identity")?)?;
    let id = format!(
        "chain-{}-{suffix}",
        crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?
    );
    let new_bond = json!({"id":id,"aId":entries[0].1.id,"bId":entries[1].1.id,"by":state.turn});
    let mut bonds = match state.extra.get("chainBonds") {
        Some(Value::Array(entries)) => entries.iter().take(64).cloned().collect::<Vec<_>>(),
        // JSON strings are the other iterable accepted by the source's
        // array spread. Their code-point entries fail normalization, but
        // still occupy its first-64 window before the new bond is appended.
        Some(Value::String(value)) => value
            .chars()
            .take(64)
            .map(|character| json!(character.to_string()))
            .collect(),
        value if !truthy(value) => Vec::new(),
        _ => {
            return Err(EngineError::InvalidState(
                "v7 chainBonds is not iterable; source array spread raises TypeError unless the value is an array, string, or falsy".into(),
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

fn apply_evacuation(state: &mut GameState, action: &Action) -> Result<()> {
    apply_evacuation_pre_sound(state, action)?;
    let sound = if state.turn == Color::White {
        "moveSelf"
    } else {
        "moveOpponent"
    };
    crate::v7_threat::play_move_sound_v7(state, sound, state.turn)
}

// The source's sound callback invokes an AiNoCards threat probe after the
// relocation, last-move highlight, and acceleration-trail writes. Keeping
// that boundary explicit lets the direct effect and the probe be compared
// independently against their pinned source receipts.
fn apply_evacuation_pre_sound(state: &mut GameState, action: &Action) -> Result<()> {
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
        // The source snapshots each move's privacy before the relocation,
        // then derives destination visibility from the updated board. Fog
        // visibility is fallible because its legal-move probes share the
        // same source rule boundary as ordinary observation.
        let visible_origin =
            crate::observation::piece_visible_to_color_at_v7(state, &piece, origin, viewer)?;
        state.board[destination.row as usize][destination.col as usize] = Some(piece.clone());
        state.board[origin.row as usize][origin.col as usize] = None;
        piece.moved = true;
        crate::transition::mark_card_no_capture(state, &mut piece)?;
        note_ultimatum_movement(state, &mut piece)?;
        mark_animation(state, &piece)?;
        write_piece(state, &piece);
        let hidden = if !visible_origin
            || !crate::observation::piece_visible_to_color_at_v7(
                state,
                &piece,
                destination,
                viewer,
            )? {
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
    Ok(())
}

fn apply_bribe(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let (square, mut piece) = selected_piece(state, card, action)?;
    piece.kind = "amazon".into();
    mark_transformed_origin(state, &mut piece, square)?;
    piece.extra.insert("bribed".into(), json!(true));
    piece.extra.insert("bribedRemaining".into(), json!(3));
    let created = state.move_count;
    state
        .extra
        .get_mut("temporaryQueens")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EngineError::InvalidState("temporaryQueens must be an array".into()))?
        .push(json!({"id":piece.id,"color":piece.color,"remaining":3,"createdAt":created}));
    write_piece(state, &piece);
    Ok(Vec::new())
}

fn apply_judgment(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let (selected, piece) = selected_piece(state, card, action)?;
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
        return Ok(vec![removed]);
    };
    if !state
        .extra
        .get("judgmentExiles")
        .is_some_and(Value::is_array)
    {
        state.extra.insert("judgmentExiles".into(), json!([]));
    }
    crate::transition::clear_piece(state, &piece.id);
    let suffix =
        crate::draft::random_suffix(state.rng.sample_opaque("source judgment exile identity")?)?;
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
            crate::replay::source_piece_label(&piece.kind).unwrap_or(&piece.kind)
        ),
    )?;
    Ok(Vec::new())
}

/// main:99283-99345. 복귀는 원문의 드래프트 시작 시점에서 정산한다.
/// 배치 조건은 설치 예약·양자 기물·상대 d4·붕괴 칸이며, Crown과
/// BlackHole의 환경 정산은 이 콜백의 책임에 포함되지 않는다.
pub(super) fn return_judgment_exiles_for_draft(
    state: &mut GameState,
    phase: &str,
) -> Result<usize> {
    if !["MIDDLE", "END"].contains(&phase) {
        return Ok(0);
    }
    let Some(entries) = state.extra.get("judgmentExiles").and_then(Value::as_array) else {
        return Ok(0);
    };
    if entries.is_empty() {
        return Ok(0);
    }
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "Judgment draft returns require the pinned v7 ruleset".into(),
        ));
    }
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::InvalidState(
            "Judgment draft returns require an 8x8 board".into(),
        ));
    }
    let entries = entries.clone();
    let mut remaining = Vec::with_capacity(entries.len());
    let mut returned = Vec::new();
    for entry in entries {
        let return_phase = entry.get("returnPhase").and_then(Value::as_str);
        let due = return_phase == Some(phase) || (return_phase == Some("MIDDLE") && phase == "END");
        let raw_piece = entry.get("piece");
        let owner = match raw_piece
            .and_then(|piece| piece.get("color"))
            .and_then(Value::as_str)
        {
            Some("white") => Some(Color::White),
            Some("black") => Some(Color::Black),
            _ => None,
        };
        let Some(owner) = owner.filter(|_| due) else {
            // 먼저 시기를 검사하므로 미래 항목은 같은 ID가 보드에 있어도 남긴다.
            remaining.push(entry);
            continue;
        };
        let raw_piece = raw_piece.ok_or_else(|| {
            EngineError::InvalidState("Due Judgment exile piece is absent".into())
        })?;
        if let Some(id) = raw_piece
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            && state
                .board
                .iter()
                .flatten()
                .flatten()
                .any(|live| live.id == id)
        {
            // source findPieceById가 이미 찾은 항목은 반환·로그 없이 제거한다.
            continue;
        }
        let mut piece: Piece = serde_json::from_value(raw_piece.clone()).map_err(|error| {
            EngineError::InvalidState(format!(
                "Due Judgment exile piece cannot be represented: {error}"
            ))
        })?;
        let seed_id = if truthy(entry.get("id")) {
            judgment_seed_string(
                entry.get("id").ok_or_else(|| {
                    EngineError::InvalidState("Truthy Judgment exile ID is absent".into())
                })?,
                0,
            )?
        } else if !piece.id.is_empty() {
            piece.id.clone()
        } else {
            "judgment".into()
        };
        let seed = format!("{seed_id}:{phase}");
        let Some(destination) = judgment_return_destination(state, &piece, owner, &seed)? else {
            remaining.push(entry);
            continue;
        };
        if piece.is_large() {
            piece
                .extra
                .insert("anchorRow".into(), json!(destination.row));
            piece
                .extra
                .insert("anchorCol".into(), json!(destination.col));
            if truthy(state.extra.get("monochromeChess")) && !truthy(piece.extra.get("monoShade")) {
                piece.extra.insert(
                    "monoShade".into(),
                    json!(if (destination.row + destination.col) % 2 == 0 {
                        "light"
                    } else {
                        "dark"
                    }),
                );
            }
            for dr in 0..2 {
                for dc in 0..2 {
                    state.board[(destination.row + dr) as usize][(destination.col + dc) as usize] =
                        Some(piece.clone());
                }
            }
        } else {
            state.board[destination.row as usize][destination.col as usize] = Some(piece.clone());
        }
        mark_animation(state, &piece)?;
        returned.push((piece, owner, destination));
    }
    state
        .extra
        .insert("judgmentExiles".into(), Value::Array(remaining));
    // 모든 배치를 먼저 마친 뒤 원문 항목 순서대로 로그를 추가한다.
    for (piece, owner, destination) in &returned {
        crate::replay::add_log(
            state,
            format!(
                "레드카드: 추방되었던 {} {}이 {}{}로 돌아왔습니다.",
                crate::replay::label(*owner),
                crate::replay::source_piece_label(&piece.kind).unwrap_or(&piece.kind),
                char::from(b'a' + destination.col),
                8 - destination.row
            ),
        )?;
    }
    if !returned.is_empty() {
        // reviewed headless/local 프로필의 recordOnlineEvent는 비활성이다.
        crate::flow::note_card_event(state)?;
    }
    Ok(returned.len())
}

fn judgment_return_destination(
    state: &GameState,
    piece: &Piece,
    owner: Color,
    seed: &str,
) -> Result<Option<Square>> {
    let size: u8 = if piece.is_large() { 2 } else { 1 };
    let limit = 8 - size;
    // JS hashString은 Unicode scalar가 아닌 UTF-16 code unit을 누적한다.
    let hash = seed
        .encode_utf16()
        .fold(0i32, |hash, unit| {
            hash.wrapping_mul(31).wrapping_add(i32::from(unit))
        })
        .unsigned_abs() as usize;
    for offset in 0..=limit {
        let row = if owner == Color::White {
            limit - offset
        } else {
            offset
        };
        let mut candidates = Vec::new();
        for col in 0..=limit {
            let mut open = true;
            for dr in 0..size {
                for dc in 0..size {
                    let cell = Square {
                        row: row + dr,
                        col: col + dc,
                    };
                    if !crate::movement::open_placement(state, cell, Some(owner))?
                        || crate::movement::collapsed(state, cell)
                    {
                        open = false;
                        break;
                    }
                }
                if !open {
                    break;
                }
            }
            if open {
                candidates.push(Square { row, col });
            }
        }
        if !candidates.is_empty() {
            return Ok(Some(candidates[hash % candidates.len()]));
        }
    }
    Ok(None)
}

fn judgment_seed_string(value: &Value, depth: usize) -> Result<String> {
    if depth > 64 {
        return Err(EngineError::InvalidState(
            "Judgment exile ID String conversion exceeds depth 64".into(),
        ));
    }
    match value {
        Value::Null => Ok("null".into()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(_) => String::from_utf8(
            serde_jcs::to_vec(value).map_err(EngineError::serialization)?
        ).map_err(|error| EngineError::InvalidState(error.to_string())),
        Value::String(value) => Ok(value.clone()),
        Value::Array(values) => values.iter().map(|value| {
            if value.is_null() { Ok(String::new()) } else { judgment_seed_string(value, depth + 1) }
        }).collect::<Result<Vec<_>>>().map(|parts| parts.join(",")),
        Value::Object(value) if value.contains_key("toString") => Err(EngineError::InvalidState(
            "Judgment exile ID String conversion: TypeError: Cannot convert object to primitive value".into(),
        )),
        Value::Object(_) => Ok("[object Object]".into()),
    }
}

// main:100680-100701. Placement selection precedes summoned identity RNG.
fn apply_missionary(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let (selected, mut piece) = selected_piece(state, card, action)?;
    let candidates = missionary_candidates(state, selected)?;
    if candidates.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let chosen = candidates[(state.rng.sample()? * candidates.len() as f64).floor() as usize];
    state
        .rng
        .record_last_probability(1.0 / candidates.len() as f64, "source Missionary placement")?;
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
    write_piece(state, &piece);
    Ok(Vec::new())
}

// main:105027-105063. Capture order and duplicates weight the one type draw.
fn apply_necromancy(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let (selected, mut piece) = selected_piece(state, card, action)?;
    let types = necromancy_types(state);
    if types.is_empty() {
        return Err(EngineError::IllegalAction);
    }
    let revived = types[(state.rng.sample()? * types.len() as f64).floor() as usize].clone();
    // source는 중복 타입도 각각의 capture entry index로 균등 선택한다.
    state
        .rng
        .record_last_probability(1.0 / types.len() as f64, "source Necromancy capture entry")?;
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
            crate::replay::source_piece_label(&revived).unwrap_or(&revived)
        ),
    )?;
    Ok(Vec::new())
}

// main:103604-103633. Raw exile permits a collapsed origin despite the
// first-click UI excluding it, then force-removes the just-relocated piece.
fn apply_exile(state: &mut GameState, card: &CardSlot, action: &Action) -> Result<Vec<Piece>> {
    let (selected, mut piece) = selected_piece(state, card, action)?;
    let origin = exile_origin(&piece, selected)?;
    if state.at(origin).is_some() {
        return Err(EngineError::IllegalAction);
    }
    let returns_to_collapsed_square = crate::movement::collapsed(state, origin);
    state.board[selected.row as usize][selected.col as usize] = None;
    piece.moved = true;
    state.board[origin.row as usize][origin.col as usize] = Some(piece.clone());
    write_piece(state, &piece);
    if returns_to_collapsed_square {
        if let Some(removed) = crate::transition::force_remove_piece_at(state, origin, state.turn)?
        {
            crate::v7_piece_lifecycle::schedule_undead_resurrection(
                state, &removed, state.turn, false,
            )?;
        }
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn first_difference(left: &Value, right: &Value, path: &str) -> Option<String> {
        if left == right {
            return None;
        }
        match (left, right) {
            (Value::Object(left), Value::Object(right)) => {
                let keys = left
                    .keys()
                    .chain(right.keys())
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>();
                for key in keys {
                    let child = format!("{path}/{key}");
                    match (left.get(key), right.get(key)) {
                        (Some(left), Some(right)) => {
                            if let Some(difference) = first_difference(left, right, &child) {
                                return Some(difference);
                            }
                        }
                        _ => return Some(child),
                    }
                }
                None
            }
            (Value::Array(left), Value::Array(right)) => {
                if left.len() != right.len() {
                    return Some(format!("{path}/length"));
                }
                for (index, (left, right)) in left.iter().zip(right).enumerate() {
                    if let Some(difference) =
                        first_difference(left, right, &format!("{path}/{index}"))
                    {
                        return Some(difference);
                    }
                }
                None
            }
            (Value::Number(left), Value::Number(right)) if left.as_f64() == right.as_f64() => None,
            _ => Some(path.into()),
        }
    }

    fn card(id: &str) -> CardSlot {
        let definition = crate::draft::definitions()
            .definitions
            .iter()
            .find(|definition| definition["id"] == id)
            .unwrap()
            .clone();
        let mut slot: CardSlot = serde_json::from_value(definition).unwrap();
        slot.instance_id = format!("card-board-{id}");
        slot
    }

    #[test]
    fn pair_actions_follow_source_row_major_selection_order() {
        let mut state = GameState::new(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            11,
        )
        .unwrap();
        state.board = vec![vec![None; 8]; 8];
        for col in 0..3 {
            state.board[0][col] = Some(Piece::new("pawn", Color::Black, format!("black-{col}")));
        }
        let chain = card("chain");
        let ui = ui_targets(&state, &chain).unwrap().unwrap();
        assert_eq!(
            ui,
            vec![
                Square { row: 0, col: 0 },
                Square { row: 0, col: 1 },
                Square { row: 0, col: 2 }
            ]
        );
        let pairs = actions(&state, &chain).unwrap().unwrap();
        let selected = pairs
            .iter()
            .map(|action| action.target.as_ref().unwrap()["selections"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            selected,
            vec![
                json!([{"row":0,"col":0},{"row":0,"col":1}]),
                json!([{"row":0,"col":0},{"row":0,"col":2}]),
                json!([{"row":0,"col":1},{"row":0,"col":2}]),
            ]
        );
    }

    #[test]
    fn failed_effect_preserves_board_and_rng() {
        let mut state = GameState::new(GameConfig::default(), 11).unwrap();
        state.board = vec![vec![None; 8]; 8];
        state.board[7][3] = Some(Piece::new("queen", Color::White, "white-queen"));
        state
            .extra
            .insert("queensGambitFiles".into(), json!("malformed"));
        let before = state.clone();
        let card = card("queens-gambit");
        let action = Action::card(Color::White, &card, Some(json!({"row":7,"col":3})));
        assert!(apply(&mut state, &card, &action).is_err());
        assert_eq!(state, before);
    }

    fn require_faithful_board_receipt(receipt: &Value, label: &str) -> Result<()> {
        for (field, expected) in [
            (
                "sourceSha256",
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c",
            ),
            (
                "sourceProfileVersion",
                "accelerate-headless-semantic-v7-faithful-init-v1",
            ),
            (
                "sourceExecutionProfileSha256",
                "d811f0232ac38af4e45e0e4f93e89c49712142dfd0f2b5fe57d36f63cd05a29f",
            ),
            (
                "sourceCatalogVersion",
                "f80ebcd21759df179bccfb301415e672194de67691a6383beafc549538ffae7c",
            ),
            (
                "sourcePublicCatalogHash",
                "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
            ),
        ] {
            if receipt[field].as_str() != Some(expected) {
                return Err(EngineError::InvalidState(format!(
                    "{label}.{field}: current faithful authority required; source={}, expected={expected}",
                    receipt[field]
                )));
            }
        }
        Ok(())
    }

    /// 4개 공개 source 입력을 모두 인증하되 이 검사의 실행 범위는 Guard의
    /// raw applyCardEffect다. 공개 전체 전이 expectedPosition과 직접 callback
    /// sourceDirectPosition을 구분하며 이전 catalog의 고정 digest는 재사용하지 않는다.
    #[test]
    fn frozen_guard_direct_state_when_receipt_is_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_CARD_SOURCE_CASES") else {
            return;
        };
        use crate::tests::source_callback_fixture::{
            collect_case_diagnostics, compare_callback_envelope, compare_value,
            source_callback_state,
        };
        let lines = std::fs::read_to_string(path).unwrap();
        let mut failures = Vec::new();
        let mut checked = 0;
        let mut guard_count = 0;
        let mut cases = BTreeSet::new();
        for (index, line) in lines.lines().enumerate() {
            let label = format!("public source input {index}");
            collect_case_diagnostics(&label, &mut failures, |failures| {
                let receipt: Value =
                    serde_json::from_str(line).map_err(EngineError::serialization)?;
                require_faithful_board_receipt(&receipt, &label)?;
                if receipt["fixtureKind"] != "fresh-faithful-source-public-action" {
                    return Err(EngineError::InvalidState(format!(
                        "{label}.fixtureKind: fresh public source action required; source={}",
                        receipt["fixtureKind"]
                    )));
                }
                let name = receipt["sourceCaseName"].as_str().ok_or_else(|| {
                    EngineError::InvalidState(format!("{label}.sourceCaseName: string required"))
                })?;
                if !cases.insert(name.to_owned()) {
                    failures.push(format!("{label}.sourceCaseName: duplicate {name}"));
                }
                let mut state = source_callback_state(&receipt["sourcePosition"])?;
                source_callback_state(&receipt["expectedPosition"])?;
                let source_payload =
                    receipt["sourceAction"]["payload"]
                        .as_object()
                        .ok_or_else(|| {
                            EngineError::InvalidState(format!(
                                "{name}.sourceAction.payload: object required"
                            ))
                        })?;
                let source_action = board_callback_action_envelope(
                    Value::Object(source_payload.clone()),
                    &receipt["sourcePosition"]["positionId"],
                )?;
                compare_value(
                    &receipt["sourceAction"],
                    &source_action,
                    &format!("{name}.sourceActionIdentity"),
                    failures,
                )?;
                if source_payload.get("cardId").and_then(Value::as_str) != Some("guard") {
                    return Ok(());
                }
                guard_count += 1;
                if receipt["sourceCallback"] != "applyCardEffect"
                    || receipt["sourceDirectResult"]["ok"] != true
                {
                    return Err(EngineError::InvalidState(format!(
                        "{name}: successful raw applyCardEffect receipt required; callback={}, result={}",
                        receipt["sourceCallback"], receipt["sourceDirectResult"]
                    )));
                }
                source_callback_state(&receipt["sourceDirectPosition"])?;
                let card = state
                    .deck_slots
                    .get(state.turn)
                    .iter()
                    .find(|slot| slot.id == "guard")
                    .ok_or_else(|| {
                        EngineError::InvalidState(format!(
                            "{name}.sourcePosition.state.deckSlots: exact source Guard is absent"
                        ))
                    })?
                    .clone();
                let action = Action::card(state.turn, &card, source_payload.get("target").cloned());
                let actual_action = board_callback_action_envelope(
                    serde_json::to_value(&action).map_err(EngineError::serialization)?,
                    &receipt["sourcePosition"]["positionId"],
                )?;
                compare_value(
                    &receipt["sourceAction"],
                    &actual_action,
                    &format!("{name}.nativeActionIdentity"),
                    failures,
                )?;
                let before = state.clone();
                let callbacks = apply(&mut state, &card, &action)?;
                if !callbacks.is_empty() {
                    failures.push(format!(
                        "{name}: raw Guard unexpectedly requested callbacks: {callbacks:?}"
                    ));
                }
                compare_callback_envelope(
                    &state,
                    &receipt["sourceDirectPosition"],
                    &format!("{name}.rawGuard"),
                    failures,
                )?;
                compare_value(
                    &serde_json::to_value(&before.rng).map_err(EngineError::serialization)?,
                    &serde_json::to_value(&state.rng).map_err(EngineError::serialization)?,
                    &format!("{name}.nativeRngUnchanged"),
                    failures,
                )?;
                compare_value(
                    &receipt["sourcePosition"]["rng"],
                    &receipt["sourceDirectPosition"]["rng"],
                    &format!("{name}.sourceRngUnchanged"),
                    failures,
                )?;
                compare_value(
                    &serde_json::to_value(&before.history).map_err(EngineError::serialization)?,
                    &serde_json::to_value(&state.history).map_err(EngineError::serialization)?,
                    &format!("{name}.nativePrivateHistoryUnchanged"),
                    failures,
                )?;
                compare_value(
                    &receipt["sourcePosition"]["history"],
                    &receipt["sourceDirectPosition"]["history"],
                    &format!("{name}.sourcePrivateHistoryUnchanged"),
                    failures,
                )?;
                Ok(())
            });
            checked += 1;
        }
        if checked != 4 || guard_count != 1 {
            failures.push(format!("source-card authority requires all 4 rows and exactly 1 raw Guard; rows={checked}, guard={guard_count}"));
        }
        for name in [
            "chaos-seed37-draft",
            "chaos-seed19-first-active-play",
            "grand-seed37-first-play",
            "grand-seed19-first-active-play",
        ] {
            if !cases.contains(name) {
                failures.push(format!(
                    "source-card authority is missing reviewed input {name}"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn frozen_evacuation_pre_sound_state_when_receipt_is_supplied() {
        let (Some(cases_path), Some(sound_path)) = (
            std::env::var_os("ACCELERATE_V7_CARD_BOARD_CASES"),
            std::env::var_os("ACCELERATE_V7_EVACUATION_PRE_SOUND"),
        ) else {
            return;
        };
        let cases = std::fs::read_to_string(cases_path).unwrap();
        let receipt = cases
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|row| row["id"] == "emergency-evacuation")
            .expect("pinned evacuation action receipt");
        let sound: Value = serde_json::from_slice(&std::fs::read(sound_path).unwrap()).unwrap();
        assert_eq!(receipt["sourceSha256"], sound["sourceSha256"]);
        assert_eq!(
            receipt["sourcePosition"]["positionId"],
            sound["sourcePositionId"]
        );
        assert_eq!(receipt["sourceAction"]["actionId"], sound["sourceActionId"]);
        let mut state: GameState =
            serde_json::from_value(receipt["sourcePosition"]["state"].clone()).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.rng = serde_json::from_value(receipt["sourcePosition"]["rng"].clone()).unwrap();
        let card = state
            .deck_slots
            .get(state.turn)
            .iter()
            .find(|slot| slot.id == "emergency-evacuation")
            .unwrap()
            .clone();
        let expected_ui: Vec<Square> =
            serde_json::from_value(receipt["sourceUiTargets"].clone()).unwrap();
        assert_eq!(ui_targets(&state, &card).unwrap().unwrap(), expected_ui);
        let expected_actions = receipt["sourceLegalActions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|source| Action::card(state.turn, &card, source["payload"].get("target").cloned()))
            .collect::<Vec<_>>();
        assert_eq!(actions(&state, &card).unwrap().unwrap(), expected_actions);
        let action = Action::card(
            state.turn,
            &card,
            receipt["sourceAction"]["payload"].get("target").cloned(),
        );
        let history = state.history.clone();
        apply_evacuation_pre_sound(&mut state, &action).unwrap();
        let mut actual = serde_json::to_value(&state).unwrap();
        for envelope in ["rulesetId", "rng", "history"] {
            actual.as_object_mut().unwrap().remove(envelope);
        }
        let expected = &sound["preSound"]["state"];
        assert_eq!(
            first_difference(&actual, expected, "state"),
            None,
            "evacuation changed pre-sound source state"
        );
        assert_eq!(
            serde_json::to_value(&state.rng).unwrap(),
            sound["beforeRng"]
        );
        assert_eq!(state.history, history);
    }

    #[test]
    fn frozen_three_piece_evacuation_pre_sound_when_receipt_is_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_EVACUATION_MULTI") else {
            return;
        };
        let receipt: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            receipt["sourceSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        assert_eq!(receipt["sourceDirectResult"]["ok"], true);
        let mut state: GameState =
            serde_json::from_value(receipt["sourcePosition"]["state"].clone()).unwrap();
        state.ruleset_id = RULES_VERSION_V7.into();
        state.rng = serde_json::from_value(receipt["sourcePosition"]["rng"].clone()).unwrap();
        let card = state
            .deck_slots
            .get(state.turn)
            .iter()
            .find(|slot| slot.id == "emergency-evacuation")
            .unwrap()
            .clone();
        let expected_ui: Vec<Square> =
            serde_json::from_value(receipt["sourceUiTargets"].clone()).unwrap();
        assert_eq!(ui_targets(&state, &card).unwrap().unwrap(), expected_ui);
        let expected_actions = receipt["sourceLegalActions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|source| Action::card(state.turn, &card, source["payload"].get("target").cloned()))
            .collect::<Vec<_>>();
        assert_eq!(actions(&state, &card).unwrap().unwrap(), expected_actions);
        let action = Action::card(
            state.turn,
            &card,
            receipt["sourceAction"]["payload"].get("target").cloned(),
        );
        let before_rng = state.rng.clone();
        let before_history = state.history.clone();
        apply_evacuation_pre_sound(&mut state, &action).unwrap();
        let mut actual = serde_json::to_value(&state).unwrap();
        for envelope in ["rulesetId", "rng", "history"] {
            actual.as_object_mut().unwrap().remove(envelope);
        }
        assert_eq!(
            first_difference(&actual, &receipt["preSound"]["state"], "state"),
            None,
            "three-piece evacuation pre-sound source state diverged"
        );
        assert_eq!(state.rng, before_rng);
        assert_eq!(state.history, before_history);
    }

    /// Probe all four origin/destination visibility combinations. Each
    /// receipt compares the full source state before and after the sound
    /// callback, so concealed highlights cannot pass by matching one flag.
    #[test]
    fn frozen_evacuation_fog_privacy_when_receipt_is_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_EVACUATION_FOG_CASES") else {
            return;
        };
        let lines = std::fs::read_to_string(path).unwrap();
        let mut visibility_cases = BTreeSet::new();
        for line in lines.lines() {
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            assert_eq!(receipt["sourceDirectResult"]["ok"], true);
            let mut state: GameState =
                serde_json::from_value(receipt["sourcePosition"]["state"].clone()).unwrap();
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(receipt["sourcePosition"]["rng"].clone()).unwrap();
            let card = state
                .deck_slots
                .get(state.turn)
                .iter()
                .find(|slot| slot.id == "emergency-evacuation")
                .unwrap()
                .clone();
            let action = Action::card(
                state.turn,
                &card,
                receipt["sourceAction"]["payload"].get("target").cloned(),
            );
            let before = state.clone();
            let origin = Square { row: 4, col: 0 };
            let origin_visible = crate::observation::piece_visible_to_color_at_v7(
                &state,
                state.at(origin).unwrap(),
                origin,
                Color::Black,
            )
            .unwrap();
            assert_eq!(
                json!(origin_visible),
                receipt["sourceVisibility"]["originVisible"],
                "{} origin fog visibility diverged",
                receipt["id"]
            );
            apply_evacuation_pre_sound(&mut state, &action).unwrap();
            let destination = Square { row: 5, col: 0 };
            let destination_visible = crate::observation::piece_visible_to_color_at_v7(
                &state,
                state.at(destination).unwrap(),
                destination,
                Color::Black,
            )
            .unwrap();
            assert_eq!(
                json!(destination_visible),
                receipt["sourceVisibility"]["destinationVisible"],
                "{} destination fog visibility diverged",
                receipt["id"]
            );
            visibility_cases.insert((origin_visible, destination_visible));
            let mut actual = serde_json::to_value(&state).unwrap();
            for envelope in ["rulesetId", "rng", "history"] {
                actual.as_object_mut().unwrap().remove(envelope);
            }
            assert_eq!(
                first_difference(&actual, &receipt["preSound"]["state"], "state"),
                None,
                "{} pre-sound fog evacuation state diverged",
                receipt["id"]
            );
            assert_eq!(state.rng, before.rng);
            assert_eq!(state.history, before.history);
            state = before.clone();
            apply(&mut state, &card, &action).unwrap();
            let mut actual = serde_json::to_value(&state).unwrap();
            for envelope in ["rulesetId", "rng", "history"] {
                actual.as_object_mut().unwrap().remove(envelope);
            }
            assert_eq!(
                first_difference(&actual, &receipt["sourceDirectPosition"]["state"], "state"),
                None,
                "{} full direct fog evacuation state diverged",
                receipt["id"]
            );
            assert_eq!(
                serde_json::to_value(&state.rng).unwrap(),
                receipt["sourceDirectPosition"]["rng"]
            );
            assert_eq!(state.history, before.history);
        }
        assert_eq!(
            visibility_cases,
            BTreeSet::from([(false, false), (false, true), (true, false), (true, true),])
        );
    }

    #[test]
    fn frozen_board_campaign_terminal_effects_when_receipt_is_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_CARD_BOARD_CAMPAIGN_CASES") else {
            return;
        };
        use crate::tests::source_callback_fixture::{
            collect_case_diagnostics, compare_callback_envelope, compare_value,
            source_callback_state,
        };
        let lines = std::fs::read_to_string(path).unwrap();
        let mut checked = 0;
        let mut seen = BTreeSet::new();
        let mut failures = Vec::new();
        for (index, line) in lines.lines().enumerate() {
            let label = format!("synthetic campaign callback {index}");
            collect_case_diagnostics(&label, &mut failures, |failures| {
                let receipt: Value =
                    serde_json::from_str(line).map_err(EngineError::serialization)?;
                require_faithful_board_receipt(&receipt, &label)?;
                if receipt["fixtureKind"]
                    != "synthetic-contract-fresh-faithful-campaign-card-callback"
                    || receipt["sourceCallback"] != "applyCardEffect"
                    || receipt["sourceDirectResult"]["ok"] != true
                {
                    return Err(EngineError::InvalidState(format!(
                        "{label}: successful fresh synthetic applyCardEffect receipt required; kind={}, callback={}, result={}",
                        receipt["fixtureKind"],
                        receipt["sourceCallback"],
                        receipt["sourceDirectResult"]
                    )));
                }
                let id = receipt["id"]
                    .as_str()
                    .filter(|id| ["queens-gambit", "scarecrow"].contains(id))
                    .ok_or_else(|| {
                        EngineError::InvalidState(format!(
                            "{label}.id: reviewed campaign card ID required"
                        ))
                    })?;
                if !seen.insert(id.to_owned()) {
                    failures.push(format!("{label}.id: duplicate {id}"));
                }
                let mut state = source_callback_state(&receipt["sourcePosition"])?;
                source_callback_state(&receipt["sourceDirectPosition"])?;
                let card = state
                    .deck_slots
                    .get(state.turn)
                    .iter()
                    .find(|slot| slot.id == id)
                    .ok_or_else(|| {
                        EngineError::InvalidState(format!(
                            "{id}.sourcePosition.state.deckSlots: exact source card is absent"
                        ))
                    })?
                    .clone();
                let source_payload =
                    receipt["sourceAction"]["payload"]
                        .as_object()
                        .ok_or_else(|| {
                            EngineError::InvalidState(format!(
                                "{id}.sourceAction.payload: object required"
                            ))
                        })?;
                let action = Action::card(state.turn, &card, source_payload.get("target").cloned());
                let actual_action = board_callback_action_envelope(
                    serde_json::to_value(&action).map_err(EngineError::serialization)?,
                    &receipt["sourcePosition"]["positionId"],
                )?;
                compare_value(
                    &receipt["sourceAction"],
                    &actual_action,
                    &format!("{id}.nativeActionIdentity"),
                    failures,
                )?;
                let before = state.clone();
                apply(&mut state, &card, &action)?;
                // OracleRuntime.snapshot의 queued gameover replay 정산까지 같은 경계다.
                // 이 합성 raw callback 검사는 공개 캠페인 전체 전이 검사가 아니다.
                crate::replay::settle(&mut state)?;
                compare_callback_envelope(&state, &receipt["sourceDirectPosition"], id, failures)?;
                compare_value(
                    &receipt["sourcePosition"]["history"],
                    &receipt["sourceDirectPosition"]["history"],
                    &format!("{id}.sourcePrivateHistoryUnchanged"),
                    failures,
                )?;
                compare_value(
                    &serde_json::to_value(&before.history).map_err(EngineError::serialization)?,
                    &serde_json::to_value(&state.history).map_err(EngineError::serialization)?,
                    &format!("{id}.nativePrivateHistoryUnchanged"),
                    failures,
                )?;
                compare_value(
                    &json!("gameover"),
                    &receipt["sourceDirectPosition"]["state"]["mode"],
                    &format!("{id}.sourceTerminalMode"),
                    failures,
                )?;
                compare_value(
                    &json!("black"),
                    &receipt["sourceDirectPosition"]["state"]["winner"],
                    &format!("{id}.sourceWinner"),
                    failures,
                )?;
                Ok(())
            });
            checked += 1;
        }
        if checked != 2
            || seen != BTreeSet::from(["queens-gambit".to_owned(), "scarecrow".to_owned()])
        {
            failures.push(format!(
                "synthetic campaign receipt requires both cards once; rows={checked}, IDs={seen:?}"
            ));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// `getTargetSquares` excludes a collapsed origin, but the source's raw
    /// `exile(target)` accepts one. This checks the raw callback and its
    /// separately scheduled undead resurrection without broadening UI legal.
    #[test]
    fn frozen_exile_collapsed_direct_effect_when_receipt_is_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_EXILE_COLLAPSED_CASES") else {
            return;
        };
        let lines = std::fs::read_to_string(path).unwrap();
        let mut checked = 0;
        for line in lines.lines() {
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            assert_eq!(receipt["cardId"], "exile");
            assert_eq!(receipt["sourceDirectResult"]["ok"], true);
            let mut state: GameState =
                serde_json::from_value(receipt["sourcePosition"]["state"].clone()).unwrap();
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(receipt["sourcePosition"]["rng"].clone()).unwrap();
            let card = state
                .deck_slots
                .get(state.turn)
                .iter()
                .find(|slot| slot.id == "exile")
                .unwrap()
                .clone();
            let action = Action::card(
                state.turn,
                &card,
                receipt["sourceAction"]["payload"].get("target").cloned(),
            );
            let before = state.clone();
            assert!(apply(&mut state, &card, &action).unwrap().is_empty());
            let mut actual = serde_json::to_value(&state).unwrap();
            for envelope in ["rulesetId", "rng", "history"] {
                actual.as_object_mut().unwrap().remove(envelope);
            }
            assert_eq!(
                first_difference(&actual, &receipt["sourceDirectPosition"]["state"], "state"),
                None,
                "collapsed exile {} state mismatch",
                receipt["id"]
            );
            assert_eq!(
                serde_json::to_value(&state.rng).unwrap(),
                receipt["sourceDirectPosition"]["rng"],
                "collapsed exile {} RNG mismatch",
                receipt["id"]
            );
            assert_eq!(state.history, before.history);
            assert_eq!(
                receipt["sourcePosition"]["history"],
                receipt["sourceDirectPosition"]["history"]
            );
            checked += 1;
        }
        assert_eq!(
            checked, 2,
            "collapsed exile receipt must cover ordinary and undead victims"
        );
    }

    /// 전체 복귀 콜백을 fresh faithful source에서 재실행한 자료만 사용한다.
    /// 합성 중간 상태의 importer를 공유하며 state/RNG/history와 원문
    /// envelope identity를 함께 비교한다. 공개 spatial admission은 바꾸지 않는다.
    #[test]
    fn frozen_judgment_returns_when_receipts_are_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_JUDGMENT_RETURN_CASES") else {
            return;
        };
        use crate::tests::source_callback_fixture::{compare_value, source_callback_state};
        let lines = std::fs::read_to_string(path).unwrap();
        let mut checked = 0;
        let mut mismatches = Vec::new();
        for line in lines.lines() {
            let receipt: Value = serde_json::from_str(line).unwrap();
            let id = receipt["id"].as_str().unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c",
                "{id}: source SHA"
            );
            assert_eq!(
                receipt["sourceProfileVersion"], "accelerate-headless-semantic-v7-faithful-init-v1",
                "{id}: source profile"
            );
            assert_eq!(
                receipt["sourceExecutionProfileSha256"],
                "d811f0232ac38af4e45e0e4f93e89c49712142dfd0f2b5fe57d36f63cd05a29f",
                "{id}: source manifest"
            );
            assert_eq!(
                receipt["sourceCatalogVersion"],
                "f80ebcd21759df179bccfb301415e672194de67691a6383beafc549538ffae7c",
                "{id}: source catalog"
            );
            assert_eq!(
                receipt["sourcePublicCatalogHash"], "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
                "{id}: public source catalog"
            );
            assert_eq!(
                receipt["sourceCallback"], "returnJudgmentExilesForDraft",
                "{id}: callback boundary"
            );
            let mut state = source_callback_state(&receipt["sourcePosition"])
                .unwrap_or_else(|error| panic!("{id}: invalid source input: {error}"));
            source_callback_state(&receipt["sourceDirectPosition"])
                .unwrap_or_else(|error| panic!("{id}: invalid source result: {error}"));
            let before_rng = state.rng.clone();
            let before_history = state.history.clone();
            let returned =
                return_judgment_exiles_for_draft(&mut state, receipt["phase"].as_str().unwrap())
                    .unwrap_or_else(|error| panic!("{id}: native return callback failed: {error}"));
            assert_eq!(
                json!(returned),
                receipt["sourceReturned"],
                "{id}: returned count"
            );
            assert_eq!(state.rng, before_rng, "{id}: native callback consumed RNG");
            assert_eq!(
                state.history, before_history,
                "{id}: native callback changed private history"
            );
            let mut actual_state = serde_json::to_value(&state).unwrap();
            for field in ["rulesetId", "rng", "history"] {
                actual_state.as_object_mut().unwrap().remove(field);
            }
            let content = json!({
                "protocolVersion": crate::v7_host::V7_POSITION_PROTOCOL,
                "rulesVersion": state.ruleset_id,
                "catalogVersion": crate::v7_execution_profile::catalog_version().unwrap(),
                "state": actual_state,
                "rng": state.rng,
                "history": state.history,
            });
            let position_id = format!("{:x}", Sha256::digest(serde_jcs::to_vec(&content).unwrap()));
            let mut envelope = content.as_object().unwrap().clone();
            envelope.insert("positionId".into(), json!(position_id));
            compare_value(
                &receipt["sourceDirectPosition"],
                &Value::Object(envelope),
                id,
                &mut mismatches,
            )
            .unwrap_or_else(|error| panic!("{id}: invalid native result: {error}"));
            checked += 1;
        }
        assert_eq!(
            checked, 10,
            "Judgment return receipt must contain all 10 reviewed cases"
        );
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    fn board_callback_action_envelope(payload: Value, position_id: &Value) -> Result<Value> {
        let bytes = serde_jcs::to_vec(&payload).map_err(EngineError::serialization)?;
        Ok(json!({
            "protocolVersion":"accelerate-action-v1",
            "positionId":position_id,
            "actionId":format!("{:x}", Sha256::digest(bytes)),
            "payload":payload,
        }))
    }

    /// 20개 기본 자료와 구분되는 26개 확장 자료의 first-click·전체 행동
    /// 목록·applyCardEffect 경계를 검증한다. 누락된 필드를 빈 목록으로
    /// 바꾸지 않으며 정확한 사례/필드 오류를 모아 마지막에 실패시킨다.
    #[test]
    fn frozen_board_card_direct_effects_when_receipt_is_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_CARD_BOARD_CASES") else {
            return;
        };
        let lines = std::fs::read_to_string(path).unwrap();
        use crate::tests::source_callback_fixture::{
            collect_case_diagnostics, compare_callback_envelope, compare_value,
            source_callback_state,
        };
        let mut checked = 0;
        let mut seen_cards = BTreeSet::new();
        let mut failures = Vec::new();
        for (index, line) in lines.lines().enumerate() {
            let label = format!("board receipt {index}");
            collect_case_diagnostics(&label, &mut failures, |failures| {
                let receipt: Value =
                    serde_json::from_str(line).map_err(EngineError::serialization)?;
                if receipt["sourceSha256"]
                    != "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
                {
                    return Err(EngineError::InvalidState(format!(
                        "{label}.sourceSha256: frozen client SHA differs: {}",
                        receipt["sourceSha256"]
                    )));
                }
                let id = receipt["id"]
                    .as_str()
                    .filter(|id| IDS.contains(id))
                    .ok_or_else(|| {
                        EngineError::InvalidState(format!(
                            "{label}.id: owned card ID required, source={}",
                            receipt["id"]
                        ))
                    })?;
                seen_cards.insert(id.to_owned());
                let label = format!("{label} {id}");
                let expected_ui = receipt.get("sourceUiTargets").filter(|value| value.is_array())
                    .ok_or_else(|| EngineError::InvalidState(format!(
                        "{label}.sourceUiTargets: required complete array missing or non-array; source={:?}; \
                         the 20-case basic receipt omits surfaces; use the fresh 26-case extended recipe",
                        receipt.get("sourceUiTargets")
                    )))?;
                let expected_actions = receipt.get("sourceLegalActions").filter(|value| value.is_array())
                    .ok_or_else(|| EngineError::InvalidState(format!(
                        "{label}.sourceLegalActions: required complete action array missing or non-array; source={:?}",
                        receipt.get("sourceLegalActions")
                    )))?;
                if receipt["sourceDirectResult"]["ok"] != true {
                    return Err(EngineError::InvalidState(format!(
                        "{label}.sourceDirectResult.ok: successful source callback required; source={}",
                        receipt["sourceDirectResult"]
                    )));
                }
                let mut state = source_callback_state(&receipt["sourcePosition"])?;
                let card = state
                    .deck_slots
                    .get(state.turn)
                    .iter()
                    .find(|slot| slot.id == id)
                    .ok_or_else(|| {
                        EngineError::InvalidState(format!(
                            "{label}.sourcePosition.state.deckSlots: selected source card is absent"
                        ))
                    })?
                    .clone();
                match ui_targets(&state, &card) {
                    Ok(Some(actual)) => compare_value(
                        expected_ui,
                        &serde_json::to_value(actual).map_err(EngineError::serialization)?,
                        &format!("{label}.sourceUiTargets"),
                        failures,
                    )?,
                    other => failures.push(format!("{label}.sourceUiTargets: native={other:?}")),
                }
                match actions(&state, &card) {
                    Ok(Some(actual)) => {
                        let actual = actual
                            .into_iter()
                            .map(|action| {
                                board_callback_action_envelope(
                                    serde_json::to_value(action)
                                        .map_err(EngineError::serialization)?,
                                    &receipt["sourcePosition"]["positionId"],
                                )
                            })
                            .collect::<Result<Vec<_>>>()?;
                        compare_value(
                            expected_actions,
                            &Value::Array(actual),
                            &format!("{label}.sourceLegalActions"),
                            failures,
                        )?;
                    }
                    other => failures.push(format!("{label}.sourceLegalActions: native={other:?}")),
                }
                let source_payload = receipt["sourceAction"]["payload"].as_object()
                    .ok_or_else(|| EngineError::InvalidState(format!(
                        "{label}.sourceAction.payload: selected source payload must be an object"
                    )))?;
                let action = Action::card(state.turn, &card, source_payload.get("target").cloned());
                let actual_action = board_callback_action_envelope(
                    serde_json::to_value(&action).map_err(EngineError::serialization)?,
                    &receipt["sourcePosition"]["positionId"],
                )?;
                compare_value(
                    &receipt["sourceAction"],
                    &actual_action,
                    &format!("{label}.sourceAction"),
                    failures,
                )?;
                let before = state.clone();
                if let Err(error) = apply(&mut state, &card, &action) {
                    compare_callback_envelope(
                        &state,
                        &receipt["sourcePosition"],
                        &format!("{label}.failedEffectAtomicity"),
                        failures,
                    )?;
                    return Err(EngineError::InvalidState(format!(
                        "{label}: native rejected successful source callback: {error}"
                    )));
                }
                compare_callback_envelope(
                    &state,
                    &receipt["sourceDirectPosition"],
                    &label,
                    failures,
                )?;
                compare_value(
                    &receipt["sourcePosition"]["history"],
                    &receipt["sourceDirectPosition"]["history"],
                    &format!("{label}.sourcePrivateHistoryUnchanged"),
                    failures,
                )?;
                compare_value(
                    &serde_json::to_value(&before.history).map_err(EngineError::serialization)?,
                    &serde_json::to_value(&state.history).map_err(EngineError::serialization)?,
                    &format!("{label}.nativePrivateHistoryUnchanged"),
                    failures,
                )?;
                Ok(())
            });
            checked += 1;
        }
        if checked != 26 {
            failures.push(format!(
                "board-card extended receipt requires all 26 cases; received {checked}; \
                 the basic 20-case receipt is a different input contract"
            ));
        }
        for id in IDS {
            if !seen_cards.contains(*id) {
                failures.push(format!(
                    "board-card extended receipt is missing owned card {id}"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
