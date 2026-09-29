use crate::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(crate) const ORTHO: &[(i8, i8)] = &[(-1, 0), (1, 0), (0, -1), (0, 1)];
pub(crate) const DIAG: &[(i8, i8)] = &[(-1, -1), (-1, 1), (1, -1), (1, 1)];
pub(crate) const KING: &[(i8, i8)] = &[
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -1),
    (0, 1),
    (1, -1),
    (1, 0),
    (1, 1),
];
pub(crate) const KNIGHT: &[(i8, i8)] = &[
    (-2, -1),
    (-2, 1),
    (-1, -2),
    (-1, 2),
    (1, -2),
    (1, 2),
    (2, -1),
    (2, 1),
];
const CAMEL: &[(i8, i8)] = &[
    (-3, -1),
    (-3, 1),
    (-1, -3),
    (-1, 3),
    (1, -3),
    (1, 3),
    (3, -1),
    (3, 1),
];
const EAGLE: &[(i8, i8)] = &[
    (-2, -2),
    (-2, 0),
    (-2, 2),
    (0, -2),
    (0, 2),
    (2, -2),
    (2, 0),
    (2, 2),
];

pub fn implemented_piece_types() -> &'static [&'static str] {
    &[
        "pawn",
        "squire",
        "standardBearer",
        "rook",
        "bishop",
        "queen",
        "king",
        "knight",
        "royalKnight",
        "unicorn",
        "amazon",
        "man",
        "guard",
        "camel",
        "alfil",
        "ferz",
        "eagle",
        "alibaba",
        "knightmaster",
        "cannon",
        "grasshopper",
        "princess",
        "clockwork",
        "campfire",
        "wall",
        "coffin",
        "scarecrow",
        "checker",
        "checkerKing",
        "missionary",
        "bigRook",
        "bigBishop",
    ]
}

pub(crate) fn legal_actions(state: &GameState) -> Result<Vec<Action>> {
    if state.result().is_some() || state.mode == "gameover" {
        return Ok(Vec::new());
    }
    if state.mode == "draft" {
        return crate::draft::legal_actions(state);
    }
    if let Some(pending) = state
        .extra
        .get("pendingPromotion")
        .filter(|window| !window.is_null())
    {
        let color: Color = serde_json::from_value(
            pending
                .get("color")
                .cloned()
                .ok_or_else(|| EngineError::InvalidState("promotion color missing".into()))?,
        )
        .map_err(EngineError::serialization)?;
        let choices = pending
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| EngineError::InvalidState("promotion choices missing".into()))?;
        return choices
            .iter()
            .map(|choice| {
                let kind = choice.as_str().ok_or_else(|| {
                    EngineError::UnsupportedFeature("compound promotion choice".into())
                })?;
                if !implemented_piece_types().contains(&kind) {
                    return Err(EngineError::UnsupportedFeature(format!(
                        "promotion result {kind}"
                    )));
                }
                let mut action = Action::movement(
                    color,
                    Square { row: 0, col: 0 },
                    MoveTarget::at(Square { row: 0, col: 0 }),
                );
                action.kind = ActionKind::PromotionChoice;
                action.from = None;
                action.destination = None;
                action.extra.insert("promotionType".into(), json!(kind));
                Ok(action)
            })
            .collect();
    }
    if state.mode != "play" {
        return Err(EngineError::UnsupportedFeature(format!(
            "game mode {}",
            state.mode
        )));
    }
    let mut actions = legal_move_actions(state)?;
    if !has_checker_capture(state)? && forced_piece_id(state).is_none() {
        for card in state.deck_slots.get(state.turn) {
            if usable_card(card) {
                actions.extend(crate::transition::card_actions(state, card)?);
            }
        }
    }
    Ok(actions)
}

/// Source collectValidAiActions(includeCards:false) has a separate movement
/// surface. Unrelated unknown card families must not poison royal threat probes.
pub(crate) fn legal_move_actions(state: &GameState) -> Result<Vec<Action>> {
    ensure_supported(state)?;
    legal_move_candidates(state)
}

/// Movement-only first-play profile observed in the frozen v7 headless
/// adapter. This does not enumerate cards or authorize Position execution.
/// The three source-reachable active-only draft samples had the same 20 raw
/// moves, and all 20 survived actionStream(legal=true). The complete public
/// action stream and post-apply state/RNG still belong to the Position gate.
#[allow(dead_code, reason = "v7 Position opening gate is staged")]
pub(crate) fn v7_opening_legal_move_actions(state: &GameState) -> Result<Vec<Action>> {
    ensure_v7_orthodox_opening(state)?;
    let moves = legal_move_candidates(state)?;
    if moves != v7_expected_orthodox_first_play_moves() {
        return Err(EngineError::UnsupportedFeature(
            "v7 opening movement differs from the source-verified orthodox profile".into(),
        ));
    }
    Ok(moves)
}

/// Frozen v7 actionStream movement prefix, shared by the three verified
/// active-only first-play samples. Compare the complete ordered payload so a
/// future candidate change cannot silently preserve only its count and flags.
fn v7_expected_orthodox_first_play_moves() -> Vec<Action> {
    let mut actions = Vec::with_capacity(20);
    for col in 0..8 {
        let from = Square { row: 6, col };
        actions.push(Action::movement(
            Color::White,
            from,
            MoveTarget::at(Square { row: 5, col }),
        ));
        let mut double = MoveTarget::at(Square { row: 4, col });
        double
            .flags
            .insert("standardPawnDoubleStep".into(), json!(true));
        actions.push(Action::movement(Color::White, from, double));
    }
    for (from_col, to_col) in [(1, 0), (1, 2), (6, 5), (6, 7)] {
        actions.push(Action::movement(
            Color::White,
            Square {
                row: 7,
                col: from_col,
            },
            MoveTarget::at(Square {
                row: 5,
                col: to_col,
            }),
        ));
    }
    actions
}

/// Validate an unbound movement payload against the same narrow source
/// profile. Binding, card actions, and execution remain separate boundaries.
#[allow(dead_code, reason = "v7 Position opening gate is staged")]
pub(crate) fn v7_opening_validate_move(state: &GameState, action: &Action) -> Result<()> {
    if action.position_key.is_some() {
        return Err(EngineError::IllegalAction);
    }
    if v7_opening_legal_move_actions(state)?.contains(action) {
        Ok(())
    } else {
        Err(EngineError::IllegalAction)
    }
}

/// Ordered movement-only actions for the source-verified seed-19 normal
/// first-play position after White uses Relay. This is a staged candidate
/// surface: Position binding, swap execution, cards and general v7 play remain
/// behind their separate guards.
#[allow(dead_code, reason = "v7 Relay swap execution gate is staged")]
pub(crate) fn v7_opening_relay_legal_move_actions(state: &GameState) -> Result<Vec<Action>> {
    ensure_v7_relay_after_opening(state)?;
    let moves = legal_move_candidates_with_relay(state)?;
    let swaps = moves
        .iter()
        .filter(|action| {
            action
                .destination
                .as_ref()
                .is_some_and(|to| to.flag("relaySwap"))
        })
        .count();
    let base_moves = moves
        .iter()
        .filter(|action| {
            action
                .destination
                .as_ref()
                .is_none_or(|to| !to.flag("relaySwap"))
        })
        .cloned()
        .collect::<Vec<_>>();
    if moves.len() != 148 || swaps != 128 || base_moves != v7_expected_orthodox_first_play_moves() {
        return Err(EngineError::UnsupportedFeature(
            "v7 Relay first-play candidates differ from source-verified 148 moves".into(),
        ));
    }
    Ok(moves)
}

/// The movement-only source profiles verified from reachable first-play
/// Positions. Card actions, Position binding and applying a move are separate
/// contracts; none of these variants opens the general v7 public legal gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code, reason = "v7 Position first-play gate is staged")]
pub(crate) enum V7VerifiedFirstPlayProfile {
    Normal,
    Chaos,
    Grand,
    NormalRelayAfter,
}

#[allow(dead_code, reason = "v7 Position first-play gate is staged")]
pub(crate) fn v7_verified_first_play_movement_actions(
    state: &GameState,
) -> Result<(V7VerifiedFirstPlayProfile, Vec<Action>)> {
    let relay = state.extra.get("relay");
    if relay == Some(&json!({"black":false,"white":true})) {
        return Ok((
            V7VerifiedFirstPlayProfile::NormalRelayAfter,
            v7_opening_relay_legal_move_actions(state)?,
        ));
    }
    if relay != Some(&json!({"black":false,"white":false})) {
        return Err(EngineError::UnsupportedFeature(
            "v7 first-play Relay/Solidarity profile".into(),
        ));
    }
    let actions = v7_opening_legal_move_actions(state)?;
    let profile = match state.extra.get("gameStyle").and_then(Value::as_str) {
        Some("normal") => V7VerifiedFirstPlayProfile::Normal,
        Some("chaos") => V7VerifiedFirstPlayProfile::Chaos,
        Some("grand") => V7VerifiedFirstPlayProfile::Grand,
        _ => {
            return Err(EngineError::UnsupportedFeature(
                "v7 first-play game style".into(),
            ));
        }
    };
    Ok((profile, actions))
}

/// In the verified orthodox profile, a Relay exchange selects a different
/// allied single-cell piece on the same row or column. This descriptor pins
/// the target identity so an eventual executor can revalidate it against the
/// action's position; Solidarity and altered footprints are outside the gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RelaySwap {
    #[allow(dead_code, reason = "v7 Relay swap execution gate is staged")]
    pub(crate) from: Square,
    pub(crate) to: Square,
    mover_id: String,
    #[allow(dead_code, reason = "v7 Relay swap execution gate is staged")]
    pub(crate) target_id: String,
}

impl RelaySwap {
    fn for_pair(state: &GameState, mover: &Piece, from: Square, to: Square) -> Option<Self> {
        let current = state.at(from)?;
        let target = state.at(to)?;
        if current.id != mover.id
            || !state.flag("relay", mover.color)
            || state.flag("solidarity", mover.color)
            || mover.id == target.id
            || mover.color != target.color
            || from.row != to.row && from.col != to.col
            || [mover, target].into_iter().any(|piece| {
                piece.is_large()
                    || piece.ability_kind() == "slime"
                    || matches!(
                        piece.kind.as_str(),
                        "wall" | "football" | "blackHole" | "monster" | "coffin"
                    )
            })
        {
            return None;
        }
        Some(Self {
            from,
            to,
            mover_id: mover.id.clone(),
            target_id: target.id.clone(),
        })
    }

    #[allow(dead_code, reason = "v7 Relay swap execution gate is staged")]
    pub(crate) fn from_target(
        state: &GameState,
        mover: &Piece,
        from: Square,
        target: &MoveTarget,
    ) -> Result<Option<Self>> {
        if !target.flag("relaySwap") {
            return Ok(None);
        }
        if target.flags.len() != 1 || target.flags.get("relaySwap") != Some(&json!(true)) {
            return Err(EngineError::IllegalAction);
        }
        Self::for_pair(state, mover, from, target.square())
            .map(Some)
            .ok_or(EngineError::IllegalAction)
    }

    fn target(&self) -> MoveTarget {
        let mut target = MoveTarget::at(self.to);
        target.flags.insert("relaySwap".into(), json!(true));
        target
    }

    /// Exchange two occupied cells without changing either identity or the
    /// target piece. The source marks only the acting piece as moved. A stale
    /// descriptor must not be applied to a different occupant.
    fn board_after(&self, state: &GameState) -> Result<Vec<Vec<Option<Piece>>>> {
        let mover = state.at(self.from).ok_or(EngineError::IllegalAction)?;
        if Self::for_pair(state, mover, self.from, self.to).as_ref() != Some(self) {
            return Err(EngineError::IllegalAction);
        }
        let mut board = state.board.clone();
        let mut mover = board[self.from.row as usize][self.from.col as usize]
            .take()
            .ok_or(EngineError::IllegalAction)?;
        let other = board[self.to.row as usize][self.to.col as usize]
            .take()
            .ok_or(EngineError::IllegalAction)?;
        if mover.id != self.mover_id || other.id != self.target_id {
            return Err(EngineError::IllegalAction);
        }
        mover.moved = true;
        board[self.from.row as usize][self.from.col as usize] = Some(other);
        board[self.to.row as usize][self.to.col as usize] = Some(mover);
        Ok(board)
    }
}

/// Stage only the board effect of a source-legal normal first-play Relay
/// action. The caller must bind/check the Position key and perform the source
/// turn, replay, clock, history, and RNG transition separately. Candidate
/// enumeration bounds work to the verified 148-move profile.
#[allow(dead_code, reason = "v7 Relay swap Position execution gate is staged")]
pub(crate) fn v7_opening_relay_swap_board(
    state: &GameState,
    action: &Action,
) -> Result<Vec<Vec<Option<Piece>>>> {
    let mut semantic = action.clone();
    semantic.position_key = None;
    if !v7_opening_relay_legal_move_actions(state)?.contains(&semantic) {
        return Err(EngineError::IllegalAction);
    }
    let from = action.from.ok_or(EngineError::IllegalAction)?;
    let target = action
        .destination
        .as_ref()
        .ok_or(EngineError::IllegalAction)?;
    let mover = state.at(from).ok_or(EngineError::IllegalAction)?;
    RelaySwap::from_target(state, mover, from, target)?
        .ok_or(EngineError::IllegalAction)?
        .board_after(state)
}

