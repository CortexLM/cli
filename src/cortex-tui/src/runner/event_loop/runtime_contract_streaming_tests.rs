use super::*;
use crate::app::AppState;
use crate::views::MinimalSessionView;
use cortex_engine::client::runtime_contract::INCOMPLETE_STREAM;
use cortex_tui_capture::{CaptureConfig, MockTerminal};

#[tokio::test]
async fn runtime_contract_stream_error_clears_work_and_holds_followups() {
    let mut event_loop = EventLoop::new(AppState::default());
    event_loop
        .app_state
        .queue_message("Do not submit after failure".into());
    event_loop.app_state.add_pending_tool_result(
        "tool".into(),
        "Read".into(),
        "partial".into(),
        true,
    );
    let task = tokio::spawn(std::future::pending::<()>());
    let aborted = task.abort_handle();
    event_loop.running_tool_tasks.insert("tool".into(), task);
    event_loop
        .handle_stream_event(StreamEvent::Error(INCOMPLETE_STREAM.into()))
        .await;
    tokio::task::yield_now().await;
    assert!(aborted.is_finished());
    assert!(event_loop.running_tool_tasks.is_empty());
    assert!(event_loop.app_state.pending_tool_results.is_empty());
    assert_eq!(event_loop.app_state.queued_count(), 1);
    assert!(!event_loop.stream_done_received);
    assert!(
        event_loop
            .app_state
            .messages
            .iter()
            .any(|m| m.content.contains(INCOMPLETE_STREAM))
    );
}

#[tokio::test]
async fn runtime_contract_remote_observation_does_not_start_local_work() {
    let mut event_loop = EventLoop::new(AppState::default());
    event_loop.handle_stream_event(StreamEvent::ToolCall {
        id: "remote1".into(), name: "Create".into(),
        arguments: serde_json::json!({"remote":true,"file_path":"never-written","content":"remote"}),
    }).await;
    assert!(event_loop.running_tool_tasks.is_empty());
    assert!(event_loop.pending_assistant_tool_calls.is_empty());
    assert!(event_loop.app_state.pending_approval.is_none());
}

#[tokio::test]
async fn runtime_contract_incomplete_error_renders_at_narrow_and_wide_sizes() {
    for (width, height) in [(40, 12), (120, 40)] {
        let mut state = AppState::default();
        state.terminal_size = (width, height);
        state.show_launch_splash = false;
        state.opt_in_banner = false;
        state.set_view(AppView::Session);
        let mut event_loop = EventLoop::new(state);
        event_loop
            .handle_stream_event(StreamEvent::Error(INCOMPLETE_STREAM.into()))
            .await;
        let config = CaptureConfig::minimal(width, height);
        let mut terminal = MockTerminal::from_config(config.clone()).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    MinimalSessionView::new(event_loop.app_state()),
                    frame.area(),
                );
            })
            .unwrap();
        let snapshot = terminal.snapshot().to_ascii(&config);
        let compact: String = snapshot.split_whitespace().collect();
        assert!(
            compact.contains("taskisincomplete."),
            "{width}x{height}: {snapshot}"
        );
        assert!(!compact.contains("Tasksucceeded"), "{snapshot}");
    }
}

struct StreamFixture {
    events: Vec<ResponseEvent>,
    silent: bool,
    cancels: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    capabilities: cortex_engine::client::ModelCapabilities,
}
#[async_trait::async_trait]
impl cortex_engine::client::ModelClient for StreamFixture {
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
        if self.silent {
            return std::future::pending().await;
        }
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
    async fn cancel_turn(&self) {
        self.cancels.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn runtime_contract_forwarder_rejects_incomplete_and_local_handoffs() {
    use cortex_engine::client::{CompletionResponse, FinishReason, ToolCallEvent};
    for events in [
        vec![],
        vec![ResponseEvent::Done(CompletionResponse {
            finish_reason: FinishReason::Length,
            ..Default::default()
        })],
        vec![ResponseEvent::ToolCall(ToolCallEvent {
            id: "a".into(),
            name: "Read".into(),
            arguments: "{}".into(),
            remote: false,
        })],
        vec![
            ResponseEvent::ToolCall(ToolCallEvent {
                id: "a".into(),
                name: "Read".into(),
                arguments: "[]".into(),
                remote: true,
            }),
            ResponseEvent::Done(CompletionResponse::default()),
        ],
    ] {
        let cancels = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let fixture = StreamFixture {
            events,
            silent: false,
            cancels: cancels.clone(),
            capabilities: Default::default(),
        };
        let (tx, mut rx) = mpsc::channel(8);
        forward_code_stream(
            Box::new(fixture),
            CompletionRequest::default(),
            Default::default(),
            tx,
        )
        .await;
        let mut errors = 0;
        while let Some(event) = rx.recv().await {
            assert!(!matches!(event, StreamEvent::Done { .. }));
            if matches!(event, StreamEvent::Error(_)) {
                errors += 1;
            }
        }
        assert_eq!(errors, 1);
        assert_eq!(cancels.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn runtime_contract_forwarder_cancels_silent_initial_request() {
    let cancels = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let fixture = StreamFixture {
        events: vec![],
        silent: true,
        cancels: cancels.clone(),
        capabilities: Default::default(),
    };
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (tx, mut rx) = mpsc::channel(8);
    let task = tokio::spawn(forward_code_stream(
        Box::new(fixture),
        CompletionRequest::default(),
        cancelled.clone(),
        tx,
    ));
    cancelled.store(true, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(rx.recv().await, Some(StreamEvent::Error(error)) if error.contains("Cancelled"))
    );
    assert_eq!(cancels.load(Ordering::SeqCst), 1);
}
