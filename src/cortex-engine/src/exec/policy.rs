//! Required policy and platform sandbox preparation before every spawn.
use super::ExecOptions;
use crate::error::{CortexError, Result};
use crate::sandbox::{SandboxPolicyType, SandboxRunner, SandboxedCommand, WritableRoot};
use cortex_protocol::SandboxPolicy;

pub(super) async fn prepare(command: &[String], options: &ExecOptions) -> Result<SandboxedCommand> {
    evaluate(command, options.approval_granted).await?;
    let policy = match &options.sandbox_policy {
        SandboxPolicy::DangerFullAccess => SandboxPolicyType::DangerFullAccess,
        SandboxPolicy::ReadOnly => SandboxPolicyType::Custom {
            writable_roots: Vec::new(),
            network_access: false,
            allow_read_outside_workspace: true,
        },
        SandboxPolicy::WorkspaceWrite {
            writable_roots,
            network_access,
            ..
        } => {
            // Never infer permission from HOME caches, TMPDIR, or /tmp.
            let mut roots = vec![WritableRoot::with_standard_protections(
                options.cwd.canonicalize()?,
            )];
            for path in writable_roots {
                roots.push(WritableRoot::with_standard_protections(
                    path.canonicalize()?,
                ));
            }
            SandboxPolicyType::Custom {
                writable_roots: roots,
                network_access: *network_access,
                allow_read_outside_workspace: true,
            }
        }
    };
    SandboxRunner::new().prepare(command, &policy, &options.cwd)
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
