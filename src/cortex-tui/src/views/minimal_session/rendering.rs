//! Rendering functions for minimal session view.
//!
//! Contains all render_* methods for messages, tool calls, subagents, and UI elements.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget,
};

use cortex_core::markdown::MarkdownTheme;
use cortex_core::widgets::{Message, MessageRole};
use cortex_tui_components::welcome_card::{InfoCard, InfoCardPair, ToLines, WelcomeCard};

use crate::app::{AppState, SubagentDisplayStatus, SubagentTaskDisplay};
use crate::ui::colors::AdaptiveColors;
use crate::ui::text_utils::wrap_or_drop;
use crate::views::tool_call::{ContentSegment, ToolCallDisplay, ToolStatus};

use super::VERSION;
use super::text_utils::wrap_text;

/// Renders the "← Back to main conversation" hint when viewing a subagent.
/// Displays in the top-left area of the screen.
pub fn render_back_to_main_hint(area: Rect, buf: &mut Buffer, colors: &AdaptiveColors) {
    let hint = "← Back to main (Esc)";
    let style = Style::default().fg(colors.text_dim);
    // Render at the start of the area with 1 character padding
    buf.set_string(area.x + 1, area.y, hint, style);
}

/// Renders a single message to lines with optional markdown theme.
pub fn render_message_with_theme(
    msg: &Message,
    width: u16,
    colors: &AdaptiveColors,
    markdown_theme: &MarkdownTheme,
    compact: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    match msg.role {
        MessageRole::User => {
            // "> message" — violet marker, gray/white copy (locked chrome).
            let prefix = Span::styled("> ", Style::default().fg(colors.accent));

            // Calculate available width for text (after "> " prefix)
            let text_width = (width as usize).saturating_sub(3); // "> " + margin

            // Wrap text and render each line
            let wrapped_lines = wrap_text(&msg.content, text_width);
            for (i, line_content) in wrapped_lines.iter().enumerate() {
                if i == 0 {
                    lines.push(Line::from(vec![
                        prefix.clone(),
                        Span::styled(line_content.clone(), Style::default().fg(colors.text)),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw("  "), // Indent continuation (2 spaces = "> " length)
                        Span::styled(line_content.clone(), Style::default().fg(colors.text)),
                    ]));
                }
            }
        }
        MessageRole::Assistant => {
            // Use full markdown renderer with theme
            use cortex_core::markdown::MarkdownRenderer;

            // Create renderer with width and theme
            let content_width = width.saturating_sub(4); // Leave margin
            let renderer =
                MarkdownRenderer::with_theme(markdown_theme.clone()).with_width(content_width);

            // Render markdown content
            let mut rendered_lines = renderer.render(&msg.content);

            // Add streaming cursor if still streaming
            if msg.is_streaming
                && let Some(last) = rendered_lines.last_mut()
            {
                last.spans
                    .push(Span::styled("▌", Style::default().fg(colors.text)));
            }

            lines.extend(rendered_lines);
        }
        MessageRole::System => {
            // Detect error messages - show in error color
            let is_error = msg.content.contains("Check your")
                || msg.content.contains("temporarily unavailable")
                || msg.content.contains("Access denied")
                || msg.content.contains("timed out")
                || msg.content.contains("failed")
                || msg.content.contains("Invalid")
                || msg.content.contains("limit")
                || msg.content.starts_with("Error:")
                || msg.content.contains("provider appears to be overloaded")
                || msg.content.contains("internet connection")
                || msg.content.contains("proxy is experiencing issues");

            // No prefix for any system messages - use error color for errors, muted for info
            let text_color = if is_error {
                colors.error
            } else {
                colors.text_muted
            };

            // Calculate available width for text (no prefix)
            let text_width = (width as usize).saturating_sub(1);

            // Wrap text and render each line
            let wrapped_lines = wrap_text(&msg.content, text_width);
            for line_content in wrapped_lines.iter() {
                lines.push(Line::from(vec![Span::styled(
                    line_content.clone(),
                    Style::default().fg(text_color),
                )]));
            }
        }
        MessageRole::Tool => {
            // "[>] tool_name: result"
            let prefix = Span::styled("[>] ", Style::default().fg(colors.accent));
            let tool_name = msg.tool_name.as_deref().unwrap_or("tool");
            let name_span = Span::styled(
                format!("{}: ", tool_name),
                Style::default().fg(colors.text_dim),
            );

            // Truncate content for tool results
            let max_content = 100;
            let content = if msg.content.len() > max_content {
                format!("{}...", &msg.content[..max_content])
            } else {
                msg.content.clone()
            };

            lines.push(Line::from(vec![
                prefix,
                name_span,
                Span::styled(content, Style::default().fg(colors.text_muted)),
            ]));
        }
    }

    // System notices stack tightly (an error and its next step belong
    // together); conversation turns keep their breathing room.
    if !compact && msg.role != MessageRole::System {
        lines.push(Line::from(""));
    }

    lines
}

