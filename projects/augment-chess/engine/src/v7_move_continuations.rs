//! 동결 v7의 이동 후 같은 기물 추가 이동과 승급 선택 후속 처리.
//!
//! 이동 시작 스냅샷, 소비된 기물 표식, source tail의 분기 순서와 선택 상태를
//! 소유한다. 전이 소유자가 실제 이동·체크 반응·승급 효과·endMove를 실행한다.
//! 이 콜백들은 전이의 사본 안에서 호출하며 오류를 최종 commit 전에 전파한다.
//! Source: main-OahWs0tU.js / SHA-256
//! e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c.

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const FORCED_FLAGS: &[&str] = &[
    "frenzyExtraMove",
    "thiefSecondMove",
    "fileSurgeSecondMove",
    "rookLiftSecondMove",
    "ironMonarchExtraMove",
    "underpromotionSecondMove",
    "checkerChainCapture",
    "madHorseSecondMove",
    "platformExtraMove",
    "desperado",
];

#[derive(Clone, Debug)]
pub(crate) struct V7MoveContinuationSnapshot {
    pub(crate) moved_as_type: String,
    pub(crate) moved_ability_type: String,
    pub(crate) was_file_surge_second_move: bool,
    pub(crate) was_rook_lift_second_move: bool,
    pub(crate) rook_lift_chain_before_move: Option<Value>,
    pub(crate) had_queued_backward_knight_turn: bool,
    pub(crate) was_desperado_move: bool,
    clear_consumed_forced_flags: bool,
}

/// main91655-91676: thiefSecondMove는 시작 시 제거하고, 다른 소비된 표식은
/// trackMovingProgress/crownCheckerIfNeeded 다음 단계에서 한꺼번에 제거한다.
pub(crate) fn capture_move_start(piece: &mut Piece) -> V7MoveContinuationSnapshot {
    let flag = |field| crate::observation::truth(piece.extra.get(field));
    let snapshot = V7MoveContinuationSnapshot {
        moved_as_type: piece.kind.clone(),
        moved_ability_type: piece.ability_kind().to_owned(),
        was_file_surge_second_move: flag("fileSurgeSecondMove"),
        was_rook_lift_second_move: flag("rookLiftSecondMove"),
        rook_lift_chain_before_move: piece
            .extra
            .get("rookLiftChain")
            .filter(|value| crate::observation::truth(Some(value)))
            .cloned(),
        had_queued_backward_knight_turn: flag("queuedBackwardKnightTurn"),
        was_desperado_move: flag("desperado"),
        clear_consumed_forced_flags: [
            "frenzyExtraMove",
            "fileSurgeSecondMove",
            "rookLiftSecondMove",
            "ironMonarchExtraMove",
            "underpromotionSecondMove",
            "checkerChainCapture",
            "madHorseSecondMove",
            "platformExtraMove",
        ]
        .iter()
        .any(|field| flag(field)),
    };
    piece.extra.shift_remove("thiefSecondMove");
    snapshot
}

pub(crate) fn clear_consumed_extra_move_flags(
    piece: &mut Piece,
    before: &V7MoveContinuationSnapshot,
) {
    if before.clear_consumed_forced_flags {
        for field in FORCED_FLAGS {
            piece.extra.shift_remove(*field);
        }
    }
}

/// 이동/capture kernel이 확정한 결과만 전달한다. captured_something은 원문의
/// bool이며, 직접 포획·jump 포획·특수 공격을 단순한 Vec 길이로 대체하지 않는다.
#[derive(Clone, Debug)]
pub(crate) struct V7MoveContinuationInput {
    pub(crate) piece_id: String,
    pub(crate) from: Square,
    pub(crate) landing: Square,
    pub(crate) start: V7MoveContinuationSnapshot,
    pub(crate) captured_something: bool,
    pub(crate) direct_captured_piece: Option<Piece>,
    pub(crate) internal_thief_jump: bool,
    pub(crate) mad_horse_friendly_capture: bool,
    pub(crate) captured_mad_horse_friendly: bool,
    pub(crate) checker_jumped_attack: bool,
    pub(crate) checker_capture: bool,
    /// 현재 source-pinned v7 catalog의 InternalSixFixes chameleon 분기 결과.
    pub(crate) chameleon_transformed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum V7ContinuationControl {
    /// 후속 콜백 또는 보통 endMove까지 계속한다.
    Continue,
    /// 이 분기가 count/history/선택까지 처리했다. 같은 행동을 더 기록하지 않는다.
    Retained,
    /// 데스페라도 제거가 끝났다. 전이 소유자가 원문의 endMove를 한 번 실행한다.
    FinishMove(Color),
    /// 이 분기의 제거 후속으로 게임이 종료되어 source tail이 반환했다.
    Terminal,
}

fn boundary(state: &GameState) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 move continuation outside the pinned ruleset".into(),
        ));
    }
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::InvalidState(
            "v7 move continuation requires the frozen 8x8 board".into(),
        ));
    }
    Ok(())
}

