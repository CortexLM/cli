//! Configuration management for Cortex.
//!
//! This module provides configuration loading with support for:
//! - Global configuration from `~/.cortex/config.toml`
//! - Per-project configuration from `.cortex/config.toml` or `cortex.toml`
//! - Configuration merging (global → project → CLI args)
//! - Environment variable overrides (`CORTEX_CONFIG`, `CORTEX_CONFIG_DIR`)

mod config_discovery;
mod execution;
mod loader;
mod project_config;
mod providers;
mod types;

pub use execution::ExecutionConfig;

pub use config_discovery::{
    cache_size as config_cache_size, clear_cache as clear_config_cache, find_project_root, find_up,
    git_root, is_in_git_repo,
};
pub use loader::{
    CONFIG_FILE_JSON, CONFIG_FILE_JSONC, CORTEX_CONFIG_DIR_ENV, CORTEX_CONFIG_ENV, ConfigFormat,
    find_cortex_home, get_config_path, load_config, load_config_sync, load_merged_config,
    load_merged_config_sync, parse_config_content, strip_json_comments,
};
pub use project_config::{
    PROJECT_CONFIG_NAMES, find_project_config, get_project_config_dir, get_project_config_path,
    load_project_config, merge_configs,
};
pub use providers::*;
pub use types::*;

use std::collections::HashMap;
use std::path::PathBuf;

use cortex_protocol::{AskForApproval, SandboxPolicy};

/// Main configuration struct.
#[derive(Debug, Clone)]
pub struct Config {
    /// Model to use.
    pub model: String,
    /// Model provider ID.
    pub model_provider_id: String,
    /// Provider configuration.
    pub model_provider: ModelProviderInfo,
    /// Context window size.
    pub model_context_window: Option<i64>,
    /// Auto-compact token limit.
    pub model_auto_compact_token_limit: Option<i64>,
    /// Approval policy.
    pub approval_policy: AskForApproval,
    /// Sandbox policy.
    pub sandbox_policy: SandboxPolicy,
    /// Working directory.
    pub cwd: PathBuf,
    /// Cortex home directory.
    pub cortex_home: PathBuf,
    /// MCP server configurations.
    pub mcp_servers: HashMap<String, McpServerConfig>,
    /// User instructions from AGENTS.md.
    pub user_instructions: Option<String>,
    /// History settings.
    pub history: HistoryConfig,
    /// Feature flags.
    pub features: Features,
    /// Reasoning effort.
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Reasoning summary mode.
    pub reasoning_summary: ReasoningSummary,
    /// Hide agent reasoning in output.
    pub hide_agent_reasoning: bool,
    /// Show raw reasoning content.
    pub show_raw_agent_reasoning: bool,
    /// Check for updates on startup.
    pub check_for_update_on_startup: bool,
    /// Disable paste burst detection.
    pub disable_paste_burst: bool,
    /// Enable TUI animations.
    pub animations: bool,
    /// Opt in to the alternate screen buffer. Default is inline (false).
    pub alternate_screen: bool,
    /// Current agent profile.
    pub current_agent: Option<String>,
    /// Granular permission configuration.
    pub permission: PermissionConfig,
    /// Small model for lightweight tasks (title generation, summaries).
    /// Format: "provider/model" (e.g., "openai/gpt-4o-mini").
    pub small_model: Option<String>,
    /// Temperature for generation (0.0-2.0).
    /// CLI override takes precedence over agent default.
    pub temperature: Option<f32>,
    /// Custom providers defined in config.toml.
    pub providers: HashMap<String, CustomProviderConfig>,
    /// Execution configuration for runtime behavior.
    pub execution: ExecutionConfig,
}

