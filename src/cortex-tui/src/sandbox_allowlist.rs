//! Sandbox network allowlist — the domains a sandboxed command may reach.
//!
//! The sandbox decides egress as a single boolean today (see
//! `cortex_engine::sandbox::policy`). This module is the *UX* half: it holds the
//! domain list the user edits, and it fails closed — an entry that is not on the
//! list is blocked, and the list starts empty.
//!
//! Entries are hostnames only. A URL, a path, or a bare `*` is refused rather
//! than normalised, so the list cannot be widened by a typo.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// Project file that holds committed allowlist entries.
pub const SANDBOX_ALLOWLIST_FILE: &str = ".cortex/sandbox.toml";

/// One allowed host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowedDomain {
    /// Hostname, e.g. `github.com`. No scheme, port, or path.
    pub host: String,
    /// Optional note shown in the picker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl AllowedDomain {
    /// Build an entry.
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            note: None,
        }
    }

    /// Attach a note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// The committed allowlist.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxAllowlist {
    /// Hosts that may be reached.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Optional notes keyed by host.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub notes: std::collections::BTreeMap<String, String>,
}

impl SandboxAllowlist {
    /// True when nothing is allowed — the fail-closed default.
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty()
    }

    /// Entries in file order.
    pub fn domains(&self) -> Vec<AllowedDomain> {
        self.allow
            .iter()
            .map(|host| {
                let mut entry = AllowedDomain::new(host.clone());
                entry.note = self.notes.get(host).cloned();
                entry
            })
            .collect()
    }

    /// Parse a `.cortex/sandbox.toml` document.
    pub fn parse(document: &str) -> Result<Self> {
        let list: SandboxAllowlist =
            toml::from_str(document).map_err(|error| anyhow::anyhow!("{error}"))?;
        for host in &list.allow {
            validate_host(host)?;
        }
        Ok(list)
    }

    /// True when `host` may be reached.
    ///
    /// Matching is exact or by parent domain: `github.com` allows
    /// `api.github.com`, but `notgithub.com` is never a match.
    pub fn allows(&self, host: &str) -> bool {
        let host = host.trim().to_ascii_lowercase();
        if host.is_empty() {
            return false;
        }
        self.allow.iter().any(|allowed| {
            let allowed = allowed.trim().to_ascii_lowercase();
            host == allowed || host.ends_with(&format!(".{allowed}"))
        })
    }

    /// Add a host, rejecting anything that is not a plain hostname.
    pub fn add(&mut self, host: &str) -> Result<()> {
        validate_host(host)?;
        let host = host.trim().to_ascii_lowercase();
        if !self.allow.iter().any(|existing| existing == &host) {
            self.allow.push(host);
        }
        Ok(())
    }

    /// Remove a host. Returns whether it was present.
    pub fn remove(&mut self, host: &str) -> bool {
        let host = host.trim().to_ascii_lowercase();
        let before = self.allow.len();
        self.allow.retain(|existing| existing != &host);
        self.notes.remove(&host);
        self.allow.len() != before
    }
}

/// Reject anything that is not a plain hostname.
///
/// A scheme, port, path, whitespace, or a wildcard would each change what the
/// entry means, so they are refused instead of being stripped.
pub fn validate_host(host: &str) -> Result<()> {
    let trimmed = host.trim();
    if trimmed.is_empty() {
        bail!("A sandbox allowlist entry cannot be empty.");
    }
    if trimmed != host {
        bail!("`{host}` has leading or trailing whitespace. Write `{trimmed}`.");
    }
    if trimmed.contains('*') {
        bail!("`{trimmed}` uses a wildcard. List each host, so the allowlist stays explicit.");
    }
    if trimmed.contains("://") {
        bail!("`{trimmed}` is a URL. Enter a hostname such as `github.com`.");
    }
    if trimmed.contains('/') {
        bail!("`{trimmed}` contains a path. Enter a hostname such as `github.com`.");
    }
    if trimmed.contains(':') {
        bail!("`{trimmed}` contains a port. Enter a hostname such as `github.com`.");
    }
    if trimmed.contains(char::is_whitespace) {
        bail!("`{trimmed}` contains whitespace. Enter one hostname per entry.");
    }
    if trimmed.starts_with('.') || trimmed.ends_with('.') || trimmed.contains("..") {
        bail!("`{trimmed}` is not a valid hostname.");
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    {
        bail!("`{trimmed}` contains characters that are not valid in a hostname.");
    }
    Ok(())
}

/// Load the committed allowlist for `cwd`, if the file exists.
pub fn load_for_project(cwd: &std::path::Path) -> Result<Option<SandboxAllowlist>> {
    let path = allowlist_path(cwd);
    if !path.exists() {
        return Ok(None);
    }
    let document = std::fs::read_to_string(&path)
        .map_err(|error| anyhow::anyhow!("Could not read {}: {error}", path.display()))?;
    SandboxAllowlist::parse(&document)
        .map(Some)
        .map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))
}

