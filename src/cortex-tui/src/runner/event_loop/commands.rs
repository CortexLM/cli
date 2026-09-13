//! Command handling: slash commands and result processing.
//!
//! This module handles the execution of slash commands and the processing
//! of their results. Due to the large number of command handlers, they are
//! delegated to the main event_loop module in the original monolithic file.
//! This file provides the command result handling infrastructure.

use anyhow::Result;

use crate::app::AppView;
use crate::commands::{CommandResult, FormRegistry, ModalType, ViewType};
use crate::session::{ExportFormat, default_export_filename, export_session};

use super::core::EventLoop;

impl EventLoop {
    /// Handles a command result from the CommandExecutor.
    pub(super) async fn handle_command_result(&mut self, result: CommandResult) -> Result<()> {
        match result {
            CommandResult::Success => {}

            CommandResult::Message(msg) => {
                self.add_system_message(&msg);
            }

            CommandResult::Error(err) => {
                self.add_system_message(&format!("Error: {}", err));
            }

            CommandResult::Quit => {
                self.app_state.set_quit();
            }

            CommandResult::Clear => {
                self.app_state
                    .enter_interactive_mode(crate::interactive::builders::build_clear_confirm());
            }

            CommandResult::Interrupt => {
                self.cancel_streaming();
            }

            CommandResult::NewSession => {
                let result = self.new_local_session();
                self.report_local_result(result, "New session started");
            }

            CommandResult::ResumeSession(id) => {
                let result = self.resume_local_session(&id);
                self.report_local_result(result, "Session resumed");
            }

            CommandResult::Toggle(feature) => {
                self.handle_toggle(&feature);
            }

            CommandResult::SetValue(key, value) => {
                self.handle_set_value(&key, &value);
            }

            CommandResult::OpenModal(modal_type) => {
                self.handle_open_modal(modal_type).await;
            }

            CommandResult::SwitchView(view_type) => match view_type {
                ViewType::Session => self.app_state.set_view(AppView::Session),
                ViewType::Settings => self.app_state.set_view(AppView::Settings),
                ViewType::Help => self.app_state.set_view(AppView::Help),
            },

            CommandResult::Async(cmd_string) => {
                self.handle_async_command(&cmd_string).await?;
            }

            CommandResult::NotFound(cmd) => {
                self.add_system_message(&format!("Unknown command: {}", cmd));
            }

            CommandResult::NeedsArgs(msg) => {
                self.add_system_message(&msg);
            }
        }

        Ok(())
    }

    /// Handle toggle commands
    fn handle_toggle(&mut self, feature: &str) {
        match feature {
            "sidebar" => {
                self.app_state.toggle_sidebar();
                let state = if self.app_state.sidebar_visible {
                    "shown"
                } else {
                    "hidden"
                };
                self.app_state.toasts.info(format!("Sidebar {}", state));
            }
            "compact" => {
                self.app_state.toggle_compact();
                let state = if self.app_state.compact_mode {
                    "on"
                } else {
                    "off"
                };
                self.app_state
                    .toasts
                    .info(format!("Compact mode: {}", state));
            }
            "favorite" => {
                let value = self
                    .cortex_session
                    .as_ref()
                    .is_some_and(|s| !s.meta.favorite);
                let result =
                    self.change_session_metadata("favorite", if value { "true" } else { "false" });
                self.report_local_result(result, "Favorite updated");
            }

            "debug" => {
                self.app_state.toggle_debug();
                let state = if self.app_state.debug_mode {
                    "on"
                } else {
                    "off"
                };
                self.app_state.toasts.info(format!("Debug mode: {}", state));
            }
            "sandbox" => {
                self.app_state.toggle_sandbox();
                let state = if self.app_state.sandbox_mode {
                    "on"
                } else {
                    "off"
                };
                self.app_state
                    .toasts
                    .info(format!("Sandbox mode: {}", state));
            }
            "auto" => {
                let is_yolo = matches!(
                    self.app_state.permission_mode,
                    crate::permissions::PermissionMode::Yolo
                );
                if is_yolo {
                    self.app_state.permission_mode = crate::permissions::PermissionMode::High;
                    self.app_state.toasts.info("Auto-approve: OFF");
                } else {
                    self.app_state.permission_mode = crate::permissions::PermissionMode::Yolo;
                    self.app_state.toasts.success("Auto-approve: ON");
                }
            }
            _ => {
                self.app_state
                    .toasts
                    .error(format!("Unknown toggle: {}", feature));
            }
        }
    }

