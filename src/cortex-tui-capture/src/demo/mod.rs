//! Headless recording of the Cortex Code session view.
//!
//! This module renders the storyboard in [`script`] through [`MockTerminal`],
//! producing one ANSI frame per step plus a manifest describing frame timing.
//! `scripts/render-demo-gif.sh` turns those frames into `docs/media/intro.gif`.
//!
//! Recording headlessly is deliberate: the demo has to be reproducible on a
//! machine that cannot start a real terminal or sign in to the coding service.

mod render;
pub mod scene;
pub mod script;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{CaptureConfig, StyleRendering};
use crate::mock_terminal::MockTerminal;
use crate::types::{CaptureError, CaptureResult};

pub use render::draw_scene;
pub use scene::{Scene, Status, TimelineBlock, ToolRow, ToolState};
pub use script::{Beat, DEMO_PROMPT, storyboard, total_frames};

/// Default recording width in terminal columns.
pub const DEFAULT_WIDTH: u16 = 120;

/// Default recording height in terminal rows.
pub const DEFAULT_HEIGHT: u16 = 32;

/// Default playback rate of the generated GIF.
pub const DEFAULT_FPS: u32 = 12;

/// Directory the recording is written to by default.
pub const DEFAULT_OUTPUT_DIR: &str = "target/tui-demo";

/// Options for a demo recording.
#[derive(Debug, Clone)]
pub struct DemoConfig {
    /// Terminal width in columns.
    pub width: u16,
    /// Terminal height in rows.
    pub height: u16,
    /// Playback rate written into the manifest.
    pub fps: u32,
    /// Where frames and the manifest are written.
    pub output_dir: PathBuf,
}

impl Default for DemoConfig {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            fps: DEFAULT_FPS,
            output_dir: PathBuf::from(DEFAULT_OUTPUT_DIR),
        }
    }
}

/// One rendered frame of the recording.
#[derive(Debug, Clone)]
pub struct DemoFrame {
    /// Zero-based position in the recording.
    pub index: usize,
    /// Storyboard label this frame came from.
    pub label: String,
    /// How many playback frames this image is held for.
    pub hold: u32,
    /// The frame with ANSI colour escapes, ready to rasterise.
    pub ansi: String,
    /// The same frame as plain text, used by tests and snapshots.
    pub plain: String,
}

impl DemoFrame {
    /// File name this frame is written to.
    pub fn file_name(&self) -> String {
        format!("frame_{:04}.ans", self.index)
    }
}

/// A complete recording, in memory.
#[derive(Debug, Clone)]
pub struct DemoRecording {
    /// Terminal width in columns.
    pub width: u16,
    /// Terminal height in rows.
    pub height: u16,
    /// Playback rate.
    pub fps: u32,
    /// Rendered frames in playback order.
    pub frames: Vec<DemoFrame>,
}

/// Manifest entry describing one frame for the rasteriser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFrame {
    /// Frame file name, relative to the manifest.
    pub file: String,
    /// Storyboard label.
    pub label: String,
    /// Playback frames this image is held for.
    pub hold: u32,
}

/// Manifest consumed by the GIF rasteriser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoManifest {
    /// Terminal width in columns.
    pub width: u16,
    /// Terminal height in rows.
    pub height: u16,
    /// Playback rate.
    pub fps: u32,
    /// Total playback frames once holds are expanded.
    pub total_frames: u32,
    /// Frames in playback order.
    pub frames: Vec<ManifestFrame>,
}

impl DemoRecording {
    /// Total playback frames once holds are expanded.
    pub fn total_frames(&self) -> u32 {
        self.frames.iter().map(|frame| frame.hold).sum()
    }

    /// Playback duration in seconds.
    pub fn duration_secs(&self) -> f64 {
        if self.fps == 0 {
            return 0.0;
        }
        f64::from(self.total_frames()) / f64::from(self.fps)
    }

    /// Build the manifest for this recording.
    pub fn manifest(&self) -> DemoManifest {
        DemoManifest {
            width: self.width,
            height: self.height,
            fps: self.fps,
            total_frames: self.total_frames(),
            frames: self
                .frames
                .iter()
                .map(|frame| ManifestFrame {
                    file: frame.file_name(),
                    label: frame.label.clone(),
                    hold: frame.hold,
                })
                .collect(),
        }
    }

    /// Write every frame plus `manifest.json` into `dir`.
    pub fn write_to_dir(&self, dir: &Path) -> CaptureResult<Vec<PathBuf>> {
        std::fs::create_dir_all(dir)?;

        let mut written = Vec::with_capacity(self.frames.len() + 1);
        for frame in &self.frames {
            let path = dir.join(frame.file_name());
            std::fs::write(&path, &frame.ansi)?;
            written.push(path);
        }

        let manifest_path = dir.join("manifest.json");
        let manifest = serde_json::to_string_pretty(&self.manifest())
            .map_err(|err| CaptureError::SerializationError(err.to_string()))?;
        std::fs::write(&manifest_path, manifest)?;
        written.push(manifest_path);

        Ok(written)
    }
}

