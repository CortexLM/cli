//! Offline unit tests for prompt composition and terminal-result emission.
//! No live service, no simulated model turn.
use super::*;
use clap::Parser;
use cortex_engine::Session;
use cortex_engine::client::{
    CompletionRequest, CompletionResponse, FinishReason, ModelCapabilities, ModelClient,
    ResponseEvent, ResponseStream, ToolCallEvent,
};
use cortex_protocol::{AgentMessageEvent, ErrorEvent};
use std::future::Future;
use std::pin::Pin;

fn cli(args: &[&str]) -> ExecCli {
    let mut argv = vec!["exec"];
    argv.extend_from_slice(args);
    ExecCli::try_parse_from(argv).expect("exec arguments parse")
}

/// What the fixture transport does when the engine asks it for a completion.
#[derive(Clone, Copy)]
enum Behaviour {
    /// Ask the client to run one dangerous shell command.
    RequestDangerousCommand,
    /// Answer with plain text, which the engine turns into a completed task.
    AnswerWithText,
    /// Fail the turn the way an unavailable service does.
    Fail,
}

/// A transport with no network behind it. It never fabricates a terminal
/// event: the engine derives those from what this actually returns.
struct Fixture {
    behaviour: Behaviour,
    capabilities: ModelCapabilities,
}

impl Fixture {
    fn session(
        behaviour: Behaviour,
        home: &std::path::Path,
    ) -> (Session, cortex_engine::SessionHandle) {
        Session::with_client(
            cortex_engine::Config {
                cwd: home.into(),
                cortex_home: home.join("state"),
                ..Default::default()
            },
            Box::new(Self {
                behaviour,
                capabilities: ModelCapabilities::default(),
            }),
        )
        .unwrap()
    }
}

async fn submit_text(handle: &cortex_engine::SessionHandle, text: &str) {
    handle
        .submission_tx
        .send(Submission {
            id: uuid::Uuid::new_v4().to_string(),
            op: Op::UserInput {
                items: vec![UserInput::Text { text: text.into() }],
            },
        })
        .await
        .unwrap();
}

