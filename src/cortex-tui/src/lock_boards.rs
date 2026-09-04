//! Pixel-lock painters for boards 01–50.
//!
//! These scenes share the gray session chrome — a past user turn on its gray
//! bar, the hairline-framed `> ` composer, and the `model · hint` footer —
//! and Cortex product copy only. The one accent is the violet of a focused
//! selection; green covers `✓` and `+diff`; red and amber stay on
//! diagnostics; the Thinking status is the muted gold.

use cortex_core::markdown::{TableBuilder, render_table};
use cortex_core::style::{
    ACCENT, DIFF_ADD, ERROR, HAIRLINE, PANEL_BG, SELECTION_BG, SUCCESS, SURFACE_2, TEXT, TEXT_DIM,
    THINKING, USER_TURN_BG, WARNING,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::ui::text_utils::{
    first_fitting_line, fit_line, trim_dangling_separator, wrap_keep_indent, wrap_or_drop,
};

/// Prompt typed in the README hero GIF and the typing lock board.
pub const USER_PROMPT: &str = "Add rate limiting to POST /v1/completions – 60 req/min per API key, sliding window, Redis-backed, with tests";
const CWD: &str = "~/cortex-api";
/// English product name — the TUI never shows the served `cortex-1-mini` slug.
const MODEL: &str = "Cortex Mini 1";
/// Command rows (`$ npm install …`, the sudo password) sit on the user-turn gray.
const COMMAND_BG: Color = USER_TURN_BG;

/// Composer placeholder while idle — the live composer's copy.
const GHOST_IDLE: &str = crate::views::minimal_session::PLACEHOLDER_IDLE;
/// Composer placeholder while a run is live — the live composer's copy.
const GHOST_RUNNING: &str = crate::views::minimal_session::PLACEHOLDER_RUNNING;
/// Right-hand footer hint, and its narrow form — the live footer's copy.
const FOOTER_HINT: &str = crate::widgets::key_hints::FOOTER_HINT_IDLE;
const FOOTER_HINT_SHORT: &str = "shift+tab modes";
/// Keystroke hints under the splash, and the form that fits 40 columns.
const LAUNCH_HINTS: &str = crate::views::minimal_session::EMPTY_SESSION_HINTS;
const LAUNCH_HINTS_NARROW: &str = "/ commands · @ files · ! shell";
/// Rows the composer takes: hairline, prompt, hairline.
const COMPOSER_ROWS: u16 = crate::views::minimal_session::COMPOSER_ROWS;

/// True when `id` is a dedicated lock-board painter (01–50).
pub fn is_lock_board(id: &str) -> bool {
    matches!(
        id,
        "typing"
            | "model_compact"
            | "model_full"
            | "mode"
            | "permissions"
            | "working"
            | "read"
            | "shell"
            | "permission"
            | "plan"
            | "streaming"
            | "resume"
            | "mcp"
            | "usage"
            | "quota"
            | "sandbox"
            | "cloud"
            | "sudo"
            | "ask"
            | "files"
            | "queue"
            | "jobs"
            | "help"
            | "first_run"
            | "bash"
            | "config"
            | "footer_max"
            | "thinking"
            | "todos"
            | "question"
            | "skills"
            | "btw"
            | "stopped"
            | "compacted"
            | "write"
            | "clear_confirm"
            | "grep"
            | "glob"
            | "delete"
            | "list"
            | "fetch"
            | "mcp_call"
            | "task"
            | "diagnostics"
            | "multi_diff"
            | "settings_hub"
            | "edit"
            | "splash"
            | "palette"
            | "sandbox_deny"
            | "mcp_drop"
    )
}

/// Paint one board into `area`.
pub fn render_lock_board(id: &str, area: Rect, buf: &mut Buffer) {
    match id {
        "typing" => board_typing(area, buf),
        "model_compact" => board_model_compact(area, buf),
        "model_full" => board_model_full(area, buf),
        "mode" => board_mode(area, buf),
        "permissions" => board_permissions(area, buf),
        "working" => board_working(area, buf),
        "read" => board_read(area, buf),
        "shell" => board_shell(area, buf),
        "permission" => board_permission(area, buf),
        "plan" => board_plan(area, buf),
        "streaming" => board_streaming(area, buf),
        "resume" => board_resume(area, buf),
        "mcp" => board_mcp(area, buf),
        "usage" => board_usage(area, buf),
        "quota" => board_quota(area, buf),
        "sandbox" => board_sandbox(area, buf),
        "cloud" => board_cloud(area, buf),
        "sudo" => board_sudo(area, buf),
        "ask" => board_ask(area, buf),
        "files" => board_files(area, buf),
        "queue" => board_queue(area, buf),
        "jobs" => board_jobs(area, buf),
        "help" => board_help(area, buf),
        "first_run" => board_first_run(area, buf),
        "bash" => board_bash(area, buf),
        "config" => board_config(area, buf),
        "footer_max" => board_footer_max(area, buf),
        "thinking" => board_thinking(area, buf),
        "todos" => board_todos(area, buf),
        "question" => board_question(area, buf),
        "skills" => board_skills(area, buf),
        "btw" => board_btw(area, buf),
        "stopped" => board_stopped(area, buf),
        "compacted" => board_compacted(area, buf),
        "write" => board_write(area, buf),
        "clear_confirm" => board_clear_confirm(area, buf),
        "grep" => board_grep(area, buf),
        "glob" => board_glob(area, buf),
        "delete" => board_delete(area, buf),
        "list" => board_list(area, buf),
        "fetch" => board_fetch(area, buf),
        "mcp_call" => board_mcp_call(area, buf),
        "task" => board_task(area, buf),
        "diagnostics" => board_diagnostics(area, buf),
        "multi_diff" => board_multi_diff(area, buf),
        "settings_hub" => board_settings_hub(area, buf),
        "edit" => board_edit(area, buf),
        "splash" => board_splash(area, buf),
        "palette" => board_palette(area, buf),
        "sandbox_deny" => board_sandbox_deny(area, buf),
        "mcp_drop" => board_mcp_drop(area, buf),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Shared chrome
// ---------------------------------------------------------------------------

fn inner_width(area: Rect) -> usize {
    area.width.saturating_sub(1).max(1) as usize
}

fn compact(area: Rect) -> bool {
    area.height < 20 || area.width < 50
}

/// One full-width hairline on row `y`.
fn paint_hairline(area: Rect, buf: &mut Buffer, y: u16) {
    if y >= area.bottom() || y < area.y {
        return;
    }
    buf.set_string(
        area.x,
        y,
        "─".repeat(area.width as usize),
        Style::default().fg(HAIRLINE),
    );
}

/// Fill row `y` across the area with `bg`, keeping white as the default fg.
fn fill_row(buf: &mut Buffer, area: Rect, y: u16, bg: Color) {
    if y >= area.bottom() || y < area.y {
        return;
    }
    for x in area.x..area.right() {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_bg(bg);
            cell.set_fg(TEXT);
        }
    }
}

/// What the composer shows between its hairlines.
enum Composer<'a> {
    /// Idle or running: block cursor at input col 0, dim placeholder after it.
    Ghost(&'a str),
    /// Typed copy with the white block caret at end of line; wraps whole.
    Typed(&'a str),
}

/// The lock prompt bar: a hairline, `> …`, a hairline. Starts on
/// row `y`; returns the rows used (3, more when typed copy wraps).
fn paint_composer(area: Rect, buf: &mut Buffer, y: u16, composer: Composer<'_>) -> u16 {
    let w = inner_width(area);
    paint_hairline(area, buf, y);
    let mut row = y + 1;
    match composer {
        Composer::Ghost(ghost) => {
            crate::views::minimal_session::paint_composer_contents(
                buf,
                area.x,
                row,
                area.width,
                "",
                0,
                true,
                Some(ghost),
            );
            row += 1;
        }
        Composer::Typed(text) => {
            let parts = if compact(area) {
                vec![first_fitting_line(text, w.saturating_sub(3))]
            } else {
                wrap_or_drop(text, w.saturating_sub(3))
            };
            let last = parts.len().saturating_sub(1);
            for (i, part) in parts.iter().enumerate() {
                if i == 0 && parts.len() == 1 {
                    crate::views::minimal_session::paint_composer_contents(
                        buf,
                        area.x,
                        row,
                        area.width,
                        part,
                        part.chars().count(),
                        true,
                        None,
                    );
                    row += 1;
                    continue;
                }
                let prefix = if i == 0 { "> " } else { "  " };
                let prefix_style = if i == 0 {
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                buf.set_string(area.x, row, prefix, prefix_style);
                buf.set_string(
                    area.x + prefix.chars().count() as u16,
                    row,
                    part,
                    Style::default().fg(TEXT),
                );
                if i == last {
                    let x = area.x + prefix.chars().count() as u16 + part.chars().count() as u16;
                    buf.set_string(
                        x,
                        row,
                        crate::views::minimal_session::BLOCK_CURSOR.to_string(),
                        Style::default().fg(cortex_core::style::TEXT_BRIGHT),
                    );
                }
                row += 1;
            }
            if parts.is_empty() {
                crate::views::minimal_session::paint_composer_contents(
                    buf, area.x, row, area.width, "", 0, true, None,
                );
                row += 1;
            }
        }
    }
    paint_hairline(area, buf, row);
    row + 1 - y
}

/// Session footer: `left` (model · mode · context) dim on the left, the
/// `shift+tab` hint dim on the right. The model always wins: the left side
/// falls back to the bare model name, the hint to its short form, then off.
fn paint_footer(area: Rect, buf: &mut Buffer, left: &str) {
    paint_footer_with_hint(area, buf, left, FOOTER_HINT, FOOTER_HINT_SHORT);
}

/// Footer with a picker-specific hint on the right (`""` for none).
fn paint_footer_with_hint(area: Rect, buf: &mut Buffer, left: &str, hint: &str, hint_short: &str) {
    let y = area.bottom().saturating_sub(1);
    let w = area.width as usize;
    let left_opts = [left, MODEL];
    let right_opts = [hint, hint_short, ""];
    let (left_fit, right_fit) =
        crate::widgets::key_hints::fit_footer_pair(&left_opts, &right_opts, w);
    let dim = Style::default().fg(TEXT_DIM);
    if !left_fit.is_empty() {
        buf.set_string(area.x, y, left_fit, dim);
    }
    if !right_fit.is_empty() {
        let rx = area
            .right()
            .saturating_sub(right_fit.chars().count() as u16)
            .max(area.x);
        buf.set_string(rx, y, right_fit, dim);
    }
}

/// Body lines, then the composer directly under them — pinned above the
/// footer once the body fills the screen — then the footer. One blank row
/// separates the body from the composer hairline when there is room.
fn paint_session(
    area: Rect,
    buf: &mut Buffer,
    mut lines: Vec<Line<'_>>,
    footer_left: &str,
    ghost: &str,
) {
    let footer_h = 1u16;
    let max_body = area.height.saturating_sub(footer_h + COMPOSER_ROWS);
    let ends_blank = lines
        .last()
        .map(|line| line.to_string().trim().is_empty())
        .unwrap_or(true);
    if !ends_blank && (lines.len() as u16) < max_body {
        lines.push(Line::from(""));
    }
    let body_h = (lines.len() as u16).min(max_body);
    Paragraph::new(lines).render(Rect::new(area.x, area.y, area.width, body_h), buf);
    paint_composer(area, buf, area.y + body_h, Composer::Ghost(ghost));
    paint_footer(area, buf, footer_left);
}

/// Dim hints row (`↑↓ select · ↵ confirm · esc close`) one blank row under
/// the content that ended at `after_y` — never below the row above the
/// footer — then a footer with the model only: the hints are the hint. A
/// hint cut at a word boundary never ends on a dangling `·`.
fn paint_hints_and_footer(
    area: Rect,
    buf: &mut Buffer,
    after_y: u16,
    hints: &str,
    footer_left: &str,
) {
    let w = inner_width(area);
    let hints_y = after_y
        .saturating_add(1)
        .min(area.bottom().saturating_sub(2));
    if hints_y > area.y && hints_y < area.bottom() {
        buf.set_string(
            area.x,
            hints_y,
            trim_dangling_separator(&first_fitting_line(hints, w)),
            Style::default().fg(TEXT_DIM),
        );
    }
    paint_footer_with_hint(area, buf, footer_left, "", "");
}

/// A past user turn: `> text` white on the full-width gray bar. Wide areas
/// wrap the copy whole (continuations indented under it); narrow ones keep
/// the first whole line. Rows are padded so the bar spans the terminal.
fn user_turn_lines(text: &str, area: Rect) -> Vec<Line<'static>> {
    let width = area.width.max(3) as usize;
    let bar = Style::default().fg(TEXT).bg(USER_TURN_BG);
    let parts = if compact(area) {
        vec![first_fitting_line(text, width.saturating_sub(3))]
    } else {
        wrap_or_drop(text, width.saturating_sub(3))
    };
    parts
        .into_iter()
        .enumerate()
        .map(|(i, part)| {
            let prefix = if i == 0 { "> " } else { "  " };
            let mut row = format!("{prefix}{part}");
            let used = row.chars().count();
            row.push_str(&" ".repeat(width.saturating_sub(used)));
            Line::from(Span::styled(row, bar))
        })
        .collect()
}

/// The locked user prompt as a past turn, followed by a blank row.
fn user_prompt_lines(area: Rect) -> Vec<Line<'static>> {
    let mut lines = user_turn_lines(USER_PROMPT, area);
    lines.push(Line::from(""));
    lines
}

/// Paint `> /command` as a past turn on row `y`.
fn paint_command_turn(area: Rect, buf: &mut Buffer, y: u16, command: &str) {
    if let Some(line) = user_turn_lines(command, area).into_iter().next() {
        Paragraph::new(line).render(Rect::new(area.x, y, area.width, 1), buf);
    }
}

/// Search field on rows `y..y+3`: hairline, `/ query█` (or the dim
/// placeholder), hairline. Returns the rows used.
fn paint_search_field(area: Rect, buf: &mut Buffer, y: u16, query: &str, placeholder: &str) -> u16 {
    let w = inner_width(area);
    paint_hairline(area, buf, y);
    buf.set_string(area.x, y + 1, "/ ", Style::default().fg(TEXT_DIM));
    if query.is_empty() {
        buf.set_string(
            area.x + 2,
            y + 1,
            first_fitting_line(placeholder, w.saturating_sub(2)),
            Style::default().fg(TEXT_DIM),
        );
    } else {
        buf.set_string(
            area.x + 2,
            y + 1,
            format!("{}█", first_fitting_line(query, w.saturating_sub(3))),
            Style::default().fg(TEXT),
        );
    }
    paint_hairline(area, buf, y + 2);
    3
}

/// One picker option at row `y`.
///
/// Selected: the dark gray bar, a violet `>`, the white number, the violet label,
/// dim `meta` right-aligned, and the dim description on the bar's second
/// row. Unselected: a dim `·`, white number and label, dim meta and
/// description. Returns the rows used; nothing is painted past `limit`.
fn picker_option(
    area: Rect,
    buf: &mut Buffer,
    y: u16,
    limit: u16,
    selected: bool,
    number: Option<usize>,
    label: &str,
    meta: &str,
    description: &str,
) -> u16 {
    if y >= limit {
        return 0;
    }
    let w = inner_width(area);
    let has_description = !description.is_empty() && y + 1 < limit;
    let base_bg = if selected { SELECTION_BG } else { Color::Reset };
    if selected {
        fill_row(buf, area, y, SELECTION_BG);
        if has_description {
            fill_row(buf, area, y + 1, SELECTION_BG);
        }
    }
    let marker_style = if selected {
        Style::default().fg(ACCENT).bg(SELECTION_BG)
    } else {
        Style::default().fg(TEXT_DIM)
    };
    buf.set_string(area.x, y, if selected { "> " } else { "· " }, marker_style);
    let mut x = area.x + 2;
    if let Some(n) = number {
        buf.set_string(x, y, format!("{n} "), Style::default().fg(TEXT).bg(base_bg));
        x += 2;
    }
    let indent = (x - area.x) as usize;
    let label_style = if selected {
        Style::default()
            .fg(ACCENT)
            .bg(SELECTION_BG)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT)
    };
    let mut budget = w.saturating_sub(indent);
    let meta_fit = fit_line(meta, w.saturating_sub(indent + 2));
    let mut show_meta = false;
    if !meta_fit.is_empty() {
        let need = indent + label.chars().count() + 2 + meta_fit.chars().count();
        if need <= w {
            show_meta = true;
            budget = w.saturating_sub(indent + meta_fit.chars().count() + 2);
        }
    }
    let name = fit_line(label, budget);
    buf.set_string(x, y, &name, label_style);
    if show_meta {
        let mx = area
            .right()
            .saturating_sub(meta_fit.chars().count() as u16 + 1)
            .max(x + name.chars().count() as u16 + 2);
        buf.set_string(mx, y, &meta_fit, Style::default().fg(TEXT_DIM).bg(base_bg));
    }
    let mut rows = 1;
    if has_description {
        let desc = fit_line(description, w.saturating_sub(indent));
        buf.set_string(
            area.x + indent as u16,
            y + 1,
            desc,
            Style::default().fg(TEXT_DIM).bg(base_bg),
        );
        rows += 1;
    } else if !meta_fit.is_empty() && !show_meta && y + 1 < limit {
        // The meta moves whole under the label when it cannot fit beside it.
        buf.set_string(
            area.x + indent as u16,
            y + 1,
            &meta_fit,
            Style::default().fg(TEXT_DIM).bg(base_bg),
        );
        if selected {
            fill_row(buf, area, y + 1, SELECTION_BG);
            buf.set_string(
                area.x + indent as u16,
                y + 1,
                &meta_fit,
                Style::default().fg(TEXT_DIM).bg(SELECTION_BG),
            );
        }
        rows += 1;
    }
    rows
}

/// Filled charcoal panel holding `lines`, one blank column of padding on the
/// left, from row `y` down to (exclusive) `limit`. Returns the rows used.
fn paint_panel(area: Rect, buf: &mut Buffer, y: u16, limit: u16, lines: &[Line<'_>]) -> u16 {
    let mut row = y;
    for line in lines {
        if row >= limit {
            break;
        }
        fill_row(buf, area, row, PANEL_BG);
        let mut x = area.x + 1;
        for span in &line.spans {
            let style = span.style.bg(PANEL_BG);
            let content = span.content.as_ref();
            let budget = area.right().saturating_sub(x + 1) as usize;
            let shown: String = content.chars().take(budget).collect();
            buf.set_string(x, row, &shown, style);
            x += shown.chars().count() as u16;
        }
        row += 1;
    }
    row - y
}

fn dim(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), Style::default().fg(TEXT_DIM)))
}

