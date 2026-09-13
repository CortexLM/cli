//! Keyboard handlers for interactive mode.

use super::state::{InteractiveAction, InteractiveResult, InteractiveState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

/// Handle a key event in interactive mode.
///
/// Returns an `InteractiveResult` indicating what action to take.
pub fn handle_interactive_key(state: &mut InteractiveState, key: KeyEvent) -> InteractiveResult {
    if state.is_form_active() {
        return handle_form_key(state, key);
    }
    if let Some(result) = handle_interactive_nav(state, key) {
        return result;
    }
    match key.code {
        KeyCode::Enter => handle_interactive_enter(state),
        KeyCode::Char(' ') if state.multi_select => {
            state.toggle_check();
            state.select_next();
            InteractiveResult::Continue
        }
        KeyCode::Esc => InteractiveResult::Cancelled,
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
            InteractiveResult::Cancelled
        }
        KeyCode::Char('q') if key.modifiers == KeyModifiers::CONTROL => {
            InteractiveResult::Cancelled
        }
        KeyCode::Char(c) if state.searchable && key.modifiers.is_empty() => {
            if state.search_query.is_empty() {
                if let Some(result) = try_resume_picker_key(state, c) {
                    return result;
                }
            }
            if !state.search_query.is_empty() || !is_shortcut(state, c) {
                state.push_search_char(c);
            } else if let Some(result) = try_shortcut(state, c) {
                return result;
            }
            InteractiveResult::Continue
        }
        KeyCode::Char(c) if !state.searchable && key.modifiers.is_empty() => {
            handle_non_search_char(state, c)
        }
        KeyCode::Backspace if state.searchable && !state.search_query.is_empty() => {
            state.pop_search_char();
            InteractiveResult::Continue
        }
        KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL && state.searchable => {
            state.update_search("");
            InteractiveResult::Continue
        }
        _ => InteractiveResult::Continue,
    }
}

fn handle_interactive_nav(
    state: &mut InteractiveState,
    key: KeyEvent,
) -> Option<InteractiveResult> {
    match key.code {
        KeyCode::Up | KeyCode::Char('k')
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::NONE =>
        {
            if state.effort_focused {
                state.effort_up();
            } else {
                state.select_prev();
            }
            Some(InteractiveResult::Continue)
        }
        KeyCode::Down | KeyCode::Char('j')
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::NONE =>
        {
            if state.effort_focused {
                state.effort_down();
            } else {
                state.select_next();
            }
            Some(InteractiveResult::Continue)
        }
        KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
            state.select_prev();
            Some(InteractiveResult::Continue)
        }
        KeyCode::Char('n') if key.modifiers == KeyModifiers::CONTROL => {
            state.select_next();
            Some(InteractiveResult::Continue)
        }
        KeyCode::PageUp => {
            for _ in 0..state.max_visible {
                state.select_prev();
            }
            Some(InteractiveResult::Continue)
        }
        KeyCode::PageDown => {
            for _ in 0..state.max_visible {
                state.select_next();
            }
            Some(InteractiveResult::Continue)
        }
        KeyCode::Home => {
            state.selected = 0;
            state.scroll_offset = 0;
            Some(InteractiveResult::Continue)
        }
        KeyCode::End => {
            if !state.filtered_indices.is_empty() {
                state.selected = state.filtered_indices.len() - 1;
                if state.selected >= state.max_visible {
                    state.scroll_offset = state.selected - state.max_visible + 1;
                }
            }
            Some(InteractiveResult::Continue)
        }
        KeyCode::Left if !state.tabs.is_empty() => {
            Some(InteractiveResult::SwitchTab { direction: -1 })
        }
        KeyCode::Right if !state.tabs.is_empty() => {
            Some(InteractiveResult::SwitchTab { direction: 1 })
        }
        KeyCode::Tab if state.effort.is_some() => {
            state.toggle_effort_focus();
            Some(InteractiveResult::Continue)
        }
        _ => None,
    }
}

