//! MCP panel: picker actions and lifecycle events.

use super::core::EventLoop;
use crate::modal::mcp_manager::McpStatus;
use cortex_engine::mcp::McpLifecycleEvent;

impl EventLoop {
    /// Re-opens the MCP panel after add / reconnect / stop.
    pub(super) fn reopen_mcp_panel(&mut self) {
        use crate::interactive::builders::build_mcp_selector;
        let servers = self.app_state.mcp_servers.clone();
        let interactive = build_mcp_selector(&servers);
        self.app_state.enter_interactive_mode(interactive);
    }

    pub(super) async fn handle_mcp_selector_item(&mut self, item_id: &str) -> bool {
        if let Some(name) = item_id.strip_prefix("__reconnect__:") {
            return self.handle_mcp_server_action(name, "restart").await;
        }
        match item_id {
            "__add__" => {
                let interactive = crate::interactive::builders::build_mcp_source_selector();
                self.app_state.enter_interactive_mode(interactive);
                true
            }
            "__tools__" => {
                let manager = self.mcp_manager.clone();
                let tools = manager.list_all_tools().await;
                if tools.is_empty() {
                    self.add_system_message("No MCP tools connected.");
                } else {
                    let mut lines: Vec<String> = tools.keys().cloned().collect();
                    lines.sort();
                    self.add_system_message(&format!(
                        "MCP tools ({})\n{}",
                        lines.len(),
                        lines.join("\n")
                    ));
                }
                false
            }
            "__reload__" => {
                let manager = self.mcp_manager.clone();
                tokio::spawn(async move {
                    let names = manager.server_names().await;
                    for name in names {
                        let _ = manager.disconnect(&name).await;
                        let _ = manager.connect(&name).await;
                    }
                });
                self.app_state.toasts.info("Reloading MCP servers…");
                self.reopen_mcp_panel();
                true
            }
            name => {
                if let Some(server) = self
                    .app_state
                    .mcp_servers
                    .iter()
                    .find(|s| s.name == name)
                    .cloned()
                {
                    let interactive =
                        crate::interactive::builders::build_mcp_server_actions(&server);
                    self.app_state.enter_interactive_mode(interactive);
                    true
                } else {
                    false
                }
            }
        }
    }

    pub(super) async fn handle_mcp_server_action(&mut self, server: &str, action: &str) -> bool {
        let manager = self.mcp_manager.clone();
        let name = server.to_string();
        match action {
            "start" | "restart" => {
                if action == "restart" {
                    self.mcp_stopping.insert(name.clone());
                    let _ = manager.disconnect(&name).await;
                }
                if let Some(s) = self
                    .app_state
                    .mcp_servers
                    .iter_mut()
                    .find(|s| s.name == name)
                {
                    s.status = McpStatus::Starting;
                }
                tokio::spawn(async move {
                    let _ = manager.connect(&name).await;
                });
                self.reopen_mcp_panel();
                true
            }
            "stop" => {
                self.mcp_stopping.insert(name.clone());
                tokio::spawn(async move {
                    let _ = manager.disconnect(&name).await;
                });
                self.reopen_mcp_panel();
                true
            }
            "remove" => {
                self.mcp_stopping.insert(name.clone());
                let _ = manager.remove_server(&name).await;
                self.app_state.mcp_servers.retain(|s| s.name != name);
                self.reopen_mcp_panel();
                true
            }
            _ => false,
        }
    }

    /// Apply an MCP lifecycle event to the session list and transcript.
    pub(super) fn handle_mcp_event(&mut self, event: McpLifecycleEvent) {
        match event {
            McpLifecycleEvent::ServerAdded { name } => {
                if !self.app_state.mcp_servers.iter().any(|s| s.name == name) {
                    self.app_state
                        .mcp_servers
                        .push(crate::modal::mcp_manager::McpServerInfo {
                            name,
                            status: McpStatus::Stopped,
                            tool_count: 0,
                            error: None,
                            requires_auth: false,
                        });
                }
            }
            McpLifecycleEvent::ServerConnected {
                name, tool_count, ..
            } => {
                if let Some(server) = self
                    .app_state
                    .mcp_servers
                    .iter_mut()
                    .find(|s| s.name == name)
                {
                    server.status = McpStatus::Running;
                    server.tool_count = tool_count;
                    server.error = None;
                }
            }
            McpLifecycleEvent::ServerDisconnected { name } => {
                let user_stop = self.mcp_stopping.remove(&name);
                if let Some(server) = self
                    .app_state
                    .mcp_servers
                    .iter_mut()
                    .find(|s| s.name == name)
                {
                    if user_stop {
                        server.status = McpStatus::Stopped;
                        server.error = None;
                    } else {
                        server.status = McpStatus::Error;
                        server.error = Some("connection lost".into());
                        self.add_system_message(&format!("× {name} dropped"));
                        self.add_system_message(&format!(
                            "Reconnecting — tools from {name} are paused until it is back."
                        ));
                    }
                }
            }
            McpLifecycleEvent::ServerRemoved { name } => {
                self.app_state.mcp_servers.retain(|s| s.name != name);
            }
            McpLifecycleEvent::ConnectionFailed { name, error } => {
                if let Some(server) = self
                    .app_state
                    .mcp_servers
                    .iter_mut()
                    .find(|s| s.name == name)
                {
                    server.status = McpStatus::Error;
                    server.error = Some(error.clone());
                }
                self.add_system_message(&format!("× {name} failed"));
            }
        }
    }
}
