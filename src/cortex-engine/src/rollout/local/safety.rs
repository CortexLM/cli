use super::{SessionMeta, SessionStorage, StoredMessage, validate_id};
use anyhow::{Context, Result, bail};
use std::fs;
use std::io::Write;
use std::path::Path;

impl SessionStorage {
    pub(super) fn check_regular_file(&self, path: &Path) -> Result<()> {
        if !fs::symlink_metadata(path)?.file_type().is_file() {
            bail!("Session data must be a regular file");
        }
        Ok(())
    }

    pub(super) fn check_session_path(&self, id: &str) -> Result<()> {
        validate_id(id)?;
        for path in [self.base_dir().to_path_buf(), self.session_dir(id)] {
            if let Ok(meta) = fs::symlink_metadata(path)
                && (!meta.is_dir() || meta.file_type().is_symlink())
            {
                bail!("Session directory must not be a symbolic link or file");
            }
        }
        Ok(())
    }

    pub(super) fn atomic_write(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        if fs::symlink_metadata(path).is_ok() {
            self.check_regular_file(path)?;
        }
        let parent = path.parent().context("Invalid storage path")?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(bytes)?;
        temp.as_file().sync_all()?;
        temp.persist(path).map_err(|e| e.error)?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    }

    /// Create a complete record before publishing it in the inventory.
    pub fn create_session(&self, meta: &SessionMeta, messages: &[StoredMessage]) -> Result<()> {
        self.check_session_path(&meta.id)?;
        if self.session_dir(&meta.id).exists() {
            bail!("Session already exists");
        }
        self.ensure_base_dir()?;
        let staging = tempfile::Builder::new()
            .prefix(".new-")
            .tempdir_in(self.base_dir())?;
        let mut history = Vec::new();
        for message in messages {
            serde_json::to_writer(&mut history, message)?;
            history.push(b'\n');
        }
        let mut meta = meta.clone();
        meta.message_count = messages.len() as u32;
        self.atomic_write(&staging.path().join("history.jsonl"), &history)?;
        self.atomic_write(
            &staging.path().join("meta.json"),
            &serde_json::to_vec_pretty(&meta)?,
        )?;
        fs::rename(staging.path(), self.session_dir(&meta.id))?;
        Ok(())
    }

    /// Promotion retains the legacy rollout verbatim as a recovery source.
    // ponytail: read-compatible promotion only; headless writers must adopt this
    // store before simultaneous CLI/TUI continuation of the same ID is supported.
    pub(super) fn materialize_legacy(&self, id: &str) -> Result<()> {
        if !self.session_dir(id).exists() && self.base_dir().join(format!("{id}.jsonl")).exists() {
            let (meta, messages) = self.legacy_document(id)?;
            self.create_session(&meta, &messages)?;
        }
        Ok(())
    }

    /// Includes legacy lock files. Unreadable/corrupt protection fails closed.
    pub fn is_protected(&self, id: &str) -> Result<bool> {
        if self.load_meta(id)?.protected {
            return Ok(true);
        }
        let locks = self
            .base_dir()
            .parent()
            .context("Invalid sessions directory")?
            .join("session_locks.json");
        if !locks.exists() {
            return Ok(false);
        }
        self.check_regular_file(&locks)?;
        let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(locks)?)?;
        let entries = value["locked_sessions"]
            .as_array()
            .context("Invalid session locks")?;
        for entry in entries {
            let locked = entry["session_id"]
                .as_str()
                .context("Invalid session lock ID")?;
            validate_id(locked)?;
            if id == locked || (locked.len() >= 8 && id.starts_with(locked)) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
