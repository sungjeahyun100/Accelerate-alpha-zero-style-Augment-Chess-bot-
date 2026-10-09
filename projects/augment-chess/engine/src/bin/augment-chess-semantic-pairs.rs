//! Explicit offline paired capture from pinned JS cases and the Rust v7 host.
//! This binary never generates oracle rules, samples a distribution, or marks parity PASS.
use augment_chess_engine::{Color, EngineError, GameResult, V7HostPosition, v7_adapter_actions};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

const SOURCE_SHA256: &str = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
const PROFILE: &str = "accelerate-headless-semantic-v7-faithful-init-v1";
const MAX_SOURCE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_CASES: usize = 10_000;
const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

fn observations(host: &V7HostPosition) -> Result<Value, String> {
    let state = host.state();
    Ok(json!({
        "white": state.try_observe(Color::White).map_err(|error| error.to_string())?,
        "black": state.try_observe(Color::Black).map_err(|error| error.to_string())?,
    }))
}

fn rule_result(host: &V7HostPosition) -> Value {
    let state = host.state();
    let outcome = state.result();
    let winner = if matches!(&outcome, Some(GameResult::White | GameResult::Black)) {
        json!(state.winner)
    } else {
        Value::Null
    };
    json!({
        "protocolVersion": "accelerate-result-v1",
        "status": if outcome.is_some() { "terminal" } else { "ongoing" },
        "winner": winner,
        "outcome": outcome,
    })
}

fn replay_sequence(start: &V7HostPosition, source: &Value) -> Result<Value, String> {
    let source_unavailable = source["status"] == "unavailable";
    if source["status"] != "complete" && !source_unavailable {
        return Ok(json!({"status": "unsupported", "reason": source["reason"]}));
    }
    let source_steps = source["steps"].as_array().ok_or("source Replay steps missing")?;
    let actor = source["actor"].as_str().ok_or("source Replay actor missing")?;
    if actor != "white" && actor != "black" {
        return Err("source Replay actor invalid".into());
    }
    if (source_unavailable && (source_steps.len() > 2
        || source_steps.iter().any(|step| step["kind"] != "bridge")))
        || (!source_unavailable && (source_steps.len() < 2 || source_steps.len() > 4
            || source_steps.iter().filter(|step| step["kind"] == "replay").count() != 1
            || source_steps.last().and_then(|step| step["kind"].as_str()) != Some("follow")))
    {
        return Err("source Replay sequence lacks a bounded Replay and follow-up".into());
    }
    let mut current = start.clone();
    let mut rust_steps = Vec::new();
    for (index, step) in source_steps.iter().enumerate() {
        let before_actions = v7_adapter_actions::legal_public_intents(&current)
            .map_err(|error| format!("Replay step {index} legal intents: {error}"))?;
        let intent = step["publicIntent"].clone();
        let admitted = match v7_adapter_actions::bind_public_intent(&current, intent) {
            Ok(admitted) => admitted,
            Err(error) => return Ok(json!({
                "status": "unavailable", "failedStep": index,
                "sourceSteps": source_steps, "rustSteps": rust_steps,
                "rustBeforeActions": before_actions,
                "reason": format!("Replay step {index} intent binding: {error}"),
            })),
        };
        let applied = v7_adapter_actions::apply_admitted(&current, &admitted)
            .map_err(|error| format!("Replay step {index} apply: {error}"))?;
        current = applied.position;
        rust_steps.push(json!({
            "kind": step["kind"], "beforeActions": before_actions,
            "position": current.export_envelope().map_err(|error| error.to_string())?,
            "replayFrame": current.state().extra["moveReplay"][actor],
            "actions": v7_adapter_actions::legal_public_intents(&current)
                .map_err(|error| format!("Replay step {index} next intents: {error}"))?,
            "observations": observations(&current)?, "result": rule_result(&current),
        }));
    }
    if source_unavailable {
        return Ok(json!({
            "status": "availability", "sourceSteps": source_steps,
            "rustSteps": rust_steps, "sourceBeforeActions": source["beforeActions"],
            "rustBeforeActions": v7_adapter_actions::legal_public_intents(&current)
                .map_err(|error| format!("Replay availability intents: {error}"))?,
        }));
    }
    Ok(json!({"status": "complete", "sourceSteps": source_steps, "rustSteps": rust_steps}))
}