fn owner(piece: &Piece) -> Result<Color> {
    piece.color.owner().ok_or_else(|| {
        EngineError::InvalidState("v7 extra-move continuation requires a player-owned piece".into())
    })
}

fn live_piece(state: &GameState, id: &str, at: Square) -> Option<Piece> {
    state.at(at).filter(|piece| piece.id == id).cloned()
}

fn source_piece(state: &GameState, id: &str, at: Square) -> Result<Piece> {
    live_piece(state, id, at)
        .or_else(|| {
            state
                .board
                .iter()
                .flatten()
                .flatten()
                .chain(state.captures.white.iter())
                .chain(state.captures.black.iter())
                .find(|piece| piece.id == id)
                .cloned()
        })
        .ok_or_else(|| {
            EngineError::InvalidState(format!(
                "v7 continuation cannot recover source mover object alias {id:?}"
            ))
        })
}

fn write_piece(state: &mut GameState, piece: &Piece) {
    // 원문은 같은 JS 객체를 board와 captures가 함께 참조한다. 포획 반응으로
    // 제거된 mover의 후속 표식 변경도 이미 저장된 포획 alias에 반영한다.
    crate::v7_board_hazards::replace_object_aliases(state, piece);
}

fn count_move(state: &mut GameState) -> Result<()> {
    state.move_count = state
        .move_count
        .checked_add(1)
        .ok_or_else(|| EngineError::InvalidState("v7 extra-move count overflow".into()))?;
    Ok(())
}

fn legal_moves(state: &GameState, _piece: &Piece, at: Square) -> Result<Vec<MoveTarget>> {
    // Source getLegalMoves(row,col)는 인자로 넘긴 mover가 아니라 그 좌표의
    // 현재 board 기물을 읽는다. 제거된 mover로 가상의 후보를 만들지 않는다.
    let Some(piece) = state.at(at) else {
        return Ok(Vec::new());
    };
    crate::movement::v7_legal_move_targets(
        state,
        piece,
        at,
        crate::movement::V7MoveOptions::default(),
    )
}

fn clear_selection(state: &mut GameState) {
    state.extra.insert("selected".into(), Value::Null);
    state.extra.insert("legalMoves".into(), json!([]));
    state.extra.insert("targeting".into(), Value::Null);
}

fn select_moves(state: &mut GameState, at: Square, moves: &[MoveTarget]) {
    state.extra.insert("selected".into(), json!(at));
    state.extra.insert("legalMoves".into(), json!(moves));
    state.extra.insert("targeting".into(), Value::Null);
}

/// main91035. 호출자에게 true/false를 돌려주며 count/history는 만들지 않는다.
pub(crate) fn start_forced_extra_move_v7(
    state: &mut GameState,
    piece: &Piece,
    at: Square,
) -> Result<bool> {
    boundary(state)?;
    let moves = legal_moves(state, piece, at)?;
    if moves.is_empty() {
        return Ok(false);
    }
    select_moves(state, at, &moves);
    Ok(true)
}

fn start_counted_flag(
    state: &mut GameState,
    piece: &mut Piece,
    at: Square,
    flag: &str,
    history_label: &str,
) -> Result<Option<bool>> {
    piece.extra.insert(flag.into(), json!(true));
    write_piece(state, piece);
    if legal_moves(state, piece, at)?.is_empty() {
        piece.extra.shift_remove(flag);
        write_piece(state, piece);
        return Ok(None);
    }
    count_move(state)?;
    crate::replay::record(state, history_label)?;
    Ok(Some(start_forced_extra_move_v7(state, piece, at)?))
}

/// main95196: 모든 같은 색 재배치 표식을 지운 뒤 선택→count→replay→history.
pub(crate) fn try_start_reposition_second_move_v7(
    state: &mut GameState,
    piece: &mut Piece,
    at: Square,
) -> Result<bool> {
    boundary(state)?;
    let color = owner(piece)?;
    if state.free_move_resolution == Some(color)
        || !crate::observation::truth(piece.extra.get("repositionSecondMove"))
        || crate::observation::truth(
            piece
                .extra
                .get("repositionSecondMove")
                .and_then(|v| v.get("used")),
        )
    {
        return Ok(false);
    }
    for candidate in state.board.iter_mut().flatten().flatten() {
        if candidate.color == color {
            candidate.extra.shift_remove("repositionSecondMove");
        }
    }
    piece
        .extra
        .insert("repositionSecondMove".into(), json!({"used":true}));
    write_piece(state, piece);
    let moves = legal_moves(state, piece, at)?;
    if moves.is_empty() {
        piece.extra.shift_remove("repositionSecondMove");
        write_piece(state, piece);
        return Ok(false);
    }
    select_moves(state, at, &moves);
    count_move(state)?;
    // main95210/99912: 확정할 실제 pending capture가 없으면 replay는
    // 그대로 둔다. 승격 창의 현재 상태를 before snapshot으로 만들지 않는다.
    crate::replay::commit_active_move(state, color)?;
    crate::replay::record(state, "reposition")?;
    Ok(true)
}

