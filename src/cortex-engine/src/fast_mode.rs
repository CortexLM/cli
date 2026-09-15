//! Fast-mode availability and organization policy.
//!
//! Fast mode is the low-latency turn path. An organization can disable it for
//! every member. When it is disabled, `/fast` and the CLI never silently fall
//! back to Standard mid-turn: the request is refused with product copy and the
//! session stays on Standard.
//!
//! Policy resolution fails closed and lives in [`crate::org_policy`]: a policy
//! document that exists but cannot be read, or that carries an unknown value,
//! denies fast mode. Only an absent document means the host default applies.

use crate::error::{CortexError, Result};
use crate::org_policy;

pub use crate::org_policy::{FastModePolicy, POLICY_DIR_ENV, POLICY_FILE, policy_dir, policy_path};

/// Product copy shown when the organization has disabled fast mode.
pub const ORG_DISABLED_TOAST: &str =
    "Fast mode is disabled for your organization. Contact your admin.";
/// Second line of the same refusal: the session does not switch.
pub const ORG_DISABLED_STAYING: &str = "Staying on Standard.";
/// Status-line chip while fast mode is on for a remote session.
pub const FAST_CHIP: &str = "Fast";
/// Wide status line for a remote session with fast mode on.
pub const REMOTE_FAST_STATUS: &str = "Remote · Fast mode on";
/// Narrow (40-column) status line for a remote session with fast mode on.
pub const REMOTE_FAST_STATUS_NARROW: &str = "Remote · Fast";
/// Wide status line for a remote session on the default path.
pub const REMOTE_STATUS: &str = "Remote · Standard";
/// Narrow (40-column) status line for a remote session on the default path.
pub const REMOTE_STATUS_NARROW: &str = "Remote";

/// Resolve fast-mode policy from an explicit directory.
pub fn resolve_policy(dir: Option<&std::path::Path>) -> FastModePolicy {
    org_policy::resolve(dir).fast_mode
}

/// Resolve fast-mode policy from the environment.
pub fn current_policy() -> FastModePolicy {
    org_policy::current().fast_mode
}

/// Requested fast-mode state for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FastMode {
    /// Default turn path.
    #[default]
    Standard,
    /// Low-latency turn path, when policy allows it.
    Fast,
}

impl FastMode {
    /// True when fast mode is on.
    pub const fn is_on(self) -> bool {
        matches!(self, Self::Fast)
    }

    /// Parse an on/off token. Unknown tokens are an error, never a toggle.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "on" | "true" | "enable" | "enabled" => Ok(Self::Fast),
            "off" | "false" | "disable" | "disabled" => Ok(Self::Standard),
            other => Err(CortexError::InvalidInput(format!(
                "Invalid fast mode value: {other}. Use on|off"
            ))),
        }
    }
}

/// Outcome of a `/fast` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastModeOutcome {
    /// Fast mode is now on.
    Enabled,
    /// Fast mode is now off.
    Disabled,
    /// The organization disabled fast mode; the session stays on Standard.
    RefusedByOrgPolicy {
        /// First toast line.
        toast: &'static str,
        /// Second toast line, naming the state the session stays in.
        staying: &'static str,
    },
}

impl FastModeOutcome {
    /// True when the request changed the session state.
    pub const fn applied(&self) -> bool {
        matches!(self, Self::Enabled | Self::Disabled)
    }

    /// True when the request was refused because of organization policy.
    pub const fn refused(&self) -> bool {
        matches!(self, Self::RefusedByOrgPolicy { .. })
    }

    /// Toasts to show, in order.
    pub fn toasts(&self) -> Vec<&'static str> {
        match self {
            Self::Enabled => vec!["Fast mode on."],
            Self::Disabled => vec!["Fast mode off."],
            Self::RefusedByOrgPolicy { toast, staying } => vec![*toast, *staying],
        }
    }
}

/// Apply a fast-mode request against a resolved policy.
///
/// A refusal never mutates the session and never queues a silent re-send.
pub fn apply_fast_mode(requested: FastMode, policy: FastModePolicy) -> FastModeOutcome {
    if requested.is_on() && policy.is_disabled() {
        return FastModeOutcome::RefusedByOrgPolicy {
            toast: ORG_DISABLED_TOAST,
            staying: ORG_DISABLED_STAYING,
        };
    }
    if requested.is_on() {
        FastModeOutcome::Enabled
    } else {
        FastModeOutcome::Disabled
    }
}

