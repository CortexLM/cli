//! Real `Cortex github` subprocesses against isolated directories.
//!
//! No test contacts github.com: every run points `GITHUB_API_URL` at a loopback
//! stub that only ever answers "service unavailable", so a successful GitHub read
//! or write is impossible and every asserted outcome is a real failure or a
//! purely local file operation.

use std::path::Path;
use std::process::{Command, Output, Stdio};

#[path = "github_support/loopback.rs"]
mod loopback;
use loopback::Loopback;

fn github(home: &Path, api: &Loopback, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_Cortex"))
        .arg("github")
        .args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("CORTEX_HOME", home)
        .env("NO_COLOR", "1")
        .env("GITHUB_API_URL", api.url())
        .env_remove("GITHUB_EVENT_PATH")
        .env_remove("GITHUB_REPOSITORY")
        .env_remove("GITHUB_TOKEN")
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

/// Distinctive strings that must never be echoed back into the Actions log.
const SECRET_BODY: &str = "confidential-event-body-marker";
const SECRET_TITLE: &str = "confidential-event-title-marker";

fn comment_event(directory: &Path, repository: &str) -> std::path::PathBuf {
    let path = directory.join("event.json");
    let payload = serde_json::json!({
        "action": "created",
        "repository": {"full_name": repository},
        "sender": {"type": "User", "login": "maintainer"},
        "issue": {"number": 7, "title": SECRET_TITLE, "pull_request": {}},
        "comment": {
            "id": 42,
            "body": format!("@cortex review {SECRET_BODY}"),
            "author_association": "MEMBER",
            "user": {"login": "maintainer"}
        }
    });
    std::fs::write(&path, payload.to_string()).unwrap();
    path
}

#[test]
fn run_fails_closed_without_output_or_publish_and_never_echoes_the_event() {
    let home = tempfile::tempdir().unwrap();
    let api = Loopback::refusing();
    let event = comment_event(home.path(), "fixture/project");
    let output = github(
        home.path(),
        &api,
        &[
            "run",
            "--event",
            "issue_comment",
            "--event-path",
            event.to_str().unwrap(),
            "--repository",
            "fixture/project",
        ],
    );
    assert!(!output.status.success());
    let text = text(&output);
    assert!(text.contains("--output"), "{text}");
    assert!(text.contains("--publish"), "{text}");
    assert!(!text.contains(SECRET_BODY), "{text}");
    assert!(!text.contains(SECRET_TITLE), "{text}");
    assert!(api.requests().is_empty(), "{:?}", api.requests());
}

#[test]
fn dry_run_starts_no_agent_writes_nothing_and_contacts_nothing() {
    let home = tempfile::tempdir().unwrap();
    let api = Loopback::refusing();
    let event = comment_event(home.path(), "fixture/project");
    let response = home.path().join("response.md");
    let output = github(
        home.path(),
        &api,
        &[
            "run",
            "--event",
            "issue_comment",
            "--event-path",
            event.to_str().unwrap(),
            "--repository",
            "fixture/project",
            "--dry-run",
            "--output",
            response.to_str().unwrap(),
        ],
    );
    let text = text(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("dry run did not start"), "{text}");
    assert!(!text.contains(SECRET_BODY), "{text}");
    assert!(!response.exists());
    assert!(api.requests().is_empty(), "{:?}", api.requests());
}

#[test]
fn unmatched_untrusted_and_malformed_events_are_refused_without_leaking_bodies() {
    let home = tempfile::tempdir().unwrap();
    let api = Loopback::refusing();
    let run = |path: &Path, repository: &str, event: &str| {
        github(
            home.path(),
            &api,
            &[
                "run",
                "--event",
                event,
                "--event-path",
                path.to_str().unwrap(),
                "--repository",
                repository,
                "--publish",
            ],
        )
    };

    // An event addressed to another repository must never start work here.
    let other = home.path().join("other.json");
    std::fs::copy(comment_event(home.path(), "other/project"), &other).unwrap();
    let mismatch = run(&other, "fixture/project", "issue_comment");
    assert!(!mismatch.status.success());
    assert!(
        text(&mismatch).contains("does not match"),
        "{}",
        text(&mismatch)
    );

    // A payload larger than the 1 MiB bound is refused before parsing.
    let oversized = home.path().join("oversized.json");
    std::fs::write(
        &oversized,
        format!("{{\"padding\":\"{}\"}}", "p".repeat(1024 * 1024)),
    )
    .unwrap();
    let too_large = run(&oversized, "fixture/project", "issue_comment");
    assert!(!too_large.status.success());
    assert!(
        text(&too_large).contains("too large"),
        "{}",
        text(&too_large)
    );

    // Non-JSON content is rejected as a whole, never partially interpreted.
    let invalid = home.path().join("invalid.json");
    std::fs::write(&invalid, format!("not json {SECRET_BODY}")).unwrap();
    let malformed = run(&invalid, "fixture/project", "issue_comment");
    assert!(!malformed.status.success());
    assert!(text(&malformed).contains("not valid JSON"));
    assert!(!text(&malformed).contains(SECRET_BODY));

    // A missing file is an error, not an empty "no request" success.
    let missing = run(
        &home.path().join("absent.json"),
        "fixture/project",
        "issue_comment",
    );
    assert!(!missing.status.success());
    assert!(text(&missing).contains("Could not open"));

    // Unsupported event kinds stop before any automation plan is built.
    let unsupported = run(
        &comment_event(home.path(), "fixture/project"),
        "fixture/project",
        "workflow_dispatch",
    );
    assert!(!unsupported.status.success());
    assert!(text(&unsupported).contains("Unsupported GitHub event"));

    // A comment without a Cortex mention is a no-op, not an agent run.
    let unrelated = home.path().join("unrelated.json");
    std::fs::write(
        &unrelated,
        serde_json::json!({
            "action": "created",
            "repository": {"full_name": "fixture/project"},
            "sender": {"type": "User", "login": "maintainer"},
            "issue": {"number": 7, "title": "Fixture", "pull_request": {}},
            "comment": {"id": 42, "body": SECRET_BODY,
                "author_association": "MEMBER", "user": {"login": "maintainer"}}
        })
        .to_string(),
    )
    .unwrap();
    let quiet = run(&unrelated, "fixture/project", "issue_comment");
    assert!(quiet.status.success(), "{}", text(&quiet));
    assert!(text(&quiet).contains("No matching Cortex request"));
    assert!(!text(&quiet).contains(SECRET_BODY));

    assert!(
        api.requests().is_empty(),
        "no rejected event may reach the API: {:?}",
        api.requests()
    );
}

/// Pull request event whose head commit is `sha`, addressed to `repository`.
fn pull_request_event(directory: &Path, repository: &str, sha: &str) -> std::path::PathBuf {
    let path = directory.join(format!("pr-{sha}.json"));
    let payload = serde_json::json!({
        "action": "opened",
        "repository": {"full_name": repository},
        "sender": {"type": "User", "login": "maintainer"},
        "pull_request": {
            "number": 5, "title": SECRET_TITLE, "body": SECRET_BODY, "draft": false,
            "author_association": "OWNER", "user": {"login": "maintainer"}, "labels": [],
            "head": {"ref": "topic", "sha": sha, "repo": {"full_name": repository}},
            "base": {"ref": "main", "sha": "0000"}
        }
    });
    std::fs::write(&path, payload.to_string()).unwrap();
    path
}

fn run_against(api: &Loopback, home: &Path, event: &Path, extra: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_Cortex"));
    command
        .args([
            "github",
            "run",
            "--event",
            "pull_request",
            "--repository",
            "fixture/project",
            "--token",
            "fixture-token",
        ])
        .arg("--event-path")
        .arg(event)
        .args(extra)
        .current_dir(home)
        .env("HOME", home)
        .env("CORTEX_HOME", home)
        .env("NO_COLOR", "1")
        .env("GITHUB_API_URL", api.url())
        .env_remove("GITHUB_EVENT_PATH")
        .env_remove("GITHUB_REPOSITORY")
        .env_remove("GITHUB_TOKEN")
        .stdin(Stdio::null());
    command.output().unwrap()
}

fn pull_request_body(head_repository: &str, sha: &str) -> serde_json::Value {
    serde_json::json!({
        "number": 5, "title": "Fixture", "state": "open", "body": null,
        "user": {"login": "maintainer"}, "mergeable": true, "draft": false, "labels": [],
        "head": {"ref": "topic", "sha": sha, "repo": {"full_name": head_repository}},
        "base": {"ref": "main", "sha": "0000", "repo": {"full_name": "fixture/project"}}
    })
}

#[test]
fn fork_and_stale_pull_requests_are_refused_after_the_metadata_read() {
    let home = tempfile::tempdir().unwrap();
    let output = home.path().join("response.md");

    // The event claims this repository, but the live PR head lives in a fork.
    let fork = Loopback::start(
        vec![(
            "/repos/fixture/project/pulls/5".into(),
            pull_request_body("attacker/project", "abc123"),
        )],
        "503 Service Unavailable",
    );
    let event = pull_request_event(home.path(), "fixture/project", "abc123");
    let refused = run_against(
        &fork,
        home.path(),
        &event,
        &["--output", output.to_str().unwrap()],
    );
    assert!(!refused.status.success(), "{}", text(&refused));
    assert!(
        text(&refused).contains("Fork pull requests"),
        "{}",
        text(&refused)
    );
    assert!(!output.exists());
    assert!(
        fork.requests().iter().all(|line| line.starts_with("GET ")),
        "{:?}",
        fork.requests()
    );

    // The branch moved since the event, so the pinned diff no longer exists.
    let moved = Loopback::start(
        vec![(
            "/repos/fixture/project/pulls/5".into(),
            pull_request_body("fixture/project", "def456"),
        )],
        "503 Service Unavailable",
    );
    let stale = run_against(
        &moved,
        home.path(),
        &event,
        &["--output", output.to_str().unwrap()],
    );
    assert!(!stale.status.success());
    assert!(
        text(&stale).contains("changed since this event"),
        "{}",
        text(&stale)
    );
    assert!(!output.exists());
}

#[test]
fn unavailable_patches_stop_a_review_that_could_not_be_complete() {
    let home = tempfile::tempdir().unwrap();
    let output = home.path().join("response.md");
    let api = Loopback::start(
        vec![
            (
                "/repos/fixture/project/pulls/5".into(),
                pull_request_body("fixture/project", "abc123"),
            ),
            (
                "/repos/fixture/project/pulls/5/files".into(),
                serde_json::json!([{
                    "filename": "assets/logo.png", "status": "added",
                    "additions": 0, "deletions": 0, "changes": 0, "patch": null
                }]),
            ),
        ],
        "503 Service Unavailable",
    );
    let event = pull_request_event(home.path(), "fixture/project", "abc123");
    let refused = run_against(
        &api,
        home.path(),
        &event,
        &["--output", output.to_str().unwrap()],
    );
    assert!(!refused.status.success(), "{}", text(&refused));
    assert!(
        text(&refused).contains("complete review is not supported"),
        "{}",
        text(&refused)
    );
    assert!(!output.exists());
    assert!(
        api.requests().iter().all(|line| line.starts_with("GET ")),
        "{:?}",
        api.requests()
    );
}

#[test]
fn issue_analysis_reads_the_issue_then_stops_at_the_unavailable_model_service() {
    let home = tempfile::tempdir().unwrap();
    let output = home.path().join("response.md");
    let api = Loopback::start(
        vec![(
            "/repos/fixture/project/issues/11".into(),
            serde_json::json!({
                "number": 11, "title": SECRET_TITLE, "state": "open",
                "body": SECRET_BODY, "user": {"login": "maintainer"},
                "labels": [], "pull_request": null
            }),
        )],
        "503 Service Unavailable",
    );
    let event = home.path().join("issue.json");
    std::fs::write(
        &event,
        serde_json::json!({
            "action": "opened",
            "repository": {"full_name": "fixture/project"},
            "sender": {"type": "User", "login": "maintainer"},
            "issue": {"number": 11, "title": "Cortex please analyze", "body": "fixture",
                "author_association": "OWNER", "user": {"login": "maintainer"}, "labels": []}
        })
        .to_string(),
    )
    .unwrap();

    let mut command = Command::new(env!("CARGO_BIN_EXE_Cortex"));
    let attempt = command
        .args([
            "github",
            "run",
            "--event",
            "issues",
            "--repository",
            "fixture/project",
            "--token",
            "fixture-token",
        ])
        .arg("--event-path")
        .arg(&event)
        .args(["--output", output.to_str().unwrap()])
        .current_dir(home.path())
        .env("HOME", home.path())
        .env("CORTEX_HOME", home.path())
        .env("NO_COLOR", "1")
        .env("GITHUB_API_URL", api.url())
        .env_remove("GITHUB_EVENT_PATH")
        .env_remove("GITHUB_REPOSITORY")
        .env_remove("CORTEX_API_KEY")
        .env_remove("CORTEX_AUTH_TOKEN")
        .stdin(Stdio::null())
        .output()
        .unwrap();

    // The issue is really fetched, but there is no coding service here, so the
    // command must fail rather than save or publish an invented analysis.
    assert!(
        api.requests()
            .iter()
            .any(|line| line.contains("/repos/fixture/project/issues/11")),
        "{:?}",
        api.requests()
    );
    assert!(!attempt.status.success(), "{}", text(&attempt));
    assert!(
        !output.exists(),
        "no analysis existed, so none may be saved"
    );
    let reported = text(&attempt);
    assert!(!reported.contains(SECRET_BODY), "{reported}");
    assert!(!reported.contains(SECRET_TITLE), "{reported}");
    assert!(!reported.contains("fixture-token"), "{reported}");
    assert!(
        api.requests().iter().all(|line| line.starts_with("GET ")),
        "{:?}",
        api.requests()
    );
}

#[test]
fn an_already_answered_event_starts_no_second_analysis() {
    let home = tempfile::tempdir().unwrap();
    let output = home.path().join("response.md");
    let api = Loopback::start(
        vec![(
            "/repos/fixture/project/issues/5/comments".into(),
            serde_json::json!([{"body": "earlier reply\n\n<!-- cortex-automation:pr-5-abc123 -->"}]),
        )],
        "503 Service Unavailable",
    );
    let event = pull_request_event(home.path(), "fixture/project", "abc123");
    let skipped = run_against(
        &api,
        home.path(),
        &event,
        &["--publish", "--output", output.to_str().unwrap()],
    );
    assert!(skipped.status.success(), "{}", text(&skipped));
    assert!(
        text(&skipped).contains("already has a Cortex response"),
        "{}",
        text(&skipped)
    );
    assert!(!output.exists(), "no second analysis may be saved");
    let requests = api.requests();
    assert!(
        requests.iter().all(|line| line.starts_with("GET ")),
        "{requests:?}"
    );
    assert!(
        !requests.iter().any(|line| line.contains("/pulls/5")),
        "the pull request must not even be fetched: {requests:?}"
    );
}

#[test]
fn actions_environment_supplies_the_event_but_a_bad_repository_stops_first() {
    let home = tempfile::tempdir().unwrap();
    let api = Loopback::refusing();
    let event = comment_event(home.path(), "fixture/project");
    let actions = |repository: &str, event_path: Option<&Path>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_Cortex"));
        command
            .args(["github", "run", "--event", "issue_comment", "--dry-run"])
            .current_dir(home.path())
            .env("HOME", home.path())
            .env("CORTEX_HOME", home.path())
            .env("NO_COLOR", "1")
            .env("GITHUB_API_URL", api.url())
            .env("GITHUB_REPOSITORY", repository)
            .env_remove("GITHUB_EVENT_PATH")
            .stdin(Stdio::null());
        if let Some(path) = event_path {
            command.env("GITHUB_EVENT_PATH", path);
        }
        command.output().unwrap()
    };

    let resolved = actions("fixture/project", Some(&event));
    assert!(resolved.status.success(), "{}", text(&resolved));
    assert!(text(&resolved).contains("dry run did not start"));

    // A repository that is not owner/repo is refused before the event is read.
    for repository in ["fixture", "fixture/project/extra", "   "] {
        let rejected = actions(repository, Some(&event));
        assert!(!rejected.status.success(), "{repository}");
        assert!(!text(&rejected).contains(SECRET_BODY));
    }

    // Without GITHUB_EVENT_PATH there is nothing to authorize.
    let no_event = actions("fixture/project", None);
    assert!(!no_event.status.success());
    assert!(text(&no_event).contains("GITHUB_EVENT_PATH"));
    assert!(api.requests().is_empty(), "{:?}", api.requests());
}

