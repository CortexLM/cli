//! Integration tests for the plugin lifecycle: discover → load → init → hook → shutdown.
//!
//! Tests the full plugin lifecycle including:
//! - Plugin discovery in various scenarios
//! - Loading valid and invalid plugins
//! - Initialization and shutdown
//! - Hook registration and dispatch
//! - Error conditions

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tempfile::TempDir;

use cortex_plugins::manifest::{PluginManifest, PluginMetadata, WasmSettings};
use cortex_plugins::plugin::Plugin;
use cortex_plugins::registry::PluginRegistry;
use cortex_plugins::runtime::WasmRuntime;
use cortex_plugins::{
    HookDispatcher, HookPriority, HookRegistry, HookResult, PluginCommand, PluginCommandRegistry,
    PluginConfig, PluginContext, PluginError, PluginInfo, PluginManager, PluginState,
    ToolExecuteBeforeHook, ToolExecuteBeforeInput, ToolExecuteBeforeOutput,
};

// =============================================================================
// Mock Plugin Implementation for Testing
// =============================================================================

/// A mock plugin implementation for testing lifecycle operations.
struct MockPlugin {
    info: PluginInfo,
    manifest: PluginManifest,
    state: PluginState,
    config: tokio::sync::RwLock<HashMap<String, serde_json::Value>>,
}

impl MockPlugin {
    fn new(id: &str) -> Self {
        let manifest = PluginManifest {
            plugin: PluginMetadata {
                id: id.to_string(),
                name: format!("Mock Plugin {}", id),
                version: "1.0.0".to_string(),
                description: "A mock plugin for testing".to_string(),
                authors: vec!["Test Author".to_string()],
                homepage: None,
                license: Some("MIT".to_string()),
                min_cortex_version: None,
                keywords: vec!["test".to_string()],
                icon: None,
            },
            capabilities: vec![],
            permissions: vec![],
            dependencies: vec![],
            commands: vec![],
            hooks: vec![],
            config: HashMap::new(),
            wasm: WasmSettings::default(),
        };

        let info = PluginInfo::from_manifest(&manifest, PathBuf::from("/tmp/test"));

        Self {
            info,
            manifest,
            state: PluginState::Loaded,
            config: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    fn with_state(mut self, state: PluginState) -> Self {
        self.state = state;
        self
    }
}

#[async_trait]
impl Plugin for MockPlugin {
    fn info(&self) -> &PluginInfo {
        &self.info
    }

    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn state(&self) -> PluginState {
        self.state
    }

    async fn init(&mut self) -> cortex_plugins::Result<()> {
        if self.state != PluginState::Loaded {
            return Err(PluginError::InvalidState {
                expected: "loaded".to_string(),
                actual: self.state.to_string(),
            });
        }
        self.state = PluginState::Active;
        Ok(())
    }

    async fn shutdown(&mut self) -> cortex_plugins::Result<()> {
        self.state = PluginState::Unloaded;
        Ok(())
    }

    async fn execute_command(
        &self,
        name: &str,
        _args: Vec<String>,
        _ctx: &PluginContext,
    ) -> cortex_plugins::Result<String> {
        Ok(format!("Mock command executed: {}", name))
    }

    fn get_config(&self, key: &str) -> Option<serde_json::Value> {
        let config = self.config.blocking_read();
        config.get(key).cloned()
    }

    fn set_config(&mut self, key: &str, value: serde_json::Value) -> cortex_plugins::Result<()> {
        let mut config = self.config.blocking_write();
        config.insert(key.to_string(), value);
        Ok(())
    }
}

/// Mock plugin that fails on init
struct FailingInitPlugin {
    info: PluginInfo,
    manifest: PluginManifest,
    state: PluginState,
}

impl FailingInitPlugin {
    fn new(id: &str) -> Self {
        let manifest = PluginManifest {
            plugin: PluginMetadata {
                id: id.to_string(),
                name: format!("Failing Plugin {}", id),
                version: "1.0.0".to_string(),
                description: "A plugin that fails to initialize".to_string(),
                authors: vec![],
                homepage: None,
                license: None,
                min_cortex_version: None,
                keywords: vec![],
                icon: None,
            },
            capabilities: vec![],
            permissions: vec![],
            dependencies: vec![],
            commands: vec![],
            hooks: vec![],
            config: HashMap::new(),
            wasm: WasmSettings::default(),
        };

        let info = PluginInfo::from_manifest(&manifest, PathBuf::from("/tmp/test"));

        Self {
            info,
            manifest,
            state: PluginState::Loaded,
        }
    }
}

#[async_trait]
impl Plugin for FailingInitPlugin {
    fn info(&self) -> &PluginInfo {
        &self.info
    }

    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn state(&self) -> PluginState {
        self.state
    }

    async fn init(&mut self) -> cortex_plugins::Result<()> {
        Err(PluginError::init_error(
            &self.info.id,
            "Simulated init failure",
        ))
    }

    async fn shutdown(&mut self) -> cortex_plugins::Result<()> {
        self.state = PluginState::Unloaded;
        Ok(())
    }

    async fn execute_command(
        &self,
        _name: &str,
        _args: Vec<String>,
        _ctx: &PluginContext,
    ) -> cortex_plugins::Result<String> {
        Err(PluginError::execution_error(
            &self.info.id,
            "Plugin not initialized",
        ))
    }

    fn get_config(&self, _key: &str) -> Option<serde_json::Value> {
        None
    }

    fn set_config(&mut self, _key: &str, _value: serde_json::Value) -> cortex_plugins::Result<()> {
        Ok(())
    }
}

// =============================================================================
// Test Helpers
// =============================================================================

/// Create a valid plugin manifest as TOML content
fn create_valid_manifest(id: &str, name: &str, version: &str) -> String {
    format!(
        r#"
[plugin]
id = "{id}"
name = "{name}"
version = "{version}"
description = "Test plugin for lifecycle tests"
authors = ["Test Author"]

[[commands]]
name = "test-cmd"
description = "A test command"
"#
    )
}

/// Create a minimal WASM module (just enough to compile)
fn create_minimal_wasm_bytes() -> Vec<u8> {
    // Minimal valid WASM module with just a memory export
    // This is the smallest valid WASM module that can be loaded
    vec![
        0x00, 0x61, 0x73, 0x6D, // WASM magic number
        0x01, 0x00, 0x00, 0x00, // WASM version 1
    ]
}

/// Setup a test plugin directory with manifest
async fn setup_plugin_dir(base: &Path, id: &str) -> PathBuf {
    let plugin_dir = base.join(id);
    tokio::fs::create_dir_all(&plugin_dir)
        .await
        .expect("Failed to create plugin dir");

    let manifest_content = create_valid_manifest(id, &format!("Test Plugin {}", id), "1.0.0");
    let manifest_path = plugin_dir.join("plugin.toml");
    tokio::fs::write(&manifest_path, manifest_content)
        .await
        .expect("Failed to write manifest");

    plugin_dir
}

/// Setup a test plugin directory with WASM file
async fn setup_plugin_dir_with_wasm(base: &Path, id: &str) -> PathBuf {
    let plugin_dir = setup_plugin_dir(base, id).await;

    let wasm_bytes = create_minimal_wasm_bytes();
    let wasm_path = plugin_dir.join("plugin.wasm");
    tokio::fs::write(&wasm_path, wasm_bytes)
        .await
        .expect("Failed to write WASM file");

    plugin_dir
}

// =============================================================================
// Plugin Discovery Tests
// =============================================================================

mod discovery_tests {
    use super::*;

    #[tokio::test]
    async fn test_discover_empty_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        let config = PluginConfig {
            search_paths: vec![temp_dir.path().to_path_buf()],
            ..Default::default()
        };

        let manager = PluginManager::new(config)
            .await
            .expect("Failed to create manager");
        let discovered = manager.discover().await;

        assert!(
            discovered.is_empty(),
            "Expected no plugins in empty directory"
        );
    }

    #[tokio::test]
    async fn test_discover_nonexistent_path() {
        let config = PluginConfig {
            search_paths: vec![PathBuf::from("/nonexistent/path/that/does/not/exist")],
            ..Default::default()
        };

        let manager = PluginManager::new(config)
            .await
            .expect("Failed to create manager");
        let discovered = manager.discover().await;

        assert!(
            discovered.is_empty(),
            "Expected no plugins from nonexistent path"
        );
    }

    #[tokio::test]
    async fn test_discover_single_plugin() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        setup_plugin_dir(temp_dir.path(), "test-plugin").await;

        let config = PluginConfig {
            search_paths: vec![temp_dir.path().to_path_buf()],
            ..Default::default()
        };

        let manager = PluginManager::new(config)
            .await
            .expect("Failed to create manager");
        let discovered = manager.discover().await;

        assert_eq!(discovered.len(), 1, "Expected exactly one plugin");
        assert_eq!(discovered[0].id(), "test-plugin");
    }

