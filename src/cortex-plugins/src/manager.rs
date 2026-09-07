//! Plugin manager - the main interface for the plugin system.

use std::path::Path;
use std::sync::Arc;

use crate::commands::PluginCommandRegistry;
use crate::config::PluginConfig;
use crate::events::EventBus;
use crate::hooks::HookRegistry;
use crate::loader::{DiscoveredPlugin, PluginLoader};
use crate::plugin::{PluginInfo, PluginStatus};
use crate::registry::PluginRegistry;
use crate::runtime::WasmRuntime;
use crate::{PluginContext, PluginError, Result};

/// Plugin manager - the main entry point for the plugin system.
///
/// The manager handles:
/// - Plugin discovery and loading
/// - Plugin lifecycle (init, shutdown)
/// - Command and hook registration
/// - Event distribution
pub struct PluginManager {
    activation_order: tokio::sync::Mutex<Vec<String>>,
    executable_hooks: Arc<crate::executable_hooks::ExecutableHooks>,
    /// Configuration
    config: PluginConfig,

    /// WASM runtime (kept for future use)
    #[allow(dead_code)]
    runtime: Arc<WasmRuntime>,

    /// Plugin loader
    loader: PluginLoader,

    /// Plugin registry
    registry: Arc<PluginRegistry>,

    /// Command registry
    commands: Arc<PluginCommandRegistry>,

    /// Hook registry
    hooks: Arc<HookRegistry>,

    /// Event bus
    events: Arc<EventBus>,
}

impl PluginManager {
    /// Create a new plugin manager.
    pub async fn new(config: PluginConfig) -> Result<Self> {
        let runtime = Arc::new(WasmRuntime::new()?);
        let loader = PluginLoader::new(config.clone(), runtime.clone());
        let registry = Arc::new(PluginRegistry::new());
        let commands = Arc::new(PluginCommandRegistry::new());
        let hooks = Arc::new(HookRegistry::new());
        let events = Arc::new(EventBus::new());
        let executable_hooks = Arc::new(crate::executable_hooks::ExecutableHooks::new(
            registry.clone(),
        ));

        Ok(Self {
            activation_order: tokio::sync::Mutex::new(Vec::new()),
            executable_hooks,
            config,
            runtime,
            loader,
            registry,
            commands,
            hooks,
            events,
        })
    }

    /// Create a plugin manager with default configuration.
    pub async fn default_manager() -> Result<Self> {
        Self::new(PluginConfig::default()).await
    }

    // ========== Discovery and Loading ==========

    /// Discover plugins in all search paths.
    pub async fn discover(&self) -> Vec<DiscoveredPlugin> {
        self.loader.discover().await
    }

    /// Discover and load all plugins.
    pub async fn discover_and_load(&self) -> Result<Vec<String>> {
        let discovered = self.loader.discover_checked().await?;
        let mut loaded = Vec::new();

        for plugin in discovered {
            // Check if plugin is enabled
            if !self.config.is_plugin_enabled(plugin.id()) {
                tracing::debug!("Plugin {} is disabled, skipping", plugin.id());
                continue;
            }
            if crate::activation::Activation::load(self.config.state_path.as_deref())?
                .disabled
                .contains(plugin.id())
            {
                continue;
            }

            // Check if already loaded
            if self.registry.is_registered(plugin.id()).await {
                tracing::debug!("Plugin {} is already loaded", plugin.id());
                continue;
            }

            self.load_discovered(&plugin).await?;
            loaded.push(plugin.id().to_string());
        }

        Ok(loaded)
    }

    /// Load a discovered plugin.
    async fn load_discovered(&self, discovered: &DiscoveredPlugin) -> Result<()> {
        // Load the WASM plugin
        let plugin = self.loader.load(discovered)?;
        let plugin_id = plugin.info().id.clone();

        // Register the plugin
        self.registry.register(plugin).await?;

        // Register commands from manifest
        self.register_declarations(&plugin_id, &discovered.manifest)
            .await?;

        // Publish load event
        self.events
            .publish(crate::events::Event::PluginLoaded {
                plugin_id: plugin_id.clone(),
            })
            .await;

        Ok(())
    }

