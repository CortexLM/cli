//! OAuth 2.0 with PKCE support for MCP servers.
//!
//! Provides OAuth 2.0 authentication flow with PKCE (Proof Key for Code Exchange)
//! for secure authorization with remote MCP servers.

use std::collections::HashMap;
use std::path::PathBuf;

use base64::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs;
use tracing::info;

use crate::error::{CortexError, Result};

/// OAuth callback port for local redirect URI.
pub const OAUTH_CALLBACK_PORT: u16 = 19876;

/// OAuth callback path.
pub const OAUTH_CALLBACK_PATH: &str = "/mcp/oauth/callback";

/// OAuth tokens.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthTokens {
    /// Access token for API requests.
    pub access_token: String,
    /// Refresh token for obtaining new access tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Unix timestamp when the token expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// OAuth scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// OAuth client information (for dynamic registration).
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthClientInfo {
    /// Client ID.
    pub client_id: String,
    /// Client secret (if confidential client).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Unix timestamp when client ID was issued.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id_issued_at: Option<i64>,
    /// Unix timestamp when client secret expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret_expires_at: Option<i64>,
}

/// OAuth entry for a single MCP server.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthEntry {
    /// OAuth tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<OAuthTokens>,
    /// Client information (from dynamic registration).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_info: Option<OAuthClientInfo>,
    /// PKCE code verifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_verifier: Option<String>,
    /// OAuth state parameter for CSRF protection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_state: Option<String>,
    /// Server URL these credentials are for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
}

/// OAuth storage - manages persistent storage of OAuth credentials.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct OAuthStorage {
    /// Entries keyed by MCP server name.
    #[serde(flatten)]
    entries: HashMap<String, OAuthEntry>,
}