fn white(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), Style::default().fg(TEXT)))
}

/// `● Label rest` tile header: white dot, bold white label, white rest.
fn tile_header(label: &str, rest: &str, rest_style: Style, width: usize) -> Line<'static> {
    let mut spans = vec![
        Span::styled("● ", Style::default().fg(TEXT)),
        Span::styled(
            format!("{label} "),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
    ];
    let shown = first_fitting_line(rest, width.saturating_sub(label.chars().count() + 3));
    if !shown.is_empty() {
        spans.push(Span::styled(shown, rest_style));
    }
    Line::from(spans)
}

/// A marker (`● `, `⠇ `, `✓ `, `• `) followed by `text`, word-wrapped so the
/// copy is never cut to a fragment; continuation lines indent under the text.
fn marker_lines(
    marker: &str,
    marker_style: Style,
    text: &str,
    text_style: Style,
    width: usize,
) -> Vec<Line<'static>> {
    let indent = marker.chars().count();
    let mut lines = Vec::new();
    for (i, part) in wrap_or_drop(text, width.saturating_sub(indent))
        .into_iter()
        .enumerate()
    {
        if i == 0 {
            lines.push(Line::from(vec![
                Span::styled(marker.to_string(), marker_style),
                Span::styled(part, text_style),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(indent)),
                Span::styled(part, text_style),
            ]));
        }
    }
    lines
}

/// Fit a list row to `width` keeping its column gaps; a row that had to be
/// shortened ends in ` ...` so the cut is visible.
fn ellipsis_fit_line(text: &str, width: usize) -> String {
    let fitted = fit_line(text, width);
    if fitted == text.trim_end() || fitted.is_empty() {
        return fitted;
    }
    let short = fit_line(text, width.saturating_sub(4));
    if short.is_empty() {
        fitted
    } else {
        format!("{short} ...")
    }
}

fn ellipsis_fit(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let fitted = first_fitting_line(text, max_width);
    if fitted == text || fitted.is_empty() {
        return fitted;
    }
    let with_mark = first_fitting_line(text, max_width.saturating_sub(4));
    if with_mark.is_empty() {
        return fitted;
    }
    format!("{with_mark} ...")
}

/// `  NN  code` rows in one style, the code wrapped under the gutter with its
/// own indentation kept.
fn gutter_lines(width: usize, line_no: u32, code: &str, style: Style) -> Vec<Line<'static>> {
    let prefix = format!("  {line_no:<3} ");
    let pad = " ".repeat(prefix.chars().count());
    wrap_keep_indent(code, width.saturating_sub(prefix.chars().count()).max(1))
        .into_iter()
        .enumerate()
        .map(|(i, part)| {
            Line::from(Span::styled(
                format!("{}{part}", if i == 0 { &prefix } else { &pad }),
                style,
            ))
        })
        .collect()
}

/// Paint `text` word-wrapped from `y` down to (exclusive) `limit`; returns the
/// next free row. Copy is never cut mid-sentence to a single fragment.
fn paint_wrapped(
    buf: &mut Buffer,
    area: Rect,
    x: u16,
    mut y: u16,
    limit: u16,
    text: &str,
    style: Style,
) -> u16 {
    let width = area.right().saturating_sub(x).saturating_sub(1).max(1) as usize;
    for part in wrap_or_drop(text, width) {
        if y >= limit {
            break;
        }
        buf.set_string(x, y, &part, style);
        y += 1;
    }
    y
}

/// Monochrome code: keywords bold white, strings dim, the rest white. Code
/// is content, not chrome — it gets no colour of its own.
fn highlight_code(code: &str) -> Vec<Span<'static>> {
    let keywords = [
        "import",
        "from",
        "export",
        "function",
        "return",
        "const",
        "let",
        "async",
        "await",
        "type",
        "interface",
    ];
    let mut spans = Vec::new();
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' || c == '\'' {
            let quote = c;
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < chars.len() {
                i += 1;
            }
            spans.push(Span::styled(
                chars[start..i].iter().collect::<String>(),
                Style::default().fg(TEXT_DIM),
            ));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let style = if keywords.contains(&word.as_str()) {
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            spans.push(Span::styled(word, style));
            continue;
        }
        spans.push(Span::styled(c.to_string(), Style::default().fg(TEXT)));
        i += 1;
    }
    spans
}

fn grep_hit_line(width: usize, line_no: u32, code: &str) -> Vec<Line<'static>> {
    let num = format!("{line_no}");
    let prefix = format!("  {num:<3} ");
    let rest_w = width.saturating_sub(prefix.chars().count());
    let mut out = Vec::new();
    // Code keeps its own nesting when it wraps.
    let wrapped = wrap_keep_indent(code, rest_w.max(1));
    for (i, part) in wrapped.into_iter().enumerate() {
        let mut spans = Vec::new();
        if i == 0 {
            spans.push(Span::styled(prefix.clone(), Style::default().fg(TEXT_DIM)));
        } else {
            spans.push(Span::styled(
                " ".repeat(prefix.chars().count()),
                Style::default().fg(TEXT_DIM),
            ));
        }
        spans.extend(highlight_code(&part));
        out.push(Line::from(spans));
    }
    out
}

fn paint_match_path(path: &str, needle: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = path;
    while let Some(idx) = rest.find(needle) {
        if idx > 0 {
            spans.push(Span::styled(
                rest[..idx].to_string(),
                Style::default().fg(TEXT),
            ));
        }
        spans.push(Span::styled(
            needle.to_string(),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ));
        rest = &rest[idx + needle.len()..];
    }
    if !rest.is_empty() {
        spans.push(Span::styled(rest.to_string(), Style::default().fg(TEXT)));
    }
    spans
}

fn bar(filled: u16, total: u16) -> String {
    let n = 10u16;
    let on = ((filled as u32 * n as u32) / total.max(1) as u32) as u16;
    let mut s = String::new();
    for i in 0..n {
        s.push(if i < on { '█' } else { '░' });
    }
    s
}

/// Launch header: `Welcome to Cortex, the coding agent CLI` then
/// `v{version} · / commands · …`. No fake `> cortex` or painted cwd.
fn paint_launch_header(area: Rect, buf: &mut Buffer, full: bool) -> u16 {
    let w = inner_width(area);
    let mut y = area.y;
    let dim = Style::default().fg(TEXT_DIM);
    let bold = Style::default().fg(TEXT).add_modifier(Modifier::BOLD);
    buf.set_string(area.x, y, "Welcome to ", dim);
    buf.set_string(area.x + 11, y, "Cortex", bold);
    buf.set_string(area.x + 17, y, ", the coding agent CLI", dim);
    y += 1;
    if full {
        let version = env!("CARGO_PKG_VERSION");
        let full_h = format!("v{version} · {LAUNCH_HINTS}");
        let mid_h = format!("v{version} · {LAUNCH_HINTS_NARROW}");
        let hints = if full_h.chars().count() <= w {
            full_h
        } else if mid_h.chars().count() <= w {
            mid_h
        } else {
            format!("v{version} · / commands")
        };
        buf.set_string(
            area.x,
            y,
            trim_dangling_separator(&first_fitting_line(&hints, w)),
            dim,
        );
        y += 1;
    }
    y + 1
}

// ---------------------------------------------------------------------------
// Boards 01–10
// ---------------------------------------------------------------------------

fn board_splash(area: Rect, buf: &mut Buffer) {
    paint_hero(area, buf, HeroScene::Splash);
}

fn board_typing(area: Rect, buf: &mut Buffer) {
    paint_hero(area, buf, HeroScene::Typing(USER_PROMPT));
}

/// README hero beat: signed splash, in-progress typing, or the working lock.
#[derive(Debug, Clone, Copy)]
pub enum HeroScene<'a> {
    /// Dual-hairline splash with the idle placeholder.
    Splash,
    /// Splash chrome with `text` in the composer (partial or full prompt).
    Typing(&'a str),
    /// Submitted prompt plus the working indicator.
    Working,
}

/// Paint one README-hero / lock frame from the signed chrome.
pub fn paint_hero(area: Rect, buf: &mut Buffer, scene: HeroScene<'_>) {
    match scene {
        HeroScene::Splash => {
            let y = paint_launch_header(area, buf, true);
            paint_composer(area, buf, y, Composer::Ghost(GHOST_IDLE));
            paint_footer(area, buf, &format!("{MODEL} · Agent · 100% context"));
        }
        HeroScene::Typing(text) => {
            let y = paint_launch_header(area, buf, true);
            paint_composer(area, buf, y, Composer::Typed(text));
            paint_footer(area, buf, &format!("{MODEL} · Agent · 100% context"));
        }
        HeroScene::Working => board_working(area, buf),
    }
}

const PALETTE_ROWS: &[(&str, &str)] = &[
    ("/model", "Choose the model for this session"),
    ("/mode", "Switch between Agent, Plan and Ask"),
    (
        "/permissions",
        "Set the approval policy for edits and commands",
    ),
    ("/plan", "Draft a plan before writing any code"),
    ("/effort", "Tune reasoning effort for the current model"),
    ("/mcp", "View and manage MCP servers"),
    ("/sandbox", "Configure sandboxed command execution"),
    ("/usage", "Plan usage, quota and limits"),
    ("/resume", "Resume a previous session"),
    ("/jobs", "Background agents and subagents"),
    ("/skills", "List and manage skills"),
    ("/btw", "Side note for the current turn"),
    ("/compact", "Toggle compact display mode"),
    ("/clear", "Clear current conversation"),
    ("/diff", "Show file diff"),
    ("/copy", "Show how to copy text"),
    ("/config", "Show configuration"),
    ("/login", "Authenticate with Cortex"),
    ("/logout", "Clear stored credentials"),
    ("/settings", "Open settings panel"),
];

