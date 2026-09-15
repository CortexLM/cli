//! COR-35 batch lock v2 scenes (COR-362 … COR-376).
//!
//! Each board locks one shipped CLI/TUI surface from the COR-35 batch:
//! headless CI, the permission DSL, file-checkpoint rewind, the CI cookbook,
//! JSON Schema output, cloud teleport with apply-back, review-only runs, the
//! plugin marketplace, the sandbox network allowlist, the auto-approval
//! classifier, PR apply-back, the ACP editor handshake, and the stdin
//! multi-turn stream.
//!
//! Split out of [`crate::lock_v2_boards`] so adding these boards does not grow
//! that file past the source-policy line-count baseline. Copy stays Cortex-only:
//! no competitor or provider names.

use cortex_core::widgets::Message;

use crate::app::AppState;
use crate::interactive::builders::build_question_prompt;
use crate::lock_v2_scenes::{conversation, radios, resumed};

/// Boards captured at both 120×40 and 40×12.
pub const COR35_NARROW_IDS: &[&str] = &[
    "bare-ci",
    "permission-rules",
    "checkpoint-rewind",
    "sandbox-allowlist",
    "pr-apply-back",
    "acp-editor",
    "browser-use",
    "stdin-multiturn",
];

/// Boards captured at 120×40 only — pickers that need the wide modal.
pub const COR35_WIDE_IDS: &[&str] = &[
    "ci-cookbook",
    "json-schema",
    "cloud-teleport",
    "review-only",
    "plugin-marketplace",
    "auto-approval",
];

/// Every COR-35 board at `width`: wide-only first, then the shared set.
///
/// The id lists in [`crate::lock_v2_ids`] stay the source of truth for what is
/// registered; this is the render order used by the tests.
#[cfg(test)]
pub fn cor35_ids(width: u16) -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = if width <= 40 {
        COR35_NARROW_IDS.to_vec()
    } else {
        COR35_WIDE_IDS.to_vec()
    };
    if width > 40 {
        ids.extend_from_slice(COR35_NARROW_IDS);
    }
    ids
}

/// True when `id` belongs to this batch.
pub fn is_cor35_id(id: &str) -> bool {
    COR35_WIDE_IDS.contains(&id) || COR35_NARROW_IDS.contains(&id)
}

/// Apply a COR-35 lock scene. Returns `false` when `id` is not one of ours.
pub fn apply_cor35_scene(id: &str, state: &mut AppState, width: u16) -> bool {
    if !is_cor35_id(id) {
        return false;
    }
    let narrow = width <= 40;
    match id {
        "bare-ci" => apply_bare_ci(state, narrow),
        "ci-cookbook" => apply_ci_cookbook(state),
        "permission-rules" => apply_permission_rules(state, narrow),
        "checkpoint-rewind" => apply_checkpoint_rewind(state, narrow),
        "json-schema" => apply_json_schema(state),
        "cloud-teleport" => apply_cloud_teleport(state),
        "review-only" => apply_review_only(state),
        "plugin-marketplace" => apply_plugin_marketplace(state),
        "sandbox-allowlist" => apply_sandbox_allowlist(state, narrow),
        "auto-approval" => apply_auto_approval(state),
        "pr-apply-back" => apply_pr_apply_back(state, narrow),
        "acp-editor" => apply_acp_editor(state, narrow),
        "browser-use" => apply_browser_use(state, narrow),
        "stdin-multiturn" => apply_stdin_multiturn(state, narrow),
        _ => return false,
    }
    true
}

fn apply_bare_ci(state: &mut AppState, narrow: bool) {
    resumed(state);
    state.add_message(Message::user("cortex run --bare --ephemeral").with_timestamp("09:02 AM"));
    state.add_message(Message::system(if narrow {
        "bare run · no session file · no banner"
    } else {
        "Bare run — no session file, no banners, no alternate screen. Exit code is the result."
    }));
    if !narrow {
        state.add_message(Message::system(
            "cortex run --bare --ephemeral \"review src/auth\" --format json",
        ));
    }
}

