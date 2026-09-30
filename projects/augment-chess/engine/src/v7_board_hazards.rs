//! 고정 v7 클라이언트의 보드 환경 제거와 후속 패배 판정.
//!
//! `main-OahWs0tU.js` SHA-256
//! `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`.
//! `applyBlackHoleDeaths`(102361), `applyPostCardBoardHazards`(102388),
//! `resolveEnvironmentalDefeats`(109395)의 호출 경계를 보존한다. 블랙홀은
//! 포획·불사 예약·경계 보호·예언 취소를 수행하는 일반 포획과 다르다.

use crate::{Color, EngineError, GameState, Piece, RULES_VERSION_V7, Result, Square};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// 제거 직전의 기물과 공식 소스가 선택한 좌표·포획 귀속. 큰 기물은
/// 호출자가 정한 첫 환경 접촉 칸과 anchor가 서로 다를 수 있다.
#[derive(Clone, Debug)]
pub(crate) struct EnvironmentalRemoval {
    pub(crate) piece: Piece,
    pub(crate) square: Square,
    pub(crate) capture_owner: Color,
}

fn require_v7(state: &GameState, callback: &str) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 {callback} on rules version {}",
            state.ruleset_id
        )));
    }
    Ok(())
}

fn invalid(callback: &str, field: &str) -> EngineError {
    EngineError::InvalidState(format!("v7 {callback}: invalid {field}"))
}

pub(crate) fn source_royal_king(state: &GameState, piece: &Piece) -> Result<bool> {
    Ok(
        (piece.kind == "merchant" && crate::v7_queued_effects::uses_september18_balance(state)?)
            || crate::observation::truth(piece.extra.get("crownRoyal"))
            || crate::observation::truth(piece.extra.get("editorRoyal"))
            || matches!(
                piece.kind.as_str(),
                "king" | "royalKnight" | "shotgunKing" | "darkWizard"
            ),
    )
}

pub(crate) fn source_royal_identity(state: &GameState, piece: &Piece) -> Result<bool> {
    Ok(source_royal_king(state, piece)?
        || (crate::observation::truth(piece.extra.get("regencyHeir"))
            && state.flag("regency", piece.color)
            && state.flag("kingDead", piece.color)))
}

fn square_name(state: &GameState, square: Square) -> String {
    format!(
        "{}{}",
        char::from(b'a' + square.col),
        state.board.len().saturating_sub(usize::from(square.row))
    )
}

fn replace_piece(state: &mut GameState, piece: &Piece) {
    for candidate in state.board.iter_mut().flatten().flatten() {
        if candidate.id == piece.id {
            *candidate = piece.clone();
        }
    }
}

fn remove_piece(state: &mut GameState, piece: &Piece, square: Square) {
    if piece.is_large() {
        for cell in state.board.iter_mut().flatten() {
            if cell
                .as_ref()
                .is_some_and(|candidate| candidate.id == piece.id)
            {
                *cell = None;
            }
        }
    } else {
        state.board[usize::from(square.row)][usize::from(square.col)] = None;
    }
}

/// `applyPostCardBoardHazards`의 submerged 갱신과 블랙홀 제거만 수행한다.
/// Palace·Herald·Religious Victory는 각 호출자가 이어서 실행한다.
pub(crate) fn post_card(state: &mut GameState, cause: Color) -> Result<bool> {
    require_v7(state, "applyPostCardBoardHazards")?;
    let mut next = state.clone();
    crate::transition::refresh_submerged(&mut next)?;
    let removed = black_hole_deaths_inner(&mut next, cause)?;
    *state = next;
    Ok(removed)
}

/// 이동 종료 경로 등에서 submerged를 다시 실행하지 않고 블랙홀만
/// 처리할 때 사용한다. 제거 전체와 환경 판정은 하나의 원자적 변경이다.
pub(crate) fn black_hole_deaths(state: &mut GameState, cause: Color) -> Result<bool> {
    require_v7(state, "applyBlackHoleDeaths")?;
    let mut next = state.clone();
    let removed = black_hole_deaths_inner(&mut next, cause)?;
    *state = next;
    Ok(removed)
}

/// Source 327. September 26 profile의 Revolving Door도 원거리 효과의
/// 면역 대상이다. campaign의 기본값은 일반 대국의 기본값과 다르다.
pub(crate) fn indirect_attack_immune(state: &GameState, piece: &Piece) -> bool {
    let source = state
        .extra
        .get("cardState")
        .filter(|value| crate::observation::truth(Some(value)));
    let hash = source
        .and_then(|value| value.get("profile"))
        .or_else(|| {
            if source.is_none() {
                state.extra.get("profile")
            } else {
                None
            }
        })
        .and_then(|profile| profile.get("catalogHash"))
        .and_then(Value::as_str)
        .filter(|hash| !hash.is_empty());
    let door_guard = match hash {
        Some(hash) => matches!(
            hash,
            "yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4"
                | "V4n3CNLCGaC3g3oB8pBsmXSzqHCUP5PTvK6E5X5uReI"
        ),
        None if crate::observation::truth(state.extra.get("campaign")) => {
            state.extra.get("revolvingDoorGuard") == Some(&Value::Bool(true))
        }
        None => state.extra.get("revolvingDoorGuard") != Some(&Value::Bool(false)),
    };
    matches!(piece.ability_kind(), "guard" | "jester")
        || door_guard && piece.ability_kind() == "revolvingDoor"
}

/// Source 109609. 3×3 자폭은 rule-bomb sweep와 면역·포획·콜백 순서가
/// 다르다. 원문은 여기에서 경계 보호·Deathmatch·campaign 진행을 추가하지
/// 않는다. ICBM과 포획 후 explosive 반응도 같은 함수를 사용한다.
pub(crate) fn explode_at(state: &mut GameState, at: Square, cause: &str) -> Result<()> {
    require_v7(state, "explodeAt")?;
    let mut next = state.clone();
    if !cause.is_empty() {
        let threat_probe = next.threat_probe_depth > 0;
        crate::v7_threat::mark_king_threat_effect_cause(&mut next, cause, threat_probe)?;
    }
    let mut seen = BTreeSet::new();
    let mut removed = Vec::new();
    let rows = next.board.len();
    let cols = next.board.iter().map(Vec::len).max().unwrap_or(8);
    for row in i16::from(at.row) - 1..=i16::from(at.row) + 1 {
        for col in i16::from(at.col) - 1..=i16::from(at.col) + 1 {
            if row < 0 || col < 0 || row as usize >= rows || col as usize >= cols {
                continue;
            }
            let square = Square {
                row: row as u8,
                col: col as u8,
            };
            let Some(piece) = next.at(square) else {
                continue;
            };
            let frozen =
                piece.kind != "scarecrow" && crate::observation::truth(piece.extra.get("frozen"));
            if frozen
                || indirect_attack_immune(&next, piece)
                || matches!(piece.kind.as_str(), "football" | "monster")
                || !seen.insert(piece.id.clone())
            {
                continue;
            }
            removed.push(EnvironmentalRemoval {
                piece: piece.clone(),
                square,
                capture_owner: if piece.color == Color::White {
                    Color::Black
                } else {
                    Color::White
                },
            });
        }
    }
    for entry in &removed {
        // Unlike applyBlackHoleDeaths, explodeAt clears all aliases only for
        // a colossus. A big rook/bishop loses the first impacted cell only.
        if entry.piece.kind == "colossus" {
            remove_piece(&mut next, &entry.piece, entry.square);
        } else {
            next.board[usize::from(entry.square.row)][usize::from(entry.square.col)] = None;
        }
        crate::v7_rule_bombs::cancel_prophecies(&mut next)?;
        next.captures
            .get_mut(entry.capture_owner)
            .push(entry.piece.clone());
        crate::v7_piece_lifecycle::schedule_undead_resurrection(
            &mut next,
            &entry.piece,
            entry.capture_owner,
            false,
        )?;
    }
    for entry in &removed {
        resolve_reaper_nearby_deaths(&mut next, std::slice::from_ref(entry))?;
    }
    resolve_environmental_defeats_inner(
        &mut next,
        &removed,
        if cause.is_empty() { "자폭" } else { cause },
        true,
    )?;
    let message = format!("{}에서 자폭이 발생했습니다.", square_name(&next, at));
    crate::replay::add_log(&mut next, message)?;
    *state = next;
    Ok(())
}

