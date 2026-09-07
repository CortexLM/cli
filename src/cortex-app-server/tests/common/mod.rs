//! Shared loopback harness for the app-server integration tests.
//!
//! Nothing here contacts the coding service. The engine transport is pointed at a
//! closed loopback port, so a real turn fails exactly the way the product fails
//! when the coding service is unreachable; no turn is ever simulated as
//! successful. All state lives under a temporary CORTEX_HOME.
//!
//! Each test binary that includes this module uses a different subset of the
//! re-exports, so unused ones are expected per binary.
#![allow(unused_imports, dead_code)]

pub use std::sync::{Arc, LazyLock};
pub use std::time::Duration;

pub use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
pub use cortex_app_server::session_manager::{CreateSessionOptions, SessionEvent};
pub use cortex_app_server::storage::{SessionStorage, StoredSession};
pub use cortex_app_server::{AppState, ServerConfig, create_router_with_state};
pub use futures::StreamExt;
pub use serde_json::{Value, json};
pub use tower::ServiceExt;

/// Redirect all engine/CLI state away from the real user home exactly once.
pub static SANDBOX: LazyLock<tempfile::TempDir> = LazyLock::new(|| {
    let dir = tempfile::tempdir().unwrap();
    // Safety: set before any test body observes the environment; LazyLock
    // guarantees this runs once, before the first isolated() caller proceeds.
    unsafe {
        std::env::set_var("HOME", dir.path());
        std::env::set_var("CORTEX_HOME", dir.path().join("cortex"));
        std::env::set_var("CORTEX_AUTH_TOKEN", "local-fixture-token");
        // Closed loopback port: a real turn fails as an unreachable service.
        std::env::set_var("CORTEX_API_URL", "http://127.0.0.1:1");
    }
    dir
});

pub struct Fixture {
    pub router: Router,
    pub state: Arc<AppState>,
    pub key: String,
    pub _storage: tempfile::TempDir,
}

pub async fn fixture(mut config: ServerConfig) -> Fixture {
    LazyLock::force(&SANDBOX);
    let storage = tempfile::tempdir().unwrap();
    let key = uuid::Uuid::new_v4().to_string();
    config.auth.enabled = true;
    config.auth.api_keys = vec![key.clone()];
    config.rate_limit.enabled = false;
    config.sessions.storage_path = Some(storage.path().to_path_buf());
    let state = Arc::new(AppState::new(config).await.unwrap());
    Fixture {
        router: create_router_with_state(Arc::clone(&state)),
        state,
        key,
        _storage: storage,
    }
}

pub struct Live {
    pub id: String,
}

impl Fixture {
    /// Create a session through the authenticated route so it is owned by `self.key`.
    pub async fn create_owned_session(&self) -> Live {
        let response = self
            .router
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/cli/sessions",
                Some(&self.key),
                Some(json!({})),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        Live {
            id: json_body(response).await["id"].as_str().unwrap().to_owned(),
        }
    }
}

pub fn request(method: &str, path: &str, key: Option<&str>, body: Option<Value>) -> Request<Body> {
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

pub async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}

/// Drive the real engine turn to its terminal event and return the event names.
pub async fn drain_turn(
    events: &mut tokio::sync::broadcast::Receiver<SessionEvent>,
) -> Vec<(String, Option<String>)> {
    let mut seen = Vec::new();
    while let Ok(Ok(event)) = tokio::time::timeout(Duration::from_secs(30), events.recv()).await {
        let name = event.event.msg.to_string();
        let terminal = matches!(
            event.event.msg,
            cortex_protocol::EventMsg::Error(_)
                | cortex_protocol::EventMsg::TaskComplete(_)
                | cortex_protocol::EventMsg::TurnAborted(_)
                | cortex_protocol::EventMsg::ShutdownComplete
        );
        seen.push((name, event.turn_id.clone()));
        if terminal {
            break;
        }
    }
    seen
}
