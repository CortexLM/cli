use super::*;
use cortex_engine::github::{WorkflowConfig, generate_workflow, parse_event};
use serde_json::json;

fn comment_payload() -> serde_json::Value {
    json!({
        "action": "created", "repository": {"full_name": "fixture/project"},
        "sender": {"type": "User", "login": "maintainer"},
        "issue": {"number": 7, "title": "Fixture", "pull_request": {}},
        "comment": {"id": 42, "body": "@CoRtEx ReViEw",
            "author_association": "MEMBER", "user": {"login": "maintainer"}}
    })
}

#[test]
fn mentions_are_case_insensitive_whole_tokens() {
    assert_eq!(cortex_command("@CoRtEx ReViEw").as_deref(), Some("review"));
    assert_eq!(cortex_command("Please /CORTEX fix").as_deref(), Some("fix"));
    for text in [
        "@cortex-other fix",
        "/cortexevil review",
        "user@cortex",
        "cortex help",
    ] {
        assert!(cortex_command(text).is_none());
    }
}

#[test]
fn untrusted_mismatched_bot_and_fork_events_fail_before_execution() {
    let payload = comment_payload();
    validate_event_authority("issue_comment", &payload, "fixture/project").unwrap();
    for association in ["NONE", "FIRST_TIME_CONTRIBUTOR", "CONTRIBUTOR", ""] {
        let mut untrusted = payload.clone();
        untrusted["comment"]["author_association"] = json!(association);
        assert!(validate_event_authority("issue_comment", &untrusted, "fixture/project").is_err());
    }
    let mut bot = payload.clone();
    bot["sender"]["type"] = json!("Bot");
    assert!(validate_event_authority("issue_comment", &bot, "fixture/project").is_err());
    assert!(validate_event_authority("issue_comment", &payload, "other/project").is_err());
    assert!(validate_event_authority("workflow_dispatch", &payload, "fixture/project").is_err());
    let mut fork = payload;
    fork["pull_request"] = json!({"head": {"repo": {"full_name": "fork/project"}}});
    assert!(validate_event_authority("issue_comment", &fork, "fixture/project").is_err());
}

#[test]
fn planned_comment_identity_is_stable_and_not_a_fake_reply() {
    let payload = comment_payload().to_string();
    let event = parse_event("issue_comment", &payload).unwrap();
    let first = plan_event(&event).unwrap().unwrap();
    let second = plan_event(&event).unwrap().unwrap();
    assert_eq!(first.marker, "<!-- cortex-automation:comment-42 -->");
    assert_eq!(first.marker, second.marker);
    assert_eq!(first.number, 7);
    assert!(first.is_pr);
    assert_eq!(first.request, "@CoRtEx ReViEw");
}

