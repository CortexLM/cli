use anyhow::{Result, bail};

use cortex_common::resolve_model_alias;
use cortex_engine::{Config, Session, SessionHandle};
use cortex_protocol::{AskForApproval, SandboxPolicy};

use super::ExecCli;
use super::autonomy::AutonomyLevel;

/// What a review-only run should read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewRequest<'a> {
    /// The uncommitted working-tree diff.
    WorkingTree,
    /// The current branch against a base branch.
    Branch(&'a str),
    /// A pull request by number.
    PullRequest(u64),
}

impl ReviewRequest<'_> {
    /// The instruction sent to the model for this review.
    pub fn prompt(&self) -> String {
        let scope = match self {
            ReviewRequest::WorkingTree => "the uncommitted working-tree diff".to_string(),
            ReviewRequest::Branch(base) => format!("the current branch against `{base}`"),
            ReviewRequest::PullRequest(number) => format!("pull request #{number}"),
        };
        format!(
            "Review {scope}. Report findings only: correctness, regressions, and missing tests. \
Do not edit files, run commands that change the repository, or apply fixes. \
If the change is sound, say so plainly."
        )
    }
}

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
        if self.json_schema && !matches!(self.output_format, super::ExecOutputFormat::Json) {
            bail!("--json-schema validates the `-o json` result document. Add -o json.");
        }
        if self.review_only() {
            // A review must never be able to write. Anything that widens
            // authority is refused rather than silently downgraded.
            if let Some(widening) = [
                (self.skip_permissions, "--skip-permissions-unsafe"),
                (
                    matches!(self.autonomy, Some(AutonomyLevel::High)),
                    "--auto high",
                ),
            ]
            .into_iter()
            .find(|(set, _)| *set)
            .map(|(_, flag)| flag)
            {
                bail!(
                    "--review-only never writes, so {widening} cannot be combined with it. No turn was submitted."
                );
            }
            // `--prompt` accepts hyphen values, so an unknown flag would be
            // swallowed into the prompt instead of failing. A review whose scope
            // silently changed is worse than a refused one.
            if let Some(flag) = self.swallowed_flag_in_prompt() {
                bail!(
                    "`{flag}` is not a flag this command accepts, and a review never guesses its scope. \
Use --review-base <BRANCH> or --review-pr <NUMBER>. No turn was submitted."
                );
            }
        }
        Ok(())
    }

    /// The first prompt token that looks like a mistyped flag, if any.
    ///
    /// Only reported for review runs: elsewhere a leading dash is a legitimate
    /// prompt (`cortex exec -- "--help explain this"`).
    fn swallowed_flag_in_prompt(&self) -> Option<&str> {
        self.prompt
            .iter()
            .find(|token| token.starts_with("--") && token.len() > 2)
            .map(String::as_str)
    }

    /// True when this run is pinned to review-only.
    pub(crate) fn review_only(&self) -> bool {
        self.review_only || self.review_pr.is_some() || self.review_base.is_some()
    }

    /// The review request this run should perform, if any.
    pub(crate) fn review_request(&self) -> Option<ReviewRequest<'_>> {
        if let Some(number) = self.review_pr {
            return Some(ReviewRequest::PullRequest(number));
        }
        if let Some(base) = self.review_base.as_deref() {
            return Some(ReviewRequest::Branch(base));
        }
        self.review_only.then_some(ReviewRequest::WorkingTree)
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
        if self.review_only() {
            // Review-only pins both halves: the sandbox cannot write, and the
            // approval policy never auto-approves a write it is asked about.
            config.approval_policy = AskForApproval::UnlessTrusted;
            config.sandbox_policy = SandboxPolicy::ReadOnly;
            return Ok(config);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn cli(args: &[&str]) -> ExecCli {
        let mut argv = vec!["exec"];
        argv.extend_from_slice(args);
        ExecCli::try_parse_from(argv).expect("exec arguments parse")
    }

    #[test]
    fn review_only_is_implied_by_every_review_flag() {
        assert!(!cli(&["hello"]).review_only());
        assert!(cli(&["--review-only"]).review_only());
        assert!(cli(&["--review-pr", "128"]).review_only());
        assert!(cli(&["--review-base", "main"]).review_only());
    }

    #[test]
    fn review_only_refuses_flags_that_widen_authority() {
        // `--skip-permissions-unsafe` is refused by the service contract for
        // every run; `--auto high` is the widening flag review-only must catch.
        for args in [
            vec!["--review-only", "--auto", "high"],
            vec!["--review-pr", "128", "--auto", "high"],
            vec!["--review-base", "main", "--auto", "high"],
        ] {
            let error = cli(&args)
                .validate_runtime_options()
                .expect_err("must refuse");
            let message = error.to_string();
            assert!(message.contains("--review-only never writes"), "{message}");
            assert!(message.contains("No turn was submitted"), "{message}");
        }
        // The blanket contract check still refuses the unsafe flag outright.
        let error = cli(&["--review-only", "--skip-permissions-unsafe"])
            .validate_runtime_options()
            .expect_err("must refuse");
        assert!(
            error.to_string().contains("--skip-permissions-unsafe"),
            "{error}"
        );
    }

    #[test]
    fn review_only_accepts_a_read_only_auto_level() {
        cli(&["--review-only", "--auto", "read-only"])
            .validate_runtime_options()
            .expect("read-only review is the intended combination");
        cli(&["--review-only"])
            .validate_runtime_options()
            .expect("bare review-only");
    }

    #[test]
    fn review_requests_describe_their_scope_and_forbid_writes() {
        assert_eq!(
            cli(&["--review-only"]).review_request(),
            Some(ReviewRequest::WorkingTree)
        );
        assert_eq!(
            cli(&["--review-base", "main"]).review_request(),
            Some(ReviewRequest::Branch("main"))
        );
        assert_eq!(
            cli(&["--review-pr", "128"]).review_request(),
            Some(ReviewRequest::PullRequest(128))
        );
        assert_eq!(cli(&["hello"]).review_request(), None);

        for request in [
            ReviewRequest::WorkingTree,
            ReviewRequest::Branch("main"),
            ReviewRequest::PullRequest(128),
        ] {
            let prompt = request.prompt();
            assert!(prompt.contains("Review"), "{prompt}");
            assert!(prompt.contains("Do not edit files"), "{prompt}");
            assert!(prompt.contains("missing tests"), "{prompt}");
        }
        assert!(
            ReviewRequest::PullRequest(128)
                .prompt()
                .contains("pull request #128")
        );
    }

    #[test]
    fn a_mistyped_flag_in_a_review_is_refused_not_swallowed() {
        // `--base` is not a flag this command accepts. Without the guard it
        // becomes prompt text and the review silently runs against the working
        // tree instead of the intended base branch.
        let error = cli(&["--review-only", "--base", "main", "review this"])
            .validate_runtime_options()
            .expect_err("mistyped flag");
        let message = error.to_string();
        assert!(message.contains("`--base`"), "{message}");
        assert!(message.contains("--review-base"), "{message}");
        assert!(message.contains("No turn was submitted"), "{message}");

        // The correct flag is accepted and selects the branch scope.
        cli(&["--review-only", "--review-base", "main", "review this"])
            .validate_runtime_options()
            .expect("declared flag");
        assert_eq!(
            cli(&["--review-only", "--review-base", "main"]).review_request(),
            Some(ReviewRequest::Branch("main"))
        );
    }

    #[test]
    fn a_leading_dash_prompt_stays_legal_outside_review_runs() {
        // A prompt may legitimately start with a dash; only review runs refuse
        // it, because only there does the scope silently change.
        cli(&["--", "--help explain this"])
            .validate_runtime_options()
            .expect("dash prompt outside review");
        cli(&["explain --verbose output"])
            .validate_runtime_options()
            .expect("dash inside a prompt");
    }

    #[test]
    fn json_schema_requires_the_json_result_format() {
        let error = cli(&["--json-schema", "hello"])
            .validate_runtime_options()
            .expect_err("text output");
        assert!(error.to_string().contains("Add -o json"), "{error}");

        cli(&["--json-schema", "-o", "json", "hello"])
            .validate_runtime_options()
            .expect("json output");
    }

    #[test]
    fn stream_jsonl_requires_a_machine_readable_output_format() {
        // The pairing is enforced when the stream starts, not by the option
        // contract, so assert it where it lives.
        let text_output = cli(&["--input-format", "stream-jsonl"]);
        assert_eq!(
            text_output.output_format,
            super::super::ExecOutputFormat::Text
        );
        cli(&["--input-format", "stream-jsonl", "-o", "stream-json"])
            .validate_runtime_options()
            .expect("stream output");
        assert_eq!(
            cli(&["--input-format", "stream-jsonl", "-o", "stream-json"]).input_format,
            super::super::ExecInputFormat::StreamJsonl
        );
    }
}