impl OAuthStorage {
    /// Get the path to the OAuth storage file.
    fn file_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| CortexError::config("Could not determine home directory"))?;
        Ok(home.join(".cortex").join("mcp-auth.json"))
    }

    /// Load credentials from the OS credential store. Migrate a legacy owner-only file once.
    pub async fn load() -> Result<Self> {
        let entry = keyring::Entry::new("cortex-mcp", "oauth-storage")
            .map_err(|_| CortexError::Auth("MCP credential store is unavailable".into()))?;
        match entry.get_password() {
            Ok(content) => serde_json::from_str(&content)
                .map_err(|_| CortexError::Auth("MCP credential store is invalid".into())),
            Err(keyring::Error::NoEntry) => {
                let path = Self::file_path()?;
                if !path.exists() {
                    return Ok(Self::default());
                }
                let metadata = fs::symlink_metadata(&path).await?;
                if !metadata.is_file() || metadata.len() > 1024 * 1024 {
                    return Err(CortexError::Auth(
                        "Legacy MCP credential storage is unsafe".into(),
                    ));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if metadata.permissions().mode() & 0o077 != 0 {
                        return Err(CortexError::Auth(
                            "Legacy MCP credential permissions must be owner-only".into(),
                        ));
                    }
                }
                let content = fs::read_to_string(&path).await?;
                let storage: Self = serde_json::from_str(&content)
                    .map_err(|_| CortexError::Auth("Legacy MCP credentials are invalid".into()))?;
                storage.save().await?;
                fs::remove_file(path).await?;
                Ok(storage)
            }
            Err(_) => Err(CortexError::Auth(
                "MCP credential store is unavailable".into(),
            )),
        }
    }

    /// Tokens never return to plaintext configuration files.
    pub async fn save(&self) -> Result<()> {
        let content = serde_json::to_string(self)
            .map_err(|_| CortexError::Auth("MCP credentials could not be encoded".into()))?;
        keyring::Entry::new("cortex-mcp", "oauth-storage")
            .and_then(|entry| entry.set_password(&content))
            .map_err(|_| CortexError::Auth("MCP credentials could not be saved securely".into()))
    }

    /// Get entry for an MCP server.
    pub fn get(&self, mcp_name: &str) -> Option<&OAuthEntry> {
        self.entries.get(mcp_name)
    }

    /// Get entry for an MCP server, validating it's for the correct URL.
    pub fn get_for_url(&self, mcp_name: &str, server_url: &str) -> Option<&OAuthEntry> {
        let entry = self.entries.get(mcp_name)?;

        // If no server URL stored, credentials are from old version - invalid
        let stored_url = entry.server_url.as_ref()?;

        // If URL changed, credentials are invalid
        if stored_url != server_url {
            return None;
        }

        Some(entry)
    }

    /// Set entry for an MCP server.
    pub fn set(&mut self, mcp_name: impl Into<String>, entry: OAuthEntry) {
        self.entries.insert(mcp_name.into(), entry);
    }

    /// Remove entry for an MCP server.
    pub fn remove(&mut self, mcp_name: &str) -> Option<OAuthEntry> {
        self.entries.remove(mcp_name)
    }

    /// Get all entries.
    pub fn all(&self) -> &HashMap<String, OAuthEntry> {
        &self.entries
    }

    /// Check if tokens are expired.
    pub fn is_token_expired(&self, mcp_name: &str) -> Option<bool> {
        let entry = self.entries.get(mcp_name)?;
        let tokens = entry.tokens.as_ref()?;
        let expires_at = tokens.expires_at?;

        let now = chrono::Utc::now().timestamp();
        Some(expires_at < now)
    }

    /// Update tokens for an MCP server.
    pub fn update_tokens(&mut self, mcp_name: &str, tokens: OAuthTokens, server_url: Option<&str>) {
        let entry = self.entries.entry(mcp_name.to_string()).or_default();
        entry.tokens = Some(tokens);
        if let Some(url) = server_url {
            entry.server_url = Some(url.to_string());
        }
    }

    /// Update client info for an MCP server.
    pub fn update_client_info(
        &mut self,
        mcp_name: &str,
        client_info: OAuthClientInfo,
        server_url: Option<&str>,
    ) {
        let entry = self.entries.entry(mcp_name.to_string()).or_default();
        entry.client_info = Some(client_info);
        if let Some(url) = server_url {
            entry.server_url = Some(url.to_string());
        }
    }

    /// Update code verifier for an MCP server.
    pub fn update_code_verifier(&mut self, mcp_name: &str, code_verifier: String) {
        let entry = self.entries.entry(mcp_name.to_string()).or_default();
        entry.code_verifier = Some(code_verifier);
    }

    /// Clear code verifier for an MCP server.
    pub fn clear_code_verifier(&mut self, mcp_name: &str) {
        if let Some(entry) = self.entries.get_mut(mcp_name) {
            entry.code_verifier = None;
        }
    }

    /// Update OAuth state for an MCP server.
    pub fn update_oauth_state(&mut self, mcp_name: &str, state: String) {
        let entry = self.entries.entry(mcp_name.to_string()).or_default();
        entry.oauth_state = Some(state);
    }

    /// Get OAuth state for an MCP server.
    pub fn get_oauth_state(&self, mcp_name: &str) -> Option<&str> {
        self.entries.get(mcp_name)?.oauth_state.as_deref()
    }

    /// Clear OAuth state for an MCP server.
    pub fn clear_oauth_state(&mut self, mcp_name: &str) {
        if let Some(entry) = self.entries.get_mut(mcp_name) {
            entry.oauth_state = None;
        }
    }
}

/// PKCE helper for OAuth 2.0 with Proof Key for Code Exchange.
pub struct Pkce;

impl Pkce {
    /// Generate a cryptographically random code verifier.
    ///
    /// The code verifier is a high-entropy cryptographic random string
    /// using unreserved characters [A-Z] / [a-z] / [0-9] / "-" / "." / "_" / "~",
    /// with a minimum length of 43 and maximum of 128 characters.
    pub fn generate_code_verifier() -> String {
        const CHARSET: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
        const LENGTH: usize = 64;

        let mut rng = rand::thread_rng();
        (0..LENGTH)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// Generate the code challenge from a code verifier using S256 method.
    ///
    /// code_challenge = BASE64URL(SHA256(code_verifier))
    pub fn generate_code_challenge(code_verifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();

        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash)
    }

