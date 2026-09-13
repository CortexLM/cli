//! Live Code session attach (terminal follow + approvals).

use crate::error::{CortexError, Result};

/// Parse a live-attach target. URLs are rejected so a local session is never
/// started as a fallback.
pub fn parse_attach_target(raw: &str) -> Result<String> {
    let id = raw.trim();
    if id.is_empty() {
        return Err(CortexError::InvalidInput(
            "Session id is required. No local session was started.".into(),
        ));
    }
    if id.contains("://") {
        return Err(CortexError::InvalidInput(
            "Use `cortex attach <session-id>` for a live Code session. Connecting by URL is not supported. No local session was started.".into(),
        ));
    }
    if !id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(CortexError::InvalidInput(
            "Invalid Code session ID. No local session was started.".into(),
        ));
    }
    Ok(id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_session_ids() {
        assert_eq!(parse_attach_target("sess-1").unwrap(), "sess-1");
        assert_eq!(parse_attach_target("  abc_DEF-9  ").unwrap(), "abc_DEF-9");
    }

    #[test]
    fn url_does_not_start_a_local_session() {
        let err = parse_attach_target("http://127.0.0.1:1").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("No local session was started"), "{msg}");
        assert!(msg.contains("session-id"), "{msg}");
    }

    #[test]
    fn empty_fails_closed() {
        let err = parse_attach_target("  ").unwrap_err();
        assert!(err.to_string().contains("No local session was started"));
    }
}
