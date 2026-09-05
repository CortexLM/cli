//! Lock v2 session chrome: inky fill, token chip, dual-hairline composer,
//! contextual footer shortcut strip.

use cortex_core::style::{ACCENT, BAR_HOVER, BORDER_FOCUS, HAIRLINE, TEXT, TEXT_DIM, VOID};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::text_utils::{first_fitting_line, model_display_name};

/// One-column gutter on each side of the composer box and user bars.
pub fn content_gutter(compact: bool, width: u16) -> u16 {
    if compact || width < 20 { 0 } else { 1 }
}

/// Fill `area` with the inky `#000000` background.
pub fn fill_inky(area: Rect, buf: &mut Buffer) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_bg(VOID);
                cell.set_fg(TEXT);
                cell.set_char(' ');
            }
        }
    }
}

/// Format `{used} / {window}` with K/M suffixes.
pub fn format_token_counter(used: u64, window: u64) -> String {
    format!("{} / {}", compact_count(used), compact_count(window))
}

fn compact_count(n: u64) -> String {
    if n >= 1_000_000 {
        let m = n as f64 / 1_000_000.0;
        if m.fract() < 0.05 {
            format!("{}M", m as u64)
        } else {
            format!("{m:.1}M")
        }
    } else if n >= 1000 {
        let k = n as f64 / 1000.0;
        if (n >= 10_000 && k.fract() < 0.05) || n.is_multiple_of(1000) {
            format!("{}K", k as u64)
        } else {
            format!("{:.0}K", k)
        }
    } else {
        n.to_string()
    }
}

/// Right-align the token counter on row `y`, one column in from the edge.
pub fn paint_token_counter(
    area: Rect,
    y: u16,
    buf: &mut Buffer,
    used: u64,
    window: u64,
    warn: bool,
) {
    if area.width == 0 || y >= area.bottom() {
        return;
    }
    let text = format_token_counter(used, window);
    let len = text.chars().count() as u16;
    let x = area.right().saturating_sub(len + 1).max(area.x);
    let style = if warn {
        Style::default().fg(cortex_core::style::WARNING)
    } else {
        Style::default().fg(TEXT_DIM)
    };
    buf.set_string(x, y, &text, style);
}

/// 12-hour `h:mm AM` clock.
pub fn format_clock_12h(hour: u32, minute: u32) -> String {
    use chrono::Timelike;
    let (is_pm, h12) = chrono::NaiveTime::from_hms_opt(hour, minute, 0)
        .map(|t| t.hour12())
        .unwrap_or((false, 12));
    let h = if h12 == 0 { 12 } else { h12 };
    format!("{}:{:02} {}", h, minute, if is_pm { "PM" } else { "AM" })
}

/// Local now as `h:mm AM`.
pub fn now_clock_12h() -> String {
    use chrono::Timelike;
    let now = chrono::Local::now();
    format_clock_12h(now.hour(), now.minute())
}

/// Format a duration like `0.4s` / `4.6s` / `12s`.
pub fn format_secs(secs: f32) -> String {
    if secs < 10.0 {
        let rounded = (secs * 10.0).round() / 10.0;
        if (rounded - rounded.round()).abs() < f32::EPSILON {
            format!("{}s", rounded as i32)
        } else {
            format!("{rounded:.1}s")
        }
    } else {
        format!("{}s", secs.round() as i32)
    }
}

/// Mode chip in the composer top hairline.
pub fn mode_chip(label: &str) -> (String, bool) {
    match label.trim() {
        "Plan" => ("Plan · no edits".into(), true),
        "Ask" => ("Ask · read-only".into(), true),
        "Bash" => ("Bash · runs in your shell".into(), true),
        other => (other.to_string(), false),
    }
}

/// Model chip in the composer bottom hairline, e.g. `Cortex Mini 1 (medium)`.
pub fn model_chip(model: &str, effort: Option<&str>) -> String {
    let name = model_display_name(model);
    match effort {
        Some(e) if !e.is_empty() => format!("{name} ({})", e.to_ascii_lowercase()),
        _ => name,
    }
}

