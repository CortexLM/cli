//! Cortex harness contract (aligned with CortexLM/harness).
//!
//! This module is the CLI-side source of truth for keep-set compaction,
//! Task child roles, tool surface gating, secret redaction, plugin naming,
//! spec-mode mutate gating, and default timeouts.
//!
//! Product copy and system prompts in this module are English only.
//! Do not introduce provider brand names.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};

use once_cell::sync::Lazy;
use regex::Regex;

/// Default tool / turn timeout required by the harness (15 minutes).
pub const TOOL_TIMEOUT_SECS: u64 = 900;

/// Tool results at or above this size become artifacts.
pub const ARTIFACT_THRESHOLD_BYTES: usize = 32 * 1024;

/// Compaction keep-set keys from CortexLM/harness.
pub const KEEP_SET_KEYS: &[&str] = &[
    "user_turns",
    "last_ask",
    "active_plan",
    "open_task_ids",
    "memory_profile",
    "pinned_files",
    "open_artifact_ids",
];

/// Agent surfaces that may invoke tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSurface {
    /// Interactive Chat.
    Chat,
    /// Coding / Code agent.
    Code,
    /// Automated Bot.
    Bot,
}

impl AgentSurface {
    /// Parse a surface name. Unknown values default to Code (CLI).
    pub fn parse(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "chat" => Self::Chat,
            "bot" => Self::Bot,
            _ => Self::Code,
        }
    }
}

/// Task child roles from the harness spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskRole {
    /// Read-only investigation.
    Explore,
    /// Planning only (mermaid + no mutate).
    Plan,
    /// Implementation worker.
    Worker,
}

impl TaskRole {
    /// Parse a role name. Unknown values default to Worker.
    pub fn parse(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "explore" | "research" | "investigate" => Self::Explore,
            "plan" | "architect" | "design" => Self::Plan,
            _ => Self::Worker,
        }
    }

    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explore => "explore",
            Self::Plan => "plan",
            Self::Worker => "worker",
        }
    }

    /// English system prompt for this child role.
    pub fn system_prompt(self) -> &'static str {
        match self {
            Self::Explore => {
                "You are an explore child task. Investigate the codebase and report findings. \
                 Do not modify files. Do not spawn nested Task tools. \
                 Do not send messages to the user or ask the user questions."
            }
            Self::Plan => {
                "You are a plan child task. Produce an implementation plan with a mermaid diagram. \
                 Do not modify files. Do not spawn nested Task tools. \
                 Do not send messages to the user or ask the user questions. \
                 Call ExitSpecMode before any mutate step is proposed to a parent."
            }
            Self::Worker => {
                "You are a worker child task. Implement the assigned work. \
                 Do not spawn nested Task tools. \
                 Do not send messages to the user or ask the user questions."
            }
        }
    }
}

/// Parent-visible Task lifecycle events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskParentEvent {
    /// Child started.
    Started {
        task_id: String,
        role: String,
        description: String,
    },
    /// Progress update.
    Progress { task_id: String, message: String },
    /// Child finished successfully.
    Completed {
        task_id: String,
        summary: String,
        artifact_id: Option<String>,
    },
    /// Child failed.
    Failed {
        task_id: String,
        error: String,
        artifact_id: Option<String>,
    },
}

impl TaskParentEvent {
    /// Wire type name: task_started | progress | completed | failed.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Started { .. } => "task_started",
            Self::Progress { .. } => "progress",
            Self::Completed { .. } => "completed",
            Self::Failed { .. } => "failed",
        }
    }

    /// Optional artifact id on terminal events.
    pub fn artifact_id(&self) -> Option<&str> {
        match self {
            Self::Completed { artifact_id, .. } | Self::Failed { artifact_id, .. } => {
                artifact_id.as_deref()
            }
            _ => None,
        }
    }
}

