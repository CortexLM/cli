//! Isolated delegation to the server-owned Code harness.

use super::core::EventLoop;
use crate::app::SubagentTaskDisplay;
use crate::events::ToolEvent;
use cortex_engine::client::runtime_contract::{INCOMPLETE_STREAM, LOCAL_TOOLS_UNSUPPORTED};
use cortex_engine::client::{CompletionRequest, Message, ModelClient, ResponseEvent};
use std::time::{Duration, Instant};
use tokio_stream::StreamExt;

static CHILD_LIMIT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

impl EventLoop {
    pub(super) fn spawn_subagent(&mut self, tool_call_id: String, args: serde_json::Value) {
        let parsed = parse_child_request(&args);
        let (prompt, description, role, timeout_secs, model) = match parsed {
            Ok(request) => request,
            Err(error) => {
                self.app_state.add_pending_tool_result(
                    tool_call_id,
                    "Task".into(),
                    error.to_string(),
                    false,
                );
                return;
            }
        };
        let Some(manager) = self.provider_manager.clone() else {
            self.app_state.add_pending_tool_result(
                tool_call_id,
                "Task".into(),
                "No client is configured for delegation.".into(),
                false,
            );
            return;
        };
        let permit = match CHILD_LIMIT.try_acquire() {
            Ok(permit) => permit,
            Err(_) => {
                self.app_state.add_pending_tool_result(tool_call_id, "Task".into(), "The four-child concurrency limit was reached. Wait for an active child to finish.".into(), false);
                return;
            }
        };
        let display_id = format!("subagent_{tool_call_id}");
        self.app_state.add_subagent_task(SubagentTaskDisplay::new(
            display_id,
            tool_call_id.clone(),
            description,
            role.clone(),
        ));
        self.app_state.streaming.start_delegation();
        let tool_tx = self.tool_event_tx.clone();
        let id = tool_call_id.clone();
        let task = tokio::spawn(async move {
            let _permit = permit;
            let started_at = Instant::now();
            let _ = tool_tx
                .send(ToolEvent::Started {
                    id: id.clone(),
                    name: "Task".into(),
                    started_at,
                })
                .await;
            let client = {
                let mut manager = manager.write().await;
                manager.ensure_client().ok();
                manager.snapshot_client().and_then(|client| {
                    let child = client.fresh_clone_box();
                    // snapshot_client can transfer ownership for non-cloneable
                    // clients; never consume the parent's transport.
                    manager.restore_client(client);
                    child
                })
            };
            let Some(client) = client else {
                let _ = tool_tx.send(ToolEvent::Failed { id, name: "Task".into(), error: "The active client cannot provide an isolated child session. No task was started.".into(), duration: started_at.elapsed() }).await;
                return;
            };
            let request = CompletionRequest {
                model: model.unwrap_or_else(|| client.model().to_string()),
                messages: vec![
                    Message::system(format!(
                        "Delegated role: {role}. Complete only the assigned task. This role is guidance, not a permission grant."
                    )),
                    Message::user(prompt),
                ],
                ..Default::default()
            };
            let outcome = tokio::time::timeout(
                Duration::from_secs(timeout_secs),
                collect_child_turn(client.as_ref(), request),
            )
            .await;
            let outcome = match outcome {
                Ok(result) => result,
                Err(_) => {
                    let cancellation = client.cancel_turn_checked().await;
                    Err(anyhow::anyhow!(
                        "Child deadline reached. {}",
                        cancellation
                            .err()
                            .map(|e| e.user_friendly_message())
                            .unwrap_or_else(|| "Remote cancellation was requested.".into())
                    ))
                }
            };
            let event = match outcome {
                Ok(output) => ToolEvent::Completed {
                    id,
                    name: "Task".into(),
                    output,
                    success: true,
                    duration: started_at.elapsed(),
                },
                Err(error) => ToolEvent::Failed {
                    id,
                    name: "Task".into(),
                    error: error.to_string(),
                    duration: started_at.elapsed(),
                },
            };
            let _ = tool_tx.send(event).await;
        });
        self.running_tool_tasks.insert(tool_call_id, task);
    }
}

type ChildRequest = (String, String, String, u64, Option<String>);
fn parse_child_request(args: &serde_json::Value) -> anyhow::Result<ChildRequest> {
    let object = args
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Task requires an object."))?;
    for key in object.keys() {
        if ![
            "prompt",
            "task",
            "description",
            "agent",
            "subagent_type",
            "context",
            "timeout_secs",
            "model",
        ]
        .contains(&key.as_str())
        {
            anyhow::bail!(
                "Task option '{key}' is unsupported by the Code delegation contract. No task was started."
            );
        }
    }
    let mut prompt = args
        .get("prompt")
        .or_else(|| args.get("task"))
        .and_then(|v| v.as_str())
        .filter(|p| !p.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Task requires a nonempty prompt or task."))?
        .to_string();
    if let Some(context) = args.get("context") {
        let context = context
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Task context must be text."))?;
        prompt.push_str(&format!("\n\nAssigned context:\n{context}"));
    }
    for key in ["subagent_type", "agent", "description"] {
        if args.get(key).is_some_and(|value| !value.is_string()) {
            anyhow::bail!("Task {key} must be text.");
        }
    }
    let role = args
        .get("subagent_type")
        .or_else(|| args.get("agent"))
        .and_then(|v| v.as_str())
        .unwrap_or("code")
        .to_string();
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("Delegated task")
        .to_string();
    let timeout = match args.get("timeout_secs") {
        None => 300,
        Some(value) => value
            .as_u64()
            .filter(|v| (1..=600).contains(v))
            .ok_or_else(|| anyhow::anyhow!("Task timeout_secs must be between 1 and 600."))?,
    };
    let model = args
        .get("model")
        .map(|value| {
            value
                .as_str()
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("Task model must be nonempty text."))
        })
        .transpose()?;
    Ok((prompt, description, role, timeout, model))
}

async fn collect_child_turn(
    client: &dyn ModelClient,
    request: CompletionRequest,
) -> anyhow::Result<String> {
    let mut stream = client
        .complete(request)
        .await
        .map_err(|error| anyhow::anyhow!(error.user_friendly_message()))?;
    let mut text = String::new();
    let mut observations = std::collections::HashSet::new();
    while let Some(event) = stream.next().await {
        match event.map_err(|error| anyhow::anyhow!(error.user_friendly_message()))? {
            ResponseEvent::Delta(delta) => text.push_str(&delta),
            ResponseEvent::ToolCall(call) => {
                if !call.remote {
                    anyhow::bail!(LOCAL_TOOLS_UNSUPPORTED);
                }
                observations.insert(call.id);
            }
            ResponseEvent::ToolResult { id, .. } => {
                if !observations.remove(&id) {
                    anyhow::bail!(INCOMPLETE_STREAM);
                }
            }
            ResponseEvent::Done(response) => {
                response.finish_reason.require_success()?;
                if !observations.is_empty() {
                    anyhow::bail!(INCOMPLETE_STREAM);
                }
                return Ok(text);
            }
            ResponseEvent::Error(error) => anyhow::bail!("{error}"),
            ResponseEvent::Reasoning(_) => {}
        }
    }
    anyhow::bail!(INCOMPLETE_STREAM)
}

#[cfg(test)]
#[path = "runtime_contract_subagent_tests.rs"]
mod runtime_contract_tests;
