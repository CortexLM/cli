//! Cortex CLI - Main entry point.
//!
//! This is the main entry point for the Cortex CLI, providing:
//! - Interactive TUI mode (default)
//! - Non-interactive exec mode
//! - Session management (resume, list)
//! - Login/logout authentication
//! - MCP server management
//! - Debug sandbox commands
//! - Shell completions
//!
//! # Architecture
//!
//! The CLI is structured as follows:
//! - `cli/` - Command-line argument parsing and dispatch
//! - `utils/` - Shared utilities for all commands
//! - `*_cmd.rs` - Individual command implementations

use anyhow::Result;
use clap::Parser;

use cortex_cli::cli::{Cli, ColorMode, Commands, LogLevel, dispatch_command};

/// Apply process hardening measures early in startup.
#[cfg(not(debug_assertions))]
#[ctor::ctor]
fn pre_main_hardening() {
    cortex_process_hardening::pre_main_hardening();
}

/// Check if CORTEX_HOME is writable.
fn check_cortex_home_writable() -> Result<()> {
    if let Some(home) = std::env::var_os("CORTEX_HOME") {
        cortex_cli::startup::check_cortex_home_writable(std::path::Path::new(&home))?;
    }
    Ok(())
}

/// Check for updates in the background.
async fn check_for_updates_background() {
    // Use cortex_update crate for update checking
    // This runs asynchronously and doesn't block the main command
    if let Ok(manager) = cortex_update::UpdateManager::new()
        && let Ok(Some(update_info)) = manager.check_update().await
    {
        eprintln!(
            "\n\x1b[1;33mUpdate available:\x1b[0m {} -> {}\n\
                 Run 'cortex upgrade' to update.\n",
            update_info.current_version, update_info.latest_version
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    cortex_cli::startup::run_as_sandbox_wrapper_if_requested();

    // Install Ctrl+C handler to restore terminal state before exiting
    cortex_cli::install_cleanup_handler();

    // Install panic hook that suggests RUST_BACKTRACE for debugging
    cortex_cli::install_panic_hook();

    let cli = Cli::parse();
    cortex_cli::startup::initialize_diagnostics(cli.interactive.debug)?;

    // Handle color mode
    // SAFETY: Environment variable mutations happen early before threads spawn
    match cli.color {
        ColorMode::Never => unsafe { std::env::set_var("NO_COLOR", "1") },
        ColorMode::Always => unsafe { std::env::remove_var("NO_COLOR") },
        ColorMode::Auto => {}
    }

    // Early check for CORTEX_HOME writability
    let is_debug_cmd = matches!(&cli.command, Some(Commands::Debug(_)));
    if !is_debug_cmd {
        check_cortex_home_writable()?;
    }

    // Initialize logging for non-TUI commands (when not in debug mode)
    if cli.command.is_some() && !cli.interactive.debug {
        let log_level = if cli.trace {
            LogLevel::Trace
        } else if cli.verbose {
            LogLevel::Debug
        } else if let Ok(env_level) = std::env::var("CORTEX_LOG_LEVEL") {
            LogLevel::from_str_loose(&env_level).unwrap_or(cli.interactive.log_level)
        } else {
            cli.interactive.log_level
        };

        let filter_str = if std::env::var("RUST_LOG").is_ok() {
            format!(
                "error,cortex={},cortex_cli={},cortex_engine={},cortex_common={}",
                log_level.as_filter_str(),
                log_level.as_filter_str(),
                log_level.as_filter_str(),
                log_level.as_filter_str()
            )
        } else {
            log_level.as_filter_str().to_string()
        };

        tracing_subscriber::fmt()
            .with_env_filter(&filter_str)
            .with_writer(std::io::stderr)
            .init();
    }

    // Background update check (non-blocking)
    let skip_auto_update = cli.interactive.debug
        || matches!(
            &cli.command,
            Some(Commands::Upgrade(_) | Commands::Serve(_) | Commands::McpServer(_))
        );
    let is_tui_mode = cli.command.is_none();
    if !skip_auto_update && !is_tui_mode && !is_debug_cmd {
        tokio::spawn(async {
            check_for_updates_background().await;
        });
    }

    let operation = if is_tui_mode {
        cortex_common::diagnostics::Operation::CliInteractive
    } else if is_debug_cmd {
        cortex_common::diagnostics::Operation::CliDebug
    } else {
        cortex_common::diagnostics::Operation::CliCommand
    };
    let trace = cortex_common::diagnostics::TraceContext::default();
    let start = std::time::Instant::now();
    let result = trace.clone().scope(dispatch_command(cli)).await;
    if cortex_common::diagnostics::record(
        operation,
        &trace,
        if result.is_ok() { 200 } else { 500 },
        start.elapsed(),
    )
    .is_err()
    {
        eprintln!("Local diagnostics could not be written");
    }
    result
}
