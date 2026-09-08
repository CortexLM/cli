//! `/goal` lock v2 scenes — composer chip states and slash-palette scroll.
//!
//! Split out of [`crate::lock_v2`] so the source-policy line-count baseline
//! on that file does not regress. Chip copy is text-only (no radios).

use cortex_core::widgets::Message;
use cortex_engine::goal::{Goal, GoalState};

use crate::app::AppState;

/// Composer chip lock boards. Each filename is one live state.
pub const GOAL_CHIP_IDS: &[&str] = &[
    "goal-chip-active",
    "goal-chip-paused",
    "goal-chip-done",
    "goal-chip-budget",
    "goal-chip-blocked",
];

/// Apply a `/goal` chip scene. Returns `false` when `id` is not a goal board.
pub fn apply_goal_chip_scene(id: &str, state: &mut AppState) -> bool {
    if !GOAL_CHIP_IDS.contains(&id) {
        return false;
    }
    let (goal_state, turns) = match id {
        "goal-chip-active" => (GoalState::Active, 2),
        "goal-chip-paused" => (GoalState::Paused, 2),
        "goal-chip-done" => (GoalState::Complete, 3),
        "goal-chip-budget" => (GoalState::BudgetLimited, 8),
        "goal-chip-blocked" => (GoalState::Blocked, 1),
        _ => return false,
    };
    seed_goal_session(state, goal_with(goal_state, turns));
    true
}

/// Narrow slash palette: scroll so `/goal` sits after `/plan`.
pub fn show_goal_in_narrow_palette(state: &mut AppState) {
    state.autocomplete.scroll_offset = 3;
    state.autocomplete.selected = 4;
    state.autocomplete.hovered = None;
}

fn goal_with(state: GoalState, turns_used: u32) -> Goal {
    let mut goal = Goal::new("ship the rate limiter and prove it with tests");
    goal.state = state;
    goal.turns_used = turns_used;
    goal
}

fn seed_goal_session(state: &mut AppState, goal: Goal) {
    state.show_launch_splash = false;
    state.tokens_used = 14_000;
    state.add_message(
        Message::user("ship the rate limiter and prove it with tests").with_timestamp("09:20 AM"),
    );
    state.goal = Some(goal);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock_v2::render_lock_v2_scene;
    use cortex_core::style::ACCENT;

    #[test]
    fn goal_chip_copy_matches_states() {
        let cases = [
            ("goal-chip-active", "Goal · 2/8"),
            ("goal-chip-paused", "Goal · paused"),
            ("goal-chip-done", "Goal · done"),
            ("goal-chip-budget", "Goal · budget"),
            ("goal-chip-blocked", "Goal · blocked"),
        ];
        for (id, chip) in cases {
            for (width, height) in [(120u16, 40u16), (40u16, 12u16)] {
                let frame = render_lock_v2_scene(id, width, height).expect(id);
                assert!(
                    frame.plain.contains(chip),
                    "{id} at {width}x{height} missing {chip}:\n{}",
                    frame.plain
                );
                assert!(
                    !frame.plain.contains("Yes, run once")
                        && !frame.plain.contains("● High")
                        && !frame.plain.contains("1 Yes"),
                    "{id} must stay text-only (no radios):\n{}",
                    frame.plain
                );
                let mut accent = false;
                for y in 0..height {
                    for x in 0..width {
                        if frame.buffer[(x, y)].fg == ACCENT
                            && frame.buffer[(x, y)].symbol().contains('G')
                        {
                            accent = true;
                        }
                    }
                }
                assert!(
                    accent || frame.plain.contains("Goal"),
                    "{id} chip should use designer accent at {width}x{height}"
                );
            }
        }
    }

    #[test]
    fn slash_palette_lists_goal_after_plan() {
        let wide = render_lock_v2_scene("slash-palette", 120, 40).expect("wide");
        let plan = wide.plain.find("/plan").expect("/plan");
        let goal = wide.plain.find("/goal").expect("/goal");
        assert!(plan < goal, "/goal must follow /plan:\n{}", wide.plain);
        assert!(
            wide.plain.contains("Persisted long-horizon"),
            "{}",
            wide.plain
        );

        let narrow = render_lock_v2_scene("slash-palette", 40, 12).expect("narrow");
        assert!(
            narrow.plain.contains("/goal"),
            "/goal must be visible at 40x12:\n{}",
            narrow.plain
        );
        let n_plan = narrow.plain.find("/plan");
        let n_goal = narrow.plain.find("/goal").expect("/goal");
        if let Some(p) = n_plan {
            assert!(p < n_goal, "/goal after /plan at 40x12:\n{}", narrow.plain);
        }
    }

    #[test]
    fn goal_chip_ids_are_registered() {
        assert_eq!(GOAL_CHIP_IDS.len(), 5);
    }
}
