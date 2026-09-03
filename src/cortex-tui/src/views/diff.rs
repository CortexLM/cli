//! Unified-diff rendering for Edit / Write tiles.
//!
//! A hunk renders like a reviewer's diff: a dim `@@` header, a dim
//! two-column line-number gutter (old │ new), context in white, deletions in
//! the diagnostic red, additions in the diff green. When a deletion is paired
//! with the addition that replaced it, only the tokens that actually changed
//! carry the colour — the rest of both lines stays dim — so a one-token edit
//! reads as a one-token edit. Backgrounds are never tinted.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use similar::{ChangeTag, TextDiff};

use crate::ui::colors::AdaptiveColors;

/// One parsed diff row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffRow {
    /// `@@ -a,b +c,d @@` header (kept verbatim).
    Hunk(String),
    /// Unchanged line, with its old and new numbers.
    Context { old: u32, new: u32, text: String },
    /// Removed line, with its old number.
    Delete { old: u32, text: String },
    /// Added line, with its new number.
    Insert { new: u32, text: String },
}

/// True when `body` looks like a unified diff: a hunk header, or a majority
/// of its lines carrying `+` / `-` markers.
pub fn looks_like_diff(body: &str) -> bool {
    let mut marked = 0usize;
    let mut total = 0usize;
    for line in body.lines() {
        if line.starts_with("@@") {
            return true;
        }
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        total += 1;
        if line.starts_with('+') || line.starts_with('-') {
            marked += 1;
        }
    }
    total > 0 && marked * 2 >= total
}

/// `(additions, deletions)` in a unified diff body.
pub fn count_changes(body: &str) -> (usize, usize) {
    parse_unified_diff(body)
        .iter()
        .fold((0, 0), |(adds, dels), row| match row {
            DiffRow::Insert { .. } => (adds + 1, dels),
            DiffRow::Delete { .. } => (adds, dels + 1),
            _ => (adds, dels),
        })
}

/// Parse unified-diff text into rows, numbering lines from the hunk header
/// (or from 1 when there is none).
pub fn parse_unified_diff(body: &str) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let mut old = 1u32;
    let mut new = 1u32;
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("@@") {
            if let Some((o, n)) = parse_hunk_header(rest) {
                old = o;
                new = n;
            }
            rows.push(DiffRow::Hunk(line.to_string()));
            continue;
        }
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if let Some(text) = line.strip_prefix('+') {
            rows.push(DiffRow::Insert {
                new,
                text: text.to_string(),
            });
            new += 1;
        } else if let Some(text) = line.strip_prefix('-') {
            rows.push(DiffRow::Delete {
                old,
                text: text.to_string(),
            });
            old += 1;
        } else {
            let text = line.strip_prefix(' ').unwrap_or(line);
            rows.push(DiffRow::Context {
                old,
                new,
                text: text.to_string(),
            });
            old += 1;
            new += 1;
        }
    }
    rows
}

/// `-a,b +c,d @@ …` → `(a, c)`.
fn parse_hunk_header(rest: &str) -> Option<(u32, u32)> {
    let mut parts = rest.split_whitespace();
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let first = |range: &str| range.split(',').next()?.parse::<u32>().ok();
    Some((first(old)?, first(new)?))
}

/// Render `body` (a unified diff) as styled lines fitting `width` columns.
pub fn render_unified_diff(body: &str, width: u16, colors: &AdaptiveColors) -> Vec<Line<'static>> {
    render_diff_rows(&parse_unified_diff(body), width, colors)
}

