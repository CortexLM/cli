//! Authentication and authorization.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config::AuthConfig;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// JWT claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID).
    pub sub: String,
    /// Expiration time (Unix timestamp).
    pub exp: u64,
    /// Issued at (Unix timestamp).
    pub iat: u64,
    /// Issuer.
    pub iss: String,
    /// Audience.
    #[serde(default)]
    pub aud: Vec<String>,
}

impl Claims {
    /// Create new claims for a user.
    pub fn new(user_id: impl Into<String>, expiry_seconds: u64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            sub: user_id.into(),
            exp: now + expiry_seconds,
            iat: now,
            iss: "Cortex".to_string(),
            aud: vec!["cortex-api".to_string()],
        }
    }

    /// Check if the token is expired.
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.exp < now
    }
}

/// Authentication service.
pub struct AuthService {
    /// Configuration.
    config: AuthConfig,
    /// Encoding key for JWT.
    encoding_key: Option<EncodingKey>,
    /// Decoding key for JWT.
    decoding_key: Option<DecodingKey>,
    /// API key hash cache.
    _api_key_hashes: RwLock<HashMap<String, String>>,
    /// Revoked tokens.
    revoked_tokens: RwLock<HashMap<String, u64>>,
}

impl std::fmt::Debug for AuthService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthService")
            .field("enabled", &self.config.enabled)
            .field("encoding_key", &self.encoding_key.is_some())
            .field("decoding_key", &self.decoding_key.is_some())
            .finish()
    }
}

impl AuthService {
    /// Create a new authentication service.
    pub fn new(config: AuthConfig) -> Self {
        let (encoding_key, decoding_key) = config
            .jwt_secret
            .as_ref()
            .map(|secret| {
                (
                    EncodingKey::from_secret(secret.as_bytes()),
                    DecodingKey::from_secret(secret.as_bytes()),
                )
            })
            .map(|(e, d)| (Some(e), Some(d)))
            .unwrap_or((None, None));

        Self {
            config,
            encoding_key,
            decoding_key,
            _api_key_hashes: RwLock::new(HashMap::new()),
            revoked_tokens: RwLock::new(HashMap::new()),
        }
    }

    /// Generate a JWT token for a user.
    pub fn generate_token(&self, user_id: &str) -> AppResult<String> {
        let encoding_key = self
            .encoding_key
            .as_ref()
            .ok_or_else(|| AppError::Internal("JWT secret not configured".to_string()))?;

        let claims = Claims::new(user_id, self.config.jwt_expiry);

        encode(&Header::default(), &claims, encoding_key)
            .map_err(|e| AppError::Internal(format!("Failed to generate token: {e}")))
    }

    /// Validate a JWT token.
    pub fn validate_token(&self, token: &str) -> AppResult<Claims> {
        let decoding_key = self
            .decoding_key
            .as_ref()
            .ok_or_else(|| AppError::Internal("JWT secret not configured".to_string()))?;

        let mut validation = Validation::default();
        validation.set_issuer(&["Cortex"]);
        validation.set_audience(&["cortex-api"]);

        let token_data = decode::<Claims>(token, decoding_key, &validation)
            .map_err(|e| AppError::Authentication(format!("Invalid token: {e}")))?;

        if token_data.claims.is_expired() {
            return Err(AppError::Authentication("Token expired".to_string()));
        }

        Ok(token_data.claims)
    }

    /// Check if a token is revoked.
    pub async fn is_token_revoked(&self, token: &str) -> bool {
        let revoked = self.revoked_tokens.read().await;
        revoked.contains_key(token)
    }

    /// Revoke a token.
    pub async fn revoke_token(&self, token: &str, expiry: u64) {
        let mut revoked = self.revoked_tokens.write().await;
        revoked.insert(token.to_string(), expiry);
    }

    /// Clean up expired revoked tokens.
    pub async fn cleanup_revoked_tokens(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut revoked = self.revoked_tokens.write().await;
        revoked.retain(|_, expiry| *expiry > now);
    }

    /// Validate an API key.
    pub fn validate_api_key(&self, api_key: &str) -> bool {
        self.config.api_keys.iter().any(|k| k == api_key)
    }

    /// Hash an API key for storage.
    pub fn hash_api_key(api_key: &str) -> String {
        // Reuse the installed SHA-256/base64url primitive; never persist a raw API key.
        cortex_engine::mcp::Pkce::generate_code_challenge(api_key)
    }
}

/// Authentication result.
#[derive(Debug, Clone)]
pub enum AuthResult {
    /// Authenticated with JWT.
    Jwt(Claims),
    /// Authenticated with API key.
    ApiKey(String),
    /// Anonymous access.
    Anonymous,
}

