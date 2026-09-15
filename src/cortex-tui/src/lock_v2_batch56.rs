//! Batch 56 runtime lock boards (COR-447 / COR-448 / COR-449).
//!
//! Six real `MockTerminal` scenes captured through the same raster pipeline as
//! the rest of lock v2, without joining the SPEC §7 id lists: the boards here
//! document shipped Code/CLI behavior for one batch, so they are captured by
//! id through [`write_batch56_frames`] and land next to the existing runtime
//! boards.
//!
//! Copy stays Cortex-only: no competitor or provider names.
//!
//! - COR-447: `remote-fast`, `remote-standard`, `org-disabled-fast`
//! - COR-448: `plugin-accept-command`, `command-hash-mismatch`
//! - COR-449: `omit-instructions`

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cortex_core::widgets::Message;
use cortex_engine::fast_mode::{
    FastMode, FastModePolicy, ORG_DISABLED_STAYING, ORG_DISABLED_TOAST,
};

use crate::app::AppState;
use crate::lock_v2::render_lock_v2_scene;
use crate::lock_v2_scenes::{conversation, resumed};

/// The batch's board ids. Each one is also its capture file name.
pub const BATCH56_IDS: &[&str] = &[
    "remote-fast",
    "remote-standard",
    "org-disabled-fast",
    "plugin-accept-command",
    "command-hash-mismatch",
    "omit-instructions",
];

/// True when `id` belongs to this batch.
pub fn is_batch56_id(id: &str) -> bool {
    BATCH56_IDS.contains(&id)
}

/// Reject empty, unknown, or repeated ids before any frame is written.
pub fn validate_batch56_only_ids(ids: &[&str]) -> Result<()> {
    if ids.is_empty() {
        anyhow::bail!("--only requires at least one scene id");
    }
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        if !is_batch56_id(id) {
            anyhow::bail!("unknown batch 56 scene id `{id}`");
        }
        if !seen.insert(*id) {
            anyhow::bail!("repeated batch 56 scene id `{id}`");
        }
    }
    Ok(())
}

/// Write the requested batch frames as ANSI plus a manifest, in the same shape
/// [`crate::lock_v2::write_lock_v2_id_frames`] uses so the existing
/// `ansi-frames-to-gif.py --png-only` step consumes them unchanged.
pub fn write_batch56_frames(
    ids: &[&str],
    width: u16,
    height: u16,
    output_dir: &Path,
) -> Result<PathBuf> {
    validate_batch56_only_ids(ids)?;
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("create {}", output_dir.display()))?;
    let mut frames = Vec::new();
    for id in ids {
        let frame = render_lock_v2_scene(id, width, height)?;
        let file = format!("{id}.ans");
        std::fs::write(output_dir.join(&file), &frame.ansi)
            .with_context(|| format!("write {file}"))?;
        frames.push(serde_json::json!({
            "file": file,
            "label": (*id).to_string(),
            "hold": 1,
        }));
    }
    let manifest = output_dir.join("manifest.json");
    std::fs::write(
        &manifest,
        serde_json::to_string_pretty(&serde_json::json!({
            "width": width,
            "height": height,
            "fps": 1,
            "frames": frames,
        }))?,
    )?;
    Ok(manifest)
}

/// Apply a Batch 56 scene. Returns `false` when `id` is not one of ours.
pub fn apply_batch56_scene(id: &str, state: &mut AppState, width: u16) -> bool {
    if !is_batch56_id(id) {
        return false;
    }
    let narrow = width <= 40;
    match id {
        "remote-fast" => apply_remote_fast(state),
        "remote-standard" => apply_remote_standard(state),
        "org-disabled-fast" => apply_org_disabled_fast(state, narrow),
        "plugin-accept-command" => apply_plugin_accept_command(state, narrow),
        "command-hash-mismatch" => apply_command_hash_mismatch(state, narrow),
        "omit-instructions" => apply_omit_instructions(state, narrow),
        _ => return false,
    }
    true
}

