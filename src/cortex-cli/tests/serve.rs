use std::time::Duration;
use tokio::process::Command;

#[tokio::test]
async fn test_serve_honors_environment_auth_and_rejects_empty_credentials() {
    let home = tempfile::tempdir().unwrap();
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new(env!("CARGO_BIN_EXE_Cortex"))
            .args(["serve", "--host", "127.0.0.1", "--port", "0"])
            .env_clear()
            .env("HOME", home.path())
            .env("CORTEX_HOME", home.path())
            .env("CORTEX_SERVER_API_KEY", "")
            .current_dir(home.path())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("invalid configuration must fail before listening")
    .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Authentication is enabled but no server credential is configured")
    );
}
