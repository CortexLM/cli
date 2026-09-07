//! Read-only agent adapter. Publication is separate, never an agent tool.

use anyhow::{Result, bail};
use cortex_protocol::{AskForApproval, EventMsg, Op, SandboxPolicy, Submission, UserInput};
use std::path::Path;
use std::time::Duration;

use super::GitHubClient;

pub const MAX_CONTEXT_BYTES: usize = 256 * 1024;
pub const MAX_REPLY_BYTES: usize = 60_000;

/// Narrow adapter seam: tests provide a controlled agent and publisher, never
/// credentials or a live GitHub account.
pub trait AutomationAdapter {
    fn run_agent(&self, prompt: String) -> impl Future<Output = Result<String>> + Send;
    fn publish(&self, number: u64, text: &str) -> impl Future<Output = Result<()>> + Send;
}

pub struct ReadOnlyAutomation<'a> {
    pub client: &'a GitHubClient,
    pub cwd: &'a Path,
}

impl AutomationAdapter for ReadOnlyAutomation<'_> {
    async fn run_agent(&self, prompt: String) -> Result<String> {
        run_read_only_agent(self.cwd, prompt).await
    }

    async fn publish(&self, number: u64, text: &str) -> Result<()> {
        self.client.create_comment(number, text).await?;
        Ok(())
    }
}

/// Returns a local result by default. A write requires an explicit caller decision.
pub async fn execute_automation(
    adapter: &impl AutomationAdapter,
    number: u64,
    prompt: String,
    approve_publication: bool,
) -> Result<String> {
    if number == 0 || prompt.trim().is_empty() || prompt.len() > MAX_CONTEXT_BYTES {
        bail!("GitHub automation context is invalid or too large");
    }
    let result = adapter.run_agent(prompt).await?;
    if result.trim().is_empty() || result.len() > MAX_REPLY_BYTES {
        bail!("The agent did not produce a publishable response");
    }
    if approve_publication {
        adapter.publish(number, &result).await?;
    }
    Ok(result)
}

async fn run_read_only_agent(cwd: &Path, prompt: String) -> Result<String> {
    let config = crate::Config {
        cwd: cwd.to_path_buf(),
        approval_policy: AskForApproval::Never,
        sandbox_policy: SandboxPolicy::ReadOnly,
        ..Default::default()
    };
    let (mut session, handle) = crate::Session::new(config)?;
    let runner = tokio::spawn(async move { session.run().await });
    let result = tokio::time::timeout(Duration::from_secs(300), async {
        handle
            .submission_tx
            .send(Submission {
                id: uuid::Uuid::new_v4().to_string(),
                op: Op::UserInput {
                    items: vec![UserInput::Text { text: prompt }],
                },
            })
            .await?;
        loop {
            let event =
                handle.event_rx.recv().await.map_err(|_| {
                    anyhow::anyhow!("The coding service is temporarily unavailable")
                })?;
            match event.msg {
                EventMsg::TaskComplete(result) => {
                    return result
                        .last_agent_message
                        .filter(|message| !message.trim().is_empty())
                        .ok_or_else(|| anyhow::anyhow!("The agent did not produce a response"));
                }
                EventMsg::Error(_) => bail!("The coding service is temporarily unavailable"),
                EventMsg::TurnAborted(_) => {
                    bail!("GitHub analysis was interrupted; remote cancellation is unconfirmed")
                }
                EventMsg::ExecApprovalRequest(_) | EventMsg::ApplyPatchApprovalRequest(_) => {
                    bail!("GitHub automation cannot approve agent writes");
                }
                _ => {}
            }
        }
    })
    .await;
    // Interrupt is attempted before stopping the owner, including timeout and
    // channel failure. No completion is manufactured after an error.
    let _ = handle.submission_tx.try_send(Submission {
        id: uuid::Uuid::new_v4().to_string(),
        op: Op::Interrupt,
    });
    runner.abort();
    let _ = runner.await;
    result.map_err(|_| {
        anyhow::anyhow!("GitHub agent timed out; remote cancellation is unconfirmed")
    })?
}

#[cfg(test)]
mod integration_contract_tests {
    use super::*;
    use std::sync::Mutex;

    struct Controlled {
        failure: bool,
        agent_calls: Mutex<Vec<String>>,
        posts: Mutex<Vec<String>>,
    }
    impl AutomationAdapter for Controlled {
        async fn run_agent(&self, prompt: String) -> Result<String> {
            self.agent_calls.lock().unwrap().push(prompt);
            if self.failure {
                bail!("controlled agent failure");
            }
            Ok("Controlled adapter response, not a model result".into())
        }
        async fn publish(&self, _: u64, text: &str) -> Result<()> {
            self.posts.lock().unwrap().push(text.into());
            Ok(())
        }
    }
    fn adapter(failure: bool) -> Controlled {
        Controlled {
            failure,
            agent_calls: Mutex::default(),
            posts: Mutex::default(),
        }
    }
    #[tokio::test]
    async fn generation_is_required_and_publication_is_explicit() {
        let controlled = adapter(false);
        let text = execute_automation(&controlled, 1, "fixture prompt".into(), false)
            .await
            .unwrap();
        assert_eq!(controlled.agent_calls.lock().unwrap().len(), 1);
        assert!(controlled.posts.lock().unwrap().is_empty());
        assert!(text.starts_with("Controlled adapter"));
        execute_automation(&controlled, 1, "fixture prompt".into(), true)
            .await
            .unwrap();
        assert_eq!(controlled.posts.lock().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn agent_failure_never_posts_a_success() {
        let controlled = adapter(true);
        assert!(
            execute_automation(&controlled, 1, "fixture".into(), true)
                .await
                .is_err()
        );
        assert!(controlled.posts.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn context_limits_fail_before_agent_or_publisher() {
        let controlled = adapter(false);
        assert!(
            execute_automation(&controlled, 1, "x".repeat(MAX_CONTEXT_BYTES + 1), true)
                .await
                .is_err()
        );
        assert!(controlled.agent_calls.lock().unwrap().is_empty());
        assert!(controlled.posts.lock().unwrap().is_empty());
    }
}