fn apply_ci_cookbook(state: &mut AppState) {
    conversation(state);
    state.input.set_text("/help ci");
    state.add_message(Message::system(
        "CI cookbook — docs/guides/ci.md. Export CORTEX_API_KEY from your CI secret store; never paste the value into a workflow file.",
    ));
    state.enter_interactive_mode(radios(
        "CI cookbook",
        &[
            (
                "gha",
                "GitHub Actions",
                "CORTEX_API_KEY from secrets · cortex run --bare",
            ),
            (
                "gl",
                "GitLab CI",
                "masked variable · cortex exec --auto read-only",
            ),
            (
                "other",
                "Any CI",
                "export CORTEX_API_KEY · cortex run --bare --format json",
            ),
            (
                "docs",
                "docs/guides/ci.md",
                "full cookbook · exit codes · artifacts",
            ),
        ],
        0,
        None,
    ));
}

fn apply_permission_rules(state: &mut AppState, narrow: bool) {
    resumed(state);
    state.input.set_text("/permissions rules");
    let rules = crate::permissions::PermissionRules::parse(if narrow {
        "allow = [\"git status*\"]\nask = [\"cargo test*\"]\ndeny = [\"rm -rf *\"]\n"
    } else {
        "allow = [\"git status*\"]\nask = [\"cargo test*\"]\ndeny = [\"rm -rf *\", \"curl * | bash*\"]\n"
    })
    .expect("shipped permission rules parse");
    // The picker banner already carries the rule-order copy.
    state.enter_interactive_mode(crate::interactive::builders::build_permission_rules(
        &rules, 0, None,
    ));
}

fn apply_checkpoint_rewind(state: &mut AppState, narrow: bool) {
    resumed(state);
    state.input.set_text("/rewind");
    state.add_message(
        Message::user("move the model chip into the composer border").with_timestamp("04:11 PM"),
    );
    state.add_message(Message::system(if narrow {
        "checkpoint · 3 files before the edit"
    } else {
        "Checkpoint — 3 files captured before the edit. Rewinding restores them."
    }));
    let rows: &[(&str, &str, &str)] = if narrow {
        &[
            ("last", "1 Undo last turn", "restore files"),
            ("point", "2 Rewind to checkpoint", "pick a point"),
            ("keep", "3 Keep files", "conversation only"),
        ]
    } else {
        &[
            (
                "last",
                "1 Undo last turn",
                "restore the files this turn changed",
            ),
            (
                "point",
                "2 Rewind to checkpoint",
                "pick a checkpoint from this session",
            ),
            (
                "keep",
                "3 Keep files, rewind conversation",
                "leave the working tree alone",
            ),
        ]
    };
    state.enter_interactive_mode(radios("Rewind", rows, 0, None));
}

fn apply_json_schema(state: &mut AppState) {
    conversation(state);
    state.input.set_text("/help json");
    state.add_message(Message::system(
        "JSON Schema output — every result document is validated against the shipped schema before it is printed.",
    ));
    state.enter_interactive_mode(radios(
        "JSON Schema output",
        &[
            (
                "run",
                "cortex run --format json --json-schema",
                "one result document, schema-checked",
            ),
            (
                "exec",
                "cortex exec --output-format json --json-schema",
                "headless result, same schema",
            ),
            (
                "print",
                "cortex schema print run-result",
                "print the schema and exit",
            ),
        ],
        0,
        None,
    ));
}

fn apply_cloud_teleport(state: &mut AppState) {
    resumed(state);
    state.add_message(
        Message::user("& fix the flaky login redirect test").with_timestamp("03:02 PM"),
    );
    state.add_message(
        Message::assistant(
            "↑ Teleported to Cortex Cloud\nbranch   cortex/fix-login-redirect\nturn     2 of 5 · running\nfollow   /jobs right here.",
        )
        .with_timestamp("03:02 PM")
        .with_thought_secs(1.2),
    );
    state.add_message(Message::system(
        "The cloud turn edits its own worktree. Follow it with /jobs right here.",
    ));
}