/// Render parsed rows. Gutter: `old new` right-aligned, dim; then the
/// marker; then the text, cut at the width without ever splitting a
/// grapheme.
pub fn render_diff_rows(
    rows: &[DiffRow],
    width: u16,
    colors: &AdaptiveColors,
) -> Vec<Line<'static>> {
    let dim = Style::default().fg(colors.text_dim);
    let text = Style::default().fg(colors.text);
    let add = Style::default().fg(colors.diff_add);
    let del = Style::default().fg(colors.error);

    let max_no = rows
        .iter()
        .map(|row| match row {
            DiffRow::Context { old, new, .. } => (*old).max(*new),
            DiffRow::Delete { old, .. } => *old,
            DiffRow::Insert { new, .. } => *new,
            DiffRow::Hunk(_) => 0,
        })
        .max()
        .unwrap_or(1);
    let digits = max_no.to_string().len().max(2);
    let gutter_w = digits * 2 + 1;
    let budget = (width as usize).saturating_sub(gutter_w + 3);

    let gutter = |old: Option<u32>, new: Option<u32>| -> String {
        let o = old.map(|n| n.to_string()).unwrap_or_default();
        let n = new.map(|n| n.to_string()).unwrap_or_default();
        format!("{o:>digits$} {n:>digits$}")
    };
    let cut = |s: &str| -> String { s.chars().take(budget).collect() };

    let mut lines = Vec::with_capacity(rows.len());
    let mut i = 0;
    while i < rows.len() {
        match &rows[i] {
            DiffRow::Hunk(header) => {
                lines.push(Line::from(Span::styled(
                    cut_to(header, width as usize),
                    dim,
                )));
                i += 1;
            }
            DiffRow::Context { old, new, text: t } => {
                lines.push(Line::from(vec![
                    Span::styled(gutter(Some(*old), Some(*new)), dim),
                    Span::styled("   ", dim),
                    Span::styled(cut(t), text),
                ]));
                i += 1;
            }
            DiffRow::Delete { old, text: removed } => {
                // A deletion immediately followed by one insertion is a
                // changed line: colour only the tokens that differ.
                if let Some(DiffRow::Insert { new, text: added }) = rows.get(i + 1) {
                    let (del_spans, add_spans) = word_diff_spans(removed, added, dim, del, add);
                    lines.push(Line::from(
                        [
                            Span::styled(gutter(Some(*old), None), dim),
                            Span::styled(" - ", del),
                        ]
                        .into_iter()
                        .chain(clip_spans(del_spans, budget))
                        .collect::<Vec<_>>(),
                    ));
                    lines.push(Line::from(
                        [
                            Span::styled(gutter(None, Some(*new)), dim),
                            Span::styled(" + ", add),
                        ]
                        .into_iter()
                        .chain(clip_spans(add_spans, budget))
                        .collect::<Vec<_>>(),
                    ));
                    i += 2;
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(gutter(Some(*old), None), dim),
                        Span::styled(" - ", del),
                        Span::styled(cut(removed), del),
                    ]));
                    i += 1;
                }
            }
            DiffRow::Insert { new, text: added } => {
                lines.push(Line::from(vec![
                    Span::styled(gutter(None, Some(*new)), dim),
                    Span::styled(" + ", add),
                    Span::styled(cut(added), add),
                ]));
                i += 1;
            }
        }
    }
    lines
}

/// Word-level diff of one changed line: unchanged tokens dim, removed tokens
/// red (bold) on the `-` row, inserted tokens green (bold) on the `+` row.
pub fn word_diff_spans(
    removed: &str,
    added: &str,
    dim: Style,
    del: Style,
    add: Style,
) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let diff = TextDiff::from_words(removed, added);
    let mut del_spans = Vec::new();
    let mut add_spans = Vec::new();
    for change in diff.iter_all_changes() {
        let value = change.value().to_string();
        match change.tag() {
            ChangeTag::Equal => {
                del_spans.push(Span::styled(value.clone(), dim));
                add_spans.push(Span::styled(value, dim));
            }
            ChangeTag::Delete => {
                del_spans.push(Span::styled(value, del.add_modifier(Modifier::BOLD)));
            }
            ChangeTag::Insert => {
                add_spans.push(Span::styled(value, add.add_modifier(Modifier::BOLD)));
            }
        }
    }
    (del_spans, add_spans)
}

fn cut_to(s: &str, width: usize) -> String {
    s.chars().take(width).collect()
}