    #[tokio::test]
    async fn test_discover_multiple_plugins() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        setup_plugin_dir(temp_dir.path(), "plugin-a").await;
        setup_plugin_dir(temp_dir.path(), "plugin-b").await;
        setup_plugin_dir(temp_dir.path(), "plugin-c").await;

        let config = PluginConfig {
            search_paths: vec![temp_dir.path().to_path_buf()],
            ..Default::default()
        };

        let manager = PluginManager::new(config)
            .await
            .expect("Failed to create manager");
        let discovered = manager.discover().await;

        assert_eq!(discovered.len(), 3, "Expected three plugins");

        let ids: Vec<_> = discovered.iter().map(|p| p.id()).collect();
        assert!(ids.contains(&"plugin-a"));
        assert!(ids.contains(&"plugin-b"));
        assert!(ids.contains(&"plugin-c"));
    }

    #[tokio::test]
    async fn test_discover_skips_invalid_manifest() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a valid plugin
        setup_plugin_dir(temp_dir.path(), "valid-plugin").await;

        // Create an invalid plugin (bad manifest)
        let invalid_dir = temp_dir.path().join("invalid-plugin");
        tokio::fs::create_dir_all(&invalid_dir)
            .await
            .expect("Failed to create dir");
        tokio::fs::write(invalid_dir.join("plugin.toml"), "invalid toml content {{{")
            .await
            .expect("Failed to write");

        let config = PluginConfig {
            search_paths: vec![temp_dir.path().to_path_buf()],
            ..Default::default()
        };

        let manager = PluginManager::new(config)
            .await
            .expect("Failed to create manager");
        let discovered = manager.discover().await;

        assert_eq!(discovered.len(), 1, "Should only discover valid plugin");
        assert_eq!(discovered[0].id(), "valid-plugin");
    }

    #[tokio::test]
    async fn test_discover_from_multiple_paths() {
        let temp_dir_a = TempDir::new().expect("Failed to create temp dir");
        let temp_dir_b = TempDir::new().expect("Failed to create temp dir");

        setup_plugin_dir(temp_dir_a.path(), "plugin-from-a").await;
        setup_plugin_dir(temp_dir_b.path(), "plugin-from-b").await;

        let config = PluginConfig {
            search_paths: vec![
                temp_dir_a.path().to_path_buf(),
                temp_dir_b.path().to_path_buf(),
            ],
            ..Default::default()
        };

        let manager = PluginManager::new(config)
            .await
            .expect("Failed to create manager");
        let discovered = manager.discover().await;

        assert_eq!(discovered.len(), 2, "Expected plugins from both paths");

        let ids: Vec<_> = discovered.iter().map(|p| p.id()).collect();
        assert!(ids.contains(&"plugin-from-a"));
        assert!(ids.contains(&"plugin-from-b"));
    }

    #[tokio::test]
    async fn test_discover_checks_wasm_existence() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Plugin with WASM
        setup_plugin_dir_with_wasm(temp_dir.path(), "plugin-with-wasm").await;

        // Plugin without WASM
        setup_plugin_dir(temp_dir.path(), "plugin-without-wasm").await;

        let config = PluginConfig {
            search_paths: vec![temp_dir.path().to_path_buf()],
            ..Default::default()
        };

        let manager = PluginManager::new(config)
            .await
            .expect("Failed to create manager");
        let discovered = manager.discover().await;

        assert_eq!(discovered.len(), 2, "Should discover both plugins");

        let with_wasm = discovered
            .iter()
            .find(|p| p.id() == "plugin-with-wasm")
            .unwrap();
        let without_wasm = discovered
            .iter()
            .find(|p| p.id() == "plugin-without-wasm")
            .unwrap();

        assert!(
            with_wasm.has_wasm,
            "Plugin with WASM should have has_wasm=true"
        );
        assert!(
            !without_wasm.has_wasm,
            "Plugin without WASM should have has_wasm=false"
        );
    }
}

