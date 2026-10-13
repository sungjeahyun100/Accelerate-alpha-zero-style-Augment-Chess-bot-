//! Eight user-run legal-action scenarios. Setup, reference comparison and
//! serialization are outside the timed complete-enumeration interval.
use crate::{
    Action, ActionKind, CardSlot, EngineError, GameConfig, GameState, RULES_VERSION_V7, Result,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, time::Instant};

struct Options {
    scenarios: Vec<u8>,
    modes: Vec<&'static str>,
    seed: u64,
    warmup: usize,
    repeats: usize,
    oracle: Option<Value>,
    output: Option<String>,
    cpu_mode: String,
    threads: String,
    profile: String,
}

pub fn run_cli(args: impl Iterator<Item = String>) -> std::result::Result<(), String> {
    let mut options = Options {
        scenarios: (1..=8).collect(),
        modes: vec!["normal"],
        seed: 19,
        warmup: 3,
        repeats: 20,
        oracle: None,
        output: None,
        cpu_mode: "unspecified".into(),
        threads: "unspecified".into(),
        profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
    };
    let mut args = args;
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag.as_str() {
            "--scenario" => {
                options.scenarios = if value == "all" {
                    (1..=8).collect()
                } else {
                    let n: u8 = value.parse().map_err(|_| "scenario must be 1..8 or all")?;
                    if !(1..=8).contains(&n) {
                        return Err("scenario must be 1..8 or all".into());
                    }
                    vec![n]
                }
            }
            "--mode" => {
                options.modes = match value.as_str() {
                    "normal" => vec!["normal"],
                    "chaos" => vec!["chaos"],
                    "grand" => vec!["grand"],
                    "all" => vec!["normal", "chaos", "grand"],
                    _ => return Err("mode must be normal, chaos, grand or all".into()),
                }
            }
            "--seed" => options.seed = value.parse().map_err(|_| "seed must be u64")?,
            "--warmup" => {
                options.warmup = value.parse().map_err(|_| "warmup must be nonnegative")?
            }
            "--repeats" => {
                options.repeats = value.parse().map_err(|_| "repeats must be positive")?
            }
            "--oracle" => {
                let bytes = fs::read(&value).map_err(|e| format!("oracle read {value}: {e}"))?;
                options.oracle = Some(
                    serde_json::from_slice(&bytes)
                        .map_err(|e| format!("oracle JSON {value}: {e}"))?,
                );
            }
            "--output" => options.output = Some(value),
            "--cpu-mode" => options.cpu_mode = value,
            "--threads" => options.threads = value,
            "--profile" => options.profile = value,
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    if options.repeats == 0 || options.repeats > 1_000_000 || options.warmup > 1_000_000 {
        return Err("warmup/repeats must be within 0..=1000000, with repeats positive".into());
    }
    let mut rows = Vec::new();
    for mode in &options.modes {
        for scenario in &options.scenarios {
            for (variant, state) in scenario_states(*scenario, mode, options.seed) {
                rows.push(measure(&options, *scenario, mode, &variant, state));
            }
        }
    }
    let document = json!({"schemaVersion":1,"unit":"ns","rows":rows});
    let bytes = serde_json::to_vec_pretty(&document).map_err(|e| e.to_string())?;
    if let Some(path) = &options.output {
        fs::write(path, bytes).map_err(|e| format!("output write {path}: {e}"))?;
    } else {
        println!("{}", String::from_utf8(bytes).map_err(|e| e.to_string())?);
    }
    Ok(())
}

fn initial(mode: &str, seed: u64, rule: Option<&str>) -> Result<GameState> {
    let config = GameConfig {
        game_style: mode.into(),
        draft_delete: true,
        rule_card_ids: rule.into_iter().map(str::to_owned).collect(),
        ..GameConfig::default()
    };
    crate::draft::initialize_for_ruleset(config, seed, RULES_VERSION_V7)
}

fn add_card(state: &mut GameState, id: &str) -> Result<usize> {
    let definition = crate::card_registry::definition_for(RULES_VERSION_V7, id)?;
    let mut card: CardSlot = serde_json::from_value(definition.source_definition.clone())
        .map_err(EngineError::serialization)?;
    card.instance_id = format!("experiment-{id}");
    card.used = false;
    card.recovering = false;
    card.vacant = false;
    let deck = state.deck_slots.get_mut(state.turn);
    let index = deck
        .iter()
        .position(|card| card.vacant)
        .ok_or_else(|| EngineError::InvalidState("no vacant card slot".into()))?;
    deck[index] = card;
    Ok(index)
}

fn scramble(mut state: GameState, seed: u64) -> Result<GameState> {
    // Every step is selected from the engine's complete legal action stream.
    // A fixed seed and turn index make the resulting position reproducible.
    for ply in 0..20_u64 {
        let actions: Vec<Action> = crate::v7_action_surface::legal_source_actions(&state)?
            .into_iter()
            .filter(|action| action.kind == ActionKind::Move)
            .collect();
        if actions.is_empty() {
            return Err(EngineError::UnsupportedFeature(format!(
                "scramble has no movement at ply {ply}"
            )));
        }
        let index = ((seed.wrapping_mul(1664525).wrapping_add(ply * 1013904223))
            % actions.len() as u64) as usize;
        crate::transition::apply(&mut state, &actions[index])?;
        crate::replay::canonicalize_position_frames(&mut state)?;
        if state.result().is_some() {
            return Err(EngineError::UnsupportedFeature(format!(
                "scramble ended at ply {ply}"
            )));
        }
    }
    Ok(state)
}

fn scenario_states(number: u8, mode: &str, seed: u64) -> Vec<(String, Result<GameState>)> {
    let base = || initial(mode, seed, None);
    match number {
        1 => vec![("baseline".into(), base())],
        2 => vec![(
            "opening-holdout-unused".into(),
            base().and_then(|mut state| {
                add_card(&mut state, "holdout")?;
                Ok(state)
            }),
        )],
        3 => vec![(
            "opening-holdout-applied".into(),
            base().and_then(|mut state| {
                add_card(&mut state, "holdout")?;
                let action = crate::v7_action_surface::legal_source_actions(&state)?
                    .into_iter()
                    .find(|action| {
                        action.kind == ActionKind::Card
                            && action.card_id.as_deref() == Some("holdout")
                    })
                    .ok_or_else(|| {
                        EngineError::UnsupportedFeature(
                            "holdout has no legal first-play target".into(),
                        )
                    })?;
                crate::transition::apply(&mut state, &action)?;
                crate::replay::canonicalize_position_frames(&mut state)?;
                Ok(state)
            }),
        )],
        4 => vec![(
            "diagonal-chess-applied".into(),
            initial(mode, seed, Some("diagonal-chess")),
        )],
        5 => {
            let middle = base().and_then(|state| scramble(state, seed));
            vec![
                ("middle-absent".into(), middle.clone()),
                (
                    "middle-e4-unused".into(),
                    middle.and_then(|mut state| {
                        if mode == "grand"
                            || state.turns_taken.white.min(state.turns_taken.black) < 10
                        {
                            return Err(EngineError::UnsupportedFeature(
                                "MIDDLE stage unavailable in this position and mode".into(),
                            ));
                        }
                        add_card(&mut state, "e4")?;
                        Ok(state)
                    }),
                ),
            ]
        }
        6 => vec![(
            "scrambled-no-card".into(),
            base().and_then(|state| scramble(state, seed)),
        )],
        7 | 8 => {
            let scrambled = base().and_then(|state| scramble(state, seed));
            let activation = if number == 7 {
                crate::card_registry::CardActType::Active
            } else {
                crate::card_registry::CardActType::Passive
            };
            let registry = match crate::card_registry::registry_for(RULES_VERSION_V7) {
                Ok(registry) => registry,
                Err(error) => return vec![("catalog".into(), Err(error))],
            };
            registry
                .cards
                .values()
                .filter(|definition| {
                    definition.activation == Some(activation)
                        && matches!(
                            definition.card_type,
                            Some(
                                crate::card_registry::CardType::Opening
                                    | crate::card_registry::CardType::Middle
                                    | crate::card_registry::CardType::End
                                    | crate::card_registry::CardType::Piece
                            )
                        )
                })
                .map(|definition| {
                    let id = definition.id.clone();
                    let state = scrambled.clone().and_then(|mut state| {
                        let slot = add_card(&mut state, &id)?;
                        if number == 8 {
                            let color = state.turn;
                            if !crate::draft::apply_draft_passive_with_outcome(
                                &mut state, color, slot,
                            )? {
                                return Err(EngineError::UnsupportedFeature(format!(
                                    "PASSIVE {id} acquisition was declined in this position"
                                )));
                            }
                        }
                        Ok(state)
                    });
                    (id, state)
                })
                .collect()
        }
        _ => unreachable!(),
    }
}

fn semantic_ids(actions: &[Action]) -> Result<BTreeSet<String>> {
    actions
        .iter()
        .map(|action| {
            let value = json!({"type":action.kind,"color":action.color,"from":action.from,
            "move":action.destination,"cardId":action.card_id,
            "target":action.target,"extra":action.extra});
            canonical_id(&value)
        })
        .collect()
}

fn canonical_id(value: &Value) -> Result<String> {
    String::from_utf8(serde_jcs::to_vec(value).map_err(EngineError::serialization)?)
        .map_err(|error| EngineError::InvalidState(format!("semantic ID is not UTF-8: {error}")))
}

fn error_status(error: &EngineError) -> &'static str {
    match error {
        EngineError::UnsupportedFeature(_) => "UNSUPPORTED",
        _ => "FAIL",
    }
}

