//! 동결 v7의 특수 이동 실행. 후보 기하는 movement owner가 제공한다.
//!
//! 통나무 방향 지정은 첫 이동 undo/replay capture보다 앞에, 샷건은 첫 이동
//! undo 뒤이지만 replay capture보다 앞에 반환한다. 일반 착지·승급·턴 진행은
//! transition owner가 처리하며, 이 모듈은 원문이 요청한 종료 방식만 반환한다.

use crate::observation::{number, truth};
use crate::v7_capture_reactions::{CaptureOptions, DefendedAttack};
use crate::{Color, EngineError, GameState, MoveTarget, Piece, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StationaryCompletion {
    /// endMove를 한 번 실행하고 UI 상태를 반환한다. 상태 문자열은 DTO 필드가 아니다.
    EndMove {
        actor: Color,
        title: &'static str,
        message: String,
    },
    /// main91808의 직접 grappler ghost는 endMove/history 기록 뒤 생성된다.
    EndGrapplerPull { actor: Color, visual_target: Piece },
    /// finishShotgunAction의 terminal branch는 endMove 없이 history를 기록한다.
    RecordTerminal,
    /// 원문의 관측 후 취소 또는 실행 전 gate. 일반 이동으로 이어지지 않는다.
    Return,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StationaryMoveOutcome {
    pub(crate) captures: Vec<Piece>,
    pub(crate) completion: StationaryCompletion,
    /// Swap branches call beginMoveReplayCapture. endMove commits this at its
    /// common source boundary; early/terminal branches leave it uncommitted.
    pub(crate) replay_before: Option<Box<GameState>>,
}

impl StationaryMoveOutcome {
    fn returned() -> Self {
        Self {
            captures: Vec::new(),
            completion: StationaryCompletion::Return,
            replay_before: None,
        }
    }

    fn end_move(actor: Color, title: &'static str, message: String, captures: Vec<Piece>) -> Self {
        Self {
            captures,
            completion: StationaryCompletion::EndMove {
                actor,
                title,
                message,
            },
            replay_before: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct RollerOrigin {
    id: String,
    from: Square,
    arrival: Option<Square>,
}

#[derive(Clone, Debug, PartialEq)]
struct RollerMoveContext {
    origins: Vec<RollerOrigin>,
    primary_id: String,
    target: MoveTarget,
    flushed: bool,
}

/// Source activeMetalMove is a nested wrapper-local object. The state owner
/// stores this as a private non-serialized field and restores the previous
/// context after every movePiece wrapper, including cancellation and errors.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct V7MoveExecutionContext {
    metal_before: Vec<(String, Square)>,
    initial_color: Color,
    roller: Option<RollerMoveContext>,
    medium_snapshot: Option<(String, Value)>,
}

pub(crate) fn capture_move_execution_context(
    state: &GameState,
    from: Square,
    target: &MoveTarget,
) -> Result<V7MoveExecutionContext> {
    if state.ruleset_id != RULES_VERSION_V7
        || state.board.len() != 8
        || state.board.iter().any(|row| row.len() != 8)
    {
        return Err(EngineError::InvalidState(
            "v7 move execution context requires the pinned 8x8 board".into(),
        ));
    }
    let mover = state.at(from);
    let mut metal_before = Vec::new();
    let mut seen = BTreeSet::new();
    let only_mover = crate::v7_threat::uses_september22_rules(state);
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            let Some(piece) = state.at(at) else {
                continue;
            };
            if !truth(piece.extra.get("metalized"))
                || only_mover && mover.is_none_or(|mover| mover.id != piece.id)
            {
                continue;
            }
            if piece.id.is_empty() {
                return Err(EngineError::InvalidState(
                    "v7 active metal context requires stable metalized identities".into(),
                ));
            }
            if seen.insert(piece.id.clone()) {
                metal_before.push((piece.id.clone(), at));
            }
        }
    }
    let roller = if let Some(mover) = mover.filter(|mover| state.flag("roller", mover.color)) {
        let mut origins = Vec::new();
        for row in 0..8 {
            for col in 0..8 {
                let at = Square { row, col };
                if let Some(piece) = state.at(at)
                    && piece.kind == "rook"
                    && piece.color == mover.color
                    && (piece.id == mover.id || target.flag("castle"))
                {
                    if piece.id.is_empty() {
                        return Err(EngineError::InvalidState(
                            "v7 Roller context requires stable rook identities".into(),
                        ));
                    }
                    origins.push(RollerOrigin {
                        id: piece.id.clone(),
                        from: at,
                        arrival: None,
                    });
                }
            }
        }
        if origins.is_empty() {
            None
        } else {
            Some(RollerMoveContext {
                origins,
                primary_id: mover.id.clone(),
                target: target.clone(),
                flushed: false,
            })
        }
    } else {
        None
    };
    let medium_snapshot = mover.filter(|mover| mover.kind == "medium").map(|mover| {
        (
            mover.id.clone(),
            crate::card_effects::current_base_movement(state, mover).unwrap_or(Value::Null),
        )
    });
    Ok(V7MoveExecutionContext {
        metal_before,
        initial_color: state.turn,
        roller,
        medium_snapshot,
    })
}

pub(crate) fn medium_move_snapshot(context: &V7MoveExecutionContext) -> Option<(&str, &Value)> {
    context
        .medium_snapshot
        .as_ref()
        .map(|(id, memory)| (id.as_str(), memory))
}

/// main15645 + 91451. Every call compares against the wrapper's original
/// positions, so the post-endMove sync can replace cooldown4 with cooldown3.
pub(crate) fn sync_move_execution_context(
    state: &mut GameState,
    context: &V7MoveExecutionContext,
) -> Result<()> {
    let completed = (state.turn != context.initial_color).then_some(context.initial_color);
    let mut seen = BTreeSet::new();
    let mut updates = Vec::new();
    for row in 0..8 {
        for col in 0..8 {
            let at = Square { row, col };
            let Some(piece) = state.at(at) else {
                continue;
            };
            if !seen.insert(piece.id.clone()) || !truth(piece.extra.get("metalized")) {
                continue;
            }
            if let Some((_, old)) = context.metal_before.iter().find(|(id, _)| id == &piece.id)
                && *old != at
            {
                let mut piece = piece.clone();
                piece.extra.insert(
                    "metalCooldown".into(),
                    json!(if piece.color.owner() == completed && completed.is_some() {
                        3
                    } else {
                        4
                    }),
                );
                updates.push(piece);
            }
        }
    }
    for piece in updates {
        crate::v7_board_hazards::replace_object_aliases(state, &piece);
    }
    Ok(())
}

/// main92246. Only ordinary actual placement records a direct arrival.
/// Other branches use finish's final board scan; cancelled moves add no road.
pub(crate) fn note_roller_arrival(context: &mut V7MoveExecutionContext, from: Square, to: Square) {
    let Some(roller) = context.roller.as_mut().filter(|roller| !roller.flushed) else {
        return;
    };
    if from == to {
        return;
    }
    if let Some(origin) = roller
        .origins
        .iter_mut()
        .find(|origin| origin.from == from && origin.arrival.is_none())
    {
        origin.arrival = Some(to);
    }
}

/// main4300 / endMove93294 / wrapper91468. Flushing is idempotent. Direct
/// arrivals survive a subsequent relocation or capture, while unspecified
/// arrivals use the last matching board cell exactly like source forEach.
pub(crate) fn finish_roller_context(
    state: &mut GameState,
    context: &mut V7MoveExecutionContext,
) -> Result<bool> {
    let mut next = state.clone();
    let mut active = context.clone();
    let Some(roller) = active.roller.as_mut().filter(|roller| !roller.flushed) else {
        return Ok(false);
    };
    roller.flushed = true;
    let mut changed = false;
    for origin in &roller.origins {
        let destination = origin.arrival.or_else(|| {
            (0..8)
                .flat_map(|row| (0..8).map(move |col| Square { row, col }))
                .rfind(|cell| next.at(*cell).is_some_and(|piece| piece.id == origin.id))
        });
        let Some(destination) = destination else {
            continue;
        };
        let empty = MoveTarget::at(destination);
        let target = if origin.id == roller.primary_id {
            &roller.target
        } else {
            &empty
        };
        let added = crate::movement::v7_roller_travel_cells(origin.from, destination, target)?;
        if !added.is_empty() {
            let mut cells = crate::movement::v7_normalized_highway_cells(&next)?
                .into_iter()
                .collect::<BTreeSet<_>>();
            cells.extend(added);
            next.extra.insert(
                "highwayCells".into(),
                json!(cells.into_iter().collect::<Vec<_>>()),
            );
            changed = true;
        }
    }
    *state = next;
    *context = active;
    Ok(changed)
}

fn require_origin(state: &GameState, at: Square) -> Result<Piece> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::InvalidState(
            "v7 stationary execution requires the pinned ruleset".into(),
        ));
    }
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(EngineError::InvalidState(
            "v7 stationary execution requires an 8x8 board".into(),
        ));
    }
    let piece = state.at(at).cloned().ok_or(EngineError::IllegalAction)?;
    if piece.id.is_empty() {
        return Err(EngineError::InvalidState(
            "v7 stationary mover requires a stable piece identity".into(),
        ));
    }
    piece.color.owner().ok_or(EngineError::WrongActor)?;
    Ok(piece)
}

fn square_name(at: Square) -> String {
    format!("{}{}", char::from(b'a' + at.col), 8 - at.row)
}

fn notation_direction([dr, dc]: [i8; 2]) -> String {
    let vertical = if dr < 0 {
        "N"
    } else if dr > 0 {
        "S"
    } else {
        ""
    };
    let horizontal = if dc < 0 {
        "W"
    } else if dc > 0 {
        "E"
    } else {
        ""
    };
    let direction = format!("{vertical}{horizontal}");
    if direction.is_empty() {
        "·".into()
    } else {
        direction
    }
}

fn parse_direction(value: &Value, object: bool, field: &str) -> Result<[i8; 2]> {
    let coordinates = if object {
        [value.get("dr"), value.get("dc")]
    } else {
        [value.get(0), value.get(1)]
    };
    let component = |value: Option<&Value>| {
        value
            .and_then(Value::as_i64)
            .filter(|value| (-1..=1).contains(value))
            .map(|value| value as i8)
    };
    let direction = component(coordinates[0])
        .zip(component(coordinates[1]))
        .map(|(dr, dc)| [dr, dc])
        .filter(|direction| *direction != [0, 0]);
    direction.ok_or_else(|| {
        EngineError::InvalidState(format!(
            "v7 {field} requires integer deltas in -1..1 and a nonzero direction",
        ))
    })
}

fn privacy_snapshot(state: &GameState, piece: &Piece, at: Square) -> Result<Value> {
    let mut privacy = json!({});
    for viewer in [Color::White, Color::Black] {
        let visible = crate::observation::piece_visible_to_color_at_v7(state, piece, at, viewer)?;
        privacy[viewer.as_str()] = json!({
            "originVisible":visible,
            "typeKnown":visible || piece.color == viewer
                || piece.extra.get("hiddenFrom").and_then(Value::as_str) == Some(viewer.as_str()),
        });
    }
    Ok(privacy)
}

const DISPLAY_AND_SIX_FIXES_HASHES: &[&str] = &[
    "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4",
    "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI",
    "fREmSQ2Dm168z-Kbw2yQqvivoP-lJ-CxFGC5YFLTKZs",
    "kWXPFPmgOmKoI1zMkHzSLbhkx-2wxOhsNXGvDmO6KFc",
    "P9ZXnDcBx7WGbrutYve1S2Q5255dQug3pnmS8YQym-g",
    "HVzv4vxzNKDRi-ylshTw9HnLmIlyiQhMScQ4caG_aSI",
    "jkECTP8OBtMeXmZF7wmz1dmgw0mgTFn8jdD-d9LZhxU",
    "s0_j9SmUy1tSUcko_X3I32akB93Iz2bvV1Cn7Nw4Uoo",
    "_Hw8otJVztzWQyIN69bg5khIqcJv-au6UwAcOQhtKcM",
    "Vj80kM6RlfZvbMo9kevioRks2VbcDGk8bASi5uz0yNA",
    "MM-bvPG6PYiUQ0-UrCM3GmEbFfBMTyPxjKk0vFXbXj0",
    "abhSgcd4RVrr2b-bzPtaJo6gbqfUrYKk4IqsrW7oCO0",
    "jvr27l0Kk-YFld45zoyTV-MMOIAXuIXtubma6eKOPL4",
    "uIkywAndqkm8YawR1KMkyw_GROmXd4h1wpCXwH-0oYM",
    "RzvDge9_4q7il_D_pgjgO-vg2cu7Njx-VGcV_0ZeRLA",
    "Rq32ku1EGlC0GbWIZx5RTtxP43JwU2dz1dIrPWElXRM",
    "DxKZRNW24FynvEKBMzBDT7B1AH1e0BZFU-Ixpm1LtoM",
    "FMXnrO0g9t3Yd2TMDS2I9bbNBhbVEZaz7V93GzBAvOE",
    "2ltd5-M692bro1FgeSC0N8MnEEYR_QTEcxJwurJj2ME",
];
const THIEF_REMAKE_ONLY_HASHES: &[&str] = &[
    "WaZMsX0HrVmtxwAakiwEjDS56uQtQqztvwLnJrv-sHE",
    "IFEPd1kgPLE5sPYp8yeRI_45n3sVsZZ4h3Y0MLemuyg",
    "OcuYVKEgBuAf8Pj5oy22E_YKE_1TeNMYdK3wMRWhnKU",
    "kf6NclPPEjBozgM7tKI4l0uSWrN2xvEmU7LphuWuTOU",
];

fn source_catalog_hash(state: &GameState) -> Option<&str> {
    if let Some(cards) = state
        .extra
        .get("cardState")
        .filter(|value| truth(Some(value)))
    {
        cards
            .get("profile")
            .and_then(|value| value.get("catalogHash"))
    } else {
        state
            .extra
            .get("profile")
            .and_then(|value| value.get("catalogHash"))
    }
    .and_then(Value::as_str)
    .filter(|hash| !hash.is_empty())
}

pub(crate) fn uses_internal_six_fixes(state: &GameState) -> bool {
    source_catalog_hash(state).map_or_else(
        || state.extra.get("internalSixFixes") != Some(&json!(false)),
        |hash| DISPLAY_AND_SIX_FIXES_HASHES.contains(&hash),
    )
}

pub(crate) fn uses_thief_remake_v7(state: &GameState) -> bool {
    source_catalog_hash(state).map_or_else(
        || state.extra.get("thiefRemake") != Some(&json!(false)),
        |hash| {
            DISPLAY_AND_SIX_FIXES_HASHES.contains(&hash) || THIEF_REMAKE_ONLY_HASHES.contains(&hash)
        },
    )
}

/// main15847. Under the remake every moved object records visited squares;
/// this is not restricted to the current Thief ability. End-turn owns reset.
pub(crate) fn note_thief_move_v7(
    state: &GameState,
    piece: &mut Piece,
    from: Square,
    to: Square,
    extra_cells: &[Option<Square>],
) -> Result<()> {
    if !uses_thief_remake_v7(state) {
        if (piece.kind == "thief"
            || piece.extra.get("tricksterMoveType").and_then(Value::as_str) == Some("thief"))
            && !piece
                .extra
                .get("thiefTurnsLeft")
                .and_then(Value::as_f64)
                .is_some_and(|value| value.is_finite() && value.fract() == 0.0)
        {
            piece.extra.insert("thiefTurnsLeft".into(), json!(2));
        }
        return Ok(());
    }
    let mut visited = Vec::new();
    let mut seen = BTreeSet::new();
    if let Some(previous) = piece
        .extra
        .get("thiefVisited")
        .filter(|value| truth(Some(value)))
    {
        for value in previous.as_array().ok_or_else(|| {
            EngineError::InvalidState(
                "v7 moved-piece thiefVisited must be an ordered square string array".into(),
            )
        })? {
            let value = value.as_str().ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 moved-piece thiefVisited contains a non-string square".into(),
                )
            })?;
            if seen.insert(value.to_owned()) {
                visited.push(json!(value));
            }
        }
    }
    for at in [Some(from), Some(to)].iter().chain(extra_cells).flatten() {
        let value = format!("{},{}", at.row, at.col);
        if seen.insert(value.clone()) {
            visited.push(json!(value));
        }
    }
    piece.extra.insert("thiefVisited".into(), json!(visited));
    let direction_to = extra_cells.first().copied().flatten().unwrap_or(to);
    let dr = (i16::from(direction_to.row) - i16::from(from.row)).signum();
    let dc = (i16::from(direction_to.col) - i16::from(from.col)).signum();
    if dr != 0 || dc != 0 {
        piece
            .extra
            .insert("thiefLastDirection".into(), json!(format!("{dr},{dc}")));
    }
    Ok(())
}

fn refresh_alias(state: &GameState, piece: &mut Piece) {
    // Explosive/feudal/reaper reactions can move or remove the shooter. JS
    // retains the same object, including a captured alias after removal.
    if let Some(updated) = state
        .board
        .iter()
        .flatten()
        .flatten()
        .find(|item| item.id == piece.id)
        .or_else(|| {
            state
                .captures
                .white
                .iter()
                .chain(&state.captures.black)
                .find(|item| item.id == piece.id)
        })
    {
        *piece = updated.clone();
    }
}

fn ammo(piece: &Piece) -> Result<f64> {
    let value = piece.extra.get("ammo").filter(|value| !value.is_null());
    if value.is_none() {
        return Ok(0.0);
    }
    number(value)
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            EngineError::InvalidState(
                "v7 shotgun ammo cannot be represented as a finite source number".into(),
            )
        })
}

/// main91541. This callback does not begin a replay capture, move the log,
/// advance movement memory, or mark capture progress. endMove belongs to root.
pub(crate) fn execute_log_direction(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
) -> Result<Option<StationaryMoveOutcome>> {
    let Some(direction) = target
        .flags
        .get("setLogDirection")
        .filter(|value| truth(Some(value)))
    else {
        return Ok(None);
    };
    let direction = parse_direction(direction, true, "setLogDirection")?;
    let mut next = state.clone();
    let mut piece = require_origin(&next, from)?;
    let actor = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let Some(_) = from.offset(direction[0], direction[1]) else {
        next.extra.insert("selected".into(), Value::Null);
        next.extra.insert("legalMoves".into(), json!([]));
        *state = next;
        return Ok(Some(StationaryMoveOutcome::returned()));
    };
    let privacy = privacy_snapshot(&next, &piece, from)?;
    piece.extra.insert(
        "logDir".into(),
        json!({"dr":direction[0],"dc":direction[1]}),
    );
    piece.extra.insert(
        "logRollAfterTurn".into(),
        json!(u64::from(*next.turns_taken.get(actor)) + 1),
    );
    piece.moved = true;
    piece
        .extra
        .insert("coolGuyCapturedLast".into(), json!(false));
    crate::v7_board_hazards::replace_object_aliases(&mut next, &piece);
    crate::card_effects::mark_animation(&mut next, &piece)?;
    next.en_passant = None;
    let notation_direction = notation_direction(direction);
    crate::replay::queue_special_move_notation(
        &mut next,
        &piece,
        from,
        format!(
            "{}⇢{notation_direction}",
            crate::replay::piece_code(RULES_VERSION_V7, &piece.kind)
        ),
        format!(
            "{} 통나무 방향 {notation_direction}",
            crate::replay::label(actor)
        ),
        crate::replay::MoveNotationVisibility {
            privacy: &privacy,
            capture: false,
            capture_known_squares: &json!({}),
        },
    )?;
    crate::replay::add_piece_action_log(
        &mut next,
        &piece,
        Some(from),
        Some(&privacy),
        format!("{}의 통나무 방향을 지정했습니다.", square_name(from)),
    )?;
    let viewer = actor.opponent();
    let hidden = privacy[viewer.as_str()]["originVisible"] == json!(false)
        || !crate::observation::piece_visible_to_color_at_v7(&next, &piece, from, viewer)?;
    let hidden_from = if hidden { viewer.as_str() } else { "" };
    let sound = if actor == Color::White {
        "moveSelf"
    } else {
        "moveOpponent"
    };
    next.extra.insert("lastMove".into(), Value::Null);
    next.extra.insert("accelerationTrail".into(), Value::Null);
    next.extra.insert("lastMove".into(), json!({
        "from":from,"to":from,"pieceId":piece.id,"pieceType":piece.kind,
        "soundName":sound,"soundColor":actor,"hiddenFrom":hidden_from,
        "idolEncoreEligible":false,"idolEncoreId":"","idolEncorePieceId":"","idolEncoreConsumed":false,
    }));
    // Source calls playSound directly: no playMoveSound/check-probe callback.
    *state = next;
    Ok(Some(StationaryMoveOutcome::end_move(
        actor,
        "통나무 방향 지정",
        String::new(),
        Vec::new(),
    )))
}

