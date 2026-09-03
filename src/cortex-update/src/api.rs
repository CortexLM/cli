//! Cortex Foundation Software API client.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use cortex_engine::create_client_builder;
use futures::StreamExt;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::SOFTWARE_URL;
use crate::config::ReleaseChannel;
use crate::error::{UpdateError, UpdateResult};

/// Asset information for a specific platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    /// Download URL for this asset
    pub url: String,
    /// SHA256 checksum of the file
    pub sha256: String,
    /// File size in bytes
    pub size: u64,
}

/// Release information from the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    /// Version string (semver)
    pub version: String,
    /// Release channel
    pub channel: ReleaseChannel,
    /// Release timestamp
    pub released_at: DateTime<Utc>,
    /// Minimum supported version for upgrade
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_version: Option<String>,
    /// URL to full changelog
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog_url: Option<String>,
    /// Brief release notes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<String>,
    /// Assets by platform key (e.g., "linux-x86_64", "darwin-aarch64", "windows-x86_64")
    pub assets: HashMap<String, ReleaseAsset>,
    /// Signature URLs by platform key
    #[serde(default)]
    pub signatures: HashMap<String, String>,
}

/// Channel pointer file at `/releases/manifest.json` (and `/v1/releases/manifest.json`).
///
/// `cortex upgrade` prefers this static layout on software.cortex.foundation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseManifest {
    #[serde(default)]
    pub stable: Option<ReleaseInfo>,
    #[serde(default)]
    pub beta: Option<ReleaseInfo>,
    #[serde(default)]
    pub nightly: Option<ReleaseInfo>,
    #[serde(default)]
    pub all_versions: Vec<String>,
}

impl ReleaseManifest {
    /// Return the release published for `channel`, if any.
    pub fn for_channel(&self, channel: ReleaseChannel) -> Option<&ReleaseInfo> {
        match channel {
            ReleaseChannel::Stable => self.stable.as_ref(),
            ReleaseChannel::Beta => self.beta.as_ref(),
            ReleaseChannel::Nightly => self.nightly.as_ref(),
        }
    }
}

impl ReleaseInfo {
    /// Get the asset for the current platform.
    pub fn asset_for_current_platform(&self) -> Option<&ReleaseAsset> {
        let key = platform_key();
        self.assets.get(&key)
    }

    /// Get the signature URL for the current platform.
    pub fn signature_for_current_platform(&self) -> Option<&String> {
        let key = platform_key();
        self.signatures.get(&key)
    }
}

/// Changelog entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    pub version: String,
    pub released_at: DateTime<Utc>,
    pub title: String,
    pub changes: Vec<String>,
}

/// Client for the Cortex Foundation Software Distribution API.
#[derive(Clone)]
pub struct CortexSoftwareClient {
    client: Client,
    base_url: String,
}

impl CortexSoftwareClient {
    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl CortexSoftwareClient {
    /// Create a new client with the default URL.
    pub fn new() -> Self {
        // Default URL is known to be HTTPS, so unwrap is safe
        Self::with_url(SOFTWARE_URL.to_string()).expect("Default SOFTWARE_URL must be HTTPS")
    }

    /// Create a new client with a custom URL.
    ///
    /// # Errors
    ///
    /// Returns `UpdateError::InsecureUrl` if the URL does not use HTTPS
    /// (except for localhost/127.0.0.1 which are allowed for development).
    pub fn with_url(base_url: String) -> UpdateResult<Self> {
        // Validate URL uses HTTPS for security (allow http for localhost development only)
        let url_lower = base_url.to_lowercase();
        if !url_lower.starts_with("https://")
            && !url_lower.starts_with("http://localhost")
            && !url_lower.starts_with("http://127.0.0.1")
        {
            return Err(UpdateError::InsecureUrl { url: base_url });
        }

        let client = create_client_builder()
            .build()
            .unwrap_or_else(|_| Client::new());

        Ok(Self { client, base_url })
    }

    /// GET JSON from a path under `base_url`. Paths may include a query string.
    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> UpdateResult<T> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let response =
            self.client
                .get(&url)
                .send()
                .await
                .map_err(|e| UpdateError::ConnectionFailed {
                    message: e.to_string(),
                })?;

        let status = response.status();
        if status.as_u16() == 404 {
            return Err(UpdateError::ServerError {
                status: 404,
                message: format!("Not found: {url}"),
            });
        }
        if !status.is_success() {
            let status_code = status.as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(UpdateError::ServerError {
                status: status_code,
                message,
            });
        }

        Ok(response.json().await?)
    }

