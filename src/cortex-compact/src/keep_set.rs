//! Harness compaction keep-set.
//!
//! When compacting, these items are retained:
//! `user_turns`, `last_ask`, `active_plan`, `open_task_ids`,
//! `memory_profile`, `pinned_files`, `open_artifact_ids`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Keep-set keys required by CortexLM/harness.
pub const KEEP_SET_KEYS: &[&str] = &[
    "user_turns",
    "last_ask",
    "active_plan",
    "open_task_ids",
    "memory_profile",
    "pinned_files",
    "open_artifact_ids",
];

/// Items the harness must retain across compaction.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionKeepSet {
    #[serde(default)]
    pub user_turns: Vec<String>,
    #[serde(default)]
    pub last_ask: Option<String>,
    #[serde(default)]
    pub active_plan: Option<String>,
    #[serde(default)]
    pub open_task_ids: BTreeSet<String>,
    #[serde(default)]
    pub memory_profile: Option<String>,
    #[serde(default)]
    pub pinned_files: BTreeSet<String>,
    #[serde(default)]
    pub open_artifact_ids: BTreeSet<String>,
}

impl CompactionKeepSet {
    pub fn keys() -> &'static [&'static str] {
        KEEP_SET_KEYS
    }

    /// Merge keep-set values into a compactable item filter.
    pub fn should_keep_text(&self, text: &str) -> bool {
        if self.user_turns.iter().any(|t| text.contains(t)) {
            return true;
        }
        if self.last_ask.as_ref().is_some_and(|ask| text.contains(ask)) {
            return true;
        }
        if self
            .active_plan
            .as_ref()
            .is_some_and(|plan| text.contains(plan))
        {
            return true;
        }
        if self.open_task_ids.iter().any(|id| text.contains(id)) {
            return true;
        }
        if self
            .memory_profile
            .as_ref()
            .is_some_and(|p| text.contains(p))
        {
            return true;
        }
        if self.pinned_files.iter().any(|f| text.contains(f)) {
            return true;
        }
        if self.open_artifact_ids.iter().any(|id| text.contains(id)) {
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_set_has_seven_keys() {
        assert_eq!(KEEP_SET_KEYS.len(), 7);
        assert!(KEEP_SET_KEYS.contains(&"open_artifact_ids"));
        assert!(KEEP_SET_KEYS.contains(&"open_task_ids"));
    }

    #[test]
    fn retains_pinned_and_open_ids() {
        let mut set = CompactionKeepSet::default();
        set.pinned_files.insert("src/main.rs".into());
        set.open_artifact_ids.insert("art_9".into());
        set.open_task_ids.insert("task_explore_1".into());
        assert!(set.should_keep_text("see src/main.rs"));
        assert!(set.should_keep_text("artifact art_9 saved"));
        assert!(set.should_keep_text("waiting on task_explore_1"));
        assert!(!set.should_keep_text("unrelated chatter"));
    }
}
