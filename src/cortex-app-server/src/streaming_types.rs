//! SSE compatibility payloads. Outer envelopes supply shared identity and sequence.
use cortex_protocol::{Event as CliEvent, EventMsg};
use serde::{Deserialize, Serialize};
#[derive(Debug, Deserialize)]
pub struct CreateCliSessionRequest {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CliSessionResponse {
    pub id: String,
    pub conversation_id: String,
    pub model: String,
    pub cwd: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ApproveRequest {
    pub call_id: String,
    pub approved: bool,
}

/// SSE event types sent to client.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Session is ready.
    SessionReady {
        session_id: String,
        model: String,
        cwd: String,
    },
    /// User message acknowledged.
    MessageReceived { id: String },
    /// Task started processing.
    TaskStarted,
    /// Streaming text delta.
    Delta { content: String },
    /// Full message (replaces deltas).
    Message { content: String },
    /// Reasoning/thinking delta.
    Reasoning { content: String },
    /// Tool call started.
    ToolStart {
        call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    /// Tool call output delta.
    ToolOutput { call_id: String, content: String },
    /// Tool call completed.
    ToolEnd {
        call_id: String,
        tool_name: String,
        output: String,
        success: bool,
        duration_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    /// Command requires approval.
    ApprovalRequired {
        call_id: String,
        command: Vec<String>,
        cwd: String,
    },
    /// Token usage update.
    TokenUsage {
        input_tokens: i64,
        output_tokens: i64,
        total_tokens: i64,
    },
    /// Task completed.
    TaskComplete { message: Option<String> },
    /// Warning message.
    Warning { message: String },
    /// Error occurred.
    Error { message: String },
    /// The engine confirmed local turn cancellation.
    Cancelled,
    /// Session closed.
    SessionClosed,
    /// Keep-alive ping.
    Ping { timestamp: u64 },
}

#[derive(Debug, Deserialize)]
pub struct ForkCliSessionRequest {
    pub message_index: usize,
}

/// Convert CLI event to stream event.
pub(super) fn convert_cli_event(event: &CliEvent) -> Option<StreamEvent> {
    match &event.msg {
        EventMsg::AgentMessageDelta(e) => Some(StreamEvent::Delta {
            content: e.delta.clone(),
        }),

        EventMsg::AgentMessage(e) => Some(StreamEvent::Message {
            content: e.message.clone(),
        }),

        EventMsg::AgentReasoningDelta(e) => Some(StreamEvent::Reasoning {
            content: e.delta.clone(),
        }),

        EventMsg::ExecCommandBegin(e) => Some(StreamEvent::ToolStart {
            call_id: e.call_id.clone(),
            tool_name: e.tool_name.clone().unwrap_or_else(|| "Execute".to_string()),
            arguments: e.tool_arguments.clone().unwrap_or_default(),
        }),

        EventMsg::ExecCommandOutputDelta(e) => {
            // Decode base64 output
            let content =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &e.chunk)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .unwrap_or_default();

            Some(StreamEvent::ToolOutput {
                call_id: e.call_id.clone(),
                content,
            })
        }

        EventMsg::ExecCommandEnd(e) => Some(StreamEvent::ToolEnd {
            call_id: e.call_id.clone(),
            tool_name: "Execute".to_string(),
            output: e.formatted_output.clone(),
            success: e.exit_code == 0,
            duration_ms: e.duration_ms,
            metadata: e.metadata.clone(),
        }),

        EventMsg::ExecApprovalRequest(e) => Some(StreamEvent::ApprovalRequired {
            call_id: e.call_id.clone(),
            command: e.command.clone(),
            cwd: e.cwd.to_string_lossy().to_string(),
        }),

        EventMsg::TurnAborted(_) => Some(StreamEvent::Cancelled),

        EventMsg::TaskStarted(_) => Some(StreamEvent::TaskStarted),

        EventMsg::TaskComplete(e) => Some(StreamEvent::TaskComplete {
            message: e.last_agent_message.clone(),
        }),

        EventMsg::TokenCount(e) => e.info.as_ref().map(|info| StreamEvent::TokenUsage {
            input_tokens: info.last_token_usage.input_tokens,
            output_tokens: info.last_token_usage.output_tokens,
            total_tokens: info.last_token_usage.total_tokens,
        }),

        EventMsg::Warning(e) => Some(StreamEvent::Warning {
            message: e.message.clone(),
        }),

        EventMsg::Error(_) => Some(StreamEvent::Error {
            message: "The coding service is temporarily unavailable".into(),
        }),

        EventMsg::ShutdownComplete => Some(StreamEvent::SessionClosed),

        EventMsg::SessionConfigured(e) => Some(StreamEvent::SessionReady {
            session_id: e.session_id.to_string(),
            model: e.model.clone(),
            cwd: e.cwd.to_string_lossy().to_string(),
        }),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_protocol::*;
    use std::path::PathBuf;

    fn event(msg: EventMsg) -> CliEvent {
        CliEvent {
            id: "engine-id".into(),
            msg,
        }
    }

    fn payload(msg: EventMsg) -> String {
        serde_json::to_string(&convert_cli_event(&event(msg)).expect("event must convert")).unwrap()
    }

    #[test]
    fn engine_events_map_onto_the_documented_sse_payloads() {
        assert_eq!(
            payload(EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
                delta: "partial".into(),
            })),
            r#"{"type":"delta","content":"partial"}"#
        );
        assert_eq!(
            payload(EventMsg::AgentMessage(AgentMessageEvent {
                id: None,
                parent_id: None,
                message: "final".into(),
                finish_reason: None,
            })),
            r#"{"type":"message","content":"final"}"#
        );
        assert_eq!(
            payload(EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
                delta: "thinking".into(),
            })),
            r#"{"type":"reasoning","content":"thinking"}"#
        );
        assert_eq!(
            payload(EventMsg::TaskStarted(TaskStartedEvent {
                model_context_window: None,
            })),
            r#"{"type":"task_started"}"#
        );
        assert_eq!(
            payload(EventMsg::TaskComplete(TaskCompleteEvent {
                last_agent_message: None,
            })),
            r#"{"type":"task_complete","message":null}"#
        );
        assert_eq!(
            payload(EventMsg::TurnAborted(TurnAbortedEvent {
                reason: TurnAbortReason::Interrupted,
            })),
            r#"{"type":"cancelled"}"#
        );
        assert_eq!(
            payload(EventMsg::ShutdownComplete),
            r#"{"type":"session_closed"}"#
        );
        assert_eq!(
            payload(EventMsg::Warning(WarningEvent {
                message: "heads up".into(),
            })),
            r#"{"type":"warning","message":"heads up"}"#
        );
    }

    #[test]
    fn engine_failures_are_reduced_to_the_product_facing_message() {
        let json = payload(EventMsg::Error(ErrorEvent {
            message: "connection refused to 127.0.0.1:1 (transport detail)".into(),
            cortex_error_info: None,
        }));
        assert_eq!(
            json,
            r#"{"type":"error","message":"The coding service is temporarily unavailable"}"#
        );
        assert!(!json.contains("127.0.0.1"));
        assert!(!json.contains("transport detail"));
    }

    #[test]
    fn tool_output_is_base64_decoded_and_undecodable_chunks_do_not_panic() {
        let decoded = payload(EventMsg::ExecCommandOutputDelta(
            ExecCommandOutputDeltaEvent {
                call_id: "call-1".into(),
                stream: ExecOutputStream::Stdout,
                chunk: "aGVsbG8gd29ybGQ=".into(),
            },
        ));
        assert_eq!(
            decoded,
            r#"{"type":"tool_output","call_id":"call-1","content":"hello world"}"#
        );
        // Malformed and non-UTF-8 payloads degrade to empty content rather than
        // panicking or leaking raw bytes.
        for chunk in ["not-valid-base64!!", "/w==", ""] {
            let json = payload(EventMsg::ExecCommandOutputDelta(
                ExecCommandOutputDeltaEvent {
                    call_id: "call-1".into(),
                    stream: ExecOutputStream::Stderr,
                    chunk: chunk.into(),
                },
            ));
            assert_eq!(
                json, r#"{"type":"tool_output","call_id":"call-1","content":""}"#,
                "{chunk}"
            );
        }
    }

    #[test]
    fn tool_lifecycle_reports_the_real_exit_status_and_named_tool() {
        let named = payload(EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: "call-1".into(),
            turn_id: "turn-1".into(),
            command: vec!["ls".into()],
            cwd: PathBuf::from("/workspace"),
            parsed_cmd: vec![],
            source: ExecCommandSource::default(),
            interaction_input: None,
            tool_name: Some("Read".into()),
            tool_arguments: Some(serde_json::json!({"path":"a.rs"})),
        }));
        assert_eq!(
            named,
            r#"{"type":"tool_start","call_id":"call-1","tool_name":"Read","arguments":{"path":"a.rs"}}"#
        );

        for (exit_code, expected) in [(0, true), (2, false)] {
            let end = convert_cli_event(&event(EventMsg::ExecCommandEnd(Box::new(
                ExecCommandEndEvent {
                    call_id: "call-1".into(),
                    turn_id: "turn-1".into(),
                    command: vec!["ls".into()],
                    cwd: PathBuf::from("/workspace"),
                    parsed_cmd: vec![],
                    source: ExecCommandSource::default(),
                    interaction_input: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    aggregated_output: String::new(),
                    exit_code,
                    duration_ms: 5,
                    formatted_output: "output".into(),
                    metadata: None,
                },
            ))))
            .unwrap();
            let StreamEvent::ToolEnd {
                success, metadata, ..
            } = end
            else {
                panic!("expected tool_end");
            };
            // A non-zero exit is never reported as a successful tool call.
            assert_eq!(success, expected, "exit {exit_code}");
            assert!(metadata.is_none());
        }
    }

    #[test]
    fn approvals_and_token_usage_are_reported_only_when_real() {
        let approval = payload(EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
            call_id: "call-9".into(),
            turn_id: "turn-1".into(),
            command: vec!["rm".into(), "-rf".into()],
            cwd: PathBuf::from("/workspace"),
            sandbox_assessment: None,
        }));
        // The exact command must survive so the user consents to what really runs.
        assert!(approval.contains(r#""command":["rm","-rf"]"#), "{approval}");

        // Absent usage info produces no event rather than a fabricated zero.
        assert!(
            convert_cli_event(&event(EventMsg::TokenCount(TokenCountEvent {
                info: None,
                rate_limits: None,
            })))
            .is_none()
        );
        let usage = TokenUsage {
            input_tokens: 11,
            cached_input_tokens: 0,
            output_tokens: 22,
            reasoning_output_tokens: 0,
            total_tokens: 33,
        };
        assert_eq!(
            payload(EventMsg::TokenCount(TokenCountEvent {
                info: Some(TokenUsageInfo {
                    total_token_usage: usage.clone(),
                    last_token_usage: usage,
                    model_context_window: None,
                    context_tokens: 0,
                }),
                rate_limits: None,
            })),
            r#"{"type":"token_usage","input_tokens":11,"output_tokens":22,"total_tokens":33}"#
        );
    }

    #[test]
    fn unmapped_engine_events_are_dropped_rather_than_guessed() {
        for msg in [
            EventMsg::Unknown,
            EventMsg::AgentReasoning(AgentReasoningEvent {
                text: "internal".into(),
            }),
            EventMsg::TurnDiff(TurnDiffEvent {
                unified_diff: "diff".into(),
            }),
        ] {
            assert!(convert_cli_event(&event(msg)).is_none());
        }
    }

    #[test]
    fn request_payloads_default_their_optional_fields() {
        let create: CreateCliSessionRequest = serde_json::from_str("{}").unwrap();
        assert!(create.model.is_none() && create.cwd.is_none() && create.provider.is_none());
        let fork: ForkCliSessionRequest = serde_json::from_str(r#"{"message_index":3}"#).unwrap();
        assert_eq!(fork.message_index, 3);
        // A chat turn without content is rejected at parse time, not defaulted.
        assert!(serde_json::from_str::<ChatRequest>("{}").is_err());
        assert!(serde_json::from_str::<ApproveRequest>(r#"{"call_id":"a"}"#).is_err());
    }
}
