//! Main MinimalSessionView struct and Widget implementation.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use super::rendering::{
    generate_message_lines, generate_welcome_lines, render_scroll_to_bottom_hint, render_scrollbar,
};
use crate::app::AppState;
use crate::commands::PALETTE_HOME_LIMIT;
use crate::ui::colors::AdaptiveColors;
use crate::widgets::{HintContext, KeyHints, StatusIndicator};
use cortex_core::style::SELECTION_BG;

// Re-export for convenience
pub use cortex_core::widgets::Message as ChatMessage;

/// Minimalist session view for the chat interface.
///
/// Layout:
/// ```text
/// ┌─────────────────────────────────────────────────────────┐
/// │ > You: Hello, how are you?                              │
/// │                                                         │
/// │ Assistant: I'm doing well! How can I help you today?    │
/// │                                                         │
/// │ ⠹ Working · Analyzing code... (12s • Esc to interrupt)  │
/// ├─────────────────────────────────────────────────────────┤
/// │ > _                                                     │
/// ├─────────────────────────────────────────────────────────┤
/// │ Enter submit · Ctrl+K palette · Ctrl+M model · ? help   │
/// └─────────────────────────────────────────────────────────┘
/// ```
pub struct MinimalSessionView<'a> {
    /// Reference to the application state
    app_state: &'a AppState,
    /// Color palette
    colors: AdaptiveColors,
}

