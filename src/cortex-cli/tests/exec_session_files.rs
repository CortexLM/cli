//! Import / export / lock / ACP behaviour on a real on-disk session store.
//!
//! Every session used here is created by the product's own import path, so the
//! assertions are about real stored state, not fixtures written behind its back.

use std::io::Write;
use std::process::{Command, Output, Stdio};

const VERSION_1_EXPORT: &str = r#"{
  "version": 1,
  "session": {
    "id": "11111111-1111-4111-8111-111111111111",
    "title": "seed",
    "model": "fixture-model",
    "created_at": "2024-01-01T00:00:00Z",
    "cwd": "/tmp/fixture"
  },
  "messages": [
    {"role": "user", "content": "hello, world", "timestamp": "2024-01-01T00:00:01Z"},
    {"role": "assistant", "content": "line one\nline two", "timestamp": "2024-01-01T00:00:02Z"}
  ]
}"#;

struct Store {
    home: tempfile::TempDir,
}

impl Store {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
        }
    }

    fn cortex(&self, args: &[&str], stdin: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_Cortex"))
            .args(args)
            .env("HOME", self.home.path())
            .env("CORTEX_HOME", self.home.path())
            .env("RUST_LOG", "off")
            .current_dir(self.home.path())
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(stdin) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(stdin.as_bytes())
                .unwrap();
        }
        child.wait_with_output().unwrap()
    }

    /// Import the fixture export and return the id the store assigned.
    fn seed(&self) -> String {
        let path = self.home.path().join("export.json");
        std::fs::write(&path, VERSION_1_EXPORT).unwrap();
        let output = self.cortex(&["import", path.to_str().unwrap()], None);
        assert!(
            output.status.success(),
            "seed import failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        // The confirmation is styled output on stderr; the resume hint is on stdout.
        format!("{}{}", stdout(&output), stderr(&output))
            .lines()
            .find_map(|line| line.split("Imported session as: ").nth(1))
            .expect("the import reports the new id")
            .trim()
            .to_string()
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_version_1_export_is_imported_under_a_fresh_id_that_records_its_origin() {
    let store = Store::new();
    let id = store.seed();
    assert_ne!(
        id, "11111111-1111-4111-8111-111111111111",
        "import must never reuse the source identity"
    );

    let output = store.cortex(&["export", &id], None);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let document: serde_json::Value = serde_json::from_str(stdout(&output).trim()).unwrap();
    assert_eq!(document["version"], 2);
    assert_eq!(document["meta"]["id"], id);
    assert_eq!(
        document["meta"]["forked_from"], "11111111-1111-4111-8111-111111111111",
        "the origin must be recorded: {document}"
    );
    assert_eq!(document["meta"]["protected"], false);
    assert_eq!(document["messages"].as_array().unwrap().len(), 2);
    assert_eq!(document["messages"][0]["content"], "hello, world");
}

#[test]
fn importing_the_same_export_twice_creates_two_distinct_sessions() {
    let store = Store::new();
    let first = store.seed();
    let second = store.seed();
    assert_ne!(first, second, "each import is a separate session");
}

#[test]
fn a_stdin_import_is_accepted_and_a_remote_source_is_refused() {
    let store = Store::new();
    let output = store.cortex(&["import", "-"], Some(VERSION_1_EXPORT));
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("Imported session as:"));

    let output = store.cortex(&["import", "https://example.invalid/session.json"], None);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("Remote session import is unavailable"),
        "import must not fetch over the network: {}",
        stderr(&output)
    );

    let output = store.cortex(&["import", "   "], None);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("Source path cannot be empty"));
}

#[test]
fn a_malformed_or_unsupported_export_is_rejected_instead_of_partially_imported() {
    let store = Store::new();
    let before = std::fs::read_dir(store.home.path().join("sessions"))
        .map(|entries| entries.count())
        .unwrap_or(0);

    for (body, expected) in [
        ("not json at all: [", "Invalid session JSON"),
        (
            r#"{"version":99,"session":{"id":"a","created_at":"2024-01-01T00:00:00Z"},"messages":[]}"#,
            "Unsupported export version",
        ),
        (
            r#"{"version":1,"session":{"id":"a","created_at":"not-a-date"},"messages":[]}"#,
            "premature end of input",
        ),
    ] {
        let path = store.home.path().join("bad.json");
        std::fs::write(&path, body).unwrap();
        let output = store.cortex(&["import", path.to_str().unwrap()], None);
        assert!(!output.status.success(), "must reject: {body}");
        assert!(
            stderr(&output).contains(expected),
            "expected {expected:?} for {body}, got {}",
            stderr(&output)
        );
    }

    let after = std::fs::read_dir(store.home.path().join("sessions"))
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(before, after, "a rejected import must store nothing");
}

#[test]
fn a_version_2_document_round_trips_through_export_and_import() {
    let store = Store::new();
    let id = store.seed();
    let exported = store.cortex(&["export", &id, "--format", "yaml"], None);
    assert!(exported.status.success(), "stderr: {}", stderr(&exported));

    let path = store.home.path().join("round-trip.yaml");
    std::fs::write(&path, stdout(&exported)).unwrap();
    let output = store.cortex(&["import", path.to_str().unwrap()], None);
    assert!(
        output.status.success(),
        "a YAML v2 document must import: {}",
        stderr(&output)
    );
}

