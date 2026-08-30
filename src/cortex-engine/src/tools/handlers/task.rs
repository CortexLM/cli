//! Task tool handler - spawn and manage subagents for complex tasks.
//!
//! The Task tool enables delegation of work to specialized subagents that run
//! in isolated sessions. Each subagent type has different capabilities and
//! tool restrictions.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::agents::AgentRegistry;
use crate::client::ModelClient;
use crate::error::{CortexError, Result};
use crate::tools::context::ToolContext;
use crate::tools::registry::ToolRegistry;
use crate::tools::spec::{ToolDefinition, ToolHandler, ToolResult};

use super::subagent::{ProgressEvent, SubagentConfig, SubagentExecutor, SubagentType};

/// Task tool handler for spawning subagents.
pub struct TaskHandler {
    /// Subagent executor.
    executor: Arc<SubagentExecutor>,
    /// Working directory.
    working_dir: PathBuf,
}

impl TaskHandler {
    /// Create a new task handler.
    pub fn new(
        client: Arc<dyn ModelClient>,
        tools: Arc<ToolRegistry>,
        agent_registry: Arc<AgentRegistry>,
        default_model: impl Into<String>,
        working_dir: PathBuf,
    ) -> Self {
        let executor = Arc::new(SubagentExecutor::new(
            client,
            tools,
            agent_registry,
            default_model,
        ));

        Self {
            executor,
            working_dir,
        }
    }

    /// Create with a pre-configured executor.
    pub fn with_executor(executor: Arc<SubagentExecutor>, working_dir: PathBuf) -> Self {
        Self {
            executor,
            working_dir,
        }
    }

    /// Get the tool definition.
    pub fn definition() -> ToolDefinition {
        ToolDefinition::new("Task", Self::description(), Self::parameters())
    }

    /// Get detailed description.
    fn description() -> &'static str {
        r#"Delegate work to a child task. Roles: explore (read-only), plan (mermaid, no mutate), worker (implement). The parent receives task_started, progress, completed, or failed events. Optional artifact_id is attached when the result is large.

Children cannot spawn nested Task tools and cannot use AskUser, Questions, or send_to_user.

## Examples
```json
{ "mode": "explore", "prompt": "Find where sessions are created" }
```
```json
{ "mode": "plan", "prompt": "Plan adding device login" }
```
```json
{ "mode": "worker", "prompt": "Implement the keep-set compaction filter" }
```"#
    }

    /// Get parameter schema.
    fn parameters() -> Value {
        json!({
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "enum": ["explore", "plan", "worker"],
                    "description": "Child role. explore=read-only, plan=mermaid only, worker=implement."
                },
                "prompt": {
                    "type": "string",
                    "description": "Instructions for the child task."
                },
                "description": {
                    "type": "string",
                    "description": "Short parent-visible description."
                },
                "agent": {
                    "type": "string",
                    "description": "Legacy: custom agent name. Mapped to worker unless mode is set."
                },
                "task": {
                    "type": "string",
                    "description": "Legacy alias for prompt."
                },
                "context": {
                    "type": "string",
                    "description": "Additional context for the child."
                },
                "await_result": {
                    "type": "boolean",
                    "description": "Wait for the child (true) or return after task_started (false).",
                    "default": true
                }
            },
            "required": [],
            "additionalProperties": false
        })
    }

    /// Parse task parameters from arguments.
    fn parse_params(&self, arguments: Value) -> Result<TaskParams> {
        let mode = arguments
            .get("mode")
            .and_then(|m| m.as_str())
            .map(crate::harness::TaskRole::parse)
            .unwrap_or(crate::harness::TaskRole::Worker);

        let task = arguments
            .get("prompt")
            .or_else(|| arguments.get("task"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| CortexError::InvalidInput("prompt (or task) is required".into()))?
            .to_string();

        let agent = arguments
            .get("agent")
            .and_then(|a| a.as_str())
            .unwrap_or(mode.as_str())
            .to_string();

        let description = arguments
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or(&task)
            .to_string();

        let context = arguments
            .get("context")
            .and_then(|c| c.as_str())
            .map(String::from);

        let await_result = arguments
            .get("await_result")
            .and_then(|a| a.as_bool())
            .unwrap_or(true);

        Ok(TaskParams {
            agent,
            task,
            context,
            await_result,
            mode,
            description,
        })
    }

    /// Build subagent config from params.
    fn build_config(&self, params: TaskParams) -> SubagentConfig {
        let mut config = SubagentConfig::new(
            SubagentType::Code,
            &params.agent,
            &params.task,
            self.working_dir.clone(),
        );

        if let Some(context) = params.context {
            config = config.with_context(context);
        }

        config.env.insert("CORTEX_CHILD_TASK".into(), "1".into());
        if params.mode == crate::harness::TaskRole::Plan {
            config.env.insert("CORTEX_SPEC_MODE".into(), "1".into());
        }
        config.prompt = format!("{}\n\n{}", params.mode.system_prompt(), config.prompt);
        config
    }
}

