//! End-to-end `cortex exec` runs against an unreachable loopback endpoint.
//!
//! These prove the product rule: a turn that cannot complete is reported as a
//! failure on both the process exit status and the machine-readable result.
//! Nothing here contacts a real service and no successful turn is simulated.

use std::io::Write;
use std::process::{Command, Output, Stdio};

/// Loopback port 1 is reserved and never listening: the request cannot leave
/// the machine and cannot succeed.
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

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_Cortex"));
        command
            .arg("exec")
            .arg("--cwd")
            .arg(self.home.path())
            .args(args)
            .env("HOME", self.home.path())
            .env("CORTEX_HOME", self.home.path())
            .env("CORTEX_API_KEY", "offline-fixture")
            .env("CORTEX_API_URL", UNREACHABLE)
            .env_remove("CORTEX_COMPUTER")
            .env_remove("CORTEX_SSH_HOST")
            .env_remove("CORTEX_SSH_TARGET")
            .env("RUST_LOG", "off")
            .current_dir(self.home.path());
        command
    }

    fn exec(&self, args: &[&str], stdin: &str) -> Output {
        let mut child = self
            .command(args)
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

fn frames(stdout: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(stdout)
        .lines()
        .filter(|line| line.starts_with('{'))
        .map(|line| serde_json::from_str(line).expect("each protocol line is one JSON object"))
        .collect()
}

#[test]
fn default_runtime_reaches_the_api_instead_of_refusing_this_pc() {
    let run = Run::new();
    let output = run.exec(&["--output-format", "json", "--timeout", "20", "hello"], "");
    assert!(!output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let error = result["error"].as_str().unwrap_or_default();
    assert!(
        !error.contains("already connected Code session"),
        "unset CORTEX_COMPUTER must not refuse as This PC: {result}"
    );
    assert!(
        error.contains("temporarily unavailable"),
        "default Cloud path must reach the coding service: {result}"
    );
}

#[test]
fn this_pc_without_a_session_refuses_with_product_copy() {
    let run = Run::new();
    let mut command = run.command(&["--output-format", "json", "--timeout", "20", "hello"]);
    command.env("CORTEX_COMPUTER", "this_pc");
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let error = result["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("This PC") && error.contains("already connected Code session"),
        "This PC without a session must use product copy: {result}"
    );
    assert!(
        error.contains("CORTEX_COMPUTER"),
        "the refuse path must name the explicit override: {result}"
    );
    assert!(!error.to_lowercase().contains("reqwest"), "{result}");
}

#[test]
fn an_unreachable_service_fails_the_run_and_reports_an_error_result_not_a_success() {
    let run = Run::new();
    let output = run.exec(&["--output-format", "json", "--timeout", "20", "hello"], "");

    assert!(
        !output.status.success(),
        "an incomplete turn must exit non-zero; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("a JSON result document is always emitted");
    assert_eq!(result["type"], "result");
    assert_eq!(result["subtype"], "error");
    assert_eq!(result["is_error"], true);
    assert!(
        result.get("result").is_none(),
        "a failed run must not carry a result payload: {result}"
    );
    assert!(
        result["error"].as_str().is_some_and(|e| !e.is_empty()),
        "the failure must be explained: {result}"
    );
    assert!(
        result["session_id"]
            .as_str()
            .unwrap()
            .parse::<uuid::Uuid>()
            .is_ok()
    );
}

#[test]
fn the_stream_json_completion_of_a_failed_run_is_not_marked_successful() {
    let run = Run::new();
    let output = run.exec(
        &["--output-format", "stream-json", "--timeout", "20", "hello"],
        "",
    );

    assert!(!output.status.success());
    let frames = frames(&output.stdout);
    let init = frames.first().expect("an init event opens the stream");
    assert_eq!(init["type"], "system");
    assert_eq!(init["subtype"], "init");

    let completion = frames
        .last()
        .expect("a completion object closes the stream");
    assert_eq!(completion["type"], "completion");
    assert_eq!(
        completion["success"], false,
        "an incomplete turn is never a successful completion: {completion}"
    );
    assert!(completion["error"].as_str().is_some_and(|e| !e.is_empty()));
    assert_eq!(completion["session_id"], init["session_id"]);
}

#[test]
fn a_zero_turn_budget_is_rejected_before_any_submission() {
    let run = Run::new();
    let output = run.exec(&["--max-turns", "0", "hello"], "");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--max-turns must be greater than zero"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_started_turn_is_counted_in_the_failed_result() {
    let run = Run::new();
    let output = run.exec(
        &["--output-format", "json", "--max-turns", "1", "hello"],
        "",
    );
    assert!(!output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["is_error"], true);
    assert_eq!(
        result["num_turns"], 1,
        "the turn that started must be counted even though it failed: {result}"
    );
}

#[test]
fn an_empty_prompt_is_refused_rather_than_submitted() {
    let run = Run::new();
    let output = run.exec(&[], "   \n");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("No prompt provided"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn piped_stdin_becomes_the_prompt_and_echo_reports_the_composed_text() {
    let run = Run::new();
    let output = run.exec(
        &[
            "--echo",
            "--output-format",
            "json",
            "--timeout",
            "20",
            "lead",
        ],
        "piped tail",
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--- Prompt ---\nlead\npiped tail\n--- End Prompt ---"),
        "argument text must precede piped stdin; stderr: {stderr}"
    );
    assert!(!output.status.success());
}

#[test]
fn the_protocol_rejects_malformed_requests_and_shuts_down_without_stdin_eof() {
    let run = Run::new();
    let output = run.exec(
        &[
            "--input-format",
            "stream-jsonrpc",
            "--output-format",
            "stream-jsonrpc",
            "--max-turns",
            "4",
            "--timeout",
            "20",
        ],
        concat!(
            r#"{"jsonrpc":"1.0","id":1,"method":"message","params":{"text":"x"}}"#,
            "\n",
            "this is not json\n",
            "\n",
            r#"{"jsonrpc":"2.0","id":3,"method":"no_such_method"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":4,"method":"message","params":{"text":"   "}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":5,"method":"shutdown"}"#,
            "\n",
        ),
    );

    assert!(
        output.status.success(),
        "a clean shutdown is a successful protocol session; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let frames = frames(&output.stdout);
    assert!(frames.iter().all(|frame| frame["jsonrpc"] == "2.0"));
    assert_eq!(frames[0]["method"], "initialized");
    assert!(frames[0]["params"]["session_id"].is_string());

    let answer = |id: i64| {
        frames
            .iter()
            .find(|frame| frame["id"] == id)
            .unwrap_or_else(|| panic!("no answer for request {id}"))
    };
    assert_eq!(answer(1)["error"]["code"], -32600);
    assert_eq!(answer(1)["error"]["message"], "jsonrpc must be 2.0.");
    assert_eq!(answer(3)["error"]["code"], -32601);
    assert_eq!(
        answer(4)["error"]["code"],
        -32602,
        "an empty message is never an accepted turn"
    );
    assert_eq!(answer(5)["result"]["shutdown_requested"], true);

    let parse_error = frames
        .iter()
        .find(|frame| frame["error"]["code"] == -32700)
        .expect("unparsable input is answered, not ignored");
    assert_eq!(parse_error["id"], serde_json::Value::Null);
    assert_eq!(parse_error["error"]["message"], "Invalid JSON-RPC input.");
}

#[test]
fn a_second_turn_is_refused_while_one_is_running_and_the_budget_is_enforced() {
    let run = Run::new();
    let output = run.exec(
        &[
            "--input-format",
            "stream-jsonrpc",
            "--output-format",
            "stream-jsonrpc",
            "--max-turns",
            "1",
            "--timeout",
            "20",
        ],
        concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"message","params":{"text":"first"}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"message","params":{"text":"second"}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":3,"method":"cancel"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":4,"method":"shutdown"}"#,
            "\n",
        ),
    );

    let frames = frames(&output.stdout);
    let answer = |id: i64| {
        frames
            .iter()
            .find(|frame| frame["id"] == id)
            .unwrap_or_else(|| panic!("no answer for request {id}"))
    };
    assert_eq!(answer(1)["result"]["accepted"], true);
    assert_eq!(
        answer(2)["error"]["code"],
        -32000,
        "a concurrent or over-budget turn must be refused: {:?}",
        answer(2)
    );
    assert_eq!(answer(3)["result"]["cancellation_requested"], true);

    // The failing turn surfaces as an error/abort event, never as a completion.
    assert!(
        frames.iter().any(|frame| {
            let event = &frame["params"]["event"];
            event["type"] == "Error" || event["type"] == "TurnAborted"
        }) || frames.iter().any(|frame| {
            let msg = &frame["params"]["msg"];
            msg["type"] == "error" || msg["type"] == "turn_aborted"
        }),
        "the unreachable service must produce a failure event: {frames:#?}"
    );
    assert!(
        frames.iter().any(|frame| {
            frame["params"]["msg"]["type"] == "shutdown_complete"
                || frame["params"]["event"]["type"] == "ShutdownComplete"
        }),
        "shutdown must be acknowledged: {frames:#?}"
    );
}

#[test]
fn listing_tools_needs_no_session_and_honours_the_disabled_filter() {
    let run = Run::new();
    let output = run.exec(
        &[
            "--list-tools",
            "--output-format",
            "json",
            "--disabled-tools",
            "Read",
        ],
        "",
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tools: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!tools.is_empty());
    assert!(
        !tools
            .iter()
            .any(|tool| tool["name"].as_str() == Some("Read")),
        "a disabled tool must not be listed"
    );
}
