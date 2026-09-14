//! File checkpoints — capture file contents before a turn so `/rewind` can put
//! them back.
//!
//! The conversation rewind in
//! [`crate::runner::event_loop::sessions`] only forks history; it never touches
//! the working tree. This module is the file half: before a turn runs, the files
//! a turn is about to change are copied into a checkpoint directory, and
//! `/rewind` can restore them.
//!
//! A checkpoint never guesses. It records exactly the files it was given, keeps
//! the original bytes, and refuses to restore when the current content has
//! diverged in a way it cannot undo (a file that was deleted stays deleted only
//! if the caller says so).

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Directory under the session home that holds checkpoints.
pub const CHECKPOINTS_DIR: &str = "checkpoints";

/// Largest file a checkpoint will store, so a stray large artifact cannot fill
/// the disk.
pub const MAX_CHECKPOINT_BYTES: u64 = 2 * 1024 * 1024;

/// One captured file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointFile {
    /// Path relative to the workspace root, using `/` separators.
    pub path: String,
    /// Original contents, or `None` when the file did not exist yet (it was
    /// created by the turn, so restoring means deleting it).
    pub original: Option<String>,
}

impl CheckpointFile {
    /// True when restoring this file means deleting it.
    pub fn was_created(&self) -> bool {
        self.original.is_none()
    }
}

/// One checkpoint: the files captured before a turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Identifier, e.g. `turn-3`.
    pub id: String,
    /// ISO-8601 capture time.
    pub captured_at: String,
    /// Captured files, keyed by workspace-relative path.
    pub files: Vec<CheckpointFile>,
}

impl Checkpoint {
    /// Number of files in this checkpoint.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// True when nothing was captured.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// One-line summary for the picker.
    pub fn summary(&self) -> String {
        match self.files.len() {
            0 => "no files changed".to_string(),
            1 => "1 file".to_string(),
            count => format!("{count} files"),
        }
    }
}

/// A restored file, so the caller can report exactly what changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredFile {
    /// Workspace-relative path.
    pub path: String,
    /// What the restore did.
    pub action: RestoreAction,
}

/// What a restore did to one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreAction {
    /// Original contents written back.
    Reverted,
    /// The file was created by the turn and has been removed.
    Removed,
    /// The file was already in its original state.
    Unchanged,
}

impl RestoreAction {
    /// Display word for the transcript.
    pub fn label(&self) -> &'static str {
        match self {
            RestoreAction::Reverted => "restored",
            RestoreAction::Removed => "removed",
            RestoreAction::Unchanged => "unchanged",
        }
    }
}

/// Capture `paths` (workspace-relative) into `checkpoint_dir`.
///
/// Paths that escape the workspace, are absolute, or contain `..` are refused:
/// a checkpoint must never read outside the repository it belongs to.
pub fn capture(
    workspace: &Path,
    checkpoint_dir: &Path,
    id: &str,
    paths: &[PathBuf],
) -> Result<Checkpoint> {
    std::fs::create_dir_all(checkpoint_dir)
        .with_context(|| format!("Could not create {}", checkpoint_dir.display()))?;

    let mut files = Vec::new();
    for path in paths {
        let relative = validate_relative(path)?;
        let absolute = workspace.join(&relative);
        let original = if absolute.exists() {
            let metadata = std::fs::metadata(&absolute)
                .with_context(|| format!("Could not read {}", absolute.display()))?;
            if metadata.len() > MAX_CHECKPOINT_BYTES {
                bail!(
                    "{} is larger than the checkpoint limit ({} bytes). It was not captured.",
                    relative.display(),
                    MAX_CHECKPOINT_BYTES
                );
            }
            Some(
                std::fs::read_to_string(&absolute)
                    .with_context(|| format!("Could not read {}", absolute.display()))?,
            )
        } else {
            None
        };
        files.push(CheckpointFile {
            path: normalize(&relative),
            original,
        });
    }

    let checkpoint = Checkpoint {
        id: id.to_string(),
        captured_at: chrono::Utc::now().to_rfc3339(),
        files,
    };
    write_checkpoint(checkpoint_dir, &checkpoint)?;
    Ok(checkpoint)
}