fn apply_review_only(state: &mut AppState) {
    resumed(state);
    state.input.set_text("/review");
    state.add_message(Message::system(
        "Review-only — reads the diff, never writes. No edits, no commands.",
    ));
    state.enter_interactive_mode(radios(
        "Review",
        &[
            ("diff", "Review the working diff", "read-only"),
            ("branch", "Review this branch", "against main"),
            (
                "pr",
                "Review a pull request",
                "cortex exec --review-pr 128 · read-only",
            ),
        ],
        0,
        None,
    ));
}

fn apply_plugin_marketplace(state: &mut AppState) {
    resumed(state);
    state.input.set_text("/plugins");
    // Use the same builder the live `/plugins` sheet opens.
    state.enter_interactive_mode(crate::interactive::builders::build_plugin_marketplace(
        &[
            ("cortex-review".to_string(), "0.4.1".to_string()),
            ("mermaid-preview".to_string(), "0.2.0".to_string()),
        ],
        0,
        None,
    ));
}

fn apply_sandbox_allowlist(state: &mut AppState, narrow: bool) {
    resumed(state);
    state.input.set_text("/sandbox network");
    let mut allowlist = crate::sandbox_allowlist::SandboxAllowlist::default();
    allowlist.add("crates.io").expect("shipped entry");
    allowlist.add("github.com").expect("shipped entry");
    state.add_message(Message::system(if narrow {
        "network allowlist · 2 domains · rest blocked"
    } else {
        "Network is allowlisted — anything off the list fails closed and asks."
    }));
    state.enter_interactive_mode(crate::interactive::builders::build_sandbox_allowlist(
        &allowlist, 0, None,
    ));
}

fn apply_auto_approval(state: &mut AppState) {
    resumed(state);
    state.input.set_text("/permissions");
    state.add_message(Message::system(
        "Auto-approval classifier — reads and safe commands pass; anything else asks.",
    ));
    state.enter_interactive_mode(radios(
        "Auto-approval",
        &[
            ("auto", "Auto-approve safe reads", "Glob · Grep · Read"),
            ("ask", "Ask before commands", "cargo test · npm install"),
            ("never", "Never auto-approve", "every tool call asks"),
        ],
        0,
        None,
    ));
}

fn apply_pr_apply_back(state: &mut AppState, narrow: bool) {
    resumed(state);
    state.add_message(Message::user("cortex pr 128 --apply").with_timestamp("05:40 PM"));
    state.add_message(Message::system(if narrow {
        "PR 128 · 4 files · clean tree"
    } else {
        "PR 128 — 4 files, +86 −14. Working tree is clean; the patch applies directly."
    }));
    state.enter_interactive_mode(build_question_prompt(
        "Apply PR 128?",
        if narrow {
            &[
                ("apply", "1 Apply the patch", "4 files"),
                ("checkout", "2 Check out the branch", "keep the tree"),
                ("cancel", "3 Cancel", "no changes"),
            ]
        } else {
            &[
                ("apply", "1 Apply the patch", "4 files · +86 −14"),
                (
                    "checkout",
                    "2 Check out the branch",
                    "switch and keep local commits",
                ),
                ("cancel", "3 Cancel", "nothing is written"),
            ]
        },
        0,
    ));
}

fn apply_acp_editor(state: &mut AppState, narrow: bool) {
    resumed(state);
    state.add_message(Message::user("/ide").with_timestamp("10:44 AM"));
    state.add_message(Message::system(if narrow {
        "ACP · stdio · approvals unchanged"
    } else {
        "ACP over stdio — the editor drives this session; tools stay behind the same approvals and sandbox."
    }));
    state.enter_interactive_mode(radios(
        "Editor (ACP)",
        &[
            ("connect", "Connect an editor", "cortex acp · stdio"),
            (
                "session",
                "Share this session",
                if narrow {
                    "same approvals"
                } else {
                    "editor reads files through the same sandbox"
                },
            ),
            ("stop", "Disconnect", "the CLI keeps running"),
        ],
        0,
        None,
    ));
}