fn measure(
    options: &Options,
    number: u8,
    mode: &str,
    variant: &str,
    state: Result<GameState>,
) -> Value {
    let mut row = json!({"scenario":number,"rulesVersion":RULES_VERSION_V7,"mode":mode,
        "positionId":format!("s{number}-{mode}-{variant}-seed{}", options.seed),"seed":options.seed,
        "ruleCardIds": if number == 4 { vec!["diagonal-chess"] } else { vec![] },
        "cardId": if matches!(number, 2 | 3) { Value::String("holdout".into()) } else if number == 5 && variant.contains("e4") { Value::String("e4".into()) } else if matches!(number, 7 | 8) { Value::String(variant.into()) } else { Value::Null },
        "cardDisposition": if matches!(number, 3 | 8) { "applied" } else if number == 2 || number == 7 || number == 5 && variant.contains("e4") { "held" } else { "none" },
        "warmup":options.warmup,"repeats":options.repeats,"cpuMode":options.cpu_mode,
        "threads":options.threads,"buildProfile":options.profile,
        "status":"UNSUPPORTED","reason":Value::Null,"rawNs":[],"legalCount":Value::Null,
        "moveCount":Value::Null,"cardActionCount":Value::Null});
    let state = match state {
        Ok(state) => state,
        Err(error) => {
            row["status"] = json!(error_status(&error));
            row["reason"] = json!(error.to_string());
            return row;
        }
    };
    row["positionHash"] = match serde_jcs::to_vec(&state) {
        Ok(bytes) => json!(format!("{:016x}", crate::stable_hash(&bytes))),
        Err(error) => {
            row["reason"] = json!(error.to_string());
            return row;
        }
    };
    for _ in 0..options.warmup {
        if let Err(error) = crate::v7_action_surface::legal_source_actions(&state) {
            row["status"] = json!(error_status(&error));
            row["reason"] = json!(error.to_string());
            return row;
        }
    }
    let mut raw = Vec::with_capacity(options.repeats);
    let mut actions = Vec::new();
    for _ in 0..options.repeats {
        let start = Instant::now();
        let outcome = crate::v7_action_surface::legal_source_actions(&state);
        let elapsed = start.elapsed().as_nanos();
        match outcome {
            Ok(found) => {
                raw.push(elapsed);
                actions = found;
            }
            Err(error) => {
                row["status"] = json!(error_status(&error));
                row["reason"] = json!(error.to_string());
                return row;
            }
        }
    }
    row["rawNs"] = json!(raw);
    let mut sorted = raw.clone();
    sorted.sort_unstable();
    let median = if sorted.len() % 2 == 0 {
        (sorted[sorted.len() / 2 - 1] as f64 + sorted[sorted.len() / 2] as f64) / 2.0
    } else {
        sorted[sorted.len() / 2] as f64
    };
    row["meanNs"] = json!(raw.iter().map(|ns| *ns as f64).sum::<f64>() / raw.len() as f64);
    row["medianNs"] = json!(median);
    row["p95Ns"] = json!(sorted[(sorted.len() * 95).div_ceil(100) - 1]);
    row["legalCount"] = json!(actions.len());
    row["moveCount"] = json!(
        actions
            .iter()
            .filter(|action| action.kind == ActionKind::Move)
            .count()
    );
    row["cardActionCount"] = json!(
        actions
            .iter()
            .filter(|action| action.kind == ActionKind::Card)
            .count()
    );
    if number == 7 && row["cardActionCount"] == 0 {
        row["reason"] = json!("card has no legal action in this position");
        return row;
    }
    let ids = match semantic_ids(&actions) {
        Ok(ids) => ids,
        Err(error) => {
            row["reason"] = json!(error.to_string());
            return row;
        }
    };
    row["semanticIds"] = json!(ids);
    if ids.len() != actions.len() {
        row["status"] = json!("FAIL");
        row["reason"] = json!("duplicate semantic action identifiers");
        return row;
    }
    let oracle_key = format!("s{number}-{mode}-{variant}");
    let reference = options
        .oracle
        .as_ref()
        .and_then(|oracle| oracle.get(&oracle_key))
        .and_then(Value::as_array);
    match reference {
        None => {
            row["status"] = json!("UNVERIFIED");
            row["reason"] = json!("independent semantic-action reference is absent");
        }
        Some(reference) => {
            let expected: BTreeSet<String> = reference
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|text| serde_json::from_str::<Value>(text).ok())
                .filter_map(|value| canonical_id(&value).ok())
                .collect();
            if expected.len() != reference.len() {
                row["status"] = json!("FAIL");
                row["reason"] = json!("oracle must contain unique valid semantic-ID JSON strings");
                return row;
            }
            let missing: Vec<_> = expected.difference(&ids).cloned().collect();
            let extra: Vec<_> = ids.difference(&expected).cloned().collect();
            row["missing"] = json!(missing);
            row["extra"] = json!(extra);
            row["status"] = json!(if missing.is_empty() && extra.is_empty() {
                "PASS"
            } else {
                "FAIL"
            });
            if !missing.is_empty() || !extra.is_empty() {
                row["reason"] = json!("semantic action set differs from independent reference");
            }
        }
    }
    row
}
