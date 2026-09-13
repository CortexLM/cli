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
    let dest = if flag.as_os_str().is_empty() || flag == Path::new("auto") {
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
        project_root.clone(),
        extra.to_vec(),
    )
    .await
    {
        tracing::warn!("Plugin manager was not started: {e}");
    } else if let Err(e) = cortex_engine::plugin::discover_and_load().await {
        tracing::warn!("Plugin directory was not loaded: {e}");
    }
    // Metadata PluginManager is not the session tool runtime. Extra dirs must
    // also reach the executable (WASM/Node) runtime so tools/hooks/commands load.
    let cwd = project_root.unwrap_or_else(|| cortex_home.to_path_buf());
    cortex_engine::plugin::start_executable_runtime(cwd, extra).await;
}

/// Apply `--bash-edit-diff`, `--worktree`, and `--plugin-dir` to a session cwd.
pub async fn apply_session_harness(
    cwd: &mut PathBuf,
    cortex_home: &Path,
    bash_edit_diff: bool,
    worktree: Option<&PathBuf>,
    plugin_dir: &[PathBuf],
) -> Result<()> {
    enable_bash_edit_diff(bash_edit_diff);
    if worktree.is_some() {
        *cwd = apply_worktree(cwd, worktree)?;
    }
    load_plugin_dirs(cortex_home, Some(cwd.clone()), plugin_dir).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::process::Command;

    struct CwdGuard(PathBuf);
    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    fn init_git_repo(dir: &Path) {
        assert!(
            Command::new("git")
                .args(["init"])
                .current_dir(dir)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args(["-c", "user.email=ci@example.com", "-c", "user.name=ci"])
                .args(["commit", "--allow-empty", "-m", "init"])
                .current_dir(dir)
                .status()
                .unwrap()
                .success()
        );
    }

    #[test]
    fn apply_worktree_without_flag_keeps_cwd() {
        let cwd = PathBuf::from("/tmp");
        let out = apply_worktree(&cwd, None).unwrap();
        assert_eq!(out, cwd);
    }

    #[test]
    #[serial]
    fn apply_worktree_auto_isolates_git_repo() {
        let home = std::env::current_dir().unwrap();
        let _guard = CwdGuard(home);
        let repo = tempfile::tempdir().unwrap();
        init_git_repo(repo.path());
        let dest = PathBuf::from("auto");
        let isolated = apply_worktree(repo.path(), Some(&dest)).unwrap();
        assert!(isolated.exists());
        assert_ne!(isolated, repo.path());
    }

    #[test]
    #[serial]
    fn apply_worktree_explicit_dir() {
        let home = std::env::current_dir().unwrap();
        let _guard = CwdGuard(home);
        let repo = tempfile::tempdir().unwrap();
        init_git_repo(repo.path());
        let dest = repo.path().join("isolated-wt");
        let isolated = apply_worktree(repo.path(), Some(&dest)).unwrap();
        assert_eq!(isolated, dest);
        assert!(dest.is_dir());
    }

    #[test]
    #[serial]
    fn enable_bash_edit_diff_sets_env() {
        unsafe {
            std::env::remove_var("CORTEX_BASH_EDIT_DIFF");
        }
        enable_bash_edit_diff(false);
        assert!(std::env::var("CORTEX_BASH_EDIT_DIFF").is_err());
        enable_bash_edit_diff(true);
        assert_eq!(std::env::var("CORTEX_BASH_EDIT_DIFF").unwrap(), "1");
        unsafe {
            std::env::remove_var("CORTEX_BASH_EDIT_DIFF");
        }
    }

    #[tokio::test]
    #[serial]
    async fn load_plugin_dirs_warns_instead_of_failing() {
        let home = tempfile::tempdir().unwrap();
        let extra = tempfile::tempdir().unwrap();
        let plugin = extra.path().join("sample");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(plugin.join("plugin.json"), "{}").unwrap();
        load_plugin_dirs(
            home.path(),
            Some(home.path().to_path_buf()),
            &[extra.path().to_path_buf()],
        )
        .await;
        apply_session_harness(
            &mut home.path().to_path_buf(),
            home.path(),
            false,
            None,
            &[],
        )
        .await
        .unwrap();
        let extra_path = extra.path().to_path_buf();
        let config =
            cortex_engine::plugin::plugin_config_with_extra_dirs(std::slice::from_ref(&extra_path));
        assert!(
            config.search_paths.iter().any(|path| path == &extra_path),
            "{:?}",
            config.search_paths
        );
    }
}
