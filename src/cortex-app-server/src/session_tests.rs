//! These are local protocol peers, not coding-service or inference success fixtures.
use super::*;
use cortex_protocol::{EventMsg, TaskCompleteEvent, TurnAbortReason, TurnAbortedEvent};

#[tokio::test]
async fn ownership_turn_identity_cancel_and_two_subscribers() {
    let root = tempfile::tempdir().unwrap();
    let storage = SessionStorage::new(root.path()).unwrap();
    let manager = SessionManager::with_storage(storage, 2);
    let (submission_tx, submissions) = async_channel::bounded(8);
    let (event_tx, event_rx) = async_channel::bounded(8);
    let conversation_id = ConversationId::new();
    let id = conversation_id.to_string();
    let handle = SessionHandle {
        submission_tx,
        event_rx,
        conversation_id,
        cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };
    let hub = Arc::new(EventHub::new(id.clone()));
    let forwarder = events::spawn_forwarder(
        handle.event_rx.clone(),
        hub.clone(),
        manager.storage.clone(),
    );
    let runner = tokio::spawn(std::future::pending());
    manager.sessions.write().await.insert(
        id.clone(),
        ManagedSession {
            info: SessionInfo {
                status: "ready".into(),
                id: id.clone(),
                conversation_id: id.clone(),
                model: "fixture".into(),
                cwd: root.path().into(),
            },
            user_id: Some("jwt:alice".into()),
            handle,
            hub,
            runner,
            forwarder,
        },
    );
    assert!(manager.authorize(&id, Some("jwt:alice")).await.is_ok());
    assert!(manager.authorize(&id, Some("jwt:bob")).await.is_err());
    assert!(manager.authorize(&id, None).await.is_err());
    assert!(
        manager
            .approve_exec(&id, "unrequested".into(), true)
            .await
            .is_err()
    );
    let (mut a, mut b) = (
        manager.subscribe(&id).await.unwrap(),
        manager.subscribe(&id).await.unwrap(),
    );
    let turn_id = manager
        .submit_message(&id, "local fixture".into())
        .await
        .unwrap();
    assert_eq!(submissions.recv().await.unwrap().id, turn_id);
    assert_eq!(manager.get_session(&id).await.unwrap().status, "processing");
    assert!(
        manager
            .submit_message(&id, "concurrent".into())
            .await
            .is_err()
    );
    manager.interrupt(&id).await.unwrap();
    assert!(matches!(
        submissions.recv().await.unwrap().op,
        Op::Interrupt
    ));
    assert!(a.try_recv().is_err()); // acceptance is not fabricated cancellation
    event_tx
        .send(Event {
            id: "engine-turn".into(),
            msg: EventMsg::TurnAborted(TurnAbortedEvent {
                reason: TurnAbortReason::Interrupted,
            }),
        })
        .await
        .unwrap();
    let (one, two) = (a.recv().await.unwrap(), b.recv().await.unwrap());
    assert_eq!(one.turn_id, Some(turn_id));
    assert_eq!(one.sequence, two.sequence);
    assert_eq!(one.session_id, id);
    let next = manager
        .submit_message(&id, "next fixture".into())
        .await
        .unwrap();
    assert_eq!(submissions.recv().await.unwrap().id, next);
    event_tx
        .send(Event {
            id: "engine-next".into(),
            msg: EventMsg::TaskComplete(TaskCompleteEvent {
                last_agent_message: None,
            }),
        })
        .await
        .unwrap();
    assert_eq!(a.recv().await.unwrap().turn_id, Some(next.clone()));
    assert_eq!(b.recv().await.unwrap().turn_id, Some(next));
    manager.destroy_session(&id).await.unwrap();
}