/// Footer hint while the slash palette is open, and its narrow form — the
/// live footer's copy.
const PALETTE_HINT: &str = crate::views::minimal_session::PALETTE_FOOTER_HINT;
const PALETTE_HINT_SHORT: &str = crate::views::minimal_session::PALETTE_FOOTER_HINT_SHORT;

fn board_palette(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let narrow = compact(area);
    let mut y = paint_launch_header(area, buf, !narrow);
    y += paint_composer(area, buf, y, Composer::Typed("/"));
    let limit = area.bottom().saturating_sub(1);
    let take = if narrow { 2 } else { PALETTE_ROWS.len() };
    // Descriptions line up in one column after the widest command.
    let widest = PALETTE_ROWS
        .iter()
        .map(|(cmd, _)| cmd.chars().count())
        .max()
        .unwrap_or(0);
    let mut shown = 0usize;
    for (i, (cmd, desc)) in PALETTE_ROWS.iter().enumerate() {
        if shown >= take || y + 1 >= limit {
            break;
        }
        let selected = i == 0;
        let gap = 2 + widest + 2;
        let same_line = first_fitting_line(desc, w.saturating_sub(gap));
        if narrow || same_line.is_empty() {
            // Narrow: the description moves whole under the command.
            y += picker_option(area, buf, y, limit, selected, None, cmd, "", desc);
        } else {
            if selected {
                fill_row(buf, area, y, SELECTION_BG);
            }
            let (marker, marker_style, cmd_style) = if selected {
                (
                    "> ",
                    Style::default().fg(ACCENT).bg(SELECTION_BG),
                    Style::default()
                        .fg(ACCENT)
                        .bg(SELECTION_BG)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    "· ",
                    Style::default().fg(TEXT_DIM),
                    Style::default().fg(TEXT),
                )
            };
            buf.set_string(area.x, y, marker, marker_style);
            buf.set_string(area.x + 2, y, cmd, cmd_style);
            let desc_style = if selected {
                Style::default().fg(TEXT_DIM).bg(SELECTION_BG)
            } else {
                Style::default().fg(TEXT_DIM)
            };
            buf.set_string(area.x + gap as u16, y, &same_line, desc_style);
            y += 1;
        }
        shown += 1;
    }
    let remaining = 21usize.saturating_sub(shown);
    if remaining > 0 && y < limit {
        buf.set_string(
            area.x,
            y,
            first_fitting_line(&format!("{remaining} more — keep typing to filter"), w),
            Style::default().fg(TEXT_DIM),
        );
    }
    paint_footer_with_hint(
        area,
        buf,
        &format!("{MODEL} · Agent"),
        PALETTE_HINT,
        PALETTE_HINT_SHORT,
    );
}

const MODEL_ROWS: &[(bool, &str, &str, &str)] = &[
    (
        true,
        "Cortex Mini 1",
        "Medium · current",
        "Fast default for everyday coding.",
    ),
    (
        false,
        "Cortex 1",
        "High",
        "Deeper reasoning for hard changes.",
    ),
    (
        false,
        "Cortex Max 1",
        "MAX · token billing",
        "Longest context — bills by token instead of per request.",
    ),
];

const MODEL_HINTS: &str = "↑↓ select · ↵ confirm · tab effort · esc close";

fn board_model_compact(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    paint_command_turn(area, buf, y, "/model");
    y += 1;
    buf.set_string(
        area.x,
        y,
        first_fitting_line("Model", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    y += 1;
    y += paint_search_field(area, buf, y, "", "Type to search models");
    let limit = area.bottom().saturating_sub(2);
    for (selected, name, meta, _detail) in MODEL_ROWS {
        if y >= limit {
            break;
        }
        y += picker_option(area, buf, y, limit, *selected, None, name, meta, "");
    }
    paint_hints_and_footer(area, buf, y, MODEL_HINTS, &format!("{MODEL} · Agent"));
}

fn board_model_full(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let narrow = compact(area);
    let mut y = area.y;
    // The full picker is the full picker at every size: a description under
    // each model plus the Effort radios. At 40×12 the prompt row and the
    // Model title give way so all of it fits above the hints.
    if !narrow {
        paint_command_turn(area, buf, y, "/model");
        y += 1;
        buf.set_string(
            area.x,
            y,
            first_fitting_line("Model", w),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        );
        y += 1;
    }
    y += paint_search_field(area, buf, y, "", "Type to search models");
    let limit = area.bottom().saturating_sub(2);
    for (selected, name, meta, detail) in MODEL_ROWS {
        if y + 1 >= limit {
            break;
        }
        // Narrow keeps a whole sentence under the MAX model.
        let detail = if narrow && *name == "Cortex Max 1" {
            "Longest context — token billing."
        } else {
            detail
        };
        y += picker_option(area, buf, y, limit, *selected, None, name, meta, detail);
    }
    if !narrow {
        y += 1;
    }
    if y < limit {
        let effort = "○ Low   ● Medium   ○ High";
        buf.set_string(
            area.x,
            y,
            "Effort",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        );
        if narrow {
            buf.set_string(
                area.x + 8,
                y,
                fit_line(effort, w.saturating_sub(8)),
                Style::default().fg(TEXT),
            );
            y += 1;
        } else {
            y += 1;
            if y < limit {
                buf.set_string(area.x, y, fit_line(effort, w), Style::default().fg(TEXT));
                y += 1;
            }
        }
    }
    if !narrow {
        y += 1;
        for part in wrap_or_drop(
            "MAX bills by token instead of per request — manage at cortex.foundation/billing",
            w,
        ) {
            if y >= limit {
                break;
            }
            buf.set_string(area.x, y, &part, Style::default().fg(TEXT_DIM));
            y += 1;
        }
    }
    paint_hints_and_footer(area, buf, y, MODEL_HINTS, &format!("{MODEL} · Agent"));
}

fn board_mode(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    paint_command_turn(area, buf, y, "/mode");
    y += 1;
    if !compact(area) {
        y += 1;
    }
    buf.set_string(
        area.x,
        y,
        first_fitting_line("Mode", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    y += 1;
    let rows = [
        (true, "Agent", "edits files and runs commands"),
        (false, "Plan", "draft an approach first — no edits"),
        (false, "Ask", "read-only answers on the codebase"),
    ];
    let limit = area.bottom().saturating_sub(2);
    for (i, (selected, label, desc)) in rows.iter().enumerate() {
        if y >= limit {
            break;
        }
        y += picker_option(area, buf, y, limit, *selected, Some(i + 1), label, "", desc);
    }
    if !compact(area) {
        y += 1;
        buf.set_string(
            area.x,
            y,
            first_fitting_line("shift+tab cycles modes anytime — even mid-turn.", w),
            Style::default().fg(TEXT_DIM),
        );
    }
    paint_hints_and_footer(
        area,
        buf,
        y,
        "↑↓ select · ↵ confirm · esc close",
        &format!("{MODEL} · Agent"),
    );
}

fn board_permissions(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    paint_command_turn(area, buf, y, "/permissions");
    y += 1;
    if !compact(area) {
        y += 1;
    }
    buf.set_string(
        area.x,
        y,
        first_fitting_line("Permissions", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    y += 1;
    // Narrow rows keep the whole policy sentence.
    let smart = if compact(area) {
        "auto-approve reads — ask before edits"
    } else {
        "auto-approve safe reads — ask before edits"
    };
    let rows = [
        (false, "Read-only", "never edit files or run commands"),
        (true, "Smart", smart),
        (false, "Full access", "only ask when leaving the sandbox"),
    ];
    let limit = area.bottom().saturating_sub(2);
    for (i, (selected, label, desc)) in rows.iter().enumerate() {
        if y >= limit {
            break;
        }
        y += picker_option(area, buf, y, limit, *selected, Some(i + 1), label, "", desc);
    }
    if !compact(area) {
        y += 1;
        for part in wrap_or_drop(
            "Applies to this project — overrides live in .cortex/config.json.",
            w,
        ) {
            if y >= limit {
                break;
            }
            buf.set_string(area.x, y, &part, Style::default().fg(TEXT_DIM));
            y += 1;
        }
    }
    paint_hints_and_footer(
        area,
        buf,
        y,
        "↑↓ select · ↵ confirm · esc close",
        &format!("{MODEL} · Agent · Smart"),
    );
}

fn board_working(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    lines.extend(marker_lines(
        "⠇ ",
        Style::default().fg(TEXT_DIM),
        "Working — wiring the limiter into completions",
        Style::default().fg(TEXT),
        w,
    ));
    lines.push(dim(first_fitting_line(
        "1m 12s · 8.2k tokens · esc to interrupt",
        w,
    )));
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 92% context"),
        GHOST_RUNNING,
    );
}

fn board_read(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    let path = if compact(area) {
        "completions.ts"
    } else {
        "src/server/routes/completions.ts"
    };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(TEXT)),
        Span::styled(
            "Read ",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            first_fitting_line(path, w.saturating_sub(18)),
            Style::default().fg(TEXT),
        ),
        Span::styled(" · 141 lines", Style::default().fg(TEXT_DIM)),
    ]));
    let excerpt: &[(u32, &str)] = &[
        (21, "import { requireApiKey } from \"../middleware/auth\";"),
        (
            23,
            "export async function completionsRoute(app: FastifyInstance) {",
        ),
        (
            24,
            "  app.post(\"/v1/completions\", { preHandler: [requireApiKey] },",
        ),
    ];
    let body_rows = area.height.saturating_sub(1 + COMPOSER_ROWS) as usize;
    for (no, code) in excerpt {
        let hit = grep_hit_line(w, *no, code);
        if lines.len() + hit.len() > body_rows {
            break;
        }
        lines.extend(hit);
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 95% context"),
        GHOST_RUNNING,
    );
}

fn board_edit(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    let path = if compact(area) {
        "completions.ts"
    } else {
        "src/server/routes/completions.ts"
    };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(TEXT)),
        Span::styled(
            "Edit ",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            first_fitting_line(path, w.saturating_sub(16)),
            Style::default().fg(TEXT),
        ),
        Span::styled(" +9", Style::default().fg(DIFF_ADD)),
        Span::styled(" -2", Style::default().fg(TEXT_DIM)),
    ]));
    // The hunk shows at both sizes: the context line keeps its gutter
    // indentation and the addition wraps whole under the `+` marker.
    lines.extend(gutter_lines(
        w,
        22,
        "{ preHandler: [requireApiKey, limiter] },",
        Style::default().fg(TEXT_DIM),
    ));
    lines.extend(marker_lines(
        "  +   ",
        Style::default().fg(DIFF_ADD),
        "const limiter = rateLimit({ limit: 60, windowSec: 60 });",
        Style::default().fg(TEXT),
        w,
    ));
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 92% context"),
        GHOST_RUNNING,
    );
}

// ---------------------------------------------------------------------------
// Boards 11–20
// ---------------------------------------------------------------------------

fn board_shell(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(TEXT)),
        Span::styled(
            first_fitting_line("Shell npm test -- rateLimit", w.saturating_sub(2)),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        first_fitting_line("⠇ running · 8s · ctrl+c to cancel", w),
        Style::default().fg(TEXT_DIM),
    )));
    if !compact(area) {
        // npm's own output keeps its indentation — its `>` is tool output,
        // not a caret, so it never sits in column 0.
        for line in [
            "  > cortex-api@2.4.1 test",
            "  > vitest run --reporter=verbose \"rateLimit\"",
        ] {
            lines.push(dim(fit_line(line, w)));
        }
        // `✓` is the green; the test names stay dim.
        lines.push(Line::from(vec![
            Span::styled("  ✓ ", Style::default().fg(SUCCESS)),
            Span::styled(
                first_fitting_line(
                    "test/rateLimit.test.ts rejects a 61st request in the window  412ms",
                    w.saturating_sub(4),
                ),
                Style::default().fg(TEXT_DIM),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  ✓ ", Style::default().fg(SUCCESS)),
            Span::styled(
                first_fitting_line(
                    "test/rateLimit.test.ts returns 429 with Retry-After  187ms",
                    w.saturating_sub(4),
                ),
                Style::default().fg(TEXT_DIM),
            ),
        ]));
    }
    lines.extend(marker_lines(
        "  ⠇ ",
        Style::default().fg(TEXT_DIM),
        "test/rateLimit.test.ts allows requests again after the window slides",
        Style::default().fg(TEXT_DIM),
        w,
    ));
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 85% context"),
        GHOST_RUNNING,
    );
}

fn board_permission(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    let reserve = 8u16;

    if !compact(area) {
        for line in user_prompt_lines(area) {
            if y + reserve >= area.bottom() {
                break;
            }
            Paragraph::new(vec![line]).render(Rect::new(area.x, y, area.width, 1), buf);
            y += 1;
        }
        for part in wrap_or_drop(
            "Redis client is missing from package.json — Cortex wants to install it.",
            w,
        ) {
            if y + reserve >= area.bottom() {
                break;
            }
            buf.set_string(area.x, y, &part, Style::default().fg(TEXT_DIM));
            y += 1;
        }
        y += 1;
    }

    buf.set_string(
        area.x,
        y,
        first_fitting_line("● Cortex wants to run", w),
        Style::default().fg(TEXT),
    );
    y += 1;

    fill_row(buf, area, y, COMMAND_BG);
    buf.set_string(
        area.x,
        y,
        first_fitting_line("$ npm install ioredis && npm install -D ioredis-mock", w),
        Style::default().fg(TEXT).bg(COMMAND_BG),
    );
    y += 1;
    buf.set_string(
        area.x,
        y,
        first_fitting_line(&format!("in {CWD}"), w),
        Style::default().fg(TEXT_DIM),
    );
    y += if compact(area) { 1 } else { 2 };

    let options = [
        (true, "Yes, run once"),
        (false, "Yes, always allow npm install in this project"),
        (false, "Edit command"),
        (false, "No — tell Cortex what to do instead"),
    ];
    let limit = area.bottom().saturating_sub(2);
    for (i, (selected, label)) in options.iter().enumerate() {
        if y >= limit {
            break;
        }
        // Option copy word-wraps at narrow widths — words are never dropped
        // or truncated; the bar covers every row of the chosen option.
        let parts = wrap_or_drop(label, w.saturating_sub(4));
        for (j, part) in parts.iter().enumerate() {
            if y >= limit {
                break;
            }
            if j == 0 {
                y += picker_option(area, buf, y, limit, *selected, Some(i + 1), part, "", "");
            } else {
                if *selected {
                    fill_row(buf, area, y, SELECTION_BG);
                }
                let style = if *selected {
                    Style::default().fg(ACCENT).bg(SELECTION_BG)
                } else {
                    Style::default().fg(TEXT)
                };
                buf.set_string(area.x + 4, y, part, style);
                y += 1;
            }
        }
    }
    paint_hints_and_footer(
        area,
        buf,
        y,
        "↑↓ select · ↵ confirm · e edit command · esc cancel",
        &format!("{MODEL} · Agent · Normal · 90% context"),
    );
}