fn facing_delta(piece: &Piece, actor: Color) -> [i8; 2] {
    match piece
        .extra
        .get("facing")
        .and_then(Value::as_str)
        .filter(|facing| !facing.is_empty())
        .unwrap_or(if actor == Color::White { "up" } else { "down" })
    {
        "down" => [1, 0],
        "left" => [0, -1],
        "right" => [0, 1],
        _ => [-1, 0],
    }
}

fn facing_name([dr, dc]: [i8; 2]) -> Option<&'static str> {
    if dr < 0 {
        Some("up")
    } else if dr > 0 {
        Some("down")
    } else if dc < 0 {
        Some("left")
    } else if dc > 0 {
        Some("right")
    } else {
        None
    }
}

fn target_allowed(state: &GameState, shooter: &Piece, victim: &Piece) -> bool {
    victim.id != shooter.id
        && victim.kind != "wall"
        && !crate::movement::frozen(victim)
        && !crate::v7_board_hazards::indirect_attack_immune(state, victim)
}

fn capture_known_squares(entries: &[(Piece, Square)]) -> Value {
    let mut known = json!({});
    for color in [Color::White, Color::Black] {
        let mut seen = BTreeSet::new();
        let cells = entries
            .iter()
            .filter(|(piece, _)| piece.color == color)
            .map(|(_, at)| *at)
            .filter(|at| seen.insert(*at))
            .collect::<Vec<_>>();
        if cells.len() == 1 {
            known[color.as_str()] = json!(cells[0]);
        }
    }
    known
}

fn attack_target(
    state: &mut GameState,
    shooter: &mut Piece,
    at: Square,
    source: &str,
    options: &CaptureOptions,
) -> Result<Option<Piece>> {
    let Some(victim) = state.at(at).cloned() else {
        return Ok(None);
    };
    // Source protected precedes encouragement; HP follows encouragement.
    if crate::v7_capture_reactions::is_protected_piece(state, &victim, shooter) {
        crate::v7_capture_reactions::attack_defended_piece(state, shooter, at, source, options)?;
        return Ok(None);
    }
    if crate::movement::v7_encouraged_at(state, &victim, at) {
        return Ok(None);
    }
    match crate::v7_capture_reactions::attack_defended_piece(state, shooter, at, source, options)? {
        DefendedAttack::Health { removed } => Ok(removed),
        DefendedAttack::NotDefended => {
            let removed = crate::v7_capture_reactions::capture_at(state, shooter, at, options)?;
            if removed
                .as_ref()
                .is_some_and(|captured| truth(captured.extra.get("explosive")))
            {
                crate::v7_board_hazards::explode_at(state, at, "자폭병")?;
                refresh_alias(state, shooter);
            }
            Ok(removed)
        }
        DefendedAttack::Protected | DefendedAttack::ShieldBroken => Ok(None),
    }
}

