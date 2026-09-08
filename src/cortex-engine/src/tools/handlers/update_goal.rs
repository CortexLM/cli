//! UpdateGoal tool — evidence-based progress on a persisted session goal.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::ToolHandler;
use crate::error::{CortexError, Result};
use crate::goal::{GoalEvidence, apply_model_update, load_goal, save_goal};
use crate::tools::context::ToolContext;
use crate::tools::spec::{ToolMetadata, ToolResult};

#[derive(Debug, Deserialize)]
struct UpdateGoalArgs {
    status: String,
    #[serde(default)]
    progress: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    evidence: Vec<EvidenceInput>,
}

#[derive(Debug, Deserialize)]
struct EvidenceInput {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    detail: String,
}

/// Model-facing goal progress handler.
pub struct UpdateGoalHandler;

impl UpdateGoalHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UpdateGoalHandler {
    fn default() -> Self {
        Self::new()
    }
}

fn session_dir(context: &ToolContext) -> Result<std::path::PathBuf> {
    if let Some(dir) = &context.session_dir {
        return Ok(dir.clone());
    }
    if context.conversation_id.is_empty() {
        return Err(CortexError::ToolExecution {
            tool: "UpdateGoal".to_string(),
            message: "No session directory for this goal.".to_string(),
        });
    }
    let home = crate::config::find_cortex_home().map_err(|e| CortexError::ToolExecution {
        tool: "UpdateGoal".to_string(),
        message: format!("Cannot resolve session home: {e}"),
    })?;
    Ok(home.join("sessions").join(&context.conversation_id))
}

#[async_trait]
impl ToolHandler for UpdateGoalHandler {
    fn name(&self) -> &str {
        "UpdateGoal"
    }

    async fn execute(&self, arguments: Value, context: &ToolContext) -> Result<ToolResult> {
        let args: UpdateGoalArgs =
            serde_json::from_value(arguments).map_err(|e| CortexError::ToolExecution {
                tool: "UpdateGoal".to_string(),
                message: format!("Failed to parse UpdateGoal arguments: {e}"),
            })?;

        let dir = session_dir(context)?;
        let mut goal = load_goal(&dir)?.ok_or_else(|| CortexError::ToolExecution {
            tool: "UpdateGoal".to_string(),
            message: "No durable goal. The user starts one with /goal <objective>.".to_string(),
        })?;

        let evidence = args
            .evidence
            .into_iter()
            .map(|e| GoalEvidence::new(e.kind, e.detail))
            .collect();

        apply_model_update(
            &mut goal,
            &args.status,
            args.progress,
            args.reason,
            evidence,
        )
        .map_err(|message| CortexError::ToolExecution {
            tool: "UpdateGoal".to_string(),
            message,
        })?;

        save_goal(&dir, &goal)?;

        let output = format!(
            "Goal updated.\n  state: {}\n  progress: {}\n  turns: {} / {}\n  evidence: {}",
            goal.state,
            goal.progress.as_deref().unwrap_or("(none)"),
            goal.turns_used,
            goal.turn_budget,
            goal.evidence.len()
        );

        Ok(ToolResult::success(output).with_metadata(ToolMetadata {
            duration_ms: 0,
            exit_code: None,
            files_modified: vec![],
            data: Some(json!({
                "type": "goal",
                "id": goal.id,
                "state": goal.state.as_str(),
                "progress": goal.progress,
                "turns_used": goal.turns_used,
                "turn_budget": goal.turn_budget,
                "chip": goal.chip(),
            })),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal::{Goal, save_goal};

    #[tokio::test]
    async fn complete_requires_evidence() {
        let dir = tempfile::tempdir().unwrap();
        save_goal(dir.path(), &Goal::new("write out.txt")).unwrap();
        let ctx =
            ToolContext::new(dir.path().to_path_buf()).with_session_dir(dir.path().to_path_buf());
        let handler = UpdateGoalHandler::new();
        let err = handler
            .execute(
                json!({
                    "status": "complete",
                    "reason": "looks good"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("evidence"));
    }

    #[tokio::test]
    async fn records_progress_and_evidence() {
        let dir = tempfile::tempdir().unwrap();
        save_goal(dir.path(), &Goal::new("write out.txt")).unwrap();
        let ctx =
            ToolContext::new(dir.path().to_path_buf()).with_session_dir(dir.path().to_path_buf());
        let handler = UpdateGoalHandler::new();
        let result = handler
            .execute(
                json!({
                    "status": "complete",
                    "progress": "created out.txt",
                    "reason": "file exists",
                    "evidence": [{"kind": "file", "detail": "out.txt"}]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("complete"));
        let loaded = load_goal(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.state, crate::goal::GoalState::Complete);
    }

    #[tokio::test]
    async fn model_cannot_pause() {
        let dir = tempfile::tempdir().unwrap();
        save_goal(dir.path(), &Goal::new("x")).unwrap();
        let ctx =
            ToolContext::new(dir.path().to_path_buf()).with_session_dir(dir.path().to_path_buf());
        let err = UpdateGoalHandler::new()
            .execute(json!({"status": "paused"}), &ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("cannot pause"));
    }

    #[tokio::test]
    async fn missing_goal_errors() {
        let dir = tempfile::tempdir().unwrap();
        let ctx =
            ToolContext::new(dir.path().to_path_buf()).with_session_dir(dir.path().to_path_buf());
        let err = UpdateGoalHandler::new()
            .execute(json!({"status": "active", "progress": "hi"}), &ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("/goal"));
    }
}
