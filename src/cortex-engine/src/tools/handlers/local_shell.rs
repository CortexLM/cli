//! Local shell tool handler.
//!
//! Every command uses the context's authorization and required sandbox policy.
//!
//! SECURITY: This module uses the exec/runner module which provides:
//! - Process isolation via kill_on_drop and setpgid
//! - Environment filtering (removes sensitive variables)
//! - Non-interactive mode enforcement (CI=true, TERM=dumb, etc.)
//! - Proper timeout handling with process group killing

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use super::{ToolContext, ToolHandler, ToolResult};
use crate::error::Result;
use crate::exec::{ExecOptions, ExecOutput, OutputChunk, execute_command_streaming};
use crate::tools::context::ToolOutputChunk;
use crate::tools::spec::ToolMetadata;

/// Handler for local_shell tool.
pub struct LocalShellHandler;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LocalShellArgs {
    command: Vec<String>,
    workdir: Option<String>,
    timeout: Option<u64>,
    risk_level: Option<String>,
    risk_reason: Option<String>,
    background: Option<bool>,
}

impl LocalShellHandler {
    pub fn new() -> Self {
        Self
    }

    /// The advertised contract is argv, not a shell fragment. Shell syntax must
    /// be requested explicitly as ["/bin/sh", "-c", script].
    fn build_command(args: &LocalShellArgs) -> Vec<String> {
        args.command.clone()
    }
}

impl Default for LocalShellHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for LocalShellHandler {
    fn name(&self) -> &str {
        "Execute"
    }

    async fn execute(&self, arguments: Value, context: &ToolContext) -> Result<ToolResult> {
        if let Err(message) =
            crate::tools::boundary::check_tool_call(context, "Execute", &arguments)
        {
            return Ok(ToolResult::error(message));
        }
        let args: LocalShellArgs = serde_json::from_value(arguments)?;

        if args.command.is_empty() {
            return Ok(ToolResult::error("Empty command"));
        }
        if args.background == Some(true) {
            return Ok(ToolResult::error(
                "Background execution is unavailable; command was not started",
            ));
        }

        let command = Self::build_command(&args);
        let capture_edits = super::bash_edit_diff::bash_edit_diff_enabled(&context.env);

        // Resolve working directory
        let cwd = match context.resolve_and_validate_path(args.workdir.as_deref().unwrap_or(".")) {
            Ok(cwd) => cwd,
            Err(message) => return Ok(ToolResult::error(message)),
        };
        let before = if capture_edits {
            Some(super::bash_edit_diff::snapshot_workspace(&cwd))
        } else {
            None
        };

        // Build execution options
        let timeout = args
            .timeout
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_secs(60));

        let options = ExecOptions {
            cwd: cwd.clone(),
            timeout,
            env: context.env.clone(),
            capture_output: true,
            sandbox_policy: context.sandbox_policy.clone(),
            approval_granted: true,
        };

        // Create channel for streaming output
        let (tx, mut rx) = mpsc::channel::<OutputChunk>(100);

        // Clone context for the streaming task
        let ctx = context.clone();

        // Spawn task to forward output chunks to the tool context
        let forward_task = tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                match chunk {
                    OutputChunk::Stdout(s) => {
                        ctx.send_output(ToolOutputChunk::Stdout(s)).await;
                    }
                    OutputChunk::Stderr(s) => {
                        ctx.send_output(ToolOutputChunk::Stderr(s)).await;
                    }
                }
            }
        });

        // Execute command with streaming - this uses proper process isolation:
        // - kill_on_drop(true)
        // - env_clear() with safe environment rebuild
        // - setpgid(0,0) on Unix for process group isolation
        // - Filters sensitive environment variables
        // - Forces non-interactive mode (CI=true, TERM=dumb, etc.)
        let result = execute_command_streaming(&command, options, tx).await;

        // Wait for forwarding to complete
        let _ = forward_task.await;

        // Convert ExecOutput to ToolResult
        match result {
            Ok(output) => {
                let mut tool = exec_output_to_tool_result(output);
                if let Some(before) = before {
                    let after = super::bash_edit_diff::snapshot_workspace(&cwd);
                    let diff = super::bash_edit_diff::diff_snapshots(&cwd, &before, &after);
                    if let Some(meta) = tool.metadata.as_mut() {
                        let changed: Vec<String> = after
                            .keys()
                            .filter(|p| before.get(*p) != after.get(*p))
                            .map(|p| p.display().to_string())
                            .collect();
                        meta.files_modified = changed;
                    }
                    tool.output = super::bash_edit_diff::append_edit_diff(&tool.output, &diff);
                }
                Ok(tool)
            }
            Err(e) => Ok(ToolResult::error(format!("Execution failed: {e}"))),
        }
    }
}

/// Convert ExecOutput to ToolResult with proper metadata
fn exec_output_to_tool_result(output: ExecOutput) -> ToolResult {
    let mut result_text = String::new();

    if !output.stdout.is_empty() {
        result_text.push_str(&output.stdout);
    }
    if !output.stderr.is_empty() {
        if !result_text.is_empty() {
            result_text.push('\n');
        }
        result_text.push_str(&output.stderr);
    }

    if result_text.is_empty() {
        if output.timed_out {
            result_text = format!("Command timed out after {:?}", output.duration);
        } else {
            result_text = format!("Command completed with exit code {}", output.exit_code);
        }
    }

    let metadata = ToolMetadata {
        duration_ms: output.duration.as_millis() as u64,
        exit_code: Some(output.exit_code),
        files_modified: vec![],
        data: None,
    };

    let result = if output.timed_out {
        ToolResult::error(format!("Command timed out: {result_text}"))
    } else if output.exit_code == 0 {
        ToolResult::success(result_text)
    } else {
        ToolResult::error(format!("Exit code {}: {result_text}", output.exit_code))
    };

    result.with_metadata(metadata)
}
