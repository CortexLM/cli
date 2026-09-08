//! Streaming event handling and provider communication.

use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use crate::agent::build_system_prompt;
use crate::app::{AppView, PendingToolResult};
use crate::question::{QuestionRequest, QuestionState};
use crate::session::StoredToolCall;
use crate::views::tool_call::ToolStatus;

use cortex_engine::client::{
    CodeTurnContext, CodeTurnMode, CompletionRequest, ComputerKind, Message, ResponseEvent,
    ToolDefinition as ClientToolDefinition,
};
use cortex_engine::streaming::StreamEvent;

use super::core::{EventLoop, PendingToolCall};

/// How a failed turn should be reported. Classification is pure so the
/// product-facing copy can be tested without a live stream.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum StreamErrorKind {
    Cancelled,
    AuthenticationRequired,
    InsufficientBalance,
    QuotaExhausted,
    ServiceUnavailable,
    Actionable,
}

pub(super) fn classify_stream_error(error: &str) -> StreamErrorKind {
    let lower = error.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| lower.contains(needle));

    if error.eq_ignore_ascii_case("cancelled")
        || error.eq_ignore_ascii_case("cancelled.")
        || lower.contains("cancelled by user")
    {
        return StreamErrorKind::Cancelled;
    }
    if has(&[
        "401",
        "unauthorized",
        "auth_required",
        "authentication required",
        "authentication failed",
    ]) {
        return StreamErrorKind::AuthenticationRequired;
    }
    if has(&[
        "402",
        "insufficient_balance",
        "insufficient token balance",
        "payment required",
    ]) {
        return StreamErrorKind::InsufficientBalance;
    }
    if has(&[
        "rate limit",
        "usage limit",
        "quota exceeded",
        "quota exhausted",
        "limit exceeded",
        "too many requests",
        "429",
    ]) {
        return StreamErrorKind::QuotaExhausted;
    }
    // Product-facing copy only - never raw provider or transport names.
    let actionable = has(&[
        "cortex login",
        "cortex_api",
        "not signed in",
        "not found",
        "sign in",
    ]);
    if !actionable
        && has(&[
            "unavailable",
            "connection",
            "timed out",
            "timeout",
            "dns",
            "reqwest",
            "hyper",
        ])
    {
        return StreamErrorKind::ServiceUnavailable;
    }
    StreamErrorKind::Actionable
}

/// Shared TUI + exec product rule: Cloud unless This PC/SSH is explicit.
fn tui_code_turn_context(plan_or_spec: bool) -> CodeTurnContext {
    CodeTurnContext {
        workspace: std::env::current_dir()
            .ok()
            .map(|p| p.display().to_string()),
        computer: ComputerKind::detect(),
        turn_mode: Some(if plan_or_spec {
            CodeTurnMode::Chat
        } else {
            CodeTurnMode::Code
        }),
        ssh_target: std::env::var("CORTEX_SSH_HOST")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::env::var("CORTEX_SSH_TARGET")
                    .ok()
                    .filter(|s| !s.is_empty())
            }),
    }
}

