//! Goal state machine: user commands, model updates, and budgets.

use super::types::{
    Goal, GoalCommand, GoalEvidence, GoalState, WRAP_UP_TOKEN_RATIO, WRAP_UP_TURNS_REMAINING,
};

/// Apply a user `/goal` command to the current (optional) goal.
pub fn apply_command(current: Option<Goal>, command: GoalCommand) -> Result<Option<Goal>, String> {
    match command {
        GoalCommand::Status => Ok(current),
        GoalCommand::Clear => Ok(None),
        GoalCommand::Set { objective } => Ok(Some(Goal::new(objective))),
        GoalCommand::Pause => {
            let mut goal = current.ok_or_else(|| "No active goal to pause.".to_string())?;
            match goal.state {
                GoalState::Active | GoalState::Blocked => {
                    goal.state = GoalState::Paused;
                    goal.touch();
                    Ok(Some(goal))
                }
                GoalState::Paused => Ok(Some(goal)),
                GoalState::Complete => Err(
                    "Goal is already complete. /goal clear or set a new /goal <objective>."
                        .to_string(),
                ),
                GoalState::BudgetLimited => Err(
                    "Budget exhausted. Start a new /goal <objective> or /goal clear.".to_string(),
                ),
            }
        }
        GoalCommand::Resume => {
            let mut goal = current.ok_or_else(|| "No goal to resume.".to_string())?;
            match goal.state {
                GoalState::Paused | GoalState::Blocked => {
                    if goal.turns_remaining() == 0 {
                        goal.state = GoalState::BudgetLimited;
                        goal.last_reason = Some("Turn or token budget exhausted.".to_string());
                        goal.touch();
                        return Ok(Some(goal));
                    }
                    goal.state = GoalState::Active;
                    goal.touch();
                    Ok(Some(goal))
                }
                GoalState::Active => Ok(Some(goal)),
                GoalState::Complete => Err(
                    "Goal is already complete. /goal clear or set a new /goal <objective>."
                        .to_string(),
                ),
                GoalState::BudgetLimited => Err(
                    "Budget exhausted. Start a new /goal <objective> or /goal clear.".to_string(),
                ),
            }
        }
    }
}

/// Model-facing update. The model cannot pause; the user owns that.
/// Rejected updates leave the goal unchanged.
pub fn apply_model_update(
    goal: &mut Goal,
    status: &str,
    progress: Option<String>,
    reason: Option<String>,
    evidence: Vec<GoalEvidence>,
) -> Result<(), String> {
    let mut next = goal.clone();
    if let Some(progress) = progress {
        if !progress.trim().is_empty() {
            next.progress = Some(progress);
        }
    }
    if let Some(reason) = reason {
        if !reason.trim().is_empty() {
            next.last_reason = Some(reason);
        }
    }
    for item in evidence {
        let Some(item) = item.normalized() else {
            continue;
        };
        if !next
            .evidence
            .iter()
            .any(|existing| existing.kind == item.kind && existing.detail == item.detail)
        {
            next.evidence.push(item);
        }
    }

    match status {
        "active" => {
            if matches!(next.state, GoalState::Complete | GoalState::BudgetLimited) {
                return Err(format!("Cannot reactivate a goal that is {}.", next.state));
            }
            if next.state == GoalState::Paused {
                return Err(
                    "The user paused this goal. Use /goal resume; the model cannot unpause."
                        .to_string(),
                );
            }
            next.state = GoalState::Active;
        }
        "blocked" => {
            if next.state == GoalState::Paused {
                return Err("The user paused this goal.".to_string());
            }
            if matches!(next.state, GoalState::Complete | GoalState::BudgetLimited) {
                return Err(format!("Cannot block a goal that is {}.", next.state));
            }
            next.state = GoalState::Blocked;
        }
        "complete" => {
            if next.state == GoalState::Paused {
                return Err("The user paused this goal.".to_string());
            }
            if !next.evidence.iter().any(GoalEvidence::is_usable) {
                return Err(
                    "Completion requires evidence (file, command, or test). Do not mark complete on vibes."
                        .to_string(),
                );
            }
            if next
                .last_reason
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                return Err("Completion requires a reason describing the evidence.".to_string());
            }
            next.state = GoalState::Complete;
        }
        "paused" => {
            return Err("The model cannot pause a goal. The user runs /goal pause.".to_string());
        }
        other => return Err(format!("Unknown goal status: {other}")),
    }
    next.touch();
    *goal = next;
    Ok(())
}

/// Record a finished turn against the budget. May flip to `budget_limited`.
pub fn record_turn(goal: &mut Goal, tokens: u64) {
    goal.turns_used = goal.turns_used.saturating_add(1);
    goal.tokens_used = goal.tokens_used.saturating_add(tokens);
    if goal.state == GoalState::Active && !within_budget(goal) {
        goal.state = GoalState::BudgetLimited;
        goal.last_reason = Some("Turn or token budget exhausted.".to_string());
    }
    goal.touch();
}

/// Count a finished *active* agent turn. Returns whether idle continuation should start.
///
/// Call this *after* the turn completes. Recording first and then deciding
/// whether to continue lets the last remaining turn run as wrap-up.
pub fn finish_turn(goal: &mut Goal, tokens: u64) -> bool {
    if goal.state != GoalState::Active {
        return false;
    }
    record_turn(goal, tokens);
    should_continue(goal)
}

pub fn within_budget(goal: &Goal) -> bool {
    if goal.turns_used >= goal.turn_budget {
        return false;
    }
    if let Some(budget) = goal.token_budget {
        if goal.tokens_used >= budget {
            return false;
        }
    }
    true
}

/// Idle continuation: another turn should start toward the goal.
pub fn should_continue(goal: &Goal) -> bool {
    goal.state == GoalState::Active && within_budget(goal)
}