/// Renders a single tool call as one card: name + path + body.
pub fn render_tool_call(
    call: &ToolCallDisplay,
    width: u16,
    colors: &AdaptiveColors,
) -> Vec<Line<'static>> {
    use crate::ui::consts::TOOL_SPINNER_FRAMES;
    let mut lines = Vec::new();
    let inner = (width as usize).saturating_sub(1).max(1);

    let (dot, dot_color) = match call.status {
        ToolStatus::Pending => (None, colors.text_muted),
        ToolStatus::Running => {
            let frame = TOOL_SPINNER_FRAMES[call.spinner_frame % TOOL_SPINNER_FRAMES.len()];
            // Spinners stay gray — violet is reserved for markers and success.
            (Some(frame.to_string()), colors.text_dim)
        }
        // Completed tiles carry the violet status dot, same as the locked
        // Grep tile; the label stays white.
        ToolStatus::Completed => (Some("●".to_string()), colors.accent),
        ToolStatus::Failed => (Some("●".to_string()), colors.error),
    };

    let name = crate::views::tool_call::tool_tile_label(&call.name);
    let path = crate::views::tool_call::format_tool_summary(&call.name, &call.arguments);
    let mut header = name.clone();
    if !path.is_empty() {
        header.push(' ');
        header.push_str(&path);
    }

    let mut header_spans = Vec::new();
    if let Some(dot) = dot {
        header_spans.push(Span::styled(
            format!("{dot} "),
            Style::default().fg(dot_color),
        ));
    }
    let header_text = wrap_or_drop(&header, inner.saturating_sub(2))
        .into_iter()
        .next()
        .unwrap_or(name);
    header_spans.push(Span::styled(
        header_text,
        Style::default()
            .fg(colors.text)
            .add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::from(header_spans));

    if call.status == ToolStatus::Running && call.result.is_none() && !call.live_output.is_empty() {
        for output_line in &call.live_output {
            for wrapped in wrap_or_drop(output_line, inner.saturating_sub(2)) {
                lines.push(Line::from(Span::styled(
                    wrapped,
                    Style::default().fg(colors.text_dim),
                )));
            }
        }
    }

    let body = call
        .result
        .as_ref()
        .map(|r| r.output.as_str())
        .unwrap_or("");
    if !body.is_empty() {
        let max_body = if width < 50 { 4 } else { 8 };
        let mut emitted = 0usize;
        for raw in body.lines() {
            if emitted >= max_body {
                break;
            }
            let is_add = raw.starts_with('+') && !raw.starts_with("+++");
            let is_del = raw.starts_with('-') && !raw.starts_with("---");
            // Diff additions are the only green in the chrome.
            let color = if is_add {
                colors.diff_add
            } else if is_del {
                colors.error
            } else {
                colors.text_dim
            };
            let wrapped = wrap_or_drop(raw, inner);
            if wrapped.is_empty() {
                continue;
            }
            for wrapped_line in wrapped {
                if emitted >= max_body {
                    break;
                }
                lines.push(Line::from(Span::styled(
                    wrapped_line,
                    Style::default().fg(color),
                )));
                emitted += 1;
            }
        }
    } else if let Some(ref result) = call.result {
        let summary = result.summary.trim();
        let has_diff = summary.contains('+') || summary.contains('−') || summary.contains('-');
        for line in wrap_or_drop(summary, inner) {
            lines.push(Line::from(Span::styled(
                line,
                Style::default().fg(if has_diff {
                    colors.diff_add
                } else {
                    colors.text_dim
                }),
            )));
        }
    }

    lines.push(Line::from(""));
    lines
}

