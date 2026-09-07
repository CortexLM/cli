//! Live session, SSE and turn-lifecycle coverage against the real loopback server.
mod common;
use common::*;

#[tokio::test]
async fn live_session_uses_the_engine_conversation_id_and_persists_its_record() {
    let f = fixture(ServerConfig::default()).await;
    let created = json_body(
        f.router
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/cli/sessions",
                Some(&f.key),
                Some(json!({"model":"local-fixture"})),
            ))
            .await
            .unwrap(),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_owned();
    // The live transport ID must be the engine conversation ID, not a stray UUID.
    assert_eq!(created["conversation_id"].as_str().unwrap(), id);
    assert!(uuid::Uuid::parse_str(&id).is_ok());
    assert_eq!(created["model"], "local-fixture");
    assert_eq!(created["status"], "ready");

    let listed = json_body(
        f.router
            .clone()
            .oneshot(request("GET", "/api/v1/cli/sessions", Some(&f.key), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["id"].as_str().unwrap(), id);

    // The same identity is readable through the persistent record.
    let stored = json_body(
        f.router
            .clone()
            .oneshot(request(
                "GET",
                &format!("/api/v1/stored-sessions/{id}"),
                Some(&f.key),
                None,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(stored["id"].as_str().unwrap(), id);
    assert_eq!(stored["model"], "local-fixture");

    // A live session's history cannot be deleted out from under it.
    let response = f
        .router
        .clone()
        .oneshot(request(
            "DELETE",
            &format!("/api/v1/stored-sessions/{id}"),
            Some(&f.key),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let response = f
        .router
        .clone()
        .oneshot(request(
            "DELETE",
            &format!("/api/v1/cli/sessions/{id}"),
            Some(&f.key),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(json_body(response).await["deleted"], true);
    assert_eq!(f.state.cli_sessions.count().await, 0);

    // With the live session gone the record deletes, and stays gone.
    let response = f
        .router
        .clone()
        .oneshot(request(
            "DELETE",
            &format!("/api/v1/stored-sessions/{id}"),
            Some(&f.key),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(json_body(response).await["deleted"], true);
    assert_eq!(
        f.router
            .oneshot(request(
                "GET",
                &format!("/api/v1/stored-sessions/{id}"),
                Some(&f.key),
                None
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_failed_engine_turn_reports_the_product_error_and_does_not_wedge_the_session() {
    let f = fixture(ServerConfig::default()).await;
    let info = f
        .state
        .cli_sessions
        .create_detached(CreateSessionOptions::default())
        .await
        .unwrap();

    let mut events = f.state.cli_sessions.subscribe(&info.id).await.unwrap();
    let turn = f
        .state
        .cli_sessions
        .submit_message(&info.id, "unreachable coding service".into())
        .await
        .unwrap();
    assert_eq!(
        f.state
            .cli_sessions
            .get_session(&info.id)
            .await
            .unwrap()
            .status,
        "processing"
    );
    // A second turn cannot start while one is in flight.
    assert!(
        f.state
            .cli_sessions
            .submit_message(&info.id, "concurrent".into())
            .await
            .is_err()
    );

    let seen = drain_turn(&mut events).await;
    let names: Vec<_> = seen.iter().map(|(n, _)| n.as_str()).collect();
    // The turn really failed against an unreachable service; it is not a fake success.
    assert!(names.contains(&"error"), "{names:?}");
    assert!(!names.contains(&"task_complete"), "{names:?}");
    // Every turn-scoped event carries the submitted turn ID.
    for (name, turn_id) in &seen {
        if name != "session_configured" {
            assert_eq!(turn_id.as_deref(), Some(turn.as_str()), "{name}");
        }
    }

    // The failed turn must release the session, not leave it stuck in `processing`.
    assert_eq!(
        f.state
            .cli_sessions
            .get_session(&info.id)
            .await
            .unwrap()
            .status,
        "ready"
    );
    let next = f
        .state
        .cli_sessions
        .submit_message(&info.id, "after failure".into())
        .await
        .unwrap();
    assert_ne!(next, turn);
    f.state
        .cli_sessions
        .destroy_session(&info.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn two_independent_sse_subscribers_see_the_same_failed_turn() {
    let f = fixture(ServerConfig::default()).await;
    let info = f.create_owned_session().await;
    let path = format!("/api/v1/cli/sessions/{}/events", info.id);

    // Two fully independent HTTP subscriptions, opened before any turn.
    let (one, two) = tokio::join!(
        f.router
            .clone()
            .oneshot(request("GET", &path, Some(&f.key), None)),
        f.router
            .clone()
            .oneshot(request("GET", &path, Some(&f.key), None))
    );
    let (one, two) = (one.unwrap(), two.unwrap());
    assert_eq!(one.status(), StatusCode::OK);
    assert_eq!(two.status(), StatusCode::OK);

    let turn = f
        .state
        .cli_sessions
        .submit_message(&info.id, "unreachable coding service".into())
        .await
        .unwrap();

    async fn collect(response: axum::response::Response) -> String {
        let mut body = response.into_body().into_data_stream();
        let mut text = String::new();
        while let Ok(Some(Ok(chunk))) =
            tokio::time::timeout(Duration::from_secs(30), body.next()).await
        {
            text.push_str(&String::from_utf8_lossy(&chunk));
            if text.contains("\"error\"") {
                break;
            }
        }
        text
    }
    let (one, two) = tokio::join!(collect(one), collect(two));

    for (label, text) in [("first", &one), ("second", &two)] {
        // Both subscribers independently observe the real failure and the turn identity.
        assert!(text.contains(r#""type":"error""#), "{label}: {text}");
        assert!(
            text.contains("The coding service is temporarily unavailable"),
            "{label}: {text}"
        );
        assert!(text.contains(&turn), "{label}: {text}");
        assert!(text.contains(&info.id), "{label}: {text}");
        // No fabricated completion is reported for a turn that failed.
        assert!(
            !text.contains(r#""type":"task_complete""#),
            "{label}: {text}"
        );
        // Raw transport/provider detail must never reach a subscriber.
        assert!(!text.contains("127.0.0.1:1"), "{label}: {text}");
    }
    assert_eq!(
        one.matches(r#""type":"error""#).count(),
        two.matches(r#""type":"error""#).count()
    );
    f.state
        .cli_sessions
        .destroy_session(&info.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn sse_chat_rejects_replay_and_scopes_the_stream_to_its_own_turn() {
    let f = fixture(ServerConfig::default()).await;
    let info = f.create_owned_session().await;

    // Replay is refused outright rather than silently returning a partial stream.
    for path in ["events", "chat"] {
        let mut req = request(
            if path == "chat" { "POST" } else { "GET" },
            &format!("/api/v1/cli/sessions/{}/{path}", info.id),
            Some(&f.key),
            (path == "chat").then(|| json!({"content":"fixture"})),
        );
        req.headers_mut()
            .insert("last-event-id", "42".parse().unwrap());
        let response = f.router.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED, "{path}");
        assert_eq!(
            json_body(response).await["error"]["message"],
            "This operation is not available"
        );
    }

    // An empty prompt is refused before any turn is started.
    let response = f
        .router
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/v1/cli/sessions/{}/chat", info.id),
            Some(&f.key),
            Some(json!({"content":"   "})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        f.state
            .cli_sessions
            .get_session(&info.id)
            .await
            .unwrap()
            .status,
        "ready"
    );

    // A real chat stream terminates on the turn's own failure.
    let response = f
        .router
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/v1/cli/sessions/{}/chat", info.id),
            Some(&f.key),
            Some(json!({"content":"unreachable coding service"})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let text = String::from_utf8(
        to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(text.contains(r#""type":"error""#), "{text}");
    assert!(
        text.contains("The coding service is temporarily unavailable"),
        "{text}"
    );
    // The stream closes on its own turn's terminal event rather than hanging,
    // and every frame it emitted belongs to that turn.
    let turn = text
        .split(r#""turn_id":""#)
        .skip(1)
        .map(|s| s.split('"').next().unwrap().to_owned())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(turn.len(), 1, "{text}");
    assert!(text.contains(&info.id), "{text}");
    // A failed turn is never reported as a completed one.
    assert!(!text.contains(r#""type":"task_complete""#), "{text}");
    // Raw transport detail must never reach a subscriber.
    assert!(!text.contains("127.0.0.1:1"), "{text}");
    f.state
        .cli_sessions
        .destroy_session(&info.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn interrupt_and_approval_controls_refuse_states_they_cannot_honour() {
    let f = fixture(ServerConfig::default()).await;
    let info = f.create_owned_session().await;
    let base = format!("/api/v1/cli/sessions/{}", info.id);

    // Nothing is running, so there is nothing to interrupt.
    assert_eq!(
        f.router
            .clone()
            .oneshot(request(
                "POST",
                &format!("{base}/interrupt"),
                Some(&f.key),
                None
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    // No approval was ever requested, so approving a call ID is refused.
    assert_eq!(
        f.router
            .clone()
            .oneshot(request(
                "POST",
                &format!("{base}/approve"),
                Some(&f.key),
                Some(json!({"call_id":"never-requested","approved":true})),
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );

    let mut events = f.state.cli_sessions.subscribe(&info.id).await.unwrap();
    f.state
        .cli_sessions
        .submit_message(&info.id, "unreachable coding service".into())
        .await
        .unwrap();
    // With a turn in flight the interrupt is accepted — as a request, not as
    // a fabricated confirmation of backend cancellation.
    let response = f
        .router
        .clone()
        .oneshot(request(
            "POST",
            &format!("{base}/interrupt"),
            Some(&f.key),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await, json!({"accepted":true}));

    let names: Vec<_> = drain_turn(&mut events)
        .await
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert!(
        names.iter().any(|n| n == "error" || n == "turn_aborted"),
        "{names:?}"
    );
    f.state
        .cli_sessions
        .destroy_session(&info.id)
        .await
        .unwrap();
}
