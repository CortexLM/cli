//! Tests for minimal session view.

mod harness_snapshots {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use serde_json::json;

    use crate::app::AppState;
    use crate::views::minimal_session::{ChatMessage, MinimalSessionView};
    use crate::views::tool_call::{ToolCallDisplay, ToolResultDisplay, ToolStatus};

    fn buffer_text(buf: &Buffer) -> String {
        let area = buf.area();
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn render(state: &AppState, w: u16, h: u16) -> String {
        let view = MinimalSessionView::new(state);
        let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
        view.render(Rect::new(0, 0, w, h), &mut buf);
        buffer_text(&buf)
    }

    fn dump_snapshot(name: &str, text: &str) {
        let Ok(dir) = std::env::var("CORTEX_DUMP_SNAPSHOTS") else {
            return;
        };
        let path = std::path::Path::new(&dir);
        let _ = std::fs::create_dir_all(path);
        let _ = std::fs::write(path.join(format!("{name}.txt")), text);
    }

    #[test]
    fn snapshot_home_empty_session() {
        let state = AppState::default();
        let text = render(&state, 80, 24);
        dump_snapshot("home", &text);
        assert!(!text.to_lowercase().contains("grok"));
        assert!(
            text.contains("Cortex CLI"),
            "home session should render splash: {text}"
        );
        assert!(
            !text.contains("Directory") && !text.contains("Computer"),
            "home session must not show header cards: {text}"
        );
        for glyph in ["▄█▀▀▀▀█▄", "██ ▌  ▐ ██", "█▄▄▄▄▄▄█"]
        {
            assert!(
                !text.contains(glyph),
                "home session must not show mascot {glyph:?}: {text}"
            );
        }
    }

    #[test]
    fn snapshot_session_with_turn() {
        let mut state = AppState::default();
        state.add_message(cortex_core::widgets::Message::user("List the files"));
        state.add_message(cortex_core::widgets::Message::assistant("Hi"));
        let text = render(&state, 80, 24);
        dump_snapshot("session", &text);
        assert!(text.contains("List the files") || text.contains("Hi"));
        assert!(!text.to_lowercase().contains("grok"));
    }

    #[test]
    fn snapshot_tool_tiles_and_diagnostics() {
        let mut state = AppState::default();
        state.add_message(cortex_core::widgets::Message::user("run tools"));
        let mut call = ToolCallDisplay::new(
            "1".into(),
            "Read".into(),
            json!({"file_path": "src/auth.rs"}),
            1,
        );
        call.set_status(ToolStatus::Completed);
        call.set_result(ToolResultDisplay {
            output: "pub fn sign_in() {}".into(),
            success: true,
            summary: "src/auth.rs".into(),
        });
        state.tool_calls.push(call);
        let mut diag = ToolCallDisplay::new(
            "12".into(),
            "diagnostics".into(),
            json!({"file": "a.rs"}),
            2,
        );
        diag.set_status(ToolStatus::Completed);
        state.tool_calls.push(diag);
        let text = render(&state, 120, 40);
        dump_snapshot("tool_tiles", &text);
        assert!(text.contains("Read"), "missing Read: {text}");
        assert!(text.contains("Diagnostics"), "missing Diagnostics: {text}");
        assert!(!text.contains("L a.rs"), "{text}");
        let narrow = render(&state, 40, 12);
        assert!(
            narrow.contains("Read") || narrow.contains("Diagnostics"),
            "{narrow}"
        );
    }

    #[test]
    fn snapshot_tools_running_first_class_rows() {
        let mut state = AppState::default();
        state.add_message(cortex_core::widgets::Message::user("Read main.rs"));
        let mut call = ToolCallDisplay::new(
            "tci_1".into(),
            "Read".into(),
            json!({"file_path": "src/main.rs"}),
            1,
        );
        call.set_status(ToolStatus::Running);
        call.append_output("reading src/main.rs".into());
        state.tool_calls.push(call);
        let text = render(&state, 80, 24);
        dump_snapshot("tools_running", &text);
        assert!(
            text.contains("Read") && text.contains("main.rs"),
            "tool row missing: {text}"
        );
        assert!(!text.contains("dump"));
    }

    #[test]
    fn snapshot_plan_mermaid() {
        let mut state = AppState::default();
        state.add_message(cortex_core::widgets::Message::user("Plan device login"));
        let mut call = ToolCallDisplay::new(
            "plan_1".into(),
            "Plan".into(),
            json!({"title": "Add device login"}),
            2,
        );
        call.set_status(ToolStatus::Completed);
        call.set_result(ToolResultDisplay {
            output: "## Plan (mermaid)\n\n```mermaid\nflowchart TD\n  a-->b\n```".into(),
            success: true,
            summary: "↳ Plan (mermaid)".into(),
        });
        state.tool_calls.push(call);
        let text = render(&state, 80, 24);
        dump_snapshot("plan", &text);
        assert!(
            text.contains("Plan") && (text.contains("mermaid") || text.contains("device")),
            "plan mermaid missing: {text}"
        );
    }

    #[test]
    fn snapshot_error_state() {
        let mut state = AppState::default();
        state.add_message(cortex_core::widgets::Message::assistant(
            "The coding service is temporarily unavailable",
        ));
        let text = render(&state, 80, 24);
        dump_snapshot("error", &text);
        assert!(text.contains("temporarily unavailable") || text.contains("coding service"));
        assert!(!text.to_lowercase().contains("reqwest"));
        assert!(!text.to_lowercase().contains("grok"));
    }

    #[test]
    fn snapshot_narrow_and_wide() {
        let state = AppState::default();
        let narrow = render(&state, 40, 12);
        let wide = render(&state, 120, 40);
        assert!(!narrow.trim().is_empty());
        assert!(!wide.trim().is_empty());
    }

    #[test]
    fn chat_message_helpers_still_work() {
        let _ = ChatMessage::user("x");
    }

    #[test]
    fn snapshot_cancel_message() {
        let mut state = AppState::default();
        state.add_message(cortex_core::widgets::Message::user("list files"));
        state.add_message(cortex_core::widgets::Message::system("Cancelled."));
        let text = render(&state, 80, 24);
        dump_snapshot("cancel", &text);
        assert!(text.contains("Cancelled."), "cancel missing: {text}");
        assert!(!text.to_lowercase().contains("grok"));
    }

    #[test]
    fn snapshot_ask_questions() {
        use crate::question::{Question, QuestionRequest, QuestionState, QuestionType};
        use crate::views::question_prompt::QuestionPromptView;
        use ratatui::widgets::Widget;

        let request = QuestionRequest {
            id: "q1".into(),
            title: "Questions".into(),
            description: Some("Need a decision before continuing.".into()),
            questions: vec![Question {
                id: "q1".into(),
                question: "Ship the change?".into(),
                question_type: QuestionType::Text,
                options: vec![],
                placeholder: None,
                required: true,
                allow_custom: true,
            }],
        };
        let q_state = QuestionState::new(request);
        let view = QuestionPromptView::new(&q_state);
        let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 80, 24));
        view.render(ratatui::layout::Rect::new(0, 0, 80, 24), &mut buf);
        let text = buffer_text(&buf);
        dump_snapshot("ask", &text);
        assert!(
            text.contains("Ship the change?") || text.contains("Questions"),
            "ask/questions missing: {text}"
        );
        assert!(!text.to_lowercase().contains("grok"));
    }
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;

    use cortex_core::widgets::MessageRole;

    use crate::app::AppState;
    use crate::views::minimal_session::{ChatMessage, MinimalSessionView};

    fn create_test_buffer(width: u16, height: u16) -> Buffer {
        Buffer::empty(Rect::new(0, 0, width, height))
    }

    fn create_test_app_state() -> AppState {
        AppState::default()
    }

    #[test]
    fn test_chat_message_user() {
        let msg = ChatMessage::user("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert!(!msg.is_streaming);
    }

    #[test]
    fn test_chat_message_assistant() {
        let msg = ChatMessage::assistant("Hi there");
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.content, "Hi there");
    }

    #[test]
    fn test_chat_message_streaming() {
        let msg = ChatMessage::assistant("Working...").streaming();
        assert!(msg.is_streaming);
    }

    #[test]
    fn test_chat_message_tool() {
        let msg = ChatMessage::tool("read_file", "Contents here");
        assert_eq!(msg.role, MessageRole::Tool);
        assert_eq!(msg.tool_name, Some("read_file".to_string()));
    }

    #[test]
    fn test_minimal_session_view_new() {
        let state = create_test_app_state();
        let _view = MinimalSessionView::new(&state);
        // View created successfully
    }

    #[test]
    fn test_minimal_session_view_render() {
        let state = create_test_app_state();
        let view = MinimalSessionView::new(&state);

        let mut buf = create_test_buffer(80, 24);
        let area = Rect::new(0, 0, 80, 24);
        view.render(area, &mut buf);

        // Check that something was rendered somewhere in the buffer
        // This is a basic sanity check that the view renders without panic
        // and produces some output
        let mut has_content = false;
        for y in 0..24 {
            for x in 0..80 {
                let symbol = buf[(x, y)].symbol();
                if !symbol.trim().is_empty() && symbol != " " {
                    has_content = true;
                    break;
                }
            }
            if has_content {
                break;
            }
        }
        // The view should render something
        assert!(has_content, "View should render some content");
    }

    #[test]
    #[ignore = "TUI behavior differs across platforms"]
    fn test_cursor_position() {
        let state = create_test_app_state();
        let view = MinimalSessionView::new(&state);

        let input_area = Rect::new(0, 20, 80, 1);
        let cursor = view.cursor_position(input_area);

        assert!(cursor.is_some());
        let (x, y) = cursor.unwrap();
        // x should be 0 + 2 ("> ") + cursor_pos
        assert_eq!(x, 2); // Empty input, cursor at position 0
        assert_eq!(y, 20);
    }
}