/// COR-447: a remote session on fast mode. The status line and the composer
/// Fast chip are painted by the real view, not by this scene.
fn apply_remote_fast(state: &mut AppState) {
    conversation(state);
    state.remote_session = true;
    state.fast_mode = FastMode::Fast;
    state.fast_mode_policy = FastModePolicy::Allowed;
}

/// COR-447: a remote session on the default path. No Fast chip.
fn apply_remote_standard(state: &mut AppState) {
    conversation(state);
    state.remote_session = true;
    state.fast_mode = FastMode::Standard;
    state.fast_mode_policy = FastModePolicy::Allowed;
}

/// COR-447: the organization refused `/fast on`. Fail-closed, stays Standard.
fn apply_org_disabled_fast(state: &mut AppState, narrow: bool) {
    resumed(state);
    state.remote_session = true;
    state.fast_mode = FastMode::Standard;
    state.fast_mode_policy = FastModePolicy::Disabled;
    state.add_message(Message::user("/fast on").with_timestamp("09:26 AM"));
    state.add_message(Message::system(format!("! {ORG_DISABLED_TOAST}")));
    let staying = if narrow {
        ORG_DISABLED_STAYING.to_string()
    } else {
        format!("{ORG_DISABLED_STAYING} Nothing was re-sent, and there is no client override.")
    };
    state.add_message(Message::system(staying));
}

/// The fixture package the install review describes. Parsed through the
/// shipped manifest type so the hash printed in the frame is a real one.
const REVIEW_FIXTURE: &str = r#"
[plugin]
id = "cortex-review"
name = "Cortex Review"
version = "1.2.0"

[runtime]
kind = "node"
entrypoint = "plugin.mjs"

[[commands]]
name = "review"
description = "Review the working tree"
usage = "/review [path]"

[[commands.args]]
name = "path"
required = false
"#;

/// The reviewed command hash for the fixture, as the CLI would print it.
fn review_fixture_hash() -> String {
    let manifest = cortex_engine::plugin::runtime::PluginManifest::parse(REVIEW_FIXTURE)
        .expect("review fixture parses");
    cortex_engine::plugin::runtime::command_pin::command_hash(&manifest)
}

/// COR-448: the `--accept-command` review surface with its real command hash.
fn apply_plugin_accept_command(state: &mut AppState, narrow: bool) {
    resumed(state);
    let hash = review_fixture_hash();
    state.input.set_text("/plugins install cortex-review");
    state.add_message(Message::system(if narrow {
        "Install review — accept the pinned command"
    } else {
        "Install review — read every command before you accept it."
    }));
    state.add_message(Message::system(if narrow {
        "cortex-review 1.2.0 · 1 command"
    } else {
        "cortex-review 1.2.0 — registers /review. No hooks, no tools."
    }));
    // The hash is long; the narrow board keeps the field name and the fact
    // that it is a sha256, the wide board prints the whole value.
    // The hash is long; the narrow board keeps the field name plus a real
    // prefix/suffix, the wide board prints the whole value.
    state.add_message(Message::system(if narrow {
        format!("command_hash {}…{}", &hash[..4], &hash[hash.len() - 4..])
    } else {
        // The value the user pastes back into --accept-command.
        format!("command_hash {hash}")
    }));
    state.add_message(Message::system(if narrow {
        "accept-command pins it"
    } else {
        "Pass it back with --accept-command to pin exactly these commands."
    }));
}

/// COR-448: a manifest that changed after review. Fail-closed.
fn apply_command_hash_mismatch(state: &mut AppState, narrow: bool) {
    resumed(state);
    state
        .input
        .set_text("/plugins update cortex-review --accept-command 8f4c2a71");
    state.add_message(Message::user(
        "cortex plugin update cortex-review --accept-command 8f4c2a71",
    ));
    state.add_message(Message::system(
        "× Command hash mismatch. Manifest may have changed. Re-run with --json and accept the new hash.",
    ));
    state.add_message(Message::system(if narrow {
        "Nothing was installed."
    } else {
        "Nothing was installed. The installed cortex-review 1.2.0 stays as it is, and no trust was renewed."
    }));
}

