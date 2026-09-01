//! Login Screen - Full-screen TUI
//!
//! Full-screen login screen using ratatui and alternate screen buffer for reliable
//! rendering across all terminal emulators.

use std::io::stdout;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use tokio::sync::mpsc;

use crate::ui::text_utils::{first_fitting_line, wrap_or_drop};
use cortex_core::style::{ACCENT, ERROR, SELECTION_BG, TEXT, TEXT_DIM};
use cortex_login::{SecureAuthData, save_auth_with_fallback};
use cortex_tui_components::spinner::SpinnerStyle;

// ============================================================================
// Constants
// ============================================================================

const API_BASE_URL: &str = "https://api.cortex.foundation";

/// Highlight token (violet) — success accents only; selection is `SELECTION_BG`.
const HIGHLIGHT: ratatui::style::Color = ACCENT;

// ============================================================================
// Login Screen State
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginState {
    SelectMethod,
    WaitingForAuth,
    Success,
    Failed,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginMethod {
    Browser,
    ApiKey,
}

impl LoginMethod {
    fn all() -> &'static [LoginMethod] {
        &[LoginMethod::Browser, LoginMethod::ApiKey]
    }

    fn label(&self) -> &'static str {
        match self {
            LoginMethod::Browser => "Continue with browser",
            LoginMethod::ApiKey => "Paste an API key",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            LoginMethod::Browser => {
                "Opens cortex.foundation/cli/auth — token never hits the model."
            }
            LoginMethod::ApiKey => "Paste a key from your Cortex account.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginResult {
    LoggedIn,
    ContinueWithApiKey,
    Exit,
    Failed(String),
}

// ============================================================================
// Async Messages
// ============================================================================

#[derive(Debug)]
enum AsyncMessage {
    DeviceCodeReceived {
        user_code: String,
        device_code: String,
        #[allow(dead_code)]
        verification_uri: String,
    },
    DeviceCodeError(String),
    TokenReceived,
    TokenError(String),
}

// ============================================================================
// Login Screen
// ============================================================================

pub struct LoginScreen {
    state: LoginState,
    selected_method: usize,
    frame_count: u64,
    error_message: Option<String>,
    user_code: Option<String>,
    verification_uri: Option<String>,
    cortex_home: PathBuf,
    #[allow(dead_code)]
    message: Option<String>,
    async_rx: Option<mpsc::Receiver<AsyncMessage>>,
    copied_notification: Option<Instant>,
    /// Splash line version (`Cortex CLI v{version}`).
    splash_version: String,
}

impl LoginScreen {
    pub fn new(cortex_home: PathBuf, message: Option<String>) -> Self {
        Self {
            state: LoginState::SelectMethod,
            selected_method: 0,
            frame_count: 0,
            error_message: None,
            user_code: None,
            verification_uri: None,
            cortex_home,
            message,
            async_rx: None,
            copied_notification: None,
            splash_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Override the splash version (lock captures pin `1.0.0`).
    pub fn with_splash_version(mut self, version: impl Into<String>) -> Self {
        self.splash_version = version.into();
        self
    }

    /// Select-method screen with an optional product error under the radios.
    pub fn lock_select(version: &str, error: Option<&str>) -> Self {
        let mut screen =
            Self::new(PathBuf::from("/tmp/cortex-lock"), None).with_splash_version(version);
        screen.state = LoginState::SelectMethod;
        screen.error_message = error.map(str::to_string);
        screen
    }

    /// Waiting-for-browser (loading) screen.
    pub fn lock_waiting(version: &str, user_code: &str, verification_uri: &str) -> Self {
        let mut screen =
            Self::new(PathBuf::from("/tmp/cortex-lock"), None).with_splash_version(version);
        screen.state = LoginState::WaitingForAuth;
        screen.user_code = Some(user_code.into());
        screen.verification_uri = Some(verification_uri.into());
        screen
    }

    /// Success screen (`Signed in.` violet).
    pub fn lock_success(version: &str) -> Self {
        let mut screen =
            Self::new(PathBuf::from("/tmp/cortex-lock"), None).with_splash_version(version);
        screen.state = LoginState::Success;
        screen
    }

    /// Failed screen (product-facing error, no accent).
    pub fn lock_failed(version: &str, error: &str) -> Self {
        let mut screen =
            Self::new(PathBuf::from("/tmp/cortex-lock"), None).with_splash_version(version);
        screen.state = LoginState::Failed;
        screen.error_message = Some(error.into());
        screen
    }

    pub async fn run(&mut self) -> Result<LoginResult> {
        // Enter alternate screen mode for reliable rendering
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = stdout();
        crossterm::execute!(
            stdout,
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableMouseCapture,
        )?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let result = self.run_loop(&mut terminal).await;

        // Cleanup - leave alternate screen
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture,
        )?;
        terminal.show_cursor()?;

        result
    }

    async fn run_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<LoginResult> {
        // Create an async event stream - this is crucial for non-blocking event handling
        // that allows the tokio runtime to process async messages concurrently
        let mut event_stream = EventStream::new();

        // Small delay to let terminal settle
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Timer for UI updates (60fps refresh rate)
        let mut render_interval = tokio::time::interval(Duration::from_millis(16));

        loop {
            self.frame_count = self.frame_count.wrapping_add(1);

            // Clear copied notification after 2 seconds
            if let Some(notif_time) = self.copied_notification
                && notif_time.elapsed() > Duration::from_secs(2)
            {
                self.copied_notification = None;
            }

            // Render
            terminal.draw(|f| self.render(f))?;

            // Check state before waiting for events
            match self.state {
                LoginState::Success => {
                    return Ok(LoginResult::LoggedIn);
                }
                LoginState::Exit => {
                    return Ok(LoginResult::Exit);
                }
                LoginState::Failed => {
                    let msg = self.error_message.clone().unwrap_or_default();
                    return Ok(LoginResult::Failed(msg));
                }
                _ => {}
            }

            // Use tokio::select! to concurrently wait for:
            // 1. Terminal events (keyboard input)
            // 2. Async messages from background tasks (token polling)
            // 3. Render timer tick
            // This prevents blocking the async runtime and ensures responsive UI
            tokio::select! {
                // Handle keyboard/terminal events
                maybe_event = event_stream.next() => {
                    if let Some(Ok(Event::Key(key))) = maybe_event
                        && key.kind == crossterm::event::KeyEventKind::Press
                        && let Some(result) = self.handle_key(key)
                    {
                        return Ok(result);
                    }
                }

                // Handle async messages from token polling
                msg = async {
                    if let Some(ref mut rx) = self.async_rx {
                        rx.recv().await
                    } else {
                        // No receiver, wait forever (will be cancelled by other branches)
                        std::future::pending::<Option<AsyncMessage>>().await
                    }
                } => {
                    if let Some(msg) = msg {
                        self.handle_async_message(msg);
                    }
                }

                // Periodic render tick to keep UI responsive
                _ = render_interval.tick() => {
                    // Just continue to re-render
                }
            }
        }
    }

    pub fn render(&self, f: &mut ratatui::Frame) {
        let area = f.area();
        // No background wash: cells stay on `Color::Reset`, so the host
        // terminal (black by default) shows through.
        f.render_widget(Clear, area);

        match self.state {
            LoginState::SelectMethod => self.render_select_method(f, area),
            LoginState::WaitingForAuth => self.render_waiting(f, area),
            LoginState::Success => self.render_success(f, area),
            LoginState::Failed => self.render_failed(f, area),
            LoginState::Exit => {}
        }
    }

    fn render_select_method(&self, f: &mut ratatui::Frame, area: Rect) {
        let version = self.splash_version.as_str();
        let methods = LoginMethod::all();
        let buf = f.buffer_mut();
        let w = area.width.saturating_sub(1).max(1) as usize;
        let mut y = area.y;

        buf.set_string(
            area.x,
            y,
            first_fitting_line(&format!("Cortex CLI v{version}"), w),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        );
        y += 2;
        buf.set_string(
            area.x,
            y,
            first_fitting_line("Sign in to Cortex", w),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        );
        y += 2;

        for (i, method) in methods.iter().enumerate() {
            if y >= area.bottom().saturating_sub(3) {
                break;
            }
            let is_selected = i == self.selected_method;
            let radio = if is_selected { "●" } else { "○" };
            let label = first_fitting_line(method.label(), w.saturating_sub(2));
            if is_selected {
                for x in area.x..area.right() {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_bg(SELECTION_BG);
                        cell.set_fg(TEXT);
                    }
                }
                buf.set_string(
                    area.x,
                    y,
                    first_fitting_line(&format!("{radio} {label}"), w),
                    Style::default()
                        .fg(TEXT)
                        .bg(SELECTION_BG)
                        .add_modifier(Modifier::BOLD),
                );
            } else {
                buf.set_string(
                    area.x,
                    y,
                    first_fitting_line(&format!("{radio} {label}"), w),
                    Style::default().fg(TEXT),
                );
            }
            y += 1;
            if is_selected {
                for hint_line in wrap_or_drop(method.description(), w) {
                    if y >= area.bottom().saturating_sub(3) {
                        break;
                    }
                    buf.set_string(area.x, y, &hint_line, Style::default().fg(TEXT_DIM));
                    y += 1;
                }
            }
        }

        if let Some(ref error) = self.error_message {
            y += 1;
            for err_line in wrap_or_drop(error, w) {
                if y >= area.bottom().saturating_sub(2) {
                    break;
                }
                buf.set_string(area.x, y, &err_line, Style::default().fg(ERROR));
                y += 1;
            }
        }

        buf.set_string(
            area.x,
            area.bottom().saturating_sub(2),
            first_fitting_line("↑↓ select · ↵ continue · esc quit", w),
            Style::default().fg(TEXT_DIM),
        );
    }

    fn render_waiting(&self, f: &mut ratatui::Frame, area: Rect) {
        let version = self.splash_version.as_str();
        let breathing = SpinnerStyle::Breathing.frames();
        let spinner = breathing[(self.frame_count % breathing.len() as u64) as usize];

        let direct_url = self
            .verification_uri
            .clone()
            .unwrap_or_else(|| "https://api.cortex.foundation".to_string());

        let content_width = 70.min(area.width.saturating_sub(4)).max(20);
        let content_height = 10;
        let content_x = (area.width.saturating_sub(content_width)) / 2;
        let content_y = (area.height.saturating_sub(content_height)) / 2;
        let content_area = Rect::new(content_x, content_y, content_width, content_height);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(content_area);

        let splash = Paragraph::new(format!("Cortex CLI v{version}"))
            .style(Style::default().fg(TEXT).add_modifier(Modifier::BOLD));
        f.render_widget(splash, chunks[0]);

        let waiting_text = if area.width < 50 {
            format!("{spinner} Waiting for browser")
        } else {
            format!("{spinner} Waiting for browser authentication")
        };
        let waiting = Paragraph::new(first_fitting_line(&waiting_text, content_width as usize))
            .style(Style::default().fg(TEXT));
        f.render_widget(waiting, chunks[2]);

        if let Some(code) = &self.user_code {
            let code_line =
                Paragraph::new(format!("Code: {code}")).style(Style::default().fg(TEXT));
            f.render_widget(code_line, chunks[3]);
        }

        let copy_hint = if self.copied_notification.is_some() {
            "(copied)"
        } else {
            "(c to copy)"
        };
        let browser_msg = if area.width < 50 {
            first_fitting_line(copy_hint, content_width as usize)
        } else {
            first_fitting_line(
                &format!("Browser didn't open? Open the URL below {copy_hint}"),
                content_width as usize,
            )
        };
        f.render_widget(
            Paragraph::new(browser_msg).style(Style::default().fg(TEXT_DIM)),
            chunks[4],
        );

        let url_line = Paragraph::new(first_fitting_line(&direct_url, content_width as usize))
            .style(Style::default().fg(TEXT));
        f.render_widget(url_line, chunks[5]);

        let hints =
            Paragraph::new("Esc to go back · Ctrl+C to exit").style(Style::default().fg(TEXT_DIM));
        f.render_widget(hints, chunks[7]);
    }

    fn render_success(&self, f: &mut ratatui::Frame, area: Rect) {
        let msg = Paragraph::new("Signed in.")
            .style(Style::default().fg(HIGHLIGHT).add_modifier(Modifier::BOLD));
        f.render_widget(msg, centered_line(area, 20, 1));
    }

    fn render_failed(&self, f: &mut ratatui::Frame, area: Rect) {
        let msg = self
            .error_message
            .clone()
            .unwrap_or_else(|| "Sign-in failed.".to_string());
        let width = area.width.saturating_sub(2).max(1) as usize;
        let mut lines: Vec<Line> = wrap_or_drop(&msg, width)
            .into_iter()
            .map(|line| Line::from(Span::styled(line, Style::default().fg(ERROR))))
            .collect();
        if let Some(hint) = wrap_or_drop("↑↓ select · esc close", width)
            .into_iter()
            .next()
        {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(TEXT_DIM),
            )));
        }
        f.render_widget(
            Paragraph::new(lines.clone()),
            centered_line(
                area,
                area.width.saturating_sub(2).max(1),
                (lines.len() as u16).min(area.height).max(1),
            ),
        );
    }
}