/// Status line for a remote session.
///
/// `remote` is true for cloud and self-hosted Code sessions. Fast mode shows
/// its chip only while the session is actually on fast.
pub fn remote_status_line(remote: bool, fast: bool, narrow: bool) -> Option<&'static str> {
    if !remote {
        return None;
    }
    Some(match (fast, narrow) {
        (true, false) => REMOTE_FAST_STATUS,
        (true, true) => REMOTE_FAST_STATUS_NARROW,
        (false, false) => REMOTE_STATUS,
        (false, true) => REMOTE_STATUS_NARROW,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_policy(dir: &std::path::Path, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(policy_path(dir), body).unwrap();
    }

    #[test]
    fn absent_policy_document_uses_the_host_default() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_policy(Some(temp.path())),
            FastModePolicy::HostDefault
        );
        assert_eq!(resolve_policy(None), FastModePolicy::HostDefault);
        assert!(resolve_policy(None).allows());
    }

    #[test]
    fn a_disabled_organization_refuses_fast_mode() {
        let temp = tempfile::tempdir().unwrap();
        write_policy(temp.path(), r#"{"fast_mode": false}"#);
        let policy = resolve_policy(Some(temp.path()));
        assert_eq!(policy, FastModePolicy::Disabled);
        assert!(policy.is_disabled());
        assert!(!policy.allows());

        let outcome = apply_fast_mode(FastMode::Fast, policy);
        assert!(outcome.refused());
        assert!(!outcome.applied());
        assert_eq!(
            outcome.toasts(),
            vec![ORG_DISABLED_TOAST, ORG_DISABLED_STAYING]
        );
        assert!(
            outcome.toasts()[0].contains("disabled for your organization"),
            "{:?}",
            outcome.toasts()
        );
        assert!(
            outcome.toasts()[1].contains("Staying on Standard"),
            "{:?}",
            outcome.toasts()
        );
    }

    #[test]
    fn a_refusal_is_independent_of_the_current_state() {
        let temp = tempfile::tempdir().unwrap();
        write_policy(temp.path(), r#"{"fast_mode": false}"#);
        let policy = resolve_policy(Some(temp.path()));
        // Already on fast: a refused request still reports refusal, so the
        // caller cannot treat it as a state change.
        let outcome = apply_fast_mode(FastMode::Fast, policy);
        assert!(outcome.refused());
        assert!(!outcome.applied());
        // Turning fast off is always allowed.
        assert_eq!(
            apply_fast_mode(FastMode::Standard, policy),
            FastModeOutcome::Disabled
        );
    }

    #[test]
    fn allowed_organization_applies_the_request() {
        let temp = tempfile::tempdir().unwrap();
        write_policy(temp.path(), r#"{"fast_mode": true}"#);
        let policy = resolve_policy(Some(temp.path()));
        assert_eq!(policy, FastModePolicy::Allowed);
        let outcome = apply_fast_mode(FastMode::Fast, policy);
        assert_eq!(outcome, FastModeOutcome::Enabled);
        assert!(outcome.applied());
        assert_eq!(outcome.toasts(), vec!["Fast mode on."]);
        let off = apply_fast_mode(FastMode::Standard, policy);
        assert_eq!(off, FastModeOutcome::Disabled);
        assert!(off.applied());
    }

    #[test]
    fn text_values_are_resolved_and_unknown_values_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        write_policy(temp.path(), r#"{"fast_mode": "on"}"#);
        assert_eq!(resolve_policy(Some(temp.path())), FastModePolicy::Allowed);
        write_policy(temp.path(), r#"{"fast_mode": "ENABLED"}"#);
        assert_eq!(resolve_policy(Some(temp.path())), FastModePolicy::Allowed);
        write_policy(temp.path(), r#"{"fast_mode": "off"}"#);
        assert_eq!(resolve_policy(Some(temp.path())), FastModePolicy::Disabled);
        // An unknown value is not permission.
        write_policy(temp.path(), r#"{"fast_mode": "maybe"}"#);
        assert_eq!(resolve_policy(Some(temp.path())), FastModePolicy::Disabled);
    }

    #[test]
    fn an_unparseable_or_unreadable_policy_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        write_policy(temp.path(), "{ not json");
        assert_eq!(resolve_policy(Some(temp.path())), FastModePolicy::Disabled);

        let other = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(policy_path(other.path())).unwrap();
        assert_eq!(resolve_policy(Some(other.path())), FastModePolicy::Disabled);
    }

    #[test]
    fn a_policy_document_without_the_key_keeps_the_host_default() {
        let temp = tempfile::tempdir().unwrap();
        write_policy(temp.path(), r#"{"other_setting": true}"#);
        assert_eq!(
            resolve_policy(Some(temp.path())),
            FastModePolicy::HostDefault
        );
    }

    #[test]
    fn fast_mode_tokens_reject_unknown_values() {
        assert_eq!(FastMode::parse("on").unwrap(), FastMode::Fast);
        assert_eq!(FastMode::parse(" OFF ").unwrap(), FastMode::Standard);
        assert!(FastMode::parse("sometimes").is_err());
        assert!(!FastMode::default().is_on());
        assert!(FastMode::Fast.is_on());
    }

    #[test]
    fn remote_status_line_names_fast_only_when_it_is_on() {
        assert_eq!(
            remote_status_line(true, true, false),
            Some("Remote · Fast mode on")
        );
        assert_eq!(remote_status_line(true, true, true), Some("Remote · Fast"));
        assert_eq!(
            remote_status_line(true, false, false),
            Some("Remote · Standard")
        );
        assert_eq!(remote_status_line(true, false, true), Some("Remote"));
        assert_eq!(remote_status_line(false, true, false), None);
        assert_eq!(remote_status_line(false, false, false), None);
        assert_eq!(FAST_CHIP, "Fast");
    }

    #[test]
    fn policy_names_are_stable() {
        assert_eq!(FastModePolicy::HostDefault.as_str(), "host_default");
        assert_eq!(FastModePolicy::Allowed.as_str(), "allowed");
        assert_eq!(FastModePolicy::Disabled.as_str(), "disabled");
        assert_eq!(POLICY_FILE, "policy.json");
        assert_eq!(POLICY_DIR_ENV, "CORTEX_ORG_POLICY_DIR");
    }
}
