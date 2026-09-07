//! Decision and validation surfaces of `Cortex upgrade` / `Cortex uninstall`.
//!
//! No test performs a real upgrade or uninstall: the update server is pointed at
//! a loopback endpoint that only ever refuses, and every filesystem assertion is
//! made against a temporary directory. `uninstall` is only ever exercised in
//! `--dry-run`, which is the deepest it can go without deleting the running
//! binary and the caller's real home directory.

use std::path::Path;
use std::process::{Command, Output, Stdio};

#[path = "github_support/loopback.rs"]
mod loopback;
use loopback::Loopback;

/// A release payload for this platform pointing at an address nothing serves.
fn release(version: &str) -> serde_json::Value {
    let platform = format!(
        "{}-{}",
        if cfg!(target_os = "macos") {
            "darwin"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            "linux"
        },
        std::env::consts::ARCH
    );
    serde_json::json!({
        "version": version,
        "channel": "stable",
        "released_at": "2024-01-01T00:00:00Z",
        "changelog_url": null,
        "release_notes": "fixture release notes",
        "assets": {platform: {
            "url": "http://127.0.0.1:1/never-served.tar.gz",
            "sha256": "0".repeat(64),
            "size": 1
        }},
        "signatures": {}
    })
}

fn cortex(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_Cortex"))
        .args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("CORTEX_HOME", home)
        .env("NO_COLOR", "1")
        .env_remove("CORTEX_API_KEY")
        .env_remove("CORTEX_AUTH_TOKEN")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn upgrade_rejects_invalid_versions_and_channels_before_contacting_a_server() {
    let home = tempfile::tempdir().unwrap();
    let api = Loopback::refusing();
    let url = api.url();
    for args in [
        vec!["upgrade", "1.0", "--url", &url],
        vec!["upgrade", "vv1.0.0", "--url", &url],
        vec!["upgrade", "../1.0.0", "--url", &url],
        vec!["upgrade", "--channel", "experimental", "--url", &url],
    ] {
        let output = cortex(home.path(), &args);
        assert!(!output.status.success(), "{args:?}");
        assert!(!text(&output).contains("Downloading"), "{args:?}");
    }
    assert!(
        api.requests().is_empty(),
        "invalid input must not reach the update server: {:?}",
        api.requests()
    );
}

#[test]
fn upgrade_reports_an_unavailable_service_instead_of_installing_anything() {
    let home = tempfile::tempdir().unwrap();
    let api = Loopback::refusing();
    let url = api.url();

    let latest = cortex(home.path(), &["upgrade", "--check", "--url", &url]);
    assert!(!latest.status.success(), "{}", text(&latest));
    assert!(
        text(&latest).contains("temporarily unavailable"),
        "{}",
        text(&latest)
    );

    let specific = cortex(home.path(), &["upgrade", "9.9.9", "--url", &url, "--yes"]);
    assert!(!specific.status.success());
    assert!(
        text(&specific).contains("No update was installed"),
        "{}",
        text(&specific)
    );

    assert!(
        !api.requests().is_empty(),
        "the command must really attempt the lookup it reports on"
    );
    assert!(
        text(&specific).contains(&url),
        "the operator must see which server was used"
    );
}

#[test]
fn a_current_release_needs_no_upgrade_and_check_only_installs_nothing() {
    let home = tempfile::tempdir().unwrap();
    let current = env!("CARGO_PKG_VERSION");
    let api = Loopback::start(
        vec![
            (format!("/v1/releases/{current}.json"), release(current)),
            ("/v1/releases/latest.json".into(), release("99.0.0")),
        ],
        "404 Not Found",
    );
    let url = api.url();

    // Asking for the version already installed is not an upgrade.
    let same = cortex(home.path(), &["upgrade", current, "--url", &url, "--yes"]);
    let report = text(&same);
    assert!(same.status.success(), "{report}");
    assert!(report.contains("No upgrade needed"), "{report}");
    assert!(report.contains("--force to reinstall"), "{report}");
    assert!(!report.contains("Downloading"), "{report}");

    // --check reports the newer release and stops before installing it.
    let checked = cortex(home.path(), &["upgrade", "--check", "--url", &url]);
    let report = text(&checked);
    assert!(checked.status.success(), "{report}");
    assert!(report.contains("99.0.0"), "{report}");
    assert!(report.contains("Update available"), "{report}");
    assert!(report.contains("fixture release notes"), "{report}");
    assert!(
        report.contains("Run `cortex upgrade` to install"),
        "{report}"
    );
    assert!(!report.contains("Downloading"), "{report}");
    assert!(!report.contains("Verifying checksum"), "{report}");

    assert!(
        api.requests().iter().all(|line| line.starts_with("GET ")),
        "{:?}",
        api.requests()
    );
}

#[test]
fn a_release_server_answering_with_another_version_is_not_trusted() {
    let home = tempfile::tempdir().unwrap();
    // The operator asked for 9.9.9 but the server answers with a different one.
    let api = Loopback::start(
        vec![("/v1/releases/9.9.9.json".into(), release("1.2.3"))],
        "404 Not Found",
    );
    let output = cortex(
        home.path(),
        &["upgrade", "9.9.9", "--url", &api.url(), "--yes"],
    );
    let report = text(&output);
    assert!(!output.status.success(), "{report}");
    assert!(report.contains("No update was installed"), "{report}");
    assert!(!report.contains("Downloading"), "{report}");
}

#[test]
fn an_unconfirmed_upgrade_downloads_nothing() {
    let home = tempfile::tempdir().unwrap();
    let api = Loopback::start(
        vec![("/v1/releases/latest.json".into(), release("99.0.0"))],
        "404 Not Found",
    );
    // stdin is closed, so the confirmation reads as "no".
    let output = cortex(home.path(), &["upgrade", "--url", &api.url()]);
    let report = text(&output);
    assert!(output.status.success(), "{report}");
    assert!(report.contains("Upgrade cancelled"), "{report}");
    assert!(!report.contains("Downloading v99.0.0"), "{report}");
    assert!(
        !api.requests()
            .iter()
            .any(|line| line.contains("never-served")),
        "the asset must not be requested: {:?}",
        api.requests()
    );
}

#[test]
fn uninstall_dry_run_reports_without_deleting_anything() {
    let home = tempfile::tempdir().unwrap();
    let cortex_home = home.path().join(".cortex");
    std::fs::create_dir_all(cortex_home.join("sessions")).unwrap();
    std::fs::write(cortex_home.join("config.toml"), "fixture = true").unwrap();
    std::fs::write(cortex_home.join("sessions/history.jsonl"), "fixture").unwrap();

    let output = cortex(home.path(), &["uninstall", "--dry-run"]);
    let report = text(&output);
    assert!(output.status.success(), "{report}");
    assert!(
        report.contains("[DRY RUN] No files were deleted."),
        "{report}"
    );
    assert!(report.contains("config.toml"), "{report}");
    assert!(report.contains("sessions"), "{report}");
    assert_eq!(
        std::fs::read_to_string(cortex_home.join("config.toml")).unwrap(),
        "fixture = true"
    );
    assert_eq!(
        std::fs::read_to_string(cortex_home.join("sessions/history.jsonl")).unwrap(),
        "fixture"
    );
}

#[test]
fn uninstall_keep_flags_exclude_preserved_categories_from_the_plan() {
    let home = tempfile::tempdir().unwrap();
    let cortex_home = home.path().join(".cortex");
    std::fs::create_dir_all(cortex_home.join("sessions")).unwrap();
    std::fs::write(cortex_home.join("config.toml"), "fixture = true").unwrap();
    std::fs::write(cortex_home.join("sessions/history.jsonl"), "fixture").unwrap();

    let kept = text(&cortex(
        home.path(),
        &["uninstall", "--dry-run", "--keep-config", "--keep-data"],
    ));
    assert!(!kept.contains("config.toml"), "{kept}");
    assert!(!kept.contains("Session Data"), "{kept}");

    let purged = text(&cortex(home.path(), &["uninstall", "--dry-run", "--purge"]));
    assert!(purged.contains("config.toml"), "{purged}");
    assert!(purged.contains("Session Data"), "{purged}");

    // --purge and the keep flags are mutually exclusive by construction.
    let conflict = cortex(
        home.path(),
        &["uninstall", "--dry-run", "--purge", "--keep-config"],
    );
    assert!(!conflict.status.success());

    assert!(cortex_home.join("config.toml").exists());
    assert!(cortex_home.join("sessions/history.jsonl").exists());
}

#[test]
fn a_declined_uninstall_creates_no_backup_of_the_users_data() {
    let home = tempfile::tempdir().unwrap();
    let cortex_home = home.path().join(".cortex");
    std::fs::create_dir_all(cortex_home.join("sessions")).unwrap();
    std::fs::write(cortex_home.join("sessions/history.jsonl"), "fixture").unwrap();

    // The backup is deliberately created only after confirmation; stdin is
    // closed here, so nothing may be copied anywhere.
    let declined = cortex(home.path(), &["uninstall", "--backup"]);
    assert!(declined.status.success(), "{}", text(&declined));
    assert!(text(&declined).contains("cancelled"), "{}", text(&declined));
    assert!(!home.path().join(".cortex-backup").exists());
    assert!(cortex_home.join("sessions/history.jsonl").exists());
}

#[test]
fn uninstall_without_confirmation_or_installed_data_removes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let cortex_home = home.path().join(".cortex");
    std::fs::create_dir_all(&cortex_home).unwrap();
    std::fs::write(cortex_home.join("config.toml"), "fixture = true").unwrap();

    // stdin is closed, so the confirmation reads as "no" and nothing is removed.
    let cancelled = cortex(home.path(), &["uninstall"]);
    assert!(cancelled.status.success(), "{}", text(&cancelled));
    assert!(
        text(&cancelled).contains("cancelled"),
        "{}",
        text(&cancelled)
    );
    assert_eq!(
        std::fs::read_to_string(cortex_home.join("config.toml")).unwrap(),
        "fixture = true"
    );

    // A home with no Cortex data still reports only what actually exists.
    let empty = tempfile::tempdir().unwrap();
    let nothing = text(&cortex(empty.path(), &["uninstall", "--dry-run"]));
    assert!(!nothing.contains("config.toml"), "{nothing}");
    assert!(!nothing.contains("Session Data"), "{nothing}");
}
