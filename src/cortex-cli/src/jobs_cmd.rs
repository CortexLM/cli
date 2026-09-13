//! Background agents: list, logs, stop, attach.

use anyhow::{Result, bail};
use clap::Parser;

use crate::attach_cmd::AttachCli;
use cortex_engine::client::{AUTH_REQUIRED, CodeAgentClient};
use cortex_engine::session_attach::parse_attach_target;
use cortex_login::load_auth_with_fallback;

/// Manage background / remote Code agents.
#[derive(Debug, Parser)]
pub struct JobsCli {
    #[command(subcommand)]
    pub subcommand: JobsSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum JobsSubcommand {
    /// List live Code sessions (cloud + attached).
    #[command(visible_alias = "ls")]
    List(JobsListArgs),
    /// Print the transcript for a session.
    Logs(JobsIdArgs),
    /// Request stop; the session is not deleted.
    Stop(JobsIdArgs),
    /// Attach this terminal (same as `cortex attach`).
    Attach(AttachCli),
}

#[derive(Debug, Parser)]
pub struct JobsListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Parser)]
pub struct JobsIdArgs {
    pub session_id: String,
}

impl JobsCli {
    pub async fn run(self) -> Result<()> {
        match self.subcommand {
            JobsSubcommand::List(args) => list_jobs(args).await,
            JobsSubcommand::Logs(args) => logs_job(args).await,
            JobsSubcommand::Stop(args) => stop_job(args).await,
            JobsSubcommand::Attach(cli) => cli.run().await,
        }
    }
}

async fn client() -> Result<CodeAgentClient> {
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
    let Some(token) = token else {
        bail!("{AUTH_REQUIRED}");
    };
    Ok(CodeAgentClient::new(None, Some(token)))
}

async fn list_jobs(args: JobsListArgs) -> Result<()> {
    let client = client().await?;
    let sessions = client.list_sessions().await?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&sessions)?);
        return Ok(());
    }
    if sessions.is_empty() {
        println!("No background Code sessions.");
        return Ok(());
    }
    for session in sessions {
        println!(
            "{}\t{}\t{}\t{}",
            session.id,
            if session.state.is_empty() {
                "live"
            } else {
                session.state.as_str()
            },
            session.runtime,
            session.title
        );
    }
    Ok(())
}

async fn logs_job(args: JobsIdArgs) -> Result<()> {
    let id = parse_attach_target(&args.session_id)?;
    let client = client().await?;
    for message in client.list_messages(&id).await? {
        if !message.text.is_empty() {
            println!("{}: {}", message.role, message.text);
        }
    }
    Ok(())
}

async fn stop_job(args: JobsIdArgs) -> Result<()> {
    let id = parse_attach_target(&args.session_id)?;
    let client = client().await?;
    client.resume_session(&id).await?;
    client.cancel_in_flight_checked().await?;
    println!("Stop requested for {id}. Detach left the session; it is not deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn jobs_subcommands_parse() {
        assert!(JobsCli::try_parse_from(["jobs", "list"]).is_ok());
        assert!(JobsCli::try_parse_from(["jobs", "logs", "sess-1"]).is_ok());
        assert!(JobsCli::try_parse_from(["jobs", "stop", "sess-1"]).is_ok());
        assert!(JobsCli::try_parse_from(["jobs", "attach", "sess-1"]).is_ok());
    }
}
