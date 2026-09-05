//! Main MinimalSessionView struct and Widget implementation.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

use super::rendering::{
    generate_message_lines, generate_welcome_lines, render_scroll_to_bottom_hint, render_scrollbar,
};
use crate::app::AppState;
use crate::commands::PALETTE_HOME_LIMIT;
use crate::ui::chrome::{
    FooterSet, composer_caret_style, composer_inner, fill_inky, model_chip, paint_composer_box,
    paint_footer, paint_token_counter,
};
use crate::ui::colors::AdaptiveColors;
use cortex_core::style::{ACCENT, TEXT, TEXT_BRIGHT, TEXT_DIM};

// Re-export for convenience
pub use cortex_core::widgets::Message as ChatMessage;

/// Rows the composer occupies: hairline, `> ` prompt, hairline.
pub const COMPOSER_ROWS: u16 = 3;

/// White block caret. Occupies one cell; never a glyph after the placeholder.
pub const BLOCK_CURSOR: char = '█';

/// Composer placeholder while idle.
pub const PLACEHOLDER_IDLE: &str = "Plan, search, build anything";
/// Composer placeholder while a run is live — stdin stays alive and a
/// submitted follow-up is queued.
pub const PLACEHOLDER_RUNNING: &str = "Add a follow-up — Enter to queue";

/// Paint the composer input row (after `> `) to the lock:
/// empty = block cursor at input col 0, dim placeholder after that cell;
/// blink-off = no block, placeholder from col 0; typing = `#F5F5F5` + block at caret.
pub fn paint_composer_contents(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    input: &str,
    caret: usize,
    caret_visible: bool,
    placeholder: Option<&str>,
    focused: bool,
) {
    if width == 0 {
        return;
    }
    buf.set_string(x, y, "> ", composer_caret_style(focused));
    if width < 3 {
        return;
    }
    let col0 = x + 2;
    let budget = width.saturating_sub(2) as usize;

    if input.is_empty() {
        let mut col = col0;
        let mut rest = budget;
        if caret_visible {
            buf.set_string(
                col,
                y,
                BLOCK_CURSOR.to_string(),
                Style::default().fg(TEXT_BRIGHT),
            );
            col = col.saturating_add(1);
            rest = rest.saturating_sub(1);
        }
        if let Some(ph) = placeholder {
            let shown = crate::ui::text_utils::first_fitting_line(ph, rest);
            if !shown.is_empty() {
                buf.set_string(col, y, &shown, Style::default().fg(TEXT_DIM));
            }
        }
        return;
    }

    let shown = crate::ui::text_utils::first_fitting_line(input, budget.saturating_sub(1));
    let chars: Vec<char> = shown.chars().collect();
    let caret = caret.min(chars.len());
    let slash_end = if shown.starts_with('/') {
        shown.find(' ').unwrap_or(shown.len())
    } else {
        0
    };
    let mut col = col0;
    for (i, ch) in chars.iter().enumerate() {
        let fg = if i < slash_end { ACCENT } else { TEXT };
        if caret_visible && i == caret {
            buf.set_string(
                col,
                y,
                BLOCK_CURSOR.to_string(),
                Style::default().fg(TEXT_BRIGHT),
            );
        } else {
            buf.set_string(col, y, ch.to_string(), Style::default().fg(fg));
        }
        col = col.saturating_add(1);
        if col >= x + width {
            return;
        }
    }
    if caret_visible && caret == chars.len() && col < x + width {
        buf.set_string(
            col,
            y,
            BLOCK_CURSOR.to_string(),
            Style::default().fg(TEXT_BRIGHT),
        );
    }
}