/// COR-449: `omit_instructions` loads, skips, and records — managed policy is
/// never omitted.
fn apply_omit_instructions(state: &mut AppState, narrow: bool) {
    resumed(state);
    state
        .input
        .set_text("/agents review --omit-instructions user,project,local,managed");
    state.add_message(Message::system(
        "Subagent instructions — managed policy is never omitted.",
    ));
    state.add_message(Message::system(if narrow {
        "loads managed · skips user, project, local"
    } else {
        "Requested: user, project, local, managed. Loading: managed policy. Skipping: user, project, local."
    }));
    state.add_message(Message::system(if narrow {
        "audit: managed_policy_never_omitted"
    } else {
        "Audit: managed_policy_never_omitted {source: subagent, loaded: true}"
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::consts::FAST_CHIP_SUFFIX;

    const SIZES: [(u16, u16); 2] = [(120, 40), (40, 12)];

    /// Collapse wrapped rows and box drawing so copy can be matched.
    fn squeezed(plain: &str) -> String {
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

    #[test]
    fn batch56_ids_are_known_and_validated() {
        assert_eq!(BATCH56_IDS.len(), 6);
        let mut seen = std::collections::HashSet::new();
        for id in BATCH56_IDS {
            assert!(seen.insert(*id), "duplicate {id}");
            assert!(is_batch56_id(id));
        }
        assert!(!is_batch56_id("welcome-cortex"));
        assert!(validate_batch56_only_ids(&["remote-fast"]).is_ok());
        assert!(
            validate_batch56_only_ids(&["remote-fast", "remote-fast"])
                .unwrap_err()
                .to_string()
                .contains("repeated")
        );
        assert!(
            validate_batch56_only_ids(&["nope"])
                .unwrap_err()
                .to_string()
                .contains("unknown")
        );
        assert!(
            validate_batch56_only_ids(&[])
                .unwrap_err()
                .to_string()
                .contains("at least one")
        );
    }

    #[test]
    fn remote_fast_shows_the_status_line_and_the_chip() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("remote-fast", width, height)
                .unwrap_or_else(|e| panic!("remote-fast {width}x{height}: {e}"));
            assert!(frame.plain.contains("Remote"), "{width}x{height}");
            assert!(
                frame.plain.contains(FAST_CHIP_SUFFIX),
                "composer Fast chip at {width}x{height}:\n{}",
                frame.plain
            );
            if width >= 120 {
                assert!(
                    frame
                        .plain
                        .contains(cortex_engine::fast_mode::REMOTE_FAST_STATUS),
                    "{width}x{height}:\n{}",
                    frame.plain
                );
            } else {
                assert!(
                    frame
                        .plain
                        .contains(cortex_engine::fast_mode::REMOTE_FAST_STATUS_NARROW),
                    "{width}x{height}:\n{}",
                    frame.plain
                );
            }
        }
    }

    #[test]
    fn remote_standard_has_no_fast_chip() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("remote-standard", width, height)
                .unwrap_or_else(|e| panic!("remote-standard {width}x{height}: {e}"));
            assert!(frame.plain.contains("Remote"), "{width}x{height}");
            assert!(
                !frame.plain.contains(FAST_CHIP_SUFFIX),
                "Standard must not paint the chip at {width}x{height}:\n{}",
                frame.plain
            );
        }
    }

    #[test]
    fn org_disabled_fast_uses_the_refusal_copy_and_no_chip() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("org-disabled-fast", width, height)
                .unwrap_or_else(|e| panic!("org-disabled-fast {width}x{height}: {e}"));
            let plain = squeezed(&frame.plain);
            assert!(
                plain.contains(ORG_DISABLED_TOAST),
                "{width}x{height}:\n{plain}"
            );
            assert!(
                plain.contains(ORG_DISABLED_STAYING),
                "{width}x{height}:\n{plain}"
            );
            // The refusal copy names fast mode, so the chip check is the
            // composer suffix, not the bare word.
            assert!(
                !frame.plain.contains(FAST_CHIP_SUFFIX),
                "a refused session must not paint the Fast chip:\n{plain}"
            );
            assert!(
                !plain.contains(cortex_engine::fast_mode::REMOTE_FAST_STATUS),
                "a refused session must not paint the fast status line:\n{plain}"
            );
            assert!(
                plain.contains(cortex_engine::fast_mode::REMOTE_STATUS)
                    || plain.contains(cortex_engine::fast_mode::REMOTE_STATUS_NARROW),
                "a refused session stays on the Standard remote status line:\n{plain}"
            );
        }
    }

    #[test]
    fn plugin_accept_command_carries_a_real_hash_and_the_pin() {
        let hash = review_fixture_hash();
        assert_eq!(hash.len(), 64);
        let frame = render_lock_v2_scene("plugin-accept-command", 120, 40).unwrap();
        assert!(frame.plain.contains("command_hash"), "{}", frame.plain);
        assert!(frame.plain.contains(&hash), "{}", frame.plain);
        assert!(frame.plain.contains("accept-command"), "{}", frame.plain);
        let narrow = render_lock_v2_scene("plugin-accept-command", 40, 12).unwrap();
        assert!(narrow.plain.contains("command_hash"), "{}", narrow.plain);
        // The narrow board keeps a real prefix and suffix of the same hash.
        assert!(
            narrow.plain.contains(&hash[..4]) && narrow.plain.contains(&hash[hash.len() - 4..]),
            "{}",
            narrow.plain
        );
        assert!(narrow.plain.contains("accept-command"), "{}", narrow.plain);
    }

    #[test]
    fn command_hash_mismatch_is_the_fail_closed_copy() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("command-hash-mismatch", width, height)
                .unwrap_or_else(|e| panic!("command-hash-mismatch {width}x{height}: {e}"));
            let plain = squeezed(&frame.plain);
            assert!(
                plain.contains(
                    "Command hash mismatch. Manifest may have changed. Re-run with --json and accept the new hash."
                ),
                "{width}x{height}:\n{plain}"
            );
            assert!(
                plain.contains("Nothing was installed"),
                "{width}x{height}:\n{plain}"
            );
        }
    }

    #[test]
    fn omit_instructions_keeps_managed_policy_loading() {
        for (width, height) in SIZES {
            let frame = render_lock_v2_scene("omit-instructions", width, height)
                .unwrap_or_else(|e| panic!("omit-instructions {width}x{height}: {e}"));
            let plain = squeezed(&frame.plain);
            assert!(plain.contains("managed"), "{width}x{height}:\n{plain}");
            assert!(
                plain.contains("user") && plain.contains("project") && plain.contains("local"),
                "{width}x{height}:\n{plain}"
            );
            assert!(
                plain.contains("managed_policy_never_omitted") || plain.contains("never omitted"),
                "{width}x{height}:\n{plain}"
            );
            assert!(
                !plain.contains("loads nothing"),
                "managed policy always loads:\n{plain}"
            );
        }
    }

    #[test]
    fn every_batch_frame_is_distinct() {
        for (width, height) in SIZES {
            let mut seen: std::collections::HashMap<String, &str> =
                std::collections::HashMap::new();
            for id in BATCH56_IDS {
                let frame = render_lock_v2_scene(id, width, height)
                    .unwrap_or_else(|e| panic!("{id} {width}x{height}: {e}"));
                if let Some(prev) = seen.insert(frame.ansi.clone(), id) {
                    panic!("{id} is identical to {prev} at {width}x{height}");
                }
            }
            assert_eq!(seen.len(), BATCH56_IDS.len());
        }
    }

    #[test]
    fn unknown_ids_are_not_applied() {
        let mut state = AppState::default();
        assert!(!apply_batch56_scene("welcome-cortex", &mut state, 120));
        assert!(!apply_batch56_scene("session-empty", &mut state, 120));
    }

    #[test]
    fn writing_an_invalid_id_set_writes_nothing() {
        let dir = std::env::temp_dir().join(format!("cortex-batch56-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let err = write_batch56_frames(&["remote-fast", "nope"], 120, 40, &dir).unwrap_err();
        assert!(err.to_string().contains("unknown"), "{err}");
        assert!(!dir.join("remote-fast.ans").exists());
        assert!(!dir.join("manifest.json").exists());
    }
}
