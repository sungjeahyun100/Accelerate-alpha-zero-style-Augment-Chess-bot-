//! 동결 v7의 `applyCard`와 보드 행동 context. 실행 context는 DTO에 직렬화하지 않는다.

use crate::{Color, EngineError, GameState, Piece, Result, Square};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Restore source simulation scope even when a raw effect returns an error.
/// The caller decides whether its disposable clone's RNG must be carried out.
pub(crate) fn with_ai_simulation<T>(
    state: &mut GameState,
    invoke: impl FnOnce(&mut GameState) -> Result<T>,
) -> Result<T> {
    let previous = state.ai_simulation_depth;
    state.ai_simulation_depth = previous
        .checked_add(1)
        .ok_or_else(|| EngineError::InvalidState("source AI simulation depth overflow".into()))?;
    let result = invoke(state);
    state.ai_simulation_depth = previous;
    result
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BoardActionOrigin {
    pub(crate) id: String,
    pub(crate) cells: Vec<Square>,
}

pub(crate) struct V7CardApplicationContext {
    before_types: BTreeMap<String, String>,
}

/// 가상 상자의 재귀는 호출이 소유하는 context에서 제한하며 전역 상태를 공유하지 않는다.
pub(crate) fn with_virtual_card_limit<T>(
    state: &mut GameState,
    effect: impl FnOnce(&mut GameState) -> Result<T>,
) -> Result<T> {
    const MAX_DEPTH: u16 = 64;
    let previous = state.virtual_card_depth;
    if previous >= MAX_DEPTH {
        return Err(EngineError::InvalidState(format!(
            "v7 virtual card recursion depth {previous} exceeds limit {MAX_DEPTH}"
        )));
    }
    state.virtual_card_depth = previous + 1;
    let result = effect(state);
    state.virtual_card_depth = previous;
    result
}

fn board_entries(state: &GameState) -> Vec<(Piece, Vec<Square>)> {
    let mut indices = BTreeMap::<String, usize>::new();
    let mut entries: Vec<(Piece, Vec<Square>)> = Vec::new();
    for (row, line) in state.board.iter().enumerate() {
        for (col, piece) in line.iter().enumerate() {
            let Some(piece) = piece else { continue };
            let index = *indices.entry(piece.id.clone()).or_insert_with(|| {
                let index = entries.len();
                entries.push((piece.clone(), Vec::new()));
                index
            });
            entries[index].1.push(Square {
                row: row as u8,
                col: col as u8,
            });
        }
    }
    entries
}

/// `septemberBeginBoardAction`의 WeakMap 값을 snapshot이 소유하는 실행 context로 보존한다.
pub(crate) fn begin_board_action(state: &mut GameState) {
    if [Color::White, Color::Black]
        .into_iter()
        .any(|color| state.flag("infiltration", color))
        || state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.flag("outpostProtected"))
    {
        state.board_action_origins = Some(
            board_entries(state)
                .into_iter()
                .map(|(piece, cells)| BoardActionOrigin {
                    id: piece.id,
                    cells,
                })
                .collect(),
        );
    } else {
        state.board_action_origins = None;
    }
}

pub(crate) fn begin_v7_card(state: &mut GameState) -> Result<V7CardApplicationContext> {
    if state.ruleset_id != crate::RULES_VERSION_V7 {
        return Err(EngineError::InvalidState(
            "v7 card context requires v7 rules".into(),
        ));
    }
    begin_board_action(state);
    let before_types = board_entries(state)
        .into_iter()
        .map(|(piece, _)| (piece.id, piece.kind))
        .collect();
    Ok(V7CardApplicationContext { before_types })
}

/// `applyCard`의 성공한 직접 효과 뒤 callback 순서를 공유한다. false/reject 효과에는 호출하지 않는다.
pub(crate) fn finish_v7_card(
    state: &mut GameState,
    context: V7CardApplicationContext,
) -> Result<()> {
    if crate::observation::truth(state.extra.get("crownRule")) {
        crate::v7_board_automata::reconcile_crown_rule(state, true)?;
    }
    for (mut piece, _) in board_entries(state) {
        if context
            .before_types
            .get(&piece.id)
            .is_some_and(|kind| kind != &piece.kind)
            && piece.flag("queensGambitProtection")
        {
            clear_queens_gambit_protection_after_transformation(&mut piece);
            update_piece(state, &piece);
        }
    }
    crate::transition::refresh_submerged(state)?;
    crate::v7_capture_objectives::update_capture_flags(state, state.turn, false)?;
    for color in [Color::White, Color::Black] {
        crate::v7_threat::break_initiative_by_check_v7(state, color)?;
    }
    Ok(())
}

