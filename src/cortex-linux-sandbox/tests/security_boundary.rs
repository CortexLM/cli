#![cfg(target_os = "linux")]
use std::process::Command;

fn sandbox(root: &std::path::Path, policy: &str, script: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cortex-linux-sandbox"))
        .args([
            "--sandbox-policy-cwd",
            root.to_str().unwrap(),
            "--sandbox-policy",
            policy,
            "--",
            "/bin/sh",
            "-c",
            script,
        ])
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap()
}

#[test]
fn security_boundary_read_only_native_denies_workspace_write() {
    let root = std::env::temp_dir().join(format!("cortex-native-ro-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let ready = sandbox(&root, r#"{"type":"ReadOnly"}"#, "printf ready");
    assert!(
        ready.status.success(),
        "Native sandbox did not initialize: {}",
        String::from_utf8_lossy(&ready.stderr)
    );
    assert_eq!(ready.stdout, b"ready");
    let denied = sandbox(&root, r#"{"type":"ReadOnly"}"#, "printf bad > denied");
    assert!(!denied.status.success());
    assert!(!root.join("denied").exists());
    std::fs::remove_dir(&root).unwrap();
}

#[test]
fn security_boundary_native_workspace_write_stays_inside_root() {
    let parent = std::env::temp_dir().join(format!("cortex-native-write-{}", std::process::id()));
    let root = parent.join("workspace");
    std::fs::create_dir_all(&root).unwrap();
    let policy = serde_json::json!({
        "type":"Custom", "network_access":false,
        "writable_roots":[{"root":root,"read_only_subpaths":[]}]
    })
    .to_string();
    let allowed = sandbox(&root, &policy, "printf inside > allowed");
    assert!(
        allowed.status.success(),
        "Native sandbox did not initialize: {}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("allowed")).unwrap(),
        "inside"
    );
    let denied = sandbox(&root, &policy, "printf outside > ../denied");
    assert!(!denied.status.success());
    assert!(!parent.join("denied").exists());
    std::fs::remove_file(root.join("allowed")).unwrap();
    std::fs::remove_dir(&root).unwrap();
    std::fs::remove_dir(&parent).unwrap();
}
