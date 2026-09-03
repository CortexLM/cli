//! Text utilities for adaptive TUI rendering.
//!
//! Provides functions for truncating text and adapting hints
//! to fit within available terminal width. This module helps
//! ensure the TUI remains usable even on narrow terminal windows.

use unicode_width::UnicodeWidthStr;

/// Minimum terminal width we support gracefully.
///
/// Below this width, the TUI may not render all elements properly,
/// but we still attempt to show something useful.
pub const MIN_TERMINAL_WIDTH: u16 = 40;

/// Truncate text with ellipsis if it exceeds max_width.
///
/// Uses unicode-aware width calculation for proper handling of
/// CJK characters and other wide glyphs.
///
/// # Arguments
///
/// * `text` - The text to potentially truncate
/// * `max_width` - Maximum display width in terminal columns
///
/// # Returns
///
/// The original text if it fits within `max_width`, otherwise
/// a truncated version with "..." appended.
///
/// # Examples
///
/// ```
/// use cortex_tui::ui::text_utils::truncate_with_ellipsis;
///
/// assert_eq!(truncate_with_ellipsis("Hello World", 8), "Hello...");
/// assert_eq!(truncate_with_ellipsis("Hi", 8), "Hi");
/// assert_eq!(truncate_with_ellipsis("Test", 3), "...");
/// ```
pub fn truncate_with_ellipsis(text: &str, max_width: usize) -> String {
    // Use unicode-aware width calculation for proper handling of
    // CJK characters and other wide glyphs
    let text_width = UnicodeWidthStr::width(text);

    if text_width <= max_width {
        return text.to_string();
    }

    // Need at least 3 columns for "..."
    if max_width < 3 {
        return "...".chars().take(max_width).collect();
    }

    let target_width = max_width.saturating_sub(3);
    let mut result = String::new();
    let mut current_width = 0;

    for ch in text.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width + ch_width > target_width {
            break;
        }
        result.push(ch);
        current_width += ch_width;
    }

    result.push_str("...");
    result
}

