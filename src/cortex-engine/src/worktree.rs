//! Git worktree isolation for parallel Code agent runs.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{CortexError, Result};

/// Isolated worktree created for a Code session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsolatedWorktree {
    /// Path the agent should use as cwd.
    pub path: PathBuf,
    /// Branch checked out in the worktree.
    pub branch: String,
    /// Parent repository root.
    pub repo: PathBuf,
    /// True when this process created the worktree (cleanup may remove it).
    pub created: bool,
}

/// Session metadata recorded next to an isolated worktree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeSessionMeta {
    pub path: PathBuf,
    pub branch: String,
    pub repo: PathBuf,
}

/// Create or reuse an isolated git worktree.
///
/// `dest` is an optional target path. When omitted, a directory named
/// `.cortex-worktrees/<id>` is created beside the repository.
pub fn isolate_worktree(cwd: &Path, dest: Option<&Path>, id: &str) -> Result<IsolatedWorktree> {
    let repo = git_root(cwd)?;
    let path = match dest {
        Some(p) if p.as_os_str().is_empty() || p == Path::new("auto") => {
            default_worktree_path(&repo, id)
        }
        Some(p) => {
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                cwd.join(p)
            }
        }
        None => default_worktree_path(&repo, id),
    };

    if path.exists() {
        if !path.is_dir() {
            return Err(CortexError::InvalidInput(
                "Worktree path exists and is not a directory".into(),
            ));
        }
        return reuse_linked_worktree(&repo, &path);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(CortexError::Io)?;
    }

    let branch = format!("cortex/{}", sanitize_id(id));
    let output = Command::new("git")
        .arg("worktree")
        .arg("add")
        .arg("-b")
        .arg(&branch)
        .arg(&path)
        .arg("HEAD")
        .current_dir(&repo)
        .output()
        .map_err(|e| CortexError::Internal(format!("Could not start git worktree: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CortexError::InvalidInput(format!(
            "Could not create an isolated worktree: {}",
            first_line(&stderr)
        )));
    }

    let isolated = IsolatedWorktree {
        path: path.clone(),
        branch,
        repo,
        created: true,
    };
    write_session_meta(&isolated)?;
    Ok(isolated)
}

/// Remove a worktree this process created. Leaves user trees untouched.
pub fn cleanup_worktree(worktree: &IsolatedWorktree) -> Result<()> {
    if !worktree.created {
        return Ok(());
    }
    let output = Command::new("git")
        .arg("worktree")
        .arg("remove")
        .arg("--force")
        .arg(&worktree.path)
        .current_dir(&worktree.repo)
        .output()
        .map_err(|e| CortexError::Internal(format!("Could not remove git worktree: {e}")))?;
    if !output.status.success() {
        return Err(CortexError::InvalidInput(
            "Could not remove the isolated worktree".into(),
        ));
    }
    Ok(())
}

fn reuse_linked_worktree(repo: &Path, path: &Path) -> Result<IsolatedWorktree> {
    if same_dir(path, repo) {
        return Err(CortexError::InvalidInput(
            "Worktree path is the primary checkout, not a linked worktree".into(),
        ));
    }
    let listed = git_worktree_paths(repo)?;
    if !listed.iter().any(|listed_path| same_dir(listed_path, path)) {
        return Err(CortexError::InvalidInput(
            "Worktree path is not a linked worktree of this repository".into(),
        ));
    }
    Ok(IsolatedWorktree {
        path: path.to_path_buf(),
        branch: current_branch(path).unwrap_or_else(|_| "HEAD".into()),
        repo: repo.to_path_buf(),
        created: false,
    })
}

fn git_worktree_paths(repo: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo)
        .output()
        .map_err(|e| CortexError::Internal(format!("Could not list git worktrees: {e}")))?;
    if !output.status.success() {
        return Err(CortexError::InvalidInput(
            "Could not list git worktrees for this repository".into(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect())
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => a == b,
    }
}

fn default_worktree_path(repo: &Path, id: &str) -> PathBuf {
    repo.join(".cortex-worktrees").join(sanitize_id(id))
}

fn git_root(cwd: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .map_err(|_| CortexError::InvalidInput("Not a git repository".into()))?;
    if !output.status.success() {
        return Err(CortexError::InvalidInput(
            "Worktree isolation requires a git repository".into(),
        ));
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(path))
}

fn current_branch(repo: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo)
        .output()
        .map_err(CortexError::Io)?;
    if !output.status.success() {
        return Err(CortexError::InvalidInput(
            "Could not read git branch".into(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn write_session_meta(worktree: &IsolatedWorktree) -> Result<()> {
    let meta = WorktreeSessionMeta {
        path: worktree.path.clone(),
        branch: worktree.branch.clone(),
        repo: worktree.repo.clone(),
    };
    let path = worktree.path.join(".cortex-worktree.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&meta)?).map_err(CortexError::Io)?;
    Ok(())
}

fn sanitize_id(id: &str) -> String {
    let cleaned: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .take(32)
        .collect();
    if cleaned.is_empty() {
        "session".into()
    } else {
        cleaned
    }
}

fn first_line(s: &str) -> String {
    s.lines()
        .next()
        .unwrap_or("git worktree failed")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn git(cwd: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    fn init_repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        git(tmp.path(), &["init"]);
        git(tmp.path(), &["checkout", "-b", "main"]);
        git(
            tmp.path(),
            &["config", "user.email", "dev@cortex.foundation"],
        );
        git(tmp.path(), &["config", "user.name", "Cortex"]);
        std::fs::write(tmp.path().join("README.md"), "hello\n").unwrap();
        git(tmp.path(), &["add", "README.md"]);
        git(tmp.path(), &["commit", "-m", "init"]);
        tmp
    }

    #[test]
    fn isolate_creates_a_separate_worktree() {
        let repo = init_repo();
        let isolated = isolate_worktree(repo.path(), None, "sess-1").unwrap();
        assert!(isolated.created);
        assert!(isolated.path.join("README.md").exists());
        assert_ne!(isolated.path, repo.path());
        assert!(isolated.path.join(".cortex-worktree.json").exists());
        cleanup_worktree(&isolated).unwrap();
        assert!(!isolated.path.exists());
    }

    #[test]
    fn missing_git_repo_fails_closed() {
        let tmp = TempDir::new().unwrap();
        let err = isolate_worktree(tmp.path(), None, "x").unwrap_err();
        assert!(err.to_string().to_lowercase().contains("git"), "{}", err);
    }

    #[test]
    fn existing_unrelated_directory_is_rejected() {
        let repo = init_repo();
        let other = TempDir::new().unwrap();
        let err = isolate_worktree(repo.path(), Some(other.path()), "sess-other").unwrap_err();
        assert!(err.to_string().contains("linked worktree"), "{err}");
    }

    #[test]
    fn existing_linked_worktree_is_reused() {
        let repo = init_repo();
        let linked = repo.path().join("linked-wt");
        git(
            repo.path(),
            &[
                "worktree",
                "add",
                "-b",
                "cortex-reuse",
                linked.to_str().unwrap(),
                "HEAD",
            ],
        );
        let isolated = isolate_worktree(repo.path(), Some(&linked), "sess-reuse").unwrap();
        assert!(!isolated.created);
        assert!(same_dir(&isolated.path, &linked));
        assert_eq!(isolated.branch, "cortex-reuse");
    }

    #[test]
    fn primary_checkout_is_not_a_linked_worktree() {
        let repo = init_repo();
        let err = isolate_worktree(repo.path(), Some(repo.path()), "sess-main").unwrap_err();
        assert!(err.to_string().contains("primary checkout"), "{err}");
    }
}
