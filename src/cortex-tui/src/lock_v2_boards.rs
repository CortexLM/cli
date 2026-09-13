//! Lock v2 session board states (`scene_state`).
//!
//! Permission-prompt lock chrome is unchanged; this module only relocates
//! existing scene construction so `lock_v2.rs` stays under 1000 lines.

use cortex_core::widgets::Message;
use std::time::{Duration, Instant};

use crate::app::{
    AppState, SubagentDisplayStatus, SubagentTaskDisplay, SubagentTodoItem, SubagentTodoStatus,
};
use crate::interactive::builders::{
    JobRow, SkillListItem, build_clear_confirm, build_jobs_picker, build_mcp_selector,
    build_permissions_picker, build_plan_confirm, build_question_prompt, build_sandbox_deny_prompt,
};
use crate::lock_v2::PRODUCT_ERROR;
use crate::lock_v2_goal::{
    apply_computer_scene, apply_goal_chip_scene, show_goal_in_narrow_palette,
};
use crate::lock_v2_network::apply_offline_rate_limit_scene;
use crate::lock_v2_parity::apply_parity_scene;
use crate::lock_v2_scenes::*;
use crate::modal::mcp_manager::{McpServerInfo, McpStatus};
use crate::session::SessionSummary;
use crate::ui::consts::SERVICE_UNAVAILABLE_NEXT_STEP;
use crate::views::tool_call::ToolStatus;
use crate::widgets::settings_modal::SettingsRowKind;

pub(crate) fn scene_state(id: &str, width: u16, height: u16) -> AppState {
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
            if width <= 40 {
                show_goal_in_narrow_palette(&mut s);
            } else {
                s.autocomplete.hovered = Some(3);
            }
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
                    "**Plan**\n1. Recapture every SPEC §7 board from the live session.\n2. Keep banner green on keyboard focus only.\n3. Do not merge until Designer signs off.",
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
            lock_permission_prompt(&mut state, None);
        }
        "permission-prompt-hover" => {
            lock_permission_prompt(&mut state, Some(1));
        }
        "permissions-picker" => {
            resumed(&mut state);
            state.input.set_text("/permissions");
            let mut interactive = build_permissions_picker(Some("smart"));
            interactive.selected = 1;
            state.enter_interactive_mode(interactive);
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
            state.enter_interactive_mode(build_sandbox_deny_prompt());
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
            state.enter_interactive_mode(build_clear_confirm());
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
            state.enter_interactive_mode(build_plan_confirm());
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
            state.enter_interactive_mode(build_jobs_picker(&[
                JobRow {
                    id: "cloud".into(),
                    kind: "cloud agent".into(),
                    title: "bc-4f2a".into(),
                    status: "running".into(),
                },
                JobRow {
                    id: "sub".into(),
                    kind: "subagent".into(),
                    title: "rate-limiter".into(),
                    status: "running".into(),
                },
                JobRow {
                    id: "q".into(),
                    kind: "queued".into(),
                    title: "recapture PNGs".into(),
                    status: "waiting".into(),
                },
            ]));
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
            state.enter_interactive_mode(build_question_prompt(
                "Question",
                &[
                    ("wide", "1 120×40 first", "wide boards"),
                    ("narrow", "2 40×12 first", "narrow boards"),
                    ("both", "3 Both together", "full SPEC §7 set"),
                ],
                2,
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
        id if apply_offline_rate_limit_scene(id, &mut state) => {}
        id if apply_parity_scene(id, &mut state, width) => {}
        id if apply_goal_chip_scene(id, &mut state) => {}
        "computer-disconnected" | "computer-cloud-default" => {
            assert!(
                apply_computer_scene(id, &mut state, width),
                "computer lock scene {id}"
            );
        }
        other => panic!("unknown lock v2 scene {other}"),
    }
    state
}
