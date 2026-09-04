//! Trust Screen TUI
//!
//! Security prompt shown before accessing a workspace for the first time.

use std::io::stdout;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Clear;

use crate::ui::text_utils::{first_fitting_line, wrap_or_drop};
use cortex_core::style::{ACCENT, SELECTION_BG, TEXT, TEXT_DIM};

/// Title of the trust prompt.
pub const TRUST_TITLE: &str = "Trust this workspace?";
/// Key hints under the trust options.
pub const TRUST_HINTS: &str = "↑↓ select · ↵ confirm · esc quit";

const TRUST_OPTIONS: [(&str, &str); 2] = [
    (
        "Yes, trust this folder",
        "Cortex may read, edit and run commands here",
    ),
    ("No, exit", "Review the contents before granting access"),
];

// ============================================================================
// Trust Result
// ============================================================================

/// Result of the trust screen interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustResult {
    /// User trusts the workspace.
    Trusted,
    /// User rejected/exited.
    Rejected,
}

// ============================================================================
// Trust Screen
// ============================================================================

/// Security trust verification screen.
pub struct TrustScreen {
    workspace_path: PathBuf,
    selected: usize,
}

impl TrustScreen {
    /// Create a new trust screen for the given workspace.
    pub fn new(workspace_path: PathBuf) -> Self {
        Self {
            workspace_path,
            selected: 0,
        }
    }

    /// Run the trust screen and return the user's decision.
    pub async fn run(&mut self) -> Result<TrustResult> {
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = stdout();
        crossterm::execute!(
            stdout,
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableMouseCapture,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        )?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.hide_cursor()?;

        let result = self.run_loop(&mut terminal).await;

        let _ = crossterm::execute!(
            terminal.backend_mut(),
            crossterm::event::DisableMouseCapture,
            crossterm::terminal::LeaveAlternateScreen,
        );
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = terminal.show_cursor();

        result
    }