fn should_retain_backward_knight(
    state: &GameState,
    color: Color,
    moved_as_type: &str,
    from: Square,
    to: Square,
    captured: bool,
) -> bool {
    captured
        && moved_as_type == "knight"
        && state.flag("backwardKnight", color)
        && state.free_move_resolution != Some(color)
        && match color {
            Color::White => to.row > from.row,
            Color::Black => to.row < from.row,
        }
}

/// Backward Knight의 한 번의 이동과 이미 준비된 continuation 판정 입력.
pub(crate) struct V7BackwardKnightMove<'a> {
    pub(crate) color: Color,
    pub(crate) moved_as_type: &'a str,
    pub(crate) from: Square,
    pub(crate) to: Square,
    pub(crate) captured: bool,
    pub(crate) queued: bool,
}

/// main91430. 이미 queue된 턴은 free-move predicate와 별개로 우선한다.
pub(crate) fn retain_backward_knight_turn_v7(
    state: &mut GameState,
    piece: &mut Piece,
    input: V7BackwardKnightMove<'_>,
) -> Result<bool> {
    let V7BackwardKnightMove {
        color,
        moved_as_type,
        from,
        to,
        captured,
        queued,
    } = input;
    boundary(state)?;
    if !queued
        && piece.extra.get("queuedBackwardKnightTurn") != Some(&json!(true))
        && !should_retain_backward_knight(state, color, moved_as_type, from, to, captured)
    {
        return Ok(false);
    }
    piece.extra.shift_remove("queuedBackwardKnightTurn");
    write_piece(state, piece);
    clear_selection(state);
    count_move(state)?;
    crate::replay::record(state, "backward knight")?;
    Ok(true)
}

fn optional_knight_continuation(piece: &Piece) -> bool {
    // 현재 source-pinned v7의 ThiefRemake에서 도적의 추가 이동은 필수다.
    !crate::observation::truth(piece.extra.get("thiefSecondMove"))
        && [
            "ironMonarchExtraMove",
            "rookLiftSecondMove",
            "fileSurgeSecondMove",
            "madHorseSecondMove",
            "platformExtraMove",
        ]
        .iter()
        .any(|field| crate::observation::truth(piece.extra.get(*field)))
}

fn queued_knight_reasons(piece: &Piece) -> Vec<String> {
    let mut seen = BTreeSet::new();
    piece
        .extra
        .get("queuedKnightExtraMoveReasons")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|reason| ["mad-horse", "file-surge"].contains(reason))
        .filter(|reason| seen.insert((*reason).to_owned()))
        .map(str::to_owned)
        .collect()
}

fn queue_knight_reason(state: &mut GameState, piece: &mut Piece, reason: &str) {
    let mut reasons = queued_knight_reasons(piece);
    if !reasons.iter().any(|candidate| candidate == reason) {
        reasons.push(reason.into());
    }
    piece
        .extra
        .insert("queuedKnightExtraMoveReasons".into(), json!(reasons));
    write_piece(state, piece);
}

/// main95264. history 이후 startForcedExtraMove가 false여도 즉시 반환한다.
pub(crate) fn start_next_queued_knight_extra_move_v7(
    state: &mut GameState,
    piece: &mut Piece,
    at: Square,
) -> Result<bool> {
    boundary(state)?;
    let mut reasons = queued_knight_reasons(piece);
    piece.extra.shift_remove("queuedKnightExtraMoveReasons");
    write_piece(state, piece);
    while !reasons.is_empty() {
        let reason = reasons.remove(0);
        if !reasons.is_empty() {
            piece
                .extra
                .insert("queuedKnightExtraMoveReasons".into(), json!(reasons));
        }
        piece.extra.insert(
            if reason == "mad-horse" {
                "madHorseSecondMove"
            } else {
                "fileSurgeSecondMove"
            }
            .into(),
            json!(true),
        );
        write_piece(state, piece);
        if optional_knight_continuation(piece) && !legal_moves(state, piece, at)?.is_empty() {
            count_move(state)?;
            crate::replay::record(state, &reason)?;
            return start_forced_extra_move_v7(state, piece, at);
        }
        for field in [
            "madHorseSecondMove",
            "thiefSecondMove",
            "fileSurgeSecondMove",
            "queuedKnightExtraMoveReasons",
        ] {
            piece.extra.shift_remove(field);
        }
        write_piece(state, piece);
    }
    Ok(false)
}

