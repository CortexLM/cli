//! Cortex Theme - Gray chrome on the host terminal background.
//!
//! The chrome never paints its own background: the terminal shows through
//! (`Color::Reset`, black by default). Structure comes from gray — hairlines,
//! filled charcoal panels, dim secondary copy, white primary copy. The one
//! accent is the Cortex violet `#A78BFA`, reserved for the focused selection:
//! the `>` caret and the selected label on the `#221A38` selection bar.
//! Green exists only for `✓` success and `+` diff additions; red and amber
//! only on diagnostics; a muted gold only on the Thinking status. Mint
//! `#00F5D4` / `#1A3330` is never painted.

use ratatui::style::{Color, Modifier, Style};

// ============================================================
// ACCENT - one violet, focused selection only
// ============================================================

/// Primary accent - Cortex violet for the focused selection (`>` caret + label)
pub const ACCENT: Color = Color::Rgb(167, 139, 250); // #A78BFA

/// Legacy brand slot. Widgets that once painted titles, icons and cursors in
/// the brand colour now get a light gray, so the violet stays on the focused
/// selection only — use `ACCENT` for that.
pub const CYAN_PRIMARY: Color = SKY_BLUE;

/// Soft emphasis - light gray for secondary emphasis (legacy name)
pub const SKY_BLUE: Color = Color::Rgb(212, 212, 216); // #D4D4D8

/// Bright emphasis - near-white for highlights (legacy name)
pub const ELECTRIC_BLUE: Color = Color::Rgb(229, 231, 235); // #E5E7EB

/// Mid gray - links and interactive elements at rest (legacy name)
pub const DEEP_CYAN: Color = Color::Rgb(156, 163, 175); // #9CA3AF

/// Dark gray (legacy name)
pub const TEAL: Color = Color::Rgb(75, 85, 99); // #4B5563

// ============================================================
// BACKGROUND COLORS - the host terminal owns the canvas
// ============================================================

/// Main background — never painted. The host terminal shows through
/// (black by default). Do not swap this back to a solid fill.
pub const VOID: Color = Color::Reset;

/// Filled charcoal panel for tips / info blocks
pub const PANEL_BG: Color = Color::Rgb(20, 20, 20); // #141414

/// Bar behind a past user turn (`> prompt text`) — slightly lighter gray
pub const USER_TURN_BG: Color = Color::Rgb(28, 28, 28); // #1C1C1C

/// Surface level 0 - the charcoal panel
pub const SURFACE_0: Color = PANEL_BG;

/// Surface level 1 - mid neutral surface
pub const SURFACE_1: Color = USER_TURN_BG;

/// Surface level 2 - the selection bar tone
pub const SURFACE_2: Color = Color::Rgb(38, 38, 38); // #262626

/// Surface level 3 - lightest neutral surface
pub const SURFACE_3: Color = Color::Rgb(51, 51, 51); // #333333

// ============================================================
// TEXT COLORS
// ============================================================

/// Primary text — off-white lock copy
pub const TEXT: Color = Color::Rgb(245, 245, 245); // #F5F5F5

/// Dimmed text - secondary copy, placeholders, hints, descriptions
pub const TEXT_DIM: Color = Color::Rgb(107, 114, 128); // #6B7280

/// Muted text - very dim for background elements
pub const TEXT_MUTED: Color = Color::Rgb(75, 85, 99); // #4B5563

/// Bright text - pure white for emphasis
pub const TEXT_BRIGHT: Color = Color::Rgb(255, 255, 255); // #FFFFFF

// ============================================================
// SEMANTIC COLORS
// ============================================================

/// Diff additions - green for `+N` / `+` lines
pub const DIFF_ADD: Color = Color::Rgb(74, 222, 128); // #4ADE80

/// Success - the same green, for `✓` checks only
pub const SUCCESS: Color = DIFF_ADD; // #4ADE80

/// Warning - amber, diagnostics only (`warn`)
pub const WARNING: Color = Color::Rgb(255, 200, 87); // #FFC857