/// Tools children must never invoke.
const CHILD_FORBIDDEN_TOOLS: &[&str] = &[
    "Task",
    "task",
    "AskUser",
    "ask_user",
    "Questions",
    "questions",
    "send_to_user",
    "SendToUser",
];

/// Deep Research family — Chat-only per harness PR #4.
const CHAT_ONLY_TOOLS: &[&str] = &[
    "DeepResearch",
    "deep_research",
    "deep-research",
    "WebSearch",
    "web_search",
];

/// Tools that mutate the workspace. Blocked in Spec mode until ExitSpecMode.
const MUTATE_TOOLS: &[&str] = &[
    "Write",
    "Create",
    "Edit",
    "MultiEdit",
    "ApplyPatch",
    "Patch",
    "Execute",
    "Bash",
    "local_shell",
    "LocalShell",
];

static SPEC_MODE_MUTATE_UNLOCKED: AtomicBool = AtomicBool::new(false);

/// Reset spec-mode mutate lock (enter Spec / Plan).
pub fn enter_spec_mode() {
    SPEC_MODE_MUTATE_UNLOCKED.store(false, Ordering::SeqCst);
}

/// Unlock mutate tools after ExitSpecMode.
pub fn exit_spec_mode() {
    SPEC_MODE_MUTATE_UNLOCKED.store(true, Ordering::SeqCst);
}

/// Whether ExitSpecMode has been called in this process/session.
pub fn spec_mode_mutate_unlocked() -> bool {
    SPEC_MODE_MUTATE_UNLOCKED.load(Ordering::SeqCst)
}

/// Whether `tool` is a mutate tool that requires ExitSpecMode in Spec mode.
pub fn is_mutate_tool(tool: &str) -> bool {
    MUTATE_TOOLS
        .iter()
        .any(|name| name.eq_ignore_ascii_case(tool))
}

/// Whether a child Task may call this tool.
pub fn child_tool_allowed(tool: &str) -> bool {
    !CHILD_FORBIDDEN_TOOLS
        .iter()
        .any(|name| name.eq_ignore_ascii_case(tool))
}

/// Whether a surface may call this tool.
/// Deep Research is Chat-only.
pub fn surface_allows_tool(surface: AgentSurface, tool: &str) -> bool {
    let chat_only = CHAT_ONLY_TOOLS
        .iter()
        .any(|name| name.eq_ignore_ascii_case(tool));
    if chat_only {
        surface == AgentSurface::Chat
    } else {
        true
    }
}

/// Gate a tool call. Returns an English product error if blocked.
pub fn gate_tool_call(
    surface: AgentSurface,
    tool: &str,
    spec_mode: bool,
    is_child_task: bool,
) -> Result<(), String> {
    if is_child_task && !child_tool_allowed(tool) {
        return Err(format!(
            "Child tasks cannot use {tool}. Nested Task, AskUser, and send_to_user are not allowed."
        ));
    }
    if !surface_allows_tool(surface, tool) {
        return Err(format!(
            "{tool} is available in Chat only. Switch to Chat to run Deep Research."
        ));
    }
    if spec_mode && is_mutate_tool(tool) && !spec_mode_mutate_unlocked() {
        return Err(
            "Spec mode is active. Call ExitSpecMode before mutating the workspace.".to_string(),
        );
    }
    Ok(())
}

/// MCP / plugin tool name: `plugin_{slug}`.
pub fn plugin_tool_name(slug: &str) -> String {
    let slug = slug
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("plugin_{slug}")
}

/// Parse `plugin_{slug}` or legacy `mcp__server__tool`.
pub fn parse_plugin_tool_name(name: &str) -> Option<String> {
    if let Some(slug) = name.strip_prefix("plugin_")
        && !slug.is_empty()
    {
        return Some(slug.to_string());
    }
    if let Some(rest) = name.strip_prefix("mcp__") {
        let server = rest.split("__").next().unwrap_or("");
        if !server.is_empty() {
            return Some(server.to_string());
        }
    }
    None
}