/// main92513-92529. 반환 뒤 root가 breakInitiativeByCheck/knightJourney를 실행한다.
pub(crate) fn before_middle_callbacks_v7(
    state: &mut GameState,
    input: &V7MoveContinuationInput,
) -> Result<V7ContinuationControl> {
    crate::legal_profile::measure("move_before_middle_callbacks", || {
        before_middle_callbacks_v7_profiled(state, input)
    })
}

pub(crate) fn before_middle_callbacks_v7_profiled(
    state: &mut GameState,
    input: &V7MoveContinuationInput,
) -> Result<V7ContinuationControl> {
    boundary(state)?;
    if state.mode == "gameover" {
        return Ok(V7ContinuationControl::Terminal);
    }
    let mut piece = source_piece(state, &input.piece_id, input.landing)?;
    if input.internal_thief_jump
        && !input.captured_something
        && live_piece(state, &input.piece_id, input.landing).is_some()
    {
        // 이 분기에는 free guard가 없고, 예약 FreeMove 쪽에서 이후 flag를 제거한다.
        crate::transition::refresh_submerged(state)?;
        if let Some(current) = live_piece(state, &input.piece_id, input.landing) {
            piece = current;
        }
        piece.extra.insert("thiefSecondMove".into(), json!(true));
        write_piece(state, &piece);
        if start_forced_extra_move_v7(state, &piece, input.landing)? {
            return Ok(V7ContinuationControl::Retained);
        }
        piece.extra.shift_remove("thiefSecondMove");
        write_piece(state, &piece);
    }
    if try_start_reposition_second_move_v7(state, &mut piece, input.landing)? {
        return Ok(V7ContinuationControl::Retained);
    }
    if input.start.had_queued_backward_knight_turn
        || should_retain_backward_knight(
            state,
            owner(&piece)?,
            &input.start.moved_as_type,
            input.from,
            input.landing,
            input.captured_something,
        )
    {
        piece
            .extra
            .insert("queuedBackwardKnightTurn".into(), json!(true));
        write_piece(state, &piece);
    }
    Ok(V7ContinuationControl::Continue)
}

fn active_platform_cells(state: &GameState) -> Vec<Square> {
    let Some(rule) = state.extra.get("platformRule") else {
        return Vec::new();
    };
    if !crate::observation::truth(rule.get("enabled")) {
        return Vec::new();
    }
    let fallback = [rule.get("cell").unwrap_or(&Value::Null)];
    let cells = rule
        .get("cells")
        .and_then(Value::as_array)
        .filter(|cells| !cells.is_empty());
    let cells = cells
        .map(|cells| cells.iter().collect::<Vec<_>>())
        .unwrap_or_else(|| fallback.to_vec());
    let mut seen = BTreeSet::new();
    cells
        .into_iter()
        .filter_map(|cell| {
            let row = cell.get("row").and_then(Value::as_f64)?;
            let col = cell.get("col").and_then(Value::as_f64)?;
            if !row.is_finite()
                || !col.is_finite()
                || row.fract() != 0.0
                || col.fract() != 0.0
                || !(0.0..8.0).contains(&row)
                || !(0.0..8.0).contains(&col)
            {
                return None;
            }
            let at = Square {
                row: row as u8,
                col: col as u8,
            };
            seen.insert(at).then_some(at)
        })
        .collect()
}

pub(crate) fn try_start_platform_extra_move_v7(
    state: &mut GameState,
    piece: &mut Piece,
    at: Square,
) -> Result<bool> {
    boundary(state)?;
    let color = owner(piece)?;
    if state.free_move_resolution == Some(color)
        || state.mode == "gameover"
        || live_piece(state, &piece.id, at).is_none()
        || piece.id.is_empty()
        || !active_platform_cells(state).contains(&at)
    {
        return Ok(false);
    }
    let rule = state
        .extra
        .get_mut("platformRule")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            EngineError::InvalidState("v7 active platformRule must be an object".into())
        })?;
    if rule.get("triggeredIds").and_then(Value::as_array).is_none() {
        rule.insert("triggeredIds".into(), json!([]));
    }
    let ids = rule
        .get_mut("triggeredIds")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            EngineError::InvalidState("v7 platformRule triggeredIds must be an array".into())
        })?;
    if !ids.iter().any(|id| id.as_str() == Some(piece.id.as_str())) {
        ids.push(json!(piece.id));
    }
    piece.extra.insert("platformExtraMove".into(), json!(true));
    write_piece(state, piece);
    if legal_moves(state, piece, at)?.is_empty() {
        piece.extra.shift_remove("platformExtraMove");
        write_piece(state, piece);
        return Ok(false);
    }
    state.en_passant = None;
    count_move(state)?;
    crate::replay::record(state, "platform")?;
    start_forced_extra_move_v7(state, piece, at)
}

