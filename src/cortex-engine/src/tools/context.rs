//! Tool execution context.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cortex_common::normalize_path as normalize_path_util;
use cortex_protocol::SandboxPolicy;
use tokio::sync::mpsc;

use crate::integrations::LspIntegration;

#[derive(Clone)]
struct ApprovedCall {
    name: String,
    arguments: serde_json::Value,
    cwd: PathBuf,
    used: Arc<AtomicBool>,
}

/// Output chunk from tool execution
#[derive(Debug, Clone)]
pub enum ToolOutputChunk {
    Stdout(String),
    Stderr(String),
}

/// Context for tool execution.
#[derive(Clone)]
pub struct ToolContext {
    /// Current working directory.
    pub cwd: PathBuf,
    /// Sandbox policy.
    pub sandbox_policy: SandboxPolicy,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Turn ID.
    pub turn_id: String,
    /// Conversation ID.
    pub conversation_id: String,
    /// Whether to auto-approve.
    pub auto_approve: bool,
    /// Call ID for the current tool execution.
    pub call_id: String,
    /// Optional sender for streaming output chunks.
    pub output_sender: Option<mpsc::Sender<(String, ToolOutputChunk)>>,
    /// LSP integration.
    pub lsp: Option<Arc<LspIntegration>>,
    /// Only roots explicitly opened by the trusted caller, never sandbox cache roots.
    opened_roots: Vec<PathBuf>,
    read_only: bool,
    child: bool,
    approved_calls: Vec<ApprovedCall>,
    denied_tools: Vec<String>,
    allowed_fetch_hosts: Vec<String>,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("cwd", &self.cwd)
            .field("sandbox_policy", &self.sandbox_policy)
            .field("env_keys", &self.env.keys().collect::<Vec<_>>())
            .field("turn_id", &self.turn_id)
            .field("conversation_id", &self.conversation_id)
            .field("auto_approve", &self.auto_approve)
            .field("call_id", &self.call_id)
            .field("has_output_sender", &self.output_sender.is_some())
            .finish()
    }
}

impl ToolContext {
    /// Create a new tool context.
    pub fn new(cwd: PathBuf) -> Self {
        // Build environment with non-interactive settings
        let mut env = crate::exec::build_safe_environment(&HashMap::new());

        // Force non-interactive mode for common tools
        env.insert("CI".to_string(), "true".to_string());
        env.insert("DEBIAN_FRONTEND".to_string(), "noninteractive".to_string());
        env.insert("NPM_CONFIG_YES".to_string(), "true".to_string());
        env.insert(
            "YARN_ENABLE_IMMUTABLE_INSTALLS".to_string(),
            "false".to_string(),
        );
        env.insert("NO_COLOR".to_string(), "1".to_string());
        env.insert("TERM".to_string(), "dumb".to_string());
        env.insert("NONINTERACTIVE".to_string(), "1".to_string());
        // Force create-next-app to not ask questions
        env.insert("npm_config_yes".to_string(), "true".to_string());

        let opened_roots = cwd.canonicalize().ok().into_iter().collect();
        Self {
            cwd,
            sandbox_policy: SandboxPolicy::default(),
            env,
            turn_id: String::new(),
            conversation_id: String::new(),
            auto_approve: false,
            call_id: String::new(),
            output_sender: None,
            lsp: None,
            opened_roots,
            read_only: false,
            child: false,
            approved_calls: Vec::new(),
            denied_tools: Vec::new(),
            allowed_fetch_hosts: vec![
                "api.cortex.foundation".into(),
                "auth.cortex.foundation".into(),
                "software.cortex.foundation".into(),
            ],
        }
    }