fn handle_interactive_enter(state: &mut InteractiveState) -> InteractiveResult {
    if state.effort.is_some() && !state.effort_focused {
        state.effort_focused = true;
        return InteractiveResult::Continue;
    }
    let Some(item) = state.selected_item() else {
        return InteractiveResult::Continue;
    };
    if item.disabled {
        return InteractiveResult::Continue;
    }
    if state.multi_select && !state.checked.is_empty() {
        let item_ids: Vec<String> = state.checked_items().iter().map(|i| i.id.clone()).collect();
        return InteractiveResult::Selected {
            action: state.action.clone(),
            item_id: item_ids.first().cloned().unwrap_or_default(),
            item_ids,
        };
    }
    InteractiveResult::Selected {
        action: state.action.clone(),
        item_id: item.id.clone(),
        item_ids: vec![item.id.clone()],
    }
}

/// Handle key events when inline form is active.
fn handle_form_key(state: &mut InteractiveState, key: KeyEvent) -> InteractiveResult {
    match key.code {
        // Submit form with Enter
        KeyCode::Enter => {
            if let Some(ref form) = state.inline_form
                && form.is_valid()
            {
                let action_id = form.action_id.clone();
                let values: HashMap<String, String> = form
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), f.value.clone()))
                    .collect();
                state.close_form();
                return InteractiveResult::FormSubmitted { action_id, values };
            }
            InteractiveResult::Continue
        }

        // Cancel form with Esc
        KeyCode::Esc => {
            state.close_form();
            InteractiveResult::Continue
        }

        // Navigate fields with Tab
        KeyCode::Tab => {
            if let Some(ref mut form) = state.inline_form {
                form.focus_next();
            }
            InteractiveResult::Continue
        }

        // Navigate fields with Shift+Tab
        KeyCode::BackTab => {
            if let Some(ref mut form) = state.inline_form {
                form.focus_prev();
            }
            InteractiveResult::Continue
        }

        // Type characters into focused field
        KeyCode::Char(c) => {
            if let Some(ref mut form) = state.inline_form
                && let Some(ref mut field) = form.focused_mut()
            {
                field.value.push(c);
            }
            InteractiveResult::Continue
        }

        // Backspace
        KeyCode::Backspace => {
            if let Some(ref mut form) = state.inline_form
                && let Some(ref mut field) = form.focused_mut()
            {
                field.value.pop();
            }
            InteractiveResult::Continue
        }

        _ => InteractiveResult::Continue,
    }
}

/// Check if a character is a shortcut for any item.
fn is_shortcut(state: &InteractiveState, c: char) -> bool {
    state.items.iter().any(|item| item.shortcut == Some(c))
}

/// Shortcuts when the picker is not in search-input mode.
fn handle_non_search_char(state: &mut InteractiveState, c: char) -> InteractiveResult {
    if let Some(result) = try_permission_prompt_edit(state, c) {
        return result;
    }
    if let Some(result) = try_jobs_picker_key(state, c) {
        return result;
    }
    if let Some(result) = try_mcp_picker_key(state, c) {
        return result;
    }
    if let Some(result) = try_shortcut(state, c) {
        return result;
    }
    InteractiveResult::Continue
}

fn try_mcp_picker_key(state: &InteractiveState, c: char) -> Option<InteractiveResult> {
    if !matches!(state.action, InteractiveAction::McpServerAction) {
        return None;
    }
    let item_id = match c {
        'a' => "__add__".to_string(),
        'r' => {
            let item = state.selected_item()?;
            if item.disabled || item.id.starts_with("__") {
                return None;
            }
            format!("__reconnect__:{}", item.id)
        }
        _ => return None,
    };
    Some(InteractiveResult::Selected {
        action: state.action.clone(),
        item_id: item_id.clone(),
        item_ids: vec![item_id],
    })
}

fn try_resume_picker_key(state: &InteractiveState, c: char) -> Option<InteractiveResult> {
    if !matches!(state.action, InteractiveAction::ResumeSession) {
        return None;
    }
    let item = state.selected_item()?;
    if item.disabled || item.id.starts_with("__") {
        return None;
    }
    let action_id = match c {
        'f' => "resume-favorite",
        'd' => "resume-delete",
        _ => return None,
    };
    Some(InteractiveResult::Selected {
        action: InteractiveAction::Custom(action_id.into()),
        item_id: item.id.clone(),
        item_ids: vec![item.id.clone()],
    })
}