fn classify_transition(sample: &Value) -> Value {
    // The available source export is one realized path. In each family the
    // listed callback can reach rule-visible effects outside that one path.
    // Do not infer determinism from seed equality or absence of an RNG call.
    let reason = match sample["publicIntent"]["type"].as_str() {
        Some("move") => "move/endMove can activate cards, hazards, capture and turn-entry effects",
        Some("card") => "card effect and finishCard callbacks have state-dependent rule branches",
        Some("draftPick" | "draftBundlePick") => "draft replacement offers and acquired effects can sample rule outcomes",
        _ => "no complete rule-effect dependency proof for this action family",
    };
    json!({"kind": "unknown", "method": "static-dependency-triage-v1",
        "reason": reason, "unresolved": "conditional rule effects and future Replay dependencies"})
}

fn paired_sample(case: &Value, sample: &Value, index: usize) -> Result<Value, String> {
    let name = case["name"].as_str().ok_or("source case name missing")?;
    let host = V7HostPosition::from_envelope(case["position"].clone())
        .map_err(|error| format!("host import: {error}"))?;
    let rust = host.export_envelope().map_err(|error| format!("baseline export: {error}"))?;
    let rust_actions = v7_adapter_actions::legal_public_intents(&host)
        .map_err(|error| format!("complete Rust legal intents: {error}"))?;
    let rust_observations = observations(&host)?;
    let rejected = if case["rejectPublicIntent"].is_null() {
        return Err("sample case has no wrong-actor rejection input".into());
    } else {
        let before = host.export_envelope().map_err(|error| error.to_string())?;
        let result = v7_adapter_actions::bind_public_intent(
            &host, case["rejectPublicIntent"].clone(),
        );
        let rejected = matches!(result, Err(v7_adapter_actions::V7ActionHostError::Engine(EngineError::WrongActor)));
        let after = host.export_envelope().map_err(|error| error.to_string())?;
        json!({"rejected": rejected, "unchanged": before == after})
    };
    let intent = sample["publicIntent"].clone();
    let admitted = v7_adapter_actions::bind_public_intent(&host, intent.clone())
        .map_err(|error| format!("selected intent binding: {error}"))?;
    let bound_payload = serde_json::to_value(admitted.action()).map_err(|error| error.to_string())?;
    let applied = v7_adapter_actions::apply_admitted(&host, &admitted)
        .map_err(|error| format!("Rust apply: {error}"))?;
    let rust_next = applied.position.export_envelope()
        .map_err(|error| format!("next export: {error}"))?;
    let rust_next_observations = observations(&applied.position)?;
    let replay = replay_sequence(&applied.position, &sample["replaySequence"])
        .unwrap_or_else(|reason| json!({"status": "unsupported", "reason": reason}));
    let mut replay = replay;
    let search_witness = case["replaySearch"]["selectedMove"] == sample["publicIntent"]
        && sample["replaySequence"]["status"] == "complete";
    replay["searchAttempts"] = if search_witness { case["replaySearch"]["attempts"].clone() } else { json!(0) };
    replay["acquired"] = json!(search_witness && case["replaySearch"]["acquired"].is_object());
    replay["sourceStatus"] = sample["replaySequence"]["status"].clone();
    let classification = classify_transition(sample);
    Ok(json!({
        "name": format!("{name}/sample[{index}]"),
        "generationStatus": "complete",
        "transitionKind": classification["kind"],
        "classificationEvidence": classification,
        "source": case["position"], "rust": rust,
        "sourceActions": case["publicIntents"], "rustActions": rust_actions,
        "sourceAction": intent, "rustAction": sample["publicIntent"],
        "rustBoundPayload": bound_payload,
        "sourceObservations": case["observations"], "rustObservations": rust_observations,
        "sourceRejection": case["sourceRejection"],
        "rustRejection": rejected,
        "sourceNext": sample["position"], "rustNext": rust_next,
        "sourceNextObservations": sample["observations"],
        "rustNextObservations": rust_next_observations,
        "sourceResult": sample["result"], "rustResult": rule_result(&applied.position),
        "replaySequence": replay,
    }))
}

