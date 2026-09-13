//! Lock v2 residual chrome: local-tools consent, `@file` chip, undo sheet.
//!
//! Split out of [`crate::lock_v2_boards`] so adding COR-222/223/224 scenes
//! does not grow that file past the source-policy line-count baseline.

use cortex_core::widgets::Message;

use crate::app::AppState;
use crate::interactive::builders::build_question_prompt;
use crate::lock_v2_scenes::{conversation, radios, resumed};

/// Residual parity boards (wide + narrow).
pub const PARITY_IDS: &[&str] = &["consent-local-tools", "composer-file-chip", "undo-sheet"];

/// Attached `@file` token shown after the picker closes.
pub const FILE_CHIP_TOKEN: &str = "@src/cortex-tui/src/composer.rs";

/// Wide composer prompt with the attached file chip and a trailing space.
pub const FILE_CHIP_PROMPT: &str = "explain @src/cortex-tui/src/composer.rs ";

/// Apply a residual parity scene. Returns `false` when `id` is not one of ours.
pub fn apply_parity_scene(id: &str, state: &mut AppState, width: u16) -> bool {
    if !PARITY_IDS.contains(&id) {
        return false;
    }
    match id {
        "consent-local-tools" => {
            apply_consent_local_tools(state, width);
            true
        }
        "composer-file-chip" => {
            apply_composer_file_chip(state, width);
            true
        }
        "undo-sheet" => {
            apply_undo_sheet(state, width);
            true
        }
        _ => false,
    }
}

fn apply_consent_local_tools(state: &mut AppState, width: u16) {
    resumed(state);
    let narrow = width <= 40;
    if !narrow {
        state.add_message(
            Message::user("fix the failing test in this repo").with_timestamp("10:18 AM"),
        );
        state.add_message(
            Message::assistant(
                "Run tools locally?\nTools run on This PC against the current working directory. Cloud is the default when unset.",
            )
            .with_timestamp("10:18 AM")
            .with_thought_secs(0.6),
        );
    } else {
        state.add_message(
            Message::assistant(
                "Run tools locally?\nThis PC · current directory · Cloud is default",
            )
            .with_timestamp("10:18 AM"),
        );
    }
    let rows: &[(&str, &str, &str)] = if narrow {
        &[
            ("yes", "1 Yes — run tools here", ""),
            ("always", "2 Always allow project", ""),
            ("no", "3 No — keep using Cloud", ""),
        ]
    } else {
        &[
            (
                "yes",
                "1 Yes — run tools in this directory",
                "This PC · current directory",
            ),
            (
                "always",
                "2 Always allow for this project",
                "remember this workspace",
            ),
            ("no", "3 No — keep using Cloud", "Cloud stays the default"),
        ]
    };
    state.enter_interactive_mode(
        build_question_prompt("Run tools locally?", rows, 0).with_prompt_focus(),
    );
}

/// Narrow composer cannot hold the full crate path after `> explain `.
pub const FILE_CHIP_PROMPT_NARROW: &str = "explain @src/composer.rs ";

fn apply_composer_file_chip(state: &mut AppState, width: u16) {
    resumed(state);
    if width <= 40 {
        state.input.set_text(FILE_CHIP_PROMPT_NARROW);
    } else {
        debug_assert!(FILE_CHIP_PROMPT.contains(FILE_CHIP_TOKEN));
        state.add_message(Message::system("@ attaches files  ·  ! enters Bash"));
        state.input.set_text(FILE_CHIP_PROMPT);
    }
}

