//! Project permission rules — `.cortex/permissions.toml`.
//!
//! The permission *mode* (see [`crate::permissions`]) decides the default
//! posture. Rules let a repository name specific commands and paths that are
//! always allowed, always asked about, or always denied, so a team can commit
//! its policy next to the code.
//!
//! Evaluation order is deliberate and matches the lock board: the first rule
//! that matches wins, and `deny` beats `allow` for the same specificity, so a
//! broad `allow` cannot be widened by adding a narrower `deny` in the wrong
//! order. A rule only ever makes the posture *stricter* than the mode unless it
//! is an explicit `allow`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Project file that holds committed permission rules.
pub const PERMISSION_RULES_FILE: &str = ".cortex/permissions.toml";

/// What a matching rule says to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleDecision {
    /// Run without asking.
    Allow,
    /// Ask before running.
    Ask,
    /// Never run.
    Deny,
}

impl RuleDecision {
    /// Display word used in the picker and in status copy.
    pub fn label(&self) -> &'static str {
        match self {
            RuleDecision::Allow => "allow",
            RuleDecision::Ask => "ask",
            RuleDecision::Deny => "deny",
        }
    }

    /// Relative strictness, so the most restrictive decision can win a tie.
    fn strictness(&self) -> u8 {
        match self {
            RuleDecision::Allow => 0,
            RuleDecision::Ask => 1,
            RuleDecision::Deny => 2,
        }
    }
}

/// One committed rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRule {
    /// Decision this rule applies.
    pub decision: RuleDecision,
    /// Command or path pattern. `*` matches any run of characters.
    pub pattern: String,
    /// Optional human note, shown in the picker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl PermissionRule {
    /// Build a rule.
    pub fn new(decision: RuleDecision, pattern: impl Into<String>) -> Self {
        Self {
            decision,
            pattern: pattern.into(),
            note: None,
        }
    }

    /// Attach a note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// True when `subject` matches this rule's pattern.
    ///
    /// Matching is case-sensitive and anchored: `cargo test` matches the
    /// command `cargo test`, not `cargo test --all`.
    pub fn matches(&self, subject: &str) -> bool {
        glob_matches(&self.pattern, subject)
    }
}

/// The committed rule set, grouped by decision for a stable file order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRules {
    /// Rules that allow without asking.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Rules that always ask.
    #[serde(default)]
    pub ask: Vec<String>,
    /// Rules that never run.
    #[serde(default)]
    pub deny: Vec<String>,
    /// Optional notes keyed by pattern, so a rule can explain itself.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub notes: BTreeMap<String, String>,
}

impl PermissionRules {
    /// True when no rule is declared.
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.ask.is_empty() && self.deny.is_empty()
    }

    /// Every rule, `deny` first so the strictest rules are shown at the top.
    pub fn rules(&self) -> Vec<PermissionRule> {
        let mut rules = Vec::new();
        for (decision, patterns) in [
            (RuleDecision::Deny, &self.deny),
            (RuleDecision::Ask, &self.ask),
            (RuleDecision::Allow, &self.allow),
        ] {
            for pattern in patterns {
                let mut rule = PermissionRule::new(decision, pattern.clone());
                rule.note = self.notes.get(pattern).cloned();
                rules.push(rule);
            }
        }
        rules
    }

    /// Parse a `.cortex/permissions.toml` document.
    pub fn parse(document: &str) -> Result<Self> {
        let rules: PermissionRules =
            toml::from_str(document).context("Could not parse .cortex/permissions.toml")?;
        rules.validate()?;
        Ok(rules)
    }

    /// Reject rules that cannot be evaluated safely.
    ///
    /// An empty pattern would match nothing and a bare `*` would match
    /// everything, so both are refused rather than silently ignored.
    pub fn validate(&self) -> Result<()> {
        for (decision, patterns) in [
            ("allow", &self.allow),
            ("ask", &self.ask),
            ("deny", &self.deny),
        ] {
            for pattern in patterns {
                let trimmed = pattern.trim();
                if trimmed.is_empty() {
                    anyhow::bail!("A `{decision}` rule has an empty pattern.");
                }
                if trimmed == "*" {
                    anyhow::bail!(
                        "The `{decision}` rule `*` would match every command. Name the commands or paths instead."
                    );
                }
            }
        }
        Ok(())
    }

    /// Decide what to do with `subject` (a command line or a file path).
    ///
    /// The first matching rule wins. When several rules match at the same
    /// position the most restrictive decision is used, so adding a rule can
    /// never widen access by accident.
    pub fn decide(&self, subject: &str) -> Option<RuleDecision> {
        let matching: Vec<PermissionRule> = self
            .rules()
            .into_iter()
            .filter(|rule| rule.matches(subject))
            .collect();
        matching
            .iter()
            .max_by_key(|rule| rule.decision.strictness())
            .map(|rule| rule.decision)
    }
}

/// Read the committed rules for `cwd`, if the file exists.
pub fn load_for_project(cwd: &Path) -> Result<Option<PermissionRules>> {
    let path = rules_path(cwd);
    if !path.exists() {
        return Ok(None);
    }
    let document = std::fs::read_to_string(&path)
        .with_context(|| format!("Could not read {}", path.display()))?;
    Ok(Some(PermissionRules::parse(&document)?))
}

/// Path of the committed rules file for `cwd`.
pub fn rules_path(cwd: &Path) -> PathBuf {
    cwd.join(PERMISSION_RULES_FILE)
}

