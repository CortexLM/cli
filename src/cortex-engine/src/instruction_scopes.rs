//! Instruction-document scopes and the opt-in omission plan.
//!
//! Cortex merges instruction Markdown from several places: the personal
//! `~/.cortex` documents, the project root, the directories between the
//! project root and the working directory, and organization-managed policy.
//! A subagent or a headless run may ask to omit the user, project, or local
//! documents.
//!
//! Organization-managed policy is **never** omitted. A request that names it
//! is recorded and the managed document still loads.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Where an instruction document came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstructionScope {
    /// Personal documents under the Cortex home (`~/.cortex/AGENTS.md`).
    User,
    /// The repository-root document.
    Project,
    /// Documents between the repository root and the working directory.
    Local,
    /// Organization-managed policy. Never omitted.
    Managed,
}

impl InstructionScope {
    /// Every scope, in merge order.
    pub const ALL: [Self; 4] = [Self::User, Self::Project, Self::Local, Self::Managed];

    /// Stable name used in JSON, audit records, and diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Local => "local",
            Self::Managed => "managed",
        }
    }

    /// Parse a scope name. Unknown names are rejected so a typo cannot
    /// silently widen what is omitted.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "user" | "personal" => Some(Self::User),
            "project" | "repo" => Some(Self::Project),
            "local" | "cwd" => Some(Self::Local),
            "managed" | "org" | "organization" => Some(Self::Managed),
            _ => None,
        }
    }

    /// True for the scope that no request can remove.
    pub const fn is_managed(self) -> bool {
        matches!(self, Self::Managed)
    }
}

impl std::fmt::Display for InstructionScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resolved load/skip decision for one run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstructionPlan {
    skipped: BTreeSet<InstructionScope>,
    managed_forced: bool,
    requested_managed: bool,
}

impl InstructionPlan {
    /// Nothing omitted: every scope loads.
    pub fn load_all() -> Self {
        Self::default()
    }

    /// Resolve a requested omission list.
    ///
    /// `Managed` is removed from the skip set. When the request named it, the
    /// plan reports [`Self::managed_forced`] and [`Self::requested_managed`]
    /// so the caller can audit that managed policy still loaded.
    pub fn new(requested: &[InstructionScope]) -> Self {
        let mut skipped: BTreeSet<InstructionScope> = BTreeSet::new();
        let mut requested_managed = false;
        for scope in requested {
            if scope.is_managed() {
                requested_managed = true;
                continue;
            }
            skipped.insert(*scope);
        }
        Self {
            skipped,
            managed_forced: requested_managed,
            requested_managed,
        }
    }

    /// True when this scope loads for the run. `Managed` always loads.
    pub fn loads(&self, scope: InstructionScope) -> bool {
        scope.is_managed() || !self.skipped.contains(&scope)
    }

    /// True when any document is actually omitted.
    pub fn omits_anything(&self) -> bool {
        !self.skipped.is_empty()
    }

    /// Scopes that will not load, in a stable order.
    pub fn skipped(&self) -> Vec<InstructionScope> {
        self.skipped.iter().copied().collect()
    }

    /// True when the request named managed policy and it loaded anyway.
    pub fn managed_forced(&self) -> bool {
        self.managed_forced
    }

    /// True when the request named managed policy, even if it was already
    /// covered by another rule.
    pub fn requested_managed(&self) -> bool {
        self.requested_managed
    }

    /// One product-facing line describing what this run loads and skips.
    ///
    /// Returns `None` when nothing was omitted, so an untouched run prints no
    /// extra chrome.
    pub fn summary(&self, source: &str) -> Option<String> {
        if !self.omits_anything() && !self.requested_managed {
            return None;
        }
        let skipped = if self.skipped.is_empty() {
            "nothing".to_string()
        } else {
            self.skipped()
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let managed = if self.requested_managed {
            "managed policy still loaded"
        } else {
            "managed policy loaded"
        };
        Some(format!(
            "Instructions omitted for {source}: {skipped}. {managed}."
        ))
    }
}

/// Parse an `omit_instructions` list, rejecting unknown names.
///
/// A name that is not a scope is an error rather than a silent no-op: an
/// omission request must never quietly apply to the wrong set of documents.
pub fn parse_scopes<'a, I>(values: I) -> Result<Vec<InstructionScope>, String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut scopes = Vec::new();
    for value in values {
        let scope = InstructionScope::parse(value).ok_or_else(|| {
            format!("Unknown instruction scope: {value}. Use user, project, local, or managed.")
        })?;
        if !scopes.contains(&scope) {
            scopes.push(scope);
        }
    }
    Ok(scopes)
}

/// Managed policy file name. Read from the organization policy directory and
/// never omitted.
pub const MANAGED_POLICY_FILE: &str = "AGENTS.md";

