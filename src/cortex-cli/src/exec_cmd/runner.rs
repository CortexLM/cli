//! Execution runner for exec mode.

use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use cortex_protocol::{ConversationId, Event, EventMsg, Op, Submission, UserInput};

use super::autonomy::AutonomyLevel;
use super::cli::ExecCli;
use super::helpers::{
    collect_files_by_pattern, ensure_utf8_locale, fetch_url_content, get_git_diff, read_clipboard,
    validate_path_environment,
};
use super::output::{ExecInputFormat, ExecOutputFormat};

/// What one exec run produced, so the terminal result cannot claim success
/// for an incomplete or failed run.
#[derive(Default)]
struct RunOutcome {
    final_message: String,
    num_turns: u64,
    error_occurred: bool,
    task_completed: bool,
    error_message: Option<String>,
    tool_calls_count: u64,
}

impl ExecCli {
    /// Run the exec command.
    pub async fn run(self) -> Result<()> {
        self.validate_runtime_options()?;

        // Validate mutually exclusive tool flags
        if !self.enabled_tools.is_empty() && !self.disabled_tools.is_empty() {
            bail!(
                "Cannot specify both --enabled-tools and --disabled-tools. Choose one to filter tools."
            );
        }

        // Ensure UTF-8 locale for proper text handling
        let _ = ensure_utf8_locale();

        // Validate PATH environment
        let path_warnings = validate_path_environment();
        for warning in &path_warnings {
            if self.verbose {
                eprintln!("{}", warning);
            }
        }

        // Handle --list-tools
        if self.list_tools {
            return self.list_available_tools().await;
        }

        let autonomy = if self.skip_permissions {
            None
        } else {
            Some(self.autonomy.unwrap_or_default())
        };
        // Protocol stdin has one owner. Never read it as a text prompt first.
        if matches!(self.input_format, ExecInputFormat::StreamJsonrpc) {
            if !self.prompt.is_empty()
                || self.file.is_some()
                || self.clipboard
                || !self.urls.is_empty()
                || self.git_diff
                || !self.include_patterns.is_empty()
                || !self.exclude_patterns.is_empty()
            {
                bail!(
                    "stream-jsonrpc accepts prompts through message requests, not text-input flags."
                );
            }
            return self.run_multiturn(String::new(), autonomy).await;
        }
        if matches!(self.output_format, ExecOutputFormat::StreamJsonrpc) {
            bail!("--output-format stream-jsonrpc requires --input-format stream-jsonrpc.");
        }
        let prompt = self.build_prompt().await?;
        if prompt.is_empty() {
            bail!("No prompt provided. Use positional argument, --file, or pipe via stdin.");
        }
        if self.echo {
            eprintln!("--- Prompt ---\n{}\n--- End Prompt ---", prompt);
        }
        self.run_single(prompt, autonomy).await
    }

    /// Build the prompt from various sources.
    pub(crate) async fn build_prompt(&self) -> Result<String> {
        let prompt = self.direct_input().await?;
        let context_sections = self.context_sections().await;

        let mut final_prompt = String::new();
        if !context_sections.is_empty() {
            final_prompt.push_str(&context_sections.join("\n\n"));
            final_prompt.push_str("\n\n");
        }
        final_prompt.push_str(prompt.trim());
        if let Some(ref suffix) = self.suffix {
            final_prompt.push_str("\n\n[Suffix for completion insertion: ");
            final_prompt.push_str(suffix);
            final_prompt.push(']');
        }

        Ok(final_prompt.trim().to_string())
    }

    /// The instruction itself: prompt file, command line, then piped stdin.
    async fn direct_input(&self) -> Result<String> {
        let mut prompt = String::new();
        if let Some(ref file_path) = self.file {
            let file_path = self
                .cwd
                .as_ref()
                .map(|cwd| cwd.join(file_path))
                .unwrap_or_else(|| file_path.clone());
            let content = tokio::fs::read_to_string(&file_path)
                .await
                .with_context(|| format!("Failed to read prompt file: {}", file_path.display()))?;
            prompt.push_str(&content);
        }
        if !self.prompt.is_empty() {
            if !prompt.is_empty() {
                prompt.push('\n');
            }
            prompt.push_str(&self.prompt.join(" "));
        }
        if !io::stdin().is_terminal() {
            let mut stdin_content = String::new();
            io::stdin().lock().read_to_string(&mut stdin_content)?;
            if !stdin_content.is_empty() {
                if !prompt.is_empty() {
                    prompt.push('\n');
                }
                prompt.push_str(&stdin_content);
            }
        }
        Ok(prompt)
    }

