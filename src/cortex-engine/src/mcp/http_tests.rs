use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn peer(responses: Vec<String>) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/rpc", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut received = Vec::new();
        for response in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let (header_end, length) = loop {
                let mut byte = [0u8; 1];
                stream.read_exact(&mut byte).await.unwrap();
                bytes.push(byte[0]);
                assert!(bytes.len() < 16384);
                if bytes.ends_with(b"\r\n\r\n") {
                    let headers = String::from_utf8(bytes.clone()).unwrap().to_lowercase();
                    assert!(headers.contains("authorization: bearer deterministic-fixture"));
                    assert!(headers.contains("mcp-protocol-version: 2025-03-26"));
                    if !received.is_empty() {
                        assert!(headers.contains("mcp-session-id: fixture-session"));
                    }
                    let length = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .map(|n| n.parse::<usize>().unwrap())
                        .unwrap_or(0);
                    break (bytes.len(), length);
                }
            };
            bytes.resize(header_end + length, 0);
            stream.read_exact(&mut bytes[header_end..]).await.unwrap();
            if length > 0 {
                received.push(serde_json::from_slice(&bytes[header_end..]).unwrap());
            } else {
                received.push(Value::String("DELETE".into()));
            }
            stream.write_all(response.as_bytes()).await.unwrap();
        }
        received
    });
    (url, task)
}
fn response(body: &str, content_type: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nConnection: close\r\nMcp-Session-Id: fixture-session\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}
#[tokio::test]
async fn streamable_http_json_sse_and_session_headers_are_correlated() {
    let replies = vec![
        response(
            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}}}"#,
            "application/json",
        ),
        response("", "application/json"),
        response(
            "data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\",\"params\":{}}\r\n\r\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[{\"name\":\"echo\",\"inputSchema\":{\"type\":\"object\"}}]}}\r\n\r\n",
            "text/event-stream",
        ),
        response(
            r#"{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"fixture"}],"isError":false}}"#,
            "application/json",
        ),
        response("", "application/json"),
    ];
    let (url, peer) = peer(replies).await;
    let mut config = McpServerConfig::new("http-fixture", "");
    config.transport = TransportType::Http;
    config.sse_url = Some(url);
    config.headers.insert(
        "Authorization".into(),
        "Bearer deterministic-fixture".into(),
    );
    let client = McpClient::with_timeout(config, Duration::from_secs(3));
    client.connect().await.unwrap();
    client.call_tool("echo", None).await.unwrap();
    client.disconnect().await.unwrap();
    let requests = tokio::time::timeout(Duration::from_secs(3), peer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(requests[1]["method"], "notifications/initialized");
    assert!(requests[1].get("id").is_none());
    assert!(requests[1].get("params").is_none());
    assert!(requests[3]["params"].get("arguments").is_none());
    assert_eq!(requests[4], "DELETE");
}
#[tokio::test]
async fn http_wrong_response_id_fails_initialization() {
    let (url, peer) = peer(vec![response(
        r#"{"jsonrpc":"2.0","id":999,"result":{}}"#,
        "application/json",
    )])
    .await;
    let mut config = McpServerConfig::new("http-fixture", "");
    config.transport = TransportType::Http;
    config.sse_url = Some(url);
    config.headers.insert(
        "Authorization".into(),
        "Bearer deterministic-fixture".into(),
    );
    let client = McpClient::with_timeout(config, Duration::from_secs(3));
    assert!(client.connect().await.is_err());
    assert_eq!(client.state().await, ConnectionState::Failed);
    peer.await.unwrap();
}
