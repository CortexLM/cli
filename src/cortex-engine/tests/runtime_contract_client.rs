//! Deterministic loopback wire contracts, not live coding-service evidence.

use cortex_engine::client::runtime_contract::{
    INCOMPLETE_STREAM, LOCAL_TOOLS_UNSUPPORTED, code_message,
};
use cortex_engine::client::{
    CodeAgentClient, CodeTurnContext, CodeTurnMode, CompletionRequest, ComputerKind, ContentPart,
    CortexClient, FinishReason, Message, MessageContent, ModelClient, ResponseEvent,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_stream::StreamExt;

struct Reply {
    path: &'static str,
    status: u16,
    body: String,
    sse: bool,
}

async fn peer(replies: Vec<Reply>) -> (String, tokio::task::JoinHandle<Vec<serde_json::Value>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut bodies = Vec::new();
        for reply in replies {
            let (mut socket, _) =
                tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                    .await
                    .unwrap()
                    .unwrap();
            let mut bytes = Vec::new();
            let header_end = loop {
                let mut chunk = [0; 1024];
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                assert!(bytes.len() < 1024 * 1024);
                if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            assert!(headers.lines().next().unwrap().contains(reply.path));
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < header_end + length {
                let mut chunk = [0; 1024];
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
            }
            bodies.push(if length == 0 {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap()
            });
            let content_type = if reply.sse {
                "text/event-stream"
            } else {
                "application/json"
            };
            let response = format!(
                "HTTP/1.1 {} OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                reply.status,
                reply.body.len(),
                reply.body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
        bodies
    });
    (url, task)
}

fn client(url: String) -> CortexClient {
    let client = CortexClient::new("cortex-1-mini".into(), Some(url))
        .with_auth_token("contract-fixture-not-a-credential".into());
    client.configure_code_turn(CodeTurnContext {
        computer: ComputerKind::Cloud,
        turn_mode: Some(CodeTurnMode::Code),
        ..Default::default()
    });
    client
}

fn creation() -> Reply {
    Reply {
        path: "/v1/code/sessions",
        status: 200,
        body: r#"{"id":"session_fixture","model_slug":"cortex-1-mini"}"#.into(),
        sse: false,
    }
}

fn turn(body: &str) -> Reply {
    Reply {
        path: "/v1/code/sessions/session_fixture/turns",
        status: 200,
        body: body.into(),
        sse: true,
    }
}

fn request() -> CompletionRequest {
    CompletionRequest {
        model: "cortex-1-mini".into(),
        messages: vec![
            Message::system("Project rule: preserve tests."),
            Message::user("Inspect only."),
        ],
        ..Default::default()
    }
}

#[tokio::test]
#[serial_test::serial]
async fn default_detect_ensures_a_cloud_session() {
    let mut fixture = cortex_engine::testing::TestFixture::new();
    fixture.unset_env("CORTEX_COMPUTER");
    fixture.unset_env("CORTEX_SSH_HOST");
    fixture.unset_env("CORTEX_SSH_TARGET");

    let (url, peer) = peer(vec![creation()]).await;
    let client = CodeAgentClient::new(Some(url), Some("contract-fixture-not-a-credential".into()));
    assert_eq!(
        client.turn_context().computer,
        ComputerKind::Cloud,
        "unset CORTEX_COMPUTER must default to Cloud"
    );
    let id = client
        .ensure_session()
        .await
        .expect("Cloud can create a session");
    assert_eq!(id, "session_fixture");
    let bodies = peer.await.unwrap();
    assert_eq!(bodies[0]["runtime"], "cloud");
}

#[tokio::test]
async fn runtime_contract_remote_tools_are_observations_even_with_arguments() {
    let (url, peer) = peer(vec![creation(), turn(concat!(
        "data: {\"type\":\"tool_start\",\"invocation_id\":\"read1\",\"tool_name\":\"Read\",\"arguments\":{\"file_path\":\"source.rs\"}}\n\n",
        "data: {\"type\":\"tool_end\",\"invocation_id\":\"read1\",\"outcome\":\"ok\",\"output\":\"exact result\"}\n\n",
        "data: {\"type\":\"text_delta\",\"delta\":\"Checked.\"}\n\n",
        "data: {\"type\":\"done\",\"finish_reason\":\"stop\"}\n\n"
    ))]).await;
    let mut stream = client(url).complete(request()).await.unwrap();
    assert!(
        matches!(stream.next().await.unwrap().unwrap(), ResponseEvent::ToolCall(call) if call.remote)
    );
    assert!(
        matches!(stream.next().await.unwrap().unwrap(), ResponseEvent::ToolResult { output, .. } if output == "exact result")
    );
    assert!(matches!(
        stream.next().await.unwrap().unwrap(),
        ResponseEvent::Delta(_)
    ));
    assert!(
        matches!(stream.next().await.unwrap().unwrap(), ResponseEvent::Done(response) if response.tool_calls.is_empty() && response.finish_reason == FinishReason::Stop)
    );
    assert!(stream.next().await.is_none());
    let bodies = peer.await.unwrap();
    assert_eq!(bodies[0]["model_slug"], "cortex-1-mini");
    assert_eq!(bodies[0]["runtime"], "cloud");
    assert_eq!(bodies[1]["mode"], "code");
    assert_eq!(bodies[1]["interaction"], "agent");
    assert!(
        bodies[1]["message"]
            .as_str()
            .unwrap()
            .contains("Project rule: preserve tests.")
    );
    assert!(bodies[1].get("messages").is_none());
}

#[tokio::test]
async fn runtime_contract_eof_malformed_and_local_handoffs_fail() {
    for body in [
        "",
        "data: {broken}\n\n",
        "data: {\"type\":\"text_delta\",\"delta\":\"partial\"}\n\n",
        "data: {\"type\":\"tool_start\",\"remote\":false,\"invocation_id\":\"a\",\"tool_name\":\"Read\",\"arguments\":{\"path\":\"x\"}}\n\n",
        "data: {\"type\":\"question\",\"invocation_id\":\"a\",\"questions\":{}}\n\n",
    ] {
        let (url, peer) = peer(vec![creation(), turn(body)]).await;
        let mut stream = client(url).complete(request()).await.unwrap();
        let mut failure = None;
        while let Some(event) = stream.next().await {
            match event {
                Err(error) => failure = Some(error.to_string()),
                Ok(ResponseEvent::Done(_)) => panic!("Incomplete response reported success"),
                _ => {}
            }
        }
        let error = failure.expect("required terminal failure");
        assert!(error.contains(INCOMPLETE_STREAM) || error.contains(LOCAL_TOOLS_UNSUPPORTED));
        peer.await.unwrap();
    }
}

#[tokio::test]
async fn runtime_contract_finish_reasons_and_missing_session_do_not_fall_back() {
    for reason in ["length", "tool_calls", "content_filter", "unexpected"] {
        let body = format!("data: {{\"type\":\"done\",\"finish_reason\":\"{reason}\"}}\n\n");
        let (url, peer) = peer(vec![creation(), turn(&body)]).await;
        assert!(client(url).complete_sync(request()).await.is_err());
        peer.await.unwrap();
    }
    let (url, peer) = peer(vec![Reply {
        path: "/v1/code/sessions/missing",
        status: 404,
        body: "{}".into(),
        sse: false,
    }])
    .await;
    let client = client(url);
    client.resume_code_session("missing").await.unwrap();
    assert!(client.complete(request()).await.is_err());
    assert_eq!(peer.await.unwrap().len(), 1);
}

#[test]
fn runtime_contract_local_results_and_generation_settings_are_rejected() {
    let mut req = request();
    req.messages
        .push(Message::tool_result("read1", "the exact local result"));
    assert!(
        code_message(&req)
            .unwrap_err()
            .to_string()
            .contains(LOCAL_TOOLS_UNSUPPORTED)
    );
    req = request();
    req.temperature = Some(0.7);
    assert!(code_message(&req).is_err());
    req = request();
    req.max_tokens = Some(100);
    assert!(code_message(&req).is_err());
    req = request();
    req.seed = Some(42);
    assert!(code_message(&req).is_err());
}

#[test]
fn runtime_contract_all_text_parts_survive_but_images_are_rejected() {
    let mut req = request();
    req.messages.last_mut().unwrap().content = MessageContent::Parts(vec![
        ContentPart::Text {
            text: "first".into(),
            cache_control: None,
        },
        ContentPart::Text {
            text: "second".into(),
            cache_control: None,
        },
    ]);
    assert!(code_message(&req).unwrap().ends_with("first\nsecond"));
    req.messages.last_mut().unwrap().content = MessageContent::Parts(vec![ContentPart::Image {
        url: "data:image/png;base64,fixture".into(),
        detail: None,
    }]);
    assert!(code_message(&req).is_err());
}

#[tokio::test]
async fn runtime_contract_cancellation_status_is_checked() {
    for status in [200, 500] {
        let (url, peer) = peer(vec![Reply {
            path: "/v1/code/sessions/session_fixture/cancel",
            status,
            body: "{}".into(),
            sse: false,
        }])
        .await;
        let client = client(url);
        client.resume_code_session("session_fixture").await.unwrap();
        assert_eq!(client.cancel_turn_checked().await.is_ok(), status == 200);
        assert_eq!(peer.await.unwrap(), vec![serde_json::json!({})]);
    }
}

#[tokio::test]
async fn runtime_contract_identity_resume_and_origin_are_bound() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("sessions")).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let stop = "data: {\"type\":\"done\",\"finish_reason\":\"stop\"}\n\n";
    let (url, peer) = peer(vec![
        creation(),
        turn(stop),
        Reply {
            path: "/v1/code/sessions/session_fixture",
            ..creation()
        },
        turn(stop),
    ])
    .await;
    let original = client(url.clone());
    original
        .configure_session_identity(temp.path(), &id, false)
        .unwrap();
    original.complete_sync(request()).await.unwrap();
    let resumed = client(url);
    resumed
        .configure_session_identity(temp.path(), &id, true)
        .unwrap();
    assert_eq!(
        resumed.code_session_id().await.as_deref(),
        Some("session_fixture")
    );
    resumed.complete_sync(request()).await.unwrap();
    assert_eq!(peer.await.unwrap().len(), 4);
    let wrong_origin = client("http://127.0.0.1:1".into());
    assert!(
        wrong_origin
            .configure_session_identity(temp.path(), &id, true)
            .is_err()
    );
    assert!(
        wrong_origin
            .configure_session_identity(temp.path(), &uuid::Uuid::new_v4().to_string(), true)
            .is_err()
    );
}

#[tokio::test]
async fn runtime_contract_fresh_child_does_not_reuse_parent_identity() {
    let stop = "data: {\"type\":\"done\",\"finish_reason\":\"stop\"}\n\n";
    let (url, peer) = peer(vec![
        creation(),
        turn(stop),
        Reply {
            body: r#"{"id":"child_fixture","model_slug":"cortex-1-mini"}"#.into(),
            ..creation()
        },
        Reply {
            path: "/v1/code/sessions/child_fixture/turns",
            ..turn(stop)
        },
    ])
    .await;
    let parent = client(url);
    parent.complete_sync(request()).await.unwrap();
    let child = parent.fresh_clone_box().expect("isolated transport");
    assert_eq!(child.code_session_id().await, None);
    child.complete_sync(request()).await.unwrap();
    assert_eq!(
        child.code_session_id().await.as_deref(),
        Some("child_fixture")
    );
    assert_eq!(
        parent.code_session_id().await.as_deref(),
        Some("session_fixture")
    );
    assert_eq!(peer.await.unwrap().len(), 4);
}