/// Drop spans past `budget` columns, cutting the last one that fits.
fn clip_spans(spans: Vec<Span<'static>>, budget: usize) -> Vec<Span<'static>> {
    let mut used = 0usize;
    let mut out = Vec::new();
    for span in spans {
        let len = span.content.chars().count();
        if used + len <= budget {
            used += len;
            out.push(span);
            continue;
        }
        let keep = budget.saturating_sub(used);
        if keep > 0 {
            let text: String = span.content.chars().take(keep).collect();
            out.push(Span::styled(text, span.style));
        }
        break;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HUNK: &str = "@@ -20,4 +20,5 @@\n import Redis from \"ioredis\";\n-const limit = 30;\n+const limit = 60;\n+const windowSec = 60;\n export function rateLimit() {";

    #[test]
    fn detects_unified_diffs() {
        assert!(looks_like_diff(HUNK));
        assert!(looks_like_diff("-a\n+b"));
        assert!(!looks_like_diff("src/auth.rs"));
        assert!(!looks_like_diff("pub fn sign_in() {}\nlet x = 1;"));
    }

    #[test]
    fn counts_changes() {
        assert_eq!(count_changes(HUNK), (2, 1));
        assert_eq!(count_changes("-a\n+b"), (1, 1));
    }

    #[test]
    fn parses_hunk_numbers() {
        let rows = parse_unified_diff(HUNK);
        assert_eq!(rows[0], DiffRow::Hunk("@@ -20,4 +20,5 @@".into()));
        assert_eq!(
            rows[1],
            DiffRow::Context {
                old: 20,
                new: 20,
                text: "import Redis from \"ioredis\";".into()
            }
        );
        assert_eq!(
            rows[2],
            DiffRow::Delete {
                old: 21,
                text: "const limit = 30;".into()
            }
        );
        assert_eq!(
            rows[3],
            DiffRow::Insert {
                new: 21,
                text: "const limit = 60;".into()
            }
        );
        assert_eq!(
            rows[4],
            DiffRow::Insert {
                new: 22,
                text: "const windowSec = 60;".into()
            }
        );
        assert_eq!(
            rows[5],
            DiffRow::Context {
                old: 22,
                new: 23,
                text: "export function rateLimit() {".into()
            }
        );
    }

    #[test]
    fn renders_gutter_markers_and_colours() {
        let colors = AdaptiveColors::default_dark();
        let lines = render_unified_diff(HUNK, 80, &colors);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert_eq!(text[0], "@@ -20,4 +20,5 @@");
        assert_eq!(text[1], "20 20   import Redis from \"ioredis\";");
        assert_eq!(text[2], "21    - const limit = 30;");
        assert_eq!(text[3], "   21 + const limit = 60;");
        assert_eq!(text[4], "   22 + const windowSec = 60;");
        assert_eq!(text[5], "22 23   export function rateLimit() {");
        // Header and gutter dim, context white, whole-line insert green.
        assert_eq!(lines[0].spans[0].style.fg, Some(colors.text_dim));
        assert_eq!(lines[1].spans[0].style.fg, Some(colors.text_dim));
        assert_eq!(lines[1].spans[2].style.fg, Some(colors.text));
        assert_eq!(lines[4].spans[1].style.fg, Some(colors.diff_add));
        assert_eq!(lines[4].spans[2].style.fg, Some(colors.diff_add));
    }

    #[test]
    fn changed_line_tints_only_the_mutated_token() {
        let colors = AdaptiveColors::default_dark();
        let lines = render_unified_diff("-const limit = 30;\n+const limit = 60;", 80, &colors);
        let fg_of = |line: &Line<'static>, token: &str| {
            line.spans
                .iter()
                .find(|s| s.content.as_ref() == token)
                .unwrap_or_else(|| panic!("no {token:?} in {:?}", line.spans))
                .style
                .fg
        };
        // `const`, `limit`, `=` stay dim on both rows; only `30` is red and
        // only `60` is green.
        assert_eq!(fg_of(&lines[0], "const"), Some(colors.text_dim));
        assert_eq!(fg_of(&lines[0], "30;"), Some(colors.error));
        assert_eq!(fg_of(&lines[1], "const"), Some(colors.text_dim));
        assert_eq!(fg_of(&lines[1], "60;"), Some(colors.diff_add));
        // The markers carry the colour too, and no background is tinted.
        assert_eq!(fg_of(&lines[0], " - "), Some(colors.error));
        assert_eq!(fg_of(&lines[1], " + "), Some(colors.diff_add));
        for span in lines.iter().flat_map(|l| l.spans.iter()) {
            assert_eq!(span.style.bg, None, "{span:?}");
        }
    }

    #[test]
    fn narrow_widths_clip_without_panicking() {
        let colors = AdaptiveColors::default_dark();
        let lines = render_unified_diff(HUNK, 24, &colors);
        for line in &lines {
            assert!(
                line.to_string().chars().count() <= 24,
                "{}",
                line.to_string()
            );
        }
    }
}
