//! Code runtime selection. Cloud is the shipped TUI + exec default.

use serde::{Deserialize, Serialize};

/// This PC and SSH need a bound host session. Cloud is never substituted.
pub const DISCONNECTED_RUNTIME: &str = "This PC and SSH Code execution require an already connected Code session. Connect a host and resume that session, or leave CORTEX_COMPUTER unset to use Cloud. No runtime was substituted.";

/// Where tools run for this CLI session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerKind {
    /// Local workspace passed to the CLI (This PC). Explicit via `CORTEX_COMPUTER`.
    ThisPc,
    /// Cloud runtime (Firecracker VM when the API provisions one). Default.
    #[default]
    Cloud,
    /// SSH remote, when `CORTEX_COMPUTER=ssh` or `CORTEX_SSH_HOST` is set.
    Ssh,
}

impl ComputerKind {
    /// Detect from the environment. Cloud is the default Code runtime so a
    /// fresh install can complete a turn. This PC and SSH are explicit.
    pub fn detect() -> Self {
        Self::from_env(
            nonempty_env("CORTEX_SSH_HOST").as_deref(),
            nonempty_env("CORTEX_SSH_TARGET").as_deref(),
            nonempty_env("CORTEX_COMPUTER").as_deref(),
        )
    }

    /// Resolve the Code runtime from explicit environment values.
    ///
    /// `CORTEX_SSH_HOST` / `CORTEX_SSH_TARGET` select SSH. `CORTEX_COMPUTER`
    /// selects This PC or SSH. Unset, `cloud`, and unknown values stay Cloud.
    pub fn from_env(
        ssh_host: Option<&str>,
        ssh_target: Option<&str>,
        computer: Option<&str>,
    ) -> Self {
        if nonempty(ssh_host) || nonempty(ssh_target) {
            return Self::Ssh;
        }
        let normalized = computer
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
            .replace(['-', ' '], "_");
        match normalized.as_str() {
            "this_pc" | "thispc" | "local" | "paired" | "connected" => Self::ThisPc,
            "ssh" => Self::Ssh,
            _ => Self::Cloud,
        }
    }

    /// Product label for the TUI.
    pub fn label(self) -> &'static str {
        match self {
            Self::ThisPc => "This PC",
            Self::Cloud => "Cloud",
            Self::Ssh => "SSH",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ThisPc => "this_pc",
            Self::Cloud => "cloud",
            Self::Ssh => "ssh",
        }
    }
}

fn nonempty(value: Option<&str>) -> bool {
    value.is_some_and(|s| !s.is_empty())
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{CodeAgentClient, CodeTurnContext};

    #[test]
    fn computer_kind_labels() {
        assert_eq!(ComputerKind::ThisPc.label(), "This PC");
        assert_eq!(ComputerKind::Cloud.label(), "Cloud");
        assert_eq!(ComputerKind::Ssh.label(), "SSH");
        assert_eq!(ComputerKind::ThisPc.as_str(), "this_pc");
        assert_eq!(ComputerKind::default(), ComputerKind::Cloud);
        assert_eq!(CodeTurnContext::default().computer, ComputerKind::Cloud);
    }

    #[test]
    fn computer_kind_from_env_defaults_to_cloud() {
        assert_eq!(
            ComputerKind::from_env(None, None, None),
            ComputerKind::Cloud
        );
        assert_eq!(
            ComputerKind::from_env(None, None, Some("")),
            ComputerKind::Cloud
        );
        assert_eq!(
            ComputerKind::from_env(None, None, Some("cloud")),
            ComputerKind::Cloud
        );
        assert_eq!(
            ComputerKind::from_env(None, None, Some("unknown")),
            ComputerKind::Cloud
        );
        assert_eq!(
            ComputerKind::from_env(None, None, Some("this_pc")),
            ComputerKind::ThisPc
        );
        assert_eq!(
            ComputerKind::from_env(None, None, Some("this-pc")),
            ComputerKind::ThisPc
        );
        assert_eq!(
            ComputerKind::from_env(None, None, Some("local")),
            ComputerKind::ThisPc
        );
        assert_eq!(
            ComputerKind::from_env(None, None, Some("ssh")),
            ComputerKind::Ssh
        );
        assert_eq!(
            ComputerKind::from_env(Some("box.example"), None, Some("cloud")),
            ComputerKind::Ssh
        );
    }

    #[test]
    #[serial_test::serial]
    fn detect_without_cortex_computer_is_cloud() {
        let mut fixture = crate::testing::TestFixture::new();
        fixture.unset_env("CORTEX_COMPUTER");
        fixture.unset_env("CORTEX_SSH_HOST");
        fixture.unset_env("CORTEX_SSH_TARGET");
        assert_eq!(ComputerKind::detect(), ComputerKind::Cloud);
        fixture.set_env("CORTEX_COMPUTER", "this_pc");
        assert_eq!(ComputerKind::detect(), ComputerKind::ThisPc);
    }

    #[tokio::test]
    async fn this_pc_without_session_refuses_with_product_copy() {
        let client = CodeAgentClient::new(
            Some("http://127.0.0.1:1".into()),
            Some("contract-fixture-not-a-credential".into()),
        );
        client.set_turn_context(CodeTurnContext {
            computer: ComputerKind::ThisPc,
            ..Default::default()
        });
        let err = client
            .ensure_session()
            .await
            .expect_err("This PC must refuse");
        let msg = err.user_friendly_message();
        assert!(
            msg.contains(DISCONNECTED_RUNTIME),
            "expected product copy, got {msg}"
        );
        assert!(msg.contains("This PC"), "{msg}");
        assert!(!msg.to_lowercase().contains("reqwest"), "{msg}");
        assert!(!msg.contains("127.0.0.1"), "{msg}");
    }

    #[tokio::test]
    async fn ssh_without_session_refuses_with_product_copy() {
        let client = CodeAgentClient::new(
            Some("http://127.0.0.1:1".into()),
            Some("contract-fixture-not-a-credential".into()),
        );
        client.set_turn_context(CodeTurnContext {
            computer: ComputerKind::Ssh,
            ..Default::default()
        });
        let err = client.ensure_session().await.expect_err("SSH must refuse");
        let msg = err.user_friendly_message();
        assert!(
            msg.contains(DISCONNECTED_RUNTIME),
            "expected product copy, got {msg}"
        );
        assert!(msg.contains("SSH"), "{msg}");
    }
}
