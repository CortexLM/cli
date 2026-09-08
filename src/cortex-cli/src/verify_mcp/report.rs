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
