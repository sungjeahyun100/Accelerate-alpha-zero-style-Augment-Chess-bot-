//! 동결 v7 일반 이동의 착지 후 기물 형태·trait 정산.
//!
//! Source main91007–91034, 92246–92287, 92356–92398, 92501–92503. 착지 승급은
//! BeforeLandingPromotion과 AfterLandingPromotion 사이에서 전이 소유자가
//! 실행한다. 양자화·포획·기보·폭발·승급 선택·턴 정산은 이 모듈의 책임이 아니다.
//! 콜백은 전이가 소유한 사본에서 실행하며, 오류는 최종 상태 반영 전에 전파한다.
//! main-OahWs0tU.js / SHA-256
//! e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c.

use crate::v7_move_continuations::V7MoveContinuationSnapshot;
use crate::{Color, EngineError, GameState, Piece, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};

/// capture/placement 소유자가 확정한 source 지역 변수. 포획 성공 여부는
/// 포획 목록 길이로 재계산하지 않으며, 피해 기물은 직접·jump·siege 순서를 유지한다.
pub(crate) struct V7MovePieceEffectsContext<'a> {
    pub(crate) from: Square,
    pub(crate) landing: Square,
    pub(crate) start: &'a V7MoveContinuationSnapshot,
    pub(crate) direct_captured_piece: Option<&'a Piece>,
    pub(crate) jump_captured_piece: Option<&'a Piece>,
    pub(crate) siege_ram_chameleon_victim: Option<&'a Piece>,
    pub(crate) captured_something: bool,
    pub(crate) slime_move: bool,
    pub(crate) switcheroo_move: bool,
    pub(crate) portal_entry: Option<Square>,
    pub(crate) portal_exit: Option<Square>,
    pub(crate) privacy: Option<&'a Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum V7PlacementPhase {
    /// noteRollerArrival 직후. atomic 착지 승급보다 먼저 실행한다.
    BeforeLandingPromotion,
    /// atomic 착지 승급 직후. setLastMove/기보보다 먼저 실행한다.
    AfterLandingPromotion,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct V7PlacementEffectsOutcome {
    pub(crate) transformed_into_crown: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct V7SurvivingMoveEffectsOutcome {
    /// source의 원래 chargeRush 폰 이동 여부. 표식 제거 뒤 LastSprint에서 사용한다.
    pub(crate) was_charge_rush_pawn_move: bool,
    /// source 추가 이동 분기가 사용하는 변신 결과. 일반 포획 여부와는 다르다.
    pub(crate) chameleon_transformed: bool,
}

fn boundary(
    state: &GameState,
    moving: &Piece,
    context: Option<&V7MovePieceEffectsContext<'_>>,
) -> Result<Color> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 moved-piece effects require the pinned ruleset".into(),
        ));
    }
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::InvalidState(
            "v7 moved-piece effects require the frozen 8x8 board".into(),
        ));
    }
    if moving.id.is_empty() {
        return Err(EngineError::InvalidState(
            "v7 moved-piece effects require a stable mover identity".into(),
        ));
    }
    let owner = moving.color.owner().ok_or_else(|| {
        EngineError::InvalidState("v7 moved-piece effects require a player-owned mover".into())
    })?;
    if let Some(context) = context {
        if [context.from, context.landing]
            .iter()
            .any(|at| at.row >= 8 || at.col >= 8)
        {
            return Err(EngineError::InvalidState(
                "v7 moved-piece effects contain a square outside the frozen board".into(),
            ));
        }
        if state
            .at(context.landing)
            .is_none_or(|piece| piece.id != moving.id)
        {
            return Err(EngineError::InvalidState(
                "v7 moved-piece effects require the mover at its actual landing".into(),
            ));
        }
    }
    // 공통 catalog 경계는 확인되지 않은 source profile을 명시적으로 거절한다.
    crate::v7_queued_effects::uses_september18_balance(state)?;
    Ok(owner)
}

fn side_truth(state: &GameState, field: &str, owner: Color) -> bool {
    crate::observation::truth(
        state
            .extra
            .get(field)
            .and_then(|value| value.get(owner.as_str())),
    )
}

fn side_write(state: &mut GameState, field: &str, owner: Color, value: Value) -> Result<()> {
    state
        .extra
        .get_mut(field)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            EngineError::InvalidState(format!(
                "v7 moved-piece effects: {field} must be a color map",
            ))
        })?
        .insert(owner.as_str().into(), value);
    Ok(())
}

fn initialize_falsy_map(state: &mut GameState, field: &str, initial: Value) {
    if !crate::observation::truth(state.extra.get(field)) {
        state.extra.insert(field.into(), initial);
    }
}

fn square_name(at: Square) -> String {
    format!("{}{}", char::from(b'a' + at.col), 8 - at.row)
}

fn publish(state: &mut GameState, piece: &Piece) {
    crate::v7_board_hazards::replace_object_aliases(state, piece);
}

