//! Lock v2 session scene helpers (not login screens).
//!
//! Permission-prompt lock chrome is unchanged; this module only relocates
//! existing scene construction so `lock_v2.rs` stays under 1000 lines.

use cortex_core::widgets::Message;

use crate::app::{AppState, AutocompleteItem, AutocompleteTrigger};
use crate::commands::{CommandRegistry, CompletionEngine, PALETTE_HOME_LIMIT};
use crate::interactive::builders::build_model_selector;
use crate::interactive::state::{InteractiveAction, InteractiveItem, InteractiveState};
use crate::views::tool_call::{ToolCallDisplay, ToolResultDisplay, ToolStatus};
use crate::widgets::SettingsModalState;

pub(crate) fn lock_app() -> AppState {
    let mut state = AppState::default();
    state.cli_version = env!("CARGO_PKG_VERSION").to_string();
    state.model = "cortex-1-mini".into();
    state.agent_mode_label = "Agent".into();
    state.thinking_budget = Some("medium".into());
    state.tokens_used = 0;
    state.context_window = 500_000;
    state.opt_in_banner = false;
    state.timestamps_enabled = true;
    state.caret_visible = true;
    state
}

pub(crate) fn resumed(state: &mut AppState) {
    state.show_launch_splash = false;
    state.tokens_used = 14_000;
}

pub(crate) fn conversation(state: &mut AppState) {
    resumed(state);
    state.add_message(
        Message::user("hey")
            .with_timestamp("12:49 AM")
            .with_thought_secs(0.4)
            .with_worked_secs(1.8),
    );
    if let Some(last) = state.messages.last_mut() {
        last.thought_secs = None;
        last.worked_secs = None;
    }
    state.add_message(
        Message::assistant("Hey — what do you want to work on?")
            .with_timestamp("12:49 AM")
            .with_thought_secs(0.4)
            .with_worked_secs(1.8),
    );
    state.add_message(Message::user("tell me about yourself").with_timestamp("12:49 AM"));
    state.add_message(
        Message::assistant(
            "I'm **Cortex**, a coding agent that runs in your terminal.\n\n\
I mostly help you **build and debug software**: code, architecture, debugging, reviews, docs, and a bit of research.\n\
Here I run in an interactive terminal, so I can read your files, run commands, and change the project.\n\n\
In practice:\n\
• I get straight to the point\n\
• I prefer concrete work over long explanations\n\
• I can also discuss, explain, or help you plan\n\n\
Tell me what you'd like to do.",
        )
        .with_timestamp("12:49 AM")
        .with_thought_secs(0.4)
        .with_worked_secs(4.6),
    );
}

pub(crate) fn palette_state(query: &str) -> AppState {
    let mut state = lock_app();
    conversation(&mut state);
    state.input.set_text(query);
    state.autocomplete.show(AutocompleteTrigger::Command, 0);
    let registry = CommandRegistry::default();
    let engine = CompletionEngine::new(&registry);
    let completions = engine.complete(query);
    let items: Vec<AutocompleteItem> = completions
        .into_iter()
        .map(|c| AutocompleteItem::new(&c.command, &c.display, &c.description))
        .collect();
    state.autocomplete.set_items(items);
    state.autocomplete.max_visible = PALETTE_HOME_LIMIT;
    if query.len() > 1 {
        state.autocomplete.set_query(&query[1..]);
    }
    state
}

pub(crate) fn dummy_model(id: &str, name: &str) -> crate::providers::models::ModelInfo {
    crate::providers::models::ModelInfo::new(id, name, "cortex")
}

pub(crate) fn model_picker(state: &mut AppState, hovered: Option<usize>) {
    conversation(state);
    state.input.set_text("/model");
    let mut interactive = build_model_selector(
        vec![
            dummy_model("cortex-1-mini", "Cortex Mini 1"),
            dummy_model("cortex-1", "Cortex 1"),
            dummy_model("cortex-1-max", "Cortex Max 1"),
        ],
        Some("cortex-1-mini"),
        Some("medium"),
    );
    interactive.hovered = hovered;
    state.enter_interactive_mode(interactive);
}

pub(crate) fn effort_picker(
    state: &mut AppState,
    effort: crate::interactive::EffortLevel,
    hover_low: bool,
) {
    conversation(state);
    state.input.set_text("/model Cortex Mini 1");
    let mut interactive = build_model_selector(
        vec![dummy_model("cortex-1-mini", "Cortex Mini 1")],
        Some("cortex-1-mini"),
        Some(effort.as_str()),
    );
    interactive.effort = Some(effort);
    interactive.effort_focused = true;
    if hover_low {
        interactive.hovered = Some(1000 + 2);
    }
    state.enter_interactive_mode(interactive);
}