    /// Add a root that the operator explicitly opened. Sandbox temp/cache paths
    /// do not grant file-tool authority.
    pub fn with_opened_root(mut self, root: PathBuf) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|_| "Opened root does not exist")?;
        if !root.is_dir() {
            return Err("Opened root must be a directory".into());
        }
        self.opened_roots.push(root);
        Ok(self)
    }

    /// Set the trusted harness mode. Environment variables can only tighten it.
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Approve exactly this tool and argument value at the current cwd.
    /// This does not approve a different call nested inside Task or Batch.
    pub fn with_approved_tool_call(
        mut self,
        name: impl Into<String>,
        arguments: &serde_json::Value,
    ) -> Self {
        self.approved_calls.push(ApprovedCall {
            name: name.into(),
            arguments: arguments.clone(),
            cwd: self.cwd.clone(),
            used: Arc::new(AtomicBool::new(false)),
        });
        self
    }

    /// Explicit deny rules take precedence even over unattended auto-approval.
    pub fn with_denied_tools(mut self, names: Vec<String>) -> Self {
        self.denied_tools.extend(names);
        self
    }

    /// Derive a child with identical roots, mode, and sandbox but no one-call grants.
    pub fn for_child(mut self) -> Self {
        self.child = true;
        self.approved_calls.clear();
        self
    }

    /// Explicit operator allowlist extension; never populated from tool arguments.
    pub fn with_allowed_fetch_host(mut self, host: impl Into<String>) -> Self {
        self.allowed_fetch_hosts
            .push(host.into().to_ascii_lowercase());
        self
    }

    pub fn allowed_fetch_hosts(&self) -> &[String] {
        &self.allowed_fetch_hosts
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
            || matches!(self.sandbox_policy, SandboxPolicy::ReadOnly)
            || self.env.get("CORTEX_SPEC_MODE").is_some_and(|v| v == "1")
            || self
                .env
                .get("CORTEX_OPERATION_MODE")
                .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "spec" | "plan" | "ask"))
    }

    pub fn is_child(&self) -> bool {
        self.child
            || self.env.get("CORTEX_CHILD_TASK").is_some_and(|v| v == "1")
            || self.conversation_id.starts_with("sub_")
            || self.conversation_id.starts_with("task_")
    }

    pub(crate) fn is_tool_denied(&self, name: &str) -> bool {
        self.denied_tools
            .iter()
            .any(|n| n.eq_ignore_ascii_case(name))
    }

    pub(crate) fn is_approved(&self, name: &str, arguments: &serde_json::Value) -> bool {
        self.auto_approve
            || self.approved_calls.iter().any(|call| {
                call.name == name
                    && call.arguments == *arguments
                    && call.cwd == self.cwd
                    && !call.used.load(Ordering::Acquire)
            })
    }

    pub(crate) fn consume_approval(&self, name: &str, arguments: &serde_json::Value) -> bool {
        self.auto_approve
            || self.approved_calls.iter().any(|call| {
                call.name == name
                    && call.arguments == *arguments
                    && call.cwd == self.cwd
                    && call
                        .used
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
            })
    }

    /// Redact known sensitive override values as well as recognizable secret syntax.
    pub fn redact(&self, text: &str) -> String {
        let mut redacted = text.to_owned();
        for (name, value) in &self.env {
            if crate::exec::is_sensitive_env_name(name) && !value.is_empty() {
                redacted = redacted.replace(value, "[REDACTED]");
            }
        }
        super::redaction::redact(&redacted)
    }

    /// Set LSP integration.
    pub fn with_lsp(mut self, lsp: Arc<LspIntegration>) -> Self {
        self.lsp = Some(lsp);
        self
    }

    /// Set the sandbox policy.
    pub fn with_sandbox_policy(mut self, policy: SandboxPolicy) -> Self {
        self.sandbox_policy = policy;
        self
    }

    /// Set turn ID.
    pub fn with_turn_id(mut self, turn_id: impl Into<String>) -> Self {
        self.turn_id = turn_id.into();
        self
    }

    /// Set conversation ID.
    pub fn with_conversation_id(mut self, id: impl Into<String>) -> Self {
        self.conversation_id = id.into();
        self
    }

    /// Set auto-approve flag.
    pub fn with_auto_approve(mut self, auto_approve: bool) -> Self {
        self.auto_approve = auto_approve;
        self
    }

    /// Set call ID for the current tool execution.
    pub fn with_call_id(mut self, call_id: impl Into<String>) -> Self {
        self.call_id = call_id.into();
        self
    }

    /// Set output sender for streaming.
    pub fn with_output_sender(mut self, sender: mpsc::Sender<(String, ToolOutputChunk)>) -> Self {
        self.output_sender = Some(sender);
        self
    }

    /// Send an output chunk if sender is available.
    pub async fn send_output(&self, chunk: ToolOutputChunk) {
        if let Some(sender) = &self.output_sender {
            let chunk = match chunk {
                ToolOutputChunk::Stdout(s) => ToolOutputChunk::Stdout(self.redact(&s)),
                ToolOutputChunk::Stderr(s) => ToolOutputChunk::Stderr(self.redact(&s)),
            };
            let _ = sender.send((self.call_id.clone(), chunk)).await;
        }
    }

    /// Resolve a path relative to cwd with path traversal protection.
    ///
    /// This method:
    /// 1. Joins relative paths to the cwd
    /// 2. Normalizes the path to resolve `.` and `..` components
    /// 3. Validates that the resolved path stays within the cwd (for relative paths)
    ///
    /// # Arguments
    /// * `path` - The path to resolve (can be absolute or relative)
    ///
    /// # Returns
    /// The resolved and normalized path
    pub fn resolve_path(&self, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        let resolved = if p.is_absolute() {
            p
        } else {
            self.cwd.join(&p)
        };

        // Normalize the path to resolve . and .. components
        Self::normalize_path(&resolved)
    }

    /// Resolve and validate a path, ensuring it stays within allowed roots.
    ///
    /// This is a more secure version of `resolve_path` that validates the
    /// resolved path is within the cwd or allowed writable roots.
    ///
    /// # Arguments
    /// * `path` - The path to resolve (can be absolute or relative)
    ///
    /// # Returns
    /// * `Ok(PathBuf)` - The resolved and validated path
    /// * `Err(String)` - If the path would escape allowed directories
    pub fn resolve_and_validate_path(&self, path: &str) -> Result<PathBuf, String> {
        // Never normalize `symlink/..` before filesystem resolution.
        if Path::new(path)
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return Err("Parent traversal is not allowed; use a path inside an opened root".into());
        }
        let resolved = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.cwd.join(path)
        };
        let canonical = canonicalize_create_path(&resolved)?;
        if self
            .opened_roots
            .iter()
            .any(|root| canonical.starts_with(root))
        {
            Ok(canonical)
        } else {
            Err("Path is outside explicitly opened workspace roots".into())
        }
    }

    pub fn resolve_write_path(&self, path: &str) -> Result<PathBuf, String> {
        if self.is_read_only() {
            return Err("Read-only mode prohibits filesystem changes".into());
        }
        let resolved = self.resolve_and_validate_path(path)?;
        for candidate in [Path::new(path), resolved.as_path()] {
            if candidate.components().any(|part| {
                matches!(part, Component::Normal(name) if name == ".git" || name == ".cortex")
            }) {
                return Err("Workspace control metadata is read-only".into());
            }
        }
        Ok(resolved)
    }

    /// Normalize a path by resolving `.` and `..` components without filesystem access.
    fn normalize_path(path: &Path) -> PathBuf {
        normalize_path_util(path)
    }

    /// Check if a path contains path traversal sequences.
    pub fn contains_traversal(path: &str) -> bool {
        let p = Path::new(path);
        p.components().any(|c| matches!(c, Component::ParentDir))
    }
}

/// Canonicalize the nearest existing ancestor, including dangling symlink
/// detection, before appending components of a new nested file.
fn canonicalize_create_path(path: &Path) -> Result<PathBuf, String> {
    let mut ancestor = path.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match std::fs::symlink_metadata(&ancestor) {
            Ok(_) => {
                let mut resolved = ancestor
                    .canonicalize()
                    .map_err(|_| "Cannot resolve file path")?;
                for name in missing.iter().rev() {
                    resolved.push(name);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or("Invalid file path")?
                        .to_os_string(),
                );
                if !ancestor.pop() {
                    return Err("Cannot resolve file path".into());
                }
            }
            Err(_) => return Err("Cannot access file path".into()),
        }
    }
}