/// Castle의 king→rook 순서로 호출한다. 대형 기물은 platform cell 위치를 사용한다.
pub(crate) fn try_start_platform_for_piece_v7(state: &mut GameState, id: &str) -> Result<bool> {
    boundary(state)?;
    let Some(at) = active_platform_cells(state)
        .into_iter()
        .find(|&at| state.at(at).is_some_and(|piece| piece.id == id))
    else {
        return Ok(false);
    };
    let mut piece = state.at(at).cloned().ok_or_else(|| {
        EngineError::InvalidState("v7 platform continuation identity disappeared".into())
    })?;
    try_start_platform_extra_move_v7(state, &mut piece, at)
}

fn try_start_frenzy(
    state: &mut GameState,
    piece: &mut Piece,
    at: Square,
    captured: bool,
) -> Result<bool> {
    let color = owner(piece)?;
    if !captured
        || state.free_move_resolution == Some(color)
        || piece.kind != "pawn"
        || !crate::observation::truth(piece.extra.get("frenzy"))
        || state.mode == "gameover"
        || live_piece(state, &piece.id, at).is_none()
    {
        return Ok(false);
    }
    Ok(start_counted_flag(state, piece, at, "frenzyExtraMove", "frenzy")?.unwrap_or(false))
}

fn try_start_iron_monarch(
    state: &mut GameState,
    piece: &mut Piece,
    at: Square,
    captured: Option<&Piece>,
) -> Result<bool> {
    let color = owner(piece)?;
    if state.free_move_resolution == Some(color)
        || state.mode == "gameover"
        || live_piece(state, &piece.id, at).is_none()
        || !state.flag("ironMonarch", color)
        || !(crate::observation::truth(piece.extra.get("regencyHeir"))
            || ["king", "royalKnight", "shotgunKing", "darkWizard"].contains(&piece.kind.as_str()))
        || !captured.is_some_and(|captured| {
            captured.color != piece.color && ["pawn", "fanatic"].contains(&captured.kind.as_str())
        })
    {
        return Ok(false);
    }
    Ok(
        start_counted_flag(state, piece, at, "ironMonarchExtraMove", "iron monarch")?
            .unwrap_or(false),
    )
}

fn remove_desperado(state: &mut GameState, piece: &mut Piece, at: Square) -> Result<()> {
    let color = owner(piece)?;
    piece.extra.shift_remove("desperado");
    piece.extra.shift_remove("quantum");
    write_piece(state, piece);
    let removed_at = live_piece(state, &piece.id, at).map(|_| at).or_else(|| {
        (0..8)
            .flat_map(|row| (0..8).map(move |col| Square { row, col }))
            .find(|&at| {
                state
                    .at(at)
                    .is_some_and(|candidate| candidate.id == piece.id)
            })
    });
    if let Some(square) = removed_at {
        state.board[usize::from(square.row)][usize::from(square.col)] = None;
        crate::v7_board_hazards::resolve_reaper_nearby_deaths(
            state,
            &[crate::v7_board_hazards::EnvironmentalRemoval {
                piece: piece.clone(),
                square,
                capture_owner: color.opponent(),
            }],
        )?;
    }
    let capturer = color.opponent();
    crate::transition::cancel_prophecies_by_capture(state)?;
    crate::v7_capture_reactions::record_new_card_capture_reactions(state, piece, capturer, false)?;
    state.captures.get_mut(capturer).push(piece.clone());
    crate::v7_promotion::resolve_recycling_holdout_after_queen_loss_v7(state, piece)?;
    crate::transition::add_capture_type(state, "capturedTypes", capturer, &piece.kind)?;
    // main91153: TYPE_LABELS[type] || type를 원문의 v7 이름으로 조회한다.
    crate::replay::add_piece_action_log(
        state,
        piece,
        Some(at),
        None,
        format!(
            "데스페라도: {}이 이동을 마치고 사망했습니다.",
            crate::replay::source_piece_label(&piece.kind)
                .filter(|label| !label.is_empty())
                .unwrap_or(&piece.kind),
        ),
    )?;
    crate::v7_capture_objectives::check_campaign_objectives(state)?;
    Ok(())
}

