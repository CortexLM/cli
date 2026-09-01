//! Pixel-lock painters for boards 11–50.
//!
//! These scenes share session chrome (prompt, composer, cwd+git footer) and
//! Cortex product copy only.

use cortex_core::style::{DIFF_ADD, ERROR, SELECTION_BG, SUCCESS, TEXT, TEXT_DIM, WARNING};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::ui::text_utils::{first_fitting_line, wrap_or_drop};

const USER_PROMPT: &str = "Add rate limiting to POST /v1/completions – 60 req/min per API key, sliding window, Redis-backed, with tests";
const CWD: &str = "~/cortex-api";
const GIT: &str = "main*";
const MODEL: &str = "cortex-1-mini";
const COMMAND_BG: Color = Color::Rgb(0x1C, 0x1C, 0x20);

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
            | "login"
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
        "login" => board_login(area, buf),
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
        _ => {}
    }
}

fn inner_width(area: Rect) -> usize {
    area.width.saturating_sub(1).max(1) as usize
}

fn compact(area: Rect) -> bool {
    area.height < 20 || area.width < 50
}

fn paint_lines(area: Rect, buf: &mut Buffer, lines: Vec<Line<'_>>, footer_mode: &str, extra: &str) {
    let w = inner_width(area);
    let footer_h = 1u16;
    let composer_h = if extra.is_empty() { 1u16 } else { 2u16 };
    let body_h = area.height.saturating_sub(footer_h + composer_h);
    let body = Rect::new(area.x, area.y, area.width, body_h);
    Paragraph::new(lines).render(body, buf);

    if extra.is_empty() {
        let hints_y = area.bottom().saturating_sub(2);
        if hints_y > area.y {
            buf.set_string(
                area.x,
                hints_y,
                first_fitting_line("/ commands · @ files · ! shell · shift+tab modes", w),
                Style::default().fg(TEXT_DIM),
            );
        }
        paint_footer(area, buf, footer_mode);
        return;
    }

    let composer_y = area.y + body_h;
    let ghost = first_fitting_line(extra, w.saturating_sub(2));
    buf.set_string(
        area.x,
        composer_y,
        first_fitting_line(&format!("> {ghost}"), w),
        Style::default().fg(TEXT_DIM),
    );
    if let Some(cell) = buf.cell_mut((area.x, composer_y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('>');
    }
    let hints_y = composer_y
        .saturating_add(1)
        .min(area.bottom().saturating_sub(2));
    if hints_y > composer_y && hints_y < area.bottom() {
        buf.set_string(
            area.x,
            hints_y,
            first_fitting_line("/ commands · @ files · ! shell · shift+tab modes", w),
            Style::default().fg(TEXT_DIM),
        );
    }

    paint_footer(area, buf, footer_mode);
}

fn paint_footer(area: Rect, buf: &mut Buffer, right: &str) {
    let y = area.bottom().saturating_sub(1);
    let w = area.width as usize;
    let left = format!("{CWD} {GIT}");
    let left_fit = first_fitting_line(&left, w);
    buf.set_string(area.x, y, &left_fit, Style::default().fg(TEXT_DIM));
    let left_len = left_fit.chars().count();
    let mut right_fit = first_fitting_line(right, w);
    let gap_ok = |r: &str| left_len + 1 + r.chars().count() <= w;
    if !gap_ok(&right_fit) {
        right_fit = MODEL.to_string();
    }
    if right_fit.is_empty() || !gap_ok(&right_fit) {
        return;
    }
    let rx = area
        .right()
        .saturating_sub(right_fit.chars().count() as u16)
        .max(area.x);
    buf.set_string(rx, y, &right_fit, Style::default().fg(TEXT_DIM));
}

fn user_prompt_lines(width: usize, area: Rect) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if compact(area) {
        lines.push(Line::from(Span::styled(
            first_fitting_line(&format!("> {USER_PROMPT}"), width),
            Style::default().fg(TEXT),
        )));
    } else {
        let wrapped = wrap_or_drop(&format!("> {USER_PROMPT}"), width);
        for (i, line) in wrapped.into_iter().enumerate() {
            let style = if i == 0 {
                Style::default().fg(TEXT)
            } else {
                Style::default().fg(TEXT_DIM)
            };
            lines.push(Line::from(Span::styled(line, style)));
        }
    }
    lines.push(Line::from(""));
    lines
}

fn dim(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), Style::default().fg(TEXT_DIM)))
}

fn white(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), Style::default().fg(TEXT)))
}

fn fill_row(buf: &mut Buffer, area: Rect, y: u16, bg: Color) {
    if y >= area.bottom() {
        return;
    }
    for x in area.x..area.right() {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_bg(bg);
            cell.set_fg(TEXT);
        }
    }
}

/// Paint the `>` caret of a selection bar violet, like the locked slash rows.
fn accent_selection_caret(buf: &mut Buffer, area: Rect, y: u16) {
    if let Some(cell) = buf.cell_mut((area.x, y)) {
        cell.set_style(Style::default().fg(SUCCESS).bg(SELECTION_BG));
        cell.set_char('>');
    }
}

fn board_shell(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
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
        for line in [
            "  > cortex-api@2.4.1 test",
            "  > vitest run --reporter=verbose \"rateLimit\"",
        ] {
            lines.push(dim(first_fitting_line(line, w)));
        }
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
    lines.push(Line::from(vec![
        Span::styled("  ⠇ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line(
                "test/rateLimit.test.ts allows requests again after the window slides",
                w.saturating_sub(4),
            ),
            Style::default().fg(TEXT_DIM),
        ),
    ]));
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 85% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_permission(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    let reserve = 6u16;

    if !compact(area) {
        for line in user_prompt_lines(w, area) {
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
            buf.set_string(
                area.x,
                y,
                first_fitting_line(&part, w),
                Style::default().fg(TEXT_DIM),
            );
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
    if let Some(cell) = buf.cell_mut((area.x, y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('●');
    }
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
    y += 2;

    let options = [
        (true, "Yes, run once"),
        (false, "Yes, always allow npm install in this project"),
        (false, "Edit command"),
        (false, "No — tell Cortex what to do instead"),
    ];
    for (selected, label) in options {
        if y >= area.bottom().saturating_sub(3) {
            break;
        }
        if selected {
            fill_row(buf, area, y, SELECTION_BG);
            buf.set_string(
                area.x,
                y,
                first_fitting_line(&format!("> {label}"), w),
                Style::default()
                    .fg(TEXT)
                    .bg(SELECTION_BG)
                    .add_modifier(Modifier::BOLD),
            );
            accent_selection_caret(buf, area, y);
            y += 1;
        } else {
            // Option copy word-wraps at narrow widths — words are never
            // dropped or truncated.
            for part in wrap_or_drop(label, w.saturating_sub(2)) {
                if y >= area.bottom().saturating_sub(3) {
                    break;
                }
                buf.set_string(area.x + 2, y, &part, Style::default().fg(TEXT));
                y += 1;
            }
        }
    }
    y += 1;
    if y < area.bottom().saturating_sub(1) {
        buf.set_string(
            area.x,
            y,
            first_fitting_line("↑↓ select · ↵ confirm · e edit command · esc cancel", w),
            Style::default().fg(TEXT_DIM),
        );
    }
    paint_footer(
        area,
        buf,
        &format!("{MODEL} · Agent · Normal · 90% context"),
    );
}

fn board_plan(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = if compact(area) {
        vec![Line::from("")]
    } else {
        user_prompt_lines(w, area)
    };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
        Span::styled(
            first_fitting_line(
                "Plan Redis-backed rate limiting for /v1/completions",
                w.saturating_sub(2),
            ),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
    ]));
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
        &steps[..3]
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
    let yes_lines: Vec<String> = wrap_or_drop("> Yes, switch to Agent mode and implement", w)
        .into_iter()
        .take(2)
        .collect();
    let yes_rows = yes_lines.len().max(1) as u16;

    let body_h = area.height.saturating_sub(3 + yes_rows);
    Paragraph::new(lines).render(Rect::new(area.x, area.y, area.width, body_h), buf);

    let no_y = area.bottom().saturating_sub(3);
    let yes_y = no_y.saturating_sub(yes_rows);
    for (i, part) in yes_lines.iter().enumerate() {
        let y = yes_y + i as u16;
        fill_row(buf, area, y, SELECTION_BG);
        let x = if i == 0 { area.x } else { area.x + 2 };
        buf.set_string(
            x,
            y,
            first_fitting_line(part, w.saturating_sub(if i == 0 { 0 } else { 2 })),
            Style::default()
                .fg(TEXT)
                .bg(SELECTION_BG)
                .add_modifier(Modifier::BOLD),
        );
        if i == 0 {
            accent_selection_caret(buf, area, y);
        }
    }
    buf.set_string(
        area.x + 2,
        no_y,
        first_fitting_line(
            "No, keep planning — tell Cortex what to change",
            w.saturating_sub(2),
        ),
        Style::default().fg(TEXT),
    );
    buf.set_string(
        area.x,
        no_y.saturating_add(1),
        first_fitting_line("↑↓ select · ↵ confirm · esc keep planning", w),
        Style::default().fg(TEXT_DIM),
    );
    paint_footer(area, buf, &format!("{MODEL} · Plan · 93% context"));
}

fn board_streaming(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(white(first_fitting_line(
        "Done — the limiter is in place. Here is how it works:",
        w,
    )));
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
        lines.push(Line::from(vec![
            Span::styled("• ", Style::default().fg(TEXT_DIM)),
            Span::styled(
                label.to_string(),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                first_fitting_line(&format!(" {detail}"), w.saturating_sub(label.len() + 2)),
                Style::default().fg(TEXT_DIM),
            ),
        ]));
    }
    if !compact(area) {
        lines.push(Line::from(""));
        lines.push(dim(first_fitting_line("```ts", w)));
        for code in [
            "export async function rateLimit(key: string) {",
            "  const now = Date.now();",
            "  await redis.zadd(key, now, String(now));",
            "  await redis.zremrangebyscore(key, 0, now - 60_000);",
            "  const n = await redis.zcard(key);",
            "  return n <= 60;",
            "}",
        ] {
            lines.push(white(first_fitting_line(code, w)));
        }
        lines.push(dim(first_fitting_line("```", w)));
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
                Span::styled("█", Style::default().fg(TEXT)),
            ]));
            break;
        }
        lines.push(white(part));
    }
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 81% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_resume(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    Paragraph::new(vec![
        Line::from(vec![
            Span::styled("> ", Style::default().fg(SUCCESS)),
            Span::styled("/resume", Style::default().fg(TEXT)),
        ]),
        dim(first_fitting_line("/ Type to search sessions", w)),
    ])
    .render(Rect::new(area.x, area.y, area.width, 3), buf);

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

    let mut y = area.y + 3;
    for (selected, when, title, branch, msgs) in rows {
        if y >= area.bottom().saturating_sub(4) {
            break;
        }
        if selected {
            fill_row(buf, area, y, SELECTION_BG);
            let shown = if compact(area) {
                first_fitting_line(&format!("> {when}  Rate limiting  {msgs}"), w)
            } else {
                first_fitting_line(&format!("> {when}  {title}  {branch}  {msgs}"), w)
            };
            buf.set_string(
                area.x,
                y,
                &shown,
                Style::default().fg(TEXT).bg(SELECTION_BG),
            );
            accent_selection_caret(buf, area, y);
        } else if !compact(area) {
            let row = first_fitting_line(&format!("{when}  {title}  {branch}  {msgs}"), w);
            buf.set_string(area.x, y, format!("  {row}"), Style::default().fg(TEXT));
        } else {
            continue;
        }
        y += 1;
    }
    y += 1;
    if y < area.bottom().saturating_sub(2) {
        buf.set_string(
            area.x,
            y,
            first_fitting_line(
                "Sessions sync through Cortex Cloud — resume from any machine.",
                w,
            ),
            Style::default().fg(TEXT_DIM),
        );
        y += 1;
    }
    if y < area.bottom().saturating_sub(1) {
        buf.set_string(
            area.x,
            y.min(area.bottom().saturating_sub(2)),
            first_fitting_line("↑↓ select · ↵ resume · x delete · esc cancel", w),
            Style::default().fg(TEXT_DIM),
        );
    }
    paint_footer(area, buf, &format!("{MODEL} · Agent"));
}

