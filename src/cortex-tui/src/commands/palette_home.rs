//! Default `/` palette rows.
//!
//! Unfiltered `/` shows twenty first-class commands (ten on screen, the rest
//! via “keep typing to filter”). Everything else is reachable by a filter.

/// Rows visible in the unfiltered slash menu (lock: 10 + “11 more”).
pub const SLASH_VISIBLE: usize = 10;

/// Maximum rows shown in the unfiltered slash palette viewport.
pub const PALETTE_HOME_LIMIT: usize = SLASH_VISIBLE;

/// Commands listed when the user types `/` with no filter, in lock order.
pub const PALETTE_HOME_COMMANDS: [&str; 21] = [
    "model",
    "mode",
    "permissions",
    "plan",
    "effort",
    "mcp",
    "sandbox",
    "usage",
    "resume",
    "jobs",
    "skills",
    "btw",
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
