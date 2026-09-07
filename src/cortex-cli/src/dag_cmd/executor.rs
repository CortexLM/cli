//! Task executor for running DAG tasks.

use anyhow::{Context, Result, bail};
use cortex_agents::task::{Task, TaskStatus};
use cortex_engine::tools::{ToolContext, ToolRouter};
use cortex_protocol::SandboxPolicy;
use std::time::{Duration, Instant};

use crate::styled_output::print_info;

use super::types::TaskExecutionResult;

/// Task executor that runs the actual task commands.
pub struct TaskExecutor {
    timeout: Duration,
    verbose: bool,
}

impl TaskExecutor {
    pub fn new(timeout_secs: u64, verbose: bool) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs),
            verbose,
        }
    }

    /// Execute a single task.
    pub async fn execute(&self, task: &Task) -> TaskExecutionResult {
        let start = Instant::now();
        let task_id = task.id.expect("Task must have an ID");

        if self.verbose {
            print_info(&format!("Executing task {}", task_id));
        }

        let execution = match task.metadata.get("command") {
            Some(serde_json::Value::String(cmd)) if !cmd.trim().is_empty() => {
                self.run_command(cmd).await
            }
            Some(_) => Err(anyhow::anyhow!("Task command must be a nonempty string")),
            None => Err(anyhow::anyhow!(
                "Agent DAG execution is unavailable: this task has no command. \
                 No agent was started. Supply an explicitly reviewed command task."
            )),
        };
        let (status, output, error) = match execution {
            Ok(output) => (TaskStatus::Completed, Some(output), None),
            Err(error) => (TaskStatus::Failed, None, Some(error.to_string())),
        };

        TaskExecutionResult {
            task_id,
            task_name: task.name.clone(),
            status,
            duration: start.elapsed(),
            output,
            error,
        }
    }

    /// Use the same tool authority and process owner as normal CLI execution.
    /// The user reviewed the DAG file, so each command receives one exact,
    /// single-use approval; DAG metadata never widens the workspace sandbox.
    async fn run_command(&self, cmd: &str) -> Result<String> {
        let cwd = std::env::current_dir().context("Could not resolve task workspace")?;
        let command = if cfg!(windows) {
            vec!["cmd.exe", "/C", cmd]
        } else {
            vec!["/bin/sh", "-c", cmd]
        };
        let timeout_ms =
            u64::try_from(self.timeout.as_millis()).context("Task timeout is too large")?;
        let arguments = serde_json::json!({"command": command, "timeout": timeout_ms});
        let context = ToolContext::new(cwd)
            .with_sandbox_policy(SandboxPolicy::new_workspace_write_policy())
            .with_auto_approve(false)
            .with_approved_tool_call("Execute", &arguments);
        let result = ToolRouter::new()
            .execute("Execute", arguments, &context)
            .await?;
        if !result.success {
            bail!(
                "{}",
                result.error.unwrap_or_else(|| "Task command failed".into())
            );
        }
        Ok(result.output)
    }
}