// The async-trait ABI is spelled out because this crate carries no such macro.
impl ModelClient for Fixture {
    fn model(&self) -> &str {
        "cortex-1-mini"
    }
    fn provider(&self) -> &str {
        "runner-fixture"
    }
    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }
    fn complete<'life0, 'async_trait>(
        &'life0 self,
        _: CompletionRequest,
    ) -> Pin<Box<dyn Future<Output = cortex_engine::Result<ResponseStream>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let behaviour = self.behaviour;
        Box::pin(async move {
            let mut done = CompletionResponse::default();
            let events = match behaviour {
                Behaviour::RequestDangerousCommand => {
                    done.finish_reason = FinishReason::ToolCalls;
                    vec![
                        Ok(ResponseEvent::ToolCall(ToolCallEvent {
                            id: "call-1".into(),
                            name: "Execute".into(),
                            arguments: r#"{"command":["rm","-rf","/"]}"#.into(),
                            remote: false,
                        })),
                        Ok(ResponseEvent::Done(done)),
                    ]
                }
                Behaviour::AnswerWithText => vec![
                    Ok(ResponseEvent::Delta("an answer".into())),
                    Ok(ResponseEvent::Done(done)),
                ],
                // An ended stream with no completion: the engine must treat
                // this as incomplete rather than as an empty success.
                Behaviour::Fail => vec![],
            };
            Ok(Box::pin(futures::stream::iter(events)) as ResponseStream)
        })
    }
    fn complete_sync<'life0, 'async_trait>(
        &'life0 self,
        _: CompletionRequest,
    ) -> Pin<
        Box<dyn Future<Output = cortex_engine::Result<CompletionResponse>> + Send + 'async_trait>,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async { unreachable!("This fixture is streaming-only") })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_denied_unattended_approval_fails_the_run_and_is_never_reported_as_success() {
    let temp = tempfile::tempdir().unwrap();
    let (mut session, handle) = Fixture::session(Behaviour::RequestDangerousCommand, temp.path());
    let session_task = tokio::spawn(async move { session.run().await });
    submit_text(&handle, "clean up").await;

    // Unattended: no sandbox assessment accompanies the request, so the only
    // safe decision is to deny it and stop.
    let cli = cli(&["--output-format", "json", "--timeout", "20", "clean up"]);
    let error = cli
        .process_events(&handle, Instant::now(), Some(AutonomyLevel::High))
        .await
        .expect_err("a denied approval must fail the run");
    assert_eq!(
        error.to_string(),
        "Approval required; the unattended command was denied."
    );

    let _ = cortex_engine::session::control::stop_session(&handle, session_task).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_completed_turn_is_the_only_case_that_succeeds() {
    let temp = tempfile::tempdir().unwrap();
    let (mut session, handle) = Fixture::session(Behaviour::AnswerWithText, temp.path());
    let session_task = tokio::spawn(async move { session.run().await });
    submit_text(&handle, "answer me").await;

    cli(&["--output-format", "json", "--timeout", "20", "answer me"])
        .process_events(&handle, Instant::now(), None)
        .await
        .expect("a task the engine completed is a success");

    let _ = cortex_engine::session::control::stop_session(&handle, session_task).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stream_that_ends_without_completing_fails_with_product_facing_copy() {
    let temp = tempfile::tempdir().unwrap();
    let (mut session, handle) = Fixture::session(Behaviour::Fail, temp.path());
    let session_task = tokio::spawn(async move { session.run().await });
    submit_text(&handle, "answer me").await;

    let error = cli(&["--timeout", "20", "answer me"])
        .process_events(&handle, Instant::now(), None)
        .await
        .expect_err("an incomplete stream is never a success");
    let message = error.to_string();
    assert!(
        message.contains("incomplete") || message.contains("temporarily unavailable"),
        "the user must be told the task did not finish: {message}"
    );
    for internal in ["reqwest", "hyper", "openai", "anthropic"] {
        assert!(
            !message.to_lowercase().contains(internal),
            "no provider or transport internals may leak: {message}"
        );
    }

    let _ = cortex_engine::session::control::stop_session(&handle, session_task).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_budget_of_zero_user_turns_stops_the_run_at_the_first_turn() {
    let temp = tempfile::tempdir().unwrap();
    let (mut session, handle) = Fixture::session(Behaviour::AnswerWithText, temp.path());
    let session_task = tokio::spawn(async move { session.run().await });
    submit_text(&handle, "answer me").await;

    // `--max-turns` is validated as non-zero on the CLI, but the loop must also
    // enforce the budget it is given: the first TaskStarted already exceeds it.
    let mut cli = cli(&["--timeout", "20", "answer me"]);
    cli.max_turns = 0;
    let error = cli
        .process_events(&handle, Instant::now(), None)
        .await
        .expect_err("an exhausted turn budget must stop the run");
    assert_eq!(error.to_string(), "Max turns (0) exceeded");

    let _ = cortex_engine::session::control::stop_session(&handle, session_task).await;
}

fn emit(cli: &ExecCli, outcome: &RunOutcome) -> String {
    let session = ConversationId::new();
    let mut buffer = Vec::new();
    cli.emit_result(&mut buffer, &session, outcome, 42)
        .expect("terminal result is written");
    String::from_utf8(buffer).expect("terminal result is UTF-8")
}

#[tokio::test]
async fn prompt_file_precedes_arguments_and_suffix_closes_the_prompt() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("prompt.txt"), "from file").unwrap();
    let built = cli(&[
        "--cwd",
        dir.path().to_str().unwrap(),
        "--file",
        "prompt.txt",
        "--suffix",
        "TAIL",
        "from",
        "args",
    ])
    .build_prompt()
    .await
    .unwrap();

    assert_eq!(
        built,
        "from file\nfrom args\n\n[Suffix for completion insertion: TAIL]"
    );
}