/// main91599-91675. Public move selection observes its clicked square before
/// firstMoveUndo and before the fire callback takes a second privacy snapshot.
/// None means ready (or not a shotgun); an outcome means source early return.
pub(crate) fn prepare_shotgun_selection(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
) -> Result<Option<StationaryMoveOutcome>> {
    if !target.flag("shotgunBlast") && !target.flag("shotgunSnipe") {
        return Ok(None);
    }
    let mut next = state.clone();
    let mut shooter = require_origin(&next, from)?;
    let actor = shooter.color.owner().ok_or(EngineError::WrongActor)?;
    let at = target.square();
    let own_quantum = crate::v7_quantum_state::find_quantum_at(&next, at)?
        .is_some_and(|quantum| quantum.piece.id == shooter.id);
    if own_quantum {
        shooter.extra.shift_remove("quantum");
        shooter.source_order.retain(|field| field != "quantum");
        crate::v7_board_hazards::replace_object_aliases(&mut next, &shooter);
    } else {
        crate::v7_quantum_state::observe_quantum_at_in_place(&mut next, at, Some(actor))?;
    }
    if next
        .at(at)
        .is_some_and(|victim| victim.id != shooter.id && victim.color == shooter.color)
    {
        cancel_selection(&mut next)?;
        *state = next;
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    shooter.extra.shift_remove("thiefSecondMove");
    shooter
        .source_order
        .retain(|field| field != "thiefSecondMove");
    crate::v7_board_hazards::replace_object_aliases(&mut next, &shooter);
    *state = next;
    Ok(None)
}

/// main108970-109125. Fire callbacks are stationary and do not create ordinary
/// move notation or beginMoveReplayCapture. Caller stores firstMoveUndo first.
pub(crate) fn execute_shotgun(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
    threat_probe: bool,
) -> Result<Option<StationaryMoveOutcome>> {
    let blast = target.flag("shotgunBlast");
    let snipe = target.flag("shotgunSnipe");
    if !blast && !snipe {
        return Ok(None);
    }
    if blast && snipe {
        return Err(EngineError::InvalidState(
            "v7 shotgun action selects two attack modes".into(),
        ));
    }
    let mut next = state.clone();
    let mut shooter = require_origin(&next, from)?;
    let actor = shooter.color.owner().ok_or(EngineError::WrongActor)?;
    if shooter.kind != "shotgunKing" {
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    let cost = if blast { 2.0 } else { 3.0 };
    let locked = crate::v7_capture_reactions::saturation_locked(&next, &shooter);
    if ammo(&shooter)? < cost || locked {
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    let privacy = privacy_snapshot(&next, &shooter, from)?;
    let options = CaptureOptions {
        saturation_locked: Some(locked),
        threat_probe,
        ..Default::default()
    };
    let mut entries = Vec::new();
    let mut captured_royal = false;
    let supplied_direction = target
        .flags
        .get("shotgunDirection")
        .filter(|value| !value.is_null())
        .map(|value| parse_direction(value, false, "shotgunDirection"))
        .transpose()?;
    if blast {
        let direction = supplied_direction.unwrap_or_else(|| facing_delta(&shooter, actor));
        let sector = crate::v7_special_piece_moves::shotgun_blast_cells(from, direction);
        let mut has_target = false;
        for at in &sector {
            let victim = if let Some(piece) = next.at(*at) {
                Some(piece.clone())
            } else {
                crate::v7_quantum_state::find_quantum_at(&next, *at)?.map(|item| item.piece)
            };
            if victim
                .as_ref()
                .is_some_and(|victim| target_allowed(&next, &shooter, victim))
            {
                has_target = true;
            }
        }
        if has_target && crate::v7_threat::manner_capture_locked(&next, &shooter) {
            return Ok(Some(StationaryMoveOutcome::returned()));
        }
        let mut seen = BTreeSet::new();
        for at in sector {
            crate::v7_quantum_state::observe_quantum_at_in_place(&mut next, at, Some(actor))?;
            refresh_alias(&next, &mut shooter);
            let Some(victim) = next.at(at).cloned() else {
                continue;
            };
            if !target_allowed(&next, &shooter, &victim)
                || crate::movement::v7_overwhelm_capture_blocked(&next, &shooter, &victim)
                || !seen.insert(victim.id.clone())
            {
                continue;
            }
            if let Some(captured) = attack_target(&mut next, &mut shooter, at, "샷건", &options)?
            {
                captured_royal |= crate::v7_board_hazards::source_royal_king(&next, &captured)?;
                entries.push((captured, at));
            }
        }
    } else {
        let at = target.square();
        crate::v7_quantum_state::observe_quantum_at_in_place(&mut next, at, Some(actor))?;
        refresh_alias(&next, &mut shooter);
        let Some(victim) = next.at(at).cloned() else {
            *state = next;
            return Ok(Some(StationaryMoveOutcome::returned()));
        };
        if victim.color == shooter.color
            || !target_allowed(&next, &shooter, &victim)
            || crate::movement::v7_overwhelm_capture_blocked(&next, &shooter, &victim)
            || crate::v7_threat::manner_capture_locked(&next, &shooter)
        {
            *state = next;
            return Ok(Some(StationaryMoveOutcome::returned()));
        }
        if let Some(captured) = attack_target(&mut next, &mut shooter, at, "저격", &options)? {
            captured_royal = crate::v7_board_hazards::source_royal_king(&next, &victim)?;
            entries.push((captured, at));
        }
    }
    refresh_alias(&next, &mut shooter);
    shooter.moved = true;
    crate::card_effects::mark_animation(&mut next, &shooter)?;
    shooter
        .extra
        .insert("ammo".into(), json!((ammo(&shooter)? - cost).max(0.0)));
    if blast {
        if let Some(facing) = supplied_direction.and_then(facing_name) {
            shooter.extra.insert("facing".into(), json!(facing));
        } else if !shooter
            .extra
            .get("facing")
            .is_some_and(|value| truth(Some(value)))
        {
            shooter.extra.insert(
                "facing".into(),
                json!(if actor == Color::White { "up" } else { "down" }),
            );
        }
    }
    shooter
        .extra
        .insert("coolGuyCapturedLast".into(), json!(!entries.is_empty()));
    crate::v7_board_hazards::replace_object_aliases(&mut next, &shooter);
    next.en_passant = None;
    let removed = entries.len();
    let code = crate::replay::piece_code(RULES_VERSION_V7, &shooter.kind);
    let ending = if next.mode == "gameover" && captured_royal {
        "#"
    } else {
        ""
    };
    let (text, description, message, title) = if blast {
        let direction = notation_direction(supplied_direction.unwrap_or([0, 0]));
        (
            format!(
                "{code}⇢{direction}{}{ending}",
                if removed > 0 {
                    format!("×{removed}")
                } else {
                    String::new()
                }
            ),
            format!(
                "{} 샷건 킹 {direction} 방향 공격, 제거 {removed}개",
                crate::replay::label(actor)
            ),
            format!(
                "{} 샷건 킹이 샷건을 발사했습니다.",
                crate::replay::label(actor)
            ),
            "샷건",
        )
    } else {
        let at = square_name(target.square());
        (
            format!("{code}⇢{at}{ending}"),
            format!("{} 샷건 킹이 {at} 저격", crate::replay::label(actor)),
            format!(
                "{} 샷건 킹이 {at}을 저격했습니다.",
                crate::replay::label(actor)
            ),
            "저격",
        )
    };
    crate::replay::queue_special_move_notation(
        &mut next,
        &shooter,
        from,
        text,
        description,
        crate::replay::MoveNotationVisibility {
            privacy: &privacy,
            capture: removed > 0,
            capture_known_squares: &capture_known_squares(&entries),
        },
    )?;
    crate::replay::add_piece_action_log(&mut next, &shooter, Some(from), Some(&privacy), message)?;
    // Both shotgun sounds use playSound directly, without an ordinary move
    // sound/check probe. Herald settlement happens before terminal/endMove.
    crate::v7_threat::resolve_herald_threats_v7_with_probe(&mut next, actor, threat_probe)?;
    let captures = entries.into_iter().map(|(piece, _)| piece).collect();
    let completion = if next.mode == "gameover" {
        StationaryCompletion::RecordTerminal
    } else {
        StationaryCompletion::EndMove {
            actor,
            title,
            message: if blast {
                format!("{removed}개의 기물을 제거했습니다.")
            } else if removed > 0 {
                "기물을 제거했습니다.".into()
            } else {
                "공격이 방어막에 막혔습니다.".into()
            },
        }
    };
    *state = next;
    Ok(Some(StationaryMoveOutcome {
        captures,
        completion,
        replay_before: None,
    }))
}

/// main91902-91950. The selection owner observes the clicked quantum square,
/// removes thiefSecondMove and stores firstMoveUndo before this callback.
/// Conversion deliberately bypasses direct-capture protection/HP/recurrence,
/// does not beginMoveReplayCapture, and remembers the converted destination.
pub(crate) fn execute_missionary_conversion(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
    privacy_before_observation: &Value,
    captured_quantum_illusion: bool,
    threat_probe: bool,
) -> Result<Option<StationaryMoveOutcome>> {
    if !target.flag("missionaryConvert") {
        return Ok(None);
    }
    let mut next = state.clone();
    let mut moving = require_origin(&next, from)?;
    let actor = moving.color.owner().ok_or(EngineError::WrongActor)?;
    let to = target.square();
    if captured_quantum_illusion && next.at(to).is_none() && moving.ability_kind() == "missionary" {
        next.extra.insert("selected".into(), Value::Null);
        next.extra.insert("legalMoves".into(), json!([]));
        crate::replay::add_log(
            &mut next,
            "양자 역학의 허상을 관측하여 선교하지 못했습니다. 차례는 유지됩니다.".into(),
        )?;
        crate::replay::record(&mut next, "quantum-observation")?;
        *state = next;
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    let Some(mut converted) = next.at(to).cloned() else {
        return Ok(Some(StationaryMoveOutcome::returned()));
    };
    if moving.ability_kind() != "missionary"
        || converted.color == moving.color
        || converted.color.owner().is_none()
        || crate::movement::desperado_royal_capture_blocked(&moving, &converted)
        || from.row.abs_diff(to.row) != 1
        || from.col.abs_diff(to.col) != 1
    {
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    let former = converted.clone();
    let former_color = converted.color.owner().ok_or(EngineError::WrongActor)?;
    converted.color = moving.color;
    let reset_hp = match converted.kind.as_str() {
        "colossus" => Some(3),
        "bigRook" | "bigBishop" | "big-rook" | "big-bishop" => Some(2),
        _ => None,
    };
    if let Some(hp) = reset_hp {
        converted.extra.insert("hp".into(), json!(hp));
        converted.extra.insert("maxHp".into(), json!(hp));
    }
    crate::card_effects::mark_transformed_origin_with_options(&next, &mut converted, to, true)?;
    converted.extra.shift_remove("freshNoCaptureUntil");
    converted
        .source_order
        .retain(|field| field != "freshNoCaptureUntil");
    converted
        .extra
        .insert("coolGuyCapturedLast".into(), json!(false));
    converted.moved = true;
    moving.moved = true;
    moving
        .extra
        .insert("coolGuyCapturedLast".into(), json!(false));
    crate::v7_board_hazards::replace_object_aliases(&mut next, &converted);
    crate::v7_board_hazards::replace_object_aliases(&mut next, &moving);
    crate::replay::track_moving_with_exhaustion(&mut next, &moving, false)?;
    refresh_alias(&next, &mut moving);
    crate::card_effects::mark_animation(&mut next, &moving)?;
    crate::card_effects::mark_animation(&mut next, &converted)?;
    next.en_passant = None;
    let viewer = actor.opponent();
    let hidden_from = if privacy_before_observation[viewer.as_str()]["originVisible"]
        == json!(false)
        || !crate::observation::piece_visible_to_color_at_v7(&next, &moving, from, viewer)?
    {
        viewer.as_str()
    } else {
        ""
    };
    crate::card_effects::set_last_move(&mut next, from, to, "capture", actor, hidden_from, None)?;
    crate::v7_threat::play_move_sound_v7(&mut next, "capture", actor)?;
    let destination = square_name(to);
    crate::replay::queue_special_history_notation(
        &mut next,
        actor,
        "special",
        &format!("선교 {destination}"),
        &format!(
            "{} 선교사가 {destination}의 {}을 전향시켰습니다.",
            crate::replay::label(actor),
            crate::replay::source_piece_label(&converted.kind).unwrap_or(&converted.kind)
        ),
    )?;
    crate::replay::add_log(
        &mut next,
        format!(
            "{}의 선교사가 {destination}의 {} 기물을 아군으로 전향시켰습니다.",
            square_name(from),
            crate::replay::label(former_color)
        ),
    )?;
    crate::v7_threat::mark_king_threat_removal_cause(
        &mut next,
        &former,
        to,
        &json!({"attacker":moving,"origin":from,"label":"선교"}),
        threat_probe,
    )?;
    crate::transition::resolve_royal_capture(&mut next, &former, actor)?;
    let outcome = if next.mode == "gameover" {
        StationaryMoveOutcome::returned()
    } else {
        StationaryMoveOutcome::end_move(actor, "선교", String::new(), Vec::new())
    };
    *state = next;
    Ok(Some(outcome))
}

pub(crate) fn hidden_from_for_move(
    state: &GameState,
    piece: &Piece,
    to: Square,
    privacy: &Value,
) -> Result<&'static str> {
    let viewer = piece
        .color
        .owner()
        .ok_or(EngineError::WrongActor)?
        .opponent();
    if privacy[viewer.as_str()]["originVisible"] == json!(false)
        || !crate::observation::piece_visible_to_color_at_v7(state, piece, to, viewer)?
    {
        Ok(viewer.as_str())
    } else {
        Ok("")
    }
}

fn swap_move_log(
    state: &GameState,
    piece: &Piece,
    from: Square,
    to: Square,
    privacy: &Value,
) -> Result<String> {
    let actor = piece.color.owner().ok_or(EngineError::WrongActor)?;
    let setup = state
        .extra
        .get("campaign")
        .and_then(|value| value.get("setup"))
        .and_then(Value::as_str);
    let fog = matches!(setup, Some("fogWar" | "fog"))
        || truth(state.extra.get("fogWar"))
        || truth(state.extra.get("fogOfWar"))
        || truth(
            state
                .extra
                .get("fog")
                .and_then(|value| value.get("enabled")),
        );
    // The pinned local profile has no browser-global madAiOption("fog").
    if fog {
        return Ok(format!(
            "{} 기물이 이동했습니다.",
            crate::replay::label(actor)
        ));
    }
    if privacy[actor.opponent().as_str()]["originVisible"] == json!(false)
        || truth(piece.extra.get("hiddenFrom"))
        || crate::observation::piece_hidden_from_v7(state, piece, to).is_some()
    {
        return Ok("기물이 움직였습니다.".into());
    }
    Ok(format!(
        "{} {}: {} -> {}",
        crate::replay::label(actor),
        crate::replay::source_piece_label(&piece.kind).unwrap_or("undefined"),
        square_name(from),
        square_name(to)
    ))
}

fn large_anchor(piece: &Piece) -> Result<Square> {
    let row = piece
        .extra
        .get("anchorRow")
        .and_then(Value::as_u64)
        .filter(|row| *row < 7);
    let col = piece
        .extra
        .get("anchorCol")
        .and_then(Value::as_u64)
        .filter(|col| *col < 7);
    row.zip(col)
        .map(|(row, col)| Square {
            row: row as u8,
            col: col as u8,
        })
        .ok_or_else(|| {
            EngineError::InvalidState(format!(
                "v7 swap {} requires a complete in-bounds 2x2 anchor",
                piece.kind
            ))
        })
}

fn place_large_swap(state: &mut GameState, piece: &mut Piece, anchor: Square) {
    piece.extra.insert("anchorRow".into(), json!(anchor.row));
    piece.extra.insert("anchorCol".into(), json!(anchor.col));
    for row in anchor.row..=anchor.row + 1 {
        for col in anchor.col..=anchor.col + 1 {
            state.board[usize::from(row)][usize::from(col)] = Some(piece.clone());
        }
    }
}

fn disassemble_swapped_queen(state: &GameState, piece: &mut Piece, original: &str) -> Result<()> {
    // Every swap leaves its partner in the origin, so source's optional rook
    // creation cannot run. Ordinary empty-origin disassembly belongs to root.
    if original == "queen"
        && piece.kind == "queen"
        && state.flag("disassembly", piece.color)
        && !crate::v7_board_hazards::source_royal_identity(state, piece)?
    {
        piece.kind = "bishop".into();
        piece.moved = true;
    }
    Ok(())
}

fn can_resolve_dragon_swap(
    state: &GameState,
    moving: &Piece,
    other: &Piece,
    target: &MoveTarget,
) -> bool {
    let ability = moving.ability_kind();
    let imperial = target.flag("dragonSwap")
        && state.flag("imperialStudies", moving.color)
        && crate::movement::v7_king_augment_recipient(state, moving)
        && moving
            .extra
            .get("imperialMoves")
            .and_then(Value::as_array)
            .is_some_and(|moves| moves.iter().any(|kind| kind.as_str() == Some("dragon")));
    let copied = crate::v7_threat::uses_september22_rules(state)
        && ["parrot", "medium"].contains(&ability)
        && crate::card_effects::current_base_movement(state, moving)
            .and_then(|memory| memory.get("type").cloned())
            .as_ref()
            .and_then(Value::as_str)
            == Some("dragon");
    moving.color == other.color && !other.is_large() && (ability == "dragon" || imperial || copied)
}

/// main91802/91846/108829/108914. Geometry admits the exact descriptor;
/// this executor swaps source objects and supplies the common endMove replay
/// capture. It never uses ordinary capture/landing/promotion or notation.
#[cfg(test)]
pub(crate) fn execute_position_swap(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
    threat_probe: bool,
) -> Result<Option<StationaryMoveOutcome>> {
    execute_position_swap_with_privacy(state, from, target, threat_probe, None)
}

pub(crate) fn execute_position_swap_with_privacy(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
    threat_probe: bool,
    privacy_before_observation: Option<&Value>,
) -> Result<Option<StationaryMoveOutcome>> {
    let dragon = target.flag("dragonSwap");
    let substitution = target.flag("substitutionSwap");
    let relay = target.flag("relaySwap");
    if !dragon && !substitution && !relay {
        return Ok(None);
    }
    if [dragon, substitution, relay]
        .into_iter()
        .filter(|flag| *flag)
        .count()
        != 1
    {
        return Err(EngineError::InvalidState(
            "v7 swap descriptor selects more than one execution mode".into(),
        ));
    }
    let mut next = state.clone();
    let mut moving = require_origin(&next, from)?;
    let actor = moving.color.owner().ok_or(EngineError::WrongActor)?;
    let Some(mut other) = next.at(target.square()).cloned() else {
        return Ok(Some(StationaryMoveOutcome::returned()));
    };
    if other.id.is_empty() {
        return Err(EngineError::InvalidState(
            "v7 swap partner requires a stable piece identity".into(),
        ));
    }
    let solidarity =
        relay && crate::v7_rule_geometry::v7_solidarity_pair_allowed(&next, &moving, &other);
    let allowed = if dragon {
        can_resolve_dragon_swap(&next, &moving, &other, target)
            && (!truth(next.extra.get("monochromeChess"))
                || moving.kind == "wall"
                || (from.row + from.col) % 2 == (target.row + target.col) % 2)
            && crate::movement::fianchetto_destination_allowed(
                &next,
                &moving,
                from,
                &[target.square()],
            )
            && crate::movement::fianchetto_destination_allowed(
                &next,
                &other,
                target.square(),
                &[from],
            )
    } else if substitution {
        crate::movement::v7_can_substitute_pieces(
            &next,
            &moving,
            &other,
            next.flag("substitution", moving.color),
        )
    } else {
        (solidarity
            || next.flag("relay", moving.color)
                && (from.row == target.row || from.col == target.col))
            && other.color == moving.color
            && other.id != moving.id
            && !moving.is_large()
            && !other.is_large()
    };
    // Relay validates before beginMoveReplayCapture; dragon/substitution begin
    // at their dispatch boundary before their nested validation function.
    let before = if relay && !allowed {
        None
    } else {
        Some(crate::replay::begin_move(&mut next, actor)?)
    };
    if !allowed {
        if substitution {
            next.extra.insert("selected".into(), Value::Null);
            next.extra.insert("legalMoves".into(), json!([]));
        }
        *state = next;
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    let privacy = if relay {
        privacy_before_observation
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| privacy_snapshot(&next, &moving, from))?
    } else {
        privacy_snapshot(&next, &moving, from)?
    };
    let original_type = moving.kind.clone();
    let original_ability = moving.ability_kind().to_owned();
    let was_desperado = truth(moving.extra.get("desperado"));
    let quantum_candidates = if dragon && next.flag("quantumPending", moving.color) {
        let legal = crate::movement::v7_legal_move_targets(
            &next,
            &moving,
            from,
            crate::movement::V7MoveOptions::default(),
        )?;
        crate::v7_quantum_state::candidate_moves_for_move(&next, &moving, target.square(), &legal)?
    } else {
        Vec::new()
    };
    let (from, to) = if substitution && moving.is_large() && other.is_large() {
        let source_anchor = large_anchor(&moving)?;
        let target_anchor = large_anchor(&other)?;
        for cell in next.board.iter_mut().flatten() {
            if cell
                .as_ref()
                .is_some_and(|piece| piece.id == moving.id || piece.id == other.id)
            {
                *cell = None;
            }
        }
        place_large_swap(&mut next, &mut other, source_anchor);
        place_large_swap(&mut next, &mut moving, target_anchor);
        (source_anchor, target_anchor)
    } else {
        next.board[usize::from(from.row)][usize::from(from.col)] = Some(other.clone());
        next.board[usize::from(target.row)][usize::from(target.col)] = Some(moving.clone());
        (from, target.square())
    };
    if truth(next.extra.get("monochromeChess")) {
        other.extra.insert(
            "monoShade".into(),
            json!(if (from.row + from.col).is_multiple_of(2) {
                "light"
            } else {
                "dark"
            }),
        );
        moving.extra.insert(
            "monoShade".into(),
            json!(if (to.row + to.col).is_multiple_of(2) {
                "light"
            } else {
                "dark"
            }),
        );
    }
    if substitution {
        if next.flag("quantumPending", moving.color) {
            next.set_flag("quantumPending", actor, false);
        }
        moving.extra.shift_remove("quantum");
        moving.source_order.retain(|field| field != "quantum");
        other.extra.shift_remove("quantum");
        other.source_order.retain(|field| field != "quantum");
    }
    crate::v7_board_hazards::replace_object_aliases(&mut next, &moving);
    crate::v7_board_hazards::replace_object_aliases(&mut next, &other);
    let sound = if actor == Color::White {
        "moveSelf"
    } else {
        "moveOpponent"
    };
    if dragon || substitution {
        let hidden = hidden_from_for_move(&next, &moving, to, &privacy)?;
        let override_piece = (dragon || uses_internal_six_fixes(&next)).then_some(&moving);
        crate::card_effects::set_last_move(
            &mut next,
            from,
            to,
            sound,
            actor,
            hidden,
            override_piece,
        )?;
        let mut cells = vec![from, to];
        if dragon {
            crate::v7_quantum_state::clear_quantum_for_piece_in_place(&mut next, &mut moving)?;
            let quantum_to = crate::v7_quantum_state::apply_after_move_in_place(
                &mut next,
                &mut moving,
                to,
                &quantum_candidates,
            )?;
            if let Some(quantum_to) = quantum_to {
                next.extra
                    .get_mut("lastMove")
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| {
                        EngineError::InvalidState(
                            "v7 dragon swap lost lastMove before quantum highlight".into(),
                        )
                    })?
                    .insert("quantumTo".into(), json!(quantum_to));
                if !cells.contains(&quantum_to) {
                    cells.push(quantum_to);
                }
            }
            crate::v7_threat::play_move_sound_v7(&mut next, sound, actor)?;
        }
        crate::card_effects::track_acceleration_trail(&mut next, actor, &cells, false, hidden)?;
    }
    moving.moved = true;
    if substitution {
        moving
            .extra
            .insert("coolGuyCapturedLast".into(), json!(false));
    }
    if substitution || dragon || solidarity {
        other.moved = true;
    }
    if !dragon {
        disassemble_swapped_queen(&next, &mut moving, &original_type)?;
        let portal = if relay {
            [
                target.flags.get("portalEntry"),
                target.flags.get("portalExit"),
            ]
            .map(|value| {
                value
                    .filter(|value| !value.is_null())
                    .map(|value| {
                        serde_json::from_value(value.clone()).map_err(EngineError::serialization)
                    })
                    .transpose()
            })
            .into_iter()
            .collect::<Result<Vec<Option<Square>>>>()?
        } else {
            Vec::new()
        };
        note_thief_move_v7(&next, &mut moving, from, to, &portal)?;
        if uses_thief_remake_v7(&next) {
            note_thief_move_v7(&next, &mut other, to, from, &[])?;
        }
        if moving.kind == "trickster" {
            moving.extra.insert(
                "tricksterPreviousAbilityForTurn".into(),
                json!(original_ability),
            );
            crate::card_effects::reroll_trickster_ability(&mut next, &mut moving)?;
        }
    }
    crate::v7_board_hazards::replace_object_aliases(&mut next, &moving);
    crate::v7_board_hazards::replace_object_aliases(&mut next, &other);
    if !relay {
        crate::replay::track_moving(&mut next, &moving)?;
        refresh_alias(&next, &mut moving);
        refresh_alias(&next, &mut other);
        if dragon {
            moving
                .extra
                .insert("coolGuyCapturedLast".into(), json!(false));
        }
        crate::card_effects::note_ultimatum_movement(&mut next, &mut moving)?;
        crate::card_effects::note_ultimatum_movement(&mut next, &mut other)?;
        crate::v7_board_hazards::replace_object_aliases(&mut next, &moving);
        crate::v7_board_hazards::replace_object_aliases(&mut next, &other);
    }
    crate::card_effects::mark_animation(&mut next, &moving)?;
    crate::card_effects::mark_animation(&mut next, &other)?;
    next.en_passant = None;
    let title = if dragon {
        "드래곤 체인징"
    } else if substitution {
        "치환"
    } else if solidarity {
        "연대"
    } else {
        "교대"
    };
    if relay {
        crate::card_effects::set_last_move(&mut next, from, to, sound, actor, "", Some(&moving))?;
        crate::v7_threat::play_move_sound_v7(&mut next, sound, actor)?;
        crate::replay::queue_special_history_notation(
            &mut next,
            actor,
            "special",
            &format!("{title} {}", square_name(to)),
            &format!(
                "{} {title} {}↔{}",
                crate::replay::label(actor),
                square_name(from),
                square_name(to)
            ),
        )?;
        crate::replay::add_log(
            &mut next,
            format!(
                "{title}: {}와 {}의 아군 기물이 위치를 바꿨습니다.",
                square_name(from),
                square_name(to)
            ),
        )?;
    } else {
        let ending = if dragon && next.mode == "gameover" {
            "#"
        } else {
            ""
        };
        let code = crate::replay::piece_code(RULES_VERSION_V7, &moving.kind);
        let description = if dragon {
            format!(
                "{} 드래곤이 {}의 {}과 교환",
                crate::replay::label(actor),
                square_name(to),
                crate::replay::source_piece_label(&other.kind).unwrap_or(&other.kind)
            )
        } else {
            format!(
                "{} {}이 {}의 같은 기물과 치환",
                crate::replay::label(actor),
                crate::replay::source_piece_label(&moving.kind).unwrap_or(&moving.kind),
                square_name(to)
            )
        };
        crate::replay::queue_special_move_notation(
            &mut next,
            &moving,
            to,
            format!("{code}↔{}{ending}", square_name(to)),
            description,
            crate::replay::MoveNotationVisibility {
                privacy: &privacy,
                capture: false,
                capture_known_squares: &json!({}),
            },
        )?;
        let log = swap_move_log(&next, &moving, from, to, &privacy)?;
        crate::replay::add_log(&mut next, log)?;
        if substitution {
            crate::v7_threat::play_move_sound_v7(&mut next, sound, actor)?;
        }
        if dragon && moving.kind == "trickster" {
            moving
                .extra
                .insert("tricksterPreviousAbilityForTurn".into(), json!("dragon"));
            crate::card_effects::reroll_trickster_ability(&mut next, &mut moving)?;
            crate::v7_board_hazards::replace_object_aliases(&mut next, &moving);
        }
    }
    let mut outcome = StationaryMoveOutcome::end_move(actor, title, String::new(), Vec::new());
    if !relay {
        crate::v7_threat::resolve_herald_threats_v7_with_probe(&mut next, actor, threat_probe)?;
        if substitution {
            crate::v7_threat::check_racing_kings_v7(&mut next)?;
            crate::v7_capture_objectives::check_campaign_objectives(&mut next)?;
            if next.mode == "gameover" {
                outcome.completion = StationaryCompletion::RecordTerminal;
            }
        } else if next.mode == "gameover" {
            outcome.completion = StationaryCompletion::Return;
        } else if crate::v7_threat::check_racing_kings_v7(&mut next)? {
            outcome.completion = StationaryCompletion::RecordTerminal;
        }
        if was_desperado && next.mode != "gameover" {
            use crate::v7_move_continuations::V7ContinuationControl;
            match crate::v7_move_continuations::advance_desperado_step_v7(
                &mut next,
                &mut moving,
                to,
            )? {
                V7ContinuationControl::Retained | V7ContinuationControl::Terminal => {
                    outcome.completion = StationaryCompletion::Return
                }
                V7ContinuationControl::Continue | V7ContinuationControl::FinishMove(_) => {}
            }
        }
    }
    if matches!(outcome.completion, StationaryCompletion::EndMove { .. }) {
        outcome.replay_before = before.map(Box::new);
    }
    *state = next;
    Ok(Some(outcome))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct V7MovePrelude {
    pub(crate) moving: Piece,
    pub(crate) destination: Square,
    pub(crate) portal_entry: Option<Square>,
    pub(crate) portal_exit: Option<Square>,
    pub(crate) portal_entry_target: Option<Piece>,
    /// Source switcheroo validates the physical pawn, then clears the logical
    /// capture target. The common landing kernel still reads the pawn later.
    pub(crate) landing_target: Option<Piece>,
    pub(crate) privacy: Value,
    pub(crate) quantum_observation: crate::v7_quantum_state::QuantumMoveObservation,
    pub(crate) mad_horse_entry_capture: bool,
    pub(crate) mad_horse_exit_capture: bool,
    pub(crate) switcheroo: bool,
    pub(crate) siege_ram_path: Vec<Square>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum V7MoveSelection {
    Ready(Box<V7MovePrelude>),
    Return(StationaryMoveOutcome),
}

fn optional_descriptor_square(target: &MoveTarget, field: &str) -> Result<Option<Square>> {
    let Some(value) = target.flags.get(field) else {
        return Ok(None);
    };
    let row = value.get("row").and_then(Value::as_f64);
    let col = value.get("col").and_then(Value::as_f64);
    match row
        .zip(col)
        .filter(|(row, col)| (0.0..8.0).contains(row) && (0.0..8.0).contains(col))
    {
        None => Ok(None),
        Some((row, col)) if row.fract() == 0.0 && col.fract() == 0.0 => Ok(Some(Square {
            row: row as u8,
            col: col as u8,
        })),
        Some(_) => Err(EngineError::InvalidState(format!(
            "v7 {field} requires integer source board coordinates"
        ))),
    }
}

fn cancel_selection(state: &mut GameState) -> Result<V7MoveSelection> {
    state.extra.insert("selected".into(), Value::Null);
    state.extra.insert("legalMoves".into(), json!([]));
    render_board_animation_entries_v7(state)?;
    Ok(V7MoveSelection::Return(StationaryMoveOutcome::returned()))
}

fn render_piece_type(piece: &Piece) -> &str {
    if piece.kind == "log" && truth(piece.extra.get("logDir")) {
        "logRolling"
    } else if piece.kind == "windmill" {
        if piece.extra.get("windmillMode").and_then(Value::as_str) == Some("rook") {
            "windmillRook"
        } else {
            "windmillBishop"
        }
    } else {
        &piece.kind
    }
}

/// main91574-91653. Admission's bound/frozen/poison/scarecrow guards and the
/// early Football/log branch precede this callback. Call it before thief flag
/// consumption and firstMoveUndo. A cancelled quantum observation is retained;
/// errors preserve the input state. Both ordinary and special moves share it.
pub(crate) fn prepare_v7_move_selection(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
) -> Result<V7MoveSelection> {
    let mut next = state.clone();
    let mut moving = require_origin(&next, from)?;
    let privacy = privacy_snapshot(&next, &moving, from)?;
    let has_portal = target.flag("portalLanding") || target.flag("portalThrough");
    let entry = if has_portal {
        optional_descriptor_square(target, "portalEntry")?
    } else {
        None
    };
    let exit = if has_portal {
        optional_descriptor_square(target, "portalExit")?
    } else {
        None
    };
    let mut destination = target.square();
    if let Some((entry, exit)) = entry.zip(exit) {
        let changed = crate::movement::v7_portal_exit_at(&next, entry) != Some(exit);
        let mut blocked =
            crate::movement::collapsed(&next, entry) || crate::movement::collapsed(&next, exit);
        if target.flag("portalThrough") && !target.flag("siegeRamMove") {
            for cell in [entry, exit] {
                if let Some(blocker) = next.at(cell)
                    && !crate::movement::v7_time_phase_transparent_blocker(&next, &moving, blocker)
                    && !crate::movement::v7_ghost_transparent_for(
                        &next,
                        &moving,
                        blocker,
                        cell,
                        render_piece_type(&moving),
                    )
                {
                    blocked = true;
                }
            }
        }
        if changed || blocked {
            let outcome = cancel_selection(&mut next)?;
            *state = next;
            return Ok(outcome);
        }
        if target.flag("portalLanding") {
            destination = exit;
        }
    }
    let quantum_observation = crate::v7_quantum_state::observe_move_landing_in_place(
        &mut next,
        &mut moving,
        destination,
        if target.flag("portalLanding") {
            entry
        } else {
            None
        },
    )?;
    let mut landing_target = next.at(destination).cloned();
    let mut siege_ram_path = Vec::new();
    if target.flag("siegeRamMove") {
        let mut seen = BTreeSet::new();
        if let Some(cells) = target.flags.get("highlightCells").and_then(Value::as_array) {
            for cell in cells {
                let mut descriptor = MoveTarget::at(destination);
                descriptor.flags.insert("cell".into(), cell.clone());
                if let Some(square) = optional_descriptor_square(&descriptor, "cell")?
                    && seen.insert(square)
                {
                    siege_ram_path.push(square);
                }
            }
        }
        if !crate::movement::v7_can_resolve_siege_ram_move(&moving, target)
            || siege_ram_path.is_empty()
            || siege_ram_path.len() > 3
        {
            *state = next;
            return Ok(V7MoveSelection::Return(StationaryMoveOutcome::returned()));
        }
    }
    let mut portal_entry_target = if target.flag("portalLanding") {
        entry.and_then(|at| next.at(at)).cloned()
    } else {
        None
    };
    if target.flag("portalTransparentEntry")
        && portal_entry_target.as_ref().is_some_and(|blocker| {
            crate::movement::v7_time_phase_transparent_blocker(&next, &moving, blocker)
                || crate::movement::v7_ghost_transparent_for(
                    &next,
                    &moving,
                    blocker,
                    entry.unwrap_or(destination),
                    render_piece_type(&moving),
                )
        })
    {
        portal_entry_target = None;
    }
    let mad_horse_entry_capture = target.flag("madHorsePortalEntryCapture")
        && entry
            .zip(portal_entry_target.as_ref())
            .is_some_and(|(at, victim)| {
                crate::movement::v7_mad_horse_friendly_target(&next, &moving, victim, at)
            });
    let mad_horse_exit_capture = (target.flag("madHorsePortalExitCapture")
        || !target.flag("portalLanding") && target.flag("madHorseCapture"))
        && landing_target.as_ref().is_some_and(|victim| {
            crate::movement::v7_mad_horse_friendly_target(&next, &moving, victim, destination)
        });
    let friendly_entry_blocked = portal_entry_target
        .as_ref()
        .is_some_and(|victim| victim.id != moving.id && victim.color == moving.color)
        && !mad_horse_entry_capture
        && !target.flag("siegeRamMove");
    let friendly_landing_blocked = landing_target
        .as_ref()
        .is_some_and(|victim| victim.id != moving.id && victim.color == moving.color)
        && !mad_horse_exit_capture
        && ![
            "dragonSwap",
            "switcherooMove",
            "relaySwap",
            "siegeRamMove",
            "castle",
            "colossusMove",
            "bigRookMove",
        ]
        .iter()
        .any(|field| target.flag(field));
    let switcheroo = target.flag("switcherooMove")
        && landing_target.as_ref().is_some_and(|victim| {
            crate::movement::v7_can_resolve_switcheroo_move(&next, &moving, victim)
        });
    if friendly_entry_blocked
        || friendly_landing_blocked
        || target.flag("switcherooMove") && !switcheroo
    {
        let outcome = cancel_selection(&mut next)?;
        *state = next;
        return Ok(outcome);
    }
    if switcheroo {
        landing_target = None;
    }
    if !target.flag("mistakeReverse")
        && target.flag("highwayMove")
        && [&landing_target, &portal_entry_target]
            .into_iter()
            .flatten()
            .any(|victim| {
                victim.id != moving.id
                    && victim.color != moving.color
                    && !crate::movement::v7_highway_capture_allowed(&moving, victim)
            })
    {
        let outcome = cancel_selection(&mut next)?;
        *state = next;
        return Ok(outcome);
    }
    *state = next;
    Ok(V7MoveSelection::Ready(Box::new(V7MovePrelude {
        moving,
        destination,
        portal_entry: entry,
        portal_exit: exit,
        portal_entry_target,
        landing_target,
        privacy,
        quantum_observation,
        mad_horse_entry_capture,
        mad_horse_exit_capture,
        switcheroo,
        siege_ram_path,
    })))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MistakeReversalPlan {
    pub(crate) moving: Piece,
    pub(crate) counter: Piece,
    pub(crate) counter_from: Square,
    pub(crate) destination: Square,
    pub(crate) attempted_destination: Square,
    pub(crate) privacy: Value,
    pub(crate) attempted_game_end: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MistakeReversalContext {
    previous_turn: Color,
    previous_free_move_resolution: Option<Color>,
    original_actor: Color,
    counter_actor: Color,
    replay_count_before: usize,
}

/// main91182 + 92006. The ordinary capture owner calls this only after its
/// saturation/frozen/manner/initiative/high-ground/Jester/blood-veil guards.
/// Every admitted candidate consumes exactly one roll, even when it misses.
/// A typed MoveTarget contains the source candidate's integer row/col.
pub(crate) fn roll_mistake_reversal(
    state: &mut GameState,
    prelude: &V7MovePrelude,
    target: &MoveTarget,
) -> Result<Option<MistakeReversalPlan>> {
    if !(truth(state.extra.get("mistakeRule")) || state.flag("mistakeCard", prelude.moving.color))
        || target.flag("mistakeReverse")
        || state.is_ai_simulation()
    {
        return Ok(None);
    }
    let mut candidates = Vec::new();
    if let Some((at, piece)) = prelude
        .portal_entry
        .zip(prelude.portal_entry_target.as_ref())
    {
        candidates.push((at, piece.clone(), false));
    }
    if let Some(piece) = &prelude.landing_target {
        candidates.push((prelude.destination, piece.clone(), false));
    }
    if target.flag("enPassant") {
        let row = target
            .flags
            .get("capturedRow")
            .and_then(Value::as_u64)
            .filter(|row| *row < 8);
        let col = target
            .flags
            .get("capturedCol")
            .and_then(Value::as_u64)
            .filter(|col| *col < 8);
        if let Some((row, col)) = row.zip(col) {
            let at = Square {
                row: row as u8,
                col: col as u8,
            };
            if let Some(piece) = state.at(at) {
                candidates.push((at, piece.clone(), false));
            }
        }
    }
    if target.flag("jumpCapture")
        && let Some(at) = optional_descriptor_square(target, "jumpCapture")?
        && let Some(piece) = state.at(at)
    {
        candidates.push((at, piece.clone(), true));
    }
    let mut candidate = None;
    for (at, counter, jump) in candidates {
        if counter.color.owner().is_none()
            || counter.color == prelude.moving.color
            || matches!(
                counter.kind.as_str(),
                "colossus"
                    | "bigRook"
                    | "bigBishop"
                    | "wall"
                    | "football"
                    | "monster"
                    | "blackHole"
                    | "coffin"
            )
        {
            continue;
        }
        let allowed = if jump {
            if target.flag("checkerCapture") {
                crate::movement::v7_checker_capture_target_allowed(
                    state,
                    &prelude.moving,
                    &counter,
                    at,
                )?
            } else {
                crate::movement::v7_radical_charge_capture_target_allowed(
                    state,
                    &prelude.moving,
                    &counter,
                    at,
                )?
            }
        } else {
            crate::movement::v7_can_capture_target(
                state,
                &prelude.moving,
                &counter,
                false,
                target.flag("basicTrainingCapture"),
            )?
        };
        if allowed {
            candidate = Some((at, counter));
            break;
        }
    }
    let Some((counter_from, counter)) = candidate else {
        return Ok(None);
    };
    let chance = if state.flag("mistakeCard", prelude.moving.color) {
        0.5
    } else {
        0.2
    };
    let roll = state.rng.sample()?;
    state.rng.record_last_probability(
        if roll >= chance { 1.0 - chance } else { chance },
        "source Mistake reversal branch",
    )?;
    if roll >= chance {
        return Ok(None);
    }
    if state
        .at(counter_from)
        .is_none_or(|current| current.id != counter.id)
    {
        return Ok(None);
    }
    let destination = crate::movement::find_square(state, &prelude.moving.id).ok_or_else(|| {
        EngineError::InvalidState("v7 mistake reversal lost the attacker's source object".into())
    })?;
    Ok(Some(MistakeReversalPlan {
        moving: prelude.moving.clone(),
        counter,
        counter_from,
        destination,
        attempted_destination: prelude.destination,
        privacy: prelude.privacy.clone(),
        attempted_game_end: defeat_royal(state, &prelude.moving)?,
    }))
}

/// Call only after queuing the source attempt notation. The caller executes
/// one recursive ordinary move with mistakeReverse=true, without candidate
/// geometry validation, then restores this context even on an error.
pub(crate) fn begin_mistake_reversal(
    state: &mut GameState,
    plan: &MistakeReversalPlan,
) -> Result<MistakeReversalContext> {
    let actor = plan.moving.color.owner().ok_or(EngineError::WrongActor)?;
    let counter = plan.counter.color.owner().ok_or(EngineError::WrongActor)?;
    if state
        .at(plan.counter_from)
        .is_none_or(|current| current.id != plan.counter.id)
    {
        return Err(EngineError::InvalidState(
            "v7 mistake counter identity changed before reverse execution".into(),
        ));
    }
    let hidden = if crate::observation::piece_visible_to_color_at_v7(
        state,
        &plan.counter,
        plan.counter_from,
        actor,
    )? {
        ""
    } else {
        actor.as_str()
    };
    let context = MistakeReversalContext {
        previous_turn: state.turn,
        previous_free_move_resolution: state.free_move_resolution,
        original_actor: actor,
        counter_actor: counter,
        replay_count_before: state
            .extra
            .get("replayEvents")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
    };
    crate::replay::queue_visual(
        state,
        json!({"type":"mistake","color":actor,"cells":[plan.counter_from,plan.destination],
        "from":plan.counter_from,"to":plan.destination,"hiddenFrom":hidden}),
    )?;
    crate::replay::add_log(state, "실수: 상대 기물이 공격을 뒤집었습니다.".into())?;
    state.free_move_resolution = Some(counter);
    state.turn = counter;
    Ok(context)
}

pub(crate) fn restore_mistake_context(state: &mut GameState, context: &MistakeReversalContext) {
    state.free_move_resolution = context.previous_free_move_resolution;
    state.turn = context.previous_turn;
}

pub(crate) fn complete_mistake_reversal(
    state: &mut GameState,
    context: MistakeReversalContext,
) -> Result<StationaryMoveOutcome> {
    restore_mistake_context(state, &context);
    if state.mode != "gameover" {
        return Ok(StationaryMoveOutcome::end_move(
            context.original_actor,
            "실수",
            format!(
                "{} 기물이 공격을 뒤집었습니다.",
                crate::replay::label(context.counter_actor)
            ),
            Vec::new(),
        ));
    }
    let replay_count = state
        .extra
        .get("replayEvents")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    Ok(if replay_count == context.replay_count_before {
        StationaryMoveOutcome {
            captures: Vec::new(),
            completion: StationaryCompletion::RecordTerminal,
            replay_before: None,
        }
    } else {
        StationaryMoveOutcome::returned()
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SiegeRamPreCapture {
    Return(StationaryMoveOutcome),
    Continue {
        captured_something: bool,
        chameleon_victim: Option<Piece>,
        captures: Vec<Piece>,
        replay_before: Box<GameState>,
    },
}

/// main91850-91900. canSiegeRamAffectTarget(95486) is unconditionally true in
/// the frozen source. Initial hit count still includes neutral/wall identities.
/// Force removals use only Reaper nearby deaths; ordinary hits use forceCapture
/// and deferReaperNotation. Source supplies no explosion or notation capture
/// cells here, and does not mark capturedByMovingPiece/capturedOutsideJump.
pub(crate) fn execute_siege_ram_pre_capture(
    state: &mut GameState,
    prelude: &mut V7MovePrelude,
    target: &MoveTarget,
    threat_probe: bool,
) -> Result<Option<SiegeRamPreCapture>> {
    if !target.flag("siegeRamMove") {
        return Ok(None);
    }
    let mut next = state.clone();
    let mut prepared = prelude.clone();
    let mut moving = prepared.moving.clone();
    let actor = moving.color.owner().ok_or(EngineError::WrongActor)?;
    let mut hit_ids = BTreeSet::new();
    for cell in &prepared.siege_ram_path {
        if let Some(victim) = next.at(*cell).filter(|victim| victim.id != moving.id) {
            if victim.id.is_empty() {
                return Err(EngineError::InvalidState(
                    "v7 siege ram potential capture needs stable victim identities".into(),
                ));
            }
            hit_ids.insert(victim.id.clone());
        }
    }
    let locked = crate::v7_capture_reactions::saturation_locked(&next, &moving);
    if !hit_ids.is_empty()
        && (locked
            || crate::v7_threat::manner_capture_locked(&next, &moving)
            || crate::movement::v7_initiative_capture_locked(&next, moving.color))
    {
        return Ok(Some(SiegeRamPreCapture::Return(
            StationaryMoveOutcome::returned(),
        )));
    }
    let before = crate::replay::begin_move(&mut next, actor)?;
    let options = CaptureOptions {
        force_capture: true,
        allow_jester: true,
        attacker_landing: Some(prepared.destination),
        defer_notation: true,
        threat_probe,
        saturation_locked: Some(locked),
        ..CaptureOptions::default()
    };
    let mut captured_something = false;
    let mut chameleon_victim = None;
    let mut captures = Vec::new();
    for cell in &prepared.siege_ram_path {
        let Some(victim) = next
            .at(*cell)
            .cloned()
            .filter(|victim| victim.id != moving.id)
        else {
            continue;
        };
        if victim.kind == "wall" || victim.color.owner().is_none() {
            if victim.is_large() {
                crate::transition::clear_piece(&mut next, &victim.id);
            } else {
                next.board[usize::from(cell.row)][usize::from(cell.col)] = None;
            }
            captured_something = true;
            let removed = crate::v7_board_hazards::EnvironmentalRemoval {
                piece: victim,
                square: *cell,
                capture_owner: actor,
            };
            crate::v7_board_hazards::resolve_reaper_nearby_deaths_with_context(
                &mut next,
                &[removed],
                Some(&mut moving),
                Some(prepared.destination),
                true,
            )?;
        } else if let Some(captured) =
            crate::v7_capture_reactions::capture_at(&mut next, &mut moving, *cell, &options)?
        {
            captured_something = true;
            if chameleon_victim.is_none()
                && !crate::v7_board_hazards::source_royal_identity(&next, &captured)?
                && !matches!(
                    captured.kind.as_str(),
                    "wall" | "colossus" | "bigRook" | "bigBishop"
                )
            {
                chameleon_victim = Some(captured.clone());
            }
            captures.push(captured);
        }
    }
    prepared.moving = moving;
    prepared.portal_entry_target = None;
    prepared.landing_target = next.at(prepared.destination).cloned();
    *state = next;
    *prelude = prepared;
    Ok(Some(SiegeRamPreCapture::Continue {
        captured_something,
        chameleon_victim,
        captures,
        replay_before: Box::new(before),
    }))
}

fn normalize_knight_journey_data(state: &mut GameState) -> Result<bool> {
    let setup = state
        .extra
        .get("campaign")
        .and_then(|campaign| campaign.get("setup"))
        .and_then(Value::as_str);
    if !matches!(setup, Some("knightJourney" | "knightGame")) {
        return Ok(false);
    }
    let campaign = state
        .extra
        .get_mut("campaign")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            EngineError::InvalidState("v7 Knight Journey campaign must be an object".into())
        })?;
    if !truth(campaign.get("knightJourney")) {
        campaign.insert(
            "knightJourney".into(),
            json!({"visited":[],"kingSquare":"","undo":[],"superHintMoves":0}),
        );
    }
    let data = campaign
        .get_mut("knightJourney")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            EngineError::InvalidState("v7 Knight Journey data must be an object".into())
        })?;
    for field in ["visited", "undo"] {
        if !data.get(field).is_some_and(Value::is_array) {
            data.insert(field.into(), json!([]));
        }
    }
    let moves = crate::card_effects::js_number(data.get("superHintMoves"), 0)
        .unwrap_or(0.0)
        .max(0.0);
    if !moves.is_finite() {
        return Err(EngineError::InvalidState(
            "v7 Knight Journey superHintMoves is not representable as a finite source number"
                .into(),
        ));
    }
    data.insert("superHintMoves".into(), json!(moves));
    Ok(true)
}

fn source_side_arrays(state: &GameState, field: &str) -> Result<Value> {
    let mut result = json!({"white":[],"black":[]});
    for actor in [Color::White, Color::Black] {
        if let Some(value) = state
            .extra
            .get(field)
            .and_then(|value| value.get(actor.as_str()))
            .filter(|value| truth(Some(value)))
        {
            // main89308/89312는 배열 clone이 아니라 iterable spread다.
            // live capturedTypes/turnCaptures는 Set이며 undo 안에서만 배열이 된다.
            let array = match value {
                Value::Array(array) => array.clone(),
                Value::String(text) => text
                    .chars()
                    .map(|character| json!(character.to_string()))
                    .collect(),
                Value::Object(fields)
                    if fields.get("__simType").and_then(Value::as_str) == Some("Set") =>
                {
                    fields
                        .get("values")
                        .and_then(Value::as_array)
                        .cloned()
                        .ok_or_else(|| {
                            EngineError::InvalidState(format!(
                                "v7 Mad Knight {field}.{} source Set requires an array of values",
                                actor.as_str()
                            ))
                        })?
                }
                Value::Object(fields)
                    if fields.get("__simType").and_then(Value::as_str) == Some("Map") =>
                {
                    let entries = fields.get("entries").and_then(Value::as_array).ok_or_else(
                        || {
                            EngineError::InvalidState(format!(
                                "v7 Mad Knight {field}.{} source Map requires an array of entries",
                                actor.as_str()
                            ))
                        },
                    )?;
                    if entries
                        .iter()
                        .any(|entry| entry.as_array().is_none_or(|entry| entry.len() != 2))
                    {
                        return Err(EngineError::InvalidState(format!(
                            "v7 Mad Knight {field}.{} source Map entries must be key/value pairs",
                            actor.as_str()
                        )));
                    }
                    entries.clone()
                }
                _ => {
                    return Err(EngineError::InvalidState(format!(
                        "v7 Mad Knight {field}.{} requires a source iterable",
                        actor.as_str()
                    )));
                }
            };
            result[actor.as_str()] = json!(array);
        }
    }
    Ok(result)
}

/// main92117 / 89290-89334. Call after beginMoveReplayCapture and quantum
/// candidate calculation but before ordinary captures. The source snapshot
/// omits RNG/decks/other game fields and stores the exact listed undo fields.
pub(crate) fn push_mad_knight_undo_before_move(
    state: &mut GameState,
    moving: &Piece,
) -> Result<bool> {
    if state.free_move_resolution == moving.color.owner()
        || state.mode != "play"
        || moving.color != Color::White
        || moving.kind != "knight"
        || state
            .extra
            .get("campaign")
            .and_then(|campaign| campaign.get("setup"))
            .and_then(Value::as_str)
            != Some("knightJourney")
    {
        return Ok(false);
    }
    let mut next = state.clone();
    if !normalize_knight_journey_data(&mut next)? {
        return Ok(false);
    }
    let data = &next.extra["campaign"]["knightJourney"];
    let selected = next
        .extra
        .get("selected")
        .filter(|value| truth(Some(value)))
        .cloned()
        .unwrap_or(Value::Null);
    let legal = next
        .extra
        .get("legalMoves")
        .filter(|value| truth(Some(value)))
        .cloned()
        .unwrap_or_else(|| json!([]));
    let fallback = |field: &str, default: Value| {
        next.extra
            .get(field)
            .filter(|value| truth(Some(value)))
            .cloned()
            .unwrap_or(default)
    };
    let snapshot = json!({
        "board":next.board,
        "knightJourney":{"visited":data["visited"],"kingSquare":data.get("kingSquare").filter(|value| truth(Some(value))).cloned().unwrap_or_else(|| json!("")),
            "superHintMoves":data["superHintMoves"]},
        "selected":selected,"legalMoves":legal,"captures":next.captures,
        "capturedTypes":source_side_arrays(&next,"capturedTypes")?,"turnCaptures":source_side_arrays(&next,"turnCaptures")?,
        "kingDead":fallback("kingDead",json!({"white":false,"black":false})),
        "castled":fallback("castled",json!({"white":false,"black":false})),
        "enPassant":next.en_passant,"lastMove":fallback("lastMove",Value::Null),"logs":fallback("logs",json!([])),
        "boardHistoryLength":next.extra.get("boardHistory").and_then(Value::as_array).map_or(0,Vec::len),
        "replayEventLength":next.extra.get("replayEvents").and_then(Value::as_array).map_or(0,Vec::len),
        "winner":fallback("winner",Value::Null),"mode":next.mode,"turn":next.turn,"moveCount":next.move_count,"fullMove":next.full_move,
    });
    next.extra
        .get_mut("campaign")
        .and_then(|campaign| campaign.get_mut("knightJourney"))
        .and_then(|data| data.get_mut("undo"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            EngineError::InvalidState("v7 Mad Knight undo stack was lost before push".into())
        })?
        .push(snapshot);
    *state = next;
    Ok(true)
}

fn regency_heir(state: &GameState, piece: &Piece) -> bool {
    truth(piece.extra.get("regencyHeir"))
        && state.flag("kingDead", piece.color)
        && state.flag("regency", piece.color)
}

fn defeat_royal(state: &GameState, piece: &Piece) -> Result<bool> {
    Ok(crate::v7_board_hazards::source_royal_king(state, piece)?
        || piece.kind == "vip"
        || regency_heir(state, piece))
}

/// main108251. A purchase converts an object without ordinary capture,
/// fresh-origin, HP reset or exhaustion. A purchased royal stays on its square
/// in the terminal branch. The second quantum observation belongs here.
pub(crate) fn execute_merchant_purchase(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
    threat_probe: bool,
) -> Result<Option<StationaryMoveOutcome>> {
    if !target.flag("merchantBuy") {
        return Ok(None);
    }
    let mut next = state.clone();
    let mut merchant = require_origin(&next, from)?;
    let actor = merchant.color.owner().ok_or(EngineError::WrongActor)?;
    let to = target.square();
    let privacy = privacy_snapshot(&next, &merchant, from)?;
    crate::v7_quantum_state::observe_quantum_at_in_place(&mut next, to, Some(actor))?;
    refresh_alias(&next, &mut merchant);
    let Some(mut purchased) = next.at(to).cloned() else {
        *state = next;
        return Ok(Some(StationaryMoveOutcome::returned()));
    };
    let purchased_audience = purchased.clone();
    let purchased_privacy = privacy_snapshot(&next, &purchased, to)?;
    if merchant.kind != "merchant"
        || purchased.color == merchant.color
        || crate::movement::frozen(&purchased)
        || purchased.kind != "scarecrow" && truth(purchased.extra.get("shielded"))
        || crate::v7_threat::manner_capture_locked(&next, &merchant)
    {
        *state = next;
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    let cost = crate::variant_movement::merchant_cost_v7(&purchased);
    let gold = match merchant.extra.get("gold").filter(|value| !value.is_null()) {
        None => 0.0,
        Some(value) => crate::card_effects::js_number(Some(value), 0)
            .filter(|gold| gold.is_finite())
            .ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 merchant gold cannot be represented as a finite source number".into(),
                )
            })?,
    };
    let Some(cost) = cost.filter(|cost| *cost <= gold) else {
        *state = next;
        return Ok(Some(StationaryMoveOutcome::returned()));
    };
    merchant.extra.insert("gold".into(), json!(gold - cost));
    crate::v7_board_hazards::replace_object_aliases(&mut next, &merchant);
    let hidden = hidden_from_for_move(&next, &merchant, to, &privacy)?;
    crate::replay::queue_visual(
        &mut next,
        json!({
            "type":"merchant-buy","color":actor,"targetColor":purchased.color,"targetType":purchased.kind,
            "from":from,"to":to,"cells":[from,to],"hiddenFrom":hidden,
        }),
    )?;
    let democracy_protected = next.flag("democracy", purchased.color)
        && crate::v7_board_hazards::source_royal_king(&next, &purchased)?;
    if democracy_protected && let Some(owner) = purchased.color.owner() {
        next.set_flag("kingDead", owner, true);
    }
    let terminal = !democracy_protected
        && defeat_royal(&next, &purchased)?
        && (regency_heir(&next, &purchased)
            || purchased.kind == "vip"
            || !next.flag("regency", purchased.color)
            || !next
                .board
                .iter()
                .flatten()
                .flatten()
                .any(|piece| piece.color == purchased.color && piece.kind == "queen"));
    if !terminal {
        purchased.color = merchant.color;
        purchased.moved = true;
        crate::v7_board_hazards::replace_object_aliases(&mut next, &purchased);
        crate::card_effects::mark_animation(&mut next, &merchant)?;
        crate::card_effects::mark_animation(&mut next, &purchased)?;
    }
    let target_label =
        crate::replay::source_piece_label(&purchased.kind).unwrap_or(&purchased.kind);
    let description = format!(
        "{} 상인이 {}의 {target_label}을 매수",
        crate::replay::label(actor),
        square_name(to)
    );
    crate::replay::queue_special_move_notation(
        &mut next,
        &merchant,
        from,
        format!(
            "{}${}{}",
            crate::replay::piece_code(RULES_VERSION_V7, &merchant.kind),
            square_name(to),
            if terminal { "#" } else { "" }
        ),
        description,
        crate::replay::MoveNotationVisibility {
            privacy: &privacy,
            capture: false,
            capture_known_squares: &json!({}),
        },
    )?;
    // Both subjects retain their original audience color. Visibility after
    // conversion must not turn the opponent's previously concealed object
    // into the buyer's visible ally for this event.
    let concealed = !hidden_from_for_move(&next, &merchant, from, &privacy)?.is_empty()
        || purchased_audience.color.owner().is_some()
            && !hidden_from_for_move(&next, &purchased_audience, to, &purchased_privacy)?
                .is_empty();
    let message = if terminal {
        format!(
            "{} 상인이 상대 {target_label}을 {cost}골드에 매수했습니다.",
            crate::replay::label(actor)
        )
    } else {
        // main108328 intentionally omits the description's type fallback.
        // 원본 직접 조회는 알 수 없는 물리 type에만 undefined를 남긴다.
        format!(
            "{} 상인이 {}을 {cost}골드에 매수했습니다.",
            crate::replay::label(actor),
            crate::replay::source_piece_label(&purchased.kind).unwrap_or("undefined")
        )
    };
    crate::replay::add_log(
        &mut next,
        if concealed {
            "기물이 행동했습니다.".into()
        } else {
            message
        },
    )?;
    let outcome = if terminal {
        crate::v7_threat::mark_king_threat_removal_cause(
            &mut next,
            &purchased,
            to,
            &json!({"attacker":merchant,"origin":from,"label":"상인 매수","terminal":true}),
            threat_probe,
        )?;
        crate::flow::end_game(
            &mut next,
            Some(actor),
            &format!("상인이 상대 {target_label}을 매수했습니다."),
        )?;
        StationaryMoveOutcome {
            captures: Vec::new(),
            completion: StationaryCompletion::RecordTerminal,
            replay_before: None,
        }
    } else {
        crate::v7_threat::resolve_herald_threats_v7_with_probe(&mut next, actor, threat_probe)?;
        if next.mode == "gameover" {
            StationaryMoveOutcome::returned()
        } else {
            StationaryMoveOutcome::end_move(actor, "상인 매수", String::new(), Vec::new())
        }
    };
    *state = next;
    Ok(Some(outcome))
}

/// main91815 + applyGrapplerPull(848). The queried plan owns geometry. Large
/// targets retain their old anchor fields because the source only places the
/// object in the translated cells here.
pub(crate) fn execute_grappler_pull(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
) -> Result<Option<StationaryMoveOutcome>> {
    if !target.flag("grapplePull") {
        return Ok(None);
    }
    let mut next = state.clone();
    let moving = require_origin(&next, from)?;
    let actor = moving.color.owner().ok_or(EngineError::WrongActor)?;
    if moving.kind != "grappler" {
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    let Some(plan) = crate::variant_movement::v7_grappler_pull_plan(&next, from, target.square())?
    else {
        return Ok(Some(StationaryMoveOutcome::returned()));
    };
    let before = crate::replay::begin_move(&mut next, actor)?;
    let mut pulled = plan.target;
    let mut caster = moving;
    for square in &plan.source_cells {
        next.board[usize::from(square.row)][usize::from(square.col)] = None;
    }
    pulled.moved = true;
    pulled.extra.insert(
        "grapplerBound".into(),
        json!({"untilColor":plan.until_color}),
    );
    caster.extra.insert(
        "grapplerBound".into(),
        json!({"untilColor":plan.until_color}),
    );
    for square in &plan.cells {
        next.board[usize::from(square.row)][usize::from(square.col)] = Some(pulled.clone());
    }
    crate::v7_board_hazards::replace_object_aliases(&mut next, &caster);
    next.en_passant = None;
    crate::replay::queue_visual(
        &mut next,
        json!({"type":"grappler-pull","color":actor,"from":from,
        "targetFrom":plan.target_from,"to":plan.to,"target":{"id":pulled.id,"type":pulled.kind,"color":pulled.color},
        "size":if plan.cells.len() == 4 { 2 } else { 1 }}),
    )?;
    crate::replay::add_log(
        &mut next,
        format!(
            "그래플러가 {}의 기물을 끌어와 속박했습니다.",
            square_name(target.square())
        ),
    )?;
    next.extra.insert("lastMove".into(), Value::Null);
    next.extra.insert("accelerationTrail".into(), Value::Null);
    crate::card_effects::set_last_move(&mut next, from, from, "", actor, "", None)?;
    // visual.target은 main91799의 {id,type,color}만 가진 별도 render 객체다.
    // 기물의 hiddenFrom이나 이후 환경 제거를 ghost visibility에 적용하지 않는다.
    let visual_target = Piece::new(pulled.kind.clone(), pulled.color, pulled.id.clone());
    let mut outcome = StationaryMoveOutcome {
        captures: Vec::new(),
        completion: StationaryCompletion::EndGrapplerPull {
            actor,
            visual_target,
        },
        replay_before: None,
    };
    outcome.replay_before = Some(Box::new(before));
    *state = next;
    Ok(Some(outcome))
}

/// main101997→3908→3932. 동결 browserShell의 board/querySelector와 motion
/// 프로필은 ghost 생성을 허용한다. visual.hiddenFrom은 실행 분기에서 생략되므로
/// live board의 fog/camouflage를 조회하지 않는다. AI 실행만 source대로 억제한다.
pub(crate) fn finish_grappler_visual(state: &mut GameState, visual_target: &Piece) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::InvalidState(
            "v7 Grappler visual requires the pinned rules version".into(),
        ));
    }
    if state.is_ai_simulation() {
        return Ok(());
    }
    crate::card_effects::mark_rendered_piece_animation(state, visual_target)
}

fn descriptor_cells(target: &MoveTarget, field: &str) -> Result<Vec<Square>> {
    target
        .flags
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineError::InvalidState(format!("v7 {field} requires an ordered cell array"))
        })?
        .iter()
        .map(|cell| {
            let row = cell
                .get("row")
                .and_then(Value::as_u64)
                .filter(|row| *row < 8);
            let col = cell
                .get("col")
                .and_then(Value::as_u64)
                .filter(|col| *col < 8);
            row.zip(col)
                .map(|(row, col)| Square {
                    row: row as u8,
                    col: col as u8,
                })
                .ok_or_else(|| {
                    EngineError::InvalidState(format!(
                        "v7 {field} contains a non-integer or out-of-bounds cell"
                    ))
                })
        })
        .collect()
}

