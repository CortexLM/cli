//! Durable goal types for long-horizon `/goal` workflows.

use serde::{Deserialize, Serialize};

/// Default number of agent turns a goal may consume before wrap-up.
pub const DEFAULT_TURN_BUDGET: u32 = 8;

/// Turns remaining at which wrap-up steering is injected.
pub const WRAP_UP_TURNS_REMAINING: u32 = 1;

/// Token-budget ratio that triggers wrap-up steering.
pub const WRAP_UP_TOKEN_RATIO: f64 = 0.85;

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
        !self.kind.trim().is_empty() && !self.detail.trim().is_empty()
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
        Self {
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
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp();
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

    /// Multi-line status for `/goal` with no arguments.
    pub fn status_text(&self) -> String {
        let progress = self.progress.as_deref().unwrap_or("(none yet)");
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
            "Goal\n  objective: {}\n  state: {}\n  progress: {}\n  turns: {} / {}\n  tokens: {}\n  evidence:\n{evidence}",
            self.objective, self.state, progress, self.turns_used, self.turn_budget, tokens
        )
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