fn apply_browser_use(state: &mut AppState, narrow: bool) {
    resumed(state);
    state.input.set_text("/browser");
    let capability = crate::browser_use::resolve_capability(&[]);
    state.add_message(Message::system(if narrow {
        crate::browser_use::NO_BUILTIN_TOOL_NOTE_NARROW.to_string()
    } else {
        capability.status_line()
    }));
    let rows: &[(&str, &str, &str)] = if narrow {
        &[
            ("connect", "1 Connect an MCP server", "browser tools"),
            ("runtime", "2 Computer runtime", "Cloud · This PC · SSH"),
            ("cancel", "3 Cancel", "nothing changes"),
        ]
    } else {
        &[
            (
                "connect",
                "1 Connect a browser MCP server",
                "the CLI ships no browser tool",
            ),
            (
                "runtime",
                "2 Computer runtime",
                "where tools run — not browser automation",
            ),
            ("cancel", "3 Cancel", "no server is installed"),
        ]
    };
    state.enter_interactive_mode(radios("Browser", rows, 0, None));
}

fn apply_stdin_multiturn(state: &mut AppState, narrow: bool) {
    resumed(state);
    state.add_message(
        Message::user("cortex exec --input-format stream-jsonl").with_timestamp("11:06 AM"),
    );
    state.add_message(Message::system(if narrow {
        "stdin · 3 turns · one stream"
    } else {
        "stdin — one JSON line per turn, one stream out. The connection outlives each turn."
    }));
    if !narrow {
        state.add_message(Message::system("{\"text\":\"add a retry helper\"}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock_v2::{LOCK_V2_NARROW_IDS, LOCK_V2_WIDE_IDS, render_lock_v2_scene};
    use cortex_core::style::{ACCENT, SELECTION_BG};
    use std::collections::HashSet;

    const SIZES: [(u16, u16); 2] = [(120, 40), (40, 12)];

    fn banned_names(plain: &str) -> bool {
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
            "grok",
            "rakazo",
        ]
        .iter()
        .any(|n| lower.contains(n))
    }

    #[test]
    fn cor35_ids_are_registered_at_the_right_sizes() {
        assert_eq!(COR35_NARROW_IDS.len(), 8);
        assert_eq!(COR35_WIDE_IDS.len(), 6);
        for id in COR35_NARROW_IDS {
            assert!(LOCK_V2_WIDE_IDS.contains(id), "{id} missing from wide list");
            assert!(
                LOCK_V2_NARROW_IDS.contains(id),
                "{id} missing from narrow list"
            );
        }
        for id in COR35_WIDE_IDS {
            assert!(LOCK_V2_WIDE_IDS.contains(id), "{id} missing from wide list");
            assert!(
                !LOCK_V2_NARROW_IDS.contains(id),
                "{id} is wide-only and must not be in the narrow set"
            );
        }
        assert_eq!(cor35_ids(120).len(), 14);
        assert_eq!(cor35_ids(40).len(), 8);
    }
    #[test]
    fn every_cor35_board_is_named_and_cortex_only() {
        for (width, height) in SIZES {
            for id in cor35_ids(width) {
                let frame = render_lock_v2_scene(id, width, height)
                    .unwrap_or_else(|e| panic!("{id} at {width}x{height}: {e}"));
                assert!(
                    !frame.plain.trim().is_empty(),
                    "{id} rendered an empty frame"
                );
                assert!(
                    !banned_names(&frame.plain),
                    "{id} at {width}x{height} names a competitor:\n{}",
                    frame.plain
                );
            }
        }
    }

    #[test]
    fn bare_ci_says_it_leaves_no_session_behind() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("bare-ci", width, height).expect("bare-ci");
            assert!(
                frame.plain.contains("bare") || frame.plain.contains("--bare"),
                "bare-ci must name the bare run at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("no session file") || frame.plain.contains("ephemeral"),
                "bare-ci must state the session is not persisted at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                !frame.plain.contains("Choose an option above"),
                "bare-ci is not an approval sheet:\n{}",
                frame.plain
            );
        }
    }

    #[test]
    fn ci_cookbook_leads_with_secret_via_env() {
        let frame = render_lock_v2_scene("ci-cookbook", 120, 40).expect("cookbook");
        assert!(frame.plain.contains("CI cookbook"), "{}", frame.plain);
        assert!(frame.plain.contains("GitHub Actions"), "{}", frame.plain);
        assert!(frame.plain.contains("GitLab"), "{}", frame.plain);
        assert!(
            frame.plain.contains("CORTEX_API_KEY"),
            "cookbook must pass the secret through the environment:\n{}",
            frame.plain
        );
        assert!(
            !frame.plain.contains("sk-") && !frame.plain.contains("Bearer "),
            "cookbook must never print a secret value:\n{}",
            frame.plain
        );
    }

    #[test]
    fn permission_rules_use_a_project_file_with_deny_wins() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("permission-rules", width, height).expect("rules");
            assert!(
                frame.plain.contains(".cortex/permissions.toml")
                    || frame.plain.contains("permissions.toml"),
                "permission-rules must name the project file at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("allow") && frame.plain.contains("deny"),
                "permission-rules must show allow and deny rules at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("rm -rf"),
                "permission-rules must show a denied command at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                !frame.plain.contains("$ npm install"),
                "permission-rules is not the exec approval prompt:\n{}",
                frame.plain
            );
            assert_ne!(
                frame.ansi,
                render_lock_v2_scene("permissions-picker", width, height)
                    .expect("picker")
                    .ansi,
                "permission-rules must differ from the permissions picker"
            );
        }
    }

    #[test]
    fn checkpoint_rewind_restores_files() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("checkpoint-rewind", width, height).expect("rewind");
            assert!(
                frame.plain.contains("checkpoint") || frame.plain.contains("Checkpoint"),
                "checkpoint-rewind must name the checkpoint at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("restore") || frame.plain.contains("Undo"),
                "checkpoint-rewind must offer a restore at {width}x{height}:\n{}",
                frame.plain
            );
            assert_ne!(
                frame.ansi,
                render_lock_v2_scene("undo-sheet", width, height)
                    .expect("undo-sheet")
                    .ansi,
                "checkpoint-rewind must differ from the undo sheet"
            );
        }
    }

    #[test]
    fn json_schema_board_names_the_schema_commands() {
        let frame = render_lock_v2_scene("json-schema", 120, 40).expect("schema");
        assert!(frame.plain.contains("JSON Schema"), "{}", frame.plain);
        assert!(frame.plain.contains("--json-schema"), "{}", frame.plain);
        assert!(frame.plain.contains("--format json"), "{}", frame.plain);
    }

    #[test]
    fn cloud_teleport_and_apply_back_are_named() {
        let frame = render_lock_v2_scene("cloud-teleport", 120, 40).expect("teleport");
        assert!(frame.plain.contains("Teleported"), "{}", frame.plain);
        assert!(
            frame.plain.contains("Cortex Cloud"),
            "teleport must stay Cortex-branded:\n{}",
            frame.plain
        );
        assert!(
            frame.plain.contains("/jobs") || frame.plain.contains("applies the diff"),
            "teleport must say how the work comes back:\n{}",
            frame.plain
        );
        assert!(
            !frame.plain.contains("/teleport"),
            "teleport must not name a command that does not exist:\n{}",
            frame.plain
        );
        assert!(
            !frame.plain.contains("Handed off to Cortex Cloud"),
            "teleport replaces the one-way handoff board:\n{}",
            frame.plain
        );
    }

    #[test]
    fn review_only_promises_no_writes() {
        let frame = render_lock_v2_scene("review-only", 120, 40).expect("review");
        assert!(frame.plain.contains("Review-only"), "{}", frame.plain);
        assert!(
            frame.plain.contains("never writes") || frame.plain.contains("read-only"),
            "review-only must state it does not write:\n{}",
            frame.plain
        );
        assert!(
            frame.plain.contains("--review-pr") || frame.plain.contains("--pr"),
            "review-only must cover a pull request:\n{}",
            frame.plain
        );
    }

    #[test]
    fn plugin_marketplace_is_signed_and_cortex_hosted() {
        let frame = render_lock_v2_scene("plugin-marketplace", 120, 40).expect("marketplace");
        assert!(
            frame.plain.contains("cortex.foundation"),
            "marketplace must use the Cortex origin:\n{}",
            frame.plain
        );
        assert!(frame.plain.contains("signed"), "{}", frame.plain);
        assert!(frame.plain.contains("installed"), "{}", frame.plain);
        assert!(
            !frame.plain.contains("jira.cortex"),
            "marketplace copy must not invent a vendor host:\n{}",
            frame.plain
        );
    }

    #[test]
    fn sandbox_allowlist_lists_domains_and_fails_closed() {
        for (width, height) in SIZES {
            let frame =
                render_lock_v2_scene("sandbox-allowlist", width, height).expect("allowlist");
            assert!(
                frame.plain.contains("crates.io") && frame.plain.contains("github.com"),
                "allowlist must list the allowed domains at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("allowlist") || frame.plain.contains("allowed"),
                "allowlist board must name the allowlist at {width}x{height}:\n{}",
                frame.plain
            );
            assert_ne!(
                frame.ansi,
                render_lock_v2_scene("sandbox", width, height)
                    .expect("sandbox")
                    .ansi,
                "sandbox-allowlist must differ from the sandbox picker"
            );
        }
    }

    #[test]
    fn auto_approval_classifier_names_what_passes() {
        let frame = render_lock_v2_scene("auto-approval", 120, 40).expect("classifier");
        assert!(frame.plain.contains("Auto-approval"), "{}", frame.plain);
        assert!(frame.plain.contains("Glob"), "{}", frame.plain);
        assert!(
            frame.plain.contains("cargo test"),
            "classifier must show a command that still asks:\n{}",
            frame.plain
        );
        assert!(
            !frame.plain.contains("cert") && !frame.plain.contains("badge"),
            "classifier must not claim a certification:\n{}",
            frame.plain
        );
    }

    #[test]
    fn pr_apply_back_is_confirmable_and_cancellable() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("pr-apply-back", width, height).expect("apply-back");
            assert!(
                frame.plain.contains("PR 128") || frame.plain.contains("Apply PR"),
                "pr-apply-back must name the PR at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("Apply the patch") || frame.plain.contains("1 Apply"),
                "pr-apply-back must offer the patch at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("Cancel"),
                "pr-apply-back must be cancellable at {width}x{height}:\n{}",
                frame.plain
            );
            assert_ne!(
                frame.ansi,
                render_lock_v2_scene("plan-confirm", width, height)
                    .expect("plan-confirm")
                    .ansi,
                "pr-apply-back must differ from plan-confirm"
            );
        }
    }

    #[test]
    fn acp_editor_board_is_stdio_and_approval_bound() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("acp-editor", width, height).expect("acp");
            assert!(frame.plain.contains("ACP"), "{}", frame.plain);
            assert!(
                frame.plain.contains("cortex acp") || frame.plain.contains("stdio"),
                "acp-editor must name the stdio entrypoint at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("approval") || frame.plain.contains("sandbox"),
                "acp-editor must keep approvals in the path at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                !frame.plain.contains("extension"),
                "acp-editor must not claim a packaged extension:\n{}",
                frame.plain
            );
        }
    }

    #[test]
    fn browser_use_board_claims_no_builtin_tool() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("browser-use", width, height).expect("browser");
            assert!(
                frame.plain.contains("browser") || frame.plain.contains("Browser"),
                "browser-use must name the surface at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("MCP"),
                "browser-use must say the capability comes from an MCP server at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("no browser tool") || frame.plain.contains("ships no browser"),
                "browser-use must not imply a built-in tool at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("Computer runtime") || frame.plain.contains("not browser"),
                "browser-use must disambiguate the Computer runtime at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                !frame.plain.contains("extension"),
                "browser-use must not claim an extension:\n{}",
                frame.plain
            );
            assert_ne!(
                frame.ansi,
                render_lock_v2_scene("acp-editor", width, height)
                    .expect("acp")
                    .ansi,
                "browser-use must differ from acp-editor"
            );
            assert_ne!(
                frame.ansi,
                render_lock_v2_scene("computer-cloud-default", width, height)
                    .expect("computer")
                    .ansi,
                "browser-use must differ from the Computer runtime board"
            );
        }
    }

    #[test]
    fn stdin_multiturn_stream_outlives_a_turn() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("stdin-multiturn", width, height).expect("stdin");
            assert!(
                frame.plain.contains("stdin"),
                "stdin-multiturn must name stdin at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("turn"),
                "stdin-multiturn must be multi-turn at {width}x{height}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("stream-jsonl") || frame.plain.contains("stream"),
                "stdin-multiturn must name the stream format at {width}x{height}:\n{}",
                frame.plain
            );
        }
    }

    #[test]
    fn cor35_frames_are_unique_at_every_size() {
        for (width, height) in SIZES {
            let mut seen: HashSet<String> = HashSet::new();
            for id in cor35_ids(width) {
                let frame = render_lock_v2_scene(id, width, height).expect(id);
                assert!(
                    seen.insert(frame.ansi.clone()),
                    "{id} collided with a sibling at {width}x{height}"
                );
            }
            assert_eq!(seen.len(), cor35_ids(width).len());
        }
    }

    #[test]
    fn every_cor35_board_stays_off_the_retired_palette() {
        // SPEC §1 retires thinking gold, mint, cyan, and the violet wash. The
        // shared audit covers those; the SPEC cyan is checked here as well
        // because `count_palette` only recognises `#00FFFF` for cyan.
        const SPEC_CYAN: ratatui::style::Color = ratatui::style::Color::Rgb(0x7D, 0xD3, 0xFC);
        for (width, height) in SIZES {
            for id in cor35_ids(width) {
                let frame = render_lock_v2_scene(id, width, height).expect(id);
                let counts = crate::lock_palette::count_palette(&frame.buffer);
                assert!(
                    !counts.has_banned(),
                    "{id} at {width}x{height} paints retired chrome: violet={} wash={} gold={} mint={} cyan={}",
                    counts.violet_px,
                    counts.wash_px,
                    counts.gold_px,
                    counts.mint_px,
                    counts.cyan_px
                );
                for y in 0..height {
                    for x in 0..width {
                        let cell = &frame.buffer[(x, y)];
                        assert_ne!(
                            cell.fg, SPEC_CYAN,
                            "{id} at {width}x{height} paints retired cyan at {x},{y}"
                        );
                        assert_ne!(
                            cell.bg, SPEC_CYAN,
                            "{id} at {width}x{height} paints retired cyan at {x},{y}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_retired_palette_audit_actually_fires() {
        // Guard the guard: a buffer with a banned colour must fail the audit,
        // so the assertion above cannot pass by doing nothing.
        let mut buffer = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 4, 2));
        crate::lock_palette::inject_violet_cell(&mut buffer, 1, 1);
        assert!(
            crate::lock_palette::count_palette(&buffer).has_banned(),
            "the palette audit must detect a banned colour"
        );
    }

    #[test]
    fn focused_rows_paint_selection_or_accent() {
        let mut checked = 0;
        for (width, height) in SIZES {
            for id in cor35_ids(width) {
                let frame = render_lock_v2_scene(id, width, height).expect(id);
                let mut selection = false;
                let mut accent = false;
                for y in 0..height {
                    for x in 0..width {
                        let cell = &frame.buffer[(x, y)];
                        if cell.bg == SELECTION_BG {
                            selection = true;
                        }
                        if cell.fg == ACCENT {
                            accent = true;
                        }
                    }
                }
                assert!(
                    selection || accent,
                    "{id} at {width}x{height} paints no focus: no SELECTION_BG and no ACCENT"
                );
                checked += 1;
            }
        }
        assert!(checked > 0);
    }

    #[test]
    fn apply_rejects_unknown_ids() {
        let mut state = AppState::default();
        for id in [
            "cloud-handoff",
            "handoff-confirm",
            "sandbox",
            "permissions-picker",
        ] {
            assert!(
                !apply_cor35_scene(id, &mut state, 120),
                "{id} must not be claimed by the COR-35 batch"
            );
        }
        assert!(state.messages.is_empty());
        assert!(state.get_interactive_state().is_none());
    }
}
