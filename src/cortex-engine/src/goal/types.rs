//! Durable goal types for long-horizon `/goal` workflows.

use serde::{Deserialize, Serialize};

/// Default number of agent turns a goal may consume before wrap-up.
pub const DEFAULT_TURN_BUDGET: u32 = 8;

/// Turns remaining at which wrap-up steering is injected.
pub const WRAP_UP_TURNS_REMAINING: u32 = 1;

/// Token-budget ratio that triggers wrap-up steering.
pub const WRAP_UP_TOKEN_RATIO: f64 = 0.85;

/// On-disk schema for `goal.json`. Bump when the file shape changes.
pub const GOAL_SCHEMA_VERSION: u32 = 1;

fn default_schema_version() -> u32 {
    GOAL_SCHEMA_VERSION
}

/// Lifecycle of a persisted session goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalState {
    Active,
    Paused,
    Complete,
    BudgetLimited,
    Blocked,
}

impl GoalState {
    /// Wire / display name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Complete => "complete",
            Self::BudgetLimited => "budget_limited",
            Self::Blocked => "blocked",
        }
    }

    /// Composer chip suffix after `Goal · `.
    pub fn chip_label(self) -> &'static str {
        match self {
            Self::Active => "",
            Self::Paused => "paused",
            Self::Complete => "done",
            Self::BudgetLimited => "budget",
            Self::Blocked => "blocked",
        }
    }
}

impl std::fmt::Display for GoalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Concrete proof that work toward the goal happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalEvidence {
    /// `file`, `command`, or `test`.
    pub kind: String,
    /// Path, command line, or test name.
    pub detail: String,
}

impl GoalEvidence {
    pub fn new(kind: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            detail: detail.into(),
        }
    }

    /// True when both fields are non-empty after trim.
    pub fn is_usable(&self) -> bool {
        normalize_evidence_kind(&self.kind).is_some() && !self.detail.trim().is_empty()
    }

    /// Canonical `file` / `command` / `test` plus trimmed detail, if usable.
    pub fn normalized(self) -> Option<Self> {
        let kind = normalize_evidence_kind(&self.kind)?;
        let detail = self.detail.trim();
        if detail.is_empty() {
            return None;
        }
        Some(Self {
            kind: kind.to_string(),
            detail: detail.to_string(),
        })
    }
}

/// Map a model-supplied evidence kind onto the allowed set.
pub fn normalize_evidence_kind(kind: &str) -> Option<&'static str> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "file" | "path" | "filepath" => Some("file"),
        "command" | "cmd" | "shell" => Some("command"),
        "test" | "tests" => Some("test"),
        _ => None,
    }
}

/// Parsed `/goal` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalCommand {
    Status,
    Pause,
    Resume,
    Clear,
    Set { objective: String },
}

