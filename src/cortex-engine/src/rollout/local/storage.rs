//! Session file storage operations.
//!
//! Handles reading and writing session data to the filesystem.
//! Uses a directory-per-session structure with atomic writes for safety.

use super::validate_id;
use anyhow::{Context, Result, bail};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use super::types::{SessionMeta, SessionSummary, StoredMessage};

// ============================================================
// CONSTANTS
// ============================================================

/// Metadata file name.
const META_FILE: &str = "meta.json";

/// History file name.
const HISTORY_FILE: &str = "history.jsonl";

// ============================================================
// SESSION STORAGE
// ============================================================

/// Handles session file operations.
#[derive(Clone)]
pub struct SessionStorage {
    /// Base directory for all sessions.
    base_dir: PathBuf,
}

impl SessionStorage {
    /// Creates a new SessionStorage with the default directory.
    pub fn new() -> Result<Self> {
        let base_dir = super::default_home()?.join("sessions");
        Ok(Self { base_dir })
    }

    /// Creates a new SessionStorage with a custom directory.
    pub fn with_dir(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Gets the base directory.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Gets the directory for a specific session.
    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        self.base_dir.join(session_id)
    }

    /// Gets the metadata file path for a session.
    pub fn meta_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join(META_FILE)
    }

    /// Gets the history file path for a session.
    pub fn history_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join(HISTORY_FILE)
    }

    /// Checks if a session exists.
    pub fn exists(&self, session_id: &str) -> bool {
        validate_id(session_id).is_ok()
            && (self.meta_path(session_id).exists()
                || self.base_dir.join(format!("{session_id}.jsonl")).is_file())
    }

    /// Ensures the base directory exists.
    pub fn ensure_base_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.base_dir)
            .with_context(|| format!("Failed to create sessions directory: {:?}", self.base_dir))
    }

    /// Ensures a session directory exists.
    pub fn ensure_session_dir(&self, session_id: &str) -> Result<PathBuf> {
        self.check_session_path(session_id)?;
        self.materialize_legacy(session_id)?;
        let dir = self.session_dir(session_id);
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create session directory: {:?}", dir))?;
        Ok(dir)
    }

    // ========================================================================
    // METADATA OPERATIONS
    // ========================================================================

    /// Saves session metadata.
    pub fn save_meta(&self, meta: &SessionMeta) -> Result<()> {
        self.ensure_session_dir(&meta.id)?;

        let path = self.meta_path(&meta.id);
        let content =
            serde_json::to_string_pretty(meta).context("Failed to serialize session metadata")?;

        self.atomic_write(&path, content.as_bytes())?;

        Ok(())
    }

    /// Loads session metadata.
    pub fn load_meta(&self, session_id: &str) -> Result<SessionMeta> {
        self.check_session_path(session_id)?;
        let path = self.meta_path(session_id);
        if !path.exists() {
            return self.legacy_document(session_id).map(|(meta, _)| meta);
        }
        self.check_regular_file(&path)?;
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read metadata file: {:?}", path))?;
        let meta: SessionMeta = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse metadata file: {:?}", path))?;
        if meta.id != session_id {
            bail!("Session metadata ID mismatch");
        }
        Ok(meta)
    }

    // ========================================================================
    // HISTORY OPERATIONS
    // ========================================================================

    /// Appends a message to the history file.
    ///
    /// This function ensures data durability by:
    /// 1. Flushing the BufWriter to the OS buffer
    /// 2. Calling sync_all() to force data to disk (fsync)
    ///
    /// This prevents data loss on crash or forceful termination.
    pub fn append_message(&self, session_id: &str, message: &StoredMessage) -> Result<()> {
        self.ensure_session_dir(session_id)?;

        let path = self.history_path(session_id);
        if fs::symlink_metadata(&path).is_ok() {
            self.check_regular_file(&path)?;
        }
        let mut options = fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&path)
            .with_context(|| format!("Failed to open history file: {:?}", path))?;

        let mut writer = BufWriter::new(file);
        let line = serde_json::to_string(message).context("Failed to serialize message")?;
        writeln!(writer, "{}", line)
            .with_context(|| format!("Failed to write to history file: {:?}", path))?;
        writer.flush()?;

        // Ensure data is durably written to disk (fsync) to prevent data loss on crash
        writer
            .get_ref()
            .sync_all()
            .with_context(|| format!("Failed to sync history file to disk: {:?}", path))?;

        Ok(())
    }

    /// Loads all messages from the history file.
    pub fn load_messages(&self, session_id: &str) -> Result<Vec<StoredMessage>> {
        self.check_session_path(session_id)?;
        if !self.meta_path(session_id).exists() && self.exists(session_id) {
            return self
                .legacy_document(session_id)
                .map(|(_, messages)| messages);
        }
        let path = self.history_path(session_id);

        if !path.exists() {
            return Ok(vec![]);
        }

        self.check_regular_file(&path)?;
        let file = File::open(&path)
            .with_context(|| format!("Failed to open history file: {:?}", path))?;
        let reader = BufReader::new(file);

        let mut messages = Vec::new();
        for (line_num, line_result) in reader.lines().enumerate() {
            let line = line_result.with_context(|| {
                format!("Failed to read line {} from history file", line_num + 1)
            })?;

            if line.trim().is_empty() {
                continue;
            }

            let message: StoredMessage = serde_json::from_str(&line)
                .with_context(|| format!("Failed to parse message at line {}", line_num + 1))?;
            messages.push(message);
        }

        Ok(messages)
    }

    /// Rewrites the entire history file (used for fork/compact/undo).
    pub fn rewrite_messages(&self, session_id: &str, messages: &[StoredMessage]) -> Result<()> {
        self.rewrite_history(session_id, messages)
    }

    /// Rewrites the entire history file (used for fork/compact).
    ///
    /// Uses atomic write (temp file + rename) with fsync for durability.
    pub fn rewrite_history(&self, session_id: &str, messages: &[StoredMessage]) -> Result<()> {
        self.ensure_session_dir(session_id)?;

        let path = self.history_path(session_id);
        let mut bytes = Vec::new();
        for message in messages {
            serde_json::to_writer(&mut bytes, message)?;
            bytes.push(b'\n');
        }
        self.atomic_write(&path, &bytes)?;

        Ok(())
    }

    // ========================================================================
    // SESSION LISTING
    // ========================================================================

    /// Lists all sessions (sorted by updated_at descending).
    pub fn list_sessions(&self) -> Result<Vec<SessionSummary>> {
        let mut summaries = Vec::new();
        if !self.base_dir.exists() {
            return Ok(summaries);
        }
        let mut ids = std::collections::BTreeSet::new();
        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let id = if kind.is_dir() {
                name
            } else if kind.is_file() {
                name.strip_suffix(".jsonl").unwrap_or("")
            } else {
                continue;
            };
            if validate_id(id).is_ok() {
                ids.insert(id.to_owned());
            }
        }
        for id in ids {
            // Damaged records are not silently dropped from the inventory.
            let meta = self.load_meta(&id)?;
            summaries.push(SessionSummary::from(&meta));
        }

        // Sort by updated_at descending (most recent first)
        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(summaries)
    }

    /// Lists recent sessions (non-archived, limited count).
    pub fn list_recent_sessions(&self, limit: usize) -> Result<Vec<SessionSummary>> {
        let all = self.list_sessions()?;
        Ok(all
            .into_iter()
            .filter(|s| !s.archived)
            .take(limit)
            .collect())
    }

    // ========================================================================
    // SESSION DELETION
    // ========================================================================

    /// Deletes a session and all its files.
    pub fn delete_session(&self, session_id: &str) -> Result<()> {
        self.delete_session_with_force(session_id, false)
    }

    /// Deletion moves data to a unique trash directory, preserving recoverability.
    pub fn delete_session_with_force(&self, session_id: &str, force: bool) -> Result<()> {
        self.check_session_path(session_id)?;
        if !force && self.is_protected(session_id)? {
            bail!("Session is protected; unlock it or explicitly use --force");
        }
        self.load_meta(session_id)?;
        let trash_root = self.base_dir.join(".trash");
        if let Ok(metadata) = fs::symlink_metadata(&trash_root)
            && (!metadata.is_dir() || metadata.file_type().is_symlink())
        {
            bail!("Session trash must be a regular directory");
        }
        let legacy_path = self.base_dir.join(format!("{session_id}.jsonl"));
        if fs::symlink_metadata(&legacy_path).is_ok() {
            self.check_regular_file(&legacy_path)?;
        }
        let trash = trash_root.join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&trash)?;
        let dir = self.session_dir(session_id);
        if dir.exists() {
            fs::rename(&dir, trash.join(session_id))?;
        }
        let legacy = self.base_dir.join(format!("{session_id}.jsonl"));
        if legacy.exists() {
            self.check_regular_file(&legacy)?;
            fs::rename(&legacy, trash.join(format!("{session_id}.jsonl")))?;
        }
        Ok(())
    }

    /// Archives a session (sets archived flag in metadata).
    pub fn archive_session(&self, session_id: &str) -> Result<()> {
        let mut meta = self.load_meta(session_id)?;
        meta.archived = true;
        self.save_meta(&meta)?;
        Ok(())
    }

    /// Unarchives a session.
    pub fn unarchive_session(&self, session_id: &str) -> Result<()> {
        let mut meta = self.load_meta(session_id)?;
        meta.archived = false;
        self.save_meta(&meta)?;
        Ok(())
    }

    // ========================================================================
    // FORK OPERATIONS
    // ========================================================================

    /// Forks a session, copying messages up to a certain point.
    pub fn fork_session(
        &self,
        source_id: &str,
        new_meta: &SessionMeta,
        up_to_message_id: Option<&str>,
    ) -> Result<()> {
        if source_id == new_meta.id || self.exists(&new_meta.id) {
            bail!("Fork destination already exists");
        }
        let mut messages = self.load_messages(source_id)?;
        if let Some(id) = up_to_message_id {
            let index = messages
                .iter()
                .position(|m| m.id == id)
                .context("Fork message not found")?;
            messages.truncate(index + 1);
        }
        let mut meta = new_meta.clone();
        meta.message_count = messages.len() as u32;
        meta.total_input_tokens = messages
            .iter()
            .filter_map(|m| m.tokens)
            .map(|t| t.input_tokens)
            .sum();
        meta.total_output_tokens = messages
            .iter()
            .filter_map(|m| m.tokens)
            .map(|t| t.output_tokens)
            .sum();
        self.create_session(&meta, &messages)?;

        Ok(())
    }
}

