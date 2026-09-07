//! `Cortex pr` against a local git repository and a loopback GitHub API.
//!
//! github.com is never contacted: the repository's `origin` is a local bare
//! clone and `GITHUB_API_URL` points at a fixture endpoint that answers only
//! read-only GET paths. No test pushes, comments, or applies suggestions.

use std::path::Path;
use std::process::{Command, Output, Stdio};

#[path = "github_support/loopback.rs"]
mod loopback;
use loopback::Loopback;

fn git(directory: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(directory)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap()
}

/// A repository whose `origin` is a GitHub-shaped URL that is never reachable.
fn repository(home: &Path) -> std::path::PathBuf {
    let repo = home.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    assert!(git(&repo, &["init", "--quiet"]).status.success());
    std::fs::write(repo.join("README.md"), "fixture\n").unwrap();
    assert!(git(&repo, &["add", "README.md"]).status.success());
    assert!(
        git(&repo, &["commit", "--quiet", "-m", "initial"])
            .status
            .success()
    );
    assert!(
        git(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/fixture/project.git"
            ]
        )
        .status
        .success()
    );
    repo
}

fn pr(home: &Path, api: &Loopback, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_Cortex"))
        .arg("pr")
        .args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("CORTEX_HOME", home)
        .env("NO_COLOR", "1")
        .env("GITHUB_API_URL", api.url())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
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

fn pull_request() -> serde_json::Value {
    serde_json::json!({
        "number": 12, "title": "Fixture change", "state": "open",
        "body": "Fixture description", "user": {"login": "contributor"},
        "mergeable": true, "draft": true, "labels": [{"name": "review"}],
        "head": {"ref": "topic", "sha": "abc123", "repo": {"full_name": "fixture/project"}},
        "base": {"ref": "main", "sha": "0000", "repo": {"full_name": "fixture/project"}}
    })
}

#[test]
fn info_reports_the_pull_request_without_changing_the_working_tree() {
    let home = tempfile::tempdir().unwrap();
    let repo = repository(home.path());
    let api = Loopback::start(
        vec![("/repos/fixture/project/pulls/12".into(), pull_request())],
        "503 Service Unavailable",
    );
    let before = git(&repo, &["rev-parse", "HEAD"]).stdout;

    let output = pr(
        home.path(),
        &api,
        &["12", "--path", repo.to_str().unwrap(), "--info"],
    );
    let report = text(&output);
    assert!(output.status.success(), "{report}");
    assert!(report.contains("Fixture change"), "{report}");
    assert!(report.contains("@contributor"), "{report}");
    assert!(report.contains("(draft)"), "{report}");
    assert!(report.contains("Base: main"), "{report}");
    assert!(report.contains("Fixture description"), "{report}");
    assert!(
        report.contains("https://github.com/fixture/project/pull/12"),
        "{report}"
    );

    // Nothing was fetched or checked out.
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).stdout, before);
    assert!(
        git(&repo, &["rev-parse", "--verify", "pr-12"])
            .status
            .success()
            .eq(&false)
    );
    assert!(
        api.requests().iter().all(|line| line.starts_with("GET ")),
        "{:?}",
        api.requests()
    );
}

#[test]
fn a_dirty_worktree_blocks_checkout_before_any_fetch() {
    let home = tempfile::tempdir().unwrap();
    let repo = repository(home.path());
    let api = Loopback::start(
        vec![("/repos/fixture/project/pulls/12".into(), pull_request())],
        "503 Service Unavailable",
    );
    std::fs::write(repo.join("README.md"), "uncommitted work\n").unwrap();

    let output = pr(home.path(), &api, &["12", "--path", repo.to_str().unwrap()]);
    let report = text(&output);
    assert!(!output.status.success(), "{report}");
    assert!(report.contains("Uncommitted changes detected"), "{report}");
    assert!(report.contains("--force"), "{report}");
    // The user's uncommitted work is untouched.
    assert_eq!(
        std::fs::read_to_string(repo.join("README.md")).unwrap(),
        "uncommitted work\n"
    );
}

#[test]
fn a_clean_checkout_fails_honestly_when_the_pull_request_cannot_be_fetched() {
    let home = tempfile::tempdir().unwrap();
    let repo = repository(home.path());
    let api = Loopback::start(
        vec![("/repos/fixture/project/pulls/12".into(), pull_request())],
        "503 Service Unavailable",
    );
    let before = git(&repo, &["rev-parse", "HEAD"]).stdout;

    // origin points at github.com but no network exists here, so the fetch must
    // fail loudly rather than reporting a checkout that never happened.
    let output = pr(
        home.path(),
        &api,
        &["12", "--path", repo.to_str().unwrap(), "--branch", "pr-12"],
    );
    let report = text(&output);
    assert!(!output.status.success(), "{report}");
    assert!(report.contains("Failed to fetch PR"), "{report}");
    assert!(!report.contains("Checked out PR"), "{report}");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).stdout, before);
}

#[test]
fn an_unsafe_branch_name_is_refused_before_git_is_invoked() {
    let home = tempfile::tempdir().unwrap();
    let repo = repository(home.path());
    let api = Loopback::start(
        vec![("/repos/fixture/project/pulls/12".into(), pull_request())],
        "503 Service Unavailable",
    );
    for branch in [
        "evil;touch owned",
        "$(touch owned)",
        "a..b",
        "topic.lock",
        "-upload-pack=touch owned",
    ] {
        let output = pr(
            home.path(),
            &api,
            &[
                "12",
                "--path",
                repo.to_str().unwrap(),
                &format!("--branch={branch}"),
                "--force",
            ],
        );
        let report = text(&output);
        assert!(!output.status.success(), "{branch}: {report}");
        assert!(report.contains("Invalid branch name"), "{report}");
    }
    assert!(!repo.join("owned").exists());
}
