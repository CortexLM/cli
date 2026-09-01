//! Builder for settings selection with categories.

use crate::interactive::state::{InteractiveAction, InteractiveItem, InteractiveState};

/// Lock hub rows for `/settings` (never Display / Behavior / AI / Git / Cloud / Privacy).
#[derive(Debug, Clone, Copy, PartialEq)]
enum SettingsHubRow {
    Model,
    Mode,
    Permissions,
    Sandbox,
    Mcp,
    Config,
    Usage,
}

impl SettingsHubRow {
    const ALL: [SettingsHubRow; 7] = [
        Self::Model,
        Self::Mode,
        Self::Permissions,
        Self::Sandbox,
        Self::Mcp,
        Self::Config,
        Self::Usage,
    ];

    fn label(&self) -> &'static str {
        match self {
            Self::Model => "Model",
            Self::Mode => "Mode",
            Self::Permissions => "Permissions",
            Self::Sandbox => "Sandbox",
            Self::Mcp => "MCP",
            Self::Config => "Config",
            Self::Usage => "Usage",
        }
    }

    fn id(&self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Mode => "mode",
            Self::Permissions => "permissions",
            Self::Sandbox => "sandbox",
            Self::Mcp => "mcp",
            Self::Config => "config",
            Self::Usage => "usage",
        }
    }

    fn value(&self) -> &'static str {
        match self {
            Self::Model => "cortex-1-mini · Medium",
            Self::Mode => "Agent",
            Self::Permissions => "Smart",
            Self::Sandbox => "On · workspace",
            Self::Mcp => "3 of 4 connected",
            Self::Config => "~/.cortex/config.json",
            Self::Usage => "42 / 500 agent requests",
        }
    }
}

/// Setting category for grouping legacy toggles.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SettingCategory {
    Display,
    Behavior,
    AI,
    Git,
    Cloud,
    Privacy,
}

impl SettingCategory {
    const ALL: [SettingCategory; 6] = [
        Self::Display,
        Self::Behavior,
        Self::AI,
        Self::Git,
        Self::Cloud,
        Self::Privacy,
    ];

    fn label(&self) -> &'static str {
        match self {
            Self::Display => "Display",
            Self::Behavior => "Behavior",
            Self::AI => "AI",
            Self::Git => "Git",
            Self::Cloud => "Cloud",
            Self::Privacy => "Privacy",
        }
    }

    fn id(&self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::Behavior => "behavior",
            Self::AI => "ai",
            Self::Git => "git",
            Self::Cloud => "cloud",
            Self::Privacy => "privacy",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id {
            "display" => Some(Self::Display),
            "behavior" => Some(Self::Behavior),
            "ai" => Some(Self::AI),
            "git" => Some(Self::Git),
            "cloud" => Some(Self::Cloud),
            "privacy" => Some(Self::Privacy),
            other => Self::ALL
                .iter()
                .copied()
                .find(|c| c.label().eq_ignore_ascii_case(other)),
        }
    }
}

/// Settings definition
struct SettingDef {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    category: SettingCategory,
}

