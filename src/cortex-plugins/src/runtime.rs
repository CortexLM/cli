//! Persistent protocol-1 WASM runtime with fuel, memory and epoch deadlines.
use crate::api::PluginContext;
use crate::contract::{InvocationResult, MAX_FRAME_BYTES, Notification};
use crate::host::{self, HasHostState, PluginHostState};
use crate::manifest::{PluginManifest, PluginPermission};
use crate::plugin::{Plugin, PluginInfo, PluginState};
use crate::{PluginError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use wasmtime::*;

const DEFAULT_FUEL_LIMIT: u64 = 10_000_000;
const MAX_MEMORY_SIZE: usize = 16 * 1024 * 1024;
const MAX_TABLE_ELEMENTS: u64 = 10_000;
const MAX_INSTANCES: u32 = 10;
const MAX_TABLES: u32 = 10;
const MAX_MEMORIES: u32 = 1;

pub struct WasmRuntime {
    engine: Engine,
    linker: Linker<PluginStoreState>,
    stop: Arc<AtomicBool>,
    ticker: Option<std::thread::JoinHandle<()>>,
}
impl WasmRuntime {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config)?;
        let linker = host::create_linker::<PluginStoreState>(&engine)?;
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let timer_engine = engine.clone();
        let ticker = std::thread::Builder::new()
            .name("plugin-deadlines".into())
            .spawn(move || {
                while !signal.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(10));
                    timer_engine.increment_epoch();
                }
            })?;
        Ok(Self {
            engine,
            linker,
            stop,
            ticker: Some(ticker),
        })
    }
    pub fn linker(&self) -> &Linker<PluginStoreState> {
        &self.linker
    }
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
    pub fn compile(&self, bytes: &[u8]) -> Result<Module> {
        Module::new(&self.engine, bytes)
            .map_err(|e| PluginError::compilation_error("module", e.to_string()))
    }
    pub fn compile_file(&self, path: &Path) -> Result<Module> {
        self.compile(&std::fs::read(path)?)
    }
}
impl Drop for WasmRuntime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(ticker) = self.ticker.take() {
            let _ = ticker.join();
        }
    }
}
struct InstanceState {
    store: Store<PluginStoreState>,
    instance: Instance,
}
pub struct WasmPlugin {
    info: PluginInfo,
    manifest: PluginManifest,
    state: PluginState,
    wasm_path: PathBuf,
    module: Option<Module>,
    instance: Arc<Mutex<Option<InstanceState>>>,
    failed: Arc<AtomicBool>,
    config: RwLock<HashMap<String, serde_json::Value>>,
    notifications: tokio::sync::Mutex<Vec<Notification>>,
    runtime: Arc<WasmRuntime>,
}
impl WasmPlugin {
    pub fn new(manifest: PluginManifest, path: PathBuf, runtime: Arc<WasmRuntime>) -> Result<Self> {
        let info = PluginInfo::from_manifest(&manifest, path.clone());
        let wasm_path = path.join(&manifest.runtime.entrypoint);
        Ok(Self {
            info,
            manifest,
            state: PluginState::Discovered,
            wasm_path,
            module: None,
            instance: Arc::new(Mutex::new(None)),
            failed: Arc::new(AtomicBool::new(false)),
            config: RwLock::new(HashMap::new()),
            notifications: tokio::sync::Mutex::new(Vec::new()),
            runtime,
        })
    }
    pub fn load(&mut self) -> Result<()> {
        let result = self.load_module();
        self.state = if result.is_ok() {
            PluginState::Loaded
        } else {
            PluginState::Error
        };
        result
    }
    fn load_module(&mut self) -> Result<()> {
        self.manifest.validate()?;
        if !self.wasm_path.is_file() {
            return Err(PluginError::load_error(
                &self.info.id,
                "WASM artifact not found",
            ));
        }
        self.wasm_path =
            crate::package::confined_file(&self.info.path, &self.manifest.runtime.entrypoint)?;
        if self.wasm_path.metadata()?.len() > crate::contract::MAX_ARTIFACT_BYTES {
            return Err(PluginError::load_error(
                &self.info.id,
                "WASM artifact exceeds 16 MiB",
            ));
        }
        let module = self.runtime.compile_file(&self.wasm_path)?;
        // Old UI/event imports have no rendering/broker contract. Refuse them rather
        // than accepting effects which would be silently discarded.
        for import in module.imports() {
            if import.module() != "cortex"
                || !["log", "input_len", "read_input", "set_result"].contains(&import.name())
            {
                return Err(PluginError::load_error(
                    &self.info.id,
                    "Unsupported host import; use protocol-1 input/results",
                ));
            }
        }
        let required = std::iter::once("init".to_string())
            .chain(std::iter::once("shutdown".to_string()))
            .chain(
                self.manifest
                    .commands
                    .iter()
                    .map(|c| format!("cmd_{}", c.name.replace('-', "_"))),
            )
            .chain(self.manifest.hooks.iter().map(crate::node::hook_function))
            .chain(
                self.manifest
                    .tools
                    .iter()
                    .map(|t| format!("tool_{}", t.name.replace('-', "_"))),
            );
        for name in required {
            let Some(ExternType::Func(function)) = module.get_export(&name) else {
                return Err(PluginError::load_error(
                    &self.info.id,
                    format!("Missing export: {name}"),
                ));
            };
            if function.params().len() != 0
                || function.results().len() != 1
                || !matches!(function.results().next(), Some(ValType::I32))
            {
                return Err(PluginError::load_error(
                    &self.info.id,
                    format!("Export must be () -> i32: {name}"),
                ));
            }
        }
        self.module = Some(module);
        Ok(())
    }
    async fn call(
        &self,
        name: &str,
        input: serde_json::Value,
        context: PluginContext,
    ) -> Result<(InvocationResult, PluginHostState)> {
        if self.failed.load(Ordering::Relaxed) {
            return Err(PluginError::execution_error(
                &self.info.id,
                "Plugin failed; reload before calling again",
            ));
        }
        let module = self
            .module
            .clone()
            .ok_or_else(|| PluginError::load_error(&self.info.id, "Plugin is not loaded"))?;
        let name = name.to_string();
        let id = self.info.id.clone();
        let runtime = self.runtime.clone();
        let instance = self.instance.clone();
        let failed = self.failed.clone();
        let timeout = self
            .manifest
            .wasm
            .timeout_ms
            .min(self.manifest.runtime.timeout_ms)
            .div_ceil(10)
            .max(1);
        let max_memory = self.manifest.wasm.memory_pages as usize * 65536;
        let context = context.with_plugin(&id);
        let payload =
            serde_json::to_vec(&serde_json::json!({"protocol":1,"input":input,"context":context}))?;
        if payload.len() > MAX_FRAME_BYTES {
            return Err(PluginError::validation_error(
                "input",
                "WASM input exceeds 1 MiB",
            ));
        }
        let dispatcher = tracing::dispatcher::get_default(Clone::clone);
        let task = move || {
            let mut guard = instance
                .lock()
                .map_err(|_| PluginError::execution_error(&id, "Plugin instance lock failed"))?;
            let result = invoke_instance(
                &mut guard, &runtime, &module, &id, &name, context, payload, timeout, max_memory,
            );
            if result.is_err() {
                guard.take();
                failed.store(true, Ordering::Relaxed);
            }
            result
        };
        let result = tokio::task::spawn_blocking(move || {
            tracing::dispatcher::with_default(&dispatcher, task)
        })
        .await
        .map_err(|_| PluginError::execution_error(&self.info.id, "WASM worker failed"))??;
        if let Err(error) = result.0.validate(
            self.manifest
                .has_permission(&PluginPermission::Notifications),
        ) {
            self.failed.store(true, Ordering::Relaxed);
            return Err(error);
        }
        Ok(result)
    }
    pub async fn call_function(&self, name: &str) -> Result<i32> {
        self.call_function_with_context(name, PluginContext::new(&self.info.path))
            .await
    }
    pub async fn call_function_with_context(
        &self,
        name: &str,
        context: PluginContext,
    ) -> Result<i32> {
        let (result, _) = self.call(name, serde_json::Value::Null, context).await?;
        self.notifications.lock().await.extend(result.notifications);
        Ok(0)
    }
    pub async fn call_and_get_state(
        &self,
        name: &str,
        context: PluginContext,
    ) -> Result<(i32, PluginHostState)> {
        let (_, state) = self.call(name, serde_json::Value::Null, context).await?;
        Ok((0, state))
    }
}
#[allow(clippy::too_many_arguments)]
fn invoke_instance(
    guard: &mut Option<InstanceState>,
    runtime: &WasmRuntime,
    module: &Module,
    id: &str,
    name: &str,
    context: PluginContext,
    input: Vec<u8>,
    timeout: u64,
    max_memory: usize,
) -> Result<(InvocationResult, PluginHostState)> {
    let error = |e: wasmtime::Error| PluginError::execution_error(id, e.to_string());
    if guard.is_none() {
        let mut state = PluginStoreState::new(PluginHostState::new(id, context.clone()));
        state.limits.max_memory = max_memory;
        let mut store = Store::new(runtime.engine(), state);
        store.limiter(|state| state);
        store.set_fuel(DEFAULT_FUEL_LIMIT).map_err(error)?;
        store.set_epoch_deadline(timeout);
        let instance = runtime
            .linker()
            .instantiate(&mut store, module)
            .map_err(error)?;
        *guard = Some(InstanceState { store, instance });
    }
    let state = guard
        .as_mut()
        .ok_or_else(|| PluginError::execution_error(id, "Missing WASM instance"))?;
    state.store.data_mut().host_state.context = context;
    state.store.data_mut().host_state.input = input;
    state.store.data_mut().host_state.result = None;
    state.store.set_fuel(DEFAULT_FUEL_LIMIT).map_err(error)?;
    state.store.set_epoch_deadline(timeout);
    let function = state
        .instance
        .get_typed_func::<(), i32>(&mut state.store, name)
        .map_err(error)?;
    let status = function.call(&mut state.store, ()).map_err(error)?;
    if status != 0 {
        return Err(PluginError::execution_error(
            id,
            format!("Export {name} failed with status {status}"),
        ));
    }
    let result = state.store.data_mut().host_state.result.take();
    let result = match result {
        Some(result) => result,
        None if name == "init" || name == "shutdown" => InvocationResult::default(),
        None => {
            return Err(PluginError::execution_error(
                id,
                "WASM operation did not set a protocol-1 result",
            ));
        }
    };
    Ok((result, state.store.data().host_state.clone()))
}