/// Paint the dual-hairline rounded composer box into `area` (3+ rows).
///
/// `area` is the full-width strip; the box is inset by one column.
pub fn paint_composer_box(
    area: Rect,
    buf: &mut Buffer,
    mode_label: &str,
    model_chip_text: &str,
    hovered: bool,
    focused: bool,
) {
    if area.height < 3 || area.width < 8 {
        return;
    }
    let hair = if hovered { BORDER_FOCUS } else { HAIRLINE };
    let rule = Style::default().fg(hair);
    let gutter = 1u16.min(area.width.saturating_sub(4));
    let x = area.x + gutter;
    let w = area.width.saturating_sub(gutter * 2).max(4);
    let top_y = area.y;
    let mid_y = area.y + 1;
    let bot_y = area.y + area.height.saturating_sub(1);

    let (chip, chip_emph) = mode_chip(mode_label);
    let chip_style = if chip_emph {
        Style::default().fg(TEXT)
    } else {
        Style::default().fg(TEXT_DIM)
    };

    // Top: ╭─ chip ─…╮
    buf.set_string(x, top_y, "╭", rule);
    buf.set_string(x + 1, top_y, "─", rule);
    let chip_x = x + 3;
    let chip_shown = first_fitting_line(&chip, w.saturating_sub(6) as usize);
    buf.set_string(chip_x, top_y, &chip_shown, chip_style);
    let after_chip = chip_x + chip_shown.chars().count() as u16;
    for col in after_chip..x + w.saturating_sub(1) {
        buf.set_string(col, top_y, "─", rule);
    }
    buf.set_string(x + w.saturating_sub(1), top_y, "╮", rule);
    // Space before chip
    buf.set_string(x + 2, top_y, " ", rule);

    // Sides
    for y in (mid_y)..bot_y {
        buf.set_string(x, y, "│", rule);
        buf.set_string(x + w.saturating_sub(1), y, "│", rule);
    }

    // Bottom: ╰─… chip ─╯
    buf.set_string(x, bot_y, "╰", rule);
    buf.set_string(x + w.saturating_sub(1), bot_y, "╯", rule);
    let chip_shown = first_fitting_line(model_chip_text, w.saturating_sub(4) as usize);
    let chip_len = chip_shown.chars().count() as u16;
    let chip_start = x + w.saturating_sub(3 + chip_len).max(x + 1);
    for col in (x + 1)..chip_start.saturating_sub(1) {
        buf.set_string(col, bot_y, "─", rule);
    }
    if chip_start > x + 1 {
        buf.set_string(chip_start.saturating_sub(1), bot_y, " ", rule);
    }
    buf.set_string(
        chip_start,
        bot_y,
        &chip_shown,
        Style::default().fg(TEXT_DIM),
    );
    let after = chip_start + chip_len;
    if after < x + w.saturating_sub(1) {
        buf.set_string(after, bot_y, " ", rule);
    }
    for col in (after + 1)..(x + w.saturating_sub(1)) {
        buf.set_string(col, bot_y, "─", rule);
    }

    let _ = focused; // caret colour is the caller's job
}

/// Inner input rect of the composer box (inside the `│` sides).
pub fn composer_inner(area: Rect) -> Rect {
    let gutter = 1u16.min(area.width.saturating_sub(4));
    Rect::new(
        area.x + gutter + 1,
        area.y + 1,
        area.width.saturating_sub(gutter * 2).saturating_sub(2),
        area.height.saturating_sub(2).max(1),
    )
}

/// Footer shortcut pair.
#[derive(Debug, Clone, Copy)]
pub struct FooterHint {
    pub key: &'static str,
    pub label: &'static str,
}

/// Contextual footer sets from SPEC §3.8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterSet {
    Idle,
    Typed,
    TypedNarrow,
    Running,
    Queue,
    ModelList,
    Effort,
    Approval,
    Mcp,
    Plugins,
    Resume,
    Bash,
    Unavailable,
    Palette,
}

