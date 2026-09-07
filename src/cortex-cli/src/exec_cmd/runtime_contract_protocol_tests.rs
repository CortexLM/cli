//! Deterministic session/protocol fixtures; no auth lookup or live service.
use super::*;
use cortex_engine::Session;
use cortex_engine::client::{
    CompletionRequest, CompletionResponse, ModelCapabilities, ModelClient, ResponseEvent,
    ResponseStream,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Fixture {
    calls: AtomicUsize,
    cancelled: Arc<AtomicUsize>,
    capabilities: ModelCapabilities,
}

// Spell out the async-trait ABI because this crate has no direct macro
// dependency. No production dependency is added just for this fixture.
impl ModelClient for Fixture {
    fn model(&self) -> &str {
        "cortex-1-mini"
    }
    fn provider(&self) -> &str {
        "runtime-fixture"
    }
    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }
    fn owns_tool_execution(&self) -> bool {
        true
    }
    fn complete<'life0, 'async_trait>(
        &'life0 self,
        _: CompletionRequest,
    ) -> Pin<Box<dyn Future<Output = cortex_engine::Result<ResponseStream>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            if self.calls.fetch_add(1, Ordering::SeqCst) > 0 {
                return std::future::pending().await;
            }
            Ok(Box::pin(futures::stream::iter([Ok(ResponseEvent::Done(
                CompletionResponse::default(),
            ))])) as ResponseStream)
        })
    }
    fn complete_sync<'life0, 'async_trait>(
        &'life0 self,
        _: CompletionRequest,
    ) -> Pin<
        Box<dyn Future<Output = cortex_engine::Result<CompletionResponse>> + Send + 'async_trait>,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async { unreachable!("This fixture is streaming-only") })
    }
    fn cancel_turn<'life0, 'async_trait>(
        &'life0 self,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            self.cancelled.fetch_add(1, Ordering::SeqCst);
        })
    }
}

struct Frames {
    pending: Vec<u8>,
    sender: mpsc::UnboundedSender<Value>,
}
impl Write for Frames {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        for line in self
            .pending
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
        {
            self.sender
                .send(serde_json::from_slice(line).unwrap())
                .unwrap();
        }
        self.pending.clear();
        Ok(())
    }
}

async fn wait_for(frames: &mut mpsc::UnboundedReceiver<Value>, method: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let frame = frames.recv().await.expect("protocol output");
            assert_eq!(frame["jsonrpc"], "2.0");
            if frame["method"] == method {
                return frame;
            }
        }
    })
    .await
    .expect("protocol made no progress")
}

#[tokio::test]
async fn runtime_contract_protocol_two_turns_cancel_and_shutdown_without_eof() {
    let temp = tempfile::tempdir().unwrap();
    let config = Config {
        cwd: temp.path().into(),
        cortex_home: temp.path().join("state"),
        ..Default::default()
    };
    let cancelled = Arc::new(AtomicUsize::new(0));
    let client = Fixture {
        calls: AtomicUsize::new(0),
        cancelled: cancelled.clone(),
        capabilities: ModelCapabilities::default(),
    };
    let (mut session, handle) = Session::with_client(config.clone(), Box::new(client)).unwrap();
    let session_id = handle.conversation_id.to_string();
    let session_task = tokio::spawn(async move { session.run().await });
    let (input, lines) = mpsc::channel(8);
    let (sender, mut frames) = mpsc::unbounded_channel();
    let protocol = tokio::spawn(async move {
        let mut output = Frames {
            pending: Vec::new(),
            sender,
        };
        let result = run_protocol(&handle, lines, &mut output, &config, None, false, 2, 5).await;
        cortex_engine::session::control::stop_session(&handle, session_task)
            .await
            .unwrap();
        result
    });
    assert_eq!(
        wait_for(&mut frames, "initialized").await["params"]["session_id"],
        session_id
    );
    input
        .send(Ok(
            json!({"jsonrpc":"2.0","id":1,"method":"message","params":{"text":"first"}})
                .to_string(),
        ))
        .await
        .unwrap();
    let completed = wait_for(&mut frames, "task_complete").await;
    assert_eq!(completed["params"]["request_id"], 1);
    input
        .send(Ok(
            json!({"jsonrpc":"2.0","id":2,"method":"message","params":{"text":"second"}})
                .to_string(),
        ))
        .await
        .unwrap();
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(3), frames.recv())
            .await
            .unwrap()
            .unwrap();
        if frame["params"]["event"]["type"] == "TaskStarted" {
            break;
        }
    }
    input
        .send(Ok(
            json!({"jsonrpc":"2.0","id":3,"method":"cancel"}).to_string()
        ))
        .await
        .unwrap();
    assert_eq!(
        wait_for(&mut frames, "turn_aborted").await["params"]["request_id"],
        2
    );
    assert_eq!(cancelled.load(Ordering::SeqCst), 1);
    input
        .send(Ok(
            json!({"jsonrpc":"2.0","id":4,"method":"shutdown"}).to_string()
        ))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), protocol)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    // The sender is still alive: shutdown did not depend on EOF.
    assert!(input.is_closed());
}

#[test]
fn runtime_contract_unsupported_options_fail_before_input() {
    use crate::exec_cmd::ExecCli;
    use crate::run_cmd::RunCli;
    use clap::Parser;
    for args in [
        vec!["run", "--attach", "http://127.0.0.1:1"],
        vec!["run", "--temperature", "0.5"],
        vec!["run", "--schema", "{}"],
    ] {
        assert!(
            RunCli::try_parse_from(args)
                .unwrap()
                .validate_runtime_options()
                .is_err()
        );
    }
    for args in [
        vec!["exec", "--output-schema", "{}"],
        vec!["exec", "--reasoning-effort", "high"],
        vec!["exec", "--max-turns", "0"],
    ] {
        assert!(
            ExecCli::try_parse_from(args)
                .unwrap()
                .validate_runtime_options()
                .is_err()
        );
    }
}

#[test]
fn runtime_contract_approval_requires_evidence_and_allowed_risk() {
    let mut request = ExecApprovalRequestEvent {
        call_id: "fixture".into(),
        turn_id: "1".into(),
        command: vec!["cat".into(), "file".into()],
        cwd: "/tmp".into(),
        sandbox_assessment: None,
    };
    assert_eq!(
        approval_decision(&request, Some(AutonomyLevel::High), false),
        ReviewDecision::Denied
    );
    request.sandbox_assessment = Some(cortex_protocol::SandboxCommandAssessment {
        risk_level: cortex_protocol::SandboxRiskLevel::High,
        explanation: "fixture".into(),
    });
    assert_eq!(
        approval_decision(&request, Some(AutonomyLevel::Low), false),
        ReviewDecision::Denied
    );
    request.sandbox_assessment.as_mut().unwrap().risk_level =
        cortex_protocol::SandboxRiskLevel::Low;
    assert_eq!(
        approval_decision(&request, Some(AutonomyLevel::ReadOnly), false),
        ReviewDecision::Approved
    );
}
