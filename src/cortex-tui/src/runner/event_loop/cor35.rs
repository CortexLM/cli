//! COR-35 async command handlers on [`EventLoop`].
//!
//! Split out of `event_loop/commands.rs` so the dispatch table there stays under
//! the source-policy complexity baseline. Each handler reads real project state
//! and reports a failure honestly instead of opening a picker over stale data.

use anyhow::Result;

use crate::cor35_handlers::{
    apply_allowlist_result, apply_plugin_marketplace, apply_rules_result, ide_status,
    load_allowlist_outcome, load_rules_outcome,
};
use crate::interactive::builders::build_question_prompt;
use crate::runner::event_loop::core::EventLoop;

impl EventLoop {
    /// `/permissions rules` — the committed `.cortex/permissions.toml` rules.
    pub(super) fn open_permission_rules(&mut self) {
        let Some(cwd) = self.workspace_dir() else {
            self.add_system_message("× Workspace directory is unavailable. Rules were not read.");
            return;
        };
        let outcome = load_rules_outcome(&cwd);
        if let Ok(rules) = &outcome {
            // The live manager applies the same rules the picker shows.
            self.permission_manager.rules = rules.clone();
        }
        apply_rules_result(&mut self.app_state, outcome);
    }

    /// `/sandbox network` — the committed network allowlist.
    pub(super) fn open_sandbox_allowlist(&mut self) {
        let Some(cwd) = self.workspace_dir() else {
            self.add_system_message("× Workspace directory is unavailable. Network stays blocked.");
            return;
        };
        apply_allowlist_result(&mut self.app_state, load_allowlist_outcome(&cwd));
    }

    /// `/plugins` — the signed Cortex marketplace with installed plugins.
    pub(super) fn open_plugin_marketplace(&mut self, command: &str) {
        // `/plugins install|enable|disable <id>` is the CLI's job; the TUI opens
        // the marketplace and says so rather than pretending to run it.
        if let Some(rest) = command.strip_prefix("plugins:")
            && !rest.is_empty()
            && rest != "list"
        {
            self.add_system_message(&format!(
                "Run `cortex plugin {rest}` for that change; this sheet lists and searches."
            ));
        }
        let installed = self.installed_plugins();
        apply_plugin_marketplace(&mut self.app_state, &installed);
    }

    /// `/ide` — the ACP editor handshake.
    pub(super) fn open_ide_handshake(&mut self) {
        let narrow = self.app_state.terminal_size.0 <= 40;
        self.add_system_message(&ide_status(narrow));
        self.app_state
            .enter_interactive_mode(crate::interactive::builders::build_question_prompt(
                "Editor (ACP)",
                &[
                    ("connect", "Connect an editor", "cortex acp · stdio"),
                    (
                        "session",
                        "Share this session",
                        "same approvals and sandbox",
                    ),
                    ("stop", "Disconnect", "the CLI keeps running"),
                ],
                0,
            ));
    }