fn board_mcp(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("> ", Style::default().fg(SUCCESS)),
            Span::styled("/mcp", Style::default().fg(TEXT)),
        ]),
        white(first_fitting_line("MCP servers · 2 of 4 connected", w)),
        Line::from(""),
    ];
    // Violet marks connected servers only; every status word stays gray.
    let servers = [
        (
            "●",
            SUCCESS,
            "github",
            "connected",
            "12 tools · repos, issues, pull requests",
        ),
        (
            "●",
            SUCCESS,
            "postgres",
            "connected",
            "6 tools · localhost:5432/cortex",
        ),
        (
            "●",
            TEXT_DIM,
            "sentry",
            "authenticating",
            "waiting for browser sign-in...",
        ),
        (
            "x",
            TEXT,
            "linear",
            "failed",
            "connection refused — retrying in 30s",
        ),
    ];
    for (mark, mark_color, name, status, detail) in servers {
        if compact(area) && matches!(name, "postgres" | "sentry") {
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
    lines.push(Line::from(""));
    lines.push(dim(first_fitting_line(
        "Config: ~/.cortex/mcp.json — servers inherit the sandbox network policy.",
        w,
    )));
    lines.push(dim(first_fitting_line(
        "↵ details · r reconnect · a add server · esc close",
        w,
    )));
    Paragraph::new(lines).render(
        Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1)),
        buf,
    );
    paint_footer(area, buf, &format!("{MODEL} · Agent"));
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

fn board_usage(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("> ", Style::default().fg(SUCCESS)),
            Span::styled("/usage", Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled(
                "Cortex Pro",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" · ", Style::default().fg(TEXT_DIM)),
            Span::styled("renews Sep 28", Style::default().fg(TEXT_DIM)),
        ]),
        Line::from(""),
    ];
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
    lines.push(Line::from(""));
    for part in wrap_or_drop(
        "MAX mode bills by token instead of per request — manage at cortex.foundation/billing",
        w,
    ) {
        lines.push(dim(part));
    }
    paint_lines(area, buf, lines, &format!("{MODEL} · Agent"), "");
}

fn board_quota(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = vec![Line::from(vec![
        Span::styled("> ", Style::default().fg(TEXT)),
        Span::styled(
            first_fitting_line(
                "Now add the same limiter to the embeddings endpoint",
                w.saturating_sub(2),
            ),
            Style::default().fg(TEXT),
        ),
    ])];
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("x ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled(
            "Agent quota exhausted",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
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
        if compact(area) {
            break;
        }
    }
    if !compact(area) {
        for part in wrap_or_drop(
            "Switch to MAX token billing to continue now, or upgrade at cortex.foundation/billing",
            w,
        ) {
            lines.push(dim(part));
        }
    }
    lines.push(dim(first_fitting_line(
        "/usage details · /model switch to MAX · esc dismiss",
        w,
    )));
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 71% context"),
        "Add a follow-up — held until quota resets",
    );
}

fn board_sandbox(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    Paragraph::new(vec![Line::from(vec![
        Span::styled("> ", Style::default().fg(SUCCESS)),
        Span::styled("/sandbox", Style::default().fg(TEXT)),
    ])])
    .render(Rect::new(area.x, area.y, area.width, 1), buf);

    let y = area.y + 2;
    fill_row(buf, area, y, SELECTION_BG);
    let row = first_fitting_line("> Sandbox mode", w.saturating_sub(8));
    buf.set_string(
        area.x,
        y,
        &row,
        Style::default()
            .fg(TEXT)
            .bg(SELECTION_BG)
            .add_modifier(Modifier::BOLD),
    );
    accent_selection_caret(buf, area, y);
    let on = "● On";
    let ox = area.right().saturating_sub(on.len() as u16 + 1);
    if ox > area.x + 4 {
        buf.set_string(ox, y, "●", Style::default().fg(SUCCESS).bg(SELECTION_BG));
        buf.set_string(ox + 2, y, "On", Style::default().fg(TEXT).bg(SELECTION_BG));
    }

    let details = [
        (
            "Filesystem",
            "read/write inside ~/cortex-api · read-only elsewhere",
        ),
        ("Network", "allowlist · registry.npmjs.org, github.com"),
        ("Escalation", "ask before running outside the sandbox"),
    ];
    let mut dy = y + 2;
    for (label, value) in details {
        if dy >= area.bottom().saturating_sub(4) {
            break;
        }
        buf.set_string(
            area.x,
            dy,
            first_fitting_line(label, w),
            Style::default().fg(TEXT),
        );
        dy += 1;
        buf.set_string(
            area.x,
            dy,
            first_fitting_line(value, w),
            Style::default().fg(TEXT_DIM),
        );
        dy += 1;
    }
    if !compact(area) {
        dy += 1;
        for part in wrap_or_drop(
            "Commands run in an isolated container with the repo mounted. Anything that needs to leave the sandbox — networking, global installs — asks first.",
            w,
        ) {
            if dy >= area.bottom().saturating_sub(3) {
                break;
            }
            buf.set_string(area.x, dy, &part, Style::default().fg(TEXT_DIM));
            dy += 1;
        }
    }
    buf.set_string(
        area.x,
        area.bottom().saturating_sub(2),
        first_fitting_line("↑↓ select · space toggle · a add domain · esc close", w),
        Style::default().fg(TEXT_DIM),
    );
    paint_footer(area, buf, &format!("{MODEL} · Agent · Smart"));
}

