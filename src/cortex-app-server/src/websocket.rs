//! WebSocket support for real-time communication.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    Extension, Router,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
    routing::get,
};
use futures::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::auth::{AuthResult, AuthService, session_principal};
use crate::error::AppError;
use crate::state::AppState;

/// Create WebSocket routes.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ws", get(websocket_handler))
        .route("/ws/sessions/{id}", get(session_websocket_handler))
}

/// WebSocket connection query parameters.
#[derive(Debug, Deserialize)]
pub struct WsConnectQuery {
    /// Legacy parameter, ignored for authentication. Use the Authorization header.
    pub token: Option<String>,
    /// Session ID to join.
    pub session_id: Option<String>,
}

/// Handle WebSocket upgrade.
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsConnectQuery>,
    auth: Option<Extension<AuthResult>>,
) -> Response {
    ws.max_message_size(state.config.max_body_size.min(1024 * 1024))
        .max_frame_size(1024 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, state, query, auth.map(|a| a.0)))
}

/// Handle session-specific WebSocket.
async fn session_websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    auth: Option<Extension<AuthResult>>,
) -> Response {
    let query = WsConnectQuery {
        token: None,
        session_id: Some(session_id),
    };
    ws.max_message_size(state.config.max_body_size.min(1024 * 1024))
        .max_frame_size(1024 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, state, query, auth.map(|a| a.0)))
}

/// Handle a WebSocket connection.
async fn handle_socket(
    socket: WebSocket,
    state: Arc<AppState>,
    query: WsConnectQuery,
    auth: Option<AuthResult>,
) {
    let connection_id = Uuid::new_v4().to_string();
    info!(connection_id = %connection_id, "WebSocket connected");

    let (sender, receiver) = socket.split();

    // Create connection context
    let mut ctx = ConnectionContext {
        _id: connection_id.clone(),
        user_id: session_principal(auth.as_ref(), state.config.auth.enabled)
            .ok()
            .flatten(),
        session_id: None,
        subscription: None,
        authenticated: auth.as_ref().is_some_and(AuthResult::is_authenticated),
        created_at: Instant::now(),
        last_ping: Instant::now(),
    };

    // Create channels for internal communication
    let (tx, rx) = mpsc::channel::<WsMessage>(100);

    // Spawn sender task
    let sender_task = tokio::spawn(async move {
        handle_sender(sender, rx).await;
    });

    if let Some(session_id) = query.session_id {
        let join = serde_json::json!({"type":"join_session","session_id":session_id}).to_string();
        if let Err(error) = handle_text_message(&join, &tx, &mut ctx, &state).await {
            let _ = tx
                .send(WsMessage::Error {
                    code: error.error_code().into(),
                    message: error.public_message().into(),
                })
                .await;
        }
    }
    // Handle incoming messages
    let _receiver_result = handle_receiver(receiver, tx.clone(), &mut ctx, &state).await;

    // Cleanup
    sender_task.abort();
    info!(connection_id = %connection_id, "WebSocket disconnected");
}

/// Handle outgoing messages.
async fn handle_sender(
    mut sender: SplitSink<WebSocket, Message>,
    mut rx: mpsc::Receiver<WsMessage>,
) {
    while let Some(msg) = rx.recv().await {
        let text = match serde_json::to_string(&msg) {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to serialize message: {}", e);
                continue;
            }
        };

        if let Err(e) = sender.send(Message::Text(text.into())).await {
            error!("Failed to send message: {}", e);
            break;
        }
    }
}

/// Handle incoming messages.
async fn handle_receiver(
    mut receiver: SplitStream<WebSocket>,
    tx: mpsc::Sender<WsMessage>,
    ctx: &mut ConnectionContext,
    state: &AppState,
) -> Result<(), AppError> {
    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                error!("WebSocket receive error: {}", e);
                break;
            }
        };

        match msg {
            Message::Text(text) => {
                if let Err(e) = handle_text_message(&text, &tx, ctx, state).await {
                    error!(code = e.error_code(), "Error handling message");
                    let _ = tx
                        .send(WsMessage::Error {
                            code: e.error_code().to_string(),
                            message: e.public_message().to_string(),
                        })
                        .await;
                }
            }
            Message::Binary(data) => {
                debug!("Received binary message: {} bytes", data.len());
            }
            Message::Ping(_data) => {
                ctx.last_ping = Instant::now();
                // Pong is handled automatically by axum
            }
            Message::Pong(_) => {
                ctx.last_ping = Instant::now();
            }
            Message::Close(_) => {
                info!("WebSocket close requested");
                break;
            }
        }
    }

    Ok(())
}

