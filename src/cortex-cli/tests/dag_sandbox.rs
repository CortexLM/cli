//! `dag run` executes task commands only through the sandboxed Execute tool.
use std::process::Command;

#[cfg(target_os = "linux")]
#[test]
fn dag_run_executes_commands_inside_the_sandbox() {
    let workspace = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let spec = workspace.path().join("dag.json");
    std::fs::write(
        &spec,
        r#"{"name":"sandbox","tasks":[
            {"name":"inside","description":"write in workspace","command":"printf inside > inside.txt"},
            {"name":"outside","description":"write outside workspace","command":"printf leaked > /tmp/cortex-dag-sandbox-leak","depends_on":["inside"]}
        ]}"#,
    )
    .unwrap();
    let _ = std::fs::remove_file("/tmp/cortex-dag-sandbox-leak");

    let output = Command::new(env!("CARGO_BIN_EXE_Cortex"))
        .args(["dag", "run", "-f"])
        .arg(&spec)
        .args([
            "--strategy",
            "sequential",
            "--on-failure",
            "continue",
            "--format",
            "json",
            "-q",
        ])
        .current_dir(workspace.path())
        .env("CORTEX_HOME", home.path())
        .env_remove("CORTEX_DIAGNOSTICS_DIR")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        std::fs::read_to_string(workspace.path().join("inside.txt")).unwrap(),
        "inside",
        "{stderr}"
    );
    assert!(
        !std::path::Path::new("/tmp/cortex-dag-sandbox-leak").exists(),
        "sandbox allowed a write outside the workspace: {stderr}"
    );
    assert!(!output.status.success(), "{stderr}");
}