    /// Load a plugin from a path.
    pub async fn load_from_path(&self, path: &Path) -> Result<String> {
        let plugin = self.loader.load_from_path(path).await?;
        let plugin_id = plugin.info().id.clone();
        let manifest = plugin.manifest().clone();

        self.registry.register(plugin).await?;
        self.register_declarations(&plugin_id, &manifest).await?;

        self.events
            .publish(crate::events::Event::PluginLoaded {
                plugin_id: plugin_id.clone(),
            })
            .await;

        Ok(plugin_id)
    }

    async fn register_declarations(
        &self,
        id: &str,
        manifest: &crate::PluginManifest,
    ) -> Result<()> {
        if let Err(error) = self.register_plugin_commands(id, manifest).await {
            self.commands.unregister_plugin(id).await;
            let _ = self.registry.unregister(id).await;
            return Err(error);
        }
        self.executable_hooks.register(id, &manifest.hooks).await;
        Ok(())
    }

    /// Register commands from a plugin manifest.
    async fn register_plugin_commands(
        &self,
        plugin_id: &str,
        manifest: &crate::manifest::PluginManifest,
    ) -> Result<()> {
        for cmd_manifest in &manifest.commands {
            let cmd = crate::commands::PluginCommand::from_manifest(plugin_id, cmd_manifest);

            // Create executor that calls the plugin
            let registry = self.registry.clone();
            let pid = plugin_id.to_string();
            let cmd_name = cmd.name.clone();

            let executor: crate::commands::CommandExecutor = Arc::new(move |args, ctx| {
                let registry = registry.clone();
                let pid = pid.clone();
                let cmd_name = cmd_name.clone();
                let ctx = ctx.clone();

                Box::pin(async move {
                    let handle = registry
                        .get(&pid)
                        .await
                        .ok_or_else(|| PluginError::NotFound(pid.clone()))?;

                    let result = handle.execute_command(&cmd_name, args, &ctx).await?;

                    Ok(crate::commands::PluginCommandResult::success(result))
                })
            });

            self.commands.register(cmd, executor).await?;
        }

        Ok(())
    }

    /// Unload a plugin.
    pub async fn unload(&self, plugin_id: &str) -> Result<()> {
        self.activation_order
            .lock()
            .await
            .retain(|id| id != plugin_id);
        self.executable_hooks.unregister(plugin_id).await;
        // Unregister commands
        self.commands.unregister_plugin(plugin_id).await;

        // Unregister hooks
        self.hooks.unregister_plugin(plugin_id).await;

        // Unregister event handlers
        self.events.unsubscribe_plugin(plugin_id).await;

        // Unregister plugin
        self.registry.unregister(plugin_id).await?;

        // Publish unload event
        self.events
            .publish(crate::events::Event::PluginUnloaded {
                plugin_id: plugin_id.to_string(),
            })
            .await;

        Ok(())
    }

    // ========== Plugin Lifecycle ==========

    /// Initialize all plugins.
    pub async fn init_all(&self) -> Result<()> {
        let order = self.validate_dependencies().await?;
        for id in order {
            let handle = self
                .registry
                .get(&id)
                .await
                .ok_or_else(|| PluginError::NotFound(id.clone()))?;
            let result = {
                let mut plugin = handle.write().await;
                if plugin.state() == crate::PluginState::Active {
                    continue;
                }
                plugin.init().await
            };
            if let Err(error) = result {
                let _ = self.shutdown_all().await;
                return Err(error);
            }
            self.activation_order.lock().await.push(id);
        }
        Ok(())
    }