// =============================================================================
// Plugin Manager Tests
// =============================================================================

mod manager_tests {
    use super::*;

    #[tokio::test]
    async fn test_manager_new_with_default_config() {
        let manager = PluginManager::default_manager().await;
        assert!(manager.is_ok(), "Should create manager with default config");
    }

    #[tokio::test]
    async fn test_manager_new_with_custom_config() {
        let config = PluginConfig {
            search_paths: vec![PathBuf::from("/custom/path")],
            hot_reload: true,
            sandbox_enabled: true,
            default_memory_pages: 128,
            default_timeout_ms: 15000,
            ..Default::default()
        };

        let manager = PluginManager::new(config.clone()).await;
        assert!(manager.is_ok());

        let manager = manager.unwrap();
        assert_eq!(manager.config().default_memory_pages, 128);
        assert_eq!(manager.config().default_timeout_ms, 15000);
    }

    #[tokio::test]
    async fn test_manager_plugin_count_initially_zero() {
        let config = PluginConfig::default();
        let manager = PluginManager::new(config).await.unwrap();

        assert_eq!(manager.plugin_count().await, 0);
    }

    #[tokio::test]
    async fn test_manager_command_count_initially_zero() {
        let config = PluginConfig::default();
        let manager = PluginManager::new(config).await.unwrap();

        assert_eq!(manager.command_count().await, 0);
    }

    #[tokio::test]
    async fn test_manager_discover_and_load_empty() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        let config = PluginConfig {
            search_paths: vec![temp_dir.path().to_path_buf()],
            ..Default::default()
        };

        let manager = PluginManager::new(config).await.unwrap();
        let loaded = manager.discover_and_load().await.unwrap();

        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn test_manager_list_plugins_empty() {
        let config = PluginConfig::default();
        let manager = PluginManager::new(config).await.unwrap();

        let plugins = manager.list_plugins().await;
        assert!(plugins.is_empty());
    }

    #[tokio::test]
    async fn test_manager_has_command_false_for_unknown() {
        let config = PluginConfig::default();
        let manager = PluginManager::new(config).await.unwrap();

        assert!(!manager.has_command("nonexistent-command").await);
    }

    #[tokio::test]
    async fn test_manager_list_commands_empty() {
        let config = PluginConfig::default();
        let manager = PluginManager::new(config).await.unwrap();

        let commands = manager.list_commands().await;
        assert!(commands.is_empty());
    }

    #[tokio::test]
    async fn test_manager_is_loaded_false_for_unknown() {
        let config = PluginConfig::default();
        let manager = PluginManager::new(config).await.unwrap();

        assert!(!manager.is_loaded("nonexistent-plugin").await);
    }

    #[tokio::test]
    async fn test_manager_get_plugin_none_for_unknown() {
        let config = PluginConfig::default();
        let manager = PluginManager::new(config).await.unwrap();

        let plugin = manager.get_plugin("nonexistent-plugin").await;
        assert!(plugin.is_none());
    }

    #[tokio::test]
    async fn test_manager_init_all_empty() {
        let config = PluginConfig::default();
        let manager = PluginManager::new(config).await.unwrap();

        let result = manager.init_all().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_manager_shutdown_all_empty() {
        let config = PluginConfig::default();
        let manager = PluginManager::new(config).await.unwrap();

        let result = manager.shutdown_all().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_manager_disabled_plugin_not_loaded() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        setup_plugin_dir_with_wasm(temp_dir.path(), "disabled-plugin").await;

        let config = PluginConfig {
            search_paths: vec![temp_dir.path().to_path_buf()],
            disabled_plugins: vec!["disabled-plugin".to_string()],
            ..Default::default()
        };

        let manager = PluginManager::new(config).await.unwrap();
        let loaded = manager.discover_and_load().await.unwrap();

        assert!(loaded.is_empty(), "Disabled plugin should not be loaded");
    }
}

// =============================================================================
// Plugin Registry Tests
// =============================================================================

mod registry_tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_register_plugin() {
        let registry = PluginRegistry::new();
        let plugin = MockPlugin::new("test-plugin");

        let result = registry.register(Box::new(plugin)).await;
        assert!(result.is_ok());
        assert!(registry.is_registered("test-plugin").await);
    }

    #[tokio::test]
    async fn test_registry_register_duplicate_fails() {
        let registry = PluginRegistry::new();

        registry
            .register(Box::new(MockPlugin::new("dup-plugin")))
            .await
            .unwrap();

        let result = registry
            .register(Box::new(MockPlugin::new("dup-plugin")))
            .await;
        assert!(result.is_err());

        match result {
            Err(PluginError::AlreadyExists(id)) => {
                assert_eq!(id, "dup-plugin");
            }
            _ => panic!("Expected AlreadyExists error"),
        }
    }

    #[tokio::test]
    async fn test_registry_unregister_plugin() {
        let registry = PluginRegistry::new();

        registry
            .register(Box::new(MockPlugin::new("to-remove")))
            .await
            .unwrap();

        assert!(registry.is_registered("to-remove").await);

        registry.unregister("to-remove").await.unwrap();

        assert!(!registry.is_registered("to-remove").await);
    }

