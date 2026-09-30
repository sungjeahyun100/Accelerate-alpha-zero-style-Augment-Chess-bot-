//! 동결 v7 로컬 캠페인의 보드 초기화·동적 카드·공통 clock/획득 callback.
//!
//! `main-OahWs0tU.js` SHA-256
//! `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`,
//! Blood Moon은 52601, 68968-69000, 87077-87104, 102394-102725,
//! Black Tower는 15576-15594, 67370, 87980-88064, 99607-99632를 기준으로 한다.
//! Knight Journey는 81213-81303/81397-81413, Time Traveler query는 102756-102790이다.
//! 로컬 장기 초기화는 66462-66503/66616-66619/80208-80390을 기준으로 한다.
//! 공개 카드 256개와 aux 카드의 registry/드래프트 가중치에는 포함하지 않는다.
//! 카드 사용 정산과 턴 변경은 호출자, 관을 통한 사망 후 부활은 board hazards가 맡는다.

use crate::{
    Action, CardSlot, Color, EngineError, Fields, GameState, Piece, RULES_VERSION_V7, Result,
    Square,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// 로컬 8×8 캠페인 `createCampaignBoard`의 장기 분기(main66462/66616).
/// 호출자는 결과를 목표 문구용 cold 미리보기로 폐기할 수도 있다. 이 경우에도
/// 원문과 같이 white 16개, black 16개의 ID 난수는 소비한다.
pub(crate) fn campaign_board(
    state: &mut GameState,
    setup: &str,
    _player_color: Color,
) -> Result<Vec<Vec<Option<Piece>>>> {
    if state.ruleset_id != RULES_VERSION_V7 || setup != "janggi" {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 local campaign board setup {setup} on rules version {}",
            state.ruleset_id,
        )));
    }
    let mut board = vec![vec![None; 8]; 8];
    fn put(
        state: &mut GameState,
        board: &mut [Vec<Option<Piece>>],
        color: Color,
        kind: &str,
        row: u8,
        col: u8,
    ) -> Result<()> {
        let mut piece = spawn_piece(state, color, kind)?;
        piece
            .extra
            .insert("origin".into(), json!(square_name(Square { row, col })));
        board[usize::from(row)][usize::from(col)] = Some(piece);
        Ok(())
    }
    for color in [Color::White, Color::Black] {
        let (home, man, cannon, pawn) = if color == Color::White {
            (7, 6, 5, 4)
        } else {
            (0, 1, 2, 3)
        };
        for (kind, cols) in [("rook", [0, 7]), ("knight", [1, 6]), ("camel", [2, 5])] {
            for col in cols {
                put(state, &mut board, color, kind, home, col)?;
            }
        }
        put(state, &mut board, color, "king", home, 4)?;
        put(state, &mut board, color, "man", man, 3)?;
        for col in [1, 6] {
            put(state, &mut board, color, "cannon", cannon, col)?;
        }
        for col in [0, 1, 3, 4, 6, 7] {
            put(state, &mut board, color, "pawn", pawn, col)?;
        }
    }
    Ok(board)
}

/// main66475. 궁성의 배열·칸 순서는 원문 계약이며 man을 왕족으로 바꾸지 않는다.
pub(crate) fn janggi_palaces() -> Value {
    json!([
        {"color":"black","center":{"row":0,"col":4},"cells":[
            {"row":0,"col":3},{"row":0,"col":4},{"row":1,"col":3},{"row":1,"col":4}
        ]},
        {"color":"white","center":{"row":7,"col":4},"cells":[
            {"row":7,"col":3},{"row":7,"col":4},{"row":6,"col":3},{"row":6,"col":4}
        ]}
    ])
}

/// main80208의 local Janggi 시작 중 resetGame(false) 이후 단계.
/// 공통 초기화 owner는 먼저 campaignDisplayGoal의 cold board 32 draws를 소비하고,
/// 동일 RNG를 넘겨 fresh resetGame(false)의 표준 보드 32 draws와 모듈 설정을 구성한다.
/// 이 함수는 마지막 장기 board 32 draws와 source clock/history 시작을 소유한다.
/// 온라인 server-authoritative campaignAuthorityState는 이 로컬 초기화 API의 입력이 아니다.
pub(crate) fn initialize_campaign(
    state: &mut GameState,
    setup: &str,
    player_color: Color,
    local_mode: &str,
) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 || setup != "janggi" {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 local campaign initialization {setup} on rules version {}",
            state.ruleset_id,
        )));
    }
    if !matches!(local_mode, "single" | "offline")
        || state.mode != "idle"
        || state.move_count != 0
        || state.full_move != 1
        || !state.captures.white.is_empty()
        || !state.captures.black.is_empty()
    {
        return Err(EngineError::InvalidState(
            "v7 local Janggi requires a fresh resetGame(false) state and single/offline mode"
                .into(),
        ));
    }
    let mut next = state.clone();
    next.extra.insert(
        "campaign".into(),
        json!({
            "id":"janggi","name":"장기","setup":"janggi","goal":"상대 킹을 잡으세요.",
            "playerColor":player_color,"enemyColor":player_color.opponent(),"reviewEnded":false
        }),
    );
    next.board = campaign_board(&mut next, setup, player_color)?;
    next.extra.insert("localMode".into(), json!(local_mode));
    next.extra
        .insert("aiHumanColor".into(), json!(player_color));
    let mad_ai = next
        .extra
        .get_mut("madAi")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            EngineError::InvalidState("v7 local Janggi reset madAi must be an object".into())
        })?;
    mad_ai.insert("enabled".into(), json!(false));
    mad_ai.insert("openingSequenceDone".into(), json!(false));
    mad_ai.insert("diceRoutineTurn".into(), json!(0));
    mad_ai.insert("randomCardsUsed".into(), json!(false));
    if local_mode == "offline" {
        next.extra.insert("boardFlip".into(), json!(true));
        next.extra.insert("pieceCardFlip".into(), json!(false));
    }
    next.mode = "play".into();
    next.turn = Color::White;
    next.winner = None;
    next.extra.insert("onlineGameStarted".into(), json!(true));
    for (field, value) in [
        ("draftDelete", true),
        ("shotgunDlc", false),
        ("ruleOpeningEnabled", false),
        ("ruleSelectionEnabled", false),
        ("middleDraftDone", true),
        ("endDraftDone", true),
        ("cardArchiveAvailable", false),
    ] {
        next.extra.insert(field.into(), json!(value));
    }
    for field in ["shotgunOpeningColor", "appliedRuleCard", "ruleOpeningEvent"] {
        next.extra.insert(field.into(), Value::Null);
    }
    next.extra.insert("palaces".into(), janggi_palaces());
    next.extra
        .insert("hands".into(), json!({"white":[],"black":[]}));
    next.extra
        .insert("playerCards".into(), json!({"white":null,"black":null}));
    // Use the canonical null-slot decoder; a campaign has three vacant slots
    // for both sides regardless of the previously selected normal/chaos/grand UI.
    next.deck_slots = serde_json::from_value::<GameState>(json!({
        "board":[],"turn":"white","deckSlots":{"white":[null,null,null],"black":[null,null,null]}
    }))
    .map_err(EngineError::serialization)?
    .deck_slots;
    for field in [
        "logs",
        "boardHistory",
        "replayEvents",
        "notationTimeline",
        "pendingNotations",
        "pendingReplayVisuals",
    ] {
        next.extra.insert(field.into(), json!([]));
    }
    for field in ["replayBaseFrame", "replayTailFrame", "historyViewIndex"] {
        next.extra.insert(field.into(), Value::Null);
    }
    next.extra.insert("replayEventNonce".into(), json!(0));
    // Source ensureReplayTimeline (main88873) migrates an empty campaign
    // timeline to these same initial values before recording its first frame.
    next.extra.insert("replayTimelineReady".into(), json!(true));
    next.actions_remaining = 1;
    crate::flow::start_clock_for(&mut next, Color::White)?;
    crate::flow::record_position(&mut next)?;
    // Both decks are empty, so queueInitialDeckGainNotations consumes no draws.
    crate::replay::record(&mut next, "campaign:janggi")?;
    crate::replay::add_log(&mut next, "캠페인 시작: 장기".into())?;
    *state = next;
    Ok(())
}

struct BloodEffect {
    id: &'static str,
    name: &'static str,
    image_id: &'static str,
}

// 원문의 Object.keys(BLOOD_MOON_EFFECT_CARDS) 순서가 난수 선택의 계약이다.
const BLOOD_EFFECTS: [BloodEffect; 5] = [
    BloodEffect {
        id: "summon",
        name: "권속 소환",
        image_id: "blood-summon",
    },
    BloodEffect {
        id: "veil",
        name: "핏빛 망토",
        image_id: "blood-cloak",
    },
    BloodEffect {
        id: "sunlight",
        name: "밤의 장막",
        image_id: "night-veil",
    },
    BloodEffect {
        id: "curse",
        name: "피의 저주",
        image_id: "blood-curse",
    },
    BloodEffect {
        id: "coffin",
        name: "오래된 관",
        image_id: "old-coffin",
    },
];

fn invalid(field: &str, reason: &str) -> EngineError {
    EngineError::InvalidState(format!("v7 Blood Moon {field}: {reason}"))
}

fn require_v7(state: &GameState) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 Blood Moon callback on rules version {}",
            state.ruleset_id
        )));
    }
    Ok(())
}

fn blood_moon_setup(state: &GameState) -> bool {
    state
        .extra
        .get("campaign")
        .and_then(|value| value.get("setup"))
        .and_then(Value::as_str)
        == Some("bloodMoon")
}

pub(crate) fn source_blood_card_definition() -> Value {
    json!({
        "id":"blood", "instanceId":"blood", "name":"피", "phase":"BLOOD",
        "stars":0, "text":"신선한 피입니다.", "effect":"bloodCard", "campaignCard":true
    })
}

/// BLACK_TOWER_CARD는 공개 256개와 aux GUN의 드래프트 풀에 속하지 않는다.
/// cloneCard/addCardToPlayerDeck가 instanceId와 보관 상태를 붙이기 전의 정의다.
pub(crate) fn source_black_tower_card_definition() -> Value {
    json!({
        "id":"black-tower-legacy-magic", "name":"흑마법", "phase":"???", "stars":5,
        "text":"킹을 흑마법사로 변경하고 나머지 아군 기물을 제거합니다. 제거한 기물 수의 절반만큼 괴물을 소환합니다.",
        "art":"wizard", "imageId":"black-tower", "effect":"blackTowerLegacyMagic",
        "helpItems":[
            {"icons":["darkWizard"],"text":"흑마법사: 킹처럼 이동합니다. 괴물에게 공격당하지 않으며 괴물을 잡을 수 있습니다."},
            {"icons":["monster"],"text":"괴물: 중립 기물이며 턴이 끝날때마다 무작위로 한 칸씩 움직입니다. 일부 예외를 제외하고는 잡히지 않습니다."}
        ],
        "campaignCard":true, "firstTurnCard":true, "used":false, "passiveApplied":false
    })
}

fn invalid_black_tower(field: &str, reason: &str) -> EngineError {
    EngineError::InvalidState(format!("v7 Black Tower {field}: {reason}"))
}

/// frozen 정의의 권한·효과·가격·표시 필드는 고정하고 실제 deck 상태만 허용한다.
/// 첫 이동 예약은 획득 시점에 따라 false일 수 있으며 효과 성공 여부와 별개다.
pub(crate) fn validate_black_tower_card_instance(state: &GameState, card: &CardSlot) -> Result<()> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 Black Tower callback on rules version {}",
            state.ruleset_id
        )));
    }
    if state
        .extra
        .get("campaign")
        .and_then(|value| value.get("setup"))
        .and_then(Value::as_str)
        != Some("blackTower")
    {
        return Err(invalid_black_tower(
            "card",
            "dynamic Black Tower card requires campaign.setup=blackTower",
        ));
    }
    if card.vacant
        || card.id != "black-tower-legacy-magic"
        || card.effect != "blackTowerLegacyMagic"
        || card.stars != 5.0
    {
        return Err(invalid_black_tower(
            "card",
            "id/effect/stars do not match the frozen Black Tower definition",
        ));
    }
    if card.instance_id.trim().is_empty()
        || card.instance_id.chars().count() > 120
        || card.instance_id.chars().any(char::is_control)
    {
        return Err(invalid_black_tower(
            "card.instanceId",
            "must be a nonempty identifier of at most 120 characters",
        ));
    }
    let definition = source_black_tower_card_definition();
    for field in [
        "name",
        "phase",
        "text",
        "art",
        "imageId",
        "helpItems",
        "campaignCard",
    ] {
        if card.extra.get(field) != definition.get(field) {
            return Err(invalid_black_tower(
                &format!("card.{field}"),
                "does not match the frozen Black Tower definition",
            ));
        }
    }
    const BOOLEAN_FIELDS: &[&str] = &[
        "deckCard",
        "passiveApplied",
        "nextTurnPending",
        "firstTurnCard",
        "devCard",
        "disabled",
    ];
    const COUNTER_FIELDS: &[&str] = &[
        "slot",
        "acquiredOrder",
        "nextTurnPendingSinceTurn",
        "usedAt",
    ];
    for (field, value) in &card.extra {
        match field.as_str() {
            "name" | "phase" | "text" | "art" | "imageId" | "helpItems" | "campaignCard" => {}
            "color" => {
                if !matches!(value.as_str(), Some("white" | "black")) {
                    return Err(invalid_black_tower("card.color", "must be white or black"));
                }
            }
            field if BOOLEAN_FIELDS.contains(&field) => {
                if !value.is_null() && !value.is_boolean() {
                    return Err(invalid_black_tower(
                        &format!("card.{field}"),
                        "must be a boolean or null",
                    ));
                }
            }
            field if COUNTER_FIELDS.contains(&field) => {
                if !value.is_null()
                    && !value.as_f64().is_some_and(|number| {
                        number.is_finite()
                            && number >= 0.0
                            && number.fract() == 0.0
                            && number <= 9_007_199_254_740_991.0
                    })
                {
                    return Err(invalid_black_tower(
                        &format!("card.{field}"),
                        "must be a nonnegative JavaScript-safe integer or null",
                    ));
                }
                if field == "slot" && value.as_f64().is_some_and(|slot| slot >= 32.0) {
                    return Err(invalid_black_tower("card.slot", "must be below 32"));
                }
            }
            _ => {
                return Err(invalid_black_tower(
                    &format!("card.{field}"),
                    "unknown Black Tower card metadata",
                ));
            }
        }
    }
    Ok(())
}

