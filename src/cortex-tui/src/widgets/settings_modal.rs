//! Categorised Settings modal (F2 / `/settings`) — lock v2.

use cortex_core::style::{
    ACCENT, BAR_HOVER, HAIRLINE, SELECTION_BG, TEXT, TEXT_DIM, TEXT_MUTED, VOID,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;

use crate::ui::text_utils::first_fitting_line;

/// A visible row in the settings list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsRowKind {
    Category,
    Toggle,
    Number,
    Submenu,
}

#[derive(Debug, Clone)]
pub struct SettingsRow {
    pub id: &'static str,
    pub label: &'static str,
    pub category: &'static str,
    pub kind: SettingsRowKind,
}

/// Live values for every settings row.
#[derive(Debug, Clone)]
pub struct SettingsValues {
    pub compact_mode: bool,
    pub alternate_screen: bool,
    pub timestamps: bool,
    pub show_thinking_blocks: bool,
    pub group_tool_calls: bool,
    pub collapsed_edit_blocks: bool,
    pub line_numbers: bool,
    pub word_wrap: bool,
    pub syntax_highlight: bool,
    pub animations: bool,
    pub theme: String,
    pub mouse_capture: bool,
    pub scroll_lines: u16,
    pub invert_scroll: bool,
    pub copy_on_select: bool,
    pub auto_approve: bool,
    pub sandbox_mode: bool,
    pub streaming: bool,
    pub auto_scroll: bool,
    pub sound: bool,
    pub notifications: bool,
    pub thinking_mode: bool,
    pub context_aware: bool,
    pub debug_mode: bool,
    pub co_author: bool,
    pub auto_commit: bool,
    pub sign_commits: bool,
    pub cloud_sync: bool,
    pub auto_save: bool,
    pub session_history: bool,
    pub telemetry: bool,
    pub analytics: bool,
}

impl Default for SettingsValues {
    fn default() -> Self {
        Self {
            compact_mode: false,
            alternate_screen: true,
            timestamps: true,
            show_thinking_blocks: true,
            group_tool_calls: true,
            collapsed_edit_blocks: false,
            line_numbers: true,
            word_wrap: true,
            syntax_highlight: true,
            animations: true,
            theme: "dark".into(),
            mouse_capture: true,
            scroll_lines: 3,
            invert_scroll: false,
            copy_on_select: false,
            auto_approve: false,
            sandbox_mode: true,
            streaming: true,
            auto_scroll: true,
            sound: false,
            notifications: false,
            thinking_mode: true,
            context_aware: true,
            debug_mode: false,
            co_author: true,
            auto_commit: false,
            sign_commits: false,
            cloud_sync: false,
            auto_save: true,
            session_history: true,
            telemetry: false,
            analytics: false,
        }
    }
}

impl SettingsValues {
    fn display(&self, id: &str) -> String {
        match id {
            "compact" => on_off(self.compact_mode),
            "screen_mode" => {
                if self.alternate_screen {
                    "Fullscreen".into()
                } else {
                    "Inline".into()
                }
            }
            "timestamps" => on_off(self.timestamps),
            "show_thinking" => on_off(self.show_thinking_blocks),
            "group_tools" => on_off(self.group_tool_calls),
            "collapse_edits" => on_off(self.collapsed_edit_blocks),
            "line_numbers" => on_off(self.line_numbers),
            "word_wrap" => on_off(self.word_wrap),
            "syntax_highlight" => on_off(self.syntax_highlight),
            "animations" => on_off(self.animations),
            "theme" => theme_display(&self.theme).into(),
            "mouse_capture" => on_off(self.mouse_capture),
            "scroll_lines" => self.scroll_lines.to_string(),
            "invert_scroll" => on_off(self.invert_scroll),
            "copy_on_select" => on_off(self.copy_on_select),
            "auto_approve" => on_off(self.auto_approve),
            "sandbox" => on_off(self.sandbox_mode),
            "streaming" => on_off(self.streaming),
            "auto_scroll" => on_off(self.auto_scroll),
            "sound" => on_off(self.sound),
            "notifications" => on_off(self.notifications),
            "thinking" => on_off(self.thinking_mode),
            "context_aware" => on_off(self.context_aware),
            "debug" => on_off(self.debug_mode),
            "co_author" => on_off(self.co_author),
            "auto_commit" => on_off(self.auto_commit),
            "sign_commits" => on_off(self.sign_commits),
            "cloud_sync" => on_off(self.cloud_sync),
            "auto_save" => on_off(self.auto_save),
            "session_history" => on_off(self.session_history),
            "telemetry" => on_off(self.telemetry),
            "analytics" => on_off(self.analytics),
            _ => String::new(),
        }
    }

