//! Runtime lock v2 captures — real `MinimalSessionView` at 40×12 and 120×40.
//!
//! Scene ids match SPEC §7 / `docs/media/tui-lock-v2/index.md`. Each filename
//! is one live MockTerminal state — never an alias of another frame.

use anyhow::{Context, Result};
use cortex_core::widgets::Message;
use cortex_tui_capture::{CaptureConfig, MockTerminal, StyleRendering};
use ratatui::widgets::Clear;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::app::{
    AppState, AutocompleteItem, AutocompleteTrigger, SubagentDisplayStatus, SubagentTaskDisplay,
    SubagentTodoItem, SubagentTodoStatus,
};
use crate::commands::{CommandRegistry, CompletionEngine, PALETTE_HOME_LIMIT};
use crate::interactive::builders::{SkillListItem, build_mcp_selector, build_model_selector};
use crate::interactive::state::{InteractiveAction, InteractiveItem, InteractiveState};
use crate::lock_proof::{LOCK_SPLASH_VERSION, LockFrame};
use crate::modal::mcp_manager::{McpServerInfo, McpStatus};
use crate::runner::login_screen::LoginScreen;
use crate::session::SessionSummary;
use crate::ui::consts::SERVICE_UNAVAILABLE_NEXT_STEP;
use crate::views::minimal_session::MinimalSessionView;
use crate::views::tool_call::{ToolCallDisplay, ToolResultDisplay, ToolStatus};
use crate::widgets::SettingsModalState;
use crate::widgets::settings_modal::SettingsRowKind;

const PRODUCT_ERROR: &str = "The coding service is temporarily unavailable";

/// Narrow (40×12) SPEC §7 set — 31 boards.
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
    "diff-hunk",
    "login",
    "shortcuts-overlay",
];

/// Wide (120×40) SPEC §7 set — 77 boards.
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
];

/// Boards captured at both sizes. Narrow (40×12) is a subset.
pub fn lock_v2_scene_ids(width: u16) -> &'static [&'static str] {
    if width <= 40 {
        LOCK_V2_NARROW_IDS
    } else {
        LOCK_V2_WIDE_IDS
    }
}

#[derive(Debug, Clone, Serialize)]
struct Manifest {
    width: u16,
    height: u16,
    fps: u32,
    frames: Vec<ManifestFrame>,
}

#[derive(Debug, Clone, Serialize)]
struct ManifestFrame {
    file: String,
    label: String,
    hold: u32,
}

pub fn write_lock_v2_frames(width: u16, height: u16, output_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("create {}", output_dir.display()))?;
    let mut manifest_frames = Vec::new();
    for id in lock_v2_scene_ids(width) {
        let frame = render_lock_v2_scene(id, width, height)?;
        let file = format!("{}.ans", id);
        std::fs::write(output_dir.join(&file), &frame.ansi)
            .with_context(|| format!("write {file}"))?;
        manifest_frames.push(ManifestFrame {
            file,
            label: id.to_string(),
            hold: 1,
        });
    }
    let manifest_path = output_dir.join("manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&Manifest {
            width,
            height,
            fps: 1,
            frames: manifest_frames,
        })?,
    )?;
    Ok(manifest_path)
}

fn capture_config(width: u16, height: u16) -> CaptureConfig {
    CaptureConfig::minimal(width, height)
        .with_style_rendering(StyleRendering::Ansi)
        .trim_whitespace(false)
        .with_cursor(false)
}

fn render_lock_v2_scene(id: &str, width: u16, height: u16) -> Result<LockFrame> {
    let config = capture_config(width, height);
    let mut terminal =
        MockTerminal::from_config(config.clone()).map_err(|err| anyhow::anyhow!("{err}"))?;
    terminal.draw(|frame| {
        let area = frame.area();
        frame.render_widget(Clear, area);
        match id {
            "login" => LoginScreen::lock_select(LOCK_SPLASH_VERSION, None).render(frame),
            "login-waiting" => LoginScreen::lock_waiting(
                LOCK_SPLASH_VERSION,
                "ABCD-1234",
                "https://api.cortex.foundation/device",
            )
            .render(frame),
            "login-success" => LoginScreen::lock_success(LOCK_SPLASH_VERSION).render(frame),
            "login-error" => {
                LoginScreen::lock_failed(LOCK_SPLASH_VERSION, PRODUCT_ERROR).render(frame)
            }
            _ => {
                let state = scene_state(id, width, height);
                let view = MinimalSessionView::new(&state);
                frame.render_widget(view, area);
            }
        }
    })?;
    let snapshot = terminal.snapshot();
    Ok(LockFrame {
        id: id.to_string(),
        ansi: snapshot.to_ansi(&config),
        plain: snapshot.to_ascii(&config),
        buffer: terminal.backend().buffer().clone(),
    })
}

