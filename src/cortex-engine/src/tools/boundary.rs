//! Authority shared by registry, router, Batch and extension dispatch.
//!
//! These are in-process tool checks, not a replacement for the OS sandbox.
use serde_json::Value;

use super::{ToolContext, ToolResult};

/// Pure authorization check for callers with their own dispatch (for example MCP).
/// Authority must come from a trusted caller, never model arguments/environment.
pub fn check_tool_call(context: &ToolContext, name: &str, arguments: &Value) -> Result<(), String> {
    context.resolve_and_validate_path(".")?;
    if name == "WebSearch" {
        return Err("Web search is unavailable until its destination policy is configured".into());
    }
    if context.is_child() && !crate::harness::child_tool_allowed(name) {
        return Err(format!("Child tasks cannot use {name}."));
    }
    if context.is_tool_denied(name) {
        return Err(format!("Permission denied for tool: {name}"));
    }
    let read_only = is_read_only_tool(name);
    if context.is_read_only() && !read_only {
        return Err("Read-only mode prohibits this tool.".into());
    }
    if !read_only && !context.is_approved(name, arguments) {
        return Err(format!("Approval required for tool: {name}"));
    }
    Ok(())
}

/// Authorize one dispatch, consuming an exact one-call grant. Use this at
/// external dispatch boundaries (MCP/Task), not just the pure preview check.
pub fn authorize_tool_call(
    context: &ToolContext,
    name: &str,
    arguments: &Value,
) -> Result<(), String> {
    check_tool_call(context, name, arguments)?;
    if !is_read_only_tool(name) && !context.consume_approval(name, arguments) {
        return Err(format!("Approval required for tool: {name}"));
    }
    Ok(())
}

/// Unknown tools are effectful. Read-only status is never accepted from an
/// untrusted plugin manifest, tool description, or argument.
pub fn is_read_only_tool(name: &str) -> bool {
    matches!(
        name,
        "Read"
            | "LS"
            | "Tree"
            | "Grep"
            | "Glob"
            | "SearchFiles"
            | "TodoRead"
            | "TodoWrite"
            | "Plan"
            | "UpdateGoal"
            | "Propose"
            | "Questions"
            | "ListSubagents"
            | "Batch"
            | "Skill"
            | "LspHover"
            | "LspDiagnostics"
            | "LspSymbols"
    )
}

/// Validate/resolve all file paths consumed by built-in handlers before dispatch.
pub(crate) fn prepare_arguments(
    context: &ToolContext,
    name: &str,
    mut args: Value,
) -> Result<Value, String> {
    match name {
        "Read" => resolve_field(context, &mut args, &["file_path", "path"], false, None)?,
        "Create" | "Edit" => resolve_field(context, &mut args, &["file_path", "path"], true, None)?,
        "LS" | "Tree" => resolve_field(
            context,
            &mut args,
            &["directory_path", "path"],
            false,
            Some("."),
        )?,
        "Grep" | "SearchFiles" | "LspSymbols" | "LspDiagnostics" => {
            resolve_field(context, &mut args, &["path"], false, Some("."))?
        }
        "Glob" => resolve_field(context, &mut args, &["folder", "path"], false, Some("."))?,
        "LspHover" => resolve_field(context, &mut args, &["file", "file_path"], false, None)?,
        "Execute" => resolve_field(context, &mut args, &["workdir"], false, Some("."))?,
        "MultiEdit" => {
            let edits = args
                .get_mut("edits")
                .and_then(Value::as_array_mut)
                .ok_or("edits must be an array")?;
            for edit in edits {
                resolve_field(context, edit, &["file_path"], true, None)?;
            }
        }
        "ApplyPatch" | "Patch" => validate_patch(context, &args)?,
        _ => {}
    }
    Ok(args)
}

fn resolve_field(
    context: &ToolContext,
    args: &mut Value,
    fields: &[&str],
    write: bool,
    default: Option<&str>,
) -> Result<(), String> {
    let field = fields
        .iter()
        .find(|field| args.get(**field).is_some())
        .copied()
        .unwrap_or(fields[0]);
    let path = match args.get(field) {
        Some(value) => value.as_str().ok_or("Path must be a string")?,
        None => default.ok_or("A file path is required")?,
    };
    let resolved = if write {
        context.resolve_write_path(path)?
    } else {
        context.resolve_and_validate_path(path)?
    };
    args[field] = Value::String(resolved.to_str().ok_or("Path is not valid UTF-8")?.into());
    Ok(())
}

fn validate_patch(context: &ToolContext, args: &Value) -> Result<(), String> {
    let patch = args
        .get("patch")
        .and_then(Value::as_str)
        .ok_or("patch is required")?;
    for line in patch.lines() {
        let Some(path) = line
            .strip_prefix("--- ")
            .or_else(|| line.strip_prefix("+++ "))
        else {
            continue;
        };
        // Match the unified-diff parser's path semantics exactly.
        let path = path.trim();
        let path = path
            .strip_prefix("a/")
            .or_else(|| path.strip_prefix("b/"))
            .unwrap_or(path);
        let path = path.split('\t').next().unwrap_or(path).trim();
        if path != "/dev/null" {
            context.resolve_write_path(path)?;
        }
    }
    Ok(())
}

/// Redact outputs and metadata from dispatchers outside the registry (Task/MCP).
pub fn redact_result(context: &ToolContext, mut result: ToolResult) -> ToolResult {
    result.output = context.redact(&result.output);
    result.error = result.error.map(|message| context.redact(&message));
    if let Some(metadata) = &mut result.metadata {
        for path in &mut metadata.files_modified {
            *path = context.redact(path);
        }
        if let Some(data) = &mut metadata.data {
            redact_value(context, data);
        }
    }
    result
}

fn redact_value(context: &ToolContext, value: &mut Value) {
    match value {
        Value::String(text) => *text = context.redact(text),
        Value::Array(values) => values.iter_mut().for_each(|v| redact_value(context, v)),
        Value::Object(values) => {
            // Keys may also contain tool-controlled content.
            let original = std::mem::take(values);
            for (key, mut value) in original {
                if crate::exec::is_sensitive_env_name(&key) {
                    value = Value::String("[REDACTED]".into());
                } else {
                    redact_value(context, &mut value);
                }
                values.insert(context.redact(&key), value);
            }
        }
        _ => {}
    }
}
