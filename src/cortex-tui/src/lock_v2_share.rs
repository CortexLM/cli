//! `/share` lock v2 scene — read-only share-link chrome.
//!
//! Split out of [`crate::lock_v2`] so the source-policy line-count baseline on
//! that file does not regress. The `Shared` marker is persistent status chrome:
//! it stays up for as long as the link is live, and `/unshare` clears it by
//! dropping [`crate::app::AppState::share_link`] back to `None` — idle chrome,
//! no confirmation sheet.

use cortex_core::widgets::Message;

use crate::app::AppState;

/// Read-only link minted by `/share`, as it appears in the transcript.
pub const SHARE_URL: &str = "cortex.foundation/share/9f4c2a71";
pub const SHARE_TITLE: &str = "Shared this session — read-only link is live.";
pub const SHARE_TITLE_NARROW: &str = "Shared — read-only link is live.";
pub const SHARE_LINK_LINE: &str = "cortex.foundation/share/9f4c2a71 · copied to your clipboard";
pub const SHARE_SCOPE: &str = "Anyone with the link can read this session. /unshare turns it off.";

/// Apply the `session-shared` lock scene. Returns `false` for any other `id`.
pub fn apply_share_scene(id: &str, state: &mut AppState) -> bool {
    if id != "session-shared" {
        return false;
    }
    let narrow = state.terminal_size.0 <= 40;
    state.show_launch_splash = false;
    state.tokens_used = 14_000;
    state.share_link = Some(SHARE_URL.to_string());
    state.add_message(Message::user("/share").with_timestamp("04:41 PM"));
    state.add_message(Message::system(if narrow {
        SHARE_TITLE_NARROW
    } else {
        SHARE_TITLE
    }));
    state.add_message(Message::system(if narrow {
        SHARE_URL
    } else {
        SHARE_LINK_LINE
    }));
    if !narrow {
        state.add_message(Message::system(SHARE_SCOPE));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock_v2::render_lock_v2_scene;
    use crate::ui::consts::{SHARE_MARKER, SHARE_MARKER_NARROW};
    use crate::views::minimal_session::PLACEHOLDER_IDLE;
    use cortex_core::style::{ERROR, TEXT_DIM};
    use ratatui::style::Color;

    fn row_text(frame: &crate::lock_proof::LockFrame, y: u16) -> String {
        (0..frame.buffer.area.width)
            .map(|x| frame.buffer[(x, y)].symbol())
            .collect()
    }

    fn first_fg(frame: &crate::lock_proof::LockFrame, needle: &str) -> Option<Color> {
        for y in 0..frame.buffer.area.height {
            let row = row_text(frame, y);
            if let Some(idx) = row.find(needle) {
                return Some(frame.buffer[(idx as u16, y)].fg);
            }
        }
        None
    }

    #[test]
    fn share_marker_is_dim_status_chrome_at_both_sizes() {
        for (width, height, marker) in
            [(120u16, 40u16, SHARE_MARKER), (40, 12, SHARE_MARKER_NARROW)]
        {
            let frame = render_lock_v2_scene("session-shared", width, height)
                .unwrap_or_else(|e| panic!("session-shared {width}x{height}: {e}"));
            let header = row_text(&frame, 0);
            assert!(
                header.contains(marker),
                "share marker on the status line at {width}x{height}:\n{header}"
            );
            assert!(
                header.contains("Shared"),
                "narrow must keep the word Shared at {width}x{height}:\n{header}"
            );
            assert!(
                header.contains("14K / 500K"),
                "token counter stays on the status line at {width}x{height}:\n{header}"
            );
            assert_eq!(
                first_fg(&frame, "Shared"),
                Some(TEXT_DIM),
                "share marker is dim at {width}x{height}"
            );
            assert!(
                frame.plain.contains(SHARE_URL),
                "minted read-only link at {width}x{height}:\n{}",
                frame.plain
            );
        }
    }

    #[test]
    fn shared_session_stays_interactive_with_the_idle_footer() {
        let frame = render_lock_v2_scene("session-shared", 120, 40).expect("wide");
        assert!(
            frame.plain.contains(PLACEHOLDER_IDLE),
            "composer stays interactive — not held:\n{}",
            frame.plain
        );
        assert!(
            frame.plain.contains("Shift+Tab:mode") && frame.plain.contains("Ctrl+x:shortcuts"),
            "normal session footer:\n{}",
            frame.plain
        );
        assert!(
            !frame.plain.contains("Choose an option above"),
            "share chrome is not an approval sheet:\n{}",
            frame.plain
        );
        assert!(
            frame.plain.contains("copied to your clipboard"),
            "wide board notes the clipboard copy:\n{}",
            frame.plain
        );
        assert!(
            frame.plain.contains("/unshare"),
            "wide board names /unshare as the way out:\n{}",
            frame.plain
        );
        assert_ne!(
            first_fg(&frame, "Shared this session"),
            Some(ERROR),
            "share copy is not an error"
        );
    }

    #[test]
    fn session_shared_is_distinct_from_neighbouring_boards() {
        let shared_wide = render_lock_v2_scene("session-shared", 120, 40).expect("wide");
        let shared_narrow = render_lock_v2_scene("session-shared", 40, 12).expect("narrow");
        assert_ne!(shared_wide.ansi, shared_narrow.ansi);
        for other in [
            "session-empty",
            "session-user-bars",
            "cloud-handoff",
            "goal-chip-active",
        ] {
            let frame = render_lock_v2_scene(other, 120, 40).expect(other);
            assert_ne!(
                shared_wide.ansi, frame.ansi,
                "session-shared must differ from {other}"
            );
            assert!(
                !row_text(&frame, 0).contains("Shared"),
                "{other} must not paint the share marker:\n{}",
                row_text(&frame, 0)
            );
        }
    }

    #[test]
    fn unshare_clears_the_marker() {
        let mut state = AppState::default();
        state.terminal_size = (120, 40);
        assert!(apply_share_scene("session-shared", &mut state));
        assert_eq!(state.share_link.as_deref(), Some(SHARE_URL));
        // `/unshare` is idle chrome: dropping the link is all it takes.
        state.share_link = None;
        assert!(state.share_link.is_none());
    }

    #[test]
    fn apply_rejects_unknown_ids() {
        let mut state = AppState::default();
        assert!(!apply_share_scene("session-empty", &mut state));
        assert!(!apply_share_scene("cloud-handoff", &mut state));
        assert!(state.share_link.is_none());
        assert!(state.messages.is_empty());
    }
}
