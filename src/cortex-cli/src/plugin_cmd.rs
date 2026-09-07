//! Artifact management and executable Node/WASM plugin commands.
use anyhow::{Result, bail};
use clap::Parser;
use cortex_engine::plugin::runtime;
use std::path::PathBuf;

mod build;
mod install;
mod run;
mod scaffold;
mod validate;

#[derive(Debug, Parser)]
pub struct PluginCli {
    #[command(subcommand)]
    pub subcommand: PluginSubcommand,
}

/// Plugin subcommands.
#[derive(Debug, clap::Subcommand)]
pub enum PluginSubcommand {
    /// List installed plugins
    #[command(visible_alias = "ls")]
    List(PluginListArgs),

    /// Install a plugin
    #[command(visible_alias = "add")]
    Install(PluginInstallArgs),

    /// Remove a plugin
    #[command(visible_aliases = ["rm", "uninstall"])]
    Remove(PluginRemoveArgs),

    /// Enable a plugin
    Enable(PluginEnableArgs),

    /// Disable a plugin
    Disable(PluginDisableArgs),

    /// Show plugin information
    #[command(visible_alias = "info")]
    Show(PluginShowArgs),

    /// Create a new plugin project
    #[command(visible_alias = "create")]
    New(PluginNewArgs),

    /// Rebuild in development mode (no runtime hot-reload)
    Dev(PluginDevArgs),

    /// Build the selected executable artifact
    Build(PluginBuildArgs),

    /// Validate plugin manifest and structure
    #[command(visible_alias = "check")]
    Validate(PluginValidateArgs),

    /// Prepare plugin for publication (dry-run)
    Publish(PluginPublishArgs),

    /// Search the plugin registry
    Search(PluginSearchArgs),

    /// Browse plugins in the registry
    Browse(PluginBrowseArgs),

    /// Update an installed plugin from the registry
    Update(PluginUpdateArgs),

    /// Explicitly trust native JavaScript execution for this installed artifact
    Trust(PluginTrustArgs),

    /// Invoke an installed plugin command or tool through the executable runtime
    Run(PluginRunArgs),
}

/// Arguments for plugin list command.
#[derive(Debug, Parser)]
pub struct PluginListArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Show only enabled plugins
    #[arg(long)]
    pub enabled: bool,

    /// Show only disabled plugins
    #[arg(long)]
    pub disabled: bool,
}

/// Arguments for plugin install command.
#[derive(Debug, Parser)]
pub struct PluginInstallArgs {
    /// Trust this exact package to execute native Node code (NOT a sandbox)
    #[arg(long)]
    pub trust_code: bool,
    /// Local package path or registry plugin ID
    pub name: String,

    /// Plugin version (defaults to latest)
    #[arg(long)]
    pub version: Option<String>,

    /// Force reinstall if already installed
    #[arg(long, short = 'f')]
    pub force: bool,
}

/// Arguments for plugin remove command.
#[derive(Debug, Parser)]
pub struct PluginRemoveArgs {
    /// Plugin name to remove
    pub name: String,

    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

/// Arguments for plugin enable command.
#[derive(Debug, Parser)]
pub struct PluginEnableArgs {
    /// Plugin name to enable
    pub name: String,
}

/// Arguments for plugin disable command.
#[derive(Debug, Parser)]
pub struct PluginDisableArgs {
    /// Plugin name to disable
    pub name: String,
}

/// Arguments for plugin show command.
#[derive(Debug, Parser)]
pub struct PluginShowArgs {
    /// Plugin name to show
    pub name: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Arguments for plugin new command.
#[derive(Debug, Parser)]
pub struct PluginNewArgs {
    /// Plugin name (will be used as directory name and ID)
    pub name: String,

    /// Plugin description
    #[arg(long, short = 'd', default_value = "A Cortex plugin")]
    pub description: String,

    /// Plugin author
    #[arg(long, short = 'a')]
    pub author: Option<String>,

    /// Output directory (defaults to current directory)
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,

    /// Use advanced template with TUI hooks
    #[arg(long)]
    pub advanced: bool,

    /// Use TypeScript template instead of Rust
    #[arg(long)]
    pub typescript: bool,
}

/// Arguments for plugin dev command.
#[derive(Debug, Parser)]
pub struct PluginDevArgs {
    /// Plugin directory (defaults to current directory)
    #[arg(long, short = 'p')]
    pub path: Option<PathBuf>,

    /// Watch for file changes and auto-rebuild
    #[arg(long, short = 'w')]
    pub watch: bool,

    /// Debounce time in milliseconds for file change events
    #[arg(long, default_value = "500")]
    pub debounce_ms: u64,
}

/// Arguments for plugin build command.
#[derive(Debug, Parser)]
pub struct PluginBuildArgs {
    /// Allow native Rust build scripts/procedural macros; not needed for TypeScript stripping
    #[arg(long)]
    pub trust_code: bool,
    /// Plugin directory (defaults to current directory)
    #[arg(long, short = 'p')]
    pub path: Option<PathBuf>,

