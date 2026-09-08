//! Builders for approval mode, permission prompts, and related confirms.

use crate::app::ApprovalState;
use crate::interactive::state::{InteractiveAction, InteractiveItem, InteractiveState};

/// Composer placeholder while a §3.10 prompt owns focus.
pub const PERMISSION_PROMPT_PLACEHOLDER: &str = "Choose an option above";

/// Interactive title for the exec permission prompt (footer → Approval).
pub const PERMISSION_PROMPT_TITLE: &str = "Approve command";

/// Custom action id for the inline permission prompt.
pub const PERMISSION_PROMPT_ACTION: &str = "permission-prompt";

/// SPEC §3.10 option 1.
pub const PERMISSION_ONCE_LABEL: &str = "1 Yes, run once";
/// SPEC §3.10 option 3.
pub const PERMISSION_EDIT_LABEL: &str = "3 Edit command";
/// SPEC §3.10 option 4 (em dash).
pub const PERMISSION_NO_LABEL: &str = "4 No — tell Cortex what to do instead";

/// Build an interactive state for approval mode selection.
pub fn build_approval_selector(current: Option<&str>) -> InteractiveState {
    let items = vec![
        InteractiveItem::new("ask", "Ask")
            .with_icon('?')
            .with_shortcut('a')
            .with_description("Ask for each tool call")
            .with_current(current == Some("ask")),
        InteractiveItem::new("session", "Session")
            .with_icon('S')
            .with_shortcut('s')
            .with_description("Remember choice for this session")
            .with_current(current == Some("session")),
        InteractiveItem::new("always", "Always")
            .with_icon('Y')
            .with_shortcut('y')
            .with_description("Always approve automatically")
            .with_current(current == Some("always")),
        InteractiveItem::new("never", "Never")
            .with_icon('N')
            .with_shortcut('n')
            .with_description("Never approve (reject all)")
            .with_current(current == Some("never")),
    ];

    InteractiveState::new("Approval Mode", items, InteractiveAction::SetApprovalMode)
}

/// `/permissions` picker — lock `permissions-picker` copy (SPEC §3.9 radios).
pub fn build_permissions_picker(current: Option<&str>) -> InteractiveState {
    let current = current.unwrap_or("smart").to_ascii_lowercase();
    let items = vec![
        InteractiveItem::new("ro", "Read-only")
            .with_description("never edit files or run commands")
            .with_current(current == "ro" || current == "ask")
            .with_shortcut('1'),
        InteractiveItem::new("smart", "Smart")
            .with_description("ask before leaving the sandbox")
            .with_current(current == "smart" || current == "medium")
            .with_shortcut('2'),
        InteractiveItem::new("full", "Full access")
            .with_description("only ask when leaving the sandbox")
            .with_current(current == "full" || current == "auto" || current == "yolo")
            .with_shortcut('3'),
    ];
    InteractiveState::new(
        "Permissions",
        items,
        InteractiveAction::Custom("permissions-picker".into()),
    )
}

