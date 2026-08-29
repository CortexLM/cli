//! Cortex Code agent API client (backend PR #58, now on main).
//!
//! Live contract probed against `https://api.cortex.foundation`:
//!
//! - `POST /v1/auth/guest` — guest session cookie `cortex_gt`
//! - `GET  /v1/me`
//! - `GET  /v1/models` — `{ items: [{ slug, display_name, context_tokens, ... }] }`
//! - `GET|POST /v1/code/sessions`
//! - `POST /v1/code/sessions/{id}/turns` body `{ message: string, mode: "chat"|"code" }`
//!   streams SSE: `reasoning_delta`, `reasoning_done`, `text_delta`,
//!   `tool_start`, `tool_end`, `usage`, `done`
//! - `GET|POST /v1/code/hosts` — local host pairing (`name` → pairing_code)
//!
//! Device login: the CLI historically called `POST /auth/device/code`. That
//! path is 404 on the live API. Guest session is the working unauthenticated
//! adapter. TODO(backend): restore a CLI device-code grant on
//! `POST /v1/auth/device/code` (or document WorkOS device authorization).
//!
//! Cancel: abort the local SSE request. The live API has no
//! `POST .../cancel` route (404). TODO(backend): add an explicit cancel.

use std::sync::Arc;
use std::time::Duration;

use eventsource_stream::Eventsource;
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
            session_id: Arc::new(Mutex::new(None)),
            cancel: Arc::new(Mutex::new(None)),
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
        self.ensure_auth().await?;
        let url = format!("{}/v1/code/sessions", self.base_url);
        let body = match title {
            Some(t) if !t.is_empty() => serde_json::json!({"title": t}),
            _ => serde_json::json!({}),
        };
        let resp = self.authed_post(&url, &body).await?;
        let session: CodeSession = parse_json(resp).await?;
        *self.session_id.lock().await = Some(session.id.clone());
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
            return Ok(id);
        }
        Ok(self.create_session(None).await?.id)
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
    /// TODO(backend): `POST /v1/code/sessions/{id}/cancel` is not implemented
    /// (404). This is a CLI-side adapter.
    pub async fn cancel_in_flight(&self) {
        if let Some(handle) = self.cancel.lock().await.take() {
            handle.abort();
        }
    }

    /// Stream a turn against the Code agent API.
    pub async fn stream_turn(&self, message: &str, mode: CodeTurnMode) -> Result<ResponseStream> {
        self.ensure_auth().await?;
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
        let task = tokio::spawn(async move {
            pump_sse(stream, tx).await;
        });
        *self.cancel.lock().await = Some(task.abort_handle());

        Ok(Box::pin(ReceiverStream::new(rx)))
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

async fn pump_sse<S>(stream: S, tx: mpsc::Sender<Result<ResponseEvent>>)
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
            } => {
                tool_calls.push(super::ToolCall {
                    id: invocation_id.clone(),
                    call_type: "function".to_string(),
                    function: super::FunctionCall {
                        name: tool_name.clone(),
                        arguments: serde_json::json!({"label": label}).to_string(),
                    },
                });
                Some(ResponseEvent::ToolCall(ToolCallEvent {
                    id: invocation_id,
                    name: tool_name,
                    arguments: serde_json::json!({"label": label}).to_string(),
                    remote: true,
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
}
