//! Cortex Backend Client
//!
//! Provides unified interface for the Cortex Backend API.
//! All LLM requests go through the Cortex backend with OAuth authentication.

mod code_agent;
mod cortex;
pub mod types;

pub use code_agent::{
    CodeAgentClient, CodeHost, CodeHostPairing, CodeMessage, CodeSession, CodeTurnContext,
    CodeTurnEvent, CodeTurnMode, ComputerKind, CreateCodeSession, GUEST_TOKEN_PREFIX, GuestSession,
    cached_code_session_id,
};
pub use cortex::{CortexClient, CortexModel, PricingInfo};
pub use types::*;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio_stream::Stream;

use crate::error::{CortexError, Result};

/// Stream type for response events.
pub type ResponseStream = Pin<Box<dyn Stream<Item = Result<ResponseEvent>> + Send>>;

/// Simple tool call reference for MessageContent::ToolCalls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRef {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Trait for model clients.
#[async_trait]
pub trait ModelClient: Send + Sync {
    /// Get the model name.
    fn model(&self) -> &str;

    /// Get the provider name.
    fn provider(&self) -> &str;

    /// Get model capabilities.
    fn capabilities(&self) -> &ModelCapabilities;

    /// Send a completion request and get a stream of responses.
    async fn complete(&self, request: CompletionRequest) -> Result<ResponseStream>;

    /// Send a completion request and get the full response (non-streaming).
    async fn complete_sync(&self, request: CompletionRequest) -> Result<CompletionResponse>;

    /// Configure the next Code session turn (workspace / computer / mode).
    fn configure_code_turn(&self, _ctx: CodeTurnContext) {}

    /// Abort an in-flight Code turn (local SSE + best-effort API cancel).
    async fn cancel_turn(&self) {}

    /// Live Code session id, if one has been created.
    async fn code_session_id(&self) -> Option<String> {
        None
    }

    /// Clone this client when the implementation shares session state via `Arc`.
    /// Used so the TUI can stream and cancel against the same Code session.
    fn clone_box(&self) -> Option<Box<dyn ModelClient>> {
        None
    }
}

/// Get the Cortex auth token from environment or keyring.
fn get_auth_token() -> Result<String> {
    // First check environment variable
    if let Ok(token) = std::env::var("CORTEX_AUTH_TOKEN") {
        return Ok(token);
    }

    // Try to load from cortex-login keyring storage
    if let Some(token) = cortex_login::get_auth_token() {
        return Ok(token);
    }

    // Not authenticated
    Err(CortexError::Auth(
        "Not authenticated. Run 'cortex login' first or set CORTEX_AUTH_TOKEN environment variable.".to_string()
    ))
}

/// Create a Cortex backend client.
///
/// All requests go through the Cortex backend with OAuth authentication.
///
/// # Named providers
/// `base_url` from `[providers.<id>]` is passed through as `_base_url`.
/// Auth still uses the Cortex session / `CORTEX_API_KEY`.
pub fn create_client(
    _provider_id: &str,
    model: &str,
    api_key: &str,
    _base_url: Option<&str>,
) -> Result<Box<dyn ModelClient>> {
    // Use provided api_key as auth token, or try to get from environment/keyring
    let auth_token = if !api_key.is_empty() {
        api_key.to_string()
    } else {
        get_auth_token()?
    };

    let resolved_url = _base_url
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .or_else(|| std::env::var("CORTEX_API_URL").ok());
    Ok(Box::new(
        CortexClient::new(model.to_string(), resolved_url).with_auth_token(auth_token),
    ))
}

/// Create a Cortex client with explicit auth token.
pub fn create_client_with_auth(model: &str, auth_token: &str) -> Box<dyn ModelClient> {
    let base_url = std::env::var("CORTEX_API_URL").ok();
    Box::new(CortexClient::new(model.to_string(), base_url).with_auth_token(auth_token.to_string()))
}

/// Create a Cortex client with custom base URL.
pub fn create_client_with_url(
    model: &str,
    auth_token: &str,
    base_url: &str,
) -> Box<dyn ModelClient> {
    Box::new(
        CortexClient::new(model.to_string(), Some(base_url.to_string()))
            .with_auth_token(auth_token.to_string()),
    )
}
