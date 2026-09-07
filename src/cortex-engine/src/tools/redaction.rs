//! User/model egress redaction, including structured and multiline secret forms.
use once_cell::sync::Lazy;
use regex::Regex;

static HTTP_CREDENTIAL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
    r"(?im)^\s*(?:authorization|proxy-authorization|cookie|set-cookie)\s*:\s*[^\r\n]+|\b(?:bearer|basic)\s+[a-z0-9._~+/-]+=*"
).expect("fixed credential header expression")
});
static STRUCTURED_SECRET: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
    r#"(?i)["']?(?:api[_-]?key|secret|password|token|authorization|cookie)["']?\s*[:=]\s*(?:"[^"]*"|'[^']*'|[^\s,;]+)"#
).expect("fixed structured redaction expression")
});
static PRIVATE_KEY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*")
        .expect("fixed private key redaction expression")
});

pub(crate) fn redact(text: &str) -> String {
    let text = HTTP_CREDENTIAL.replace_all(text, "[REDACTED]");
    let text = STRUCTURED_SECRET.replace_all(&text, "[REDACTED]");
    let text = PRIVATE_KEY.replace_all(&text, "[REDACTED]");
    crate::harness::redact_secrets(&text)
}
