//! App state tests: splash, clear, and message bookkeeping.
use super::*;
use cortex_core::widgets::Message;

#[test]
fn first_user_turn_drops_the_launch_splash() {
    let mut state = AppState::default();
    assert!(state.show_launch_splash);
    state.add_message(Message::user("hello"));
    assert!(!state.show_launch_splash);
    state.clear_messages();
    assert!(
        !state.show_launch_splash,
        "/clear must not restore the splash"
    );
}
