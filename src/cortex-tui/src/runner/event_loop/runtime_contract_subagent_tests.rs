use super::*;
use serde_json::json;

#[test]
fn runtime_contract_child_context_and_controls_are_explicit() {
    let (prompt, _, role, seconds, model) = parse_child_request(&json!({
        "prompt":"Read the assigned file", "context":"Only assigned context",
        "subagent_type":"review", "timeout_secs":12, "model":"cortex-1-mini"
    }))
    .unwrap();
    assert_eq!(
        prompt,
        "Read the assigned file\n\nAssigned context:\nOnly assigned context"
    );
    assert_eq!(role, "review");
    assert_eq!(seconds, 12);
    assert_eq!(model.as_deref(), Some("cortex-1-mini"));
    for args in [
        json!({"prompt":""}),
        json!({"prompt":"work", "resume":"old"}),
        json!({"prompt":"work", "max_iterations":5}),
        json!({"prompt":"work", "timeout_secs":0}),
        json!({"prompt":"work", "context":{}}),
    ] {
        assert!(parse_child_request(&args).is_err());
    }
}

#[test]
fn runtime_contract_child_concurrency_is_bounded() {
    let permits: Vec<_> = (0..4).map(|_| CHILD_LIMIT.try_acquire().unwrap()).collect();
    assert!(CHILD_LIMIT.try_acquire().is_err());
    drop(permits);
    assert_eq!(CHILD_LIMIT.available_permits(), 4);
}

struct ChildFixture {
    events: Vec<ResponseEvent>,
    capabilities: cortex_engine::client::ModelCapabilities,
}

#[async_trait::async_trait]
impl ModelClient for ChildFixture {
    fn model(&self) -> &str {
        "cortex-1-mini"
    }
    fn provider(&self) -> &str {
        "runtime-fixture"
    }
    fn capabilities(&self) -> &cortex_engine::client::ModelCapabilities {
        &self.capabilities
    }
    fn owns_tool_execution(&self) -> bool {
        true
    }
    async fn complete(
        &self,
        _: CompletionRequest,
    ) -> cortex_engine::Result<cortex_engine::client::ResponseStream> {
        Ok(Box::pin(tokio_stream::iter(
            self.events.clone().into_iter().map(Ok),
        )))
    }
    async fn complete_sync(
        &self,
        _: CompletionRequest,
    ) -> cortex_engine::Result<cortex_engine::client::CompletionResponse> {
        Err(cortex_engine::CortexError::InvalidInput(
            "Streaming fixture only.".into(),
        ))
    }
}

#[tokio::test]
async fn runtime_contract_child_requires_terminal_and_never_invokes_tools() {
    use cortex_engine::client::{CompletionResponse, FinishReason, ToolCallEvent};
    for events in [
        vec![],
        vec![ResponseEvent::Delta("partial".into())],
        vec![ResponseEvent::Done(CompletionResponse {
            finish_reason: FinishReason::Length,
            ..Default::default()
        })],
        vec![ResponseEvent::ToolCall(ToolCallEvent {
            id: "call".into(),
            name: "Read".into(),
            arguments: "{}".into(),
            remote: false,
        })],
        vec![
            ResponseEvent::ToolCall(ToolCallEvent {
                id: "call".into(),
                name: "Read".into(),
                arguments: "{}".into(),
                remote: true,
            }),
            ResponseEvent::Done(CompletionResponse::default()),
        ],
    ] {
        let fixture = ChildFixture {
            events,
            capabilities: Default::default(),
        };
        assert!(
            collect_child_turn(&fixture, CompletionRequest::default())
                .await
                .is_err()
        );
    }
    let fixture = ChildFixture {
        events: vec![
            ResponseEvent::Delta("Exact child output".into()),
            ResponseEvent::Done(CompletionResponse::default()),
        ],
        capabilities: Default::default(),
    };
    assert_eq!(
        collect_child_turn(&fixture, CompletionRequest::default())
            .await
            .unwrap(),
        "Exact child output"
    );
}

#[tokio::test]
async fn runtime_contract_child_failure_is_visible_in_headless_view() {
    use crate::{app::AppState, app::AppView, views::MinimalSessionView};
    use cortex_tui_capture::{CaptureConfig, MockTerminal};
    for (width, height) in [(40, 12), (120, 40)] {
        let mut state = AppState::default();
        state.terminal_size = (width, height);
        state.show_launch_splash = false;
        state.opt_in_banner = false;
        state.set_view(AppView::Session);
        state.add_message(cortex_core::widgets::Message::user("Delegate the review"));
        state.add_subagent_task(SubagentTaskDisplay::new(
            "subagent_child",
            "child",
            "Review fixture",
            "review",
        ));
        let mut event_loop = EventLoop::new(state);
        event_loop
            .handle_tool_event(ToolEvent::Failed {
                id: "child".into(),
                name: "Task".into(),
                error: "Child deadline reached".into(),
                duration: Duration::from_secs(1),
            })
            .await;
        assert!(!event_loop.stream_done_received);
        let config = CaptureConfig::minimal(width, height);
        let mut terminal = MockTerminal::from_config(config.clone()).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    MinimalSessionView::new(event_loop.app_state()),
                    frame.area(),
                )
            })
            .unwrap();
        let snapshot = terminal.snapshot().to_ascii(&config);
        let compact: String = snapshot.split_whitespace().collect();
        assert!(compact.contains("Childdeadlinereached"), "{snapshot}");
    }
}