/// Wrap `text` on word boundaries. Words that cannot fit on a line are dropped
/// instead of being cut mid-word.
pub fn wrap_or_drop(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for word in text.split_whitespace() {
        let word_width = UnicodeWidthStr::width(word);
        if word_width > max_width {
            continue;
        }
        if current.is_empty() {
            current = word.to_string();
            current_width = word_width;
        } else if current_width + 1 + word_width <= max_width {
            current.push(' ');
            current.push_str(word);
            current_width += 1 + word_width;
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
            current_width = word_width;
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// First wrapped line, or empty when nothing fits without cutting a word.
pub fn first_fitting_line(text: &str, max_width: usize) -> String {
    wrap_or_drop(text, max_width)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Fit `text` into `max_width` while preserving its exact spacing.
///
/// Unlike [`first_fitting_line`], internal runs of spaces (column gaps such
/// as `/pr  Open a pull request`) and leading indentation survive. When the
/// text is too wide it is cut at the last whitespace boundary — never inside
/// a word — and trailing whitespace is dropped.
pub fn fit_line(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return text.trim_end().to_string();
    }
    let mut cut = 0usize;
    let mut width = 0usize;
    for (idx, ch) in text.char_indices() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > max_width {
            if ch.is_whitespace() {
                cut = idx;
            }
            break;
        }
        if ch.is_whitespace() {
            cut = idx;
        }
        width += ch_width;
    }
    text[..cut].trim_end().to_string()
}

/// Wrap `text` on word boundaries while preserving its leading indentation
/// on every produced line (code excerpts keep their nesting).
pub fn wrap_keep_indent(text: &str, max_width: usize) -> Vec<String> {
    let indent: String = text.chars().take_while(|c| *c == ' ').collect();
    let indent_width = indent.chars().count();
    if indent_width >= max_width {
        return wrap_or_drop(text, max_width);
    }
    wrap_or_drop(text.trim_start(), max_width - indent_width)
        .into_iter()
        .map(|line| format!("{indent}{line}"))
        .collect()
}

/// Drop a dangling ` ·` separator left behind by a word-boundary cut.
pub fn trim_dangling_separator(text: &str) -> String {
    text.trim_end().trim_end_matches('·').trim_end().to_string()
}

/// Abbreviated versions of common hint descriptions.
///
/// Maps full description to a shorter version for use when
/// terminal width is limited.
fn abbreviate_hint(description: &str) -> &str {
    match description {
        "navigate" => "nav",
        "select" => "sel",
        "cancel" => "esc",
        "close" => "cls",
        "confirm" => "ok",
        "interrupt" => "int",
        "force quit" => "quit",
        "approve" => "y",
        "reject" => "n",
        "filter" => "/",
        _ => description,
    }
}

/// Represents a key hint that can be displayed in abbreviated form.
///
/// Key hints are displayed at the bottom of TUI elements to show
/// available keyboard shortcuts. This struct allows hints to be
/// rendered at different verbosity levels based on available space.
#[derive(Debug, Clone)]
pub struct AdaptiveHint {
    /// The key or key combination (e.g., "Enter", "Ctrl+C")
    pub key: &'static str,
    /// Full description of the action (e.g., "select", "cancel")
    pub description: &'static str,
    /// Priority (lower = more important, shown first when space is limited)
    pub priority: u8,
}

impl AdaptiveHint {
    /// Create a new adaptive hint with default priority.
    ///
    /// # Arguments
    ///
    /// * `key` - The key or key combination
    /// * `description` - Full description of the action
    pub fn new(key: &'static str, description: &'static str) -> Self {
        Self {
            key,
            description,
            priority: 5,
        }
    }

    /// Set the priority for this hint.
    ///
    /// Lower priority values are more important and will be shown
    /// first when space is limited.
    ///
    /// # Arguments
    ///
    /// * `priority` - Priority value (0-255, lower = more important)
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// Calculate width when fully displayed: "Key description".
    ///
    /// Returns the number of terminal columns needed to display
    /// this hint in full form with a space separator.
    pub fn full_width(&self) -> usize {
        UnicodeWidthStr::width(self.key) + 1 + UnicodeWidthStr::width(self.description)
    }

    /// Calculate width when abbreviated: "Key abbr".
    ///
    /// Returns the number of terminal columns needed to display
    /// this hint with an abbreviated description.
    pub fn abbreviated_width(&self) -> usize {
        UnicodeWidthStr::width(self.key)
            + 1
            + UnicodeWidthStr::width(abbreviate_hint(self.description))
    }

    /// Calculate width for key only.
    ///
    /// Returns the number of terminal columns needed to display
    /// just the key without any description.
    pub fn key_only_width(&self) -> usize {
        UnicodeWidthStr::width(self.key)
    }
}

/// Display mode for hints based on available width.
///
/// Determines how hints should be rendered based on the
/// terminal space available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintDisplayMode {
    /// Full display: "Enter select · Esc close"
    Full,
    /// Abbreviated: "Enter sel · Esc cls"
    Abbreviated,
    /// Keys only: "Enter · Esc"
    KeysOnly,
    /// Too narrow, show nothing or "..."
    Minimal,
}

/// Calculate which display mode to use based on available width.
///
/// Evaluates hints from most verbose (Full) to least verbose (Minimal)
/// and returns the first mode that fits within the available width.
///
/// # Arguments
///
/// * `hints` - The hints to display
/// * `available_width` - Width in terminal columns
/// * `separator_width` - Width of separator between hints (typically 3 for " · ")
///
/// # Returns
///
/// The appropriate `HintDisplayMode` for the given constraints.
pub fn calculate_hint_display_mode(
    hints: &[AdaptiveHint],
    available_width: usize,
    separator_width: usize,
) -> HintDisplayMode {
    if hints.is_empty() || available_width < 5 {
        return HintDisplayMode::Minimal;
    }

    // Calculate total width needed for separators
    let separators_width = if hints.len() > 1 {
        (hints.len() - 1) * separator_width
    } else {
        0
    };

    // Check if full display fits
    let full_width: usize = hints.iter().map(|h| h.full_width()).sum::<usize>() + separators_width;
    if full_width <= available_width {
        return HintDisplayMode::Full;
    }

    // Check if abbreviated display fits
    let abbrev_width: usize =
        hints.iter().map(|h| h.abbreviated_width()).sum::<usize>() + separators_width;
    if abbrev_width <= available_width {
        return HintDisplayMode::Abbreviated;
    }

    // Check if keys-only display fits
    let keys_width: usize =
        hints.iter().map(|h| h.key_only_width()).sum::<usize>() + separators_width;
    if keys_width <= available_width {
        return HintDisplayMode::KeysOnly;
    }

    HintDisplayMode::Minimal
}

/// Format hints according to the display mode.
///
/// Renders the given hints as a string using the specified display mode.
///
/// # Arguments
///
/// * `hints` - The hints to format
/// * `mode` - The display mode to use
/// * `separator` - The separator string between hints (e.g., " · ")
///
/// # Returns
///
/// A formatted string ready to display.
pub fn format_hints(hints: &[AdaptiveHint], mode: HintDisplayMode, separator: &str) -> String {
    match mode {
        HintDisplayMode::Full => hints
            .iter()
            .map(|h| format!("{} {}", h.key, h.description))
            .collect::<Vec<_>>()
            .join(separator),
        HintDisplayMode::Abbreviated => hints
            .iter()
            .map(|h| format!("{} {}", h.key, abbreviate_hint(h.description)))
            .collect::<Vec<_>>()
            .join(separator),
        HintDisplayMode::KeysOnly => hints
            .iter()
            .map(|h| h.key.to_string())
            .collect::<Vec<_>>()
            .join(separator),
        HintDisplayMode::Minimal => {
            if hints.is_empty() {
                String::new()
            } else {
                "...".to_string()
            }
        }
    }
}

/// Convenience function: format hints adaptively for the given width.
///
/// Creates `AdaptiveHint` instances from tuples and automatically
/// selects the best display mode for the available width.
///
/// # Arguments
///
/// * `hints` - Slice of (key, description) tuples
/// * `available_width` - Width in terminal columns
///
/// # Returns
///
/// A formatted string with hints separated by " · ".
///
/// # Examples
///
/// ```
/// use cortex_tui::ui::text_utils::adaptive_hints;
///
/// // Wide terminal: shows full hints
/// let full = adaptive_hints(&[("Enter", "select"), ("Esc", "close")], 50);
/// assert_eq!(full, "Enter select · Esc close");
///
/// // Narrow terminal: shows abbreviated hints
/// // "Enter select · Esc close" = 25 chars, so width=20 forces abbreviation
/// let abbrev = adaptive_hints(&[("Enter", "select"), ("Esc", "close")], 20);
/// assert_eq!(abbrev, "Enter sel · Esc cls");
/// ```
pub fn adaptive_hints(hints: &[(&'static str, &'static str)], available_width: usize) -> String {
    let adaptive: Vec<AdaptiveHint> = hints.iter().map(|(k, d)| AdaptiveHint::new(k, d)).collect();

    let mode = calculate_hint_display_mode(&adaptive, available_width, 3);
    format_hints(&adaptive, mode, " · ")
}

/// English product name for a model id — the TUI never shows raw slugs.
///
/// Served/API ids stay hyphenated internally; every user-facing surface maps
/// them here:
///
/// - `cortex-1-mini` → `Cortex Mini 1`
/// - `cortex-1-max` → `Cortex Max 1`
/// - `cortex-1` → `Cortex 1`
///
/// The Cortex family keeps its generation number last. Any other id is
/// title-cased with the hyphens dropped so a slug never reaches the screen.
///
/// # Examples
///
/// ```
/// use cortex_tui::ui::text_utils::model_display_name;
///
/// assert_eq!(model_display_name("cortex-1-mini"), "Cortex Mini 1");
/// assert_eq!(model_display_name("cortex-1-max"), "Cortex Max 1");
/// assert_eq!(model_display_name("cortex-1"), "Cortex 1");
/// assert_eq!(model_display_name("Cortex Mini 1"), "Cortex Mini 1");
/// ```
pub fn model_display_name(id: &str) -> String {
    fn capitalize(part: &str) -> String {
        let mut chars = part.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }

    let slug = id.rsplit('/').next().unwrap_or(id).trim();
    if slug.is_empty() || !slug.contains('-') {
        return slug.to_string();
    }
    let parts: Vec<&str> = slug.split('-').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 2
        && parts[0].eq_ignore_ascii_case("cortex")
        && parts[1].chars().all(|c| c.is_ascii_digit())
    {
        let generation = parts[1];
        let variants: Vec<String> = parts[2..].iter().map(|p| capitalize(p)).collect();
        return if variants.is_empty() {
            format!("Cortex {generation}")
        } else {
            format!("Cortex {} {generation}", variants.join(" "))
        };
    }
    parts
        .iter()
        .map(|p| capitalize(p))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== truncate_with_ellipsis tests ====================

    #[test]
    fn should_return_original_text_when_shorter_than_max_width() {
        assert_eq!(truncate_with_ellipsis("Hi", 10), "Hi");
        assert_eq!(truncate_with_ellipsis("Hello", 5), "Hello");
        assert_eq!(truncate_with_ellipsis("", 5), "");
    }

    #[test]
    fn should_truncate_with_ellipsis_when_exceeds_max_width() {
        assert_eq!(truncate_with_ellipsis("Hello World", 8), "Hello...");
        assert_eq!(truncate_with_ellipsis("Hello World", 6), "Hel...");
    }

    #[test]
    fn should_handle_exact_boundary_length() {
        // "Test" is 4 chars, max_width=4 should return as-is
        assert_eq!(truncate_with_ellipsis("Test", 4), "Test");
        // "Test" is 4 chars, max_width=3 should truncate
        assert_eq!(truncate_with_ellipsis("Test", 3), "...");
    }

    #[test]
    fn should_handle_very_small_max_width() {
        assert_eq!(truncate_with_ellipsis("Hello", 3), "...");
        assert_eq!(truncate_with_ellipsis("Hello", 2), "..");
        assert_eq!(truncate_with_ellipsis("Hello", 1), ".");
        assert_eq!(truncate_with_ellipsis("Hello", 0), "");
    }

    #[test]
    fn should_handle_unicode_characters() {
        // CJK characters are typically 2 columns wide
        // "日本語" = 6 columns (3 chars × 2 columns each)
        let japanese = "日本語";
        assert_eq!(UnicodeWidthStr::width(japanese), 6);

        // With max_width=6, should fit exactly
        assert_eq!(truncate_with_ellipsis(japanese, 6), "日本語");

        // With max_width=5, need ellipsis (3 cols), so only 2 cols for text
        // "日" = 2 cols, fits in target_width=2
        assert_eq!(truncate_with_ellipsis(japanese, 5), "日...");
    }

    #[test]
    fn should_handle_emoji() {
        // Most emoji are 2 columns wide
        // "Hi 👋" = 1 + 1 + 1 + 2 = 5 columns
        let emoji_text = "Hi 👋";
        assert_eq!(UnicodeWidthStr::width(emoji_text), 5);

        // Fits exactly in 5 columns
        assert_eq!(truncate_with_ellipsis(emoji_text, 5), "Hi 👋");

        // With max_width=4, need ellipsis (3 cols), so only 1 col for text = "H..."
        assert_eq!(truncate_with_ellipsis(emoji_text, 4), "H...");

        // Longer emoji text: "Hello 🌍 World" = 5 + 1 + 2 + 1 + 5 = 14 columns
        // At max_width=10: target_width=7, "Hello " = 6 cols, but adding 🌍 (2 cols) = 8 > 7
        // So we get "Hello " (6 cols) + "..." = "Hello ..."
        let long_emoji = "Hello 🌍 World";
        assert_eq!(UnicodeWidthStr::width(long_emoji), 14);
        assert_eq!(truncate_with_ellipsis(long_emoji, 10), "Hello ...");

        // At max_width=11: target_width=8, "Hello 🌍" = 8 cols exactly
        assert_eq!(truncate_with_ellipsis(long_emoji, 11), "Hello 🌍...");
    }

    // ==================== abbreviate_hint tests ====================

    #[test]
    fn should_abbreviate_known_descriptions() {
        assert_eq!(abbreviate_hint("navigate"), "nav");
        assert_eq!(abbreviate_hint("select"), "sel");
        assert_eq!(abbreviate_hint("cancel"), "esc");
        assert_eq!(abbreviate_hint("close"), "cls");
        assert_eq!(abbreviate_hint("confirm"), "ok");
        assert_eq!(abbreviate_hint("interrupt"), "int");
        assert_eq!(abbreviate_hint("force quit"), "quit");
        assert_eq!(abbreviate_hint("approve"), "y");
        assert_eq!(abbreviate_hint("reject"), "n");
        assert_eq!(abbreviate_hint("filter"), "/");
    }

    #[test]
    fn should_return_original_for_unknown_descriptions() {
        assert_eq!(abbreviate_hint("custom"), "custom");
        assert_eq!(abbreviate_hint("unknown action"), "unknown action");
    }

    // ==================== AdaptiveHint tests ====================

    #[test]
    fn should_create_hint_with_default_priority() {
        let hint = AdaptiveHint::new("Enter", "select");
        assert_eq!(hint.key, "Enter");
        assert_eq!(hint.description, "select");
        assert_eq!(hint.priority, 5);
    }

    #[test]
    fn should_allow_setting_custom_priority() {
        let hint = AdaptiveHint::new("Enter", "select").with_priority(1);
        assert_eq!(hint.priority, 1);
    }

    #[test]
    fn should_calculate_full_width_correctly() {
        let hint = AdaptiveHint::new("Enter", "select");
        // "Enter" (5) + " " (1) + "select" (6) = 12
        assert_eq!(hint.full_width(), 12);
    }

    #[test]
    fn should_calculate_abbreviated_width_correctly() {
        let hint = AdaptiveHint::new("Enter", "select");
        // "Enter" (5) + " " (1) + "sel" (3) = 9
        assert_eq!(hint.abbreviated_width(), 9);
    }

    #[test]
    fn should_calculate_key_only_width_correctly() {
        let hint = AdaptiveHint::new("Ctrl+C", "interrupt");
        // "Ctrl+C" = 6
        assert_eq!(hint.key_only_width(), 6);
    }

    // ==================== calculate_hint_display_mode tests ====================

    #[test]
    fn should_return_minimal_for_empty_hints() {
        let hints: Vec<AdaptiveHint> = vec![];
        assert_eq!(
            calculate_hint_display_mode(&hints, 100, 3),
            HintDisplayMode::Minimal
        );
    }

    #[test]
    fn should_return_minimal_for_very_narrow_width() {
        let hints = vec![AdaptiveHint::new("Enter", "select")];
        assert_eq!(
            calculate_hint_display_mode(&hints, 4, 3),
            HintDisplayMode::Minimal
        );
    }

    #[test]
    fn should_return_full_when_space_available() {
        let hints = vec![
            AdaptiveHint::new("Enter", "select"),
            AdaptiveHint::new("Esc", "close"),
        ];
        // "Enter select" (12) + " · " (3) + "Esc close" (9) = 24
        assert_eq!(
            calculate_hint_display_mode(&hints, 30, 3),
            HintDisplayMode::Full
        );
    }

    #[test]
    fn should_return_abbreviated_when_full_does_not_fit() {
        let hints = vec![
            AdaptiveHint::new("Enter", "select"),
            AdaptiveHint::new("Esc", "close"),
        ];
        // Full: 24 cols, Abbreviated: "Enter sel" (9) + " · " (3) + "Esc cls" (7) = 19
        assert_eq!(
            calculate_hint_display_mode(&hints, 20, 3),
            HintDisplayMode::Abbreviated
        );
    }

    #[test]
    fn should_return_keys_only_when_abbreviated_does_not_fit() {
        let hints = vec![
            AdaptiveHint::new("Enter", "select"),
            AdaptiveHint::new("Esc", "close"),
        ];
        // Abbreviated: 19 cols, Keys only: "Enter" (5) + " · " (3) + "Esc" (3) = 11
        assert_eq!(
            calculate_hint_display_mode(&hints, 15, 3),
            HintDisplayMode::KeysOnly
        );
    }

    #[test]
    fn should_return_minimal_when_even_keys_do_not_fit() {
        let hints = vec![
            AdaptiveHint::new("Enter", "select"),
            AdaptiveHint::new("Esc", "close"),
        ];
        // Keys only: 11 cols
        assert_eq!(
            calculate_hint_display_mode(&hints, 8, 3),
            HintDisplayMode::Minimal
        );
    }

    // ==================== format_hints tests ====================

    #[test]
    fn should_format_hints_in_full_mode() {
        let hints = vec![
            AdaptiveHint::new("Enter", "select"),
            AdaptiveHint::new("Esc", "close"),
        ];
        assert_eq!(
            format_hints(&hints, HintDisplayMode::Full, " · "),
            "Enter select · Esc close"
        );
    }

    #[test]
    fn should_format_hints_in_abbreviated_mode() {
        let hints = vec![
            AdaptiveHint::new("Enter", "select"),
            AdaptiveHint::new("Esc", "close"),
        ];
        assert_eq!(
            format_hints(&hints, HintDisplayMode::Abbreviated, " · "),
            "Enter sel · Esc cls"
        );
    }

    #[test]
    fn should_format_hints_in_keys_only_mode() {
        let hints = vec![
            AdaptiveHint::new("Enter", "select"),
            AdaptiveHint::new("Esc", "close"),
        ];
        assert_eq!(
            format_hints(&hints, HintDisplayMode::KeysOnly, " · "),
            "Enter · Esc"
        );
    }

    #[test]
    fn should_format_hints_in_minimal_mode() {
        let hints = vec![AdaptiveHint::new("Enter", "select")];
        assert_eq!(format_hints(&hints, HintDisplayMode::Minimal, " · "), "...");
    }

    #[test]
    fn should_format_empty_hints_in_minimal_mode() {
        let hints: Vec<AdaptiveHint> = vec![];
        assert_eq!(format_hints(&hints, HintDisplayMode::Minimal, " · "), "");
    }

    #[test]
    fn should_use_custom_separator() {
        let hints = vec![
            AdaptiveHint::new("Enter", "select"),
            AdaptiveHint::new("Esc", "close"),
        ];
        assert_eq!(
            format_hints(&hints, HintDisplayMode::Full, " | "),
            "Enter select | Esc close"
        );
    }

    // ==================== adaptive_hints tests ====================

    #[test]
    fn should_adapt_hints_to_wide_terminal() {
        let result = adaptive_hints(&[("Enter", "select"), ("Esc", "close")], 50);
        assert_eq!(result, "Enter select · Esc close");
    }

    #[test]
    fn should_adapt_hints_to_medium_terminal() {
        let result = adaptive_hints(&[("Enter", "select"), ("Esc", "close")], 20);
        assert_eq!(result, "Enter sel · Esc cls");
    }

    #[test]
    fn should_adapt_hints_to_narrow_terminal() {
        let result = adaptive_hints(&[("Enter", "select"), ("Esc", "close")], 12);
        assert_eq!(result, "Enter · Esc");
    }

    #[test]
    fn should_adapt_hints_to_very_narrow_terminal() {
        let result = adaptive_hints(&[("Enter", "select"), ("Esc", "close")], 5);
        assert_eq!(result, "...");
    }

    #[test]
    fn should_handle_single_hint() {
        let result = adaptive_hints(&[("Enter", "confirm")], 20);
        assert_eq!(result, "Enter confirm");
    }

    #[test]
    fn should_handle_empty_hints_slice() {
        let result = adaptive_hints(&[], 50);
        assert_eq!(result, "");
    }

    #[test]
    fn wrap_or_drop_never_cuts_a_word() {
        let lines = wrap_or_drop("Continue with browser Opens your browser", 12);
        for line in &lines {
            assert!(
                !line.contains("Contin") || line.contains("Continue"),
                "{line}"
            );
            assert!(!line.ends_with("brow"));
        }
        assert!(wrap_or_drop("supercalifragilistic", 8).is_empty());
        assert_eq!(
            first_fitting_line("Paste an API key", 40),
            "Paste an API key"
        );
    }

    #[test]
    fn fit_line_keeps_spacing_and_never_cuts_a_word() {
        // Column gaps and indentation survive when the text fits.
        assert_eq!(
            fit_line("/pr  Open a pull request", 40),
            "/pr  Open a pull request"
        );
        assert_eq!(fit_line("  22  { preHandler }", 40), "  22  { preHandler }");
        // Too wide: cut at a word boundary, trailing space dropped.
        assert_eq!(
            fit_line("cloud  Fix flaky rateLimit test on CI", 22),
            "cloud  Fix flaky"
        );
        assert_eq!(fit_line("supercalifragilistic expialidocious", 10), "");
        assert_eq!(fit_line("anything", 0), "");
    }

    #[test]
    fn wrap_keep_indent_preserves_code_nesting() {
        let lines = wrap_keep_indent("    const n = await redis.zcard(key);", 24);
        assert_eq!(lines[0], "    const n = await");
        assert!(lines.iter().all(|l| l.starts_with("    ")), "{lines:?}");
        assert_eq!(wrap_keep_indent("flush", 10), vec!["flush".to_string()]);
    }

    #[test]
    fn trim_dangling_separator_drops_orphan_dot() {
        assert_eq!(
            trim_dangling_separator("Cortex Mini 1 · Max ·"),
            "Cortex Mini 1 · Max"
        );
        assert_eq!(
            trim_dangling_separator("Cortex Mini 1 · Max"),
            "Cortex Mini 1 · Max"
        );
    }

    #[test]
    fn model_names_are_english_product_names() {
        // The Cortex catalog maps to product names with the generation last.
        assert_eq!(model_display_name("cortex-1-mini"), "Cortex Mini 1");
        assert_eq!(model_display_name("cortex-1-max"), "Cortex Max 1");
        assert_eq!(model_display_name("cortex-1"), "Cortex 1");
        assert_eq!(model_display_name("cortex-2-mini"), "Cortex Mini 2");
        // Provider prefixes are dropped; already-pretty names pass through.
        assert_eq!(model_display_name("cortex/cortex-1-mini"), "Cortex Mini 1");
        assert_eq!(model_display_name("Cortex Mini 1"), "Cortex Mini 1");
        // No hyphenated slug ever reaches the screen.
        assert_eq!(model_display_name("some-other-model"), "Some Other Model");
        assert!(!model_display_name("cortex-1-mini").contains('-'));
    }
}