    fn is_missing(err: &UpdateError) -> bool {
        matches!(
            err,
            UpdateError::ServerError { status: 404, .. } | UpdateError::Json(_)
        )
    }

    /// Get the latest release for a channel.
    ///
    /// Tries `/v1/releases/latest` first (query string is ignored by static
    /// object storage), then the channel pointer in `releases/manifest.json`.
    pub async fn get_latest(&self, channel: ReleaseChannel) -> UpdateResult<ReleaseInfo> {
        let latest_paths = [
            format!("/v1/releases/latest.json?channel={}", channel.as_str()),
            format!("/v1/releases/latest?channel={}", channel.as_str()),
        ];
        for path in &latest_paths {
            match self.get_json::<ReleaseInfo>(path).await {
                Ok(info) if info.channel == channel => return Ok(info),
                Ok(_) => continue,
                Err(e) if Self::is_missing(&e) => continue,
                Err(e) => return Err(e),
            }
        }

        for path in ["/v1/releases/manifest.json", "/releases/manifest.json"] {
            match self.get_json::<ReleaseManifest>(path).await {
                Ok(manifest) => {
                    return manifest.for_channel(channel).cloned().ok_or_else(|| {
                        UpdateError::ServerError {
                            status: 404,
                            message: format!(
                                "No {} releases available yet. Check {} or try again later.",
                                channel.as_str(),
                                crate::SOFTWARE_URL
                            ),
                        }
                    });
                }
                Err(e) if Self::is_missing(&e) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(UpdateError::ServerError {
            status: 404,
            message: format!(
                "No releases available yet. Check {} or try again later.",
                crate::SOFTWARE_URL
            ),
        })
    }

    /// Get a specific release by version.
    pub async fn get_release(&self, version: &str) -> UpdateResult<ReleaseInfo> {
        let version = version.trim_start_matches('v');
        let paths = [
            format!("/v1/releases/{version}.json"),
            format!("/v1/releases/{version}"),
            format!("/releases/{version}.json"),
        ];
        for path in &paths {
            match self.get_json::<ReleaseInfo>(path).await {
                Ok(info) => return Ok(info),
                Err(e) if Self::is_missing(&e) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(UpdateError::VersionNotFound {
            version: version.to_string(),
        })
    }

    /// Get changelog entries since a version.
    pub async fn get_changelog(&self, since: &str) -> UpdateResult<Vec<ChangelogEntry>> {
        let url = format!("{}/v1/changelog?since={}", self.base_url, since);

        let response =
            self.client
                .get(&url)
                .send()
                .await
                .map_err(|e| UpdateError::ConnectionFailed {
                    message: e.to_string(),
                })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(UpdateError::ServerError { status, message });
        }

        let entries: Vec<ChangelogEntry> = response.json().await?;
        Ok(entries)
    }

    /// Download an asset to a destination path with progress reporting.
    pub async fn download<F>(
        &self,
        asset: &ReleaseAsset,
        dest: &Path,
        mut on_progress: F,
    ) -> UpdateResult<()>
    where
        F: FnMut(u64, u64), // (downloaded, total)
    {
        // Validate asset URL uses HTTPS (allow localhost for development)
        let url_lower = asset.url.to_lowercase();
        if !url_lower.starts_with("https://")
            && !url_lower.starts_with("http://localhost")
            && !url_lower.starts_with("http://127.0.0.1")
        {
            return Err(UpdateError::InsecureUrl {
                url: asset.url.clone(),
            });
        }

        let response =
            self.client
                .get(&asset.url)
                .send()
                .await
                .map_err(|e| UpdateError::DownloadFailed {
                    message: e.to_string(),
                })?;

        if !response.status().is_success() {
            return Err(UpdateError::DownloadFailed {
                message: format!("HTTP {}", response.status()),
            });
        }

        let total_size = asset.size;
        let mut downloaded: u64 = 0;

        let mut file = tokio::fs::File::create(dest).await?;
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| UpdateError::DownloadFailed {
                message: e.to_string(),
            })?;

            file.write_all(&chunk).await?;

            downloaded += chunk.len() as u64;
            on_progress(downloaded, total_size);
        }

        file.flush().await?;

        Ok(())
    }

    /// Download a signature file.
    pub async fn download_signature(&self, url: &str) -> UpdateResult<Vec<u8>> {
        let response =
            self.client
                .get(url)
                .send()
                .await
                .map_err(|e| UpdateError::DownloadFailed {
                    message: e.to_string(),
                })?;

        if !response.status().is_success() {
            return Err(UpdateError::DownloadFailed {
                message: format!("Failed to download signature: HTTP {}", response.status()),
            });
        }

        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }
}

impl Default for CortexSoftwareClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the platform key for the current system.
pub fn platform_key() -> String {
    let os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "unknown"
    };