fn refresh_alias(state: &GameState, piece: &mut Piece) {
    // JS 함수의 local Piece 참조는 보드와 포획 배열의 동일 객체를 가리킨다.
    // 공통 callback이 그 객체를 수정했으면 같은 ID의 최신 값으로 이어간다.
    if let Some(current) = state
        .board
        .iter()
        .flatten()
        .flatten()
        .chain(state.captures.white.iter())
        .chain(state.captures.black.iter())
        .find(|current| current.id == piece.id)
    {
        *piece = current.clone();
    }
}

fn consume_resolve_move(
    state: &mut GameState,
    moving: &Piece,
    moved_as_type: &str,
) -> Result<bool> {
    let owner = moving.color.owner().ok_or(EngineError::WrongActor)?;
    if moved_as_type != "pawn"
        || !side_truth(state, "resolve", owner)
        || !side_truth(state, "resolveReady", owner)
    {
        return Ok(false);
    }
    side_write(state, "resolveReady", owner, json!(false))?;
    initialize_falsy_map(state, "resolveSpentTurn", json!({}));
    let spent_turn = *state.turns_taken.get(owner);
    side_write(state, "resolveSpentTurn", owner, json!(spent_turn))?;
    initialize_falsy_map(
        state,
        "resolveMoveCredit",
        json!({"white":false,"black":false}),
    );
    side_write(state, "resolveMoveCredit", owner, json!(true))?;
    if crate::v7_queued_effects::uses_september26_rebalance(state)? {
        initialize_falsy_map(state, "resolveCreditPieceId", json!({}));
        side_write(state, "resolveCreditPieceId", owner, json!(moving.id))?;
    }
    Ok(true)
}

/// Source disassembleMovedQueen/main20847와 internalLocalCallbacks/main115218.
fn disassemble_moved_queen(
    state: &mut GameState,
    moving: &mut Piece,
    context: &V7MovePieceEffectsContext<'_>,
) -> Result<bool> {
    let owner = moving.color.owner().ok_or(EngineError::WrongActor)?;
    if context.start.moved_as_type != "queen" || moving.kind != "queen"
        || !side_truth(state, "disassembly", owner)
        // main67854/67857의 기본 source=state 두 predicate는 동일하다.
        || crate::v7_board_hazards::source_royal_identity(state, moving)?
        || context.from == context.landing
        || state.at(context.landing).is_none_or(|piece| piece.id != moving.id)
    {
        return Ok(false);
    }
    moving.kind = "bishop".into();
    moving.moved = true;
    publish(state, moving);
    // 분해의 create는 일반 배치와 달리 붕괴·블랙홀·왕관 ground도 차단한다.
    if state.at(context.from).is_none()
        && !crate::movement::collapsed(state, context.from)
        && !crate::v7_board_hazards::black_hole_cells(state)?.contains(&context.from)
        && !crate::v7_queued_effects::crown_ground_at(state, context.from)
        && crate::movement::open_placement(state, context.from, Some(owner))?
    {
        let mut rook = crate::opening::spawn(state, owner, "rook")?;
        rook.moved = true;
        rook.extra
            .insert("origin".into(), json!(square_name(context.from)));
        state.board[usize::from(context.from.row)][usize::from(context.from.col)] = Some(rook);
    }
    Ok(true)
}

