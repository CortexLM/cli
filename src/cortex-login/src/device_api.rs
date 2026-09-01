//! Live Cortex device-login contract (`api.cortex.foundation`).
//!
//! Probed 2026-08-30:
//! - `POST /v1/auth/device` → `{ device_code, user_code, verification_uri, ... }`
//! - `POST /v1/auth/device/token` → `202 { status: authorization_pending, interval }`
//!   until approved, then `200` with `access_token`.
//!
//! Do not call `/v1/auth/device/code` or `/auth/device/code` (those 404).
//! Do not silently start a guest session in place of device login.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::constants::API_BASE_URL;

/// Authorize a device (WorkOS device grant, hosted at the Cortex API).
pub const DEVICE_AUTHORIZE_PATH: &str = "/v1/auth/device";
/// Poll for the device token.
pub const DEVICE_TOKEN_PATH: &str = "/v1/auth/device/token";

/// Resolve the API origin used for device login.
///
/// `auth.cortex.foundation` is not the device endpoint (and may not resolve).
/// Device login is on `api.cortex.foundation`. `CORTEX_API_URL` wins when set.
pub fn resolve_device_api_base(issuer: &str) -> String {
    if let Ok(url) = std::env::var("CORTEX_API_URL") {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            return trimmed.trim_end_matches('/').to_string();
        }
    }
    let issuer = issuer.trim().trim_end_matches('/');
    if issuer.is_empty()
        || issuer.contains("auth.cortex.foundation")
        || issuer.contains("authkit.app")
    {
        return API_BASE_URL.trim_end_matches('/').to_string();
    }
    issuer.to_string()
}

/// Device-code payload accepted by `POST /v1/auth/device`.
#[derive(Debug, Serialize)]
struct DeviceAuthorizeBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scopes: Option<&'a [String]>,
}

/// Device authorization response.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceAuthorization {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    #[serde(default = "default_expires")]
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_expires() -> u64 {
    300
}

fn default_interval() -> u64 {
    5
}

impl DeviceAuthorization {
    /// URL the user should open. Prefer the complete URI when the API sends one.
    pub fn open_url(&self) -> &str {
        self.verification_uri_complete
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(self.verification_uri.as_str())
    }
}

/// Token returned after the user approves the device.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DeviceToken {
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub token_type: String,
    #[serde(default)]
    pub status: Option<String>,
}

impl DeviceToken {
    /// Bearer / session token from either `access_token` or `token`.
    pub fn bearer(&self) -> Option<&str> {
        if !self.access_token.is_empty() {
            Some(self.access_token.as_str())
        } else if !self.token.is_empty() {
            Some(self.token.as_str())
        } else {
            None
        }
    }

    pub fn is_pending(&self) -> bool {
        self.status
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("authorization_pending"))
    }
}

/// One poll of `POST /v1/auth/device/token`.
#[derive(Debug, Clone)]
pub enum DeviceTokenStatus {
    Pending { interval: u64 },
    Success(DeviceToken),
    Expired,
    Denied,
    Error(String),
}

/// Request a device code from the live Cortex API.
pub async fn request_device_authorization(
    client: &Client,
    api_base: &str,
    device_name: Option<&str>,
    scopes: &[String],
) -> Result<DeviceAuthorization> {
    let url = format!("{}{DEVICE_AUTHORIZE_PATH}", api_base.trim_end_matches('/'));
    let body = DeviceAuthorizeBody {
        device_id: None,
        device_name,
        client_id: Some(crate::CLIENT_ID),
        scopes: if scopes.is_empty() {
            None
        } else {
            Some(scopes)
        },
    };

    let response = client
        .post(&url)
        .header(reqwest::header::USER_AGENT, crate::constants::USER_AGENT)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("device authorize request to {url}"))?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!(product_login_error(status.as_u16(), &text));
    }
    serde_json::from_str::<DeviceAuthorization>(&text)
        .with_context(|| format!("failed to parse device authorization response ({status})"))
}