fn authorize_message(
    msg: &WsClientMessage,
    ctx: &mut ConnectionContext,
    auth_enabled: bool,
) -> Result<(), AppError> {
    if auth_enabled
        && !ctx.authenticated
        && !matches!(
            msg,
            WsClientMessage::Auth { .. } | WsClientMessage::Ping { .. }
        )
    {
        return Err(AppError::Authentication("Authentication required".into()));
    }
    Ok(())
}

/// Handle a text message.
async fn handle_text_message(
    text: &str,
    tx: &mpsc::Sender<WsMessage>,
    ctx: &mut ConnectionContext,
    state: &AppState,
) -> Result<(), AppError> {
    let msg: WsClientMessage = serde_json::from_str(text)
        .map_err(|e| AppError::Validation(format!("Invalid message format: {e}")))?;
    authorize_message(&msg, ctx, state.config.auth.enabled)?;

    match msg {
        WsClientMessage::Ping { timestamp } => {
            let _ = tx.send(WsMessage::Pong { timestamp }).await;
        }
        WsClientMessage::Auth { token } => {
            ctx.detach();
            let result = AuthService::new(state.config.auth.clone()).validate_token(&token);
            ctx.authenticated = result.is_ok();
            ctx.user_id = result.ok().map(|c| format!("jwt:{}", c.sub));
            let _ = tx
                .send(WsMessage::AuthResult {
                    success: ctx.authenticated,
                    user_id: ctx.user_id.clone(),
                })
                .await;
        }
        WsClientMessage::CreateSession { model, cwd } => {
            let options = crate::session_manager::CreateSessionOptions {
                user_id: ctx.user_id.clone(),
                model,
                cwd: cwd.map(Into::into),
                ..Default::default()
            };
            let info = state.cli_sessions.create_detached(options).await?;
            join_session(state, ctx, tx, &info.id).await?;
        }
        WsClientMessage::JoinSession { session_id } => {
            join_session(state, ctx, tx, &session_id).await?;
        }
        WsClientMessage::LeaveSession => {
            let session_id = ctx.session_id.clone();
            ctx.detach();
            let _ = tx.send(WsMessage::LeftSession { session_id }).await;
        }
        WsClientMessage::GetStatus => {
            let _ = tx
                .send(WsMessage::Status {
                    connected: true,
                    authenticated: ctx.authenticated,
                    session_id: ctx.session_id.clone(),
                    uptime_seconds: ctx.created_at.elapsed().as_secs(),
                })
                .await;
        }
        WsClientMessage::DestroySession { session_id } => {
            state
                .cli_sessions
                .authorize(&session_id, ctx.user_id.as_deref())
                .await?;
            state.cli_sessions.destroy_session(&session_id).await?;
            if ctx.session_id.as_deref() == Some(&session_id) {
                ctx.detach();
            }
            let _ = tx.send(WsMessage::SessionClosed).await;
        }
        other => {
            let id = ctx
                .session_id
                .clone()
                .ok_or_else(|| AppError::BadRequest("Join a session first".into()))?;
            state
                .cli_sessions
                .authorize(&id, ctx.user_id.as_deref())
                .await?;
            match other {
                WsClientMessage::SendMessage { content, role } => {
                    if role.as_deref().is_some_and(|r| r != "user") {
                        return Err(AppError::Validation("Only user turns are supported".into()));
                    }
                    let turn_id = state.cli_sessions.submit_message(&id, content).await?;
                    let _ = tx
                        .send(WsMessage::TurnAccepted {
                            session_id: id,
                            turn_id,
                        })
                        .await;
                }
                WsClientMessage::ApproveExec { call_id, approved } => {
                    state
                        .cli_sessions
                        .approve_exec(&id, call_id, approved)
                        .await?;
                }
                WsClientMessage::Cancel => {
                    state.cli_sessions.interrupt(&id).await?;
                    let _ = tx.send(WsMessage::CancelRequested { session_id: id }).await;
                }
                WsClientMessage::ForkSession { message_index } => {
                    let info = state
                        .cli_sessions
                        .fork_session(tx.clone(), &id, message_index)
                        .await?;
                    join_session(state, ctx, tx, &info.id).await?;
                }
                WsClientMessage::UpdateModel { .. }
                | WsClientMessage::DesignSystemResponse { .. } => {
                    return Err(AppError::NotImplemented(
                        "Unsupported session control".into(),
                    ));
                }
                _ => return Err(AppError::Validation("Invalid session control".into())),
            }
        }
    }
    Ok(())
}