pub(crate) fn open_settings(state: &mut AppState, tune: impl FnOnce(&mut SettingsModalState)) {
    conversation(state);
    let mut modal = SettingsModalState::default();
    modal.values = state.settings_values();
    tune(&mut modal);
    state.settings_modal = Some(modal);
}

/// Production permission prompt for lock `permission-prompt*`.
pub(crate) fn lock_permission_prompt(state: &mut AppState, hovered: Option<usize>) {
    resumed(state);
    state.add_message(
        Message::user("add ioredis and a mock for the tests").with_timestamp("09:40 AM"),
    );
    state.add_message(
        Message::assistant(
            "Cortex wants to run\n`$ npm install ioredis && npm install -D ioredis-mock`",
        )
        .with_timestamp("09:40 AM")
        .with_thought_secs(1.4),
    );
    state.request_tool_approval(
        "lock-perm".into(),
        "shell".into(),
        serde_json::json!({
            "command": "npm install ioredis && npm install -D ioredis-mock"
        }),
        None,
    );
    if let Some(hover) = hovered
        && let Some(interactive) = state.get_interactive_state_mut()
    {
        interactive.hovered = Some(hover);
    }
}

pub(crate) fn radios(
    title: &str,
    rows: &[(&str, &str, &str)],
    selected: usize,
    hovered: Option<usize>,
) -> InteractiveState {
    let items = rows
        .iter()
        .map(|(id, label, desc)| InteractiveItem::new(*id, *label).with_description(*desc))
        .collect();
    let mut interactive =
        InteractiveState::new(title, items, InteractiveAction::Custom(title.into()));
    interactive.selected = selected.min(rows.len().saturating_sub(1));
    interactive.hovered = hovered;
    interactive
}

pub(crate) fn tool(
    id: &str,
    name: &str,
    args: serde_json::Value,
    status: ToolStatus,
    output: &str,
    summary: &str,
    sequence: u64,
) -> ToolCallDisplay {
    let mut call = ToolCallDisplay::new(id.into(), name.into(), args, sequence);
    call.status = status;
    call.collapsed = false;
    if !output.is_empty() || !summary.is_empty() {
        call.result = Some(ToolResultDisplay {
            output: output.into(),
            success: status != ToolStatus::Failed,
            summary: summary.into(),
        });
    }
    call
}

pub(crate) const DIFF_HUNK: &str = r#"@@ -41,7 +41,8 @@
     let chip = model_chip(state);
-    footer.push(chip);
+    border.push_right(chip);
+    footer.push(hints);
     Ok(())
"#;

pub(crate) const PLAN_BODY: &str = "Plan — no files change until you approve.\n\n\
1. Wrap `ApiClient::send` in a retry loop: 3 attempts, exponential backoff with jitter.\n\
2. Retry only on timeouts and 5xx; never on 4xx.\n\
3. Surface the final failure as product copy: 'The coding service is temporarily unavailable'.\n\
4. Unit tests for the backoff schedule and the give-up path.";

pub(crate) const MD_TABLE: &str = r#"Here is how the three models compare:

| Model | Default effort | Context | Billing |
|---|---|---|---|
| Cortex Mini 1 | Medium | 500K | per request |
| Cortex 1 | High | 500K | per request |
| Cortex Max 1 | High | 1M | per token |
"#;

pub(crate) fn apply_btw_scene(id: &str, state: &mut AppState) -> bool {
    if id != "btw" {
        return false;
    }
    resumed(state);
    state.add_message(Message::user("keep rewriting chrome.rs").with_timestamp("09:50 AM"));
    state.start_streaming(None, true);
    state.streaming.thinking = false;
    state.streaming.is_actively_streaming = true;
    state.add_message(
        Message::user("/btw keep the composer dual-hairline").with_timestamp("09:51 AM"),
    );
    true
}

pub(crate) const MD_FENCE: &str = r#"```rust
pub async fn with_retry<F, T>(mut op: F) -> Result<T>
where F: FnMut() -> Fut<Result<T>>,
{
    for attempt in 0..3 {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt == 2 => return Err(e),
            Err(_) => sleep(backoff(attempt)).await,
        }
    }
    unreachable!()
}
```"#;
