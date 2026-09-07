//! Lock command for protecting sessions from deletion.
//!
//! Provides session protection functionality:
//! - Lock sessions to prevent deletion
//! - Unlock sessions
//! - List locked sessions

use anyhow::{Result, bail};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// Lock CLI command.
#[derive(Debug, Parser)]
pub struct LockCli {
    #[command(subcommand)]
    pub subcommand: Option<LockSubcommand>,

    /// Session ID to lock (if no subcommand)
    #[arg()]
    pub session_id: Option<String>,
}

/// Lock subcommands.
#[derive(Debug, clap::Subcommand)]
pub enum LockSubcommand {
    /// Lock a session
    #[command(visible_alias = "protect")]
    Add(LockAddArgs),

    /// Unlock a session
    #[command(visible_aliases = ["unprotect", "rm"])]
    Remove(LockRemoveArgs),

    /// List locked sessions
    #[command(visible_alias = "ls")]
    List(LockListArgs),

    /// Check if a session is locked
    Check(LockCheckArgs),
}

/// Arguments for lock add command.
#[derive(Debug, Parser)]
pub struct LockAddArgs {
    /// Session ID(s) to lock
    #[arg(required = true)]
    pub session_ids: Vec<String>,

    /// Reason for locking
    #[arg(long, short = 'r')]
    pub reason: Option<String>,
}

/// Arguments for lock remove command.
#[derive(Debug, Parser)]
pub struct LockRemoveArgs {
    /// Session ID(s) to unlock
    #[arg(required = true)]
    pub session_ids: Vec<String>,

    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

/// Arguments for lock list command.
#[derive(Debug, Parser)]
pub struct LockListArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Arguments for lock check command.
#[derive(Debug, Parser)]
pub struct LockCheckArgs {
    /// Session ID to check
    pub session_id: String,
}

/// Lock entry information.
#[derive(Debug, Serialize, Deserialize)]
struct LockEntry {
    session_id: String,
    locked_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

/// Lock file containing all locked sessions.
#[derive(Debug, Default, Serialize, Deserialize)]
struct LockFile {
    version: u32,
    locked_sessions: Vec<LockEntry>,
}

/// Validate session ID format (must be valid UUID or 8-char prefix)
fn validate_session_id(session_id: &str) -> Result<()> {
    // Check if it's a valid UUID
    if uuid::Uuid::parse_str(session_id).is_ok() {
        return Ok(());
    }

    // Check if it's a valid 8-character hex prefix
    if session_id.len() == 8 && session_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(());
    }