impl EventLoop {
    /// Handles message submission using the new provider system.
    ///
    /// This method is NON-BLOCKING. It:
    /// 1. Validates that a provider is available
    /// 2. Adds the user message to the session
    /// 3. Spawns a background task for streaming
    /// 4. Returns immediately so the UI stays responsive
    ///
    /// The streaming events are received in the main loop via `streaming_rx`.
    pub(super) async fn handle_submit_with_provider(&mut self, text: String) -> Result<()> {
        // Check provider exists
        if self.provider_manager.is_none() {
            // Switch to session view and show user message + error
            self.app_state.set_view(AppView::Session);
            let ui_message = cortex_core::widgets::Message::user(&text);
            self.app_state.add_message(ui_message);
            self.add_system_message(
                "No provider configured. Use /provider <name> to select one.\n\
                 Run /providers to list available providers.",
            );
            return Ok(());
        }

        // Check if Cortex authentication is configured
        let is_authenticated = if let Some(ref pm) = self.provider_manager {
            pm.read().await.is_available()
        } else {
            cortex_login::has_valid_auth()
        };

        if !is_authenticated {
            // Switch to session view and show user message
            self.app_state.set_view(AppView::Session);
            let ui_message = cortex_core::widgets::Message::user(&text);
            self.app_state.add_message(ui_message);

            // Queue the message so it can be auto-sent after API key configuration
            self.app_state.queue_message(text);
            tracing::info!(
                "Queued message for auto-send after authentication (queue size: {})",
                self.app_state.queued_count()
            );

            // Show authentication required toast notification
            self.app_state
                .toasts
                .error("Authentication required. Please run `cortex login` to authenticate.");
            return Ok(());
        }

        // Clear previous tool calls from display (new conversation turn)
        self.app_state.clear_tool_calls();

        let ui_message = cortex_core::widgets::Message::user(&text);
        self.app_state.add_message(ui_message);

        if let Some(ref mut session) = self.cortex_session {
            session.add_user_message(&text);
        }

        // Switch to session view
        self.app_state.set_view(AppView::Session);

        // Build system prompt
        let system_prompt = build_system_prompt();
        let system_message = Message::system(system_prompt);

        // Build messages for API
        let session_messages: Vec<Message> = if let Some(ref session) = self.cortex_session {
            session.messages_for_api()
        } else {
            vec![Message::user(&text)]
        };

        // Prepend system prompt to messages
        let mut messages: Vec<Message> = vec![system_message];
        messages.extend(session_messages);

        // Get tool definitions from registry and convert to client format
        let tools: Vec<ClientToolDefinition> = self
            .tool_registry
            .as_ref()
            .map(|r| r.get_definitions())
            .unwrap_or_default()
            .into_iter()
            .map(|t| ClientToolDefinition::function(t.name, t.description, t.parameters))
            .collect();

        // Start streaming UI state
        self.stream_controller.start_processing();
        self.app_state.start_streaming(None, true); // Reset timer for new user prompt

        // Reset cancellation flag and stream_done flag for new request
        self.streaming_cancelled.store(false, Ordering::SeqCst);
        self.stream_done_received = false;

        // Get provider manager for client operations
        let provider_manager = match &self.provider_manager {
            Some(pm) => pm.clone(),
            None => {
                self.add_system_message("No provider manager configured.");
                self.app_state.stop_streaming();
                return Ok(());
            }
        };

        // Get completion request parameters using read lock
        let (model, client) = {
            let mut pm = provider_manager.write().await;

            // Ensure client is created
            if let Err(e) = pm.ensure_client() {
                self.stream_controller.set_error(e.to_string());
                self.app_state.stop_streaming();
                self.add_system_message(&format!("Failed to initialize provider: {}", e));
                return Ok(());
            }

            let model = pm.current_model().to_string();
            let client = pm.snapshot_client();

            (model, client)
        };

        if client.is_none() {
            self.add_system_message("Failed to get provider client. Try again.");
            self.app_state.stop_streaming();
            return Ok(());
        }

        if let Some(ref c) = client {
            let plan_or_spec = self.app_state.is_plan_mode() || self.app_state.is_spec_mode();
            if plan_or_spec {
                cortex_engine::harness::enter_spec_mode();
            }
            c.configure_code_turn(tui_code_turn_context(plan_or_spec));
        }

        // Create channel for streaming events
        let (tx, rx) = mpsc::channel::<StreamEvent>(100);
        self.streaming_rx = Some(rx);

        // Clone what we need for the background task
        let cancelled = self.streaming_cancelled.clone();

        // Spawn background streaming task
        let request = CompletionRequest {
            messages,
            model,
            max_tokens: None,
            temperature: None,
            seed: None,
            tools,
            stream: true,
        };
        let task = tokio::spawn(forward_code_stream(client.unwrap(), request, cancelled, tx));

        self.streaming_task = Some(task);

        Ok(())
    }