impl<'a> MinimalSessionView<'a> {
    /// Creates a new minimal session view.
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            app_state,
            colors: app_state.adaptive_colors(),
        }
    }

    /// Renders all scrollable content (welcome cards + messages) as unified lines.
    /// Returns the actual content height rendered (for dynamic input positioning).
    fn render_scrollable_content(&self, area: Rect, buf: &mut Buffer, _welcome_height: u16) -> u16 {
        if area.is_empty() || area.height == 0 {
            return 0;
        }

        let mut all_lines: Vec<Line<'static>> = Vec::new();

        // 1. Generate welcome card lines (same visual style as render_motd)
        all_lines.extend(generate_welcome_lines(
            area.width,
            &self.colors,
            self.app_state,
        ));

        if self.app_state.compact_mode {
            all_lines.push(Line::from(""));
        } else {
            all_lines.push(Line::from(""));
            all_lines.push(Line::from(""));
        }

        // 3. Generate message lines
        all_lines.extend(generate_message_lines(
            area.width,
            &self.colors,
            self.app_state,
        ));

        let total_lines = all_lines.len();
        let visible_lines = area.height as usize;

        // Calculate scroll bounds
        let max_scroll = total_lines.saturating_sub(visible_lines);
        let scroll_offset = self.app_state.chat_scroll.min(max_scroll);

        // Calculate visible window
        let start = if total_lines > visible_lines {
            total_lines - visible_lines - scroll_offset
        } else {
            0
        };
        let end = (start + visible_lines).min(total_lines);

        // Render the visible portion
        let visible: Vec<Line<'static>> = all_lines[start..end].to_vec();
        let paragraph = Paragraph::new(visible);
        paragraph.render(area, buf);

        // Render scrollbar if needed
        if total_lines > visible_lines {
            let opacity = self.app_state.scrollbar_opacity();
            render_scrollbar(
                area,
                buf,
                total_lines,
                scroll_offset,
                max_scroll,
                visible_lines,
                opacity,
            );
        }

        // Render "go to bottom" indicator if not at bottom
        if !self.app_state.is_chat_at_bottom() && total_lines > visible_lines {
            render_scroll_to_bottom_hint(area, buf, &self.colors);
        }

        // Return actual content height (capped at area height)
        (total_lines as u16).min(area.height)
    }

    /// Renders the input area.
    fn render_input(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() || area.height < 1 {
            return;
        }

        let _ = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        let queue_count = self.app_state.queued_count();
        if queue_count > 0 {
            let indicator = format!("[{} pending]", queue_count);
            let indicator_x = area.right().saturating_sub(indicator.len() as u16);
            if indicator_x > area.x {
                buf.set_string(
                    indicator_x,
                    area.y,
                    &indicator,
                    Style::default().fg(self.colors.warning),
                );
            }
        }

        let input_text = self.app_state.input.text();

        // Simple unboxed prompt. Ghost when idle; block cursor.
        let content_x = area.x;
        let content_y = area.y;
        let content_width = area.width.max(1);

        let prompt_span = Span::styled("> ", Style::default().fg(self.colors.accent));
        let mut spans = vec![prompt_span];
        if input_text.is_empty() {
            let ghost = crate::ui::text_utils::first_fitting_line(
                "Plan, search, build anything",
                content_width.saturating_sub(2) as usize,
            );
            if !ghost.is_empty() {
                spans.push(Span::styled(
                    ghost,
                    Style::default().fg(self.colors.text_muted),
                ));
            }
        } else {
            let shown = crate::ui::text_utils::first_fitting_line(
                &input_text,
                content_width.saturating_sub(2) as usize,
            );
            spans.push(Span::styled(shown, Style::default().fg(self.colors.text)));
        }
        spans.push(Span::styled("█", Style::default().fg(self.colors.accent)));

        let line = Line::from(spans);
        let text_area = Rect::new(content_x, content_y, content_width, 1);
        let paragraph = Paragraph::new(line);
        paragraph.render(text_area, buf);
    }

    /// Returns the cursor position for the input field.
    pub fn cursor_position(&self, input_area: Rect) -> Option<(u16, u16)> {
        // Cursor is after "> " prefix (2 chars) plus the input text
        // Input starts at input_area.x + 2 (border + space)
        // Text starts after prompt "> " (length 2)
        // So cursor is at input_area.x + 2 + 2 + cursor_pos

        let cursor_pos = self.app_state.input.cursor_pos();
        // x = area.x + border(1) + space(1) + prompt(2) + cursor_pos
        let x = input_area.x + 4 + cursor_pos as u16;
        let y = input_area.y + 1; // Middle line

        if x < input_area.right() - 2 {
            // Ensure inside right border
            Some((x, y))
        } else {
            None
        }
    }

    /// Returns whether a task is currently running.
    fn is_task_running(&self) -> bool {
        self.app_state.streaming.is_streaming
            || self.app_state.streaming.is_tool_executing()
            || self.app_state.streaming.is_delegating
            || self.app_state.has_active_subagents()
    }

    /// Returns the status header text based on current state.
    fn status_header(&self) -> String {
        // Check for delegation/subagent first (highest priority)
        if self.app_state.streaming.is_delegating || self.app_state.has_active_subagents() {
            "Delegation".to_string()
        } else if self.app_state.streaming.is_tool_executing() {
            let tool_name = self
                .app_state
                .streaming
                .executing_tool
                .as_deref()
                .unwrap_or("tool");
            format!("Executing {}", tool_name)
        } else if self.app_state.streaming.thinking && self.app_state.thinking_budget.is_some() {
            "Thinking".to_string()
        } else if self.app_state.streaming.is_streaming {
            // Differentiate between waiting for first token and actively streaming
            if self.app_state.streaming.is_actively_streaming {
                "Streaming..".to_string()
            } else {
                "Execute".to_string()
            }
        } else {
            "Idle".to_string()
        }
    }

    /// Renders autocomplete suggestions inline below the input.
    /// The top stays fixed, only the bottom varies with item count.
    fn render_autocomplete_inline(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let accent = self.colors.accent;
        let dim = self.colors.text_dim;
        let text = self.colors.text;
        let border_style = Style::default().fg(dim);

        // Calculate actual height based on items (top stays fixed, bottom varies)
        let visible_items = self.app_state.autocomplete.visible_items();
        let remaining = self
            .app_state
            .autocomplete
            .items
            .len()
            .saturating_sub(self.app_state.autocomplete.scroll_offset + visible_items.len());
        let more_rows = if remaining > 0 { 1_u16 } else { 0 };
        let draw_box = area.width >= 50 && area.height > PALETTE_HOME_LIMIT as u16 + more_rows + 2;
        let chrome = if draw_box { 2 } else { 0 };
        let max_items = area.height.saturating_sub(chrome + more_rows).max(1) as usize;
        let drawn = visible_items.len().min(max_items);

        let actual_height = drawn as u16 + more_rows + chrome;
        if draw_box {
            // Draw top border with rounded corners
            if let Some(cell) = buf.cell_mut((area.x, area.y)) {
                cell.set_char('╭').set_style(border_style);
            }
            if let Some(cell) = buf.cell_mut((area.right() - 1, area.y)) {
                cell.set_char('╮').set_style(border_style);
            }
            for x in (area.x + 1)..(area.right() - 1) {
                if let Some(cell) = buf.cell_mut((x, area.y)) {
                    cell.set_char('─').set_style(border_style);
                }
            }

            // Draw side borders (only for actual content height)
            for y in (area.y + 1)..(area.y + actual_height - 1) {
                if let Some(cell) = buf.cell_mut((area.x, y)) {
                    cell.set_char('│').set_style(border_style);
                }
                if let Some(cell) = buf.cell_mut((area.right() - 1, y)) {
                    cell.set_char('│').set_style(border_style);
                }
            }

            // Draw bottom border at actual content height (not at area.bottom)
            let bottom_y = area.y + actual_height - 1;
            if bottom_y > area.y {
                if let Some(cell) = buf.cell_mut((area.x, bottom_y)) {
                    cell.set_char('╰').set_style(border_style);
                }
                if let Some(cell) = buf.cell_mut((area.right() - 1, bottom_y)) {
                    cell.set_char('╯').set_style(border_style);
                }
                for x in (area.x + 1)..(area.right() - 1) {
                    if let Some(cell) = buf.cell_mut((x, bottom_y)) {
                        cell.set_char('─').set_style(border_style);
                    }
                }
            }
        }

        let inner_y = if draw_box { area.y + 1 } else { area.y };
        let inner_x = if draw_box { area.x + 2 } else { area.x + 1 };

        if visible_items.is_empty() {
            buf.set_string(
                inner_x,
                inner_y,
                "No matching commands",
                Style::default().fg(dim),
            );
            return;
        }

        for (i, item) in visible_items.iter().take(drawn).enumerate() {
            let y = inner_y + i as u16;
            if y >= area.y + area.height {
                break;
            }

            let is_selected = self.app_state.autocomplete.scroll_offset + i
                == self.app_state.autocomplete.selected;
            if is_selected {
                for dx in 0..area.width {
                    if let Some(cell) = buf.cell_mut((area.x + dx, y)) {
                        cell.set_bg(SELECTION_BG);
                        cell.set_fg(text);
                    }
                }
                buf.set_string(
                    inner_x.saturating_sub(1),
                    y,
                    "> ",
                    Style::default().fg(accent).bg(SELECTION_BG),
                );
            }

            let mut x = inner_x + 1;
            let label_style = if is_selected {
                Style::default()
                    .fg(text)
                    .bg(SELECTION_BG)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(text)
            };
            let label = crate::ui::text_utils::first_fitting_line(
                &item.label,
                area.right().saturating_sub(x + 1) as usize,
            );
            buf.set_string(x, y, &label, label_style);
            x += label.chars().count() as u16;

            if !item.description.is_empty() {
                let remaining = area.right().saturating_sub(x + 3) as usize;
                let desc = crate::ui::text_utils::first_fitting_line(&item.description, remaining);
                if !desc.is_empty() {
                    // Descriptions stay dim even on the selection bar; only
                    // the label is bright and only the `>` marker is mint.
                    let desc_style = if is_selected {
                        Style::default().fg(dim).bg(SELECTION_BG)
                    } else {
                        Style::default().fg(dim)
                    };
                    buf.set_string(x, y, "  ", desc_style);
                    buf.set_string(x + 2, y, &desc, desc_style);
                }
            }
        }

        let remaining = self
            .app_state
            .autocomplete
            .items
            .len()
            .saturating_sub(self.app_state.autocomplete.scroll_offset + drawn);
        if remaining > 0 {
            let y = inner_y + drawn as u16;
            if y < area.y + area.height {
                let more = format!("{remaining} more — keep typing to filter");
                let line = crate::ui::text_utils::first_fitting_line(
                    &more,
                    area.width.saturating_sub(2) as usize,
                );
                buf.set_string(
                    inner_x.saturating_sub(1),
                    y,
                    &line,
                    Style::default().fg(dim),
                );
            }
        }
    }
}

