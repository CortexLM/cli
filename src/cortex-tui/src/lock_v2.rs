//! Runtime lock v2 captures — real `MinimalSessionView` at 40×12 and 120×40.
//!
//! Scene ids match `docs/media/tui-lock-v2/txt/` board names. Designer
//! boards in that pack stay the pixel target; these frames are the product.

use anyhow::{Context, Result};
use cortex_core::widgets::Message;
use cortex_tui_capture::{CaptureConfig, MockTerminal, StyleRendering};
use ratatui::widgets::Clear;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::app::{AppState, AutocompleteItem, AutocompleteTrigger};
use crate::commands::{CommandRegistry, CompletionEngine, PALETTE_HOME_LIMIT};
use crate::views::minimal_session::MinimalSessionView;
use crate::widgets::SettingsModalState;

/// Boards captured at both sizes. Narrow (40×12) is a subset.
pub fn lock_v2_scene_ids(width: u16) -> &'static [&'static str] {
    if width <= 40 {
        &[
            "welcome-cortex",
            "welcome-agent",
            "session-empty",
            "session-user-bars",
            "session-thinking-live",
            "session-assistant",
            "session-optin",
            "composer-empty",
            "composer-typing",
            "slash-palette",
            "slash-model-typed",
            "model-list",
            "model-effort-high",
            "settings-appearance",
            "settings-mouse",
            "shortcuts-overlay",
            "tokens-topright",
        ]
    } else {
        &[
            "welcome-cortex",
            "welcome-agent",
            "session-empty",
            "session-user-bars",
            "session-thought",
            "session-thinking-live",
            "session-assistant",
            "session-optin",
            "composer-empty",
            "composer-typing",
            "slash-palette",
            "slash-model-typed",
            "model-list",
            "model-effort-high",
            "model-effort-medium",
            "model-effort-low",
            "settings-appearance",
            "settings-mouse",
            "settings-row-hover",
            "shortcuts-overlay",
            "tokens-topright",
            "footer-shortcuts",
        ]
    }
}

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
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("create {}", output_dir.display()))?;
    let mut manifest_frames = Vec::new();
    for id in lock_v2_scene_ids(width) {
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

fn render_lock_v2_scene(id: &str, width: u16, height: u16) -> Result<crate::lock_proof::LockFrame> {
    let config = capture_config(width, height);
    let mut terminal =
        MockTerminal::from_config(config.clone()).map_err(|err| anyhow::anyhow!("{err}"))?;
    terminal.draw(|frame| {
        let area = frame.area();
        frame.render_widget(Clear, area);
        let state = scene_state(id, width, height);
        let view = MinimalSessionView::new(&state);
        frame.render_widget(view, area);
    })?;
    let snapshot = terminal.snapshot();
    Ok(crate::lock_proof::LockFrame {
        id: id.to_string(),
        ansi: snapshot.to_ansi(&config),
        plain: snapshot.to_ascii(&config),
        buffer: terminal.backend().buffer().clone(),
    })
}

fn lock_app() -> AppState {
    let mut state = AppState::default();
    state.cli_version = env!("CARGO_PKG_VERSION").to_string();
    state.model = "cortex-1-mini".into();
    state.agent_mode_label = "Agent".into();
    state.thinking_budget = Some("medium".into());
    state.tokens_used = 0;
    state.context_window = 500_000;
    state.opt_in_banner = false;
    state.timestamps_enabled = true;
    state.caret_visible = true;
    state
}

fn conversation(state: &mut AppState) {
    state.show_launch_splash = false;
    state.tokens_used = 14_000;
    state.add_message(
        Message::user("hey")
            .with_timestamp("12:49 AM")
            .with_thought_secs(0.4)
            .with_worked_secs(1.8),
    );
    // thought/worked belong on the assistant reply
    if let Some(last) = state.messages.last_mut() {
        last.thought_secs = None;
        last.worked_secs = None;
    }
    state.add_message(
        Message::assistant("Hey — what do you want to work on?")
            .with_timestamp("12:49 AM")
            .with_thought_secs(0.4)
            .with_worked_secs(1.8),
    );
    state.add_message(Message::user("tell me about yourself").with_timestamp("12:49 AM"));
    state.add_message(
        Message::assistant(
            "I'm **Cortex**, a coding agent that runs in your terminal.\n\n\
I mostly help you **build and debug software**: code, architecture, debugging, reviews, docs, and a bit of research.\n\
Here I run in an interactive terminal, so I can read your files, run commands, and change the project.\n\n\
In practice:\n\
• I get straight to the point\n\
• I prefer concrete work over long explanations\n\
• I can also discuss, explain, or help you plan\n\n\
Tell me what you'd like to do.",
        )
        .with_timestamp("12:49 AM")
        .with_thought_secs(0.4)
        .with_worked_secs(4.6),
    );
}

fn palette_state(query: &str) -> AppState {
    let mut state = lock_app();
    conversation(&mut state);
    state.input.set_text(query);
    state.autocomplete.show(AutocompleteTrigger::Command, 0);
    let registry = CommandRegistry::default();
    let engine = CompletionEngine::new(&registry);
    let completions = engine.complete(query);
    let items: Vec<AutocompleteItem> = completions
        .into_iter()
        .map(|c| AutocompleteItem::new(&c.command, &c.display, &c.description))
        .collect();
    state.autocomplete.set_items(items);
    state.autocomplete.max_visible = PALETTE_HOME_LIMIT;
    if query.len() > 1 {
        state.autocomplete.set_query(&query[1..]);
    }
    state
}

fn scene_state(id: &str, width: u16, height: u16) -> AppState {
    let mut state = lock_app();
    state.terminal_size = (width, height);
    match id {
        "welcome-agent" => {
            state.agent_entrypoint = true;
        }
        "session-empty" => {
            state.show_launch_splash = false;
        }
        "session-user-bars" | "session-assistant" | "session-thought" => {
            conversation(&mut state);
        }
        "session-thinking-live" => {
            state.show_launch_splash = false;
            state.tokens_used = 14_000;
            state.add_message(
                Message::user("why does the composer lose focus after /model?")
                    .with_timestamp("10:02 AM"),
            );
            state.start_streaming(None, true);
            state.streaming.thinking = true;
        }
        "session-optin" => {
            conversation(&mut state);
            state.opt_in_banner = true;
        }
        "composer-empty" | "footer-shortcuts" | "tokens-topright" => {}
        "composer-typing" => {
            state.input.set_text("hello");
        }
        "slash-palette" => {
            let mut s = palette_state("/");
            s.terminal_size = (width, height);
            return s;
        }
        "slash-model-typed" => {
            let mut s = palette_state("/model");
            s.terminal_size = (width, height);
            return s;
        }
        "model-list" => {
            conversation(&mut state);
            state.input.set_text("/model");
            let interactive = crate::interactive::builders::build_model_selector(
                vec![
                    dummy_model("cortex-1-mini", "Cortex Mini 1"),
                    dummy_model("cortex-1", "Cortex 1"),
                    dummy_model("cortex-1-max", "Cortex Max 1"),
                ],
                Some("cortex-1-mini"),
                Some("medium"),
            );
            state.enter_interactive_mode(interactive);
        }
        "model-effort-high" | "model-effort-medium" | "model-effort-low" => {
            conversation(&mut state);
            state.input.set_text("/model Cortex Mini 1");
            let effort = match id {
                "model-effort-low" => crate::interactive::EffortLevel::Low,
                "model-effort-high" => crate::interactive::EffortLevel::High,
                _ => crate::interactive::EffortLevel::Medium,
            };
            let mut interactive = crate::interactive::builders::build_model_selector(
                vec![dummy_model("cortex-1-mini", "Cortex Mini 1")],
                Some("cortex-1-mini"),
                Some(effort.as_str()),
            );
            interactive.effort = Some(effort);
            interactive.effort_focused = true;
            state.thinking_budget = Some(effort.as_str().to_ascii_lowercase());
            state.enter_interactive_mode(interactive);
        }
        "settings-appearance" | "settings-mouse" | "settings-row-hover" => {
            conversation(&mut state);
            let mut modal = SettingsModalState::default();
            modal.values = state.settings_values();
            if id == "settings-mouse" {
                if let Some(i) = modal
                    .visible_rows()
                    .iter()
                    .position(|r| r.id == "mouse_capture")
                {
                    modal.selected = i;
                }
            }
            if id == "settings-row-hover" {
                modal.hovered = Some(modal.selected);
            }
            state.settings_modal = Some(modal);
        }
        "shortcuts-overlay" => {
            state.shortcuts_open = true;
        }
        _ => {}
    }
    if id == "tokens-topright" {
        state.tokens_used = 14_000;
    }
    state
}

fn dummy_model(id: &str, name: &str) -> crate::providers::models::ModelInfo {
    crate::providers::models::ModelInfo::new(id, name, "cortex")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_core::style::{ACCENT, BAR_HOVER, SELECTION_BG, VOID};
    use ratatui::widgets::Widget;

    fn cell_bg(buf: &ratatui::buffer::Buffer, x: u16, y: u16) -> ratatui::style::Color {
        buf[(x, y)].bg
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
        // Composer caret at inner col is violet.
        let mut found_accent = false;
        for y in 0..40u16 {
            for x in 0..120u16 {
                if frame.buffer[(x, y)].fg == ACCENT {
                    found_accent = true;
                }
            }
        }
        assert!(found_accent, "expected violet caret on welcome");
    }

    #[test]
    fn slash_hover_is_not_violet_wash() {
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
        // Hover bar exists somewhere as BAR_HOVER
        let mut found_hover = false;
        for y in 0..40u16 {
            for x in 0..120u16 {
                if buf[(x, y)].bg == BAR_HOVER {
                    found_hover = true;
                }
                // Never the retired violet wash
                if buf[(x, y)].bg == ratatui::style::Color::Rgb(0x22, 0x1A, 0x38) {
                    panic!("retired violet wash at {x},{y}");
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
        assert!(frame.plain.contains("Worked for"), "{}", frame.plain);
        assert!(
            frame.plain.contains("AM") || frame.plain.contains("PM"),
            "{}",
            frame.plain
        );
        assert!(frame.plain.contains("Cortex Mini 1"), "{}", frame.plain);
    }
}