    #[tokio::test]
    async fn test_registry_get_plugin() {
        let registry = PluginRegistry::new();

        registry
            .register(Box::new(MockPlugin::new("get-test")))
            .await
            .unwrap();

        let handle = registry.get("get-test").await;
        assert!(handle.is_some());

        let info = handle.unwrap().info().await;
        assert_eq!(info.id, "get-test");
    }

    #[tokio::test]
    async fn test_registry_get_nonexistent() {
        let registry = PluginRegistry::new();

        let handle = registry.get("does-not-exist").await;
        assert!(handle.is_none());
    }

    #[tokio::test]
    async fn test_registry_list_plugins() {
        let registry = PluginRegistry::new();

        registry
            .register(Box::new(MockPlugin::new("list-a")))
            .await
            .unwrap();
        registry
            .register(Box::new(MockPlugin::new("list-b")))
            .await
            .unwrap();

        let plugins = registry.list().await;
        assert_eq!(plugins.len(), 2);
    }

    #[tokio::test]
    async fn test_registry_count() {
        let registry = PluginRegistry::new();

        assert_eq!(registry.count().await, 0);

        registry
            .register(Box::new(MockPlugin::new("count-1")))
            .await
            .unwrap();
        assert_eq!(registry.count().await, 1);

        registry
            .register(Box::new(MockPlugin::new("count-2")))
            .await
            .unwrap();
        assert_eq!(registry.count().await, 2);
    }

    #[tokio::test]
    async fn test_registry_init_all() {
        let registry = PluginRegistry::new();

        registry
            .register(Box::new(MockPlugin::new("init-test")))
            .await
            .unwrap();

        let results = registry.init_all().await;

        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_ok());

        let handle = registry.get("init-test").await.unwrap();
        assert_eq!(handle.state().await, PluginState::Active);
    }

    #[tokio::test]
    async fn test_registry_init_all_with_failure() {
        let registry = PluginRegistry::new();

        registry
            .register(Box::new(MockPlugin::new("good-plugin")))
            .await
            .unwrap();
        registry
            .register(Box::new(FailingInitPlugin::new("bad-plugin")))
            .await
            .unwrap();

        let results = registry.init_all().await;

        assert_eq!(results.len(), 2);

        let good_result = results.iter().find(|(id, _)| id == "good-plugin");
        let bad_result = results.iter().find(|(id, _)| id == "bad-plugin");

        assert!(good_result.unwrap().1.is_ok());
        assert!(bad_result.unwrap().1.is_err());
    }

    #[tokio::test]
    async fn test_registry_shutdown_all() {
        let registry = PluginRegistry::new();

        registry
            .register(Box::new(MockPlugin::new("shutdown-test")))
            .await
            .unwrap();

        // Initialize first
        registry.init_all().await;

        let results = registry.shutdown_all().await;

        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_ok());

        let handle = registry.get("shutdown-test").await.unwrap();
        assert_eq!(handle.state().await, PluginState::Unloaded);
    }

    #[tokio::test]
    async fn test_registry_list_status() {
        let registry = PluginRegistry::new();

        registry
            .register(Box::new(MockPlugin::new("status-test")))
            .await
            .unwrap();

        let statuses = registry.list_status().await;

        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].info.id, "status-test");
        assert_eq!(statuses[0].state, PluginState::Loaded);
    }

    #[tokio::test]
    async fn test_registry_active_plugin_ids() {
        let registry = PluginRegistry::new();

        registry
            .register(Box::new(MockPlugin::new("active-1")))
            .await
            .unwrap();
        registry
            .register(Box::new(MockPlugin::new("active-2")))
            .await
            .unwrap();

        // Initially no active plugins
        let active = registry.active_plugin_ids().await;
        assert!(active.is_empty());

        // Initialize all
        registry.init_all().await;

        let active = registry.active_plugin_ids().await;
        assert_eq!(active.len(), 2);
    }

    #[tokio::test]
    async fn test_registry_stats() {
        let registry = PluginRegistry::new();

        registry
            .register(Box::new(MockPlugin::new("stats-test")))
            .await
            .unwrap();

        // Update stats
        registry
            .update_stats("stats-test", |s| {
                s.commands_executed = 5;
                s.hooks_triggered = 10;
            })
            .await;

        let stats = registry.get_stats("stats-test").await.unwrap();
        assert_eq!(stats.commands_executed, 5);
        assert_eq!(stats.hooks_triggered, 10);
    }
}

// =============================================================================
// Plugin Lifecycle Tests
// =============================================================================

mod lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_state_transitions() {
        let registry = PluginRegistry::new();
        let plugin = MockPlugin::new("state-test");

        registry.register(Box::new(plugin)).await.unwrap();

        let handle = registry.get("state-test").await.unwrap();

        // Initial state: Loaded
        assert_eq!(handle.state().await, PluginState::Loaded);

        // After init: Active
        {
            let mut p = handle.write().await;
            p.init().await.unwrap();
        }
        assert_eq!(handle.state().await, PluginState::Active);