fn board_plan(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = if compact(area) {
        Vec::new()
    } else {
        user_prompt_lines(area)
    };
    // The plan title wraps at 40 columns instead of stopping mid-sentence.
    lines.extend(marker_lines(
        "● ",
        Style::default().fg(TEXT),
        "Plan Redis-backed rate limiting for /v1/completions",
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        w,
    ));
    let steps = [
        (
            "1. Add a Redis client singleton",
            "src/lib/redis.ts — shared connection for the process.",
        ),
        (
            "2. Create the rateLimit middleware",
            "Sliding window with Redis sorted sets.",
        ),
        (
            "3. Wire into POST /v1/completions",
            "Limit 60 req/min per API key.",
        ),
        (
            "4. Make limits configurable",
            "Read from environment variables.",
        ),
        (
            "5. Integration tests using ioredis-mock",
            "Cover window slide and 429 responses.",
        ),
    ];
    let shown_steps = if compact(area) {
        &steps[..2]
    } else {
        &steps[..]
    };
    for (title, detail) in shown_steps {
        lines.push(white(first_fitting_line(title, w)));
        if !compact(area) {
            for part in wrap_or_drop(detail, w) {
                lines.push(dim(part));
            }
        }
    }
    lines.push(Line::from(""));
    lines.push(white(first_fitting_line("Implement this plan?", w)));

    // The confirm label wraps onto a second bar row when narrow — copy is
    // never mid-word truncated.
    let yes_label = "Yes, switch to Agent mode and implement";
    let yes_lines: Vec<String> = wrap_or_drop(yes_label, w.saturating_sub(4))
        .into_iter()
        .take(2)
        .collect();
    let yes_rows = yes_lines.len().max(1) as u16;
    // At 40 columns the option ends at the dash instead of trailing off
    // mid-sentence.
    let no_label = if compact(area) {
        "No, keep planning"
    } else {
        "No, keep planning — tell Cortex what to change"
    };

    let body_h = area.height.saturating_sub(3 + yes_rows);
    Paragraph::new(lines).render(Rect::new(area.x, area.y, area.width, body_h), buf);

    let no_y = area.bottom().saturating_sub(3);
    let yes_y = no_y.saturating_sub(yes_rows);
    let limit = no_y;
    for (i, part) in yes_lines.iter().enumerate() {
        let y = yes_y + i as u16;
        if i == 0 {
            picker_option(area, buf, y, limit, true, Some(1), part, "", "");
        } else {
            fill_row(buf, area, y, SELECTION_BG);
            buf.set_string(
                area.x + 4,
                y,
                part,
                Style::default()
                    .fg(ACCENT)
                    .bg(SELECTION_BG)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
    picker_option(area, buf, no_y, no_y + 1, false, Some(2), no_label, "", "");
    paint_hints_and_footer(
        area,
        buf,
        area.bottom(),
        "↑↓ select · ↵ confirm · esc keep planning",
        &format!("{MODEL} · Plan · 93% context"),
    );
}

fn board_streaming(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    for part in wrap_or_drop("Done — the limiter is in place. Here is how it works:", w) {
        lines.push(white(part));
    }
    for (label, detail) in [
        ("rateLimit()", "checks a Redis sorted set per API key."),
        ("ZADD", "records the request timestamp."),
        ("429", "is returned when the window is full."),
        (
            "Retry-After",
            "and X-RateLimit-Remaining: 0 go on the response.",
        ),
    ] {
        if compact(area) && label != "rateLimit()" {
            continue;
        }
        // The code span carries its own trailing space so the detail never
        // smashes into it, and the bullet wraps whole under its marker.
        let full = format!("{label} {detail}");
        for (i, part) in wrap_or_drop(&full, w.saturating_sub(2))
            .into_iter()
            .enumerate()
        {
            let mut spans = vec![Span::styled(
                if i == 0 { "• " } else { "  " },
                Style::default().fg(TEXT_DIM),
            )];
            match part.strip_prefix(label) {
                Some(rest) if i == 0 => {
                    spans.push(Span::styled(
                        format!("{label} "),
                        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::styled(
                        rest.trim_start().to_string(),
                        Style::default().fg(TEXT_DIM),
                    ));
                }
                _ => spans.push(Span::styled(part, Style::default().fg(TEXT_DIM))),
            }
            lines.push(Line::from(spans));
        }
    }
    if !compact(area) {
        lines.push(Line::from(""));
        lines.push(dim("```ts"));
        for code in [
            "export async function rateLimit(key: string) {",
            "  const now = Date.now();",
            "  await redis.zadd(key, now, String(now));",
            "  await redis.zremrangebyscore(key, 0, now - 60_000);",
            "  const n = await redis.zcard(key);",
            "  return n <= 60;",
            "}",
        ] {
            // Keep the code's own indentation.
            lines.push(white(fit_line(code, w)));
        }
        lines.push(dim("```"));
        lines.push(Line::from(""));
    }
    let closing = "The middleware fails open if Redis is unreachable, logging a warning instead of blocking traffic. Next I will run the integration suite against a local Redis";
    let wrapped = wrap_or_drop(closing, w);
    let last_i = wrapped.len().saturating_sub(1);
    for (i, part) in wrapped.into_iter().enumerate() {
        if compact(area) && i > 0 {
            break;
        }
        if i == last_i || compact(area) {
            lines.push(Line::from(vec![
                Span::styled(part, Style::default().fg(TEXT)),
                Span::styled("▌", Style::default().fg(TEXT)),
            ]));
            break;
        }
        lines.push(white(part));
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 81% context"),
        GHOST_RUNNING,
    );
}

fn board_resume(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    paint_command_turn(area, buf, y, "/resume");
    y += 1;
    y += paint_search_field(area, buf, y, "", "Type to search sessions");

    let rows = [
        (
            true,
            "2h ago",
            "Rate limiting for /v1/completions",
            "main",
            "24 messages",
        ),
        (
            false,
            "yesterday",
            "Fix flaky auth token refresh test",
            "fix/auth-refresh",
            "41 messages",
        ),
        (
            false,
            "2 days ago",
            "Migrate billing webhooks to a queue",
            "main",
            "18 messages",
        ),
        (
            false,
            "5 days ago",
            "SDK retries with exponential backoff",
            "sdk-retries",
            "32 messages",
        ),
        (
            false,
            "last week",
            "Prune unused feature flags",
            "chore/flags",
            "9 messages",
        ),
    ];

    let list_limit = if compact(area) {
        // Rows of sessions, then the sync note above the hints.
        area.bottom().saturating_sub(3)
    } else {
        area.bottom().saturating_sub(4)
    };
    for (selected, when, title, branch, msgs) in rows {
        if y >= list_limit {
            break;
        }
        // Column gaps survive (`fit_line`); narrow rows end in an ellipsis
        // rather than a mid-name cut, and the focused row keeps its message
        // count beside a short title.
        let (label, meta) = if compact(area) && selected {
            (format!("{when}  Rate limiting"), msgs.to_string())
        } else if compact(area) {
            (
                ellipsis_fit_line(&format!("{when}  {title}"), w.saturating_sub(2)),
                String::new(),
            )
        } else {
            (format!("{when}  {title}"), format!("{branch}  {msgs}"))
        };
        y += picker_option(area, buf, y, list_limit, selected, None, &label, &meta, "");
    }
    if !compact(area) {
        y += 1;
        y = paint_wrapped(
            buf,
            area,
            area.x,
            y,
            area.bottom().saturating_sub(2),
            "Sessions sync through Cortex Cloud — resume from any machine.",
            Style::default().fg(TEXT_DIM),
        );
    }
    paint_hints_and_footer(
        area,
        buf,
        y,
        "↑↓ select · ↵ resume · x delete · esc cancel",
        &format!("{MODEL} · Agent"),
    );
}

fn board_mcp(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_turn_lines("/mcp", area);
    lines.push(white(first_fitting_line(
        "MCP servers · 2 of 4 connected",
        w,
    )));
    lines.push(Line::from(""));
    // A green `✓` marks connected servers only; every status word stays gray.
    let servers = [
        (
            "✓",
            SUCCESS,
            "github",
            "connected",
            "12 tools · repos, issues, pull requests",
        ),
        (
            "✓",
            SUCCESS,
            "postgres",
            "connected",
            "6 tools · localhost:5432/cortex",
        ),
        (
            "⠇",
            TEXT_DIM,
            "sentry",
            "authenticating",
            "waiting for browser sign-in...",
        ),
        (
            "x",
            ERROR,
            "linear",
            "failed",
            "connection refused — retrying in 30s",
        ),
    ];
    for (mark, mark_color, name, status, detail) in servers {
        if compact(area) && matches!(name, "postgres" | "sentry") {
            continue;
        }
        if compact(area) {
            // Narrow: name + status on one row, the detail whole on the next
            // (github's tool list uses its short form).
            let detail = if name == "github" {
                "12 tools · repos, issues, PRs"
            } else {
                detail
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{mark} "), Style::default().fg(mark_color)),
                Span::styled(format!("{name}  "), Style::default().fg(TEXT)),
                Span::styled(status.to_string(), Style::default().fg(TEXT_DIM)),
            ]));
            lines.push(dim(format!(
                "  {}",
                first_fitting_line(detail, w.saturating_sub(2))
            )));
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(format!("{mark} "), Style::default().fg(mark_color)),
            Span::styled(format!("{name}  "), Style::default().fg(TEXT)),
            Span::styled(
                first_fitting_line(status, 16),
                Style::default().fg(TEXT_DIM),
            ),
            Span::styled(
                format!("  {}", first_fitting_line(detail, w.saturating_sub(24))),
                Style::default().fg(TEXT_DIM),
            ),
        ]));
    }
    if !compact(area) {
        lines.push(Line::from(""));
        for part in wrap_or_drop(
            "Config: ~/.cortex/mcp.json — servers inherit the sandbox network policy.",
            w,
        ) {
            lines.push(dim(part));
        }
    }
    let body_h = (lines.len() as u16).min(area.height.saturating_sub(3));
    Paragraph::new(lines).render(Rect::new(area.x, area.y, area.width, body_h), buf);
    paint_hints_and_footer(
        area,
        buf,
        area.y + body_h,
        "↵ details · r reconnect · a add server · esc close",
        &format!("{MODEL} · Agent"),
    );
}

