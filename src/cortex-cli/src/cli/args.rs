//! CLI argument structures and parsing.
//!
//! Defines all command-line argument structures using clap.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use super::styles::{AFTER_HELP, BEFORE_HELP, HELP_TEMPLATE, categories, get_styles};
use crate::acp_cmd::AcpCli;
use crate::agent_cmd::AgentCli;
use crate::alias_cmd::AliasCli;
use crate::cache_cmd::CacheCli;
use crate::compact_cmd::CompactCli;
use crate::dag_cmd::DagCli;
use crate::debug_cmd::DebugCli;
use crate::exec_cmd::ExecCli;
use crate::export_cmd::ExportCommand;
use crate::feedback_cmd::FeedbackCli;
use crate::github_cmd::GitHubCli;
use crate::import_cmd::ImportCommand;
use crate::lock_cmd::LockCli;
use crate::logs_cmd::LogsCli;
use crate::mcp_cmd::McpCli;
use crate::models_cmd::ModelsCli;
use crate::plugin_cmd::PluginCli;
use crate::pr_cmd::PrCli;
use crate::run_cmd::RunCli;
use crate::scrape_cmd::ScrapeCommand;
use crate::shell_cmd::ShellCli;
use crate::stats_cmd::StatsCli;
use crate::uninstall_cmd::UninstallCli;
use crate::upgrade_cmd::UpgradeCli;
use crate::workspace_cmd::WorkspaceCli;
use crate::{LandlockCommand, SeatbeltCommand, WindowsCommand};
use cortex_common::CliConfigOverrides;

/// Build-time version string with commit hash and build date.
pub fn get_long_version() -> &'static str {
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    const GIT_HASH: &str = match option_env!("CORTEX_GIT_HASH") {
        Some(v) => v,
        None => "unknown",
    };
    const BUILD_DATE: &str = match option_env!("CORTEX_BUILD_DATE") {
        Some(v) => v,
        None => "unknown",
    };

    static LONG_VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    LONG_VERSION.get_or_init(|| format!("{} ({} {})", VERSION, GIT_HASH, BUILD_DATE))
}

/// Log verbosity level for CLI output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum LogLevel {
    /// Only show errors
    Error,
    /// Show warnings and errors
    Warn,
    /// Show informational messages, warnings, and errors (default)
    #[default]
    Info,
    /// Show debug messages and above
    Debug,
    /// Show all messages including trace-level details
    Trace,
}

impl LogLevel {
    /// Convert to tracing filter string.
    pub fn as_filter_str(&self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }

    /// Parse from string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<LogLevel> {
        match s.to_lowercase().as_str() {
            "error" => Some(LogLevel::Error),
            "warn" | "warning" => Some(LogLevel::Warn),
            "info" => Some(LogLevel::Info),
            "debug" => Some(LogLevel::Debug),
            "trace" => Some(LogLevel::Trace),
            _ => None,
        }
    }
}

/// Color output mode for CLI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ColorMode {
    /// Automatically detect if output is a terminal
    #[default]
    Auto,
    /// Always output with colors
    Always,
    /// Never output with colors
    Never,
}

/// Cortex CLI - AI Coding Agent
///
/// If no subcommand is specified, starts the interactive TUI.
#[derive(Parser)]
#[command(name = "cortex")]
#[command(author, version, long_version = get_long_version())]
#[command(about = "Cortex - AI Coding Agent", long_about = None)]
#[command(
    styles = get_styles(),
    subcommand_negates_reqs = true,
    override_usage = "cortex [OPTIONS] [PROMPT]\n       cortex [OPTIONS] <COMMAND> [ARGS]",
    before_help = BEFORE_HELP,
    after_help = AFTER_HELP,
    help_template = HELP_TEMPLATE
)]
pub struct Cli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    /// Enable verbose output (same as --log-level debug)
    #[arg(long = "verbose", short = 'v', global = true)]
    pub verbose: bool,

    /// Enable trace-level logging for debugging
    #[arg(long = "trace", global = true)]
    pub trace: bool,

    /// Control color output: auto (default), always, or never
    #[arg(long = "color", global = true, value_enum, default_value_t = ColorMode::Auto)]
    pub color: ColorMode,

    #[clap(flatten)]
    pub interactive: InteractiveArgs,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Arguments for interactive mode.