/// Error - red (`error`, `× Stopped`, quota, failed MCP `x`)
pub const ERROR: Color = Color::Rgb(248, 113, 113); // #F87171

/// Thinking status - muted gold, nothing else uses it
pub const THINKING: Color = Color::Rgb(201, 169, 92); // #C9A95C

/// Info - mid gray for informational messages (legacy name)
pub const INFO: Color = DEEP_CYAN; // #9CA3AF

/// Highlight - near-white for emphasis (legacy name)
pub const HIGHLIGHT: Color = ELECTRIC_BLUE; // #E5E7EB

/// Selected-row background — violet-tinted bar `#221A38`. The caret and
/// label on it are `#A78BFA`; everything else stays white/dim. Never invert
/// onto the accent.
pub const SELECTION_BG: Color = Color::Rgb(34, 26, 56); // #221A38

// ============================================================
// BORDER COLORS
// ============================================================

/// Hairline - the thin gray rule above and below the prompt, and around
/// search fields
pub const HAIRLINE: Color = Color::Rgb(58, 58, 58); // #3A3A3A

/// Normal border - the hairline gray
pub const BORDER: Color = HAIRLINE;

/// Focused border - still gray; the accent never outlines a box
pub const BORDER_FOCUS: Color = Color::Rgb(82, 82, 82); // #525252

/// Dim border - very subtle border
pub const BORDER_DIM: Color = Color::Rgb(38, 38, 38); // #262626

// ============================================================
// LEGACY ALIASES - Backward compatibility
// ============================================================

/// Alias for CYAN_PRIMARY (legacy: PINK)
pub const PINK: Color = CYAN_PRIMARY;

/// Alias for TEAL (legacy: PURPLE)
pub const PURPLE: Color = TEAL;

/// Alias for DIFF_ADD (legacy: GREEN) — green is diff additions only
pub const GREEN: Color = DIFF_ADD;

/// Alias for WARNING (legacy: ORANGE)
pub const ORANGE: Color = WARNING;

/// Alias for INFO (legacy: BLUE)
pub const BLUE: Color = INFO;

/// Alias for ERROR (legacy: RED)
pub const RED: Color = ERROR;

/// Alias for HIGHLIGHT (legacy: YELLOW)
pub const YELLOW: Color = HIGHLIGHT;

/// Alias for BORDER_FOCUS (legacy: BORDER_HIGHLIGHT)
pub const BORDER_HIGHLIGHT: Color = BORDER_FOCUS;

// ============================================================
// THEME COLORS STRUCT - For future theme switching
// ============================================================

/// Theme color configuration for supporting multiple themes
pub struct ThemeColors {
    /// Primary accent color
    pub primary: Color,
    /// Secondary accent color
    pub secondary: Color,
    /// Bright accent for highlights
    pub accent: Color,
    /// Main background color
    pub background: Color,
    /// Surface colors (0=darkest, 3=lightest)
    pub surface: [Color; 4],
    /// Primary text color
    pub text: Color,
    /// Dimmed text color
    pub text_dim: Color,
    /// Muted text color
    pub text_muted: Color,
    /// Success color
    pub success: Color,
    /// Warning color
    pub warning: Color,
    /// Error color
    pub error: Color,
    /// Info color
    pub info: Color,
    /// Normal border color
    pub border: Color,
    /// Focused border color
    pub border_focus: Color,
}

impl ThemeColors {
    /// Gray-chrome theme - the default Cortex theme (legacy fn name)
    pub fn ocean_cyan() -> Self {
        Self {
            primary: ACCENT,
            secondary: SKY_BLUE,
            accent: ELECTRIC_BLUE,
            background: VOID,
            surface: [SURFACE_0, SURFACE_1, SURFACE_2, SURFACE_3],
            text: TEXT,
            text_dim: TEXT_DIM,
            text_muted: TEXT_MUTED,
            success: SUCCESS,
            warning: WARNING,
            error: ERROR,
            info: INFO,
            border: BORDER,
            border_focus: BORDER_FOCUS,
        }
    }

