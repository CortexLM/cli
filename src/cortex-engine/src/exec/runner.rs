//! Policy-enforced process execution with bounded capture and owned process groups.
use super::{DEFAULT_TIMEOUT, MAX_OUTPUT_SIZE, build_safe_environment};
use crate::error::{CortexError, Result};
use cortex_protocol::SandboxPolicy;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum OutputChunk {
    Stdout(String),
    Stderr(String),
}

#[derive(Clone)]
pub struct ExecOptions {
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub sandbox_policy: SandboxPolicy,
    pub env: HashMap<String, String>,
    pub capture_output: bool,
    /// Only the trusted authorization boundary may grant this escalation.
    pub approval_granted: bool,
}
impl std::fmt::Debug for ExecOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecOptions")
            .field("cwd", &self.cwd)
            .field("sandbox_policy", &self.sandbox_policy)
            .field("timeout", &self.timeout)
            .field("env_keys", &self.env.keys())
            .field("approval_granted", &self.approval_granted)
            .finish()
    }
}
impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_default(),
            timeout: DEFAULT_TIMEOUT,
            sandbox_policy: SandboxPolicy::default(),
            env: HashMap::new(),
            capture_output: true,
            approval_granted: false,
        }
    }
}
#[derive(Debug, Clone)]
pub struct ExecOutput {
    pub stdout: String,
    pub stderr: String,
    pub aggregated: String,
    pub exit_code: i32,
    pub duration: Duration,
    pub timed_out: bool,
}

async fn capture(mut stream: impl AsyncRead + Unpin) -> std::io::Result<String> {
    let mut saved = Vec::new();
    let mut total = 0usize;
    let mut buf = [0u8; 8192];
    loop {
        let count = stream.read(&mut buf).await?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count);
        let keep = count.min(MAX_OUTPUT_SIZE.saturating_sub(saved.len()));
        saved.extend_from_slice(&buf[..keep]);
    }
    let mut output = String::from_utf8_lossy(&saved).into_owned();
    if total > saved.len() {
        output.push_str("\n[Output truncated at 1 MiB]");
    }
    Ok(output)
}
fn redact(text: &str, env: &HashMap<String, String>) -> String {
    let mut text = text.to_owned();
    for (name, value) in env {
        if super::is_sensitive_env_name(name) && !value.is_empty() {
            text = text.replace(value, "[REDACTED]");
        }
    }
    crate::tools::redaction::redact(&text)
}
fn aggregate(stdout: &str, stderr: &str) -> String {
    let mut combined = format!("{stdout}{stderr}");
    if combined.len() > MAX_OUTPUT_SIZE {
        let mut boundary = MAX_OUTPUT_SIZE;
        while !combined.is_char_boundary(boundary) {
            boundary -= 1;
        }
        combined.truncate(boundary);
        combined.push_str("\n[Output truncated at 1 MiB]");
    }
    combined
}

pub async fn execute_command(command: &[String], options: ExecOptions) -> Result<ExecOutput> {
    if command.is_empty() {
        return Err(CortexError::InvalidInput("Empty command".into()));
    }
    if !options.cwd.is_dir() {
        return Err(CortexError::tool_execution(
            "Execute",
            "Working directory is unavailable",
        ));
    }
    let prepared = super::policy::prepare(command, &options).await?;
    let mut cmd = Command::new(&prepared.program);
    cmd.args(&prepared.args)
        .current_dir(&options.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env_clear()
        .envs(build_safe_environment(&options.env));
    // Only backend-generated control variables are applied after final filtering.
    cmd.envs(prepared.env);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.as_std_mut().process_group(0);
    }
    let start = Instant::now();
    let mut child = cmd
        .spawn()
        .map_err(|_| CortexError::tool_execution("Execute", "Command could not be started"))?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let group = cortex_sandbox::process_group::ProcessGroup::new(
        child
            .id()
            .ok_or_else(|| std::io::Error::other("Missing child ID"))?,
    )?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("Missing stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("Missing stderr"))?;
    // Both pipes are drained concurrently and never accumulated without a bound.
    let result = tokio::time::timeout(options.timeout, async {
        tokio::try_join!(capture(stdout), capture(stderr), child.wait())
    })
    .await;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    group.terminate().map_err(|_| {
        CortexError::tool_execution("Execute", "Process-tree termination could not be confirmed")
    })?;
    match result {
        Ok(Ok((stdout, stderr, status))) => {
            let stdout = if options.capture_output {
                redact(&stdout, &options.env)
            } else {
                String::new()
            };
            let stderr = if options.capture_output {
                redact(&stderr, &options.env)
            } else {
                String::new()
            };
            Ok(ExecOutput {
                aggregated: aggregate(&stdout, &stderr),
                stdout,
                stderr,
                exit_code: status.code().unwrap_or(-1),
                duration: start.elapsed(),
                timed_out: false,
            })
        }
        Ok(Err(_)) => Err(CortexError::tool_execution(
            "Execute",
            "Command output or exit status could not be read",
        )),
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Ok(ExecOutput {
                stdout: String::new(),
                stderr: String::new(),
                aggregated: "Command timed out; execution was cancelled".into(),
                exit_code: -1,
                duration: start.elapsed(),
                timed_out: true,
            })
        }
    }
}

/// Deliver bounded, fully redacted output after capture. Holding incomplete text
/// until exit prevents a secret split across pipe reads from escaping redaction.
pub async fn execute_command_streaming(
    command: &[String],
    options: ExecOptions,
    sender: mpsc::Sender<OutputChunk>,
) -> Result<ExecOutput> {
    let output = execute_command(command, options).await?;
    if !output.stdout.is_empty() {
        let _ = sender.try_send(OutputChunk::Stdout(output.stdout.clone()));
    }
    if !output.stderr.is_empty() {
        let _ = sender.try_send(OutputChunk::Stderr(output.stderr.clone()));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_execute_echo() {
        let output = execute_command(
            &["echo".into(), "hello".into()],
            ExecOptions {
                sandbox_policy: SandboxPolicy::DangerFullAccess,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("hello"));
    }
    #[tokio::test]
    async fn test_execute_timeout() {
        let output = execute_command(
            &["sleep".into(), "10".into()],
            ExecOptions {
                timeout: Duration::from_millis(100),
                sandbox_policy: SandboxPolicy::DangerFullAccess,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(output.timed_out);
    }
}