fn spawn_slime_child(
    state: &mut GameState,
    moving: &Piece,
    context: &V7MovePieceEffectsContext<'_>,
) -> Result<()> {
    if context.start.moved_ability_type != "slime"
        || !context.slime_move
        || state.at(context.from).is_some()
        || !crate::movement::d4_destination_allowed(state, moving.color, &[context.from])
    {
        return Ok(());
    }
    let timestamp = crate::draft::frozen_timestamp_for_ruleset(&state.ruleset_id)?;
    let suffix: String =
        crate::draft::random_suffix(state.rng.sample_opaque("source slime child identity")?)?
            .chars()
            .take(6)
            .collect();
    // Slime는 piece() factory가 아니라 이 축약 객체를 생성하므로 shielded도 없다.
    let mut spawned = Piece::new("slime", moving.color, format!("slime-{timestamp}-{suffix}"));
    spawned.moved = true;
    spawned.source_order = ["id", "type", "color", "moved", "origin"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    spawned
        .extra
        .insert("origin".into(), json!(square_name(context.from)));
    crate::transition::mark_card_no_capture(state, &mut spawned)?;
    state.board[usize::from(context.from.row)][usize::from(context.from.col)] =
        Some(spawned.clone());
    crate::card_effects::mark_animation(state, &spawned)?;
    Ok(())
}

fn queen_identity(piece: &Piece) -> bool {
    piece.kind == "queen" && piece.extra.get("regencyHeir") != Some(&Value::Bool(true))
}

fn spawn_queen_afterimage(
    state: &mut GameState,
    moving: &Piece,
    context: &V7MovePieceEffectsContext<'_>,
) -> Result<()> {
    let Some(captured) = context.direct_captured_piece else {
        return Ok(());
    };
    let owner = moving.color.owner().ok_or(EngineError::WrongActor)?;
    if !queen_identity(moving)
        || !queen_identity(captured)
        || captured.color == moving.color
        || !side_truth(state, "afterimageQueen", owner)
        || !crate::movement::open_placement(state, context.from, Some(owner))?
    {
        return Ok(());
    }
    let mut afterimage = crate::opening::spawn(state, owner, "queen")?;
    afterimage.moved = true;
    afterimage
        .extra
        .insert("origin".into(), json!(square_name(context.from)));
    afterimage.extra.insert(
        "freshNoCaptureUntil".into(),
        json!(state.turns_taken.get(owner).checked_add(1).ok_or_else(|| {
            EngineError::InvalidState("v7 queen afterimage capture lock overflow".into())
        })?),
    );
    state.board[usize::from(context.from.row)][usize::from(context.from.col)] =
        Some(afterimage.clone());
    crate::card_effects::mark_animation(state, &afterimage)?;
    crate::replay::add_piece_action_log(
        state,
        moving,
        Some(context.landing),
        context.privacy,
        format!(
            "잔상: {}에 퀸의 잔상이 소환되었습니다.",
            square_name(context.from)
        ),
    )?;
    Ok(())
}

fn leave_recruiter_pawn(
    state: &mut GameState,
    moving: &Piece,
    context: &V7MovePieceEffectsContext<'_>,
) -> Result<()> {
    if context.start.moved_ability_type != "recruiter" {
        return Ok(());
    }
    let owner = moving.color.owner().ok_or(EngineError::WrongActor)?;
    if !crate::movement::open_placement(state, context.from, Some(owner))? {
        return Ok(());
    }
    let mut pawn = crate::opening::spawn(state, owner, "pawn")?;
    pawn.extra
        .insert("origin".into(), json!(square_name(context.from)));
    if crate::observation::truth(state.extra.get("monochromeChess")) {
        pawn.extra.insert(
            "monoShade".into(),
            json!(if (context.from.row + context.from.col).is_multiple_of(2) {
                "light"
            } else {
                "dark"
            }),
        );
    }
    // Source는 moved=false인 factory 폰을 둔다. fresh/card 포획 제한도 부여하지 않는다.
    state.board[usize::from(context.from.row)][usize::from(context.from.col)] = Some(pawn.clone());
    crate::card_effects::mark_animation(state, &pawn)?;
    crate::replay::add_piece_action_log(
        state,
        moving,
        Some(context.landing),
        context.privacy,
        format!("징집관: {}에 폰을 소환했습니다.", square_name(context.from)),
    )?;
    Ok(())
}

/// main92246–92287. Roller 도착 기록 후 첫 phase, atomic 착지 승급 후
/// 두 번째 phase를 호출한다. 이미 terminal인 포획도 source의 이 단계는 실행한다.
pub(crate) fn after_placement_before_last_move(
    state: &mut GameState,
    moving: &mut Piece,
    context: &V7MovePieceEffectsContext<'_>,
    phase: V7PlacementPhase,
) -> Result<V7PlacementEffectsOutcome> {
    crate::legal_profile::measure("move_after_placement", || {
        after_placement_before_last_move_profiled(state, moving, context, phase)
    })
}

pub(crate) fn after_placement_before_last_move_profiled(
    state: &mut GameState,
    moving: &mut Piece,
    context: &V7MovePieceEffectsContext<'_>,
    phase: V7PlacementPhase,
) -> Result<V7PlacementEffectsOutcome> {
    let owner = boundary(state, moving, Some(context))?;
    let mut outcome = V7PlacementEffectsOutcome::default();
    match phase {
        V7PlacementPhase::BeforeLandingPromotion => {
            if crate::observation::truth(moving.extra.get("locustOrigin")) {
                moving.extra.insert("locustUsed".into(), json!(true));
            }
            crate::v7_move_execution::note_thief_move_v7(
                state,
                moving,
                context.from,
                context.landing,
                &[context.portal_entry, context.portal_exit],
            )?;
            publish(state, moving);
            disassemble_moved_queen(state, moving, context)?;
            if state.free_move_resolution != Some(owner) {
                consume_resolve_move(state, moving, &context.start.moved_as_type)?;
            }
            spawn_slime_child(state, moving, context)?;
        }
        V7PlacementPhase::AfterLandingPromotion => {
            publish(state, moving);
            if crate::observation::truth(state.extra.get("crownRule")) {
                crate::v7_board_automata::reconcile_crown_rule(state, true)?;
                refresh_alias(state, moving);
            }
            outcome.transformed_into_crown =
                context.start.moved_as_type != "crown" && moving.kind == "crown";
            if context.start.moved_as_type == "pawn" {
                crate::v7_rule_bombs::mark_deathmatch_progress(state)?;
            }
            spawn_queen_afterimage(state, moving, context)?;
            if context.switcheroo_move && crate::observation::truth(state.extra.get("switcheroo")) {
                side_write(state, "switcheroo", owner, json!(false))?;
            }
            leave_recruiter_pawn(state, moving, context)?;
        }
    }
    publish(state, moving);
    Ok(outcome)
}

fn facing_from_delta(from: Square, to: Square) -> Option<&'static str> {
    let dr = i16::from(to.row) - i16::from(from.row);
    let dc = i16::from(to.col) - i16::from(from.col);
    if dr.abs() >= dc.abs() && dr < 0 {
        Some("up")
    } else if dr.abs() >= dc.abs() && dr > 0 {
        Some("down")
    } else if dc.abs() > dr.abs() && dc < 0 {
        Some("left")
    } else if dc.abs() > dr.abs() && dc > 0 {
        Some("right")
    } else {
        None
    }
}