/// Restore `checkpoint` into `workspace`.
///
/// Returns one [`RestoredFile`] per captured file. Files whose content already
/// matches the checkpoint are reported as `Unchanged` rather than rewritten.
pub fn restore(workspace: &Path, checkpoint: &Checkpoint) -> Result<Vec<RestoredFile>> {
    let mut restored = Vec::new();
    for file in &checkpoint.files {
        let relative = validate_relative(Path::new(&file.path))?;
        let absolute = workspace.join(&relative);
        match &file.original {
            Some(original) => {
                let current = std::fs::read_to_string(&absolute).ok();
                if current.as_deref() == Some(original.as_str()) {
                    restored.push(RestoredFile {
                        path: file.path.clone(),
                        action: RestoreAction::Unchanged,
                    });
                    continue;
                }
                if let Some(parent) = absolute.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("Could not create {}", parent.display()))?;
                }
                std::fs::write(&absolute, original)
                    .with_context(|| format!("Could not restore {}", absolute.display()))?;
                restored.push(RestoredFile {
                    path: file.path.clone(),
                    action: RestoreAction::Reverted,
                });
            }
            None => {
                // The turn created this file; restoring means removing it.
                if absolute.exists() {
                    std::fs::remove_file(&absolute)
                        .with_context(|| format!("Could not remove {}", absolute.display()))?;
                    restored.push(RestoredFile {
                        path: file.path.clone(),
                        action: RestoreAction::Removed,
                    });
                } else {
                    restored.push(RestoredFile {
                        path: file.path.clone(),
                        action: RestoreAction::Unchanged,
                    });
                }
            }
        }
    }
    Ok(restored)
}

/// Write a checkpoint to `checkpoint_dir/<id>.json`.
pub fn write_checkpoint(checkpoint_dir: &Path, checkpoint: &Checkpoint) -> Result<()> {
    std::fs::create_dir_all(checkpoint_dir)
        .with_context(|| format!("Could not create {}", checkpoint_dir.display()))?;
    let path = checkpoint_path(checkpoint_dir, &checkpoint.id);
    let document = serde_json::to_string_pretty(checkpoint)?;
    std::fs::write(&path, document).with_context(|| format!("Could not write {}", path.display()))
}

/// Read a checkpoint by id.
pub fn read_checkpoint(checkpoint_dir: &Path, id: &str) -> Result<Checkpoint> {
    let path = checkpoint_path(checkpoint_dir, id);
    let document = std::fs::read_to_string(&path)
        .with_context(|| format!("No checkpoint `{id}` at {}", path.display()))?;
    serde_json::from_str(&document)
        .with_context(|| format!("Checkpoint `{id}` is not readable JSON"))
}

/// Every checkpoint on disk, newest first.
pub fn list_checkpoints(checkpoint_dir: &Path) -> Result<Vec<Checkpoint>> {
    if !checkpoint_dir.exists() {
        return Ok(Vec::new());
    }
    let mut checkpoints = Vec::new();
    let entries = std::fs::read_dir(checkpoint_dir)
        .with_context(|| format!("Could not list {}", checkpoint_dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let document = std::fs::read_to_string(&path)
            .with_context(|| format!("Could not read {}", path.display()))?;
        // A single unreadable checkpoint must not hide the rest.
        if let Ok(checkpoint) = serde_json::from_str::<Checkpoint>(&document) {
            checkpoints.push(checkpoint);
        }
    }
    checkpoints.sort_by(|a, b| b.captured_at.cmp(&a.captured_at));
    Ok(checkpoints)
}

/// Path of the checkpoint file for `id`.
pub fn checkpoint_path(checkpoint_dir: &Path, id: &str) -> PathBuf {
    checkpoint_dir.join(format!("{id}.json"))
}

/// Refuse a path that is not safely inside the workspace.
pub fn validate_relative(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        bail!(
            "Checkpoint paths must be relative to the workspace: {}",
            path.display()
        );
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => bail!(
                "Checkpoint paths cannot leave the workspace: {}",
                path.display()
            ),
            Component::RootDir | Component::Prefix(_) => bail!(
                "Checkpoint paths must be relative to the workspace: {}",
                path.display()
            ),
        }
    }
    if path.as_os_str().is_empty() {
        bail!("A checkpoint path cannot be empty.");
    }
    Ok(path.to_path_buf())
}

