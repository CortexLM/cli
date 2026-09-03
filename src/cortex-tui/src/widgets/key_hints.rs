//! Contextual Key Hints Widget
//!
//! A single-line widget that displays context-sensitive keyboard shortcuts
//! at the bottom of the screen. The hints change based on the current
//! application context (idle, task running, modal active, etc.).

use crate::permissions::PermissionMode;
use crate::ui::colors::AdaptiveColors;
use crate::ui::text_utils::{
    AdaptiveHint, HintDisplayMode, MIN_TERMINAL_WIDTH, calculate_hint_display_mode,
    first_fitting_line,
};
use ratatui::prelude::*;
use ratatui::widgets::Widget;

// ============================================================
// HINT CONTEXT
// ============================================================

/// The current context that determines which key hints to display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HintContext {
    /// Normal idle state - show general hints
    #[default]
    Idle,
    /// A task is running - show interrupt hints
    TaskRunning,
    /// A card/modal is active - show card-specific hints
    CardActive,
    /// Approval dialog is shown
    Approval,
    /// Selection list is focused
    Selection,
    /// Viewing a subagent's conversation
    SubagentView,
}

impl HintContext {
    /// Returns the default hints for this context.
    fn default_hints(&self) -> Vec<(&'static str, &'static str)> {
        match self {
            HintContext::Idle => vec![],
            HintContext::TaskRunning => vec![("Esc", "interrupt"), ("Ctrl+C", "force quit")],
            HintContext::CardActive => {
                vec![("Up/Dn", "navigate"), ("Enter", "select"), ("Esc", "close")]
            }
            HintContext::Approval => vec![
                ("y", "approve"),
                ("n", "reject"),
                ("Esc", "cancel"),
                ("Ctrl+A", "full diff"),
            ],
            HintContext::Selection => vec![
                ("Up/Dn", "navigate"),
                ("Enter", "select"),
                ("Esc", "close"),
                ("/", "filter"),
            ],
            HintContext::SubagentView => vec![("Esc", "back to main"), ("↑/↓", "scroll")],
        }
    }
}

// ============================================================
// KEY HINTS WIDGET
// ============================================================

/// A contextual key hints widget that displays keyboard shortcuts.
///
/// The widget shows a single line of hints at the bottom of the screen,
/// changing based on the current application context.
///
/// # Example
///
/// ```ignore
/// use cortex_tui::widgets::{KeyHints, HintContext};
///
/// // Use default hints for idle context
/// let hints = KeyHints::new(HintContext::Idle);
/// frame.render_widget(hints, area);
///
/// // Use custom hints
/// let mut hints = KeyHints::new(HintContext::Idle);
/// hints.set_custom_hints(vec![("Enter", "confirm"), ("Esc", "cancel")]);
/// frame.render_widget(hints, area);
/// ```
#[derive(Debug, Clone)]
pub struct KeyHints {
    /// The current context determining default hints
    context: HintContext,
    /// Optional custom hints that override defaults
    custom_hints: Option<Vec<(&'static str, &'static str)>>,
    /// Color palette for rendering
    colors: AdaptiveColors,
    /// Permission mode to display
    permission_mode: Option<PermissionMode>,
    /// Model name to display on the right
    model_name: Option<String>,
    /// Thinking budget level (e.g., "medium", "high")
    thinking_budget: Option<String>,
    /// Session footer: `Cortex Mini 1 · Agent · 92% context` on the left,
    /// one shortcut hint on the right.
    agent_mode: Option<String>,
    context_percent: Option<u8>,
    /// `(full, short)` override for the right-hand footer hint.
    footer_hint: Option<(String, String)>,
}

/// Right-hand footer hint while idle, with its narrower fallbacks.
pub const FOOTER_HINT_IDLE: &str = "shift+tab to cycle modes";
const FOOTER_HINT_IDLE_SHORT: &str = "shift+tab modes";

impl KeyHints {
    /// Creates a new key hints widget with the given context.
    pub fn new(context: HintContext) -> Self {
        Self {
            context,
            custom_hints: None,
            colors: AdaptiveColors::default(),
            permission_mode: None,
            model_name: None,
            thinking_budget: None,
            agent_mode: None,
            context_percent: None,
            footer_hint: None,
        }
    }