    /// Handles a streaming event from the background task.
    pub(super) async fn handle_stream_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::Delta(delta) => {
                self.stream_controller.append_text(&delta);
                // Mark that we are now actively receiving tokens (transition from Execute to Streaming..)
                self.app_state.streaming.start_active_streaming();
                // Track text for interleaved display
                self.app_state.append_streaming_text(&delta);
                // Keep scroll at bottom if pinned (user hasn't scrolled up)
                if self.app_state.chat_scroll_pinned_bottom {
                    self.app_state.chat_scroll = 0;
                }
            }
            StreamEvent::Reasoning(r) => {
                // Could display reasoning in a separate area if desired
                tracing::debug!("Reasoning: {}", r);
            }
            StreamEvent::Done {
                content,
                reasoning,
                tokens,
            } => {
                self.handle_stream_done(content, reasoning, tokens).await;
            }
            StreamEvent::Error(e) => {
                self.handle_stream_error(e).await;
            }
            StreamEvent::ToolCall {
                id,
                name,
                arguments,
            } => {
                self.handle_stream_tool_call(id, name, arguments).await;
            }
            StreamEvent::ToolCallStart { id, name } => {
                let args = serde_json::json!({"remote": true});
                self.app_state.add_tool_call(id.clone(), name, args);
                self.app_state
                    .update_tool_status(&id, crate::views::tool_call::ToolStatus::Running);
            }
            StreamEvent::ToolCallComplete {
                id,
                success,
                output,
            } => {
                let summary = if success {
                    "↳ done".to_string()
                } else {
                    "↳ error".to_string()
                };
                if !output.is_empty() || !success {
                    self.app_state
                        .update_tool_result(&id, output, success, summary);
                } else {
                    self.app_state
                        .update_tool_status(&id, crate::views::tool_call::ToolStatus::Completed);
                }
            }
            _ => {
                // Other variants not specifically handled
            }
        }
    }

    /// Handle stream completion
    async fn handle_stream_done(
        &mut self,
        content: String,
        reasoning: String,
        tokens: Option<cortex_engine::streaming::StreamTokenUsage>,
    ) {
        self.stream_controller.complete();
        self.app_state.stop_streaming();

        // Flush any remaining text as a final segment
        self.app_state.flush_pending_text();

        // Take pending tool calls from this streaming response
        let tool_calls_for_message = std::mem::take(&mut self.pending_assistant_tool_calls);
        let has_tool_calls = !tool_calls_for_message.is_empty();

        // Add assistant message to UI (if there's content)
        if !content.is_empty() {
            let ui_message = cortex_core::widgets::Message::assistant(&content);
            self.app_state.add_message(ui_message);
        }

        // Add assistant message to session (always, if there's content OR tool calls)
        if (!content.is_empty() || has_tool_calls)
            && let Some(ref mut session) = self.cortex_session
        {
            // Create assistant message with tool calls
            let mut stored_msg = crate::session::StoredMessage::assistant(&content);

            // Add tool calls to the message
            for tc in &tool_calls_for_message {
                let tool_call = StoredToolCall::new(&tc.id, &tc.name, tc.arguments.clone());
                stored_msg = stored_msg.with_tool_call(tool_call);
            }

            // Add reasoning if present
            if !reasoning.is_empty() {
                stored_msg = stored_msg.with_reasoning(&reasoning);
            }

            // Add tokens
            if let Some(ref t) = tokens {
                stored_msg =
                    stored_msg.with_tokens(t.prompt_tokens as i64, t.completion_tokens as i64);
            }

            // Add to session via internal method
            session.add_message_raw(stored_msg);

            // Update token counts in metadata
            if let Some(ref t) = tokens {
                session.add_tokens(t.prompt_tokens as i64, t.completion_tokens as i64);
            }

            // Auto-generate title from first message
            if session.message_count() <= 2 {
                session.auto_title();
            }
        }

        let remote_id = if let Some(pm) = &self.provider_manager {
            let mut manager = pm.write().await;
            if let Some(client) = manager.snapshot_client() {
                let id = client.code_session_id().await;
                manager.restore_client(client);
                id
            } else {
                None
            }
        } else {
            None
        };
        if let Some(ref mut session) = self.cortex_session
            && let Some(id) = remote_id
        {
            session.meta.code_session_id = Some(id);
            if let Err(error) = session.save() {
                self.app_state
                    .toasts
                    .error(format!("Could not save Code session identity: {error}"));
            }
        }

        self.stream_controller.reset();
        self.streaming_rx = None;
        self.streaming_task = None;

        // Mark stream as done
        self.stream_done_received = true;

        // Reset continuation flag after stream completes
        self.is_continuation = false;

        // Check if we need to continue the agentic loop
        tracing::info!(
            running_tools = self.running_tool_tasks.len(),
            running_subagents = self.running_subagents.len(),
            has_pending_results = self.app_state.has_pending_tool_results(),
            "StreamEvent::Done - checking continuation state"
        );

        if self.running_tool_tasks.is_empty() && self.running_subagents.is_empty() {
            if self.app_state.has_pending_tool_results() {
                tracing::info!("Calling continue_with_tool_results from StreamEvent::Done");
                let _ = self.continue_with_tool_results().await;
            } else if self.app_state.has_queued_messages() {
                tracing::info!("Processing message queue");
                let _ = self.process_message_queue().await;
            } else {
                // No more work to do - full reset the prompt timer
                tracing::info!("Conversation turn complete, full resetting streaming state");
                self.app_state.streaming.full_reset();
            }
        } else {
            tracing::info!("Tools still running, will continue when they complete");
        }

        // Play completion sound notification
        crate::sound::play_response_complete(self.app_state.sound_enabled);
    }

    /// Handle stream error
    async fn handle_stream_error(&mut self, e: String) {
        self.stream_controller.set_error(e.clone());
        self.app_state.stop_streaming();
        self.streaming_cancelled.store(true, Ordering::SeqCst);
        self.stream_done_received = false;
        for (_, task) in self.running_tool_tasks.drain() {
            task.abort();
        }
        for (_, task) in self.running_subagents.drain() {
            task.abort();
        }
        self.pending_assistant_tool_calls.clear();
        self.app_state.pending_tool_results.clear();
        self.app_state.pending_approval = None;

        match classify_stream_error(&e) {
            StreamErrorKind::Cancelled => {
                self.mark_turn_stopped();
                self.stream_controller.reset();
                self.streaming_rx = None;
                self.streaming_task = None;
                return;
            }
            StreamErrorKind::AuthenticationRequired => {
                self.add_system_message(
                    "Session expired or authentication required.\n\n\
                     Opening login screen to re-authenticate...",
                );
                self.app_state
                    .toasts
                    .warning("Session expired. Please re-authenticate.");

                // Reset streaming state before starting login
                self.stream_controller.reset();
                self.streaming_rx = None;
                self.streaming_task = None;

                self.start_login_flow().await;
                return;
            }
            StreamErrorKind::InsufficientBalance => {
                self.add_system_message(
                    "Error: Insufficient token balance to continue the conversation.\n\n\
                     Please recharge your account at: https://app.cortex.foundation\n\n\
                     Once recharged, you can continue your conversation.",
                );
                self.app_state
                    .toasts
                    .error("Insufficient balance. Please recharge at app.cortex.foundation");
                self.stream_controller.reset();
                self.streaming_rx = None;
                self.streaming_task = None;
                return;
            }
            StreamErrorKind::QuotaExhausted => {
                self.app_state.quota_held = true;
                self.add_system_message(&format!("x {}", crate::ui::consts::QUOTA_EXHAUSTED));
                self.add_system_message(
                    "Your work so far is saved in this session. Switch to MAX token billing to continue now, or upgrade at cortex.foundation/billing",
                );
                self.app_state
                    .toasts
                    .warning("Agent quota exhausted. Follow-ups are held until it resets.");
            }
            StreamErrorKind::ServiceUnavailable => {
                self.add_system_message("The coding service is temporarily unavailable");
                self.add_system_message(crate::ui::consts::SERVICE_UNAVAILABLE_NEXT_STEP);
            }
            StreamErrorKind::Actionable => self.add_system_message(&e),
        }

        self.stream_controller.reset();
        self.streaming_rx = None;
        self.streaming_task = None;

        // Failed turns hold queued follow-ups; do not silently start more work.
    }

    /// Handle tool call from stream
    async fn handle_stream_tool_call(
        &mut self,
        id: String,
        name: String,
        arguments: serde_json::Value,
    ) {
        let remote = arguments
            .get("remote")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // First-class tool row (not a dump). Task uses SubagentTaskDisplay.
        if name != "Task" && name != "task" {
            self.app_state
                .add_tool_call(id.clone(), name.clone(), arguments.clone());
        }

        if remote {
            self.app_state
                .update_tool_status(&id, crate::views::tool_call::ToolStatus::Running);
            return;
        }

        // Store tool call for assistant message (to be added on StreamEvent::Done)
        self.pending_assistant_tool_calls.push(PendingToolCall {
            id: id.clone(),
            name: name.clone(),
            arguments: arguments.clone(),
        });

        // Special handling for Questions tool - show interactive TUI
        if (name == "Questions" || name == "question")
            && let Some(request) = QuestionRequest::from_tool_args(&id, &arguments)
        {
            let state = QuestionState::new(request);
            self.app_state.start_question_prompt(state);
            self.app_state.update_tool_status(&id, ToolStatus::Running);
            // Don't execute the tool yet - wait for user answers
            return;
        }

        // Special handling for Task and Batch tools - use UnifiedToolExecutor
        if name == "Task" || name == "task" || name == "Batch" || name == "batch" {
            if name == "Batch" || name == "batch" {
                self.app_state.update_tool_status(&id, ToolStatus::Running);
            }

            // Use UnifiedToolExecutor if available, otherwise fall back to old behavior
            if self.unified_executor.is_some() {
                self.spawn_unified_tool_execution(id, name, arguments);
            } else if name == "Task" || name == "task" {
                // Fall back to spawn_subagent for Task
                self.spawn_subagent(id, arguments);
            } else {
                // Batch without unified executor - return error
                self.app_state.add_pending_tool_result(
                    id,
                    name,
                    "Batch tool requires UnifiedToolExecutor to be configured. \
                     This enables parallel tool execution."
                        .to_string(),
                    false,
                );
            }
            return;
        }

        // Check if we need to ask for permission
        if self.permission_manager.should_ask(&name) {
            // Generate diff preview for Edit/Write tools
            let diff_preview = if name == "Edit" || name == "Write" || name == "ApplyPatch" {
                self.generate_diff_preview(&name, &arguments)
            } else {
                None
            };

            // Request approval with full tool call details
            self.app_state.request_tool_approval(
                id.clone(),
                name.clone(),
                arguments.clone(),
                diff_preview,
            );
            // The approval handler will execute the tool when approved
        } else {
            // Auto-approved - execute in background (non-blocking!)
            self.app_state.update_tool_status(&id, ToolStatus::Running);
            self.spawn_tool_execution(id, name, arguments);
        }
    }

    /// Continues with tool results after execution - sends results back to LLM.
    pub(super) async fn continue_with_tool_results(&mut self) -> Result<()> {
        // Check if there are pending tool results
        if !self.app_state.has_pending_tool_results() {
            return Ok(());
        }

        tracing::info!(
            "Continuing with {} pending tool results",
            self.app_state.pending_tool_results.len()
        );

        // Take pending results
        let pending_results = std::mem::take(&mut self.app_state.pending_tool_results);

        // Save tool results to session
        if let Some(ref mut session) = self.cortex_session {
            for result in &pending_results {
                let stored_msg = crate::session::StoredMessage::tool_result(
                    &result.tool_call_id,
                    &result.output,
                );
                session.add_message_raw(stored_msg);
            }
        }

        // Send tool results back to LLM to continue the conversation
        tracing::info!("About to call send_tool_results_to_llm");
        self.send_tool_results_to_llm(pending_results).await?;
        tracing::info!("send_tool_results_to_llm completed, streaming_rx is now set");

        Ok(())
    }

    /// Sends tool results to the LLM to continue the agentic loop.
    pub(super) async fn send_tool_results_to_llm(
        &mut self,
        results: Vec<PendingToolResult>,
    ) -> Result<()> {
        if results.is_empty() {
            return Ok(());
        }

        tracing::info!(
            "Sending {} tool results to LLM for continuation",
            results.len()
        );

        // Mark as continuation - tool calls should NOT be cleared
        self.is_continuation = true;

        // Check provider exists
        let provider_manager = match &self.provider_manager {
            Some(pm) => pm.clone(),
            None => {
                anyhow::bail!("No client is available for tool-result continuation.");
            }
        };

        // Build system prompt
        let system_prompt = build_system_prompt();
        let system_message = Message::system(system_prompt);

        // Build messages for API (includes tool results that were just added to session)
        let session_messages: Vec<Message> = if let Some(ref session) = self.cortex_session {
            session.messages_for_api()
        } else {
            anyhow::bail!("No session is available for tool-result continuation.");
        };

        // Prepend system prompt to messages
        let mut messages: Vec<Message> = vec![system_message];
        messages.extend(session_messages);

        // Get tool definitions from registry
        let tools: Vec<ClientToolDefinition> = self
            .tool_registry
            .as_ref()
            .map(|r| r.get_definitions())
            .unwrap_or_default()
            .into_iter()
            .map(|t| ClientToolDefinition::function(t.name, t.description, t.parameters))
            .collect();

        // Start streaming UI state
        self.stream_controller.start_processing();
        self.app_state.start_streaming(None, false); // Don't reset timer for tool continuation

        // Reset cancellation flag and stream_done flag
        self.streaming_cancelled.store(false, Ordering::SeqCst);
        self.stream_done_received = false;

        // Get completion request parameters
        let (model, client) = {
            let mut pm = provider_manager.write().await;

            if let Err(e) = pm.ensure_client() {
                self.stream_controller.set_error(e.to_string());
                self.app_state.stop_streaming();
                return Ok(());
            }

            let model = pm.current_model().to_string();
            let client = pm.snapshot_client();

            (model, client)
        };

        if client.is_none() {
            self.app_state.stop_streaming();
            return Ok(());
        }

        if let Some(ref c) = client {
            let plan_or_spec = self.app_state.is_plan_mode() || self.app_state.is_spec_mode();
            c.configure_code_turn(tui_code_turn_context(plan_or_spec));
        }

        // Create channel for streaming events
        let (tx, rx) = mpsc::channel::<StreamEvent>(100);
        self.streaming_rx = Some(rx);

        let cancelled = self.streaming_cancelled.clone();

        // Spawn background streaming task
        let request = CompletionRequest {
            messages,
            model,
            max_tokens: None,
            temperature: None,
            seed: None,
            tools,
            stream: true,
        };
        let task = tokio::spawn(forward_code_stream(client.unwrap(), request, cancelled, tx));

        self.streaming_task = Some(task);

        Ok(())
    }

    /// Processes the message queue (queued user messages).
    pub(super) async fn process_message_queue(&mut self) -> Result<()> {
        // Take first message from queue
        if let Some(message) = self.app_state.message_queue.pop_front() {
            tracing::debug!(
                "Processing queued message: {}",
                message.chars().take(50).collect::<String>()
            );
            self.handle_submit_with_provider(message).await?;
        }
        Ok(())
    }

    /// Generates a diff preview for Edit/Write tools.
    pub(super) fn generate_diff_preview(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Option<String> {
        match tool_name {
            "Edit" => {
                let file_path = arguments.get("file_path")?.as_str()?;
                let old_str = arguments.get("old_str")?.as_str()?;
                let new_str = arguments.get("new_str")?.as_str()?;

                let _current_content = std::fs::read_to_string(file_path).ok()?;

                let preview = format!(
                    "--- a/{}\n+++ b/{}\n@@ Edit Preview @@\n-{}\n+{}",
                    file_path,
                    file_path,
                    old_str
                        .lines()
                        .map(|l| format!("-{}", l))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    new_str
                        .lines()
                        .map(|l| format!("+{}", l))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );

                Some(preview)
            }
            "Write" => {
                let file_path = arguments.get("file_path")?.as_str()?;
                let content = arguments.get("content")?.as_str()?;

                let file_exists = std::path::Path::new(file_path).exists();
                let action = if file_exists { "Replace" } else { "Create" };

                let content_preview = if content.len() > 500 {
                    format!(
                        "{}... ({} bytes)",
                        content.chars().take(500).collect::<String>(),
                        content.len()
                    )
                } else {
                    content.to_string()
                };

                let preview = format!(
                    "=== {} file: {} ===\n{}",
                    action, file_path, content_preview
                );

                Some(preview)
            }
            "ApplyPatch" => {
                let patch = arguments.get("patch")?.as_str()?;
                Some(patch.to_string())
            }
            _ => None,
        }
    }
}

