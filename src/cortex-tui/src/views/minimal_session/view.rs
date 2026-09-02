//! Main MinimalSessionView struct and Widget implementation.

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

// Re-export for convenience
pub use cortex_core::widgets::Message as ChatMessage;

/// Rows the composer occupies: hairline, `> ` prompt, hairline.
pub const COMPOSER_ROWS: u16 = 3;

/// Composer placeholder while idle.
pub const PLACEHOLDER_IDLE: &str = "Plan, search, build anything";
/// Composer placeholder while a run is live — stdin stays alive and a
/// submitted follow-up is queued.
pub const PLACEHOLDER_RUNNING: &str = "Add a follow-up ↵ to queue";

/// Minimalist session view for the chat interface.
///
/// The view is frameless: content, composer and footer sit directly on the
/// host terminal background and bleed to the terminal edges. The composer is
/// the Devin-style bar — a full-width gray hairline above the `> ` prompt and
/// another below it — and follows the transcript until the transcript fills
/// the screen, after which it stays pinned above the footer.
///
/// Layout:
/// ```text
/// ▏> Hello, how are you?            ← past user turn on its gray bar
///
/// I'm doing well! How can I help you today?
///
/// ⠇ Working · 12s · esc to interrupt
/// ────────────────────────────────────
/// > Add a follow-up ↵ to queue █
/// ────────────────────────────────────
/// Cortex Mini 1 · Agent · 92% context      shift+tab to cycle modes
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

    /// All scrollable content (welcome header + messages) as unified lines.
    fn content_lines(&self, width: u16, height: u16) -> Vec<Line<'static>> {
        let mut all_lines: Vec<Line<'static>> = Vec::new();

        all_lines.extend(generate_welcome_lines(width, &self.colors, self.app_state));

        let message_lines = generate_message_lines(width, &self.colors, self.app_state);
        if !message_lines.is_empty() {
            all_lines.push(Line::from(""));
            // Short terminals keep one blank row under the header, tall ones
            // two.
            if !self.app_state.compact_mode && height >= 20 {
                all_lines.push(Line::from(""));
            }
            all_lines.extend(message_lines);
        }
        // One blank row between the transcript and the composer hairline —
        // a transcript that already ends blank does not add another.
        let ends_blank = all_lines
            .last()
            .map(|line| line.to_string().trim().is_empty())
            .unwrap_or(false);
        if !ends_blank {
            all_lines.push(Line::from(""));
        }
        all_lines
    }

    /// Renders the scrollable content into `area`, newest lines at the
    /// bottom, honouring the chat scroll offset.
    fn render_scrollable_content(
        &self,
        area: Rect,
        buf: &mut Buffer,
        all_lines: Vec<Line<'static>>,
    ) {
        if area.is_empty() || area.height == 0 {
            return;
        }

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
    }

    /// Paints one full-width hairline on row `y`.
    fn hairline(&self, area: Rect, y: u16, buf: &mut Buffer) {
        if y >= area.bottom() {
            return;
        }
        let rule = "─".repeat(area.width as usize);
        buf.set_string(area.x, y, rule, Style::default().fg(self.colors.border));
    }

    /// Renders the composer: hairline, `> ` prompt, hairline. `area` is
    /// `COMPOSER_ROWS` tall.
    fn render_input(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() || area.height < COMPOSER_ROWS {
            return;
        }

        self.hairline(area, area.y, buf);
        self.hairline(area, area.y + 2, buf);

        let content_y = area.y + 1;
        let content_width = area.width.max(1);
        let input_text = self.app_state.input.text();

        // The pending badge sits at the right edge of the prompt row.
        let queue_count = self.app_state.queued_count();
        let mut badge_cols = 0u16;
        if queue_count > 0 {
            let indicator = format!("[{} pending]", queue_count);
            let indicator_x = area.right().saturating_sub(indicator.len() as u16);
            if indicator_x > area.x + 4 {
                buf.set_string(
                    indicator_x,
                    content_y,
                    &indicator,
                    Style::default().fg(self.colors.text_dim),
                );
                badge_cols = indicator.len() as u16 + 1;
            }
        }
        let text_budget = content_width.saturating_sub(3 + badge_cols) as usize;

        // White `>`; dim placeholder while idle; white copy and block cursor.
        let mut spans = vec![Span::styled("> ", Style::default().fg(self.colors.text))];
        if input_text.is_empty() {
            let ghost_copy = if self.is_task_running() {
                PLACEHOLDER_RUNNING
            } else {
                PLACEHOLDER_IDLE
            };
            let ghost = crate::ui::text_utils::first_fitting_line(ghost_copy, text_budget);
            if !ghost.is_empty() {
                spans.push(Span::styled(
                    format!("{ghost} "),
                    Style::default().fg(self.colors.text_dim),
                ));
            }
        } else {
            let shown = crate::ui::text_utils::first_fitting_line(&input_text, text_budget);
            spans.push(Span::styled(shown, Style::default().fg(self.colors.text)));
        }
        spans.push(Span::styled("█", Style::default().fg(self.colors.text)));

        let text_area = Rect::new(area.x, content_y, content_width, 1);
        Paragraph::new(Line::from(spans)).render(text_area, buf);
    }

    /// Returns the cursor position for the input field.
    pub fn cursor_position(&self, input_area: Rect) -> Option<(u16, u16)> {
        // Cursor is after the "> " prefix (2 chars) plus the input text, on
        // the middle row of the composer (between the two hairlines).
        let cursor_pos = self.app_state.input.cursor_pos();
        let x = input_area.x + 2 + cursor_pos as u16;
        let y = input_area.y + 1;

        if x < input_area.right() {
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
            // Locked copy: a live turn is "Working", whether the first token
            // has arrived yet or not.
            "Working".to_string()
        } else {
            "Idle".to_string()
        }
    }

    /// Renders autocomplete suggestions inline below the composer.
    ///
    /// The popup is frameless at every width: rows sit directly on the host
    /// terminal background. The focused row is the dark gray selection bar
    /// with a violet `>` and a violet label; every other row is a dim `·`, a
    /// white label and a dim description.
    fn render_autocomplete_inline(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let accent = self.colors.accent;
        let dim = self.colors.text_dim;
        let text = self.colors.text;
        let bar = self.colors.selection;

        // Calculate actual height based on items (top stays fixed, bottom varies)
        let visible_items = self.app_state.autocomplete.visible_items();
        let remaining = self
            .app_state
            .autocomplete
            .items
            .len()
            .saturating_sub(self.app_state.autocomplete.scroll_offset + visible_items.len());
        let more_rows = if remaining > 0 { 1_u16 } else { 0 };
        let max_items = area.height.saturating_sub(more_rows).max(1) as usize;
        let drawn = visible_items.len().min(max_items);

        let inner_y = area.y;

        if visible_items.is_empty() {
            buf.set_string(
                area.x,
                inner_y,
                "No matching commands",
                Style::default().fg(dim),
            );
            return;
        }

        // Descriptions line up in one column after the widest label.
        let widest = visible_items
            .iter()
            .take(drawn)
            .map(|item| item.label.chars().count())
            .max()
            .unwrap_or(0);

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
                        cell.set_bg(bar);
                        cell.set_fg(text);
                    }
                }
                buf.set_string(area.x, y, "> ", Style::default().fg(accent).bg(bar));
            } else {
                buf.set_string(area.x, y, "· ", Style::default().fg(dim));
            }

            let mut x = area.x + 2;
            let label_style = if is_selected {
                Style::default()
                    .fg(accent)
                    .bg(bar)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(text)
            };
            let label = crate::ui::text_utils::first_fitting_line(
                &item.label,
                area.right().saturating_sub(x + 1) as usize,
            );
            buf.set_string(x, y, &label, label_style);
            x += (widest as u16).max(label.chars().count() as u16);

            if !item.description.is_empty() {
                let remaining = area.right().saturating_sub(x + 3) as usize;
                let desc = crate::ui::text_utils::first_fitting_line(&item.description, remaining);
                if !desc.is_empty() {
                    // Descriptions stay dim even on the selection bar; only
                    // the `>` and the label are violet.
                    let desc_style = if is_selected {
                        Style::default().fg(dim).bg(bar)
                    } else {
                        Style::default().fg(dim)
                    };
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
                buf.set_string(area.x, y, &line, Style::default().fg(dim));
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
        let footer_height: u16 = 1;
        let footer_y = area.bottom().saturating_sub(footer_height);

        // ---- bottom stack sizes -------------------------------------------
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

        let rows_above_footer = footer_y.saturating_sub(area.y);
        let fixed_stack = status_height + update_banner_height + COMPOSER_ROWS;
        let max_ac_height = rows_above_footer.saturating_sub(fixed_stack.saturating_add(1));
        let autocomplete_height: u16 = if autocomplete_visible {
            (ac_items as u16).min(max_ac_height).max(1)
        } else {
            0
        };

        // ---- interactive pickers replace the composer --------------------
        if self.app_state.is_interactive_mode() {
            if let Some(state) = self.app_state.get_interactive_state() {
                // Always leave rows for the empty-state copy and one for the
                // search filter, so "no matches" is never a blank panel.
                let items_count = if state.filtered_indices.is_empty() {
                    2
                } else {
                    state.filtered_indices.len().min(state.max_visible)
                };
                let search_rows: u16 = if state.searchable { 3 } else { 0 };
                let required_height = (items_count as u16) + 3 + search_rows;
                let max_height = rows_above_footer.max(3);
                let widget_height = required_height.min(max_height);
                let interactive_y = footer_y.saturating_sub(widget_height);
                let content_area = Rect::new(
                    area.x,
                    area.y,
                    area.width,
                    interactive_y.saturating_sub(area.y),
                );
                let lines = self.content_lines(area.width, area.height);
                self.render_scrollable_content(content_area, buf, lines);
                let interactive_area = Rect::new(area.x, interactive_y, area.width, widget_height);
                let widget = crate::interactive::InteractiveWidget::new(state);
                widget.render(interactive_area, buf);
            }
            self.render_footer(area, footer_y, buf, is_task_running);
            return;
        }

        // ---- transcript, then the composer right under it ---------------
        let lines = self.content_lines(area.width, area.height);
        let stack = fixed_stack + autocomplete_height;
        let max_content = rows_above_footer.saturating_sub(stack);
        let content_height = (lines.len() as u16).min(max_content);
        let content_area = Rect::new(area.x, area.y, area.width, content_height);
        self.render_scrollable_content(content_area, buf, lines);

        let mut next_y = area.y + content_height;

        if is_task_running {
            let status_area = Rect::new(area.x, next_y, area.width, status_height);
            let header = self.status_header();
            let elapsed = self.app_state.streaming.prompt_elapsed_seconds();
            let status = StatusIndicator::new(header)
                .with_elapsed_secs(elapsed)
                .with_interrupt_hint(true);
            status.render(status_area, buf);
            next_y += status_height;
        }

        if show_update_banner {
            let banner_area = Rect::new(area.x, next_y, area.width, update_banner_height);
            super::rendering::render_update_banner(
                banner_area,
                buf,
                &self.colors,
                &self.app_state.update_status,
            );
            next_y += update_banner_height;
        }

        let input_area = Rect::new(area.x, next_y, area.width, COMPOSER_ROWS);
        self.render_input(input_area, buf);
        next_y += COMPOSER_ROWS;

        if autocomplete_visible {
            let autocomplete_area = Rect::new(area.x, next_y, area.width, autocomplete_height);
            self.render_autocomplete_inline(autocomplete_area, buf);
        }

        self.render_footer(area, footer_y, buf, is_task_running);
    }
}

