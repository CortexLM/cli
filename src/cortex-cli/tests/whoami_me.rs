//! `cortex whoami` talks to `GET /v1/me` on the configured API origin.
//!
//! These tests prove a staging/loopback `CORTEX_API_URL` is the only host
//! contacted. Nothing here reaches `api.cortex.foundation`.

use std::process::Command;

fn whoami(home: &std::path::Path, api_url: &str, token: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_Cortex"));
    command
        .arg("whoami")
        .env("HOME", home)
        .env("CORTEX_HOME", home)
        .env("CORTEX_API_URL", api_url)
        .env("RUST_LOG", "off")
        .env("NO_COLOR", "1")
        .env_remove("CORTEX_API_KEY")
        .current_dir(home);
    match token {
        Some(token) => {
            command.env("CORTEX_AUTH_TOKEN", token);
        }
        None => {
            command.env_remove("CORTEX_AUTH_TOKEN");
        }
    }
    command.output().unwrap()
}

fn combined(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[tokio::test]
async fn whoami_hits_v1_me_on_configured_origin_never_production() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1/me"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "Ada Lovelace",
                "email": "ada@example.com",
                "organizations": [{ "org_name": "Analytical Engines" }]
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/auth/me"))
        .respond_with(wiremock::ResponseTemplate::new(599))
        .expect(0)
        .mount(&server)
        .await;

    let home = tempfile::tempdir().unwrap();
    let output = whoami(home.path(), &server.uri(), Some("staging-bearer"));
    assert!(
        output.status.success(),
        "whoami against a 200 fixture must succeed: {}",
        combined(&output)
    );
    let text = combined(&output);
    assert!(
        text.contains("Ada Lovelace"),
        "live /v1/me identity should be printed: {text}"
    );
    assert!(
        !text.contains("api.cortex.foundation"),
        "production host must not appear: {text}"
    );

    let requests = server.received_requests().await.expect("recorded requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/v1/me");
    let request_url = requests[0].url.to_string();
    assert!(
        !request_url.contains("api.cortex.foundation"),
        "production host must not be contacted: {request_url}"
    );
    assert!(
        request_url.contains("127.0.0.1") || request_url.contains("localhost"),
        "request must stay on the loopback fixture: {request_url}"
    );
}

#[tokio::test]
async fn whoami_401_prints_cortex_login_and_exits_nonzero() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1/me"))
        .respond_with(wiremock::ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;

    let home = tempfile::tempdir().unwrap();
    let output = whoami(home.path(), &server.uri(), Some("revoked-token"));
    assert!(
        !output.status.success(),
        "a revoked token must fail whoami; got {}: {}",
        output.status,
        combined(&output)
    );
    let text = combined(&output);
    assert!(
        text.contains("cortex login"),
        "401 must print the login recovery copy: {text}"
    );
    assert!(!text.to_lowercase().contains("reqwest"), "{text}");
    assert!(!text.contains("api.cortex.foundation"), "{text}");

    let requests = server.received_requests().await.expect("recorded requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/v1/me");
}

#[test]
fn whoami_without_credentials_prints_login_copy_and_does_not_need_the_network() {
    let home = tempfile::tempdir().unwrap();
    let output = whoami(home.path(), "http://127.0.0.1:1", None);
    assert!(
        !output.status.success(),
        "no credential is not a successful whoami: {}",
        combined(&output)
    );
    let text = combined(&output);
    assert!(
        text.contains("cortex login") || text.contains("CORTEX_API_KEY"),
        "{text}"
    );
    assert!(!text.contains("api.cortex.foundation"), "{text}");
}