pub(crate) fn clear_queens_gambit_protection_after_transformation(piece: &mut Piece) {
    let previous = piece.extra.get("queensGambitPreviousProtected") == Some(&Value::Bool(true));
    if !previous {
        for field in [
            "lastResistance",
            "sacrificeProtection",
            "coronationProtection",
        ] {
            if let Some(protection) = piece.extra.get_mut(field).and_then(Value::as_object_mut) {
                protection.insert("previousProtected".into(), json!(false));
            }
        }
    }
    piece.extra.shift_remove("queensGambitProtection");
    piece.extra.shift_remove("queensGambitPreviousProtected");
    if !previous
        && ![
            "lastResistance",
            "sacrificeProtection",
            "coronationProtection",
        ]
        .into_iter()
        .any(|field| crate::observation::truth(piece.extra.get(field)))
    {
        piece.extra.shift_remove("protected");
    }
}

fn update_piece(state: &mut GameState, piece: &Piece) {
    for cell in state.board.iter_mut().flatten() {
        if cell
            .as_ref()
            .is_some_and(|candidate| candidate.id == piece.id)
        {
            *cell = Some(piece.clone());
        }
    }
}

/// `septemberResolveBoardInfiltration`: 동일 ID의 위치 변화에만 잠입을 부여한다.
pub(crate) fn resolve_board_infiltration(state: &mut GameState) -> Result<()> {
    let Some(before) = state.board_action_origins.clone() else {
        return Ok(());
    };
    let before: BTreeMap<_, _> = before
        .into_iter()
        .map(|entry| (entry.id, entry.cells))
        .collect();
    let after = board_entries(state);
    let occupied: BTreeSet<_> = after
        .iter()
        .flat_map(|(_, cells)| cells.iter().copied())
        .collect();
    state.board_action_origins = Some(
        after
            .iter()
            .map(|(piece, cells)| BoardActionOrigin {
                id: piece.id.clone(),
                cells: cells.clone(),
            })
            .collect(),
    );
    for (mut piece, cells) in after {
        let Some(origin) = before.get(&piece.id) else {
            continue;
        };
        if origin == &cells {
            continue;
        }
        let mut changed = false;
        if piece.flag("outpostProtected") {
            piece.extra.shift_remove("outpostProtected");
            changed = true;
        }
        if let Some(color) = piece.color.owner() {
            let home = if color == Color::White {
                0
            } else {
                state.board.len() - 1
            };
            let body: BTreeSet<_> = cells.iter().copied().collect();
            let isolated = !occupied
                .iter()
                .filter(|cell| !body.contains(cell))
                .any(|other| {
                    cells.iter().any(|cell| {
                        cell.row
                            .abs_diff(other.row)
                            .max(cell.col.abs_diff(other.col))
                            == 1
                    })
                });
            if state.flag("infiltration", color)
                && cells.iter().any(|cell| usize::from(cell.row) == home)
                && isolated
                && !piece.flag("submerged")
            {
                piece.extra.insert("submerged".into(), json!(true));
                update_piece(state, &piece);
                crate::card_effects::mark_animation(state, &piece)?;
                let origin = cells.first().copied().ok_or_else(|| {
                    EngineError::InvalidState("infiltration piece has no board cell".into())
                })?;
                crate::replay::add_piece_action_log(
                    state,
                    &piece,
                    Some(origin),
                    None,
                    format!(
                        "잠입: {}{}의 기물이 잠복했습니다.",
                        char::from(b'a' + origin.col),
                        state.board.len() - usize::from(origin.row)
                    ),
                )?;
                changed = true;
            }
        }
        if changed {
            update_piece(state, &piece);
        }
    }
    Ok(())
}
