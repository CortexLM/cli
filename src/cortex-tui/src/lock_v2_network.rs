//! Offline and HTTP 429 rate-limit lock v2 scenes.
//!
//! Split out of [`crate::lock_v2`] so the source-policy line-count baseline
//! on that file does not regress. Distinct from API-down (`error-unavailable`)
//! and period-quota (`quota-exhausted`).

use cortex_core::widgets::Message;

use crate::app::AppState;

pub const OFFLINE_TITLE: &str = "× You're offline";
pub const OFFLINE_BODY: &str =
    "Reconnect when the network is back — your work is saved in this session.";
pub const OFFLINE_BODY_NARROW: &str = "Reconnect — your work is saved.";
pub const RATE_LIMIT_TITLE: &str = "× Rate limited";
pub const RATE_LIMIT_BODY: &str = "Too many requests — try again in 2m (Retry-After: 120s). Follow-ups stay held until the rate limit resets.";
pub const RATE_LIMIT_BODY_NARROW: &str = "Too many requests — try again in 2m.";

/// Apply an offline or rate-limit lock scene. Returns `false` when `id` is neither.
pub fn apply_offline_rate_limit_scene(id: &str, state: &mut AppState) -> bool {
    let narrow = state.terminal_size.0 <= 40;
    match id {
        "offline" => {
            state.show_launch_splash = false;
            state.tokens_used = 14_000;
            state.offline_held = true;
            state.add_message(
                Message::user("keep going on this Cortex session").with_timestamp("03:18 PM"),
            );
            state.add_message(Message::system(OFFLINE_TITLE));
            state.add_message(Message::system(if narrow {
                OFFLINE_BODY_NARROW
            } else {
                OFFLINE_BODY
            }));
            true
        }
        "rate-limit" => {
            state.show_launch_splash = false;
            state.tokens_used = 14_000;
            state.rate_limit_held = true;
            state.add_message(
                Message::user("send another follow-up on Cortex Mini 1").with_timestamp("03:22 PM"),
            );
            state.add_message(Message::system(RATE_LIMIT_TITLE));
            state.add_message(Message::system(if narrow {
                RATE_LIMIT_BODY_NARROW
            } else {
                RATE_LIMIT_BODY
            }));
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock_v2::render_lock_v2_scene;
    use crate::ui::consts::{
        PLACEHOLDER_OFFLINE, PLACEHOLDER_OFFLINE_NARROW, PLACEHOLDER_QUOTA, PLACEHOLDER_RATE_LIMIT,
        PLACEHOLDER_RATE_LIMIT_NARROW,
    };
    use cortex_core::style::{ERROR, TEXT_DIM};
    use ratatui::style::Color;

    fn first_fg(frame: &crate::lock_proof::LockFrame, needle: &str) -> Option<Color> {
        let width = frame.buffer.area.width;
        let height = frame.buffer.area.height;
        for y in 0..height {
            let mut row = String::new();
            for x in 0..width {
                row.push_str(frame.buffer[(x, y)].symbol());
            }
            if let Some(idx) = row.find(needle) {
                return Some(frame.buffer[(idx as u16, y)].fg);
            }
        }
        None
    }

    #[test]
    fn offline_and_rate_limit_are_distinct_from_api_and_quota() {
        let offline = render_lock_v2_scene("offline", 120, 40).expect("offline");
        let rate = render_lock_v2_scene("rate-limit", 120, 40).expect("rate-limit");
        let unavail = render_lock_v2_scene("error-unavailable", 120, 40).expect("unavail");
        let quota = render_lock_v2_scene("quota-exhausted", 120, 40).expect("quota");

        assert!(
            offline.plain.contains("You're offline"),
            "offline title:\n{}",
            offline.plain
        );
        assert!(
            offline.plain.contains(OFFLINE_BODY),
            "offline body:\n{}",
            offline.plain
        );
        assert!(
            !offline.plain.contains("temporarily unavailable"),
            "offline must not reuse the API-down line:\n{}",
            offline.plain
        );
        assert!(
            offline.plain.contains(PLACEHOLDER_OFFLINE),
            "offline held composer:\n{}",
            offline.plain
        );
        assert!(
            offline.plain.contains("Enter:retry") && offline.plain.contains("Ctrl+x:shortcuts"),
            "offline footer:\n{}",
            offline.plain
        );
        assert!(
            !offline.plain.contains("Shift+Tab:mode"),
            "offline must not use the idle footer:\n{}",
            offline.plain
        );

        assert!(
            rate.plain.contains("Rate limited"),
            "rate-limit title:\n{}",
            rate.plain
        );
        assert!(
            rate.plain.contains("Retry-After") || rate.plain.contains("try again in 2m"),
            "rate-limit retry-after:\n{}",
            rate.plain
        );
        assert!(
            !rate.plain.contains("Agent quota exhausted") && !rate.plain.contains("500 / 500"),
            "rate-limit must not reuse the quota board:\n{}",
            rate.plain
        );
        assert!(
            !rate.plain.contains("upgrade at cortex.foundation/billing"),
            "rate-limit must not lead with a billing upgrade CTA:\n{}",
            rate.plain
        );
        assert!(
            rate.plain.contains(PLACEHOLDER_RATE_LIMIT),
            "rate-limit held composer:\n{}",
            rate.plain
        );
        assert!(
            !rate.plain.contains(PLACEHOLDER_QUOTA),
            "rate-limit must not reuse the quota placeholder:\n{}",
            rate.plain
        );
        assert!(
            rate.plain.contains("Enter:retry") && rate.plain.contains("/usage:details"),
            "rate-limit footer:\n{}",
            rate.plain
        );
        assert!(
            !rate.plain.contains("Shift+Tab:mode"),
            "rate-limit must not use the idle footer:\n{}",
            rate.plain
        );

        assert_eq!(first_fg(&offline, "You're offline"), Some(ERROR));
        assert_eq!(first_fg(&offline, "Reconnect"), Some(TEXT_DIM));
        assert_eq!(first_fg(&rate, "Rate limited"), Some(ERROR));
        assert_eq!(first_fg(&rate, "Too many requests"), Some(TEXT_DIM));
        assert_eq!(first_fg(&unavail, "temporarily unavailable"), Some(ERROR));

        assert_ne!(offline.ansi, unavail.ansi);
        assert_ne!(rate.ansi, quota.ansi);
        assert_ne!(offline.ansi, rate.ansi);

        for (width, height) in [(120u16, 40u16), (40u16, 12u16)] {
            let a = render_lock_v2_scene("offline", width, height).expect("offline");
            let b = render_lock_v2_scene("rate-limit", width, height).expect("rate-limit");
            assert_ne!(
                a.ansi, b.ansi,
                "offline and rate-limit collide at {width}x{height}"
            );
            assert!(
                a.plain.contains("offline") || a.plain.contains("You're offline"),
                "offline at {width}x{height}:\n{}",
                a.plain
            );
            assert!(
                b.plain.contains("Rate limited") || b.plain.contains("Too many requests"),
                "rate-limit at {width}x{height}:\n{}",
                b.plain
            );
            assert_eq!(
                first_fg(&a, "You're offline"),
                Some(ERROR),
                "offline title red at {width}x{height}"
            );
            assert_eq!(
                first_fg(&b, "Rate limited"),
                Some(ERROR),
                "rate-limit title red at {width}x{height}"
            );
            assert_eq!(
                first_fg(&b, "Too many requests"),
                Some(TEXT_DIM),
                "rate-limit body dim at {width}x{height}"
            );
        }

        let offline_n = render_lock_v2_scene("offline", 40, 12).expect("offline narrow");
        assert!(
            offline_n.plain.contains(OFFLINE_BODY_NARROW),
            "narrow offline body:\n{}",
            offline_n.plain
        );
        assert!(
            !offline_n.plain.contains("when the network is back"),
            "narrow offline must use the short body:\n{}",
            offline_n.plain
        );
        assert!(
            offline_n.plain.contains(PLACEHOLDER_OFFLINE_NARROW),
            "narrow offline placeholder:\n{}",
            offline_n.plain
        );
        assert_eq!(first_fg(&offline_n, "Reconnect"), Some(TEXT_DIM));

        let rate_n = render_lock_v2_scene("rate-limit", 40, 12).expect("rate-limit narrow");
        assert!(
            rate_n.plain.contains(PLACEHOLDER_RATE_LIMIT_NARROW)
                || rate_n.plain.contains("rate limit resets"),
            "narrow rate-limit placeholder:\n{}",
            rate_n.plain
        );
        assert!(
            rate_n.plain.contains("try again in 2m"),
            "narrow rate-limit retry timing:\n{}",
            rate_n.plain
        );
        assert!(
            !rate_n.plain.contains("Retry-After"),
            "narrow rate-limit must use the compact body:\n{}",
            rate_n.plain
        );
        assert!(
            rate_n.plain.contains("Enter:retry") && rate_n.plain.contains("/usage:details"),
            "narrow rate-limit footer:\n{}",
            rate_n.plain
        );
    }

    #[test]
    fn apply_rejects_unknown_ids() {
        let mut state = AppState::default();
        assert!(!apply_offline_rate_limit_scene(
            "error-unavailable",
            &mut state
        ));
        assert!(!apply_offline_rate_limit_scene(
            "quota-exhausted",
            &mut state
        ));
        assert!(!state.offline_held);
        assert!(!state.rate_limit_held);
        assert!(!state.quota_held);
    }
}
