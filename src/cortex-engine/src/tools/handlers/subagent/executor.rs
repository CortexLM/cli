//! Subagent executor - runs agents in isolated sessions.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{RwLock, mpsc};
use tokio::time::timeout;
use uuid::Uuid;

use crate::agent::{AgentConfig, Orchestrator, SandboxPolicy};
use crate::agents::{Agent, AgentRegistry};
use crate::client::ModelClient;
use crate::error::{CortexError, Result};
use crate::tools::registry::ToolRegistry;

use super::instruction_audit::{managed_policy_sources, record_instruction_audit};
use super::progress::{ProgressEvent, SubagentProgress};
use super::result::SubagentResult;
use super::run_helpers::{
    TurnOutcome, apply_summary_turn, build_result, effective_max_iterations, effective_model,
    needs_summary_turn, spawn_event_forwarder,
};
use super::types::{SubagentConfig, SubagentSession, SubagentStatus};

/// Executor for running subagents in isolated sessions.
pub struct SubagentExecutor {
    /// Model client.
    client: Arc<dyn ModelClient>,
    /// Tool registry.
    tools: Arc<ToolRegistry>,
    /// Agent registry.
    agent_registry: Arc<AgentRegistry>,
    /// Active sessions.
    sessions: RwLock<HashMap<String, SubagentSession>>,
    /// Default model.
    default_model: String,
    /// Default sandbox policy.
    default_sandbox_policy: SandboxPolicy,
    /// Maximum concurrent subagents.
    max_concurrent: usize,
    /// Active subagent count.
    active_count: RwLock<usize>,
}

impl SubagentExecutor {
    /// Create a new subagent executor.
    pub fn new(
        client: Arc<dyn ModelClient>,
        tools: Arc<ToolRegistry>,
        agent_registry: Arc<AgentRegistry>,
        default_model: impl Into<String>,
    ) -> Self {
        Self {
            client,
            tools,
            agent_registry,
            sessions: RwLock::new(HashMap::new()),
            default_model: default_model.into(),
            default_sandbox_policy: SandboxPolicy::Prompt,
            max_concurrent: 3,
            active_count: RwLock::new(0),
        }
    }

    /// Set sandbox policy.
    pub fn with_sandbox_policy(mut self, policy: SandboxPolicy) -> Self {
        self.default_sandbox_policy = policy;
        self
    }

