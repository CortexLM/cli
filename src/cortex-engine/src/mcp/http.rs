//! Explicitly configured Streamable HTTP endpoints. Redirects never inherit credentials.
use super::{McpServerConfig, oauth::stored_access_token};
use crate::acp::server::MAX_MESSAGE_BYTES;
use anyhow::{Result, anyhow, bail};
use cortex_mcp_types::{JsonRpcRequest, JsonRpcResponse};
use serde_json::Value;
use tokio::sync::{RwLock, broadcast};

pub(super) struct HttpTransport {
    config: McpServerConfig,
    client: reqwest::Client,
    session: RwLock<Option<String>>,
    version: RwLock<String>,
}
impl HttpTransport {
    pub fn new(config: McpServerConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("MCP HTTP client"),
            session: RwLock::new(None),
            version: RwLock::new("2025-03-26".into()),
        }
    }
    fn url(&self) -> Result<&str> {
        self.config
            .sse_post_url
            .as_deref()
            .or(self.config.sse_url.as_deref())
            .ok_or_else(|| anyhow!("MCP HTTP URL must be explicitly configured"))
    }
    pub fn validate_url(&self) -> Result<()> {
        validate_endpoint(self.url()?).map(|_| ())
    }
    pub async fn set_version(&self, version: &str) {
        *self.version.write().await = version.into();
    }
    async fn builder(&self, method: reqwest::Method) -> Result<reqwest::RequestBuilder> {
        self.validate_url()?;
        let url = self.url()?;
        let mut req = self.client.request(method, url);
        let mut explicit_auth = false;
        for (key, raw) in &self.config.headers {
            let value = if let Some(var) = raw.strip_prefix("env:") {
                std::env::var(var).map_err(|_| {
                    anyhow!("Configured MCP header environment variable is unavailable")
                })?
            } else {
                raw.clone()
            };
            explicit_auth |= key.eq_ignore_ascii_case("authorization");
            req = req.header(key, value);
        }
        if let Some(var) = &self.config.bearer_token_env_var {
            let token = std::env::var(var)
                .ok()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow!("Configured MCP bearer token is unavailable"))?;
            req = req.bearer_auth(token);
            explicit_auth = true;
        }
        if !explicit_auth {
            if let Some(token) = stored_access_token(&self.config.name, url).await? {
                req = req.bearer_auth(token);
            }
        }
        req = req
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", self.version.read().await.as_str());
        if let Some(session) = &*self.session.read().await {
            req = req.header("Mcp-Session-Id", session);
        }
        Ok(req)
    }
    pub async fn request(
        &self,
        request: &JsonRpcRequest,
        notifications: &broadcast::Sender<Value>,
    ) -> Result<JsonRpcResponse> {
        if serde_json::to_vec(request)?.len() > MAX_MESSAGE_BYTES {
            bail!("MCP request exceeds 1 MiB limit");
        }
        let response = self
            .builder(reqwest::Method::POST)
            .await?
            .json(request)
            .send()
            .await
            .map_err(|_| anyhow!("MCP HTTP request failed"))?;
        check_status(&response)?;
        if request.method == "initialize" {
            if let Some(value) = response.headers().get("mcp-session-id") {
                let value = value
                    .to_str()
                    .map_err(|_| anyhow!("Invalid MCP session header"))?;
                if value.is_empty()
                    || value.len() > 256
                    || !value.bytes().all(|b| (0x21..=0x7e).contains(&b))
                {
                    bail!("Invalid MCP session header");
                }
                *self.session.write().await = Some(value.to_owned());
            }
        }
        read_response(response, notifications).await
    }
    pub async fn notify(&self, value: &Value) -> Result<()> {
        let response = self
            .builder(reqwest::Method::POST)
            .await?
            .json(value)
            .send()
            .await
            .map_err(|_| anyhow!("MCP notification failed"))?;
        check_status(&response)
    }
    pub async fn close(&self) -> Result<()> {
        if self.session.read().await.is_none() {
            return Ok(());
        }
        let result = async {
            let response = self
                .builder(reqwest::Method::DELETE)
                .await?
                .send()
                .await
                .map_err(|_| anyhow!("MCP session close failed"))?;
            // Servers may explicitly decline client-initiated session termination.
            if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED
                || response.status() == reqwest::StatusCode::NOT_FOUND
            {
                return Ok(());
            }
            check_status(&response)
        }
        .await;
        *self.session.write().await = None;
        result
    }
}

