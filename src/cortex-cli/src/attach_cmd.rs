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

impl AttachCli {
    pub async fn run(self) -> Result<()> {
        let id = parse_attach_target(&self.session_id)?;
        let token = load_attach_token()?;
        let client = CodeAgentClient::new(None, Some(token));
        client.resume_session(&id).await?;
        let session = client.get_session(&id).await?;
        println!(
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
        );

        for message in client.list_messages(&id).await? {
            let role = if message.role.is_empty() {
                "message"
            } else {
                message.role.as_str()
            };
            if !message.text.is_empty() {
                println!("{role}: {}", message.text);
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
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("approve ") {
                client.approve_invocation(&id, rest.trim(), true).await?;
                println!("Approved {rest}");
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("deny ") {
                client.approve_invocation(&id, rest.trim(), false).await?;
                println!("Denied {rest}");
                continue;
            }
            let mut stream = client.stream_turn(trimmed, CodeTurnMode::Code).await?;
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
        }
        println!("Detached. Session {id} is still running.");
        Ok(())
    }
}

fn load_attach_token() -> Result<String> {
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

    #[test]
    fn attach_cli_requires_session_id() {
        use clap::Parser;
        assert!(AttachCli::try_parse_from(["attach"]).is_err());
        let parsed = AttachCli::try_parse_from(["attach", "sess-9"]).unwrap();
        assert_eq!(parsed.session_id, "sess-9");
    }
}
