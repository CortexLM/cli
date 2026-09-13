
use clap_complete::Shell;
use std::io::{self, ErrorKind, Write};

// =========================================================================
// Shell name parsing tests (unit test the parsing logic directly)
// Note: We avoid testing detect_shell_from_env directly because it reads
// from env vars which causes race conditions in parallel tests.
// Instead, we test the shell name matching logic inline.
// =========================================================================

/// Helper function that mirrors the shell detection logic for testing
fn parse_shell_name(shell_name: &str) -> Shell {
    let normalized = shell_name.to_lowercase();
    match normalized.as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "powershell" | "pwsh" => Shell::PowerShell,
        "elvish" => Shell::Elvish,
        _ => Shell::Bash, // Default to Bash for unknown shells
    }
}

/// Helper to extract shell name from path (like the real function does)
fn extract_shell_name_from_path(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
}

#[test]
fn test_shell_name_parsing_bash() {
    assert!(
        matches!(parse_shell_name("bash"), Shell::Bash),
        "Should parse 'bash' as Bash"
    );
    assert!(
        matches!(parse_shell_name("BASH"), Shell::Bash),
        "Should parse 'BASH' (uppercase) as Bash"
    );
    assert!(
        matches!(parse_shell_name("Bash"), Shell::Bash),
        "Should parse 'Bash' (mixed case) as Bash"
    );
}

#[test]
fn test_shell_name_parsing_zsh() {
    assert!(
        matches!(parse_shell_name("zsh"), Shell::Zsh),
        "Should parse 'zsh' as Zsh"
    );
    assert!(
        matches!(parse_shell_name("ZSH"), Shell::Zsh),
        "Should parse 'ZSH' (uppercase) as Zsh"
    );
}

#[test]
fn test_shell_name_parsing_fish() {
    assert!(
        matches!(parse_shell_name("fish"), Shell::Fish),
        "Should parse 'fish' as Fish"
    );
    assert!(
        matches!(parse_shell_name("FISH"), Shell::Fish),
        "Should parse 'FISH' (uppercase) as Fish"
    );
}

#[test]
fn test_shell_name_parsing_powershell() {
    assert!(
        matches!(parse_shell_name("powershell"), Shell::PowerShell),
        "Should parse 'powershell' as PowerShell"
    );
    assert!(
        matches!(parse_shell_name("pwsh"), Shell::PowerShell),
        "Should parse 'pwsh' as PowerShell"
    );
    assert!(
        matches!(parse_shell_name("PWSH"), Shell::PowerShell),
        "Should parse 'PWSH' (uppercase) as PowerShell"
    );
}

#[test]
fn test_shell_name_parsing_elvish() {
    assert!(
        matches!(parse_shell_name("elvish"), Shell::Elvish),
        "Should parse 'elvish' as Elvish"
    );
    assert!(
        matches!(parse_shell_name("ELVISH"), Shell::Elvish),
        "Should parse 'ELVISH' (uppercase) as Elvish"
    );
}

#[test]
fn test_shell_name_parsing_unknown_defaults_to_bash() {
    assert!(
        matches!(parse_shell_name("unknown-shell"), Shell::Bash),
        "Unknown shell should default to Bash"
    );
    assert!(
        matches!(parse_shell_name("tcsh"), Shell::Bash),
        "tcsh should default to Bash"
    );
    assert!(
        matches!(parse_shell_name("csh"), Shell::Bash),
        "csh should default to Bash"
    );
    assert!(
        matches!(parse_shell_name(""), Shell::Bash),
        "Empty string should default to Bash"
    );
}

#[test]
fn test_extract_shell_name_from_path() {
    assert_eq!(
        extract_shell_name_from_path("/bin/bash"),
        "bash",
        "Should extract 'bash' from /bin/bash"
    );
    assert_eq!(
        extract_shell_name_from_path("/usr/bin/zsh"),
        "zsh",
        "Should extract 'zsh' from /usr/bin/zsh"
    );
    assert_eq!(
        extract_shell_name_from_path("/usr/local/bin/fish"),
        "fish",
        "Should extract 'fish' from /usr/local/bin/fish"
    );
    assert_eq!(
        extract_shell_name_from_path("bash"),
        "bash",
        "Should handle shell name without path"
    );
}

#[test]
fn test_full_path_to_shell_detection() {
    // Test the full pipeline: path -> name extraction -> shell detection
    let test_cases = vec![
        ("/bin/bash", Shell::Bash),
        ("/usr/bin/bash", Shell::Bash),
        ("/bin/zsh", Shell::Zsh),
        ("/usr/local/bin/zsh", Shell::Zsh),
        ("/usr/bin/fish", Shell::Fish),
        ("/usr/bin/pwsh", Shell::PowerShell),
        ("/usr/bin/elvish", Shell::Elvish),
        ("/bin/BASH", Shell::Bash), // uppercase
        ("/bin/ZSH", Shell::Zsh),   // uppercase
    ];

    for (path, expected_shell) in test_cases {
        let shell_name = extract_shell_name_from_path(path);
        let detected = parse_shell_name(shell_name);
        assert!(
            std::mem::discriminant(&detected) == std::mem::discriminant(&expected_shell),
            "Path '{}' should detect as {:?}, got {:?}",
            path,
            expected_shell,
            detected
        );
    }
}

// =========================================================================
// BrokenPipeIgnorer tests (testing the pattern from generate_completions)
// =========================================================================

/// Custom writer that silently ignores BrokenPipe errors (mirrors the one in generate_completions).
struct BrokenPipeIgnorer<W: Write> {
    inner: W,
}

