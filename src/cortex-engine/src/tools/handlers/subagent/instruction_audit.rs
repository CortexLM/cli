//! Managed-policy sources and instruction-omission audit journal helpers.

use crate::instruction_scopes::{InstructionPlan, InstructionScope};

/// Organization-managed instruction documents for this host.
///
/// Managed policy is read from the organization policy directory. There is no
/// local or project path that can stand in for it, so a child cannot omit it.
fn managed_policy_sources() -> Vec<std::path::PathBuf> {
    crate::org_policy::policy_dir()
        .map(|dir| vec![dir.join(crate::instruction_scopes::MANAGED_POLICY_FILE)])
        .unwrap_or_default()
}

/// Record an omission and, when the request named managed policy, the fact
/// that it still loaded. A journal that cannot be written is reported, never
/// silently dropped.
fn record_instruction_audit(
    plan: &InstructionPlan,
    load: &crate::instruction_scopes::InstructionLoad,
) {
    let Ok(home) = crate::config::find_cortex_home() else {
        return;
    };
    let skipped: Vec<&str> = plan.skipped().iter().map(|s| s.as_str()).collect();
    let loaded: Vec<&str> = load.loaded.iter().map(|s| s.as_str()).collect();
    if let Err(error) = crate::audit::record(
        &home,
        crate::audit::AuditKind::InstructionsOmitted,
        serde_json::json!({
            "source": "subagent",
            "skipped": skipped,
            "loaded": loaded,
        }),
    ) {
        tracing::warn!(%error, "Could not record instruction omission in the audit journal");
    }
    if plan.requested_managed() {
        if let Err(error) = crate::audit::record(
            &home,
            crate::audit::AuditKind::ManagedPolicyNeverOmitted,
            serde_json::json!({
                "source": "subagent",
                "requested": [InstructionScope::Managed.as_str()],
                "loaded": load.loaded.contains(&InstructionScope::Managed),
            }),
        ) {
            tracing::warn!(%error, "Could not record managed-policy retention in the audit journal");
        }
    }
}