fn crown_checker_if_needed(
    state: &mut GameState,
    moving: &mut Piece,
    context: &V7MovePieceEffectsContext<'_>,
) -> Result<bool> {
    let owner = moving.color.owner().ok_or(EngineError::WrongActor)?;
    if moving.ability_kind() != "checker" || context.landing.row != owner.promotion_row() {
        return Ok(false);
    }
    moving.kind = "checkerKing".into();
    moving.extra.shift_remove("tricksterMoveType");
    moving.extra.shift_remove("tricksterPreviousAbilityForTurn");
    moving.moved = true;
    crate::card_effects::mark_transformed_origin(state, moving, context.landing)?;
    publish(state, moving);
    crate::card_effects::mark_animation(state, moving)?;
    crate::replay::add_piece_action_log(
        state,
        moving,
        Some(context.landing),
        context.privacy,
        format!(
            "{}의 체커가 킹 체커가 되었습니다.",
            square_name(context.landing)
        ),
    )?;
    Ok(true)
}

fn monochrome_type(state: &GameState, kind: &str) -> String {
    if kind == "knight" && crate::observation::truth(state.extra.get("monochromeChess")) {
        "camel".into()
    } else {
        kind.into()
    }
}

/// main91007–91034/92500. 착지 승급과 Transcendence capture upgrade 뒤,
/// Transcendence가 실행되지 않은 경우에 전이 소유자가 호출한다. 원문은 AI
/// 조사에서도 가중 선택 난수·animation·log를 실행하며 기보는 변경하지 않는다.
pub(crate) fn transform_chimera_after_move_v7(
    state: &mut GameState,
    moving: &mut Piece,
    landing: Square,
    privacy: Option<&Value>,
) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(
            "v7 Chimera transformation requires the pinned ruleset".into(),
        ));
    }
    // Source의 local 객체가 포획·제거되거나 landing이 바뀌었으면 동작하지 않는다.
    // 이 guard보다 앞에서 profile 검사, RNG, 기물 필드 정리를 실행하지 않는다.
    if !crate::observation::truth(moving.extra.get("chimera"))
        || state.at(landing).is_none_or(|piece| piece.id != moving.id)
    {
        return Ok(());
    }
    boundary(state, moving, None)?;

    let was_queen = moving.kind == "queen";
    let source_types: &[&str] = if was_queen {
        &["pawn", "knight", "bishop", "rook"]
    } else {
        &["pawn", "knight", "bishop", "rook", "queen"]
    };
    let resolved_current_type = monochrome_type(state, &moving.kind);
    let options: Vec<_> = source_types
        .iter()
        .map(|kind| monochrome_type(state, kind))
        .filter(|kind| *kind != resolved_current_type)
        .map(|kind| {
            let weight = if was_queen {
                25.0
            } else if kind == "queen" {
                10.0
            } else {
                30.0
            };
            (kind, weight)
        })
        .collect();
    let weights: Vec<_> = options.iter().map(|(_, weight)| *weight).collect();
    let selected = crate::transition::sample_weighted(state, &weights)?;
    let next_type = options[selected].0.clone();

    const TYPE_STATE: &[&str] = &[
        "windmillMode",
        "logDir",
        "logRollAfterTurn",
        "mana",
        "maxMana",
        "ammo",
        "maxAmmo",
        "facing",
    ];
    for field in TYPE_STATE {
        moving.extra.shift_remove(*field);
    }
    moving
        .source_order
        .retain(|field| !TYPE_STATE.contains(&field.as_str()));
    moving.kind = next_type;
    moving.moved = true;
    if !moving.source_order.iter().any(|field| field == "moved") {
        moving.source_order.push("moved".into());
    }
    moving.extra.insert("shielded".into(), json!(false));
    if !moving.source_order.iter().any(|field| field == "shielded") {
        moving.source_order.push("shielded".into());
    }
    if crate::observation::truth(state.extra.get("monochromeChess")) {
        moving.extra.insert(
            "monoShade".into(),
            json!(if (landing.row + landing.col).is_multiple_of(2) {
                "light"
            } else {
                "dark"
            }),
        );
        if !moving.source_order.iter().any(|field| field == "monoShade") {
            moving.source_order.push("monoShade".into());
        }
    }
    publish(state, moving);
    crate::card_effects::mark_animation(state, moving)?;
    crate::replay::add_piece_action_log(
        state,
        moving,
        Some(landing),
        privacy,
        format!(
            "키메라: {}의 기물이 {}으로 변신했습니다.",
            square_name(landing),
            crate::replay::source_piece_label(&moving.kind).unwrap_or("undefined"),
        ),
    )?;
    Ok(())
}

fn same_set_primitive(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.as_f64() == right.as_f64(),
        (Value::Array(_) | Value::Object(_), _) | (_, Value::Array(_) | Value::Object(_)) => false,
        _ => left == right,
    }
}

