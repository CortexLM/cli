//! Authoritative in-memory engine session registry shared by REST control, SSE and WS.
use crate::error::{AppError, AppResult};
use crate::storage::{SessionStorage, StoredSession};
use crate::websocket::WsMessage;
use cortex_engine::{Config as CoreConfig, Session, SessionHandle};
use cortex_protocol::{
    AskForApproval, ConversationId, Event, Op, ReviewDecision, Submission, UserInput,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};
use tokio::sync::{RwLock, broadcast, mpsc};
use uuid::Uuid;
#[path = "session_events.rs"]
mod events;
#[path = "session_ws.rs"]
mod ws;
use events::EventHub;
pub use events::SessionEvent;

pub struct SessionManager {
    sessions: RwLock<HashMap<String, ManagedSession>>,
    storage: Arc<SessionStorage>,
    max_sessions: usize,
}
impl std::fmt::Debug for SessionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionManager")
            .field("max_sessions", &self.max_sessions)
            .finish_non_exhaustive()
    }
}
struct ManagedSession {
    info: SessionInfo,
    user_id: Option<String>,
    handle: SessionHandle,
    hub: Arc<EventHub>,
    runner: tokio::task::JoinHandle<()>,
    forwarder: tokio::task::JoinHandle<()>,
}
impl std::fmt::Debug for ManagedSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedSession")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}
impl Drop for ManagedSession {
    fn drop(&mut self) {
        self.handle.cancelled.store(true, Ordering::SeqCst);
        self.runner.abort();
        self.forwarder.abort();
    }
}
impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
impl SessionManager {
    pub fn new() -> Self {
        Self::with_storage(
            SessionStorage::default_location().expect("Session storage unavailable"),
            100,
        )
    }
    pub fn with_storage(storage: SessionStorage, max_sessions: usize) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            storage: Arc::new(storage),
            max_sessions,
        }
    }
    pub fn storage(&self) -> &Arc<SessionStorage> {
        &self.storage
    }
    pub async fn count(&self) -> usize {
        self.sessions.read().await.len()
    }
    pub async fn authorize(&self, id: &str, user_id: Option<&str>) -> AppResult<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(id)
            .ok_or_else(|| AppError::NotFound("Session not found".into()))?;
        if session.user_id.as_deref() != user_id {
            return Err(AppError::NotFound("Session not found".into()));
        }
        Ok(())
    }
    pub async fn create_session(
        &self,
        _sender: mpsc::Sender<WsMessage>,
        options: CreateSessionOptions,
    ) -> Result<SessionInfo, SessionError> {
        self.create_detached(options).await
    }
    pub async fn create_detached(
        &self,
        options: CreateSessionOptions,
    ) -> Result<SessionInfo, SessionError> {
        let mut sessions = self.sessions.write().await;
        if sessions.len() >= self.max_sessions {
            return Err(SessionError::InvalidState("Session limit reached".into()));
        }
        let config = build_config(&options)?;
        let (session, handle) = Session::new(config.clone()).map_err(|_| SessionError::Creation)?;
        let managed = self.manage(session, handle, config, options.user_id)?;
        let info = managed.info.clone();
        sessions.insert(info.id.clone(), managed);
        Ok(info)
    }
    fn manage(
        &self,
        mut session: Session,
        handle: SessionHandle,
        config: CoreConfig,
        user_id: Option<String>,
    ) -> Result<ManagedSession, SessionError> {
        // All live transports use the engine conversation ID, not an unrelated transport UUID.
        let id = handle.conversation_id.to_string();
        let info = SessionInfo {
            id: id.clone(),
            conversation_id: id.clone(),
            status: "ready".into(),
            model: config.model,
            cwd: config.cwd,
        };
        self.storage
            .save_session(&StoredSession {
                id: id.clone(),
                model: info.model.clone(),
                cwd: info.cwd.to_string_lossy().into_owned(),
                created_at: chrono::Utc::now().timestamp(),
                updated_at: chrono::Utc::now().timestamp(),
                title: None,
                user_id: user_id.clone(),
            })
            .map_err(|_| SessionError::Creation)?;
        let hub = Arc::new(EventHub::new(id));
        let forwarder =
            events::spawn_forwarder(handle.event_rx.clone(), hub.clone(), self.storage.clone());
        let runner = tokio::spawn(async move {
            let _ = session.run().await;
        });
        Ok(ManagedSession {
            info,
            user_id,
            handle,
            hub,
            runner,
            forwarder,
        })
    }
    pub async fn get_session(&self, id: &str) -> Option<SessionInfo> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(id)?;
        let mut info = session.info.clone();
        info.status = session.hub.status().await.into();
        Some(info)
    }
    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        let mut result = Vec::new();
        for session in sessions.values() {
            let mut info = session.info.clone();
            info.status = session.hub.status().await.into();
            result.push(info);
        }
        result
    }
    pub async fn list_owned(&self, owner: Option<&str>) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        let mut result = Vec::new();
        for session in sessions.values().filter(|s| s.user_id.as_deref() == owner) {
            let mut info = session.info.clone();
            info.status = session.hub.status().await.into();
            result.push(info);
        }
        result
    }
    pub async fn subscribe(
        &self,
        id: &str,
    ) -> Result<broadcast::Receiver<SessionEvent>, SessionError> {
        Ok(self
            .sessions
            .read()
            .await
            .get(id)
            .ok_or(SessionError::NotFound)?
            .hub
            .subscribe())
    }
    pub async fn submit_message(&self, id: &str, content: String) -> Result<String, SessionError> {
        if content.trim().is_empty() || content.len() > 1024 * 1024 {
            return Err(SessionError::InvalidState("Invalid prompt length".into()));
        }
        let sessions = self.sessions.read().await;
        let session = sessions.get(id).ok_or(SessionError::NotFound)?;
        let turn = Uuid::new_v4().to_string();
        session.hub.start_turn(&turn).await?;
        let result = session.handle.submission_tx.try_send(Submission {
            id: turn.clone(),
            op: Op::UserInput {
                items: vec![UserInput::Text {
                    text: content.clone(),
                }],
            },
        });
        if result.is_err() {
            session.hub.clear_turn().await;
            return Err(SessionError::Send);
        }
        Ok(turn)
    }
    pub async fn send_message(&self, id: &str, content: String) -> Result<(), SessionError> {
        self.submit_message(id, content).await.map(|_| ())
    }
    pub async fn interrupt(&self, id: &str) -> Result<(), SessionError> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(id).ok_or(SessionError::NotFound)?;
        session.hub.require_turn().await?;
        session.handle.cancelled.store(true, Ordering::SeqCst);
        session
            .handle
            .submission_tx
            .try_send(Submission {
                id: Uuid::new_v4().to_string(),
                op: Op::Interrupt,
            })
            .map_err(|_| SessionError::Send)
    }
    pub async fn approve_exec(
        &self,
        id: &str,
        call_id: String,
        approved: bool,
    ) -> Result<(), SessionError> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(id).ok_or(SessionError::NotFound)?;
        session.hub.take_approval(&call_id).await?;
        session
            .handle
            .submission_tx
            .try_send(Submission {
                id: Uuid::new_v4().to_string(),
                op: Op::ExecApproval {
                    id: call_id,
                    decision: if approved {
                        ReviewDecision::Approved
                    } else {
                        ReviewDecision::Denied
                    },
                },
            })
            .map_err(|_| SessionError::Send)
    }
    pub async fn destroy_session(&self, id: &str) -> Result<(), SessionError> {
        let session = self
            .sessions
            .write()
            .await
            .remove(id)
            .ok_or(SessionError::NotFound)?;
        session.hub.close().await;
        drop(session);
        Ok(())
    }
    pub async fn shutdown_all(&self) {
        let sessions = std::mem::take(&mut *self.sessions.write().await);
        for session in sessions.into_values() {
            session.hub.close().await;
        }
    }
    pub async fn update_model(&self, _id: &str, _model: &str) -> Result<(), SessionError> {
        Err(SessionError::InvalidState(
            "Changing models in an active server session is unsupported".into(),
        ))
    }
    pub async fn submit_design_system(
        &self,
        _id: &str,
        _call_id: String,
        _config: serde_json::Value,
    ) -> Result<(), SessionError> {
        Err(SessionError::InvalidState(
            "Design-system client service is unsupported".into(),
        ))
    }
    pub async fn fork_session(
        &self,
        _sender: mpsc::Sender<WsMessage>,
        id: &str,
        message_index: usize,
    ) -> Result<SessionInfo, SessionError> {
        let mut sessions = self.sessions.write().await;
        if sessions.len() >= self.max_sessions {
            return Err(SessionError::InvalidState("Session limit reached".into()));
        }
        let original = sessions.get(id).ok_or(SessionError::NotFound)?;
        original.hub.require_idle().await?;
        let config = build_config(&CreateSessionOptions {
            model: Some(original.info.model.clone()),
            cwd: Some(original.info.cwd.clone()),
            ..Default::default()
        })?;
        let conversation: ConversationId = original
            .info
            .conversation_id
            .parse()
            .map_err(|_| SessionError::Creation)?;
        let owner = original.user_id.clone();
        let (session, handle) = Session::fork(config.clone(), conversation, message_index)
            .map_err(|_| SessionError::Creation)?;
        let managed = self.manage(session, handle, config, owner)?;
        let info = managed.info.clone();
        sessions.insert(info.id.clone(), managed);
        Ok(info)
    }
}
fn build_config(options: &CreateSessionOptions) -> Result<CoreConfig, SessionError> {
    if options.provider.is_some() || options.system_prompt.is_some() {
        return Err(SessionError::InvalidState(
            "Unsupported session option".into(),
        ));
    }
    let mut config = CoreConfig::default();
    config.approval_policy = AskForApproval::OnRequest;
    if let Some(model) = &options.model {
        config.model = model.clone();
    }
    let cwd = options.cwd.as_deref().unwrap_or(std::path::Path::new("."));
    config.cwd = crate::api::path_security::validate_path_safe(cwd).map_err(|_| {
        SessionError::InvalidState("Working directory outside the server workspace".into())
    })?;
    if !config.cwd.is_dir() {
        return Err(SessionError::InvalidState(
            "Working directory does not exist".into(),
        ));
    }
    Ok(config)
}
#[derive(Debug, Clone, Default)]
pub struct CreateSessionOptions {
    pub user_id: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub cwd: Option<PathBuf>,
    pub system_prompt: Option<String>,
}
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub status: String,
    pub id: String,
    pub conversation_id: String,
    pub model: String,
    pub cwd: PathBuf,
}
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("The coding service is temporarily unavailable")]
    Creation,
    #[error("Session not found")]
    NotFound,
    #[error("Session is unavailable")]
    Send,
    #[error("{0}")]
    InvalidState(String),
}
impl From<SessionError> for AppError {
    fn from(error: SessionError) -> Self {
        match error {
            SessionError::NotFound => Self::NotFound("Session not found".into()),
            SessionError::InvalidState(message) => Self::Conflict(message),
            _ => Self::Unavailable("Session is unavailable".into()),
        }
    }
}

pub(crate) fn spawn_ws_subscription(
    mut rx: broadcast::Receiver<SessionEvent>,
    tx: mpsc::Sender<WsMessage>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let event = tokio::select! { result = rx.recv() => result, _ = tx.closed() => break };
            match event {
                Ok(event) => {
                    let msg = ws::convert_event_to_ws(&Event {
                        id: event.turn_id.clone().unwrap_or_default(),
                        msg: event.event.msg,
                    });
                    if let Some(message) = msg {
                        if tx
                            .send(WsMessage::SessionEvent {
                                session_id: event.session_id,
                                turn_id: event.turn_id,
                                sequence: event.sequence,
                                event: Box::new(message),
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let _ = tx
                        .send(WsMessage::Error {
                            code: "event_gap".into(),
                            message: "Events were lost; replay is not supported".into(),
                        })
                        .await;
                    break;
                }
                Err(_) => break,
            }
        }
    })
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
