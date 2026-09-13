//! Working checklist rows for the live session (lock `todos`).

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::app::{AppState, SubagentTodoStatus};
use crate::ui::colors::AdaptiveColors;
use crate::ui::text_utils::wrap_or_drop;

/// Paint `⠇ Working n/m` plus ✓ / › / ○ rows. Empty when no session checklist.
pub fn render_working_checklist(
    app_state: &AppState,
    width: u16,
    colors: &AdaptiveColors,
) -> Vec<Line<'static>> {
    let Some(board) = app_state.working_checklist.as_ref() else {
        return Vec::new();
    };
    if board.items.is_empty() {
        return Vec::new();
    }
    let indent = if app_state.compact_mode { 0 } else { 3 };
    let inner = (width as usize).saturating_sub(indent + 2).max(8);
    let mut lines = Vec::new();
    let header = format!("{}{}", " ".repeat(indent), board.header());
    lines.push(Line::from(Span::styled(
        header,
        Style::default().fg(colors.text_dim),
    )));
    for todo in &board.items {
        let (mark, color) = match todo.status {
            SubagentTodoStatus::Completed => ("✓", colors.success),
            SubagentTodoStatus::InProgress => ("›", colors.text),
            SubagentTodoStatus::Pending => ("○", colors.text_muted),
        };
        let prefix = format!("{}  {} ", " ".repeat(indent), mark);
        let room = inner.saturating_sub(mark.len() + 2);
        let body = wrap_or_drop(&todo.content, room)
            .into_iter()
            .next()
            .unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(color)),
            Span::styled(body, Style::default().fg(colors.text_dim)),
        ]));
    }
    lines.push(Line::from(""));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{SubagentTodoItem, WorkingChecklist};

    #[test]
    fn paints_marks_and_counts() {
        let mut state = AppState::default();
        state.working_checklist = Some(WorkingChecklist::new(
            vec![
                SubagentTodoItem::new(
                    "Read composer.rs and footer.rs",
                    SubagentTodoStatus::Completed,
                ),
                SubagentTodoItem::new(
                    "Move the chip into the bottom hairline",
                    SubagentTodoStatus::InProgress,
                ),
                SubagentTodoItem::new("Run the TUI snapshot tests", SubagentTodoStatus::Pending),
            ],
            38,
            6_100,
        ));
        let colors = AdaptiveColors::default_dark();
        let lines = render_working_checklist(&state, 80, &colors);
        let text: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(text.contains("Working 1/3"), "{text}");
        assert!(text.contains("✓"), "{text}");
        assert!(text.contains("›"), "{text}");
        assert!(text.contains("○"), "{text}");
        assert!(text.contains("Read composer.rs"), "{text}");
    }
}