    /// Handle open modal command
    pub(super) async fn handle_open_modal(&mut self, modal_type: ModalType) {
        match modal_type {
            ModalType::Help(topic) => {
                use crate::modal::HelpModal;
                self.modal_stack
                    .push(Box::new(HelpModal::with_topic(topic)));
            }
            ModalType::Settings => {
                self.app_state.open_settings_modal();
            }
            ModalType::ModelPicker | ModalType::Effort => {
                let (models, current_model) = if let Some(ref pm) = self.provider_manager {
                    if let Ok(manager) = pm.try_read() {
                        let models = manager.available_models();
                        let current = manager.current_model().to_string();

                        if models.is_empty() {
                            tracing::warn!(
                                "No models available. Auth token present: {}, cached_models: {}",
                                manager.is_authenticated(),
                                manager.available_models().len()
                            );
                        }

                        (models, Some(current))
                    } else {
                        (Vec::new(), None)
                    }
                } else {
                    (Vec::new(), None)
                };

                if models.is_empty() {
                    self.add_system_message(
                        "No models available. Run /login to authenticate or check your connection.",
                    );
                }

                let interactive = crate::interactive::builders::build_model_selector(
                    models,
                    current_model.as_deref(),
                    self.app_state.thinking_budget.as_deref(),
                );
                self.app_state.enter_interactive_mode(interactive);
            }
            ModalType::CommandPalette => {
                use crate::modal::CommandsModal;
                self.modal_stack.push(Box::new(CommandsModal::new()));
            }
            ModalType::Sessions => {
                self.open_sessions_modal();
            }
            ModalType::McpManager => {
                use crate::interactive::builders::build_mcp_selector;
                let servers = self.app_state.mcp_servers.clone();
                let interactive = build_mcp_selector(&servers);
                self.app_state.enter_interactive_mode(interactive);
            }
            ModalType::Form(cmd_name) => match cmd_name.as_str() {
                "temperature" => {
                    let interactive = crate::interactive::builders::build_temperature_selector(
                        self.app_state.temperature,
                    );
                    self.app_state.enter_interactive_mode(interactive);
                }
                "scroll" => {
                    let interactive = crate::interactive::builders::build_scroll_selector();
                    self.app_state.enter_interactive_mode(interactive);
                }
                "sandbox" => {
                    let interactive = crate::interactive::builders::build_sandbox_selector(
                        self.app_state.sandbox_mode,
                    );
                    self.app_state.enter_interactive_mode(interactive);
                }
                _ => {
                    let registry = FormRegistry::new();
                    if let Some(form_state) = registry.get_form(&cmd_name) {
                        self.app_state.active_modal =
                            Some(crate::app::ActiveModal::Form(form_state));
                    } else {
                        let usage = match cmd_name.as_str() {
                            "rename" => "/rename <new name>",
                            "remove" => "/remove <file>...",
                            "search" => "/search <pattern>",
                            "mention" => "/mention <file|symbol>",
                            "tokens" => "/tokens <number>",
                            "goto" => "/goto <message number>",
                            "delete" => "/delete <session-id>",
                            "eval" => "/eval <expression>",
                            _ => &cmd_name,
                        };
                        self.app_state.toasts.info(format!("Usage: {}", usage));
                    }
                }
            },
            ModalType::ApprovalPicker | ModalType::Permissions => {
                let current = match self.app_state.permission_mode {
                    crate::permissions::PermissionMode::High => "ro",
                    crate::permissions::PermissionMode::Medium => "smart",
                    crate::permissions::PermissionMode::Low
                    | crate::permissions::PermissionMode::Yolo => "full",
                };
                let interactive =
                    crate::interactive::builders::build_permissions_picker(Some(current));
                self.app_state.enter_interactive_mode(interactive);
            }
            ModalType::LogLevelPicker => {
                let current = self.app_state.log_level.clone();
                let interactive =
                    crate::interactive::builders::build_log_level_selector(Some(&current));
                self.app_state.enter_interactive_mode(interactive);
            }
            ModalType::Timeline => {
                self.app_state.toasts.info(
                    "Timeline opens the persisted transcript; workspace restore is unavailable.",
                );
                self.handle_transcript();
            }
            ModalType::ThemePicker => {
                use crate::modal::ThemeSelectorModal;
                // Use the effective theme (preview or active) for the modal
                let current_theme = self.app_state.get_effective_theme_name().to_string();
                self.modal_stack
                    .push(Box::new(ThemeSelectorModal::new(&current_theme)));
            }
            ModalType::Fork => {
                let result = self.fork_local_session(None);
                self.report_local_result(result, "Fork created; original session unchanged");
            }

            ModalType::FilePicker => {
                let cwd = std::env::current_dir().unwrap_or_default();
                let interactive = crate::interactive::builders::build_file_browser(&cwd);
                self.app_state.enter_interactive_mode(interactive);
            }
            ModalType::Export(format) => {
                if let Some(fmt) = format {
                    match fmt.as_str() {
                        "md" | "markdown" => {
                            let _ = self.handle_export(ExportFormat::Markdown).await;
                        }
                        "json" => {
                            let _ = self.handle_export(ExportFormat::Json).await;
                        }
                        "txt" | "text" => {
                            let _ = self.handle_export(ExportFormat::Text).await;
                        }
                        _ => {
                            self.app_state
                                .toasts
                                .error(format!("Unknown format: {}", fmt));
                        }
                    }
                } else {
                    let interactive = crate::interactive::builders::build_export_selector();
                    self.app_state.enter_interactive_mode(interactive);
                }
            }
            ModalType::Confirm(msg) => {
                self.add_system_message(&format!("{msg} Use cortex delete SESSION_ID from the CLI for protected, confirmed deletion. No session was deleted."));
            }
            ModalType::Login => {
                self.start_login_flow().await;
            }
            ModalType::Upgrade => {
                self.app_state.toasts.info("Checking for updates...");
            }
            ModalType::Agents => {
                let cwd = std::env::current_dir().ok();
                let terminal_height = self.app_state.terminal_size.1;
                let interactive = crate::interactive::builders::build_agents_selector(
                    cwd.as_deref(),
                    Some(terminal_height),
                );
                self.app_state.enter_interactive_mode(interactive);
            }
            ModalType::Mode => {
                let interactive = crate::interactive::builders::build_mode_selector(
                    &self.app_state.agent_mode_label,
                );
                self.app_state.enter_interactive_mode(interactive);
            }
            ModalType::Plan => {
                self.app_state.set_agent_mode("plan");
                self.sync_agent_mode_harness();
                self.app_state.toasts.info("Mode: Plan");
            }
            ModalType::Skills => {
                let interactive = self.build_skills_picker().await;
                self.app_state.enter_interactive_mode(interactive);
            }
            ModalType::Tasks => {
                let rows: Vec<_> = self
                    .app_state
                    .active_subagents
                    .iter()
                    .map(crate::interactive::builders::JobRow::from_subagent)
                    .collect();
                let interactive = crate::interactive::builders::build_jobs_picker(&rows);
                self.app_state.enter_interactive_mode(interactive);
            }
        }
    }

