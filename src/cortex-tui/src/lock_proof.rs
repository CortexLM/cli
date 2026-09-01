//! Headless visual-lock captures for PR review.
//!
//! Renders the real session, login, palette, and settings widgets through
//! [`cortex_tui_capture::MockTerminal`] and writes ANSI frames a rasteriser
//! turns into PNGs.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cortex_core::style::VOID;
use cortex_core::widgets::Message;
use cortex_tui_capture::{CaptureConfig, MockTerminal, StyleRendering};
use ratatui::style::Style;
use ratatui::widgets::Clear;
use serde::Serialize;

use crate::app::{AppState, AutocompleteItem, AutocompleteTrigger};
use crate::commands::{CommandRegistry, CompletionEngine, PALETTE_HOME_LIMIT};
use crate::interactive::builders::build_settings_hub;
use crate::runner::login_screen::LoginScreen;
use crate::views::minimal_session::MinimalSessionView;

/// Splash line pinned by the visual lock (product version stays on the binary).
pub const LOCK_SPLASH_VERSION: &str = "1.0.0";

const PRODUCT_ERROR: &str = "The coding service is temporarily unavailable";

/// One named ANSI frame.
#[derive(Debug, Clone)]
pub struct LockFrame {
    pub id: String,
    pub ansi: String,
    pub plain: String,
}

/// Manifest consumed by `scripts/ansi-frames-to-pngs.py`.
#[derive(Debug, Clone, Serialize)]
pub struct LockManifest {
    pub width: u16,
    pub height: u16,
    pub fps: u32,
    pub frames: Vec<LockManifestFrame>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LockManifestFrame {
    pub file: String,
    pub label: String,
    pub hold: u32,
}

/// Scene ids captured at each terminal size.
pub fn lock_scene_ids() -> &'static [&'static str] {
    &[
        "splash",
        "login_select",
        "login_waiting",
        "login_success",
        "login_error",
        "palette",
        "palette_empty",
        "settings_hub",
        "settings_empty",
        "tool_tiles",
        "diagnostics",
        "multi_diff",
        "compact",
        "interrupt",
        "clear",
        "session_empty",
        "session_loading",
        "session_error",
        "session_success",
        "shell",
        "permission",
        "plan",
        "streaming",
        "resume",
        "mcp",
        "usage",
        "quota",
        "sandbox",
        "cloud",
        "sudo",
        "ask",
        "files",
        "queue",
        "jobs",
        "help",
        "first_run",
        "bash",
        "config",
        "footer_max",
        "login",
        "thinking",
        "todos",
        "question",
        "skills",
        "btw",
        "stopped",
        "compacted",
        "write",
        "clear_confirm",
        "grep",
        "glob",
        "delete",
        "list",
        "fetch",
        "mcp_call",
        "task",
        "edit",
    ]
}

/// Render every lock scene at `width`×`height`.
pub fn render_lock_frames(width: u16, height: u16) -> Result<Vec<LockFrame>> {
    let mut frames = Vec::new();
    for id in lock_scene_ids() {
        frames.push(render_lock_scene(id, width, height)?);
    }
    Ok(frames)
}

/// Write ANSI frames and `manifest.json` under `output_dir`.
pub fn write_lock_frames(width: u16, height: u16, output_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("create {}", output_dir.display()))?;
    let frames = render_lock_frames(width, height)?;
    let mut manifest_frames = Vec::new();
    for frame in &frames {
        let file = format!("{}.ans", frame.id);
        std::fs::write(output_dir.join(&file), &frame.ansi)
            .with_context(|| format!("write {file}"))?;
        manifest_frames.push(LockManifestFrame {
            file,
            label: frame.id.clone(),
            hold: 1,
        });
    }
    let manifest = LockManifest {
        width,
        height,
        fps: 1,
        frames: manifest_frames,
    };
    let manifest_path = output_dir.join("manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).context("serialize lock manifest")?,
    )?;
    Ok(manifest_path)
}

fn capture_config(width: u16, height: u16) -> CaptureConfig {
    CaptureConfig::minimal(width, height)
        .with_style_rendering(StyleRendering::Ansi)
        .trim_whitespace(false)
        .with_cursor(false)
}

