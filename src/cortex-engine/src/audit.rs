//! Append-only audit journal for fail-closed decisions.
//!
//! One JSON object per line under `{cortex_home}/audit/events.jsonl`. Records
//! name the decision, the affected scope or plugin, and a hash — never a
//! prompt, file body, or secret. A record that cannot be written is reported
//! to the caller so the decision can fail closed instead of proceeding
//! unlogged.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// Directory under the Cortex home that holds the journal.
pub const AUDIT_DIR: &str = "audit";
/// Journal file name.
pub const AUDIT_FILE: &str = "events.jsonl";
/// Journal schema version, bumped when the record shape changes.
pub const AUDIT_SCHEMA_VERSION: u32 = 1;

/// Decision kinds written by this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    /// A plugin command hash was pinned by the user and accepted.
    PluginCommandAccepted,
    /// Instruction documents were omitted from a session or subagent.
    InstructionsOmitted,
    /// An omission request named managed policy, which is never omitted.
    ManagedPolicyNeverOmitted,
}

impl AuditKind {
    /// Stable `kind` value in the journal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PluginCommandAccepted => "plugin_command_accepted",
            Self::InstructionsOmitted => "instructions_omitted",
            Self::ManagedPolicyNeverOmitted => "managed_policy_never_omitted",
        }
    }
}

/// Journal path for a Cortex home.
pub fn audit_path(cortex_home: &Path) -> PathBuf {
    cortex_home.join(AUDIT_DIR).join(AUDIT_FILE)
}

/// Append one record. Creates the directory on first write.
///
/// The file is opened append-only so a previous journal is never rewritten.
pub fn record(cortex_home: &Path, kind: AuditKind, detail: Value) -> std::io::Result<PathBuf> {
    let path = audit_path(cortex_home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = json!({
        "schema": AUDIT_SCHEMA_VERSION,
        "ts": chrono::Utc::now().to_rfc3339(),
        "kind": kind.as_str(),
        "detail": detail,
    });
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{line}")?;
    file.flush()?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn records_append_as_one_json_object_per_line() {
        let temp = tempfile::tempdir().unwrap();
        let first = record(
            temp.path(),
            AuditKind::InstructionsOmitted,
            json!({"source": "subagent", "skipped": ["user", "project"]}),
        )
        .unwrap();
        record(
            temp.path(),
            AuditKind::ManagedPolicyNeverOmitted,
            json!({"source": "subagent", "requested": ["managed"]}),
        )
        .unwrap();
        assert_eq!(first, audit_path(temp.path()));
        let body = std::fs::read_to_string(&first).unwrap();
        let lines: Vec<_> = body.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            let value: Value = serde_json::from_str(line).unwrap();
            assert_eq!(value["schema"], AUDIT_SCHEMA_VERSION);
            assert!(value["ts"].as_str().is_some_and(|t| t.contains('T')));
            assert!(value["kind"].as_str().is_some_and(|k| !k.is_empty()));
            assert!(value["detail"].is_object());
        }
        // No prompt or file body field can appear in a record.
        assert!(!body.contains("prompt"));
    }

    #[test]
    fn kind_strings_are_stable() {
        assert_eq!(
            AuditKind::PluginCommandAccepted.as_str(),
            "plugin_command_accepted"
        );
        assert_eq!(
            AuditKind::InstructionsOmitted.as_str(),
            "instructions_omitted"
        );
        assert_eq!(
            AuditKind::ManagedPolicyNeverOmitted.as_str(),
            "managed_policy_never_omitted"
        );
    }

    #[test]
    fn an_unwritable_journal_is_reported_not_swallowed() {
        let temp = tempfile::tempdir().unwrap();
        let blocked = temp.path().join("audit");
        // A file where the directory must be makes the append impossible.
        std::fs::write(&blocked, b"not a directory").unwrap();
        assert!(
            record(temp.path(), AuditKind::InstructionsOmitted, json!({})).is_err(),
            "an unwritable journal must surface as an error"
        );
    }
}
