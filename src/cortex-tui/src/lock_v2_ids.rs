//! Lock v2 scene id lists. Split out of [`crate::lock_v2`] so adding `/goal`
//! boards does not grow that file past the source-policy line-count baseline.

/// Narrow (40×12) SPEC §7 set — 41 boards.
pub const LOCK_V2_NARROW_IDS: &[&str] = &[
    "welcome-cortex",
    "welcome-agent",
    "first-run-tips",
    "session-empty",
    "session-user-bars",
    "session-thinking-live",
    "session-assistant",
    "session-optin",
    "composer-empty",
    "composer-typing",
    "composer-hover",
    "tokens-topright",
    "compact-chat",
    "slash-palette",
    "goal-chip-active",
    "goal-chip-paused",
    "goal-chip-done",
    "goal-chip-budget",
    "goal-chip-blocked",
    "slash-model-typed",
    "model-list",
    "model-effort-high",
    "settings-appearance",
    "settings-mouse",
    "settings-row-hover",
    "settings-theme-submenu",
    "mode-plan",
    "mode-ask",
    "permission-prompt",
    "mcp-servers",
    "usage",
    "diagnostics",
    "interrupt-stopped",
    "offline",
    "rate-limit",
    "diff-hunk",
    "login",
    "shortcuts-overlay",
    "consent-local-tools",
    "composer-file-chip",
    "undo-sheet",
];

/// Wide (120×40) SPEC §7 set — 87 boards.
pub const LOCK_V2_WIDE_IDS: &[&str] = &[
    "welcome-cortex",
    "welcome-agent",
    "first-run-tips",
    "session-empty",
    "session-user-bars",
    "session-thought",
    "session-thought-expanded",
    "session-thinking-live",
    "session-assistant",
    "session-worked",
    "session-optin",
    "session-optin-hover",
    "composer-empty",
    "composer-typing",
    "composer-typing-blink",
    "composer-hover",
    "composer-multiline",
    "footer-shortcuts",
    "footer-hover",
    "tokens-topright",
    "tokens-topright-warn",
    "compact-chat",
    "slash-palette",
    "goal-chip-active",
    "goal-chip-paused",
    "goal-chip-done",
    "goal-chip-budget",
    "goal-chip-blocked",
    "slash-model-typed",
    "model-list",
    "model-list-hover",
    "model-effort-high",
    "model-effort-medium",
    "model-effort-low",
    "model-effort-hover",
    "settings-appearance",
    "settings-mouse",
    "settings-row-hover",
    "settings-search",
    "settings-theme-submenu",
    "mode-agent",
    "mode-plan",
    "mode-ask",
    "mode-bash",
    "permission-prompt",
    "permission-prompt-hover",
    "permissions-picker",
    "mcp-servers",
    "mcp-drop",
    "plugins",
    "usage",
    "quota-exhausted",
    "sandbox",
    "sandbox-deny",
    "cloud-handoff",
    "diagnostics",
    "interrupt-stopped",
    "error-unavailable",
    "offline",
    "rate-limit",
    "tool-tiles",
    "tool-tiles-collapsed",
    "shell-running",
    "diff-hunk",
    "edit-collapsed",
    "md-table",
    "code-fence",
    "login",
    "login-waiting",
    "login-success",
    "login-error",
    "shortcuts-overlay",
    "resume-picker",
    "clear-confirm",
    "plan-confirm",
    "queue",
    "files-picker",
    "jobs",
    "skills",
    "todos",
    "question",
    "sudo",
    "config-tree",
    "btw",
    "consent-local-tools",
    "composer-file-chip",
    "undo-sheet",
];

/// Boards captured at both sizes. Narrow (40×12) is a subset.
pub fn lock_v2_scene_ids(width: u16) -> &'static [&'static str] {
    if width <= 40 {
        LOCK_V2_NARROW_IDS
    } else {
        LOCK_V2_WIDE_IDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn lock_v2_id_counts_and_unique() {
        assert_eq!(LOCK_V2_WIDE_IDS.len(), 87);
        assert_eq!(LOCK_V2_NARROW_IDS.len(), 41);
        let mut wide = HashSet::new();
        for id in LOCK_V2_WIDE_IDS {
            assert!(wide.insert(*id), "duplicate wide id {id}");
        }
        let mut narrow = HashSet::new();
        for id in LOCK_V2_NARROW_IDS {
            assert!(narrow.insert(*id), "duplicate narrow id {id}");
            assert!(
                wide.contains(id),
                "narrow id {id} is not in LOCK_V2_WIDE_IDS"
            );
        }
    }
}