/// Poll once for the device token.
pub async fn poll_device_token(
    client: &Client,
    api_base: &str,
    device_code: &str,
) -> Result<DeviceTokenStatus> {
    let url = format!("{}{DEVICE_TOKEN_PATH}", api_base.trim_end_matches('/'));
    let response = match client
        .post(&url)
        .header(reqwest::header::USER_AGENT, crate::constants::USER_AGENT)
        .json(&serde_json::json!({ "device_code": device_code }))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(error = %e, "device token poll transport error");
            return Ok(DeviceTokenStatus::Pending { interval: 5 });
        }
    };

    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    if status.as_u16() == 202 {
        let interval = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("interval").and_then(|i| i.as_u64()))
            .unwrap_or(5);
        return Ok(DeviceTokenStatus::Pending { interval });
    }

    if let Ok(err) = serde_json::from_str::<serde_json::Value>(&text)
        && let Some(code) = err
            .get("error")
            .and_then(|e| e.as_str())
            .or_else(|| err.get("status").and_then(|e| e.as_str()))
    {
        match code {
            "authorization_pending" => {
                let interval = err.get("interval").and_then(|i| i.as_u64()).unwrap_or(5);
                return Ok(DeviceTokenStatus::Pending { interval });
            }
            "slow_down" => return Ok(DeviceTokenStatus::Pending { interval: 10 }),
            "expired_token" | "expired" => return Ok(DeviceTokenStatus::Expired),
            "access_denied" | "denied" => return Ok(DeviceTokenStatus::Denied),
            _ => {}
        }
    }

    if status.is_success() {
        let token: DeviceToken = serde_json::from_str(&text).unwrap_or_default();
        if token.is_pending() {
            return Ok(DeviceTokenStatus::Pending { interval: 5 });
        }
        if token.bearer().is_some() {
            return Ok(DeviceTokenStatus::Success(token));
        }
        return Ok(DeviceTokenStatus::Pending { interval: 5 });
    }

    Ok(DeviceTokenStatus::Error(product_login_error(
        status.as_u16(),
        &text,
    )))
}

fn product_login_error(status: u16, body: &str) -> String {
    match status {
        401 | 403 => "Sign in to continue.".to_string(),
        404 => "The coding service is temporarily unavailable".to_string(),
        429 => "Too many login attempts. Please wait.".to_string(),
        500..=599 => "The coding service is temporarily unavailable".to_string(),
        _ => {
            let detail = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|v| {
                    v.get("detail")
                        .and_then(|d| d.as_str())
                        .or_else(|| v.get("title").and_then(|d| d.as_str()))
                        .map(str::to_string)
                });
            detail.unwrap_or_else(|| "The coding service is temporarily unavailable".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_rewrites_auth_host_to_api() {
        assert_eq!(
            resolve_device_api_base("https://auth.cortex.foundation"),
            "https://api.cortex.foundation"
        );
        assert_eq!(
            resolve_device_api_base("https://api.cortex.foundation"),
            "https://api.cortex.foundation"
        );
        assert_eq!(resolve_device_api_base(""), "https://api.cortex.foundation");
    }

    #[test]
    fn open_url_prefers_complete() {
        let auth = DeviceAuthorization {
            device_code: "d".into(),
            user_code: "ABCD-1234".into(),
            verification_uri: "https://example.invalid/device".into(),
            verification_uri_complete: Some(
                "https://example.invalid/device?user_code=ABCD-1234".into(),
            ),
            expires_in: 300,
            interval: 5,
        };
        assert!(auth.open_url().contains("user_code=ABCD-1234"));
    }

    #[test]
    fn pending_token_detected() {
        let t: DeviceToken =
            serde_json::from_str(r#"{"status":"authorization_pending","interval":5}"#).unwrap();
        assert!(t.is_pending());
        assert!(t.bearer().is_none());
    }

    #[test]
    fn paths_are_live_contract() {
        assert_eq!(DEVICE_AUTHORIZE_PATH, "/v1/auth/device");
        assert_eq!(DEVICE_TOKEN_PATH, "/v1/auth/device/token");
        assert_ne!(DEVICE_AUTHORIZE_PATH, "/v1/auth/device/code");
    }
}
