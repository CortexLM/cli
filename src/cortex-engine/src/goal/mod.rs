//! Durable `/goal` workflows: parse, persist, state machine, continuation.

mod continuation;
#[cfg(test)]
mod live_smoke;
mod machine;
mod parse;
mod persist;
mod types;

pub use continuation::{continuation_gate, continuation_prompt, kickoff_prompt};
pub use machine::{
    apply_command, apply_model_update, needs_wrap_up, record_turn, should_continue, within_budget,
};
pub use parse::{parse_goal_args, parse_goal_input};
pub use persist::{GOAL_FILE, clear_goal, goal_path, load_goal, save_goal};
pub use types::{
    DEFAULT_TURN_BUDGET, Goal, GoalCommand, GoalEvidence, GoalState, WRAP_UP_TOKEN_RATIO,
    WRAP_UP_TURNS_REMAINING,
};
