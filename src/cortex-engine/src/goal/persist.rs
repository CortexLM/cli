//! `goal.json` next to session metadata.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::types::Goal;
use crate::error::{CortexError, Result};

/// File name stored in a session directory.
pub const GOAL_FILE: &str = "goal.json";

/// Suffix for a quarantined unreadable `goal.json`.
pub const GOAL_CORRUPT_SUFFIX: &str = "json.corrupt";

/// Outcome of reading the session goal file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalLoad {
    /// No `goal.json` on disk.
    Missing,
    /// A persistable goal.
    Loaded(Goal),
    /// File existed but was unreadable; it was moved aside so resume can continue.
    Quarantined { reason: String },
}

pub fn goal_path(session_dir: impl AsRef<Path>) -> PathBuf {
    session_dir.as_ref().join(GOAL_FILE)
}

/// Load a goal if `goal.json` exists. Missing or quarantined files are `Ok(None)`.
pub fn load_goal(session_dir: impl AsRef<Path>) -> Result<Option<Goal>> {
    match load_goal_report(session_dir)? {
        GoalLoad::Loaded(goal) => Ok(Some(goal)),
        GoalLoad::Missing | GoalLoad::Quarantined { .. } => Ok(None),
    }
}

/// Load with an explicit missing / loaded / quarantined result for resume UX.
pub fn load_goal_report(session_dir: impl AsRef<Path>) -> Result<GoalLoad> {
    let dir = session_dir.as_ref();
    cleanup_stale_tmps(dir);
    let path = goal_path(dir);
    if !path.exists() {
        return Ok(GoalLoad::Missing);
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) => {
            return Ok(quarantine(
                &path,
                &format!("could not read goal.json ({error})"),
            ));
        }
    };
    let mut goal: Goal = match serde_json::from_str(&content) {
        Ok(goal) => goal,
        Err(_) => {
            return Ok(quarantine(
                &path,
                "goal.json was unreadable. Moved aside so the session can resume.",
            ));
        }
    };
    goal.sanitize();
    if !goal.is_persistable() {
        return Ok(quarantine(
            &path,
            "goal.json was missing an objective. Moved aside so the session can resume.",
        ));
    }
    Ok(GoalLoad::Loaded(goal))
}

pub fn save_goal(session_dir: impl AsRef<Path>, goal: &Goal) -> Result<()> {
    let mut goal = goal.clone();
    goal.sanitize();
    if !goal.is_persistable() {
        return Err(CortexError::InvalidInput(
            "Goal objective cannot be empty.".to_string(),
        ));
    }
    let dir = session_dir.as_ref();
    std::fs::create_dir_all(dir)?;
    let path = goal_path(dir);
    let content = serde_json::to_string_pretty(&goal)
        .map_err(|e| CortexError::InvalidInput(format!("Failed to persist goal: {e}")))?;
    durable_atomic_write(&path, content.as_bytes())
}

pub fn clear_goal(session_dir: impl AsRef<Path>) -> Result<()> {
    let path = goal_path(session_dir);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

fn quarantine(path: &Path, reason: &str) -> GoalLoad {
    let dest = path.with_extension(GOAL_CORRUPT_SUFFIX);
    let _ = std::fs::rename(path, &dest);
    GoalLoad::Quarantined {
        reason: reason.to_string(),
    }
}

fn cleanup_stale_tmps(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(".goal.json.tmp.") || name.starts_with(".goal.tmp.") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn durable_atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        CortexError::InvalidInput(format!("Cannot write goal to {}", path.display()))
    })?;
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("goal"),
        std::process::id()
    ));
    let write_tmp = || -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    };
    if let Err(error) = write_tmp() {
        let _ = std::fs::remove_file(&tmp);
        return Err(CortexError::from(error));
    }
    if let Err(error) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(CortexError::from(error));
    }
    #[cfg(unix)]
    {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal::types::GoalState;

    #[test]
    fn roundtrip_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            load_goal_report(dir.path()).unwrap(),
            GoalLoad::Missing
        ));

        let goal = Goal::new("persist me");
        save_goal(dir.path(), &goal).unwrap();
        let loaded = load_goal(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.objective, "persist me");
        assert_eq!(loaded.state, GoalState::Active);
        assert_eq!(loaded.id, goal.id);
        assert_eq!(
            loaded.schema_version,
            crate::goal::types::GOAL_SCHEMA_VERSION
        );

        clear_goal(dir.path()).unwrap();
        assert!(load_goal(dir.path()).unwrap().is_none());
    }

    #[test]
    fn corrupt_json_is_quarantined_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let path = goal_path(dir.path());
        std::fs::write(&path, "{not-json").unwrap();
        match load_goal_report(dir.path()).unwrap() {
            GoalLoad::Quarantined { reason } => {
                assert!(reason.contains("unreadable"), "{reason}");
            }
            other => panic!("expected quarantine, got {other:?}"),
        }
        assert!(!path.exists());
        assert!(dir.path().join("goal.json.corrupt").exists());
        assert!(load_goal(dir.path()).unwrap().is_none());
    }

    #[test]
    fn empty_objective_is_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        let mut goal = Goal::new("x");
        goal.objective = String::new();
        let path = goal_path(dir.path());
        std::fs::write(&path, serde_json::to_string(&goal).unwrap()).unwrap();
        assert!(matches!(
            load_goal_report(dir.path()).unwrap(),
            GoalLoad::Quarantined { .. }
        ));
        assert!(load_goal(dir.path()).unwrap().is_none());
    }

    #[test]
    fn zero_budget_is_repaired_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let mut goal = Goal::new("keep going");
        goal.turn_budget = 0;
        let path = goal_path(dir.path());
        std::fs::write(&path, serde_json::to_string(&goal).unwrap()).unwrap();
        let loaded = load_goal(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.turn_budget, crate::goal::types::DEFAULT_TURN_BUDGET);
    }

    #[test]
    fn save_rejects_empty_objective() {
        let dir = tempfile::tempdir().unwrap();
        let mut goal = Goal::new("x");
        goal.objective.clear();
        assert!(save_goal(dir.path(), &goal).is_err());
    }
}