fn run(report_path: &Path, cases_path: &Path, output_path: &Path) -> Result<(), String> {
    if ![report_path, cases_path, output_path].iter().all(|path| path.is_absolute()) {
        return Err("source report, source cases and paired output must be absolute paths".into());
    }
    if output_path == report_path || output_path == cases_path {
        return Err("paired output must not replace a source input".into());
    }
    let report: Value = serde_json::from_slice(&fs::read(report_path).map_err(|error| error.to_string())?)
        .map_err(|error| format!("source report JSON: {error}"))?;
    if report["status"] != "oracle-only"
        || report["source"]["sha256"] != SOURCE_SHA256 || report["source"]["profile"] != PROFILE
        || report["sourceExport"]["file"] != "source-cases.jsonl"
    {
        return Err("source report does not identify the pinned client/profile export".into());
    }
    let size = fs::metadata(cases_path).map_err(|error| error.to_string())?.len();
    if size > MAX_SOURCE_BYTES {
        return Err("source case export exceeds the 128 MiB budget".into());
    }
    let bytes = fs::read(cases_path).map_err(|error| error.to_string())?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    if report["sourceExport"]["sha256"] != digest {
        return Err("source case export digest differs from its report".into());
    }
    let source = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
    let lines = source.lines().collect::<Vec<_>>();
    if lines.is_empty() || lines.len() > MAX_CASES || report["sourceExport"]["cases"].as_u64() != Some(lines.len() as u64) {
        return Err("source case count differs from the bounded report".into());
    }
    let parent = output_path.parent().ok_or("paired output has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let partial_path = output_path.with_extension("partial");
    let mut output = BufWriter::new(File::create(&partial_path).map_err(|error| error.to_string())?);
    let mut pairs = 0usize;
    let mut incomplete = 0usize;
    for (line_index, line) in lines.iter().enumerate() {
        if line.len() > MAX_LINE_BYTES {
            return Err(format!("source case {} exceeds the 16 MiB line budget", line_index + 1));
        }
        let case: Value = serde_json::from_str(line)
            .map_err(|error| format!("source case {} JSON: {error}", line_index + 1))?;
        let samples = case["samples"].as_array()
            .ok_or_else(|| format!("source case {} samples missing", line_index + 1))?;
        for (sample_index, sample) in samples.iter().enumerate() {
            if pairs >= MAX_CASES {
                return Err("paired row count exceeds the 10,000 row budget".into());
            }
            let mut pair = match paired_sample(&case, sample, sample_index) {
                Ok(pair) => pair,
                Err(reason) => {
                    incomplete += 1;
                    json!({
                        "name": format!("{}/sample[{sample_index}]", case["name"].as_str().unwrap_or("unnamed")),
                        "generationStatus": "unsupported", "generationReason": reason,
                    })
                }
            };
            pair["provenance"] = json!({
                "sourceSha256": SOURCE_SHA256, "profile": PROFILE,
                "sourceExportSha256": digest.as_str(), "sourceCaseLine": line_index + 1,
                "sourceCaseSha256": format!("{:x}", Sha256::digest(line.as_bytes())),
                "sampleIndex": sample_index,
            });
            serde_json::to_writer(&mut output, &pair).map_err(|error| error.to_string())?;
            output.write_all(b"\n").map_err(|error| error.to_string())?;
            pairs += 1;
        }
    }
    output.flush().map_err(|error| error.to_string())?;
    eprintln!("paired source cases: {}, transition rows: {pairs}, incomplete: {incomplete}", lines.len());
    if pairs == 0 || incomplete > 0 {
        return Err("paired capture is empty or contains unsupported Rust executions; inspect the .partial file".into());
    }
    fs::rename(&partial_path, output_path).map_err(|error| error.to_string())?;
    Ok(())
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        eprintln!("usage: augment-chess-semantic-pairs SOURCE_REPORT SOURCE_CASES PAIRED_JSONL");
        std::process::exit(2);
    }
    if let Err(error) = run(Path::new(&args[1]), Path::new(&args[2]), Path::new(&args[3])) {
        eprintln!("semantic paired capture failed: {error}");
        std::process::exit(1);
    }
}
