//! Default `/` palette rows.
//!
//! Unfiltered `/` shows eight first-class commands on a tall viewport, then
//! “N more — keep typing to filter” if anything remains. Everything else is
//! reachable by a filter.

/// Rows visible in the unfiltered slash menu on a wide/tall lock surface.
pub const SLASH_VISIBLE: usize = 8;

/// Maximum rows shown in the unfiltered slash palette viewport.
pub const PALETTE_HOME_LIMIT: usize = SLASH_VISIBLE;

/// Commands listed when the user types `/` with no filter, in lock order.
pub const PALETTE_HOME_COMMANDS: [&str; 21] = [
    "model",
    "mode",
    "permissions",
    "plan",
    "goal",
    "effort",
    "mcp",
    "sandbox",
    "usage",
    "resume",
    "jobs",
    "skills",
    "compact",
    "clear",
    "diff",
    "copy",
    "config",
    "login",
    "logout",
    "settings",
    "help",
];

/// True when `name` is one of the unfiltered palette rows.
pub fn is_palette_home_command(name: &str) -> bool {
    PALETTE_HOME_COMMANDS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_includes_goal_and_keeps_lock_count() {
        assert_eq!(PALETTE_HOME_COMMANDS.len(), 21);
        assert!(PALETTE_HOME_COMMANDS.contains(&"goal"));
        assert_eq!(PALETTE_HOME_COMMANDS[4], "goal");
    }
}
