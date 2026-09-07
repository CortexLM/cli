use super::*;

fn peer(mode: &str) -> McpServerConfig {
    McpServerConfig::new("fixture", "python3").args(["-u", "-c", r#"
import json, sys, os
mode=sys.argv[1]
for line in sys.stdin:
    r=json.loads(line)
    if 'id' not in r:
        if r['method']=='notifications/cancelled':
            print(json.dumps({'jsonrpc':'2.0','method':'fixture/cancelled','params':r['params']}),flush=True)
        continue
    if 'method' not in r: continue
    m=r['method']
    if m=='initialize':
        value={'protocolVersion':'unsupported' if mode=='version' else '2024-11-05','capabilities':{'tools':{}},'serverInfo':{'name':'fixture','version':'1'}}
    elif m=='tools/list':
        if mode=='malformed': print('not json',flush=True); continue
        if mode=='oversized': print('x'*1048577,flush=True); continue
        value={'tools':[{'name':'echo','inputSchema':{'type':'object'},'description':'fixture'}]}
    elif m=='wait': continue
    elif m=='tools/call':
        print(json.dumps({'jsonrpc':'2.0','method':'notifications/tools/list_changed'}),flush=True)
        print(json.dumps({'jsonrpc':'2.0','id':'reverse','method':'sampling/createMessage','params':{}}),flush=True)
        value={'content':[{'type':'text','text':json.dumps(r['params']['arguments'])}],'isError':False}
    else: value={'ok':True}
    print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':value}),flush=True)
"#, mode])
}

#[tokio::test]
async fn concurrent_calls_correlate_with_notifications_and_reverse_requests() {
    let client = McpClient::with_timeout(peer("ok"), Duration::from_secs(3));
    let mut notifications = client.subscribe_notifications();
    client.connect().await.unwrap();
    let (a, b) = tokio::join!(
        client.call_tool("echo", Some(json!({"value":"a b"}))),
        client.call_tool("echo", Some(json!({"value":2})))
    );
    assert!(
        serde_json::to_value(a.unwrap())
            .unwrap()
            .to_string()
            .contains("a b")
    );
    assert!(
        serde_json::to_value(b.unwrap())
            .unwrap()
            .to_string()
            .contains('2')
    );
    assert_eq!(
        notifications.recv().await.unwrap()["method"],
        "notifications/tools/list_changed"
    );
    assert_eq!(client.discovered_tools().await.unwrap().len(), 1);
    client.disconnect().await.unwrap();
    assert!(!client.is_connected().await);
}

#[tokio::test]
async fn invalid_version_malformed_and_oversized_peers_fail_connect() {
    for mode in ["version", "malformed", "oversized"] {
        let client = McpClient::with_timeout(peer(mode), Duration::from_secs(3));
        assert!(client.connect().await.is_err(), "{mode}");
        assert_eq!(client.state().await, ConnectionState::Failed);
        assert!(client.tools().await.is_empty());
    }
}

#[tokio::test]
async fn timeout_cancels_request_without_poisoning_next_response() {
    let client = McpClient::with_timeout(peer("ok"), Duration::from_millis(200));
    client.connect().await.unwrap();
    let mut notifications = client.subscribe_notifications();
    assert!(client.request::<Value>("wait", None).await.is_err());
    let cancelled = tokio::time::timeout(Duration::from_secs(2), notifications.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cancelled["method"], "fixture/cancelled");
    assert!(cancelled["params"]["requestId"].is_i64());
    assert_eq!(
        client.request::<Value>("ping", None).await.unwrap()["ok"],
        true
    );
    client.disconnect().await.unwrap();
}

#[tokio::test]
async fn dropping_request_sends_cancellation_while_peer_stays_open() {
    let client = Arc::new(McpClient::with_timeout(peer("ok"), Duration::from_secs(3)));
    client.connect().await.unwrap();
    let mut notifications = client.subscribe_notifications();
    let c = client.clone();
    let pending = tokio::spawn(async move { c.request::<Value>("wait", None).await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    pending.abort();
    let _ = pending.await;
    let event = tokio::time::timeout(Duration::from_secs(2), notifications.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event["method"], "fixture/cancelled");
    client.disconnect().await.unwrap();
}

#[tokio::test]
async fn unsupported_transports_reject_without_connection() {
    let client = McpClient::new(McpServerConfig::new_sse("fixture", "http://127.0.0.1:1"));
    assert!(
        client
            .connect()
            .await
            .unwrap_err()
            .to_string()
            .contains("unsupported")
    );
}

#[test]
fn endpoint_validation_rejects_unsafe_credentials_and_nonloopback_http() {
    for url in [
        "http://example.com/rpc",
        "https://user:secret@example.com",
        "file:///tmp/test",
        "https://example.com/#secret",
    ] {
        assert!(super::super::http::validate_endpoint(url).is_err());
    }
    assert!(super::super::http::validate_endpoint("http://127.0.0.1:42/rpc").is_ok());
}