fn centered_line(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    )
}

impl LoginScreen {
    fn get_direct_url(&self) -> String {
        self.verification_uri
            .clone()
            .unwrap_or_else(|| "https://api.cortex.foundation".to_string())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<LoginResult> {
        // Ctrl+C quits from anywhere
        if key.code == KeyCode::Char('c')
            && key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
        {
            return Some(LoginResult::Exit);
        }

        match self.state {
            LoginState::SelectMethod => self.handle_select_method_key(key),
            LoginState::WaitingForAuth => self.handle_waiting_key(key),
            _ => None,
        }
    }

    fn handle_select_method_key(&mut self, key: KeyEvent) -> Option<LoginResult> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_method > 0 {
                    self.selected_method -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_method < LoginMethod::all().len() - 1 {
                    self.selected_method += 1;
                }
            }
            KeyCode::Enter => {
                return self.select_method();
            }
            KeyCode::Char('1') => {
                self.selected_method = 0;
                return self.select_method();
            }
            KeyCode::Char('2') => {
                self.selected_method = 1;
                return self.select_method();
            }
            KeyCode::Char('3') | KeyCode::Char('4') => {}
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                return Some(LoginResult::Exit);
            }
            _ => {}
        }
        None
    }

    fn select_method(&mut self) -> Option<LoginResult> {
        match LoginMethod::all()[self.selected_method] {
            LoginMethod::Browser => {
                self.start_device_code_flow();
                None
            }
            LoginMethod::ApiKey => Some(LoginResult::ContinueWithApiKey),
        }
    }

    fn handle_waiting_key(&mut self, key: KeyEvent) -> Option<LoginResult> {
        match key.code {
            KeyCode::Esc => {
                // Cancel the current auth flow and go back to method selection
                self.state = LoginState::SelectMethod;
                self.error_message = None;
                self.user_code = None;
                self.verification_uri = None;
                // Drop the receiver to signal the async task it can stop
                // (the task will get a send error and terminate)
                self.async_rx = None;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                // Only handle 'c' for copy if NOT Ctrl+C (Ctrl+C is handled in handle_key)
                // Copy URL to clipboard using the safe clipboard function
                // This properly handles Linux (with wait()) and Windows clipboard behavior
                let url = self.get_direct_url();
                if super::terminal::safe_clipboard_copy(&url) {
                    self.copied_notification = Some(Instant::now());
                }
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                // Also allow 'q' to exit from waiting screen for better UX
                return Some(LoginResult::Exit);
            }
            _ => {}
        }
        None
    }

    fn start_device_code_flow(&mut self) {
        self.state = LoginState::WaitingForAuth;
        self.error_message = None;
        self.user_code = None;
        self.verification_uri = None;

        let tx = self.create_async_channel();
        let cortex_home = self.cortex_home.clone();
        tokio::spawn(async move {
            request_device_code_async(cortex_home, tx).await;
        });
    }

    #[allow(dead_code)]
    fn start_guest_session(&mut self) {
        self.state = LoginState::WaitingForAuth;
        self.error_message = None;
        self.user_code = None;
        self.verification_uri = None;
        let tx = self.create_async_channel();
        let cortex_home = self.cortex_home.clone();
        tokio::spawn(async move {
            let client = match cortex_engine::create_default_client() {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(AsyncMessage::DeviceCodeError(e.to_string())).await;
                    return;
                }
            };
            match begin_guest_session(&client, &cortex_home).await {
                Ok(()) => {
                    let _ = tx.send(AsyncMessage::TokenReceived).await;
                }
                Err(e) => {
                    let _ = tx
                        .send(AsyncMessage::DeviceCodeError(format!(
                            "Guest session failed: {e}"
                        )))
                        .await;
                }
            }
        });
    }

    fn create_async_channel(&mut self) -> mpsc::Sender<AsyncMessage> {
        let (tx, rx) = mpsc::channel(16);
        self.async_rx = Some(rx);
        tx
    }

    fn handle_async_message(&mut self, msg: AsyncMessage) {
        match msg {
            AsyncMessage::DeviceCodeReceived {
                user_code,
                device_code,
                verification_uri,
            } => {
                tracing::info!("Device code received: {}", user_code);
                self.user_code = Some(user_code.clone());
                self.verification_uri = Some(verification_uri.clone());

                let link_url = verification_uri;
                tracing::debug!("Opening browser to: {}", link_url);
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open")
                        .arg(&link_url)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn();
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(&link_url)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn();
                }
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("cmd")
                        .args(["/C", "start", "", &link_url])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn();
                }

                // Start token polling - create new channel for this phase
                tracing::debug!("Starting token polling for device code");
                let cortex_home = self.cortex_home.clone();
                let tx = self.create_async_channel();
                tokio::spawn(async move {
                    poll_for_token_async(cortex_home, device_code, tx).await;
                });
            }
            AsyncMessage::DeviceCodeError(e) => {
                tracing::error!("Device code error: {}", e);
                self.state = LoginState::SelectMethod;
                self.error_message = Some(e);
            }
            AsyncMessage::TokenReceived => {
                tracing::info!("Authentication token received - login successful");
                self.state = LoginState::Success;
            }
            AsyncMessage::TokenError(e) => {
                tracing::error!("Token error: {}", e);
                self.state = LoginState::SelectMethod;
                self.error_message = Some(e);
            }
        }
    }
}

