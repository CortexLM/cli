//! Supervised trusted-code Node host. A child process is explicitly NOT a sandbox.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::RwLock;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::contract::{InvocationResult, MAX_FRAME_BYTES, Notification, PROTOCOL_VERSION};
use crate::manifest::PluginPermission;
use crate::{Plugin, PluginContext, PluginError, PluginInfo, PluginManifest, PluginState, Result};

pub const HOST_SOURCE: &str = include_str!("../../../packages/plugin-host/host.mjs");
pub const BUILD_SOURCE: &str = include_str!("../../../packages/plugin-host/build.mjs");

struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_id: u64,
    pid: Option<u32>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        // kill_on_drop covers the direct child. On Unix, also clean up its group.
        // Trusted plugins can deliberately escape a process group; this is not isolation.
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            let _ = std::process::Command::new("node")
                .env_clear()
                .arg("--input-type=module")
                .arg("--eval")
                .arg("try { process.kill(-Number(process.argv[1]), 'SIGKILL'); } catch (e) { if(e.code !== 'ESRCH') process.exitCode=1; }")
                .arg(pid.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = self.child.start_kill();
    }
}

impl Worker {
    fn spawn(root: &Path) -> Result<Self> {
        let mut command = Command::new("node");
        command
            .env_clear()
            .current_dir(root)
            .arg("--max-old-space-size=128")
            .arg("--input-type=module")
            .arg("--eval")
            .arg(HOST_SOURCE)
            .arg("--")
            .arg("--cortex-plugin-host")
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command.spawn().map_err(|_| {
            PluginError::load_error(
                "node",
                "Cannot launch Node 22.13+; install the supported Node 22 LTS runtime",
            )
        })?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| PluginError::load_error("node", "Missing host input"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| PluginError::load_error("node", "Missing host output"))?;
        let pid = child.id();
        Ok(Self {
            child,
            input,
            output: BufReader::new(output),
            next_id: 1,
            pid,
        })
    }

    async fn exchange(&mut self, mut request: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        request["protocol"] = json!(PROTOCOL_VERSION);
        request["id"] = json!(id);
        let mut bytes = serde_json::to_vec(&request)?;
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(PluginError::validation_error(
                "request",
                "Plugin request exceeds 1 MiB",
            ));
        }
        bytes.push(b'\n');
        self.input.write_all(&bytes).await?;
        self.input.flush().await?;
        let bytes = read_frame(&mut self.output).await?;
        let reply: Value = serde_json::from_slice(&bytes)?;
        if reply["protocol"] != PROTOCOL_VERSION || reply["id"] != id {
            return Err(PluginError::execution_error(
                "node",
                "Plugin host protocol mismatch",
            ));
        }
        if reply.get("error").is_some() {
            return Err(PluginError::execution_error(
                "node",
                "Plugin request failed",
            ));
        }
        reply
            .get("result")
            .cloned()
            .ok_or_else(|| PluginError::execution_error("node", "Plugin host returned no result"))
    }
}

async fn read_frame(reader: &mut BufReader<ChildStdout>) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            return Err(PluginError::execution_error(
                "node",
                "Plugin host exited before replying",
            ));
        }
        let end = buffer.iter().position(|b| *b == b'\n');
        let length = end.map_or(buffer.len(), |n| n + 1);
        if bytes.len() + length > MAX_FRAME_BYTES + 1 {
            return Err(PluginError::execution_error(
                "node",
                "Plugin host output exceeds 1 MiB",
            ));
        }
        bytes.extend_from_slice(&buffer[..length]);
        reader.consume(length);
        if end.is_some() {
            return Ok(bytes);
        }
    }
}

pub struct NodePlugin {
    info: PluginInfo,
    manifest: PluginManifest,
    state: PluginState,
    entrypoint: PathBuf,
    fingerprint: String,
    worker: Mutex<Option<Worker>>,
    config: RwLock<HashMap<String, Value>>,
    notifications: Mutex<Vec<Notification>>,
}

impl NodePlugin {
    /// The caller must supply the exact package fingerprint the user explicitly trusted.
    pub fn new(manifest: PluginManifest, root: PathBuf, trusted_hash: &str) -> Result<Self> {
        let fingerprint = crate::package::fingerprint(&root)?;
        if trusted_hash != fingerprint {
            return Err(PluginError::PermissionDenied(
                "JavaScript plugins execute native code, not sandboxed code. Explicit artifact trust is required".into(),
            ));
        }
        let entrypoint = crate::package::confined_file(&root, &manifest.runtime.entrypoint)?;
        Ok(Self {
            info: PluginInfo::from_manifest(&manifest, root),
            manifest,
            state: PluginState::Loaded,
            entrypoint,
            fingerprint,
            worker: Mutex::new(None),
            config: RwLock::new(HashMap::new()),
            notifications: Mutex::new(Vec::new()),
        })
    }