async fn join_session(
    state: &AppState,
    ctx: &mut ConnectionContext,
    tx: &mpsc::Sender<WsMessage>,
    id: &str,
) -> Result<(), AppError> {
    state
        .cli_sessions
        .authorize(id, ctx.user_id.as_deref())
        .await?;
    let events = state.cli_sessions.subscribe(id).await?;
    ctx.detach();
    ctx.session_id = Some(id.into());
    let _ = tx
        .send(WsMessage::JoinedSession {
            session_id: id.into(),
        })
        .await;
    ctx.subscription = Some(crate::session_manager::spawn_ws_subscription(
        events,
        tx.clone(),
    ));
    Ok(())
}

/// Connection context.
#[derive(Debug)]
struct ConnectionContext {
    /// Connection ID.
    _id: String,
    /// User ID if authenticated.
    user_id: Option<String>,
    /// Session ID if joined.
    session_id: Option<String>,
    /// Current independent event subscription.
    subscription: Option<tokio::task::JoinHandle<()>>,
    /// Whether authenticated.
    authenticated: bool,
    /// Creation time.
    created_at: Instant,
    /// Last ping time.
    last_ping: Instant,
}

impl ConnectionContext {
    fn detach(&mut self) {
        self.session_id = None;
        if let Some(task) = self.subscription.take() {
            task.abort();
        }
    }
}
impl Drop for ConnectionContext {
    fn drop(&mut self) {
        self.detach();
    }
}

/// Client-to-server WebSocket messages.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMessage {
    /// Ping message.
    Ping { timestamp: u64 },
    /// Authentication.
    Auth { token: String },
    /// Create a new CLI session.
    CreateSession {
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
    },
    /// Join an existing session.
    JoinSession { session_id: String },
    /// Leave current session.
    LeaveSession,
    /// Send a message to the CLI session.
    SendMessage {
        content: String,
        #[serde(default)]
        role: Option<String>,
    },
    /// Approve a command execution.
    ApproveExec { call_id: String, approved: bool },
    /// Cancel/interrupt current operation.
    Cancel,
    /// Get connection status.
    GetStatus,
    /// Destroy/close a session.
    DestroySession { session_id: String },
    /// Update model for current session.
    UpdateModel { model: String },
    /// Submit design system selection (response to DesignSystemPending).
    DesignSystemResponse {
        call_id: String,
        config: serde_json::Value,
    },
    /// Fork current session.
    ForkSession { message_index: usize },
}

