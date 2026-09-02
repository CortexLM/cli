//! Table rendering.
//!
//! [`render_table`] is the only table renderer: every table — finished or
//! still open when a streamed reply is cut off — comes out as the framed
//! plus-ASCII grid. There is no frameless `Header | Header` / `---+---`
//! variant.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::border;
use super::types::{Alignment, CELL_PADDING, Table, TableCell};
use super::utils::{align_text, truncate_with_ellipsis};

/// Renders a table to Lines with the full plus-ASCII grid (`+---+`, `|`).
///
/// Every row is framed: rules start and end with `+`, cell rows with `|`.
///
/// ```text
/// +-----+-----+
/// | col | col |
/// +-----+-----+
/// | a   | b   |
/// +-----+-----+
/// ```
///
/// # Arguments
/// * `table` - The table to render
/// * `border_color` - Color for border characters
/// * `header_style` - Style for header cell content
/// * `cell_style` - Style for data cell content
/// * `max_width` - Maximum total width for the table
///
/// # Returns
/// A vector of `Line`s ready for display in ratatui.
pub fn render_table(
    table: &Table,
    border_color: Color,
    header_style: Style,
    cell_style: Style,
    max_width: u16,
) -> Vec<Line<'static>> {
    // Handle empty table
    if table.is_empty() {
        return Vec::new();
    }

    // Clone and calculate widths if not already done
    let mut table = table.clone();
    if table.column_widths.iter().all(|&w| w == 0) {
        table.calculate_column_widths(max_width);
    }

    // Handle case where we still have no columns
    if table.num_columns() == 0 {
        return Vec::new();
    }

    let border_style = Style::default().fg(border_color);
    let widths = &table.column_widths;
    let alignments = &table.alignments;

    let mut lines = Vec::new();

    // Top border: +------+------+
    lines.push(render_horizontal_line(
        widths,
        border::TOP_LEFT,
        border::T_DOWN,
        border::TOP_RIGHT,
        border_style,
    ));

    // Header row
    if !table.headers.is_empty() {
        lines.push(render_row(
            &table.headers,
            widths,
            alignments,
            header_style,
            border_style,
        ));

        // Header separator: +------+------+
        lines.push(render_horizontal_line(
            widths,
            border::T_RIGHT,
            border::CROSS,
            border::T_LEFT,
            border_style,
        ));
    }

    // Data rows
    for row in &table.rows {
        lines.push(render_row(
            row,
            widths,
            alignments,
            cell_style,
            border_style,
        ));
    }

    // Bottom border: +------+------+
    lines.push(render_horizontal_line(
        widths,
        border::BOTTOM_LEFT,
        border::T_UP,
        border::BOTTOM_RIGHT,
        border_style,
    ));

    lines
}

/// Renders a horizontal border line.
///
/// # Arguments
/// * `widths` - Column widths (content only, excluding padding)
/// * `left` - Left corner/tee character
/// * `mid` - Middle intersection character
/// * `right` - Right corner/tee character
/// * `border_style` - Style for border characters
fn render_horizontal_line(
    widths: &[usize],
    left: char,
    mid: char,
    right: char,
    border_style: Style,
) -> Line<'static> {
    let mut spans = Vec::new();

    spans.push(Span::styled(left.to_string(), border_style));

    for (i, &width) in widths.iter().enumerate() {
        // Each column segment: padding + content + padding
        let segment_width = width + 2 * CELL_PADDING;
        let segment: String = std::iter::repeat(border::HORIZONTAL)
            .take(segment_width)
            .collect();
        spans.push(Span::styled(segment, border_style));

        if i < widths.len() - 1 {
            spans.push(Span::styled(mid.to_string(), border_style));
        }
    }

    spans.push(Span::styled(right.to_string(), border_style));

    Line::from(spans)
}

/// Renders a single row of cells.
///
/// # Arguments
/// * `cells` - The cells in this row
/// * `widths` - Column widths
/// * `alignments` - Column alignments
/// * `style` - Style for cell content
/// * `border_style` - Style for border characters
fn render_row(
    cells: &[TableCell],
    widths: &[usize],
    alignments: &[Alignment],
    style: Style,
    border_style: Style,
) -> Line<'static> {
    let mut spans = Vec::new();

    // Left border
    spans.push(Span::styled(border::VERTICAL.to_string(), border_style));

    for (i, width) in widths.iter().enumerate() {
        // Get cell content or empty string if missing
        let cell = cells.get(i);
        let content = cell.map(|c| c.content.as_str()).unwrap_or("");
        let alignment = alignments.get(i).copied().unwrap_or_default();

        // Truncate and align
        let truncated = truncate_with_ellipsis(content, *width);
        let aligned = align_text(&truncated, *width, alignment);

        // Left padding
        spans.push(Span::styled(" ".repeat(CELL_PADDING), style));

        // Cell content - use spans if available, otherwise plain text
        if let Some(cell) = cell {
            if cell.spans.len() == 1 && cell.spans[0].content == cell.content {
                // Simple case: just style the aligned text
                spans.push(Span::styled(aligned, style));
            } else {
                // Complex case: we have styled spans, need to handle alignment
                // For now, just use the aligned plain text with the base style
                spans.push(Span::styled(aligned, style));
            }
        } else {
            spans.push(Span::styled(aligned, style));
        }

        // Right padding
        spans.push(Span::styled(" ".repeat(CELL_PADDING), style));

        // Column separator or right border
        spans.push(Span::styled(border::VERTICAL.to_string(), border_style));
    }

    Line::from(spans)
}
