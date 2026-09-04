//! Adaptive colors system
//!
//! Automatically detects terminal background and adjusts colors for optimal contrast.

use cortex_core::style::ThemeColors;
use ratatui::style::Color;

/// Check if a background color is light (for theme detection)
pub fn is_light(bg: (u8, u8, u8)) -> bool {
    // Use relative luminance formula (ITU-R BT.709)
    let (r, g, b) = bg;
    let luminance =
        0.2126 * (r as f32 / 255.0) + 0.7152 * (g as f32 / 255.0) + 0.0722 * (b as f32 / 255.0);
    luminance > 0.5
}

/// Blend two colors together with alpha
pub fn blend(fg: (u8, u8, u8), bg: (u8, u8, u8), alpha: f32) -> (u8, u8, u8) {
    let alpha = alpha.clamp(0.0, 1.0);
    let inv_alpha = 1.0 - alpha;

    let r = (fg.0 as f32 * alpha + bg.0 as f32 * inv_alpha).round() as u8;
    let g = (fg.1 as f32 * alpha + bg.1 as f32 * inv_alpha).round() as u8;
    let b = (fg.2 as f32 * alpha + bg.2 as f32 * inv_alpha).round() as u8;

    (r, g, b)
}

/// Try to detect terminal background color
///
/// This attempts to query the terminal for its background color using
/// OSC 11 escape sequence. Returns None if detection fails.
pub fn detect_terminal_bg() -> Option<(u8, u8, u8)> {
    // Try environment variables first
    if let Ok(colorfgbg) = std::env::var("COLORFGBG") {
        // Format: "fg;bg" where bg is typically 0 (dark) or 15 (light)
        if let Some(bg_str) = colorfgbg.split(';').next_back()
            && let Ok(bg_num) = bg_str.parse::<u8>()
        {
            return match bg_num {
                0 => Some((0, 0, 0)),        // Black background
                15 => Some((255, 255, 255)), // White background
                _ => None,
            };
        }
    }

    // Check for common terminal theme environment variables
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        // Most modern terminals default to dark theme
        if term_program.contains("iTerm")
            || term_program.contains("Alacritty")
            || term_program.contains("kitty")
            || term_program.contains("WezTerm")
        {
            // Default assumption for modern terminals
            return None; // Let caller use default dark
        }
    }

    // Check COLORTERM for true color support hint
    if std::env::var("COLORTERM").is_ok() {
        // Terminal supports true color, but we can't determine bg without
        // more invasive terminal queries
        return None;
    }

    None
}

/// Adaptive color palette that adjusts to terminal background
#[derive(Debug, Clone)]
pub struct AdaptiveColors {
    /// Selection accent — the Cortex violet, for the focused `>` caret and label only
    pub accent: Color,
    /// Primary text color
    pub text: Color,
    /// Secondary/dimmed text color
    pub text_dim: Color,
    /// Very subtle/muted text color
    pub text_muted: Color,
    /// Bar behind a past user turn
    pub user_bg: Color,
    /// Filled charcoal panel for tips / info blocks
    pub panel_bg: Color,
    /// Hairline / border color for UI elements
    pub border: Color,
    /// Success color — green, for `✓` checks
    pub success: Color,
    /// Diff-addition color — the same green, for `+N`
    pub diff_add: Color,
    /// Error/danger color (red)
    pub error: Color,
    /// Warning/caution color (amber/yellow)
    pub warning: Color,
    /// Thinking status color (muted gold)
    pub thinking: Color,
    /// Selection bar background (dark gray)
    pub selection: Color,
}

impl AdaptiveColors {
    /// Create colors by auto-detecting terminal background
    pub fn from_terminal() -> Self {
        match detect_terminal_bg() {
            Some(bg) if is_light(bg) => Self::light_theme(bg),
            Some(bg) => Self::dark_theme(bg),
            None => Self::default_dark(),
        }
    }

    /// Create dark theme colors adapted to the given background
    pub fn dark_theme(bg: (u8, u8, u8)) -> Self {
        // Blend the structural grays with the background for better
        // integration; the accent and the semantic colors stay locked.
        let text_dim_rgb = blend((0x6B, 0x72, 0x80), bg, 0.9);
        let text_muted_rgb = blend((0x4B, 0x55, 0x63), bg, 0.9);
        let border_rgb = blend((0x3A, 0x3A, 0x3A), bg, 0.9);
        let user_bg_rgb = blend((0x1C, 0x1C, 0x1C), bg, 0.8);
        let panel_rgb = blend((0x14, 0x14, 0x14), bg, 0.8);
        let selection_rgb = blend((0x22, 0x1A, 0x38), bg, 0.9);

        Self {
            accent: cortex_core::style::ACCENT,
            text: cortex_core::style::TEXT,
            text_dim: Color::Rgb(text_dim_rgb.0, text_dim_rgb.1, text_dim_rgb.2),
            text_muted: Color::Rgb(text_muted_rgb.0, text_muted_rgb.1, text_muted_rgb.2),
            user_bg: Color::Rgb(user_bg_rgb.0, user_bg_rgb.1, user_bg_rgb.2),
            panel_bg: Color::Rgb(panel_rgb.0, panel_rgb.1, panel_rgb.2),
            border: Color::Rgb(border_rgb.0, border_rgb.1, border_rgb.2),
            success: cortex_core::style::SUCCESS,
            diff_add: cortex_core::style::DIFF_ADD,
            error: cortex_core::style::ERROR,
            warning: cortex_core::style::WARNING,
            thinking: cortex_core::style::THINKING,
            selection: Color::Rgb(selection_rgb.0, selection_rgb.1, selection_rgb.2),
        }
    }