        // After shutdown: Unloaded
        {
            let mut p = handle.write().await;
            p.shutdown().await.unwrap();
        }
        assert_eq!(handle.state().await, PluginState::Unloaded);
    }

    #[tokio::test]
    async fn test_init_changes_state_to_active() {
        let plugin = MockPlugin::new("counter-test");

        let registry = PluginRegistry::new();
        registry.register(Box::new(plugin)).await.unwrap();

        let handle = registry.get("counter-test").await.unwrap();

        // Verify initial state
        assert_eq!(handle.state().await, PluginState::Loaded);

        {
            let mut p = handle.write().await;
            p.init().await.unwrap();
        }

        // The state change confirms init was called
        assert_eq!(handle.state().await, PluginState::Active);
    }

    #[tokio::test]
    async fn test_init_fails_for_wrong_state() {
        let plugin = MockPlugin::new("wrong-state").with_state(PluginState::Active);

        let registry = PluginRegistry::new();
        registry.register(Box::new(plugin)).await.unwrap();

        let handle = registry.get("wrong-state").await.unwrap();

        let result = {
            let mut p = handle.write().await;
            p.init().await
        };

        assert!(result.is_err());
        match result {
            Err(PluginError::InvalidState { expected, actual }) => {
                assert_eq!(expected, "loaded");
                assert_eq!(actual, "active");
            }
            _ => panic!("Expected InvalidState error"),
        }
    }

    #[tokio::test]
    async fn test_multiple_plugins_lifecycle() {
        let registry = PluginRegistry::new();

        // Register multiple plugins
        for i in 1..=5 {
            let plugin = MockPlugin::new(&format!("multi-{}", i));
            registry.register(Box::new(plugin)).await.unwrap();
        }

        assert_eq!(registry.count().await, 5);

        // Init all
        let init_results = registry.init_all().await;
        assert_eq!(init_results.len(), 5);
        assert!(init_results.iter().all(|(_, r)| r.is_ok()));

        // Verify all active
        let active = registry.active_plugin_ids().await;
        assert_eq!(active.len(), 5);

        // Shutdown all
        let shutdown_results = registry.shutdown_all().await;
        assert_eq!(shutdown_results.len(), 5);
        assert!(shutdown_results.iter().all(|(_, r)| r.is_ok()));

        // Verify all unloaded
        let active = registry.active_plugin_ids().await;
        assert!(active.is_empty());
    }
}

// =============================================================================
// Hook System Tests
// =============================================================================

mod hook_tests {
    use super::*;
    use cortex_plugins::manifest::HookType;

    struct TestBeforeHook {
        priority: HookPriority,
        pattern: Option<String>,
        modify_args: bool,
    }

    impl TestBeforeHook {
        fn new(priority: HookPriority) -> Self {
            Self {
                priority,
                pattern: None,
                modify_args: true,
            }
        }

        fn with_pattern(mut self, pattern: &str) -> Self {
            self.pattern = Some(pattern.to_string());
            self
        }
    }

    #[async_trait]
    impl ToolExecuteBeforeHook for TestBeforeHook {
        fn priority(&self) -> HookPriority {
            self.priority
        }

        fn pattern(&self) -> Option<&str> {
            self.pattern.as_deref()
        }