    fn toggle(&mut self, id: &str) {
        match id {
            "compact" => self.compact_mode = !self.compact_mode,
            "screen_mode" => self.alternate_screen = !self.alternate_screen,
            "timestamps" => self.timestamps = !self.timestamps,
            "show_thinking" => self.show_thinking_blocks = !self.show_thinking_blocks,
            "group_tools" => self.group_tool_calls = !self.group_tool_calls,
            "collapse_edits" => self.collapsed_edit_blocks = !self.collapsed_edit_blocks,
            "line_numbers" => self.line_numbers = !self.line_numbers,
            "word_wrap" => self.word_wrap = !self.word_wrap,
            "syntax_highlight" => self.syntax_highlight = !self.syntax_highlight,
            "animations" => self.animations = !self.animations,
            "mouse_capture" => self.mouse_capture = !self.mouse_capture,
            "invert_scroll" => self.invert_scroll = !self.invert_scroll,
            "copy_on_select" => self.copy_on_select = !self.copy_on_select,
            "auto_approve" => self.auto_approve = !self.auto_approve,
            "sandbox" => self.sandbox_mode = !self.sandbox_mode,
            "streaming" => self.streaming = !self.streaming,
            "auto_scroll" => self.auto_scroll = !self.auto_scroll,
            "sound" => self.sound = !self.sound,
            "notifications" => self.notifications = !self.notifications,
            "thinking" => self.thinking_mode = !self.thinking_mode,
            "context_aware" => self.context_aware = !self.context_aware,
            "debug" => self.debug_mode = !self.debug_mode,
            "co_author" => self.co_author = !self.co_author,
            "auto_commit" => self.auto_commit = !self.auto_commit,
            "sign_commits" => self.sign_commits = !self.sign_commits,
            "cloud_sync" => self.cloud_sync = !self.cloud_sync,
            "auto_save" => self.auto_save = !self.auto_save,
            "session_history" => self.session_history = !self.session_history,
            "telemetry" => self.telemetry = !self.telemetry,
            "analytics" => self.analytics = !self.analytics,
            _ => {}
        }
    }

    fn reset(&mut self, id: &str) {
        let d = Self::default();
        match id {
            "compact" => self.compact_mode = d.compact_mode,
            "screen_mode" => self.alternate_screen = d.alternate_screen,
            "timestamps" => self.timestamps = d.timestamps,
            "show_thinking" => self.show_thinking_blocks = d.show_thinking_blocks,
            "group_tools" => self.group_tool_calls = d.group_tool_calls,
            "collapse_edits" => self.collapsed_edit_blocks = d.collapsed_edit_blocks,
            "line_numbers" => self.line_numbers = d.line_numbers,
            "word_wrap" => self.word_wrap = d.word_wrap,
            "syntax_highlight" => self.syntax_highlight = d.syntax_highlight,
            "animations" => self.animations = d.animations,
            "theme" => self.theme = d.theme,
            "mouse_capture" => self.mouse_capture = d.mouse_capture,
            "scroll_lines" => self.scroll_lines = d.scroll_lines,
            "invert_scroll" => self.invert_scroll = d.invert_scroll,
            "copy_on_select" => self.copy_on_select = d.copy_on_select,
            "auto_approve" => self.auto_approve = d.auto_approve,
            "sandbox" => self.sandbox_mode = d.sandbox_mode,
            "streaming" => self.streaming = d.streaming,
            "auto_scroll" => self.auto_scroll = d.auto_scroll,
            "sound" => self.sound = d.sound,
            "notifications" => self.notifications = d.notifications,
            "thinking" => self.thinking_mode = d.thinking_mode,
            "context_aware" => self.context_aware = d.context_aware,
            "debug" => self.debug_mode = d.debug_mode,
            "co_author" => self.co_author = d.co_author,
            "auto_commit" => self.auto_commit = d.auto_commit,
            "sign_commits" => self.sign_commits = d.sign_commits,
            "cloud_sync" => self.cloud_sync = d.cloud_sync,
            "auto_save" => self.auto_save = d.auto_save,
            "session_history" => self.session_history = d.session_history,
            "telemetry" => self.telemetry = d.telemetry,
            "analytics" => self.analytics = d.analytics,
            _ => {}
        }
    }
}