    async fn rpc(&self, request: Value) -> Result<Value> {
        let mut guard = self.worker.lock().await;
        let mut worker = guard.take().ok_or_else(|| {
            PluginError::execution_error(
                &self.info.id,
                "Plugin host is stopped; explicitly reload to restart",
            )
        })?;
        let result = tokio::time::timeout(
            Duration::from_millis(self.manifest.runtime.timeout_ms),
            worker.exchange(request),
        )
        .await
        .unwrap_or_else(|_| Err(PluginError::Timeout(self.info.id.clone())));
        if result.is_ok() {
            *guard = Some(worker);
        }
        result
    }

    async fn lifecycle(&self, method: &str) -> Result<()> {
        let value = self
            .rpc(json!({
                "method": method,
                "context": PluginContext::new(&self.info.path).with_plugin(&self.info.id),
            }))
            .await?;
        let result: InvocationResult = serde_json::from_value(value)?;
        result.validate(
            self.manifest
                .has_permission(&PluginPermission::Notifications),
        )?;
        self.notifications.lock().await.extend(result.notifications);
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        if crate::package::fingerprint(&self.info.path)? != self.fingerprint {
            return Err(PluginError::PermissionDenied(
                "Plugin changed after trust was granted".into(),
            ));
        }
        *self.worker.lock().await = Some(Worker::spawn(&self.info.path)?);
        let sha256 = crate::package::artifact_hash(&self.entrypoint)?;
        let hooks: Vec<_> = self
            .manifest
            .hooks
            .iter()
            .map(|h| {
                json!({
                    "function": hook_function(h),
                })
            })
            .collect();
        let reply = self.rpc(json!({
            "method": "handshake",
            "entrypoint": self.entrypoint,
            "sha256": sha256,
            "cli_version": crate::VERSION,
            "manifest": {
                "id": self.info.id, "version": self.info.version,
                "commands": self.manifest.commands, "tools": self.manifest.tools, "hooks": hooks,
                "capabilities": self.manifest.capabilities,
            },
        })).await?;
        if reply
            != json!({"protocol": PROTOCOL_VERSION, "id": self.info.id,
            "version": self.info.version, "sha256": sha256})
        {
            return Err(PluginError::execution_error(
                &self.info.id,
                "Plugin handshake mismatch",
            ));
        }
        self.lifecycle("init").await
    }
}

pub fn hook_function(hook: &crate::manifest::PluginHookManifest) -> String {
    hook.function
        .clone()
        .unwrap_or_else(|| format!("hook_{}", hook.hook_type.to_string().replace('.', "_")))
}

#[async_trait::async_trait]
impl Plugin for NodePlugin {
    fn info(&self) -> &PluginInfo {
        &self.info
    }
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
    fn state(&self) -> PluginState {
        if self.state == PluginState::Active
            && self.worker.try_lock().is_ok_and(|worker| worker.is_none())
        {
            PluginState::Error
        } else {
            self.state
        }
    }

    async fn init(&mut self) -> Result<()> {
        if !matches!(
            self.state(),
            PluginState::Loaded | PluginState::Unloaded | PluginState::Error
        ) {
            return Err(PluginError::init_error(
                &self.info.id,
                "Plugin cannot initialize in its current state",
            ));
        }
        let result = self.start().await;
        self.state = if result.is_ok() {
            PluginState::Active
        } else {
            PluginState::Error
        };
        if result.is_err() {
            self.worker.lock().await.take();
        }
        result
    }

    async fn shutdown(&mut self) -> Result<()> {
        let result = if self.worker.lock().await.is_some() {
            self.lifecycle("shutdown").await
        } else {
            Ok(())
        };
        self.worker.lock().await.take();
        self.state = if result.is_ok() {
            PluginState::Unloaded
        } else {
            PluginState::Error
        };
        result
    }

    async fn invoke(
        &self,
        method: &str,
        name: &str,
        input: Value,
        ctx: &PluginContext,
    ) -> Result<InvocationResult> {
        if self.state() != PluginState::Active {
            return Err(PluginError::Disabled(self.info.id.clone()));
        }
        let value = self
            .rpc(json!({
                "method": method, "name": name, "input": input,
                "context": ctx.clone().with_plugin(&self.info.id),
            }))
            .await?;
        let result = serde_json::from_value::<InvocationResult>(value)
            .map_err(PluginError::from)
            .and_then(|result| {
                result.validate(
                    self.manifest
                        .has_permission(&PluginPermission::Notifications),
                )?;
                Ok(result)
            });
        if result.is_err() {
            self.worker.lock().await.take();
        }
        result
    }

    async fn execute_command(
        &self,
        name: &str,
        args: Vec<String>,
        ctx: &PluginContext,
    ) -> Result<String> {
        let result = self.invoke("command", name, json!(args), ctx).await?;
        self.notifications.lock().await.extend(result.notifications);
        Ok(match result.data {
            Value::String(s) => s,
            value => value.to_string(),
        })
    }

    async fn take_notifications(&self) -> Vec<Notification> {
        std::mem::take(&mut *self.notifications.lock().await)
    }
    fn get_config(&self, key: &str) -> Option<Value> {
        self.config.read().ok()?.get(key).cloned()
    }
    fn set_config(&mut self, key: &str, value: Value) -> Result<()> {
        self.config
            .write()
            .map_err(|_| PluginError::ConfigError("Plugin config lock failed".into()))?
            .insert(key.into(), value);
        Ok(())
    }
}