    /// Create light theme colors adapted to the given background
    pub fn light_theme(bg: (u8, u8, u8)) -> Self {
        // Darker violet for contrast on light backgrounds
        let accent_rgb = (0x7C, 0x3A, 0xED);

        // Blend colors with background for better integration
        let text_dim_rgb = blend((0x60, 0x60, 0x60), bg, 0.9);
        let text_muted_rgb = blend((0xA0, 0xA0, 0xA0), bg, 0.9);
        let border_rgb = blend((0xC0, 0xC0, 0xC0), bg, 0.9);
        let user_bg_rgb = blend((0xF0, 0xF0, 0xF0), bg, 0.8);
        let panel_rgb = blend((0xF5, 0xF5, 0xF5), bg, 0.8);
        let selection_rgb = blend((0xE0, 0xE0, 0xE0), bg, 0.9);

        Self {
            accent: Color::Rgb(accent_rgb.0, accent_rgb.1, accent_rgb.2),
            text: Color::Rgb(0x1A, 0x1A, 0x1A),
            text_dim: Color::Rgb(text_dim_rgb.0, text_dim_rgb.1, text_dim_rgb.2),
            text_muted: Color::Rgb(text_muted_rgb.0, text_muted_rgb.1, text_muted_rgb.2),
            user_bg: Color::Rgb(user_bg_rgb.0, user_bg_rgb.1, user_bg_rgb.2),
            panel_bg: Color::Rgb(panel_rgb.0, panel_rgb.1, panel_rgb.2),
            border: Color::Rgb(border_rgb.0, border_rgb.1, border_rgb.2),
            success: Color::Rgb(0x16, 0xA3, 0x4A), // Darker green for light bg
            diff_add: Color::Rgb(0x16, 0xA3, 0x4A),
            error: Color::Rgb(0xD9, 0x3D, 0x3D), // Darker red for light bg
            warning: Color::Rgb(0xC9, 0x9A, 0x2E), // Darker amber for light bg
            thinking: Color::Rgb(0x9A, 0x7B, 0x2E), // Darker gold for light bg
            selection: Color::Rgb(selection_rgb.0, selection_rgb.1, selection_rgb.2),
        }
    }

    /// Create default dark theme colors when detection fails — the locked
    /// palette from `cortex_core::style`.
    pub fn default_dark() -> Self {
        Self {
            accent: cortex_core::style::ACCENT,          // #A78BFA violet
            text: cortex_core::style::TEXT,              // #F5F5F5
            text_dim: cortex_core::style::TEXT_DIM,      // #6B7280
            text_muted: cortex_core::style::TEXT_MUTED,  // #4B5563
            user_bg: cortex_core::style::USER_TURN_BG,   // #1C1C1C
            panel_bg: cortex_core::style::PANEL_BG,      // #141414
            border: cortex_core::style::HAIRLINE,        // #3A3A3A
            success: cortex_core::style::SUCCESS,        // #4ADE80
            diff_add: cortex_core::style::DIFF_ADD,      // #4ADE80
            error: cortex_core::style::ERROR,            // #F87171
            warning: cortex_core::style::WARNING,        // #FFC857
            thinking: cortex_core::style::THINKING,      // #C9A95C
            selection: cortex_core::style::SELECTION_BG, // #221A38
        }
    }
}

impl AdaptiveColors {
    /// Create colors from a named theme (dark, light, ocean_dark, monokai)
    pub fn from_theme_name(name: &str) -> Self {
        let theme = ThemeColors::from_name(name);
        Self::from_theme_colors(&theme)
    }

    /// Create AdaptiveColors from a ThemeColors instance
    pub fn from_theme_colors(theme: &ThemeColors) -> Self {
        // Light backgrounds blend their own selection tint; dark themes use
        // the locked dark gray bar.
        let selection = match theme.background {
            Color::Rgb(r, g, b) if is_light((r, g, b)) => {
                let blended = blend((0x80, 0x80, 0x80), (r, g, b), 0.2);
                Color::Rgb(blended.0, blended.1, blended.2)
            }
            _ => cortex_core::style::SELECTION_BG,
        };

        Self {
            // The theme's primary is its selection accent.
            accent: theme.primary,
            text: theme.text,
            text_dim: theme.text_dim,
            text_muted: theme.text_muted,
            user_bg: theme.surface[1],
            panel_bg: theme.surface[0],
            border: theme.border,
            success: theme.success,
            diff_add: cortex_core::style::DIFF_ADD,
            error: theme.error,
            warning: theme.warning,
            thinking: cortex_core::style::THINKING,
            selection,
        }
    }

