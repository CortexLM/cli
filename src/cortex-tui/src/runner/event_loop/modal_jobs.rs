//! `/jobs` picker Enter handling (numbered `>` / `·` chrome; not radios).

use super::core::EventLoop;
use crate::app::SubagentDisplayStatus;

impl EventLoop {
    pub(super) fn handle_jobs_picker_choice(&mut self, item_id: &str) -> bool {
        if item_id == "empty" || item_id.is_empty() {
            return false;
        }
        let attach = crate::interactive::builders::jobs::JOBS_ATTACH_PREFIX;
        let stop = crate::interactive::builders::jobs::JOBS_STOP_PREFIX;
        if let Some(id) = item_id.strip_prefix(attach) {
            self.app_state.toasts.info(format!(
                "Attach from this terminal: cortex attach {id} — empty line detaches, session keeps running"
            ));
            return false;
        }
        if let Some(id) = item_id.strip_prefix(stop) {
            self.stop_job(id);
            return false;
        }
        self.open_job(item_id);
        false
    }

    fn abort_job_task(&mut self, display_id: &str) {
        let tool_id = display_id.strip_prefix("subagent_").unwrap_or(display_id);
        for key in [tool_id, display_id] {
            if let Some(handle) = self.running_tool_tasks.remove(key) {
                handle.abort();
            }
            if let Some(handle) = self.running_subagents.remove(key) {
                handle.abort();
            }
        }
    }

    fn stop_job(&mut self, id: &str) {
        self.abort_job_task(id);
        let tool_id = id.strip_prefix("subagent_").unwrap_or(id);
        self.app_state.add_pending_tool_result(
            tool_id.to_string(),
            "Task".into(),
            "The job was stopped.".into(),
            false,
        );
        let found = self
            .app_state
            .active_subagents
            .iter()
            .any(|t| t.session_id == id);
        if found {
            self.app_state.update_subagent(id, |task| {
                task.status = SubagentDisplayStatus::Failed;
                task.error_message = Some("The job was stopped.".into());
                task.current_activity = "Stopped".into();
            });
            if !self.app_state.has_active_subagents() {
                self.app_state.streaming.stop_delegation();
            }
            self.app_state
                .toasts
                .info(format!("Stop requested for {id}. The job is not deleted."));
        } else {
            self.app_state.toasts.info(format!(
                "Stop from the CLI: cortex jobs stop {id}. The session is not deleted."
            ));
        }
    }

    fn open_job(&mut self, item_id: &str) {
        if let Some(task) = self
            .app_state
            .active_subagents
            .iter()
            .find(|task| task.session_id == item_id)
        {
            let preview = if task.output_preview.is_empty() {
                "(no output yet)".to_string()
            } else {
                task.output_preview.clone()
            };
            self.add_system_message(&format!(
                "Job {} · {}\nStatus: {}\n{preview}",
                task.session_id,
                task.description,
                task.status.description()
            ));
            return;
        }
        self.add_system_message(&format!(
            "Job {item_id} is not in this session. Live attach: cortex attach {item_id}"
        ));
    }

    /// Process pending actions from the card handler.
    pub(super) fn process_card_actions(&mut self) {
        let actions = self.card_handler.take_actions();
        for _action in actions {
            tracing::debug!("Card action received");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppState, SubagentTaskDisplay};
    use crate::runner::event_loop::EventLoop;
    use std::time::Duration;

    #[tokio::test]
    async fn x_aborts_running_job_task() {
        let mut event_loop = EventLoop::new(AppState::new());
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let _ = tx.send(());
        });
        event_loop
            .running_tool_tasks
            .insert("tool-1".into(), handle);
        event_loop
            .app_state
            .add_subagent_task(SubagentTaskDisplay::new(
                "subagent_tool-1",
                "tool-1",
                "rate-limiter",
                "subagent",
            ));
        event_loop.handle_jobs_picker_choice("stop:subagent_tool-1");
        assert!(event_loop.running_tool_tasks.is_empty());
        let outcome = tokio::time::timeout(Duration::from_millis(400), rx).await;
        assert!(
            matches!(outcome, Ok(Err(_)) | Err(_)),
            "stopped job must not keep running to completion"
        );
        let task = event_loop
            .app_state
            .active_subagents
            .iter()
            .find(|task| task.session_id == "subagent_tool-1")
            .expect("stopped job stays listed");
        assert!(matches!(task.status, SubagentDisplayStatus::Failed));
        assert!(
            event_loop
                .app_state
                .pending_tool_results
                .iter()
                .any(|result| result.tool_call_id == "tool-1" && !result.success)
        );
    }

    #[test]
    fn enter_opens_job_details() {
        let mut event_loop = EventLoop::new(AppState::new());
        let mut task = SubagentTaskDisplay::new(
            "subagent_tool-2",
            "tool-2",
            "summarize lockfile",
            "subagent",
        );
        task.output_preview = "diff hunk".into();
        event_loop.app_state.add_subagent_task(task);
        event_loop.handle_jobs_picker_choice("subagent_tool-2");
        let last = event_loop
            .app_state
            .messages
            .last()
            .expect("open posts job details");
        assert!(last.content.contains("summarize lockfile"), "{last:?}");
        assert!(last.content.contains("diff hunk"), "{last:?}");
    }
}