// ============================================================================
// Async Functions
// ============================================================================

async fn request_device_code_async(cortex_home: PathBuf, tx: mpsc::Sender<AsyncMessage>) {
    let _ = cortex_home;
    let client = match cortex_engine::create_default_client() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(AsyncMessage::DeviceCodeError(e.to_string())).await;
            return;
        }
    };

    let api_base = cortex_login::resolve_device_api_base(API_BASE_URL);
    let device_name = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "Cortex CLI".to_string());
    let scopes = vec!["openid".to_string(), "profile".to_string()];

    match cortex_login::request_device_authorization(
        &client,
        &api_base,
        Some(&device_name),
        &scopes,
    )
    .await
    {
        Ok(data) => {
            let verification_uri = data.open_url().to_string();
            let _ = tx
                .send(AsyncMessage::DeviceCodeReceived {
                    user_code: data.user_code,
                    device_code: data.device_code,
                    verification_uri,
                })
                .await;
        }
        Err(e) => {
            let _ = tx.send(AsyncMessage::DeviceCodeError(e.to_string())).await;
        }
    }
}

async fn poll_for_token_async(
    cortex_home: PathBuf,
    device_code: String,
    tx: mpsc::Sender<AsyncMessage>,
) {
    tracing::debug!("Token polling started");

    let client = match cortex_engine::create_default_client() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to create HTTP client: {}", e);
            let _ = tx.send(AsyncMessage::TokenError(e.to_string())).await;
            return;
        }
    };

    let api_base = cortex_login::resolve_device_api_base(API_BASE_URL);
    let mut interval = Duration::from_secs(5);
    let max_attempts = 180;

    for attempt in 0..max_attempts {
        tokio::time::sleep(interval).await;
        if tx.is_closed() {
            tracing::debug!("Token polling cancelled (receiver dropped)");
            return;
        }
        tracing::trace!(
            "Polling for token (attempt {}/{})",
            attempt + 1,
            max_attempts
        );

        match cortex_login::poll_device_token(&client, &api_base, &device_code).await {
            Ok(cortex_login::DeviceTokenStatus::Pending { interval: next }) => {
                interval = Duration::from_secs(next.max(1));
            }
            Ok(cortex_login::DeviceTokenStatus::Success(token)) => {
                let Some(access) = token.bearer().map(str::to_string) else {
                    continue;
                };
                let expires_at = token
                    .expires_in
                    .map(|secs| chrono::Utc::now().timestamp() + secs as i64)
                    .or(Some(chrono::Utc::now().timestamp() + 3600));
                let auth_data = SecureAuthData::with_oauth(access, token.refresh_token, expires_at);
                match save_auth_with_fallback(&cortex_home, &auth_data) {
                    Ok(mode) => {
                        tracing::info!("Auth credentials saved using {:?} storage", mode);
                        let _ = tx.send(AsyncMessage::TokenReceived).await;
                        return;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(AsyncMessage::TokenError(format!(
                                "Failed to save credentials: {e}"
                            )))
                            .await;
                        return;
                    }
                }
            }
            Ok(cortex_login::DeviceTokenStatus::Expired) => {
                let _ = tx
                    .send(AsyncMessage::TokenError("Device code expired".to_string()))
                    .await;
                return;
            }
            Ok(cortex_login::DeviceTokenStatus::Denied) => {
                let _ = tx
                    .send(AsyncMessage::TokenError("Access denied".to_string()))
                    .await;
                return;
            }
            Ok(cortex_login::DeviceTokenStatus::Error(e)) => {
                tracing::debug!("Token poll error: {e}");
            }
            Err(e) => {
                tracing::debug!("Token poll request failed: {e}");
            }
        }
    }

    tracing::warn!("Token polling timed out after {} attempts", max_attempts);
    let _ = tx
        .send(AsyncMessage::TokenError(
            "Authentication timed out".to_string(),
        ))
        .await;
}