fn black_hole_deaths_inner(state: &mut GameState, _cause: Color) -> Result<bool> {
    let cells = black_hole_cells(state)?;
    if cells.is_empty() {
        return Ok(false);
    }
    let mut removed_ids = BTreeSet::new();
    let mut removed = Vec::new();
    let mut removed_any = false;
    for square in cells {
        let Some(piece) = state.at(square).cloned() else {
            continue;
        };
        if !piece.id.is_empty() && !removed_ids.insert(piece.id.clone()) {
            continue;
        }
        let privacy = json!({
            "white":{"originVisible":state.piece_visible(&piece,square,Color::White)},
            "black":{"originVisible":state.piece_visible(&piece,square,Color::Black)}
        });
        remove_piece(state, &piece, square);
        removed_any = true;
        let label = crate::replay::source_piece_label(&piece.kind)
            .filter(|label| !label.is_empty())
            .unwrap_or(if piece.kind.is_empty() {
                "기물"
            } else {
                &piece.kind
            });
        crate::replay::add_piece_action_log(
            state,
            &piece,
            Some(square),
            Some(&privacy),
            format!("블랙홀: {label}이 사라졌습니다."),
        )?;
        if let Some(owner) = piece.color.owner()
            && !try_revive_blood_moon_lord(state, &piece)?
        {
            removed.push(EnvironmentalRemoval {
                piece,
                square,
                capture_owner: owner.opponent(),
            });
        }
    }
    if !removed.is_empty() {
        resolve_environmental_defeats_inner(state, &removed, "블랙홀", false)?;
    }
    if removed_any {
        crate::v7_capture_objectives::check_campaign_objectives(state)?;
    }
    Ok(removed_any)
}

pub(crate) fn black_hole_cells(state: &GameState) -> Result<Vec<Square>> {
    let Some(cells) = state.extra.get("blackHole").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut normalized = Vec::new();
    for cell in cells {
        for field in ["row", "col"] {
            if matches!(cell.get(field), Some(Value::Array(_) | Value::Object(_))) {
                return Err(EngineError::UnsupportedFeature(format!(
                    "v7 normalizeBlackHole: {field} JavaScript object coercion"
                )));
            }
        }
        let (Some(row), Some(col)) = (
            crate::card_effects::js_number(cell.get("row"), 0),
            crate::card_effects::js_number(cell.get("col"), 0),
        ) else {
            continue;
        };
        // Source inBounds accepts fractions, but indexing an ordinary board
        // with a fractional property yields no piece and performs no removal.
        if row.fract() != 0.0
            || col.fract() != 0.0
            || row < 0.0
            || col < 0.0
            || row >= state.board.len() as f64
            || col >= state.board.iter().map(Vec::len).max().unwrap_or(8) as f64
        {
            continue;
        }
        if row > f64::from(u8::MAX) || col > f64::from(u8::MAX) {
            return Err(invalid("normalizeBlackHole", "coordinate range"));
        }
        normalized.push(Square {
            row: row as u8,
            col: col as u8,
        });
    }
    Ok(normalized)
}

/// Source 102692. `bloodMoonState`를 먼저 호출하므로 일반 색 기물의
/// 블랙홀 제거도 BloodMoon 내부 기본값을 정규화할 수 있다.
pub(crate) fn try_revive_blood_moon_lord(state: &mut GameState, captured: &Piece) -> Result<bool> {
    require_v7(state, "tryReviveBloodMoonLord")?;
    if !crate::v7_campaign::normalize_blood_moon(state)? {
        return Ok(false);
    }
    if captured.kind != "vampireLord" || crate::v7_campaign::is_blood_moon_night(state) {
        return Ok(false);
    }
    let data = state.extra["campaign"]["bloodMoon"]
        .as_object()
        .ok_or_else(|| invalid("tryReviveBloodMoonLord", "campaign.bloodMoon"))?;
    let ids = data["coffinIds"]
        .as_array()
        .expect("coffinIds normalized")
        .iter()
        .cloned()
        .chain(data.get("coffinId").cloned())
        .filter(|id| crate::observation::truth(Some(id)))
        .collect::<Vec<_>>();
    let mut coffin = None;
    for id in ids.iter().rev() {
        let found = state.board.iter().enumerate().find_map(|(row, line)| {
            line.iter().enumerate().find_map(|(col, cell)| {
                cell.as_ref()
                    .filter(|piece| *id == json!(piece.id))
                    .map(|piece| {
                        (
                            Square {
                                row: row as u8,
                                col: col as u8,
                            },
                            piece.clone(),
                        )
                    })
            })
        });
        if let Some((square, piece)) = found
            && piece.kind == "coffin"
            && piece.color == captured.color
        {
            coffin = Some((square, piece));
            break;
        }
    }
    let Some((square, coffin_piece)) = coffin else {
        return Ok(false);
    };
    let mut revived = captured.clone();
    revived.moved = true;
    if !revived.source_order.iter().any(|field| field == "moved") {
        revived.source_order.push("moved".into());
    }
    state.board[usize::from(square.row)][usize::from(square.col)] = Some(revived.clone());
    crate::card_effects::mark_animation(state, &revived)?;
    let data = state.extra["campaign"]["bloodMoon"]
        .as_object_mut()
        .ok_or_else(|| invalid("tryReviveBloodMoonLord", "campaign.bloodMoon"))?;
    data.insert("coffinId".into(), Value::Null);
    data.get_mut("coffinIds")
        .and_then(Value::as_array_mut)
        .expect("coffinIds normalized")
        .retain(|id| *id != json!(coffin_piece.id));
    let message = format!(
        "오래된 관: 뱀파이어 군주가 {}에서 부활했습니다.",
        square_name(state, square)
    );
    crate::replay::add_piece_action_log(state, &revived, Some(square), None, message)?;
    Ok(true)
}