impl AuthResult {
    /// Get the user ID if authenticated.
    pub fn user_id(&self) -> Option<&str> {
        match self {
            Self::Jwt(claims) => Some(&claims.sub),
            Self::ApiKey(key) => Some(key),
            Self::Anonymous => None,
        }
    }

    /// Check if the user is authenticated.
    pub fn is_authenticated(&self) -> bool {
        !matches!(self, Self::Anonymous)
    }
}

/// Namespaced session ownership; JWT subjects cannot impersonate API-key principals.
pub fn session_principal(auth: Option<&AuthResult>, enabled: bool) -> AppResult<Option<String>> {
    match auth {
        Some(AuthResult::Jwt(claims)) => Ok(Some(format!("jwt:{}", claims.sub))),
        Some(AuthResult::ApiKey(id)) => Ok(Some(format!("api:{id}"))),
        _ if !enabled => Ok(None),
        _ => Err(AppError::Authentication("Authentication required".into())),
    }
}

/// Extract authorization from request headers.
pub fn extract_auth_header(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(std::string::ToString::to_string)
}

/// Parse Bearer token from Authorization header.
pub fn parse_bearer_token(auth_header: &str) -> Option<&str> {
    auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "))
}

/// Parse API key from Authorization header.
pub fn parse_api_key(auth_header: &str) -> Option<&str> {
    auth_header
        .strip_prefix("ApiKey ")
        .or_else(|| auth_header.strip_prefix("apikey "))
}

/// Authentication middleware with proper JWT validation.
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip auth if disabled
    if !state.config.auth.enabled {
        return Ok(next.run(request).await);
    }

    // Check if path is in anonymous list
    let path = request.uri().path();
    if state
        .config
        .auth
        .anonymous_endpoints
        .iter()
        .any(|p| path == p)
    {
        return Ok(next.run(request).await);
    }

    // Try to authenticate
    let auth_header = extract_auth_header(request.headers());

    let auth_result = match auth_header.as_deref() {
        Some(header) if header.starts_with("Bearer ") || header.starts_with("bearer ") => {
            let token = parse_bearer_token(header).ok_or(StatusCode::UNAUTHORIZED)?;

            // Validate JWT token properly
            let decoding_key = state
                .config
                .auth
                .jwt_secret
                .as_ref()
                .map(|secret| DecodingKey::from_secret(secret.as_bytes()))
                .ok_or(StatusCode::UNAUTHORIZED)?;

            // Configure validation
            let mut validation = Validation::default();
            validation.set_issuer(&["Cortex"]);
            validation.set_audience(&["cortex-api"]);
            validation.validate_exp = true;
            validation.validate_nbf = false;

            // Decode and validate the token
            let token_data = decode::<Claims>(token, &decoding_key, &validation).map_err(|e| {
                tracing::warn!("JWT validation failed: {}", e);
                StatusCode::UNAUTHORIZED
            })?;

            let claims = token_data.claims;

            // Check if token is expired (extra safety check)
            if claims.is_expired() {
                tracing::warn!("JWT token expired");
                return Err(StatusCode::UNAUTHORIZED);
            }

            AuthResult::Jwt(claims)
        }
        Some(header) if header.starts_with("ApiKey ") || header.starts_with("apikey ") => {
            let api_key = parse_api_key(header).ok_or(StatusCode::UNAUTHORIZED)?;

            // Validate API key against configured keys
            // Use constant-time comparison to prevent timing attacks
            let key_index = state
                .config
                .auth
                .api_keys
                .iter()
                .position(|configured_key| {
                    constant_time_compare(api_key.as_bytes(), configured_key.as_bytes())
                });

            if key_index.is_some() {
                AuthResult::ApiKey(format!("sha256:{}", AuthService::hash_api_key(api_key)))
            } else {
                tracing::warn!("Invalid API key attempted");
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
        _ => {
            tracing::debug!("No valid authorization header provided");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Add auth result to request extensions
    request.extensions_mut().insert(auth_result);

    Ok(next.run(request).await)
}

/// Constant-time string comparison to prevent timing attacks.
fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claims_creation() {
        let claims = Claims::new("user123", 3600);
        assert_eq!(claims.sub, "user123");
        assert!(!claims.is_expired());
    }

    #[test]
    fn test_parse_bearer_token() {
        assert_eq!(parse_bearer_token("Bearer abc123"), Some("abc123"));
        assert_eq!(parse_bearer_token("bearer abc123"), Some("abc123"));
        assert_eq!(parse_bearer_token("ApiKey abc123"), None);
    }

    #[test]
    fn test_parse_api_key() {
        assert_eq!(parse_api_key("ApiKey abc123"), Some("abc123"));
        assert_eq!(parse_api_key("apikey abc123"), Some("abc123"));
        assert_eq!(parse_api_key("Bearer abc123"), None);
    }
}