    /// Generate a cryptographically random state parameter for CSRF protection.
    pub fn generate_state() -> String {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill(&mut bytes);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    }
}

/// OAuth configuration for an MCP server.
#[derive(Clone)]
pub struct OAuthConfig {
    /// Pre-registered client ID.
    pub client_id: Option<String>,
    /// Pre-registered client secret.
    pub client_secret: Option<String>,
    /// OAuth scope to request.
    pub scope: Option<String>,
}

impl Default for OAuthConfig {
    fn default() -> Self {
        Self {
            client_id: None,
            client_secret: None,
            scope: None,
        }
    }
}

/// OAuth client metadata for dynamic registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClientMetadata {
    /// Redirect URIs.
    pub redirect_uris: Vec<String>,
    /// Client name.
    pub client_name: String,
    /// Client URI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_uri: Option<String>,
    /// Grant types.
    pub grant_types: Vec<String>,
    /// Response types.
    pub response_types: Vec<String>,
    /// Token endpoint auth method.
    pub token_endpoint_auth_method: String,
}

impl Default for OAuthClientMetadata {
    fn default() -> Self {
        Self {
            redirect_uris: vec![format!(
                "http://127.0.0.1:{OAUTH_CALLBACK_PORT}{OAUTH_CALLBACK_PATH}"
            )],
            client_name: "Cortex".to_string(),
            client_uri: None,
            grant_types: vec![
                "authorization_code".to_string(),
                "refresh_token".to_string(),
            ],
            response_types: vec!["code".to_string()],
            token_endpoint_auth_method: "none".to_string(),
        }
    }
}

impl OAuthClientMetadata {
    /// Create metadata for a confidential client (with client secret).
    pub fn with_client_secret(mut self) -> Self {
        self.token_endpoint_auth_method = "client_secret_post".to_string();
        self
    }
}

/// OAuth authorization server metadata (RFC 8414).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthServerMetadata {
    /// Authorization endpoint.
    pub authorization_endpoint: String,
    /// Token endpoint.
    pub token_endpoint: String,
    /// Client registration endpoint (for dynamic registration).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_endpoint: Option<String>,
    /// Supported scopes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes_supported: Option<Vec<String>>,
    /// Supported response types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_types_supported: Option<Vec<String>>,
    /// Supported grant types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_types_supported: Option<Vec<String>>,
    /// Supported code challenge methods.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_challenge_methods_supported: Option<Vec<String>>,
}

/// Token response from OAuth token endpoint.
#[derive(Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// Access token.
    pub access_token: String,
    /// Token type (usually "Bearer").
    pub token_type: String,
    /// Expires in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<i64>,
    /// Refresh token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl From<TokenResponse> for OAuthTokens {
    fn from(response: TokenResponse) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            access_token: response.access_token,
            refresh_token: response.refresh_token,
            expires_at: response.expires_in.map(|exp| now + exp),
            scope: response.scope,
        }
    }
}

/// OAuth flow manager for handling the OAuth 2.0 authorization code flow.
pub struct OAuthFlow {
    /// MCP server name.
    mcp_name: String,
    /// Server URL.
    server_url: String,
    /// OAuth configuration.
    config: OAuthConfig,
    /// HTTP client.
    client: reqwest::Client,
}

impl OAuthFlow {
    /// Create a new OAuth flow.
    pub fn new(mcp_name: impl Into<String>, server_url: impl Into<String>) -> Self {
        Self {
            mcp_name: mcp_name.into(),
            server_url: server_url.into(),
            config: OAuthConfig::default(),
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("OAuth HTTP client"),
        }
    }