impl FooterSet {
    pub fn hints(self) -> &'static [FooterHint] {
        match self {
            Self::Idle => &[
                FooterHint {
                    key: "Shift+Tab",
                    label: "mode",
                },
                FooterHint {
                    key: "Ctrl+x",
                    label: "shortcuts",
                },
            ],
            Self::Typed => &[
                FooterHint {
                    key: "Enter",
                    label: "send",
                },
                FooterHint {
                    key: "Alt+Enter",
                    label: "newline",
                },
                FooterHint {
                    key: "Shift+Tab",
                    label: "mode",
                },
                FooterHint {
                    key: "Ctrl+x",
                    label: "shortcuts",
                },
            ],
            Self::TypedNarrow => &[
                FooterHint {
                    key: "Enter",
                    label: "send",
                },
                FooterHint {
                    key: "Ctrl+x",
                    label: "shortcuts",
                },
            ],
            Self::Running => &[
                FooterHint {
                    key: "Esc",
                    label: "interrupt",
                },
                FooterHint {
                    key: "Enter",
                    label: "queue follow-up",
                },
                FooterHint {
                    key: "Ctrl+x",
                    label: "shortcuts",
                },
            ],
            Self::Queue => &[
                FooterHint {
                    key: "Enter",
                    label: "queue",
                },
                FooterHint {
                    key: "↑",
                    label: "edit queued",
                },
                FooterHint {
                    key: "Ctrl+c",
                    label: "stop",
                },
                FooterHint {
                    key: "Ctrl+x",
                    label: "shortcuts",
                },
            ],
            Self::ModelList => &[
                FooterHint {
                    key: "Enter",
                    label: "choose",
                },
                FooterHint {
                    key: "Tab",
                    label: "effort",
                },
                FooterHint {
                    key: "↑↓",
                    label: "select",
                },
                FooterHint {
                    key: "Esc",
                    label: "close",
                },
            ],
            Self::Effort => &[
                FooterHint {
                    key: "Enter",
                    label: "apply",
                },
                FooterHint {
                    key: "Tab",
                    label: "back to models",
                },
                FooterHint {
                    key: "Esc",
                    label: "close",
                },
            ],
            Self::Approval => &[
                FooterHint {
                    key: "↑↓",
                    label: "select",
                },
                FooterHint {
                    key: "Enter",
                    label: "confirm",
                },
                FooterHint {
                    key: "e",
                    label: "edit command",
                },
                FooterHint {
                    key: "Esc",
                    label: "cancel",
                },
            ],
            Self::Mcp => &[
                FooterHint {
                    key: "Enter",
                    label: "details",
                },
                FooterHint {
                    key: "r",
                    label: "reconnect",
                },
                FooterHint {
                    key: "a",
                    label: "add server",
                },
                FooterHint {
                    key: "Esc",
                    label: "close",
                },
            ],
            Self::Plugins => &[
                FooterHint {
                    key: "Enter",
                    label: "toggle",
                },
                FooterHint {
                    key: "i",
                    label: "install",
                },
                FooterHint {
                    key: "u",
                    label: "update",
                },
                FooterHint {
                    key: "Esc",
                    label: "close",
                },
            ],
            Self::Resume => &[
                FooterHint {
                    key: "Enter",
                    label: "resume",
                },
                FooterHint {
                    key: "f",
                    label: "favorite",
                },
                FooterHint {
                    key: "d",
                    label: "delete",
                },
                FooterHint {
                    key: "Esc",
                    label: "close",
                },
            ],
            Self::Bash => &[
                FooterHint {
                    key: "Enter",
                    label: "run",
                },
                FooterHint {
                    key: "Esc",
                    label: "leave bash",
                },
                FooterHint {
                    key: "Ctrl+x",
                    label: "shortcuts",
                },
            ],
            Self::Unavailable => &[
                FooterHint {
                    key: "Enter",
                    label: "retry",
                },
                FooterHint {
                    key: "Ctrl+x",
                    label: "shortcuts",
                },
            ],
            Self::Palette => &[
                FooterHint {
                    key: "Enter",
                    label: "send",
                },
                FooterHint {
                    key: "Alt+Enter",
                    label: "newline",
                },
                FooterHint {
                    key: "Shift+Tab",
                    label: "mode",
                },
                FooterHint {
                    key: "Ctrl+x",
                    label: "shortcuts",
                },
            ],
        }
    }
}

/// Paint the shortcut strip. `hovered` is the chunk index under the mouse.
pub fn paint_footer(area: Rect, buf: &mut Buffer, set: FooterSet, hovered: Option<usize>) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let wide = area.width >= 80;
    let sep = if wide { "  |  " } else { " | " };
    let hints = set.hints();
    let mut x = area.x + 1;
    let y = area.y;
    for (i, hint) in hints.iter().enumerate() {
        if i > 0 {
            let sep_shown = first_fitting_line(sep, area.right().saturating_sub(x) as usize);
            if sep_shown.is_empty() {
                break;
            }
            buf.set_string(x, y, &sep_shown, Style::default().fg(TEXT_DIM));
            x = x.saturating_add(sep_shown.chars().count() as u16);
        }
        let chunk = format!("{}:{}", hint.key, hint.label);
        let remain = area.right().saturating_sub(x) as usize;
        if remain == 0 {
            break;
        }
        let shown = first_fitting_line(&chunk, remain);
        if shown.is_empty() {
            break;
        }
        let hover = hovered == Some(i);
        let key_style = if hover {
            Style::default()
                .fg(TEXT)
                .bg(BAR_HOVER)
                .add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
        } else {
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
        };
        let label_style = if hover {
            Style::default().fg(TEXT).bg(BAR_HOVER)
        } else {
            Style::default().fg(TEXT_DIM)
        };
        let key = format!("{}:", hint.key);
        let key_len = key.chars().count().min(remain);
        buf.set_string(
            x,
            y,
            key.chars().take(key_len).collect::<String>(),
            key_style,
        );
        x = x.saturating_add(key_len as u16);
        let rest = remain.saturating_sub(key_len);
        if rest > 0 {
            let lab = first_fitting_line(hint.label, rest);
            let lab_len = lab.chars().count() as u16;
            buf.set_string(x, y, lab, label_style);
            x = x.saturating_add(lab_len);
        }
    }
}