/// Path of the committed allowlist file for `cwd`.
pub fn allowlist_path(cwd: &std::path::Path) -> std::path::PathBuf {
    cwd.join(SANDBOX_ALLOWLIST_FILE)
}

/// Status copy for the sandbox picker, so the count is never stale.
pub fn status_line(allowlist: &SandboxAllowlist) -> String {
    match allowlist.allow.len() {
        0 => "Network is blocked — no domains are allowed.".to_string(),
        1 => "1 domain allowed; everything else is blocked.".to_string(),
        count => format!("{count} domains allowed; everything else is blocked."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_allowlist_blocks_everything() {
        let list = SandboxAllowlist::default();
        assert!(list.is_empty());
        assert!(!list.allows("github.com"));
        assert!(!list.allows("crates.io"));
        assert!(status_line(&list).contains("blocked"));
    }

    #[test]
    fn a_committed_file_parses_into_entries() {
        let list = SandboxAllowlist::parse(
            r#"
allow = ["crates.io", "github.com"]

[notes]
"github.com" = "git fetch"
"#,
        )
        .expect("parse");
        assert_eq!(list.domains().len(), 2);
        assert_eq!(
            list.domains()[1].note.as_deref(),
            Some("git fetch"),
            "notes are keyed by host"
        );
        assert!(list.allows("crates.io"));
        assert!(list.allows("github.com"));
        assert!(!list.allows("example.com"));
    }

    #[test]
    fn a_parent_domain_allows_its_subdomains_but_not_lookalikes() {
        let list = SandboxAllowlist::parse(r#"allow = ["github.com"]"#).expect("parse");
        assert!(list.allows("github.com"));
        assert!(list.allows("api.github.com"));
        assert!(
            list.allows("API.GitHub.com"),
            "matching is case-insensitive"
        );
        assert!(!list.allows("notgithub.com"));
        assert!(!list.allows("github.com.evil.test"));
        assert!(!list.allows(""));
    }

    #[test]
    fn entries_that_would_widen_the_list_are_refused() {
        for host in [
            "*",
            "*.github.com",
            "https://github.com",
            "github.com/path",
            "github.com:443",
            " two hosts",
            "github.com ",
            ".github.com",
            "github.com.",
            "git..hub.com",
            "github com",
            "github_com",
        ] {
            assert!(
                validate_host(host).is_err(),
                "`{host}` must be refused so the allowlist stays explicit"
            );
        }
        for host in [
            "github.com",
            "crates.io",
            "api.cortex.foundation",
            "a-b.test",
        ] {
            validate_host(host).expect(host);
        }
    }

    #[test]
    fn adding_and_removing_entries_is_idempotent() {
        let mut list = SandboxAllowlist::default();
        list.add("GitHub.com").expect("add");
        list.add("github.com").expect("add again");
        assert_eq!(list.allow, vec!["github.com".to_string()]);

        assert!(list.remove("github.com"));
        assert!(!list.remove("github.com"));
        assert!(list.is_empty());
    }

    #[test]
    fn adding_a_bad_entry_does_not_change_the_list() {
        let mut list = SandboxAllowlist::default();
        assert!(list.add("*").is_err());
        assert!(list.is_empty(), "a refused entry must not be stored");
    }

    #[test]
    fn a_broken_file_is_reported_not_ignored() {
        assert!(SandboxAllowlist::parse("allow = [").is_err());
        assert!(
            SandboxAllowlist::parse(r#"allow = ["https://github.com"]"#).is_err(),
            "a URL in the file must be refused"
        );
    }

    #[test]
    fn a_project_without_a_file_has_no_allowlist() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(load_for_project(dir.path()).expect("load").is_none());
        assert_eq!(
            allowlist_path(dir.path()),
            dir.path().join(SANDBOX_ALLOWLIST_FILE)
        );
    }

    #[test]
    fn a_project_file_is_read_from_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = allowlist_path(dir.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, r#"allow = ["crates.io"]"#).expect("write");
        let list = load_for_project(dir.path())
            .expect("load")
            .expect("present");
        assert!(list.allows("crates.io"));
    }

    #[test]
    fn status_copy_tracks_the_count() {
        let mut list = SandboxAllowlist::default();
        assert!(status_line(&list).contains("no domains"));
        list.add("crates.io").expect("add");
        assert!(status_line(&list).contains("1 domain"));
        list.add("github.com").expect("add");
        assert!(status_line(&list).contains("2 domains"));
        assert!(status_line(&list).contains("blocked"));
    }
}