fn relay_swap_targets(state: &GameState, mover: &Piece, from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let to = Square { row, col };
            if let Some(swap) = RelaySwap::for_pair(state, mover, from, to) {
                moves.push(swap.target());
            }
        }
    }
    moves
}

/// Proves only that the source-shaped first white play has a legal pawn move.
/// This witness neither enumerates nor authorizes actions. `None` means the
/// position is outside the proven profile and the general v7 guard stays closed.
#[allow(dead_code, reason = "v7 first-play no-action flow is staged")]
pub(crate) fn v7_first_play_has_legal_move_witness(
    state: &GameState,
    color: Color,
) -> Result<Option<bool>> {
    if color != Color::White || state.ruleset_id != RULES_VERSION_V7 {
        return Ok(None);
    }
    let verified = if state.extra.get("relay") == Some(&json!({"black":false,"white":true})) {
        ensure_v7_relay_after_opening(state)
    } else if state.extra.get("relay") == Some(&json!({"black":false,"white":false})) {
        ensure_v7_orthodox_opening(state)
    } else {
        return Ok(None);
    };
    match verified {
        Ok(()) => {}
        Err(EngineError::UnsupportedFeature(_)) => return Ok(None),
        Err(error) => return Err(error),
    }
    let from = Square { row: 6, col: 0 };
    let pawn = state
        .at(from)
        .ok_or_else(|| EngineError::InvalidState("source-shaped first-play pawn missing".into()))?;
    let forward = MoveTarget::at(Square { row: 5, col: 0 });
    if piece_moves(state, pawn, from)?.contains(&forward) {
        Ok(Some(true))
    } else {
        Err(EngineError::InvalidState(
            "source-shaped first-play pawn witness disappeared".into(),
        ))
    }
}

fn legal_move_candidates(state: &GameState) -> Result<Vec<Action>> {
    legal_move_candidates_inner(state, false)
}

fn legal_move_candidates_with_relay(state: &GameState) -> Result<Vec<Action>> {
    legal_move_candidates_inner(state, true)
}

fn legal_move_candidates_inner(state: &GameState, relay: bool) -> Result<Vec<Action>> {
    let mut actions = Vec::new();
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let from = Square { row, col };
            let Some(piece) = state.at(from) else {
                continue;
            };
            if piece.color != state.turn || !seen.insert(piece.id.clone()) {
                continue;
            }
            if frozen(piece)
                || piece
                    .extra
                    .get("staked")
                    .and_then(|s| s.get("remaining"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    > 0
            {
                continue;
            }
            if relay {
                for target in relay_swap_targets(state, piece, from) {
                    actions.push(Action::movement(state.turn, from, target));
                }
            }
            for target in piece_moves(state, piece, from)? {
                actions.push(Action::movement(state.turn, from, target));
            }
        }
    }
    let has_checker_capture = actions.iter().any(|action| {
        action
            .destination
            .as_ref()
            .is_some_and(|target| target.flag("checkerCapture"))
    });
    if has_checker_capture {
        actions.retain(|action| {
            action
                .destination
                .as_ref()
                .is_some_and(|target| target.flag("checkerCapture"))
        });
    }
    if let Some(forced_id) = forced_piece_id(state) {
        actions.retain(|action| {
            action
                .from
                .and_then(|square| state.at(square))
                .is_some_and(|piece| piece.id == forced_id)
        });
    }
    Ok(actions)
}

fn ensure_v7_relay_after_opening(state: &GameState) -> Result<()> {
    let unsupported = |reason: &str| {
        EngineError::UnsupportedFeature(format!("v7 Relay first-play profile: {reason}"))
    };
    if state.extra.get("relay") != Some(&json!({"black":false,"white":true}))
        || state.extra.get("gameStyle") != Some(&json!("normal"))
        || state.cards_used_this_turn.white != 1
        || state.cards_used_this_turn.black != 0
        || state.extra.get("replayEventNonce") != Some(&json!(3))
        || state
            .extra
            .get("replayEvents")
            .and_then(Value::as_array)
            .is_none_or(|events| events.len() != 3)
    {
        return Err(unsupported("effect or replay counters"));
    }
    let used: Vec<_> = state
        .deck_slots
        .white
        .iter()
        .filter(|card| !card.vacant && card.used)
        .collect();
    if used.len() != 1
        || used[0].id != "relay"
        || used[0].effect != "relay"
        || used[0].recovering
        || used[0]
            .extra
            .get("usedAt")
            .and_then(Value::as_i64)
            .is_none_or(|stamp| stamp <= 0)
    {
        return Err(unsupported("used Relay card identity"));
    }
    // The source Relay action changes no board piece. Restore only its known
    // effect/counters in a private clone, then use the full orthodox profile
    // check to reject any other changed rule state or footprint.
    let mut baseline = state.clone();
    baseline.cards_used_this_turn.white = 0;
    baseline
        .extra
        .insert("relay".into(), json!({"black":false,"white":false}));
    baseline.extra.insert("replayEventNonce".into(), json!(2));
    if let Some(card) = baseline.deck_slots.white.iter_mut().find(|card| card.used) {
        card.used = false;
        card.extra.shift_remove("usedAt");
    }
    if let Some(events) = baseline
        .extra
        .get_mut("replayEvents")
        .and_then(Value::as_array_mut)
    {
        events.pop();
    }
    ensure_v7_orthodox_opening(&baseline)
}

#[allow(dead_code, reason = "v7 Position opening gate is staged")]
fn ensure_v7_orthodox_opening(state: &GameState) -> Result<()> {
    // Presentation, draft, clock, and replay fields may vary across the three
    // verified seed-19 first-play Positions. Every other source extra field,
    // including inactive rule fields, must retain its source default.
    const VARIABLE_EXTRAS: &[&str] = &[
        "boardHistory",
        "cardAcquisitionNonce",
        "clock",
        "draft",
        "draftClock",
        "endDraftDone",
        "endPhaseStartMove",
        "gameStyle",
        "logs",
        "middleDraftDone",
        "notationEvent",
        "notationEvents",
        "notationTimeline",
        "openingAutoNoticeShown",
        "positionCounts",
        "repetitionSalt",
        "replayBaseFrame",
        "replayEventNonce",
        "replayEvents",
        "replayStartedAt",
        "replayTailFrame",
    ];
    // SHA-256 of JCS({ stable source extra fields }), shared by normal,
    // chaos, and grand active-only first-play oracle snapshots. This compact
    // profile check also rejects added, removed, or changed inactive fields.
    const STABLE_EXTRAS_SHA256: &str =
        "a4bf8022aca2454da4c23a4e948a3bc02fde9beefe8dbd752411c22b8d275534";
    let unsupported = |reason: &str| {
        EngineError::UnsupportedFeature(format!("v7 orthodox first-play profile: {reason}"))
    };
    if state.ruleset_id != RULES_VERSION_V7
        || state.mode != "play"
        || state.result().is_some()
        || state.turn != Color::White
        || state.actions_remaining != 1
        || state.move_count != 0
        || state.full_move != 1
        || state.turns_taken.white != 0
        || state.turns_taken.black != 0
        || state.cards_used_this_turn.white != 0
        || state.cards_used_this_turn.black != 0
        || state.en_passant.is_some()
        || !state.captures.white.is_empty()
        || !state.captures.black.is_empty()
    {
        return Err(unsupported("turn or capture state"));
    }
    let style = state
        .extra
        .get("gameStyle")
        .and_then(Value::as_str)
        .ok_or_else(|| unsupported("game style"))?;
    let (cards_per_side, slots_per_side, draft_events) = match style {
        "normal" => (1, 3, 2),
        "chaos" => (2, 6, 2),
        "grand" => (6, 6, 12),
        _ => return Err(unsupported("game style")),
    };
    let stable = state
        .extra
        .iter()
        .filter(|(name, _)| !VARIABLE_EXTRAS.contains(&name.as_str()))
        .map(|(name, value)| (name.as_str(), value))
        .collect::<BTreeMap<_, _>>();
    let stable_json = serde_jcs::to_vec(&stable).map_err(EngineError::serialization)?;
    if format!("{:x}", Sha256::digest(stable_json)) != STABLE_EXTRAS_SHA256 {
        return Err(unsupported("unverified rule-state defaults"));
    }
    let clock = state
        .extra
        .get("clock")
        .ok_or_else(|| unsupported("clock state"))?;
    if clock.get("enabled") != Some(&json!(true))
        || clock.get("runningColor") != Some(&json!("white"))
        || clock.get("timeoutLoser") != Some(&Value::Null)
        || clock.get("timeoutWinner") != Some(&Value::Null)
        || ["whiteMs", "blackMs"].into_iter().any(|name| {
            clock
                .get(name)
                .and_then(Value::as_f64)
                .is_none_or(|ms| ms <= 0.0)
        })
    {
        return Err(unsupported("active clock window"));
    }
    if state
        .extra
        .get("replayEvents")
        .and_then(Value::as_array)
        .is_none_or(|events| events.len() != draft_events)
        || state.extra.get("replayEventNonce") != Some(&json!(draft_events))
        || state.extra.get("cardAcquisitionNonce") != Some(&json!(cards_per_side * 2))
        || state.extra.get("endDraftDone") != Some(&json!(style == "grand"))
        || state.extra.get("middleDraftDone") != Some(&json!(style == "grand"))
    {
        return Err(unsupported("draft provenance counters"));
    }
    let draft = state
        .extra
        .get("draft")
        .ok_or_else(|| unsupported("draft state"))?;
    if style == "grand" {
        if draft.get("kind").and_then(Value::as_str) != Some("grand")
            || draft.get("phase").and_then(Value::as_str) != Some("GRAND")
            || draft.get("color").and_then(Value::as_str) != Some("white")
            || draft.get("version") != Some(&json!(1))
            || draft.get("pickIndex") != Some(&json!(12))
            || draft
                .get("picks")
                .and_then(Value::as_array)
                .is_none_or(|picks| picks.len() != 12)
            || state.extra.get("endPhaseStartMove") != Some(&json!(0))
        {
            return Err(unsupported("grand draft completion"));
        }
    } else if draft.get("phase").and_then(Value::as_str) != Some("OPENING")
        || draft.get("color").and_then(Value::as_str) != Some("black")
        || draft.get("tutorial") != Some(&json!(false))
        || draft
            .get("choices")
            .and_then(Value::as_array)
            .is_none_or(|choices| !choices.is_empty())
        || state.extra.get("endPhaseStartMove") != Some(&Value::Null)
    {
        return Err(unsupported("opening draft completion"));
    }
    for side in [Color::White, Color::Black] {
        let deck = state.deck_slots.get(side);
        if deck.len() != slots_per_side
            || deck.iter().filter(|card| !card.vacant).count() != cards_per_side
            || deck
                .iter()
                .any(|card| !card.vacant && (card.used || card.recovering))
        {
            return Err(unsupported("selected active card count"));
        }
        for card in deck.iter().filter(|card| !card.vacant) {
            let definition = crate::card_registry::definition_for(RULES_VERSION_V7, &card.id)?;
            if definition.activation != Some(crate::card_registry::CardActType::Active)
                || definition.effect != card.effect
            {
                return Err(unsupported("selected card can alter first-play movement"));
            }
        }
    }
    let spatial = crate::SpatialState::from_v7_source(state)?;
    if spatial.geometry() != crate::BoardGeometry::new(0, 0, 8, 8)? || spatial.pieces().len() != 32
    {
        return Err(unsupported("board geometry or identity count"));
    }
    const BACK_RANK: [&str; 8] = [
        "rook", "knight", "bishop", "queen", "king", "bishop", "knight", "rook",
    ];
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let coord = crate::Coord::new(i32::from(row), i32::from(col));
            if !spatial.is_usable(coord) {
                return Err(unsupported("collapsed opening cell"));
            }
            let expected = match row {
                0 | 7 => Some(BACK_RANK[col as usize]),
                1 | 6 => Some("pawn"),
                _ => None,
            };
            let Some(kind) = expected else {
                if state.at(square).is_some() || spatial.piece_at(coord).is_some() {
                    return Err(unsupported("occupied middle rank"));
                }
                continue;
            };
            let piece = state
                .at(square)
                .ok_or_else(|| unsupported("missing initial piece"))?;
            let spatial_piece = spatial
                .piece_at(coord)
                .ok_or_else(|| unsupported("missing spatial occupancy"))?;
            let owner = if row <= 1 { Color::Black } else { Color::White };
            let origin = format!("{}{}", char::from(b'a' + col), 8 - row);
            if piece.kind != kind
                || piece.color != owner
                || piece.moved
                || piece.extra.len() != 2
                || piece.extra.get("origin") != Some(&json!(origin))
                || piece.extra.get("shielded") != Some(&json!(false))
                || spatial_piece.id != piece.id
                || spatial_piece.anchor != coord
                || spatial_piece.footprint.len() != 1
                || !spatial_piece.footprint.contains(&crate::Offset::new(0, 0))
            {
                return Err(unsupported("piece identity or source attributes"));
            }
        }
    }
    Ok(())
}