fn render_square(value: &Value) -> Option<Square> {
    let coordinate = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_f64)
            .filter(|number| {
                number.is_finite() && number.fract() == 0.0 && (0.0..8.0).contains(number)
            })
            .map(|number| number as u8)
    };
    coordinate("row")
        .zip(coordinate("col"))
        .map(|(row, col)| Square { row, col })
}

fn pending_render_piece(entry: &Value, kind: &str, fallback_id: &str) -> Result<Piece> {
    let id = if truth(entry.get("id")) {
        entry.get("id").and_then(Value::as_str).ok_or_else(|| {
            EngineError::InvalidState(format!("v7 renderBoard pending {kind} id must be a string"))
        })?
    } else {
        fallback_id
    };
    let color = if entry.get("color").and_then(Value::as_str) == Some("black") {
        Color::Black
    } else {
        Color::White
    };
    Ok(Piece::new(kind, color, id))
}

fn render_pending_entries<'a>(state: &'a GameState, field: &str) -> Result<&'a [Value]> {
    match state.extra.get(field) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(entries)) => Ok(entries),
        Some(_) => Err(EngineError::InvalidState(format!(
            "v7 renderBoard {field} must be an array"
        ))),
    }
}

/// main72874/73123/74303/74560/75858. 동결 프로필은 renderAll만 억제한다.
/// 이 분기의 직접 renderBoard 호출은 미리보기와 후속 quantum/large overlay를
/// 포함한 표시 순서대로 직렬화 애니메이션 Set을 갱신한다. RNG·replay는 추가하지 않는다.
pub(crate) fn render_board_animation_entries_v7(state: &mut GameState) -> Result<()> {
    if state.is_ai_simulation() {
        return Ok(());
    }
    if state.ruleset_id != RULES_VERSION_V7
        || state.board.len() != 8
        || state.board.iter().any(|row| row.len() != 8)
    {
        return Err(EngineError::InvalidState(
            "v7 direct renderBoard requires the pinned 8x8 board".into(),
        ));
    }
    let frame = crate::replay::v7_history_frame(state)?;
    let historical = frame.is_some();
    let mut display = state.clone();
    let mut visibility_state = state.clone();
    let mut fog_state = state.clone();
    if let Some(frame) = frame {
        let fields = frame.as_object().ok_or_else(|| {
            EngineError::InvalidState("v7 renderBoard historical frame must be an object".into())
        })?;
        display.board = serde_json::from_value(fields.get("board").cloned().unwrap_or(Value::Null))
            .map_err(|error| {
                EngineError::InvalidState(format!("v7 renderBoard historical board: {error}"))
            })?;
        if display.board.len() != 8 || display.board.iter().any(|row| row.len() != 8) {
            return Err(EngineError::InvalidState(
                "v7 renderBoard historical board must be 8x8".into(),
            ));
        }
        // pending 목록과 overlay는 historical frame을 읽는다. main109801의
        // activeCamouflageRule만 activeReplayValue를 쓰고, isRoyalIdentityPiece는
        // 명시 source 없이 live state의 profile/regency/kingDead를 읽는다.
        display.extra = fields
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        visibility_state.board = display.board.clone();
        visibility_state.extra.insert(
            "camouflageRule".into(),
            fields
                .get("camouflageRule")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        );
        // main62334는 board/turn만 임시 교체하고 getLegalMoves를 호출한다.
        // campaign은 activeReplayValue("campaign", state.campaign)를 따른다.
        fog_state.board = display.board.clone();
        if let Some(campaign) = fields.get("campaign") {
            fog_state.extra.insert("campaign".into(), campaign.clone());
        }
    }
    if !historical {
        let selected = state
            .extra
            .get("selected")
            .and_then(render_square)
            .and_then(|at| state.at(at));
        let preserve_preview = selected.is_some_and(|piece| {
            matches!(piece.kind.as_str(), "bigRook" | "bigBishop")
                && state
                    .extra
                    .get("bigRookPreview")
                    .and_then(|preview| preview.get("pieceId"))
                    .and_then(Value::as_str)
                    == Some(&piece.id)
        });
        if !preserve_preview {
            state.extra.insert("bigRookPreview".into(), Value::Null);
        }
        if truth(state.extra.get("barricadePreview"))
            && state
                .extra
                .get("targeting")
                .and_then(|target| target.get("card"))
                .and_then(|card| card.get("effect"))
                .and_then(Value::as_str)
                != Some("barricade")
        {
            state.extra.insert("barricadePreview".into(), Value::Null);
        }
    }
    let fog = crate::observation::fog_visible_squares_v7(&fog_state, state.turn)?;
    let concealed = |piece: &Piece, at: Square| {
        visibility_state.mode != "gameover"
            && crate::observation::piece_hidden_from_v7(&visibility_state, piece, at)
                == Some(visibility_state.turn)
    };
    let mut scarecrows = BTreeMap::new();
    for entry in render_pending_entries(&display, "pendingScarecrows")? {
        let at = if truth(entry.get("pieceId")) {
            entry.get("pieceId").and_then(Value::as_str).and_then(|id| {
                display.board.iter().enumerate().find_map(|(row, cells)| {
                    cells
                        .iter()
                        .position(|occupant| occupant.as_ref().is_some_and(|piece| piece.id == id))
                        .map(|col| Square {
                            row: row as u8,
                            col: col as u8,
                        })
                })
            })
        } else {
            render_square(entry)
        };
        if let Some(at) = at {
            scarecrows.insert(at, entry);
        }
    }
    let mut lobsters = BTreeMap::new();
    for entry in render_pending_entries(&display, "pendingLobsters")? {
        if let Some(at) = render_square(entry) {
            lobsters.insert(at, entry);
        }
    }
    let mut rendered = Vec::new();
    // 원본 restore는 playMode=local이며 전역 기본 board flip을 유지한다.
    // 일반 칸은 표시 순서를 따르고, overlay의 원본 보드 순회는 뒤집지 않는다.
    let order: Vec<u8> = if state.turn == Color::Black {
        (0..8).rev().collect()
    } else {
        (0..8).collect()
    };
    for row in &order {
        for col in &order {
            let at = Square {
                row: *row,
                col: *col,
            };
            let occupant = display.at(at);
            let fog_hidden = fog.as_ref().is_some_and(|visible| !visible.contains(&at));
            let hidden = occupant.is_some_and(|piece| fog_hidden || concealed(piece, at));
            let scarecrow = scarecrows.get(&at).copied();
            if let Some(piece) = occupant
                && !piece.is_large()
                && !hidden
                && !(truth(piece.extra.get("scarecrowReserved")) && scarecrow.is_some())
            {
                rendered.push(piece.clone());
            }
            if let Some(entry) = scarecrow {
                let preview_visible = if truth(entry.get("pieceId")) {
                    occupant.is_some() && !hidden
                } else {
                    occupant.is_none() || hidden
                };
                if !fog_hidden && preview_visible {
                    rendered.push(pending_render_piece(
                        entry,
                        "scarecrow",
                        "pending-scarecrow",
                    )?);
                }
            }
            if occupant.is_none()
                && let Some(entry) = lobsters.get(&at)
            {
                rendered.push(pending_render_piece(entry, "lobster", "pending-lobster")?);
            }
        }
    }
    for large in [false, true] {
        let mut seen = BTreeSet::new();
        for row in 0..8 {
            for col in 0..8 {
                let at = Square { row, col };
                let Some(piece) = display.at(at) else {
                    continue;
                };
                if concealed(piece, at) || seen.contains(&piece.id) {
                    continue;
                }
                let anchor = if large {
                    if !piece.is_large() {
                        continue;
                    }
                    let position = json!({"row":piece.extra.get("anchorRow"),"col":piece.extra.get("anchorCol")});
                    render_square(&position)
                } else {
                    if piece.id.is_empty() {
                        continue;
                    }
                    crate::v7_quantum_state::quantum_anchor(piece)
                };
                let Some(anchor) = anchor else {
                    continue;
                };
                seen.insert(piece.id.clone());
                let body = crate::v7_quantum_state::quantum_cells_for_item_at(piece, anchor);
                if body.is_empty()
                    || fog
                        .as_ref()
                        .is_some_and(|visible| !body.iter().any(|cell| visible.contains(cell)))
                {
                    continue;
                }
                rendered.push(piece.clone());
            }
        }
    }
    for piece in &rendered {
        crate::card_effects::mark_rendered_piece_animation(state, piece)?;
    }
    Ok(())
}

