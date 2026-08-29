//! Authentication handling: login, logout, account management.

use super::core::EventLoop;

impl EventLoop {
    /// Starts the login flow with the interactive widget.
    pub(super) async fn start_login_flow(&mut self) {
        use crate::interactive::builders::{
            LoginFlowState, build_already_logged_in_selector, build_login_selector,
        };
        use cortex_login::{CredentialsStoreMode, load_auth, logout_with_fallback};

        let cortex_home = match dirs::home_dir() {
            Some(home) => home.join(".cortex"),
            None => {
                self.app_state.toasts.error("Could not find home directory");
                return;
            }
        };

        // Check if already logged in
        if let Ok(Some(auth)) = load_auth(&cortex_home, CredentialsStoreMode::default()) {
            if !auth.is_expired() {
                let interactive = build_already_logged_in_selector();
                self.app_state.enter_interactive_mode(interactive);
                return;
            } else {
                tracing::info!("Detected expired session, removing stale credentials");
                if let Err(e) = logout_with_fallback(&cortex_home) {
                    tracing::warn!("Failed to remove expired credentials: {}", e);
                }
            }
        }

        // Show loading widget immediately
        let flow_state = LoginFlowState::loading();
        self.app_state.login_flow = Some(flow_state);
        let interactive = build_login_selector(self.app_state.login_flow.as_ref().unwrap());
        self.app_state.enter_interactive_mode(interactive);

        // Launch background task for device code request
        let tx = self.tool_event_tx.clone();
        tokio::spawn(async move {
            const API_BASE_URL: &str = "https://api.cortex.foundation";
            const AUTH_BASE_URL: &str = "https://auth.cortex.foundation";

            let client = match cortex_engine::create_client_builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx
                        .send(crate::events::ToolEvent::Failed {
                            id: "login_init".to_string(),
                            name: "login".to_string(),
                            error: format!("login:error:{}", e),
                            duration: std::time::Duration::from_secs(0),
                        })
                        .await;
                    return;
                }
            };

            let device_name = hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "Cortex CLI".to_string());