fn board_usage(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_turn_lines("/usage", area);
    lines.push(Line::from(vec![
        Span::styled(
            "Cortex Pro",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", Style::default().fg(TEXT_DIM)),
        Span::styled("renews Sep 28", Style::default().fg(TEXT_DIM)),
    ]));
    if !compact(area) {
        lines.push(Line::from(""));
    }
    let rows = [
        (
            "Agent requests",
            bar(412, 500),
            "412 / 500",
            "resets in 6d 4h",
        ),
        ("Tokens this month", bar(84, 120), "8.4M / 12M", ""),
        ("Cloud agent minutes", bar(132, 400), "132 / 400", ""),
    ];
    for (label, blocks, nums, extra) in rows {
        if compact(area) && label != "Agent requests" {
            continue;
        }
        lines.push(white(first_fitting_line(label, w)));
        let mut row = format!("{blocks}  {nums}");
        if !extra.is_empty() && w > 40 {
            row.push_str("    ");
            row.push_str(extra);
        }
        lines.push(Line::from(Span::styled(
            first_fitting_line(&row, w),
            Style::default().fg(TEXT),
        )));
    }
    if !compact(area) {
        lines.push(Line::from(""));
        for part in wrap_or_drop(
            "MAX mode bills by token instead of per request — manage at cortex.foundation/billing",
            w,
        ) {
            lines.push(dim(part));
        }
    }
    paint_session(area, buf, lines, &format!("{MODEL} · Agent"), GHOST_IDLE);
}

fn board_quota(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_turn_lines("Now add the same limiter to the embeddings endpoint", area);
    if !compact(area) {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(vec![
        Span::styled(
            "x ",
            Style::default().fg(ERROR).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            crate::ui::consts::QUOTA_EXHAUSTED,
            Style::default().fg(ERROR).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(white(first_fitting_line("Agent requests", w)));
    lines.push(Line::from(Span::styled(
        first_fitting_line(&format!("{}  500 / 500", bar(500, 500)), w),
        Style::default().fg(TEXT),
    )));
    for part in wrap_or_drop(
        "Resets in 6d 4h (Sep 7, 16:02). Your work so far is saved in this session.",
        w,
    ) {
        lines.push(dim(part));
    }
    if !compact(area) {
        for part in wrap_or_drop(
            "Switch to MAX token billing to continue now, or upgrade at cortex.foundation/billing",
            w,
        ) {
            lines.push(dim(part));
        }
        lines.push(dim(trim_dangling_separator(&first_fitting_line(
            "/usage details · /model switch to MAX · esc dismiss",
            w,
        ))));
    }
    // The composer ghost keeps its full meaning at 40 columns.
    let ghost = if compact(area) {
        crate::ui::consts::PLACEHOLDER_QUOTA_NARROW
    } else {
        crate::ui::consts::PLACEHOLDER_QUOTA
    };
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 71% context"),
        ghost,
    );
}

fn board_sandbox(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    paint_command_turn(area, buf, y, "/sandbox");
    y += if compact(area) { 1 } else { 2 };

    let limit = area.bottom().saturating_sub(2);
    y += picker_option(area, buf, y, limit, true, None, "Sandbox mode", "", "");
    // `✓ On` sits at the right edge of the selection bar — the check is the
    // only green.
    let on_x = area.right().saturating_sub(5);
    if on_x > area.x + 16 {
        buf.set_string(
            on_x,
            y - 1,
            "✓",
            Style::default().fg(SUCCESS).bg(SELECTION_BG),
        );
        buf.set_string(
            on_x + 2,
            y - 1,
            "On",
            Style::default().fg(TEXT).bg(SELECTION_BG),
        );
    }

    let details = [
        (
            "Filesystem",
            "read/write inside ~/cortex-api · read-only elsewhere",
        ),
        ("Network", "allowlist · registry.npmjs.org, github.com"),
        ("Escalation", "ask before running outside the sandbox"),
    ];
    let mut dy = y + 1;
    for (label, value) in details {
        // Values wrap whole at 40 columns instead of stopping at a dangling
        // `·`; a section that cannot fit entirely is left out, never cut.
        let parts = wrap_or_drop(value, w);
        if dy + 1 + parts.len() as u16 > limit {
            break;
        }
        buf.set_string(
            area.x,
            dy,
            first_fitting_line(label, w),
            Style::default().fg(TEXT),
        );
        dy += 1;
        for part in parts {
            buf.set_string(area.x, dy, &part, Style::default().fg(TEXT_DIM));
            dy += 1;
        }
    }
    if !compact(area) {
        dy += 1;
        for part in wrap_or_drop(
            "Commands run in an isolated container with the repo mounted. Anything that needs to leave the sandbox — networking, global installs — asks first.",
            w,
        ) {
            if dy >= limit.saturating_sub(1) {
                break;
            }
            buf.set_string(area.x, dy, &part, Style::default().fg(TEXT_DIM));
            dy += 1;
        }
    }
    paint_hints_and_footer(
        area,
        buf,
        dy,
        "↑↓ select · space toggle · a add domain · esc close",
        &format!("{MODEL} · Agent · Smart"),
    );
}

fn board_cloud(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_turn_lines("& fix the flaky rateLimit integration test on CI", area);
    lines.push(Line::from(vec![
        Span::styled("↑ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            "Handed off to Cortex Cloud",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
    ]));
    for text in [
        "agent    bc-4f2a · started from main @ 9d31c4e",
        "branch   cortex/fix-flaky-ratelimit",
        "follow   cortex.foundation/agents/bc-4f2a · or /jobs right here",
        "Your terminal is free — the cloud agent pushes commits as it goes.",
    ] {
        for part in wrap_or_drop(text, w) {
            lines.push(dim(part));
        }
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 97% context"),
        GHOST_IDLE,
    );
}

// ---------------------------------------------------------------------------
// Boards 21–30
// ---------------------------------------------------------------------------

fn board_sudo(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    paint_command_turn(
        area,
        buf,
        y,
        "Something is already bound to 6379 - find it and free the port",
    );
    y += if compact(area) { 1 } else { 2 };
    buf.set_string(
        area.x,
        y,
        first_fitting_line("● Shell sudo lsof -i :6379 -sTCP:LISTEN", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    if let Some(cell) = buf.cell_mut((area.x, y)) {
        cell.set_style(Style::default().fg(TEXT));
    }
    y += 1;
    let limit = area.bottom().saturating_sub(COMPOSER_ROWS + 1);
    y = paint_wrapped(
        buf,
        area,
        area.x,
        y,
        limit.saturating_sub(2),
        "needs elevated privileges — password goes straight to sudo",
        Style::default().fg(TEXT_DIM),
    );
    fill_row(buf, area, y, COMMAND_BG);
    let pw = first_fitting_line("Password for mathis: ••••••••", w.saturating_sub(1));
    buf.set_string(
        area.x,
        y,
        format!("{pw}█"),
        Style::default().fg(TEXT).bg(COMMAND_BG),
    );
    y += 1;
    for part in wrap_or_drop(
        "Never stored, logged, or shown to the model. esc cancels the command instead.",
        w,
    ) {
        if y >= limit {
            break;
        }
        buf.set_string(area.x, y, &part, Style::default().fg(TEXT_DIM));
        y += 1;
    }
    paint_composer(area, buf, y.min(limit), Composer::Ghost(GHOST_RUNNING));
    paint_footer(area, buf, &format!("{MODEL} · Agent · Smart · 89% context"));
}

fn board_ask(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = Vec::new();
    let badge = "Ask — read-only";
    let rest = "Cortex will not edit files or run commands. shift+tab to switch.";
    // The badge closes with `┐ ` — its own trailing space — so the sentence
    // never smashes into the bracket. At 40 columns the sentence moves whole
    // onto the rows below the badge.
    let mut badge_spans = vec![
        Span::styled("┌ ", Style::default().fg(TEXT_DIM)),
        Span::styled(badge.to_string(), Style::default().fg(TEXT)),
        Span::styled(" ┐ ", Style::default().fg(TEXT_DIM)),
    ];
    if compact(area) {
        lines.push(Line::from(badge_spans));
        for part in wrap_or_drop(rest, w) {
            lines.push(dim(part));
        }
    } else {
        badge_spans.push(Span::styled(
            first_fitting_line(rest, w.saturating_sub(badge.len() + 5)),
            Style::default().fg(TEXT_DIM),
        ));
        lines.push(Line::from(badge_spans));
        lines.push(Line::from(""));
    }
    lines.extend(user_turn_lines(
        "How does token counting work for streamed completions?",
        area,
    ));
    for part in wrap_or_drop(
        "Streamed completions estimate tokens up front, then reconcile when the stream ends.",
        w,
    ) {
        lines.push(white(part));
    }
    if compact(area) {
        lines.push(Line::from(vec![
            Span::styled("See ", Style::default().fg(TEXT_DIM)),
            Span::styled("src/lib/tokens.ts", Style::default().fg(TEXT)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("See ", Style::default().fg(TEXT_DIM)),
            Span::styled("src/lib/tokens.ts", Style::default().fg(TEXT)),
            Span::styled(" for the implementation.", Style::default().fg(TEXT_DIM)),
        ]));
        for (n, ident, detail) in [
            (
                "1. ",
                "estimateTokens(prompt)",
                "counts the request before the stream.",
            ),
            (
                "2. ",
                "usage.completion_tokens",
                "arrives on the final chunk.",
            ),
            ("3. ", "reconcileUsage()", "corrects the running total."),
        ] {
            // Each code span carries its own trailing space.
            lines.push(Line::from(vec![
                Span::styled(n.to_string(), Style::default().fg(TEXT_DIM)),
                Span::styled(
                    format!("{ident} "),
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    first_fitting_line(detail, w.saturating_sub(n.len() + ident.len() + 1)),
                    Style::default().fg(TEXT_DIM),
                ),
            ]));
        }
        lines.push(Line::from(""));
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Ask · 96% context"),
        "Reply, or shift+tab for Agent mode",
    );
}

fn board_files(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let typed = "Add integration tests for @rate";
    let mut y = area.y;
    y += paint_composer(area, buf, y, Composer::Typed(typed));

    let rows = [
        (true, "src/middleware/rateLimit.ts", "edited 2m ago"),
        (false, "test/rateLimit.test.ts", "edited 5m ago"),
        (false, "src/config/rateLimits.json", "2 days ago"),
        (false, "docs/rate-limiting.md", "last week"),
    ];
    let limit = area.bottom().saturating_sub(2);
    for (selected, path, when) in rows {
        if y >= limit {
            break;
        }
        // The full filename always wins: when the row cannot hold both, the
        // timestamp is dropped rather than ever cutting inside the name.
        let when_fit = first_fitting_line(when, 16);
        let show_when = 2 + path.chars().count() + 2 + when_fit.chars().count() <= w;
        let path_budget = if show_when {
            w.saturating_sub(when_fit.chars().count() + 4)
        } else {
            w.saturating_sub(2)
        };
        if selected {
            fill_row(buf, area, y, SELECTION_BG);
        }
        let mut x = area.x;
        let (marker, marker_style) = if selected {
            ("> ", Style::default().fg(ACCENT).bg(SELECTION_BG))
        } else {
            ("· ", Style::default().fg(TEXT_DIM))
        };
        buf.set_string(x, y, marker, marker_style);
        x = x.saturating_add(2);
        let mut spans = paint_match_path(path, "rate");
        let mut used = 0usize;
        for span in &mut spans {
            if selected {
                span.style = span.style.fg(ACCENT).bg(SELECTION_BG);
            }
            let content = span.content.to_string();
            let take = first_fitting_line(&content, path_budget.saturating_sub(used));
            if take.is_empty() {
                break;
            }
            buf.set_string(x, y, &take, span.style);
            x = x.saturating_add(take.chars().count() as u16);
            used += take.chars().count();
        }
        if show_when {
            let rx = area
                .right()
                .saturating_sub(when_fit.chars().count() as u16 + 1)
                .max(x.saturating_add(1));
            let when_style = if selected {
                Style::default().fg(TEXT_DIM).bg(SELECTION_BG)
            } else {
                Style::default().fg(TEXT_DIM)
            };
            buf.set_string(rx, y, &when_fit, when_style);
        }
        y += 1;
    }
    paint_hints_and_footer(
        area,
        buf,
        y,
        "↑↓ select · ↵ insert · tab complete · esc dismiss",
        &format!("{MODEL} · Agent · 100% context"),
    );
}

fn board_queue(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    // Narrow rows keep the whole thought: the path shortens rather than the
    // diff stats falling off the end. The in-progress Edit is the Edit tile
    // (state 10) verbatim — `● Edit <path> +58 -0` — with `+58` in the diff
    // green, exactly like `+9`, the MAX footer's `+214` and Write's `+84`.
    let path = if compact(area) {
        "rateLimit.ts"
    } else {
        "src/middleware/rateLimit.ts"
    };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(TEXT)),
        Span::styled(
            "Edit ",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            first_fitting_line(path, w.saturating_sub(14)),
            Style::default().fg(TEXT),
        ),
        Span::styled(" +58", Style::default().fg(DIFF_ADD)),
        Span::styled(" -0", Style::default().fg(TEXT_DIM)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("⁝ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line("Writing integration tests...", w.saturating_sub(2)),
            Style::default().fg(TEXT),
        ),
    ]));
    if !compact(area) {
        lines.push(dim(first_fitting_line(
            "1m 42s · 12.6k tokens · ctrl+c to stop",
            w,
        )));
        lines.push(Line::from(""));
    }
    for part in wrap_or_drop("Queued · 2 — sent when the current step finishes", w) {
        lines.push(dim(part));
    }
    let queued: [&str; 2] = if compact(area) {
        [
            "1. Send a Retry-After header on 429s",
            "2. Update docs/rate-limiting.md",
        ]
    } else {
        [
            "1. Also send a Retry-After header on 429 responses",
            "2. Update docs/rate-limiting.md with the new limits",
        ]
    };
    for item in queued {
        lines.push(dim(first_fitting_line(item, w)));
    }
    if !compact(area) {
        lines.push(dim(first_fitting_line(
            "↑ edit queued · ctrl+x clear queue · ctrl+c stop",
            w,
        )));
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 76% context"),
        "Add a follow-up — ⏎ to queue",
    );
}

fn board_jobs(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    paint_command_turn(area, buf, area.y, "/jobs");
    buf.set_string(
        area.x,
        area.y + 1,
        first_fitting_line("Agents & jobs · 2 running", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );

    struct Job {
        selected: bool,
        icon: &'static str,
        icon_color: Color,
        kind: &'static str,
        title: &'static str,
        status: &'static str,
        meta: &'static str,
    }
    // Spinners and status words stay gray; the green `✓` marks the finished
    // job.
    let jobs = [
        Job {
            selected: true,
            icon: "⠇",
            icon_color: TEXT_DIM,
            kind: "cloud",
            title: "Fix flaky rateLimit test on CI",
            status: "running",
            meta: "4m · cortex/fix-flaky-ratelimit",
        },
        Job {
            selected: false,
            icon: "⠇",
            icon_color: TEXT_DIM,
            kind: "subagent",
            title: "Docs sweep — rate limits + 429 examples",
            status: "running",
            meta: "1m · docs/rate-limiting.md",
        },
        Job {
            selected: false,
            icon: "✓",
            icon_color: SUCCESS,
            kind: "subagent",
            title: "Typecheck all packages",
            status: "done",
            meta: "finished 2m ago · 0 errors",
        },
        Job {
            selected: false,
            icon: "x",
            icon_color: TEXT,
            kind: "cloud",
            title: "Bump ioredis 5 → 6",
            status: "failed",
            meta: "18m ago · 3 tests failing",
        },
    ];
    let mut y = area.y + if compact(area) { 2 } else { 3 };
    let limit = area.bottom().saturating_sub(2);
    for job in jobs {
        if y + 1 >= limit {
            break;
        }
        if job.selected {
            fill_row(buf, area, y, SELECTION_BG);
            fill_row(buf, area, y + 1, SELECTION_BG);
        }
        let bg = if job.selected {
            SELECTION_BG
        } else {
            Color::Reset
        };
        let (marker, marker_style) = if job.selected {
            ("> ", Style::default().fg(ACCENT).bg(bg))
        } else {
            ("· ", Style::default().fg(TEXT_DIM))
        };
        buf.set_string(area.x, y, marker, marker_style);
        buf.set_string(
            area.x + 2,
            y,
            job.icon,
            Style::default().fg(job.icon_color).bg(bg),
        );
        let title_style = if job.selected {
            Style::default()
                .fg(ACCENT)
                .bg(bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        buf.set_string(
            area.x + 4,
            y,
            ellipsis_fit_line(
                &format!("{}  {}", job.kind, job.title),
                w.saturating_sub(14),
            ),
            title_style,
        );
        let st = job.status;
        let sx = area.right().saturating_sub(st.len() as u16 + 1);
        buf.set_string(sx, y, st, Style::default().fg(TEXT_DIM).bg(bg));
        buf.set_string(
            area.x + 4,
            y + 1,
            first_fitting_line(job.meta, w.saturating_sub(4)),
            Style::default().fg(TEXT_DIM).bg(bg),
        );
        y += 2;
    }
    paint_hints_and_footer(
        area,
        buf,
        y,
        "⏎ open · a attach · x cancel job · esc close",
        &format!("{MODEL} · Agent"),
    );
}

fn board_help(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    paint_command_turn(area, buf, area.y, "/help");
    buf.set_string(
        area.x,
        area.y + 1,
        first_fitting_line("Commands", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );

    const HELP: [(&str, &str); 20] = [
        ("/model", "Choose the model for this session"),
        ("/mode", "Switch between Agent, Plan and Ask"),
        ("/permissions", "Set the approval policy for edits"),
        ("/plan", "Draft a plan before writing any code"),
        ("/effort", "Tune reasoning effort for the model"),
        ("/mcp", "View and manage MCP servers"),
        ("/sandbox", "Configure sandboxed command execution"),
        ("/usage", "Plan usage, quota and limits"),
        ("/resume", "Resume a previous session"),
        ("/jobs", "Background agents and subagents"),
        ("/skills", "Run a skill, or pin one as always-on"),
        ("/btw", "Ask a side question without changing the plan"),
        ("/compact", "Summarize the thread to free context"),
        ("/clear", "Start a new thread, keep the workspace"),
        ("/diff", "Review the files Cortex has changed"),
        ("/copy", "Copy the last reply"),
        ("/config", "Open the configuration editor"),
        ("/login", "Sign in or switch accounts"),
        ("/logout", "Sign out of this machine"),
        ("/settings", "Model, mode, permissions, sandbox"),
    ];

    let two_col = w >= 72;
    let compact_help = compact(area);
    let mut y = area.y + 2;
    let reserve = if compact_help { 4 } else { 8 };
    if two_col {
        // Command white, description dim — same tone as the slash palette.
        let col_w = w / 2;
        fn paint_help(buf: &mut Buffer, x: u16, y: u16, cmd: &str, desc: &str, budget: usize) {
            buf.set_string(x, y, cmd, Style::default().fg(TEXT));
            let fitted = ellipsis_fit(desc, budget.saturating_sub(cmd.len() + 2));
            if !fitted.is_empty() {
                buf.set_string(
                    x + cmd.len() as u16 + 2,
                    y,
                    fitted,
                    Style::default().fg(TEXT_DIM),
                );
            }
        }
        for i in 0..10 {
            if y >= area.bottom().saturating_sub(reserve) {
                break;
            }
            let (lcmd, ldesc) = HELP[i];
            let (rcmd, rdesc) = HELP[i + 10];
            paint_help(buf, area.x, y, lcmd, ldesc, col_w.saturating_sub(1));
            paint_help(buf, area.x + col_w as u16, y, rcmd, rdesc, col_w);
            y += 1;
        }
    } else {
        // Narrow: as many one-line `cmd  description` rows as fit, with an
        // ellipsis rather than a mid-word cut.
        let shown = if compact_help { &HELP[..5] } else { &HELP[..] };
        for (cmd, desc) in shown {
            if y >= area.bottom().saturating_sub(reserve) {
                break;
            }
            if compact_help {
                buf.set_string(area.x, y, cmd, Style::default().fg(TEXT));
                let fitted = ellipsis_fit(desc, w.saturating_sub(cmd.len() + 2));
                if !fitted.is_empty() {
                    buf.set_string(
                        area.x + cmd.len() as u16 + 2,
                        y,
                        fitted,
                        Style::default().fg(TEXT_DIM),
                    );
                }
                y += 1;
                continue;
            }
            buf.set_string(
                area.x,
                y,
                first_fitting_line(cmd, w),
                Style::default().fg(TEXT),
            );
            y += 1;
            if y >= area.bottom().saturating_sub(reserve) {
                break;
            }
            buf.set_string(
                area.x,
                y,
                ellipsis_fit(desc, w),
                Style::default().fg(TEXT_DIM),
            );
            y += 1;
        }
    }

    if !compact_help {
        y += 1;
    }
    if y < area.bottom().saturating_sub(3) {
        buf.set_string(
            area.x,
            y,
            first_fitting_line("Shortcuts", w),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        );
        y += 1;
    }
    let shortcuts = if compact(area) {
        [
            "shift+tab  cycle Agent / Plan / Ask",
            "@  mention files",
            "",
            "",
            "",
            "",
            "",
            "",
        ]
    } else {
        [
            "shift+tab  cycle Agent / Plan / Ask",
            "@  mention files",
            "!  bash mode — run a command in your shell",
            "&  hand the task to a cloud agent",
            "ctrl+p  command palette",
            "ctrl+c  stop the current run",
            "ctrl+r  search past sessions",
            "↵ while working  queue a follow-up",
        ]
    };
    for item in shortcuts {
        if item.is_empty() {
            continue;
        }
        if y >= area.bottom().saturating_sub(2) {
            break;
        }
        buf.set_string(
            area.x,
            y,
            first_fitting_line(item, w),
            Style::default().fg(TEXT_DIM),
        );
        y += 1;
    }
    paint_hints_and_footer(
        area,
        buf,
        y,
        &trim_dangling_separator(&first_fitting_line(
            "Docs & guides: cortex.foundation/docs · Cortex CLI v1.0.0",
            w,
        )),
        &format!("{MODEL} · Agent"),
    );
}

/// The three first-run tips: `(lead, code, rest)` — the code span sits on
/// its own lighter chip inside the panel, padded by one space each side.
const FIRST_RUN_TIPS: [(&str, &str, &str); 3] = [
    (
        "1. Use",
        "/model",
        "to switch between models and adjust reasoning effort.",
    ),
    ("2. Add", "@", "files to give Cortex CLI the right context."),
    (
        "3. Press",
        "shift+tab",
        "anytime to cycle modes and view available options.",
    ),
];

/// The same tips, short enough for a 40-column panel.
const FIRST_RUN_TIPS_NARROW: [(&str, &str, &str); 3] = [
    ("1. Use", "/model", "to switch models."),
    ("2. Add", "@", "files for context."),
    ("3.", "shift+tab", "cycles modes."),
];

/// Title of the first-run tips panel, and its narrow form.
pub const FIRST_RUN_TITLE: &str = "A few tips to get the most out of this tool:";
const FIRST_RUN_TITLE_NARROW: &str = "A few tips to get started:";

fn board_first_run(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let narrow = compact(area);
    let mut y = area.y;
    // Splash: a dim dot-grid mark beside the product name, or the one-line
    // splash when narrow.
    if narrow {
        buf.set_string(
            area.x,
            y,
            first_fitting_line("Cortex CLI v1.0.0", w),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        );
        y += 1;
    } else {
        let mark = "· · · · ·";
        buf.set_string(area.x, y, mark, Style::default().fg(TEXT_DIM));
        buf.set_string(
            area.x + 11,
            y,
            "Cortex CLI",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        );
        y += 1;
        buf.set_string(area.x, y, mark, Style::default().fg(TEXT_DIM));
        buf.set_string(area.x + 11, y, "v1.0.0", Style::default().fg(TEXT_DIM));
        y += 2;
    }

    // Tips panel: filled charcoal, one column of padding, the code spans on
    // a slightly lighter chip.
    let chip = Style::default().fg(TEXT).bg(SURFACE_2);
    let mut panel: Vec<Line<'_>> = Vec::new();
    if !narrow {
        panel.push(Line::from(""));
    }
    let title = if narrow {
        FIRST_RUN_TITLE_NARROW
    } else {
        FIRST_RUN_TITLE
    };
    panel.push(Line::from(Span::styled(
        first_fitting_line(title, w.saturating_sub(2)),
        Style::default().fg(TEXT),
    )));
    if !narrow {
        panel.push(Line::from(""));
    }
    let tips = if narrow {
        FIRST_RUN_TIPS_NARROW
    } else {
        FIRST_RUN_TIPS
    };
    for (lead, code, rest) in tips {
        let budget = w.saturating_sub(2 + lead.chars().count() + code.chars().count() + 2);
        panel.push(Line::from(vec![
            Span::styled(lead, Style::default().fg(TEXT)),
            Span::styled(format!(" {code} "), chip),
            Span::styled(first_fitting_line(rest, budget), Style::default().fg(TEXT)),
        ]));
        if !narrow {
            panel.push(Line::from(""));
        }
    }
    if !narrow {
        panel.pop();
        panel.push(Line::from(""));
    }
    let limit = area.bottom().saturating_sub(COMPOSER_ROWS + 2);
    y += paint_panel(area, buf, y, limit, &panel);
    if !narrow {
        y += 1;
    }
    if y < limit {
        buf.set_string(
            area.x,
            y,
            first_fitting_line("Cortex Pro · 100% remaining", w),
            Style::default().fg(TEXT_DIM),
        );
        y += 1;
    }
    if !narrow {
        y += 1;
    }
    paint_composer(area, buf, y, Composer::Ghost(GHOST_IDLE));
    paint_footer(area, buf, &format!("{MODEL} · Agent · 100% context"));
}

fn board_bash(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    buf.set_string(
        area.x,
        area.y,
        first_fitting_line("┌ Bash mode ┐", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    let mut y = area.y + 1;
    for part in wrap_or_drop(
        "Commands run directly in your shell — the model is not involved. esc to exit.",
        w,
    ) {
        buf.set_string(area.x, y, &part, Style::default().fg(TEXT_DIM));
        y += 1;
        if compact(area) && y > area.y + 2 {
            break;
        }
    }
    if !compact(area) {
        y += 1;
    }
    fill_row(buf, area, y, USER_TURN_BG);
    buf.set_string(
        area.x,
        y,
        first_fitting_line("! redis-cli PING", w),
        Style::default().fg(TEXT).bg(USER_TURN_BG),
    );
    y += 1;
    buf.set_string(area.x, y, "PONG", Style::default().fg(TEXT_DIM));
    y += if compact(area) { 1 } else { 2 };
    // The typed command wraps whole at 40 columns — the cursor is never lost.
    let limit = area.bottom().saturating_sub(2);
    paint_hairline(area, buf, y);
    y += 1;
    for (i, part) in wrap_or_drop("! npm run test:integration -- --grep rateLimit█", w)
        .into_iter()
        .enumerate()
    {
        if y >= limit {
            break;
        }
        let x = if i == 0 { area.x } else { area.x + 2 };
        buf.set_string(x, y, &part, Style::default().fg(TEXT));
        y += 1;
    }
    paint_hairline(area, buf, y);
    paint_hints_and_footer(
        area,
        buf,
        y,
        "↵ run · ↑↓ shell history · esc back to Cortex",
        &format!("{MODEL} · Agent · 94% context"),
    );
}

fn board_config(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    paint_command_turn(area, buf, area.y, "/config");
    buf.set_string(
        area.x,
        area.y + 1,
        first_fitting_line("Config · ~/.cortex/config.json", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );

    // Narrow rows keep a whole value rather than a cut one.
    let filesystem = if compact(area) {
        "read/write"
    } else {
        "Workspace read/write"
    };
    let rows = [
        (true, false, "model", "Cortex Mini 1 · Max · Fast Mode On"),
        (false, false, "permissions", "Smart"),
        (false, false, "sandbox", "Enabled"),
        (false, true, "network", "Allowlist · 2 domains"),
        (false, true, "filesystem", filesystem),
        (false, false, "editor", "zed --wait"),
        (false, false, "theme", "Cortex Dark"),
        (false, false, "notifications", "On finish + on approval"),
        (false, false, "telemetry", "Off"),
    ];
    let mut y = area.y + if compact(area) { 2 } else { 3 };
    let last_idx = rows.len() - 1;
    let limit = area
        .bottom()
        .saturating_sub(if compact(area) { 2 } else { 4 });
    for (i, (selected, child, key, value)) in rows.iter().enumerate() {
        if y >= limit {
            break;
        }
        let branch = if *child {
            if i == last_idx {
                "└── "
            } else {
                "├── "
            }
        } else if i == last_idx {
            "└── "
        } else {
            "├── "
        };
        let prefix = if *child { "│   " } else { "" };
        let label = format!("{prefix}{branch}{key}");
        let value_fit = first_fitting_line(value, w.saturating_sub(label.chars().count() + 4));
        if *selected {
            fill_row(buf, area, y, SELECTION_BG);
            buf.set_string(
                area.x,
                y,
                "> ",
                Style::default().fg(ACCENT).bg(SELECTION_BG),
            );
            buf.set_string(
                area.x + 2,
                y,
                &label,
                Style::default()
                    .fg(ACCENT)
                    .bg(SELECTION_BG)
                    .add_modifier(Modifier::BOLD),
            );
            // The selected value keeps its column gap and never ends on a
            // dangling `·` when the `⏎ edit` affordance takes the right edge.
            let shown = trim_dangling_separator(&fit_line(
                &value_fit,
                w.saturating_sub(label.chars().count() + 12),
            ));
            buf.set_string(
                area.x + 2 + label.chars().count() as u16 + 2,
                y,
                shown,
                Style::default().fg(TEXT_DIM).bg(SELECTION_BG),
            );
            buf.set_string(
                area.right().saturating_sub(7),
                y,
                "⏎ edit",
                Style::default().fg(TEXT_DIM).bg(SELECTION_BG),
            );
        } else {
            buf.set_string(area.x, y, "· ", Style::default().fg(TEXT_DIM));
            buf.set_string(area.x + 2, y, &label, Style::default().fg(TEXT));
            buf.set_string(
                area.x + 2 + label.chars().count() as u16 + 2,
                y,
                value_fit,
                Style::default().fg(TEXT_DIM),
            );
        }
        y += 1;
    }
    if !compact(area) {
        // The override note wraps whole at 120 columns.
        y += 1;
        y = paint_wrapped(
            buf,
            area,
            area.x,
            y,
            area.bottom().saturating_sub(2),
            "Project overrides in .cortex/config.json win over the global file.",
            Style::default().fg(TEXT_DIM),
        );
    }
    let hints_y = y.saturating_add(1).min(area.bottom().saturating_sub(2));
    buf.set_string(
        area.x,
        hints_y,
        trim_dangling_separator(&first_fitting_line(
            "↑↓ navigate · ↵ edit · r reset to default · esc close",
            w,
        )),
        Style::default().fg(TEXT_DIM),
    );
    paint_max_footer(area, buf, " · Agent", "");
}

/// Footer with the bold `MAX` badge on the left — `Cortex Mini 1 · MAX
/// {suffix}` — and the shortcut hint on the right. The model name is always
/// shown: when the full left side cannot fit beside the hint, the hint goes
/// first, then the suffix.
fn paint_max_footer(area: Rect, buf: &mut Buffer, suffix: &str, hint: &str) {
    let w = area.width as usize;
    let fy = area.bottom().saturating_sub(1);
    let prefix = format!("{MODEL} · ");
    let badge = "MAX";
    let left_len = |suffix: &str| prefix.chars().count() + badge.len() + suffix.chars().count();
    let fits = |suffix: &str, hint: &str| {
        let hint_len = if hint.is_empty() {
            0
        } else {
            hint.chars().count() + 1
        };
        left_len(suffix) + hint_len <= w
    };
    let mut hint_fit = hint;
    let mut suffix_fit = suffix;
    if !fits(suffix_fit, hint_fit) {
        hint_fit = FOOTER_HINT_SHORT;
        if hint.is_empty() {
            hint_fit = "";
        }
    }
    if !fits(suffix_fit, hint_fit) {
        hint_fit = "";
    }
    if !fits(suffix_fit, hint_fit) {
        suffix_fit = "";
    }
    if !fits(suffix_fit, hint_fit) {
        return;
    }
    let dim = Style::default().fg(TEXT_DIM);
    buf.set_string(area.x, fy, &prefix, dim);
    buf.set_string(
        area.x + prefix.chars().count() as u16,
        fy,
        badge,
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    if !suffix_fit.is_empty() {
        buf.set_string(
            area.x + prefix.chars().count() as u16 + badge.len() as u16,
            fy,
            suffix_fit,
            dim,
        );
    }
    if !hint_fit.is_empty() {
        let rx = area.right().saturating_sub(hint_fit.chars().count() as u16);
        buf.set_string(rx, fy, hint_fit, dim);
    }
}

fn board_footer_max(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_turn_lines("Ship it — commit and push the rate limiter", area);
    // Every line keeps its whole meaning at 40 columns: the command, the
    // diff stats, the commit subject and the branch note wrap instead of
    // stopping at a fragment.
    let command = "git add -A && git commit && git push -u origin rate-limit-9e4d";
    for (i, part) in wrap_or_drop(command, w.saturating_sub(8))
        .into_iter()
        .enumerate()
    {
        if i == 0 {
            lines.push(Line::from(vec![
                Span::styled("● ", Style::default().fg(TEXT)),
                Span::styled(
                    "Shell ",
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(part, Style::default().fg(TEXT)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw("        "),
                Span::styled(part, Style::default().fg(TEXT)),
            ]));
        }
    }
    let stats = vec![
        Span::styled("+214", Style::default().fg(DIFF_ADD)),
        Span::styled(" ", Style::default().fg(TEXT_DIM)),
        Span::styled("-9", Style::default().fg(TEXT_DIM)),
    ];
    if compact(area) {
        lines.push(Line::from(vec![
            Span::styled("✓ ", Style::default().fg(SUCCESS)),
            Span::styled("Committed and pushed", Style::default().fg(TEXT)),
        ]));
        let mut row = vec![Span::styled("  3 files · ", Style::default().fg(TEXT))];
        row.extend(stats);
        lines.push(Line::from(row));
    } else {
        let mut row = vec![
            Span::styled("✓ ", Style::default().fg(SUCCESS)),
            Span::styled(
                "Committed and pushed · 3 files · ",
                Style::default().fg(TEXT),
            ),
        ];
        row.extend(stats);
        lines.push(Line::from(row));
    }
    for text in [
        "a4f21c9 · Add Redis sliding-window rate limiting to /v1/completions",
        "branch rate-limit-9e4d -> origin · open a PR with /pr",
    ] {
        for part in wrap_or_drop(text, w) {
            lines.push(dim(part));
        }
    }
    let footer_h = 1u16;
    let max_body = area.height.saturating_sub(footer_h + COMPOSER_ROWS);
    let body_h = (lines.len() as u16).min(max_body);
    Paragraph::new(lines).render(Rect::new(area.x, area.y, area.width, body_h), buf);
    paint_composer(
        area,
        buf,
        area.y + body_h,
        Composer::Typed("Add a follow-up"),
    );
    paint_max_footer(
        area,
        buf,
        " · Agent · Smart · 38% context left",
        FOOTER_HINT,
    );
}

// ---------------------------------------------------------------------------
// Boards 31–40
// ---------------------------------------------------------------------------

fn board_thinking(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    // The Thinking status is the one muted-gold word in the chrome.
    let meta = " · 14s · esc to interrupt";
    let mut status = vec![
        Span::styled("⠇ ", Style::default().fg(TEXT_DIM)),
        Span::styled("Thinking", Style::default().fg(THINKING)),
    ];
    if 10 + meta.chars().count() <= w {
        status.push(Span::styled(meta, Style::default().fg(TEXT_DIM)));
    } else {
        status.push(Span::styled(" · 14s", Style::default().fg(TEXT_DIM)));
    }
    lines.push(Line::from(status));
    for thought in [
        "Need a sliding window, not a fixed counter — bursts at minute boundaries would leak.",
        "ioredis sorted set per API key: ZADD now, ZREMRANGEBYSCORE older than window.",
        "Fail closed if Redis is down — don't let completions through unmetered.",
    ] {
        // Narrow shows the first thought whole rather than three fragments.
        for part in wrap_or_drop(thought, w) {
            lines.push(dim(part));
        }
        if compact(area) {
            break;
        }
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 97% context"),
        GHOST_RUNNING,
    );
}

fn board_todos(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    lines.push(white(first_fitting_line("Working 1/5", w)));
    lines.push(Line::from(vec![
        Span::styled("✓ ", Style::default().fg(SUCCESS)),
        Span::styled(
            first_fitting_line("Add Redis client singleton", w.saturating_sub(2)),
            Style::default().fg(TEXT),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("› ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled(
            first_fitting_line("Write rateLimit middleware", w.saturating_sub(2)),
            Style::default().fg(TEXT),
        ),
    ]));
    let pending = if compact(area) {
        &["Wire into POST /v1/completions", "Env + .env.example"][..]
    } else {
        &[
            "Wire into POST /v1/completions",
            "Env + .env.example",
            "Integration tests with ioredis-mock",
        ][..]
    };
    for item in pending {
        lines.push(Line::from(vec![
            Span::styled("○ ", Style::default().fg(TEXT_DIM)),
            Span::styled(
                first_fitting_line(item, w.saturating_sub(2)),
                Style::default().fg(TEXT_DIM),
            ),
        ]));
    }
    if !compact(area) {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(vec![
        Span::styled("⠋ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line("Writing src/middleware/rateLimit.ts", w.saturating_sub(2)),
            Style::default().fg(TEXT),
        ),
    ]));
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 90% context"),
        GHOST_RUNNING,
    );
}

fn board_question(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    if !compact(area) {
        for line in user_prompt_lines(area) {
            Paragraph::new(vec![line]).render(Rect::new(area.x, y, area.width, 1), buf);
            y += 1;
            if y + 8 >= area.bottom() {
                break;
            }
        }
    }
    buf.set_string(
        area.x,
        y,
        first_fitting_line("Where should the limiter live?", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    y += if compact(area) { 1 } else { 2 };
    let options = [
        (false, "Middleware on POST /v1/completions only"),
        (true, "Shared limiter for every /v1/* route"),
        (false, "Per-model limits, configured in the catalog"),
        (false, "Skip for now - I'll point you at an existing helper"),
    ];
    let limit = area.bottom().saturating_sub(2);
    for (i, (selected, label)) in options.iter().enumerate() {
        // Options wrap whole at 40 columns; the selection bar covers every
        // row of the chosen option.
        let parts = wrap_or_drop(label, w.saturating_sub(4));
        if y + parts.len() as u16 > limit {
            break;
        }
        for (j, part) in parts.iter().enumerate() {
            if j == 0 {
                y += picker_option(area, buf, y, limit, *selected, Some(i + 1), part, "", "");
            } else {
                if *selected {
                    fill_row(buf, area, y, SELECTION_BG);
                }
                let style = if *selected {
                    Style::default()
                        .fg(ACCENT)
                        .bg(SELECTION_BG)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                buf.set_string(area.x + 4, y, part, style);
                y += 1;
            }
        }
    }
    paint_hints_and_footer(
        area,
        buf,
        y,
        "1-9 pick · ↑↓ move · ↵ confirm · esc skip",
        &format!("{MODEL} · Plan"),
    );
}

fn board_skills(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    paint_command_turn(area, buf, y, "/skills");
    y += 1;
    y += paint_search_field(area, buf, y, "", "Type to search skills");
    let rows = [
        (false, "/commit", "Stage, write a message, commit"),
        (true, "/pr", "Open a pull request with summary + test plan"),
        (false, "/review", "Review the current diff like a teammate"),
        (false, "/fix-ci", "Reproduce the failed check and patch it"),
        (false, "/migrate", "Draft a reversible database migration"),
    ];
    let limit = area.bottom().saturating_sub(2);
    for (selected, cmd, desc) in rows {
        if y >= limit {
            break;
        }
        // Every skill lists at both sizes; narrow descriptions end in an
        // ellipsis instead of a mid-word cut, and the column gap survives.
        let desc_fit = ellipsis_fit(desc, w.saturating_sub(cmd.len() + 4));
        if selected {
            fill_row(buf, area, y, SELECTION_BG);
            buf.set_string(
                area.x,
                y,
                "> ",
                Style::default().fg(ACCENT).bg(SELECTION_BG),
            );
            buf.set_string(
                area.x + 2,
                y,
                cmd,
                Style::default()
                    .fg(ACCENT)
                    .bg(SELECTION_BG)
                    .add_modifier(Modifier::BOLD),
            );
            buf.set_string(
                area.x + 2 + cmd.len() as u16 + 2,
                y,
                &desc_fit,
                Style::default().fg(TEXT_DIM).bg(SELECTION_BG),
            );
        } else {
            buf.set_string(area.x, y, "· ", Style::default().fg(TEXT_DIM));
            buf.set_string(area.x + 2, y, cmd, Style::default().fg(TEXT));
            buf.set_string(
                area.x + 2 + cmd.len() as u16 + 2,
                y,
                &desc_fit,
                Style::default().fg(TEXT_DIM),
            );
        }
        y += 1;
    }
    paint_hints_and_footer(
        area,
        buf,
        y,
        "↑↓ select · ↵ run once · ⌥↵ pin as mode · esc close",
        &format!("{MODEL} · Agent"),
    );
}

fn board_btw(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    lines.extend(marker_lines(
        "⠇ ",
        Style::default().fg(TEXT_DIM),
        "Implementing the sliding-window limiter...",
        Style::default().fg(TEXT),
        w,
    ));
    if !compact(area) {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            "btw",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line("Is ioredis already a dependency?", w.saturating_sub(2)),
            Style::default().fg(TEXT),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line(
                "Yes – ioredis@5.4.1 is in dependencies. No install needed.",
                w.saturating_sub(3),
            ),
            Style::default().fg(TEXT),
        ),
        Span::styled("▌", Style::default().fg(TEXT)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line("not added to the main thread", w.saturating_sub(2)),
            Style::default().fg(TEXT_DIM),
        ),
    ]));
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 91% context"),
        GHOST_RUNNING,
    );
}

/// State 37 — the run was interrupted: prompt and tool tiles stay on screen,
/// then `× Stopped` in error red. Shared by the `interrupt` and `stopped` captures.
fn board_stopped(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    let tiles = if compact(area) {
        vec![("completions.ts", " · 141 lines")]
    } else {
        vec![
            ("src/server/routes/completions.ts", " · 141 lines"),
            ("src/middleware/auth.ts", " · 68 lines"),
        ]
    };
    for (path, meta) in tiles {
        lines.push(Line::from(vec![
            Span::styled("● ", Style::default().fg(TEXT)),
            Span::styled(
                "Read ",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                first_fitting_line(path, w.saturating_sub(20)),
                Style::default().fg(TEXT),
            ),
            Span::styled(meta, Style::default().fg(TEXT_DIM)),
        ]));
    }
    if !compact(area) {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(vec![
        Span::styled(
            format!("{} ", crate::ui::consts::STOPPED_MARK),
            Style::default().fg(ERROR),
        ),
        Span::styled(
            crate::ui::consts::STOPPED_TITLE,
            Style::default().fg(ERROR).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(dim(first_fitting_line("12s · 4.1k tokens · ctrl+c", w)));
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 94% context"),
        GHOST_IDLE,
    );
}

/// State 38 — `/compact` result. Shared by the `compact` and `compacted`
/// captures.
fn board_compacted(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_turn_lines("/compact", area);
    if !compact(area) {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        first_fitting_line("Thread compacted", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(vec![
        Span::styled("Context  ", Style::default().fg(TEXT_DIM)),
        Span::styled("86%", Style::default().fg(TEXT_DIM)),
        Span::styled("  →  ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            "12%",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" used", Style::default().fg(TEXT_DIM)),
    ]));
    if compact(area) {
        for part in wrap_or_drop("Summary kept · files and todos are unchanged.", w) {
            lines.push(dim(part));
        }
    } else {
        lines.push(dim(first_fitting_line(
            "Summary kept  ·  2.1k tokens kept  ·  files and todos are unchanged.",
            w,
        )));
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 88% context"),
        GHOST_IDLE,
    );
}

fn board_write(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(TEXT)),
        Span::styled(
            first_fitting_line("Write src/middleware/rateLimit.ts", w.saturating_sub(6)),
            Style::default().fg(TEXT),
        ),
        Span::styled(" +84", Style::default().fg(DIFF_ADD)),
    ]));
    lines.push(dim(first_fitting_line("new file", w)));
    let code: &[(u32, &str)] = &[
        (1, "import Redis from \"ioredis\";"),
        (
            2,
            "import type { FastifyRequest, FastifyReply } from \"fastify\";",
        ),
        (
            4,
            "export function rateLimit(opts: { limit: number; windowSec: number }) {",
        ),
        (5, "  const redis = new Redis(process.env.REDIS_URL);"),
        (
            6,
            "  return async (req: FastifyRequest, reply: FastifyReply) => {",
        ),
        (7, "    const key = `rl:${opts.keyOf(req)}`;"),
    ];
    // Whole numbered lines only, as many as the body has rows for.
    let body_rows = area.height.saturating_sub(1 + COMPOSER_ROWS) as usize;
    for (no, line) in code {
        let hit = grep_hit_line(w, *no, line);
        if lines.len() + hit.len() > body_rows {
            break;
        }
        lines.extend(hit);
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 86% context"),
        GHOST_RUNNING,
    );
}

fn board_clear_confirm(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    paint_command_turn(area, buf, y, "/clear");
    y += if compact(area) { 1 } else { 2 };
    buf.set_string(
        area.x,
        y,
        first_fitting_line("Start a new thread?", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    y += 1;
    for part in wrap_or_drop(
        "The transcript is dropped. Git, files and config stay as they are.",
        w,
    ) {
        buf.set_string(area.x, y, &part, Style::default().fg(TEXT_DIM));
        y += 1;
    }
    y += 1;
    let limit = area.bottom().saturating_sub(2);
    y += picker_option(area, buf, y, limit, true, Some(1), "Clear thread", "", "");
    y += picker_option(area, buf, y, limit, false, Some(2), "Cancel", "", "");
    paint_hints_and_footer(
        area,
        buf,
        y,
        "↑↓ select · ↵ confirm · esc cancel",
        &format!("{MODEL} · Agent"),
    );
}

// ---------------------------------------------------------------------------
// Boards 41–50
// ---------------------------------------------------------------------------

fn board_grep(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(TEXT)),
        Span::styled(
            first_fitting_line("Grep rateLimit src 4 hits", w.saturating_sub(2)),
            Style::default().fg(TEXT),
        ),
    ]));
    let hits: &[(u32, &str)] = &[
        (18, "import { rateLimit } from \"../middleware/rateLimit\";"),
        (24, "{ preHandler: [requireApiKey, limiter] },"),
        (41, "export function rateLimit(opts: RateLimitOpts) {"),
        (
            88,
            "return reply.code(429).send({ error: \"rate_limited\" });",
        ),
    ];
    let body_rows = area.height.saturating_sub(1 + COMPOSER_ROWS) as usize;
    for (no, code) in hits {
        let hit = grep_hit_line(w, *no, code);
        if lines.len() + hit.len() > body_rows {
            break;
        }
        lines.extend(hit);
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 90% context"),
        GHOST_RUNNING,
    );
}

fn board_glob(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(TEXT)),
        Span::styled(
            first_fitting_line("Glob **/*rate* 4 files", w.saturating_sub(2)),
            Style::default().fg(TEXT),
        ),
    ]));
    let files = [
        "src/middleware/rateLimit.ts",
        "src/config/rateLimits.json",
        "test/rateLimit.test.ts",
        "docs/rate-limiting.md",
    ];
    for path in files.iter() {
        let fitted = first_fitting_line(path, w.saturating_sub(2));
        let mut spans = vec![Span::styled("  ", Style::default().fg(TEXT))];
        spans.extend(paint_match_path(&fitted, "rate"));
        lines.push(Line::from(spans));
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 91% context"),
        GHOST_RUNNING,
    );
}

fn board_delete(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = if compact(area) {
        Vec::new()
    } else {
        user_prompt_lines(area)
    };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(TEXT)),
        Span::styled(
            "Delete",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
    ]));
    for part in wrap_or_drop("  src/middleware/rateLimit.legacy.ts", w) {
        lines.push(white(part));
    }
    for part in wrap_or_drop("File will be removed from disk. Undo via git.", w) {
        lines.push(dim(part));
    }
    lines.push(Line::from(""));
    let body_h = lines.len() as u16;
    Paragraph::new(lines).render(
        Rect::new(area.x, area.y, area.width, area.height.saturating_sub(4)),
        buf,
    );
    let limit = area.bottom().saturating_sub(2);
    let mut y = area.y + body_h.min(area.height.saturating_sub(4));
    y += picker_option(area, buf, y, limit, true, Some(1), "Delete", "", "");
    y += picker_option(area, buf, y, limit, false, Some(2), "Keep", "", "");
    paint_hints_and_footer(
        area,
        buf,
        y,
        "↑↓ select · ↵ confirm · esc keep",
        &format!("{MODEL} · Agent"),
    );
}

fn board_list(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    lines.push(tile_header(
        "List",
        "src/middleware 4 entries",
        Style::default().fg(TEXT_DIM),
        w,
    ));
    for name in ["auth.ts", "rateLimit.ts", "cors.ts"] {
        lines.push(white(format!("  {name}")));
    }
    lines.push(Line::from(Span::styled(
        format!("  {}", first_fitting_line("internal/", w.saturating_sub(2))),
        Style::default().fg(TEXT),
    )));
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 93% context"),
        GHOST_RUNNING,
    );
}

fn board_fetch(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    let url = "https://redis.io/docs/latest/commands/zadd/";
    let url_fit = first_fitting_line(url, w.saturating_sub(8));
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(TEXT)),
        Span::styled(
            "Fetch ",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if url_fit.is_empty() {
                first_fitting_line("redis.io/zadd", w.saturating_sub(8))
            } else {
                url_fit
            },
            Style::default().fg(TEXT_DIM),
        ),
    ]));
    lines.push(dim(first_fitting_line("ZADD | Redis", w)));
    if !compact(area) {
        for body in [
            "ZADD key [NX | XX] [GT | LT] [CH] [INCR] score member [score member ...]",
            "Adds all the specified members with the specified scores to the",
            "sorted set stored at key. Returns the number of elements added.",
        ] {
            for part in wrap_or_drop(&format!("  {body}"), w) {
                lines.push(dim(part));
            }
        }
    } else {
        for part in wrap_or_drop("  Adds members with scores to a sorted set.", w) {
            lines.push(dim(part));
        }
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 88% context"),
        GHOST_RUNNING,
    );
}

fn board_mcp_call(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(TEXT)),
        Span::styled(
            first_fitting_line("MCP linear / list_issues", w.saturating_sub(2)),
            Style::default().fg(TEXT),
        ),
    ]));
    lines.push(dim("  team=API  state=started"));
    // The issue list is a real table — the same plus-ASCII grid the markdown
    // renderer draws: gray `+---+` borders, bold white header, white cells.
    // At 40 columns the assignee column gives way so the ids stay whole.
    let mut table = TableBuilder::new();
    let headers: &[&str] = if compact(area) {
        &["Issue", "Title", "State"]
    } else {
        &["Issue", "Title", "State", "Assignee"]
    };
    table.start_header();
    for header in headers {
        table.add_cell(header.to_string());
    }
    table.end_header();
    let issues = [
        ("API-184", "Rate limit 429 body", "In Progress", "you"),
        ("API-191", "Sliding window spike", "In Progress", "you"),
        ("API-172", "Retry-After on 429", "Todo", "mathis"),
    ];
    for (id, title, state, who) in issues {
        let mut cells: Vec<&str> = vec![id, title, state];
        if !compact(area) {
            cells.push(who);
        }
        for cell in cells {
            table.add_cell(cell.to_string());
        }
        table.end_row();
    }
    let mut table = table.build();
    table.calculate_column_widths(w.saturating_sub(2) as u16);
    for row in render_table(
        &table,
        HAIRLINE,
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        Style::default().fg(TEXT),
        w.saturating_sub(2) as u16,
    ) {
        let mut spans = vec![Span::raw("  ")];
        spans.extend(row.spans);
        lines.push(Line::from(spans));
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 87% context"),
        GHOST_RUNNING,
    );
}