    fn validate_metadata(&self, metadata: &OAuthServerMetadata) -> Result<()> {
        let base = super::http::validate_endpoint(&self.server_url)
            .map_err(|_| CortexError::Auth("Invalid MCP OAuth origin".into()))?;
        for raw in [&metadata.authorization_endpoint, &metadata.token_endpoint] {
            let endpoint = super::http::validate_endpoint(raw)
                .map_err(|_| CortexError::Auth("Invalid MCP OAuth endpoint".into()))?;
            if endpoint.origin() != base.origin() {
                return Err(CortexError::Auth("An additional OAuth host requires explicit configuration; cross-origin discovery is unsupported".into()));
            }
        }
        Ok(())
    }

    /// Set OAuth configuration.
    pub fn with_config(mut self, config: OAuthConfig) -> Self {
        self.config = config;
        self
    }

    /// Get the redirect URL.
    pub fn redirect_url() -> String {
        format!("http://127.0.0.1:{OAUTH_CALLBACK_PORT}{OAUTH_CALLBACK_PATH}")
    }

    /// Discover OAuth server metadata.
    pub async fn discover_metadata(&self) -> Result<OAuthServerMetadata> {
        // Try well-known OAuth metadata endpoint
        let base_url = url::Url::parse(&self.server_url)
            .map_err(|e| CortexError::config(format!("Invalid server URL: {e}")))?;

        let well_known_url = base_url
            .join("/.well-known/oauth-authorization-server")
            .map_err(|e| CortexError::config(format!("Failed to construct well-known URL: {e}")))?;

        let response = self.client.get(well_known_url.as_str()).send().await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                let metadata: OAuthServerMetadata = resp
                    .json()
                    .await
                    .map_err(|_| CortexError::config("MCP authentication metadata is invalid"))?;
                Ok(metadata)
            }
            _ => {
                // Fallback: try OpenID Connect discovery
                let oidc_url = base_url
                    .join("/.well-known/openid-configuration")
                    .map_err(|e| {
                        CortexError::config(format!("Failed to construct OIDC URL: {e}"))
                    })?;

                let response = self
                    .client
                    .get(oidc_url.as_str())
                    .send()
                    .await
                    .map_err(|_| CortexError::config("MCP authentication discovery failed"))?;

                if response.status().is_success() {
                    let metadata: OAuthServerMetadata = response.json().await.map_err(|_| {
                        CortexError::config("MCP authentication metadata is invalid")
                    })?;
                    Ok(metadata)
                } else {
                    Err(CortexError::config(format!(
                        "OAuth metadata not found at server: {}",
                        self.server_url
                    )))
                }
            }
        }
    }

    /// Build the authorization URL for the OAuth flow.
    pub async fn build_authorization_url(
        &self,
        metadata: &OAuthServerMetadata,
        storage: &mut OAuthStorage,
    ) -> Result<String> {
        self.validate_metadata(metadata)?;
        // Generate PKCE values
        let code_verifier = Pkce::generate_code_verifier();
        let code_challenge = Pkce::generate_code_challenge(&code_verifier);
        let state = Pkce::generate_state();

        // Store PKCE values
        storage.update_code_verifier(&self.mcp_name, code_verifier);
        storage.update_oauth_state(&self.mcp_name, state.clone());
        storage.save().await?;

        // Get client ID
        let client_id = if let Some(id) = &self.config.client_id {
            id.clone()
        } else if let Some(entry) = storage.get_for_url(&self.mcp_name, &self.server_url) {
            if let Some(info) = &entry.client_info {
                info.client_id.clone()
            } else {
                return Err(CortexError::Auth(
                    "No client ID available. Dynamic registration may be required.".into(),
                ));
            }
        } else {
            return Err(CortexError::Auth(
                "No client ID configured. Add clientId to your MCP server config.".into(),
            ));
        };

        // Build authorization URL
        let mut auth_url = url::Url::parse(&metadata.authorization_endpoint)
            .map_err(|e| CortexError::config(format!("Invalid authorization endpoint: {e}")))?;

        auth_url
            .query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &client_id)
            .append_pair("redirect_uri", &Self::redirect_url())
            .append_pair("state", &state)
            .append_pair("code_challenge", &code_challenge)
            .append_pair("code_challenge_method", "S256");

        if let Some(scope) = &self.config.scope {
            auth_url.query_pairs_mut().append_pair("scope", scope);
        }

        info!(
            mcp_name = %self.mcp_name,
            "Built authorization URL"
        );

        Ok(auth_url.to_string())
    }

    /// Exchange authorization code for tokens.
    pub async fn exchange_code(
        &self,
        metadata: &OAuthServerMetadata,
        code: &str,
        storage: &mut OAuthStorage,
    ) -> Result<OAuthTokens> {
        self.validate_metadata(metadata)?;
        // Get stored code verifier
        let code_verifier = storage
            .get(&self.mcp_name)
            .and_then(|e| e.code_verifier.as_ref())
            .ok_or_else(|| CortexError::Auth("No code verifier found".into()))?
            .clone();

        // Get client ID
        let client_id = if let Some(id) = &self.config.client_id {
            id.clone()
        } else if let Some(entry) = storage.get_for_url(&self.mcp_name, &self.server_url) {
            if let Some(info) = &entry.client_info {
                info.client_id.clone()
            } else {
                return Err(CortexError::Auth("No client ID available".into()));
            }
        } else {
            return Err(CortexError::Auth("No client ID configured".into()));
        };

        // Build token request
        let mut params = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("redirect_uri", Self::redirect_url()),
            ("client_id", client_id),
            ("code_verifier", code_verifier),
        ];

        // Add client secret if available
        if let Some(secret) = &self.config.client_secret {
            params.push(("client_secret", secret.clone()));
        } else if let Some(entry) = storage.get_for_url(&self.mcp_name, &self.server_url) {
            if let Some(info) = &entry.client_info {
                if let Some(secret) = &info.client_secret {
                    params.push(("client_secret", secret.clone()));
                }
            }
        }

        // Make token request
        let response = self
            .client
            .post(&metadata.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|_| CortexError::Auth("MCP authentication request failed".into()))?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(CortexError::Auth(format!(
                "MCP authentication failed (status {status})"
            )));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|_| CortexError::Auth("MCP authentication response is invalid".into()))?;

        let tokens = OAuthTokens::from(token_response);

        if let Some(client_id) = &self.config.client_id {
            storage.update_client_info(
                &self.mcp_name,
                OAuthClientInfo {
                    client_id: client_id.clone(),
                    client_secret: self.config.client_secret.clone(),
                    client_id_issued_at: None,
                    client_secret_expires_at: None,
                },
                Some(&self.server_url),
            );
        }
        // Store tokens
        storage.update_tokens(&self.mcp_name, tokens.clone(), Some(&self.server_url));
        storage.clear_code_verifier(&self.mcp_name);
        storage.clear_oauth_state(&self.mcp_name);
        storage.save().await?;

        info!(mcp_name = %self.mcp_name, "Token exchange successful");
        Ok(tokens)
    }

    /// Refresh an expired access token.
    pub async fn refresh_tokens(
        &self,
        metadata: &OAuthServerMetadata,
        storage: &mut OAuthStorage,
    ) -> Result<OAuthTokens> {
        self.validate_metadata(metadata)?;
        // Get stored refresh token
        let refresh_token = storage
            .get(&self.mcp_name)
            .and_then(|e| e.tokens.as_ref())
            .and_then(|t| t.refresh_token.as_ref())
            .ok_or_else(|| CortexError::Auth("No refresh token available".into()))?
            .clone();

        // Get client ID
        let client_id = if let Some(id) = &self.config.client_id {
            id.clone()
        } else if let Some(entry) = storage.get_for_url(&self.mcp_name, &self.server_url) {
            if let Some(info) = &entry.client_info {
                info.client_id.clone()
            } else {
                return Err(CortexError::Auth("No client ID available".into()));
            }
        } else {
            return Err(CortexError::Auth("No client ID configured".into()));
        };

        // Build refresh request
        let mut params = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.clone()),
            ("client_id", client_id),
        ];

        // Add client secret if available
        if let Some(secret) = &self.config.client_secret {
            params.push(("client_secret", secret.clone()));
        } else if let Some(entry) = storage.get_for_url(&self.mcp_name, &self.server_url) {
            if let Some(info) = &entry.client_info {
                if let Some(secret) = &info.client_secret {
                    params.push(("client_secret", secret.clone()));
                }
            }
        }

        // Make refresh request
        let response = self
            .client
            .post(&metadata.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|_| CortexError::Auth("MCP authentication refresh failed".into()))?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(CortexError::Auth(format!(
                "MCP authentication refresh failed (status {status})"
            )));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|_| CortexError::Auth("MCP authentication response is invalid".into()))?;

        let mut tokens = OAuthTokens::from(token_response);
        if tokens.refresh_token.is_none() {
            tokens.refresh_token = Some(refresh_token);
        }
        if tokens.scope.is_none() {
            tokens.scope = storage
                .get_for_url(&self.mcp_name, &self.server_url)
                .and_then(|e| e.tokens.as_ref())
                .and_then(|t| t.scope.clone());
        }

        // Store new tokens
        storage.update_tokens(&self.mcp_name, tokens.clone(), Some(&self.server_url));
        storage.save().await?;

        info!(mcp_name = %self.mcp_name, "Token refresh successful");
        Ok(tokens)
    }
}