    /// Dark theme (default) - gray chrome, violet selection on dark background
    pub fn dark() -> Self {
        Self::ocean_cyan()
    }

    /// Light theme - darker violet selection, gray chrome on a light background
    pub fn light() -> Self {
        Self {
            primary: Color::Rgb(124, 58, 237),
            secondary: Color::Rgb(82, 82, 91),
            accent: Color::Rgb(39, 39, 42),
            background: Color::Rgb(255, 255, 255),
            surface: [
                Color::Rgb(245, 245, 245),
                Color::Rgb(235, 235, 235),
                Color::Rgb(225, 225, 225),
                Color::Rgb(215, 215, 215),
            ],
            text: Color::Rgb(30, 30, 30),
            text_dim: Color::Rgb(100, 100, 100),
            text_muted: Color::Rgb(150, 150, 150),
            success: Color::Rgb(22, 163, 74),
            warning: Color::Rgb(200, 150, 0),
            error: Color::Rgb(200, 50, 50),
            info: Color::Rgb(100, 100, 100),
            border: Color::Rgb(200, 200, 200),
            border_focus: Color::Rgb(160, 160, 160),
        }
    }

    /// Ocean dark theme - deep blue/cyan aesthetic
    pub fn ocean_dark() -> Self {
        Self {
            primary: Color::Rgb(0, 200, 200),
            secondary: Color::Rgb(100, 200, 220),
            accent: Color::Rgb(0, 180, 180),
            background: Color::Rgb(10, 25, 47),
            surface: [
                Color::Rgb(15, 35, 60),
                Color::Rgb(25, 50, 80),
                Color::Rgb(35, 65, 100),
                Color::Rgb(45, 80, 120),
            ],
            text: Color::Rgb(230, 240, 250),
            text_dim: Color::Rgb(140, 170, 200),
            text_muted: Color::Rgb(80, 110, 140),
            success: Color::Rgb(0, 220, 180),
            warning: Color::Rgb(255, 200, 100),
            error: Color::Rgb(255, 100, 100),
            info: Color::Rgb(100, 180, 255),
            border: Color::Rgb(40, 80, 120),
            border_focus: Color::Rgb(0, 200, 200),
        }
    }

    /// Monokai theme - classic code editor colors
    pub fn monokai() -> Self {
        Self {
            primary: Color::Rgb(166, 226, 46),
            secondary: Color::Rgb(102, 217, 239),
            accent: Color::Rgb(249, 38, 114),
            background: Color::Rgb(39, 40, 34),
            surface: [
                Color::Rgb(45, 46, 40),
                Color::Rgb(55, 56, 50),
                Color::Rgb(65, 66, 60),
                Color::Rgb(75, 76, 70),
            ],
            text: Color::Rgb(248, 248, 242),
            text_dim: Color::Rgb(180, 180, 170),
            text_muted: Color::Rgb(117, 113, 94),
            success: Color::Rgb(166, 226, 46),
            warning: Color::Rgb(230, 219, 116),
            error: Color::Rgb(249, 38, 114),
            info: Color::Rgb(102, 217, 239),
            border: Color::Rgb(70, 71, 65),
            border_focus: Color::Rgb(166, 226, 46),
        }
    }

    /// Get a theme by name
    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "light" => Self::light(),
            "ocean_dark" | "ocean" => Self::ocean_dark(),
            "monokai" => Self::monokai(),
            "dark" | _ => Self::dark(),
        }
    }

    /// Get all available theme names
    pub fn available_themes() -> &'static [&'static str] {
        &["dark", "light", "ocean_dark", "monokai"]
    }
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self::ocean_cyan()
    }
}

// ============================================================
// CORTEX STYLE HELPER
// ============================================================

/// Helper struct providing pre-configured styles for common UI elements.
///
/// All methods return fresh `Style` instances - no internal state is maintained.
pub struct CortexStyle;

impl CortexStyle {
    /// Default style: primary text on the host terminal background
    #[inline]
    pub fn default() -> Style {
        Style::default().fg(TEXT)
    }

