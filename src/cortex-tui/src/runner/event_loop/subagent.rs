//! Subagent spawning and event handling.

use std::time::{Duration, Instant};

use crate::app::SubagentTaskDisplay;
use crate::events::ToolEvent;

use cortex_engine::client::{Message, ResponseEvent, ToolDefinition as ClientToolDefinition};
use tokio_stream::StreamExt;

use super::core::{EventLoop, simplify_error_message};

impl EventLoop {
    /// Spawns a subagent task (for Task tool).
    /// Handles agent spawning, progress tracking, and result collection.
    pub(super) fn spawn_subagent(&mut self, tool_call_id: String, args: serde_json::Value) {
        tracing::info!("Spawning subagent for tool call: {}", tool_call_id);

        // Support both API format and internal format
        let agent = args.get("agent").and_then(|v| v.as_str());
        let task = args.get("task").and_then(|v| v.as_str());
        let context = args.get("context").and_then(|v| v.as_str());

        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .or(agent)
            .unwrap_or("Subagent task")
            .to_string();

        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .or(task)
            .map(|p| {
                if let Some(ctx) = context {
                    format!("{}\n\nContext: {}", p, ctx)
                } else {
                    p.to_string()
                }
            })
            .unwrap_or_default();

        let subagent_type = args
            .get("subagent_type")
            .and_then(|v| v.as_str())
            .or(agent)
            .unwrap_or("code")
            .to_string();

        if prompt.is_empty() {
            self.app_state.add_pending_tool_result(
                tool_call_id,
                "Task".to_string(),
                "Task tool requires a 'task' or 'prompt' parameter with instructions for the subagent.".to_string(),
                false,
            );
            return;
        }

        // Get dependencies for the spawned task
        let Some(registry) = self.tool_registry.clone() else {
            self.app_state.add_pending_tool_result(
                tool_call_id,
                "Task".to_string(),
                "Tool registry not available for subagent.".to_string(),
                false,
            );
            return;
        };

        let Some(provider_manager) = self.provider_manager.clone() else {
            self.app_state.add_pending_tool_result(
                tool_call_id,
                "Task".to_string(),
                "Provider not configured for subagent.".to_string(),
                false,
            );
            return;
        };

        let tool_tx = self.tool_event_tx.clone();
        let id = tool_call_id.clone();

        // Add to UI display
        self.app_state.add_subagent_task(SubagentTaskDisplay::new(
            format!("subagent_{}", id),
            id.clone(),
            description.clone(),
            subagent_type.clone(),
        ));

        // Mark that we're in delegation mode for UI status indicator
        self.app_state.streaming.start_delegation();

        // Spawn background task with full agentic loop
        let task = tokio::spawn(async move {
            let started_at = Instant::now();

            // Send started event
            if let Err(e) = tool_tx
                .send(ToolEvent::Started {
                    id: id.clone(),
                    name: "Task".to_string(),
                    started_at,
                })
                .await
            {
                tracing::error!(
                    "Failed to send ToolEvent::Started for subagent {}: {:?}",
                    id,
                    e
                );
                return;
            }

            // Build subagent system prompt
            let system_prompt = format!(
                "You are a specialized {} subagent working on: {}\n\n\
                 You have access to tools like Read, Edit, Grep, Glob, LS, Execute, Batch, TodoWrite, etc.\n\
                 Note: You cannot use the Task tool (no nested delegation).\n\n\
                 IMPORTANT - Todo List:\n\
                 - For any multi-step task, IMMEDIATELY use TodoWrite to create a todo list\n\
                 - Update the todo list as you progress (mark items in_progress or completed)\n\
                 - This provides real-time visibility to the user\n\
                 - Keep only ONE item as in_progress at a time\n\n\
                 Use Batch to execute multiple tools in parallel for efficiency.\n\
                 If a tool fails, try an alternative approach instead of giving up.\n\
                 Complete the task and provide a clear summary when done.",
                subagent_type, description
            );

            // Build initial messages for subagent
            let mut messages = vec![Message::system(system_prompt), Message::user(&prompt)];

            // Get tool definitions - filter based on subagent permissions
            let tools: Vec<ClientToolDefinition> = registry
                .get_definitions()
                .into_iter()
                .filter(|t| {
                    let name_lower = t.name.to_lowercase();
                    name_lower != "task"
                })
                .map(|t| ClientToolDefinition::function(t.name, t.description, t.parameters))
                .collect();

            // Get model info
            let model = {
                let pm = provider_manager.read().await;
                pm.current_model().to_string()
            };

            let mut final_content = String::new();
            let mut tool_calls_executed: Vec<String> = Vec::new();
            let max_iterations = 500;

            // Agentic loop - continues until no more tool calls
            for iteration in 0..max_iterations {
                tracing::info!("Subagent iteration {}", iteration + 1);

                // Get fresh client for each iteration
                let client = {
                    let mut pm = provider_manager.write().await;
                    if let Err(e) = pm.ensure_client() {
                        let error_msg = format!("Failed to initialize provider: {}", e);
                        tracing::error!("Subagent {}: {}", id, error_msg);
                        if let Err(send_err) = tool_tx
                            .send(ToolEvent::Failed {
                                id: id.clone(),
                                name: "Task".to_string(),
                                error: error_msg,
                                duration: started_at.elapsed(),
                            })
                            .await
                        {
                            tracing::error!("Failed to send ToolEvent::Failed: {:?}", send_err);
                        }
                        return;
                    }
                    pm.take_client()
                };

                let Some(client) = client else {
                    tracing::error!("Subagent {}: No client available", id);
                    if let Err(e) = tool_tx
                        .send(ToolEvent::Failed {
                            id: id.clone(),
                            name: "Task".to_string(),
                            error: "No client available".to_string(),
                            duration: started_at.elapsed(),
                        })
                        .await
                    {
                        tracing::error!("Failed to send ToolEvent::Failed: {:?}", e);
                    }
                    return;
                };

                // Make LLM request
                let request = cortex_engine::client::CompletionRequest {
                    messages: messages.clone(),
                    model: model.clone(),
                    max_tokens: Some(8192),
                    temperature: Some(0.7),
                    seed: None,
                    tools: tools.clone(),
                    stream: true,
                };

                let stream_result =
                    tokio::time::timeout(Duration::from_secs(120), client.complete(request)).await;

                let mut stream = match stream_result {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => {
                        let error_msg = simplify_error_message(&e.to_string());
                        tracing::error!("Subagent {} LLM request failed: {}", id, error_msg);
                        if let Err(send_err) = tool_tx
                            .send(ToolEvent::Failed {
                                id: id.clone(),
                                name: "Task".to_string(),
                                error: error_msg,
                                duration: started_at.elapsed(),
                            })
                            .await
                        {
                            tracing::error!("Failed to send ToolEvent::Failed: {:?}", send_err);
                        }
                        return;
                    }
                    Err(_) => {
                        tracing::error!("Subagent {} connection timeout (120s)", id);
                        if let Err(e) = tool_tx
                            .send(ToolEvent::Failed {
                                id: id.clone(),
                                name: "Task".to_string(),
                                error: "Connection timeout".to_string(),
                                duration: started_at.elapsed(),
                            })
                            .await
                        {
                            tracing::error!("Failed to send ToolEvent::Failed: {:?}", e);
                        }
                        return;
                    }
                };

                // Collect response from this iteration
                let mut iteration_content = String::new();
                let mut iteration_tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();

                loop {
                    let event = tokio::time::timeout(Duration::from_secs(60), stream.next()).await;

                    match event {
                        Ok(Some(Ok(ResponseEvent::Delta(delta)))) => {
                            iteration_content.push_str(&delta);
                        }
                        Ok(Some(Ok(ResponseEvent::Done(_)))) => {
                            break;
                        }
                        Ok(Some(Ok(ResponseEvent::ToolCall(tc)))) => {
                            let args: serde_json::Value = serde_json::from_str(&tc.arguments)
                                .unwrap_or(serde_json::json!({}));
                            iteration_tool_calls.push((tc.id, tc.name, args));
                        }
                        Ok(Some(Ok(ResponseEvent::Error(e)))) => {
                            tracing::error!("Subagent {} received error from LLM: {}", id, e);
                            if let Err(send_err) = tool_tx
                                .send(ToolEvent::Failed {
                                    id: id.clone(),
                                    name: "Task".to_string(),
                                    error: e,
                                    duration: started_at.elapsed(),
                                })
                                .await
                            {
                                tracing::error!("Failed to send ToolEvent::Failed: {:?}", send_err);
                            }
                            return;
                        }
                        Ok(Some(Err(e))) => {
                            tracing::error!("Subagent {} stream error: {}", id, e);
                            if let Err(send_err) = tool_tx
                                .send(ToolEvent::Failed {
                                    id: id.clone(),
                                    name: "Task".to_string(),
                                    error: e.to_string(),
                                    duration: started_at.elapsed(),
                                })
                                .await
                            {
                                tracing::error!("Failed to send ToolEvent::Failed: {:?}", send_err);
                            }
                            return;
                        }
                        Ok(None) => {
                            tracing::warn!(
                                "Subagent {} stream ended without Done event at iteration {}",
                                id,
                                iteration + 1
                            );
                            break;
                        }
                        Err(_) => {
                            tracing::error!(
                                "Subagent {} response timeout at iteration {}",
                                id,
                                iteration + 1
                            );
                            if let Err(e) = tool_tx
                                .send(ToolEvent::Failed {
                                    id: id.clone(),
                                    name: "Task".to_string(),
                                    error: "The provider appears to be overloaded or your internet connection/proxy is experiencing issues communicating with it.".to_string(),
                                    duration: started_at.elapsed(),
                                })
                                .await
                            {
                                tracing::error!("Failed to send ToolEvent::Failed: {:?}", e);
                            }
                            return;
                        }
                        _ => {}
                    }
                }

                // If no tool calls, we're done
                if iteration_tool_calls.is_empty() {
                    if !tool_calls_executed.is_empty() && iteration_content.trim().is_empty() {
                        // Request explicit summary if LLM didn't provide one
                        final_content = format!(
                            "Task completed with {} tool call(s).",
                            tool_calls_executed.len()
                        );
                    } else {
                        final_content = iteration_content;
                    }
                    break;
                }

                // Execute tool calls
                let mut tool_results: Vec<(String, String)> = Vec::new();
                let tool_calls_for_msg: Vec<cortex_engine::client::ToolCall> = iteration_tool_calls
                    .iter()
                    .map(
                        |(tc_id, tc_name, tc_args)| cortex_engine::client::ToolCall {
                            id: tc_id.clone(),
                            call_type: "function".to_string(),
                            function: cortex_engine::client::FunctionCall {
                                name: tc_name.clone(),
                                arguments: tc_args.to_string(),
                            },
                        },
                    )
                    .collect();

                const MAX_TOOL_OUTPUT_SIZE: usize = 32_000;

                for (tc_id, tc_name, tc_args) in &iteration_tool_calls {
                    tracing::info!("Subagent executing tool: {} ({})", tc_name, tc_id);

                    // Handle TodoWrite for progress tracking
                    if tc_name == "TodoWrite"
                        && let Some(todos_arr) = tc_args.get("todos").and_then(|v| v.as_array())
                    {
                        let todos: Vec<(String, String)> = todos_arr
                            .iter()
                            .filter_map(|t| {
                                let content = t.get("content").and_then(|v| v.as_str())?;
                                let status = t
                                    .get("status")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("pending");
                                Some((content.to_string(), status.to_string()))
                            })
                            .collect();

                        if !todos.is_empty()
                            && let Err(e) = tool_tx
                                .send(ToolEvent::TodoUpdated {
                                    session_id: format!("subagent_{}", id),
                                    todos,
                                })
                                .await
                        {
                            tracing::warn!("Failed to send TodoUpdated event: {:?}", e);
                        }
                    }

                    let result = registry.execute(tc_name, tc_args.clone()).await;
                    match result {
                        Ok(tool_result) => {
                            let status = if tool_result.success {
                                "success"
                            } else {
                                "failed"
                            };
                            tool_calls_executed.push(format!("{}: {}", tc_name, status));

                            let output = if tool_result.output.len() > MAX_TOOL_OUTPUT_SIZE {
                                let truncated = &tool_result.output[..MAX_TOOL_OUTPUT_SIZE];
                                format!(
                                    "{}...\n\n[Output truncated: {} bytes total, showing first {} bytes]",
                                    truncated,
                                    tool_result.output.len(),
                                    MAX_TOOL_OUTPUT_SIZE
                                )
                            } else {
                                tool_result.output
                            };
                            tool_results.push((tc_id.clone(), output));
                        }
                        Err(e) => {
                            let error_msg = format!("Error executing {}: {}", tc_name, e);
                            tool_calls_executed.push(format!("{}: error", tc_name));
                            tool_results.push((tc_id.clone(), error_msg));
                        }
                    }
                }

                // Add assistant message with tool calls to conversation
                let assistant_msg = Message {
                    role: cortex_engine::client::MessageRole::Assistant,
                    content: cortex_engine::client::MessageContent::Text(iteration_content.clone()),
                    tool_call_id: None,
                    tool_calls: Some(tool_calls_for_msg),
                };
                messages.push(assistant_msg);

                // Add tool results to conversation
                for (tc_id, output) in tool_results {
                    messages.push(Message::tool_result(&tc_id, &output));
                }

                // Store content for final output
                if !iteration_content.is_empty() {
                    final_content = iteration_content;
                }
            }

            // Build output with metadata
            let tools_summary = if tool_calls_executed.is_empty() {
                "No tools executed".to_string()
            } else {
                tool_calls_executed.join("\n")
            };

            // Handle case where LLM produced no text output
            let effective_content = if final_content.trim().is_empty() {
                if tool_calls_executed.is_empty() {
                    format!(
                        "The {} subagent completed but produced no output or tool calls. \
                         This may indicate an issue with the task or model response.",
                        subagent_type
                    )
                } else {
                    let success_count = tool_calls_executed
                        .iter()
                        .filter(|s| s.contains("success"))
                        .count();
                    let error_count = tool_calls_executed
                        .iter()
                        .filter(|s| s.contains("error") || s.contains("failed"))
                        .count();
                    format!(
                        "The {} subagent completed {} tool call(s) ({} successful, {} failed) \
                         but did not provide a textual summary. Task: {}",
                        subagent_type,
                        tool_calls_executed.len(),
                        success_count,
                        error_count,
                        description
                    )
                }
            } else {
                final_content
            };

            let output = format!(
                "{}\n\n\
                 Tools executed:\n{}\n\n\
                 <task_metadata>\n\
                 session_id: subagent_{}\n\
                 agent_type: {}\n\
                 description: {}\n\
                 </task_metadata>",
                effective_content, tools_summary, id, subagent_type, description
            );

            let duration = started_at.elapsed();

            if let Err(e) = tool_tx
                .send(ToolEvent::Completed {
                    id: id.clone(),
                    name: "Task".to_string(),
                    output: output.clone(),
                    success: true,
                    duration,
                })
                .await
            {
                tracing::error!(
                    "CRITICAL: Failed to send ToolEvent::Completed for subagent {}: {:?}. Output was: {}",
                    id,
                    e,
                    &output[..output.len().min(500)]
                );
            }
        });

        self.running_tool_tasks.insert(tool_call_id, task);
    }
}