/// The frozen v7 `hasAnyLegalMove(color)` temporarily makes `color` the turn,
/// then examines every board cell whose occupant belongs to that player or is
/// a football. A neutral or opposite-colored football can therefore prevent
/// no-action loss. The callback owns the still-incomplete v7 `getLegalMoves`
/// and optional move predicate; this selector never falls back to v6 moves.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "v7 terminal waits for the complete source getLegalMoves callback"
    )
)]
pub(crate) fn has_any_legal_move_v7_with(
    state: &GameState,
    color: Color,
    mut has_allowed_move: impl FnMut(&GameState, &Piece, Square) -> Result<bool>,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 legal-move probe requires v7 rules profile".into(),
        ));
    }
    let mut probe = state.clone();
    probe.validate_v7_snapshot_shape_and_identify()?;
    probe.turn = color;
    for row in 0..8 {
        for col in 0..8 {
            let square = Square { row, col };
            let Some(piece) = probe.at(square) else {
                continue;
            };
            if piece.kind == "wall" || piece.color != color && piece.kind != "football" {
                continue;
            }
            if has_allowed_move(&probe, piece, square)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// The typed card/RULE veto portion of a v7 movement capture. The caller must
/// also apply the source target, ability, terrain and action-option policies;
/// a `true` result alone does not authorize a capture. Malformed live counters
/// are returned as errors rather than quietly producing a legal move.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "v7 public movement waits for complete source capture policy"
    )
)]
pub(crate) fn v7_movement_capture_constraints_allow(
    state: &GameState,
    attacker: &Piece,
    target: &Piece,
) -> Result<bool> {
    let constraints =
        crate::card_constraints::CaptureConstraints::from_source_state(state, attacker)?;
    Ok(!constraints.piece_veto() && !constraints.game_veto(attacker, target))
}

/// A cursor stores one piece/card family at a time. Enumeration never allocates
/// the complete Cartesian action space; a family's dedicated cursor can replace
/// its bounded batch as compound cards are ported.
#[derive(Clone)]
pub(crate) struct ActionCursor {
    pending: VecDeque<Action>,
    active_staged: Option<(usize, crate::card_effects::OrderedSelectionCursor)>,
    board_index: usize,
    card_index: usize,
    seen: BTreeSet<String>,
    forced_id: Option<String>,
    checker_capture: bool,
    finished: bool,
}
pub(crate) struct CursorWork {
    pub(crate) action: Option<Action>,
    pub(crate) staged: Option<crate::card_effects::StagedActionPage>,
}
impl CursorWork {
    fn one(action: Option<Action>) -> Self {
        Self {
            action,
            staged: None,
        }
    }
    fn staged(page: crate::card_effects::StagedActionPage) -> Self {
        Self {
            action: None,
            staged: Some(page),
        }
    }
}
impl ActionCursor {
    pub(crate) fn new(state: &GameState) -> Result<Self> {
        let special = state.result().is_some()
            || state.mode != "play"
            || state
                .extra
                .get("pendingPromotion")
                .is_some_and(|v| !v.is_null());
        if special {
            return Ok(Self {
                pending: legal_actions(state)?.into(),
                active_staged: None,
                board_index: 64,
                card_index: state.deck_slots.get(state.decision_actor()).len(),
                seen: BTreeSet::new(),
                forced_id: None,
                checker_capture: false,
                finished: true,
            });
        }
        ensure_supported(state)?;
        Ok(Self {
            pending: VecDeque::new(),
            active_staged: None,
            board_index: 0,
            card_index: 0,
            seen: BTreeSet::new(),
            forced_id: forced_piece_id(state).map(str::to_owned),
            checker_capture: has_checker_capture(state)?,
            finished: false,
        })
    }
    /// This is a structural check only. It does not probe another candidate
    /// outside the caller's examination budget to determine exhaustion.
    pub(crate) fn is_exhausted(&self, state: &GameState) -> bool {
        self.pending.is_empty()
            && self.active_staged.is_none()
            && (self.finished
                || self.board_index >= 64
                    && (self.checker_capture
                        || self.forced_id.is_some()
                        || self.card_index >= state.deck_slots.get(state.turn).len()))
    }

