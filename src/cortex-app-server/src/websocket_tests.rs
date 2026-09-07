//! WebSocket frame handling against a real registry and real engine sessions.
//! These are local protocol peers, never coding-service success fixtures.
use super::*;

#[tokio::test]
async fn test_auth_frames_validate_identity_and_failed_auth_blocks_commands() {
    let mut config = crate::ServerConfig::default();
    config.auth.enabled = true;
    config.auth.jwt_secret = Some(Uuid::new_v4().to_string());
    let token = AuthService::new(config.auth.clone())
        .generate_token("fixture-user")
        .unwrap();
    let state = AppState::new(config).await.unwrap();
    let mut ctx = ConnectionContext {
        _id: "fixture".into(),
        user_id: None,
        session_id: None,
        subscription: None,
        authenticated: false,
        created_at: Instant::now(),
        last_ping: Instant::now(),
    };
    let (tx, mut rx) = mpsc::channel(4);
    let command = r#"{"type":"create_session","model":null,"cwd":null}"#;
    assert!(matches!(
        handle_text_message(command, &tx, &mut ctx, &state).await,
        Err(AppError::Authentication(_))
    ));
    let auth = serde_json::json!({"type":"auth","token":token}).to_string();
    handle_text_message(&auth, &tx, &mut ctx, &state)
        .await
        .unwrap();
    assert!(ctx.authenticated);
    assert_eq!(ctx.user_id.as_deref(), Some("jwt:fixture-user"));
    assert!(matches!(
        rx.recv().await,
        Some(WsMessage::AuthResult { success: true, .. })
    ));
    handle_text_message(
        r#"{"type":"auth","token":"invalid-fixture"}"#,
        &tx,
        &mut ctx,
        &state,
    )
    .await
    .unwrap();
    assert!(!ctx.authenticated);
    assert!(ctx.user_id.is_none());
    assert!(matches!(
        rx.recv().await,
        Some(WsMessage::AuthResult { success: false, .. })
    ));
    assert!(matches!(
        handle_text_message(command, &tx, &mut ctx, &state).await,
        Err(AppError::Authentication(_))
    ));
}

/// Point the engine at a temporary home and a closed loopback port, so real
/// sessions can be created offline and a real turn fails as an unreachable
/// coding service. `HOME` is deliberately left alone: other tests in this
/// binary read it, and only the engine consults `CORTEX_HOME`.
static ENGINE_SANDBOX: std::sync::LazyLock<tempfile::TempDir> = std::sync::LazyLock::new(|| {
    let dir = tempfile::tempdir().unwrap();
    // Safety: LazyLock runs this once, before any test body that forces
    // it observes these variables.
    unsafe {
        std::env::set_var("CORTEX_HOME", dir.path());
        std::env::set_var("CORTEX_AUTH_TOKEN", "local-fixture-token");
        std::env::set_var("CORTEX_API_URL", "http://127.0.0.1:1");
    }
    dir
});

/// A fixture connection that is already authenticated as `user`.
fn context(user: Option<&str>) -> ConnectionContext {
    ConnectionContext {
        _id: "fixture".into(),
        user_id: user.map(Into::into),
        session_id: None,
        subscription: None,
        authenticated: user.is_some(),
        created_at: Instant::now(),
        last_ping: Instant::now(),
    }
}

/// A server with authentication off and its own temporary session storage.
async fn open_state(storage: &tempfile::TempDir) -> AppState {
    std::sync::LazyLock::force(&ENGINE_SANDBOX);
    let mut config = crate::ServerConfig::default();
    config.auth.enabled = false;
    config.sessions.storage_path = Some(storage.path().to_path_buf());
    AppState::new(config).await.unwrap()
}

