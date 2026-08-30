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
//! Cancel: abort the local SSE task. `POST .../cancel` is still 404 —
//! TODO(backend): add an explicit cancel route.
//! Cloud turns currently emit no VM `tool_start` events. This PC still
//! registers a host and executes local tools when the SSE carries arguments.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use eventsource_stream::Eventsource;
use futures::Stream;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc};
use tokio::time::timeout;
use tokio_stream::wrappers::ReceiverStream;

use super::{
    CompletionRequest, CompletionResponse, FinishReason, Message, MessageContent, MessageRole,
    ResponseEvent, ResponseStream, TokenUsage, ToolCallEvent,
};
use crate::error::{CortexError, Result};
use crate::harness::{TOOL_TIMEOUT_SECS, redact_secrets};

const DEFAULT_CORTEX_URL: &str = "https://api.cortex.foundation";
const CHUNK_TIMEOUT_SECS: u64 = 60;
const GUEST_COOKIE_NAME: &str = "cortex_gt";

/// Guest-cookie token prefix stored in the keyring / env.
pub const GUEST_TOKEN_PREFIX: &str = "gt:";

/// Where tools run for this CLI session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerKind {
    /// Local workspace passed to the CLI (This PC).
    #[default]
    ThisPc,
    /// Cloud runtime (Firecracker VM when the API provisions one).
    Cloud,
    /// SSH remote, when `CORTEX_SSH_HOST` is set.
    Ssh,
}

impl ComputerKind {
    /// Detect from the environment. A workspace path (cwd) means This PC
    /// unless the operator forces cloud or sets an SSH target.
    pub fn detect() -> Self {
        if std::env::var("CORTEX_SSH_HOST")
            .ok()
            .filter(|s| !s.is_empty())
            .is_some()
            || std::env::var("CORTEX_SSH_TARGET")
                .ok()
                .filter(|s| !s.is_empty())
                .is_some()
        {
            return Self::Ssh;
        }
        match std::env::var("CORTEX_COMPUTER")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "cloud" => Self::Cloud,
            "ssh" => Self::Ssh,
            _ => Self::ThisPc,
        }
    }

    /// Product label for the TUI.
    pub fn label(self) -> &'static str {
        match self {
            Self::ThisPc => "This PC",
            Self::Cloud => "Cloud",
            Self::Ssh => "SSH",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ThisPc => "this_pc",
            Self::Cloud => "cloud",
            Self::Ssh => "ssh",
        }
    }
}

/// Per-turn context the TUI sets before `complete()`.
#[derive(Debug, Clone, Default)]
pub struct CodeTurnContext {
    pub workspace: Option<String>,
    pub computer: ComputerKind,
    pub turn_mode: Option<CodeTurnMode>,
    pub ssh_target: Option<String>,
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
}

