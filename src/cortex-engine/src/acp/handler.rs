//! ACP v1 text-only session handling. One engine receiver owns each event queue.
use std::collections::HashMap;
use std::sync::{Arc, atomic::Ordering};

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, RwLock, broadcast, oneshot};

use crate::acp::protocol::{AcpError, AcpRequestId, AcpResponse};
use crate::acp::types::*;
use crate::config::Config;
use crate::session::{Session, SessionHandle};
use cortex_protocol::{EventMsg, Op, ReviewDecision, Submission, UserInput};

struct ActivePrompt {
    waiter: Option<oneshot::Sender<Result<StopReason>>>,
}

pub struct AcpSessionState {
    pub handle: SessionHandle,
    active: Arc<Mutex<Option<ActivePrompt>>>,
    runner: tokio::task::JoinHandle<()>,
    forwarder: tokio::task::JoinHandle<()>,
}

impl Drop for AcpSessionState {
    fn drop(&mut self) {
        self.handle.cancelled.store(true, Ordering::SeqCst);
        self.runner.abort();
        self.forwarder.abort();
    }
}

pub struct AcpHandler {
    sessions: RwLock<HashMap<String, AcpSessionState>>,
    config: Config,
    notification_tx: broadcast::Sender<AcpNotificationEvent>,
}

#[derive(Debug, Clone)]
pub struct AcpNotificationEvent {
    pub method: String,
    pub params: Value,
}

