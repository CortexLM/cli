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
use cortex_tui_components::welcome_card::WelcomeCard;
use ratatui::style::Style;
use ratatui::widgets::{Clear, Widget};
use serde::Serialize;
use serde_json::json;

use crate::app::{AppState, AutocompleteItem, AutocompleteTrigger};
use crate::commands::{CommandRegistry, CompletionEngine, PALETTE_HOME_COMMANDS};
use crate::interactive::builders::build_settings_hub;
use crate::runner::login_screen::LoginScreen;
use crate::views::minimal_session::MinimalSessionView;
use crate::views::tool_call::{ToolCallDisplay, ToolResultDisplay, ToolStatus};

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
                    WelcomeCard::new()
                        .version(LOCK_SPLASH_VERSION)
                        .render(area, frame.buffer_mut());
                }
                "login_select" => LoginScreen::lock_select(LOCK_SPLASH_VERSION, None).render(frame),
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
                "palette" => draw_session(frame, palette_state(false)),
                "palette_empty" => draw_session(frame, palette_state(true)),
                "settings_hub" => draw_session(frame, settings_state(false, height)),
                "settings_empty" => draw_session(frame, settings_state(true, height)),
                "tool_tiles" => draw_session(frame, tool_tiles_state()),
                "diagnostics" => draw_session(frame, diagnostics_state()),
                "multi_diff" => draw_session(frame, multi_diff_state()),
                "compact" => draw_session(frame, compact_state()),
                "interrupt" => draw_session(frame, interrupt_state()),
                "clear" => draw_session(frame, clear_state()),
                "session_empty" => draw_session(frame, empty_session_state()),
                "session_loading" => draw_session(frame, loading_session_state()),
                "session_error" => draw_session(frame, error_session_state()),
                "session_success" => draw_session(frame, success_session_state()),
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
    state.autocomplete.max_visible = PALETTE_HOME_COMMANDS.len();
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

fn push_tool(
    state: &mut AppState,
    id: &str,
    name: &str,
    args: serde_json::Value,
    sequence: u64,
    summary: &str,
    success: bool,
) {
    let mut call = ToolCallDisplay::new(id.into(), name.into(), args, sequence);
    call.set_status(if success {
        ToolStatus::Completed
    } else {
        ToolStatus::Failed
    });
    call.set_result(ToolResultDisplay {
        output: summary.into(),
        success,
        summary: summary.into(),
    });
    state.tool_calls.push(call);
}

fn tool_tiles_state() -> AppState {
    let mut state = lock_app();
    state.add_message(Message::user("run tools"));
    let tiles = [
        ("1", "Read", json!({"file_path": "a.rs"}), "a.rs"),
        ("2", "Write", json!({"file_path": "b.rs"}), "+12"),
        ("3", "Edit", json!({"file_path": "c.rs"}), "+3 −1"),
        ("4", "Shell", json!({"command": "ls"}), "$ ls"),
        ("5", "Grep", json!({"pattern": "fn"}), "fn"),
        ("6", "Glob", json!({"pattern": "*.rs"}), "*.rs"),
        ("7", "Delete", json!({"file_path": "gone.rs"}), "gone.rs"),
        ("8", "List", json!({"path": "src"}), "src"),
        (
            "9",
            "Fetch",
            json!({"url": "https://example.test"}),
            "example",
        ),
        ("10", "mcp__docs", json!({}), "docs"),
        ("11", "Task", json!({"description": "explore"}), "explore"),
    ];
    for (id, name, args, summary) in tiles {
        push_tool(
            &mut state,
            id,
            name,
            args,
            id.parse().unwrap_or(1),
            summary,
            true,
        );
    }
    state
}

fn diagnostics_state() -> AppState {
    let mut state = lock_app();
    state.add_message(Message::user("lint a.rs"));
    push_tool(
        &mut state,
        "d1",
        "diagnostics",
        json!({"file": "a.rs"}),
        1,
        "2 warnings",
        true,
    );
    state
}