/// Footer hint while the slash palette is open, and its narrow form.
pub const PALETTE_FOOTER_HINT: &str = "↑↓ select · ↵ run · tab complete · esc close";
pub const PALETTE_FOOTER_HINT_SHORT: &str = "↵ run · esc close";

impl<'a> MinimalSessionView<'a> {
    /// The session footer stays on screen in every mode, including the
    /// interactive pickers: model · mode · context on the left, one shortcut
    /// hint on the right, all gray. The hint follows the context — the
    /// palette's keys while it is open, nothing while a picker panel shows
    /// its own hints row, `shift+tab to cycle modes` otherwise.
    fn render_footer(&self, area: Rect, footer_y: u16, buf: &mut Buffer, is_task_running: bool) {
        let hints_area = Rect::new(area.x, footer_y, area.width, 1);
        let context = if self.app_state.is_viewing_subagent() {
            HintContext::SubagentView
        } else if is_task_running {
            HintContext::TaskRunning
        } else {
            HintContext::Idle
        };
        let mut hints = KeyHints::new(context)
            .with_colors(self.colors.clone())
            .with_permission_mode(self.app_state.permission_mode)
            .with_model(&self.app_state.model)
            .with_session_footer(
                &self.app_state.agent_mode_label,
                self.app_state.context_percent,
            );
        if self.app_state.is_interactive_mode() {
            hints = hints.with_footer_hint("", "");
        } else if self.app_state.autocomplete.visible {
            hints = hints.with_footer_hint(PALETTE_FOOTER_HINT, PALETTE_FOOTER_HINT_SHORT);
        }
        if let Some(ref budget) = self.app_state.thinking_budget {
            hints = hints.with_thinking_budget(budget);
        }
        hints.render(hints_area, buf);

        if self.app_state.is_viewing_subagent() && !self.app_state.is_interactive_mode() {
            crate::views::minimal_session::rendering::render_back_to_main_hint(
                hints_area,
                buf,
                &self.colors,
            );
        }
    }
}