    /// Header style: bright white bold text for titles and headers
    #[inline]
    pub fn header() -> Style {
        Style::default()
            .fg(TEXT_BRIGHT)
            .add_modifier(Modifier::BOLD)
    }

    /// Selected item style: violet text on the dark gray bar — never inverted
    #[inline]
    pub fn selected() -> Style {
        Style::default().fg(ACCENT).bg(SELECTION_BG)
    }

    /// Error style: lock red text for error messages
    #[inline]
    pub fn error() -> Style {
        Style::default().fg(ERROR)
    }

    /// Success style: green, for `✓` checks
    #[inline]
    pub fn success() -> Style {
        Style::default().fg(SUCCESS)
    }

    /// Thinking status style: the muted gold
    #[inline]
    pub fn thinking() -> Style {
        Style::default().fg(THINKING)
    }

    /// Hairline style: the thin gray rule framing the prompt
    #[inline]
    pub fn hairline() -> Style {
        Style::default().fg(HAIRLINE)
    }

    /// Warning style: golden text for warnings
    #[inline]
    pub fn warning() -> Style {
        Style::default().fg(WARNING)
    }

    /// Info style: mid gray text for informational messages
    #[inline]
    pub fn info() -> Style {
        Style::default().fg(INFO)
    }

    /// Dimmed style: secondary text color
    #[inline]
    pub fn dimmed() -> Style {
        Style::default().fg(TEXT_DIM)
    }

    /// Muted style: very dim text for background elements
    #[inline]
    pub fn muted() -> Style {
        Style::default().fg(TEXT_MUTED)
    }

    /// Highlight style: near-white bold text
    #[inline]
    pub fn highlight() -> Style {
        Style::default().fg(HIGHLIGHT).add_modifier(Modifier::BOLD)
    }

    /// User message style: white copy on the user-turn bar
    #[inline]
    pub fn user_message() -> Style {
        Style::default().fg(TEXT).bg(USER_TURN_BG)
    }

    /// Assistant message style: light gray text
    #[inline]
    pub fn assistant_message() -> Style {
        Style::default().fg(SKY_BLUE)
    }

    /// System message style: muted italic text for informational system messages
    #[inline]
    pub fn system_message() -> Style {
        Style::default()
            .fg(TEXT_MUTED)
            .add_modifier(Modifier::ITALIC)
    }

    /// Error message style: red italic text for backend error messages
    #[inline]
    pub fn error_message() -> Style {
        Style::default().fg(ERROR).add_modifier(Modifier::ITALIC)
    }

    /// Code style: near-white text on a slightly lighter surface
    #[inline]
    pub fn code() -> Style {
        Style::default().fg(ELECTRIC_BLUE).bg(SURFACE_1)
    }

    /// Border style: hairline gray
    #[inline]
    pub fn border() -> Style {
        Style::default().fg(BORDER)
    }

    /// Focused border style: a lighter gray — violet never outlines a box
    #[inline]
    pub fn border_focused() -> Style {
        Style::default().fg(BORDER_FOCUS)
    }

    /// Brain pulse style: interpolates CYAN_PRIMARY -> SKY_BLUE -> ELECTRIC_BLUE based on intensity.
    ///
    /// # Arguments
    /// * `intensity` - Value from 0.0 to 1.0 representing pulse position
    ///   - 0.0 = CYAN_PRIMARY (#00FFFF)
    ///   - 0.5 = SKY_BLUE (#87CEEB)
    ///   - 1.0 = ELECTRIC_BLUE (#7DF9FF)
    ///
    /// # Example
    /// ```
    /// use cortex_engine::style::CortexStyle;
    ///
    /// let pulse_progress = 0.5; // Middle of animation
    /// let style = CortexStyle::brain_pulse(pulse_progress);
    /// ```
    pub fn brain_pulse(intensity: f32) -> Style {
        let color = interpolate_brain_pulse(intensity);
        Style::default().fg(color)
    }

