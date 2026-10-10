//! 10월 7일 규칙의 검증된 switcheroo 이동 경계.
//! 다른 10월 행동은 명시적으로 지원하지 않는다.
use crate::{
    Action, ActionKind, Color, EngineError, GameState, MoveTarget, RULES_VERSION_OCTOBER, Result,
    Square,
};
use serde_json::{Value, json};

const CATALOG: &str = "disfTpO_11gGrXKr6Q_AO_SsQHVecw5XIXQ0mJExQ4k";
const PROFILE: &str = "accelerate-headless-october-draft-probe-v1";
const SOURCE: &str = "958e8e6787d8d107152e4c07e45736d2ffbf05ad8fad4e7de63558c3c70d024c";
const INPUT_FIELDS: &[&str] = &[
    "board",
    "deckSlots",
    "captures",
    "turn",
    "mode",
    "actionsRemaining",
    "switcheroo",
    "moveCount",
    "turnsTaken",
    "cardsUsedThisTurn",
    "winner",
    "fullMove",
    "octoberCatalogHash",
    "octoberExecutionProfile",
    "octoberSourceMainSha256",
];

pub(crate) fn check_input_shape(raw: &Value) -> Result<()> {
    let fields = raw.as_object().ok_or_else(|| {
        EngineError::InvalidState("October switcheroo snapshot must be an object".into())
    })?;
    if INPUT_FIELDS.iter().any(|name| !fields.contains_key(*name))
        || fields
            .keys()
            .any(|name| name != "rulesetId" && !INPUT_FIELDS.contains(&name.as_str()))
    {
        return Err(EngineError::UnsupportedFeature(
            "October snapshot contains missing or unreviewed state fields".into(),
        ));
    }
    Ok(())
}

pub(crate) fn check_profile(state: &GameState) -> Result<()> {
    let manifest: Value = serde_json::from_str(include_str!(
        "../../contracts/catalog/execution-profile-20261007-probe.json"
    ))
    .map_err(EngineError::serialization)?;
    if manifest["rulesVersion"] != RULES_VERSION_OCTOBER
        || manifest["sourcePublicCatalogHash"] != CATALOG
        || manifest["profileVersion"] != PROFILE
        || manifest["sourceMainSha256"] != SOURCE
        || state.extra.get("octoberCatalogHash") != Some(&json!(CATALOG))
        || state.extra.get("octoberExecutionProfile") != Some(&json!(PROFILE))
        || state.extra.get("octoberSourceMainSha256") != Some(&json!(SOURCE))
    {
        return Err(EngineError::InvalidState(
            "October rules/catalog/profile mismatch".into(),
        ));
    }
    Ok(())
}

pub(crate) fn visible_board(state: &GameState, viewer: Color) -> Result<Value> {
    check_profile(state)?;
    if state
        .extra
        .get("camouflageRule")
        .is_some_and(|value| value == &json!(true))
        || state
            .extra
            .get("fogOfWar")
            .is_some_and(|value| value == &json!(true))
    {
        return Err(EngineError::UnsupportedFeature(
            "October viewer visibility effect".into(),
        ));
    }
    let mut board = Vec::new();
    for (row, line) in state.board.iter().enumerate() {
        for (col, piece) in line.iter().enumerate() {
            let Some(piece) = piece else { continue };
            if piece.extra.get("hiddenFrom").is_some_and(|hidden| {
                hidden
                    .as_str()
                    .is_some_and(|text| text.contains(viewer.as_str()))
                    || hidden
                        .as_array()
                        .is_some_and(|colors| colors.contains(&json!(viewer.as_str())))
            }) {
                return Err(EngineError::UnsupportedFeature(
                    "October hidden piece projection".into(),
                ));
            }
            board.push(json!({"row":row,"col":col,"type":piece.kind,"color":piece.color}));
        }
    }
    Ok(json!(board))
}

fn candidate(state: &GameState, color: Color, from: Square, to: Square) -> Result<Action> {
    check_profile(state)?;
    if state.ruleset_id != RULES_VERSION_OCTOBER
        || state.mode != "play"
        || state.turn != color
        || state.actions_remaining != 1
        || state.result().is_some()
        || from.row >= 8
        || from.col >= 8
        || to.row >= 8
        || to.col >= 8
        || !state.flag("switcheroo", color)
    {
        return Err(EngineError::IllegalAction);
    }
    let king = state.at(from).ok_or(EngineError::IllegalAction)?;
    let pawn = state.at(to).ok_or(EngineError::IllegalAction)?;
    if king.kind != "king"
        || king.color != color
        || pawn.kind != "pawn"
        || pawn.color != color
        || king.id == pawn.id
        || (king.flag("undergroundBunker")
            && crate::card_effects::js_number(king.extra.get("hp"), 0).is_some_and(f64::is_finite))
        || !state
            .deck_slots
            .get(color)
            .iter()
            .any(|card| card.id == "switcheroo" && card.used)
    {
        return Err(EngineError::IllegalAction);
    }
    let mut target = MoveTarget::at(to);
    target.flags.insert("switcherooMove".into(), json!(true));
    Ok(Action::movement(color, from, target))
}

