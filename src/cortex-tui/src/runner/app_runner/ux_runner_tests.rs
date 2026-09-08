use super::*;
use crate::runner::terminal::TerminalOptions;

#[test]
fn test_app_runner_builder() {
    let config = Config::default();

    let runner = AppRunner::new(config.clone())
        .with_initial_prompt("Hello")
        .inline();

    assert_eq!(runner.initial_prompt, Some("Hello".to_string()));
    assert!(!runner.terminal_options.alternate_screen);
}

#[test]
fn test_app_runner_model_provider() {
    let config = Config::default();
    let runner = AppRunner::new(config);

    // Default config values
    assert!(!runner.model().is_empty());
    assert!(!runner.provider().is_empty());
}

#[test]
fn test_app_runner_terminal_options() {
    let config = Config::default();

    // Default: alternate screen (always)
    let runner = AppRunner::new(config.clone());
    assert!(
        runner.terminal_options.alternate_screen,
        "default must enter the alternate screen (always)"
    );
    assert!(runner.terminal_options.clear_on_start);

    let mut inline = config.clone();
    inline.alternate_screen = false;
    let runner = AppRunner::new(inline);
    assert!(!runner.terminal_options.alternate_screen);
    assert!(runner.terminal_options.clear_on_start);

    // Custom options
    let custom_options = TerminalOptions::new()
        .alternate_screen(false)
        .mouse_capture(false);

    let runner = AppRunner::new(config.clone()).with_terminal_options(custom_options);
    assert!(!runner.terminal_options.alternate_screen);
    assert!(!runner.terminal_options.mouse_capture);

    // Inline mode
    let runner = AppRunner::new(config).inline();
    assert!(!runner.terminal_options.alternate_screen);
}

#[test]
fn test_app_runner_direct_provider_mode() {
    let config = Config::default();

    // Default is direct provider mode
    let runner = AppRunner::new(config.clone());
    assert!(runner.use_direct_provider);

    // Can explicitly enable
    let runner = AppRunner::new(config.clone()).direct_provider(true);
    assert!(runner.use_direct_provider);

    // Can switch to legacy mode
    let runner = AppRunner::new(config.clone()).legacy_backend();
    assert!(!runner.use_direct_provider);

    // with_conversation_id switches to legacy
    let id = ConversationId::new();
    let runner = AppRunner::new(config).with_conversation_id(id);
    assert!(!runner.use_direct_provider);
}

#[test]
fn test_app_runner_cortex_session_id() {
    let config = Config::default();

    let runner = AppRunner::new(config).with_cortex_session_id("test-session-123");
    assert_eq!(
        runner.cortex_session_id,
        Some("test-session-123".to_string())
    );
    // Direct provider mode should still be enabled
    assert!(runner.use_direct_provider);
}

#[test]
fn me_profile_request_url_uses_configured_origin_never_production_or_auth_me() {
    let url = super::me_profile_request_url("http://127.0.0.1:18081/");
    assert_eq!(url, "http://127.0.0.1:18081/v1/me");
    assert!(!url.contains("api.cortex.foundation"));
    assert!(!url.contains("/auth/me"));
    assert_eq!(
        super::me_profile_request_url("https://api.cortex.foundation"),
        "https://api.cortex.foundation/v1/me"
    );
}
