//! Frame I/O for the README hero recording.
//!
//! Scene painting lives in `cortex-tui::readme_hero` so the GIF is the signed
//! lock TUI (dual hairline, violet `>`, splash → typing → working). This
//! module owns the manifest format `scripts/ansi-frames-to-gif.py` consumes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::types::{CaptureError, CaptureResult};

/// Default recording width in terminal columns.
pub const DEFAULT_WIDTH: u16 = 120;

/// Default recording height in terminal rows (README GIF is 120×40).
pub const DEFAULT_HEIGHT: u16 = 40;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_recording() -> DemoRecording {
        DemoRecording {
            width: 8,
            height: 2,
            fps: 12,
            frames: vec![DemoFrame {
                index: 0,
                label: "splash".into(),
                hold: 2,
                ansi: "splash-ansi".into(),
                plain: "splash".into(),
            }],
        }
    }

    #[test]
    fn default_tty_is_one_hundred_twenty_by_forty() {
        let config = DemoConfig::default();
        assert_eq!(config.width, 120);
        assert_eq!(config.height, 40);
    }

    #[test]
    fn manifest_matches_the_frames() {
        let recording = sample_recording();
        let manifest = recording.manifest();
        assert_eq!(manifest.frames.len(), 1);
        assert_eq!(manifest.total_frames, 2);
        assert_eq!(manifest.frames[0].file, "frame_0000.ans");
        assert_eq!(manifest.frames[0].label, "splash");
    }

    #[test]
    fn write_to_dir_emits_frames_and_manifest() {
        let dir = tempfile::tempdir().expect("temp dir");
        let recording = sample_recording();
        let written = recording
            .write_to_dir(dir.path())
            .expect("writing the recording");

        assert_eq!(written.len(), 2);
        assert!(dir.path().join("manifest.json").is_file());
        assert!(dir.path().join("frame_0000.ans").is_file());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("frame_0000.ans")).expect("frame"),
            "splash-ansi"
        );
    }
}
