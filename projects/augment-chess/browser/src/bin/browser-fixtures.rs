//! Bounded, public-only native reference for the real browser/WASM CI test.
//! No fixture import is exposed by BrowserGameSession.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use adapter_runtime::{
        AdapterOutcome, AdapterRequest, AdapterSelection, InvocationControl, NeverCancelled,
    };
    use augment_chess_engine::{
        Color, GameConfig,
        adapter::{GameAdapterPayload, GameAdapterSession, GameAdapterValue},
    };
    use serde_json::{Value, json};

    fn request(
        session: &GameAdapterSession,
        capability_id: &str,
        payload: GameAdapterPayload,
    ) -> AdapterRequest<GameAdapterPayload> {
        let descriptors = session.descriptors();
        let descriptor = descriptors
            .into_iter()
            .find(|descriptor| {
                descriptor
                    .capabilities
                    .iter()
                    .any(|capability| capability.id == capability_id)
            })
            .expect("reference case uses a declared capability");
        let capability = descriptor
            .capabilities
            .iter()
            .find(|capability| capability.id == capability_id)
            .expect("reference case uses a declared capability");
        AdapterRequest {
            request_id: capability_id.into(),
            selection: AdapterSelection {
                project_id: descriptor.project_id.clone(),
                adapter_id: descriptor.adapter_id.clone(),
                contract_version: descriptor.contract_version,
                implementation_version: descriptor.implementation_version.clone(),
                capability_id: capability.id.clone(),
                request_schema: capability.request_schema.clone(),
                response_schema: capability.response_schema.clone(),
            },
            snapshot_revision: session.position().position_id().to_owned(),
            call_limits: descriptor.call_limits,
            payload,
        }
    }

    fn record(
        session: &mut GameAdapterSession,
        steps: &mut Vec<Value>,
        request: AdapterRequest<GameAdapterPayload>,
    ) -> Result<Option<GameAdapterValue>, Box<dyn std::error::Error>> {
        let cancellation = NeverCancelled;
        let result = session.invoke(&request, &InvocationControl::unlimited_time(&cancellation));
        if let Err(error) = &result {
            eprintln!(
                "native reference request {} failed: {}",
                request.request_id,
                serde_json::to_string(error)?
            );
        }
        let value = result.as_ref().ok().map(|response| response.result.clone());
        steps.push(json!({"request": request, "outcome": AdapterOutcome::from(result)}));
        Ok(value)
    }

    let mut cases = Vec::new();
    for style in ["normal", "chaos", "grand"] {
        let config = GameConfig {
            game_style: style.into(),
            ..Default::default()
        };
        let mut session = GameAdapterSession::new_game(config.clone(), 19)?;
        let mut steps = Vec::new();
        for viewer in [Color::White, Color::Black] {
            let request = request(&session, "observe", GameAdapterPayload::Observe { viewer });
            record(&mut session, &mut steps, request)?;
        }
        let page_request = request(
            &session,
            "legal-actions-page",
            GameAdapterPayload::LegalActionsPage {
                limit: 1,
                max_examined: 64,
                cursor: None,
            },
        );
        let page = record(&mut session, &mut steps, page_request)?;
        let Some(GameAdapterValue::LegalActionsPage {
            intents, cursor, ..
        }) = page
        else {
            return Err(format!("{style}: initial public action page did not succeed").into());
        };
        let intent = intents
            .first()
            .cloned()
            .ok_or_else(|| format!("{style}: initial page has no public intent"))?;
        if let Some(cursor) = cursor {
            let continuation = request(
                &session,
                "legal-actions-page",
                GameAdapterPayload::LegalActionsPage {
                    limit: 1,
                    max_examined: 64,
                    cursor: Some(cursor),
                },
            );
            record(&mut session, &mut steps, continuation)?;
        }
        let apply = request(
            &session,
            "apply-public-intent",
            GameAdapterPayload::ApplyPublicIntent { intent },
        );
        let stale = apply.clone();
        if !matches!(
            record(&mut session, &mut steps, apply)?,
            Some(GameAdapterValue::AppliedPublicIntent { .. })
        ) {
            return Err(format!("{style}: draft public intent did not commit").into());
        }
        record(&mut session, &mut steps, stale)?;
        for viewer in [Color::White, Color::Black] {
            let request = request(&session, "observe", GameAdapterPayload::Observe { viewer });
            record(&mut session, &mut steps, request)?;
        }
        let mut wrong_schema = request(
            &session,
            "observe",
            GameAdapterPayload::Observe {
                viewer: Color::White,
            },
        );
        wrong_schema.selection.request_schema.sha256 = "0".repeat(64);
        record(&mut session, &mut steps, wrong_schema)?;
        let final_result = serde_json::to_value(session.position().state().result())?;
        cases.push(json!({
            "config": config, "seed": 19, "steps": steps,
            "finalRevision": session.position().position_id(),
            "finalDecisionActor": session.position().state().decision_actor().as_str(),
            "finalResult": final_result,
        }));
    }

    #[cfg(feature = "browser-test-fixtures")]
    for fixture_id in augment_chess_browser::fixtures::CASE_IDS {
        let mut session = augment_chess_browser::fixtures::session(fixture_id)?;
        let mut steps = Vec::new();
        let mut observed = Vec::new();
        for viewer in [Color::White, Color::Black] {
            let observe = request(&session, "observe", GameAdapterPayload::Observe { viewer });
            let Some(GameAdapterValue::Observation { observation }) =
                record(&mut session, &mut steps, observe)?
            else {
                return Err(format!("{fixture_id}: observation failed").into());
            };
            if serde_json::to_string(&observation)?.contains("must-not-be-public") {
                return Err(format!(
                    "{fixture_id}: private fixture marker entered public projection"
                )
                .into());
            }
            observed.push(observation);
        }
        if fixture_id == "private-projection" {
            if observed[0].board[2][3]
                .as_ref()
                .and_then(|piece| piece["type"].as_str())
                == Some("rook")
                || observed[1].board[2][3]
                    .as_ref()
                    .and_then(|piece| piece["type"].as_str())
                    != Some("rook")
            {
                return Err("private-projection: hidden piece visibility witness failed".into());
            }
        }
        let intent = if fixture_id == "ordered-pawn-storm" {
            // The completed source UI prefix can contain ten clicks. The
            // bounded AI collector's depth-eight family is not an authority
            // for rejecting this public UI command. Bind the exact retained
            // ordered prefix through the engine's scalar public boundary.
            json!({
                "type":"card", "color":"white", "cardId":"pawn-storm",
                "cardInstanceId":"test-pawn-storm",
                "target":{"selections":session.position().state().extra["targeting"]["pawnStorm"]},
            })
        } else {
            let page_request = request(
                &session,
                "legal-actions-page",
                GameAdapterPayload::LegalActionsPage {
                    limit: 4096,
                    max_examined: 65536,
                    cursor: None,
                },
            );
            let Some(GameAdapterValue::LegalActionsPage { intents, .. }) =
                record(&mut session, &mut steps, page_request)?
            else {
                return Err(format!("{fixture_id}: action page failed").into());
            };
            intents
                .iter()
                .find(|intent| match fixture_id {
                    "promotion-choice" => {
                        intent["type"] == "promotionChoice" && intent["promotionType"] == "queen"
                    }
                    "large-piece-move" => {
                        intent["type"] == "move"
                            && intent["from"] == json!({"row":3,"col":1})
                            && intent["destination"]["row"] == 2
                            && intent["destination"]["col"] == 1
                    }
                    "private-projection" => {
                        intent["type"] == "move"
                            && intent["from"] == json!({"row":1,"col":4})
                            && intent["destination"]["row"] == 0
                            && intent["destination"]["col"] == 4
                    }
                    _ => false,
                })
                .cloned()
                .ok_or_else(|| format!("{fixture_id}: required public intent missing"))?
        };
        if fixture_id == "ordered-pawn-storm" {
            if intent["target"]["selections"]
                .as_array()
                .is_none_or(|selections| selections.len() != 10)
            {
                return Err("ordered-pawn-storm: ten-click public selection witness failed".into());
            }
            let mut reordered = intent.clone();
            reordered["target"]["selections"]
                .as_array_mut()
                .expect("checked selections")
                .reverse();
            let bind = request(
                &session,
                "bind-public-intent",
                GameAdapterPayload::BindPublicIntent { intent: reordered },
            );
            if record(&mut session, &mut steps, bind)?.is_some() {
                return Err("ordered-pawn-storm: reordered selection unexpectedly bound".into());
            }
            let valid_bind = request(
                &session,
                "bind-public-intent",
                GameAdapterPayload::BindPublicIntent {
                    intent: intent.clone(),
                },
            );
            if !matches!(
                record(&mut session, &mut steps, valid_bind)?,
                Some(GameAdapterValue::BoundPublicIntent { .. })
            ) {
                return Err(
                    "ordered-pawn-storm: completed ordered UI intent failed to bind".into(),
                );
            }
        }
        let apply = request(
            &session,
            "apply-public-intent",
            GameAdapterPayload::ApplyPublicIntent { intent },
        );
        let old_revision = session.position().position_id().to_owned();
        if !matches!(
            record(&mut session, &mut steps, apply)?,
            Some(GameAdapterValue::AppliedPublicIntent { .. })
        ) {
            return Err(format!("{fixture_id}: required public intent failed to apply").into());
        }
        if old_revision == session.position().position_id() {
            return Err(format!("{fixture_id}: successful apply retained old revision").into());
        }
        let state = session.position().state();
        let semantic_witness = match fixture_id {
            "promotion-choice" => state.board[0][0]
                .as_ref()
                .is_some_and(|piece| piece.kind == "queen"),
            "ordered-pawn-storm" => {
                state.extra["targeting"].is_null()
                    && state.deck_slots.white[0].used
                    && state.board[1][0]
                        .as_ref()
                        .is_some_and(|piece| piece.kind == "pawn")
            }
            "large-piece-move" => {
                let large = state.board[2][1].as_ref();
                (2..4).all(|row| {
                    (1..3).all(|col| {
                        state.board[row][col].as_ref().is_some_and(|piece| {
                            piece.kind == "bigRook"
                                && piece.id == "test-large-rook"
                                && piece.extra["anchorRow"] == 2
                                && piece.extra["anchorCol"] == 1
                                && Some(piece) == large
                        })
                    })
                }) && state.board[4][1].is_none()
                    && state.board[4][2].is_none()
                    && state
                        .board
                        .iter()
                        .flatten()
                        .flatten()
                        .filter(|piece| piece.id == "test-large-rook")
                        .count()
                        == 4
            }
            "private-projection" => state.result() == Some(augment_chess_engine::GameResult::White),
            _ => false,
        };
        if !semantic_witness {
            return Err(format!("{fixture_id}: post-apply semantic witness failed").into());
        }
        for viewer in [Color::White, Color::Black] {
            let observe = request(&session, "observe", GameAdapterPayload::Observe { viewer });
            if !matches!(
                record(&mut session, &mut steps, observe)?,
                Some(GameAdapterValue::Observation { .. })
            ) {
                return Err(format!("{fixture_id}: post-apply observation failed").into());
            }
        }
        cases.push(json!({
            "fixtureId": fixture_id, "config": null, "seed": 19, "steps": steps,
            "finalRevision": session.position().position_id(),
            "finalDecisionActor": session.position().state().decision_actor().as_str(),
            "finalResult": serde_json::to_value(session.position().state().result())?,
            "semanticWitness": true,
        }));
    }
    println!(
        "{}",
        serde_json::to_string(&json!({"schemaVersion": 1, "cases": cases}))?
    );
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
