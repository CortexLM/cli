//! Renderer for interactive selection in the input area.

use super::state::{EffortLevel, InlineFormState, InteractiveItem, InteractiveState};
use cortex_core::style::{
    ACCENT, BAR_HOVER, BORDER_FOCUS, HAIRLINE, SELECTION_BG, SUCCESS, SURFACE_1, TEXT, TEXT_DIM,
    TEXT_MUTED,
};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// Rows the search field takes: hairline, `/ query`, hairline.
pub const SEARCH_FIELD_ROWS: u16 = 3;

/// Placeholder of an empty search field.
pub const SEARCH_PLACEHOLDER: &str = "Type to search";

/// Widget for rendering the interactive selection list.
pub struct InteractiveWidget<'a> {
    state: &'a InteractiveState,
    /// Skip title, hairline, hints, and `Clear` — rows sit above the composer.
    rows_only: bool,
}

/// Paint a full-width hairline on row `y`.
fn hairline(area: Rect, y: u16, buf: &mut Buffer) {
    if y >= area.bottom() {
        return;
    }
    buf.set_string(
        area.x,
        y,
        "─".repeat(area.width as usize),
        Style::default().fg(HAIRLINE),
    );
}

impl<'a> InteractiveWidget<'a> {
    /// Create a new interactive widget.
    pub fn new(state: &'a InteractiveState) -> Self {
        Self {
            state,
            rows_only: false,
        }
    }

    /// Paint only the option rows (slash / model / effort) above the composer.
    pub fn rows_only(mut self) -> Self {
        self.rows_only = true;
        self
    }

    /// Calculate click zones for the interactive list.
    /// Call this after rendering to populate state.click_zones.
    pub fn calculate_click_zones(state: &mut InteractiveState, area: Rect) {
        state.click_zones.clear();
        state.tab_click_zones.clear();

        // Calculate tab click zones (on title line)
        if !state.tabs.is_empty() {
            let title = format!(" {} ", state.title);
            let title_y = area.y + 1;
            let tabs_x = area.x + 2 + title.len() as u16 + 2;
            let mut x = tabs_x;
            for (i, tab) in state.tabs.iter().enumerate() {
                let tab_text = format!(" {} ", tab.label);
                let tab_width = tab_text.len() as u16;
                let tab_rect = Rect::new(x, title_y, tab_width, 1);
                state.tab_click_zones.push((tab_rect, i));
                x += tab_width + 2;
            }
        }

        // If inline form is active, no item click zones
        if state.is_form_active() {
            return;
        }

        // Calculate the inner area (same logic as render)
        // Inner area starts after top border (1) + title line (1) = 2
        let inner = Rect::new(
            area.x,
            area.y + 2,
            area.width,
            area.height.saturating_sub(2),
        );

        if inner.height < 3 {
            return;
        }

        // Layout: search (optional, framed by hairlines) + items + hints
        let search_height = if state.searchable {
            SEARCH_FIELD_ROWS
        } else {
            0
        };
        let hints_height = 1;
        let effort_height = if state.effort_focused {
            3
        } else if state.effort.is_some() {
            0
        } else {
            0
        };
        let items_height = inner
            .height
            .saturating_sub(search_height + hints_height + effort_height);

        let items_y = inner.y + search_height;
        let items_area = Rect::new(inner.x, items_y, inner.width, items_height);

        // Register click zones for visible items
        // We need to collect indices first to avoid borrow conflicts
        let start = state.scroll_offset;
        let visible_count = state.filtered_indices.len();
        let end = (start + items_area.height as usize).min(visible_count);

        for i in 0..(end - start) {
            let y = items_area.y + i as u16;
            if y >= items_area.y + items_area.height {
                break;
            }

            let filtered_idx = start + i;
            let item_rect = Rect::new(items_area.x, y, items_area.width, 1);
            state.click_zones.push((item_rect, filtered_idx));
        }
    }

    /// Calculate the required height for this widget.
    pub fn required_height(&self) -> u16 {
        // If inline form is active, calculate form height
        if let Some(ref form) = self.state.inline_form {
            let fields_count = form.fields.len() as u16;
            let header_height = 1; // Title
            let hints_height = 1;
            let border_height = 2;
            // Each field takes 2 lines (label + input)
            return (fields_count * 2) + header_height + hints_height + border_height;
        }

        let items_count = self
            .state
            .filtered_indices
            .len()
            .min(self.state.max_visible);
        let header_height = 2; // Top hairline + title
        let search_height = if self.state.searchable {
            SEARCH_FIELD_ROWS
        } else {
            0
        };
        let hints_height = 1;
        let effort_height = if self.state.effort_focused { 3 } else { 0 };

        if self.rows_only {
            if self.state.effort_focused {
                return 3;
            }
            return items_count as u16;
        }

        (items_count as u16) + header_height + search_height + hints_height + effort_height
    }
}

