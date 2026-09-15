//! Cortex Code agent API client.
//!
//! Live contract probed against `https://api.cortex.foundation` (2026-08-30):
//!
//! - `POST /v1/auth/guest` — guest cookie `cortex_gt`
//! - `POST /v1/auth/device` + `POST /v1/auth/device/token` — device login
//! - `GET  /v1/me`, `GET /v1/models`
//! - `GET|POST /v1/code/sessions` (`runtime`: `cloud` | `paired` | `connected`)
//! - `GET  /v1/code/sessions/{id}/messages`
//! - `POST /v1/code/sessions/{id}/turns` body `{ message, mode: "chat"|"code" }`
//!   SSE: `reasoning_delta`, `reasoning_done`, `text_delta`, `tool_start`,
//!   `tool_end`, `usage`, `done` (plus `cancelled` / `error` / `question` when sent)
//! - `GET|POST /v1/code/hosts` — This PC pairing
//!
//! Device login is implemented in `cortex-login` against `/v1/auth/device`.
//! Code tool events describe server-owned execution, including paired/SSH
//! runtimes. No local invocation/result continuation contract is exposed.
//! Cancellation uses the explicit session cancel route; disconnect is not an
//! acknowledgement that remote work stopped.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use eventsource_stream::Eventsource;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc};
use tokio::time::timeout;
use tokio_stream::wrappers::ReceiverStream;

use super::{CompletionRequest, MessageRole, ResponseEvent, ResponseStream};
use crate::error::{CortexError, Result};
use crate::harness::TOOL_TIMEOUT_SECS;

const DEFAULT_CORTEX_URL: &str = "https://api.cortex.foundation";
const GUEST_COOKIE_NAME: &str = "cortex_gt";

/// Product-facing copy for a true outage / unreachable API.
pub const SERVICE_UNAVAILABLE: &str = "The coding service is temporarily unavailable";
/// Auth failures: tell the user how to recover. Never dump tokens.
pub const AUTH_REQUIRED: &str =
    "Not signed in. Run `cortex login` or set CORTEX_API_KEY, then try again.";
/// Wrong origin, missing route, or a session the API no longer has.
pub const ENDPOINT_NOT_FOUND: &str = "The coding endpoint was not found. Check CORTEX_API_URL (default https://api.cortex.foundation) or run `cortex login`.";
/// Quota / 429.
pub const TOO_MANY_REQUESTS: &str = "Too many requests. Please wait and try again.";

/// Strip a trailing slash so `/v1/...` is not doubled as `//v1`.
pub fn normalize_api_base(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

/// Guest-cookie token prefix stored in the keyring / env.
pub const GUEST_TOKEN_PREFIX: &str = "gt:";

pub use super::computer::{ComputerKind, DISCONNECTED_RUNTIME};

/// Per-turn context the TUI sets before `complete()`.
#[derive(Debug, Clone, Default)]
pub struct CodeTurnContext {
    pub workspace: Option<String>,
    pub computer: ComputerKind,
    pub turn_mode: Option<CodeTurnMode>,
    pub ssh_target: Option<String>,
    /// When true, the turn uses the low-latency Fast path (org policy allowing).
    pub fast_mode: bool,
}

/// Fields accepted by `POST /v1/code/sessions`.
#[derive(Debug, Clone, Default)]
pub struct CreateCodeSession {
    pub title: Option<String>,
    pub runtime: Option<String>,
    pub host_id: Option<String>,
}

/// A persisted Code session message (`GET .../messages`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeMessage {
    pub id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub created_at: String,
}

/// Turn mode accepted by `POST /v1/code/sessions/{id}/turns`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodeTurnMode {
    Chat,
    Code,
}

impl CodeTurnMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Code => "code",
        }
    }
}