/// Insert a session backed by local channels rather than a live engine transport.
/// This is a protocol peer for registry behaviour; it never stands in for a turn.
async fn insert_peer(
    manager: &SessionManager,
    owner: Option<&str>,
    cwd: &std::path::Path,
) -> (String, async_channel::Sender<Event>, Peer) {
    let (submission_tx, submissions) = async_channel::bounded(8);
    let (event_tx, event_rx) = async_channel::bounded(8);
    let conversation_id = ConversationId::new();
    let id = conversation_id.to_string();
    let handle = SessionHandle {
        submission_tx,
        event_rx,
        conversation_id,
        cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };
    manager
        .storage
        .save_session(&StoredSession {
            id: id.clone(),
            model: "fixture".into(),
            cwd: cwd.to_string_lossy().into_owned(),
            created_at: 0,
            updated_at: 0,
            title: None,
            user_id: owner.map(Into::into),
        })
        .unwrap();
    let hub = Arc::new(EventHub::new(id.clone()));
    let forwarder = events::spawn_forwarder(
        handle.event_rx.clone(),
        hub.clone(),
        manager.storage.clone(),
    );
    manager.sessions.write().await.insert(
        id.clone(),
        ManagedSession {
            info: SessionInfo {
                status: "ready".into(),
                id: id.clone(),
                conversation_id: id.clone(),
                model: "fixture".into(),
                cwd: cwd.into(),
            },
            user_id: owner.map(Into::into),
            handle,
            hub,
            runner: tokio::spawn(std::future::pending()),
            forwarder,
        },
    );
    (id, event_tx, Peer { submissions })
}

/// Holds the submission receiver open so the session's queue stays writable.
struct Peer {
    #[allow(dead_code)]
    submissions: async_channel::Receiver<Submission>,
}

#[tokio::test]
async fn unsupported_session_controls_report_their_limits_instead_of_succeeding() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::with_storage(SessionStorage::new(root.path()).unwrap(), 4);
    let (id, _events, _peer) = insert_peer(&manager, None, root.path()).await;

    // Neither control is implemented; both must say so rather than no-op green.
    for result in [
        manager.update_model(&id, "another-model").await,
        manager
            .submit_design_system(&id, "call".into(), serde_json::json!({}))
            .await,
    ] {
        let Err(error) = result else {
            panic!("unsupported control must not report success");
        };
        assert!(matches!(error, SessionError::InvalidState(_)));
        // The limit surfaces as a conflict, not as a fake success or a 500.
        assert!(matches!(AppError::from(error), AppError::Conflict(_)));
    }
}

#[tokio::test]
async fn every_control_rejects_an_unknown_session_and_maps_to_not_found() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::with_storage(SessionStorage::new(root.path()).unwrap(), 4);
    let missing = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = mpsc::channel(1);

    let errors = vec![
        manager.submit_message(&missing, "hi".into()).await.err(),
        manager.send_message(&missing, "hi".into()).await.err(),
        manager.interrupt(&missing).await.err(),
        manager
            .approve_exec(&missing, "call".into(), true)
            .await
            .err(),
        manager.destroy_session(&missing).await.err(),
        manager.subscribe(&missing).await.err(),
        manager.fork_session(tx, &missing, 0).await.err(),
    ];
    for error in errors {
        let error = error.expect("an unknown session must not be operated on");
        assert!(matches!(error, SessionError::NotFound));
        assert!(matches!(AppError::from(error), AppError::NotFound(_)));
    }
    assert!(manager.get_session(&missing).await.is_none());
    assert!(manager.authorize(&missing, None).await.is_err());
}

#[tokio::test]
async fn prompts_that_cannot_be_sent_are_refused_before_a_turn_starts() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::with_storage(SessionStorage::new(root.path()).unwrap(), 4);
    let (id, _events, _peer) = insert_peer(&manager, None, root.path()).await;

    for content in ["", "   ", "\n\t "] {
        assert!(matches!(
            manager.submit_message(&id, content.into()).await,
            Err(SessionError::InvalidState(_))
        ));
    }
    // An oversized prompt is refused rather than truncated silently.
    assert!(matches!(
        manager
            .submit_message(&id, "x".repeat(1024 * 1024 + 1))
            .await,
        Err(SessionError::InvalidState(_))
    ));
    // None of the refusals started a turn.
    assert_eq!(manager.get_session(&id).await.unwrap().status, "ready");
}

