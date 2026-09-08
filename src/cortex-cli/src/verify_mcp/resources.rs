//! MCP resources for the verification server.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use tokio::sync::Mutex;

use cortex_mcp_server::ResourceProvider;
use cortex_mcp_types::{Resource, ResourceContent};
use cortex_tui::lock_proof::lock_scene_ids;
use cortex_tui::lock_v2::{LOCK_V2_NARROW_IDS, LOCK_V2_WIDE_IDS};

use super::lock::workspace_root;
use super::state::VerifyState;

pub struct VerifyResources {
    pub state: Arc<Mutex<VerifyState>>,
}

#[async_trait::async_trait]
impl ResourceProvider for VerifyResources {
    async fn list(&self) -> Result<Vec<Resource>> {
        let mut resources = vec![
            Resource::new("cortex-verify://matrix", "Verification state matrix"),
            Resource::new(
                "cortex-verify://report/latest",
                "Last cortex-verify/1 report",
            ),
        ];
        let root = workspace_root().join("docs/media/tui-lock-v2/txt");
        for (size, ids) in [("40x12", LOCK_V2_NARROW_IDS), ("120x40", LOCK_V2_WIDE_IDS)] {
            for id in ids.iter() {
                let uri = format!("cortex-verify://lock/v2/{size}/{id}.txt");
                if root.join(size).join(format!("{id}.txt")).exists() {
                    resources.push(Resource::new(uri, format!("Designer grid {id} {size}")));
                }
            }
        }
        Ok(resources)
    }

    async fn read(&self, uri: &str) -> Result<ResourceContent> {
        if uri == "cortex-verify://matrix" {
            let matrix = serde_json::json!({
                "v1": lock_scene_ids(),
                "v2_narrow": LOCK_V2_NARROW_IDS,
                "v2_wide": LOCK_V2_WIDE_IDS,
                "sizes": [[40, 12], [120, 40]],
            });
            return Ok(ResourceContent::text(uri, matrix.to_string()));
        }
        if uri == "cortex-verify://report/latest" {
            let state = self.state.lock().await;
            let body = state
                .last_report
                .as_ref()
                .map(|v| serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".into()))
                .unwrap_or_else(|| "{}".into());
            return Ok(ResourceContent::text(uri, body));
        }
        if let Some(rest) = uri.strip_prefix("cortex-verify://lock/v2/") {
            let path = workspace_root()
                .join("docs/media/tui-lock-v2/txt")
                .join(rest);
            let text =
                std::fs::read_to_string(&path).map_err(|_| anyhow!("Resource not found: {uri}"))?;
            return Ok(ResourceContent::text(uri, text));
        }
        Err(anyhow!("Resource not found: {uri}"))
    }
}
