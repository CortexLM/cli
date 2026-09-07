//! Authenticated live-session REST controls and independent SSE subscriptions.
use crate::auth::{AuthResult, session_principal};
use crate::error::{AppError, AppResult};
pub use crate::session_manager::SessionManager as CliSessionManager;
use crate::session_manager::{CreateSessionOptions, SessionEvent, SessionInfo};
use crate::state::AppState;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
};
use futures::stream::Stream;
use std::{convert::Infallible, sync::Arc};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
#[path = "streaming_types.rs"]
mod types;
pub use types::*;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/cli/sessions",
            post(create_cli_session).get(list_cli_sessions),
        )
        .route(
            "/cli/sessions/{id}",
            get(get_cli_session).delete(delete_cli_session),
        )
        .route("/cli/sessions/{id}/chat", post(chat_stream))
        .route("/cli/sessions/{id}/events", get(session_events_stream))
        .route("/cli/sessions/{id}/approve", post(approve_command))
        .route("/cli/sessions/{id}/interrupt", post(interrupt_session))
        .route("/cli/sessions/{id}/fork", post(fork_cli_session))
}
fn response(info: SessionInfo) -> CliSessionResponse {
    CliSessionResponse {
        id: info.id,
        conversation_id: info.conversation_id,
        model: info.model,
        cwd: info.cwd.to_string_lossy().into_owned(),
        status: info.status,
    }
}
async fn authorize(
    state: &AppState,
    auth: Option<Extension<AuthResult>>,
    id: &str,
) -> AppResult<()> {
    let principal = session_principal(auth.as_ref().map(|a| &a.0), state.config.auth.enabled)?;
    state.cli_sessions.authorize(id, principal.as_deref()).await
}
async fn create_cli_session(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
    Json(req): Json<CreateCliSessionRequest>,
) -> AppResult<Json<CliSessionResponse>> {
    let principal = session_principal(auth.as_ref().map(|a| &a.0), state.config.auth.enabled)?;
    let info = state
        .cli_sessions
        .create_detached(CreateSessionOptions {
            user_id: principal,
            model: req.model,
            cwd: req.cwd.map(Into::into),
            provider: req.provider,
            system_prompt: None,
        })
        .await?;
    Ok(Json(response(info)))
}
async fn list_cli_sessions(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
) -> AppResult<Json<Vec<CliSessionResponse>>> {
    let principal = session_principal(auth.as_ref().map(|a| &a.0), state.config.auth.enabled)?;
    Ok(Json(
        state
            .cli_sessions
            .list_owned(principal.as_deref())
            .await
            .into_iter()
            .map(response)
            .collect(),
    ))
}
async fn get_cli_session(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
    Path(id): Path<String>,
) -> AppResult<Json<CliSessionResponse>> {
    authorize(&state, auth, &id).await?;
    Ok(Json(response(
        state
            .cli_sessions
            .get_session(&id)
            .await
            .ok_or_else(|| AppError::NotFound("Session not found".into()))?,
    )))
}
async fn delete_cli_session(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    authorize(&state, auth, &id).await?;
    state.cli_sessions.destroy_session(&id).await?;
    Ok(Json(serde_json::json!({"deleted":true})))
}
async fn fork_cli_session(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
    Path(id): Path<String>,
    Json(req): Json<ForkCliSessionRequest>,
) -> AppResult<Json<CliSessionResponse>> {
    authorize(&state, auth, &id).await?;
    let (tx, _) = mpsc::channel(1);
    Ok(Json(response(
        state
            .cli_sessions
            .fork_session(tx, &id, req.message_index)
            .await?,
    )))
}
async fn chat_stream(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<ChatRequest>,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    reject_replay(&headers)?;
    authorize(&state, auth, &id).await?;
    // Subscribe before submitting so even immediate failure reaches this request.
    let events = state.cli_sessions.subscribe(&id).await?;
    let turn = state.cli_sessions.submit_message(&id, req.content).await?;
    Ok(event_stream(events, Some(turn)))
}
async fn session_events_stream(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    reject_replay(&headers)?;
    authorize(&state, auth, &id).await?;
    Ok(event_stream(state.cli_sessions.subscribe(&id).await?, None))
}
fn reject_replay(headers: &HeaderMap) -> AppResult<()> {
    if headers.contains_key("last-event-id") {
        return Err(AppError::NotImplemented(
            "Event replay is not supported".into(),
        ));
    }
    Ok(())
}
async fn approve_command(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
    Path(id): Path<String>,
    Json(req): Json<ApproveRequest>,
) -> AppResult<Json<serde_json::Value>> {
    authorize(&state, auth, &id).await?;
    state
        .cli_sessions
        .approve_exec(&id, req.call_id, req.approved)
        .await?;
    Ok(Json(
        serde_json::json!({"accepted":true,"approved":req.approved}),
    ))
}
async fn interrupt_session(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    authorize(&state, auth, &id).await?;
    state.cli_sessions.interrupt(&id).await?;
    // Queue acceptance is not backend cancellation confirmation.
    Ok(Json(serde_json::json!({"accepted":true})))
}
fn event_stream(
    mut events: broadcast::Receiver<SessionEvent>,
    turn: Option<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(async move {
        loop {
            let event =
                tokio::select! { result = events.recv() => result, _ = tx.closed() => break };
            match event {
                Ok(event) => {
                    if turn.is_some()
                        && event.turn_id != turn
                        && !matches!(event.event.msg, cortex_protocol::EventMsg::ShutdownComplete)
                    {
                        continue;
                    }
                    let Some(mut payload) = types::convert_cli_event(&event.event) else {
                        continue;
                    };
                    if let StreamEvent::SessionReady { session_id, .. } = &mut payload {
                        *session_id = event.session_id.clone();
                    }
                    let terminal = matches!(
                        payload,
                        StreamEvent::TaskComplete { .. }
                            | StreamEvent::Error { .. }
                            | StreamEvent::Cancelled
                    );
                    let closed = matches!(payload, StreamEvent::SessionClosed);
                    let data = serde_json::json!({"session_id":event.session_id,"turn_id":event.turn_id,"sequence":event.sequence,"event":payload});
                    if tx
                        .send(Ok(Event::default().event("message").data(data.to_string())))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    if closed || (turn.is_some() && terminal) {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let _ = tx.send(Ok(Event::default().event("error").data(r#"{"code":"event_gap","message":"Events were lost; replay is not supported"}"#))).await;
                    break;
                }
                Err(_) => break,
            }
        }
    });
    Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_requests_are_explicitly_unsupported() {
        let mut headers = HeaderMap::new();
        headers.insert("last-event-id", "42".parse().unwrap());
        assert!(matches!(
            reject_replay(&headers),
            Err(AppError::NotImplemented(_))
        ));
        assert!(reject_replay(&HeaderMap::new()).is_ok());
    }
    #[tokio::test]
    async fn two_sse_subscribers_receive_both_turns_and_same_identity() {
        use axum::{body::to_bytes, response::IntoResponse};
        use cortex_protocol::{Event as EngineEvent, EventMsg, TaskCompleteEvent};
        let (tx, _) = broadcast::channel(8);
        let one = event_stream(tx.subscribe(), None).into_response();
        let two = event_stream(tx.subscribe(), None).into_response();
        for (index, turn) in ["turn-1", "turn-2"].into_iter().enumerate() {
            tx.send(SessionEvent {
                session_id: "same-session".into(),
                turn_id: Some(turn.into()),
                sequence: index as u64 + 1,
                event: EngineEvent {
                    id: "engine-counter".into(),
                    msg: EventMsg::TaskComplete(TaskCompleteEvent {
                        last_agent_message: None,
                    }),
                },
            })
            .unwrap();
        }
        tx.send(SessionEvent {
            session_id: "same-session".into(),
            turn_id: None,
            sequence: 3,
            event: EngineEvent {
                id: String::new(),
                msg: EventMsg::ShutdownComplete,
            },
        })
        .unwrap();
        let (one, two) = tokio::join!(
            to_bytes(one.into_body(), 10000),
            to_bytes(two.into_body(), 10000)
        );
        let one = one.unwrap();
        assert_eq!(one, two.unwrap());
        let text = std::str::from_utf8(&one).unwrap();
        assert_eq!(text.matches("task_complete").count(), 2);
        assert_eq!(text.matches("same-session").count(), 3);
        assert!(text.contains("turn-1") && text.contains("turn-2"));
    }

    #[tokio::test]
    async fn lagged_sse_subscription_reports_gap_without_replay_claim() {
        use axum::{body::to_bytes, response::IntoResponse};
        use cortex_protocol::{Event as EngineEvent, EventMsg};
        let (tx, _) = broadcast::channel(1);
        let stream = event_stream(tx.subscribe(), None).into_response();
        for sequence in 1..=3 {
            tx.send(SessionEvent {
                session_id: "session".into(),
                turn_id: None,
                sequence,
                event: EngineEvent {
                    id: String::new(),
                    msg: EventMsg::ShutdownComplete,
                },
            })
            .unwrap();
        }
        let body = to_bytes(stream.into_body(), 10000).await.unwrap();
        assert!(std::str::from_utf8(&body).unwrap().contains("event_gap"));
    }
}