fn on_off(v: bool) -> String {
    if v { "on".into() } else { "off".into() }
}

pub fn theme_display(id: &str) -> &'static str {
    match id {
        "light" => "Cortex Day",
        "ocean_dark" | "ocean" => "Ocean Dark",
        "monokai" => "Monokai",
        _ => "Cortex Night",
    }
}

const CATALOG: &[SettingsRow] = &[
    SettingsRow {
        id: "appearance",
        label: "Appearance",
        category: "Appearance",
        kind: SettingsRowKind::Category,
    },
    SettingsRow {
        id: "compact",
        label: "Compact mode",
        category: "Appearance",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "screen_mode",
        label: "Default screen mode",
        category: "Appearance",
        kind: SettingsRowKind::Submenu,
    },
    SettingsRow {
        id: "timestamps",
        label: "Show timestamps",
        category: "Appearance",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "show_thinking",
        label: "Show thinking blocks",
        category: "Appearance",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "group_tools",
        label: "Group tool calls",
        category: "Appearance",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "collapse_edits",
        label: "Collapsed edit blocks",
        category: "Appearance",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "line_numbers",
        label: "Line numbers",
        category: "Appearance",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "word_wrap",
        label: "Word wrap",
        category: "Appearance",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "syntax_highlight",
        label: "Syntax highlight",
        category: "Appearance",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "animations",
        label: "Animations",
        category: "Appearance",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "theme",
        label: "Theme",
        category: "Appearance",
        kind: SettingsRowKind::Submenu,
    },
    SettingsRow {
        id: "mouse",
        label: "Mouse",
        category: "Mouse",
        kind: SettingsRowKind::Category,
    },
    SettingsRow {
        id: "mouse_capture",
        label: "Mouse capture",
        category: "Mouse",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "scroll_lines",
        label: "Scroll lines",
        category: "Mouse",
        kind: SettingsRowKind::Number,
    },
    SettingsRow {
        id: "invert_scroll",
        label: "Invert scroll",
        category: "Mouse",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "copy_on_select",
        label: "Copy on select",
        category: "Mouse",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "behavior",
        label: "Behavior",
        category: "Behavior",
        kind: SettingsRowKind::Category,
    },
    SettingsRow {
        id: "auto_approve",
        label: "Auto approve",
        category: "Behavior",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "sandbox",
        label: "Sandbox mode",
        category: "Behavior",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "streaming",
        label: "Streaming",
        category: "Behavior",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "auto_scroll",
        label: "Auto scroll",
        category: "Behavior",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "sound",
        label: "Sound",
        category: "Behavior",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "notifications",
        label: "Notifications",
        category: "Behavior",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "ai",
        label: "AI",
        category: "AI",
        kind: SettingsRowKind::Category,
    },
    SettingsRow {
        id: "thinking",
        label: "Thinking mode",
        category: "AI",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "context_aware",
        label: "Context aware",
        category: "AI",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "debug",
        label: "Debug mode",
        category: "AI",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "git",
        label: "Git",
        category: "Git",
        kind: SettingsRowKind::Category,
    },
    SettingsRow {
        id: "co_author",
        label: "Co-author",
        category: "Git",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "auto_commit",
        label: "Auto commit",
        category: "Git",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "sign_commits",
        label: "Sign commits",
        category: "Git",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "cloud",
        label: "Cloud",
        category: "Cloud",
        kind: SettingsRowKind::Category,
    },
    SettingsRow {
        id: "cloud_sync",
        label: "Cloud sync",
        category: "Cloud",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "auto_save",
        label: "Auto save",
        category: "Cloud",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "session_history",
        label: "Session history",
        category: "Cloud",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "privacy",
        label: "Privacy",
        category: "Privacy",
        kind: SettingsRowKind::Category,
    },
    SettingsRow {
        id: "telemetry",
        label: "Telemetry",
        category: "Privacy",
        kind: SettingsRowKind::Toggle,
    },
    SettingsRow {
        id: "analytics",
        label: "Analytics",
        category: "Privacy",
        kind: SettingsRowKind::Toggle,
    },
];

