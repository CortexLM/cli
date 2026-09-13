//! Computer lock v2 scenes — This PC/SSH fail-closed and Cloud default.
//!
//! Split out of [`crate::lock_v2`] so adding these boards does not grow that
//! file past the source-policy line-count baseline.

use cortex_core::widgets::Message;
use cortex_engine::client::DISCONNECTED_RUNTIME;

use crate::app::AppState;
use crate::ui::consts::DISCONNECTED_TITLE;

/// Computer lock boards. Each filename is one live state.
pub const COMPUTER_SCENE_IDS: &[&str] = &["computer-disconnected", "computer-cloud-default"];

/// Narrow (40-column) fail-closed body — still names This PC/SSH and refuses
/// a substitute. Wide boards use the engine constant verbatim.
pub const DISCONNECTED_RUNTIME_NARROW: &str =
    "This PC and SSH need a connected Code session. No runtime was substituted.";

/// Apply a Computer lock scene. Returns `false` when `id` is not one of these.
pub fn apply_computer_scene(id: &str, state: &mut AppState, width: u16) -> bool {
    if !COMPUTER_SCENE_IDS.contains(&id) {
        return false;
    }
    match id {
        "computer-disconnected" => seed_disconnected(state, width),
        "computer-cloud-default" => seed_cloud_default(state),
        _ => return false,
    }
    true
}

fn seed_disconnected(state: &mut AppState, width: u16) {
    state.show_launch_splash = false;
    state.tokens_used = 14_000;
    state.computer_held = true;
    state.caret_visible = false;
    state.add_message(
        Message::user(if width <= 40 {
            "run local tests"
        } else {
            "run the local cargo tests"
        })
        .with_timestamp("09:18 AM"),
    );
    state.add_message(Message::system(format!("× {DISCONNECTED_TITLE}")));
    state.add_message(Message::system(if width <= 40 {
        DISCONNECTED_RUNTIME_NARROW
    } else {
        DISCONNECTED_RUNTIME
    }));
}

fn seed_cloud_default(state: &mut AppState) {
    state.show_launch_splash = true;
    state.tokens_used = 0;
    state.show_computer_default = true;
}

