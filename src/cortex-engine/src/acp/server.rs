//! Bounded, concurrent ACP v1 stdio transport. Network listeners are unsupported.
use anyhow::{Result, bail};
use serde_json::Value;
use std::{collections::HashSet, net::SocketAddr, sync::Arc};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinSet;

use crate::acp::handler::AcpHandler;
use crate::acp::protocol::{AcpError, AcpNotification, AcpRequest, AcpRequestId, AcpResponse};
use crate::config::Config;

pub(crate) const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

pub struct AcpServer {
    handler: Arc<AcpHandler>,
}

impl AcpServer {
    pub fn new(config: Config) -> Self {
        Self {
            handler: Arc::new(AcpHandler::new(config)),
        }
    }

    pub async fn run_stdio(&self) -> Result<()> {
        self.run_io(tokio::io::stdin(), tokio::io::stdout()).await
    }

    pub async fn run_http(&self, _addr: SocketAddr) -> Result<()> {
        // ponytail: stdio only until authenticated ingress exists; reject even loopback before bind.
        bail!("ACP network transport is unsupported; use stdio")
    }

    pub async fn run(&self) -> Result<()> {
        self.run_stdio().await
    }

    async fn run_io<R, W>(&self, reader: R, mut writer: W) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let mut reader = BufReader::new(reader);
        let (tx, mut rx) = mpsc::channel::<Value>(64);
        let mut notifications = self.handler.subscribe();
        let output = tokio::spawn(async move {
            loop {
                let value = tokio::select! {
                    biased;
                    event = notifications.recv() => match event {
                        Ok(event) => serde_json::to_value(AcpNotification::new(event.method).with_params(event.params))?,
                        Err(_) => bail!("ACP event subscription closed or lagged"),
                    },
                    value = rx.recv() => match value { Some(value) => value, None => break },
                };
                let mut bytes = serde_json::to_vec(&value)?;
                if bytes.len() > MAX_MESSAGE_BYTES {
                    bail!("ACP output exceeds message limit");
                }
                bytes.push(b'\n');
                tokio::time::timeout(std::time::Duration::from_secs(30), writer.write_all(&bytes))
                    .await??;
                writer.flush().await?;
            }
            Ok::<_, anyhow::Error>(())
        });
        let active = Arc::new(Mutex::new(HashSet::new()));
        let mut tasks = JoinSet::new();
        let mut initialized = false;
        let result = async {
            loop {
                let line = tokio::select! {
                    line = read_bounded_line(&mut reader) => line?,
                    _ = tx.closed() => bail!("ACP output closed"),
                };
                let Some(line) = line else { break };
                while tasks.try_join_next().is_some() {}
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                let request = match decode_frame(&line) {
                    Ok(request) => request,
                    Err(response) => {
                        send_response(&tx, response).await?;
                        continue;
                    }
                };
                self.dispatch_request(request, &mut initialized, &active, &mut tasks, &tx)
                    .await?;
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        tasks.abort_all();
        self.handler.shutdown().await;
        drop(tx);
        let output_result = output.await?;
        result.and(output_result)
    }
    async fn dispatch_request(
        &self,
        request: AcpRequest,
        initialized: &mut bool,
        active: &Arc<Mutex<HashSet<AcpRequestId>>>,
        tasks: &mut JoinSet<()>,
        tx: &mpsc::Sender<Value>,
    ) -> Result<()> {
        let id = request.id.clone();
        if request.jsonrpc != "2.0" || matches!(id, Some(AcpRequestId::Null)) {
            send_response(
                tx,
                AcpResponse::error(
                    id.unwrap_or(AcpRequestId::Null),
                    AcpError::invalid_request(
                        "Expected JSON-RPC 2.0 and a string or integer request ID",
                    ),
                ),
            )
            .await?;
            return Ok(());
        }
        let params = request.params.unwrap_or(Value::Null);
        // Id-less cancellation is a notification, never a response with id:null.
        let Some(id) = id else {
            if *initialized && request.method == "session/cancel" {
                if let Ok(params) = serde_json::from_value(params) {
                    let _ = self.handler.handle_session_cancel(params).await;
                }
            }
            return Ok(());
        };
        if request.method != "initialize" && !*initialized {
            send_response(
                tx,
                AcpResponse::error(id, AcpError::invalid_request("Initialize first")),
            )
            .await?;
            return Ok(());
        }
        if active.lock().await.contains(&id) {
            send_response(
                tx,
                AcpResponse::error(
                    id,
                    AcpError::invalid_request("Duplicate in-flight request ID"),
                ),
            )
            .await?;
            return Ok(());
        }
        if request.method == "session/prompt" {
            if tasks.len() >= 32 {
                send_response(
                    &tx,
                    AcpResponse::error(id, AcpError::invalid_request("Too many active requests")),
                )
                .await?;
                return Ok(());
            }
            let params = match serde_json::from_value(params) {
                Ok(params) => params,
                Err(_) => {
                    send_response(
                        &tx,
                        AcpResponse::error(
                            id,
                            AcpError::invalid_params("Invalid prompt parameters"),
                        ),
                    )
                    .await?;
                    return Ok(());
                }
            };
            // Reserve the session turn before reading the next frame, including cancel.
            let receiver = match self.handler.begin_prompt(params).await {
                Ok(receiver) => receiver,
                Err(_) => {
                    send_response(
                        &tx,
                        AcpResponse::error(
                            id,
                            AcpError::invalid_params(
                                "Session unavailable, busy, or unsupported prompt content",
                            ),
                        ),
                    )
                    .await?;
                    return Ok(());
                }
            };
            active.lock().await.insert(id.clone());
            let (tx, active) = (tx.clone(), active.clone());
            tasks.spawn(async move {
                let response = match receiver.await {
                    Ok(Ok(reason)) => {
                        AcpResponse::success(id.clone(), serde_json::json!({"stopReason":reason}))
                    }
                    _ => AcpResponse::error(
                        id.clone(),
                        AcpError::internal("The coding service is temporarily unavailable"),
                    ),
                };
                let _ = send_response(&tx, response).await;
                active.lock().await.remove(&id);
            });
        } else {
            let response = self
                .handler
                .process_request(id, &request.method, params)
                .await;
            if request.method == "initialize" {
                *initialized = response.error.is_none();
            }
            send_response(tx, response).await?;
        }
        Ok(())
    }
}

fn decode_frame(line: &[u8]) -> std::result::Result<AcpRequest, AcpResponse> {
    let value: Value = serde_json::from_slice(line).map_err(|_| {
        AcpResponse::error(AcpRequestId::Null, AcpError::parse_error("Invalid JSON"))
    })?;
    let id = value
        .get("id")
        .and_then(|id| serde_json::from_value(id.clone()).ok())
        .unwrap_or(AcpRequestId::Null);
    if value.get("id").is_some_and(Value::is_null) {
        return Err(AcpResponse::error(
            AcpRequestId::Null,
            AcpError::invalid_request("Request IDs must be strings or integers"),
        ));
    }
    serde_json::from_value(value)
        .map_err(|_| AcpResponse::error(id, AcpError::invalid_request("Invalid JSON-RPC request")))
}

async fn send_response(tx: &mpsc::Sender<Value>, response: AcpResponse) -> Result<()> {
    tx.send(serde_json::to_value(response)?)
        .await
        .map_err(|_| anyhow::anyhow!("ACP output closed"))
}

/// Read without allocating beyond the frame limit; EOF mid-frame is accepted as a last JSON line.
pub(crate) async fn read_bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            return Ok((!line.is_empty()).then_some(line));
        }
        let count = buffer
            .iter()
            .position(|b| *b == b'\n')
            .map_or(buffer.len(), |i| i + 1);
        if line.len() + count > MAX_MESSAGE_BYTES {
            bail!("Protocol message exceeds 1 MiB limit");
        }
        let complete = buffer[count - 1] == b'\n';
        line.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if complete {
            return Ok(Some(line));
        }
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
