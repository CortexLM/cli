//! Organization policy document (`policy.json`).
//!
//! Administrators point `CORTEX_ORG_POLICY_DIR` at a directory holding
//! `policy.json`. Every key is read fail-closed: a document that exists but
//! cannot be read or parsed denies the restricted behavior rather than
//! allowing it, and an unknown value is treated as the restrictive one.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Managed policy file name.
pub const POLICY_FILE: &str = "policy.json";
/// Environment variable naming the organization policy directory.
pub const POLICY_DIR_ENV: &str = "CORTEX_ORG_POLICY_DIR";

/// Whether fast mode is permitted for this organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastModePolicy {
    /// No policy document (or no key): the host setting decides.
    HostDefault,
    /// The organization allows fast mode.
    Allowed,
    /// The organization disabled fast mode. Fail closed.
    Disabled,
}

impl FastModePolicy {
    /// True only when the organization explicitly disabled fast mode.
    pub const fn is_disabled(self) -> bool {
        matches!(self, Self::Disabled)
    }

    /// True when a request may turn fast mode on.
    pub const fn allows(self) -> bool {
        !self.is_disabled()
    }

    /// Short name for diagnostics and audit records.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HostDefault => "host_default",
            Self::Allowed => "allowed",
            Self::Disabled => "disabled",
        }
    }
}

/// Whether a plugin install must pin the reviewed command hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginInstallPolicy {
    /// No policy document (or no key): `--accept-command` is optional.
    HostDefault,
    /// The organization requires `--accept-command` on install and update.
    RequireAcceptCommand,
}

impl PluginInstallPolicy {
    /// True when an install without a pinned hash must be refused.
    pub const fn requires_pin(self) -> bool {
        matches!(self, Self::RequireAcceptCommand)
    }

    /// Short name for diagnostics and audit records.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HostDefault => "host_default",
            Self::RequireAcceptCommand => "require_accept_command",
        }
    }
}

/// Resolved organization policy for this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrgPolicy {
    /// Fast-mode availability.
    pub fast_mode: FastModePolicy,
    /// Plugin-install pinning requirement.
    pub plugin_install: PluginInstallPolicy,
}

impl Default for OrgPolicy {
    fn default() -> Self {
        Self {
            fast_mode: FastModePolicy::HostDefault,
            plugin_install: PluginInstallPolicy::HostDefault,
        }
    }
}

impl OrgPolicy {
    /// True when the organization governs at least one key.
    ///
    /// An explicit allow counts: the organization decided, so the value is
    /// managed even though it does not restrict anything.
    pub const fn is_managed(&self) -> bool {
        !matches!(self.fast_mode, FastModePolicy::HostDefault)
            || !matches!(self.plugin_install, PluginInstallPolicy::HostDefault)
    }
}

#[derive(Debug, Deserialize)]
struct PolicyDocument {
    #[serde(default)]
    fast_mode: Option<PolicyValue>,
    #[serde(default)]
    plugin_install: Option<PolicyValue>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PolicyValue {
    Bool(bool),
    Text(String),
}

impl PolicyValue {
    /// True only for an explicitly permissive value. Everything else — an
    /// unknown word, a wrong type, an empty string — denies.
    fn is_permissive(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Text(text) => matches!(
                text.trim().to_ascii_lowercase().as_str(),
                "on" | "true" | "enabled" | "allow" | "allowed" | "optional"
            ),
        }
    }
}

/// Directory holding the organization policy document, when configured.
pub fn policy_dir() -> Option<PathBuf> {
    std::env::var(POLICY_DIR_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Policy document path inside `dir`.
pub fn policy_path(dir: &Path) -> PathBuf {
    dir.join(POLICY_FILE)
}

/// Restrictive fallback used when a configured policy path is broken.
const fn restrictive() -> OrgPolicy {
    OrgPolicy {
        fast_mode: FastModePolicy::Disabled,
        plugin_install: PluginInstallPolicy::RequireAcceptCommand,
    }
}

/// Resolve the policy document from an explicit directory.
///
/// An absent directory or document yields [`OrgPolicy::default`]. A path that
/// exists as a dangling symlink, cannot be read, or cannot be parsed yields
/// the restrictive policy for every key — never host defaults.
pub fn resolve(dir: Option<&Path>) -> OrgPolicy {
    let Some(dir) = dir else {
        return OrgPolicy::default();
    };
    let path = policy_path(dir);
    // Distinguish a truly absent path from a dangling symlink / metadata error.
    // `Path::exists` follows links and treats a dangling symlink as absent,
    // which would fail open; `symlink_metadata` sees the link itself.
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return OrgPolicy::default();
        }
        Err(_) => return restrictive(),
        Ok(_) => {}
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => return restrictive(),
    };
    match serde_json::from_str::<PolicyDocument>(&text) {
        Ok(document) => OrgPolicy {
            fast_mode: match document.fast_mode {
                Some(value) if value.is_permissive() => FastModePolicy::Allowed,
                Some(_) => FastModePolicy::Disabled,
                None => FastModePolicy::HostDefault,
            },
            plugin_install: match document.plugin_install {
                // Only an explicit permissive value relaxes the pin; anything
                // else in a present document requires it.
                Some(value) if value.is_permissive() => PluginInstallPolicy::HostDefault,
                Some(_) => PluginInstallPolicy::RequireAcceptCommand,
                None => PluginInstallPolicy::HostDefault,
            },
        },
        Err(_) => restrictive(),
    }
}

