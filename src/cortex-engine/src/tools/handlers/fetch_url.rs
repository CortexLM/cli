//! Destination-validated fetch with DNS pinning, no ambient proxies and bounded bodies.
use super::{ToolContext, ToolHandler, ToolResult};
use crate::error::Result;
use crate::security::ssrf::{SsrfConfig, SsrfProtection};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

pub struct FetchUrlHandler {
    ssrf_protection: SsrfProtection,
    #[cfg(test)]
    test_destination: Option<(String, SocketAddr)>,
}
#[derive(Deserialize)]
struct FetchUrlArgs {
    url: String,
    #[serde(default)]
    allowed_domains: Option<Vec<String>>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    format: Option<String>,
}
impl FetchUrlHandler {
    pub fn new() -> Self {
        Self::with_config(SsrfConfig::default())
    }
    pub fn with_allowed_domains(domains: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::with_config(SsrfConfig::new().allow_domains(domains))
    }
    pub fn with_config(config: SsrfConfig) -> Self {
        Self {
            ssrf_protection: SsrfProtection::with_config(config),
            #[cfg(test)]
            test_destination: None,
        }
    }
    fn validate_url_with_allowlist(
        &self,
        url: &str,
        domains: Option<&[String]>,
    ) -> std::result::Result<url::Url, String> {
        // Validation is pure here. Resolved addresses are checked and pinned at send.
        let mut config = self.ssrf_protection.config().clone();
        config.resolve_dns_first = false;
        let url = SsrfProtection::with_config(config)
            .validate_url(url)
            .map_err(|e| e.to_string())?;
        if let Some(domains) = domains {
            let host = url.host_str().ok_or("Missing URL host")?;
            if !domains.iter().any(|d| d.eq_ignore_ascii_case(host)) {
                return Err("Domain not allowed for this request".into());
            }
        }
        Ok(url)
    }
    async fn destination(&self, url: &url::Url) -> std::result::Result<Vec<SocketAddr>, String> {
        let host = url.host_str().ok_or("Missing URL host")?;
        let port = url.port_or_known_default().ok_or("Missing URL port")?;
        #[cfg(test)]
        if let Some((expected, address)) = &self.test_destination {
            if host == expected {
                return Ok(vec![*address]);
            }
        }
        let addresses = match url.host() {
            Some(url::Host::Ipv4(ip)) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
            Some(url::Host::Ipv6(ip)) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
            _ => tokio::net::lookup_host((host, port))
                .await
                .map_err(|_| "Destination lookup failed")?
                .collect(),
        };
        self.validate_destinations(&addresses)?;
        Ok(addresses)
    }
    fn validate_destinations(&self, addresses: &[SocketAddr]) -> std::result::Result<(), String> {
        if addresses.is_empty()
            || addresses
                .iter()
                .any(|a| self.ssrf_protection.is_blocked_ip(a.ip()))
        {
            return Err("Destination resolves to a blocked address".into());
        }
        Ok(())
    }
    async fn fetch(
        &self,
        args: &FetchUrlArgs,
        context: &ToolContext,
    ) -> std::result::Result<String, String> {
        let config = self.ssrf_protection.config();
        let mut url =
            self.validate_url_with_allowlist(&args.url, args.allowed_domains.as_deref())?;
        for hop in 0..=config.max_redirects.min(10) {
            let host = url.host_str().ok_or("Missing URL host")?;
            if !context
                .allowed_fetch_hosts()
                .iter()
                .any(|h| h.eq_ignore_ascii_case(host))
            {
                return Err("Destination host is not explicitly allowed".into());
            }
            let addresses = self.destination(&url).await?;
            // This client cannot re-resolve a checked hostname, use a proxy, or
            // automatically redirect to an unchecked private/metadata endpoint.
            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .resolve_to_addrs(host, &addresses)
                .timeout(Duration::from_secs(config.timeout_secs.min(60)))
                .connect_timeout(Duration::from_secs(config.connect_timeout_secs.min(10)))
                .no_gzip()
                .no_brotli()
                .no_deflate()
                .no_zstd()
                .build()
                .map_err(|_| "Unable to initialize web fetch")?;
            let mut response = client
                .get(url.clone())
                .send()
                .await
                .map_err(|_| "Web fetch is temporarily unavailable")?;
            if response.status().is_redirection() {
                if !config.allow_redirects || hop == config.max_redirects.min(10) {
                    return Err("Redirect limit exceeded".into());
                }
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|h| h.to_str().ok())
                    .ok_or("Redirect has no valid destination")?;
                let target = url
                    .join(location)
                    .map_err(|_| "Invalid redirect destination")?;
                if url.scheme() == "https" && target.scheme() != "https" {
                    return Err("Insecure redirect rejected".into());
                }
                url = self.validate_url_with_allowlist(
                    target.as_str(),
                    args.allowed_domains.as_deref(),
                )?;
                continue;
            }
            if !response.status().is_success() {
                return Err(format!(
                    "Web fetch returned HTTP {}",
                    response.status().as_u16()
                ));
            }
            return read_body(
                &mut response,
                config.max_response_size.min(10 * 1024 * 1024),
            )
            .await;
        }
        Err("Redirect limit exceeded".into())
    }
}

