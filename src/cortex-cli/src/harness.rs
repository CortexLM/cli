//! Shared interactive/`run` harness flags (worktree, plugin-dir, bash-edit-diff).

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Opt-in Execute/Bash file diffs via process env (read by the tool handler).
pub fn enable_bash_edit_diff(on: bool) {
    if on {
        // SAFETY: set once before the agent loop; tests that fork should set this first.
        unsafe {
            std::env::set_var("CORTEX_BASH_EDIT_DIFF", "1");
        }
    }
}

/// Isolate the session cwd into a git worktree when `--worktree` is present.
pub fn apply_worktree(cwd: &Path, worktree: Option<&PathBuf>) -> Result<PathBuf> {
    let Some(flag) = worktree else {
        return Ok(cwd.to_path_buf());
    };
    let dest = if flag.as_os_str().is_empty() {
        None
    } else {
        Some(flag.as_path())
    };
    let id = format!("{}", std::process::id());
    let isolated = cortex_engine::worktree::isolate_worktree(cwd, dest, &id)?;
    std::env::set_current_dir(&isolated.path)?;
    Ok(isolated.path)
}

/// Load user/project plugins plus `--plugin-dir` folders. Failures are warnings.
pub async fn load_plugin_dirs(
    cortex_home: &Path,
    project_root: Option<PathBuf>,
    extra: &[PathBuf],
) {
    if let Err(e) = cortex_engine::plugin::init_with_project_and_dirs(
        cortex_home.to_path_buf(),
        project_root,
        extra.to_vec(),
    )
    .await
    {
        tracing::warn!("Plugin manager was not started: {e}");
        return;
    }
    if let Err(e) = cortex_engine::plugin::discover_and_load().await {
        tracing::warn!("Plugin directory was not loaded: {e}");
    }
}