            let response = match client
                .post(format!("{}/auth/device/code", API_BASE_URL))
                .json(&serde_json::json!({
                    "device_name": device_name,
                    "scopes": ["chat", "models"]
                }))
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(crate::events::ToolEvent::Failed {
                            id: "login_init".to_string(),
                            name: "login".to_string(),
                            error: format!("login:error:Network error: {}", e),
                            duration: std::time::Duration::from_secs(0),
                        })
                        .await;
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let error_msg = match status.as_u16() {
                    403 => "Cannot connect to Cortex API".to_string(),
                    429 => "Too many attempts. Please wait.".to_string(),
                    _ => format!("API error ({})", status),
                };
                let _ = tx
                    .send(crate::events::ToolEvent::Failed {
                        id: "login_init".to_string(),
                        name: "login".to_string(),
                        error: format!("login:error:{}", error_msg),
                        duration: std::time::Duration::from_secs(0),
                    })
                    .await;
                return;
            }

            #[derive(serde::Deserialize)]
            struct DeviceCodeResponse {
                user_code: String,
                device_code: String,
                #[allow(dead_code)]
                verification_uri: String,
            }

            let device_code_data: DeviceCodeResponse = match response.json().await {
                Ok(d) => d,
                Err(e) => {
                    let _ = tx
                        .send(crate::events::ToolEvent::Failed {
                            id: "login_init".to_string(),
                            name: "login".to_string(),
                            error: format!("login:error:Parse error: {}", e),
                            duration: std::time::Duration::from_secs(0),
                        })
                        .await;
                    return;
                }
            };

            let verification_url = format!("{}/device", AUTH_BASE_URL);

            let _ = tx
                .send(crate::events::ToolEvent::Completed {
                    id: "login_init".to_string(),
                    name: "login".to_string(),
                    output: serde_json::json!({
                        "device_code": device_code_data.device_code,
                        "user_code": device_code_data.user_code,
                        "verification_uri": verification_url,
                    })
                    .to_string(),
                    success: true,
                    duration: std::time::Duration::from_secs(0),
                })
                .await;
        });
    }

    /// Starts polling for login token after device code is received.
    pub(super) fn start_login_polling(&mut self, device_code: String) {
        use cortex_login::{SecureAuthData, save_auth_with_fallback};

        const API_BASE_URL: &str = "https://api.cortex.foundation";

        let cortex_home = match dirs::home_dir() {
            Some(home) => home.join(".cortex"),
            None => return,
        };

        let tx = self.tool_event_tx.clone();

        tokio::spawn(async move {
            let poll_client = match cortex_engine::create_default_client() {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Login polling failed: {}", e);
                    let _ = tx
                        .send(crate::events::ToolEvent::Failed {
                            id: "login_poll".to_string(),
                            name: "login".to_string(),
                            error: format!("login:error:{}", e),
                            duration: std::time::Duration::from_secs(0),
                        })
                        .await;
                    return;
                }
            };

            let interval = std::time::Duration::from_secs(5);
            let max_attempts = 180;

            for _ in 0..max_attempts {
                tokio::time::sleep(interval).await;

                let poll_response = match poll_client
                    .post(format!("{}/auth/device/token", API_BASE_URL))
                    .json(&serde_json::json!({ "device_code": device_code }))
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                let status = poll_response.status();
                let body = poll_response.text().await.unwrap_or_default();

                if status.is_success() {
                    #[derive(serde::Deserialize)]
                    struct TokenResponse {
                        access_token: String,
                        refresh_token: String,
                    }

                    if let Ok(token) = serde_json::from_str::<TokenResponse>(&body) {
                        let expires_at = chrono::Utc::now().timestamp() + 3600;
                        let auth_data = SecureAuthData::with_oauth(
                            token.access_token,
                            Some(token.refresh_token),
                            Some(expires_at),
                        );

                        match save_auth_with_fallback(&cortex_home, &auth_data) {
                            Ok(mode) => {
                                tracing::info!("Auth credentials saved using {:?} storage", mode);
                                let _ = tx
                                    .send(crate::events::ToolEvent::Completed {
                                        id: "login_poll".to_string(),
                                        name: "login".to_string(),
                                        output: "login:success".to_string(),
                                        success: true,
                                        duration: std::time::Duration::from_secs(0),
                                    })
                                    .await;
                                return;
                            }
                            Err(e) => {
                                tracing::error!("Failed to save auth credentials: {}", e);
                                let _ = tx
                                    .send(crate::events::ToolEvent::Failed {
                                        id: "login_poll".to_string(),
                                        name: "login".to_string(),
                                        error: format!("Failed to save credentials: {}", e),
                                        duration: std::time::Duration::from_secs(0),
                                    })
                                    .await;
                                return;
                            }
                        }
                    }
                    continue;
                }

                if let Ok(error) = serde_json::from_str::<serde_json::Value>(&body)
                    && let Some(err) = error.get("error").and_then(|e| e.as_str())
                {
                    match err {
                        "authorization_pending" | "slow_down" => continue,
                        "expired_token" => {
                            let _ = tx
                                .send(crate::events::ToolEvent::Failed {
                                    id: "login_poll".to_string(),
                                    name: "login".to_string(),
                                    error: "login:expired".to_string(),
                                    duration: std::time::Duration::from_secs(0),
                                })
                                .await;
                            return;
                        }
                        "access_denied" => {
                            let _ = tx
                                .send(crate::events::ToolEvent::Failed {
                                    id: "login_poll".to_string(),
                                    name: "login".to_string(),
                                    error: "login:denied".to_string(),
                                    duration: std::time::Duration::from_secs(0),
                                })
                                .await;
                            return;
                        }
                        _ => {}
                    }
                }
            }

            let _ = tx
                .send(crate::events::ToolEvent::Failed {
                    id: "login_poll".to_string(),
                    name: "login".to_string(),
                    error: "login:timeout".to_string(),
                    duration: std::time::Duration::from_secs(0),
                })
                .await;
        });
    }

    /// Handle login init success
    pub(super) async fn handle_login_init_success(&mut self, output: &str) {
        use crate::interactive::builders::build_login_selector;

        if let Ok(data) = serde_json::from_str::<serde_json::Value>(output) {
            let device_code = data["device_code"].as_str().unwrap_or_default().to_string();
            let user_code = data["user_code"].as_str().unwrap_or_default().to_string();
            let verification_uri = data["verification_uri"]
                .as_str()
                .unwrap_or_default()
                .to_string();

            if let Some(ref mut flow) = self.app_state.login_flow {
                flow.set_device_code(device_code.clone(), user_code, verification_uri);
            }

            if let Some(ref flow) = self.app_state.login_flow {
                let interactive = build_login_selector(flow);
                self.app_state.enter_interactive_mode(interactive);
            }

            self.start_login_polling(device_code);
        }
    }

    /// Handle login poll success
    pub(super) async fn handle_login_poll_success(&mut self) {
        self.app_state.login_flow = None;
        self.app_state.exit_interactive_mode();

        // Reload fresh auth token into provider_manager
        if let Some(ref pm) = self.provider_manager {
            if let Some(token) = cortex_login::get_auth_token() {
                tracing::info!("Reloading fresh auth token into provider_manager after login");
                pm.write().await.set_auth_token(token);
            } else {
                tracing::warn!("Login succeeded but could not load fresh token from keyring");
            }
        }

        self.app_state.toasts.success("Logged in!");
    }

    /// Handle legacy login success
    pub(super) async fn handle_legacy_login_success(&mut self, output: &str) {
        // Reload fresh auth token into provider_manager
        if let Some(ref pm) = self.provider_manager {
            if let Some(token) = cortex_login::get_auth_token() {
                tracing::info!(
                    "Reloading fresh auth token into provider_manager after legacy login"
                );
                pm.write().await.set_auth_token(token);
            } else {
                tracing::warn!("Login succeeded but could not load fresh token from keyring");
            }
        }
        self.add_system_message(output);
        self.app_state.toasts.success("Logged in!");
    }

    /// Handle billing data
    pub(super) fn handle_billing_data(&mut self, data_str: &str) {
        use crate::interactive::builders::build_billing_selector;

        if let Some(ref mut flow) = self.app_state.billing_flow {
            for part in data_str.split('|') {
                if let Some((key, value)) = part.split_once('=') {
                    match key {
                        "plan" => flow.plan_name = Some(value.to_string()),
                        "status" => flow.plan_status = Some(value.to_string()),
                        "period_start" => flow.current_period_start = Some(value.to_string()),
                        "period_end" => flow.current_period_end = Some(value.to_string()),
                        "tokens" => flow.total_tokens = value.parse().ok(),
                        "requests" => flow.total_requests = value.parse().ok(),
                        "cost" => flow.total_cost_usd = value.parse().ok(),
                        "quota_used" => flow.quota_used = value.parse().ok(),
                        "quota_limit" => flow.quota_limit = value.parse().ok(),
                        _ => {}
                    }
                }
            }
            flow.set_ready();
            let interactive = build_billing_selector(flow);
            self.app_state.enter_interactive_mode(interactive);
        }
    }

    /// Handle billing error
    pub(super) fn handle_billing_error(&mut self, error: &str) {
        use crate::interactive::builders::build_billing_selector;

        if let Some(ref mut flow) = self.app_state.billing_flow {
            if error == "billing:not_logged_in" {
                flow.set_not_logged_in();
            } else if let Some(msg) = error.strip_prefix("billing:error:") {
                flow.set_error(msg.to_string());
            } else {
                flow.set_error(error.to_string());
            }
            let interactive = build_billing_selector(flow);
            self.app_state.enter_interactive_mode(interactive);
        }
    }

    /// Save MCP server to storage
    pub(super) fn save_mcp_server(
        &self,
        server: &crate::mcp_storage::StoredMcpServer,
    ) -> anyhow::Result<()> {
        let storage = crate::mcp_storage::McpStorage::new()?;
        storage.save_server(server)
    }

    /// Inject agent created event message
    pub(super) fn inject_agent_created_event(&mut self, name: &str) {
        self.add_system_message(&format!(
            "Agent @{} has been created and is now available. You can mention it with @{} in your messages.",
            name, name
        ));
    }
}
