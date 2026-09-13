//! `/jobs` picker Enter handling (numbered `>` / `·` chrome; not radios).

use super::core::EventLoop;

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
            if self.app_state.remove_subagent(id).is_some() {
                self.app_state
                    .toasts
                    .info(format!("Stop requested for {id}. The job is not deleted."));
            } else {
                self.app_state.toasts.info(format!(
                    "Stop from the CLI: cortex jobs stop {id}. The session is not deleted."
                ));
            }
            return false;
        }
        self.app_state.toasts.info(format!(
            "Job {item_id} — Enter opens, a attaches, x cancels. Live attach: cortex attach {item_id}"
        ));
        false
    }

    /// Process pending actions from the card handler.
    pub(super) fn process_card_actions(&mut self) {
        let actions = self.card_handler.take_actions();
        for _action in actions {
            tracing::debug!("Card action received");
        }
    }
}
