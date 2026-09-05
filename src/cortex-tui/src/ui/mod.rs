//! UI module for the minimalist Cortex TUI.
//!
//! This module provides the core UI infrastructure:
//! - Adaptive colors that detect terminal background
//! - Shimmer animation effects for status text
//! - Layout constants for consistent spacing
//!
//! ## Architecture
//!
//! The session is frameless — no box is drawn around it and nothing paints
//! a background; content bleeds to the terminal edges:
//!
//! ```text
//! [Chat History - Terminal Scrollback]
//!
//! ● Status Indicator (shimmer) · Working...
//! > Input Composer
//! / commands · Ctrl+K palette · ? help
//! ```

pub mod chrome;
pub mod colors;
pub mod consts;
pub mod shimmer;
pub mod text_utils;

// Re-export commonly used items
pub use colors::AdaptiveColors;
pub use consts::*;
pub use shimmer::shimmer_spans;
pub use text_utils::{
    AdaptiveHint, HintDisplayMode, MIN_TERMINAL_WIDTH, adaptive_hints, calculate_hint_display_mode,
    first_fitting_line, fit_line, format_hints, model_display_name, trim_dangling_separator,
    truncate_with_ellipsis, wrap_keep_indent, wrap_or_drop,
};
