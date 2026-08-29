//! Integration tests for the cortex-plugins crate.
//!
//! This module contains comprehensive integration tests covering:
//! - Plugin API and context
//! - SDK code generation
//! - Hook system integration
//! - Command registry edge cases
//! - Plugin signing and verification

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use cortex_plugins::{
    HookDispatcher,
    // Hooks
    HookPriority,
    HookRegistry,
    HookResult,
    // Config
    PluginCapability,
    // Commands
    PluginCommand,
    PluginCommandRegistry,
    // API
    PluginContext,
    // Error
    PluginError,
    // Manifest
    PluginManifest,
    // Registry
    PluginRegistry,
    // Signing
    PluginSigner,
    SessionStartHook,
    SessionStartInput,
    SessionStartOutput,
    ToolExecuteBeforeHook,
    ToolExecuteBeforeInput,
    ToolExecuteBeforeOutput,
    generate_advanced_rust_code,
    generate_cargo_toml,
    // SDK
    generate_manifest,
    generate_rust_code,
    generate_typescript_code,
};

// ============================================================================
// PLUGIN CONTEXT TESTS
// ============================================================================

mod context_tests {
    use super::*;

    #[test]
    fn test_context_builder_chain() {
        let ctx = PluginContext::new("/workspace")
            .with_session("sess-123")
            .with_message("msg-456")
            .with_agent("agent-1")
            .with_model("gpt-4")
            .with_plugin("my-plugin")
            .with_extra("key1", serde_json::json!("value1"))
            .with_extra("key2", serde_json::json!(42));

        assert_eq!(ctx.cwd, PathBuf::from("/workspace"));
        assert_eq!(ctx.session_id, Some("sess-123".to_string()));
        assert_eq!(ctx.message_id, Some("msg-456".to_string()));
        assert_eq!(ctx.agent, Some("agent-1".to_string()));
        assert_eq!(ctx.model, Some("gpt-4".to_string()));
        assert_eq!(ctx.plugin_id, Some("my-plugin".to_string()));
        assert_eq!(ctx.extra.get("key1").unwrap(), "value1");
        assert_eq!(ctx.extra.get("key2").unwrap(), 42);
    }

    #[test]
    fn test_context_default() {
        let ctx = PluginContext::default();

        assert!(ctx.session_id.is_none());
        assert!(ctx.extra.is_empty());
    }

    #[test]
    fn test_context_clone() {
        let ctx = PluginContext::new("/test")
            .with_session("sess")
            .with_extra("key", serde_json::json!("value"));

        let cloned = ctx.clone();

        assert_eq!(ctx.session_id, cloned.session_id);
        assert_eq!(ctx.extra, cloned.extra);
    }
}

// ============================================================================
// SDK CODE GENERATION TESTS
// ============================================================================

mod sdk_tests {
    use super::*;

    #[test]
    fn test_manifest_generation_all_fields() {
        let manifest = generate_manifest(
            "my-plugin-id",
            "My Awesome Plugin",
            "This plugin does awesome things",
            "John Doe <john@example.com>",
            "my-command",
            "Execute the awesome command",
        );

        // Verify it's valid TOML
        let parsed: Result<toml::Value, _> = toml::from_str(&manifest);
        assert!(parsed.is_ok(), "Generated manifest should be valid TOML");

        let value = parsed.unwrap();

        // Check plugin section
        assert_eq!(value["plugin"]["id"].as_str().unwrap(), "my-plugin-id");
        assert_eq!(
            value["plugin"]["name"].as_str().unwrap(),
            "My Awesome Plugin"
        );
        assert!(manifest.contains("John Doe <john@example.com>"));

        // Check command section
        assert!(manifest.contains("my-command"));
        assert!(manifest.contains("Execute the awesome command"));
    }

    #[test]
    fn test_rust_code_generation() {
        let code = generate_rust_code("Test Plugin", "test-cmd");

        // Check for required components
        assert!(code.contains("#![no_std]"), "Should have no_std");
        assert!(code.contains("extern crate alloc"), "Should have alloc");
        assert!(
            code.contains("#[panic_handler]"),
            "Should have panic handler"
        );
        assert!(
            code.contains("#[global_allocator]"),
            "Should have global allocator"
        );
        assert!(
            code.contains("pub extern \"C\" fn init()"),
            "Should have init"
        );
        assert!(
            code.contains("pub extern \"C\" fn shutdown()"),
            "Should have shutdown"
        );

        // Command handler should use snake_case
        assert!(
            code.contains("cmd_test_cmd"),
            "Command handler should be snake_case"
        );
    }