    /// Optional surrounding context. Unavailable sources are reported and
    /// skipped rather than failing the run or silently substituting content.
    async fn context_sections(&self) -> Vec<String> {
        let mut context_sections: Vec<String> = Vec::new();
        if self.clipboard
            && let Ok(clipboard_content) = read_clipboard()
            && !clipboard_content.is_empty()
        {
            context_sections.push(format!(
                "--- Clipboard Content ---\n{}\n--- End Clipboard ---",
                clipboard_content
            ));
        }
        for url in &self.urls {
            match fetch_url_content(url).await {
                Ok(content) => {
                    context_sections.push(format!(
                        "--- Content from {} ---\n{}\n--- End URL Content ---",
                        url, content
                    ));
                }
                Err(e) => {
                    eprintln!("Warning: Failed to fetch URL {}: {}", url, e);
                }
            }
        }
        if self.git_diff
            && let Ok(diff) = get_git_diff(&self.context_cwd())
            && !diff.is_empty()
        {
            context_sections.push(format!("--- Git Diff ---\n{}\n--- End Git Diff ---", diff));
        }
        if (!self.include_patterns.is_empty() || !self.exclude_patterns.is_empty())
            && let Ok(files_content) = collect_files_by_pattern(
                &self.context_cwd(),
                &self.include_patterns,
                &self.exclude_patterns,
            )
            && !files_content.is_empty()
        {
            context_sections.push(files_content);
        }
        context_sections
    }

    fn context_cwd(&self) -> PathBuf {
        self.cwd
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    /// List available tools for the selected model.
    pub(crate) async fn list_available_tools(&self) -> Result<()> {
        use cortex_engine::tools::ToolRouter;

        let router = ToolRouter::new();
        let tools = router.get_tool_definitions();

        // Filter by enabled/disabled
        let filtered_tools: Vec<_> = tools
            .iter()
            .filter(|t| {
                if !self.enabled_tools.is_empty() {
                    return self
                        .enabled_tools
                        .iter()
                        .any(|e| e.eq_ignore_ascii_case(&t.name));
                }
                if !self.disabled_tools.is_empty() {
                    return !self
                        .disabled_tools
                        .iter()
                        .any(|d| d.eq_ignore_ascii_case(&t.name));
                }
                true
            })
            .collect();

        match self.output_format {
            ExecOutputFormat::Json | ExecOutputFormat::StreamJson => {
                let tools_json: Vec<_> = filtered_tools
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "name": t.name,
                            "description": t.description,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&tools_json)?);
            }
            _ => {
                println!("Available tools ({}):", filtered_tools.len());
                println!("{:-<60}", "");
                for tool in filtered_tools {
                    println!(
                        "  {:<20} - {}",
                        tool.name,
                        tool.description.chars().take(50).collect::<String>()
                    );
                }
            }
        }