    bail!(
        "Invalid session ID: '{}'. Expected a full UUID (e.g., '550e8400-e29b-41d4-a716-446655440000') or an 8-character prefix.",
        session_id
    )
}

/// Get the lock file path.
fn get_lock_file_path() -> Result<PathBuf> {
    Ok(cortex_engine::rollout::local::default_home()?.join("session_locks.json"))
}

/// Load the lock file.
fn load_lock_file() -> Result<LockFile> {
    let path = get_lock_file_path()?;

    if !path.exists() {
        return Ok(LockFile {
            version: 1,
            locked_sessions: Vec::new(),
        });
    }

    let content = std::fs::read_to_string(&path)?;
    let lock_file: LockFile = serde_json::from_str(&content)?;
    Ok(lock_file)
}

/// Save the lock file.
fn save_lock_file(lock_file: &LockFile) -> Result<()> {
    let path = get_lock_file_path()?;

    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(lock_file)?;
    use std::io::Write;
    let mut temp = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid lock path"))?,
    )?;
    temp.write_all(content.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(&path).map_err(|error| error.error)?;
    Ok(())
}

/// Check if a session is locked.
pub fn is_session_locked(session_id: &str) -> bool {
    cortex_engine::rollout::local::SessionStorage::new()
        .and_then(|store| {
            let id = store.resolve_id(session_id)?;
            store.is_protected(&id)
        })
        .unwrap_or(true)
}

impl LockCli {
    /// Run the lock command.
    pub async fn run(self) -> Result<()> {
        match self.subcommand {
            Some(LockSubcommand::Add(args)) => run_add(args).await,
            Some(LockSubcommand::Remove(args)) => run_remove(args).await,
            Some(LockSubcommand::List(args)) => run_list(args).await,
            Some(LockSubcommand::Check(args)) => run_check(args).await,
            None => {
                // Lock the session directly if ID provided
                if let Some(session_id) = self.session_id {
                    run_add(LockAddArgs {
                        session_ids: vec![session_id],
                        reason: None,
                    })
                    .await
                } else {
                    println!("Session Lock Management");
                    println!("{}", "=".repeat(50));
                    println!();
                    println!("Protect sessions from accidental deletion:");
                    println!();
                    println!("  cortex lock <session-id>          Lock a session");
                    println!("  cortex lock add <session-id>      Lock a session");
                    println!("  cortex lock remove <session-id>   Unlock a session");
                    println!("  cortex lock list                  List locked sessions");
                    println!("  cortex lock check <session-id>    Check if locked");
                    println!();
                    println!("Options:");
                    println!("  --reason <text>  Add reason for locking");
                    println!();
                    println!("Example:");
                    println!("  cortex lock abc12345 --reason \"Important conversation\"");
                    Ok(())
                }
            }
        }
    }
}

async fn run_add(mut args: LockAddArgs) -> Result<()> {
    // Validate all session IDs first (Issue #3696)
    for session_id in &args.session_ids {
        validate_session_id(session_id)?;
    }

    let store = cortex_engine::rollout::local::SessionStorage::new()?;
    args.session_ids = args
        .session_ids
        .iter()
        .map(|id| store.resolve_id(id))
        .collect::<Result<_>>()?;
    let mut lock_file = load_lock_file()?;
    let timestamp = chrono::Utc::now().to_rfc3339();

    // Get existing locked session IDs
    let existing_ids: HashSet<String> = lock_file
        .locked_sessions
        .iter()
        .map(|e| e.session_id.clone())
        .collect();

    let mut added = 0;
    let mut skipped = 0;

    for session_id in &args.session_ids {
        if existing_ids.contains(session_id) {
            println!("Session '{}' is already locked.", session_id);
            skipped += 1;
        } else {
            lock_file.locked_sessions.push(LockEntry {
                session_id: session_id.clone(),
                locked_at: timestamp.clone(),
                reason: args.reason.clone(),
            });
            added += 1;
        }
    }

    if added > 0 {
        save_lock_file(&lock_file)?;
        println!(
            "Locked {} session(s){}.",
            added,
            if skipped > 0 {
                format!(" ({} already locked)", skipped)
            } else {
                String::new()
            }
        );
    } else if skipped > 0 {
        println!("All sessions were already locked.");
    }

    Ok(())
}

async fn run_remove(args: LockRemoveArgs) -> Result<()> {
    use std::io::IsTerminal;
    let store = cortex_engine::rollout::local::SessionStorage::new()?;
    let ids = args
        .session_ids
        .iter()
        .map(|id| store.resolve_id(id))
        .collect::<Result<Vec<_>>>()?;
    let mut lock_file = load_lock_file()?;
    if !args.yes {
        if !std::io::stdin().is_terminal() {
            bail!("Unlock requires --yes without an interactive terminal");
        }
        println!("Unlock {} session(s)? [y/N]", ids.len());
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }
    for id in &ids {
        let mut meta = store.load_meta(id)?;
        meta.protected = false;
        store.save_meta(&meta)?;
    }
    lock_file.locked_sessions.retain(|entry| {
        !ids.iter().any(|id| {
            id == &entry.session_id
                || (entry.session_id.len() >= 8 && id.starts_with(&entry.session_id))
        })
    });
    save_lock_file(&lock_file)?;
    println!("Unlocked {} session(s).", ids.len());
    Ok(())
}

async fn run_list(args: LockListArgs) -> Result<()> {
    let lock_file = load_lock_file()?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&lock_file.locked_sessions)?
        );
    } else if lock_file.locked_sessions.is_empty() {
        println!("No sessions are locked.");
        println!();
        println!("Use 'cortex lock <session-id>' to protect a session.");
    } else {
        println!("Locked Sessions:");
        println!("{}", "-".repeat(60));

        for entry in &lock_file.locked_sessions {
            let short_id: String = entry.session_id.chars().take(8).collect();
            println!("  {} - locked at {}", short_id, entry.locked_at);
            if let Some(ref reason) = entry.reason {
                println!("    Reason: {}", reason);
            }
        }

        println!();
        println!(
            "Total: {} locked session(s)",
            lock_file.locked_sessions.len()
        );
    }

    Ok(())
}

