//! Headless visual-lock captures for PR review.
//!
//! Renders the real session, login, palette, and settings widgets through
//! [`cortex_tui_capture::MockTerminal`] and writes ANSI frames a rasteriser
//! turns into PNGs.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cortex_core::widgets::Message;
use cortex_tui_capture::{CaptureConfig, MockTerminal, StyleRendering};
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
        "typing",
        "login_select",
        "login_waiting",
        "login_success",
        "login_error",
        "palette",
        "palette_empty",
        "model_compact",
        "model_full",
        "mode",
        "permissions",
        "working",
        "read",
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
            // The chrome never paints its own background: cells stay on
            // `Color::Reset` so the host terminal (black by default) shows
            // through.
            frame.render_widget(Clear, area);
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
                // States 37, 38 and 40 are Designer boards, not the live
                // session view: tiles stay on interrupt, compact reports the
                // summary, and clear is the confirm dialog.
                "interrupt" => {
                    crate::lock_boards::render_lock_board("stopped", area, frame.buffer_mut());
                }
                "compact" => {
                    crate::lock_boards::render_lock_board("compacted", area, frame.buffer_mut());
                }
                "clear" => {
                    crate::lock_boards::render_lock_board(
                        "clear_confirm",
                        area,
                        frame.buffer_mut(),
                    );
                }
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
    state.footer_cwd = "~/cortex-api".into();
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
    // Same two lines the runtime emits: the product error, then what to do.
    state.add_message(Message::system(PRODUCT_ERROR));
    state.add_message(Message::system(
        crate::ui::consts::SERVICE_UNAVAILABLE_NEXT_STEP,
    ));
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

    /// Every character the ANSI stream paints with `fg`, in order.
    ///
    /// `to_ansi` resets styles (`ESC[0m`) before each change, so tracking the
    /// last `38;2;…` foreground since the previous reset is exact.
    fn painted_chars(ansi: &str, fg: &str) -> String {
        let mut painted = String::new();
        let mut active = false;
        let mut rest = ansi;
        while let Some(start) = rest.find('\x1b') {
            if active {
                painted.push_str(&rest[..start]);
            }
            rest = &rest[start..];
            let Some(end) = rest.find('m') else {
                break;
            };
            let params = &rest[2..end];
            if params == "0" {
                active = false;
            } else if params.starts_with("38;2;") {
                active = params == fg;
            }
            rest = &rest[end + 1..];
        }
        if active {
            painted.push_str(rest);
        }
        painted.retain(|c| c != '\n');
        painted
    }

    /// Locked violet accent `#A78BFA` as an SGR foreground.
    const ACCENT_FG: &str = "38;2;167;139;250";
    /// Locked diff green `#4ADE80` as an SGR foreground.
    const DIFF_GREEN_FG: &str = "38;2;74;222;128";

    fn accent_painted_chars(ansi: &str) -> String {
        painted_chars(ansi, ACCENT_FG)
    }

    #[test]
    fn violet_is_reserved_for_markers_everywhere() {
        // The locked chrome allows violet on the `>` prompt, the `●` dot,
        // `✓` checks and small stats — never on a command name, label or
        // sentence. Login sub-states are locked as shipped.
        let locked_login = ["login_waiting", "login_success", "login_error"];
        for id in lock_scene_ids() {
            if locked_login.contains(id) {
                continue;
            }
            for size in [(40u16, 12u16), (120u16, 40u16)] {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let painted = accent_painted_chars(&frame.ansi);
                assert!(
                    painted
                        .chars()
                        .all(|c| matches!(c, '>' | '●' | '✓' | '%' | '0'..='9' | ' ')),
                    "{id} paints violet outside the marker set at {size:?}: {painted:?}"
                );
            }
        }
    }

    #[test]
    fn green_is_reserved_for_diff_additions_everywhere() {
        // The only green in the chrome is `+N` / `+` diff additions.
        for id in lock_scene_ids() {
            for size in [(40u16, 12u16), (120u16, 40u16)] {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let painted = painted_chars(&frame.ansi, DIFF_GREEN_FG);
                assert!(
                    painted.chars().all(|c| matches!(c, '+' | '0'..='9' | ' ')),
                    "{id} paints green outside +diff at {size:?}: {painted:?}"
                );
            }
        }
        // Diff additions really are green, not violet.
        for id in ["footer_max", "write", "edit", "multi_diff"] {
            let frame = render_lock_scene(id, 120, 40).expect(id);
            let painted = painted_chars(&frame.ansi, DIFF_GREEN_FG);
            assert!(
                painted.contains('+'),
                "{id} must paint its +diff green: {painted:?}"
            );
        }
    }

    #[test]
    fn banned_colors_never_painted() {
        // The mint chrome is dead: #00F5D4 and #1A3330 are banned outright,
        // the old brand green #00FFA3 with them, and no scene paints the
        // navy #0A1628 wash — the host terminal owns the background.
        const BANNED: [&str; 4] = ["0;245;212", "26;51;48", "0;255;163", "10;22;40"];
        for id in lock_scene_ids() {
            for size in [(40u16, 12u16), (120u16, 40u16)] {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                for banned in BANNED {
                    assert!(
                        !frame.ansi.contains(banned),
                        "{id} paints banned color {banned} at {size:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn model_slugs_never_shown() {
        // Users see English product names — `Cortex Mini 1`, never the
        // served `cortex-1-mini` / `cortex-1-max` / `cortex-1` slugs.
        for id in lock_scene_ids() {
            for size in [(40u16, 12u16), (120u16, 40u16)] {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                assert!(
                    !frame.plain.contains("cortex-1"),
                    "{id} shows a model slug at {size:?}:\n{}",
                    frame.plain
                );
                assert!(
                    !frame.plain.contains("cortex-mini") && !frame.plain.contains("cortex-max"),
                    "{id} shows a reordered slug at {size:?}:\n{}",
                    frame.plain
                );
            }
        }
        for id in ["splash", "settings_hub", "model_compact", "model_full"] {
            let frame = render_lock_scene(id, 120, 40).expect(id);
            assert!(
                frame.plain.contains("Cortex Mini 1"),
                "{id} must show the product model name:\n{}",
                frame.plain
            );
        }
    }

    #[test]
    fn no_smashed_tokens_anywhere() {
        // A code span is always followed by a space: never
        // `estimateTokens(prompt)counts` or `rateLimit()checks`.
        for id in lock_scene_ids() {
            for size in [(40u16, 12u16), (120u16, 40u16)] {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let chars: Vec<char> = frame.plain.chars().collect();
                for pair in chars.windows(2) {
                    assert!(
                        !(pair[0] == ')' && pair[1].is_ascii_alphabetic()),
                        "{id} smashes a code span into the next word at {size:?}:\n{}",
                        frame.plain
                    );
                }
            }
        }
        let ask = render_lock_scene("ask", 120, 40).expect("ask");
        for needle in [
            "estimateTokens(prompt) counts",
            "usage.completion_tokens arrives",
            "reconcileUsage() corrects",
            "┐ Cortex will not",
        ] {
            assert!(
                ask.plain.contains(needle),
                "ask missing `{needle}`:\n{}",
                ask.plain
            );
        }
        let stream = render_lock_scene("streaming", 120, 40).expect("streaming");
        for needle in ["rateLimit() checks", "ZADD records", "429 is returned"] {
            assert!(
                stream.plain.contains(needle),
                "streaming missing `{needle}`:\n{}",
                stream.plain
            );
        }
        // Code excerpts keep their indentation.
        assert!(
            stream.plain.contains("  const now = Date.now();"),
            "{}",
            stream.plain
        );
        let mcp = render_lock_scene("mcp_call", 120, 40).expect("mcp_call");
        assert!(
            mcp.plain.contains("429 body    In Progress"),
            "{}",
            mcp.plain
        );
    }

    #[test]
    fn live_states_keep_chrome_complete() {
        // Empty is allowed, but the chrome is whole at both sizes: version,
        // keystroke hints, composer, cwd + model footer.
        for id in [
            "session_empty",
            "session_loading",
            "session_error",
            "palette_empty",
        ] {
            for size in [(40u16, 12u16), (120u16, 40u16)] {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                for needle in ["Cortex CLI v1.0.0", "> ", "~/cortex-api", "Cortex Mini 1"] {
                    assert!(
                        frame.plain.contains(needle),
                        "{id} missing `{needle}` at {size:?}:\n{}",
                        frame.plain
                    );
                }
                assert!(
                    !frame.plain.contains('▐'),
                    "{id} must not overflow into a scrollbar at {size:?}:\n{}",
                    frame.plain
                );
            }
        }
        for size in [(40u16, 12u16), (120u16, 40u16)] {
            let empty = render_lock_scene("session_empty", size.0, size.1).expect("empty");
            assert!(empty.plain.contains("/ commands"), "{}", empty.plain);
            assert!(
                empty.plain.contains("Plan, search, build anything"),
                "{}",
                empty.plain
            );

            // The error is never a lone line: what happened, then what to do
            // (the sentence may wrap at 40 columns, never mid-word).
            let error = render_lock_scene("session_error", size.0, size.1).expect("error");
            assert!(
                error.plain.contains("temporarily") && error.plain.contains("unavailable"),
                "{}",
                error.plain
            );
            assert!(
                error.plain.contains("Try again in a moment"),
                "{}",
                error.plain
            );

            // A live run says Working, offers esc, and invites a follow-up.
            let loading = render_lock_scene("session_loading", size.0, size.1).expect("loading");
            assert!(loading.plain.contains("Working"), "{}", loading.plain);
            assert!(
                loading.plain.contains("esc to interrupt"),
                "{}",
                loading.plain
            );
            assert!(
                loading.plain.contains("Add a follow-up"),
                "{}",
                loading.plain
            );

            // No-match states keep their panel: title, filter and a real
            // empty copy, over the session footer.
            let settings = render_lock_scene("settings_empty", size.0, size.1).expect("settings");
            assert!(settings.plain.contains("Settings"), "{}", settings.plain);
            assert!(
                settings.plain.contains("zzzz-no-such-setting"),
                "{}",
                settings.plain
            );
            assert!(
                settings.plain.contains("No settings match")
                    || settings.plain.contains("No matches for"),
                "{}",
                settings.plain
            );
            assert!(
                settings.plain.contains("esc clears the search"),
                "{}",
                settings.plain
            );
            assert!(
                settings.plain.contains("~/cortex-api"),
                "{}",
                settings.plain
            );
            let palette = render_lock_scene("palette_empty", size.0, size.1).expect("palette");
            assert!(
                palette.plain.contains("No matching commands"),
                "{}",
                palette.plain
            );
        }
    }

    #[test]
    fn footer_shows_product_model_everywhere() {
        // Every session footer names the model as a product — `Cortex Mini 1`
        // — at both sizes. Only the login sub-states have no footer.
        let no_footer = ["login_waiting", "login_success", "login_error"];
        for id in lock_scene_ids() {
            if no_footer.contains(id) {
                continue;
            }
            for size in [(40u16, 12u16), (120u16, 40u16)] {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let footer = frame.plain.lines().last().unwrap_or_default();
                assert!(
                    footer.contains("Cortex Mini 1"),
                    "{id} footer must name Cortex Mini 1 at {size:?}: {footer:?}"
                );
            }
        }
        // The MAX badge keeps the model beside it, even at 40 columns.
        for id in ["footer_max", "config"] {
            let frame = render_lock_scene(id, 40, 12).expect(id);
            let footer = frame.plain.lines().last().unwrap_or_default();
            assert!(
                footer.contains("Cortex Mini 1 · MAX"),
                "{id} footer must read Cortex Mini 1 · MAX: {footer:?}"
            );
        }
    }

    #[test]
    fn mode_chips_are_kept() {
        // `┌ Ask — read-only ┐` and `┌ Bash mode ┐` are square mode chips,
        // not session frames — they stay at both sizes.
        for size in [(40u16, 12u16), (120u16, 40u16)] {
            let ask = render_lock_scene("ask", size.0, size.1).expect("ask");
            assert!(
                ask.plain.contains("┌ Ask — read-only ┐"),
                "the Ask mode chip must stay at {size:?}:\n{}",
                ask.plain
            );
            let bash = render_lock_scene("bash", size.0, size.1).expect("bash");
            assert!(
                bash.plain.contains("┌ Bash mode ┐"),
                "the Bash mode chip must stay at {size:?}:\n{}",
                bash.plain
            );
        }
    }

    #[test]
    fn no_rounded_frame_glyphs_anywhere() {
        // Zero rounded frames: the TUI bleeds to the terminal edges and no
        // scene draws a ╭╮╰╯ box. Square `┌ … ┐` mode chips are not frames
        // and are allowed.
        for id in lock_scene_ids() {
            for size in [(40u16, 12u16), (120u16, 40u16)] {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                for glyph in ['╭', '╮', '╰', '╯'] {
                    assert!(
                        !frame.plain.contains(glyph),
                        "{id} draws a rounded frame glyph {glyph} at {size:?}:\n{}",
                        frame.plain
                    );
                }
            }
        }
    }

    #[test]
    fn slash_palette_paints_violet_on_the_marker_only() {
        for size in [(40u16, 12u16), (120u16, 40u16)] {
            let frame = render_lock_scene("palette", size.0, size.1).expect("palette");
            let painted = accent_painted_chars(&frame.ansi);
            assert!(
                painted.chars().all(|c| matches!(c, '>' | ' ')),
                "violet must stay on the `>` marker at {size:?}; painted {painted:?}"
            );
            assert!(
                painted.contains('>'),
                "the prompt marker must stay violet at {size:?}"
            );
        }
        // The wide selected row keeps the 40×12 tone: dim description on the
        // dark #221A38 bar, never a bright (or violet) command row.
        let wide = render_lock_scene("palette", 120, 40).expect("palette wide");
        assert!(
            wide.ansi
                .contains("\x1b[38;2;130;154;177m\x1b[48;2;34;26;56m"),
            "selected description must be dim on the selection bar"
        );
    }

    #[test]
    fn no_inverted_accent_selection_anywhere() {
        // Violet is highlight-only: `>`, small accents, success. A selected
        // row is a dark #221A38 bar with light text, never text-on-violet.
        const ACCENT_BG: &str = "48;2;167;139;250";
        const DARK_SELECTION_BG: &str = "48;2;34;26;56";
        for id in lock_scene_ids() {
            for size in [(40u16, 12u16), (120u16, 40u16)] {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                assert!(
                    !frame.ansi.contains(ACCENT_BG),
                    "{id} paints an inverted violet bar at {size:?}"
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
    fn session_stays_interactive_while_running() {
        // Not a frozen session: while a run is streaming, stdin stays alive —
        // the composer keeps rendering and a submitted follow-up is queued
        // instead of dropped.
        let config = capture_config(120, 40);
        let mut terminal = MockTerminal::from_config(config.clone()).expect("terminal");
        let mut state = loading_session_state();
        assert!(state.streaming.is_streaming, "the run must be live");
        state.queue_message("also add a Retry-After header".to_string());
        assert_eq!(state.queued_count(), 1, "follow-ups queue while running");
        terminal
            .draw(|frame| draw_session(frame, state))
            .map_err(|err| anyhow::anyhow!("{err}"))
            .expect("draw");
        let snapshot = terminal.snapshot();
        let plain = snapshot.to_ascii(&config);
        assert!(
            plain.contains("[1 pending]"),
            "queued follow-up badge must render:\n{plain}"
        );
        assert!(
            plain.contains("> "),
            "the composer stays on screen during a run:\n{plain}"
        );
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
                frame.plain.contains("Cortex Mini 1"),
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
            frame.plain.contains("Cortex Mini 1 · Medium"),
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
                assert!(
                    !frame.plain.contains("~/cli "),
                    "{id} must use the ~/cortex-api cwd at {size:?}:\n{}",
                    frame.plain
                );
            }
        }
        // 37 interrupt keeps the prompt and tool tiles on screen, then shows
        // `✗ Stopped` — never a lone `/interrupt` line or a bare splash.
        for size in [(40u16, 12u16), (120u16, 40u16)] {
            let interrupt = render_lock_scene("interrupt", size.0, size.1).expect("interrupt");
            assert!(interrupt.plain.contains("✗"), "{}", interrupt.plain);
            assert!(interrupt.plain.contains("Stopped"), "{}", interrupt.plain);
            assert!(interrupt.plain.contains("ctrl+c"), "{}", interrupt.plain);
            assert!(
                interrupt.plain.contains("completions.ts"),
                "tiles must stay on screen:\n{}",
                interrupt.plain
            );
            assert!(
                !interrupt.plain.contains("/interrupt"),
                "{}",
                interrupt.plain
            );
            assert!(
                !interrupt.plain.contains("Cortex CLI v1.0.0"),
                "37 must not be a splash:\n{}",
                interrupt.plain
            );
        }
        // 38 compact reports the compaction, on the locked chrome.
        for size in [(40u16, 12u16), (120u16, 40u16)] {
            let compacted = render_lock_scene("compact", size.0, size.1).expect("compact");
            assert!(compacted.plain.contains("/compact"), "{}", compacted.plain);
            assert!(
                compacted.plain.contains("Thread compacted"),
                "{}",
                compacted.plain
            );
            assert!(compacted.plain.contains("86%"), "{}", compacted.plain);
            assert!(compacted.plain.contains("12%"), "{}", compacted.plain);
            assert!(
                compacted.plain.contains("~/cortex-api"),
                "{}",
                compacted.plain
            );
        }
        // 40 clear is the confirm dialog, not an empty splash.
        for size in [(40u16, 12u16), (120u16, 40u16)] {
            let clear = render_lock_scene("clear", size.0, size.1).expect("clear");
            assert!(
                clear.plain.contains("Start a new thread?"),
                "{}",
                clear.plain
            );
            assert!(clear.plain.contains("Clear thread"), "{}", clear.plain);
            assert!(clear.plain.contains("Cancel"), "{}", clear.plain);
            assert!(
                !clear.plain.contains("Cortex CLI v1.0.0"),
                "40 must not be a splash:\n{}",
                clear.plain
            );
        }
        let empty = render_lock_scene("session_empty", 80, 24).expect("empty");
        assert!(empty.plain.contains("Cortex CLI v1.0.0"), "{}", empty.plain);
    }

    #[test]
    fn diagnostics_severity_words_carry_the_only_color() {
        // 48 diagnostics: `error` is red and `warn` is amber — the message
        // and the path stay gray/white.
        const RED_FG: &str = "\x1b[38;2;255;107;107m";
        const AMBER_FG: &str = "\x1b[38;2;255;200;87m";

        /// Visible text right after `marker`, skipping SGR runs and spaces.
        fn painted_after<'a>(ansi: &'a str, marker: &str) -> &'a str {
            let at = ansi.find(marker).map(|i| i + marker.len()).unwrap_or(0);
            let mut rest = &ansi[at..];
            loop {
                rest = rest.trim_start();
                let Some(stripped) = rest.strip_prefix('\x1b') else {
                    return rest;
                };
                let Some(end) = stripped.find('m') else {
                    return rest;
                };
                rest = &stripped[end + 1..];
            }
        }

        for size in [(40u16, 12u16), (120u16, 40u16)] {
            let frame = render_lock_scene("diagnostics", size.0, size.1).expect("diagnostics");
            assert!(frame.ansi.contains(RED_FG), "error must be red at {size:?}");
            assert!(
                frame.ansi.contains(AMBER_FG),
                "warn must be amber at {size:?}"
            );
            assert!(
                painted_after(&frame.ansi, RED_FG).starts_with("error"),
                "red is reserved for the word `error` at {size:?}"
            );
            assert!(
                painted_after(&frame.ansi, AMBER_FG).starts_with("warn"),
                "amber is reserved for the word `warn` at {size:?}"
            );
            // One red run and one amber run — the color never bleeds into
            // the message copy.
            assert_eq!(frame.ansi.matches(RED_FG).count(), 1, "{size:?}");
            assert_eq!(frame.ansi.matches(AMBER_FG).count(), 1, "{size:?}");
        }
    }

    #[test]
    fn tool_tile_dots_are_violet() {
        // Every tool tile paints its `●` status dot violet, exactly like the
        // locked Grep tile. Labels stay white.
        let tiles = [
            "tool_tiles",
            "grep",
            "read",
            "plan",
            "write",
            "glob",
            "edit",
            "delete",
            "list",
            "fetch",
            "mcp_call",
            "task",
            "diagnostics",
            "shell",
            "sudo",
            "queue",
            "stopped",
            "interrupt",
            "footer_max",
            "permission",
        ];
        for id in tiles {
            for size in [(40u16, 12u16), (120u16, 40u16)] {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let painted = accent_painted_chars(&frame.ansi);
                assert!(
                    painted.contains('●'),
                    "{id} must paint its tile dot violet at {size:?}; painted {painted:?}"
                );
            }
        }
    }

    #[test]
    fn lock_boards_02_09_product_copy() {
        let always: &[(&str, &[&str])] = &[
            ("typing", &["Cortex CLI v1.0.0", "Add rate limiting", "█"]),
            (
                "model_compact",
                &[
                    "/model",
                    "Model",
                    "Cortex Mini 1",
                    "Cortex Max 1",
                    "current",
                ],
            ),
            (
                "model_full",
                &["/model", "Model", "Cortex Mini 1", "Cortex Max 1"],
            ),
            ("mode", &["/mode", "Agent", "Plan", "Ask"]),
            (
                "permissions",
                &["/permissions", "Read-only", "Smart", "Full access"],
            ),
            ("working", &["Working", "esc to interrupt", "follow-up"]),
            ("read", &["Read", "completions.ts", "141 lines"]),
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
                    "{id} must use Cortex product model names:\n{}",
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

        let typing = render_lock_scene("typing", 120, 40).expect("typing");
        assert!(typing.plain.contains("> cortex"), "{}", typing.plain);
        assert!(
            typing.plain.contains("Redis-backed, with tests█"),
            "typing must end with the block cursor:\n{}",
            typing.plain
        );

        let full = render_lock_scene("model_full", 120, 40).expect("model_full");
        assert!(full.plain.contains("Effort"), "{}", full.plain);
        assert!(full.plain.contains("● Medium"), "{}", full.plain);
        assert!(
            full.plain.contains("cortex.foundation/billing"),
            "{}",
            full.plain
        );

        let mode = render_lock_scene("mode", 120, 40).expect("mode");
        assert!(
            mode.plain.contains("shift+tab cycles modes"),
            "{}",
            mode.plain
        );

        let read = render_lock_scene("read", 120, 40).expect("read");
        assert!(read.plain.contains("requireApiKey"), "{}", read.plain);
        assert!(read.plain.contains("21"), "{}", read.plain);
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
                    frame.plain.contains("Cortex Mini 1"),
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

        // At 40 columns option 2 wraps — `project` lands on its own line and
        // is never dropped.
        let perm_n = render_lock_scene("permission", 40, 12).expect("perm narrow");
        assert!(
            perm_n.plain.lines().any(|line| line.trim() == "project"),
            "option copy must wrap, not truncate:\n{}",
            perm_n.plain
        );
        assert!(perm_n.plain.contains("Edit command"), "{}", perm_n.plain);

        let plan = render_lock_scene("plan", 120, 40).expect("plan");
        assert!(plan.plain.contains("Redis-backed"));
        assert!(plan.plain.contains(" · Plan · ") || plan.plain.contains("Plan ·"));
        let plan_n = render_lock_scene("plan", 40, 12).expect("plan narrow");
        assert!(
            plan_n.plain.contains("implement"),
            "narrow plan must wrap the confirm label, never truncate it:\n{}",
            plan_n.plain
        );

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
                &["/config", "~/.cortex/config.json", "Cortex Mini 1"],
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
                    "{id} must use a Cortex product model name:\n{}",
                    frame.plain
                );
                if *id != "footer_max" {
                    assert!(
                        frame.plain.contains("Cortex Mini 1") || frame.plain.contains("MAX"),
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

        // 23 @files never cuts inside a filename: at 40 columns the full
        // names win and the timestamp is dropped where both cannot fit.
        let files_n = render_lock_scene("files", 40, 12).expect("files narrow");
        assert!(
            files_n.plain.contains("src/middleware/rateLimit.ts"),
            "full filename required:\n{}",
            files_n.plain
        );
        assert!(
            files_n.plain.contains("src/config/rateLimits.json"),
            "full filename required:\n{}",
            files_n.plain
        );
        assert!(
            !files_n.plain.contains("rateLimit.ts  edited"),
            "the cramped timestamp must be dropped:\n{}",
            files_n.plain
        );

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
                &["Working 1/5", "Write rateLimit middleware", "rateLimit.ts"],
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
        // The identifier is camelCase everywhere, same as @files.
        for size in [(40u16, 12u16), (120u16, 40u16)] {
            let list = render_lock_scene("list", size.0, size.1).expect("list");
            assert!(
                list.plain.contains("rateLimit.ts") && !list.plain.contains("ratelimit.ts"),
                "list must use the camelCase identifier at {size:?}:\n{}",
                list.plain
            );
            let todos = render_lock_scene("todos", size.0, size.1).expect("todos");
            assert!(
                todos.plain.contains("Write rateLimit middleware"),
                "todos must use the camelCase identifier at {size:?}:\n{}",
                todos.plain
            );
        }
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
        assert!(settings.plain.contains("Cortex Mini 1"));
        assert!(!settings.plain.contains("gpt-"));
        let diff = render_lock_scene("multi_diff", 120, 40).expect("diff");
        assert!(diff.plain.contains("completions.ts"));
        assert!(diff.plain.contains("-2"));
    }
}
