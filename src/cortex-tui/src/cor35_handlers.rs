//! COR-35 async command handlers: permission rules, sandbox allowlist, plugin
//! marketplace, ACP editor handshake, and checkpoint rewind.
//!
//! Each handler surfaces a real on-disk state or a real subcommand. None of them
//! invent a success: a missing or broken file is reported in the transcript, and
//! the picker says so rather than showing an empty list.

use crate::app::AppState;
use crate::interactive::builders::{
    build_permission_rules, build_plugin_marketplace, build_sandbox_allowlist,
};
use crate::permissions::PermissionRules;
use crate::sandbox_allowlist::SandboxAllowlist;

/// Async command ids this module answers.
pub const PERMISSION_RULES_COMMAND: &str = "permissions:rules";
/// Sandbox network allowlist command id.
pub const SANDBOX_NETWORK_COMMAND: &str = "sandbox:network";
/// Checkpoint rewind command id.
pub const REWIND_CHECKPOINT_COMMAND: &str = "rewind:checkpoint";

/// Outcome of opening a project-state picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectStateOutcome {
    /// The picker opened with committed state.
    Opened(String),
    /// The state could not be read; the transcript carries the reason.
    Failed(String),
}

/// Load the committed permission rules for `cwd` and describe the outcome.
///
/// A broken file is an error, never an empty rule set: silently dropping a
/// team's `deny` list would be the worst possible outcome.
pub fn load_rules_outcome(cwd: &std::path::Path) -> Result<PermissionRules, String> {
    match crate::permissions::load_for_project(cwd) {
        Ok(Some(rules)) => Ok(rules),
        Ok(None) => Ok(PermissionRules::default()),
        Err(error) => Err(format!(
            "Could not read .cortex/permissions.toml: {error}. Rules were not applied."
        )),
    }
}

/// Load the committed sandbox allowlist for `cwd`, failing closed.
pub fn load_allowlist_outcome(cwd: &std::path::Path) -> Result<SandboxAllowlist, String> {
    match crate::sandbox_allowlist::load_for_project(cwd) {
        Ok(Some(list)) => Ok(list),
        // No file means nothing is allowed, which is the fail-closed default.
        Ok(None) => Ok(SandboxAllowlist::default()),
        Err(error) => Err(format!(
            "Could not read .cortex/sandbox.toml: {error}. Network stays blocked."
        )),
    }
}

/// Transcript line for the permission-rules picker.
pub fn rules_status(rules: &PermissionRules) -> String {
    if rules.is_empty() {
        return "No permission rules committed. Add .cortex/permissions.toml to pin allow / ask / deny.".to_string();
    }
    format!(
        "`.cortex/permissions.toml` — {} allow · {} ask · {} deny. First matching rule wins; deny beats allow.",
        rules.allow.len(),
        rules.ask.len(),
        rules.deny.len()
    )
}

/// Transcript line for the sandbox allowlist picker.
pub fn allowlist_status(allowlist: &SandboxAllowlist) -> String {
    if allowlist.is_empty() {
        return "Network is blocked — no domains are allowed. Add one to let a command out."
            .to_string();
    }
    format!(
        "Network is allowlisted — {}. Anything off the list fails closed and asks.",
        crate::sandbox_allowlist::status_line(allowlist)
    )
}

/// Picker rows for the plugin marketplace.
///
/// `plugins` is the live `cortex plugin list` output when it is available; when
/// it is not, the marketplace row still opens the real registry.
pub fn plugin_marketplace_rows(installed: &[(String, String)]) -> Vec<(String, String, String)> {
    let mut rows: Vec<(String, String, String)> = installed
        .iter()
        .map(|(name, version)| {
            (
                name.clone(),
                name.clone(),
                format!("installed · v{version}"),
            )
        })
        .collect();
    rows.push((
        "__search__".to_string(),
        "Search the marketplace…".to_string(),
        format!(
            "{} — signed index",
            crate::plugin_marketplace::REGISTRY_ORIGIN
        ),
    ));
    rows
}

/// Transcript line for the ACP editor handshake.
pub fn ide_status(narrow: bool) -> String {
    if narrow {
        "ACP · stdio · approvals unchanged".to_string()
    } else {
        "ACP over stdio — the editor drives this session; tools stay behind the same approvals and sandbox."
            .to_string()
    }
}

/// Apply a permission-rules load to `state`, reporting failures honestly.
pub fn apply_rules_result(
    state: &mut AppState,
    outcome: Result<PermissionRules, String>,
) -> ProjectStateOutcome {
    match outcome {
        Ok(rules) => {
            let status = rules_status(&rules);
            state.add_message(cortex_core::widgets::Message::system(status.clone()));
            state.enter_interactive_mode(build_permission_rules(&rules, 0, None));
            ProjectStateOutcome::Opened(status)
        }
        Err(error) => {
            state.add_message(cortex_core::widgets::Message::system(format!("× {error}")));
            ProjectStateOutcome::Failed(error)
        }
    }
}