/// main91686-91782 / 108636-108711. The selection owner has already observed
/// the clicked cell; this branch separately observes every occupied footprint
/// cell before admitting landing captures. The source postpones replay capture
/// until those observations and gates finish.
pub(crate) fn execute_large_piece_move(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
    privacy_before_observation: &Value,
    threat_probe: bool,
) -> Result<Option<StationaryMoveOutcome>> {
    if target.flag("colossusBody") {
        render_board_animation_entries_v7(state)?;
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    let giant = target.flag("colossusMove");
    let rook = target.flag("bigRookMove");
    if !giant && !rook {
        return Ok(None);
    }
    if giant && rook {
        return Err(EngineError::InvalidState(
            "v7 large descriptor selects two movement branches".into(),
        ));
    }
    let row = target
        .flags
        .get("anchorRow")
        .and_then(Value::as_u64)
        .filter(|row| *row < 8);
    let col = target
        .flags
        .get("anchorCol")
        .and_then(Value::as_u64)
        .filter(|col| *col < 8);
    let destination = row
        .zip(col)
        .map(|(row, col)| Square {
            row: row as u8,
            col: col as u8,
        })
        .ok_or_else(|| {
            EngineError::InvalidState("v7 large movement requires an in-bounds anchor".into())
        })?;
    execute_large_piece_move_at(
        state,
        from,
        target,
        destination,
        privacy_before_observation,
        threat_probe,
        if giant {
            LargeMoveKind::Colossus
        } else {
            LargeMoveKind::BigRook
        },
    )
}

#[derive(Clone, Copy)]
enum LargeMoveKind {
    Colossus,
    BigRook,
}

fn execute_large_piece_move_at(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
    destination: Square,
    privacy: &Value,
    threat_probe: bool,
    kind: LargeMoveKind,
) -> Result<Option<StationaryMoveOutcome>> {
    let giant = matches!(kind, LargeMoveKind::Colossus);
    let rook = matches!(kind, LargeMoveKind::BigRook);
    let mut next = state.clone();
    let mut moving = require_origin(&next, from)?;
    let actor = moving.color.owner().ok_or(EngineError::WrongActor)?;
    let footprint = crate::movement::source_large_footprint(&moving, from)?;
    let cells = crate::movement::translated_large_cells(&footprint, destination)
        .ok_or_else(|| EngineError::InvalidState("v7 large movement clips its footprint".into()))?;
    let quantum_candidates = if next.flag("quantumPending", moving.color) {
        let legal = crate::movement::v7_legal_move_targets(
            &next,
            &moving,
            from,
            crate::movement::V7MoveOptions::default(),
        )?;
        crate::v7_quantum_state::candidate_moves_for_move(&next, &moving, destination, &legal)?
    } else {
        Vec::new()
    };
    if cells.iter().any(|cell| {
        crate::movement::v7_scarecrow_reserved_square(&next, *cell)
            && next
                .at(*cell)
                .is_none_or(|occupant| occupant.id != moving.id)
    }) {
        cancel_selection(&mut next)?;
        *state = next;
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    for cell in &cells {
        if next
            .at(*cell)
            .is_none_or(|occupant| occupant.id != moving.id)
        {
            crate::v7_quantum_state::observe_quantum_at_in_place(&mut next, *cell, Some(actor))?;
            refresh_alias(&next, &mut moving);
        }
    }
    let limit = if rook {
        match moving.kind.as_str() {
            "bigBishop" | "big-bishop" => 3,
            "bigRook" | "big-rook" => 2,
            _ => usize::MAX,
        }
    } else {
        usize::MAX
    };
    let landings = crate::movement::v7_large_landing_captures(&next, &moving, &cells, limit, rook)?;
    let locked = crate::v7_capture_reactions::saturation_locked(&next, &moving);
    let Some(landings) = landings.filter(|landings| {
        landings.is_empty() || !crate::v7_threat::manner_capture_locked(&next, &moving) && !locked
    }) else {
        cancel_selection(&mut next)?;
        *state = next;
        return Ok(Some(StationaryMoveOutcome::returned()));
    };
    let before = crate::replay::begin_move(&mut next, actor)?;
    let options = CaptureOptions {
        attacker_landing_cells: Some(cells.clone()),
        saturation_locked: Some(locked),
        threat_probe,
        ..CaptureOptions::default()
    };
    let mut entries = Vec::new();
    for cell in landings {
        if let Some(captured) =
            crate::v7_capture_reactions::capture_at(&mut next, &mut moving, cell, &options)?
        {
            entries.push((captured, cell));
        }
    }
    let mut captured_royal = false;
    for (captured, _) in &mut entries {
        refresh_alias(&next, captured);
        captured_royal |= defeat_royal(&next, captured)?;
    }
    let source = Square {
        row: u8::try_from(footprint.anchor.row)
            .map_err(|_| EngineError::InvalidState("large source anchor outside board".into()))?,
        col: u8::try_from(footprint.anchor.col)
            .map_err(|_| EngineError::InvalidState("large source anchor outside board".into()))?,
    };
    crate::transition::clear_piece(&mut next, &moving.id);
    moving
        .extra
        .insert("anchorRow".into(), json!(destination.row));
    moving
        .extra
        .insert("anchorCol".into(), json!(destination.col));
    for cell in &cells {
        next.board[usize::from(cell.row)][usize::from(cell.col)] = Some(moving.clone());
    }
    let capture = !entries.is_empty();
    let sound = if giant {
        "chessatronMove"
    } else if capture {
        "capture"
    } else if actor == Color::White {
        "moveSelf"
    } else {
        "moveOpponent"
    };
    let hidden = hidden_from_for_move(&next, &moving, destination, privacy)?;
    crate::card_effects::set_last_move(&mut next, source, destination, sound, actor, hidden, None)?;
    crate::v7_quantum_state::clear_quantum_for_piece_in_place(&mut next, &mut moving)?;
    let quantum_to = crate::v7_quantum_state::apply_after_move_in_place(
        &mut next,
        &mut moving,
        destination,
        &quantum_candidates,
    )?;
    let mut trail = vec![source, destination];
    if rook {
        for cell in &cells {
            if !trail.contains(cell) {
                trail.push(*cell);
            }
        }
    }
    if let Some(quantum_to) = quantum_to {
        next.extra
            .get_mut("lastMove")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                EngineError::InvalidState(
                    "v7 large move lost its lastMove before quantum highlight".into(),
                )
            })?
            .insert("quantumTo".into(), json!(quantum_to));
        if !trail.contains(&quantum_to) {
            trail.push(quantum_to);
        }
    }
    crate::card_effects::track_acceleration_trail(&mut next, actor, &trail, false, hidden)?;
    moving.moved = true;
    crate::v7_board_hazards::replace_object_aliases(&mut next, &moving);
    crate::replay::track_moving(&mut next, &moving)?;
    refresh_alias(&next, &mut moving);
    if rook {
        moving
            .extra
            .insert("coolGuyCapturedLast".into(), json!(capture));
    }
    crate::card_effects::note_ultimatum_movement(&mut next, &mut moving)?;
    crate::v7_board_hazards::replace_object_aliases(&mut next, &moving);
    crate::card_effects::mark_animation(&mut next, &moving)?;
    if rook {
        crate::v7_threat::play_move_sound_v7(&mut next, sound, actor)?;
    }
    if giant {
        moving
            .extra
            .insert("coolGuyCapturedLast".into(), json!(false));
        crate::v7_board_hazards::replace_object_aliases(&mut next, &moving);
    }
    next.en_passant = None;
    let game_end = next.mode == "gameover" && captured_royal;
    crate::replay::queue_v7_move_notation(
        &mut next,
        &moving,
        source,
        destination,
        target,
        crate::replay::MoveNotationVisibility {
            privacy,
            capture,
            capture_known_squares: &capture_known_squares(&entries),
        },
        game_end,
    )?;
    let log = swap_move_log(&next, &moving, source, destination, privacy)?;
    crate::replay::add_log(&mut next, log)?;
    crate::v7_threat::resolve_herald_threats_v7_with_probe(&mut next, actor, threat_probe)?;
    let captures = entries.into_iter().map(|(piece, _)| piece).collect();
    let mut outcome = if next.mode == "gameover" {
        next.pending_colossus_actor = None;
        StationaryMoveOutcome {
            captures,
            completion: StationaryCompletion::RecordTerminal,
            replay_before: None,
        }
    } else if crate::v7_move_continuations::try_start_reposition_second_move_v7(
        &mut next,
        &mut moving,
        destination,
    )? || crate::v7_move_continuations::try_start_platform_for_piece_v7(
        &mut next, &moving.id,
    )? {
        StationaryMoveOutcome {
            captures,
            completion: StationaryCompletion::Return,
            replay_before: None,
        }
    } else if giant {
        delay_colossus_turn(
            &mut next,
            actor,
            "메가체스트론 이동",
            String::new(),
            captures,
        )?
    } else {
        StationaryMoveOutcome::end_move(actor, "ROOK 이동", String::new(), captures)
    };
    if matches!(outcome.completion, StationaryCompletion::EndMove { .. }) {
        outcome.replay_before = Some(Box::new(before));
    }
    *state = next;
    Ok(Some(outcome))
}