fn render_lock_scene(id: &str, width: u16, height: u16) -> Result<LockFrame> {
    let config = capture_config(width, height);
    let mut terminal =
        MockTerminal::from_config(config.clone()).map_err(|err| anyhow::anyhow!("{err}"))?;
    terminal
        .draw(|frame| {
            let area = frame.area();
            frame.render_widget(Clear, area);
            frame.render_widget(
                ratatui::widgets::Block::default().style(Style::default().bg(VOID)),
                area,
            );
            match id {
                "splash" => {
                    crate::lock_boards::render_lock_board("splash", area, frame.buffer_mut());
                }
                "login_select" => {
                    crate::lock_boards::render_lock_board("login", area, frame.buffer_mut());
                }
                "login_waiting" => LoginScreen::lock_waiting(
                    LOCK_SPLASH_VERSION,
                    "ABCD-1234",
                    "https://api.cortex.foundation/device",
                )
                .render(frame),
                "login_success" => LoginScreen::lock_success(LOCK_SPLASH_VERSION).render(frame),
                "login_error" => {
                    LoginScreen::lock_failed(LOCK_SPLASH_VERSION, PRODUCT_ERROR).render(frame)
                }
                "palette" => {
                    crate::lock_boards::render_lock_board("palette", area, frame.buffer_mut());
                }
                "palette_empty" => draw_session(frame, palette_state(true)),
                "settings_empty" => draw_session(frame, settings_state(true, height)),
                "settings_hub" => {
                    crate::lock_boards::render_lock_board("settings_hub", area, frame.buffer_mut());
                }
                "tool_tiles" => {
                    crate::lock_boards::render_lock_board("grep", area, frame.buffer_mut());
                }
                "diagnostics" => {
                    crate::lock_boards::render_lock_board("diagnostics", area, frame.buffer_mut());
                }
                "multi_diff" => {
                    crate::lock_boards::render_lock_board("multi_diff", area, frame.buffer_mut());
                }
                "compact" => draw_session(frame, compact_state()),
                "interrupt" => draw_session(frame, interrupt_state()),
                "clear" => draw_session(frame, clear_state()),
                "session_empty" => draw_session(frame, empty_session_state()),
                "session_loading" => draw_session(frame, loading_session_state()),
                "session_error" => draw_session(frame, error_session_state()),
                "session_success" => draw_session(frame, success_session_state()),
                board if crate::lock_boards::is_lock_board(board) => {
                    crate::lock_boards::render_lock_board(board, area, frame.buffer_mut());
                }
                other => {
                    let msg = format!("unknown lock scene {other}");
                    frame.render_widget(ratatui::widgets::Paragraph::new(msg), area);
                }
            }
        })
        .map_err(|err| anyhow::anyhow!("{err}"))?;

    let snapshot = terminal.snapshot();
    Ok(LockFrame {
        id: id.to_string(),
        ansi: snapshot.to_ansi(&config),
        plain: snapshot.to_ascii(&config),
    })
}

fn draw_session(frame: &mut ratatui::Frame, state: AppState) {
    let view = MinimalSessionView::new(&state);
    frame.render_widget(view, frame.area());
}

fn lock_app() -> AppState {
    let mut state = AppState::default();
    state.cli_version = LOCK_SPLASH_VERSION.to_string();
    state.footer_cwd = "~/cli".into();
    state.git_branch = "main".into();
    state.git_dirty = true;
    state.model = "cortex-1-mini".into();
    state.agent_mode_label = "Agent".into();
    state.context_percent = 100;
    state
}

fn empty_session_state() -> AppState {
    lock_app()
}

fn loading_session_state() -> AppState {
    let mut state = lock_app();
    state.add_message(Message::user("review the auth module"));
    state.start_streaming(None, true);
    state
}

fn error_session_state() -> AppState {
    let mut state = lock_app();
    state.add_message(Message::user("review the auth module"));
    state.add_message(Message::assistant(PRODUCT_ERROR));
    state
}

fn success_session_state() -> AppState {
    let mut state = lock_app();
    state.add_message(Message::user("review the auth module"));
    state.add_message(Message::assistant(
        "Signed in. Auth module looks consistent.",
    ));
    state
}

fn palette_state(empty: bool) -> AppState {
    let mut state = lock_app();
    state.input.set_text("/");
    state.autocomplete.show(AutocompleteTrigger::Command, 0);
    let registry = CommandRegistry::default();
    let engine = CompletionEngine::new(&registry);
    let query = if empty { "/zzzz-no-such-command" } else { "/" };
    let completions = engine.complete(query);
    let items: Vec<AutocompleteItem> = completions
        .into_iter()
        .map(|c| AutocompleteItem::new(&c.command, &c.display, &c.description))
        .collect();
    if empty {
        state.autocomplete.set_query("zzzz-no-such-command");
        state.input.set_text("/zzzz-no-such-command");
        state.autocomplete.set_items(Vec::new());
        state.autocomplete.visible = true;
    } else {
        state.autocomplete.set_items(items);
    }
    state.autocomplete.max_visible = PALETTE_HOME_LIMIT;
    state
}