/// One terminal decoder for initial turns and local-client continuations.
async fn forward_code_stream(
    client: Box<dyn cortex_engine::client::ModelClient>,
    request: CompletionRequest,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    tx: mpsc::Sender<StreamEvent>,
) {
    use cortex_engine::client::runtime_contract::{INCOMPLETE_STREAM, LOCAL_TOOLS_UNSUPPORTED};
    let result: Result<StreamEvent, String> = async {
        let mut stream = tokio::select! {
            _ = cortex_engine::session::control::wait_for_cancellation(&cancelled) => return Err("Cancelled.".into()),
            result = tokio::time::timeout(Duration::from_secs(60), client.complete(request)) => {
                result.map_err(|_| "The coding service is temporarily unavailable".to_string())?
                    .map_err(|error| error.user_friendly_message())?
            }
        };
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut observations = std::collections::HashSet::new();
        loop {
            let event = tokio::select! {
                _ = cortex_engine::session::control::wait_for_cancellation(&cancelled) => return Err("Cancelled.".into()),
                event = tokio::time::timeout(Duration::from_secs(30), stream.next()) => {
                    event.map_err(|_| "The coding service is temporarily unavailable".to_string())?
                        .ok_or_else(|| INCOMPLETE_STREAM.to_string())?
                        .map_err(|error| error.user_friendly_message())?
                }
            };
            let event = match event {
                ResponseEvent::Delta(delta) => { content.push_str(&delta); StreamEvent::Delta(delta) }
                ResponseEvent::Reasoning(delta) => { reasoning.push_str(&delta); StreamEvent::Reasoning(delta) }
                ResponseEvent::Done(response) => {
                    response.finish_reason.require_success().map_err(|error| error.user_friendly_message())?;
                    if !observations.is_empty() { return Err(INCOMPLETE_STREAM.into()); }
                    return Ok(StreamEvent::Done {
                        content, reasoning,
                        tokens: Some(cortex_engine::streaming::StreamTokenUsage::from(response.usage)),
                    });
                }
                ResponseEvent::Error(error) => return Err(error),
                ResponseEvent::ToolCall(call) => {
                    if !call.remote && client.owns_tool_execution() { return Err(LOCAL_TOOLS_UNSUPPORTED.into()); }
                    let mut arguments = serde_json::from_str(&call.arguments)
                        .unwrap_or_else(|_| serde_json::json!({"raw":call.arguments}));
                    if call.remote {
                        if call.id.is_empty() || !observations.insert(call.id.clone()) { return Err(INCOMPLETE_STREAM.into()); }
                        if !arguments.is_object() { arguments = serde_json::json!({"arguments":arguments}); }
                        arguments["remote"] = serde_json::json!(true);
                    }
                    StreamEvent::ToolCall { id:call.id, name:call.name, arguments }
                }
                ResponseEvent::ToolResult { id, success, output } => {
                    if !observations.remove(&id) { return Err(INCOMPLETE_STREAM.into()); }
                    StreamEvent::ToolCallComplete { id, success, output }
                }
            };
            tx.send(event).await.map_err(|_| "The turn output was closed.".to_string())?;
        }
    }.await;
    let terminal = match result {
        Ok(done) => done,
        Err(mut message) => {
            if let Err(error) = client.cancel_turn_checked().await {
                message.push_str(&format!(" {}", error.user_friendly_message()));
            }
            StreamEvent::Error(message)
        }
    };
    let _ = tx.send(terminal).await;
}

#[cfg(test)]
#[path = "runtime_contract_streaming_tests.rs"]
mod runtime_contract_tests;

#[cfg(test)]
mod stream_error_tests {
    use super::{StreamErrorKind, classify_stream_error};

    #[test]
    fn transport_names_never_reach_the_user_but_actions_do() {
        assert_eq!(
            classify_stream_error("hyper: connection reset"),
            StreamErrorKind::ServiceUnavailable
        );
        assert_eq!(
            classify_stream_error("Run cortex login: not signed in"),
            StreamErrorKind::Actionable
        );
        assert_eq!(
            classify_stream_error("Cancelled"),
            StreamErrorKind::Cancelled
        );
        assert_eq!(
            classify_stream_error("HTTP 401 unauthorized"),
            StreamErrorKind::AuthenticationRequired
        );
        assert_eq!(
            classify_stream_error("402 payment required"),
            StreamErrorKind::InsufficientBalance
        );
        assert_eq!(
            classify_stream_error("429 too many requests"),
            StreamErrorKind::QuotaExhausted
        );
    }
}
