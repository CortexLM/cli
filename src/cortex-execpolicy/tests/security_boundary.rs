use cortex_execpolicy::{Decision, ExecPolicy, ParsedCommand};

#[test]
fn security_boundary_explicit_shell_payload_is_evaluated() {
    assert_eq!(
        ExecPolicy::new().evaluate(&["/bin/sh".into(), "-c".into(), "rm -rf /".into()]),
        Decision::Deny
    );
    assert_eq!(
        ExecPolicy::new().evaluate(&["/usr/bin/sudo".into(), "true".into()]),
        Decision::Deny
    );
}

#[test]
fn security_boundary_later_deny_wins_over_earlier_ask() {
    let command =
        ParsedCommand::from_shell_string("curl https://example.invalid; sudo true").unwrap();
    assert_eq!(ExecPolicy::new().evaluate_parsed(&command), Decision::Deny);
}

#[test]
fn security_boundary_malformed_quoting_is_not_fallback_authority() {
    assert!(ParsedCommand::from_shell_string("echo 'unterminated").is_err());
}
