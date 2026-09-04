//! Welcome splash for an empty Cortex session.
//!
//! Two lines, no mascot, no boxes, no painted shell prompt:
//! `Welcome to Cortex, the coding agent CLI` then
//! `v{version} · / commands · @ files · ! shell · & cloud`.

use crate::borders::ROUNDED_BORDER;
use cortex_core::style::{BORDER, TEXT, TEXT_DIM};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

/// Trait for components that can generate scrollable lines.
pub trait ToLines {
    /// Generate styled lines for scrollable rendering.
    fn to_lines(&self, width: u16) -> Vec<Line<'static>>;
}

const WELCOME_LEAD: &str = "Welcome to ";
const WELCOME_PRODUCT: &str = "Cortex";
const WELCOME_TAIL: &str = ", the coding agent CLI";

/// Empty-session splash: welcome line plus version/hints. No mascot or logo.
pub struct WelcomeCard<'a> {
    user_name: Option<&'a str>,
    subtitle: Option<&'a str>,
    version: Option<&'a str>,
    tips: Vec<&'a str>,
    accent_color: Color,
    text_color: Color,
    dim_color: Color,
    border_color: Color,
}

impl<'a> WelcomeCard<'a> {
    /// Create a new welcome card.
    pub fn new() -> Self {
        Self {
            user_name: None,
            subtitle: None,
            version: None,
            tips: Vec::new(),
            accent_color: TEXT,
            text_color: TEXT,
            dim_color: TEXT_DIM,
            border_color: BORDER,
        }
    }

    /// Set the user name for the greeting.
    pub fn user_name(mut self, name: &'a str) -> Self {
        self.user_name = Some(name);
        self
    }

    /// Set the subtitle text.
    pub fn subtitle(mut self, text: &'a str) -> Self {
        self.subtitle = Some(text);
        self
    }

    /// Set the version string for the title.
    pub fn version(mut self, version: &'a str) -> Self {
        self.version = Some(version);
        self
    }

    /// Set the tips to display.
    pub fn tips(mut self, tips: &[&'a str]) -> Self {
        self.tips = tips.to_vec();
        self
    }

    /// Set the accent color.
    pub fn accent_color(mut self, color: Color) -> Self {
        self.accent_color = color;
        self
    }

    /// Set the text color.
    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = color;
        self
    }

    /// Set the dim/muted text color.
    pub fn dim_color(mut self, color: Color) -> Self {
        self.dim_color = color;
        self
    }

    /// Set the border color.
    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = color;
        self
    }

    /// Calculate the required height for this card.
    pub fn required_height(&self) -> u16 {
        2
    }
}

impl Default for WelcomeCard<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl ToLines for WelcomeCard<'_> {
    fn to_lines(&self, width: u16) -> Vec<Line<'static>> {
        let _ = (
            self.user_name,
            self.subtitle,
            self.tips.len(),
            self.accent_color,
            self.border_color,
        );
        welcome_lines(self.version, width, self.text_color, self.dim_color)
    }
}

/// `v{version} · / commands · @ files · ! shell · & cloud`, shortened to fit.
pub fn splash_hint_line(version: &str, width: usize) -> String {
    let full = format!("v{version} · / commands · @ files · ! shell · & cloud");
    if full.chars().count() <= width {
        return full;
    }
    let mid = format!("v{version} · / commands · @ files · ! shell");
    if mid.chars().count() <= width {
        return mid;
    }
    let short = format!("v{version} · / commands");
    if short.chars().count() <= width {
        return short;
    }
    let bare = format!("v{version}");
    if bare.chars().count() <= width {
        bare
    } else {
        String::new()
    }
}

fn welcome_lines(version: Option<&str>, width: u16, text: Color, dim: Color) -> Vec<Line<'static>> {
    let w = width as usize;
    let dim_st = Style::default().fg(dim);
    let bold_st = Style::default().fg(text).add_modifier(Modifier::BOLD);
    let mut lines = Vec::new();
    let title_w = WELCOME_LEAD.len() + WELCOME_PRODUCT.len() + WELCOME_TAIL.len();
    if title_w <= w {
        lines.push(Line::from(vec![
            Span::styled(WELCOME_LEAD, dim_st),
            Span::styled(WELCOME_PRODUCT, bold_st),
            Span::styled(WELCOME_TAIL, dim_st),
        ]));
    } else if WELCOME_LEAD.len() + WELCOME_PRODUCT.len() <= w {
        lines.push(Line::from(vec![
            Span::styled(WELCOME_LEAD, dim_st),
            Span::styled(WELCOME_PRODUCT, bold_st),
        ]));
    } else {
        lines.push(Line::from(Span::styled(WELCOME_PRODUCT, bold_st)));
    }
    let ver = version.unwrap_or(env!("CARGO_PKG_VERSION"));
    let hint = splash_hint_line(ver, w);
    if !hint.is_empty() {
        lines.push(Line::from(Span::styled(hint, dim_st)));
    }
    lines
}

