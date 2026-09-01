//! # Cortex TUI Capture
//!
//! A comprehensive TUI capture and snapshot testing framework for debugging
//! terminal user interfaces in the Cortex CLI ecosystem.
//!
//! ## Overview
//!
//! This crate provides tools for:
//! - Capturing TUI frames as ASCII art snapshots
//! - Recording TUI sessions with all actions and state changes
//! - Generating markdown reports for debugging
//! - Creating test harnesses for TUI components
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    TUI Capture System                           │
//! │                                                                 │
//! │  ┌─────────────┐    ┌──────────────┐    ┌─────────────────┐   │
//! │  │ FrameCapture│───▶│SessionRecorder│───▶│MarkdownExporter │   │
//! │  │             │    │              │    │                 │   │
//! │  │ - ASCII art │    │ - Events     │    │ - .md reports   │   │
//! │  │ - Metadata  │    │ - Frames     │    │ - ASCII blocks  │   │
//! │  │ - Timing    │    │ - Actions    │    │ - Timestamps    │   │
//! │  └─────────────┘    └──────────────┘    └─────────────────┘   │
//! │                                                                 │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │                    MockTerminal                          │   │
//! │  │  - Simulates terminal for headless testing               │   │
//! │  │  - Captures all rendering operations                     │   │
//! │  │  - Provides frame-by-frame inspection                    │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ### Capturing a single frame
//!
//! ```rust,ignore
//! use cortex_tui_capture::{FrameCapture, CaptureConfig};
//! use ratatui::widgets::Paragraph;
//!
//! let config = CaptureConfig::new(80, 24);
//! let mut capture = FrameCapture::new(config);
//!
//! // Render a widget
//! capture.render(|frame| {
//!     let widget = Paragraph::new("Hello, TUI!");
//!     frame.render_widget(widget, frame.area());
//! });
//!
//! // Export to markdown
//! let md = capture.to_markdown();
//! println!("{}", md);
//! ```
//!
//! ### Recording a session
//!
//! ```rust,ignore
//! use cortex_tui_capture::{SessionRecorder, TuiAction, ActionType};
//!
//! let mut recorder = SessionRecorder::new("my_session", 80, 24);
//!
//! // Record actions
//! recorder.record_action(TuiAction::new(ActionType::KeyPress("Enter".into())));
//! recorder.record_frame("Initial state", &buffer);
//!
//! // Export session report
//! recorder.export_markdown("./debug_output").await?;
//! ```
//!
//! ## Output Format
//!
//! The markdown output includes:
//! - Session metadata (timestamp, terminal size)
//! - Chronological list of actions with timestamps
//! - ASCII captures of TUI state at key moments
//! - Event details with formatted parameters

mod capture;
mod config;
pub mod demo;
mod exporter;
pub mod integration;
mod mock_terminal;
mod recorder;
pub mod screenshot_generator;
mod types;

pub use capture::{BufferSnapshot, FrameCapture, SnapshotCell};
pub use config::{CaptureConfig, OutputFormat, StyleRendering};
pub use demo::{DemoConfig, DemoFrame, DemoManifest, DemoRecording};
pub use exporter::{MarkdownExporter, ReportSection};
pub use integration::{CaptureManager, ExportResult, QuickCapture};
pub use mock_terminal::{MockBackend, MockTerminal};
pub use recorder::{SessionRecorder, SessionReport, SessionStats};
pub use screenshot_generator::{
    DEFAULT_OUTPUT_DIR, GeneratorConfig, GeneratorResult, ScreenshotGenerator, ScreenshotScenario,
    generate_all_screenshots, generate_screenshots_to,
};
pub use types::{ActionType, CaptureError, CaptureResult, CapturedFrame, TuiAction, TuiEvent};

/// Cortex TUI Capture version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Convenience function to create a default capture configuration.
#[inline]
pub fn default_config() -> CaptureConfig {
    CaptureConfig::default()
}