const SETTINGS: &[SettingDef] = &[
    // Display
    SettingDef {
        id: "compact",
        label: "Compact Mode",
        description: "Reduce visual spacing",
        category: SettingCategory::Display,
    },
    SettingDef {
        id: "timestamps",
        label: "Timestamps",
        description: "Show message timestamps",
        category: SettingCategory::Display,
    },
    SettingDef {
        id: "line_numbers",
        label: "Line Numbers",
        description: "Show line numbers in code",
        category: SettingCategory::Display,
    },
    SettingDef {
        id: "word_wrap",
        label: "Word Wrap",
        description: "Wrap long lines",
        category: SettingCategory::Display,
    },
    SettingDef {
        id: "syntax_highlight",
        label: "Syntax Highlight",
        description: "Colorize code blocks",
        category: SettingCategory::Display,
    },
    // Behavior
    SettingDef {
        id: "auto_approve",
        label: "Auto Approve",
        description: "Auto-approve tool calls",
        category: SettingCategory::Behavior,
    },
    SettingDef {
        id: "sandbox",
        label: "Sandbox Mode",
        description: "Run tools in sandbox",
        category: SettingCategory::Behavior,
    },
    SettingDef {
        id: "streaming",
        label: "Streaming",
        description: "Stream responses live",
        category: SettingCategory::Behavior,
    },
    SettingDef {
        id: "auto_scroll",
        label: "Auto Scroll",
        description: "Scroll to new messages",
        category: SettingCategory::Behavior,
    },
    SettingDef {
        id: "sound",
        label: "Sound",
        description: "Play notification sounds",
        category: SettingCategory::Behavior,
    },
    // AI
    SettingDef {
        id: "thinking",
        label: "Thinking Mode",
        description: "Show model thinking",
        category: SettingCategory::AI,
    },
    SettingDef {
        id: "debug",
        label: "Debug Mode",
        description: "Show debug info",
        category: SettingCategory::AI,
    },
    SettingDef {
        id: "context_aware",
        label: "Context Aware",
        description: "Include open files context",
        category: SettingCategory::AI,
    },
    // Git
    SettingDef {
        id: "co_author",
        label: "Co-Author",
        description: "Add as commit co-author",
        category: SettingCategory::Git,
    },
    SettingDef {
        id: "auto_commit",
        label: "Auto Commit",
        description: "Suggest commits after changes",
        category: SettingCategory::Git,
    },
    SettingDef {
        id: "sign_commits",
        label: "Sign Commits",
        description: "GPG sign commits",
        category: SettingCategory::Git,
    },
    // Cloud
    SettingDef {
        id: "cloud_sync",
        label: "Cloud Sync",
        description: "Sync sessions to cloud",
        category: SettingCategory::Cloud,
    },
    SettingDef {
        id: "auto_save",
        label: "Auto Save",
        description: "Auto-save sessions",
        category: SettingCategory::Cloud,
    },
    SettingDef {
        id: "session_history",
        label: "Session History",
        description: "Keep session history",
        category: SettingCategory::Cloud,
    },
    // Privacy
    SettingDef {
        id: "telemetry",
        label: "Telemetry",
        description: "Send usage telemetry",
        category: SettingCategory::Privacy,
    },
    SettingDef {
        id: "analytics",
        label: "Analytics",
        description: "Usage analytics",
        category: SettingCategory::Privacy,
    },
];

/// Current settings state for display
#[derive(Default, Clone)]
pub struct SettingsSnapshot {
    // Display
    pub compact_mode: bool,
    pub timestamps: bool,
    pub line_numbers: bool,
    pub word_wrap: bool,
    pub syntax_highlight: bool,
    // Behavior
    pub auto_approve: bool,
    pub sandbox_mode: bool,
    pub streaming_enabled: bool,
    pub auto_scroll: bool,
    pub sound: bool,
    // AI
    pub thinking_enabled: bool,
    pub debug_mode: bool,
    pub context_aware: bool,
    // Git
    pub co_author: bool,
    pub auto_commit: bool,
    pub sign_commits: bool,
    // Cloud
    pub cloud_sync: bool,
    pub auto_save: bool,
    pub session_history: bool,
    // Privacy
    pub telemetry: bool,
    pub analytics: bool,
}

/// Build an interactive state for settings: a hub of sections, not a dumped list.
pub fn build_settings_selector(
    snapshot: SettingsSnapshot,
    terminal_height: Option<u16>,
) -> InteractiveState {
    let _ = snapshot;
    build_settings_hub(terminal_height)
}

/// Settings hub: Model, Mode, Permissions, Sandbox, MCP, Config, Usage.
pub fn build_settings_hub(terminal_height: Option<u16>) -> InteractiveState {
    let items: Vec<InteractiveItem> = SettingsHubRow::ALL
        .iter()
        .map(|row| {
            InteractiveItem::new(row.id().to_string(), row.label().to_string())
                .with_description(row.value().to_string())
        })
        .collect();

    let max_visible = visible_count(terminal_height, items.len());
    InteractiveState::new("Settings", items, InteractiveAction::ToggleSetting)
        .with_max_visible(max_visible)
        .with_hints(vec![
            ("↑↓".into(), "select".into()),
            ("↵".into(), "open".into()),
            ("esc".into(), "close".into()),
        ])
}

