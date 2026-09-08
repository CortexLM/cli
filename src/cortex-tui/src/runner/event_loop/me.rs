//! Off-render-path `GET /v1/me` apply for the TUI event loop.

use tokio::task::JoinHandle;

use super::core::EventLoop;

impl EventLoop {
    /// Attach a background `GET /v1/me` so identity can land after the first frame.
    pub fn with_me_profile_task(
        mut self,
        task: JoinHandle<Option<cortex_engine::client::MeProfile>>,
    ) -> Self {
        self.me_profile_task = Some(task);
        self
    }

    /// Apply a finished `/v1/me` fetch without waiting on the render path.
    pub(super) async fn apply_pending_me_profile(&mut self) -> bool {
        let Some(handle) = self.me_profile_task.take() else {
            return false;
        };
        if !handle.is_finished() {
            self.me_profile_task = Some(handle);
            return false;
        }
        match handle.await {
            Ok(Some(profile)) => {
                self.app_state.apply_me_profile(profile);
                true
            }
            Ok(None) => false,
            Err(e) => {
                tracing::debug!("User info task ended: {e}");
                false
            }
        }
    }
}
