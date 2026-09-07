//! Confined file tools. All paths are checked again at the I/O boundary.
use crate::error::{CortexError, Result};
use crate::tools::registry::ToolRegistry;
use crate::tools::{ToolContext, ToolResult};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

pub(super) const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_READ_BYTES: usize = 1024 * 1024;

pub(super) fn input(message: impl Into<String>) -> CortexError {
    CortexError::InvalidInput(message.into())
}
pub(super) fn text<'a>(args: &'a Value, fields: &[&str]) -> Result<&'a str> {
    fields
        .iter()
        .find_map(|field| args.get(field).and_then(Value::as_str))
        .ok_or_else(|| input(format!("{} is required", fields[0])))
}
pub(super) fn number(args: &Value, name: &str, default: usize, maximum: usize) -> Result<usize> {
    let Some(value) = args.get(name) else {
        return Ok(default);
    };
    let n = value
        .as_u64()
        .ok_or_else(|| input(format!("{name} must be a nonnegative integer")))?;
    let n = usize::try_from(n).map_err(|_| input("Number too large"))?;
    if n > maximum {
        return Err(input(format!("{name} exceeds {maximum}")));
    }
    Ok(n)
}
pub(super) async fn bounded_file(path: &std::path::Path) -> Result<Vec<u8>> {
    let file = tokio::fs::File::open(path).await?;
    if !file.metadata().await?.is_file() {
        return Err(input("Path must be a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(input("File exceeds the 16 MiB tool limit"));
    }
    Ok(bytes)
}

impl ToolRegistry {
    pub(crate) async fn execute_read_file(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let path = context
            .resolve_and_validate_path(text(&args, &["file_path", "path"])?)
            .map_err(input)?;
        // The advertised contract is zero-based; an offset of 0 is the first line.
        let offset = number(&args, "offset", 0, 10_000_000)?;
        let limit = number(&args, "limit", 2400, 10_000)?;
        let file = tokio::fs::File::open(&path).await?;
        if !file.metadata().await?.is_file() {
            return Err(input("Read requires a regular file"));
        }
        let mut reader = BufReader::new(file);
        let mut output = Vec::new();
        let mut line = 0usize;
        let mut scanned = 0usize;
        let mut truncated = false;
        while line < offset.saturating_add(limit) {
            let chunk = reader.fill_buf().await?;
            if chunk.is_empty() {
                break;
            }
            let take = chunk
                .iter()
                .position(|b| *b == b'\n')
                .map_or(chunk.len(), |i| i + 1);
            let newline = chunk[take - 1] == b'\n';
            scanned += take;
            if scanned > 64 * 1024 * 1024 {
                return Err(input("Read scan limit exceeded; use a smaller offset"));
            }
            if line >= offset {
                if output.len() + take > MAX_READ_BYTES {
                    truncated = true;
                    break;
                }
                output.extend_from_slice(&chunk[..take]);
            }
            reader.consume(take);
            if newline {
                line += 1;
            }
        }
        let mut output = String::from_utf8(output).map_err(|_| input("File is not UTF-8 text"))?;
        if truncated {
            output.push_str("\n[Output truncated at 1 MiB; request a smaller page]");
        }
        Ok(ToolResult::success(output))
    }

    pub(crate) async fn execute_write_file(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let path = context
            .resolve_write_path(text(&args, &["file_path", "path"])?)
            .map_err(input)?;
        let content = text(&args, &["content"])?;
        if content.len() as u64 > MAX_FILE_BYTES {
            return Err(input("Content exceeds the 16 MiB tool limit"));
        }
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let path = context
            .resolve_write_path(path.to_str().ok_or_else(|| input("Invalid path"))?)
            .map_err(input)?;
        if path.exists() && !path.is_file() {
            return Err(input("Create requires a regular file"));
        }
        tokio::fs::write(&path, content).await?;
        Ok(ToolResult::success(format!(
            "Wrote {} bytes to {}",
            content.len(),
            path.display()
        )))
    }

    pub(crate) async fn execute_list_dir(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let path = context
            .resolve_and_validate_path(text(&args, &["directory_path", "path"])?)
            .map_err(input)?;
        let ignores = super::search::patterns(args.get("ignorePatterns"))?;
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(path).await?;
        while let Some(entry) = dir.next_entry().await? {
            if entries.len() >= 10_000 {
                return Err(input(
                    "Directory exceeds the 10000 entry limit; use a targeted search",
                ));
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if ignores.iter().any(|p| p.matches(&name)) {
                continue;
            }
            let kind = entry.file_type().await?;
            let suffix = if kind.is_symlink() {
                "@"
            } else if kind.is_dir() {
                "/"
            } else {
                ""
            };
            entries.push(format!("{name}{suffix}"));
        }
        entries.sort();
        Ok(ToolResult::success(entries.join("\n")))
    }

    pub(crate) async fn execute_edit_file(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let path = context
            .resolve_write_path(text(&args, &["file_path", "path"])?)
            .map_err(input)?;
        let old = text(&args, &["old_str"])?;
        let new = text(&args, &["new_str"])?;
        if old.is_empty() {
            return Err(input("old_str must not be empty"));
        }
        let content = String::from_utf8(bounded_file(&path).await?)
            .map_err(|_| input("File is not UTF-8 text"))?;
        let count = content.matches(old).count();
        let all = args
            .get("change_all")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if count == 0 {
            return Err(input("Could not find the specified text"));
        }
        if !all && count > 1 {
            return Err(input(
                "Text is ambiguous; provide more context or change_all=true",
            ));
        }
        let edited = if all {
            content.replace(old, new)
        } else {
            content.replacen(old, new, 1)
        };
        if edited.len() as u64 > MAX_FILE_BYTES {
            return Err(input("Edited file exceeds the tool size limit"));
        }
        let path = context
            .resolve_write_path(path.to_str().ok_or_else(|| input("Invalid path"))?)
            .map_err(input)?;
        tokio::fs::write(&path, edited).await?;
        Ok(ToolResult::success(format!(
            "Successfully edited {}",
            path.display()
        )))
    }
}
