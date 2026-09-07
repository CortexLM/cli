//! One local-session transition path for typed, palette, keyboard and mouse UX.
use anyhow::{Context, Result, bail};
use cortex_core::widgets::{Message, MessageRole};

use super::core::EventLoop;
use crate::app::AppView;
use crate::session::{CortexSession, SessionStorage};

impl EventLoop {
    pub(super) fn session_storage(&self) -> Result<SessionStorage> {
        self.cortex_session
            .as_ref()
            .map(|s| s.storage().clone())
            .map_or_else(SessionStorage::new, Ok)
    }

    fn ensure_session_idle(&self) -> Result<()> {
        if self.app_state.is_busy()
            || self
                .streaming_task
                .as_ref()
                .is_some_and(|t| !t.is_finished())
            || !self.running_tool_tasks.is_empty()
            || !self.running_subagents.is_empty()
        {
            bail!("Stop the active turn before changing sessions");
        }
        if self.session_bridge.is_some() {
            bail!("In-session switching is unavailable in legacy mode; restart with cortex resume");
        }
        Ok(())
    }

    fn flush_session_before_switch(&mut self) -> Result<()> {
        if let Some(session) = self.cortex_session.as_mut()
            && session.is_modified()
        {
            session
                .save()
                .context("Current session has unsaved changes; fix the storage error and retry")?;
        }
        Ok(())
    }

    pub(super) fn report_persistence_error(&mut self) {
        if let Some(error) = self
            .cortex_session
            .as_mut()
            .and_then(|session| session.take_persistence_error())
        {
            self.add_system_message(&error);
        }
    }

    pub(super) fn resume_local_session(&mut self, id: &str) -> Result<()> {
        self.ensure_session_idle()?;
        self.flush_session_before_switch()?;
        let session = CortexSession::load_with_storage(id, self.session_storage()?)?;
        self.activate_local_session(session)?;
        Ok(())
    }

    pub(super) fn new_local_session(&mut self) -> Result<()> {
        self.ensure_session_idle()?;
        self.flush_session_before_switch()?;
        let storage = self.session_storage()?;
        let mut session =
            CortexSession::with_storage(&self.app_state.provider, &self.app_state.model, storage)?;
        if let Some(current) = &self.cortex_session {
            session.meta.cwd = current.meta.cwd.clone();
            session.save()?;
        }
        self.activate_local_session(session)?;
        self.session_redo.clear();
        Ok(())
    }

    pub(super) fn fork_local_session(&mut self, message: Option<&str>) -> Result<()> {
        self.ensure_session_idle()?;
        self.flush_session_before_switch()?;
        let session = self
            .cortex_session
            .as_ref()
            .context("No active session to fork")?;
        let fork = session.fork(message)?;
        self.activate_local_session(fork)
    }