pub(crate) fn bind(state: &GameState, intent: &Value) -> Result<Action> {
    let fields = intent.as_object().ok_or(EngineError::IllegalAction)?;
    if fields.get("type") == Some(&json!("card")) {
        if fields.len() != 4
            || fields.get("cardId") != Some(&json!("switcheroo"))
            || fields.get("target") != Some(&Value::Null)
        {
            return Err(EngineError::IllegalAction);
        }
        let color: Color = serde_json::from_value(
            fields
                .get("color")
                .cloned()
                .ok_or(EngineError::IllegalAction)?,
        )
        .map_err(EngineError::serialization)?;
        return card_candidate(state, color);
    }
    if fields.len() != 4 || fields["type"] != "move" {
        return Err(EngineError::IllegalAction);
    }
    let color: Color = serde_json::from_value(
        fields
            .get("color")
            .cloned()
            .ok_or(EngineError::IllegalAction)?,
    )
    .map_err(EngineError::serialization)?;
    let from: Square = serde_json::from_value(
        fields
            .get("from")
            .cloned()
            .ok_or(EngineError::IllegalAction)?,
    )
    .map_err(EngineError::serialization)?;
    let to: Square = serde_json::from_value(
        fields
            .get("destination")
            .cloned()
            .ok_or(EngineError::IllegalAction)?,
    )
    .map_err(EngineError::serialization)?;
    candidate(state, color, from, to)
}

fn card_candidate(state: &GameState, color: Color) -> Result<Action> {
    check_profile(state)?;
    if state.ruleset_id != RULES_VERSION_OCTOBER
        || state.mode != "play"
        || state.turn != color
        || state.actions_remaining != 1
        || state.result().is_some()
        || state.flag("switcheroo", color)
        || state
            .extra
            .get("switcheroo")
            .and_then(|flags| flags.get(color.as_str()))
            != Some(&json!(false))
        || *state.cards_used_this_turn.get(color) != 0
    {
        return Err(EngineError::IllegalAction);
    }
    let card = state
        .deck_slots
        .get(color)
        .iter()
        .find(|card| card.id == "switcheroo" && !card.vacant && !card.used && !card.recovering)
        .ok_or(EngineError::IllegalAction)?;
    let mobile_king = state
        .board
        .iter()
        .flatten()
        .flatten()
        .find(|piece| piece.color == color && piece.kind == "king")
        .is_some_and(|piece| {
            !(piece.flag("undergroundBunker")
                && crate::card_effects::js_number(piece.extra.get("hp"), 0)
                    .is_some_and(f64::is_finite))
        });
    let pawn = state
        .board
        .iter()
        .flatten()
        .flatten()
        .any(|piece| piece.color == color && piece.kind == "pawn");
    if !mobile_king || !pawn {
        return Err(EngineError::IllegalAction);
    }
    Ok(Action::card(color, card, None))
}

pub(crate) fn validate(state: &GameState, action: &Action) -> Result<()> {
    if action.kind == ActionKind::Card {
        if *action == card_candidate(state, action.color)? {
            return Ok(());
        }
        return Err(EngineError::IllegalAction);
    }
    if action.kind != ActionKind::Move || action.card_id.is_some() || action.target.is_some() {
        return Err(EngineError::UnsupportedFeature(
            "October action outside switcheroo scope".into(),
        ));
    }
    let from = action.from.ok_or(EngineError::IllegalAction)?;
    let target = action
        .destination
        .as_ref()
        .ok_or(EngineError::IllegalAction)?;
    let expected = candidate(
        state,
        action.color,
        from,
        Square {
            row: target.row,
            col: target.col,
        },
    )?;
    if *action != expected {
        return Err(EngineError::IllegalAction);
    }
    Ok(())
}

pub(crate) fn apply(state: &mut GameState, action: &Action) -> Result<Vec<crate::Piece>> {
    validate(state, action)?;
    if action.kind == ActionKind::Card {
        let card = state
            .deck_slots
            .get_mut(action.color)
            .iter_mut()
            .find(|card| Some(&card.instance_id) == action.card_instance_id.as_ref())
            .expect("validated card");
        card.used = true;
        state
            .extra
            .entry("switcheroo")
            .and_modify(|flags| flags[action.color.as_str()] = json!(true));
        *state.cards_used_this_turn.get_mut(action.color) += 1;
        return Ok(Vec::new());
    }
    let from = action.from.expect("validated origin");
    let to = action.destination.as_ref().expect("validated destination");
    let to = Square {
        row: to.row,
        col: to.col,
    };
    let mut king = state.board[from.row as usize][from.col as usize]
        .take()
        .expect("validated king");
    let mut pawn = state.board[to.row as usize][to.col as usize]
        .take()
        .expect("validated pawn");
    king.moved = true;
    pawn.moved = true;
    state.board[to.row as usize][to.col as usize] = Some(king);
    state.board[from.row as usize][from.col as usize] = Some(pawn);
    state
        .extra
        .entry("switcheroo")
        .and_modify(|flags| flags[action.color.as_str()] = json!(false));
    state.move_count += 1;
    *state.turns_taken.get_mut(action.color) += 1;
    *state.cards_used_this_turn.get_mut(action.color) = 0;
    state.turn = action.color.opponent();
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameConfig, RULES_VERSION_V7};

    #[test]
    fn september_switcheroo_still_removes_the_pawn() {
        let mut state = crate::v7_new_game::new_game(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .expect("September source state");
        assert_eq!(state.ruleset_id, RULES_VERSION_V7);
        state.mode = "play".into();
        state.turn = Color::White;
        state.actions_remaining = 1;
        state
            .extra
            .insert("switcheroo".into(), json!({"white":true,"black":false}));
        let from = Square { row: 7, col: 4 };
        let to = Square { row: 6, col: 0 };
        let pawn_id = state.at(to).expect("pawn").id.clone();
        let mut target = MoveTarget::at(to);
        target.flags.insert("switcherooMove".into(), json!(true));
        let action = Action::movement(Color::White, from, target);
        crate::v7_move_transition::execute(&mut state, &action, false).expect("September move");
        assert_eq!(state.at(to).expect("king").kind, "king");
        assert!(
            state
                .board
                .iter()
                .flatten()
                .flatten()
                .all(|piece| piece.id != pawn_id)
        );
    }
}