impl<'a> Widget for MinimalSessionView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let is_task_running = self.is_task_running();

        // Calculate fixed heights
        let autocomplete_visible = self.app_state.autocomplete.visible;
        let remaining_cmds = self.app_state.autocomplete.items.len().saturating_sub(
            self.app_state
                .autocomplete
                .visible_items()
                .len()
                .min(PALETTE_HOME_LIMIT),
        );
        let ac_items = if autocomplete_visible {
            self.app_state
                .autocomplete
                .visible_items()
                .len()
                .min(PALETTE_HOME_LIMIT)
                .max(if self.app_state.autocomplete.has_items() {
                    0
                } else {
                    1
                })
                + if remaining_cmds > 0 { 1 } else { 0 }
        } else {
            0
        };
        let status_height: u16 = if is_task_running { 1 } else { 0 };
        let show_update_banner = self.app_state.should_show_update_banner();
        let update_banner_height: u16 = if show_update_banner { 1 } else { 0 };
        let input_height: u16 = 1;
        let hints_height: u16 = 1;

        let bottom_stack = status_height + update_banner_height + input_height + hints_height;
        let box_pad: u16 = if area.width >= 50 { 2 } else { 0 };
        let max_ac_height = area.height.saturating_sub(bottom_stack.saturating_add(1));
        let autocomplete_height: u16 = if autocomplete_visible {
            ((ac_items as u16).saturating_add(box_pad))
                .min(max_ac_height)
                .max(1)
        } else {
            0
        };

        let hints_y = area.y + area.height.saturating_sub(hints_height);
        let input_y = hints_y.saturating_sub(autocomplete_height + input_height);
        let content_height = if autocomplete_visible {
            0
        } else {
            input_y.saturating_sub(area.y)
        };
        let content_area = Rect::new(area.x, area.y, area.width, content_height);
        self.render_scrollable_content(content_area, buf, 1);

        let mut next_y = input_y;
        if is_task_running && status_height > 0 {
            let status_area = Rect::new(
                area.x,
                next_y.saturating_sub(status_height),
                area.width,
                status_height,
            );
            let _ = status_area;
        }

        if show_update_banner {
            let banner_y = input_y.saturating_sub(update_banner_height);
            let banner_area = Rect::new(area.x, banner_y, area.width, update_banner_height);
            super::rendering::render_update_banner(
                banner_area,
                buf,
                &self.colors,
                &self.app_state.update_status,
            );
        }

        if is_task_running {
            let status_area = Rect::new(
                area.x,
                input_y.saturating_sub(update_banner_height + status_height),
                area.width,
                status_height,
            );
            let header = self.status_header();
            let elapsed = self.app_state.streaming.prompt_elapsed_seconds();
            let status = StatusIndicator::new(header)
                .with_elapsed_secs(elapsed)
                .with_interrupt_hint(true);
            status.render(status_area, buf);
        }

        let input_area = Rect::new(area.x, input_y, area.width, input_height);

        if self.app_state.is_interactive_mode() {
            if let Some(state) = self.app_state.get_interactive_state() {
                let items_count = state.filtered_indices.len().min(state.max_visible);
                let required_height = (items_count as u16) + 3;
                let max_height = area.height.saturating_sub(hints_height).max(3);
                let widget_height = required_height.min(max_height);
                let interactive_y = area.y + area.height - widget_height - hints_height;
                let interactive_area = Rect::new(area.x, interactive_y, area.width, widget_height);
                let widget = crate::interactive::InteractiveWidget::new(state);
                widget.render(interactive_area, buf);
            }
        } else {
            self.render_input(input_area, buf);
        }

        next_y = input_y + input_height;
        if autocomplete_visible {
            let autocomplete_area = Rect::new(area.x, next_y, area.width, autocomplete_height);
            self.render_autocomplete_inline(autocomplete_area, buf);
        }

        if !self.app_state.is_interactive_mode() {
            let hints_area = Rect::new(area.x, hints_y, area.width, hints_height);
            let context = if self.app_state.is_viewing_subagent() {
                HintContext::SubagentView
            } else if is_task_running {
                HintContext::TaskRunning
            } else {
                HintContext::Idle
            };
            let mut hints =
                KeyHints::new(context).with_permission_mode(self.app_state.permission_mode);
            hints = hints.with_model(&self.app_state.model);
            hints = hints.with_session_footer(
                &self.app_state.footer_cwd,
                &self.app_state.git_branch,
                self.app_state.git_dirty,
                &self.app_state.agent_mode_label,
                self.app_state.context_percent,
            );
            if let Some(ref budget) = self.app_state.thinking_budget {
                hints = hints.with_thinking_budget(budget);
            }
            hints.render(hints_area, buf);

            if self.app_state.is_viewing_subagent() {
                crate::views::minimal_session::rendering::render_back_to_main_hint(
                    hints_area,
                    buf,
                    &self.colors,
                );
            }
        }
    }
}