/// Render the storyboard into an in-memory recording.
pub fn record(config: &DemoConfig) -> CaptureResult<DemoRecording> {
    let capture_config = CaptureConfig::minimal(config.width, config.height)
        .with_style_rendering(StyleRendering::Ansi)
        .trim_whitespace(false)
        .with_cursor(false);

    let mut terminal = MockTerminal::from_config(capture_config.clone())
        .map_err(|err| CaptureError::RenderError(err.to_string()))?;

    let beats = storyboard();
    let mut frames = Vec::with_capacity(beats.len());

    for (index, beat) in beats.iter().enumerate() {
        let scene = beat.scene.clone();
        terminal
            .draw(|frame| draw_scene(frame, &scene))
            .map_err(|err| CaptureError::RenderError(err.to_string()))?;

        let snapshot = terminal.snapshot();
        frames.push(DemoFrame {
            index,
            label: beat.label.clone(),
            hold: beat.hold,
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
        record(&DemoConfig::default()).expect("recording the demo storyboard")
    }

    #[test]
    fn recording_produces_one_frame_per_beat() {
        let recording = recording();
        assert_eq!(recording.frames.len(), storyboard().len());
        assert!(!recording.frames.is_empty());
    }

    #[test]
    fn recording_loops_between_eight_and_fifteen_seconds() {
        let recording = recording();
        let seconds = recording.duration_secs();
        assert!(
            (8.0..=15.0).contains(&seconds),
            "demo loop is {seconds:.1}s, expected 8-15s"
        );
    }

    #[test]
    fn first_frame_shows_the_welcome_card() {
        let recording = recording();
        let first = &recording.frames[0];
        assert!(
            first.plain.contains("Cortex Code"),
            "welcome frame missing the product name:\n{}",
            first.plain
        );
        assert!(
            first.plain.contains("api.cortex.foundation"),
            "welcome frame missing the public endpoint:\n{}",
            first.plain
        );
    }

    #[test]
    fn recording_shows_a_prompt_tool_rows_and_a_reply() {
        let recording = recording();
        let last = recording.frames.last().expect("frames");

        // The whole turn has to still be on screen at the end of the loop, so
        // the banner reads as a complete story when it stops moving.
        assert!(
            last.plain.contains(DEMO_PROMPT),
            "final frame scrolled the prompt away:\n{}",
            last.plain
        );
        for tool in ["Grep", "Read", "Create", "Edit", "Execute"] {
            assert!(
                recording
                    .frames
                    .iter()
                    .any(|frame| frame.plain.contains(tool)),
                "no frame contains the '{tool}' tool row"
            );
        }
        assert!(
            last.plain.contains("test result: ok."),
            "final frame missing the test result:\n{}",
            last.plain
        );
    }

    #[test]
    fn frames_are_the_configured_terminal_size() {
        let recording = recording();
        for frame in &recording.frames {
            let lines: Vec<&str> = frame.plain.lines().collect();
            assert_eq!(lines.len(), usize::from(recording.height));
            for line in lines {
                assert_eq!(line.chars().count(), usize::from(recording.width));
            }
        }
    }

    #[test]
    fn frames_carry_colour() {
        let recording = recording();
        // The recording is only useful as a GIF if the palette survives.
        assert!(
            recording.frames[0].ansi.contains("\x1b[38;2;0;255;163m"),
            "expected the product accent colour in the first frame"
        );
    }

    #[test]
    fn recording_never_names_a_provider_or_transport() {
        let recording = recording();
        for frame in &recording.frames {
            let lower = frame.plain.to_lowercase();
            for banned in ["grok", "openai", "anthropic", "reqwest"] {
                assert!(
                    !lower.contains(banned),
                    "frame {} leaks '{banned}'",
                    frame.index
                );
            }
        }
    }

    #[test]
    fn manifest_matches_the_frames() {
        let recording = recording();
        let manifest = recording.manifest();
        assert_eq!(manifest.frames.len(), recording.frames.len());
        assert_eq!(manifest.total_frames, recording.total_frames());
        assert_eq!(manifest.frames[0].file, "frame_0000.ans");
    }

    #[test]
    fn write_to_dir_emits_frames_and_manifest() {
        let dir = tempfile::tempdir().expect("temp dir");
        let recording = recording();
        let written = recording
            .write_to_dir(dir.path())
            .expect("writing the recording");

        assert_eq!(written.len(), recording.frames.len() + 1);
        assert!(dir.path().join("manifest.json").is_file());
        assert!(dir.path().join("frame_0000.ans").is_file());
    }
}
