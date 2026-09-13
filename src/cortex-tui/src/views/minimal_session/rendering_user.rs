//! User-turn bars for the session transcript.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::ui::colors::AdaptiveColors;

use super::text_utils::wrap_text;

/// A past user turn as full-width bar rows: `> text` on the first row, the
/// wrapped continuation indented under the copy, every row padded to `width`
/// so the gray bar spans the terminal. Timestamp is 12h on the first row when
/// the viewport is wide enough.
pub fn user_turn_lines(
    content: &str,
    width: u16,
    colors: &AdaptiveColors,
    timestamp: Option<&str>,
    compact: bool,
) -> Vec<Line<'static>> {
    let bar = Style::default().fg(colors.text).bg(colors.user_bg);
    let gutter = if compact { 0 } else { 1 };
    let indent = if compact { 0 } else { 2 };
    let width = width.max(3) as usize;
    let ts = timestamp.filter(|_| width >= 80 && !compact).unwrap_or("");
    let ts_w = if ts.is_empty() {
        0
    } else {
        ts.chars().count() + 2
    };
    let text_width = width
        .saturating_sub(gutter + 1 + indent + 2)
        .saturating_sub(ts_w)
        .max(8);
    let mut lines = Vec::new();
    let wrapped = wrap_text(content, text_width);
    let rows: Vec<String> = if wrapped.is_empty() {
        vec![String::new()]
    } else {
        wrapped
    };
    for (i, row) in rows.iter().enumerate() {
        let prefix = if i == 0 { "> " } else { "  " };
        let mut spans = Vec::new();
        if gutter > 0 {
            spans.push(Span::raw(" ".repeat(gutter)));
        }
        let mut text = format!("{}{}{row}", " ".repeat(indent), prefix);
        let used = unicode_width::UnicodeWidthStr::width(text.as_str());
        let pad_target = if i == 0 && !ts.is_empty() {
            width.saturating_sub(gutter + ts.chars().count())
        } else {
            width.saturating_sub(gutter)
        };
        text.push_str(&" ".repeat(pad_target.saturating_sub(used)));
        spans.push(Span::styled(text, bar));
        if i == 0 && !ts.is_empty() {
            spans.push(Span::styled(
                ts.to_string(),
                Style::default().fg(colors.text_dim).bg(colors.user_bg),
            ));
        }
        lines.push(Line::from(spans));
    }
    lines
}