    /// A normal piece, pending action, or deck slot consumes one unit of work.
    /// An ordered card family may consume a bounded batch of raw UI tuples;
    /// rejected tuples still count toward the caller's budget.
    pub(crate) fn examine(&mut self, state: &GameState, budget: usize) -> Result<CursorWork> {
        if let Some(action) = self.pending.pop_front() {
            return Ok(CursorWork::one(Some(action)));
        }
        if let Some((slot, cursor)) = &mut self.active_staged {
            let page = cursor.next_public_page(state, *slot, budget, budget)?;
            if page.examined == 0 || page.examined > budget || page.actions.len() > page.examined {
                return Err(EngineError::InvalidState(
                    "staged action cursor exceeded or failed its examination budget".into(),
                ));
            }
            if page.exhausted {
                self.active_staged = None;
            }
            return Ok(CursorWork::staged(page));
        }
        if self.board_index < 64 {
            let from = Square {
                row: (self.board_index / 8) as u8,
                col: (self.board_index % 8) as u8,
            };
            self.board_index += 1;
            if let Some(piece) = state.at(from)
                && piece.color == state.turn
                && self.seen.insert(piece.id.clone())
                && mobile(piece)
                && self.forced_id.as_ref().is_none_or(|id| id == &piece.id)
            {
                self.pending.extend(
                    piece_moves(state, piece, from)?
                        .into_iter()
                        .filter(|target| !self.checker_capture || target.flag("checkerCapture"))
                        .map(|target| Action::movement(state.turn, from, target)),
                );
            }
            return Ok(CursorWork::one(self.pending.pop_front()));
        }
        if !self.checker_capture
            && self.forced_id.is_none()
            && self.card_index < state.deck_slots.get(state.turn).len()
        {
            let slot = self.card_index;
            let card = &state.deck_slots.get(state.turn)[slot];
            self.card_index += 1;
            if usable_card(card) {
                if matches!(card.effect.as_str(), "cleanupPieces" | "hypocrisy") {
                    if let Some(cursor) = crate::card_effects::staged_cursor_for_slot(state, slot)?
                    {
                        if !cursor.is_exhausted() {
                            self.active_staged = Some((slot, cursor));
                        }
                    } else {
                        self.pending
                            .extend(crate::transition::card_actions(state, card)?);
                    }
                } else {
                    // Portal Gun still needs its future rule lifecycle before
                    // its staged candidates can be exposed as legal actions.
                    self.pending
                        .extend(crate::transition::card_actions(state, card)?);
                }
            }
            return Ok(CursorWork::one(self.pending.pop_front()));
        }
        Err(EngineError::InvalidState(
            "action cursor examined after exhaustion".into(),
        ))
    }
}
fn mobile(piece: &Piece) -> bool {
    !frozen(piece)
        && piece
            .extra
            .get("staked")
            .and_then(|s| s.get("remaining"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            <= 0
}
fn usable_card(card: &CardSlot) -> bool {
    !card.vacant
        && !card.used
        && !card.recovering
        && !card
            .extra
            .get("nextTurnPending")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}
fn has_checker_capture(state: &GameState) -> Result<bool> {
    let mut seen = BTreeSet::new();
    for row in 0..8 {
        for col in 0..8 {
            let from = Square { row, col };
            let Some(piece) = state.at(from) else {
                continue;
            };
            if piece.color == state.turn
                && matches!(piece.kind.as_str(), "checker" | "checkerKing")
                && mobile(piece)
                && seen.insert(piece.id.clone())
                && piece_moves(state, piece, from)?
                    .iter()
                    .any(|target| target.flag("checkerCapture"))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Validate the selected semantic action directly, rather than enumerating all
/// other card combinations to locate it.
pub(crate) fn validate_action(state: &GameState, action: &Action) -> Result<()> {
    if state.result().is_some() {
        return Err(EngineError::Terminal);
    }
    if action.color != state.decision_actor() {
        return Err(EngineError::WrongActor);
    }
    if state.mode != "play"
        || state
            .extra
            .get("pendingPromotion")
            .is_some_and(|v| !v.is_null())
    {
        return if legal_actions(state)?.contains(action) {
            Ok(())
        } else {
            Err(EngineError::IllegalAction)
        };
    }
    ensure_supported(state)?;
    let checker_capture = has_checker_capture(state)?;
    let forced = forced_piece_id(state);
    match action.kind {
        ActionKind::Move => {
            let from = action.from.ok_or(EngineError::IllegalAction)?;
            let piece = state.at(from).ok_or(EngineError::IllegalAction)?;
            if piece.color != state.turn
                || !mobile(piece)
                || forced.is_some_and(|id| id != piece.id)
                || checker_capture
                    && !action
                        .destination
                        .as_ref()
                        .is_some_and(|m| m.flag("checkerCapture"))
            {
                return Err(EngineError::IllegalAction);
            }
            if piece_moves(state, piece, from)?
                .into_iter()
                .any(|target| Action::movement(state.turn, from, target) == *action)
            {
                Ok(())
            } else {
                Err(EngineError::IllegalAction)
            }
        }
        ActionKind::Card if !checker_capture && forced.is_none() => {
            let card = state
                .deck_slots
                .get(state.turn)
                .iter()
                .find(|card| {
                    usable_card(card)
                        && Some(&card.id) == action.card_id.as_ref()
                        && Some(&card.instance_id) == action.card_instance_id.as_ref()
                })
                .ok_or(EngineError::IllegalAction)?;
            if crate::transition::validate_card_action(state, card, action)? {
                Ok(())
            } else {
                Err(EngineError::IllegalAction)
            }
        }
        _ => Err(EngineError::IllegalAction),
    }
}

pub(crate) fn public_hints(state: &GameState, viewer: Color) -> Result<Value> {
    if state.mode != "play"
        || state.turn != viewer
        || state
            .extra
            .get("pendingPromotion")
            .is_some_and(|v| !v.is_null())
        || state
            .extra
            .get("activeTrolley")
            .is_some_and(|v| !v.is_null())
    {
        return Ok(json!({"moves":[],"cardTargets":[]}));
    }
    ensure_supported(state)?;
    let checker_capture = has_checker_capture(state)?;
    let forced = forced_piece_id(state);
    let mut hints = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let from = Square { row, col };
            let Some(piece) = state.at(from) else {
                continue;
            };
            if piece.color != viewer
                || !mobile(piece)
                || !state.piece_visible(piece, from, viewer)
                || forced.is_some_and(|id| id != piece.id)
            {
                continue;
            }
            let mut destinations = Vec::<Square>::new();
            for target in piece_moves(state, piece, from)? {
                if checker_capture && !target.flag("checkerCapture") {
                    continue;
                }
                for square in highlight_cells(&target)? {
                    if !destinations.contains(&square) {
                        destinations.push(square);
                    }
                }
            }
            if !destinations.is_empty() {
                hints.push(json!({"from":from,"destinations":destinations}));
            }
        }
    }
    let mut card_targets = Vec::new();
    if !state
        .extra
        .get("draftDelete")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        for card in state
            .deck_slots
            .get(viewer)
            .iter()
            .filter(|card| !card.vacant && !card.used && !card.recovering)
        {
            if card
                .extra
                .get("target")
                .is_none_or(|target| target.is_null())
            {
                continue;
            }
            let targets = if let Some(targets) = crate::card_effects::target_squares(state, card)? {
                targets
            } else {
                crate::transition::card_ui_actions(state, card)?
                    .into_iter()
                    .map(|a| {
                        a.target
                            .ok_or_else(|| {
                                EngineError::UnsupportedFeature(format!(
                                    "card target surface {}",
                                    card.effect
                                ))
                            })
                            .and_then(|v| {
                                serde_json::from_value::<Square>(v).map_err(|_| {
                                    EngineError::UnsupportedFeature(format!(
                                        "compound card target surface {}",
                                        card.effect
                                    ))
                                })
                            })
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            let targets = targets
                .into_iter()
                .filter(|&square| {
                    state
                        .at(square)
                        .is_none_or(|piece| state.piece_visible(piece, square, viewer))
                })
                .collect::<Vec<_>>();
            card_targets.push(json!({"cardInstanceId":card.instance_id,"targets":targets}));
        }
    }
    Ok(json!({"moves":hints,"cardTargets":card_targets}))
}
fn highlight_cells(target: &MoveTarget) -> Result<Vec<Square>> {
    if target.flags.contains_key("bodyCells") || target.flag("shotgunBlast") {
        return Ok(Vec::new());
    }
    let read = |name: &str| -> Result<Option<Vec<Square>>> {
        target
            .flags
            .get(name)
            .map(|value| serde_json::from_value(value.clone()).map_err(EngineError::serialization))
            .transpose()
    };
    if let Some(highlights) = read("highlightCells")? {
        if let Some(display) = read("displayCells")? {
            return Ok(display);
        }
        let excluded = read("excludeHighlightCells")?.unwrap_or_default();
        return Ok(highlights
            .into_iter()
            .filter(|square| !excluded.contains(square))
            .collect());
    }
    Ok(read("sectorCells")?.unwrap_or_else(|| vec![target.square()]))
}
pub(crate) fn public_move_intent(state: &GameState, action: &Action) -> Result<Value> {
    validate_action(state, action)?;
    let target = action
        .destination
        .as_ref()
        .ok_or(EngineError::IllegalAction)?;
    if !highlight_cells(target)?.contains(&target.square()) {
        return Err(EngineError::UnsupportedFeature(
            "public move intent for a non-coordinate action mode".into(),
        ));
    }
    Ok(json!({"type":"move","color":action.color,"from":action.from,"destination":target.square()}))
}
pub(crate) fn resolve_move_intent(state: &GameState, value: &Value) -> Result<Action> {
    let fields = ["type", "color", "from", "destination"];
    let object = value.as_object().ok_or(EngineError::IllegalAction)?;
    if object.len() != fields.len() || fields.iter().any(|name| !object.contains_key(*name)) {
        return Err(EngineError::IllegalAction);
    }
    for name in ["from", "destination"] {
        let square = value[name].as_object().ok_or(EngineError::IllegalAction)?;
        if square.len() != 2 || !square.contains_key("row") || !square.contains_key("col") {
            return Err(EngineError::IllegalAction);
        }
    }
    let color: Color =
        serde_json::from_value(value["color"].clone()).map_err(EngineError::serialization)?;
    if color != state.decision_actor() {
        return Err(EngineError::WrongActor);
    }
    let from: Square =
        serde_json::from_value(value["from"].clone()).map_err(EngineError::serialization)?;
    let click: Square =
        serde_json::from_value(value["destination"].clone()).map_err(EngineError::serialization)?;
    if from.row >= 8 || from.col >= 8 || click.row >= 8 || click.col >= 8 {
        return Err(EngineError::IllegalAction);
    }
    if state.mode != "play"
        || state
            .extra
            .get("pendingPromotion")
            .is_some_and(|v| !v.is_null())
    {
        return Err(EngineError::IllegalAction);
    }
    ensure_supported(state)?;
    let piece = state.at(from).ok_or(EngineError::IllegalAction)?;
    if piece.color != color || !mobile(piece) {
        return Err(EngineError::IllegalAction);
    }
    for target in piece_moves(state, piece, from)? {
        // Actual handleSquareClick selects the first legal move containing the
        // clicked public display cell. Internal capture IDs/flags stay here.
        let cells = if let Some(body) = target.flags.get("bodyCells") {
            serde_json::from_value::<Vec<Square>>(body.clone())
                .map_err(EngineError::serialization)?
        } else if let Some(display) = target.flags.get("displayCells") {
            serde_json::from_value::<Vec<Square>>(display.clone())
                .map_err(EngineError::serialization)?
        } else {
            highlight_cells(&target)?
        };
        if cells.contains(&click) {
            let action = Action::movement(color, from, target);
            match validate_action(state, &action) {
                Ok(()) => return Ok(action),
                Err(EngineError::IllegalAction) => continue,
                Err(error) => return Err(error),
            }
        }
    }
    Err(EngineError::IllegalAction)
}

fn ensure_supported(state: &GameState) -> Result<()> {
    if state.ruleset_id == RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 public legal movement requires source-equivalent candidate and restriction coverage"
                .into(),
        ));
    }
    ensure_supported_interactions(state, true)
}
fn ensure_supported_interactions(state: &GameState, require_execution_support: bool) -> Result<()> {
    // Preserving an unknown JSON field does not prove that its rules are executed.
    // Active effects outside the implemented set are explicit errors during porting.
    const PENDING: &[&str] = &[
        "activeTrolley",
        "ruleTicketChoice",
        "jokerChoice",
        "barricadeDirectionChoice",
        "earlyPromotion",
        "fastGrowth",
        "afterimageQueen",
        "initiative",
        "moving",
        "coronation",
        "majesty",
        "infiltration",
        "killerKing",
        "resolve",
        "vanguard",
        "e4",
        "solidarity",
        "bishopInfiltration",
        "assembly",
        "vigilance",
        "roller",
        "binaMate",
        "overwhelm",
        "regency",
        "racingKing",
        "radicalCharge",
        "ironMonarch",
        "imperialStudies",
        "religiousVictory",
        "backwardKnight",
        "trojanHorse",
        "madHorse",
        "clonePassive",
        "frontlineResponse",
        "relay",
        "fieldPromotion",
        "gomoku",
        "vanishing",
        "knightInjury",
        "pawnConversion",
        "fileSurge",
        "rookLift",
        "underpromotion",
        "finalWeapon",
        "highway",
        "recycling",
        "coolGuy",
        "manner",
        "switcheroo",
        "substitution",
        "chainBonds",
        "highGround",
        "platformRule",
        "portalRule",
        "crownRule",
        "conveyorRule",
        "periodicCollapse",
        "ruleBombs",
        "exhaustion",
        "camouflageRule",
        "transcendenceRule",
        "captureTheFlag",
        "idolEncorePending",
        "sirenExposure",
        "necromancy",
        "temporaryQueens",
        "timeStop",
        "skipTurn",
        "pendingIcbm",
        "pendingLobsters",
        "pendingScarecrows",
        "quantumMechanics",
        "monochromeChess",
        "symmetry",
        "locustSwarm",
        "zugzwang",
        "freeMoveCaptureLock",
        "captureLock",
        "taunt",
        "monsterRule",
    ];
    if let Some(pending) = state
        .extra
        .get("pendingPortals")
        .filter(|value| active(value))
    {
        let entries = pending.as_array().ok_or_else(|| {
            EngineError::UnsupportedFeature("non-array portal movement ledger".into())
        })?;
        if entries.len() > 4096 {
            return Err(EngineError::UnsupportedFeature(
                "portal pending ledger capacity".into(),
            ));
        }
        if entries
            .iter()
            .any(|entry| entry.get("blocksMovement") == Some(&Value::Bool(true)))
        {
            return Err(EngineError::UnsupportedFeature(
                "movement-blocking portal reservation".into(),
            ));
        }
    }
    for name in PENDING {
        if state.extra.get(*name).is_some_and(active) {
            return Err(EngineError::UnsupportedFeature(format!(
                "active state effect {name}"
            )));
        }
    }
    for piece in state.board.iter().flatten().flatten() {
        if require_execution_support && !implemented_piece_types().contains(&piece.kind.as_str()) {
            return Err(EngineError::UnsupportedFeature(format!(
                "piece {}",
                piece.kind
            )));
        }
        for flag in [
            "quantum",
            "frenzy",
            "desperado",
            "stealth",
            "poisoned",
            "hedgehog",
            "crownBearer",
            "crownRoyal",
            "metalized",
            "repositionSecondMove",
            "thiefSecondMove",
            "rookLiftSecondMove",
            "ironMonarchExtraMove",
            "madHorseSecondMove",
            "fileSurgeSecondMove",
            "promotionRush",
            "royalCommand",
        ] {
            if piece.extra.get(flag).is_some_and(active) {
                return Err(EngineError::UnsupportedFeature(format!(
                    "piece effect {flag}"
                )));
            }
        }
    }
    Ok(())
}
fn active(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => v
            .iter()
            .any(|(key, value)| !key.starts_with("__") && active(value)),
    }
}
pub(crate) fn frozen(piece: &Piece) -> bool {
    piece.kind != "scarecrow"
        && piece.extra.get("frozen").is_some_and(|v| match v {
            Value::Null => false,
            Value::Bool(v) => *v,
            Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
            Value::String(v) => !v.is_empty(),
            _ => true,
        })
}

/// Source placement and relocation differ: placement checks opponent d4 and
/// all reserved portal cells; relocation checks only movement-blocking portals.
pub(crate) fn open_placement(
    state: &GameState,
    square: Square,
    color: Option<Color>,
) -> Result<bool> {
    if square.row >= 8
        || square.col >= 8
        || state.at(square).is_some()
        || quantum_occupied(state, square)?
    {
        return Ok(false);
    }
    if let Some(color) = color {
        let enemy = color.opponent();
        if state.flag("d4", enemy)
            && square
                == (Square {
                    row: if enemy == Color::Black { 3 } else { 4 },
                    col: 3,
                })
        {
            return Ok(false);
        }
    }
    Ok(!reserved(state, square, false)?)
}
pub(crate) fn open_relocation(state: &GameState, square: Square) -> Result<bool> {
    Ok(square.row < 8
        && square.col < 8
        && state.at(square).is_none()
        && !quantum_occupied(state, square)?
        && !reserved(state, square, true)?)
}
pub(crate) fn open_alibaba_placement(
    state: &GameState,
    square: Square,
    owner: Color,
) -> Result<bool> {
    if !open_placement(state, square, Some(owner))? || collapsed(state, square) {
        return Ok(false);
    }
    if state.extra.get("crownRule").is_some_and(active) {
        return Err(EngineError::UnsupportedFeature(
            "placement on ground crowns".into(),
        ));
    }
    let holes = state.extra.get("blackHole").filter(|v| !v.is_null());
    if let Some(holes) = holes {
        let holes = holes.as_array().ok_or_else(|| {
            EngineError::UnsupportedFeature("legacy black-hole placement shape".into())
        })?;
        if holes.iter().any(|cell| {
            cell.get("row").and_then(Value::as_u64) == Some(u64::from(square.row))
                && cell.get("col").and_then(Value::as_u64) == Some(u64::from(square.col))
        }) {
            return Ok(false);
        }
    }
    Ok(true)
}
/// main68010 canReservePortalSquare first rejects every occupied board cell,
/// so the concealed-occupant branch of isSquareOpenForInstallation cannot
/// admit a portal reservation. Installation reservations and crown ground are
/// still distinct from ordinary piece placement.
pub(crate) fn open_portal_reservation(
    state: &GameState,
    square: Square,
    _owner: Color,
) -> Result<bool> {
    if square.row >= 8
        || square.col >= 8
        || state.at(square).is_some()
        || quantum_occupied(state, square)?
        || reserved(state, square, false)?
    {
        return Ok(false);
    }
    Ok(!portal_installation_hazard(state, square))
}
/// The due Portal Gun callback checks only terrain hazards. Occupancy,
/// quantum bodies and other reservations matter when selecting the cells,
/// but are not checked again when the portal is installed (main74148-63).
pub(crate) fn portal_installation_hazard(state: &GameState, square: Square) -> bool {
    if square.row >= 8 || square.col >= 8 || collapsed(state, square) {
        return true;
    }
    let crown = state.extra.get("crownRule").unwrap_or(&Value::Null);
    let entries = crown
        .get("crowns")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty());
    let crown_ground = |entry: &Value| {
        if !crate::observation::truth(Some(entry))
            || crate::observation::truth(entry.get("removed"))
        {
            return false;
        }
        if entry == &Value::Bool(true) {
            return square == (Square { row: 3, col: 3 });
        }
        let ground = entry.get("ground");
        let row = ground
            .and_then(|ground| ground.get("row"))
            .and_then(Value::as_f64);
        let col = ground
            .and_then(|ground| ground.get("col"))
            .and_then(Value::as_f64);
        row == Some(f64::from(square.row)) && col == Some(f64::from(square.col))
    };
    if entries.is_some_and(|entries| entries.iter().any(crown_ground))
        || entries.is_none() && crown_ground(crown)
    {
        return true;
    }
    state
        .extra
        .get("blackHole")
        .and_then(Value::as_array)
        .is_some_and(|holes| {
            holes.iter().any(|cell| {
                crate::observation::number(cell.get("row")) == Some(f64::from(square.row))
                    && crate::observation::number(cell.get("col")) == Some(f64::from(square.col))
            })
        })
}
fn quantum_occupied(state: &GameState, square: Square) -> Result<bool> {
    for piece in state.board.iter().flatten().flatten() {
        let Some(quantum) = piece.extra.get("quantum").filter(|q| !q.is_null()) else {
            continue;
        };
        let anchor: Square =
            serde_json::from_value(quantum.clone()).map_err(EngineError::serialization)?;
        let size = if piece.is_large() { 2 } else { 1 };
        if square.row >= anchor.row
            && square.col >= anchor.col
            && square.row - anchor.row < size
            && square.col - anchor.col < size
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn reserved(state: &GameState, square: Square, relocation: bool) -> Result<bool> {
    let same_square = |value: &Value| {
        value.get("row").and_then(Value::as_u64) == Some(u64::from(square.row))
            && value.get("col").and_then(Value::as_u64) == Some(u64::from(square.col))
    };
    for name in ["pendingScarecrows", "pendingLobsters", "pendingPortals"] {
        let Some(entries) = state.extra.get(name).filter(|v| !v.is_null()) else {
            continue;
        };
        let Some(entries) = entries.as_array() else {
            if name == "pendingPortals" {
                // main67957 treats a non-array portal ledger as no active
                // reservations; portalGun later replaces it with an array.
                continue;
            }
            return Err(EngineError::InvalidState(format!(
                "{name} must be an array"
            )));
        };
        for entry in entries {
            let matches = match name {
                "pendingScarecrows" => {
                    entry
                        .get("pieceId")
                        .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
                        && same_square(entry)
                }
                "pendingLobsters" => same_square(entry),
                _ => {
                    (!relocation || entry.get("blocksMovement") == Some(&json!(true)))
                        && entry
                            .get("cells")
                            .and_then(Value::as_array)
                            .is_some_and(|cells| cells.iter().any(&same_square))
                }
            };
            if matches {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn forced_piece_id(state: &GameState) -> Option<&str> {
    state
        .board
        .iter()
        .flatten()
        .flatten()
        .find(|piece| piece.color == state.turn && piece.flag("checkerChainCapture"))
        .map(|piece| piece.id.as_str())
}

fn desperado_royal_capture_blocked(attacker: &Piece, target: &Piece) -> bool {
    crate::observation::truth(attacker.extra.get("desperado"))
        && (crate::observation::truth(target.extra.get("regencyHeir"))
            || crate::observation::truth(target.extra.get("crownRoyal"))
            || matches!(
                target.kind.as_str(),
                "king" | "royalKnight" | "shotgunKing" | "darkWizard"
            ))
}

/// The card temporarily sets desperado before calling source getLegalMoves.
/// Its movement stays unchanged. This owned probe applies the ordinary kernel
/// and source forced-piece/royal-capture guards without changing the caller.
pub(crate) fn desperado_has_legal_move(
    state: &GameState,
    piece: &Piece,
    square: Square,
) -> Result<bool> {
    let Some(actor) = piece.color.owner() else {
        return Ok(false);
    };
    let socialism = crate::observation::number(
        state
            .extra
            .get("socialism")
            .and_then(|value| value.get(actor.as_str())),
    )
    .unwrap_or(0.0)
        > 0.0;
    if frozen(piece)
        || crate::observation::number(piece.extra.get("staked").and_then(|v| v.get("remaining")))
            .unwrap_or(0.0)
            > 0.0
        || crate::observation::number(piece.extra.get("poisonStunTurns"))
            .unwrap_or(0.0)
            .floor()
            > 0.0
        || (matches!(piece.kind.as_str(), "hedgehog" | "bear")
            || piece.ability_kind() == "hedgehog")
            && crate::observation::number(piece.extra.get("bearMoveLockedUntilTurn")).unwrap_or(0.0)
                > f64::from(*state.turns_taken.get(actor))
        || piece.kind == "babyBear" && !socialism
        || piece.kind == "medium"
            && state
                .extra
                .get("mediumMovement")
                .and_then(|v| v.get("type"))
                .is_none_or(|v| !crate::observation::truth(Some(v)))
            && !socialism
    {
        return Ok(false);
    }
    let mut trial = state.clone();
    let mut candidate = piece.clone();
    candidate
        .extra
        .insert("desperado".into(), serde_json::json!({"remaining":2}));
    if state.at(square).is_none_or(|at| at.id != piece.id) {
        return Err(EngineError::IllegalAction);
    }
    for cell in trial.board.iter_mut().flatten().flatten() {
        if cell.id == piece.id {
            *cell = candidate.clone();
        }
    }
    let reposition = trial.board.iter().flatten().flatten().find(|at| {
        at.color == actor && crate::observation::truth(at.extra.get("repositionSecondMove"))
    });
    let forced = reposition.or_else(|| {
        trial.board.iter().flatten().flatten().find(|at| {
            at.color == actor
                && [
                    "thiefSecondMove",
                    "frenzyExtraMove",
                    "fileSurgeSecondMove",
                    "rookLiftSecondMove",
                    "ironMonarchExtraMove",
                    "underpromotionSecondMove",
                    "checkerChainCapture",
                    "madHorseSecondMove",
                    "platformExtraMove",
                    "desperado",
                ]
                .iter()
                .any(|key| crate::observation::truth(at.extra.get(*key)))
        })
    });
    if forced.is_some_and(|forced| forced.id != candidate.id) {
        return Ok(false);
    }
    // The temporary marker is the only newly implemented trial effect. All
    // other global interactions still pass the common support guard.
    let mut checked = trial.clone();
    for cell in checked.board.iter_mut().flatten().flatten() {
        cell.extra.shift_remove("desperado");
    }
    // Basic movement can be proven before that piece's execution/capture
    // reactions are complete. The ordinary support registry is unchanged.
    ensure_supported_interactions(&checked, false)?;
    let moves = piece_moves(&trial, &candidate, square)?;
    for target in moves {
        if target.flag("setLogDirection") {
            return Ok(true);
        }
        let mut cells = vec![target.square()];
        if target.flag("enPassant")
            && let (Some(row), Some(col)) = (
                target.flags.get("capturedRow").and_then(Value::as_u64),
                target.flags.get("capturedCol").and_then(Value::as_u64),
            )
            && row < 8
            && col < 8
        {
            cells.push(Square {
                row: row as u8,
                col: col as u8,
            });
        }
        for key in ["jumpCapture"] {
            if let Some(value) = target.flags.get(key) {
                cells.push(
                    serde_json::from_value(value.clone()).map_err(EngineError::serialization)?,
                );
            }
        }
        for key in [
            "sectorCells",
            "colossusLandingCaptures",
            "bigRookLandingCaptures",
        ] {
            if let Some(value) = target.flags.get(key) {
                cells.extend(
                    serde_json::from_value::<Vec<Square>>(value.clone())
                        .map_err(EngineError::serialization)?,
                );
            }
        }
        if (target.flag("shotgunBlast")
            || target.flag("colossusAttack")
            || target.flag("siegeRamMove"))
            && let Some(value) = target.flags.get("highlightCells")
        {
            cells.extend(
                serde_json::from_value::<Vec<Square>>(value.clone())
                    .map_err(EngineError::serialization)?,
            );
        }
        if !cells.into_iter().any(|cell| {
            trial.at(cell).is_some_and(|victim| {
                victim.color != candidate.color
                    && desperado_royal_capture_blocked(&candidate, victim)
            })
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn can_capture(state: &GameState, attacker: &Piece, target: &Piece) -> bool {
    if desperado_royal_capture_blocked(attacker, target) {
        return false;
    }
    let ability = attacker.ability_kind();
    let Some(actor) = attacker.color.owner() else {
        return false;
    };
    if crate::observation::number(
        attacker
            .extra
            .get("disarmed")
            .and_then(|entry| entry.get("remaining")),
    )
    .unwrap_or(0.0)
        > 0.0
        || attacker.kind != "monster"
            && crate::observation::truth(attacker.extra.get("potionManner"))
            && crate::observation::truth(attacker.extra.get("coolGuyCapturedLast"))
    {
        return false;
    }
    for lock in ["freshNoCaptureUntil", "cardNoCaptureUntil"] {
        if attacker.number(lock) > i64::from(*state.turns_taken.get(actor)) {
            return false;
        }
    }
    if target.color == attacker.color
        || matches!(
            target.kind.as_str(),
            "wall" | "football" | "blackHole" | "guard"
        )
        || target.flag("protected")
        || frozen(target)
        || target.flag("submerged")
        || target.ability_kind() == "guard"
        || target.ability_kind() == "revolvingDoor"
        || encouraged(state, target)
    {
        return false;
    }
    // main1193: physical royal augmentation identity is used here, including
    // inactive regency/editor side flags. Copied movement is not that identity.
    fn nullification_identity(piece: &Piece) -> &str {
        if ["regencyHeir", "crownRoyal", "editorRoyal"]
            .iter()
            .any(|field| crate::observation::truth(piece.extra.get(*field)))
            || matches!(
                piece.kind.as_str(),
                "king" | "royalKnight" | "shotgunKing" | "darkWizard" | "merchant"
            )
        {
            "king"
        } else {
            piece.kind.as_str()
        }
    }
    if target.kind != "scarecrow"
        && crate::observation::truth(target.extra.get("nullification"))
        && nullification_identity(target) == nullification_identity(attacker)
    {
        return false;
    }
    let socialist = state.flag("socialism", actor)
        && !state.royal_identity(attacker)
        && attacker.kind != "crown"
        && ability != "slime";
    if matches!(
        ability,
        "campfire" | "paladin" | "revolvingDoor" | "recruiter" | "guard"
    ) && !socialist
    {
        return false;
    }
    if ability == "idol" && !attacker.flag("crownBearer") && !socialist {
        return false;
    }
    if (crate::observation::truth(state.extra.get("saturationRule"))
        || crate::observation::truth(attacker.extra.get("potionSaturation")))
        && crate::observation::number(attacker.extra.get("capturesMade")).unwrap_or(0.0) >= 3.0
    {
        return false;
    }
    if state.flag("genevaConvention", target.color)
        && attacker.kind == "queen"
        && target.kind == "pawn"
    {
        return false;
    }
    if target
        .extra
        .get("captureRestriction")
        .and_then(Value::as_str)
        == Some("immune")
    {
        return false;
    }
    if (target.ability_kind() == "jester"
        || target
            .extra
            .get("captureRestriction")
            .and_then(Value::as_str)
            == Some("royal-only"))
        && (ability == "jester"
            || !(attacker.flag("crownBearer") || state.royal_identity(attacker)))
    {
        return false;
    }
    true
}
pub(crate) fn encouraged(state: &GameState, target: &Piece) -> bool {
    if target.kind == "scarecrow" {
        return false;
    }
    if target.flag("outpostProtected") {
        return true;
    }
    let Some(cell) = find_square(state, &target.id) else {
        return false;
    };
    if !state.royal_identity(target)
        && ORTHO.iter().any(|delta| {
            cell.offset(delta.0, delta.1)
                .and_then(|square| state.at(square))
                .is_some_and(|piece| piece.kind == "campfire" && piece.color == target.color)
        })
    {
        return true;
    }
    if !state.flag("encouragement", target.color) {
        return false;
    }
    for row in 0..8 {
        for col in 0..8 {
            let king = Square { row, col };
            if state.at(king).is_some_and(|piece| {
                piece.color == target.color && (piece.is_royal() || piece.flag("regencyHeir"))
            }) {
                return row.abs_diff(cell.row) + col.abs_diff(cell.col) == 1;
            }
        }
    }
    false
}
fn find_square(state: &GameState, id: &str) -> Option<Square> {
    for row in 0..8 {
        for col in 0..8 {
            if state.at(Square { row, col }).is_some_and(|p| p.id == id) {
                return Some(Square { row, col });
            }
        }
    }
    None
}
pub(crate) fn collapsed(state: &GameState, square: Square) -> bool {
    if state
        .extra
        .get("collapsed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let depth = state
            .extra
            .get("collapseDepth")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        if u64::from(square.row) < depth
            || u64::from(square.col) < depth
            || u64::from(7 - square.row) < depth
            || u64::from(7 - square.col) < depth
        {
            return true;
        }
    }
    state
        .extra
        .get("collapsedCells")
        .and_then(Value::as_array)
        .is_some_and(|cells| {
            cells.iter().any(|cell| {
                cell.get("row").and_then(Value::as_u64) == Some(u64::from(square.row))
                    && cell.get("col").and_then(Value::as_u64) == Some(u64::from(square.col))
            })
        })
}
pub(crate) fn landing(state: &GameState, piece: &Piece, square: Square) -> bool {
    !collapsed(state, square)
        && state
            .at(square)
            .is_none_or(|target| can_capture(state, piece, target))
}
pub(crate) fn leaps(
    state: &GameState,
    piece: &Piece,
    from: Square,
    deltas: &[(i8, i8)],
) -> Vec<MoveTarget> {
    deltas
        .iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .filter(|&to| landing(state, piece, to))
        .map(MoveTarget::at)
        .collect()
}
pub(crate) fn rays(
    state: &GameState,
    piece: &Piece,
    from: Square,
    directions: &[(i8, i8)],
    limit: u8,
) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(dr, dc) in directions {
        let mut cursor = from;
        for _ in 0..limit {
            let Some(to) = cursor.offset(dr, dc) else {
                break;
            };
            cursor = to;
            if collapsed(state, to) {
                break;
            }
            match state.at(to) {
                None => moves.push(MoveTarget::at(to)),
                Some(target) => {
                    if target.color == piece.color
                        && target.flag("ghost")
                        && piece.kind != "cannon"
                        && crate::card_effects::ranged_piece(state, piece)
                    {
                        continue;
                    }
                    if can_capture(state, piece, target) {
                        moves.push(MoveTarget::at(to));
                    }
                    break;
                }
            }
        }
    }
    moves
}

pub(crate) fn piece_moves(
    state: &GameState,
    piece: &Piece,
    from: Square,
) -> Result<Vec<MoveTarget>> {
    if frozen(piece)
        || crate::observation::number(
            piece
                .extra
                .get("staked")
                .and_then(|entry| entry.get("remaining")),
        )
        .unwrap_or(0.0)
            > 0.0
        || crate::observation::number(piece.extra.get("poisonStunTurns"))
            .unwrap_or(0.0)
            .floor()
            > 0.0
    {
        return Ok(Vec::new());
    }
    let reversed = state.flag("reversal", piece.color);
    let mut moves = match piece.kind.as_str() {
        "pawn" | "squire" | "standardBearer" => pawn_moves(state, piece, from),
        "rook" => rays(state, piece, from, if reversed { DIAG } else { ORTHO }, 7),
        "bishop" => rays(state, piece, from, if reversed { ORTHO } else { DIAG }, 7),
        "queen" => {
            // Source queenDirections emits every diagonal ray before the
            // orthogonal rays. KING is row-major and is only the king order.
            let mut moves = rays(state, piece, from, DIAG, 7);
            moves.extend(rays(state, piece, from, ORTHO, 7));
            moves
        }
        "king" => {
            let mut moves = leaps(state, piece, from, KING);
            moves.extend(castling(state, piece, from));
            if state.flag("kingKnight", piece.color) {
                moves.extend(leaps(state, piece, from, KNIGHT));
            }
            if state.flag("hillKing", piece.color)
                && (3..=4).contains(&from.row)
                && (3..=4).contains(&from.col)
            {
                moves.extend(rays(state, piece, from, KING, 7));
            }
            moves
        }
        "knight" | "royalKnight" | "unicorn" => {
            let mut moves = leaps(state, piece, from, KNIGHT);
            if state.flag("cornerKick", piece.color)
                && (from.row == 0 || from.row == 7)
                && (from.col == 0 || from.col == 7)
            {
                moves.extend(rays(state, piece, from, DIAG, 7));
            }
            if piece.kind == "royalKnight" && state.flag("royalKnightKing", piece.color) {
                moves.extend(leaps(state, piece, from, KING));
            }
            moves
        }
        "amazon" => {
            let mut moves = rays(state, piece, from, KING, 7);
            moves.extend(leaps(state, piece, from, KNIGHT));
            moves
        }
        "man" | "guard" => leaps(state, piece, from, KING),
        "camel" => leaps(state, piece, from, CAMEL),
        "eagle" | "alibaba" => leaps(state, piece, from, EAGLE),
        "alfil" => leaps(state, piece, from, &[(-2, -2), (-2, 2), (2, -2), (2, 2)]),
        "ferz" | "knightmaster" => leaps(state, piece, from, DIAG),
        "princess" => {
            if state
                .board
                .iter()
                .flatten()
                .flatten()
                .any(|p| p.color == piece.color && p.kind == "queen")
            {
                leaps(state, piece, from, DIAG)
            } else {
                rays(state, piece, from, KING, 7)
            }
        }
        "clockwork" => {
            if KING
                .iter()
                .filter_map(|&(dr, dc)| from.offset(dr, dc))
                .any(|s| state.at(s).is_some_and(|p| p.color == piece.color))
            {
                rays(state, piece, from, KING, 7)
            } else {
                Vec::new()
            }
        }
        "campfire" => leaps(state, piece, from, ORTHO)
            .into_iter()
            .filter(|target| state.at(target.square()).is_none())
            .collect(),
        "cannon" => cannon(state, piece, from),
        "grasshopper" => grasshopper(state, piece, from),
        "checker" | "checkerKing" => checker(state, piece, from),
        "missionary" => missionary(state, piece, from),
        "bigRook" | "bigBishop" => large_rays(state, piece, from),
        "wall" | "coffin" | "scarecrow" => Vec::new(),
        other => crate::variant_movement::base_moves(state, piece, from)?
            .ok_or_else(|| EngineError::UnsupportedFeature(format!("movement {other}")))?,
    };
    if state.flag("socialism", piece.color)
        && !piece.is_royal()
        && !matches!(
            piece.kind.as_str(),
            "wall" | "coffin" | "scarecrow" | "bigRook" | "bigBishop"
        )
    {
        moves = pawn_moves(state, piece, from)
            .into_iter()
            .filter(|target| !target.flag("standardPawnDoubleStep"))
            .collect();
    }
    if piece.flag("basicTraining")
        && !piece.is_large()
        && !matches!(piece.kind.as_str(), "wall" | "football" | "blackHole")
        && piece.ability_kind() != "slime"
        && (piece.flag("potionBasicTraining")
            || !matches!(
                piece.kind.as_str(),
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
            ))
        && let Some(actor) = piece.color.owner()
    {
        for direction in [actor.pawn_dir(), -actor.pawn_dir()] {
            if direction != actor.pawn_dir() && !state.flag("retreat", actor) {
                continue;
            }
            if let Some(to) = from.offset(direction, 0)
                && state.at(to).is_none()
            {
                moves.push(MoveTarget::at(to));
            }
            if piece.ability_kind() != "missionary" {
                for dc in [-1, 1] {
                    if let Some(to) = from.offset(direction, dc)
                        && state
                            .at(to)
                            .is_some_and(|victim| can_capture(state, piece, victim))
                    {
                        let mut target = MoveTarget::at(to);
                        target
                            .flags
                            .insert("basicTrainingCapture".into(), json!(true));
                        moves.push(target);
                    }
                }
            }
        }
    }
    if piece.flag("loyalist") && piece.ability_kind() != "slime" {
        for row in 0..8 {
            for col in 0..8 {
                let anchor = Square { row, col };
                if state.at(anchor).is_some_and(|candidate| {
                    candidate.color == piece.color
                        && (state.royal_identity(candidate) || candidate.kind == "merchant")
                }) {
                    for &(dr, dc) in KING {
                        if let Some(to) = anchor.offset(dr, dc)
                            && open_relocation(state, to)?
                        {
                            let mut target = MoveTarget::at(to);
                            target.flags.insert("loyalistMove".into(), json!(true));
                            moves.push(target);
                        }
                    }
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut allowed = Vec::new();
    for target in moves {
        let stationary = [
            "colossusBody",
            "colossusAttack",
            "shotgunBlast",
            "shotgunSnipe",
            "merchantBuy",
            "setLogDirection",
        ]
        .iter()
        .any(|field| target.flag(field));
        if crate::observation::truth(piece.extra.get("grapplerBound"))
            && !stationary
            && target.square() != from
        {
            continue;
        }
        let severed = crate::observation::number(
            piece
                .extra
                .get("severed")
                .and_then(|entry| entry.get("remaining")),
        )
        .unwrap_or(0.0)
            > 0.0;
        let exempt = target.flag("castle") || target.flag("colossusMove") || stationary;
        if severed && !exempt {
            let memory = crate::card_effects::current_base_movement(state, piece);
            let kind = memory
                .as_ref()
                .and_then(|memory| memory.get("type"))
                .and_then(Value::as_str)
                .unwrap_or(piece.kind.as_str());
            let dr = from.row.abs_diff(target.row);
            let dc = from.col.abs_diff(target.col);
            let distance = if matches!(kind, "hook" | "brutus") {
                dr + dc
            } else {
                dr.max(dc)
            };
            if distance > 1 {
                continue;
            }
        }
        if piece.flag("inertia")
            && !exempt
            && !(matches!(piece.kind.as_str(), "hook" | "brutus")
                && (target.flag("bent") || target.flag("portalThrough")))
            && from
                .row
                .abs_diff(target.row)
                .max(from.col.abs_diff(target.col))
                == 1
        {
            continue;
        }
        if expansion_move_allowed(state, piece, from, &target)?
            && fianchetto_move_allowed(state, piece, from, &target)?
            && seen.insert(serde_json::to_string(&target).expect("move serializes"))
        {
            allowed.push(target);
        }
    }
    let moves = allowed;
    Ok(moves)
}

fn diagonal_key(square: Square) -> Option<bool> {
    if square.row == square.col {
        Some(false)
    } else if square.row + square.col == 7 {
        Some(true)
    } else {
        None
    }
}

/// Current catalog restricts pawns entering a new main diagonal guarded by an
/// opposing physical bishop. Existing diagonal occupancy, swaps and twins use
/// the source origin of each moving piece separately.
pub(crate) fn fianchetto_destination_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    destinations: &[Square],
) -> bool {
    let Some(owner) = piece.color.owner() else {
        return true;
    };
    if piece.kind != "pawn" || !state.flag("fianchetto", owner.opponent()) {
        return true;
    }
    let origin = diagonal_key(from);
    !destinations.iter().any(|destination| {
        let Some(key) = diagonal_key(*destination).filter(|key| Some(*key) != origin) else {
            return false;
        };
        (0..8).any(|row| {
            (0..8).any(|col| {
                let square = Square { row, col };
                diagonal_key(square) == Some(key)
                    && state.at(square).is_some_and(|bishop| {
                        bishop.color == owner.opponent() && bishop.kind == "bishop"
                    })
            })
        })
    })
}

fn fianchetto_move_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
) -> Result<bool> {
    if [
        "colossusAttack",
        "shotgunBlast",
        "shotgunSnipe",
        "setLogDirection",
    ]
    .into_iter()
    .any(|flag| target.flag(flag))
    {
        return Ok(true);
    }
    let destination = if target.flag("portalLanding") || target.flag("portalThrough") {
        target
            .flags
            .get("portalExit")
            .map(|value| serde_json::from_value(value.clone()).map_err(EngineError::serialization))
            .transpose()?
            .unwrap_or(target.square())
    } else {
        target.square()
    };
    let cells = if target.flag("colossusMove") || target.flag("bigRookMove") {
        target
            .flags
            .get("highlightCells")
            .map(|value| {
                serde_json::from_value::<Vec<Square>>(value.clone())
                    .map_err(EngineError::serialization)
            })
            .transpose()?
            .unwrap_or_else(|| vec![destination])
    } else {
        vec![destination]
    };
    if !fianchetto_destination_allowed(state, piece, from, &cells) {
        return Ok(false);
    }
    let swapped = ["dragonSwap", "substitutionSwap", "relaySwap"]
        .into_iter()
        .any(|flag| target.flag(flag))
        .then(|| state.at(target.square()))
        .flatten();
    if let Some(swapped) = swapped
        && !fianchetto_destination_allowed(state, swapped, target.square(), &[from])
    {
        return Ok(false);
    }
    if crate::observation::truth(piece.extra.get("twinBondId"))
        && let Some(id) = piece.extra.get("twinPartnerId").and_then(Value::as_str)
    {
        for row in 0..8 {
            for col in 0..8 {
                let square = Square { row, col };
                if let Some(partner) = state
                    .at(square)
                    .filter(|partner| partner.id == id && partner.color == piece.color)
                    && swapped.is_none_or(|swapped| swapped.id != partner.id)
                    && !fianchetto_destination_allowed(state, partner, square, &cells)
                {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

pub(crate) fn expansion_destination_allowed(
    state: &GameState,
    color: PieceColor,
    cells: &[Square],
) -> bool {
    if !d4_destination_allowed(state, color, cells) {
        return false;
    }
    let Some(owner) = color.owner() else {
        return true;
    };
    let enemy = owner.opponent();
    if state.flag("synchronization", enemy) {
        let mut parity = None;
        let mut uniform = true;
        for row in 0..8 {
            for col in 0..8 {
                if state
                    .at(Square { row, col })
                    .is_some_and(|piece| piece.color == enemy)
                {
                    let next = (row + col) % 2;
                    if parity.is_some_and(|old| old != next) {
                        uniform = false;
                        break;
                    }
                    parity = Some(next);
                }
            }
        }
        if uniform
            && parity.is_some_and(|required| {
                cells
                    .iter()
                    .any(|cell| (cell.row + cell.col) % 2 != required)
            })
        {
            return false;
        }
    }
    true
}

/// Some source installation callers pass d4 but deliberately omit the
/// synchronization argument. Keep that policy distinct from move placement.
pub(crate) fn d4_destination_allowed(
    state: &GameState,
    color: PieceColor,
    cells: &[Square],
) -> bool {
    if cells.is_empty() || cells.iter().any(|cell| cell.row >= 8 || cell.col >= 8) {
        return false;
    }
    color.owner().is_none_or(|owner| {
        let enemy = owner.opponent();
        !state.flag("d4", enemy)
            || !cells.contains(&Square {
                row: if enemy == Color::Black { 3 } else { 4 },
                col: 3,
            })
    })
}
fn expansion_move_allowed(
    state: &GameState,
    piece: &Piece,
    from: Square,
    target: &MoveTarget,
) -> Result<bool> {
    if ![Color::White, Color::Black]
        .into_iter()
        .any(|side| state.flag("d4", side) || state.flag("synchronization", side))
    {
        return Ok(true);
    }
    if [
        "grapplePull",
        "setLogDirection",
        "colossusBody",
        "colossusAttack",
        "shotgunBlast",
        "shotgunSnipe",
        "merchantBuy",
    ]
    .into_iter()
    .any(|flag| target.flag(flag))
    {
        return Ok(true);
    }
    let read = |name: &str| {
        target
            .flags
            .get(name)
            .map(|v| {
                serde_json::from_value::<Square>(v.clone()).map_err(EngineError::serialization)
            })
            .transpose()
    };
    let destination = if target.flag("portalLanding") {
        read("portalExit")?.unwrap_or(target.square())
    } else {
        target.square()
    };
    let cells = target
        .flags
        .get("highlightCells")
        .map(|v| {
            serde_json::from_value::<Vec<Square>>(v.clone()).map_err(EngineError::serialization)
        })
        .transpose()?
        .filter(|cells| !cells.is_empty())
        .unwrap_or_else(|| vec![destination]);
    if !expansion_destination_allowed(state, piece.color, &cells) {
        return Ok(false);
    }
    for name in ["portalEntry", "portalExit"] {
        if let Some(cell) = read(name)?
            && !expansion_destination_allowed(state, piece.color, &[cell])
        {
            return Ok(false);
        }
    }
    if target
        .flags
        .get("castle")
        .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
        && let (Some(rook_from), Some(rook_to)) = (read("rookFrom")?, read("rookTo")?)
        && let Some(rook) = state.at(rook_from)
    {
        let landing = target
            .flags
            .get("rookHighlightCells")
            .map(|v| {
                serde_json::from_value::<Vec<Square>>(v.clone()).map_err(EngineError::serialization)
            })
            .transpose()?
            .unwrap_or_else(|| vec![rook_to]);
        if !expansion_destination_allowed(state, rook.color, &landing) {
            return Ok(false);
        }
    }
    if [
        "relaySwap",
        "substitutionSwap",
        "dragonSwap",
        "solidaritySwap",
    ]
    .into_iter()
    .any(|flag| target.flag(flag))
        && let Some(other) = state.at(destination)
        && !expansion_destination_allowed(state, other.color, &[from])
    {
        return Ok(false);
    }
    Ok(true)
}

pub(crate) fn pawn_moves(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let Some(actor) = piece.color.owner() else {
        return Vec::new();
    };
    let dir = if state.flag("reversePawns", piece.color) {
        -actor.pawn_dir()
    } else {
        actor.pawn_dir()
    };
    if piece.kind == "pawn" {
        let neighboring_master =
            KING.iter()
                .filter_map(|&(dr, dc)| from.offset(dr, dc))
                .any(|square| {
                    state
                        .at(square)
                        .is_some_and(|p| p.color == piece.color && p.kind == "knightmaster")
                });
        if neighboring_master {
            return leaps(state, piece, from, KNIGHT);
        }
    }
    let mut moves = Vec::new();
    for forward in [dir, -dir] {
        if forward != dir && !state.flag("retreat", piece.color) {
            continue;
        }
        if let Some(one) = from.offset(forward, 0)
            && state.at(one).is_none()
            && !collapsed(state, one)
        {
            moves.push(MoveTarget::at(one));
            let start = if piece.color == Color::White { 6 } else { 1 };
            if forward == dir
                && !piece.moved
                && (from.row == start || from.row == actor.home_row())
                && let Some(two) = one.offset(forward, 0)
                && state.at(two).is_none()
                && !collapsed(state, two)
            {
                let mut target = MoveTarget::at(two);
                target
                    .flags
                    .insert("standardPawnDoubleStep".into(), json!(true));
                moves.push(target);
                if piece.kind == "pawn"
                    && state.flag("pawnSprint", piece.color)
                    && let Some(three) = two.offset(forward, 0)
                    && state.at(three).is_none()
                    && !collapsed(state, three)
                {
                    let mut target = MoveTarget::at(three);
                    target
                        .flags
                        .insert("pawnSprintTripleStep".into(), json!(true));
                    moves.push(target);
                }
            }
        } else if piece.kind == "pawn"
            && state.flag("pawnLeap", piece.color)
            && forward == actor.pawn_dir()
            && let Some(one) = from.offset(forward, 0)
            && state
                .at(one)
                .is_some_and(|piece| piece.color == actor.opponent())
            && let Some(two) = one.offset(forward, 0)
            && state.at(two).is_none()
            && !collapsed(state, two)
        {
            let mut target = MoveTarget::at(two);
            target.flags.insert("pawnLeap".into(), json!(true));
            moves.push(target);
        }
        for dc in [-1, 1] {
            if let Some(to) = from.offset(forward, dc)
                && state
                    .at(to)
                    .is_some_and(|target| can_capture(state, piece, target))
                && !collapsed(state, to)
            {
                moves.push(MoveTarget::at(to));
            }
        }
    }
    if let Some(right) = &state.en_passant
        && right.color != piece.color
        && right.row as i16 == from.row as i16 + i16::from(dir)
        && (right.col as i16 - from.col as i16).abs() == 1
    {
        let captured = Square {
            row: right.captured_row,
            col: right.captured_col,
        };
        let to = Square {
            row: right.row,
            col: right.col,
        };
        if state
            .at(captured)
            .is_some_and(|p| p.kind == "pawn" && can_capture(state, piece, p))
            && landing(state, piece, to)
        {
            moves.retain(|target| target.square() != to);
            let mut target = MoveTarget::at(to);
            target.flags.insert("enPassant".into(), json!(true));
            target
                .flags
                .insert("capturedRow".into(), json!(captured.row));
            target
                .flags
                .insert("capturedCol".into(), json!(captured.col));
            moves.push(target);
        }
    }
    let bearer = piece.ability_kind() == "standardBearer"
        || piece.kind == "pawn"
            && state.board[from.row as usize]
                .iter()
                .flatten()
                .any(|target| {
                    target.color == piece.color && target.ability_kind() == "standardBearer"
                });
    if bearer {
        let capture_ready = state.board[from.row as usize]
            .iter()
            .flatten()
            .any(|target| {
                target.color == piece.color
                    && target.ability_kind() == "standardBearer"
                    && target.number("freshNoCaptureUntil")
                        <= i64::from(*state.turns_taken.get(actor))
            });
        for dc in [-1, 1] {
            if let Some(cell) = from.offset(0, dc)
                && state
                    .at(cell)
                    .is_none_or(|target| capture_ready && can_capture(state, piece, target))
                && !collapsed(state, cell)
            {
                moves.push(MoveTarget::at(cell));
            }
        }
    }
    // main96225: the source replaces a diagonal landing with the prioritized
    // side capture. The destination can itself contain a second victim.
    if piece.kind == "pawn" && state.flag("enPassantFrenzy", actor) {
        for dc in [-1, 1] {
            let Some(to) = from.offset(dir, dc) else {
                continue;
            };
            let Some(side) = from.offset(0, dc) else {
                continue;
            };
            let ordinary_right = state.en_passant.as_ref().is_some_and(|right| {
                right.color != actor
                    && right.row == to.row
                    && right.col == to.col
                    && state
                        .at(Square {
                            row: right.captured_row,
                            col: right.captured_col,
                        })
                        .is_some_and(|victim| {
                            victim.kind == "pawn" && victim.color == actor.opponent()
                        })
            });
            if ordinary_right
                || state
                    .at(side)
                    .is_none_or(|victim| victim.color != actor.opponent())
            {
                continue;
            }
            moves.retain(|target| target.square() != to);
            if state
                .at(side)
                .is_some_and(|victim| can_capture(state, piece, victim))
                && state
                    .at(to)
                    .is_none_or(|victim| can_capture(state, piece, victim))
            {
                let mut target = MoveTarget::at(to);
                target.flags.insert("enPassant".into(), json!(true));
                target.flags.insert("capturedRow".into(), json!(side.row));
                target.flags.insert("capturedCol".into(), json!(side.col));
                target.flags.insert("enPassantFrenzy".into(), json!(true));
                moves.push(target);
            }
        }
    }
    moves
}

pub(crate) fn missionary(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    DIAG.iter()
        .filter_map(|&(dr, dc)| from.offset(dr, dc))
        .filter(|&to| {
            !collapsed(state, to)
                && state.at(to).is_none_or(|target| {
                    target.color != piece.color
                        && target.color.owner().is_some()
                        && !matches!(
                            target.kind.as_str(),
                            "wall" | "football" | "monster" | "blackHole"
                        )
                        && !desperado_royal_capture_blocked(piece, target)
                })
        })
        .map(|to| {
            let mut target = MoveTarget::at(to);
            if state.at(to).is_some() {
                target.flags.insert("missionaryConvert".into(), json!(true));
            }
            target
        })
        .collect()
}

fn castling(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let Some(actor) = piece.color.owner() else {
        return Vec::new();
    };
    if piece.kind != "king"
        || piece.moved
        || from.row != actor.home_row()
        || state.flag("castlingCanceled", piece.color)
    {
        return Vec::new();
    }
    let mut moves = Vec::new();
    for rook_col in [0, 7] {
        let rook_square = Square {
            row: from.row,
            col: rook_col,
        };
        let Some(rook) = state.at(rook_square) else {
            continue;
        };
        if rook.color != piece.color || rook.kind != "rook" || rook.moved {
            continue;
        }
        let dc = if rook_col < from.col { -1 } else { 1 };
        let Some(to) = from.offset(0, 2 * dc) else {
            continue;
        };
        let Some(rook_to) = from.offset(0, dc) else {
            continue;
        };
        let low = rook_col.min(from.col) + 1;
        let high = rook_col.max(from.col);
        if (low..high).any(|col| {
            state.at(Square { row: from.row, col }).is_some()
                || collapsed(state, Square { row: from.row, col })
        }) {
            continue;
        }
        let mut target = MoveTarget::at(to);
        target.flags.insert(
            "castle".into(),
            json!(if dc > 0 {
                "킹사이드"
            } else {
                "퀸사이드"
            }),
        );
        target.flags.insert("rookFrom".into(), json!(rook_square));
        target.flags.insert("rookTo".into(), json!(rook_to));
        target.flags.insert("bigRookCastle".into(), json!(false));
        moves.push(target);
    }
    moves
}
pub(crate) fn cannon(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(dr, dc) in ORTHO {
        let mut cursor = from;
        let mut screen = false;
        while let Some(to) = cursor.offset(dr, dc) {
            cursor = to;
            if collapsed(state, to) {
                break;
            }
            if let Some(target) = state.at(to) {
                if !screen {
                    if target.kind == "cannon" {
                        break;
                    }
                    screen = true;
                } else {
                    if target.kind != "cannon" && can_capture(state, piece, target) {
                        moves.push(MoveTarget::at(to));
                    }
                    break;
                }
            } else if screen {
                moves.push(MoveTarget::at(to));
            }
        }
    }
    moves
}
pub(crate) fn grasshopper(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let mut moves = Vec::new();
    for &(dr, dc) in DIAG.iter().chain(ORTHO.iter()) {
        let mut cursor = from;
        while let Some(to) = cursor.offset(dr, dc) {
            cursor = to;
            if collapsed(state, to) {
                break;
            }
            if state.at(to).is_some() {
                if let Some(landing_square) = to.offset(dr, dc)
                    && landing(state, piece, landing_square)
                {
                    moves.push(MoveTarget::at(landing_square));
                }
                break;
            }
        }
    }
    moves
}
pub(crate) fn checker(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let Some(actor) = piece.color.owner() else {
        return Vec::new();
    };
    let dir = actor.pawn_dir();
    let dirs = if piece.kind == "checkerKing" {
        DIAG.to_vec()
    } else {
        vec![(dir, -1), (dir, 1)]
    };
    let mut moves = Vec::new();
    for (dr, dc) in dirs {
        if let Some(adjacent) = from.offset(dr, dc) {
            if state.at(adjacent).is_none() && !collapsed(state, adjacent) {
                if !piece.flag("checkerChainCapture") {
                    moves.push(MoveTarget::at(adjacent));
                }
            } else if state
                .at(adjacent)
                .is_some_and(|target| can_capture(state, piece, target))
                && let Some(to) = adjacent.offset(dr, dc)
                && state.at(to).is_none()
                && !collapsed(state, to)
            {
                let mut target = MoveTarget::at(to);
                target.flags.insert("checkerCapture".into(), json!(true));
                target.flags.insert("jumpCapture".into(), json!(adjacent));
                moves.push(target);
            }
        }
    }
    let capture = moves.iter().any(|m| m.flag("checkerCapture"));
    if capture {
        moves.retain(|m| m.flag("checkerCapture"));
    }
    moves
}
fn large_rays(state: &GameState, piece: &Piece, from: Square) -> Vec<MoveTarget> {
    let dirs = if piece.kind == "bigBishop" {
        DIAG
    } else {
        ORTHO
    };
    let mut moves = Vec::new();
    for &(dr, dc) in dirs {
        let mut cursor = from;
        while let Some(anchor) = cursor.offset(dr, dc) {
            cursor = anchor;
            if anchor.row > 6 || anchor.col > 6 {
                break;
            }
            let cells = [
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
            ];
            if cells.iter().any(|&cell| {
                collapsed(state, cell)
                    || state.at(cell).is_some_and(|target| {
                        target.id != piece.id && !can_capture(state, piece, target)
                    })
            }) {
                break;
            }
            let occupied = cells
                .iter()
                .any(|&cell| state.at(cell).is_some_and(|target| target.id != piece.id));
            let captures: Vec<Square> = cells
                .iter()
                .copied()
                .filter(|&cell| state.at(cell).is_some_and(|target| target.id != piece.id))
                .collect();
            let mut target = MoveTarget::at(anchor);
            target.flags.insert("bigRookMove".into(), json!(true));
            target.flags.insert("anchorRow".into(), json!(anchor.row));
            target.flags.insert("anchorCol".into(), json!(anchor.col));
            target.flags.insert("highlightCells".into(), json!(cells));
            target
                .flags
                .insert("bigRookLandingCaptures".into(), json!(captures));
            moves.push(target);
            if occupied {
                break;
            }
        }
    }
    moves
}

#[cfg(test)]
mod v7_movement_tests {
    use super::*;
    use crate::geometry::Offset;
    use crate::move_program::{
        ActivationCondition, MoveNode, MoveProgram, MoveProgramLimits, MoveProgramSet, Primitive,
        SpatialMoveBoard,
    };

    fn empty_v7() -> GameState {
        let mut state = GameState::new(GameConfig::default(), 17).unwrap();
        state.board = vec![vec![None; 8]; 8];
        state.ruleset_id = RULES_VERSION_V7.into();
        state
    }

    #[test]
    fn no_action_loss_probe_considers_football_under_requested_turn() {
        let mut state = empty_v7();
        state.turn = Color::White;
        state.board[2][0] = Some(Piece::new("rook", Color::White, "opponent"));
        state.board[3][2] = Some(Piece::new("rook", Color::Black, "kicker"));
        state.board[3][3] = Some(Piece::new("football", PieceColor::Neutral, "ball"));
        state.board[3][4] = Some(Piece::new("wall", PieceColor::Neutral, "wall"));
        let mut probed = Vec::new();
        let has_move = has_any_legal_move_v7_with(&state, Color::Black, |probe, piece, square| {
            assert_eq!(probe.turn, Color::Black);
            probed.push((piece.id.clone(), square));
            Ok(piece.kind == "football")
        })
        .unwrap();
        assert!(has_move);
        assert_eq!(state.turn, Color::White);
        assert_eq!(
            probed,
            vec![
                ("kicker".into(), Square { row: 3, col: 2 }),
                ("ball".into(), Square { row: 3, col: 3 }),
            ]
        );
    }

    #[test]
    fn v7_capture_constraint_projection_propagates_invalid_counters() {
        let mut state = empty_v7();
        let mut queen = Piece::new("queen", Color::White, "attacker");
        let pawn = Piece::new("pawn", Color::Black, "target");
        state.set_flag("genevaConvention", Color::Black, true);
        assert!(!v7_movement_capture_constraints_allow(&state, &queen, &pawn).unwrap());
        queen.extra.insert("regencyHeir".into(), json!(true));
        assert!(v7_movement_capture_constraints_allow(&state, &queen, &pawn).unwrap());
        queen.extra.insert("promotionRushUntil".into(), json!(2));
        assert!(!v7_movement_capture_constraints_allow(&state, &queen, &pawn).unwrap());
        *state.turns_taken.get_mut(Color::White) = 2;
        assert!(v7_movement_capture_constraints_allow(&state, &queen, &pawn).unwrap());
        queen
            .extra
            .insert("capturesMade".into(), json!("not-a-number"));
        assert!(matches!(
            v7_movement_capture_constraints_allow(&state, &queen, &pawn),
            Err(EngineError::InvalidState(_))
        ));
    }

    #[test]
    fn v7_public_enumeration_stays_closed_while_draft_probe_remains_available() {
        let mut state = empty_v7();
        let rook = Piece::new("rook", Color::White, "rook");
        let from = Square { row: 4, col: 4 };
        state.board[4][4] = Some(rook.clone());
        assert!(matches!(
            legal_move_actions(&state),
            Err(EngineError::UnsupportedFeature(_))
        ));
        assert!(!piece_moves(&state, &rook, from).unwrap().is_empty());
        state.ruleset_id = RULES_VERSION_V6.into();
        assert!(!piece_moves(&state, &rook, from).unwrap().is_empty());
    }

    #[test]
    fn orthodox_opening_candidate_reuse_keeps_source_order_and_flag_payloads() {
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.mode = "play".into();
        let moves = legal_move_candidates(&state).unwrap();
        let expected = (0..8)
            .flat_map(|col| {
                let from = Square { row: 6, col };
                let single =
                    Action::movement(Color::White, from, MoveTarget::at(Square { row: 5, col }));
                let mut double = MoveTarget::at(Square { row: 4, col });
                double
                    .flags
                    .insert("standardPawnDoubleStep".into(), json!(true));
                [single, Action::movement(Color::White, from, double)]
            })
            .chain(
                [(1, 0), (1, 2), (6, 5), (6, 7)]
                    .into_iter()
                    .map(|(from_col, to_col)| {
                        Action::movement(
                            Color::White,
                            Square {
                                row: 7,
                                col: from_col,
                            },
                            MoveTarget::at(Square {
                                row: 5,
                                col: to_col,
                            }),
                        )
                    }),
            )
            .collect::<Vec<_>>();
        assert_eq!(moves, expected);
        let mut bound = moves[1].clone();
        bound.position_key = Some("not-an-unbound-action".into());
        assert!(matches!(
            v7_opening_validate_move(&state, &bound),
            Err(EngineError::IllegalAction)
        ));
        state.ruleset_id = RULES_VERSION_V7.into();
        state
            .extra
            .insert("cornerKick".into(), json!({"white":true,"black":false}));
        assert!(matches!(
            v7_opening_legal_move_actions(&state),
            Err(EngineError::UnsupportedFeature(_))
        ));
    }

    #[test]
    fn staged_relay_swaps_precede_base_moves_in_source_order() {
        // Frozen seed-19 normal first-play after White uses Relay has 128
        // exchanges followed per piece by the existing 20 orthodox moves.
        let mut state = GameState::new(GameConfig::default(), 19).unwrap();
        state.mode = "play".into();
        state.set_flag("relay", Color::White, true);
        let staged = legal_move_candidates_with_relay(&state).unwrap();
        assert_eq!(staged.len(), 148);
        assert_eq!(
            staged
                .iter()
                .filter(|action| action
                    .destination
                    .as_ref()
                    .is_some_and(|to| to.flag("relaySwap")))
                .count(),
            128
        );
        let from = Square { row: 6, col: 0 };
        let mut prefix = (1..8)
            .map(|col| {
                let mut target = MoveTarget::at(Square { row: 6, col });
                target.flags.insert("relaySwap".into(), json!(true));
                Action::movement(Color::White, from, target)
            })
            .collect::<Vec<_>>();
        let mut rook = MoveTarget::at(Square { row: 7, col: 0 });
        rook.flags.insert("relaySwap".into(), json!(true));
        prefix.push(Action::movement(Color::White, from, rook));
        prefix.push(Action::movement(
            Color::White,
            from,
            MoveTarget::at(Square { row: 5, col: 0 }),
        ));
        let mut double = MoveTarget::at(Square { row: 4, col: 0 });
        double
            .flags
            .insert("standardPawnDoubleStep".into(), json!(true));
        prefix.push(Action::movement(Color::White, from, double));
        assert_eq!(&staged[..prefix.len()], prefix.as_slice());
        let pawn = state.at(from).unwrap();
        let swap =
            RelaySwap::from_target(&state, pawn, from, staged[0].destination.as_ref().unwrap())
                .unwrap()
                .unwrap();
        assert_eq!(swap.from, from);
        assert_eq!(swap.to, Square { row: 6, col: 1 });
        assert_eq!(swap.target_id, state.at(swap.to).unwrap().id);
        let original_board = state.board.clone();
        let board = swap.board_after(&state).unwrap();
        let mut expected = original_board.clone();
        let mut acting_pawn = state.at(from).unwrap().clone();
        acting_pawn.moved = true;
        expected[from.row as usize][from.col as usize] = state.at(swap.to).cloned();
        expected[swap.to.row as usize][swap.to.col as usize] = Some(acting_pawn);
        assert_eq!(board, expected);
        assert_eq!(state.board, original_board);
        let reverse_from = Square { row: 7, col: 0 };
        let reverse_to = Square { row: 6, col: 0 };
        let reverse_mover = state.at(reverse_from).unwrap();
        let mut reverse_target = MoveTarget::at(reverse_to);
        reverse_target.flags.insert("relaySwap".into(), json!(true));
        let reverse = RelaySwap::from_target(&state, reverse_mover, reverse_from, &reverse_target)
            .unwrap()
            .unwrap();
        let reverse_board = reverse.board_after(&state).unwrap();
        assert_eq!(reverse_board[6][0].as_ref().unwrap().id, reverse_mover.id);
        assert!(reverse_board[6][0].as_ref().unwrap().moved);
        assert_eq!(reverse_board[7][0].as_ref().unwrap().id, pawn.id);
        assert!(!reverse_board[7][0].as_ref().unwrap().moved);
        let mut replaced_target = state.clone();
        replaced_target.board[swap.to.row as usize][swap.to.col as usize] =
            Some(Piece::new("pawn", Color::White, "replacement"));
        assert!(matches!(
            swap.board_after(&replaced_target),
            Err(EngineError::IllegalAction)
        ));
        let mut forged = staged[0].destination.clone().unwrap();
        forged.flags.insert("capture".into(), json!(true));
        assert!(matches!(
            RelaySwap::from_target(&state, pawn, from, &forged),
            Err(EngineError::IllegalAction)
        ));
        let mut off_axis = MoveTarget::at(Square { row: 7, col: 1 });
        off_axis.flags.insert("relaySwap".into(), json!(true));
        assert!(matches!(
            RelaySwap::from_target(&state, pawn, from, &off_axis),
            Err(EngineError::IllegalAction)
        ));
        let impostor = Piece::new("pawn", Color::White, "different-origin-id");
        assert!(matches!(
            RelaySwap::from_target(
                &state,
                &impostor,
                from,
                staged[0].destination.as_ref().unwrap()
            ),
            Err(EngineError::IllegalAction)
        ));
        // The older v6 candidate path does not inherit the staged v7 swaps.
        assert_eq!(legal_move_candidates(&state).unwrap().len(), 20);
    }

    #[test]
    fn v7_spatial_projection_can_feed_a_bounded_program_cursor() {
        fn deny_capture(_: &crate::SpatialState, _: &str, _: &str, _: bool) -> bool {
            false
        }
        fn deny_shift(_: &crate::SpatialState, _: &str, _: &str) -> bool {
            false
        }
        let mut state = empty_v7();
        state.board[4][4] = Some(Piece::new("rook", Color::White, "rook"));
        let program = MoveProgramSet {
            base: MoveProgram {
                source_id: "probe-only".into(),
                roots: vec![MoveNode {
                    primitive: Primitive::Move,
                    direction: Offset::new(0, 1),
                    max_distance: Some(1),
                    activation_condition: ActivationCondition::Any,
                    activate_at_parent_distance: None,
                    children: Vec::new(),
                }],
            },
            modifiers: Vec::new(),
        };
        let spatial = crate::SpatialState::from_v7_source(&state).unwrap();
        let board = SpatialMoveBoard {
            state: &spatial,
            capture: deny_capture,
            shift: deny_shift,
        };
        let mut cursor = program
            .cursor(&board, "rook", MoveProgramLimits::default())
            .unwrap();
        let page = cursor.next_page(1, 1).unwrap();
        assert_eq!(page.examined, 1);
        assert_eq!(page.raw.len(), 1);
        assert_eq!(page.raw[0].intent.selected, crate::Coord::new(4, 5));

        state
            .extra
            .insert("collapsedCells".into(), json!([{"row":4,"col":5}]));
        let spatial = crate::SpatialState::from_v7_source(&state).unwrap();
        let board = SpatialMoveBoard {
            state: &spatial,
            capture: deny_capture,
            shift: deny_shift,
        };
        let mut cursor = program
            .cursor(&board, "rook", MoveProgramLimits::default())
            .unwrap();
        let page = cursor.next_page(1, 1).unwrap();
        assert!(page.raw.is_empty());
    }
}
