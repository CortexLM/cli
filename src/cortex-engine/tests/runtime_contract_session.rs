//! In-process event fixtures exercise session control, not model quality.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use cortex_engine::client::{
    CompletionRequest, CompletionResponse, FinishReason, ModelCapabilities, ModelClient,
    ResponseEvent, ResponseStream, ToolCallEvent,
};
use cortex_engine::session::control::{next_event, stop_session};
use cortex_engine::{Config, Result, Session, SessionHandle};
use cortex_protocol::{EventMsg, Op, Submission, UserInput};

struct EventFixture {
    events: Vec<ResponseEvent>,
    silent: bool,
    cancel_fails: bool,
    cancelled: Arc<AtomicUsize>,
    capabilities: ModelCapabilities,
}

#[async_trait]
impl ModelClient for EventFixture {
    fn model(&self) -> &str {
        "cortex-1-mini"
    }
    fn provider(&self) -> &str {
        "contract-fixture"
    }
    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }
    fn owns_tool_execution(&self) -> bool {
        true
    }
    async fn complete(&self, _: CompletionRequest) -> Result<ResponseStream> {
        if self.silent {
            return std::future::pending().await;
        }
        Ok(Box::pin(tokio_stream::iter(
            self.events.clone().into_iter().map(Ok),
        )))
    }
    async fn complete_sync(&self, _: CompletionRequest) -> Result<CompletionResponse> {
        Err(cortex_engine::CortexError::InvalidInput(
            "Not a synchronous fixture.".into(),
        ))
    }
    async fn cancel_turn(&self) {
        self.cancelled.fetch_add(1, Ordering::SeqCst);
    }
    async fn cancel_turn_checked(&self) -> Result<()> {
        self.cancel_turn().await;
        if self.cancel_fails {
            return Err(cortex_engine::CortexError::BackendError {
                message: cortex_engine::client::runtime_contract::REMOTE_CANCEL_UNCONFIRMED.into(),
            });
        }
        Ok(())
    }
}

async fn start(
    events: Vec<ResponseEvent>,
    silent: bool,
) -> (
    tempfile::TempDir,
    SessionHandle,
    tokio::task::JoinHandle<Result<()>>,
    Arc<AtomicUsize>,
) {
    let temp = tempfile::tempdir().unwrap();
    let config = Config {
        cwd: temp.path().into(),
        cortex_home: temp.path().join("state"),
        ..Default::default()
    };
    let cancelled = Arc::new(AtomicUsize::new(0));
    let client = EventFixture {
        events,
        silent,
        cancel_fails: false,
        cancelled: cancelled.clone(),
        capabilities: ModelCapabilities::default(),
    };
    let (mut session, handle) = Session::with_client(config, Box::new(client)).unwrap();
    let task = tokio::spawn(async move { session.run().await });
    (temp, handle, task, cancelled)
}

async fn submit(handle: &SessionHandle, op: Op) {
    handle
        .submission_tx
        .send(Submission {
            id: uuid::Uuid::new_v4().to_string(),
            op,
        })
        .await
        .unwrap();
}

fn user() -> Op {
    Op::UserInput {
        items: vec![UserInput::Text {
            text: "Fixture turn".into(),
        }],
    }
}

async fn terminal(handle: &SessionHandle) -> EventMsg {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(3), handle.event_rx.recv())
            .await
            .unwrap()
            .unwrap();
        if matches!(
            event.msg,
            EventMsg::Error(_) | EventMsg::TaskComplete(_) | EventMsg::TurnAborted(_)
        ) {
            return event.msg;
        }
    }
}

#[tokio::test]
async fn runtime_contract_only_explicit_stop_completes() {
    for events in [
        vec![],
        vec![ResponseEvent::Delta("partial".into())],
        vec![ResponseEvent::Done(CompletionResponse {
            finish_reason: FinishReason::Length,
            ..Default::default()
        })],
        vec![ResponseEvent::Error(
            "The coding service is temporarily unavailable".into(),
        )],
    ] {
        let (_temp, handle, task, _) = start(events, false).await;
        submit(&handle, user()).await;
        assert!(matches!(terminal(&handle).await, EventMsg::Error(_)));
        while let Ok(event) = handle.event_rx.try_recv() {
            assert!(!matches!(event.msg, EventMsg::TaskComplete(_)));
        }
        stop_session(&handle, task).await.unwrap();
    }
    let (_temp, handle, task, _) = start(
        vec![ResponseEvent::Done(CompletionResponse::default())],
        false,
    )
    .await;
    submit(&handle, user()).await;
    assert!(matches!(terminal(&handle).await, EventMsg::TaskComplete(_)));
    submit(&handle, user()).await;
    assert!(matches!(terminal(&handle).await, EventMsg::TaskComplete(_)));
    stop_session(&handle, task).await.unwrap();
}