        Ok(())
    }

    /// Run single-shot execution.
    pub(crate) async fn run_single(
        &self,
        prompt: String,
        autonomy: Option<AutonomyLevel>,
    ) -> Result<()> {
        let start_time = Instant::now();
        let _is_json = matches!(self.output_format, ExecOutputFormat::Json);
        let is_stream = matches!(
            self.output_format,
            ExecOutputFormat::StreamJson | ExecOutputFormat::Debug
        );

        let config = self.runtime_config(autonomy).await?;
        let cwd = config.cwd.clone();

        // Initialize custom command registry
        let project_root = Some(cwd.clone());
        let _custom_registry = cortex_engine::init_custom_command_registry(
            &config.cortex_home,
            project_root.as_deref(),
        );
        if let Err(e) = _custom_registry.scan().await {
            tracing::warn!("Failed to scan custom commands: {}", e);
        }

        // Create session
        let input_items = self.build_input_items(&prompt).await?;
        let (mut session, handle) = self.open_runtime_session(config.clone())?;

        // Spawn session task
        let session_task = tokio::spawn(async move { session.run().await });

        let result: Result<()> = async {
            // Emit init event for stream formats
            if is_stream {
                let init_event = serde_json::json!({
                    "type": "system",
                    "subtype": "init",
                    "cwd": cwd.display().to_string(),
                    "session_id": handle.conversation_id.to_string(),
                    "model": config.model,
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                });
                writeln!(io::stdout(), "{}", serde_json::to_string(&init_event)?)?;
            }

            // Send the submission
            let submission = Submission {
                id: uuid::Uuid::new_v4().to_string(),
                op: Op::UserInput { items: input_items },
            };
            handle.submission_tx.send(submission).await?;

            // Process events
            self.process_events(&handle, start_time, autonomy).await
        }
        .await;

        let cleanup = cortex_engine::session::control::stop_session(&handle, session_task).await;
        cleanup?;
        result?;
        Ok(())
    }

    /// Build input items from prompt and images.
    pub(crate) async fn build_input_items(&self, prompt: &str) -> Result<Vec<UserInput>> {
        let mut items = Vec::new();

        // Add images first
        for image_path in &self.images {
            let resolved_path = if image_path.is_absolute() {
                image_path.clone()
            } else {
                self.cwd
                    .clone()
                    .unwrap_or(std::env::current_dir()?)
                    .join(image_path)
            };

            if !resolved_path.exists() {
                bail!("File not found: {}", image_path.display());
            }

            let image_bytes = tokio::fs::read(&resolved_path)
                .await
                .with_context(|| format!("Failed to read image: {}", resolved_path.display()))?;

            use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
            let base64_data = BASE64.encode(&image_bytes);

            let media_type = resolved_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| match ext.to_lowercase().as_str() {
                    "png" => "image/png",
                    "jpg" | "jpeg" => "image/jpeg",
                    "gif" => "image/gif",
                    "webp" => "image/webp",
                    _ => "application/octet-stream",
                })
                .unwrap_or("application/octet-stream")
                .to_string();

            items.push(UserInput::Image {
                data: base64_data,
                media_type,
            });
        }

        // Add text prompt
        items.push(UserInput::Text {
            text: prompt.to_string(),
        });

        Ok(items)
    }

    /// Process events from the session.
    pub(crate) async fn process_events(
        &self,
        handle: &cortex_engine::SessionHandle,
        start_time: Instant,
        autonomy: Option<AutonomyLevel>,
    ) -> Result<()> {
        let _is_json = matches!(self.output_format, ExecOutputFormat::Json);
        let is_stream = matches!(
            self.output_format,
            ExecOutputFormat::StreamJson | ExecOutputFormat::Debug
        );
        let is_text = matches!(self.output_format, ExecOutputFormat::Text);

        let timeout = if self.timeout > 0 {
            Some(Duration::from_secs(self.timeout))
        } else {
            None
        };

        let mut outcome = RunOutcome::default();
        let RunOutcome {
            ref mut final_message,
            ref mut num_turns,
            ref mut error_occurred,
            ref mut task_completed,
            ref mut error_message,
            ref mut tool_calls_count,
        } = outcome;

        let deadline =
            timeout.map(|duration| tokio::time::Instant::from_std(start_time + duration));
        loop {
            let event = tokio::select! {
                event = cortex_engine::session::control::next_event(handle, deadline) => event,
                _ = tokio::signal::ctrl_c() => Err(cortex_engine::CortexError::Cancelled),
            };
            let event = match event {
                Ok(event) => event,
                Err(error) => {
                    *error_occurred = true;
                    *error_message = Some(error.user_friendly_message());
                    break;
                }
            };

            // Emit stream events
            if is_stream {
                self.emit_stream_event(&mut io::stdout(), &event, &handle.conversation_id)?;
            }

            match &event.msg {
                EventMsg::AgentMessage(msg) => {
                    *final_message = msg.message.clone();
                }
                EventMsg::AgentMessageDelta(delta) => {
                    if is_text {
                        write!(io::stdout(), "{}", delta.delta)?;
                        io::stdout().flush()?;
                    }
                    final_message.push_str(&delta.delta);
                }
                EventMsg::TaskStarted(_) => {
                    *num_turns += 1;
                    if *num_turns > self.max_turns as u64 {
                        *error_occurred = true;
                        *error_message = Some(format!("Max turns ({}) exceeded", self.max_turns));
                        break;
                    }
                }
                EventMsg::TaskComplete(_) => {
                    *task_completed = true;
                    break;
                }
                EventMsg::ExecCommandBegin(cmd_begin) => {
                    *tool_calls_count += 1;
                    if is_text && self.verbose {
                        let cmd_str = cmd_begin.command.join(" ");
                        eprintln!("\x1b[1;34m[EXEC]\x1b[0m {}", cmd_str);
                    }
                }
                EventMsg::McpToolCallBegin(mcp_begin) => {
                    *tool_calls_count += 1;
                    if is_text && self.verbose {
                        eprintln!(
                            "\x1b[1;36m[TOOL]\x1b[0m {} ({})",
                            mcp_begin.invocation.tool, mcp_begin.invocation.server
                        );
                    }
                }
                EventMsg::ExecApprovalRequest(approval) => {
                    let decision = super::runtime_contract_protocol::approval_decision(
                        approval,
                        autonomy,
                        self.skip_permissions,
                    );
                    let denied = matches!(decision, cortex_protocol::ReviewDecision::Denied);
                    handle
                        .submission_tx
                        .send(Submission {
                            id: uuid::Uuid::new_v4().to_string(),
                            op: Op::ExecApproval {
                                id: approval.call_id.clone(),
                                decision,
                            },
                        })
                        .await?;
                    if denied {
                        *error_occurred = true;
                        *error_message =
                            Some("Approval required; the unattended command was denied.".into());
                        break;
                    }
                }
                EventMsg::TurnAborted(_) => {
                    *error_occurred = true;
                    *error_message = Some("The task was interrupted.".into());
                    break;
                }
                EventMsg::Error(e) => {
                    *error_occurred = true;
                    *error_message = Some(e.message.clone());
                    if is_text {
                        eprintln!("\x1b[1;31m[ERROR]\x1b[0m {}", e.message);
                    }
                    break;
                }
                EventMsg::Warning(w) => {
                    if is_text {
                        eprintln!("\x1b[1;33m[WARN]\x1b[0m {}", w.message);
                    }
                }
                _ => {}
            }
        }

        if !outcome.task_completed {
            outcome.error_occurred = true;
        }
        // Stop the owner before emitting an unsuccessful terminal result.
        if outcome.error_occurred {
            handle
                .submission_tx
                .send(Submission {
                    id: uuid::Uuid::new_v4().to_string(),
                    op: Op::Shutdown,
                })
                .await
                .ok();
        }
        self.emit_result(
            &mut io::stdout(),
            &handle.conversation_id,
            &outcome,
            start_time.elapsed().as_millis() as u64,
        )?;

        if outcome.error_occurred {
            bail!(
                "{}",
                outcome
                    .error_message
                    .unwrap_or_else(|| "The task did not complete successfully.".into())
            );
        }

        Ok(())
    }

    /// Emit the terminal result. An unsuccessful run is never reported as success.
    fn emit_result(
        &self,
        output: &mut impl Write,
        session_id: &ConversationId,
        outcome: &RunOutcome,
        duration_ms: u64,
    ) -> Result<()> {
        match self.output_format {
            ExecOutputFormat::Text => {
                if !outcome.final_message.is_empty() && !outcome.final_message.ends_with('\n') {
                    writeln!(output)?;
                }
            }
            ExecOutputFormat::Json => {
                let result = if outcome.error_occurred {
                    serde_json::json!({
                        "type": "result",
                        "subtype": "error",
                        "is_error": true,
                        "error": outcome.error_message,
                        "duration_ms": duration_ms,
                        "num_turns": outcome.num_turns,
                        "session_id": session_id.to_string(),
                    })
                } else {
                    serde_json::json!({
                        "type": "result",
                        "subtype": "success",
                        "is_error": false,
                        "result": outcome.final_message,
                        "duration_ms": duration_ms,
                        "num_turns": outcome.num_turns,
                        "session_id": session_id.to_string(),
                    })
                };
                writeln!(output, "{}", serde_json::to_string_pretty(&result)?)?;
            }
            ExecOutputFormat::StreamJson | ExecOutputFormat::Debug => {
                let completion = serde_json::json!({
                    "type": "completion",
                    "finalText": outcome.final_message,
                    "numTurns": outcome.num_turns,
                    "durationMs": duration_ms,
                    "session_id": session_id.to_string(),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                    "toolCalls": outcome.tool_calls_count,
                    "success": outcome.task_completed && !outcome.error_occurred,
                    "error": outcome.error_message,
                });
                writeln!(output, "{}", serde_json::to_string(&completion)?)?;
            }
            ExecOutputFormat::StreamJsonrpc => {
                // Already handled in multi-turn mode
            }
        }
        Ok(())
    }

    /// Emit a stream event in JSONL format.
    pub(crate) fn emit_stream_event(
        &self,
        output: &mut impl Write,
        event: &Event,
        session_id: &ConversationId,
    ) -> Result<()> {
        let event_json = match &event.msg {
            EventMsg::AgentMessage(msg) => {
                serde_json::json!({
                    "type": "message",
                    "role": "assistant",
                    "id": msg.id,
                    "text": msg.message,
                    "session_id": session_id.to_string(),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                })
            }
            EventMsg::AgentMessageDelta(delta) => {
                serde_json::json!({
                    "type": "delta",
                    "content": delta.delta,
                    "session_id": session_id.to_string(),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                })
            }
            EventMsg::ExecCommandBegin(cmd) => {
                serde_json::json!({
                    "type": "tool_call",
                    "id": cmd.call_id,
                    "toolName": "Execute",
                    "parameters": {
                        "command": cmd.command.join(" "),
                        "cwd": cmd.cwd.display().to_string(),
                    },
                    "session_id": session_id.to_string(),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                })
            }
            EventMsg::ExecCommandEnd(cmd) => {
                serde_json::json!({
                    "type": "tool_result",
                    "id": cmd.call_id,
                    "toolName": "Execute",
                    "isError": cmd.exit_code != 0,
                    "value": cmd.formatted_output,
                    "exitCode": cmd.exit_code,
                    "durationMs": cmd.duration_ms,
                    "session_id": session_id.to_string(),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                })
            }
            EventMsg::McpToolCallBegin(mcp) => {
                serde_json::json!({
                    "type": "tool_call",
                    "id": mcp.call_id,
                    "toolName": mcp.invocation.tool,
                    "server": mcp.invocation.server,
                    "parameters": mcp.invocation.arguments,
                    "session_id": session_id.to_string(),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                })
            }
            EventMsg::McpToolCallEnd(mcp) => {
                let (is_error, value) = match &mcp.result {
                    Ok(result) => (
                        result.is_error.unwrap_or(false),
                        serde_json::to_value(&result.content).unwrap_or_default(),
                    ),
                    Err(e) => (true, serde_json::json!(e)),
                };
                serde_json::json!({
                    "type": "tool_result",
                    "id": mcp.call_id,
                    "toolName": mcp.invocation.tool,
                    "isError": is_error,
                    "value": value,
                    "durationMs": mcp.duration_ms,
                    "session_id": session_id.to_string(),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                })
            }
            EventMsg::Error(e) => {
                serde_json::json!({
                    "type": "error",
                    "message": e.message,
                    "session_id": session_id.to_string(),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                })
            }
            EventMsg::AgentReasoning(r) => {
                serde_json::json!({
                    "type": "reasoning",
                    "text": r.text,
                    "session_id": session_id.to_string(),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                })
            }
            _ => return Ok(()), // Skip other events
        };

        writeln!(output, "{}", serde_json::to_string(&event_json)?)?;
        output.flush()?;
        Ok(())
    }

    /// Run multi-turn execution via stream-jsonrpc.
    pub(crate) async fn run_multiturn(
        &self,
        _initial_prompt: String,
        autonomy: Option<AutonomyLevel>,
    ) -> Result<()> {
        let config = self.runtime_config(autonomy).await?;
        let (mut session, handle) = self.open_runtime_session(config.clone())?;
        let session_task = tokio::spawn(async move { session.run().await });
        let lines = super::runtime_contract_protocol::stdin_lines();
        let result = super::runtime_contract_protocol::run_protocol(
            &handle,
            lines,
            &mut io::stdout(),
            &config,
            autonomy,
            self.skip_permissions,
            self.max_turns,
            self.timeout,
        )
        .await;
        let cleanup = cortex_engine::session::control::stop_session(&handle, session_task).await;
        cleanup?;
        result?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