/// main1339: 표식을 삭제하고 shielded를 부여한다. 배열의 다른 효과는 source
/// Set 순서대로 남긴다. 객체 원소는 JSON 복원 시 각각의 객체이므로 병합하지 않는다.
pub(crate) fn resolve_witch_trial_capture(
    moving: &mut Piece,
    captured_something: bool,
) -> Result<bool> {
    if !captured_something || !crate::observation::truth(moving.extra.get("witchTrial")) {
        return Ok(false);
    }
    moving.extra.shift_remove("witchTrial");
    moving.extra.insert("shielded".into(), json!(true));
    let Some(effects) = moving.extra.get("potionEffects") else {
        return Ok(true);
    };
    let contains = match effects {
        Value::Array(values) => values
            .iter()
            .any(|value| value.as_str() == Some("witchTrial")),
        Value::String(value) => value.contains("witchTrial"),
        Value::Null => false,
        _ => {
            return Err(EngineError::InvalidState(
                "v7 resolveWitchTrialCapture: potionEffects has no source includes method".into(),
            ));
        }
    };
    if !contains {
        return Ok(true);
    }
    let values = effects.as_array().ok_or_else(|| {
        EngineError::InvalidState(
            "v7 resolveWitchTrialCapture: potionEffects includes witchTrial but is not an array"
                .into(),
        )
    })?;
    let mut rewritten = Vec::<Value>::new();
    for value in values {
        let value = if value.as_str() == Some("witchTrial") {
            json!("shield")
        } else {
            value.clone()
        };
        if !rewritten
            .iter()
            .any(|previous| same_set_primitive(previous, &value))
        {
            rewritten.push(value);
        }
    }
    moving
        .extra
        .insert("potionEffects".into(), json!(rewritten));
    Ok(true)
}

/// main92356–92398. 호출자는 bomb/feudal/Trojan 뒤 생존과 source terminal
/// 반환 조건을 확인한다. 일반 이동 로그·enPassant·폭발보다 앞에서 실행한다.
pub(crate) fn after_surviving_move(
    state: &mut GameState,
    moving: &mut Piece,
    context: &V7MovePieceEffectsContext<'_>,
) -> Result<V7SurvivingMoveEffectsOutcome> {
    crate::legal_profile::measure("move_after_surviving", || {
        after_surviving_move_profiled(state, moving, context)
    })
}