    /// Shutdown in reverse successful initialization order, attempting all disposals.
    pub async fn shutdown_all(&self) -> Result<()> {
        let order = std::mem::take(&mut *self.activation_order.lock().await);
        let mut failure = None;
        for id in order.into_iter().rev() {
            if let Some(handle) = self.registry.get(&id).await {
                if let Err(error) = handle.write().await.shutdown().await {
                    failure = Some(error);
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }

    // ========== Plugin Management ==========

    /// List all loaded plugins.
    pub async fn list_plugins(&self) -> Vec<PluginInfo> {
        self.registry.list().await
    }

    /// List plugins with status.
    pub async fn list_plugins_status(&self) -> Vec<PluginStatus> {
        self.registry.list_status().await
    }

    /// Get a plugin by ID.
    pub async fn get_plugin(&self, id: &str) -> Option<PluginInfo> {
        match self.registry.get(id).await {
            Some(handle) => Some(handle.info().await),
            None => None,
        }
    }

    /// Check if a plugin is loaded.
    pub async fn is_loaded(&self, id: &str) -> bool {
        self.registry.is_registered(id).await
    }

    /// Enable a plugin.
    pub async fn enable(&self, plugin_id: &str) -> Result<()> {
        self.persist_enabled(plugin_id, true)?;
        let handle = self
            .registry
            .get(plugin_id)
            .await
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;

        let mut plugin = handle.write().await;
        plugin.init().await?;
        let mut order = self.activation_order.lock().await;
        if !order.iter().any(|id| id == plugin_id) {
            order.push(plugin_id.into());
        }

        Ok(())
    }

    /// Disable a plugin (without unloading).
    pub async fn disable(&self, plugin_id: &str) -> Result<()> {
        self.persist_enabled(plugin_id, false)?;
        self.activation_order
            .lock()
            .await
            .retain(|id| id != plugin_id);
        let handle = self
            .registry
            .get(plugin_id)
            .await
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;

        let mut plugin = handle.write().await;
        plugin.shutdown().await?;

        Ok(())
    }

    fn persist_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        crate::contract::validate_id(id)?;
        if let Some(path) = &self.config.state_path {
            let mut activation = crate::activation::Activation::load(Some(path))?;
            if enabled {
                activation.disabled.remove(id);
            } else {
                activation.disabled.insert(id.into());
            }
            activation.save(path)?;
        }
        Ok(())
    }

    async fn validate_dependencies(&self) -> Result<Vec<String>> {
        let plugins = self.registry.list().await;
        let mut graph = std::collections::BTreeMap::new();
        for info in &plugins {
            let handle = self
                .registry
                .get(&info.id)
                .await
                .ok_or_else(|| PluginError::NotFound(info.id.clone()))?;
            let plugin = handle.read().await;
            let mut edges = Vec::new();
            for dependency in &plugin.manifest().dependencies {
                let found = plugins.iter().find(|p| p.id == dependency.id);
                if found.is_none() && dependency.optional {
                    continue;
                }
                let found = found.ok_or_else(|| {
                    PluginError::dependency_error(&info.id, "Required plugin is missing")
                })?;
                let range = semver::VersionReq::parse(&dependency.version).map_err(|_| {
                    PluginError::dependency_error(&info.id, "Invalid version range")
                })?;
                let version = semver::Version::parse(&found.version).map_err(|_| {
                    PluginError::dependency_error(&info.id, "Invalid dependency version")
                })?;
                if !range.matches(&version) {
                    return Err(PluginError::dependency_error(
                        &info.id,
                        "Dependency version does not match",
                    ));
                }
                edges.push(dependency.id.clone());
            }
            graph.insert(info.id.clone(), edges);
        }
        let mut order = Vec::new();
        while !graph.is_empty() {
            let ready: Vec<_> = graph
                .iter()
                .filter(|(_, edges)| edges.is_empty())
                .map(|(id, _)| id.clone())
                .collect();
            if ready.is_empty() {
                return Err(PluginError::dependency_error("plugins", "Dependency cycle"));
            }
            for id in ready {
                order.push(id.clone());
                graph.remove(&id);
                for edges in graph.values_mut() {
                    edges.retain(|edge| edge != &id);
                }
            }
        }
        Ok(order)
    }

    /// Dispatch before final schema/path validation and the host security gate.
    pub async fn dispatch_hook(
        &self,
        kind: crate::HookType,
        input: serde_json::Value,
        ctx: &PluginContext,
    ) -> Result<crate::executable_hooks::DispatchOutcome> {
        self.executable_hooks.dispatch(kind, input, ctx).await
    }

    pub async fn list_tools(&self) -> Vec<(String, crate::contract::ToolDeclaration)> {
        let mut tools = Vec::new();
        for info in self.registry.list().await {
            if let Some(handle) = self.registry.get(&info.id).await {
                let plugin = handle.read().await;
                if plugin.state() == crate::PluginState::Active {
                    tools.extend(
                        plugin
                            .manifest()
                            .tools
                            .iter()
                            .cloned()
                            .map(|tool| (info.id.clone(), tool)),
                    );
                }
            }
        }
        tools.sort_by(|a, b| (&a.0, &a.1.name).cmp(&(&b.0, &b.1.name)));
        tools
    }

    /// The caller must run its normal approval gate on the final input first.
    pub async fn execute_tool(
        &self,
        id: &str,
        name: &str,
        input: serde_json::Value,
        ctx: &PluginContext,
    ) -> Result<crate::contract::InvocationResult> {
        let handle = self
            .registry
            .get(id)
            .await
            .ok_or_else(|| PluginError::NotFound(id.into()))?;
        let plugin = handle.read().await;
        let tool = plugin
            .manifest()
            .tools
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| PluginError::NotFound(name.into()))?;
        crate::contract::validate_arguments(&tool.input_schema, &input)?;
        plugin.invoke("tool", name, input, ctx).await
    }

    pub async fn take_notifications(&self) -> Vec<crate::contract::Notification> {
        let mut result = Vec::new();
        for info in self.registry.list().await {
            if let Some(handle) = self.registry.get(&info.id).await {
                result.extend(handle.read().await.take_notifications().await);
            }
        }
        result
    }

    // ========== Commands ==========

    /// Execute a plugin command.
    pub async fn execute_command(
        &self,
        name: &str,
        args: Vec<String>,
        ctx: &PluginContext,
    ) -> Result<crate::commands::PluginCommandResult> {
        self.commands.execute(name, args, ctx).await
    }

    /// Check if a command exists.
    pub async fn has_command(&self, name: &str) -> bool {
        self.commands.exists(name).await
    }

    /// List all plugin commands.
    pub async fn list_commands(&self) -> Vec<crate::commands::PluginCommand> {
        self.commands.list_visible().await
    }

    /// Get command registry.
    pub fn command_registry(&self) -> &Arc<PluginCommandRegistry> {
        &self.commands
    }

    // ========== Hooks ==========

    /// Get hook registry.
    pub fn hook_registry(&self) -> &Arc<HookRegistry> {
        &self.hooks
    }

    /// Create a hook dispatcher.
    pub fn hook_dispatcher(&self) -> crate::hooks::HookDispatcher {
        crate::hooks::HookDispatcher::new(self.hooks.clone())
    }

    // ========== Events ==========

    /// Get event bus.
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.events
    }

    /// Publish an event.
    pub async fn publish_event(&self, event: crate::events::Event) {
        self.events.publish(event).await;
    }

    // ========== Configuration ==========

    /// Get configuration.
    pub fn config(&self) -> &PluginConfig {
        &self.config
    }

    /// Get plugin configuration.
    pub fn get_plugin_config(&self, plugin_id: &str) -> Option<&serde_json::Value> {
        self.config.get_plugin_config(plugin_id)
    }

    // ========== Statistics ==========

    /// Get plugin count.
    pub async fn plugin_count(&self) -> usize {
        self.registry.count().await
    }

    /// Get command count.
    pub async fn command_count(&self) -> usize {
        self.commands.list().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_create_manager() {
        let config = PluginConfig::default();
        let manager = PluginManager::new(config).await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_discover_empty() {
        let config = PluginConfig {
            search_paths: vec![PathBuf::from("/nonexistent")],
            ..Default::default()
        };
        let manager = PluginManager::new(config).await.unwrap();
        let plugins = manager.discover().await;
        assert!(plugins.is_empty());
    }
}