fn board_task(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    lines.push(tile_header(
        "Task",
        "Write integration tests",
        Style::default().fg(TEXT),
        w,
    ));
    lines.push(Line::from(vec![
        Span::styled("  ⠇ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line("Running vitest · 18s", w.saturating_sub(4)),
            Style::default().fg(TEXT_DIM),
        ),
    ]));
    // Live vitest output — the same suite the Shell tile runs; `✓` green.
    for (mark, mark_color, result) in [
        ("  ✓ ", SUCCESS, "rejects a 61st request in the window"),
        ("  ✓ ", SUCCESS, "returns 429 with Retry-After"),
        (
            "  ⠇ ",
            TEXT_DIM,
            "allows requests again after the window slides",
        ),
    ] {
        lines.extend(marker_lines(
            mark,
            Style::default().fg(mark_color),
            result,
            Style::default().fg(TEXT_DIM),
            w,
        ));
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 89% context"),
        GHOST_RUNNING,
    );
}

fn board_diagnostics(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(area);
    let path = if compact(area) {
        "rateLimit.ts".to_string()
    } else {
        "src/middleware/rateLimit.ts".to_string()
    };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(TEXT)),
        Span::styled(
            "Diagnostics ",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(path, Style::default().fg(TEXT_DIM)),
        Span::styled("  2", Style::default().fg(TEXT_DIM)),
    ]));
    let error_msg = "Property 'apiKey' does not exist on type 'FastifyRequest'.";
    let warn_msg = "'redis' is declared but its value is never used.";
    // Severity words only carry color; message and path stay gray/white.
    // Messages wrap whole under the message column at 40 columns.
    for (severity, severity_color, location, message) in [
        ("  error ", ERROR, "L22  ", error_msg),
        ("  warn  ", WARNING, "L47  ", warn_msg),
    ] {
        for (i, part) in wrap_or_drop(message, w.saturating_sub(13))
            .into_iter()
            .enumerate()
        {
            if i == 0 {
                lines.push(Line::from(vec![
                    Span::styled(severity, Style::default().fg(severity_color)),
                    Span::styled(location, Style::default().fg(TEXT_DIM)),
                    Span::styled(part, Style::default().fg(TEXT)),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(13)),
                    Span::styled(part, Style::default().fg(TEXT)),
                ]));
            }
        }
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 86% context"),
        GHOST_RUNNING,
    );
}

