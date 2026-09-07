use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use cortex_app_server::auth::{AuthService, Claims};
use cortex_app_server::{AppState, ServerConfig, create_router_with_state};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn configured(mut config: ServerConfig) -> (Router, Arc<AppState>, String) {
    let key = uuid::Uuid::new_v4().to_string();
    config.auth.enabled = true;
    config.auth.api_keys = vec![key.clone()];
    let state = Arc::new(AppState::new(config).await.unwrap());
    (create_router_with_state(Arc::clone(&state)), state, key)
}

fn request(method: &str, path: &str, key: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(key) = key {
        builder = builder.header("Authorization", format!("ApiKey {key}"));
    }
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    builder
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}

#[tokio::test]
async fn test_session_crud_and_message_storage_use_real_router() {
    let (router, state, key) = configured(ServerConfig::default()).await;
    let response = router
        .clone()
        .oneshot(request(
            "POST",
            "/api/v1/sessions",
            Some(&key),
            Some(json!({"model":"local-qa"})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let session = json_body(response).await;
    let path = format!("/api/v1/sessions/{}", session["id"].as_str().unwrap());
    assert_eq!(session["model"], "local-qa");
    let response = router
        .clone()
        .oneshot(request(
            "POST",
            &format!("{path}/messages"),
            Some(&key),
            Some(json!({"content":"local fixture"})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["content"], "local fixture");
    let response = router
        .clone()
        .oneshot(request(
            "GET",
            &format!("{path}/messages"),
            Some(&key),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(json_body(response).await.as_array().unwrap().len(), 1);
    let response = router
        .clone()
        .oneshot(request("GET", &path, Some(&key), None))
        .await
        .unwrap();
    assert_eq!(json_body(response).await["message_count"], 1);
    let response = router
        .clone()
        .oneshot(request("DELETE", &path, Some(&key), None))
        .await
        .unwrap();
    assert_eq!(json_body(response).await["deleted"], true);
    assert_eq!(
        router
            .oneshot(request("GET", &path, Some(&key), None))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let metrics = state.get_metrics().await;
    assert_eq!(metrics.sessions_created, 1);
    assert_eq!(metrics.active_sessions, 0);
    assert_eq!(metrics.total_requests, 6);
}

#[tokio::test]
async fn test_auth_is_enforced_for_rest_websocket_and_health_prefix() {
    let (router, _, key) = configured(ServerConfig::default()).await;
    for path in [
        "/api/v1/sessions",
        "/api/v1/metrics",
        "/api/v1/ws",
        "/api/v1/health/sessions",
        "/api/v1/admin/stats",
    ] {
        let response = router
            .clone()
            .oneshot(request("GET", path, None, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        assert!(response.headers().contains_key("x-request-id"));
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    }
    let response = router
        .clone()
        .oneshot(request("GET", "/api/v1/sessions", Some("invalid"), None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        router
            .clone()
            .oneshot(request("GET", "/api/v1/health", None, None))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        router
            .oneshot(request("GET", "/api/v1/sessions", Some(&key), None))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn test_correlation_is_validated_and_query_data_is_not_reflected() {
    let (router, _, key) = configured(ServerConfig::default()).await;
    let trace = cortex_common::diagnostics::TraceContext::default();
    let mut req = request(
        "GET",
        "/api/v1/sessions?token=private-fixture",
        Some(&key),
        None,
    );
    req.headers_mut()
        .insert("x-request-id", "private-fixture".parse().unwrap());
    req.headers_mut()
        .insert("traceparent", trace.traceparent().parse().unwrap());
    let response = router.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(uuid::Uuid::parse_str(response.headers()["x-request-id"].to_str().unwrap()).is_ok());
    assert!(
        response.headers()["traceparent"]
            .to_str()
            .unwrap()
            .contains(trace.trace_id())
    );
    assert_ne!(response.headers()["traceparent"], trace.traceparent());
    assert!(response.headers().contains_key("x-response-time"));
    assert_eq!(json_body(response).await, json!([]));
}

#[tokio::test]
async fn test_rate_limit_is_live_and_health_remains_available() {
    let mut config = ServerConfig::default();
    config.rate_limit.burst_size = 1;
    config.rate_limit.requests_per_minute = 0;
    let (router, state, key) = configured(config).await;
    assert_eq!(
        router
            .clone()
            .oneshot(request("GET", "/api/v1/sessions", Some(&key), None))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let response = router
        .clone()
        .oneshot(request("GET", "/api/v1/sessions", Some(&key), None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["retry-after"], "60");
    assert_eq!(
        router
            .oneshot(request("GET", "/api/v1/health", None, None))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(state.get_metrics().await.rate_limit_hits, 1);
}

#[tokio::test]
async fn test_limits_reject_oversized_bodies_with_and_without_content_length() {
    let mut config = ServerConfig::default();
    config.max_body_size = 64;
    let (router, _, key) = configured(config).await;
    for content_length in [false, true] {
        let mut req = request(
            "POST",
            "/api/v1/sessions",
            Some(&key),
            Some(json!({"model":"x".repeat(1000)})),
        );
        if content_length {
            req.headers_mut()
                .insert("content-length", "1012".parse().unwrap());
        }
        assert_eq!(
            router.clone().oneshot(req).await.unwrap().status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
}

#[tokio::test]
async fn test_cors_denies_unknown_origins_and_allows_configured_origin() {
    let mut config = ServerConfig::default();
    config.cors_origins = vec!["http://127.0.0.1:3000".into()];
    let (router, _, _) = configured(config).await;
    for (origin, allowed) in [
        ("http://127.0.0.1:3000", true),
        ("https://untrusted.example", false),
    ] {
        let req = Request::builder()
            .method("OPTIONS")
            .uri("/api/v1/sessions")
            .header("origin", origin)
            .header("access-control-request-method", "POST")
            .body(Body::empty())
            .unwrap();
        let response = router.clone().oneshot(req).await.unwrap();
        assert_eq!(
            response
                .headers()
                .contains_key("access-control-allow-origin"),
            allowed
        );
    }
}

#[tokio::test]
async fn test_jwt_validation_and_admin_role_are_enforced() {
    let mut config = ServerConfig::default();
    config.auth.jwt_secret = Some(uuid::Uuid::new_v4().to_string());
    let service = AuthService::new(config.auth.clone());
    let token = service.generate_token("fixture-user").unwrap();
    assert_eq!(service.validate_token(&token).unwrap().sub, "fixture-user");
    let (router, _, _) = configured(config.clone()).await;
    let req = Request::builder()
        .uri("/api/v1/admin/stats")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router.clone().oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let mut claims = Claims::new("fixture-user", 3600).with_role("admin");
    claims.iss = "wrong-issuer".into();
    let key =
        jsonwebtoken::EncodingKey::from_secret(config.auth.jwt_secret.as_ref().unwrap().as_bytes());
    let wrong = jsonwebtoken::encode(&jsonwebtoken::Header::default(), &claims, &key).unwrap();
    assert!(service.validate_token(&wrong).is_err());
    let req = Request::builder()
        .uri("/api/v1/sessions")
        .header("authorization", format!("Bearer {wrong}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router.oneshot(req).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn test_disabled_health_and_metrics_and_unsafe_readiness_fail() {
    let mut config = ServerConfig::default();
    config.health_enabled = false;
    config.metrics_enabled = false;
    let (router, _, key) = configured(config).await;
    for path in ["/api/v1/health", "/api/v1/metrics"] {
        assert_eq!(
            router
                .clone()
                .oneshot(request("GET", path, Some(&key), None))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    let mut config = ServerConfig::default();
    config.listen_addr = "0.0.0.0:55554".into();
    assert!(config.validate().is_err());
    let state = Arc::new(AppState::new(config).await.unwrap());
    let router = create_router_with_state(state);
    assert_eq!(
        router
            .oneshot(request("GET", "/api/v1/health", None, None))
            .await
            .unwrap()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn test_metrics_and_session_updates_do_not_deadlock() {
    let (router, state, key) = configured(ServerConfig::default()).await;
    let metrics = async {
        for _ in 0..100 {
            state.get_metrics().await;
            tokio::task::yield_now().await;
        }
    };
    let sessions = async {
        for _ in 0..5 {
            router
                .clone()
                .oneshot(request(
                    "POST",
                    "/api/v1/sessions",
                    Some(&key),
                    Some(json!({})),
                ))
                .await
                .unwrap();
        }
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(metrics, sessions);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_internal_errors_do_not_disclose_private_details() {
    use axum::response::IntoResponse;
    let response =
        cortex_app_server::AppError::Internal("private-fixture /home/user".into()).into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let value = json_body(response).await;
    assert_eq!(
        value["error"]["message"],
        "The coding service is temporarily unavailable"
    );
}

#[tokio::test]
async fn test_file_mutations_are_confined_to_the_open_workspace() {
    let cwd = std::env::current_dir().unwrap();
    let inside = tempfile::tempdir_in(cwd).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let original = outside.path().join("private");
    std::fs::write(&original, "outside fixture").unwrap();
    let directory = inside.path().join("nested");
    let file = directory.join("file");
    let renamed = directory.join("renamed");
    let escaped = outside.path().join("escaped");
    let (router, _, key) = configured(ServerConfig::default()).await;
    let cases = [
        ("/files/mkdir", json!({"path":directory}), StatusCode::OK),
        (
            "/files/write",
            json!({"path":file,"content":"fixture"}),
            StatusCode::OK,
        ),
        (
            "/files/mkdir",
            json!({"path":escaped}),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/files/rename",
            json!({"old_path":original,"new_path":renamed}),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/files/rename",
            json!({"old_path":file,"new_path":escaped}),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/files/rename",
            json!({"old_path":".","new_path":escaped}),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/files/rename",
            json!({"old_path":file,"new_path":renamed}),
            StatusCode::OK,
        ),
        ("/files/delete", json!({"path":renamed}), StatusCode::OK),
    ];
    for (path, body, expected) in cases {
        let response = router
            .clone()
            .oneshot(request(
                "POST",
                &format!("/api/v1{path}"),
                Some(&key),
                Some(body),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{path}");
    }
    assert!(!escaped.exists());
    assert!(!renamed.exists());
    assert_eq!(
        std::fs::read_to_string(original).unwrap(),
        "outside fixture"
    );
}