impl<'a> Widget for InteractiveWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.rows_only {
            if self.state.effort_focused {
                self.render_effort_radios(area, buf);
            } else {
                self.render_items(area, buf);
            }
            return;
        }

        // Clear the area first
        Clear.render(area, buf);

        // If inline form is active, render the form instead
        if let Some(ref form) = self.state.inline_form {
            self.render_form(form, area, buf);
            return;
        }

        // A hairline separates the panel from the transcript above.
        hairline(area, area.y, buf);

        // Render title as normal text on the line below
        let title_y = area.y + 1;
        let title = format!(" {} ", self.state.title);
        buf.set_string(
            area.x + 1,
            title_y,
            &title,
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        );

        // Render tabs if present (on the same line as title, after it)
        let header_height = 2;
        if !self.state.tabs.is_empty() {
            let tabs_x = area.x + 2 + title.len() as u16 + 2;
            let mut x = tabs_x;
            for (i, tab) in self.state.tabs.iter().enumerate() {
                let is_active = i == self.state.active_tab;
                let is_hovered = self.state.hovered_tab == Some(i);
                let tab_text = format!(" {} ", tab.label);
                // Active tab: the focused selection — violet on the dark gray
                // bar. Never inverted onto the accent.
                let style = if is_active {
                    Style::default()
                        .fg(ACCENT)
                        .bg(SELECTION_BG)
                        .add_modifier(Modifier::BOLD)
                } else if is_hovered {
                    Style::default().fg(TEXT)
                } else {
                    Style::default().fg(TEXT_DIM)
                };
                buf.set_string(x, title_y, &tab_text, style);
                x += tab_text.len() as u16 + 2;
            }
        }

        // Inner area starts after top border + title line
        let inner = Rect::new(
            area.x,
            area.y + header_height,
            area.width,
            area.height.saturating_sub(header_height),
        );

        if inner.height < 3 {
            return;
        }

        // Layout: search (optional, framed by two hairlines) + items +
        // optional Effort radios + hints
        let mut constraints = Vec::new();
        if self.state.searchable && !self.state.effort_focused {
            constraints.push(Constraint::Length(SEARCH_FIELD_ROWS));
        }
        if self.state.effort_focused {
            constraints.push(Constraint::Length(3));
        } else {
            constraints.push(Constraint::Min(1)); // Items
        }
        constraints.push(Constraint::Length(1)); // Hints

        let chunks = Layout::vertical(constraints).split(inner);
        let mut chunk_idx = 0;

        // Render the search field if enabled: `/ Type to search` between two
        // hairlines — the same bar as the composer.
        if self.state.searchable && !self.state.effort_focused {
            let search_area = chunks[chunk_idx];
            chunk_idx += 1;
            render_search_field(search_area, buf, &self.state.search_query);
        }

        if self.state.effort_focused {
            let effort_area = chunks[chunk_idx];
            chunk_idx += 1;
            self.render_effort_radios(effort_area, buf);
        } else {
            let items_area = chunks[chunk_idx];
            chunk_idx += 1;
            self.render_items(items_area, buf);
        }

        // Render hints
        let hints_area = chunks[chunk_idx];
        self.render_hints(hints_area, buf);
    }
}

/// Paint the search field: a hairline, `/ query█` (or the dim placeholder),
/// and a closing hairline. `area` is `SEARCH_FIELD_ROWS` tall.
pub fn render_search_field(area: Rect, buf: &mut Buffer, query: &str) {
    if area.is_empty() {
        return;
    }
    hairline(area, area.y, buf);
    let field_y = area.y + 1;
    if field_y >= area.bottom() {
        return;
    }
    buf.set_string(area.x, field_y, "/ ", Style::default().fg(TEXT_DIM));
    let budget = area.width.saturating_sub(3) as usize;
    if query.is_empty() {
        let ghost = crate::ui::text_utils::first_fitting_line(SEARCH_PLACEHOLDER, budget);
        buf.set_string(area.x + 2, field_y, ghost, Style::default().fg(TEXT_DIM));
    } else {
        let shown = crate::ui::text_utils::first_fitting_line(query, budget);
        let typed = format!("{shown}█");
        buf.set_string(area.x + 2, field_y, typed, Style::default().fg(TEXT));
    }
    hairline(area, area.y + 2, buf);
}

