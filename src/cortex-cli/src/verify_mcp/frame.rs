//! Headless frame capture helpers.

use anyhow::Result;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use ratatui::widgets::{Clear, Widget};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use cortex_tui::app::AppState;
use cortex_tui::lock_proof::LockFrame;
use cortex_tui::views::minimal_session::MinimalSessionView;
use cortex_tui_capture::{CaptureConfig, MockTerminal, StyleRendering};

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn capture_config(width: u16, height: u16) -> CaptureConfig {
    CaptureConfig::minimal(width, height)
        .with_style_rendering(StyleRendering::Ansi)
        .trim_whitespace(false)
        .with_cursor(false)
}

pub fn render_session(state: &AppState, width: u16, height: u16) -> Result<LockFrame> {
    let config = capture_config(width, height);
    let mut terminal = MockTerminal::from_config(config).map_err(|err| anyhow::anyhow!("{err}"))?;
    terminal.draw(|frame| {
        let area = frame.area();
        frame.render_widget(Clear, area);
        MinimalSessionView::new(state).render(area, frame.buffer_mut());
    })?;
    let buffer = terminal.backend().buffer().clone();
    let plain = buffer_plain(&buffer);
    let ansi = terminal.backend().snapshot().to_ansi(&Default::default());
    Ok(LockFrame {
        id: "session".into(),
        ansi,
        plain,
        buffer,
    })
}

pub fn buffer_plain(buffer: &Buffer) -> String {
    let area = buffer.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(area.x + x, area.y + y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn color_hex(color: Color) -> Option<String> {
    match color {
        Color::Rgb(r, g, b) => Some(format!("#{r:02X}{g:02X}{b:02X}")),
        Color::Reset => None,
        other => Some(format!("{other:?}")),
    }
}

pub fn cells_json(buffer: &Buffer) -> Value {
    let area = buffer.area;
    let mut rows = Vec::new();
    for y in 0..area.height {
        let mut row = Vec::new();
        for x in 0..area.width {
            let cell = &buffer[(area.x + x, area.y + y)];
            row.push(json!({
                "ch": cell.symbol(),
                "fg": color_hex(cell.fg),
                "bg": color_hex(cell.bg),
                "bold": cell.modifier.contains(Modifier::BOLD),
            }));
        }
        rows.push(row);
    }
    json!(rows)
}

pub fn frame_payload(frame: &LockFrame, format: &str) -> Value {
    let sha = sha256_hex(frame.plain.as_bytes());
    match format {
        "ansi" => json!({
            "plain": frame.plain,
            "ansi": frame.ansi,
            "sha256": sha,
        }),
        "cells" => json!({
            "plain": frame.plain,
            "sha256": sha,
            "cells": cells_json(&frame.buffer),
        }),
        _ => json!({
            "plain": frame.plain,
            "sha256": sha,
        }),
    }
}
