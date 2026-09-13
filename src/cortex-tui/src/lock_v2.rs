//! Runtime lock v2 captures — real `MinimalSessionView` at 40×12 and 120×40.
//!
//! Scene ids match SPEC §7 / `docs/media/tui-lock-v2/index.md`. Each filename
//! is one live MockTerminal state — never an alias of another frame.

use anyhow::{Context, Result};
use cortex_tui_capture::{CaptureConfig, MockTerminal, StyleRendering};
use ratatui::widgets::Clear;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::lock_proof::{LOCK_SPLASH_VERSION, LockFrame};
use crate::runner::login_screen::LoginScreen;
use crate::views::minimal_session::MinimalSessionView;

pub use crate::lock_v2_ids::{LOCK_V2_NARROW_IDS, LOCK_V2_WIDE_IDS, lock_v2_scene_ids};

pub(crate) const PRODUCT_ERROR: &str = "The coding service is temporarily unavailable";

#[derive(Debug, Clone, Serialize)]
struct Manifest {
    width: u16,
    height: u16,
    fps: u32,
    frames: Vec<ManifestFrame>,
}

#[derive(Debug, Clone, Serialize)]
struct ManifestFrame {
    file: String,
    label: String,
    hold: u32,
}

pub fn write_lock_v2_frames(width: u16, height: u16, output_dir: &Path) -> Result<PathBuf> {
    write_lock_v2_id_frames(lock_v2_scene_ids(width), width, height, output_dir)
}

/// Reject empty, unknown, or repeated `--only` scene ids before any writes.
pub fn validate_lock_v2_only_ids(ids: &[&str], width: u16) -> Result<()> {
    if ids.is_empty() {
        anyhow::bail!("--only requires at least one scene id");
    }
    let known = lock_v2_scene_ids(width);
    let mut seen = HashSet::new();
    for id in ids {
        if !known.contains(id) {
            anyhow::bail!("unknown lock v2 scene id `{id}`");
        }
        if !seen.insert(*id) {
            anyhow::bail!("repeated lock v2 scene id `{id}`");
        }
    }
    Ok(())
}

/// Write a subset of lock v2 scenes (used to recapture residual boards).
pub fn write_lock_v2_id_frames(
    ids: &[&str],
    width: u16,
    height: u16,
    output_dir: &Path,
) -> Result<PathBuf> {
    validate_lock_v2_only_ids(ids, width)?;
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("create {}", output_dir.display()))?;
    let mut manifest_frames = Vec::new();
    for id in ids {
        let frame = render_lock_v2_scene(id, width, height)?;
        let file = format!("{}.ans", id);
        std::fs::write(output_dir.join(&file), &frame.ansi)
            .with_context(|| format!("write {file}"))?;
        manifest_frames.push(ManifestFrame {
            file,
            label: id.to_string(),
            hold: 1,
        });
    }
    let manifest_path = output_dir.join("manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&Manifest {
            width,
            height,
            fps: 1,
            frames: manifest_frames,
        })?,
    )?;
    Ok(manifest_path)
}

fn capture_config(width: u16, height: u16) -> CaptureConfig {
    CaptureConfig::minimal(width, height)
        .with_style_rendering(StyleRendering::Ansi)
        .trim_whitespace(false)
        .with_cursor(false)
}