fn try_jobs_picker_key(state: &mut InteractiveState, c: char) -> Option<InteractiveResult> {
    if !matches!(
        &state.action,
        InteractiveAction::Custom(id) if id == "jobs-picker"
    ) {
        return None;
    }
    let prefix = match c {
        'a' => super::builders::jobs::JOBS_ATTACH_PREFIX,
        'x' => super::builders::jobs::JOBS_STOP_PREFIX,
        _ => return None,
    };
    let item = state.selected_item()?;
    if item.disabled {
        return None;
    }
    let item_id = format!("{prefix}{}", item.id);
    Some(InteractiveResult::Selected {
        action: state.action.clone(),
        item_id: item_id.clone(),
        item_ids: vec![item_id],
    })
}

/// `e` selects Edit on the §3.10 permission prompt (in addition to digit `3`).
fn try_permission_prompt_edit(state: &mut InteractiveState, c: char) -> Option<InteractiveResult> {
    if c != 'e'
        || !matches!(
            &state.action,
            InteractiveAction::Custom(id) if id == super::builders::PERMISSION_PROMPT_ACTION
        )
    {
        return None;
    }
    let idx = state.items.iter().position(|item| item.id == "edit")?;
    let filtered_idx = state.filtered_indices.iter().position(|&i| i == idx)?;
    state.selected = filtered_idx;
    Some(InteractiveResult::Selected {
        action: state.action.clone(),
        item_id: "edit".into(),
        item_ids: vec!["edit".into()],
    })
}