impl AcpHandler {
    pub fn new(config: Config) -> Self {
        let (notification_tx, _) = broadcast::channel(1024);
        Self {
            sessions: RwLock::new(HashMap::new()),
            config,
            notification_tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AcpNotificationEvent> {
        self.notification_tx.subscribe()
    }

    pub async fn shutdown(&self) {
        self.sessions.write().await.clear();
    }

    pub async fn handle_initialize(&self, params: InitializeRequest) -> Result<InitializeResponse> {
        if params.protocol_version != PROTOCOL_VERSION {
            bail!("Unsupported ACP version; this agent supports version 1");
        }
        Ok(InitializeResponse {
            protocol_version: PROTOCOL_VERSION,
            agent_capabilities: AgentCapabilities::default(),
            agent_info: AgentInfo {
                name: "Cortex".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            auth_methods: vec![],
        })
    }

    pub async fn handle_session_new(
        &self,
        params: NewSessionRequest,
    ) -> Result<NewSessionResponse> {
        if !params.mcp_servers.is_empty() {
            bail!("ACP-supplied MCP servers are not supported");
        }
        let cwd = std::path::PathBuf::from(params.cwd);
        if !cwd.is_absolute() || !cwd.is_dir() {
            bail!("Working directory must be an existing absolute directory");
        }
        let mut sessions = self.sessions.write().await;
        if sessions.len() >= 32 {
            bail!("ACP session limit reached");
        }
        let mut config = self.config.clone();
        config.cwd = cwd.canonicalize()?;
        let (mut session, handle) = Session::new(config)?;
        let id = handle.conversation_id.to_string();
        let runner = tokio::spawn(async move {
            let _ = session.run().await;
        });
        let active = Arc::new(Mutex::new(None));
        let forwarder = spawn_forwarder(
            id.clone(),
            handle.clone(),
            active.clone(),
            self.notification_tx.clone(),
        );
        sessions.insert(
            id.clone(),
            AcpSessionState {
                handle,
                active,
                runner,
                forwarder,
            },
        );
        Ok(NewSessionResponse {
            session_id: id,
            models: None,
            modes: None,
        })
    }

    pub async fn handle_session_prompt(&self, params: PromptRequest) -> Result<PromptResponse> {
        let receiver = self.begin_prompt(params).await?;
        let stop_reason = receiver
            .await
            .map_err(|_| anyhow!("Session closed before turn completion"))??;
        Ok(PromptResponse { stop_reason })
    }

    pub(super) async fn begin_prompt(
        &self,
        params: PromptRequest,
    ) -> Result<oneshot::Receiver<Result<StopReason>>> {
        let items = text_inputs(params.prompt)?;
        let sessions = self.sessions.read().await;
        let state = sessions
            .get(&params.session_id)
            .ok_or_else(|| anyhow!("Session not found"))?;
        let handle = state.handle.clone();
        let active = state.active.clone();
        let (tx, rx) = oneshot::channel();
        let mut guard = active.lock().await;
        if guard.is_some() {
            bail!("A prompt is already active in this session");
        }
        *guard = Some(ActivePrompt { waiter: Some(tx) });
        // Reserve the turn before queueing; cancellation cannot be lost between the two.
        let sent = handle.submission_tx.try_send(Submission {
            id: uuid::Uuid::new_v4().to_string(),
            op: Op::UserInput { items },
        });
        if sent.is_err() {
            *guard = None;
            bail!("Session is unavailable");
        }
        drop(guard);
        drop(sessions);
        Ok(rx)
    }

    pub async fn handle_session_cancel(&self, params: CancelRequest) -> Result<CancelResponse> {
        let sessions = self.sessions.read().await;
        let state = sessions
            .get(&params.session_id)
            .ok_or_else(|| anyhow!("Session not found"))?;
        if state.active.lock().await.is_none() {
            return Ok(CancelResponse { cancelled: false });
        }
        // Setting the existing public flag interrupts in-flight work; queueing alone cannot.
        state.handle.cancelled.store(true, Ordering::SeqCst);
        state
            .handle
            .submission_tx
            .try_send(Submission {
                id: uuid::Uuid::new_v4().to_string(),
                op: Op::Interrupt,
            })
            .map_err(|_| anyhow!("Session control queue unavailable"))?;
        Ok(CancelResponse { cancelled: true })
    }

    pub async fn process_request(
        &self,
        id: AcpRequestId,
        method: &str,
        params: Value,
    ) -> AcpResponse {
        // Unsupported methods deliberately remain absent rather than returning invented data.
        let result = match method {
            "initialize" => match serde_json::from_value(params) {
                Ok(p) => self
                    .handle_initialize(p)
                    .await
                    .and_then(|r| Ok(serde_json::to_value(r)?)),
                Err(_) => {
                    return AcpResponse::error(
                        id,
                        AcpError::invalid_params("Invalid initialize parameters"),
                    );
                }
            },
            "session/new" => match serde_json::from_value(params) {
                Ok(p) => self
                    .handle_session_new(p)
                    .await
                    .and_then(|r| Ok(serde_json::to_value(r)?)),
                Err(_) => {
                    return AcpResponse::error(
                        id,
                        AcpError::invalid_params("Invalid session parameters"),
                    );
                }
            },
            "session/prompt" => match serde_json::from_value(params) {
                Ok(p) => self
                    .handle_session_prompt(p)
                    .await
                    .and_then(|r| Ok(serde_json::to_value(r)?)),
                Err(_) => {
                    return AcpResponse::error(
                        id,
                        AcpError::invalid_params("Invalid prompt parameters"),
                    );
                }
            },
            "session/cancel" => match serde_json::from_value(params) {
                Ok(p) => self
                    .handle_session_cancel(p)
                    .await
                    .and_then(|r| Ok(serde_json::to_value(r)?)),
                Err(_) => {
                    return AcpResponse::error(
                        id,
                        AcpError::invalid_params("Invalid cancel parameters"),
                    );
                }
            },
            _ => return AcpResponse::error(id, AcpError::method_not_found(method)),
        };
        match result {
            Ok(value) => AcpResponse::success(id, value),
            Err(_) => AcpResponse::error(
                id,
                AcpError::internal("The requested ACP operation could not be completed"),
            ),
        }
    }
}

fn text_inputs(prompt: Vec<PromptContent>) -> Result<Vec<UserInput>> {
    if prompt.is_empty() {
        bail!("Prompt must contain text");
    }
    prompt
        .into_iter()
        .map(|p| match p {
            PromptContent::Text { text } => Ok(UserInput::Text { text }),
            _ => bail!("Only text prompts are supported"),
        })
        .collect()
}

fn spawn_forwarder(
    id: String,
    handle: SessionHandle,
    active: Arc<Mutex<Option<ActivePrompt>>>,
    notifications: broadcast::Sender<AcpNotificationEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Ok(event) = handle.event_rx.recv().await {
            let terminal = match &event.msg {
                EventMsg::TaskComplete(_) => Some(Ok(if handle.cancelled.load(Ordering::SeqCst) {
                    StopReason::Cancelled
                } else {
                    StopReason::EndTurn
                })),
                EventMsg::TurnAborted(_) => Some(Ok(StopReason::Cancelled)),
                EventMsg::Error(_) => Some(Err(anyhow!(
                    "The coding service is temporarily unavailable"
                ))),
                EventMsg::ShutdownComplete => Some(Err(anyhow!("Session closed"))),
                EventMsg::ExecApprovalRequest(req) => {
                    // No client permission round trip is advertised. Fail closed rather than hang or auto-approve.
                    let _ = handle.submission_tx.try_send(Submission {
                        id: uuid::Uuid::new_v4().to_string(),
                        op: Op::ExecApproval {
                            id: req.call_id.clone(),
                            decision: ReviewDecision::Denied,
                        },
                    });
                    None
                }
                _ => None,
            };
            let failed = matches!(event.msg, EventMsg::Error(_));
            if let Some(update) = event_to_notification(&id, event.msg) {
                let _ = notifications.send(update);
            }
            if let Some(outcome) = terminal {
                let mut current = active.lock().await;
                if let Some(prompt) = current.as_mut() {
                    if let Some(waiter) = prompt.waiter.take() {
                        let _ = waiter.send(outcome);
                    }
                }
                // Drain the terminal completion after an error before accepting a new turn.
                if !failed {
                    *current = None;
                }
            }
        }
        if let Some(prompt) = active.lock().await.take() {
            if let Some(waiter) = prompt.waiter {
                let _ = waiter.send(Err(anyhow!("Session closed")));
            }
        }
    })
}

/// Convert an event message to a notification.
fn event_to_notification(session_id: &str, msg: EventMsg) -> Option<AcpNotificationEvent> {
    let update = match msg {
        EventMsg::AgentMessageDelta(delta) => SessionUpdate::AgentMessageChunk {
            content: MessageContent::Text { text: delta.delta },
        },
        EventMsg::AgentReasoningDelta(delta) => SessionUpdate::AgentThoughtChunk {
            content: MessageContent::Text { text: delta.delta },
        },
        EventMsg::ExecCommandBegin(begin) => SessionUpdate::ToolCall {
            tool_call_id: begin.call_id,
            title: begin.command.join(" "),
            kind: ToolKind::Execute,
            status: ToolStatus::InProgress,
            locations: vec![],
            raw_input: Value::Null,
        },
        EventMsg::ExecCommandEnd(end) => SessionUpdate::ToolCallUpdate {
            tool_call_id: end.call_id,
            status: if end.exit_code == 0 {
                ToolStatus::Completed
            } else {
                ToolStatus::Failed
            },
            content: None,
            raw_output: None,
        },
        EventMsg::McpToolCallBegin(begin) => SessionUpdate::ToolCall {
            tool_call_id: begin.call_id,
            title: format!("{}:{}", begin.invocation.server, begin.invocation.tool),
            kind: ToolKind::Other,
            status: ToolStatus::InProgress,
            locations: vec![],
            raw_input: begin.invocation.arguments.unwrap_or(Value::Null),
        },
        EventMsg::McpToolCallEnd(end) => SessionUpdate::ToolCallUpdate {
            tool_call_id: end.call_id,
            status: if end.result.is_ok() {
                ToolStatus::Completed
            } else {
                ToolStatus::Failed
            },
            content: None,
            raw_output: Some(serde_json::to_value(&end.result).unwrap_or(Value::Null)),
        },
        _ => return None,
    };

    let notification = SessionNotification {
        session_id: session_id.to_string(),
        update,
    };

    Some(AcpNotificationEvent {
        method: "session/update".to_string(),
        params: serde_json::to_value(notification).unwrap_or(Value::Null),
    })
}

// Additional request/response types for extended protocol

/// Load session request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSessionRequest {
    pub session_id: String,
}

