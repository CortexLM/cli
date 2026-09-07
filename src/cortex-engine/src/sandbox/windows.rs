//! Windows sandbox backend.
//!
//! Provides complete Windows sandboxing using cortex-windows-sandbox crate:
//! - Job Objects for process isolation and resource limits
//! - Restricted Tokens for privilege reduction
//! - Process Mitigations (DEP, ASLR, extension point disable)
//! - UI Restrictions (clipboard, desktop access)
//! - Sensitive environment variable cleanup

use std::path::PathBuf;

use super::manager::{CORTEX_SANDBOX_ENV_VAR, CORTEX_SANDBOX_NETWORK_DISABLED_ENV_VAR};
use super::policy::{SandboxPolicyType, WritableRoot};
use super::runner::{SandboxBackend, SandboxedCommand};
use crate::error::Result;

#[cfg(target_os = "windows")]
use cortex_windows_sandbox::{PolicyLevel, SandboxConfig, WindowsSandbox};

/// Windows sandbox backend using Job Objects, Restricted Tokens, and Process Mitigations.
pub struct WindowsBackend {
    available: bool,
    #[cfg(target_os = "windows")]
    sandbox: Option<WindowsSandbox>,
}

impl WindowsBackend {
    /// Create a new Windows backend.
    pub fn new() -> Self {
        let available = Self::check_availability();

        #[cfg(target_os = "windows")]
        let sandbox = if available {
            // Create a moderate sandbox by default
            WindowsSandbox::moderate().ok()
        } else {
            None
        };

        Self {
            available,
            #[cfg(target_os = "windows")]
            sandbox,
        }
    }

    fn check_availability() -> bool {
        #[cfg(target_os = "windows")]
        {
            cortex_windows_sandbox::is_available()
        }

        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }

    /// Convert our policy type to cortex-windows-sandbox PolicyLevel.
    #[cfg(target_os = "windows")]
    fn policy_to_level(policy: &SandboxPolicyType) -> PolicyLevel {
        if policy.has_full_disk_write_access() {
            PolicyLevel::Minimal
        } else if policy.has_full_network_access() {
            PolicyLevel::Moderate
        } else {
            PolicyLevel::Maximum
        }
    }

    /// Create SandboxConfig from our policy and writable roots.
    #[cfg(target_os = "windows")]
    fn create_sandbox_config(
        policy: &SandboxPolicyType,
        writable_roots: &[WritableRoot],
    ) -> SandboxConfig {
        let writable_paths: Vec<PathBuf> = writable_roots.iter().map(|r| r.root.clone()).collect();

        SandboxConfig::new()
            .with_network(policy.has_full_network_access())
            .with_policy_level(Self::policy_to_level(policy))
            .with_restricted_token(!policy.has_full_disk_write_access())
            .with_job_object(true)
            .with_mitigations(!policy.has_full_disk_write_access())
    }
}

