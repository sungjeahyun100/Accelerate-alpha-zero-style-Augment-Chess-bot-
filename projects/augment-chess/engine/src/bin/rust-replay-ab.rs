//! Rust-only replay cost comparison. No oracle, network, Python or inference.
use augment_chess_engine::replay_experiment::ReplayMode;
use augment_chess_engine::replay_experiment::probe_record_and_delta;
use augment_chess_engine::v7_adapter_actions::{
    apply_admitted, bind_public_intent, legal_public_intents,
};
use augment_chess_engine::{GameConfig, V7HostPosition};
use serde_json::{Value, json};
use std::fs;
use std::hint::black_box;
use std::time::Instant;

struct Options {
    seed: u64,
    iterations: usize,
    simulations: usize,
    rollout_depth: usize,
    actions: Option<String>,
    mode: Option<ReplayMode>,
    output: String,
}

fn options() -> Result<Options, String> {
    let mut options = Options {
        seed: 19,
        iterations: 20,
        simulations: 32,
        rollout_depth: 2,
        actions: None,
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
            "--actions" => options.actions = Some(value),
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
    Ok(options)
}

fn summary(mut times: Vec<u128>) -> Value {
    times.sort_unstable();
    let len = times.len();
    json!({
        "count":len,
        "meanNs":times.iter().sum::<u128>() as f64 / len as f64,
        "medianNs":times[len / 2],
        "p95Ns":times[((len * 95).saturating_sub(1)) / 100],
    })
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

fn choose_intent(position: &V7HostPosition) -> Result<Value, String> {
    legal_public_intents(position)
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| "no legal public actions".into())
}

/// Deterministic root UCB1 search with rule-engine rollouts and no neural network.
/// The reward is terminal outcome, or material balance at the fixed depth.
fn mcts(
    position: &V7HostPosition,
    simulations: usize,
    depth: usize,
) -> Result<(Value, usize), String> {
    let actions = legal_public_intents(position).map_err(|e| e.to_string())?;
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
            let Some(intent) = legal_public_intents(&leaf)
                .map_err(|e| e.to_string())?
                .into_iter()
                .next()
            else {
                break;
            };
            leaf = apply_intent(&leaf, intent)?;
        }
        let material: f64 = leaf
            .state()
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

fn run_mode(
    initial: &V7HostPosition,
    mode: ReplayMode,
    options: &Options,
    sequence: &[Value],
) -> Result<Value, String> {
    let root = initial.with_replay_mode(mode).map_err(|e| e.to_string())?;
    let selected = if sequence.is_empty() {
        choose_intent(&root)?
    } else {
        sequence[0].clone()
    };
    let mut apply_times = Vec::with_capacity(options.iterations);
    let mut enumerate_times = Vec::with_capacity(options.iterations);
    let mut clone_times = Vec::with_capacity(options.iterations);
    let mut apply_elapsed = 0u128;
    for _ in 0..options.iterations {
        let start = Instant::now();
        let state = black_box(root.state().clone());
        clone_times.push(start.elapsed().as_nanos());
        black_box(state);
        let start = Instant::now();
        black_box(legal_public_intents(&root).map_err(|e| e.to_string())?);
        enumerate_times.push(start.elapsed().as_nanos());
        let start = Instant::now();
        black_box(apply_intent(&root, selected.clone())?);
        let ns = start.elapsed().as_nanos();
        apply_times.push(ns);
        apply_elapsed += ns;
    }
    let start = Instant::now();
    let (mcts_choice, completed) = mcts(&root, options.simulations, options.rollout_depth)?;
    let mcts_elapsed = start.elapsed().as_nanos();
    // Diagnostic pass is intentionally outside all timed loops.
    let probe_after = apply_intent(&root, selected.clone())?;
    let replay_probe =
        probe_record_and_delta(root.state(), probe_after.state()).map_err(|e| e.to_string())?;
    let mut position = root;
    let mut sequence_status = Vec::new();
    for (index, intent) in sequence.iter().enumerate() {
        match apply_intent(&position, intent.clone()) {
            Ok(next) => {
                position = next;
                sequence_status.push(json!({"index":index,"ok":true}));
            }
            Err(error) => {
                sequence_status.push(json!({"index":index,"ok":false,"error":error}));
                break;
            }
        }
    }
    let mut state = serde_json::to_value(position.state()).map_err(|e| e.to_string())?;
    let object = state.as_object_mut().expect("state object");
    for key in [
        "replayEvents",
        "replayBaseFrame",
        "replayTailFrame",
        "boardHistory",
        "notationTimeline",
        "replayExperimentEventCount",
        "replayExperimentBoardCount",
        "moveReplay",
        "notationEvent",
        "notationEvents",
        "pendingNotation",
        "pendingNotations",
        "pendingReplayVisuals",
        "history",
        "rng",
    ] {
        object.remove(key);
    }
    Ok(json!({
        "mode":mode,
        "apply":summary(apply_times),
        "legalEnumeration":summary(enumerate_times),
        "gameStateDeepClone":summary(clone_times),
        "replayPhaseProbe":replay_probe,
        "applyActionsPerSecond":options.iterations as f64 * 1e9 / apply_elapsed as f64,
        "mcts":{"simulationsCompleted":completed,"elapsedNs":mcts_elapsed,
            "simulationsPerSecond":completed as f64 * 1e9 / mcts_elapsed as f64,
            "selectedIntent":mcts_choice},
        "maxRssKiBProcessHighWater":max_rss_kib(),
        "sequence":sequence_status,
        "ruleState":state,
        "rng":position.state().rng,
        "legalIntents":legal_public_intents(&position).map_err(|e| e.to_string())?,
        "result":position.state().result(),
    }))
}

fn differences(left: &Value, right: &Value, path: &str, output: &mut Vec<String>) {
    if left == right || output.len() >= 100 {
        return;
    }
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().chain(b.keys()) {
                if a.get(key) != b.get(key) && !output.iter().any(|p| p == &format!("{path}.{key}"))
                {
                    let child = format!("{path}.{key}");
                    match (a.get(key), b.get(key)) {
                        (Some(left), Some(right)) => differences(left, right, &child, output),
                        _ => output.push(child),
                    }
                }
            }
        }
        _ => output.push(path.into()),
    }
}

