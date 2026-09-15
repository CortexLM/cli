//! Helpers for one subagent run: event forwarding, turn setup, and result assembly.
//!
//! Kept beside `executor.rs` so the executor stays under the file-size cap while
//! `run_subagent` stays under the complexity cap.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::agent::{AgentEvent, Orchestrator, OrchestratorTurnResult, TurnStatus};
use crate::agents::Agent;
use crate::error::Result;

use super::progress::ProgressEvent;
use super::result::{
    FileChange, FileChangeType, SubagentResult, SubagentResultBuilder, TokenUsageBreakdown,
};
use super::types::{SubagentConfig, SubagentSession};

/// Forward agent events to the progress channel, collecting modified files.
///
/// Returns the modified-file list once the event channel closes. The caller
/// must drop the orchestrator so the channel closes and this task can finish.
pub(super) fn spawn_event_forwarder(
    progress_tx: mpsc::UnboundedSender<ProgressEvent>,
    session_id: String,
    mut event_rx: mpsc::UnboundedReceiver<AgentEvent>,
) -> tokio::task::JoinHandle<Vec<String>> {
    tokio::spawn(async move {
        let mut files_modified = Vec::new();
        let mut turn_number: u32 = 0; // Track actual turn number
        while let Some(event) = event_rx.recv().await {
            match &event {
                AgentEvent::Thinking => {
                    // Increment turn number each time model is called
                    turn_number += 1;
                    let _ = progress_tx.send(ProgressEvent::Thinking {
                        session_id: session_id.clone(),
                        turn_number,
                    });
                }
                AgentEvent::TextDelta { content } => {
                    let _ = progress_tx.send(ProgressEvent::TextOutput {
                        session_id: session_id.clone(),
                        content: content.clone(),
                        is_partial: true,
                    });
                }
                AgentEvent::ToolCallStarted {
                    id,
                    name,
                    arguments,
                } => {
                    let _ = progress_tx.send(ProgressEvent::ToolCallStarted {
                        session_id: session_id.clone(),
                        tool_name: name.clone(),
                        tool_id: id.clone(),
                        arguments_preview: arguments.chars().take(200).collect(),
                    });
                }
                AgentEvent::ToolCallCompleted { id, name, result } => {
                    // Track file modifications
                    if matches!(
                        name.as_str(),
                        "Create" | "Edit" | "ApplyPatch" | "MultiEdit"
                    ) {
                        // Extract file path from result if possible
                        if let Some(path) = extract_file_path(&result.output) {
                            files_modified.push(path);
                        }
                    }

                    let _ = progress_tx.send(ProgressEvent::ToolCallCompleted {
                        session_id: session_id.clone(),
                        tool_name: name.clone(),
                        tool_id: id.clone(),
                        success: result.success,
                        output_preview: result.output.chars().take(200).collect(),
                        duration_ms: 0, // Not tracked at event level
                    });
                }
                AgentEvent::ToolCallPending {
                    id,
                    name,
                    arguments,
                    risk_level,
                } => {
                    let _ = progress_tx.send(ProgressEvent::ToolCallPending {
                        session_id: session_id.clone(),
                        tool_name: name.clone(),
                        tool_id: id.clone(),
                        arguments: arguments.clone(),
                        risk_level: format!("{:?}", risk_level),
                    });
                }
                AgentEvent::Error {
                    message,
                    recoverable,
                } => {
                    if *recoverable {
                        let _ = progress_tx.send(ProgressEvent::Warning {
                            session_id: session_id.clone(),
                            message: message.clone(),
                        });
                    } else {
                        let _ = progress_tx.send(ProgressEvent::Failed {
                            session_id: session_id.clone(),
                            error: message.clone(),
                            recoverable: false,
                        });
                    }
                }
                _ => {}
            }
        }
        files_modified
    })
}

/// Model for this run: the custom agent's choice wins over the config.
pub(super) fn effective_model(
    custom_agent: Option<&Agent>,
    config: &SubagentConfig,
    default_model: &str,
) -> String {
    match custom_agent {
        Some(agent) => agent.effective_model(default_model),
        None => config
            .model
            .clone()
            .unwrap_or_else(|| default_model.to_string()),
    }
}

