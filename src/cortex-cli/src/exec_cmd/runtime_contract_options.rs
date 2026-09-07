use anyhow::{Result, bail};

use cortex_common::resolve_model_alias;
use cortex_engine::{Config, Session, SessionHandle};
use cortex_protocol::{AskForApproval, SandboxPolicy};

use super::ExecCli;
use super::autonomy::AutonomyLevel;

impl ExecCli {
    pub(crate) fn validate_runtime_options(&self) -> Result<()> {
        let unsupported = [
            (
                matches!(
                    self.autonomy,
                    Some(AutonomyLevel::Low | AutonomyLevel::Medium)
                ),
                "--auto low/medium (client risk thresholds)",
            ),
            (self.skip_permissions, "--skip-permissions-unsafe"),
            (self.output_schema.is_some(), "--output-schema"),
            (
                self.response_format.as_deref().is_some_and(|f| f != "text"),
                "--response-format",
            ),
            (self.max_tokens.is_some(), "--max-tokens"),
            (self.frequency_penalty.is_some(), "--frequency-penalty"),
            (self.presence_penalty.is_some(), "--presence-penalty"),
            (!self.stop_sequences.is_empty(), "--stop"),
            (self.logprobs.is_some(), "--logprobs"),
            (self.num_completions.is_some(), "--n"),
            (self.best_of.is_some(), "--best-of"),
            (self.user.is_some(), "--user"),
            (self.suffix.is_some(), "--suffix"),
            (self.spec_model.is_some(), "--spec-model"),
            (
                self.reasoning_effort
                    .as_deref()
                    .is_some_and(|e| e != "medium"),
                "--reasoning-effort",
            ),
            (!self.images.is_empty(), "--image"),
            (
                !self.list_tools && !self.enabled_tools.is_empty(),
                "--enabled-tools",
            ),
            (
                !self.list_tools && !self.disabled_tools.is_empty(),
                "--disabled-tools",
            ),
        ];
        if let Some((_, flag)) = unsupported.into_iter().find(|(set, _)| *set) {
            bail!("{flag} is not supported by the Code service contract. No turn was submitted.");
        }
        if self.max_turns == 0 {
            bail!("--max-turns must be greater than zero.");
        }
        Ok(())
    }

    pub(crate) async fn runtime_config(&self, autonomy: Option<AutonomyLevel>) -> Result<Config> {
        let mut config = cortex_engine::session::control::load_runtime_config(
            self.cwd.clone(),
            self.model
                .as_deref()
                .map(|m| resolve_model_alias(m).to_string()),
            self.system_prompt.clone(),
        )
        .await?;
        if let Some(level) = autonomy {
            config.approval_policy = level.to_approval_policy();
            config.sandbox_policy = level.to_sandbox_policy(&config.cwd);
        } else if self.skip_permissions {
            config.approval_policy = AskForApproval::Never;
            config.sandbox_policy = SandboxPolicy::DangerFullAccess;
        }
        if self.use_spec {
            config.sandbox_policy = SandboxPolicy::ReadOnly;
        }
        Ok(config)
    }

    pub(crate) fn open_runtime_session(&self, config: Config) -> Result<(Session, SessionHandle)> {
        match &self.session_id {
            Some(id) => {
                let id = id
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid session ID."))?;
                Ok(Session::resume(config, id)?)
            }
            None => Ok(Session::new(config)?),
        }
    }
}
