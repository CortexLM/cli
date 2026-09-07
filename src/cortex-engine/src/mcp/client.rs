//! Active MCP runtime client. Stdio and Streamable HTTP are explicitly negotiated.
use super::http::HttpTransport;
use super::stdio::StdioTransport;
use super::{McpServerConfig, TransportType};
use anyhow::{Result, anyhow, bail};
use cortex_mcp_types::{
    CallToolResult, GetPromptResult, InitializeParams, InitializeResult, JsonRpcRequest,
    JsonRpcResponse, Prompt, ReadResourceResult, Resource, Tool,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI64, Ordering},
};
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, broadcast};

/// Version-pinned support, not a claim to implement draft/latest specifications.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26"];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

pub struct McpClient {
    config: McpServerConfig,
    state: RwLock<ConnectionState>,
    server_info: RwLock<Option<InitializeResult>>,
    request_id: AtomicI64,
    stdio: RwLock<Option<Arc<StdioTransport>>>,
    http: HttpTransport,
    cached_tools: RwLock<Vec<Tool>>,
    cached_resources: RwLock<Vec<Resource>>,
    lifecycle: Mutex<()>,
    notifications: broadcast::Sender<Value>,
    changes: Mutex<broadcast::Receiver<Value>>,
    timeout: Duration,
    tools_dirty: AtomicBool,
}
impl McpClient {
    pub fn new(config: McpServerConfig) -> Self {
        Self::with_timeout(config, Duration::from_secs(30))
    }
    pub fn with_timeout(config: McpServerConfig, timeout: Duration) -> Self {
        let (notifications, changes) = broadcast::channel(128);
        Self {
            http: HttpTransport::new(config.clone()),
            config,
            state: RwLock::new(ConnectionState::Disconnected),
            server_info: RwLock::new(None),
            request_id: AtomicI64::new(1),
            stdio: RwLock::new(None),
            cached_tools: RwLock::new(vec![]),
            cached_resources: RwLock::new(vec![]),
            lifecycle: Mutex::new(()),
            notifications,
            changes: Mutex::new(changes),
            timeout,
            tools_dirty: AtomicBool::new(false),
        }
    }
    pub fn name(&self) -> &str {
        &self.config.name
    }
    pub async fn state(&self) -> ConnectionState {
        let state = *self.state.read().await;
        if state == ConnectionState::Connected
            && self.config.transport == TransportType::Stdio
            && self
                .stdio
                .read()
                .await
                .as_ref()
                .is_none_or(|s| !s.is_open())
        {
            *self.state.write().await = ConnectionState::Failed;
            return ConnectionState::Failed;
        }
        state
    }
    pub async fn is_connected(&self) -> bool {
        self.state().await == ConnectionState::Connected
    }
    pub async fn server_info(&self) -> Option<InitializeResult> {
        self.server_info.read().await.clone()
    }
    pub async fn tools(&self) -> Vec<Tool> {
        self.cached_tools.read().await.clone()
    }
    pub async fn resources(&self) -> Vec<Resource> {
        self.cached_resources.read().await.clone()
    }
    pub fn subscribe_notifications(&self) -> broadcast::Receiver<Value> {
        self.notifications.subscribe()
    }