async fn run_check(args: LockCheckArgs) -> Result<()> {
    let store = cortex_engine::rollout::local::SessionStorage::new()?;
    let id = store.resolve_id(&args.session_id)?;
    if store.is_protected(&id)? {
        println!("Session '{id}' is LOCKED.");
        Ok(())
    } else {
        bail!("Session '{id}' is not locked")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_entry_serialization_with_reason() {
        let entry = LockEntry {
            session_id: "abc123def456".to_string(),
            locked_at: "2024-01-15T10:30:00Z".to_string(),
            reason: Some("Important session".to_string()),
        };

        let json = serde_json::to_string(&entry).expect("Failed to serialize LockEntry");
        let parsed: LockEntry =
            serde_json::from_str(&json).expect("Failed to deserialize LockEntry");

        assert_eq!(parsed.session_id, "abc123def456");
        assert_eq!(parsed.locked_at, "2024-01-15T10:30:00Z");
        assert_eq!(parsed.reason, Some("Important session".to_string()));
    }

    #[test]
    fn test_lock_entry_serialization_without_reason() {
        let entry = LockEntry {
            session_id: "xyz789".to_string(),
            locked_at: "2024-02-20T15:45:00Z".to_string(),
            reason: None,
        };

        let json = serde_json::to_string(&entry).expect("Failed to serialize LockEntry");

        // Verify reason field is omitted when None (due to skip_serializing_if)
        assert!(!json.contains("reason"));

        let parsed: LockEntry =
            serde_json::from_str(&json).expect("Failed to deserialize LockEntry");

        assert_eq!(parsed.session_id, "xyz789");
        assert_eq!(parsed.locked_at, "2024-02-20T15:45:00Z");
        assert_eq!(parsed.reason, None);
    }

    #[test]
    fn test_lock_entry_deserialization_from_json() {
        let json =
            r#"{"session_id":"test123","locked_at":"2024-03-01T00:00:00Z","reason":"Test reason"}"#;
        let entry: LockEntry =
            serde_json::from_str(json).expect("Failed to deserialize LockEntry from JSON");

        assert_eq!(entry.session_id, "test123");
        assert_eq!(entry.locked_at, "2024-03-01T00:00:00Z");
        assert_eq!(entry.reason, Some("Test reason".to_string()));
    }

    #[test]
    fn test_lock_entry_deserialization_without_reason_field() {
        let json = r#"{"session_id":"no_reason_session","locked_at":"2024-04-10T12:00:00Z"}"#;
        let entry: LockEntry =
            serde_json::from_str(json).expect("Failed to deserialize LockEntry without reason");

        assert_eq!(entry.session_id, "no_reason_session");
        assert_eq!(entry.locked_at, "2024-04-10T12:00:00Z");
        assert_eq!(entry.reason, None);
    }

    #[test]
    fn test_lock_file_default() {
        let lock_file = LockFile::default();

        assert_eq!(lock_file.version, 0);
        assert!(lock_file.locked_sessions.is_empty());
    }

    #[test]
    fn test_lock_file_serialization_empty() {
        let lock_file = LockFile {
            version: 1,
            locked_sessions: Vec::new(),
        };

        let json = serde_json::to_string(&lock_file).expect("Failed to serialize empty LockFile");
        let parsed: LockFile =
            serde_json::from_str(&json).expect("Failed to deserialize empty LockFile");

        assert_eq!(parsed.version, 1);
        assert!(parsed.locked_sessions.is_empty());
    }

    #[test]
    fn test_lock_file_serialization_with_entries() {
        let lock_file = LockFile {
            version: 1,
            locked_sessions: vec![
                LockEntry {
                    session_id: "session_one".to_string(),
                    locked_at: "2024-01-01T00:00:00Z".to_string(),
                    reason: Some("First session".to_string()),
                },
                LockEntry {
                    session_id: "session_two".to_string(),
                    locked_at: "2024-01-02T00:00:00Z".to_string(),
                    reason: None,
                },
            ],
        };

        let json =
            serde_json::to_string(&lock_file).expect("Failed to serialize LockFile with entries");
        let parsed: LockFile =
            serde_json::from_str(&json).expect("Failed to deserialize LockFile with entries");

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.locked_sessions.len(), 2);
        assert_eq!(parsed.locked_sessions[0].session_id, "session_one");
        assert_eq!(
            parsed.locked_sessions[0].reason,
            Some("First session".to_string())
        );
        assert_eq!(parsed.locked_sessions[1].session_id, "session_two");
        assert_eq!(parsed.locked_sessions[1].reason, None);
    }

    #[test]
    fn test_lock_file_deserialization_from_json() {
        let json = r#"{
            "version": 2,
            "locked_sessions": [
                {"session_id": "sess_abc", "locked_at": "2024-05-15T08:00:00Z", "reason": "Production"}
            ]
        }"#;

        let lock_file: LockFile =
            serde_json::from_str(json).expect("Failed to deserialize LockFile from JSON");

        assert_eq!(lock_file.version, 2);
        assert_eq!(lock_file.locked_sessions.len(), 1);
        assert_eq!(lock_file.locked_sessions[0].session_id, "sess_abc");
        assert_eq!(
            lock_file.locked_sessions[0].reason,
            Some("Production".to_string())
        );
    }

    #[test]
    fn test_get_lock_file_path_returns_valid_path() {
        let path = get_lock_file_path().unwrap();

        // Path should end with session_locks.json
        assert!(path.ends_with("session_locks.json"));

        // Path should include .cortex directory
        let path_str = path.to_string_lossy();
        assert!(path_str.contains(".cortex"));
    }
}