/// The files that make up each scope for one run.
#[derive(Debug, Clone, Default)]
pub struct InstructionSources {
    /// `{cortex_home}/AGENTS.md`.
    pub user: Vec<std::path::PathBuf>,
    /// The repository-root `AGENTS.md`.
    pub project: Vec<std::path::PathBuf>,
    /// `AGENTS.md` in directories between the repository root and the cwd.
    pub local: Vec<std::path::PathBuf>,
    /// Organization-managed policy. Loaded whatever the plan says.
    pub managed: Vec<std::path::PathBuf>,
}

impl InstructionSources {
    /// Paths for one scope.
    pub fn for_scope(&self, scope: InstructionScope) -> &[std::path::PathBuf] {
        match scope {
            InstructionScope::User => &self.user,
            InstructionScope::Project => &self.project,
            InstructionScope::Local => &self.local,
            InstructionScope::Managed => &self.managed,
        }
    }
}

/// Which documents a run actually read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstructionLoad {
    /// Scopes whose documents were read.
    pub loaded: Vec<InstructionScope>,
    /// Scopes whose documents were skipped without being read.
    pub skipped: Vec<InstructionScope>,
    /// Joined document text, empty when nothing loaded.
    pub text: String,
}

/// Load instruction documents under a plan.
///
/// A skipped scope's files are never opened, so an omitted document cannot
/// reach the prompt by any path. Managed policy is read whenever it exists.
pub fn load(sources: &InstructionSources, plan: &InstructionPlan) -> InstructionLoad {
    let mut result = InstructionLoad::default();
    let mut blocks: Vec<String> = Vec::new();
    for scope in InstructionScope::ALL {
        let paths = sources.for_scope(scope);
        if !plan.loads(scope) {
            result.skipped.push(scope);
            continue;
        }
        let mut read_any = false;
        for path in paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                blocks.push(content);
                read_any = true;
            }
        }
        if read_any {
            result.loaded.push(scope);
        }
    }
    result.text = blocks.join("\n\n---\n\n");
    result
}

/// The `omit_instructions` JSON value accepted by `--agents` and subagent
/// frontmatter.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct OmitInstructions {
    /// Scope names to omit. `managed` is accepted and ignored.
    #[serde(default)]
    pub omit_instructions: Vec<String>,
}

