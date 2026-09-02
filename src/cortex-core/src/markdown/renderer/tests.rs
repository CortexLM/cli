//! Tests for the markdown renderer.

#[cfg(test)]
mod tests {
    use pulldown_cmark::HeadingLevel;

    use crate::markdown::renderer::helpers::{get_bullet, hash_string, heading_level_to_u8};
    use crate::markdown::renderer::{IncrementalMarkdownRenderer, MarkdownRenderer};
    use crate::markdown::theme::MarkdownTheme;

    // ============================================================
    // MarkdownRenderer Tests
    // ============================================================

    #[test]
    fn test_markdown_renderer_new() {
        let renderer = MarkdownRenderer::new();
        assert_eq!(renderer.width(), 80);
    }

    #[test]
    fn test_markdown_renderer_with_width() {
        let renderer = MarkdownRenderer::new().with_width(100);
        assert_eq!(renderer.width(), 100);
    }

    #[test]
    fn test_markdown_renderer_with_theme() {
        let theme = MarkdownTheme::default();
        let renderer = MarkdownRenderer::with_theme(theme);
        assert!(renderer.theme().h1.fg.is_some());
    }

    #[test]
    fn test_markdown_renderer_default() {
        let renderer = MarkdownRenderer::default();
        assert_eq!(renderer.width(), 80);
    }

    // ============================================================
    // Simple Paragraph Tests
    // ============================================================

