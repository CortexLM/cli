//! Router compatibility API over the shared, context-enforcing registry.
use super::context::ToolContext;
use super::handlers::batch::BatchToolExecutor;
use super::registry::ToolRegistry;
use super::spec::{ToolDefinition, ToolHandler, ToolResult};
use crate::error::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

pub struct ToolRouter {
    registry: ToolRegistry,
}

/// Batch shares the same handlers and state, not a weaker reconstructed subset.
pub struct RouterExecutor {
    registry: ToolRegistry,
}

impl RouterExecutor {
    pub(crate) fn from_registry(registry: ToolRegistry) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl BatchToolExecutor for RouterExecutor {
    async fn execute_tool(
        &self,
        name: &str,
        arguments: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        self.registry
            .execute_with_context(name, arguments, context.clone())
            .await
    }
    fn has_tool(&self, name: &str) -> bool {
        self.registry.has(name)
    }
}

impl ToolRouter {
    pub fn new() -> Self {
        Self {
            registry: ToolRegistry::new(),
        }
    }
    pub async fn execute(
        &self,
        name: &str,
        arguments: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        self.registry
            .execute_with_context(name, arguments, context.clone())
            .await
    }
    pub fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.registry.get_definitions()
    }
    pub fn has_tool(&self, name: &str) -> bool {
        self.registry.has(name)
    }
    pub fn register_handler(&mut self, handler: Box<dyn ToolHandler>) {
        self.registry
            .handlers
            .insert(handler.name().to_string(), Arc::from(handler));
    }
    pub fn register(&mut self, definition: ToolDefinition, handler: Box<dyn ToolHandler>) {
        self.registry
            .register_with_handler(definition, Arc::from(handler));
    }
    pub fn set_lsp(&mut self, lsp: Arc<crate::integrations::LspIntegration>) {
        self.registry.set_lsp(lsp);
    }
}
impl Default for ToolRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_read_file() {
        let router = ToolRouter::new();
        let context = ToolContext::new(PathBuf::from("."));

        let result = router
            .execute(
                "Read",
                serde_json::json!({ "path": "Cargo.toml" }),
                &context,
            )
            .await;

        // This test assumes we're in the cortex-core directory
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let router = ToolRouter::new();
        let context = ToolContext::new(PathBuf::from("."));

        let result = router
            .execute("unknown_tool", serde_json::json!({}), &context)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_batch_tool_exists() {
        let router = ToolRouter::new();
        assert!(router.has_tool("Batch"));
    }

    #[tokio::test]
    async fn test_batch_tool_execution() {
        let router = ToolRouter::new();
        let context = ToolContext::new(PathBuf::from("."));

        // Test batch execution with multiple LS operations
        // Note: LS handler may fail if directory doesn't exist in test context,
        // but batch should still complete and report results
        let result = router
            .execute(
                "Batch",
                serde_json::json!({
                    "calls": [
                        {"tool": "LS", "arguments": {"directory_path": "."}},
                        {"tool": "LS", "arguments": {"directory_path": "."}}
                    ]
                }),
                &context,
            )
            .await;

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        // Batch completed - check output contains results summary
        assert!(
            tool_result.output.contains("Results:")
                || tool_result.output.contains("tools executed"),
            "Expected batch results summary, got: {}",
            tool_result.output
        );
    }

    #[tokio::test]
    async fn test_batch_prevents_recursion() {
        let router = ToolRouter::new();
        let context = ToolContext::new(PathBuf::from("."));

        // Try to call Batch within Batch - should fail validation with Result::Err
        // because recursive/disallowed tools trigger a validation error
        let result = router
            .execute(
                "Batch",
                serde_json::json!({
                    "calls": [
                        {"tool": "Batch", "arguments": {"calls": []}}
                    ]
                }),
                &context,
            )
            .await;

        // Validation errors return Result::Err, not Result::Ok with error ToolResult
        assert!(result.is_err());
        let error = result.unwrap_err();
        let error_msg = error.to_string();
        assert!(
            error_msg.contains("cannot be called within a batch")
                || error_msg.contains("Recursive"),
            "Expected recursion error message, got: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn child_task_cannot_call_task_or_ask_user() {
        let router = ToolRouter::new();
        let mut context = ToolContext::new(PathBuf::from("."));
        context.env.insert("CORTEX_CHILD_TASK".into(), "1".into());

        let nested = router
            .execute("Task", serde_json::json!({"prompt": "nested"}), &context)
            .await
            .unwrap();
        assert!(!nested.success);
        assert!(nested.output.contains("Child tasks cannot") || nested.output.contains("Nested"));

        let ask = router
            .execute("Questions", serde_json::json!({"questions": []}), &context)
            .await
            .unwrap();
        assert!(!ask.success);
    }
}