/// Minimalist session view for the chat interface.
///
/// The view is frameless: content, composer and footer sit directly on the
/// host terminal background and bleed to the terminal edges. The composer is
/// the hairline-framed composer — a full-width gray hairline above the `> ` prompt and
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
/// > █Add a follow-up ↵ to queue
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
    #[allow(dead_code)]
    fn hairline(&self, area: Rect, y: u16, buf: &mut Buffer) {
        if y >= area.bottom() {
            return;
        }
        let rule = "─".repeat(area.width as usize);
        buf.set_string(area.x, y, rule, Style::default().fg(self.colors.border));
    }

    /// Renders the composer: rounded dual-hairline box with mode + model chips.
    fn render_input(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() || area.height < COMPOSER_ROWS {
            return;
        }

        let focused = self.app_state.settings_modal.is_none() && !self.app_state.shortcuts_open;
        let effort = self
            .app_state
            .thinking_budget
            .as_deref()
            .unwrap_or("medium");
        let chip = model_chip(&self.app_state.model, Some(effort));
        paint_composer_box(
            area,
            buf,
            &self.app_state.agent_mode_label,
            &chip,
            self.app_state.composer_hovered,
            focused,
        );

        let inner = composer_inner(area);
        if inner.width == 0 {
            return;
        }
        let content_y = inner.y;
        let input_text = composer_display_text(self.app_state);

        let queue_count = self.app_state.queued_count();
        let mut badge_cols = 0u16;
        if queue_count > 0 {
            let indicator = format!("[{} pending]", queue_count);
            let indicator_x = inner.right().saturating_sub(indicator.len() as u16);
            if indicator_x > inner.x + 4 {
                buf.set_string(
                    indicator_x,
                    content_y,
                    &indicator,
                    Style::default().fg(self.colors.text_dim),
                );
                badge_cols = indicator.len() as u16 + 1;
            }
        }
        let text_budget = inner.width.saturating_sub(badge_cols);
        let placeholder = if input_text.is_empty() {
            Some(if self.app_state.quota_held {
                crate::ui::consts::PLACEHOLDER_QUOTA
            } else if self.is_task_running() {
                PLACEHOLDER_RUNNING
            } else if self.app_state.agent_entrypoint {
                "Describe a task for the agent"
            } else if self.app_state.agent_mode_label == "Ask" {
                "Ask about the codebase — read-only"
            } else if self.app_state.agent_mode_label == "Plan" {
                "Describe what you want — Cortex drafts a plan first"
            } else {
                PLACEHOLDER_IDLE
            })
        } else {
            None
        };
        let caret = if input_text.is_empty() {
            0
        } else {
            self.app_state.input.cursor_pos()
        };
        paint_composer_contents(
            buf,
            inner.x,
            content_y,
            text_budget,
            &input_text,
            caret,
            self.app_state.caret_visible,
            placeholder,
            focused,
        );
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
    #[allow(dead_code)]
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

    /// Renders autocomplete suggestions inline above the composer.
    ///
    /// Focused row: dark gray bar, violet `>` , label in text with matched
    /// characters in violet. Hovered (not focused) row: `#1A1A1A` bar, no
    /// violet. Trailer is muted.
    fn render_autocomplete_inline(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let accent = self.colors.accent;
        let dim = self.colors.text_dim;
        let muted = self.colors.text_muted;
        let text = self.colors.text;
        let bar = self.colors.selection;
        let hover = self.colors.hover;
        let compact = self.app_state.compact_mode || area.width < 40;
        let indent = if compact { 0 } else { 3 };

        let remaining = self.app_state.autocomplete.items.len().saturating_sub(
            self.app_state.autocomplete.scroll_offset
                + self.app_state.autocomplete.visible_items().len(),
        );
        let more_rows = if remaining > 0 && area.height > 1 {
            1_u16
        } else {
            0
        };
        let max_items = area.height.saturating_sub(more_rows).max(1) as usize;
        let visible_items = self.app_state.autocomplete.visible_items();
        let drawn = visible_items.len().min(max_items);

        let inner_y = area.y;
        let query = self.app_state.autocomplete.query.trim_start_matches('/');

        if visible_items.is_empty() {
            buf.set_string(
                area.x + indent,
                inner_y,
                "No matching commands",
                Style::default().fg(dim),
            );
            return;
        }

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

            let idx = self.app_state.autocomplete.scroll_offset + i;
            let is_selected = idx == self.app_state.autocomplete.selected;
            let is_hovered = self.app_state.autocomplete.hovered == Some(idx) && !is_selected;
            let row_bg = if is_selected {
                Some(bar)
            } else if is_hovered {
                Some(hover)
            } else {
                None
            };

            if let Some(bg) = row_bg {
                for dx in 0..area.width {
                    if let Some(cell) = buf.cell_mut((area.x + dx, y)) {
                        cell.set_bg(bg);
                    }
                }
            }

            let marker_x = area.x + indent;
            if is_selected {
                buf.set_string(marker_x, y, "> ", Style::default().fg(accent).bg(bar));
            } else {
                let st = Style::default()
                    .fg(dim)
                    .bg(row_bg.unwrap_or(self.colors.background));
                buf.set_string(marker_x, y, "  ", st);
            }

            let mut x = marker_x + 2;
            paint_matched_label(
                buf,
                x,
                y,
                &item.label,
                query,
                is_selected,
                row_bg,
                text,
                accent,
            );
            x += (widest as u16).max(item.label.chars().count() as u16);

            if !item.description.is_empty() {
                let remaining_w = area.right().saturating_sub(x + 3) as usize;
                let desc =
                    crate::ui::text_utils::first_fitting_line(&item.description, remaining_w);
                if !desc.is_empty() {
                    let desc_style = if let Some(bg) = row_bg {
                        Style::default().fg(dim).bg(bg)
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
        if remaining > 0 && more_rows > 0 {
            let y = inner_y + drawn as u16;
            if y < area.y + area.height {
                let more = format!("… {remaining} more — keep typing to filter");
                let line = crate::ui::text_utils::first_fitting_line(
                    &more,
                    area.width.saturating_sub(indent + 1) as usize,
                );
                buf.set_string(area.x + indent, y, &line, Style::default().fg(muted));
            }
        }
    }
}

/// Paint a slash-command label, highlighting fuzzy-matched characters in accent.
fn paint_matched_label(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    label: &str,
    query: &str,
    selected: bool,
    bg: Option<ratatui::style::Color>,
    text: ratatui::style::Color,
    accent: ratatui::style::Color,
) {
    let q: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    let mut qi = 0usize;
    let mut col = x;
    for ch in label.chars() {
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        let matched = qi < q.len() && lower == q[qi];
        if matched {
            qi += 1;
        }
        let fg = if matched { accent } else { text };
        let mut style = Style::default().fg(fg);
        if selected {
            style = style.add_modifier(Modifier::BOLD);
        }
        if let Some(bg) = bg {
            style = style.bg(bg);
        }
        buf.set_string(col, y, ch.to_string(), style);
        col = col.saturating_add(1);
    }
}

impl<'a> Widget for MinimalSessionView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        fill_inky(area, buf);

        let is_task_running = self.is_task_running();
        let footer_height: u16 = 1;
        let blank_before_footer: u16 = 1;
        let footer_y = area.bottom().saturating_sub(footer_height);
        let composer_bottom = footer_y.saturating_sub(blank_before_footer);
        let composer_y = composer_bottom.saturating_sub(COMPOSER_ROWS);
        let composer_area = Rect::new(area.x, composer_y, area.width, COMPOSER_ROWS);

        let warn_tokens = self.app_state.context_window > 0
            && self.app_state.tokens_used * 100 / self.app_state.context_window.max(1) >= 90;
        paint_token_counter(
            area,
            area.y,
            buf,
            self.app_state.tokens_used,
            self.app_state.context_window,
            warn_tokens,
        );

        let autocomplete_visible = self.app_state.autocomplete.visible;
        let palette_cap = if area.height >= 20 {
            PALETTE_HOME_LIMIT
        } else {
            3
        };
        let remaining_cmds = self.app_state.autocomplete.items.len().saturating_sub(
            self.app_state
                .autocomplete
                .visible_items()
                .len()
                .min(palette_cap),
        );
        let ac_items = if autocomplete_visible {
            self.app_state
                .autocomplete
                .visible_items()
                .len()
                .min(palette_cap)
                .max(if self.app_state.autocomplete.has_items() {
                    0
                } else {
                    1
                })
                + if remaining_cmds > 0 && area.height >= 20 {
                    1
                } else {
                    0
                }
        } else {
            0
        };

        let interactive = self.app_state.is_interactive_mode();
        let effort_focused = self
            .app_state
            .get_interactive_state()
            .map(|s| s.effort_focused)
            .unwrap_or(false);
        let picker_height: u16 = if interactive {
            if let Some(state) = self.app_state.get_interactive_state() {
                if effort_focused {
                    3
                } else {
                    let n = if state.filtered_indices.is_empty() {
                        1
                    } else {
                        state.filtered_indices.len().min(state.max_visible).min(8)
                    };
                    n as u16
                }
            } else {
                0
            }
        } else if autocomplete_visible {
            ac_items as u16
        } else {
            0
        };

        let show_optin = self.app_state.opt_in_banner && !self.app_state.messages.is_empty();
        let optin_height: u16 = if show_optin {
            if area.height >= 20 { 5 } else { 3 }
        } else {
            0
        };
        let show_update_banner = self.app_state.should_show_update_banner();
        let update_banner_height: u16 = if show_update_banner { 1 } else { 0 };

        let stack_below_transcript = picker_height + optin_height + update_banner_height;
        let transcript_bottom = composer_y.saturating_sub(stack_below_transcript);
        let content_y = area.y.saturating_add(1); // row 0 is the token chip
        let content_height = transcript_bottom.saturating_sub(content_y);
        let content_area = Rect::new(area.x, content_y, area.width, content_height);
        let lines = self.content_lines(area.width, area.height);
        self.render_scrollable_content(content_area, buf, lines);

        let mut next_y = content_y.saturating_add(content_height);

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

        if show_optin {
            crate::ui::chrome::paint_opt_in_banner(
                Rect::new(area.x, next_y, area.width, optin_height),
                buf,
                self.app_state.opt_in_hover,
                None,
            );
            next_y += optin_height;
        }

        if picker_height > 0 {
            let picker_area = Rect::new(area.x, next_y, area.width, picker_height);
            if interactive {
                if let Some(state) = self.app_state.get_interactive_state() {
                    crate::interactive::InteractiveWidget::new(state)
                        .rows_only()
                        .render(picker_area, buf);
                }
            } else if autocomplete_visible {
                self.render_autocomplete_inline(picker_area, buf);
            }
        }

        self.render_input(composer_area, buf);
        self.render_footer(area, footer_y, buf, is_task_running);

        if let Some(ref modal) = self.app_state.settings_modal {
            crate::widgets::SettingsModal::new(modal).render(area, buf);
        }
        if self.app_state.shortcuts_open {
            crate::widgets::ShortcutsOverlay::new(self.app_state.cli_version.clone())
                .render(area, buf);
        }

        let _ = next_y;
    }
}

/// Footer hint while the slash palette is open, and its narrow form.
pub const PALETTE_FOOTER_HINT: &str =
    "Enter:send | Alt+Enter:newline | Shift+Tab:mode | Ctrl+x:shortcuts";
pub const PALETTE_FOOTER_HINT_SHORT: &str = "Enter:send | Ctrl+x:shortcuts";

impl<'a> MinimalSessionView<'a> {
    fn footer_set(&self, is_task_running: bool, width: u16) -> FooterSet {
        if self.app_state.quota_held {
            return FooterSet::Unavailable;
        }
        if let Some(state) = self.app_state.get_interactive_state() {
            if state.effort_focused {
                return FooterSet::Effort;
            }
            let title = state.title.to_ascii_lowercase();
            if title.contains("model") {
                return FooterSet::ModelList;
            }
            if title.contains("mcp") {
                return FooterSet::Mcp;
            }
            if title.contains("plugin") {
                return FooterSet::Plugins;
            }
            if title.contains("resume") || title.contains("session") {
                return FooterSet::Resume;
            }
            if title.contains("permission") || title.contains("approv") {
                return FooterSet::Approval;
            }
        }
        if self.app_state.agent_mode_label == "Bash" {
            return FooterSet::Bash;
        }
        if is_task_running {
            if self.app_state.queued_count() > 0 {
                return FooterSet::Queue;
            }
            return FooterSet::Running;
        }
        if self.app_state.autocomplete.visible {
            return FooterSet::Palette;
        }
        if !self.app_state.input.text().is_empty() {
            if area_is_narrow(width) {
                return FooterSet::TypedNarrow;
            }
            return FooterSet::Typed;
        }
        FooterSet::Idle
    }

    /// Contextual shortcut strip. Composer model/mode chips live on the box.
    fn render_footer(&self, area: Rect, footer_y: u16, buf: &mut Buffer, is_task_running: bool) {
        let hints_area = Rect::new(area.x, footer_y, area.width, 1);
        paint_footer(
            hints_area,
            buf,
            self.footer_set(is_task_running, area.width),
            self.app_state.footer_hover,
        );

        if self.app_state.is_viewing_subagent() && !self.app_state.is_interactive_mode() {
            crate::views::minimal_session::rendering::render_back_to_main_hint(
                hints_area,
                buf,
                &self.colors,
            );
        }
    }
}

fn area_is_narrow(width: u16) -> bool {
    width < 80
}

fn composer_display_text(state: &AppState) -> String {
    if let Some(istate) = state.get_interactive_state() {
        let title = istate.title.to_ascii_lowercase();
        if title.contains("model") {
            if istate.effort_focused {
                let name = crate::ui::text_utils::model_display_name(&state.model);
                return format!("/model {name}");
            }
            let typed = state.input.text();
            if typed.is_empty() || typed == "/model" {
                return "/model".into();
            }
        }
    }
    state.input.text()
}