    /// Sets the hint context.
    pub fn set_context(&mut self, context: HintContext) {
        self.context = context;
    }

    /// Sets custom hints that override the default hints for the current context.
    pub fn set_custom_hints(&mut self, hints: Vec<(&'static str, &'static str)>) {
        self.custom_hints = Some(hints);
    }

    /// Clears custom hints, reverting to default hints for the current context.
    pub fn clear_custom_hints(&mut self) {
        self.custom_hints = None;
    }

    /// Sets the color palette for rendering.
    pub fn with_colors(mut self, colors: AdaptiveColors) -> Self {
        self.colors = colors;
        self
    }

    /// Sets the permission mode to display on the left
    pub fn with_permission_mode(mut self, mode: PermissionMode) -> Self {
        self.permission_mode = Some(mode);
        self
    }

    /// Sets the model to display on the right — always the English product
    /// name, never the served slug.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model_name = Some(crate::ui::text_utils::model_display_name(&model.into()));
        self
    }

    /// Sets the thinking budget level to display
    pub fn with_thinking_budget(mut self, budget: impl Into<String>) -> Self {
        self.thinking_budget = Some(budget.into());
        self
    }

    /// Session footer: `model · mode · context` on the left, a shortcut hint
    /// on the right — gray throughout.
    pub fn with_session_footer(
        mut self,
        agent_mode: impl Into<String>,
        context_percent: u8,
    ) -> Self {
        self.agent_mode = Some(agent_mode.into());
        self.context_percent = Some(context_percent);
        self
    }

    /// Override the right-hand shortcut hint of the session footer (defaults
    /// to `shift+tab to cycle modes`); `short` is the narrow fallback. Pass
    /// two empty strings to leave the right side blank — a panel that shows
    /// its own hints does that.
    pub fn with_footer_hint(mut self, hint: impl Into<String>, short: impl Into<String>) -> Self {
        self.footer_hint = Some((hint.into(), short.into()));
        self
    }

    /// Returns the hints to display (custom if set, otherwise defaults).
    fn get_hints(&self) -> Vec<(&'static str, &'static str)> {
        self.custom_hints
            .clone()
            .unwrap_or_else(|| self.context.default_hints())
    }

    /// Calculates the total width needed to render all hints.
    fn calculate_width(&self) -> usize {
        let hints = self.get_hints();
        if hints.is_empty() {
            return 0;
        }

        let mut width = 0;
        for (i, (key, desc)) in hints.iter().enumerate() {
            if i > 0 {
                width += 3; // " · " separator
            }
            width += key.chars().count();
            width += 1; // space between key and description
            width += desc.chars().count();
        }
        width
    }

    /// Builds the spans for rendering with adaptive display mode.
    fn build_spans_with_mode(&self, mode: HintDisplayMode) -> Vec<Span<'static>> {
        let hints = self.get_hints();
        if hints.is_empty() || mode == HintDisplayMode::Minimal {
            return Vec::new();
        }

        let mut spans = Vec::new();
        let separator_style = Style::default().fg(self.colors.text_muted);
        // Hint rows are uniformly dim in the gray chrome — violet stays on the
        // focused selection only.
        let key_style = Style::default().fg(self.colors.text_dim);
        let desc_style = Style::default().fg(self.colors.text_dim);

        for (i, (key, desc)) in hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" · ", separator_style));
            }
            spans.push(Span::styled(*key, key_style));

            // Add description based on mode
            match mode {
                HintDisplayMode::Full => {
                    spans.push(Span::styled(" ", Style::default()));
                    spans.push(Span::styled(*desc, desc_style));
                }
                HintDisplayMode::Abbreviated => {
                    let abbrev = abbreviate_hint_desc(desc);
                    spans.push(Span::styled(" ", Style::default()));
                    spans.push(Span::styled(abbrev, desc_style));
                }
                HintDisplayMode::KeysOnly | HintDisplayMode::Minimal => {
                    // No description for keys-only mode
                }
            }
        }

        spans
    }

    /// Builds the spans for rendering (full mode for backward compatibility).
    fn _build_spans(&self) -> Vec<Span<'static>> {
        self.build_spans_with_mode(HintDisplayMode::Full)
    }
}

