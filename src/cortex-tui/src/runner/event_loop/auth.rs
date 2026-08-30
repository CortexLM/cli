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
            let scopes = vec!["openid".to_string(), "profile".to_string()];
            let api_base = cortex_login::resolve_device_api_base(API_BASE_URL);

            match cortex_login::request_device_authorization(
                &client,
                &api_base,
                Some(&device_name),
                &scopes,
            )
            .await
            {
                Ok(device_code_data) => {
                    let verification_url = device_code_data.open_url().to_string();
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
                }
                Err(e) => {
                    let _ = tx
                        .send(crate::events::ToolEvent::Failed {
                            id: "login_init".to_string(),
                            name: "login".to_string(),
                            error: format!("login:error:{}", e),
                            duration: std::time::Duration::from_secs(0),
                        })
                        .await;
                }
            }
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

            let api_base = cortex_login::resolve_device_api_base(API_BASE_URL);
            let mut interval = std::time::Duration::from_secs(5);
            let max_attempts = 180;

            for _ in 0..max_attempts {
                tokio::time::sleep(interval).await;
                match cortex_login::poll_device_token(&poll_client, &api_base, &device_code).await {
                    Ok(cortex_login::DeviceTokenStatus::Pending { interval: next }) => {
                        interval = std::time::Duration::from_secs(next.max(1));
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
                                let _ = tx
                                    .send(crate::events::ToolEvent::Failed {
                                        id: "login_poll".to_string(),
                                        name: "login".to_string(),
                                        error: format!("Failed to save credentials: {e}"),
                                        duration: std::time::Duration::from_secs(0),
                                    })
                                    .await;
                                return;
                            }
                        }
                    }
                    Ok(cortex_login::DeviceTokenStatus::Expired) => {
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
                    Ok(cortex_login::DeviceTokenStatus::Denied) => {
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
                    Ok(cortex_login::DeviceTokenStatus::Error(_)) | Err(_) => {}
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