    async fn run_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<TrustResult> {
        loop {
            terminal.draw(|f| self.render(f))?;

            if event::poll(Duration::from_millis(100))?
                && let Event::Key(key) = event::read()?
                && let Some(result) = self.handle_key(key)
            {
                return Ok(result);
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<TrustResult> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected < 1 {
                    self.selected += 1;
                }
                None
            }
            KeyCode::Char('1') => {
                self.selected = 0;
                Some(TrustResult::Trusted)
            }
            KeyCode::Char('2') => {
                self.selected = 1;
                Some(TrustResult::Rejected)
            }
            KeyCode::Enter => {
                if self.selected == 0 {
                    Some(TrustResult::Trusted)
                } else {
                    Some(TrustResult::Rejected)
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => Some(TrustResult::Rejected),
            _ => None,
        }
    }

    /// The trust prompt: title and workspace path, the why in dim copy, then
    /// two numbered options — the focused one a violet `>` and label on the
    /// dark gray bar, the other a dim `·` — each with its description under
    /// the title, and the key hints at the bottom.
    fn render(&self, f: &mut ratatui::Frame) {
        let area = f.area();
        f.render_widget(Clear, area);
        render_trust_prompt(
            f.buffer_mut(),
            area,
            &self.workspace_path.display().to_string(),
            self.selected,
        );
    }
}

/// Paint the trust prompt into `buf`.
pub fn render_trust_prompt(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    workspace: &str,
    selected: usize,
) {
    if area.is_empty() {
        return;
    }
    let w = area.width.saturating_sub(1).max(1) as usize;
    let compact = area.height < 14;
    let hints_y = area.bottom().saturating_sub(1);
    let mut y = area.y;

    buf.set_string(
        area.x,
        y,
        first_fitting_line(TRUST_TITLE, w),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    );
    y += 1;
    buf.set_string(
        area.x,
        y,
        truncate_path(workspace, w),
        Style::default().fg(TEXT),
    );
    y += 2;

    if !compact {
        for part in wrap_or_drop(
            "Only continue for your own projects, verified repositories, or code from collaborators. Cortex reads and edits files and runs commands in this directory with your approval.",
            w,
        ) {
            if y + 6 >= hints_y {
                break;
            }
            buf.set_string(area.x, y, &part, Style::default().fg(TEXT_DIM));
            y += 1;
        }
        y += 1;
    }

    for (i, (label, description)) in TRUST_OPTIONS.iter().enumerate() {
        if y + 1 >= hints_y {
            break;
        }
        let is_selected = i == selected;
        let number = i + 1;
        let label = first_fitting_line(label, w.saturating_sub(4));
        let description = first_fitting_line(description, w.saturating_sub(4));
        if is_selected {
            for row in [y, y + 1] {
                for x in area.x..area.right() {
                    if let Some(cell) = buf.cell_mut((x, row)) {
                        cell.set_bg(SELECTION_BG);
                        cell.set_fg(TEXT);
                    }
                }
            }
            let bar = Style::default().bg(SELECTION_BG);
            buf.set_string(area.x, y, "> ", bar.fg(ACCENT));
            buf.set_string(area.x + 2, y, format!("{number} "), bar.fg(TEXT));
            buf.set_string(
                area.x + 4,
                y,
                &label,
                bar.fg(ACCENT).add_modifier(Modifier::BOLD),
            );
            buf.set_string(area.x + 4, y + 1, &description, bar.fg(TEXT_DIM));
        } else {
            buf.set_string(area.x, y, "· ", Style::default().fg(TEXT_DIM));
            buf.set_string(
                area.x + 2,
                y,
                format!("{number} "),
                Style::default().fg(TEXT),
            );
            buf.set_string(area.x + 4, y, &label, Style::default().fg(TEXT));
            buf.set_string(
                area.x + 4,
                y + 1,
                &description,
                Style::default().fg(TEXT_DIM),
            );
        }
        y += if compact { 2 } else { 3 };
    }

    if !compact && y < hints_y {
        buf.set_string(
            area.x,
            hints_y.saturating_sub(1).max(y),
            first_fitting_line("Learn more: cortex.foundation/docs/security", w),
            Style::default().fg(TEXT_DIM),
        );
    }
    buf.set_string(
        area.x,
        hints_y,
        first_fitting_line(TRUST_HINTS, w),
        Style::default().fg(TEXT_DIM),
    );
}

/// Truncate a path string to fit within max_len.
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.chars().count() <= max_len {
        path.to_string()
    } else if max_len > 5 {
        let tail: String = path
            .chars()
            .rev()
            .take(max_len - 3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("...{tail}")
    } else {
        path.chars().take(max_len).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;

    fn buffer_text(buf: &Buffer) -> String {
        let area = buf.area();
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn trust_prompt_is_a_numbered_picker_at_both_sizes() {
        for (w, h) in [(40u16, 12u16), (120u16, 40u16)] {
            let area = Rect::new(0, 0, w, h);
            let mut buf = Buffer::empty(area);
            render_trust_prompt(&mut buf, area, "~/cortex-api", 0);
            let text = buffer_text(&buf);
            for needle in [
                TRUST_TITLE,
                "~/cortex-api",
                "> 1 Yes, trust this folder",
                "· 2 No, exit",
                TRUST_HINTS,
            ] {
                assert!(text.contains(needle), "{w}x{h} missing {needle}:\n{text}");
            }
            let row = (0..h)
                .find(|y| buf[(0, *y)].symbol() == ">")
                .expect("selected");
            assert_eq!(buf[(0, row)].style().fg, Some(ACCENT));
            assert_eq!(buf[(4, row)].style().fg, Some(ACCENT));
            assert_eq!(buf[(4, row)].style().bg, Some(SELECTION_BG));
            assert_eq!(buf[(4, row + 1)].style().fg, Some(TEXT_DIM));
            let other = (0..h)
                .find(|y| buf[(0, *y)].symbol() == "·")
                .expect("other");
            assert_eq!(buf[(4, other)].style().fg, Some(TEXT));
        }
    }

    #[test]
    fn truncate_path_keeps_the_tail() {
        assert_eq!(truncate_path("~/a", 10), "~/a");
        assert_eq!(truncate_path("/very/long/path/to/repo", 12), "...h/to/repo");
    }
}