#[tokio::test]
async fn a_relative_prompt_file_is_resolved_against_cwd_and_a_missing_one_fails() {
    let dir = tempfile::tempdir().unwrap();
    let error = cli(&[
        "--cwd",
        dir.path().to_str().unwrap(),
        "--file",
        "absent.txt",
    ])
    .build_prompt()
    .await
    .expect_err("a missing prompt file must not be silently ignored");

    assert!(
        error.to_string().contains("Failed to read prompt file"),
        "unexpected message: {error}"
    );
    assert!(
        error.to_string().contains(dir.path().to_str().unwrap()),
        "the reported path must be the cwd-resolved one: {error}"
    );
}

#[tokio::test]
async fn an_unfetchable_url_is_skipped_and_never_substituted_with_other_content() {
    let dir = tempfile::tempdir().unwrap();
    // Reserved port 1 on loopback: no request leaves the machine and none succeeds.
    let built = cli(&[
        "--cwd",
        dir.path().to_str().unwrap(),
        "--url",
        "http://127.0.0.1:1/unreachable",
        "keep going",
    ])
    .build_prompt()
    .await
    .unwrap();

    assert_eq!(
        built, "keep going",
        "an unavailable URL must contribute no section at all"
    );
    assert!(!built.contains("--- Content from"));
}

#[tokio::test]
async fn context_sections_precede_the_instruction_and_carry_git_diff() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "fixture@example.invalid"],
        vec!["config", "user.name", "fixture"],
    ] {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::write(repo.join("tracked.txt"), "one\n").unwrap();
    for args in [vec!["add", "-A"], vec!["commit", "-qm", "seed"]] {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::write(repo.join("tracked.txt"), "two\n").unwrap();

    let built = cli(&["--cwd", repo.to_str().unwrap(), "--git-diff", "review this"])
        .build_prompt()
        .await
        .unwrap();

    assert!(built.starts_with("--- Git Diff ---"), "got: {built}");
    assert!(built.contains("tracked.txt"));
    assert!(
        built.ends_with("review this"),
        "the instruction must come last: {built}"
    );
}

#[tokio::test]
async fn include_patterns_contribute_matching_file_content_before_the_instruction() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("kept.rs"), "fn kept() {}\n").unwrap();
    std::fs::write(dir.path().join("other.md"), "not rust\n").unwrap();

    let built = cli(&[
        "--cwd",
        dir.path().to_str().unwrap(),
        "--include",
        "*.rs",
        "explain",
    ])
    .build_prompt()
    .await
    .unwrap();

    assert!(built.contains("kept.rs"), "got: {built}");
    assert!(!built.contains("not rust"));
    assert!(built.ends_with("explain"));
}

#[test]
fn a_json_result_for_a_failed_run_is_an_error_and_never_reports_the_partial_text() {
    let outcome = RunOutcome {
        final_message: "partial answer".into(),
        num_turns: 3,
        error_occurred: true,
        task_completed: false,
        error_message: Some("Max turns (3) exceeded".into()),
        tool_calls_count: 1,
    };
    let value: serde_json::Value =
        serde_json::from_str(&emit(&cli(&["--output-format", "json", "x"]), &outcome)).unwrap();

    assert_eq!(value["type"], "result");
    assert_eq!(value["subtype"], "error");
    assert_eq!(value["is_error"], true);
    assert_eq!(value["error"], "Max turns (3) exceeded");
    assert_eq!(value["num_turns"], 3);
    assert!(
        value.get("result").is_none(),
        "a failed run must not publish a result field: {value}"
    );
    assert!(
        value["session_id"]
            .as_str()
            .unwrap()
            .parse::<uuid::Uuid>()
            .is_ok()
    );
}

#[test]
fn a_json_result_reports_success_only_for_a_completed_turn() {
    let outcome = RunOutcome {
        final_message: "done".into(),
        num_turns: 1,
        task_completed: true,
        ..Default::default()
    };
    let value: serde_json::Value =
        serde_json::from_str(&emit(&cli(&["--output-format", "json", "x"]), &outcome)).unwrap();

    assert_eq!(value["subtype"], "success");
    assert_eq!(value["is_error"], false);
    assert_eq!(value["result"], "done");
    assert_eq!(value["duration_ms"], 42);
}

