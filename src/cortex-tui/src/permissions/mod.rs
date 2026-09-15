//! Permission system for tool execution.
//!
//! This module provides a comprehensive permission system that controls
//! which tools can be executed automatically and which require user approval.

use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

mod rules;

pub use rules::{
    PERMISSION_RULES_FILE, PermissionRule, PermissionRules, RuleDecision, glob_matches,
    load_for_project, rules_path,
};

/// Permission mode that determines the level of automatic tool approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PermissionMode {
    /// All tools execute without asking - use with caution!
    Yolo,
    /// Only high-risk tools require approval
    Low,
    /// Medium and high-risk tools require approval (default)
    #[default]
    Medium,
    /// All tools except safe ones require approval
    High,
}

impl PermissionMode {
    /// Returns the display name for this permission mode.
    pub fn display_name(&self) -> &'static str {
        match self {
            PermissionMode::Yolo => "yolo",
            PermissionMode::Low => "low",
            PermissionMode::Medium => "medium",
            PermissionMode::High => "high",
        }
    }

    /// Returns a short description for this permission mode (autonomy level).
    pub fn description(&self) -> &'static str {
        match self {
            PermissionMode::Yolo => "allow all",
            PermissionMode::High => "high autonomy",
            PermissionMode::Medium => "med autonomy",
            PermissionMode::Low => "low autonomy",
        }
    }

    /// Returns the display color for this permission mode.
    ///
    /// The chrome is gray: only the allow-all mode carries the diagnostic
    /// red, because it disables every approval.
    pub fn display_color(&self) -> Color {
        match self {
            PermissionMode::Yolo => cortex_core::style::ERROR,
            PermissionMode::Low => cortex_core::style::TEXT_DIM,
            PermissionMode::Medium => cortex_core::style::TEXT,
            PermissionMode::High => cortex_core::style::TEXT,
        }
    }

    /// Cycles to the next permission mode.
    pub fn cycle_next(&self) -> PermissionMode {
        match self {
            PermissionMode::Yolo => PermissionMode::Low,
            PermissionMode::Low => PermissionMode::Medium,
            PermissionMode::Medium => PermissionMode::High,
            PermissionMode::High => PermissionMode::Yolo,
        }
    }
}

/// Risk level associated with a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ToolRisk {
    /// Safe tools that only read data
    Safe,
    /// Low-risk tools that fetch external data
    Low,
    /// Medium-risk tools that modify files
    Medium,
    /// High-risk tools that execute arbitrary code
    High,
}

impl ToolRisk {
    /// Determines the risk level from a tool name.
    pub fn from_tool_name(name: &str) -> ToolRisk {
        match name {
            // Safe: Read-only operations
            "Read" | "Glob" | "Grep" | "LS" | "TodoRead" | "ListSubagents" => ToolRisk::Safe,

            // Low: External data fetching
            "WebFetch" | "FetchUrl" | "WebSearch" | "CodeSearch" | "ViewImage" => ToolRisk::Low,

            // Medium: File modification
            "Edit" | "Create" | "TodoWrite" | "MultiEdit" | "Task" => ToolRisk::Medium,

            // High: Code execution and system operations
            "Execute" | "Bash" | "ApplyPatch" | "ImageGenerate" | "CreateTerminal"
            | "KillTerminal" => ToolRisk::High,

            // Unknown tools default to high risk
            _ => ToolRisk::High,
        }
    }
}