/// Convenience function to create a capture configuration with specific dimensions.
#[inline]
pub fn config_with_size(width: u16, height: u16) -> CaptureConfig {
    CaptureConfig::new(width, height)
}

#[cfg(test)]
mod tests {
    use ratatui::widgets::Paragraph;

    use super::*;

    #[test]
    fn snapshot_empty_loading_error_success_surfaces() {
        for (label, copy) in [
            ("empty", "Cortex CLI v1.0.0"),
            ("loading", "Waiting for browser authentication"),
            ("error", "The coding service is temporarily unavailable"),
            ("success", "Signed in."),
        ] {
            let config = CaptureConfig::new(80, 12);
            let mut capture = FrameCapture::new(config);
            let mut terminal = MockTerminal::with_size(80, 12).expect("mock terminal");
            terminal
                .draw(|frame| {
                    let widget = Paragraph::new(copy);
                    frame.render_widget(widget, frame.area());
                })
                .expect("draw");
            capture.capture_ratatui(terminal.backend().buffer(), Some(label));
            let frame = capture.latest_frame().expect("frame");
            assert!(
                frame.ascii_content.contains(copy),
                "{label} snapshot missing copy:\n{}",
                frame.ascii_content
            );
        }
    }

    #[test]
    fn snapshot_splash_reflows_narrow_and_wide() {
        use cortex_tui_components::welcome_card::{ToLines, WelcomeCard};
        for (w, h) in [(40u16, 12u16), (120u16, 40u16)] {
            let config = CaptureConfig::new(w, h);
            let mut capture = FrameCapture::new(config);
            let mut terminal = MockTerminal::with_size(w, h).expect("mock terminal");
            terminal
                .draw(|frame| {
                    let lines = WelcomeCard::new().version("1.0.0").to_lines(w);
                    frame.render_widget(Paragraph::new(lines), frame.area());
                })
                .expect("draw");
            capture.capture_ratatui(terminal.backend().buffer(), Some("splash"));
            let frame = capture.latest_frame().expect("frame");
            assert!(
                frame.ascii_content.contains("Cortex CLI v1.0.0"),
                "splash missing at {w}x{h}:\n{}",
                frame.ascii_content
            );
            assert!(!frame.ascii_content.contains("▄█▀▀▀▀█▄"));
        }
    }

    #[test]
    fn snapshot_service_unavailable_surface() {
        let config = CaptureConfig::new(72, 10);
        let mut capture = FrameCapture::new(config);
        let mut terminal = MockTerminal::with_size(72, 10).expect("mock terminal");
        terminal
            .draw(|frame| {
                let widget = Paragraph::new("The coding service is temporarily unavailable");
                frame.render_widget(widget, frame.area());
            })
            .expect("draw");
        capture.capture_ratatui(terminal.backend().buffer(), Some("service-unavailable"));
        let frame = capture
            .latest_frame()
            .expect("expected a captured TUI frame");
        assert!(
            frame
                .ascii_content
                .contains("The coding service is temporarily unavailable"),
            "snapshot missing product-facing error:\n{}",
            frame.ascii_content
        );
        let lower = frame.ascii_content.to_lowercase();
        assert!(!lower.contains("reqwest"));
        assert!(!lower.contains("openai"));
        assert!(!lower.contains("grok"));
    }

    #[test]
    fn mock_connection_error_is_product_facing() {
        use crate::screenshot_generator::mocks::StateMocks;
        struct Harness;
        impl StateMocks for Harness {}
        let scenario = crate::screenshot_generator::ScreenshotScenario::new(
            "error_connection",
            "connection",
            "error",
            "API down",
        );
        let content = Harness
            .create_state_content(&scenario)
            .expect("error_connection mock");
        assert!(content.contains("The coding service is temporarily unavailable"));
        assert!(content.contains("api.cortex.foundation"));
        assert!(!content.contains("api.cortex.ai"));
        assert!(!content.to_lowercase().contains("grok"));
    }
}
