use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

#[tokio::test]
async fn network_transport_is_rejected_before_bind() {
    let server = AcpServer::new(Config::default());
    for addr in ["0.0.0.0:0", "127.0.0.1:0", "[::]:0"] {
        assert!(
            server
                .run_http(addr.parse().unwrap())
                .await
                .unwrap_err()
                .to_string()
                .contains("unsupported")
        );
    }
}

#[tokio::test]
async fn stdio_correlates_ids_and_recovers_after_malformed_input() {
    let server = AcpServer::new(Config::default());
    let (mut client_input, server_input) = tokio::io::duplex(4096);
    let (server_output, client_output) = tokio::io::duplex(4096);
    let task = tokio::spawn(async move { server.run_io(server_input, server_output).await });
    let mut output = BufReader::new(client_output).lines();
    client_input.write_all(b"{bad json}\n{\"jsonrpc\":\"2.0\",\"id\":\"init\",\"method\":\"initialize\",\"params\":{\"protocolVersion\":1}}\n").await.unwrap();
    let first: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
    assert!(first["id"].is_null());
    assert_eq!(first["error"]["code"], -32700);
    let second: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(second["id"], "init");
    assert_eq!(second["result"]["protocolVersion"], 1);
    assert_eq!(
        second["result"]["agentCapabilities"]["promptCapabilities"]["image"],
        false
    );
    client_input.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"session/cancel\",\"params\":{\"sessionId\":\"missing\"}}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"missing\"}\n").await.unwrap();
    let third: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(third["id"], 2);
    assert_eq!(third["error"]["code"], -32601);
    drop(client_input);
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn bounded_reader_rejects_oversized_input() {
    let input = vec![b'a'; MAX_MESSAGE_BYTES + 1];
    assert!(
        read_bounded_line(&mut BufReader::new(input.as_slice()))
            .await
            .is_err()
    );
}

#[test]
fn acp_v1_update_fields_are_camel_case() {
    use crate::acp::types::*;
    let value = serde_json::to_value(SessionUpdate::ToolCallUpdate {
        tool_call_id: "call-1".into(),
        status: ToolStatus::Completed,
        content: None,
        raw_output: Some(Value::Bool(true)),
    })
    .unwrap();
    assert_eq!(value["sessionUpdate"], "tool_call_update");
    assert_eq!(value["toolCallId"], "call-1");
    assert_eq!(value["rawOutput"], true);
    assert!(value.get("tool_call_id").is_none());
}

#[tokio::test]
async fn cancel_is_read_while_prompt_pending_and_notifications_do_not_steal_completion() {
    use cortex_protocol::{
        AgentMessageDeltaEvent, Event, EventMsg, Op, TurnAbortReason, TurnAbortedEvent,
    };
    let server = AcpServer::new(Config::default());
    let (id, submissions, events) = server.handler.install_protocol_peer().await;
    let (mut input, server_input) = tokio::io::duplex(4096);
    let (server_output, output) = tokio::io::duplex(4096);
    let task = tokio::spawn(async move { server.run_io(server_input, server_output).await });
    let mut lines = BufReader::new(output).lines();
    input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"initialize\",\"params\":{\"protocolVersion\":1}}\n").await.unwrap();
    lines.next_line().await.unwrap().unwrap();
    let prompt = serde_json::json!({"jsonrpc":"2.0","id":"turn-request","method":"session/prompt","params":{"sessionId":id,"prompt":[{"type":"text","text":"fixture"}]}});
    let cancel =
        serde_json::json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":id}});
    input
        .write_all(format!("{prompt}\n{cancel}\n").as_bytes())
        .await
        .unwrap();
    let turn = tokio::time::timeout(std::time::Duration::from_secs(2), submissions.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(turn.op, Op::UserInput { .. }));
    assert!(matches!(
        tokio::time::timeout(std::time::Duration::from_secs(2), submissions.recv())
            .await
            .unwrap()
            .unwrap()
            .op,
        Op::Interrupt
    ));
    events
        .send(Event {
            id: turn.id.clone(),
            msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
                delta: "fixture delta".into(),
            }),
        })
        .await
        .unwrap();
    events
        .send(Event {
            id: turn.id,
            msg: EventMsg::TurnAborted(TurnAbortedEvent {
                reason: TurnAbortReason::Interrupted,
            }),
        })
        .await
        .unwrap();
    let mut seen_update = false;
    let mut seen_cancel = false;
    for _ in 0..2 {
        let line = tokio::time::timeout(std::time::Duration::from_secs(2), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        if value.get("method").is_some() {
            seen_update = true;
            assert!(value.get("id").is_none());
            assert_eq!(value["params"]["sessionId"], id);
        } else {
            seen_cancel = true;
            assert_eq!(value["id"], "turn-request");
            assert_eq!(value["result"]["stopReason"], "cancelled");
        }
    }
    assert!(seen_update && seen_cancel);
    drop(input);
    task.await.unwrap().unwrap();
}

#[test]
fn invalid_request_envelopes_are_not_reported_as_json_syntax_errors() {
    assert_eq!(decode_frame(b"{").unwrap_err().error.unwrap().code, -32700);
    let error = decode_frame(br#"{"jsonrpc":"2.0","id":"known"}"#).unwrap_err();
    assert_eq!(error.id, AcpRequestId::String("known".into()));
    assert_eq!(error.error.unwrap().code, -32600);
    assert_eq!(
        decode_frame(br#"{"jsonrpc":"2.0","id":null,"method":"initialize"}"#)
            .unwrap_err()
            .error
            .unwrap()
            .code,
        -32600
    );
}
