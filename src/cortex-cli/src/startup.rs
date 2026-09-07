//! Process startup without overwriting or cleaning up another session's files.

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

/// Check an existing home using an exclusively created, owned temporary file.
pub fn check_cortex_home_writable(path: &Path) -> Result<()> {
    if !path.try_exists().context("Cannot inspect CORTEX_HOME")? {
        return Ok(());
    }
    let mut probe = tempfile::Builder::new()
        .prefix(".cortex-write-check-")
        .tempfile_in(path)
        .context("Cannot write to CORTEX_HOME; choose a writable directory")?;
    probe
        .write_all(b"check")
        .context("Cannot write to CORTEX_HOME")?;
    probe
        .close()
        .context("Cannot remove the owned write check")?;
    Ok(())
}

/// `--debug` opts into bounded local diagnostics, never raw trace transcripts.
pub fn initialize_diagnostics(debug: bool) -> Result<()> {
    let result = if debug && std::env::var_os("CORTEX_DIAGNOSTICS_DIR").is_none() {
        let home = cortex_common::get_cortex_home().context("Cannot locate CORTEX_HOME")?;
        cortex_common::diagnostics::init(&home.join("diagnostics"))
    } else {
        cortex_common::diagnostics::init_from_env()
    };
    result.map_err(|_| anyhow::anyhow!("Local diagnostics could not be initialized"))?;
    if debug {
        eprintln!("Local diagnostics enabled. Prompts and tool output are not recorded.");
    }
    Ok(())
}

/// Effective execution policy from interactive flags. Unknown values are
/// errors, never silently downgraded to a more permissive default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractivePolicy {
    pub sandbox_mode: Option<cortex_engine::config::SandboxMode>,
    pub approval_policy: Option<cortex_protocol::AskForApproval>,
}

pub fn interactive_policy(
    sandbox: Option<&str>,
    approval: Option<&str>,
    full_auto: bool,
    bypass_all: bool,
) -> Result<InteractivePolicy> {
    use cortex_engine::config::SandboxMode;
    use cortex_protocol::AskForApproval;

    let sandbox_mode = match sandbox.map(str::trim) {
        None => None,
        Some("read-only") => Some(SandboxMode::ReadOnly),
        Some("workspace-write") => Some(SandboxMode::WorkspaceWrite),
        Some("danger-full-access") => Some(SandboxMode::DangerFullAccess),
        Some(other) => anyhow::bail!(
            "Unknown sandbox mode '{other}'. Use read-only, workspace-write or danger-full-access."
        ),
    };
    let approval_policy = match approval.map(str::trim) {
        None => None,
        Some("untrusted") => Some(AskForApproval::UnlessTrusted),
        Some("on-failure") => Some(AskForApproval::OnFailure),
        Some("on-request") => Some(AskForApproval::OnRequest),
        Some("never") => Some(AskForApproval::Never),
        Some(other) => anyhow::bail!(
            "Unknown approval policy '{other}'. Use untrusted, on-failure, on-request or never."
        ),
    };
    if bypass_all {
        return Ok(InteractivePolicy {
            sandbox_mode: Some(SandboxMode::DangerFullAccess),
            approval_policy: Some(AskForApproval::Never),
        });
    }
    if full_auto {
        // Full-auto removes prompts but keeps the sandbox; it must not widen
        // an explicit read-only request.
        return Ok(InteractivePolicy {
            sandbox_mode: sandbox_mode.or(Some(SandboxMode::WorkspaceWrite)),
            approval_policy: Some(AskForApproval::OnFailure),
        });
    }
    Ok(InteractivePolicy {
        sandbox_mode,
        approval_policy,
    })
}

#[cfg(unix)]
pub(crate) fn install_signal_handler() {
    use signal_hook::consts::{SIGINT, SIGTERM};

    match signal_hook::iterator::Signals::new([SIGINT, SIGTERM]) {
        Ok(mut signals) => {
            std::thread::spawn(move || {
                if let Some(signal) = signals.forever().next() {
                    // A filename prefix is not ownership. Never scan shared
                    // temp/home directories for files to delete on shutdown.
                    crate::restore_terminal();
                    std::process::exit(128 + signal);
                }
            });
        }
        Err(_) => eprintln!("Signal handling could not be initialized"),
    }
}

#[cfg(not(unix))]
pub(crate) fn install_signal_handler() {
    if ctrlc::set_handler(|| {
        crate::restore_terminal();
        std::process::exit(130);
    })
    .is_err()
    {
        eprintln!("Signal handling could not be initialized");
    }
}

