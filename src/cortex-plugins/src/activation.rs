//! One durable activation/trust store for the CLI and executable manager.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{PluginError, Result};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Activation {
    pub disabled: BTreeSet<String>,
    pub trusted: BTreeMap<String, String>,
}

impl Activation {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        match std::fs::read(path) {
            Ok(bytes) if bytes.len() <= 1024 * 1024 => Ok(serde_json::from_slice(&bytes)?),
            Ok(_) => Err(PluginError::ConfigError(
                "Plugin activation file is too large".into(),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| PluginError::ConfigError("Missing config parent".into()))?;
        std::fs::create_dir_all(parent)?;
        let temp = parent.join(format!(".plugins-{}.json", uuid::Uuid::new_v4()));
        let result = (|| {
            use std::io::Write;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp)?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.sync_all()?;
            std::fs::rename(&temp, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        result
    }
}

pub fn default_state_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".cortex/plugins.json"))
}

pub fn default_plugins_dir() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".cortex/plugins"))
        .ok_or_else(|| PluginError::ConfigError("Cannot locate the user plugin directory".into()))
}
