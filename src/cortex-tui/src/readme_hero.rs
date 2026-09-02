//! README hero GIF: signed lock chrome, splash → typing → working.
//!
//! [`record`] paints the visual-lock boards through [`MockTerminal`] so the
//! banner cannot drift onto the retired welcome-card splash. `scripts/render-demo-gif.sh`
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

/// Storyboard: idle splash, type the rate-limit prompt, hold the working lock.
pub fn storyboard_beats() -> Vec<(String, u32, String)> {
    let mut beats = Vec::new();
    beats.push(("splash".into(), 18, String::new()));

    let chars: Vec<char> = HERO_PROMPT.chars().collect();
    let mut typed = String::new();
    for chunk in chars.chunks(3) {
        typed.extend(chunk);
        beats.push(("typing".into(), 1, typed.clone()));
    }
    beats.push(("prompt-ready".into(), 8, HERO_PROMPT.to_string()));
    beats.push(("working".into(), 36, String::new()));
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
    fn sequence_is_splash_then_typing_then_working() {
        let labels: Vec<&str> = recording()
            .frames
            .iter()
            .map(|frame| frame.label.as_str())
            .collect();
        assert_eq!(labels.first().copied(), Some("splash"));
        assert!(labels.contains(&"typing"));
        assert_eq!(labels.last().copied(), Some("working"));
        let first_typing = labels.iter().position(|l| *l == "typing").expect("typing");
        let working = labels
            .iter()
            .rposition(|l| *l == "working")
            .expect("working");
        assert!(first_typing > 0);
        assert!(working > first_typing);
    }

    #[test]
    fn splash_is_the_signed_lock() {
        let first = &recording().frames[0];
        for needle in [
            "Cortex CLI v1.0.0",
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
            .position(|line| line.contains("> Plan, search, build anything"))
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
        // Violet caret `#A78BFA`.
        assert!(
            first.ansi.contains("\x1b[38;2;167;139;250m"),
            "splash missing the violet caret"
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

    #[test]
    fn working_matches_the_signed_lock() {
        let last = recording().frames.last().expect("frames");
        assert_eq!(last.label, "working");
        for needle in [
            "Add rate limiting to POST /v1/completions",
            "Working",
            "wiring the limiter into completions",
            "Add a follow-up",
            "Cortex Mini 1 · Agent",
        ] {
            assert!(
                last.plain.contains(needle),
                "working missing {needle:?}:\n{}",
                last.plain
            );
        }
        assert!(
            last.ansi.contains("\x1b[38;2;167;139;250m"),
            "working missing the violet caret"
        );
        assert!(!last.plain.contains("▄█▀▀▀▀█▄"));
        assert!(!last.plain.contains("BUILD"));
    }

    #[test]
    fn hero_never_names_a_provider_or_transport() {
        for frame in &recording().frames {
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
