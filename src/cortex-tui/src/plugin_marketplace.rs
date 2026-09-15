//! Plugin marketplace surface — the signed Cortex plugin registry.
//!
//! The CLI already ships a real registry client (`cortex plugin search/browse/
//! install` against `software.cortex.foundation/plugins`). This module is the
//! TUI-facing half: the origin constant, the installed-plugin reader, and the
//! picker rows, so `/plugins` shows the live state instead of a stub list.
//!
//! Publishing is not implemented on the CLI side, so nothing here claims a
//! package was uploaded.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Signed Cortex plugin registry origin.
pub const REGISTRY_ORIGIN: &str = "software.cortex.foundation/plugins";

/// Installed-plugin state file, matching the CLI runtime.
pub const PLUGINS_STATE_FILE: &str = "plugins.json";

/// One installed plugin as the marketplace shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPlugin {
    /// Plugin id.
    pub id: String,
    /// Installed version.
    pub version: String,
    /// Whether the plugin is currently enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl InstalledPlugin {
    /// Row copy for the picker.
    pub fn status_line(&self) -> String {
        format!(
            "installed · v{}{}",
            self.version,
            if self.enabled { "" } else { " · disabled" }
        )
    }
}

/// The on-disk plugin state file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginState {
    /// Installed plugins.
    #[serde(default)]
    pub plugins: Vec<InstalledPlugin>,
}

impl PluginState {
    /// Parse a `plugins.json` document.
    pub fn parse(document: &str) -> anyhow::Result<Self> {
        let state: PluginState = serde_json::from_str(document)
            .map_err(|error| anyhow::anyhow!("Could not parse {PLUGINS_STATE_FILE}: {error}"))?;
        Ok(state)
    }

    /// Picker rows: installed plugins first, then the registry search row.
    pub fn marketplace_rows(&self) -> Vec<(String, String, String)> {
        let mut rows: Vec<(String, String, String)> = self
            .plugins
            .iter()
            .map(|plugin| (plugin.id.clone(), plugin.id.clone(), plugin.status_line()))
            .collect();
        rows.push((
            "__search__".to_string(),
            "Search the marketplace…".to_string(),
            format!("{REGISTRY_ORIGIN} — signed index"),
        ));
        rows
    }
}

/// Read the plugin state for `cortex_home`, if the file exists.
///
/// A missing file means nothing is installed yet, which is not an error. A file
/// that exists and cannot be parsed is an error: showing an empty marketplace
/// over a broken state file would hide real plugins.
pub fn load_state(cortex_home: &Path) -> anyhow::Result<PluginState> {
    let path = state_path(cortex_home);
    if !path.exists() {
        return Ok(PluginState::default());
    }
    let document = std::fs::read_to_string(&path)
        .map_err(|error| anyhow::anyhow!("Could not read {}: {error}", path.display()))?;
    PluginState::parse(&document).map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))
}

/// Path of the plugin state file for `cortex_home`.
pub fn state_path(cortex_home: &Path) -> PathBuf {
    cortex_home.join(PLUGINS_STATE_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_origin_is_the_cortex_host() {
        assert!(REGISTRY_ORIGIN.starts_with("software.cortex.foundation"));
        assert!(!REGISTRY_ORIGIN.contains("http"), "origin is host-relative");
    }

    #[test]
    fn an_empty_state_still_offers_the_marketplace_row() {
        let state = PluginState::default();
        let rows = state.marketplace_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "__search__");
        assert!(rows[0].2.contains(REGISTRY_ORIGIN));
    }

    #[test]
    fn installed_plugins_lead_the_rows() {
        let state = PluginState {
            plugins: vec![
                InstalledPlugin {
                    id: "cortex-review".into(),
                    version: "0.4.1".into(),
                    enabled: true,
                },
                InstalledPlugin {
                    id: "mermaid-preview".into(),
                    version: "0.2.0".into(),
                    enabled: false,
                },
            ],
        };
        let rows = state.marketplace_rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].1, "cortex-review");
        assert_eq!(rows[0].2, "installed · v0.4.1");
        assert_eq!(rows[1].2, "installed · v0.2.0 · disabled");
        assert_eq!(rows[2].0, "__search__");
    }

    #[test]
    fn a_missing_state_file_is_an_empty_marketplace() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = load_state(dir.path()).expect("missing");
        assert!(state.plugins.is_empty());
        assert_eq!(state_path(dir.path()), dir.path().join(PLUGINS_STATE_FILE));
    }

    #[test]
    fn a_state_file_is_read_from_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            state_path(dir.path()),
            r#"{"plugins":[{"id":"cortex-review","version":"1.0.0"}]}"#,
        )
        .expect("write");
        let state = load_state(dir.path()).expect("load");
        assert_eq!(state.plugins.len(), 1);
        assert!(state.plugins[0].enabled, "enabled defaults to true");
    }

    #[test]
    fn a_broken_state_file_is_reported_not_hidden() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(state_path(dir.path()), "{ not json").expect("write");
        let error = load_state(dir.path()).expect_err("broken");
        assert!(error.to_string().contains(PLUGINS_STATE_FILE), "{error}");
    }

    #[test]
    fn nothing_here_claims_a_publish_succeeded() {
        // Publishing is not implemented in the CLI; the marketplace surface must
        // not offer it as if it were.
        let rows = PluginState::default().marketplace_rows();
        for (_, label, description) in &rows {
            let text = format!("{label} {description}").to_ascii_lowercase();
            assert!(!text.contains("publish"), "{text}");
            assert!(!text.contains("upload"), "{text}");
        }
    }
}