/// Compaction keep-set snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeepSet {
    pub user_turns: Vec<String>,
    pub last_ask: Option<String>,
    pub active_plan: Option<String>,
    pub open_task_ids: BTreeSet<String>,
    pub memory_profile: Option<String>,
    pub pinned_files: BTreeSet<String>,
    pub open_artifact_ids: BTreeSet<String>,
}

impl KeepSet {
    /// Keys present in this keep-set (for tests / telemetry).
    pub fn keys(&self) -> Vec<&'static str> {
        KEEP_SET_KEYS.to_vec()
    }

    /// Record a user turn and update last_ask.
    pub fn push_user_turn(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.last_ask = Some(text.clone());
        self.user_turns.push(text);
    }

    /// Whether an item id is pinned by the keep-set.
    pub fn retains_artifact(&self, artifact_id: &str) -> bool {
        self.open_artifact_ids.contains(artifact_id)
    }

    /// Whether conversation text must be retained across compaction.
    pub fn should_keep_text(&self, text: &str) -> bool {
        if self
            .user_turns
            .iter()
            .any(|t| !t.is_empty() && text.contains(t))
        {
            return true;
        }
        if self
            .last_ask
            .as_ref()
            .is_some_and(|ask| !ask.is_empty() && text.contains(ask))
        {
            return true;
        }
        if self
            .active_plan
            .as_ref()
            .is_some_and(|plan| !plan.is_empty() && text.contains(plan))
        {
            return true;
        }
        if self.open_task_ids.iter().any(|id| text.contains(id)) {
            return true;
        }
        if self
            .memory_profile
            .as_ref()
            .is_some_and(|p| !p.is_empty() && text.contains(p))
        {
            return true;
        }
        if self.pinned_files.iter().any(|f| text.contains(f)) {
            return true;
        }
        if self.open_artifact_ids.iter().any(|id| text.contains(id)) {
            return true;
        }
        false
    }
}

static SECRET_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?i)(api[_-]?key|secret|password|token|authorization|bearer)\s*[:=]\s*\S+")
            .expect("secret key=value regex"),
        Regex::new(r"(?i)bearer\s+[A-Za-z0-9\-\._~\+\/]+=*").expect("bearer regex"),
        Regex::new(r"sk-[A-Za-z0-9]{10,}").expect("sk- regex"),
        Regex::new(r"ghp_[A-Za-z0-9]{20,}").expect("ghp_ regex"),
        Regex::new(r"cortex_gt=[A-Za-z0-9\.\-_]+").expect("guest cookie regex"),
    ]
});

/// Redact secrets from tool output and logs shown to the model or user.
pub fn redact_secrets(input: &str) -> String {
    let mut out = input.to_string();
    for re in SECRET_PATTERNS.iter() {
        out = re.replace_all(&out, "[REDACTED]").into_owned();
    }
    out
}

