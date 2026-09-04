//! Minimalist Session View
//!
//! A terminal-style chat interface for conversations.
//! This view provides a clean, minimal UI with:
//! - Chat history as simple terminal scrollback
//! - Status indicator with shimmer animation
//! - Simple input line with prompt
//! - Contextual key hints at the bottom

mod layout;
mod rendering;
mod text_utils;
mod view;

#[cfg(test)]
mod tests;

/// Application version
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

// Re-export main types for backwards compatibility
pub use rendering::{EMPTY_SESSION_HINTS, user_turn_lines};
pub use view::{
    BLOCK_CURSOR, COMPOSER_ROWS, ChatMessage, MinimalSessionView, PALETTE_FOOTER_HINT,
    PALETTE_FOOTER_HINT_SHORT, PLACEHOLDER_IDLE, PLACEHOLDER_RUNNING, paint_composer_contents,
};