    /// Build in debug mode (faster, larger output)
    #[arg(long)]
    pub debug: bool,

    /// Output directory for the compiled WASM file
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,
}

/// Arguments for plugin validate command.
#[derive(Debug, Parser)]
pub struct PluginValidateArgs {
    /// Plugin directory (defaults to current directory)
    #[arg(long, short = 'p')]
    pub path: Option<PathBuf>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Show verbose output with all checks
    #[arg(long, short = 'v')]
    pub verbose: bool,
}

/// Arguments for plugin publish command.
#[derive(Debug, Parser)]
pub struct PluginPublishArgs {
    /// Plugin directory (defaults to current directory)
    #[arg(long, short = 'p')]
    pub path: Option<PathBuf>,

    /// Dry-run mode (default, no actual publishing)
    #[arg(long, default_value = "true")]
    pub dry_run: bool,

    /// Output tarball path (defaults to plugin-name-version.tar.gz)
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,
}

/// Arguments for plugin search.
#[derive(Debug, Parser)]
pub struct PluginSearchArgs {
    /// Search query
    pub query: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Arguments for plugin browse.
#[derive(Debug, Parser)]
pub struct PluginBrowseArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Arguments for plugin update.
#[derive(Debug, Parser)]
pub struct PluginUpdateArgs {
    /// Local replacement package; otherwise use the registry
    #[arg(long)]
    pub source: Option<PathBuf>,
    /// Plugin name to update
    pub name: String,
}

#[derive(Debug, Parser)]
pub struct PluginTrustArgs {
    pub name: String,
    /// Acknowledge that Node plugins can access files, network and subprocesses
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Parser)]
pub struct PluginRunArgs {
    pub name: String,
    #[arg(required_unless_present = "tool", conflicts_with = "tool")]
    pub command: Option<String>,
    pub args: Vec<String>,
    #[arg(long)]
    pub tool: Option<String>,
    #[arg(long, requires = "tool", default_value = "{}")]
    pub input: String,
    #[arg(long)]
    pub json: bool,
}

impl PluginCli {
    pub async fn run(self) -> Result<()> {
        let json = match &self.subcommand {
            PluginSubcommand::List(args) => args.json,
            PluginSubcommand::Show(args) => args.json,
            PluginSubcommand::Validate(args) => args.json,
            PluginSubcommand::Search(args) => args.json,
            PluginSubcommand::Browse(args) => args.json,
            PluginSubcommand::Run(args) => args.json,
            _ => false,
        };
        if json {
            use tracing::instrument::WithSubscriber;
            self.dispatch()
                .with_subscriber(tracing::subscriber::NoSubscriber::default())
                .await
        } else {
            self.dispatch().await
        }
    }

    async fn dispatch(self) -> Result<()> {
        match self.subcommand {
            PluginSubcommand::List(args) => install::list(args).await,
            PluginSubcommand::Install(args) => install::install(args).await,
            PluginSubcommand::Remove(args) => install::remove(args),
            PluginSubcommand::Enable(args) => install::set_enabled(&args.name, true),
            PluginSubcommand::Disable(args) => install::set_enabled(&args.name, false),
            PluginSubcommand::Show(args) => install::show(args),
            PluginSubcommand::New(args) => scaffold::create(args),
            PluginSubcommand::Build(args) => build::run(args),
            PluginSubcommand::Dev(args) => {
                if args.watch {
                    bail!(
                        "Watch mode is not supported by the executable runtime; rebuild and explicitly reload"
                    );
                }
                build::run(PluginBuildArgs {
                    path: args.path,
                    debug: true,
                    output: None,
                    trust_code: false,
                })
            }
            PluginSubcommand::Validate(args) => validate::run(args).await,
            PluginSubcommand::Publish(args) => install::publish(args),
            PluginSubcommand::Search(args) => install::search(Some(args.query), args.json).await,
            PluginSubcommand::Browse(args) => install::search(None, args.json).await,
            PluginSubcommand::Update(args) => install::update(args).await,
            PluginSubcommand::Trust(args) => install::trust(args),
            PluginSubcommand::Run(args) => run::run(args).await,
        }
    }
}

fn plugins_dir() -> Result<PathBuf> {
    Ok(runtime::activation::default_plugins_dir()?)
}
fn state_path() -> Result<PathBuf> {
    runtime::activation::default_state_path()
        .ok_or_else(|| anyhow::anyhow!("Cannot locate plugin activation configuration"))
}
fn package_path(path: Option<PathBuf>) -> Result<PathBuf> {
    Ok(path.unwrap_or(std::env::current_dir()?))
}