fn multi_diff_state() -> AppState {
    let mut state = lock_app();
    state.add_message(Message::user("show the diff"));
    push_tool(
        &mut state,
        "diff1",
        "diff",
        json!({"file": "a.rs"}),
        1,
        "+12 −3",
        true,
    );
    push_tool(
        &mut state,
        "diff2",
        "multidiff",
        json!({"files": ["a.rs", "b.rs"]}),
        2,
        "+4 −1",
        true,
    );
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

    #[test]
    fn splash_is_one_line_v1_no_mascot() {
        let frame = render_lock_scene("splash", 40, 12).expect("splash");
        assert!(frame.plain.contains("Cortex CLI v1.0.0"), "{}", frame.plain);
        assert!(!frame.plain.contains("▄█▀▀▀▀█▄"));
        let occupied: Vec<_> = frame
            .plain
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        assert_eq!(occupied.len(), 1, "splash must be one line: {occupied:?}");
    }

    #[test]
    fn login_radios_and_states() {
        let select = render_lock_scene("login_select", 120, 40).expect("select");
        assert!(select.plain.contains("(●)"));
        assert!(select.plain.contains("( )"));
        assert!(select.plain.contains("Cortex Foundation account"));
        assert!(!select.plain.contains("▄█▀▀▀▀█▄"));

        let waiting = render_lock_scene("login_waiting", 120, 40).expect("waiting");
        assert!(waiting.plain.contains("Waiting for browser authentication"));

        let ok = render_lock_scene("login_success", 80, 24).expect("ok");
        assert!(ok.plain.contains("Signed in."));

        let err = render_lock_scene("login_error", 80, 24).expect("err");
        assert!(err.plain.contains("temporarily unavailable"));
        assert!(!err.plain.to_lowercase().contains("grok"));
    }

    #[test]
    fn palette_home_shows_twenty_on_wide() {
        let frame = render_lock_scene("palette", 120, 40).expect("palette");
        for name in PALETTE_HOME_COMMANDS {
            assert!(
                frame.plain.contains(name) || frame.plain.contains(&format!("/{name}")),
                "missing {name}:\n{}",
                frame.plain
            );
        }
        let command_rows = frame
            .plain
            .lines()
            .filter(|line| {
                line.contains('/') && PALETTE_HOME_COMMANDS.iter().any(|n| line.contains(n))
            })
            .count();
        assert!(
            command_rows <= PALETTE_HOME_COMMANDS.len(),
            "must not dump extra palette rows: {command_rows}"
        );
    }

    #[test]
    fn settings_hub_is_sections() {
        let frame = render_lock_scene("settings_hub", 120, 40).expect("settings");
        for section in ["Display", "Behavior", "AI", "Git", "Cloud", "Privacy"] {
            assert!(
                frame.plain.contains(section),
                "missing {section}:\n{}",
                frame.plain
            );
        }
        assert!(!frame.plain.contains("Syntax Highlight"));
    }

    #[test]
    fn tool_tiles_match_lock() {
        let frame = render_lock_scene("tool_tiles", 120, 40).expect("tiles");
        for tile in [
            "Read", "Write", "Edit", "Shell", "Grep", "Glob", "Delete", "List", "Fetch", "MCP",
            "Task",
        ] {
            assert!(
                frame.plain.contains(tile),
                "missing {tile}:\n{}",
                frame.plain
            );
        }
        let diag = render_lock_scene("diagnostics", 120, 40).expect("diag");
        assert!(diag.plain.contains("Diagnostics"), "{}", diag.plain);
        let diff = render_lock_scene("multi_diff", 120, 40).expect("diff");
        assert!(diff.plain.contains("Diff"), "{}", diff.plain);
        assert!(diff.plain.contains('+'), "{}", diff.plain);
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
}