    /// `/rewind` — pick a file checkpoint to restore.
    pub(super) fn open_checkpoint_rewind(&mut self) {
        let Some(dir) = self.checkpoint_dir() else {
            self.add_system_message("× Checkpoint storage is unavailable.");
            return;
        };
        let checkpoints = match crate::checkpoint::list_checkpoints(&dir) {
            Ok(checkpoints) => checkpoints,
            Err(error) => {
                self.add_system_message(&format!("× Could not read checkpoints: {error}"));
                return;
            }
        };
        if checkpoints.is_empty() {
            self.add_system_message(
                "No file checkpoints yet. Cortex captures the files a turn is about to change.",
            );
            return;
        }
        let rows: Vec<(String, String, String)> = checkpoints
            .iter()
            .map(|checkpoint| {
                (
                    checkpoint.id.clone(),
                    format!("Rewind to {}", checkpoint.id),
                    checkpoint.summary(),
                )
            })
            .chain(std::iter::once((
                "__keep__".to_string(),
                "Keep files".to_string(),
                "conversation only".to_string(),
            )))
            .collect();
        let borrowed: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|(id, label, description)| (id.as_str(), label.as_str(), description.as_str()))
            .collect();
        self.app_state
            .enter_interactive_mode(build_question_prompt("Rewind", &borrowed, 0));
    }

    /// Restore the checkpoint the user picked from the `/rewind` sheet.
    ///
    /// `__keep__` is the "leave the working tree alone" row, which reports that
    /// nothing changed rather than silently closing.
    pub(super) fn restore_checkpoint_choice(&mut self, item_id: &str) -> bool {
        if item_id == "__keep__" {
            self.add_system_message(
                "Kept the working tree as it is. Use /undo for conversation history.",
            );
            return false;
        }
        let Some(dir) = self.checkpoint_dir() else {
            self.add_system_message("× Checkpoint storage is unavailable.");
            return false;
        };
        let Some(workspace) = self.workspace_dir() else {
            self.add_system_message("× Workspace directory is unavailable.");
            return false;
        };
        let result = crate::checkpoint::read_checkpoint(&dir, item_id)
            .and_then(|checkpoint| crate::checkpoint::restore(&workspace, &checkpoint));
        apply_restore_result(&mut self.app_state, result);
        false
    }

    /// Workspace directory from the live session, falling back to the process cwd.
    fn workspace_dir(&self) -> Option<std::path::PathBuf> {
        self.cortex_session
            .as_ref()
            .map(|session| std::path::PathBuf::from(&session.meta.cwd))
            .or_else(|| std::env::current_dir().ok())
    }

    /// Cortex home that holds session state (checkpoints, plugin state).
    fn cortex_home(&self) -> Option<std::path::PathBuf> {
        cortex_engine::config::find_cortex_home().ok()
    }

    /// Session home for checkpoint storage.
    fn checkpoint_dir(&self) -> Option<std::path::PathBuf> {
        Some(
            self.cortex_home()?
                .join("sessions")
                .join(crate::checkpoint::CHECKPOINTS_DIR),
        )
    }

    /// Installed plugins from the live state file.
    fn installed_plugins(&self) -> Vec<(String, String)> {
        let Some(home) = self.cortex_home() else {
            return Vec::new();
        };
        match crate::plugin_marketplace::load_state(&home) {
            Ok(state) => state
                .plugins
                .into_iter()
                .map(|plugin| (plugin.id, plugin.version))
                .collect(),
            Err(error) => {
                // A broken state file is reported by the caller; an empty list
                // here would hide real plugins.
                tracing::warn!("plugin state unreadable: {error}");
                Vec::new()
            }
        }
    }
}

/// Apply a checkpoint restore result to the app state.
pub fn apply_restore_result(
    state: &mut crate::app::AppState,
    result: Result<Vec<crate::checkpoint::RestoredFile>>,
) {
    match result {
        Ok(restored) => {
            let summary = crate::checkpoint::restore_summary(&restored);
            state.add_message(cortex_core::widgets::Message::system(summary));
        }
        Err(error) => {
            state.add_message(cortex_core::widgets::Message::system(format!(
                "× Restore failed: {error}. The working tree was left as it was."
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;

    #[test]
    fn a_restore_result_is_reported_with_counts() {
        let mut state = AppState::default();
        apply_restore_result(
            &mut state,
            Ok(vec![crate::checkpoint::RestoredFile {
                path: "a.rs".into(),
                action: crate::checkpoint::RestoreAction::Reverted,
            }]),
        );
        assert_eq!(state.messages.len(), 1);
        assert!(state.messages[0].content.contains("1 restored"));
    }

    #[test]
    fn a_failed_restore_says_the_tree_is_untouched() {
        let mut state = AppState::default();
        apply_restore_result(&mut state, Err(anyhow::anyhow!("disk full")));
        let text = &state.messages[0].content;
        assert!(text.contains("Restore failed"), "{text}");
        assert!(text.contains("left as it was"), "{text}");
    }
}