impl Default for Config {
    fn default() -> Self {
        let cortex_home = find_cortex_home().unwrap_or_else(|_| PathBuf::from(".cortex"));
        Self {
            model: "claude-opus-4-5-20251101".to_string(),
            model_provider_id: "cortex".to_string(),
            model_provider: ModelProviderInfo::default(),
            model_context_window: Some(128_000),
            model_auto_compact_token_limit: None,
            approval_policy: AskForApproval::default(),
            sandbox_policy: SandboxPolicy::default(),
            cwd: std::env::current_dir().unwrap_or_default(),
            cortex_home,
            mcp_servers: HashMap::new(),
            user_instructions: None,
            history: HistoryConfig::default(),
            features: Features::default(),
            reasoning_effort: None,
            reasoning_summary: ReasoningSummary::default(),
            hide_agent_reasoning: false,
            show_raw_agent_reasoning: false,
            check_for_update_on_startup: true,
            disable_paste_burst: false,
            animations: true,
            alternate_screen: false,
            current_agent: None,
            permission: PermissionConfig::default(),
            small_model: None, // Auto-detected based on available providers
            temperature: None,
            providers: HashMap::new(),
            execution: ExecutionConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration with optional overrides.
    ///
    /// This method:
    /// 1. Loads global config from `~/.cortex/config.toml` (or `CORTEX_CONFIG_DIR`)
    /// 2. Discovers project config (`.cortex/config.toml` or `cortex.toml`)
    /// 3. Merges them (global → project → CLI overrides)
    ///
    /// Environment variables:
    /// - `CORTEX_CONFIG`: Path to a specific config file
    /// - `CORTEX_CONFIG_DIR`: Directory containing config.toml
    /// - `CORTEX_HOME`: Alias for `CORTEX_CONFIG_DIR`
    pub async fn load(overrides: ConfigOverrides) -> std::io::Result<Self> {
        let cortex_home = find_cortex_home()?;

        // Get the working directory from overrides or current dir
        let cwd = overrides
            .cwd
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();

        // Load merged config (global + project)
        let (toml, _project_config_path) = load_merged_config(&cortex_home, &cwd).await?;

        Ok(Self::from_toml(toml, overrides, cortex_home))
    }

    /// Load configuration synchronously with optional overrides.
    pub fn load_sync(overrides: ConfigOverrides) -> std::io::Result<Self> {
        let cortex_home = find_cortex_home()?;

        // Get the working directory from overrides or current dir
        let cwd = overrides
            .cwd
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();

        // Load merged config (global + project)
        let (toml, _project_config_path) = load_merged_config_sync(&cortex_home, &cwd)?;

        Ok(Self::from_toml(toml, overrides, cortex_home))
    }

    /// Create config from TOML and overrides.
    pub fn from_toml(toml: ConfigToml, overrides: ConfigOverrides, cortex_home: PathBuf) -> Self {
        let cwd = overrides
            .cwd
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();

        let model = overrides
            .model
            .or(toml.model)
            .unwrap_or_else(|| "claude-opus-4-5-20251101".to_string());

        let model_provider_id = overrides
            .model_provider
            .or(toml.model_provider)
            .unwrap_or_else(|| "cortex".to_string());

        let approval_policy = overrides
            .approval_policy
            .or(toml.approval_policy)
            .unwrap_or_default();

        let sandbox_policy = toml
            .sandbox_mode
            .map(|mode| match mode {
                SandboxMode::DangerFullAccess => SandboxPolicy::DangerFullAccess,
                SandboxMode::ReadOnly => SandboxPolicy::ReadOnly,
                SandboxMode::WorkspaceWrite => {
                    if let Some(cfg) = &toml.sandbox_workspace_write {
                        SandboxPolicy::WorkspaceWrite {
                            writable_roots: cfg.writable_roots.clone(),
                            network_access: cfg.network_access,
                            exclude_tmpdir_env_var: cfg.exclude_tmpdir_env_var,
                            exclude_slash_tmp: cfg.exclude_slash_tmp,
                        }
                    } else {
                        SandboxPolicy::default()
                    }
                }
            })
            .unwrap_or_default();

        let model_provider = resolve_model_provider(&model_provider_id, &toml.providers);

        Self {
            model,
            model_provider_id,
            model_provider,
            model_context_window: toml.model_context_window,
            model_auto_compact_token_limit: toml.model_auto_compact_token_limit,
            approval_policy,
            sandbox_policy,
            cwd,
            cortex_home,
            mcp_servers: toml.mcp_servers,
            user_instructions: toml.instructions,
            history: toml.history.unwrap_or_default(),
            features: Features::default(),
            reasoning_effort: toml.model_reasoning_effort,
            reasoning_summary: toml.model_reasoning_summary.unwrap_or_default(),
            hide_agent_reasoning: toml.hide_agent_reasoning.unwrap_or(false),
            show_raw_agent_reasoning: toml.show_raw_agent_reasoning.unwrap_or(false),
            check_for_update_on_startup: toml.check_for_update_on_startup.unwrap_or(true),
            disable_paste_burst: toml.disable_paste_burst.unwrap_or(false),
            animations: toml.tui.as_ref().map(|t| t.animations).unwrap_or(true),
            alternate_screen: overrides.alternate_screen.unwrap_or_else(|| {
                toml.tui
                    .as_ref()
                    .map(|t| t.alternate_screen)
                    .unwrap_or(false)
            }),
            current_agent: toml.current_agent,
            permission: toml.permission,
            small_model: toml.small_model,
            // CLI temperature override takes precedence
            temperature: overrides.temperature,
            providers: toml.providers,
            execution: toml.execution,
        }
    }
}

fn resolve_model_provider(
    id: &str,
    providers: &HashMap<String, CustomProviderConfig>,
) -> ModelProviderInfo {
    if let Some(custom) = providers.get(id) {
        return ModelProviderInfo {
            id: id.to_string(),
            name: if custom.name.is_empty() {
                id.to_string()
            } else {
                custom.name.clone()
            },
            base_url: custom.base_url.clone(),
            api_type: ApiType::from_config(&custom.api_type),
            api_key_env: custom.api_key_env.clone(),
        };
    }
    match id {
        "anthropic" | "anthropic-compatible" => ModelProviderInfo {
            id: id.to_string(),
            name: "Anthropic-compatible".into(),
            base_url: "https://api.anthropic.com".into(),
            api_type: ApiType::Anthropic,
            api_key_env: Some("ANTHROPIC_API_KEY".into()),
        },
        "azure" => ModelProviderInfo {
            id: id.to_string(),
            name: "Azure".into(),
            base_url: String::new(),
            api_type: ApiType::Azure,
            api_key_env: Some("AZURE_OPENAI_API_KEY".into()),
        },
        "local" | "ollama" => ModelProviderInfo {
            id: id.to_string(),
            name: "Local".into(),
            base_url: "http://127.0.0.1:11434/v1".into(),
            api_type: ApiType::Local,
            api_key_env: None,
        },
        "openai" | "openai-compatible" => ModelProviderInfo {
            id: id.to_string(),
            name: "OpenAI-compatible".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_type: ApiType::OpenAiCompatible,
            api_key_env: Some("OPENAI_API_KEY".into()),
        },
        _ => ModelProviderInfo::default(),
    }
}

/// Configuration overrides from CLI.
#[derive(Debug, Clone, Default)]
pub struct ConfigOverrides {
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub cwd: Option<PathBuf>,
    pub approval_policy: Option<AskForApproval>,
    pub sandbox_mode: Option<SandboxMode>,
    pub additional_writable_roots: Vec<PathBuf>,
    /// Temperature override from CLI (0.0-2.0).
    pub temperature: Option<f32>,
    /// When `Some(true)`, enter the alternate screen. Default (None/false) is inline.
    pub alternate_screen: Option<bool>,
}

#[cfg(test)]
mod alternate_screen_tests {
    use super::*;
    use types::ConfigToml;

    #[test]
    fn from_toml_keeps_alternate_screen_off_by_default() {
        let toml: ConfigToml = toml::from_str("").unwrap();
        let cfg = Config::from_toml(toml, ConfigOverrides::default(), PathBuf::from("/tmp"));
        assert!(!cfg.alternate_screen);
    }

    #[test]
    fn from_toml_honors_tui_alternate_screen_opt_in() {
        let toml: ConfigToml = toml::from_str("[tui]\nalternate_screen = true\n").unwrap();
        let cfg = Config::from_toml(toml, ConfigOverrides::default(), PathBuf::from("/tmp"));
        assert!(cfg.alternate_screen);
    }

    #[test]
    fn cli_override_can_force_alternate_screen() {
        let toml: ConfigToml = toml::from_str("").unwrap();
        let cfg = Config::from_toml(
            toml,
            ConfigOverrides {
                alternate_screen: Some(true),
                ..Default::default()
            },
            PathBuf::from("/tmp"),
        );
        assert!(cfg.alternate_screen);
    }
}