        async fn execute(
            &self,
            _input: &ToolExecuteBeforeInput,
            output: &mut ToolExecuteBeforeOutput,
        ) -> cortex_plugins::Result<()> {
            if self.modify_args {
                if let Some(obj) = output.args.as_object_mut() {
                    obj.insert("hook_executed".to_string(), serde_json::json!(true));
                }
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_hook_registry_creation() {
        let registry = HookRegistry::new();
        assert_eq!(registry.hook_count(HookType::ToolExecuteBefore).await, 0);
    }

    #[tokio::test]
    async fn test_register_tool_before_hook() {
        let registry = HookRegistry::new();

        let hook = Arc::new(TestBeforeHook::new(HookPriority::NORMAL));
        registry
            .register_tool_execute_before("test-plugin", hook)
            .await;

        assert_eq!(registry.hook_count(HookType::ToolExecuteBefore).await, 1);
    }

    #[tokio::test]
    async fn test_hook_priority_ordering() {
        // Test that hooks execute in priority order by verifying ordering via dispatcher
        let registry = Arc::new(HookRegistry::new());

        // Create hooks that add their priority to output args to verify execution order
        struct PriorityTrackingHook {
            priority: HookPriority,
            id: &'static str,
        }

        #[async_trait]
        impl ToolExecuteBeforeHook for PriorityTrackingHook {
            fn priority(&self) -> HookPriority {
                self.priority
            }

            async fn execute(
                &self,
                _input: &ToolExecuteBeforeInput,
                output: &mut ToolExecuteBeforeOutput,
            ) -> cortex_plugins::Result<()> {
                if let Some(obj) = output.args.as_object_mut() {
                    // Append our ID to the execution order array
                    let order = obj
                        .entry("execution_order")
                        .or_insert(serde_json::json!([]));
                    if let Some(arr) = order.as_array_mut() {
                        arr.push(serde_json::json!(self.id));
                    }
                }
                Ok(())
            }
        }

        let low_hook = Arc::new(PriorityTrackingHook {
            priority: HookPriority::LOW,
            id: "low",
        });
        let high_hook = Arc::new(PriorityTrackingHook {
            priority: HookPriority::PLUGIN_HIGH,
            id: "high",
        });
        let normal_hook = Arc::new(PriorityTrackingHook {
            priority: HookPriority::NORMAL,
            id: "normal",
        });

        // Register in "wrong" order
        registry
            .register_tool_execute_before("plugin-low", low_hook)
            .await;
        registry
            .register_tool_execute_before("plugin-high", high_hook)
            .await;
        registry
            .register_tool_execute_before("plugin-normal", normal_hook)
            .await;

        // Verify correct count
        assert_eq!(registry.hook_count(HookType::ToolExecuteBefore).await, 3);

        // Execute hooks via dispatcher to verify order
        let dispatcher = HookDispatcher::new(registry);
        let input = ToolExecuteBeforeInput {
            tool: "test".to_string(),
            session_id: "session".to_string(),
            call_id: "call".to_string(),
            args: serde_json::json!({}),
        };
        let output = dispatcher.trigger_tool_execute_before(input).await.unwrap();

        // Verify hooks executed in priority order (high -> normal -> low)
        let order = output.args["execution_order"].as_array().unwrap();
        assert_eq!(order[0], "high");
        assert_eq!(order[1], "normal");
        assert_eq!(order[2], "low");
    }

    #[tokio::test]
    async fn test_unregister_plugin_hooks() {
        let registry = HookRegistry::new();

        let hook1 = Arc::new(TestBeforeHook::new(HookPriority::NORMAL));
        let hook2 = Arc::new(TestBeforeHook::new(HookPriority::NORMAL));

        registry
            .register_tool_execute_before("plugin-a", hook1)
            .await;
        registry
            .register_tool_execute_before("plugin-b", hook2)
            .await;

        assert_eq!(registry.hook_count(HookType::ToolExecuteBefore).await, 2);

        registry.unregister_plugin("plugin-a").await;

        assert_eq!(registry.hook_count(HookType::ToolExecuteBefore).await, 1);
    }

    #[tokio::test]
    async fn test_hook_dispatcher_executes_hooks() {
        let registry = Arc::new(HookRegistry::new());

        let hook = Arc::new(TestBeforeHook::new(HookPriority::NORMAL));
        registry
            .register_tool_execute_before("test-plugin", hook)
            .await;

        let dispatcher = HookDispatcher::new(registry);

        let input = ToolExecuteBeforeInput {
            tool: "read".to_string(),
            session_id: "session-1".to_string(),
            call_id: "call-1".to_string(),
            args: serde_json::json!({"path": "/test"}),
        };

        let output = dispatcher.trigger_tool_execute_before(input).await.unwrap();

        // Hook should have added the "hook_executed" field
        assert_eq!(output.args["hook_executed"], true);
        assert_eq!(output.args["path"], "/test");
    }

    #[tokio::test]
    async fn test_hook_pattern_matching() {
        let registry = Arc::new(HookRegistry::new());

        let hook = Arc::new(TestBeforeHook::new(HookPriority::NORMAL).with_pattern("read*"));
        registry
            .register_tool_execute_before("test-plugin", hook)
            .await;

        let dispatcher = HookDispatcher::new(registry);

        // Should match "read"
        let input_read = ToolExecuteBeforeInput {
            tool: "read".to_string(),
            session_id: "session-1".to_string(),
            call_id: "call-1".to_string(),
            args: serde_json::json!({}),
        };
        let output = dispatcher
            .trigger_tool_execute_before(input_read)
            .await
            .unwrap();
        assert_eq!(output.args["hook_executed"], true);

        // Should match "read_file"
        let input_read_file = ToolExecuteBeforeInput {
            tool: "read_file".to_string(),
            session_id: "session-1".to_string(),
            call_id: "call-2".to_string(),
            args: serde_json::json!({}),
        };
        let output = dispatcher
            .trigger_tool_execute_before(input_read_file)
            .await
            .unwrap();
        assert_eq!(output.args["hook_executed"], true);

        // Should NOT match "write"
        let input_write = ToolExecuteBeforeInput {
            tool: "write".to_string(),
            session_id: "session-1".to_string(),
            call_id: "call-3".to_string(),
            args: serde_json::json!({}),
        };
        let output = dispatcher
            .trigger_tool_execute_before(input_write)
            .await
            .unwrap();
        assert!(output.args.get("hook_executed").is_none());
    }

    #[tokio::test]
    async fn test_hook_result_continue() {
        let registry = Arc::new(HookRegistry::new());

        let hook = Arc::new(TestBeforeHook::new(HookPriority::NORMAL));
        registry
            .register_tool_execute_before("test-plugin", hook)
            .await;

        let dispatcher = HookDispatcher::new(registry);

        let input = ToolExecuteBeforeInput {
            tool: "test".to_string(),
            session_id: "session-1".to_string(),
            call_id: "call-1".to_string(),
            args: serde_json::json!({}),
        };

        let output = dispatcher.trigger_tool_execute_before(input).await.unwrap();
        assert!(matches!(output.result, HookResult::Continue));
    }
}

// =============================================================================
// Command Registry Tests
// =============================================================================

mod command_tests {
    use super::*;
    use cortex_plugins::commands::{CommandExecutor, PluginCommandResult};

    fn create_test_command(plugin_id: &str, name: &str) -> PluginCommand {
        PluginCommand {
            plugin_id: plugin_id.to_string(),
            name: name.to_string(),
            aliases: vec![format!("{}alias", name)],
            description: format!("Test command {}", name),
            usage: Some(format!("/{} [args]", name)),
            args: vec![],
            hidden: false,
            category: None,
        }
    }

    fn create_test_executor() -> CommandExecutor {
        Arc::new(|args, _ctx| {
            Box::pin(async move {
                Ok(PluginCommandResult::success(format!(
                    "Executed with {} args",
                    args.len()
                )))
            })
        })
    }

    #[tokio::test]
    async fn test_command_registry_register() {
        let registry = PluginCommandRegistry::new();
        let cmd = create_test_command("test-plugin", "test-cmd");
        let executor = create_test_executor();

        let result = registry.register(cmd, executor).await;
        assert!(result.is_ok());
        assert!(registry.exists("test-cmd").await);
    }

    #[tokio::test]
    async fn test_command_registry_register_duplicate_fails() {
        let registry = PluginCommandRegistry::new();

        let cmd1 = create_test_command("plugin-1", "dup-cmd");
        let cmd2 = create_test_command("plugin-2", "dup-cmd");

        registry
            .register(cmd1, create_test_executor())
            .await
            .unwrap();
        let result = registry.register(cmd2, create_test_executor()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_command_registry_alias_lookup() {
        let registry = PluginCommandRegistry::new();
        let cmd = create_test_command("test-plugin", "mycommand");

        registry
            .register(cmd, create_test_executor())
            .await
            .unwrap();

        // Should find by name
        assert!(registry.exists("mycommand").await);
        // Should find by alias
        assert!(registry.exists("mycommandalias").await);
    }

    #[tokio::test]
    async fn test_command_registry_execute() {
        let registry = PluginCommandRegistry::new();
        let cmd = create_test_command("test-plugin", "exec-test");

        registry
            .register(cmd, create_test_executor())
            .await
            .unwrap();

        let ctx = PluginContext::default();
        let result = registry
            .execute(
                "exec-test",
                vec!["arg1".to_string(), "arg2".to_string()],
                &ctx,
            )
            .await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.success);
        assert!(result.message.unwrap().contains("2 args"));
    }

    #[tokio::test]
    async fn test_command_registry_execute_not_found() {
        let registry = PluginCommandRegistry::new();
        let ctx = PluginContext::default();

        let result = registry.execute("nonexistent", vec![], &ctx).await;

        assert!(result.is_err());
        match result {
            Err(PluginError::CommandError(msg)) => {
                assert!(msg.contains("not found"));
            }
            _ => panic!("Expected CommandError"),
        }
    }

    #[tokio::test]
    async fn test_command_registry_unregister_plugin() {
        let registry = PluginCommandRegistry::new();

        let cmd1 = create_test_command("plugin-a", "cmd-a");
        let cmd2 = create_test_command("plugin-b", "cmd-b");

        registry
            .register(cmd1, create_test_executor())
            .await
            .unwrap();
        registry
            .register(cmd2, create_test_executor())
            .await
            .unwrap();

        assert!(registry.exists("cmd-a").await);
        assert!(registry.exists("cmd-b").await);

        registry.unregister_plugin("plugin-a").await;

        assert!(!registry.exists("cmd-a").await);
        assert!(registry.exists("cmd-b").await);
    }

    #[tokio::test]
    async fn test_command_registry_list() {
        let registry = PluginCommandRegistry::new();

        registry
            .register(
                create_test_command("plugin", "cmd-1"),
                create_test_executor(),
            )
            .await
            .unwrap();
        registry
            .register(
                create_test_command("plugin", "cmd-2"),
                create_test_executor(),
            )
            .await
            .unwrap();

        let commands = registry.list().await;
        assert_eq!(commands.len(), 2);
    }

    #[tokio::test]
    async fn test_command_registry_list_visible_excludes_hidden() {
        let registry = PluginCommandRegistry::new();

        let visible_cmd = create_test_command("plugin", "visible");
        let mut hidden_cmd = create_test_command("plugin", "hidden");
        hidden_cmd.hidden = true;

        registry
            .register(visible_cmd, create_test_executor())
            .await
            .unwrap();
        registry
            .register(hidden_cmd, create_test_executor())
            .await
            .unwrap();

        let visible = registry.list_visible().await;
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "visible");
    }

    #[tokio::test]
    async fn test_command_registry_list_for_plugin() {
        let registry = PluginCommandRegistry::new();

        registry
            .register(
                create_test_command("plugin-a", "cmd-a1"),
                create_test_executor(),
            )
            .await
            .unwrap();
        registry
            .register(
                create_test_command("plugin-a", "cmd-a2"),
                create_test_executor(),
            )
            .await
            .unwrap();
        registry
            .register(
                create_test_command("plugin-b", "cmd-b1"),
                create_test_executor(),
            )
            .await
            .unwrap();

        let plugin_a_cmds = registry.list_for_plugin("plugin-a").await;
        assert_eq!(plugin_a_cmds.len(), 2);

        let plugin_b_cmds = registry.list_for_plugin("plugin-b").await;
        assert_eq!(plugin_b_cmds.len(), 1);
    }

    #[tokio::test]
    async fn test_command_registry_all_names() {
        let registry = PluginCommandRegistry::new();

        registry
            .register(
                create_test_command("plugin", "mycmd"),
                create_test_executor(),
            )
            .await
            .unwrap();

        let names = registry.all_names().await;
        assert!(names.contains(&"mycmd".to_string()));
        assert!(names.contains(&"mycmdalias".to_string()));
    }
}

// =============================================================================
// Error Condition Tests
// =============================================================================

mod error_tests {
    use super::*;

    #[tokio::test]
    async fn test_error_plugin_not_found() {
        let config = PluginConfig::default();
        let manager = PluginManager::new(config).await.unwrap();

        let result = manager.enable("nonexistent-plugin").await;

        assert!(result.is_err());
        match result {
            Err(PluginError::NotFound(id)) => {
                assert_eq!(id, "nonexistent-plugin");
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_error_invalid_state_transition() {
        let registry = PluginRegistry::new();
        let plugin = MockPlugin::new("state-error").with_state(PluginState::Unloaded);

        registry.register(Box::new(plugin)).await.unwrap();

        let handle = registry.get("state-error").await.unwrap();
        let result = {
            let mut p = handle.write().await;
            p.init().await
        };

        assert!(result.is_err());
        assert!(matches!(result, Err(PluginError::InvalidState { .. })));
    }

    #[tokio::test]
    async fn test_error_command_not_found() {
        let registry = PluginCommandRegistry::new();
        let ctx = PluginContext::default();

        let result = registry.execute("missing-command", vec![], &ctx).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(PluginError::CommandError(_))));
    }

    #[tokio::test]
    async fn test_error_duplicate_plugin() {
        let registry = PluginRegistry::new();

        registry
            .register(Box::new(MockPlugin::new("dup")))
            .await
            .unwrap();

        let result = registry.register(Box::new(MockPlugin::new("dup"))).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(PluginError::AlreadyExists(_))));
    }

    #[tokio::test]
    async fn test_error_manifest_validation_empty_id() {
        let manifest_str = r#"
[plugin]
id = ""
name = "Test"
version = "1.0.0"
"#;
        let manifest = PluginManifest::parse(manifest_str).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(matches!(result, Err(PluginError::InvalidManifest { .. })));
    }

    #[tokio::test]
    async fn test_error_manifest_validation_invalid_version() {
        let manifest_str = r#"
[plugin]
id = "test"
name = "Test"
version = "not-a-version"
"#;
        let manifest = PluginManifest::parse(manifest_str).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_error_manifest_validation_invalid_id_chars() {
        let manifest_str = r#"
[plugin]
id = "test/plugin"
name = "Test"
version = "1.0.0"
"#;
        let manifest = PluginManifest::parse(manifest_str).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
    }
}

// =============================================================================
// WASM Runtime Tests
// =============================================================================

mod wasm_runtime_tests {
    use super::*;

    #[test]
    fn test_wasm_runtime_creation() {
        let runtime = WasmRuntime::new();
        assert!(
            runtime.is_ok(),
            "WasmRuntime should be created successfully"
        );
    }

    #[test]
    fn test_wasm_runtime_compile_minimal_module() {
        let runtime = WasmRuntime::new().unwrap();
        let wasm_bytes = create_minimal_wasm_bytes();

        let module = runtime.compile(&wasm_bytes);
        assert!(module.is_ok(), "Should compile minimal WASM module");
    }

    #[test]
    fn test_wasm_runtime_compile_invalid_module() {
        let runtime = WasmRuntime::new().unwrap();
        let invalid_bytes = vec![0x00, 0x00, 0x00, 0x00]; // Not valid WASM

        let module = runtime.compile(&invalid_bytes);
        assert!(module.is_err(), "Should fail to compile invalid WASM");
    }

    #[tokio::test]
    async fn test_wasm_plugin_load_missing_file() {
        let runtime = Arc::new(WasmRuntime::new().unwrap());
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create manifest but no WASM file
        let manifest_content = create_valid_manifest("test-plugin", "Test", "1.0.0");
        let manifest = PluginManifest::parse(&manifest_content).unwrap();

        let plugin_result =
            cortex_plugins::WasmPlugin::new(manifest, temp_dir.path().to_path_buf(), runtime);

        assert!(plugin_result.is_ok());

        let mut plugin = plugin_result.unwrap();
        let load_result = plugin.load();

        assert!(
            load_result.is_err(),
            "Should fail to load non-existent WASM file"
        );
        match load_result {
            Err(PluginError::LoadError { plugin, message }) => {
                assert_eq!(plugin, "test-plugin");
                assert!(message.contains("not found"));
            }
            _ => panic!("Expected LoadError"),
        }
    }
}

// =============================================================================
// Plugin Configuration Tests
// =============================================================================

mod config_tests {
    use super::*;

    #[test]
    fn test_plugin_config_default() {
        let config = PluginConfig::default();

        assert!(!config.search_paths.is_empty());
        assert!(config.sandbox_enabled);
        assert_eq!(config.default_memory_pages, 256); // 16MB
        assert_eq!(config.default_timeout_ms, 30000);
    }

    #[test]
    fn test_plugin_config_is_enabled_default() {
        let config = PluginConfig::default();

        // All plugins enabled by default
        assert!(config.is_plugin_enabled("any-plugin"));
    }

    #[test]
    fn test_plugin_config_disable_plugin() {
        let mut config = PluginConfig::default();

        config.disable_plugin("disabled-plugin");

        assert!(!config.is_plugin_enabled("disabled-plugin"));
        assert!(config.is_plugin_enabled("other-plugin"));
    }

    #[test]
    fn test_plugin_config_enable_plugin() {
        let mut config = PluginConfig::default();

        config.disable_plugin("test-plugin");
        assert!(!config.is_plugin_enabled("test-plugin"));

        config.enable_plugin("test-plugin");
        assert!(config.is_plugin_enabled("test-plugin"));
    }

    #[test]
    fn test_plugin_config_get_set() {
        let mut config = PluginConfig::default();

        config.set_plugin_config(
            "my-plugin",
            serde_json::json!({
                "option1": true,
                "option2": "value"
            }),
        );

        let plugin_config = config.get_plugin_config("my-plugin").unwrap();
        assert_eq!(plugin_config["option1"], true);
        assert_eq!(plugin_config["option2"], "value");
    }

    #[test]
    fn test_plugin_config_get_nonexistent() {
        let config = PluginConfig::default();

        let result = config.get_plugin_config("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_plugin_config_enabled_list() {
        let config = PluginConfig {
            enabled_plugins: vec!["plugin-a".to_string(), "plugin-b".to_string()],
            ..Default::default()
        };

        assert!(config.is_plugin_enabled("plugin-a"));
        assert!(config.is_plugin_enabled("plugin-b"));
        assert!(!config.is_plugin_enabled("plugin-c")); // Not in enabled list
    }

    #[test]
    fn test_plugin_config_disabled_overrides_enabled() {
        let config = PluginConfig {
            enabled_plugins: vec!["plugin-a".to_string()],
            disabled_plugins: vec!["plugin-a".to_string()],
            ..Default::default()
        };

        // Disabled takes precedence
        assert!(!config.is_plugin_enabled("plugin-a"));
    }

    #[test]
    fn test_plugin_config_add_search_path() {
        let mut config = PluginConfig::default();
        let initial_count = config.search_paths.len();

        config.add_search_path(PathBuf::from("/custom/path"));

        assert_eq!(config.search_paths.len(), initial_count + 1);
        assert!(config.search_paths.contains(&PathBuf::from("/custom/path")));
    }

    #[test]
    fn test_plugin_config_add_search_path_no_duplicate() {
        let mut config = PluginConfig::default();
        let path = PathBuf::from("/unique/path");

        config.add_search_path(path.clone());
        let count_after_first = config.search_paths.len();

        config.add_search_path(path);
        assert_eq!(config.search_paths.len(), count_after_first);
    }
}

// =============================================================================
// Plugin Context Tests
// =============================================================================

mod context_tests {
    use super::*;

    #[test]
    fn test_plugin_context_default() {
        let ctx = PluginContext::default();

        assert!(ctx.session_id.is_none());
        assert!(ctx.message_id.is_none());
        assert!(ctx.agent.is_none());
        assert!(ctx.model.is_none());
        assert!(ctx.plugin_id.is_none());
        assert!(ctx.extra.is_empty());
    }

    #[test]
    fn test_plugin_context_new() {
        let ctx = PluginContext::new("/test/dir");

        assert_eq!(ctx.cwd, PathBuf::from("/test/dir"));
    }

    #[test]
    fn test_plugin_context_builder() {
        let ctx = PluginContext::new("/work")
            .with_session("session-123")
            .with_message("msg-456")
            .with_agent("test-agent")
            .with_model("gpt-4")
            .with_plugin("my-plugin")
            .with_extra("custom_key", serde_json::json!("custom_value"));

        assert_eq!(ctx.session_id, Some("session-123".to_string()));
        assert_eq!(ctx.message_id, Some("msg-456".to_string()));
        assert_eq!(ctx.agent, Some("test-agent".to_string()));
        assert_eq!(ctx.model, Some("gpt-4".to_string()));
        assert_eq!(ctx.plugin_id, Some("my-plugin".to_string()));
        assert_eq!(ctx.extra.get("custom_key").unwrap(), "custom_value");
    }
}