/// Act as the Linux sandbox wrapper when re-executed by the engine.
///
/// Must run before argument parsing: the wrapper argv is never a user command
/// and returns only by `exec`ing the sandboxed program.
pub fn run_as_sandbox_wrapper_if_requested() {
    #[cfg(target_os = "linux")]
    {
        let mut args = std::env::args();
        let program = args.next().unwrap_or_default();
        if args.next().as_deref() == Some(cortex_engine::sandbox::SELF_WRAPPER_ARG) {
            cortex_linux_sandbox::run_main_with(std::iter::once(program).chain(args));
        }
        cortex_engine::sandbox::enable_self_wrapper();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_probe_preserves_existing_files() {
        let home = tempfile::tempdir().unwrap();
        let existing = home.path().join(".write_test");
        std::fs::write(&existing, "another session").unwrap();
        check_cortex_home_writable(home.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(existing).unwrap(),
            "another session"
        );
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 1);
    }

    #[test]
    fn test_home_probe_rejects_non_directory() {
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(check_cortex_home_writable(file.path()).is_err());
    }

    #[test]
    fn test_home_probe_does_not_create_missing_home() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("missing");
        check_cortex_home_writable(&home).unwrap();
        assert!(!home.exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_home_probe_does_not_follow_write_test_symlink() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let target = root.path().join("preserve");
        std::fs::write(&target, "keep").unwrap();
        std::os::unix::fs::symlink(&target, home.join(".write_test")).unwrap();
        check_cortex_home_writable(&home).unwrap();
        assert_eq!(std::fs::read_to_string(target).unwrap(), "keep");
        assert!(home.join(".write_test").is_symlink());
    }

    #[test]
    fn test_interactive_policy_rejects_unknown_values() {
        assert!(interactive_policy(Some("yolo"), None, false, false).is_err());
        assert!(interactive_policy(None, Some("always"), false, false).is_err());
    }

    #[test]
    fn test_interactive_policy_maps_explicit_flags() {
        let policy =
            interactive_policy(Some("read-only"), Some("untrusted"), false, false).unwrap();
        assert_eq!(
            policy.sandbox_mode,
            Some(cortex_engine::config::SandboxMode::ReadOnly)
        );
        assert_eq!(
            policy.approval_policy,
            Some(cortex_protocol::AskForApproval::UnlessTrusted)
        );
        assert_eq!(
            interactive_policy(None, None, false, false).unwrap(),
            InteractivePolicy {
                sandbox_mode: None,
                approval_policy: None
            }
        );
    }

    #[test]
    fn test_full_auto_keeps_explicit_read_only_sandbox() {
        let policy = interactive_policy(Some("read-only"), None, true, false).unwrap();
        assert_eq!(
            policy.sandbox_mode,
            Some(cortex_engine::config::SandboxMode::ReadOnly)
        );
        assert_eq!(
            policy.approval_policy,
            Some(cortex_protocol::AskForApproval::OnFailure)
        );
        let default = interactive_policy(None, None, true, false).unwrap();
        assert_eq!(
            default.sandbox_mode,
            Some(cortex_engine::config::SandboxMode::WorkspaceWrite)
        );
    }

    #[test]
    fn test_bypass_flag_is_the_only_path_to_full_access_without_sandbox_flag() {
        let policy = interactive_policy(None, None, false, true).unwrap();
        assert_eq!(
            policy.sandbox_mode,
            Some(cortex_engine::config::SandboxMode::DangerFullAccess)
        );
        assert_eq!(
            policy.approval_policy,
            Some(cortex_protocol::AskForApproval::Never)
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_signal_child() {
        if std::env::var_os("CORTEX_SIGNAL_TEST_CHILD").is_none() {
            return;
        }
        crate::install_cleanup_handler();
        println!("signal-fixture-ready");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_signals_preserve_other_sessions_files() {
        for (signal, exit_code) in [("-INT", 130), ("-TERM", 143)] {
            assert_signal_preserves_files(signal, exit_code);
        }
    }

    #[cfg(unix)]
    fn assert_signal_preserves_files(signal: &str, exit_code: i32) {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        let root = tempfile::tempdir().unwrap();
        let home = root.path().join(".cortex");
        std::fs::create_dir(&home).unwrap();
        let other_lock = home.join("other-session.lock");
        let other_temp = root.path().join("cortex-other-session");
        std::fs::write(&other_lock, "lock").unwrap();
        std::fs::create_dir(&other_temp).unwrap();
        std::fs::write(other_temp.join("keep"), "content").unwrap();

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "startup::tests::test_signal_child",
                "--nocapture",
            ])
            .env("CORTEX_SIGNAL_TEST_CHILD", "1")
            .env("HOME", root.path())
            .env("CORTEX_HOME", &home)
            .env("TMPDIR", root.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let ready = BufReader::new(stdout)
                .lines()
                .map_while(Result::ok)
                .any(|line| line == "signal-fixture-ready");
            let _ = sender.send(ready);
        });
        if receiver.recv_timeout(Duration::from_secs(5)) != Ok(true) {
            let _ = child.kill();
            let _ = child.wait();
            reader.join().unwrap();
            panic!("signal fixture did not start");
        }
        reader.join().unwrap();
        assert!(
            Command::new("kill")
                .args([signal, &child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("signal handler did not terminate");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(status.code(), Some(exit_code));
        assert_eq!(std::fs::read_to_string(other_lock).unwrap(), "lock");
        assert_eq!(
            std::fs::read_to_string(other_temp.join("keep")).unwrap(),
            "content"
        );
    }
}
