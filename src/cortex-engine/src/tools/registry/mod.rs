//! Tool registry - manages tool definitions, handlers, and plugins.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use super::spec::{ToolDefinition, ToolHandler, ToolResult};
use crate::error::Result;

mod definitions;
mod executors;
mod plugins;
mod types;

pub use types::PluginTool;
type TodoState = Arc<tokio::sync::RwLock<Vec<super::handlers::TodoItem>>>;
// ponytail: registry-lifetime state only; use caller-owned storage for cross-session persistence.
type TodoLists = Arc<tokio::sync::RwLock<HashMap<(std::path::PathBuf, String), TodoState>>>;

/// Registry of available tools.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    pub(crate) tools: HashMap<String, ToolDefinition>,
    pub(crate) plugins: HashMap<String, PluginTool>,
    pub(crate) handlers: HashMap<String, Arc<dyn ToolHandler>>,
    /// LSP integration.
    pub(crate) lsp: Option<Arc<crate::integrations::LspIntegration>>,
    pub(crate) todo_lists: TodoLists,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.tools)
            .field("plugins", &self.plugins)
            .field("handlers_count", &self.handlers.len())
            .finish()
    }
}

impl ToolRegistry {
    /// Create a new registry with default tools.
    pub fn new() -> Self {
        let mut registry = Self::default();
        registry.register_default_tools();
        registry
    }

    /// Create a new registry and load plugins from standard directories.
    pub async fn new_with_plugins() -> Self {
        let mut registry = Self::new();

        // Load plugins from ~/.cortex/plugins
        if let Some(home) = dirs::home_dir() {
            let plugins_dir = home.join(".cortex").join("plugins");
            if let Ok(count) = registry.load_plugins_from_dir(&plugins_dir).await
                && count > 0
            {
                tracing::info!("Loaded {} plugins from {}", count, plugins_dir.display());
            }
        }

        // Load plugins from .cortex/plugins in current directory
        let local_plugins = std::path::Path::new(".cortex/plugins");
        if let Ok(count) = registry.load_plugins_from_dir(local_plugins).await
            && count > 0
        {
            tracing::info!("Loaded {} local plugins", count);
        }

        registry
    }

    /// Register a tool.
    pub fn register(&mut self, tool: ToolDefinition) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Register a tool with handler.
    pub fn register_with_handler(&mut self, tool: ToolDefinition, handler: Arc<dyn ToolHandler>) {
        self.handlers.insert(tool.name.clone(), handler);
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Set the LSP integration.
    pub fn set_lsp(&mut self, lsp: Arc<crate::integrations::LspIntegration>) {
        self.lsp = Some(lsp);
    }

    /// Get a tool definition.
    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// Get all tool definitions.
    pub fn all(&self) -> Vec<&ToolDefinition> {
        self.tools.values().collect()
    }

    /// Get tool definitions for API.
    pub fn get_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .filter(|tool| match tool.name.as_str() {
                "LspSymbols" | "WebSearch" => false,
                "LspDiagnostics" | "LspHover" => self.lsp.is_some(),
                "Task" => self.handlers.contains_key("Task"),
                _ => true,
            })
            .cloned()
            .collect()
    }

    /// Check if a tool is registered.
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
            || self.plugins.contains_key(name)
            || self.handlers.contains_key(name)
    }

    /// Execute a tool.
    pub async fn execute(&self, name: &str, arguments: Value) -> Result<ToolResult> {
        let _ = (name, arguments);
        Ok(ToolResult::error(
            "Tool execution requires an explicit workspace and authorization context",
        ))
    }

    /// Execute a tool with a custom context (for output streaming support).
    pub async fn execute_with_context(
        &self,
        name: &str,
        arguments: Value,
        mut context: super::context::ToolContext,
    ) -> Result<ToolResult> {
        if !self.has(name) {
            return Err(crate::error::CortexError::UnknownTool {
                name: context.redact(name),
            });
        }
        if let Err(message) = super::boundary::authorize_tool_call(&context, name, &arguments) {
            return Ok(ToolResult::error(context.redact(&message)));
        }
        let arguments = match super::boundary::prepare_arguments(&context, name, arguments) {
            Ok(arguments) => arguments,
            Err(message) => return Ok(ToolResult::error(context.redact(&message))),
        };
        if !super::boundary::is_read_only_tool(name) {
            // Preserve the exact authorized call after canonical path rewriting.
            context = context.with_approved_tool_call(name, &arguments);
        }
        if context.lsp.is_none() {
            context.lsp = self.lsp.clone();
        }
        let result = self.dispatch(name, arguments, &context).await;
        match result {
            Ok(result) => Ok(super::boundary::redact_result(&context, result)),
            Err(error) => Err(crate::error::CortexError::tool_execution(
                context.redact(name),
                context.redact(&error.to_string()),
            )),
        }
    }

    async fn dispatch(
        &self,
        name: &str,
        arguments: Value,
        context: &super::context::ToolContext,
    ) -> Result<ToolResult> {
        // Every handler, including plugins and Batch children, enters above.
        if let Some(handler) = self.handlers.get(name) {
            return handler.execute(arguments, context).await;
        }
        if let Some(plugin) = self.plugins.get(name) {
            return self.execute_plugin(plugin, arguments, context).await;
        }
        match name {
            "Read" => self.execute_read_file(arguments, context).await,
            "Create" => self.execute_write_file(arguments, context).await,
            "LS" | "Tree" => self.execute_list_dir(arguments, context).await,
            "SearchFiles" => self.execute_search_files(arguments, context).await,
            "Edit" => self.execute_edit_file(arguments, context).await,
            "Grep" => self.execute_grep(arguments, context).await,
            "Glob" => self.execute_glob(arguments, context).await,
            "FetchUrl" | "WebFetch" => self.execute_fetch_url(arguments, context).await,
            "TodoWrite" => self.execute_todo_write(arguments, context).await,
            "TodoRead" => self.execute_todo_read(arguments, context).await,
            "Task" => self.execute_task(arguments, context).await,
            "ListSubagents" => self.execute_list_subagents(arguments, context).await,
            "Batch" => {
                use super::handlers::batch::BatchToolHandler;
                let executor = Arc::new(super::router::RouterExecutor::from_registry(self.clone()));
                BatchToolHandler::new(executor)
                    .execute(arguments, context)
                    .await
            }
            "Plan" => {
                super::handlers::PlanHandler::new()
                    .execute(arguments, context)
                    .await
            }
            "UpdateGoal" => {
                super::handlers::UpdateGoalHandler::new()
                    .execute(arguments, context)
                    .await
            }
            "Propose" => {
                super::handlers::ProposeHandler::new()
                    .execute(arguments, context)
                    .await
            }
            "Questions" => {
                super::handlers::QuestionsHandler::new()
                    .execute(arguments, context)
                    .await
            }
            // A tool must not unlock its own read-only authorization.
            "ExitSpecMode" => Ok(ToolResult::error(
                "Mode changes require explicit user confirmation",
            )),
            "LspDiagnostics" | "LspHover" | "LspSymbols" => Ok(ToolResult::error(
                "This language-server capability is unavailable",
            )),
            _ => Ok(ToolResult::error(format!("Tool not implemented: {name}"))),
        }
    }
}
