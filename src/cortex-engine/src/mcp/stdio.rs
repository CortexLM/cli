//! A persistent, bounded bidirectional MCP dispatcher for the active runtime.
use super::McpServerConfig;
use crate::acp::server::{MAX_MESSAGE_BYTES, read_bounded_line};
use anyhow::{Result, anyhow, bail};
use cortex_mcp_types::{JsonRpcRequest, JsonRpcResponse, RequestId};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

type Pending = Arc<Mutex<HashMap<RequestId, oneshot::Sender<Result<JsonRpcResponse>>>>>;

pub(super) struct StdioTransport {
    outgoing: mpsc::Sender<Value>,
    pending: Pending,
    reader: JoinHandle<()>,
    writer: JoinHandle<()>,
    child: JoinHandle<()>,
}

impl StdioTransport {
    pub fn spawn(
        config: &McpServerConfig,
        notifications: broadcast::Sender<Value>,
    ) -> Result<Self> {
        let mut cmd = tokio::process::Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .env_clear();
        // Only essential process context is inherited; all secrets must be explicitly configured.
        for key in [
            "PATH",
            "HOME",
            "USER",
            "TMPDIR",
            "LANG",
            "LC_ALL",
            "SYSTEMROOT",
        ] {
            if let Some(value) = std::env::var_os(key) {
                cmd.env(key, value);
            }
        }
        cmd.envs(&config.env);
        if let Some(cwd) = &config.cwd {
            cmd.current_dir(cwd);
        }
        let mut child = cmd
            .spawn()
            .map_err(|_| anyhow!("Could not start configured MCP server"))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("MCP input unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("MCP output unavailable"))?;
        let (outgoing, mut input) = mpsc::channel::<Value>(64);
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let writer_pending = pending.clone();
        let writer = tokio::spawn(async move {
            while let Some(value) = input.recv().await {
                let Ok(mut data) = serde_json::to_vec(&value) else {
                    break;
                };
                if data.len() > MAX_MESSAGE_BYTES {
                    break;
                }
                data.push(b'\n');
                if !matches!(
                    tokio::time::timeout(Duration::from_secs(30), stdin.write_all(&data)).await,
                    Ok(Ok(()))
                ) {
                    break;
                }
            }
            fail_pending(&writer_pending);
        });
        let reader_pending = pending.clone();
        let replies = outgoing.clone();
        let reader = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(line)) = read_bounded_line(&mut reader).await {
                let Ok(value) = serde_json::from_slice::<Value>(&line) else {
                    break;
                };
                if dispatch(value, &reader_pending, &replies, &notifications).is_err() {
                    break;
                }
            }
            fail_pending(&reader_pending);
        });
        let child = tokio::spawn(async move {
            let _ = child.wait().await;
        });
        Ok(Self {
            outgoing,
            pending,
            reader,
            writer,
            child,
        })
    }

    pub fn is_open(&self) -> bool {
        !self.reader.is_finished() && !self.writer.is_finished()
    }
    pub fn close(&self) {
        self.reader.abort();
        self.writer.abort();
        self.child.abort();
        fail_pending(&self.pending);
    }

    pub async fn request(
        &self,
        request: &JsonRpcRequest,
        timeout: Duration,
    ) -> Result<JsonRpcResponse> {
        if self.reader.is_finished() || self.writer.is_finished() {
            bail!("MCP connection closed");
        }
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().expect("MCP pending mutex");
            if pending.len() >= 32 {
                bail!("MCP concurrent request limit reached");
            }
            pending.insert(request.id.clone(), tx);
        }
        // Dropping a request future also cancels its registration and informs the peer.
        let guard = PendingGuard {
            id: request.id.clone(),
            pending: self.pending.clone(),
            outgoing: self.outgoing.clone(),
        };
        let result = tokio::time::timeout(timeout, async {
            self.notify(serde_json::to_value(request)?).await?;
            rx.await.map_err(|_| anyhow!("MCP connection closed"))?
        })
        .await
        .map_err(|_| anyhow!("MCP request timed out"))?;
        drop(guard);
        result
    }

    pub async fn notify(&self, value: Value) -> Result<()> {
        if serde_json::to_vec(&value)?.len() > MAX_MESSAGE_BYTES {
            bail!("MCP message exceeds 1 MiB limit");
        }
        self.outgoing
            .send(value)
            .await
            .map_err(|_| anyhow!("MCP connection closed"))
    }
}

struct PendingGuard {
    id: RequestId,
    pending: Pending,
    outgoing: mpsc::Sender<Value>,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        if self
            .pending
            .lock()
            .expect("MCP pending mutex")
            .remove(&self.id)
            .is_some()
        {
            let _ = self.outgoing.try_send(json!({"jsonrpc":"2.0", "method":"notifications/cancelled", "params":{"requestId":self.id}}));
        }
    }
}
impl Drop for StdioTransport {
    fn drop(&mut self) {
        self.reader.abort();
        self.writer.abort();
        self.child.abort();
        fail_pending(&self.pending);
    }
}
fn fail_pending(pending: &Pending) {
    for (_, tx) in pending.lock().expect("MCP pending mutex").drain() {
        let _ = tx.send(Err(anyhow!("MCP connection closed")));
    }
}

fn dispatch(
    value: Value,
    pending: &Pending,
    replies: &mpsc::Sender<Value>,
    notifications: &broadcast::Sender<Value>,
) -> Result<()> {
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        bail!("Invalid MCP protocol envelope");
    }
    if let Some(method) = value.get("method").and_then(Value::as_str) {
        if let Some(id) = value.get("id") {
            let id: RequestId = serde_json::from_value(id.clone())?;
            let response = if method == "ping" {
                json!({"jsonrpc":"2.0","id":id,"result":{}})
            } else {
                // No sampling, roots or elicitation capabilities were advertised.
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Client method not supported"}})
            };
            replies
                .try_send(response)
                .map_err(|_| anyhow!("MCP response queue full"))?;
        } else {
            let _ = notifications.send(value);
        }
        return Ok(());
    }
    if value.get("result").is_some() == value.get("error").is_some() {
        bail!("Invalid MCP response");
    }
    let response: JsonRpcResponse = serde_json::from_value(value)?;
    // Late responses after cancellation are ignored, not assigned to a newer request.
    if let Some(tx) = pending
        .lock()
        .expect("MCP pending mutex")
        .remove(&response.id)
    {
        let _ = tx.send(Ok(response));
    }
    Ok(())
}
