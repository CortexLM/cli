//! `GET /v1/me` on the configured API origin (`CORTEX_API_URL`).

use std::time::Duration;

use tokio::time::timeout;

use super::code_agent::{AUTH_REQUIRED, CodeAgentClient, normalize_api_base, parse_json};
use crate::error::{CortexError, Result};

/// Documented identity route on the configured API origin.
pub const ME_PATH: &str = "/v1/me";

/// Cap for `GET /v1/me` so identity never freezes the TUI render path.
pub const ME_FETCH_TIMEOUT: Duration = Duration::from_secs(3);

/// Build `GET {base}/v1/me` from the same origin resolver as device login and turns.
pub fn me_url(base_url: &str) -> String {
    format!("{}{ME_PATH}", normalize_api_base(base_url))
}

/// Identity returned by `GET /v1/me` on the configured API origin.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MeProfile {
    pub name: Option<String>,
    pub email: Option<String>,
    pub org_name: Option<String>,
}

impl MeProfile {
    /// Parse the live `/v1/me` JSON (and a few stable aliases).
    pub fn from_json(value: &serde_json::Value) -> Self {
        let name = first_nonempty_str(value, &["name", "display_name"]).or_else(|| {
            value
                .get("user")
                .and_then(|user| first_nonempty_str(user, &["name", "display_name"]))
        });
        let email = first_nonempty_str(value, &["email"]).or_else(|| {
            value
                .get("user")
                .and_then(|user| first_nonempty_str(user, &["email"]))
        });
        let org_name = value
            .get("organizations")
            .and_then(|v| v.as_array())
            .and_then(|orgs| orgs.first())
            .and_then(|org| first_nonempty_str(org, &["org_name", "name"]))
            .or_else(|| first_nonempty_str(value, &["org_name", "organization"]));
        Self {
            name,
            email,
            org_name,
        }
    }
}

fn first_nonempty_str(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|v| v.as_str()).and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    })
}

impl CodeAgentClient {
    /// `GET {base}/v1/me` on this client's configured origin, capped at 3s.
    ///
    /// Does not start a guest session. A missing token is an auth error.
    pub async fn fetch_me(&self) -> Result<MeProfile> {
        let token = self.auth_token().await;
        if token.as_ref().is_none_or(|t| t.is_empty()) {
            return Err(CortexError::AuthenticationError {
                message: AUTH_REQUIRED.to_string(),
            });
        }
        let url = me_url(self.base_url());
        let resp = match timeout(ME_FETCH_TIMEOUT, self.authed_get(&url)).await {
            Ok(result) => result?,
            Err(_) => return Err(CortexError::Timeout),
        };
        let json: serde_json::Value = parse_json(resp).await?;
        Ok(MeProfile::from_json(&json))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn me_url_is_v1_me_on_the_given_origin() {
        assert_eq!(
            me_url("https://api.cortex.foundation/"),
            "https://api.cortex.foundation/v1/me"
        );
        assert_eq!(
            me_url("http://127.0.0.1:18081"),
            "http://127.0.0.1:18081/v1/me"
        );
        assert!(!me_url("http://127.0.0.1:18081").contains("api.cortex.foundation"));
        assert!(!me_url("http://127.0.0.1:18081").contains("/auth/me"));
    }

    #[test]
    fn me_profile_parses_name_email_and_org() {
        let json = serde_json::json!({
            "name": "Ada Lovelace",
            "email": "ada@example.com",
            "organizations": [{ "org_name": "Analytical Engines" }]
        });
        let profile = MeProfile::from_json(&json);
        assert_eq!(profile.name.as_deref(), Some("Ada Lovelace"));
        assert_eq!(profile.email.as_deref(), Some("ada@example.com"));
        assert_eq!(profile.org_name.as_deref(), Some("Analytical Engines"));
    }

    #[test]
    #[serial_test::serial]
    fn client_origin_follows_cortex_api_url() {
        let previous = std::env::var("CORTEX_API_URL").ok();
        unsafe {
            std::env::set_var("CORTEX_API_URL", "http://127.0.0.1:18081/");
        }
        let client = CodeAgentClient::new(None, Some("staging-bearer".into()));
        assert_eq!(client.base_url(), "http://127.0.0.1:18081");
        assert_eq!(me_url(client.base_url()), "http://127.0.0.1:18081/v1/me");
        assert!(
            !me_url(client.base_url()).contains("api.cortex.foundation"),
            "configured origin must not fall back to production"
        );
        match previous {
            Some(value) => unsafe { std::env::set_var("CORTEX_API_URL", value) },
            None => unsafe { std::env::remove_var("CORTEX_API_URL") },
        }
    }

    #[tokio::test]
    async fn fetch_me_hits_configured_origin_never_production() {
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

        let client = CodeAgentClient::new(Some(server.uri()), Some("staging-bearer".into()));
        assert!(
            !client.base_url().contains("api.cortex.foundation"),
            "client origin was {}",
            client.base_url()
        );

        let profile = client
            .fetch_me()
            .await
            .expect("loopback /v1/me must succeed");
        assert_eq!(profile.name.as_deref(), Some("Ada Lovelace"));
        assert_eq!(profile.org_name.as_deref(), Some("Analytical Engines"));

        let requests = server.received_requests().await.expect("recorded requests");
        assert_eq!(requests.len(), 1, "only /v1/me should be contacted");
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
        let auth = requests[0]
            .headers
            .get("authorization")
            .expect("bearer must be sent to the configured origin")
            .to_str()
            .unwrap();
        assert_eq!(auth, "Bearer staging-bearer");
    }

    #[tokio::test]
    async fn fetch_me_401_asks_for_cortex_login() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/v1/me"))
            .respond_with(wiremock::ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = CodeAgentClient::new(Some(server.uri()), Some("revoked".into()));
        let err = client.fetch_me().await.expect_err("401 is not success");
        let msg = err.user_friendly_message();
        assert!(
            msg.contains("cortex login") || msg.contains("CORTEX_API_KEY"),
            "{msg}"
        );
        assert!(!msg.to_lowercase().contains("reqwest"), "{msg}");
    }
}
