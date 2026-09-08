//! Continuation prompts for an idle, in-budget goal.

use super::machine::{needs_wrap_up, should_continue};
use super::types::Goal;

/// First user turn after `/goal <objective>`.
pub fn kickoff_prompt(objective: &str) -> String {
    format!(
        "Durable goal — work until evidence-complete:\n\
         {objective}\n\n\
         Plan, then act, then verify. Use the UpdateGoal tool to record progress \
         with concrete evidence (files written, commands run, tests that passed). \
         Call UpdateGoal with status=complete only when that evidence exists. \
         Do not stop because the work seems done."
    )
}

/// Follow-up turn when the thread is idle and the goal is still active.
pub fn continuation_prompt(goal: &Goal) -> String {
    let progress = goal.progress.as_deref().unwrap_or("(none yet)");
    let wrap = if needs_wrap_up(goal) {
        "\n\nBudget is nearly exhausted. Wrap up: verify remaining work, \
         record evidence, and call UpdateGoal complete or blocked. Do not start new scope."
    } else {
        ""
    };
    format!(
        "Continue the durable goal (plan → act → verify).\n\
         Objective: {}\n\
         State: {}\n\
         Progress: {progress}\n\
         Turns: {} / {}\n\n\
         Use UpdateGoal to record evidence-based progress. \
         Complete only with files, commands, or tests as proof.{wrap}",
        goal.objective, goal.state, goal.turns_used, goal.turn_budget
    )
}

/// Gate used after a turn goes idle.
pub fn continuation_gate(goal: Option<&Goal>) -> bool {
    goal.is_some_and(should_continue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal::machine::record_turn;
    use crate::goal::types::GoalState;

    #[test]
    fn idle_active_within_budget_continues() {
        let goal = Goal::new("add a file");
        assert!(continuation_gate(Some(&goal)));
        assert!(!continuation_gate(None));
    }

    #[test]
    fn paused_and_complete_do_not_continue() {
        let mut paused = Goal::new("x");
        paused.state = GoalState::Paused;
        assert!(!continuation_gate(Some(&paused)));

        let mut done = Goal::new("x");
        done.state = GoalState::Complete;
        assert!(!continuation_gate(Some(&done)));
    }

    #[test]
    fn wrap_up_text_appears_near_limit() {
        let mut goal = Goal::new("x");
        goal.turn_budget = 1;
        record_turn(&mut goal, 0);
        // budget limited after the only turn — no continuation
        assert!(!continuation_gate(Some(&goal)));

        let mut near = Goal::new("x");
        near.turn_budget = 2;
        near.turns_used = 1;
        let text = continuation_prompt(&near);
        assert!(text.contains("Wrap up"));
        assert!(text.contains("UpdateGoal"));
    }

    #[test]
    fn kickoff_names_the_objective() {
        let text = kickoff_prompt("create hello.txt");
        assert!(text.contains("create hello.txt"));
        assert!(text.contains("UpdateGoal"));
    }
}
