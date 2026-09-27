//! JSON transport adapter for rules validation; stdout contains protocol responses only.
use accelerate_engine::{Action, Color, GameConfig, Position};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

fn execute(request: &Value) -> Result<Value, String> {
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("inspect");
    if method == "new_game" {
        let config: GameConfig =
            serde_json::from_value(request.get("config").cloned().unwrap_or_else(|| json!({})))
                .map_err(|e| e.to_string())?;
        let position = Position::new_game(
            config,
            request.get("seed").and_then(Value::as_u64).unwrap_or(0),
        )
        .map_err(|e| e.to_string())?;
        return Ok(
            json!({"state":position.state(),"positionKey":format!("{:016x}",position.key())}),
        );
    }
    let state = request.get("state").ok_or("state missing")?;
    let position = Position::from_json(&state.to_string()).map_err(|e| e.to_string())?;
    match method {
        "legal_actions" => {
            Ok(json!({"legalActions":position.legal_actions().map_err(|e|e.to_string())?}))
        }
        "apply" => {
            let action: Action =
                serde_json::from_value(request.get("action").cloned().ok_or("action missing")?)
                    .map_err(|e| e.to_string())?;
            let step = position.apply(&action).map_err(|e| e.to_string())?;
            Ok(
                json!({"state":step.position.state(),"actor":step.actor,"turnChanged":step.turn_changed,"captures":step.captures,"result":step.result}),
            )
        }
        "observe" => {
            let viewer: Color =
                serde_json::from_value(request.get("viewer").cloned().ok_or("viewer missing")?)
                    .map_err(|e| e.to_string())?;
            Ok(json!({"observation":position.try_observe(viewer).map_err(|e|e.to_string())?}))
        }
        "inspect" => Ok(
            json!({"state":position.state(),"legalActions":position.legal_actions().map_err(|e|e.to_string())?,"result":position.result()}),
        ),
        _ => Err(format!("unknown method {method}")),
    }
}

fn main() {
    let input = io::stdin();
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in input.lock().lines() {
        let response = match line {
            Ok(line) if line.len() <= 16 * 1024 * 1024 => {
                match serde_json::from_str::<Value>(&line) {
                    Ok(request) => {
                        let id = request.get("id").cloned().unwrap_or(Value::Null);
                        match execute(&request) {
                            Ok(mut response) => {
                                response["id"] = id;
                                response
                            }
                            Err(error) => json!({"id":id,"error":error}),
                        }
                    }
                    Err(error) => json!({"error":format!("invalid JSON: {error}")}),
                }
            }
            Ok(_) => json!({"error":"request exceeds 16 MiB limit"}),
            Err(error) => {
                eprintln!("stdin: {error}");
                break;
            }
        };
        if writeln!(output, "{response}")
            .and_then(|_| output.flush())
            .is_err()
        {
            break;
        }
    }
}