/// Source 109223. 기존 표식이 있는 기물을 첫 퀸보다 우선하며, 다른
/// 모든 아군의 표식을 지운 다음 새 계승자의 animation/log를 기록한다.
pub(crate) fn ensure_regency_heir(state: &mut GameState, color: Color) -> Result<Option<Square>> {
    require_v7(state, "ensureRegencyHeir")?;
    if !state.flag("regency", color) || !state.flag("kingDead", color) {
        return Ok(None);
    }
    let mut marked = None;
    let mut first_queen = None;
    for (row, line) in state.board.iter().enumerate() {
        for (col, cell) in line.iter().enumerate() {
            let Some(piece) = cell.as_ref().filter(|piece| piece.color == color) else {
                continue;
            };
            let at = Square {
                row: row as u8,
                col: col as u8,
            };
            if marked.is_none() && crate::observation::truth(piece.extra.get("regencyHeir")) {
                marked = Some((at, piece.clone()));
            }
            if first_queen.is_none() && piece.kind == "queen" {
                first_queen = Some((at, piece.clone()));
            }
        }
    }
    let heir = marked.or(first_queen);
    for piece in state.board.iter_mut().flatten().flatten() {
        if piece.color == color && heir.as_ref().is_none_or(|(_, heir)| heir.id != piece.id) {
            piece.extra.shift_remove("regencyHeir");
            piece.source_order.retain(|field| field != "regencyHeir");
        }
    }
    let Some((square, mut piece)) = heir else {
        return Ok(None);
    };
    let was_marked = crate::observation::truth(piece.extra.get("regencyHeir"));
    piece.extra.insert("regencyHeir".into(), json!(true));
    replace_piece(state, &piece);
    if !was_marked {
        crate::card_effects::mark_animation(state, &piece)?;
        let message = format!("{}의 퀸이 왕위를 계승했습니다.", square_name(state, square));
        crate::replay::add_piece_action_log(state, &piece, Some(square), None, message)?;
    }
    Ok(Some(square))
}

fn has_democracy_pawn(state: &GameState, color: Color) -> bool {
    state
        .extra
        .get("pendingRecurrences")
        .and_then(Value::as_array)
        .is_some_and(|queue| {
            queue.iter().any(|entry| {
                entry["piece"]["color"] == json!(color) && entry["piece"]["type"] == json!("pawn")
            })
        })
        || state
            .board
            .iter()
            .flatten()
            .flatten()
            .any(|piece| piece.color == color && piece.kind == "pawn")
}

/// 회귀·왕관·왕위 계승·민주주의·사신의 source 순서를 공유한다. 한
/// 후속 반응이 실패하면 호출 직전 상태 전체를 보존한다.
pub(crate) fn resolve_environmental_defeats(
    state: &mut GameState,
    removed: &[EnvironmentalRemoval],
    cause: &str,
    souls_already_counted: bool,
) -> Result<bool> {
    require_v7(state, "resolveEnvironmentalDefeats")?;
    let mut next = state.clone();
    let ended =
        resolve_environmental_defeats_inner(&mut next, removed, cause, souls_already_counted)?;
    *state = next;
    Ok(ended)
}

fn resolve_environmental_defeats_inner(
    state: &mut GameState,
    removed: &[EnvironmentalRemoval],
    cause: &str,
    souls_already_counted: bool,
) -> Result<bool> {
    for entry in removed {
        crate::v7_queued_effects::note_resolve_pawn_capture(state, &entry.piece)?;
        if crate::observation::truth(entry.piece.extra.get("recurrence")) {
            crate::v7_piece_lifecycle::queue_recurrence(state, &entry.piece, entry.capture_owner)?;
        }
    }
    let revived = if state
        .extra
        .get("pendingRecurrences")
        .and_then(Value::as_array)
        .is_some_and(|queue| !queue.is_empty())
    {
        crate::v7_piece_lifecycle::resolve_recurrences(state)?
    } else {
        Vec::new()
    };
    let revived_ids = revived
        .iter()
        .map(|entry| entry.piece.id.as_str())
        .collect::<BTreeSet<_>>();
    let removed = removed
        .iter()
        .filter(|entry| !revived_ids.contains(entry.piece.id.as_str()))
        .collect::<Vec<_>>();
    let threat_probe = state.threat_probe_depth > 0;
    let removal_source = json!({"label":cause});
    for entry in &removed {
        crate::v7_threat::mark_king_threat_removal_cause(
            state,
            &entry.piece,
            entry.square,
            &removal_source,
            threat_probe,
        )?;
    }
    if crate::observation::truth(state.extra.get("crownRule")) {
        crate::v7_board_automata::reconcile_crown_rule_with_simulation(state, true, threat_probe)?;
    }
    for entry in &removed {
        if source_royal_king(state, &entry.piece)? {
            let flags = state
                .extra
                .get_mut("kingDead")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| invalid("resolveEnvironmentalDefeats", "kingDead"))?;
            flags.insert(entry.piece.color.as_str().into(), json!(true));
        }
    }
    for color in [Color::White, Color::Black] {
        if !state.flag("democracy", color)
            && state.flag("regency", color)
            && state.flag("kingDead", color)
        {
            ensure_regency_heir(state, color)?;
        }
    }
    let mut defeated_colors = BTreeSet::new();
    let mut defeats = Vec::new();
    for entry in &removed {
        let Some(color) = entry.piece.color.owner() else {
            continue;
        };
        let piece = &entry.piece;
        let label = crate::replay::label(color);
        let reason = if piece.kind == "vip" {
            Some(format!("{label} 귀빈이 {cause}에 휘말렸습니다."))
        } else if state.democracy_protects_royal(piece) {
            None
        } else if piece.kind == "merchant" {
            Some(format!("{label} 상인이 {cause}에 휘말렸습니다."))
        } else if crate::observation::truth(piece.extra.get("regencyHeir"))
            && state.flag("kingDead", color)
            && state.flag("regency", color)
        {
            Some(format!("왕위를 찬탈한 기물이 {cause}에 휘말렸습니다."))
        } else if source_royal_king(state, piece)?
            && (!state.flag("regency", color)
                || !state
                    .board
                    .iter()
                    .flatten()
                    .flatten()
                    .any(|piece| piece.color == color && piece.kind == "queen"))
        {
            Some(format!("{label} 킹이 {cause}에 휘말렸습니다."))
        } else {
            None
        };
        if let Some(reason) = reason
            && defeated_colors.insert(color)
        {
            defeats.push((entry.capture_owner, reason));
        }
    }
    for entry in &removed {
        let Some(color) = entry.piece.color.owner() else {
            continue;
        };
        if state.flag("democracy", color) {
            continue;
        }
        if entry.piece.kind == "queen"
            && state.flag("kingDead", color)
            && state.flag("regency", color)
            && (crate::observation::truth(entry.piece.extra.get("regencyHeir"))
                || ensure_regency_heir(state, color)?.is_none())
            && defeated_colors.insert(color)
        {
            defeats.push((
                entry.capture_owner,
                format!("왕위를 계승할 퀸이 {cause}에 휘말렸습니다."),
            ));
        }
    }
    for color in [Color::White, Color::Black] {
        if !state.flag("democracy", color) || has_democracy_pawn(state, color) {
            continue;
        }
        let winner = removed
            .iter()
            .find(|entry| entry.piece.color == color)
            .map_or(color.opponent(), |entry| entry.capture_owner);
        if defeated_colors.insert(color) {
            defeats.push((
                winner,
                format!(
                    "{}의 모든 폰이 {cause}으로 사라졌습니다.",
                    crate::replay::label(color)
                ),
            ));
        }
    }
    if defeated_colors.len() == 2 {
        let mut both_kings = true;
        for color in [Color::White, Color::Black] {
            let mut king_fell = false;
            for entry in &removed {
                if entry.piece.color == color && source_royal_king(state, &entry.piece)? {
                    king_fell = true;
                    break;
                }
            }
            both_kings &= king_fell;
        }
        let particle = if cause == "붕괴" {
            "붕괴로".to_owned()
        } else {
            format!("{cause}으로")
        };
        let reason = if both_kings {
            format!("{particle} 양쪽 킹이 함께 쓰러졌습니다.")
        } else {
            format!("{particle} 양쪽 왕권이 함께 무너졌습니다.")
        };
        crate::flow::end_game(state, None, &reason)?;
        return Ok(true);
    }
    if let Some((winner, reason)) = defeats.first() {
        crate::flow::end_game(state, Some(*winner), reason)?;
        return Ok(true);
    }
    if souls_already_counted {
        return Ok(false);
    }
    let removed = removed.into_iter().cloned().collect::<Vec<_>>();
    Ok(resolve_reaper_nearby_deaths(state, &removed)? && state.mode == "gameover")
}

