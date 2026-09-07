use super::*;

#[test]
fn startup_clears_visible_screen_in_both_modes() {
    let mut output = Vec::new();
    init_screen(&mut output, &TerminalOptions::default()).unwrap();
    let startup = String::from_utf8(output).unwrap();
    assert!(startup.find("\x1b[?1049h").unwrap() < startup.find("\x1b[2J").unwrap());
    assert!(startup.contains("\x1b[0m\x1b[2J\x1b[1;1H"));
    assert!(!startup.contains("\x1b[3J"), "never erase scrollback");

    let mut output = Vec::new();
    init_screen(&mut output, &TerminalOptions::inline()).unwrap();
    let startup = String::from_utf8(output).unwrap();
    assert!(startup.contains("\x1b[0m\x1b[2J\x1b[1;1H"));
    assert!(!startup.contains("\x1b[3J"));
}

#[test]
fn explicit_clear_on_start_false_does_not_clear_screen() {
    for options in [TerminalOptions::default(), TerminalOptions::inline()] {
        let mut output = Vec::new();
        init_screen(&mut output, &options.clear_on_start(false)).unwrap();
        let startup = String::from_utf8(output).unwrap();
        assert!(!startup.contains("\x1b[2J"));
        assert!(!startup.contains("\x1b[3J"));
    }
}

#[test]
fn inline_preflight_options_preserve_scrollback_without_switching_screens() {
    use crate::runner::AppRunner;
    use cortex_engine::Config;

    let config = Config {
        alternate_screen: false,
        ..Config::default()
    };
    for runner in [
        AppRunner::new(config),
        AppRunner::new(Config::default()).inline(),
    ] {
        let options = runner.terminal_options.clone();
        assert!(!options.alternate_screen);
        assert!(options.clear_on_start);
        let mut output = Vec::new();
        init_screen(&mut output, &options).unwrap();
        restore_screen(
            &mut output,
            options.alternate_screen,
            options.mouse_capture,
            options.bracketed_paste,
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        // CortexTerminal also clears the visible viewport after construction.
        // 2J is allowed; switching screens and purging history are not.
        for forbidden in ["\x1b[?1049h", "\x1b[?1049l", "\x1b[3J"] {
            assert!(!output.contains(forbidden), "unexpected {forbidden:?}");
        }
    }
}

#[test]
fn cleanup_attempts_remaining_steps_after_write_error() {
    struct FailOnce {
        failed: bool,
        output: Vec<u8>,
    }
    impl io::Write for FailOnce {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.failed {
                self.failed = true;
                return Err(io::Error::other("injected output failure"));
            }
            self.output.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = FailOnce {
        failed: false,
        output: Vec::new(),
    };
    assert!(restore_screen(&mut writer, true, true, true).is_err());
    let output = String::from_utf8(writer.output).unwrap();
    assert!(output.contains("\x1b[?25h"));
    assert!(output.contains("\x1b[?2004l"));
    assert!(output.contains("\x1b[?1000l"));
    assert!(output.ends_with("\x1b[?1049l"));
    assert!(!output.contains("\x1b[2J"));
    assert!(!output.contains("\x1b[3J"));
}

#[test]
fn test_terminal_options_default() {
    let options = TerminalOptions::default();
    assert!(
        options.alternate_screen,
        "default must enter the alternate screen (always)"
    );
    assert!(options.mouse_capture);
    assert!(options.bracketed_paste);
    assert_eq!(options.title, Some("Cortex".to_string()));
    assert!(options.clear_on_start);
}

#[test]
fn test_terminal_options_builder() {
    let options = TerminalOptions::new()
        .alternate_screen(false)
        .mouse_capture(false)
        .bracketed_paste(false)
        .title("Test")
        .clear_on_start(false);

    assert!(!options.alternate_screen);
    assert!(!options.mouse_capture);
    assert!(!options.bracketed_paste);
    assert_eq!(options.title, Some("Test".to_string()));
    assert!(!options.clear_on_start);
}

#[test]
fn test_terminal_options_inline() {
    let options = TerminalOptions::inline();
    assert!(!options.alternate_screen);
    assert!(options.mouse_capture);
    assert!(options.bracketed_paste);
    assert!(options.title.is_none());
    assert!(options.clear_on_start);
}

#[test]
fn test_terminal_guard_creation() {
    let guard = TerminalGuard::new(true, true, true, true);
    assert!(guard.alternate_screen);
    assert!(guard.mouse_capture);
    assert!(guard.bracketed_paste);
    assert!(guard.restore_title);
}

// Note: Tests that actually create terminals are difficult to run
// in CI environments as they require a real TTY. These would be
// integration tests run manually or in a special test environment.
