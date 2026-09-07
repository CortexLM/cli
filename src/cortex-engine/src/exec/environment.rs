//! The final child environment is filtered after combining inherited/overrides.
use std::collections::HashMap;

pub fn is_sensitive_env_name(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    [
        "KEY",
        "SECRET",
        "TOKEN",
        "PASSWORD",
        "CREDENTIAL",
        "PRIVATE",
        "COOKIE",
        "AUTH",
    ]
    .iter()
    .any(|part| name.contains(part))
}

fn is_safe_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    !is_sensitive_env_name(&upper)
        && !upper.starts_with("LD_")
        && !upper.starts_with("DYLD_")
        && !upper.starts_with("CORTEX_SANDBOX")
        && !matches!(
            upper.as_str(),
            "BASH_ENV"
                | "ENV"
                | "SHELLOPTS"
                | "BASHOPTS"
                | "CDPATH"
                | "NODE_OPTIONS"
                | "PYTHONSTARTUP"
                | "RUBYOPT"
                | "PERL5OPT"
                | "GIT_CONFIG"
                | "GIT_CONFIG_COUNT"
                | "GIT_CONFIG_PARAMETERS"
                | "HTTP_PROXY"
                | "HTTPS_PROXY"
                | "ALL_PROXY"
        )
}

pub fn build_safe_environment(overrides: &HashMap<String, String>) -> HashMap<String, String> {
    let inherited = std::env::vars().filter(|(key, _)| {
        matches!(
            key.as_str(),
            "PATH"
                | "HOME"
                | "USER"
                | "LOGNAME"
                | "SHELL"
                | "LANG"
                | "LC_ALL"
                | "TZ"
                | "TMPDIR"
                | "TEMP"
                | "TMP"
                | "SYSTEMROOT"
                | "SystemRoot"
                | "COMSPEC"
                | "PATHEXT"
        )
    });
    filter_environment(inherited, overrides)
}

pub(crate) fn filter_environment(
    inherited: impl IntoIterator<Item = (String, String)>,
    overrides: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut env: HashMap<_, _> = inherited
        .into_iter()
        .chain(overrides.iter().map(|(k, v)| (k.clone(), v.clone())))
        .filter(|(key, _)| is_safe_name(key))
        .collect();
    // Noninteractive settings cannot be undone by a full-context override.
    for (key, value) in [
        ("CI", "true"),
        ("DEBIAN_FRONTEND", "noninteractive"),
        ("NPM_CONFIG_YES", "true"),
        ("NO_COLOR", "1"),
        ("TERM", "dumb"),
    ] {
        env.insert(key.into(), value.into());
    }
    env
}
