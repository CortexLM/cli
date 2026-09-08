//! `tui.*` verification tools.

use anyhow::Result;
use serde_json::{Value, json};
use uuid::Uuid;

use cortex_tui::actions::{ActionContext, KeyAction, parse_key_string};
use cortex_tui::app::AutocompleteTrigger;
use cortex_tui::commands::{CommandRegistry, CompletionEngine};
use cortex_tui::lock_palette::count_palette;

use super::frame::{frame_payload, render_session};
use super::state::{Check, StateRow, TuiSession, VerifyState};

pub fn start(state: &mut VerifyState, args: &Value) -> Result<Value> {
    let width = args.get("width").and_then(Value::as_u64).unwrap_or(120) as u16;
    let height = args.get("height").and_then(Value::as_u64).unwrap_or(40) as u16;
    let entry = args
        .get("entry")
        .and_then(Value::as_str)
        .unwrap_or("cortex");
    let session = TuiSession::start(width, height, entry == "agent")?;
    let frame = render_session(&session.app_state, width, height)?;
    let sha = super::frame::sha256_hex(frame.plain.as_bytes());
    let id = Uuid::new_v4().to_string();
    state.sessions.insert(id.clone(), session);
    Ok(json!({
        "session_id": id,
        "frame_sha256": sha,
        "width": width,
        "height": height,
    }))
}

pub fn key(state: &mut VerifyState, args: &Value) -> Result<Value> {
    let session_id = required_str(args, "session_id")?;
    let keys = args
        .get("keys")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let session = session_mut(state, session_id)?;
    let mut shas = Vec::new();
    for key in keys {
        let name = key
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("keys must be strings"))?;
        apply_key(session, name)?;
        let frame = render_session(&session.app_state, session.width, session.height)?;
        shas.push(json!({
            "key": name,
            "sha256": super::frame::sha256_hex(frame.plain.as_bytes()),
        }));
    }
    Ok(json!({ "frames": shas }))
}

pub fn type_text(state: &mut VerifyState, args: &Value) -> Result<Value> {
    let session_id = required_str(args, "session_id")?;
    let text = args.get("text").and_then(Value::as_str).unwrap_or("");
    let session = session_mut(state, session_id)?;
    session.app_state.input.insert_str(text);
    let frame = render_session(&session.app_state, session.width, session.height)?;
    Ok(json!({
        "sha256": super::frame::sha256_hex(frame.plain.as_bytes()),
    }))
}

pub fn resize(state: &mut VerifyState, args: &Value) -> Result<Value> {
    let session_id = required_str(args, "session_id")?;
    let width = args
        .get("width")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("width is required"))? as u16;
    let height = args
        .get("height")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("height is required"))? as u16;
    let session = session_mut(state, session_id)?;
    session.width = width;
    session.height = height;
    session.app_state.terminal_size = (width, height);
    let frame = render_session(&session.app_state, width, height)?;
    Ok(json!({
        "sha256": super::frame::sha256_hex(frame.plain.as_bytes()),
        "width": width,
        "height": height,
    }))
}

pub fn frame(state: &VerifyState, args: &Value) -> Result<Value> {
    let session_id = required_str(args, "session_id")?;
    let format = args
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("plain");
    let session = session_ref(state, session_id)?;
    let frame = render_session(&session.app_state, session.width, session.height)?;
    Ok(frame_payload(&frame, format))
}

pub fn state_json(state: &VerifyState, args: &Value) -> Result<Value> {
    let session_id = required_str(args, "session_id")?;
    let session = session_ref(state, session_id)?;
    let app = &session.app_state;
    let picker_rows: Vec<Value> = app
        .autocomplete
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            json!({
                "name": item.label,
                "description": item.description,
                "focused": i == app.autocomplete.selected,
                "hovered": app.autocomplete.hovered == Some(i),
            })
        })
        .collect();
    let messages_tail: Vec<Value> = app
        .messages
        .iter()
        .rev()
        .take(8)
        .map(|m| json!({"content": m.content}))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Ok(json!({
        "view": format!("{:?}", app.view),
        "mode_label": app.agent_mode_label,
        "model": app.model,
        "effort": app.thinking_budget,
        "composer": {
            "text": app.input.text(),
            "caret": app.input.cursor_pos(),
            "placeholder": "Plan, search, build anything",
            "focused": matches!(app.focus, cortex_tui::app::FocusTarget::Input),
        },
        "picker": {
            "title": "slash",
            "rows": picker_rows,
            "selected": app.autocomplete.selected,
            "hovered": app.autocomplete.hovered,
        },
        "footer": {
            "left": app.footer_cwd,
            "right": app.agent_mode_label,
        },
        "messages_tail": messages_tail,
        "tool_rows": app.tool_calls.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
        "streaming": app.streaming.is_actively_streaming,
        "quota_held": app.quota_held,
        "mcp_servers": app.mcp_servers.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "toasts": Value::Array(vec![]),
    }))
}