/// Near-limit wrap-up should be injected into the next prompt.
pub fn needs_wrap_up(goal: &Goal) -> bool {
    if goal.state != GoalState::Active {
        return false;
    }
    if goal.turns_remaining() <= WRAP_UP_TURNS_REMAINING {
        return true;
    }
    goal.token_ratio()
        .is_some_and(|ratio| ratio >= WRAP_UP_TOKEN_RATIO)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_replaces_and_pause_resume_clear() {
        let goal = apply_command(
            None,
            GoalCommand::Set {
                objective: "ship it".into(),
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(goal.state, GoalState::Active);
        assert_eq!(goal.objective, "ship it");

        let paused = apply_command(Some(goal.clone()), GoalCommand::Pause)
            .unwrap()
            .unwrap();
        assert_eq!(paused.state, GoalState::Paused);

        let resumed = apply_command(Some(paused), GoalCommand::Resume)
            .unwrap()
            .unwrap();
        assert_eq!(resumed.state, GoalState::Active);

        assert!(
            apply_command(Some(resumed), GoalCommand::Clear)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn pause_without_goal_fails() {
        assert!(apply_command(None, GoalCommand::Pause).is_err());
    }

    #[test]
    fn resume_complete_and_budget_explain_next_step() {
        let mut done = Goal::new("x");
        done.state = GoalState::Complete;
        let err = apply_command(Some(done), GoalCommand::Resume).unwrap_err();
        assert!(err.contains("already complete"), "{err}");

        let mut limited = Goal::new("x");
        limited.state = GoalState::BudgetLimited;
        let err = apply_command(Some(limited), GoalCommand::Resume).unwrap_err();
        assert!(err.contains("Budget exhausted"), "{err}");
    }

    #[test]
    fn complete_requires_evidence_and_reason() {
        let mut goal = Goal::new("write a file");
        assert!(
            apply_model_update(&mut goal, "complete", None, Some("done".into()), vec![]).is_err()
        );
        assert!(
            apply_model_update(
                &mut goal,
                "complete",
                None,
                None,
                vec![GoalEvidence::new("file", "out.txt")],
            )
            .is_err()
        );
        apply_model_update(
            &mut goal,
            "complete",
            Some("created out.txt".into()),
            Some("file exists and tests passed".into()),
            vec![GoalEvidence::new("file", "out.txt")],
        )
        .unwrap();
        assert_eq!(goal.state, GoalState::Complete);
        assert!(!should_continue(&goal));
    }

    #[test]
    fn complete_rejects_unknown_evidence_kind() {
        let mut goal = Goal::new("write a file");
        let err = apply_model_update(
            &mut goal,
            "complete",
            None,
            Some("feels done".into()),
            vec![GoalEvidence::new("vibe", "shipped")],
        )
        .unwrap_err();
        assert!(err.contains("evidence"), "{err}");
        assert_eq!(goal.state, GoalState::Active);
        assert!(goal.evidence.is_empty());
    }

    #[test]
    fn evidence_is_normalized_and_deduped() {
        let mut goal = Goal::new("write a file");
        apply_model_update(
            &mut goal,
            "active",
            Some("wrote it".into()),
            None,
            vec![
                GoalEvidence::new("FILE", "out.txt"),
                GoalEvidence::new("file", "out.txt"),
                GoalEvidence::new("shell", "ls out.txt"),
            ],
        )
        .unwrap();
        assert_eq!(goal.evidence.len(), 2);
        assert_eq!(goal.evidence[0].kind, "file");
        assert_eq!(goal.evidence[1].kind, "command");
    }

    #[test]
    fn model_cannot_pause() {
        let mut goal = Goal::new("x");
        assert!(apply_model_update(&mut goal, "paused", None, None, vec![]).is_err());
        assert_eq!(goal.state, GoalState::Active);
    }

    #[test]
    fn budget_stops_continuation() {
        let mut goal = Goal::new("x");
        goal.turn_budget = 2;
        assert!(should_continue(&goal));
        record_turn(&mut goal, 10);
        assert!(should_continue(&goal));
        record_turn(&mut goal, 10);
        assert_eq!(goal.state, GoalState::BudgetLimited);
        assert!(!should_continue(&goal));
    }

    #[test]
    fn wrap_up_near_turn_limit() {
        let mut goal = Goal::new("x");
        goal.turn_budget = 2;
        goal.turns_used = 1;
        assert!(needs_wrap_up(&goal));
        assert!(should_continue(&goal));
    }

    #[test]
    fn finish_turn_lets_last_remaining_turn_wrap_up() {
        let mut goal = Goal::new("x");
        goal.turn_budget = 2;
        assert!(should_continue(&goal));
        assert!(!needs_wrap_up(&goal));

        assert!(finish_turn(&mut goal, 4));
        assert_eq!(goal.turns_used, 1);
        assert!(needs_wrap_up(&goal));
        assert!(should_continue(&goal));

        assert!(!finish_turn(&mut goal, 4));
        assert_eq!(goal.state, GoalState::BudgetLimited);
        assert!(!should_continue(&goal));
    }

    #[test]
    fn finish_turn_ignores_paused() {
        let mut goal = Goal::new("x");
        goal.state = GoalState::Paused;
        assert!(!finish_turn(&mut goal, 10));
        assert_eq!(goal.turns_used, 0);
    }

    #[test]
    fn resume_paused_when_budget_gone_is_limited() {
        let mut goal = Goal::new("x");
        goal.turn_budget = 1;
        goal.turns_used = 1;
        goal.state = GoalState::Paused;
        let resumed = apply_command(Some(goal), GoalCommand::Resume)
            .unwrap()
            .unwrap();
        assert_eq!(resumed.state, GoalState::BudgetLimited);
    }
}
