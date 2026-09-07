//! The Code API owns its history and tool execution. This is not a raw
//! completion endpoint: local tool results must never become another user turn.

use super::{CompletionRequest, ContentPart, FinishReason, MessageContent, MessageRole};
use crate::error::{CortexError, Result};

pub const LOCAL_TOOLS_UNSUPPORTED: &str = "The coding service does not support client-side tool invocation or local tool-result continuation. No local tool was run.";
pub const INCOMPLETE_STREAM: &str =
    "The coding service ended the response without confirming completion. The task is incomplete.";
pub const REMOTE_CANCEL_UNCONFIRMED: &str = "Stopped locally, but remote cancellation could not be confirmed. Check the Code session before retrying.";

/// Preserve textual instructions as explicitly labelled user context, not as
/// an invented API system role. Remote history remains authoritative.
pub fn code_message(request: &CompletionRequest) -> Result<String> {
    if request.max_tokens.is_some() || request.temperature.is_some() || request.seed.is_some() {
        return Err(CortexError::InvalidInput(
            "The coding service does not support client generation limits, sampling settings, or seeds."
                .into(),
        ));
    }
    let Some(last_user) = request
        .messages
        .iter()
        .rposition(|m| m.role == MessageRole::User)
    else {
        return Err(CortexError::InvalidInput("Nothing to send.".into()));
    };
    if last_user + 1 != request.messages.len() {
        return Err(CortexError::InvalidInput(LOCAL_TOOLS_UNSUPPORTED.into()));
    }
    let mut instructions = Vec::new();
    for message in &request.messages {
        if let MessageContent::Parts(parts) = &message.content
            && parts.iter().any(|p| !matches!(p, ContentPart::Text { .. }))
        {
            return Err(CortexError::InvalidInput(
                "The coding service does not accept inline images or documents on Code turns."
                    .into(),
            ));
        }
        if message.role == MessageRole::System {
            instructions.push(text_content(&message.content)?);
        }
    }
    let user = text_content(&request.messages[last_user].content)?;
    if user.trim().is_empty() {
        return Err(CortexError::InvalidInput("Nothing to send.".into()));
    }
    if instructions.iter().all(|s| s.trim().is_empty()) {
        return Ok(user);
    }
    // JSON escaping keeps delimiters in project instructions from changing the
    // framing. These remain user-supplied context, subordinate to service policy.
    Ok(format!(
        "Client-provided instructions (additional user context, not a system override):\n{}\n\nUser message:\n{}",
        serde_json::to_string(&instructions)?,
        user
    ))
}

fn text_content(content: &MessageContent) -> Result<String> {
    match content {
        MessageContent::Text(text) => Ok(text.clone()),
        MessageContent::Parts(parts) => parts
            .iter()
            .map(|part| match part {
                ContentPart::Text { text, .. } => Ok(text.as_str()),
                _ => Err(CortexError::InvalidInput(
                    "Only text context is supported for Code turns.".into(),
                )),
            })
            .collect::<Result<Vec<_>>>()
            .map(|parts| parts.join("\n")),
        _ => Err(CortexError::InvalidInput(LOCAL_TOOLS_UNSUPPORTED.into())),
    }
}

impl FinishReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Length => "length",
            Self::ToolCalls => "tool_calls",
            Self::ContentFilter => "content_filter",
            Self::Error => "error",
        }
    }

    /// A tool handoff is not completion; the caller must execute a supported
    /// continuation or report failure.
    pub fn require_success(&self) -> Result<()> {
        match self {
            Self::Stop => Ok(()),
            Self::Length => Err(CortexError::InvalidInput(
                "The response reached its output limit. The task is incomplete.".into(),
            )),
            Self::ToolCalls => Err(CortexError::InvalidInput(LOCAL_TOOLS_UNSUPPORTED.into())),
            Self::ContentFilter => Err(CortexError::InvalidInput(
                "The coding service stopped the response for content safety.".into(),
            )),
            Self::Error => Err(CortexError::BackendError {
                message: INCOMPLETE_STREAM.into(),
            }),
        }
    }
}