/// Renders a subagent task with todos in Factory-style format
///
/// Format:
/// ```text
/// ● Task {agent_type}
///   ⎿ [pending] task1
///     [in_progress] task2
///     [completed] task3
/// ```
pub fn render_subagent(
    task: &SubagentTaskDisplay,
    width: u16,
    colors: &AdaptiveColors,
) -> Vec<Line<'static>> {
    use crate::app::SubagentTodoStatus;
    let mut lines = Vec::new();

    // Calculate available width for content (accounting for indentation)
    let content_width = (width as usize).saturating_sub(6); // 6 chars for prefix/indent
    let line_width = (width as usize).saturating_sub(8); // 8 chars for nested content

    // Status indicator with color
    let (indicator, indicator_color) = match &task.status {
        SubagentDisplayStatus::Starting
        | SubagentDisplayStatus::Thinking
        | SubagentDisplayStatus::ExecutingTool(_) => ("●", colors.accent),
        SubagentDisplayStatus::Completed => ("●", colors.success),
        SubagentDisplayStatus::Failed => ("●", colors.error),
    };

    // Line 1: ● Task {agent_type}
    lines.push(Line::from(vec![
        Span::styled(indicator, Style::default().fg(indicator_color)),
        Span::raw(" "),
        Span::styled(
            format!("Task {}", task.agent_type),
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // Display error message if task failed
    if task.status == SubagentDisplayStatus::Failed {
        if let Some(ref error_msg) = task.error_message {
            lines.push(Line::from(vec![
                Span::styled("  ⎿ ", Style::default().fg(colors.text_muted)),
                Span::styled("Error: ", Style::default().fg(colors.error)),
            ]));
            // Display error message with wrapping
            for err_line in error_msg.lines().take(5) {
                let wrapped = wrap_text(err_line, line_width.saturating_sub(4));
                for wrapped_line in wrapped.iter() {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default().fg(colors.text_muted)),
                        Span::styled(wrapped_line.clone(), Style::default().fg(colors.error)),
                    ]));
                }
            }
        } else {
            // Fallback: no error message provided
            lines.push(Line::from(vec![
                Span::styled("  ⎿ ", Style::default().fg(colors.text_muted)),
                Span::styled("Task failed", Style::default().fg(colors.error)),
            ]));
        }
    } else if !task.todos.is_empty() {
        // Display todos if any - use ⎿ prefix for first, space for rest
        for (i, todo) in task.todos.iter().enumerate() {
            let (status_text, status_color) = match todo.status {
                SubagentTodoStatus::Completed => ("[completed]", colors.success),
                SubagentTodoStatus::InProgress => ("[in_progress]", colors.accent),
                SubagentTodoStatus::Pending => ("[pending]", colors.text_muted),
            };
            // Calculate max content width (accounting for status text)
            let max_content = content_width.saturating_sub(status_text.len() + 1);
            let content = if todo.content.len() > max_content {
                format!(
                    "{}...",
                    &todo
                        .content
                        .chars()
                        .take(max_content.saturating_sub(3))
                        .collect::<String>()
                )
            } else {
                todo.content.clone()
            };
            // First line uses ⎿, rest use indentation
            let prefix = if i == 0 { "  ⎿ " } else { "    " };
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(colors.text_muted)),
                Span::styled(status_text, Style::default().fg(status_color)),
                Span::styled(" ", Style::default()),
                Span::styled(content, Style::default().fg(colors.text_dim)),
            ]));
        }
    } else {
        // No todos yet - show current activity with ⎿ (wrap if too long)
        let activity = if task.current_activity.is_empty() {
            "Initializing...".to_string()
        } else if task.current_activity.len() > content_width {
            format!(
                "{}...",
                &task
                    .current_activity
                    .chars()
                    .take(content_width.saturating_sub(3))
                    .collect::<String>()
            )
        } else {
            task.current_activity.clone()
        };
        lines.push(Line::from(vec![
            Span::styled("  ⎿ ", Style::default().fg(colors.text_muted)),
            Span::styled(activity, Style::default().fg(colors.text_dim)),
        ]));
    }

    lines.push(Line::from("")); // Spacing
    lines
}

/// Generates welcome card as styled lines using TUI components.
pub fn generate_welcome_lines(
    width: u16,
    colors: &AdaptiveColors,
    app_state: &AppState,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    let version = if app_state.cli_version.is_empty() {
        VERSION
    } else {
        app_state.cli_version.as_str()
    };
    let welcome_card = WelcomeCard::new()
        .version(version)
        .text_color(colors.text)
        .dim_color(colors.text_dim)
        .border_color(colors.text_dim);

    lines.extend(welcome_card.to_lines(width));

    let empty = app_state.messages.is_empty()
        && !app_state.streaming.is_streaming
        && app_state.tool_calls.is_empty();
    if empty {
        // The keystroke hints are part of the empty-session chrome at every
        // width; narrow terminals show the leading keys that fit.
        let hints = "/ commands · @ files · ! shell · shift+tab modes";
        let line = crate::ui::text_utils::first_fitting_line(hints, width as usize);
        if !line.is_empty() {
            lines.push(Line::from(Span::styled(
                line,
                Style::default().fg(colors.text_dim),
            )));
        }
    }

    lines
}

