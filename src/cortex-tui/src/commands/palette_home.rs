//! Default `/` palette rows.
//!
//! Unfiltered `/` and Ctrl+K show twenty first-class commands. Everything else
//! is reachable by typing a filter.

/// Maximum rows shown in the unfiltered slash palette.
pub const PALETTE_HOME_LIMIT: usize = 20;

/// Commands listed when the user types `/` with no filter.
///
/// `compact`, `interrupt`, and `clear` stay on this list.
pub const PALETTE_HOME_COMMANDS: [&str; PALETTE_HOME_LIMIT] = [
    "help",
    "settings",
    "compact",
    "interrupt",
    "clear",
    "new",
    "login",
    "models",
    "palette",
    "status",
    "mcp",
    "tasks",
    "diff",
    "diagnostics",
    "spec",
    "resume",
    "sessions",
    "theme",
    "quit",
    "version",
];

/// True when `name` is one of the unfiltered palette rows.
pub fn is_palette_home_command(name: &str) -> bool {
    PALETTE_HOME_COMMANDS.contains(&name)
}
