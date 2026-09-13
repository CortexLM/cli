//! Inline picker geometry shared by paint and mouse hit-testing.

use ratatui::layout::Rect;

use super::state::InteractiveState;

/// Must match `views::minimal_session::COMPOSER_ROWS` (avoid a crate cycle).
const COMPOSER_ROWS: u16 = 3;

/// Session inline lists cap at 8 option rows (lock chrome).
pub const INLINE_OPTION_CAP: usize = 8;

/// Footer + blank line under the composer in [`MinimalSessionView`].
const FOOTER_ROWS: u16 = 1;
const BLANK_BEFORE_FOOTER: u16 = 1;

/// Rows the rows-only picker occupies above the composer.
pub fn picker_stack_height(state: &InteractiveState) -> u16 {
    if state.effort_focused {
        return 3;
    }
    let n = if state.filtered_indices.is_empty() {
        1
    } else {
        state
            .filtered_indices
            .len()
            .min(state.max_visible)
            .min(INLINE_OPTION_CAP)
    };
    (n as u16).saturating_add(state.inline_chrome_rows())
}

/// Screen rect of the rows-only picker (matches [`crate::views::minimal_session::MinimalSessionView`]).
pub fn session_inline_picker_area(screen: Rect, picker_height: u16) -> Rect {
    let footer_y = screen.bottom().saturating_sub(FOOTER_ROWS);
    let composer_y = footer_y
        .saturating_sub(BLANK_BEFORE_FOOTER)
        .saturating_sub(COMPOSER_ROWS);
    let y = composer_y.saturating_sub(picker_height);
    Rect::new(screen.x, y, screen.width, picker_height)
}

/// Click targets for rows-only chrome: banner / subtitle / search skip, then options.
pub fn calculate_inline_click_zones(state: &mut InteractiveState, area: Rect) {
    state.click_zones.clear();
    state.tab_click_zones.clear();
    if state.is_form_active() || state.effort_focused {
        return;
    }
    let chrome = state.inline_chrome_rows();
    let items_y = area.y.saturating_add(chrome);
    let items_h = area.height.saturating_sub(chrome);
    if items_h == 0 {
        return;
    }
    let start = state.scroll_offset;
    let visible = state.filtered_indices.len();
    let end = (start + items_h as usize).min(visible);
    for i in 0..(end.saturating_sub(start)) {
        let y = items_y.saturating_add(i as u16);
        if y >= items_y.saturating_add(items_h) {
            break;
        }
        state
            .click_zones
            .push((Rect::new(area.x, y, area.width, 1), start + i));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interactive::builders::{
        build_clear_confirm, build_mcp_selector, build_permissions_picker, build_resume_picker,
    };
    use crate::modal::mcp_manager::{McpServerInfo, McpStatus};
    use crate::session::SessionSummary;
    use chrono::Utc;

    fn hit(state: &InteractiveState, y: u16) -> Option<usize> {
        state.hit_test(2, y)
    }

    #[test]
    fn clear_confirm_clicks_map_to_visible_options() {
        let mut state = build_clear_confirm();
        assert_eq!(state.inline_chrome_rows(), 2);
        let area = Rect::new(0, 10, 80, 4);
        calculate_inline_click_zones(&mut state, area);
        assert_eq!(hit(&state, 10), None, "banner");
        assert_eq!(hit(&state, 11), None, "subtitle");
        assert_eq!(hit(&state, 12), Some(0), "first option");
        assert_eq!(hit(&state, 13), Some(1), "second option");
    }

    #[test]
    fn mcp_clicks_skip_banner() {
        let servers = vec![
            McpServerInfo {
                name: "git".into(),
                status: McpStatus::Running,
                tool_count: 3,
                error: None,
                requires_auth: false,
            },
            McpServerInfo {
                name: "docs".into(),
                status: McpStatus::Error,
                tool_count: 0,
                error: Some("lost".into()),
                requires_auth: false,
            },
        ];
        let mut state = build_mcp_selector(&servers);
        let area = Rect::new(0, 5, 80, 3);
        calculate_inline_click_zones(&mut state, area);
        assert_eq!(hit(&state, 5), None);
        assert_eq!(hit(&state, 6), Some(0));
        assert_eq!(hit(&state, 7), Some(1));
    }

    #[test]
    fn permissions_clicks_skip_banner() {
        let mut state = build_permissions_picker(Some("smart"));
        let area = Rect::new(0, 0, 80, 4);
        calculate_inline_click_zones(&mut state, area);
        assert_eq!(hit(&state, 0), None);
        assert_eq!(hit(&state, 1), Some(0));
        assert_eq!(hit(&state, 3), Some(2));
    }

    #[test]
    fn resume_clicks_skip_search_chrome() {
        let sessions = vec![SessionSummary {
            id: "a".into(),
            title: "one".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            message_count: 2,
            model: "cortex-1-mini".into(),
            provider: "cortex".into(),
            archived: false,
        }];
        let mut state = build_resume_picker(&sessions, false);
        assert_eq!(state.inline_chrome_rows(), 2);
        let area = Rect::new(0, 20, 80, 3);
        calculate_inline_click_zones(&mut state, area);
        assert_eq!(hit(&state, 20), None);
        assert_eq!(hit(&state, 21), None);
        assert_eq!(hit(&state, 22), Some(0));
    }

    #[test]
    fn session_picker_sits_above_composer() {
        let screen = Rect::new(0, 0, 80, 24);
        let area = session_inline_picker_area(screen, 4);
        assert_eq!(area.height, 4);
        assert_eq!(area.y, 24 - 1 - 1 - COMPOSER_ROWS - 4);
    }
}