/// Abbreviate hint descriptions for narrow displays.
fn abbreviate_hint_desc(description: &str) -> &'static str {
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
        "full diff" => "diff",
        _ => "...",
    }
}

impl Widget for KeyHints {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        if self.agent_mode.is_some() {
            render_session_footer(&self, area, buf);
            return;
        }

        // Handle narrow terminals gracefully
        if area.width < MIN_TERMINAL_WIDTH {
            // Just render "..." centered
            let ellipsis = "...";
            let x = area.x + (area.width.saturating_sub(3)) / 2;
            buf.set_string(
                x,
                area.y,
                ellipsis,
                Style::default().fg(self.colors.text_muted),
            );
            return;
        }

        // Build permission mode display on the left: "» yolo (allow all) mode (shift+tab)"
        let mut left_spans: Vec<Span<'static>> = Vec::new();
        let badge_width = if let Some(mode) = &self.permission_mode {
            // Add chevron
            left_spans.push(Span::styled(
                "» ",
                Style::default().fg(self.colors.text_muted),
            ));
            // Add mode name in color
            left_spans.push(Span::styled(
                mode.display_name().to_string(),
                Style::default().fg(mode.display_color()),
            ));
            // Add description
            left_spans.push(Span::styled(
                format!(" ({}) mode ", mode.description()),
                Style::default().fg(self.colors.text_muted),
            ));
            // Add keybinding hint
            left_spans.push(Span::styled(
                "(shift+tab)",
                Style::default().fg(self.colors.text_dim),
            ));

            // Calculate total width
            let total: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();
            total + 1 // +1 for spacing
        } else {
            0
        };

        // Render permission mode on the left if set
        if !left_spans.is_empty() {
            let left_line = Line::from(left_spans);
            buf.set_line(area.x, area.y, &left_line, badge_width as u16);
        }

        // Build and render model info on the right
        let mut right_spans: Vec<Span<'static>> = Vec::new();
        if let Some(ref model) = self.model_name {
            // Extract short model name (after last /)
            let short_name = model.rsplit('/').next().unwrap_or(model);
            right_spans.push(Span::styled(
                short_name.to_string(),
                Style::default().fg(self.colors.text_dim),
            ));
        }
        if let Some(ref budget) = self.thinking_budget {
            if !right_spans.is_empty() {
                right_spans.push(Span::styled(
                    " · ",
                    Style::default().fg(self.colors.text_muted),
                ));
            }
            right_spans.push(Span::styled(
                budget.clone(),
                Style::default().fg(self.colors.thinking),
            ));
        }

        // Calculate right side width
        let right_width: usize = right_spans.iter().map(|s| s.content.chars().count()).sum();

        // Render right side if we have content
        if !right_spans.is_empty() && right_width < area.width as usize {
            let right_x = area.right().saturating_sub(right_width as u16 + 1);
            let right_line = Line::from(right_spans);
            buf.set_line(right_x, area.y, &right_line, right_width as u16 + 1);
        }

        // Calculate available width for hints
        let available_width = area.width as usize;
        let remaining_width = available_width
            .saturating_sub(badge_width)
            .saturating_sub(right_width + 2); // +2 for spacing

        // Get hints and calculate adaptive display mode
        let hints = self.get_hints();
        if hints.is_empty() {
            return;
        }

        let adaptive: Vec<AdaptiveHint> =
            hints.iter().map(|(k, d)| AdaptiveHint::new(k, d)).collect();

        let mode = calculate_hint_display_mode(&adaptive, remaining_width, 3);

        // Handle minimal mode (too narrow for any hints)
        if mode == HintDisplayMode::Minimal {
            if remaining_width >= 3 {
                let x = area.x + badge_width as u16 + ((remaining_width - 3) / 2) as u16;
                buf.set_string(
                    x,
                    area.y,
                    "...",
                    Style::default().fg(self.colors.text_muted),
                );
            }
            return;
        }

