//! Plugin loader for discovering and loading plugins.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::PluginConfig;
use crate::manifest::PluginManifest;
use crate::runtime::{WasmPlugin, WasmRuntime};
use crate::{MANIFEST_FILE, Plugin, PluginError, Result, WASM_FILE};

/// Discovered plugin information.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    /// Plugin manifest
    pub manifest: PluginManifest,
    /// Plugin directory path
    pub path: PathBuf,
    /// Whether a WASM file exists
    pub has_wasm: bool,
}

impl DiscoveredPlugin {
    /// Get the plugin ID.
    pub fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    /// Get the plugin name.
    pub fn name(&self) -> &str {
        &self.manifest.plugin.name
    }

    /// Get the plugin version.
    pub fn version(&self) -> &str {
        &self.manifest.plugin.version
    }
}

/// Plugin loader for discovering and loading plugins from directories.
pub struct PluginLoader {
    config: PluginConfig,
    runtime: Arc<WasmRuntime>,
}

impl PluginLoader {
    /// Create a new plugin loader.
    pub fn new(config: PluginConfig, runtime: Arc<WasmRuntime>) -> Self {
        Self { config, runtime }
    }

    /// Discover plugins in all search paths.
    pub async fn discover(&self) -> Vec<DiscoveredPlugin> {
        let mut plugins = Vec::new();

        for search_path in &self.config.search_paths {
            if !search_path.exists() {
                tracing::debug!("Plugin search path does not exist: {:?}", search_path);
                continue;
            }

            tracing::debug!("Searching for plugins in: {:?}", search_path);

            match self.discover_in_path(search_path, false).await {
                Ok(found) => plugins.extend(found),
                Err(e) => {
                    tracing::warn!("Error discovering plugins in {:?}: {}", search_path, e);
                }
            }
        }

        tracing::info!("Discovered {} plugins", plugins.len());
        plugins
    }

    /// Startup discovery fails closed instead of turning malformed packages into absence.
    pub async fn discover_checked(&self) -> Result<Vec<DiscoveredPlugin>> {
        let mut plugins = Vec::new();
        for path in &self.config.search_paths {
            if path.exists() {
                plugins.extend(self.discover_in_path(path, true).await?);
            }
        }
        Ok(plugins)
    }

    /// Discover plugins in a specific directory.
    async fn discover_in_path(&self, path: &Path, strict: bool) -> Result<Vec<DiscoveredPlugin>> {
        let mut plugins = Vec::new();

        let mut entries = tokio::fs::read_dir(path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let entry_path = entry.path();

            // Check if it's a directory
            if !entry_path.is_dir() || entry.file_type().await?.is_symlink() {
                continue;
            }

            // Check for manifest file
            let manifest_path = entry_path.join(MANIFEST_FILE);
            if !manifest_path.exists() {
                continue;
            }

            // Try to load the manifest
            match self.load_manifest(&manifest_path).await {
                Ok(manifest) => {
                    // Validate manifest
                    if let Err(error) = manifest.validate() {
                        if strict {
                            return Err(error);
                        }
                        tracing::warn!("Invalid manifest in {:?}: {}", manifest_path, error);
                        continue;
                    }

                    // Check for WASM file
                    let wasm_path = entry_path.join(WASM_FILE);
                    let has_wasm = wasm_path.exists();

                    plugins.push(DiscoveredPlugin {
                        manifest,
                        path: entry_path,
                        has_wasm,
                    });
                }
                Err(error) if strict => return Err(error),
                Err(error) => tracing::warn!("Invalid manifest in {:?}: {}", manifest_path, error),
            }
        }

        plugins.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(plugins)
    }

    /// Load a manifest from a file.
    async fn load_manifest(&self, path: &Path) -> Result<PluginManifest> {
        let content = tokio::fs::read_to_string(path).await?;
        PluginManifest::parse(&content)
    }

    /// Load a validated artifact; importing native code still requires pinned trust.
    pub fn load(&self, discovered: &DiscoveredPlugin) -> Result<Box<dyn Plugin>> {
        self.load_validated(&discovered.path)
    }

    fn load_validated(&self, root: &Path) -> Result<Box<dyn Plugin>> {
        let root = root.canonicalize()?;
        let manifest = crate::package::validate_package(&root)?;
        let activation = crate::activation::Activation::load(self.config.state_path.as_deref())?;
        if activation.disabled.contains(&manifest.plugin.id)
            || !self.config.is_plugin_enabled(&manifest.plugin.id)
        {
            return Err(PluginError::Disabled(manifest.plugin.id.clone()));
        }
        match manifest.runtime.kind {
            crate::contract::RuntimeKind::Node => {
                let trusted = activation
                    .trusted
                    .get(&manifest.plugin.id)
                    .map(String::as_str)
                    .unwrap_or("");
                Ok(Box::new(crate::node::NodePlugin::new(
                    manifest, root, trusted,
                )?))
            }
            crate::contract::RuntimeKind::Wasm => {
                let mut plugin = WasmPlugin::new(manifest, root, self.runtime.clone())?;
                plugin.load()?;
                Ok(Box::new(plugin))
            }
        }
    }

    pub async fn load_from_path(&self, path: &Path) -> Result<Box<dyn Plugin>> {
        let directory = if path.is_dir() {
            path
        } else {
            path.parent()
                .ok_or_else(|| PluginError::load_error("path", "Missing plugin directory"))?
        };
        self.load_validated(directory)
    }

    /// Get the configuration.
    pub fn config(&self) -> &PluginConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_discover_empty_paths() {
        let config = PluginConfig {
            search_paths: vec![PathBuf::from("/nonexistent/path")],
            ..Default::default()
        };
        let runtime = Arc::new(WasmRuntime::new().unwrap());
        let loader = PluginLoader::new(config, runtime);

        let plugins = loader.discover().await;
        assert!(plugins.is_empty());
    }
}
