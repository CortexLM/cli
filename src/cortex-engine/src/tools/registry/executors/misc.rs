//! Miscellaneous tool executors (todo, task, web fetch).

use serde_json::Value;

use crate::error::Result;
use crate::tools::ToolContext;
use crate::tools::registry::ToolRegistry;
use crate::tools::spec::{ToolHandler, ToolResult};

impl ToolRegistry {
    pub(crate) async fn execute_fetch_url(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        crate::tools::handlers::FetchUrlHandler::new()
            .execute(args, context)
            .await
    }

    async fn todo_state(
        &self,
        context: &ToolContext,
    ) -> std::sync::Arc<tokio::sync::RwLock<Vec<crate::tools::handlers::TodoItem>>> {
        let key = (context.cwd.clone(), context.conversation_id.clone());
        self.todo_lists
            .write()
            .await
            .entry(key)
            .or_default()
            .clone()
    }

    pub(crate) async fn execute_todo_write(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let handler = crate::tools::handlers::TodoWriteHandler::with_shared_state(
            self.todo_state(context).await,
        );
        handler.execute(args, context).await
    }

    pub(crate) async fn execute_todo_read(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let handler = crate::tools::handlers::TodoReadHandler::with_shared_state(
            self.todo_state(context).await,
        );
        handler.execute(args, context).await
    }

    pub(crate) async fn execute_task(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        // Use the SimpleTaskHandler for registry-based execution
        // Full subagent execution happens through the orchestrator
        let handler = crate::tools::handlers::SimpleTaskHandler::new();
        handler.execute(args, context).await
    }

    pub(crate) async fn execute_list_subagents(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let handler = crate::tools::handlers::ListSubagentsHandler::new();
        handler.execute(args, context).await
    }
}