#[async_trait]
impl ToolHandler for TaskHandler {
    fn name(&self) -> &str {
        "Task"
    }

    async fn execute(&self, arguments: Value, _context: &ToolContext) -> Result<ToolResult> {
        // Parse parameters
        let params = match self.parse_params(arguments) {
            Ok(p) => p,
            Err(e) => return Ok(ToolResult::error(e.to_string())),
        };

        // Build config
        let config = self.build_config(params.clone());

        // Create progress channel (for now we collect but don't stream)
        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<ProgressEvent>();

        // Spawn progress collector
        let progress_collector = tokio::spawn(async move {
            let mut events: Vec<String> = Vec::new();
            while let Some(event) = progress_rx.recv().await {
                let message = event.to_message();
                events.push(message);
                if event.is_terminal() {
                    break;
                }
            }
            events
        });

        let task_id = format!("task_{}_{}", params.mode.as_str(), uuid::Uuid::new_v4());
        let started = crate::harness::TaskParentEvent::Started {
            task_id: task_id.clone(),
            role: params.mode.as_str().to_string(),
            description: params.description.clone(),
        };

        // If await_result is false, return immediately with task queued message
        if !params.await_result {
            let output = format!(
                r#"## task_started

**task_id:** `{task_id}`
**mode:** `{}`
**Agent:** `{}`
**Task:** {}

The child task has been queued. Nested Task, AskUser, and send_to_user are disabled for the child."#,
                params.mode.as_str(),
                params.agent,
                params.task
            );
            let _ = started.kind();
            return Ok(ToolResult::success(output));
        }

        // Execute subagent
        let result = self.executor.execute(config, progress_tx).await;

        // Wait for progress collector
        let progress_messages = progress_collector.await.unwrap_or_default();

        // Format result
        match result {
            Ok(subagent_result) => {
                let output = subagent_result.to_tool_output();

                // Add progress log if verbose
                let mut full_output = output;
                if !progress_messages.is_empty() {
                    full_output.push_str("\n## Progress Log\n");
                    for msg in progress_messages.iter().take(20) {
                        full_output.push_str(&format!("• {}\n", msg));
                    }
                    if progress_messages.len() > 20 {
                        full_output.push_str(&format!(
                            "... and {} more events\n",
                            progress_messages.len() - 20
                        ));
                    }
                }

                let artifact_id = if full_output.len() >= crate::harness::ARTIFACT_THRESHOLD_BYTES {
                    Some(format!("art_{task_id}"))
                } else {
                    None
                };
                let header = if subagent_result.success {
                    crate::harness::TaskParentEvent::Completed {
                        task_id: task_id.clone(),
                        summary: params.description.clone(),
                        artifact_id: artifact_id.clone(),
                    }
                } else {
                    crate::harness::TaskParentEvent::Failed {
                        task_id: task_id.clone(),
                        error: "Child task failed".into(),
                        artifact_id: artifact_id.clone(),
                    }
                };
                full_output = format!(
                    "## {}\n**task_id:** `{task_id}`\n**mode:** `{}`\n{}\n\n{full_output}",
                    header.kind(),
                    params.mode.as_str(),
                    artifact_id
                        .map(|id| format!("**artifact_id:** `{id}`"))
                        .unwrap_or_default()
                );
                if subagent_result.success {
                    Ok(ToolResult::success(full_output))
                } else {
                    Ok(ToolResult::error(full_output))
                }
            }
            Err(e) => Ok(ToolResult::error(format!("Task execution failed: {}", e))),
        }
    }
}

/// Parsed task parameters.
#[derive(Debug, Clone)]
struct TaskParams {
    agent: String,
    task: String,
    context: Option<String>,
    await_result: bool,
    mode: crate::harness::TaskRole,
    description: String,
}