/// Command string shown on the gray `$` row and used for “always allow …”.
pub fn permission_command_line(approval: &ApprovalState) -> String {
    if let Some(json) = &approval.tool_args_json {
        if let Some(cmd) = json.get("command") {
            if let Some(s) = cmd.as_str() {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
            if let Some(arr) = cmd.as_array() {
                let parts: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                if !parts.is_empty() {
                    return parts.join(" ");
                }
            }
        }
    }
    if !approval.tool_name.is_empty()
        && !matches!(
            approval.tool_name.to_ascii_lowercase().as_str(),
            "shell" | "bash" | "exec"
        )
    {
        return approval.tool_name.clone();
    }
    String::new()
}

/// First two tokens of a command, used in “always allow {snippet} in this project”.
pub fn always_allow_snippet(command: &str) -> String {
    let tokens: Vec<&str> = command.split_whitespace().take(2).collect();
    if tokens.is_empty() {
        "this command".into()
    } else {
        tokens.join(" ")
    }
}

/// SPEC §3.10 option 2 label for this command.
pub fn permission_always_label(command: &str) -> String {
    format!(
        "2 Yes, always allow {} in this project",
        always_allow_snippet(command)
    )
}

/// Inline numbered radios for a pending tool approval (SPEC §3.10).
pub fn build_permission_prompt(approval: &ApprovalState) -> InteractiveState {
    let command = permission_command_line(approval);
    let always = permission_always_label(&command);
    let items = vec![
        InteractiveItem::new("once", PERMISSION_ONCE_LABEL)
            .with_description("run this command once")
            .with_shortcut('1'),
        InteractiveItem::new("always", always)
            .with_description("remember for this project")
            .with_shortcut('2'),
        InteractiveItem::new("edit", PERMISSION_EDIT_LABEL)
            .with_description("edit before running")
            .with_shortcut('3'),
        InteractiveItem::new("no", PERMISSION_NO_LABEL)
            .with_description("reject")
            .with_shortcut('4'),
    ];
    InteractiveState::new(
        PERMISSION_PROMPT_TITLE,
        items,
        InteractiveAction::Custom(PERMISSION_PROMPT_ACTION.into()),
    )
    .with_prompt_focus()
}

/// Sandbox deny radios (lock `sandbox-deny`).
pub fn build_sandbox_deny_prompt() -> InteractiveState {
    let items = vec![
        InteractiveItem::new("retry", "1 Retry inside the sandbox")
            .with_description("stay in workspace")
            .with_shortcut('1'),
        InteractiveItem::new("allow", "2 Allow this domain")
            .with_description("ask next time")
            .with_shortcut('2'),
        InteractiveItem::new("cancel", "3 Cancel")
            .with_description("do not run")
            .with_shortcut('3'),
    ];
    InteractiveState::new(
        "Sandbox blocked",
        items,
        InteractiveAction::Custom("sandbox-deny".into()),
    )
    .with_prompt_focus()
}

/// One-shot question radios (lock `question`).
pub fn build_question_prompt(
    title: &str,
    rows: &[(&str, &str, &str)],
    selected: usize,
) -> InteractiveState {
    let items = rows
        .iter()
        .enumerate()
        .map(|(i, (id, label, desc))| {
            let shortcut = char::from_digit((i + 1) as u32, 10);
            let mut item = InteractiveItem::new(*id, *label).with_description(*desc);
            if let Some(s) = shortcut {
                item = item.with_shortcut(s);
            }
            item
        })
        .collect();
    let mut state =
        InteractiveState::new(title, items, InteractiveAction::Custom("question".into()))
            .with_prompt_focus();
    if !state.items.is_empty() {
        state.selected = selected.min(state.items.len() - 1);
    }
    state
}

/// Plan-mode confirm (lock `plan-confirm`).
pub fn build_plan_confirm() -> InteractiveState {
    let items = vec![
        InteractiveItem::new("yes", "1 Yes, implement")
            .with_description("switch to Agent and execute")
            .with_shortcut('1'),
        InteractiveItem::new("no", "2 Not yet")
            .with_description("stay in Plan")
            .with_shortcut('2'),
    ];
    InteractiveState::new(
        "Implement this plan?",
        items,
        InteractiveAction::Custom("plan-confirm".into()),
    )
    .with_prompt_focus()
}

/// `/clear` confirm (lock `clear-confirm`).
pub fn build_clear_confirm() -> InteractiveState {
    let items = vec![
        InteractiveItem::new("yes", "1 Clear")
            .with_description("wipe this thread, keep the workspace")
            .with_shortcut('1'),
        InteractiveItem::new("no", "2 Keep")
            .with_description("leave messages in place")
            .with_shortcut('2'),
    ];
    InteractiveState::new(
        "Clear conversation?",
        items,
        InteractiveAction::Custom("clear-confirm".into()),
    )
    .with_prompt_focus()
}

/// Build an interactive state for log level selection.
pub fn build_log_level_selector(current: Option<&str>) -> InteractiveState {
    let items = vec![
        InteractiveItem::new("trace", "Trace")
            .with_icon('T')
            .with_shortcut('t')
            .with_description("Most verbose - all details")
            .with_current(current == Some("trace")),
        InteractiveItem::new("debug", "Debug")
            .with_icon('D')
            .with_shortcut('d')
            .with_description("Debug information")
            .with_current(current == Some("debug")),
        InteractiveItem::new("info", "Info")
            .with_icon('I')
            .with_shortcut('i')
            .with_description("General information")
            .with_current(current == Some("info")),
        InteractiveItem::new("warn", "Warn")
            .with_icon('W')
            .with_shortcut('w')
            .with_description("Warnings only")
            .with_current(current == Some("warn")),
        InteractiveItem::new("error", "Error")
            .with_icon('E')
            .with_shortcut('e')
            .with_description("Errors only")
            .with_current(current == Some("error")),
    ];

    InteractiveState::new("Log Level", items, InteractiveAction::SetLogLevel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_approval_selector() {
        let state = build_approval_selector(Some("ask"));
        assert_eq!(state.items.len(), 4);
        assert!(state.items[0].is_current);
    }

    #[test]
    fn test_build_log_level_selector() {
        let state = build_log_level_selector(Some("info"));
        assert_eq!(state.items.len(), 5);
        assert!(state.items[2].is_current); // info is at index 2
    }

    #[test]
    fn permission_prompt_uses_spec_copy_and_shortcuts() {
        let approval = ApprovalState::new(
            "shell".into(),
            serde_json::json!({
                "command": "npm install ioredis && npm install -D ioredis-mock"
            }),
        );
        let state = build_permission_prompt(&approval);
        assert_eq!(state.title, PERMISSION_PROMPT_TITLE);
        assert!(state.prompt_owns_focus);
        assert_eq!(state.items.len(), 4);
        assert_eq!(state.items[0].label, PERMISSION_ONCE_LABEL);
        assert_eq!(
            state.items[1].label,
            "2 Yes, always allow npm install in this project"
        );
        assert_eq!(state.items[2].label, PERMISSION_EDIT_LABEL);
        assert_eq!(state.items[3].label, PERMISSION_NO_LABEL);
        assert_eq!(state.items[0].shortcut, Some('1'));
        assert_eq!(state.items[3].shortcut, Some('4'));
        assert_eq!(
            permission_command_line(&approval),
            "npm install ioredis && npm install -D ioredis-mock"
        );
    }

    #[test]
    fn permissions_picker_marks_smart() {
        let state = build_permissions_picker(Some("smart"));
        assert!(state.items[1].is_current);
        assert!(!state.prompt_owns_focus);
    }

    #[test]
    fn related_prompts_own_composer() {
        assert!(build_sandbox_deny_prompt().prompt_owns_focus);
        assert!(build_plan_confirm().prompt_owns_focus);
        assert!(build_clear_confirm().prompt_owns_focus);
        let q = build_question_prompt(
            "Question",
            &[
                ("wide", "1 120×40 first", "wide boards"),
                ("narrow", "2 40×12 first", "narrow boards"),
                ("both", "3 Both together", "full SPEC §7 set"),
            ],
            2,
        );
        assert_eq!(q.selected, 2);
        assert!(q.prompt_owns_focus);
    }
}