#[test]
fn issues_and_review_events_produce_real_analysis_plans() {
    let issue = json!({"action": "opened", "issue": {
        "number": 3, "title": "CORTEX issue", "body": "fixture", "labels": []
    }});
    let plan = plan_event(&parse_event("issues", &issue.to_string()).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(plan.number, 3);
    assert!(!plan.is_pr);
    let review = json!({"action": "submitted", "pull_request": {"number": 5},
        "review": {"id": 9, "body": "/Cortex explain", "state": "commented"}});
    let plan = plan_event(&parse_event("pull_request_review", &review.to_string()).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(plan.marker, "<!-- cortex-automation:review-9 -->");
}

#[test]
fn generated_command_arguments_match_clap_without_a_token_on_argv() {
    let workflow = generate_workflow(&WorkflowConfig::default());
    let value: serde_yaml::Value = serde_yaml::from_str(&workflow).unwrap();
    let run = value["jobs"]["cortex"]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|step| step["name"].as_str() == Some("Analyze event"))
        .unwrap()["run"]
        .as_str()
        .unwrap();
    // The command has one option/value pair per line. Expand only the exact
    // known Actions variables, without evaluating a shell or contacting GitHub.
    let replacements = [
        ("$GITHUB_EVENT_NAME", "issue_comment"),
        ("$GITHUB_EVENT_PATH", "/tmp/fixture event.json"),
        ("$GITHUB_REPOSITORY", "fixture/project"),
        (
            "$RUNNER_TEMP/cortex-response.md",
            "/tmp/fixture response.md",
        ),
    ];
    let mut argv = vec!["github".to_string(), "run".to_string()];
    for line in run.lines().skip(1) {
        let line = line.trim().trim_end_matches('\\').trim();
        let (flag, value) = line.split_once(' ').unwrap();
        let value = value.trim_matches('"');
        let expanded = replacements
            .iter()
            .find(|(key, _)| *key == value)
            .unwrap()
            .1;
        argv.extend([flag.into(), expanded.into()]);
    }
    let args = GitHubCli::try_parse_from(argv).unwrap();
    let GitHubSubcommand::Run(args) = args.subcommand else {
        panic!("Expected run")
    };
    assert_eq!(
        args.event_path.unwrap(),
        PathBuf::from("/tmp/fixture event.json")
    );
    assert_eq!(
        args.output.unwrap(),
        PathBuf::from("/tmp/fixture response.md")
    );
    assert!(args.token.is_none());
    assert!(!args.publish);
    assert!(workflow.contains("github.event_name == 'pull_request_review'"));
    assert!(workflow.contains("persist-credentials: false"));
    assert!(!workflow.contains("pull_request_target"));
    assert!(!workflow.contains("--token"));
    assert!(!workflow.contains(": write"));
}

#[test]
fn workflow_names_cannot_escape_paths_or_inject_yaml() {
    for name in [
        "../outside",
        "/tmp/out",
        "name\njobs:",
        "with spaces",
        ".hidden",
        "",
    ] {
        assert!(validate_workflow_name(name).is_err());
    }
    validate_workflow_name("cortex-review").unwrap();
    let workflow = generate_workflow(&WorkflowConfig {
        name: "quoted:\nname".into(),
        ..Default::default()
    });
    let value: serde_yaml::Value = serde_yaml::from_str(&workflow).unwrap();
    assert_eq!(value["name"].as_str().unwrap(), "quoted:\nname");
}

#[test]
fn pull_request_events_plan_only_for_actionable_reviewable_commits() {
    let event = |action: &str, draft: bool, sha: &str, number: u64| {
        parse_event(
            "pull_request",
            &json!({"action": action, "pull_request": {
                "number": number, "title": "Fixture", "body": null, "draft": draft,
                "user": {"login": "maintainer"}, "labels": [],
                "head": {"ref": "topic", "sha": sha}, "base": {"ref": "main", "sha": "b"}
            }})
            .to_string(),
        )
        .unwrap()
    };
    let plan = plan_event(&event("opened", false, "abc123", 4))
        .unwrap()
        .unwrap();
    assert_eq!(plan.number, 4);
    assert!(plan.is_pr);
    assert_eq!(plan.expected_sha.as_deref(), Some("abc123"));
    assert_eq!(plan.marker, "<!-- cortex-automation:pr-4-abc123 -->");
    // Drafts and closures are not requests for analysis.
    assert!(
        plan_event(&event("opened", true, "abc123", 4))
            .unwrap()
            .is_none()
    );
    assert!(
        plan_event(&event("closed", false, "abc123", 4))
            .unwrap()
            .is_none()
    );
    // A commit ID that is missing or not hexadecimal cannot be pinned to a diff.
    for sha in ["", "not-a-sha"] {
        assert!(
            plan_event(&event("opened", false, sha, 4)).is_err(),
            "{sha}"
        );
    }
    // A zero object ID would address the wrong conversation.
    assert!(plan_event(&event("opened", false, "abc123", 0)).is_err());
}

#[test]
fn issue_events_require_an_explicit_cortex_request() {
    let event = |title: &str, body: &str, labels: serde_json::Value, action: &str| {
        parse_event(
            "issues",
            &json!({"action": action, "issue": {"number": 11, "title": title,
                "body": body, "user": {"login": "maintainer"}, "labels": labels}})
            .to_string(),
        )
        .unwrap()
    };
    for (title, body, labels) in [
        ("Cortex please look", "unrelated", json!([])),
        ("unrelated", "ask CORTEX for help", json!([])),
        ("unrelated", "unrelated", json!([{"name": "Cortex:review"}])),
    ] {
        let plan = plan_event(&event(title, body, labels, "opened"))
            .unwrap()
            .unwrap();
        assert_eq!(plan.number, 11);
        assert!(!plan.is_pr);
        assert_eq!(plan.marker, "<!-- cortex-automation:issue-11 -->");
    }
    // No mention, an unrelated label, or an unhandled action starts nothing.
    assert!(
        plan_event(&event("bug", "bug", json!([{"name": "triage"}]), "opened"))
            .unwrap()
            .is_none()
    );
    assert!(
        plan_event(&event("Cortex", "Cortex", json!([]), "closed"))
            .unwrap()
            .is_none()
    );
    assert!(plan_event(&parse_event("deployment", "{}").unwrap()).is_err());
}

#[test]
fn only_read_only_commands_are_accepted() {
    for command in ["help", "fix", "explain", "test"] {
        validate_command(command, false).unwrap();
        validate_command(command, true).unwrap();
    }
    validate_command("review", true).unwrap();
    assert!(validate_command("review", false).is_err());
    for command in ["merge", "push", "deploy", "approve", ""] {
        assert!(validate_command(command, true).is_err(), "{command}");
    }
    // A bare mention defaults to the read-only help command.
    assert_eq!(cortex_command("@cortex").as_deref(), Some("help"));
}

#[test]
fn saved_responses_are_new_private_files_that_never_replace_existing_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("response.md");
    save_response(&path, "analysis body").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "analysis body");
    // A second save must fail rather than overwrite the earlier analysis.
    assert!(save_response(&path, "replacement").is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "analysis body");
    assert!(save_response(&dir.path().join("missing/response.md"), "x").is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn events_are_read_from_the_environment_within_a_bounded_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("event.json");
    std::fs::write(&path, comment_payload().to_string()).unwrap();
    let event =
        load_authorized_event("issue_comment", Some(path.clone()), "fixture/project").unwrap();
    assert_eq!(plan_event(&event).unwrap().unwrap().number, 7);
    // The repository named on the command line is authoritative.
    assert!(load_authorized_event("issue_comment", Some(path.clone()), "other/project").is_err());
    // Without a path and without the Actions variable there is no event at all.
    assert!(load_authorized_event("issue_comment", None, "fixture/project").is_err());

    let oversized = dir.path().join("oversized.json");
    std::fs::write(
        &oversized,
        format!("{{\"padding\":\"{}\"}}", "p".repeat(1024 * 1024)),
    )
    .unwrap();
    let error = load_authorized_event("issue_comment", Some(oversized), "fixture/project")
        .unwrap_err()
        .to_string();
    assert!(error.contains("too large"), "{error}");

    let invalid = dir.path().join("invalid.json");
    std::fs::write(&invalid, "{ not json").unwrap();
    assert!(load_authorized_event("issue_comment", Some(invalid), "fixture/project").is_err());
    assert!(
        load_authorized_event("issue_comment", Some(dir.path().join("absent")), "a/b").is_err()
    );
}

#[test]
fn blank_option_values_are_not_accepted_as_credentials_or_repositories() {
    // The environment fallback itself is exercised by the CLI subprocess tests,
    // which can set Actions variables without mutating this process.
    assert_eq!(
        option_or_env(Some("explicit".into()), "CORTEX_TEST_UNSET_VARIABLE").as_deref(),
        Some("explicit")
    );
    for blank in ["", "   ", "\t\n"] {
        assert_eq!(
            option_or_env(Some(blank.into()), "CORTEX_TEST_UNSET_VARIABLE"),
            None
        );
    }
    assert_eq!(option_or_env(None, "CORTEX_TEST_UNSET_VARIABLE"), None);
}

#[cfg(unix)]
#[test]
fn workflow_mutations_reject_symlinked_directories_and_files() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), root.path().join(".github")).unwrap();
    assert!(checked_workflows_dir(root.path()).is_err());
    let file = outside.path().join("workflow.yml");
    std::fs::write(&file, "controlled existing content").unwrap();
    let link = root.path().join("link.yml");
    symlink(&file, &link).unwrap();
    assert!(reject_workflow_symlink(&link).is_err());
    assert_eq!(
        std::fs::read_to_string(file).unwrap(),
        "controlled existing content"
    );
}
