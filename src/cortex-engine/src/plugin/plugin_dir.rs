//! Folder-of-plugins load path (`--plugin-dir`) with path containment.
//!
//! A plugin-dir may be:
//! - a **folder of plugins**: each child directory with a manifest is a plugin
//! - a **single plugin** directory that itself has a manifest
//!
//! Symlink children are skipped so a pack cannot escape the pointed-at folder.
//! Errors never include raw plugin source URLs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::error::{CortexError, Result};

/// Manifest filenames recognised as a plugin root.
pub const PLUGIN_MANIFESTS: &[&str] = &[
    "plugin.toml",
    "cortex-plugin.json",
    "plugin.json",
    "cortex-plugin.toml",
    ".cortex-plugin/plugin.json",
];

/// A child plugin folder discovered under a `--plugin-dir` path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDirChild {
    pub id: String,
    pub path: PathBuf,
}

/// Snapshot used to detect add/remove while a session is running.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginDirSnapshot {
    pub root: PathBuf,
    pub children: BTreeSet<String>,
}

/// Add/remove delta between two snapshots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginDirDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl PluginDirDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// True when `dir` contains a plugin manifest (single-plugin path).
pub fn has_plugin_manifest(dir: &Path) -> bool {
    PLUGIN_MANIFESTS.iter().any(|name| dir.join(name).is_file())
}

/// Canonicalize `path` as a directory. Fail closed on missing paths.
pub fn contain_plugin_dir(path: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|_| {
        CortexError::InvalidInput(format!(
            "Plugin directory was not found: {}",
            display_path(path)
        ))
    })?;
    if !canonical.is_dir() {
        return Err(CortexError::InvalidInput(format!(
            "Plugin directory is not a folder: {}",
            display_path(&canonical)
        )));
    }
    Ok(canonical)
}

/// Discover plugin children under a contained folder.
///
/// Skips symlink entries and children whose canonical path leaves `root`.
pub fn discover_plugin_dir_folder(root: &Path) -> Result<Vec<PluginDirChild>> {
    let root = contain_plugin_dir(root)?;
    let mut children = Vec::new();

    if has_plugin_manifest(&root) {
        children.push(PluginDirChild {
            id: folder_id(&root),
            path: root,
        });
        return Ok(children);
    }

    let entries = std::fs::read_dir(&root).map_err(|_| {
        CortexError::InvalidInput(format!(
            "Plugin directory could not be read: {}",
            display_path(&root)
        ))
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let Ok(canonical) = std::fs::canonicalize(&path) else {
            continue;
        };
        if !canonical.starts_with(&root) {
            continue;
        }
        if !has_plugin_manifest(&canonical) {
            continue;
        }
        children.push(PluginDirChild {
            id: folder_id(&canonical),
            path: canonical,
        });
    }

    children.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(children)
}

/// Snapshot child ids for hot-reload.
pub fn snapshot_plugin_dir(root: &Path) -> Result<PluginDirSnapshot> {
    let children = discover_plugin_dir_folder(root)?;
    Ok(PluginDirSnapshot {
        root: contain_plugin_dir(root)?,
        children: children.into_iter().map(|c| c.id).collect(),
    })
}

/// Diff two snapshots taken of the same folder.
pub fn diff_plugin_dir(before: &PluginDirSnapshot, after: &PluginDirSnapshot) -> PluginDirDiff {
    PluginDirDiff {
        added: after
            .children
            .difference(&before.children)
            .cloned()
            .collect(),
        removed: before
            .children
            .difference(&after.children)
            .cloned()
            .collect(),
    }
}

/// Fingerprint several `--plugin-dir` roots.
pub fn snapshot_plugin_dirs(roots: &[PathBuf]) -> Result<BTreeMap<PathBuf, PluginDirSnapshot>> {
    let mut map = BTreeMap::new();
    for root in roots {
        let snap = snapshot_plugin_dir(root)?;
        map.insert(snap.root.clone(), snap);
    }
    Ok(map)
}

fn folder_id(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("plugin")
        .to_string()
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_plugin(dir: &Path, name: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("plugin.toml"),
            format!("[plugin]\nid = \"{name}\"\nname = \"{name}\"\nversion = \"0.0.1\"\n"),
        )
        .unwrap();
    }

    #[test]
    fn folder_of_plugins_loads_children_with_manifests() {
        let tmp = TempDir::new().unwrap();
        write_plugin(&tmp.path().join("alpha"), "alpha");
        write_plugin(&tmp.path().join("beta"), "beta");
        std::fs::create_dir_all(tmp.path().join("empty")).unwrap();

        let found = discover_plugin_dir_folder(tmp.path()).unwrap();
        let ids: Vec<_> = found.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["alpha", "beta"]);
    }

    #[test]
    fn single_plugin_directory_loads_itself() {
        let tmp = TempDir::new().unwrap();
        write_plugin(tmp.path(), "solo");
        let found = discover_plugin_dir_folder(tmp.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].id,
            tmp.path().file_name().unwrap().to_str().unwrap()
        );
    }

    #[test]
    fn symlink_children_are_skipped() {
        let tmp = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        write_plugin(outside.path(), "escaped");
        write_plugin(&tmp.path().join("ok"), "ok");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), tmp.path().join("link")).unwrap();
        }
        let found = discover_plugin_dir_folder(tmp.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "ok");
    }

    #[test]
    fn hot_reload_diff_reports_added_and_removed_children() {
        let tmp = TempDir::new().unwrap();
        write_plugin(&tmp.path().join("keep"), "keep");
        write_plugin(&tmp.path().join("gone"), "gone");
        let before = snapshot_plugin_dir(tmp.path()).unwrap();

        std::fs::remove_dir_all(tmp.path().join("gone")).unwrap();
        write_plugin(&tmp.path().join("new"), "new");
        let after = snapshot_plugin_dir(tmp.path()).unwrap();

        let diff = diff_plugin_dir(&before, &after);
        assert_eq!(diff.added, vec!["new".to_string()]);
        assert_eq!(diff.removed, vec!["gone".to_string()]);
    }

    #[test]
    fn missing_folder_fails_closed() {
        let err = contain_plugin_dir(Path::new("/no/such/plugin-dir-cortex")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not found"), "{msg}");
        assert!(!msg.contains("http"), "{msg}");
    }
}