pub(crate) fn after_surviving_move_profiled(
    state: &mut GameState,
    moving: &mut Piece,
    context: &V7MovePieceEffectsContext<'_>,
) -> Result<V7SurvivingMoveEffectsOutcome> {
    let owner = boundary(state, moving, Some(context))?;
    if moving.kind == "shotgunKing" {
        let facing = facing_from_delta(context.from, context.landing)
            .map(|value| json!(value))
            .or_else(|| {
                moving
                    .extra
                    .get("facing")
                    .filter(|value| crate::observation::truth(Some(value)))
                    .cloned()
            })
            .unwrap_or_else(|| json!(if owner == Color::White { "up" } else { "down" }));
        moving.extra.insert("facing".into(), facing);
    }
    let was_charge_rush_pawn_move = context.start.moved_as_type == "pawn"
        && crate::observation::truth(moving.extra.get("chargeRush"));
    moving.moved = true;
    if (crate::v7_queued_effects::uses_september18_balance(state)? || moving.kind != "merchant")
        && crate::v7_board_hazards::source_royal_identity(state, moving)?
        && side_truth(state, "zugzwang", owner)
    {
        side_write(state, "zugzwang", owner, json!(false))?;
    }
    publish(state, moving);
    crate::replay::track_moving(state, moving)?;
    refresh_alias(state, moving);
    if was_charge_rush_pawn_move {
        moving.extra.shift_remove("chargeRush");
    }
    crown_checker_if_needed(state, moving, context)?;
    crate::v7_move_continuations::clear_consumed_extra_move_flags(moving, context.start);
    crate::card_effects::note_ultimatum_movement(state, moving)?;
    let internal_six = crate::v7_move_execution::uses_internal_six_fixes(state);
    if internal_six
        && context.start.was_file_surge_second_move
        && crate::observation::truth(moving.extra.get("twinBondId"))
    {
        moving.extra.insert("twinSwapPending".into(), json!(1));
    }
    if internal_six {
        moving.extra.shift_remove("promotionRushUntil");
    }
    if context.start.moved_ability_type == "herald" {
        moving
            .extra
            .insert("heraldJumpUnlocked".into(), json!(true));
    }
    if context.captured_something
        && context.start.moved_ability_type == "squire"
        && !crate::v7_promotion::should_promote_v7(state, moving, context.landing)?
    {
        moving.kind = monochrome_type(state, "knight");
        moving.extra.shift_remove("tricksterMoveType");
        moving.extra.shift_remove("tricksterPreviousAbilityForTurn");
        crate::card_effects::mark_transformed_origin(state, moving, context.landing)?;
        publish(state, moving);
        crate::replay::add_piece_action_log(
            state,
            moving,
            Some(context.landing),
            context.privacy,
            format!(
                "{}의 종자가 {}가 되었습니다.",
                square_name(context.landing),
                crate::replay::source_piece_label(&moving.kind).unwrap_or("undefined")
            ),
        )?;
    }
    if context.start.moved_ability_type == "windmill" {
        let mode = if moving.extra.get("windmillMode").and_then(Value::as_str) == Some("rook") {
            "bishop"
        } else {
            "rook"
        };
        moving.extra.insert("windmillMode".into(), json!(mode));
    }
    if context.start.moved_as_type == "trickster" && moving.kind == "trickster" {
        moving.extra.insert(
            "tricksterPreviousAbilityForTurn".into(),
            json!(context.start.moved_ability_type),
        );
        crate::card_effects::reroll_trickster_ability(state, moving)?;
    }
    moving.extra.shift_remove("londonSystemPawn");
    let victim = context
        .siege_ram_chameleon_victim
        .or(context.direct_captured_piece)
        .or(if internal_six {
            context.jump_captured_piece
        } else {
            None
        });
    let chameleon_transformed = if let Some(victim) = victim {
        crate::observation::truth(moving.extra.get("chameleon"))
            && !crate::v7_board_hazards::source_royal_identity(state, victim)?
            && !["wall", "colossus", "bigRook", "bigBishop"].contains(&victim.kind.as_str())
    } else {
        false
    };
    if let Some(victim) = victim.filter(|_| chameleon_transformed) {
        if crate::v7_board_hazards::source_royal_king(state, moving)? {
            moving.extra.insert("crownRoyal".into(), json!(true));
        }
        moving.kind = monochrome_type(state, &victim.kind);
        if victim.kind == "windmill" {
            moving.extra.insert(
                "windmillMode".into(),
                victim
                    .extra
                    .get("windmillMode")
                    .filter(|value| crate::observation::truth(Some(value)))
                    .cloned()
                    .unwrap_or_else(|| json!("bishop")),
            );
        } else {
            moving.extra.shift_remove("windmillMode");
        }
        crate::card_effects::mark_transformed_origin(state, moving, context.landing)?;
    }
    if resolve_witch_trial_capture(moving, context.captured_something)? {
        publish(state, moving);
        crate::replay::add_piece_action_log(
            state,
            moving,
            Some(context.landing),
            context.privacy,
            format!(
                "마녀재판: {}의 표식이 사라지고 가호를 얻었습니다.",
                crate::replay::source_piece_label(&moving.kind).unwrap_or("undefined")
            ),
        )?;
    }
    if crate::observation::truth(moving.extra.get("assemblyPromotionPending")) {
        moving.extra.shift_remove("assemblyPromotionPending");
        moving.kind = "queen".into();
        moving.moved = true;
        moving.extra.shift_remove("windmillMode");
        crate::card_effects::mark_transformed_origin(state, moving, context.landing)?;
    }
    moving.extra.insert(
        "coolGuyCapturedLast".into(),
        json!(context.captured_something),
    );
    publish(state, moving);
    Ok(V7SurvivingMoveEffectsOutcome {
        was_charge_rush_pawn_move,
        chameleon_transformed,
    })
}

