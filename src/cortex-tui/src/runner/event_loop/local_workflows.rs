//! Read-only workspace review and safe, non-overwriting local export.
use anyhow::{Context, Result, bail};
use std::path::{Component, Path, PathBuf};
use tokio::process::Command;

use super::core::EventLoop;

impl EventLoop {
    pub(super) async fn show_local_diff(&mut self, command: &str) {
        let cwd = self
            .cortex_session
            .as_ref()
            .map(|s| PathBuf::from(&s.meta.cwd))
            .or_else(|| std::env::current_dir().ok());
        let result = match cwd {
            Some(cwd) => read_local_diff(&cwd, command).await,
            None => Err(anyhow::anyhow!("Workspace directory is unavailable")),
        };
        match result {
            Ok(diff) => self.add_system_message(&diff),
            Err(error) => self.add_system_message(&format!("Review unavailable: {error}")),
        }
    }
}

pub(super) async fn read_local_diff(cwd: &Path, command: &str) -> Result<String> {
    let mut args = vec!["diff", "--no-ext-diff", "--no-textconv", "--no-color"];
    let mut revision = None;
    let mut file = None;
    if let Some(path) = command.strip_prefix("diff:") {
        if Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
            || path.starts_with('-')
        {
            bail!("Diff paths must be relative to this workspace");
        }
        file = Some(path);
    } else if let Some(target) = command.strip_prefix("review:") {
        if let Some((_, base)) = target.split_once(":base=") {
            if base.is_empty() || base.starts_with('-') || base.contains(':') {
                bail!("Invalid review base");
            }
            revision = Some(base);
        } else if target != "uncommitted" {
            bail!("Local review supports uncommitted changes or --base=REVISION");
        }
    }
    // HEAD includes staged and unstaged changes; it never invokes an editor,
    // external diff driver, textconv command, or repository hook.
    args.push(revision.unwrap_or("HEAD"));
    args.push("--");
    if let Some(path) = file {
        args.push(path);
    }
    let text = read_git_output(cwd, &args).await?;
    let mut result = String::from(
        "Local change review (read-only; no model review or file restore performed)\n",
    );
    if text.is_empty() {
        result.push_str("No tracked changes. Untracked files are not included.");
    } else {
        result.push_str(&text.replace('\x1b', ""));
    }
    Ok(result)
}

pub(super) fn write_export(path: &Path, content: &str) -> Result<()> {
    use std::io::Write;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .context("Cannot create export; existing files are never overwritten")?;
    output.write_all(content.as_bytes())?;
    output.sync_all()?;
    Ok(())
}

async fn read_git_output(cwd: &Path, args: &[&str]) -> Result<String> {
    use std::process::Stdio;
    use tokio::io::AsyncReadExt;
    const LIMIT: u64 = 2 * 1024 * 1024;
    let mut child = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .env("GIT_PAGER", "cat")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("Cannot start local diff")?;
    let mut stdout = child
        .stdout
        .take()
        .context("Cannot read local diff")?
        .take(LIMIT + 1);
    let mut bytes = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        stdout.read_to_end(&mut bytes).await?;
        if bytes.len() as u64 > LIMIT {
            bail!("Diff exceeds 2 MiB; use /diff PATH to narrow the review");
        }
        if !child.wait().await?.success() {
            bail!("Cannot read this revision in the current Git workspace");
        }
        Ok(())
    })
    .await
    .context("Local diff timed out")??;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
