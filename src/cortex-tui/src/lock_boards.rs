//! Pixel-lock painters for boards 11–20.
//!
//! These scenes share session chrome (prompt, composer, cwd+git footer) and
//! Cortex product copy only.

use cortex_core::style::{ERROR, INFO, SUCCESS, TEXT, TEXT_DIM, VOID, WARNING};
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
const PASS: Color = Color::Rgb(0x22, 0xC5, 0x5E);
const AUTH_ORANGE: Color = Color::Rgb(0xFF, 0x8C, 0x32);
const COMMAND_BG: Color = Color::Rgb(0x1C, 0x1C, 0x20);

/// True when `id` is a board 11–20 lock scene.
pub fn is_lock_board(id: &str) -> bool {
    matches!(
        id,
        "shell"
            | "permission"
            | "plan"
            | "streaming"
            | "resume"
            | "mcp"
            | "usage"
            | "quota"
            | "sandbox"
            | "cloud"
    )
}

/// Paint one board into `area`.
pub fn render_lock_board(id: &str, area: Rect, buf: &mut Buffer) {
    match id {
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
            cell.set_fg(VOID);
        }
    }
}

fn board_shell(area: Rect, buf: &mut Buffer) {
    let w = inner_width(area);
    let mut lines = user_prompt_lines(w, area);
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(WARNING)),
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
            Span::styled("  ✓ ", Style::default().fg(PASS)),
            Span::styled(
                first_fitting_line(
                    "test/rateLimit.test.ts rejects a 61st request in the window  412ms",
                    w.saturating_sub(4),
                ),
                Style::default().fg(TEXT_DIM),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  ✓ ", Style::default().fg(PASS)),
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
        Span::styled("  ⠇ ", Style::default().fg(WARNING)),
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
        cell.set_style(Style::default().fg(WARNING));
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
        let text = if selected {
            format!("> {label}")
        } else {
            format!("  {label}")
        };
        let shown = first_fitting_line(&text, w);
        if selected {
            fill_row(buf, area, y, SUCCESS);
            buf.set_string(
                area.x,
                y,
                &shown,
                Style::default()
                    .fg(VOID)
                    .bg(SUCCESS)
                    .add_modifier(Modifier::BOLD),
            );
        } else {
            buf.set_string(area.x, y, &shown, Style::default().fg(TEXT));
        }
        y += 1;
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

    let body_h = area.height.saturating_sub(4);
    Paragraph::new(lines).render(Rect::new(area.x, area.y, area.width, body_h), buf);

    let y = area.bottom().saturating_sub(4);
    let yes = first_fitting_line("> Yes, switch to Agent mode and implement", w);
    fill_row(buf, area, y, SUCCESS);
    buf.set_string(
        area.x,
        y,
        &yes,
        Style::default()
            .fg(VOID)
            .bg(SUCCESS)
            .add_modifier(Modifier::BOLD),
    );
    buf.set_string(
        area.x,
        y.saturating_add(1),
        first_fitting_line("  No, keep planning — tell Cortex what to change", w),
        Style::default().fg(TEXT),
    );
    buf.set_string(
        area.x,
        y.saturating_add(2),
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
            Span::styled(label.to_string(), Style::default().fg(SUCCESS)),
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
            fill_row(buf, area, y, SUCCESS);
            let shown = if compact(area) {
                first_fitting_line(&format!("> {when}  Rate limiting  {msgs}"), w)
            } else {
                first_fitting_line(&format!("> {when}  {title}  {branch}  {msgs}"), w)
            };
            buf.set_string(area.x, y, &shown, Style::default().fg(VOID).bg(SUCCESS));
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
    let servers = [
        (
            SUCCESS,
            "github",
            "connected",
            "12 tools · repos, issues, pull requests",
        ),
        (
            SUCCESS,
            "postgres",
            "connected",
            "6 tools · localhost:5432/cortex",
        ),
        (
            AUTH_ORANGE,
            "sentry",
            "authenticating",
            "waiting for browser sign-in...",
        ),
        (
            ERROR,
            "linear",
            "failed",
            "connection refused — retrying in 30s",
        ),
    ];
    for (color, name, status, detail) in servers {
        if compact(area) && matches!(name, "postgres" | "sentry") {
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled("● ", Style::default().fg(color)),
            Span::styled(format!("{name}  "), Style::default().fg(TEXT)),
            Span::styled(first_fitting_line(status, 16), Style::default().fg(color)),
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
            WARNING,
        ),
        ("Tokens this month", bar(84, 120), "8.4M / 12M", "", INFO),
        ("Cloud agent minutes", bar(132, 400), "132 / 400", "", INFO),
    ];
    for (label, blocks, nums, extra, color) in rows {
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
            Style::default().fg(color),
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
        Span::styled("x ", Style::default().fg(ERROR)),
        Span::styled(
            "Agent quota exhausted",
            Style::default().fg(ERROR).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(white(first_fitting_line("Agent requests", w)));
    lines.push(Line::from(Span::styled(
        first_fitting_line(&format!("{}  500 / 500", bar(500, 500)), w),
        Style::default().fg(ERROR),
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
    fill_row(buf, area, y, SUCCESS);
    let row = first_fitting_line("> Sandbox mode", w.saturating_sub(8));
    buf.set_string(
        area.x,
        y,
        &row,
        Style::default()
            .fg(VOID)
            .bg(SUCCESS)
            .add_modifier(Modifier::BOLD),
    );
    let on = "● On";
    let ox = area.right().saturating_sub(on.len() as u16 + 1);
    if ox > area.x + 4 {
        buf.set_string(ox, y, on, Style::default().fg(VOID).bg(SUCCESS));
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
        Span::styled("> ", Style::default().fg(SUCCESS)),
        Span::styled(
            first_fitting_line(
                "& fix the flaky rateLimit integration test on CI",
                w.saturating_sub(2),
            ),
            Style::default().fg(TEXT),
        ),
    ])];
    lines.push(Line::from(vec![
        Span::styled("↑ ", Style::default().fg(INFO)),
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