impl Widget for WelcomeCard<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let lines = welcome_lines(self.version, area.width, self.text_color, self.dim_color);
        for (i, line) in lines.into_iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.bottom() {
                break;
            }
            buf.set_line(area.x, y, &line, area.width);
        }
    }
}

/// A card displaying key-value information pairs.
///
/// # Example
/// ```rust,ignore
/// use cortex_tui_components::welcome_card::InfoCard;
///
/// let card = InfoCard::new()
///     .add("Directory", "~/projects")
///     .add("User", "user@email.com")
///     .add("Model", "Cortex Mini 1");
/// ```
pub struct InfoCard<'a> {
    items: Vec<(&'a str, String)>,
    dim_color: Color,
    text_color: Color,
    border_color: Color,
}

impl<'a> InfoCard<'a> {
    /// Create a new info card.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            dim_color: TEXT_DIM,
            text_color: TEXT,
            border_color: BORDER,
        }
    }

    /// Add a label-value pair.
    pub fn add(mut self, label: &'a str, value: impl Into<String>) -> Self {
        self.items.push((label, value.into()));
        self
    }

    /// Set the dim color for labels.
    pub fn dim_color(mut self, color: Color) -> Self {
        self.dim_color = color;
        self
    }

    /// Set the text color for values.
    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = color;
        self
    }

    /// Set the border color.
    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = color;
        self
    }

    /// Calculate the required height for this card.
    pub fn required_height(&self) -> u16 {
        // Border (2) + items
        2 + self.items.len() as u16
    }
}

impl Default for InfoCard<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl ToLines for InfoCard<'_> {
    fn to_lines(&self, width: u16) -> Vec<Line<'static>> {
        let w = width as usize;
        let bs = Style::default().fg(self.border_color);
        let inner = w.saturating_sub(4);

        let mut lines: Vec<Line<'static>> = Vec::new();

        // Top border
        lines.push(Line::from(Span::styled(
            format!("╭{}╮", "─".repeat(w.saturating_sub(2))),
            bs,
        )));

        // Content lines
        for (label, value) in &self.items {
            if label.is_empty() && value.is_empty() {
                // Empty row
                lines.push(Line::from(vec![
                    Span::styled("│", bs),
                    Span::raw(" ".repeat(w.saturating_sub(2))),
                    Span::styled("│", bs),
                ]));
            } else {
                let lbl = format!("{}: ", label);
                let avail = inner.saturating_sub(lbl.len());
                let val = if value.len() > avail {
                    format!("{}...", &value[..avail.saturating_sub(3)])
                } else {
                    value.clone()
                };
                let fill = inner.saturating_sub(lbl.len() + val.len());

                lines.push(Line::from(vec![
                    Span::styled("│ ", bs),
                    Span::styled(lbl, Style::default().fg(self.dim_color)),
                    Span::styled(val, Style::default().fg(self.text_color)),
                    Span::raw(" ".repeat(fill)),
                    Span::styled(" │", bs),
                ]));
            }
        }

        // Bottom border
        lines.push(Line::from(Span::styled(
            format!("╰{}╯", "─".repeat(w.saturating_sub(2))),
            bs,
        )));

        lines
    }
}

impl Widget for InfoCard<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 10 {
            return;
        }

        // Render border with custom color
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ROUNDED_BORDER)
            .border_style(Style::default().fg(self.border_color));

        let inner = block.inner(area);
        block.render(area, buf);
        let content_width = inner.width.saturating_sub(2) as usize;

        for (i, (label, value)) in self.items.iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }

            let label_with_colon = format!("{}: ", label);
            let max_value_len = content_width.saturating_sub(label_with_colon.len());
            let truncated_value = if value.len() > max_value_len {
                format!("{}...", &value[..max_value_len.saturating_sub(3)])
            } else {
                value.clone()
            };

            // Render label
            buf.set_string(
                inner.x + 1,
                y,
                &label_with_colon,
                Style::default().fg(self.dim_color),
            );

            // Render value
            buf.set_string(
                inner.x + 1 + label_with_colon.len() as u16,
                y,
                &truncated_value,
                Style::default().fg(self.text_color),
            );
        }
    }
}

/// Renders two info cards side by side.
///
/// # Example
/// ```rust,ignore
/// use cortex_tui_components::welcome_card::{InfoCardPair, InfoCard};
///
/// let left = InfoCard::new().add("Dir", "~/projects").add("User", "me@email.com");
/// let right = InfoCard::new().add("Model", "Cortex Mini 1").add("Plan", "Pro");
///
/// InfoCardPair::new(left, right).render(area, buf);
/// ```
pub struct InfoCardPair<'a> {
    left: InfoCard<'a>,
    right: InfoCard<'a>,
    gap: u16,
    right_width: u16,
}

impl<'a> InfoCardPair<'a> {
    /// Create a new info card pair.
    pub fn new(left: InfoCard<'a>, right: InfoCard<'a>) -> Self {
        Self {
            left,
            right,
            gap: 2,
            right_width: 25,
        }
    }