    #[test]
    fn test_simple_paragraph() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("Hello, world!");
        assert!(!lines.is_empty());
        let content: String = lines[0].spans.iter().map(|s| &*s.content).collect();
        assert!(content.contains("Hello, world!"));
    }

    #[test]
    fn test_multiple_paragraphs() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("First paragraph.\n\nSecond paragraph.");
        // Should have at least 3 lines (first, blank, second)
        assert!(lines.len() >= 2);
    }

    // ============================================================
    // Header Tests
    // ============================================================

    #[test]
    fn test_header_h1() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("# Header 1");
        assert!(!lines.is_empty());
        let content: String = lines[0].spans.iter().map(|s| &*s.content).collect();
        assert!(content.contains("Header 1"));
    }

    #[test]
    fn test_header_h2() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("## Header 2");
        assert!(!lines.is_empty());
        let content: String = lines[0].spans.iter().map(|s| &*s.content).collect();
        assert!(content.contains("Header 2"));
    }

    #[test]
    fn test_header_h3() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("### Header 3");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_header_h4() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("#### Header 4");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_header_h5() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("##### Header 5");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_header_h6() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("###### Header 6");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_all_header_levels() {
        let renderer = MarkdownRenderer::new();
        let md = "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6";
        let lines = renderer.render(md);
        // Each header plus blank lines between
        assert!(lines.len() >= 6);
    }

    // ============================================================
    // Text Formatting Tests
    // ============================================================

    #[test]
    fn test_bold() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("This is **bold** text.");
        assert!(!lines.is_empty());
        let content: String = lines[0].spans.iter().map(|s| &*s.content).collect();
        assert!(content.contains("bold"));
    }

    #[test]
    fn test_italic() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("This is *italic* text.");
        assert!(!lines.is_empty());
        let content: String = lines[0].spans.iter().map(|s| &*s.content).collect();
        assert!(content.contains("italic"));
    }

    #[test]
    fn test_strikethrough() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("This is ~~strikethrough~~ text.");
        assert!(!lines.is_empty());
        let content: String = lines[0].spans.iter().map(|s| &*s.content).collect();
        assert!(content.contains("strikethrough"));
    }

    #[test]
    fn test_bold_italic() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("This is ***bold italic*** text.");
        assert!(!lines.is_empty());
        let content: String = lines[0].spans.iter().map(|s| &*s.content).collect();
        assert!(content.contains("bold italic"));
    }

    // ============================================================
    // Inline Code Tests
    // ============================================================

    #[test]
    fn test_inline_code() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("This is `inline code` in text.");
        assert!(!lines.is_empty());
        let content: String = lines[0].spans.iter().map(|s| &*s.content).collect();
        assert!(content.contains("inline code"));
    }

    #[test]
    fn test_multiple_inline_code() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("Use `foo` and `bar` functions.");
        assert!(!lines.is_empty());
        let content: String = lines[0].spans.iter().map(|s| &*s.content).collect();
        assert!(content.contains("foo"));
        assert!(content.contains("bar"));
    }

    // ============================================================
    // Code Block Tests
    // ============================================================

    #[test]
    fn test_code_block_without_language() {
        let renderer = MarkdownRenderer::new();
        let md = "```\nfn main() {}\n```";
        let lines = renderer.render(md);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_code_block_with_language() {
        let renderer = MarkdownRenderer::new();
        let md = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let lines = renderer.render(md);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_code_block_multiple_languages() {
        let renderer = MarkdownRenderer::new();

        let rust_md = "```rust\nlet x = 1;\n```";
        let rust_lines = renderer.render(rust_md);
        assert!(!rust_lines.is_empty());

        let python_md = "```python\nx = 1\n```";
        let python_lines = renderer.render(python_md);
        assert!(!python_lines.is_empty());

        let js_md = "```javascript\nconst x = 1;\n```";
        let js_lines = renderer.render(js_md);
        assert!(!js_lines.is_empty());
    }

    // ============================================================
    // List Tests
    // ============================================================

    #[test]
    fn test_unordered_list() {
        let renderer = MarkdownRenderer::new();
        let md = "- Item 1\n- Item 2\n- Item 3";
        let lines = renderer.render(md);
        assert!(lines.len() >= 3);

        let content: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert!(content.contains("Item 1"));
        assert!(content.contains("Item 2"));
        assert!(content.contains("Item 3"));
    }

    #[test]
    fn test_ordered_list() {
        let renderer = MarkdownRenderer::new();
        let md = "1. First\n2. Second\n3. Third";
        let lines = renderer.render(md);
        assert!(lines.len() >= 3);

        let content: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert!(content.contains("First"));
        assert!(content.contains("Second"));
        assert!(content.contains("Third"));
    }

    #[test]
    fn test_nested_list() {
        let renderer = MarkdownRenderer::new();
        let md = "- Parent\n  - Child 1\n  - Child 2";
        let lines = renderer.render(md);
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_deeply_nested_list() {
        let renderer = MarkdownRenderer::new();
        let md = "- Level 1\n  - Level 2\n    - Level 3\n      - Level 4";
        let lines = renderer.render(md);
        assert!(lines.len() >= 4);
    }

    // ============================================================
    // Task List Tests
    // ============================================================

    #[test]
    fn test_task_list() {
        let renderer = MarkdownRenderer::new();
        let md = "- [x] Completed task\n- [ ] Pending task";
        let lines = renderer.render(md);
        assert!(lines.len() >= 2);

        let content: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        // `- [x]` renders as the green `✓`, `- [ ]` as a dim `○`.
        assert!(content.contains("✓ Completed task"), "{content}");
        assert!(content.contains("○ Pending task"), "{content}");
        assert!(
            !content.contains("[x]") && !content.contains("[ ]"),
            "{content}"
        );
    }

    #[test]
    fn nested_lists_keep_the_parent_text_and_indent_children() {
        let renderer = MarkdownRenderer::new();
        let md = "- Redis client\n  - shared connection\n  - fail open\n- Middleware\n  - sliding window";
        let lines = renderer.render(md);
        let text: Vec<String> = lines
            .iter()
            .map(|l| l.to_string())
            .filter(|l| !l.trim().is_empty())
            .collect();
        // Parent rows keep their text and come first; children indent one
        // level under them with the next bullet glyph.
        assert_eq!(text[0], "• Redis client", "{text:?}");
        assert!(
            text[1].starts_with("  ") && text[1].ends_with("shared connection"),
            "{text:?}"
        );
        assert!(
            text[2].starts_with("  ") && text[2].ends_with("fail open"),
            "{text:?}"
        );
        assert_eq!(text[3], "• Middleware", "{text:?}");
        assert!(
            text[4].starts_with("  ") && text[4].ends_with("sliding window"),
            "{text:?}"
        );
        assert_eq!(text.len(), 5, "no empty bullets: {text:?}");
    }

    #[test]
    fn test_mixed_task_list() {
        let renderer = MarkdownRenderer::new();
        let md = "- [x] Done\n- Regular item\n- [ ] Todo";
        let lines = renderer.render(md);
        assert!(lines.len() >= 3);
    }

    #[test]
    fn cortex_renderer_fences_are_hairlines_with_lang_tag_and_line_numbers() {
        use crate::style::{HAIRLINE, TEXT_DIM};
        use ratatui::style::Modifier;

        let renderer = MarkdownRenderer::cortex(MarkdownTheme::default(), 60);
        let md = "Here:\n\n```ts\nexport async function rateLimit(key: string) {\n  const now = Date.now();\n}\n```\n";
        let lines = renderer.render(md);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        // A hairline carrying the language tag, then numbered code, then a
        // closing hairline — no side borders, no box.
        let top = text
            .iter()
            .position(|l| l.starts_with("─ ts ─"))
            .unwrap_or_else(|| panic!("no tagged hairline: {text:?}"));
        assert_eq!(text[top].chars().count(), 60, "{text:?}");
        assert!(
            text[top + 1].starts_with("1 │ export async function"),
            "{text:?}"
        );
        assert!(text[top + 2].starts_with("2 │   const now"), "{text:?}");
        assert!(text[top + 3].starts_with("3 │ }"), "{text:?}");
        assert!(text[top + 4].chars().all(|c| c == '─'), "{text:?}");
        assert!(
            !text.iter().any(|l| l.contains('┌') || l.contains('┐')),
            "{text:?}"
        );

        // The rule and the gutter are the hairline gray, the tag dim.
        let top_line = &lines[top];
        assert_eq!(top_line.spans[0].style.fg, Some(HAIRLINE));
        assert_eq!(top_line.spans[1].content.as_ref(), " ts ");
        assert_eq!(top_line.spans[1].style.fg, Some(TEXT_DIM));
        assert_eq!(lines[top + 1].spans[0].style.fg, Some(HAIRLINE));

        // Keywords come out bold; nothing in the fence carries a colour other
        // than white / gray.
        let code_line = &lines[top + 1];
        let export = code_line
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "export")
            .unwrap_or_else(|| panic!("no export span: {:?}", code_line.spans));
        assert!(export.style.add_modifier.contains(Modifier::BOLD));
        for span in lines.iter().flat_map(|l| l.spans.iter()) {
            assert!(
                is_chrome_gray(span.style.fg),
                "fence paints a colour: {span:?}"
            );
        }
    }

    /// White or one of the chrome grays (`#6B7280` dim included) — never a
    /// saturated colour.
    fn is_chrome_gray(color: Option<ratatui::style::Color>) -> bool {
        match color {
            Some(ratatui::style::Color::Rgb(r, g, b)) => {
                let hi = r.max(g).max(b);
                let lo = r.min(g).min(b);
                hi - lo <= 25
            }
            _ => true,
        }
    }

    #[test]
    fn cortex_renderer_still_highlights_registered_grammars_in_gray() {
        let renderer = MarkdownRenderer::cortex(MarkdownTheme::default(), 60);
        let lines = renderer.render("```bash\nif [ -f x ]; then echo \"hi\"; fi\n```\n");
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert!(text[0].starts_with("─ bash ─"), "{text:?}");
        assert!(text[1].starts_with("1 │ if"), "{text:?}");
        for span in lines.iter().flat_map(|l| l.spans.iter()) {
            assert!(
                is_chrome_gray(span.style.fg),
                "tree-sitter paints a colour: {span:?}"
            );
        }
    }

    // ============================================================
    // Table Tests
    // ============================================================

    #[test]
    fn test_simple_table() {
        let renderer = MarkdownRenderer::new();
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let lines = renderer.render(md);
        assert!(!lines.is_empty());
        // Markdown tables render as the full plus-ASCII grid: a `+---+` rule
        // on top, under the header and at the bottom, `|` separators, and no
        // Unicode box drawing anywhere.
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert_eq!(text.len(), 5, "{text:?}");
        assert!(
            text[0].starts_with("+-") && text[0].ends_with('+'),
            "{text:?}"
        );
        assert!(
            text[1].starts_with("| A") && text[1].contains("| B"),
            "{text:?}"
        );
        assert!(text[2].contains("-+-"), "{text:?}");
        assert!(
            text[4].starts_with("+-") && text[4].ends_with('+'),
            "{text:?}"
        );
        for glyph in ['┌', '┐', '└', '┘', '─', '│', '┼', '┬', '┴', '├', '┤'] {
            assert!(
                !text.iter().any(|l| l.contains(glyph)),
                "tables never use box drawing: {text:?}"
            );
        }
        // Borders carry the theme's gray, cells the white text.
        let border = lines[0].spans[0].style.fg;
        assert_eq!(border, Some(renderer.theme().table_border));
    }

    #[test]
    fn test_table_with_alignment() {
        let renderer = MarkdownRenderer::new();
        let md = "| Left | Center | Right |\n|:-----|:------:|------:|\n| L | C | R |";
        let lines = renderer.render(md);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_table_multiple_rows() {
        let renderer = MarkdownRenderer::new();
        let md = "| H1 | H2 |\n|---|---|\n| A | B |\n| C | D |\n| E | F |";
        let lines = renderer.render(md);
        // Table should have multiple lines
        assert!(lines.len() >= 5);
    }

    fn is_box_drawing(c: char) -> bool {
        ('\u{2500}'..='\u{257F}').contains(&c)
    }

    /// Every table row starts and ends on the grid — `+` for rules, `|` for
    /// cell rows. This is what bans the frameless `Header | Header` layout.
    fn assert_framed_grid(text: &[String]) {
        for row in text {
            let first = row.chars().next().unwrap_or(' ');
            let last = row.trim_end().chars().last().unwrap_or(' ');
            assert!(
                matches!(first, '+' | '|') && first == last,
                "unframed table row {row:?} in {text:?}"
            );
            assert!(
                !row.chars().any(is_box_drawing),
                "box drawing in table row {row:?}"
            );
        }
    }

    #[test]
    fn streaming_table_cut_mid_row_is_still_the_framed_grid() {
        // The incremental renderer re-renders every partial chunk, so a
        // reply that stops inside a table row goes through the EOF path.
        // It must paint the same `+---+` grid, never a frameless fallback.
        let mut incremental =
            IncrementalMarkdownRenderer::new(MarkdownRenderer::new().with_width(60));
        incremental.append("| Model | Effort |\n|---|---|\n| Mini 1 | Med");
        let partial: Vec<String> = incremental
            .get_lines()
            .iter()
            .map(|l| l.to_string())
            .collect();
        assert_eq!(partial.len(), 5, "{partial:?}");
        assert_framed_grid(&partial);
        assert!(partial[0].starts_with("+-"), "{partial:?}");
        assert!(partial[1].contains("| Model"), "{partial:?}");
        assert!(partial[2].contains("-+-"), "{partial:?}");
        assert!(partial[3].contains("| Mini 1 | Med"), "{partial:?}");

        // Finishing the row fills the cell inside the same frame.
        incremental.append("ium |\n| Max 1 | MAX |\n");
        let done: Vec<String> = incremental
            .get_lines()
            .iter()
            .map(|l| l.to_string())
            .collect();
        assert_eq!(done.len(), 6, "{done:?}");
        assert_framed_grid(&done);
        assert!(done[3].contains("| Mini 1 | Medium |"), "{done:?}");
        assert!(done[4].contains("| Max 1  | MAX    |"), "{done:?}");
        assert!(
            done[5].starts_with("+-") && done[5].ends_with('+'),
            "{done:?}"
        );
    }

    #[test]
    fn table_inside_a_blockquote_keeps_the_grid_behind_the_quote_bar() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("> | A | B |\n> |---|---|\n> | 1 | 2 |");
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert_eq!(text.len(), 5, "{text:?}");
        // Strip the quote prefix; the grid behind it is complete and framed.
        let grid: Vec<String> = text
            .iter()
            .map(|row| {
                let start = row
                    .find(|c| c == '+' || c == '|')
                    .unwrap_or_else(|| panic!("no grid in {row:?}"));
                row[start..].to_string()
            })
            .collect();
        assert_framed_grid(&grid);
        assert!(
            grid[0].starts_with("+-") && grid[4].starts_with("+-"),
            "{grid:?}"
        );
        assert!(
            grid[1].contains("| A") && grid[3].contains("| 1"),
            "{grid:?}"
        );
    }

    // ============================================================
    // Blockquote Tests
    // ============================================================

    #[test]
    fn test_blockquote() {
        let renderer = MarkdownRenderer::new();
        let md = "> This is a quote";
        let lines = renderer.render(md);
        assert!(!lines.is_empty());
        let content: String = lines[0].spans.iter().map(|s| &*s.content).collect();
        assert!(content.contains("This is a quote") || content.contains("│"));
    }

    #[test]
    fn test_nested_blockquote() {
        let renderer = MarkdownRenderer::new();
        let md = "> Level 1\n>> Level 2";
        let lines = renderer.render(md);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_blockquote_with_formatting() {
        let renderer = MarkdownRenderer::new();
        let md = "> This is **bold** in a quote";
        let lines = renderer.render(md);
        assert!(!lines.is_empty());
    }

    // ============================================================
    // Link Tests
    // ============================================================

    #[test]
    fn test_link() {
        let renderer = MarkdownRenderer::new();
        let md = "[Link text](https://example.com)";
        let lines = renderer.render(md);
        assert!(!lines.is_empty());
        let content: String = lines[0].spans.iter().map(|s| &*s.content).collect();
        assert!(content.contains("Link text"));
    }

    #[test]
    fn test_link_with_same_text_and_url() {
        let renderer = MarkdownRenderer::new();
        let md = "[https://example.com](https://example.com)";
        let lines = renderer.render(md);
        assert!(!lines.is_empty());
    }

    // ============================================================
    // Horizontal Rule Tests
    // ============================================================

    #[test]
    fn test_horizontal_rule() {
        let renderer = MarkdownRenderer::new();
        let md = "---";
        let lines = renderer.render(md);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_horizontal_rule_variants() {
        let renderer = MarkdownRenderer::new();

        let lines1 = renderer.render("---");
        assert!(!lines1.is_empty());

        let lines2 = renderer.render("***");
        assert!(!lines2.is_empty());

        let lines3 = renderer.render("___");
        assert!(!lines3.is_empty());
    }

    // ============================================================
    // IncrementalMarkdownRenderer Tests
    // ============================================================

    #[test]
    fn test_incremental_new() {
        let renderer = MarkdownRenderer::new();
        let incremental = IncrementalMarkdownRenderer::new(renderer);
        assert!(incremental.is_dirty());
        assert!(incremental.source().is_empty());
    }

    #[test]
    fn test_incremental_set_source() {
        let renderer = MarkdownRenderer::new();
        let mut incremental = IncrementalMarkdownRenderer::new(renderer);

        incremental.set_source("# Hello");
        assert!(incremental.is_dirty());
        assert_eq!(incremental.source(), "# Hello");

        let _ = incremental.get_lines();
        assert!(!incremental.is_dirty());

        // Setting same source shouldn't mark dirty
        incremental.set_source("# Hello");
        assert!(!incremental.is_dirty());

        // Setting different source should mark dirty
        incremental.set_source("# World");
        assert!(incremental.is_dirty());
    }

    #[test]
    fn test_incremental_append() {
        let renderer = MarkdownRenderer::new();
        let mut incremental = IncrementalMarkdownRenderer::new(renderer);

        incremental.append("Hello ");
        assert_eq!(incremental.source(), "Hello ");

        incremental.append("World");
        assert_eq!(incremental.source(), "Hello World");
    }

    #[test]
    fn test_incremental_get_lines() {
        let renderer = MarkdownRenderer::new();
        let mut incremental = IncrementalMarkdownRenderer::new(renderer);

        incremental.set_source("# Hello World");
        let lines = incremental.get_lines();
        assert!(!lines.is_empty());

        // Should not be dirty after get_lines
        assert!(!incremental.is_dirty());
    }

    #[test]
    fn test_incremental_caching() {
        let renderer = MarkdownRenderer::new();
        let mut incremental = IncrementalMarkdownRenderer::new(renderer);

        incremental.set_source("Test content");
        let lines1 = incremental.get_lines();
        let lines2 = incremental.get_lines();

        // Both calls should return same result
        assert_eq!(lines1.len(), lines2.len());
    }

    #[test]
    fn test_incremental_invalidate() {
        let renderer = MarkdownRenderer::new();
        let mut incremental = IncrementalMarkdownRenderer::new(renderer);

        incremental.set_source("Test");
        let _ = incremental.get_lines();
        assert!(!incremental.is_dirty());

        incremental.invalidate();
        assert!(incremental.is_dirty());
    }

    #[test]
    fn test_incremental_clear() {
        let renderer = MarkdownRenderer::new();
        let mut incremental = IncrementalMarkdownRenderer::new(renderer);

        incremental.set_source("Some content");
        let _ = incremental.get_lines();

        incremental.clear();
        assert!(incremental.source().is_empty());
        assert!(incremental.is_dirty());
    }

    #[test]
    fn test_incremental_set_width() {
        let renderer = MarkdownRenderer::new().with_width(80);
        let mut incremental = IncrementalMarkdownRenderer::new(renderer);

        incremental.set_source("Test");
        let _ = incremental.get_lines();
        assert!(!incremental.is_dirty());

        incremental.set_width(100);
        assert!(incremental.is_dirty());
    }

    #[test]
    fn test_incremental_width_no_change() {
        let renderer = MarkdownRenderer::new().with_width(80);
        let mut incremental = IncrementalMarkdownRenderer::new(renderer);

        incremental.set_source("Test");
        let _ = incremental.get_lines();

        // Same width shouldn't mark dirty
        incremental.set_width(80);
        assert!(!incremental.is_dirty());
    }

    // ============================================================
    // Edge Cases
    // ============================================================

    #[test]
    fn test_empty_input() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("");
        assert!(lines.is_empty());
    }

    #[test]
    fn test_whitespace_only() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("   \n\n   ");
        // May or may not have lines depending on how whitespace is handled
        assert!(lines.is_empty() || lines.iter().all(|l| l.spans.is_empty()));
    }

    #[test]
    fn test_unicode_content() {
        let renderer = MarkdownRenderer::new();
        let md = "# 你好世界\n\nこんにちは **太字**";
        let lines = renderer.render(md);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_emoji() {
        let renderer = MarkdownRenderer::new();
        let md = "Hello 👋 World 🌍";
        let lines = renderer.render(md);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_mixed_content() {
        let renderer = MarkdownRenderer::new();
        let md = r#"# Header

Some **bold** and *italic* text with `code`.

- List item 1
- List item 2

> A quote

```rust
fn main() {}
```

| A | B |
|---|---|
| 1 | 2 |

---

End."#;
        let lines = renderer.render(md);
        assert!(lines.len() > 10);
    }

    // ============================================================
    // Hash Function Test
    // ============================================================

    #[test]
    fn test_hash_string() {
        let h1 = hash_string("hello");
        let h2 = hash_string("hello");
        let h3 = hash_string("world");

        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    // ============================================================
    // Bullet Function Test
    // ============================================================

    #[test]
    fn test_get_bullet() {
        assert_eq!(get_bullet(0), "•");
        assert_eq!(get_bullet(1), "◦");
        assert_eq!(get_bullet(2), "▪");
        assert_eq!(get_bullet(3), "▸");
        assert_eq!(get_bullet(100), "▸"); // Should cap at last
    }

    // ============================================================
    // HeadingLevel Conversion Test
    // ============================================================

    #[test]
    fn test_heading_level_to_u8() {
        assert_eq!(heading_level_to_u8(HeadingLevel::H1), 1);
        assert_eq!(heading_level_to_u8(HeadingLevel::H2), 2);
        assert_eq!(heading_level_to_u8(HeadingLevel::H3), 3);
        assert_eq!(heading_level_to_u8(HeadingLevel::H4), 4);
        assert_eq!(heading_level_to_u8(HeadingLevel::H5), 5);
        assert_eq!(heading_level_to_u8(HeadingLevel::H6), 6);
    }
}