/// Load session response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSessionResponse {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<SessionModels>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modes: Option<SessionModes>,
}

/// List sessions request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsRequest {
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// List sessions response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsResponse {
    pub sessions: Vec<SessionListInfo>,
}

/// Session list info.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListInfo {
    pub session_id: String,
    pub title: Option<String>,
    pub cwd: String,
    pub created_at: String,
    pub message_count: usize,
}

/// Cancel request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelRequest {
    pub session_id: String,
}

/// Cancel response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelResponse {
    pub cancelled: bool,
}

/// Models list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsListResponse {
    pub models: Vec<ModelInfo>,
}

/// Agents list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsListResponse {
    pub agents: Vec<AgentInfo>,
}

#[cfg(test)]
impl AcpHandler {
    pub(super) async fn install_protocol_peer(
        &self,
    ) -> (
        String,
        async_channel::Receiver<Submission>,
        async_channel::Sender<cortex_protocol::Event>,
    ) {
        let (submission_tx, submissions) = async_channel::bounded(8);
        let (events, event_rx) = async_channel::bounded(8);
        let conversation_id = cortex_protocol::ConversationId::new();
        let id = conversation_id.to_string();
        let handle = SessionHandle {
            submission_tx,
            event_rx,
            conversation_id,
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let active = Arc::new(Mutex::new(None));
        let forwarder = spawn_forwarder(
            id.clone(),
            handle.clone(),
            active.clone(),
            self.notification_tx.clone(),
        );
        let runner = tokio::spawn(std::future::pending());
        self.sessions.write().await.insert(
            id.clone(),
            AcpSessionState {
                handle,
                active,
                runner,
                forwarder,
            },
        );
        (id, submissions, events)
    }
}