    #[test]
    fn test_cargo_toml_generation() {
        let cargo = generate_cargo_toml("my-plugin-crate");

        // Verify it's valid TOML
        let parsed: Result<toml::Value, _> = toml::from_str(&cargo);
        assert!(parsed.is_ok(), "Generated Cargo.toml should be valid TOML");

        let value = parsed.unwrap();

        // Check package
        assert_eq!(
            value["package"]["name"].as_str().unwrap(),
            "my-plugin-crate"
        );

        // Check lib settings
        assert!(cargo.contains("cdylib"), "Should have cdylib crate type");

        // Check release profile
        assert!(cargo.contains("lto = true"), "Should have LTO enabled");
    }

    #[test]
    fn test_advanced_rust_code_generation() {
        let code = generate_advanced_rust_code("my-plugin", "My Plugin", "cmd");

        // Check for TUI features
        assert!(
            code.contains("register_widget"),
            "Should have register_widget"
        );
        assert!(
            code.contains("register_keybinding"),
            "Should have register_keybinding"
        );
        assert!(code.contains("show_toast"), "Should have show_toast");

        // Check for hooks
        assert!(
            code.contains("hook_ui_render"),
            "Should have hook_ui_render"
        );
        assert!(
            code.contains("hook_animation_frame"),
            "Should have hook_animation_frame"
        );
        assert!(
            code.contains("hook_focus_change"),
            "Should have hook_focus_change"
        );
    }

    #[test]
    fn test_typescript_code_generation() {
        let code = generate_typescript_code("my-ts-plugin", "TS Plugin", "ts-cmd");

        // Check for exports
        assert!(code.contains("export function init"), "Should export init");
        assert!(
            code.contains("export function shutdown"),
            "Should export shutdown"
        );
        assert!(code.contains("my-ts-plugin"), "Should contain plugin ID");
        assert!(code.contains("cmd_ts_cmd"), "Should have command handler");
    }

    #[test]
    fn test_command_name_snake_case_conversion() {
        let code = generate_rust_code("Plugin", "multi-word-command-name");
        assert!(
            code.contains("cmd_multi_word_command_name"),
            "Should convert hyphens to underscores"
        );
    }
}

// ============================================================================
// PLUGIN SIGNING TESTS
// ============================================================================

mod signing_tests {
    use super::*;

    #[test]
    fn test_checksum_consistency() {
        let data = b"test data for checksum verification";

        // Compute checksum multiple times
        let checksum1 = PluginSigner::compute_checksum(data);
        let checksum2 = PluginSigner::compute_checksum(data);
        let checksum3 = PluginSigner::compute_checksum(data);

        assert_eq!(checksum1, checksum2);
        assert_eq!(checksum2, checksum3);
    }

    #[test]
    fn test_checksum_different_for_different_data() {
        let data1 = b"first data";
        let data2 = b"second data";
        let data3 = b"first data"; // Same as data1

        let checksum1 = PluginSigner::compute_checksum(data1);
        let checksum2 = PluginSigner::compute_checksum(data2);
        let checksum3 = PluginSigner::compute_checksum(data3);

        assert_ne!(checksum1, checksum2);
        assert_eq!(checksum1, checksum3);
    }

    #[test]
    fn test_signer_verify_checksum() {
        let data = b"test plugin data";
        let checksum = PluginSigner::compute_checksum(data);

        // Valid checksum
        let computed = PluginSigner::compute_checksum(data);
        assert_eq!(computed, checksum);

        // Invalid checksum - should not match
        let wrong_data = b"wrong data";
        let wrong_checksum = PluginSigner::compute_checksum(wrong_data);
        assert_ne!(checksum, wrong_checksum);
    }