fn run() -> Result<(), String> {
    let options = options()?;
    let config = GameConfig {
        draft_delete: true,
        ..GameConfig::default()
    };
    let initial =
        V7HostPosition::new_replay_experiment(config, options.seed).map_err(|e| e.to_string())?;
    let sequence: Vec<Value> = if let Some(path) = &options.actions {
        serde_json::from_str(&fs::read_to_string(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
    } else {
        vec![choose_intent(&initial)?]
    };
    let modes = [
        ReplayMode::FullReplay,
        ReplayMode::NoHistoryReplay,
        ReplayMode::NoReplay,
    ];
    let mut results = Vec::new();
    for mode in modes {
        if options.mode.is_none_or(|selected| selected == mode) {
            results.push(match run_mode(&initial, mode, &options, &sequence) {
                Ok(result) => result,
                Err(error) => json!({"mode":mode,"error":error}),
            });
        }
    }
    let baseline = &results[0];
    let comparisons: Vec<Value> = results.iter().skip(1).map(|result| {
        if baseline.get("error").is_some() || result.get("error").is_some() {
            return json!({"mode":result["mode"],"comparisonUnavailable":true,
                "baselineError":baseline.get("error"),"modeError":result.get("error")});
        }
        let mut paths = Vec::new();
        differences(&baseline["ruleState"], &result["ruleState"], "state", &mut paths);
        json!({"mode":result["mode"],"ruleStateDifferentPaths":paths,
            "rngDiffers":baseline["rng"] != result["rng"],
            "legalIntentsDiffer":baseline["legalIntents"] != result["legalIntents"],
            "resultDiffers":baseline["result"] != result["result"],
            "mctsSelectedIntentDiffers":baseline["mcts"]["selectedIntent"] != result["mcts"]["selectedIntent"]})
    }).collect();
    let failures: Vec<String> = results
        .iter()
        .filter_map(|result| {
            result
                .get("error")
                .and_then(Value::as_str)
                .map(|error| format!("{}: {error}", result["mode"]))
        })
        .collect();
    let output = json!({"seed":options.seed,"iterations":options.iterations,
        "mctsSimulations":options.simulations,"rolloutDepth":options.rollout_depth,
        "neuralInference":"disabled","sequence":sequence,"results":results,"comparisons":comparisons,
        "rssDefinition":"process VmHWM after each mode; cumulative, not isolated per mode"});
    let content = serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?;
    if options.output == "-" {
        println!("{content}");
    } else {
        fs::write(&options.output, content).map_err(|e| e.to_string())?;
    }
    if !failures.is_empty() {
        return Err(failures.join("; "));
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("rust-replay-ab: {error}");
        std::process::exit(1);
    }
}