/// Truncate in the middle, keeping head and tail.
pub fn truncate_middle(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    if max_bytes < 32 {
        return text.chars().take(max_bytes).collect();
    }
    let keep = (max_bytes.saturating_sub(28)) / 2;
    let head: String = text.chars().take(keep).collect();
    let tail: String = text
        .chars()
        .rev()
        .take(keep)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{head}\n\n[... truncated middle ...]\n\n{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_set_keys_match_harness_spec() {
        assert_eq!(
            KEEP_SET_KEYS,
            [
                "user_turns",
                "last_ask",
                "active_plan",
                "open_task_ids",
                "memory_profile",
                "pinned_files",
                "open_artifact_ids",
            ]
        );
        let set = KeepSet::default();
        assert_eq!(set.keys().len(), 7);
    }

    #[test]
    fn child_cannot_nest_task_or_ask_user() {
        assert!(!child_tool_allowed("Task"));
        assert!(!child_tool_allowed("AskUser"));
        assert!(!child_tool_allowed("send_to_user"));
        assert!(!child_tool_allowed("Questions"));
        assert!(child_tool_allowed("Read"));
        assert!(child_tool_allowed("Grep"));
    }

    #[test]
    fn deep_research_is_chat_only() {
        assert!(surface_allows_tool(AgentSurface::Chat, "DeepResearch"));
        assert!(!surface_allows_tool(AgentSurface::Code, "DeepResearch"));
        assert!(!surface_allows_tool(AgentSurface::Bot, "WebSearch"));
        assert!(surface_allows_tool(AgentSurface::Code, "Read"));
    }

    #[test]
    fn exit_spec_mode_unlocks_mutate() {
        enter_spec_mode();
        let err = gate_tool_call(AgentSurface::Code, "Write", true, false).unwrap_err();
        assert!(err.contains("ExitSpecMode"));
        exit_spec_mode();
        assert!(gate_tool_call(AgentSurface::Code, "Write", true, false).is_ok());
        enter_spec_mode();
    }

    #[test]
    fn plugin_slug_naming() {
        assert_eq!(plugin_tool_name("GitHub"), "plugin_github");
        assert_eq!(
            parse_plugin_tool_name("plugin_browser").as_deref(),
            Some("browser")
        );
        assert_eq!(
            parse_plugin_tool_name("mcp__linear__list_issues").as_deref(),
            Some("linear")
        );
    }

    #[test]
    fn redacts_secrets() {
        let raw = "Authorization: Bearer sk-abcdefghijklmnopqrstuvwxyz token=ghp_abcdefghijklmnopqrstuvwxyz";
        let redacted = redact_secrets(raw);
        assert!(!redacted.contains("sk-abcdefgh"));
        assert!(!redacted.contains("ghp_abcdefgh"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn truncate_middle_keeps_head_and_tail() {
        let text = "AAAA".repeat(40) + "MID" + &"ZZZZ".repeat(40);
        let out = truncate_middle(&text, 80);
        assert!(out.contains("AAAA"));
        assert!(out.contains("ZZZZ"));
        assert!(out.contains("truncated middle"));
        assert!(out.len() < text.len());
    }

    #[test]
    fn task_parent_event_kinds() {
        let ev = TaskParentEvent::Started {
            task_id: "t1".into(),
            role: "explore".into(),
            description: "look around".into(),
        };
        assert_eq!(ev.kind(), "task_started");
        let done = TaskParentEvent::Completed {
            task_id: "t1".into(),
            summary: "ok".into(),
            artifact_id: Some("art_1".into()),
        };
        assert_eq!(done.kind(), "completed");
        assert_eq!(done.artifact_id(), Some("art_1"));
    }

    #[test]
    fn prompts_are_english_and_not_branded() {
        for role in [TaskRole::Explore, TaskRole::Plan, TaskRole::Worker] {
            let p = role.system_prompt().to_lowercase();
            assert!(p.contains("you are"));
            assert!(!p.contains("grok"));
        }
    }

    #[test]
    fn timeout_is_900s() {
        assert_eq!(TOOL_TIMEOUT_SECS, 900);
        assert_eq!(ARTIFACT_THRESHOLD_BYTES, 32 * 1024);
    }

    #[test]
    fn keep_set_retains_last_ask_and_ids() {
        let mut set = KeepSet::default();
        set.push_user_turn("how do sessions work");
        set.open_task_ids.insert("task_explore_1".into());
        set.open_artifact_ids.insert("art_9".into());
        set.pinned_files.insert("src/main.rs".into());
        assert!(set.should_keep_text("how do sessions work"));
        assert!(set.should_keep_text("waiting on task_explore_1"));
        assert!(set.should_keep_text("see artifact art_9"));
        assert!(set.should_keep_text("pinned src/main.rs"));
        assert!(!set.should_keep_text("unrelated chatter"));
    }
}