pub(super) fn validate_endpoint(raw: &str) -> Result<url::Url> {
    let url = url::Url::parse(raw).map_err(|_| anyhow!("Invalid MCP endpoint"))?;
    let loopback = url.host_str().is_some_and(|h| {
        h == "localhost"
            || h.trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        bail!("MCP endpoints require HTTPS (HTTP is allowed only on loopback)");
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        bail!("Invalid MCP endpoint credentials or fragment");
    }
    Ok(url)
}
fn check_status(response: &reqwest::Response) -> Result<()> {
    if !response.status().is_success() {
        bail!(
            "MCP HTTP request rejected (status {})",
            response.status().as_u16()
        );
    }
    Ok(())
}
async fn read_response(
    mut response: reqwest::Response,
    notifications: &broadcast::Sender<Value>,
) -> Result<JsonRpcResponse> {
    let kind = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("");
    let sse = match kind {
        "application/json" => false,
        "text/event-stream" => true,
        _ => bail!("Unsupported MCP response content type"),
    };
    if response
        .content_length()
        .is_some_and(|n| n > MAX_MESSAGE_BYTES as u64)
    {
        bail!("MCP response exceeds 1 MiB limit");
    }
    let mut buffer = Vec::new();
    let mut total = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow!("MCP response stream failed"))?
    {
        total += chunk.len();
        if total > MAX_MESSAGE_BYTES {
            bail!("MCP response exceeds 1 MiB limit");
        }
        buffer.extend_from_slice(&chunk);
        if sse {
            // Normalize CRLF only after complete UTF-8 frames, preserving split chunks.
            while let Some((end, separator)) = frame_end(&buffer) {
                let frame = std::str::from_utf8(&buffer[..end])
                    .map_err(|_| anyhow!("Invalid MCP event encoding"))?;
                let data = frame
                    .lines()
                    .filter_map(|line| {
                        line.strip_prefix("data:")
                            .map(|s| s.strip_prefix(' ').unwrap_or(s))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let parsed = if data.is_empty() {
                    None
                } else {
                    Some(
                        serde_json::from_str::<Value>(&data)
                            .map_err(|_| anyhow!("Invalid MCP event"))?,
                    )
                };
                buffer.drain(..end + separator);
                if let Some(value) = parsed {
                    if value.get("method").is_some() && value.get("id").is_none() {
                        let _ = notifications.send(value);
                    } else {
                        return parse_response(value);
                    }
                }
            }
        }
    }
    if sse {
        bail!("MCP event stream closed without a response");
    }
    parse_response(serde_json::from_slice(&buffer).map_err(|_| anyhow!("Invalid MCP response"))?)
}
fn frame_end(buffer: &[u8]) -> Option<(usize, usize)> {
    (0..buffer.len()).find_map(|i| {
        if buffer[i..].starts_with(b"\r\n\r\n") {
            Some((i, 4))
        } else if buffer[i..].starts_with(b"\n\n") {
            Some((i, 2))
        } else {
            None
        }
    })
}
fn parse_response(value: Value) -> Result<JsonRpcResponse> {
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || value.get("result").is_some() == value.get("error").is_some()
        || value.get("method").is_some()
    {
        bail!("Invalid MCP response envelope");
    }
    serde_json::from_value(value).map_err(|_| anyhow!("Invalid MCP response envelope"))
}
