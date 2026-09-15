use super::*;
use crate::client::Message;
use futures::StreamExt;

#[test]
fn parses_tool_start_event() {
    let raw = r#"{"type":"tool_start","invocation_id":"tci_1","tool_name":"python","label":"Ran Python"}"#;
    let ev: CodeTurnEvent = serde_json::from_str(raw).unwrap();
    match ev {
        CodeTurnEvent::ToolStart { tool_name, .. } => assert_eq!(tool_name, "python"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn parses_done_event() {
    let raw = r#"{"type":"done","message_id":"msg_1","finish_reason":"stop"}"#;
    let ev: CodeTurnEvent = serde_json::from_str(raw).unwrap();
    assert!(matches!(ev, CodeTurnEvent::Done { .. }));
}

#[test]
fn last_user_message_picks_latest() {
    let req = CompletionRequest {
        messages: vec![
            Message::system("sys"),
            Message::user("first"),
            Message::assistant("ok"),
            Message::user("second"),
        ],
        model: "cortex-1-mini".into(),
        ..Default::default()
    };
    assert_eq!(CodeAgentClient::last_user_message(&req), "second");
}

#[test]
fn guest_token_prefix_is_stable() {
    assert_eq!(GUEST_TOKEN_PREFIX, "gt:");
}

#[test]
fn parses_tool_start_with_arguments() {
    let raw = r#"{"type":"tool_start","invocation_id":"tci_2","tool_name":"Bash","arguments":{"command":"ls"}}"#;
    let ev: CodeTurnEvent = serde_json::from_str(raw).unwrap();
    match ev {
        CodeTurnEvent::ToolStart {
            tool_name,
            arguments,
            ..
        } => {
            assert_eq!(tool_name, "Bash");
            assert_eq!(arguments.unwrap()["command"].as_str().unwrap(), "ls");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn parses_cancelled_and_question() {
    let cancelled: CodeTurnEvent =
        serde_json::from_str(r#"{"type":"cancelled","message_id":"m"}"#).unwrap();
    assert!(matches!(cancelled, CodeTurnEvent::Cancelled { .. }));
    let q: CodeTurnEvent = serde_json::from_str(
        r#"{"type":"question","invocation_id":"q1","questions":{"prompt":"Ship it?"}}"#,
    )
    .unwrap();
    match q {
        CodeTurnEvent::Question { invocation_id, .. } => assert_eq!(invocation_id, "q1"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn turn_mode_only_chat_or_code() {
    assert_eq!(CodeTurnMode::Code.as_str(), "code");
    assert_eq!(CodeTurnMode::Chat.as_str(), "chat");
}

#[test]
fn api_errors_are_product_facing() {
    let err = map_api_error(
        reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        r#"{"detail":"nope"}"#,
    );
    let msg = err.user_friendly_message();
    assert!(
        msg.contains("temporarily unavailable"),
        "expected product error, got {msg}"
    );
    assert!(!msg.to_lowercase().contains("reqwest"));
    assert!(!msg.contains("nope"));
}

#[test]
fn api_401_asks_the_user_to_login() {
    let err = map_api_error(reqwest::StatusCode::UNAUTHORIZED, r#"{"detail":"nope"}"#);
    let msg = err.user_friendly_message();
    assert!(
        msg.contains("cortex login") || msg.contains("CORTEX_API_KEY"),
        "{msg}"
    );
    assert!(!msg.contains("temporarily unavailable"), "{msg}");
    assert!(!msg.contains("nope"));
}

#[test]
fn api_404_is_not_a_false_outage() {
    let err = map_api_error(reqwest::StatusCode::NOT_FOUND, r#"{"detail":"missing"}"#);
    let msg = err.to_string();
    assert!(
        msg.contains("not found") || msg.contains("CORTEX_API_URL"),
        "{msg}"
    );
    assert!(!msg.contains("temporarily unavailable"), "{msg}");
    assert!(!msg.contains("missing"));
}

#[test]
fn backend_error_display_keeps_actionable_copy() {
    let err = CortexError::BackendError {
        message: AUTH_REQUIRED.to_string(),
    };
    assert_eq!(err.to_string(), AUTH_REQUIRED);
    assert_eq!(err.user_friendly_message(), AUTH_REQUIRED);
}

#[test]
fn normalize_api_base_strips_trailing_slash() {
    assert_eq!(
        normalize_api_base("https://api.cortex.foundation/"),
        "https://api.cortex.foundation"
    );
    assert_eq!(
        CodeAgentClient::new(Some("https://api.cortex.foundation/".into()), None).base_url(),
        "https://api.cortex.foundation"
    );
}

#[test]
fn sse_error_maps_auth_without_outage_copy() {
    assert_eq!(map_sse_error_copy("unauthorized"), AUTH_REQUIRED);
    assert_eq!(map_sse_error_copy(""), SERVICE_UNAVAILABLE);
    assert_eq!(map_sse_error_copy("rate limit 429"), TOO_MANY_REQUESTS);
}

#[test]
fn persists_session_id_under_cortex_home() {
    let dir = std::env::temp_dir().join(format!("cortex-sess-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let prev = std::env::var_os("CORTEX_HOME");
    unsafe {
        std::env::set_var("CORTEX_HOME", &dir);
    }
    persist_session_id("/tmp/demo-ws", "sess_abc");
    assert_eq!(
        cached_code_session_id("/tmp/demo-ws").as_deref(),
        Some("sess_abc")
    );
    match prev {
        Some(v) => unsafe { std::env::set_var("CORTEX_HOME", v) },
        None => unsafe { std::env::remove_var("CORTEX_HOME") },
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn tool_start_without_args_is_display_only() {
    let args = serde_json::json!({"label": "Ran Python"});
    let has_exec_args = args.get("command").is_some()
        || args.get("file_path").is_some()
        || args.get("path").is_some()
        || args
            .as_object()
            .is_some_and(|o| o.len() > 1 && !o.contains_key("label"));
    assert!(!has_exec_args);
}

fn live_api_enabled() -> bool {
    std::env::var("CORTEX_LIVE_API").ok().as_deref() == Some("1")
}

#[tokio::test]
#[ignore = "hits api.cortex.foundation; CORTEX_LIVE_API=1 cargo test -p cortex-engine -- --ignored"]
async fn live_guest_code_turn_streams_tokens() {
    if !live_api_enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    unsafe {
        std::env::set_var("CORTEX_HOME", dir.path());
    }
    let client = CodeAgentClient::new(None, None);
    let guest = client
        .begin_guest_session()
        .await
        .expect("guest session against the live API");
    assert_eq!(guest.kind, "guest");
    assert!(guest.user_id.starts_with("usr_"));

    client.set_turn_context(CodeTurnContext {
        workspace: Some("/tmp".into()),
        computer: ComputerKind::Cloud,
        turn_mode: Some(CodeTurnMode::Chat),
        ssh_target: None,
        fast_mode: false,
    });

    let mut stream = client
        .stream_turn(
            "Reply with the single word pong and nothing else.",
            CodeTurnMode::Chat,
        )
        .await
        .expect("Code session turn");
    let mut text = String::new();
    let mut done = false;
    while let Some(ev) = stream.next().await {
        match ev.expect("SSE event") {
            ResponseEvent::Delta(d) => text.push_str(&d),
            ResponseEvent::Done(_) => {
                done = true;
                break;
            }
            ResponseEvent::Error(e) => panic!("turn error: {e}"),
            _ => {}
        }
    }
    assert!(done, "expected a done event from the Code turn stream");
    assert!(
        text.to_lowercase().contains("pong"),
        "expected streamed pong, got {text:?}"
    );

    let sid = client
        .current_session_id()
        .await
        .expect("session id after turn");
    let messages = client.list_messages(&sid).await.expect("transcript");
    assert!(
        messages.iter().any(|m| m.role == "user"),
        "server transcript should persist the user message"
    );

    client.cancel_in_flight().await;
}
