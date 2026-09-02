//! Plus-ASCII border characters for table rendering.
//!
//! Markdown tables render with the classic `+---+` grid — `+` at every
//! corner and junction, `-` rules, `|` column separators — never Unicode box
//! drawing:
//!
//! ```text
//! +------------+----------+--------------+
//! | Model      | Effort   | Billing      |
//! +------------+----------+--------------+
//! | Mini 1     | Medium   | per request  |
//! +------------+----------+--------------+
//! ```

/// Top-left corner: +
pub const TOP_LEFT: char = '+';
/// Top-right corner: +
pub const TOP_RIGHT: char = '+';
/// Bottom-left corner: +
pub const BOTTOM_LEFT: char = '+';
/// Bottom-right corner: +
pub const BOTTOM_RIGHT: char = '+';
/// Horizontal rule: -
pub const HORIZONTAL: char = '-';
/// Vertical separator: |
pub const VERTICAL: char = '|';
/// Cross junction: +
pub const CROSS: char = '+';
/// Top junction: +
pub const T_DOWN: char = '+';
/// Bottom junction: +
pub const T_UP: char = '+';
/// Left junction: +
pub const T_RIGHT: char = '+';
/// Right junction: +
pub const T_LEFT: char = '+';