#[tokio::test]
async fn listings_and_counts_are_scoped_to_the_owning_principal() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::with_storage(SessionStorage::new(root.path()).unwrap(), 4);
    let (alice, _a, _pa) = insert_peer(&manager, Some("jwt:alice"), root.path()).await;
    let (bob, _b, _pb) = insert_peer(&manager, Some("jwt:bob"), root.path()).await;
    let (anon, _c, _pc) = insert_peer(&manager, None, root.path()).await;

    assert_eq!(manager.count().await, 3);
    assert_eq!(manager.list_sessions().await.len(), 3);
    for (owner, expected) in [
        (Some("jwt:alice"), vec![alice.clone()]),
        (Some("jwt:bob"), vec![bob.clone()]),
        (None, vec![anon.clone()]),
        (Some("jwt:nobody"), vec![]),
    ] {
        let ids: Vec<_> = manager
            .list_owned(owner)
            .await
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, expected, "{owner:?}");
    }
    // Ownership is exact: neither principal can reach the other's session.
    assert!(manager.authorize(&alice, Some("jwt:alice")).await.is_ok());
    assert!(manager.authorize(&alice, Some("jwt:bob")).await.is_err());
    assert!(manager.authorize(&anon, None).await.is_ok());
    assert!(manager.authorize(&anon, Some("jwt:alice")).await.is_err());

    manager.shutdown_all().await;
    assert_eq!(manager.count().await, 0);
    assert!(manager.list_owned(Some("jwt:alice")).await.is_empty());
}

#[tokio::test]
async fn the_session_limit_is_enforced_for_creation_and_forking() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::with_storage(SessionStorage::new(root.path()).unwrap(), 1);
    let (id, _events, _peer) = insert_peer(&manager, None, root.path()).await;
    let (tx, _rx) = mpsc::channel(1);

    // The single slot is taken, so neither a new nor a forked session fits.
    assert!(matches!(
        manager
            .create_detached(CreateSessionOptions::default())
            .await,
        Err(SessionError::InvalidState(_))
    ));
    assert!(matches!(
        manager.fork_session(tx, &id, 0).await,
        Err(SessionError::InvalidState(_))
    ));
    assert_eq!(manager.count().await, 1);
}

#[tokio::test]
async fn a_busy_session_cannot_be_forked() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::with_storage(SessionStorage::new(root.path()).unwrap(), 4);
    let (id, _events, _peer) = insert_peer(&manager, None, root.path()).await;
    let (tx, _rx) = mpsc::channel(1);

    manager
        .submit_message(&id, "in flight".into())
        .await
        .unwrap();
    assert_eq!(manager.get_session(&id).await.unwrap().status, "processing");
    // Forking mid-turn would capture an incomplete transcript, so it is refused.
    assert!(matches!(
        manager.fork_session(tx, &id, 0).await,
        Err(SessionError::InvalidState(_))
    ));
    assert_eq!(manager.count().await, 1);
}

#[tokio::test]
async fn unsupported_creation_options_and_bad_working_directories_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    for options in [
        // A second model provider is not a supported option.
        CreateSessionOptions {
            provider: Some("another-vendor".into()),
            ..Default::default()
        },
        CreateSessionOptions {
            system_prompt: Some("override".into()),
            ..Default::default()
        },
        // A working directory outside the server workspace is refused.
        CreateSessionOptions {
            cwd: Some(outside.path().into()),
            ..Default::default()
        },
        CreateSessionOptions {
            cwd: Some("../../..".into()),
            ..Default::default()
        },
        // A working directory that does not exist is refused.
        CreateSessionOptions {
            cwd: Some("definitely-missing-fixture-dir".into()),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            build_config(&options),
            Err(SessionError::InvalidState(_))
        ));
    }
    // The registry stayed empty; no partial session was left behind.
    let manager = SessionManager::with_storage(SessionStorage::new(root.path()).unwrap(), 4);
    assert_eq!(manager.count().await, 0);
}

#[tokio::test]
async fn a_destroyed_session_closes_its_subscribers_exactly_once() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::with_storage(SessionStorage::new(root.path()).unwrap(), 4);
    let (id, _events, _peer) = insert_peer(&manager, None, root.path()).await;
    let (mut one, mut two) = (
        manager.subscribe(&id).await.unwrap(),
        manager.subscribe(&id).await.unwrap(),
    );

    manager.destroy_session(&id).await.unwrap();
    // Both independent subscribers see the close, and then the stream ends.
    for subscriber in [&mut one, &mut two] {
        assert!(matches!(
            subscriber.recv().await.unwrap().event.msg,
            EventMsg::ShutdownComplete
        ));
        assert!(subscriber.recv().await.is_err());
    }
    // Destroying it again is not found; the registry did not keep a ghost entry.
    assert!(matches!(
        manager.destroy_session(&id).await,
        Err(SessionError::NotFound)
    ));
    assert_eq!(manager.count().await, 0);
}

