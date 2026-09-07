use super::*;
use crate::tools::{ToolDefinition, ToolRegistry};
use cortex_protocol::SandboxPolicy;
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn fixture(
    response: &str,
    limit: usize,
) -> (ToolRegistry, String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let response = response.to_owned();
    let server = tokio::spawn(async move {
        let (mut connection, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let count = connection.read(&mut request).await.unwrap();
        assert!(
            std::str::from_utf8(&request[..count])
                .unwrap()
                .contains("host: fixture.test:")
        );
        connection.write_all(response.as_bytes()).await.unwrap();
    });
    let mut handler = FetchUrlHandler::with_config(SsrfConfig::new().max_response_size(limit));
    // Compile-time-only injection: no tool arguments or environment can enable it.
    handler.test_destination = Some(("fixture.test".into(), address));
    let mut registry = ToolRegistry::new();
    registry.register_with_handler(
        ToolDefinition::new("FetchUrl", "fixture", json!({})),
        Arc::new(handler),
    );
    (
        registry,
        format!("http://fixture.test:{}/", address.port()),
        server,
    )
}

fn context(root: &std::path::Path) -> ToolContext {
    ToolContext::new(root.into())
        .with_auto_approve(true)
        .with_sandbox_policy(SandboxPolicy::DangerFullAccess)
        .with_allowed_fetch_host("fixture.test")
}

#[tokio::test]
async fn security_boundary_pinned_fetch_uses_allowed_host() {
    let root = tempfile::tempdir().unwrap();
    let (registry, url, server) = fixture(
        "HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nfixture",
        100,
    )
    .await;
    let result = registry
        .execute_with_context("FetchUrl", json!({"url":url}), context(root.path()))
        .await
        .unwrap();
    assert!(result.success, "{}", result.output);
    assert_eq!(result.output, "fixture");
    server.await.unwrap();
}

#[tokio::test]
async fn security_boundary_redirect_rechecks_private_destination() {
    let root = tempfile::tempdir().unwrap();
    let (registry, url, server) = fixture("HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", 100).await;
    let result = registry
        .execute_with_context("FetchUrl", json!({"url":url}), context(root.path()))
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.output.contains("Blocked"));
    server.await.unwrap();
}

#[tokio::test]
async fn security_boundary_fetch_stream_limit_without_content_length() {
    let root = tempfile::tempdir().unwrap();
    let (registry, url, server) = fixture(
        "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nexceeds-bound",
        4,
    )
    .await;
    let result = registry
        .execute_with_context("FetchUrl", json!({"url":url}), context(root.path()))
        .await
        .unwrap();
    assert!(!result.success && result.output.contains("size limit"));
    server.await.unwrap();
}

#[tokio::test]
async fn security_boundary_url_arguments_cannot_allow_unapproved_host() {
    let root = tempfile::tempdir().unwrap();
    let registry = ToolRegistry::new();
    let context = context(root.path());
    for url in [
        "http://127.1/",
        "http://[::ffff:127.0.0.1]/",
        "http://[64:ff9b::a9fe:a9fe]/",
        "http://localhost./",
        "https://unapproved.invalid/",
    ] {
        let result = registry
            .execute_with_context(
                "FetchUrl",
                json!({"url":url,"allowed_domains":["unapproved.invalid"]}),
                context.clone(),
            )
            .await
            .unwrap();
        assert!(!result.success, "{url}");
    }
}

#[test]
fn security_boundary_all_dns_answers_must_be_public() {
    let handler = FetchUrlHandler::new();
    assert!(
        handler
            .validate_destinations(&["1.1.1.1:443".parse().unwrap()])
            .is_ok()
    );
    assert!(
        handler
            .validate_destinations(&[
                "1.1.1.1:443".parse().unwrap(),
                "127.0.0.1:443".parse().unwrap()
            ])
            .is_err()
    );
    assert!(handler.validate_destinations(&[]).is_err());
}