#[test]
fn a_stream_completion_is_unsuccessful_whenever_the_turn_did_not_complete() {
    let interrupted = RunOutcome {
        final_message: "half".into(),
        num_turns: 1,
        error_occurred: true,
        error_message: Some("The task was interrupted.".into()),
        tool_calls_count: 2,
        ..Default::default()
    };
    let value: serde_json::Value = serde_json::from_str(&emit(
        &cli(&["--output-format", "stream-json", "x"]),
        &interrupted,
    ))
    .unwrap();
    assert_eq!(value["type"], "completion");
    assert_eq!(value["success"], false);
    assert_eq!(value["error"], "The task was interrupted.");
    assert_eq!(value["toolCalls"], 2);
    assert_eq!(value["finalText"], "half");

    // A run that never errored but also never completed is still not a success.
    let silent = RunOutcome {
        num_turns: 1,
        ..Default::default()
    };
    let value: serde_json::Value = serde_json::from_str(&emit(
        &cli(&["--output-format", "stream-json", "x"]),
        &silent,
    ))
    .unwrap();
    assert_eq!(value["success"], false);
    assert_eq!(value["error"], serde_json::Value::Null);

    let completed = RunOutcome {
        task_completed: true,
        ..Default::default()
    };
    let value: serde_json::Value =
        serde_json::from_str(&emit(&cli(&["--output-format", "debug", "x"]), &completed)).unwrap();
    assert_eq!(
        value["success"], true,
        "debug is an alias of stream-json and must share its completion shape"
    );
}

#[test]
fn text_output_terminates_an_unterminated_message_and_jsonrpc_emits_nothing_here() {
    let unterminated = RunOutcome {
        final_message: "no trailing newline".into(),
        task_completed: true,
        ..Default::default()
    };
    assert_eq!(emit(&cli(&["x"]), &unterminated), "\n");

    let terminated = RunOutcome {
        final_message: "ends with newline\n".into(),
        task_completed: true,
        ..Default::default()
    };
    assert_eq!(emit(&cli(&["x"]), &terminated), "");
    assert_eq!(emit(&cli(&["x"]), &RunOutcome::default()), "");

    // stream-jsonrpc terminal frames belong to the protocol loop, not here.
    let jsonrpc = cli(&[
        "--input-format",
        "stream-jsonrpc",
        "--output-format",
        "stream-jsonrpc",
    ]);
    assert_eq!(emit(&jsonrpc, &unterminated), "");
}

#[test]
fn stream_events_carry_the_session_and_mark_a_failed_command_as_an_error() {
    let cli = cli(&["--output-format", "stream-json", "x"]);
    let session = ConversationId::new();

    let mut buffer = Vec::new();
    cli.emit_stream_event(
        &mut buffer,
        &Event {
            id: "7".into(),
            msg: EventMsg::AgentMessage(AgentMessageEvent {
                id: Some("m1".into()),
                parent_id: None,
                message: "hello".into(),
                finish_reason: None,
            }),
        },
        &session,
    )
    .unwrap();
    let value: serde_json::Value =
        serde_json::from_slice(buffer.strip_suffix(b"\n").unwrap()).unwrap();
    assert_eq!(value["type"], "message");
    assert_eq!(value["text"], "hello");
    assert_eq!(value["session_id"], session.to_string());

    let mut buffer = Vec::new();
    cli.emit_stream_event(
        &mut buffer,
        &Event {
            id: "8".into(),
            msg: EventMsg::Error(ErrorEvent {
                message: "The coding service is temporarily unavailable".into(),
                cortex_error_info: None,
            }),
        },
        &session,
    )
    .unwrap();
    let value: serde_json::Value =
        serde_json::from_slice(buffer.strip_suffix(b"\n").unwrap()).unwrap();
    assert_eq!(value["type"], "error");
    assert_eq!(
        value["message"], "The coding service is temporarily unavailable",
        "the product-facing message must reach the stream verbatim"
    );

    // Events without a stream representation must not emit an empty frame.
    let mut buffer = Vec::new();
    cli.emit_stream_event(
        &mut buffer,
        &Event {
            id: "9".into(),
            msg: EventMsg::ShutdownComplete,
        },
        &session,
    )
    .unwrap();
    assert!(buffer.is_empty(), "unexpected frame: {buffer:?}");
}