    pub async fn connect(&self) -> Result<()> {
        let _lock = self.lifecycle.lock().await;
        if self.is_connected().await {
            return Ok(());
        }
        *self.state.write().await = ConnectionState::Connecting;
        let result = tokio::time::timeout(self.timeout, self.connect_inner())
            .await
            .unwrap_or_else(|_| Err(anyhow!("MCP initialization timed out")));
        if result.is_ok() {
            *self.state.write().await = ConnectionState::Connected;
        } else {
            if let Some(transport) = self.stdio.write().await.take() {
                transport.close();
            }
            *self.state.write().await = ConnectionState::Failed;
            *self.server_info.write().await = None;
            self.cached_tools.write().await.clear();
            self.cached_resources.write().await.clear();
        }
        result
    }
    async fn connect_inner(&self) -> Result<()> {
        match self.config.transport {
            TransportType::Stdio => {
                *self.stdio.write().await = Some(Arc::new(StdioTransport::spawn(
                    &self.config,
                    self.notifications.clone(),
                )?));
            }
            TransportType::Http => {
                self.http.validate_url()?;
            }
            TransportType::Sse => bail!(
                "Legacy MCP SSE transport is unsupported; explicitly configure Streamable HTTP"
            ),
            TransportType::WebSocket => bail!("MCP WebSocket transport is unsupported"),
        }
        let mut params = InitializeParams::default();
        if self.config.transport == TransportType::Http {
            params.protocol_version = "2025-03-26".into();
        }
        let info: InitializeResult = self
            .request("initialize", Some(serde_json::to_value(params)?))
            .await?;
        if !SUPPORTED_PROTOCOL_VERSIONS.contains(&info.protocol_version.as_str()) {
            bail!("MCP server selected an unsupported protocol version");
        }
        if self.config.transport == TransportType::Http && info.protocol_version != "2025-03-26" {
            bail!("Streamable HTTP requires MCP 2025-03-26");
        }
        self.http.set_version(&info.protocol_version).await;
        *self.server_info.write().await = Some(info.clone());
        self.notify("notifications/initialized", None).await?;
        if info.capabilities.tools.is_some() {
            self.refresh_tools().await?;
        }
        if info.capabilities.resources.is_some() {
            self.refresh_resources().await?;
        }
        if info.capabilities.prompts.is_some() {
            self.list_prompts().await?;
        }
        Ok(())
    }
    pub async fn disconnect(&self) -> Result<()> {
        let _lock = self.lifecycle.lock().await;
        if let Some(transport) = self.stdio.write().await.take() {
            transport.close();
        }
        let result = if self.config.transport == TransportType::Http {
            self.http.close().await
        } else {
            Ok(())
        };
        *self.state.write().await = ConnectionState::Disconnected;
        *self.server_info.write().await = None;
        self.cached_tools.write().await.clear();
        self.cached_resources.write().await.clear();
        result
    }
    pub async fn refresh_tools(&self) -> Result<Vec<Tool>> {
        let tools = self.list("tools/list", "tools").await?;
        *self.cached_tools.write().await = tools.clone();
        self.tools_dirty.store(false, Ordering::Release);
        Ok(tools)
    }
    /// Fresh tool schemas after list-changed notifications; failures are not hidden.
    pub async fn discovered_tools(&self) -> Result<Vec<Tool>> {
        let mut changed = false;
        let mut rx = self.changes.lock().await;
        loop {
            match rx.try_recv() {
                Ok(v) => {
                    changed |= v.get("method").and_then(Value::as_str)
                        == Some("notifications/tools/list_changed")
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => changed = true,
                Err(_) => break,
            }
        }
        drop(rx);
        if changed {
            self.tools_dirty.store(true, Ordering::Release);
        }
        if self.tools_dirty.load(Ordering::Acquire) {
            self.refresh_tools().await
        } else {
            Ok(self.tools().await)
        }
    }
    pub async fn refresh_resources(&self) -> Result<Vec<Resource>> {
        let resources = self.list("resources/list", "resources").await?;
        *self.cached_resources.write().await = resources.clone();
        Ok(resources)
    }
    pub async fn list_prompts(&self) -> Result<Vec<Prompt>> {
        self.list("prompts/list", "prompts").await
    }
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<Value>,
    ) -> Result<GetPromptResult> {
        self.request("prompts/get", Some(named_arguments(name, arguments)?))
            .await
    }
    pub async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<CallToolResult> {
        if !self.is_connected().await {
            bail!("MCP server is not connected");
        }
        if !self
            .discovered_tools()
            .await?
            .iter()
            .any(|t| t.name == name)
        {
            bail!("MCP tool is not available");
        }
        self.request("tools/call", Some(named_arguments(name, arguments)?))
            .await
    }
    pub async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult> {
        self.request("resources/read", Some(json!({"uri":uri})))
            .await
    }
    async fn list<T: DeserializeOwned>(&self, method: &str, field: &str) -> Result<Vec<T>> {
        let mut result = Vec::new();
        let mut cursor: Option<String> = None;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..100 {
            let page: Value = self
                .request(method, cursor.as_ref().map(|c| json!({"cursor":c})))
                .await?;
            let values = page
                .get(field)
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("Invalid MCP list response"))?;
            if result.len() + values.len() > 10_000 {
                bail!("MCP discovery limit exceeded");
            }
            for value in values {
                result.push(serde_json::from_value(value.clone())?);
            }
            cursor = page
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let Some(next) = &cursor else {
                return Ok(result);
            };
            if !seen.insert(next.clone()) {
                bail!("Repeated MCP pagination cursor");
            }
        }
        bail!("MCP pagination limit exceeded")
    }
    async fn request<T: DeserializeOwned>(&self, method: &str, params: Option<Value>) -> Result<T> {
        let mut request =
            JsonRpcRequest::new(self.request_id.fetch_add(1, Ordering::Relaxed), method);
        request.params = params;
        let response: JsonRpcResponse = match self.config.transport {
            TransportType::Stdio => {
                let transport = self
                    .stdio
                    .read()
                    .await
                    .clone()
                    .ok_or_else(|| anyhow!("MCP server is not connected"))?;
                transport.request(&request, self.timeout).await?
            }
            TransportType::Http => tokio::time::timeout(
                self.timeout,
                self.http.request(&request, &self.notifications),
            )
            .await
            .map_err(|_| anyhow!("MCP request timed out"))??,
            _ => bail!("Unsupported MCP transport"),
        };
        if response.jsonrpc != "2.0" || response.id != request.id {
            bail!("MCP response correlation failed");
        }
        if let Some(error) = response.error {
            bail!("MCP server rejected {method} (code {})", error.code);
        }
        serde_json::from_value(response.result.unwrap_or(Value::Null))
            .map_err(|_| anyhow!("Invalid MCP response payload"))
    }
    async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        let value = serde_json::to_value(cortex_mcp_types::JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
        })?;
        match self.config.transport {
            TransportType::Stdio => {
                self.stdio
                    .read()
                    .await
                    .clone()
                    .ok_or_else(|| anyhow!("MCP server is not connected"))?
                    .notify(value)
                    .await
            }
            TransportType::Http => self.http.notify(&value).await,
            _ => bail!("Unsupported MCP transport"),
        }
    }
}
fn named_arguments(name: &str, arguments: Option<Value>) -> Result<Value> {
    let mut params = json!({"name":name});
    if let Some(arguments) = arguments {
        if !arguments.is_object() {
            bail!("MCP arguments must be an object");
        }
        params["arguments"] = arguments;
    }
    Ok(params)
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "http_tests.rs"]
mod http_tests;