fn lock_app() -> AppState {
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

fn resumed(state: &mut AppState) {
    state.show_launch_splash = false;
    state.tokens_used = 14_000;
}

fn conversation(state: &mut AppState) {
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

fn palette_state(query: &str) -> AppState {
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

fn dummy_model(id: &str, name: &str) -> crate::providers::models::ModelInfo {
    crate::providers::models::ModelInfo::new(id, name, "cortex")
}

fn model_picker(state: &mut AppState, hovered: Option<usize>) {
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

fn effort_picker(state: &mut AppState, effort: crate::interactive::EffortLevel, hover_low: bool) {
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
    state.thinking_budget = Some(effort.as_str().to_ascii_lowercase());
    state.enter_interactive_mode(interactive);
}

fn open_settings(state: &mut AppState, tune: impl FnOnce(&mut SettingsModalState)) {
    conversation(state);
    let mut modal = SettingsModalState::default();
    modal.values = state.settings_values();
    tune(&mut modal);
    state.settings_modal = Some(modal);
}

fn radios(
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

fn tool(
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

const DIFF_HUNK: &str = r#"@@ -20,6 +20,10 @@
 import Redis from "ioredis";
 import type { FastifyRequest } from "fastify";
-const limit = 30;
+const limit = 60;
+const windowSec = 60;

 export function rateLimit(opts: RateLimitOpts) {
-  const redis = new Redis();
+  const redis = new Redis(process.env.REDIS_URL);
"#;

const MD_TABLE: &str = r#"Here is how the three models compare:

| Model | Effort | Billing |
|---|---|---|
| Mini 1 | Medium | per request |
| Cortex 1 | High | per request |
| Max 1 | MAX | per token |

Mini 1 is the default; switch with /model when a change needs deeper reasoning."#;

const MD_FENCE: &str = r#"The limiter is a sliding window over a Redis sorted set:

```ts
export async function rateLimit(key: string, limit = 60) {
  const now = Date.now();
  await redis.zadd(key, now, String(now));
  return count <= limit;
}
```

It fails open when Redis is unreachable."#;

fn scene_state(id: &str, width: u16, height: u16) -> AppState {
    let mut state = lock_app();
    state.terminal_size = (width, height);
    match id {
        "welcome-cortex" => {}
        "welcome-agent" => {
            state.agent_entrypoint = true;
        }
        "first-run-tips" => {
            state.settings.insert("first_run_tips".into(), "1".into());
        }
        "session-empty" => {
            resumed(&mut state);
        }
        "session-user-bars" => {
            conversation(&mut state);
            state.opt_in_banner = true;
        }
        "session-thought" => {
            resumed(&mut state);
            state.add_message(
                Message::user("why does the composer lose focus after /model?")
                    .with_timestamp("10:02 AM"),
            );
            state.add_message(
                Message::assistant(
                    "The picker steals focus and never hands it back. `close_picker()` returns early when the effort radios are open.",
                )
                .with_timestamp("10:02 AM")
                .with_thought_secs(3.2),
            );
        }
        "session-thought-expanded" => {
            resumed(&mut state);
            state.show_thinking_blocks = true;
            state.add_message(
                Message::user("why does the composer lose focus after /model?")
                    .with_timestamp("10:02 AM"),
            );
            state.add_message(
                Message::assistant(
                    "**Thinking**\nThe picker steals focus and never hands it back because `close_picker()` returns early when the effort radios are open.\n\nThe picker steals focus and never hands it back. `close_picker()` returns early when the effort radios are open.",
                )
                .with_timestamp("10:02 AM")
                .with_thought_secs(3.2)
                .with_worked_secs(6.0),
            );
        }
        "session-thinking-live" => {
            resumed(&mut state);
            state.add_message(
                Message::user("why does the composer lose focus after /model?")
                    .with_timestamp("10:02 AM"),
            );
            state.start_streaming(None, true);
            state.streaming.thinking = true;
            state.streaming.prompt_started_at = Some(Instant::now() - Duration::from_secs(3));
        }
        "session-assistant" => {
            resumed(&mut state);
            state.add_message(Message::user("tell me about yourself").with_timestamp("12:49 AM"));
            state.add_message(
                Message::assistant(
                    "I'm **Cortex**, a coding agent that runs in your terminal.\n\n\
In practice:\n\
• I get straight to the point\n\
• I prefer concrete work over long explanations\n\
• I can also discuss, explain, or help you plan\n\n\
Tell me what you'd like to do.",
                )
                .with_timestamp("12:49 AM"),
            );
        }
        "session-worked" => {
            resumed(&mut state);
            state
                .add_message(Message::user("summarize the auth module").with_timestamp("12:51 AM"));
            state.add_message(
                Message::assistant("Auth looks consistent — tokens land in the keyring and `/login` is the only entry.")
                    .with_timestamp("12:51 AM")
                    .with_worked_secs(4.6),
            );
        }
        "session-optin" => {
            resumed(&mut state);
            state.opt_in_banner = true;
            state.add_message(
                Message::user("can Cortex retain traces to improve the product?")
                    .with_timestamp("08:15 AM"),
            );
            state.add_message(
                Message::assistant(
                    "Off by default. Opt in from the banner, or later in /settings → Privacy.",
                )
                .with_timestamp("08:15 AM"),
            );
        }
        "session-optin-hover" => {
            resumed(&mut state);
            state.opt_in_banner = true;
            state.opt_in_hover = Some(1);
            state.add_message(
                Message::user("can Cortex retain traces to improve the product?")
                    .with_timestamp("08:15 AM"),
            );
            state.add_message(
                Message::assistant(
                    "Off by default. Opt in from the banner, or later in /settings → Privacy.",
                )
                .with_timestamp("08:15 AM"),
            );
        }
        "composer-empty" => {
            state.show_launch_splash = false;
            state.tokens_used = 0;
        }
        "composer-typing" => {
            resumed(&mut state);
            state.input.set_text("hello");
        }
        "composer-typing-blink" => {
            resumed(&mut state);
            state.input.set_text("hello");
            state.caret_visible = false;
        }
        "composer-hover" => {
            resumed(&mut state);
            state.composer_hovered = true;
        }
        "composer-multiline" => {
            resumed(&mut state);
            state.input.set_text("first line\nsecond line\nthird line");
        }
        "footer-shortcuts" => {
            resumed(&mut state);
            state.input.set_text("list every shortcut");
        }
        "footer-hover" => {
            resumed(&mut state);
            state.footer_hover = Some(1);
        }
        "tokens-topright" => {
            resumed(&mut state);
            state.tokens_used = 142_000;
            state.add_message(
                Message::user("run the tui tests and fix whatever fails")
                    .with_timestamp("09:14 AM"),
            );
            state.add_message(
                Message::assistant("I'll run the suite and patch failures.")
                    .with_timestamp("09:14 AM")
                    .with_thought_secs(2.1),
            );
            state.tool_calls = vec![
                tool(
                    "sh",
                    "shell",
                    serde_json::json!({"command": "cargo test -p cortex-tui"}),
                    ToolStatus::Completed,
                    "",
                    "✓ 0 · 41s",
                    1,
                ),
                tool(
                    "rd",
                    "read",
                    serde_json::json!({"path": "src/cortex-tui/src/composer.rs"}),
                    ToolStatus::Completed,
                    "",
                    "212 lines",
                    2,
                ),
                tool(
                    "gr",
                    "grep",
                    serde_json::json!({"pattern": "alternate_screen", "path": "src/"}),
                    ToolStatus::Completed,
                    "",
                    "6 hits in 4 files",
                    3,
                ),
            ];
        }
        "tokens-topright-warn" => {
            resumed(&mut state);
            state.tokens_used = 460_000;
            state.add_message(
                Message::user("keep going on the rate limiter").with_timestamp("11:08 AM"),
            );
            state.add_message(
                Message::assistant("Context is nearly full — /compact will reclaim room.")
                    .with_timestamp("11:08 AM"),
            );
        }
        "compact-chat" => {
            resumed(&mut state);
            state.compact_mode = true;
            state.timestamps_enabled = false;
            state.add_message(Message::user("hey"));
            state.add_message(Message::assistant("Hey — what do you want to work on?"));
            state.add_message(Message::user("tell me about yourself"));
            state.add_message(Message::assistant(
                "I'm Cortex. Edge-to-edge bars, no timestamps.",
            ));
        }
        "slash-palette" => {
            let mut s = palette_state("/");
            s.autocomplete.hovered = Some(3);
            s.terminal_size = (width, height);
            return s;
        }
        "slash-model-typed" => {
            let mut s = palette_state("/mod");
            s.terminal_size = (width, height);
            return s;
        }
        "model-list" => model_picker(&mut state, None),
        "model-list-hover" => model_picker(&mut state, Some(2)),
        "model-effort-high" => {
            effort_picker(&mut state, crate::interactive::EffortLevel::High, false)
        }
        "model-effort-medium" => {
            effort_picker(&mut state, crate::interactive::EffortLevel::Medium, false)
        }
        "model-effort-low" => {
            effort_picker(&mut state, crate::interactive::EffortLevel::Low, false)
        }
        "model-effort-hover" => {
            effort_picker(&mut state, crate::interactive::EffortLevel::Medium, true)
        }
        "settings-appearance" => open_settings(&mut state, |_| {}),
        "settings-mouse" => open_settings(&mut state, |modal| {
            if let Some(i) = modal
                .visible_rows()
                .iter()
                .position(|r| r.id == "mouse_capture")
            {
                modal.selected = i;
                modal.scroll = modal
                    .visible_rows()
                    .iter()
                    .position(|r| r.id == "mouse" || r.label == "Mouse")
                    .unwrap_or(i.saturating_sub(1));
            }
        }),
        "settings-row-hover" => open_settings(&mut state, |modal| {
            modal.selected = 1; // Compact mode
            if let Some(i) = modal
                .visible_rows()
                .iter()
                .position(|r| r.id == "timestamps")
            {
                modal.hovered = Some(i);
            }
        }),
        "settings-search" => open_settings(&mut state, |modal| {
            modal.search = "scro".into();
            modal.search_focused = true;
            modal.selected = 0;
            if let Some(i) = modal
                .visible_rows()
                .iter()
                .position(|r| r.kind != SettingsRowKind::Category)
            {
                modal.selected = i;
            }
        }),
        "settings-theme-submenu" => open_settings(&mut state, |modal| {
            modal.theme_open = true;
            modal.theme_selected = 0;
        }),
        "mode-agent" => {
            resumed(&mut state);
            state.agent_mode_label = "Agent".into();
            state.add_message(Message::user("ship the lock v2 chrome").with_timestamp("09:00 AM"));
            state.add_message(
                Message::assistant("On it — Agent mode, edits allowed.").with_timestamp("09:00 AM"),
            );
        }
        "mode-plan" => {
            resumed(&mut state);
            state.agent_mode_label = "Plan".into();
            state.add_message(
                Message::user("how should we ship lock v2?").with_timestamp("09:04 AM"),
            );
            state.add_message(
                Message::assistant(
                    "**Plan**\n1. Recapture every SPEC §7 board from the live session.\n2. Keep violet on keyboard focus only.\n3. Do not merge until Designer signs off.",
                )
                .with_timestamp("09:04 AM"),
            );
        }
        "mode-ask" => {
            resumed(&mut state);
            state.agent_mode_label = "Ask".into();
            state.add_message(
                Message::user("where does the composer pin?").with_timestamp("09:06 AM"),
            );
            state.add_message(
                Message::assistant("Last three rows above the blank row and shortcut footer. Ask mode is read-only.")
                    .with_timestamp("09:06 AM"),
            );
        }
        "mode-bash" => {
            resumed(&mut state);
            state.agent_mode_label = "Bash".into();
            state.input.set_text("git status");
        }
        "permission-prompt" => {
            resumed(&mut state);
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
            state.enter_interactive_mode(radios(
                "Approve command",
                &[
                    ("once", "1 Yes, run once", "run this command once"),
                    (
                        "always",
                        "2 Yes, always allow npm install in this project",
                        "remember for this project",
                    ),
                    ("edit", "3 Edit command", "edit before running"),
                    ("no", "4 No — tell Cortex what to do instead", "reject"),
                ],
                0,
                None,
            ));
        }
        "permission-prompt-hover" => {
            resumed(&mut state);
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
            state.enter_interactive_mode(radios(
                "Approve command",
                &[
                    ("once", "1 Yes, run once", "run this command once"),
                    (
                        "always",
                        "2 Yes, always allow npm install in this project",
                        "remember for this project",
                    ),
                    ("edit", "3 Edit command", "edit before running"),
                    ("no", "4 No — tell Cortex what to do instead", "reject"),
                ],
                0,
                Some(1),
            ));
        }
        "permissions-picker" => {
            resumed(&mut state);
            state.input.set_text("/permissions");
            state.enter_interactive_mode(radios(
                "Permissions",
                &[
                    ("ro", "Read-only", "never edit files or run commands"),
                    ("smart", "Smart", "ask before leaving the sandbox"),
                    ("full", "Full access", "only ask when leaving the sandbox"),
                ],
                1,
                None,
            ));
        }
        "mcp-servers" => {
            resumed(&mut state);
            state.input.set_text("/mcp");
            let servers = vec![
                McpServerInfo {
                    name: "github".into(),
                    status: McpStatus::Running,
                    tool_count: 12,
                    error: None,
                    requires_auth: false,
                },
                McpServerInfo {
                    name: "linear".into(),
                    status: McpStatus::Starting,
                    tool_count: 0,
                    error: None,
                    requires_auth: true,
                },
                McpServerInfo {
                    name: "jira".into(),
                    status: McpStatus::Error,
                    tool_count: 0,
                    error: Some("auth failed".into()),
                    requires_auth: true,
                },
            ];
            state.enter_interactive_mode(build_mcp_selector(&servers));
        }
        "mcp-drop" => {
            resumed(&mut state);
            state.add_message(Message::user("list open PRs").with_timestamp("10:11 AM"));
            state.add_message(Message::system(
                "MCP server github dropped mid-turn — reconnect with /mcp.",
            ));
        }
        "plugins" => {
            resumed(&mut state);
            state.input.set_text("/plugins");
            state.enter_interactive_mode(radios(
                "Plugins",
                &[
                    ("review", "cortex-review", "enabled"),
                    ("mermaid", "mermaid-preview", "enabled"),
                    ("jira", "jira", "disabled"),
                ],
                0,
                None,
            ));
        }
        "usage" => {
            resumed(&mut state);
            state.input.set_text("/usage");
            state.enter_interactive_mode(radios(
                "Usage",
                &[
                    ("plan", "Cortex Pro", "current plan"),
                    ("agent", "Agent requests", "42 / 500 this period"),
                    ("tokens", "Tokens", "8.4M / 12M"),
                    ("billing", "Billing", "cortex.foundation/billing"),
                ],
                0,
                None,
            ));
        }
        "quota-exhausted" => {
            resumed(&mut state);
            state.quota_held = true;
            state.add_message(Message::user("keep going").with_timestamp("04:12 PM"));
            state.add_message(Message::system(
                "Agent quota exhausted — 500 / 500. Follow-ups stay in the composer until quota resets.",
            ));
        }
        "sandbox" => {
            resumed(&mut state);
            state.input.set_text("/sandbox");
            state.enter_interactive_mode(radios(
                "Sandbox",
                &[
                    ("fs", "Filesystem", "workspace only"),
                    ("net", "Network", "ask before leaving"),
                    ("esc", "Escalation", "Smart"),
                ],
                0,
                None,
            ));
        }
        "sandbox-deny" => {
            resumed(&mut state);
            state.add_message(
                Message::user("curl https://example.invalid").with_timestamp("10:22 AM"),
            );
            state.add_message(Message::system(
                "Sandbox denied: network egress is blocked for this command.",
            ));
            state.enter_interactive_mode(radios(
                "Sandbox blocked",
                &[
                    ("retry", "1 Retry inside the sandbox", "stay in workspace"),
                    ("allow", "2 Allow this domain", "ask next time"),
                    ("cancel", "3 Cancel", "do not run"),
                ],
                0,
                None,
            ));
        }
        "cloud-handoff" => {
            resumed(&mut state);
            state.add_message(
                Message::user("& ship this on a cloud agent").with_timestamp("02:18 PM"),
            );
            state.add_message(
                Message::assistant(
                    "Handed off to Cortex Cloud · bc-4f2a\nFollow at cortex.foundation/agents/bc-4f2a · or /jobs right here.",
                )
                .with_timestamp("02:18 PM"),
            );
        }
        "diagnostics" => {
            resumed(&mut state);
            state.add_message(Message::user("check the workspace").with_timestamp("08:03 AM"));
            state.tool_calls = vec![tool(
                "diag",
                "diagnostics",
                serde_json::json!({"path": "src/"}),
                ToolStatus::Failed,
                "error: type mismatch in lock_v2.rs\nwarning: unused import in chrome.rs",
                "1 error · 1 warning",
                1,
            )];
            state.add_message(Message::system(
                "Diagnostics · 1 error, 1 warning — Check your types before capturing.",
            ));
        }
        "interrupt-stopped" => {
            resumed(&mut state);
            state.add_message(Message::user("rewrite the whole crate").with_timestamp("09:33 AM"));
            state.add_message(Message::system("× Stopped"));
        }
        "error-unavailable" => {
            resumed(&mut state);
            state.add_message(Message::user("review the auth module").with_timestamp("01:02 PM"));
            state.add_message(Message::system(PRODUCT_ERROR));
            state.add_message(Message::system(SERVICE_UNAVAILABLE_NEXT_STEP));
        }
        "tool-tiles" => {
            resumed(&mut state);
            state.group_tool_calls = true;
            state.add_message(Message::user("inspect the composer").with_timestamp("09:14 AM"));
            state.tool_calls = vec![
                tool(
                    "rd",
                    "read",
                    serde_json::json!({"path": "src/cortex-tui/src/ui/chrome.rs"}),
                    ToolStatus::Completed,
                    "pub fn paint_composer_box(\n    area: Rect,\n    buf: &mut Buffer,\n) {",
                    "88 lines",
                    1,
                ),
                tool(
                    "gr",
                    "grep",
                    serde_json::json!({"pattern": "paint_composer", "path": "src/"}),
                    ToolStatus::Completed,
                    "src/cortex-tui/src/ui/chrome.rs\nsrc/cortex-tui/src/views/minimal_session/view.rs",
                    "6 hits in 2 files",
                    2,
                ),
                tool(
                    "sh",
                    "shell",
                    serde_json::json!({"command": "rg paint_composer_box"}),
                    ToolStatus::Completed,
                    "chrome.rs:127:pub fn paint_composer_box",
                    "1 match",
                    3,
                ),
            ];
        }
        "tool-tiles-collapsed" => {
            resumed(&mut state);
            state.group_tool_calls = true;
            state.add_message(Message::user("inspect the composer").with_timestamp("09:14 AM"));
            state.tool_calls = vec![
                tool(
                    "rd",
                    "read",
                    serde_json::json!({"path": "src/cortex-tui/src/ui/chrome.rs"}),
                    ToolStatus::Completed,
                    "",
                    "88 lines",
                    1,
                ),
                tool(
                    "gr",
                    "grep",
                    serde_json::json!({"pattern": "paint_composer"}),
                    ToolStatus::Completed,
                    "",
                    "6 hits",
                    2,
                ),
                tool(
                    "sh",
                    "shell",
                    serde_json::json!({"command": "rg paint_composer_box"}),
                    ToolStatus::Completed,
                    "",
                    "1 match",
                    3,
                ),
            ];
        }
        "shell-running" => {
            resumed(&mut state);
            state.add_message(
                Message::user("run cargo test -p cortex-tui").with_timestamp("09:20 AM"),
            );
            state.start_streaming(Some("shell".into()), true);
            let mut sh = tool(
                "sh",
                "shell",
                serde_json::json!({"command": "cargo test -p cortex-tui"}),
                ToolStatus::Running,
                "",
                "",
                1,
            );
            sh.live_output = vec![
                "running 1025 tests".into(),
                "test lock_v2::tests::lock_v2_wide_frames_are_unique ... ok".into(),
                "test lock_v2::tests::welcome_paints_inky_and_token_counter ... ok".into(),
            ];
            state.tool_calls = vec![sh];
        }
        "diff-hunk" => {
            resumed(&mut state);
            state.add_message(
                Message::user("raise the rate limit to 60").with_timestamp("10:40 AM"),
            );
            state.tool_calls = vec![tool(
                "ed",
                "edit",
                serde_json::json!({"path": "src/config/rateLimits.ts"}),
                ToolStatus::Completed,
                DIFF_HUNK,
                "Edit src/config/rateLimits.ts · +4 -2",
                1,
            )];
        }
        "edit-collapsed" => {
            resumed(&mut state);
            state.collapsed_edit_blocks = true;
            state.add_message(
                Message::user("raise the rate limit to 60").with_timestamp("10:40 AM"),
            );
            state.tool_calls = vec![tool(
                "ed",
                "edit",
                serde_json::json!({"path": "src/config/rateLimits.ts"}),
                ToolStatus::Completed,
                "",
                "Edit src/config/rateLimits.ts · +4 -2",
                1,
            )];
        }
        "md-table" => {
            resumed(&mut state);
            state.add_message(
                Message::user("Compare the three models for this project")
                    .with_timestamp("03:11 PM"),
            );
            state.add_message(Message::assistant(MD_TABLE).with_timestamp("03:11 PM"));
        }
        "code-fence" => {
            resumed(&mut state);
            state.add_message(
                Message::user("Show me the middleware you wrote").with_timestamp("03:12 PM"),
            );
            state.add_message(Message::assistant(MD_FENCE).with_timestamp("03:12 PM"));
        }
        "shortcuts-overlay" => {
            resumed(&mut state);
            state.shortcuts_open = true;
        }
        "resume-picker" => {
            resumed(&mut state);
            let now = chrono::Utc::now();
            let sessions = vec![
                SessionSummary {
                    id: "sess-lock-v2".into(),
                    title: "lock v2 runtime chrome".into(),
                    model: "cortex-1-mini".into(),
                    provider: "cortex".into(),
                    created_at: now - chrono::Duration::hours(2),
                    updated_at: now - chrono::Duration::minutes(12),
                    message_count: 18,
                    archived: false,
                },
                SessionSummary {
                    id: "sess-rate-limit".into(),
                    title: "rate limiter redis window".into(),
                    model: "cortex-1".into(),
                    provider: "cortex".into(),
                    created_at: now - chrono::Duration::days(1),
                    updated_at: now - chrono::Duration::hours(5),
                    message_count: 42,
                    archived: false,
                },
            ];
            state.enter_interactive_mode(crate::interactive::builders::build_resume_picker(
                &sessions, false,
            ));
        }
        "clear-confirm" => {
            resumed(&mut state);
            conversation(&mut state);
            state.enter_interactive_mode(radios(
                "Clear conversation?",
                &[
                    ("yes", "1 Clear", "wipe this thread, keep the workspace"),
                    ("no", "2 Keep", "leave messages in place"),
                ],
                0,
                None,
            ));
        }
        "plan-confirm" => {
            resumed(&mut state);
            state.agent_mode_label = "Plan".into();
            state.add_message(
                Message::assistant(
                    "**Plan**\nRecapture every SPEC §7 board, then wait for Designer.",
                )
                .with_timestamp("09:05 AM"),
            );
            state.enter_interactive_mode(radios(
                "Implement this plan?",
                &[
                    ("yes", "1 Yes, implement", "switch to Agent and execute"),
                    ("no", "2 Not yet", "stay in Plan"),
                ],
                0,
                None,
            ));
        }
        "queue" => {
            resumed(&mut state);
            state.add_message(Message::user("rewrite chrome.rs").with_timestamp("09:30 AM"));
            state.start_streaming(None, true);
            state.streaming.thinking = true;
            state.queue_message("also recapture the 40x12 set".into());
        }
        "files-picker" => {
            resumed(&mut state);
            let mut interactive = radios(
                "Files",
                &[
                    ("rl", "src/config/rateLimits.json", "2 days ago"),
                    ("ch", "src/cortex-tui/src/ui/chrome.rs", "today"),
                    ("lv", "src/cortex-tui/src/lock_v2.rs", "today"),
                ],
                0,
                Some(1),
            )
            .with_search();
            interactive.search_query = "rate".into();
            state.enter_interactive_mode(interactive);
        }
        "jobs" => {
            resumed(&mut state);
            state.input.set_text("/jobs");
            state.enter_interactive_mode(radios(
                "Jobs",
                &[
                    ("cloud", "cloud agent · bc-4f2a", "running"),
                    ("sub", "subagent · rate-limiter", "running"),
                    ("q", "queued · recapture PNGs", "waiting"),
                ],
                0,
                None,
            ));
        }
        "skills" => {
            resumed(&mut state);
            let skills = [
                SkillListItem {
                    name: "review".into(),
                    description: "Review a diff against the lock".into(),
                },
                SkillListItem {
                    name: "capture".into(),
                    description: "Recapture TUI lock boards".into(),
                },
            ];
            state.enter_interactive_mode(crate::interactive::builders::build_skills_selector(
                &skills,
            ));
        }
        "todos" => {
            resumed(&mut state);
            state.add_message(
                Message::user("work through the capture checklist").with_timestamp("09:41 AM"),
            );
            let mut task = SubagentTaskDisplay::new("sub-1", "tool-1", "lock v2 captures", "code");
            task.status = SubagentDisplayStatus::ExecutingTool("edit".into());
            task.todos = vec![
                SubagentTodoItem::new("Expand lock_v2 scene ids", SubagentTodoStatus::Completed),
                SubagentTodoItem::new(
                    "Make every frame a unique state",
                    SubagentTodoStatus::Completed,
                ),
                SubagentTodoItem::new("Recapture 120×40 and 40×12", SubagentTodoStatus::InProgress),
                SubagentTodoItem::new("Verify sha256 uniqueness", SubagentTodoStatus::Pending),
                SubagentTodoItem::new("Keep the PR drafted", SubagentTodoStatus::Pending),
            ];
            state.active_subagents = vec![task];
        }
        "question" => {
            resumed(&mut state);
            state.add_message(
                Message::assistant("Which capture size should we lock first?")
                    .with_timestamp("09:44 AM"),
            );
            state.enter_interactive_mode(radios(
                "Question",
                &[
                    ("wide", "1 120×40 first", "wide boards"),
                    ("narrow", "2 40×12 first", "narrow boards"),
                    ("both", "3 Both together", "full SPEC §7 set"),
                ],
                2,
                None,
            ));
        }
        "sudo" => {
            resumed(&mut state);
            state.add_message(
                Message::user("restart the sandbox daemon").with_timestamp("11:12 AM"),
            );
            state.enter_interactive_mode(radios(
                "Elevated Shell",
                &[
                    ("pw", "Password", "••••••••"),
                    ("once", "1 Run once", "elevated"),
                    ("cancel", "2 Cancel", "do not run"),
                ],
                0,
                None,
            ));
        }
        "config-tree" => {
            resumed(&mut state);
            state.input.set_text("/config");
            state.enter_interactive_mode(radios(
                "Config",
                &[
                    ("path", "~/.cortex/config.json", "read-only"),
                    ("model", "model", "Cortex Mini 1"),
                    ("effort", "effort", "medium"),
                    ("tui", "tui.theme", "dark → Cortex Night"),
                ],
                0,
                None,
            ));
        }
        "btw" => {
            resumed(&mut state);
            state.add_message(Message::user("keep rewriting chrome.rs").with_timestamp("09:50 AM"));
            state.start_streaming(None, true);
            state.streaming.thinking = false;
            state.streaming.is_actively_streaming = true;
            state.add_message(
                Message::user("/btw keep the composer dual-hairline").with_timestamp("09:51 AM"),
            );
        }
        other => panic!("unknown lock v2 scene {other}"),
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_core::style::{ACCENT, BAR_HOVER, SELECTION_BG, VOID};
    use ratatui::widgets::Widget;
    use std::collections::HashMap;

    fn cell_bg(buf: &ratatui::buffer::Buffer, x: u16, y: u16) -> ratatui::style::Color {
        buf[(x, y)].bg
    }

    fn assert_unique_frames(width: u16, height: u16, ids: &[&str]) {
        let mut by_ansi: HashMap<String, String> = HashMap::new();
        for id in ids {
            let frame =
                render_lock_v2_scene(id, width, height).unwrap_or_else(|e| panic!("{id}: {e}"));
            if let Some(prev) = by_ansi.insert(frame.ansi.clone(), (*id).to_string()) {
                panic!("{id} is identical to {prev} at {width}x{height}");
            }
        }
        assert_eq!(by_ansi.len(), ids.len());
    }

    #[test]
    fn lock_v2_wide_count_is_spec() {
        assert_eq!(LOCK_V2_WIDE_IDS.len(), 77);
        assert_eq!(LOCK_V2_NARROW_IDS.len(), 31);
    }

    #[test]
    fn lock_v2_wide_frames_are_unique() {
        assert_unique_frames(120, 40, LOCK_V2_WIDE_IDS);
    }

    #[test]
    fn lock_v2_narrow_frames_are_unique() {
        assert_unique_frames(40, 12, LOCK_V2_NARROW_IDS);
    }

    #[test]
    fn reported_collisions_are_distinct() {
        let pairs = [
            ("session-user-bars", "session-thought"),
            ("session-thought", "session-assistant"),
            ("session-user-bars", "session-assistant"),
            ("settings-appearance", "settings-row-hover"),
            ("welcome-cortex", "composer-empty"),
            ("composer-empty", "footer-shortcuts"),
            ("welcome-cortex", "footer-shortcuts"),
        ];
        for (a, b) in pairs {
            let fa = render_lock_v2_scene(a, 120, 40).expect(a);
            let fb = render_lock_v2_scene(b, 120, 40).expect(b);
            assert_ne!(fa.ansi, fb.ansi, "{a} must differ from {b}");
        }
        let hover = render_lock_v2_scene("settings-row-hover", 120, 40).expect("hover");
        let mut found_hover = false;
        for y in 0..40u16 {
            for x in 0..120u16 {
                if hover.buffer[(x, y)].bg == BAR_HOVER {
                    found_hover = true;
                }
            }
        }
        assert!(found_hover, "settings-row-hover must paint BAR_HOVER");
    }

    #[test]
    fn welcome_paints_inky_and_token_counter() {
        let frame = render_lock_v2_scene("welcome-cortex", 120, 40).expect("welcome");
        assert!(frame.plain.contains("Welcome to"), "{}", frame.plain);
        assert!(frame.plain.contains("Cortex"));
        assert!(frame.plain.contains("0 / 500K"), "{}", frame.plain);
        assert!(frame.plain.contains("Shift+Tab"), "{}", frame.plain);
        assert!(frame.plain.contains("Ctrl+x"), "{}", frame.plain);
        assert_eq!(cell_bg(&frame.buffer, 0, 0), VOID);
        let mut found_accent = false;
        for y in 0..40u16 {
            for x in 0..120u16 {
                if frame.buffer[(x, y)].fg == ACCENT {
                    found_accent = true;
                }
            }
        }
        assert!(found_accent, "expected violet caret on welcome");
    }

    #[test]
    fn slash_hover_is_not_violet_wash() {
        let mut state = palette_state("/");
        state.autocomplete.hovered = Some(3);
        let config = capture_config(120, 40);
        let mut terminal = MockTerminal::from_config(config).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(Clear, frame.area());
                MinimalSessionView::new(&state).render(frame.area(), frame.buffer_mut());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let mut found_hover = false;
        for y in 0..40u16 {
            for x in 0..120u16 {
                if buf[(x, y)].bg == BAR_HOVER {
                    found_hover = true;
                }
                if buf[(x, y)].bg == ratatui::style::Color::Rgb(0x22, 0x1A, 0x38) {
                    panic!("retired violet wash at {x},{y}");
                }
            }
        }
        assert!(found_hover || buf[(3, 26)].bg == SELECTION_BG);
    }

    #[test]
    fn effort_order_is_high_medium_low() {
        let frame = render_lock_v2_scene("model-effort-high", 120, 40).expect("effort");
        let high = frame.plain.find("High Effort").expect("high");
        let med = frame.plain.find("Medium Effort").expect("med");
        let low = frame.plain.find("Low Effort").expect("low");
        assert!(high < med && med < low, "{}", frame.plain);
        assert!(frame.plain.contains("Tab"), "{}", frame.plain);
    }

    #[test]
    fn settings_modal_has_appearance_and_search() {
        let frame = render_lock_v2_scene("settings-appearance", 120, 40).expect("settings");
        assert!(frame.plain.contains("Settings"), "{}", frame.plain);
        assert!(frame.plain.contains("Appearance"), "{}", frame.plain);
        assert!(frame.plain.contains("Compact mode"), "{}", frame.plain);
        assert!(
            frame.plain.contains("/ to search") || frame.plain.contains("search"),
            "{}",
            frame.plain
        );
    }

    #[test]
    fn agent_welcome_copy() {
        let frame = render_lock_v2_scene("welcome-agent", 120, 40).expect("agent");
        assert!(frame.plain.contains("Cortex Agent"), "{}", frame.plain);
        assert!(
            frame.plain.contains("Describe a task for the agent")
                || frame.plain.contains("Plan, search"),
            "{}",
            frame.plain
        );
    }

    #[test]
    fn user_bars_and_thought_metadata() {
        let frame = render_lock_v2_scene("session-thought", 120, 40).expect("thought");
        assert!(frame.plain.contains("Thought for"), "{}", frame.plain);
        assert!(
            !frame.plain.contains("Worked for"),
            "session-thought stays collapsed without Worked:\n{}",
            frame.plain
        );
        assert!(
            frame.plain.contains("AM") || frame.plain.contains("PM"),
            "{}",
            frame.plain
        );
        assert!(frame.plain.contains("Cortex Mini 1"), "{}", frame.plain);
    }

    #[test]
    fn first_run_tips_are_visible() {
        let frame = render_lock_v2_scene("first-run-tips", 120, 40).expect("tips");
        assert!(frame.plain.contains("A few tips"), "{}", frame.plain);
        assert!(frame.plain.contains("/model"), "{}", frame.plain);
    }
}