#[derive(Args, Debug, Default)]
pub struct InteractiveArgs {
    /// Model to use (e.g., claude-sonnet-4-20250514, gpt-4o, gemini-2.0-flash)
    #[arg(short, long, help_heading = "Model Configuration")]
    pub model: Option<String>,

    /// Use open-source/local LLM providers instead of cloud APIs.
    #[arg(
        long = "oss",
        default_value_t = false,
        help_heading = "Model Configuration"
    )]
    pub oss: bool,

    /// Configuration profile from config.toml
    #[arg(long = "profile", short = 'p', help_heading = "Model Configuration")]
    pub config_profile: Option<String>,

    /// Select the sandbox policy for shell commands
    #[arg(long = "sandbox", short = 's', help_heading = "Security")]
    pub sandbox_mode: Option<String>,

    /// Set the approval policy for tool executions.
    #[arg(
        long = "ask-for-approval",
        short = 'a',
        value_name = "POLICY",
        help_heading = "Security"
    )]
    pub approval_policy: Option<String>,

    /// Enable fully automatic mode with sandboxed execution.
    #[arg(long = "full-auto", default_value_t = false, help_heading = "Security")]
    pub full_auto: bool,

    /// Skip all confirmation prompts and execute commands without sandboxing. DANGEROUS!
    #[arg(
        long = "dangerously-bypass-approvals-and-sandbox",
        alias = "yolo",
        default_value_t = false,
        conflicts_with_all = ["approval_policy", "full_auto"],
        help_heading = "Security"
    )]
    pub dangerously_bypass_approvals_and_sandbox: bool,

    /// Tell the agent to use the specified directory as its working root
    #[arg(
        long = "cd",
        short = 'C',
        value_name = "DIR",
        help_heading = "Workspace"
    )]
    pub cwd: Option<PathBuf>,

    /// Additional directories that should be writable
    #[arg(long = "add-dir", value_name = "DIR", help_heading = "Workspace")]
    pub add_dir: Vec<PathBuf>,

    /// Isolated git worktree for this session (`--worktree` or `--worktree DIR`).
    #[arg(
        long = "worktree",
        value_name = "DIR",
        num_args = 0..=1,
        default_missing_value = "auto",
        help_heading = "Workspace"
    )]
    pub worktree: Option<PathBuf>,

    /// Extra plugin folder (folder-of-plugins or a single plugin). Repeatable.
    #[arg(
        long = "plugin-dir",
        value_name = "DIR",
        action = clap::ArgAction::Append,
        help_heading = "Features"
    )]
    pub plugin_dir: Vec<PathBuf>,

    /// Attach unified diffs of files Execute/Bash changed (also `CORTEX_BASH_EDIT_DIFF=1`).
    #[arg(
        long = "bash-edit-diff",
        default_value_t = false,
        help_heading = "Features"
    )]
    pub bash_edit_diff: bool,

    /// Image files to attach to the initial prompt
    #[arg(long = "image", short = 'i', value_delimiter = ',', num_args = 1.., help_heading = "Workspace")]
    pub images: Vec<PathBuf>,

    /// Enable web search capability for the agent.
    #[arg(long = "search", default_value_t = false, help_heading = "Features")]
    pub web_search: bool,

    /// Enter the alternate screen buffer. Default is **always** (full
    /// viewport). Same as `[tui] alternate_screen = true`. Use
    /// `--no-alternate-screen` to stay inline.
    #[arg(
        long = "alternate-screen",
        default_value_t = false,
        conflicts_with = "no_alternate_screen",
        help_heading = "Features"
    )]
    pub alternate_screen: bool,

    /// Stay inline in the host terminal (never alternate screen). Same as
    /// `[tui] alternate_screen = false`.
    #[arg(
        long = "no-alternate-screen",
        default_value_t = false,
        conflicts_with = "alternate_screen",
        help_heading = "Features"
    )]
    pub no_alternate_screen: bool,

    /// Maximum number of concurrent agent threads
    #[arg(
        long = "max-agent-threads",
        value_name = "N",
        help_heading = "Execution"
    )]
    pub max_agent_threads: Option<usize>,

    /// Maximum number of concurrent tool executions
    #[arg(
        long = "max-tool-threads",
        value_name = "N",
        help_heading = "Execution"
    )]
    pub max_tool_threads: Option<usize>,

    /// Timeout for shell commands in seconds
    #[arg(
        long = "command-timeout",
        value_name = "SECONDS",
        help_heading = "Execution"
    )]
    pub command_timeout: Option<u64>,

    /// Timeout for HTTP requests in seconds
    #[arg(
        long = "http-timeout",
        value_name = "SECONDS",
        help_heading = "Execution"
    )]
    pub http_timeout: Option<u64>,

    /// Disable streaming responses
    #[arg(
        long = "no-streaming",
        default_value_t = false,
        help_heading = "Execution"
    )]
    pub no_streaming: bool,

    /// Set log verbosity level (error, warn, info, debug, trace)
    #[arg(
        long = "log-level",
        short = 'L',
        value_enum,
        default_value = "info",
        help_heading = "Debugging"
    )]
    pub log_level: LogLevel,

    /// Record private, content-free local diagnostics (never prompts or tool output)
    #[arg(long = "debug", help_heading = "Debugging")]
    pub debug: bool,

    /// Initial prompt (if no subcommand).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub prompt: Vec<String>,
}