    #[test]
    fn test_checksum_case_insensitive() {
        let data = b"test";
        let checksum = PluginSigner::compute_checksum(data);

        // Checksum should be lowercase hex
        assert_eq!(checksum, checksum.to_lowercase());

        // Compare uppercase and lowercase - they represent the same value
        assert_eq!(
            checksum.to_uppercase().to_lowercase(),
            checksum.to_lowercase()
        );
    }

    #[test]
    fn test_signer_trusted_keys_management() {
        let mut signer = PluginSigner::new();

        assert!(!signer.has_trusted_keys());

        // Add a valid 32-byte key
        let valid_key = vec![0u8; 32];
        let result = signer.add_trusted_key(&valid_key);
        assert!(result.is_ok());
        assert!(signer.has_trusted_keys());
    }
}

// ============================================================================
// HOOK SYSTEM INTEGRATION TESTS
// ============================================================================

mod hook_integration_tests {
    use super::*;

    struct ModifyingHook {
        priority: HookPriority,
        field_to_add: String,
    }

    #[async_trait::async_trait]
    impl ToolExecuteBeforeHook for ModifyingHook {
        fn priority(&self) -> HookPriority {
            self.priority
        }

        async fn execute(
            &self,
            _input: &ToolExecuteBeforeInput,
            output: &mut ToolExecuteBeforeOutput,
        ) -> cortex_plugins::Result<()> {
            if let Some(obj) = output.args.as_object_mut() {
                obj.insert(self.field_to_add.clone(), serde_json::json!(true));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_multiple_hooks_modify_args() {
        let registry = Arc::new(HookRegistry::new());

        // Register three hooks that each add a field
        let hook1 = Arc::new(ModifyingHook {
            priority: HookPriority::PLUGIN_HIGH,
            field_to_add: "hook1_executed".to_string(),
        });
        let hook2 = Arc::new(ModifyingHook {
            priority: HookPriority::NORMAL,
            field_to_add: "hook2_executed".to_string(),
        });
        let hook3 = Arc::new(ModifyingHook {
            priority: HookPriority::LOW,
            field_to_add: "hook3_executed".to_string(),
        });

        registry
            .register_tool_execute_before("plugin1", hook1)
            .await;
        registry
            .register_tool_execute_before("plugin2", hook2)
            .await;
        registry
            .register_tool_execute_before("plugin3", hook3)
            .await;

        let dispatcher = HookDispatcher::new(registry);

        let input = ToolExecuteBeforeInput {
            tool: "test_tool".to_string(),
            session_id: "session".to_string(),
            call_id: "call".to_string(),
            args: serde_json::json!({"original": "value"}),
        };

        let output = dispatcher.trigger_tool_execute_before(input).await.unwrap();

        // All hooks should have executed and added their fields
        assert_eq!(output.args["original"], "value");
        assert_eq!(output.args["hook1_executed"], true);
        assert_eq!(output.args["hook2_executed"], true);
        assert_eq!(output.args["hook3_executed"], true);
    }

    struct AbortingHook {
        priority: HookPriority,
        reason: String,
    }

    #[async_trait::async_trait]
    impl ToolExecuteBeforeHook for AbortingHook {
        fn priority(&self) -> HookPriority {
            self.priority
        }

        async fn execute(
            &self,
            _input: &ToolExecuteBeforeInput,
            output: &mut ToolExecuteBeforeOutput,
        ) -> cortex_plugins::Result<()> {
            output.result = HookResult::Abort {
                reason: self.reason.clone(),
            };
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_abort_stops_later_hooks() {
        let registry = Arc::new(HookRegistry::new());

        // High priority hook that aborts
        let abort_hook = Arc::new(AbortingHook {
            priority: HookPriority::PLUGIN_HIGH,
            reason: "Security policy violation".to_string(),
        });

        // Lower priority hook that should not execute
        let modify_hook = Arc::new(ModifyingHook {
            priority: HookPriority::NORMAL,
            field_to_add: "should_not_appear".to_string(),
        });

        registry
            .register_tool_execute_before("aborter", abort_hook)
            .await;
        registry
            .register_tool_execute_before("modifier", modify_hook)
            .await;

        let dispatcher = HookDispatcher::new(registry);

        let input = ToolExecuteBeforeInput {
            tool: "dangerous_tool".to_string(),
            session_id: "session".to_string(),
            call_id: "call".to_string(),
            args: serde_json::json!({}),
        };

        let output = dispatcher.trigger_tool_execute_before(input).await.unwrap();

        // Should have abort result
        match output.result {
            HookResult::Abort { reason } => {
                assert_eq!(reason, "Security policy violation");
            }
            _ => panic!("Expected Abort result"),
        }

        // The modify hook should NOT have executed
        assert!(output.args.get("should_not_appear").is_none());
    }

    #[tokio::test]
    async fn test_session_hooks() {
        struct TestSessionStartHook;

        #[async_trait::async_trait]
        impl SessionStartHook for TestSessionStartHook {
            fn priority(&self) -> HookPriority {
                HookPriority::NORMAL
            }

            async fn execute(
                &self,
                _input: &SessionStartInput,
                output: &mut SessionStartOutput,
            ) -> cortex_plugins::Result<()> {
                output
                    .system_prompt_additions
                    .push("Welcome to the session!".to_string());
                Ok(())
            }
        }

        let registry = Arc::new(HookRegistry::new());
        let hook = Arc::new(TestSessionStartHook);
        registry.register_session_start("test-plugin", hook).await;

        // Verify registration
        assert_eq!(
            registry
                .hook_count(cortex_plugins::HookType::SessionStart)
                .await,
            1
        );
    }
}

// ============================================================================
// COMMAND REGISTRY TESTS
// ============================================================================

mod command_registry_tests {
    use super::*;
    use cortex_plugins::commands::{CommandExecutor, PluginCommandResult};

    fn create_test_command(plugin_id: &str, name: &str) -> PluginCommand {
        PluginCommand {
            plugin_id: plugin_id.to_string(),
            name: name.to_string(),
            aliases: vec![],
            description: format!("Test command {}", name),
            usage: None,
            args: vec![],
            hidden: false,
            category: Some("test".to_string()),
        }
    }

    fn create_executor_with_result(result: &'static str) -> CommandExecutor {
        Arc::new(move |_args, _ctx| {
            let result = result.to_string();
            Box::pin(async move { Ok(PluginCommandResult::success(result)) })
        })
    }

    #[tokio::test]
    async fn test_command_by_category() {
        let registry = PluginCommandRegistry::new();

        let mut cmd1 = create_test_command("plugin", "cmd1");
        cmd1.category = Some("category-a".to_string());

        let mut cmd2 = create_test_command("plugin", "cmd2");
        cmd2.category = Some("category-b".to_string());

        let mut cmd3 = create_test_command("plugin", "cmd3");
        cmd3.category = Some("category-a".to_string());

        registry
            .register(cmd1, create_executor_with_result("r1"))
            .await
            .unwrap();
        registry
            .register(cmd2, create_executor_with_result("r2"))
            .await
            .unwrap();
        registry
            .register(cmd3, create_executor_with_result("r3"))
            .await
            .unwrap();

        let all_commands = registry.list().await;
        assert_eq!(all_commands.len(), 3);

        // Filter by category
        let category_a: Vec<_> = all_commands
            .iter()
            .filter(|c| c.category.as_deref() == Some("category-a"))
            .collect();
        assert_eq!(category_a.len(), 2);
    }

    #[tokio::test]
    async fn test_command_with_aliases() {
        let registry = PluginCommandRegistry::new();

        let cmd = PluginCommand {
            plugin_id: "test".to_string(),
            name: "my-long-command".to_string(),
            aliases: vec!["mlc".to_string(), "mycommand".to_string()],
            description: "Test".to_string(),
            usage: None,
            args: vec![],
            hidden: false,
            category: None,
        };

        registry
            .register(cmd, create_executor_with_result("ok"))
            .await
            .unwrap();

        // Should find by name
        assert!(registry.exists("my-long-command").await);
        // Should find by aliases
        assert!(registry.exists("mlc").await);
        assert!(registry.exists("mycommand").await);
        // Should NOT find by partial name
        assert!(!registry.exists("my-long").await);
    }

    #[tokio::test]
    async fn test_execute_with_args() {
        let registry = PluginCommandRegistry::new();

        let cmd = create_test_command("plugin", "echo");

        let executor: CommandExecutor = Arc::new(|args, _ctx| {
            Box::pin(async move {
                let result = args.join(" ");
                Ok(PluginCommandResult::success(result))
            })
        });

        registry.register(cmd, executor).await.unwrap();

        let ctx = PluginContext::default();
        let result = registry
            .execute("echo", vec!["hello".to_string(), "world".to_string()], &ctx)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.message.unwrap(), "hello world");
    }

    #[tokio::test]
    async fn test_command_registry_unregister_all() {
        let registry = PluginCommandRegistry::new();

        registry
            .register(
                create_test_command("plugin-a", "cmd1"),
                create_executor_with_result("r1"),
            )
            .await
            .unwrap();
        registry
            .register(
                create_test_command("plugin-a", "cmd2"),
                create_executor_with_result("r2"),
            )
            .await
            .unwrap();

        assert_eq!(registry.list().await.len(), 2);

        // Unregister all commands for the plugin
        registry.unregister_plugin("plugin-a").await;

        assert_eq!(registry.list().await.len(), 0);
    }
}

// ============================================================================
// MANIFEST PARSING TESTS
// ============================================================================

mod manifest_tests {
    use super::*;

    #[test]
    fn test_manifest_with_all_capabilities() {
        let manifest_str = r#"
capabilities = ["commands", "hooks", "events", "tools", "config", "network"]

[plugin]
id = "full-plugin"
name = "Full Plugin"
version = "1.0.0"
description = "A plugin with all capabilities"
"#;

        let manifest = PluginManifest::parse(manifest_str).expect("should parse");

        assert!(manifest.has_capability(PluginCapability::Commands));
        assert!(manifest.has_capability(PluginCapability::Hooks));
        assert!(manifest.has_capability(PluginCapability::Events));
        assert!(manifest.has_capability(PluginCapability::Tools));
        assert!(manifest.has_capability(PluginCapability::Config));
        assert!(manifest.has_capability(PluginCapability::Network));
    }

    #[test]
    fn test_manifest_with_wasm_settings() {
        let manifest_str = r#"
[plugin]
id = "wasm-plugin"
name = "WASM Plugin"
version = "1.0.0"

[wasm]
memory_pages = 512
timeout_ms = 60000
wasi_enabled = false
"#;

        let manifest = PluginManifest::parse(manifest_str).expect("should parse");

        assert_eq!(manifest.wasm.memory_pages, 512);
        assert_eq!(manifest.wasm.timeout_ms, 60000);
        assert!(!manifest.wasm.wasi_enabled);
    }

    #[test]
    fn test_manifest_with_dependencies() {
        let manifest_str = r#"
[plugin]
id = "dependent-plugin"
name = "Dependent Plugin"
version = "1.0.0"

[[dependencies]]
id = "base-plugin"
version = ">=1.0.0"
optional = false

[[dependencies]]
id = "optional-plugin"
version = "^2.0.0"
optional = true
"#;

        let manifest = PluginManifest::parse(manifest_str).expect("should parse");

        assert_eq!(manifest.dependencies.len(), 2);

        let base = manifest
            .dependencies
            .iter()
            .find(|d| d.id == "base-plugin")
            .unwrap();
        assert!(!base.optional);

        let optional = manifest
            .dependencies
            .iter()
            .find(|d| d.id == "optional-plugin")
            .unwrap();
        assert!(optional.optional);
    }

    #[test]
    fn test_manifest_with_hooks() {
        let manifest_str = r#"
[plugin]
id = "hook-plugin"
name = "Hook Plugin"
version = "1.0.0"

[[hooks]]
hook_type = "tool_execute_before"
priority = 50
pattern = "read*"

[[hooks]]
hook_type = "session_start"
priority = 100
"#;

        let manifest = PluginManifest::parse(manifest_str).expect("should parse");

        assert_eq!(manifest.hooks.len(), 2);
        assert_eq!(manifest.hooks[0].priority, 50);
        assert_eq!(manifest.hooks[0].pattern, Some("read*".to_string()));
    }
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

mod error_tests {
    use super::*;

    #[test]
    fn test_error_chaining() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "plugin.wasm not found");
        let plugin_error: PluginError = io_error.into();

        let error_string = plugin_error.to_string();
        assert!(error_string.contains("not found") || error_string.contains("I/O"));
    }

    #[test]
    fn test_plugin_error_variants() {
        let errors: Vec<PluginError> = vec![
            PluginError::NotFound("test-plugin".to_string()),
            PluginError::AlreadyExists("test-plugin".to_string()),
            PluginError::Disabled("test-plugin".to_string()),
            PluginError::Timeout("operation".to_string()),
            PluginError::ConfigError("invalid config".to_string()),
            PluginError::PermissionDenied("network".to_string()),
            PluginError::NetworkError("connection failed".to_string()),
            PluginError::RegistryError("registry unavailable".to_string()),
            PluginError::SignatureError("invalid signature".to_string()),
            PluginError::CommandError("command failed".to_string()),
        ];

        for error in errors {
            // All errors should be displayable
            let display = error.to_string();
            assert!(!display.is_empty());

            // All errors should be debuggable
            let debug = format!("{:?}", error);
            assert!(!debug.is_empty());
        }
    }
}

// ============================================================================
// REGISTRY INTEGRATION TESTS
// ============================================================================

mod registry_integration_tests {
    use super::*;
    use cortex_plugins::plugin::{Plugin, PluginInfo, PluginState};

    // Mock plugin for testing
    struct TestPlugin {
        info: PluginInfo,
        manifest: PluginManifest,
        state: PluginState,
    }

    impl TestPlugin {
        fn new(id: &str, version: &str) -> Self {
            let manifest = PluginManifest {
                plugin: cortex_plugins::manifest::PluginMetadata {
                    id: id.to_string(),
                    name: format!("Test Plugin {}", id),
                    version: version.to_string(),
                    description: "A test plugin".to_string(),
                    authors: vec!["Test Author".to_string()],
                    homepage: None,
                    license: Some("MIT".to_string()),
                    min_cortex_version: None,
                    keywords: vec!["test".to_string()],
                    icon: None,
                },
                capabilities: vec![PluginCapability::Commands],
                permissions: vec![],
                dependencies: vec![],
                commands: vec![],
                hooks: vec![],
                config: HashMap::new(),
                wasm: Default::default(),
            };

            let info = PluginInfo::from_manifest(&manifest, PathBuf::from("/tmp"));

            Self {
                info,
                manifest,
                state: PluginState::Loaded,
            }
        }
    }

    #[async_trait::async_trait]
    impl Plugin for TestPlugin {
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
            args: Vec<String>,
            _ctx: &PluginContext,
        ) -> cortex_plugins::Result<String> {
            Ok(format!("Executed {} with {} args", name, args.len()))
        }

        fn get_config(&self, _key: &str) -> Option<serde_json::Value> {
            None
        }

        fn set_config(
            &mut self,
            _key: &str,
            _value: serde_json::Value,
        ) -> cortex_plugins::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_registry_with_multiple_versions() {
        let registry = PluginRegistry::new();

        // Register first version
        let plugin_v1 = TestPlugin::new("versioned-plugin", "1.0.0");
        registry.register(Box::new(plugin_v1)).await.unwrap();

        // Try to register second version (should fail - same ID)
        let plugin_v2 = TestPlugin::new("versioned-plugin", "2.0.0");
        let result = registry.register(Box::new(plugin_v2)).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_shutdown_order() {
        let registry = PluginRegistry::new();

        // Register multiple plugins
        registry
            .register(Box::new(TestPlugin::new("plugin-a", "1.0.0")))
            .await
            .unwrap();
        registry
            .register(Box::new(TestPlugin::new("plugin-b", "1.0.0")))
            .await
            .unwrap();
        registry
            .register(Box::new(TestPlugin::new("plugin-c", "1.0.0")))
            .await
            .unwrap();

        // Init all
        registry.init_all().await;

        // Verify all active
        let active = registry.active_plugin_ids().await;
        assert_eq!(active.len(), 3);

        // Shutdown all
        let results = registry.shutdown_all().await;

        // All should succeed
        assert!(results.iter().all(|(_, r)| r.is_ok()));

        // All should be unloaded
        let active = registry.active_plugin_ids().await;
        assert!(active.is_empty());
    }
}
