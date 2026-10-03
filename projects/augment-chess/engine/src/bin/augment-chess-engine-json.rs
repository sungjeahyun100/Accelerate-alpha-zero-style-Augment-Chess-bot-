//! JSON transport adapter for rules validation; stdout contains protocol responses only.
use augment_chess_engine::{Action, Color, GameConfig, Position};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

/// Inspect the reader's bounded buffer before extending a request allocation.
/// Oversized input closes this transport; it is never drained into a giant
/// temporary string or passed to the rules kernel.
fn read_request(reader: &mut impl BufRead, limit: usize) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::with_capacity(limit.min(4096));
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok((!line.is_empty()).then_some(line));
        }
        let newline = chunk.iter().position(|byte| *byte == b'\n');
        let amount = newline.map_or(chunk.len(), |index| index + 1);
        if amount > limit.saturating_sub(line.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request exceeds 16 MiB limit",
            ));
        }
        line.extend_from_slice(&chunk[..amount]);
        reader.consume(amount);
        if newline.is_some() {
            return Ok(Some(line));
        }
    }
}

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
    let position = Position::from_snapshot_value(state.clone()).map_err(|e| e.to_string())?;
    let position = if let Some(rng) = request.get("rng") {
        position
            .with_metadata(
                serde_json::from_value(rng.clone()).map_err(|e| e.to_string())?,
                request
                    .get("history")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            )
            .map_err(|e| e.to_string())?
    } else {
        position
    };
    match method {
        "draft_availability" => {
            let color: Color =
                serde_json::from_value(request.get("color").cloned().ok_or("color missing")?)
                    .map_err(|e| e.to_string())?;
            Ok(
                json!({"availability":position.draft_availability(color).map_err(|e|e.to_string())?}),
            )
        }
        "legal_actions" => {
            Ok(json!({"legalActions":position.legal_actions().map_err(|e|e.to_string())?}))
        }
        "bind_payload" => Ok(json!({"action":position.bind_payload(
            request.get("action").cloned().ok_or("action missing")?
        ).map_err(|e|e.to_string())?})),
        "public_intent" => {
            let action: Action =
                serde_json::from_value(request.get("action").cloned().ok_or("action missing")?)
                    .map_err(|e| e.to_string())?;
            Ok(json!({"intent":position.public_intent(&action).map_err(|e|e.to_string())?}))
        }
        "bind_public_intent" => Ok(json!({"action":position.bind_public_intent(
            request.get("intent").cloned().ok_or("intent missing")?
        ).map_err(|e|e.to_string())?})),
        "apply" => {
            let action: Action =
                serde_json::from_value(request.get("action").cloned().ok_or("action missing")?)
                    .map_err(|e| e.to_string())?;
            let step = position.apply(&action).map_err(|e| e.to_string())?;
            Ok(
                json!({"state":step.position.state(),"sourceState":step.position.export_state().map_err(|e|e.to_string())?,"rng":step.position.state().rng,"history":step.position.state().history,"actor":step.actor,"turnChanged":step.turn_changed,"captures":step.captures,"result":step.result}),
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
    let mut input = input.lock();
    let mut output = io::BufWriter::new(io::stdout().lock());
    loop {
        let response = match read_request(&mut input, 16 * 1024 * 1024) {
            Ok(None) => break,
            Ok(Some(line)) => match serde_json::from_slice::<Value>(&line) {
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
            },
            Err(error) => {
                let _ = writeln!(output, "{}", json!({"error":error.to_string()}));
                let _ = output.flush();
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_requests_accept_multiple_lines_and_reject_before_overflow_copy() {
        let mut reader = io::Cursor::new(b"{}\n[]\n");
        assert_eq!(
            read_request(&mut reader, 4).unwrap(),
            Some(b"{}\n".to_vec())
        );
        assert_eq!(
            read_request(&mut reader, 4).unwrap(),
            Some(b"[]\n".to_vec())
        );
        assert_eq!(read_request(&mut reader, 4).unwrap(), None);
        let mut reader = io::Cursor::new(b"123456789\n");
        assert_eq!(
            read_request(&mut reader, 8).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(reader.position(), 0);
    }
}