/// CLI subcommands.
#[derive(Subcommand)]
pub enum Commands {
    // ========================================================================
    // 🚀 Execution (order 1-9)
    // ========================================================================
    /// Run Cortex non-interactively with advanced options
    #[command(visible_alias = "r", display_order = 1)]
    #[command(next_help_heading = categories::EXECUTION)]
    Run(RunCli),

    /// Execute in headless mode (for CI/CD, scripts, automation)
    #[command(visible_alias = "e", display_order = 2)]
    #[command(next_help_heading = categories::EXECUTION)]
    Exec(ExecCli),

    // ========================================================================
    // 📋 Session Management (order 10-19)
    // ========================================================================
    /// Resume a previous interactive session
    #[command(display_order = 10)]
    #[command(next_help_heading = categories::SESSION)]
    Resume(ResumeCommand),

    /// List previous sessions
    #[command(display_order = 11)]
    #[command(next_help_heading = categories::SESSION)]
    Sessions(SessionsCommand),

    /// Export a session to JSON format
    #[command(display_order = 12)]
    #[command(next_help_heading = categories::SESSION)]
    Export(ExportCommand),

    /// Import a session from JSON file or URL
    #[command(display_order = 13)]
    #[command(next_help_heading = categories::SESSION)]
    Import(ImportCommand),

    /// Delete a session
    #[command(display_order = 14)]
    #[command(next_help_heading = categories::SESSION)]
    Delete(DeleteCommand),

    /// Attach this terminal to a live Code session (session keeps running on detach)
    #[command(display_order = 15)]
    #[command(next_help_heading = categories::SESSION)]
    Attach(crate::attach_cmd::AttachCli),

    /// List, follow, or stop background Code agents
    #[command(display_order = 16)]
    #[command(next_help_heading = categories::SESSION)]
    Jobs(crate::jobs_cmd::JobsCli),

    // ========================================================================
    // 🔐 Authentication (order 20-29)
    // ========================================================================
    /// Authenticate with Cortex API
    #[command(display_order = 20)]
    #[command(next_help_heading = categories::AUTH)]
    Login(LoginCommand),

    /// Remove stored authentication credentials
    #[command(display_order = 21)]
    #[command(next_help_heading = categories::AUTH)]
    Logout(LogoutCommand),

    /// Show currently authenticated user
    #[command(display_order = 22)]
    #[command(next_help_heading = categories::AUTH)]
    Whoami,

    // ========================================================================
    // 🔌 Extensibility (order 30-39)
    // ========================================================================
    /// Manage agents (list, create, show)
    #[command(display_order = 30)]
    #[command(next_help_heading = categories::EXTENSION)]
    Agent(AgentCli),

    /// Manage MCP (Model Context Protocol) servers
    #[command(display_order = 31)]
    #[command(next_help_heading = categories::EXTENSION)]
    Mcp(McpCli),

    /// Run the MCP server (stdio transport)
    #[command(display_order = 32, hide = true)]
    #[command(next_help_heading = categories::EXTENSION)]
    McpServer(super::mcp_server::McpServerCli),

    /// Start ACP server for IDE integration (e.g., Zed)
    #[command(display_order = 33)]
    #[command(next_help_heading = categories::EXTENSION)]
    Acp(AcpCli),

    // ========================================================================
    // ⚙️ Configuration (order 40-49)
    // ========================================================================
    /// Show or edit configuration
    #[command(display_order = 40)]
    #[command(next_help_heading = categories::CONFIG)]
    Config(ConfigCommand),

