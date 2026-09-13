//! Background agents: list, logs, stop, attach.

use anyhow::{Result, bail};
use clap::Parser;

use crate::attach_cmd::{AttachCli, load_attach_token};
use cortex_engine::client::{CodeAgentClient, CodeSession};
use cortex_engine::session_attach::parse_attach_target;

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
    let token = load_attach_token()?;
    Ok(CodeAgentClient::new(None, Some(token)))
}

pub(crate) fn format_job_line(session: &CodeSession) -> String {
    format!(
        "{}\t{}\t{}\t{}",
        session.id,
        if session.state.is_empty() {
            "live"
        } else {
            session.state.as_str()
        },
        session.runtime,
        session.title
    )
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
        println!("{}", format_job_line(&session));
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
    use cortex_engine::client::AUTH_REQUIRED;
    use serial_test::serial;

    #[test]
    fn jobs_subcommands_parse() {
        assert!(JobsCli::try_parse_from(["jobs", "list"]).is_ok());
        assert!(JobsCli::try_parse_from(["jobs", "list", "--json"]).is_ok());
        assert!(JobsCli::try_parse_from(["jobs", "logs", "sess-1"]).is_ok());
        assert!(JobsCli::try_parse_from(["jobs", "stop", "sess-1"]).is_ok());
        assert!(JobsCli::try_parse_from(["jobs", "attach", "sess-1"]).is_ok());
        assert!(JobsCli::try_parse_from(["jobs", "ls"]).is_ok());
    }

    #[test]
    fn format_job_line_uses_live_when_state_empty() {
        let session = CodeSession {
            id: "sess-1".into(),
            runtime: "cloud".into(),
            kind: String::new(),
            host_status: String::new(),
            state: String::new(),
            model_slug: String::new(),
            model_ref: String::new(),
            title: "rate limiter".into(),
            created_at: String::new(),
            host_id: String::new(),
            stream: None,
        };
        let line = format_job_line(&session);
        assert_eq!(line, "sess-1\tlive\tcloud\trate limiter");
        let mut busy = session.clone();
        busy.state = "running".into();
        assert!(format_job_line(&busy).contains("running"));
    }

    #[tokio::test]
    #[serial]
    async fn jobs_list_requires_auth() {
        let home = tempfile::tempdir().unwrap();
        unsafe {
            std::env::remove_var("CORTEX_AUTH_TOKEN");
            std::env::remove_var("CORTEX_API_KEY");
            std::env::set_var("CORTEX_HOME", home.path());
        }
        let err = JobsCli::try_parse_from(["jobs", "list"])
            .unwrap()
            .run()
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Not signed in"), "{err}");
        assert!(err.to_string().contains(AUTH_REQUIRED));
        unsafe {
            std::env::remove_var("CORTEX_HOME");
        }
    }

    #[tokio::test]
    async fn jobs_logs_rejects_url() {
        let err = JobsCli::try_parse_from(["jobs", "logs", "https://example.com/x"])
            .unwrap()
            .run()
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("No local session was started"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn jobs_stop_rejects_url() {
        let err = JobsCli::try_parse_from(["jobs", "stop", "http://127.0.0.1/s"])
            .unwrap()
            .run()
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("No local session was started"),
            "{err}"
        );
    }
}