/// Generates message lines for scrollable content.
pub fn generate_message_lines(
    width: u16,
    colors: &AdaptiveColors,
    app_state: &AppState,
) -> Vec<Line<'static>> {
    let mut all_lines: Vec<Line<'static>> = Vec::new();

    if app_state.messages.is_empty()
        && !app_state.streaming.is_streaming
        && app_state.content_segments.is_empty()
    {
        return all_lines;
    }

    // Determine what content we have for display
    let has_tool_calls = !app_state.tool_calls.is_empty();
    let has_content_segments = !app_state.content_segments.is_empty();
    let last_is_assistant = app_state
        .messages
        .last()
        .map(|m| m.role == cortex_core::widgets::MessageRole::Assistant)
        .unwrap_or(false);

    // If we have content segments, skip the last assistant message (it's in the segments)
    let messages_to_render = if has_content_segments && last_is_assistant {
        let len = app_state.messages.len();
        &app_state.messages[..len.saturating_sub(1)]
    } else {
        &app_state.messages[..]
    };

    // Get markdown theme from app state
    let markdown_theme = &app_state.markdown_theme;

    for msg in messages_to_render.iter() {
        all_lines.extend(render_message_with_theme(
            msg,
            width,
            colors,
            markdown_theme,
            app_state.compact_mode,
        ));
    }

    // Get streaming content if any
    let streaming_content = if app_state.streaming.is_streaming {
        app_state
            .typewriter
            .as_ref()
            .map(|tw| tw.visible_text().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    };

    // Render content segments (interleaved text and tool calls)
    if has_content_segments {
        let mut sorted_segments: Vec<_> = app_state.content_segments.iter().collect();
        sorted_segments.sort_by_key(|s| s.sequence());

        let last_tool_id = sorted_segments.iter().rev().find_map(|seg| match seg {
            ContentSegment::ToolCall { tool_call_id, .. } => Some(tool_call_id.as_str()),
            _ => None,
        });

        for segment in sorted_segments {
            match segment {
                ContentSegment::Text { content, .. } => {
                    all_lines.extend(render_text_content_with_theme(
                        content,
                        width,
                        markdown_theme,
                    ));
                }
                ContentSegment::ToolCall { tool_call_id, .. } => {
                    if width < 50 && last_tool_id != Some(tool_call_id.as_str()) {
                        continue;
                    }
                    if let Some(call) = app_state.tool_calls.iter().find(|c| &c.id == tool_call_id)
                    {
                        all_lines.extend(render_tool_call(call, width, colors));
                    }
                }
            }
        }

        if app_state.streaming.is_streaming {
            let pending_text = &app_state.pending_text_segment;
            if !pending_text.is_empty() {
                all_lines.extend(render_streaming_content_with_theme(
                    pending_text,
                    width,
                    colors,
                    markdown_theme,
                ));
            }
        }
    } else if has_tool_calls {
        let mut sorted_calls: Vec<_> = app_state.tool_calls.iter().collect();
        sorted_calls.sort_by_key(|c| c.sequence);

        if let Some(ref content) = streaming_content {
            all_lines.extend(render_streaming_content_with_theme(
                content,
                width,
                colors,
                markdown_theme,
            ));
        }

        let last_id = sorted_calls.last().map(|c| c.id.as_str());
        for call in &sorted_calls {
            if width < 50 && last_id != Some(call.id.as_str()) {
                continue;
            }
            all_lines.extend(render_tool_call(call, width, colors));
        }
    } else if let Some(ref content) = streaming_content {
        all_lines.extend(render_streaming_content_with_theme(
            content,
            width,
            colors,
            markdown_theme,
        ));
    }

    // Render active subagents
    for task in &app_state.active_subagents {
        all_lines.extend(render_subagent(task, width, colors));
    }

    all_lines
}

/// Renders finalized text content with markdown theme (without streaming cursor).
/// Used for text segments that are already committed in content_segments.
pub fn render_text_content_with_theme(
    content: &str,
    width: u16,
    markdown_theme: &MarkdownTheme,
) -> Vec<Line<'static>> {
    use cortex_core::markdown::MarkdownRenderer;

    let content_width = width.saturating_sub(4);
    let renderer = MarkdownRenderer::with_theme(markdown_theme.clone()).with_width(content_width);

    // No cursor for finalized content
    renderer.render(content)
}

