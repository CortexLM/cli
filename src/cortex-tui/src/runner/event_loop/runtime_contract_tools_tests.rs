use super::*;
use cortex_engine::tools::ToolResult;
use cortex_engine::tools::spec::ToolMetadata;
use serde_json::json;

#[test]
fn runtime_contract_batch_schema_and_legacy_aliases_are_validated() {
    for args in [
        json!({"calls":[{"tool":"Read","arguments":{"file_path":"a"}}]}),
        json!({"tool_calls":[{"tool":"Read","parameters":{"file_path":"a"}}]}),
    ] {
        assert_eq!(
            parse_batch_request(args).unwrap().calls[0].arguments["file_path"],
            "a"
        );
    }
    for args in [
        json!({"calls":[]}),
        json!({"calls":[{"tool":"Read","arguments":null}]}),
        json!({"calls":[{"tool":"Task","arguments":{}}]}),
        json!({"calls":[{"tool":"Batch","arguments":{}}]}),
        json!({"calls":[{"tool":"Read","arguments":{}}],"timeout_secs":0}),
    ] {
        assert!(parse_batch_request(args).is_err());
    }
}

#[tokio::test]
async fn runtime_contract_batch_retains_exact_outputs_and_partial_failures() {
    let batch = parse_batch_request(json!({"calls":[
        {"tool":"Read","arguments":{"value":"unicode: 日本語"}},
        {"tool":"Read","arguments":{"fail":true}}
    ]}))
    .unwrap();
    let results = execute_batch_calls(batch, "batch", |_name, args| async move {
        if args["fail"] == true {
            Ok(ToolResult::error("stderr: permission denied"))
        } else {
            let mut result = ToolResult::success(args["value"].as_str().unwrap());
            result.metadata = Some(ToolMetadata {
                duration_ms: 3,
                exit_code: Some(0),
                files_modified: vec![],
                data: Some(json!({"lines":1})),
            });
            Ok(result)
        }
    })
    .await;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["id"], "batch/0");
    assert_eq!(results[0]["output"], "unicode: 日本語");
    assert_eq!(results[0]["metadata"]["exit_code"], 0);
    assert_eq!(results[0]["metadata"]["data"]["lines"], 1);
    assert_eq!(results[1]["success"], false);
    assert_eq!(results[1]["output"], "stderr: permission denied");
}

#[tokio::test]
async fn runtime_contract_batch_deadline_reports_each_unfinished_call() {
    let batch = parse_batch_request(json!({"calls":[
        {"tool":"Read","arguments":{}}, {"tool":"Read","arguments":{}}
    ],"tool_timeout_secs":1,"timeout_secs":1}))
    .unwrap();
    let results = execute_batch_calls(batch, "batch", |_, _| std::future::pending()).await;
    assert_eq!(results.len(), 2);
    assert!(
        results
            .iter()
            .all(|result| result["success"] == false && result["timed_out"] == true)
    );
}

#[tokio::test]
async fn runtime_contract_dropping_batch_drops_all_owned_calls() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct InFlight(Arc<AtomicUsize>);
    impl Drop for InFlight {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    let count = Arc::new(AtomicUsize::new(0));
    let batch = parse_batch_request(
        json!({"calls":[{"tool":"Read","arguments":{}},{"tool":"Read","arguments":{}}]}),
    )
    .unwrap();
    let work = execute_batch_calls(batch, "batch", |_, _| {
        let count = count.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            let _active = InFlight(count);
            std::future::pending().await
        }
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(20), work)
            .await
            .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn runtime_contract_batch_partial_failure_renders_without_continuation() {
    use crate::{app::AppState, app::AppView, views::MinimalSessionView};
    use cortex_tui_capture::{CaptureConfig, MockTerminal};
    for (width, height) in [(40, 12), (120, 40)] {
        let mut state = AppState::default();
        state.terminal_size = (width, height);
        state.show_launch_splash = false;
        state.opt_in_banner = false;
        state.set_view(AppView::Session);
        state.add_tool_call(
            "batch".into(),
            "Batch".into(),
            json!({"calls":[{"tool":"Read","arguments":{}}]}),
        );
        let mut event_loop = EventLoop::new(state);
        event_loop
            .handle_tool_event(ToolEvent::Completed {
                id: "batch".into(),
                name: "Batch".into(),
                output: "Partial batch failure".into(),
                success: false,
                duration: Duration::from_millis(1),
            })
            .await;
        assert!(!event_loop.stream_done_received);
        assert!(event_loop.app_state.has_pending_tool_results());
        let config = CaptureConfig::minimal(width, height);
        let mut terminal = MockTerminal::from_config(config.clone()).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    MinimalSessionView::new(event_loop.app_state()),
                    frame.area(),
                )
            })
            .unwrap();
        let snapshot = terminal.snapshot().to_ascii(&config);
        assert!(snapshot.contains("Batch"), "{snapshot}");
        assert!(snapshot.contains("Partial batch failure"), "{snapshot}");
    }
}