/// main1857/92501. 승급 보호는 승급 API가 처리하며, 그 외 형태 변화만
/// Queen's Gambit 임시 보호를 해제한다. 실제 이전 종류는 이동 시작 snapshot이다.
pub(crate) fn reconcile_after_type_change(
    state: &mut GameState,
    moving: &mut Piece,
    previous_type: &str,
    promoted_by_landing: bool,
) -> Result<bool> {
    boundary(state, moving, None)?;
    if !crate::observation::truth(moving.extra.get("queensGambitProtection"))
        || moving.kind == previous_type
        || promoted_by_landing
    {
        return Ok(false);
    }
    crate::v7_card_context::clear_queens_gambit_protection_after_transformation(moving);
    publish(state, moving);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;

    fn initial() -> GameState {
        crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..Default::default()
            },
            19,
        )
        .unwrap()
    }

    fn context<'a>(start: &'a V7MoveContinuationSnapshot) -> V7MovePieceEffectsContext<'a> {
        V7MovePieceEffectsContext {
            from: Square { row: 6, col: 0 },
            landing: Square { row: 5, col: 0 },
            start,
            direct_captured_piece: None,
            jump_captured_piece: None,
            siege_ram_chameleon_victim: None,
            captured_something: false,
            slime_move: false,
            switcheroo_move: false,
            portal_entry: None,
            portal_exit: None,
            privacy: None,
        }
    }

    #[test]
    fn wrong_landing_rejects_before_mover_state_rng_or_history_changes() {
        let mut state = initial();
        let mut moving = state.board[6][0].as_ref().unwrap().clone();
        moving
            .extra
            .insert("locustOrigin".into(), json!({"row":6,"col":0}));
        let start = crate::v7_move_continuations::capture_move_start(&mut moving);
        let before = serde_json::to_value(&state).unwrap();
        let mover_before = moving.clone();
        let error = after_placement_before_last_move(
            &mut state,
            &mut moving,
            &context(&start),
            V7PlacementPhase::BeforeLandingPromotion,
        )
        .unwrap_err();
        assert!(matches!(error, EngineError::InvalidState(_)));
        assert!(
            error
                .to_string()
                .contains("require the mover at its actual landing")
        );
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        assert_eq!(moving, mover_before);
    }

    #[test]
    fn unknown_profile_rejects_before_mover_state_rng_or_history_changes() {
        let mut state = initial();
        let mut moving = state.board[6][0].take().unwrap();
        moving
            .extra
            .insert("locustOrigin".into(), json!({"row":6,"col":0}));
        state.board[5][0] = Some(moving.clone());
        // draftDelete의 source 초기 상태는 cardState를 생략할 수 있다.
        // 존재하지 않는 Fields 키를 인덱싱하지 않고 거절 대상 profile을 구성한다.
        state.extra.insert(
            "cardState".into(),
            json!({
                "profile":{"catalogHash":"unrecognized-profile"},
            }),
        );
        let start = crate::v7_move_continuations::capture_move_start(&mut moving);
        let before = serde_json::to_value(&state).unwrap();
        let mover_before = moving.clone();
        let error = after_placement_before_last_move(
            &mut state,
            &mut moving,
            &context(&start),
            V7PlacementPhase::BeforeLandingPromotion,
        )
        .unwrap_err();
        assert!(matches!(error, EngineError::UnsupportedFeature(_)));
        assert!(
            error
                .to_string()
                .contains("unrecognized source catalog hash")
        );
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        assert_eq!(moving, mover_before);
    }

    #[test]
    fn chimera_inactive_or_removed_mover_preserves_rng_fields_and_history() {
        for removed in [false, true] {
            let mut state = initial();
            let mut moving = state.board[6][0].take().unwrap();
            moving.extra.insert("chimera".into(), json!(removed));
            moving.extra.insert("windmillMode".into(), json!("rook"));
            moving.extra.insert("shielded".into(), json!(true));
            let landing = Square { row: 5, col: 0 };
            if removed {
                state.captures.black.push(moving.clone());
            } else {
                state.board[5][0] = Some(moving.clone());
            }
            let before = serde_json::to_value(&state).unwrap();
            let mover_before = moving.clone();
            transform_chimera_after_move_v7(&mut state, &mut moving, landing, None).unwrap();
            assert_eq!(serde_json::to_value(&state).unwrap(), before);
            assert_eq!(moving, mover_before);
        }
    }

    fn source_body(state: &GameState) -> Value {
        let mut value = serde_json::to_value(state).unwrap();
        for field in ["rulesetId", "rng", "history"] {
            value.as_object_mut().unwrap().remove(field);
        }
        value
    }

    fn differences(actual: &Value, expected: &Value, path: &str, output: &mut Vec<String>) {
        if output.len() >= 12 {
            return;
        }
        match (actual, expected) {
            (Value::Object(actual), Value::Object(expected)) => {
                let keys: std::collections::BTreeSet<_> =
                    actual.keys().chain(expected.keys()).collect();
                for key in keys {
                    match (actual.get(key), expected.get(key)) {
                        (Some(actual), Some(expected)) => {
                            differences(actual, expected, &format!("{path}.{key}"), output)
                        }
                        (actual, expected) => output.push(format!(
                            "{path}.{key}: actual={actual:?}, expected={expected:?}"
                        )),
                    }
                    if output.len() >= 12 {
                        break;
                    }
                }
            }
            (Value::Array(actual), Value::Array(expected)) => {
                if actual.len() != expected.len() {
                    output.push(format!(
                        "{path}.length: actual={}, expected={}",
                        actual.len(),
                        expected.len()
                    ));
                }
                for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                    differences(actual, expected, &format!("{path}[{index}]"), output);
                    if output.len() >= 12 {
                        break;
                    }
                }
            }
            _ => {
                if serde_jcs::to_vec(actual).unwrap() != serde_jcs::to_vec(expected).unwrap() {
                    output.push(format!("{path}: actual={actual}, expected={expected}"));
                }
            }
        }
    }

    fn optional_piece(value: &Value) -> Option<Piece> {
        (!value.is_null()).then(|| serde_json::from_value(value.clone()).unwrap())
    }

    fn optional_square(value: &Value) -> Option<Square> {
        (!value.is_null()).then(|| serde_json::from_value(value.clone()).unwrap())
    }

    /// 이 검사는 main source의 착지/생존 블록과 Chimera callback을 비교한다. 일반 이동의 주변
    /// 포획·기보·폭발·턴 전체 순서는 전이 소유자의 integration 검증에 속한다.
    #[test]
    #[ignore = "메인이 생성한 ACCELERATE_V7_MOVE_PIECE_EFFECT_CASES source receipt 필요"]
    fn frozen_move_piece_effect_boundaries_match_full_state_rng_and_history() {
        let path = std::env::var_os("ACCELERATE_V7_MOVE_PIECE_EFFECT_CASES")
            .expect("main agent must provide the frozen move-piece effects JSONL receipt");
        let text =
            std::fs::read_to_string(path).expect("move-piece effects receipt must be readable");
        let mut checked = 0;
        let mut failures = Vec::new();
        let mut names = std::collections::BTreeSet::new();
        for (index, line) in text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .enumerate()
        {
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            assert_eq!(
                receipt["executionProfile"],
                "accelerate-headless-semantic-v7-faithful-init-v1"
            );
            assert_eq!(receipt["rulesVersion"], RULES_VERSION_V7);
            assert_eq!(
                receipt["fixtureKind"],
                "synthetic-contract-frozen-source-slice"
            );
            let name = receipt["name"].as_str().unwrap();
            assert!(
                names.insert(name.to_owned()),
                "duplicate source recipe name {name}"
            );
            let fixture = &receipt["fixture"];
            let from: Square = serde_json::from_value(fixture["from"].clone()).unwrap();
            let landing: Square = serde_json::from_value(fixture["landing"].clone()).unwrap();
            let mut state: GameState = serde_json::from_value(receipt["before"].clone()).unwrap();
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(receipt["rngBefore"].clone()).unwrap();
            state.history = receipt["historyBefore"].as_array().unwrap().clone();
            let mut mismatch = Vec::new();
            differences(
                &source_body(&state),
                &receipt["before"],
                "imported_state",
                &mut mismatch,
            );
            let mut moving = if fixture["moverLocation"] == "capture-black" {
                state.captures.black.last().unwrap().clone()
            } else {
                state.at(landing).unwrap().clone()
            };
            let free_move = fixture["freeMove"] == true;
            state.free_move_resolution = free_move.then_some(moving.color.owner().unwrap());
            let ai_depth = u32::from(fixture["aiSimulation"] == true);
            state.ai_simulation_depth = ai_depth;
            let stage = fixture["stage"].as_str().unwrap();
            let privacy = fixture.get("privacy").filter(|value| !value.is_null());
            let operation = if stage == "chimera" {
                transform_chimera_after_move_v7(&mut state, &mut moving, landing, privacy)
                    .map(|()| json!({}))
            } else {
                let start = crate::v7_move_continuations::capture_move_start(&mut moving);
                let direct = optional_piece(&fixture["direct"]);
                let jump = optional_piece(&fixture["jump"]);
                let siege = optional_piece(&fixture["siege"]);
                let context = V7MovePieceEffectsContext {
                    from,
                    landing,
                    start: &start,
                    direct_captured_piece: direct.as_ref(),
                    jump_captured_piece: jump.as_ref(),
                    siege_ram_chameleon_victim: siege.as_ref(),
                    captured_something: fixture["captured"] == true,
                    slime_move: fixture["slimeMove"] == true,
                    switcheroo_move: fixture["switcheroo"] == true,
                    portal_entry: optional_square(&fixture["portalEntry"]),
                    portal_exit: optional_square(&fixture["portalExit"]),
                    privacy,
                };
                let operation = match stage {
                    "before" | "after" => {
                        let phase = if stage == "before" {
                            V7PlacementPhase::BeforeLandingPromotion
                        } else {
                            V7PlacementPhase::AfterLandingPromotion
                        };
                        after_placement_before_last_move(&mut state, &mut moving, &context, phase)
                            .map(|outcome| json!({"transformed_into_crown":outcome.transformed_into_crown}))
                    }
                    "survive" => {
                        after_surviving_move(&mut state, &mut moving, &context).map(|outcome| {
                            json!({
                                "was_charge_rush_pawn_move":outcome.was_charge_rush_pawn_move,
                                "chameleon_transformed":outcome.chameleon_transformed,
                            })
                        })
                    }
                    other => panic!("unknown move-piece effects receipt stage {other}"),
                };
                if fixture["reconcile"] == true
                    && let Err(error) = reconcile_after_type_change(
                        &mut state,
                        &mut moving,
                        &start.moved_as_type,
                        false,
                    )
                {
                    mismatch.push(format!("reconcile: {error}"));
                }
                operation
            };
            match operation {
                Ok(result) => differences(&result, &receipt["returned"], "returned", &mut mismatch),
                Err(error) => mismatch.push(format!("operation: {error}")),
            }
            differences(
                &source_body(&state),
                &receipt["after"],
                "state",
                &mut mismatch,
            );
            differences(
                &json!(state.rng),
                &receipt["rngAfter"],
                "rng",
                &mut mismatch,
            );
            differences(
                &json!(state.history),
                &receipt["historyAfter"],
                "history",
                &mut mismatch,
            );
            assert_eq!(
                state.free_move_resolution,
                free_move.then_some(moving.color.owner().unwrap()),
                "{name}: execution-only free move context changed"
            );
            assert_eq!(
                state.ai_simulation_depth, ai_depth,
                "{name}: execution-only AI simulation context changed"
            );
            if !mismatch.is_empty() {
                failures.push(format!("receipt {index} {name}: {}", mismatch.join("; ")));
            }
            checked += 1;
        }
        assert_eq!(
            checked, 18,
            "move-piece effects receipt must contain 12 stage boundaries and 6 Chimera cases"
        );
        assert!(
            failures.is_empty(),
            "{} of {checked} move-piece effects receipts differ:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