#[test]
fn publish_never_writes_when_the_service_is_unavailable() {
    let home = tempfile::tempdir().unwrap();
    let api = Loopback::refusing();
    let event = comment_event(home.path(), "fixture/project");
    let response = home.path().join("response.md");
    let output = github(
        home.path(),
        &api,
        &[
            "run",
            "--event",
            "issue_comment",
            "--event-path",
            event.to_str().unwrap(),
            "--repository",
            "fixture/project",
            "--publish",
            "--output",
            response.to_str().unwrap(),
            "--token",
            "fixture-token",
        ],
    );
    let text = text(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        !response.exists(),
        "no analysis existed, so none may be saved"
    );
    assert!(!text.contains(SECRET_BODY), "{text}");
    assert!(!text.contains("fixture-token"), "{text}");
    let requests = api.requests();
    assert!(
        requests.iter().all(|line| line.starts_with("GET ")),
        "publication must not be attempted: {requests:?}"
    );
}

#[test]
fn install_status_update_and_uninstall_operate_on_a_real_workflow_file() {
    let home = tempfile::tempdir().unwrap();
    let api = Loopback::refusing();
    let repo = home.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let path = repo.to_str().unwrap();
    let workflow = repo.join(".github/workflows/cortex.yml");

    // Status before installation reports a missing workflow and fails.
    let missing = github(home.path(), &api, &["status", "--path", path, "--json"]);
    assert!(!missing.status.success());
    let status: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(status["workflow_installed"], false);

    let installed = github(
        home.path(),
        &api,
        &["install", "--path", path, "--workflow-name", "cortex"],
    );
    assert!(installed.status.success(), "{}", text(&installed));
    let original = std::fs::read_to_string(&workflow).unwrap();

    // A second install must not silently overwrite the existing workflow.
    let refused = github(
        home.path(),
        &api,
        &["install", "--path", path, "--workflow-name", "cortex"],
    );
    assert!(!refused.status.success());
    assert!(text(&refused).contains("already exists"));
    assert_eq!(std::fs::read_to_string(&workflow).unwrap(), original);

    let reported = github(home.path(), &api, &["status", "--path", path]);
    assert!(reported.status.success(), "{}", text(&reported));
    assert!(text(&reported).contains("workflow is installed"));
    assert!(text(&reported).contains("issue_comment"));

    let updated = github(
        home.path(),
        &api,
        &["update", "--path", path, "--workflow-name", "cortex"],
    );
    assert!(updated.status.success(), "{}", text(&updated));
    assert_eq!(std::fs::read_to_string(&workflow).unwrap(), original);

    // Without --force and without an answer, the workflow must survive.
    let cancelled = github(home.path(), &api, &["uninstall", "--path", path]);
    assert!(cancelled.status.success(), "{}", text(&cancelled));
    assert!(workflow.exists());

    let removed = github(home.path(), &api, &["uninstall", "--path", path, "--force"]);
    assert!(removed.status.success(), "{}", text(&removed));
    assert!(!workflow.exists());

    // Removing again is an explicit error, not a silent success.
    let absent = github(home.path(), &api, &["uninstall", "--path", path, "--force"]);
    assert!(!absent.status.success());
    assert!(text(&absent).contains("not found"));
    let stale = github(home.path(), &api, &["update", "--path", path]);
    assert!(!stale.status.success());
    assert!(text(&stale).contains("not found"));
}

