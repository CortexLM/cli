//! Session-owned executable extension coordinator.
//!
//! Callers must preserve this order: before-hook -> final schema/path validation
//! -> existing approval/execpolicy/sandbox gate -> tool -> observer after-hook.
//! This API does not authorize execution, grant capabilities or contact a service.
use std::sync::Arc;

use cortex_plugins_ext::contract::{InvocationResult, Notification, ToolDeclaration};
use cortex_plugins_ext::{HookType, PluginConfig, PluginContext, PluginError, PluginManager};
use serde_json::{Value, json};
use tokio::sync::Mutex;

pub struct ExecutableSession {
    manager: Arc<PluginManager>,
    context: PluginContext,
    notifications: Mutex<Vec<Notification>>,
}

impl ExecutableSession {
    pub async fn start(
        config: PluginConfig,
        context: PluginContext,
    ) -> cortex_plugins_ext::Result<Self> {
        let manager = Arc::new(PluginManager::new(config).await?);
        manager.discover_and_load().await?;
        manager.init_all().await?;
        let session = Self {
            manager,
            context,
            notifications: Mutex::new(Vec::new()),
        };
        let result = session
            .dispatch(HookType::SessionStart, json!({}), None)
            .await;
        if let Err(error) = result {
            let _ = session.manager.shutdown_all().await;
            return Err(error);
        }
        Ok(session)
    }

    pub fn manager(&self) -> &Arc<PluginManager> {
        &self.manager
    }

    async fn dispatch(
        &self,
        kind: HookType,
        input: Value,
        call_id: Option<&str>,
    ) -> cortex_plugins_ext::Result<Value> {
        let context = self.call_context(call_id);
        let result = self.manager.dispatch_hook(kind, input, &context).await?;
        self.notifications.lock().await.extend(result.notifications);
        if result.denied {
            return Err(PluginError::hook_error(
                "plugins",
                result
                    .reason
                    .unwrap_or("Operation denied by a plugin".into()),
            ));
        }
        Ok(result.input)
    }

    fn call_context(&self, call_id: Option<&str>) -> PluginContext {
        let mut context = self.context.clone();
        if let Some(call_id) = call_id {
            context.extra.insert("call_id".into(), json!(call_id));
        }
        context
    }

    pub async fn before_tool(
        &self,
        tool: &str,
        call_id: &str,
        args: Value,
    ) -> cortex_plugins_ext::Result<Value> {
        let result = self
            .dispatch(
                HookType::ToolExecuteBefore,
                json!({"tool":tool,"args":args}),
                Some(call_id),
            )
            .await?;
        if result["tool"] != tool || result.get("args").is_none() {
            return Err(PluginError::hook_error(
                "plugins",
                "A hook cannot change tool identity or remove arguments",
            ));
        }
        Ok(result["args"].clone())
    }

    /// An observer never replaces the tool's success/failure status or output.
    pub async fn after_tool(
        &self,
        tool: &str,
        call_id: &str,
        success: bool,
        output: &Value,
    ) -> cortex_plugins_ext::Result<()> {
        self.dispatch(
            HookType::ToolExecuteAfter,
            json!({"tool":tool,"success":success,"output":output}),
            Some(call_id),
        )
        .await?;
        Ok(())
    }

    pub async fn preprocess_user_message(
        &self,
        content: &str,
    ) -> cortex_plugins_ext::Result<String> {
        let result = self
            .dispatch(
                HookType::ChatMessage,
                json!({"role":"user","content":content}),
                None,
            )
            .await?;
        if result["role"] != "user" {
            return Err(PluginError::hook_error(
                "plugins",
                "Hooks cannot elevate message roles",
            ));
        }
        result["content"].as_str().map(String::from).ok_or_else(|| {
            PluginError::hook_error("plugins", "Message hook returned non-text content")
        })
    }

    pub async fn tools(&self) -> Vec<(String, ToolDeclaration)> {
        self.manager.list_tools().await
    }

    /// Only call after the host security gate has approved the FINAL args.
    pub async fn execute_approved_tool(
        &self,
        plugin: &str,
        tool: &str,
        call_id: &str,
        args: Value,
    ) -> cortex_plugins_ext::Result<InvocationResult> {
        self.manager
            .execute_tool(plugin, tool, args, &self.call_context(Some(call_id)))
            .await
    }

    pub async fn command(
        &self,
        name: &str,
        args: Vec<String>,
    ) -> cortex_plugins_ext::Result<cortex_plugins_ext::commands::PluginCommandResult> {
        self.manager
            .execute_command(name, args, &self.context)
            .await
    }

    /// Only pass content-free product errors here, not credentials or transport diagnostics.
    pub async fn report_error(&self, message: &str) -> cortex_plugins_ext::Result<()> {
        self.dispatch(HookType::ErrorHandle, json!({"message":message}), None)
            .await?;
        Ok(())
    }

    pub async fn take_notifications(&self) -> Vec<Notification> {
        let mut result = std::mem::take(&mut *self.notifications.lock().await);
        result.extend(self.manager.take_notifications().await);
        result
    }

    pub async fn finish(&self) -> cortex_plugins_ext::Result<()> {
        let event = self.dispatch(HookType::SessionEnd, json!({}), None).await;
        let shutdown = self.manager.shutdown_all().await;
        event?;
        shutdown
    }
}