#[tokio::test]
async fn an_engine_error_ends_the_turn_and_is_not_followed_by_a_success() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::with_storage(SessionStorage::new(root.path()).unwrap(), 4);
    let (id, events, _peer) = insert_peer(&manager, None, root.path()).await;
    let mut subscriber = manager.subscribe(&id).await.unwrap();

    let turn = manager.submit_message(&id, "fixture".into()).await.unwrap();
    events
        .send(Event {
            id: "engine-turn".into(),
            msg: EventMsg::Error(cortex_protocol::ErrorEvent {
                message: "fixture failure".into(),
                cortex_error_info: None,
            }),
        })
        .await
        .unwrap();
    let failure = subscriber.recv().await.unwrap();
    assert!(matches!(failure.event.msg, EventMsg::Error(_)));
    assert_eq!(failure.turn_id.as_deref(), Some(turn.as_str()));

    // A late TaskComplete must not turn a failed turn into a successful one.
    events
        .send(Event {
            id: "engine-turn".into(),
            msg: EventMsg::TaskComplete(TaskCompleteEvent {
                last_agent_message: None,
            }),
        })
        .await
        .unwrap();
    tokio::task::yield_now().await;
    assert!(subscriber.try_recv().is_err());

    // The failed turn released the session rather than wedging it.
    assert_eq!(manager.get_session(&id).await.unwrap().status, "ready");
    let next = manager.submit_message(&id, "after".into()).await.unwrap();
    assert_ne!(next, turn);
}

#[tokio::test]
async fn user_and_agent_messages_are_persisted_to_the_session_history() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::with_storage(SessionStorage::new(root.path()).unwrap(), 4);
    let (id, events, _peer) = insert_peer(&manager, None, root.path()).await;
    let mut subscriber = manager.subscribe(&id).await.unwrap();

    manager.submit_message(&id, "fixture".into()).await.unwrap();
    for msg in [
        EventMsg::UserMessage(cortex_protocol::UserMessageEvent {
            id: None,
            parent_id: None,
            message: "what the user asked".into(),
            images: None,
        }),
        EventMsg::AgentMessage(cortex_protocol::AgentMessageEvent {
            id: None,
            parent_id: None,
            message: "what the agent replied".into(),
            finish_reason: None,
        }),
        // A non-message event must not be written to the transcript.
        EventMsg::TaskStarted(cortex_protocol::TaskStartedEvent {
            model_context_window: None,
        }),
    ] {
        events
            .send(Event {
                id: "engine-turn".into(),
                msg,
            })
            .await
            .unwrap();
        subscriber.recv().await.unwrap();
    }

    let history = manager.storage.read_history(&id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].role, "user");
    assert_eq!(history[0].content, "what the user asked");
    assert_eq!(history[1].role, "assistant");
    assert_eq!(history[1].content, "what the agent replied");
    // The persisted record is the same session the events belong to.
    assert_eq!(manager.storage.load_session(&id).unwrap().id, id);
}

#[tokio::test]
async fn an_approval_can_only_be_answered_once_and_only_while_its_turn_runs() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::with_storage(SessionStorage::new(root.path()).unwrap(), 4);
    let (id, events, _peer) = insert_peer(&manager, None, root.path()).await;
    let mut subscriber = manager.subscribe(&id).await.unwrap();

    // No turn is running, so nothing may be approved.
    assert!(
        manager
            .approve_exec(&id, "call-1".into(), true)
            .await
            .is_err()
    );
    manager.submit_message(&id, "fixture".into()).await.unwrap();
    events
        .send(Event {
            id: "engine-turn".into(),
            msg: EventMsg::ExecApprovalRequest(cortex_protocol::ExecApprovalRequestEvent {
                call_id: "call-1".into(),
                turn_id: "engine-turn".into(),
                command: vec!["rm".into()],
                cwd: root.path().into(),
                sandbox_assessment: None,
            }),
        })
        .await
        .unwrap();
    assert!(matches!(
        subscriber.recv().await.unwrap().event.msg,
        EventMsg::ExecApprovalRequest(_)
    ));

    // An unrelated call ID is refused even while a real approval is pending.
    assert!(
        manager
            .approve_exec(&id, "other-call".into(), true)
            .await
            .is_err()
    );
    manager
        .approve_exec(&id, "call-1".into(), false)
        .await
        .unwrap();
    // The approval is consumed; it cannot be replayed to flip the decision.
    assert!(
        manager
            .approve_exec(&id, "call-1".into(), true)
            .await
            .is_err()
    );
}
