//! `batch`: many commands over one connection, for programs that would otherwise pay for a new
//! connection and sign-in each time. Each line of standard input is a JSON array of arguments,
//! or an object with `args`, an optional `id` to echo, and `input` or `inputBase64` for
//! `write`. Each gets one line back on standard output, in order.

use base64::Engine;
use clap::Parser;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};

use super::args::{BatchLine, Command};
use super::commands;
use super::context::Context;
use super::exit::Exit;
use super::output::{error_value, Reply};
use crate::error::{AppError, AppResult};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Request {
    Arguments(Vec<String>),
    #[serde(rename_all = "camelCase")]
    Full {
        #[serde(default)]
        id: Option<Value>,
        #[serde(rename = "args")]
        arguments: Vec<String>,
        #[serde(default)]
        input: Option<String>,
        #[serde(default)]
        input_base64: Option<String>,
    },
}

struct Parsed {
    id: Value,
    arguments: Vec<String>,
    input: Option<Vec<u8>>,
}

fn parse_request(line: &str, number: u64) -> Result<Parsed, (Value, AppError)> {
    let request: Request = serde_json::from_str(line).map_err(|error| {
        (
            json!(number),
            AppError::invalid(format!(
                "Line {number} is not a JSON array of arguments or an object with \"args\" ({error})"
            )),
        )
    })?;
    match request {
        Request::Arguments(arguments) => Ok(Parsed {
            id: json!(number),
            arguments,
            input: None,
        }),
        Request::Full {
            id,
            arguments,
            input,
            input_base64,
        } => {
            let id = id.unwrap_or_else(|| json!(number));
            let input = match (input, input_base64) {
                (Some(text), _) => Some(text.into_bytes()),
                (None, Some(encoded)) => Some(
                    base64::engine::general_purpose::STANDARD
                        .decode(encoded.trim())
                        .map_err(|error| {
                            (
                                id.clone(),
                                AppError::invalid(format!("inputBase64 is not valid ({error})")),
                            )
                        })?,
                ),
                (None, None) => None,
            };
            Ok(Parsed {
                id,
                arguments,
                input,
            })
        }
    }
}

fn answer(id: Value, outcome: AppResult<Reply>) -> (Value, bool) {
    match outcome {
        Ok(reply) => {
            let ok = reply.exit == Exit::Success;
            (
                json!({
                    "id": id,
                    "ok": ok,
                    "exitCode": reply.exit.code(),
                    "result": reply.value,
                }),
                ok,
            )
        }
        Err(error) => {
            let mut value = error_value(&error);
            if let Some(fields) = value.as_object_mut() {
                let code = Exit::for_error(error.kind).code();
                fields.insert("id".into(), id);
                fields.insert("ok".into(), json!(false));
                fields.insert("exitCode".into(), json!(code));
            }
            (value, false)
        }
    }
}

async fn run_line(context: &Context, parsed: Parsed) -> AppResult<Reply> {
    let line = BatchLine::try_parse_from(&parsed.arguments).map_err(|error| {
        let message = error.render().to_string();
        AppError::invalid(message.trim().trim_start_matches("error: "))
    })?;
    match &line.command {
        Command::Batch => Err(AppError::invalid("batch cannot run inside batch")),
        Command::Write(_) if parsed.input.is_none() => Err(AppError::invalid(
            "In a batch, write takes its data from \"input\" or \"inputBase64\"",
        )),
        command => commands::run(context, command, true, parsed.input).await,
    }
}

/// Answers every line, then ends with the status of the run as a whole: partial when any line
/// failed.
pub async fn run(context: &Context) -> AppResult<Reply> {
    context.connected().await?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let (mut answered, mut failed) = (0u64, 0u64);
    loop {
        let line = tokio::select! {
            line = lines.next_line() => line?,
            _ = context.interrupt.cancelled() => return Err(AppError::cancelled()),
        };
        let Some(line) = line else { break };
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        answered += 1;
        let (value, ok) = match parse_request(line, answered) {
            Ok(parsed) => {
                let id = parsed.id.clone();
                answer(id, run_line(context, parsed).await)
            }
            Err((id, error)) => answer(id, Err(error)),
        };
        if !ok {
            failed += 1;
        }
        println!("{}", serde_json::to_string(&value).unwrap_or_default());
    }
    let reply = Reply::new(Value::Null, "");
    Ok(match failed {
        0 => reply,
        _ => reply.with_exit(Exit::Partial),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_arrays_or_objects() {
        let plain = parse_request(r#"["ls", "-l", "/srv"]"#, 3).unwrap();
        assert_eq!(plain.id, json!(3));
        assert_eq!(plain.arguments, ["ls", "-l", "/srv"]);

        let full = parse_request(
            r#"{"id": "note", "args": ["write", "/tmp/a"], "input": "hello"}"#,
            1,
        )
        .unwrap();
        assert_eq!(full.id, json!("note"));
        assert_eq!(full.input.as_deref(), Some(&b"hello"[..]));

        let binary =
            parse_request(r#"{"args": ["write", "/b"], "inputBase64": "/wA="}"#, 2).unwrap();
        assert_eq!(binary.input.as_deref(), Some(&[0xff, 0x00][..]));
        assert_eq!(binary.id, json!(2));

        let (id, error) = parse_request("ls /srv", 5).err().unwrap();
        assert_eq!(id, json!(5));
        assert!(error.message.contains("Line 5"));
    }

    #[test]
    fn answers_carry_the_id_and_status() {
        let (value, ok) = answer(json!(7), Err(AppError::cancelled()));
        assert!(!ok);
        assert_eq!(value["id"], 7);
        assert_eq!(value["exitCode"], 130);
        assert_eq!(value["error"]["kind"], "cancelled");

        let (value, ok) = answer(json!("x"), Ok(Reply::new(json!({ "a": 1 }), "")));
        assert!(ok);
        assert_eq!(value["result"]["a"], 1);
    }
}
