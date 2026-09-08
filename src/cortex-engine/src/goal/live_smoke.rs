//! Live openai-compatible smoke for `/goal` against Astra.
//!
//! Skips cleanly when no key is injected. Never prints key values.

use super::super::config::llm_env::{llm_api_key, llm_base_url, llm_model};

/// Human-readable skip when the operator has not injected a key.
pub const LIVE_SMOKE_SKIP: &str = "SKIP: live /goal smoke needs OPENAI_API_KEY or CORTEX_LLM_API_KEY \
(operator injects from ~/.cortex-private/judge.key). Unit tests still pass. \
Set CORTEX_LLM_MODEL=cx/gpt-6-astra (not gpt-astra) and OPENAI_BASE_URL or \
CORTEX_LLM_BASE_URL to the Astra /v1 endpoint.";

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::print_stderr)]
    #[tokio::test]
    async fn live_chat_completions_or_skip() {
        let Some(key) = llm_api_key() else {
            eprintln!("{LIVE_SMOKE_SKIP}");
            return;
        };

        let base = llm_base_url();
        let model = llm_model();
        assert_ne!(
            model, "gpt-astra",
            "gpt-astra 404s; use cx/gpt-6-astra (CORTEX_LLM_MODEL)"
        );

        let url = format!("{}/chat/completions", base.trim_end_matches('/'));
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(45))
            .build()
            .expect("http client");

        let response = client
            .post(&url)
            .bearer_auth(&key)
            .json(&serde_json::json!({
                "model": model,
                "messages": [
                    {"role": "user", "content": "Reply with the single word pong."}
                ],
                "max_tokens": 16
            }))
            .send()
            .await
            .expect("live request should reach the configured /v1 endpoint");

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        assert!(
            status.is_success(),
            "live /goal smoke HTTP {} from {url} (model {model}). Body length {}.",
            status.as_u16(),
            body.len()
        );
        assert!(!body.is_empty(), "live /goal smoke returned an empty body");
    }
}