fn board_cloud(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = vec![Line::from(vec![
        Span::styled("> ", Style::default().fg(TEXT)),
        Span::styled(
            first_fitting_line(
                "& fix the flaky rateLimit integration test on CI",
                w.saturating_sub(2),
            ),
            Style::default().fg(TEXT),
        ),
    ])];
    lines.push(Line::from(vec![
        Span::styled("↑ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            "Handed off to Cortex Cloud",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(dim(first_fitting_line(
        "agent    bc-4f2a · started from main @ 9d31c4e",
        w,
    )));
    lines.push(dim(first_fitting_line(
        "branch   cortex/fix-flaky-ratelimit",
        w,
    )));
    for part in wrap_or_drop(
        "follow   cortex.foundation/agents/bc-4f2a · or /jobs right here",
        w,
    ) {
        lines.push(dim(part));
    }
    lines.push(dim(first_fitting_line(
        "Your terminal is free — the cloud agent pushes commits as it goes.",
        w,
    )));
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 97% context"),
        "Plan, search, build anything",
    );
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

fn paint_hints_and_footer(area: Rect, buf: &mut Buffer, hints: &str, footer: &str) {
    let w = inner_width(area);
    let hints_y = area.bottom().saturating_sub(2);
    if hints_y > area.y {
        buf.set_string(
            area.x,
            hints_y,
            first_fitting_line(hints, w),
            Style::default().fg(TEXT_DIM),
        );
    }
    paint_footer(area, buf, footer);
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

fn board_sudo(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    buf.set_string(
        area.x,
        y,
        first_fitting_line(
            "> Something is already bound to 6379 - find it and free the port",
            w,
        ),
        Style::default().fg(TEXT),
    );
    y += 2;
    buf.set_string(
        area.x,
        y,
        first_fitting_line("● Shell sudo lsof -i :6379 -sTCP:LISTEN", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    if let Some(cell) = buf.cell_mut((area.x, y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('●');
    }
    y += 1;
    buf.set_string(
        area.x,
        y,
        first_fitting_line(
            "needs elevated privileges — password goes straight to sudo",
            w,
        ),
        Style::default().fg(TEXT_DIM),
    );
    y += 1;
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
        buf.set_string(area.x, y, &part, Style::default().fg(TEXT_DIM));
        y += 1;
        if y + 2 >= area.bottom() {
            break;
        }
    }
    paint_hints_and_footer(
        area,
        buf,
        "/ commands · @ files · ! shell · shift+tab modes",
        &format!("{MODEL} · Agent · Smart · 89% context"),
    );
}

fn board_ask(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = Vec::new();
    let badge = "Ask — read-only";
    let rest = " Cortex will not edit files or run commands. shift+tab to switch.";
    lines.push(Line::from(vec![
        Span::styled("┌ ", Style::default().fg(TEXT_DIM)),
        Span::styled(badge.to_string(), Style::default().fg(TEXT)),
        Span::styled(" ┐", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line(rest, w.saturating_sub(badge.len() + 4)),
            Style::default().fg(TEXT_DIM),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("> ", Style::default().fg(TEXT)),
        Span::styled(
            first_fitting_line(
                "How does token counting work for streamed completions?",
                w.saturating_sub(2),
            ),
            Style::default().fg(TEXT),
        ),
    ]));
    for part in wrap_or_drop(
        "Streamed completions estimate tokens up front, then reconcile when the stream ends.",
        w,
    ) {
        lines.push(white(part));
        if compact(area) {
            break;
        }
    }
    if !compact(area) {
        lines.push(Line::from(vec![
            Span::styled("See ", Style::default().fg(TEXT_DIM)),
            Span::styled("src/lib/tokens.ts", Style::default().fg(TEXT)),
            Span::styled(" for the implementation.", Style::default().fg(TEXT_DIM)),
        ]));
        for (n, ident, detail) in [
            (
                "1. ",
                "estimateTokens(prompt)",
                " counts the request before the stream.",
            ),
            (
                "2. ",
                "usage.completion_tokens",
                " arrives on the final chunk.",
            ),
            ("3. ", "reconcileUsage()", " corrects the running total."),
        ] {
            lines.push(Line::from(vec![
                Span::styled(n.to_string(), Style::default().fg(TEXT_DIM)),
                Span::styled(
                    ident.to_string(),
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    first_fitting_line(detail, w.saturating_sub(n.len() + ident.len())),
                    Style::default().fg(TEXT_DIM),
                ),
            ]));
        }
    }
    let body_h = area.height.saturating_sub(3);
    Paragraph::new(lines).render(Rect::new(area.x, area.y, area.width, body_h), buf);

    let composer_y = area.bottom().saturating_sub(3);
    buf.set_string(area.x, composer_y, "> █", Style::default().fg(TEXT_DIM));
    if let Some(cell) = buf.cell_mut((area.x, composer_y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('>');
    }
    buf.set_string(
        area.x.saturating_add(3),
        composer_y,
        first_fitting_line("Reply, or shift+tab for Agent mode", w.saturating_sub(3)),
        Style::default().fg(TEXT_DIM),
    );
    paint_hints_and_footer(
        area,
        buf,
        "/ commands · @ files · ! shell · shift+tab modes",
        &format!("{MODEL} · Ask · 96% context"),
    );
}

fn board_files(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let typed = "Add integration tests for @rate";
    buf.set_string(
        area.x,
        area.y,
        first_fitting_line(&format!("> {typed}█"), w),
        Style::default().fg(TEXT),
    );
    if let Some(cell) = buf.cell_mut((area.x, area.y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('>');
    }

    let rows = [
        (true, "src/middleware/rateLimit.ts", "edited 2m ago"),
        (false, "test/rateLimit.test.ts", "edited 5m ago"),
        (false, "src/config/rateLimits.json", "2 days ago"),
        (false, "docs/rate-limiting.md", "last week"),
    ];
    let mut y = area.y + 2;
    for (selected, path, when) in rows {
        if y >= area.bottom().saturating_sub(3) {
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
        let marker = if selected { ">" } else { " " };
        let marker_style = if selected {
            Style::default().fg(SUCCESS).bg(SELECTION_BG)
        } else {
            Style::default().fg(TEXT)
        };
        buf.set_string(x, y, marker, marker_style);
        x = x.saturating_add(2);
        let mut spans = paint_match_path(path, "rate");
        let mut used = 0usize;
        for span in &mut spans {
            if selected {
                span.style = span.style.fg(TEXT).bg(SELECTION_BG);
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
                .saturating_sub(when_fit.chars().count() as u16)
                .max(x.saturating_add(1));
            let when_style = if selected {
                Style::default().fg(TEXT).bg(SELECTION_BG)
            } else {
                Style::default().fg(TEXT_DIM)
            };
            buf.set_string(rx, y, &when_fit, when_style);
        }
        y += 1;
    }
    y += 1;
    if y < area.bottom().saturating_sub(1) {
        buf.set_string(
            area.x,
            y.min(area.bottom().saturating_sub(2)),
            first_fitting_line("↑↓ select · ↵ insert · tab complete · esc dismiss", w),
            Style::default().fg(TEXT_DIM),
        );
    }
    paint_footer(area, buf, &format!("{MODEL} · Agent · 100% context"));
}

fn board_queue(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
        Span::styled(
            first_fitting_line(
                "Edit src/middleware/rateLimit.ts · +58 -0",
                w.saturating_sub(2),
            ),
            Style::default().fg(TEXT),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("⁝ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line("Writing integration tests...", w.saturating_sub(2)),
            Style::default().fg(TEXT),
        ),
    ]));
    lines.push(dim(first_fitting_line(
        "1m 42s · 12.6k tokens · ctrl+c to stop",
        w,
    )));
    lines.push(Line::from(""));
    lines.push(dim(first_fitting_line(
        "Queued · 2 — sent when the current step finishes",
        w,
    )));
    lines.push(dim(first_fitting_line(
        "1. Also send a Retry-After header on 429 responses",
        w,
    )));
    lines.push(dim(first_fitting_line(
        "2. Update docs/rate-limiting.md with the new limits",
        w,
    )));
    let body_h = area.height.saturating_sub(3);
    Paragraph::new(lines).render(Rect::new(area.x, area.y, area.width, body_h), buf);
    let composer_y = area.bottom().saturating_sub(3);
    buf.set_string(area.x, composer_y, "> █", Style::default().fg(TEXT_DIM));
    if let Some(cell) = buf.cell_mut((area.x, composer_y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('>');
    }
    buf.set_string(
        area.x.saturating_add(3),
        composer_y,
        first_fitting_line("Add a follow-up — ⏎ to queue", w.saturating_sub(3)),
        Style::default().fg(TEXT_DIM),
    );
    paint_hints_and_footer(
        area,
        buf,
        "↑ edit queued · ctrl+x clear queue · ctrl+c stop",
        &format!("{MODEL} · Agent · 76% context"),
    );
}

fn board_jobs(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    buf.set_string(
        area.x,
        area.y,
        first_fitting_line("> /jobs", w),
        Style::default().fg(TEXT),
    );
    if let Some(cell) = buf.cell_mut((area.x, area.y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('>');
    }
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
        status_color: Color,
        meta: &'static str,
    }
    // Spinners and status words stay gray; violet marks the finished job.
    let jobs = [
        Job {
            selected: true,
            icon: "⠇",
            icon_color: TEXT_DIM,
            kind: "cloud",
            title: "Fix flaky rateLimit test on CI",
            status: "running",
            status_color: TEXT_DIM,
            meta: "4m · cortex/fix-flaky-ratelimit",
        },
        Job {
            selected: false,
            icon: "⠇",
            icon_color: TEXT_DIM,
            kind: "subagent",
            title: "Docs sweep — rate limits + 429 examples",
            status: "running",
            status_color: TEXT_DIM,
            meta: "1m · docs/rate-limiting.md",
        },
        Job {
            selected: false,
            icon: "✓",
            icon_color: SUCCESS,
            kind: "subagent",
            title: "Typecheck all packages",
            status: "done",
            status_color: TEXT_DIM,
            meta: "finished 2m ago · 0 errors",
        },
        Job {
            selected: false,
            icon: "x",
            icon_color: TEXT,
            kind: "cloud",
            title: "Bump ioredis 5 → 6",
            status: "failed",
            status_color: TEXT_DIM,
            meta: "18m ago · 3 tests failing",
        },
    ];
    let mut y = area.y + 3;
    for job in jobs {
        if y + 1 >= area.bottom().saturating_sub(3) {
            break;
        }
        if job.selected {
            fill_row(buf, area, y, SELECTION_BG);
            fill_row(buf, area, y + 1, SELECTION_BG);
        }
        let base = if job.selected {
            Style::default().fg(TEXT).bg(SELECTION_BG)
        } else {
            Style::default().fg(TEXT)
        };
        let mut icon_style = Style::default().fg(job.icon_color);
        if job.selected {
            icon_style = icon_style.bg(SELECTION_BG);
        }
        buf.set_string(area.x, y, job.icon, icon_style);
        buf.set_string(
            area.x.saturating_add(2),
            y,
            first_fitting_line(
                &format!("{}  {}", job.kind, job.title),
                w.saturating_sub(12),
            ),
            base,
        );
        let st = job.status;
        let sx = area.right().saturating_sub(st.len() as u16);
        let mut status_style = Style::default().fg(job.status_color);
        if job.selected {
            status_style = status_style.bg(SELECTION_BG);
        }
        buf.set_string(sx, y, st, status_style);
        buf.set_string(
            area.x.saturating_add(2),
            y + 1,
            first_fitting_line(job.meta, w.saturating_sub(2)),
            if job.selected {
                Style::default().fg(TEXT).bg(SELECTION_BG)
            } else {
                Style::default().fg(TEXT_DIM)
            },
        );
        y += 2;
        if compact(area) {
            break;
        }
    }
    buf.set_string(
        area.x,
        area.bottom().saturating_sub(2),
        first_fitting_line("⏎ open · a attach · x cancel job · esc close", w),
        Style::default().fg(TEXT_DIM),
    );
    paint_footer(area, buf, &format!("{MODEL} · Agent"));
}

fn board_help(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    buf.set_string(
        area.x,
        area.y,
        first_fitting_line("> /help", w),
        Style::default().fg(TEXT),
    );
    if let Some(cell) = buf.cell_mut((area.x, area.y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('>');
    }
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
    let reserve = if compact_help { 5 } else { 8 };
    if two_col {
        // Command white, description dim — same tone as the locked slash.
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
        let shown = if compact_help { &HELP[..2] } else { &HELP[..] };
        for (cmd, desc) in shown {
            if y >= area.bottom().saturating_sub(reserve) {
                break;
            }
            buf.set_string(
                area.x,
                y,
                first_fitting_line(cmd, w),
                Style::default().fg(TEXT),
            );
            y += 1;
            if !compact_help {
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
    }

    y += 1;
    if y < area.bottom().saturating_sub(4) {
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
        if y >= area.bottom().saturating_sub(3) {
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
    buf.set_string(
        area.x,
        area.bottom().saturating_sub(2),
        first_fitting_line(
            "Docs & guides: cortex.foundation/docs · Cortex CLI v1.0.0",
            w,
        ),
        Style::default().fg(TEXT_DIM),
    );
    paint_footer(area, buf, &format!("{MODEL} · Agent"));
}

fn board_first_run(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = vec![
        white("Cortex CLI v1.0.0"),
        white("Tips for getting started"),
        Line::from(""),
    ];
    let tips = [
        (
            "1.",
            "Describe a task in plain language.",
            "Cortex plans, edits files and runs commands — with your approval.",
        ),
        (
            "2.",
            "Steer with one keystroke.",
            "/ commands · @ mention files · ! run shell directly · & hand off to the cloud",
        ),
        (
            "3.",
            "Pick the right lane.",
            "shift+tab cycles Agent, Plan and Ask modes at any time.",
        ),
        (
            "4.",
            "Nothing is lost.",
            "cortex resume picks up any previous session, on any machine.",
        ),
    ];
    for (n, title, detail) in tips {
        if compact(area) && n != "1." && n != "2." {
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(format!("{n} "), Style::default().fg(TEXT_DIM)),
            Span::styled(
                first_fitting_line(title, w.saturating_sub(3)),
                Style::default().fg(TEXT),
            ),
        ]));
        for part in wrap_or_drop(detail, w) {
            lines.push(dim(part));
            if compact(area) {
                break;
            }
        }
    }
    lines.push(Line::from(""));
    for part in wrap_or_drop(
        "Docs: cortex.foundation/docs — this card is shown once. Bring it back anytime with /help.",
        w,
    ) {
        lines.push(dim(part));
    }
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 100% context"),
        "Plan, search, build anything",
    );
}

fn board_bash(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    buf.set_string(
        area.x,
        area.y,
        first_fitting_line("┌ Bash mode ┐", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    let mut hy = area.y + 1;
    for part in wrap_or_drop(
        "Commands run directly in your shell — the model is not involved. esc to exit.",
        w,
    ) {
        buf.set_string(area.x, hy, &part, Style::default().fg(TEXT_DIM));
        hy += 1;
        if compact(area) && hy > area.y + 2 {
            break;
        }
    }
    let mut y = area.y + 3;
    buf.set_string(
        area.x,
        y,
        first_fitting_line("! redis-cli PING", w),
        Style::default().fg(TEXT),
    );
    y += 1;
    buf.set_string(area.x, y, "PONG", Style::default().fg(TEXT_DIM));
    y += 2;
    buf.set_string(
        area.x,
        y,
        first_fitting_line("! npm run test:integration -- --grep rateLimit█", w),
        Style::default().fg(TEXT),
    );
    paint_hints_and_footer(
        area,
        buf,
        "↵ run · ↑↓ shell history · esc back to Cortex",
        &format!("{MODEL} · Agent · 94% context"),
    );
}

fn board_config(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    buf.set_string(
        area.x,
        area.y,
        first_fitting_line("> /config", w),
        Style::default().fg(TEXT),
    );
    if let Some(cell) = buf.cell_mut((area.x, area.y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('>');
    }
    buf.set_string(
        area.x,
        area.y + 1,
        first_fitting_line("Config · ~/.cortex/config.json", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );

    let rows = [
        (true, false, "model", "cortex-1-mini · Max · Fast Mode On"),
        (false, false, "permissions", "Smart"),
        (false, false, "sandbox", "Enabled"),
        (false, true, "network", "Allowlist · 2 domains"),
        (false, true, "filesystem", "Workspace read/write"),
        (false, false, "editor", "zed --wait"),
        (false, false, "theme", "Cortex Dark"),
        (false, false, "notifications", "On finish + on approval"),
        (false, false, "telemetry", "Off"),
    ];
    let mut y = area.y + 3;
    let last_idx = rows.len() - 1;
    for (i, (selected, child, key, value)) in rows.iter().enumerate() {
        if y >= area.bottom().saturating_sub(4) {
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
        let value_fit = first_fitting_line(value, w.saturating_sub(label.chars().count() + 2));
        if *selected {
            fill_row(buf, area, y, SELECTION_BG);
            buf.set_string(
                area.x,
                y,
                first_fitting_line(&format!("{label}  {value_fit}"), w.saturating_sub(8)),
                Style::default().fg(TEXT).bg(SELECTION_BG),
            );
            buf.set_string(
                area.right().saturating_sub(6),
                y,
                "⏎ edit",
                Style::default().fg(TEXT).bg(SELECTION_BG),
            );
        } else {
            buf.set_string(area.x, y, &label, Style::default().fg(TEXT));
            buf.set_string(
                area.x.saturating_add(label.chars().count() as u16 + 2),
                y,
                value_fit,
                Style::default().fg(TEXT_DIM),
            );
        }
        y += 1;
    }
    y += 1;
    if y < area.bottom().saturating_sub(2) {
        buf.set_string(
            area.x,
            y,
            first_fitting_line(
                "Project overrides in .cortex/config.json win over the global file.",
                w,
            ),
            Style::default().fg(TEXT_DIM),
        );
    }
    buf.set_string(
        area.x,
        area.bottom().saturating_sub(2),
        first_fitting_line("↑↓ navigate · ↵ edit · r reset to default · esc close", w),
        Style::default().fg(TEXT_DIM),
    );
    let fy = area.bottom().saturating_sub(1);
    let left = first_fitting_line(&format!("{CWD} {GIT}"), w);
    buf.set_string(area.x, fy, &left, Style::default().fg(TEXT_DIM));
    let right_full = format!("{MODEL} · MAX · Agent");
    if left.chars().count() + 1 + right_full.chars().count() <= w {
        let prefix = format!("{MODEL} · ");
        let rx = area
            .right()
            .saturating_sub(right_full.chars().count() as u16);
        buf.set_string(rx, fy, &prefix, Style::default().fg(TEXT_DIM));
        buf.set_string(
            rx.saturating_add(prefix.chars().count() as u16),
            fy,
            "MAX",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        );
        buf.set_string(
            rx.saturating_add(prefix.chars().count() as u16 + 3),
            fy,
            " · Agent",
            Style::default().fg(TEXT_DIM),
        );
    }
}

fn board_footer_max(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = vec![Line::from(vec![
        Span::styled("> ", Style::default().fg(TEXT)),
        Span::styled(
            first_fitting_line(
                "Ship it — commit and push the rate limiter",
                w.saturating_sub(2),
            ),
            Style::default().fg(TEXT),
        ),
    ])];
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
        Span::styled(
            "Shell ",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            first_fitting_line(
                "git add -A && git commit && git push -u origin rate-limit-9e4d",
                w.saturating_sub(8),
            ),
            Style::default().fg(TEXT),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("✓ ", Style::default().fg(SUCCESS)),
        Span::styled(
            "Committed and pushed · 3 files · ",
            Style::default().fg(TEXT),
        ),
        Span::styled("+214", Style::default().fg(DIFF_ADD)),
        Span::styled(" ", Style::default().fg(TEXT_DIM)),
        Span::styled("-9", Style::default().fg(TEXT_DIM)),
    ]));
    lines.push(dim(first_fitting_line(
        "a4f21c9 · Add Redis sliding-window rate limiting to /v1/completions",
        w,
    )));
    lines.push(dim(first_fitting_line(
        "branch rate-limit-9e4d -> origin · open a PR with /pr",
        w,
    )));
    let body_h = area.height.saturating_sub(3);
    Paragraph::new(lines).render(Rect::new(area.x, area.y, area.width, body_h), buf);
    let composer_y = area.bottom().saturating_sub(3);
    buf.set_string(
        area.x,
        composer_y,
        "> Add a follow-up█",
        Style::default().fg(TEXT),
    );
    if let Some(cell) = buf.cell_mut((area.x, composer_y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('>');
    }
    buf.set_string(
        area.x,
        area.bottom().saturating_sub(2),
        first_fitting_line(
            if compact(area) {
                "& cloud · / commands · ! shell · shift+tab"
            } else {
                "/ commands · @ files · ! shell · & cloud · shift+tab modes"
            },
            w,
        ),
        Style::default().fg(TEXT_DIM),
    );

    let y = area.bottom().saturating_sub(1);
    let left = first_fitting_line("~/cortex-api rate-limit-9e4d", w);
    buf.set_string(area.x, y, &left, Style::default().fg(TEXT_DIM));
    let right_full = format!("{MODEL} · MAX · Agent · Smart · 38% context left");
    if left.chars().count() + 1 + right_full.chars().count() <= w {
        let prefix = format!("{MODEL} · ");
        let suffix = " · Agent · Smart · 38% context left";
        let rx = area
            .right()
            .saturating_sub(right_full.chars().count() as u16);
        buf.set_string(rx, y, &prefix, Style::default().fg(TEXT_DIM));
        buf.set_string(
            rx.saturating_add(prefix.chars().count() as u16),
            y,
            "MAX",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        );
        buf.set_string(
            rx.saturating_add(prefix.chars().count() as u16 + 3),
            y,
            suffix,
            Style::default().fg(TEXT_DIM),
        );
    } else {
        let rx = area.right().saturating_sub(3);
        if rx > area.x + left.chars().count() as u16 {
            buf.set_string(
                rx,
                y,
                "MAX",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            );
        }
    }
}

fn board_login(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    if !compact(area) {
        buf.set_string(area.x, y, "> cortex", Style::default().fg(TEXT));
        if let Some(cell) = buf.cell_mut((area.x, y)) {
            cell.set_style(Style::default().fg(SUCCESS));
            cell.set_char('>');
        }
        y += 2;
    }
    buf.set_string(
        area.x,
        y,
        first_fitting_line("Cortex CLI v1.0.0", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    y += 2;
    buf.set_string(
        area.x,
        y,
        first_fitting_line("Sign in to Cortex", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    y += 2;

    fill_row(buf, area, y, SELECTION_BG);
    buf.set_string(
        area.x,
        y,
        first_fitting_line("● Continue with browser", w),
        Style::default()
            .fg(TEXT)
            .bg(SELECTION_BG)
            .add_modifier(Modifier::BOLD),
    );
    y += 1;
    for part in wrap_or_drop(
        "Opens cortex.foundation/cli/auth — token never hits the model.",
        w,
    ) {
        if y >= area.bottom().saturating_sub(4) {
            break;
        }
        buf.set_string(area.x, y, &part, Style::default().fg(TEXT_DIM));
        y += 1;
    }
    buf.set_string(
        area.x,
        y,
        first_fitting_line("○ Paste an API key", w),
        Style::default().fg(TEXT),
    );
    paint_hints_and_footer(
        area,
        buf,
        "↑↓ select · ↵ continue · esc quit",
        &format!("{MODEL} · Agent"),
    );
}

fn board_thinking(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("⠇ ", Style::default().fg(TEXT_DIM)),
        Span::styled("Thinking", Style::default().fg(TEXT)),
    ]));
    for thought in [
        "Need a sliding window, not a fixed counter — bursts at minute boundaries would leak.",
        "ioredis sorted set per API key: ZADD now, ZREMRANGEBYSCORE older than window.",
        "Fail closed if Redis is down — don't let completions through unmetered.",
    ] {
        for part in wrap_or_drop(thought, w) {
            lines.push(dim(part));
            if compact(area) {
                break;
            }
        }
        if compact(area) {
            break;
        }
    }
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 97% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_todos(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
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
    if !compact(area) {
        for pending in [
            "Wire into POST /v1/completions",
            "Env + .env.example",
            "Integration tests with ioredis-mock",
        ] {
            lines.push(Line::from(vec![
                Span::styled("○ ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    first_fitting_line(pending, w.saturating_sub(2)),
                    Style::default().fg(TEXT_DIM),
                ),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("⠋ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line("Writing src/middleware/rateLimit.ts", w.saturating_sub(2)),
            Style::default().fg(TEXT),
        ),
    ]));
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 90% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_question(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = area.y;
    if !compact(area) {
        for line in user_prompt_lines(w, area) {
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
    y += 2;
    let options = [
        (false, "1", "Middleware on POST /v1/completions only"),
        (true, "2", "Shared limiter for every /v1/* route"),
        (false, "3", "Per-model limits, configured in the catalog"),
        (
            false,
            "4",
            "Skip for now - I'll point you at an existing helper",
        ),
    ];
    for (selected, number, label) in options {
        if y >= area.bottom().saturating_sub(3) {
            break;
        }
        let shown = first_fitting_line(label, w.saturating_sub(3));
        if selected {
            fill_row(buf, area, y, SELECTION_BG);
            buf.set_string(
                area.x,
                y,
                number,
                Style::default().fg(TEXT_DIM).bg(SELECTION_BG),
            );
            buf.set_string(
                area.x + 3,
                y,
                &shown,
                Style::default()
                    .fg(TEXT)
                    .bg(SELECTION_BG)
                    .add_modifier(Modifier::BOLD),
            );
        } else {
            buf.set_string(area.x, y, number, Style::default().fg(TEXT_DIM));
            buf.set_string(area.x + 3, y, &shown, Style::default().fg(TEXT));
        }
        y += 1;
    }
    paint_hints_and_footer(
        area,
        buf,
        "1-9 pick · ↑↓ move · ↵ confirm · esc skip",
        &format!("{MODEL} · Plan"),
    );
}

fn board_skills(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    buf.set_string(
        area.x,
        area.y,
        first_fitting_line("> /skills", w),
        Style::default().fg(TEXT),
    );
    if let Some(cell) = buf.cell_mut((area.x, area.y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('>');
    }
    buf.set_string(
        area.x,
        area.y + 1,
        first_fitting_line("/ Type to search skills", w),
        Style::default().fg(TEXT_DIM),
    );
    let rows = [
        (false, "/commit", "Stage, write a message, commit"),
        (true, "/pr", "Open a pull request with summary + test plan"),
        (false, "/review", "Review the current diff like a teammate"),
        (false, "/fix-ci", "Reproduce the failed check and patch it"),
        (false, "/migrate", "Draft a reversible database migration"),
    ];
    let mut y = area.y + 3;
    for (selected, cmd, desc) in rows {
        if y >= area.bottom().saturating_sub(3) {
            break;
        }
        let line = first_fitting_line(&format!("{cmd}  {desc}"), w);
        if selected {
            fill_row(buf, area, y, SELECTION_BG);
            buf.set_string(area.x, y, &line, Style::default().fg(TEXT).bg(SELECTION_BG));
        } else {
            buf.set_string(area.x, y, cmd, Style::default().fg(TEXT));
            buf.set_string(
                area.x.saturating_add(cmd.len() as u16 + 2),
                y,
                first_fitting_line(desc, w.saturating_sub(cmd.len() + 2)),
                Style::default().fg(TEXT_DIM),
            );
        }
        y += 1;
        if compact(area) && selected {
            break;
        }
    }
    buf.set_string(
        area.x,
        area.bottom().saturating_sub(2),
        first_fitting_line("↑↓ select · ↵ run once · ⌥↵ pin as mode · esc close", w),
        Style::default().fg(TEXT_DIM),
    );
    paint_footer(area, buf, &format!("{MODEL} · Agent"));
}

fn board_btw(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("⠇ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line(
                "Implementing the sliding-window limiter...",
                w.saturating_sub(2),
            ),
            Style::default().fg(TEXT),
        ),
    ]));
    lines.push(Line::from(""));
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
        Span::styled("█", Style::default().fg(TEXT)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line("not added to the main thread", w.saturating_sub(2)),
            Style::default().fg(TEXT_DIM),
        ),
    ]));
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 91% context"),
        "Add a follow-up ↵ to queue",
    );
}

/// Composer row with a block cursor ahead of the ghost, per the Designer
/// boards for interrupt and compact: violet `>`, white cursor, dim ghost.
fn paint_cursor_composer(area: Rect, buf: &mut Buffer, footer_right: &str) {
    let w = inner_width(area);
    let composer_y = area.bottom().saturating_sub(3);
    buf.set_string(area.x, composer_y, "> ", Style::default().fg(SUCCESS));
    buf.set_string(area.x + 2, composer_y, "█", Style::default().fg(TEXT));
    let ghost = first_fitting_line("Plan, search, build anything", w.saturating_sub(4));
    if !ghost.is_empty() {
        buf.set_string(
            area.x + 4,
            composer_y,
            &ghost,
            Style::default().fg(TEXT_DIM),
        );
    }
    paint_hints_and_footer(
        area,
        buf,
        "/ commands · @ files · ! shell · shift+tab modes",
        footer_right,
    );
}

/// State 37 — the run was interrupted: prompt and tool tiles stay on screen,
/// then `✗ Stopped`. Shared by the `interrupt` and `stopped` captures.
fn board_stopped(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = Vec::new();
    // Prompt per the Designer board: violet `>`, white copy on every line.
    if compact(area) {
        lines.push(Line::from(vec![
            Span::styled("> ", Style::default().fg(SUCCESS)),
            Span::styled(
                first_fitting_line(USER_PROMPT, w.saturating_sub(2)),
                Style::default().fg(TEXT),
            ),
        ]));
    } else {
        for (i, part) in wrap_or_drop(USER_PROMPT, w.saturating_sub(2))
            .into_iter()
            .enumerate()
        {
            if i == 0 {
                lines.push(Line::from(vec![
                    Span::styled("> ", Style::default().fg(SUCCESS)),
                    Span::styled(part, Style::default().fg(TEXT)),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(part, Style::default().fg(TEXT)),
                ]));
            }
        }
    }
    lines.push(Line::from(""));
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
            Span::styled("● ", Style::default().fg(SUCCESS)),
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
        Span::styled("✗ ", Style::default().fg(ERROR)),
        Span::styled(
            "Stopped",
            Style::default().fg(ERROR).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(dim(first_fitting_line("12s · 4.1k tokens · ctrl+c", w)));
    let body_h = area.height.saturating_sub(3);
    Paragraph::new(lines).render(Rect::new(area.x, area.y, area.width, body_h), buf);
    paint_cursor_composer(area, buf, &format!("{MODEL} · Agent · 94% context"));
}

/// State 38 — `/compact` result. Shared by the `compact` and `compacted`
/// captures; 12% is the violet success stat on the Designer board.
fn board_compacted(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("> ", Style::default().fg(SUCCESS)),
            Span::styled("/compact", Style::default().fg(TEXT)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            first_fitting_line("Thread compacted", w),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("Context  ", Style::default().fg(TEXT_DIM)),
            Span::styled("86%", Style::default().fg(TEXT)),
            Span::styled("  →  ", Style::default().fg(TEXT_DIM)),
            Span::styled("12%", Style::default().fg(SUCCESS)),
            Span::styled(" used", Style::default().fg(TEXT_DIM)),
        ]),
        dim(first_fitting_line(
            "Summary kept  ·  2.1k tokens kept  ·  files and todos are unchanged.",
            w,
        )),
    ];
    if compact(area) {
        lines.remove(1);
        lines.pop();
        lines.push(dim(first_fitting_line(
            "Summary kept · files and todos are unchanged.",
            w,
        )));
    }
    let body_h = area.height.saturating_sub(3);
    Paragraph::new(lines).render(Rect::new(area.x, area.y, area.width, body_h), buf);
    paint_cursor_composer(area, buf, &format!("{MODEL} · Agent · 88% context"));
}

fn board_write(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
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
    let take = if compact(area) { 1 } else { code.len() };
    for (no, line) in code.iter().take(take) {
        lines.extend(grep_hit_line(w, *no, line));
    }
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 86% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_clear_confirm(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    buf.set_string(
        area.x,
        area.y,
        first_fitting_line("> /clear", w),
        Style::default().fg(TEXT),
    );
    if let Some(cell) = buf.cell_mut((area.x, area.y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('>');
    }
    buf.set_string(
        area.x,
        area.y + 2,
        first_fitting_line("Start a new thread?", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    let mut y = area.y + 3;
    for part in wrap_or_drop(
        "The transcript is dropped. Git, files and config stay as they are.",
        w,
    ) {
        buf.set_string(area.x, y, &part, Style::default().fg(TEXT_DIM));
        y += 1;
    }
    y += 1;
    fill_row(buf, area, y, SELECTION_BG);
    buf.set_string(
        area.x,
        y,
        first_fitting_line("● Clear thread", w),
        Style::default()
            .fg(TEXT)
            .bg(SELECTION_BG)
            .add_modifier(Modifier::BOLD),
    );
    y += 1;
    buf.set_string(
        area.x,
        y,
        first_fitting_line("○ Cancel", w),
        Style::default().fg(TEXT),
    );
    paint_hints_and_footer(
        area,
        buf,
        "↑↓ select · ↵ confirm · esc cancel",
        &format!("{MODEL} · Agent"),
    );
}

const KW: Color = Color::Rgb(0x48, 0xCA, 0xE4);
const STR: Color = Color::Rgb(0xF5, 0xE6, 0x6E);
const NUM: Color = Color::Rgb(0xFF, 0xC8, 0x57);

fn paint_command_prompt(area: Rect, buf: &mut Buffer, command: &str) {
    let w = inner_width(area);
    buf.set_string(
        area.x,
        area.y,
        first_fitting_line(&format!("> {command}"), w),
        Style::default().fg(TEXT),
    );
    if let Some(cell) = buf.cell_mut((area.x, area.y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('>');
    }
}

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
                Style::default().fg(STR),
            ));
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            spans.push(Span::styled(
                chars[start..i].iter().collect::<String>(),
                Style::default().fg(NUM),
            ));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let color = if keywords.contains(&word.as_str()) {
                KW
            } else {
                TEXT
            };
            spans.push(Span::styled(word, Style::default().fg(color)));
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
    let wrapped = wrap_or_drop(code, rest_w.max(1));
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

fn board_grep(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
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
    let take = if compact(area) { 2 } else { hits.len() };
    for (no, code) in hits.iter().take(take) {
        lines.extend(grep_hit_line(w, *no, code));
    }
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 90% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_glob(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
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
    let take = if compact(area) { 3 } else { files.len() };
    for path in files.iter().take(take) {
        let fitted = first_fitting_line(path, w.saturating_sub(2));
        let mut spans = vec![Span::styled("  ", Style::default().fg(TEXT))];
        spans.extend(paint_match_path(&fitted, "rate"));
        lines.push(Line::from(spans));
    }
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 91% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_delete(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
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
    Paragraph::new(lines.clone()).render(
        Rect::new(area.x, area.y, area.width, area.height.saturating_sub(3)),
        buf,
    );
    let body_h = lines.len() as u16;
    let y = area.y + body_h.min(area.height.saturating_sub(4));
    fill_row(buf, area, y, SELECTION_BG);
    buf.set_string(
        area.x,
        y,
        first_fitting_line("● Delete", w),
        Style::default()
            .fg(TEXT)
            .bg(SELECTION_BG)
            .add_modifier(Modifier::BOLD),
    );
    buf.set_string(
        area.x,
        y.saturating_add(1),
        first_fitting_line("○ Keep", w),
        Style::default().fg(TEXT),
    );
    paint_hints_and_footer(
        area,
        buf,
        "↑↓ select  ·  ↵ confirm  ·  esc keep",
        &format!("{MODEL} · Agent"),
    );
}

fn board_list(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
        Span::styled(
            "List ",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            first_fitting_line("src/middleware 4 entries", w.saturating_sub(8)),
            Style::default().fg(TEXT_DIM),
        ),
    ]));
    for name in ["auth.ts", "rateLimit.ts", "cors.ts"] {
        lines.push(white(format!("  {name}")));
    }
    lines.push(Line::from(Span::styled(
        format!("  {}", first_fitting_line("internal/", w.saturating_sub(2))),
        Style::default().fg(TEXT),
    )));
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 93% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_fetch(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    let url = "https://redis.io/docs/latest/commands/zadd/";
    let url_fit = first_fitting_line(url, w.saturating_sub(8));
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
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
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 88% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_mcp_call(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
        Span::styled(
            first_fitting_line("MCP linear / list_issues", w.saturating_sub(2)),
            Style::default().fg(TEXT),
        ),
    ]));
    lines.push(dim("  team=API  state=started"));
    let rows = [
        ("  API-184  Rate limit 429 body", "  In Progress  you"),
        ("  API-191  Sliding window spike", "  In Progress  you"),
    ];
    for (item, status) in rows {
        let item_fit = first_fitting_line(item, w.saturating_sub(2));
        let status_fit = first_fitting_line(status, w.saturating_sub(item_fit.chars().count()));
        lines.push(Line::from(vec![
            Span::styled(item_fit, Style::default().fg(TEXT)),
            Span::styled(status_fit, Style::default().fg(TEXT_DIM)),
        ]));
        if compact(area) {
            break;
        }
    }
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 87% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_task(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
        Span::styled(
            "Task ",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            first_fitting_line("Write integration tests", w.saturating_sub(8)),
            Style::default().fg(TEXT),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  ⠇ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line("Running vitest · 18s", w.saturating_sub(4)),
            Style::default().fg(TEXT_DIM),
        ),
    ]));
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 89% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_diagnostics(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    let path = if compact(area) {
        "rateLimit.ts".to_string()
    } else {
        "src/middleware/rateLimit.ts".to_string()
    };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
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
    lines.push(Line::from(vec![
        Span::styled("  error ", Style::default().fg(ERROR)),
        Span::styled("L22  ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line(error_msg, w.saturating_sub(12)),
            Style::default().fg(TEXT),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  warn  ", Style::default().fg(WARNING)),
        Span::styled("L47  ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line(warn_msg, w.saturating_sub(12)),
            Style::default().fg(TEXT),
        ),
    ]));
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 86% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_edit(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    let path = if compact(area) {
        "completions.ts"
    } else {
        "src/server/routes/completions.ts"
    };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
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
    if !compact(area) {
        lines.push(dim(first_fitting_line(
            "  22  { preHandler: [requireApiKey, limiter] },",
            w,
        )));
        lines.push(Line::from(vec![
            Span::styled("  +   ", Style::default().fg(DIFF_ADD)),
            Span::styled(
                first_fitting_line(
                    "const limiter = rateLimit({ limit: 60, windowSec: 60 });",
                    w.saturating_sub(6),
                ),
                Style::default().fg(TEXT),
            ),
        ]));
    }
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 92% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_multi_diff(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    paint_command_prompt(area, buf, "/diff");
    buf.set_string(
        area.x,
        area.y + 2,
        first_fitting_line("Changed this turn", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    let header = first_fitting_line("Changed this turn", w);
    let count = " 4 files";
    let hx = area.x + header.chars().count() as u16;
    if (hx as usize) + count.trim().len() < w {
        buf.set_string(hx, area.y + 2, count, Style::default().fg(TEXT_DIM));
    }

    let files: &[(&str, &str, &str)] = &[
        ("src/middleware/rateLimit.ts", "+84", "-"),
        ("src/server/routes/completions.ts", "+9", "-2"),
        ("test/rateLimit.test.ts", "+61", "-"),
        (".env.example", "+2", "-"),
    ];
    let mut y = area.y + 3;
    for (i, (path, plus, minus)) in files.iter().enumerate() {
        if y + 2 >= area.bottom().saturating_sub(2) {
            break;
        }
        if compact(area) && i > 1 {
            break;
        }
        if i == 0 {
            fill_row(buf, area, y, SELECTION_BG);
        }
        let stats = format!("{plus} {minus}");
        let path_w = w.saturating_sub(stats.chars().count() + 1);
        let shown = first_fitting_line(path, path_w);
        let selected = i == 0;
        let with_row_bg = |style: Style| {
            if selected {
                style.bg(SELECTION_BG)
            } else {
                style
            }
        };
        buf.set_string(area.x, y, &shown, with_row_bg(Style::default().fg(TEXT)));
        let plus_x = area
            .right()
            .saturating_sub(stats.chars().count() as u16)
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
        "↑↓ select  ·  ↵ open  ·  esc close",
        &format!("{MODEL} · Agent"),
    );
}

fn board_settings_hub(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    paint_command_prompt(area, buf, "/settings");
    buf.set_string(
        area.x,
        area.y + 2,
        first_fitting_line("Settings", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    let rows: &[(&str, &str)] = &[
        ("Model", "cortex-1-mini · Medium"),
        ("Mode", "Agent"),
        ("Permissions", "Smart"),
        ("Sandbox", "On · workspace"),
        ("MCP", "3 of 4 connected"),
        ("Config", "~/.cortex/config.json"),
        ("Usage", "42 / 500 agent requests"),
    ];
    let mut y = area.y + 3;
    for (i, (label, value)) in rows.iter().enumerate() {
        if y >= area.bottom().saturating_sub(2) {
            break;
        }
        if i == 0 {
            fill_row(buf, area, y, SELECTION_BG);
        }
        let label_style = if i == 0 {
            Style::default()
                .fg(TEXT)
                .bg(SELECTION_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        buf.set_string(area.x, y, label, label_style);
        let shown = first_fitting_line(value, w.saturating_sub(label.chars().count() + 2));
        if !shown.is_empty() {
            let vx = area
                .right()
                .saturating_sub(shown.chars().count() as u16)
                .max(area.x + label.chars().count() as u16 + 2);
            let value_style = if i == 0 {
                Style::default().fg(TEXT).bg(SELECTION_BG)
            } else {
                Style::default().fg(TEXT_DIM)
            };
            buf.set_string(vx, y, &shown, value_style);
        }
        y += 1;
    }
    paint_hints_and_footer(
        area,
        buf,
        "↑↓ select · ↵ open · esc close",
        &format!("{MODEL} · Agent"),
    );
}

fn paint_accent_prompt(area: Rect, buf: &mut Buffer, y: u16, rest: &str, dim_rest: bool) {
    let w = inner_width(area);
    if let Some(cell) = buf.cell_mut((area.x, y)) {
        cell.set_style(Style::default().fg(SUCCESS));
        cell.set_char('>');
    }
    if rest.is_empty() {
        return;
    }
    let shown = first_fitting_line(rest, w.saturating_sub(2));
    if shown.is_empty() {
        return;
    }
    let fg = if dim_rest { TEXT_DIM } else { TEXT };
    buf.set_string(area.x + 2, y, &shown, Style::default().fg(fg));
}

/// History chrome shared by splash and slash: cwd/git, `> cortex`, version.
fn paint_launch_header(area: Rect, buf: &mut Buffer) -> u16 {
    let w = inner_width(area);
    let mut y = area.y;
    buf.set_string(
        area.x,
        y,
        first_fitting_line(&format!("{CWD} {GIT}"), w),
        Style::default().fg(TEXT_DIM),
    );
    y += 1;
    paint_accent_prompt(area, buf, y, "cortex", false);
    y += 1;
    buf.set_string(
        area.x,
        y,
        first_fitting_line("Cortex CLI v1.0.0", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    y + 2
}

fn board_splash(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let y = paint_launch_header(area, buf);
    let ghost = first_fitting_line("Plan, search, build anything", w.saturating_sub(2));
    paint_accent_prompt(area, buf, y, "", false);
    if !ghost.is_empty() {
        buf.set_string(area.x + 2, y, &ghost, Style::default().fg(TEXT_DIM));
    }
    paint_hints_and_footer(
        area,
        buf,
        "/ commands · @ files · ! shell · shift+tab modes",
        &format!("{MODEL} · Agent · 100% context"),
    );
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

fn board_palette(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = paint_launch_header(area, buf);
    paint_accent_prompt(area, buf, y, "/", false);
    y += if compact(area) { 1 } else { 2 };
    let hint_reserve = 2u16;
    let two_line = compact(area);
    let take = if compact(area) { 2 } else { PALETTE_ROWS.len() };
    let mut shown = 0usize;
    for (i, (cmd, desc)) in PALETTE_ROWS.iter().enumerate() {
        if shown >= take {
            break;
        }
        if y + hint_reserve >= area.bottom() {
            break;
        }
        if i == 0 {
            fill_row(buf, area, y, SELECTION_BG);
            buf.set_string(
                area.x,
                y,
                "> ",
                Style::default().fg(SUCCESS).bg(SELECTION_BG),
            );
        }
        let cmd_style = if i == 0 {
            Style::default()
                .fg(TEXT)
                .bg(SELECTION_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        let name = first_fitting_line(cmd, w.saturating_sub(2));
        buf.set_string(area.x + 2, y, &name, cmd_style);
        let gap = 2 + name.chars().count() + 2;
        let same_line = first_fitting_line(desc, w.saturating_sub(gap));
        if !same_line.is_empty() && !two_line {
            // Selected row keeps the 40×12 tone: bold white label, dim
            // description. Only the `>` marker is violet — never the copy.
            let desc_style = if i == 0 {
                Style::default().fg(TEXT_DIM).bg(SELECTION_BG)
            } else {
                Style::default().fg(TEXT_DIM)
            };
            buf.set_string(area.x + gap as u16, y, &same_line, desc_style);
        }
        y += 1;
        shown += 1;
        if two_line && y + hint_reserve < area.bottom() {
            let wrapped = wrap_or_drop(desc, w.saturating_sub(4));
            if let Some(line) = wrapped.first() {
                buf.set_string(
                    area.x,
                    y,
                    format!("    {line}"),
                    Style::default().fg(TEXT_DIM),
                );
                y += 1;
            }
        }
    }
    let remaining = 21usize.saturating_sub(shown);
    if remaining > 0 && y + hint_reserve < area.bottom() {
        buf.set_string(
            area.x,
            y,
            first_fitting_line(&format!("{remaining} more — keep typing to filter"), w),
            Style::default().fg(TEXT_DIM),
        );
    }
    paint_hints_and_footer(
        area,
        buf,
        "↑↓ select  ·  ↵ run  ·  tab complete  ·  esc close",
        &format!("{MODEL} · Agent"),
    );
}

fn board_typing(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut y = paint_launch_header(area, buf);
    let typed = format!("{USER_PROMPT}█");
    if compact(area) {
        let fitted = first_fitting_line(USER_PROMPT, w.saturating_sub(3));
        paint_accent_prompt(area, buf, y, &format!("{fitted}█"), false);
    } else {
        for (i, part) in wrap_or_drop(&typed, w.saturating_sub(2))
            .into_iter()
            .enumerate()
        {
            if y + 2 >= area.bottom() {
                break;
            }
            if i == 0 {
                paint_accent_prompt(area, buf, y, &part, false);
            } else {
                buf.set_string(area.x + 2, y, &part, Style::default().fg(TEXT));
            }
            y += 1;
        }
    }
    paint_hints_and_footer(
        area,
        buf,
        "/ commands · @ files · ! shell · shift+tab modes",
        &format!("{MODEL} · Agent · 100% context"),
    );
}

/// One picker row: `>` marker plus bold label on the dark bar when selected,
/// dim meta right-aligned. Violet never touches the label itself.
fn picker_row(area: Rect, buf: &mut Buffer, y: u16, selected: bool, label: &str, meta: &str) {
    let w = inner_width(area);
    if selected {
        fill_row(buf, area, y, SELECTION_BG);
        buf.set_string(
            area.x,
            y,
            "> ",
            Style::default().fg(SUCCESS).bg(SELECTION_BG),
        );
    }
    let label_style = if selected {
        Style::default()
            .fg(TEXT)
            .bg(SELECTION_BG)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT)
    };
    let name = first_fitting_line(label, w.saturating_sub(2));
    buf.set_string(area.x + 2, y, &name, label_style);
    let shown = first_fitting_line(meta, w.saturating_sub(name.chars().count() + 4));
    if shown.is_empty() {
        return;
    }
    let vx = area
        .right()
        .saturating_sub(shown.chars().count() as u16)
        .max(area.x + name.chars().count() as u16 + 4);
    let meta_style = if selected {
        Style::default().fg(TEXT_DIM).bg(SELECTION_BG)
    } else {
        Style::default().fg(TEXT_DIM)
    };
    buf.set_string(vx, y, &shown, meta_style);
}

const MODEL_ROWS: &[(bool, &str, &str, &str)] = &[
    (
        true,
        "cortex-1-mini",
        "Medium · current",
        "Fast default for everyday coding.",
    ),
    (
        false,
        "cortex-1",
        "High",
        "Deeper reasoning for hard changes.",
    ),
    (
        false,
        "cortex-1-max",
        "MAX · token billing",
        "Longest context — bills by token instead of per request.",
    ),
];

fn board_model_compact(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    paint_command_prompt(area, buf, "/model");
    buf.set_string(
        area.x,
        area.y + 2,
        first_fitting_line("Model", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    let mut y = area.y + 3;
    for (selected, name, meta, _detail) in MODEL_ROWS {
        if y >= area.bottom().saturating_sub(2) {
            break;
        }
        picker_row(area, buf, y, *selected, name, meta);
        y += 1;
    }
    paint_hints_and_footer(
        area,
        buf,
        "↑↓ select · ↵ confirm · tab effort · esc close",
        &format!("{MODEL} · Agent"),
    );
}

fn board_model_full(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    paint_command_prompt(area, buf, "/model");
    buf.set_string(
        area.x,
        area.y + 2,
        first_fitting_line("Model", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    let mut y = area.y + 3;
    for (selected, name, meta, detail) in MODEL_ROWS {
        if y >= area.bottom().saturating_sub(3) {
            break;
        }
        picker_row(area, buf, y, *selected, name, meta);
        y += 1;
        if !compact(area) {
            buf.set_string(
                area.x + 2,
                y,
                first_fitting_line(detail, w.saturating_sub(2)),
                Style::default().fg(TEXT_DIM),
            );
            y += 1;
        }
    }
    if !compact(area) {
        y += 1;
        buf.set_string(
            area.x,
            y,
            first_fitting_line("Effort", w),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        );
        y += 1;
        buf.set_string(
            area.x,
            y,
            first_fitting_line("○ Low   ● Medium   ○ High", w),
            Style::default().fg(TEXT),
        );
        y += 2;
        for part in wrap_or_drop(
            "MAX bills by token instead of per request — manage at cortex.foundation/billing",
            w,
        ) {
            if y >= area.bottom().saturating_sub(3) {
                break;
            }
            buf.set_string(area.x, y, &part, Style::default().fg(TEXT_DIM));
            y += 1;
        }
    }
    paint_hints_and_footer(
        area,
        buf,
        "↑↓ select · ↵ confirm · tab effort · esc close",
        &format!("{MODEL} · Agent"),
    );
}

fn board_mode(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    paint_command_prompt(area, buf, "/mode");
    buf.set_string(
        area.x,
        area.y + 2,
        first_fitting_line("Mode", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    let rows = [
        (true, "● Agent", "edits files and runs commands"),
        (false, "○ Plan", "draft an approach first — no edits"),
        (false, "○ Ask", "read-only answers about the codebase"),
    ];
    let mut y = area.y + 3;
    for (selected, label, desc) in rows {
        if y >= area.bottom().saturating_sub(2) {
            break;
        }
        if selected {
            fill_row(buf, area, y, SELECTION_BG);
        }
        let label_style = if selected {
            Style::default()
                .fg(TEXT)
                .bg(SELECTION_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        buf.set_string(area.x, y, label, label_style);
        let gap = label.chars().count() + 2;
        let shown = first_fitting_line(desc, w.saturating_sub(gap));
        if !shown.is_empty() {
            let desc_style = if selected {
                Style::default().fg(TEXT_DIM).bg(SELECTION_BG)
            } else {
                Style::default().fg(TEXT_DIM)
            };
            buf.set_string(area.x + gap as u16, y, &shown, desc_style);
        }
        y += 1;
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
        "↑↓ select · ↵ confirm · esc close",
        &format!("{MODEL} · Agent"),
    );
}

fn board_permissions(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    paint_command_prompt(area, buf, "/permissions");
    buf.set_string(
        area.x,
        area.y + 2,
        first_fitting_line("Permissions", w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    let rows = [
        (false, "Read-only", "never edit files or run commands"),
        (true, "Smart", "auto-approve safe reads — ask before edits"),
        (false, "Full access", "only ask when leaving the sandbox"),
    ];
    let mut y = area.y + 3;
    for (selected, label, desc) in rows {
        if y >= area.bottom().saturating_sub(2) {
            break;
        }
        picker_row(area, buf, y, selected, label, desc);
        y += 1;
    }
    if !compact(area) {
        y += 1;
        for part in wrap_or_drop(
            "Applies to this project — overrides live in .cortex/config.json.",
            w,
        ) {
            if y >= area.bottom().saturating_sub(3) {
                break;
            }
            buf.set_string(area.x, y, &part, Style::default().fg(TEXT_DIM));
            y += 1;
        }
    }
    paint_hints_and_footer(
        area,
        buf,
        "↑↓ select · ↵ confirm · esc close",
        &format!("{MODEL} · Agent · Smart"),
    );
}

fn board_working(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("⠇ ", Style::default().fg(TEXT_DIM)),
        Span::styled(
            first_fitting_line(
                "Working — wiring the limiter into completions",
                w.saturating_sub(2),
            ),
            Style::default().fg(TEXT),
        ),
    ]));
    lines.push(dim(first_fitting_line(
        "1m 12s · 8.2k tokens · esc to interrupt",
        w,
    )));
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 92% context"),
        "Add a follow-up ↵ to queue",
    );
}

fn board_read(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    let path = if compact(area) {
        "completions.ts"
    } else {
        "src/server/routes/completions.ts"
    };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(SUCCESS)),
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
    let take = if compact(area) { 2 } else { excerpt.len() };
    for (no, code) in excerpt.iter().take(take) {
        lines.extend(grep_hit_line(w, *no, code));
    }
    paint_lines(
        area,
        buf,
        lines,
        &format!("{MODEL} · Agent · 95% context"),
        "Add a follow-up ↵ to queue",
    );
}