/// Source 109338. Royal identity가 없는 궁성은 로그 없이 사라지고,
/// 존재하는 왕이 궁성에서 나온 경우에만 탈출 로그를 쓴다.
pub(crate) fn update_palaces(state: &mut GameState) -> Result<()> {
    require_v7(state, "updatePalaces")?;
    let palaces = state
        .extra
        .get("palaces")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("updatePalaces", "palaces"))?
        .clone();
    let mut retained = Vec::new();
    for palace in palaces {
        let color = match palace.get("color").and_then(Value::as_str) {
            Some("white") => Color::White,
            Some("black") => Color::Black,
            _ => return Err(invalid("updatePalaces", "palace.color")),
        };
        let mut king_square = None;
        for (row, line) in state.board.iter().enumerate() {
            for (col, cell) in line.iter().enumerate() {
                if let Some(piece) = cell.as_ref()
                    && piece.color == color
                    && source_royal_identity(state, piece)?
                {
                    king_square = Some(Square {
                        row: row as u8,
                        col: col as u8,
                    });
                    break;
                }
            }
            if king_square.is_some() {
                break;
            }
        }
        let Some(king_square) = king_square else {
            continue;
        };
        let cells = palace
            .get("cells")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("updatePalaces", "palace.cells"))?;
        if cells.iter().any(|cell| {
            cell.get("row").and_then(Value::as_f64) == Some(f64::from(king_square.row))
                && cell.get("col").and_then(Value::as_f64) == Some(f64::from(king_square.col))
        }) {
            retained.push(palace);
        } else {
            crate::replay::add_log(
                state,
                format!(
                    "{} 킹이 궁성 밖으로 탈출해 궁성이 무너졌습니다.",
                    crate::replay::label(color)
                ),
            )?;
        }
    }
    state.extra.insert("palaces".into(), json!(retained));
    Ok(())
}

/// Source 107916. 환경 제거에는 activePiece/deferNotation이 없다.
/// 일반 포획에서 이동 중 객체가 있는 경우에는 context 함수를 사용한다.
pub(crate) fn resolve_reaper_nearby_deaths(
    state: &mut GameState,
    removed: &[EnvironmentalRemoval],
) -> Result<bool> {
    resolve_reaper_nearby_deaths_with_context(state, removed, None, None, false)
}

/// 이동 중 객체는 board에서 아직 원래 칸에 있거나 이미 제거됐을 수
/// 있다. 그 객체의 예약 표식을 별도 인자로 전달하고 성공할 때만 caller
/// 객체와 board/capture aliases에 반영하여 JS object identity를 보존한다.
pub(crate) fn resolve_reaper_nearby_deaths_with_context(
    state: &mut GameState,
    removed: &[EnvironmentalRemoval],
    active_piece: Option<&mut Piece>,
    active_piece_landing: Option<Square>,
    defer_notation: bool,
) -> Result<bool> {
    require_v7(state, "resolveReaperNearbyDeaths")?;
    let mut next = state.clone();
    let mut active = active_piece.as_ref().map(|piece| (**piece).clone());
    let executed = resolve_reaper_nearby_deaths_inner(
        &mut next,
        removed,
        active.as_mut(),
        active_piece_landing,
        defer_notation,
    )?;
    if let (Some(target), Some(updated)) = (active_piece, active) {
        *target = updated;
    }
    *state = next;
    Ok(executed)
}

pub(crate) fn replace_object_aliases(state: &mut GameState, piece: &Piece) {
    replace_piece(state, piece);
    for candidate in state
        .captures
        .white
        .iter_mut()
        .chain(state.captures.black.iter_mut())
    {
        if candidate.id == piece.id {
            *candidate = piece.clone();
        }
    }
}

fn synchronize_active_piece(
    state: &mut GameState,
    active_piece: &mut Option<&mut Piece>,
    updated: &Piece,
) {
    replace_object_aliases(state, updated);
    if let Some(active) = active_piece.as_deref_mut()
        && active.id == updated.id
    {
        *active = updated.clone();
    }
}

fn unique_pieces<F>(state: &GameState, mut predicate: F) -> Result<Vec<(Square, Piece)>>
where
    F: FnMut(&Piece, Square) -> Result<bool>,
{
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    for (row, line) in state.board.iter().enumerate() {
        for (col, cell) in line.iter().enumerate() {
            let Some(piece) = cell else { continue };
            let square = Square {
                row: row as u8,
                col: col as u8,
            };
            if !seen.contains(piece.id.as_str()) && predicate(piece, square)? {
                seen.insert(piece.id.as_str());
                found.push((square, piece.clone()));
            }
        }
    }
    Ok(found)
}

/// Source 107999. 성공한 이동만 moved/animation을 갱신한다. 기물이
/// 이미 처형 칸에 있으면 source는 false를 반환하고 객체를 건드리지 않는다.
pub(crate) fn relocate_reaper_to_execution_square(
    state: &mut GameState,
    reaper: &mut Piece,
    target: Square,
) -> Result<bool> {
    require_v7(state, "relocateReaperToExecutionSquare")?;
    if reaper.kind != "reaper"
        || state
            .board
            .get(usize::from(target.row))
            .and_then(|line| line.get(usize::from(target.col)))
            .is_none()
        || state.at(target).is_some()
    {
        return Ok(false);
    }
    if let Some(origin) = state.board.iter().enumerate().find_map(|(row, line)| {
        line.iter().enumerate().find_map(|(col, cell)| {
            cell.as_ref()
                .is_some_and(|piece| piece.id == reaper.id)
                .then_some(Square {
                    row: row as u8,
                    col: col as u8,
                })
        })
    }) {
        remove_piece(state, reaper, origin);
    }
    reaper.moved = true;
    if !reaper.source_order.iter().any(|field| field == "moved") {
        reaper.source_order.push("moved".into());
    }
    state.board[usize::from(target.row)][usize::from(target.col)] = Some(reaper.clone());
    crate::card_effects::mark_animation(state, reaper)?;
    Ok(true)
}

#[derive(Clone, Debug)]
pub(crate) struct ReaperExecution {
    pub(crate) reaper: Piece,
    pub(crate) from: Square,
    pub(crate) to: Square,
}

fn reaper_notation_square(value: &Value, row: &str, col: &str) -> Result<Square> {
    let coordinate = |field| {
        value
            .get(field)
            .and_then(Value::as_f64)
            .filter(|coordinate| coordinate.fract() == 0.0)
            .filter(|coordinate| (0.0..=f64::from(u8::MAX)).contains(coordinate))
            .map(|coordinate| coordinate as u8)
            .ok_or_else(|| invalid("reaper execution notation", field))
    };
    Ok(Square {
        row: coordinate(row)?,
        col: coordinate(col)?,
    })
}