fn settings_state(empty: bool, height: u16) -> AppState {
    let mut state = lock_app();
    let mut interactive = build_settings_hub(Some(height));
    if empty {
        interactive.searchable = true;
        interactive.search_query = "zzzz-no-such-setting".into();
        interactive.filtered_indices.clear();
    }
    state.enter_interactive_mode(interactive);
    state
}

fn compact_state() -> AppState {
    let mut state = success_session_state();
    state.compact_mode = true;
    state
}

fn interrupt_state() -> AppState {
    let mut state = loading_session_state();
    state.input.set_text("/interrupt");
    state
}

fn clear_state() -> AppState {
    let mut state = success_session_state();
    state.clear_messages();
    state.cli_version = LOCK_SPLASH_VERSION.to_string();
    state
}

/// Render the Ctrl+K palette over a session (wide lock surfaces).
#[allow(dead_code)]
pub fn command_palette_home() -> crate::widgets::CommandPaletteState {
    let mut palette = crate::widgets::CommandPaletteState::new();
    let registry = CommandRegistry::default();
    palette.load_commands(&registry);
    palette.filter();
    palette
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{PALETTE_HOME_COMMANDS, SLASH_VISIBLE};

    #[test]
    fn no_inverted_mint_selection_anywhere() {
        // Mint is highlight-only: `>`, small accents, success. A selected row
        // is a dark #1A3330 bar, never black-on-mint.
        const MINT_BG: &str = "48;2;0;245;212";
        const DARK_SELECTION_BG: &str = "48;2;26;51;48";
        for id in lock_scene_ids() {
            for size in [(40u16, 12u16), (120u16, 40u16)] {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                assert!(
                    !frame.ansi.contains(MINT_BG),
                    "{id} paints an inverted mint bar at {size:?}"
                );
            }
        }
        for id in ["palette", "settings_hub", "login_select", "multi_diff"] {
            let frame = render_lock_scene(id, 120, 40).expect(id);
            assert!(
                frame.ansi.contains(DARK_SELECTION_BG),
                "{id} must use the dark selection bar"
            );
        }
    }

    #[test]
    fn splash_has_session_chrome() {
        for size in [(40u16, 12u16), (120u16, 40u16)] {
            let frame = render_lock_scene("splash", size.0, size.1).expect("splash");
            assert!(
                frame.plain.contains("Cortex CLI v1.0.0"),
                "{:?}\n{}",
                size,
                frame.plain
            );
            assert!(
                frame.plain.contains("Plan, search, build anything")
                    || frame.plain.contains("Plan, search"),
                "{:?}\n{}",
                size,
                frame.plain
            );
            assert!(
                frame.plain.contains("~/cortex-api") || frame.plain.contains("cortex-api"),
                "{:?}\n{}",
                size,
                frame.plain
            );
            assert!(
                frame.plain.contains("cortex-1-mini"),
                "{:?}\n{}",
                size,
                frame.plain
            );
            assert!(!frame.plain.contains("▄█▀▀▀▀█▄"), "{}", frame.plain);
            assert_no_junk(&frame.plain);
            assert!(
                !frame.plain.trim().is_empty(),
                "splash must not be empty at {size:?}"
            );
        }
        let wide = render_lock_scene("splash", 120, 40).expect("splash wide");
        assert!(wide.plain.contains("> cortex") || wide.plain.contains("cortex"));
        assert!(
            wide.plain.contains("/ commands") || wide.plain.contains("commands"),
            "{}",
            wide.plain
        );
        assert!(wide.plain.contains("100% context") || wide.plain.contains("Agent"));
    }

    fn assert_no_junk(plain: &str) {
        for needle in [
            "Guest",
            "Directory",
            "Computer",
            "Display",
            "L a.rs",
            "L example",
            "mocortex",
        ] {
            assert!(!plain.contains(needle), "junk {needle:?} in:\n{plain}");
        }
        if plain.contains("foundatio") {
            assert!(
                plain.contains("foundation"),
                "truncated foundation:\n{plain}"
            );
        }
        if plain.contains("Devi") {
            assert!(
                plain.contains("Device") || plain.contains("device"),
                "truncated Device:\n{plain}"
            );
        }
        assert!(!plain.to_lowercase().contains("grok"));
        assert!(!plain.to_lowercase().contains("claude"));
        assert!(!plain.to_lowercase().contains("fable"));
    }

    #[test]
    fn login_radios_and_states() {
        let select = render_lock_scene("login_select", 120, 40).expect("select");
        assert!(select.plain.contains("Sign in to Cortex"));
        assert!(select.plain.contains("●"));
        assert!(select.plain.contains("○"));
        assert!(select.plain.contains("Continue with browser"));
        assert!(select.plain.contains("Paste an API key"));
        assert!(select.plain.contains("cortex.foundation/cli/auth"));
        assert!(select.plain.contains("token never hits the model"));
        assert!(select.plain.contains("continue") || select.plain.contains("↵"));
        assert!(!select.plain.contains("Guest"));
        assert!(!select.plain.contains("Exit"));
        assert!(!select.plain.contains("▄█▀▀▀▀█▄"));
        let radio_count = select.plain.matches("Continue with browser").count()
            + select.plain.matches("Paste an API key").count();
        assert_eq!(radio_count, 2, "{}", select.plain);
        assert_no_junk(&select.plain);

        let narrow = render_lock_scene("login_select", 40, 12).expect("narrow");
        assert!(
            narrow.plain.contains("Continue with browser"),
            "{}",
            narrow.plain
        );
        assert!(
            narrow.plain.contains("Paste an API key"),
            "{}",
            narrow.plain
        );
        assert!(
            narrow.plain.contains("Sign in to Cortex"),
            "{}",
            narrow.plain
        );
        assert!(!narrow.plain.contains("Devi"));
        assert!(
            !narrow.plain.contains("foundatio") || narrow.plain.contains("foundation"),
            "{}",
            narrow.plain
        );
        let hint_idx = narrow
            .plain
            .find("Opens")
            .or_else(|| narrow.plain.find("token"));
        let paste_idx = narrow.plain.find("Paste an API key");
        if let (Some(h), Some(p)) = (hint_idx, paste_idx) {
            assert!(
                h < p,
                "hint must sit under the selected radio:\n{}",
                narrow.plain
            );
        }
        assert_no_junk(&narrow.plain);

        let waiting = render_lock_scene("login_waiting", 120, 40).expect("waiting");
        assert!(waiting.plain.contains("Waiting for browser"));

        let ok = render_lock_scene("login_success", 80, 24).expect("ok");
        assert!(ok.plain.contains("Signed in."));

        let err = render_lock_scene("login_error", 80, 24).expect("err");
        assert!(err.plain.contains("temporarily unavailable"));
        assert_no_junk(&err.plain);
    }

    #[test]
    fn palette_home_leads_with_lock_order() {
        let frame = render_lock_scene("palette", 120, 40).expect("palette");
        let first = PALETTE_HOME_COMMANDS[..SLASH_VISIBLE].to_vec();
        for name in &first {
            assert!(
                frame.plain.contains(&format!("/{name}")) || frame.plain.contains(name),
                "missing {name}:\n{}",
                frame.plain
            );
        }
        assert!(
            frame.plain.contains("more") && frame.plain.contains("keep typing"),
            "{}",
            frame.plain
        );
        assert!(
            !frame.plain.contains("11 more"),
            "wide palette must show 20 rows, not 10+11:\n{}",
            frame.plain
        );
        let model_idx = frame.plain.find("/model").unwrap_or(usize::MAX);
        let settings_idx = frame.plain.find("/settings").unwrap_or(usize::MAX);
        let help_idx = frame.plain.find("/help").unwrap_or(usize::MAX);
        assert!(
            model_idx < settings_idx,
            "must not lead with /help:\n{}",
            frame.plain
        );
        assert!(
            help_idx == usize::MAX || settings_idx < help_idx,
            "/help must not lead:\n{}",
            frame.plain
        );
        assert!(!frame.plain.contains("/interrupt"));
        assert_no_junk(&frame.plain);

        let narrow = render_lock_scene("palette", 40, 12).expect("narrow palette");
        assert!(
            narrow.plain.contains("/model") || narrow.plain.contains("model"),
            "{}",
            narrow.plain
        );
        assert!(
            narrow.plain.contains("Choose the model")
                || narrow.plain.contains("model for this")
                || narrow.plain.contains("session"),
            "narrow slash should keep a description when it fits:\n{}",
            narrow.plain
        );
        assert!(!narrow.plain.contains("/interrupt"));
        assert!(!narrow.plain.contains("/theme"));
        assert_no_junk(&narrow.plain);
    }

    #[test]
    fn settings_hub_is_lock_rows() {
        let frame = render_lock_scene("settings_hub", 120, 40).expect("settings");
        for section in [
            "Model",
            "Mode",
            "Permissions",
            "Sandbox",
            "MCP",
            "Config",
            "Usage",
        ] {
            assert!(
                frame.plain.contains(section),
                "missing {section}:\n{}",
                frame.plain
            );
        }
        for banned in [
            "Display",
            "Behavior",
            "Privacy",
            "Syntax Highlight",
            "Cloud",
        ] {
            assert!(
                !frame.plain.contains(banned),
                "banned {banned}:\n{}",
                frame.plain
            );
        }
        assert!(
            frame.plain.contains("cortex-1-mini · Medium"),
            "{}",
            frame.plain
        );
        assert!(frame.plain.contains("On · workspace"), "{}", frame.plain);
        assert!(frame.plain.contains("3 of 4 connected"), "{}", frame.plain);
        assert!(
            frame.plain.contains("~/.cortex/config.json"),
            "{}",
            frame.plain
        );
        assert!(
            frame.plain.contains("42 / 500 agent requests"),
            "{}",
            frame.plain
        );
        assert!(
            frame.plain.contains("open") && frame.plain.contains("close"),
            "{}",
            frame.plain
        );
        assert_no_junk(&frame.plain);
        let narrow = render_lock_scene("settings_hub", 40, 12).expect("narrow settings");
        assert!(narrow.plain.contains("Model"), "{}", narrow.plain);
        assert!(narrow.plain.contains("Usage"), "{}", narrow.plain);
        assert!(!narrow.plain.contains("Display"), "{}", narrow.plain);
        assert_no_junk(&narrow.plain);
    }

    #[test]
    fn tool_tiles_one_card() {
        let frame = render_lock_scene("tool_tiles", 120, 40).expect("tiles");
        assert!(frame.plain.contains("Grep"), "{}", frame.plain);
        assert!(frame.plain.contains("rateLimit"), "{}", frame.plain);
        assert!(frame.plain.contains("4 hits"), "{}", frame.plain);
        assert!(!frame.plain.contains("L a.rs"), "{}", frame.plain);
        assert!(!frame.plain.contains("L example"), "{}", frame.plain);
        let narrow = render_lock_scene("tool_tiles", 40, 12).expect("narrow tiles");
        let extra_tiles = ["Write", "Shell", "Glob", "List"]
            .iter()
            .filter(|t| narrow.plain.contains(**t))
            .count();
        assert!(
            extra_tiles == 0,
            "40-col tile must be a single card:\n{}",
            narrow.plain
        );
        let diag = render_lock_scene("diagnostics", 120, 40).expect("diag");
        assert!(diag.plain.contains("Diagnostics"), "{}", diag.plain);
        assert!(diag.plain.contains("error"), "{}", diag.plain);
        assert!(diag.plain.contains("L22"), "{}", diag.plain);
        assert!(diag.plain.contains("warn"), "{}", diag.plain);
        assert!(diag.plain.contains("L47"), "{}", diag.plain);
        assert!(!diag.plain.contains("L a.rs"), "{}", diag.plain);
        assert!(!diag.plain.contains("L example"), "{}", diag.plain);
        let diff = render_lock_scene("multi_diff", 120, 40).expect("diff");
        assert!(diff.plain.contains("Changed this turn"), "{}", diff.plain);
        assert!(diff.plain.contains("4 files"), "{}", diff.plain);
        assert!(diff.plain.contains("+84"), "{}", diff.plain);
        assert!(diff.plain.contains("open"), "{}", diff.plain);
    }

    #[test]
    fn compact_interrupt_clear_and_states_reflow() {
        for size in [(40u16, 12u16), (120u16, 40u16)] {
            for id in [
                "compact",
                "interrupt",
                "clear",
                "session_empty",
                "session_loading",
                "session_error",
                "session_success",
            ] {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                assert!(!frame.plain.trim().is_empty(), "{id} empty at {size:?}");
                assert!(!frame.plain.to_lowercase().contains("grok"));
            }
        }
        let interrupt = render_lock_scene("interrupt", 120, 40).expect("interrupt");
        assert!(
            interrupt.plain.contains("interrupt") || interrupt.plain.contains("Esc"),
            "{}",
            interrupt.plain
        );
        let empty = render_lock_scene("session_empty", 80, 24).expect("empty");
        assert!(empty.plain.contains("Cortex CLI v1.0.0"), "{}", empty.plain);
    }

    #[test]
    fn lock_boards_11_20_product_copy() {
        let always: &[(&str, &[&str])] = &[
            (
                "shell",
                &["Shell npm test", "running", "ctrl+c", "follow-up"],
            ),
            (
                "permission",
                &[
                    "Cortex wants to run",
                    "npm install",
                    "Yes, run once",
                    "Cortex",
                ],
            ),
            (
                "plan",
                &[
                    "Plan",
                    "Implement this plan?",
                    "Agent mode",
                    "keep planning",
                ],
            ),
            ("streaming", &["Done", "rateLimit()", "follow-up"]),
            ("resume", &["/resume", "search sessions", "24 messages"]),
            ("mcp", &["/mcp", "2 of 4 connected", "mcp.json"]),
            ("usage", &["/usage", "Cortex Pro", "Agent requests"]),
            (
                "quota",
                &["Agent quota exhausted", "500 / 500", "held until quota"],
            ),
            ("sandbox", &["/sandbox", "Sandbox mode"]),
            ("cloud", &["Handed off to Cortex Cloud", "bc-4f2a", "/jobs"]),
        ];

        for size in [(40u16, 12u16), (120u16, 40u16)] {
            for (id, needles) in always {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let lower = frame.plain.to_lowercase();
                assert!(!lower.contains("grok"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("claude"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("fable"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("rakazo"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("opencode"), "{id}\n{}", frame.plain);
                assert!(
                    frame.plain.contains("cortex-1-mini"),
                    "{id} footer model at {size:?}:\n{}",
                    frame.plain
                );
                assert!(
                    frame.plain.contains("~/cortex-api") || frame.plain.contains("cortex-api"),
                    "{id} cwd at {size:?}:\n{}",
                    frame.plain
                );
                for needle in *needles {
                    let hit = frame.plain.contains(needle)
                        || needle
                            .split_whitespace()
                            .all(|word| frame.plain.contains(word));
                    assert!(hit, "{id} missing `{needle}` at {size:?}:\n{}", frame.plain);
                }
            }
        }

        let wide_shell = render_lock_scene("shell", 120, 40).expect("shell wide");
        assert!(wide_shell.plain.contains("vitest"), "{}", wide_shell.plain);
        assert!(wide_shell.plain.contains("✓") || wide_shell.plain.contains("rateLimit.test"));

        let perm = render_lock_scene("permission", 120, 40).expect("perm");
        assert!(perm.plain.contains("always allow npm install"));
        assert!(perm.plain.contains("Edit command"));
        assert!(perm.plain.contains("Normal"));
        assert!(perm.plain.contains("tell Cortex"));

        let plan = render_lock_scene("plan", 120, 40).expect("plan");
        assert!(plan.plain.contains("Redis-backed"));
        assert!(plan.plain.contains(" · Plan · ") || plan.plain.contains("Plan ·"));

        let resume = render_lock_scene("resume", 120, 40).expect("resume");
        assert!(resume.plain.contains("Sessions sync through Cortex Cloud"));

        let usage = render_lock_scene("usage", 120, 40).expect("usage");
        assert!(usage.plain.contains("cortex.foundation/billing"));
        assert!(usage.plain.contains("8.4M") || usage.plain.contains("12M"));

        let cloud = render_lock_scene("cloud", 120, 40).expect("cloud");
        assert!(cloud.plain.contains("cortex.foundation/agents"));
        assert!(cloud.plain.contains("Plan, search, build anything"));

        let sandbox = render_lock_scene("sandbox", 120, 40).expect("sandbox");
        assert!(sandbox.plain.contains("Filesystem"));
        assert!(sandbox.plain.contains("space toggle"));
        assert!(sandbox.plain.contains("Smart"));

        let stream = render_lock_scene("streaming", 120, 40).expect("stream");
        assert!(stream.plain.contains("zadd") || stream.plain.contains("rateLimit"));
        assert!(!stream.plain.contains("Read src/") && !stream.plain.contains("Write src/"));

        let mcp = render_lock_scene("mcp", 120, 40).expect("mcp");
        assert!(mcp.plain.contains("authenticating"));
        assert!(mcp.plain.contains("failed"));
    }

    #[test]
    fn lock_boards_21_30_product_copy() {
        let always: &[(&str, &[&str])] = &[
            (
                "sudo",
                &[
                    "sudo",
                    "elevated privileges",
                    "Password for mathis",
                    "Never stored",
                ],
            ),
            ("ask", &["Ask", "read-only", "shift+tab", "Agent mode"]),
            ("files", &["@rate", "rateLimit", "insert", "tab complete"]),
            ("queue", &["Queued", "Retry-After", "ctrl+x clear queue"]),
            ("jobs", &["/jobs", "2 running", "cloud"]),
            ("help", &["/help", "/model", "Shortcuts"]),
            (
                "first_run",
                &["Cortex CLI v1.0.0", "Tips for getting started"],
            ),
            (
                "bash",
                &["Bash mode", "the model is not involved", "redis-cli"],
            ),
            (
                "config",
                &["/config", "~/.cortex/config.json", "cortex-1-mini"],
            ),
            ("footer_max", &["Committed and pushed", "MAX", "& cloud"]),
        ];

        for size in [(40u16, 12u16), (120u16, 40u16)] {
            for (id, needles) in always {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let lower = frame.plain.to_lowercase();
                assert!(!lower.contains("grok"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("claude"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("fable"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("rakazo"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("opencode"), "{id}\n{}", frame.plain);
                assert!(
                    !frame.plain.contains("gpt-"),
                    "{id} must use a Cortex catalog slug:\n{}",
                    frame.plain
                );
                if *id != "footer_max" {
                    assert!(
                        frame.plain.contains("cortex-1-mini") || frame.plain.contains("MAX"),
                        "{id} footer at {size:?}:\n{}",
                        frame.plain
                    );
                }
                for needle in *needles {
                    let hit = frame.plain.contains(needle)
                        || needle
                            .split_whitespace()
                            .all(|word| frame.plain.contains(word));
                    assert!(hit, "{id} missing `{needle}` at {size:?}:\n{}", frame.plain);
                }
            }
        }

        let ask = render_lock_scene("ask", 120, 40).expect("ask");
        assert!(ask.plain.contains("Ask — read-only") || ask.plain.contains("read-only"));
        assert!(ask.plain.contains("estimateTokens") || ask.plain.contains("src/lib/tokens.ts"));
        assert!(ask.plain.contains(" · Ask · ") || ask.plain.contains("Ask"));

        let jobs = render_lock_scene("jobs", 120, 40).expect("jobs");
        assert!(jobs.plain.contains("subagent"));
        assert!(jobs.plain.contains("failed"));
        assert!(jobs.plain.contains("done"));

        let help = render_lock_scene("help", 120, 40).expect("help");
        for cmd in [
            "/model",
            "/jobs",
            "/skills",
            "/settings",
            "/config",
            "/resume",
        ] {
            assert!(help.plain.contains(cmd), "missing {cmd}:\n{}", help.plain);
        }
        assert!(help.plain.contains("cortex.foundation/docs"));
        assert!(help.plain.contains("Cortex CLI v1.0.0"));
        assert!(!help.plain.contains("/interrupt"));

        let help_n = render_lock_scene("help", 40, 12).expect("help narrow");
        assert!(help_n.plain.contains("/model"), "{}", help_n.plain);
        for line in help_n.plain.lines() {
            let t = line.trim();
            if t.contains("foundatio") && !t.contains("foundation") {
                panic!("mid-word cut: {t}");
            }
        }

        let cfg = render_lock_scene("config", 120, 40).expect("config");
        assert!(cfg.plain.contains(".cortex/config.json"));
        assert!(cfg.plain.contains("MAX"));

        let max = render_lock_scene("footer_max", 120, 40).expect("max");
        assert!(max.plain.contains("rate-limit-9e4d"));
        assert!(max.plain.contains("+214"));
        assert!(max.plain.contains("-9"));
        assert!(max.plain.contains("38% context left"));
    }

    #[test]
    fn lock_boards_31_40_product_copy() {
        let always: &[(&str, &[&str])] = &[
            (
                "login",
                &[
                    "Sign in to Cortex",
                    "Continue with browser",
                    "Paste an API key",
                    "continue",
                ],
            ),
            ("thinking", &["Thinking", "follow-up"]),
            (
                "todos",
                &["Working 1/5", "Write ratelimit middleware", "rateLimit.ts"],
            ),
            ("question", &["Where should the limiter live?", "1-9 pick"]),
            ("skills", &["/skills", "/pr", "run once"]),
            ("btw", &["btw", "not added to the main thread"]),
            ("stopped", &["Stopped", "ctrl+c"]),
            ("compacted", &["Thread compacted", "12%"]),
            ("write", &["Write", "+84", "new file"]),
            (
                "clear_confirm",
                &["Start a new thread?", "Clear thread", "Cancel"],
            ),
        ];
        for size in [(40u16, 12u16), (120u16, 40u16)] {
            for (id, needles) in always {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let lower = frame.plain.to_lowercase();
                assert!(!lower.contains("grok"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("claude"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("fable"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("rakazo"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("opencode"), "{id}\n{}", frame.plain);
                for needle in *needles {
                    let hit = frame.plain.contains(needle)
                        || needle
                            .split_whitespace()
                            .all(|word| frame.plain.contains(word));
                    assert!(hit, "{id} missing `{needle}` at {size:?}:\n{}", frame.plain);
                }
            }
        }

        let login = render_lock_scene("login", 120, 40).expect("login");
        assert!(login.plain.contains("Cortex CLI v1.0.0"));
        assert!(login.plain.contains("cortex.foundation/cli/auth"));
        assert!(!login.plain.contains("Guest"));
        assert_eq!(login.plain.matches("Continue with browser").count(), 1);

        let think = render_lock_scene("thinking", 120, 40).expect("think");
        assert!(think.plain.contains("ZADD") || think.plain.contains("sliding window"));

        let q = render_lock_scene("question", 120, 40).expect("q");
        assert!(q.plain.contains("Shared limiter"));
        assert!(q.plain.contains("Plan"));

        let skills = render_lock_scene("skills", 120, 40).expect("skills");
        for cmd in ["/commit", "/pr", "/review", "/fix-ci", "/migrate"] {
            assert!(skills.plain.contains(cmd), "{cmd}\n{}", skills.plain);
        }

        let compact = render_lock_scene("compacted", 120, 40).expect("compacted");
        assert!(compact.plain.contains("86%"));
        assert!(compact.plain.contains("unchanged"));
    }

    #[test]
    fn lock_boards_41_50_product_copy() {
        let always: &[(&str, &[&str])] = &[
            ("grep", &["Grep", "rateLimit", "4 hits", "import"]),
            ("glob", &["Glob", "**/*rate*", "4 files", "rateLimit.ts"]),
            (
                "delete",
                &[
                    "Delete",
                    "rateLimit.legacy.ts",
                    "File will be removed from disk",
                    "Keep",
                ],
            ),
            (
                "list",
                &["List", "src/middleware", "4 entries", "internal/"],
            ),
            ("fetch", &["Fetch", "redis.io", "ZADD"]),
            ("mcp_call", &["MCP", "list_issues", "team=API", "API-184"]),
            (
                "task",
                &["Task", "Write integration tests", "vitest", "18s"],
            ),
            (
                "diagnostics",
                &["Diagnostics", "rateLimit.ts", "error", "L22", "warn", "L47"],
            ),
            (
                "multi_diff",
                &["/diff", "Changed this turn", "4 files", "+84"],
            ),
            (
                "settings_hub",
                &["/settings", "Settings", "Model", "Usage", "Permissions"],
            ),
            ("edit", &["Edit", "completions.ts", "+9"]),
        ];
        for size in [(40u16, 12u16), (120u16, 40u16)] {
            for (id, needles) in always {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let lower = frame.plain.to_lowercase();
                assert!(!lower.contains("grok"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("claude"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("fable"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("rakazo"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("opencode"), "{id}\n{}", frame.plain);
                assert!(!frame.plain.contains("L example"), "{id}\n{}", frame.plain);
                assert!(!frame.plain.contains("L a.rs"), "{id}\n{}", frame.plain);
                for needle in *needles {
                    let hit = frame.plain.contains(needle)
                        || needle
                            .split_whitespace()
                            .all(|word| frame.plain.contains(word));
                    assert!(hit, "{id} missing `{needle}` at {size:?}:\n{}", frame.plain);
                }
            }
        }

        let grep = render_lock_scene("grep", 120, 40).expect("grep");
        assert!(grep.plain.contains("429") || grep.plain.contains("rate_limited"));
        let glob = render_lock_scene("glob", 120, 40).expect("glob");
        assert!(glob.plain.contains("docs/rate-limiting.md"));
        let del = render_lock_scene("delete", 120, 40).expect("delete");
        assert!(del.plain.contains("Undo via git"));
        assert!(del.plain.contains("esc keep") || del.plain.contains("keep"));
        let list = render_lock_scene("list", 120, 40).expect("list");
        assert!(list.plain.contains("auth.ts"));
        assert!(list.plain.contains("cors.ts"));
        let fetch = render_lock_scene("fetch", 120, 40).expect("fetch");
        assert!(fetch.plain.contains("sorted set") || fetch.plain.contains("ZADD"));
        let mcp = render_lock_scene("mcp_call", 120, 40).expect("mcp_call");
        assert!(mcp.plain.contains("API-191"));
        let settings = render_lock_scene("settings_hub", 120, 40).expect("settings");
        for banned in ["Display", "Behavior", "Privacy", "Cloud"] {
            assert!(
                !settings.plain.contains(banned),
                "{banned}\n{}",
                settings.plain
            );
        }
        assert!(settings.plain.contains("cortex-1-mini"));
        assert!(!settings.plain.contains("gpt-"));
        let diff = render_lock_scene("multi_diff", 120, 40).expect("diff");
        assert!(diff.plain.contains("completions.ts"));
        assert!(diff.plain.contains("-2"));
    }
}