/// Iteration cap for this run: the custom agent's `max_turns` wins.
pub(super) fn effective_max_iterations(
    custom_agent: Option<&Agent>,
    config: &SubagentConfig,
) -> u32 {
    match custom_agent {
        Some(agent) => agent
            .metadata
            .max_turns
            .unwrap_or_else(|| config.effective_max_iterations()),
        None => config.effective_max_iterations(),
    }
}

/// True when a completed turn produced no structured summary.
pub(super) fn needs_summary_turn(turn_result: &Result<OrchestratorTurnResult>) -> bool {
    match turn_result {
        Ok(result) => {
            result.status == TurnStatus::Completed && !has_summary_output(&result.response)
        }
        Err(_) => false,
    }
}

/// Ask the subagent for an explicit summary and fold it into `turn_result`.
pub(super) async fn apply_summary_turn(
    orchestrator: &Orchestrator,
    session: &SubagentSession,
    session_id: &str,
    config: &SubagentConfig,
    turn_ctx: &mut crate::agent::TurnContext,
    turn_result: &mut Result<OrchestratorTurnResult>,
) {
    let Ok(result) = turn_result else {
        return;
    };
    let original = result.clone();

    // Request a summary turn
    let summary_turn_id = session.turns_completed as u64 + 2;
    let mut summary_turn_ctx = crate::agent::TurnContext::new(
        summary_turn_id,
        session_id.to_string(),
        SUMMARY_REQUEST_PROMPT.to_string(),
        config.working_dir.clone(),
    );

    // Execute summary turn with a reasonable timeout
    let summary_result = timeout(
        Duration::from_secs(60),
        orchestrator.process_turn(&mut summary_turn_ctx),
    )
    .await;

    // Update turn_result with the summary if successful
    let Ok(Ok(summary_response)) = summary_result else {
        return;
    };
    if summary_response.status != TurnStatus::Completed || summary_response.response.is_empty() {
        return;
    }

    tracing::info!(
        session_id = %session_id,
        "Received explicit summary from subagent"
    );
    // Combine original response with summary
    *turn_result = Ok(OrchestratorTurnResult {
        turn_id: original.turn_id,
        status: TurnStatus::Completed,
        response: format!("{}\n\n{}", original.response, summary_response.response),
        tool_calls: original.tool_calls.clone(),
        token_usage: original.token_usage.clone(),
        duration: original.duration,
    });
    // Update token counts
    turn_ctx.tokens.input_tokens += summary_turn_ctx.tokens.input_tokens;
    turn_ctx.tokens.output_tokens += summary_turn_ctx.tokens.output_tokens;
    turn_ctx.tokens.cached_tokens += summary_turn_ctx.tokens.cached_tokens;
    turn_ctx.tokens.reasoning_tokens += summary_turn_ctx.tokens.reasoning_tokens;
}

/// Terminal state of a run, derived from the turn result.
pub(super) struct TurnOutcome {
    pub(super) success: bool,
    pub(super) output: String,
    pub(super) error: Option<String>,
    pub(super) status_info: Option<String>,
}

impl TurnOutcome {
    /// Extract status for better error messages when success=false but no error.
    pub(super) fn from_result(turn_result: &Result<OrchestratorTurnResult>) -> Self {
        match turn_result {
            Ok(result) => {
                let success = result.status == TurnStatus::Completed;
                Self {
                    success,
                    output: result.response.clone(),
                    error: None,
                    status_info: (!success).then(|| format!("{:?}", result.status)),
                }
            }
            Err(e) => Self {
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
                status_info: None,
            },
        }
    }

    /// Message for a failure that carried no explicit error (interrupted,
    /// cancelled, or a turn that ended with a non-completed status).
    pub(super) fn failure_message(&self) -> String {
        match &self.status_info {
            Some(status) => format!("Task ended with status: {status}"),
            None => "Task did not complete successfully".to_string(),
        }
    }
}