impl OmitInstructions {
    /// Resolve to a plan, rejecting unknown scope names.
    pub fn plan(&self) -> Result<InstructionPlan, String> {
        let names: Vec<&str> = self.omit_instructions.iter().map(String::as_str).collect();
        Ok(InstructionPlan::new(&parse_scopes(names)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_policy_is_never_omitted() {
        let plan = InstructionPlan::new(&[
            InstructionScope::User,
            InstructionScope::Project,
            InstructionScope::Local,
            InstructionScope::Managed,
        ]);
        assert!(plan.loads(InstructionScope::Managed));
        assert!(plan.managed_forced());
        assert!(plan.requested_managed());
        assert_eq!(
            plan.skipped(),
            vec![
                InstructionScope::User,
                InstructionScope::Project,
                InstructionScope::Local
            ]
        );
        assert!(!plan.loads(InstructionScope::User));
        let summary = plan.summary("subagent").expect("summary");
        assert!(summary.contains("user, project, local"), "{summary}");
        assert!(summary.contains("managed policy still loaded"), "{summary}");
    }

    #[test]
    fn managed_alone_omits_nothing_and_still_loads() {
        let plan = InstructionPlan::new(&[InstructionScope::Managed]);
        assert!(!plan.omits_anything());
        assert!(plan.loads(InstructionScope::Managed));
        assert!(plan.loads(InstructionScope::User));
        assert!(plan.managed_forced());
        assert!(
            plan.summary("subagent")
                .is_some_and(|s| s.contains("nothing") && s.contains("managed policy still loaded"))
        );
    }

    #[test]
    fn an_empty_request_is_a_no_op() {
        let plan = InstructionPlan::new(&[]);
        assert_eq!(plan, InstructionPlan::load_all());
        assert!(!plan.omits_anything());
        assert!(plan.summary("subagent").is_none());
        for scope in InstructionScope::ALL {
            assert!(plan.loads(scope), "{scope}");
        }
    }

    #[test]
    fn unknown_scope_names_are_rejected() {
        let error = parse_scopes(["user", "projekt"]).unwrap_err();
        assert!(error.contains("projekt"), "{error}");
        assert!(
            OmitInstructions {
                omit_instructions: vec!["everything".into()],
            }
            .plan()
            .is_err()
        );
        assert!(
            OmitInstructions {
                omit_instructions: vec!["USER".into(), "personal".into()],
            }
            .plan()
            .unwrap()
            .omits_anything()
        );
    }

    #[test]
    fn scope_names_round_trip() {
        for scope in InstructionScope::ALL {
            assert_eq!(InstructionScope::parse(scope.as_str()), Some(scope));
            assert_eq!(scope.to_string(), scope.as_str());
        }
        assert_eq!(
            InstructionScope::parse(" org "),
            Some(InstructionScope::Managed)
        );
        assert_eq!(InstructionScope::parse("nope"), None);
        assert!(InstructionScope::Managed.is_managed());
        assert!(!InstructionScope::Local.is_managed());
    }

    fn write(path: &std::path::Path, body: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn a_skipped_scope_is_never_read() {
        let temp = tempfile::tempdir().unwrap();
        let sources = InstructionSources {
            user: vec![temp.path().join("user/AGENTS.md")],
            project: vec![temp.path().join("repo/AGENTS.md")],
            local: vec![temp.path().join("repo/src/AGENTS.md")],
            managed: vec![temp.path().join("org/AGENTS.md")],
        };
        write(&sources.user[0], "USER DOC");
        write(&sources.project[0], "PROJECT DOC");
        write(&sources.local[0], "LOCAL DOC");
        write(&sources.managed[0], "MANAGED POLICY");

        let plan = InstructionPlan::new(&[
            InstructionScope::User,
            InstructionScope::Project,
            InstructionScope::Local,
            InstructionScope::Managed,
        ]);
        let load = load(&sources, &plan);
        assert_eq!(load.loaded, vec![InstructionScope::Managed]);
        assert_eq!(
            load.skipped,
            vec![
                InstructionScope::User,
                InstructionScope::Project,
                InstructionScope::Local
            ]
        );
        assert_eq!(load.text, "MANAGED POLICY");
        for omitted in ["USER DOC", "PROJECT DOC", "LOCAL DOC"] {
            assert!(
                !load.text.contains(omitted),
                "{omitted} leaked: {}",
                load.text
            );
        }
    }

    #[test]
    fn managed_policy_loads_even_when_nothing_else_does() {
        let temp = tempfile::tempdir().unwrap();
        let sources = InstructionSources {
            managed: vec![temp.path().join("org/AGENTS.md")],
            ..Default::default()
        };
        write(&sources.managed[0], "MANAGED POLICY");
        // A plan built from the most aggressive request still loads managed.
        let plan = InstructionPlan::new(&[
            InstructionScope::User,
            InstructionScope::Project,
            InstructionScope::Local,
            InstructionScope::Managed,
        ]);
        assert!(plan.loads(InstructionScope::Managed));
        let load = load(&sources, &plan);
        assert_eq!(load.text, "MANAGED POLICY");
        assert!(load.loaded.contains(&InstructionScope::Managed));
    }

    #[test]
    fn an_unmanaged_plan_loads_every_present_document() {
        let temp = tempfile::tempdir().unwrap();
        let sources = InstructionSources {
            user: vec![temp.path().join("user/AGENTS.md")],
            project: vec![temp.path().join("repo/AGENTS.md")],
            local: vec![temp.path().join("repo/src/AGENTS.md")],
            managed: vec![temp.path().join("org/AGENTS.md")],
        };
        write(&sources.user[0], "USER DOC");
        write(&sources.project[0], "PROJECT DOC");
        write(&sources.local[0], "LOCAL DOC");
        write(&sources.managed[0], "MANAGED POLICY");
        let load = load(&sources, &InstructionPlan::load_all());
        assert_eq!(load.loaded, InstructionScope::ALL.to_vec());
        assert!(load.skipped.is_empty());
        for text in ["USER DOC", "PROJECT DOC", "LOCAL DOC", "MANAGED POLICY"] {
            assert!(load.text.contains(text), "{text} missing: {}", load.text);
        }
    }

    #[test]
    fn only_omitting_user_keeps_the_project_documents() {
        let temp = tempfile::tempdir().unwrap();
        let sources = InstructionSources {
            user: vec![temp.path().join("user/AGENTS.md")],
            project: vec![temp.path().join("repo/AGENTS.md")],
            managed: vec![temp.path().join("org/AGENTS.md")],
            ..Default::default()
        };
        write(&sources.user[0], "USER DOC");
        write(&sources.project[0], "PROJECT DOC");
        write(&sources.managed[0], "MANAGED POLICY");
        let plan = InstructionPlan::new(&[InstructionScope::User]);
        let load = load(&sources, &plan);
        assert!(!load.text.contains("USER DOC"));
        assert!(load.text.contains("PROJECT DOC"));
        assert!(load.text.contains("MANAGED POLICY"));
        assert_eq!(load.skipped, vec![InstructionScope::User]);
    }
}