/// Squeezed transcript text so wrapped rows still match engine copy.
/// Scrollbar and box-drawing cells become spaces before collapse.
#[cfg(test)]
fn squeezed_plain(plain: &str) -> String {
    plain
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c.is_ascii_punctuation() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock_v2::render_lock_v2_scene;
    use crate::lock_v2_ids::{LOCK_V2_NARROW_IDS, LOCK_V2_WIDE_IDS};
    use crate::ui::consts::PLACEHOLDER_DISCONNECTED;
    use cortex_core::style::{ERROR, TEXT_DIM};
    use cortex_engine::client::ComputerKind;

    const SIZES: [(u16, u16); 2] = [(120, 40), (40, 12)];

    fn has_fg(
        frame: &crate::lock_proof::LockFrame,
        needle: &str,
        color: ratatui::style::Color,
    ) -> bool {
        let width = frame.buffer.area.width;
        let height = frame.buffer.area.height;
        for y in 0..height {
            let row: String = (0..width)
                .map(|x| frame.buffer[(x, y)].symbol().to_string())
                .collect();
            if let Some(at) = row.find(needle) {
                let x = at as u16;
                if frame.buffer[(x, y)].fg == color {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn computer_scene_ids_are_registered() {
        assert_eq!(COMPUTER_SCENE_IDS.len(), 2);
        for id in COMPUTER_SCENE_IDS {
            assert!(LOCK_V2_WIDE_IDS.contains(id), "{id} missing from wide");
            assert!(LOCK_V2_NARROW_IDS.contains(id), "{id} missing from narrow");
        }
        assert_eq!(LOCK_V2_WIDE_IDS.len(), 95);
        assert_eq!(LOCK_V2_NARROW_IDS.len(), 49);
        let mut seen = std::collections::HashSet::new();
        for id in LOCK_V2_WIDE_IDS.iter().chain(LOCK_V2_NARROW_IDS) {
            seen.insert(*id);
        }
        assert!(seen.contains("computer-disconnected"));
        assert!(seen.contains("computer-cloud-default"));
    }

    #[test]
    fn disconnected_uses_engine_copy_and_fail_closed() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("computer-disconnected", width, height)
                .expect("computer-disconnected");
            let plain = &frame.plain;
            let squeezed = squeezed_plain(plain);
            assert!(
                plain.contains("This PC"),
                "missing This PC at {width}x{height}:\n{plain}"
            );
            assert!(
                plain.contains("SSH"),
                "missing SSH at {width}x{height}:\n{plain}"
            );
            assert!(
                plain.contains("No runtime was substituted"),
                "missing refuse at {width}x{height}:\n{plain}"
            );
            assert!(
                plain.contains(DISCONNECTED_TITLE),
                "missing title at {width}x{height}:\n{plain}"
            );
            if width >= 120 {
                assert!(
                    squeezed.contains(DISCONNECTED_RUNTIME),
                    "wide board must carry the engine constant:\n{plain}"
                );
            } else {
                assert!(
                    squeezed.contains(DISCONNECTED_RUNTIME_NARROW),
                    "narrow board must keep the connected-session gist:\n{plain}"
                );
            }
            assert!(
                !plain.contains("temporarily unavailable"),
                "must not reuse error-unavailable:\n{plain}"
            );
            assert!(
                !plain.contains("Handed off"),
                "must not look like cloud-handoff:\n{plain}"
            );
            assert!(
                plain.contains("Enter") && plain.contains("retry"),
                "held footer Enter:retry at {width}x{height}:\n{plain}"
            );
            assert!(
                !plain.contains("Shift+Tab"),
                "held board must not show idle Shift+Tab:\n{plain}"
            );
            assert!(
                plain.contains("Connect a host"),
                "held placeholder at {width}x{height}:\n{plain}"
            );
            if width >= 120 {
                assert!(
                    plain.contains(PLACEHOLDER_DISCONNECTED)
                        || squeezed.contains("CORTEX_COMPUTER"),
                    "wide placeholder:\n{plain}"
                );
            } else {
                assert!(
                    squeezed.contains("Connect a host") && squeezed.contains("unset for"),
                    "narrow placeholder:\n{plain}"
                );
            }
        }
    }

    #[test]
    fn disconnected_title_is_error_red_and_body_is_dim() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("computer-disconnected", width, height).expect("disc");
            assert!(
                has_fg(&frame, "This PC disconnected", ERROR) || has_fg(&frame, "×", ERROR),
                "title must be error-red at {width}x{height}"
            );
            assert!(
                has_fg(&frame, "No runtime", TEXT_DIM)
                    || has_fg(&frame, "substituted", TEXT_DIM)
                    || has_fg(&frame, "connected", TEXT_DIM),
                "body must be dim at {width}x{height}:\n{}",
                frame.plain
            );
        }
    }

    #[test]
    fn disconnected_is_not_unavailable_or_handoff() {
        let disc = render_lock_v2_scene("computer-disconnected", 120, 40).unwrap();
        let unavail = render_lock_v2_scene("error-unavailable", 120, 40).unwrap();
        let handoff = render_lock_v2_scene("cloud-handoff", 120, 40).unwrap();
        let quota = render_lock_v2_scene("quota-exhausted", 120, 40).unwrap();
        assert_ne!(disc.ansi, unavail.ansi);
        assert_ne!(disc.ansi, handoff.ansi);
        assert_ne!(disc.ansi, quota.ansi);
        assert!(unavail.plain.contains("temporarily unavailable"));
        assert!(!disc.plain.contains("temporarily unavailable"));
        assert!(handoff.plain.contains("Handed off") || handoff.plain.contains("Cortex Cloud"));
        assert!(!disc.plain.contains("Handed off"));
    }

    #[test]
    fn cloud_default_shows_computer_cloud_not_handoff() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("computer-cloud-default", width, height)
                .expect("cloud-default");
            let plain = &frame.plain;
            assert!(
                plain.contains("Computer"),
                "missing Computer at {width}x{height}:\n{plain}"
            );
            assert!(
                plain.contains(ComputerKind::Cloud.label()),
                "missing Cloud label at {width}x{height}:\n{plain}"
            );
            assert!(
                plain.contains("Welcome") && plain.contains("Cortex"),
                "welcome chrome at {width}x{height}:\n{plain}"
            );
            assert!(
                !plain.contains("Handed off"),
                "must not reuse cloud-handoff:\n{plain}"
            );
            assert!(
                !plain.contains("This PC disconnected"),
                "cloud default is not the fail-closed board:\n{plain}"
            );
            assert!(
                plain.contains("Shift+Tab"),
                "idle footer at {width}x{height}:\n{plain}"
            );
        }
        let cloud = render_lock_v2_scene("computer-cloud-default", 120, 40).unwrap();
        let welcome = render_lock_v2_scene("welcome-cortex", 120, 40).unwrap();
        let handoff = render_lock_v2_scene("cloud-handoff", 120, 40).unwrap();
        assert_ne!(cloud.ansi, welcome.ansi);
        assert_ne!(cloud.ansi, handoff.ansi);
        assert!(!welcome.plain.contains("Computer"));
        assert!(cloud.plain.contains("Directory") || cloud.plain.contains("Computer"));
    }

    #[test]
    fn computer_kind_labels_match_product() {
        assert_eq!(ComputerKind::ThisPc.label(), "This PC");
        assert_eq!(ComputerKind::Cloud.label(), "Cloud");
        assert_eq!(ComputerKind::Ssh.label(), "SSH");
        assert!(DISCONNECTED_RUNTIME.contains("This PC"));
        assert!(DISCONNECTED_RUNTIME.contains("SSH"));
        assert!(DISCONNECTED_RUNTIME.contains("No runtime was substituted"));
    }
}