/// main91116. 원문의 데스페라도 전용 제거 콜백 순서를 보존한다.
pub(crate) fn advance_desperado_step_v7(
    state: &mut GameState,
    piece: &mut Piece,
    at: Square,
) -> Result<V7ContinuationControl> {
    boundary(state)?;
    if !crate::observation::truth(piece.extra.get("desperado")) {
        return Ok(V7ContinuationControl::Continue);
    }
    let previous_remaining = crate::card_effects::js_number(
        piece
            .extra
            .get("desperado")
            .and_then(|value| value.get("remaining")),
        0,
    );
    if previous_remaining.is_some_and(|number| number.is_infinite()) {
        return Err(EngineError::InvalidState(
            "v7 desperado remaining coerces to Infinity; bounded continuation requires a finite remaining count".into(),
        ));
    }
    // 원문 Number가 NaN이면 remaining > 0이 false여서 즉시 제거한다.
    // 제거 전에 표식 전체를 삭제하므로 최종 capture에 NaN/null이 남지 않는다.
    let remaining = previous_remaining
        .filter(|number| !number.is_nan())
        .map(|number| (number - 1.0).max(0.0));
    piece.extra.get_mut("desperado").and_then(Value::as_object_mut)
        .ok_or_else(|| EngineError::InvalidState(
            "v7 desperado must be an object; source remaining assignment requires a mutable object".into(),
        ))?.insert("remaining".into(), remaining.map(|number| json!(number)).unwrap_or(Value::Null));
    write_piece(state, piece);
    if remaining.is_some_and(|remaining| remaining > 0.0)
        && live_piece(state, &piece.id, at).is_some()
    {
        let moves = legal_moves(state, piece, at)?;
        if !moves.is_empty() {
            count_move(state)?;
            crate::replay::record(state, "desperado")?;
            select_moves(state, at, &moves);
            return Ok(V7ContinuationControl::Retained);
        }
    }
    let color = owner(piece)?;
    remove_desperado(state, piece, at)?;
    Ok(if state.mode == "gameover" {
        V7ContinuationControl::Terminal
    } else {
        V7ContinuationControl::FinishMove(color)
    })
}

/// main92034/92063/92097의 HP 공격 후 제자리 후속 처리. 호출자는 HP 피해,
/// 해당 분기의 Reaper 즉시 실행, Transcendence 변환과 HP notation을 먼저 끝낸다.
/// 직접 HP 공격만 Desperado를 진행한다. 포탈·앙파상 HP 공격은 바로 Frenzy로
/// 이어지며 세 분기 모두 Platform/Reposition을 시도하지 않는다.
pub(crate) fn after_stationary_hp_v7(
    state: &mut GameState,
    piece: &mut Piece,
    origin: Square,
    start: &V7MoveContinuationSnapshot,
    captured_something: bool,
    direct_hp_attack: bool,
) -> Result<V7ContinuationControl> {
    boundary(state)?;
    piece
        .extra
        .insert("coolGuyCapturedLast".into(), json!(captured_something));
    clear_consumed_extra_move_flags(piece, start);
    write_piece(state, piece);
    state.en_passant = None;
    let color = owner(piece)?;
    // 직접 HP 분기에는 gameover guard가 없으므로 원문 순서대로 Desperado를
    // 시도한다. 제거된 mover도 호출자가 전달한 객체 alias로 처리한다.
    if direct_hp_attack
        && state.free_move_resolution != Some(color)
        && crate::observation::truth(piece.extra.get("desperado"))
    {
        let control = advance_desperado_step_v7(state, piece, origin)?;
        if control != V7ContinuationControl::Continue {
            return Ok(control);
        }
    }
    if state.mode != "gameover" && try_start_frenzy(state, piece, origin, captured_something)? {
        return Ok(V7ContinuationControl::Retained);
    }
    Ok(if state.mode == "gameover" {
        V7ContinuationControl::Terminal
    } else {
        V7ContinuationControl::Continue
    })
}

/// main92531-92610. Continue만 root의 일반 endMove로 이어진다.
pub(crate) fn after_middle_callbacks_v7(
    state: &mut GameState,
    input: &V7MoveContinuationInput,
) -> Result<V7ContinuationControl> {
    crate::legal_profile::measure("move_after_middle_callbacks", || {
        after_middle_callbacks_v7_profiled(state, input)
    })
}