    /// Handle async commands
    pub(super) async fn handle_async_command(&mut self, cmd: &str) -> Result<()> {
        tracing::debug!("Async command: {}", cmd);

        match cmd {
            "sessions:list" => self.open_sessions_modal(),
            "undo" => {
                let result = self.rewind_conversation(1);
                self.report_local_result(result, "");
            }
            "redo" => {
                let result = self.redo_conversation();
                self.report_local_result(result, "");
            }
            cmd if cmd.starts_with("export:") => {
                if let Some(format) = ExportFormat::parse(&cmd[7..]) {
                    self.handle_export(format).await?;
                } else {
                    self.add_system_message("Unsupported export format");
                }
            }
            cmd if cmd == "diff" || cmd.starts_with("diff:") || cmd.starts_with("review:") => {
                self.show_local_diff(cmd).await;
            }
            cmd if cmd == "images" || cmd.starts_with("images:") => {
                self.add_system_message("Image attachments are not supported by this interactive session. No image was sent.");
            }
            "providers:list" => {
                self.handle_providers_list();
            }
            "session:info" => {
                self.handle_session_info();
            }
            "transcript" => {
                self.handle_transcript();
            }
            "history" => {
                self.handle_history();
            }
            "models:fetch-and-pick" => {
                // First, fetch models from the backend to populate the cache
                if let Some(pm) = &self.provider_manager {
                    // Need to acquire write lock to call fetch_models()
                    match pm.try_write() {
                        Ok(mut manager) => {
                            if let Err(e) = manager.fetch_models().await {
                                tracing::warn!("Failed to fetch models: {}", e);
                                self.app_state
                                    .toasts
                                    .warning("Could not fetch models, showing cached list");
                            }
                        }
                        Err(_) => {
                            tracing::warn!("Could not acquire write lock for provider manager");
                        }
                    }
                }
                // Now open the modal with freshly fetched (or cached) models
                self.handle_open_modal(ModalType::ModelPicker).await;
            }
            cmd if cmd.starts_with("billing:usage") => {
                let (from, to) = parse_usage_range(cmd);
                crate::runner::billing_handlers::spawn_usage_fetch(
                    self.tool_event_tx.clone(),
                    from,
                    to,
                );
                self.app_state.toasts.info("Fetching usage…");
            }
            cmd if cmd.starts_with("skill:invoke:") => {
                self.invoke_skill_command(cmd).await;
            }
            cmd if cmd == "goal:status"
                || cmd == "goal:pause"
                || cmd == "goal:resume"
                || cmd == "goal:clear"
                || cmd.starts_with("goal:set:") =>
            {
                self.handle_goal_command(cmd).await?;
            }
            _ => {
                self.add_system_message(&format!(
                    "Unsupported command in this session: {cmd}. No operation was performed."
                ));
            }
        }

        Ok(())
    }

