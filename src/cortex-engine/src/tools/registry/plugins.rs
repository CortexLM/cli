//! Plugin loading and execution for the tool registry.

use serde_json::{Value, json};

use super::ToolRegistry;
use super::types::PluginTool;
use crate::error::Result;
use crate::tools::spec::{ToolDefinition, ToolResult};

impl ToolRegistry {
    /// Register a plugin tool.
    pub fn register_plugin(&mut self, plugin: PluginTool) {
        if self.has(&plugin.name) {
            tracing::warn!("Plugin registration refused: tool name is already registered");
            return;
        }
        // Also register as a regular tool definition
        self.tools.insert(
            plugin.name.clone(),
            ToolDefinition::new(&plugin.name, &plugin.description, plugin.parameters.clone()),
        );
        self.plugins.insert(plugin.name.clone(), plugin);
    }

    /// Load plugins from a directory.
    pub async fn load_plugins_from_dir(&mut self, dir: &std::path::Path) -> Result<usize> {
        let mut count = 0;

        if !dir.exists() {
            return Ok(0);
        }

        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => return Ok(0),
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();

            // Look for plugin.json or tool.json files
            if path.is_dir() {
                let plugin_json = path.join("plugin.json");
                let tool_json = path.join("tool.json");

                let config_path = if plugin_json.exists() {
                    Some(plugin_json)
                } else if tool_json.exists() {
                    Some(tool_json)
                } else {
                    None
                };

                if let Some(config_path) = config_path
                    && let Ok(content) = tokio::fs::read_to_string(&config_path).await
                    && let Ok(config) = serde_json::from_str::<Value>(&content)
                {
                    let name = config
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");
                    let description = config
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("");
                    let parameters = config
                        .get("parameters")
                        .cloned()
                        .unwrap_or(json!({"type": "object", "properties": {}}));
                    let script = config
                        .get("script")
                        .and_then(|s| s.as_str())
                        .unwrap_or("run.sh");

                    let plugin = PluginTool {
                        name: name.to_string(),
                        description: description.to_string(),
                        parameters,
                        script_path: path.join(script),
                    };

                    self.register_plugin(plugin);
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Get list of registered plugins.
    pub fn list_plugins(&self) -> Vec<&PluginTool> {
        self.plugins.values().collect()
    }

    /// Execute a plugin tool.
    pub(super) async fn execute_plugin(
        &self,
        plugin: &PluginTool,
        arguments: Value,
        context: &crate::tools::ToolContext,
    ) -> Result<ToolResult> {
        let path =
            match context.resolve_and_validate_path(plugin.script_path.to_str().unwrap_or("")) {
                Ok(path) => path,
                Err(message) => return Ok(ToolResult::error(message)),
            };

        // Serialize arguments to JSON for the script
        let args_json = serde_json::to_string(&arguments)?;

        // Execute the plugin script
        let mut env = context.env.clone();
        env.insert("CORTEX_PLUGIN_ARGS".into(), args_json.clone());
        let output = crate::exec::execute_command(
            &[path.to_string_lossy().into_owned(), args_json],
            crate::exec::ExecOptions {
                cwd: context
                    .resolve_and_validate_path(".")
                    .map_err(crate::error::CortexError::InvalidInput)?,
                sandbox_policy: context.sandbox_policy.clone(),
                env,
                approval_granted: true,
                ..Default::default()
            },
        )
        .await;

        match output {
            Ok(output) => {
                if output.exit_code == 0 && !output.timed_out {
                    Ok(ToolResult::success(output.stdout))
                } else {
                    Ok(ToolResult::error(output.aggregated))
                }
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to execute plugin: {e}"))),
        }
    }
}