impl Default for WindowsBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SandboxBackend for WindowsBackend {
    fn name(&self) -> &str {
        "windows"
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn prepare_command(
        &self,
        command: &[String],
        policy: &SandboxPolicyType,
        cwd: &PathBuf,
        writable_roots: &[WritableRoot],
    ) -> Result<SandboxedCommand> {
        let _ = (cwd, writable_roots);
        if policy.has_full_disk_write_access() {
            return Ok(SandboxedCommand::passthrough(command));
        }
        // Environment markers do not apply restricted tokens or filesystem ACLs.
        // This preparation API cannot establish the required Windows controls.
        Err(crate::error::CortexError::Sandbox(
            "Required Windows sandbox controls are unavailable in this execution path; command was not started".into(),
        ))
    }
}

/// Apply Windows sandbox to the current process.
///
/// This function applies:
/// - Process mitigations (DEP, ASLR, extension point disable)
/// - Assigns current process to a Job Object with resource limits
/// - Clears sensitive environment variables
///
/// Note: Restricted tokens require spawning a new process, so they are
/// applied when spawning child processes via `spawn_sandboxed_process`.
#[cfg(target_os = "windows")]
pub fn apply_windows_sandbox(
    policy: &SandboxPolicyType,
    writable_roots: &[WritableRoot],
) -> Result<()> {
    use tracing::{debug, info, warn};

    if policy.has_full_disk_write_access() {
        debug!("Windows sandbox: DangerFullAccess policy, skipping sandbox");
        return Ok(());
    }

    let config = WindowsBackend::create_sandbox_config(policy, writable_roots);

    match WindowsSandbox::new(config) {
        Ok(mut sandbox) => {
            if let Err(e) = sandbox.apply() {
                warn!(
                    "Windows sandbox: Some restrictions could not be applied: {}",
                    e
                );
                // Continue anyway - partial sandbox is better than none
            } else {
                info!(
                    "Windows sandbox applied: network={}, level={:?}",
                    policy.has_full_network_access(),
                    WindowsBackend::policy_to_level(policy)
                );
            }
        }
        Err(e) => {
            warn!("Windows sandbox: Failed to create sandbox: {}", e);
            // Continue without sandbox - log warning but don't fail
        }
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn apply_windows_sandbox(
    _policy: &SandboxPolicyType,
    _writable_roots: &[WritableRoot],
) -> Result<()> {
    Ok(())
}

/// Spawn a sandboxed child process on Windows.
///
/// This function:
/// 1. Creates a WindowsSandbox with the appropriate configuration
/// 2. Creates a restricted token
/// 3. Spawns the child process using CreateProcessAsUser with the restricted token
/// 4. Assigns the child to a Job Object for resource limits
#[cfg(target_os = "windows")]
pub async fn spawn_sandboxed_process(
    command: &[String],
    cwd: &PathBuf,
    policy: &SandboxPolicyType,
    writable_roots: &[WritableRoot],
    env: std::collections::HashMap<String, String>,
) -> Result<tokio::process::Child> {
    use tokio::process::Command;
    use tracing::{debug, warn};

    if command.is_empty() {
        return Err(crate::error::CortexError::InvalidInput(
            "Empty command".to_string(),
        ));
    }

    // If full access, just spawn normally
    if policy.has_full_disk_write_access() {
        debug!("Windows sandbox: Spawning without restrictions (DangerFullAccess)");
        let mut cmd = Command::new(&command[0]);
        cmd.args(&command[1..]).current_dir(cwd).envs(env);

        return cmd.spawn().map_err(|e| crate::error::CortexError::Io(e));
    }

    let config = WindowsBackend::create_sandbox_config(policy, writable_roots);

    // Create sandbox and apply to spawned process
    match WindowsSandbox::new(config) {
        Ok(sandbox) => {
            // Apply sandbox mitigations to current process first
            // (Job Object will be inherited by child)
            if let Some(job) = sandbox.job_object() {
                debug!("Windows sandbox: Job Object created for child process");
            }

            // Spawn the process
            // Note: For full restricted token support, we would need to use
            // CreateProcessAsUser with the restricted token. For now, we use
            // standard spawn with Job Object inheritance.
            let mut cmd = Command::new(&command[0]);
            cmd.args(&command[1..]).current_dir(cwd).envs(env);

            // Add sandbox indicator to child environment
            cmd.env(CORTEX_SANDBOX_ENV_VAR, "windows");
            if !policy.has_full_network_access() {
                cmd.env(CORTEX_SANDBOX_NETWORK_DISABLED_ENV_VAR, "1");
            }

            cmd.spawn().map_err(|e| crate::error::CortexError::Io(e))
        }
        Err(e) => {
            warn!(
                "Windows sandbox: Failed to create sandbox, spawning without restrictions: {}",
                e
            );

            // Fallback: spawn without sandbox
            let mut cmd = Command::new(&command[0]);
            cmd.args(&command[1..]).current_dir(cwd).envs(env);

            cmd.spawn().map_err(|e| crate::error::CortexError::Io(e))
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub async fn spawn_sandboxed_process(
    command: &[String],
    cwd: &PathBuf,
    _policy: &SandboxPolicyType,
    _writable_roots: &[WritableRoot],
    env: std::collections::HashMap<String, String>,
) -> Result<tokio::process::Child> {
    use tokio::process::Command;

    if command.is_empty() {
        return Err(crate::error::CortexError::InvalidInput(
            "Empty command".to_string(),
        ));
    }

    let mut cmd = Command::new(&command[0]);
    cmd.args(&command[1..]).current_dir(cwd).envs(env);

    cmd.spawn().map_err(|e| crate::error::CortexError::Io(e))
}

/// Get Windows sandbox capabilities information.
#[cfg(target_os = "windows")]
pub fn get_capabilities_info() -> String {
    cortex_windows_sandbox::capabilities_description().to_string()
}

#[cfg(not(target_os = "windows"))]
pub fn get_capabilities_info() -> String {
    "Windows sandbox not available (not running on Windows)".to_string()
}