const THEME_OPTIONS: &[(&str, &str, &str)] = &[
    (
        "dark",
        "Cortex Night",
        "Default inky chrome · violet on focus only",
    ),
    ("light", "Cortex Day", "Light chrome for bright rooms"),
    ("ocean_dark", "Ocean Dark", "Deep blue and cyan accents"),
    ("monokai", "Monokai", "Classic code-editor colors"),
];

/// Settings modal interaction state.
#[derive(Debug, Clone)]
pub struct SettingsModalState {
    pub values: SettingsValues,
    pub search: String,
    pub search_focused: bool,
    pub selected: usize,
    pub hovered: Option<usize>,
    pub scroll: usize,
    pub theme_open: bool,
    pub theme_selected: usize,
    pub click_zones: Vec<(Rect, usize)>,
}

impl Default for SettingsModalState {
    fn default() -> Self {
        Self {
            values: SettingsValues::default(),
            search: String::new(),
            search_focused: false,
            selected: 1, // Compact mode — first interactive row
            hovered: None,
            scroll: 0,
            theme_open: false,
            theme_selected: 0,
            click_zones: Vec::new(),
        }
    }
}

impl SettingsModalState {
    pub fn visible_rows(&self) -> Vec<&'static SettingsRow> {
        let q = self.search.trim().to_ascii_lowercase();
        if q.is_empty() {
            return CATALOG.iter().collect();
        }
        CATALOG
            .iter()
            .filter(|row| {
                if row.kind == SettingsRowKind::Category {
                    CATALOG.iter().any(|r| {
                        r.category == row.category
                            && r.kind != SettingsRowKind::Category
                            && (r.label.to_ascii_lowercase().contains(&q) || r.id.contains(&q))
                    })
                } else {
                    row.label.to_ascii_lowercase().contains(&q) || row.id.contains(&q)
                }
            })
            .collect()
    }

    fn selectable_indices(&self) -> Vec<usize> {
        self.visible_rows()
            .iter()
            .enumerate()
            .filter(|(_, r)| r.kind != SettingsRowKind::Category)
            .map(|(i, _)| i)
            .collect()
    }

    fn clamp_selected(&mut self) {
        let sel = self.selectable_indices();
        if sel.is_empty() {
            self.selected = 0;
            return;
        }
        if !sel.contains(&self.selected) {
            self.selected = sel[0];
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SettingsAction {
        if self.theme_open {
            return self.handle_theme_key(key);
        }
        if self.search_focused {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.search_focused = false;
                    return SettingsAction::Continue;
                }
                KeyCode::Backspace => {
                    self.search.pop();
                    self.clamp_selected();
                    return SettingsAction::Continue;
                }
                KeyCode::Char(c)
                    if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
                {
                    self.search.push(c);
                    self.clamp_selected();
                    return SettingsAction::Continue;
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Esc | KeyCode::F(2) => SettingsAction::Close,
            KeyCode::Char('/') if key.modifiers.is_empty() => {
                self.search_focused = true;
                SettingsAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_sel(-1);
                SettingsAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_sel(1);
                SettingsAction::Continue
            }
            KeyCode::Char('g') if key.modifiers.is_empty() => {
                if let Some(i) = self.selectable_indices().first() {
                    self.selected = *i;
                }
                SettingsAction::Continue
            }
            KeyCode::Char('G') => {
                if let Some(i) = self.selectable_indices().last() {
                    self.selected = *i;
                }
                SettingsAction::Continue
            }
            KeyCode::Char(' ') | KeyCode::Enter => self.activate(),
            KeyCode::Right => self.expand(),
            KeyCode::Char('d') if key.modifiers.is_empty() => {
                if let Some(row) = self.visible_rows().get(self.selected) {
                    let id = row.id.to_string();
                    self.values.reset(&id);
                    SettingsAction::Changed(id)
                } else {
                    SettingsAction::Continue
                }
            }
            _ => SettingsAction::Continue,
        }
    }

    fn handle_theme_key(&mut self, key: KeyEvent) -> SettingsAction {
        match key.code {
            KeyCode::Esc | KeyCode::Left => {
                self.theme_open = false;
                SettingsAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.theme_selected > 0 {
                    self.theme_selected -= 1;
                }
                SettingsAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.theme_selected + 1 < THEME_OPTIONS.len() {
                    self.theme_selected += 1;
                }
                SettingsAction::Continue
            }
            KeyCode::Enter => {
                let id = THEME_OPTIONS[self.theme_selected].0;
                self.values.theme = id.to_string();
                self.theme_open = false;
                SettingsAction::Changed("theme".into())
            }
            _ => SettingsAction::Continue,
        }
    }

    fn move_sel(&mut self, dir: i32) {
        let sel = self.selectable_indices();
        if sel.is_empty() {
            return;
        }
        let pos = sel.iter().position(|&i| i == self.selected).unwrap_or(0);
        let next = if dir < 0 {
            pos.saturating_sub(1)
        } else {
            (pos + 1).min(sel.len() - 1)
        };
        self.selected = sel[next];
    }

    fn activate(&mut self) -> SettingsAction {
        let rows = self.visible_rows();
        let Some(row) = rows.get(self.selected) else {
            return SettingsAction::Continue;
        };
        match row.kind {
            SettingsRowKind::Toggle => {
                let id = row.id.to_string();
                self.values.toggle(&id);
                SettingsAction::Changed(id)
            }
            SettingsRowKind::Submenu if row.id == "theme" => {
                self.theme_open = true;
                self.theme_selected = THEME_OPTIONS
                    .iter()
                    .position(|(id, _, _)| *id == self.values.theme)
                    .unwrap_or(0);
                SettingsAction::Continue
            }
            SettingsRowKind::Submenu if row.id == "screen_mode" => {
                self.values.toggle("screen_mode");
                SettingsAction::Changed("screen_mode".into())
            }
            _ => SettingsAction::Continue,
        }
    }

    fn expand(&mut self) -> SettingsAction {
        self.activate()
    }

    pub fn hover_at(&mut self, y: u16) {
        self.hovered = self
            .click_zones
            .iter()
            .find(|(r, _)| y >= r.y && y < r.bottom())
            .map(|(_, i)| *i);
    }
}

/// Result of a settings key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsAction {
    Continue,
    Close,
    Changed(String),
}

