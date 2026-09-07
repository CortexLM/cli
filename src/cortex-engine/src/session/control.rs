//! Shared cancellation/deadline ownership for unattended runtimes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cortex_protocol::{Event, Op, Submission};

use super::SessionHandle;
use crate::error::{CortexError, Result};

/// Resolve the effective root before project discovery; malformed configuration
/// is an error, never a reason to silently run with defaults.
pub async fn load_runtime_config(
    cwd: Option<std::path::PathBuf>,
    model: Option<String>,
    instructions: Option<String>,
) -> Result<crate::config::Config> {
    let cwd = cwd.unwrap_or(std::env::current_dir()?);
    let cwd = std::fs::canonicalize(cwd)?;
    if !cwd.is_dir() {
        return Err(CortexError::InvalidInput(
            "The working directory must be a directory.".into(),
        ));
    }
    use crate::config::{
        Config, ConfigOverrides, find_cortex_home, find_project_config, load_config,
        load_project_config, merge_configs,
    };
    let home = find_cortex_home()?;
    let global = load_config(&home).await.map_err(|_| {
        CortexError::InvalidInput(
            "The global configuration is unreadable or invalid. Fix it before running a turn."
                .into(),
        )
    })?;
    // ponytail: use the existing parsers/merge, but not the permissive loader;
    // switch back when Config::load propagates project parse failures.
    let project = find_project_config(&cwd)
        .map(|path| load_project_config(&path))
        .transpose()
        .map_err(|_| {
            CortexError::InvalidInput(
                "The project configuration is unreadable or invalid. Fix it before running a turn."
                    .into(),
            )
        })?;
    let mut merged = merge_configs(global, project);
    merged.model.get_or_insert_with(|| Config::default().model);
    let mut config = Config::from_toml(
        merged,
        ConfigOverrides {
            cwd: Some(cwd),
            model,
            ..Default::default()
        },
        home,
    );
    if config
        .reasoning_effort
        .as_ref()
        .is_some_and(|effort| !matches!(effort, crate::config::ReasoningEffort::Medium))
    {
        return Err(CortexError::InvalidInput(
            "The Code service pins reasoning effort to medium; this configuration cannot be applied."
                .into(),
        ));
    }
    if let Some(instructions) = instructions {
        config.user_instructions = Some(match config.user_instructions {
            Some(existing) => format!("{existing}\n\n{instructions}"),
            None => instructions,
        });
    }
    Ok(config)
}

pub async fn wait_for_cancellation(cancelled: &AtomicBool) {
    while !cancelled.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// A deadline is independent of the event source. A closed channel is never a
/// successful task terminal.
pub async fn next_event(
    handle: &SessionHandle,
    deadline: Option<tokio::time::Instant>,
) -> Result<Event> {
    tokio::select! {
        biased;
        _ = async {
            match deadline {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending().await,
            }
        } => Err(CortexError::Timeout),
        _ = wait_for_cancellation(&handle.cancelled) => Err(CortexError::Cancelled),
        event = handle.event_rx.recv() => event.map_err(|_| CortexError::BackendError {
            message: crate::client::runtime_contract::INCOMPLETE_STREAM.into(),
        }),
    }
}

/// Signal the owner before awaiting it. Never leave an unattended approval or
/// detached session waiting after a CLI result has been returned.
pub async fn stop_session(
    handle: &SessionHandle,
    mut task: tokio::task::JoinHandle<Result<()>>,
) -> Result<()> {
    handle
        .submission_tx
        .send(Submission {
            id: uuid::Uuid::new_v4().to_string(),
            op: Op::Shutdown,
        })
        .await
        .ok();
    match tokio::time::timeout(Duration::from_secs(5), &mut task).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(CortexError::InvalidInput(
            "The session task stopped unexpectedly.".into(),
        )),
        Err(_) => {
            task.abort();
            let _ = task.await;
            Err(CortexError::BackendError {
                message: crate::client::runtime_contract::REMOTE_CANCEL_UNCONFIRMED.into(),
            })
        }
    }
}
