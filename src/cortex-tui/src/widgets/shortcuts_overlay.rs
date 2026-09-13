//! `/shortcuts` interactive sheet — lock v2 Ctrl+x overlay.
//!
//! Open, navigate (↑↓), dismiss (Esc / Ctrl+x / `[x]`). Selected key uses
//! banner green `#1F4945`. Chrome hairlines stay inky gray.

use cortex_core::style::{ACCENT, HAIRLINE, TEXT, TEXT_DIM, TEXT_MUTED, VOID};
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

const NARROW: &[(&str, &str)] = &[
    ("Shift+Tab", "cycle Agent / Plan / Ask"),
    ("@", "mention files"),
    ("!", "bash mode"),
    ("&", "hand off to Cortex Cloud"),
    ("Ctrl+c", "stop"),
    ("F2", "settings"),
    ("Esc", "interrupt / close"),
];

/// Two-column shortcuts overlay.
pub struct ShortcutsOverlay {
    pub version: String,
    pub selected: usize,
    pub hovered: Option<usize>,
}

impl Default for ShortcutsOverlay {
    fn default() -> Self {
        Self {
            version: VERSION.to_string(),
            selected: 0,
            hovered: None,
        }
    }
}

impl ShortcutsOverlay {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            selected: 0,
            hovered: None,
        }
    }

    pub fn with_selection(mut self, selected: usize, hovered: Option<usize>) -> Self {
        self.selected = selected;
        self.hovered = hovered;
        self
    }

    pub fn catalog_len(area: Rect) -> usize {
        if compact(area) {
            NARROW.len()
        } else {
            LEFT.len() + RIGHT.len()
        }
    }

    /// Rows that actually paint inside `area` (compact terminals clip the list).
    pub fn row_count(area: Rect) -> usize {
        let modal = Self::modal_rect(area);
        let inner_h = modal.height.saturating_sub(4);
        if compact(area) {
            let visible = inner_h.saturating_sub(2) as usize;
            NARROW.len().min(visible.max(1))
        } else {
            let per = inner_h.saturating_sub(3) as usize;
            LEFT.len().min(per) + RIGHT.len().min(per)
        }
        .max(1)
        .min(Self::catalog_len(area))
    }

    pub fn wrap_index(area: Rect, index: usize) -> usize {
        let n = Self::row_count(area).max(1);
        index % n
    }

    pub fn modal_rect(area: Rect) -> Rect {
        let w = 84u16
            .min(area.width.saturating_sub(4))
            .max(40.min(area.width));
        let h = 14u16
            .min(area.height.saturating_sub(2))
            .max(8.min(area.height));
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        Rect::new(x, y, w, h)
    }

    /// True when `(x, y)` is on the `[x]` close control.
    pub fn close_hit(area: Rect, x: u16, y: u16) -> bool {
        let modal = Self::modal_rect(area);
        if modal.width < 6 || y != modal.y {
            return false;
        }
        let close_x = modal.right().saturating_sub(6);
        x >= close_x && x < close_x.saturating_add(3)
    }

    /// Binding under `(x, y)`, if any. Pointers outside the sheet are ignored.
    pub fn row_at(area: Rect, x: u16, y: u16) -> Option<usize> {
        let modal = Self::modal_rect(area);
        if x < modal.x || x >= modal.right() || y < modal.y || y >= modal.bottom() {
            return None;
        }
        let inner = Rect::new(
            modal.x + 2,
            modal.y + 2,
            modal.width.saturating_sub(4),
            modal.height.saturating_sub(4),
        );
        if y < inner.y {
            return None;
        }
        let idx = (y - inner.y) as usize;
        if compact(area) {
            let stop = inner.bottom().saturating_sub(2);
            if y >= stop {
                return None;
            }
            return (idx < NARROW.len()).then_some(idx);
        }
        let stop = inner.bottom().saturating_sub(3);
        if y >= stop {
            return None;
        }
        let col2 = inner.x + inner.width / 2;
        if x >= col2 {
            let i = LEFT.len() + idx;
            (i < LEFT.len() + RIGHT.len()).then_some(i)
        } else if idx < LEFT.len() {
            Some(idx)
        } else {
            None
        }
    }
}

