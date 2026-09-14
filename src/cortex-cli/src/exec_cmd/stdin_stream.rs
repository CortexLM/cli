//! JSONL stdin multi-turn stream for `cortex exec --input-format stream-jsonl`.
//!
//! One JSON object per line, one line per turn — no envelope and no ids, unlike
//! [`super::runtime_contract_protocol`]. The connection stays open after a turn
//! completes, so a caller can drive several turns down one pipe and read one
//! stream back. A line that is not a usable turn is reported and skipped rather
//! than ending the stream, so a malformed line cannot silently drop the rest of
//! the input.

use std::io::{self, Write};
use std::time::Duration;

use anyhow::Result;
use cortex_engine::{Config, SessionHandle};
use cortex_protocol::{EventMsg, Op, Submission, UserInput};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use super::jsonrpc::event_to_jsonrpc;

/// One line of the JSONL turn stream.
#[derive(Debug, Deserialize)]
pub struct TurnLine {
    /// The turn text. Also accepted as `message` or `prompt`.
    #[serde(default, alias = "message", alias = "prompt")]
    pub text: Option<String>,

    /// Optional control message: `interrupt` stops the running turn, `shutdown`
    /// ends the stream.
    #[serde(default)]
    pub control: Option<String>,
}

/// Parsed action for one JSONL line.
#[derive(Debug, PartialEq, Eq)]
pub enum TurnAction {
    /// Submit this text as the next turn.
    Turn(String),
    /// Interrupt the running turn.
    Interrupt,
    /// End the stream.
    Shutdown,
}

/// Parse one JSONL line into a [`TurnAction`].
///
/// Blank lines are not actions (the caller skips them). A line that is not a
/// JSON object, or that carries no turn text and no known control, is an error
/// the caller reports without ending the stream.
pub fn parse_turn_line(line: &str) -> Result<TurnAction> {
    let parsed: TurnLine = serde_json::from_str(line)
        .map_err(|error| anyhow::anyhow!("Each line must be a JSON object: {error}"))?;
    if let Some(control) = parsed.control.as_deref() {
        return match control {
            "interrupt" | "cancel" => Ok(TurnAction::Interrupt),
            "shutdown" | "exit" => Ok(TurnAction::Shutdown),
            other => Err(anyhow::anyhow!(
                "Unknown control `{other}`. Use `interrupt` or `shutdown`."
            )),
        };
    }
    match parsed.text.as_deref().map(str::trim) {
        Some(text) if !text.is_empty() => Ok(TurnAction::Turn(text.to_string())),
        _ => Err(anyhow::anyhow!(
            "Each line needs a nonempty `text` field, or a `control` of `interrupt` or `shutdown`."
        )),
    }
}