#[test]
fn workflow_commands_reject_unusable_repository_roots() {
    let home = tempfile::tempdir().unwrap();
    let api = Loopback::refusing();
    let repo = home.path().join("blocked");
    std::fs::create_dir_all(&repo).unwrap();
    // A regular file where the workflow directory belongs must not be replaced.
    std::fs::write(repo.join(".github"), "not a directory").unwrap();
    let path = repo.to_str().unwrap();
    for args in [
        vec!["install", "--path", path],
        vec!["update", "--path", path],
        vec!["uninstall", "--path", path, "--force"],
    ] {
        let output = github(home.path(), &api, &args);
        assert!(!output.status.success(), "{:?}", args);
        assert!(
            text(&output).contains("not a directory"),
            "{}",
            text(&output)
        );
    }
    assert_eq!(
        std::fs::read_to_string(repo.join(".github")).unwrap(),
        "not a directory"
    );

    let absent = github(home.path(), &api, &["uninstall", "--path", "/missing/root"]);
    assert!(!absent.status.success());
    assert!(text(&absent).contains("does not exist"));

    // A traversal component is refused before the path is resolved.
    let traversal = github(home.path(), &api, &["install", "--path", "../elsewhere"]);
    assert!(!traversal.status.success());
    assert!(text(&traversal).contains("traversal"));
}
