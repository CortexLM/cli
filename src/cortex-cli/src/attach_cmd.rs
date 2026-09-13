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
    Invalid(String),
    Message(String),
}

pub(crate) fn parse_followup_line(trimmed: &str) -> FollowupLine {
    if trimmed.is_empty() {
        return FollowupLine::Detach;
    }
    if trimmed.eq_ignore_ascii_case("approve") || trimmed.eq_ignore_ascii_case("deny") {
        return FollowupLine::Invalid(
            "`approve <id>` / `deny <id>` need an invocation id. No turn was sent.".into(),
        );
    }
    if let Some(rest) = trimmed.strip_prefix("approve ") {
        let id = rest.trim();
        if id.is_empty() {
            return FollowupLine::Invalid(
                "`approve <id>` needs an invocation id. No turn was sent.".into(),
            );
        }
        return FollowupLine::Approve(id.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("deny ") {
        let id = rest.trim();
        if id.is_empty() {
            return FollowupLine::Invalid(
                "`deny <id>` needs an invocation id. No turn was sent.".into(),
            );
        }
        return FollowupLine::Deny(id.to_string());
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

        let mut seen = std::collections::HashSet::new();
        print_unseen_messages(client.list_messages(&id).await?, &mut seen);

        if self.print_only || !io::stdin().is_terminal() {
            println!("Detached. Session {id} is still running.");
            return Ok(());
        }

        println!(
            "Send a follow-up and press Enter. `approve <id>` / `deny <id>` for paused tools. Empty line detaches. Remote output is followed while this terminal is attached."
        );
        follow_attached_session(&client, &id, seen).await?;
        println!("Detached. Session {id} is still running.");
        Ok(())
    }
}

fn print_unseen_messages(
    messages: Vec<cortex_engine::client::CodeMessage>,
    seen: &mut std::collections::HashSet<String>,
) {
    for message in unseen_messages(messages, seen) {
        if let Some(line) = format_message_line(&message.role, &message.text) {
            println!("{line}");
        }
    }
}

pub(crate) fn unseen_messages(
    messages: Vec<cortex_engine::client::CodeMessage>,
    seen: &mut std::collections::HashSet<String>,
) -> Vec<cortex_engine::client::CodeMessage> {
    messages
        .into_iter()
        .filter(|message| {
            let key = if message.id.is_empty() {
                format!("{}:{}", message.created_at, message.text)
            } else {
                message.id.clone()
            };
            seen.insert(key)
        })
        .collect()
}

async fn follow_attached_session(
    client: &CodeAgentClient,
    id: &str,
    mut seen: std::collections::HashSet<String>,
) -> Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<std::io::Result<String>>(8);
    std::thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            if tx.blocking_send(line).is_err() {
                break;
            }
        }
    });
    loop {
        tokio::select! {
            line = rx.recv() => {
                let Some(line) = line else { break; };
                match parse_followup_line(line?.trim()) {
                    FollowupLine::Detach => break,
                    FollowupLine::Invalid(msg) => println!("{msg}"),
                    FollowupLine::Approve(invocation) => {
                        client.approve_invocation(id, &invocation, true).await?;
                        println!("Approved {invocation}");
                    }
                    FollowupLine::Deny(invocation) => {
                        client.approve_invocation(id, &invocation, false).await?;
                        println!("Denied {invocation}");
                    }
                    FollowupLine::Message(text) => {
                        stream_followup(client, &text).await?;
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                match client.list_messages(id).await {
                    Ok(messages) => print_unseen_messages(messages, &mut seen),
                    Err(e) => eprintln!("{}", e.user_friendly_message()),
                }
            }
        }
    }
    Ok(())
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
        assert!(matches!(
            parse_followup_line("approve"),
            FollowupLine::Invalid(_)
        ));
        assert!(matches!(
            parse_followup_line("deny"),
            FollowupLine::Invalid(_)
        ));
        assert!(matches!(
            parse_followup_line("approve "),
            FollowupLine::Invalid(_)
        ));
        let mut seen = std::collections::HashSet::new();
        let first = cortex_engine::client::CodeMessage {
            id: "m1".into(),
            role: "assistant".into(),
            text: "hello".into(),
            created_at: String::new(),
        };
        let again = first.clone();
        let later = cortex_engine::client::CodeMessage {
            id: "m2".into(),
            role: "assistant".into(),
            text: "later".into(),
            created_at: String::new(),
        };
        assert_eq!(unseen_messages(vec![first], &mut seen).len(), 1);
        assert!(unseen_messages(vec![again], &mut seen).is_empty());
        assert_eq!(unseen_messages(vec![later], &mut seen).len(), 1);
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