#[test]
fn csv_export_escapes_content_and_file_export_never_overwrites() {
    let store = Store::new();
    let id = store.seed();

    let output = store.cortex(&["export", &id, "--format", "csv"], None);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let csv = stdout(&output);
    assert!(csv.starts_with("timestamp,role,content\n"));
    assert!(
        csv.contains("\"hello, world\""),
        "a comma in content must be quoted: {csv}"
    );
    assert!(
        csv.contains("\"line one\nline two\""),
        "a newline in content must be quoted: {csv}"
    );

    let target = store.home.path().join("export-out.json");
    let output = store.cortex(&["export", &id, "--output", target.to_str().unwrap()], None);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let written = std::fs::read_to_string(&target).unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&written).is_ok());

    let output = store.cortex(&["export", &id, "--output", target.to_str().unwrap()], None);
    assert!(
        !output.status.success(),
        "an existing export must never be overwritten"
    );
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        written,
        "the original export must be untouched"
    );
}

#[test]
fn exporting_without_a_session_id_or_for_an_unknown_id_fails() {
    let store = Store::new();
    let output = store.cortex(&["export"], None);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("Specify the session ID to export"));

    let output = store.cortex(&["export", "00000000-0000-4000-8000-000000000000"], None);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("Session not found"));
}

#[test]
fn locking_then_unlocking_a_session_changes_what_check_reports() {
    let store = Store::new();
    let id = store.seed();

    let output = store.cortex(&["lock", "check", &id], None);
    assert!(
        !output.status.success(),
        "a fresh session must not be reported as locked"
    );
    assert!(stderr(&output).contains("is not locked"));

    let output = store.cortex(&["lock", "add", &id, "--reason", "keep this"], None);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("Locked 1 session(s)"));

    let output = store.cortex(&["lock", "check", &id], None);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("is LOCKED"));

    let output = store.cortex(&["lock", "list", "--json"], None);
    let entries: Vec<serde_json::Value> = serde_json::from_str(stdout(&output).trim()).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["session_id"], id);
    assert_eq!(entries[0]["reason"], "keep this");

    let output = store.cortex(&["lock", "list"], None);
    assert!(output.status.success());
    assert!(
        stdout(&output).contains("Reason: keep this"),
        "{}",
        stdout(&output)
    );
    assert!(stdout(&output).contains(&id[..8]));

    // Locking again is idempotent, not a duplicate entry.
    let output = store.cortex(&["lock", "add", &id], None);
    assert!(output.status.success());
    assert!(stdout(&output).contains("already locked"));

    let output = store.cortex(&["lock", "remove", &id, "--yes"], None);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("Unlocked 1 session(s)"));

    let output = store.cortex(&["lock", "check", &id], None);
    assert!(
        !output.status.success(),
        "the session must no longer be locked"
    );
    let output = store.cortex(&["lock", "list"], None);
    assert!(stdout(&output).contains("No sessions are locked"));
}

#[test]
fn unlocking_without_confirmation_is_refused_when_there_is_no_terminal() {
    let store = Store::new();
    let id = store.seed();
    assert!(store.cortex(&["lock", "add", &id], None).status.success());

    // stdin is /dev/null here: not a terminal, so there is nobody to prompt.
    let output = store.cortex(&["lock", "remove", &id], None);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("Unlock requires --yes without an interactive terminal"),
        "stderr: {}",
        stderr(&output)
    );

    let output = store.cortex(&["lock", "check", &id], None);
    assert!(
        output.status.success(),
        "the refused unlock must leave the lock in place"
    );
}

#[test]
fn a_malformed_session_id_is_rejected_before_the_lock_file_is_touched() {
    let store = Store::new();
    let output = store.cortex(&["lock", "add", "not-a-uuid"], None);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("Invalid session ID"),
        "{}",
        stderr(&output)
    );
    assert!(
        !store.home.path().join("session_locks.json").exists(),
        "a rejected lock must not create a lock file"
    );

    let output = store.cortex(
        &["lock", "check", "00000000-0000-4000-8000-000000000000"],
        None,
    );
    assert!(!output.status.success());
    assert!(stderr(&output).contains("Session not found"));
}

#[test]
fn acp_rejects_network_transport_and_unsupported_tool_controls() {
    let store = Store::new();
    for args in [
        vec!["acp", "--port", "8080"],
        vec!["acp", "--host", "0.0.0.0"],
    ] {
        let output = store.cortex(&args, None);
        assert!(!output.status.success(), "{args:?} must be refused");
        assert!(
            stderr(&output).contains("ACP network transport is unsupported"),
            "stderr: {}",
            stderr(&output)
        );
    }
    for args in [
        vec!["acp", "--agent", "some-agent"],
        vec!["acp", "--allow-tool", "Read"],
        vec!["acp", "--deny-tool", "Write"],
    ] {
        let output = store.cortex(&args, None);
        assert!(!output.status.success(), "{args:?} must be refused");
        assert!(
            stderr(&output).contains("tool allow/deny controls are not supported"),
            "stderr: {}",
            stderr(&output)
        );
    }
}

#[test]
fn the_acp_stdio_server_starts_and_stops_cleanly_on_end_of_input() {
    let store = Store::new();
    // Empty stdin is immediate EOF: the server must start and exit, not hang.
    let output = store.cortex(&["acp", "--stdio"], Some(""));
    assert!(
        stderr(&output).contains("Starting ACP server on stdio transport"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        output.status.success(),
        "a clean EOF is not a failure; stderr: {}",
        stderr(&output)
    );
}