/// Source 108008. Caller가 setLastMove·movement log·기보 flush·terminal
/// frame을 원래 순서로 실행할 수 있도록 실제 처형 이동만 반환한다.
pub(crate) fn consume_active_royal_reaper_execution(
    state: &mut GameState,
    active_piece: &mut Piece,
) -> Result<Option<ReaperExecution>> {
    require_v7(state, "consumeActiveRoyalReaperExecution")?;
    let mut next = state.clone();
    let mut active = active_piece.clone();
    let pending = active.extra.shift_remove("pendingReaperDefeat");
    active
        .source_order
        .retain(|field| field != "pendingReaperDefeat");
    replace_object_aliases(&mut next, &active);
    let execution =
        if let Some(pending) = pending.filter(|pending| crate::observation::truth(Some(pending))) {
            let target = reaper_notation_square(&pending, "toRow", "toCol")?;
            let origin = reaper_notation_square(&pending, "fromRow", "fromCol")?;
            let located = unique_pieces(&next, |piece, _| {
                Ok(pending.get("reaperId") == Some(&json!(piece.id)))
            })?
            .into_iter()
            .next();
            if let Some((_, mut reaper)) = located
                && reaper.kind == "reaper"
                && (relocate_reaper_to_execution_square(&mut next, &mut reaper, target)?
                    || next.at(target).is_some_and(|piece| piece.id == reaper.id))
            {
                Some(ReaperExecution {
                    reaper,
                    from: origin,
                    to: target,
                })
            } else {
                None
            }
        } else {
            None
        };
    *active_piece = active;
    *state = next;
    Ok(execution)
}

/// Source 108040. target를 먼저 조회하고, 도달 여부와 무관하게 표식을
/// 삭제한다. Caller는 반환된 target이 있을 때만 이동 로그를 작성한다.
pub(crate) fn consume_immediate_reaper_target(
    state: &mut GameState,
    active_piece: &mut Piece,
) -> Result<Option<Square>> {
    require_v7(state, "finalizeImmediateReaperExecution")?;
    let target = if active_piece.kind == "reaper" {
        active_piece.extra.get("reaperExecutionTarget").cloned()
    } else {
        None
    };
    let reached = match target {
        Some(target) if crate::observation::truth(Some(&target)) => {
            let square: Square = serde_json::from_value(target).map_err(|_| {
                invalid("finalizeImmediateReaperExecution", "reaperExecutionTarget")
            })?;
            state
                .at(square)
                .is_some_and(|piece| piece.id == active_piece.id)
                .then_some(square)
        }
        _ => None,
    };
    active_piece.extra.shift_remove("reaperExecutionTarget");
    active_piece
        .source_order
        .retain(|field| field != "reaperExecutionTarget");
    replace_object_aliases(state, active_piece);
    Ok(reached)
}

/// Source 107908. 예약 배열을 비운 뒤 소유 기물에서 예약 필드 자체를
/// 삭제한다. 반환 개수는 source처럼 유효한 기보 개수와 무관한 배열 길이다.
pub(crate) fn flush_reaper_execution_notations(
    state: &mut GameState,
    active_piece: &mut Piece,
) -> Result<usize> {
    require_v7(state, "flushReaperExecutionNotations")?;
    let mut next = state.clone();
    let mut active = active_piece.clone();
    let pending = active
        .extra
        .shift_remove("pendingReaperExecutionNotations")
        .and_then(|pending| pending.as_array().cloned())
        .unwrap_or_default();
    active
        .source_order
        .retain(|field| field != "pendingReaperExecutionNotations");
    replace_object_aliases(&mut next, &active);
    let queue_notations = next.threat_probe_depth == 0;
    for entry in pending.iter().filter(|_| queue_notations) {
        let color = match entry.get("color").and_then(Value::as_str) {
            Some("white") => Color::White,
            Some("black") => Color::Black,
            _ => continue,
        };
        let id = entry
            .get("reaperId")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("flushReaperExecutionNotations", "reaperId"))?;
        let reaper = unique_pieces(&next, |piece, _| Ok(piece.id == id))?
            .into_iter()
            .next()
            .map(|(_, piece)| piece)
            .unwrap_or_else(|| Piece::new("reaper", color, id));
        let from = reaper_notation_square(entry, "fromRow", "fromCol")?;
        let to = reaper_notation_square(entry, "toRow", "toCol")?;
        crate::replay::queue_reaper_execution_notation(&mut next, &reaper, from, to)?;
    }
    *active_piece = active;
    *state = next;
    Ok(pending.len())
}

// Reaper progress does not use Number.isFinite: explicit Infinity triggers
// execution, while -Infinity becomes one soul. Reuse the common finite
// primitive coercion; valid numeric text that overflows also remains Infinity.
fn reaper_capture_count(value: Option<&Value>) -> Result<Option<f64>> {
    if matches!(value, Some(Value::Array(_) | Value::Object(_))) {
        return Err(EngineError::UnsupportedFeature(
            "v7 resolveReaperNearbyDeaths: reaperCaptures JavaScript object coercion".into(),
        ));
    }
    if let Some(number) = crate::card_effects::js_number(value, 0) {
        return Ok(Some(number));
    }
    if let Some(Value::String(text)) = value {
        let text = text
            .trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}');
        match text {
            "Infinity" | "+Infinity" => return Ok(Some(f64::INFINITY)),
            "-Infinity" => return Ok(Some(f64::NEG_INFINITY)),
            _ => {}
        }
        if text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.' | b'e' | b'E'))
            && let Ok(number) = text.parse::<f64>()
            && number.is_infinite()
        {
            return Ok(Some(number));
        }
        if [
            ("0x", 16),
            ("0X", 16),
            ("0o", 8),
            ("0O", 8),
            ("0b", 2),
            ("0B", 2),
        ]
        .into_iter()
        .any(|(prefix, radix)| {
            text.strip_prefix(prefix).is_some_and(|digits| {
                !digits.is_empty() && digits.chars().all(|digit| digit.to_digit(radix).is_some())
            })
        }) {
            // A valid radix string reached here only when the common finite
            // parser overflowed; JS Number returns positive Infinity.
            return Ok(Some(f64::INFINITY));
        }
    }
    Ok(None)
}