        // Build spans with appropriate mode
        let spans = self.build_spans_with_mode(mode);
        if spans.is_empty() {
            return;
        }

        let line = Line::from(spans);

        // Calculate actual width of rendered content
        let total_width = self.calculate_width();
        let rendered_width = match mode {
            HintDisplayMode::Full => total_width,
            HintDisplayMode::Abbreviated | HintDisplayMode::KeysOnly => {
                remaining_width.min(total_width)
            }
            HintDisplayMode::Minimal => 3,
        };

        // Center the hints in the remaining area
        let x = if rendered_width < remaining_width {
            area.x + badge_width as u16 + ((remaining_width - rendered_width) / 2) as u16
        } else {
            area.x + badge_width as u16
        };

        let render_width = remaining_width.min(rendered_width);
        buf.set_line(x, area.y, &line, render_width as u16);
    }
}

/// Pick the widest `(left, right)` pair that fits `width` with a one-column
/// gap. Both sides are tried in order; a pairing that keeps some hint on the
/// right wins over a wider left side alone, and the left side (the model)
/// always survives on its own when nothing else fits.
pub fn fit_footer_pair<'a>(
    left_opts: &[&'a str],
    right_opts: &[&'a str],
    width: usize,
) -> (&'a str, &'a str) {
    let fits = |left: &str, right: &str| {
        let need = if left.is_empty() {
            right.chars().count()
        } else if right.is_empty() {
            left.chars().count()
        } else {
            left.chars().count() + 1 + right.chars().count()
        };
        need <= width
    };
    for left in left_opts {
        for right in right_opts.iter().filter(|r| !r.is_empty()) {
            if fits(left, right) {
                return (left, right);
            }
        }
    }
    for left in left_opts {
        if fits(left, "") {
            return (left, "");
        }
    }
    ("", "")
}

/// Session footer: the model on the left, one shortcut hint on the right,
/// both dim gray. The left side degrades `model · mode · N% context` →
/// `model · mode` → `model`; the right side degrades to its short form and
/// then disappears. The model always wins over the hint.
fn render_session_footer(hints: &KeyHints, area: Rect, buf: &mut Buffer) {
    let width = area.width as usize;
    let model = hints
        .model_name
        .as_deref()
        .map(|m| m.rsplit('/').next().unwrap_or(m).to_string())
        .unwrap_or_default();
    let mode = hints.agent_mode.clone().unwrap_or_else(|| "Agent".into());
    let pct = hints.context_percent.unwrap_or(100);
    let left_full = format!("{model} · {mode} · {pct}% context");
    let left_mid = format!("{model} · {mode}");
    let left_model = model.clone();
    let (hint_full, hint_short) = hints.footer_hint.clone().unwrap_or_else(|| {
        (
            FOOTER_HINT_IDLE.to_string(),
            FOOTER_HINT_IDLE_SHORT.to_string(),
        )
    });

    let left_opts = [left_full.as_str(), left_mid.as_str(), left_model.as_str()];
    let right_opts = [hint_full.as_str(), hint_short.as_str(), ""];
    let (left, right) = fit_footer_pair(&left_opts, &right_opts, width);

    let dim = Style::default().fg(hints.colors.text_dim);
    if !left.is_empty() {
        buf.set_string(area.x, area.y, first_fitting_line(left, width), dim);
    }
    if !right.is_empty() {
        let text = first_fitting_line(right, width);
        let x = area
            .right()
            .saturating_sub(text.chars().count() as u16)
            .max(area.x);
        if left.is_empty() || x > area.x + left.chars().count() as u16 {
            buf.set_string(x, area.y, &text, dim);
        }
    }
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;

    fn create_test_buffer(width: u16, height: u16) -> Buffer {
        Buffer::empty(Rect::new(0, 0, width, height))
    }

    #[test]
    fn test_hint_context_default() {
        let context: HintContext = Default::default();
        assert_eq!(context, HintContext::Idle);
    }

    #[test]
    #[ignore = "TUI behavior differs across platforms"]
    fn test_hint_context_default_hints() {
        // Idle hints
        let idle_hints = HintContext::Idle.default_hints();
        assert_eq!(idle_hints.len(), 4);
        assert!(idle_hints.iter().any(|(k, _)| *k == "/"));
        assert!(idle_hints.iter().any(|(k, _)| *k == "?"));

        // Task running hints
        let task_hints = HintContext::TaskRunning.default_hints();
        assert_eq!(task_hints.len(), 2);
        assert!(task_hints.iter().any(|(k, _)| *k == "Esc"));
        assert!(task_hints.iter().any(|(k, _)| *k == "Ctrl+C"));

        // Card active hints
        let card_hints = HintContext::CardActive.default_hints();
        assert_eq!(card_hints.len(), 3);
        assert!(card_hints.iter().any(|(k, _)| *k == "Enter"));

        // Approval hints
        let approval_hints = HintContext::Approval.default_hints();
        assert_eq!(approval_hints.len(), 4);
        assert!(approval_hints.iter().any(|(k, _)| *k == "y"));
        assert!(approval_hints.iter().any(|(k, _)| *k == "n"));

        // Selection hints
        let selection_hints = HintContext::Selection.default_hints();
        assert_eq!(selection_hints.len(), 4);
        assert!(selection_hints.iter().any(|(k, _)| *k == "/"));
    }

    #[test]
    fn test_key_hints_new() {
        let hints = KeyHints::new(HintContext::Idle);
        assert_eq!(hints.context, HintContext::Idle);
        assert!(hints.custom_hints.is_none());
    }

    #[test]
    fn test_key_hints_set_context() {
        let mut hints = KeyHints::new(HintContext::Idle);
        hints.set_context(HintContext::TaskRunning);
        assert_eq!(hints.context, HintContext::TaskRunning);
    }

    #[test]
    #[ignore = "TUI behavior differs across platforms"]
    fn test_key_hints_custom_hints() {
        let mut hints = KeyHints::new(HintContext::Idle);

        // Set custom hints
        hints.set_custom_hints(vec![("Enter", "confirm"), ("Esc", "cancel")]);
        assert!(hints.custom_hints.is_some());
        assert_eq!(hints.get_hints().len(), 2);

        // Clear custom hints
        hints.clear_custom_hints();
        assert!(hints.custom_hints.is_none());
        assert_eq!(hints.get_hints().len(), 4); // Back to default idle hints
    }

    #[test]
    fn test_key_hints_calculate_width() {
        let hints = KeyHints::new(HintContext::TaskRunning);
        // "Esc interrupt · Ctrl+C force quit"
        // Esc=3 + space=1 + interrupt=9 + separator=3 + Ctrl+C=6 + space=1 + "force quit"=10 = 33
        let width = hints.calculate_width();
        assert!(width > 0);
    }

    #[test]
    #[ignore = "TUI behavior differs across platforms"]
    fn test_key_hints_render_idle() {
        let hints = KeyHints::new(HintContext::Idle);

        let mut buf = create_test_buffer(80, 1);
        let area = Rect::new(0, 0, 80, 1);
        hints.render(area, &mut buf);

        // Check that content was rendered
        let content: String = (0..80)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(content.contains("commands"));
        assert!(content.contains("help"));
    }

    #[test]
    fn test_key_hints_render_task_running() {
        let hints = KeyHints::new(HintContext::TaskRunning);

        let mut buf = create_test_buffer(80, 1);
        let area = Rect::new(0, 0, 80, 1);
        hints.render(area, &mut buf);

        let content: String = (0..80)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(content.contains("interrupt"));
    }

    #[test]
    fn test_key_hints_render_approval() {
        let hints = KeyHints::new(HintContext::Approval);

        let mut buf = create_test_buffer(80, 1);
        let area = Rect::new(0, 0, 80, 1);
        hints.render(area, &mut buf);

        let content: String = (0..80)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(content.contains("approve"));
        assert!(content.contains("reject"));
    }

    #[test]
    fn test_key_hints_render_zero_area() {
        let hints = KeyHints::new(HintContext::Idle);

        let mut buf = create_test_buffer(0, 0);
        let area = Rect::new(0, 0, 0, 0);
        // Should not panic
        hints.render(area, &mut buf);
    }

    #[test]
    fn test_key_hints_render_narrow() {
        let hints = KeyHints::new(HintContext::Idle);

        let mut buf = create_test_buffer(20, 1);
        let area = Rect::new(0, 0, 20, 1);
        // Should not panic even with narrow width
        hints.render(area, &mut buf);
    }

    #[test]
    fn test_key_hints_with_colors() {
        let colors = AdaptiveColors::default_dark();
        let hints = KeyHints::new(HintContext::Idle).with_colors(colors);

        let mut buf = create_test_buffer(80, 1);
        let area = Rect::new(0, 0, 80, 1);
        hints.render(area, &mut buf);
    }

    fn row_text(buf: &Buffer, width: u16) -> String {
        (0..width)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect()
    }

    #[test]
    fn session_footer_is_model_left_hint_right() {
        let hints = KeyHints::new(HintContext::Idle)
            .with_colors(AdaptiveColors::default_dark())
            .with_model("cortex-1-mini")
            .with_session_footer("Agent", 92);
        let mut buf = create_test_buffer(120, 1);
        hints.render(Rect::new(0, 0, 120, 1), &mut buf);
        let row = row_text(&buf, 120);
        assert!(
            row.starts_with("Cortex Mini 1 · Agent · 92% context"),
            "{row}"
        );
        assert!(row.trim_end().ends_with(FOOTER_HINT_IDLE), "{row}");
        // Every footer cell is the dim gray — never the accent.
        let dim = AdaptiveColors::default_dark().text_dim;
        for x in 0..120u16 {
            if buf[(x, 0)].symbol() != " " {
                assert_eq!(buf[(x, 0)].style().fg, Some(dim), "col {x}: {row}");
            }
        }
    }

    #[test]
    fn session_footer_degrades_but_keeps_the_model() {
        let make = |w: u16| {
            let hints = KeyHints::new(HintContext::Idle)
                .with_colors(AdaptiveColors::default_dark())
                .with_model("cortex-1-mini")
                .with_session_footer("Agent", 100);
            let mut buf = create_test_buffer(w, 1);
            hints.render(Rect::new(0, 0, w, 1), &mut buf);
            row_text(&buf, w)
        };
        let narrow = make(40);
        assert!(narrow.starts_with("Cortex Mini 1"), "{narrow}");
        assert!(narrow.contains("shift+tab"), "{narrow}");
        assert!(!narrow.contains("context"), "{narrow}");
        let tiny = make(16);
        assert!(tiny.starts_with("Cortex Mini 1"), "{tiny}");
        assert!(!tiny.contains("shift"), "{tiny}");
    }

    #[test]
    fn session_footer_takes_a_custom_hint() {
        let hints = KeyHints::new(HintContext::Idle)
            .with_colors(AdaptiveColors::default_dark())
            .with_model("cortex-1-mini")
            .with_session_footer("Plan", 88)
            .with_footer_hint("↑↓ select · ↵ confirm · esc close", "esc close");
        let mut buf = create_test_buffer(120, 1);
        hints.render(Rect::new(0, 0, 120, 1), &mut buf);
        let row = row_text(&buf, 120);
        assert!(row.contains("Cortex Mini 1 · Plan · 88% context"), "{row}");
        assert!(row.trim_end().ends_with("esc close"), "{row}");
    }

    #[test]
    fn test_all_contexts_render() {
        let contexts = vec![
            HintContext::Idle,
            HintContext::TaskRunning,
            HintContext::CardActive,
            HintContext::Approval,
            HintContext::Selection,
        ];

        for context in contexts {
            let hints = KeyHints::new(context);
            let mut buf = create_test_buffer(100, 1);
            let area = Rect::new(0, 0, 100, 1);
            // Should not panic for any context
            hints.render(area, &mut buf);
        }
    }
}
