//! README hero GIF: signed lock chrome, local TUI command tour.
//!
//! [`record`] paints the visual-lock boards through [`MockTerminal`] so the
//! banner cannot drift onto the retired welcome-card splash. Sequence is
//! splash → type a prompt → working → slash palette → `/model` → Shell tool
//! row → idle composer. No Cortex Cloud handoff. `scripts/render-demo-gif.sh`
//! rasterises the frames into `docs/media/intro.gif`.

use cortex_tui_capture::{
    CaptureConfig, CaptureError, CaptureResult, DemoConfig, DemoFrame, DemoRecording, MockTerminal,
    StyleRendering,
};
use ratatui::widgets::Clear;

use crate::lock_boards::{self, HeroScene, USER_PROMPT};

/// The prompt the hero types — same copy as the typing lock board.
pub const HERO_PROMPT: &str = USER_PROMPT;

/// Default TTY size for the README GIF (1232×912 at the rasteriser defaults).
pub const HERO_WIDTH: u16 = 120;
pub const HERO_HEIGHT: u16 = 40;

fn capture_config(width: u16, height: u16) -> CaptureConfig {
    CaptureConfig::minimal(width, height)
        .with_style_rendering(StyleRendering::Ansi)
        .trim_whitespace(false)
        .with_cursor(false)
}

fn paint_beat(scene: HeroScene<'_>) -> impl FnOnce(&mut ratatui::Frame<'_>) + '_ {
    move |frame| {
        let area = frame.area();
        frame.render_widget(Clear, area);
        lock_boards::paint_hero(area, frame.buffer_mut(), scene);
    }
}

/// Storyboard: splash, type, working, slash / model, a tool row, composer.
pub fn storyboard_beats() -> Vec<(String, u32, String)> {
    let mut beats = Vec::new();
    beats.push(("splash".into(), 12, String::new()));

    let chars: Vec<char> = HERO_PROMPT.chars().collect();
    let mut typed = String::new();
    for chunk in chars.chunks(3) {
        typed.extend(chunk);
        beats.push(("typing".into(), 1, typed.clone()));
    }
    beats.push(("prompt-ready".into(), 6, HERO_PROMPT.to_string()));
    beats.push(("working".into(), 12, String::new()));
    beats.push(("palette".into(), 12, String::new()));
    beats.push(("model".into(), 10, String::new()));
    beats.push(("shell".into(), 12, String::new()));
    beats.push(("composer".into(), 10, String::new()));
    beats
}