/// 원문의 completeCardTargets는 target 없는 카드를 한 번 내보낸다.
/// 흑 차례/왕의 존재는 이후 효과 적용 시 판정하며 raw 후보를 먼저 거르지 않는다.
pub(crate) fn collect_black_tower_card_actions(
    state: &GameState,
    card: &CardSlot,
    color: Color,
) -> Result<Vec<Action>> {
    validate_black_tower_card_instance(state, card)?;
    Ok(vec![Action::card(color, card, None)])
}

pub(crate) fn campaign_king_square(state: &GameState, color: Color) -> Option<Square> {
    // findPiece(color,"king")는 isRoyalKing도 인정하지만 Regency heir는
    // 포함하지 않는다. 실제 능력이 아닌 물리 type/왕권을 row-major로 읽는다.
    (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .find(|&square| {
            state.at(square).is_some_and(|piece| {
                piece.color == color
                    && (matches!(
                        piece.kind.as_str(),
                        "king" | "royalKnight" | "shotgunKing" | "darkWizard"
                    ) || crate::observation::truth(piece.extra.get("crownRoyal"))
                        || crate::observation::truth(piece.extra.get("editorRoyal"))
                        || piece.kind == "merchant" && crate::card_effects::september18(state))
            })
        })
}

pub(crate) fn black_tower_king_square(state: &GameState) -> Option<Square> {
    campaign_king_square(state, Color::Black)
}

fn time_traveler_setup(state: &GameState) -> bool {
    state
        .extra
        .get("campaign")
        .and_then(|value| value.get("setup"))
        .and_then(Value::as_str)
        == Some("timeTraveler")
}

fn time_traveler_data(state: &GameState) -> Result<Option<&Fields>> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 Time Traveler query on rules version {}",
            state.ruleset_id
        )));
    }
    if !time_traveler_setup(state) {
        return Ok(None);
    }
    match state.extra["campaign"].get("timeTraveler") {
        value if !crate::observation::truth(value) => Ok(None),
        Some(value) => value.as_object().map(Some).ok_or_else(|| {
            EngineError::InvalidState(
                "v7 Time Traveler campaign.timeTraveler: must be an object".into(),
            )
        }),
        None => Ok(None),
    }
}

fn normalized_time_phase(value: Option<&Value>) -> &'static str {
    if value.and_then(Value::as_str) == Some("past") {
        "past"
    } else {
        "future"
    }
}

/// timePhaseOf가 piece에 기본 phase를 쓰는 의미를 불변 query에서 읽는다.
pub(crate) fn time_phase_of(state: &GameState, piece: &Piece) -> Result<Option<&'static str>> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 Time Traveler phase query on rules version {}",
            state.ruleset_id
        )));
    }
    if !time_traveler_setup(state) || matches!(piece.kind.as_str(), "wall" | "football") {
        return Ok(None);
    }
    Ok(Some(normalized_time_phase(piece.extra.get("timePhase"))))
}

/// traveler의 truthy phase가 data fallback보다 먼저 평가된다. truthy 잘못된
/// phase도 future로 정규화하므로 그 분기에서는 campaign data를 읽지 않는다.
pub(crate) fn current_time_traveler_phase(state: &GameState) -> Result<&'static str> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 Time Traveler current phase query on rules version {}",
            state.ruleset_id
        )));
    }
    if let Some(phase) = state
        .board
        .iter()
        .flatten()
        .flatten()
        .find(|piece| piece.color == Color::White && piece.kind == "timeTraveler")
        .and_then(|piece| piece.extra.get("timePhase"))
        .filter(|phase| crate::observation::truth(Some(phase)))
    {
        return Ok(normalized_time_phase(Some(phase)));
    }
    Ok(normalized_time_phase(
        time_traveler_data(state)?.and_then(|data| data.get("phase")),
    ))
}

pub(crate) fn is_time_phase_distant(state: &GameState, piece: &Piece) -> Result<bool> {
    if !time_traveler_setup(state)
        || matches!(piece.kind.as_str(), "timeTraveler" | "wall" | "football")
    {
        return Ok(false);
    }
    Ok(time_phase_of(state, piece)? != Some(current_time_traveler_phase(state)?))
}

pub(crate) fn time_traveler_attack_enabled_for(state: &GameState, color: Color) -> Result<bool> {
    Ok(time_traveler_data(state)?
        .and_then(|data| data.get("attackEnabledFor"))
        .and_then(Value::as_str)
        == Some(color.as_str()))
}

/// blackTowerLegacyMagic는 일반 포획·사망 callback을 부르지 않는 직접 board 변환이다.
/// 반환 capture 목록은 비어 있고, 카드 사용·후속 hazard·기록은 호출자가 정산한다.
pub(crate) fn apply_black_tower_card_effect(
    state: &mut GameState,
    card: &CardSlot,
) -> Result<Vec<Piece>> {
    validate_black_tower_card_instance(state, card)?;
    if state.turn != Color::Black {
        return Err(EngineError::IllegalAction);
    }
    if state.board.len() != 8 || state.board.iter().any(|row| row.len() != 8) {
        return Err(invalid_black_tower("board", "must be an 8x8 board"));
    }
    let king_square = black_tower_king_square(state).ok_or(EngineError::IllegalAction)?;
    let mut next = state.clone();
    let mut removed = 0usize;
    for row in 0..8 {
        for col in 0..8 {
            // findPiece의 반환은 {row,col,item}인데 원문은 king.piece를 참조한다.
            // 따라서 왕 ID의 다른 alias도 지워지고 물리 칸마다 removed가 증가한다.
            if (row != usize::from(king_square.row) || col != usize::from(king_square.col))
                && next.board[row][col]
                    .as_ref()
                    .is_some_and(|piece| piece.color == Color::Black)
            {
                next.board[row][col] = None;
                removed += 1;
            }
        }
    }
    let wizard = next.board[usize::from(king_square.row)][usize::from(king_square.col)]
        .as_mut()
        .ok_or_else(|| {
            invalid_black_tower("king", "selected royal disappeared before conversion")
        })?;
    wizard.kind = "darkWizard".into();
    wizard.moved = true;
    wizard.extra.insert("darkWizard".into(), json!(true));
    wizard
        .extra
        .insert("blackMagicOwner".into(), json!("black"));
    let mut count = removed.div_ceil(2);
    for row in 0..8 {
        for col in 0..8 {
            if count != 0 && next.board[row][col].is_none() {
                // 빈칸 조건은 board의 null뿐이다. 일반 배치의 지형 제약은 호출하지 않는다.
                let id = format!(
                    "neutral-monster-{}",
                    crate::draft::random_suffix(
                        next.rng
                            .sample_opaque("source black tower virtual card identity")?
                    )?
                );
                let monster = serde_json::from_value(json!({
                    "color":"neutral", "type":"monster", "moved":true, "shielded":false,
                    "id":id, "blackMagicMonster":true, "blackMagicOwner":"black",
                    "origin":square_name(Square {row:row as u8,col:col as u8})
                }))
                .map_err(EngineError::serialization)?;
                next.board[row][col] = Some(monster);
                count -= 1;
            }
        }
    }
    let canceled = next
        .extra
        .get_mut("castlingCanceled")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid_black_tower("castlingCanceled", "must be an object"))?;
    canceled.insert("black".into(), json!(true));
    *state = next;
    Ok(Vec::new())
}

/// 외부 메타데이터가 phase/effect/별점/캠페인 권한을 바꾸지 못하도록 고정 정의를 인증한다.
/// 보관된 ACG 카드 상태가 되돌아올 때의 공통 상태 필드만 추가로 허용한다.
pub(crate) fn validate_blood_card_instance(state: &GameState, card: &CardSlot) -> Result<()> {
    require_v7(state)?;
    if !blood_moon_setup(state) {
        return Err(invalid(
            "card",
            "dynamic blood card requires campaign.setup=bloodMoon",
        ));
    }
    if card.vacant || card.id != "blood" || card.effect != "bloodCard" || card.stars != 0.0 {
        return Err(invalid(
            "card",
            "id/effect/stars do not match the frozen Blood definition",
        ));
    }
    if card.instance_id.trim().is_empty()
        || card.instance_id.chars().count() > 120
        || card.instance_id.chars().any(char::is_control)
    {
        return Err(invalid(
            "card.instanceId",
            "must be a nonempty identifier of at most 120 characters",
        ));
    }
    for (field, expected) in [
        ("name", "피"),
        ("phase", "BLOOD"),
        ("text", "신선한 피입니다."),
    ] {
        if card.extra.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(invalid(
                &format!("card.{field}"),
                "does not match the frozen Blood definition",
            ));
        }
    }
    if card.extra.get("campaignCard") != Some(&json!(true)) {
        return Err(invalid("card.campaignCard", "must be true"));
    }
    const BOOLEAN_FIELDS: &[&str] = &[
        "deckCard",
        "bloodRevealed",
        "passiveApplied",
        "nextTurnPending",
        "firstTurnCard",
        "devCard",
        "disabled",
    ];
    const COUNTER_FIELDS: &[&str] = &[
        "slot",
        "acquiredOrder",
        "nextTurnPendingSinceTurn",
        "usedAt",
    ];
    for (field, value) in &card.extra {
        match field.as_str() {
            "name" | "phase" | "text" | "campaignCard" | "bloodEffectId" | "imageId" => {}
            "color" => {
                if !matches!(value.as_str(), Some("white" | "black")) {
                    return Err(invalid("card.color", "must be white or black"));
                }
            }
            field if BOOLEAN_FIELDS.contains(&field) => {
                if !value.is_null() && !value.is_boolean() {
                    return Err(invalid(
                        &format!("card.{field}"),
                        "must be a boolean or null",
                    ));
                }
            }
            field if COUNTER_FIELDS.contains(&field) => {
                if !value.is_null()
                    && !value.as_f64().is_some_and(|number| {
                        number.is_finite()
                            && number >= 0.0
                            && number.fract() == 0.0
                            && number <= 9_007_199_254_740_991.0
                    })
                {
                    return Err(invalid(
                        &format!("card.{field}"),
                        "must be a nonnegative JavaScript-safe integer or null",
                    ));
                }
                if field == "slot" && value.as_f64().is_some_and(|slot| slot >= 32.0) {
                    return Err(invalid("card.slot", "must be below 32"));
                }
            }
            _ => {
                return Err(invalid(
                    &format!("card.{field}"),
                    "unknown Blood card metadata",
                ));
            }
        }
    }
    let effect = match card.extra.get("bloodEffectId") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            BLOOD_EFFECTS
                .iter()
                .find(|effect| value.as_str() == Some(effect.id))
                .ok_or_else(|| invalid("card.bloodEffectId", "unknown frozen Blood effect id"))?,
        ),
    };
    if card.extra.get("bloodRevealed") == Some(&json!(true)) && effect.is_none() {
        return Err(invalid(
            "card.bloodRevealed",
            "revealed card requires a known bloodEffectId",
        ));
    }
    if let Some(value) = card.extra.get("imageId") {
        let expected = effect.map_or("blood", |effect| effect.image_id);
        if value.as_str() != Some(expected) {
            return Err(invalid(
                "card.imageId",
                "does not match the frozen Blood effect image",
            ));
        }
    }
    Ok(())
}

fn create_blood_moon_state() -> Value {
    json!({"nightBloodPeriods":{"white":null,"black":null},"veilUntil":null,
        "sunlightOverride":null,"coffinId":null,"coffinIds":[],"lastPeriod":0})
}

/// 원문 bloodMoonState는 비활성 캠페인에 아무 필드도 만들지 않는다.
/// 기존 coffinId만 가진 저장 상태도 원문의 fallback 순서로 정규화한다.
pub(crate) fn normalize_blood_moon(state: &mut GameState) -> Result<bool> {
    require_v7(state)?;
    if !blood_moon_setup(state) {
        return Ok(false);
    }
    let campaign = state
        .extra
        .get_mut("campaign")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid("campaign", "must be an object"))?;
    if !crate::observation::truth(campaign.get("bloodMoon")) {
        campaign.insert("bloodMoon".into(), create_blood_moon_state());
    }
    let data = campaign
        .get_mut("bloodMoon")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid("campaign.bloodMoon", "must be an object"))?;
    if !crate::observation::truth(data.get("nightBloodPeriods")) {
        data.insert(
            "nightBloodPeriods".into(),
            json!({"white":null,"black":null}),
        );
    }
    if !data.get("nightBloodPeriods").is_some_and(Value::is_object) {
        return Err(invalid(
            "campaign.bloodMoon.nightBloodPeriods",
            "must be an object",
        ));
    }
    if !data.get("coffinIds").is_some_and(Value::is_array) {
        let fallback = data
            .get("coffinId")
            .filter(|id| crate::observation::truth(Some(id)))
            .cloned();
        data.insert(
            "coffinIds".into(),
            json!(fallback.into_iter().collect::<Vec<_>>()),
        );
    }
    Ok(true)
}

fn blood_data(state: &GameState) -> Result<Option<&Fields>> {
    require_v7(state)?;
    if !blood_moon_setup(state) {
        return Ok(None);
    }
    match state.extra["campaign"].get("bloodMoon") {
        value if !crate::observation::truth(value) => Ok(None),
        Some(value) => value
            .as_object()
            .map(Some)
            .ok_or_else(|| invalid("campaign.bloodMoon", "must be an object")),
        None => Ok(None),
    }
}

