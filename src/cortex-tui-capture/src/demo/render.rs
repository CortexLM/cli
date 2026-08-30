//! Renders a [`Scene`] into a ratatui frame using the product theme.
//!
//! Every colour comes from `cortex_core::style`, so the recording tracks the
//! real TUI palette instead of a second, drifting copy of it.

use cortex_core::style::{
    BORDER, BORDER_FOCUS, CYAN_PRIMARY, SKY_BLUE, SUCCESS, SURFACE_0, TEXT, TEXT_DIM, TEXT_MUTED,
    VOID,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};

use super::scene::{Scene, TimelineBlock, ToolState};

/// Height reserved for the bordered composer.
const COMPOSER_HEIGHT: u16 = 3;

/// Draw a scene across the whole frame.
pub fn draw_scene(frame: &mut Frame, scene: &Scene) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(Style::default().bg(VOID)), area);

    let body = Block::default()
        .padding(Padding::new(2, 2, 1, 1))
        .inner(area);

    let [timeline_area, status_area, composer_area, hints_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(COMPOSER_HEIGHT),
        Constraint::Length(1),
    ])
    .areas(body);

    if scene.is_empty_session() {
        draw_welcome(frame, timeline_area, scene);
    } else {
        draw_timeline(frame, timeline_area, scene);
    }

    draw_status(frame, status_area, scene);
    draw_composer(frame, composer_area, scene);
    draw_hints(frame, hints_area, scene);
}

fn draw_welcome(frame: &mut Frame, area: Rect, scene: &Scene) {
    let card_width = area.width.min(82);
    let card_height = 11.min(area.height);
    let card = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(card_height) / 2,
        width: card_width,
        height: card_height,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(SURFACE_0))
        .padding(Padding::new(2, 2, 1, 0))
        .title(Span::styled(
            " Cortex Code ",
            Style::default()
                .fg(CYAN_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ));

    let lines = vec![
        Line::from(Span::styled(
            "A coding agent that reads, edits, runs and tests your project.",
            Style::default().fg(TEXT),
        )),
        Line::default(),
        field_line("workspace", &scene.workspace),
        field_line("endpoint", &scene.endpoint),
        field_line(
            "mode",
            &format!("{} · {} autonomy", scene.mode, scene.autonomy),
        ),
        Line::default(),
        Line::from(Span::styled(
            "Describe a change and Cortex Code works through it, tool call by tool call.",
            Style::default().fg(TEXT_MUTED),
        )),
    ];

    frame.render_widget(Paragraph::new(lines).block(block), card);
}

fn field_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<11}"), Style::default().fg(TEXT_MUTED)),
        Span::styled(value.to_string(), Style::default().fg(TEXT_DIM)),
    ])
}

fn draw_timeline(frame: &mut Frame, area: Rect, scene: &Scene) {
    let mut lines: Vec<Line<'static>> = Vec::new();

    for (index, block) in scene.blocks.iter().enumerate() {
        let next = scene.blocks.get(index + 1);
        match block {
            TimelineBlock::User(text) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "> ",
                        Style::default()
                            .fg(CYAN_PRIMARY)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(text.clone(), Style::default().fg(SKY_BLUE)),
                ]));
                lines.push(Line::default());
            }
            TimelineBlock::Agent(paragraphs) => {
                for text in paragraphs {
                    lines.push(Line::from(Span::styled(
                        text.clone(),
                        Style::default().fg(TEXT),
                    )));
                }
                lines.push(Line::default());
            }
            TimelineBlock::Tool(row) => {
                let (glyph, glyph_style) = match row.state {
                    ToolState::Running => ("◐", Style::default().fg(CYAN_PRIMARY)),
                    ToolState::Done => ("●", Style::default().fg(SUCCESS)),
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("{glyph} "), glyph_style),
                    Span::styled(
                        row.name.clone(),
                        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("  ", Style::default()),
                    Span::styled(row.summary.clone(), Style::default().fg(TEXT_DIM)),
                ]));
                if let Some(result) = &row.result {
                    lines.push(Line::from(vec![
                        Span::styled("  ⎿ ", Style::default().fg(TEXT_MUTED)),
                        Span::styled(result.clone(), Style::default().fg(TEXT_MUTED)),
                    ]));
                }
                // Consecutive tool rows read as one run of work, so they are not
                // separated by a blank line the way prose blocks are.
                if !matches!(next, Some(TimelineBlock::Tool(_))) {
                    lines.push(Line::default());
                }
            }
        }
    }

    // The timeline is pinned to the bottom, the way scrollback behaves once the
    // transcript is taller than the viewport.
    let height = area.height as usize;
    if lines.len() > height {
        lines.drain(..lines.len() - height);
    }

    frame.render_widget(Paragraph::new(lines).style(Style::default().bg(VOID)), area);
}

fn draw_status(frame: &mut Frame, area: Rect, scene: &Scene) {
    let line = match &scene.status {
        Some(status) => Line::from(vec![
            Span::styled(
                format!("{} ", status.spinner_glyph()),
                Style::default().fg(CYAN_PRIMARY),
            ),
            Span::styled(
                status.header.clone(),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}s", status.elapsed_secs),
                Style::default().fg(TEXT_MUTED),
            ),
            Span::styled("  ·  Esc to interrupt", Style::default().fg(TEXT_MUTED)),
        ]),
        None => Line::default(),
    };

    frame.render_widget(Paragraph::new(line).style(Style::default().bg(VOID)), area);
}

fn draw_composer(frame: &mut Frame, area: Rect, scene: &Scene) {
    let border_style = if scene.status.is_some() {
        Style::default().fg(BORDER)
    } else {
        Style::default().fg(BORDER_FOCUS)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .style(Style::default().bg(VOID))
        .padding(Padding::horizontal(1));

    let mut spans = vec![Span::styled(
        "> ",
        Style::default()
            .fg(CYAN_PRIMARY)
            .add_modifier(Modifier::BOLD),
    )];

    if scene.composer.is_empty() && scene.status.is_none() {
        spans.push(Span::styled(
            "Describe a change…",
            Style::default().fg(TEXT_MUTED),
        ));
    } else {
        spans.push(Span::styled(
            scene.composer.clone(),
            Style::default().fg(TEXT),
        ));
    }

    if scene.cursor_on {
        spans.push(Span::styled("▌", Style::default().fg(CYAN_PRIMARY)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn draw_hints(frame: &mut Frame, area: Rect, scene: &Scene) {
    let right_text = format!("{} · {}", scene.mode, scene.autonomy);
    let right_width = (right_text.chars().count() as u16).min(area.width);

    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right_width)]).areas(area);

    let left = Line::from(Span::styled(
        "Enter send  ·  Shift+Enter newline  ·  Ctrl+K palette  ·  Shift+Tab autonomy  ·  /help",
        Style::default().fg(TEXT_MUTED),
    ));

    let right = Line::from(vec![
        Span::styled(
            scene.mode.clone(),
            Style::default()
                .fg(CYAN_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", Style::default().fg(TEXT_MUTED)),
        Span::styled(scene.autonomy.clone(), Style::default().fg(TEXT_DIM)),
    ]);

    frame.render_widget(
        Paragraph::new(left).style(Style::default().bg(VOID)),
        left_area,
    );
    frame.render_widget(
        Paragraph::new(right)
            .right_aligned()
            .style(Style::default().bg(VOID)),
        right_area,
    );
}