/// True when the composer caret should be violet (keyboard focus).
pub fn composer_caret_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_DIM)
    }
}

/// Hover bar fill — `#1A1A1A`, no violet.
pub fn paint_hover_bar(area: Rect, buf: &mut Buffer) {
    for dx in 0..area.width {
        if let Some(cell) = buf.cell_mut((area.x + dx, area.y)) {
            cell.set_bg(BAR_HOVER);
        }
    }
}

/// Terms / privacy URLs for the opt-in banner.
pub const TERMS_URL: &str = "https://cortex.foundation/terms";
pub const PRIVACY_URL: &str = "https://cortex.foundation/privacy";

/// "Help improve Cortex" banner (SPEC §3.6).
pub fn paint_opt_in_banner(area: Rect, buf: &mut Buffer, hover: Option<u8>, focus: Option<u8>) {
    if area.height == 0 || area.width < 20 {
        return;
    }
    let y0 = area.y;
    buf.set_string(
        area.x + 1,
        y0,
        "Help improve Cortex",
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    let opt_out = "[Opt out]";
    let opt_in = "[Opt in]";
    let right = area.right().saturating_sub(1);
    let in_x = right.saturating_sub(opt_in.len() as u16);
    let out_x = in_x.saturating_sub(opt_out.len() as u16 + 2);
    let out_style = banner_btn_style(hover == Some(0), focus == Some(0), false);
    let in_style = banner_btn_style(hover == Some(1), focus == Some(1), true);
    if out_x > area.x + 20 {
        buf.set_string(out_x, y0, opt_out, out_style);
        buf.set_string(in_x, y0, opt_in, in_style);
    }
    if area.height >= 2 {
        let body = "Off by default. Opt in to let Cortex retain coding data — prompts, traces & metrics — to improve the product.";
        let shown = first_fitting_line(body, area.width.saturating_sub(2) as usize);
        buf.set_string(area.x + 1, y0 + 1, &shown, Style::default().fg(TEXT_DIM));
    }
    if area.height >= 3 {
        buf.set_string(
            area.x + 1,
            y0 + 2,
            "Change anytime in /settings → Privacy.",
            Style::default().fg(TEXT_DIM),
        );
    }
    if area.height >= 4 {
        buf.set_string(
            area.x + 1,
            y0 + 3,
            "Read Terms and Privacy Policy.",
            Style::default()
                .fg(TEXT_DIM)
                .add_modifier(Modifier::UNDERLINED),
        );
    }
}

fn banner_btn_style(hover: bool, focus: bool, recommended: bool) -> Style {
    if focus {
        Style::default()
            .fg(ACCENT)
            .bg(cortex_core::style::SELECTION_BG)
    } else if hover {
        Style::default()
            .fg(TEXT)
            .bg(BAR_HOVER)
            .add_modifier(Modifier::UNDERLINED)
    } else if recommended {
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_DIM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_counter_k_suffix() {
        assert_eq!(format_token_counter(0, 500_000), "0 / 500K");
        assert_eq!(format_token_counter(14_000, 500_000), "14K / 500K");
        assert_eq!(format_token_counter(142_000, 500_000), "142K / 500K");
    }

    #[test]
    fn clock_12h() {
        assert_eq!(format_clock_12h(0, 49), "12:49 AM");
        assert_eq!(format_clock_12h(10, 2), "10:02 AM");
        assert_eq!(format_clock_12h(15, 7), "3:07 PM");
    }

    #[test]
    fn secs_format() {
        assert_eq!(format_secs(0.4), "0.4s");
        assert_eq!(format_secs(4.6), "4.6s");
        assert_eq!(format_secs(12.0), "12s");
    }

    #[test]
    fn model_chip_lowercase_effort() {
        assert_eq!(
            model_chip("cortex-1-mini", Some("Medium")),
            "Cortex Mini 1 (medium)"
        );
    }

    #[test]
    fn composer_box_has_rounded_corners_and_chips() {
        let area = Rect::new(0, 0, 40, 3);
        let mut buf = Buffer::empty(area);
        fill_inky(area, &mut buf);
        paint_composer_box(
            area,
            &mut buf,
            "Agent",
            "Cortex Mini 1 (medium)",
            false,
            true,
        );
        let top: String = (0..40)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        let bot: String = (0..40)
            .map(|x| buf[(x, 2)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(top.contains('╭'), "{top}");
        assert!(top.contains("Agent"), "{top}");
        assert!(bot.contains('╰'), "{bot}");
        assert!(bot.contains("Cortex Mini 1 (medium)"), "{bot}");
        assert_eq!(buf[(1, 0)].style().fg, Some(HAIRLINE));
    }
}