/// Read stdin as JSONL lines on a dedicated thread, so a shutdown does not wait
/// for EOF while the runtime is being torn down.
pub(super) fn stdin_lines() -> mpsc::Receiver<io::Result<String>> {
    use std::io::BufRead;
    let (tx, rx) = mpsc::channel(16);
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            if tx.blocking_send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn write_json(output: &mut impl Write, value: &impl serde::Serialize) -> Result<()> {
    serde_json::to_writer(&mut *output, value)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}

fn write_error(output: &mut impl Write, message: &str) -> Result<()> {
    write_json(output, &json!({ "type": "error", "message": message }))
}

async fn submit(handle: &SessionHandle, op: Op) -> Result<()> {
    handle
        .submission_tx
        .send(Submission {
            id: uuid::Uuid::new_v4().to_string(),
            op,
        })
        .await?;
    Ok(())
}

/// Mutable stream state for one JSONL connection.
struct StreamState {
    busy: bool,
    turns: usize,
    deadline: Option<tokio::time::Instant>,
    shutdown_requested: bool,
    max_turns: usize,
    timeout_secs: u64,
}

impl StreamState {
    fn new(max_turns: usize, timeout_secs: u64) -> Self {
        Self {
            busy: false,
            turns: 0,
            deadline: None,
            shutdown_requested: false,
            max_turns,
            timeout_secs,
        }
    }

    /// Forward one session event. Returns `true` when the stream is done.
    fn on_event(
        &mut self,
        event: &cortex_protocol::Event,
        session_id: &cortex_protocol::ConversationId,
        output: &mut impl Write,
    ) -> Result<bool> {
        let legacy = event_to_jsonrpc(event, session_id);
        let payload = match &event.msg {
            EventMsg::Warning(warning) => {
                json!({"method":"warning", "params":{"message":warning.message}})
            }
            EventMsg::TurnAborted(aborted) => {
                json!({"method":"turn_aborted", "params":aborted})
            }
            _ => legacy.result.unwrap_or_default(),
        };
        write_json(
            output,
            &json!({
                "type": "event",
                "method": payload["method"],
                "params": payload["params"],
                "turn_id": event.id,
            }),
        )?;
        match &event.msg {
            // TaskComplete ends a turn, never the connection.
            EventMsg::TaskComplete(_) | EventMsg::Error(_) | EventMsg::TurnAborted(_) => {
                self.busy = false;
                self.deadline = None;
                write_json(output, &json!({"type": "turn_complete"}))?;
                Ok(false)
            }
            EventMsg::ShutdownComplete => Ok(true),
            _ => Ok(false),
        }
    }

    /// Apply one parsed stdin line. Returns `true` when the stream is done.
    async fn on_action(
        &mut self,
        action: TurnAction,
        handle: &SessionHandle,
        output: &mut impl Write,
    ) -> Result<bool> {
        match action {
            TurnAction::Shutdown => {
                self.shutdown_requested = true;
                submit(handle, Op::Shutdown).await?;
                Ok(false)
            }
            TurnAction::Interrupt => {
                submit(handle, Op::Interrupt).await?;
                write_json(output, &json!({"type": "interrupt_requested"}))?;
                Ok(false)
            }
            TurnAction::Turn(text) => {
                if self.busy {
                    write_error(
                        output,
                        "A turn is already running. Send `interrupt` or wait for it to finish.",
                    )?;
                    return Ok(false);
                }
                if self.turns >= self.max_turns {
                    write_error(output, "Maximum user turns reached.")?;
                    return Ok(false);
                }
                submit(
                    handle,
                    Op::UserInput {
                        items: vec![UserInput::Text { text: text.into() }],
                    },
                )
                .await?;
                self.turns += 1;
                self.busy = true;
                self.deadline = (self.timeout_secs != 0)
                    .then(|| tokio::time::Instant::now() + Duration::from_secs(self.timeout_secs));
                write_json(
                    output,
                    &json!({"type": "turn_accepted", "turn": self.turns}),
                )?;
                Ok(false)
            }
        }
    }
}

/// Drive a JSONL stdin multi-turn stream until EOF, `shutdown`, or an error.
pub(super) async fn run_stream(
    handle: &SessionHandle,
    mut lines: mpsc::Receiver<io::Result<String>>,
    output: &mut impl Write,
    config: &Config,
    max_turns: usize,
    timeout_secs: u64,
) -> Result<()> {
    write_json(
        output,
        &json!({
            "type": "initialized",
            "session_id": handle.conversation_id.to_string(),
            "model": config.model,
            "cwd": config.cwd,
            "input_format": "stream-jsonl",
        }),
    )?;

    let mut state = StreamState::new(max_turns, timeout_secs);

    loop {
        tokio::select! {
            event = handle.event_rx.recv() => {
                let event = event.map_err(|_| anyhow::anyhow!(
                    "The session event channel closed without a shutdown acknowledgement."
                ))?;
                if state.on_event(&event, &handle.conversation_id, output)? {
                    return Ok(());
                }
            }
            line = lines.recv(), if !state.shutdown_requested => {
                let Some(line) = line else {
                    if state.busy {
                        anyhow::bail!("Input closed while a turn was active; the turn was stopped.");
                    }
                    return Ok(());
                };
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let action = match parse_turn_line(&line) {
                    Ok(action) => action,
                    Err(error) => {
                        // A bad line is reported; the stream keeps going.
                        write_error(output, &error.to_string())?;
                        continue;
                    }
                };
                state.on_action(action, handle, output).await?;
            }
            _ = async {
                match state.deadline {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            } => {
                submit(handle, Op::Interrupt).await?;
                state.deadline = None;
                write_error(output, "The turn deadline was reached; cancellation was requested.")?;
            }
        }
        if state.shutdown_requested {
            return wait_for_shutdown(handle).await;
        }
    }
}

async fn wait_for_shutdown(handle: &SessionHandle) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Ok(event) = handle.event_rx.recv().await {
            if matches!(event.msg, EventMsg::ShutdownComplete) {
                return Ok(());
            }
        }
        anyhow::bail!("The session closed without acknowledging shutdown.")
    })
    .await?
}