/// Manages tool execution permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionManager {
    /// Current permission mode
    pub mode: PermissionMode,
    /// Tools allowed for the current session only
    pub session_allowed: HashSet<String>,
    /// Tools always allowed (persisted)
    pub always_allowed: HashSet<String>,
    /// Committed project rules from `.cortex/permissions.toml`.
    #[serde(default)]
    pub rules: PermissionRules,
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionManager {
    /// Creates a new permission manager with default settings.
    pub fn new() -> Self {
        Self {
            mode: PermissionMode::default(),
            session_allowed: HashSet::new(),
            always_allowed: HashSet::new(),
            rules: PermissionRules::default(),
        }
    }

    /// Load the committed project rules for `cwd` into this manager.
    ///
    /// A rules file that cannot be read or parsed is an error: silently
    /// ignoring a team's `deny` list would be the worst possible outcome.
    pub fn load_project_rules(&mut self, cwd: &std::path::Path) -> anyhow::Result<()> {
        if let Some(rules) = load_for_project(cwd)? {
            self.rules = rules;
        }
        Ok(())
    }

    /// Decide a command line against the committed project rules.
    ///
    /// Returns `None` when no rule matches, which leaves the mode in charge.
    pub fn rule_decision(&self, command: &str) -> Option<RuleDecision> {
        self.rules.decide(command)
    }

    /// Determines if the user should be asked for permission to execute a tool.
    ///
    /// Returns `true` if permission is required, `false` if the tool can execute automatically.
    pub fn should_ask(&self, tool_name: &str) -> bool {
        // Check if tool is always allowed
        if self.always_allowed.contains(tool_name) {
            return false;
        }

        // Check if tool is allowed for this session
        if self.session_allowed.contains(tool_name) {
            return false;
        }

        let tool_risk = ToolRisk::from_tool_name(tool_name);

        match self.mode {
            // YOLO mode: never ask
            PermissionMode::Yolo => false,
            // Low mode: only ask for high-risk tools
            PermissionMode::Low => tool_risk >= ToolRisk::High,
            // Medium mode: ask for medium and high-risk tools
            PermissionMode::Medium => tool_risk >= ToolRisk::Medium,
            // High mode: ask for everything except safe tools
            PermissionMode::High => tool_risk >= ToolRisk::Low,
        }
    }

    /// Whether a specific command needs approval, combining the mode with the
    /// committed project rules.
    ///
    /// Rules win over the mode in both directions: a `deny` always asks (the
    /// caller refuses), and an `allow` never asks. A `deny` cannot be overridden
    /// by a session or always-allow grant, because those are per-tool and the
    /// rule is per-command.
    pub fn should_ask_for_command(&self, tool_name: &str, command: &str) -> bool {
        match self.rule_decision(command) {
            Some(RuleDecision::Deny) | Some(RuleDecision::Ask) => true,
            Some(RuleDecision::Allow) => false,
            None => self.should_ask(tool_name),
        }
    }

    /// Whether `command` is refused outright by a committed `deny` rule.
    pub fn is_denied(&self, command: &str) -> bool {
        self.rule_decision(command) == Some(RuleDecision::Deny)
    }

    /// Allows a tool for the current session only.
    pub fn allow_for_session(&mut self, tool_name: &str) {
        self.session_allowed.insert(tool_name.to_string());
    }

    /// Allows a tool permanently (always allowed).
    pub fn allow_always(&mut self, tool_name: &str) {
        self.always_allowed.insert(tool_name.to_string());
    }

    /// Approves a specific tool call once (does not persist).
    /// Note: For one-time approvals, we just don't add to any set.
    pub fn approve_once(&mut self, _tool_call_id: &str) {
        // One-time approval - no state change needed, the caller handles execution
    }

    /// Approves a tool for the remainder of the session.
    pub fn approve_for_session(&mut self, tool_name: &str) {
        self.session_allowed.insert(tool_name.to_string());
    }

    /// Rejects a tool call. Does not change persistent state.
    pub fn reject(&mut self, _tool_call_id: &str) {
        // Rejection is handled by the caller - no state change needed
    }

    /// Sets a default permission mode (for compatibility).
    pub fn set_default_mode(&mut self, mode: String) {
        self.mode = match mode.to_lowercase().as_str() {
            "yolo" => PermissionMode::Yolo,
            "low" => PermissionMode::Low,
            "medium" => PermissionMode::Medium,
            "high" => PermissionMode::High,
            _ => PermissionMode::Medium,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_mode_display_name() {
        assert_eq!(PermissionMode::Yolo.display_name(), "yolo");
        assert_eq!(PermissionMode::Low.display_name(), "low");
        assert_eq!(PermissionMode::Medium.display_name(), "medium");
        assert_eq!(PermissionMode::High.display_name(), "high");
    }

    #[test]
    fn test_permission_mode_description() {
        assert_eq!(PermissionMode::Yolo.description(), "allow all");
        assert_eq!(PermissionMode::High.description(), "high autonomy");
        assert_eq!(PermissionMode::Medium.description(), "med autonomy");
        assert_eq!(PermissionMode::Low.description(), "low autonomy");
    }

    #[test]
    fn test_permission_mode_display_color() {
        // Allow-all is the one red badge; the rest stay gray.
        assert_eq!(
            PermissionMode::Yolo.display_color(),
            cortex_core::style::ERROR
        );
        assert_eq!(
            PermissionMode::Low.display_color(),
            cortex_core::style::TEXT_DIM
        );
        assert_eq!(
            PermissionMode::Medium.display_color(),
            cortex_core::style::TEXT
        );
        assert_eq!(
            PermissionMode::High.display_color(),
            cortex_core::style::TEXT
        );
    }

    #[test]
    fn test_permission_mode_cycle() {
        assert_eq!(PermissionMode::Yolo.cycle_next(), PermissionMode::Low);
        assert_eq!(PermissionMode::Low.cycle_next(), PermissionMode::Medium);
        assert_eq!(PermissionMode::Medium.cycle_next(), PermissionMode::High);
        assert_eq!(PermissionMode::High.cycle_next(), PermissionMode::Yolo);
    }

    #[test]
    fn test_tool_risk_ordering() {
        assert!(ToolRisk::Safe < ToolRisk::Low);
        assert!(ToolRisk::Low < ToolRisk::Medium);
        assert!(ToolRisk::Medium < ToolRisk::High);
    }

    #[test]
    fn test_tool_risk_from_name() {
        assert_eq!(ToolRisk::from_tool_name("Read"), ToolRisk::Safe);
        assert_eq!(ToolRisk::from_tool_name("WebFetch"), ToolRisk::Low);
        assert_eq!(ToolRisk::from_tool_name("Edit"), ToolRisk::Medium);
        assert_eq!(ToolRisk::from_tool_name("Bash"), ToolRisk::High);
        assert_eq!(ToolRisk::from_tool_name("Unknown"), ToolRisk::High);
    }

    #[test]
    fn test_permission_manager_yolo_mode() {
        let mut manager = PermissionManager::new();
        manager.mode = PermissionMode::Yolo;

        assert!(!manager.should_ask("Bash"));
        assert!(!manager.should_ask("Edit"));
        assert!(!manager.should_ask("Read"));
    }

    #[test]
    fn test_permission_manager_high_mode() {
        let mut manager = PermissionManager::new();
        manager.mode = PermissionMode::High;

        assert!(manager.should_ask("Bash"));
        assert!(manager.should_ask("Edit"));
        assert!(manager.should_ask("WebFetch"));
        assert!(!manager.should_ask("Read"));
    }

    #[test]
    fn test_permission_manager_session_allowed() {
        let mut manager = PermissionManager::new();
        manager.mode = PermissionMode::High;

        assert!(manager.should_ask("WebFetch"));
        manager.allow_for_session("WebFetch");
        assert!(!manager.should_ask("WebFetch"));
    }

    #[test]
    fn test_permission_manager_always_allowed() {
        let mut manager = PermissionManager::new();
        manager.mode = PermissionMode::High;

        assert!(manager.should_ask("Edit"));
        manager.allow_always("Edit");
        assert!(!manager.should_ask("Edit"));
    }

    fn manager_with_rules() -> PermissionManager {
        let mut manager = PermissionManager::new();
        manager.mode = PermissionMode::High;
        manager.rules = PermissionRules::parse(
            r#"
allow = ["git status*"]
ask = ["cargo publish*"]
deny = ["rm -rf *"]
"#,
        )
        .expect("rules");
        manager
    }

    #[test]
    fn project_rules_decide_commands_before_the_mode() {
        let manager = manager_with_rules();
        // High mode asks for everything non-safe, but an allow rule wins.
        assert!(!manager.should_ask_for_command("Execute", "git status"));
        // A deny rule asks even in a mode that would auto-approve.
        assert!(manager.should_ask_for_command("Execute", "rm -rf /"));
        assert!(manager.is_denied("rm -rf /"));
        assert!(!manager.is_denied("git status"));
        // No matching rule falls back to the mode.
        assert!(manager.should_ask_for_command("Execute", "cargo build"));
    }

    #[test]
    fn a_session_grant_does_not_override_a_deny_rule() {
        let mut manager = manager_with_rules();
        manager.allow_for_session("Execute");
        manager.allow_always("Execute");
        assert!(
            manager.should_ask_for_command("Execute", "rm -rf /"),
            "a deny rule is per-command and must not be widened by a per-tool grant"
        );
        assert!(!manager.should_ask_for_command("Execute", "git status"));
    }

    #[test]
    fn project_rules_load_from_disk_and_refuse_broken_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut manager = PermissionManager::new();
        manager.load_project_rules(dir.path()).expect("no file");
        assert!(manager.rules.is_empty());

        let path = rules_path(dir.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, "deny = [\"rm -rf *\"]").expect("write");
        manager.load_project_rules(dir.path()).expect("load");
        assert!(manager.is_denied("rm -rf /"));

        std::fs::write(&path, "deny = [").expect("write broken");
        assert!(
            manager.load_project_rules(dir.path()).is_err(),
            "a broken rules file must not be ignored"
        );
    }
}