/// Assemble the result the orchestrator sees.
pub(super) fn build_result(
    session: SubagentSession,
    outcome: TurnOutcome,
    files_modified: &[String],
    turn_ctx: &crate::agent::TurnContext,
    config: &SubagentConfig,
) -> SubagentResult {
    // Build token usage breakdown
    let mut token_usage = TokenUsageBreakdown::default();
    token_usage.add_turn(
        turn_ctx.tokens.input_tokens as u64,
        turn_ctx.tokens.output_tokens as u64,
        turn_ctx.tokens.cached_tokens as u64,
        turn_ctx.tokens.reasoning_tokens as u64,
    );

    // Build result
    let mut builder = SubagentResultBuilder::new(session)
        .success(outcome.success)
        .output(&outcome.output)
        .tokens(token_usage);

    // Add error to result - either from explicit error or from status_info
    if let Some(err) = outcome.error {
        builder = builder.error(err);
    } else if let Some(ref status) = outcome.status_info {
        builder = builder.error(format!("Task ended with status: {}", status));
    }

    for change in files_modified {
        builder = builder.file_changed(FileChange::new(change.clone(), FileChangeType::Modified));
    }

    // Allow continuation if partially complete
    if outcome.success || turn_ctx.tool_iterations < config.effective_max_iterations() {
        builder = builder.continuable();
    }

    builder.build()
}

/// Prompt used to request an explicit summary from a subagent when none was provided.
/// Ensures structured output from agents for orchestrator consumption.
const SUMMARY_REQUEST_PROMPT: &str = r#"You have completed your work but did not provide a summary. Please provide a final summary NOW using EXACTLY this format:

## Summary for Orchestrator

### Tasks Completed
- [List each task you completed with brief outcome]

### Key Findings/Changes
- [Main discoveries or modifications made]

### Files Modified (if any)
- [List of files with type of change]

### Recommendations (if applicable)
- [Any follow-up actions or suggestions]

### Status: COMPLETED

DO NOT use any tools. Just provide the summary based on the work you have already done."#;

/// Check if the response contains a proper summary for the orchestrator.
/// Returns true if summary markers are present, false otherwise.
pub(super) fn has_summary_output(response: &str) -> bool {
    // Empty responses definitely don't have a summary
    if response.trim().is_empty() {
        return false;
    }

    // Check for key summary markers that indicate structured output
    let summary_markers = [
        "## Summary for Orchestrator",
        "### Tasks Completed",
        "### Key Findings",
        "### Status: COMPLETED",
        "Status: COMPLETED",
        // Also accept some variations
        "## Summary",
        "### Summary",
        "## Final Summary",
        "### Final Summary",
    ];

    let response_lower = response.to_lowercase();
    summary_markers
        .iter()
        .any(|marker| response_lower.contains(&marker.to_lowercase()))
}