    /// List available models
    #[command(display_order = 41)]
    #[command(next_help_heading = categories::CONFIG)]
    Models(ModelsCli),

    /// Inspect feature flags
    #[command(display_order = 42)]
    #[command(next_help_heading = categories::CONFIG)]
    Features(FeaturesCommand),

    /// Initialize AGENTS.md in the current directory
    #[command(display_order = 43)]
    #[command(next_help_heading = categories::CONFIG)]
    Init(InitCommand),

    // ========================================================================
    // 🛠️ Utilities (order 50-59)
    // ========================================================================
    /// GitHub integration (actions, workflows)
    #[command(visible_alias = "gh", display_order = 50)]
    #[command(next_help_heading = categories::UTILITIES)]
    Github(GitHubCli),

    /// Checkout a pull request
    #[command(display_order = 51)]
    #[command(next_help_heading = categories::UTILITIES)]
    Pr(PrCli),

    /// Scrape web content to markdown/text/html
    #[command(display_order = 52)]
    #[command(next_help_heading = categories::UTILITIES)]
    Scrape(ScrapeCommand),

    /// Show usage statistics
    #[command(display_order = 53)]
    #[command(next_help_heading = categories::UTILITIES)]
    Stats(StatsCli),

    /// Generate shell completion scripts
    #[command(display_order = 54)]
    #[command(next_help_heading = categories::UTILITIES)]
    Completion(CompletionCommand),

    // ========================================================================
    // 🔧 Maintenance (order 60-69)
    // ========================================================================
    /// Check for and install updates
    #[command(display_order = 60)]
    #[command(next_help_heading = categories::MAINTENANCE)]
    Upgrade(UpgradeCli),

    /// Uninstall Cortex CLI
    #[command(display_order = 61)]
    #[command(next_help_heading = categories::MAINTENANCE)]
    Uninstall(UninstallCli),

    /// Data compaction and cleanup (logs, sessions, history)
    #[command(visible_aliases = ["gc", "cleanup"], display_order = 62)]
    #[command(next_help_heading = categories::MAINTENANCE)]
    Compact(CompactCli),

    /// Manage cache
    #[command(display_order = 63)]
    #[command(next_help_heading = categories::MAINTENANCE)]
    Cache(CacheCli),

    /// View application logs
    #[command(display_order = 64)]
    #[command(next_help_heading = categories::MAINTENANCE)]
    Logs(LogsCli),

    /// Submit feedback and bug reports
    #[command(visible_alias = "report", display_order = 65)]
    #[command(next_help_heading = categories::MAINTENANCE)]
    Feedback(FeedbackCli),

    /// Lock/protect sessions from deletion
    #[command(visible_alias = "protect", display_order = 66)]
    #[command(next_help_heading = categories::MAINTENANCE)]
    Lock(LockCli),

    /// Manage command aliases
    #[command(visible_alias = "aliases", display_order = 67)]
    #[command(next_help_heading = categories::MAINTENANCE)]
    Alias(AliasCli),

    /// Manage plugins
    #[command(visible_alias = "plugins", display_order = 68)]
    #[command(next_help_heading = categories::MAINTENANCE)]
    Plugin(PluginCli),

    // ========================================================================
    // Hidden commands (internal/debug/advanced)
    // ========================================================================
    /// Debug and diagnostic commands
    #[command(display_order = 99, hide = true)]
    Debug(DebugCli),

    /// Start interactive shell/REPL mode
    #[command(visible_aliases = ["interactive", "repl"], hide = true)]
    Shell(ShellCli),

    /// Execute and manage task DAGs (dependency graphs)
    #[command(visible_alias = "tasks", hide = true)]
    Dag(DagCli),

    /// Discover Cortex servers on the local network
    #[command(hide = true)]
    Servers(ServersCommand),

    /// View prompt history from past sessions
    #[command(hide = true)]
    History(HistoryCommand),

    /// Manage workspace/project settings
    #[command(visible_alias = "project", hide = true)]
    Workspace(WorkspaceCli),

    /// Run commands within a Cortex-provided sandbox
    #[command(visible_alias = "sb", hide = true)]
    Sandbox(SandboxArgs),

    /// Run the HTTP API server (for desktop/web integration)
    #[command(hide = true)]
    Serve(ServeCommand),
}

// ============================================================================
// Subcommand argument structures
// ============================================================================

/// Login command.
#[derive(Args)]
pub struct LoginCommand {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// Read the API key from stdin
    #[arg(long = "with-api-key")]
    pub with_api_key: bool,