fn blood_data_mut(state: &mut GameState) -> Result<&mut Fields> {
    state
        .extra
        .get_mut("campaign")
        .and_then(|value| value.get_mut("bloodMoon"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid("campaign.bloodMoon", "must be normalized before mutation"))
}

fn add_log(state: &mut GameState, message: String) -> Result<()> {
    if state.extra.get("logs").is_none_or(Value::is_null) {
        state.extra.insert("logs".into(), json!([]));
    }
    crate::replay::add_log(state, message)
}

pub(crate) fn blood_moon_half_turns(state: &GameState) -> u64 {
    u64::from(state.turns_taken.white) + u64::from(state.turns_taken.black)
}

pub(crate) fn is_blood_moon_night(state: &GameState) -> bool {
    blood_moon_setup(state) && state.turns_taken.white.min(state.turns_taken.black) % 7 >= 4
}

fn blood_moon_period(state: &GameState) -> u64 {
    let shared = u64::from(state.turns_taken.white.min(state.turns_taken.black));
    shared / 7 * 2 + u64::from(shared % 7 >= 4)
}

/// 불변 query는 기본 상태를 쓰지 않고 bloodMoonState의 기본값 의미를 읽는다.
/// 실제 실행 callback은 normalize_blood_moon을 먼저 호출한다.
pub(crate) fn uses_blood_moon_night_movement(state: &GameState, color: Color) -> Result<bool> {
    let data = blood_data(state)?;
    Ok(blood_moon_setup(state)
        && (is_blood_moon_night(state)
            || data
                .and_then(|data| data.get("sunlightOverride"))
                .and_then(Value::as_str)
                == Some(color.as_str())))
}

pub(crate) fn is_blood_curse_active(state: &GameState, piece: &Piece) -> Result<bool> {
    if !crate::observation::truth(piece.extra.get("bloodCurse")) {
        return Ok(false);
    }
    let data = blood_data(state)?;
    Ok(blood_moon_setup(state)
        && !is_blood_moon_night(state)
        && data
            .and_then(|data| data.get("sunlightOverride"))
            .and_then(Value::as_str)
            != Some(piece.color.as_str()))
}

pub(crate) fn is_blood_veil_active(state: &GameState, piece: &Piece) -> Result<bool> {
    let data = blood_data(state)?;
    Ok(piece.kind == "vampireLord"
        && data
            .and_then(|data| data.get("veilUntil"))
            .and_then(Value::as_f64)
            .is_some_and(|until| {
                until.is_finite() && (blood_moon_half_turns(state) as f64) < until
            }))
}

fn vampire_present(state: &GameState, color: Color) -> bool {
    state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|piece| piece.color == color && piece.kind == "vampireLord")
}

pub(crate) fn collect_blood_card_actions(
    state: &mut GameState,
    card: &CardSlot,
    color: Color,
) -> Result<Vec<Action>> {
    validate_blood_card_instance(state, card)?;
    if !normalize_blood_moon(state)? || !vampire_present(state, color) {
        return Ok(Vec::new());
    }
    Ok(vec![Action::card(color, card, None)])
}

fn vacant_slot() -> CardSlot {
    CardSlot {
        id: String::new(),
        effect: String::new(),
        instance_id: String::new(),
        stars: 0.0,
        used: false,
        recovering: false,
        vacant: true,
        extra: Fields::new(),
        source_order: Vec::new(),
    }
}

fn player_deck(state: &mut GameState, color: Color) -> Result<&mut Vec<CardSlot>> {
    let deck = state.deck_slots.get_mut(color);
    if deck.len() > 32 {
        return Err(invalid("deckSlots", "campaign deck exceeds 32 slots"));
    }
    // campaign가 있으면 gameDeckSlotCount는 gameStyle과 무관하게 normal의 3칸이다.
    while deck.len() < 3 {
        deck.push(vacant_slot());
    }
    Ok(deck)
}

fn fresh_blood_card(state: &mut GameState) -> Result<CardSlot> {
    let mut value = source_blood_card_definition();
    value["instanceId"] = json!(format!(
        "blood-{}",
        crate::draft::random_suffix(state.rng.sample_opaque("source blood card identity")?)?
    ));
    let fields = value
        .as_object_mut()
        .ok_or_else(|| invalid("card", "frozen definition must be an object"))?;
    fields.insert("bloodEffectId".into(), Value::Null);
    fields.insert("bloodRevealed".into(), json!(false));
    fields.insert("deckCard".into(), json!(true));
    serde_json::from_value(value).map_err(EngineError::serialization)
}

fn grant_blood_card_inner(state: &mut GameState, color: Color, reason: &str) -> Result<bool> {
    if !normalize_blood_moon(state)? || !vampire_present(state, color) {
        return Ok(false);
    }
    let deck = player_deck(state, color)?;
    if deck
        .iter()
        .filter(|card| !card.vacant && card.id == "blood" && !card.used)
        .count()
        >= 3
    {
        return Ok(false);
    }
    let Some(slot) = deck.iter().position(|card| card.vacant) else {
        return Ok(false);
    };
    let mut card = fresh_blood_card(state)?;
    card.extra.insert("slot".into(), json!(slot));
    let previous =
        crate::card_effects::js_number(state.extra.get("cardAcquisitionNonce"), 0).unwrap_or(0.0);
    if !previous.is_finite() || previous < 0.0 || previous.fract() != 0.0 {
        return Err(invalid(
            "cardAcquisitionNonce",
            "must coerce to a nonnegative finite integer",
        ));
    }
    let nonce = if previous == 0.0 { 1.0 } else { previous + 1.0 };
    if !nonce.is_finite() || nonce.abs() > 9_007_199_254_740_991.0 {
        return Err(invalid(
            "cardAcquisitionNonce",
            "acquisition order exceeds the JavaScript-safe range",
        ));
    }
    let nonce = if nonce.fract() == 0.0 && nonce >= 0.0 {
        json!(nonce as u64)
    } else {
        json!(nonce)
    };
    state
        .extra
        .insert("cardAcquisitionNonce".into(), nonce.clone());
    card.extra.insert("acquiredOrder".into(), nonce);
    state.deck_slots.get_mut(color)[slot] = card.clone();
    crate::replay::queue_gain(
        state,
        color,
        &serde_json::to_value(&card).map_err(EngineError::serialization)?,
        "",
    )?;
    add_log(
        state,
        if reason.is_empty() {
            "피 카드 획득".into()
        } else {
            format!("피 카드 획득: {reason}")
        },
    )?;
    crate::flow::note_card_event(state)?;
    Ok(true)
}

pub(crate) fn grant_blood_card(state: &mut GameState, color: Color, reason: &str) -> Result<bool> {
    require_v7(state)?;
    if !blood_moon_setup(state) {
        return Ok(false);
    }
    let mut next = state.clone();
    let granted = grant_blood_card_inner(&mut next, color, reason)?;
    *state = next;
    Ok(granted)
}

pub(crate) fn maybe_grant_night_blood_for_turn(
    state: &mut GameState,
    incoming: Color,
) -> Result<()> {
    require_v7(state)?;
    if !blood_moon_setup(state) {
        return Ok(());
    }
    let mut next = state.clone();
    if normalize_blood_moon(&mut next)?
        && is_blood_moon_night(&next)
        && vampire_present(&next, incoming)
    {
        let period = blood_moon_period(&next);
        let periods = blood_data_mut(&mut next)?
            .get_mut("nightBloodPeriods")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| invalid("nightBloodPeriods", "must be an object"))?;
        if periods.get(incoming.as_str()).and_then(Value::as_f64) != Some(period as f64) {
            // 보유 한도/빈 슬롯으로 획득이 실패해도 이 밤의 첫 자기 턴은 소비된다.
            periods.insert(incoming.as_str().into(), json!(period));
            grant_blood_card(&mut next, incoming, "밤이 된 첫 자기 턴")?;
        }
    }
    *state = next;
    Ok(())
}

pub(crate) fn grant_blood_for_direct_capture(
    state: &mut GameState,
    captured: &Piece,
    attacker: Option<&Piece>,
    capture_square: Square,
    attacker_landing: Option<Square>,
) -> Result<()> {
    require_v7(state)?;
    if !blood_moon_setup(state) {
        return Ok(());
    }
    let mut next = state.clone();
    // 원문은 일반 기물의 포획에도 먼저 bloodMoonState를 호출한다.
    if normalize_blood_moon(&mut next)?
        && let Some(attacker) = attacker
        && attacker.kind == "vampireLord"
        && attacker_landing == Some(capture_square)
        && captured.color != attacker.color
    {
        let owner = attacker.color.owner().ok_or(EngineError::WrongActor)?;
        grant_blood_card(&mut next, owner, "직접 포획")?;
    }
    *state = next;
    Ok(())
}

pub(crate) fn clear_blood_moon_turn_effects(state: &mut GameState, moving: Color) -> Result<()> {
    require_v7(state)?;
    if !blood_moon_setup(state) {
        return Ok(());
    }
    let mut next = state.clone();
    if normalize_blood_moon(&mut next)? {
        let data = blood_data_mut(&mut next)?;
        if data.get("sunlightOverride").and_then(Value::as_str) == Some(moving.as_str()) {
            data.insert("sunlightOverride".into(), Value::Null);
        }
    }
    *state = next;
    Ok(())
}

pub(crate) fn update_blood_moon_cycle_log(state: &mut GameState) -> Result<()> {
    require_v7(state)?;
    if !blood_moon_setup(state) {
        return Ok(());
    }
    let mut next = state.clone();
    if normalize_blood_moon(&mut next)? {
        let period = blood_moon_period(&next);
        let night = is_blood_moon_night(&next);
        let data = blood_data_mut(&mut next)?;
        if data.get("lastPeriod").and_then(Value::as_f64) != Some(period as f64) {
            data.insert("lastPeriod".into(), json!(period));
            add_log(
                &mut next,
                if night {
                    "핏빛 달밤: 밤이 되었습니다.".into()
                } else {
                    "핏빛 달밤: 낮이 되었습니다.".into()
                },
            )?;
        }
    }
    *state = next;
    Ok(())
}

/// markCardUsed/removeDeckCard의 white→black 최초 instanceId 검색 순서를 따른다.
/// Blood에는 used/usedAt를 쓰지 않고 슬롯을 null로 바꾼다.
pub(crate) fn consume_blood_card(state: &mut GameState, card: &CardSlot) -> Result<bool> {
    validate_blood_card_instance(state, card)?;
    let mut next = state.clone();
    for color in [Color::White, Color::Black] {
        let deck = player_deck(&mut next, color)?;
        if let Some(slot) = deck
            .iter()
            .position(|candidate| !candidate.vacant && candidate.instance_id == card.instance_id)
        {
            if deck[slot].effect != "bloodCard" {
                return Err(invalid(
                    "card.instanceId",
                    "resolved live card is not a Blood card",
                ));
            }
            deck[slot] = vacant_slot();
            *state = next;
            return Ok(true);
        }
    }
    *state = next;
    Ok(false)
}

fn square_name(square: Square) -> String {
    format!("{}{}", char::from(b'a' + square.col), 8 - square.row)
}

fn spawn_piece(state: &mut GameState, color: Color, kind: &str) -> Result<Piece> {
    spawn_campaign_piece(state, color.into(), kind)
}

fn spawn_campaign_piece(
    state: &mut GameState,
    color: crate::PieceColor,
    kind: &str,
) -> Result<Piece> {
    let id = format!(
        "{}-{kind}-{}",
        color.as_str(),
        crate::draft::random_suffix(state.rng.sample_opaque("source campaign piece identity")?)?
    );
    serde_json::from_value(
        json!({"color":color,"type":kind,"moved":false,"shielded":false,"id":id}),
    )
    .map_err(EngineError::serialization)
}

fn knight_campaign_setup(state: &GameState) -> Option<&str> {
    state
        .extra
        .get("campaign")
        .and_then(|value| value.get("setup"))
        .and_then(Value::as_str)
        .filter(|setup| matches!(*setup, "knightJourney" | "knightGame"))
}

fn campaign_hero_color(state: &GameState) -> Color {
    if state
        .extra
        .get("campaign")
        .and_then(|value| value.get("playerColor"))
        .and_then(Value::as_str)
        == Some("black")
    {
        Color::Black
    } else {
        Color::White
    }
}

fn invalid_knight_journey(field: &str, reason: &str) -> EngineError {
    EngineError::InvalidState(format!("v7 Knight Journey {field}: {reason}"))
}

fn knight_hint_string(value: &Value, depth: usize) -> Result<String> {
    if depth > 64 {
        return Err(invalid_knight_journey(
            "superHintMoves",
            "Number coercion exceeds depth 64",
        ));
    }
    Ok(match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(_) => String::from_utf8(serde_jcs::to_vec(value).map_err(|error| {
            invalid_knight_journey("superHintMoves", &format!("Number coercion: {error}"))
        })?)
        .map_err(|error| {
            invalid_knight_journey("superHintMoves", &format!("Number coercion UTF-8: {error}"))
        })?,
        Value::Array(values) => values
            .iter()
            .map(|value| knight_hint_string(value, depth + 1))
            .collect::<Result<Vec<_>>>()?
            .join(","),
        Value::Object(_) => "[object Object]".into(),
    })
}

fn knight_hint_number(value: Option<&Value>) -> Result<f64> {
    let number = match value {
        None | Some(Value::Null) => 0.0,
        Some(Value::Bool(value)) => f64::from(u8::from(*value)),
        Some(Value::Number(value)) => value.as_f64().ok_or_else(|| {
            invalid_knight_journey("superHintMoves", "Number is outside the execution range")
        })?,
        Some(value) => {
            let text = knight_hint_string(value, 0)?;
            let text = text.trim();
            if matches!(text, "Infinity" | "+Infinity") {
                return Err(invalid_knight_journey(
                    "superHintMoves",
                    "Number coercion produced a nonfinite value",
                ));
            }
            let radix = if text.starts_with("0x") || text.starts_with("0X") {
                Some(16)
            } else if text.starts_with("0b") || text.starts_with("0B") {
                Some(2)
            } else if text.starts_with("0o") || text.starts_with("0O") {
                Some(8)
            } else {
                None
            };
            if let Some(radix) = radix {
                let digits = &text[2..];
                if digits.is_empty() || !digits.chars().all(|digit| digit.is_digit(radix)) {
                    0.0
                } else {
                    u64::from_str_radix(digits, radix).map_err(|error| {
                        invalid_knight_journey(
                            "superHintMoves",
                            &format!("Number exceeds integer range: {error}"),
                        )
                    })? as f64
                }
            } else if text.is_empty() {
                0.0
            } else {
                // Number(NaN 형태의 입력)||0은 0이다. 양수 overflow는
                // source JSON 경계로 반환할 수 없는 비유한 상태가 된다.
                match text.parse::<f64>() {
                    Ok(number) if number.is_finite() => number,
                    Ok(number) if number.is_infinite() && number.is_sign_negative() => 0.0,
                    Ok(number)
                        if number.is_infinite()
                            && text.chars().any(|value| value.is_ascii_digit()) =>
                    {
                        return Err(invalid_knight_journey(
                            "superHintMoves",
                            "Number coercion produced a nonfinite value",
                        ));
                    }
                    _ => 0.0,
                }
            }
        }
    };
    let number = number.max(0.0);
    if !number.is_finite() || number.fract() == 0.0 && number > 9_007_199_254_740_991.0 {
        return Err(invalid_knight_journey(
            "superHintMoves",
            "normalized Number must be finite and JavaScript-safe",
        ));
    }
    Ok(number)
}