/// main92605. The firstMoveUndo and beginMoveReplayCapture boundary is owned
/// here after selection. Large-rook castle crushing calls only Reaper nearby
/// deaths; it does not count the crushed allies as ordinary captures.
pub(crate) fn execute_castle(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
    privacy: &Value,
    threat_probe: bool,
) -> Result<Option<StationaryMoveOutcome>> {
    if !target.flag("castle") {
        return Ok(None);
    }
    let mut next = state.clone();
    let mut king = require_origin(&next, from)?;
    let actor = king.color.owner().ok_or(EngineError::WrongActor)?;
    let to = target.square();
    let rook_from = optional_descriptor_square(target, "rookFrom")?
        .ok_or_else(|| EngineError::InvalidState("v7 castle requires rookFrom".into()))?;
    let rook_to = optional_descriptor_square(target, "rookTo")?
        .ok_or_else(|| EngineError::InvalidState("v7 castle requires rookTo".into()))?;
    let mut rook = next
        .at(rook_from)
        .cloned()
        .ok_or_else(|| EngineError::InvalidState("v7 castle rook is absent".into()))?;
    if rook.id.is_empty() {
        return Err(EngineError::InvalidState(
            "v7 castle rook needs a stable identity".into(),
        ));
    }
    let before = crate::replay::begin_move(&mut next, actor)?;
    let mut trail = vec![from, to];
    for field in ["portalEntry", "portalExit"] {
        if let Some(cell) = optional_descriptor_square(target, field)?
            && !trail.contains(&cell)
        {
            trail.push(cell);
        }
    }
    if target.flag("bigRookCastle") && rook.kind == "bigRook" {
        if rook_to.row >= 7 || rook_to.col >= 7 {
            return Err(EngineError::InvalidState(
                "v7 big-rook castle anchor clips its footprint".into(),
            ));
        }
        let cells = crate::v7_quantum_state::quantum_cells_for_item_at(&rook, rook_to);
        crate::transition::clear_piece(&mut next, &rook.id);
        next.board[usize::from(from.row)][usize::from(from.col)] = None;
        next.board[usize::from(to.row)][usize::from(to.col)] = Some(king.clone());
        let mut crushed = Vec::new();
        for cell in &cells {
            if let Some(occupant) = next.at(*cell).cloned()
                && occupant.id != king.id
                && occupant.id != rook.id
                && occupant.color == king.color
            {
                if occupant.is_large() {
                    crate::transition::clear_piece(&mut next, &occupant.id);
                } else {
                    next.board[usize::from(cell.row)][usize::from(cell.col)] = None;
                }
                crushed.push(crate::v7_board_hazards::EnvironmentalRemoval {
                    piece: occupant,
                    square: *cell,
                    capture_owner: actor,
                });
            }
        }
        place_large_swap(&mut next, &mut rook, rook_to);
        crate::v7_board_hazards::resolve_reaper_nearby_deaths(&mut next, &crushed)?;
        refresh_alias(&next, &mut king);
        refresh_alias(&next, &mut rook);
        for cell in cells {
            if !trail.contains(&cell) {
                trail.push(cell);
            }
        }
    } else {
        next.board[usize::from(to.row)][usize::from(to.col)] = Some(king.clone());
        next.board[usize::from(from.row)][usize::from(from.col)] = None;
        next.board[usize::from(rook_to.row)][usize::from(rook_to.col)] = Some(rook.clone());
        next.board[usize::from(rook_from.row)][usize::from(rook_from.col)] = None;
        for cell in [rook_from, rook_to] {
            if !trail.contains(&cell) {
                trail.push(cell);
            }
        }
    }
    let hidden = hidden_from_for_move(&next, &king, to, privacy)?;
    crate::card_effects::set_last_move(&mut next, from, to, "castle", actor, hidden, None)?;
    crate::card_effects::track_acceleration_trail(&mut next, actor, &trail, false, hidden)?;
    crate::card_effects::mark_animation(&mut next, &king)?;
    crate::card_effects::mark_animation(&mut next, &rook)?;
    crate::v7_threat::play_move_sound_v7(&mut next, "castle", actor)?;
    king.moved = true;
    rook.moved = true;
    crate::v7_board_hazards::replace_object_aliases(&mut next, &king);
    crate::v7_board_hazards::replace_object_aliases(&mut next, &rook);
    crate::replay::track_moving(&mut next, &king)?;
    refresh_alias(&next, &mut king);
    refresh_alias(&next, &mut rook);
    if !truth(next.extra.get("castled")) {
        next.extra
            .insert("castled".into(), json!({"white":false,"black":false}));
    }
    next.set_flag("castled", actor, true);
    if next.flag("zugzwang", actor) {
        next.set_flag("zugzwang", actor, false);
    }
    crate::card_effects::note_ultimatum_movement(&mut next, &mut king)?;
    crate::card_effects::note_ultimatum_movement(&mut next, &mut rook)?;
    crate::v7_board_hazards::replace_object_aliases(&mut next, &king);
    crate::v7_board_hazards::replace_object_aliases(&mut next, &rook);
    next.en_passant = None;
    crate::replay::queue_v7_move_notation_with_options(
        &mut next,
        &king,
        from,
        to,
        target,
        &crate::replay::V7MoveNotationOptions {
            piece_type: &king.kind,
            disambiguation: "",
            promotion: None,
            privacy,
            capture: false,
            capture_known_squares: &json!({}),
            game_end: false,
        },
    )?;
    let castle_name = target
        .flags
        .get("castle")
        .and_then(Value::as_str)
        .unwrap_or("true");
    movement_log(
        &mut next,
        actor,
        format!("{} 캐슬링: {castle_name}", crate::replay::label(actor)),
    )?;
    crate::v7_threat::resolve_herald_threats_v7_with_probe(&mut next, actor, threat_probe)?;
    crate::v7_threat::check_racing_kings_v7(&mut next)?;
    let mut outcome = if next.mode == "gameover"
        || crate::v7_move_continuations::try_start_reposition_second_move_v7(
            &mut next, &mut king, to,
        )?
        || crate::v7_move_continuations::try_start_platform_for_piece_v7(&mut next, &king.id)?
        || crate::v7_move_continuations::try_start_platform_for_piece_v7(&mut next, &rook.id)?
    {
        StationaryMoveOutcome::returned()
    } else {
        StationaryMoveOutcome::end_move(actor, "캐슬링", String::new(), Vec::new())
    };
    if matches!(outcome.completion, StationaryCompletion::EndMove { .. }) {
        outcome.replay_before = Some(Box::new(before));
    }
    *state = next;
    Ok(Some(outcome))
}

fn fog_log_redaction(state: &GameState) -> bool {
    matches!(
        state
            .extra
            .get("campaign")
            .and_then(|campaign| campaign.get("setup"))
            .and_then(Value::as_str),
        Some("fogWar" | "fog")
    ) || truth(state.extra.get("fogWar"))
        || truth(state.extra.get("fogOfWar"))
        || truth(state.extra.get("fog").and_then(|fog| fog.get("enabled")))
}

fn movement_log(state: &mut GameState, actor: Color, message: String) -> Result<()> {
    crate::replay::add_log(
        state,
        if fog_log_redaction(state) {
            format!("{} 기물이 이동했습니다.", crate::replay::label(actor))
        } else {
            message
        },
    )
}

/// main90919. Football executes before generic privacy/undo/selection. The
/// neutral ball keeps its identity/color; state.turn owns capture attribution.
/// HP/shield hits leave the ball in place, never queue ordinary move notation,
/// and do not start replay capture. Explosive landings have no extra explosion
/// callback in this source branch.
pub(crate) fn execute_football_move(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
    threat_probe: bool,
) -> Result<Option<StationaryMoveOutcome>> {
    let Some(mut ball) = state
        .at(from)
        .cloned()
        .filter(|piece| piece.kind == "football")
    else {
        return Ok(None);
    };
    if state.ruleset_id != RULES_VERSION_V7
        || state.board.len() != 8
        || state.board.iter().any(|row| row.len() != 8)
    {
        return Err(EngineError::InvalidState(
            "v7 Football execution requires the pinned 8x8 board".into(),
        ));
    }
    if ball.id.is_empty() {
        return Err(EngineError::InvalidState(
            "v7 Football requires a stable object identity".into(),
        ));
    }
    let mut next = state.clone();
    let actor = next.turn;
    let to = target.square();
    if crate::v7_queued_effects::crown_ground_at(&next, to) {
        cancel_selection(&mut next)?;
        *state = next;
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    if crate::movement::v7_scarecrow_reserved_square(&next, to) && next.at(to).is_none() {
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    crate::v7_quantum_state::observe_quantum_at_in_place(&mut next, to, Some(actor))?;
    refresh_alias(&next, &mut ball);
    let victim = next.at(to).cloned();
    let victim_privacy = victim
        .as_ref()
        .map(|victim| privacy_snapshot(&next, victim, to))
        .transpose()?;
    if let Some(victim) = &victim {
        let protected = truth(victim.extra.get("protected"))
            || crate::card_effects::js_number(
                victim
                    .extra
                    .get("vigilanceProtection")
                    .and_then(|value| value.get("remaining")),
                0,
            )
            .is_some_and(|remaining| remaining > 0.0);
        if protected
            || crate::movement::v7_encouraged_at(&next, victim, to)
            || !crate::movement::v7_can_capture_target_as(
                &next, &ball, victim, actor, false, false,
            )?
        {
            *state = next;
            return Ok(Some(StationaryMoveOutcome::returned()));
        }
    }
    let options = CaptureOptions {
        threat_probe,
        saturation_locked: Some(crate::v7_capture_reactions::saturation_locked(&next, &ball)),
        ..CaptureOptions::default()
    };
    if let Some(mut victim) = victim
        .as_ref()
        .filter(|victim| truth(victim.extra.get("shielded")))
        .cloned()
    {
        crate::v7_capture_reactions::break_initiative_by_attack(&mut next, &victim, actor)?;
        crate::v7_capture_reactions::break_shield(&mut next, &mut victim, actor)?;
        crate::replay::add_log(
            &mut next,
            format!("{}의 보호가 축구공을 막았습니다.", square_name(to)),
        )?;
        next.en_passant = None;
        *state = next;
        return Ok(Some(StationaryMoveOutcome::end_move(
            actor,
            "보호 발동",
            "축구공 공격이 막혔습니다.".into(),
            Vec::new(),
        )));
    }
    if let Some(victim) = &victim
        && crate::v7_capture_reactions::is_hp_piece(&next, victim)
    {
        crate::v7_capture_reactions::break_initiative_by_attack(&mut next, victim, actor)?;
        let removed = crate::v7_capture_reactions::damage_health_piece_with_optional_attacker(
            &mut next,
            to,
            actor,
            Some(&mut ball),
            "축구공",
            &options,
        )?;
        next.en_passant = None;
        let captures = removed.into_iter().collect();
        *state = next;
        return Ok(Some(StationaryMoveOutcome::end_move(
            actor,
            "HP 피해",
            String::new(),
            captures,
        )));
    }
    let before = crate::replay::begin_move(&mut next, actor)?;
    let removed = if victim.is_some() {
        let options = CaptureOptions {
            attacker_landing: Some(to),
            ..options
        };
        crate::v7_capture_reactions::capture_at_with_optional_attacker(
            &mut next,
            to,
            actor,
            Some(&mut ball),
            &options,
        )?
    } else {
        None
    };
    next.board[usize::from(to.row)][usize::from(to.col)] = Some(ball.clone());
    next.board[usize::from(from.row)][usize::from(from.col)] = None;
    ball.moved = true;
    crate::v7_board_hazards::replace_object_aliases(&mut next, &ball);
    crate::card_effects::mark_animation(&mut next, &ball)?;
    let black_hole =
        removed.is_none() && crate::movement::v7_black_hole_cells(&next)?.contains(&to);
    let sound = if removed.is_some() || black_hole {
        "capture"
    } else if actor == Color::White {
        "moveSelf"
    } else {
        "moveOpponent"
    };
    crate::card_effects::set_last_move(&mut next, from, to, sound, actor, "", None)?;
    crate::card_effects::track_acceleration_trail(&mut next, actor, &[from, to], false, "")?;
    let message = if let Some(captured) = &removed {
        let concealed = captured.color.owner().is_some()
            && !hidden_from_for_move(
                &next,
                captured,
                to,
                victim_privacy.as_ref().unwrap_or(&Value::Null),
            )?
            .is_empty();
        if concealed {
            "축구공이 이동했습니다.".into()
        } else {
            format!(
                "축구공: {} -> {} ({} 포획)",
                square_name(from),
                square_name(to),
                crate::replay::source_piece_label(&captured.kind).unwrap_or(&captured.kind)
            )
        }
    } else {
        format!("축구공: {} -> {}", square_name(from), square_name(to))
    };
    movement_log(&mut next, actor, message)?;
    let mut outcome = StationaryMoveOutcome::end_move(
        actor,
        "축구공 이동",
        String::new(),
        removed.into_iter().collect(),
    );
    outcome.replay_before = Some(Box::new(before));
    *state = next;
    Ok(Some(outcome))
}

/// main108809. 동결 headless source는 타이머를 실행하지 않는다. live host만
/// 원본 500ms callback의 state 동일성 검사 후 complete_colossus_turn을 호출한다.
fn delay_colossus_turn(
    state: &mut GameState,
    actor: Color,
    title: &'static str,
    message: String,
    captures: Vec<Piece>,
) -> Result<StationaryMoveOutcome> {
    state.pending_colossus_actor = None;
    if state.free_move_resolution == Some(actor) {
        state.extra.insert("turnResolving".into(), json!(false));
        return Ok(StationaryMoveOutcome::end_move(
            actor, title, message, captures,
        ));
    }
    if state.is_ai_simulation() {
        return Ok(StationaryMoveOutcome::end_move(
            actor, title, message, captures,
        ));
    }
    state.extra.insert("turnResolving".into(), json!(true));
    for field in ["selected", "targeting", "wizardSpell"] {
        state.extra.insert(field.into(), Value::Null);
    }
    for field in ["legalMoves", "wizardPreview", "shotgunPreview"] {
        state.extra.insert(field.into(), json!([]));
    }
    state.extra.insert("shotgunAction".into(), json!("move"));
    render_board_animation_entries_v7(state)?;
    // source는 renderBoard 성공 뒤 실제 setTimeout을 예약한다. private owner도
    // 이 지점에서만 생성하며 snapshot의 turnResolving으로 복원하지 않는다.
    state.pending_colossus_actor = Some(actor);
    Ok(StationaryMoveOutcome {
        captures,
        completion: StationaryCompletion::Return,
        replay_before: None,
    })
}

/// 호출자는 대기 중인 원본 state와 같은 인스턴스인지를 먼저 확인한다.
/// 동일하지 않으면 원본처럼 이 함수 자체를 호출하지 않으며 turnResolving도
/// 변경하지 않는다. 동일하면 flag를 먼저 해제하고, 기존 actor의 턴이 계속되는
/// 비종료 상태에서만 반환된 EndMove를 공통 finisher에 한 번 전달한다.
pub(crate) fn complete_colossus_turn(
    state: &mut GameState,
    actor: Color,
) -> Result<StationaryMoveOutcome> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::InvalidState(
            "v7 colossus timer completion requires the pinned ruleset".into(),
        ));
    }
    if state.pending_colossus_actor != Some(actor) {
        return Err(EngineError::InvalidState(format!(
            "v7 colossus completion has no pending timer for {}",
            actor.as_str()
        )));
    }
    state.pending_colossus_actor = None;
    state.extra.insert("turnResolving".into(), json!(false));
    Ok(if state.mode != "gameover" && state.turn == actor {
        StationaryMoveOutcome::end_move(actor, "메가체스트론", String::new(), Vec::new())
    } else {
        StationaryMoveOutcome::returned()
    })
}

