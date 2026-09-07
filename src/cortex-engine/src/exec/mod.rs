//! Command execution with sandboxing.

mod environment;
mod output;
mod policy;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod runner;
pub use environment::{build_safe_environment, is_sensitive_env_name};

pub use output::OutputCapture;
pub use runner::{
    ExecOptions, ExecOutput, OutputChunk, execute_command, execute_command_streaming,
};

use std::time::Duration;

/// Default command timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Maximum output size to capture.
pub const MAX_OUTPUT_SIZE: usize = 1024 * 1024; // 1MB