/// Persisted durable objective attached to a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Goal {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub objective: String,
    pub state: GoalState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
    #[serde(default)]
    pub evidence: Vec<GoalEvidence>,
    pub turns_used: u32,
    pub turn_budget: u32,
    pub tokens_used: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Goal {
    /// Create a replacement active goal for `objective`.
    pub fn new(objective: impl Into<String>) -> Self {
        let now = chrono::Utc::now().timestamp();
        let mut goal = Self {
            schema_version: GOAL_SCHEMA_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            objective: objective.into(),
            state: GoalState::Active,
            progress: None,
            evidence: Vec::new(),
            turns_used: 0,
            turn_budget: DEFAULT_TURN_BUDGET,
            tokens_used: 0,
            token_budget: None,
            last_reason: None,
            created_at: now,
            updated_at: now,
        };
        goal.sanitize();
        goal
    }

    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Drop empty evidence, canonicalize kinds, and repair a zero budget.
    pub fn sanitize(&mut self) {
        self.objective = self.objective.trim().to_string();
        if self.schema_version == 0 {
            self.schema_version = GOAL_SCHEMA_VERSION;
        }
        if self.turn_budget == 0 {
            self.turn_budget = DEFAULT_TURN_BUDGET;
        }
        let mut seen = std::collections::HashSet::new();
        self.evidence = self
            .evidence
            .drain(..)
            .filter_map(GoalEvidence::normalized)
            .filter(|item| seen.insert((item.kind.clone(), item.detail.clone())))
            .collect();
    }

    /// True when the record is safe to keep on disk and show in the chip.
    pub fn is_persistable(&self) -> bool {
        !self.id.trim().is_empty() && !self.objective.trim().is_empty()
    }

    /// Composer / footer chip, e.g. `Goal · 2/8`.
    pub fn chip(&self) -> String {
        match self.state {
            GoalState::Active => format!("Goal · {}/{}", self.turns_used, self.turn_budget),
            GoalState::Paused => "Goal · paused".to_string(),
            GoalState::Complete => "Goal · done".to_string(),
            GoalState::BudgetLimited => "Goal · budget".to_string(),
            GoalState::Blocked => "Goal · blocked".to_string(),
        }
    }

    /// Short next-step line for `/goal` status.
    pub fn next_step(&self) -> &'static str {
        match self.state {
            GoalState::Active if self.needs_wrap_up_hint() => {
                "wrap-up — verify, record evidence, do not open new scope"
            }
            GoalState::Active => "continue when idle",
            GoalState::Paused => "paused — /goal resume to continue",
            GoalState::Complete => "done — /goal clear to remove",
            GoalState::BudgetLimited => {
                "budget exhausted — /goal <objective> to replace, or /goal clear"
            }
            GoalState::Blocked => "blocked — /goal resume after you unblock",
        }
    }

    fn needs_wrap_up_hint(&self) -> bool {
        if self.turns_remaining() <= WRAP_UP_TURNS_REMAINING {
            return true;
        }
        self.token_ratio()
            .is_some_and(|ratio| ratio >= WRAP_UP_TOKEN_RATIO)
    }

    /// Multi-line status for `/goal` with no arguments.
    pub fn status_text(&self) -> String {
        let progress = self.progress.as_deref().unwrap_or("(none yet)");
        let last = self.last_reason.as_deref().unwrap_or("(none)");
        let tokens = match self.token_budget {
            Some(budget) => format!("{} / {budget}", self.tokens_used),
            None => format!("{} (no cap)", self.tokens_used),
        };
        let evidence = if self.evidence.is_empty() {
            "  (none)".to_string()
        } else {
            self.evidence
                .iter()
                .map(|e| format!("  - {}: {}", e.kind, e.detail))
                .collect::<Vec<_>>()
                .join("\n")
        };
        format!(
            "{}\n  objective: {}\n  state: {}\n  progress: {}\n  last: {}\n  turns: {} / {}\n  tokens: {}\n  next: {}\n  evidence:\n{evidence}",
            self.chip(),
            self.objective,
            self.state,
            progress,
            last,
            self.turns_used,
            self.turn_budget,
            tokens,
            self.next_step()
        )
    }

    /// Pause / resume / set acknowledgement that repeats the chip.
    pub fn action_text(&self, action: &str) -> String {
        format!("{action} {}", self.chip())
    }

    pub fn turns_remaining(&self) -> u32 {
        self.turn_budget.saturating_sub(self.turns_used)
    }

    pub fn token_ratio(&self) -> Option<f64> {
        self.token_budget
            .filter(|b| *b > 0)
            .map(|b| self.tokens_used as f64 / b as f64)
    }

    /// Protocol event after a persist.
    pub fn to_event(&self) -> cortex_protocol::GoalUpdatedEvent {
        cortex_protocol::GoalUpdatedEvent {
            cleared: false,
            goal_id: Some(self.id.clone()),
            objective: Some(self.objective.clone()),
            state: Some(self.state.as_str().to_string()),
            progress: self.progress.clone(),
            turns_used: self.turns_used,
            turn_budget: self.turn_budget,
            chip: Some(self.chip()),
        }
    }

    /// Protocol event for `/goal clear`.
    pub fn cleared_event() -> cortex_protocol::GoalUpdatedEvent {
        cortex_protocol::GoalUpdatedEvent {
            cleared: true,
            goal_id: None,
            objective: None,
            state: Some("cleared".to_string()),
            progress: None,
            turns_used: 0,
            turn_budget: 0,
            chip: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_copy_is_stable_per_state() {
        let mut goal = Goal::new("ship it");
        goal.turns_used = 2;
        assert_eq!(goal.chip(), "Goal · 2/8");
        goal.state = GoalState::Paused;
        assert_eq!(goal.chip(), "Goal · paused");
        goal.state = GoalState::Complete;
        assert_eq!(goal.chip(), "Goal · done");
        goal.state = GoalState::BudgetLimited;
        assert_eq!(goal.chip(), "Goal · budget");
        goal.state = GoalState::Blocked;
        assert_eq!(goal.chip(), "Goal · blocked");
    }

    #[test]
    fn status_names_chip_and_next_step() {
        let mut goal = Goal::new("write out.txt");
        goal.turns_used = 7;
        let text = goal.status_text();
        assert!(text.starts_with("Goal · 7/8"), "{text}");
        assert!(text.contains("wrap-up"), "{text}");
        assert!(text.contains("write out.txt"), "{text}");
    }

    #[test]
    fn sanitize_repairs_budget_and_kinds() {
        let mut goal = Goal::new("  keep me  ");
        goal.turn_budget = 0;
        goal.evidence = vec![
            GoalEvidence::new("FILE", "src/lib.rs"),
            GoalEvidence::new("vibe", "feels done"),
            GoalEvidence::new("file", "src/lib.rs"),
        ];
        goal.sanitize();
        assert_eq!(goal.objective, "keep me");
        assert_eq!(goal.turn_budget, DEFAULT_TURN_BUDGET);
        assert_eq!(goal.evidence.len(), 1);
        assert_eq!(goal.evidence[0].kind, "file");
    }

    #[test]
    fn evidence_kinds_are_canonical() {
        assert_eq!(normalize_evidence_kind("Path"), Some("file"));
        assert_eq!(normalize_evidence_kind("shell"), Some("command"));
        assert_eq!(normalize_evidence_kind("tests"), Some("test"));
        assert!(normalize_evidence_kind("vibe").is_none());
    }

    #[test]
    fn sanitize_dedups_nonadjacent_evidence() {
        let mut goal = Goal::new("ship it");
        goal.evidence = vec![
            GoalEvidence::new("file", "a.rs"),
            GoalEvidence::new("command", "ls a.rs"),
            GoalEvidence::new("FILE", "a.rs"),
            GoalEvidence::new("test", "goal_ok"),
            GoalEvidence::new("file", "a.rs"),
        ];
        goal.sanitize();
        assert_eq!(
            goal.evidence,
            vec![
                GoalEvidence::new("file", "a.rs"),
                GoalEvidence::new("command", "ls a.rs"),
                GoalEvidence::new("test", "goal_ok"),
            ]
        );
    }
}