/// Extract file path from tool output.
pub(super) fn extract_file_path(output: &str) -> Option<String> {
    // Try to extract path from common patterns
    // "Created file: path/to/file"
    // "Edited path/to/file"
    // "Wrote N bytes to path/to/file"

    let patterns = [
        "Created file: ",
        "Created: ",
        "Edited ",
        "Modified ",
        "Wrote ",
        "to ",
    ];

    for pattern in patterns {
        if let Some(idx) = output.find(pattern) {
            let rest = &output[idx + pattern.len()..];
            // Extract until whitespace or end
            let path: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
            if !path.is_empty() && (path.contains('/') || path.contains('\\') || path.contains('.'))
            {
                return Some(path);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_file_path() {
        assert_eq!(
            extract_file_path("Created file: src/main.rs"),
            Some("src/main.rs".to_string())
        );
        assert_eq!(
            extract_file_path("Wrote 100 bytes to config.json"),
            Some("config.json".to_string())
        );
        assert_eq!(
            extract_file_path("Successfully edited src/lib.rs"),
            None // "edited" doesn't match "Edited "
        );
        assert_eq!(extract_file_path("No path here"), None);
    }

    #[test]
    fn test_has_summary_output_with_proper_summary() {
        let response_with_summary = r#"
## Summary for Orchestrator

### Tasks Completed
- Analyzed the codebase structure

### Key Findings
- Found 10 modules

### Status: COMPLETED
"#;
        assert!(has_summary_output(response_with_summary));
    }

    #[test]
    fn test_has_summary_output_with_variation() {
        // Test case-insensitive matching
        let response = "## summary\nSome content here";
        assert!(has_summary_output(response));

        // Test "Status: COMPLETED" alone
        let response2 = "Work done.\n\nStatus: COMPLETED";
        assert!(has_summary_output(response2));
    }

    #[test]
    fn test_has_summary_output_empty() {
        assert!(!has_summary_output(""));
        assert!(!has_summary_output("   "));
        assert!(!has_summary_output("\n\n"));
    }

    #[test]
    fn test_has_summary_output_no_markers() {
        let response_without_summary = "I analyzed the code and found some issues.";
        assert!(!has_summary_output(response_without_summary));

        let response_partial = "Here are some findings:\n- Item 1\n- Item 2";
        assert!(!has_summary_output(response_partial));
    }

    #[test]
    fn a_missing_summary_is_requested_only_for_completed_turns() {
        use crate::error::CortexError;

        let completed_without_summary = Ok(OrchestratorTurnResult {
            turn_id: 1,
            response: "did the work".into(),
            tool_calls: Vec::new(),
            token_usage: Default::default(),
            duration: Duration::ZERO,
            status: TurnStatus::Completed,
        });
        assert!(needs_summary_turn(&completed_without_summary));

        let completed_with_summary = Ok(OrchestratorTurnResult {
            turn_id: 1,
            response: "## Summary\n- done".into(),
            tool_calls: Vec::new(),
            token_usage: Default::default(),
            duration: Duration::ZERO,
            status: TurnStatus::Completed,
        });
        assert!(!needs_summary_turn(&completed_with_summary));

        let interrupted = Ok(OrchestratorTurnResult {
            turn_id: 1,
            response: "stopped early".into(),
            tool_calls: Vec::new(),
            token_usage: Default::default(),
            duration: Duration::ZERO,
            status: TurnStatus::Interrupted,
        });
        assert!(!needs_summary_turn(&interrupted));

        let failed = Err(CortexError::Timeout);
        assert!(!needs_summary_turn(&failed));
    }

    #[test]
    fn outcome_reports_explicit_and_derived_failures() {
        let ok_completed = Ok(OrchestratorTurnResult {
            turn_id: 7,
            response: "all good".into(),
            tool_calls: Vec::new(),
            token_usage: Default::default(),
            duration: Duration::ZERO,
            status: TurnStatus::Completed,
        });
        let outcome = TurnOutcome::from_result(&ok_completed);
        assert!(outcome.success);
        assert_eq!(outcome.output, "all good");
        assert!(outcome.error.is_none());
        assert!(outcome.status_info.is_none());

        // A non-completed status is a failure with no explicit error.
        let ok_interrupted = Ok(OrchestratorTurnResult {
            turn_id: 7,
            response: "partial".into(),
            tool_calls: Vec::new(),
            token_usage: Default::default(),
            duration: Duration::ZERO,
            status: TurnStatus::Interrupted,
        });
        let outcome = TurnOutcome::from_result(&ok_interrupted);
        assert!(!outcome.success);
        assert!(outcome.error.is_none());
        assert!(outcome.failure_message().contains("Interrupted"));
        assert!(outcome.status_info.is_some());

        // A hard error keeps its own message.
        let failed: Result<OrchestratorTurnResult> = Err(crate::error::CortexError::Timeout);
        let outcome = TurnOutcome::from_result(&failed);
        assert!(!outcome.success);
        assert!(outcome.output.is_empty());
        assert!(outcome.error.is_some());
        assert!(!outcome.failure_message().is_empty());
    }

    #[test]
    fn a_custom_agent_task_message_carries_context() {
        let mut config = SubagentConfig::new(
            crate::tools::handlers::subagent::SubagentType::Code,
            "review the parser",
            "check the tokenizer",
            std::path::PathBuf::from("/tmp"),
        );
        config.context = Some("the parser is in src/parse.rs".into());

        let message = config.build_user_message();
        assert!(message.contains("check the tokenizer"), "{message}");
        assert!(message.contains("Additional Context"), "{message}");
        assert!(
            message.contains("the parser is in src/parse.rs"),
            "{message}"
        );
    }
}
