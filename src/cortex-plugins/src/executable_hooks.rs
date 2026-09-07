//! Deterministic manifest hook dispatch shared by the CLI and session bridge.
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::contract::{HookDecision, HookOutcome, Notification};
use crate::manifest::{HookType, PluginHookManifest};
use crate::registry::PluginRegistry;
use crate::{PluginContext, PluginError, PluginState, Result};

#[derive(Debug)]
pub struct DispatchOutcome {
    pub input: Value,
    pub denied: bool,
    pub reason: Option<String>,
    pub notifications: Vec<Notification>,
}

pub struct ExecutableHooks {
    registry: Arc<PluginRegistry>,
    declarations: RwLock<BTreeMap<String, Vec<PluginHookManifest>>>,
}

impl ExecutableHooks {
    pub fn new(registry: Arc<PluginRegistry>) -> Self {
        Self {
            registry,
            declarations: RwLock::new(BTreeMap::new()),
        }
    }

    pub async fn register(&self, id: &str, hooks: &[PluginHookManifest]) {
        self.declarations
            .write()
            .await
            .insert(id.into(), hooks.to_vec());
    }

    pub async fn unregister(&self, id: &str) {
        self.declarations.write().await.remove(id);
    }

    pub async fn dispatch(
        &self,
        kind: HookType,
        mut input: Value,
        ctx: &PluginContext,
    ) -> Result<DispatchOutcome> {
        let mut ordered = Vec::new();
        for (id, hooks) in self.declarations.read().await.iter() {
            for (index, hook) in hooks
                .iter()
                .enumerate()
                .filter(|(_, h)| h.hook_type == kind)
            {
                ordered.push((hook.priority, id.clone(), index, hook.clone()));
            }
        }
        ordered.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
        let mut notifications = Vec::new();
        for (_, id, _, hook) in ordered {
            if !matches_pattern(&hook, &input) {
                continue;
            }
            let handle = self
                .registry
                .get(&id)
                .await
                .ok_or_else(|| PluginError::NotFound(id.clone()))?;
            let plugin = handle.read().await;
            if plugin.state() == PluginState::Unloaded || plugin.state() == PluginState::Disabled {
                continue;
            }
            if plugin.state() != PluginState::Active {
                return Err(PluginError::hook_error(id, "Plugin is not active"));
            }
            let result = plugin
                .invoke(
                    "hook",
                    &crate::node::hook_function(&hook),
                    input.clone(),
                    ctx,
                )
                .await?;
            let outcome: HookOutcome = serde_json::from_value(result.data)?;
            let observer = matches!(
                kind,
                HookType::ToolExecuteAfter | HookType::SessionEnd | HookType::ErrorHandle
            );
            if observer
                && (outcome.input.is_some() || matches!(outcome.decision, HookDecision::Deny))
            {
                return Err(PluginError::hook_error(
                    id,
                    "Observer hooks cannot change or replace operation results",
                ));
            }
            notifications.extend(result.notifications);
            if notifications.len() > 64 {
                return Err(PluginError::hook_error(
                    id,
                    "Too many lifecycle notifications",
                ));
            }
            if matches!(outcome.decision, HookDecision::Deny) {
                return Ok(DispatchOutcome {
                    input,
                    denied: true,
                    reason: outcome.reason,
                    notifications,
                });
            }
            if let Some(updated) = outcome.input {
                input = updated;
            }
        }
        Ok(DispatchOutcome {
            input,
            denied: false,
            reason: None,
            notifications,
        })
    }
}

fn matches_pattern(hook: &PluginHookManifest, input: &Value) -> bool {
    let Some(pattern) = &hook.pattern else {
        return true;
    };
    let name = input["tool"].as_str().unwrap_or("");
    if pattern == "*" {
        return true;
    }
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        return !suffix.contains('*') && name.starts_with(prefix) && name.ends_with(suffix);
    }
    name == pattern
}