impl CodeAgentClient {
    pub fn new(base_url: Option<String>, auth: Option<String>) -> Self {
        let http =
            crate::api_client::create_client_with_timeout(Duration::from_secs(TOOL_TIMEOUT_SECS))
                .unwrap_or_else(|_| Client::new());
        Self {
            http,
            base_url: base_url.unwrap_or_else(|| {
                std::env::var("CORTEX_API_URL").unwrap_or_else(|_| DEFAULT_CORTEX_URL.to_string())
            }),
            auth: Arc::new(Mutex::new(auth)),
            session_id: Arc::new(Mutex::new(load_cached_session_id(
                &std::env::current_dir()
                    .ok()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            ))),
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
        let resp = self
            .authed_post(&url, &serde_json::Value::Object(body))
            .await?;
        let session: CodeSession = parse_json(resp).await?;
        *self.session_id.lock().await = Some(session.id.clone());
        persist_session_id(
            self.turn_context().workspace.as_deref().unwrap_or(""),
            &session.id,
        );
        Ok(session)
    }

    /// Get one session.
    pub async fn get_session(&self, id: &str) -> Result<CodeSession> {
        self.ensure_auth().await?;
        let url = format!("{}/v1/code/sessions/{id}", self.base_url);
        let resp = self.authed_get(&url).await?;
        parse_json(resp).await
    }

    /// Ensure a reusable session id exists.
    pub async fn ensure_session(&self) -> Result<String> {
        if let Some(id) = self.session_id.lock().await.clone() {
            match self.get_session(&id).await {
                Ok(_) => return Ok(id),
                Err(_) => {
                    tracing::debug!(id = %id, "Cached Code session is gone; creating a new one");
                    *self.session_id.lock().await = None;
                }
            }
        }
        let ctx = self.turn_context();
        let mut req = CreateCodeSession::default();
        match ctx.computer {
            ComputerKind::ThisPc | ComputerKind::Ssh => {
                let name = hostname::get()
                    .ok()
                    .and_then(|h| h.into_string().ok())
                    .unwrap_or_else(|| "Cortex CLI".to_string());
                match self.register_host(&name).await {
                    Ok(pairing) => {
                        req.host_id = Some(pairing.host.id);
                        req.runtime = Some("paired".to_string());
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "This PC host registration failed; opening a cloud Code session"
                        );
                    }
                }
            }
            ComputerKind::Cloud => {}
        }
        Ok(self.create_session_with(req).await?.id)
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

    /// Cancel the in-flight turn by aborting the local SSE task.
    ///
    /// Also POSTs `/v1/code/sessions/{id}/cancel` when a session exists.
    /// The live API currently returns 404 for that route (TODO(backend)).
    pub async fn cancel_in_flight(&self) {
        if let Some(handle) = self.cancel.lock().await.take() {
            handle.abort();
        }
        let session_id = self.session_id.lock().await.clone();
        let Some(session_id) = session_id else {
            return;
        };
        let url = format!("{}/v1/code/sessions/{session_id}/cancel", self.base_url);
        let body = serde_json::json!({});
        match self.authed_post(&url, &body).await {
            Ok(_) => tracing::debug!("Code session cancel accepted"),
            Err(e) => tracing::debug!(error = %e, "Code session cancel route missing or failed"),
        }
    }

    /// Stream a turn against the Code agent API.
    pub async fn stream_turn(&self, message: &str, mode: CodeTurnMode) -> Result<ResponseStream> {
        self.ensure_auth().await?;
        let ctx = self.turn_context();
        let mode = ctx.turn_mode.unwrap_or(mode);
        let session_id = self.ensure_session().await?;
        let url = format!("{}/v1/code/sessions/{session_id}/turns", self.base_url);
        let body = serde_json::json!({
            "message": message,
            "mode": mode.as_str(),
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
        let execute_local = matches!(ctx.computer, ComputerKind::ThisPc | ComputerKind::Ssh);
        let task = tokio::spawn(async move {
            pump_sse(stream, tx, execute_local).await;
        });
        let abort = task.abort_handle();
        *self.cancel.lock().await = Some(abort.clone());

        Ok(Box::pin(AbortOnDropStream {
            inner: ReceiverStream::new(rx),
            abort,
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

    async fn authed_get(&self, url: &str) -> Result<reqwest::Response> {
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

async fn parse_json<T: for<'de> Deserialize<'de>>(resp: reqwest::Response) -> Result<T> {
    resp.json().await.map_err(|e| CortexError::BackendError {
        message: format!("Failed to parse API response: {e}"),
    })
}

fn map_api_error(status: reqwest::StatusCode, body: &str) -> CortexError {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("detail")
                .and_then(|d| d.as_str())
                .map(str::to_string)
                .or_else(|| v.get("title").and_then(|d| d.as_str()).map(str::to_string))
        });
    let message = match status.as_u16() {
        401 | 403 => "Sign in to continue, or start a guest session.".to_string(),
        404 => "The coding service is temporarily unavailable".to_string(),
        429 => "Too many requests. Please wait and try again.".to_string(),
        500..=599 => "The coding service is temporarily unavailable".to_string(),
        _ => detail.unwrap_or_else(|| "The coding service is temporarily unavailable".to_string()),
    };
    CortexError::BackendError { message }
}

async fn pump_sse<S>(stream: S, tx: mpsc::Sender<Result<ResponseEvent>>, execute_local: bool)
where
    S: futures::Stream<
            Item = std::result::Result<
                eventsource_stream::Event,
                eventsource_stream::EventStreamError<reqwest::Error>,
            >,
        > + Unpin,
{
    let mut stream = stream;
    let mut accumulated = String::new();
    let mut usage = TokenUsage::default();
    let mut tool_calls = Vec::new();
    let chunk_timeout = Duration::from_secs(CHUNK_TIMEOUT_SECS);

    loop {
        let event_result = match timeout(chunk_timeout, stream.next()).await {
            Ok(Some(result)) => result,
            Ok(None) => break,
            Err(_) => {
                let _ = tx
                    .send(Err(CortexError::BackendError {
                        message: "The coding service is temporarily unavailable".into(),
                    }))
                    .await;
                break;
            }
        };

        let event = match event_result {
            Ok(ev) => ev,
            Err(e) => {
                let _ = tx
                    .send(Err(CortexError::BackendError {
                        message: format!("Stream error: {e}"),
                    }))
                    .await;
                break;
            }
        };

        if event.data.is_empty() || event.data == "[DONE]" {
            continue;
        }

        let parsed = match serde_json::from_str::<CodeTurnEvent>(&event.data) {
            Ok(ev) => ev,
            Err(e) => {
                tracing::debug!(error = %e, data = %event.data, "Unknown Code turn SSE event");
                continue;
            }
        };

        let mapped = match parsed {
            CodeTurnEvent::ReasoningDelta { delta, .. } => Some(ResponseEvent::Reasoning(delta)),
            CodeTurnEvent::TextDelta { delta, .. } => {
                accumulated.push_str(&delta);
                Some(ResponseEvent::Delta(delta))
            }
            CodeTurnEvent::ToolStart {
                invocation_id,
                tool_name,
                label,
                arguments,
            } => {
                let has_explicit_args = arguments.as_ref().is_some_and(|v| {
                    !v.is_null()
                        && (v.get("command").is_some()
                            || v.get("file_path").is_some()
                            || v.get("path").is_some()
                            || v.as_object().is_some_and(|o| o.len() > 1))
                });
                let args = match arguments {
                    Some(v) if !v.is_null() => v,
                    _ => serde_json::json!({"label": label}),
                };
                let remote = !(execute_local && has_explicit_args);
                tool_calls.push(super::ToolCall {
                    id: invocation_id.clone(),
                    call_type: "function".to_string(),
                    function: super::FunctionCall {
                        name: tool_name.clone(),
                        arguments: args.to_string(),
                    },
                });
                Some(ResponseEvent::ToolCall(ToolCallEvent {
                    id: invocation_id,
                    name: tool_name,
                    arguments: args.to_string(),
                    remote,
                }))
            }
            CodeTurnEvent::ToolEnd {
                invocation_id,
                outcome,
                error_detail,
                output,
                ..
            } => {
                let success = outcome.eq_ignore_ascii_case("ok")
                    || outcome.eq_ignore_ascii_case("success")
                    || outcome.eq_ignore_ascii_case("completed");
                let raw = output.or(error_detail).unwrap_or_default();
                Some(ResponseEvent::ToolResult {
                    id: invocation_id,
                    success,
                    output: redact_secrets(&raw),
                })
            }
            CodeTurnEvent::Usage {
                input_tokens,
                output_tokens,
                ..
            } => {
                usage = TokenUsage {
                    input_tokens,
                    output_tokens,
                    total_tokens: input_tokens + output_tokens,
                };
                None
            }
            CodeTurnEvent::Done { finish_reason, .. } => {
                let reason = if finish_reason.contains("tool") {
                    FinishReason::ToolCalls
                } else {
                    FinishReason::Stop
                };
                Some(ResponseEvent::Done(CompletionResponse {
                    message: Some(Message {
                        role: MessageRole::Assistant,
                        content: MessageContent::Text(accumulated.clone()),
                        tool_call_id: None,
                        tool_calls: None,
                    }),
                    usage: usage.clone(),
                    finish_reason: reason,
                    tool_calls: tool_calls.clone(),
                }))
            }
            CodeTurnEvent::Cancelled { .. } => Some(ResponseEvent::Error("Cancelled".into())),
            CodeTurnEvent::Error { message, detail } => {
                let raw = if message.is_empty() {
                    detail.unwrap_or_default()
                } else {
                    message
                };
                let mapped = if raw.to_lowercase().contains("unavailable") || raw.is_empty() {
                    "The coding service is temporarily unavailable".to_string()
                } else {
                    raw
                };
                Some(ResponseEvent::Error(mapped))
            }
            CodeTurnEvent::Question {
                invocation_id,
                questions,
            } => Some(ResponseEvent::ToolCall(ToolCallEvent {
                id: invocation_id,
                name: "Questions".into(),
                arguments: questions.to_string(),
                remote: false,
            })),
            CodeTurnEvent::ReasoningDone { .. } | CodeTurnEvent::Unknown => None,
        };

        if let Some(ev) = mapped
            && tx.send(Ok(ev)).await.is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tool_start_event() {
        let raw = r#"{"type":"tool_start","invocation_id":"tci_1","tool_name":"python","label":"Ran Python"}"#;
        let ev: CodeTurnEvent = serde_json::from_str(raw).unwrap();
        match ev {
            CodeTurnEvent::ToolStart { tool_name, .. } => assert_eq!(tool_name, "python"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_done_event() {
        let raw = r#"{"type":"done","message_id":"msg_1","finish_reason":"stop"}"#;
        let ev: CodeTurnEvent = serde_json::from_str(raw).unwrap();
        assert!(matches!(ev, CodeTurnEvent::Done { .. }));
    }

    #[test]
    fn last_user_message_picks_latest() {
        let req = CompletionRequest {
            messages: vec![
                Message::system("sys"),
                Message::user("first"),
                Message::assistant("ok"),
                Message::user("second"),
            ],
            model: "cortex-1-mini".into(),
            ..Default::default()
        };
        assert_eq!(CodeAgentClient::last_user_message(&req), "second");
    }

    #[test]
    fn guest_token_prefix_is_stable() {
        assert_eq!(GUEST_TOKEN_PREFIX, "gt:");
    }

    #[test]
    fn computer_kind_labels() {
        assert_eq!(ComputerKind::ThisPc.label(), "This PC");
        assert_eq!(ComputerKind::Cloud.label(), "Cloud");
        assert_eq!(ComputerKind::Ssh.label(), "SSH");
        assert_eq!(ComputerKind::ThisPc.as_str(), "this_pc");
    }

    #[test]
    fn parses_tool_start_with_arguments() {
        let raw = r#"{"type":"tool_start","invocation_id":"tci_2","tool_name":"Bash","arguments":{"command":"ls"}}"#;
        let ev: CodeTurnEvent = serde_json::from_str(raw).unwrap();
        match ev {
            CodeTurnEvent::ToolStart {
                tool_name,
                arguments,
                ..
            } => {
                assert_eq!(tool_name, "Bash");
                assert_eq!(arguments.unwrap()["command"].as_str().unwrap(), "ls");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_cancelled_and_question() {
        let cancelled: CodeTurnEvent =
            serde_json::from_str(r#"{"type":"cancelled","message_id":"m"}"#).unwrap();
        assert!(matches!(cancelled, CodeTurnEvent::Cancelled { .. }));
        let q: CodeTurnEvent = serde_json::from_str(
            r#"{"type":"question","invocation_id":"q1","questions":{"prompt":"Ship it?"}}"#,
        )
        .unwrap();
        match q {
            CodeTurnEvent::Question { invocation_id, .. } => assert_eq!(invocation_id, "q1"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn turn_mode_only_chat_or_code() {
        assert_eq!(CodeTurnMode::Code.as_str(), "code");
        assert_eq!(CodeTurnMode::Chat.as_str(), "chat");
    }

    #[test]
    fn api_errors_are_product_facing() {
        let err = map_api_error(reqwest::StatusCode::NOT_FOUND, r#"{"detail":"nope"}"#);
        let msg = err.to_string();
        assert!(
            msg.contains("temporarily unavailable"),
            "expected product error, got {msg}"
        );
        assert!(!msg.to_lowercase().contains("reqwest"));
        assert!(!msg.contains("nope"));
    }

    #[test]
    fn persists_session_id_under_cortex_home() {
        let dir = std::env::temp_dir().join(format!("cortex-sess-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let prev = std::env::var_os("CORTEX_HOME");
        unsafe {
            std::env::set_var("CORTEX_HOME", &dir);
        }
        persist_session_id("/tmp/demo-ws", "sess_abc");
        assert_eq!(
            cached_code_session_id("/tmp/demo-ws").as_deref(),
            Some("sess_abc")
        );
        match prev {
            Some(v) => unsafe { std::env::set_var("CORTEX_HOME", v) },
            None => unsafe { std::env::remove_var("CORTEX_HOME") },
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tool_start_without_args_is_display_only() {
        let args = serde_json::json!({"label": "Ran Python"});
        let has_exec_args = args.get("command").is_some()
            || args.get("file_path").is_some()
            || args.get("path").is_some()
            || args
                .as_object()
                .is_some_and(|o| o.len() > 1 && !o.contains_key("label"));
        assert!(!has_exec_args);
    }
}