fn apply_undo_sheet(state: &mut AppState, width: u16) {
    if width > 40 {
        conversation(state);
        state.add_message(
            Message::assistant(
                "Undo\nUndo the last turn, redo it, or rewind this Cortex session to a checkpoint.",
            )
            .with_timestamp("10:22 AM"),
        );
    } else {
        resumed(state);
        state.add_message(Message::assistant("Undo").with_timestamp("10:22 AM"));
    }
    state.input.set_text("/undo");
    let rows: &[(&str, &str, &str)] = if width <= 40 {
        &[
            ("undo", "1 Undo last turn", ""),
            ("redo", "2 Redo", ""),
            ("rewind", "3 Rewind…", ""),
        ]
    } else {
        &[
            ("undo", "1 Undo last turn", "restore the last turn"),
            ("redo", "2 Redo", "re-apply the undone turn"),
            (
                "rewind",
                "3 Rewind to checkpoint",
                "pick a Cortex session checkpoint",
            ),
        ]
    };
    state.enter_interactive_mode(radios("Undo", rows, 0, None));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock_v2::{LOCK_V2_NARROW_IDS, LOCK_V2_WIDE_IDS, render_lock_v2_scene};
    use cortex_core::style::{ACCENT, SELECTION_BG};
    use std::collections::HashSet;

    fn banned_competitors(plain: &str) -> bool {
        let lower = plain.to_ascii_lowercase();
        [
            "claude",
            "openai",
            "anthropic",
            "cursor",
            "codex",
            "devin",
            "gemini",
            "copilot",
        ]
        .iter()
        .any(|n| lower.contains(n))
    }

    #[test]
    fn parity_ids_are_registered_at_both_sizes() {
        assert_eq!(PARITY_IDS.len(), 3);
        for id in PARITY_IDS {
            assert!(LOCK_V2_WIDE_IDS.contains(id), "{id} missing from wide list");
            assert!(
                LOCK_V2_NARROW_IDS.contains(id),
                "{id} missing from narrow list"
            );
        }
    }

    #[test]
    fn consent_local_tools_is_not_permission_or_optin() {
        for (width, height) in [(120u16, 40u16), (40u16, 12u16)] {
            let frame =
                render_lock_v2_scene("consent-local-tools", width, height).expect("consent");
            assert!(
                frame.plain.contains("Run tools locally")
                    || frame.plain.contains("Yes") && frame.plain.contains("Cloud"),
                "consent missing local-tools copy at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("This PC")
                    || frame.plain.contains("directory")
                    || frame.plain.contains("Cloud"),
                "consent missing This PC / directory / Cloud at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                !frame.plain.contains("$ npm install"),
                "consent must not be a permission-prompt command block:\n{}",
                frame.plain
            );
            assert!(
                !frame
                    .plain
                    .to_ascii_lowercase()
                    .contains("retain coding data"),
                "consent must not reuse session-optin privacy copy:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("Choose an option above"),
                "consent composer must stay held:\n{}",
                frame.plain
            );
            assert!(!banned_competitors(&frame.plain), "{}", frame.plain);
            assert_ne!(
                frame.ansi,
                render_lock_v2_scene("permission-prompt", width, height)
                    .expect("permission-prompt")
                    .ansi,
                "consent-local-tools must differ from permission-prompt"
            );
            if width >= 80 {
                assert_ne!(
                    frame.ansi,
                    render_lock_v2_scene("session-optin", width, height)
                        .expect("session-optin")
                        .ansi,
                    "consent-local-tools must differ from session-optin"
                );
            }
            let mut sel = false;
            for y in 0..height {
                for x in 0..width {
                    if frame.buffer[(x, y)].bg == SELECTION_BG {
                        sel = true;
                    }
                }
            }
            assert!(sel, "consent focused row must paint SELECTION_BG");
            assert!(
                frame.plain.contains("confirm") && frame.plain.contains("cancel"),
                "consent footer must be confirm/cancel, not Typed, at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                !frame.plain.contains("Shift+Tab") && !frame.plain.contains("Alt+Enter"),
                "consent footer must not be the Typed composer strip:\n{}",
                frame.plain
            );
            if width <= 40 {
                assert!(
                    frame.plain.contains("run tools here")
                        && !frame.plain.contains("in this directory"),
                    "narrow consent must use shortened option copy:\n{}",
                    frame.plain
                );
                assert!(
                    !frame.plain.contains("current working directory"),
                    "narrow consent must not use the wide body:\n{}",
                    frame.plain
                );
            }
        }
    }

    #[test]
    fn composer_file_chip_shows_attached_path() {
        for (width, height) in [(120u16, 40u16), (40u16, 12u16)] {
            let frame =
                render_lock_v2_scene("composer-file-chip", width, height).expect("file-chip");
            assert!(
                frame.plain.contains('@')
                    && (frame.plain.contains("composer.rs")
                        || frame.plain.contains(FILE_CHIP_TOKEN)),
                "file-chip must show @ + path at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("explain"),
                "file-chip prompt prefix missing at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                !frame.plain.contains("rateLimits.json") && !frame.plain.contains("Type to search"),
                "file-chip must not open the files-picker menu:\n{}",
                frame.plain
            );
            assert!(!banned_competitors(&frame.plain), "{}", frame.plain);
            if width >= 80 {
                assert_ne!(
                    frame.ansi,
                    render_lock_v2_scene("files-picker", width, height)
                        .expect("files-picker")
                        .ansi,
                    "composer-file-chip must differ from files-picker"
                );
                assert_ne!(
                    frame.ansi,
                    render_lock_v2_scene("composer-typing", width, height)
                        .expect("composer-typing")
                        .ansi,
                    "composer-file-chip must differ from composer-typing"
                );
            }
            let mut accent_at = false;
            for y in 0..height {
                for x in 0..width {
                    let cell = &frame.buffer[(x, y)];
                    if cell.fg == ACCENT && cell.symbol().contains('@') {
                        accent_at = true;
                    }
                }
            }
            assert!(
                accent_at,
                "file-chip should paint accent on the @ token at {width}x{height}"
            );
        }
    }

    #[test]
    fn undo_sheet_lists_undo_redo_rewind() {
        for (width, height) in [(120u16, 40u16), (40u16, 12u16)] {
            let frame = render_lock_v2_scene("undo-sheet", width, height).expect("undo-sheet");
            assert!(
                frame.plain.contains("Undo") || frame.plain.contains("/undo"),
                "undo-sheet missing Undo at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("Rewind") || frame.plain.contains("Redo"),
                "undo-sheet missing Rewind/Redo at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                !frame
                    .plain
                    .to_ascii_lowercase()
                    .contains("clear conversation")
                    && !frame.plain.contains("Clear this conversation"),
                "undo-sheet must not reuse clear-confirm copy:\n{}",
                frame.plain
            );
            assert!(!banned_competitors(&frame.plain), "{}", frame.plain);
            if width >= 80 {
                assert_ne!(
                    frame.ansi,
                    render_lock_v2_scene("clear-confirm", width, height)
                        .expect("clear-confirm")
                        .ansi,
                    "undo-sheet must differ from clear-confirm"
                );
                assert_ne!(
                    frame.ansi,
                    render_lock_v2_scene("resume-picker", width, height)
                        .expect("resume-picker")
                        .ansi,
                    "undo-sheet must differ from resume-picker"
                );
            }
            let mut sel = false;
            for y in 0..height {
                for x in 0..width {
                    if frame.buffer[(x, y)].bg == SELECTION_BG {
                        sel = true;
                    }
                }
            }
            assert!(sel, "undo-sheet focused row must paint SELECTION_BG");
            assert!(
                frame.plain.contains("confirm") && frame.plain.contains("cancel"),
                "undo-sheet footer must be confirm/cancel, not Typed, at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                !frame.plain.contains("Alt+Enter"),
                "undo-sheet footer must not be the Typed composer strip:\n{}",
                frame.plain
            );
            if width >= 80 {
                assert!(
                    frame.plain.contains("tell me about yourself")
                        || frame.plain.contains("Worked for"),
                    "wide undo-sheet must paint the conversation backdrop:\n{}",
                    frame.plain
                );
            }
        }
    }

    #[test]
    fn parity_frames_are_unique_from_each_other() {
        let mut seen = HashSet::new();
        for id in PARITY_IDS {
            let frame = render_lock_v2_scene(id, 120, 40).expect(id);
            assert!(
                seen.insert(frame.ansi.clone()),
                "{id} collided with a sibling at 120x40"
            );
        }
        assert_eq!(seen.len(), PARITY_IDS.len());
    }
}