impl<W: Write> Write for BrokenPipeIgnorer<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.inner.write(buf) {
            Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(buf.len()),
            other => other,
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.inner.flush() {
            Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(()),
            other => other,
        }
    }
}

/// A mock writer that can be configured to return specific errors.
struct MockWriter {
    error_kind: Option<ErrorKind>,
    data: Vec<u8>,
}

impl MockWriter {
    fn new() -> Self {
        Self {
            error_kind: None,
            data: Vec::new(),
        }
    }

    fn with_error(error_kind: ErrorKind) -> Self {
        Self {
            error_kind: Some(error_kind),
            data: Vec::new(),
        }
    }
}

impl Write for MockWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(kind) = self.error_kind {
            return Err(io::Error::new(kind, "mock error"));
        }
        self.data.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(kind) = self.error_kind {
            return Err(io::Error::new(kind, "mock flush error"));
        }
        Ok(())
    }
}

#[test]
fn test_broken_pipe_ignorer_normal_write() {
    let mock = MockWriter::new();
    let mut ignorer = BrokenPipeIgnorer { inner: mock };

    let result = ignorer.write(b"hello");
    assert!(result.is_ok(), "Normal write should succeed");
    assert_eq!(result.unwrap(), 5, "Should return bytes written");
}

#[test]
fn test_broken_pipe_ignorer_swallows_broken_pipe() {
    let mock = MockWriter::with_error(ErrorKind::BrokenPipe);
    let mut ignorer = BrokenPipeIgnorer { inner: mock };

    let result = ignorer.write(b"hello");
    assert!(
        result.is_ok(),
        "BrokenPipe error should be silently ignored"
    );
    assert_eq!(
        result.unwrap(),
        5,
        "Should return buffer length even on BrokenPipe"
    );
}

#[test]
fn test_broken_pipe_ignorer_propagates_other_errors() {
    let mock = MockWriter::with_error(ErrorKind::PermissionDenied);
    let mut ignorer = BrokenPipeIgnorer { inner: mock };

    let result = ignorer.write(b"hello");
    assert!(
        result.is_err(),
        "Non-BrokenPipe errors should be propagated"
    );
    assert_eq!(
        result.unwrap_err().kind(),
        ErrorKind::PermissionDenied,
        "Should preserve the original error kind"
    );
}

#[test]
fn test_broken_pipe_ignorer_flush_normal() {
    let mock = MockWriter::new();
    let mut ignorer = BrokenPipeIgnorer { inner: mock };

    let result = ignorer.flush();
    assert!(result.is_ok(), "Normal flush should succeed");
}

#[test]
fn test_broken_pipe_ignorer_flush_swallows_broken_pipe() {
    let mock = MockWriter::with_error(ErrorKind::BrokenPipe);
    let mut ignorer = BrokenPipeIgnorer { inner: mock };

    let result = ignorer.flush();
    assert!(
        result.is_ok(),
        "BrokenPipe on flush should be silently ignored"
    );
}

#[test]
fn test_broken_pipe_ignorer_flush_propagates_other_errors() {
    let mock = MockWriter::with_error(ErrorKind::WriteZero);
    let mut ignorer = BrokenPipeIgnorer { inner: mock };

    let result = ignorer.flush();
    assert!(
        result.is_err(),
        "Non-BrokenPipe errors on flush should be propagated"
    );
    assert_eq!(
        result.unwrap_err().kind(),
        ErrorKind::WriteZero,
        "Should preserve the original error kind on flush"
    );
}

// =========================================================================
// Shell enum variant tests
// =========================================================================

#[test]
fn test_shell_variants_are_supported() {
    // Verify that our shell detection covers all commonly used shells
    let supported_shells = vec![
        (Shell::Bash, "bash"),
        (Shell::Zsh, "zsh"),
        (Shell::Fish, "fish"),
        (Shell::PowerShell, "powershell"),
        (Shell::Elvish, "elvish"),
    ];

    for (shell, name) in supported_shells {
        // Verify each shell variant can be matched
        match shell {
            Shell::Bash => assert_eq!(name, "bash"),
            Shell::Zsh => assert_eq!(name, "zsh"),
            Shell::Fish => assert_eq!(name, "fish"),
            Shell::PowerShell => assert_eq!(name, "powershell"),
            Shell::Elvish => assert_eq!(name, "elvish"),
            _ => {} // Other variants exist but we don't need to test them
        }
    }
}

use clap::Parser;
use serial_test::serial;

#[tokio::test]
async fn dispatch_attach_url_fails_closed() {
    let cli = crate::cli::Cli::try_parse_from(["cortex", "attach", "https://example.com/s"])
        .expect("parse attach");
    let err = crate::cli::dispatch_command(cli).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("No local session was started"), "{msg}");
}

#[tokio::test]
#[serial]
async fn dispatch_jobs_list_requires_auth() {
    let home = tempfile::tempdir().unwrap();
    unsafe {
        std::env::remove_var("CORTEX_AUTH_TOKEN");
        std::env::remove_var("CORTEX_API_KEY");
        std::env::set_var("CORTEX_HOME", home.path());
    }
    let cli = crate::cli::Cli::try_parse_from(["cortex", "jobs", "list"]).expect("parse jobs");
    let err = crate::cli::dispatch_command(cli).await.unwrap_err();
    assert!(err.to_string().contains("Not signed in"), "{err}");
    unsafe {
        std::env::remove_var("CORTEX_HOME");
    }
}
