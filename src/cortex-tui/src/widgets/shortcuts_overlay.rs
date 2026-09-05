//! Ctrl+x shortcuts overlay — lock v2 §3.12.

use cortex_core::style::{HAIRLINE, TEXT, TEXT_DIM, TEXT_MUTED, VOID};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::ui::text_utils::first_fitting_line;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const LEFT: &[(&str, &str)] = &[
    ("Shift+Tab", "cycle Agent / Plan / Ask"),
    ("@", "mention files"),
    ("!", "bash mode"),
    ("&", "hand off to Cortex Cloud"),
    ("/", "slash commands"),
    ("Alt+Enter", "newline in the composer"),
    ("↑ / ↓", "edit last · browse history"),
];

const RIGHT: &[(&str, &str)] = &[
    ("Ctrl+p", "command palette"),
    ("Ctrl+r", "search past sessions"),
    ("Ctrl+c", "stop the current turn"),
    ("Esc", "interrupt · close"),
    ("F2", "settings"),
    ("PgUp / PgDn", "scroll the transcript"),
    ("Ctrl+x", "this overlay"),
];

/// Two-column shortcuts overlay.
pub struct ShortcutsOverlay {
    pub version: String,
}

impl Default for ShortcutsOverlay {
    fn default() -> Self {
        Self {
            version: VERSION.to_string(),
        }
    }
}

impl ShortcutsOverlay {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
        }
    }

    fn modal_rect(area: Rect) -> Rect {
        let w = 84u16.min(area.width.saturating_sub(4)).max(40);
        let h = 14u16.min(area.height.saturating_sub(2)).max(8);
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        Rect::new(x, y, w, h)
    }
}

impl Widget for ShortcutsOverlay {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let modal = Self::modal_rect(area);
        if modal.width < 20 || modal.height < 6 {
            return;
        }
        for y in modal.y..modal.bottom() {
            for x in modal.x..modal.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_bg(VOID);
                    cell.set_char(' ');
                    cell.set_fg(TEXT);
                }
            }
        }
        let rule = Style::default().fg(HAIRLINE);
        buf.set_string(modal.x, modal.y, "╭", rule);
        buf.set_string(modal.x + 1, modal.y, "─", rule);
        buf.set_string(modal.x + 3, modal.y, "Shortcuts", Style::default().fg(TEXT));
        let close_x = modal.right().saturating_sub(6);
        for col in (modal.x + 13)..close_x {
            buf.set_string(col, modal.y, "─", rule);
        }
        buf.set_string(close_x, modal.y, "[x]", Style::default().fg(TEXT_DIM));
        buf.set_string(modal.right() - 1, modal.y, "╮", rule);
        for y in (modal.y + 1)..(modal.bottom() - 1) {
            buf.set_string(modal.x, y, "│", rule);
            buf.set_string(modal.right() - 1, y, "│", rule);
        }
        buf.set_string(modal.x, modal.bottom() - 1, "╰", rule);
        for col in (modal.x + 1)..(modal.right() - 1) {
            buf.set_string(col, modal.bottom() - 1, "─", rule);
        }
        buf.set_string(modal.right() - 1, modal.bottom() - 1, "╯", rule);

        let inner = Rect::new(
            modal.x + 2,
            modal.y + 2,
            modal.width.saturating_sub(4),
            modal.height.saturating_sub(4),
        );
        let col2 = inner.x + inner.width / 2;
        for (i, (key, label)) in LEFT.iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.bottom().saturating_sub(3) {
                break;
            }
            buf.set_string(inner.x, y, &format!("{key:<12}"), Style::default().fg(TEXT));
            buf.set_string(
                inner.x + 13,
                y,
                &first_fitting_line(label, (col2.saturating_sub(inner.x + 14)) as usize),
                Style::default().fg(TEXT_DIM),
            );
        }
        for (i, (key, label)) in RIGHT.iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.bottom().saturating_sub(3) {
                break;
            }
            buf.set_string(col2, y, &format!("{key:<12}"), Style::default().fg(TEXT));
            buf.set_string(
                col2 + 13,
                y,
                &first_fitting_line(label, inner.right().saturating_sub(col2 + 13) as usize),
                Style::default().fg(TEXT_DIM),
            );
        }
        let hair_y = inner.bottom().saturating_sub(3);
        buf.set_string(inner.x, hair_y, "─".repeat(inner.width as usize), rule);
        let docs = format!(
            "Docs & guides: cortex.foundation/docs · Cortex CLI v{}",
            self.version
        );
        buf.set_string(
            inner.x,
            hair_y + 1,
            &first_fitting_line(&docs, inner.width as usize),
            Style::default().fg(TEXT_MUTED),
        );
        let close = "Ctrl+x/Esc close";
        let cx = inner.x + (inner.width.saturating_sub(close.len() as u16)) / 2;
        buf.set_string(cx, hair_y + 2, close, Style::default().fg(TEXT_DIM));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_paints_title_and_docs() {
        let area = Rect::new(0, 0, 120, 20);
        let mut buf = Buffer::empty(area);
        ShortcutsOverlay::new("0.1.7").render(area, &mut buf);
        let mut found_title = false;
        let mut found_docs = false;
        for y in 0..20 {
            let line: String = (0..120)
                .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
                .collect();
            if line.contains("Shortcuts") {
                found_title = true;
            }
            if line.contains("cortex.foundation/docs") {
                found_docs = true;
            }
        }
        assert!(found_title);
        assert!(found_docs);
    }
}
