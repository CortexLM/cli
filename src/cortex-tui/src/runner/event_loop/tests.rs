//! Tests for the event loop.

#[cfg(test)]
mod tests {
    use crate::actions::ActionContext;
    use crate::app::{AppState, ApprovalMode, ApprovalState, FocusTarget};
    use crate::runner::event_loop::EventLoop;

    #[test]
    fn test_event_loop_new() {
        let app_state = AppState::new();
        let event_loop = EventLoop::new(app_state);

        assert!(!event_loop.is_running());
        assert!(event_loop.session_bridge().is_none());
    }

    #[test]
    fn test_event_loop_stop() {
        let app_state = AppState::new();
        let event_loop = EventLoop::new(app_state);

        event_loop.stop();
        assert!(!event_loop.is_running());
    }

    #[test]
    fn test_get_action_context_input() {
        let mut app_state = AppState::new();
        app_state.focus = FocusTarget::Input;
        let event_loop = EventLoop::new(app_state);

        assert_eq!(event_loop.get_action_context(), ActionContext::Input);
    }

    #[test]
    fn test_get_action_context_approval() {
        let mut app_state = AppState::new();
        app_state.pending_approval = Some(ApprovalState {
            tool_call_id: "test-id".to_string(),
            tool_name: "test".to_string(),
            tool_args: "{}".to_string(),
            tool_args_json: Some(serde_json::json!({})),
            diff_preview: None,
            approval_mode: ApprovalMode::Ask,
        });
        let event_loop = EventLoop::new(app_state);

        assert_eq!(event_loop.get_action_context(), ActionContext::Approval);
    }

    #[test]
    fn test_app_state_should_quit() {
        let mut app_state = AppState::new();
        assert!(!app_state.should_quit());

        app_state.set_quit();
        assert!(app_state.should_quit());
    }

    #[test]
    fn test_event_loop_accessors() {
        let app_state = AppState::new();
        let mut event_loop = EventLoop::new(app_state);

        let _ = event_loop.app_state();
        let _ = event_loop.stream_controller();
        let _ = event_loop.session_bridge();

        let _ = event_loop.app_state_mut();
        let _ = event_loop.stream_controller_mut();
    }

    #[test]
    fn test_event_loop_mouse_handler_accessors() {
        let app_state = AppState::new();
        let mut event_loop = EventLoop::new(app_state);

        let _ = event_loop.mouse_handler();
        let _ = event_loop.mouse_handler_mut();

        let _ = event_loop.click_zones();
        let _ = event_loop.click_zones_mut();
    }

    #[test]
    fn mcp_disconnect_without_user_stop_is_a_drop() {
        use crate::modal::mcp_manager::{McpServerInfo, McpStatus};
        use cortex_engine::mcp::McpLifecycleEvent;

        let mut event_loop = EventLoop::new(AppState::new());
        event_loop.app_state.mcp_servers.push(McpServerInfo {
            name: "github".into(),
            status: McpStatus::Running,
            tool_count: 12,
            error: None,
            requires_auth: false,
        });
        event_loop.handle_mcp_event(McpLifecycleEvent::ServerDisconnected {
            name: "github".into(),
        });
        assert_eq!(event_loop.app_state.mcp_servers[0].status, McpStatus::Error);
        assert!(
            event_loop
                .app_state
                .messages
                .iter()
                .any(|m| m.content.contains("github dropped"))
        );
    }

    #[test]
    fn interrupt_records_stopped_once() {
        let mut event_loop = EventLoop::new(AppState::new());
        event_loop.mark_turn_stopped();
        event_loop.mark_turn_stopped();
        let stopped = event_loop
            .app_state
            .messages
            .iter()
            .filter(|m| m.content.contains("Stopped"))
            .count();
        assert_eq!(stopped, 1);
    }
}
