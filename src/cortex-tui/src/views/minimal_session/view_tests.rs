//! Composer file-chip footer tests.
use super::*;

#[test]
fn completed_at_path_is_a_file_chip() {
    assert!(composer_has_completed_file_chip(
        "explain @src/cortex-tui/src/composer.rs "
    ));
    assert!(composer_has_completed_file_chip(
        "explain @src/composer.rs "
    ));
    assert!(composer_has_completed_file_chip("@src/foo.rs"));
}

#[test]
fn email_and_bare_at_are_not_file_chips() {
    assert!(!composer_has_completed_file_chip("contact me@example.com"));
    assert!(!composer_has_completed_file_chip("ada@example.com"));
    assert!(!composer_has_completed_file_chip("please inspect @"));
    assert!(!composer_has_completed_file_chip("please inspect @src/"));
    assert!(!composer_has_completed_file_chip("@"));
    assert!(!composer_has_completed_file_chip("hello world"));
}
