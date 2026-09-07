//! Linux Landlock sandbox backend.
//!
//! Uses the cortex-linux-sandbox wrapper binary to apply:
//! - Landlock filesystem restrictions
//! - Seccomp network filtering
//! - Mount namespace for read-only .git/.cortex protection

use std::path::PathBuf;

use super::manager::{
    CORTEX_SANDBOX_CWD_ENV_VAR, CORTEX_SANDBOX_ENV_VAR, CORTEX_SANDBOX_NETWORK_DISABLED_ENV_VAR,
    CORTEX_SANDBOX_POLICY_ENV_VAR,
};
use super::policy::{SandboxPolicyType, WritableRoot};
use super::runner::{SandboxBackend, SandboxedCommand};
use crate::error::Result;

/// Name of the Linux sandbox wrapper binary.
const LINUX_SANDBOX_BINARY: &str = "cortex-linux-sandbox";

/// Reserved first argument that makes a registered Cortex binary act as the wrapper.
pub const SELF_WRAPPER_ARG: &str = "__cortex_linux_sandbox";

static SELF_WRAPPER_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Declare that the running executable dispatches `SELF_WRAPPER_ARG` before
/// argument parsing. Unregistered hosts (test harnesses, other embedders)
/// never re-execute themselves and fail closed without a sibling wrapper.
pub fn enable_self_wrapper() {
    SELF_WRAPPER_ENABLED.store(true, std::sync::atomic::Ordering::Release);
}

/// Landlock sandbox backend.
pub struct LandlockBackend {
    available: bool,
    /// Wrapper binary and whether it is the running executable itself.
    wrapper_path: Option<(PathBuf, bool)>,
}

impl LandlockBackend {
    /// Create a new Landlock backend.
    pub fn new() -> Self {
        let available = Self::check_availability();
        let wrapper_path = Self::find_wrapper_binary();

        Self {
            available,
            wrapper_path,
        }
    }

    fn check_availability() -> bool {
        // Check if Landlock is available by checking kernel version
        // Landlock requires Linux 5.13+
        #[cfg(target_os = "linux")]
        {
            use std::fs;
            if let Ok(version) = fs::read_to_string("/proc/version") {
                // Parse kernel version
                if let Some(ver_str) = version.split_whitespace().nth(2) {
                    let parts: Vec<&str> = ver_str.split('.').collect();
                    if parts.len() >= 2 {
                        if let (Ok(major), Ok(minor)) =
                            (parts[0].parse::<u32>(), parts[1].parse::<u32>())
                        {
                            return major > 5 || (major == 5 && minor >= 13);
                        }
                    }
                }
            }
            false
        }

        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Find the sandbox wrapper: a sibling binary, else the running
    /// executable itself, which every Cortex binary re-executes as the
    /// wrapper when its first argument is `SELF_WRAPPER_ARG`.
    fn find_wrapper_binary() -> Option<(PathBuf, bool)> {
        let exe_path = std::env::current_exe().ok()?;
        let wrapper = exe_path.parent()?.join(LINUX_SANDBOX_BINARY);
        if wrapper.is_file() {
            return Some((wrapper, false));
        }
        SELF_WRAPPER_ENABLED
            .load(std::sync::atomic::Ordering::Acquire)
            .then_some((exe_path, true))
    }

    /// Check if the wrapper binary is available.
    #[allow(dead_code)]
    pub fn has_wrapper(&self) -> bool {
        self.wrapper_path.is_some()
    }
}

impl Default for LandlockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SandboxBackend for LandlockBackend {
    fn name(&self) -> &str {
        "landlock"
    }

    fn is_available(&self) -> bool {
        self.available && self.wrapper_path.is_some()
    }

    fn prepare_command(
        &self,
        command: &[String],
        policy: &SandboxPolicyType,
        cwd: &PathBuf,
        writable_roots: &[WritableRoot],
    ) -> Result<SandboxedCommand> {
        if command.is_empty() {
            return Ok(SandboxedCommand::passthrough(command));
        }

        // Build environment variables
        let mut env = vec![(CORTEX_SANDBOX_ENV_VAR.to_string(), "landlock".to_string())];

        if !policy.has_full_network_access() {
            env.push((
                CORTEX_SANDBOX_NETWORK_DISABLED_ENV_VAR.to_string(),
                "1".to_string(),
            ));
        }

        // Serialize policy to JSON for the wrapper
        let policy_json = serde_json::to_string(policy).unwrap_or_default();
        env.push((
            CORTEX_SANDBOX_POLICY_ENV_VAR.to_string(),
            policy_json.clone(),
        ));
        env.push((
            CORTEX_SANDBOX_CWD_ENV_VAR.to_string(),
            cwd.display().to_string(),
        ));

        // If we have the wrapper binary, use it
        if let Some((wrapper, self_exec)) = &self.wrapper_path {
            let mut args = Vec::new();
            if *self_exec {
                args.push(SELF_WRAPPER_ARG.to_string());
            }
            args.extend([
                "--sandbox-policy-cwd".to_string(),
                cwd.display().to_string(),
                "--sandbox-policy".to_string(),
                policy_json,
            ]);

            // Add writable roots
            for root in writable_roots {
                args.push("--writable-root".to_string());
                args.push(root.root.display().to_string());

                // Add read-only subpaths
                for ro in &root.read_only_subpaths {
                    args.push("--read-only-subpath".to_string());
                    args.push(ro.display().to_string());
                }
            }

            // Add the actual command
            args.push("--".to_string());
            args.extend(command.iter().cloned());

            return Ok(SandboxedCommand {
                program: wrapper.display().to_string(),
                args,
                env,
            });
        }

        Err(crate::error::CortexError::Sandbox(
            "Required sandbox wrapper is unavailable; command was not started".into(),
        ))
    }
}
