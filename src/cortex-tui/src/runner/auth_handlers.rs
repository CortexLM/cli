//! Authentication handlers for the TUI.
//!
//! This module contains handlers for authentication-related commands:
//! - /login - Device code OAuth flow
//! - /logout - Clear stored credentials
//! - /account - Display account information
//!
//! These handlers were extracted from `event_loop.rs` to improve modularity
//! and reduce file size.

use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;

use cortex_login::{
    AuthMode, CredentialsStoreMode, SecureAuthData, load_auth, logout_with_fallback,
    save_auth_with_fallback,
};

use crate::events::ToolEvent;

/// API base URL for Cortex authentication.
const API_BASE_URL: &str = "https://api.cortex.foundation";

/// Result of an auth operation for UI updates.
pub enum AuthResult {
    /// Already logged in, no action needed.
    AlreadyLoggedIn,
    /// Login flow started, show verification URL.
    LoginStarted {
        verification_url: String,
        user_code: String,
    },
    /// Logout successful.
    LoggedOut,
    /// No credentials found.
    NotLoggedIn,
    /// Account info loaded.
    AccountInfo {
        auth_method: String,
        expires_at: Option<String>,
        account_id: Option<String>,
    },
    /// Error occurred.
    Error(String),
}

/// Get the cortex home directory.
pub fn get_cortex_home() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| home.join(".cortex"))
}

/// Check if user is already logged in.
pub fn is_logged_in() -> bool {
    let Some(cortex_home) = get_cortex_home() else {
        return false;
    };

    if let Ok(Some(auth)) = load_auth(&cortex_home, CredentialsStoreMode::default()) {
        !auth.is_expired()
    } else {
        false
    }
}

/// Start the login flow asynchronously.
/// Returns the device code response or an error.
pub async fn start_login_flow() -> Result<(String, String, String)> {
    let client = cortex_engine::create_default_client()?;
    let api_base = cortex_login::resolve_device_api_base(API_BASE_URL);
    let device_name = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "Cortex CLI".to_string());
    let scopes = vec!["openid".to_string(), "profile".to_string()];
    let data =
        cortex_login::request_device_authorization(&client, &api_base, Some(&device_name), &scopes)
            .await?;
    let verification_uri = data.open_url().to_string();
    Ok((data.device_code, data.user_code, verification_uri))
}

