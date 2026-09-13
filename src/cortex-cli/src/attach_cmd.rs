//! Live attach to a remote Code session (follow, send, approve/deny).

use anyhow::{Result, bail};
use clap::Parser;
use std::io::{self, BufRead, IsTerminal, Write};

use cortex_engine::client::{AUTH_REQUIRED, CodeAgentClient, CodeTurnMode};
use cortex_engine::session_attach::parse_attach_target;
use cortex_login::load_auth_with_fallback;

/// Attach this terminal to an in-flight Code session.
#[derive(Debug, Parser)]
pub struct AttachCli {
    /// Code session id (not a URL).
    pub session_id: String,

    /// Print the transcript and pending prompts, then detach (session keeps running).
    #[arg(long)]
    pub print_only: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FollowupLine {
    Detach,
    Approve(String),
    Deny(String),
    Message(String),
}

pub(crate) fn parse_followup_line(trimmed: &str) -> FollowupLine {
    if trimmed.is_empty() {
        return FollowupLine::Detach;
    }
    if let Some(rest) = trimmed.strip_prefix("approve ") {
        return FollowupLine::Approve(rest.trim().to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("deny ") {
        return FollowupLine::Deny(rest.trim().to_string());
    }
    FollowupLine::Message(trimmed.to_string())
}

impl AttachCli {
    pub async fn run(self) -> Result<()> {
        let id = parse_attach_target(&self.session_id)?;
        let token = load_attach_token()?;
        let client = CodeAgentClient::new(None, Some(token));
        client.resume_session(&id).await?;
        let session = client.get_session(&id).await?;
        println!("{}", attached_banner(&session));

        for message in client.list_messages(&id).await? {
            if let Some(line) = format_message_line(&message.role, &message.text) {
                println!("{line}");
            }
        }

        if self.print_only || !io::stdin().is_terminal() {
            println!("Detached. Session {id} is still running.");
            return Ok(());
        }

        println!(
            "Send a follow-up and press Enter. `approve <id>` / `deny <id>` for paused tools. Empty line detaches."
        );
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line?;
            match parse_followup_line(line.trim()) {
                FollowupLine::Detach => break,
                FollowupLine::Approve(invocation) => {
                    client.approve_invocation(&id, &invocation, true).await?;
                    println!("Approved {invocation}");
                }
                FollowupLine::Deny(invocation) => {
                    client.approve_invocation(&id, &invocation, false).await?;
                    println!("Denied {invocation}");
                }
                FollowupLine::Message(text) => {
                    stream_followup(&client, &text).await?;
                }
            }
        }
        println!("Detached. Session {id} is still running.");
        Ok(())
    }
}

async fn stream_followup(client: &CodeAgentClient, text: &str) -> Result<()> {
    let mut stream = client.stream_turn(text, CodeTurnMode::Code).await?;
    use futures::StreamExt;
    while let Some(event) = stream.next().await {
        match event {
            Ok(cortex_engine::client::ResponseEvent::Delta(text)) => {
                print!("{text}");
                let _ = io::stdout().flush();
            }
            Ok(cortex_engine::client::ResponseEvent::Error(e)) => {
                bail!("{e}");
            }
            Ok(_) => {}
            Err(e) => bail!("{}", e.user_friendly_message()),
        }
    }
    println!();
    Ok(())
}

pub(crate) fn attached_banner(session: &cortex_engine::client::CodeSession) -> String {
    format!(
        "Attached to {} · {} · {} — detach leaves the session running",
        session.id,
        if session.title.is_empty() {
            "Code session"
        } else {
            session.title.as_str()
        },
        if session.state.is_empty() {
            "live"
        } else {
            session.state.as_str()
        }
    )
}

pub(crate) fn format_message_line(role: &str, text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    let role = if role.is_empty() { "message" } else { role };
    Some(format!("{role}: {text}"))
}

pub(crate) fn load_attach_token() -> Result<String> {
    let cortex_home = crate::utils::paths::get_cortex_home();
    let token = std::env::var("CORTEX_AUTH_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| {
            std::env::var("CORTEX_API_KEY")
                .ok()
                .filter(|t| !t.is_empty())
        })
        .or_else(|| {
            load_auth_with_fallback(&cortex_home)
                .ok()
                .flatten()
                .and_then(|auth| auth.get_token().map(str::to_string))
        });
    token.ok_or_else(|| anyhow::anyhow!("{AUTH_REQUIRED}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use serial_test::serial;

    #[test]
    fn attach_cli_requires_session_id() {
        assert!(AttachCli::try_parse_from(["attach"]).is_err());
        let parsed = AttachCli::try_parse_from(["attach", "sess-9"]).unwrap();
        assert_eq!(parsed.session_id, "sess-9");
        assert!(!parsed.print_only);
        let printed = AttachCli::try_parse_from(["attach", "sess-9", "--print-only"]).unwrap();
        assert!(printed.print_only);
    }

    #[test]
    fn followup_line_parses_approve_deny_detach() {
        assert_eq!(parse_followup_line(""), FollowupLine::Detach);
        assert_eq!(
            parse_followup_line("approve inv-1"),
            FollowupLine::Approve("inv-1".into())
        );
        assert_eq!(
            parse_followup_line("deny  inv-2"),
            FollowupLine::Deny("inv-2".into())
        );
        assert_eq!(
            parse_followup_line("keep going"),
            FollowupLine::Message("keep going".into())
        );
        let session = cortex_engine::client::CodeSession {
            id: "sess-9".into(),
            runtime: "cloud".into(),
            kind: String::new(),
            host_status: String::new(),
            state: String::new(),
            model_slug: String::new(),
            model_ref: String::new(),
            title: String::new(),
            created_at: String::new(),
            host_id: String::new(),
            stream: None,
        };
        assert!(attached_banner(&session).contains("Code session"));
        assert!(attached_banner(&session).contains("live"));
        assert_eq!(
            format_message_line("", "hi").as_deref(),
            Some("message: hi")
        );
        assert!(format_message_line("user", "").is_none());
    }

    #[tokio::test]
    async fn attach_run_rejects_url_without_starting_local() {
        let err = AttachCli {
            session_id: "https://example.com/s".into(),
            print_only: true,
        }
        .run()
        .await
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("No local session was started"), "{msg}");
    }

    #[tokio::test]
    #[serial]
    async fn attach_run_requires_auth() {
        let home = tempfile::tempdir().unwrap();
        unsafe {
            std::env::remove_var("CORTEX_AUTH_TOKEN");
            std::env::remove_var("CORTEX_API_KEY");
            std::env::set_var("CORTEX_HOME", home.path());
        }
        let err = AttachCli {
            session_id: "sess-ok".into(),
            print_only: true,
        }
        .run()
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Not signed in"), "{err}");
        assert!(load_attach_token().is_err());
        unsafe {
            std::env::remove_var("CORTEX_HOME");
        }
    }
}
