//! Offline and HTTP 429 rate-limit lock v2 scenes.
//!
//! Split out of [`crate::lock_v2`] so the source-policy line-count baseline
//! on that file does not regress. Distinct from API-down (`error-unavailable`)
//! and period-quota (`quota-exhausted`).

use cortex_core::widgets::Message;

use crate::app::AppState;

/// Apply an offline or rate-limit lock scene. Returns `false` when `id` is neither.
pub fn apply_offline_rate_limit_scene(id: &str, state: &mut AppState) -> bool {
    match id {
        "offline" => {
            state.show_launch_splash = false;
            state.tokens_used = 14_000;
            state.add_message(
                Message::user("keep going on this Cortex session").with_timestamp("03:18 PM"),
            );
            state.add_message(Message::system("× You're offline"));
            state.add_message(Message::system(
                "Reconnect when the network is back — your work is saved in this session.",
            ));
            true
        }
        "rate-limit" => {
            state.show_launch_splash = false;
            state.tokens_used = 14_000;
            state.add_message(
                Message::user("send another follow-up on Cortex Mini 1").with_timestamp("03:22 PM"),
            );
            state.add_message(Message::system("× Rate limited"));
            state.add_message(Message::system(
                "Too many requests — try again in 2m (Retry-After: 120s). Follow-ups stay held until the rate limit resets.",
            ));
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock_v2::render_lock_v2_scene;

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
            offline.plain.contains("Reconnect") && offline.plain.contains("network"),
            "offline body:\n{}",
            offline.plain
        );
        assert!(
            !offline.plain.contains("temporarily unavailable"),
            "offline must not reuse the API-down line:\n{}",
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
        }
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
    }
}
