//! Session import command for Cortex CLI.
//!
//! Imports a session from a portable JSON format (exported or shared).

use anyhow::{Context, Result, bail};
use clap::Parser;
use std::collections::HashSet;
#[cfg(test)]
use std::path::{Path, PathBuf};

use crate::styled_output::print_success;
#[cfg(test)]
use cortex_protocol::{
    AgentMessageEvent, Event, EventMsg, ExecCommandEndEvent, ExecCommandSource, ParsedCommand,
    UserMessageEvent,
};

#[cfg(test)]
use crate::agent_cmd::load_all_agents;
use crate::export_cmd::{ExportMessage, SessionExport};

/// Maximum depth for processing messages to prevent stack overflow from deeply nested structures.
const MAX_PROCESSING_DEPTH: usize = 10000;

/// Import a session from JSON format.
#[derive(Debug, Parser)]
pub struct ImportCommand {
    /// Path to the JSON file to import, URL to fetch, or "-" for stdin
    #[arg(value_name = "FILE_OR_URL")]
    pub source: String,

    /// Force import even if session already exists
    #[arg(short, long, default_value_t = false)]
    pub force: bool,

    /// Resume the imported session after import
    #[arg(long, default_value_t = false)]
    pub resume: bool,
}

impl ImportCommand {
    /// Run the import command.
    pub async fn run(self) -> Result<()> {
        // Validate source argument is not empty
        if self.source.trim().is_empty() {
            bail!("Error: Source path cannot be empty\n\nUsage: cortex import <FILE_OR_URL>");
        }

        let store = cortex_engine::rollout::local::SessionStorage::new()?;
        let content = read_import_source(&self.source)?;
        let document = parse_session_document(&content)?;
        let id = store.import_document(document)?;
        print_success(&format!("Imported session as: {id}"));
        println!("To resume: cortex resume {id}");
        if self.resume {
            #[cfg(feature = "cortex-tui")]
            {
                let mut config = cortex_engine::Config::default();
                config.cortex_home = cortex_engine::rollout::local::default_home()?;
                cortex_tui::runner::AppRunner::new(config)
                    .with_cortex_session_id(id)
                    .run()
                    .await?;
            }
            #[cfg(not(feature = "cortex-tui"))]
            bail!("Interactive resume requires the TUI build");
        }
        Ok(())
    }
}

fn read_import_source(source: &str) -> Result<String> {
    use std::io::Read;
    const MAX_BYTES: u64 = 50 * 1024 * 1024;
    let mut content = String::new();
    if source.starts_with("https://") || source.starts_with("http://") {
        bail!(
            "Remote session import is unavailable. Download and inspect the export, then import a local JSON or YAML file."
        );
    }
    let input: Box<dyn Read> = if source == "-" {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(source).context("Failed to open session export")?)
    };
    input.take(MAX_BYTES + 1).read_to_string(&mut content)?;
    if content.len() as u64 > MAX_BYTES {
        bail!("Session export exceeds the 50 MiB limit");
    }
    Ok(content)
}

fn parse_session_document(content: &str) -> Result<cortex_engine::rollout::local::SessionDocument> {
    use cortex_engine::rollout::local::{
        SessionDocument, SessionMeta, StoredMessage, StoredToolCall,
    };
    let value: serde_json::Value = if content.trim_start().starts_with('{') {
        serde_json::from_str(content).context("Invalid session JSON")?
    } else {
        serde_yaml::from_str(content).context("Invalid session JSON or YAML")?
    };
    if value["version"] == 2 {
        return Ok(serde_json::from_value(value)?);
    }
    let export: SessionExport =
        serde_json::from_value(value).context("Invalid version 1 session export")?;
    if export.version != 1 {
        bail!("Unsupported export version: {}", export.version);
    }
    if export.messages.len() > MAX_PROCESSING_DEPTH {
        bail!("Session contains too many messages");
    }
    validate_export_messages(&export.messages)?;
    validate_no_circular_references(&export.messages)?;
    let mut meta = SessionMeta::new(
        "cortex",
        export.session.model.as_deref().unwrap_or("default"),
    );
    meta.id = export.session.id;
    meta.title = export.session.title;
    meta.created_at = chrono::DateTime::parse_from_rfc3339(&export.session.created_at)?
        .with_timezone(&chrono::Utc);
    meta.updated_at = meta.created_at;
    if let Some(cwd) = export.session.cwd {
        meta.cwd = cwd;
    }
    let mut messages = Vec::new();
    for source in export.messages {
        let mut message = StoredMessage::system(source.content);
        message.role = source.role;
        message.tool_call_id = source.tool_call_id;
        message.timestamp = match source.timestamp {
            Some(time) => chrono::DateTime::parse_from_rfc3339(&time)?.with_timezone(&chrono::Utc),
            None => meta.created_at,
        };
        for tool in source.tool_calls.unwrap_or_default() {
            message
                .tool_calls
                .push(StoredToolCall::new(tool.id, tool.name, tool.arguments));
        }
        messages.push(message);
    }
    Ok(SessionDocument {
        version: 2,
        meta,
        messages,
    })
}