/// Validate the JSONL stream shape from a string slice (used by tests and by
/// callers that already hold the whole input).
#[cfg(test)]
fn parse_all(input: &str) -> Result<Vec<TurnAction>> {
    let mut actions = Vec::new();
    for line in input.lines() {
        if line.trim().is_empty() {
            continue;
        }
        actions.push(parse_turn_line(line)?);
    }
    Ok(actions)
}

/// A JSON value that is not an object cannot carry a turn.
#[cfg(test)]
fn is_turn_object(value: &serde_json::Value) -> bool {
    value.is_object()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_line_is_the_next_turn() {
        assert_eq!(
            parse_turn_line(r#"{"text":"add a retry helper"}"#).unwrap(),
            TurnAction::Turn("add a retry helper".into())
        );
        // `message` and `prompt` are accepted aliases.
        assert_eq!(
            parse_turn_line(r#"{"message":"ship it"}"#).unwrap(),
            TurnAction::Turn("ship it".into())
        );
        assert_eq!(
            parse_turn_line(r#"{"prompt":"review src/auth"}"#).unwrap(),
            TurnAction::Turn("review src/auth".into())
        );
    }

    #[test]
    fn control_lines_interrupt_and_shutdown() {
        assert_eq!(
            parse_turn_line(r#"{"control":"interrupt"}"#).unwrap(),
            TurnAction::Interrupt
        );
        assert_eq!(
            parse_turn_line(r#"{"control":"cancel"}"#).unwrap(),
            TurnAction::Interrupt
        );
        assert_eq!(
            parse_turn_line(r#"{"control":"shutdown"}"#).unwrap(),
            TurnAction::Shutdown
        );
        assert_eq!(
            parse_turn_line(r#"{"control":"exit"}"#).unwrap(),
            TurnAction::Shutdown
        );
        let error = parse_turn_line(r#"{"control":"reboot"}"#).unwrap_err();
        assert!(error.to_string().contains("Unknown control"), "{error}");
    }

    #[test]
    fn malformed_lines_are_errors_not_turns() {
        for line in [
            "not json",
            "[]",
            r#"{"text":""}"#,
            r#"{"text":"   "}"#,
            r#"{"timeout":5}"#,
            "{}",
        ] {
            assert!(parse_turn_line(line).is_err(), "{line} must be refused");
        }
    }

    #[test]
    fn a_stream_keeps_going_after_a_turn() {
        let actions =
            parse_all("{\"text\":\"one\"}\n\n{\"text\":\"two\"}\n{\"control\":\"shutdown\"}\n")
                .expect("stream");
        assert_eq!(
            actions,
            vec![
                TurnAction::Turn("one".into()),
                TurnAction::Turn("two".into()),
                TurnAction::Shutdown,
            ]
        );
    }

    #[test]
    fn a_bad_line_in_the_middle_is_reported_by_the_caller() {
        // `parse_all` surfaces the bad line; `run_stream` writes an error event
        // and continues, so the rest of the stream is not lost.
        assert!(parse_all("{\"text\":\"one\"}\nnot json\n").is_err());
    }

    #[test]
    fn json_objects_are_turns_and_other_values_are_not() {
        assert!(is_turn_object(&json!({"text": "hi"})));
        assert!(!is_turn_object(&json!([1, 2])));
        assert!(!is_turn_object(&json!("plain")));
    }
}
