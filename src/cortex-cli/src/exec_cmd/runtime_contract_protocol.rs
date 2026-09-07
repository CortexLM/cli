//! Streaming JSON-RPC with independent stdin, event, deadline, and cancellation
//! lifecycles. TaskComplete ends a turn, not the protocol connection.

use std::io::{self, BufRead, Write};
use std::time::Duration;

use anyhow::{Result, bail};
use cortex_engine::{Config, SessionHandle};
use cortex_protocol::{
    EventMsg, ExecApprovalRequestEvent, Op, ReviewDecision, Submission, UserInput,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::autonomy::AutonomyLevel;
use super::jsonrpc::{JsonRpcRequest, JsonRpcResponse, event_to_jsonrpc};

pub(super) fn stdin_lines() -> mpsc::Receiver<io::Result<String>> {
    let (tx, rx) = mpsc::channel(16);
    // A standard thread, not spawn_blocking: closing the protocol on a shutdown
    // message must not wait for stdin EOF during Tokio runtime destruction.
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            if tx.blocking_send(line).is_err() {
                break;
            }
        }
    });
    rx
}

pub(super) fn approval_decision(
    request: &ExecApprovalRequestEvent,
    autonomy: Option<AutonomyLevel>,
    skip_permissions: bool,
) -> ReviewDecision {
    if skip_permissions {
        return ReviewDecision::Approved;
    }
    let Some(assessment) = &request.sandbox_assessment else {
        // Missing evidence is not a low-risk assessment.
        return ReviewDecision::Denied;
    };
    let risk = match assessment.risk_level {
        cortex_protocol::SandboxRiskLevel::Low => "low",
        cortex_protocol::SandboxRiskLevel::Medium => "medium",
        cortex_protocol::SandboxRiskLevel::High => "high",
    };
    if autonomy.is_some_and(|level| level.allows_risk(risk, &request.command.join(" "))) {
        ReviewDecision::Approved
    } else {
        ReviewDecision::Denied
    }
}

fn write_json(output: &mut impl Write, value: &impl serde::Serialize) -> Result<()> {
    serde_json::to_writer(&mut *output, value)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}

/// Mutable protocol state for one stdio session.
#[derive(Default)]
struct ProtocolState {
    busy: bool,
    turns: usize,
    deadline: Option<tokio::time::Instant>,
    active_id: Value,
}

