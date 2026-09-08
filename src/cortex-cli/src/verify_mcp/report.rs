//! `report.finish` — write `cortex-verify/1` JSON.

use std::path::PathBuf;

use anyhow::Result;
use serde_json::{Value, json};

use super::state::VerifyState;

pub fn finish(state: &mut VerifyState, args: &Value) -> Result<Value> {
    let run_id = args
        .get("run_id")
        .and_then(Value::as_str)
        .unwrap_or("latest");
    state.report.exit_code = if state.report.summary.fail > 0 { 1 } else { 0 };
    let value = serde_json::to_value(&state.report)?;
    let path = report_path(run_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&value)?)?;
    state.last_report = Some(value.clone());
    state.last_report_path = Some(path.clone());
    Ok(json!({
        "schema": "cortex-verify/1",
        "path": path.display().to_string(),
        "report": value,
    }))
}

pub fn report_path(run_id: &str) -> PathBuf {
    let root = super::lock::workspace_root();
    root.join("target/readiness/cli-verify")
        .join(format!("{run_id}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify_mcp::state::{FlowRow, VerifyState};
    use serde_json::json;

    #[test]
    fn finish_writes_cortex_verify_schema_and_exit_code() {
        let mut state = VerifyState::new();
        let ok = finish(&mut state, &json!({})).expect("default run_id");
        assert_eq!(ok["schema"], "cortex-verify/1");
        assert_eq!(ok["report"]["exit_code"], 0);
        assert!(
            ok["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("latest.json"))
        );

        state.record_flow(FlowRow {
            id: "failed".into(),
            status: "fail".into(),
            checks: vec![],
        });
        let failed = finish(&mut state, &json!({"run_id": "unit-failed"})).expect("failed");
        assert_eq!(failed["report"]["exit_code"], 1);
        assert_eq!(
            state.last_report.as_ref().map(|v| v["schema"].as_str()),
            Some(Some("cortex-verify/1"))
        );
        assert!(report_path("unit-failed").exists());
    }
}