    /// Set the gap between cards.
    pub fn gap(mut self, gap: u16) -> Self {
        self.gap = gap;
        self
    }

    /// Set the width of the right card.
    pub fn right_width(mut self, width: u16) -> Self {
        self.right_width = width;
        self
    }
}

impl ToLines for InfoCardPair<'_> {
    fn to_lines(&self, width: u16) -> Vec<Line<'static>> {
        let total_width = (width as usize).max(40); // Adaptive width

        if total_width < (self.right_width as usize + self.gap as usize + 20) {
            return self.left.to_lines(width);
        }

        let left_width = total_width.saturating_sub(self.right_width as usize + self.gap as usize);
        let left_lines = self.left.to_lines(left_width as u16);
        let right_lines = self.right.to_lines(self.right_width);
        let gap = " ".repeat(self.gap as usize);

        let mut lines: Vec<Line<'static>> = Vec::new();
        let max_len = left_lines.len().max(right_lines.len());

        for i in 0..max_len {
            let left_part = left_lines.get(i);
            let right_part = right_lines.get(i);

            let mut spans: Vec<Span<'static>> = Vec::new();

            if let Some(l) = left_part {
                spans.extend(l.spans.iter().cloned());
            } else {
                spans.push(Span::raw(" ".repeat(left_width)));
            }

            spans.push(Span::raw(gap.clone()));

            if let Some(r) = right_part {
                spans.extend(r.spans.iter().cloned());
            } else {
                spans.push(Span::raw(" ".repeat(self.right_width as usize)));
            }

            lines.push(Line::from(spans));
        }

        lines
    }
}

impl Widget for InfoCardPair<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < self.right_width + self.gap + 20 {
            // Not enough space, render left card only
            self.left.render(area, buf);
            return;
        }

        let left_width = area.width.saturating_sub(self.right_width + self.gap);

        let left_area = Rect::new(area.x, area.y, left_width, area.height);
        let right_area = Rect::new(
            area.x + left_width + self.gap,
            area.y,
            self.right_width,
            area.height,
        );

        self.left.render(left_area, buf);
        self.right.render(right_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_welcome_card_builder() {
        let card = WelcomeCard::new()
            .user_name("Test")
            .subtitle("Test subtitle")
            .version("1.0.0")
            .tips(&["Tip 1", "Tip 2"]);

        assert_eq!(card.user_name, Some("Test"));
        assert_eq!(card.subtitle, Some("Test subtitle"));
        assert_eq!(card.version, Some("1.0.0"));
        assert_eq!(card.tips.len(), 2);
    }

    #[test]
    fn test_info_card_builder() {
        let card = InfoCard::new()
            .add("Label1", "Value1")
            .add("Label2", "Value2");

        assert_eq!(card.items.len(), 2);
    }

    #[test]
    fn test_welcome_card_height() {
        let card = WelcomeCard::new().version("1.0.0");
        assert_eq!(card.required_height(), 2);
    }

    fn buffer_text(buf: &Buffer) -> String {
        let area = buf.area();
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn assert_no_mascot(text: &str) {
        for needle in ["▄█▀▀▀▀█▄", "██ ▌  ▐ ██", "█▄▄▄▄▄▄█"]
        {
            assert!(
                !text.contains(needle),
                "splash must not include mascot {needle:?}:\n{text}"
            );
        }
    }

    #[test]
    fn welcome_card_is_two_line_splash() {
        let card = WelcomeCard::new().version("0.1.7");
        let text = card
            .to_lines(80)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Welcome to "));
        assert!(text.contains("Cortex"));
        assert!(text.contains("the coding agent CLI"));
        assert!(text.contains("v0.1.7 · / commands"));
        assert!(text.contains("& cloud"));
        assert_no_mascot(&text);
        assert!(!text.contains("Welcome!"));
        assert!(!text.contains("> cortex"));
        assert!(!text.contains("~/"));
    }

    #[test]
    fn welcome_card_widget_renders_welcome_lock() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 16));
        WelcomeCard::new()
            .version("0.1.7")
            .render(Rect::new(0, 0, 80, 16), &mut buf);
        let text = buffer_text(&buf);
        assert!(text.contains("Welcome to "));
        assert!(text.contains("the coding agent CLI"));
        assert!(text.contains("v0.1.7"));
        assert_no_mascot(&text);
    }

    #[test]
    fn welcome_card_narrow_and_wide_viewports_keep_the_splash() {
        for (width, height) in [(40, 12), (120, 40)] {
            let card = WelcomeCard::new().version("0.1.7");
            let text = card
                .to_lines(width)
                .into_iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains("Cortex"));
            assert!(text.contains("v0.1.7"));
            assert_no_mascot(&text);
            assert!(!text.contains("> cortex"));

            let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
            WelcomeCard::new()
                .version("0.1.7")
                .render(Rect::new(0, 0, width, height), &mut buf);
            let rendered = buffer_text(&buf);
            assert!(rendered.contains("Cortex"));
            assert_no_mascot(&rendered);
        }
    }
}
