//! Canonical local sessions, with non-destructive reads of legacy rollouts.
//! This store does not imply equivalence with server REST/WS session IDs.

mod compatibility;
mod safety;
mod storage;
pub mod types;

pub use storage::SessionStorage;
pub use types::{SessionMeta, SessionSummary, StoredMessage, StoredToolCall, TokenUsageInfo};

use anyhow::{Context, Result, bail};
use std::path::PathBuf;

/// The local data root. Does not inspect authentication or create files.
pub fn default_home() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("CORTEX_HOME") {
        return Ok(PathBuf::from(home));
    }
    let base = if cfg!(windows) {
        dirs::config_dir().map(|p| p.join("cortex"))
    } else {
        dirs::home_dir().map(|p| p.join(".cortex"))
    };
    base.context("Could not determine session data directory")
}

/// IDs are opaque names, never paths. Legacy non-UUID names remain readable.
pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        bail!("Invalid session ID");
    }
    Ok(())
}

/// Full-fidelity, versioned interchange shared by CLI and TUI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionDocument {
    pub version: u32,
    pub meta: SessionMeta,
    pub messages: Vec<StoredMessage>,
}

impl SessionStorage {
    pub fn document(&self, id: &str) -> Result<SessionDocument> {
        let id = self.resolve_id(id)?;
        Ok(SessionDocument {
            version: 2,
            meta: self.load_meta(&id)?,
            messages: self.load_messages(&id)?,
        })
    }

    /// Import always creates a distinct identity, even when the source exists.
    pub fn import_document(&self, mut document: SessionDocument) -> Result<String> {
        if document.version != 2 {
            bail!("Unsupported session document version: {}", document.version);
        }
        if document.messages.len() > 100_000 {
            bail!("Session contains too many messages");
        }
        for message in &document.messages {
            if !matches!(
                message.role.as_str(),
                "user" | "assistant" | "system" | "tool"
            ) {
                bail!("Unsupported message role: {}", message.role);
            }
        }
        document.meta.forked_from = Some(document.meta.id.clone());
        document.meta.id = uuid::Uuid::new_v4().to_string();
        document.meta.code_session_id = None;
        document.meta.protected = false;
        document.meta.message_count = document.messages.len() as u32;
        self.create_session(&document.meta, &document.messages)?;
        Ok(document.meta.id)
    }
}

#[cfg(test)]
mod ux_contract_tests;