/// OAuth authentication status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStatus {
    /// Successfully authenticated.
    Authenticated,
    /// Token has expired.
    Expired,
    /// Not authenticated.
    NotAuthenticated,
}

/// Get authentication status for an MCP server.
pub async fn get_auth_status(mcp_name: &str, server_url: &str) -> Result<AuthStatus> {
    let storage = OAuthStorage::load().await?;

    let entry = match storage.get_for_url(mcp_name, server_url) {
        Some(e) => e,
        None => return Ok(AuthStatus::NotAuthenticated),
    };

    let tokens = match &entry.tokens {
        Some(t) => t,
        None => return Ok(AuthStatus::NotAuthenticated),
    };

    // Check if expired
    if let Some(expires_at) = tokens.expires_at {
        let now = chrono::Utc::now().timestamp();
        if expires_at < now {
            return Ok(AuthStatus::Expired);
        }
    }

    Ok(AuthStatus::Authenticated)
}

/// Check if an MCP server has stored OAuth tokens.
pub async fn has_stored_tokens(mcp_name: &str) -> Result<bool> {
    let storage = OAuthStorage::load().await?;
    Ok(storage
        .get(mcp_name)
        .and_then(|e| e.tokens.as_ref())
        .is_some())
}

/// Serialize runtime refresh and logout so an in-flight refresh cannot restore logged-out tokens.
static CREDENTIAL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Runtime authorization never launches a browser or follows an unconfigured host.
pub async fn stored_access_token(mcp_name: &str, server_url: &str) -> Result<Option<String>> {
    let _guard = CREDENTIAL_LOCK.lock().await;
    let mut storage = OAuthStorage::load().await?;
    access_token_from_storage(mcp_name, server_url, &mut storage).await
}

