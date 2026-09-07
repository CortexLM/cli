//! Bounded, in-process searches: no unchecked subprocess exit or symlink following.
use super::file_ops::{bounded_file, input, number, text};
use crate::error::Result;
use crate::tools::registry::ToolRegistry;
use crate::tools::{ToolContext, ToolResult};
use glob::Pattern;
use regex::RegexBuilder;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(super) fn patterns(value: Option<&Value>) -> Result<Vec<Pattern>> {
    let values: Vec<&str> = match value {
        None => Vec::new(),
        Some(Value::String(s)) => vec![s],
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().ok_or_else(|| input("Patterns must be strings")))
            .collect::<Result<_>>()?,
        _ => return Err(input("Patterns must be strings or an array")),
    };
    values
        .into_iter()
        .map(|s| Pattern::new(s).map_err(|e| input(format!("Invalid glob pattern: {e}"))))
        .collect()
}
fn search_regex(args: &Value) -> Result<regex::Regex> {
    let pattern = text(args, &["pattern"])?;
    let pattern = if args
        .get("fixed_string")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        regex::escape(pattern)
    } else {
        pattern.into()
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(
            args.get("case_insensitive")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )
        .size_limit(1024 * 1024)
        .build()
        .map_err(|e| input(format!("Invalid search pattern: {e}")))
}

fn matches(patterns: &[Pattern], relative: &Path) -> bool {
    patterns.iter().any(|p| {
        let options = glob::MatchOptions {
            require_literal_separator: true,
            ..Default::default()
        };
        p.matches_path_with(relative, options)
            || (!p.as_str().contains('/')
                && relative
                    .file_name()
                    .is_some_and(|n| p.matches(&n.to_string_lossy())))
    })
}
fn files(root: &Path, context: &ToolContext) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for (count, entry) in walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(100)
        .into_iter()
        .enumerate()
    {
        if count >= 50_000 {
            return Err(input(
                "Search exceeds 50000 entries; choose a narrower directory",
            ));
        }
        let entry = entry.map_err(|_| input("Search could not access a directory entry"))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = context
            .resolve_and_validate_path(entry.path().to_str().ok_or_else(|| input("Invalid path"))?)
            .map_err(input)?;
        files.push(path);
    }
    files.sort();
    Ok(files)
}
fn push_line(lines: &mut Vec<String>, bytes: &mut usize, line: String) -> Result<()> {
    *bytes += line.len() + 1;
    if *bytes > 1024 * 1024 {
        return Err(input("Search output exceeds 1 MiB; narrow the search"));
    }
    lines.push(line);
    Ok(())
}

fn finish(lines: Vec<String>) -> Result<ToolResult> {
    let output = lines.join("\n");
    if output.len() > 1024 * 1024 {
        return Err(input("Search output exceeds 1 MiB; narrow the search"));
    }
    Ok(ToolResult::success(if output.is_empty() {
        "No matches found".into()
    } else {
        output
    }))
}

impl ToolRegistry {
    pub(crate) async fn execute_glob(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let root = context
            .resolve_and_validate_path(text(&args, &["folder", "path"])?)
            .map_err(input)?;
        let include = patterns(args.get("patterns"))?;
        let exclude = patterns(
            args.get("exclude_patterns")
                .or_else(|| args.get("excludePatterns")),
        )?;
        if include.is_empty() {
            return Err(input("At least one glob pattern is required"));
        }
        let mut lines = Vec::new();
        let mut output_bytes = 0;
        for path in files(&root, context)? {
            let relative = path.strip_prefix(&root).unwrap_or(&path);
            if matches(&include, relative) && !matches(&exclude, relative) {
                push_line(&mut lines, &mut output_bytes, path.display().to_string())?;
            }
            if lines.len() > 10_000 {
                return Err(input("Too many matches; narrow the search"));
            }
        }
        finish(lines)
    }

    pub(crate) async fn execute_search_files(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let pattern = text(&args, &["pattern"])?;
        let path = text(&args, &["path"])?;
        if let Some(content) = args.get("content_pattern").and_then(Value::as_str) {
            return self
                .execute_grep(
                    serde_json::json!({"path": path, "pattern": content, "glob_pattern": pattern}),
                    context,
                )
                .await;
        }
        self.execute_glob(
            serde_json::json!({"folder": path, "patterns": [pattern]}),
            context,
        )
        .await
    }

    pub(crate) async fn execute_grep(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let regex = search_regex(&args)?;
        let root = context
            .resolve_and_validate_path(text(&args, &["path"])?)
            .map_err(input)?;
        let include = patterns(args.get("glob_pattern"))?;
        let mode = args
            .get("output_mode")
            .and_then(Value::as_str)
            .unwrap_or("file_paths");
        if !matches!(mode, "file_paths" | "content") {
            return Err(input("Invalid output_mode"));
        }
        let limit = number(&args, "head_limit", 1000, 10_000)?;
        let around = number(&args, "context", 0, 100)?;
        let before = number(&args, "context_before", around, 100)?;
        let after = number(&args, "context_after", around, 100)?;
        let numbers = args
            .get("line_numbers")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut output = Vec::new();
        let mut output_bytes = 0;
        for path in files(&root, context)? {
            if output.len() >= limit {
                break;
            }
            if !include.is_empty() && !matches(&include, path.strip_prefix(&root).unwrap_or(&path))
            {
                continue;
            }
            let bytes = bounded_file(&path).await?;
            let Ok(content) = std::str::from_utf8(&bytes) else {
                continue;
            };
            if content.contains('\0') {
                continue;
            }
            let lines: Vec<&str> = content.lines().collect();
            let mut selected = BTreeSet::new();
            for (i, _) in lines
                .iter()
                .enumerate()
                .filter(|(_, line)| regex.is_match(line))
            {
                if mode == "file_paths" {
                    selected.insert(i);
                    break;
                }
                selected.extend(i.saturating_sub(before)..=(i + after).min(lines.len() - 1));
            }
            if mode == "file_paths" {
                if !selected.is_empty() {
                    push_line(&mut output, &mut output_bytes, path.display().to_string())?;
                }
            } else {
                for i in selected.into_iter().take(limit - output.len()) {
                    let number = if numbers {
                        format!("{}:", i + 1)
                    } else {
                        String::new()
                    };
                    push_line(
                        &mut output,
                        &mut output_bytes,
                        format!("{}:{number}{}", path.display(), lines[i]),
                    )?;
                }
            }
        }
        finish(output)
    }
}
