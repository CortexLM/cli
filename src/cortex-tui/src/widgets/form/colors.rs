//! Color configuration for form modals.

use cortex_core::style::{
    ACCENT, BORDER_FOCUS, HAIRLINE, PANEL_BG, SURFACE_1, TEXT, TEXT_DIM, TEXT_MUTED,
};
use ratatui::prelude::Color;

/// Colors used by the form modal — the gray chrome: charcoal panel, hairline
/// borders, white/dim copy, the cyan accent on the focused field only.
#[derive(Debug, Clone, Copy)]
pub struct FormModalColors {
    pub background: Color,
    pub border: Color,
    pub border_focused: Color,
    pub text: Color,
    pub text_dim: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub surface: Color,
}

impl Default for FormModalColors {
    fn default() -> Self {
        Self {
            background: PANEL_BG,
            border: HAIRLINE,
            border_focused: BORDER_FOCUS,
            text: TEXT,
            text_dim: TEXT_DIM,
            text_muted: TEXT_MUTED,
            accent: ACCENT,
            surface: SURFACE_1,
        }
    }
}