    /// Handle export command
    pub(super) async fn handle_export(&mut self, format: ExportFormat) -> Result<()> {
        let Some(session) = &self.cortex_session else {
            self.add_system_message("No active session to export.");
            return Ok(());
        };

        let filename = default_export_filename(session, format);
        let content = match export_session(session, format) {
            Ok(c) => c,
            Err(e) => {
                self.add_system_message(&format!("Export failed: {}", e));
                return Ok(());
            }
        };

        let export_dir = session.storage().base_dir().join(".exports");
        if std::fs::symlink_metadata(&export_dir).is_ok_and(|m| m.file_type().is_symlink()) {
            self.add_system_message("Export directory must not be a symbolic link");
            return Ok(());
        }
        if let Err(error) = std::fs::create_dir_all(&export_dir) {
            self.add_system_message(&format!("Export failed: {error}"));
            return Ok(());
        }
        let export_path = export_dir.join(&filename);

        self.add_system_message(&format!("Exporting session as {}...", format.name()));

        let export_path_clone = export_path.clone();

        let result = tokio::task::spawn_blocking(move || {
            super::local_workflows::write_export(&export_path_clone, &content)
        })
        .await;

        if let Some(last_msg) = self.app_state.messages.last()
            && last_msg.content.starts_with("Exporting session as")
        {
            self.app_state.messages.pop();
        }

        match result {
            Ok(Ok(())) => {
                tracing::info!(
                    path = %export_path.display(),
                    format = %format.name(),
                    "Session exported successfully"
                );
                self.add_system_message(&format!(
                    "Session exported as {} to:\n{}",
                    format.name(),
                    export_path.display()
                ));
            }
            Ok(Err(e)) => {
                tracing::error!(error = %e, "Export failed");
                self.add_system_message(&format!("Export failed: {}", e));
            }
            Err(e) => {
                tracing::error!(error = %e, "Export task failed");
                self.add_system_message(&format!("Export failed: {}", e));
            }
        }

        Ok(())
    }

    /// Handle providers list command
    fn handle_providers_list(&mut self) {
        let mut output = String::from("Provider:\n\n");

        let is_authenticated = cortex_login::has_valid_auth();

        if is_authenticated {
            output.push_str("  [x] Cortex - Authenticated\n\n");
        } else {
            output.push_str("  [ ] Cortex - Not authenticated\n\n");
            output.push_str("Run /login to authenticate.");
        }

        self.add_system_message(&output);
    }