/// Guest session via `POST /v1/auth/guest`. Stores `cortex_gt` as `gt:<cookie>`.
async fn begin_guest_session(client: &reqwest::Client, cortex_home: &Path) -> anyhow::Result<()> {
    let resp = client
        .post(format!("{API_BASE_URL}/v1/auth/guest"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({}))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("guest session HTTP {}", resp.status());
    }
    let cookie = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|c| {
            c.split(';')
                .next()
                .and_then(|pair| pair.strip_prefix("cortex_gt="))
                .map(str::to_string)
        })
        .ok_or_else(|| anyhow::anyhow!("guest session missing cortex_gt cookie"))?;

    let token = format!("gt:{cookie}");
    let auth_data = SecureAuthData::with_oauth(token, None, None);
    save_auth_with_fallback(cortex_home, &auth_data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let buf = terminal.backend().buffer();
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
    fn snapshot_auth_select_method() {
        let screen = LoginScreen::new(PathBuf::from("/tmp"), None);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal.draw(|f| screen.render(f)).expect("draw");
        let text = buffer_text(&terminal);
        if let Ok(dir) = std::env::var("CORTEX_DUMP_SNAPSHOTS") {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(std::path::Path::new(&dir).join("auth.txt"), &text);
        }
        assert!(text.contains("Cortex CLI"), "{text}");
        assert!(text.contains("Continue with browser"), "{text}");
        assert!(text.contains("Paste an API key"), "{text}");
        assert!(!text.contains("Guest"), "{text}");
        assert!(!text.contains("Exit"), "{text}");
        assert!(text.contains("Sign in to Cortex"), "{text}");
        assert!(text.contains("●"), "{text}");
        assert!(text.contains("○"), "{text}");
        assert!(
            text.contains("cortex.foundation/cli/auth") || text.contains("foundation"),
            "{text}"
        );
        assert!(
            text.contains("↵ continue") || text.contains("continue"),
            "{text}"
        );
        assert!(!text.contains("▄█▀▀▀▀█▄"), "{text}");
        assert!(!text.to_lowercase().contains("grok"));
    }

    #[test]
    fn snapshot_auth_waiting_and_error() {
        let mut screen = LoginScreen::new(PathBuf::from("/tmp"), None);
        screen.state = LoginState::WaitingForAuth;
        screen.user_code = Some("ABCD-1234".into());
        screen.verification_uri = Some("https://example.invalid/device?user_code=ABCD-1234".into());
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal.draw(|f| screen.render(f)).expect("draw");
        let waiting = buffer_text(&terminal);
        if let Ok(dir) = std::env::var("CORTEX_DUMP_SNAPSHOTS") {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(
                std::path::Path::new(&dir).join("auth_waiting.txt"),
                &waiting,
            );
        }
        assert!(
            waiting.contains("ABCD-1234")
                || waiting.contains("device")
                || waiting.contains("Cortex"),
            "{waiting}"
        );
        assert!(waiting.contains("Waiting"), "{waiting}");
        assert!(!waiting.contains("▄█▀▀▀▀█▄"), "{waiting}");

        screen.state = LoginState::SelectMethod;
        screen.error_message = Some("The coding service is temporarily unavailable".into());
        terminal.draw(|f| screen.render(f)).expect("draw");
        let err = buffer_text(&terminal);
        assert!(err.contains("temporarily unavailable"), "{err}");
        assert!(!err.to_lowercase().contains("grok"));
    }

    #[test]
    fn snapshot_auth_success_and_failed() {
        let mut screen = LoginScreen::new(PathBuf::from("/tmp"), None);
        screen.state = LoginState::Success;
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal.draw(|f| screen.render(f)).expect("draw");
        let ok = buffer_text(&terminal);
        assert!(ok.contains("Signed in."), "{ok}");

        screen.state = LoginState::Failed;
        screen.error_message = Some("The coding service is temporarily unavailable".into());
        terminal.draw(|f| screen.render(f)).expect("draw");
        let fail = buffer_text(&terminal);
        assert!(fail.contains("temporarily unavailable"), "{fail}");
    }

    #[test]
    fn snapshot_auth_narrow_and_wide() {
        let screen = LoginScreen::new(PathBuf::from("/tmp"), None);
        for (w, h) in [(40, 12), (120, 40)] {
            let backend = TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).expect("test backend");
            terminal.draw(|f| screen.render(f)).expect("draw");
            let text = buffer_text(&terminal);
            assert!(text.contains("Cortex CLI"), "{text}");
            assert!(!text.trim().is_empty());
        }
    }
}
