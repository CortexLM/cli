//! Session chrome banners (update + back-to-main).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::ui::colors::AdaptiveColors;

/// Renders the "← Back to main conversation" hint when viewing a subagent.
pub fn render_back_to_main_hint(area: Rect, buf: &mut Buffer, colors: &AdaptiveColors) {
    let hint = "← Back to main (Esc)";
    let style = Style::default().fg(colors.text_dim);
    buf.set_string(area.x + 1, area.y, hint, style);
}

/// Renders an update notification banner above the input box.
pub fn render_update_banner(
    area: Rect,
    buf: &mut Buffer,
    colors: &AdaptiveColors,
    update_status: &crate::app::UpdateStatus,
) {
    use crate::app::UpdateStatus;

    if area.is_empty() || area.height < 1 {
        return;
    }

    let (icon, icon_style, text) = match update_status {
        UpdateStatus::Available { version } => (
            "↑",
            Style::default().fg(colors.text),
            format!(" A new version ({}) is available ", version),
        ),
        UpdateStatus::Downloading {
            version: _,
            progress,
        } => (
            "⟳",
            Style::default().fg(colors.text_dim),
            format!(" Downloading update... {}% ", progress),
        ),
        UpdateStatus::ReadyToRestart { version: _ } => (
            "✓",
            Style::default().fg(colors.success),
            " You must restart to run the latest version ".to_string(),
        ),
        _ => return,
    };
    let text_style = Style::default().fg(colors.text);
    let banner_width = (icon.chars().count() + text.len() + 2) as u16;
    let x = area.x + 2;
    let y = area.y;
    if x + banner_width > area.right() {
        return;
    }
    buf.set_string(x, y, icon, icon_style);
    buf.set_string(x + icon.chars().count() as u16 + 1, y, &text, text_style);
}