impl Default for SessionStorage {
    fn default() -> Self {
        Self::new().expect("Failed to create session storage")
    }
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_storage() -> (SessionStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = SessionStorage::with_dir(temp_dir.path().to_path_buf());
        (storage, temp_dir)
    }

    #[test]
    fn test_session_storage_paths() {
        let (storage, _temp) = create_test_storage();
        let session_id = "test-session-123";

        assert!(
            storage
                .session_dir(session_id)
                .ends_with("test-session-123")
        );
        assert!(storage.meta_path(session_id).ends_with("meta.json"));
        assert!(storage.history_path(session_id).ends_with("history.jsonl"));
    }

    #[test]
    fn test_save_and_load_meta() {
        let (storage, _temp) = create_test_storage();
        let meta = SessionMeta::new("cortex", "test-model");
        let session_id = meta.id.clone();

        storage.save_meta(&meta).unwrap();
        assert!(storage.exists(&session_id));

        let loaded = storage.load_meta(&session_id).unwrap();
        assert_eq!(loaded.id, meta.id);
        assert_eq!(loaded.provider, "cortex");
    }

    #[test]
    fn test_append_and_load_messages() {
        let (storage, _temp) = create_test_storage();
        let meta = SessionMeta::new("cortex", "test-model");
        let session_id = meta.id.clone();

        storage.save_meta(&meta).unwrap();

        let msg1 = StoredMessage::user("Hello!");
        let msg2 = StoredMessage::assistant("Hi there!");

        storage.append_message(&session_id, &msg1).unwrap();
        storage.append_message(&session_id, &msg2).unwrap();

        let messages = storage.load_messages(&session_id).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "Hello!");
        assert_eq!(messages[1].content, "Hi there!");
    }

    #[test]
    fn test_list_sessions() {
        let (storage, _temp) = create_test_storage();

        // Create a few sessions
        for i in 0..3 {
            let meta = SessionMeta::new("cortex", &format!("model-{}", i));
            storage.save_meta(&meta).unwrap();
        }

        let sessions = storage.list_sessions().unwrap();
        assert_eq!(sessions.len(), 3);
    }

    #[test]
    fn test_delete_session() {
        let (storage, _temp) = create_test_storage();
        let meta = SessionMeta::new("cortex", "test-model");
        let session_id = meta.id.clone();

        storage.save_meta(&meta).unwrap();
        assert!(storage.exists(&session_id));

        storage.delete_session(&session_id).unwrap();
        assert!(!storage.exists(&session_id));
    }

    #[test]
    fn test_archive_session() {
        let (storage, _temp) = create_test_storage();
        let meta = SessionMeta::new("cortex", "test-model");
        let session_id = meta.id.clone();

        storage.save_meta(&meta).unwrap();
        storage.archive_session(&session_id).unwrap();

        let loaded = storage.load_meta(&session_id).unwrap();
        assert!(loaded.archived);
    }
}