fn compact(area: Rect) -> bool {
    area.width < 80 || area.height < 16
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
        if compact(area) {
            paint_column(
                buf,
                inner.x,
                inner.y,
                inner.width,
                inner.bottom().saturating_sub(2),
                NARROW,
                self.selected,
                self.hovered,
                0,
            );
        } else {
            let col2 = inner.x + inner.width / 2;
            let stop = inner.bottom().saturating_sub(3);
            paint_column(
                buf,
                inner.x,
                inner.y,
                col2.saturating_sub(inner.x),
                stop,
                LEFT,
                self.selected,
                self.hovered,
                0,
            );
            paint_column(
                buf,
                col2,
                inner.y,
                inner.right().saturating_sub(col2),
                stop,
                RIGHT,
                self.selected,
                self.hovered,
                LEFT.len(),
            );
            let hair_y = inner.bottom().saturating_sub(3);
            buf.set_string(inner.x, hair_y, "─".repeat(inner.width as usize), rule);
            let docs = format!(
                "Docs & guides: cortex.foundation/docs · Cortex CLI v{}",
                self.version
            );
            buf.set_string(
                inner.x,
                hair_y + 1,
                first_fitting_line(&docs, inner.width as usize),
                Style::default().fg(TEXT_MUTED),
            );
            let close = "Ctrl+x/Esc close";
            let cx = inner.x + (inner.width.saturating_sub(close.len() as u16)) / 2;
            buf.set_string(cx, hair_y + 2, close, Style::default().fg(TEXT_DIM));
            return;
        }
        let close = "Ctrl+x/Esc close";
        let cy = modal.bottom().saturating_sub(2);
        let cx = inner.x + (inner.width.saturating_sub(close.len() as u16)) / 2;
        buf.set_string(cx, cy, close, Style::default().fg(TEXT_DIM));
    }
}

fn paint_column(
    buf: &mut Buffer,
    x: u16,
    y0: u16,
    width: u16,
    stop_y: u16,
    rows: &[(&str, &str)],
    selected: usize,
    hovered: Option<usize>,
    base: usize,
) {
    for (i, (key, label)) in rows.iter().enumerate() {
        let y = y0 + i as u16;
        if y >= stop_y {
            break;
        }
        let idx = base + i;
        let active = idx == selected || hovered == Some(idx);
        let key_style = if active {
            Style::default().fg(ACCENT)
        } else {
            Style::default().fg(TEXT)
        };
        let key_w = 12u16.min(width);
        buf.set_string(x, y, format!("{key:<12}"), key_style);
        if width > key_w + 1 {
            buf.set_string(
                x + key_w,
                y,
                first_fitting_line(label, width.saturating_sub(key_w) as usize),
                Style::default().fg(TEXT_DIM),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_core::style::ACCENT;

    fn line(buf: &Buffer, y: u16, width: u16) -> String {
        (0..width)
            .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
            .collect()
    }

    #[test]
    fn overlay_paints_title_and_docs() {
        let area = Rect::new(0, 0, 120, 20);
        let mut buf = Buffer::empty(area);
        ShortcutsOverlay::new("0.1.7").render(area, &mut buf);
        let mut found_title = false;
        let mut found_docs = false;
        for y in 0..20 {
            let row = line(&buf, y, 120);
            if row.contains("Shortcuts") {
                found_title = true;
            }
            if row.contains("cortex.foundation/docs") {
                found_docs = true;
            }
        }
        assert!(found_title);
        assert!(found_docs);
    }

    #[test]
    fn selected_key_uses_banner_green() {
        let area = Rect::new(0, 0, 120, 24);
        let mut buf = Buffer::empty(area);
        ShortcutsOverlay::new("0.1.7")
            .with_selection(0, None)
            .render(area, &mut buf);
        let mut accent = false;
        for y in 0..24 {
            for x in 0..120 {
                if buf[(x, y)].fg == ACCENT && buf[(x, y)].symbol().contains('S') {
                    accent = true;
                }
            }
        }
        assert!(accent, "selected Shift+Tab must use #1F4945");
    }

    #[test]
    fn close_hit_and_row_at() {
        let area = Rect::new(0, 0, 120, 40);
        let modal = ShortcutsOverlay::modal_rect(area);
        let close_x = modal.right().saturating_sub(6);
        assert!(ShortcutsOverlay::close_hit(area, close_x, modal.y));
        assert!(!ShortcutsOverlay::close_hit(area, modal.x, modal.y));
        assert_eq!(
            ShortcutsOverlay::row_at(area, modal.x + 3, modal.y + 2),
            Some(0)
        );
        assert_eq!(
            ShortcutsOverlay::row_at(area, modal.x.saturating_sub(1), modal.y + 2),
            None
        );
        assert_eq!(
            ShortcutsOverlay::row_at(area, modal.right(), modal.y + 2),
            None
        );
        assert_eq!(
            ShortcutsOverlay::catalog_len(area),
            LEFT.len() + RIGHT.len()
        );
    }

    #[test]
    fn narrow_sheet_fits_40x12() {
        let area = Rect::new(0, 0, 40, 12);
        let mut buf = Buffer::empty(area);
        ShortcutsOverlay::new("0.1.7").render(area, &mut buf);
        let mut found = false;
        for y in 0..12 {
            if line(&buf, y, 40).contains("Shortcuts") {
                found = true;
            }
        }
        assert!(found);
        let visible = ShortcutsOverlay::row_count(area);
        assert!(
            visible <= 4,
            "40×12 paints at most 4 bindings, got {visible}"
        );
        assert_eq!(ShortcutsOverlay::catalog_len(area), NARROW.len());
        assert_eq!(ShortcutsOverlay::wrap_index(area, visible), 0);
        let modal = ShortcutsOverlay::modal_rect(area);
        let inner_y = modal.y + 2;
        assert_eq!(
            ShortcutsOverlay::row_at(area, modal.x + 2, inner_y + visible as u16),
            None
        );
    }
}