/// Server-to-client WebSocket messages.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    /// Version 1 live event envelope shared with SSE. No durable replay is supported.
    SessionEvent {
        session_id: String,
        turn_id: Option<String>,
        sequence: u64,
        event: Box<WsMessage>,
    },
    /// Accepted by the local engine queue, not a successful coding turn.
    TurnAccepted { session_id: String, turn_id: String },
    /// Cancellation requested, not yet confirmed by the engine.
    CancelRequested { session_id: String },
    /// Pong response.
    Pong { timestamp: u64 },
    /// Authentication result.
    AuthResult {
        success: bool,
        user_id: Option<String>,
    },
    /// Joined session confirmation.
    JoinedSession { session_id: String },
    /// Left session confirmation.
    LeftSession { session_id: Option<String> },
    /// Message received confirmation.
    MessageReceived {
        id: String,
        role: String,
        content: String,
    },
    /// Streaming response chunk (from CLI).
    StreamChunk { content: String },
    /// Full agent message (end of streaming).
    AgentMessage { content: String },
    /// Tool call started.
    ToolCallBegin {
        call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    /// Tool call completed.
    ToolCallEnd {
        call_id: String,
        tool_name: String,
        output: String,
        success: bool,
        duration_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    /// Tool call output chunk (streaming).
    ToolCallOutputDelta {
        call_id: String,
        stream: String, // "stdout" or "stderr"
        chunk: String,  // base64 encoded
    },
    /// Legacy tool call (kept for compatibility).
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    /// Tool result.
    ToolResult {
        id: String,
        output: String,
        success: bool,
    },
    /// Approval request from CLI.
    ApprovalRequest {
        call_id: String,
        command: Vec<String>,
        cwd: String,
    },
    /// Task started.
    TaskStarted,
    /// Task completed.
    TaskComplete { message: Option<String> },
    /// Token usage update.
    TokenUsage {
        input_tokens: u32,
        output_tokens: u32,
        total_tokens: u32,
    },
    /// Stream ended (legacy).
    StreamEnd { usage: TokenUsageInfo },
    /// Operation cancelled.
    Cancelled,
    /// Connection status.
    Status {
        connected: bool,
        authenticated: bool,
        session_id: Option<String>,
        uptime_seconds: u64,
    },
    /// Session configured by CLI.
    SessionConfigured {
        session_id: String,
        model: String,
        cwd: String,
    },
    /// Model updated confirmation.
    ModelUpdated { model: String },
    /// Reasoning/thinking delta.
    ReasoningDelta { delta: String },
    /// Warning message.
    Warning { message: String },
    /// Session closed.
    SessionClosed,
    /// Error message.
    Error { code: String, message: String },
    /// Terminal created.
    TerminalCreated {
        terminal_id: String,
        name: String,
        cwd: String,
    },
    /// Terminal output line.
    TerminalOutput {
        terminal_id: String,
        timestamp: u64,
        content: String,
        stream: String,
    },
    /// Terminal status changed.
    TerminalStatus {
        terminal_id: String,
        status: String,
        exit_code: Option<i32>,
    },
    /// Terminal list.
    TerminalList { terminals: Vec<serde_json::Value> },
    /// Design system selection pending - UI should show picker and wait for user.
    DesignSystemPending {
        call_id: String,
        project_type: String,
        fonts: serde_json::Value,
        palettes: serde_json::Value,
    },
    /// Design system selection received.
    DesignSystemReceived { call_id: String },
}

/// Token usage information.
#[derive(Debug, Clone, Serialize)]
pub struct TokenUsageInfo {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

/// WebSocket connection manager.
#[derive(Debug)]
pub struct ConnectionManager {
    /// Active connections by ID.
    connections: RwLock<HashMap<String, ConnectionInfo>>,
    /// Broadcast channel for server-wide messages.
    broadcast_tx: broadcast::Sender<WsMessage>,
}

impl ConnectionManager {
    /// Create a new connection manager.
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);
        Self {
            connections: RwLock::new(HashMap::new()),
            broadcast_tx,
        }
    }

    /// Register a new connection.
    pub async fn register(&self, id: &str, info: ConnectionInfo) {
        let mut connections = self.connections.write().await;
        connections.insert(id.to_string(), info);
        info!(connection_id = %id, "Connection registered");
    }

    /// Unregister a connection.
    pub async fn unregister(&self, id: &str) {
        let mut connections = self.connections.write().await;
        connections.remove(id);
        info!(connection_id = %id, "Connection unregistered");
    }

    /// Get connection info.
    pub async fn get(&self, id: &str) -> Option<ConnectionInfo> {
        let connections = self.connections.read().await;
        connections.get(id).cloned()
    }

    /// Get all connections for a session.
    pub async fn get_session_connections(&self, session_id: &str) -> Vec<ConnectionInfo> {
        let connections = self.connections.read().await;
        connections
            .values()
            .filter(|c| c.session_id.as_deref() == Some(session_id))
            .cloned()
            .collect()
    }

    /// Broadcast a message to all connections.
    pub fn broadcast(&self, message: WsMessage) {
        let _ = self.broadcast_tx.send(message);
    }

    /// Get connection count.
    pub async fn count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }

    /// Get all connection IDs.
    pub async fn connection_ids(&self) -> Vec<String> {
        let connections = self.connections.read().await;
        connections.keys().cloned().collect()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Connection information.
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// Connection ID.
    pub id: String,
    /// User ID if authenticated.
    pub user_id: Option<String>,
    /// Session ID if joined.
    pub session_id: Option<String>,
    /// Connection time.
    pub connected_at: Instant,
    /// Message sender channel.
    pub sender: mpsc::Sender<WsMessage>,
}

impl ConnectionInfo {
    /// Create new connection info.
    pub fn new(id: String, sender: mpsc::Sender<WsMessage>) -> Self {
        Self {
            id,
            user_id: None,
            session_id: None,
            connected_at: Instant::now(),
            sender,
        }
    }

    /// Send a message to this connection.
    pub async fn send(&self, message: WsMessage) -> Result<(), mpsc::error::SendError<WsMessage>> {
        self.sender.send(message).await
    }
}

/// WebSocket heartbeat configuration.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Interval between pings.
    pub interval: Duration,
    /// Timeout for pong response.
    pub timeout: Duration,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(10),
        }
    }
}

#[cfg(test)]
#[path = "websocket_tests.rs"]
mod tests;