/// Renders streaming content with cursor and markdown theme.
/// Used only for actively streaming content (pending_text_segment).
pub fn render_streaming_content_with_theme(
    content: &str,
    width: u16,
    colors: &AdaptiveColors,
    markdown_theme: &MarkdownTheme,
) -> Vec<Line<'static>> {
    use cortex_core::markdown::MarkdownRenderer;

    let content_width = width.saturating_sub(4);
    let renderer = MarkdownRenderer::with_theme(markdown_theme.clone()).with_width(content_width);
    let mut rendered_lines = renderer.render(content);

    // Add streaming cursor to the last line
    if let Some(last) = rendered_lines.last_mut() {
        last.spans
            .push(Span::styled("▌", Style::default().fg(colors.text)));
    }

    rendered_lines.push(Line::from(""));
    rendered_lines
}

/// Renders a thin scrollbar on the right side with fade effect.
pub fn render_scrollbar(
    area: Rect,
    buf: &mut Buffer,
    total_lines: usize,
    scroll_offset: usize,
    max_scroll: usize,
    visible_lines: usize,
    opacity: f32,
) {
    if opacity <= 0.0 {
        return;
    }

    // No scrollbar needed if content fits
    if total_lines <= visible_lines || max_scroll == 0 {
        return;
    }

    // Calculate thumb color with fade (base: gray #606060)
    let gray_value = (0x60 as f32 * opacity) as u8;
    let thumb_color = Color::Rgb(gray_value, gray_value, gray_value);

    // scroll_offset = 0 means at bottom, max_scroll means at top
    // Scrollbar position: 0 = top of content, total_lines - visible_lines = bottom
    // When scroll_offset = 0 (at bottom), position should be at max (bottom of scrollbar)
    // When scroll_offset = max_scroll (at top), position should be 0 (top of scrollbar)
    let scrollbar_position = max_scroll.saturating_sub(scroll_offset);

    // Create scrollbar state
    // content_length = max_scroll (the scrollable range)
    // position = where we are in that range
    let mut scrollbar_state = ScrollbarState::new(max_scroll).position(scrollbar_position);

    // Render thin scrollbar on right edge
    Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(None) // Invisible track for clean look
        .thumb_symbol("▐") // Thin character
        .thumb_style(Style::default().fg(thumb_color))
        .render(area, buf, &mut scrollbar_state);
}

/// Renders a hint to scroll to bottom when user has scrolled up.
pub fn render_scroll_to_bottom_hint(area: Rect, buf: &mut Buffer, colors: &AdaptiveColors) {
    let hint = " ↓ End ";
    let hint_width = hint.len() as u16;

    // Position: bottom-right of chat area
    let x = area.right().saturating_sub(hint_width + 2);
    let y = area.bottom().saturating_sub(1);

    if x >= area.x && y >= area.y {
        // Background pill style
        let style = Style::default()
            .fg(colors.text)
            .bg(Color::Rgb(0x30, 0x30, 0x30))
            .add_modifier(Modifier::BOLD);

        buf.set_string(x, y, hint, style);
    }
}

/// Renders the MOTD (Message of the Day) with cards layout.
///
/// Layout: splash line, then two info cards below.
pub fn _render_motd(area: Rect, buf: &mut Buffer, colors: &AdaptiveColors, app_state: &AppState) {
    let card_width = 79_u16.min(area.width.saturating_sub(2));
    let welcome_card_height = 1_u16;
    let info_cards_height = 4_u16; // 2 items + 2 borders
    let gap = 1_u16;
    let total_height = welcome_card_height + gap + info_cards_height;

    // Ensure we have enough space
    if area.width < 40 || area.height < total_height {
        _render_welcome_text_centered(area, buf, colors, app_state);
        return;
    }

    // Center horizontally, start 1 line from top
    let x_offset = area.width.saturating_sub(card_width) / 2;
    let y_start = 1_u16; // Start 1 line below the top

    // Welcome card area
    let welcome_area = Rect::new(
        area.x + x_offset,
        area.y + y_start,
        card_width,
        welcome_card_height,
    );

    let welcome_card = WelcomeCard::new()
        .version(if app_state.cli_version.is_empty() {
            VERSION
        } else {
            app_state.cli_version.as_str()
        })
        .text_color(colors.text)
        .dim_color(colors.text_dim)
        .border_color(colors.text_dim);

    welcome_card.render(welcome_area, buf);

    // Info cards area (below welcome card)
    let info_area = Rect::new(
        area.x + x_offset,
        area.y + y_start + welcome_card_height + gap,
        card_width,
        info_cards_height,
    );

    // Get info from app_state
    let org_name = app_state.org_name.as_deref().unwrap_or("Personal");
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "~/".to_string());

    // Left card: Directory, Org
    let left_card = InfoCard::new()
        .add("Directory", &cwd)
        .add("Org", org_name)
        .dim_color(colors.text_dim)
        .text_color(colors.text)
        .border_color(colors.text_dim);

    // Right card: Plan + where tools run
    let right_card = InfoCard::new()
        .add("Plan", "Pro")
        .add(
            "Computer",
            cortex_engine::client::ComputerKind::detect().label(),
        )
        .dim_color(colors.text_dim)
        .text_color(colors.text)
        .border_color(colors.text_dim);

    // Render info cards side by side
    InfoCardPair::new(left_card, right_card)
        .gap(2)
        .right_width(25)
        .render(info_area, buf);
}