    /// Provide API token directly (for CI/CD automation).
    #[arg(long = "token", value_name = "TOKEN", conflicts_with = "with_api_key")]
    pub token: Option<String>,

    /// Use device code authentication flow
    #[arg(long = "device-auth")]
    pub use_device_code: bool,

    /// Use enterprise SSO authentication.
    #[arg(long = "sso")]
    pub use_sso: bool,

    /// Override the OAuth issuer base URL (advanced)
    #[arg(long = "experimental_issuer", value_name = "URL", hide = true)]
    pub issuer_base_url: Option<String>,

    /// Override the OAuth client ID (advanced)
    #[arg(long = "experimental_client-id", value_name = "CLIENT_ID", hide = true)]
    pub client_id: Option<String>,

    #[command(subcommand)]
    pub action: Option<LoginSubcommand>,
}

/// Login subcommands.
#[derive(Subcommand)]
pub enum LoginSubcommand {
    /// Show login status
    Status,
}

/// Logout command.
#[derive(Args)]
pub struct LogoutCommand {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// Skip confirmation prompt and log out immediately.
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,

    /// Log out from all logged in accounts.
    #[arg(long = "all")]
    pub all: bool,
}

/// Completion command.
#[derive(Args)]
pub struct CompletionCommand {
    /// Shell to generate completions for.
    #[arg(value_enum)]
    pub shell: Option<clap_complete::Shell>,

    /// Install completions to your shell configuration file.
    #[arg(long = "install")]
    pub install: bool,
}

/// Init command - initialize AGENTS.md.
#[derive(Args)]
pub struct InitCommand {
    /// Force overwrite if AGENTS.md already exists.
    #[arg(short = 'f', long = "force")]
    pub force: bool,

    /// Accept defaults without prompting (non-interactive mode).
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
}

/// Resume command.
#[derive(Args)]
pub struct ResumeCommand {
    /// Session ID to resume (or "last" for most recent)
    #[arg(value_name = "SESSION_ID")]
    pub session_id: Option<String>,

    /// Continue the most recent session without showing the picker
    #[arg(long = "last", default_value_t = false, conflicts_with = "session_id")]
    pub last: bool,

    /// Show interactive picker to select from recent sessions
    #[arg(long = "pick", default_value_t = false, conflicts_with_all = ["session_id", "last"])]
    pub pick: bool,

    /// Show all sessions (disables cwd filtering)
    #[arg(long = "all", default_value_t = false)]
    pub all: bool,

    /// Do not persist session changes (incompatible with resume, will error).
    #[arg(long = "no-session", default_value_t = false)]
    pub no_session: bool,

    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,
}

/// Sessions command.
#[derive(Args)]
pub struct SessionsCommand {
    /// Show all sessions including from other directories
    #[arg(long)]
    pub all: bool,

    /// Show sessions from the last N days
    #[arg(long)]
    pub days: Option<u32>,

    /// Show sessions since this date (YYYY-MM-DD)
    #[arg(long)]
    pub since: Option<String>,

    /// Show sessions until this date (YYYY-MM-DD)
    #[arg(long)]
    pub until: Option<String>,

    /// Show only favorite sessions
    #[arg(long)]
    pub favorites: bool,

    /// Search sessions by title or ID
    #[arg(long, short)]
    pub search: Option<String>,

    /// Maximum number of sessions to show
    #[arg(long, short)]
    pub limit: Option<usize>,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Delete command - delete a session.
#[derive(Args)]
pub struct DeleteCommand {
    /// Session ID to delete (full UUID or 8-character prefix)
    #[arg(required = true)]
    pub session_id: String,

    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Force deletion even if session is locked
    #[arg(long, short = 'f')]
    pub force: bool,
}

/// Config command.
#[derive(Args)]
pub struct ConfigCommand {
    /// Show configuration in JSON format
    #[arg(long)]
    pub json: bool,

    /// Edit configuration interactively
    #[arg(long)]
    pub edit: bool,

