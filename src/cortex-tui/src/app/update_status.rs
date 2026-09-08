//! Auto-update banner state for the TUI.

/// Status of the auto-update system
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UpdateStatus {
    /// No update check performed yet
    #[default]
    NotChecked,
    /// An update is available
    Available {
        /// The new version available
        version: String,
    },
    /// Currently downloading the update
    Downloading {
        /// The version being downloaded
        version: String,
        /// Download progress percentage (0-100)
        progress: u8,
    },
    /// Download complete, restart required
    ReadyToRestart {
        /// The version that was downloaded
        version: String,
    },
}

impl UpdateStatus {
    /// Returns true if an update notification should be shown
    pub fn should_show_banner(&self) -> bool {
        matches!(
            self,
            UpdateStatus::Available { .. }
                | UpdateStatus::Downloading { .. }
                | UpdateStatus::ReadyToRestart { .. }
        )
    }

    /// Get the banner text for the current status
    pub fn banner_text(&self) -> Option<String> {
        match self {
            UpdateStatus::Available { version } => {
                Some(format!("A new version ({}) is available", version))
            }
            UpdateStatus::Downloading { progress, .. } => {
                Some(format!("Downloading update... {}%", progress))
            }
            UpdateStatus::ReadyToRestart { .. } => {
                Some("You must restart to run the latest version".to_string())
            }
            _ => None,
        }
    }
}