/// Store limits for WASM plugin execution.
///
/// SECURITY: Implements wasmtime's ResourceLimiter trait to enforce
/// memory and resource constraints on plugin execution.
#[derive(Debug, Clone)]
struct PluginStoreLimits {
    /// Current memory allocated by this store.
    memory_used: usize,
    max_memory: usize,
}

impl Default for PluginStoreLimits {
    fn default() -> Self {
        Self {
            memory_used: 0,
            max_memory: MAX_MEMORY_SIZE,
        }
    }
}

impl ResourceLimiter for PluginStoreLimits {
    /// Called when memory is being grown.
    ///
    /// # Security
    ///
    /// Enforces maximum memory limit of 16MB per plugin instance.
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // SECURITY: Check if the desired memory exceeds our limit
        if desired > self.max_memory {
            tracing::warn!(
                current_bytes = current,
                desired_bytes = desired,
                max_bytes = MAX_MEMORY_SIZE,
                "Plugin memory request denied: exceeds maximum allowed"
            );
            return Ok(false);
        }

        self.memory_used = desired;
        Ok(true)
    }

    /// Called when a table is being grown.
    ///
    /// # Security
    ///
    /// Enforces maximum table elements limit.
    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // SECURITY: Limit table size to prevent excessive memory usage
        if desired as u64 > MAX_TABLE_ELEMENTS {
            tracing::warn!(
                desired_elements = desired,
                max_elements = MAX_TABLE_ELEMENTS,
                "Plugin table growth denied: exceeds maximum allowed"
            );
            return Ok(false);
        }
        Ok(true)
    }

    /// Returns the maximum number of instances.
    fn instances(&self) -> usize {
        MAX_INSTANCES as usize
    }

    /// Returns the maximum number of tables.
    fn tables(&self) -> usize {
        MAX_TABLES as usize
    }

    /// Returns the maximum number of memories.
    fn memories(&self) -> usize {
        MAX_MEMORIES as usize
    }
}

