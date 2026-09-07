//! Conversion of engine events for legacy WebSocket clients.
use crate::websocket::WsMessage;
use cortex_protocol::{Event, EventMsg};
use tracing::debug;
/// Convert a cortex-protocol Event to a WebSocket message.
pub(super) fn convert_event_to_ws(event: &Event) -> Option<WsMessage> {
    match &event.msg {
        // Streaming content
        EventMsg::AgentMessageDelta(e) => Some(WsMessage::StreamChunk {
            content: e.delta.clone(),
        }),

        // Full message (end of stream)
        EventMsg::AgentMessage(e) => Some(WsMessage::AgentMessage {
            content: e.message.clone(),
        }),

        // User message echo
        EventMsg::UserMessage(e) => Some(WsMessage::MessageReceived {
            id: event.id.clone(),
            role: "user".to_string(),
            content: e.message.clone(),
        }),

        // Tool execution start
        EventMsg::ExecCommandBegin(e) => Some(WsMessage::ToolCallBegin {
            call_id: e.call_id.clone(),
            tool_name: e.tool_name.clone().unwrap_or_else(|| "Execute".to_string()),
            arguments: e.tool_arguments.clone().unwrap_or_default(),
        }),

        // Tool execution end
        EventMsg::ExecCommandEnd(e) => Some(WsMessage::ToolCallEnd {
            call_id: e.call_id.clone(),
            tool_name: "Execute".to_string(),
            output: e.formatted_output.clone(),
            success: e.exit_code == 0,
            duration_ms: e.duration_ms,
            metadata: e.metadata.clone(),
        }),

        // Tool execution output chunk (streaming)
        EventMsg::ExecCommandOutputDelta(e) => {
            let stream = match e.stream {
                cortex_protocol::ExecOutputStream::Stdout => "stdout",
                cortex_protocol::ExecOutputStream::Stderr => "stderr",
            };
            Some(WsMessage::ToolCallOutputDelta {
                call_id: e.call_id.clone(),
                stream: stream.to_string(),
                chunk: e.chunk.clone(),
            })
        }

        // Approval request
        EventMsg::ExecApprovalRequest(e) => Some(WsMessage::ApprovalRequest {
            call_id: e.call_id.clone(),
            command: e.command.clone(),
            cwd: e.cwd.to_string_lossy().to_string(),
        }),

        // Task lifecycle
        EventMsg::TaskStarted(_) => Some(WsMessage::TaskStarted),

        EventMsg::TaskComplete(e) => Some(WsMessage::TaskComplete {
            message: e.last_agent_message.clone(),
        }),

        // Token usage
        EventMsg::TokenCount(e) => e.info.as_ref().map(|info| WsMessage::TokenUsage {
            input_tokens: info.last_token_usage.input_tokens as u32,
            output_tokens: info.last_token_usage.output_tokens as u32,
            total_tokens: info.last_token_usage.total_tokens as u32,
        }),

        // Errors
        EventMsg::Error(_) => Some(WsMessage::Error {
            code: "error".to_string(),
            message: "The coding service is temporarily unavailable".to_string(),
        }),

        EventMsg::Warning(e) => Some(WsMessage::Warning {
            message: e.message.clone(),
        }),

        // Session configured
        EventMsg::SessionConfigured(e) => Some(WsMessage::SessionConfigured {
            session_id: e.session_id.to_string(),
            model: e.model.clone(),
            cwd: e.cwd.to_string_lossy().to_string(),
        }),

        // Reasoning (thinking)
        EventMsg::AgentReasoningDelta(e) => Some(WsMessage::ReasoningDelta {
            delta: e.delta.clone(),
        }),

        // Turn aborted/cancelled
        EventMsg::TurnAborted(_) => Some(WsMessage::Cancelled),

        // Shutdown
        EventMsg::ShutdownComplete => Some(WsMessage::SessionClosed),

        // Other events we don't forward yet
        _ => {
            debug!(
                "Unhandled event type: {:?}",
                std::any::type_name::<EventMsg>()
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_protocol::*;
    use std::path::PathBuf;

    fn event(msg: EventMsg) -> Event {
        Event {
            id: "engine-id".into(),
            msg,
        }
    }

    #[test]
    fn engine_events_map_onto_the_legacy_websocket_shapes() {
        let cases = [
            (
                EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
                    delta: "partial".into(),
                }),
                r#"{"type":"stream_chunk","content":"partial"}"#,
            ),
            (
                EventMsg::AgentMessage(AgentMessageEvent {
                    id: None,
                    parent_id: None,
                    message: "final".into(),
                    finish_reason: None,
                }),
                r#"{"type":"agent_message","content":"final"}"#,
            ),
            (
                EventMsg::UserMessage(UserMessageEvent {
                    id: None,
                    parent_id: None,
                    message: "asked".into(),
                    images: None,
                }),
                r#"{"type":"message_received","id":"engine-id","role":"user","content":"asked"}"#,
            ),
            (
                EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
                    delta: "thinking".into(),
                }),
                r#"{"type":"reasoning_delta","delta":"thinking"}"#,
            ),
            (
                EventMsg::TaskStarted(TaskStartedEvent {
                    model_context_window: None,
                }),
                r#"{"type":"task_started"}"#,
            ),
            (
                EventMsg::TaskComplete(TaskCompleteEvent {
                    last_agent_message: Some("done".into()),
                }),
                r#"{"type":"task_complete","message":"done"}"#,
            ),
            (
                EventMsg::TurnAborted(TurnAbortedEvent {
                    reason: TurnAbortReason::Interrupted,
                }),
                r#"{"type":"cancelled"}"#,
            ),
            (EventMsg::ShutdownComplete, r#"{"type":"session_closed"}"#),
            (
                EventMsg::Warning(WarningEvent {
                    message: "heads up".into(),
                }),
                r#"{"type":"warning","message":"heads up"}"#,
            ),
        ];
        for (msg, expected) in cases {
            let converted = convert_event_to_ws(&event(msg)).expect("event must convert");
            assert_eq!(serde_json::to_string(&converted).unwrap(), expected);
        }
    }

    #[test]
    fn engine_failures_are_reduced_to_the_product_facing_message() {
        // The raw engine text must never reach a WebSocket client.
        let converted = convert_event_to_ws(&event(EventMsg::Error(ErrorEvent {
            message: "connection refused to 127.0.0.1:1 (transport detail)".into(),
            cortex_error_info: None,
        })))
        .unwrap();
        let json = serde_json::to_string(&converted).unwrap();
        assert_eq!(
            json,
            r#"{"type":"error","code":"error","message":"The coding service is temporarily unavailable"}"#
        );
        assert!(!json.contains("127.0.0.1"));
        assert!(!json.contains("transport detail"));
    }

    #[test]
    fn tool_lifecycle_events_carry_call_identity_and_real_exit_status() {
        let begin =
            convert_event_to_ws(&event(EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id: "call-1".into(),
                turn_id: "turn-1".into(),
                command: vec!["ls".into()],
                cwd: PathBuf::from("/workspace"),
                parsed_cmd: vec![],
                source: ExecCommandSource::default(),
                interaction_input: None,
                tool_name: None,
                tool_arguments: None,
            })))
            .unwrap();
        // A missing tool name falls back to the generic executor, not to nothing.
        assert_eq!(
            serde_json::to_string(&begin).unwrap(),
            r#"{"type":"tool_call_begin","call_id":"call-1","tool_name":"Execute","arguments":null}"#
        );

        for (exit_code, success) in [(0, true), (1, false)] {
            let end = convert_event_to_ws(&event(EventMsg::ExecCommandEnd(Box::new(
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
                    duration_ms: 7,
                    formatted_output: "output".into(),
                    metadata: None,
                },
            ))))
            .unwrap();
            let WsMessage::ToolCallEnd {
                call_id,
                success: reported,
                duration_ms,
                ..
            } = end
            else {
                panic!("expected tool_call_end");
            };
            // Success is derived from the real exit code, never assumed.
            assert_eq!(reported, success, "exit {exit_code}");
            assert_eq!(call_id, "call-1");
            assert_eq!(duration_ms, 7);
        }

        for (stream, label) in [
            (ExecOutputStream::Stdout, "stdout"),
            (ExecOutputStream::Stderr, "stderr"),
        ] {
            let delta = convert_event_to_ws(&event(EventMsg::ExecCommandOutputDelta(
                ExecCommandOutputDeltaEvent {
                    call_id: "call-1".into(),
                    stream,
                    chunk: "Y2h1bms=".into(),
                },
            )))
            .unwrap();
            let WsMessage::ToolCallOutputDelta {
                stream: reported, ..
            } = delta
            else {
                panic!("expected tool_call_output_delta");
            };
            assert_eq!(reported, label);
        }
    }

    #[test]
    fn approval_requests_and_token_counts_preserve_their_payloads() {
        let approval = convert_event_to_ws(&event(EventMsg::ExecApprovalRequest(
            ExecApprovalRequestEvent {
                call_id: "call-9".into(),
                turn_id: "turn-1".into(),
                command: vec!["rm".into(), "-rf".into()],
                cwd: PathBuf::from("/workspace"),
                sandbox_assessment: None,
            },
        )))
        .unwrap();
        let WsMessage::ApprovalRequest {
            call_id, command, ..
        } = approval
        else {
            panic!("expected approval_request");
        };
        // The command must reach the user verbatim so consent is informed.
        assert_eq!(call_id, "call-9");
        assert_eq!(command, vec!["rm".to_string(), "-rf".to_string()]);

        // A token count with no usage info yields no fabricated zero usage.
        assert!(
            convert_event_to_ws(&event(EventMsg::TokenCount(TokenCountEvent {
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
        let counted = convert_event_to_ws(&event(EventMsg::TokenCount(TokenCountEvent {
            info: Some(TokenUsageInfo {
                total_token_usage: usage.clone(),
                last_token_usage: usage,
                model_context_window: None,
                context_tokens: 0,
            }),
            rate_limits: None,
        })))
        .unwrap();
        assert_eq!(
            serde_json::to_string(&counted).unwrap(),
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
            assert!(convert_event_to_ws(&event(msg)).is_none());
        }
    }
}
