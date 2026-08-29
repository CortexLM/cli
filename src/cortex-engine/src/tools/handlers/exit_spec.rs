//! ExitSpecMode — unlock mutate tools after a Plan / Spec review.

use async_trait::async_trait;
use serde_json::{Value, json};

use super::ToolHandler;
use crate::error::Result;
use crate::harness;
use crate::tools::context::ToolContext;
use crate::tools::spec::{ToolDefinition, ToolResult};

/// Marks Spec / Plan mode as ready for mutate tools.
pub struct ExitSpecModeHandler;

impl ExitSpecModeHandler {
    pub fn new() -> Self {
        Self
    }

    pub fn definition() -> ToolDefinition {
        ToolDefinition::new(
            "ExitSpecMode",
            "Leave Spec/Plan mode after the plan (including mermaid) is approved. \
             Required before any mutate tool (Write, Edit, ApplyPatch, Execute).",
            json!({
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "Why mutate access is needed now"
                    }
                },
                "additionalProperties": false
            }),
        )
    }
}

impl Default for ExitSpecModeHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for ExitSpecModeHandler {
    fn name(&self) -> &str {
        "ExitSpecMode"
    }

    async fn execute(&self, arguments: Value, _context: &ToolContext) -> Result<ToolResult> {
        harness::exit_spec_mode();
        let reason = arguments
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("plan approved");
        Ok(ToolResult::success(format!(
            "ExitSpecMode accepted ({reason}). Mutate tools are now allowed."
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn unlocks_mutate() {
        harness::enter_spec_mode();
        let handler = ExitSpecModeHandler::new();
        let ctx = ToolContext::new(PathBuf::from("."));
        let result = handler.execute(json!({}), &ctx).await.unwrap();
        assert!(result.success);
        assert!(harness::spec_mode_mutate_unlocked());
        harness::enter_spec_mode();
    }
}
