use std::time::Duration;
use tokio::process::Command;

#[tokio::test]
async fn test_server_refuses_unconfigured_auth_and_exposed_anonymous_listener() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec!["--listen", "127.0.0.1:0", "--auth"],
        vec!["--listen", "0.0.0.0:0"],
    ] {
        let output = tokio::time::timeout(
            Duration::from_secs(5),
            Command::new(env!("CARGO_BIN_EXE_cortex-server"))
                .args(args)
                .current_dir(root.path())
                .env_clear()
                .env("HOME", root.path())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .expect("invalid configuration must fail before listening")
        .unwrap();
        assert!(!output.status.success());
    }
}