pub(crate) fn after_middle_callbacks_v7_profiled(
    state: &mut GameState,
    input: &V7MoveContinuationInput,
) -> Result<V7ContinuationControl> {
    boundary(state)?;
    if state.mode == "gameover" {
        return Ok(V7ContinuationControl::Terminal);
    }
    let mut piece = source_piece(state, &input.piece_id, input.landing)?;
    let color = owner(&piece)?;
    let free_move = state.free_move_resolution == Some(color);
    if !free_move && input.start.was_desperado_move {
        let result = advance_desperado_step_v7(state, &mut piece, input.landing)?;
        if result != V7ContinuationControl::Continue {
            return Ok(result);
        }
    }
    if !free_move
        && input.mad_horse_friendly_capture
        && input.captured_mad_horse_friendly
        && input.start.moved_as_type == "knight"
        && live_piece(state, &input.piece_id, input.landing).is_some()
    {
        if !input.start.was_file_surge_second_move
            && state.flag("fileSurge", color)
            && [0, 7].contains(&input.landing.col)
        {
            queue_knight_reason(state, &mut piece, "file-surge");
        }
        if start_counted_flag(
            state,
            &mut piece,
            input.landing,
            "madHorseSecondMove",
            "mad horse",
        )?
        .is_some()
        {
            return Ok(V7ContinuationControl::Retained);
        }
    }
    if !free_move && start_next_queued_knight_extra_move_v7(state, &mut piece, input.landing)? {
        return Ok(V7ContinuationControl::Retained);
    }
    if input.start.moved_as_type == "pawn"
        && try_start_frenzy(state, &mut piece, input.landing, input.captured_something)?
    {
        return Ok(V7ContinuationControl::Retained);
    }
    if try_start_iron_monarch(
        state,
        &mut piece,
        input.landing,
        input
            .direct_captured_piece
            .as_ref()
            .filter(|_| input.captured_something),
    )? {
        return Ok(V7ContinuationControl::Retained);
    }
    if try_start_platform_extra_move_v7(state, &mut piece, input.landing)? {
        return Ok(V7ContinuationControl::Retained);
    }
    if !free_move
        && !input.chameleon_transformed
        && input.checker_jumped_attack
        && ["checker", "checkerKing"].contains(&input.start.moved_as_type.as_str())
        && input.checker_capture
        && start_counted_flag(
            state,
            &mut piece,
            input.landing,
            "checkerChainCapture",
            "checker chain",
        )?
        .is_some()
    {
        return Ok(V7ContinuationControl::Retained);
    }
    if !free_move
        && !input.start.was_file_surge_second_move
        && input.start.moved_as_type == "knight"
        && state.flag("fileSurge", color)
        && [0, 7].contains(&input.landing.col)
        && start_counted_flag(
            state,
            &mut piece,
            input.landing,
            "fileSurgeSecondMove",
            "file surge",
        )?
        .is_some()
    {
        return Ok(V7ContinuationControl::Retained);
    }
    if !free_move && input.start.moved_as_type == "rook" && state.flag("rookLift", color) {
        let chain = input
            .start
            .rook_lift_chain_before_move
            .as_ref()
            .filter(|_| input.start.was_rook_lift_second_move);
        let turns = chain
            .and_then(|chain| crate::card_effects::js_number(chain.get("turns"), 0))
            .filter(|number| !number.is_nan())
            .unwrap_or(0.0)
            .max(0.0);
        let corner = |square: Square| [0, 7].contains(&square.row) && [0, 7].contains(&square.col);
        if !input.captured_something && corner(input.landing) && turns < 3.0 {
            let blocked = chain.is_some() && corner(input.from);
            piece.extra.insert("rookLiftChain".into(), json!({
                "blockedCornerRow":if blocked { json!(input.from.row) } else { Value::Null },
                "blockedCornerCol":if blocked { json!(input.from.col) } else { Value::Null },"turns":turns+1.0,
            }));
            if start_counted_flag(
                state,
                &mut piece,
                input.landing,
                "rookLiftSecondMove",
                "rook lift",
            )?
            .is_some()
            {
                return Ok(V7ContinuationControl::Retained);
            }
        }
        piece.extra.shift_remove("rookLiftSecondMove");
        piece.extra.shift_remove("rookLiftChain");
        write_piece(state, &piece);
    }
    if retain_backward_knight_turn_v7(
        state,
        &mut piece,
        V7BackwardKnightMove {
            color,
            moved_as_type: &input.start.moved_as_type,
            from: input.from,
            to: input.landing,
            captured: input.captured_something,
            queued: false,
        },
    )? {
        return Ok(V7ContinuationControl::Retained);
    }
    Ok(V7ContinuationControl::Continue)
}