/// Widget that paints the settings modal into `area` (the full viewport).
pub struct SettingsModal<'a> {
    pub state: &'a SettingsModalState,
}

impl<'a> SettingsModal<'a> {
    pub fn new(state: &'a SettingsModalState) -> Self {
        Self { state }
    }

    fn modal_rect(area: Rect) -> Rect {
        if area.width <= 42 || area.height <= 12 {
            return area;
        }
        let w = (area.width.saturating_sub(24)).min(96).max(40);
        let h = area.height.saturating_sub(2).min(38);
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        Rect::new(x, y, w, h)
    }
}

impl Widget for SettingsModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let modal = Self::modal_rect(area);
        if modal.width < 8 || modal.height < 6 {
            return;
        }
        for y in modal.y..modal.bottom() {
            for x in modal.x..modal.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_bg(VOID);
                    cell.set_char(' ');
                    cell.set_fg(TEXT);
                }
            }
        }
        let rule = Style::default().fg(HAIRLINE);
        let w = modal.width;
        // Top
        buf.set_string(modal.x, modal.y, "╭", rule);
        buf.set_string(modal.x + 1, modal.y, "─", rule);
        buf.set_string(
            modal.x + 3,
            modal.y,
            "Settings",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        );
        let close = "[x]";
        let close_x = modal.right().saturating_sub(6);
        for col in (modal.x + 12)..close_x.saturating_sub(1) {
            buf.set_string(col, modal.y, "─", rule);
        }
        buf.set_string(close_x, modal.y, close, Style::default().fg(TEXT_DIM));
        buf.set_string(modal.right() - 1, modal.y, "─╮", rule);
        // fix last corner
        buf.set_string(modal.right() - 1, modal.y, "╮", rule);

        let inner = Rect::new(
            modal.x + 1,
            modal.y + 1,
            w.saturating_sub(2),
            modal.height.saturating_sub(2),
        );
        // Search
        let search_style = if self.state.search_focused {
            Style::default().fg(ACCENT)
        } else {
            Style::default().fg(TEXT_DIM)
        };
        let prompt = if self.state.search.is_empty() {
            "/ to search".to_string()
        } else {
            format!("/ {}", self.state.search)
        };
        buf.set_string(
            inner.x + 1,
            inner.y,
            &first_fitting_line(&prompt, inner.width.saturating_sub(2) as usize),
            search_style,
        );
        buf.set_string(inner.x, inner.y + 1, "─".repeat(inner.width as usize), rule);

        if self.state.theme_open {
            paint_theme_submenu(inner, buf, self.state);
        } else {
            paint_rows(inner, buf, self.state);
        }

        // Bottom corners
        buf.set_string(modal.x, modal.bottom() - 1, "╰", rule);
        for col in (modal.x + 1)..(modal.right() - 1) {
            buf.set_string(col, modal.bottom() - 1, "─", rule);
        }
        buf.set_string(modal.right() - 1, modal.bottom() - 1, "╯", rule);
        for y in (modal.y + 1)..(modal.bottom() - 1) {
            buf.set_string(modal.x, y, "│", rule);
            buf.set_string(modal.right() - 1, y, "│", rule);
        }
    }
}

