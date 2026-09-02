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
    /// The rendered cells, for style assertions.
    pub buffer: ratatui::buffer::Buffer,
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
        // Auto-formatted replies: every one of these renders through the live
        // session view and the real `MarkdownRenderer` / diff renderer.
        "md_table",
        "md_fence",
        "md_list",
        "md_mixed",
        "diff_hunk",
        "diff_word",
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
                // The sign-in picker is the real login screen, not a board:
                // `login` focuses option 1 (Continue with browser),
                // `login_select` shows the selection moved to option 2
                // (Paste an API key).
                "login" => LoginScreen::lock_select(LOCK_SPLASH_VERSION, None).render(frame),
                "login_select" => {
                    LoginScreen::lock_select_option(LOCK_SPLASH_VERSION, None, 1).render(frame)
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
                "md_table" => draw_session(frame, reply_state(MD_TABLE_PROMPT, MD_TABLE)),
                "md_fence" => draw_session(frame, reply_state(MD_FENCE_PROMPT, MD_FENCE)),
                "md_list" => draw_session(frame, reply_state(MD_LIST_PROMPT, MD_LIST)),
                "md_mixed" => draw_session(frame, reply_state(MD_MIXED_PROMPT, MD_MIXED)),
                "diff_hunk" => draw_session(frame, edit_state(DIFF_HUNK, 5, 1)),
                "diff_word" => draw_session(frame, edit_state(DIFF_WORD, 1, 1)),
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
        buffer: terminal.backend().buffer().clone(),
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

// ---------------------------------------------------------------------------
// Auto-formatted reply fixtures
// ---------------------------------------------------------------------------

const MD_TABLE_PROMPT: &str = "Compare the three models for this project";
/// A reply with a real markdown table: header + 3 rows.
const MD_TABLE: &str = r#"Here is how the three models compare for the rate-limiter work:

| Model | Effort | Billing |
|---|---|---|
| Mini 1 | Medium | per request |
| Cortex 1 | High | per request |
| Max 1 | MAX | per token |

Mini 1 is the default; switch with /model when a change needs deeper reasoning."#;

const MD_FENCE_PROMPT: &str = "Show me the middleware you wrote";
/// A reply with a fenced TypeScript block (raw string: the fence keeps its
/// indentation).
const MD_FENCE: &str = r#"The limiter is a sliding window over a Redis sorted set:

```ts
export async function rateLimit(key: string, limit = 60) {
  const now = Date.now();
  await redis.zadd(key, now, String(now));
  await redis.zremrangebyscore(key, 0, now - 60_000);
  const count = await redis.zcard(key); // requests in the window
  return count <= limit;
}
```

It fails open when Redis is unreachable and logs a warning instead."#;

const MD_LIST_PROMPT: &str = "What is the plan?";
/// A reply with a nested bullet list and a task list in one turn.
const MD_LIST: &str = r#"Two parts, then the checklist:

- Redis client
  - one shared connection per process
  - fail open when Redis is unreachable
- Middleware
  - sliding window per API key
  - 429 with Retry-After

Checklist:

- [x] Add the Redis client singleton
- [x] Write the rateLimit middleware
- [ ] Wire it into POST /v1/completions
- [ ] Integration tests with ioredis-mock"#;

const MD_MIXED_PROMPT: &str = "Summarize what changed";
/// Heading + bullets + table + fence in one reply — the auto-format proof.
const MD_MIXED: &str = r#"## Rate limiting — what changed

- 60 req/min per API key, sliding window
- Fails open if Redis is unreachable, with a warning

| Route | Limit | Window |
|---|---|---|
| POST /v1/completions | 60 | 60s |
| POST /v1/embeddings | 120 | 60s |

```ts
export const limiter = rateLimit({ limit: 60, windowSec: 60 });
app.post("/v1/completions", { preHandler: [requireApiKey, limiter] });
```

Run `npm test -- rateLimit` to see the 429 path covered."#;

/// Edit result: a unified hunk with context, deletions and additions.
const DIFF_HUNK: &str = r#"@@ -20,6 +20,10 @@
 import Redis from "ioredis";
 import type { FastifyRequest } from "fastify";
-const limit = 30;
+const limit = 60;
+const windowSec = 60;
 
 export function rateLimit(opts: RateLimitOpts) {
-  const redis = new Redis();
+  const redis = new Redis(process.env.REDIS_URL);
+  const key = `rl:${opts.keyOf(req)}`;
+  await redis.zremrangebyscore(key, 0, now - windowSec * 1000);
   return async (req: FastifyRequest, reply: FastifyReply) => {"#;

/// Edit result: one changed line, so only the mutated token is coloured.
const DIFF_WORD: &str = r#"@@ -21,3 +21,3 @@
 import Redis from "ioredis";
-const limit = 30;
+const limit = 60;
 export function rateLimit(opts: RateLimitOpts) {"#;

/// A finished turn: the user's prompt on its bar, then `reply` rendered by
/// the real markdown renderer.
fn reply_state(prompt: &str, reply: &str) -> AppState {
    let mut state = lock_app();
    state.context_percent = 91;
    state.add_message(Message::user(prompt));
    state.add_message(Message::assistant(reply));
    state
}

/// A finished Edit tile whose result is a unified diff.
fn edit_state(diff: &str, adds: usize, dels: usize) -> AppState {
    use crate::views::tool_call::{ToolCallDisplay, ToolResultDisplay, ToolStatus};

    let mut state = lock_app();
    state.context_percent = 92;
    state.add_message(Message::user(
        "Raise the limit to 60 and read the Redis URL from the environment",
    ));
    let mut call = ToolCallDisplay::new(
        "edit_1".into(),
        "Edit".into(),
        serde_json::json!({"file_path": "src/middleware/rateLimit.ts"}),
        1,
    );
    call.set_status(ToolStatus::Completed);
    call.set_result(ToolResultDisplay {
        output: diff.to_string(),
        success: true,
        summary: format!("src/middleware/rateLimit.ts · +{adds} -{dels}"),
    });
    state.tool_calls.push(call);
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
    use cortex_core::style::{
        ACCENT, DIFF_ADD, ERROR, HAIRLINE, PANEL_BG, SELECTION_BG, SUCCESS, TEXT, TEXT_DIM,
        THINKING, USER_TURN_BG, WARNING,
    };
    use ratatui::buffer::Buffer;
    use ratatui::style::Color;

    const SIZES: [(u16, u16); 2] = [(40, 12), (120, 40)];

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

    /// Locked diff green `#4ADE80` as an SGR foreground.
    const GREEN_FG: &str = "38;2;74;222;128";
    /// Selection violet `#A78BFA` as an SGR foreground.
    const ACCENT_FG: &str = "38;2;167;139;250";

    fn row_text(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect()
    }

    fn cells(buf: &Buffer) -> impl Iterator<Item = (u16, u16, &ratatui::buffer::Cell)> + '_ {
        (0..buf.area.height)
            .flat_map(move |y| (0..buf.area.width).map(move |x| (x, y, &buf[(x, y)])))
    }

    fn is_hairline_row(buf: &Buffer, y: u16) -> bool {
        (0..buf.area.width).all(|x| {
            let cell = &buf[(x, y)];
            cell.symbol() == "─" && cell.style().fg == Some(HAIRLINE)
        })
    }

    /// Scenes that show the hairline-framed composer.
    const COMPOSER_SCENES: &[&str] = &[
        "splash",
        "typing",
        "working",
        "read",
        "shell",
        "streaming",
        "usage",
        "quota",
        "cloud",
        "sudo",
        "ask",
        "files",
        "queue",
        "first_run",
        "footer_max",
        "thinking",
        "todos",
        "btw",
        "stopped",
        "interrupt",
        "compacted",
        "compact",
        "write",
        "grep",
        "tool_tiles",
        "glob",
        "list",
        "fetch",
        "mcp_call",
        "task",
        "diagnostics",
        "edit",
        "palette",
        "palette_empty",
        "session_empty",
        "session_loading",
        "session_error",
        "session_success",
        "md_table",
        "md_fence",
        "md_list",
        "md_mixed",
        "diff_hunk",
        "diff_word",
    ];

    /// Scenes whose Edit tile carries a unified diff: red on `-` rows and
    /// green on `+` rows are the diff, not diagnostics.
    const DIFF_SCENES: &[&str] = &["diff_hunk", "diff_word"];

    /// Scenes whose reply carries a fenced code block with its `│` gutter.
    const FENCE_SCENES: &[&str] = &["md_fence", "md_mixed"];

    /// The marker of a diff row once its gutter (spaces and line numbers) is
    /// stripped: `+`, `-`, or none.
    fn diff_marker(row: &str) -> Option<char> {
        let rest = row
            .trim_start()
            .trim_start_matches(|c: char| c.is_ascii_digit() || c == ' ');
        match rest.chars().next() {
            Some(c @ ('+' | '-')) if rest.chars().nth(1) == Some(' ') => Some(c),
            _ => None,
        }
    }

    /// Scenes that begin with a past user turn (`> …` on the gray bar).
    const TURN_SCENES: &[&str] = &[
        "working",
        "read",
        "shell",
        "streaming",
        "queue",
        "thinking",
        "todos",
        "btw",
        "stopped",
        "write",
        "grep",
        "glob",
        "list",
        "fetch",
        "mcp_call",
        "task",
        "diagnostics",
        "edit",
        "model_compact",
        "mode",
        "permissions",
        "resume",
        "mcp",
        "usage",
        "quota",
        "sandbox",
        "cloud",
        "sudo",
        "jobs",
        "help",
        "config",
        "footer_max",
        "skills",
        "compacted",
        "clear_confirm",
        "multi_diff",
        "settings_hub",
        "session_loading",
        "session_error",
        "session_success",
    ];

    /// Scenes with a focused selection row.
    const SELECTION_SCENES: &[&str] = &[
        "palette",
        "model_compact",
        "model_full",
        "mode",
        "permissions",
        "permission",
        "plan",
        "resume",
        "sandbox",
        "files",
        "jobs",
        "config",
        "login",
        "login_select",
        "question",
        "skills",
        "clear_confirm",
        "clear",
        "delete",
        "multi_diff",
        "settings_hub",
    ];

    /// Composer scenes whose footer carries a picker hint (or none) instead
    /// of the `shift+tab` hint.
    const PICKER_COMPOSER_SCENES: &[&str] = &["palette", "palette_empty", "files"];

    const LOGIN_SCENES: &[&str] = &[
        "login",
        "login_select",
        "login_waiting",
        "login_success",
        "login_error",
    ];

    #[test]
    fn violet_is_reserved_for_the_focused_selection() {
        // The one accent: a violet cell is always on the dark gray selection
        // bar, and every scene with a selection paints its `>` caret violet.
        for id in lock_scene_ids() {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                for (x, y, cell) in cells(&frame.buffer) {
                    if cell.style().fg == Some(ACCENT) && cell.symbol() != " " {
                        assert_eq!(
                            cell.style().bg,
                            Some(SELECTION_BG),
                            "{id} paints violet {:?} off the selection bar at {size:?} ({x},{y}):\n{}",
                            cell.symbol(),
                            frame.plain
                        );
                    }
                }
                let violet = painted_chars(&frame.ansi, ACCENT_FG);
                if SELECTION_SCENES.contains(id) {
                    assert!(
                        violet.contains('>'),
                        "{id} must paint its selection caret violet at {size:?}: {violet:?}"
                    );
                } else {
                    assert!(
                        violet.trim().is_empty(),
                        "{id} has no focused selection yet paints violet at {size:?}: {violet:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn selection_rows_are_violet_on_the_gray_bar_never_inverted() {
        // Violet is never a background: no inverted bar, and never the old
        // `#221A38` full-row wash — the bar is the dark gray `#262626`.
        const ACCENT_BG: &str = "48;2;167;139;250";
        const OLD_VIOLET_WASH: &str = "48;2;34;26;56";
        for id in lock_scene_ids() {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                assert!(
                    !frame.ansi.contains(ACCENT_BG),
                    "{id} paints an inverted violet bar at {size:?}"
                );
                assert!(
                    !frame.ansi.contains(OLD_VIOLET_WASH),
                    "{id} brings back the #221A38 wash at {size:?}"
                );
            }
        }
        for id in SELECTION_SCENES {
            let frame = render_lock_scene(id, 120, 40).expect(id);
            let row = (0..40u16)
                .find(|y| {
                    frame.buffer[(0, *y)].symbol() == ">"
                        && frame.buffer[(0, *y)].style().fg == Some(ACCENT)
                })
                .unwrap_or_else(|| panic!("{id} has no violet `>` row:\n{}", frame.plain));
            // The whole row is the bar; the caret and the label are violet; the
            // description / meta on the bar stay dim.
            for x in 0..120u16 {
                assert_eq!(
                    frame.buffer[(x, row)].style().bg,
                    Some(SELECTION_BG),
                    "{id} selection bar must span the row (col {x}):\n{}",
                    row_text(&frame.buffer, row)
                );
            }
            let text = row_text(&frame.buffer, row);
            let label_x = text
                .chars()
                .enumerate()
                .skip(2)
                .find(|(_, c)| c.is_alphabetic() || *c == '/' || *c == '~')
                .map(|(i, _)| i as u16)
                .unwrap_or_else(|| panic!("{id} selected row has no label: {text}"));
            assert_eq!(
                frame.buffer[(label_x, row)].style().fg,
                Some(ACCENT),
                "{id} selected label must be violet: {text}"
            );
        }
    }

    #[test]
    fn green_is_reserved_for_checks_and_diff_additions() {
        // Green covers `✓` and `+N` — never a word, a dot or a bar.
        for id in lock_scene_ids() {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                for (x, y, cell) in cells(&frame.buffer) {
                    if cell.style().fg == Some(SUCCESS) || cell.style().fg == Some(DIFF_ADD) {
                        let symbol = cell.symbol();
                        // In a diff, the whole `+` row (or the inserted token)
                        // is the diff green.
                        let on_addition_row = DIFF_SCENES.contains(id)
                            && diff_marker(&row_text(&frame.buffer, y)) == Some('+');
                        assert!(
                            on_addition_row
                                || symbol == "✓"
                                || symbol == "+"
                                || symbol == " "
                                || symbol.chars().all(|c| c.is_ascii_digit()),
                            "{id} paints green {symbol:?} at {size:?} ({x},{y}):\n{}",
                            frame.plain
                        );
                    }
                }
            }
        }
        // Diff additions really are green.
        for id in ["footer_max", "write", "edit", "multi_diff", "queue"] {
            let frame = render_lock_scene(id, 120, 40).expect(id);
            let painted = painted_chars(&frame.ansi, GREEN_FG);
            assert!(
                painted.contains('+'),
                "{id} must paint its +diff green: {painted:?}"
            );
        }
        // Checks really are green.
        for id in [
            "shell",
            "task",
            "todos",
            "jobs",
            "mcp",
            "footer_max",
            "login_success",
            "sandbox",
        ] {
            let frame = render_lock_scene(id, 120, 40).expect(id);
            let painted = painted_chars(&frame.ansi, GREEN_FG);
            assert!(
                painted.contains('✓'),
                "{id} must paint its ✓ green: {painted:?}"
            );
        }
    }

    #[test]
    fn every_edit_plus_count_is_green() {
        // Rule from states 10 (Edit +9), 24 (queued Edit +58) and 30 (MAX
        // footer +214): the `+N` of an Edit / Write / commit is the diff
        // green at both sizes — never gray or violet. Scan every scene for
        // `+N` tokens on Edit, Write and Committed lines.
        for id in lock_scene_ids() {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let green = painted_chars(&frame.ansi, GREEN_FG);
                for line in frame.plain.lines() {
                    if !(line.contains("Edit ")
                        || line.contains("Write ")
                        || line.contains("files ·"))
                    {
                        continue;
                    }
                    let mut rest = line;
                    while let Some(at) = rest.find('+') {
                        let digits: String = rest[at + 1..]
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect();
                        if !digits.is_empty() {
                            let token = format!("+{digits}");
                            assert!(
                                green.contains(&token),
                                "{id} paints `{token}` without the diff green at {size:?}: {line:?}"
                            );
                        }
                        rest = &rest[at + 1..];
                    }
                }
            }
        }
        for size in SIZES {
            let queue = render_lock_scene("queue", size.0, size.1).expect("queue");
            assert!(
                painted_chars(&queue.ansi, GREEN_FG).contains("+58"),
                "queued Edit +58 must be green at {size:?}"
            );
            assert!(
                queue.plain.contains("rateLimit.ts +58 -0"),
                "queued Edit must read `Edit <path> +58 -0` at {size:?}:\n{}",
                queue.plain
            );
            let edit = render_lock_scene("edit", size.0, size.1).expect("edit");
            assert!(painted_chars(&edit.ansi, GREEN_FG).contains("+9"));
            let max = render_lock_scene("footer_max", size.0, size.1).expect("footer_max");
            assert!(painted_chars(&max.ansi, GREEN_FG).contains("+214"));
        }
    }

    #[test]
    fn red_amber_and_gold_stay_on_diagnostics_and_thinking() {
        for id in lock_scene_ids() {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let mut red = String::new();
                let mut amber = String::new();
                let mut gold = String::new();
                for (_, _, cell) in cells(&frame.buffer) {
                    match cell.style().fg {
                        Some(c) if c == ERROR => red.push_str(cell.symbol()),
                        Some(c) if c == WARNING => amber.push_str(cell.symbol()),
                        Some(c) if c == THINKING => gold.push_str(cell.symbol()),
                        _ => {}
                    }
                }
                let error_scene = matches!(*id, "diagnostics" | "session_error" | "login_error");
                if DIFF_SCENES.contains(id) {
                    // Red is the diff's `-` rows (or the removed token), never
                    // a context or `+` row.
                    for (_, y, cell) in cells(&frame.buffer) {
                        if cell.style().fg == Some(ERROR) && cell.symbol() != " " {
                            assert_eq!(
                                diff_marker(&row_text(&frame.buffer, y)),
                                Some('-'),
                                "{id} paints red off a deletion row at {size:?}:\n{}",
                                row_text(&frame.buffer, y)
                            );
                        }
                    }
                } else {
                    assert!(
                        error_scene || red.trim().is_empty(),
                        "{id} paints red outside diagnostics at {size:?}: {red:?}"
                    );
                }
                assert!(
                    *id == "diagnostics" || amber.trim().is_empty(),
                    "{id} paints amber outside diagnostics at {size:?}: {amber:?}"
                );
                assert!(
                    *id == "thinking" || gold.trim().is_empty(),
                    "{id} paints the Thinking gold at {size:?}: {gold:?}"
                );
                if *id == "thinking" {
                    assert_eq!(gold.trim(), "Thinking", "{size:?}");
                }
            }
        }
    }

    #[test]
    fn banned_colors_never_painted() {
        // The interim violet highlight is gone with the mint one: no scene
        // paints violet, the old `#221A38` violet wash, the mint pair, the old
        // brand green, or the navy wash — the host terminal owns the
        // background and the violet lives on the focused selection alone.
        const BANNED: [&str; 6] = [
            "125;211;252",
            "34;26;56",
            "0;245;212",
            "26;51;48",
            "0;255;163",
            "10;22;40",
        ];
        for id in lock_scene_ids() {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                for banned in BANNED {
                    assert!(
                        !frame.ansi.contains(banned),
                        "{id} paints banned color {banned} at {size:?}"
                    );
                }
                // Nothing paints its own background wash: bars are the
                // documented grays, never a colour.
                for (x, y, cell) in cells(&frame.buffer) {
                    if let Some(Color::Rgb(r, g, b)) = cell.style().bg {
                        assert!(
                            r == g && g == b,
                            "{id} paints a tinted background {r},{g},{b} at {size:?} ({x},{y})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn composer_is_framed_by_hairlines_in_every_session_state() {
        for id in COMPOSER_SCENES {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let buf = &frame.buffer;
                let found = (1..buf.area.height.saturating_sub(1)).find(|&y| {
                    buf[(0, y)].symbol() == ">"
                        && buf[(0, y)].style().bg != Some(USER_TURN_BG)
                        && buf[(0, y)].style().bg != Some(SELECTION_BG)
                        && is_hairline_row(buf, y - 1)
                        && is_hairline_row(buf, y + 1)
                });
                let Some(y) = found else {
                    panic!(
                        "{id} has no hairline-framed `> ` composer at {size:?}:\n{}",
                        frame.plain
                    );
                };
                // The `>` and the block cursor are white; a placeholder is dim.
                assert_eq!(buf[(0, y)].style().fg, Some(TEXT), "{id} at {size:?}");
                let row = row_text(buf, y);
                assert!(
                    row.contains('█'),
                    "{id} composer needs its block cursor: {row}"
                );
                assert!(
                    !row.contains("▐"),
                    "{id} must not overflow into a scrollbar at {size:?}:\n{}",
                    frame.plain
                );
            }
        }
        // Idle states carry the idle placeholder, dim; live ones invite a
        // follow-up.
        for size in SIZES {
            let splash = render_lock_scene("splash", size.0, size.1).expect("splash");
            assert!(
                splash.plain.contains("> Plan, search, build anything █"),
                "{}",
                splash.plain
            );
            let ghost_x = splash
                .plain
                .lines()
                .find_map(|l| l.find("Plan, search"))
                .expect("ghost") as u16;
            let ghost_y = (0..size.1)
                .find(|y| row_text(&splash.buffer, *y).contains("Plan, search"))
                .expect("ghost row");
            assert_eq!(
                splash.buffer[(ghost_x, ghost_y)].style().fg,
                Some(TEXT_DIM),
                "placeholder must be dim"
            );
            let working = render_lock_scene("working", size.0, size.1).expect("working");
            assert!(
                working.plain.contains("> Add a follow-up ↵ to queue █"),
                "{}",
                working.plain
            );
        }
    }

    #[test]
    fn past_user_turns_sit_on_the_gray_bar() {
        for id in TURN_SCENES {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let buf = &frame.buffer;
                let y = (0..buf.area.height)
                    .find(|y| {
                        buf[(0, *y)].symbol() == ">"
                            && buf[(0, *y)].style().bg == Some(USER_TURN_BG)
                    })
                    .unwrap_or_else(|| {
                        panic!(
                            "{id} has no past user turn on the gray bar at {size:?}:\n{}",
                            frame.plain
                        )
                    });
                for x in 0..buf.area.width {
                    assert_eq!(
                        buf[(x, y)].style().bg,
                        Some(USER_TURN_BG),
                        "{id} user-turn bar must span the row at {size:?} (col {x})"
                    );
                }
                // White copy on the bar — never the accent.
                assert_eq!(buf[(0, y)].style().fg, Some(TEXT));
                assert_eq!(buf[(2, y)].style().fg, Some(TEXT));
            }
        }
    }

    #[test]
    fn past_turn_and_composer_carets_are_white_never_violet() {
        // Violet belongs to the focused selection's caret alone. The `>` of a
        // past user turn (on the gray bar) and the `>` of the composer (between
        // the hairlines) are white in every scene at both sizes — never
        // `#A78BFA`, never dim.
        for id in lock_scene_ids() {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let buf = &frame.buffer;
                for y in 0..buf.area.height {
                    let cell = &buf[(0, y)];
                    if cell.symbol() != ">" {
                        continue;
                    }
                    let bg = cell.style().bg;
                    if bg == Some(SELECTION_BG) {
                        // A selection caret: violet, checked elsewhere.
                        continue;
                    }
                    assert_eq!(
                        cell.style().fg,
                        Some(TEXT),
                        "{id} row {y} caret must be white at {size:?} (bg {bg:?}):\n{}",
                        row_text(buf, y)
                    );
                }
            }
        }
        // The scene QA called out — a historical turn on its bar plus a typed
        // composer — has no focused selection, so it paints no violet at all.
        for id in ["footer_max", "typing"] {
            let frame = render_lock_scene(id, 120, 40).expect(id);
            assert!(
                painted_chars(&frame.ansi, ACCENT_FG).trim().is_empty(),
                "{id} paints violet: {:?}",
                painted_chars(&frame.ansi, ACCENT_FG)
            );
        }
    }

    #[test]
    fn completed_turns_do_not_get_a_fake_check() {
        // A finished assistant turn is gray/white copy — green stays on real
        // `✓` tool success and `+diff`. No scene invents a `✓` on a reply.
        for size in SIZES {
            let frame = render_lock_scene("session_success", size.0, size.1).expect("success");
            // The reply may wrap at 40 columns; it is plain copy either way.
            assert!(
                frame.plain.contains("Signed in.") && frame.plain.contains("consistent."),
                "{}",
                frame.plain
            );
            assert!(
                !frame.plain.contains('✓'),
                "no fake check on a completed turn at {size:?}:\n{}",
                frame.plain
            );
            assert!(
                painted_chars(&frame.ansi, GREEN_FG).trim().is_empty(),
                "session_success paints green at {size:?}"
            );
            let stream = render_lock_scene("streaming", size.0, size.1).expect("streaming");
            assert!(
                !stream.plain.contains('✓'),
                "no fake check on the streamed reply at {size:?}:\n{}",
                stream.plain
            );
        }
    }

    // -----------------------------------------------------------------------
    // Auto-formatted replies
    // -----------------------------------------------------------------------

    const BOX_GLYPHS: [char; 9] = ['┌', '┐', '└', '┘', '┼', '┬', '┴', '├', '┤'];

    /// The table rows of a frame: every row that starts on the grid (`+`
    /// rule or `|` cell row), trailing padding removed.
    fn table_rows(plain: &str) -> Vec<String> {
        plain
            .lines()
            .map(|line| line.trim_end().to_string())
            .filter(|line| line.starts_with(['+', '|']))
            .collect()
    }

    #[test]
    fn md_table_is_a_gray_plus_ascii_grid() {
        for size in SIZES {
            let frame = render_lock_scene("md_table", size.0, size.1).expect("md_table");
            let buf = &frame.buffer;
            // `+---+` rules, `|` separators — and never Unicode box drawing.
            let rule = (0..buf.area.height)
                .find(|y| row_text(buf, *y).trim_start().starts_with("+-"))
                .unwrap_or_else(|| panic!("no plus rule at {size:?}:\n{}", frame.plain));
            let rule_text = row_text(buf, rule);
            assert!(rule_text.contains("-+-"), "junctions are `+`: {rule_text}");
            assert!(
                frame.plain.contains("| Cortex 1 | High   | per request |"),
                "{}",
                frame.plain
            );
            for glyph in BOX_GLYPHS {
                assert!(
                    !frame.plain.contains(glyph),
                    "md_table draws {glyph} at {size:?}:\n{}",
                    frame.plain
                );
            }
            // Every visible table row is framed (`+…+` or `|…|`) and carries
            // nothing from the Unicode box-drawing block — so the frameless
            // `Header | Header` / `---+---` layout can never come back.
            let visible = table_rows(&frame.plain);
            assert!(
                !visible.is_empty(),
                "no table rows at {size:?}:\n{}",
                frame.plain
            );
            for row in &visible {
                let first = row.chars().next().unwrap();
                let last = row.chars().last().unwrap();
                assert!(
                    first == last,
                    "unframed table row {row:?} at {size:?}:\n{}",
                    frame.plain
                );
                assert!(
                    !row.chars().any(|c| ('\u{2500}'..='\u{257F}').contains(&c)),
                    "box drawing in table row {row:?} at {size:?}"
                );
            }
            // The scene is a real assistant `Message` rendered by the
            // session view: its rows are exactly the `MarkdownRenderer`'s
            // rows for `MD_TABLE` at the view's content width (a contiguous
            // window of them where the 40×12 frame scrolls the top away).
            let state = reply_state(MD_TABLE_PROMPT, MD_TABLE);
            let renderer = cortex_core::markdown::MarkdownRenderer::cortex(
                state.markdown_theme.clone(),
                size.0 - 4,
            );
            let expected: Vec<String> = renderer
                .render(MD_TABLE)
                .iter()
                .map(|line| line.to_string())
                .filter(|line| line.starts_with(['+', '|']))
                .collect();
            assert_eq!(expected.len(), 7, "{expected:?}");
            assert!(
                expected
                    .windows(visible.len())
                    .any(|window| window == visible.as_slice()),
                "the scene's table is not the renderer's output at {size:?}:\n{visible:#?}\nvs\n{expected:#?}"
            );
            if size.0 == 120 {
                assert_eq!(visible, expected, "the whole grid is visible at 120×40");
                assert_eq!(
                    visible.iter().filter(|row| row.starts_with('+')).count(),
                    3,
                    "top rule, header rule, bottom rule"
                );
            }
            // Borders are the hairline gray, cells white, header bold white.
            let x = rule_text.find('+').expect("corner") as u16;
            assert_eq!(buf[(x, rule)].style().fg, Some(HAIRLINE), "{size:?}");
            let row = (0..buf.area.height)
                .find(|y| row_text(buf, *y).contains("| Cortex 1"))
                .expect("data row");
            let text_x = row_text(buf, row).find('C').expect("cell") as u16;
            assert_eq!(buf[(text_x, row)].style().fg, Some(TEXT));
            let bar_x = row_text(buf, row).find('|').expect("separator") as u16;
            assert_eq!(buf[(bar_x, row)].style().fg, Some(HAIRLINE));
            // The table paints no background of its own (the user turn above
            // it keeps its bar).
            for x in 0..buf.area.width {
                assert!(
                    matches!(buf[(x, row)].style().bg, None | Some(Color::Reset)),
                    "the table paints a background at {size:?} col {x}"
                );
            }
        }
        let wide = render_lock_scene("md_table", 120, 40).expect("md_table");
        let header = (0..40u16)
            .find(|y| row_text(&wide.buffer, *y).contains("| Model"))
            .expect("header row");
        let m = row_text(&wide.buffer, header).find('M').unwrap() as u16;
        assert!(
            wide.buffer[(m, header)]
                .style()
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD),
            "header is bold"
        );
        // Header + 3 rows.
        let rows = wide
            .plain
            .lines()
            .filter(|l| l.trim_start().starts_with("| "))
            .count();
        assert_eq!(rows, 4, "{}", wide.plain);
    }

    #[test]
    fn md_fence_has_lang_tag_line_numbers_and_hairlines() {
        for size in SIZES {
            let frame = render_lock_scene("md_fence", size.0, size.1).expect("md_fence");
            let buf = &frame.buffer;
            // A dim line-number gutter on every code row, the closing
            // hairline under the last one.
            let last = (0..buf.area.height)
                .find(|y| row_text(buf, *y).starts_with("7 │ }"))
                .unwrap_or_else(|| panic!("no numbered code at {size:?}:\n{}", frame.plain));
            assert_eq!(buf[(0, last)].style().fg, Some(HAIRLINE), "gutter is gray");
            assert!(
                row_text(buf, last + 1).trim_end().chars().all(|c| c == '─'),
                "closing hairline at {size:?}:\n{}",
                frame.plain
            );
            assert_eq!(buf[(0, last + 1)].style().fg, Some(HAIRLINE));
            // No box: the fence has no side borders or corners.
            for glyph in BOX_GLYPHS {
                assert!(!frame.plain.contains(glyph), "{size:?}:\n{}", frame.plain);
            }
        }
        let wide = render_lock_scene("md_fence", 120, 40).expect("md_fence");
        let buf = &wide.buffer;
        // The opening hairline carries the language tag.
        let top = (0..40u16)
            .find(|y| row_text(buf, *y).starts_with("─ ts ─"))
            .unwrap_or_else(|| panic!("no tagged hairline:\n{}", wide.plain));
        assert_eq!(buf[(0, top)].style().fg, Some(HAIRLINE));
        assert_eq!(buf[(2, top)].style().fg, Some(TEXT_DIM), "lang tag is dim");
        // Numbered, indented code with bold keywords; indentation survives.
        assert!(wide.plain.contains("1 │ export async function rateLimit"));
        assert!(wide.plain.contains("2 │   const now = Date.now();"));
        let code = top + 1;
        let e = row_text(buf, code).find("export").unwrap() as u16;
        assert!(
            buf[(e, code)]
                .style()
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD),
            "keywords are bold"
        );
        assert_eq!(buf[(e, code)].style().fg, Some(TEXT));
        // The fence introduces no colour of its own.
        for y in top..=top + 8 {
            for x in 0..120u16 {
                if let Some(Color::Rgb(r, g, b)) = buf[(x, y)].style().fg {
                    let spread = r.max(g).max(b) - r.min(g).min(b);
                    assert!(spread <= 25, "fence paints a colour at ({x},{y})");
                }
            }
        }
    }

    #[test]
    fn md_list_nests_bullets_and_checks_tasks() {
        let wide = render_lock_scene("md_list", 120, 40).expect("md_list");
        for needle in [
            "• Redis client",
            "  ◦ one shared connection per process",
            "• Middleware",
            "  ◦ sliding window per API key",
            "✓ Add the Redis client singleton",
            "○ Wire it into POST /v1/completions",
        ] {
            assert!(
                wide.plain.contains(needle),
                "missing {needle}:\n{}",
                wide.plain
            );
        }
        assert!(!wide.plain.contains("[x]") && !wide.plain.contains("[ ]"));
        // `✓` is the only green; bullets and `○` are dim.
        let green = painted_chars(&wide.ansi, GREEN_FG);
        assert_eq!(green.trim(), "✓ ✓", "{green:?}");
        let buf = &wide.buffer;
        let bullet = (0..40u16)
            .find(|y| row_text(buf, *y).starts_with("• Redis client"))
            .expect("bullet");
        assert_eq!(buf[(0, bullet)].style().fg, Some(TEXT_DIM));
        assert_eq!(buf[(2, bullet)].style().fg, Some(TEXT));
        let narrow = render_lock_scene("md_list", 40, 12).expect("md_list narrow");
        assert!(
            narrow.plain.contains("✓ Add the Redis client singleton"),
            "{}",
            narrow.plain
        );
        assert!(narrow.plain.contains("○ Wire it into"), "{}", narrow.plain);
    }

    #[test]
    fn md_mixed_is_the_auto_format_proof() {
        let wide = render_lock_scene("md_mixed", 120, 40).expect("md_mixed");
        let buf = &wide.buffer;
        // Heading (bold white, no `##`), bullets, a plus-ASCII table, a
        // tagged fence and a code chip — in one reply.
        let heading = (0..40u16)
            .find(|y| row_text(buf, *y).starts_with("Rate limiting — what changed"))
            .unwrap_or_else(|| panic!("no heading:\n{}", wide.plain));
        assert!(
            buf[(0, heading)]
                .style()
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD)
        );
        assert!(!wide.plain.contains("##"));
        for needle in [
            "• 60 req/min per API key, sliding window",
            "+----------------------+-------+--------+",
            "| POST /v1/completions | 60    | 60s    |",
            "─ ts ─",
            "1 │ export const limiter = rateLimit(",
            "Run npm test -- rateLimit",
        ] {
            assert!(
                wide.plain.contains(needle),
                "missing {needle}:\n{}",
                wide.plain
            );
        }
        for glyph in BOX_GLYPHS {
            assert!(!wide.plain.contains(glyph), "{}", wide.plain);
        }
        let narrow = render_lock_scene("md_mixed", 40, 12).expect("md_mixed narrow");
        assert!(narrow.plain.contains("─ ts ─"), "{}", narrow.plain);
        assert!(
            narrow.plain.contains("1 │ export const"),
            "{}",
            narrow.plain
        );
    }

    #[test]
    fn diff_hunk_has_gutter_context_deletions_and_additions() {
        for size in SIZES {
            let frame = render_lock_scene("diff_hunk", size.0, size.1).expect("diff_hunk");
            for needle in [
                "● Edit src/middleware/rateLimit.ts +5 -2",
                "@@ -20,6 +20,10 @@",
                "20 20   import Redis",
                "22    - const limit = 30;",
                "   22 + const limit = 60;",
                "   23 + const windowSec = 60;",
            ] {
                assert!(
                    frame.plain.contains(needle),
                    "diff_hunk missing `{needle}` at {size:?}:\n{}",
                    frame.plain
                );
            }
            let buf = &frame.buffer;
            let minus = (0..buf.area.height)
                .find(|y| row_text(buf, *y).contains("- const limit = 30;"))
                .expect("deletion row");
            let plus = (0..buf.area.height)
                .find(|y| row_text(buf, *y).contains("+ const windowSec"))
                .expect("addition row");
            let ctx = (0..buf.area.height)
                .find(|y| row_text(buf, *y).contains("20 20   import"))
                .expect("context row");
            // Gutter numbers dim; context white; a whole added line green; a
            // deleted line's marker red.
            assert_eq!(buf[(0, ctx)].style().fg, Some(TEXT_DIM));
            let imp = row_text(buf, ctx).find("import").unwrap() as u16;
            assert_eq!(buf[(imp, ctx)].style().fg, Some(TEXT));
            let m = row_text(buf, minus).find('-').unwrap() as u16;
            assert_eq!(buf[(m, minus)].style().fg, Some(ERROR));
            let p = row_text(buf, plus).find('+').unwrap() as u16;
            assert_eq!(buf[(p, plus)].style().fg, Some(DIFF_ADD));
            let w = row_text(buf, plus).find("windowSec").unwrap() as u16;
            assert_eq!(buf[(w, plus)].style().fg, Some(DIFF_ADD));
        }
        // The stat on the tile: `+5` green, `-2` dim.
        let wide = render_lock_scene("diff_hunk", 120, 40).expect("diff_hunk");
        assert!(painted_chars(&wide.ansi, GREEN_FG).contains("+5"));
        assert!(
            wide.plain.contains("26 29     return async"),
            "{}",
            wide.plain
        );
    }

    #[test]
    fn diff_word_tints_only_the_mutated_token() {
        for size in SIZES {
            let frame = render_lock_scene("diff_word", size.0, size.1).expect("diff_word");
            let buf = &frame.buffer;
            let minus = (0..buf.area.height)
                .find(|y| row_text(buf, *y).contains("- const limit = 30;"))
                .unwrap_or_else(|| panic!("no deletion at {size:?}:\n{}", frame.plain));
            let plus = (0..buf.area.height)
                .find(|y| row_text(buf, *y).contains("+ const limit = 60;"))
                .expect("addition");
            let col = |y: u16, token: &str| row_text(buf, y).find(token).unwrap() as u16;
            // `const` and `limit` stay dim on both rows; only `30;` is red and
            // only `60;` is green.
            assert_eq!(buf[(col(minus, "const"), minus)].style().fg, Some(TEXT_DIM));
            assert_eq!(buf[(col(minus, "limit"), minus)].style().fg, Some(TEXT_DIM));
            assert_eq!(buf[(col(minus, "30;"), minus)].style().fg, Some(ERROR));
            assert_eq!(buf[(col(plus, "const"), plus)].style().fg, Some(TEXT_DIM));
            assert_eq!(buf[(col(plus, "60;"), plus)].style().fg, Some(DIFF_ADD));
            // Markers carry the colour; nothing carries a tinted background.
            assert_eq!(buf[(col(minus, "-"), minus)].style().fg, Some(ERROR));
            assert_eq!(buf[(col(plus, "+"), plus)].style().fg, Some(DIFF_ADD));
            assert_eq!(
                painted_chars(&frame.ansi, "38;2;255;107;107").trim(),
                "- 30;",
                "red is the marker and the removed token only at {size:?}"
            );
        }
    }

    #[test]
    fn mcp_call_issue_list_is_a_plus_ascii_table() {
        for size in SIZES {
            let frame = render_lock_scene("mcp_call", size.0, size.1).expect("mcp_call");
            assert!(
                frame.plain.contains("+---------+"),
                "mcp_call table rule at {size:?}:\n{}",
                frame.plain
            );
            assert!(frame.plain.contains("| Issue   |"), "{}", frame.plain);
            assert!(frame.plain.contains("| API-184 |"), "{}", frame.plain);
            for glyph in BOX_GLYPHS {
                assert!(!frame.plain.contains(glyph), "{}", frame.plain);
            }
        }
        let wide = render_lock_scene("mcp_call", 120, 40).expect("mcp_call");
        assert!(
            wide.plain.contains("| In Progress | you      |"),
            "{}",
            wide.plain
        );
        assert!(
            wide.plain.contains("| API-172 | Retry-After on 429"),
            "{}",
            wide.plain
        );
    }

    #[test]
    fn footer_is_model_left_hint_right_and_gray() {
        for id in lock_scene_ids() {
            if LOGIN_SCENES.contains(id) {
                continue;
            }
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let buf = &frame.buffer;
                let y = buf.area.height - 1;
                let footer = row_text(buf, y);
                assert!(
                    footer.starts_with("Cortex Mini 1"),
                    "{id} footer must lead with the model at {size:?}: {footer:?}"
                );
                for x in 0..buf.area.width {
                    let cell = &buf[(x, y)];
                    if cell.symbol() == " " {
                        continue;
                    }
                    let fg = cell.style().fg;
                    // The MAX badge is the one bold white token; everything
                    // else on the footer is the dim gray.
                    assert!(
                        fg == Some(TEXT_DIM) || fg == Some(TEXT),
                        "{id} footer cell {x} is not gray at {size:?}: {footer:?}"
                    );
                    assert_ne!(fg, Some(ACCENT), "{id} footer paints the accent");
                    assert!(
                        matches!(cell.style().bg, None | Some(Color::Reset)),
                        "{id} footer must not paint a bar at {size:?}: {footer:?}"
                    );
                }
            }
        }
        // Session states end the wide footer with the shortcut hint.
        for id in COMPOSER_SCENES {
            if PICKER_COMPOSER_SCENES.contains(id) {
                continue;
            }
            let frame = render_lock_scene(id, 120, 40).expect(id);
            let footer = row_text(&frame.buffer, 39);
            assert!(
                footer.trim_end().ends_with("shift+tab to cycle modes"),
                "{id} wide footer must end with the shortcut hint: {footer:?}"
            );
        }
        // The palette's footer carries the palette hints instead.
        let palette = render_lock_scene("palette", 120, 40).expect("palette");
        let footer = row_text(&palette.buffer, 39);
        assert!(
            footer.contains("↵ run") && footer.trim_end().ends_with("esc close"),
            "{footer:?}"
        );
        // The MAX badge keeps the model beside it, even at 40 columns.
        for id in ["footer_max", "config"] {
            let frame = render_lock_scene(id, 40, 12).expect(id);
            let footer = frame.plain.lines().last().unwrap_or_default();
            assert!(
                footer.starts_with("Cortex Mini 1 · MAX"),
                "{id} footer must read Cortex Mini 1 · MAX: {footer:?}"
            );
        }
        // The sign-in screens carry the version in the footer instead.
        for id in ["login", "login_select"] {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let footer = frame.plain.lines().last().unwrap_or_default();
                assert!(
                    footer.starts_with("Cortex CLI v1.0.0"),
                    "{id} footer at {size:?}: {footer:?}"
                );
            }
        }
    }

    #[test]
    fn pickers_are_numbered_with_dim_descriptions() {
        let expectations: &[(&str, &[&str], &str)] = &[
            (
                "mode",
                &[
                    "> 1 Agent",
                    "    edits files and runs commands",
                    "· 2 Plan",
                    "    draft an approach first — no edits",
                    "· 3 Ask",
                    "    read-only answers on the codebase",
                ],
                "↑↓ select · ↵ confirm · esc close",
            ),
            (
                "permissions",
                &[
                    "· 1 Read-only",
                    "    never edit files or run commands",
                    "> 2 Smart",
                    "· 3 Full access",
                    "    only ask when leaving the sandbox",
                ],
                "↑↓ select · ↵ confirm · esc close",
            ),
            (
                "login",
                &[
                    "Welcome to Cortex CLI!",
                    "How would you like to log in?",
                    "> 1 Continue with browser",
                    "    Opens cortex.foundation/cli/auth",
                    "· 2 Paste an API key",
                    "    Enter your key to authenticate",
                ],
                "↑↓ select · ↵ confirm · esc quit",
            ),
            (
                "clear_confirm",
                &["Start a new thread?", "> 1 Clear thread", "· 2 Cancel"],
                "↑↓ select · ↵ confirm · esc cancel",
            ),
            (
                "delete",
                &["> 1 Delete", "· 2 Keep"],
                "↑↓ select · ↵ confirm · esc keep",
            ),
            (
                "question",
                &[
                    "· 1 Middleware on POST",
                    "> 2 Shared limiter for every",
                    "· 3 Per-model limits",
                ],
                "1-9 pick · ↑↓ move · ↵ confirm · esc",
            ),
            (
                "permission",
                &["> 1 Yes, run once", "· 3 Edit command"],
                "↑↓ select · ↵ confirm · e edit command",
            ),
        ];
        for (id, needles, hints) in expectations {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                for needle in *needles {
                    assert!(
                        frame.plain.contains(needle),
                        "{id} missing `{needle}` at {size:?}:\n{}",
                        frame.plain
                    );
                }
                assert!(
                    frame.plain.contains(hints),
                    "{id} missing hints `{hints}` at {size:?}:\n{}",
                    frame.plain
                );
                // Numbered options replaced the `●` / `○` radios.
                assert!(
                    !frame.plain.contains("○"),
                    "{id} keeps radios at {size:?}:\n{}",
                    frame.plain
                );
            }
            // Descriptions under a title are dim, on the bar for the focused
            // option.
            let frame = render_lock_scene(id, 120, 40).expect(id);
            let buf = &frame.buffer;
            let selected = (0..40u16)
                .find(|y| buf[(0, *y)].symbol() == ">" && buf[(0, *y)].style().fg == Some(ACCENT))
                .expect("selected row");
            let number_x = 2u16;
            assert_eq!(
                buf[(number_x, selected)].style().fg,
                Some(TEXT),
                "{id}: the number stays white"
            );
            let below = row_text(buf, selected + 1);
            if below.starts_with("    ") && !below.trim().is_empty() {
                let desc_x = below.len() - below.trim_start().len();
                assert_eq!(
                    buf[(desc_x as u16, selected + 1)].style().fg,
                    Some(TEXT_DIM),
                    "{id}: description under the focused title must be dim"
                );
                assert_eq!(
                    buf[(desc_x as u16, selected + 1)].style().bg,
                    Some(SELECTION_BG),
                    "{id}: the bar covers the description row"
                );
            }
        }
    }

    #[test]
    fn search_fields_are_framed_by_hairlines_without_a_pricing_bar() {
        for id in ["model_compact", "model_full", "resume", "skills"] {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let buf = &frame.buffer;
                let field = (1..buf.area.height.saturating_sub(1)).find(|&y| {
                    row_text(buf, y).starts_with("/ ")
                        && is_hairline_row(buf, y - 1)
                        && is_hairline_row(buf, y + 1)
                });
                assert!(
                    field.is_some(),
                    "{id} must frame its search field with two hairlines at {size:?}:\n{}",
                    frame.plain
                );
                let y = field.unwrap();
                assert_eq!(buf[(0, y)].style().fg, Some(TEXT_DIM));
                assert!(
                    row_text(buf, y).contains("Type to search"),
                    "{id} search placeholder at {size:?}: {}",
                    row_text(buf, y)
                );
                // No rainbow pricing bar: no gradient glyphs, and every
                // non-gray foreground on screen is one of the locked accents.
                assert!(!frame.plain.contains('$'), "{id} shows pricing at {size:?}");
                for (_, _, cell) in cells(buf) {
                    if let Some(Color::Rgb(r, g, b)) = cell.style().fg {
                        let gray = r == g && g == b;
                        let locked = matches!(cell.style().fg, Some(c) if c == ACCENT || c == SUCCESS || c == TEXT_DIM || c == TEXT);
                        assert!(
                            gray || locked,
                            "{id} paints {r},{g},{b} at {size:?}: only grays + the locked accents"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn first_run_tips_sit_on_the_charcoal_panel() {
        for size in SIZES {
            let frame = render_lock_scene("first_run", size.0, size.1).expect("first_run");
            let buf = &frame.buffer;
            let tips_y = (0..buf.area.height)
                .find(|y| row_text(buf, *y).contains("A few tips"))
                .unwrap_or_else(|| panic!("no tips at {size:?}:\n{}", frame.plain));
            for x in 0..buf.area.width {
                assert_eq!(
                    buf[(x, tips_y)].style().bg,
                    Some(PANEL_BG),
                    "the tips panel spans the width at {size:?} (col {x})"
                );
            }
            for needle in ["/model", "@", "shift+tab", "Cortex CLI", "Cortex Pro"] {
                assert!(
                    frame.plain.contains(needle),
                    "first_run missing `{needle}` at {size:?}:\n{}",
                    frame.plain
                );
            }
            // The panel is the only filled block besides the composer bars.
            let panel_rows = (0..buf.area.height)
                .filter(|y| buf[(0, *y)].style().bg == Some(PANEL_BG))
                .count();
            assert!(
                panel_rows >= 4,
                "panel too small at {size:?}:\n{}",
                frame.plain
            );
        }
        let wide = render_lock_scene("first_run", 120, 40).expect("first_run");
        assert!(
            wide.plain.contains("· · · · ·  Cortex CLI"),
            "{}",
            wide.plain
        );
        assert!(wide.plain.contains("v1.0.0"), "{}", wide.plain);
    }

    #[test]
    fn model_slugs_never_shown() {
        // Users see English product names — `Cortex Mini 1`, never the
        // served `cortex-1-mini` / `cortex-1-max` / `cortex-1` slugs.
        for id in lock_scene_ids() {
            for size in SIZES {
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
            for size in SIZES {
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
            mcp.plain
                .contains("| API-184 | Rate limit 429 body  | In Progress | you      |"),
            "{}",
            mcp.plain
        );
    }

    #[test]
    fn live_states_keep_chrome_complete() {
        // Empty is allowed, but the chrome is whole: composer and model
        // footer at both sizes, the version header wherever the transcript
        // has not scrolled it away (an error at 40×12 legitimately does).
        for id in [
            "session_empty",
            "session_loading",
            "session_error",
            "palette_empty",
        ] {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                for needle in ["> ", "Cortex Mini 1"] {
                    assert!(
                        frame.plain.contains(needle),
                        "{id} missing `{needle}` at {size:?}:\n{}",
                        frame.plain
                    );
                }
                let scrolls = size == (40, 12) && id == "session_error";
                if !scrolls {
                    assert!(
                        frame.plain.contains("Cortex CLI v1.0.0"),
                        "{id} missing the version at {size:?}:\n{}",
                        frame.plain
                    );
                    assert!(
                        !frame.plain.contains('▐'),
                        "{id} must not overflow into a scrollbar at {size:?}:\n{}",
                        frame.plain
                    );
                }
            }
        }
        for size in SIZES {
            let empty = render_lock_scene("session_empty", size.0, size.1).expect("empty");
            assert!(empty.plain.contains("/ commands"), "{}", empty.plain);
            assert!(empty.plain.contains("~/cortex-api"), "{}", empty.plain);
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
                settings.plain.contains("Cortex Mini 1"),
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
    fn model_full_is_the_full_picker_at_every_size() {
        // State 05 is never the compact list: a description under each model
        // plus the Effort radios, at 40×12 as well as 120×40.
        for size in SIZES {
            let full = render_lock_scene("model_full", size.0, size.1).expect("model_full");
            let compact = render_lock_scene("model_compact", size.0, size.1).expect("compact");
            assert_ne!(
                full.plain, compact.plain,
                "model_full must differ from model_compact at {size:?}"
            );
            for needle in [
                "Cortex Mini 1",
                "Fast default for everyday coding.",
                "Cortex 1",
                "Deeper reasoning for hard changes.",
                "Cortex Max 1",
                "Longest context",
                "Effort",
                "○ Low   ● Medium   ○ High",
                "Type to search models",
            ] {
                assert!(
                    full.plain.contains(needle),
                    "model_full missing `{needle}` at {size:?}:\n{}",
                    full.plain
                );
            }
            for needle in ["Fast default", "Effort"] {
                assert!(
                    !compact.plain.contains(needle),
                    "model_compact stays the short list at {size:?}:\n{}",
                    compact.plain
                );
            }
            assert!(
                compact.plain.contains("> /model") && compact.plain.contains("Model"),
                "{}",
                compact.plain
            );
        }
        let wide = render_lock_scene("model_full", 120, 40).expect("model_full");
        assert!(wide.plain.contains("> /model"), "{}", wide.plain);
        assert!(
            wide.plain.contains("cortex.foundation/billing"),
            "{}",
            wide.plain
        );
    }

    #[test]
    fn distinct_states_render_distinct_frames() {
        // Only the documented aliases may capture the same board; every other
        // state is its own frame at both sizes (`login` and `login_select`
        // differ by the focused option).
        let aliases: [&[&str]; 4] = [
            &["clear", "clear_confirm"],
            &["compact", "compacted"],
            &["grep", "tool_tiles"],
            &["interrupt", "stopped"],
        ];
        for size in SIZES {
            let mut seen: std::collections::HashMap<String, &str> = Default::default();
            for id in lock_scene_ids() {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                if let Some(other) = seen.insert(frame.ansi.clone(), id) {
                    let documented = aliases
                        .iter()
                        .any(|pair| pair.contains(id) && pair.contains(&other));
                    assert!(
                        documented,
                        "{id} and {other} render the same frame at {size:?}:\n{}",
                        frame.plain
                    );
                }
            }
        }
    }

    #[test]
    fn mode_chips_are_kept() {
        // `┌ Ask — read-only ┐` and `┌ Bash mode ┐` are square mode chips,
        // not session frames — they stay at both sizes.
        for size in SIZES {
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
        // and are allowed; hairlines are single `─` rows, never boxes.
        for id in lock_scene_ids() {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                for glyph in ['╭', '╮', '╰', '╯', '│'] {
                    if glyph == '│' && *id == "btw" {
                        continue; // the side-thread gutter
                    }
                    if glyph == '│' && *id == "config" {
                        continue; // the config tree branches
                    }
                    if glyph == '│' && FENCE_SCENES.contains(id) {
                        continue; // the fence's line-number gutter
                    }
                    assert!(
                        !frame.plain.contains(glyph),
                        "{id} draws a frame glyph {glyph} at {size:?}:\n{}",
                        frame.plain
                    );
                }
            }
        }
    }

    #[test]
    fn slash_palette_rows_are_middot_or_violet_caret() {
        for size in SIZES {
            let frame = render_lock_scene("palette", size.0, size.1).expect("palette");
            assert!(
                frame.plain.contains("> /model"),
                "the focused command leads with the violet caret at {size:?}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("· /mode"),
                "unfocused commands lead with a middot at {size:?}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("more — keep typing to filter"),
                "{}",
                frame.plain
            );
        }
        // The wide selected row keeps the dim description on the bar, never a
        // bright (or violet) description.
        let wide = render_lock_scene("palette", 120, 40).expect("palette wide");
        let buf = &wide.buffer;
        let y = (0..40u16)
            .find(|y| row_text(buf, *y).starts_with("> /model"))
            .expect("selected /model row");
        let text = row_text(buf, y);
        let desc_x = text.find("Choose").expect("description") as u16;
        assert_eq!(buf[(desc_x, y)].style().fg, Some(TEXT_DIM));
        assert_eq!(buf[(desc_x, y)].style().bg, Some(SELECTION_BG));
        assert_eq!(buf[(2, y)].style().fg, Some(ACCENT));
        let other = row_text(buf, y + 1);
        assert!(other.starts_with("· /mode"), "{other}");
        assert_eq!(buf[(0, y + 1)].style().fg, Some(TEXT_DIM));
        assert_eq!(buf[(2, y + 1)].style().fg, Some(TEXT));
        assert!(matches!(
            buf[(2, y + 1)].style().bg,
            None | Some(Color::Reset)
        ));
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
            plain.contains("> Add a follow-up ↵ to queue"),
            "the composer stays on screen during a run:\n{plain}"
        );
    }

    #[test]
    fn splash_has_session_chrome() {
        for size in SIZES {
            let frame = render_lock_scene("splash", size.0, size.1).expect("splash");
            for needle in [
                "~/cortex-api main*",
                "> cortex",
                "Cortex CLI v1.0.0",
                "/ commands · @ files · ! shell",
                "Plan, search, build anything",
                "Cortex Mini 1",
            ] {
                assert!(
                    frame.plain.contains(needle),
                    "splash missing `{needle}` at {size:?}:\n{}",
                    frame.plain
                );
            }
            assert!(!frame.plain.contains("▄█▀▀▀▀█▄"), "{}", frame.plain);
            assert_no_junk(&frame.plain);
        }
        let wide = render_lock_scene("splash", 120, 40).expect("splash wide");
        assert!(wide.plain.contains("& cloud"), "{}", wide.plain);
        assert!(wide.plain.contains("100% context"), "{}", wide.plain);
        // The composer follows the header rather than hugging the footer.
        let composer_y = (0..40u16)
            .find(|y| row_text(&wide.buffer, *y).starts_with("> Plan"))
            .expect("composer");
        assert!(
            composer_y < 10,
            "composer sits under the header:\n{}",
            wide.plain
        );
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
                plain.contains("Device") || plain.contains("device") || plain.contains("Devin"),
                "truncated Device:\n{plain}"
            );
        }
        assert!(!plain.contains("Devin"), "competitor name in:\n{plain}");
        assert!(!plain.to_lowercase().contains("grok"));
        assert!(!plain.to_lowercase().contains("claude"));
        assert!(!plain.to_lowercase().contains("fable"));
    }

    #[test]
    fn login_is_a_numbered_picker_with_live_sub_states() {
        // `login_select` is the picker with the selection moved to option 2:
        // the violet `>` and the gray bar sit on `Paste an API key`, option 1
        // falls back to the dim middot.
        let select = render_lock_scene("login_select", 120, 40).expect("select");
        assert!(select.plain.contains("Welcome to Cortex CLI!"));
        assert!(select.plain.contains("How would you like to log in?"));
        assert!(select.plain.contains("· 1 Continue with browser"));
        assert!(select.plain.contains("> 2 Paste an API key"));
        assert!(select.plain.contains("cortex.foundation/cli/auth"));
        assert!(select.plain.contains("↵ confirm"));
        assert!(!select.plain.contains("Guest"));
        assert!(!select.plain.contains("Exit"));
        assert!(!select.plain.contains("●") && !select.plain.contains("○"));
        assert_eq!(select.plain.matches("Continue with browser").count(), 1);
        assert_no_junk(&select.plain);

        // `login` keeps option 1 focused, so the two captures differ only by
        // where the caret and the bar sit.
        let login = render_lock_scene("login", 120, 40).expect("login");
        assert!(login.plain.contains("> 1 Continue with browser"));
        assert!(login.plain.contains("· 2 Paste an API key"));
        assert_ne!(login.plain, select.plain);
        for (frame, focused) in [(&login, "1 Continue"), (&select, "2 Paste")] {
            let buf = &frame.buffer;
            let row = (0..40u16)
                .find(|y| buf[(0, *y)].symbol() == ">")
                .expect("focused row");
            assert!(
                row_text(buf, row).contains(focused),
                "{focused}:\n{}",
                frame.plain
            );
            assert_eq!(buf[(0, row)].style().fg, Some(ACCENT));
            for x in 0..120u16 {
                assert_eq!(buf[(x, row)].style().bg, Some(SELECTION_BG), "col {x}");
                assert_eq!(buf[(x, row + 1)].style().bg, Some(SELECTION_BG), "col {x}");
            }
            let other = (0..40u16)
                .find(|y| buf[(0, *y)].symbol() == "·")
                .expect("unfocused row");
            assert_eq!(buf[(4, other)].style().fg, Some(TEXT));
            assert_eq!(buf[(4, other)].style().bg, Some(Color::Reset));
        }

        let narrow = render_lock_scene("login_select", 40, 12).expect("narrow");
        for needle in [
            "Welcome to Cortex CLI!",
            "· 1 Continue with browser",
            "Opens cortex.foundation/cli/auth",
            "> 2 Paste an API key",
            "Enter your key to authenticate",
            "↑↓ select · ↵ confirm · esc quit",
        ] {
            assert!(narrow.plain.contains(needle), "{needle}\n{}", narrow.plain);
        }
        let hint_idx = narrow.plain.find("Opens").expect("hint");
        let paste_idx = narrow.plain.find("Paste an API key").expect("paste");
        assert!(
            hint_idx < paste_idx,
            "the description sits under its title:\n{}",
            narrow.plain
        );
        assert_no_junk(&narrow.plain);

        let waiting = render_lock_scene("login_waiting", 120, 40).expect("waiting");
        assert!(waiting.plain.contains("Waiting for browser"));

        let ok = render_lock_scene("login_success", 80, 24).expect("ok");
        assert!(ok.plain.contains("✓ Signed in."));

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
        assert!(narrow.plain.contains("/model"), "{}", narrow.plain);
        assert!(
            narrow.plain.contains("Choose the model"),
            "narrow slash should keep a description under the command:\n{}",
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
        assert!(frame.plain.contains("> Model"), "{}", frame.plain);
        assert!(frame.plain.contains("· Mode"), "{}", frame.plain);
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
        let diff = render_lock_scene("multi_diff", 120, 40).expect("diff");
        assert!(diff.plain.contains("Changed this turn"), "{}", diff.plain);
        assert!(diff.plain.contains("4 files"), "{}", diff.plain);
        assert!(diff.plain.contains("+84"), "{}", diff.plain);
        assert!(diff.plain.contains("open"), "{}", diff.plain);
    }

    #[test]
    fn tool_tile_dots_are_white() {
        // Every tool tile paints its `●` status dot white — never the violet accent,
        // never green. Labels stay white too.
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
            "diff_hunk",
            "diff_word",
        ];
        for id in tiles {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let mut dots = 0;
                for (_, _, cell) in cells(&frame.buffer) {
                    if cell.symbol() == "●" {
                        dots += 1;
                        assert_eq!(
                            cell.style().fg,
                            Some(TEXT),
                            "{id} tile dot must be white at {size:?}"
                        );
                    }
                }
                assert!(
                    dots > 0,
                    "{id} must show a tile dot at {size:?}:\n{}",
                    frame.plain
                );
            }
        }
    }

    #[test]
    fn compact_interrupt_clear_and_states_reflow() {
        for size in SIZES {
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
        for size in SIZES {
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
        // 38 compact reports the compaction, on the gray chrome.
        for size in SIZES {
            let compacted = render_lock_scene("compact", size.0, size.1).expect("compact");
            assert!(compacted.plain.contains("/compact"), "{}", compacted.plain);
            assert!(
                compacted.plain.contains("Thread compacted"),
                "{}",
                compacted.plain
            );
            assert!(compacted.plain.contains("86%"), "{}", compacted.plain);
            assert!(compacted.plain.contains("12%"), "{}", compacted.plain);
        }
        // 40 clear is the confirm dialog, not an empty splash.
        for size in SIZES {
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

        for size in SIZES {
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
                    "Type to search",
                ],
            ),
            (
                "model_full",
                &["Cortex Mini 1", "Cortex Max 1", "Type to search"],
            ),
            ("mode", &["/mode", "Agent", "Plan", "Ask"]),
            (
                "permissions",
                &["/permissions", "Read-only", "Smart", "Full access"],
            ),
            ("working", &["Working", "esc to interrupt", "follow-up"]),
            ("read", &["Read", "completions.ts", "141 lines"]),
        ];
        for size in SIZES {
            for (id, needles) in always {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let lower = frame.plain.to_lowercase();
                assert!(!lower.contains("grok"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("claude"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("fable"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("devin"), "{id}\n{}", frame.plain);
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
        assert!(full.plain.contains("Model"), "{}", full.plain);
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
            (
                "resume",
                &["/resume", "Type to search sessions", "24 messages"],
            ),
            ("mcp", &["/mcp", "2 of 4 connected"]),
            ("usage", &["/usage", "Cortex Pro", "Agent requests"]),
            (
                "quota",
                &["Agent quota exhausted", "500 / 500", "held until quota"],
            ),
            ("sandbox", &["/sandbox", "Sandbox mode"]),
            ("cloud", &["Handed off to Cortex Cloud", "bc-4f2a", "/jobs"]),
        ];

        for size in SIZES {
            for (id, needles) in always {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let lower = frame.plain.to_lowercase();
                assert!(!lower.contains("grok"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("claude"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("fable"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("devin"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("opencode"), "{id}\n{}", frame.plain);
                assert!(
                    frame.plain.contains("Cortex Mini 1"),
                    "{id} footer model at {size:?}:\n{}",
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
        assert!(wide_shell.plain.contains("✓"), "{}", wide_shell.plain);

        let perm = render_lock_scene("permission", 120, 40).expect("perm");
        assert!(perm.plain.contains("always allow npm install"));
        assert!(perm.plain.contains("Edit command"));
        assert!(perm.plain.contains("Normal"));
        assert!(perm.plain.contains("tell Cortex"));
        assert!(perm.plain.contains("$ npm install ioredis"));

        // At 40 columns option 2 wraps — `this project` lands on its own
        // indented row and is never dropped.
        let perm_n = render_lock_scene("permission", 40, 12).expect("perm narrow");
        assert!(
            perm_n
                .plain
                .lines()
                .any(|line| line.starts_with("    ") && line.trim() == "this project"),
            "option copy must wrap, not truncate:\n{}",
            perm_n.plain
        );
        assert!(perm_n.plain.contains("Edit command"), "{}", perm_n.plain);

        let plan = render_lock_scene("plan", 120, 40).expect("plan");
        assert!(plan.plain.contains("Redis-backed"));
        assert!(
            plan.plain
                .contains("> 1 Yes, switch to Agent mode and implement")
        );
        assert!(plan.plain.contains("· 2 No, keep planning"));
        assert!(plan.plain.contains(" · Plan · "));
        let plan_n = render_lock_scene("plan", 40, 12).expect("plan narrow");
        assert!(
            plan_n.plain.contains("implement"),
            "narrow plan must wrap the confirm label, never truncate it:\n{}",
            plan_n.plain
        );

        let resume = render_lock_scene("resume", 120, 40).expect("resume");
        assert!(resume.plain.contains("Sessions sync through Cortex Cloud"));
        assert!(
            resume
                .plain
                .contains("> 2h ago  Rate limiting for /v1/completions")
        );

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
        assert!(sandbox.plain.contains("✓ On"));

        let stream = render_lock_scene("streaming", 120, 40).expect("stream");
        assert!(stream.plain.contains("zadd") || stream.plain.contains("rateLimit"));
        assert!(!stream.plain.contains("Read src/") && !stream.plain.contains("Write src/"));

        let mcp = render_lock_scene("mcp", 120, 40).expect("mcp");
        assert!(mcp.plain.contains("authenticating"));
        assert!(mcp.plain.contains("failed"));
        assert!(mcp.plain.contains("mcp.json"));
        assert!(mcp.plain.contains("✓ github"));
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
            ("queue", &["Queued", "Retry-After", "follow-up"]),
            ("jobs", &["/jobs", "2 running", "cloud"]),
            ("help", &["/help", "/model", "Shortcuts"]),
            ("first_run", &["Cortex CLI", "A few tips", "/model"]),
            (
                "bash",
                &["Bash mode", "the model is not involved", "redis-cli"],
            ),
            (
                "config",
                &["/config", "~/.cortex/config.json", "Cortex Mini 1"],
            ),
            ("footer_max", &["Committed and pushed", "MAX", "follow-up"]),
        ];

        for size in SIZES {
            for (id, needles) in always {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let lower = frame.plain.to_lowercase();
                assert!(!lower.contains("grok"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("claude"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("fable"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("devin"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("opencode"), "{id}\n{}", frame.plain);
                assert!(
                    !frame.plain.contains("gpt-"),
                    "{id} must use a Cortex product model name:\n{}",
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
        // The typed mention sits in the hairline composer.
        assert!(
            files_n.plain.contains("> Add integration tests for @rate█"),
            "{}",
            files_n.plain
        );

        let ask = render_lock_scene("ask", 120, 40).expect("ask");
        assert!(ask.plain.contains("Ask — read-only"));
        assert!(ask.plain.contains("estimateTokens"));
        assert!(ask.plain.contains(" · Ask · "));

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
        assert!(cfg.plain.contains("> ├── model"));

        let max = render_lock_scene("footer_max", 120, 40).expect("max");
        assert!(max.plain.contains("rate-limit-9e4d"));
        assert!(max.plain.contains("+214"));
        assert!(max.plain.contains("-9"));
        assert!(max.plain.contains("38% context left"));
        assert!(max.plain.contains("> Add a follow-up█"));
    }

    #[test]
    fn lock_boards_31_40_product_copy() {
        let always: &[(&str, &[&str])] = &[
            (
                "login",
                &[
                    "Welcome to Cortex CLI!",
                    "Continue with browser",
                    "Paste an API key",
                    "confirm",
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
        for size in SIZES {
            for (id, needles) in always {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let lower = frame.plain.to_lowercase();
                assert!(!lower.contains("grok"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("claude"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("fable"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("devin"), "{id}\n{}", frame.plain);
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
        assert!(think.plain.contains("⠇ Thinking · 14s · esc to interrupt"));

        let q = render_lock_scene("question", 120, 40).expect("q");
        assert!(q.plain.contains("Shared limiter"));
        assert!(q.plain.contains("Plan"));

        let skills = render_lock_scene("skills", 120, 40).expect("skills");
        for cmd in ["/commit", "/pr", "/review", "/fix-ci", "/migrate"] {
            assert!(skills.plain.contains(cmd), "{cmd}\n{}", skills.plain);
        }
        assert!(skills.plain.contains("> /pr"), "{}", skills.plain);
        assert!(skills.plain.contains("· /commit"), "{}", skills.plain);

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
        for size in SIZES {
            for (id, needles) in always {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let lower = frame.plain.to_lowercase();
                assert!(!lower.contains("grok"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("claude"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("fable"), "{id}\n{}", frame.plain);
                assert!(!lower.contains("devin"), "{id}\n{}", frame.plain);
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
        assert!(del.plain.contains("esc keep"));
        let list = render_lock_scene("list", 120, 40).expect("list");
        assert!(list.plain.contains("auth.ts"));
        assert!(list.plain.contains("cors.ts"));
        // The identifier is camelCase everywhere, same as @files.
        for size in SIZES {
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
        assert!(diff.plain.contains("> src/middleware/rateLimit.ts"));
    }
}