    pub(super) fn activate_local_session(&mut self, session: CortexSession) -> Result<()> {
        self.ensure_session_idle()?;
        self.flush_session_before_switch()?;
        if let Some(current) = &self.cortex_session
            && std::path::Path::new(&current.meta.cwd) != std::path::Path::new(&session.meta.cwd)
        {
            bail!(
                "This session belongs to another workspace. Restart with cortex resume {} to switch safely",
                session.id()
            );
        }
        // Dropping the retained client discards hidden remote-turn state. Replay
        // uses the full persisted local transcript; no remote ID equivalence.
        if let Some(manager) = &self.provider_manager {
            let mut manager = manager.try_write().context("Session client is busy")?;
            manager.set_model(&session.meta.model)?;
        }
        self.app_state.clear_messages();
        self.app_state.clear_message_queue();
        self.app_state.tool_calls.clear();
        self.app_state.pending_tool_results.clear();
        self.app_state.active_subagents.clear();
        self.app_state.viewing_subagent = None;
        self.app_state.context_files.clear();
        self.app_state.streaming = Default::default();
        self.app_state.typewriter = None;
        self.app_state.tokens_used = session.total_tokens().max(0) as u64;
        self.app_state.context_percent = 0;
        self.streaming_cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.app_state.pending_approval = None;
        self.pending_assistant_tool_calls.clear();
        self.tool_execution_started = false;
        self.is_continuation = false;
        self.streaming_rx = None;
        self.streaming_task = None;
        self.stream_done_received = false;
        self.stream_controller.reset();
        self.permission_manager = crate::permissions::PermissionManager::new();
        self.sync_permission_mode();
        // Discard completions left by the previous turn before accepting input.
        if let Some(rx) = self.tool_event_rx.as_mut() {
            while rx.try_recv().is_ok() {}
        }
        self.app_state.session_id = uuid::Uuid::parse_str(session.id()).ok();
        self.app_state.provider = session.meta.provider.clone();
        self.app_state.model = session.meta.model.clone();
        for stored in session.messages() {
            let role = match stored.role.as_str() {
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                "tool" => MessageRole::Tool,
                _ => MessageRole::System,
            };
            self.app_state.add_message(Message {
                role,
                content: stored.content.clone(),
                timestamp: None,
                is_streaming: false,
                tool_name: None,
                thought_secs: None,
                worked_secs: None,
            });
        }
        self.cortex_session = Some(session);
        self.app_state.set_view(AppView::Session);
        Ok(())
    }

    pub(super) fn change_session_metadata(&mut self, key: &str, value: &str) -> Result<()> {
        let session = self.cortex_session.as_mut().context("No active session")?;
        let mut meta = session.meta.clone();
        match key {
            "session_name" => meta.title = Some(value.to_string()),
            "favorite" => meta.favorite = value == "true",
            "protected" => meta.protected = value == "true",
            _ => bail!("Unknown session setting"),
        }
        meta.touch();
        session.persist_metadata(meta)
    }

    /// Conversation-only recovery creates a branch, never rewrites user files.
    // ponytail: file restore requires conflict-aware, verified workspace snapshots;
    // until then, undo/redo never modify the working tree.
    pub(super) fn rewind_conversation(&mut self, turns: usize) -> Result<()> {
        self.ensure_session_idle()?;
        self.flush_session_before_switch()?;
        if turns == 0 {
            bail!("Rewind requires at least one turn");
        }
        let session = self.cortex_session.as_ref().context("No active session")?;
        let users = session
            .messages()
            .iter()
            .enumerate()
            .filter(|(_, m)| m.is_user())
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let index = *users
            .get(users.len().checked_sub(turns).context("Not enough turns")?)
            .context("Not enough turns")?;
        let original = session.id().to_string();
        if index == 0 {
            self.new_local_session()?;
        } else {
            let cutoff = session.messages()[index - 1].id.clone();
            self.fork_local_session(Some(&cutoff))?;
        }
        self.session_redo.push(original);
        self.add_system_message("Conversation rewound into a new session. Original history and workspace files are unchanged. /redo returns to the original.");
        Ok(())
    }

    pub(super) fn redo_conversation(&mut self) -> Result<()> {
        let id = self.session_redo.last().context("Nothing to redo")?.clone();
        self.resume_local_session(&id)?;
        self.session_redo.pop();
        self.add_system_message(
            "Original conversation restored. Workspace files were not changed.",
        );
        Ok(())
    }

    pub(super) fn report_local_result(&mut self, result: Result<()>, success: &str) {
        match result {
            Ok(()) if !success.is_empty() => self.add_system_message(success),
            Ok(()) => {}
            Err(error) => self.add_system_message(&format!("Error: {error}")),
        }
    }

    /// Startup uses the same submission path once all startup gates have passed.
    pub async fn submit_initial_prompt(&mut self, prompt: String) -> Result<()> {
        if !prompt.trim().is_empty() {
            self.send_text_message(prompt).await?;
        }
        Ok(())
    }
}
