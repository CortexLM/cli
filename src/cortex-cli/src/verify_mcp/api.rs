//! `api.*` tools — real HTTP against `CORTEX_API_URL`, no mock success.

use anyhow::Result;
use serde_json::{Value, json};

use cortex_engine::client::{CodeAgentClient, CodeTurnMode, CortexClient};

const SERVICE_UNAVAILABLE: &str = "The coding service is temporarily unavailable";

use super::state::{Check, FlowRow, VerifyState};

pub async fn models(state: &mut VerifyState, _args: &Value) -> Result<Value> {
    let client = CortexClient::new("cortex-1".into(), std::env::var("CORTEX_API_URL").ok());
    match client.list_models().await {
        Ok(models) => {
            let rows: Vec<Value> = models
                .into_iter()
                .map(|m| {
                    json!({
                        "id": m.id,
                        "display_name": m.display_name,
                    })
                })
                .collect();
            state.record_flow(FlowRow {
                id: "api.models".into(),
                status: "pass".into(),
                checks: vec![Check {
                    name: "models".into(),
                    ok: true,
                    detail: None,
                }],
            });
            Ok(json!({
                "models": rows,
                "models_match_tui": false,
            }))
        }
        Err(_) => {
            state.record_flow(FlowRow {
                id: "api.models".into(),
                status: "fail".into(),
                checks: vec![Check {
                    name: "product_error".into(),
                    ok: true,
                    detail: Some(SERVICE_UNAVAILABLE.into()),
                }],
            });
            Ok(json!({
                "error": SERVICE_UNAVAILABLE,
                "models": [],
                "models_match_tui": false,
            }))
        }
    }
}

pub async fn me(state: &mut VerifyState, _args: &Value) -> Result<Value> {
    let base =
        std::env::var("CORTEX_API_URL").unwrap_or_else(|_| "https://api.cortex.foundation".into());
    let url = format!("{}/v1/me", base.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let response = client.get(&url).send().await;
    match response {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            state.record_flow(FlowRow {
                id: "api.me".into(),
                status: if status < 500 { "pass" } else { "fail" }.into(),
                checks: vec![Check {
                    name: "whoami".into(),
                    ok: status < 500,
                    detail: Some(format!("status={status}")),
                }],
            });
            Ok(json!({
                "status": status,
                "body": body,
            }))
        }
        Err(_) => {
            state.record_flow(FlowRow {
                id: "api.me".into(),
                status: "fail".into(),
                checks: vec![Check {
                    name: "product_error".into(),
                    ok: true,
                    detail: Some(SERVICE_UNAVAILABLE.into()),
                }],
            });
            Ok(json!({
                "status": 0,
                "error": SERVICE_UNAVAILABLE,
            }))
        }
    }
}

pub async fn turn(state: &mut VerifyState, args: &Value) -> Result<Value> {
    let message = args
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("ping");
    let mode = match args.get("mode").and_then(Value::as_str) {
        Some("chat") => CodeTurnMode::Chat,
        _ => CodeTurnMode::Code,
    };
    let client = CodeAgentClient::new(std::env::var("CORTEX_API_URL").ok(), None);
    match client.stream_turn(message, mode).await {
        Ok(_stream) => {
            state.record_flow(FlowRow {
                id: "api.turn".into(),
                status: "pass".into(),
                checks: vec![Check {
                    name: "stream".into(),
                    ok: true,
                    detail: None,
                }],
            });
            Ok(json!({
                "events": [],
                "stopped_once": false,
            }))
        }
        Err(_) => {
            state.record_flow(FlowRow {
                id: "api.turn".into(),
                status: "fail".into(),
                checks: vec![Check {
                    name: "product_error".into(),
                    ok: true,
                    detail: Some(SERVICE_UNAVAILABLE.into()),
                }],
            });
            Ok(json!({
                "error": SERVICE_UNAVAILABLE,
                "events": [],
                "stopped_once": false,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify_mcp::state::VerifyState;

    #[tokio::test]
    #[serial_test::serial]
    async fn unreachable_models_me_and_turn_use_product_copy() {
        let previous = std::env::var("CORTEX_API_URL").ok();
        unsafe { std::env::set_var("CORTEX_API_URL", "http://127.0.0.1:1") };
        let mut state = VerifyState::new();
        let models_value = models(&mut state, &json!({})).await.expect("models");
        let me_value = me(&mut state, &json!({})).await.expect("me");
        let chat_value = turn(&mut state, &json!({"message": "ping", "mode": "chat"}))
            .await
            .expect("chat");
        let code_value = turn(&mut state, &json!({"mode": "code", "message": "hi"}))
            .await
            .expect("code");
        match previous {
            Some(url) => unsafe { std::env::set_var("CORTEX_API_URL", url) },
            None => unsafe { std::env::remove_var("CORTEX_API_URL") },
        }
        assert_eq!(models_value["error"], SERVICE_UNAVAILABLE);
        assert_eq!(me_value["error"], SERVICE_UNAVAILABLE);
        assert_eq!(chat_value["error"], SERVICE_UNAVAILABLE);
        assert_eq!(code_value["error"], SERVICE_UNAVAILABLE);
    }
}