async fn read_body(
    response: &mut reqwest::Response,
    limit: usize,
) -> std::result::Result<String, String> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("Response exceeds size limit".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Unable to read web response")?
    {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err("Response exceeds size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| "Web response is not UTF-8 text".into())
}

#[cfg(test)]
#[path = "fetch_url_security_boundary.rs"]
mod security_boundary;
impl Default for FetchUrlHandler {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait]
impl ToolHandler for FetchUrlHandler {
    fn name(&self) -> &str {
        "FetchUrl"
    }
    async fn execute(&self, arguments: Value, context: &ToolContext) -> Result<ToolResult> {
        let args: FetchUrlArgs = serde_json::from_value(arguments)?;
        if !context.sandbox_policy.has_full_network_access() {
            return Ok(ToolResult::error(
                "Network access is disabled by the execution policy",
            ));
        }
        if !matches!(
            args.format.as_deref().unwrap_or("text"),
            "text" | "html" | "markdown"
        ) {
            return Ok(ToolResult::error("Unsupported fetch format"));
        }
        let deadline = Duration::from_secs(args.timeout.unwrap_or(30).clamp(1, 60));
        let result = tokio::time::timeout(deadline, self.fetch(&args, context)).await;
        match result {
            Ok(Ok(text)) => {
                let text = if args.format.as_deref() == Some("html") {
                    text
                } else {
                    html_text(&text)
                };
                let mut text = context.redact(&text);
                if text.len() > 100_000 {
                    let mut end = 100_000;
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    text.truncate(end);
                    text.push_str("\n[Content truncated at 100000 bytes]");
                }
                Ok(ToolResult::success(text))
            }
            Ok(Err(message)) => Ok(ToolResult::error(context.redact(&message))),
            Err(_) => Ok(ToolResult::error("Web fetch timed out")),
        }
    }
}
fn html_text(text: &str) -> String {
    // Plain responses are preserved. Markup is converted to conservative text;
    // no script execution, external resources, or additional network requests.
    if !text.trim_start().starts_with('<') {
        return text.into();
    }
    let mut out = String::new();
    let mut in_tag = false;
    for character in text.chars() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_creation() {
        let handler = FetchUrlHandler::new();
        assert_eq!(handler.name(), "FetchUrl");
    }

    #[test]
    fn test_blocked_urls() {
        let handler = FetchUrlHandler::new();

        // Localhost
        assert!(
            handler
                .validate_url_with_allowlist("http://localhost", None)
                .is_err()
        );
        assert!(
            handler
                .validate_url_with_allowlist("http://127.0.0.1", None)
                .is_err()
        );
        assert!(
            handler
                .validate_url_with_allowlist("http://[::1]", None)
                .is_err()
        );

        // Private IPs
        assert!(
            handler
                .validate_url_with_allowlist("http://10.0.0.1", None)
                .is_err()
        );
        assert!(
            handler
                .validate_url_with_allowlist("http://172.16.0.1", None)
                .is_err()
        );
        assert!(
            handler
                .validate_url_with_allowlist("http://192.168.1.1", None)
                .is_err()
        );

        // Link-local
        assert!(
            handler
                .validate_url_with_allowlist("http://169.254.169.254", None)
                .is_err()
        );

        // Local domains
        assert!(
            handler
                .validate_url_with_allowlist("http://server.local", None)
                .is_err()
        );
        assert!(
            handler
                .validate_url_with_allowlist("http://app.internal", None)
                .is_err()
        );

        // Blocked protocols
        assert!(
            handler
                .validate_url_with_allowlist("file:///etc/passwd", None)
                .is_err()
        );
        assert!(
            handler
                .validate_url_with_allowlist("ftp://example.com", None)
                .is_err()
        );
    }

    #[test]
    fn test_allowed_urls() {
        // Skip DNS resolution in tests (no network access)
        let config = SsrfConfig::new().skip_dns_resolution();
        let handler = FetchUrlHandler::with_config(config);

        assert!(
            handler
                .validate_url_with_allowlist("https://example.com", None)
                .is_ok()
        );
        assert!(
            handler
                .validate_url_with_allowlist("https://api.github.com", None)
                .is_ok()
        );
        assert!(
            handler
                .validate_url_with_allowlist("http://rust-lang.org", None)
                .is_ok()
        );
    }

    #[test]
    fn test_domain_allowlist() {
        // Skip DNS resolution in tests (no network access)
        let config = SsrfConfig::new()
            .allow_domain("example.com")
            .allow_domain("api.github.com")
            .skip_dns_resolution();
        let handler = FetchUrlHandler::with_config(config);

        // Allowed (exact match required)
        assert!(
            handler
                .validate_url_with_allowlist("https://example.com", None)
                .is_ok()
        );
        assert!(
            handler
                .validate_url_with_allowlist("https://api.github.com", None)
                .is_ok()
        );

        // Not in allowlist (subdomains not automatically included)
        assert!(
            handler
                .validate_url_with_allowlist("https://other.com", None)
                .is_err()
        );
    }

    #[test]
    fn test_per_request_allowlist() {
        // Skip DNS resolution in tests (no network access)
        let config = SsrfConfig::new().skip_dns_resolution();
        let handler = FetchUrlHandler::with_config(config);
        let allowed = vec!["specific.com".to_string()];

        // With per-request allowlist
        assert!(
            handler
                .validate_url_with_allowlist("https://specific.com", Some(&allowed))
                .is_ok()
        );
        assert!(
            handler
                .validate_url_with_allowlist("https://other.com", Some(&allowed))
                .is_err()
        );
    }
}