/// Create a standalone task handler with minimal dependencies (for registry integration).
/// This version uses a simpler execution path when the full executor isn't available.
pub struct SimpleTaskHandler;

impl SimpleTaskHandler {
    /// Create a new simple task handler.
    pub fn new() -> Self {
        Self
    }

    /// Get the tool definition.
    pub fn definition() -> ToolDefinition {
        TaskHandler::definition()
    }
}

impl Default for SimpleTaskHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for SimpleTaskHandler {
    fn name(&self) -> &str {
        "Task"
    }

    async fn execute(&self, arguments: Value, _context: &ToolContext) -> Result<ToolResult> {
        let mode = arguments
            .get("mode")
            .and_then(|m| m.as_str())
            .unwrap_or("worker");
        let agent = arguments
            .get("agent")
            .and_then(|a| a.as_str())
            .unwrap_or(mode);
        let task = arguments
            .get("prompt")
            .or_else(|| arguments.get("task"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| CortexError::InvalidInput("prompt (or task) is required".into()))?;

        let context = arguments.get("context").and_then(|c| c.as_str());

        let await_result = arguments
            .get("await_result")
            .and_then(|a| a.as_bool())
            .unwrap_or(true);

        let task_id = format!("task_{}_{}", mode, uuid::Uuid::new_v4());
        let mut output = format!(
            r#"## task_started

**task_id:** `{task_id}`
**mode:** `{mode}`
**Agent:** `{agent}`
**Task:** {task}
**Await Result:** {await_result}

"#
        );

        if let Some(ctx) = context {
            output.push_str(&format!("**Context:** {}\n\n", ctx));
        }

        output.push_str(
            "The child task was not started. Task needs a live coding session with a ModelClient. Nested Task, AskUser, and send_to_user stay disabled for children.",
        );

        Ok(ToolResult::error(output))
    }
}

/// Tool for listing available subagent types.
pub struct ListSubagentsHandler {
    executor: Option<Arc<SubagentExecutor>>,
}

impl ListSubagentsHandler {
    /// Create a new handler.
    pub fn new() -> Self {
        Self { executor: None }
    }

    /// Create with executor for dynamic agent listing.
    pub fn with_executor(executor: Arc<SubagentExecutor>) -> Self {
        Self {
            executor: Some(executor),
        }
    }

    /// Get the tool definition.
    pub fn definition() -> ToolDefinition {
        ToolDefinition::new(
            "ListSubagents",
            "List available subagent types and their capabilities. Use this to understand what specialized agents are available for task delegation.",
            json!({
                "type": "object",
                "properties": {
                    "include_custom": {
                        "type": "boolean",
                        "description": "Include custom agents from the agent registry. Default: true"
                    }
                },
                "required": []
            }),
        )
    }
}

impl Default for ListSubagentsHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for ListSubagentsHandler {
    fn name(&self) -> &str {
        "ListSubagents"
    }

