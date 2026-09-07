//! Strict decoding for server-owned Code turn events.
use super::{
    AUTH_REQUIRED, CodeTurnEvent, ENDPOINT_NOT_FOUND, SERVICE_UNAVAILABLE, TOO_MANY_REQUESTS,
};
use crate::client::runtime_contract::{INCOMPLETE_STREAM, LOCAL_TOOLS_UNSUPPORTED};
use crate::client::{
    CompletionResponse, FinishReason, Message, MessageContent, MessageRole, ResponseEvent,
    TokenUsage, ToolCallEvent,
};
use crate::error::{CortexError, Result};
use crate::harness::redact_secrets;
use futures::StreamExt;
use std::time::Duration;
use tokio::{sync::mpsc, time::timeout};
const CHUNK_TIMEOUT_SECS: u64 = 60;

pub(super) async fn pump_sse<S>(stream: S, tx: mpsc::Sender<Result<ResponseEvent>>)
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
    let mut observations = std::collections::HashSet::new();
    let chunk_timeout = Duration::from_secs(CHUNK_TIMEOUT_SECS);

    loop {
        let event_result = match timeout(chunk_timeout, stream.next()).await {
            Ok(Some(result)) => result,
            Ok(None) => {
                let _ = tx
                    .send(Err(CortexError::BackendError {
                        message: INCOMPLETE_STREAM.into(),
                    }))
                    .await;
                break;
            }
            Err(_) => {
                let _ = tx.send(Err(CortexError::Timeout)).await;
                break;
            }
        };

        let event = match event_result {
            Ok(ev) => ev,
            Err(_) => {
                let _ = tx
                    .send(Err(CortexError::BackendError {
                        message: super::SERVICE_UNAVAILABLE.into(),
                    }))
                    .await;
                break;
            }
        };

        if event.data.is_empty() || event.data == "[DONE]" {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(&event.data) {
            Ok(value) => value,
            Err(_) => {
                let _ = tx
                    .send(Err(CortexError::BackendError {
                        message: INCOMPLETE_STREAM.into(),
                    }))
                    .await;
                break;
            }
        };
        if matches!(
            value["type"].as_str(),
            Some("local_tool_call" | "tool_call" | "question")
        ) || (value["type"] == "tool_start" && value["remote"] == false)
        {
            let _ = tx
                .send(Err(CortexError::InvalidInput(
                    LOCAL_TOOLS_UNSUPPORTED.into(),
                )))
                .await;
            break;
        }
        let parsed = match serde_json::from_value::<CodeTurnEvent>(value) {
            Ok(ev) => ev,
            Err(_) => {
                let _ = tx
                    .send(Err(CortexError::BackendError {
                        message: INCOMPLETE_STREAM.into(),
                    }))
                    .await;
                break;
            }
        };
        let terminal = matches!(
            parsed,
            CodeTurnEvent::Done { .. }
                | CodeTurnEvent::Cancelled { .. }
                | CodeTurnEvent::Error { .. }
        );
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
                if invocation_id.is_empty()
                    || tool_name.is_empty()
                    || !observations.insert(invocation_id.clone())
                {
                    let _ = tx
                        .send(Err(CortexError::BackendError {
                            message: INCOMPLETE_STREAM.into(),
                        }))
                        .await;
                    break;
                }
                let args = arguments
                    .filter(|v| !v.is_null())
                    .unwrap_or_else(|| serde_json::json!({"label": label}));
                Some(ResponseEvent::ToolCall(ToolCallEvent {
                    id: invocation_id,
                    name: tool_name,
                    arguments: args.to_string(),
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
                if !observations.remove(&invocation_id) {
                    let _ = tx
                        .send(Err(CortexError::BackendError {
                            message: INCOMPLETE_STREAM.into(),
                        }))
                        .await;
                    break;
                }
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
                if !observations.is_empty() {
                    let _ = tx
                        .send(Err(CortexError::BackendError {
                            message: INCOMPLETE_STREAM.into(),
                        }))
                        .await;
                    break;
                }
                let reason = match finish_reason.as_str() {
                    "stop" => FinishReason::Stop,
                    "length" | "max_tokens" => FinishReason::Length,
                    "tool_calls" => FinishReason::ToolCalls,
                    "content_filter" => FinishReason::ContentFilter,
                    "interrupted" | "cancelled" => {
                        let _ = tx.send(Err(CortexError::Cancelled)).await;
                        break;
                    }
                    _ => FinishReason::Error,
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
                    tool_calls: Vec::new(),
                }))
            }
            CodeTurnEvent::Cancelled { .. } => {
                let _ = tx.send(Err(CortexError::Cancelled)).await;
                break;
            }
            CodeTurnEvent::Error { message, detail } => {
                let raw = if message.is_empty() {
                    detail.unwrap_or_default()
                } else {
                    message
                };
                let mapped = map_sse_error_copy(&raw);
                Some(ResponseEvent::Error(mapped))
            }
            CodeTurnEvent::Question { .. } => {
                Some(ResponseEvent::Error(LOCAL_TOOLS_UNSUPPORTED.into()))
            }
            CodeTurnEvent::ReasoningDone { .. } | CodeTurnEvent::Unknown => None,
        };

        if let Some(ev) = mapped
            && tx.send(Ok(ev)).await.is_err()
        {
            break;
        }
        if terminal {
            break;
        }
    }
}

pub(super) fn map_sse_error_copy(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return SERVICE_UNAVAILABLE.to_string();
    }
    let lower = trimmed.to_lowercase();
    if lower.contains("unauthorized")
        || lower.contains("unauthenticated")
        || lower.contains("401")
        || lower.contains("403")
        || lower.contains("invalid api key")
        || lower.contains("invalid token")
    {
        return AUTH_REQUIRED.to_string();
    }
    if lower.contains("not found") || lower.contains("404") {
        return ENDPOINT_NOT_FOUND.to_string();
    }
    if lower.contains("429") || lower.contains("rate limit") || lower.contains("quota") {
        return TOO_MANY_REQUESTS.to_string();
    }
    if lower.contains("reqwest") || lower.contains("hyper") {
        return SERVICE_UNAVAILABLE.to_string();
    }
    super::sanitize_api_detail(Some(trimmed.to_string()))
}
