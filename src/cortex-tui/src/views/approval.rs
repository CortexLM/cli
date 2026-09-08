//! Tool approval — dead-pathed centred modal.
//!
//! Production approval is the SPEC §3.10 inline numbered radios opened by
//! [`crate::app::AppState::request_tool_approval`]. This view stays exported
//! so `AppView::Approval` still compiles; it paints nothing.

use crate::app::AppState;
use ratatui::prelude::*;
use ratatui::widgets::Widget;

/// Legacy centred-modal approval view. Intentionally a no-op.
pub struct ApprovalView<'a> {
    #[allow(dead_code)]
    state: &'a AppState,
}

impl<'a> ApprovalView<'a> {
    /// Creates a new ApprovalView. Rendering is a no-op.
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }
}

impl Widget for ApprovalView<'_> {
    fn render(self, _area: Rect, _buf: &mut Buffer) {
        // Inline radios on MinimalSessionView own this surface.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ApprovalState;

    fn create_test_state() -> AppState {
        let mut state = AppState::new();
        state.request_tool_approval(
            "call-1".into(),
            "write_file".into(),
            serde_json::json!({
                "path": "src/main.rs",
                "content": "fn main() {}"
            }),
            None,
        );
        state
    }

    #[test]
    fn test_approval_view_creation() {
        let state = create_test_state();
        let _view = ApprovalView::new(&state);
    }

    #[test]
    fn dead_path_does_not_paint_centred_modal() {
        let state = create_test_state();
        let view = ApprovalView::new(&state);
        let area = Rect::new(0, 0, 120, 40);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);
        let text: String = (0..40)
            .map(|y| {
                (0..120)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect();
        assert!(
            !text.contains("Tool Approval Required"),
            "centred modal must stay dead:\n{text}"
        );
        assert!(
            !text.contains("[y]"),
            "legacy y/n/s/a/d chrome must stay dead:\n{text}"
        );
    }

    #[test]
    fn request_opens_inline_radios_not_approval_view() {
        let state = create_test_state();
        assert!(state.has_pending_approval());
        assert_ne!(state.view, crate::app::AppView::Approval);
        let interactive = state.get_interactive_state().expect("inline radios");
        assert_eq!(interactive.items.len(), 4);
        assert!(interactive.items[0].label.starts_with("1 "));
        assert!(interactive.prompt_owns_focus);
        let _ = ApprovalState::new("shell".into(), serde_json::json!({"command": "true"}));
    }
}
