//! Version 1 runtime contract shared by native TypeScript and WASM plugins.
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{PluginError, Result};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_ARTIFACT_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PACKAGE_FILES: usize = 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    #[default]
    Wasm,
    Node,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeSettings {
    pub kind: RuntimeKind,
    pub protocol: u32,
    pub entrypoint: String,
    pub timeout_ms: u64,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            kind: RuntimeKind::Wasm,
            protocol: PROTOCOL_VERSION,
            entrypoint: "plugin.wasm".into(),
            timeout_ms: 5000,
        }
    }
}

/// The supported schema subset is deliberately small and strictly validated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDeclaration {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notification {
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationResult {
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub notifications: Vec<Notification>,
}

impl InvocationResult {
    pub fn validate(&self, notifications_allowed: bool) -> Result<()> {
        if self.notifications.len() > 16
            || (!notifications_allowed && !self.notifications.is_empty())
        {
            return Err(PluginError::PermissionDenied(
                "Plugin notifications are not permitted or exceed the limit".into(),
            ));
        }
        for item in &self.notifications {
            if !["info", "success", "warning", "error"].contains(&item.level.as_str())
                || item.message.len() > 4096
                || item.message.chars().any(|c| c.is_control() && c != '\n')
            {
                return Err(PluginError::validation_error(
                    "notification",
                    "Invalid notification",
                ));
            }
        }
        if serde_json::to_vec(self)?.len() > MAX_FRAME_BYTES {
            return Err(PluginError::validation_error(
                "result",
                "Plugin result is too large",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookDecision {
    #[default]
    Continue,
    Deny,
}

/// No allow, replacement-success, or permission-grant result exists.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookOutcome {
    #[serde(default)]
    pub decision: HookDecision,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub input: Option<Value>,
}

pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 64
        || !id.as_bytes()[0].is_ascii_alphanumeric()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(PluginError::validation_error(
            "plugin ID",
            "Invalid plugin path identifier: use 1–64 ASCII letters, digits, hyphens or underscores; start with a letter or digit",
        ));
    }
    Ok(())
}

pub fn validate_schema(schema: &Value, depth: usize) -> Result<()> {
    let err =
        || PluginError::validation_error("input_schema", "Unsupported or invalid tool schema");
    let object = schema.as_object().ok_or_else(err)?;
    if depth > 8 || object.len() > 5 {
        return Err(err());
    }
    for key in object.keys() {
        if ![
            "type",
            "properties",
            "required",
            "additionalProperties",
            "description",
        ]
        .contains(&key.as_str())
        {
            return Err(err());
        }
    }
    match schema["type"].as_str() {
        Some("object") => {
            if schema["additionalProperties"] != false {
                return Err(err());
            }
            let props = schema["properties"].as_object().ok_or_else(err)?;
            if props.len() > 64 {
                return Err(err());
            }
            for sub in props.values() {
                validate_schema(sub, depth + 1)?;
            }
            if let Some(required) = schema.get("required") {
                for name in required.as_array().ok_or_else(err)? {
                    if !props.contains_key(name.as_str().ok_or_else(err)?) {
                        return Err(err());
                    }
                }
            }
        }
        Some("string" | "number" | "integer" | "boolean" | "null") => {
            if object.keys().any(|k| k != "type" && k != "description") {
                return Err(err());
            }
        }
        _ => return Err(err()),
    }
    Ok(())
}

pub fn validate_arguments(schema: &Value, value: &Value) -> Result<()> {
    let valid = match schema["type"].as_str() {
        Some("object") => return validate_object(schema, value),
        Some("string") => value.is_string(),
        Some("number") => value.is_number(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("boolean") => value.is_boolean(),
        Some("null") => value.is_null(),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(PluginError::validation_error(
            "arguments",
            "Tool argument type does not match schema",
        ))
    }
}

fn validate_object(schema: &Value, value: &Value) -> Result<()> {
    let err = || PluginError::validation_error("arguments", "Tool arguments do not match schema");
    let object = value.as_object().ok_or_else(err)?;
    let props = schema["properties"].as_object().ok_or_else(err)?;
    if let Some(required) = schema["required"].as_array() {
        for name in required {
            if !object.contains_key(name.as_str().ok_or_else(err)?) {
                return Err(err());
            }
        }
    }
    for (key, value) in object {
        validate_arguments(props.get(key).ok_or_else(err)?, value)?;
    }
    Ok(())
}
