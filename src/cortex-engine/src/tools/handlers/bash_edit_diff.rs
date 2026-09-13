//! Opt-in diffs of files a Bash/Execute command wrote.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::diff::diff;

const MAX_FILES: usize = 32;
const MAX_FILE_BYTES: usize = 256 * 1024;

fn env_flag_on(value: &str) -> bool {
    matches!(value.trim(), "1" | "true" | "TRUE" | "yes")
}

/// Whether Bash/Execute should attach file diffs to the tool result.
pub fn bash_edit_diff_enabled(env: &HashMap<String, String>) -> bool {
    if let Some(value) = env.get("CORTEX_BASH_EDIT_DIFF") {
        return env_flag_on(value);
    }
    std::env::var("CORTEX_BASH_EDIT_DIFF")
        .ok()
        .is_some_and(|v| env_flag_on(&v))
}

/// Snapshot of regular files under `cwd` (no `.git` / `target` / `node_modules`).
pub fn snapshot_workspace(cwd: &Path) -> HashMap<PathBuf, Vec<u8>> {
    let mut files = HashMap::new();
    walk(cwd, cwd, &mut files);
    files
}

/// Unified diffs for files that changed between two snapshots.
pub fn diff_snapshots(
    cwd: &Path,
    before: &HashMap<PathBuf, Vec<u8>>,
    after: &HashMap<PathBuf, Vec<u8>>,
) -> String {
    let mut paths: Vec<_> = before.keys().chain(after.keys()).cloned().collect();
    paths.sort();
    paths.dedup();

    let mut chunks = Vec::new();
    for path in paths.into_iter().take(MAX_FILES) {
        if should_redact_path(&path) {
            continue;
        }
        let old = before.get(&path).cloned().unwrap_or_default();
        let new = after.get(&path).cloned().unwrap_or_default();
        if old == new {
            continue;
        }
        let old_s = String::from_utf8_lossy(&old);
        let new_s = String::from_utf8_lossy(&new);
        let unified = diff(&old_s, &new_s);
        if unified.is_empty() {
            continue;
        }
        let rel = path.strip_prefix(cwd).unwrap_or(&path);
        chunks.push(format!(
            "--- {}\n+++ {}\n{}",
            rel.display(),
            rel.display(),
            unified
        ));
    }
    chunks.join("\n")
}

/// Append a diff section to a tool result when files changed.
pub fn append_edit_diff(output: &str, diff_text: &str) -> String {
    if diff_text.trim().is_empty() {
        return output.to_string();
    }
    if output.is_empty() {
        format!("Files changed:\n{diff_text}")
    } else {
        format!("{output}\n\nFiles changed:\n{diff_text}")
    }
}

fn walk(cwd: &Path, dir: &Path, files: &mut HashMap<PathBuf, Vec<u8>>) {
    if files.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if files.len() >= MAX_FILES {
            return;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.')
            || name == "target"
            || name == "node_modules"
            || name == "dist"
            || name == "__pycache__"
        {
            continue;
        }
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            walk(cwd, &path, files);
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        if should_redact_path(&path) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.len() as usize > MAX_FILE_BYTES {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            files.insert(path, bytes);
        }
    }
}

fn should_redact_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name == ".env"
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.contains("credential")
        || name.contains("secret")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn enabled_reads_tool_env() {
        let mut env = HashMap::new();
        env.insert("CORTEX_BASH_EDIT_DIFF".into(), "1".into());
        assert!(bash_edit_diff_enabled(&env));
    }

    #[test]
    fn disabled_by_default() {
        let env = HashMap::new();
        // Process env may be set in the agent host; treat missing map key as the
        // local default when the process var is unset.
        if std::env::var("CORTEX_BASH_EDIT_DIFF").is_err() {
            assert!(!bash_edit_diff_enabled(&env));
        }
    }

    #[test]
    fn snapshot_diff_includes_new_and_edited_files() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "one\n").unwrap();
        let before = snapshot_workspace(tmp.path());
        std::fs::write(tmp.path().join("a.txt"), "two\n").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "new\n").unwrap();
        let after = snapshot_workspace(tmp.path());
        let text = diff_snapshots(tmp.path(), &before, &after);
        assert!(text.contains("a.txt"), "{text}");
        assert!(text.contains("b.txt"), "{text}");
        let combined = append_edit_diff("ok", &text);
        assert!(combined.contains("Files changed"), "{combined}");
    }

    #[test]
    fn secret_filenames_are_omitted() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".env"), "SECRET=1\n").unwrap();
        let before = snapshot_workspace(tmp.path());
        std::fs::write(tmp.path().join(".env"), "SECRET=2\n").unwrap();
        let after = snapshot_workspace(tmp.path());
        let text = diff_snapshots(tmp.path(), &before, &after);
        assert!(text.is_empty(), "{text}");
    }
}
