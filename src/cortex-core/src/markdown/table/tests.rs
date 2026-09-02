//! Tests for table rendering functionality.

#[cfg(test)]
mod tests {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};

    use crate::markdown::table::utils::{align_text, longest_word_width, truncate_with_ellipsis};
    use crate::markdown::table::{Alignment, Table, TableBuilder, TableCell, border, render_table};

    #[test]
    fn test_empty_table() {
        let table = Table::default();
        assert!(table.is_empty());

        let lines = render_table(&table, Color::Gray, Style::default(), Style::default(), 80);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_single_cell() {
        let mut builder = TableBuilder::new();
        builder.start_header();
        builder.add_cell("Header".to_string());
        builder.end_header();

        builder.start_row();
        builder.add_cell("Value".to_string());
        builder.end_row();

        let table = builder.build();
        assert!(!table.is_empty());
        assert_eq!(table.num_columns(), 1);

        let lines = render_table(&table, Color::Gray, Style::default(), Style::default(), 80);

        // Should have: top border, header, separator, data row, bottom border = 5 lines
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_multiple_columns_and_alignments() {
        let mut builder = TableBuilder::new();
        builder.start_header();
        builder.add_cell("Left".to_string());
        builder.add_cell("Center".to_string());
        builder.add_cell("Right".to_string());
        builder.end_header();

        builder.start_row();
        builder.add_cell("A".to_string());
        builder.add_cell("B".to_string());
        builder.add_cell("C".to_string());
        builder.end_row();

        builder.set_alignments(vec![Alignment::Left, Alignment::Center, Alignment::Right]);

        let table = builder.build();
        assert_eq!(table.num_columns(), 3);
        assert_eq!(table.alignments[0], Alignment::Left);
        assert_eq!(table.alignments[1], Alignment::Center);
        assert_eq!(table.alignments[2], Alignment::Right);
    }

    #[test]
    fn test_unicode_content() {
        let mut builder = TableBuilder::new();
        builder.start_header();
        builder.add_cell("Language".to_string());
        builder.add_cell("Greeting".to_string());
        builder.end_header();

        builder.start_row();
        builder.add_cell("Japanese".to_string());
        builder.add_cell("こんにちは".to_string());
        builder.end_row();

        builder.start_row();
        builder.add_cell("Chinese".to_string());
        builder.add_cell("你好".to_string());
        builder.end_row();

        builder.start_row();
        builder.add_cell("Emoji".to_string());
        builder.add_cell("Hello! \u{1F44B}".to_string());
        builder.end_row();

        let table = builder.build();
        let lines = render_table(&table, Color::Gray, Style::default(), Style::default(), 80);

        // Should have: top border, header, separator, 3 data rows, bottom border = 7 lines
        assert_eq!(lines.len(), 7);
    }

    #[test]
    fn test_truncation() {
        let result = truncate_with_ellipsis("Hello, World!", 8);
        assert_eq!(result, "Hello...");

        let result = truncate_with_ellipsis("Hi", 10);
        assert_eq!(result, "Hi");

        let result = truncate_with_ellipsis("Hello", 3);
        assert_eq!(result, "...");

        let result = truncate_with_ellipsis("Hello", 2);
        assert_eq!(result, "..");
    }

    #[test]
    fn test_alignment() {
        assert_eq!(align_text("Hi", 6, Alignment::Left), "Hi    ");
        assert_eq!(align_text("Hi", 6, Alignment::Right), "    Hi");
        assert_eq!(align_text("Hi", 6, Alignment::Center), "  Hi  ");
        assert_eq!(align_text("Hi", 7, Alignment::Center), "  Hi   ");
    }

    #[test]
    fn test_cell_width() {
        let cell = TableCell::new("Hello");
        assert_eq!(cell.width(), 5);

        let cell = TableCell::new("こんにちは"); // 5 wide chars = 10 width
        assert_eq!(cell.width(), 10);

        let cell = TableCell::new("");
        assert_eq!(cell.width(), 0);
    }

    #[test]
    fn test_column_width_calculation() {
        let mut builder = TableBuilder::new();
        builder.start_header();
        builder.add_cell("Short".to_string());
        builder.add_cell("A much longer header".to_string());
        builder.end_header();

        builder.start_row();
        builder.add_cell("A".to_string());
        builder.add_cell("B".to_string());
        builder.end_row();

        let mut table = builder.build();
        table.calculate_column_widths(80);

        // First column should be at least 5 (length of "Short")
        assert!(table.column_widths[0] >= 5);
        // Second column should be at least 20 (length of "A much longer header")
        assert!(table.column_widths[1] >= 20);
    }

    #[test]
    fn test_missing_cells_in_rows() {
        let mut builder = TableBuilder::new();
        builder.start_header();
        builder.add_cell("A".to_string());
        builder.add_cell("B".to_string());
        builder.add_cell("C".to_string());
        builder.end_header();

        builder.start_row();
        builder.add_cell("1".to_string());
        // Missing cells B and C
        builder.end_row();

        builder.start_row();
        builder.add_cell("2".to_string());
        builder.add_cell("3".to_string());
        // Missing cell C
        builder.end_row();

        let table = builder.build();
        let lines = render_table(&table, Color::Gray, Style::default(), Style::default(), 80);

        // Should render without panicking
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_narrow_max_width() {
        let mut builder = TableBuilder::new();
        builder.start_header();
        builder.add_cell("Very Long Header Name".to_string());
        builder.end_header();

        builder.start_row();
        builder.add_cell("Some long cell content".to_string());
        builder.end_row();

        let table = builder.build();
        let lines = render_table(
            &table,
            Color::Gray,
            Style::default(),
            Style::default(),
            20, // Very narrow
        );

        // Should still render, content will be truncated
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_builder_workflow() {
        let mut builder = TableBuilder::new();

        // Header phase
        builder.start_header();
        builder.add_cell("Col1".to_string());
        builder.add_cell("Col2".to_string());
        builder.end_header();

        // Multiple rows
        for i in 0..3 {
            builder.start_row();
            builder.add_cell(format!("R{}C1", i));
            builder.add_cell(format!("R{}C2", i));
            builder.end_row();
        }

        builder.set_alignments(vec![Alignment::Left, Alignment::Right]);

        let table = builder.build();
        assert_eq!(table.headers.len(), 2);
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.alignments.len(), 2);
    }

    #[test]
    fn test_longest_word_width() {
        assert_eq!(longest_word_width("hello world"), 5);
        assert_eq!(longest_word_width("supercalifragilistic"), 20);
        assert_eq!(longest_word_width("a b c"), 3); // minimum is 3
        assert_eq!(longest_word_width(""), 3); // minimum is 3
    }

    #[test]
    fn test_border_characters() {
        // Plus-ASCII grid: every corner and junction is `+`, rules are `-`,
        // separators `|` — never Unicode box drawing.
        assert_eq!(border::TOP_LEFT, '+');
        assert_eq!(border::TOP_RIGHT, '+');
        assert_eq!(border::BOTTOM_LEFT, '+');
        assert_eq!(border::BOTTOM_RIGHT, '+');
        assert_eq!(border::HORIZONTAL, '-');
        assert_eq!(border::VERTICAL, '|');
        assert_eq!(border::CROSS, '+');
        assert_eq!(border::T_DOWN, '+');
        assert_eq!(border::T_UP, '+');
        assert_eq!(border::T_RIGHT, '+');
        assert_eq!(border::T_LEFT, '+');
    }

    #[test]
    fn test_table_without_headers() {
        let mut builder = TableBuilder::new();

        // Skip header, just add rows
        builder.start_row();
        builder.add_cell("A".to_string());
        builder.add_cell("B".to_string());
        builder.end_row();

        builder.start_row();
        builder.add_cell("C".to_string());
        builder.add_cell("D".to_string());
        builder.end_row();

        let table = builder.build();
        assert!(table.headers.is_empty());
        assert_eq!(table.rows.len(), 2);

        let lines = render_table(&table, Color::Gray, Style::default(), Style::default(), 80);

        // Should have: top border, 2 data rows, bottom border = 4 lines (no header separator)
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_cell_with_spans() {
        let spans = vec![
            Span::raw("Hello "),
            Span::styled("World", Style::default().fg(Color::Red)),
        ];
        let cell = TableCell::with_spans("Hello World".to_string(), spans.clone());

        assert_eq!(cell.content, "Hello World");
        assert_eq!(cell.spans.len(), 2);
        assert_eq!(cell.width(), 11);
    }

    #[test]
    fn test_various_widths() {
        let mut builder = TableBuilder::new();
        builder.start_header();
        builder.add_cell("A".to_string());
        builder.add_cell("BB".to_string());
        builder.add_cell("CCC".to_string());
        builder.end_header();

        builder.start_row();
        builder.add_cell("1".to_string());
        builder.add_cell("22".to_string());
        builder.add_cell("333".to_string());
        builder.end_row();

        let table = builder.build();

        // Test with different max widths
        for max_width in [20, 40, 60, 80, 100] {
            let lines = render_table(
                &table,
                Color::Gray,
                Style::default(),
                Style::default(),
                max_width,
            );
            assert!(!lines.is_empty(), "Failed at max_width={}", max_width);
        }
    }

    #[test]
    fn test_default_alignment() {
        let alignment = Alignment::default();
        assert_eq!(alignment, Alignment::Left);
    }

    #[test]
    fn test_default_table_cell() {
        let cell = TableCell::default();
        assert_eq!(cell.content, "");
        assert_eq!(cell.width(), 0);
    }

    // ============================================================
    // Plus-ASCII grid — the only table format
    // ============================================================

    /// The glyphs a Unicode box-drawing table would use. None may appear in
    /// rendered table output; `is_box_drawing` widens the ban to the whole
    /// U+2500–U+257F block.
    const BOX_DRAWING: [char; 11] = ['┌', '┐', '└', '┘', '─', '│', '┼', '┬', '┴', '├', '┤'];

    fn is_box_drawing(c: char) -> bool {
        ('\u{2500}'..='\u{257F}').contains(&c)
    }

    fn text_of(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|s| &*s.content).collect())
            .collect()
    }

    fn two_by_two() -> Table {
        let mut builder = TableBuilder::new();
        builder.start_header();
        builder.add_cell("col".to_string());
        builder.add_cell("col".to_string());
        builder.end_header();
        builder.start_row();
        builder.add_cell("a".to_string());
        builder.add_cell("b".to_string());
        builder.end_row();
        builder.build()
    }

    fn headerless() -> Table {
        let mut builder = TableBuilder::new();
        builder.start_row();
        builder.add_cell("A".to_string());
        builder.add_cell("B".to_string());
        builder.end_row();
        builder.start_row();
        builder.add_cell("C".to_string());
        builder.add_cell("D".to_string());
        builder.end_row();
        builder.build()
    }

    fn wide_content() -> Table {
        let mut builder = TableBuilder::new();
        builder.start_header();
        builder.add_cell("Language".to_string());
        builder.add_cell("Greeting".to_string());
        builder.add_cell("A much longer header than the cells".to_string());
        builder.end_header();
        builder.start_row();
        builder.add_cell("Japanese".to_string());
        builder.add_cell("こんにちは".to_string());
        builder.add_cell("x".to_string());
        builder.end_row();
        builder.start_row();
        builder.add_cell("Emoji".to_string());
        builder.add_cell("Hello! \u{1F44B}".to_string());
        // Missing third cell.
        builder.end_row();
        builder.build()
    }

    fn shapes() -> [Table; 3] {
        [two_by_two(), headerless(), wide_content()]
    }

    #[test]
    fn plus_grid_has_the_locked_shape() {
        let lines = render_table(
            &two_by_two(),
            Color::Gray,
            Style::default(),
            Style::default(),
            80,
        );
        assert_eq!(
            text_of(&lines),
            [
                "+-----+-----+",
                "| col | col |",
                "+-----+-----+",
                "| a   | b   |",
                "+-----+-----+",
            ]
        );
    }

    #[test]
    fn every_row_is_framed_by_the_outer_border() {
        // The frameless `Header | Header` / `---+---` layout is gone: each
        // rendered row starts and ends on the grid, at every width.
        for table in shapes() {
            for max_width in [8u16, 12, 20, 40, 80, 200] {
                let lines = render_table(
                    &table,
                    Color::Gray,
                    Style::default(),
                    Style::default(),
                    max_width,
                );
                assert!(!lines.is_empty(), "nothing rendered at {max_width}");
                for row in text_of(&lines) {
                    let first = row.chars().next().unwrap();
                    let last = row.chars().last().unwrap();
                    assert!(
                        matches!(first, '+' | '|') && first == last,
                        "unframed row {row:?} at max_width {max_width}"
                    );
                }
            }
        }
    }

    #[test]
    fn rules_are_plus_and_dash_only_and_frame_the_header() {
        for table in shapes() {
            let lines = render_table(&table, Color::Gray, Style::default(), Style::default(), 80);
            let text = text_of(&lines);
            let rules: Vec<&String> = text.iter().filter(|r| r.starts_with('+')).collect();
            let expected_rules = if table.headers.is_empty() { 2 } else { 3 };
            assert_eq!(rules.len(), expected_rules, "{text:?}");
            for rule in rules {
                assert!(
                    rule.chars().all(|c| matches!(c, '+' | '-')),
                    "rule carries a non-grid glyph: {rule:?}"
                );
                assert!(rule.starts_with("+-") && rule.ends_with("-+"), "{rule:?}");
            }
            // Top and bottom rules bracket the table; a header sits between
            // the first two rules.
            assert!(text.first().unwrap().starts_with('+'));
            assert!(text.last().unwrap().starts_with('+'));
            if !table.headers.is_empty() {
                assert!(
                    text[1].starts_with('|') && text[2].starts_with('+'),
                    "{text:?}"
                );
            }
        }
    }

    #[test]
    fn rendered_grid_never_uses_box_drawing() {
        for table in shapes() {
            for max_width in [12u16, 80] {
                let lines = render_table(
                    &table,
                    Color::Gray,
                    Style::default(),
                    Style::default(),
                    max_width,
                );
                for row in text_of(&lines) {
                    for glyph in BOX_DRAWING {
                        assert!(!row.contains(glyph), "{glyph} in {row:?}");
                    }
                    assert!(
                        !row.chars().any(is_box_drawing),
                        "box-drawing glyph in {row:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn border_constants_are_never_box_drawing() {
        for glyph in [
            border::TOP_LEFT,
            border::TOP_RIGHT,
            border::BOTTOM_LEFT,
            border::BOTTOM_RIGHT,
            border::HORIZONTAL,
            border::VERTICAL,
            border::CROSS,
            border::T_DOWN,
            border::T_UP,
            border::T_RIGHT,
            border::T_LEFT,
        ] {
            assert!(glyph.is_ascii(), "border glyph {glyph:?} is not ASCII");
            assert!(!is_box_drawing(glyph));
        }
    }

    #[test]
    fn header_and_cells_keep_their_styles_inside_the_gray_frame() {
        let header_style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);
        let cell_style = Style::default().fg(Color::White);
        let lines = render_table(&two_by_two(), Color::DarkGray, header_style, cell_style, 80);
        assert_eq!(lines.len(), 5);

        // Rules are border-coloured end to end.
        for rule in [&lines[0], &lines[2], &lines[4]] {
            for span in &rule.spans {
                assert_eq!(span.style.fg, Some(Color::DarkGray), "{span:?}");
            }
        }
        // Header text is bold white, its `|` separators gray.
        for span in &lines[1].spans {
            if span.content.contains("col") {
                assert_eq!(span.style, header_style, "{span:?}");
            } else if span.content == "|" {
                assert_eq!(span.style.fg, Some(Color::DarkGray), "{span:?}");
            }
        }
        // Cell text is plain white, its `|` separators gray.
        for span in &lines[3].spans {
            if span.content.trim() == "a" || span.content.trim() == "b" {
                assert_eq!(span.style, cell_style, "{span:?}");
            } else if span.content == "|" {
                assert_eq!(span.style.fg, Some(Color::DarkGray), "{span:?}");
            }
        }
    }
}