/// Render the README hero into an in-memory recording.
pub fn record(config: &DemoConfig) -> CaptureResult<DemoRecording> {
    let capture_config = capture_config(config.width, config.height);
    let mut terminal = MockTerminal::from_config(capture_config.clone())
        .map_err(|err| CaptureError::RenderError(err.to_string()))?;

    let beats = storyboard_beats();
    let mut frames = Vec::with_capacity(beats.len());

    for (index, (label, hold, typed)) in beats.into_iter().enumerate() {
        let scene = match label.as_str() {
            "splash" => HeroScene::Splash,
            "working" => HeroScene::Working,
            "palette" => HeroScene::Palette,
            "model" => HeroScene::Model,
            "shell" => HeroScene::Shell,
            "composer" => HeroScene::Composer,
            _ => HeroScene::Typing(&typed),
        };
        terminal
            .draw(paint_beat(scene))
            .map_err(|err| CaptureError::RenderError(err.to_string()))?;
        let snapshot = terminal.snapshot();
        frames.push(DemoFrame {
            index,
            label,
            hold,
            ansi: snapshot.to_ansi(&capture_config),
            plain: snapshot.to_ascii(&capture_config),
        });
    }

    Ok(DemoRecording {
        width: config.width,
        height: config.height,
        fps: config.fps,
        frames,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording() -> DemoRecording {
        let config = DemoConfig {
            width: HERO_WIDTH,
            height: HERO_HEIGHT,
            ..DemoConfig::default()
        };
        record(&config).expect("recording the README hero")
    }

    #[test]
    fn hero_is_one_hundred_twenty_by_forty() {
        let recording = recording();
        assert_eq!(recording.width, 120);
        assert_eq!(recording.height, 40);
        for frame in &recording.frames {
            let lines: Vec<&str> = frame.plain.lines().collect();
            assert_eq!(lines.len(), 40);
            for line in lines {
                assert_eq!(line.chars().count(), 120);
            }
        }
    }

    #[test]
    fn hero_loops_between_eight_and_fifteen_seconds() {
        let recording = recording();
        let seconds = recording.duration_secs();
        assert!(
            (8.0..=15.0).contains(&seconds),
            "hero loop is {seconds:.1}s, expected 8-15s"
        );
    }

    #[test]
    fn sequence_tours_local_commands() {
        let recording = recording();
        let labels: Vec<&str> = recording
            .frames
            .iter()
            .map(|frame| frame.label.as_str())
            .collect();
        assert_eq!(labels.first().copied(), Some("splash"));
        assert!(labels.contains(&"typing"));
        for beat in ["working", "palette", "model", "shell"] {
            assert!(labels.contains(&beat), "missing {beat} beat: {labels:?}");
        }
        assert_eq!(labels.last().copied(), Some("composer"));
        let first_typing = labels.iter().position(|l| *l == "typing").expect("typing");
        let working = labels
            .iter()
            .position(|l| *l == "working")
            .expect("working");
        let palette = labels
            .iter()
            .position(|l| *l == "palette")
            .expect("palette");
        let shell = labels.iter().position(|l| *l == "shell").expect("shell");
        assert!(first_typing > 0);
        assert!(working > first_typing);
        assert!(palette > working);
        assert!(shell > palette);
    }

    #[test]
    fn splash_is_the_signed_lock() {
        let recording = recording();
        let first = &recording.frames[0];
        for needle in [
            "Welcome to",
            "the coding agent CLI",
            "Plan, search, build anything",
            "/ commands",
            "Cortex Mini 1",
        ] {
            assert!(
                first.plain.contains(needle),
                "splash missing {needle:?}:\n{}",
                first.plain
            );
        }
        for banned in [
            "▄█▀▀▀▀█▄",
            "Directory:",
            "Endpoint:",
            "Describe a change",
            "BUILD",
            "medium",
            "Welcome! Your AI-powered",
        ] {
            assert!(
                !first.plain.contains(banned),
                "splash still has retired chrome {banned:?}:\n{}",
                first.plain
            );
        }
        // Dual hairline around the composer: a rule above and below the `> `.
        let lines: Vec<&str> = first.plain.lines().collect();
        let composer = lines
            .iter()
            .position(|line| line.contains("> █Plan, search, build anything"))
            .expect("composer");
        assert!(
            lines[composer - 1].contains('─'),
            "missing hairline above composer:\n{}",
            first.plain
        );
        assert!(
            lines[composer + 1].contains('─'),
            "missing hairline below composer:\n{}",
            first.plain
        );
        // Banner green caret `#1F4945`.
        assert!(
            first.ansi.contains("\x1b[38;2;31;73;69m"),
            "splash missing the banner green caret"
        );
        assert!(
            !first.ansi.contains("\x1b[38;2;0;245;212m"),
            "mint leaked into the splash"
        );
    }

    #[test]
    fn typing_uses_the_rate_limit_prompt() {
        let recording = recording();
        let last_typed = recording
            .frames
            .iter()
            .rev()
            .find(|frame| frame.label == "prompt-ready")
            .expect("prompt-ready");
        assert!(
            last_typed
                .plain
                .contains("Add rate limiting to POST /v1/completions"),
            "{}",
            last_typed.plain
        );
        assert!(
            last_typed.plain.contains(HERO_PROMPT)
                || last_typed
                    .plain
                    .contains("sliding window, Redis-backed, with tests"),
            "full prompt never landed:\n{}",
            last_typed.plain
        );
    }

    fn frame_named<'a>(recording: &'a DemoRecording, label: &str) -> &'a DemoFrame {
        recording
            .frames
            .iter()
            .find(|frame| frame.label == label)
            .unwrap_or_else(|| panic!("missing {label} beat"))
    }

    #[test]
    fn working_matches_the_signed_lock() {
        let recording = recording();
        let working = frame_named(&recording, "working");
        for needle in [
            "Add rate limiting to POST /v1/completions",
            "Working",
            "wiring the limiter into completions",
            "Add a follow-up",
            "Cortex Mini 1 · Agent",
        ] {
            assert!(
                working.plain.contains(needle),
                "working missing {needle:?}:\n{}",
                working.plain
            );
        }
        assert!(
            working.ansi.contains("\x1b[38;2;31;73;69m"),
            "working missing the banner green caret"
        );
        assert!(!working.plain.contains("▄█▀▀▀▀█▄"));
        assert!(!working.plain.contains("BUILD"));
    }

    #[test]
    fn slash_and_model_are_local_lock_boards() {
        let recording = recording();
        let palette = frame_named(&recording, "palette");
        assert!(palette.plain.contains("/model"), "{}", palette.plain);
        assert!(
            palette.plain.contains("Choose the model for this session"),
            "{}",
            palette.plain
        );
        let model = frame_named(&recording, "model");
        assert!(
            model.plain.contains("Type to search models"),
            "{}",
            model.plain
        );
        assert!(model.plain.contains("● Medium"), "{}", model.plain);
        let shell = frame_named(&recording, "shell");
        assert!(shell.plain.contains("Shell"), "{}", shell.plain);
        assert!(shell.plain.contains("rateLimit"), "{}", shell.plain);
        let composer = frame_named(&recording, "composer");
        assert!(
            composer.plain.contains("Plan, search, build anything"),
            "{}",
            composer.plain
        );
        assert!(
            !composer.plain.contains("Welcome to"),
            "composer return must drop the splash:\n{}",
            composer.plain
        );
    }

    #[test]
    fn hero_stays_on_the_local_tui() {
        let recording = recording();
        for frame in &recording.frames {
            for banned in [
                "Handed off to Cortex Cloud",
                "Your terminal is free",
                "cortex.foundation/agents",
            ] {
                assert!(
                    !frame.plain.contains(banned),
                    "frame {} pitches cloud linking: {banned}",
                    frame.index
                );
            }
        }
    }

    #[test]
    fn hero_never_names_a_provider_or_transport() {
        let recording = recording();
        for frame in &recording.frames {
            let lower = frame.plain.to_lowercase();
            for banned in [
                "grok",
                "openai",
                "anthropic",
                "reqwest",
                "rakazo",
                "opencode",
            ] {
                assert!(
                    !lower.contains(banned),
                    "frame {} leaks '{banned}'",
                    frame.index
                );
            }
        }
    }
}
