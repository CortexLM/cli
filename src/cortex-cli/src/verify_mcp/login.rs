//! `login.run` — headless sign-in frames with product-facing errors.

use std::time::Duration;

use anyhow::Result;
use ratatui::widgets::Clear;
use serde_json::{Value, json};

const SERVICE_UNAVAILABLE: &str = "The coding service is temporarily unavailable";
use cortex_tui::lock_proof::{LOCK_SPLASH_VERSION, LockFrame};
use cortex_tui::runner::login_screen::LoginScreen;
use cortex_tui_capture::MockTerminal;

use super::frame::{capture_config, frame_payload};
use super::state::{Check, FlowRow, VerifyState};

pub async fn run(state: &mut VerifyState, args: &Value) -> Result<Value> {
    let method = args
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("browser");
    let fixture = args
        .get("fixture")
        .and_then(Value::as_str)
        .unwrap_or("unreachable");
    let api_url = args
        .get("api_url")
        .and_then(Value::as_str)
        .unwrap_or("http://127.0.0.1:1");

    let select = render_login(LoginScreen::lock_select(LOCK_SPLASH_VERSION, None))?;
    let waiting = render_login(LoginScreen::lock_waiting(
        LOCK_SPLASH_VERSION,
        "ABCD-1234",
        "https://cortex.foundation/cli/auth",
    ))?;

    let product = match fixture {
        "ok" => None,
        "denied" => {
            Some("Not signed in. Run `cortex login` or set CORTEX_API_KEY, then try again.")
        }
        "expired" => Some(SERVICE_UNAVAILABLE),
        "429" => Some("Too many requests. Please wait and try again."),
        _ => {
            let _ = probe_device(api_url).await;
            Some(SERVICE_UNAVAILABLE)
        }
    };

    let error_frame = product
        .map(|copy| render_login(LoginScreen::lock_failed(LOCK_SPLASH_VERSION, copy)))
        .transpose()?;

    let ok = match fixture {
        "ok" => true,
        _ => error_frame.as_ref().is_some_and(|f| {
            f.plain.contains("temporarily unavailable")
                || f.plain.contains("Not signed in")
                || f.plain.contains("Too many requests")
        }),
    };

    let flow_id = format!("login.{method}.{fixture}");
    state.record_flow(FlowRow {
        id: flow_id.clone(),
        status: if ok { "pass" } else { "fail" }.into(),
        checks: vec![Check {
            name: "product_error".into(),
            ok,
            detail: product.map(str::to_string),
        }],
    });

    Ok(json!({
        "flow": flow_id,
        "method": method,
        "fixture": fixture,
        "product_copy": product,
        "frames": {
            "select": frame_payload(&select, "plain"),
            "waiting": frame_payload(&waiting, "plain"),
            "error": error_frame.as_ref().map(|f| frame_payload(f, "plain")),
        },
    }))
}

async fn probe_device(api_url: &str) -> Result<()> {
    let url = format!("{}/v1/auth/device", api_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    client
        .post(&url)
        .json(&json!({}))
        .send()
        .await
        .map(|_| ())
        .map_err(|_| anyhow::anyhow!("{SERVICE_UNAVAILABLE}"))
}

fn render_login(screen: LoginScreen) -> Result<LockFrame> {
    let config = capture_config(40, 12);
    let mut terminal = MockTerminal::from_config(config).map_err(|err| anyhow::anyhow!("{err}"))?;
    terminal.draw(|frame| {
        frame.render_widget(Clear, frame.area());
        screen.render(frame);
    })?;
    let buffer = terminal.backend().buffer().clone();
    let plain = super::frame::buffer_plain(&buffer);
    Ok(LockFrame {
        id: "login".into(),
        ansi: terminal.backend().snapshot().to_ansi(&Default::default()),
        plain,
        buffer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify_mcp::state::VerifyState;

    async fn fixture(name: &str) -> Value {
        let mut state = VerifyState::new();
        run(
            &mut state,
            &json!({
                "method": "browser",
                "api_url": "http://127.0.0.1:1",
                "fixture": name
            }),
        )
        .await
        .unwrap_or_else(|err| panic!("{name}: {err}"))
    }

    #[tokio::test]
    async fn fixtures_use_product_facing_copy() {
        let unreachable = fixture("unreachable").await;
        assert_eq!(unreachable["product_copy"], SERVICE_UNAVAILABLE);
        assert!(
            unreachable["product_copy"]
                .as_str()
                .is_some_and(|c| c.contains("temporarily unavailable"))
        );

        let denied = fixture("denied").await;
        assert!(
            denied["product_copy"]
                .as_str()
                .is_some_and(|c| c.contains("Not signed in"))
        );

        let expired = fixture("expired").await;
        assert_eq!(expired["product_copy"], SERVICE_UNAVAILABLE);

        let rate = fixture("429").await;
        assert!(
            rate["product_copy"]
                .as_str()
                .is_some_and(|c| c.contains("Too many requests"))
        );

        let ok = fixture("ok").await;
        assert!(ok["product_copy"].is_null());
        assert!(ok["frames"]["select"].is_object());
        assert!(ok["frames"]["waiting"].is_object());
    }

    #[tokio::test]
    async fn api_key_method_defaults_and_probe_error() {
        let mut state = VerifyState::new();
        let value = run(&mut state, &json!({"method": "api_key"}))
            .await
            .expect("default fixture");
        assert_eq!(value["method"], "api_key");
        assert_eq!(value["product_copy"], SERVICE_UNAVAILABLE);
        let probed = probe_device("http://127.0.0.1:1").await;
        assert!(probed.is_err());
        assert!(
            probed
                .unwrap_err()
                .to_string()
                .contains("temporarily unavailable")
        );
    }
}