#[tokio::test]
async fn runtime_contract_interrupt_preempts_silent_initial_request() {
    let (_temp, handle, task, cancelled) = start(vec![], true).await;
    submit(&handle, user()).await;
    loop {
        let event = handle.event_rx.recv().await.unwrap();
        if matches!(event.msg, EventMsg::TaskStarted(_)) {
            break;
        }
    }
    submit(&handle, Op::Interrupt).await;
    assert!(matches!(terminal(&handle).await, EventMsg::TurnAborted(_)));
    assert_eq!(cancelled.load(Ordering::SeqCst), 1);
    stop_session(&handle, task).await.unwrap();
}

#[tokio::test]
async fn runtime_contract_remote_call_never_runs_a_local_tool() {
    let (_temp, handle, task, _) = start(
        vec![
            ResponseEvent::ToolCall(ToolCallEvent {
                id: "observation".into(),
                name: "Create".into(),
                arguments: r#"{"file_path":"must-not-exist","content":"bad"}"#.into(),
                remote: true,
            }),
            ResponseEvent::ToolResult {
                id: "observation".into(),
                success: true,
                output: "remote output".into(),
            },
            ResponseEvent::Done(CompletionResponse::default()),
        ],
        false,
    )
    .await;
    submit(&handle, user()).await;
    assert!(matches!(terminal(&handle).await, EventMsg::TaskComplete(_)));
    assert!(!_temp.path().join("must-not-exist").exists());
    stop_session(&handle, task).await.unwrap();
}

#[tokio::test]
async fn runtime_contract_local_invocation_fails_before_execution() {
    let (_temp, handle, task, _) = start(
        vec![
            ResponseEvent::ToolCall(ToolCallEvent {
                id: "local".into(),
                name: "Create".into(),
                arguments: r#"{"file_path":"must-not-exist","content":"bad"}"#.into(),
                remote: false,
            }),
            ResponseEvent::Done(CompletionResponse::default()),
        ],
        false,
    )
    .await;
    submit(&handle, user()).await;
    assert!(matches!(terminal(&handle).await, EventMsg::Error(_)));
    assert!(!_temp.path().join("must-not-exist").exists());
    stop_session(&handle, task).await.unwrap();
}

#[tokio::test]
async fn runtime_contract_deadline_does_not_need_an_event() {
    let (_temp, handle, task, _) = start(vec![], true).await;
    assert!(matches!(
        handle.event_rx.recv().await.unwrap().msg,
        EventMsg::SessionConfigured(_)
    ));
    let error = next_event(
        &handle,
        Some(tokio::time::Instant::now() + Duration::from_millis(20)),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, cortex_engine::CortexError::Timeout));
    stop_session(&handle, task).await.unwrap();
}

#[tokio::test]
async fn runtime_contract_shutdown_preserves_unconfirmed_remote_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    let config = Config {
        cwd: temp.path().into(),
        cortex_home: temp.path().join("state"),
        ..Default::default()
    };
    let client = EventFixture {
        events: vec![],
        silent: true,
        cancel_fails: true,
        cancelled: Arc::new(AtomicUsize::new(0)),
        capabilities: Default::default(),
    };
    let (mut session, handle) = Session::with_client(config, Box::new(client)).unwrap();
    let task = tokio::spawn(async move { session.run().await });
    submit(&handle, user()).await;
    loop {
        let event = tokio::time::timeout(Duration::from_secs(3), handle.event_rx.recv())
            .await
            .unwrap()
            .unwrap();
        if matches!(event.msg, EventMsg::TaskStarted(_)) {
            break;
        }
    }
    let error = stop_session(&handle, task).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains(cortex_engine::client::runtime_contract::REMOTE_CANCEL_UNCONFIRMED)
    );
}

#[tokio::test]
async fn runtime_contract_configuration_uses_canonical_project_and_rejects_malformed_files() {
    if std::env::var_os("RUNTIME_CONTRACT_CONFIG_CHILD").is_none() {
        let temp = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runtime_contract_configuration_uses_canonical_project_and_rejects_malformed_files",
                "--nocapture",
            ])
            .env_clear()
            .env("HOME", temp.path())
            .env("CORTEX_HOME", temp.path().join("state"))
            .env("RUNTIME_CONTRACT_CONFIG_CHILD", "1")
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{}{}",
            String::from_utf8_lossy(&status.stdout),
            String::from_utf8_lossy(&status.stderr)
        );
        return;
    }
    let root = std::env::current_dir().unwrap().join("project");
    std::fs::create_dir_all(root.join(".cortex")).unwrap();
    std::fs::create_dir(root.join("child")).unwrap();
    let config_path = root.join(".cortex/config.toml");
    std::fs::write(&config_path, "model = \"cortex-1\"\n").unwrap();
    let config = cortex_engine::session::control::load_runtime_config(
        Some(root.join("child/..")),
        None,
        Some("Additional fixture instructions".into()),
    )
    .await
    .unwrap();
    assert_eq!(config.cwd, std::fs::canonicalize(&root).unwrap());
    assert_eq!(config.model, "cortex-1");
    assert_eq!(
        config.user_instructions.as_deref(),
        Some("Additional fixture instructions")
    );
    std::fs::write(config_path, "model = [ malformed\n").unwrap();
    assert!(
        cortex_engine::session::control::load_runtime_config(Some(root), None, None)
            .await
            .is_err()
    );
}