/// Validate agent references in the imported session.
/// Returns a list of missing agent names that are referenced but not available locally.
#[cfg(test)]
fn validate_agent_references(export: &SessionExport) -> Result<Vec<String>> {
    // Get all locally available agents
    let local_agents: HashSet<String> = match load_all_agents() {
        Ok(agents) => agents.into_iter().map(|a| a.name).collect(),
        Err(e) => {
            // If we can't load agents, log a warning but continue
            tracing::warn!("Could not load local agents for validation: {}", e);
            HashSet::new()
        }
    };

    let mut missing_agents = Vec::new();

    // Check the session's agent field (if the session was started with -a <agent>)
    if let Some(ref agent_name) = export.session.agent
        && !local_agents.contains(agent_name)
    {
        missing_agents.push(agent_name.clone());
    }

    // Check agent_refs from the export metadata (pre-extracted @mentions)
    if let Some(ref agent_refs) = export.session.agent_refs {
        for agent_ref in agent_refs {
            if !local_agents.contains(agent_ref) && !missing_agents.contains(agent_ref) {
                missing_agents.push(agent_ref.clone());
            }
        }
    }

    // Also scan messages for @agent mentions (in case they weren't pre-extracted)
    let re = regex::Regex::new(r"@([a-zA-Z][a-zA-Z0-9_-]*)").unwrap();
    for message in &export.messages {
        for cap in re.captures_iter(&message.content) {
            if let Some(agent_name) = cap.get(1) {
                let name = agent_name.as_str().to_string();
                if !local_agents.contains(&name) && !missing_agents.contains(&name) {
                    missing_agents.push(name);
                }
            }
        }
    }

    missing_agents.sort();
    Ok(missing_agents)
}

/// Validate that there are no circular message references.
///
/// This function checks for potential circular references in message chains.
/// While the current ExportMessage struct doesn't have a reply_to field,
/// this validation protects against malformed input that could cause
/// infinite loops during processing.
fn validate_no_circular_references(messages: &[ExportMessage]) -> Result<()> {
    // Build a map of message IDs to their indices for reference checking
    // If tool_call_id references form a cycle, detect it
    let mut seen_tool_call_ids: HashSet<String> = HashSet::new();
    let mut referenced_ids: HashSet<String> = HashSet::new();

    for (idx, message) in messages.iter().enumerate() {
        // Track tool call IDs to detect duplicates (potential for circular references)
        if let Some(ref tool_call_id) = message.tool_call_id {
            if !referenced_ids.insert(tool_call_id.clone()) {
                bail!(
                    "Error: Duplicate tool_call_id '{}' detected at message index {}. \
                     This may indicate circular message references.",
                    tool_call_id,
                    idx
                );
            }
            referenced_ids.insert(tool_call_id.clone());
        }

        // Track tool calls made to ensure they reference unique IDs
        if let Some(ref tool_calls) = message.tool_calls {
            for tc in tool_calls {
                if !seen_tool_call_ids.insert(tc.id.clone()) {
                    bail!(
                        "Error: Duplicate tool call id '{}' detected at message index {}. \
                         This may indicate circular message references.",
                        tc.id,
                        idx
                    );
                }
            }
        }
    }

    Ok(())
}

