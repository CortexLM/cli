use super::{SessionMeta, SessionStorage, StoredMessage, StoredToolCall, validate_id};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use cortex_protocol::EventMsg;

use crate::rollout::reader::{RolloutEntry, RolloutItem};

impl SessionStorage {
    pub(super) fn legacy_document(&self, id: &str) -> Result<(SessionMeta, Vec<StoredMessage>)> {
        validate_id(id)?;
        let path = self.base_dir().join(format!("{id}.jsonl"));
        self.check_regular_file(&path)?;
        let content = std::fs::read_to_string(path)?;
        let entries = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| {
                serde_json::from_str::<RolloutEntry>(line)
                    .context("Session history is damaged; original file has not been changed")
            })
            .collect::<Result<Vec<_>>>()?;
        let source = crate::rollout::reader::get_session_meta(&entries)
            .context("Session metadata missing")?;
        if source.id != id {
            bail!("Session metadata ID does not match its filename");
        }
        let mut meta = SessionMeta::new("cortex", source.model.as_deref().unwrap_or("default"));
        meta.id = id.to_owned();
        meta.cwd = source.cwd.clone();
        meta.created_at = DateTime::parse_from_rfc3339(&source.timestamp)?.with_timezone(&Utc);
        meta.updated_at = meta.created_at;
        meta.forked_from = source.parent_id.clone();
        let mut messages = Vec::new();
        if let Some(instructions) = &source.instructions {
            let mut message = StoredMessage::system(instructions);
            message.id = format!("{id}-instructions");
            message.timestamp = meta.created_at;
            messages.push(message);
        }
        for entry in entries {
            let timestamp = DateTime::parse_from_rfc3339(&entry.timestamp)?.with_timezone(&Utc);
            meta.updated_at = meta.updated_at.max(timestamp);
            if let Some(mut message) = legacy_message(entry.item)? {
                message.timestamp = timestamp;
                // Stable message IDs make fork-at-turn repeatable across reads.
                message.id = format!("{id}-{}", messages.len());
                messages.push(message);
            }
        }
        meta.message_count = messages.len() as u32;
        meta.title = messages
            .iter()
            .find(|m| m.is_user())
            .map(|m| m.content.chars().take(60).collect());
        Ok((meta, messages))
    }

    /// Exact IDs win; ambiguous prefixes fail rather than choose a session.
    pub fn resolve_id(&self, query: &str) -> Result<String> {
        validate_id(query)?;
        if self.exists(query) {
            self.load_meta(query)?;
            return Ok(query.to_string());
        }
        let matches = self
            .list_sessions()?
            .into_iter()
            .filter(|s| s.id.starts_with(query))
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [one] => Ok(one.id.clone()),
            [] => bail!("Session not found: {query}"),
            _ => bail!("Ambiguous session ID prefix: {query}"),
        }
    }

    /// Prompt/content search treats its query as literal text, not a regex or path.
    pub fn search(&self, query: &str) -> Result<Vec<super::SessionSummary>> {
        let needle = query.to_lowercase();
        let mut matches = Vec::new();
        for summary in self.list_sessions()? {
            if summary.id.to_lowercase().contains(&needle)
                || summary.title.to_lowercase().contains(&needle)
                || self
                    .load_messages(&summary.id)?
                    .iter()
                    .any(|m| m.content.to_lowercase().contains(&needle))
            {
                matches.push(summary);
            }
        }
        Ok(matches)
    }
}

fn legacy_message(item: RolloutItem) -> Result<Option<StoredMessage>> {
    let message = match item {
        RolloutItem::EventMsg(EventMsg::UserMessage(m)) => Some(StoredMessage::user(m.message)),
        RolloutItem::EventMsg(EventMsg::AgentMessage(m)) => {
            Some(StoredMessage::assistant(m.message))
        }
        RolloutItem::EventMsg(EventMsg::ExecCommandEnd(m)) => {
            // Legacy event logs have no assistant tool-call envelope. Retain the
            // output as context, rather than fabricate a tool-response pairing.
            Some(StoredMessage::system(format!(
                "Recorded command output:\n{}",
                m.aggregated_output
            )))
        }
        RolloutItem::ResponseItem(value) => response_message(value)?,
        RolloutItem::Compacted(_) => bail!(
            "Legacy compacted history cannot be safely replayed; original session is unchanged"
        ),
        _ => None,
    };
    Ok(message)
}

fn response_message(value: serde_json::Value) -> Result<Option<StoredMessage>> {
    let Some(role) = value.get("role").and_then(|v| v.as_str()) else {
        bail!("Legacy response item cannot be safely replayed; original session is unchanged");
    };
    let mut message = StoredMessage::system("");
    message.role = role.to_string();
    message.content = value
        .get("content")
        .and_then(|v| v.as_str())
        .context("Legacy structured content cannot be replayed as text")?
        .to_string();
    message.tool_call_id = value
        .get("tool_call_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    if let Some(calls) = value.get("tool_calls").and_then(|v| v.as_array()) {
        for call in calls {
            let id = call["id"].as_str().context("Missing tool call ID")?;
            let name = call["function"]["name"]
                .as_str()
                .context("Missing tool name")?;
            let args = call["function"]["arguments"]
                .as_str()
                .context("Missing tool arguments")?;
            message
                .tool_calls
                .push(StoredToolCall::new(id, name, serde_json::from_str(args)?));
        }
    }
    Ok(Some(message))
}
