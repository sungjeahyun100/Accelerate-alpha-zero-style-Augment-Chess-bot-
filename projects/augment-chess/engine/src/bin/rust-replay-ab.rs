//! Rust-only replay cost comparison. No oracle, network, Python or inference.
use augment_chess_engine::replay_experiment::{ReplayMode, probe_record_and_delta};
use augment_chess_engine::v7_adapter_actions::{
    apply_admitted, bind_public_intent, legal_public_intents,
};
use augment_chess_engine::{GameConfig, GameState, V7HostPosition};
use serde_json::{Value, json};
use std::fs;
use std::hint::black_box;
use std::io::Write;
use std::time::Instant;

#[derive(Debug)]
struct Options {
    seed: u64,
    iterations: usize,
    simulations: usize,
    rollout_depth: usize,
    checkpoints: Vec<usize>,
    max_pre_actions: usize,
    actions: Option<String>,
    save_actions: Option<String>,
    progress: Option<String>,
    mode: Option<ReplayMode>,
    output: String,
}

fn options() -> Result<Options, String> {
    let mut options = Options {
        seed: 19,
        iterations: 20,
        simulations: 32,
        rollout_depth: 2,
        checkpoints: vec![0],
        max_pre_actions: 51,
        actions: None,
        save_actions: None,
        progress: None,
        mode: None,
        output: "-".into(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(name) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{name} requires a value"))?;
        match name.as_str() {
            "--seed" => options.seed = value.parse().map_err(|_| "invalid seed")?,
            "--iterations" => {
                options.iterations = value.parse().map_err(|_| "invalid iterations")?
            }
            "--simulations" => {
                options.simulations = value.parse().map_err(|_| "invalid simulations")?
            }
            "--rollout-depth" => {
                options.rollout_depth = value.parse().map_err(|_| "invalid rollout depth")?
            }
            "--max-pre-actions" => {
                options.max_pre_actions = value.parse().map_err(|_| "invalid max pre-actions")?
            }
            "--checkpoints" => {
                options.checkpoints = value
                    .split(',')
                    .map(|part| {
                        part.parse::<usize>()
                            .map_err(|_| format!("invalid checkpoint: {part}"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if options.checkpoints.is_empty() {
                    return Err("checkpoints must not be empty".into());
                }
                options.checkpoints.sort_unstable();
                options.checkpoints.dedup();
            }
            "--actions" => options.actions = Some(value),
            "--save-actions" => options.save_actions = Some(value),
            "--progress" => options.progress = Some(value),
            "--mode" => {
                options.mode = Some(match value.as_str() {
                    "full_replay" => ReplayMode::FullReplay,
                    "no_history_replay" => ReplayMode::NoHistoryReplay,
                    "no_replay" => ReplayMode::NoReplay,
                    _ => {
                        return Err(
                            "mode must be full_replay, no_history_replay or no_replay".into()
                        );
                    }
                })
            }
            "--output" => options.output = value,
            _ => return Err(format!("unknown argument {name}")),
        }
    }
    if options.iterations == 0 || options.simulations == 0 || options.rollout_depth == 0 {
        return Err("iterations, simulations and rollout depth must be positive".into());
    }
    if options.max_pre_actions == 0 && options.checkpoints.iter().any(|&n| n > 0) {
        return Err("max pre-actions must be positive for nonzero checkpoints".into());
    }
    if options.actions.as_deref() == Some(options.output.as_str())
        || options.save_actions.as_deref() == Some(options.output.as_str())
        || (options.actions.is_some() && options.actions == options.save_actions)
        || options.progress.as_deref() == Some(options.output.as_str())
        || (options.progress.is_some()
            && (options.progress == options.actions || options.progress == options.save_actions))
    {
        return Err("actions, save-actions and output paths must differ".into());
    }
    Ok(options)
}

fn summary(mut times: Vec<u128>) -> Value {
    times.sort_unstable();
    let len = times.len();
    json!({"count":len,"meanNs":times.iter().sum::<u128>() as f64 / len as f64,
        "medianNs":times[len / 2],"p95Ns":times[((len * 95).saturating_sub(1)) / 100]})
}

fn max_rss_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse().ok())
    })
}

fn apply_intent(position: &V7HostPosition, intent: Value) -> Result<V7HostPosition, String> {
    let admitted = bind_public_intent(position, intent).map_err(|e| e.to_string())?;
    Ok(apply_admitted(position, &admitted)
        .map_err(|e| e.to_string())?
        .position)
}

fn legal(position: &V7HostPosition) -> Result<Vec<Value>, String> {
    legal_public_intents(position).map_err(|e| e.to_string())
}

/// Prefer actual board moves and rotate through sources/destinations. The public
/// action surface remains the only authority; no state or RNG is adjusted.
fn choose_intent(actions: &[Value], seed: u64, index: usize) -> Option<Value> {
    if index == 0 {
        return actions.first().cloned();
    }
    let moves: Vec<&Value> = actions
        .iter()
        .filter(|a| a.get("type").and_then(Value::as_str) == Some("move"))
        .collect();
    let candidates: Vec<&Value> = if moves.is_empty() {
        actions.iter().collect()
    } else {
        moves
    };
    if candidates.is_empty() {
        return None;
    }
    let offset = seed.wrapping_add((index as u64).wrapping_mul(0x9e3779b97f4a7c15));
    Some((*candidates[(offset as usize) % candidates.len()]).clone())
}

struct Sequence {
    intents: Vec<Value>,
    stop_reason: Option<String>,
}

fn make_sequence(initial: &V7HostPosition, opts: &Options) -> Result<Sequence, String> {
    let requested = opts
        .checkpoints
        .iter()
        .copied()
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let mut position = initial.clone();
    let provided: Option<Vec<Value>> = opts
        .actions
        .as_ref()
        .map(|path| {
            let content =
                fs::read_to_string(path).map_err(|e| format!("read actions {path}: {e}"))?;
            serde_json::from_str(&content).map_err(|e| format!("parse actions {path}: {e}"))
        })
        .transpose()?;
    let mut intents = Vec::new();
    let mut stop_reason = None;
    for index in 0..requested.min(opts.max_pre_actions) {
        if position.state().result().is_some() {
            stop_reason = Some("game_over".into());
            break;
        }
        let choices = legal(&position)?;
        if choices.is_empty() {
            stop_reason = Some("no_legal_actions".into());
            break;
        }
        let intent = if let Some(provided) = &provided {
            let Some(intent) = provided.get(index) else {
                stop_reason = Some("provided_sequence_exhausted".into());
                break;
            };
            intent.clone()
        } else {
            choose_intent(&choices, opts.seed, index).ok_or("no legal public actions")?
        };
        if !choices.contains(&intent) {
            stop_reason = Some(format!("provided_action_not_legal_at_index_{index}"));
            break;
        }
        match apply_intent(&position, intent.clone()) {
            Ok(next) => {
                position = next;
                intents.push(intent);
            }
            Err(error) => {
                stop_reason = Some(format!("generation_apply_failed_at_index_{index}: {error}"));
                break;
            }
        }
    }
    if intents.len() < requested && stop_reason.is_none() {
        stop_reason = Some("max_pre_actions_reached".into());
    }
    Ok(Sequence {
        intents,
        stop_reason,
    })
}

/// Deterministic root UCB1 search with rule-engine rollouts and no neural network.
fn mcts(
    position: &V7HostPosition,
    simulations: usize,
    depth: usize,
) -> Result<(Value, usize), String> {
    let actions = legal(position)?;
    if actions.is_empty() {
        return Err("MCTS root has no legal actions".into());
    }
    let mut visits = vec![0usize; actions.len()];
    let mut values = vec![0.0f64; actions.len()];
    let root_actor = position.state().decision_actor();
    let mut completed = 0;
    for simulation in 0..simulations {
        let index = if simulation < actions.len() {
            simulation
        } else {
            (0..actions.len())
                .max_by(|&a, &b| {
                    let score = |i: usize| {
                        values[i] / visits[i] as f64
                            + (2.0 * (simulation as f64).ln() / visits[i] as f64).sqrt()
                    };
                    score(a).total_cmp(&score(b)).then_with(|| b.cmp(&a))
                })
                .expect("nonempty actions")
        };
        let mut leaf = apply_intent(position, actions[index].clone())?;
        for _ in 1..depth {
            if leaf.state().result().is_some() {
                break;
            }
            let Some(intent) = legal(&leaf)?.into_iter().next() else {
                break;
            };
            leaf = apply_intent(&leaf, intent)?;
        }
        let material: f64 =
            leaf.state()
                .board
                .iter()
                .flatten()
                .flatten()
                .fold(0.0, |score, piece| {
                    let value = match piece.kind.as_str() {
                        "pawn" => 1.0,
                        "queen" => 9.0,
                        "rook" => 5.0,
                        "bishop" | "knight" => 3.0,
                        _ => 0.0,
                    };
                    if piece.color.owner() == Some(root_actor) {
                        score + value
                    } else {
                        score - value
                    }
                });
        visits[index] += 1;
        values[index] += material.tanh();
        completed += 1;
    }
    let selected = (0..actions.len())
        .max_by(|&a, &b| {
            visits[a]
                .cmp(&visits[b])
                .then_with(|| values[a].total_cmp(&values[b]))
                .then_with(|| b.cmp(&a))
        })
        .expect("nonempty actions");
    Ok((actions[selected].clone(), completed))
}

fn serialized_len(value: &Value) -> Result<usize, String> {
    serde_json::to_vec(value)
        .map(|v| v.len())
        .map_err(|e| e.to_string())
}

fn state_metrics(position: &V7HostPosition, actions: usize) -> Result<Value, String> {
    let state = position.state();
    let full = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let events = state.extra.get("replayEvents").unwrap_or(&Value::Null);
    let move_replay = state.extra.get("moveReplay").unwrap_or(&Value::Null);
    let event_count = if state.replay_mode == ReplayMode::FullReplay {
        events.as_array().map_or(0, Vec::len)
    } else {
        state
            .extra
            .get("replayExperimentEventCount")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize
    };
    Ok(json!({
        "actualActions": actions, "phase":state.mode, "turn":state.turn,
        "decisionActor":state.decision_actor(), "moveCount":state.move_count,
        "fullMove":state.full_move, "turnsTaken":state.turns_taken,
        "cardsUsedThisTurn":state.cards_used_this_turn,
        "replayEventCount":event_count,
        "replayEventsJsonBytes":serialized_len(events)?,
        "gameStateJsonBytes":serialized_len(&full)?,
        "moveReplayJsonBytes":serialized_len(move_replay)?,
        "replayEventNonce":state.extra.get("replayEventNonce"),
    }))
}

fn benchmark(
    root: &V7HostPosition,
    selected: Option<&Value>,
    opts: &Options,
    actions: usize,
) -> Value {
    let metrics = match state_metrics(root, actions) {
        Ok(v) => v,
        Err(e) => return json!({"error":e}),
    };
    let mut clone_times = Vec::with_capacity(opts.iterations);
    let mut enumerate_times = Vec::with_capacity(opts.iterations);
    let mut apply_times = Vec::with_capacity(opts.iterations);
    let before = serde_json::to_value(root.state()).ok();
    for _ in 0..opts.iterations {
        let start = Instant::now();
        black_box(root.state().clone());
        clone_times.push(start.elapsed().as_nanos());
        let start = Instant::now();
        match legal(root) {
            Ok(v) => {
                black_box(v);
                enumerate_times.push(start.elapsed().as_nanos());
            }
            Err(error) => {
                return json!({"metrics":metrics,"error":format!("legal enumeration: {error}")});
            }
        }
        if let Some(intent) = selected {
            let start = Instant::now();
            match apply_intent(root, intent.clone()) {
                Ok(v) => {
                    black_box(v);
                    apply_times.push(start.elapsed().as_nanos());
                }
                Err(error) => return json!({"metrics":metrics,"error":format!("apply: {error}")}),
            }
        }
    }
    let mut output = json!({"metrics":metrics,"gameStateDeepClone":summary(clone_times),
        "legalEnumeration":summary(enumerate_times),
        "apply":if apply_times.is_empty() { Value::Null } else { summary(apply_times) }});
    if let Some(intent) = selected {
        output["appliedIntent"] = intent.clone();
        if let Ok(after) = apply_intent(root, intent.clone()) {
            output["replayPhaseProbe"] = probe_record_and_delta(root.state(), after.state())
                .map(|v| v)
                .unwrap_or_else(|e| json!({"error":e.to_string()}));
        }
    } else {
        output["applyUnavailableReason"] = json!("no_common_next_action");
    }
    let start = Instant::now();
    output["mcts"] = match mcts(root, opts.simulations, opts.rollout_depth) {
        Ok((choice, completed)) => {
            let elapsed = start.elapsed().as_nanos();
            json!({"simulationsRequested":opts.simulations,"rolloutDepth":opts.rollout_depth,
                "simulationsCompleted":completed,"elapsedNs":elapsed,
                "simulationsPerSecond":completed as f64 * 1e9 / elapsed.max(1) as f64,
                "selectedIntent":choice,"failed":false,
                "internalReplayRecords":"not_instrumented"})
        }
        Err(error) => {
            json!({"simulationsRequested":opts.simulations,"rolloutDepth":opts.rollout_depth,
            "simulationsCompleted":0,"elapsedNs":start.elapsed().as_nanos(),
            "selectedIntent":null,"failed":true,"error":error,
            "internalReplayRecords":"not_instrumented"})
        }
    };
    output["maxRssKiBProcessHighWater"] = json!(max_rss_kib());
    output["rootStateUnchanged"] = json!(before == serde_json::to_value(root.state()).ok());
    output
}

fn semantic_state(state: &GameState) -> Result<Value, String> {
    let mut value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let object = value.as_object_mut().ok_or("state must be object")?;
    for key in [
        "replayEvents",
        "replayBaseFrame",
        "replayTailFrame",
        "boardHistory",
        "notationTimeline",
        "replayExperimentEventCount",
        "replayExperimentBoardCount",
        "notationEvent",
        "notationEvents",
        "pendingNotation",
        "pendingNotations",
        "pendingReplayVisuals",
        "history",
        "replayEventNonce",
        "rng",
    ] {
        object.remove(key);
    }
    Ok(value)
}

fn first_difference(left: &Value, right: &Value, path: &str) -> Option<Value> {
    if left == right {
        return None;
    }
    if let (Some(a), Some(b)) = (left.as_object(), right.as_object()) {
        let mut keys: Vec<_> = a.keys().chain(b.keys()).collect();
        keys.sort();
        keys.dedup();
        for key in keys {
            if a.get(key) != b.get(key) {
                let child = format!("{path}.{key}");
                return match (a.get(key), b.get(key)) {
                    (Some(l), Some(r)) => first_difference(l, r, &child),
                    _ => Some(json!({"path":child,"fullReplay":a.get(key),"mode":b.get(key)})),
                };
            }
        }
    }
    Some(json!({"path":path,"fullReplay":left,"mode":right}))
}

fn compare_states(
    full: &V7HostPosition,
    other: &V7HostPosition,
    mode: ReplayMode,
    index: usize,
    intent: &Value,
) -> Result<Option<Value>, String> {
    let a = semantic_state(full.state())?;
    let b = semantic_state(other.state())?;
    let semantic = first_difference(&a, &b, "state");
    let rng = first_difference(&json!(full.state().rng), &json!(other.state().rng), "rng");
    let metadata_a = json!({"replayEventCount":state_metrics(full, index)?["replayEventCount"],
        "replayEventNonce":full.state().extra.get("replayEventNonce")});
    let metadata_b = json!({"replayEventCount":state_metrics(other, index)?["replayEventCount"],
        "replayEventNonce":other.state().extra.get("replayEventNonce")});
    let metadata = first_difference(&metadata_a, &metadata_b, "replayMetadata");
    // Legal enumeration is diagnostic only and outside benchmark timing.
    let legal_a = legal(full);
    let legal_b = legal(other);
    let legal_diff = match (legal_a, legal_b) {
        (Ok(a), Ok(b)) => first_difference(&json!(a), &json!(b), "legalIntents"),
        (Err(a), Err(b)) if a == b => None,
        (a, b) => Some(
            json!({"path":"legalIntents","fullReplay":format!("{a:?}"),"mode":format!("{b:?}")}),
        ),
    };
    let result_diff = first_difference(
        &json!(full.state().result()),
        &json!(other.state().result()),
        "result",
    );
    let category = if semantic.is_some() || legal_diff.is_some() || result_diff.is_some() {
        "semantic"
    } else if rng.is_some() {
        "rng"
    } else if metadata.is_some() {
        "replay_metadata"
    } else {
        return Ok(None);
    };
    let detail = semantic
        .or(legal_diff)
        .or(result_diff)
        .or(rng)
        .or(metadata)
        .expect("difference present");
    Ok(Some(
        json!({"mode":mode,"actionIndex":index,"intent":intent,"category":category,
        "path":detail["path"],"fullReplayValue":detail["fullReplay"],"modeValue":detail["mode"],
        "comparable":category == "replay_metadata"}),
    ))
}

fn run_mode(
    initial: &V7HostPosition,
    mode: ReplayMode,
    opts: &Options,
    sequence: &[Value],
    sequence_stop_reason: Option<&str>,
) -> Result<Value, String> {
    let prep_start = Instant::now();
    let mut current = initial.with_replay_mode(mode).map_err(|e| e.to_string())?;
    let mut prep_ns = prep_start.elapsed().as_nanos();
    let mut baseline = initial.clone();
    let mut completed = 0usize;
    let mut first_divergence = None;
    let mut first_rule_divergence = None;
    let mut checkpoints = Vec::new();
    let mut failure = None;
    for &checkpoint in &opts.checkpoints {
        while completed < checkpoint && completed < sequence.len() && failure.is_none() {
            let step_start = Instant::now();
            let intent = &sequence[completed];
            let next_baseline = if mode == ReplayMode::FullReplay {
                None
            } else {
                Some(apply_intent(&baseline, intent.clone()))
            };
            match apply_intent(&current, intent.clone()) {
                Ok(next) => {
                    current = next;
                    completed += 1;
                }
                Err(error) => {
                    failure = Some(format!("mode action {}: {error}", completed));
                    prep_ns += step_start.elapsed().as_nanos();
                    break;
                }
            }
            if let Some(next) = next_baseline {
                match next {
                    Ok(next) => {
                        baseline = next;
                        if first_rule_divergence.is_none() {
                            if let Some(difference) =
                                compare_states(&baseline, &current, mode, completed, intent)?
                            {
                                if first_divergence.is_none() {
                                    first_divergence = Some(difference.clone());
                                }
                                if difference["category"] != "replay_metadata" {
                                    first_rule_divergence = Some(difference);
                                }
                            }
                        }
                    }
                    Err(error) => {
                        failure = Some(format!("full replay action {}: {error}", completed - 1));
                        prep_ns += step_start.elapsed().as_nanos();
                        break;
                    }
                }
            }
            prep_ns += step_start.elapsed().as_nanos();
        }
        let reached = completed == checkpoint && failure.is_none();
        let comparable = first_rule_divergence.is_none();
        let selected = if reached {
            sequence.get(checkpoint)
        } else {
            None
        };
        let measurement = if reached {
            let permitted =
                selected.map(|intent| legal(&current).map(|choices| choices.contains(intent)));
            match permitted {
                Some(Ok(false)) => {
                    let mut data = benchmark(&current, None, opts, completed);
                    data["comparisonUnavailable"] = json!("common_next_action_not_legal");
                    data
                }
                Some(Err(error)) => {
                    let mut data = benchmark(&current, None, opts, completed);
                    data["comparisonUnavailable"] = json!(format!("legal check failed: {error}"));
                    data
                }
                _ => benchmark(&current, selected, opts, completed),
            }
        } else {
            Value::Null
        };
        let consumed = &sequence[..completed];
        let move_actions = consumed.iter().filter(|v| v["type"] == "move").count();
        let card_actions = consumed.iter().filter(|v| v["type"] == "card").count();
        let checkpoint_result = json!({"checkpoint":checkpoint,"reached":reached,
            "actualActions":completed,"publicMoveActions":move_actions,
            "publicCardActions":card_actions,
            "publicOtherActions":completed - move_actions - card_actions,
            "phase":current.state().mode,
            "preparationElapsedNs":prep_ns,"measurement":measurement,
            "comparisonStatus":if comparable { "comparable" } else { "diagnostic_only" },
            "firstDivergence":&first_divergence,"firstRuleDivergence":&first_rule_divergence,
            "reason":if reached { None } else { failure.as_deref().or(sequence_stop_reason).or(Some("sequence_exhausted")) }});
        if let Some(path) = &opts.progress {
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(path)
                .map_err(|e| format!("open progress {path}: {e}"))?;
            writeln!(
                file,
                "{}",
                json!({"mode":mode,"checkpointResult":checkpoint_result})
            )
            .map_err(|e| format!("write progress {path}: {e}"))?;
            file.sync_data()
                .map_err(|e| format!("sync progress {path}: {e}"))?;
        }
        checkpoints.push(checkpoint_result);
    }
    Ok(
        json!({"mode":mode,"actionsConsumed":completed,"checkpoints":checkpoints,
        "firstDivergence":first_divergence,"firstRuleDivergence":first_rule_divergence,"error":failure}),
    )
}

fn write_output(path: &str, output: &Value) -> Result<(), String> {
    let content = serde_json::to_string_pretty(output).map_err(|e| e.to_string())?;
    if path == "-" {
        println!("{content}");
        Ok(())
    } else {
        fs::write(path, content).map_err(|e| format!("write output {path}: {e}"))
    }
}

fn run() -> Result<(), String> {
    let opts = options()?;
    let initial = V7HostPosition::new_replay_experiment(
        GameConfig {
            draft_delete: true,
            ..GameConfig::default()
        },
        opts.seed,
    )
    .map_err(|e| e.to_string())?;
    let generation_start = Instant::now();
    let sequence = make_sequence(&initial, &opts)?;
    let generation_ns = generation_start.elapsed().as_nanos();
    if let Some(path) = &opts.save_actions {
        fs::write(
            path,
            serde_json::to_string_pretty(&sequence.intents).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("save actions {path}: {e}"))?;
    }
    if let Some(path) = &opts.progress {
        fs::write(
            path,
            format!(
                "{}\n",
                json!({"schemaVersion":2,"seed":opts.seed,
            "sequence":&sequence.intents,"checkpointsRequested":&opts.checkpoints,
            "iterations":opts.iterations,"mctsSimulations":opts.simulations,
            "rolloutDepth":opts.rollout_depth})
            ),
        )
        .map_err(|e| format!("create progress {path}: {e}"))?;
    }
    let modes = [
        ReplayMode::FullReplay,
        ReplayMode::NoHistoryReplay,
        ReplayMode::NoReplay,
    ];
    let mut results = Vec::new();
    for mode in modes {
        if opts.mode.is_none_or(|selected| selected == mode) {
            results.push(
                match run_mode(
                    &initial,
                    mode,
                    &opts,
                    &sequence.intents,
                    sequence.stop_reason.as_deref(),
                ) {
                    Ok(v) => v,
                    Err(error) => json!({"mode":mode,"error":error}),
                },
            );
        }
    }
    let output = json!({
        "schemaVersion":2,"programVersion":env!("CARGO_PKG_VERSION"),
        "seed":opts.seed,"iterations":opts.iterations,"mctsSimulations":opts.simulations,
        "rolloutDepth":opts.rollout_depth,"checkpointsRequested":&opts.checkpoints,
        "maxPreActions":opts.max_pre_actions,"neuralInference":"disabled",
        "sequenceSource":if opts.actions.is_some() { "provided" } else { "generated" },
        "sequence":&sequence.intents,"sequenceGenerationElapsedNs":generation_ns,
        "sequenceStopReason":&sequence.stop_reason,"results":&results,
        "rssDefinition":"Linux process VmHWM at each checkpoint; cumulative within one process. Use --mode in separate processes for isolated peaks",
        "units":{"duration":"ns","serializedSize":"UTF-8 JSON bytes","rss":"KiB"},
    });
    write_output(&opts.output, &output)?;
    if results
        .iter()
        .any(|v| v.get("error").is_some_and(|e| !e.is_null()))
    {
        return Err("one or more modes failed; partial output was saved".into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("rust-replay-ab: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn initial() -> V7HostPosition {
        V7HostPosition::new_replay_experiment(
            GameConfig {
                draft_delete: true,
                ..GameConfig::default()
            },
            19,
        )
        .unwrap()
    }
    fn opts() -> Options {
        Options {
            seed: 19,
            iterations: 1,
            simulations: 1,
            rollout_depth: 1,
            checkpoints: vec![0, 10],
            max_pre_actions: 10,
            actions: None,
            save_actions: None,
            progress: None,
            mode: None,
            output: "-".into(),
        }
    }
    #[test]
    fn generated_sequence_is_repeatable_and_reaches_ten_actions() {
        let root = initial();
        let a = make_sequence(&root, &opts()).unwrap();
        let b = make_sequence(&root, &opts()).unwrap();
        assert_eq!(a.intents, b.intents);
        assert_eq!(a.intents[0], legal(&root).unwrap()[0]);
        assert_eq!(a.intents.len(), 10, "{:?}", a.stop_reason);
    }
    #[test]
    fn checkpoint_zero_uses_initial_play_position() {
        let root = initial();
        let mode = run_mode(
            &root,
            ReplayMode::FullReplay,
            &Options {
                checkpoints: vec![0],
                ..opts()
            },
            &[],
            Some("provided_sequence_exhausted"),
        )
        .unwrap();
        assert_eq!(mode["checkpoints"][0]["actualActions"], 0);
        assert_eq!(mode["checkpoints"][0]["phase"], root.state().mode);
    }
    #[test]
    fn modes_consume_exact_common_sequence_and_report_sizes() {
        let root = initial();
        let sequence = make_sequence(&root, &opts()).unwrap();
        for mode in [
            ReplayMode::FullReplay,
            ReplayMode::NoHistoryReplay,
            ReplayMode::NoReplay,
        ] {
            let result = run_mode(
                &root,
                mode,
                &opts(),
                &sequence.intents,
                sequence.stop_reason.as_deref(),
            )
            .unwrap();
            assert_eq!(result["actionsConsumed"], 10);
            assert_eq!(
                result["checkpoints"][1]["measurement"]["metrics"]["actualActions"],
                10
            );
            assert!(
                result["checkpoints"][1]["measurement"]["metrics"]["gameStateJsonBytes"]
                    .is_number()
            );
            assert!(
                result["checkpoints"][1]["measurement"]["metrics"]["replayEventCount"].is_number()
            );
        }
    }
    #[test]
    fn semantic_difference_preserves_move_replay() {
        let root = initial();
        let baseline = semantic_state(root.state()).unwrap();
        let mut changed = baseline.clone();
        changed["moveReplay"] = json!({"white":"changed","black":null});
        assert_eq!(
            first_difference(&baseline, &changed, "state").unwrap()["path"],
            "state.moveReplay"
        );
    }
    #[test]
    fn unreachable_checkpoint_is_explicit() {
        let root = initial();
        let result = run_mode(
            &root,
            ReplayMode::FullReplay,
            &Options {
                checkpoints: vec![0, 10],
                ..opts()
            },
            &[],
            Some("provided_sequence_exhausted"),
        )
        .unwrap();
        assert_eq!(result["checkpoints"][1]["reached"], false);
        assert_eq!(result["checkpoints"][1]["actualActions"], 0);
    }
}