fn paint_rows(inner: Rect, buf: &mut Buffer, state: &SettingsModalState) {
    let rows = state.visible_rows();
    let selectable_empty = rows.iter().all(|r| r.kind == SettingsRowKind::Category);
    if state.search.trim().is_empty() {
        // keep catalog
    } else if rows.is_empty() || selectable_empty {
        buf.set_string(
            inner.x + 2,
            inner.y + 3,
            &first_fitting_line("No settings match", inner.width.saturating_sub(3) as usize),
            Style::default().fg(TEXT_DIM),
        );
    }

    let footer_h: u16 = if inner.height >= 8 { 3 } else { 1 };
    let list_h = inner.height.saturating_sub(2 + footer_h);
    let start_y = inner.y + 2;
    let mut y = start_y;
    let mut shown = 0usize;
    for (i, row) in rows.iter().enumerate() {
        if shown < state.scroll {
            shown += 1;
            continue;
        }
        if y >= start_y + list_h {
            break;
        }
        if row.kind == SettingsRowKind::Category {
            let head = format!("{} ", row.label);
            buf.set_string(inner.x + 1, y, &head, Style::default().fg(TEXT_DIM));
            let after = inner.x + 1 + head.chars().count() as u16;
            for col in after..inner.right() {
                buf.set_string(col, y, "─", Style::default().fg(HAIRLINE));
            }
            y += 1;
            continue;
        }
        let focused = i == state.selected;
        let hovered = state.hovered == Some(i) && !focused;
        if focused {
            for dx in 0..inner.width {
                if let Some(cell) = buf.cell_mut((inner.x + dx, y)) {
                    cell.set_bg(SELECTION_BG);
                }
            }
        } else if hovered {
            for dx in 0..inner.width {
                if let Some(cell) = buf.cell_mut((inner.x + dx, y)) {
                    cell.set_bg(BAR_HOVER);
                }
            }
        }
        let marker = "▸ ";
        let marker_style = if focused {
            Style::default().fg(ACCENT).bg(SELECTION_BG)
        } else {
            Style::default().fg(TEXT_DIM)
        };
        buf.set_string(inner.x + 1, y, marker, marker_style);
        let label_style = if focused {
            Style::default()
                .fg(TEXT)
                .bg(SELECTION_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        buf.set_string(inner.x + 3, y, row.label, label_style);
        let value = state.values.display(row.id);
        let mut shown_val = value.clone();
        if row.kind == SettingsRowKind::Submenu {
            shown_val = format!("{value}  >");
        }
        let val_on = value == "on";
        let val_style = if focused {
            Style::default()
                .fg(if val_on || row.kind != SettingsRowKind::Toggle {
                    TEXT
                } else {
                    TEXT_DIM
                })
                .bg(SELECTION_BG)
        } else if val_on || row.kind != SettingsRowKind::Toggle {
            Style::default().fg(TEXT)
        } else {
            Style::default().fg(TEXT_DIM)
        };
        let val_len = shown_val.chars().count() as u16;
        let vx = inner.right().saturating_sub(val_len + 1);
        if vx > inner.x + 10 {
            buf.set_string(vx, y, &shown_val, val_style);
        }
        y += 1;
        shown += 1;
    }

    let tip_y = inner.bottom().saturating_sub(footer_h);
    if footer_h >= 3 {
        let tip =
            "Tip · Ask Cortex: \"change theme to Cortex Day\" or \"what does compact mode do?\"";
        buf.set_string(
            inner.x + 2,
            tip_y,
            &first_fitting_line(tip, inner.width.saturating_sub(2) as usize),
            Style::default().fg(TEXT_DIM),
        );
        let legend = "↑/↓/j/k nav | g/G top/btm | Space toggle | Enter toggle | → expand | / search | d reset";
        buf.set_string(
            inner.x + 1,
            tip_y + 1,
            &first_fitting_line(legend, inner.width.saturating_sub(2) as usize),
            Style::default().fg(TEXT_DIM),
        );
        let close = "F2/Esc close";
        let cx = inner.x + (inner.width.saturating_sub(close.len() as u16)) / 2;
        buf.set_string(cx, tip_y + 2, close, Style::default().fg(TEXT_DIM));
    } else {
        buf.set_string(
            inner.x + 1,
            tip_y,
            &first_fitting_line("F2/Esc close", inner.width as usize),
            Style::default().fg(TEXT_DIM),
        );
    }
}

fn paint_theme_submenu(inner: Rect, buf: &mut Buffer, state: &SettingsModalState) {
    buf.set_string(
        inner.x + 1,
        inner.y + 2,
        "Appearance › Theme",
        Style::default().fg(TEXT_DIM),
    );
    for (i, (id, label, desc)) in THEME_OPTIONS.iter().enumerate() {
        let y = inner.y + 4 + i as u16;
        if y >= inner.bottom().saturating_sub(2) {
            break;
        }
        let focused = i == state.theme_selected;
        if focused {
            for dx in 0..inner.width {
                if let Some(cell) = buf.cell_mut((inner.x + dx, y)) {
                    cell.set_bg(SELECTION_BG);
                }
            }
        }
        let current = state.values.theme == *id;
        let mark = if current { "● " } else { "○ " };
        let mark_style = if focused {
            Style::default()
                .fg(if current { TEXT } else { TEXT_DIM })
                .bg(SELECTION_BG)
        } else if current {
            Style::default().fg(TEXT)
        } else {
            Style::default().fg(TEXT_DIM)
        };
        buf.set_string(inner.x + 1, y, mark, mark_style);
        let lab_style = if focused {
            Style::default()
                .fg(ACCENT)
                .bg(SELECTION_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        buf.set_string(inner.x + 3, y, *label, lab_style);
        let desc_style = if focused {
            Style::default().fg(TEXT_DIM).bg(SELECTION_BG)
        } else {
            Style::default().fg(TEXT_DIM)
        };
        buf.set_string(
            inner.x + 3 + label.chars().count() as u16 + 2,
            y,
            *desc,
            desc_style,
        );
    }
    let legend = "Enter select | ← back";
    buf.set_string(
        inner.x + 1,
        inner.bottom().saturating_sub(2),
        legend,
        Style::default().fg(TEXT_MUTED),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_appearance_and_mouse() {
        assert!(CATALOG.iter().any(|r| r.label == "Compact mode"));
        assert!(CATALOG.iter().any(|r| r.label == "Mouse capture"));
        assert!(CATALOG.iter().any(|r| r.label == "Show thinking blocks"));
    }

    #[test]
    fn space_toggles_compact() {
        let mut s = SettingsModalState::default();
        assert!(!s.values.compact_mode);
        let act = s.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert_eq!(act, SettingsAction::Changed("compact".into()));
        assert!(s.values.compact_mode);
    }

    #[test]
    fn theme_display_renames() {
        assert_eq!(theme_display("dark"), "Cortex Night");
        assert_eq!(theme_display("light"), "Cortex Day");
    }

    #[test]
    fn slash_focuses_search() {
        let mut s = SettingsModalState::default();
        s.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert!(s.search_focused);
        s.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(s.search, "s");
        assert!(s.visible_rows().iter().any(|r| r.label.contains("scroll")
            || r.label.contains("Scroll")
            || r.id.contains("scroll")));
    }
}