async fn access_token_from_storage(
    mcp_name: &str,
    server_url: &str,
    storage: &mut OAuthStorage,
) -> Result<Option<String>> {
    let Some(entry) = storage.get_for_url(mcp_name, server_url) else {
        return Ok(None);
    };
    let Some(tokens) = &entry.tokens else {
        return Ok(None);
    };
    if !tokens
        .expires_at
        .is_some_and(|expiry| expiry <= chrono::Utc::now().timestamp() + 30)
    {
        return Ok(Some(tokens.access_token.clone()));
    }
    if tokens.refresh_token.is_none() {
        return Err(CortexError::Auth(
            "MCP login expired; run cortex mcp auth".into(),
        ));
    }
    let flow = OAuthFlow::new(mcp_name, server_url);
    let metadata = flow.discover_metadata().await?;
    let tokens = flow.refresh_tokens(&metadata, storage).await?;
    Ok(Some(tokens.access_token))
}

/// Remove OAuth credentials for an MCP server.
pub async fn remove_auth(mcp_name: &str) -> Result<()> {
    let _guard = CREDENTIAL_LOCK.lock().await;
    let mut storage = OAuthStorage::load().await?;
    storage.remove(mcp_name);
    storage.save().await?;
    info!(mcp_name = %mcp_name, "Removed OAuth credentials");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runtime_tokens_are_url_bound_expire_and_disappear_after_logout() {
        let mut storage = OAuthStorage::default();
        storage.update_tokens(
            "fixture",
            OAuthTokens {
                access_token: "synthetic-access".into(),
                refresh_token: None,
                expires_at: None,
                scope: None,
            },
            Some("https://configured.example/rpc"),
        );
        assert_eq!(
            access_token_from_storage("fixture", "https://configured.example/rpc", &mut storage)
                .await
                .unwrap()
                .as_deref(),
            Some("synthetic-access")
        );
        assert!(
            access_token_from_storage("fixture", "https://other.example/rpc", &mut storage)
                .await
                .unwrap()
                .is_none()
        );
        assert!(!format!("{storage:?}").contains("synthetic-access"));
        storage
            .entries
            .get_mut("fixture")
            .unwrap()
            .tokens
            .as_mut()
            .unwrap()
            .expires_at = Some(1);
        assert!(
            access_token_from_storage("fixture", "https://configured.example/rpc", &mut storage)
                .await
                .is_err()
        );
        storage.remove("fixture");
        assert!(
            access_token_from_storage("fixture", "https://configured.example/rpc", &mut storage)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn oauth_metadata_cannot_redirect_tokens_to_another_origin() {
        let flow = OAuthFlow::new("fixture", "https://configured.example/rpc");
        let metadata = OAuthServerMetadata {
            authorization_endpoint: "https://configured.example/authorize".into(),
            token_endpoint: "https://unconfigured.example/token".into(),
            registration_endpoint: None,
            scopes_supported: None,
            response_types_supported: None,
            grant_types_supported: None,
            code_challenge_methods_supported: None,
        };
        assert!(flow.validate_metadata(&metadata).is_err());
    }

    #[test]
    fn test_pkce_code_verifier() {
        let verifier = Pkce::generate_code_verifier();
        assert_eq!(verifier.len(), 64);
        assert!(verifier.chars().all(|c| c.is_ascii_alphanumeric()
            || c == '-'
            || c == '.'
            || c == '_'
            || c == '~'));
    }

    #[test]
    fn test_pkce_code_challenge() {
        // Test vector from RFC 7636
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = Pkce::generate_code_challenge(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn test_pkce_state() {
        let state = Pkce::generate_state();
        assert!(!state.is_empty());
        // State should be URL-safe base64
        assert!(
            state
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }

    #[test]
    fn test_oauth_tokens_from_response() {
        let response = TokenResponse {
            access_token: "test_access".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            refresh_token: Some("test_refresh".to_string()),
            scope: Some("read write".to_string()),
        };

        let tokens = OAuthTokens::from(response);
        assert_eq!(tokens.access_token, "test_access");
        assert_eq!(tokens.refresh_token, Some("test_refresh".to_string()));
        assert!(tokens.expires_at.is_some());
    }
}

macro_rules! redacted_debug {
    ($($name:ty),+ $(,)?) => {$ (
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(stringify!($name), " { [redacted] }"))
            }
        }
    )+};
}
redacted_debug!(
    OAuthTokens,
    OAuthClientInfo,
    OAuthEntry,
    OAuthStorage,
    OAuthConfig,
    TokenResponse
);
