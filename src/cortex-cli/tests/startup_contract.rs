use std::path::Path;
use std::process::{Command, Output};

fn doctor(home: &Path, cwd: &Path, debug: bool, diagnostics: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_Cortex"));
    if debug {
        command.arg("--debug");
    }
    command
        .args(["debug", "doctor", "--json"])
        .current_dir(cwd)
        .env("HOME", home)
        .env("CORTEX_HOME", home)
        .env_remove("CORTEX_DIAGNOSTICS_DIR")
        .env_remove("CORTEX_API_KEY")
        .env_remove("CORTEX_AUTH_TOKEN")
        .env_remove("RUST_LOG");
    if let Some(directory) = diagnostics {
        command.env("CORTEX_DIAGNOSTICS_DIR", directory);
    }
    command.output().unwrap()
}

fn assert_private_journal(directory: &Path) {
    let paths: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(paths.len(), 1);
    let content = std::fs::read_to_string(&paths[0]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(value["operation"], "cli.debug");
    assert_eq!(value["status"], 200);
    assert_eq!(value.as_object().unwrap().len(), 8);
    assert!(!content.contains("another-session-content"));
    assert!(!content.contains(directory.to_string_lossy().as_ref()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&paths[0]).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn test_diagnostics_remain_opt_in() {
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let output = doctor(home.path(), cwd.path(), false, None);
    assert!(output.status.success());
    assert!(!home.path().join("diagnostics").exists());
    assert!(!cwd.path().join("debug.txt").exists());
    serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap();
}

#[test]
fn test_debug_uses_private_content_free_journal_without_overwrite() {
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let existing = cwd.path().join("debug.txt");
    std::fs::write(&existing, "another-session-content").unwrap();
    let output = doctor(home.path(), cwd.path(), true, None);
    assert!(output.status.success());
    assert_eq!(
        std::fs::read_to_string(existing).unwrap(),
        "another-session-content"
    );
    assert_private_journal(&home.path().join("diagnostics"));
    serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap();
}

#[test]
fn test_debug_honors_explicit_diagnostic_directory() {
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let diagnostics = home.path().join("custom-diagnostics");
    let output = doctor(home.path(), cwd.path(), true, Some(&diagnostics));
    assert!(output.status.success());
    assert_private_journal(&diagnostics);
    assert!(!home.path().join("diagnostics").exists());
}

#[cfg(unix)]
#[test]
fn test_debug_rejects_symlinked_journal_and_preserves_debug_symlink() {
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(external.path(), home.path().join("diagnostics")).unwrap();
    let existing = external.path().join("keep");
    std::fs::write(&existing, "another-session-content").unwrap();
    std::os::unix::fs::symlink(&existing, cwd.path().join("debug.txt")).unwrap();
    let output = doctor(home.path(), cwd.path(), true, None);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Local diagnostics could not be initialized")
    );
    assert_eq!(
        std::fs::read_to_string(existing).unwrap(),
        "another-session-content"
    );
    assert_eq!(std::fs::read_dir(external.path()).unwrap().count(), 1);
}

#[test]
fn interactive_flags_are_validated_before_the_terminal_check() {
    let home = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_Cortex"))
            .args(args)
            .current_dir(home.path())
            .env("HOME", home.path())
            .env("CORTEX_HOME", home.path())
            .env_remove("CORTEX_DIAGNOSTICS_DIR")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let bad_sandbox = run(&["--sandbox", "wide-open"]);
    assert!(!bad_sandbox.status.success());
    assert!(String::from_utf8_lossy(&bad_sandbox.stderr).contains("Unknown sandbox mode"));

    let bad_dir = run(&["--add-dir", "/definitely/missing/root"]);
    assert!(!bad_dir.status.success());
    assert!(String::from_utf8_lossy(&bad_dir.stderr).contains("--add-dir"));

    // Valid policy and roots reach the TTY guard rather than being rejected.
    let ok = run(&["--sandbox", "workspace-write", "--add-dir", "."]);
    assert!(!ok.status.success());
    assert!(String::from_utf8_lossy(&ok.stderr).contains("requires a terminal"));
}