#[tokio::test]
async fn ping_and_status_frames_report_real_connection_state() {
    let storage = tempfile::tempdir().unwrap();
    let state = open_state(&storage).await;
    let mut ctx = context(None);
    let (tx, mut rx) = mpsc::channel(8);

    handle_text_message(r#"{"type":"ping","timestamp":99}"#, &tx, &mut ctx, &state)
        .await
        .unwrap();
    assert!(matches!(
        rx.recv().await,
        Some(WsMessage::Pong { timestamp: 99 })
    ));

    handle_text_message(r#"{"type":"get_status"}"#, &tx, &mut ctx, &state)
        .await
        .unwrap();
    let Some(WsMessage::Status {
        connected,
        authenticated,
        session_id,
        ..
    }) = rx.recv().await
    else {
        panic!("expected status");
    };
    // Status reflects the actual context: no session joined, not authenticated.
    assert!(connected);
    assert!(!authenticated);
    assert!(session_id.is_none());
}

#[tokio::test]
async fn malformed_and_out_of_order_frames_are_rejected_with_reasons() {
    let storage = tempfile::tempdir().unwrap();
    let state = open_state(&storage).await;
    let mut ctx = context(None);
    let (tx, _rx) = mpsc::channel(8);

    for frame in ["not json", r#"{"type":"no_such_frame"}"#, "{}"] {
        assert!(
            matches!(
                handle_text_message(frame, &tx, &mut ctx, &state).await,
                Err(AppError::Validation(_))
            ),
            "{frame}"
        );
    }
    // Session controls require a joined session; they do not silently no-op.
    for frame in [
        r#"{"type":"send_message","content":"hi"}"#,
        r#"{"type":"cancel"}"#,
        r#"{"type":"approve_exec","call_id":"a","approved":true}"#,
        r#"{"type":"fork_session","message_index":0}"#,
    ] {
        assert!(
            matches!(
                handle_text_message(frame, &tx, &mut ctx, &state).await,
                Err(AppError::BadRequest(_))
            ),
            "{frame}"
        );
    }
}

#[tokio::test]
async fn joining_a_session_subscribes_and_leaving_detaches_the_subscription() {
    let storage = tempfile::tempdir().unwrap();
    let state = open_state(&storage).await;
    let mut ctx = context(None);
    let (tx, mut rx) = mpsc::channel(16);

    handle_text_message(r#"{"type":"create_session"}"#, &tx, &mut ctx, &state)
        .await
        .unwrap();
    let Some(WsMessage::JoinedSession { session_id }) = rx.recv().await else {
        panic!("expected joined_session");
    };
    assert_eq!(ctx.session_id.as_deref(), Some(session_id.as_str()));
    assert!(ctx.subscription.is_some());

    // A real engine event reaches the socket through the live subscription.
    state
        .cli_sessions
        .submit_message(&session_id, "unreachable coding service".into())
        .await
        .unwrap();
    let mut saw_error = false;
    while let Ok(Some(message)) = tokio::time::timeout(Duration::from_secs(30), rx.recv()).await {
        if let WsMessage::SessionEvent {
            session_id: id,
            event,
            ..
        } = message
        {
            assert_eq!(id, session_id);
            if let WsMessage::Error { message, .. } = *event {
                // The failure is product-facing, not raw transport detail.
                assert_eq!(message, "The coding service is temporarily unavailable");
                saw_error = true;
                break;
            }
        }
    }
    assert!(saw_error, "the failed turn must reach the socket");

    handle_text_message(r#"{"type":"leave_session"}"#, &tx, &mut ctx, &state)
        .await
        .unwrap();
    assert!(ctx.session_id.is_none());
    assert!(ctx.subscription.is_none());
    // After leaving, controls are refused again.
    assert!(matches!(
        handle_text_message(r#"{"type":"cancel"}"#, &tx, &mut ctx, &state).await,
        Err(AppError::BadRequest(_))
    ));
    state.cli_sessions.shutdown_all().await;
}

#[tokio::test]
async fn a_socket_cannot_join_or_destroy_another_principals_session() {
    let storage = tempfile::tempdir().unwrap();
    let state = open_state(&storage).await;
    let info = state
        .cli_sessions
        .create_detached(crate::session_manager::CreateSessionOptions {
            user_id: Some("jwt:owner".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut ctx = context(Some("jwt:intruder"));
    let (tx, _rx) = mpsc::channel(8);

    for frame in [
        serde_json::json!({"type":"join_session","session_id":info.id}),
        serde_json::json!({"type":"destroy_session","session_id":info.id}),
    ] {
        assert!(
            matches!(
                handle_text_message(&frame.to_string(), &tx, &mut ctx, &state).await,
                Err(AppError::NotFound(_))
            ),
            "{frame}"
        );
    }
    // The session survives the refused attempts.
    assert!(state.cli_sessions.get_session(&info.id).await.is_some());
    assert!(ctx.session_id.is_none());
    state.cli_sessions.shutdown_all().await;
}

#[tokio::test]
async fn unsupported_session_controls_are_refused_rather_than_faked() {
    let storage = tempfile::tempdir().unwrap();
    let state = open_state(&storage).await;
    let mut ctx = context(None);
    let (tx, mut rx) = mpsc::channel(16);
    handle_text_message(r#"{"type":"create_session"}"#, &tx, &mut ctx, &state)
        .await
        .unwrap();
    let _ = rx.recv().await;

    // Changing models mid-session and the design-system service are not
    // implemented; they must report that, not silently succeed.
    for frame in [
        r#"{"type":"update_model","model":"other"}"#,
        r#"{"type":"design_system_response","call_id":"a","config":{}}"#,
    ] {
        assert!(
            matches!(
                handle_text_message(frame, &tx, &mut ctx, &state).await,
                Err(AppError::NotImplemented(_))
            ),
            "{frame}"
        );
    }
    // Only user turns are accepted on this transport.
    assert!(matches!(
        handle_text_message(
            r#"{"type":"send_message","content":"hi","role":"assistant"}"#,
            &tx,
            &mut ctx,
            &state
        )
        .await,
        Err(AppError::Validation(_))
    ));
    // Nothing may be cancelled while no turn is running.
    assert!(matches!(
        handle_text_message(r#"{"type":"cancel"}"#, &tx, &mut ctx, &state).await,
        Err(AppError::Conflict(_))
    ));
    state.cli_sessions.shutdown_all().await;
}

#[tokio::test]
async fn a_submitted_turn_is_acknowledged_as_queued_not_as_completed() {
    let storage = tempfile::tempdir().unwrap();
    let state = open_state(&storage).await;
    let mut ctx = context(None);
    let (tx, mut rx) = mpsc::channel(16);
    handle_text_message(r#"{"type":"create_session"}"#, &tx, &mut ctx, &state)
        .await
        .unwrap();
    let Some(WsMessage::JoinedSession { session_id }) = rx.recv().await else {
        panic!("expected joined_session");
    };

    handle_text_message(
        r#"{"type":"send_message","content":"unreachable coding service"}"#,
        &tx,
        &mut ctx,
        &state,
    )
    .await
    .unwrap();
    // Acceptance names the turn; it is explicitly not a completed turn.
    let mut turn = None;
    while let Some(message) = rx.recv().await {
        if let WsMessage::TurnAccepted {
            session_id: id,
            turn_id,
        } = message
        {
            assert_eq!(id, session_id);
            turn = Some(turn_id);
            break;
        }
    }
    let turn = turn.expect("expected turn_accepted");
    assert!(!turn.is_empty());

    // A second turn cannot start while the first is in flight.
    assert!(matches!(
        handle_text_message(
            r#"{"type":"send_message","content":"concurrent"}"#,
            &tx,
            &mut ctx,
            &state
        )
        .await,
        Err(AppError::Conflict(_))
    ));
    // An approval that was never requested is refused.
    assert!(matches!(
        handle_text_message(
            r#"{"type":"approve_exec","call_id":"never-requested","approved":true}"#,
            &tx,
            &mut ctx,
            &state
        )
        .await,
        Err(AppError::Conflict(_))
    ));
    state.cli_sessions.shutdown_all().await;
}

#[tokio::test]
async fn destroying_a_joined_session_detaches_the_socket() {
    let storage = tempfile::tempdir().unwrap();
    let state = open_state(&storage).await;
    let mut ctx = context(None);
    let (tx, mut rx) = mpsc::channel(16);
    handle_text_message(r#"{"type":"create_session"}"#, &tx, &mut ctx, &state)
        .await
        .unwrap();
    let Some(WsMessage::JoinedSession { session_id }) = rx.recv().await else {
        panic!("expected joined_session");
    };

    handle_text_message(
        &serde_json::json!({"type":"destroy_session","session_id":session_id}).to_string(),
        &tx,
        &mut ctx,
        &state,
    )
    .await
    .unwrap();
    // The registry and the connection both release the session.
    assert!(state.cli_sessions.get_session(&session_id).await.is_none());
    assert!(ctx.session_id.is_none());
    assert!(ctx.subscription.is_none());
    // Rejoining a destroyed session is not found.
    assert!(matches!(
        handle_text_message(
            &serde_json::json!({"type":"join_session","session_id":session_id}).to_string(),
            &tx,
            &mut ctx,
            &state
        )
        .await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn a_closed_socket_ends_its_subscription_without_wedging_the_registry() {
    let storage = tempfile::tempdir().unwrap();
    let state = open_state(&storage).await;
    let info = state
        .cli_sessions
        .create_detached(crate::session_manager::CreateSessionOptions::default())
        .await
        .unwrap();
    let events = state.cli_sessions.subscribe(&info.id).await.unwrap();
    let (tx, rx) = mpsc::channel(4);
    let task = crate::session_manager::spawn_ws_subscription(events, tx);

    // Dropping the receiver is a closed socket; the forwarder must finish.
    drop(rx);
    state
        .cli_sessions
        .submit_message(&info.id, "unreachable coding service".into())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("subscription must not outlive its socket")
        .unwrap();

    // The registry is still healthy: the session is usable by a new subscriber.
    let mut fresh = state.cli_sessions.subscribe(&info.id).await.unwrap();
    state.cli_sessions.destroy_session(&info.id).await.unwrap();
    assert!(matches!(
        fresh.recv().await.unwrap().event.msg,
        cortex_protocol::EventMsg::ShutdownComplete
    ));
    assert_eq!(state.cli_sessions.count().await, 0);
}

#[test]
fn test_client_message_parsing() {
    let json = r#"{"type": "ping", "timestamp": 1234567890}"#;
    let msg: WsClientMessage = serde_json::from_str(json).unwrap();
    assert!(matches!(
        msg,
        WsClientMessage::Ping {
            timestamp: 1234567890
        }
    ));
}

#[test]
fn test_server_message_serialization() {
    let msg = WsMessage::Pong {
        timestamp: 1234567890,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("pong"));
    assert!(json.contains("1234567890"));
}

#[tokio::test]
async fn test_connection_manager() {
    let manager = ConnectionManager::new();
    let (tx, _rx) = mpsc::channel(100);

    let info = ConnectionInfo::new("test-id".to_string(), tx);
    manager.register("test-id", info).await;

    assert_eq!(manager.count().await, 1);

    let retrieved = manager.get("test-id").await;
    assert!(retrieved.is_some());

    manager.unregister("test-id").await;
    assert_eq!(manager.count().await, 0);
}

#[tokio::test]
async fn connections_are_indexed_by_session_and_deliver_broadcasts() {
    let manager = ConnectionManager::new();
    let mut receivers = Vec::new();
    for (id, session) in [
        ("conn-a", Some("session-1")),
        ("conn-b", Some("session-1")),
        ("conn-c", Some("session-2")),
        ("conn-d", None),
    ] {
        let (tx, rx) = mpsc::channel(4);
        let mut info = ConnectionInfo::new(id.into(), tx);
        info.session_id = session.map(Into::into);
        info.user_id = Some(format!("jwt:{id}"));
        manager.register(id, info).await;
        receivers.push((id, rx));
    }

    assert_eq!(manager.count().await, 4);
    let mut ids = manager.connection_ids().await;
    ids.sort();
    assert_eq!(ids, ["conn-a", "conn-b", "conn-c", "conn-d"]);

    // A session lookup returns exactly the connections joined to it.
    let mut joined: Vec<_> = manager
        .get_session_connections("session-1")
        .await
        .into_iter()
        .map(|c| c.id)
        .collect();
    joined.sort();
    assert_eq!(joined, ["conn-a", "conn-b"]);
    assert_eq!(manager.get_session_connections("session-2").await.len(), 1);
    assert!(manager.get_session_connections("missing").await.is_empty());

    // A broadcast reaches subscribers without going through any connection's
    // own channel, so per-connection senders stay empty.
    let mut broadcast = manager.broadcast_tx.subscribe();
    manager.broadcast(WsMessage::SessionClosed);
    assert!(matches!(
        broadcast.recv().await.unwrap(),
        WsMessage::SessionClosed
    ));
    for (id, rx) in &mut receivers {
        assert!(rx.try_recv().is_err(), "{id}");
    }

    // A direct send does reach that one connection.
    manager
        .get("conn-a")
        .await
        .unwrap()
        .send(WsMessage::TaskStarted)
        .await
        .unwrap();
    assert!(matches!(
        receivers[0].1.recv().await,
        Some(WsMessage::TaskStarted)
    ));

    manager.unregister("conn-a").await;
    assert_eq!(manager.get_session_connections("session-1").await.len(), 1);
    assert!(manager.get("conn-a").await.is_none());
}

#[tokio::test]
async fn a_send_to_a_dropped_connection_reports_failure() {
    let (tx, rx) = mpsc::channel(1);
    let info = ConnectionInfo::new("gone".into(), tx);
    drop(rx);
    // A closed socket must surface as an error, never as a silent success.
    assert!(info.send(WsMessage::TaskStarted).await.is_err());
}

#[test]
fn heartbeat_defaults_keep_the_timeout_below_the_interval() {
    let config = HeartbeatConfig::default();
    assert_eq!(config.interval, Duration::from_secs(30));
    assert_eq!(config.timeout, Duration::from_secs(10));
    // A pong deadline longer than the ping period would never fire.
    assert!(config.timeout < config.interval);
}

#[tokio::test]
async fn a_cancel_frame_requests_cancellation_without_confirming_it() {
    let storage = tempfile::tempdir().unwrap();
    let state = open_state(&storage).await;
    let mut ctx = context(None);
    let (tx, mut rx) = mpsc::channel(32);
    handle_text_message(r#"{"type":"create_session"}"#, &tx, &mut ctx, &state)
        .await
        .unwrap();
    let Some(WsMessage::JoinedSession { session_id }) = rx.recv().await else {
        panic!("expected joined_session");
    };
    handle_text_message(
        r#"{"type":"send_message","content":"unreachable coding service"}"#,
        &tx,
        &mut ctx,
        &state,
    )
    .await
    .unwrap();

    handle_text_message(r#"{"type":"cancel"}"#, &tx, &mut ctx, &state)
        .await
        .unwrap();
    let mut requested = false;
    while let Ok(Some(message)) = tokio::time::timeout(Duration::from_secs(30), rx.recv()).await {
        if let WsMessage::CancelRequested { session_id: id } = message {
            assert_eq!(id, session_id);
            requested = true;
            break;
        }
    }
    // The frame is named "requested", not "cancelled": the engine has not
    // confirmed anything yet.
    assert!(requested, "cancel must be acknowledged as a request");
    state.cli_sessions.shutdown_all().await;
}

#[tokio::test]
async fn a_fork_frame_reports_the_engines_real_capability() {
    let storage = tempfile::tempdir().unwrap();
    let state = open_state(&storage).await;
    let mut ctx = context(None);
    let (tx, mut rx) = mpsc::channel(32);
    handle_text_message(r#"{"type":"create_session"}"#, &tx, &mut ctx, &state)
        .await
        .unwrap();
    let Some(WsMessage::JoinedSession { session_id }) = rx.recv().await else {
        panic!("expected joined_session");
    };

    match handle_text_message(
        r#"{"type":"fork_session","message_index":0}"#,
        &tx,
        &mut ctx,
        &state,
    )
    .await
    {
        // A successful fork moves the socket onto a genuinely new session.
        Ok(()) => {
            let Some(WsMessage::JoinedSession { session_id: forked }) = rx.recv().await else {
                panic!("expected joined_session for the fork");
            };
            assert_ne!(forked, session_id);
            assert_eq!(ctx.session_id.as_deref(), Some(forked.as_str()));
            assert_eq!(state.cli_sessions.count().await, 2);
        }
        // Or it refuses; either way no session is fabricated and the socket
        // stays attached to the original.
        Err(error) => {
            assert!(matches!(
                error,
                AppError::Conflict(_) | AppError::Unavailable(_)
            ));
            assert_eq!(ctx.session_id.as_deref(), Some(session_id.as_str()));
            assert_eq!(state.cli_sessions.count().await, 1);
        }
    }
    state.cli_sessions.shutdown_all().await;
}
