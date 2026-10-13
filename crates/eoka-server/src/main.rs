use eoka_server::{dispatch, protocol, state};

use serde_json::Value;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use protocol::{Request, Response, ServerError};
use state::AppState;

enum ParsedLine {
    Request(Request),
    Malformed { id: Value, error: ServerError },
    Skip,
}

fn parse_line(line: &str) -> ParsedLine {
    let request_err = match serde_json::from_str::<Request>(line) {
        Ok(req) => return ParsedLine::Request(req),
        Err(e) => e,
    };

    let value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, line, "unparseable input line, skipping");
            return ParsedLine::Skip;
        }
    };

    match value.get("id").cloned() {
        Some(id) => ParsedLine::Malformed {
            id,
            error: ServerError::invalid_params(request_err.to_string()),
        },
        None => {
            tracing::warn!(error = %request_err, line, "request has no id, skipping");
            ParsedLine::Skip
        }
    }
}

async fn write_response(stdout: &mut io::Stdout, response: &Response) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec(response)?;
    bytes.push(b'\n');
    stdout.write_all(&bytes).await?;
    stdout.flush().await?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut lines = BufReader::new(io::stdin()).lines();
    let stdout = std::sync::Arc::new(tokio::sync::Mutex::new(io::stdout()));
    let mut state = AppState::new();

    loop {
        let line = match lines.next_line().await? {
            Some(line) => line,
            None => {
                tracing::info!("stdin closed, exiting");
                anyhow::bail!("stdin closed before browser.close");
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let request = match parse_line(&line) {
            ParsedLine::Skip => continue,
            ParsedLine::Malformed { id, error } => {
                write_response(&mut *stdout.lock().await, &Response::err(id, error)).await?;
                continue;
            }
            ParsedLine::Request(req) => req,
        };

        if request.method == "operations.wait" {
            let manager = state.operations.clone();
            let stdout = stdout.clone();
            tokio::spawn(async move {
                let id = request.params["id"].as_str().unwrap_or_default();
                let timeout = request.params["timeout_ms"].as_u64().unwrap_or(30000);
                let response = match manager.wait(id, timeout).await {
                    Ok(record) => Response::ok(
                        request.id,
                        serde_json::to_value(record).unwrap_or(Value::Null),
                    ),
                    Err(error) => {
                        Response::err(request.id, ServerError::internal(error.to_string()))
                    }
                };
                let _ = write_response(&mut *stdout.lock().await, &response).await;
            });
            continue;
        }
        let response = match dispatch::dispatch(&mut state, &request.method, request.params).await {
            Ok(result) => Response::ok(request.id, result),
            Err(error) => Response::err(request.id, error),
        };

        write_response(&mut *stdout.lock().await, &response).await?;

        if state.should_shutdown() {
            break;
        }
    }

    Ok(())
}
