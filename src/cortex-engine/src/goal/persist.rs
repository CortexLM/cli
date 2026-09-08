//! `goal.json` next to session metadata.

use std::path::{Path, PathBuf};

use super::types::Goal;
use crate::error::{CortexError, Result};

/// File name stored in a session directory.
pub const GOAL_FILE: &str = "goal.json";

pub fn goal_path(session_dir: impl AsRef<Path>) -> PathBuf {
    session_dir.as_ref().join(GOAL_FILE)
}

/// Load a goal if `goal.json` exists. Missing file is `Ok(None)`.
pub fn load_goal(session_dir: impl AsRef<Path>) -> Result<Option<Goal>> {
    let path = goal_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let goal: Goal = serde_json::from_str(&content).map_err(|e| {
        CortexError::InvalidInput(format!("Failed to parse {}: {e}", path.display()))
    })?;
    Ok(Some(goal))
}

pub fn save_goal(session_dir: impl AsRef<Path>, goal: &Goal) -> Result<()> {
    let dir = session_dir.as_ref();
    std::fs::create_dir_all(dir)?;
    let path = goal_path(dir);
    let content = serde_json::to_string_pretty(goal)
        .map_err(|e| CortexError::InvalidInput(format!("Failed to serialize goal: {e}")))?;
    atomic_write(&path, content.as_bytes())
}

pub fn clear_goal(session_dir: impl AsRef<Path>) -> Result<()> {
    let path = goal_path(session_dir);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        CortexError::InvalidInput(format!("Cannot write goal to {}", path.display()))
    })?;
    let tmp = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("goal"),
        std::process::id()
    ));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        CortexError::from(e)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal::types::GoalState;

    #[test]
    fn roundtrip_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_goal(dir.path()).unwrap().is_none());

        let goal = Goal::new("persist me");
        save_goal(dir.path(), &goal).unwrap();
        let loaded = load_goal(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.objective, "persist me");
        assert_eq!(loaded.state, GoalState::Active);
        assert_eq!(loaded.id, goal.id);

        clear_goal(dir.path()).unwrap();
        assert!(load_goal(dir.path()).unwrap().is_none());
    }
}
