use std::path::{Path, PathBuf};

use cortex_protocol::SandboxPolicy;

use super::{SandboxMode, SandboxWorkspaceWrite};

pub(super) fn resolve(
    mode: Option<SandboxMode>,
    workspace: Option<SandboxWorkspaceWrite>,
    additional_roots: Vec<PathBuf>,
    cwd: &Path,
) -> SandboxPolicy {
    match mode.unwrap_or_default() {
        SandboxMode::ReadOnly => SandboxPolicy::ReadOnly,
        SandboxMode::DangerFullAccess => SandboxPolicy::DangerFullAccess,
        SandboxMode::WorkspaceWrite => {
            let workspace = workspace.unwrap_or_default();
            let mut writable_roots = Vec::new();
            for root in workspace.writable_roots.into_iter().chain(additional_roots) {
                let root = if root.is_absolute() {
                    root
                } else {
                    cwd.join(root)
                };
                if !writable_roots.contains(&root) {
                    writable_roots.push(root);
                }
            }
            SandboxPolicy::WorkspaceWrite {
                writable_roots,
                network_access: workspace.network_access,
                exclude_tmpdir_env_var: workspace.exclude_tmpdir_env_var,
                exclude_slash_tmp: workspace.exclude_slash_tmp,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigOverrides, ConfigToml};

    fn load(text: &str, overrides: ConfigOverrides) -> Config {
        let root = std::env::temp_dir().join("cortex-config-policy");
        Config::from_toml(
            toml::from_str::<ConfigToml>(text).unwrap(),
            ConfigOverrides {
                cwd: Some(root.clone()),
                ..overrides
            },
            root,
        )
    }

    #[test]
    fn test_read_only_override_wins_over_permissive_config() {
        let config = load(
            "sandbox_mode = 'danger-full-access'",
            ConfigOverrides {
                sandbox_mode: Some(SandboxMode::ReadOnly),
                ..Default::default()
            },
        );
        assert!(matches!(config.sandbox_policy, SandboxPolicy::ReadOnly));
    }

    #[test]
    fn test_workspace_override_preserves_network_restrictions_and_roots() {
        let root = std::env::temp_dir().join("cortex-config-policy");
        let config = load(
            "sandbox_mode = 'read-only'\n[sandbox_workspace_write]\n\
             writable_roots = ['shared']\nnetwork_access = false\n\
             exclude_tmpdir_env_var = true\nexclude_slash_tmp = true",
            ConfigOverrides {
                sandbox_mode: Some(SandboxMode::WorkspaceWrite),
                additional_writable_roots: vec![root.join("shared"), PathBuf::from("second")],
                ..Default::default()
            },
        );
        match config.sandbox_policy {
            SandboxPolicy::WorkspaceWrite {
                writable_roots,
                network_access,
                exclude_tmpdir_env_var,
                exclude_slash_tmp,
            } => {
                assert_eq!(
                    writable_roots,
                    vec![root.join("shared"), root.join("second")]
                );
                assert!(!network_access);
                assert!(exclude_tmpdir_env_var);
                assert!(exclude_slash_tmp);
            }
            _ => panic!("workspace-write override was ignored"),
        }
    }

    #[test]
    fn test_additional_roots_do_not_weaken_read_only_mode() {
        let config = load(
            "sandbox_mode = 'read-only'",
            ConfigOverrides {
                additional_writable_roots: vec![PathBuf::from("other")],
                ..Default::default()
            },
        );
        assert!(matches!(config.sandbox_policy, SandboxPolicy::ReadOnly));
    }

    #[test]
    fn test_default_workspace_mode_applies_configured_restrictions() {
        let config = load(
            "[sandbox_workspace_write]\nexclude_slash_tmp = true",
            ConfigOverrides::default(),
        );
        assert!(matches!(
            config.sandbox_policy,
            SandboxPolicy::WorkspaceWrite {
                network_access: false,
                exclude_slash_tmp: true,
                ..
            }
        ));
    }

    #[test]
    fn test_explicit_full_access_is_not_selected_implicitly() {
        let config = load(
            "sandbox_mode = 'read-only'",
            ConfigOverrides {
                sandbox_mode: Some(SandboxMode::DangerFullAccess),
                ..Default::default()
            },
        );
        assert!(matches!(
            config.sandbox_policy,
            SandboxPolicy::DangerFullAccess
        ));
        let default = load("", ConfigOverrides::default());
        assert!(!matches!(
            default.sandbox_policy,
            SandboxPolicy::DangerFullAccess
        ));
    }
}