    /// Handle session info command
    fn handle_session_info(&mut self) {
        if let Some(ref session) = self.cortex_session {
            let mut output = String::from("Session Info:\n");
            output.push_str(&format!("  ID: {}\n", session.id()));
            output.push_str(&format!("  Title: {}\n", session.title()));
            output.push_str(&format!("  Provider: {}\n", session.meta.provider));
            output.push_str(&format!("  Model: {}\n", session.meta.model));
            output.push_str(&format!("  Messages: {}\n", session.message_count()));
            output.push_str(&format!("  Tokens: {}\n", session.format_tokens()));
            output.push_str(&format!(
                "  Created: {}\n",
                session.meta.created_at.format("%Y-%m-%d %H:%M")
            ));
            self.add_system_message(&output);
        } else {
            self.add_system_message("No active session.");
        }
    }

    /// Handle transcript command
    pub(super) fn handle_transcript(&mut self) {
        if let Some(ref session) = self.cortex_session {
            let mut output = "=== Session Transcript ===\n".to_string();
            output.push_str(&format!("Title: {}\n", session.title()));
            output.push_str(&format!("Messages: {}\n\n", session.message_count()));

            for (i, msg) in session.messages().iter().enumerate() {
                let role = match msg.role.as_str() {
                    "user" => "User",
                    "assistant" => "Assistant",
                    "system" => "System",
                    _ => "Unknown",
                };
                output.push_str(&format!("[{}] {}:\n{}\n\n", i + 1, role, msg.content));
            }

            self.add_system_message(&output);
        } else {
            self.add_system_message("No active session.");
        }
    }

    /// Handle history command
    fn handle_history(&mut self) {
        let result = (|| -> Result<String> {
            let storage = self.session_storage()?;
            let mut output = String::from("Persisted prompt history:\n");
            for summary in storage.list_recent_sessions(20)? {
                for message in storage
                    .load_messages(&summary.id)?
                    .into_iter()
                    .filter(|m| m.is_user())
                {
                    output.push_str(&format!("{} | {}\n", summary.short_id(), message.content));
                }
            }
            Ok(output)
        })();
        match result {
            Ok(output) => self.add_system_message(&output),
            Err(error) => self.add_system_message(&format!("History unavailable: {error}")),
        }
    }

    /// Handle set value commands
    pub(super) fn handle_set_value(&mut self, key: &str, value: &str) {
        match key {
            "model" => {
                // Check validation result first, storing any error
                let validation_result: Result<(), String> = if let Some(pm) = &self.provider_manager
                    && let Ok(mut manager) = pm.try_write()
                {
                    manager.set_model(value).map_err(|e| e.to_string())
                } else {
                    Ok(())
                };

                // Handle validation result after releasing the borrow
                if let Err(e) = validation_result {
                    self.add_system_message(&format!("Cannot set model: {}", e));
                    return;
                }

                // Only update state if validation passed
                self.app_state.model = value.to_string();
                self.update_session_model(value);
                self.add_system_message(&format!("Model set to: {}", value));

                // Persist model selection to config
                if let Ok(mut config) = crate::providers::config::CortexConfig::load() {
                    let _ = config.save_last_model(&self.app_state.provider, value);
                }
            }
            "provider" => {
                // Check validation result first, storing any error and new model
                let validation_result: Result<Option<String>, String> = if let Some(pm) =
                    &self.provider_manager
                    && let Ok(mut manager) = pm.try_write()
                {
                    match manager.set_provider(value) {
                        Ok(()) => {
                            // Get updated model after provider switch
                            Ok(Some(manager.current_model().to_string()))
                        }
                        Err(e) => Err(e.to_string()),
                    }
                } else {
                    Ok(None)
                };

                // Handle validation result after releasing the borrow
                match validation_result {
                    Err(e) => {
                        self.add_system_message(&format!("Cannot set provider: {}", e));
                        return;
                    }
                    Ok(Some(new_model)) => {
                        // Update model to reflect any changes made during provider switch
                        self.app_state.model = new_model;
                    }
                    Ok(None) => {}
                }

                self.app_state.provider = value.to_string();
                self.add_system_message(&format!("Provider set to: {}", value));
            }
            "session_name" => {
                let result = self.change_session_metadata(key, value);
                self.report_local_result(result, "Session renamed");
            }
            "favorite" | "protected" => {
                let result = self.change_session_metadata(key, value);
                self.report_local_result(result, "Session setting saved");
            }
            "rewind" => {
                let result = value
                    .parse::<usize>()
                    .map_err(anyhow::Error::from)
                    .and_then(|turns| self.rewind_conversation(turns));
                self.report_local_result(result, "");
            }

            "temperature" => {
                if let Ok(temp) = value.parse::<f32>() {
                    let temp = temp.clamp(0.0, 2.0);
                    self.app_state.temperature = temp;
                    self.app_state
                        .toasts
                        .success(format!("Temperature: {:.1}", temp));
                } else {
                    self.app_state.toasts.error("Invalid temperature value");
                }
            }
            "sandbox" => {
                self.app_state.sandbox_mode = value == "true" || value == "on";
                let state = if self.app_state.sandbox_mode {
                    "on"
                } else {
                    "off"
                };
                self.app_state
                    .toasts
                    .info(format!("Sandbox mode: {}", state));
            }
            "approval" => {
                self.app_state.permission_mode = match value.to_ascii_lowercase().as_str() {
                    "always" | "yolo" | "never" => crate::permissions::PermissionMode::Yolo,
                    "session" | "medium" => crate::permissions::PermissionMode::Medium,
                    "auto" | "low" => crate::permissions::PermissionMode::Low,
                    _ => crate::permissions::PermissionMode::High,
                };
                self.sync_permission_mode();
                self.app_state
                    .toasts
                    .info(format!("Permissions: {}", value));
            }
            _ => {
                self.add_system_message(&format!(
                    "Setting '{key}' is unsupported in this session. No setting was changed."
                ));
            }
        }
    }