fn resolve_reaper_nearby_deaths_inner(
    state: &mut GameState,
    removed: &[EnvironmentalRemoval],
    mut active_piece: Option<&mut Piece>,
    active_piece_landing: Option<Square>,
    defer_notation: bool,
) -> Result<bool> {
    if state.mode == "gameover" || removed.is_empty() {
        return Ok(false);
    }
    let dead_ids = removed
        .iter()
        .map(|entry| entry.piece.id.as_str())
        .filter(|id| !id.is_empty())
        .collect::<BTreeSet<_>>();
    let mut executed = false;
    for death in removed {
        if state.mode == "gameover" {
            break;
        }
        let witnesses = unique_pieces(state, |piece, square| {
            Ok(piece.kind == "reaper"
                && !dead_ids.contains(piece.id.as_str())
                && piece.color.owner().is_some()
                && square
                    .row
                    .abs_diff(death.square.row)
                    .max(square.col.abs_diff(death.square.col))
                    == 1)
        })?;
        for (origin, mut reaper) in witnesses {
            if state.mode == "gameover" {
                break;
            }
            let progress = reaper_capture_count(reaper.extra.get("reaperCaptures"))?
                .unwrap_or(0.0)
                .floor()
                .max(0.0)
                + 1.0;
            let count = progress.min(4.0);
            reaper.extra.insert("reaperCaptures".into(), json!(count));
            synchronize_active_piece(state, &mut active_piece, &reaper);
            crate::replay::add_piece_action_log(
                state,
                &reaper,
                Some(origin),
                None,
                format!("사신: {count}/4 영혼"),
            )?;
            if progress < 4.0 {
                continue;
            }
            let color = reaper.color.owner().ok_or(EngineError::WrongActor)?;
            let royal = unique_pieces(state, |piece, _| {
                Ok(piece.color == color.opponent() && source_royal_identity(state, piece)?)
            })?
            .into_iter()
            .next();
            let royal = match royal {
                Some(royal) => Some(royal),
                None => unique_pieces(state, |piece, _| {
                    Ok(piece.color == color.opponent()
                        && matches!(
                            piece.kind.as_str(),
                            "merchant" | "vip" | "timeTraveler" | "vampireLord"
                        ))
                })?
                .into_iter()
                .next(),
            };
            let Some((royal_square, royal_piece)) = royal else {
                crate::flow::end_game(state, Some(color), "사신이 영혼 4개를 모았습니다.")?;
                executed = true;
                continue;
            };
            let active_reaper = active_piece
                .as_ref()
                .is_some_and(|active| active.id == reaper.id);
            let active_royal = active_piece
                .as_ref()
                .is_some_and(|active| active.id == royal_piece.id);
            let execution_square = active_piece_landing
                .filter(|target| {
                    active_royal
                        && state
                            .board
                            .get(usize::from(target.row))
                            .and_then(|line| line.get(usize::from(target.col)))
                            .is_some()
                })
                .unwrap_or(royal_square);
            if active_reaper {
                reaper
                    .extra
                    .insert("reaperExecutionTarget".into(), json!(royal_square));
            } else {
                reaper.extra.shift_remove("reaperExecutionTarget");
                reaper
                    .source_order
                    .retain(|field| field != "reaperExecutionTarget");
            }
            synchronize_active_piece(state, &mut active_piece, &reaper);
            if state.threat_probe_depth == 0 {
                crate::replay::queue_visual(
                    state,
                    json!({"type":"reaper-execution","color":color,
                        "targetColor":royal_piece.color,"targetType":royal_piece.kind,
                        "cells":[origin,execution_square],"from":origin,"to":execution_square}),
                )?;
            }
            let notation_origin = if active_reaper { death.square } else { origin };
            let notation = json!({"reaperId":reaper.id,"color":color,
                "fromRow":notation_origin.row,"fromCol":notation_origin.col,
                "toRow":execution_square.row,"toCol":execution_square.col});
            if defer_notation && active_piece.is_some() {
                let active = active_piece.as_deref_mut().expect("active checked");
                let notations = active
                    .extra
                    .entry("pendingReaperExecutionNotations")
                    .or_insert_with(|| json!([]));
                if !notations.is_array() {
                    *notations = json!([]);
                }
                notations
                    .as_array_mut()
                    .expect("notations normalized")
                    .push(notation);
                replace_object_aliases(state, active);
            } else if state.threat_probe_depth == 0 {
                crate::replay::queue_reaper_execution_notation(
                    state,
                    &reaper,
                    notation_origin,
                    execution_square,
                )?;
            }
            let threat_source = json!({"attacker":reaper,
                "origin":{"row":origin.row,"col":origin.col,"item":reaper},
                "effect":"reaper-execution"});
            crate::transition::force_remove_piece_at_with_options(
                state,
                royal_square,
                color,
                &crate::transition::ForceRemovalOptions {
                    suppress_reaper_progress: true,
                    attacker: Some(&reaper),
                    threat_source: Some(&threat_source),
                    ..Default::default()
                },
            )?;
            if active_royal {
                let active = active_piece.as_deref_mut().expect("active royal checked");
                active.extra.insert(
                    "pendingReaperDefeat".into(),
                    json!({
                        "reaperId":reaper.id,"fromRow":origin.row,"fromCol":origin.col,
                        "toRow":execution_square.row,"toCol":execution_square.col
                    }),
                );
                replace_object_aliases(state, active);
            } else {
                relocate_reaper_to_execution_square(state, &mut reaper, execution_square)?;
                synchronize_active_piece(state, &mut active_piece, &reaper);
            }
            if state.mode != "gameover" {
                crate::flow::end_game(state, Some(color), "사신이 영혼 4개를 모았습니다.")?;
            }
            crate::replay::add_log(
                state,
                format!(
                    "사신이 네 번째 영혼을 거두고 {}의 상대 킹 자리로 이동했습니다.",
                    square_name(state, execution_square)
                ),
            )?;
            executed = true;
        }
    }
    Ok(executed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameConfig;
    use sha2::{Digest, Sha256};

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

    fn source_state_digest(state: &GameState) -> String {
        let mut raw = serde_json::to_value(state).unwrap();
        let fields = raw.as_object_mut().unwrap();
        for outer in ["rulesetId", "rng", "history"] {
            fields.remove(outer);
        }
        format!("{:x}", Sha256::digest(serde_jcs::to_vec(&raw).unwrap()))
    }

    // These whole-state witnesses are derived from the pinned source recipe
    // v7-board-hazards/source-probe.cjs, rather than self-derived snapshots.
    #[test]
    fn black_hole_removal_preserves_prophecy_captures_rng_and_history() {
        let mut state = initial();
        state
            .extra
            .insert("blackHole".into(), json!([{"row":6,"col":0}]));
        state.extra["prophecy"]["white"] = json!({"remainingHalfTurns":3});
        assert_eq!(
            source_state_digest(&state),
            "8ad0eaec8d0dc951a2daea31148cea414aa32d52661e9c5516ab932b93cbdf07"
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert!(black_hole_deaths(&mut state, Color::White).unwrap());
        assert_eq!(
            source_state_digest(&state),
            "a2c7b59b64d251226951006f27776c3a3767101152c6538202eb365d1389328c"
        );
        assert!(state.board[6][0].is_none());
        assert!(state.captures.white.is_empty() && state.captures.black.is_empty());
        assert_eq!(state.extra["prophecy"]["white"]["remainingHalfTurns"], 3);
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn black_hole_last_democracy_pawn_has_frozen_terminal_reason() {
        let mut state = initial();
        state.extra["democracy"]["white"] = json!(true);
        for col in 1..8 {
            state.board[6][col] = None;
        }
        state
            .extra
            .insert("blackHole".into(), json!([{"row":6,"col":0}]));
        assert_eq!(
            source_state_digest(&state),
            "3e9d6873b4360c334d5e2e14e1f6c4247976528ed81b9b18961332bea3f585c5"
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert!(black_hole_deaths(&mut state, Color::White).unwrap());
        assert_eq!(
            source_state_digest(&state),
            "7a92d04fa234b92757fc81cdafe1f0482e6a181882d8878d5990270ef5fc6846"
        );
        assert_eq!(state.mode, "gameover");
        assert_eq!(state.winner.as_deref(), Some("black"));
        assert_eq!(
            state.extra["replayEndReason"],
            "백의 모든 폰이 블랙홀으로 사라졌습니다."
        );
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn black_hole_royal_removal_marks_regency_queen_before_defeat_check() {
        let mut state = initial();
        state.extra["regency"]["white"] = json!(true);
        state
            .extra
            .insert("blackHole".into(), json!([{"row":7,"col":4}]));
        assert_eq!(
            source_state_digest(&state),
            "1425c6137065b2745152cb91de337f59af8b0a6b69425f3893884908f2680719"
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert!(black_hole_deaths(&mut state, Color::White).unwrap());
        assert_eq!(
            source_state_digest(&state),
            "45a673fb0671e820da1f4df53c5457e3768891714ba08673896fa97271b3845b"
        );
        assert_eq!(state.extra["kingDead"]["white"], true);
        assert_eq!(
            state.board[7][3].as_ref().unwrap().extra["regencyHeir"],
            true
        );
        assert_eq!(state.mode, "play");
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn blood_moon_coffin_replaces_itself_with_the_removed_lord_in_daylight() {
        let mut state = initial();
        state.board[7][4].as_mut().unwrap().kind = "vampireLord".into();
        state.board[7][0].as_mut().unwrap().kind = "coffin".into();
        let coffin_id = state.board[7][0].as_ref().unwrap().id.clone();
        let lord_id = state.board[7][4].as_ref().unwrap().id.clone();
        state.extra.insert(
            "campaign".into(),
            json!({"setup":"bloodMoon","bloodMoon":{
                "nightBloodPeriods":{"white":null,"black":null},"veilUntil":null,
                "sunlightOverride":null,"coffinId":null,"coffinIds":[coffin_id],"lastPeriod":0
            }}),
        );
        state
            .extra
            .insert("blackHole".into(), json!([{"row":7,"col":4}]));
        assert_eq!(
            source_state_digest(&state),
            "d1366ccb4aefdfc57930a3c4aeb611a378e9e27079b8bd608fea7958ebc6b967"
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        assert!(black_hole_deaths(&mut state, Color::White).unwrap());
        // e5ed84fc의 faithful-init-v1 원문 main102378 TYPE_LABELS와 동일하다.
        // 외부 bloodmoon-day-coffin-faithful.json의 전체 state 재검증에서
        // 로그 하나만 legacy "vampireLord"로 바꾸면 이전 c0d18111…이 재현된다.
        assert_eq!(
            source_state_digest(&state),
            "3ed3e59d399fd856b556158bccb8c1cec9c6585e1aa205b3b8349ee2c6a4e162"
        );
        assert_eq!(state.board[7][0].as_ref().unwrap().id, lord_id);
        assert!(state.board[7][0].as_ref().unwrap().moved);
        assert!(state.board[7][4].is_none());
        assert!(state.captures.white.is_empty() && state.captures.black.is_empty());
        assert_eq!(state.extra["campaign"]["bloodMoon"]["coffinIds"], json!([]));
        assert_eq!(state.mode, "play");
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn ordinary_explosion_uses_frozen_capture_order_without_bomb_replay_rng() {
        let mut state = initial();
        assert_eq!(
            source_state_digest(&state),
            "56de60cd87f53f39d1baaaba4261e7af96b402786e9c1f34fe52011642edc2e5"
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        explode_at(&mut state, Square { row: 6, col: 0 }, "").unwrap();
        assert_eq!(
            source_state_digest(&state),
            "945e61256805427ac01395175a86eac805b1f1d0584ab7304732598d01acada2"
        );
        assert_eq!(
            state
                .captures
                .black
                .iter()
                .map(|piece| piece.kind.as_str())
                .collect::<Vec<_>>(),
            ["pawn", "pawn", "rook", "knight"]
        );
        assert!(state.captures.white.is_empty());
        assert_eq!(state.mode, "play");
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    #[test]
    fn icbm_explosion_respects_guard_jester_and_frozen_immunity() {
        let mut state = initial();
        state.board[7][0].as_mut().unwrap().kind = "guard".into();
        state.board[7][1].as_mut().unwrap().kind = "jester".into();
        state.board[6][1]
            .as_mut()
            .unwrap()
            .extra
            .insert("frozen".into(), json!(true));
        assert_eq!(
            source_state_digest(&state),
            "a69d0395cec2c32142f38f3e7f3f02c1b51c6195d190b83fe40fcd118987c70c"
        );
        let rng = state.rng.clone();
        let history = state.history.clone();
        explode_at(&mut state, Square { row: 6, col: 0 }, "ICBM").unwrap();
        assert_eq!(
            source_state_digest(&state),
            "32386ea126919ab0068c41f477bcda9a0104fa3a32e0fdd33c659465b2c45c5d"
        );
        assert!(
            state.board[7][0].is_some()
                && state.board[7][1].is_some()
                && state.board[6][1].is_some()
        );
        assert_eq!(state.captures.black.len(), 1);
        assert_eq!(state.rng, rng);
        assert_eq!(state.history, history);
    }

    /// 전체 endMove·queued replay 정산을 검사한다. 개별 Othello/Gale callback의
    /// 일치만으로 호출 순서나 host commit 완료를 대신하지 않는다.
    #[test]
    #[ignore = "메인이 생성한 ACCELERATE_V7_OTHELLO_TURN_CASES 전체 source 영수증 필요"]
    fn external_othello_turn_boundary_matches_whole_position() {
        use crate::tests::source_callback_fixture::{collect_case_diagnostics, compare_value};
        use crate::v7_host::V7HostPosition;
        use std::path::PathBuf;

        const EXPECTED: [(&str, &str, &[&str]); 12] = [
            ("conveyor-othello", "black", &["conveyor"]),
            ("lunchbox-pending-othello", "white", &["emptyLunchbox"]),
            ("platform-spawn-othello", "black", &["platform"]),
            ("crown-ground-othello", "white", &["crown"]),
            ("gale-othello", "white", &["gale"]),
            ("democracy-last-pawn-othello", "white", &["democracy"]),
            ("royal-regency-heir", "white", &["regency", "royal"]),
            ("royal-terminal-before-gale", "white", &["royal", "gale"]),
            (
                "conveyor-large-alias-othello",
                "black",
                &["conveyor", "largeBoardAlias"],
            ),
            (
                "lunchbox-breaks-royal-bracket",
                "white",
                &["emptyLunchbox", "royal"],
            ),
            (
                "crown-large-alias-gale",
                "white",
                &["crown", "largeBoardAlias", "gale"],
            ),
            (
                "democracy-royal-conversion",
                "white",
                &["democracy", "royal"],
            ),
        ];
        fn jcs_digest(value: &Value) -> Result<String> {
            let bytes = serde_jcs::to_vec(value).map_err(EngineError::serialization)?;
            Ok(format!("{:x}", Sha256::digest(bytes)))
        }
        fn protocol_result(state: &GameState) -> Value {
            let terminal = state.mode == "gameover";
            let winner = if terminal {
                match state.result() {
                    Some(crate::GameResult::White) => json!("white"),
                    Some(crate::GameResult::Black) => json!("black"),
                    _ => Value::Null,
                }
            } else {
                Value::Null
            };
            let outcome = if terminal {
                json!(
                    state
                        .winner
                        .as_deref()
                        .filter(|winner| !winner.is_empty())
                        .unwrap_or("draw")
                )
            } else {
                Value::Null
            };
            let reason = if terminal {
                state
                    .extra
                    .get("replayEndReason")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            } else {
                ""
            };
            json!({"protocolVersion":"accelerate-result-v1","status":if terminal {"terminal"} else {"ongoing"},
                "winner":winner,"outcome":outcome,"reason":reason})
        }

        let path = PathBuf::from(
            std::env::var_os("ACCELERATE_V7_OTHELLO_TURN_CASES")
                .expect("main agent must provide the fresh faithful whole-turn JSONL receipt"),
        );
        assert!(
            path.is_absolute(),
            "Othello whole-turn receipt must be an absolute external path"
        );
        assert!(
            std::fs::metadata(&path).unwrap().len() <= 8 * 1024 * 1024,
            "Othello whole-turn source receipt exceeds its 8 MiB budget"
        );
        let text =
            std::fs::read_to_string(path).expect("whole-turn source receipt must be readable");
        let rows: Vec<Value> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("whole-turn receipt must be JSONL"))
            .collect();
        assert_eq!(
            rows.len(),
            EXPECTED.len(),
            "whole-turn witness requires all 12 exact cases"
        );
        let profile: Value = serde_json::from_str(include_str!(
            "../../contracts/catalog/execution-profile-20260928.json"
        ))
        .unwrap();
        let catalog_version = crate::v7_execution_profile::catalog_version().unwrap();
        let profile_digest = jcs_digest(&profile).unwrap();
        let mut mismatches = Vec::new();
        let mut names = std::collections::BTreeSet::new();
        for (receipt, (name, actor_text, conditions)) in rows.iter().zip(EXPECTED) {
            assert_eq!(receipt["schemaVersion"], 1, "{name}: receipt schema");
            assert_eq!(receipt["name"], name, "source receipt case order changed");
            assert!(names.insert(name), "duplicate source witness case {name}");
            assert_eq!(receipt["actor"], actor_text, "{name}: actor");
            assert_eq!(
                receipt["conditions"],
                json!(conditions),
                "{name}: witness conditions"
            );
            assert_eq!(
                receipt["callback"], "endMove",
                "{name}: must execute whole endMove"
            );
            assert_eq!(receipt["countMove"], true, "{name}: source move count");
            assert_eq!(
                receipt["historyReason"], "move",
                "{name}: source history reason"
            );
            assert_eq!(
                receipt["fixtureKind"],
                "synthetic-board-fresh-faithful-whole-endMove"
            );
            assert_eq!(
                receipt["checkpointKind"],
                "source-immediate-unsettled-not-host-commit"
            );
            assert_eq!(receipt["sourceSha256"], profile["sourceMainSha256"]);
            assert_eq!(receipt["parserSha256"], profile["parserSha256"]);
            assert_eq!(
                receipt["executionProfile"],
                "accelerate-headless-semantic-v7-faithful-init-v1"
            );
            assert_eq!(receipt["executionProfile"], profile["profileVersion"]);
            assert_eq!(receipt["executionProfileSha256"], profile_digest);
            assert_eq!(
                receipt["sourcePublicCatalogHash"],
                profile["sourcePublicCatalogHash"]
            );
            assert_eq!(receipt["rulesVersion"], RULES_VERSION_V7);
            assert_eq!(receipt["catalogVersion"], catalog_version);
            assert_eq!(receipt["config"], json!({"draftDelete":true}));
            assert_eq!(receipt["seed"], 19);
            assert_eq!(receipt["returned"], json!({"sourceUndefined":true}));
            collect_case_diagnostics(name, &mut mismatches, |mismatches| {
                for (stage, digest_field) in [
                    ("before", "beforeJcsSha256"),
                    ("checkpoint", "checkpointJcsSha256"),
                    ("after", "afterJcsSha256"),
                ] {
                    compare_value(
                        &receipt[digest_field],
                        &json!(jcs_digest(&receipt[stage])?),
                        &format!("{name}.{stage}.receiptDigest"),
                        mismatches,
                    )?;
                    // Source checkpoint는 관측된 중간 상태다. Native의 실행 중
                    // pending flag를 세척하지 않고 이 영수증 자체 identity만 검증한다.
                    let imported = V7HostPosition::from_envelope(receipt[stage].clone())?;
                    compare_value(
                        &receipt[stage],
                        &imported.export_envelope()?,
                        &format!("{name}.{stage}.sourceRoundTrip"),
                        mismatches,
                    )?;
                }
                compare_value(
                    &receipt["rngBefore"],
                    &receipt["before"]["rng"],
                    &format!("{name}.sourceRngBefore"),
                    mismatches,
                )?;
                compare_value(
                    &receipt["rngCheckpoint"],
                    &receipt["checkpoint"]["rng"],
                    &format!("{name}.sourceRngCheckpoint"),
                    mismatches,
                )?;
                compare_value(
                    &receipt["rngAfter"],
                    &receipt["after"]["rng"],
                    &format!("{name}.sourceRngAfter"),
                    mismatches,
                )?;
                compare_value(
                    &receipt["historyBefore"],
                    &receipt["before"]["history"],
                    &format!("{name}.sourceHistoryBefore"),
                    mismatches,
                )?;
                compare_value(
                    &receipt["historyAfter"],
                    &receipt["after"]["history"],
                    &format!("{name}.sourceHistoryAfter"),
                    mismatches,
                )?;
                compare_value(
                    &receipt["before"]["history"],
                    &receipt["after"]["history"],
                    &format!("{name}.internalCallbackPreservesProtocolHistory"),
                    mismatches,
                )?;

                let actor = if actor_text == "black" {
                    Color::Black
                } else {
                    Color::White
                };
                let before = V7HostPosition::from_envelope(receipt["before"].clone())?;
                compare_value(
                    &receipt["resultBefore"],
                    &protocol_result(before.state()),
                    &format!("{name}.resultBefore"),
                    mismatches,
                )?;
                let execution = before.transact(before.position_id(), |state| {
                    crate::transition::end_move_for_decision(state, actor, true, Some("move"))?;
                    crate::replay::settle(state)?;
                    Ok(json!({"sourceUndefined":true}))
                });
                compare_value(
                    &receipt["before"],
                    &before.export_envelope()?,
                    &format!("{name}.originalHostUnchanged"),
                    mismatches,
                )?;
                let (after, returned) = execution?;
                let actual = after.export_envelope()?;
                // state/RNG/history의 구체적인 첫 차이를 derived Position ID보다
                // 먼저 보고한다. 전체 envelope identity 비교도 그대로 유지한다.
                for field in ["state", "rng", "history"] {
                    compare_value(
                        &receipt["after"][field],
                        &actual[field],
                        &format!("{name}.after.{field}"),
                        mismatches,
                    )?;
                }
                compare_value(
                    &receipt["returned"],
                    &returned,
                    &format!("{name}.returned"),
                    mismatches,
                )?;
                compare_value(
                    &receipt["resultAfter"],
                    &protocol_result(after.state()),
                    &format!("{name}.resultAfter"),
                    mismatches,
                )?;
                compare_value(
                    &receipt["after"],
                    &actual,
                    &format!("{name}.wholePosition"),
                    mismatches,
                )?;
                Ok(())
            });
        }
        assert!(
            mismatches.is_empty(),
            "whole Othello turn boundary receipts differ:\n{}",
            mismatches.join("\n")
        );
    }
}