/// 승급 소유자가 직접 effects/종교 판정을 완료한 뒤 호출한다. terminal은
/// 호출자가 먼저 처리한다. 검토된 oracle의 AI 자동 승급은 비활성이다.
pub(crate) fn after_promotion_choice_v7(
    state: &mut GameState,
    actor: Color,
    square: Square,
    piece_id: &str,
    promoted_type: &str,
    field_promotion: bool,
) -> Result<bool> {
    boundary(state)?;
    let mut piece = live_piece(state, piece_id, square).ok_or_else(|| {
        EngineError::InvalidState(
            "v7 post-promotion continuation source identity disappeared".into(),
        )
    })?;
    if !field_promotion && try_start_reposition_second_move_v7(state, &mut piece, square)? {
        return Ok(true);
    }
    if !field_promotion
        && piece.color == actor
        && state.flag("underpromotion", actor)
        && crate::v7_promotion::is_minor_promotion_result_v7(state, promoted_type)
    {
        piece.extra.shift_remove("freshNoCaptureUntil");
        piece
            .extra
            .insert("coolGuyCapturedLast".into(), json!(false));
        if start_counted_flag(
            state,
            &mut piece,
            square,
            "underpromotionSecondMove",
            "underpromotion",
        )?
        .is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn difference(left: &Value, right: &Value, path: &str) -> Option<String> {
        if left == right {
            return None;
        }
        match (left, right) {
            (Value::Number(left), Value::Number(right)) if left.as_f64() == right.as_f64() => None,
            (Value::Object(left), Value::Object(right)) => {
                let keys = left
                    .keys()
                    .chain(right.keys())
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>();
                for key in keys {
                    let path = format!("{path}/{key}");
                    match (left.get(key), right.get(key)) {
                        (Some(left), Some(right)) => {
                            if let Some(path) = difference(left, right, &path) {
                                return Some(path);
                            }
                        }
                        _ => {
                            return Some(format!(
                                "{path} (native field present={}, source field present={})",
                                left.contains_key(key),
                                right.contains_key(key)
                            ));
                        }
                    }
                }
                None
            }
            (Value::Array(left), Value::Array(right)) => {
                if left.len() != right.len() {
                    return Some(format!(
                        "{path}/length (native={}, source={})",
                        left.len(),
                        right.len()
                    ));
                }
                left.iter()
                    .zip(right)
                    .enumerate()
                    .find_map(|(index, (left, right))| {
                        difference(left, right, &format!("{path}/{index}"))
                    })
            }
            _ => Some(format!("{path} (native={left}, source={right})")),
        }
    }

    /// 외부 합성 자료는 source의 실제 후보 열거와 apply switch를 사용한다.
    /// 표식만 검사하지 않고 root dispatch 이후 상태 전체·RNG·private history를
    /// 비교한다. 자료가 없으면 이 검사를 source 검증 성공으로 보고하지 않는다.
    #[test]
    fn frozen_move_continuations_when_receipts_are_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_MOVE_CONTINUATION_CASES") else {
            return;
        };
        let lines = std::fs::read_to_string(path).unwrap();
        let mut families = BTreeSet::new();
        let mut checked = 0;
        let mut failures = Vec::new();
        for line in lines.lines() {
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            let action: Action =
                serde_json::from_value(receipt["sourceAction"]["payload"].clone()).unwrap();
            assert!(matches!(
                action.kind,
                ActionKind::Move | ActionKind::PromotionChoice
            ));
            families.insert(receipt["fixture"]["family"].as_str().unwrap().to_owned());
            let mut state: GameState =
                serde_json::from_value(receipt["sourcePosition"]["state"].clone()).unwrap();
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(receipt["sourcePosition"]["rng"].clone()).unwrap();
            let before = state.clone();
            let result = crate::transition::apply_without_public_event(&mut state, &action);
            let expected_ok = receipt["sourceDirectResult"]["ok"] == true;
            if result.is_ok() != expected_ok {
                failures.push(format!(
                    "{} source ok={expected_ok}, native={result:?}",
                    receipt["id"]
                ));
                continue;
            }
            if result.is_err() && state != before {
                failures.push(format!(
                    "{} rejected continuation changed state or RNG",
                    receipt["id"]
                ));
            }
            let mut actual = serde_json::to_value(&state).unwrap();
            for envelope in ["rulesetId", "rng", "history"] {
                actual.as_object_mut().unwrap().remove(envelope);
            }
            let expected = &receipt["sourceDirectResult"]["position"];
            if let Some(path) = difference(&actual, &expected["state"], "state") {
                failures.push(format!(
                    "{} continuation state diverged at {path}",
                    receipt["id"]
                ));
            }
            if serde_json::to_value(&state.rng).unwrap() != expected["rng"] {
                failures.push(format!("{} continuation RNG diverged", receipt["id"]));
            }
            if state.history != before.history
                || expected["history"] != receipt["sourcePosition"]["history"]
            {
                failures.push(format!(
                    "{} no-event continuation boundary changed history",
                    receipt["id"]
                ));
            }
            checked += 1;
        }
        assert!(checked > 0, "move continuation source receipt is empty");
        assert_eq!(
            families,
            BTreeSet::from(
                [
                    "file-surge",
                    "rook-lift",
                    "thief",
                    "reposition",
                    "frenzy",
                    "iron-monarch",
                    "platform",
                    "checker",
                    "mad-horse",
                    "desperado",
                    "backward",
                    "promotion",
                    "hp-stationary",
                ]
                .map(str::to_owned)
            )
        );
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