/// Outcome of one request: either an answer for the client, or a shutdown that
/// must be awaited rather than answered here.
enum Dispatch {
    Answered(std::result::Result<Value, (i32, &'static str)>),
    Shutdown,
}

/// Apply one validated JSON-RPC request. A turn starts only when no other turn
/// is active and the turn budget allows it.
#[allow(clippy::too_many_arguments)]
async fn dispatch_request(
    handle: &SessionHandle,
    output: &mut impl Write,
    request: &JsonRpcRequest,
    id: &Value,
    state: &mut ProtocolState,
    max_turns: usize,
    timeout_secs: u64,
    _autonomy: Option<AutonomyLevel>,
    _skip_permissions: bool,
) -> Result<Dispatch> {
    if request.jsonrpc.as_deref() != Some("2.0") {
        return Ok(Dispatch::Answered(Err((-32600, "jsonrpc must be 2.0."))));
    }
    let answer = match request.method.as_str() {
        "message" | "user_input" if state.busy => Err((
            -32000,
            "A turn is already running. Cancel it or wait for its terminal event.",
        )),
        "message" | "user_input" if state.turns >= max_turns => {
            Err((-32000, "Maximum user turns reached."))
        }
        "message" | "user_input" => {
            let text = request
                .params
                .get("text")
                .or_else(|| request.params.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if text.trim().is_empty() {
                Err((-32602, "A nonempty text message is required."))
            } else {
                state.active_id = id.clone();
                submit(
                    handle,
                    Op::UserInput {
                        items: vec![UserInput::Text { text: text.into() }],
                    },
                )
                .await?;
                state.turns += 1;
                state.busy = true;
                state.deadline = (timeout_secs != 0)
                    .then(|| tokio::time::Instant::now() + Duration::from_secs(timeout_secs));
                Ok(json!({"accepted": true}))
            }
        }
        "interrupt" | "cancel" => {
            submit(handle, Op::Interrupt).await?;
            Ok(json!({"cancellation_requested": true}))
        }
        "shutdown" | "exit" => {
            if request.id.is_some() {
                write_json(
                    output,
                    &JsonRpcResponse::result(id.clone(), json!({"shutdown_requested": true})),
                )?;
            }
            submit(handle, Op::Shutdown).await?;
            // Keep receiving until ShutdownComplete, without needing EOF.
            return Ok(Dispatch::Shutdown);
        }
        _ => Err((-32601, "Method not found.")),
    };
    Ok(Dispatch::Answered(answer))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_protocol(
    handle: &SessionHandle,
    mut lines: mpsc::Receiver<io::Result<String>>,
    output: &mut impl Write,
    config: &Config,
    autonomy: Option<AutonomyLevel>,
    skip_permissions: bool,
    max_turns: usize,
    timeout_secs: u64,
) -> Result<()> {
    write_json(
        output,
        &json!({
            "jsonrpc": "2.0", "method": "initialized",
            "params": { "session_id": handle.conversation_id.to_string(),
                "model": config.model, "cwd": config.cwd }
        }),
    )?;
    let mut state = ProtocolState::default();
    loop {
        tokio::select! {
            event = handle.event_rx.recv() => {
                let event = event.map_err(|_| anyhow::anyhow!(
                    "The session event channel closed without a shutdown acknowledgement."
                ))?;
                let legacy = event_to_jsonrpc(&event, &handle.conversation_id);
                let payload = match &event.msg {
                    EventMsg::Warning(warning) => json!({"method":"warning", "params":{"message":warning.message}}),
                    EventMsg::TurnAborted(aborted) => json!({"method":"turn_aborted", "params":aborted}),
                    _ => legacy.result.unwrap_or_default(),
                };
                write_json(output, &json!({
                    "jsonrpc": "2.0", "method": payload["method"],
                    "params": { "event": payload["params"], "turn_id": event.id, "request_id": state.active_id }
                }))?;
                match event.msg {
                    EventMsg::TaskComplete(_) | EventMsg::Error(_) | EventMsg::TurnAborted(_) => {
                        state.busy = false;
                        state.deadline = None;
                    }
                    EventMsg::ExecApprovalRequest(approval) => {
                        let decision = approval_decision(&approval, autonomy, skip_permissions);
                        submit(handle, Op::ExecApproval { id: approval.call_id, decision }).await?;
                    }
                    EventMsg::ShutdownComplete => return Ok(()),
                    _ => {}
                }
            }
            line = lines.recv() => {
                let Some(line) = line else {
                    if state.busy { bail!("Input closed while a turn was active; the turn was stopped."); }
                    return Ok(());
                };
                let line = line?;
                if line.trim().is_empty() { continue; }
                let request = match serde_json::from_str::<JsonRpcRequest>(&line) {
                    Ok(request) => request,
                    Err(_) => {
                        write_json(output, &JsonRpcResponse::error(Value::Null, -32700, "Invalid JSON-RPC input.".into()))?;
                        continue;
                    }
                };
                let id = request.id.clone().unwrap_or(Value::Null);
                let result = match dispatch_request(
                    handle, output, &request, &id, &mut state, max_turns, timeout_secs, autonomy,
                    skip_permissions,
                )
                .await?
                {
                    Dispatch::Shutdown => return wait_for_shutdown(handle, output).await,
                    Dispatch::Answered(result) => result,
                };
                if request.id.is_some() {
                    let response = match result {
                        Ok(value) => JsonRpcResponse::result(id, value),
                        Err((code, message)) => JsonRpcResponse::error(id, code, message.into()),
                    };
                    write_json(output, &response)?;
                }
            }
            _ = async {
                match state.deadline {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            } => {
                submit(handle, Op::Interrupt).await?;
                state.deadline = None;
                write_json(output, &json!({"jsonrpc":"2.0", "method":"error", "params":{"request_id": state.active_id, "message":"The turn deadline was reached; cancellation was requested."}}))?;
            }
        }
    }
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

async fn wait_for_shutdown(handle: &SessionHandle, output: &mut impl Write) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Ok(event) = handle.event_rx.recv().await {
            write_json(
                output,
                &json!({"jsonrpc":"2.0", "method":"event", "params":event}),
            )?;
            if matches!(event.msg, EventMsg::ShutdownComplete) {
                return Ok(());
            }
        }
        bail!("The session closed without acknowledging shutdown.")
    })
    .await?
}

#[cfg(test)]
#[path = "runtime_contract_protocol_tests.rs"]
mod tests;