    /// Set max concurrent subagents.
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }

    /// Execute a subagent with the given configuration.
    pub async fn execute(
        &self,
        config: SubagentConfig,
        progress_tx: mpsc::UnboundedSender<ProgressEvent>,
    ) -> Result<SubagentResult> {
        // Check concurrent limit
        {
            let count = *self.active_count.read().await;
            if count >= self.max_concurrent {
                return Err(CortexError::RateLimit(format!(
                    "Maximum concurrent subagents ({}) reached",
                    self.max_concurrent
                )));
            }
        }

        // Check if continuing existing session
        if let Some(ref session_id) = config.continue_session_id {
            return self
                .continue_session(session_id, &config.prompt, progress_tx)
                .await;
        }

        // Create new session - use provided session_id or generate a new one
        let session_id = config.session_id.clone().unwrap_or_else(|| {
            format!(
                "sub_{}",
                Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("unknown")
            )
        });
        let session = SubagentSession::new(
            &session_id,
            config.parent_session_id.clone(),
            config.agent_type.clone(),
            &config.description,
            config.working_dir.clone(),
        );

        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), session.clone());
        }

        // Increment active count
        {
            let mut count = self.active_count.write().await;
            *count += 1;
        }

        // Run the subagent
        let result = self.run_subagent(session, config, progress_tx).await;

        // Decrement active count
        {
            let mut count = self.active_count.write().await;
            *count = count.saturating_sub(1);
        }

        result
    }

    /// Continue an existing session.
    async fn continue_session(
        &self,
        session_id: &str,
        additional_prompt: &str,
        progress_tx: mpsc::UnboundedSender<ProgressEvent>,
    ) -> Result<SubagentResult> {
        // Get existing session
        let session = {
            let sessions = self.sessions.read().await;
            sessions.get(session_id).cloned()
        };

        let mut session = match session {
            Some(s) => s,
            None => {
                return Err(CortexError::NotFound(format!(
                    "Session not found: {}",
                    session_id
                )));
            }
        };

        // Check if session can be continued
        if !session.status.can_resume() && session.status != SubagentStatus::Completed {
            return Err(CortexError::InvalidInput(format!(
                "Session {} cannot be continued (status: {})",
                session_id, session.status
            )));
        }

        // Update session status
        session.set_status(SubagentStatus::Running);
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.to_string(), session.clone());
        }

        // Create config for continuation
        let config = SubagentConfig::new(
            session.agent_type.clone(),
            format!("Continue: {}", session.description),
            additional_prompt,
            session.working_dir.clone(),
        )
        .with_parent_session(session.parent_id.clone().unwrap_or_default());

        // Increment active count
        {
            let mut count = self.active_count.write().await;
            *count += 1;
        }

        // Run continuation
        let result = self.run_subagent(session, config, progress_tx).await;

        // Decrement active count
        {
            let mut count = self.active_count.write().await;
            *count = count.saturating_sub(1);
        }

        result
    }

    /// Look up the custom agent for this run, if the type names one.
    ///
    /// A named agent that is missing from the registry is an error, not a
    /// silent fallback: the run would otherwise use the wrong prompt.
    async fn resolve_custom_agent(&self, config: &SubagentConfig) -> Result<Option<Agent>> {
        let Some(agent_name) = config.agent_type.custom_name() else {
            return Ok(None);
        };
        match self.agent_registry.get(agent_name).await {
            Some(agent) => {
                tracing::info!(agent_name = agent_name, "Using custom agent from registry");
                Ok(Some(agent))
            }
            None => Err(CortexError::NotFound(format!(
                "Custom agent '{}' not found in registry. Available agents: {:?}",
                agent_name,
                self.agent_registry.list_names().await
            ))),
        }
    }

    /// Task-level omit list wins; otherwise take custom-agent frontmatter scopes.
    fn with_frontmatter_omissions(
        config: SubagentConfig,
        custom_agent: Option<&Agent>,
    ) -> SubagentConfig {
        if !config.omit_instructions.is_empty() {
            return config;
        }
        let Some(agent) = custom_agent else {
            return config;
        };
        if agent.metadata.omit_instructions.is_empty() {
            return config;
        }
        config.with_omit_instructions(agent.metadata.omit_instructions.clone())
    }

    /// Append the child's instruction documents to its system prompt.
    ///
    /// Organization-managed policy always loads. A request that named it is
    /// recorded in the audit journal alongside the omission itself, so an
    /// omitted run is reviewable after the fact.
    fn apply_instruction_plan(&self, config: &SubagentConfig, base: String) -> Result<String> {
        let plan = config.instruction_plan();
        let cortex_home = crate::config::find_cortex_home()
            .unwrap_or_else(|_| std::path::PathBuf::from(".cortex"));
        let mut sources = crate::instruction_scopes::InstructionSources::discover(
            &config.working_dir,
            &cortex_home,
        );
        // Always prefer the explicit managed-policy source when configured.
        let managed = managed_policy_sources();
        if !managed.is_empty() {
            sources.managed = managed;
        }
        let load = crate::instruction_scopes::load(&sources, &plan)
            .map_err(|error| CortexError::Other(anyhow::anyhow!("{error}")))?;
        if plan.omits_anything() || plan.requested_managed() {
            record_instruction_audit(&plan, &load);
        }
        if load.text.is_empty() {
            return Ok(base);
        }
        Ok(format!(
            "{base}

## Project Instructions
{}
",
            load.text
        ))
    }

    /// Run a subagent.
    async fn run_subagent(
        &self,
        mut session: SubagentSession,
        config: SubagentConfig,
        progress_tx: mpsc::UnboundedSender<ProgressEvent>,
    ) -> Result<SubagentResult> {
        let _start_time = Instant::now();
        let session_id = session.id.clone();

        // Create progress tracker
        let mut progress = SubagentProgress::new(
            &session_id,
            config.agent_type.clone(),
            &config.description,
            progress_tx.clone(),
        );

        progress.set_status(SubagentStatus::Running);
        session.set_status(SubagentStatus::Running);

        // Look up custom agent from registry if this is a Custom type
        let custom_agent = self.resolve_custom_agent(&config).await?;

        // Build system prompt - use base prompt WITHOUT task details
        // Task details will be sent as user message
        let system_prompt = if let Some(ref agent) = custom_agent {
            // Use the custom agent's base system prompt
            agent.system_prompt.clone()
        } else {
            config.build_base_system_prompt()
        };

        // Instruction documents for the child: user, project, and local
        // documents can be omitted; organization-managed policy always loads.
        // Custom-agent frontmatter omissions apply when the Task did not set any;
        // a non-empty Task-level list always wins.
        let config = Self::with_frontmatter_omissions(config, custom_agent.as_ref());
        let system_prompt = self.apply_instruction_plan(&config, system_prompt)?;

        // Build user message containing the task
        // Tasks are conversational - sent as user messages rather than system config
        let user_task_message = config.build_user_message();

        // Determine model and max iterations - custom agent can override
        let agent_config = AgentConfig {
            model: effective_model(custom_agent.as_ref(), &config, &self.default_model),
            max_tool_iterations: effective_max_iterations(custom_agent.as_ref(), &config),
            max_output_tokens: 16384,
            tool_timeout: Duration::from_secs(120),
            sandbox_policy: self.default_sandbox_policy,
            auto_approve_safe: true, // Subagents auto-approve safe operations
            streaming: true,         // Enable streaming for progressive feedback
            system_prompt: Some(system_prompt.clone()),
            working_directory: config.working_dir.clone(),
            ..AgentConfig::default()
        };

        // Custom agent tool restrictions planned for future implementation
        // This would require modifying the tool registry or orchestrator to filter tools

        // Create orchestrator for the subagent
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let orchestrator = Orchestrator::new(
            self.client.clone(),
            self.tools.clone(),
            agent_config,
            event_tx,
        );

        // Initialize the orchestrator
        orchestrator.initialize(Some(&system_prompt)).await;

        // Set up event forwarding
        let event_handler =
            spawn_event_forwarder(progress_tx.clone(), session_id.clone(), event_rx);

        // Create turn context with the full user task message
        // This sends the task as a user message, not embedded in system prompt
        let turn_id = session.turns_completed as u64 + 1;
        let mut turn_ctx = crate::agent::TurnContext::new(
            turn_id,
            session_id.clone(),
            user_task_message, // Use the formatted user message with task details
            config.working_dir.clone(),
        );

        // Execute with optional timeout (no timeout by default)
        let mut turn_result = if let Some(timeout_duration) = config.effective_timeout() {
            // With timeout
            match timeout(timeout_duration, orchestrator.process_turn(&mut turn_ctx)).await {
                Ok(result) => result,
                Err(_) => {
                    progress.fail("Execution timed out", true);
                    session.set_status(SubagentStatus::TimedOut);
                    return Err(CortexError::Timeout);
                }
            }
        } else {
            // No timeout - run until completion
            orchestrator.process_turn(&mut turn_ctx).await
        };

        // MANDATORY: Request explicit summary if the response doesn't contain one
        // This ensures subagents always provide structured output for the orchestrator
        if needs_summary_turn(&turn_result) {
            tracing::info!(
                session_id = %session_id,
                "Subagent output missing summary, requesting explicit summary turn"
            );
            apply_summary_turn(
                &orchestrator,
                &session,
                &session_id,
                &config,
                &mut turn_ctx,
                &mut turn_result,
            )
            .await;
        }

        // CRITICAL: Drop orchestrator to close the event channel
        // This allows event_handler to exit its recv() loop
        // Without this, we'd have a deadlock:
        // - event_handler.await waits for the task to finish
        // - The task waits for event_rx.recv() to return None
        // - recv() returns None only when all senders (event_tx) are dropped
        // - event_tx is owned by orchestrator, which won't drop until after await
        drop(orchestrator);

        // Wait for event handler to finish (now safe since channel is closed)
        let files_modified = event_handler.await.unwrap_or_default();

        // Process result
        // Extract status for better error messages when success=false but no error
        let outcome = TurnOutcome::from_result(&turn_result);

        // Update session
        session.record_turn(
            turn_ctx.tool_results.len() as u32,
            turn_ctx.tokens.total_tokens as u64,
        );
        for path in &files_modified {
            session.record_file_modified(path);
        }
        session.set_status(if outcome.success {
            SubagentStatus::Completed
        } else {
            // Any non-success state should be marked as Failed
            SubagentStatus::Failed
        });

        // Store updated session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), session.clone());
        }

        // Record completion or failure
        if outcome.success {
            progress.complete(&outcome.output);
        } else if let Some(ref err) = outcome.error {
            progress.fail(err, false);
        } else {
            progress.fail(&outcome.failure_message(), true);
        }

        Ok(build_result(
            session,
            outcome,
            &files_modified,
            &turn_ctx,
            &config,
        ))
    }

    /// Get a session by ID.
    pub async fn get_session(&self, session_id: &str) -> Option<SubagentSession> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// List all sessions.
    pub async fn list_sessions(&self) -> Vec<SubagentSession> {
        self.sessions.read().await.values().cloned().collect()
    }

    /// Get active session count.
    pub async fn active_count(&self) -> usize {
        *self.active_count.read().await
    }

    /// Cancel a session.
    pub async fn cancel_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.set_status(SubagentStatus::Cancelled);
            Ok(())
        } else {
            Err(CortexError::NotFound(format!(
                "Session not found: {}",
                session_id
            )))
        }
    }

    /// Get available subagent types with descriptions.
    pub fn available_types(&self) -> Vec<SubagentTypeInfo> {
        vec![
            SubagentTypeInfo {
                name: "code".to_string(),
                description: "General-purpose coding agent with full tool access. Use for implementing features, fixing bugs, and writing code.".to_string(),
                allowed_tools: None,
                denied_tools: vec![],
            },
            SubagentTypeInfo {
                name: "research".to_string(),
                description: "Read-only research agent for investigation. Use for understanding code, finding patterns, and gathering information. Cannot modify files.".to_string(),
                allowed_tools: Some(vec!["Read", "Grep", "Glob", "LS", "FetchUrl", "WebSearch"].into_iter().map(String::from).collect()),
                denied_tools: vec!["Create", "Edit", "ApplyPatch", "MultiEdit", "Execute"].into_iter().map(String::from).collect(),
            },
            SubagentTypeInfo {
                name: "refactor".to_string(),
                description: "Refactoring agent for code improvements. Use for restructuring, renaming, and cleaning up code.".to_string(),
                allowed_tools: None,
                denied_tools: vec![],
            },
            SubagentTypeInfo {
                name: "test".to_string(),
                description: "Testing agent for writing and running tests. Use for creating test cases and improving coverage.".to_string(),
                allowed_tools: None,
                denied_tools: vec![],
            },
            SubagentTypeInfo {
                name: "documentation".to_string(),
                description: "Documentation agent for writing docs. Use for creating README files, API docs, and comments.".to_string(),
                allowed_tools: None,
                denied_tools: vec![],
            },
            SubagentTypeInfo {
                name: "security".to_string(),
                description: "Security audit agent. Use for finding vulnerabilities, checking configurations, and reviewing access controls.".to_string(),
                allowed_tools: Some(vec!["Read", "Grep", "Glob", "LS", "Execute"].into_iter().map(String::from).collect()),
                denied_tools: vec![],
            },
            SubagentTypeInfo {
                name: "architect".to_string(),
                description: "Architecture planning agent. Use for designing systems, planning refactors, and making technical decisions. Cannot modify files.".to_string(),
                allowed_tools: Some(vec!["Read", "Grep", "Glob", "LS", "WebSearch"].into_iter().map(String::from).collect()),
                denied_tools: vec!["Create", "Edit", "ApplyPatch", "MultiEdit", "Execute"].into_iter().map(String::from).collect(),
            },
            SubagentTypeInfo {
                name: "reviewer".to_string(),
                description: "Code review agent. Use for reviewing changes, finding bugs, and suggesting improvements. Cannot modify files.".to_string(),
                allowed_tools: Some(vec!["Read", "Grep", "Glob", "LS"].into_iter().map(String::from).collect()),
                denied_tools: vec!["Create", "Edit", "ApplyPatch", "MultiEdit", "Execute"].into_iter().map(String::from).collect(),
            },
        ]
    }

    /// Get custom agents from the registry.
    pub async fn custom_agents(&self) -> Vec<Agent> {
        self.agent_registry.list().await
    }
}

/// Information about a subagent type.
#[derive(Debug, Clone)]
pub struct SubagentTypeInfo {
    /// Type name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Allowed tools (None = all).
    pub allowed_tools: Option<Vec<String>>,
    /// Denied tools.
    pub denied_tools: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_type_info() {
        let executor = SubagentExecutor::new(
            Arc::new(MockClient::new()),
            Arc::new(ToolRegistry::new()),
            Arc::new(AgentRegistry::new(&std::path::PathBuf::from("/tmp"), None)),
            "gpt-4o",
        );

        let types = executor.available_types();
        assert!(!types.is_empty());
        assert!(types.iter().any(|t| t.name == "code"));
        assert!(types.iter().any(|t| t.name == "research"));
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