    format!("{}-{}", os, arch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_sh_verifies_checksum_against_software_host() {
        let script = include_str!("../../../scripts/install.sh");
        assert!(script.contains("verify_sha256"));
        assert!(script.contains("https://software.cortex.foundation"));
        assert!(script.contains("$HOME/.local"));
    }

    #[test]
    fn test_release_manifest_fixture_parses() {
        let fixture = include_str!("../tests/fixtures/manifest.json");
        let manifest: ReleaseManifest =
            serde_json::from_str(fixture).expect("fixture must deserialize");
        let stable = manifest
            .for_channel(ReleaseChannel::Stable)
            .expect("stable channel");
        assert_eq!(stable.version, "0.1.2");
        assert_eq!(stable.channel, ReleaseChannel::Stable);
        let linux = stable
            .assets
            .get("linux-x86_64")
            .expect("linux-x86_64 asset");
        assert_eq!(
            linux.url,
            "https://software.cortex.foundation/v1/assets/linux-x86_64/0.1.2/cortex.tar.gz"
        );
        assert_eq!(linux.sha256.len(), 64);
        assert!(manifest.all_versions.contains(&"0.1.2".to_string()));
        assert!(manifest.for_channel(ReleaseChannel::Beta).is_none());
    }

    #[tokio::test]
    async fn get_latest_reads_manifest_when_latest_is_missing() {
        let fixture = include_str!("../tests/fixtures/manifest.json");
        let server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/v1/releases/latest.json"))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/v1/releases/latest"))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/v1/releases/manifest.json"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_raw(fixture, "application/json"),
            )
            .mount(&server)
            .await;

        let client = CortexSoftwareClient::with_url(server.uri()).expect("localhost http");
        let latest = client
            .get_latest(ReleaseChannel::Stable)
            .await
            .expect("manifest fallback");
        assert_eq!(latest.version, "0.1.2");
        assert!(latest.assets.contains_key("linux-x86_64"));
        assert_eq!(latest.assets.len(), 5);
    }

    #[tokio::test]
    async fn get_release_reads_version_json() {
        let fixture = include_str!("../tests/fixtures/release.json");
        let server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/v1/releases/0.1.2.json"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_raw(fixture, "application/json"),
            )
            .mount(&server)
            .await;

        let client = CortexSoftwareClient::with_url(server.uri()).expect("localhost http");
        let release = client.get_release("v0.1.2").await.expect("version json");
        assert_eq!(release.version, "0.1.2");
        assert_eq!(
            release
                .assets
                .get("darwin-aarch64")
                .expect("darwin asset")
                .size,
            12345678
        );
    }
}
