//! Ownership, stored-session and compatibility-surface coverage.
mod common;
use common::*;

#[tokio::test]
async fn live_sessions_are_owned_and_never_shared_across_principals() {
    LazyLock::force(&SANDBOX);
    let storage = tempfile::tempdir().unwrap();
    // Two distinct credentials on the same server.
    let owner = uuid::Uuid::new_v4().to_string();
    let other = uuid::Uuid::new_v4().to_string();
    let mut config = ServerConfig::default();
    config.auth.enabled = true;
    config.auth.api_keys = vec![owner.clone(), other.clone()];
    config.rate_limit.enabled = false;
    config.sessions.storage_path = Some(storage.path().to_path_buf());
    let state = Arc::new(AppState::new(config).await.unwrap());
    let router = create_router_with_state(Arc::clone(&state));

    let created = json_body(
        router
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/cli/sessions",
                Some(&owner),
                Some(json!({})),
            ))
            .await
            .unwrap(),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_owned();
    let base = format!("/api/v1/cli/sessions/{id}");

    // The other principal cannot see, drive, interrupt, fork or delete it.
    for (method, path, body) in [
        ("GET", base.clone(), None),
        ("GET", format!("{base}/events"), None),
        (
            "POST",
            format!("{base}/chat"),
            Some(json!({"content":"stolen"})),
        ),
        ("POST", format!("{base}/interrupt"), None),
        (
            "POST",
            format!("{base}/approve"),
            Some(json!({"call_id":"x","approved":true})),
        ),
        (
            "POST",
            format!("{base}/fork"),
            Some(json!({"message_index":0})),
        ),
        ("DELETE", base.clone(), None),
    ] {
        assert_eq!(
            router
                .clone()
                .oneshot(request(method, &path, Some(&other), body))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND,
            "{method} {path}"
        );
    }
    // Listing is scoped per principal, and the stored record is too.
    assert_eq!(
        json_body(
            router
                .clone()
                .oneshot(request("GET", "/api/v1/cli/sessions", Some(&other), None))
                .await
                .unwrap()
        )
        .await,
        json!([])
    );
    assert_eq!(
        json_body(
            router
                .clone()
                .oneshot(request(
                    "GET",
                    "/api/v1/stored-sessions",
                    Some(&other),
                    None
                ))
                .await
                .unwrap()
        )
        .await,
        json!([])
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request(
                "GET",
                &format!("/api/v1/stored-sessions/{id}/history"),
                Some(&other),
                None
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    // The owner still has full access; the session was never transferred.
    assert_eq!(
        router
            .oneshot(request("GET", &base, Some(&owner), None))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    state.cli_sessions.destroy_session(&id).await.unwrap();
}

#[tokio::test]
async fn stored_session_routes_reject_forged_and_unauthenticated_identifiers() {
    let f = fixture(ServerConfig::default()).await;
    // A non-UUID ID must not become a storage path lookup.
    for id in ["../../etc/passwd", "not-a-uuid", "..%2f..%2fescape"] {
        for suffix in ["", "/history"] {
            let response = f
                .router
                .clone()
                .oneshot(request(
                    "GET",
                    &format!("/api/v1/stored-sessions/{id}{suffix}"),
                    Some(&f.key),
                    None,
                ))
                .await
                .unwrap();
            assert!(
                response.status() == StatusCode::NOT_FOUND
                    || response.status() == StatusCode::BAD_REQUEST,
                "{id}{suffix}: {}",
                response.status()
            );
        }
    }
    // A well-formed but unknown UUID is not found, not an internal error.
    let unknown = uuid::Uuid::new_v4();
    assert_eq!(
        f.router
            .clone()
            .oneshot(request(
                "GET",
                &format!("/api/v1/stored-sessions/{unknown}"),
                Some(&f.key),
                None
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    // Every stored-session route refuses unauthenticated access.
    for (method, path) in [
        ("GET", "/api/v1/stored-sessions".to_owned()),
        ("GET", format!("/api/v1/stored-sessions/{unknown}")),
        ("DELETE", format!("/api/v1/stored-sessions/{unknown}")),
        ("GET", format!("/api/v1/stored-sessions/{unknown}/history")),
    ] {
        assert_eq!(
            f.router
                .clone()
                .oneshot(request(method, &path, None, None))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
    }
}

#[tokio::test]
async fn a_legacy_unowned_record_is_not_exposed_to_an_authenticated_principal() {
    let f = fixture(ServerConfig::default()).await;
    let id = uuid::Uuid::new_v4().to_string();
    // A record written before ownership existed carries no user_id.
    SessionStorage::new(f._storage.path())
        .unwrap()
        .save_session(&StoredSession {
            user_id: None,
            id: id.clone(),
            model: "legacy".into(),
            cwd: ".".into(),
            created_at: 0,
            updated_at: 0,
            title: None,
        })
        .unwrap();
    for (method, suffix) in [("GET", ""), ("DELETE", ""), ("GET", "/history")] {
        assert_eq!(
            f.router
                .clone()
                .oneshot(request(
                    method,
                    &format!("/api/v1/stored-sessions/{id}{suffix}"),
                    Some(&f.key),
                    None
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND,
            "{method}{suffix}"
        );
    }
    assert_eq!(
        json_body(
            f.router
                .oneshot(request(
                    "GET",
                    "/api/v1/stored-sessions",
                    Some(&f.key),
                    None
                ))
                .await
                .unwrap()
        )
        .await,
        json!([])
    );
}

#[tokio::test]
async fn unsupported_session_options_and_escaping_cwds_are_refused() {
    let f = fixture(ServerConfig::default()).await;
    let outside = tempfile::tempdir().unwrap();
    for body in [
        // A second model provider is not a supported session option.
        json!({"provider":"another-vendor"}),
        // A working directory outside the server workspace is refused.
        json!({"cwd": outside.path()}),
        json!({"cwd": "../../.."}),
        // A working directory that does not exist is refused.
        json!({"cwd": "definitely-missing-fixture-dir"}),
    ] {
        let response = f
            .router
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/cli/sessions",
                Some(&f.key),
                Some(body.clone()),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT, "{body}");
    }
    assert_eq!(f.state.cli_sessions.count().await, 0);
}

#[tokio::test]
async fn the_concurrent_session_limit_is_enforced() {
    let mut config = ServerConfig::default();
    config.sessions.max_concurrent = 1;
    let f = fixture(config).await;
    let first = f
        .router
        .clone()
        .oneshot(request(
            "POST",
            "/api/v1/cli/sessions",
            Some(&f.key),
            Some(json!({})),
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let id = json_body(first).await["id"].as_str().unwrap().to_owned();
    assert_eq!(
        f.router
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/cli/sessions",
                Some(&f.key),
                Some(json!({}))
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    // Freeing the slot makes room again; the limit is not a permanent wedge.
    f.state.cli_sessions.destroy_session(&id).await.unwrap();
    assert_eq!(
        f.router
            .oneshot(request(
                "POST",
                "/api/v1/cli/sessions",
                Some(&f.key),
                Some(json!({}))
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    f.state.cli_sessions.shutdown_all().await;
    assert_eq!(f.state.cli_sessions.count().await, 0);
}

#[tokio::test]
async fn compatibility_completions_return_501_instead_of_a_fabricated_answer() {
    let f = fixture(ServerConfig::default()).await;
    let response = f
        .router
        .clone()
        .oneshot(request(
            "POST",
            "/api/v1/chat/completions",
            Some(&f.key),
            Some(json!({
                "model": "local-fixture",
                "messages": [{"role":"user","content":"hello"}]
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "not_implemented");
    assert_eq!(body["error"]["message"], "This operation is not available");
    // Nothing resembling a completion is returned.
    assert!(body.get("choices").is_none());

    // The compatibility route is still behind authentication.
    assert_eq!(
        f.router
            .oneshot(request("POST", "/api/v1/chat/completions", None, None))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn forking_reports_the_engines_real_capability_rather_than_a_placeholder() {
    let f = fixture(ServerConfig::default()).await;
    let info = f.create_owned_session().await;
    let response = f
        .router
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/v1/cli/sessions/{}/fork", info.id),
            Some(&f.key),
            Some(json!({"message_index":0})),
        ))
        .await
        .unwrap();
    match response.status() {
        // A fork rebuilds the transcript from the original session's rollout.
        StatusCode::OK => {
            let forked = json_body(response).await;
            let forked_id = forked["id"].as_str().unwrap();
            // A fork is a distinct conversation, not an alias of the original.
            assert_ne!(forked_id, info.id);
            assert_eq!(forked["conversation_id"].as_str().unwrap(), forked_id);
            assert_eq!(f.state.cli_sessions.count().await, 2);
            // Both remain independently addressable by their owner.
            for id in [&info.id, &forked_id.to_owned()] {
                assert_eq!(
                    f.router
                        .clone()
                        .oneshot(request(
                            "GET",
                            &format!("/api/v1/cli/sessions/{id}"),
                            Some(&f.key),
                            None
                        ))
                        .await
                        .unwrap()
                        .status(),
                    StatusCode::OK
                );
            }
        }
        // Or it refuses, because this session has no forkable history yet.
        // Either way it never fabricates a session it did not build.
        status => {
            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(f.state.cli_sessions.count().await, 1);
        }
    }
    f.state.cli_sessions.shutdown_all().await;
}

#[tokio::test]
async fn a_failed_turn_is_still_recorded_in_the_owners_readable_history() {
    let f = fixture(ServerConfig::default()).await;
    let info = f.create_owned_session().await;
    let mut events = f.state.cli_sessions.subscribe(&info.id).await.unwrap();
    f.state
        .cli_sessions
        .submit_message(&info.id, "recorded prompt".into())
        .await
        .unwrap();
    drain_turn(&mut events).await;

    let history = json_body(
        f.router
            .clone()
            .oneshot(request(
                "GET",
                &format!("/api/v1/stored-sessions/{}/history", info.id),
                Some(&f.key),
                None,
            ))
            .await
            .unwrap(),
    )
    .await;
    let messages = history.as_array().unwrap();
    // The user's own prompt is persisted; no assistant reply is invented for
    // a turn that never reached the coding service.
    assert!(
        messages
            .iter()
            .any(|m| m["role"] == "user" && m["content"] == "recorded prompt"),
        "{history}"
    );
    assert!(
        !messages.iter().any(|m| m["role"] == "assistant"),
        "{history}"
    );
    f.state.cli_sessions.shutdown_all().await;
}

#[tokio::test]
async fn websocket_routes_refuse_unauthenticated_upgrades() {
    let f = fixture(ServerConfig::default()).await;
    let id = uuid::Uuid::new_v4();
    for path in [
        "/api/v1/ws".to_owned(),
        format!("/api/v1/ws/sessions/{id}"),
        // A token in the query string must not authenticate an upgrade.
        format!("/api/v1/ws?token={}", f.key),
    ] {
        let req = Request::builder()
            .uri(&path)
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            f.router.clone().oneshot(req).await.unwrap().status(),
            StatusCode::UNAUTHORIZED,
            "{path}"
        );
    }
}
