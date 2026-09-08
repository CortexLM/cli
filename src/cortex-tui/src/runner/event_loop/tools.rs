//! Tool execution handling: spawning, events, and completion.

use std::time::{Duration, Instant};

use crate::events::ToolEvent;
use crate::views::tool_call::format_result_summary;

use super::core::EventLoop;

impl EventLoop {
    /// Spawns a tool execution task in the background.
    pub(super) fn spawn_tool_execution(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
    ) {
        tracing::info!("Spawning tool execution: {} ({})", tool_name, tool_call_id);

        // Get tool registry
        let Some(registry) = self.tool_registry.clone() else {
            tracing::warn!(
                "Tool registry not initialized, cannot execute: {}",
                tool_name
            );
            self.app_state.add_pending_tool_result(
                tool_call_id,
                tool_name,
                "Tool registry not initialized. This is a configuration error.".to_string(),
                false,
            );
            return;
        };

        let tool_tx = self.tool_event_tx.clone();
        let id = tool_call_id.clone();
        let name = tool_name.clone();
        let mcp_manager = self.mcp_manager.clone();
        // Every caller reaches here only after the permission manager passed
        // the call or the user approved it in the modal.
        let context = self.tool_context(&tool_call_id, &tool_name, &args, true);

        // Spawn background task for tool execution
        let task = tokio::spawn(async move {
            let started_at = Instant::now();

            // Send started event
            let _ = tool_tx
                .send(ToolEvent::Started {
                    id: id.clone(),
                    name: name.clone(),
                    started_at,
                })
                .await;

            let result = if cortex_engine::mcp::parse_qualified_name(&name).is_some() {
                match mcp_manager.call_tool(&name, Some(args)).await {
                    Ok(call) => {
                        let text = call
                            .content
                            .iter()
                            .filter_map(|c| c.as_text())
                            .collect::<Vec<_>>()
                            .join("\n");
                        if call.is_error() {
                            Err(cortex_engine::CortexError::mcp(name.clone(), text))
                        } else {
                            Ok(cortex_engine::tools::ToolResult::success(text))
                        }
                    }
                    Err(e) => Err(cortex_engine::CortexError::mcp(name.clone(), e.to_string())),
                }
            } else {
                registry.execute_with_context(&name, args, context).await
            };
            let duration = started_at.elapsed();

            match result {
                Ok(tool_result) => {
                    let _ = tool_tx
                        .send(ToolEvent::Completed {
                            id,
                            name,
                            output: tool_result.output,
                            success: tool_result.success,
                            duration,
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tool_tx
                        .send(ToolEvent::Failed {
                            id,
                            name,
                            error: e.user_friendly_message(),
                            duration,
                        })
                        .await;
                }
            }
        });

        self.running_tool_tasks.insert(tool_call_id, task);
    }

    /// Spawns a unified tool execution (for Task/Batch tools).
    /// Routes Task to spawn_subagent, Batch to parallel execution.
    pub(super) fn spawn_unified_tool_execution(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
    ) {
        tracing::info!(
            "Spawning unified tool execution: {} ({})",
            tool_name,
            tool_call_id
        );

        // Route Task tool to spawn_subagent
        if tool_name == "Task" || tool_name == "task" {
            self.spawn_subagent(tool_call_id, args);
            return;
        }

        let batch = match parse_batch_request(args) {
            Ok(batch) => batch,
            Err(error) => {
                self.app_state.add_pending_tool_result(
                    tool_call_id,
                    tool_name,
                    error.to_string(),
                    false,
                );
                return;
            }
        };
        for call in &batch.calls {
            if self.permission_manager.should_ask(&call.tool) {
                self.app_state.add_pending_tool_result(tool_call_id, tool_name,
                    format!("Batch contains '{}' which requires individual approval. No batch tools were run.", call.tool), false);
                return;
            }
        }
        let Some(registry) = self.tool_registry.clone() else {
            self.app_state.add_pending_tool_result(
                tool_call_id,
                tool_name,
                "Tool registry not available for Batch.".into(),
                false,
            );
            return;
        };
        let tool_tx = self.tool_event_tx.clone();
        let id = tool_call_id.clone();
        // Each child passed the permission screen above, so each gets an exact
        // single-use approval for its own call rather than inheriting the parent's.
        let parent = self.tool_context(&tool_call_id, &tool_name, &serde_json::json!({}), false);
        let task = tokio::spawn(async move {
            let started_at = Instant::now();
            let _ = tool_tx
                .send(ToolEvent::Started {
                    id: id.clone(),
                    name: "Batch".into(),
                    started_at,
                })
                .await;
            // join_all owns the futures: dropping this task drops every child,
            // unlike detached JoinHandles that can keep mutating the workspace.
            let results = execute_batch_calls(batch, &id, |name, args| {
                let registry = registry.clone();
                let context = parent
                    .clone()
                    .for_child()
                    .with_approved_tool_call(&name, &args);
                async move { registry.execute_with_context(&name, args, context).await }
            })
            .await;
            let success = results.iter().all(|result| result["success"] == true);
            let output = serde_json::json!({"results": results, "success": success}).to_string();
            let _ = tool_tx
                .send(ToolEvent::Completed {
                    id,
                    name: "Batch".into(),
                    output,
                    success,
                    duration: started_at.elapsed(),
                })
                .await;
        });

        self.running_tool_tasks.insert(tool_call_id, task);
    }

    /// Handles events from background tool execution tasks.
    pub(super) async fn handle_tool_event(&mut self, event: ToolEvent) {
        use crate::app::SubagentDisplayStatus;

        match event {
            ToolEvent::Started {
                id,
                name,
                started_at: _,
            } => {
                tracing::debug!("Tool execution started: {} ({})", name, id);
                self.stream_controller
                    .set_executing_tool(Some(name.clone()));

                // If this is a Task tool, update the subagent to "Thinking" status
                if name == "Task" || name == "task" {
                    let session_id = format!("subagent_{}", id);
                    self.app_state.update_subagent(&session_id, |task| {
                        task.status = SubagentDisplayStatus::Thinking;
                        task.current_activity = "Processing request...".to_string();
                    });
                }
            }

            ToolEvent::Output { id, chunk } => {
                // Append output to the tool call's live output buffer
                for line in chunk.lines() {
                    if !line.is_empty() {
                        self.app_state.append_tool_output(&id, line.to_string());
                    }
                }
            }

            ToolEvent::Completed {
                id,
                name,
                output,
                success,
                duration,
            } => {
                self.handle_tool_completed(id, name, output, success, duration)
                    .await;
            }

            ToolEvent::Failed {
                id,
                name,
                error,
                duration,
            } => {
                self.handle_tool_failed(id, name, error, duration).await;
            }

            ToolEvent::TodoUpdated { session_id, todos } => {
                self.handle_todo_updated(session_id, todos);
            }

            ToolEvent::AgentGenerated {
                name,
                path,
                location,
            } => {
                tracing::info!(
                    "Agent generated: {} (location: {}, path: {})",
                    name,
                    location,
                    path
                );

                self.app_state
                    .toasts
                    .success(format!("Agent @{} created!", name));

                self.inject_agent_created_event(&name);

                let cwd = std::env::current_dir().ok();
                let terminal_height = self.app_state.terminal_size.1;
                let interactive = crate::interactive::builders::build_agents_selector(
                    cwd.as_deref(),
                    Some(terminal_height),
                );
                self.app_state.enter_interactive_mode(interactive);
            }

            ToolEvent::AgentGenerationFailed { error } => {
                tracing::error!("Agent generation failed: {}", error);
                self.app_state.toasts.error(&error);
                self.app_state.exit_interactive_mode();
            }
        }
    }

    /// Handle tool completion event
    async fn handle_tool_completed(
        &mut self,
        id: String,
        name: String,
        output: String,
        success: bool,
        duration: Duration,
    ) {
        // Handle login events specially
        if name == "login" {
            if id == "login_init" && success {
                self.handle_login_init_success(&output).await;
                return;
            } else if id == "login_poll" && success && output == "login:success" {
                self.handle_login_poll_success().await;
                return;
            } else if id == "login" && success {
                self.handle_legacy_login_success(&output).await;
                return;
            }
        }

        // Handle billing events specially
        if name == "billing"
            && id == "billing"
            && success
            && let Some(data_str) = output.strip_prefix("billing:data:")
        {
            self.handle_billing_data(data_str);
            return;
        }

        if name == "usage" && success {
            self.add_system_message(&output);
            return;
        }

        tracing::debug!(
            "Tool execution completed: {} ({}) in {:?}",
            name,
            id,
            duration
        );

        // Clear tool execution state
        self.tool_execution_started = false;
        self.stream_controller.set_executing_tool(None);
        self.app_state.streaming.stop_tool_execution();

        // Update tool result in UI
        let summary = format_result_summary(&name, &output, success);
        self.app_state
            .update_tool_result(&id, output.clone(), success, summary.clone());

        // If this is a Task tool, remove the subagent from display
        if name == "Task" || name == "task" {
            let session_id = format!("subagent_{}", id);
            self.app_state.remove_subagent(&session_id);
            if !self.app_state.has_active_subagents() {
                self.app_state.streaming.stop_delegation();
            }
        }

        if name == "UpdateGoal" {
            self.reload_goal_from_session();
        }

        // Store for agentic continuation
        self.app_state
            .add_pending_tool_result(id.clone(), name.clone(), output, success);

        // Remove from running tasks
        self.running_tool_tasks.remove(&id);

        // Continue agentic loop if no more tools are running
        tracing::info!(
            running_tools = self.running_tool_tasks.len(),
            running_subagents = self.running_subagents.len(),
            has_pending_results = self.app_state.has_pending_tool_results(),
            stream_done = self.stream_done_received,
            "ToolEvent::Completed - checking continuation state"
        );

        if self.stream_done_received
            && self.running_tool_tasks.is_empty()
            && self.running_subagents.is_empty()
        {
            if self.app_state.has_pending_tool_results() {
                tracing::info!("Calling continue_with_tool_results from ToolEvent::Completed");
                let _ = self.continue_with_tool_results().await;
            } else if self.app_state.has_queued_messages() {
                tracing::info!("Processing message queue after tool completion");
                let _ = self.process_message_queue().await;
            } else if self.maybe_continue_goal(0).await {
                tracing::info!("Continuing durable goal after tools");
            } else {
                tracing::info!("All tools done, full resetting streaming state");
                self.app_state.streaming.full_reset();
            }
        } else {
            tracing::info!("More tools still running after this completion");
        }
    }

    /// Handle tool failure event
    async fn handle_tool_failed(
        &mut self,
        id: String,
        name: String,
        error: String,
        duration: Duration,
    ) {
        use crate::app::SubagentDisplayStatus;

        // Handle login errors specially
        if name == "login" && (id == "login_init" || id == "login_poll") {
            let error_msg = if let Some(msg) = error.strip_prefix("login:error:") {
                msg.to_string()
            } else if error == "login:expired" {
                "Code expired. Please try again.".to_string()
            } else if error == "login:denied" {
                "Access denied".to_string()
            } else if error == "login:timeout" {
                "Login timed out".to_string()
            } else {
                error.clone()
            };
            self.app_state.login_flow = None;
            self.app_state.exit_interactive_mode();
            self.app_state.toasts.error(&error_msg);
            return;
        }

        // Handle billing errors specially
        if name == "billing" && id == "billing" {
            self.handle_billing_error(&error);
            return;
        }

        if name == "usage" {
            let msg = if error == "usage:not_logged_in" {
                "Run /login to see usage."
            } else if let Some(rest) = error.strip_prefix("usage:error:") {
                rest
            } else {
                error.as_str()
            };
            self.add_system_message(msg);
            return;
        }

        tracing::error!(
            "Tool execution failed: {} ({}) after {:?} - {}",
            name,
            id,
            duration,
            error
        );

        // Clear tool execution state
        self.tool_execution_started = false;
        self.stream_controller.set_executing_tool(None);
        self.app_state.streaming.stop_tool_execution();

        self.app_state
            .update_tool_result(&id, error.clone(), false, format!("Error: {}", error));

        if error.contains("Sandbox denied") {
            self.add_system_message(&format!(
                "{} {}",
                crate::ui::consts::STOPPED_MARK,
                crate::ui::consts::SANDBOX_DENIED_TITLE
            ));
            self.add_system_message(&error);
        }

        // If this is a Task tool, update subagent status to Failed
        if name == "Task" || name == "task" {
            let session_id = format!("subagent_{}", id);
            let error_clone = error.clone();
            self.app_state.update_subagent(&session_id, |task| {
                task.status = SubagentDisplayStatus::Failed;
                task.error_message = Some(error_clone);
                task.current_activity = "Failed".to_string();
            });
            let has_running_subagents = self
                .app_state
                .active_subagents
                .iter()
                .any(|t| !t.status.is_terminal());
            if !has_running_subagents {
                self.app_state.streaming.stop_delegation();
            }
        }

        self.app_state
            .add_pending_tool_result(id.clone(), name, error, false);

        self.running_tool_tasks.remove(&id);

        // Continue agentic loop if no more tools are running
        if self.stream_done_received
            && self.running_tool_tasks.is_empty()
            && self.running_subagents.is_empty()
        {
            if self.app_state.has_pending_tool_results() {
                let _ = self.continue_with_tool_results().await;
            } else if self.app_state.has_queued_messages() {
                let _ = self.process_message_queue().await;
            } else {
                self.app_state.streaming.full_reset();
            }
        }
    }

    /// Handle todo updated event
    fn handle_todo_updated(&mut self, session_id: String, todos: Vec<(String, String)>) {
        use crate::app::{SubagentTodoItem, SubagentTodoStatus};

        tracing::debug!(
            "Subagent todo list updated: {} ({} items)",
            session_id,
            todos.len()
        );

        self.app_state.update_subagent(&session_id, |task| {
            task.todos = todos
                .iter()
                .map(|(content, status)| {
                    let status = match status.as_str() {
                        "in_progress" => SubagentTodoStatus::InProgress,
                        "completed" => SubagentTodoStatus::Completed,
                        _ => SubagentTodoStatus::Pending,
                    };
                    SubagentTodoItem {
                        content: content.clone(),
                        status,
                    }
                })
                .collect();

            // Update activity based on in-progress item
            if let Some(in_progress) = task
                .todos
                .iter()
                .find(|t| matches!(t.status, SubagentTodoStatus::InProgress))
            {
                task.current_activity = in_progress.content.clone();
            }
        });
    }

    /// Checks for crashed background tool tasks (panics or cancelled).
    pub(super) async fn check_crashed_tasks(&mut self) {
        // Collect finished task IDs
        let finished_task_ids: Vec<String> = self
            .running_tool_tasks
            .iter()
            .filter(|(_, handle)| handle.is_finished())
            .map(|(id, _)| id.clone())
            .collect();

        // Process finished tasks - check if they actually crashed
        for id in finished_task_ids {
            if let Some(handle) = self.running_tool_tasks.remove(&id) {
                // Await the handle to check if it panicked or was cancelled
                match handle.await {
                    Ok(()) => {
                        // Task completed normally - it sent its own Completed/Failed event
                        tracing::debug!("Task {} finished normally, event pending in channel", id);
                    }
                    Err(join_error) => {
                        // Task actually crashed - send a failure event
                        let error_msg = if join_error.is_panic() {
                            format!("Subagent panicked: {:?}", join_error.into_panic())
                        } else if join_error.is_cancelled() {
                            "Subagent task was cancelled".to_string()
                        } else {
                            format!("Subagent task failed: {}", join_error)
                        };

                        tracing::error!("Detected crashed task: {} - {}", id, error_msg);

                        // Send failure event through the tool event channel
                        let _ = self
                            .tool_event_tx
                            .send(ToolEvent::Failed {
                                id: id.clone(),
                                name: "Task".to_string(),
                                error: error_msg,
                                duration: std::time::Duration::from_secs(0),
                            })
                            .await;
                    }
                }
            }
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeBatch {
    #[serde(alias = "tool_calls")]
    calls: Vec<RuntimeBatchCall>,
    timeout_secs: Option<u64>,
    tool_timeout_secs: Option<u64>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeBatchCall {
    tool: String,
    #[serde(alias = "parameters")]
    arguments: serde_json::Value,
}

fn parse_batch_request(args: serde_json::Value) -> anyhow::Result<RuntimeBatch> {
    let batch: RuntimeBatch = serde_json::from_value(args)
        .map_err(|_| anyhow::anyhow!("Batch requires calls[].tool and calls[].arguments."))?;
    if batch.calls.is_empty() || batch.calls.len() > 10 {
        anyhow::bail!("Batch requires between 1 and 10 calls.");
    }
    if batch.timeout_secs.is_some_and(|n| !(1..=600).contains(&n))
        || batch
            .tool_timeout_secs
            .is_some_and(|n| !(1..=300).contains(&n))
    {
        anyhow::bail!("Batch timeouts must be positive and within the advertised limits.");
    }
    for call in &batch.calls {
        if call.tool.trim().is_empty() || !call.arguments.is_object() {
            anyhow::bail!("Each Batch call needs a tool name and an arguments object.");
        }
        if ["batch", "task", "agent", "questions", "question"]
            .contains(&call.tool.to_ascii_lowercase().as_str())
        {
            anyhow::bail!(
                "Nested Batch, delegation, and interactive questions are not supported inside this Batch executor. No calls were run."
            );
        }
        if cortex_engine::mcp::parse_qualified_name(&call.tool).is_some() {
            anyhow::bail!(
                "MCP calls must run individually, not through the built-in Batch executor."
            );
        }
    }
    Ok(batch)
}

async fn execute_batch_calls<F, Fut>(
    batch: RuntimeBatch,
    id: &str,
    execute: F,
) -> Vec<serde_json::Value>
where
    F: Fn(String, serde_json::Value) -> Fut,
    Fut: std::future::Future<Output = cortex_engine::Result<cortex_engine::tools::ToolResult>>,
{
    let timeout = Duration::from_secs(
        batch
            .tool_timeout_secs
            .unwrap_or(60)
            .min(batch.timeout_secs.unwrap_or(300)),
    );
    let futures = batch.calls.into_iter().enumerate().map(|(index, call)| {
        let future = execute(call.tool.clone(), call.arguments);
        async move {
            let started = Instant::now();
            let result = tokio::time::timeout(timeout, future).await;
            let (success, output, error, timed_out, metadata) = match result {
                Ok(Ok(result)) => (result.success, result.output, None, false, result.metadata.map(|m| serde_json::json!({"duration_ms": m.duration_ms, "exit_code": m.exit_code, "files_modified": m.files_modified, "data": m.data}))),
                Ok(Err(error)) => (false, String::new(), Some(error.user_friendly_message()), false, None),
                Err(_) => (false, String::new(), Some("Tool deadline exceeded.".into()), true, None),
            };
            serde_json::json!({"id": format!("{id}/{index}"), "index":index, "tool":call.tool, "success":success,
                "output":output, "error":error, "timed_out":timed_out, "metadata":metadata, "duration_ms":started.elapsed().as_millis() as u64})
        }
    });
    futures::future::join_all(futures).await
}

#[cfg(test)]
#[path = "runtime_contract_tools_tests.rs"]
mod runtime_contract_tests;
