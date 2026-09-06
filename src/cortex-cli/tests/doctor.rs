use std::process::Command;

#[test]
fn test_doctor_binary_reports_local_readiness_and_invalid_config() {
    let home = tempfile::tempdir().unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_Cortex"))
            .args(["debug", "doctor", "--json"])
            .env("CORTEX_HOME", home.path())
            .env_remove("CORTEX_DIAGNOSTICS_DIR")
            .output()
            .unwrap()
    };
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ready"], true);
    assert_eq!(value["scope"], "local");
    assert_eq!(value["coding_service"], "not_checked");
    std::fs::write(home.path().join("config.toml"), "invalid = [").unwrap();
    let output = run();
    assert!(!output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["checks"]["configuration"], false);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("invalid ="));
}