    async fn build_skills_picker(&self) -> crate::interactive::InteractiveState {
        let items = discover_skills().await;
        crate::interactive::builders::build_skills_selector(&items)
    }

    pub(super) async fn invoke_skill_command(&mut self, cmd: &str) {
        let rest = cmd.strip_prefix("skill:invoke:").unwrap_or(cmd);
        let (name, args) = match rest.split_once(':') {
            Some((n, a)) => (n, Some(a)),
            None => (rest, None),
        };
        match load_skill_by_name(name).await {
            Ok(skill) => {
                self.add_system_message(&format!(
                    "Loaded skill /{}.\n\n{}",
                    skill.metadata.name, skill.content
                ));
                if let Some(args) = args.filter(|a| !a.is_empty()) {
                    self.add_system_message(&format!("Skill args: {args}"));
                }
                self.app_state
                    .toasts
                    .success(format!("Skill /{} loaded", skill.metadata.name));
            }
            Err(err) => {
                self.add_system_message(&format!("Skill /{name}: {err}"));
            }
        }
    }

    async fn handle_goal_command(&mut self, cmd: &str) -> Result<()> {
        use cortex_engine::goal::{GoalCommand, apply_command};

        let command = if cmd == "goal:status" {
            GoalCommand::Status
        } else if cmd == "goal:pause" {
            GoalCommand::Pause
        } else if cmd == "goal:resume" {
            GoalCommand::Resume
        } else if cmd == "goal:clear" {
            GoalCommand::Clear
        } else if let Some(objective) = cmd.strip_prefix("goal:set:") {
            let objective = objective.trim();
            if objective.is_empty() {
                self.add_system_message("Goal objective cannot be empty.");
                return Ok(());
            }
            GoalCommand::Set {
                objective: objective.to_string(),
            }
        } else {
            self.add_system_message("Unknown /goal action.");
            return Ok(());
        };

        self.reload_goal_from_session();
        match apply_command(self.app_state.goal.clone(), command.clone()) {
            Ok(next) => {
                let previous = self.app_state.goal.clone();
                self.app_state.goal = next;
                if !matches!(command, GoalCommand::Status) {
                    if let Err(error) = self.persist_app_goal() {
                        self.app_state.goal = previous;
                        self.add_system_message(&format!("Could not persist goal: {error}"));
                        return Ok(());
                    }
                }
                match command {
                    GoalCommand::Status => {
                        if let Some(goal) = &self.app_state.goal {
                            self.add_system_message(&goal.status_text());
                        } else {
                            self.add_system_message("No goal. Set one with /goal <objective>.");
                        }
                    }
                    GoalCommand::Pause => {
                        if let Some(goal) = &self.app_state.goal {
                            self.add_system_message(&goal.action_text("Goal paused."));
                        } else {
                            self.add_system_message("Goal paused.");
                        }
                    }
                    GoalCommand::Resume => {
                        if let Some(goal) = &self.app_state.goal {
                            self.add_system_message(
                                &goal.action_text(&format!("Goal is {}.", goal.state)),
                            );
                        }
                    }
                    GoalCommand::Clear => {
                        self.add_system_message("Goal cleared.");
                    }
                    GoalCommand::Set { objective } => {
                        if let Some(goal) = &self.app_state.goal {
                            self.add_system_message(
                                &goal.action_text(&format!("Goal set: {objective}.")),
                            );
                        } else {
                            self.add_system_message(&format!("Goal set: {objective}"));
                        }
                        self.send_text_message(cortex_engine::goal::kickoff_prompt(&objective))
                            .await?;
                    }
                }
            }
            Err(error) => self.add_system_message(&error),
        }
        Ok(())
    }