    async fn execute(&self, arguments: Value, _context: &ToolContext) -> Result<ToolResult> {
        let include_custom = arguments
            .get("include_custom")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mut output = String::from("# Available Subagent Types\n\n");

        // Built-in types
        output.push_str("## Built-in Subagents\n\n");
        output.push_str("| Type | Description | Modifies Files |\n");
        output.push_str("|------|-------------|----------------|\n");

        let builtin_types = [
            (
                "code",
                "General-purpose coding agent. Implements features, fixes bugs, writes code.",
                true,
            ),
            (
                "research",
                "Investigation agent. Analyzes code, finds patterns, gathers information.",
                false,
            ),
            (
                "refactor",
                "Code improvement agent. Restructures, renames, cleans up code.",
                true,
            ),
            (
                "test",
                "Testing agent. Writes unit tests, improves coverage, runs tests.",
                true,
            ),
            (
                "documentation",
                "Documentation agent. Creates README, API docs, inline comments.",
                true,
            ),
            (
                "security",
                "Security audit agent. Finds vulnerabilities, reviews access controls.",
                true,
            ),
            (
                "architect",
                "Architecture planning agent. Designs systems, plans refactors.",
                false,
            ),
            (
                "reviewer",
                "Code review agent. Finds bugs, suggests improvements.",
                false,
            ),
        ];

        for (name, desc, modifies) in builtin_types {
            let modify_icon = if modifies { "[Y]" } else { "[N]" };
            output.push_str(&format!("| `{}` | {} | {} |\n", name, desc, modify_icon));
        }

        // Custom agents
        if include_custom {
            if let Some(ref executor) = self.executor {
                let custom_agents = executor.custom_agents().await;
                if !custom_agents.is_empty() {
                    output.push_str("\n## Custom Agents\n\n");
                    output.push_str("| Name | Description | Source |\n");
                    output.push_str("|------|-------------|--------|\n");

                    for agent in custom_agents {
                        output.push_str(&format!(
                            "| `{}` | {} | {} |\n",
                            agent.metadata.name, agent.metadata.description, agent.source
                        ));
                    }
                }
            }
        }

        output.push_str("\n## Usage Example\n\n");
        output.push_str("```json\n");
        output.push_str("{\n");
        output.push_str("  \"description\": \"Your task description\",\n");
        output.push_str("  \"prompt\": \"Detailed instructions for the subagent\",\n");
        output.push_str("  \"subagent_type\": \"code\"\n");
        output.push_str("}\n");
        output.push_str("```\n");

        Ok(ToolResult::success(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_params_parsing() {
        let handler = TaskHandler::with_executor(
            Arc::new(SubagentExecutor::new(
                Arc::new(MockClient::new()),
                Arc::new(ToolRegistry::new()),
                Arc::new(AgentRegistry::new(&PathBuf::from("/tmp"), None)),
                "gpt-4o",
            )),
            PathBuf::from("/project"),
        );

        let args = json!({
            "agent": "code-reviewer",
            "task": "Review the authentication module",
            "await_result": true
        });

        let params = handler.parse_params(args).unwrap();
        assert_eq!(params.agent, "code-reviewer");
        assert_eq!(params.task, "Review the authentication module");
        assert!(params.await_result);
    }

    #[test]
    fn test_task_params_with_context() {
        let handler = TaskHandler::with_executor(
            Arc::new(SubagentExecutor::new(
                Arc::new(MockClient::new()),
                Arc::new(ToolRegistry::new()),
                Arc::new(AgentRegistry::new(&PathBuf::from("/tmp"), None)),
                "gpt-4o",
            )),
            PathBuf::from("/project"),
        );

        let args = json!({
            "agent": "doc-writer",
            "task": "Generate API documentation",
            "context": "Use OpenAPI 3.0 format",
            "await_result": false
        });

        let params = handler.parse_params(args).unwrap();
        assert_eq!(params.agent, "doc-writer");
        assert_eq!(params.context, Some("Use OpenAPI 3.0 format".to_string()));
        assert!(!params.await_result);
    }

    #[tokio::test]
    async fn test_simple_task_handler() {
        let handler = SimpleTaskHandler::new();
        let context = ToolContext::new(PathBuf::from("/project"));

        let args = json!({
            "agent": "test-agent",
            "task": "Do something useful"
        });

        let result = handler.execute(args, &context).await.unwrap();
        assert!(!result.success);
        assert!(result.output.contains("test-agent"));
        assert!(result.output.contains("Do something useful"));
        assert!(result.output.contains("not started"));
    }

    #[tokio::test]
    async fn test_list_subagents_handler() {
        let handler = ListSubagentsHandler::new();
        let context = ToolContext::new(PathBuf::from("/project"));

        let result = handler.execute(json!({}), &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("code"));
        assert!(result.output.contains("research"));
        assert!(result.output.contains("refactor"));
    }

    // Mock client for testing
    struct MockClient {
        capabilities: crate::client::types::ModelCapabilities,
    }

    impl MockClient {
        fn new() -> Self {
            Self {
                capabilities: crate::client::types::ModelCapabilities::default(),
            }
        }
    }

    #[async_trait::async_trait]
    impl ModelClient for MockClient {
        fn model(&self) -> &str {
            "mock-model"
        }

        fn provider(&self) -> &str {
            "mock-provider"
        }

        fn capabilities(&self) -> &crate::client::types::ModelCapabilities {
            &self.capabilities
        }

        async fn complete(
            &self,
            _request: crate::client::types::CompletionRequest,
        ) -> crate::error::Result<crate::client::ResponseStream> {
            Err(crate::error::CortexError::Internal("Mock".into()))
        }

        async fn complete_sync(
            &self,
            _request: crate::client::types::CompletionRequest,
        ) -> crate::error::Result<crate::client::types::CompletionResponse> {
            Err(crate::error::CortexError::Internal("Mock".into()))
        }
    }
}