impl<'a> InteractiveWidget<'a> {
    /// Render the list items.
    fn render_items(&self, area: Rect, buf: &mut Buffer) {
        let visible_items = self.state.visible_items();
        if visible_items.is_empty() {
            // A real empty state: say what did not match and how to recover,
            // whole, across the rows available.
            let copy: Vec<String> = if self.state.search_query.is_empty() {
                vec!["Nothing to show here yet.".to_string()]
            } else if area.width >= 60 {
                vec![format!(
                    "No settings match “{}” — esc clears the search.",
                    self.state.search_query
                )]
            } else {
                vec![
                    format!("No matches for “{}”", self.state.search_query),
                    "esc clears the search".to_string(),
                ]
            };
            let width = area.width.saturating_sub(2) as usize;
            let mut y = area.y;
            for text in copy {
                for part in crate::ui::text_utils::wrap_or_drop(&text, width) {
                    if y >= area.bottom() {
                        return;
                    }
                    buf.set_string(area.x + 1, y, &part, Style::default().fg(TEXT_DIM));
                    y += 1;
                }
            }
            return;
        }
        let start = self.state.scroll_offset;
        let end = (start + area.height as usize).min(visible_items.len());

        for (i, (real_idx, item)) in visible_items
            .iter()
            .skip(start)
            .take(end - start)
            .enumerate()
        {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }

            let filtered_idx = start + i;
            let is_selected = self.state.selected == filtered_idx;
            let is_hovered = self.state.hovered == Some(filtered_idx);
            let is_checked = self.state.is_checked(*real_idx);

            self.render_item(
                Rect::new(area.x, y, area.width, 1),
                buf,
                item,
                is_selected,
                is_hovered,
                is_checked,
            );
        }