fn normalize(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Transcript line after a restore.
pub fn restore_summary(restored: &[RestoredFile]) -> String {
    if restored.is_empty() {
        return "Nothing to restore — this checkpoint captured no files.".to_string();
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for file in restored {
        *counts.entry(file.action.label()).or_default() += 1;
    }
    let parts: Vec<String> = counts
        .into_iter()
        .map(|(action, count)| format!("{count} {action}"))
        .collect();
    format!(
        "Restored {} file(s): {}. The conversation is unchanged — use /undo for history.",
        restored.len(),
        parts.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn capturing_an_existing_file_keeps_its_bytes() {
        let ws = workspace();
        std::fs::write(ws.path().join("a.rs"), "fn main() {}\n").expect("write");
        let dir = ws.path().join(CHECKPOINTS_DIR);
        let checkpoint = capture(ws.path(), &dir, "turn-1", &[PathBuf::from("a.rs")]).expect("cap");
        assert_eq!(checkpoint.len(), 1);
        assert_eq!(
            checkpoint.files[0].original.as_deref(),
            Some("fn main() {}\n")
        );
        assert!(!checkpoint.files[0].was_created());
    }

    #[test]
    fn capturing_a_missing_file_records_it_as_created() {
        let ws = workspace();
        let dir = ws.path().join(CHECKPOINTS_DIR);
        let checkpoint =
            capture(ws.path(), &dir, "turn-1", &[PathBuf::from("new.rs")]).expect("cap");
        assert!(checkpoint.files[0].was_created());
    }

    #[test]
    fn restoring_reverts_edited_files_and_removes_created_ones() {
        let ws = workspace();
        std::fs::write(ws.path().join("edited.rs"), "original\n").expect("write");
        let dir = ws.path().join(CHECKPOINTS_DIR);
        let checkpoint = capture(
            ws.path(),
            &dir,
            "turn-1",
            &[PathBuf::from("edited.rs"), PathBuf::from("created.rs")],
        )
        .expect("cap");

        // The turn runs: one file is edited, one is created.
        std::fs::write(ws.path().join("edited.rs"), "changed\n").expect("write");
        std::fs::write(ws.path().join("created.rs"), "new\n").expect("write");

        let restored = restore(ws.path(), &checkpoint).expect("restore");
        assert_eq!(restored.len(), 2);
        assert_eq!(restored[0].action, RestoreAction::Reverted);
        assert_eq!(restored[1].action, RestoreAction::Removed);
        assert_eq!(
            std::fs::read_to_string(ws.path().join("edited.rs")).expect("read"),
            "original\n"
        );
        assert!(!ws.path().join("created.rs").exists());
    }

    #[test]
    fn restoring_an_unchanged_file_reports_it_rather_than_rewriting() {
        let ws = workspace();
        std::fs::write(ws.path().join("same.rs"), "same\n").expect("write");
        let dir = ws.path().join(CHECKPOINTS_DIR);
        let checkpoint =
            capture(ws.path(), &dir, "turn-1", &[PathBuf::from("same.rs")]).expect("cap");
        let restored = restore(ws.path(), &checkpoint).expect("restore");
        assert_eq!(restored[0].action, RestoreAction::Unchanged);
        assert!(restore_summary(&restored).contains("1 unchanged"));
    }

    #[test]
    fn restoring_a_created_file_that_is_already_gone_is_unchanged() {
        let ws = workspace();
        let dir = ws.path().join(CHECKPOINTS_DIR);
        let checkpoint =
            capture(ws.path(), &dir, "turn-1", &[PathBuf::from("gone.rs")]).expect("cap");
        let restored = restore(ws.path(), &checkpoint).expect("restore");
        assert_eq!(restored[0].action, RestoreAction::Unchanged);
    }

    #[test]
    fn a_round_trip_through_disk_preserves_the_checkpoint() {
        let ws = workspace();
        std::fs::write(ws.path().join("a.rs"), "one\n").expect("write");
        let dir = ws.path().join(CHECKPOINTS_DIR);
        let checkpoint = capture(ws.path(), &dir, "turn-1", &[PathBuf::from("a.rs")]).expect("cap");
        let read = read_checkpoint(&dir, "turn-1").expect("read");
        assert_eq!(read, checkpoint);
        let listed = list_checkpoints(&dir).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "turn-1");
    }

    #[test]
    fn listing_returns_newest_first_and_survives_a_broken_file() {
        let ws = workspace();
        let dir = ws.path().join(CHECKPOINTS_DIR);
        std::fs::create_dir_all(&dir).expect("mkdir");
        for (id, at) in [
            ("old", "2026-01-01T00:00:00Z"),
            ("new", "2026-09-01T00:00:00Z"),
        ] {
            let checkpoint = Checkpoint {
                id: id.into(),
                captured_at: at.into(),
                files: Vec::new(),
            };
            write_checkpoint(&dir, &checkpoint).expect("write");
        }
        std::fs::write(dir.join("broken.json"), "{ not json").expect("write");
        let listed = list_checkpoints(&dir).expect("list");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, "new");
    }

    #[test]
    fn paths_that_leave_the_workspace_are_refused() {
        for path in ["../outside.rs", "/etc/passwd", "a/../../b.rs"] {
            assert!(
                validate_relative(Path::new(path)).is_err(),
                "`{path}` must be refused"
            );
        }
        validate_relative(Path::new("src/a.rs")).expect("relative");
        validate_relative(Path::new("./src/a.rs")).expect("dot relative");
    }

    #[test]
    fn capturing_an_escaping_path_fails_before_reading_anything() {
        let ws = workspace();
        let dir = ws.path().join(CHECKPOINTS_DIR);
        let error =
            capture(ws.path(), &dir, "turn-1", &[PathBuf::from("../secret")]).expect_err("escape");
        assert!(error.to_string().contains("cannot leave"), "{error}");
    }

    #[test]
    fn a_file_over_the_limit_is_refused_rather_than_truncated() {
        let ws = workspace();
        let big = ws.path().join("big.bin");
        std::fs::write(&big, vec![b'x'; (MAX_CHECKPOINT_BYTES + 1) as usize]).expect("write");
        let dir = ws.path().join(CHECKPOINTS_DIR);
        let error =
            capture(ws.path(), &dir, "turn-1", &[PathBuf::from("big.bin")]).expect_err("too big");
        assert!(error.to_string().contains("checkpoint limit"), "{error}");
    }

    #[test]
    fn a_missing_checkpoint_is_reported_with_its_id() {
        let ws = workspace();
        let dir = ws.path().join(CHECKPOINTS_DIR);
        let error = read_checkpoint(&dir, "turn-9").expect_err("missing");
        assert!(error.to_string().contains("turn-9"), "{error}");
    }

    #[test]
    fn summaries_are_pluralised_and_count_every_action() {
        let empty = restore_summary(&[]);
        assert!(empty.contains("Nothing to restore"), "{empty}");

        let restored = vec![
            RestoredFile {
                path: "a.rs".into(),
                action: RestoreAction::Reverted,
            },
            RestoredFile {
                path: "b.rs".into(),
                action: RestoreAction::Reverted,
            },
            RestoredFile {
                path: "c.rs".into(),
                action: RestoreAction::Removed,
            },
        ];
        let summary = restore_summary(&restored);
        assert!(summary.contains("3 file(s)"), "{summary}");
        assert!(summary.contains("2 restored"), "{summary}");
        assert!(summary.contains("1 removed"), "{summary}");
        assert!(summary.contains("/undo"), "{summary}");
    }

    #[test]
    fn checkpoint_summaries_are_pluralised() {
        let none = Checkpoint {
            id: "t".into(),
            captured_at: "now".into(),
            files: Vec::new(),
        };
        assert_eq!(none.summary(), "no files changed");
        assert!(none.is_empty());

        let one = Checkpoint {
            files: vec![CheckpointFile {
                path: "a.rs".into(),
                original: None,
            }],
            ..none.clone()
        };
        assert_eq!(one.summary(), "1 file");

        let two = Checkpoint {
            files: vec![
                CheckpointFile {
                    path: "a.rs".into(),
                    original: None,
                },
                CheckpointFile {
                    path: "b.rs".into(),
                    original: None,
                },
            ],
            ..none
        };
        assert_eq!(two.summary(), "2 files");
    }
}