/// Poll for login completion in the background.
pub fn spawn_login_poll(device_code: String, tx: mpsc::Sender<ToolEvent>) {
    let Some(cortex_home) = get_cortex_home() else {
        return;
    };

    tokio::spawn(async move {
        let poll_client = match cortex_engine::create_default_client() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Login polling failed: Could not create HTTP client: {}", e);
                let _ = tx
                    .send(ToolEvent::Failed {
                        id: "login".to_string(),
                        name: "login".to_string(),
                        error: format!("Login failed: {}", e),
                        duration: Duration::from_secs(0),
                    })
                    .await;
                return;
            }
        };

        let api_base = cortex_login::resolve_device_api_base(API_BASE_URL);
        let mut interval = Duration::from_secs(5);
        let max_attempts = 180;

        for _ in 0..max_attempts {
            tokio::time::sleep(interval).await;
            match cortex_login::poll_device_token(&poll_client, &api_base, &device_code).await {
                Ok(cortex_login::DeviceTokenStatus::Pending { interval: next }) => {
                    interval = Duration::from_secs(next.max(1));
                }
                Ok(cortex_login::DeviceTokenStatus::Success(token)) => {
                    let Some(access) = token.bearer().map(str::to_string) else {
                        continue;
                    };
                    let expires_at = token
                        .expires_in
                        .map(|secs| chrono::Utc::now().timestamp() + secs as i64)
                        .or(Some(chrono::Utc::now().timestamp() + 3600));
                    let auth_data =
                        SecureAuthData::with_oauth(access, token.refresh_token, expires_at);
                    match save_auth_with_fallback(&cortex_home, &auth_data) {
                        Ok(mode) => {
                            tracing::info!(
                                "Login successful, credentials saved using {:?} storage",
                                mode
                            );
                            let _ = tx
                                .send(ToolEvent::Completed {
                                    id: "login".to_string(),
                                    name: "login".to_string(),
                                    output: "Login successful! You are now authenticated."
                                        .to_string(),
                                    success: true,
                                    duration: Duration::from_secs(0),
                                })
                                .await;
                            return;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(ToolEvent::Failed {
                                    id: "login".to_string(),
                                    name: "login".to_string(),
                                    error: format!("Login failed: Could not save credentials: {e}"),
                                    duration: Duration::from_secs(0),
                                })
                                .await;
                            return;
                        }
                    }
                }
                Ok(cortex_login::DeviceTokenStatus::Expired) => {
                    let _ = tx
                        .send(ToolEvent::Failed {
                            id: "login".to_string(),
                            name: "login".to_string(),
                            error: "Login failed: Device code expired. Please try again."
                                .to_string(),
                            duration: Duration::from_secs(0),
                        })
                        .await;
                    return;
                }
                Ok(cortex_login::DeviceTokenStatus::Denied) => {
                    let _ = tx
                        .send(ToolEvent::Failed {
                            id: "login".to_string(),
                            name: "login".to_string(),
                            error: "Login failed: Access denied.".to_string(),
                            duration: Duration::from_secs(0),
                        })
                        .await;
                    return;
                }
                Ok(cortex_login::DeviceTokenStatus::Error(_)) | Err(_) => {}
            }
        }

        // Max attempts reached
        tracing::error!("Login failed: Authentication timed out");
        let _ = tx
            .send(ToolEvent::Failed {
                id: "login".to_string(),
                name: "login".to_string(),
                error: "Login failed: Authentication timed out. Please try again.".to_string(),
                duration: Duration::from_secs(0),
            })
            .await;
    });
}

/// Handle logout command.
pub fn handle_logout() -> AuthResult {
    let Some(cortex_home) = get_cortex_home() else {
        return AuthResult::Error("Could not determine home directory.".to_string());
    };

    match logout_with_fallback(&cortex_home) {
        Ok(true) => AuthResult::LoggedOut,
        Ok(false) => AuthResult::NotLoggedIn,
        Err(e) => AuthResult::Error(format!("Error logging out: {}", e)),
    }
}

/// Load account information.
pub fn load_account_info() -> AuthResult {
    let Some(cortex_home) = get_cortex_home() else {
        return AuthResult::Error("Could not determine home directory.".to_string());
    };

    let auth = match load_auth(&cortex_home, CredentialsStoreMode::default()) {
        Ok(Some(auth)) => auth,
        Ok(None) => return AuthResult::NotLoggedIn,
        Err(e) => return AuthResult::Error(format!("Error loading credentials: {}", e)),
    };

    if auth.is_expired() {
        return AuthResult::Error("Session expired. Use /login to re-authenticate.".to_string());
    }

    let auth_method = match auth.mode {
        AuthMode::ApiKey => "API Key".to_string(),
        AuthMode::OAuth => "OAuth".to_string(),
    };

    let expires_at = auth.expires_at.and_then(|ts| {
        chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
    });

    let account_id = auth.account_id.clone();

    AuthResult::AccountInfo {
        auth_method,
        expires_at,
        account_id,
    }
}

/// Opens a URL in the default browser.
///
/// This function validates URLs for security (only http/https allowed).
pub fn open_browser_url(url: &str) -> Result<()> {
    let parsed_url = url::Url::parse(url)?;

    // Only allow HTTP and HTTPS URLs
    match parsed_url.scheme() {
        "http" | "https" => {}
        scheme => {
            anyhow::bail!(
                "Refusing to open URL with scheme '{}': only http and https are allowed",
                scheme
            );
        }
    }

    // Reject URLs with embedded credentials
    if !parsed_url.username().is_empty() || parsed_url.password().is_some() {
        anyhow::bail!("Refusing to open URL with embedded credentials");
    }

    // Try to open in browser
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }

    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn();
    }

    Ok(())
}