/// main108711. Attack admission collects source references before damage;
/// HP identities may repeat once per covered body cell. During the attack,
/// source re-reads each cell instead of using its collected victim snapshot.
pub(crate) fn execute_colossus_sector(
    state: &mut GameState,
    from: Square,
    target: &MoveTarget,
    threat_probe: bool,
) -> Result<Option<StationaryMoveOutcome>> {
    if !target.flag("colossusAttack") {
        return Ok(None);
    }
    let mut next = state.clone();
    let mut attacker = require_origin(&next, from)?;
    let actor = attacker.color.owner().ok_or(EngineError::WrongActor)?;
    let anchor = large_anchor(&attacker)?;
    let privacy = privacy_snapshot(&next, &attacker, anchor)?;
    let cells = descriptor_cells(target, "sectorCells")?;
    let locked = crate::v7_capture_reactions::saturation_locked(&next, &attacker);
    if locked {
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    let mut has_enemy = false;
    for cell in &cells {
        let candidate = next
            .at(*cell)
            .cloned()
            .or(crate::v7_quantum_state::find_quantum_at(&next, *cell)?.map(|ghost| ghost.piece));
        if let Some(candidate) = candidate
            && crate::variant_movement::colossus_sector_target_allowed_v7(
                &next, &attacker, &candidate, *cell,
            )?
        {
            has_enemy = true;
            break;
        }
    }
    if !has_enemy {
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    let mut collected = Vec::new();
    let mut seen = BTreeSet::new();
    for cell in &cells {
        let candidate = next
            .at(*cell)
            .cloned()
            .or(crate::v7_quantum_state::find_quantum_at(&next, *cell)?.map(|ghost| ghost.piece));
        if let Some(candidate) = candidate
            && crate::variant_movement::colossus_sector_target_allowed_v7(
                &next, &attacker, &candidate, *cell,
            )?
        {
            crate::v7_quantum_state::observe_quantum_at_in_place(&mut next, *cell, Some(actor))?;
            refresh_alias(&next, &mut attacker);
            if let Some(victim) = next.at(*cell).cloned()
                && crate::variant_movement::colossus_sector_target_allowed_v7(
                    &next, &attacker, &victim, *cell,
                )?
                && (crate::v7_capture_reactions::is_hp_piece(&next, &victim)
                    && !truth(victim.extra.get("shielded"))
                    || seen.insert(victim.id.clone()))
            {
                collected.push((*cell, victim));
            }
        }
    }
    if crate::v7_threat::manner_capture_locked(&next, &attacker) && !collected.is_empty() {
        *state = next;
        return Ok(Some(StationaryMoveOutcome::returned()));
    }
    let options = CaptureOptions {
        saturation_locked: Some(locked),
        threat_probe,
        ..CaptureOptions::default()
    };
    let mut entries = Vec::new();
    for (cell, _) in &collected {
        let Some(mut victim) = next.at(*cell).cloned() else {
            continue;
        };
        if truth(victim.extra.get("shielded")) {
            crate::v7_capture_reactions::break_initiative_by_attack(&mut next, &victim, actor)?;
            crate::v7_capture_reactions::break_shield(&mut next, &mut victim, actor)?;
            continue;
        }
        let removed = if crate::v7_capture_reactions::is_hp_piece(&next, &victim) {
            crate::v7_capture_reactions::break_initiative_by_attack(&mut next, &victim, actor)?;
            crate::v7_capture_reactions::damage_health_piece_with_optional_attacker(
                &mut next,
                *cell,
                actor,
                Some(&mut attacker),
                "거신병 섹터 공격",
                &options,
            )?
        } else {
            let removed =
                crate::v7_capture_reactions::capture_at(&mut next, &mut attacker, *cell, &options)?;
            if removed
                .as_ref()
                .is_some_and(|victim| truth(victim.extra.get("explosive")))
            {
                crate::v7_board_hazards::explode_at(&mut next, *cell, "자폭병")?;
                refresh_alias(&next, &mut attacker);
            }
            removed
        };
        if let Some(removed) = removed {
            entries.push((removed, *cell));
        }
        if next.mode == "gameover" {
            break;
        }
    }
    let count = entries.len();
    attacker
        .extra
        .insert("coolGuyCapturedLast".into(), json!(count > 0));
    crate::v7_board_hazards::replace_object_aliases(&mut next, &attacker);
    crate::card_effects::mark_animation(&mut next, &attacker)?;
    crate::v7_threat::play_move_sound_v7(
        &mut next,
        if count > 0 {
            "chessatronCatch"
        } else {
            "chessatronMove"
        },
        actor,
    )?;
    next.en_passant = None;
    let divisor = cells.len().max(1) as f64;
    let center_row = cells.iter().map(|cell| f64::from(cell.row)).sum::<f64>() / divisor;
    let center_col = cells.iter().map(|cell| f64::from(cell.col)).sum::<f64>() / divisor;
    let sign = |delta: f64| {
        if delta < 0.0 {
            -1
        } else if delta > 0.0 {
            1
        } else {
            0
        }
    };
    let direction = notation_direction([
        sign(center_row - f64::from(anchor.row)),
        sign(center_col - f64::from(anchor.col)),
    ]);
    let mut royal = false;
    for (_, piece) in &collected {
        let mut current = piece.clone();
        refresh_alias(&next, &mut current);
        royal |= crate::v7_board_hazards::source_royal_king(&next, &current)?;
    }
    let ending = if next.mode == "gameover" && royal {
        "#"
    } else {
        ""
    };
    crate::replay::queue_special_move_notation(
        &mut next,
        &attacker,
        anchor,
        format!(
            "{}⇢{direction}{}{}",
            crate::replay::piece_code(RULES_VERSION_V7, &attacker.kind),
            if count > 0 {
                format!("×{count}")
            } else {
                String::new()
            },
            ending
        ),
        format!(
            "{} 거신병 {direction} 섹터 공격, 제거 {count}개",
            crate::replay::label(actor)
        ),
        crate::replay::MoveNotationVisibility {
            privacy: &privacy,
            capture: count > 0,
            capture_known_squares: &capture_known_squares(&entries),
        },
    )?;
    crate::replay::add_piece_action_log(
        &mut next,
        &attacker,
        Some(anchor),
        Some(&privacy),
        format!(
            "{} 거신병이 섹터를 공격했습니다.",
            crate::replay::label(actor)
        ),
    )?;
    let captures = entries.into_iter().map(|(piece, _)| piece).collect();
    let outcome = if next.mode == "gameover" {
        next.pending_colossus_actor = None;
        StationaryMoveOutcome {
            captures,
            completion: StationaryCompletion::RecordTerminal,
            replay_before: None,
        }
    } else {
        delay_colossus_turn(
            &mut next,
            actor,
            "메가체스트론 공격",
            format!("{count}개의 기물이 제거되었습니다."),
            captures,
        )?
    };
    *state = next;
    Ok(Some(outcome))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;
    use crate::tests::source_callback_fixture::{
        collect_case_diagnostics, compare_callback_envelope, compare_value, source_callback_state,
    };
    use sha2::{Digest, Sha256};

    fn state(kind: &str) -> GameState {
        let mut state = GameState::new(GameConfig::default(), 19).expect("state");
        state.ruleset_id = RULES_VERSION_V7.into();
        state.mode = "play".into();
        state.turn = Color::White;
        state.board[4][4] = Some(Piece::new(kind, Color::White, "mover"));
        state
    }

    #[test]
    fn large_move_places_exact_translated_footprint() {
        for displaced_alias in [false, true] {
            let mut state = state("bigRook");
            state.board = vec![vec![None; 8]; 8];
            let from = Square { row: 3, col: 3 };
            let to = Square { row: 2, col: 3 };
            let mut piece = Piece::new("bigRook", Color::White, "mover");
            piece.extra.insert("anchorRow".into(), json!(from.row));
            piece.extra.insert("anchorCol".into(), json!(from.col));
            for row in 3..=4 {
                for col in 3..=4 {
                    state.board[row][col] = Some(piece.clone());
                }
            }
            if displaced_alias {
                state.board[4][4] = None;
                state.board[6][0] = Some(piece.clone());
            }
            let footprint = crate::movement::source_large_footprint(&piece, from).unwrap();
            assert_eq!(footprint.footprint.len(), 4);
            assert!(!footprint.footprint.contains(&crate::Offset::new(3, -3)));
            let target = crate::movement::v7_large_moves(&state, &piece, from)
                .unwrap()
                .into_iter()
                .find(|target| target.square() == to)
                .unwrap();
            assert_eq!(target.flags["highlightCells"].as_array().unwrap().len(), 4);
            let action = crate::Action::movement(Color::White, from, target.clone());
            assert_eq!(
                crate::movement::v7_public_move_intents(&state, &action)
                    .unwrap()
                    .len(),
                1
            );
            let privacy = privacy_snapshot(&state, &piece, from).unwrap();
            execute_large_piece_move(&mut state, from, &target, &privacy, false).unwrap();
            let occupied = state
                .board
                .iter()
                .enumerate()
                .flat_map(|(row, cells)| {
                    cells.iter().enumerate().filter_map(move |(col, item)| {
                        item.as_ref()
                            .filter(|item| item.id == "mover")
                            .map(|_| Square {
                                row: row as u8,
                                col: col as u8,
                            })
                    })
                })
                .collect::<Vec<_>>();
            assert_eq!(
                occupied,
                vec![
                    Square { row: 2, col: 3 },
                    Square { row: 2, col: 4 },
                    Square { row: 3, col: 3 },
                    Square { row: 3, col: 4 },
                ]
            );
            for cell in occupied {
                let moved = state.at(cell).unwrap();
                assert_eq!(moved.extra["anchorRow"], json!(2));
                assert_eq!(moved.extra["anchorCol"], json!(3));
            }
            assert!(state.board[6][0].is_none());
        }
    }

    #[test]
    fn malformed_direction_reports_exact_boundary_without_mutation() {
        let mut state = state("log");
        let before = state.clone();
        let mut target = MoveTarget::at(Square { row: 3, col: 4 });
        target
            .flags
            .insert("setLogDirection".into(), json!({"dr":0.5,"dc":0}));
        let error =
            execute_log_direction(&mut state, Square { row: 4, col: 4 }, &target).unwrap_err();
        assert!(matches!(error, EngineError::InvalidState(ref detail)
            if detail.contains("setLogDirection") && detail.contains("integer deltas")));
        assert_eq!(state, before);
    }

    #[test]
    fn invalid_ammo_reports_exact_boundary_without_mutation() {
        let mut state = state("shotgunKing");
        state.board[4][4]
            .as_mut()
            .expect("mover")
            .extra
            .insert("ammo".into(), json!("invalid"));
        let before = state.clone();
        let mut target = MoveTarget::at(Square { row: 3, col: 4 });
        target.flags.insert("shotgunBlast".into(), json!(true));
        target
            .flags
            .insert("shotgunDirection".into(), json!([-1, 0]));
        let error =
            execute_shotgun(&mut state, Square { row: 4, col: 4 }, &target, false).unwrap_err();
        assert!(matches!(error, EngineError::InvalidState(ref detail)
            if detail.contains("shotgun ammo") && detail.contains("finite source number")));
        assert_eq!(state, before);
    }

    fn source_state_value(state: &GameState) -> Value {
        let mut value = serde_json::to_value(state).expect("state JSON");
        let fields = value.as_object_mut().expect("state object");
        for key in ["rulesetId", "rng", "history"] {
            fields.remove(key);
        }
        value
    }

    fn source_envelope(state: &Value, rng: &Value) -> Result<Value> {
        let content = json!({
            "protocolVersion":crate::v7_host::V7_POSITION_PROTOCOL,
            "rulesVersion":RULES_VERSION_V7,
            "catalogVersion":crate::v7_execution_profile::catalog_version()?,
            "state":state,"rng":rng,"history":[],
        });
        let digest = format!(
            "{:x}",
            Sha256::digest(
                serde_jcs::to_vec(&content)
                    .map_err(|error| EngineError::Serialization(error.to_string()))?
            )
        );
        let mut envelope = content
            .as_object()
            .expect("source callback envelope object")
            .clone();
        envelope.insert("positionId".into(), json!(digest));
        Ok(Value::Object(envelope))
    }

    /// 원시 before/after 자료는 외부 reports에 둔다. 파일이나 env가 없으면
    /// 검사를 통과로 취급하지 않으며 메인의 명시적 --ignored 실행이 필요하다.
    #[test]
    #[ignore = "requires ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS with pinned source receipts"]
    fn frozen_stationary_callbacks_when_source_receipts_supplied() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS")
                .expect("source receipt directory"),
        );
        let mut mismatches = Vec::new();
        for name in [
            "log-direction",
            "log-direction-hidden",
            "shotgun-blast-empty",
            "shotgun-blast-line",
            "shotgun-blast-protected",
            "shotgun-blast-encouraged",
            "shotgun-blast-saturation",
            "shotgun-blast-health",
            "shotgun-blast-explosive",
            "shotgun-blast-quantum",
            "shotgun-blast-facing",
            "shotgun-blast-manner-empty",
            "shotgun-blast-manner-target",
            "shotgun-snipe",
            "shotgun-snipe-shield",
            "shotgun-snipe-health",
            "shotgun-snipe-royal",
            "shotgun-snipe-hidden",
            "shotgun-selection-friendly",
            "shotgun-selection-quantum",
            "shotgun-selection-own-quantum",
            "shotgun-selection-thief-flag",
        ] {
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                let (mut state, from, target, receipt) = load_special_receipt(&root, name)?;
                let outcome = if name.starts_with("log-") {
                    crate::v7_card_context::begin_board_action(&mut state);
                    execute_log_direction(&mut state, from, &target).expect(name)
                } else if name.starts_with("shotgun-selection-") {
                    crate::v7_card_context::begin_board_action(&mut state);
                    prepare_shotgun_selection(&mut state, from, &target)
                        .expect(name)
                        .or_else(|| execute_shotgun(&mut state, from, &target, false).expect(name))
                } else {
                    execute_shotgun(&mut state, from, &target, false).expect(name)
                }
                .expect("stationary branch");
                crate::transition::finish_stationary_move(&mut state, outcome).expect(name);
                assert_special_receipt(&root, name, &state, &receipt, mismatches)
            });
        }
        assert!(
            mismatches.is_empty(),
            "source stationary callback differences:\n{}",
            mismatches.join("\n")
        );
    }

    #[test]
    #[ignore = "requires ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS with pinned source receipts"]
    fn frozen_missionary_callbacks_when_source_receipts_supplied() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS")
                .expect("source receipt directory"),
        );
        let mut mismatches = Vec::new();
        for name in [
            "missionary-conversion",
            "missionary-protected",
            "missionary-large-health",
            "missionary-royal",
            "missionary-quantum-illusion",
            "missionary-thief-flag",
            "missionary-moving-no-exhaustion",
            "missionary-hidden",
            "missionary-fresh-origin",
            "missionary-desperado-royal",
        ] {
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                let (mut state, from, target, receipt) = load_special_receipt(&root, name)?;
                crate::v7_card_context::begin_board_action(&mut state);
                let mut moving = state.at(from).cloned().expect("source mover");
                let privacy = privacy_snapshot(&state, &moving, from).expect(name);
                let observation = crate::v7_quantum_state::observe_move_landing_in_place(
                    &mut state,
                    &mut moving,
                    target.square(),
                    None,
                )
                .expect(name);
                moving.extra.shift_remove("thiefSecondMove");
                moving
                    .source_order
                    .retain(|field| field != "thiefSecondMove");
                crate::v7_board_hazards::replace_object_aliases(&mut state, &moving);
                // These fixtures force the opening card already resolved; the
                // selection owner's firstMoveUndo branch is intentionally inactive.
                let outcome = execute_missionary_conversion(
                    &mut state,
                    from,
                    &target,
                    &privacy,
                    observation.captured_illusion(),
                    false,
                )
                .expect(name)
                .expect("missionary branch");
                assert!(
                    !matches!(outcome.completion, StationaryCompletion::RecordTerminal),
                    "missionary source does not record terminal history here"
                );
                crate::transition::finish_stationary_move(&mut state, outcome).expect(name);
                assert_special_receipt(&root, name, &state, &receipt, mismatches)
            });
        }
        assert!(
            mismatches.is_empty(),
            "source missionary callback differences:\n{}",
            mismatches.join("\n")
        );
    }

    #[test]
    #[ignore = "requires ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS with pinned source receipts"]
    fn frozen_swap_callbacks_when_source_receipts_supplied() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS")
                .expect("source receipt directory"),
        );
        let mut mismatches = Vec::new();
        for name in [
            "dragon-swap",
            "dragon-swap-hidden",
            "dragon-swap-quantum",
            "dragon-swap-monochrome-blocked",
            "dragon-swap-monochrome",
            "dragon-swap-twin-ultimatum",
            "dragon-swap-trickster",
            "dragon-swap-parrot",
            "dragon-swap-racing",
            "substitution-swap",
            "substitution-swap-large",
            "substitution-swap-monochrome",
            "substitution-swap-quantum-clear",
            "substitution-swap-disassembly",
            "substitution-swap-desperado",
            "relay-swap",
            "relay-swap-solidarity",
            "relay-swap-disassembly",
            "relay-swap-thief",
            "relay-swap-quantum-pending",
            "relay-swap-no-moving-progress",
        ] {
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                let (mut state, from, target, receipt) = load_special_receipt(&root, name)?;
                crate::v7_card_context::begin_board_action(&mut state);
                let mut moving = state.at(from).cloned().expect("source mover");
                crate::v7_quantum_state::observe_move_landing_in_place(
                    &mut state,
                    &mut moving,
                    target.square(),
                    None,
                )
                .expect(name);
                moving.extra.shift_remove("thiefSecondMove");
                moving
                    .source_order
                    .retain(|field| field != "thiefSecondMove");
                crate::v7_board_hazards::replace_object_aliases(&mut state, &moving);
                let outcome = execute_position_swap(&mut state, from, &target, false)
                    .expect(name)
                    .expect("swap branch");
                crate::transition::finish_stationary_move(&mut state, outcome).expect(name);
                assert_special_receipt(&root, name, &state, &receipt, mismatches)
            });
        }
        assert!(
            mismatches.is_empty(),
            "source swap callback differences:\n{}",
            mismatches.join("\n")
        );
    }

    #[test]
    #[ignore = "requires ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS with pinned source receipts"]
    fn frozen_purchase_pull_and_sector_when_source_receipts_supplied() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS")
                .expect("source receipt directory"),
        );
        let mut mismatches = Vec::new();
        for name in [
            "merchant-purchase",
            "merchant-purchase-hidden",
            "merchant-target-hidden",
            "merchant-purchase-royal",
            "merchant-purchase-democracy",
            "merchant-purchase-regency",
            "merchant-purchase-large",
            "merchant-purchase-shield",
            "merchant-purchase-frozen",
            "merchant-purchase-manner",
            "merchant-purchase-poor",
            "merchant-purchase-en-passant",
            "grappler-pull",
            "grappler-pull-blocked",
            "grappler-pull-shield",
            "grappler-pull-large",
            "colossus-sector",
            "colossus-sector-empty",
            "colossus-sector-protected",
            "colossus-sector-saturation",
            "colossus-sector-health-repeat",
            "colossus-sector-royal",
            "colossus-sector-free-move",
        ] {
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                let (mut state, from, target, receipt) = load_special_receipt(&root, name)?;
                if name == "colossus-sector-free-move" {
                    state.free_move_resolution = Some(Color::White);
                }
                crate::v7_card_context::begin_board_action(&mut state);
                let outcome =
                    match prepare_v7_move_selection(&mut state, from, &target).expect(name) {
                        V7MoveSelection::Return(outcome) => outcome,
                        V7MoveSelection::Ready(prelude) => {
                            let mut moving = prelude.moving;
                            moving.extra.shift_remove("thiefSecondMove");
                            moving
                                .source_order
                                .retain(|field| field != "thiefSecondMove");
                            crate::v7_board_hazards::replace_object_aliases(&mut state, &moving);
                            if name.starts_with("merchant-") {
                                execute_merchant_purchase(&mut state, from, &target, false)
                            } else if name.starts_with("grappler-") {
                                execute_grappler_pull(&mut state, from, &target)
                            } else {
                                execute_colossus_sector(&mut state, from, &target, false)
                            }
                            .expect(name)
                            .expect("source special execution branch")
                        }
                    };
                crate::transition::finish_stationary_move(&mut state, outcome).expect(name);
                assert_special_receipt(&root, name, &state, &receipt, mismatches)
            });
        }
        assert!(
            mismatches.is_empty(),
            "source purchase/pull/sector differences:\n{}",
            mismatches.join("\n")
        );
    }

    fn load_special_receipt(
        root: &std::path::Path,
        name: &str,
    ) -> Result<(GameState, Square, MoveTarget, Value)> {
        let receipt: Value = serde_json::from_slice(
            &std::fs::read(root.join(format!("{name}.json"))).expect("frozen source receipt"),
        )
        .expect("source JSON");
        assert_eq!(
            receipt["sourceSha256"],
            "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
        );
        assert_eq!(
            receipt["rulesVersion"], RULES_VERSION_V7,
            "{name} source rules version"
        );
        let state =
            source_callback_state(&source_envelope(&receipt["before"], &receipt["rngBefore"])?)?;
        let from = serde_json::from_value(receipt["from"].clone()).expect("source origin");
        let target = serde_json::from_value(receipt["target"].clone()).expect("source target");
        Ok((state, from, target, receipt))
    }

    fn assert_special_receipt(
        root: &std::path::Path,
        name: &str,
        state: &GameState,
        receipt: &Value,
        mismatches: &mut Vec<String>,
    ) -> Result<()> {
        let expected = &receipt["after"];
        let expected_digest = format!(
            "{:x}",
            Sha256::digest(
                serde_jcs::to_vec(expected)
                    .map_err(|error| EngineError::Serialization(error.to_string()))?
            )
        );
        compare_value(
            &receipt["afterJcsSha256"],
            &json!(expected_digest),
            &format!("{name}.receipt.afterJcsSha256"),
            mismatches,
        )?;
        let actual = source_state_value(state);
        if serde_jcs::to_vec(expected)
            .map_err(|error| EngineError::Serialization(error.to_string()))?
            != serde_jcs::to_vec(&actual)
                .map_err(|error| EngineError::Serialization(error.to_string()))?
        {
            // 명시적으로 공급한 외부 reports에만 native 전체 상태를 남긴다.
            std::fs::write(
                root.join(format!("{name}.native-after.json")),
                serde_json::to_vec_pretty(state).expect("native after JSON"),
            )
            .expect("write native mismatch receipt");
        }
        // state/RNG/history뿐 아니라 전체 envelope와 그 PositionID도 검사한다.
        // 1과 1.0은 JCS 의미로 비교하며 실제 Number/Boolean 차이는 유지한다.
        compare_callback_envelope(
            state,
            &source_envelope(expected, &receipt["rngAfter"])?,
            name,
            mismatches,
        )
    }

    #[test]
    #[ignore = "requires ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS with pinned source receipts"]
    fn frozen_football_when_source_receipts_supplied() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS")
                .expect("source receipt directory"),
        );
        let mut mismatches = Vec::new();
        for name in [
            "football-move",
            "football-capture",
            "football-shield",
            "football-protected",
            "football-health",
            "football-health-terminal",
            "football-royal",
            "football-explosive",
            "football-target-hidden",
            "football-fog",
            "football-en-passant",
            "football-crown-ground",
            "football-reserved",
            "football-black-hole",
            "football-own-color",
        ] {
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                let (mut state, from, target, receipt) = load_special_receipt(&root, name)?;
                crate::v7_card_context::begin_board_action(&mut state);
                let outcome = execute_football_move(&mut state, from, &target, false)
                    .expect(name)
                    .expect("Football branch");
                crate::transition::finish_stationary_move(&mut state, outcome).expect(name);
                assert_special_receipt(&root, name, &state, &receipt, mismatches)
            });
        }
        assert!(
            mismatches.is_empty(),
            "source Football differences:\n{}",
            mismatches.join("\n")
        );
    }

    #[test]
    #[ignore = "requires ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS with pinned source receipts"]
    fn frozen_large_and_castle_when_source_receipts_supplied() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS")
                .expect("source receipt directory"),
        );
        let mut mismatches = Vec::new();
        for name in [
            "colossus-move",
            "colossus-move-capture",
            "colossus-move-royal",
            "colossus-move-quantum",
            "colossus-move-reserved",
            "colossus-move-manner",
            "colossus-move-saturation",
            "colossus-move-health",
            "colossus-body",
            "big-rook-move",
            "big-rook-move-friendly-capture",
            "big-rook-move-capture-two",
            "big-rook-move-capture-limit",
            "big-rook-move-quantum",
            "castle",
            "castle-hidden",
            "castle-twin-ultimatum",
            "castle-roller",
            "castle-big-rook",
            "castle-big-rook-crush",
        ] {
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                let (mut state, from, target, receipt) = load_special_receipt(&root, name)?;
                state.active_v7_move_context =
                    Some(capture_move_execution_context(&state, from, &target).expect(name));
                crate::v7_card_context::begin_board_action(&mut state);
                let outcome =
                    match prepare_v7_move_selection(&mut state, from, &target).expect(name) {
                        V7MoveSelection::Return(outcome) => outcome,
                        V7MoveSelection::Ready(prelude) => {
                            let mut moving = prelude.moving;
                            moving.extra.shift_remove("thiefSecondMove");
                            moving
                                .source_order
                                .retain(|field| field != "thiefSecondMove");
                            crate::v7_board_hazards::replace_object_aliases(&mut state, &moving);
                            if target.flag("castle") {
                                execute_castle(&mut state, from, &target, &prelude.privacy, false)
                            } else {
                                execute_large_piece_move(
                                    &mut state,
                                    from,
                                    &target,
                                    &prelude.privacy,
                                    false,
                                )
                            }
                            .expect(name)
                            .expect("source castle/large branch")
                        }
                    };
                crate::transition::finish_stationary_move(&mut state, outcome).expect(name);
                let mut context = state
                    .active_v7_move_context
                    .take()
                    .expect("source move context");
                sync_move_execution_context(&mut state, &context).expect(name);
                finish_roller_context(&mut state, &mut context).expect(name);
                assert_special_receipt(&root, name, &state, &receipt, mismatches)
            });
        }
        assert!(
            mismatches.is_empty(),
            "source large/castle differences:\n{}",
            mismatches.join("\n")
        );
    }

    #[test]
    #[ignore = "requires ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS with pinned source receipts"]
    fn frozen_move_context_callbacks_when_source_receipts_supplied() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS")
                .expect("source receipt directory"),
        );
        let mut mismatches = Vec::new();
        for name in [
            "roller-line",
            "roller-diagonal",
            "roller-teleport",
            "roller-portal",
            "roller-arrival-captured",
            "roller-captured-without-arrival",
            "roller-cancelled",
            "roller-normalize",
            "roller-double-finish",
            "roller-castle-context",
            "metal-before-turn",
            "metal-after-turn",
            "metal-mover-filter",
            "metal-legacy-all",
            "metal-no-origin",
            "medium-original-memory",
            "medium-original-null",
            "mad-knight-push",
            "mad-knight-normalize",
            "mad-knight-free-resolution",
            "mad-knight-nonknight",
            "mad-knight-black",
            "mad-knight-other-campaign",
        ] {
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                let (mut state, from, target, receipt) = load_special_receipt(&root, name)?;
                let mut returns = Vec::new();
                if receipt["operation"] == "mad-knight" {
                    if name == "mad-knight-free-resolution" {
                        state.free_move_resolution = Some(Color::White);
                    }
                    let moving = state.at(from).expect("source snapshot mover").clone();
                    push_mad_knight_undo_before_move(&mut state, &moving).expect(name);
                } else {
                    let mut context =
                        capture_move_execution_context(&state, from, &target).expect(name);
                    for step in receipt["steps"].as_array().expect("source context steps") {
                        let square = |key: &str| {
                            serde_json::from_value::<Square>(step[key].clone())
                                .expect("source context square")
                        };
                        match step["kind"].as_str().expect("source context operation") {
                            "relocate" => {
                                let from = square("from");
                                let to = square("to");
                                let piece = state.board[usize::from(from.row)]
                                    [usize::from(from.col)]
                                .take();
                                state.board[usize::from(to.row)][usize::from(to.col)] = piece;
                            }
                            "arrival" => {
                                note_roller_arrival(&mut context, square("from"), square("to"))
                            }
                            "remove" => {
                                let at = square("at");
                                state.board[usize::from(at.row)][usize::from(at.col)] = None;
                            }
                            "turn" => {
                                state.turn = serde_json::from_value(step["color"].clone())
                                    .expect("source actor")
                            }
                            "sync" => {
                                sync_move_execution_context(&mut state, &context).expect(name)
                            }
                            "finish" => returns
                                .push(finish_roller_context(&mut state, &mut context).expect(name)),
                            "memory" => {
                                state
                                    .extra
                                    .get_mut("parrotMovement")
                                    .expect("source memory map")["white"] = step["value"].clone()
                            }
                            "lastmove" => crate::card_effects::set_last_move_with_medium_memory(
                                &mut state,
                                crate::card_effects::LastMoveContext {
                                    from: square("from"),
                                    to: square("to"),
                                    sound_name: "move",
                                    sound_color: Color::White,
                                    hidden_from: "",
                                    moved_override: None,
                                    original_medium: medium_move_snapshot(&context),
                                },
                            )
                            .expect(name),
                            unknown => panic!("unrecognized source context operation {unknown}"),
                        }
                    }
                }
                compare_value(
                    &receipt["returns"],
                    &json!(returns),
                    &format!("{name}.returns"),
                    mismatches,
                )?;
                assert_special_receipt(&root, name, &state, &receipt, mismatches)
            });
        }
        assert!(
            mismatches.is_empty(),
            "source move context differences:\n{}",
            mismatches.join("\n")
        );
    }

    /// 공통 착지·포획을 다시 구현하지 않고 source movePiece에 대응하는
    /// 내부 wrapper를 검증한다. 공개 후보 입장 검증과는 별도 실행 근거다.
    #[test]
    #[ignore = "requires ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS with pinned source receipts"]
    fn frozen_siege_mistake_and_wrapper_when_source_receipts_supplied() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS")
                .expect("source receipt directory"),
        );
        let mut mismatches = Vec::new();
        for name in [
            "siege-empty",
            "siege-forced-neutral",
            "siege-shield",
            "siege-hp",
            "siege-friendly",
            "siege-explosive",
            "siege-large",
            "siege-royal",
            "siege-saturation",
            "siege-saturation-empty",
            "siege-manner",
            "siege-chameleon",
            "mistake-rule-hit",
            "mistake-rule-miss",
            "mistake-card-hit",
            "mistake-card-boundary",
            "mistake-shielded-mover",
            "mistake-protected-mover",
            "mistake-shielded-counter",
            "mistake-frozen-counter",
            "mistake-royal-mover",
            "mistake-en-passant",
            "mistake-checker-jump",
            "mistake-portal-priority",
            "portal-capture-both",
            "ordinary-roller",
            "ordinary-roller-portal",
            "ordinary-metal",
        ] {
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                let (mut state, from, target, receipt) = load_special_receipt(&root, name)?;
                let action = crate::Action::movement(state.turn, from, target);
                crate::v7_move_transition::execute(&mut state, &action, false).expect(name);
                assert_special_receipt(&root, name, &state, &receipt, mismatches)?;
                assert!(
                    state.active_v7_move_context.is_none(),
                    "{name} wrapper context restored"
                );
                assert!(
                    state.active_v7_saturation_attack.is_none(),
                    "{name} saturation context restored"
                );
                assert!(
                    state.free_move_resolution.is_none(),
                    "{name} nested Mistake context restored"
                );
                Ok(())
            });
        }
        assert!(
            mismatches.is_empty(),
            "source Siege/Mistake/wrapper differences:\n{}",
            mismatches.join("\n")
        );
    }

    fn compare_lifecycle_state(
        root: &std::path::Path,
        label: &str,
        state: &GameState,
        expected: &Value,
        rng: &Value,
        mismatches: &mut Vec<String>,
    ) -> Result<()> {
        let digest = format!(
            "{:x}",
            Sha256::digest(
                serde_jcs::to_vec(expected)
                    .map_err(|error| EngineError::Serialization(error.to_string()))?
            )
        );
        assert_special_receipt(
            root,
            label,
            state,
            &json!({"after":expected,"rngAfter":rng,"afterJcsSha256":digest}),
            mismatches,
        )
    }

    fn compare_lifecycle_digest(
        receipt: &Value,
        digest_field: &str,
        value_field: &str,
        label: &str,
        mismatches: &mut Vec<String>,
    ) -> Result<()> {
        let digest = format!(
            "{:x}",
            Sha256::digest(
                serde_jcs::to_vec(&receipt[value_field])
                    .map_err(|error| EngineError::Serialization(error.to_string()))?
            )
        );
        compare_value(
            &receipt[digest_field],
            &json!(digest),
            &format!("{label}.receipt.{digest_field}"),
            mismatches,
        )
    }

    /// frozen browserShell은 host structuredClone을 VM에 주입한다. 그 결과의
    /// foreign-realm Set/Map은 VM __encode의 instanceof 검사에서 제외되어 {}
    /// 로 직렬화된다. 이 fixture의 교체 상태만 같은 표현으로 가져오며, 실제
    /// GameState::clone이나 trusted Host의 instance/ticket 계약은 바꾸지 않는다.
    fn foreign_realm_snapshot_encoding(value: &Value) -> Value {
        match value {
            Value::Object(fields)
                if matches!(
                    fields.get("__simType").and_then(Value::as_str),
                    Some("Set" | "Map")
                ) =>
            {
                Value::Object(serde_json::Map::new())
            }
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(key, value)| (key.clone(), foreign_realm_snapshot_encoding(value)))
                    .collect(),
            ),
            Value::Array(values) => {
                Value::Array(values.iter().map(foreign_realm_snapshot_encoding).collect())
            }
            value => value.clone(),
        }
    }

    fn source_structured_clone_fixture(state: &GameState) -> Result<GameState> {
        let cloned = foreign_realm_snapshot_encoding(&source_state_value(state));
        let rng = serde_json::to_value(&state.rng).map_err(EngineError::serialization)?;
        source_callback_state(&source_envelope(&cloned, &rng)?)
    }

    /// 실제 원본 timer callback의 scheduled/raw/settled 상태를 구분한다.
    /// 인스턴스의 생존·동일성과 ticket 발급은 Host 검사의 책임이며, 여기서는
    /// 그 판정 뒤 호출하는 kernel 및 historical render의 전체 결과를 비교한다.
    #[test]
    #[ignore = "requires ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS with pinned Colossus lifecycle receipts"]
    fn frozen_colossus_lifecycle_and_history_when_source_receipts_supplied() {
        let root = std::path::PathBuf::from(
            std::env::var_os("ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS")
                .expect("source receipt directory"),
        );
        let mut mismatches = Vec::new();
        for name in [
            "colossus-timer-same-state",
            "colossus-timer-stale-copy",
            "colossus-timer-gone-null",
            "colossus-timer-gone-undefined",
            "colossus-timer-changed-turn",
            "colossus-timer-gameover",
            "colossus-timer-ai-immediate",
            "colossus-timer-free-immediate",
            "colossus-timer-terminal-before-reservation",
            "colossus-timer-repeated-callback",
        ] {
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                let (mut state, from, target, receipt) = load_special_receipt(&root, name)?;
                assert_eq!(
                    receipt["operation"], "colossus-timer",
                    "{name} source operation"
                );
                for (digest, value) in [
                    ("scheduledJcsSha256", "scheduled"),
                    ("afterJcsSha256", "activeAfter"),
                    ("originalAfterJcsSha256", "originalAfter"),
                    ("settledJcsSha256", "settled"),
                ] {
                    compare_lifecycle_digest(&receipt, digest, value, name, mismatches)?;
                }
                let ai = receipt["executionContext"]["ai"] == Value::Bool(true);
                let free = receipt["executionContext"]["free"] == Value::Bool(true);
                state.ai_simulation_depth = u32::from(ai);
                if free {
                    state.free_move_resolution = Some(Color::White);
                }
                let action = crate::Action::movement(state.turn, from, target);
                crate::v7_move_transition::execute(&mut state, &action, false)?;
                state.ai_simulation_depth = 0;
                state.free_move_resolution = None;
                compare_lifecycle_state(
                    &root,
                    &format!("{name}-scheduled"),
                    &state,
                    &receipt["scheduled"],
                    &receipt["rngScheduled"],
                    mismatches,
                )?;
                compare_value(
                    &receipt["timerCount"],
                    &json!(usize::from(state.pending_colossus_actor.is_some())),
                    &format!("{name}.timerCount"),
                    mismatches,
                )?;

                if name == "colossus-timer-changed-turn" {
                    state.turn = Color::Black;
                }
                if name == "colossus-timer-gameover" {
                    state.mode = "gameover".into();
                    state.winner = Some("black".into());
                    state
                        .extra
                        .insert("winReason".into(), json!("external-terminal"));
                }
                let stale = if name == "colossus-timer-stale-copy" {
                    Some(source_structured_clone_fixture(&state)?)
                } else {
                    None
                };
                let active_kind = match name {
                    "colossus-timer-gone-null" => "null",
                    "colossus-timer-gone-undefined" => "undefined",
                    _ => "state",
                };
                let active_value = |original: &GameState| {
                    if active_kind == "state" {
                        source_state_value(stale.as_ref().unwrap_or(original))
                    } else {
                        Value::Null
                    }
                };
                compare_value(
                    &receipt["activeBefore"],
                    &json!({"kind":active_kind,"value":active_value(&state)}),
                    &format!("{name}.activeBefore"),
                    mismatches,
                )?;
                // Root Host의 pointer/liveness 판정과 같은 경우에만 kernel을 호출한다.
                if receipt["timerCount"] == 1 && stale.is_none() && active_kind == "state" {
                    let outcome = complete_colossus_turn(&mut state, Color::White)?;
                    crate::transition::finish_stationary_move(&mut state, outcome)?;
                    assert!(
                        state.pending_colossus_actor.is_none(),
                        "{name} timer capability consumed"
                    );
                    if receipt["repeat"] == Value::Bool(true) {
                        // 원본 2회 호출도 두 번째 endMove를 수행하지 않는다. trusted
                        // kernel은 이미 소비한 callback을 명시적으로 거절하며 상태를 보존한다.
                        let once = state.clone();
                        assert!(matches!(complete_colossus_turn(&mut state, Color::White),
                            Err(EngineError::InvalidState(detail)) if detail.contains("no pending timer")));
                        assert_eq!(
                            state, once,
                            "{name} repeated completion preserves all runtime state"
                        );
                    }
                }
                compare_value(
                    &receipt["activeAfter"],
                    &json!({"kind":active_kind,"value":active_value(&state)}),
                    &format!("{name}.activeAfter"),
                    mismatches,
                )?;
                compare_lifecycle_state(
                    &root,
                    &format!("{name}-original"),
                    &state,
                    &receipt["originalAfter"],
                    &receipt["rngAfter"],
                    mismatches,
                )?;
                let mut settled = stale.unwrap_or_else(|| state.clone());
                if active_kind == "state" {
                    crate::replay::settle(&mut settled)?;
                }
                let settled_value = if active_kind == "state" {
                    source_state_value(&settled)
                } else {
                    Value::Null
                };
                compare_value(
                    &receipt["settled"],
                    &json!({"kind":active_kind,"value":settled_value}),
                    &format!("{name}.settled"),
                    mismatches,
                )?;
                if active_kind == "state" {
                    compare_lifecycle_state(
                        &root,
                        &format!("{name}-settled"),
                        &settled,
                        &receipt["settled"]["value"],
                        &receipt["rngSettled"],
                        mismatches,
                    )?;
                } else {
                    compare_value(
                        &receipt["rngSettled"],
                        &serde_json::to_value(&state.rng).map_err(EngineError::serialization)?,
                        &format!("{name}.rngSettled"),
                        mismatches,
                    )?;
                }
                Ok(())
            });
        }
        for name in [
            "colossus-history-base",
            "colossus-history-forward",
            "colossus-history-cache-tie",
            "colossus-history-reverse-tail",
            "colossus-history-latest",
            "colossus-history-index-outside",
            "colossus-history-live-black-flip",
            "colossus-history-frame-camouflage",
            "colossus-history-frame-pending",
            "colossus-history-frame-regency",
            "colossus-history-frame-fog",
            "colossus-history-quantum-large",
        ] {
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                let (mut state, from, target, receipt) = load_special_receipt(&root, name)?;
                assert_eq!(
                    receipt["operation"], "colossus-history-render",
                    "{name} source operation"
                );
                let mut expected_frame = receipt["selectedFrame"].clone();
                if let Some(frame) = expected_frame.as_object_mut() {
                    // main74086 currentHistoryEntry의 별도 visual 장식만 분리한다.
                    // render 후 전체 상태·PositionID 비교는 어떤 필드도 생략하지 않는다.
                    for field in ["cardAnimation", "historyEffects", "replayVisuals"] {
                        frame.shift_remove(field);
                    }
                }
                compare_value(
                    &expected_frame,
                    &crate::replay::v7_history_frame(&state)?.unwrap_or(Value::Null),
                    &format!("{name}.selectedFrame"),
                    mismatches,
                )?;
                let action = crate::Action::movement(state.turn, from, target);
                crate::v7_move_transition::execute(&mut state, &action, false)?;
                assert_special_receipt(&root, name, &state, &receipt, mismatches)
            });
        }
        assert!(
            mismatches.is_empty(),
            "source Colossus lifecycle/history differences:\n{}",
            mismatches.join("\n")
        );
    }
}