fn board_multi_diff(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    paint_command_turn(area, buf, y, "/diff");
    y += if compact(area) { 1 } else { 2 };
    buf.set_string(
        area.x,
        y,
        first_fitting_line("Changed this turn", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    let header = first_fitting_line("Changed this turn", w);
    let count = " 4 files";
    let hx = area.x + header.chars().count() as u16;
    if (hx as usize) + count.trim().len() < w {
        buf.set_string(hx, y, count, Style::default().fg(TEXT_DIM));
    }
    y += 1;

    let files: &[(&str, &str, &str)] = &[
        ("src/middleware/rateLimit.ts", "+84", "-"),
        ("src/server/routes/completions.ts", "+9", "-2"),
        ("test/rateLimit.test.ts", "+61", "-"),
        (".env.example", "+2", "-"),
    ];
    let limit = area.bottom().saturating_sub(2);
    for (i, (path, plus, minus)) in files.iter().enumerate() {
        // All four changed files list at both sizes.
        if y >= limit {
            break;
        }
        let selected = i == 0;
        if selected {
            fill_row(buf, area, y, SELECTION_BG);
        }
        let stats = format!("{plus} {minus}");
        let path_w = w.saturating_sub(stats.chars().count() + 4);
        let shown = first_fitting_line(path, path_w);
        let with_row_bg = |style: Style| {
            if selected {
                style.bg(SELECTION_BG)
            } else {
                style
            }
        };
        let (marker, marker_style) = if selected {
            ("> ", Style::default().fg(ACCENT).bg(SELECTION_BG))
        } else {
            ("· ", Style::default().fg(TEXT_DIM))
        };
        buf.set_string(area.x, y, marker, marker_style);
        let path_style = if selected {
            Style::default()
                .fg(ACCENT)
                .bg(SELECTION_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        buf.set_string(area.x + 2, y, &shown, path_style);
        let plus_x = area
            .right()
            .saturating_sub(stats.chars().count() as u16 + 1)
            .max(area.x);
        buf.set_string(plus_x, y, plus, with_row_bg(Style::default().fg(DIFF_ADD)));
        buf.set_string(
            plus_x + plus.chars().count() as u16 + 1,
            y,
            minus,
            with_row_bg(Style::default().fg(TEXT_DIM)),
        );
        y += 1;
    }
    paint_hints_and_footer(
        area,
        buf,
        y,
        "↑↓ select · ↵ open · esc close",
        &format!("{MODEL} · Agent"),
    );
}

fn board_settings_hub(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    paint_command_turn(area, buf, y, "/settings");
    y += if compact(area) { 1 } else { 2 };
    buf.set_string(
        area.x,
        y,
        first_fitting_line("Settings", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    y += 1;
    let rows: &[(&str, &str)] = &[
        ("Model", "Cortex Mini 1 · Medium"),
        ("Mode", "Agent"),
        ("Permissions", "Smart"),
        ("Sandbox", "On · workspace"),
        ("MCP", "3 of 4 connected"),
        ("Config", "~/.cortex/config.json"),
        ("Usage", "42 / 500 agent requests"),
    ];
    let limit = area.bottom().saturating_sub(2);
    for (i, (label, value)) in rows.iter().enumerate() {
        if y >= limit {
            break;
        }
        y += picker_option(area, buf, y, limit, i == 0, None, label, value, "");
    }
    paint_hints_and_footer(
        area,
        buf,
        y,
        "↑↓ select · ↵ open · esc close",
        &format!("{MODEL} · Agent"),
    );
}

fn board_sandbox_deny(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_turn_lines("curl https://evil.example/steal", area);
    if !compact(area) {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(vec![
        Span::styled(
            format!("{} ", crate::ui::consts::STOPPED_MARK),
            Style::default().fg(ERROR),
        ),
        Span::styled(
            crate::ui::consts::SANDBOX_DENIED_TITLE,
            Style::default().fg(ERROR).add_modifier(Modifier::BOLD),
        ),
    ]));
    for part in wrap_or_drop(
        "curl was blocked by the workspace sandbox. Network is allowlisted.",
        w,
    ) {
        lines.push(dim(part));
    }
    paint_session(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · Smart"),
        GHOST_IDLE,
    );
}

fn board_mcp_drop(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_turn_lines("/mcp", area);
    if !compact(area) {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(vec![
        Span::styled("x ", Style::default().fg(ERROR)),
        Span::styled("github  ", Style::default().fg(TEXT)),
        Span::styled("dropped", Style::default().fg(TEXT_DIM)),
    ]));
    for part in wrap_or_drop("connection lost — retrying in 30s", w) {
        lines.push(dim(part));
    }
    if !compact(area) {
        lines.push(Line::from(""));
        lines.push(dim(first_fitting_line("r reconnect · esc close", w)));
    }
    paint_session(area, buf, lines, &format!("{MODEL} · Agent"), GHOST_IDLE);
}