    #[command(subcommand)]
    pub action: Option<ConfigSubcommand>,
}

/// Config subcommands.
#[derive(Subcommand)]
pub enum ConfigSubcommand {
    /// Get a configuration value
    Get(ConfigGetArgs),
    /// Set a configuration value
    Set(ConfigSetArgs),
    /// Unset (remove) a configuration value
    Unset(ConfigUnsetArgs),
}

/// Arguments for config get.
#[derive(Args)]
pub struct ConfigGetArgs {
    /// Configuration key to get (e.g., model, provider)
    pub key: String,
}

/// Arguments for config set.
#[derive(Args)]
pub struct ConfigSetArgs {
    /// Configuration key (e.g., model, provider)
    pub key: String,
    /// Value to set
    pub value: String,
}

/// Arguments for config unset.
#[derive(Args)]
pub struct ConfigUnsetArgs {
    /// Configuration key to remove
    pub key: String,
}

/// Sandbox debug commands.
#[derive(Args)]
pub struct SandboxArgs {
    #[command(subcommand)]
    pub cmd: SandboxCommand,
}

/// Sandbox subcommands.
#[derive(Subcommand)]
pub enum SandboxCommand {
    /// Run a command under Seatbelt (macOS only)
    #[command(visible_alias = "seatbelt")]
    Macos(SeatbeltCommand),
    /// Run a command under Landlock+seccomp (Linux only)
    #[command(visible_alias = "landlock")]
    Linux(LandlockCommand),
    /// Run a command under Windows restricted token (Windows only)
    Windows(WindowsCommand),
}

/// Features command.
#[derive(Args)]
pub struct FeaturesCommand {
    #[command(subcommand)]
    pub sub: FeaturesSubcommand,
}

/// Features subcommands.
#[derive(Subcommand)]
pub enum FeaturesSubcommand {
    /// List known features with their stage and effective state
    List,
}

/// Serve command - runs HTTP API server.
#[derive(Args)]
pub struct ServeCommand {
    /// Port to listen on
    #[arg(short, long, default_value = "3000")]
    pub port: u16,

    /// Host address to bind the server to.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Authentication token for API access.
    #[arg(long = "auth-token")]
    pub auth_token: Option<String>,

    /// Enable CORS (Cross-Origin Resource Sharing) for all origins.
    #[arg(long)]
    pub cors: bool,

    /// Allowed CORS origin(s). Can be specified multiple times.
    #[arg(long = "cors-origin", value_name = "ORIGIN")]
    pub cors_origins: Vec<String>,

    /// Enable mDNS service discovery (advertise on local network)
    #[arg(long = "mdns", default_value_t = false)]
    pub mdns: bool,

    /// Disable mDNS service discovery
    #[arg(long = "no-mdns", default_value_t = false, conflicts_with = "mdns")]
    pub no_mdns: bool,

    /// Custom service name for mDNS advertising
    #[arg(long = "mdns-name")]
    pub mdns_name: Option<String>,
}

/// Servers command - discover Cortex servers on the network.
#[derive(Args)]
pub struct ServersCommand {
    #[command(subcommand)]
    pub action: Option<ServersSubcommand>,

    /// Timeout for discovery in seconds
    #[arg(short, long, default_value = "3")]
    pub timeout: u64,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Servers subcommands.
#[derive(Subcommand)]
pub enum ServersSubcommand {
    /// Re-scan the network for mDNS servers (forces a fresh discovery)
    Refresh(ServersRefreshArgs),
}

/// Arguments for servers refresh command.
#[derive(Args)]
pub struct ServersRefreshArgs {
    /// Timeout for discovery in seconds
    #[arg(short, long, default_value = "5")]
    pub timeout: u64,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// History command - view past prompts and sessions.
#[derive(Args)]
pub struct HistoryCommand {
    #[command(subcommand)]
    pub action: Option<HistorySubcommand>,

    /// Maximum number of entries to show
    #[arg(short = 'n', long, default_value = "20")]
    pub limit: usize,

    /// Show history from all directories
    #[arg(long)]
    pub all: bool,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// History subcommands.
#[derive(Subcommand)]
pub enum HistorySubcommand {
    /// Search history for a pattern
    Search(HistorySearchArgs),
    /// Clear history (requires confirmation)
    Clear(HistoryClearArgs),
}

/// Arguments for history search command.
#[derive(Args)]
pub struct HistorySearchArgs {
    /// Pattern to search for in prompts
    pub pattern: String,

    /// Maximum number of results
    #[arg(short = 'n', long, default_value = "20")]
    pub limit: usize,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Arguments for history clear command.
#[derive(Args)]
pub struct HistoryClearArgs {
    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,
}

#[cfg(test)]
#[path = "args_parse_command_tests.rs"]
mod command_tests;
#[cfg(test)]
#[path = "args_parse_tests.rs"]
mod tests;
