//! Fast-mode command handling on [`EventLoop`].
//!
//! Split out of `event_loop/commands.rs` so the dispatch tables there stay
//! under the source-policy line-count target. The policy check happens here,
//! where the session state lives, and fails closed: when the organization
//! disabled fast mode the session stays on Standard and nothing is re-sent.

use super::core::EventLoop;
use cortex_engine::fast_mode::FastMode;

impl EventLoop {
    /// Apply a fast-mode request against organization policy.
    ///
    /// Fail-closed: when the organization disabled fast mode the session stays
    /// on Standard, both product toasts are shown, and nothing is re-sent.
    pub(super) fn apply_fast_mode(&mut self, requested: FastMode) {
        let policy = cortex_engine::fast_mode::current_policy();
        self.app_state.fast_mode_policy = policy;
        let outcome = cortex_engine::fast_mode::apply_fast_mode(requested, policy);
        if outcome.refused() {
            // Fail-closed: the session stays on Standard and nothing is re-sent.
            self.app_state.fast_mode = FastMode::Standard;
            for toast in outcome.toasts() {
                self.app_state.toasts.warning(toast);
            }
            return;
        }
        self.app_state.fast_mode = requested;
        // Subsequent turns read `app_state.fast_mode` in
        // `handle_submit_with_provider` and pass it through `CodeTurnContext`.
        for toast in outcome.toasts() {
            self.app_state.toasts.info(toast);
        }
    }

    /// Flip fast mode for the `/fast` toggle form.
    pub(super) fn toggle_fast_mode(&mut self) {
        let requested = if self.app_state.fast_mode.is_on() {
            FastMode::Standard
        } else {
            FastMode::Fast
        };
        self.apply_fast_mode(requested);
    }

    /// `/fast <value>` — reject an unknown token instead of guessing.
    pub(super) fn set_fast_mode(&mut self, value: &str) {
        match FastMode::parse(value) {
            Ok(mode) => self.apply_fast_mode(mode),
            Err(error) => {
                self.app_state.toasts.error(error.to_string());
            }
        }
    }
}