/// Renders the welcome text next to the brain (legacy).
#[allow(dead_code)]
pub fn render_welcome_text(area: Rect, buf: &mut Buffer, colors: &AdaptiveColors, model: &str) {
    let accent = colors.accent;
    let text_color = colors.text;
    let dim = colors.text_dim;

    let short_model = model.rsplit('/').next().unwrap_or(model);

    let lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            "Welcome to Cortex",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled("─────────────────", Style::default().fg(dim))),
        Line::from(""),
        Line::from(vec![
            Span::styled("Model: ", Style::default().fg(dim)),
            Span::styled(short_model.to_string(), Style::default().fg(text_color)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "• Type a message to start",
            Style::default().fg(dim),
        )),
        Line::from(Span::styled(
            "• Use / for commands",
            Style::default().fg(dim),
        )),
        Line::from(Span::styled("• Press ? for help", Style::default().fg(dim))),
        Line::from(Span::styled("• Ctrl+Q to quit", Style::default().fg(dim))),
    ];

    let paragraph = Paragraph::new(lines);
    paragraph.render(area, buf);
}

/// Renders welcome text centered (fallback for small terminals).
pub fn _render_welcome_text_centered(
    area: Rect,
    buf: &mut Buffer,
    colors: &AdaptiveColors,
    _app_state: &AppState,
) {
    let accent = colors.accent;
    let dim = colors.text_dim;

    let lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            "Welcome to Cortex",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Type a message to start • / for commands • ? for help",
            Style::default().fg(dim),
        )),
    ];

    // Center vertically
    let y_offset = area.height.saturating_sub(3) / 2;
    let text_area = Rect::new(area.x, area.y + y_offset, area.width, 3);

    let paragraph = Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center);
    paragraph.render(text_area, buf);
}

/// Renders an update notification banner above the input box.
/// Shows different states: Available, Downloading (with progress), ReadyToRestart
pub fn render_update_banner(
    area: Rect,
    buf: &mut Buffer,
    colors: &AdaptiveColors,
    update_status: &crate::app::UpdateStatus,
) {
    use crate::app::UpdateStatus;

    if area.is_empty() || area.height < 1 {
        return;
    }

    let (icon, text, style) = match update_status {
        UpdateStatus::Available { version } => {
            let icon = "↑";
            let text = format!(" A new version ({}) is available ", version);
            let style = Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD);
            (icon, text, style)
        }
        UpdateStatus::Downloading {
            version: _,
            progress,
        } => {
            let icon = "⟳";
            let text = format!(" Downloading update... {}% ", progress);
            let style = Style::default().fg(colors.warning);
            (icon, text, style)
        }
        UpdateStatus::ReadyToRestart { version: _ } => {
            let icon = "✓";
            let text = " You must restart to run the latest version ".to_string();
            let style = Style::default()
                .fg(colors.success)
                .add_modifier(Modifier::BOLD);
            (icon, text, style)
        }
        _ => return, // Don't render for other states
    };

    // Calculate banner width
    let banner_width = (icon.len() + text.len() + 2) as u16; // +2 for spacing

    // Position at left side of the area with some padding
    let x = area.x + 2;
    let y = area.y;

    // Ensure we don't overflow
    if x + banner_width > area.right() {
        return;
    }

    // Render icon
    buf.set_string(x, y, icon, style);

    // Render text
    buf.set_string(x + icon.len() as u16 + 1, y, &text, style);
}