/// Combined store state that includes both host state and resource limits.
#[derive(Debug, Clone)]
pub struct PluginStoreState {
    /// Host state for plugin communication
    pub host_state: PluginHostState,
    /// Resource limits
    limits: PluginStoreLimits,
}

impl PluginStoreState {
    /// Create a new store state with the given host state.
    pub fn new(host_state: PluginHostState) -> Self {
        Self {
            host_state,
            limits: PluginStoreLimits::default(),
        }
    }
}

impl HasHostState for PluginStoreState {
    fn host_state(&self) -> &PluginHostState {
        &self.host_state
    }
    fn host_state_mut(&mut self) -> &mut PluginHostState {
        &mut self.host_state
    }
}

impl ResourceLimiter for PluginStoreState {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.limits.memory_growing(current, desired, maximum)
    }
    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.limits.table_growing(current, desired, maximum)
    }
    fn instances(&self) -> usize {
        self.limits.instances()
    }
    fn tables(&self) -> usize {
        self.limits.tables()
    }
    fn memories(&self) -> usize {
        self.limits.memories()
    }
}

#[async_trait::async_trait]
impl Plugin for WasmPlugin {
    fn info(&self) -> &PluginInfo {
        &self.info
    }
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
    fn state(&self) -> PluginState {
        if self.failed.load(Ordering::Relaxed) {
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
                "Plugin must be loaded before initialization",
            ));
        }
        self.instance
            .lock()
            .map_err(|_| PluginError::execution_error(&self.info.id, "Instance lock failed"))?
            .take();
        self.failed.store(false, Ordering::Relaxed);
        let result = self.call_function("init").await;
        self.state = if result.is_ok() {
            PluginState::Active
        } else {
            PluginState::Error
        };
        result.map(|_| ())
    }
    async fn shutdown(&mut self) -> Result<()> {
        let result = self.call_function("shutdown").await;
        self.instance
            .lock()
            .map_err(|_| PluginError::execution_error(&self.info.id, "Instance lock failed"))?
            .take();
        self.state = if result.is_ok() {
            PluginState::Unloaded
        } else {
            PluginState::Error
        };
        result.map(|_| ())
    }
    async fn execute_command(
        &self,
        name: &str,
        args: Vec<String>,
        ctx: &PluginContext,
    ) -> Result<String> {
        let command = self
            .manifest
            .commands
            .iter()
            .find(|c| c.name == name || c.aliases.iter().any(|a| a == name))
            .ok_or_else(|| PluginError::CommandError("Command is not registered".into()))?;
        let result = self
            .invoke("command", &command.name, serde_json::json!(args), ctx)
            .await?;
        self.notifications.lock().await.extend(result.notifications);
        Ok(match result.data {
            serde_json::Value::String(s) => s,
            value => value.to_string(),
        })
    }
    async fn invoke(
        &self,
        method: &str,
        name: &str,
        input: serde_json::Value,
        ctx: &PluginContext,
    ) -> Result<InvocationResult> {
        if self.state() != PluginState::Active {
            return Err(PluginError::Disabled(self.info.id.clone()));
        }
        let function = match method {
            "command" => format!("cmd_{}", name.replace('-', "_")),
            "tool" => format!("tool_{}", name.replace('-', "_")),
            "hook" => name.to_string(),
            _ => {
                return Err(PluginError::execution_error(
                    &self.info.id,
                    "Unknown runtime operation",
                ));
            }
        };
        Ok(self.call(&function, input, ctx.clone()).await?.0)
    }
    async fn take_notifications(&self) -> Vec<Notification> {
        std::mem::take(&mut *self.notifications.lock().await)
    }
    fn get_config(&self, key: &str) -> Option<serde_json::Value> {
        self.config.read().ok()?.get(key).cloned()
    }
    fn set_config(&mut self, key: &str, value: serde_json::Value) -> Result<()> {
        self.config
            .write()
            .map_err(|_| PluginError::ConfigError("Plugin config lock failed".into()))?
            .insert(key.into(), value);
        Ok(())
    }
}