/// One settings section's toggles.
pub fn build_settings_section(
    snapshot: SettingsSnapshot,
    terminal_height: Option<u16>,
    section_id: &str,
) -> InteractiveState {
    let Some(category) = SettingCategory::from_id(section_id) else {
        return build_settings_hub(terminal_height);
    };

    let mut items = vec![
        InteractiveItem::new("__hub__", "< Sections")
            .with_description("Back to settings hub".to_string()),
    ];

    for setting in SETTINGS.iter().filter(|s| s.category == category) {
        let is_enabled = setting_enabled(&snapshot, setting.id);
        let status = if is_enabled { "Enabled" } else { "Disabled" };
        let icon = if is_enabled { '>' } else { ' ' };
        items.push(
            InteractiveItem::new(setting.id, setting.label)
                .with_description(format!("{} ({})", setting.description, status))
                .with_icon(icon),
        );
    }

    let max_visible = visible_count(terminal_height, items.len());
    InteractiveState::new(
        format!("Settings · {}", category.label()),
        items,
        InteractiveAction::ToggleSetting,
    )
    .with_max_visible(max_visible)
}

/// Build settings selector with specific tab active (legacy; maps onto a section).
pub fn build_settings_selector_with_tab(
    snapshot: SettingsSnapshot,
    terminal_height: Option<u16>,
    active_tab: usize,
) -> InteractiveState {
    let categories = SettingCategory::ALL;
    let section = categories
        .get(active_tab)
        .unwrap_or(&SettingCategory::Display);
    build_settings_section(snapshot, terminal_height, section.id())
}

fn setting_enabled(snapshot: &SettingsSnapshot, id: &str) -> bool {
    match id {
        "compact" => snapshot.compact_mode,
        "timestamps" => snapshot.timestamps,
        "line_numbers" => snapshot.line_numbers,
        "word_wrap" => snapshot.word_wrap,
        "syntax_highlight" => snapshot.syntax_highlight,
        "auto_approve" => snapshot.auto_approve,
        "sandbox" => snapshot.sandbox_mode,
        "streaming" => snapshot.streaming_enabled,
        "auto_scroll" => snapshot.auto_scroll,
        "sound" => snapshot.sound,
        "thinking" => snapshot.thinking_enabled,
        "debug" => snapshot.debug_mode,
        "context_aware" => snapshot.context_aware,
        "co_author" => snapshot.co_author,
        "auto_commit" => snapshot.auto_commit,
        "sign_commits" => snapshot.sign_commits,
        "cloud_sync" => snapshot.cloud_sync,
        "auto_save" => snapshot.auto_save,
        "session_history" => snapshot.session_history,
        "telemetry" => snapshot.telemetry,
        "analytics" => snapshot.analytics,
        _ => false,
    }
}

fn visible_count(terminal_height: Option<u16>, total_items: usize) -> usize {
    const UI_CHROME_HEIGHT: u16 = 4;
    match terminal_height {
        Some(height) => {
            let available = height.saturating_sub(UI_CHROME_HEIGHT) as usize;
            available.clamp(1, total_items.max(1))
        }
        None => total_items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_settings_selector() {
        let snapshot = SettingsSnapshot::default();
        let state = build_settings_selector(snapshot, None);
        assert_eq!(state.title, "Settings");
        assert_eq!(state.items.len(), 7);
        for label in [
            "Model",
            "Mode",
            "Permissions",
            "Sandbox",
            "MCP",
            "Config",
            "Usage",
        ] {
            assert!(state.items.iter().any(|i| i.label == label), "{label}");
        }
        assert!(!state.items.iter().any(|i| i.label == "Display"));
        assert_eq!(
            state.items[0].description.as_deref(),
            Some("cortex-1-mini · Medium")
        );
    }

    #[test]
    fn test_settings_hub_is_not_a_dumped_list() {
        let snapshot = SettingsSnapshot::default();
        let state = build_settings_selector(snapshot, None);
        assert!(
            !state.items.iter().any(|i| i.id == "compact"),
            "hub must not dump individual settings"
        );
    }

    #[test]
    fn test_settings_section_lists_toggles() {
        let snapshot = SettingsSnapshot::default();
        let state = build_settings_section(snapshot, None, "display");
        assert!(state.title.contains("Display"));
        assert!(state.items.iter().any(|i| i.id == "compact"));
        assert!(state.items.iter().any(|i| i.id == "__hub__"));
    }

    #[test]
    fn test_max_visible_dynamic_calculation() {
        let snapshot = SettingsSnapshot::default();

        let state_small = build_settings_selector(snapshot.clone(), Some(12));
        assert!(state_small.max_visible >= 7);

        let state_large = build_settings_selector(snapshot.clone(), Some(100));
        assert_eq!(state_large.max_visible, state_large.items.len());

        let state_default = build_settings_selector(snapshot, None);
        assert_eq!(state_default.max_visible, state_default.items.len());
    }
}
