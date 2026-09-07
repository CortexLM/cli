//! End-to-end `cortex run` behaviour against an unreachable loopback endpoint.
//!
//! A run whose turn cannot complete must fail, and its JSON document must say
//! so. Nothing here contacts a real service and no successful turn is faked.

use std::io::Write;
use std::process::{Command, Output, Stdio};

/// Loopback port 1 is reserved and never listening.
const UNREACHABLE: &str = "http://127.0.0.1:1";

struct Run {
    home: tempfile::TempDir,
}

impl Run {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
        }
    }

    fn exec(&self, args: &[&str], stdin: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_Cortex"))
            .arg("run")
            .arg("--cwd")
            .arg(self.home.path())
            .args(args)
            .env("HOME", self.home.path())
            .env("CORTEX_HOME", self.home.path())
            .env("CORTEX_API_KEY", "offline-fixture")
            .env("CORTEX_API_URL", UNREACHABLE)
            .env("RUST_LOG", "off")
            .current_dir(self.home.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

fn json_result(stdout: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(stdout);
    let start = text
        .find("{\n  \"complete\"")
        .or_else(|| text.find('{'))
        .expect("a JSON result document is emitted");
    serde_json::from_str(text[start..].trim()).expect("the result document is valid JSON")
}

#[test]
fn an_unreachable_service_makes_run_fail_and_marks_the_result_unsuccessful() {
    let run = Run::new();
    let output = run.exec(&["--format", "json", "--timeout", "20", "hello"], "");

    assert!(
        !output.status.success(),
        "an incomplete turn must exit non-zero; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = json_result(&output.stdout);
    assert_eq!(result["type"], "result");
    assert_eq!(
        result["success"], false,
        "a turn that never completed is never a success: {result}"
    );
    assert_eq!(result["complete"], false);
    assert_ne!(
        result["finish_reason"], "stop",
        "a failed run must not claim a normal stop: {result}"
    );
    assert!(
        result["session_id"]
            .as_str()
            .unwrap()
            .parse::<uuid::Uuid>()
            .is_ok()
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("did not complete")
            || String::from_utf8_lossy(&output.stderr).contains("Error"),
        "the user must be told the run failed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn jsonl_streams_every_event_and_still_ends_in_a_failure() {
    let run = Run::new();
    let output = run.exec(&["--format", "jsonl", "--timeout", "20", "hello"], "");
    assert!(!output.status.success());

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with('{'))
        .map(|line| serde_json::from_str(line).expect("each JSONL line is one object"))
        .collect();
    assert!(!events.is_empty(), "JSONL mode must emit event lines");
    for event in &events {
        assert!(event["event"]["type"].is_string(), "{event}");
        assert!(event["session_id"].is_string());
        assert!(event["event_id"].as_u64().is_some());
    }
    assert!(
        events
            .iter()
            .any(|event| event["event"]["type"] == "task_started"),
        "the submitted turn must be visible in the stream: {events:#?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| event["event"]["type"] == "task_complete"),
        "an unreachable service must not produce a completed task: {events:#?}"
    );
}

#[test]
fn a_run_without_a_message_or_command_is_refused() {
    let run = Run::new();
    let output = run.exec(&[], "");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must provide a message or a --command"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn an_empty_command_is_refused_before_any_session_is_created() {
    let run = Run::new();
    let output = run.exec(&["--command", "   "], "");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Command cannot be empty"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unsupported_generation_options_fail_before_a_turn_is_submitted() {
    let run = Run::new();
    for (flag, value) in [
        ("--temperature", "0.5"),
        ("--top-p", "0.9"),
        ("--top-k", "5"),
        ("--seed", "1"),
        ("--max-tokens", "10"),
        ("--retry", "2"),
    ] {
        let output = run.exec(&[flag, value, "hello"], "");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{flag} must be rejected");
        assert!(
            stderr.contains("No turn was submitted"),
            "{flag} must be rejected before submission; stderr: {stderr}"
        );
    }
    for flag in ["--no-cache", "--share"] {
        let output = run.exec(&[flag, "hello"], "");
        assert!(!output.status.success(), "{flag} must be rejected");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("No turn was submitted"),
            "{flag} must be rejected before submission"
        );
    }
    let output = run.exec(&["--attach", UNREACHABLE, "hello"], "");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("No local session was started"),
        "--attach must not silently start a local session"
    );
}

#[test]
fn a_missing_attachment_fails_instead_of_sending_a_message_without_it() {
    let run = Run::new();
    let output = run.exec(&["--file", "absent.txt", "summarise"], "");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("File not found"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn continuing_a_session_that_does_not_exist_is_an_error_not_a_new_session() {
    let run = Run::new();
    let output = run.exec(&["--continue", "hello"], "");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No sessions found"),
        "a missing session must never be replaced by a fresh one; stderr: {stderr}"
    );

    let output = run.exec(
        &["--session", "00000000-0000-4000-8000-000000000000", "hello"],
        "",
    );
    assert!(!output.status.success());
}

#[test]
fn dry_run_estimates_tokens_without_starting_a_turn() {
    let run = Run::new();
    std::fs::write(run.home.path().join("note.txt"), "attached content\n").unwrap();
    let output = run.exec(
        &[
            "--dry-run",
            "--format",
            "json",
            "--file",
            "note.txt",
            "count me",
        ],
        "",
    );

    assert!(
        output.status.success(),
        "a dry run needs no service; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["attachment_count"], 1);
    assert_eq!(value["message_preview"], "count me");
    let estimates = &value["token_estimates"];
    assert!(estimates["user_prompt"].as_u64().unwrap() > 0);
    assert!(estimates["attachments"].as_u64().unwrap() > 0);
    assert_eq!(
        estimates["total_input"].as_u64().unwrap(),
        estimates["user_prompt"].as_u64().unwrap()
            + estimates["attachments"].as_u64().unwrap()
            + estimates["system_prompt"].as_u64().unwrap()
            + estimates["tool_definitions"].as_u64().unwrap(),
        "the total must be the sum of its parts: {estimates}"
    );
}

#[test]
fn a_long_dry_run_message_is_previewed_rather_than_reproduced_in_full() {
    let run = Run::new();
    let message = "w".repeat(400);
    let output = run.exec(&["--dry-run", "--format", "json", &message], "");
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let preview = value["message_preview"].as_str().unwrap();
    assert!(preview.ends_with("..."), "preview: {preview}");
    assert_eq!(preview.len(), 103);

    // The human-readable dry run truncates at a different, longer boundary.
    let output = run.exec(&["--dry-run", &message], "");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Dry Run - Token Estimate"), "{stdout}");
    assert!(
        stdout.contains(&format!("  {}...", "w".repeat(200))),
        "{stdout}"
    );
}

#[test]
fn piped_stdin_is_appended_to_the_message_arguments() {
    let run = Run::new();
    let output = run.exec(&["--dry-run", "--format", "json", "lead"], "piped tail");
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["message_preview"], "lead\npiped tail");
}

#[test]
fn a_failed_run_still_writes_the_requested_output_file() {
    let run = Run::new();
    let target = run.home.path().join("nested/response.txt");
    let output = run.exec(
        &[
            "--timeout",
            "20",
            "--output-file",
            target.to_str().unwrap(),
            "hello",
        ],
        "",
    );
    assert!(!output.status.success(), "the turn still failed");
    assert!(
        target.exists(),
        "the parent directory and file must be created even for a failed turn"
    );
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "",
        "no text arrived, so no text may be invented"
    );
}