    fn persist_app_goal(&mut self) -> Result<()> {
        let session = self
            .cortex_session
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No session to persist the goal."))?;
        session.persist_goal(self.app_state.goal.as_ref())?;
        Ok(())
    }

    pub(super) fn reload_goal_from_session(&mut self) {
        let Some(session) = &self.cortex_session else {
            return;
        };
        match session.load_goal_report() {
            Ok(cortex_engine::goal::GoalLoad::Loaded(goal)) => {
                self.app_state.goal = Some(goal);
            }
            Ok(cortex_engine::goal::GoalLoad::Missing) => {
                self.app_state.goal = None;
            }
            Ok(cortex_engine::goal::GoalLoad::Quarantined { reason }) => {
                self.app_state.goal = None;
                self.add_system_message(&reason);
            }
            Err(error) => {
                self.add_system_message(&format!("Could not reload goal: {error}"));
            }
        }
    }

    pub(super) async fn maybe_continue_goal(&mut self, tokens: u64) -> bool {
        self.reload_goal_from_session();
        let Some(goal) = self.app_state.goal.as_mut() else {
            return false;
        };
        if goal.state != cortex_engine::goal::GoalState::Active {
            return false;
        }
        let cont = cortex_engine::goal::finish_turn(goal, tokens);
        if let Err(error) = self.persist_app_goal() {
            self.add_system_message(&format!("Could not persist goal: {error}"));
            self.reload_goal_from_session();
            return false;
        }
        if !cont {
            return false;
        }
        let prompt = cortex_engine::goal::continuation_prompt(
            self.app_state.goal.as_ref().expect("active goal"),
        );
        if let Err(error) = self.send_text_message(prompt).await {
            self.add_system_message(&format!("Could not continue goal: {error}"));
            return false;
        }
        true
    }
}

fn parse_usage_range(cmd: &str) -> (Option<String>, Option<String>) {
    let mut from = None;
    let mut to = None;
    for part in cmd.split(':') {
        if let Some(value) = part.strip_prefix("from=") {
            from = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("to=") {
            to = Some(value.to_string());
        }
    }
    (from, to)
}

fn cortex_home_and_project() -> (std::path::PathBuf, Option<std::path::PathBuf>) {
    let home = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".cortex");
    let project = std::env::current_dir().ok();
    (home, project)
}

async fn discover_skills() -> Vec<crate::interactive::builders::SkillListItem> {
    let (home, project) = cortex_home_and_project();
    let registry = cortex_engine::skills::SkillRegistry::new(&home, project.as_deref());
    registry
        .scan()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|skill| crate::interactive::builders::SkillListItem {
            name: skill.metadata.name,
            description: skill.metadata.description,
        })
        .collect()
}

async fn load_skill_by_name(name: &str) -> anyhow::Result<cortex_engine::skills::Skill> {
    let (home, project) = cortex_home_and_project();
    let registry = cortex_engine::skills::SkillRegistry::new(&home, project.as_deref());
    let skills = registry.scan().await?;
    skills
        .into_iter()
        .find(|s| s.metadata.name == name)
        .ok_or_else(|| anyhow::anyhow!("not found. Add SKILL.md under ~/.cortex/skills."))
}