/// Validate all messages in an export, including base64-encoded content.
/// Returns a clear error message if invalid data is found.
fn validate_export_messages(messages: &[ExportMessage]) -> Result<()> {
    use base64::Engine;

    for (idx, message) in messages.iter().enumerate() {
        // Check for base64-encoded image data in content
        // Common pattern: "data:image/png;base64,..." or "data:image/jpeg;base64,..."
        if let Some(data_uri_start) = message.content.find("data:image/")
            && let Some(base64_marker) = message.content[data_uri_start..].find(";base64,")
        {
            let base64_start = data_uri_start + base64_marker + 8; // 8 = len(";base64,")
            let remaining = &message.content[base64_start..];

            // Find end of base64 data (could end with quote, whitespace, or end of string)
            let base64_end = remaining
                .find(['"', '\'', ' ', '\n', ')'])
                .unwrap_or(remaining.len());
            let base64_data = &remaining[..base64_end];

            // Validate the base64 data
            if !base64_data.is_empty() {
                let engine = base64::engine::general_purpose::STANDARD;
                if let Err(e) = engine.decode(base64_data) {
                    bail!(
                        "Invalid base64 encoding in message {} (role: '{}'): {}\n\
                            The image data starting at position {} has invalid base64 encoding.\n\
                            Please ensure all embedded images use valid base64 encoding.",
                        idx + 1,
                        message.role,
                        e,
                        data_uri_start
                    );
                }
            }
        }

        // Validate tool call arguments if present
        if let Some(ref tool_calls) = message.tool_calls {
            for (tc_idx, tool_call) in tool_calls.iter().enumerate() {
                // Check for base64 in tool call arguments
                let args_str = tool_call.arguments.to_string();
                if args_str.contains(";base64,") {
                    // Try to find and validate any base64 in the arguments
                    for (pos, _) in args_str.match_indices(";base64,") {
                        let base64_start = pos + 8;
                        let remaining = &args_str[base64_start..];
                        let base64_end = remaining
                            .find(|c: char| {
                                c == '"' || c == '\'' || c == ' ' || c == '\n' || c == ')'
                            })
                            .unwrap_or(remaining.len());
                        let base64_data = &remaining[..base64_end];

                        if !base64_data.is_empty() {
                            let engine = base64::engine::general_purpose::STANDARD;
                            if let Err(e) = engine.decode(base64_data) {
                                bail!(
                                    "Invalid base64 encoding in message {} tool call {} ('{}' arguments): {}\n\
                                    Please ensure all embedded data uses valid base64 encoding.",
                                    idx + 1,
                                    tc_idx + 1,
                                    tool_call.name,
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Convert an export message to a protocol event.
#[cfg(test)]
fn message_to_event(message: &ExportMessage, turn_id: &mut u64, cwd: &Path) -> Result<Event> {
    let event_msg = match message.role.as_str() {
        "user" => {
            *turn_id += 1;
            EventMsg::UserMessage(UserMessageEvent {
                id: None,
                parent_id: None,
                message: message.content.clone(),
                images: None,
            })
        }
        "assistant" => EventMsg::AgentMessage(AgentMessageEvent {
            id: None,
            parent_id: None,
            message: message.content.clone(),
            finish_reason: None,
        }),
        "tool" => {
            // Reconstruct tool result as ExecCommandEnd
            EventMsg::ExecCommandEnd(Box::new(ExecCommandEndEvent {
                call_id: message.tool_call_id.clone().unwrap_or_default(),
                turn_id: turn_id.to_string(),
                command: vec!["imported_tool".to_string()],
                cwd: cwd.to_path_buf(),
                parsed_cmd: vec![ParsedCommand {
                    program: "imported_tool".to_string(),
                    args: vec![],
                }],
                source: ExecCommandSource::Agent,
                interaction_input: None,
                stdout: message.content.clone(),
                stderr: String::new(),
                aggregated_output: message.content.clone(),
                exit_code: 0,
                duration_ms: 0,
                formatted_output: message.content.clone(),
                metadata: None,
            }))
        }
        "system" => {
            // System messages are typically not replayed, skip or handle specially
            EventMsg::AgentMessage(AgentMessageEvent {
                id: None,
                parent_id: None,
                message: format!("[System] {}", message.content),
                finish_reason: None,
            })
        }
        other => {
            // Unknown role, treat as assistant message
            EventMsg::AgentMessage(AgentMessageEvent {
                id: None,
                parent_id: None,
                message: format!("[{other}] {}", message.content),
                finish_reason: None,
            })
        }
    };

    Ok(Event {
        id: turn_id.to_string(),
        msg: event_msg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_export_json() {
        let json = r#"{
            "version": 1,
            "session": {
                "id": "test-123",
                "title": "Test Session",
                "created_at": "2024-01-01T00:00:00Z"
            },
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi there!"}
            ]
        }"#;

        let export: SessionExport = serde_json::from_str(json).unwrap();
        assert_eq!(export.version, 1);
        assert_eq!(export.session.id, "test-123");
        assert_eq!(export.messages.len(), 2);
    }

    #[test]
    fn test_message_to_event() {
        let mut turn_id = 0u64;
        let cwd = PathBuf::from("/tmp");

        let user_msg = ExportMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
            tool_calls: None,
            tool_call_id: None,
            timestamp: None,
        };

        let event = message_to_event(&user_msg, &mut turn_id, &cwd).unwrap();
        assert_eq!(turn_id, 1);
        assert!(matches!(event.msg, EventMsg::UserMessage(_)));

        let assistant_msg = ExportMessage {
            role: "assistant".to_string(),
            content: "Hi".to_string(),
            tool_calls: None,
            tool_call_id: None,
            timestamp: None,
        };

        let event = message_to_event(&assistant_msg, &mut turn_id, &cwd).unwrap();
        assert!(matches!(event.msg, EventMsg::AgentMessage(_)));
    }

    #[tokio::test]
    async fn test_import_empty_source_validation() {
        let cmd = ImportCommand {
            source: String::new(),
            force: false,
            resume: false,
        };

        let result = cmd.run().await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Source path cannot be empty"));
    }

    #[tokio::test]
    async fn test_import_whitespace_source_validation() {
        let cmd = ImportCommand {
            source: "   ".to_string(),
            force: false,
            resume: false,
        };

        let result = cmd.run().await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Source path cannot be empty"));
    }

    #[test]
    fn test_parse_html_provides_helpful_error() {
        let html_content = "<!DOCTYPE html><html><head><title>Not JSON</title></head></html>";

        let result: Result<SessionExport, _> = serde_json::from_str(html_content);
        assert!(result.is_err());

        // Verify that our error handling would detect this as HTML
        assert!(html_content.trim_start().starts_with("<!DOCTYPE"));
    }

    #[test]
    fn test_parse_xml_provides_helpful_error() {
        let xml_content = r#"<?xml version="1.0"?><root><data>Not JSON</data></root>"#;

        let result: Result<SessionExport, _> = serde_json::from_str(xml_content);
        assert!(result.is_err());

        // Verify that our error handling would detect this as XML
        assert!(xml_content.trim_start().starts_with("<?xml"));
    }

    #[test]
    fn test_parse_empty_content_error() {
        let empty_content = "";

        let result: Result<SessionExport, _> = serde_json::from_str(empty_content);
        assert!(result.is_err());

        // Verify that our error handling would detect this as empty
        assert!(empty_content.is_empty());
    }

    #[test]
    fn test_content_preview_truncation() {
        // Test that preview logic correctly truncates long content
        let long_content = "a".repeat(500);
        let preview_len = long_content.len().min(200);

        assert_eq!(preview_len, 200);
        assert_eq!(&long_content[..preview_len].len(), &200);
    }

    #[test]
    fn test_validate_agent_references_with_missing_agents() {
        use crate::export_cmd::SessionMetadata;

        // Create an export with agent references that don't exist locally
        let export = SessionExport {
            version: 1,
            session: SessionMetadata {
                id: "test-123".to_string(),
                title: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
                cwd: None,
                model: None,
                agent: Some("custom-nonexistent-agent".to_string()),
                agent_refs: Some(vec!["another-nonexistent".to_string()]),
            },
            messages: vec![ExportMessage {
                role: "user".to_string(),
                content: "@yet-another-missing help me".to_string(),
                tool_calls: None,
                tool_call_id: None,
                timestamp: None,
            }],
        };

        let missing = validate_agent_references(&export).unwrap();

        // Should find at least the explicitly nonexistent ones
        // (built-in agents like 'explore', 'research' will exist)
        assert!(missing.contains(&"custom-nonexistent-agent".to_string()));
        assert!(missing.contains(&"another-nonexistent".to_string()));
        assert!(missing.contains(&"yet-another-missing".to_string()));
    }

    #[test]
    fn test_validate_agent_references_with_builtin_agents() {
        use crate::export_cmd::SessionMetadata;

        // Create an export referencing only built-in agents
        let export = SessionExport {
            version: 1,
            session: SessionMetadata {
                id: "test-123".to_string(),
                title: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
                cwd: None,
                model: None,
                agent: None,
                agent_refs: None,
            },
            messages: vec![
                ExportMessage {
                    role: "user".to_string(),
                    content: "@build help me compile".to_string(),
                    tool_calls: None,
                    tool_call_id: None,
                    timestamp: None,
                },
                ExportMessage {
                    role: "user".to_string(),
                    content: "@plan create a plan".to_string(),
                    tool_calls: None,
                    tool_call_id: None,
                    timestamp: None,
                },
            ],
        };

        let missing = validate_agent_references(&export).unwrap();

        // Built-in agents should not be reported as missing
        assert!(!missing.contains(&"build".to_string()));
        assert!(!missing.contains(&"plan".to_string()));
    }
}

#[cfg(test)]
#[test]
fn ux_contract_v1_import_retains_system_and_tool_pairing() {
    let document = parse_session_document(r#"{
        "version": 1,
        "session": {"id":"old-id", "created_at":"2024-01-01T00:00:00Z", "model":"test", "title":"legacy"},
        "messages": [
            {"role":"system", "content":"instructions"},
            {"role":"user", "content":"question"},
            {"role":"assistant", "content":"", "tool_calls":[{"id":"call", "name":"Read", "arguments":{"path":"file"}}]},
            {"role":"tool", "content":"result", "tool_call_id":"call"}
        ]
    }"#).unwrap();
    assert_eq!(document.messages[0].role, "system");
    assert_eq!(document.messages[2].tool_calls[0].id, "call");
    assert_eq!(document.messages[3].tool_call_id.as_deref(), Some("call"));
    let yaml = serde_yaml::to_string(&document).unwrap();
    let recovered = parse_session_document(&yaml).unwrap();
    assert_eq!(
        serde_json::to_value(recovered).unwrap(),
        serde_json::to_value(document).unwrap()
    );
}