    /// Brain cyan style: fixed cyan color with brightness variation.
    ///
    /// Used for character-based brain animation where color is fixed
    /// but brightness varies based on block character type.
    ///
    /// # Arguments
    /// * `brightness` - Value from 0.0 to 1.0 representing brightness
    ///   - 1.0 = Full brightness (CYAN_PRIMARY)
    ///   - 0.5 = Medium brightness
    ///   - 0.0 = Dark (nearly black)
    ///
    /// # Example
    /// ```
    /// use cortex_engine::style::CortexStyle;
    ///
    /// let style_full = CortexStyle::brain_cyan(1.0);   // Bright cyan
    /// let style_dim = CortexStyle::brain_cyan(0.6);    // Dimmed cyan
    /// ```
    pub fn brain_cyan(brightness: f32) -> Style {
        let b = brightness.clamp(0.0, 1.0);
        // CYAN_PRIMARY is RGB(0, 255, 255)
        // Scale the brightness while keeping the violet hue
        let r = (0.0 * b) as u8;
        let g = (255.0 * b) as u8;
        let bl = (255.0 * b) as u8;
        Style::default().fg(Color::Rgb(r, g, bl))
    }
}

/// Interpolates between CYAN_PRIMARY -> SKY_BLUE -> ELECTRIC_BLUE based on intensity (0.0 to 1.0).
///
/// Uses linear interpolation in RGB space for smooth color transitions.
fn interpolate_brain_pulse(intensity: f32) -> Color {
    // Clamp intensity to valid range
    let t = intensity.clamp(0.0, 1.0);

    // Extract RGB components from our constant colors
    // CYAN_PRIMARY:   RGB(0, 255, 255)     #00FFFF
    // SKY_BLUE:       RGB(135, 206, 235)   #87CEEB
    // ELECTRIC_BLUE:  RGB(125, 249, 255)   #7DF9FF

    let (r, g, b) = if t < 0.5 {
        // First half: CYAN_PRIMARY -> SKY_BLUE
        let local_t = t * 2.0; // Normalize to 0.0-1.0 for this segment
        (
            lerp(0.0, 135.0, local_t),
            lerp(255.0, 206.0, local_t),
            lerp(255.0, 235.0, local_t),
        )
    } else {
        // Second half: SKY_BLUE -> ELECTRIC_BLUE
        let local_t = (t - 0.5) * 2.0; // Normalize to 0.0-1.0 for this segment
        (
            lerp(135.0, 125.0, local_t),
            lerp(206.0, 249.0, local_t),
            lerp(235.0, 255.0, local_t),
        )
    };

    Color::Rgb(r as u8, g as u8, b as u8)
}

