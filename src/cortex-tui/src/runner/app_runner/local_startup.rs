//! Resolve local startup identity before workspace trust or terminal setup.
use super::AppRunner;
use crate::session::SessionStorage;
use anyhow::Result;

impl AppRunner {
    pub(super) fn load_or_create_local_session(
        storage: SessionStorage,
        provider: &str,
        model: &str,
        session_id: Option<&str>,
    ) -> Result<crate::session::CortexSession> {
        match session_id {
            Some(id) => crate::session::CortexSession::load_with_storage(id, storage),
            None => crate::session::CortexSession::with_storage(provider, model, storage),
        }
    }

    /// The local session store for the configured Cortex home.
    pub(super) fn local_store(&self) -> SessionStorage {
        SessionStorage::with_dir(self.config.cortex_home.join("sessions"))
    }

    pub(super) fn prepare_local_store(&mut self) -> Result<SessionStorage> {
        let storage = self.local_store();
        if let Some(id) = &self.cortex_session_id {
            let resolved = storage.resolve_id(id)?;
            let meta = storage.load_meta(&resolved)?;
            // Validate the full history before prompting for workspace trust.
            storage.load_messages(&resolved)?;
            self.config.cwd = meta.cwd.into();
            self.config.model = meta.model;
            self.cortex_session_id = Some(resolved);
        }
        Ok(storage)
    }
}

// Kept separate so session startup does not grow the inherited runner's complexity.
pub(super) fn build_unified_executor(
    provider_manager: &crate::providers::ProviderManager,
    provider: &str,
    model: &str,
) -> Option<std::sync::Arc<cortex_engine::tools::UnifiedToolExecutor>> {
    use cortex_engine::tools::{ExecutorConfig, UnifiedToolExecutor};
    use std::sync::Arc;

    // Get auth token using the centralized auth module
    // This properly handles: instance token → env var → keyring
    // Previous bug: only checked CORTEX_AUTH_TOKEN env var, missing keyring auth
    let api_key = cortex_engine::auth_token::get_auth_token(None).ok();
    let base_url = provider_manager.config().get_base_url(provider);

    match api_key {
        Some(api_key) if !api_key.is_empty() => {
            tracing::debug!(
                "Using API key for UnifiedToolExecutor (length: {})",
                api_key.len()
            );
            let mut config = ExecutorConfig::new(provider, model, &api_key)
                .with_working_dir(std::env::current_dir().unwrap_or_default());

            // Add base URL if configured
            if let Some(url) = base_url {
                config = config.with_base_url(url);
            }

            match UnifiedToolExecutor::new(config) {
                Ok(executor) => {
                    tracing::info!(
                        "UnifiedToolExecutor initialized - Task and Batch tools enabled"
                    );
                    Some(Arc::new(executor))
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to create UnifiedToolExecutor: {} - Task/Batch will use fallback",
                        e
                    );
                    None
                }
            }
        }
        Some(_) => {
            // Empty API key - treat same as None
            tracing::warn!(
                "Empty API key for provider '{}' - Task/Batch tools will use fallback",
                provider
            );
            None
        }
        None => {
            tracing::warn!(
                "No API key configured for provider '{}' - Task/Batch tools will use fallback",
                provider
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_engine::rollout::local::{SessionMeta, StoredMessage};

    #[test]
    fn ux_contract_startup_resolves_exact_identity_model_and_workspace() {
        let home = tempfile::tempdir().unwrap();
        let store = SessionStorage::with_dir(home.path().join("sessions"));
        let mut meta = SessionMeta::new("cortex", "saved-model");
        meta.cwd = home.path().display().to_string();
        store
            .create_session(&meta, &[StoredMessage::user("saved history")])
            .unwrap();
        let config = cortex_engine::Config {
            cortex_home: home.path().to_path_buf(),
            ..Default::default()
        };
        let mut runner = AppRunner::new(config)
            .with_cortex_session_id(meta.short_id())
            .with_initial_prompt("next turn");
        let loaded = runner.prepare_local_store().unwrap();
        assert_eq!(runner.cortex_session_id.as_deref(), Some(meta.id.as_str()));
        assert_eq!(runner.config.model, "saved-model");
        assert_eq!(runner.config.cwd, home.path());
        assert_eq!(runner.initial_prompt.as_deref(), Some("next turn"));
        assert_eq!(
            loaded.load_messages(&meta.id).unwrap()[0].content,
            "saved history"
        );
        assert_eq!(loaded.list_sessions().unwrap().len(), 1);
    }
}