    /// Get available theme names
    pub fn available_themes() -> &'static [&'static str] {
        ThemeColors::available_themes()
    }
}

impl Default for AdaptiveColors {
    fn default() -> Self {
        Self::from_terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_light() {
        assert!(is_light((255, 255, 255))); // White
        assert!(is_light((200, 200, 200))); // Light gray
        assert!(!is_light((0, 0, 0))); // Black
        assert!(!is_light((30, 30, 30))); // Dark gray
    }

    #[test]
    fn test_blend() {
        // Full alpha = foreground
        assert_eq!(blend((255, 0, 0), (0, 0, 255), 1.0), (255, 0, 0));
        // Zero alpha = background
        assert_eq!(blend((255, 0, 0), (0, 0, 255), 0.0), (0, 0, 255));
        // Half blend
        let result = blend((255, 0, 0), (0, 0, 255), 0.5);
        assert_eq!(result, (128, 0, 128)); // Purple-ish
    }

    #[test]
    fn test_default_dark_colors() {
        let colors = AdaptiveColors::default_dark();
        // The accent is the selection violet.
        assert!(matches!(colors.accent, Color::Rgb(0xA7, 0x8B, 0xFA)));
        // Green covers `✓` and diff additions alike.
        assert!(matches!(colors.diff_add, Color::Rgb(0x4A, 0xDE, 0x80)));
        assert_eq!(colors.success, colors.diff_add);
        // The selection bar is `#221A38`.
        assert!(matches!(colors.selection, Color::Rgb(0x22, 0x1A, 0x38)));
        assert!(matches!(colors.user_bg, Color::Rgb(0x1C, 0x1C, 0x1C)));
        assert!(matches!(colors.panel_bg, Color::Rgb(0x14, 0x14, 0x14)));
        assert!(matches!(colors.border, Color::Rgb(0x3A, 0x3A, 0x3A)));
        assert!(matches!(colors.text_dim, Color::Rgb(0x6B, 0x72, 0x80)));
        // Thinking is the muted gold, distinct from the warning amber.
        assert_ne!(colors.thinking, colors.warning);
    }

    #[test]
    fn test_dark_theme() {
        let colors = AdaptiveColors::dark_theme((0x1A, 0x1A, 0x1A));
        assert!(matches!(colors.accent, Color::Rgb(0xA7, 0x8B, 0xFA)));
        // Structural grays stay neutral after blending with the background.
        for color in [colors.user_bg, colors.border] {
            let Color::Rgb(r, g, b) = color else {
                panic!("{color:?} must be RGB");
            };
            assert!(r == g && g == b, "{color:?} is not a neutral gray");
        }
    }

    #[test]
    fn test_light_theme() {
        let colors = AdaptiveColors::light_theme((255, 255, 255));
        // Light theme should have darker accent for contrast
        assert!(matches!(colors.accent, Color::Rgb(0x7C, 0x3A, 0xED)));
    }

    #[test]
    fn test_from_theme_name() {
        let dark_colors = AdaptiveColors::from_theme_name("dark");
        // The session accent is the theme's primary — the selection violet.
        assert!(matches!(dark_colors.accent, Color::Rgb(0xA7, 0x8B, 0xFA)));
        // Dark themes carry the locked selection bar.
        assert!(matches!(
            dark_colors.selection,
            Color::Rgb(0x22, 0x1A, 0x38)
        ));

        let light_colors = AdaptiveColors::from_theme_name("light");
        // Light theme should have different accent
        assert!(matches!(light_colors.accent, Color::Rgb(0x7C, 0x3A, 0xED)));

        let monokai_colors = AdaptiveColors::from_theme_name("monokai");
        // Monokai has green accent
        assert!(matches!(monokai_colors.accent, Color::Rgb(166, 226, 46)));
    }

    #[test]
    fn test_from_theme_colors() {
        use cortex_core::style::ThemeColors;

        let theme = ThemeColors::ocean_dark();
        let colors = AdaptiveColors::from_theme_colors(&theme);

        // The session accent is the theme's primary, reserved for the
        // focused selection.
        assert_eq!(colors.accent, theme.primary);
        assert_eq!(colors.text, theme.text);
        assert_eq!(colors.text_dim, theme.text_dim);
        assert_eq!(colors.text_muted, theme.text_muted);
        assert_eq!(colors.border, theme.border);
        assert_eq!(colors.success, theme.success);
        assert_eq!(colors.diff_add, cortex_core::style::DIFF_ADD);
        assert_eq!(colors.error, theme.error);
        assert_eq!(colors.warning, theme.warning);
    }

    #[test]
    fn test_available_themes() {
        let themes = AdaptiveColors::available_themes();
        assert!(themes.contains(&"dark"));
        assert!(themes.contains(&"light"));
        assert!(themes.contains(&"ocean_dark"));
        assert!(themes.contains(&"monokai"));
    }
}