fn normalize_knight_journey(state: &mut GameState) -> Result<bool> {
    if knight_campaign_setup(state).is_none() {
        return Ok(false);
    }
    let campaign = state
        .extra
        .get_mut("campaign")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid_knight_journey("campaign", "must be an object"))?;
    if !crate::observation::truth(campaign.get("knightJourney")) {
        campaign.insert(
            "knightJourney".into(),
            json!({
                "visited":[], "kingSquare":"", "undo":[], "superHintMoves":0
            }),
        );
    }
    let data = campaign
        .get_mut("knightJourney")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid_knight_journey("campaign.knightJourney", "must be an object"))?;
    for field in ["visited", "undo"] {
        if !data.get(field).is_some_and(Value::is_array) {
            data.insert(field.into(), json!([]));
        }
    }
    let hint = knight_hint_number(data.get("superHintMoves"))?;
    data.insert("superHintMoves".into(), json!(hint));
    Ok(true)
}

fn knight_journey_data(state: &GameState) -> Result<Option<&Fields>> {
    if knight_campaign_setup(state).is_none() {
        return Ok(None);
    }
    match state.extra["campaign"].get("knightJourney") {
        value if !crate::observation::truth(value) => Ok(None),
        Some(value) => value
            .as_object()
            .map(Some)
            .ok_or_else(|| invalid_knight_journey("campaign.knightJourney", "must be an object")),
        None => Ok(None),
    }
}

fn knight_journey_data_mut(state: &mut GameState) -> Result<&mut Fields> {
    state
        .extra
        .get_mut("campaign")
        .and_then(|value| value.get_mut("knightJourney"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            invalid_knight_journey(
                "campaign.knightJourney",
                "must be normalized before mutation",
            )
        })
}

/// JSON 상태를 읽은 JS Set의 SameValueZero와 삽입 순서를 보존한다.
/// 별개 JSON object/array는 구조가 같아도 다른 reference이므로 서로 지우지 않는다.
fn knight_visited_set(entries: &[Value]) -> Result<Vec<Value>> {
    if entries.len() > 4096 {
        return Err(invalid_knight_journey(
            "visited",
            "exceeds the 4096-entry state budget",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut visited = Vec::new();
    for value in entries {
        if value.is_array()
            || value.is_object()
            || seen.insert(serde_jcs::to_vec(value).map_err(|error| {
                invalid_knight_journey("visited", &format!("canonical Set identity: {error}"))
            })?)
        {
            visited.push(value.clone());
        }
    }
    Ok(visited)
}

pub(crate) fn knight_journey_ready_to_capture(state: &GameState) -> Result<bool> {
    if knight_campaign_setup(state) != Some("knightJourney") {
        return Ok(false);
    }
    let data = knight_journey_data(state)?;
    let entries = data
        .and_then(|data| data.get("visited"))
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let visited = knight_visited_set(entries)?;
    let mut count = visited.len();
    let king_key = campaign_king_square(state, campaign_hero_color(state).opponent())
        .map(|square| json!(format!("{}-{}", square.row, square.col)))
        .or_else(|| data.and_then(|data| data.get("kingSquare")).cloned());
    if let Some(key) = king_key.filter(|key| crate::observation::truth(Some(key)))
        && !key.is_array()
        && !key.is_object()
    {
        let identity = serde_jcs::to_vec(&key).map_err(|error| {
            invalid_knight_journey("kingSquare", &format!("canonical Set identity: {error}"))
        })?;
        for value in &visited {
            if !value.is_array()
                && !value.is_object()
                && serde_jcs::to_vec(value).map_err(|error| {
                    invalid_knight_journey("visited", &format!("canonical Set identity: {error}"))
                })? == identity
            {
                count -= 1;
                break;
            }
        }
    }
    Ok(count >= 63)
}

fn update_knight_journey_protection(state: &mut GameState) -> Result<()> {
    normalize_knight_journey(state)?;
    if knight_campaign_setup(state) != Some("knightJourney") {
        return Ok(());
    }
    let Some(king_square) = campaign_king_square(state, campaign_hero_color(state).opponent())
    else {
        return Ok(());
    };
    knight_journey_data_mut(state)?.insert(
        "kingSquare".into(),
        json!(format!("{}-{}", king_square.row, king_square.col)),
    );
    let ready = knight_journey_ready_to_capture(state)?;
    let mut king = state
        .at(king_square)
        .cloned()
        .ok_or_else(|| invalid_knight_journey("king", "selected king disappeared"))?;
    if ready {
        let was_protected = crate::observation::truth(king.extra.get("protected"));
        king.extra.shift_remove("protected");
        crate::transition::update_piece(state, &king);
        if was_protected {
            add_log(state, "기사의 여행: 흑 킹의 보호가 해제되었습니다.".into())?;
        }
    } else {
        king.extra.insert("protected".into(), json!(true));
        crate::transition::update_piece(state, &king);
    }
    Ok(())
}

const JOURNEY_KNIGHT_DELTAS: [(i8, i8); 8] = [
    (-2, -1),
    (-2, 1),
    (-1, -2),
    (-1, 2),
    (1, -2),
    (1, 2),
    (2, -1),
    (2, 1),
];
const JOURNEY_HINT_NODE_BUDGET: usize = 250_000;

fn journey_hint_route_exists(state: &GameState) -> Result<bool> {
    let hero = campaign_hero_color(state);
    let Some(current) = (0..8)
        .flat_map(|row| (0..8).map(move |col| Square { row, col }))
        .find(|&square| {
            state
                .at(square)
                .is_some_and(|piece| piece.color == hero && piece.kind == "knight")
        })
    else {
        return Ok(false);
    };
    let Some(king) = campaign_king_square(state, hero.opponent()) else {
        return Ok(false);
    };
    let king_index = king.row * 8 + king.col;
    let required = u64::MAX & !(1u64 << king_index);
    let mut visited = 1u64 << (current.row * 8 + current.col);
    if let Some(entries) = knight_journey_data(state)?
        .and_then(|data| data.get("visited"))
        .and_then(Value::as_array)
    {
        for value in entries {
            if let Some((row, col)) = value
                .as_str()
                .and_then(|key| key.split_once('-'))
                .and_then(|(row, col)| Some((row.parse::<u8>().ok()?, col.parse::<u8>().ok()?)))
                .filter(|(row, col)| *row < 8 && *col < 8)
            {
                visited |= 1u64 << (row * 8 + col);
            }
        }
    }
    for row in 0..8 {
        for col in 0..8 {
            if state
                .at(Square { row, col })
                .is_some_and(|piece| piece.kind == "wall")
            {
                visited |= 1u64 << (row * 8 + col);
            }
        }
    }
    visited &= required;
    let targets: [Vec<u8>; 64] = std::array::from_fn(|index| {
        let square = Square {
            row: index as u8 / 8,
            col: index as u8 % 8,
        };
        JOURNEY_KNIGHT_DELTAS
            .iter()
            .filter_map(|&(dr, dc)| square.offset(dr, dc))
            .map(|square| square.row * 8 + square.col)
            .collect()
    });
    fn search(
        index: u8,
        visited: u64,
        required: u64,
        king: u8,
        targets: &[Vec<u8>; 64],
        failed: &mut BTreeSet<(u8, u64)>,
        examined: &mut usize,
    ) -> Result<bool> {
        *examined += 1;
        if *examined > JOURNEY_HINT_NODE_BUDGET {
            return Err(EngineError::UnsupportedFeature(format!(
                "v7 Knight Journey hint route exceeds the {JOURNEY_HINT_NODE_BUDGET}-node deterministic search budget"
            )));
        }
        if visited & required == required {
            return Ok(targets[usize::from(index)].contains(&king));
        }
        if failed.contains(&(index, visited)) {
            return Ok(false);
        }
        let available = |from: u8, mask: u64| {
            targets[usize::from(from)]
                .iter()
                .copied()
                .filter(|next| required & (1u64 << next) != 0 && mask & (1u64 << next) == 0)
                .collect::<Vec<_>>()
        };
        let mut next_steps = available(index, visited);
        next_steps.sort_by_key(|&next| {
            let mask = visited | (1u64 << next);
            if mask & required == required {
                if targets[usize::from(next)].contains(&king) {
                    0
                } else {
                    99
                }
            } else {
                available(next, mask).len()
            }
        });
        // 각 재귀 호출은 새로운 bit를 추가하므로 깊이는 64를 넘지 않는다.
        for next in next_steps {
            if search(
                next,
                visited | (1u64 << next),
                required,
                king,
                targets,
                failed,
                examined,
            )? {
                return Ok(true);
            }
        }
        failed.insert((index, visited));
        Ok(false)
    }
    search(
        current.row * 8 + current.col,
        visited,
        required,
        king_index,
        &targets,
        &mut BTreeSet::new(),
        &mut 0,
    )
}

fn refresh_knight_journey_hint_counter(state: &mut GameState) -> Result<()> {
    normalize_knight_journey(state)?;
    let active = knight_journey_data(state)?
        .and_then(|data| data.get("superHintMoves"))
        .and_then(Value::as_f64)
        .is_some_and(|moves| moves > 0.0);
    if knight_campaign_setup(state) == Some("knightJourney")
        && active
        && !journey_hint_route_exists(state)?
    {
        knight_journey_data_mut(state)?.insert("superHintMoves".into(), json!(0));
    }
    Ok(())
}

/// true를 반환하면 source callback 자체가 이동 횟수·기록·선택·종료를 정산했다.
/// 호출자는 일반 endMove를 더 실행하지 않는다. 무료 이동 제외는 원문 호출자가 맡는다.
pub(crate) fn handle_knight_journey_moved(
    state: &mut GameState,
    moving: &Piece,
    from: Square,
    to: Square,
) -> Result<bool> {
    if state.ruleset_id != RULES_VERSION_V7 {
        return Err(EngineError::UnsupportedFeature(format!(
            "v7 Knight Journey moved callback on rules version {}",
            state.ruleset_id
        )));
    }
    let hero = campaign_hero_color(state);
    if knight_campaign_setup(state).is_none() || moving.color != hero || moving.kind != "knight" {
        return Ok(false);
    }
    if state.board.len() != 8
        || state.board.iter().any(|row| row.len() != 8)
        || from.row >= 8
        || from.col >= 8
        || to.row >= 8
        || to.col >= 8
    {
        return Err(invalid_knight_journey(
            "move",
            "requires an 8x8 board and two board squares",
        ));
    }
    let mut next = state.clone();
    normalize_knight_journey(&mut next)?;
    let entries = knight_journey_data(&next)?
        .and_then(|data| data.get("visited"))
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_knight_journey("visited", "must be normalized before the move"))?;
    let mut visited = knight_visited_set(entries)?;
    let destination = json!(format!("{}-{}", to.row, to.col));
    if !visited.iter().any(|value| value == &destination) {
        visited.push(destination);
    }
    let visited_count = visited.len();
    let data = knight_journey_data_mut(&mut next)?;
    data.insert("visited".into(), json!(visited));
    let hint = data
        .get("superHintMoves")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    if hint > 0.0 {
        data.insert("superHintMoves".into(), json!(hint - 1.0));
    }
    if next.at(from).is_none() {
        let mut wall = spawn_campaign_piece(&mut next, crate::PieceColor::Neutral, "wall")?;
        wall.extra.insert("origin".into(), json!(square_name(from)));
        wall.moved = true;
        next.board[usize::from(from.row)][usize::from(from.col)] = Some(wall);
    }
    update_knight_journey_protection(&mut next)?;
    if knight_campaign_setup(&next) == Some("knightGame") && visited_count >= 64 {
        crate::flow::end_game(
            &mut next,
            Some(hero),
            "기사의 게임: 모든 칸을 방문했습니다.",
        )?;
        next.move_count = next.move_count.checked_add(1).ok_or_else(|| {
            invalid_knight_journey("moveCount", "overflow while completing the campaign")
        })?;
        crate::replay::record(&mut next, "gameover")?;
        *state = next;
        return Ok(true);
    }
    let moves = if let Some(piece) = next.at(to) {
        crate::movement::v7_legal_move_targets(
            &next,
            piece,
            to,
            crate::movement::V7MoveOptions::default(),
        )?
    } else {
        Vec::new()
    };
    for field in ["selected", "targeting"] {
        next.extra.insert(field.into(), Value::Null);
    }
    next.extra.insert("legalMoves".into(), json!([]));
    next.en_passant = None;
    next.move_count = next
        .move_count
        .checked_add(1)
        .ok_or_else(|| invalid_knight_journey("moveCount", "overflow after the campaign move"))?;
    if moves.is_empty() {
        crate::flow::end_game(
            &mut next,
            Some(hero.opponent()),
            "나이트가 더 이상 움직일 수 없습니다.",
        )?;
        crate::replay::record(&mut next, "gameover")?;
    } else {
        crate::replay::record(&mut next, "knight journey")?;
        next.extra.insert("selected".into(), json!(to));
        next.extra.insert("legalMoves".into(), json!(moves));
        refresh_knight_journey_hint_counter(&mut next)?;
    }
    *state = next;
    Ok(true)
}

fn open_cells(state: &GameState, color: Color, rows: &[u8]) -> Result<Vec<Square>> {
    let mut cells = Vec::new();
    for &row in rows {
        for col in 0..8 {
            let square = Square { row, col };
            if crate::movement::open_placement(state, square, Some(color))? {
                cells.push(square);
            }
        }
    }
    Ok(cells)
}

fn summon_bats(state: &mut GameState, color: Color) -> Result<()> {
    let rows = if color == Color::White {
        [6, 7]
    } else {
        [0, 1]
    };
    let mut cells = open_cells(state, color, &rows)?;
    let count = cells.len();
    // 원문의 전체 Fisher–Yates 소비량을 보존하되 사용하지 않는 tail 순서는
    // semantic chance 밀도에 넣지 않는다. 선택된 두 위치의 순서만 의미가 있다.
    for index in (1..count).rev() {
        let sample = state.rng.sample()?;
        if !sample.is_finite() || !(0.0..1.0).contains(&sample) {
            return Err(invalid("summon RNG", "draw must be finite and in [0,1)"));
        }
        // 새 trace는 실제 ordered permutation; 기존 prefix2 legacy 밀도는 아래에 유지한다.
        state.rng.record_last_probability(
            1.0 / (index + 1) as f64,
            "source Blood Moon summon shuffle",
        )?;
        cells.swap(index, (sample * (index + 1) as f64).floor() as usize);
    }
    if count >= 2
        && let Some(probability) = &mut state.semantic_chance_probability
    {
        *probability /= (count * (count - 1)) as f64;
        if !probability.is_finite() || *probability <= 0.0 {
            return Err(invalid(
                "summon chance",
                "density must be finite and positive",
            ));
        }
    }
    for square in cells.into_iter().take(2) {
        let mut bat = spawn_piece(state, color, "bat")?;
        bat.extra
            .insert("origin".into(), json!(square_name(square)));
        bat.moved = true;
        bat.extra.insert(
            "freshNoCaptureUntil".into(),
            json!(u64::from(*state.turns_taken.get(color)) + 1),
        );
        state.board[usize::from(square.row)][usize::from(square.col)] = Some(bat.clone());
        crate::card_effects::mark_animation(state, &bat)?;
    }
    Ok(())
}

fn apply_curse(state: &mut GameState, color: Color) -> Result<()> {
    let mut seen = BTreeSet::new();
    let candidates = state
        .board
        .iter()
        .flatten()
        .flatten()
        .filter(|piece| {
            piece.color == color.opponent()
                && !matches!(
                    piece.kind.as_str(),
                    "knight" | "pawn" | "wall" | "football" | "coffin"
                )
        })
        .filter(|piece| seen.insert(piece.id.clone()))
        .cloned()
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(());
    }
    let index = crate::transition::sample_choice(state, candidates.len())?;
    let mut target = candidates
        .get(index)
        .cloned()
        .ok_or_else(|| invalid("curse RNG", "selection is outside the candidate pool"))?;
    if matches!(target.kind.as_str(), "king" | "pawn") {
        return Ok(());
    }
    target.extra.insert("bloodCurse".into(), json!(true));
    crate::transition::update_piece(state, &target);
    crate::card_effects::mark_animation(state, &target)?;
    Ok(())
}

fn install_coffin(state: &mut GameState, color: Color) -> Result<()> {
    let cells = open_cells(state, color, &[color.home_row()])?;
    if cells.is_empty() {
        // randomChoice([])도 Math.random을 한 번 호출한다.
        let sample = state
            .rng
            .sample_invariant("source empty Blood Moon coffin placement")?;
        if !sample.is_finite() || !(0.0..1.0).contains(&sample) {
            return Err(invalid("coffin RNG", "draw must be finite and in [0,1)"));
        }
        return Ok(());
    }
    let index = crate::transition::sample_choice(state, cells.len())?;
    let square = *cells
        .get(index)
        .ok_or_else(|| invalid("coffin RNG", "selection is outside the candidate pool"))?;
    let mut coffin = spawn_piece(state, color, "coffin")?;
    coffin.moved = true;
    coffin
        .extra
        .insert("origin".into(), json!(square_name(square)));
    state.board[usize::from(square.row)][usize::from(square.col)] = Some(coffin.clone());
    let data = blood_data_mut(state)?;
    data.insert("coffinId".into(), json!(coffin.id));
    let ids = data
        .get_mut("coffinIds")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| invalid("coffinIds", "must be normalized before installation"))?;
    ids.push(json!(coffin.id));
    if ids.len() > 8 {
        ids.drain(..ids.len() - 8);
    }
    crate::card_effects::mark_animation(state, &coffin)?;
    Ok(())
}