        // Show scroll indicators if needed
        if start > 0 {
            buf.set_string(
                area.x + area.width.saturating_sub(3),
                area.y,
                "▲",
                Style::default().fg(TEXT_MUTED),
            );
        }
        if end < visible_items.len() {
            buf.set_string(
                area.x + area.width.saturating_sub(3),
                area.y + area.height.saturating_sub(1),
                "▼",
                Style::default().fg(TEXT_MUTED),
            );
        }
    }

    /// Render a single item.
    fn render_item(
        &self,
        area: Rect,
        buf: &mut Buffer,
        item: &InteractiveItem,
        is_selected: bool,
        is_hovered: bool,
        is_checked: bool,
    ) {
        // Selected row: the dark gray bar with a violet `>` and a violet label —
        // never inverted onto the accent. Unselected rows lead with a dim
        // middot and keep white copy.
        let selected_bar = is_selected && !item.disabled && !item.is_separator;
        let (fg, bg) = if item.disabled {
            (TEXT_MUTED, Color::Reset)
        } else if selected_bar {
            (TEXT, SELECTION_BG)
        } else if is_hovered {
            (TEXT, BAR_HOVER)
        } else {
            (TEXT, Color::Reset)
        };

        if (selected_bar || (is_hovered && !item.disabled && !item.is_separator))
            && bg != Color::Reset
        {
            for dx in 0..area.width {
                if let Some(cell) = buf.cell_mut((area.x + dx, area.y)) {
                    cell.set_bg(bg);
                    if selected_bar {
                        cell.set_fg(TEXT);
                    }
                }
            }
        }

        let mut x = area.x;
        if !item.is_separator {
            let (marker, marker_style) = if selected_bar {
                ("> ", Style::default().fg(ACCENT).bg(SELECTION_BG))
            } else if item.disabled {
                ("  ", Style::default().fg(TEXT_MUTED))
            } else {
                ("· ", Style::default().fg(TEXT_DIM))
            };
            buf.set_string(x, area.y, marker, marker_style);
        } else {
            buf.set_string(x, area.y, "  ", Style::default());
        }
        x += 2;

        // Checkbox (multi-select): a green check when on, dim brackets when
        // off.
        if self.state.multi_select {
            let checkbox = if is_checked { "[✓]" } else { "[ ]" };
            let checkbox_style = if is_checked {
                Style::default().fg(SUCCESS)
            } else {
                Style::default().fg(TEXT_DIM)
            };
            buf.set_string(x, area.y, checkbox, checkbox_style);
            x += 4;
        }

        // Icon
        if let Some(icon) = item.icon {
            buf.set_string(x, area.y, icon.to_string(), Style::default().fg(fg));
            x += 2;
        }

        // Shortcut - hidden (shortcuts still work via keyboard)

        // Label - bold for separators (category headers)
        let label_style = if item.is_separator {
            Style::default().fg(TEXT_DIM).add_modifier(Modifier::BOLD)
        } else if selected_bar {
            Style::default()
                .fg(TEXT)
                .bg(SELECTION_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        };

        let max_label_len = (area.width as usize).saturating_sub((x - area.x) as usize + 2);
        let label = crate::ui::text_utils::first_fitting_line(&item.label, max_label_len);
        buf.set_string(x, area.y, &label, label_style);
        x += label.chars().count() as u16;

        // Description right-aligned when it fits beside the label; dim even
        // on the selection bar.
        if let Some(ref desc) = item.description {
            let remaining = (area.x + area.width).saturating_sub(x + 2) as usize;
            let desc_text = crate::ui::text_utils::first_fitting_line(desc, remaining);
            if !desc_text.is_empty() {
                let desc_w = desc_text.chars().count() as u16;
                let desc_x = area.x + area.width.saturating_sub(desc_w + 1);
                if desc_x > x + 1 {
                    let desc_style = if selected_bar {
                        Style::default().fg(TEXT_DIM).bg(SELECTION_BG)
                    } else {
                        Style::default().fg(TEXT_DIM)
                    };
                    buf.set_string(desc_x, area.y, &desc_text, desc_style);
                }
            }
        }
    }

    /// Paint High / Medium / Low effort radios, High first.
    fn render_effort_radios(&self, area: Rect, buf: &mut Buffer) {
        let Some(effort) = self.state.effort else {
            return;
        };
        if area.height < 1 {
            return;
        }
        for (i, (level, label, desc)) in EffortLevel::rows().iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.bottom() {
                break;
            }
            let focused = self.state.effort_focused && *level == effort;
            let hovered = !focused && self.state.hovered == Some(1000 + i);
            if focused {
                for dx in 0..area.width {
                    if let Some(cell) = buf.cell_mut((area.x + dx, y)) {
                        cell.set_bg(SELECTION_BG);
                    }
                }
                buf.set_string(
                    area.x,
                    y,
                    "> ",
                    Style::default().fg(ACCENT).bg(SELECTION_BG),
                );
            } else if hovered {
                for dx in 0..area.width {
                    if let Some(cell) = buf.cell_mut((area.x + dx, y)) {
                        cell.set_bg(BAR_HOVER);
                    }
                }
                buf.set_string(area.x, y, "  ", Style::default().fg(TEXT_DIM).bg(BAR_HOVER));
            } else {
                buf.set_string(area.x, y, "  ", Style::default().fg(TEXT_DIM));
            }
            let name_style = if focused {
                Style::default()
                    .fg(TEXT)
                    .bg(SELECTION_BG)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            buf.set_string(area.x + 2, y, *label, name_style);
            let desc_x = area.x + 2 + 16;
            if desc_x + 4 < area.right() {
                let desc_style = if focused {
                    Style::default().fg(TEXT_DIM).bg(SELECTION_BG)
                } else {
                    Style::default().fg(TEXT_DIM)
                };
                let shown = crate::ui::text_utils::first_fitting_line(
                    desc,
                    area.right().saturating_sub(desc_x + 1) as usize,
                );
                buf.set_string(desc_x, y, &shown, desc_style);
            }
        }
    }

    /// Render the key hints at the bottom: `↑↓ select · ↵ confirm · esc close`.
    fn render_hints(&self, area: Rect, buf: &mut Buffer) {
        let hint_text = if let Some(ref custom) = self.state.hints {
            custom
                .iter()
                .map(|(key, action)| format!("{key} {action}"))
                .collect::<Vec<_>>()
                .join(" · ")
        } else {
            let mut hints = vec![("↑↓", "select"), ("↵", "confirm")];

            if self.state.multi_select {
                hints.insert(1, ("space", "toggle"));
            }

            if self.state.searchable {
                hints.push(("type", "to search"));
            }

            hints.push(("esc", "close"));
            hints
                .iter()
                .map(|(key, action)| format!("{key} {action}"))
                .collect::<Vec<_>>()
                .join(" · ")
        };

        let hint_color = TEXT_DIM;
        let shown = crate::ui::text_utils::first_fitting_line(
            &hint_text,
            area.width.saturating_sub(1) as usize,
        );
        if !shown.is_empty() {
            Paragraph::new(Span::styled(shown, Style::default().fg(hint_color))).render(area, buf);
        }
    }

    /// Render inline form for configuration within the panel.
    fn render_form(&self, form: &InlineFormState, area: Rect, buf: &mut Buffer) {
        // Draw border with form title — square corners, zero rounded frames,
        // gray hairline: violet never outlines a box.
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER_FOCUS))
            .title(Span::styled(
                format!(" {} ", form.title),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 {
            return;
        }

        // Calculate field layout: each field takes 1 line (label: value)
        let fields_count = form.fields.len();
        let mut constraints: Vec<Constraint> =
            form.fields.iter().map(|_| Constraint::Length(1)).collect();
        constraints.push(Constraint::Min(0)); // Spacer
        constraints.push(Constraint::Length(1)); // Hints

        let chunks = Layout::vertical(constraints).split(inner);

        // Render each field
        for (i, field) in form.fields.iter().enumerate() {
            if i >= chunks.len().saturating_sub(2) {
                break;
            }
            let field_area = chunks[i];
            let is_focused = i == form.focused_field;

            self.render_form_field(field_area, buf, field, is_focused);
        }

        // Render form hints
        let hints_area = chunks[fields_count + 1];
        self.render_form_hints(hints_area, buf);
    }

    /// Render a single form field.
    fn render_form_field(
        &self,
        area: Rect,
        buf: &mut Buffer,
        field: &super::state::InlineFormField,
        is_focused: bool,
    ) {
        let x = area.x + 1;

        // Label: the focused field is the selection — violet; the rest dim.
        let label_style = if is_focused {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_DIM)
        };

        let required_marker = if field.required { "*" } else { "" };
        let label = format!("{}{}:", field.label, required_marker);
        buf.set_string(x, area.y, &label, label_style);

        // Value or placeholder
        let value_x = x + label.len() as u16 + 1;
        let remaining_width = area.width.saturating_sub(value_x - area.x + 1);

        if remaining_width < 4 {
            return;
        }

        if field.value.is_empty() && !is_focused {
            // Show placeholder
            let max_ph = remaining_width as usize;
            let placeholder = if field.placeholder.len() > max_ph {
                let trunc = max_ph.saturating_sub(3);
                let end = field.placeholder.floor_char_boundary(trunc);
                format!("{}...", &field.placeholder[..end])
            } else {
                field.placeholder.clone()
            };
            buf.set_string(
                value_x,
                area.y,
                &placeholder,
                Style::default().fg(TEXT_MUTED),
            );
        } else {
            // Show value with cursor if focused
            let max_val = remaining_width as usize;
            let display_value = if field.value.len() > max_val.saturating_sub(1) {
                let tail_len = max_val.saturating_sub(4);
                let start = field.value.len().saturating_sub(tail_len);
                let start = field.value.ceil_char_boundary(start);
                format!("...{}", &field.value[start..])
            } else {
                field.value.clone()
            };

            let value_style = if is_focused {
                Style::default().fg(TEXT).bg(SURFACE_1)
            } else {
                Style::default().fg(TEXT)
            };

            // Draw input background if focused
            if is_focused {
                for xi in value_x..(value_x + remaining_width) {
                    buf[(xi, area.y)].set_bg(SURFACE_1);
                }
            }

            buf.set_string(value_x, area.y, &display_value, value_style);

            // Draw cursor
            if is_focused {
                let cursor_x = value_x + display_value.len() as u16;
                if cursor_x < area.x + area.width - 1 {
                    buf[(cursor_x, area.y)].set_char('█');
                    buf[(cursor_x, area.y)].set_fg(TEXT);
                }
            }
        }
    }

    /// Render hints for the form: `tab next · ↵ submit · esc cancel`.
    fn render_form_hints(&self, area: Rect, buf: &mut Buffer) {
        let hints = [("tab", "next"), ("↵", "submit"), ("esc", "cancel")];

        let mut spans = Vec::new();
        for (i, (key, action)) in hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" · ", Style::default().fg(TEXT_DIM)));
            }
            spans.push(Span::styled(
                format!("{key} {action}"),
                Style::default().fg(TEXT_DIM),
            ));
        }

        let hints_line = Line::from(spans);
        Paragraph::new(hints_line).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interactive::state::InteractiveAction;

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

    #[test]
    fn test_required_height() {
        let items = vec![
            InteractiveItem::new("1", "Item 1"),
            InteractiveItem::new("2", "Item 2"),
            InteractiveItem::new("3", "Item 3"),
        ];
        let state = InteractiveState::new("Test", items, InteractiveAction::Custom("test".into()));
        let widget = InteractiveWidget::new(&state);

        // 3 items + hairline + title + hints = 6
        assert_eq!(widget.required_height(), 6);
    }

    #[test]
    fn test_required_height_with_search() {
        let items = vec![
            InteractiveItem::new("1", "Item 1"),
            InteractiveItem::new("2", "Item 2"),
        ];
        let state = InteractiveState::new("Test", items, InteractiveAction::Custom("test".into()))
            .with_search();
        let widget = InteractiveWidget::new(&state);

        // 2 items + hairline + title + (hairline, search, hairline) + hints = 8
        assert_eq!(widget.required_height(), 8);
    }

    #[test]
    fn selected_row_is_violet_on_the_gray_bar_and_search_is_framed() {
        let items = vec![
            InteractiveItem::new("model", "Model").with_description("Cortex Mini 1"),
            InteractiveItem::new("mode", "Mode").with_description("Agent"),
        ];
        let state = InteractiveState::new("Settings", items, InteractiveAction::Custom("s".into()))
            .with_search();
        let widget = InteractiveWidget::new(&state);
        let area = Rect::new(0, 0, 60, 8);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        let text = buffer_text(&buf);

        // Hairline, title, then the search field framed by two hairlines.
        let rows: Vec<&str> = text.lines().collect();
        assert!(rows[0].chars().all(|c| c == '─'), "{text}");
        assert!(rows[1].contains("Settings"), "{text}");
        assert!(rows[2].chars().all(|c| c == '─'), "{text}");
        assert!(rows[3].starts_with("/ Type to search"), "{text}");
        assert!(rows[4].chars().all(|c| c == '─'), "{text}");
        assert_eq!(buf[(0, 2)].style().fg, Some(HAIRLINE), "{text}");

        // Selected row: violet `>` on the gray bar, white label, dim description.
        assert!(rows[5].starts_with("> Model"), "{text}");
        assert_eq!(buf[(0, 5)].style().fg, Some(ACCENT));
        assert_eq!(buf[(0, 5)].style().bg, Some(SELECTION_BG));
        assert_eq!(buf[(2, 5)].style().fg, Some(TEXT));
        let desc_x = rows[5].find("Cortex").expect("description") as u16;
        assert_eq!(buf[(desc_x, 5)].style().fg, Some(TEXT_DIM));
        assert_eq!(buf[(desc_x, 5)].style().bg, Some(SELECTION_BG));
        // Unselected row: dim middot, white label.
        assert!(rows[6].starts_with("· Mode"), "{text}");
        assert_eq!(buf[(0, 6)].style().fg, Some(TEXT_DIM));
        assert_eq!(buf[(2, 6)].style().fg, Some(TEXT));
        // Hints in the locked format.
        assert!(rows[7].contains("↑↓ select · ↵ confirm"), "{text}");
        assert!(rows[7].contains("esc close"), "{text}");
    }

    #[test]
    fn model_picker_paints_effort_radios_and_tab_hint() {
        let items = vec![
            InteractiveItem::new("mini", "Cortex Mini 1")
                .with_description("Fast default for everyday coding."),
            InteractiveItem::new("one", "Cortex 1")
                .with_description("Deeper reasoning for hard changes."),
        ];
        let mut state = InteractiveState::new("Model", items, InteractiveAction::SetModel)
            .with_search()
            .with_effort(crate::interactive::EffortLevel::Medium)
            .with_hints(vec![
                ("↑↓".into(), "select".into()),
                ("↵".into(), "confirm".into()),
                ("tab".into(), "effort".into()),
                ("esc".into(), "close".into()),
            ]);
        state.effort_focused = true;
        let widget = InteractiveWidget::new(&state);
        let area = Rect::new(0, 0, 72, 12);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        let text = buffer_text(&buf);

        assert!(text.contains("High Effort"), "{text}");
        assert!(text.contains("Medium Effort"), "{text}");
        assert!(text.contains("Low Effort"), "{text}");
        assert!(
            !text.contains('★') && !text.contains("A★") && !text.contains("/effort"),
            "model picker must not be an A★ /effort picker:\n{text}"
        );
    }
}