/// Resolve the policy for this process from the environment.
pub fn current() -> OrgPolicy {
    resolve(policy_dir().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(policy_path(dir), body).unwrap();
    }

    #[test]
    fn an_absent_document_leaves_every_key_on_the_host_default() {
        let temp = tempfile::tempdir().unwrap();
        for policy in [resolve(None), resolve(Some(temp.path()))] {
            assert_eq!(policy, OrgPolicy::default());
            assert!(!policy.is_managed());
            assert_eq!(policy.fast_mode, FastModePolicy::HostDefault);
            assert!(!policy.plugin_install.requires_pin());
        }
    }

    #[test]
    fn both_keys_resolve_from_one_document() {
        let temp = tempfile::tempdir().unwrap();
        write(
            temp.path(),
            r#"{"fast_mode": false, "plugin_install": "require_accept_command"}"#,
        );
        let policy = resolve(Some(temp.path()));
        assert!(policy.is_managed());
        assert_eq!(policy.fast_mode, FastModePolicy::Disabled);
        assert!(policy.plugin_install.requires_pin());
        assert_eq!(policy.plugin_install.as_str(), "require_accept_command");
    }

    #[test]
    fn a_present_document_denies_unknown_values() {
        let temp = tempfile::tempdir().unwrap();
        write(
            temp.path(),
            r#"{"fast_mode": "maybe", "plugin_install": "sometimes"}"#,
        );
        let policy = resolve(Some(temp.path()));
        assert_eq!(policy.fast_mode, FastModePolicy::Disabled);
        assert!(policy.plugin_install.requires_pin());
    }

    #[test]
    fn an_unreadable_or_unparseable_document_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), "{ not json");
        let broken = resolve(Some(temp.path()));
        assert_eq!(broken.fast_mode, FastModePolicy::Disabled);
        assert!(broken.plugin_install.requires_pin());

        // A directory where the document must be cannot be read as a file.
        let other = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(policy_path(other.path())).unwrap();
        let unreadable = resolve(Some(other.path()));
        assert_eq!(unreadable.fast_mode, FastModePolicy::Disabled);
        assert!(unreadable.plugin_install.requires_pin());
    }

    #[test]
    fn permissive_values_are_recognized_in_either_form() {
        let temp = tempfile::tempdir().unwrap();
        write(
            temp.path(),
            r#"{"fast_mode": true, "plugin_install": "optional"}"#,
        );
        let policy = resolve(Some(temp.path()));
        assert_eq!(policy.fast_mode, FastModePolicy::Allowed);
        assert!(!policy.plugin_install.requires_pin());
        // An explicit allow is still an organization decision.
        assert!(policy.is_managed());

        write(temp.path(), r#"{"fast_mode": "ENABLED"}"#);
        assert_eq!(
            resolve(Some(temp.path())).fast_mode,
            FastModePolicy::Allowed
        );

        write(temp.path(), r#"{"fast_mode": ""}"#);
        assert_eq!(
            resolve(Some(temp.path())).fast_mode,
            FastModePolicy::Disabled
        );
    }

    #[test]
    fn an_unrelated_document_keeps_the_host_defaults() {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), r#"{"other_setting": 1}"#);
        let policy = resolve(Some(temp.path()));
        assert_eq!(policy, OrgPolicy::default());
        assert!(!policy.is_managed());
    }

    #[test]
    fn a_dangling_policy_symlink_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing-target.json");
        let link = policy_path(temp.path());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&missing, &link).unwrap();
            assert!(
                !link.exists(),
                "dangling symlink must not exist() as a file"
            );
            let policy = resolve(Some(temp.path()));
            assert_eq!(policy.fast_mode, FastModePolicy::Disabled);
            assert!(policy.plugin_install.requires_pin());
        }
        #[cfg(not(unix))]
        {
            let _ = (missing, link);
        }
    }

    #[test]
    fn names_are_stable() {
        assert_eq!(POLICY_FILE, "policy.json");
        assert_eq!(POLICY_DIR_ENV, "CORTEX_ORG_POLICY_DIR");
        assert_eq!(FastModePolicy::HostDefault.as_str(), "host_default");
        assert_eq!(FastModePolicy::Allowed.as_str(), "allowed");
        assert_eq!(FastModePolicy::Disabled.as_str(), "disabled");
        assert_eq!(PluginInstallPolicy::HostDefault.as_str(), "host_default");
        assert!(policy_dir().is_none() || std::env::var(POLICY_DIR_ENV).is_ok());
    }
}