#[tokio::test]
async fn build_input_items_rejects_a_missing_image_instead_of_sending_a_text_only_turn() {
    let dir = tempfile::tempdir().unwrap();
    let error = cli(&[
        "--cwd",
        dir.path().to_str().unwrap(),
        "--image",
        "absent.png",
        "describe",
    ])
    .build_input_items("describe")
    .await
    .expect_err("a missing image must fail before submission");
    assert!(error.to_string().contains("File not found"), "{error}");

    std::fs::write(dir.path().join("pixel.png"), [0x89, b'P', b'N', b'G']).unwrap();
    let items = cli(&[
        "--cwd",
        dir.path().to_str().unwrap(),
        "--image",
        "pixel.png",
        "describe",
    ])
    .build_input_items("describe")
    .await
    .unwrap();
    assert_eq!(items.len(), 2);
    assert!(
        matches!(&items[0], UserInput::Image { media_type, .. } if media_type == "image/png"),
        "the image must be attached first with its media type"
    );
    assert!(matches!(&items[1], UserInput::Text { text } if text == "describe"));
}

#[tokio::test]
async fn an_unknown_image_extension_falls_back_to_a_generic_media_type() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("shot.bin"), [1, 2, 3]).unwrap();
    let items = cli(&[
        "--cwd",
        dir.path().to_str().unwrap(),
        "--image",
        "shot.bin",
        "x",
    ])
    .build_input_items("x")
    .await
    .unwrap();
    assert!(matches!(&items[0], UserInput::Image { media_type, .. }
            if media_type == "application/octet-stream"));
}

#[tokio::test]
async fn listing_tools_honours_the_enabled_and_disabled_filters() {
    // --list-tools is the one mode where tool filters are accepted.
    cli(&["--list-tools", "--enabled-tools", "Read"])
        .validate_runtime_options()
        .expect("--list-tools permits --enabled-tools");
    assert!(
        cli(&["--enabled-tools", "Read", "x"])
            .validate_runtime_options()
            .is_err(),
        "tool filters are unsupported outside --list-tools"
    );

    cli(&["--list-tools", "--output-format", "json"])
        .list_available_tools()
        .await
        .unwrap();
    cli(&["--list-tools", "--disabled-tools", "Read"])
        .list_available_tools()
        .await
        .unwrap();
}

#[tokio::test]
async fn stream_jsonrpc_refuses_text_input_flags_and_a_mismatched_output_format() {
    let dir = tempfile::tempdir().unwrap();
    let error = cli(&[
        "--cwd",
        dir.path().to_str().unwrap(),
        "--input-format",
        "stream-jsonrpc",
        "--git-diff",
    ])
    .run()
    .await
    .expect_err("protocol stdin has one owner");
    assert!(
        error.to_string().contains("stream-jsonrpc accepts prompts"),
        "{error}"
    );

    let error = cli(&[
        "--cwd",
        dir.path().to_str().unwrap(),
        "--output-format",
        "stream-jsonrpc",
        "hello",
    ])
    .run()
    .await
    .expect_err("stream-jsonrpc output requires stream-jsonrpc input");
    assert!(
        error
            .to_string()
            .contains("requires --input-format stream-jsonrpc"),
        "{error}"
    );

    // --list-tools is the only mode where both filters survive option validation,
    // so it is where the mutual-exclusion check is observable.
    let error = cli(&[
        "--list-tools",
        "--enabled-tools",
        "Read",
        "--disabled-tools",
        "Write",
    ])
    .run()
    .await
    .expect_err("the two tool filters are mutually exclusive");
    assert!(error.to_string().contains("Cannot specify both"), "{error}");
}
