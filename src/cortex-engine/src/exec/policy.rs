//! Required policy and platform sandbox preparation before every spawn.
use super::ExecOptions;
use crate::error::{CortexError, Result};
use crate::sandbox::{SandboxPolicyType, SandboxRunner, SandboxedCommand, WritableRoot};
use cortex_protocol::SandboxPolicy;
use std::path::Path;

pub(super) async fn prepare(command: &[String], options: &ExecOptions) -> Result<SandboxedCommand> {
    evaluate(command, options.approval_granted).await?;
    let policy = mapped_sandbox_policy(&options.sandbox_policy, &options.cwd)?;
    SandboxRunner::new().prepare(command, &policy, &options.cwd)
}

fn mapped_sandbox_policy(policy: &SandboxPolicy, cwd: &Path) -> Result<SandboxPolicyType> {
    Ok(match policy {
        SandboxPolicy::DangerFullAccess => SandboxPolicyType::DangerFullAccess,
        SandboxPolicy::ReadOnly => SandboxPolicyType::Custom {
            writable_roots: Vec::new(),
            network_access: false,
            allow_read_outside_workspace: true,
        },
        SandboxPolicy::WorkspaceWrite { writable_roots, .. } => {
            // Never infer permission from HOME caches, TMPDIR, or /tmp.
            let mut roots = vec![WritableRoot::with_standard_protections(cwd.canonicalize()?)];
            for path in writable_roots {
                roots.push(WritableRoot::with_standard_protections(
                    path.canonicalize()?,
                ));
            }
            SandboxPolicyType::Custom {
                writable_roots: roots,
                // `network_access` is a `bool`. Do not dereference it: rustc match
                // ergonomics bind the Copy field as `bool` on Windows nightly (E0614).
                network_access: policy.has_full_network_access(),
                allow_read_outside_workspace: true,
            }
        }
    })
}

async fn evaluate(command: &[String], approved: bool) -> Result<()> {
    use cortex_execpolicy::Decision;
    match cortex_execpolicy::evaluate(command) {
        Decision::Allow => Ok(()),
        Decision::Ask if approved => Ok(()),
        Decision::Ask => Err(CortexError::tool_execution(
            "Execute",
            "Execution policy requires explicit approval",
        )),
        Decision::Deny => Err(CortexError::tool_execution(
            "Execute",
            "Execution policy denied the command",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_write(network_access: bool) -> SandboxPolicy {
        SandboxPolicy::WorkspaceWrite {
            writable_roots: vec![],
            network_access,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
        }
    }

    #[test]
    fn workspace_write_network_access_is_copied_as_bool() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path();

        let enabled = mapped_sandbox_policy(&workspace_write(true), cwd).unwrap();
        assert!(matches!(
            enabled,
            SandboxPolicyType::Custom {
                network_access: true,
                ..
            }
        ));

        let disabled = mapped_sandbox_policy(&workspace_write(false), cwd).unwrap();
        assert!(matches!(
            disabled,
            SandboxPolicyType::Custom {
                network_access: false,
                ..
            }
        ));
    }

    #[test]
    fn read_only_disables_network_access() {
        let dir = tempfile::tempdir().unwrap();
        let policy = mapped_sandbox_policy(&SandboxPolicy::ReadOnly, dir.path()).unwrap();
        assert!(matches!(
            policy,
            SandboxPolicyType::Custom {
                network_access: false,
                ..
            }
        ));
    }
}
