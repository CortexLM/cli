//! Lock v2 scenes for COR-18 / COR-225 / COR-227 / COR-228.
//!
//! Runtime MockTerminal boards only — Designer PNGs on other PRs stay untouched.
//! `/share` / `/unshare` (COR-226) is not a scene here.

use cortex_core::widgets::Message;

use crate::app::AppState;
use crate::interactive::builders::build_theme_selector;
use crate::lock_v2_scenes::*;

/// Designed boards (all sizes). `handoff-confirm` is also at 40×12.
pub const DESIGNED_NARROW_IDS: &[&str] = &[
    "theme-picker",
    "session-fork",
    "init-agents",
    "custom-commands",
    "hooks-lifecycle",
    "handoff-confirm",
];

/// Apply a designed lock scene. Returns `false` when `id` is not one of these.
pub fn apply_designed_scene(id: &str, state: &mut AppState, width: u16) -> bool {
    if !DESIGNED_NARROW_IDS.contains(&id) {
        return false;
    }
    let narrow = width <= 40;
    match id {
        "theme-picker" => {
            resumed(state);
            state.input.set_text("/theme");
            let mut interactive = build_theme_selector(Some("dark"));
            interactive.hovered = Some(1);
            state.enter_interactive_mode(interactive);
        }
        "handoff-confirm" => {
            resumed(state);
            if !narrow {
                state.add_message(
                    Message::user("& ship the lock boards on Cortex Cloud")
                        .with_timestamp("02:40 PM"),
                );
            }
            state.input.set_text("/handoff");
            let mut interactive = radios(
                "Handoff",
                &[
                    (
                        "cloud",
                        "1 Cortex Cloud",
                        if narrow {
                            "Chat · Code · Bot"
                        } else {
                            "Chat · Code · Bot — this CLI session stays here"
                        },
                    ),
                    (
                        "stay",
                        if narrow {
                            "2 Stay on CLI"
                        } else {
                            "2 Stay on this CLI session"
                        },
                        "keep running locally",
                    ),
                ],
                0,
                None,
            );
            interactive.hovered = Some(0);
            state.enter_interactive_mode(interactive);
        }
        "session-fork" => {
            resumed(state);
            state.add_message(
                Message::user("fork from this reply and keep going").with_timestamp("11:08 AM"),
            );
            state.input.set_text("/fork");
            let desc_keep = if narrow {
                "this thread unchanged"
            } else {
                "original session unchanged"
            };
            state.enter_interactive_mode(radios(
                "Fork session",
                &[
                    (
                        "fork",
                        if narrow {
                            "1 Fork now"
                        } else {
                            "1 Fork this session"
                        },
                        if narrow {
                            "new branch · optional name"
                        } else {
                            "new session · optional name"
                        },
                    ),
                    ("keep", "2 Stay here", desc_keep),
                ],
                0,
                None,
            ));
        }
        "init-agents" => {
            resumed(state);
            state.add_message(Message::user("/init").with_timestamp("09:12 AM"));
            state.add_message(
                Message::assistant(if narrow {
                    "Set up AGENTS.md for this repo — accept the diff."
                } else {
                    "Set up AGENTS.md from the built-in project template. Review the diff, then write it to the repo."
                })
                .with_timestamp("09:12 AM"),
            );
            state.tool_calls = vec![tool(
                "init-agents",
                "edit",
                serde_json::json!({"path": "AGENTS.md"}),
                crate::views::tool_call::ToolStatus::Completed,
                if narrow {
                    "@@ AGENTS.md\n+# Cortex CLI\n+Chat | Code | Bot"
                } else {
                    "@@ AGENTS.md\n+# Cortex CLI / Cortex Code\n+\n+Chat | Code | Bot in this repository.\n+Prefer `cargo test` and lock_v2 scenes."
                },
                "+12",
                1,
            )];
            state.enter_interactive_mode(radios(
                "Write AGENTS.md?",
                &[
                    (
                        "write",
                        if narrow {
                            "1 Write AGENTS.md"
                        } else {
                            "1 Write AGENTS.md to the repo"
                        },
                        "accept the diff",
                    ),
                    ("cancel", "2 Cancel", "leave the file untouched"),
                ],
                0,
                None,
            ));
        }
        "custom-commands" => {
            resumed(state);
            state.input.set_text("/commands");
            state.enter_interactive_mode(radios(
                "Custom commands",
                &[
                    (
                        "pr",
                        "/pr",
                        if narrow {
                            ".cortex/commands · $ARGUMENTS"
                        } else {
                            ".cortex/commands/pr.md · $ARGUMENTS"
                        },
                    ),
                    (
                        "ship",
                        "/ship",
                        if narrow {
                            ".cortex/commands"
                        } else {
                            ".cortex/commands/ship.md · prompt template"
                        },
                    ),
                    (
                        "review",
                        "/review-lock",
                        if narrow {
                            "repo SKILL.md"
                        } else {
                            ".cortex/skills/review/SKILL.md"
                        },
                    ),
                ],
                0,
                Some(0),
            ));
        }
        "hooks-lifecycle" => {
            resumed(state);
            state.input.set_text("/hooks");
            state.add_message(Message::system(if narrow {
                "A hook is never consent. Output is capped and redacted."
            } else {
                "`.cortex/hooks.json` — a hook is never consent. Output is capped and redacted."
            }));
            state.enter_interactive_mode(radios(
                "Hooks",
                &[
                    ("pre", "pre_tool_use", "before a local tool"),
                    ("post", "post_tool_use", "after a local tool"),
                    ("stop", "stop", "when the turn stops"),
                    (
                        "start",
                        "session_start",
                        if narrow {
                            "plugin event"
                        } else {
                            "plugin · terminal notification"
                        },
                    ),
                    (
                        "end",
                        "session_end",
                        if narrow {
                            "session close"
                        } else {
                            "lifecycle event — never consent"
                        },
                    ),
                ],
                0,
                None,
            ));
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock_v2::render_lock_v2_scene;
    use crate::lock_v2_ids::{LOCK_V2_NARROW_IDS, LOCK_V2_WIDE_IDS};
    use cortex_core::style::ACCENT;

    const SIZES: [(u16, u16); 2] = [(120, 40), (40, 12)];

    #[test]
    fn designed_scene_ids_are_registered() {
        for id in DESIGNED_NARROW_IDS {
            assert!(LOCK_V2_WIDE_IDS.contains(id), "{id} missing from wide");
            assert!(LOCK_V2_NARROW_IDS.contains(id), "{id} missing from narrow");
        }
        assert!(!LOCK_V2_WIDE_IDS.contains(&"share-link"));
        assert!(!LOCK_V2_WIDE_IDS.contains(&"unshare"));
        assert_eq!(LOCK_V2_WIDE_IDS.len(), 95);
        assert_eq!(LOCK_V2_NARROW_IDS.len(), 49);
    }

    #[test]
    fn theme_picker_is_cortex_branded() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("theme-picker", width, height).expect("theme");
            let plain = &frame.plain;
            assert!(plain.contains("Cortex Night"), "{plain}");
            assert!(plain.contains("Cortex Day"), "{plain}");
            assert!(plain.contains("Ocean Dark"), "{plain}");
            assert!(plain.contains("Monokai"), "{plain}");
            assert!(!plain.contains("Midnight"), "{plain}");
            assert!(!plain.contains("Monochrome"), "{plain}");
            assert!(!plain.contains("Grok"), "{plain}");
            let mut accent = false;
            for y in 0..height {
                for x in 0..width {
                    if frame.buffer[(x, y)].fg == ACCENT {
                        accent = true;
                    }
                }
            }
            assert!(
                accent,
                "theme-picker focus uses #1F4945 at {width}x{height}"
            );
        }
    }

    #[test]
    fn handoff_slash_command_is_registered() {
        let registry = crate::commands::CommandRegistry::default();
        assert!(
            registry.exists("handoff"),
            "/handoff must be a registered command"
        );
    }

    #[test]
    fn handoff_confirm_is_cli_products_only() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("handoff-confirm", width, height).expect("handoff");
            assert!(
                frame.plain.contains("Cortex Cloud"),
                "{width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("Chat"),
                "{width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("Stay on") && frame.plain.contains("CLI"),
                "{width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                !frame.plain.contains("Handed off to Cortex Cloud"),
                "{width}x{height}:\n{}",
                frame.plain
            );
            assert!(!frame.plain.contains("ag_4f2a"), "{}", frame.plain);
            assert!(!frame.plain.contains("Arcade"), "{}", frame.plain);
        }
    }

    #[test]
    fn session_fork_is_distinct_from_resume() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("session-fork", width, height).expect("fork");
            assert!(
                frame.plain.contains("Fork"),
                "fork copy at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                !frame.plain.contains("Resume"),
                "must not look like resume-picker:\n{}",
                frame.plain
            );
        }
    }

    #[test]
    fn init_agents_shows_diff_to_accept() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("init-agents", width, height).expect("init");
            assert!(frame.plain.contains("AGENTS.md"), "{}", frame.plain);
            assert!(
                frame.plain.contains("Write AGENTS.md") || frame.plain.contains("1 Write"),
                "{}",
                frame.plain
            );
            assert!(frame.plain.contains("Cancel"), "{}", frame.plain);
            assert!(!frame.plain.contains("share"), "{}", frame.plain);
            assert!(
                !frame.plain.contains("bundled skill"),
                "init copy must not claim a bundled skill:\n{}",
                frame.plain
            );
            if width > 40 {
                assert!(
                    frame.plain.contains("built-in project template"),
                    "{}",
                    frame.plain
                );
            }
        }
    }

    #[test]
    fn custom_commands_come_from_cortex_dir() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("custom-commands", width, height).expect("cmds");
            assert!(frame.plain.contains("/pr"), "{}", frame.plain);
            assert!(
                frame.plain.contains(".cortex/commands") || frame.plain.contains("$ARGUMENTS"),
                "{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("SKILL.md") || frame.plain.contains("/review-lock"),
                "{}",
                frame.plain
            );
        }
    }

    #[test]
    fn hooks_lifecycle_is_not_consent() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("hooks-lifecycle", width, height).expect("hooks");
            assert!(frame.plain.contains("pre_tool_use"), "{}", frame.plain);
            assert!(frame.plain.contains("post_tool_use"), "{}", frame.plain);
            assert!(frame.plain.contains("session_start"), "{}", frame.plain);
            assert!(
                frame.plain.contains("never consent") || frame.plain.contains("not consent"),
                "{}",
                frame.plain
            );
            assert!(!frame.plain.contains("cert"), "{}", frame.plain);
            assert!(!frame.plain.contains("badge"), "{}", frame.plain);
        }
    }
}
