//! Observation-only adapter for the server-owned Code harness.

use std::collections::HashMap;
use std::time::Instant;

use cortex_protocol::{
    AgentMessageDeltaEvent, AgentMessageEvent, AgentReasoningDeltaEvent, EventMsg,
    ExecCommandBeginEvent, ExecCommandEndEvent, ExecCommandSource, TaskCompleteEvent,
    TokenCountEvent, TokenUsageInfo,
};
use tokio_stream::StreamExt;

use crate::client::runtime_contract::{INCOMPLETE_STREAM, LOCAL_TOOLS_UNSUPPORTED};
use crate::client::{CompletionRequest, Message, ResponseEvent};
use crate::error::{CortexError, Result};

use super::Session;

impl Session {
    pub(super) async fn run_remote_turn(&mut self) -> Result<()> {
        let request = CompletionRequest {
            model: self.config.model.clone(),
            messages: self.messages.clone(),
            ..Default::default()
        };
        let mut stream = self.client.complete(request).await?;
        let mut text = String::new();
        let mut observations = HashMap::new();
        while let Some(event) = stream.next().await {
            match event? {
                ResponseEvent::Delta(delta) => {
                    text.push_str(&delta);
                    self.emit(EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
                        delta,
                    }))
                    .await;
                }
                ResponseEvent::Reasoning(delta) => {
                    self.emit(EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
                        delta,
                    }))
                    .await;
                }
                ResponseEvent::ToolCall(call) => {
                    if !call.remote {
                        return Err(CortexError::InvalidInput(LOCAL_TOOLS_UNSUPPORTED.into()));
                    }
                    let command = vec!["[remote]".into(), call.name.clone()];
                    observations.insert(call.id.clone(), (command.clone(), Instant::now()));
                    self.emit(EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                        call_id: call.id,
                        turn_id: self.turn_id.to_string(),
                        command,
                        cwd: self.config.cwd.clone(),
                        parsed_cmd: vec![],
                        source: ExecCommandSource::Agent,
                        interaction_input: None,
                        tool_name: Some(call.name),
                        tool_arguments: serde_json::from_str(&call.arguments).ok(),
                    }))
                    .await;
                }
                ResponseEvent::ToolResult {
                    id,
                    success,
                    output,
                } => {
                    let (command, started) =
                        observations
                            .remove(&id)
                            .ok_or_else(|| CortexError::BackendError {
                                message: INCOMPLETE_STREAM.into(),
                            })?;
                    self.emit(EventMsg::ExecCommandEnd(Box::new(ExecCommandEndEvent {
                        call_id: id,
                        turn_id: self.turn_id.to_string(),
                        command,
                        cwd: self.config.cwd.clone(),
                        parsed_cmd: vec![],
                        source: ExecCommandSource::Agent,
                        interaction_input: None,
                        stdout: output.clone(),
                        stderr: String::new(),
                        aggregated_output: output.clone(),
                        exit_code: if success { 0 } else { 1 },
                        duration_ms: started.elapsed().as_millis() as u64,
                        formatted_output: output,
                        metadata: None,
                    })))
                    .await;
                }
                ResponseEvent::Error(message) => return Err(CortexError::BackendError { message }),
                ResponseEvent::Done(response) => {
                    self.emit(EventMsg::AgentMessage(AgentMessageEvent {
                        id: None,
                        parent_id: None,
                        message: text.clone(),
                        finish_reason: Some(response.finish_reason.as_str().into()),
                    }))
                    .await;
                    response.finish_reason.require_success()?;
                    if !observations.is_empty() {
                        return Err(CortexError::BackendError {
                            message: INCOMPLETE_STREAM.into(),
                        });
                    }
                    self.total_usage.input_tokens += response.usage.input_tokens;
                    self.total_usage.output_tokens += response.usage.output_tokens;
                    self.total_usage.total_tokens += response.usage.total_tokens;
                    self.emit(EventMsg::TokenCount(TokenCountEvent {
                        info: Some(TokenUsageInfo {
                            total_token_usage: self.total_usage.clone(),
                            last_token_usage: cortex_protocol::TokenUsage {
                                input_tokens: response.usage.input_tokens,
                                output_tokens: response.usage.output_tokens,
                                total_tokens: response.usage.total_tokens,
                                ..Default::default()
                            },
                            model_context_window: self.config.model_context_window,
                            context_tokens: response.usage.input_tokens,
                        }),
                        rate_limits: None,
                    }))
                    .await;
                    self.messages.push(Message::assistant(&text));
                    self.emit(EventMsg::TaskComplete(TaskCompleteEvent {
                        last_agent_message: Some(text),
                    }))
                    .await;
                    return Ok(());
                }
            }
        }
        Err(CortexError::BackendError {
            message: INCOMPLETE_STREAM.into(),
        })
    }
}