/// Try to select an item by its shortcut.
fn try_shortcut(state: &mut InteractiveState, c: char) -> Option<InteractiveResult> {
    for (idx, item) in state.items.iter().enumerate() {
        if item.shortcut == Some(c) && !item.disabled {
            // Find the filtered index
            if let Some(filtered_idx) = state.filtered_indices.iter().position(|&i| i == idx) {
                state.selected = filtered_idx;
                return Some(InteractiveResult::Selected {
                    action: state.action.clone(),
                    item_id: item.id.clone(),
                    item_ids: vec![item.id.clone()],
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interactive::state::{InteractiveAction, InteractiveItem};

    fn create_test_state() -> InteractiveState {
        let items = vec![
            InteractiveItem::new("1", "Apple").with_shortcut('a'),
            InteractiveItem::new("2", "Banana").with_shortcut('b'),
            InteractiveItem::new("3", "Cherry").with_shortcut('c'),
        ];
        InteractiveState::new("Test", items, InteractiveAction::Custom("test".into()))
    }

    #[test]
    fn test_navigation() {
        let mut state = create_test_state();

        assert_eq!(state.selected, 0);

        let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        handle_interactive_key(&mut state, key);
        assert_eq!(state.selected, 1);

        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        handle_interactive_key(&mut state, key);
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_selection() {
        let mut state = create_test_state();
        state.selected = 1;

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = handle_interactive_key(&mut state, key);

        match result {
            InteractiveResult::Selected { item_id, .. } => {
                assert_eq!(item_id, "2");
            }
            _ => panic!("Expected Selected result"),
        }
    }

    #[test]
    fn test_shortcut() {
        let mut state = create_test_state();

        let key = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE);
        let result = handle_interactive_key(&mut state, key);

        match result {
            InteractiveResult::Selected { item_id, .. } => {
                assert_eq!(item_id, "2");
            }
            _ => panic!("Expected Selected result"),
        }
    }

    #[test]
    fn test_cancel() {
        let mut state = create_test_state();

        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = handle_interactive_key(&mut state, key);

        assert!(matches!(result, InteractiveResult::Cancelled));
    }

    #[test]
    fn permission_prompt_digits_and_e() {
        let approval = crate::app::ApprovalState::new(
            "shell".into(),
            serde_json::json!({"command": "npm install ioredis"}),
        );
        let mut state = crate::interactive::builders::build_permission_prompt(&approval);
        let two = handle_interactive_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE),
        );
        assert!(matches!(
            two,
            InteractiveResult::Selected { ref item_id, .. } if item_id == "always"
        ));
        let four = handle_interactive_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE),
        );
        assert!(matches!(
            four,
            InteractiveResult::Selected { ref item_id, .. } if item_id == "no"
        ));
        let edit = handle_interactive_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
        );
        assert!(matches!(
            edit,
            InteractiveResult::Selected { ref item_id, .. } if item_id == "edit"
        ));
    }

    #[test]
    fn e_is_not_edit_outside_permission_prompt() {
        let items = vec![InteractiveItem::new("edit", "Edit")];
        let mut state =
            InteractiveState::new("Test", items, InteractiveAction::Custom("other".into()));
        let result = handle_interactive_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
        );
        assert!(matches!(result, InteractiveResult::Continue));
    }

    #[test]
    fn tab_jumps_to_effort_pane() {
        let items = vec![InteractiveItem::new("mini", "Cortex Mini 1")];
        let mut state = InteractiveState::new("Model", items, InteractiveAction::SetModel)
            .with_effort(crate::interactive::EffortLevel::Medium);
        assert!(!state.effort_focused);
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        handle_interactive_key(&mut state, tab);
        assert!(state.effort_focused);
        handle_interactive_key(&mut state, tab);
        assert!(!state.effort_focused);
        assert_eq!(state.effort, Some(crate::interactive::EffortLevel::Medium));
    }

    #[test]
    fn enter_on_model_list_opens_effort_then_applies() {
        let items = vec![InteractiveItem::new("mini", "Cortex Mini 1")];
        let mut state = InteractiveState::new("Model", items, InteractiveAction::SetModel)
            .with_effort(crate::interactive::EffortLevel::Medium);
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let first = handle_interactive_key(&mut state, enter);
        assert!(matches!(first, InteractiveResult::Continue));
        assert!(state.effort_focused);
        state.effort_down();
        let apply = handle_interactive_key(&mut state, enter);
        assert!(matches!(
            apply,
            InteractiveResult::Selected { ref item_id, .. } if item_id == "mini"
        ));
        assert_eq!(state.effort, Some(crate::interactive::EffortLevel::Low));
    }

    #[test]
    fn tab_is_a_no_op_without_effort_radios() {
        let mut state = create_test_state();
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        let result = handle_interactive_key(&mut state, tab);
        assert!(matches!(result, InteractiveResult::Continue));
        assert!(state.effort.is_none());
    }

    #[test]
    fn jobs_picker_a_and_x_are_attach_and_stop() {
        let rows = vec![crate::interactive::builders::JobRow {
            id: "sess-1".into(),
            kind: "cloud".into(),
            title: "agent".into(),
            status: "running".into(),
        }];
        let mut state = crate::interactive::builders::build_jobs_picker(&rows);
        let attach = handle_interactive_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
        );
        assert!(matches!(
            attach,
            InteractiveResult::Selected { ref item_id, .. } if item_id == "attach:sess-1"
        ));
        let stop = handle_interactive_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );
        assert!(matches!(
            stop,
            InteractiveResult::Selected { ref item_id, .. } if item_id == "stop:sess-1"
        ));
    }

    #[test]
    fn mcp_a_adds_and_r_reconnects_selected() {
        let servers = vec![crate::modal::mcp_manager::McpServerInfo {
            name: "sentry".into(),
            status: crate::modal::mcp_manager::McpStatus::Error,
            tool_count: 0,
            error: Some("token expired".into()),
            requires_auth: true,
        }];
        let mut state = crate::interactive::builders::build_mcp_selector(&servers);
        let add = handle_interactive_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
        );
        assert!(matches!(
            add,
            InteractiveResult::Selected { ref item_id, .. } if item_id == "__add__"
        ));
        let reconnect = handle_interactive_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        );
        assert!(matches!(
            reconnect,
            InteractiveResult::Selected { ref item_id, .. } if item_id == "__reconnect__:sentry"
        ));
    }

    #[test]
    fn resume_f_favorites_and_d_deletes() {
        let sessions = vec![crate::session::SessionSummary {
            id: "sess-1".into(),
            title: "fix login redirect".into(),
            model: "cortex/fix-login-redirect".into(),
            provider: "cortex".into(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            message_count: 14,
            archived: false,
        }];
        let mut state = crate::interactive::builders::build_resume_picker(&sessions, false);
        let fav = handle_interactive_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
        );
        assert!(matches!(
            fav,
            InteractiveResult::Selected { ref item_id, ref action, .. }
                if item_id == "sess-1" && matches!(action, InteractiveAction::Custom(id) if id == "resume-favorite")
        ));
        let del = handle_interactive_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
        );
        assert!(matches!(
            del,
            InteractiveResult::Selected { ref item_id, ref action, .. }
                if item_id == "sess-1" && matches!(action, InteractiveAction::Custom(id) if id == "resume-delete")
        ));
    }
}
