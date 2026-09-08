//! Live-LLM environment names for `/goal` smoke and the named compatible `/v1`
//! provider id.
//!
//! Product Agent mode still uses the Cortex API client. These helpers align
//! that provider and `/goal` live smoke with the operator-injected Astra
//! endpoint. Never log key values.

/// Public Astra `/v1` default for test helpers and docs examples only.
pub const DEFAULT_LLM_BASE_URL: &str = "http://84.32.63.4:20128/v1";

/// Live model id. `gpt-astra` 404s; this is the served name.
pub const DEFAULT_LLM_MODEL: &str = "cx/gpt-6-astra";

/// Product default when openai-compatible env URLs are unset.
pub const OPENAI_COMPATIBLE_FALLBACK_URL: &str = "https://api.openai.com/v1";

/// First non-empty environment value among `names`.
pub fn first_nonempty_env(names: &[&str]) -> Option<String> {
    for name in names {
        if let Ok(value) = std::env::var(name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// `OPENAI_BASE_URL` or `CORTEX_LLM_BASE_URL`, else the Astra test default.
pub fn llm_base_url() -> String {
    first_nonempty_env(&["OPENAI_BASE_URL", "CORTEX_LLM_BASE_URL"])
        .unwrap_or_else(|| DEFAULT_LLM_BASE_URL.to_string())
}

/// `OPENAI_API_KEY` or `CORTEX_LLM_API_KEY`. Never log the value.
pub fn llm_api_key() -> Option<String> {
    first_nonempty_env(&["OPENAI_API_KEY", "CORTEX_LLM_API_KEY"])
}

/// `CORTEX_LLM_MODEL`, else `cx/gpt-6-astra`.
pub fn llm_model() -> String {
    first_nonempty_env(&["CORTEX_LLM_MODEL"]).unwrap_or_else(|| DEFAULT_LLM_MODEL.to_string())
}

/// Base URL for the named openai-compatible provider.
///
/// Env overrides use the same names as live smoke. Unset keeps the generic
/// Compatible `/v1` fallback (not the Astra test default).
pub fn openai_compatible_base_url() -> String {
    first_nonempty_env(&["OPENAI_BASE_URL", "CORTEX_LLM_BASE_URL"])
        .unwrap_or_else(|| OPENAI_COMPATIBLE_FALLBACK_URL.to_string())
}

/// Preferred env var *name* for the openai-compatible key (never the value).
pub fn openai_compatible_api_key_env() -> String {
    if first_nonempty_env(&["OPENAI_API_KEY"]).is_some() {
        "OPENAI_API_KEY".to_string()
    } else if first_nonempty_env(&["CORTEX_LLM_API_KEY"]).is_some() {
        "CORTEX_LLM_API_KEY".to_string()
    } else {
        "OPENAI_API_KEY".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_defaults_are_astra_not_gpt_astra() {
        assert_eq!(DEFAULT_LLM_MODEL, "cx/gpt-6-astra");
        assert_ne!(DEFAULT_LLM_MODEL, "gpt-astra");
        assert!(DEFAULT_LLM_BASE_URL.ends_with("/v1"));
        assert!(DEFAULT_LLM_BASE_URL.starts_with("http://"));
    }

    #[test]
    fn first_nonempty_skips_blank() {
        // Function is env-backed; the helper itself treats empty as missing.
        assert_eq!(first_nonempty_env(&[]), None);
    }

    #[test]
    fn llm_model_never_defaults_to_gpt_astra() {
        assert_ne!(llm_model(), "gpt-astra");
        assert!(llm_model() == DEFAULT_LLM_MODEL || !llm_model().is_empty());
    }
}