/// Linear interpolation between two values.
#[inline]
fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brain_pulse_at_boundaries() {
        // At 0.0, should be CYAN_PRIMARY
        let color = interpolate_brain_pulse(0.0);
        assert_eq!(color, Color::Rgb(0, 255, 255));

        // At 1.0, should be ELECTRIC_BLUE
        let color = interpolate_brain_pulse(1.0);
        assert_eq!(color, Color::Rgb(125, 249, 255));

        // At 0.5, should be SKY_BLUE
        let color = interpolate_brain_pulse(0.5);
        assert_eq!(color, Color::Rgb(135, 206, 235));
    }

    #[test]
    fn test_brain_pulse_clamping() {
        // Values outside 0-1 should be clamped
        let color_neg = interpolate_brain_pulse(-0.5);
        let color_zero = interpolate_brain_pulse(0.0);
        assert_eq!(color_neg, color_zero);

        let color_over = interpolate_brain_pulse(1.5);
        let color_one = interpolate_brain_pulse(1.0);
        assert_eq!(color_over, color_one);
    }

    #[test]
    fn test_lerp() {
        assert_eq!(lerp(0.0, 100.0, 0.0), 0.0);
        assert_eq!(lerp(0.0, 100.0, 0.5), 50.0);
        assert_eq!(lerp(0.0, 100.0, 1.0), 100.0);
    }

    #[test]
    fn test_style_helpers() {
        // Just verify these don't panic and return valid styles
        let _ = CortexStyle::default();
        let _ = CortexStyle::header();
        let _ = CortexStyle::selected();
        let _ = CortexStyle::error();
        let _ = CortexStyle::success();
        let _ = CortexStyle::warning();
        let _ = CortexStyle::info();
        let _ = CortexStyle::dimmed();
        let _ = CortexStyle::muted();
        let _ = CortexStyle::highlight();
        let _ = CortexStyle::user_message();
        let _ = CortexStyle::assistant_message();
        let _ = CortexStyle::system_message();
        let _ = CortexStyle::error_message();
        let _ = CortexStyle::code();
        let _ = CortexStyle::border();
        let _ = CortexStyle::border_focused();
        let _ = CortexStyle::brain_pulse(0.5);
        let _ = CortexStyle::brain_cyan(0.8);
    }

    #[test]
    fn test_brain_cyan_brightness() {
        // Full brightness should be violet
        let style_full = CortexStyle::brain_cyan(1.0);
        assert_eq!(style_full.fg, Some(Color::Rgb(0, 255, 255)));

        // Zero brightness should be black
        let style_zero = CortexStyle::brain_cyan(0.0);
        assert_eq!(style_zero.fg, Some(Color::Rgb(0, 0, 0)));

        // Half brightness
        let style_half = CortexStyle::brain_cyan(0.5);
        assert_eq!(style_half.fg, Some(Color::Rgb(0, 127, 127)));
    }

    #[test]
    fn test_theme_colors() {
        let theme = ThemeColors::ocean_cyan();
        assert_eq!(theme.primary, ACCENT);
        assert_eq!(theme.secondary, SKY_BLUE);
        assert_eq!(theme.accent, ELECTRIC_BLUE);
        assert_eq!(theme.background, VOID);
        assert_eq!(theme.surface[0], SURFACE_0);
        assert_eq!(theme.surface[1], SURFACE_1);
        assert_eq!(theme.surface[2], SURFACE_2);
        assert_eq!(theme.surface[3], SURFACE_3);
        assert_eq!(theme.text, TEXT);
        assert_eq!(theme.text_dim, TEXT_DIM);
        assert_eq!(theme.text_muted, TEXT_MUTED);
        assert_eq!(theme.success, SUCCESS);
        assert_eq!(theme.warning, WARNING);
        assert_eq!(theme.error, ERROR);
        assert_eq!(theme.info, INFO);
        assert_eq!(theme.border, BORDER);
        assert_eq!(theme.border_focus, BORDER_FOCUS);

        // Test default impl
        let default_theme = ThemeColors::default();
        assert_eq!(default_theme.primary, theme.primary);
    }

    #[test]
    fn test_legacy_aliases() {
        // Verify legacy aliases point to correct new colors
        assert_eq!(PINK, CYAN_PRIMARY);
        assert_eq!(PURPLE, TEAL);
        assert_eq!(GREEN, DIFF_ADD);
        assert_eq!(ORANGE, WARNING);
        assert_eq!(BLUE, INFO);
        assert_eq!(RED, ERROR);
        assert_eq!(YELLOW, HIGHLIGHT);
        assert_eq!(BORDER_HIGHLIGHT, BORDER_FOCUS);
    }

    #[test]
    fn gray_chrome_palette_is_locked() {
        // One accent: the Cortex violet, for the focused selection only.
        assert_eq!(ACCENT, Color::Rgb(0xA7, 0x8B, 0xFA));
        assert_eq!(CortexStyle::selected().fg, Some(ACCENT));
        assert_eq!(CortexStyle::selected().bg, Some(SELECTION_BG));
        // Green covers `✓` and `+diff` — the same green.
        assert_eq!(SUCCESS, DIFF_ADD);
        assert_eq!(SUCCESS, Color::Rgb(0x4A, 0xDE, 0x80));
        // Structure is gray: the host background shows through, panels are
        // charcoal, hairlines and bars are neutral grays, secondary copy is
        // the dim gray.
        assert_eq!(VOID, Color::Reset);
        assert_eq!(PANEL_BG, Color::Rgb(0x14, 0x14, 0x14));
        assert_eq!(TEXT_DIM, Color::Rgb(0x6B, 0x72, 0x80));
        for gray in [HAIRLINE, USER_TURN_BG, BORDER_FOCUS] {
            let Color::Rgb(r, g, b) = gray else {
                panic!("{gray:?} must be an RGB gray");
            };
            assert!(r == g && g == b, "{gray:?} is not neutral");
        }
        assert_eq!(SELECTION_BG, Color::Rgb(0x22, 0x1A, 0x38));
        // The violet lives in `ACCENT` plus the selection bar. Mint is banned.
        for color in [
            SUCCESS,
            BORDER_FOCUS,
            INFO,
            HIGHLIGHT,
            SKY_BLUE,
            ELECTRIC_BLUE,
            DEEP_CYAN,
            TEAL,
        ] {
            assert_ne!(color, ACCENT, "violet leaked off the accent");
            assert_ne!(color, Color::Rgb(0x00, 0xF5, 0xD4), "mint leaked");
            assert_ne!(color, Color::Rgb(0x7D, 0xD3, 0xFC), "cyan leaked");
        }
        assert_ne!(
            ACCENT,
            Color::Rgb(0x7D, 0xD3, 0xFC),
            "the accent is not cyan"
        );
        // Thinking is the only gold.
        assert_eq!(CortexStyle::thinking().fg, Some(THINKING));
        assert_ne!(THINKING, WARNING);
    }

    #[test]
    fn test_theme_variants() {
        // Test dark theme (should be same as ocean_cyan)
        let dark = ThemeColors::dark();
        let ocean = ThemeColors::ocean_cyan();
        assert_eq!(dark.primary, ocean.primary);
        assert_eq!(dark.background, ocean.background);

        // Test light theme has light background
        let light = ThemeColors::light();
        assert_eq!(light.background, Color::Rgb(255, 255, 255));
        assert_eq!(light.text, Color::Rgb(30, 30, 30));

        // Test ocean_dark theme
        let ocean_dark = ThemeColors::ocean_dark();
        assert_eq!(ocean_dark.background, Color::Rgb(10, 25, 47));
        assert_eq!(ocean_dark.primary, Color::Rgb(0, 200, 200));

        // Test monokai theme
        let monokai = ThemeColors::monokai();
        assert_eq!(monokai.background, Color::Rgb(39, 40, 34));
        assert_eq!(monokai.primary, Color::Rgb(166, 226, 46));
    }

    #[test]
    fn test_theme_from_name() {
        // Test known theme names
        let dark = ThemeColors::from_name("dark");
        assert_eq!(dark.primary, ThemeColors::dark().primary);

        let light = ThemeColors::from_name("light");
        assert_eq!(light.background, Color::Rgb(255, 255, 255));

        let ocean_dark = ThemeColors::from_name("ocean_dark");
        assert_eq!(ocean_dark.background, Color::Rgb(10, 25, 47));

        // Test "ocean" alias for ocean_dark
        let ocean = ThemeColors::from_name("ocean");
        assert_eq!(ocean.background, Color::Rgb(10, 25, 47));

        let monokai = ThemeColors::from_name("monokai");
        assert_eq!(monokai.primary, Color::Rgb(166, 226, 46));

        // Test case insensitivity
        let light_upper = ThemeColors::from_name("LIGHT");
        assert_eq!(light_upper.background, Color::Rgb(255, 255, 255));

        // Test unknown theme falls back to dark
        let unknown = ThemeColors::from_name("nonexistent");
        assert_eq!(unknown.primary, ThemeColors::dark().primary);
    }

    #[test]
    fn test_available_themes() {
        let themes = ThemeColors::available_themes();
        assert_eq!(themes.len(), 4);
        assert!(themes.contains(&"dark"));
        assert!(themes.contains(&"light"));
        assert!(themes.contains(&"ocean_dark"));
        assert!(themes.contains(&"monokai"));
    }
}