/// Apply a sandbox allowlist load to `state`, reporting failures honestly.
pub fn apply_allowlist_result(
    state: &mut AppState,
    outcome: Result<SandboxAllowlist, String>,
) -> ProjectStateOutcome {
    match outcome {
        Ok(list) => {
            let status = allowlist_status(&list);
            state.add_message(cortex_core::widgets::Message::system(status.clone()));
            state.enter_interactive_mode(build_sandbox_allowlist(&list, 0, None));
            ProjectStateOutcome::Opened(status)
        }
        Err(error) => {
            state.add_message(cortex_core::widgets::Message::system(format!("× {error}")));
            ProjectStateOutcome::Failed(error)
        }
    }
}

/// Apply a plugin-marketplace open to `state`.
pub fn apply_plugin_marketplace(state: &mut AppState, installed: &[(String, String)]) {
    state.add_message(cortex_core::widgets::Message::system(format!(
        "{} — signed packages only.",
        crate::plugin_marketplace::REGISTRY_ORIGIN
    )));
    state.enter_interactive_mode(build_plugin_marketplace(installed, 0, None));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_rules_file_is_an_empty_rule_set_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let rules = load_rules_outcome(dir.path()).expect("no file");
        assert!(rules.is_empty());
        assert!(rules_status(&rules).contains("No permission rules committed"));
    }

    #[test]
    fn a_broken_rules_file_is_reported_and_rules_are_not_applied() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = crate::permissions::rules_path(dir.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, "deny = [").expect("write");
        let error = load_rules_outcome(dir.path()).expect_err("broken");
        assert!(error.contains("Rules were not applied"), "{error}");
    }

    #[test]
    fn a_missing_allowlist_file_stays_fail_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let list = load_allowlist_outcome(dir.path()).expect("no file");
        assert!(list.is_empty());
        assert!(!list.allows("github.com"));
        assert!(allowlist_status(&list).contains("blocked"));
    }

    #[test]
    fn a_broken_allowlist_file_keeps_the_network_blocked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = crate::sandbox_allowlist::allowlist_path(dir.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, "allow = [").expect("write");
        let error = load_allowlist_outcome(dir.path()).expect_err("broken");
        assert!(error.contains("stays blocked"), "{error}");
    }

    #[test]
    fn rules_status_counts_every_group() {
        let rules = PermissionRules::parse(
            "allow = [\"git status*\"]\nask = [\"cargo test*\"]\ndeny = [\"rm -rf *\"]\n",
        )
        .expect("rules");
        let status = rules_status(&rules);
        assert!(status.contains("1 allow"), "{status}");
        assert!(status.contains("1 ask"), "{status}");
        assert!(status.contains("1 deny"), "{status}");
        assert!(status.contains("deny beats allow"), "{status}");
    }

    #[test]
    fn allowlist_status_reports_the_count_and_the_block() {
        let mut list = SandboxAllowlist::default();
        list.add("crates.io").expect("add");
        list.add("github.com").expect("add");
        let status = allowlist_status(&list);
        assert!(status.contains("2 domains"), "{status}");
        assert!(status.contains("blocked"), "{status}");
    }

    #[test]
    fn plugin_rows_lead_with_installed_plugins_then_the_registry() {
        let rows = plugin_marketplace_rows(&[
            ("cortex-review".into(), "0.4.1".into()),
            ("mermaid-preview".into(), "0.2.0".into()),
        ]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].1, "cortex-review");
        assert!(rows[0].2.contains("installed"));
        assert_eq!(rows[2].0, "__search__");
        assert!(
            rows[2].2.contains("cortex.foundation"),
            "the marketplace row must name the Cortex registry: {}",
            rows[2].2
        );
    }

    #[test]
    fn an_empty_plugin_list_still_opens_the_marketplace() {
        let rows = plugin_marketplace_rows(&[]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "__search__");
    }

    #[test]
    fn ide_status_names_stdio_at_both_sizes() {
        assert!(ide_status(false).contains("stdio"));
        assert!(ide_status(true).contains("stdio"));
        assert!(ide_status(false).contains("approval"));
    }

    #[test]
    fn applying_a_failed_load_reports_it_instead_of_opening_a_picker() {
        let mut state = AppState::default();
        let outcome = apply_rules_result(&mut state, Err("Could not read".into()));
        assert!(matches!(outcome, ProjectStateOutcome::Failed(_)));
        assert!(
            state.get_interactive_state().is_none(),
            "a failed load must not open a picker over stale state"
        );
        assert_eq!(state.messages.len(), 1);
    }

    #[test]
    fn applying_a_good_load_opens_the_picker() {
        let mut state = AppState::default();
        let rules = PermissionRules::parse("deny = [\"rm -rf *\"]").expect("rules");
        let outcome = apply_rules_result(&mut state, Ok(rules));
        assert!(matches!(outcome, ProjectStateOutcome::Opened(_)));
        assert!(state.get_interactive_state().is_some());
    }
}
