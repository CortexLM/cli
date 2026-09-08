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