pub fn render_lock_v2_scene(id: &str, width: u16, height: u16) -> Result<LockFrame> {
    let config = capture_config(width, height);
    let mut terminal =
        MockTerminal::from_config(config.clone()).map_err(|err| anyhow::anyhow!("{err}"))?;
    terminal.draw(|frame| {
        let area = frame.area();
        frame.render_widget(Clear, area);
        match id {
            "login" => LoginScreen::lock_select(LOCK_SPLASH_VERSION, None).render(frame),
            "login-waiting" => LoginScreen::lock_waiting(
                LOCK_SPLASH_VERSION,
                "ABCD-1234",
                "https://api.cortex.foundation/device",
            )
            .render(frame),
            "login-success" => LoginScreen::lock_success(LOCK_SPLASH_VERSION).render(frame),
            "login-error" => {
                LoginScreen::lock_failed(LOCK_SPLASH_VERSION, PRODUCT_ERROR).render(frame)
            }
            _ => {
                let state = crate::lock_v2_boards::scene_state(id, width, height);
                let view = MinimalSessionView::new(&state);
                frame.render_widget(view, area);
            }
        }
    })?;
    let snapshot = terminal.snapshot();
    Ok(LockFrame {
        id: id.to_string(),
        ansi: snapshot.to_ansi(&config),
        plain: snapshot.to_ascii(&config),
        buffer: terminal.backend().buffer().clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock_v2_scenes::*;
    use cortex_core::style::{ACCENT, BAR_HOVER, SELECTION_BG, VOID};
    use ratatui::widgets::Widget;
    use std::collections::HashMap;

    fn cell_bg(buf: &ratatui::buffer::Buffer, x: u16, y: u16) -> ratatui::style::Color {
        buf[(x, y)].bg
    }

    fn assert_unique_frames(width: u16, height: u16, ids: &[&str]) {
        let mut by_ansi: HashMap<String, String> = HashMap::new();
        for id in ids {
            let frame =
                render_lock_v2_scene(id, width, height).unwrap_or_else(|e| panic!("{id}: {e}"));
            if let Some(prev) = by_ansi.insert(frame.ansi.clone(), (*id).to_string()) {
                panic!("{id} is identical to {prev} at {width}x{height}");
            }
        }
        assert_eq!(by_ansi.len(), ids.len());
    }

    #[test]
    fn lock_v2_wide_count_is_spec() {
        assert_eq!(LOCK_V2_WIDE_IDS.len(), 89);
        assert_eq!(LOCK_V2_NARROW_IDS.len(), 43);
    }

    #[test]
    fn only_ids_reject_unknown_repeated_and_empty() {
        assert!(validate_lock_v2_only_ids(&["welcome-cortex"], 120).is_ok());
        let unknown = validate_lock_v2_only_ids(&["welcome-cortex", "unknown-scene"], 120)
            .expect_err("unknown");
        assert!(unknown.to_string().contains("unknown"), "{unknown}");
        let repeated = validate_lock_v2_only_ids(&["welcome-cortex", "welcome-cortex"], 120)
            .expect_err("repeated");
        assert!(repeated.to_string().contains("repeated"), "{repeated}");
        let empty = validate_lock_v2_only_ids(&[], 120).expect_err("empty");
        assert!(empty.to_string().contains("at least one"), "{empty}");
        let wide_only_at_narrow =
            validate_lock_v2_only_ids(&["session-thought"], 40).expect_err("wide-only at 40");
        assert!(
            wide_only_at_narrow.to_string().contains("unknown"),
            "{wide_only_at_narrow}"
        );
    }

    #[test]
    fn write_lock_v2_id_frames_does_not_write_on_invalid_ids() {
        let dir = std::env::temp_dir().join(format!("cortex-lock-only-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let err = write_lock_v2_id_frames(&["welcome-cortex", "unknown-scene"], 120, 40, &dir)
            .expect_err("invalid ids");
        assert!(err.to_string().contains("unknown"), "{err}");
        assert!(
            !dir.join("welcome-cortex.ans").exists(),
            "must not write a partial capture set"
        );
        assert!(!dir.join("manifest.json").exists());
    }

    #[test]
    fn lock_v2_wide_frames_are_unique() {
        assert_unique_frames(120, 40, LOCK_V2_WIDE_IDS);
    }

    #[test]
    fn lock_v2_narrow_frames_are_unique() {
        assert_unique_frames(40, 12, LOCK_V2_NARROW_IDS);
    }

    #[test]
    fn reported_collisions_are_distinct() {
        let pairs = [
            ("session-user-bars", "session-thought"),
            ("session-thought", "session-assistant"),
            ("session-user-bars", "session-assistant"),
            ("settings-appearance", "settings-row-hover"),
            ("welcome-cortex", "composer-empty"),
            ("composer-empty", "footer-shortcuts"),
            ("welcome-cortex", "footer-shortcuts"),
        ];
        for (a, b) in pairs {
            let fa = render_lock_v2_scene(a, 120, 40).expect(a);
            let fb = render_lock_v2_scene(b, 120, 40).expect(b);
            assert_ne!(fa.ansi, fb.ansi, "{a} must differ from {b}");
        }
        let hover = render_lock_v2_scene("settings-row-hover", 120, 40).expect("hover");
        let mut found_hover = false;
        for y in 0..40u16 {
            for x in 0..120u16 {
                if hover.buffer[(x, y)].bg == BAR_HOVER {
                    found_hover = true;
                }
            }
        }
        assert!(found_hover, "settings-row-hover must paint BAR_HOVER");
    }

    #[test]
    fn welcome_paints_inky_and_token_counter() {
        let frame = render_lock_v2_scene("welcome-cortex", 120, 40).expect("welcome");
        assert!(frame.plain.contains("Welcome to"), "{}", frame.plain);
        assert!(frame.plain.contains("Cortex"));
        assert!(frame.plain.contains("0 / 500K"), "{}", frame.plain);
        assert!(frame.plain.contains("Shift+Tab"), "{}", frame.plain);
        assert!(frame.plain.contains("Ctrl+x"), "{}", frame.plain);
        assert_eq!(cell_bg(&frame.buffer, 0, 0), VOID);
        let mut found_accent = false;
        for y in 0..40u16 {
            for x in 0..120u16 {
                if frame.buffer[(x, y)].fg == ACCENT {
                    found_accent = true;
                }
            }
        }
        assert!(found_accent, "expected banner green caret on welcome");
    }

    #[test]
    fn slash_hover_is_not_banner_green_wash() {
        let mut state = palette_state("/");
        state.autocomplete.hovered = Some(3);
        let config = capture_config(120, 40);
        let mut terminal = MockTerminal::from_config(config).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(Clear, frame.area());
                MinimalSessionView::new(&state).render(frame.area(), frame.buffer_mut());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let mut found_hover = false;
        for y in 0..40u16 {
            for x in 0..120u16 {
                if buf[(x, y)].bg == BAR_HOVER {
                    found_hover = true;
                }
                if buf[(x, y)].bg == ratatui::style::Color::Rgb(0x22, 0x1A, 0x38) {
                    panic!("retired banner green wash at {x},{y}");
                }
            }
        }
        assert!(found_hover || buf[(3, 26)].bg == SELECTION_BG);
    }

    #[test]
    fn effort_order_is_high_medium_low() {
        let frame = render_lock_v2_scene("model-effort-high", 120, 40).expect("effort");
        let high = frame.plain.find("High Effort").expect("high");
        let med = frame.plain.find("Medium Effort").expect("med");
        let low = frame.plain.find("Low Effort").expect("low");
        assert!(high < med && med < low, "{}", frame.plain);
        assert!(frame.plain.contains("Tab"), "{}", frame.plain);
    }

    #[test]
    fn settings_modal_has_appearance_and_search() {
        let frame = render_lock_v2_scene("settings-appearance", 120, 40).expect("settings");
        assert!(frame.plain.contains("Settings"), "{}", frame.plain);
        assert!(frame.plain.contains("Appearance"), "{}", frame.plain);
        assert!(frame.plain.contains("Compact mode"), "{}", frame.plain);
        assert!(
            frame.plain.contains("/ to search") || frame.plain.contains("search"),
            "{}",
            frame.plain
        );
    }

    #[test]
    fn agent_welcome_copy() {
        let frame = render_lock_v2_scene("welcome-agent", 120, 40).expect("agent");
        assert!(frame.plain.contains("Cortex Agent"), "{}", frame.plain);
        assert!(
            frame.plain.contains("Describe a task for the agent")
                || frame.plain.contains("Plan, search"),
            "{}",
            frame.plain
        );
    }

    #[test]
    fn user_bars_and_thought_metadata() {
        let frame = render_lock_v2_scene("session-thought", 120, 40).expect("thought");
        assert!(frame.plain.contains("Thought for"), "{}", frame.plain);
        assert!(
            !frame.plain.contains("Worked for"),
            "session-thought stays collapsed without Worked:\n{}",
            frame.plain
        );
        assert!(
            frame.plain.contains("AM") || frame.plain.contains("PM"),
            "{}",
            frame.plain
        );
        assert!(frame.plain.contains("Cortex Mini 1"), "{}", frame.plain);
    }

    #[test]
    fn first_run_tips_are_visible() {
        let frame = render_lock_v2_scene("first-run-tips", 120, 40).expect("tips");
        assert!(frame.plain.contains("A few tips"), "{}", frame.plain);
        assert!(frame.plain.contains("/model"), "{}", frame.plain);
    }

    #[test]
    fn permission_prompt_comes_from_approval_state() {
        let mut state = lock_app();
        lock_permission_prompt(&mut state, None);
        let approval = state.pending_approval.as_ref().expect("ApprovalState");
        assert_eq!(approval.tool_name, "shell");
        let interactive = state.get_interactive_state().expect("production radios");
        assert_eq!(interactive.items.len(), 4);
        assert_eq!(interactive.items[0].label, "1 Yes, run once");
        assert!(interactive.prompt_owns_focus);
        let frame = render_lock_v2_scene("permission-prompt", 120, 40).expect("wide");
        assert!(frame.plain.contains("Yes, run once"), "{}", frame.plain);
        assert!(
            frame.plain.contains("Choose an option above"),
            "{}",
            frame.plain
        );
        let narrow = render_lock_v2_scene("permission-prompt", 40, 12).expect("narrow");
        assert!(narrow.plain.contains("Yes, run once"), "{}", narrow.plain);
    }
}
