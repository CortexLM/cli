//! `/shortcuts` sheet open / navigate / dismiss.

use ratatui::layout::Rect;

use crate::widgets::ShortcutsOverlay;

use super::state::AppState;

impl AppState {
    pub fn open_shortcuts_sheet(&mut self) {
        self.settings_modal = None;
        self.shortcuts_open = true;
        self.shortcuts_selected = 0;
        self.shortcuts_hovered = None;
    }

    pub fn close_shortcuts_sheet(&mut self) {
        self.shortcuts_open = false;
        self.shortcuts_hovered = None;
    }

    pub fn toggle_shortcuts_sheet(&mut self) {
        if self.shortcuts_open {
            self.close_shortcuts_sheet();
        } else {
            self.open_shortcuts_sheet();
        }
    }

    pub fn shortcuts_move(&mut self, delta: i32) {
        let (w, h) = self.terminal_size;
        let area = Rect::new(0, 0, w.max(40), h.max(12));
        let n = ShortcutsOverlay::row_count(area).max(1) as i32;
        let next = (self.shortcuts_selected as i32 + delta).rem_euclid(n);
        self.shortcuts_selected = next as usize;
    }

    pub fn shortcuts_hover_at(&mut self, x: u16, y: u16) {
        let (w, h) = self.terminal_size;
        let area = Rect::new(0, 0, w.max(1), h.max(1));
        self.shortcuts_hovered = ShortcutsOverlay::row_at(area, x, y);
    }

    pub fn shortcuts_close_hit(&self, x: u16, y: u16) -> bool {
        let (w, h) = self.terminal_size;
        let area = Rect::new(0, 0, w.max(1), h.max(1));
        ShortcutsOverlay::close_hit(area, x, y)
    }

    pub fn shortcuts_select_at(&mut self, x: u16, y: u16) -> bool {
        let (w, h) = self.terminal_size;
        let area = Rect::new(0, 0, w.max(1), h.max(1));
        if let Some(idx) = ShortcutsOverlay::row_at(area, x, y) {
            self.shortcuts_selected = idx;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_navigate_dismiss() {
        let mut state = AppState::default();
        state.terminal_size = (120, 40);
        state.toggle_shortcuts_sheet();
        assert!(state.shortcuts_open);
        assert_eq!(state.shortcuts_selected, 0);
        state.shortcuts_move(1);
        assert_eq!(state.shortcuts_selected, 1);
        state.shortcuts_move(-1);
        assert_eq!(state.shortcuts_selected, 0);
        state.toggle_shortcuts_sheet();
        assert!(!state.shortcuts_open);
    }

    #[test]
    fn compact_navigation_stays_on_painted_rows() {
        let mut state = AppState::default();
        state.terminal_size = (40, 12);
        state.toggle_shortcuts_sheet();
        let area = Rect::new(0, 0, 40, 12);
        let visible = ShortcutsOverlay::row_count(area);
        for _ in 0..(visible + 3) {
            state.shortcuts_move(1);
        }
        assert!(state.shortcuts_selected < visible);
        assert!(
            ShortcutsOverlay::row_at(area, 0, area.y + 2).is_none(),
            "margin left of the sheet is not a hit"
        );
    }
}