/// Match `pattern` against `subject`, where `*` matches any run of characters.
///
/// Anchored at both ends: a pattern only matches a whole command or path.
pub fn glob_matches(pattern: &str, subject: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == subject;
    }
    let mut remainder = subject;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if index == 0 {
            let Some(rest) = remainder.strip_prefix(part) else {
                return false;
            };
            remainder = rest;
        } else if index == parts.len() - 1 {
            return remainder.ends_with(part);
        } else {
            let Some(position) = remainder.find(part) else {
                return false;
            };
            remainder = &remainder[position + part.len()..];
        }
    }
    // A trailing `*` (empty final part) matches whatever is left.
    parts.last().is_some_and(|last| last.is_empty()) || remainder.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCUMENT: &str = r#"
allow = ["git status*", "cargo test*", "ls *"]
ask = ["cargo publish*"]
deny = ["rm -rf *", "curl * | bash*"]

[notes]
"rm -rf *" = "always blocked"
"#;

    #[test]
    fn a_committed_file_parses_into_grouped_rules() {
        let rules = PermissionRules::parse(DOCUMENT).expect("parse");
        assert_eq!(rules.allow.len(), 3);
        assert_eq!(rules.ask.len(), 1);
        assert_eq!(rules.deny.len(), 2);
        assert!(!rules.is_empty());
        assert_eq!(
            rules.notes.get("rm -rf *").map(String::as_str),
            Some("always blocked")
        );
    }

    #[test]
    fn rules_are_listed_deny_first_then_ask_then_allow() {
        let rules = PermissionRules::parse(DOCUMENT).expect("parse");
        let listed: Vec<RuleDecision> = rules.rules().iter().map(|r| r.decision).collect();
        assert_eq!(
            listed,
            vec![
                RuleDecision::Deny,
                RuleDecision::Deny,
                RuleDecision::Ask,
                RuleDecision::Allow,
                RuleDecision::Allow,
                RuleDecision::Allow,
            ]
        );
    }

    #[test]
    fn deny_beats_allow_for_the_same_subject() {
        let rules = PermissionRules::parse(
            r#"
allow = ["rm -rf *"]
deny = ["rm -rf *"]
"#,
        )
        .expect("parse");
        assert_eq!(rules.decide("rm -rf build"), Some(RuleDecision::Deny));
    }

    #[test]
    fn the_first_matching_group_wins_for_distinct_subjects() {
        let rules = PermissionRules::parse(DOCUMENT).expect("parse");
        assert_eq!(rules.decide("git status"), Some(RuleDecision::Allow));
        assert_eq!(
            rules.decide("git status --short"),
            Some(RuleDecision::Allow)
        );
        assert_eq!(rules.decide("cargo publish"), Some(RuleDecision::Ask));
        assert_eq!(rules.decide("rm -rf /"), Some(RuleDecision::Deny));
        assert_eq!(rules.decide("cargo build"), None);
    }

    #[test]
    fn an_empty_pattern_or_bare_star_is_refused() {
        let empty = PermissionRules::parse(r#"allow = [""]"#).expect_err("empty");
        assert!(empty.to_string().contains("empty pattern"), "{empty}");

        let star = PermissionRules::parse(r#"deny = ["*"]"#).expect_err("star");
        assert!(star.to_string().contains("match every"), "{star}");

        let whitespace = PermissionRules::parse(r#"ask = ["   "]"#).expect_err("whitespace");
        assert!(
            whitespace.to_string().contains("empty pattern"),
            "{whitespace}"
        );
    }

    #[test]
    fn malformed_documents_are_reported_not_ignored() {
        let error = PermissionRules::parse("allow = \"git status\"").expect_err("type");
        assert!(error.to_string().contains("permissions.toml"), "{error}");
    }

    #[test]
    fn glob_matching_is_anchored() {
        assert!(glob_matches("git status", "git status"));
        assert!(glob_matches("git status*", "git status --short"));
        assert!(glob_matches("*rm -rf *", "sudo rm -rf /"));
        assert!(glob_matches("src/*", "src/main.rs"));
        assert!(glob_matches("*", "anything"));

        assert!(!glob_matches("git status", "git status --short"));
        assert!(!glob_matches("git status*", "git stash"));
        assert!(!glob_matches("cargo test*", "cargo build"));
        assert!(!glob_matches("src/*", "tests/main.rs"));
    }

    #[test]
    fn a_project_without_a_rules_file_has_no_rules() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(load_for_project(dir.path()).expect("load").is_none());
        assert_eq!(
            rules_path(dir.path()),
            dir.path().join(PERMISSION_RULES_FILE)
        );
    }

    #[test]
    fn a_project_rules_file_is_read_from_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = rules_path(dir.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, DOCUMENT).expect("write");
        let rules = load_for_project(dir.path())
            .expect("load")
            .expect("present");
        assert_eq!(rules.decide("rm -rf /"), Some(RuleDecision::Deny));
    }

    #[test]
    fn an_unparseable_file_surfaces_the_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = rules_path(dir.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, "allow = ").expect("write");
        let error = load_for_project(dir.path()).expect_err("parse");
        assert!(error.to_string().contains("permissions.toml"), "{error}");
    }

    #[test]
    fn rule_labels_are_the_lock_words() {
        assert_eq!(RuleDecision::Allow.label(), "allow");
        assert_eq!(RuleDecision::Ask.label(), "ask");
        assert_eq!(RuleDecision::Deny.label(), "deny");
    }
}