/// 원문의 bloodCard는 card 자체의 공개 결과 메타데이터를 먼저 바꾼다.
/// 일반 &CardSlot dispatcher는 갱신된 세 필드를 live hand에 반영하고,
/// virtual dispatcher는 이 mutable card를 그대로 이어서 사용한다.
pub(crate) fn apply_blood_card_effect(
    state: &mut GameState,
    card: &mut CardSlot,
) -> Result<Vec<Piece>> {
    validate_blood_card_instance(state, card)?;
    let mut next = state.clone();
    let mut next_card = card.clone();
    if !normalize_blood_moon(&mut next)? || !vampire_present(&next, next.turn) {
        return Err(EngineError::IllegalAction);
    }
    let index = crate::transition::sample_choice(&mut next, BLOOD_EFFECTS.len())?;
    let effect = BLOOD_EFFECTS
        .get(index)
        .ok_or_else(|| invalid("effect RNG", "selection is outside the frozen effect pool"))?;
    add_log(&mut next, format!("피 카드: {}", effect.name))?;
    next_card
        .extra
        .insert("bloodEffectId".into(), json!(effect.id));
    next_card
        .extra
        .insert("imageId".into(), json!(effect.image_id));
    next_card.extra.insert("bloodRevealed".into(), json!(true));
    let actor = next.turn;
    match effect.id {
        "summon" => summon_bats(&mut next, actor)?,
        "veil" => {
            let until = blood_moon_half_turns(&next) + 3;
            blood_data_mut(&mut next)?.insert("veilUntil".into(), json!(until));
        }
        "sunlight" => {
            blood_data_mut(&mut next)?.insert("sunlightOverride".into(), json!(actor));
        }
        "curse" => apply_curse(&mut next, actor)?,
        "coffin" => install_coffin(&mut next, actor)?,
        _ => return Err(invalid("effect", "frozen effect dispatch drift")),
    }
    *state = next;
    *card = next_card;
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn janggi_differences(actual: &Value, expected: &Value, path: &str, output: &mut Vec<String>) {
        if actual == expected || output.len() >= 12 {
            return;
        }
        match (actual, expected) {
            (Value::Object(a), Value::Object(b)) => {
                for key in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
                    if output.len() >= 12 {
                        break;
                    }
                    let field = format!("{path}.{key}");
                    match (a.get(key), b.get(key)) {
                        (Some(a), Some(b)) => janggi_differences(a, b, &field, output),
                        (Some(_), None) => output.push(format!("{field}: unexpected native field")),
                        (None, Some(_)) => output.push(format!("{field}: missing native field")),
                        (None, None) => {
                            unreachable!("union contains the field on at least one side")
                        }
                    }
                }
            }
            (Value::Array(a), Value::Array(b)) => {
                if a.len() != b.len() {
                    output.push(format!("{path}.length {} != {}", a.len(), b.len()));
                }
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    janggi_differences(a, b, &format!("{path}[{i}]"), output);
                }
            }
            _ => {
                let preview = |value: &Value| {
                    let serialized = value.to_string();
                    let prefix = serialized.chars().take(240).collect::<String>();
                    if serialized.chars().count() > 240 {
                        format!("{prefix}... ({} UTF-8 bytes)", serialized.len())
                    } else {
                        prefix
                    }
                };
                output.push(format!(
                    "{path}: {} != {}",
                    preview(actual),
                    preview(expected)
                ));
            }
        }
    }

    fn assert_janggi_jcs(actual: &Value, expected: &Value, label: &str) {
        let actual = serde_jcs::to_vec(actual).unwrap();
        let expected = serde_jcs::to_vec(expected).unwrap();
        if actual != expected {
            let actual: Value = serde_json::from_slice(&actual).unwrap();
            let expected: Value = serde_json::from_slice(&expected).unwrap();
            let mut differences = Vec::new();
            janggi_differences(&actual, &expected, "position", &mut differences);
            panic!("{label}: full JCS diverged (first 12 fields): {differences:?}");
        }
    }

    fn janggi_native_envelope(state: GameState, label: &str) -> Value {
        crate::v7_host::V7HostPosition::from_state(state)
            .unwrap_or_else(|error| panic!("{label}: native Position construction failed: {error}"))
            .export_envelope()
            .unwrap_or_else(|error| panic!("{label}: native Position export failed: {error}"))
    }

    /// 원문 startCampaign의 cold prefix를 native seed/config부터 재현한다.
    /// source reset snapshot을 입력으로 사용하지 않고 Before/preview/reset/final과
    /// thin constructor의 전체 Position ID·JCS·RNG·history를 모두 비교한다.
    /// 4개의 local single/offline 경계는 warm UI/온라인 authority를 증명하지 않는다.
    #[test]
    fn local_janggi_initializer_matches_frozen_campaign_receipt_when_supplied() {
        let Some(path) = std::env::var_os("ACCELERATE_V7_JANGGI_CAMPAIGN_CASES") else {
            return;
        };
        let data = std::fs::read_to_string(path).unwrap();
        let expected_keys = BTreeSet::from([
            "offline-white-normal",
            "offline-black-chaos",
            "single-white-grand",
            "single-black-normal",
        ]);
        let mut checked = BTreeSet::new();
        for line in data.lines() {
            let receipt: Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                receipt["sourceSha256"],
                "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c"
            );
            assert_eq!(receipt["sourceContext"], "cold-preview-local-startCampaign");
            assert_eq!(receipt["setup"], "janggi");
            let key = receipt["caseKey"].as_str().unwrap();
            assert!(
                expected_keys.contains(key) && checked.insert(key.to_owned()),
                "unexpected or duplicate Janggi case {key}"
            );
            let player = serde_json::from_value(receipt["playerColor"].clone()).unwrap();
            let mode = receipt["localMode"].as_str().unwrap();
            let seed = receipt["seed"].as_u64().unwrap();
            let config = crate::GameConfig {
                game_style: receipt["style"].as_str().unwrap().into(),
                draft_delete: true,
                ..Default::default()
            };
            // The source recipe calls newGame({gameStyle,draftDelete:true},seed)
            // before changing its module context to a cold local campaign.
            let mut preview_state = crate::v7_new_game::new_game(config.clone(), seed)
                .unwrap_or_else(|error| panic!("{key}: native before newGame failed: {error}"));
            assert_janggi_jcs(
                &janggi_native_envelope(preview_state.clone(), key),
                &receipt["sourceBeforePosition"],
                &format!("{key}/before"),
            );
            let preview = campaign_board(&mut preview_state, "janggi", player).unwrap();
            assert_janggi_jcs(
                &serde_json::to_value(preview).unwrap(),
                &receipt["sourceGoalPreviewBoard"],
                &format!("{key}/preview-board"),
            );
            assert_janggi_jcs(
                &serde_json::to_value(&preview_state.rng).unwrap(),
                &receipt["sourceGoalRng"],
                &format!("{key}/preview-rng"),
            );
            let mut reset_config = config.clone();
            reset_config.game_style = "normal".into();
            let mut state = crate::draft::reset_for_ruleset(
                &reset_config,
                preview_state.rng,
                RULES_VERSION_V7,
                Some(crate::draft::LocalCampaignResetContext {
                    player_color: player,
                    local_mode: mode,
                }),
            )
            .unwrap_or_else(|error| panic!("{key}: native shared campaign reset failed: {error}"));
            assert_janggi_jcs(
                &janggi_native_envelope(state.clone(), key),
                &receipt["sourceResetPosition"],
                &format!("{key}/reset"),
            );
            initialize_campaign(&mut state, "janggi", player, mode).unwrap();
            assert_janggi_jcs(
                &janggi_native_envelope(state, key),
                &receipt["sourcePosition"],
                &format!("{key}/final"),
            );
            let state = crate::v7_new_game::new_local_janggi(config, seed, player, mode)
                .unwrap_or_else(|error| {
                    panic!("{key}: native cold Janggi constructor failed: {error}")
                });
            assert_janggi_jcs(
                &janggi_native_envelope(state, key),
                &receipt["sourcePosition"],
                &format!("{key}/constructor"),
            );
        }
        assert_eq!(
            checked,
            expected_keys
                .into_iter()
                .map(str::to_owned)
                .collect::<BTreeSet<_>>(),
            "all 4 local Janggi cold cases are required"
        );
    }

    fn campaign() -> GameState {
        let mut state: GameState = serde_json::from_value(json!({
            "board":vec![vec![Value::Null;8];8], "turn":"white", "rulesetId":RULES_VERSION_V7,
            "deckSlots":{"white":[null,null,null],"black":[null,null,null]},
            "turnsTaken":{"white":4,"black":4}, "fullMove":12,
            "campaign":{"setup":"bloodMoon"}, "logs":[], "cardAcquisitionNonce":0,
            "repetitionSalt":0, "positionCounts":{"__simType":"Map","entries":[["old",1]]}
        }))
        .unwrap();
        state.board[4][4] = Some(Piece::new("vampireLord", Color::White, "white-vampire"));
        state
    }

    fn card() -> CardSlot {
        let mut value = source_blood_card_definition();
        value["instanceId"] = json!("blood-test");
        value["bloodEffectId"] = Value::Null;
        value["bloodRevealed"] = json!(false);
        value["deckCard"] = json!(true);
        value["slot"] = json!(0);
        value["acquiredOrder"] = json!(1);
        serde_json::from_value(value).unwrap()
    }

    fn black_tower() -> GameState {
        let mut state = campaign();
        state.turn = Color::Black;
        state
            .extra
            .insert("campaign".into(), json!({"setup":"blackTower"}));
        state.extra.insert(
            "castlingCanceled".into(),
            json!({"white":false,"black":false}),
        );
        state.board = vec![vec![None; 8]; 8];
        state.board[0][4] = Some(Piece::new("king", Color::Black, "black-king"));
        state.board[7][4] = Some(Piece::new("king", Color::White, "white-king"));
        state
    }

    fn black_tower_card() -> CardSlot {
        let mut value = source_black_tower_card_definition();
        value["instanceId"] = json!("black-tower-legacy-magic-test");
        value["deckCard"] = json!(true);
        value["slot"] = json!(0);
        value["acquiredOrder"] = json!(1);
        serde_json::from_value(value).unwrap()
    }

    fn knight_journey(setup: &str) -> GameState {
        let mut state = crate::v7_new_game::new_game(
            crate::GameConfig {
                draft_delete: true,
                ..Default::default()
            },
            17,
        )
        .unwrap();
        state.board = vec![vec![None; 8]; 8];
        state.board[4][4] = Some(Piece::new("knight", Color::White, "hero-knight"));
        state.board[0][4] = Some(Piece::new("king", Color::Black, "enemy-king"));
        state.extra.insert(
            "campaign".into(),
            json!({"setup":setup,"playerColor":"white",
            "knightJourney":{"visited":["6-3"],"kingSquare":"0-4","undo":[],"superHintMoves":0}}),
        );
        state.extra.insert("selected".into(), Value::Null);
        state.extra.insert("legalMoves".into(), json!([]));
        state
    }

    #[test]
    fn time_traveler_pure_queries_use_source_defaults_and_do_not_materialize_state() {
        let mut state = campaign();
        state.extra["campaign"]["setup"] = json!("timeTraveler");
        state.board[4][4] = Some(Piece::new("timeTraveler", Color::White, "traveler"));
        let rook = Piece::new("rook", Color::Black, "rook");
        let before = state.clone();
        assert_eq!(time_phase_of(&state, &rook).unwrap(), Some("future"));
        assert_eq!(current_time_traveler_phase(&state).unwrap(), "future");
        assert!(!is_time_phase_distant(&state, &rook).unwrap());
        assert!(!time_traveler_attack_enabled_for(&state, Color::White).unwrap());
        assert_eq!(state, before);
        state.extra["campaign"]["timeTraveler"] =
            json!({"phase":"past","attackEnabledFor":"white","afterimageArmed":true});
        let before = state.clone();
        assert_eq!(current_time_traveler_phase(&state).unwrap(), "past");
        assert!(is_time_phase_distant(&state, &rook).unwrap());
        assert!(time_traveler_attack_enabled_for(&state, Color::White).unwrap());
        assert!(!time_traveler_attack_enabled_for(&state, Color::Black).unwrap());
        assert_eq!(state, before);
        for kind in ["wall", "football"] {
            let piece = Piece::new(kind, Color::Black, kind);
            assert_eq!(time_phase_of(&state, &piece).unwrap(), None);
            assert!(!is_time_phase_distant(&state, &piece).unwrap());
        }
        assert!(!is_time_phase_distant(&state, state.board[4][4].as_ref().unwrap()).unwrap());
    }

    #[test]
    fn current_time_phase_short_circuits_bad_campaign_data_with_truthy_piece_phase() {
        let mut state = campaign();
        state.extra["campaign"]["setup"] = json!("timeTraveler");
        state.extra["campaign"]["timeTraveler"] = json!(true);
        let mut traveler = Piece::new("timeTraveler", Color::White, "traveler");
        traveler.extra.insert("timePhase".into(), json!("unknown"));
        state.board[4][4] = Some(traveler);
        let before = state.clone();
        assert_eq!(current_time_traveler_phase(&state).unwrap(), "future");
        assert!(
            time_traveler_attack_enabled_for(&state, Color::White)
                .unwrap_err()
                .to_string()
                .contains("campaign.timeTraveler: must be an object")
        );
        assert_eq!(state, before);
        state.board[4][4]
            .as_mut()
            .unwrap()
            .extra
            .insert("timePhase".into(), json!(""));
        let before = state.clone();
        assert!(
            current_time_traveler_phase(&state)
                .unwrap_err()
                .to_string()
                .contains("must be an object")
        );
        assert_eq!(state, before);
    }

    #[test]
    fn knight_visited_set_preserves_primitive_identity_and_distinct_json_references() {
        let values = json!([1,1.0,0,-0.0,"1",{"same":true},{"same":true}]);
        let actual = knight_visited_set(values.as_array().unwrap()).unwrap();
        assert_eq!(actual.len(), 5);
        assert_eq!(actual[0], json!(1));
        assert_eq!(actual[1], json!(0));
        assert_eq!(actual[2], json!("1"));
        assert_eq!(actual[3], actual[4]);
        assert_eq!(knight_hint_number(Some(&json!(["0x3"]))).unwrap(), 3.0);
        assert_eq!(knight_hint_number(Some(&json!("-Infinity"))).unwrap(), 0.0);
        assert_eq!(knight_hint_number(Some(&json!([1, 2]))).unwrap(), 0.0);
        assert!(
            knight_hint_number(Some(&json!("Infinity")))
                .unwrap_err()
                .to_string()
                .contains("nonfinite")
        );
    }

    #[test]
    fn knight_journey_callback_owns_same_turn_count_wall_selection_and_history() {
        let mut state = knight_journey("knightJourney");
        state.rng = crate::RngState::seeded(17);
        state.rng.tape = vec![0.5, 0.25];
        let moving = state.board[4][4].as_ref().unwrap().clone();
        let full_move = state.full_move;
        let turns = state.turns_taken.clone();
        let history_count = state.extra["boardHistory"].as_array().unwrap().len();
        assert!(
            handle_knight_journey_moved(
                &mut state,
                &moving,
                Square { row: 6, col: 3 },
                Square { row: 4, col: 4 }
            )
            .unwrap()
        );
        assert_eq!(state.move_count, 1);
        assert_eq!(state.turn, Color::White);
        assert_eq!(state.full_move, full_move);
        assert_eq!(state.turns_taken, turns);
        assert_eq!(
            state.extra["campaign"]["knightJourney"]["visited"],
            json!(["6-3", "4-4"])
        );
        assert_eq!(
            serde_json::to_value(state.board[6][3].as_ref().unwrap()).unwrap(),
            json!({"color":"neutral","type":"wall","moved":true,"shielded":false,"id":"neutral-wall-i","origin":"d2"})
        );
        assert_eq!(
            state.board[0][4].as_ref().unwrap().extra["protected"],
            json!(true)
        );
        assert_eq!(state.extra["selected"], json!({"row":4,"col":4}));
        assert!(!state.extra["legalMoves"].as_array().unwrap().is_empty());
        assert_eq!(
            state.extra["boardHistory"].as_array().unwrap().len(),
            history_count + 1
        );
        assert_eq!(
            state.extra["boardHistory"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["label"],
            json!("knight journey")
        );
        assert!(state.history.is_empty());
    }

    #[test]
    fn knight_game_sixty_fourth_visit_and_trapped_knight_have_distinct_winners() {
        let moving_square = Square { row: 4, col: 4 };
        let origin = Square { row: 6, col: 3 };
        let mut complete = knight_journey("knightGame");
        complete.extra["campaign"]["knightJourney"]["visited"] = json!(
            (0..8)
                .flat_map(|row| (0..8).map(move |col| (row, col)))
                .filter(|&(row, col)| row != 4 || col != 4)
                .map(|(row, col)| format!("{row}-{col}"))
                .collect::<Vec<_>>()
        );
        let moving = complete.at(moving_square).unwrap().clone();
        assert!(
            handle_knight_journey_moved(&mut complete, &moving, origin, moving_square).unwrap()
        );
        assert_eq!(complete.mode, "gameover");
        assert_eq!(complete.winner.as_deref(), Some("white"));
        assert_eq!(
            complete.extra["replayEndReason"],
            json!("기사의 게임: 모든 칸을 방문했습니다.")
        );
        assert_eq!(complete.move_count, 1);
        let mut trapped = knight_journey("knightJourney");
        for row in 0..8 {
            for col in 0..8 {
                if (row, col) != (4, 4) && (row, col) != (0, 4) && (row, col) != (6, 3) {
                    trapped.board[row][col] = Some(Piece::new(
                        "wall",
                        crate::PieceColor::Neutral,
                        format!("wall-{row}-{col}"),
                    ));
                }
            }
        }
        let moving = trapped.at(moving_square).unwrap().clone();
        assert!(handle_knight_journey_moved(&mut trapped, &moving, origin, moving_square).unwrap());
        assert_eq!(trapped.mode, "gameover");
        assert_eq!(trapped.winner.as_deref(), Some("black"));
        assert_eq!(
            trapped.extra["replayEndReason"],
            json!("나이트가 더 이상 움직일 수 없습니다.")
        );
        assert_eq!(trapped.move_count, 1);
    }

    #[test]
    fn knight_journey_release_and_hint_no_route_preserve_source_record_order() {
        let mut state = knight_journey("knightJourney");
        let king = Square { row: 0, col: 4 };
        state.board[0][4]
            .as_mut()
            .unwrap()
            .extra
            .insert("protected".into(), json!(true));
        state.extra["campaign"]["knightJourney"]["visited"] = json!(
            (0..8)
                .flat_map(|row| (0..8).map(move |col| (row, col)))
                .filter(|&(row, col)| (row, col) != (0, 4))
                .map(|(row, col)| format!("{row}-{col}"))
                .collect::<Vec<_>>()
        );
        let moving = state.board[4][4].as_ref().unwrap().clone();
        assert!(
            handle_knight_journey_moved(
                &mut state,
                &moving,
                Square { row: 6, col: 3 },
                Square { row: 4, col: 4 }
            )
            .unwrap()
        );
        assert!(knight_journey_ready_to_capture(&state).unwrap());
        assert!(!state.at(king).unwrap().extra.contains_key("protected"));
        assert!(
            state.extra["logs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|log| log.as_str() == Some("기사의 여행: 흑 킹의 보호가 해제되었습니다."))
        );
        let mut hint = knight_journey("knightJourney");
        hint.board[0][0] = hint.board[0][4].take();
        hint.extra["campaign"]["knightJourney"]["superHintMoves"] = json!(3);
        hint.extra["campaign"]["knightJourney"]["visited"] = json!(
            (0..8)
                .flat_map(|row| (0..8).map(move |col| (row, col)))
                .filter(|&(row, col)| (row, col) != (0, 0) && (row, col) != (2, 3))
                .map(|(row, col)| format!("{row}-{col}"))
                .collect::<Vec<_>>()
        );
        let moving = hint.board[4][4].as_ref().unwrap().clone();
        assert!(
            handle_knight_journey_moved(
                &mut hint,
                &moving,
                Square { row: 6, col: 3 },
                Square { row: 4, col: 4 }
            )
            .unwrap()
        );
        assert_eq!(hint.mode, "play");
        assert_eq!(
            hint.extra["campaign"]["knightJourney"]["superHintMoves"].as_f64(),
            Some(0.0)
        );
        assert_eq!(
            hint.extra["boardHistory"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["label"],
            json!("knight journey")
        );
    }

    #[test]
    fn knight_journey_inactive_and_late_error_leave_source_state_and_rng_unchanged() {
        let mut state = knight_journey("knightJourney");
        let mut moving = state.board[4][4].as_ref().unwrap().clone();
        moving.kind = "rook".into();
        let before = state.clone();
        assert!(
            !handle_knight_journey_moved(
                &mut state,
                &moving,
                Square { row: 6, col: 3 },
                Square { row: 4, col: 4 }
            )
            .unwrap()
        );
        assert_eq!(state, before);
        moving.kind = "knight".into();
        state.move_count = u32::MAX;
        let before = state.clone();
        assert!(
            handle_knight_journey_moved(
                &mut state,
                &moving,
                Square { row: 6, col: 3 },
                Square { row: 4, col: 4 }
            )
            .unwrap_err()
            .to_string()
            .contains("moveCount: overflow")
        );
        assert_eq!(state, before);
    }

    fn campaign_source_state(state: &GameState) -> Value {
        let mut value = serde_json::to_value(state).expect("serialize native campaign state");
        let fields = value
            .as_object_mut()
            .expect("native campaign state must be an object");
        // 엔진 외부 wrapper만 분리한다. RNG/history/rules는 각 comparator가 따로 검사한다.
        for wrapper in ["rulesetId", "rng", "history"] {
            fields.remove(wrapper);
        }
        value
    }

    fn journey_receipt_differences(
        actual: &Value,
        expected: &Value,
        path: &str,
        output: &mut Vec<String>,
    ) {
        // 비교는 전체 JCS를 유지한다. 출력만 제한해 byte 배열 대신 정확한 첫 필드를 보여 준다.
        if output.len() >= 16
            || serde_jcs::to_vec(actual).unwrap() == serde_jcs::to_vec(expected).unwrap()
        {
            return;
        }
        match (actual, expected) {
            (Value::Object(a), Value::Object(b)) => {
                for key in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
                    if output.len() >= 16 {
                        break;
                    }
                    let field_path = format!("{path}.{key}");
                    match (a.get(key), b.get(key)) {
                        (Some(a), Some(b)) => {
                            journey_receipt_differences(a, b, &field_path, output)
                        }
                        (Some(_), None) => output.push(format!(
                            "{field_path}: native field exists, source field is absent"
                        )),
                        (None, Some(_)) => output.push(format!(
                            "{field_path}: native field is absent, source field exists"
                        )),
                        (None, None) => {
                            unreachable!("union of source/native keys must contain the field")
                        }
                    }
                }
            }
            (Value::Array(a), Value::Array(b)) => {
                if a.len() != b.len() {
                    output.push(format!(
                        "{path}.length: native {} != source {}",
                        a.len(),
                        b.len()
                    ));
                }
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    if output.len() >= 16 {
                        break;
                    }
                    journey_receipt_differences(a, b, &format!("{path}[{i}]"), output);
                }
            }
            _ => {
                let preview = |value: &Value| {
                    let text = value.to_string();
                    let mut prefix = text.chars().take(240).collect::<String>();
                    if text.chars().count() > 240 {
                        prefix.push_str("…");
                    }
                    prefix
                };
                output.push(format!(
                    "{path}: native {} != source {}",
                    preview(actual),
                    preview(expected)
                ));
            }
        }
    }

    #[test]
    #[ignore = "동결 Knight Journey callback 영수증 경로를 명시한 통합 검증에서만 실행한다"]
    fn frozen_knight_journey_callbacks_match_full_state_rng_and_history() {
        let path = std::env::var_os("ACCELERATE_V7_KNIGHT_JOURNEY_RECEIPTS").expect(
            "ACCELERATE_V7_KNIGHT_JOURNEY_RECEIPTS must name the source-generated callback receipt",
        );
        let path = std::path::PathBuf::from(path);
        assert!(
            std::fs::metadata(&path)
                .expect("read Knight Journey receipt metadata")
                .len()
                <= 16 * 1024 * 1024,
            "Knight Journey receipt exceeds the 16 MiB comparison budget"
        );
        let receipt: Value =
            serde_json::from_slice(&std::fs::read(path).expect("read Knight Journey receipt"))
                .expect("parse Knight Journey receipt JSON");
        assert_eq!(receipt["schemaVersion"], json!(1));
        assert_eq!(receipt["status"], json!("source-generated"));
        assert_eq!(receipt["nativeCompared"], json!(false));
        assert_eq!(
            receipt["sourceSha256"],
            json!("e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c")
        );
        assert_eq!(
            receipt["executionProfile"],
            json!("accelerate-headless-semantic-v7-faithful-init-v1")
        );
        assert_eq!(
            receipt["sourceBoundary"],
            json!("restored-synthetic-before-position"),
            "the completed source before Position must be restored before the callback"
        );
        assert_eq!(
            receipt["executionProfileMetadata"]["profileVersion"],
            receipt["executionProfile"]
        );
        assert_eq!(
            receipt["executionProfileMetadata"]["selectedInitializerCount"],
            json!(175)
        );
        assert_eq!(
            receipt["executionProfileMetadata"]["excludedInitializerCount"],
            json!(168)
        );
        assert_eq!(receipt["rulesVersion"], json!(RULES_VERSION_V7));
        assert_eq!(receipt["semanticFieldsExcluded"], json!([]));
        let rows = receipt["rows"]
            .as_array()
            .expect("Knight Journey receipt rows must be an array");
        let expected_names = BTreeSet::from([
            "journey-continue",
            "journey-default-data",
            "journey-origin-occupied",
            "journey-release-king",
            "knight-game-64th-square",
            "journey-trapped",
            "journey-hint-route",
            "journey-hint-no-route",
            "journey-black-hero",
            "journey-other-piece-inert",
        ]);
        assert_eq!(
            rows.len(),
            expected_names.len(),
            "all bounded Knight Journey callback cases are required"
        );
        assert_eq!(
            rows.iter()
                .map(|row| row["name"]
                    .as_str()
                    .expect("Knight Journey row name required"))
                .collect::<BTreeSet<_>>(),
            expected_names,
            "Knight Journey case identities are incomplete or duplicated"
        );
        let mut failures = Vec::new();
        for row in rows {
            let name = row["name"].as_str().unwrap();
            assert_eq!(
                row["callback"],
                json!("handleKnightJourneyMoved"),
                "{name}: callback identity"
            );
            assert_eq!(
                row["checkpoint"],
                json!(
                    "restored-synthetic-before-position-direct-callback-after-snapshot-microtask-settlement"
                ),
                "{name}: source/native invocation boundary must agree"
            );
            let before = &row["before"];
            let after = &row["after"];
            let mut restore_differences = Vec::new();
            journey_receipt_differences(
                &row["restoredBefore"],
                before,
                "restoredBefore",
                &mut restore_differences,
            );
            assert!(
                restore_differences.is_empty(),
                "{name}: source actual before restore must preserve the complete Position: {restore_differences:?}"
            );
            assert_eq!(
                before["rulesVersion"],
                json!(RULES_VERSION_V7),
                "{name}: before rules"
            );
            assert_eq!(
                after["rulesVersion"],
                json!(RULES_VERSION_V7),
                "{name}: after rules"
            );
            let imported = crate::v7_host::V7HostPosition::from_envelope(before.clone())
                .unwrap_or_else(|error| {
                    panic!("{name}: native complete source-position import: {error}")
                });
            let mut state = imported.state().clone();
            let mut import_differences = Vec::new();
            journey_receipt_differences(
                &campaign_source_state(&state),
                &before["state"],
                "before.state",
                &mut import_differences,
            );
            assert!(
                import_differences.is_empty(),
                "{name}: native import must preserve every source state field: {import_differences:?}"
            );
            let moving: Piece = serde_json::from_value(row["moving"].clone())
                .expect("source moving piece required");
            let from: Square =
                serde_json::from_value(row["from"].clone()).expect("source origin square required");
            let to: Square = serde_json::from_value(row["to"].clone())
                .expect("source destination square required");
            let handled = handle_knight_journey_moved(&mut state, &moving, from, to)
                .unwrap_or_else(|error| panic!("{name}: native Knight Journey callback: {error}"));
            assert_eq!(
                json!(handled),
                row["returned"],
                "{name}: source callback ownership flag"
            );
            crate::replay::settle(&mut state).unwrap_or_else(|error| {
                panic!("{name}: native snapshot replay settlement: {error}")
            });
            let actual = crate::v7_host::V7HostPosition::from_state(state.clone())
                .and_then(|host| host.export_envelope())
                .unwrap_or_else(|error| {
                    panic!("{name}: native complete source-position export: {error}")
                });
            let mut differences = Vec::new();
            journey_receipt_differences(&actual, after, "after", &mut differences);
            if !differences.is_empty() {
                failures.push(format!("{name}: {differences:?}"));
            }
            assert_eq!(
                state.ruleset_id, RULES_VERSION_V7,
                "{name}: engine wrapper rules version"
            );
        }
        assert!(
            failures.is_empty(),
            "source Knight Journey differences:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn dynamic_black_tower_definition_authenticates_help_and_campaign_metadata() {
        let mut state = black_tower();
        let mut magic = black_tower_card();
        validate_black_tower_card_instance(&state, &magic).unwrap();
        magic.extra.insert("firstTurnCard".into(), json!(false));
        magic.used = true;
        magic.extra.insert("usedAt".into(), json!(0));
        validate_black_tower_card_instance(&state, &magic).unwrap();
        magic.extra.insert("target".into(), json!("own-king"));
        assert!(
            validate_black_tower_card_instance(&state, &magic)
                .unwrap_err()
                .to_string()
                .contains("card.target: unknown Black Tower card metadata")
        );
        magic.extra.shift_remove("target");
        magic.extra["helpItems"][0]["icons"] = json!(["arbitrary-authority"]);
        assert!(
            validate_black_tower_card_instance(&state, &magic)
                .unwrap_err()
                .to_string()
                .contains("card.helpItems: does not match")
        );
        magic = black_tower_card();
        magic.extra.insert("phase".into(), json!("OPENING"));
        assert!(
            validate_black_tower_card_instance(&state, &magic)
                .unwrap_err()
                .to_string()
                .contains("card.phase: does not match")
        );
        magic = black_tower_card();
        state.extra["campaign"]["setup"] = json!("bloodMoon");
        assert!(
            validate_black_tower_card_instance(&state, &magic)
                .unwrap_err()
                .to_string()
                .contains("requires campaign.setup=blackTower")
        );
    }

    #[test]
    fn black_tower_removal_ceil_and_spawn_use_physical_row_order_and_identity_draws() {
        let mut state = black_tower();
        state.board[0][0] = Some(Piece::new("pawn", Color::Black, "pawn-0"));
        state.board[0][1] = Some(Piece::new("rook", Color::Black, "rook-1"));
        state.board[1][0] = Some(Piece::new("bishop", Color::Black, "bishop-2"));
        state.board[0][2] = Some(Piece::new("pawn", Color::White, "white-blocker"));
        state.board[0][3] = Some(Piece::new(
            "wall",
            crate::PieceColor::Neutral,
            "neutral-wall",
        ));
        // 직접 board null 검색은 일반 기물 배치를 막는 지형을 거르지 않는다.
        state
            .extra
            .insert("collapsedSquares".into(), json!([{"row":0,"col":0}]));
        state.rng.tape = vec![0.5, 0.25];
        state.semantic_chance_probability = Some(1.0);
        let magic = black_tower_card();
        let before = state.clone();
        assert!(
            apply_black_tower_card_effect(&mut state, &magic)
                .unwrap()
                .is_empty()
        );
        assert_eq!(state.board[0][4].as_ref().unwrap().kind, "darkWizard");
        assert_eq!(state.board[0][4].as_ref().unwrap().id, "black-king");
        assert!(state.board[0][4].as_ref().unwrap().moved);
        assert_eq!(
            state.board[0][4].as_ref().unwrap().extra["darkWizard"],
            json!(true)
        );
        assert_eq!(
            state.board[0][4].as_ref().unwrap().extra["blackMagicOwner"],
            json!("black")
        );
        assert_eq!(
            serde_json::to_value(state.board[0][0].as_ref().unwrap()).unwrap(),
            json!({"color":"neutral","type":"monster","moved":true,"shielded":false,
                "id":"neutral-monster-i","blackMagicMonster":true,"blackMagicOwner":"black","origin":"a8"})
        );
        assert_eq!(state.board[0][1].as_ref().unwrap().id, "neutral-monster-9");
        assert!(state.board[1][0].is_none());
        assert_eq!(state.board[0][2], before.board[0][2]);
        assert_eq!(state.board[0][3], before.board[0][3]);
        assert_eq!(state.captures, before.captures);
        assert_eq!(state.deck_slots, before.deck_slots);
        assert_eq!(state.history, before.history);
        assert_eq!(state.semantic_chance_probability, Some(1.0));
        assert_eq!(state.rng.cursor, 2);
        let first = before
            .rng
            .state
            .wrapping_mul(1664525)
            .wrapping_add(1013904223);
        assert_eq!(
            state.rng.state,
            first.wrapping_mul(1664525).wrapping_add(1013904223)
        );
        let mut expected_fields = before.extra;
        expected_fields["castlingCanceled"]["black"] = json!(true);
        assert_eq!(state.extra, expected_fields);
        assert!(!magic.used);
    }

    #[test]
    fn black_tower_source_king_piece_typo_removes_aliases_and_counts_every_cell() {
        let mut state = black_tower();
        state.board[1][4] = state.board[0][4].clone();
        let rook = Piece::new("bigRook", Color::Black, "same-large-rook");
        state.board[2][4] = Some(rook.clone());
        state.board[2][5] = Some(rook);
        state.rng.tape = vec![0.5, 0.25];
        apply_black_tower_card_effect(&mut state, &black_tower_card()).unwrap();
        assert!(state.board[1][4].is_none());
        assert!(state.board[2][4].is_none());
        assert!(state.board[2][5].is_none());
        assert_eq!(
            state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|piece| piece.kind == "darkWizard")
                .count(),
            1
        );
        assert_eq!(
            state
                .board
                .iter()
                .flatten()
                .flatten()
                .filter(|piece| piece.kind == "monster")
                .count(),
            2
        );
        assert_eq!(state.rng.cursor, 2);
    }

    #[test]
    fn black_tower_find_king_uses_frozen_royal_identity_without_regency_heirs() {
        let mut state = black_tower();
        let mut heir = Piece::new("queen", Color::Black, "heir");
        heir.extra.insert("regencyHeir".into(), json!(true));
        state.board[0][0] = Some(heir);
        state.extra.insert("regency".into(), json!({"black":true}));
        state.extra.insert("kingDead".into(), json!({"black":true}));
        assert_eq!(
            black_tower_king_square(&state),
            Some(Square { row: 0, col: 4 })
        );
        let mut royal = Piece::new("knight", Color::Black, "crowned-knight");
        royal.extra.insert("crownRoyal".into(), json!(true));
        state.board[0][1] = Some(royal);
        assert_eq!(
            black_tower_king_square(&state),
            Some(Square { row: 0, col: 1 })
        );
        state.board[0][1] = Some(Piece::new("merchant", Color::Black, "merchant"));
        assert_eq!(
            black_tower_king_square(&state),
            Some(Square { row: 0, col: 1 })
        );
        state
            .extra
            .insert("september18Balance".into(), json!(false));
        assert_eq!(
            black_tower_king_square(&state),
            Some(Square { row: 0, col: 4 })
        );
    }

    #[test]
    fn black_tower_raw_candidates_precede_actor_or_king_decline_and_preserve_state() {
        let mut state = black_tower();
        let magic = black_tower_card();
        state.turn = Color::White;
        assert_eq!(
            collect_black_tower_card_actions(&state, &magic, Color::White).unwrap(),
            vec![Action::card(Color::White, &magic, None)]
        );
        let before = state.clone();
        assert_eq!(
            apply_black_tower_card_effect(&mut state, &magic),
            Err(EngineError::IllegalAction)
        );
        assert_eq!(state, before);
        state.turn = Color::Black;
        state.board[0][4] = None;
        assert_eq!(
            collect_black_tower_card_actions(&state, &magic, Color::Black)
                .unwrap()
                .len(),
            1
        );
        let before = state.clone();
        assert_eq!(
            apply_black_tower_card_effect(&mut state, &magic),
            Err(EngineError::IllegalAction)
        );
        assert_eq!(state, before);
        let mut lone_king = black_tower();
        let before_rng = lone_king.rng.clone();
        apply_black_tower_card_effect(&mut lone_king, &magic).unwrap();
        assert_eq!(lone_king.rng, before_rng);
        assert_eq!(lone_king.board[0][4].as_ref().unwrap().kind, "darkWizard");
        assert!(
            !lone_king
                .board
                .iter()
                .flatten()
                .flatten()
                .any(|piece| piece.kind == "monster")
        );
    }

    #[test]
    fn black_tower_errors_roll_back_converted_board_and_prior_identity_draws() {
        let mut state = black_tower();
        state.board[1][0] = Some(Piece::new("pawn", Color::Black, "pawn-0"));
        state.board[1][1] = Some(Piece::new("pawn", Color::Black, "pawn-1"));
        state.board[1][2] = Some(Piece::new("pawn", Color::Black, "pawn-2"));
        state.rng.tape = vec![0.5, 1.0];
        let before = state.clone();
        let error = apply_black_tower_card_effect(&mut state, &black_tower_card()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("random identity value outside [0,1)")
        );
        assert_eq!(state, before);
        state.rng.tape[1] = 0.25;
        state.extra.insert("castlingCanceled".into(), json!(false));
        let before = state.clone();
        let error = apply_black_tower_card_effect(&mut state, &black_tower_card()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Black Tower castlingCanceled: must be an object")
        );
        assert_eq!(state, before);
    }

    #[test]
    #[ignore = "동결 source callback 영수증 경로를 명시한 통합 검증에서만 실행한다"]
    fn frozen_black_tower_callbacks_match_full_state_rng_and_history() {
        let path = std::env::var_os("ACCELERATE_V7_CAMPAIGN_CALLBACK_RECEIPTS")
            .expect("ACCELERATE_V7_CAMPAIGN_CALLBACK_RECEIPTS must name the source-generated callback receipt");
        let path = std::path::PathBuf::from(path);
        assert!(
            std::fs::metadata(&path)
                .expect("read callback receipt metadata")
                .len()
                <= 16 * 1024 * 1024,
            "callback receipt exceeds the 16 MiB comparison budget"
        );
        let bytes = std::fs::read(path).expect("read the explicit source callback receipt");
        let receipt: Value =
            serde_json::from_slice(&bytes).expect("parse source callback receipt JSON");
        assert_eq!(receipt["schemaVersion"], json!(1));
        assert_eq!(
            receipt["returnContractVersion"],
            json!(1),
            "callback return presence contract must be explicit"
        );
        assert_eq!(receipt["status"], json!("source-generated"));
        assert_eq!(receipt["nativeCompared"], json!(false));
        assert_eq!(
            receipt["sourceSha256"],
            json!("e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c")
        );
        assert_eq!(
            receipt["executionProfile"],
            json!("accelerate-headless-semantic-v7-faithful-init-v1")
        );
        assert_eq!(receipt["rulesVersion"], json!(RULES_VERSION_V7));
        assert_eq!(receipt["definition"], source_black_tower_card_definition());
        assert_eq!(receipt["semanticFieldsExcluded"], json!([]));
        let rows = receipt["rows"]
            .as_array()
            .expect("source callback rows must be an array");
        let expected_names = BTreeSet::from([
            "black-tower-lone-king",
            "black-tower-ceil-three",
            "black-tower-initial-fifteen",
            "black-tower-allied-footprint-cells",
            "black-tower-source-king-piece-typo",
            "black-tower-crown-royal-recipient",
            "black-tower-wrong-actor",
            "black-tower-no-king",
        ]);
        assert_eq!(
            rows.len(),
            expected_names.len(),
            "all bounded source callback cases must be present"
        );
        let names = rows
            .iter()
            .map(|row| row["name"].as_str().expect("source row name required"))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            names, expected_names,
            "source callback case identities are incomplete or duplicated"
        );
        for row in rows {
            let name = row["name"].as_str().unwrap();
            assert_eq!(
                row["callback"],
                json!("blackTowerLegacyMagic"),
                "{name}: callback identity"
            );
            let before = &row["before"];
            let after = &row["after"];
            assert_eq!(
                before["rulesVersion"],
                json!(RULES_VERSION_V7),
                "{name}: before rules"
            );
            assert_eq!(
                after["rulesVersion"],
                json!(RULES_VERSION_V7),
                "{name}: after rules"
            );
            let mut state: GameState = serde_json::from_value(before["state"].clone())
                .unwrap_or_else(|error| panic!("{name}: native source-state import: {error}"));
            state.ruleset_id = RULES_VERSION_V7.into();
            state.rng = serde_json::from_value(before["rng"].clone())
                .unwrap_or_else(|error| panic!("{name}: native source-RNG import: {error}"));
            state.history = serde_json::from_value(before["history"].clone())
                .unwrap_or_else(|error| panic!("{name}: native source-history import: {error}"));
            assert_eq!(
                serde_jcs::to_vec(&campaign_source_state(&state)).unwrap(),
                serde_jcs::to_vec(&before["state"]).unwrap(),
                "{name}: native import must preserve every source state field"
            );
            let card = state
                .deck_slots
                .black
                .first()
                .expect("source callback black deck has a slot")
                .clone();
            let result = apply_black_tower_card_effect(&mut state, &card);
            assert_eq!(
                row["returnPresence"],
                json!("value"),
                "{name}: frozen callback explicitly returns a value"
            );
            let source_ok = row["returned"]["ok"]
                .as_bool()
                .expect("source callback result.ok must be boolean");
            assert_eq!(
                result.is_ok(),
                source_ok,
                "{name}: source returned {:?}, native returned {result:?}",
                row["returned"]
            );
            if source_ok {
                assert!(
                    result.unwrap().is_empty(),
                    "{name}: direct erase must not emit captures"
                );
                assert_eq!(
                    row["returned"],
                    json!({"ok":true,"message":"흑마법사가 깨어나고 중립 괴물을 소환했습니다."}),
                    "{name}: complete source success return value"
                );
            } else {
                assert_eq!(
                    result,
                    Err(EngineError::IllegalAction),
                    "{name}: source normal decline classification"
                );
                let expected_message = if before["state"]["turn"] == json!("white") {
                    "검은 마탑 전용 카드입니다."
                } else {
                    "아군 킹이 없습니다."
                };
                assert_eq!(
                    row["returned"],
                    json!({"ok":false,"message":expected_message}),
                    "{name}: complete source decline return value"
                );
            }
            assert_eq!(
                serde_jcs::to_vec(&campaign_source_state(&state)).unwrap(),
                serde_jcs::to_vec(&after["state"]).unwrap(),
                "{name}: full source state differs after the callback"
            );
            assert_eq!(
                serde_json::to_value(&state.rng).unwrap(),
                after["rng"],
                "{name}: full RNG state/tape/cursor"
            );
            assert_eq!(
                json!(state.history),
                after["history"],
                "{name}: full history"
            );
            assert_eq!(
                state.ruleset_id, RULES_VERSION_V7,
                "{name}: engine wrapper rules version"
            );
        }
    }

    #[test]
    fn dynamic_blood_definition_rejects_campaign_and_metadata_forgery() {
        let mut state = campaign();
        let mut blood = card();
        validate_blood_card_instance(&state, &blood).unwrap();
        blood.extra.insert("actType".into(), json!("PASSIVE"));
        assert!(
            validate_blood_card_instance(&state, &blood)
                .unwrap_err()
                .to_string()
                .contains("card.actType: unknown Blood card metadata")
        );
        blood.extra.shift_remove("actType");
        blood
            .extra
            .insert("bloodEffectId".into(), json!("arbitrary-effect"));
        assert!(
            validate_blood_card_instance(&state, &blood)
                .unwrap_err()
                .to_string()
                .contains("unknown frozen Blood effect id")
        );
        blood.extra.insert("bloodEffectId".into(), json!("veil"));
        blood.extra.insert("imageId".into(), json!("blood-summon"));
        assert!(
            validate_blood_card_instance(&state, &blood)
                .unwrap_err()
                .to_string()
                .contains("card.imageId")
        );
        blood.extra.shift_remove("imageId");
        state.extra.shift_remove("campaign");
        assert!(
            validate_blood_card_instance(&state, &blood)
                .unwrap_err()
                .to_string()
                .contains("requires campaign.setup=bloodMoon")
        );
    }

    #[test]
    fn first_night_grant_uses_two_identity_draws_and_full_move_not_draft_phase() {
        let mut state = campaign();
        state.rng.tape = vec![0.5, 0.25];
        state
            .extra
            .insert("draft".into(), json!({"phase":"OPENING"}));
        maybe_grant_night_blood_for_turn(&mut state, Color::White).unwrap();
        let blood = &state.deck_slots.white[0];
        assert_eq!(blood.instance_id, "blood-i");
        assert_eq!(blood.extra["phase"], json!("BLOOD"));
        assert_eq!(blood.extra["acquiredOrder"], json!(1));
        assert_eq!(state.extra["cardAcquisitionNonce"], json!(1));
        assert_eq!(
            state.extra["campaign"]["bloodMoon"]["nightBloodPeriods"]["white"],
            json!(1)
        );
        assert_eq!(state.extra["pendingNotation"]["text"], json!("+피"));
        assert_eq!(state.extra["pendingNotation"]["moveNumber"], json!(12));
        assert_eq!(
            state.extra["logs"][0],
            json!("피 카드 획득: 밤이 된 첫 자기 턴")
        );
        assert_eq!(state.extra["repetitionSalt"], json!(1));
        assert_eq!(state.extra["positionCounts"]["entries"], json!([]));
        assert_eq!(state.rng.cursor, 2);
        let after = state.clone();
        maybe_grant_night_blood_for_turn(&mut state, Color::White).unwrap();
        assert_eq!(state, after);
    }

    #[test]
    fn capped_night_is_recorded_before_grant_and_consumption_removes_the_slot() {
        let mut state = campaign();
        for _ in 0..3 {
            assert!(grant_blood_card(&mut state, Color::White, "").unwrap());
        }
        let cursor = state.rng.cursor;
        maybe_grant_night_blood_for_turn(&mut state, Color::White).unwrap();
        assert_eq!(state.rng.cursor, cursor);
        assert_eq!(
            state.extra["campaign"]["bloodMoon"]["nightBloodPeriods"]["white"],
            json!(1)
        );
        let blood = state.deck_slots.white[0].clone();
        assert!(consume_blood_card(&mut state, &blood).unwrap());
        assert!(state.deck_slots.white[0].vacant);
        assert_eq!(
            serde_json::to_value(&state).unwrap()["deckSlots"]["white"][0],
            Value::Null
        );
        assert!(!state.deck_slots.white[0].extra.contains_key("usedAt"));
        maybe_grant_night_blood_for_turn(&mut state, Color::White).unwrap();
        assert!(state.deck_slots.white[0].vacant);
        assert_eq!(state.rng.cursor, cursor);
    }

    #[test]
    fn full_deck_first_night_and_absent_attacker_preserve_source_normalization() {
        let mut state = campaign();
        for slot in &mut state.deck_slots.white {
            *slot = card();
            slot.used = true;
        }
        maybe_grant_night_blood_for_turn(&mut state, Color::White).unwrap();
        assert_eq!(state.rng.cursor, 0);
        assert_eq!(
            state.extra["campaign"]["bloodMoon"]["nightBloodPeriods"]["white"],
            json!(1)
        );
        let mut normalized = campaign();
        let victim = Piece::new("rook", Color::Black, "victim");
        grant_blood_for_direct_capture(
            &mut normalized,
            &victim,
            None,
            Square { row: 0, col: 0 },
            None,
        )
        .unwrap();
        assert_eq!(
            normalized.extra["campaign"]["bloodMoon"]["coffinIds"],
            json!([])
        );
        assert_eq!(normalized.rng.cursor, 0);
        assert!(normalized.deck_slots.white.iter().all(|card| card.vacant));
    }

    #[test]
    fn grant_rolls_back_rng_deck_and_normalization_when_shared_log_is_invalid() {
        let mut state = campaign();
        state
            .extra
            .insert("logs".into(), json!({"invalid":"array expected"}));
        let before = state.clone();
        let error = maybe_grant_night_blood_for_turn(&mut state, Color::White).unwrap_err();
        assert!(error.to_string().contains("logs must be an array"));
        assert_eq!(state, before);
    }

    #[test]
    fn veil_expiration_and_sunlight_override_use_different_clocks() {
        let mut state = campaign();
        let mut blood = card();
        state.rng.tape = vec![0.25];
        apply_blood_card_effect(&mut state, &mut blood).unwrap();
        assert_eq!(blood.extra["bloodEffectId"], json!("veil"));
        assert_eq!(blood.extra["imageId"], json!("blood-cloak"));
        assert_eq!(blood.extra["bloodRevealed"], json!(true));
        assert_eq!(state.extra["campaign"]["bloodMoon"]["veilUntil"], json!(11));
        let lord = state.board[4][4].as_ref().unwrap().clone();
        state.turns_taken.white = 5;
        state.turns_taken.black = 5;
        assert!(is_blood_veil_active(&state, &lord).unwrap());
        state.turns_taken.white = 6;
        assert!(!is_blood_veil_active(&state, &lord).unwrap());
        state.turns_taken.white = 0;
        state.turns_taken.black = 0;
        state.rng.tape.push(0.45);
        apply_blood_card_effect(&mut state, &mut blood).unwrap();
        assert_eq!(blood.extra["bloodEffectId"], json!("sunlight"));
        assert!(uses_blood_moon_night_movement(&state, Color::White).unwrap());
        assert!(!uses_blood_moon_night_movement(&state, Color::Black).unwrap());
        clear_blood_moon_turn_effects(&mut state, Color::Black).unwrap();
        assert!(uses_blood_moon_night_movement(&state, Color::White).unwrap());
        clear_blood_moon_turn_effects(&mut state, Color::White).unwrap();
        assert!(!uses_blood_moon_night_movement(&state, Color::White).unwrap());
    }

    #[test]
    fn bat_spawn_preserves_full_shuffle_draws_source_shape_and_capture_lock() {
        let mut state = campaign();
        state.rng.tape = vec![0.0; 18];
        state.rng.tape[0] = 0.01;
        state.rng.tape[16] = 0.5;
        state.rng.tape[17] = 0.25;
        let mut blood = card();
        apply_blood_card_effect(&mut state, &mut blood).unwrap();
        assert_eq!(state.rng.cursor, 18);
        assert_eq!(
            serde_json::to_value(state.board[6][1].as_ref().unwrap()).unwrap(),
            json!({"color":"white","type":"bat","moved":true,"shielded":false,
                "id":"white-bat-i","origin":"b2","freshNoCaptureUntil":5})
        );
        assert_eq!(state.board[6][2].as_ref().unwrap().id, "white-bat-9");
        assert_eq!(blood.extra["bloodEffectId"], json!("summon"));
    }

    #[test]
    fn curse_skips_pawns_knights_and_king_effect_but_preserves_choice_draw() {
        let mut state = campaign();
        state.board[0][0] = Some(Piece::new("pawn", Color::Black, "pawn"));
        state.board[0][1] = Some(Piece::new("knight", Color::Black, "knight"));
        state.board[0][2] = Some(Piece::new("king", Color::Black, "king"));
        state.board[0][3] = Some(Piece::new("rook", Color::Black, "rook"));
        state.rng.tape = vec![0.7, 0.0, 0.7, 0.75];
        let mut blood = card();
        apply_blood_card_effect(&mut state, &mut blood).unwrap();
        assert_eq!(state.rng.cursor, 2);
        assert!(
            !state.board[0][2]
                .as_ref()
                .unwrap()
                .extra
                .contains_key("bloodCurse")
        );
        apply_blood_card_effect(&mut state, &mut blood).unwrap();
        assert_eq!(state.rng.cursor, 4);
        let rook = state.board[0][3].as_ref().unwrap();
        assert_eq!(rook.extra["bloodCurse"], json!(true));
        assert!(!is_blood_curse_active(&state, rook).unwrap());
        state.turns_taken.white = 0;
        state.turns_taken.black = 0;
        assert!(is_blood_curse_active(&state, state.board[0][3].as_ref().unwrap()).unwrap());
    }

    #[test]
    fn coffin_empty_home_still_draws_and_installation_keeps_latest_eight_ids() {
        let mut state = campaign();
        for col in 0..8 {
            state.board[7][col] = Some(Piece::new("wall", Color::White, format!("wall-{col}")));
        }
        let mut blood = card();
        state.rng.tape = vec![0.95, 0.5, 0.95, 0.0, 0.5];
        apply_blood_card_effect(&mut state, &mut blood).unwrap();
        assert_eq!(state.rng.cursor, 2);
        assert_eq!(
            state.extra["campaign"]["bloodMoon"]["coffinId"],
            Value::Null
        );
        state.board[7][2] = None;
        state.extra["campaign"]["bloodMoon"]["coffinIds"] =
            json!(["a", "b", "c", "d", "e", "f", "g", "h"]);
        apply_blood_card_effect(&mut state, &mut blood).unwrap();
        assert_eq!(state.rng.cursor, 5);
        assert_eq!(state.board[7][2].as_ref().unwrap().id, "white-coffin-i");
        assert_eq!(
            state.extra["campaign"]["bloodMoon"]["coffinId"],
            json!("white-coffin-i")
        );
        assert_eq!(
            state.extra["campaign"]["bloodMoon"]["coffinIds"],
            json!(["b", "c", "d", "e", "f", "g", "h", "white-coffin-i"])
        );
    }
}