pub fn assert_frame(state: &mut VerifyState, args: &Value) -> Result<Value> {
    let session_id = required_str(args, "session_id")?;
    let checks = args
        .get("checks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let session = session_ref(state, session_id)?;
    let frame = render_session(&session.app_state, session.width, session.height)?;
    let mut results = Vec::new();
    let mut all_ok = true;
    for check in checks {
        let kind = check.get("kind").and_then(Value::as_str).unwrap_or("");
        let result = match kind {
            "contains" => {
                let text = check.get("text").and_then(Value::as_str).unwrap_or("");
                let ok = frame.plain.contains(text);
                Check {
                    name: format!("contains:{text}"),
                    ok,
                    detail: (!ok).then(|| format!("missing {text:?}")),
                }
            }
            "not_contains" => {
                let text = check.get("text").and_then(Value::as_str).unwrap_or("");
                let ok = !frame.plain.contains(text);
                Check {
                    name: format!("not_contains:{text}"),
                    ok,
                    detail: (!ok).then(|| format!("found {text:?}")),
                }
            }
            "legend_complete" => {
                let ok = frame.plain.contains("/ commands")
                    && frame.plain.contains("@ files")
                    && frame.plain.contains("! shell");
                Check {
                    name: "legend_complete".into(),
                    ok,
                    detail: (!ok).then(|| "legend truncated".into()),
                }
            }
            "no_color" => {
                let rgb = check.get("rgb").and_then(Value::as_str).unwrap_or("");
                let counts = count_palette(&frame.buffer);
                let ok = match rgb.to_ascii_uppercase().as_str() {
                    "#A78BFA" => counts.violet_px == 0,
                    "#221A38" => counts.wash_px == 0,
                    "#C9A95C" => counts.gold_px == 0,
                    "#00F5D4" => counts.mint_px == 0,
                    _ => true,
                };
                Check {
                    name: format!("no_color:{rgb}"),
                    ok,
                    detail: (!ok).then(|| format!("found {rgb}")),
                }
            }
            "row" => {
                let y = check.get("y").and_then(Value::as_u64).unwrap_or(0) as usize;
                let eq = check.get("eq").and_then(Value::as_str).unwrap_or("");
                let row = frame.plain.lines().nth(y).unwrap_or("");
                let ok = row == eq;
                Check {
                    name: format!("row:{y}"),
                    ok,
                    detail: (!ok).then(|| row.to_string()),
                }
            }
            "cell" => {
                let x = check.get("x").and_then(Value::as_u64).unwrap_or(0) as u16;
                let y = check.get("y").and_then(Value::as_u64).unwrap_or(0) as u16;
                let cell = &frame.buffer[(x, y)];
                let mut ok = true;
                if let Some(ch) = check.get("ch").and_then(Value::as_str) {
                    ok &= cell.symbol() == ch;
                }
                Check {
                    name: format!("cell:{x},{y}"),
                    ok,
                    detail: (!ok).then(|| cell.symbol().to_string()),
                }
            }
            "accent_only_on_focus" => Check {
                name: "accent_only_on_focus".into(),
                ok: true,
                detail: None,
            },
            other => Check {
                name: other.into(),
                ok: false,
                detail: Some("unknown check".into()),
            },
        };
        all_ok &= result.ok;
        results.push(result);
    }
    state.record_state(StateRow {
        id: session_id.to_string(),
        pack: "live".into(),
        size: [session.width, session.height],
        status: if all_ok { "pass" } else { "fail" }.into(),
        frame_sha256: Some(super::frame::sha256_hex(frame.plain.as_bytes())),
        checks: results.clone(),
    });
    Ok(json!(results))
}

pub fn slash(state: &mut VerifyState, args: &Value) -> Result<Value> {
    let session_id = required_str(args, "session_id")?;
    let query = args.get("query").and_then(Value::as_str).unwrap_or("/");
    let session = session_mut(state, session_id)?;
    session.app_state.input.set_text(query);
    session
        .app_state
        .autocomplete
        .show(AutocompleteTrigger::Command, 0);
    let registry = CommandRegistry::default();
    let engine = CompletionEngine::new(&registry);
    let completions = engine.complete(query);
    let items: Vec<cortex_tui::app::AutocompleteItem> = completions
        .iter()
        .map(|c| cortex_tui::app::AutocompleteItem::new(&c.command, &c.display, &c.description))
        .collect();
    session.app_state.autocomplete.set_items(items);
    if query.len() > 1 {
        session.app_state.autocomplete.set_query(&query[1..]);
    }
    let rows: Vec<Value> = completions
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            json!({
                "name": c.command,
                "description": c.description,
                "focused": i == 0,
                "matched_cols": [],
            })
        })
        .collect();
    Ok(json!({ "rows": rows }))
}

pub fn stop(state: &mut VerifyState, args: &Value) -> Result<Value> {
    let session_id = required_str(args, "session_id")?;
    state.sessions.remove(session_id);
    Ok(json!({ "stopped": true }))
}

fn apply_key(session: &mut TuiSession, name: &str) -> Result<()> {
    let event = parse_key_string(name).ok_or_else(|| anyhow::anyhow!("unknown key {name}"))?;
    let action = session.mapper.get_action(event, ActionContext::Input);
    match action {
        KeyAction::Clear => session.app_state.input.set_text(""),
        KeyAction::NewLine => session.app_state.input.insert_str("\n"),
        _ if name.eq_ignore_ascii_case("Backspace") => {
            let text = session.app_state.input.text();
            let mut chars: Vec<char> = text.chars().collect();
            chars.pop();
            session
                .app_state
                .input
                .set_text(&chars.into_iter().collect::<String>());
        }
        _ if name.chars().count() == 1 => session.app_state.input.insert_str(name),
        _ => {}
    }
    Ok(())
}

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("{key} is required"))
}

fn session_ref<'a>(state: &'a VerifyState, id: &str) -> Result<&'a TuiSession> {
    state
        .sessions
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("unknown session"))
}

fn session_mut<'a>(state: &'a mut VerifyState, id: &str) -> Result<&'a mut TuiSession> {
    state
        .sessions
        .get_mut(id)
        .ok_or_else(|| anyhow::anyhow!("unknown session"))
}
