//! Session export command for Cortex CLI.
//!
//! Exports a session to a portable JSON format that can be shared or imported.

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Export format for sessions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ExportFormat {
    /// JSON format (default)
    #[default]
    Json,
    /// YAML format
    Yaml,
    /// CSV format (simplified, messages only)
    Csv,
}

/// Export a session to JSON format.
#[derive(Debug, Parser)]
pub struct ExportCommand {
    /// Session ID to export (interactive picker if not provided)
    #[arg(value_name = "SESSION_ID")]
    pub session_id: Option<String>,

    /// Output file path (stdout if not specified)
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Output format (json, yaml, csv)
    #[arg(short, long, value_enum, default_value_t = ExportFormat::Json)]
    pub format: ExportFormat,

    /// Pretty-print the output (for json/yaml)
    #[arg(long, default_value_t = true)]
    pub pretty: bool,
}

/// Portable session export format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionExport {
    /// Export format version.
    pub version: u32,
    /// Session metadata.
    pub session: SessionMetadata,
    /// Conversation messages.
    pub messages: Vec<ExportMessage>,
}

/// Session metadata in export format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Session ID.
    pub id: String,
    /// Session title (derived from first user message or cwd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Working directory where session was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Model used for the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Agent used for the session (if any).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent: Option<String>,
    /// Agent references found in messages (for validation during import).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_refs: Option<Vec<String>>,
}

/// Message in export format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMessage {
    /// Message role (user, assistant, system, tool).
    pub role: String,
    /// Message content.
    pub content: String,
    /// Tool calls made by the assistant (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ExportToolCall>>,
    /// Tool call ID this message is responding to (for tool results).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Message timestamp (ISO 8601).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// Tool call in export format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportToolCall {
    /// Tool call ID.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Tool arguments as JSON.
    pub arguments: serde_json::Value,
}

impl ExportCommand {
    /// Run the export command.
    pub async fn run(self) -> Result<()> {
        let store = cortex_engine::rollout::local::SessionStorage::new()?;
        let session_id = self
            .session_id
            .context("Specify the session ID to export (see cortex sessions)")?;
        let export = store.document(&session_id)?;
        let messages = &export.messages;

        // Serialize to the requested format
        let output_content = match self.format {
            ExportFormat::Json => {
                if self.pretty {
                    serde_json::to_string_pretty(&export)?
                } else {
                    serde_json::to_string(&export)?
                }
            }
            ExportFormat::Yaml => {
                serde_yaml::to_string(&export).with_context(|| "Failed to serialize to YAML")?
            }
            ExportFormat::Csv => {
                // CSV format: simplified, messages only
                let mut csv_output = String::new();
                csv_output.push_str("timestamp,role,content\n");
                for msg in messages {
                    let timestamp = msg.timestamp.to_rfc3339();
                    // Escape CSV content: double quotes, wrap in quotes if contains comma/newline
                    let content = escape_csv_field(&msg.content);
                    csv_output.push_str(&format!("{},{},{}\n", timestamp, msg.role, content));
                }
                csv_output
            }
        };

        // Write to output
        match self.output {
            Some(path) => {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .with_context(|| {
                        format!(
                            "Cannot create export (existing files are never overwritten): {}",
                            path.display()
                        )
                    })?;
                file.write_all(output_content.as_bytes())?;
                file.sync_all()?;
                eprintln!("Exported session to: {}", path.display());
            }
            None => {
                println!("{output_content}");
            }
        }

        Ok(())
    }
}

/// Escape a field for CSV output.
fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        // Escape double quotes by doubling them, wrap in quotes
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Returns a deduplicated list of agent names that are referenced.
#[cfg(test)]
fn extract_agent_refs(messages: &[ExportMessage]) -> Vec<String> {
    use std::collections::HashSet;

    // Regex to match @agent mentions (e.g., @explore, @general, @my-custom-agent)
    let re = regex::Regex::new(r"@([a-zA-Z][a-zA-Z0-9_-]*)").unwrap();

    let mut agent_refs: HashSet<String> = HashSet::new();

    for message in messages {
        for cap in re.captures_iter(&message.content) {
            if let Some(agent_name) = cap.get(1) {
                agent_refs.insert(agent_name.as_str().to_string());
            }
        }
    }

    let mut refs: Vec<String> = agent_refs.into_iter().collect();
    refs.sort();
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_export_serialization() {
        let export = SessionExport {
            version: 1,
            session: SessionMetadata {
                id: "test-id".to_string(),
                title: Some("Test Session".to_string()),
                created_at: "2024-01-01T00:00:00Z".to_string(),
                cwd: Some("/home/user".to_string()),
                model: Some("claude-3".to_string()),
                agent: None,
                agent_refs: None,
            },
            messages: vec![
                ExportMessage {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                    tool_calls: None,
                    tool_call_id: None,
                    timestamp: Some("2024-01-01T00:00:01Z".to_string()),
                },
                ExportMessage {
                    role: "assistant".to_string(),
                    content: "Hi there!".to_string(),
                    tool_calls: None,
                    tool_call_id: None,
                    timestamp: Some("2024-01-01T00:00:02Z".to_string()),
                },
            ],
        };

        let json = serde_json::to_string_pretty(&export).unwrap();
        assert!(json.contains("\"version\": 1"));
        assert!(json.contains("\"role\": \"user\""));
        assert!(json.contains("\"content\": \"Hello\""));
    }

    #[test]
    fn test_extract_agent_refs() {
        let messages = vec![
            ExportMessage {
                role: "user".to_string(),
                content: "@explore find the main function".to_string(),
                tool_calls: None,
                tool_call_id: None,
                timestamp: None,
            },
            ExportMessage {
                role: "user".to_string(),
                content: "@research analyze this code @my-agent".to_string(),
                tool_calls: None,
                tool_call_id: None,
                timestamp: None,
            },
        ];

        let refs = extract_agent_refs(&messages);
        assert_eq!(refs.len(), 3);
        assert!(refs.contains(&"explore".to_string()));
        assert!(refs.contains(&"research".to_string()));
        assert!(refs.contains(&"my-agent".to_string()));
    }

    #[test]
    fn test_extract_agent_refs_no_duplicates() {
        let messages = vec![
            ExportMessage {
                role: "user".to_string(),
                content: "@explore task 1".to_string(),
                tool_calls: None,
                tool_call_id: None,
                timestamp: None,
            },
            ExportMessage {
                role: "user".to_string(),
                content: "@explore task 2".to_string(),
                tool_calls: None,
                tool_call_id: None,
                timestamp: None,
            },
        ];

        let refs = extract_agent_refs(&messages);
        assert_eq!(refs.len(), 1);
        assert!(refs.contains(&"explore".to_string()));
    }
}