/// Session returned by the Code agent API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSession {
    pub id: String,
    #[serde(default)]
    pub runtime: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub host_status: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub model_slug: String,
    #[serde(default)]
    pub model_ref: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub host_id: String,
    #[serde(default)]
    pub stream: Option<CodeSessionStream>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSessionStream {
    #[serde(default)]
    pub turns: String,
    #[serde(default)]
    pub events: String,
    #[serde(default)]
    pub realtime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSessionList {
    #[serde(default)]
    pub items: Vec<CodeSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeHost {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeHostPairing {
    pub host: CodeHost,
    #[serde(default)]
    pub pairing_code: String,
    #[serde(default)]
    pub pairing_expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestSession {
    pub kind: String,
    pub user_id: String,
}

/// SSE event from a Code session turn.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum CodeTurnEvent {
    #[serde(rename = "reasoning_delta")]
    ReasoningDelta {
        #[serde(default)]
        message_id: String,
        #[serde(default)]
        delta: String,
    },
    #[serde(rename = "reasoning_done")]
    ReasoningDone {
        #[serde(default)]
        message_id: String,
        #[serde(default)]
        duration_ms: u64,
    },
    #[serde(rename = "text_delta")]
    TextDelta {
        #[serde(default)]
        message_id: String,
        #[serde(default)]
        delta: String,
    },
    #[serde(rename = "tool_start")]
    ToolStart {
        #[serde(default)]
        invocation_id: String,
        #[serde(default)]
        tool_name: String,
        #[serde(default)]
        label: String,
        #[serde(default)]
        arguments: Option<serde_json::Value>,
    },
    #[serde(rename = "tool_end")]
    ToolEnd {
        #[serde(default)]
        invocation_id: String,
        #[serde(default)]
        outcome: String,
        #[serde(default)]
        duration_ms: u64,
        #[serde(default)]
        error_detail: Option<String>,
        #[serde(default)]
        output: Option<String>,
    },
    #[serde(rename = "usage")]
    Usage {
        #[serde(default)]
        message_id: String,
        #[serde(default)]
        input_tokens: i64,
        #[serde(default)]
        output_tokens: i64,
        #[serde(default)]
        cached_tokens: i64,
    },
    #[serde(rename = "done")]
    Done {
        #[serde(default)]
        message_id: String,
        #[serde(default)]
        finish_reason: String,
    },
    #[serde(rename = "cancelled")]
    Cancelled {
        #[serde(default)]
        message_id: String,
    },
    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        message: String,
        #[serde(default)]
        detail: Option<String>,
    },
    #[serde(rename = "question")]
    Question {
        #[serde(default)]
        invocation_id: String,
        #[serde(default)]
        questions: serde_json::Value,
    },
    #[serde(other)]
    Unknown,
}

/// Client for `/v1/code/sessions` and related auth.
#[derive(Clone)]
pub struct CodeAgentClient {
    http: Client,
    base_url: String,
    /// Bearer token or `gt:` guest cookie.
    auth: Arc<Mutex<Option<String>>>,
    session_id: Arc<Mutex<Option<String>>>,
    cancel: Arc<Mutex<Option<tokio::task::AbortHandle>>>,
    turn_context: Arc<std::sync::Mutex<CodeTurnContext>>,
    model: Arc<std::sync::Mutex<Option<String>>>,
    identity_path: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
    turn_lock: Arc<Mutex<()>>,
}

impl CodeAgentClient {
    pub fn new(base_url: Option<String>, auth: Option<String>) -> Self {
        let http =
            crate::api_client::create_client_with_timeout(Duration::from_secs(TOOL_TIMEOUT_SECS))
                .unwrap_or_else(|_| Client::new());
        Self {
            http,
            base_url: normalize_api_base(&base_url.unwrap_or_else(|| {
                std::env::var("CORTEX_API_URL").unwrap_or_else(|_| DEFAULT_CORTEX_URL.to_string())
            })),
            auth: Arc::new(Mutex::new(auth)),
            session_id: Arc::new(Mutex::new(None)),
            model: Arc::new(std::sync::Mutex::new(None)),
            identity_path: Arc::new(std::sync::Mutex::new(None)),
            turn_lock: Arc::new(Mutex::new(())),
            cancel: Arc::new(Mutex::new(None)),
            turn_context: Arc::new(std::sync::Mutex::new(CodeTurnContext {
                computer: ComputerKind::detect(),
                ssh_target: std::env::var("CORTEX_SSH_HOST")
                    .ok()
                    .or_else(|| std::env::var("CORTEX_SSH_TARGET").ok()),
                workspace: std::env::current_dir()
                    .ok()
                    .map(|p| p.display().to_string()),
                turn_mode: None,
                fast_mode: false,
            })),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Begin a guest session and store the `cortex_gt` cookie as `gt:...`.
    pub async fn begin_guest_session(&self) -> Result<GuestSession> {
        let url = format!("{}/v1/auth/guest", self.base_url);
        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .header(reqwest::header::USER_AGENT, crate::api_client::USER_AGENT)
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| CortexError::from_reqwest_with_proxy_check(e, &url))?;

        let cookie = resp
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find_map(|c| {
                c.split(';')
                    .next()
                    .and_then(|pair| pair.strip_prefix(&format!("{GUEST_COOKIE_NAME}=")))
                    .map(str::to_string)
            });

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(map_api_error(status, &body));
        }

        let guest: GuestSession = resp.json().await.map_err(|e| CortexError::BackendError {
            message: format!("Failed to parse guest session: {e}"),
        })?;

        if let Some(cookie) = cookie {
            *self.auth.lock().await = Some(format!("{GUEST_TOKEN_PREFIX}{cookie}"));
        }
        Ok(guest)
    }

    /// Ensure we have a session cookie or bearer token.
    pub async fn ensure_auth(&self) -> Result<()> {
        if self.auth.lock().await.is_some() {
            return Ok(());
        }
        if let Ok(token) = std::env::var("CORTEX_AUTH_TOKEN")
            && !token.is_empty()
        {
            *self.auth.lock().await = Some(token);
            return Ok(());
        }
        if let Some(token) = cortex_login::get_auth_token() {
            *self.auth.lock().await = Some(token);
            return Ok(());
        }
        self.begin_guest_session().await?;
        Ok(())
    }

    pub async fn auth_token(&self) -> Option<String> {
        self.auth.lock().await.clone()
    }

    pub async fn current_session_id(&self) -> Option<String> {
        self.session_id.lock().await.clone()
    }

    pub async fn set_session_id(&self, id: impl Into<String>) {
        *self.session_id.lock().await = Some(id.into());
    }

    pub async fn resume_session(&self, id: &str) -> Result<()> {
        if !valid_session_id(id) {
            return Err(CortexError::InvalidInput("Invalid Code session ID.".into()));
        }
        self.set_session_id(id).await;
        Ok(())
    }

    pub fn set_turn_context(&self, ctx: CodeTurnContext) {
        if let Ok(mut guard) = self.turn_context.lock() {
            *guard = ctx;
        }
    }

    pub fn turn_context(&self) -> CodeTurnContext {
        self.turn_context
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// List Code sessions.
    pub async fn list_sessions(&self) -> Result<Vec<CodeSession>> {
        self.ensure_auth().await?;
        let url = format!("{}/v1/code/sessions", self.base_url);
        let resp = self.authed_get(&url).await?;
        let list: CodeSessionList = parse_json(resp).await?;
        Ok(list.items)
    }

    /// Create a Code session. Optional title.
    pub async fn create_session(&self, title: Option<&str>) -> Result<CodeSession> {
        self.create_session_with(CreateCodeSession {
            title: title.map(str::to_string),
            ..Default::default()
        })
        .await
    }

    /// Create a Code session with runtime / host pairing.
    pub async fn create_session_with(&self, req: CreateCodeSession) -> Result<CodeSession> {
        self.ensure_auth().await?;
        let url = format!("{}/v1/code/sessions", self.base_url);
        let mut body = serde_json::Map::new();
        if let Some(t) = req.title.filter(|t| !t.is_empty()) {
            body.insert("title".into(), serde_json::Value::String(t));
        }
        if let Some(runtime) = req.runtime.filter(|t| !t.is_empty()) {
            body.insert("runtime".into(), serde_json::Value::String(runtime));
        }
        if let Some(host_id) = req.host_id.filter(|t| !t.is_empty()) {
            body.insert("host_id".into(), serde_json::Value::String(host_id));
        }
        if let Some(model) = self.model.lock().ok().and_then(|m| m.clone()) {
            body.insert("model_slug".into(), serde_json::Value::String(model));
        }
        let resp = self
            .authed_post(&url, &serde_json::Value::Object(body))
            .await?;
        let session: CodeSession = parse_json(resp).await?;
        *self.session_id.lock().await = Some(session.id.clone());
        if let Some(path) = self.identity_path.lock().ok().and_then(|p| p.clone()) {
            let identity = serde_json::json!({"session_id": session.id, "origin": self.base_url});
            let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
            std::fs::write(&temporary, serde_json::to_vec(&identity)?)?;
            std::fs::rename(temporary, path)?;
        }
        Ok(session)
    }

    /// Get one session.
    pub async fn get_session(&self, id: &str) -> Result<CodeSession> {
        self.ensure_auth().await?;
        let url = format!("{}/v1/code/sessions/{id}", self.base_url);
        let resp = self.authed_get(&url).await?;
        parse_json(resp).await
    }

    /// Reuse only the explicitly bound session. Network/auth failures never
    /// create a replacement conversation or switch runtime ownership.
    pub async fn ensure_session(&self) -> Result<String> {
        let existing = self.session_id.lock().await.clone();
        if let Some(id) = existing {
            let session = self.get_session(&id).await?;
            let model = self.model.lock().ok().and_then(|m| m.clone());
            if model
                .as_deref()
                .is_some_and(|m| m != session.model_slug && m != session.model_ref)
            {
                return Err(CortexError::InvalidInput(
                    "The selected model differs from the resumed Code session. Start a new session to change models.".into()
                ));
            }
            return Ok(id);
        }
        let ctx = self.turn_context();
        if ctx.computer != ComputerKind::Cloud {
            return Err(CortexError::InvalidInput(DISCONNECTED_RUNTIME.into()));
        }
        Ok(self
            .create_session_with(CreateCodeSession {
                runtime: Some("cloud".into()),
                ..Default::default()
            })
            .await?
            .id)
    }

    pub fn set_model(&self, model: &str) {
        if let Ok(mut selected) = self.model.lock() {
            *selected = Some(model.to_string());
        }
    }

    pub fn configure_session_identity(
        &self,
        home: &std::path::Path,
        id: &str,
        resume: bool,
    ) -> Result<()> {
        // Local IDs come from ConversationId, never from a remote path segment.
        let id = uuid::Uuid::parse_str(id)
            .map_err(|_| CortexError::InvalidInput("Invalid local session ID.".into()))?;
        let path = home
            .join("sessions")
            .join(format!("{id}.code-session.json"));
        let remote = if resume {
            let data = std::fs::read(&path).map_err(|_| CortexError::InvalidInput(
                "This local session has no remote Code identity. It cannot be resumed safely; start a new session.".into()
            ))?;
            let value: serde_json::Value = serde_json::from_slice(&data)?;
            if value["origin"].as_str() != Some(self.base_url.as_str()) {
                return Err(CortexError::InvalidInput(
                    "The saved Code session belongs to a different API origin.".into(),
                ));
            }
            Some(
                value["session_id"]
                    .as_str()
                    .filter(|id| valid_session_id(id))
                    .ok_or_else(|| {
                        CortexError::InvalidInput("Invalid saved Code session ID.".into())
                    })?
                    .to_owned(),
            )
        } else {
            None
        };
        *self
            .session_id
            .try_lock()
            .map_err(|_| CortexError::InvalidInput("A Code turn is already running.".into()))? =
            remote;
        *self.identity_path.lock().map_err(|_| {
            CortexError::InvalidInput("Code session state is unavailable.".into())
        })? = Some(path);
        Ok(())
    }

    /// Allow or deny a paused tool call on an attached session.
    pub async fn approve_invocation(
        &self,
        session_id: &str,
        invocation_id: &str,
        approved: bool,
    ) -> Result<()> {
        self.ensure_auth().await?;
        if session_id.is_empty() || invocation_id.is_empty() {
            return Err(CortexError::InvalidInput(
                "Session id and invocation id are required.".into(),
            ));
        }
        let url = format!("{}/v1/code/sessions/{session_id}/approvals", self.base_url);
        let body = serde_json::json!({
            "invocation_id": invocation_id,
            "approved": approved,
        });
        let _ = self.authed_post(&url, &body).await?;
        Ok(())
    }

    /// Transcript for a Code session (server-side persistence).
    pub async fn list_messages(&self, session_id: &str) -> Result<Vec<CodeMessage>> {
        self.ensure_auth().await?;
        let url = format!("{}/v1/code/sessions/{session_id}/messages", self.base_url);
        let resp = self.authed_get(&url).await?;
        #[derive(Deserialize)]
        struct List {
            #[serde(default)]
            items: Vec<CodeMessage>,
        }
        let list: List = parse_json(resp).await?;
        Ok(list.items)
    }

    /// Register this CLI as a Code host (pairing).
    pub async fn register_host(&self, name: &str) -> Result<CodeHostPairing> {
        self.ensure_auth().await?;
        let url = format!("{}/v1/code/hosts", self.base_url);
        let resp = self
            .authed_post(&url, &serde_json::json!({"name": name}))
            .await?;
        parse_json(resp).await
    }

    /// Stop local streaming and request remote cancellation. The checked form
    /// distinguishes acknowledgement failure from confirmed cancellation.
    pub async fn cancel_in_flight(&self) {
        let _ = self.cancel_in_flight_checked().await;
    }

    pub async fn cancel_in_flight_checked(&self) -> Result<()> {
        if let Some(handle) = self.cancel.lock().await.take() {
            handle.abort();
        }
        let Some(session_id) = self.session_id.lock().await.clone() else {
            return Ok(());
        };
        let url = format!("{}/v1/code/sessions/{session_id}/cancel", self.base_url);
        match timeout(
            Duration::from_secs(3),
            self.authed_post(&url, &serde_json::json!({})),
        )
        .await
        {
            Ok(Ok(_)) => Ok(()),
            _ => Err(CortexError::BackendError {
                message: super::runtime_contract::REMOTE_CANCEL_UNCONFIRMED.into(),
            }),
        }
    }

    pub async fn stream_turn(&self, message: &str, mode: CodeTurnMode) -> Result<ResponseStream> {
        self.ensure_auth().await?;
        self.post_turn(message, self.turn_context().turn_mode.unwrap_or(mode))
            .await
    }

    async fn post_turn(&self, message: &str, mode: CodeTurnMode) -> Result<ResponseStream> {
        let turn_guard = self.turn_lock.clone().try_lock_owned().map_err(|_| {
            CortexError::InvalidInput("A Code turn is already running in this session.".into())
        })?;
        let session_id = self.ensure_session().await?;
        let url = format!("{}/v1/code/sessions/{session_id}/turns", self.base_url);
        let fast_mode = self.turn_context().fast_mode;
        let body = serde_json::json!({
            "message": message,
            "mode": mode.as_str(),
            // An existing Code conversation keeps its mode. Interaction is the
            // supported per-turn read-only/planning control in the backend.
            "interaction": if mode == CodeTurnMode::Chat { "plan" } else { "agent" },
            // Propagate the session Fast setting so remote turns are not UI-only.
            "fast_mode": fast_mode,
        });

        let mut req = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .header(reqwest::header::USER_AGENT, crate::api_client::USER_AGENT)
            .json(&body);
        req = apply_auth(req, self.auth.lock().await.as_deref());

        let resp = req
            .send()
            .await
            .map_err(|e| CortexError::from_reqwest_with_proxy_check(e, &url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(map_api_error(status, &text));
        }

        let (tx, rx) = mpsc::channel::<Result<ResponseEvent>>(64);
        let stream = resp.bytes_stream().eventsource();

        let task = tokio::spawn(async move {
            pump_sse(stream, tx).await;
        });
        let abort = task.abort_handle();
        *self.cancel.lock().await = Some(abort.clone());

        Ok(Box::pin(AbortOnDropStream {
            inner: ReceiverStream::new(rx),
            abort,
            _turn_guard: turn_guard,
        }))
    }

    /// Last user text from a completion request (Code API takes a single message).
    pub fn last_user_message(request: &CompletionRequest) -> String {
        request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
            .and_then(|m| m.content.as_text())
            .unwrap_or("")
            .to_string()
    }

    pub(super) async fn authed_get(&self, url: &str) -> Result<reqwest::Response> {
        let mut req = self
            .http
            .get(url)
            .header("Accept", "application/json")
            .header(reqwest::header::USER_AGENT, crate::api_client::USER_AGENT);
        req = apply_auth(req, self.auth.lock().await.as_deref());
        let resp = req
            .send()
            .await
            .map_err(|e| CortexError::from_reqwest_with_proxy_check(e, url))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(map_api_error(status, &text));
        }
        Ok(resp)
    }

    async fn authed_post(&self, url: &str, body: &serde_json::Value) -> Result<reqwest::Response> {
        let mut req = self
            .http
            .post(url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header(reqwest::header::USER_AGENT, crate::api_client::USER_AGENT)
            .json(body);
        req = apply_auth(req, self.auth.lock().await.as_deref());
        let resp = req
            .send()
            .await
            .map_err(|e| CortexError::from_reqwest_with_proxy_check(e, url))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(map_api_error(status, &text));
        }
        Ok(resp)
    }
}

struct AbortOnDropStream {
    inner: ReceiverStream<Result<ResponseEvent>>,
    abort: tokio::task::AbortHandle,
    _turn_guard: tokio::sync::OwnedMutexGuard<()>,
}

impl Stream for AbortOnDropStream {
    type Item = Result<ResponseEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl Drop for AbortOnDropStream {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

/// Cached Code session id for a workspace (`~/.cortex/code-sessions.json`).
pub fn cached_code_session_id(workspace: &str) -> Option<String> {
    load_cached_session_id(workspace)
}

#[cfg(test)]
fn persist_session_id(workspace: &str, session_id: &str) {
    let Some(path) = code_session_cache_path() else {
        return;
    };
    let mut map = load_session_cache();
    map.insert(
        workspace_key(workspace),
        serde_json::Value::String(session_id.to_string()),
    );
    if let Ok(json) = serde_json::to_string_pretty(&map) {
        let _ = std::fs::write(path, json);
    }
}

fn load_cached_session_id(workspace: &str) -> Option<String> {
    load_session_cache()
        .get(&workspace_key(workspace))
        .and_then(|v| v.as_str().map(str::to_string))
}

fn load_session_cache() -> serde_json::Map<String, serde_json::Value> {
    let Some(path) = code_session_cache_path() else {
        return serde_json::Map::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn workspace_key(workspace: &str) -> String {
    if workspace.is_empty() {
        "_default".to_string()
    } else {
        workspace.to_string()
    }
}

fn code_session_cache_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("CORTEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".cortex")))?;
    let _ = std::fs::create_dir_all(&home);
    Some(home.join("code-sessions.json"))
}

fn apply_auth(mut req: reqwest::RequestBuilder, auth: Option<&str>) -> reqwest::RequestBuilder {
    if let Some(token) = auth {
        if let Some(cookie) = token.strip_prefix(GUEST_TOKEN_PREFIX) {
            req = req.header(
                reqwest::header::COOKIE,
                format!("{GUEST_COOKIE_NAME}={cookie}"),
            );
        } else {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
    }
    req
}

pub(super) async fn parse_json<T: for<'de> Deserialize<'de>>(resp: reqwest::Response) -> Result<T> {
    resp.json().await.map_err(|e| CortexError::BackendError {
        message: format!("Failed to parse API response: {e}"),
    })
}

fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn map_api_error(status: reqwest::StatusCode, body: &str) -> CortexError {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("detail")
                .and_then(|d| d.as_str())
                .map(str::to_string)
                .or_else(|| v.get("title").and_then(|d| d.as_str()).map(str::to_string))
                .or_else(|| v.get("error").and_then(|d| d.as_str()).map(str::to_string))
        });
    match status.as_u16() {
        401 | 403 => CortexError::AuthenticationError {
            message: AUTH_REQUIRED.to_string(),
        },
        404 => CortexError::BackendError {
            message: ENDPOINT_NOT_FOUND.to_string(),
        },
        429 => CortexError::RateLimit(TOO_MANY_REQUESTS.to_string()),
        500..=599 => CortexError::BackendUnavailable(SERVICE_UNAVAILABLE.to_string()),
        _ => CortexError::BackendError {
            message: sanitize_api_detail(detail),
        },
    }
}

/// Keep operator/API detail when it is already product-safe; otherwise the
/// generic outage line. Never pass through SDK or transport names.
fn sanitize_api_detail(detail: Option<String>) -> String {
    let Some(raw) = detail.filter(|s| !s.trim().is_empty()) else {
        return SERVICE_UNAVAILABLE.to_string();
    };
    let lower = raw.to_lowercase();
    if lower.contains("reqwest")
        || lower.contains("hyper")
        || lower.contains("openai")
        || lower.contains("anthropic")
        || lower.contains("tokio")
    {
        return SERVICE_UNAVAILABLE.to_string();
    }
    if lower.contains("unauthorized")
        || lower.contains("unauthenticated")
        || lower.contains("invalid api key")
        || lower.contains("invalid token")
    {
        return AUTH_REQUIRED.to_string();
    }
    raw
}

#[path = "code_sse.rs"]
mod code_sse;
#[cfg(test)]
use code_sse::map_sse_error_copy;
use code_sse::pump_sse;

#[cfg(test)]
#[path = "code_agent_tests.rs"]
mod tests;
